//! Anthropic Messages API provider (raw HTTP + SSE; there is no official Rust SDK).
//!
//! Authentication: an Anthropic API key from the Claude Console, stored in
//! Windows Credential Manager (or the ANTHROPIC_API_KEY env var). A Claude.ai
//! Pro/Max subscription does not grant API access for third-party apps, and this
//! app never reads Claude.ai cookies or Claude Code credentials.

use super::provider::{http_client, EventSink, ModelInfo, Provider, ProviderStatus};
use super::sse::SseParser;
use super::types::*;
use crate::config::AnthropicConfig;
use crate::error::{AppError, AppResult};
use futures_util::StreamExt;
use serde_json::{json, Value};
use tokio_util::sync::CancellationToken;

const API_VERSION: &str = "2023-06-01";
const FALLBACK_BETA: &str = "server-side-fallback-2026-07-01";

pub struct AnthropicProvider {
    cfg: AnthropicConfig,
    client: reqwest::Client,
}

pub const KNOWN_MODELS: &[(&str, &str)] = &[
    ("claude-opus-5-5", "Claude Opus 5.5"),
    ("claude-sonnet-5-5", "Claude Sonnet 5.5"),
    ("claude-haiku-5-5", "Claude Haiku 5.5"),
    ("claude-fable-5-1", "Claude Fable 5.1"),
];

/// Current-generation models accept adaptive thinking + effort.
fn supports_adaptive(model: &str) -> bool {
    ["claude-opus-5", "claude-sonnet-5", "claude-haiku-5", "claude-fable-5", "claude-opus-4-8", "claude-opus-4-7", "claude-opus-4-6", "claude-sonnet-4-6"]
        .iter()
        .any(|p| model.starts_with(p))
}

fn is_first_party(base: &str) -> bool {
    base.trim_end_matches('/') == "https://api.anthropic.com"
}

impl AnthropicProvider {
    pub fn new(cfg: AnthropicConfig) -> Self {
        Self { cfg, client: http_client() }
    }

    fn key(&self) -> AppResult<String> {
        crate::secrets::get_secret("anthropic_api_key").ok_or_else(|| {
            AppError::MissingCredential(
                "No Anthropic API key. Add one in Settings → AI Providers (create it at console.anthropic.com). A Claude.ai subscription alone does not provide API access.".into(),
            )
        })
    }

    fn url(&self, path: &str) -> String {
        format!("{}{}", self.cfg.base_url.trim_end_matches('/'), path)
    }

    pub fn build_body(&self, req: &ChatRequest) -> Value {
        let mut body = json!({
            "model": self.cfg.model,
            "max_tokens": self.cfg.max_tokens,
            "stream": true,
            "system": req.system,
            "messages": req.messages,
        });
        if !req.tools.is_empty() {
            body["tools"] = Value::Array(
                req.tools
                    .iter()
                    .map(|t| json!({"name": t.name, "description": t.description, "input_schema": t.input_schema, "eager_input_streaming": true}))
                    .collect(),
            );
        }
        if supports_adaptive(&self.cfg.model) {
            body["thinking"] = json!({"type": "adaptive", "display": "summarized"});
            body["output_config"] = json!({"effort": self.cfg.effort});
        }
        if self.cfg.refusal_fallbacks && is_first_party(&self.cfg.base_url) {
            body["fallbacks"] = json!("default");
        }
        body
    }

