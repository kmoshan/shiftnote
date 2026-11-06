//! ShiftNote —— 轻量级交接班与事项跟踪工具。
//!
//! 架构：Vue 3 前端（WebView2）通过 Tauri 命令层访问 Rust 侧的 SQLite 仓储。
//! 所有业务数据保存在本地，默认不上传任何内容。

pub mod commands;
pub mod db;
pub mod error;
pub mod models;
pub mod state;

use std::path::PathBuf;

use tauri::Manager;

use crate::db::Db;
use crate::models::AppInfo;
use crate::state::AppState;

/// 数据目录优先级：环境变量（便于测试/便携） > Windows 标准 AppData 目录。
fn resolve_data_dir(app: &tauri::AppHandle) -> Result<PathBuf, Box<dyn std::error::Error>> {
    if let Ok(custom) = std::env::var("SHIFTNOTE_DATA_DIR") {
        let trimmed = custom.trim();
        if !trimmed.is_empty() {
            return Ok(PathBuf::from(trimmed));
        }
    }
    Ok(app.path().app_data_dir()?)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            let data_dir = resolve_data_dir(app.handle())?;
            std::fs::create_dir_all(&data_dir)?;

            let db_path = data_dir.join("shiftnote.db");
            let outcome = Db::open_or_recover(&db_path)?;

            let info = AppInfo {
                version: app.package_info().version.to_string(),
                data_dir: data_dir.display().to_string(),
                db_path: db_path.display().to_string(),
                schema_version: outcome.db.schema_version(),
                recovered_backup: outcome
                    .recovered_backup
                    .as_ref()
                    .map(|p| p.display().to_string()),
            };

            app.manage(AppState {
                db: std::sync::Arc::new(outcome.db),
                info,
            });

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::app::app_info,
            commands::app::db_stats,
            commands::app::db_backup,
            commands::app::db_restore,
            commands::settings::settings_get,
            commands::settings::settings_all,
            commands::settings::settings_set,
            commands::tasks::task_list,
            commands::tasks::task_get,
            commands::tasks::task_create,
            commands::tasks::task_update,
            commands::tasks::task_set_completed,
            commands::tasks::task_set_hidden,
            commands::tasks::task_set_priority,
            commands::tasks::task_trash,
            commands::tasks::task_restore,
            commands::tasks::task_delete_permanent,
            commands::tasks::task_duplicate,
            commands::tasks::task_reorder,
            commands::tasks::task_counts,
            commands::tasks::update_list,
            commands::tasks::update_add,
            commands::tasks::update_edit,
            commands::tasks::update_delete,
        ])
        .run(tauri::generate_context!())
        .expect("ShiftNote 启动失败");
}
