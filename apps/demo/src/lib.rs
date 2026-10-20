//! demo 演示模块（对齐 cool-admin-go modules/demo）

pub mod handler;
pub mod model;
pub mod service;

pub use handler::DemoSampleSvc;
pub use model::DemoSample;
pub use service::{DemoSampleService, new_demo_sample_service};

use sqlx::MySqlPool;

/// 注册/构建模块服务
pub fn build_services(pool: MySqlPool) -> DemoSampleSvc {
    let svc = new_demo_sample_service(pool);
    DemoSampleSvc::new(svc)
}

const MODULE_NAME: &str = "demo";

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
