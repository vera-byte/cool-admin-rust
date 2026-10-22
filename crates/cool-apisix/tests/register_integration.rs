//! APISIX 路由注册集成测试（内嵌 mock Admin API）。

use std::sync::Arc;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

/// 极简 mock APISIX Admin API：捕获 PUT /routes/:id 的 body
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
                let path = req.lines().next().unwrap_or("").to_string();
                // 取 body（空行之后）
                let body = req.split("\r\n\r\n").nth(1).unwrap_or("").to_string();
                captured.lock().await.push((path, body));

                let resp = b"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: 2\r\n\r\n{}";
                let _ = sock.write_all(resp).await;
            });
        }
    });
    port
}

#[tokio::test]
async fn register_routes_upserts_to_admin_api() {
    let captured = Arc::new(tokio::sync::Mutex::new(Vec::new()));
    let port = mock_apisix(captured.clone()).await;

    let cfg = cool_apisix::ApisixConfig {
        admin_url: format!("http://127.0.0.1:{port}"),
        admin_key: "test-key".into(),
        enabled: true,
        upstream_host: "127.0.0.1".into(),
        scheme: "grpc".into(),
        prefix: "cool".into(),
    };

    let route = cool_apisix::ServiceRoute::grpc("base-service", "base.v1", "127.0.0.1:8001");
    cool_apisix::register_routes(&cfg, &[route])
        .await
        .expect("register ok");

    let log = captured.lock().await;
    assert_eq!(log.len(), 1);
    let (req_line, body) = &log[0];
    assert!(req_line.contains("PUT"), "should PUT, got {req_line}");
    assert!(req_line.contains("/routes/base-service"), "path: {req_line}");
    assert!(body.contains("\"name\""), "body: {body}");
    assert!(body.contains("base.v1"), "uris in body: {body}");
    assert!(body.contains("grpc"), "scheme in body: {body}");
    assert!(body.contains("127.0.0.1:8001"), "upstream in body: {body}");
    assert!(body.contains("cool-apisix"), "label in body: {body}");
}

#[tokio::test]
async fn auto_register_uses_prefix_and_package() {
    let captured = Arc::new(tokio::sync::Mutex::new(Vec::new()));
    let port = mock_apisix(captured.clone()).await;

    let cfg = cool_apisix::ApisixConfig {
        admin_url: format!("http://127.0.0.1:{port}"),
        admin_key: "test-key".into(),
        enabled: true,
        upstream_host: "10.0.0.5".into(),
        scheme: "grpc".into(),
        prefix: "prod".into(),
    };

    cool_apisix::auto_register(&cfg, "dict-service", "dict.v1", 8002)
        .await
        .expect("register ok");

    let log = captured.lock().await;
    let (req_line, body) = &log[0];
    assert!(req_line.contains("/routes/prod-dict-service"), "path: {req_line}");
    assert!(body.contains("/dict.v1.*"), "uris: {body}");
    assert!(body.contains("10.0.0.5:8002"), "upstream: {body}");
}

#[tokio::test]
async fn deregister_route_sends_delete() {
    let captured = Arc::new(tokio::sync::Mutex::new(Vec::new()));
    let port = mock_apisix(captured.clone()).await;

    let cfg = cool_apisix::ApisixConfig {
        admin_url: format!("http://127.0.0.1:{port}"),
        admin_key: "test-key".into(),
        enabled: true,
        ..Default::default()
    };

    cool_apisix::deregister_route(&cfg, "cool-base-service")
        .await
        .expect("deregister ok");

    let log = captured.lock().await;
    assert!(log[0].0.contains("DELETE"));
    assert!(log[0].0.contains("/routes/cool-base-service"));
}
