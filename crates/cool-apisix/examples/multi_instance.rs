//! 多实例注册演示（模拟同一服务 3 个实例）
//!
//! cargo run -p cool-apisix --example multi_instance

use cool_apisix::{deregister_instance, register_instance, ApisixConfig, ServiceInstance};

#[tokio::main]
async fn main() {
    let cfg = ApisixConfig::load();
    println!("admin_url = {}", cfg.admin_url);

    let mk = |port: u16| ServiceInstance {
        service_id: "base-service".into(),
        package: "base.v1".into(),
        addr: format!("{}:{}", cfg.upstream_host, port),
    };

    // 注册 3 个实例
    for port in [8001, 8011, 8021] {
        register_instance(&cfg, &mk(port)).await.expect("register");
    }

    // 摘除中间实例
    deregister_instance(&cfg, &mk(8011)).await.expect("deregister");

    println!("done: 3 instances registered, 8011 removed -> expect 2 nodes");
}