    async fn send_with_retry(&self, body: &Value, key: &str, cancel: &CancellationToken) -> AppResult<reqwest::Response> {
        let mut attempt = 0u32;
        loop {
            let mut rb = self
                .client
                .post(self.url("/v1/messages"))
                .header("x-api-key", key)
                .header("anthropic-version", API_VERSION)
                .header("content-type", "application/json")
                .json(body);
            if body.get("fallbacks").is_some() {
                rb = rb.header("anthropic-beta", FALLBACK_BETA);
            }
            let resp = tokio::select! {
                r = rb.send() => r?,
                _ = cancel.cancelled() => return Err(AppError::Cancelled),
            };
            let status = resp.status();
            if status.is_success() {
                return Ok(resp);
            }
            let retryable = status.as_u16() == 429 || status.as_u16() == 529 || status.is_server_error();
            let retry_after = resp
                .headers()
                .get("retry-after")
                .and_then(|v| v.to_str().ok())
                .and_then(|v| v.parse::<u64>().ok());
            let text = resp.text().await.unwrap_or_default();
            if retryable && attempt < 2 {
                attempt += 1;
                let wait = retry_after.unwrap_or(2u64.pow(attempt)).min(20);
                tokio::select! {
                    _ = tokio::time::sleep(std::time::Duration::from_secs(wait)) => continue,
                    _ = cancel.cancelled() => return Err(AppError::Cancelled),
                }
            }
            return Err(map_http_error(status.as_u16(), &text));
        }
    }
}

pub fn map_http_error(status: u16, body: &str) -> AppError {
    let msg = serde_json::from_str::<Value>(body)
        .ok()
        .and_then(|v| v["error"]["message"].as_str().map(String::from))
        .unwrap_or_else(|| body.chars().take(300).collect());
    match status {
        401 => AppError::MissingCredential(format!("Anthropic rejected the API key: {msg}")),
        403 => AppError::Provider(format!("Access denied (403): {msg}")),
        404 => AppError::Provider(format!("Not found (404) — check the model id: {msg}")),
        429 => AppError::Provider(format!("Rate limited (429): {msg}")),
        529 => AppError::Provider(format!("Anthropic API overloaded (529): {msg}")),
        _ => AppError::Provider(format!("HTTP {status}: {msg}")),
    }
}

/// Accumulates streamed content blocks.
#[derive(Default)]
pub struct StreamAccumulator {
    blocks: Vec<Option<Partial>>,
    pub stop_reason: Option<String>,
    pub usage: Usage,
    pub model: String,
}

enum Partial {
    Text(String),
    Thinking { thinking: String, signature: Option<String> },
    Redacted(String),
    Tool { id: String, name: String, json: String },
}

