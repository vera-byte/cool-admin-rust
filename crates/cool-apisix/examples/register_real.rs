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
        for port in [8002u16, 8012, 8022] {
            let inst = cool_apisix::ServiceInstance {
                service_id: "dict-service".into(),
                package: "dict.v1".into(),
                addr: format!("host.docker.internal:{port}"),
            };
            cool_apisix::register_instance(&cfg, &inst).await.unwrap();
        }
        println!("3 instances registered (with health check)");
    });
}
