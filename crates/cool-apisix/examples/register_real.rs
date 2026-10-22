//! 对真实 APISIX 注册路由（原生 gRPC + gRPC-Web）
//!
//! cargo run -p cool-apisix --example register_real

use cool_apisix::{auto_register, auto_register_web, ApisixConfig};

#[tokio::main]
async fn main() {
    let cfg = ApisixConfig::load();
    let host = cfg.upstream_host.clone();
    println!("admin_url = {}, upstream_host = {}", cfg.admin_url, host);

    for (svc, pkg, port) in [
        ("base-service", "base.v1", 8001),
        ("dict-service", "dict.v1", 8002),
        ("space-service", "space.v1", 8003),
        ("task-service", "task.v1", 8004),
    ] {
        auto_register(&cfg, svc, pkg, port).await.expect("gRPC route");
        auto_register_web(&cfg, svc, pkg, port).await.expect("gRPC-Web route");
    }
    println!("routes registered (gRPC + gRPC-Web)");
}
