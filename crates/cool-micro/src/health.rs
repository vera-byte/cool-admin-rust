//! HTTP 健康检查端点（供 APISIX / K8s 主动探测）。
//!
//! 监听 `grpc_port + 10000`，返回 200。
//! 混沌测试：`COOL_TEST_RESPONSE_DELAY_MS` 同样延迟此端点。

use std::time::Duration;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

/// 启动健康检查 HTTP 服务，返回实际端口
pub async fn spawn_health_server(listen_port: u16) -> u16 {
    let delay_ms: u64 = std::env::var("COOL_TEST_RESPONSE_DELAY_MS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(0);

    let listener = match TcpListener::bind(("0.0.0.0", listen_port)).await {
        Ok(l) => l,
        Err(e) => {
            tracing::warn!("health server bind failed on {listen_port}: {e}");
            return 0;
        }
    };
    let port = listener.local_addr().map(|a| a.port()).unwrap_or(listen_port);
    tracing::info!(port, delay_ms, "health endpoint listening (GET /health)");

    tokio::spawn(async move {
        loop {
            let Ok((mut sock, _)) = listener.accept().await else {
                break;
            };
            let delay_ms = delay_ms;
            tokio::spawn(async move {
                let mut buf = vec![0u8; 1024];
                let _ = sock.read(&mut buf).await;
                if delay_ms > 0 {
                    tokio::time::sleep(Duration::from_millis(delay_ms)).await;
                }
                let body = r#"{"status":"ok"}"#;
                let resp = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                    body.len(),
                    body
                );
                let _ = sock.write_all(resp.as_bytes()).await;
            });
        }
    });
    port
}
