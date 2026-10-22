fn main() {
    let rt = tokio::runtime::Runtime::new().unwrap();
    rt.block_on(async {
        let cfg = cool_apisix::ApisixConfig {
            admin_url: "http://127.0.0.1:9180".into(),
            admin_key: "edd1c9f034335f136f87ad84b625c8f1".into(),
            enabled: true,
            upstream_host: "host.docker.internal".into(),
            scheme: "grpc".into(),
            prefix: "cool".into(),
        };
        // 摘除死节点 8021
        let dead = cool_apisix::ServiceInstance {
            service_id: "base-service".into(),
            package: "base.v1".into(),
            addr: "host.docker.internal:8021".into(),
        };
        cool_apisix::deregister_instance(&cfg, &dead).await.unwrap();
        // 确保 8001 在
        let alive = cool_apisix::ServiceInstance {
            service_id: "base-service".into(),
            package: "base.v1".into(),
            addr: "host.docker.internal:8001".into(),
        };
        cool_apisix::register_instance(&cfg, &alive).await.unwrap();
        println!("base route fixed");
    });
}
