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
