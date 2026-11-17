use crate::error::AppResult;
use crate::memory::{self, Memory, MemoryInput, Project, ProjectInput};
use crate::state::AppState;
use tauri::State;

#[tauri::command]
pub fn list_memories(state: State<AppState>, query: Option<String>, kind: Option<String>, project_id: Option<String>) -> AppResult<Vec<Memory>> {
    memory::search_memories(&state.db, query.as_deref(), kind.as_deref(), project_id.as_deref(), 500)
}

#[tauri::command]
pub fn create_memory(state: State<AppState>, input: MemoryInput, allow_sensitive: Option<bool>) -> AppResult<Memory> {
    memory::create_memory(&state.db, &input, "user", allow_sensitive.unwrap_or(false))
}

#[tauri::command]
pub fn update_memory(state: State<AppState>, id: String, input: MemoryInput, allow_sensitive: Option<bool>) -> AppResult<Memory> {
    memory::update_memory(&state.db, &id, &input, allow_sensitive.unwrap_or(false))
}

#[tauri::command]
pub fn delete_memory(state: State<AppState>, id: String) -> AppResult<()> {
    memory::delete_memory(&state.db, &id)
}

#[tauri::command]
pub fn detect_sensitive(text: String) -> Vec<&'static str> {
    memory::sensitive::detect(&text)
}

#[tauri::command]
pub fn list_projects(state: State<AppState>) -> AppResult<Vec<Project>> {
    memory::list_projects(&state.db)
}

#[tauri::command]
pub fn create_project(state: State<AppState>, input: ProjectInput) -> AppResult<Project> {
    memory::create_project(&state.db, &input)
}

#[tauri::command]
pub fn update_project(state: State<AppState>, id: String, input: ProjectInput) -> AppResult<Project> {
    memory::update_project(&state.db, &id, &input)
}

#[tauri::command]
pub fn delete_project(state: State<AppState>, id: String) -> AppResult<()> {
    memory::delete_project(&state.db, &id)
}
