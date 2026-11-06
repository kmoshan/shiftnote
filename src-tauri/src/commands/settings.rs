use std::collections::HashMap;

use tauri::State;

use crate::db::settings as store;
use crate::error::AppResult;
use crate::state::AppState;

#[tauri::command]
pub fn settings_get(state: State<'_, AppState>, key: String) -> AppResult<Option<String>> {
    state.db.with_conn(|conn| store::get(conn, &key))
}

#[tauri::command]
pub fn settings_all(state: State<'_, AppState>) -> AppResult<HashMap<String, String>> {
    state.db.with_conn(|conn| store::all(conn))
}

#[tauri::command]
pub fn settings_set(state: State<'_, AppState>, key: String, value: String) -> AppResult<()> {
    state.db.with_conn(|conn| store::set(conn, &key, &value))
}
