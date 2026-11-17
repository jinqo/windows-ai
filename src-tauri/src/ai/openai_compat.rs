//! OpenAI-compatible chat-completions provider for local or third-party
//! (non-Claude) models such as Ollama or LM Studio. Optional; the Anthropic
//! provider is the primary backend.

use super::provider::{http_client, EventSink, ModelInfo, Provider, ProviderStatus};
use super::sse::SseParser;
use super::types::*;
use crate::config::OpenAiCompatibleConfig;
use crate::error::{AppError, AppResult};
use futures_util::StreamExt;
use serde_json::{json, Value};
use std::collections::BTreeMap;
use tokio_util::sync::CancellationToken;

pub struct OpenAiCompatProvider {
    cfg: OpenAiCompatibleConfig,
    client: reqwest::Client,
    /// Credential Manager slot holding the bearer token.
    key_slot: &'static str,
}

impl OpenAiCompatProvider {
    pub fn new(cfg: OpenAiCompatibleConfig) -> Self {
        Self { cfg, client: http_client(), key_slot: "openai_compatible_api_key" }
    }

    /// Same wire protocol, different stored key (e.g. the FreeLLMAPI fallback).
    pub fn with_key_slot(cfg: OpenAiCompatibleConfig, key_slot: &'static str) -> Self {
        Self { cfg, client: http_client(), key_slot }
    }

    fn url(&self, p: &str) -> String {
        format!("{}{}", self.cfg.base_url.trim_end_matches('/'), p)
    }

    fn auth(&self, rb: reqwest::RequestBuilder) -> AppResult<reqwest::RequestBuilder> {
        if !self.cfg.use_api_key {
            return Ok(rb);
        }
        let key = crate::secrets::get_secret(self.key_slot)
            .ok_or_else(|| AppError::MissingCredential("No API key stored for the OpenAI-compatible provider".into()))?;
        Ok(rb.bearer_auth(key))
    }

    async fn send(&self, body: &Value, cancel: &CancellationToken) -> AppResult<reqwest::Response> {
        let rb = self.auth(self.client.post(self.url("/chat/completions")).json(body))?;
        tokio::select! {
            r = rb.send() => r.map_err(|e| AppError::Provider(format!("cannot reach {}: {e}", self.cfg.base_url))),
            _ = cancel.cancelled() => Err(AppError::Cancelled),
        }
    }
}

/// True when a server rejected the request because the model cannot use tools.
pub fn tools_unsupported(status: u16, body: &str) -> bool {
    let b = body.to_ascii_lowercase();
    matches!(status, 400 | 404 | 422 | 501)
        && (b.contains("tool") || b.contains("function"))
        && (b.contains("support") || b.contains("not available") || b.contains("no endpoints"))
}

/// Human-readable error for common free-tier / local-server failures.
pub fn http_error_message(status: u16, body: &str) -> String {
    let snippet: String = body.chars().take(300).collect();
    match status {
        401 | 403 => format!("The server rejected the API key (HTTP {status}). Check the key in Settings → AI providers. {snippet}"),
        429 => format!("Rate limit or free-tier quota reached (HTTP 429). Wait a bit or pick another model/provider. {snippet}"),
        404 => format!("Model or endpoint not found (HTTP 404). Check the base URL and model name. {snippet}"),
        _ => format!("HTTP {status}: {snippet}"),
    }
}

/// Converts canonical messages to chat-completions messages.
pub fn to_openai_messages(system: &str, msgs: &[ChatMessage]) -> Vec<Value> {
    let mut out = vec![json!({"role": "system", "content": system})];
    for m in msgs {
        match m.role {
            Role::User => {
                let mut text = String::new();
                for b in &m.content {
                    match b {
                        ContentBlock::ToolResult { tool_use_id, content, is_error } => {
                            let c = if *is_error { format!("ERROR: {content}") } else { content.clone() };
                            out.push(json!({"role": "tool", "tool_call_id": tool_use_id, "content": c}));
                        }
                        ContentBlock::Text { text: t } => text.push_str(t),
                        _ => {}
                    }
                }
                if !text.is_empty() {
                    out.push(json!({"role": "user", "content": text}));
                }
            }
            Role::Assistant => {
                let text = text_of(&m.content);
                let calls: Vec<Value> = m
                    .content
                    .iter()
                    .filter_map(|b| match b {
                        ContentBlock::ToolUse { id, name, input } => Some(json!({
                            "id": id, "type": "function",
                            "function": {"name": name, "arguments": input.to_string()}
                        })),
                        _ => None,
                    })
                    .collect();
                let mut msg = json!({"role": "assistant", "content": if text.is_empty() { Value::Null } else { Value::String(text) }});
                if !calls.is_empty() {
                    msg["tool_calls"] = Value::Array(calls);
                }
                out.push(msg);
            }
        }
    }
    out
}

