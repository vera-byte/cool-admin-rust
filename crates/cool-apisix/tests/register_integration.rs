//! 注册接口集成测试（内嵌 mock Admin API）。

use std::sync::Arc;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

async fn mock_apisix(captured: Arc<tokio::sync::Mutex<Vec<(String, String)>>>) -> u16 {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    tokio::spawn(async move {
        loop {
            let (mut sock, _) = match listener.accept().await {
                Ok(v) => v,
                Err(_) => break,
            };
            let captured = captured.clone();
            tokio::spawn(async move {
                let mut buf = vec![0u8; 8192];
                let n = sock.read(&mut buf).await.unwrap_or(0);
                let req = String::from_utf8_lossy(&buf[..n]).to_string();
                let line = req.lines().next().unwrap_or("").to_string();
                let body = req.split("\r\n\r\n").nth(1).unwrap_or("").to_string();
                captured.lock().await.push((line, body));
                let resp = b"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: 2\r\n\r\n{}";
                let _ = sock.write_all(resp).await;
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
        upstream_host: "127.0.0.1".into(),
        scheme: "grpc".into(),
        prefix: "cool".into(),
    }
}

#[tokio::test]
async fn auto_register_creates_service_and_route() {
    let captured = Arc::new(tokio::sync::Mutex::new(Vec::new()));
    let port = mock_apisix(captured.clone()).await;
    let c = cfg(port);

    cool_apisix::auto_register(&c, "base-service", "base.v1", 8001)
        .await
        .expect("register ok");

    let log = captured.lock().await;
    // 2 个 PUT：Service + Route
    assert_eq!(log.len(), 2, "service + route: {:?}", log.iter().map(|(l, _)| l).collect::<Vec<_>>());
    let has_service = log.iter().any(|(l, _)| l.contains("PUT") && l.contains("/services/"));
    let has_route = log.iter().any(|(l, _)| l.contains("PUT") && l.contains("/routes/"));
    assert!(has_service);
    assert!(has_route);

    // Service 体含静态 nodes（兼容模式）
    let service_body = &log
        .iter()
        .find(|(l, _)| l.contains("/services/"))
        .unwrap()
        .1;
    assert!(service_body.contains("127.0.0.1:8001"));
    assert!(service_body.contains("grpc"));
}

#[tokio::test]
async fn auto_register_web_adds_grpc_web_plugin() {
    let captured = Arc::new(tokio::sync::Mutex::new(Vec::new()));
    let port = mock_apisix(captured.clone()).await;
    let c = cfg(port);

    cool_apisix::auto_register_web(&c, "base-service", "base.v1", 8001)
        .await
        .expect("register ok");

    let log = captured.lock().await;
    let route_body = &log
        .iter()
        .find(|(l, _)| l.contains("/routes/") && l.contains("-web"))
        .unwrap()
        .1;
    assert!(route_body.contains("grpc-web"));
    assert!(route_body.contains("/web/(.*)"));
}

#[tokio::test]
async fn disabled_skips_registration() {
    let c = cool_apisix::ApisixConfig { enabled: false, ..Default::default() };
    let inst = cool_apisix::ServiceInstance {
        service_id: "s".into(),
        package: "p.v1".into(),
        addr: "1:1".into(),
    };
    assert!(cool_apisix::register_instance(&c, &inst).await.is_ok());
}
