//! 对真实 APISIX 注册路由（需本地 APISIX 运行）
//!
//! cargo run -p cool-apisix --example register_real

use cool_apisix::{auto_register, ApisixConfig};

#[tokio::main]
async fn main() {
    let cfg = ApisixConfig::load();
    println!("admin_url = {}", cfg.admin_url);
    auto_register(&cfg, "base-service", "base.v1", 8001)
        .await
        .expect("register base-service");
    auto_register(&cfg, "space-service", "space.v1", 8003)
        .await
        .expect("register space-service");
    println!("routes registered");
}