#[derive(Default)]
pub struct OaAccumulator {
    text: String,
    calls: BTreeMap<u64, (String, String, String)>,
    pub finish: Option<String>,
    pub model: String,
    pub usage: Usage,
}

impl OaAccumulator {
    pub fn apply(&mut self, v: &Value) -> Vec<StreamEvent> {
        let mut out = Vec::new();
        if let Some(m) = v["model"].as_str() {
            self.model = m.into();
        }
        if let Some(u) = v.get("usage").filter(|u| u.is_object()) {
            self.usage.input_tokens = u["prompt_tokens"].as_u64().unwrap_or(0);
            self.usage.output_tokens = u["completion_tokens"].as_u64().unwrap_or(0);
        }
        let choice = &v["choices"][0];
        let d = &choice["delta"];
        if let Some(t) = d["content"].as_str() {
            if !t.is_empty() {
                self.text.push_str(t);
                out.push(StreamEvent::TextDelta { text: t.into() });
            }
        }
        if let Some(calls) = d["tool_calls"].as_array() {
            for c in calls {
                let idx = c["index"].as_u64().unwrap_or(0);
                let entry = self.calls.entry(idx).or_insert_with(|| (String::new(), String::new(), String::new()));
                if let Some(id) = c["id"].as_str() {
                    entry.0 = id.into();
                }
                if let Some(n) = c["function"]["name"].as_str() {
                    entry.1.push_str(n);
                    out.push(StreamEvent::ToolUseStart { id: entry.0.clone(), name: entry.1.clone() });
                }
                if let Some(a) = c["function"]["arguments"].as_str() {
                    entry.2.push_str(a);
                }
            }
        }
        if let Some(f) = choice["finish_reason"].as_str() {
            self.finish = Some(f.into());
        }
        out
    }

    pub fn finish(self) -> ChatResponse {
        let mut content = Vec::new();
        if !self.text.is_empty() {
            content.push(ContentBlock::Text { text: self.text });
        }
        let has_calls = !self.calls.is_empty();
        for (i, (id, name, args)) in self.calls {
            let input = if args.trim().is_empty() { json!({}) } else { serde_json::from_str(&args).unwrap_or_else(|_| json!({"__invalid_json": args})) };
            let id = if id.is_empty() { format!("call_{i}") } else { id };
            content.push(ContentBlock::ToolUse { id, name, input });
        }
        let stop = if has_calls { StopReason::ToolUse } else { StopReason::parse(self.finish.as_deref().unwrap_or("stop")) };
        ChatResponse { content, stop_reason: stop, usage: self.usage, model: self.model }
    }
}

