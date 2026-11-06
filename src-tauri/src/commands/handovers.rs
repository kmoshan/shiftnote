use std::path::Path;
use tauri::State;

use crate::db::handovers as repo;
use crate::error::AppResult;
use crate::models::*;
use crate::state::AppState;

#[tauri::command]
pub fn handover_list(
    state: State<'_, AppState>,
    filter: Option<HandoverFilter>,
) -> AppResult<HandoverPage> {
    state
        .db
        .with_conn(|conn| repo::list(conn, &filter.unwrap_or_default()))
}

#[tauri::command]
pub fn handover_get(state: State<'_, AppState>, id: String) -> AppResult<HandoverDetail> {
    state.db.with_conn(|conn| repo::get(conn, &id))
}

#[tauri::command]
pub fn handover_save(
    state: State<'_, AppState>,
    input: HandoverSaveInput,
) -> AppResult<HandoverDetail> {
    state.db.with_conn(|conn| repo::save(conn, &input))
}

#[tauri::command]
pub fn handover_candidates(
    state: State<'_, AppState>,
    filter: HandoverCandidateFilter,
) -> AppResult<TaskPage> {
    state.db.with_conn(|conn| repo::candidates(conn, &filter))
}

#[tauri::command]
pub fn handover_finalize(state: State<'_, AppState>, id: String) -> AppResult<HandoverDetail> {
    state.db.with_conn(|conn| repo::finalize(conn, &id))
}

#[tauri::command]
pub fn handover_duplicate(state: State<'_, AppState>, id: String) -> AppResult<HandoverDetail> {
    state.db.with_conn(|conn| repo::duplicate(conn, &id))
}

#[tauri::command]
pub fn handover_delete(state: State<'_, AppState>, id: String) -> AppResult<()> {
    state.db.with_conn(|conn| repo::delete(conn, &id))
}

#[tauri::command]
pub fn handover_render(
    state: State<'_, AppState>,
    id: String,
    format: String,
) -> AppResult<String> {
    let detail = state.db.with_conn(|conn| repo::get(conn, &id))?;
    crate::export::render(&detail, &format)
}

#[tauri::command]
pub fn handover_export(
    state: State<'_, AppState>,
    id: String,
    format: String,
    path: String,
    overwrite: bool,
) -> AppResult<HandoverExportResult> {
    let detail = state.db.with_conn(|conn| repo::get(conn, &id))?;
    crate::export::export(&detail, &format, Path::new(&path), overwrite)
}
