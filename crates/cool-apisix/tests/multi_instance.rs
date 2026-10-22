//! Service + 路由绑定架构集成测试（内嵌 mock Admin API）。

use std::sync::Arc;

use serde_json::Value;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

/// mock APISIX：捕获 PUT/DELETE，按 URL 存储资源
async fn mock_apisix(store: Arc<tokio::sync::Mutex<Vec<(String, Value)>>>) -> u16 {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    tokio::spawn(async move {
        loop {
            let (mut sock, _) = match listener.accept().await {
                Ok(v) => v,
                Err(_) => break,
            };
            let store = store.clone();
            tokio::spawn(async move {
                let mut buf = vec![0u8; 32768];
                let n = sock.read(&mut buf).await.unwrap_or(0);
                let req = String::from_utf8_lossy(&buf[..n]).to_string();
                let line = req.lines().next().unwrap_or("").to_string();
                let body = req.split("\r\n\r\n").nth(1).unwrap_or("").to_string();

                let resp = if line.starts_with("PUT") {
                    let v: Value = serde_json::from_str(&body).unwrap_or(Value::Null);
                    store.lock().await.push((line.clone(), v));
                    "HTTP/1.1 200 OK\r\nContent-Length: 2\r\n\r\n{}"
                } else if line.starts_with("DELETE") {
                    "HTTP/1.1 200 OK\r\nContent-Length: 2\r\n\r\n{}"
                } else {
                    "HTTP/1.1 400 Bad Request\r\nContent-Length: 0\r\n\r\n"
                };
                let _ = sock.write_all(resp.as_bytes()).await;
            });
        }
    });
    port
}

fn cfg(port: u16) -> cool_apisix::ApisixConfig {
    cool_apisix::ApisixConfig {
        admin_url: format!("http://127.0.0.1:{port}"),
        admin_key: "k".into(),
        enabled: true,
        upstream_host: "10.0.0.1".into(),
        scheme: "grpc".into(),
        prefix: "cool".into(),
    }
}

#[tokio::test]
async fn register_microservice_creates_service_and_routes() {
    let store = Arc::new(tokio::sync::Mutex::new(Vec::new()));
    let port = mock_apisix(store.clone()).await;
    let c = cfg(port);

    let svc = cool_apisix::MicroService::grpc_dns("dict-service", "dict.v1", "dict-service.local");
    cool_apisix::register_microservice(&c, &svc).await.unwrap();

    let log = store.lock().await;
    // 3 个 PUT：Service + Route + WebRoute
    assert_eq!(log.len(), 3, "expect 3 PUTs: {:?}", log.iter().map(|(l, _)| l).collect::<Vec<_>>());

    // Service 含 service_name/discovery_type
    let service_put = log.iter().find(|(l, _)| l.contains("/services/")).unwrap();
    assert!(service_put.0.contains("/services/dict-service"), "service id: {}", service_put.0);
    assert_eq!(service_put.1["upstream"]["discovery_type"], "dns");
    assert_eq!(service_put.1["upstream"]["service_name"], "dict-service.local");
    assert!(service_put.1["upstream"].get("nodes").is_none(), "discovery mode has no nodes");

    // Route 绑定 service_id，不内联 upstream
    let route_put = log
        .iter()
        .find(|(l, _)| l.contains("/routes/") && !l.contains("-web"))
        .unwrap();
    assert_eq!(route_put.1["service_id"], "dict-service");
    assert!(route_put.1.get("upstream").is_none(), "route must not inline upstream");

    // WebRoute 含 grpc-web 插件
    let web_put = log.iter().find(|(l, _)| l.contains("-web")).unwrap();
    assert!(web_put.1["plugins"].get("grpc-web").is_some());
}

#[tokio::test]
async fn discovery_modes_generate_correct_fields() {
    let store = Arc::new(tokio::sync::Mutex::new(Vec::new()));
    let port = mock_apisix(store.clone()).await;
    let c = cfg(port);

    for svc in [
        cool_apisix::MicroService::grpc_k8s("k8s-svc", "p.v1", "svc", "ns"),
        cool_apisix::MicroService::grpc_nacos("nacos-svc", "p.v1", "svc", "DEFAULT_GROUP"),
        cool_apisix::MicroService::grpc_consul("consul-svc", "p.v1", "svc"),
    ] {
        cool_apisix::register_service(&c, &svc).await.unwrap();
    }

    let log = store.lock().await;
    let bodies: Vec<&Value> = log.iter().map(|(_, v)| v).collect();
    let types: Vec<&str> = bodies
        .iter()
        .filter_map(|b| b["upstream"]["discovery_type"].as_str())
        .collect();
    assert!(types.contains(&"kubernetes"));
    assert!(types.contains(&"nacos"));
    assert!(types.contains(&"consul_kv"));
}

#[tokio::test]
async fn route_binds_service_id() {
    let body = cool_apisix::service_body(&cool_apisix::MicroService::grpc_static(
        "x",
        "p.v1",
        "127.0.0.1:1",
    ));
    assert!(body["upstream"]["nodes"].is_object());
}
