//! 注册微服务到 APISIX（etcd 配置 + 静态节点，Service+Route 架构）。
//!
//! 服务发现（Nacos/DNS/K8s/ConsulKV）代码就绪，需对应 registry 配合。

use cool_apisix::{register_microservice, ApisixConfig, MicroService};

#[tokio::main]
async fn main() {
    let cfg = ApisixConfig::load();
    for (id, pkg, port) in [
        ("base-service", "base.v1", 8001u16),
        ("dict-service", "dict.v1", 8002),
        ("space-service", "space.v1", 8003),
        ("task-service", "task.v1", 8004),
    ] {
        let svc = MicroService::grpc_static(
            &format!("{}-{}", cfg.prefix, id),
            pkg,
            &format!("{}:{}", cfg.upstream_host, port),
        );
        register_microservice(&cfg, &svc).await.expect("register");
    }
    println!("4 services registered (static upstream, etcd provider)");
}
