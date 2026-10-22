//! 注册 4 个微服务到 APISIX（Service + 路由绑定架构）。
//!
//! 本地测试用静态节点；生产改用 MicroService::grpc_k8s / grpc_dns 等服务发现。

use cool_apisix::{register_microservice, ApisixConfig, MicroService, ServiceDiscovery};

#[tokio::main]
async fn main() {
    let cfg = ApisixConfig::load();
    println!("admin_url = {}", cfg.admin_url);

    for (id, pkg, port) in [
        ("base-service", "base.v1", 8001u16),
        ("dict-service", "dict.v1", 8002),
        ("space-service", "space.v1", 8003),
        ("task-service", "task.v1", 8004),
    ] {
        // 本地测试：静态节点（含 gRPC-Web 同端口）
        let mut nodes = std::collections::BTreeMap::new();
        nodes.insert(format!("{}:{}", cfg.upstream_host, port), 1u32);

        let svc = MicroService {
            id: format!("{}-{}", cfg.prefix, id),
            name: format!("{id} ({pkg})"),
            package: pkg.to_string(),
            scheme: "grpc".into(),
            discovery: ServiceDiscovery::Static { nodes },
            priority: 10,
        };
        register_microservice(&cfg, &svc).await.expect("register");
    }
    println!("4 microservices registered (Service + Routes)");
}
