//! APISIX 网关集成：微服务路由自动注册。
//!
//! **按需引入**：只有依赖 `cool-apisix` 的微服务才会向 APISIX 注册路由。
//!
//! ## 用法
//!
//! ```rust,ignore
//! // 服务启动时自动注册
//! cool_apisix::auto_register(&cfg, "base-service", "base.v1", 8001).await?;
//! ```
//!
//! ## APISIX Admin API
//!
//! - 默认地址：`http://127.0.0.1:9180/apisix/admin`
//! - 认证头：`X-API-KEY: {admin_key}`
//! - 参考：https://apisix.apache.org/zh/docs/apisix/admin-api/

pub mod config;
pub mod register;

pub use config::{ApisixConfig, ApisixSection};
pub use register::{
    auto_register, auto_register_web, deregister_instance, deregister_instance_web,
    deregister_route, register_instance, register_instance_web, register_routes,
    register_routes_web, ServiceInstance, ServiceRoute,
};
