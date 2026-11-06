use tauri::State;

use crate::db::tasks as repo;
use crate::db::updates as update_repo;
use crate::error::AppResult;
use crate::models::{
    Counts, CreateTaskInput, Task, TaskDetail, TaskFilter, TaskMoveInput, TaskPage, TaskPatch,
    TaskUpdate,
};
use crate::state::AppState;

#[tauri::command]
pub fn task_list(state: State<'_, AppState>, filter: Option<TaskFilter>) -> AppResult<Vec<Task>> {
    let filter = filter.unwrap_or_default();
    state.db.with_conn(|conn| repo::list(conn, &filter))
}

#[tauri::command]
pub fn task_page(state: State<'_, AppState>, filter: Option<TaskFilter>) -> AppResult<TaskPage> {
    state
        .db
        .with_conn(|conn| repo::page(conn, &filter.unwrap_or_default()))
}

#[tauri::command]
pub fn task_set_archived(
    state: State<'_, AppState>,
    id: String,
    archived: bool,
) -> AppResult<Task> {
    state
        .db
        .with_conn(|conn| repo::set_archived(conn, &id, archived))
}

#[tauri::command]
pub fn task_move(state: State<'_, AppState>, input: TaskMoveInput) -> AppResult<()> {
    state.db.with_conn(|conn| repo::move_task(conn, &input))
}

#[tauri::command]
pub fn task_get(state: State<'_, AppState>, id: String) -> AppResult<TaskDetail> {
    state.db.with_conn(|conn| {
        let task = repo::get_required(conn, &id)?;
        let items = update_repo::list(conn, &id)?;
        Ok(TaskDetail {
            task,
            updates: items,
        })
    })
}

#[tauri::command]
pub fn task_create(state: State<'_, AppState>, input: CreateTaskInput) -> AppResult<Task> {
    state.db.with_conn(|conn| repo::create(conn, &input))
}

#[tauri::command]
pub fn task_update(state: State<'_, AppState>, id: String, patch: TaskPatch) -> AppResult<Task> {
    state.db.with_conn(|conn| repo::update(conn, &id, &patch))
}

#[tauri::command]
pub fn task_set_completed(
    state: State<'_, AppState>,
    id: String,
    completed: bool,
) -> AppResult<Task> {
    state
        .db
        .with_conn(|conn| repo::set_completed(conn, &id, completed))
}

#[tauri::command]
pub fn task_set_hidden(state: State<'_, AppState>, id: String, hidden: bool) -> AppResult<Task> {
    state
        .db
        .with_conn(|conn| repo::set_hidden(conn, &id, hidden))
}

#[tauri::command]
pub fn task_set_priority(state: State<'_, AppState>, id: String, priority: i64) -> AppResult<Task> {
    state.db.with_conn(|conn| {
        repo::update(
            conn,
            &id,
            &TaskPatch {
                priority: Some(priority),
                ..Default::default()
            },
        )
    })
}

#[tauri::command]
pub fn task_trash(state: State<'_, AppState>, id: String) -> AppResult<()> {
    state.db.with_conn(|conn| repo::trash(conn, &id))
}

#[tauri::command]
pub fn task_restore(state: State<'_, AppState>, id: String) -> AppResult<Task> {
    state.db.with_conn(|conn| repo::restore(conn, &id))
}

#[tauri::command]
pub fn task_delete_permanent(state: State<'_, AppState>, id: String) -> AppResult<()> {
    state.db.with_conn(|conn| repo::delete_permanent(conn, &id))
}

#[tauri::command]
pub fn task_duplicate(state: State<'_, AppState>, id: String) -> AppResult<Task> {
    state.db.with_conn(|conn| repo::duplicate(conn, &id))
}

#[tauri::command]
pub fn task_reorder(state: State<'_, AppState>, ordered_ids: Vec<String>) -> AppResult<()> {
    state.db.with_conn(|conn| repo::reorder(conn, &ordered_ids))
}

#[tauri::command]
pub fn task_counts(state: State<'_, AppState>) -> AppResult<Counts> {
    state.db.with_conn(|conn| repo::counts(conn))
}

#[tauri::command]
pub fn update_list(state: State<'_, AppState>, task_id: String) -> AppResult<Vec<TaskUpdate>> {
    state.db.with_conn(|conn| update_repo::list(conn, &task_id))
}

#[tauri::command]
pub fn update_add(
    state: State<'_, AppState>,
    task_id: String,
    content: String,
    author: Option<String>,
) -> AppResult<TaskUpdate> {
    state
        .db
        .with_conn(|conn| update_repo::add(conn, &task_id, &content, author.as_deref()))
}

#[tauri::command]
pub fn update_edit(
    state: State<'_, AppState>,
    id: String,
    content: String,
) -> AppResult<TaskUpdate> {
    state
        .db
        .with_conn(|conn| update_repo::edit(conn, &id, &content))
}

#[tauri::command]
pub fn update_delete(state: State<'_, AppState>, id: String) -> AppResult<()> {
    state.db.with_conn(|conn| update_repo::delete(conn, &id))
}
