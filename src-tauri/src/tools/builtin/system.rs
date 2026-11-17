use crate::config::PermissionMode;
use crate::error::AppResult;
use crate::permissions::Access;
use crate::tools::{Tool, ToolContext, ToolMeta};
use serde_json::{json, Value};

pub struct SystemInfo;

pub fn collect() -> Value {
    use sysinfo::{Disks, System};
    let mut sys = System::new();
    sys.refresh_memory();
    sys.refresh_cpu_all();
    let disks = Disks::new_with_refreshed_list();
    let gb = |b: u64| (b as f64 / 1_073_741_824.0 * 10.0).round() / 10.0;
    json!({
        "os": System::long_os_version().unwrap_or_default(),
        "os_build": System::kernel_version().unwrap_or_default(),
        "hostname": System::host_name().unwrap_or_default(),
        "architecture": std::env::consts::ARCH,
        "cpu": sys.cpus().first().map(|c| c.brand().trim().to_string()).unwrap_or_default(),
        "logical_cpus": sys.cpus().len(),
        "memory_total_gb": gb(sys.total_memory()),
        "memory_used_gb": gb(sys.used_memory()),
        "uptime_hours": (System::uptime() as f64 / 360.0).round() / 10.0,
        "disks": disks.iter().map(|d| json!({
            "mount": d.mount_point().to_string_lossy(),
            "total_gb": gb(d.total_space()),
            "free_gb": gb(d.available_space()),
        })).collect::<Vec<_>>(),
        "local_time": chrono::Local::now().to_rfc2822(),
    })
}

#[async_trait::async_trait]
impl Tool for SystemInfo {
    fn meta(&self) -> ToolMeta {
        ToolMeta {
            name: "system_info".into(),
            description: "Get basic system information: Windows version, CPU, memory, disks, uptime and local time.".into(),
            input_schema: json!({"type": "object", "properties": {}, "additionalProperties": false}),
            access: Access::System,
            default_mode: PermissionMode::Allow,
            locked_ask: false,
            origin: "builtin".into(),
        }
    }

    fn describe(&self, _input: &Value) -> String {
        "Read basic system information (OS, CPU, memory, disks)".into()
    }

    async fn run(&self, _ctx: &ToolContext, _input: Value) -> AppResult<String> {
        let v = tokio::task::spawn_blocking(collect).await.unwrap_or(Value::Null);
        Ok(serde_json::to_string_pretty(&v)?)
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn collects_fields() {
        let v = super::collect();
        assert!(v["logical_cpus"].as_u64().unwrap() > 0);
        assert!(v["memory_total_gb"].as_f64().unwrap() > 0.0);
    }
}
