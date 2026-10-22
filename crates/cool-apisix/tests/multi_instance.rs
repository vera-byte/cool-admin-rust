//! 多实例自动发现集成测试（内嵌 mock Admin API）。

use std::sync::Arc;

use serde_json::Value;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

/// mock APISIX：GET 返回当前路由 nodes，PUT 保存
async fn mock_apisix(store: Arc<tokio::sync::Mutex<Option<Value>>>) -> u16 {
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
                    *store.lock().await = Some(v);
                    "HTTP/1.1 200 OK\r\nContent-Length: 2\r\n\r\n{}"
                } else if line.starts_with("GET") {
                    let guard = store.lock().await;
                    let payload = match guard.as_ref() {
                        Some(v) => serde_json::to_string(v).unwrap_or_default(),
                        None => "{\"error_msg\":\"Route Not Found\"}".to_string(),
                    };
                    let code = if guard.is_some() { "200 OK" } else { "404 Not Found" };
                    let _ = sock
                        .write_all(
                            format!(
                                "HTTP/1.1 {code}\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{}",
                                payload.len(),
                                payload
                            )
                            .as_bytes(),
                        )
                        .await;
                    return;
                } else if line.starts_with("DELETE") {
                    *store.lock().await = None;
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

fn inst(port: u16) -> cool_apisix::ServiceInstance {
    cool_apisix::ServiceInstance {
        service_id: "base-service".into(),
        package: "base.v1".into(),
        addr: format!("10.0.0.1:{port}"),
    }
}

#[tokio::test]
async fn multi_instance_merges_upstream_nodes() {
    let store = Arc::new(tokio::sync::Mutex::new(None));
    let port = mock_apisix(store.clone()).await;
    let c = cfg(port);

    // 实例 1
    cool_apisix::register_instance(&c, &inst(8001)).await.unwrap();
    // 实例 2
    cool_apisix::register_instance(&c, &inst(8011)).await.unwrap();
    // 实例 3
    cool_apisix::register_instance(&c, &inst(8021)).await.unwrap();

    let guard = store.lock().await;
    let nodes = guard.as_ref().unwrap()["upstream"]["nodes"].as_object().unwrap();
    assert_eq!(nodes.len(), 3, "3 instances merged: {:?}", nodes.keys());
    assert!(nodes.contains_key("10.0.0.1:8001"));
    assert!(nodes.contains_key("10.0.0.1:8011"));
    assert!(nodes.contains_key("10.0.0.1:8021"));
}

#[tokio::test]
async fn deregister_removes_only_own_instance() {
    let store = Arc::new(tokio::sync::Mutex::new(None));
    let port = mock_apisix(store.clone()).await;
    let c = cfg(port);

    cool_apisix::register_instance(&c, &inst(8001)).await.unwrap();
    cool_apisix::register_instance(&c, &inst(8011)).await.unwrap();
    cool_apisix::deregister_instance(&c, &inst(8011)).await.unwrap();

    let guard = store.lock().await;
    let nodes = guard.as_ref().unwrap()["upstream"]["nodes"].as_object().unwrap();
    assert_eq!(nodes.len(), 1);
    assert!(nodes.contains_key("10.0.0.1:8001"));
    assert!(!nodes.contains_key("10.0.0.1:8011"), "8011 removed");
}

#[tokio::test]
async fn last_instance_deregister_deletes_route() {
    let store = Arc::new(tokio::sync::Mutex::new(None));
    let port = mock_apisix(store.clone()).await;
    let c = cfg(port);

    cool_apisix::register_instance(&c, &inst(8001)).await.unwrap();
    cool_apisix::deregister_instance(&c, &inst(8001)).await.unwrap();

    let guard = store.lock().await;
    assert!(guard.is_none(), "route deleted when no instances left");
}

#[tokio::test]
async fn re_register_same_instance_is_idempotent() {
    let store = Arc::new(tokio::sync::Mutex::new(None));
    let port = mock_apisix(store.clone()).await;
    let c = cfg(port);

    cool_apisix::register_instance(&c, &inst(8001)).await.unwrap();
    cool_apisix::register_instance(&c, &inst(8001)).await.unwrap();
    cool_apisix::register_instance(&c, &inst(8001)).await.unwrap();

    let guard = store.lock().await;
    let nodes = guard.as_ref().unwrap()["upstream"]["nodes"].as_object().unwrap();
    assert_eq!(nodes.len(), 1, "same instance does not duplicate");
}
