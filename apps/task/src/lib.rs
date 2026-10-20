//! 定时任务模块（对齐 cool-admin-go modules/task）

pub mod funcs;
pub mod handler;
pub mod model;
pub mod scheduler;
pub mod service;

use sqlx::MySqlPool;

pub use handler::TaskInfoSvc;

/// 构建服务并注册内置函数、引导已启用任务
pub async fn build_services(pool: MySqlPool) -> TaskInfoSvc {
    funcs::register_builtins(pool.clone());
    let svc = service::TaskInfoService::new(pool.clone());
    // 启动引导：status=1 的任务加载进调度器（本地，不走 cluster）
    svc.bootstrap().await;
    handler::TaskInfoSvc::new(svc)
}

const MODULE_NAME: &str = "task";

/// 本模块迁移目录（相对 crate 根）
fn migrations_dir() -> std::path::PathBuf {
    std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("migrations")
}

/// 执行本模块数据库迁移。
/// 版本号全局唯一，见 migrations/ 文件名前缀。
pub async fn migrate(pool: &sqlx::MySqlPool) -> cool_core::CoolResult<()> {
    let migrator = sqlx::migrate::Migrator::new(migrations_dir())
        .await
        .map_err(|e| cool_core::CoolError::Internal(format!("加载迁移失败: {}", e)))?;
    migrator
        .run(pool)
        .await
        .map_err(|e| cool_core::CoolError::Internal(format!("模块迁移失败: {}", e)))?;
    tracing::info!(target: "migrate", module = MODULE_NAME, "migrations done");
    Ok(())
}
