mod apps;
mod commands;
mod files;
mod memory_tools;
mod settings_tools;
mod system;
mod task_tools;

pub use files::read_text_file;

use super::Tool;
use std::sync::Arc;

pub fn all() -> Vec<Arc<dyn Tool>> {
    vec![
        Arc::new(files::SearchFiles),
        Arc::new(files::ListDirectory),
        Arc::new(files::ReadFile),
        Arc::new(files::WriteFile),
        Arc::new(apps::LaunchApp),
        Arc::new(apps::OpenUrl),
        Arc::new(system::SystemInfo),
        Arc::new(commands::RunCommand),
        Arc::new(memory_tools::Remember),
        Arc::new(memory_tools::SearchMemory),
        Arc::new(settings_tools::GetSettings),
        Arc::new(settings_tools::UpdateSetting),
        Arc::new(task_tools::CreateTaskPlan),
    ]
}
