//! cool-admin-rust 核心框架。
//!
//! 对应 cool-admin-go 的 `cool/` 包：错误码、响应约定、日志、鉴权拦截器、
//! 通用 CRUD 引擎（Model / Query / Repo / CrudService）。

pub mod auth;
pub mod cache;
pub mod crud;
pub mod eps;
pub mod error;
pub mod jwt;
pub mod logging;
pub mod model;
pub mod password;
pub mod query;
pub mod repo;
pub mod response;
pub mod satoken;

pub use auth::{AUTH_WHITELIST, AuthLayer, AuthUser};
pub use cache::{CacheStore, get_or_set as cache_get_or_set};
pub use crud::CrudService;
pub use eps::{ApiMeta, register_api, register_module};
pub use error::{CoolError, CoolResult, ResultExt};
pub use jwt::{JwtConfig, TokenResult, issue_tokens, verify as verify_jwt};
pub use logging::{LoggingLayer, init_tracing};
pub use model::{COL_ID, ColumnMeta, FieldMap, Model, base_columns};
pub use query::{EqMap, QueryInput, QueryOp, safe_order_by};
pub use repo::{CrudConfig, CrudRepo, PageResult, eq_map};
pub use response::{CODE_FAIL, CODE_SUCCESS, fail_code, ok_code};
pub use satoken::{SaTokenConfig, login_id_from_request};

/// 运行实例 ID（对齐 Go 版 ProcessFlag）
pub fn instance_id() -> &'static str {
    use std::sync::OnceLock;
    static ID: OnceLock<String> = OnceLock::new();
    ID.get_or_init(|| {
        let ts = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis())
            .unwrap_or(0);
        format!("rs-{ts:x}")
    })
}
