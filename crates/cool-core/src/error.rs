//! 统一错误类型与错误码（对齐 cool-admin-go 业务语义）。
//!
//! 传输层用 tonic `Status`；业务层错误码见 `CODE_SUCCESS` / `CODE_FAIL`。

use tonic::Status;

pub use cool_proto::{CODE_FAIL, CODE_SUCCESS};

/// 框架统一错误
#[derive(Debug, thiserror::Error)]
pub enum CoolError {
    /// 业务失败（code=1001）
    #[error("{0}")]
    Biz(String),
    /// 参数不合法
    #[error("{0}")]
    InvalidArgument(String),
    /// 未登录 / token 失效
    #[error("{0}")]
    Unauthenticated(String),
    /// 无权限
    #[error("{0}")]
    PermissionDenied(String),
    /// 资源不存在
    #[error("{0}")]
    NotFound(String),
    /// 服务内部错误
    #[error("{0}")]
    Internal(String),
    /// 底层错误透传
    #[error(transparent)]
    Other(#[from] anyhow::Error),
}

impl CoolError {
    pub fn biz(msg: impl Into<String>) -> Self {
        Self::Biz(msg.into())
    }

    pub fn internal(msg: impl Into<String>) -> Self {
        Self::Internal(msg.into())
    }

    pub fn unauthenticated(msg: impl Into<String>) -> Self {
        Self::Unauthenticated(msg.into())
    }

    pub fn permission_denied(msg: impl Into<String>) -> Self {
        Self::PermissionDenied(msg.into())
    }

    pub fn invalid_argument(msg: impl Into<String>) -> Self {
        Self::InvalidArgument(msg.into())
    }

    /// 业务错误码（成功 1000 / 失败 1001）
    pub fn code(&self) -> i32 {
        CODE_FAIL
    }

    /// 转为传输层 Status。
    ///
    /// 注意（决策 D1）：纯业务失败（`Biz`）优先由 handler 落到响应体 `code=1001`，
    /// 这里仅在必须用 `?` 上传输错误时使用；`Biz` 映射为 `Unknown`。
    pub fn into_status(self) -> Status {
        let msg = self.to_string();
        match self {
            Self::Biz(_) => Status::unknown(msg),
            Self::InvalidArgument(_) => Status::invalid_argument(msg),
            Self::Unauthenticated(_) => Status::unauthenticated(msg),
            Self::PermissionDenied(_) => Status::permission_denied(msg),
            Self::NotFound(_) => Status::not_found(msg),
            Self::Internal(_) | Self::Other(_) => Status::internal(msg),
        }
    }

    /// 业务错误 → 响应体 code/message（1001 + message）
    pub fn into_biz_header(self) -> (i32, String) {
        (CODE_FAIL, self.to_string())
    }
}

impl From<CoolError> for Status {
    fn from(e: CoolError) -> Self {
        e.into_status()
    }
}

pub type CoolResult<T> = Result<T, CoolError>;

/// 把任意错误包装为内部错误
pub trait ResultExt<T> {
    fn internal(self) -> CoolResult<T>;
}

impl<T, E: std::fmt::Display> ResultExt<T> for Result<T, E> {
    fn internal(self) -> CoolResult<T> {
        self.map_err(|e| CoolError::Internal(e.to_string()))
    }
}
