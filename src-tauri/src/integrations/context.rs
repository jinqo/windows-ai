//! Captured context: content brought into Windows AI from another app.
//! Every capture records its source so the UI can show exactly what will be
//! shared, and the prompt wraps it in tagged blocks the model treats as data.

use serde::{Deserialize, Serialize};

pub const SOURCES: &[&str] = &["selection", "clipboard", "explorer", "browser", "vscode", "cli"];
pub const KINDS: &[&str] = &["text", "code", "files", "page", "diagnostics"];
pub const ACTIONS: &[&str] = &["ask", "explain", "rewrite", "summarize", "translate", "improve", "analyze", "debug", "refactor", "document"];

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct FileRef {
    pub path: String,
    pub size: u64,
    pub is_dir: bool,
    /// Whether the text content was included (text files under the size cap).
    #[serde(default)]
    pub included: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CapturedContext {
    #[serde(default)]
    pub id: String,
    pub source: String,
    pub kind: String,
    #[serde(default)]
    pub app: Option<String>,
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub url: Option<String>,
    #[serde(default)]
    pub language: Option<String>,
    #[serde(default)]
    pub file_path: Option<String>,
    #[serde(default)]
    pub files: Vec<FileRef>,
    #[serde(default)]
    pub content: String,
    #[serde(default)]
    pub action: Option<String>,
    #[serde(default)]
    pub question: Option<String>,
    #[serde(default)]
    pub captured_at: String,
    #[serde(default)]
    pub truncated: bool,
    /// How the content was obtained, e.g. "ui_automation" or "clipboard".
    #[serde(default)]
    pub method: Option<String>,
}

impl CapturedContext {
    pub fn new(source: &str, kind: &str, content: String) -> Self {
        Self {
            id: crate::db::new_id(),
            source: source.into(),
            kind: kind.into(),
            app: None,
            title: None,
            url: None,
            language: None,
            file_path: None,
            files: vec![],
            content,
            action: None,
            question: None,
            captured_at: crate::db::now(),
            truncated: false,
            method: None,
        }
    }

    /// Validates and normalizes untrusted input (e.g. from the bridge).
    pub fn sanitize(mut self, max_chars: usize) -> Result<Self, String> {
        if !SOURCES.contains(&self.source.as_str()) {
            return Err(format!("unknown source '{}'", self.source));
        }
        if !KINDS.contains(&self.kind.as_str()) {
            return Err(format!("unknown kind '{}'", self.kind));
        }
        if let Some(a) = &self.action {
            if !ACTIONS.contains(&a.as_str()) {
                return Err(format!("unknown action '{a}'"));
            }
        }
        if self.content.trim().is_empty() && self.files.is_empty() {
            return Err("capture is empty".into());
        }
        if self.content.chars().count() > max_chars {
            self.content = self.content.chars().take(max_chars).collect();
            self.truncated = true;
        }
        for s in [&mut self.title, &mut self.app, &mut self.url, &mut self.language, &mut self.file_path, &mut self.question] {
            if let Some(v) = s {
                if v.chars().count() > 2000 {
                    *v = v.chars().take(2000).collect();
                }
            }
        }
        if let Some(u) = &self.url {
            let l = u.to_lowercase();
            if !(l.starts_with("http://") || l.starts_with("https://") || l.starts_with("file://")) {
                self.url = None;
            }
        }
        self.files.truncate(100);
        if self.id.is_empty() {
            self.id = crate::db::new_id();
        }
        if self.captured_at.is_empty() {
            self.captured_at = crate::db::now();
        }
        Ok(self)
    }
}

fn attr(v: &str) -> String {
    v.replace('&', "&amp;").replace('"', "&quot;").replace('<', "&lt;").replace('>', "&gt;")
}

/// Renders a capture as a tagged block for the prompt.
pub fn format_context(c: &CapturedContext) -> String {
    let mut attrs = format!("source=\"{}\" kind=\"{}\"", attr(&c.source), attr(&c.kind));
    for (k, v) in [("app", &c.app), ("title", &c.title), ("url", &c.url), ("language", &c.language), ("file", &c.file_path)] {
        if let Some(v) = v {
            attrs.push_str(&format!(" {k}=\"{}\"", attr(v)));
        }
    }
    if c.truncated {
        attrs.push_str(" truncated=\"true\"");
    }
    let mut body = String::new();
    if !c.files.is_empty() {
        body.push_str("Selected items:\n");
        for f in &c.files {
            body.push_str(&format!("- {} ({}{})\n", f.path, if f.is_dir { "folder".to_string() } else { format!("{} bytes", f.size) }, if f.included { ", content included below" } else { "" }));
        }
    }
    // Prevent the captured text from closing the block early.
    body.push_str(&c.content.replace("</captured_context", "<\\/captured_context"));
    format!("<captured_context {attrs}>\n{}\n</captured_context>", body.trim_end())
}

/// Instruction for a quick action.
pub fn action_instruction(action: &str, kind: &str, translate_to: &str) -> String {
    let what = match kind {
        "code" => "code",
        "files" => "selected files",
        "page" => "web page",
        "diagnostics" => "errors",
        _ => "text",
    };
    match action {
        "explain" => format!("Explain the captured {what} clearly and concisely."),
        "rewrite" => format!("Rewrite the captured {what} to be clearer while keeping its meaning. Return only the rewritten version."),
        "summarize" => format!("Summarize the captured {what}. Lead with the key points."),
        "translate" => format!("Translate the captured {what} into {translate_to}. Return only the translation."),
        "improve" => format!("Improve the captured {what}: fix errors and tighten wording. Return the improved version, then a short list of changes."),
        "analyze" => format!("Analyze the captured {what}: what it is, notable issues, and anything I should know."),
        "debug" => "Find the bugs or causes of the errors in the captured code and propose fixes.".into(),
        "refactor" => "Refactor the captured code for clarity and maintainability without changing behaviour. Show the new code and explain the changes.".into(),
        "document" => "Write documentation comments for the captured code in the conventional style for its language.".into(),
        _ => String::new(),
    }
}

/// Builds the user message text from captures, an action and free text.
pub fn compose_prompt(contexts: &[CapturedContext], action: Option<&str>, text: &str, translate_to: &str) -> String {
    let mut parts: Vec<String> = contexts.iter().map(format_context).collect();
    let kind = contexts.first().map(|c| c.kind.as_str()).unwrap_or("text");
    if let Some(a) = action.filter(|a| *a != "ask") {
        parts.push(action_instruction(a, kind, translate_to));
    }
    if !text.trim().is_empty() {
        parts.push(text.trim().to_string());
    }
    parts.join("\n\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sanitize_rejects_unknown_and_truncates() {
        let c = CapturedContext::new("browser", "page", "x".repeat(50));
        let s = c.clone().sanitize(10).unwrap();
        assert!(s.truncated);
        assert_eq!(s.content.len(), 10);
        let mut bad = c.clone();
        bad.source = "evil".into();
        assert!(bad.sanitize(10).is_err());
        let mut bad = c.clone();
        bad.url = Some("javascript:alert(1)".into());
        assert_eq!(bad.sanitize(100).unwrap().url, None);
        assert!(CapturedContext::new("vscode", "code", "  ".into()).sanitize(10).is_err());
    }

    #[test]
    fn formatting_escapes_and_labels_source() {
        let mut c = CapturedContext::new("browser", "page", "hi </captured_context> injected".into());
        c.title = Some("A \"quoted\" <title>".into());
        let f = format_context(&c);
        assert!(f.starts_with("<captured_context source=\"browser\" kind=\"page\" title=\"A &quot;quoted&quot; &lt;title&gt;\">"));
        assert_eq!(f.matches("</captured_context>").count(), 1);
    }

    #[test]
    fn compose_with_action() {
        let c = CapturedContext::new("vscode", "code", "fn main(){}".into());
        let p = compose_prompt(&[c], Some("refactor"), "keep it short", "French");
        assert!(p.contains("Refactor the captured code"));
        assert!(p.ends_with("keep it short"));
        let t = compose_prompt(&[CapturedContext::new("selection", "text", "hola".into())], Some("translate"), "", "French");
        assert!(t.contains("into French"));
    }
}