impl StreamAccumulator {
    /// Applies one SSE event. Returns UI events to forward.
    pub fn apply(&mut self, event: &str, data: &Value) -> AppResult<Vec<StreamEvent>> {
        let mut out = Vec::new();
        match event {
            "message_start" => {
                self.model = data["message"]["model"].as_str().unwrap_or_default().to_string();
                self.usage.input_tokens = data["message"]["usage"]["input_tokens"].as_u64().unwrap_or(0);
            }
            "content_block_start" => {
                let idx = data["index"].as_u64().unwrap_or(0) as usize;
                let b = &data["content_block"];
                let partial = match b["type"].as_str() {
                    Some("text") => Some(Partial::Text(b["text"].as_str().unwrap_or_default().into())),
                    Some("thinking") => Some(Partial::Thinking {
                        thinking: b["thinking"].as_str().unwrap_or_default().into(),
                        signature: b["signature"].as_str().filter(|s| !s.is_empty()).map(String::from),
                    }),
                    Some("redacted_thinking") => Some(Partial::Redacted(b["data"].as_str().unwrap_or_default().into())),
                    Some("tool_use") => {
                        let id = b["id"].as_str().unwrap_or_default().to_string();
                        let name = b["name"].as_str().unwrap_or_default().to_string();
                        out.push(StreamEvent::ToolUseStart { id: id.clone(), name: name.clone() });
                        Some(Partial::Tool { id, name, json: String::new() })
                    }
                    _ => None, // server-side blocks are not used by this app
                };
                if self.blocks.len() <= idx {
                    self.blocks.resize_with(idx + 1, || None);
                }
                self.blocks[idx] = partial;
            }
            "content_block_delta" => {
                let idx = data["index"].as_u64().unwrap_or(0) as usize;
                let d = &data["delta"];
                if let Some(Some(p)) = self.blocks.get_mut(idx) {
                    match (p, d["type"].as_str()) {
                        (Partial::Text(t), Some("text_delta")) => {
                            let s = d["text"].as_str().unwrap_or_default();
                            t.push_str(s);
                            out.push(StreamEvent::TextDelta { text: s.into() });
                        }
                        (Partial::Thinking { thinking, .. }, Some("thinking_delta")) => {
                            let s = d["thinking"].as_str().unwrap_or_default();
                            thinking.push_str(s);
                            out.push(StreamEvent::ThinkingDelta { text: s.into() });
                        }
                        (Partial::Thinking { signature, .. }, Some("signature_delta")) => {
                            *signature = d["signature"].as_str().map(String::from);
                        }
                        (Partial::Tool { id, json, .. }, Some("input_json_delta")) => {
                            let s = d["partial_json"].as_str().unwrap_or_default();
                            json.push_str(s);
                            out.push(StreamEvent::ToolInputDelta { id: id.clone(), partial_json: s.into() });
                        }
                        _ => {}
                    }
                }
            }
            "message_delta" => {
                if let Some(r) = data["delta"]["stop_reason"].as_str() {
                    self.stop_reason = Some(r.to_string());
                }
                if let Some(o) = data["usage"]["output_tokens"].as_u64() {
                    self.usage.output_tokens = o;
                }
            }
            "error" => {
                let msg = data["error"]["message"].as_str().unwrap_or("stream error");
                let ty = data["error"]["type"].as_str().unwrap_or("error");
                return Err(AppError::Provider(format!("{ty}: {msg}")));
            }
            _ => {}
        }
        Ok(out)
    }

    pub fn finish(self) -> ChatResponse {
        let content = self
            .blocks
            .into_iter()
            .flatten()
            .map(|p| match p {
                Partial::Text(text) => ContentBlock::Text { text },
                Partial::Thinking { thinking, signature } => ContentBlock::Thinking { thinking, signature },
                Partial::Redacted(data) => ContentBlock::RedactedThinking { data },
                Partial::Tool { id, name, json } => {
                    // Eager input streaming means the server does not validate:
                    // invalid JSON becomes an `__invalid_json` marker the agent rejects.
                    let input = if json.trim().is_empty() {
                        json!({})
                    } else {
                        serde_json::from_str(&json).unwrap_or_else(|_| json!({"__invalid_json": json}))
                    };
                    ContentBlock::ToolUse { id, name, input }
                }
            })
            .filter(|b| !matches!(b, ContentBlock::Text { text } if text.is_empty()))
            .collect();
        ChatResponse {
            content,
            stop_reason: StopReason::parse(self.stop_reason.as_deref().unwrap_or("end_turn")),
            usage: self.usage,
            model: self.model,
        }
    }
}

