//! APISIX 网关集成：微服务 Service + 路由 + 服务发现。
//!
//! **按需引入**：只有依赖 `cool-apisix` 的微服务才注册。
//!
//! ## 架构
//!
//! ```text
//! APISIX Service（每微服务一个）
//!   ├── upstream：service_name + discovery_type（DNS/K8s/Nacos/ConsulKV/Eureka）
//!   ├── plugins：proxy-rewrite
//!   └── 路由 ×2：原生 gRPC + gRPC-Web，绑定 service_id
//! ```
//!
//! ## 用法
//!
//! ```rust,ignore
//! // 服务发现模式（推荐）
//! let svc = MicroService::grpc_k8s("dict-service", "dict.v1", "dict-service", "default");
//! cool_apisix::register_microservice(&cfg, &svc).await?;
//!
//! // 或 DNS / Nacos / ConsulKV
//! let svc = MicroService::grpc_dns("dict-service", "dict.v1", "dict-service.local");
//! let svc = MicroService::grpc_nacos("dict-service", "dict.v1", "dict-service", "DEFAULT_GROUP");
//! let svc = MicroService::grpc_consul("dict-service", "dict.v1", "dict-service");
//! ```

pub mod config;
pub mod register;

pub use config::{ApisixConfig, ApisixSection};
pub use register::{
    build_discovery_service, build_static_service,
    auto_register, auto_register_web, bind_route, bind_route_web, deregister_instance,
    deregister_instance_web, deregister_microservice, deregister_route, deregister_service,
    register_instance, register_instance_web, register_microservice, register_routes,
    register_routes_web, register_service, service_body, unbind_route, MicroService,
    ServiceDiscovery, ServiceInstance, ServiceRoute,
};
