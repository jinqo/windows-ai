use crate::automation::store::{create_task, NewStep};
use crate::config::PermissionMode;
use crate::error::{AppError, AppResult};
use crate::permissions::Access;
use crate::tools::{str_arg, Tool, ToolContext, ToolMeta};
use serde_json::{json, Value};

pub struct CreateTaskPlan;

#[async_trait::async_trait]
impl Tool for CreateTaskPlan {
    fn meta(&self) -> ToolMeta {
        ToolMeta {
            name: "create_task_plan".into(),
            description: "Create a multi-step task plan for the user to review. Nothing runs until the user starts it in the Activity view; each step still passes the normal permission checks. Each step names a tool and its input.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "title": {"type": "string", "minLength": 1, "maxLength": 200},
                    "steps": {
                        "type": "array",
                        "maxItems": 25,
                        "items": {
                            "type": "object",
                            "properties": {
                                "description": {"type": "string", "minLength": 1},
                                "tool": {"type": "string", "minLength": 1},
                                "input": {"type": "object"}
                            },
                            "required": ["description", "tool", "input"]
                        }
                    }
                },
                "required": ["title", "steps"],
                "additionalProperties": false
            }),
            access: Access::Read,
            default_mode: PermissionMode::Allow,
            locked_ask: false,
            origin: "builtin".into(),
        }
    }

    fn describe(&self, input: &Value) -> String {
        let n = input.get("steps").and_then(Value::as_array).map(|a| a.len()).unwrap_or(0);
        format!("Draft task plan \"{}\" with {n} steps (does not run anything)", str_arg(input, "title"))
    }

    async fn run(&self, ctx: &ToolContext, input: Value) -> AppResult<String> {
        let steps: Vec<NewStep> = input
            .get("steps")
            .and_then(Value::as_array)
            .ok_or_else(|| AppError::InvalidInput("steps required".into()))?
            .iter()
            .map(|s| NewStep {
                description: str_arg(s, "description").into(),
                tool: str_arg(s, "tool").into(),
                input: s.get("input").cloned().unwrap_or(json!({})),
            })
            .collect();
        if steps.is_empty() {
            return Err(AppError::InvalidInput("a plan needs at least one step".into()));
        }
        if steps.iter().any(|s| s.tool == "create_task_plan") {
            return Err(AppError::InvalidInput("plans cannot create nested plans".into()));
        }
        let task = create_task(&ctx.db, str_arg(&input, "title"), ctx.conversation_id.as_deref(), &steps)?;
        ctx.notifier.emit("task-updated", json!({"task_id": task.id}));
        Ok(format!(
            "Created task plan \"{}\" ({} steps, id {}). It is waiting for the user to review and start it in the Activity view.",
            task.title,
            task.steps.len(),
            task.id
        ))
    }
}
