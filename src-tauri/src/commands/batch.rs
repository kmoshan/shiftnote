use crate::db::batch;
use crate::error::AppResult;
use crate::models::{TaskBatchInput, TaskBatchResult};
use crate::state::AppState;
use tauri::State;

#[tauri::command]
pub fn task_batch(state: State<'_, AppState>, input: TaskBatchInput) -> AppResult<TaskBatchResult> {
    state.db.with_conn(|conn| batch::execute(conn, &input))
}
