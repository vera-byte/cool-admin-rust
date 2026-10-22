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
        let inst = cool_apisix::ServiceInstance {
            service_id: "dict-service".into(),
            package: "dict.v1".into(),
            addr: "host.docker.internal:8022".into(),
        };
        let _ = cool_apisix::register_instance(&cfg, &inst).await;
        println!("re-registered :8022");
    });
}
