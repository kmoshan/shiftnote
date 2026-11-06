use std::sync::Arc;

use crate::db::Db;
use crate::models::AppInfo;

/// 应用全局状态：数据库句柄 + 启动时确定的路径信息。
pub struct AppState {
    pub db: Arc<Db>,
    pub info: AppInfo,
}

impl AppState {
    pub fn data_dir(&self) -> std::path::PathBuf {
        std::path::PathBuf::from(&self.info.data_dir)
    }
}
