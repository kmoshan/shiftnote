use std::path::PathBuf;

use tauri::State;

use crate::db::now_ms;
use crate::error::{AppError, AppResult};
use crate::models::{AppInfo, BackupInfo, DbStats, RestoreSummary};
use crate::state::AppState;

#[tauri::command]
pub fn app_info(state: State<'_, AppState>) -> AppInfo {
    state.info.clone()
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
    let dest: PathBuf = state
        .db
        .backups_dir()
        .join(format!("shiftnote-backup-{stamp}.db"));

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
pub fn db_restore(state: State<'_, AppState>, path: String) -> AppResult<RestoreSummary> {
    let trimmed = path.trim();
    if trimmed.is_empty() {
        return Err(AppError::validation("请选择要恢复的备份文件"));
    }
    state.db.restore_from(std::path::Path::new(trimmed))
}
