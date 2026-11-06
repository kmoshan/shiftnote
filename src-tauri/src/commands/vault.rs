use tauri::State;
use zeroize::Zeroizing;

use crate::error::{AppError, AppResult};
use crate::state::AppState;
use crate::vault::VaultStatus;

#[tauri::command]
pub fn vault_status(state: State<'_, AppState>) -> AppResult<VaultStatus> {
    state.db.vault_status()
}

#[tauri::command]
pub async fn vault_unlock(
    state: State<'_, AppState>,
    password: String,
    confirm_password: Option<String>,
    migrate_legacy: Option<bool>,
) -> AppResult<()> {
    // No password is retained in app state or included in diagnostics.
    let password = Zeroizing::new(password);
    let confirmation = confirm_password.map(Zeroizing::new);
    let db = state.db.clone();
    tauri::async_runtime::spawn_blocking(move || {
        db.unlock_vault(
            &password,
            confirmation.as_ref().map(|value| value.as_str()),
            migrate_legacy.unwrap_or(false),
        )
    })
    .await
    .map_err(|_| AppError::Other("解锁任务中断，请重试或重启应用".into()))?
}

#[tauri::command]
pub async fn vault_skip(state: State<'_, AppState>) -> AppResult<()> {
    let db = state.db.clone();
    tauri::async_runtime::spawn_blocking(move || db.continue_without_password())
        .await
        .map_err(|_| AppError::Other("打开数据任务中断，请重试".into()))?
}

#[tauri::command]
pub async fn vault_enable_encryption(
    state: State<'_, AppState>,
    password: String,
    confirm_password: String,
) -> AppResult<()> {
    let password = Zeroizing::new(password);
    let confirmation = Zeroizing::new(confirm_password);
    let db = state.db.clone();
    tauri::async_runtime::spawn_blocking(move || {
        db.enable_encryption(&password, Some(&confirmation))
    })
    .await
    .map_err(|_| AppError::Other("启用加密任务中断，请重试".into()))?
}
