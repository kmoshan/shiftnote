use std::path::PathBuf;

use tauri::State;

use crate::db::now_ms;
use crate::error::{AppError, AppResult};
use crate::models::{AppInfo, BackupInfo, DbStats, EncryptBackupsResult, RestoreSummary};
use crate::state::AppState;

#[tauri::command]
pub fn app_info(state: State<'_, AppState>) -> AppResult<AppInfo> {
    let mut info = state.info.clone();
    info.schema_version = state.db.schema_version();
    info.encrypted = state.db.vault_status()?.configured;
    info.plaintext_backup_count = state.db.plaintext_backup_count()?;
    Ok(info)
}

#[tauri::command]
pub fn db_stats(state: State<'_, AppState>) -> AppResult<DbStats> {
    let size = state.db.file_size();
    let wal = state.db.wal_enabled();
    let (task_count, update_count) = state.db.with_conn(|conn| {
        let tasks: i64 = conn.query_row("SELECT COUNT(*) FROM tasks", [], |row| row.get(0))?;
        let updates: i64 =
            conn.query_row("SELECT COUNT(*) FROM task_updates", [], |row| row.get(0))?;
        Ok((tasks, updates))
    })?;

    Ok(DbStats {
        path: state.info.db_path.clone(),
        size_bytes: size,
        task_count,
        update_count,
        // 恢复备份后 schema 版本可能变化，这里读实时值
        schema_version: state.db.schema_version(),
        wal_enabled: wal,
    })
}

#[tauri::command]
pub fn db_backup(state: State<'_, AppState>) -> AppResult<BackupInfo> {
    let stamp = chrono::Local::now().format("%Y%m%d-%H%M%S").to_string();
    let extension = state.db.backup_extension()?;
    let dest: PathBuf = state.db.backups_dir().join(format!(
        "shiftnote-backup-{stamp}-{}.{extension}",
        uuid::Uuid::new_v4()
    ));

    let size = state.db.backup_to(&dest)?;
    if size == 0 {
        return Err(AppError::Other("备份文件写入失败".into()));
    }

    Ok(BackupInfo {
        path: dest.display().to_string(),
        size_bytes: size,
        created_at: now_ms(),
    })
}

/// 从用户选择的备份文件恢复数据。
///
/// 覆盖前会自动把当前数据快照到 backups/pre-restore-*.db；
/// 备份校验不通过或恢复过程中出错时会自动回滚。
#[tauri::command]
pub async fn db_restore(
    state: State<'_, AppState>,
    path: String,
    backup_password: Option<String>,
) -> AppResult<RestoreSummary> {
    let trimmed = path.trim();
    if trimmed.is_empty() {
        return Err(AppError::validation("请选择要恢复的备份文件"));
    }
    let path = PathBuf::from(trimmed);
    let password = backup_password.map(zeroize::Zeroizing::new);
    let db = state.db.clone();
    tauri::async_runtime::spawn_blocking(move || {
        db.restore_with_password(&path, password.as_ref().map(|value| value.as_str()))
    })
    .await
    .map_err(|_| AppError::Other("恢复任务中断，请重启应用后检查原数据".into()))?
}

#[tauri::command]
pub async fn db_encrypt_legacy_backups(
    state: State<'_, AppState>,
) -> AppResult<EncryptBackupsResult> {
    let db = state.db.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let (encrypted, remaining) = db.encrypt_legacy_backups()?;
        Ok(EncryptBackupsResult {
            encrypted,
            remaining,
        })
    })
    .await
    .map_err(|_| AppError::Other("旧备份加密任务中断，请重试".into()))?
}