#[async_trait::async_trait]
impl Provider for OpenAiCompatProvider {
    fn id(&self) -> &'static str {
        "openai_compatible"
    }

    fn model(&self) -> String {
        self.cfg.model.clone()
    }

    async fn stream(&self, req: &ChatRequest, cancel: &CancellationToken, sink: EventSink<'_>) -> AppResult<ChatResponse> {
        let mut body = json!({
            "model": self.cfg.model,
            "max_tokens": self.cfg.max_tokens,
            "stream": true,
            "messages": to_openai_messages(&req.system, &req.messages),
        });
        if !req.tools.is_empty() {
            body["tools"] = Value::Array(
                req.tools
                    .iter()
                    .map(|t| json!({"type": "function", "function": {"name": t.name, "description": t.description, "parameters": t.input_schema}}))
                    .collect(),
            );
        }
        let mut resp = self.send(&body, cancel).await?;
        if !resp.status().is_success() {
            let s = resp.status().as_u16();
            let t = resp.text().await.unwrap_or_default();
            if body.get("tools").is_some() && tools_unsupported(s, &t) {
                // Many free/local models have no function calling: answer without tools.
                body.as_object_mut().map(|o| o.remove("tools"));
                resp = self.send(&body, cancel).await?;
                if !resp.status().is_success() {
                    let s = resp.status().as_u16();
                    let t = resp.text().await.unwrap_or_default();
                    return Err(AppError::Provider(http_error_message(s, &t)));
                }
            } else {
                return Err(AppError::Provider(http_error_message(s, &t)));
            }
        }
        let mut stream = resp.bytes_stream();
        let mut parser = SseParser::new();
        let mut acc = OaAccumulator::default();
        loop {
            let chunk = tokio::select! {
                c = stream.next() => c,
                _ = cancel.cancelled() => return Err(AppError::Cancelled),
            };
            let Some(chunk) = chunk else { break };
            let chunk = chunk.map_err(|e| AppError::Provider(format!("stream interrupted: {e}")))?;
            for ev in parser.push(&String::from_utf8_lossy(&chunk)) {
                if ev.data == "[DONE]" {
                    return Ok(acc.finish());
                }
                if let Ok(v) = serde_json::from_str::<Value>(&ev.data) {
                    if let Some(err) = v.get("error") {
                        return Err(AppError::Provider(err.to_string()));
                    }
                    for e in acc.apply(&v) {
                        sink(e);
                    }
                }
            }
        }
        Ok(acc.finish())
    }

    async fn check(&self) -> ProviderStatus {
        let base = ProviderStatus { provider: "openai_compatible".into(), model: self.cfg.model.clone(), state: String::new(), detail: String::new() };
        let rb = match self.auth(self.client.get(self.url("/models")).timeout(std::time::Duration::from_secs(8))) {
            Ok(rb) => rb,
            Err(e) => return ProviderStatus { state: "missing_credentials".into(), detail: e.to_string(), ..base },
        };
        match rb.send().await {
            Ok(r) if r.status().is_success() => ProviderStatus { state: "connected".into(), detail: format!("Reachable at {}", self.cfg.base_url), ..base },
            Ok(r) => ProviderStatus { state: "error".into(), detail: format!("HTTP {}", r.status()), ..base },
            Err(e) => ProviderStatus { state: "unreachable".into(), detail: format!("{}: {e}", self.cfg.base_url), ..base },
        }
    }

    async fn list_models(&self) -> AppResult<Vec<ModelInfo>> {
        let r = self.auth(self.client.get(self.url("/models")))?.send().await?;
        let v: Value = r.json().await?;
        Ok(v["data"]
            .as_array()
            .map(|a| a.iter().filter_map(|m| m["id"].as_str()).map(|id| ModelInfo { id: id.into(), label: id.into() }).collect())
            .unwrap_or_default())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn converts_tool_round_trip() {
        let msgs = vec![
            ChatMessage { role: Role::User, content: vec![ContentBlock::Text { text: "hi".into() }] },
            ChatMessage {
                role: Role::Assistant,
                content: vec![
                    ContentBlock::Thinking { thinking: "x".into(), signature: None },
                    ContentBlock::ToolUse { id: "c1".into(), name: "f".into(), input: json!({"a": 1}) },
                ],
            },
            ChatMessage { role: Role::User, content: vec![ContentBlock::ToolResult { tool_use_id: "c1".into(), content: "ok".into(), is_error: false }] },
        ];
        let out = to_openai_messages("sys", &msgs);
        assert_eq!(out.len(), 4);
        assert_eq!(out[2]["tool_calls"][0]["function"]["arguments"], "{\"a\":1}");
        assert_eq!(out[3]["role"], "tool");
    }

    #[test]
    fn accumulates_streamed_tool_calls() {
        let mut acc = OaAccumulator::default();
        acc.apply(&json!({"choices": [{"delta": {"tool_calls": [{"index": 0, "id": "c1", "function": {"name": "read_file", "arguments": "{\"pa"}}]}}]}));
        acc.apply(&json!({"choices": [{"delta": {"tool_calls": [{"index": 0, "function": {"arguments": "th\":\"x\"}"}}]}, "finish_reason": "tool_calls"}]}));
        let r = acc.finish();
        assert_eq!(r.stop_reason, StopReason::ToolUse);
        assert_eq!(r.content[0], ContentBlock::ToolUse { id: "c1".into(), name: "read_file".into(), input: json!({"path": "x"}) });
    }

    #[test]
    fn detects_models_without_tool_support() {
        assert!(tools_unsupported(400, r#"{"error":"registry.ollama.ai/library/gemma:2b does not support tools"}"#));
        assert!(tools_unsupported(404, r#"{"error":{"message":"No endpoints found that support tool use"}}"#));
        assert!(!tools_unsupported(400, r#"{"error":"context length exceeded"}"#));
        assert!(!tools_unsupported(429, "tools not supported"));
    }

    #[test]
    fn friendly_http_errors() {
        assert!(http_error_message(429, "slow down").contains("free-tier quota"));
        assert!(http_error_message(401, "").contains("API key"));
        assert!(http_error_message(500, "boom").starts_with("HTTP 500"));
    }
}
