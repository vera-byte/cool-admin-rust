//! APISIX 路由注册实现。
//!
//! 通过 Admin API 创建/更新/删除路由（PUT 幂等 upsert）。

use std::collections::HashMap;

use anyhow::{bail, Context, Result};
use reqwest::Client;
use serde_json::{json, Value};

use crate::config::ApisixConfig;

/// 微服务路由定义
#[derive(Debug, Clone)]
pub struct ServiceRoute {
    /// 路由 ID（APISIX 资源 id，如 "base-service"）
    pub id: String,
    /// 路由名称
    pub name: String,
    /// 匹配 URI 列表（Ant 风格，如 "/base.v1.BaseOpenService/*"）
    pub uris: Vec<String>,
    /// 上游地址 "host:port"
    pub upstream_addr: String,
    /// 上游 scheme：http | grpc
    pub scheme: String,
    /// 匹配优先级
    pub priority: i32,
    /// 标签（用于过滤/分组）
    pub labels: HashMap<String, String>,
}

impl ServiceRoute {
    /// 构造 gRPC 微服务路由（前缀匹配所有方法）
    pub fn grpc(service_id: &str, package: &str, addr: &str) -> Self {
        Self {
            id: service_id.to_string(),
            name: format!("{service_id} routes"),
            uris: vec![format!("/{package}/*")],
            upstream_addr: addr.to_string(),
            scheme: "grpc".into(),
            priority: 10,
            labels: {
                let mut m = HashMap::new();
                m.insert("managed-by".into(), "cool-apisix".into());
                m.insert("service".into(), service_id.to_string());
                m
            },
        }
    }
}

fn route_body(route: &ServiceRoute) -> Value {
    let mut labels = serde_json::Map::new();
    for (k, v) in &route.labels {
        labels.insert(k.clone(), Value::String(v.clone()));
    }

    json!({
        "name": route.name,
        "desc": format!("auto-registered by cool-apisix for {}", route.id),
        // APISIX 3.19 校验：uri 与 uris 二选一
        "uris": route.uris,
        "priority": route.priority,
        "status": 1,
        "labels": Value::Object(labels),
        "upstream": {
            "type": "roundrobin",
            "scheme": route.scheme,
            "nodes": {
                route.upstream_addr.clone(): 1
            },
            "timeout": {
                "connect": 5,
                "send": 30,
                "read": 30
            }
        },
        "plugins": {
            "proxy-rewrite": {
                "scheme": route.scheme
            }
        }
    })
}

/// 批量注册路由（PUT 幂等 upsert）
pub async fn register_routes(cfg: &ApisixConfig, routes: &[ServiceRoute]) -> Result<()> {
    if !cfg.enabled {
        tracing::info!("apisix: disabled, skip route registration");
        return Ok(());
    }

    let client = Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()?;

    for route in routes {
        upsert_route(&client, cfg, route).await?;
    }
    Ok(())
}

async fn upsert_route(client: &Client, cfg: &ApisixConfig, route: &ServiceRoute) -> Result<()> {
    let url = cfg.route_url(&route.id);
    let body = route_body(route);

    let resp = client
        .put(&url)
        .header("X-API-KEY", &cfg.admin_key)
        .header("Content-Type", "application/json")
        .json(&body)
        .send()
        .await
        .with_context(|| format!("连接 APISIX 失败: {}", cfg.admin_url))?;

    let status = resp.status();
    let text = resp.text().await.unwrap_or_default();

    if status.is_success() {
        tracing::info!(
            target: "apisix",
            route_id = %route.id,
            uris = ?route.uris,
            upstream = %route.upstream_addr,
            "route registered"
        );
        Ok(())
    } else {
        tracing::warn!(target: "apisix", route_id = %route.id, status = %status, body = %text, "route register failed");
        bail!("注册路由 {} 失败: HTTP {} {}", route.id, status, text)
    }
}

/// 删除路由
pub async fn deregister_route(cfg: &ApisixConfig, route_id: &str) -> Result<()> {
    if !cfg.enabled {
        return Ok(());
    }
    let client = Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()?;
    let url = cfg.route_url(route_id);

    let resp = client
        .delete(&url)
        .header("X-API-KEY", &cfg.admin_key)
        .send()
        .await
        .context("连接 APISIX 失败")?;

    if resp.status().is_success() || resp.status().as_u16() == 404 {
        tracing::info!(target: "apisix", route_id, "route deregistered");
        Ok(())
    } else {
        let text = resp.text().await.unwrap_or_default();
        bail!("删除路由 {route_id} 失败: {text}")
    }
}

/// 为微服务自动注册路由（推荐入口）。
///
/// `package` 为 proto 包名（如 `base.v1`），自动匹配 `/{package}/*` 全部方法。
/// `port` 为服务监听端口，上游地址取 `cfg.upstream_host:port`。
pub async fn auto_register(cfg: &ApisixConfig, service_id: &str, package: &str, port: u16) -> Result<()> {
    let addr = format!("{}:{}", cfg.upstream_host, port);
    let route = ServiceRoute {
        id: format!("{}-{}", cfg.prefix, service_id),
        name: format!("{service_id} ({package})"),
        uris: vec![format!("/{package}/*")],
        upstream_addr: addr,
        scheme: cfg.scheme.clone(),
        priority: 10,
        labels: {
            let mut m = HashMap::new();
            m.insert("managed-by".into(), "cool-apisix".into());
            m.insert("service".into(), service_id.to_string());
            m.insert("package".into(), package.to_string());
            m
        },
    };
    register_routes(cfg, std::slice::from_ref(&route)).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn grpc_route_shapes() {
        let r = ServiceRoute::grpc("base-service", "base.v1", "127.0.0.1:8001");
        assert_eq!(r.id, "base-service");
        assert_eq!(r.uris, vec!["/base.v1/*"]);
        assert_eq!(r.scheme, "grpc");
        assert_eq!(r.upstream_addr, "127.0.0.1:8001");
    }

    #[test]
    fn route_body_contains_upstream_and_labels() {
        let r = ServiceRoute::grpc("dict-service", "dict.v1", "10.0.0.2:8002");
        let body = route_body(&r);
        assert_eq!(body["name"], "dict-service routes");
        assert_eq!(body["uris"][0], "/dict.v1/*");
        assert_eq!(body["upstream"]["scheme"], "grpc");
        assert_eq!(body["upstream"]["nodes"]["10.0.0.2:8002"], 1);
        assert_eq!(body["labels"]["managed-by"], "cool-apisix");
    }

    #[test]
    fn config_urls() {
        let cfg = ApisixConfig {
            admin_url: "http://apisix:9180/".into(),
            prefix: "cool".into(),
            ..Default::default()
        };
        assert_eq!(cfg.routes_url(), "http://apisix:9180/apisix/admin/routes");
        assert_eq!(cfg.route_url("x"), "http://apisix:9180/apisix/admin/routes/x");
    }

    #[tokio::test]
    async fn disabled_skips_registration() {
        let cfg = ApisixConfig { enabled: false, ..Default::default() };
        let routes = vec![ServiceRoute::grpc("s", "p", "1:1")];
        assert!(register_routes(&cfg, &routes).await.is_ok());
    }
}
