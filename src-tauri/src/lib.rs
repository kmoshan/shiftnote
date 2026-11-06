//! ShiftNote —— 轻量级交接班与事项跟踪工具。
//!
//! 架构：Vue 3 前端（WebView2）通过 Tauri 命令层访问 Rust 侧的 SQLite 仓储。
//! 业务数据以口令加密文件保存在本地；WebView2 自身诊断遵循其运行时设置。

pub mod commands;
pub mod db;
pub mod error;
pub mod export;
pub mod models;
mod startup;
pub mod state;
pub mod vault;

use std::path::PathBuf;

use tauri::{Manager, WebviewWindowBuilder};

use crate::db::Db;
use crate::models::AppInfo;
use crate::state::AppState;

/// 环境变量 > exe 同名 .portable 标记 > Windows 标准 AppData 目录。
fn resolve_data_directories(
    app: &tauri::AppHandle,
) -> Result<(PathBuf, PathBuf), Box<dyn std::error::Error>> {
    let custom = std::env::var("SHIFTNOTE_DATA_DIR").ok();
    if let Some(data_dir) =
        startup::data_dir_override(&std::env::current_exe()?, custom.as_deref())?
    {
        let webview_dir = data_dir.join("webview2");
        return Ok((data_dir, webview_dir));
    }
    Ok((app.path().app_data_dir()?, app.path().app_local_data_dir()?))
}

fn initialize_app(
    app: &mut tauri::App,
    windows: &[tauri::utils::config::WindowConfig],
) -> Result<(), Box<dyn std::error::Error>> {
    let (data_dir, webview_dir) = resolve_data_directories(app.handle())?;
    std::fs::create_dir_all(&data_dir)
        .map_err(|error| format!("无法创建数据目录 {}：{error}", data_dir.display()))?;

    std::fs::create_dir_all(&webview_dir).map_err(|error| {
        format!(
            "无法创建 WebView2 数据目录 {}：{error}",
            webview_dir.display()
        )
    })?;

    let db_path = data_dir.join("shiftnote.db");
    let db = Db::new_vault(&db_path)
        .map_err(|error| format!("无法打开数据库 {}：{error}", db_path.display()))?;
    let info = AppInfo {
        version: app.package_info().version.to_string(),
        data_dir: data_dir.display().to_string(),
        db_path: db_path.display().to_string(),
        schema_version: 0,
        recovered_backup: None,
        encrypted: false,
        plaintext_backup_count: 0,
    };
    app.manage(AppState {
        db: std::sync::Arc::new(db),
        info,
    });

    for window in windows {
        WebviewWindowBuilder::from_config(app.handle(), window)?
            .data_directory(webview_dir.clone())
            .build()
            .map_err(|error| {
                format!(
                    "无法初始化 WebView2 窗口（数据目录 {}）：{error}",
                    webview_dir.display()
                )
            })?;
    }
    Ok(())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    startup::install_panic_hook();
    let mut context = tauri::generate_context!();
    let windows: Vec<_> = context
        .config()
        .app
        .windows
        .iter()
        .filter(|window| window.create)
        .cloned()
        .collect();
    // 先初始化数据库，再显式创建窗口；目录错误由应用显示并记录，避免框架 panic。
    for window in &mut context.config_mut().app.windows {
        window.create = false;
    }

    let result = tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .setup(move |app| {
            if let Err(error) = initialize_app(app, &windows) {
                let data_dir = resolve_data_directories(app.handle())
                    .ok()
                    .map(|(data_dir, _)| data_dir);
                startup::report_failure(&error.to_string(), data_dir.as_deref());
                app.handle().exit(1);
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::app::app_info,
            commands::app::db_stats,
            commands::app::db_backup,
            commands::app::db_restore,
            commands::app::db_encrypt_legacy_backups,
            commands::vault::vault_status,
            commands::vault::vault_unlock,
            commands::settings::settings_get,
            commands::settings::settings_all,
            commands::settings::settings_set,
            commands::tasks::task_list,
            commands::tasks::task_page,
            commands::tasks::task_move,
            commands::tasks::task_set_archived,
            commands::batch::task_batch,
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
            commands::handovers::handover_list,
            commands::handovers::handover_get,
            commands::handovers::handover_save,
            commands::handovers::handover_candidates,
            commands::handovers::handover_finalize,
            commands::handovers::handover_duplicate,
            commands::handovers::handover_delete,
            commands::handovers::handover_render,
            commands::handovers::handover_export,
        ])
        .run(context);
    if let Err(error) = result {
        startup::report_failure(&error.to_string(), None);
        std::process::exit(1);
    }
}