#[async_trait::async_trait]
impl Provider for AnthropicProvider {
    fn id(&self) -> &'static str {
        "anthropic"
    }

    fn model(&self) -> String {
        self.cfg.model.clone()
    }

    async fn stream(&self, req: &ChatRequest, cancel: &CancellationToken, sink: EventSink<'_>) -> AppResult<ChatResponse> {
        let key = self.key()?;
        let body = self.build_body(req);
        let resp = self.send_with_retry(&body, &key, cancel).await?;
        let mut stream = resp.bytes_stream();
        let mut parser = SseParser::new();
        let mut acc = StreamAccumulator::default();
        let mut pending = Vec::<u8>::new();
        loop {
            let chunk = tokio::select! {
                c = stream.next() => c,
                _ = cancel.cancelled() => return Err(AppError::Cancelled),
            };
            let Some(chunk) = chunk else { break };
            let chunk = chunk.map_err(|e| AppError::Provider(format!("stream interrupted: {e}")))?;
            pending.extend_from_slice(&chunk);
            // Only decode complete UTF-8 sequences.
            let valid = match std::str::from_utf8(&pending) {
                Ok(_) => pending.len(),
                Err(e) => e.valid_up_to(),
            };
            let text = String::from_utf8_lossy(&pending[..valid]).to_string();
            pending.drain(..valid);
            for ev in parser.push(&text) {
                let name = ev.event.clone().unwrap_or_default();
                if name == "ping" || ev.data.is_empty() {
                    continue;
                }
                let data: Value = serde_json::from_str(&ev.data).unwrap_or(Value::Null);
                let name = if name.is_empty() { data["type"].as_str().unwrap_or_default().to_string() } else { name };
                for ui in acc.apply(&name, &data)? {
                    sink(ui);
                }
                if name == "message_stop" {
                    return Ok(acc.finish());
                }
            }
        }
        if acc.stop_reason.is_some() {
            Ok(acc.finish())
        } else {
            Err(AppError::Provider("stream ended before message_stop".into()))
        }
    }

    async fn check(&self) -> ProviderStatus {
        let base = ProviderStatus { provider: "anthropic".into(), model: self.cfg.model.clone(), state: String::new(), detail: String::new() };
        let key = match self.key() {
            Ok(k) => k,
            Err(e) => return ProviderStatus { state: "missing_credentials".into(), detail: e.to_string(), ..base },
        };
        let r = self
            .client
            .get(self.url(&format!("/v1/models/{}", self.cfg.model)))
            .header("x-api-key", key)
            .header("anthropic-version", API_VERSION)
            .timeout(std::time::Duration::from_secs(15))
            .send()
            .await;
        match r {
            Ok(r) if r.status().is_success() => ProviderStatus { state: "connected".into(), detail: "API key valid, model available".into(), ..base },
            Ok(r) => {
                let s = r.status().as_u16();
                let t = r.text().await.unwrap_or_default();
                let e = map_http_error(s, &t);
                let state = if s == 401 { "missing_credentials" } else { "error" };
                ProviderStatus { state: state.into(), detail: e.to_string(), ..base }
            }
            Err(e) => ProviderStatus { state: "unreachable".into(), detail: e.to_string(), ..base },
        }
    }

    async fn list_models(&self) -> AppResult<Vec<ModelInfo>> {
        let fallback = || KNOWN_MODELS.iter().map(|(id, l)| ModelInfo { id: (*id).into(), label: (*l).into() }).collect::<Vec<_>>();
        let Ok(key) = self.key() else { return Ok(fallback()) };
        let r = self
            .client
            .get(self.url("/v1/models?limit=100"))
            .header("x-api-key", key)
            .header("anthropic-version", API_VERSION)
            .send()
            .await;
        match r {
            Ok(r) if r.status().is_success() => {
                let v: Value = r.json().await?;
                let list: Vec<ModelInfo> = v["data"]
                    .as_array()
                    .map(|a| {
                        a.iter()
                            .filter_map(|m| {
                                Some(ModelInfo {
                                    id: m["id"].as_str()?.to_string(),
                                    label: m["display_name"].as_str().unwrap_or(m["id"].as_str()?).to_string(),
                                })
                            })
                            .collect()
                    })
                    .unwrap_or_default();
                Ok(if list.is_empty() { fallback() } else { list })
            }
            _ => Ok(fallback()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn feed(acc: &mut StreamAccumulator, events: &[(&str, Value)]) -> Vec<StreamEvent> {
        events.iter().flat_map(|(n, d)| acc.apply(n, d).unwrap()).collect()
    }

    #[test]
    fn accumulates_text_thinking_and_tool_use() {
        let mut acc = StreamAccumulator::default();
        let ui = feed(
            &mut acc,
            &[
                ("message_start", json!({"message": {"model": "claude-opus-5-5", "usage": {"input_tokens": 10}}})),
                ("content_block_start", json!({"index": 0, "content_block": {"type": "thinking", "thinking": ""}})),
                ("content_block_delta", json!({"index": 0, "delta": {"type": "thinking_delta", "thinking": "plan"}})),
                ("content_block_delta", json!({"index": 0, "delta": {"type": "signature_delta", "signature": "sig"}})),
                ("content_block_start", json!({"index": 1, "content_block": {"type": "text", "text": ""}})),
                ("content_block_delta", json!({"index": 1, "delta": {"type": "text_delta", "text": "Hi"}})),
                ("content_block_start", json!({"index": 2, "content_block": {"type": "tool_use", "id": "tu1", "name": "read_file", "input": {}}})),
                ("content_block_delta", json!({"index": 2, "delta": {"type": "input_json_delta", "partial_json": "{\"path\":"}})),
                ("content_block_delta", json!({"index": 2, "delta": {"type": "input_json_delta", "partial_json": "\"C:/a.txt\"}"}})),
                ("message_delta", json!({"delta": {"stop_reason": "tool_use"}, "usage": {"output_tokens": 42}})),
            ],
        );
        assert!(ui.contains(&StreamEvent::TextDelta { text: "Hi".into() }));
        let r = acc.finish();
        assert_eq!(r.stop_reason, StopReason::ToolUse);
        assert_eq!(r.usage.output_tokens, 42);
        assert_eq!(r.content.len(), 3);
        assert_eq!(r.content[0], ContentBlock::Thinking { thinking: "plan".into(), signature: Some("sig".into()) });
        assert_eq!(
            r.content[2],
            ContentBlock::ToolUse { id: "tu1".into(), name: "read_file".into(), input: json!({"path": "C:/a.txt"}) }
        );
    }

    #[test]
    fn invalid_tool_json_is_flagged() {
        let mut acc = StreamAccumulator::default();
        feed(
            &mut acc,
            &[
                ("content_block_start", json!({"index": 0, "content_block": {"type": "tool_use", "id": "t", "name": "x"}})),
                ("content_block_delta", json!({"index": 0, "delta": {"type": "input_json_delta", "partial_json": "{\"a\": "}})),
                ("message_delta", json!({"delta": {"stop_reason": "max_tokens"}})),
            ],
        );
        let r = acc.finish();
        match &r.content[0] {
            ContentBlock::ToolUse { input, .. } => assert!(input.get("__invalid_json").is_some()),
            _ => panic!(),
        }
    }

    #[test]
    fn stream_error_event_maps_to_error() {
        let mut acc = StreamAccumulator::default();
        let e = acc.apply("error", &json!({"error": {"type": "overloaded_error", "message": "busy"}})).unwrap_err();
        assert!(e.to_string().contains("overloaded_error"));
    }

    #[test]
    fn request_body_shape() {
        let p = AnthropicProvider::new(AnthropicConfig::default());
        let req = ChatRequest {
            system: "sys".into(),
            messages: vec![ChatMessage { role: Role::User, content: vec![ContentBlock::Text { text: "hi".into() }] }],
            tools: vec![ToolSpec { name: "t".into(), description: "d".into(), input_schema: json!({"type": "object"}) }],
        };
        let b = p.build_body(&req);
        assert_eq!(b["model"], "claude-opus-5-5");
        assert_eq!(b["thinking"]["type"], "adaptive");
        assert_eq!(b["output_config"]["effort"], "medium");
        assert_eq!(b["messages"][0]["content"][0]["type"], "text");
        assert_eq!(b["tools"][0]["eager_input_streaming"], true);
        assert_eq!(b["fallbacks"], "default");
        assert!(b.get("temperature").is_none());
    }

    #[test]
    fn http_errors_are_classified() {
        assert_eq!(map_http_error(401, r#"{"error":{"message":"bad key"}}"#).kind(), "missing_credential");
        assert!(map_http_error(429, "{}").to_string().contains("Rate limited"));
    }
}
