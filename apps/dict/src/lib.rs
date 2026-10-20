//! 字典模块（对齐 cool-admin-go modules/dict）

pub mod handler;
pub mod model;
pub mod service;

use sqlx::MySqlPool;

pub use handler::{DictInfoSvc, DictTypeSvc};

pub fn build_services(pool: MySqlPool) -> (DictTypeSvc, DictInfoSvc) {
    (DictTypeSvc::new(pool.clone()), DictInfoSvc::new(pool))
}

const MODULE_NAME: &str = "dict";

/// 本模块迁移目录（相对 crate 根）
fn migrations_dir() -> std::path::PathBuf {
    std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("migrations")
}

/// 执行本模块数据库迁移。
/// 版本号全局唯一，见 migrations/ 文件名前缀。
pub async fn migrate(pool: &sqlx::MySqlPool) -> cool_core::CoolResult<()> {
    let mut migrator = sqlx::migrate::Migrator::new(migrations_dir())
        .await
        .map_err(|e| cool_core::CoolError::Internal(format!("加载迁移失败: {}", e)))?;
    // 允许 _sqlx_migrations 中存在其他模块的迁移记录（多 migrator 共存）
    migrator.set_ignore_missing(true);
    migrator
        .run(pool)
        .await
        .map_err(|e| cool_core::CoolError::Internal(format!("模块迁移失败: {}", e)))?;
    tracing::info!(target: "migrate", module = MODULE_NAME, "migrations done");
    Ok(())
}
