use crate::config::PermissionMode;
use crate::error::{AppError, AppResult};
use crate::memory::{self, MemoryInput};
use crate::permissions::Access;
use crate::tools::{str_arg, Tool, ToolContext, ToolMeta};
use serde_json::{json, Value};

pub struct Remember;

#[async_trait::async_trait]
impl Tool for Remember {
    fn meta(&self) -> ToolMeta {
        ToolMeta {
            name: "remember".into(),
            description: "Propose saving a long-term memory about the user (a preference, fact, project detail or note). The user must approve every save. Never store secrets, passwords or financial data.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "kind": {"enum": ["preference", "fact", "project", "note"]},
                    "content": {"type": "string", "minLength": 1, "maxLength": 1000},
                    "tags": {"type": "string"}
                },
                "required": ["kind", "content"],
                "additionalProperties": false
            }),
            access: Access::Memory,
            default_mode: PermissionMode::Ask,
            locked_ask: true,
            origin: "builtin".into(),
        }
    }

    fn describe(&self, input: &Value) -> String {
        format!("Save {} memory: \"{}\"", str_arg(input, "kind"), str_arg(input, "content"))
    }

    async fn run(&self, ctx: &ToolContext, input: Value) -> AppResult<String> {
        if !ctx.cfg.memory.allow_ai_save {
            return Err(AppError::PermissionDenied("Saving memories from chat is disabled in Settings → Memory.".into()));
        }
        let m = memory::create_memory(
            &ctx.db,
            &MemoryInput {
                kind: str_arg(&input, "kind").into(),
                content: str_arg(&input, "content").into(),
                tags: str_arg(&input, "tags").into(),
                project_id: None,
                pinned: false,
            },
            "assistant",
            false,
        )?;
        ctx.notifier.emit("memory-changed", json!({"id": m.id}));
        Ok(format!("Saved memory {}", m.id))
    }
}

pub struct SearchMemory;

#[async_trait::async_trait]
impl Tool for SearchMemory {
    fn meta(&self) -> ToolMeta {
        ToolMeta {
            name: "search_memory".into(),
            description: "Search the user's saved local memories.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {"query": {"type": "string", "minLength": 1}},
                "required": ["query"],
                "additionalProperties": false
            }),
            access: Access::Read,
            default_mode: PermissionMode::Allow,
            locked_ask: false,
            origin: "builtin".into(),
        }
    }

    fn describe(&self, input: &Value) -> String {
        format!("Search memories for \"{}\"", str_arg(input, "query"))
    }

    async fn run(&self, ctx: &ToolContext, input: Value) -> AppResult<String> {
        let hits = memory::search_memories(&ctx.db, Some(str_arg(&input, "query")), None, None, 10)?;
        if hits.is_empty() {
            return Ok("No matching memories.".into());
        }
        Ok(hits.iter().map(|m| format!("- [{}] {}", m.kind, m.content)).collect::<Vec<_>>().join("\n"))
    }
}
