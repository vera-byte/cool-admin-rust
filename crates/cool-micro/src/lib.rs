//! 微服务公共脚手架。
//!
//! 每个业务模块（base/dict/space/task）作为**独立 gRPC 微服务**启动，
//! 通过 sa-token **分布式会话**共享登录态与服务凭证。

pub mod auth;
pub mod chaos;
pub mod delay;
pub mod health;
pub mod config;
pub mod session;

pub use chaos::spawn_resource_pressure;
pub use delay::DelayLayer;
pub use health::spawn_health_server;
pub use auth::{ServiceAuthLayer, ServiceAuthMiddleware, SERVICE_HEADER_ID, SERVICE_HEADER_SECRET};
pub use config::MicroConfig;
pub use session::{init_distributed, MicroSession, ServiceIdentity};

/// 统一启动日志初始化
pub fn init_tracing(default_level: &str) {
    let filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new(default_level));
    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_target(true)
        .with_level(true)
        .init();
}
