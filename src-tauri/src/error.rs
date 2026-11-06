use serde::ser::{Serialize, SerializeStruct, Serializer};

/// 应用统一错误类型。
///
/// 所有面向命令层的错误都会被序列化为一个带 `message` 的对象，
/// 前端可以据此展示友好的中文提示，而不是原始的 Rust 调试信息。
#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("数据库操作失败：{0}")]
    Db(#[from] rusqlite::Error),

    #[error("文件操作失败：{0}")]
    Io(#[from] std::io::Error),

    #[error("{0}")]
    Validation(String),

    #[error("{0}")]
    Conflict(String),

    #[error("{message}")]
    Batch {
        message: String,
        invalid_ids: Vec<String>,
    },

    #[error("未找到对应的数据：{0}")]
    NotFound(String),

    #[error("{0}")]
    Other(String),
}

impl AppError {
    pub fn validation(msg: impl Into<String>) -> Self {
        Self::Validation(msg.into())
    }

    pub fn not_found(msg: impl Into<String>) -> Self {
        Self::NotFound(msg.into())
    }

    /// 供前端区分错误类别（用于提示样式与重试策略）。
    pub fn code(&self) -> &'static str {
        match self {
            Self::Db(_) => "db",
            Self::Io(_) => "io",
            Self::Validation(_) => "validation",
            Self::Conflict(_) => "conflict",
            Self::Batch { .. } => "batch_invalid",
            Self::NotFound(_) => "not_found",
            Self::Other(_) => "other",
        }
    }
}

impl Serialize for AppError {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut state = serializer.serialize_struct("AppError", 3)?;
        state.serialize_field("code", self.code())?;
        state.serialize_field("message", &self.to_string())?;
        if let Self::Batch { invalid_ids, .. } = self {
            state.serialize_field("invalidIds", invalid_ids)?;
        }
        state.end()
    }
}

pub type AppResult<T> = Result<T, AppError>;
