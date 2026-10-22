//! APISIX 路由注册实现。
//!
//! **多实例自动发现**：同服务多实例共享同一路由，
//! 每个实例启动时把自己的 `host:port` **合并**进上游节点，停止时**摘除**自己。
//!
//! 节点管理策略：GET 当前路由 → 合并/移除本实例节点 → PUT 整体更新。

use std::collections::HashMap;

use anyhow::{bail, Context, Result};
use reqwest::Client;
use serde_json::{json, Map, Value};

use crate::config::ApisixConfig;

/// 微服务路由定义
#[derive(Debug, Clone)]
pub struct ServiceRoute {
    /// 路由 ID（同一服务的多实例共享）
    pub id: String,
    /// 路由名称
    pub name: String,
    /// 匹配 URI 列表
    pub uris: Vec<String>,
    /// 上游地址 "host:port"
    pub upstream_addr: String,
    /// 上游 scheme：http | grpc
    pub scheme: String,
    /// 匹配优先级
    pub priority: i32,
    /// 标签
    pub labels: HashMap<String, String>,
}

/// 服务实例（用于多实例注册）
#[derive(Debug, Clone)]
pub struct ServiceInstance {
    /// 服务 ID（路由共享标识）
    pub service_id: String,
    /// proto 包名，如 base.v1
    pub package: String,
    /// 实例地址 "host:port"
    pub addr: String,
}

impl ServiceRoute {
    /// 构造 gRPC 微服务路由（前缀匹配所有方法）
    pub fn grpc(service_id: &str, package: &str, addr: &str) -> Self {
        Self {
            id: service_id.to_string(),
            name: format!("{service_id} routes"),
            uris: vec![format!("/{package}.*")],
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

fn base_route_json(name: &str, id: &str, uris: &[String], priority: i32, labels: &HashMap<String, String>) -> Value {
    let mut label_map = Map::new();
    for (k, v) in labels {
        label_map.insert(k.clone(), Value::String(v.clone()));
    }
    json!({
        "name": name,
        "desc": format!("auto-registered by cool-apisix for {id}"),
        "uris": uris,
        "priority": priority,
        "status": 1,
        "labels": Value::Object(label_map),
    })
}

fn upstream_json(scheme: &str, nodes: &Map<String, Value>) -> Value {
    json!({
        "type": "roundrobin",
        "scheme": scheme,
        "nodes": Value::Object(nodes.clone()),
        "timeout": { "connect": 5, "send": 30, "read": 30 }
    })
}

fn single_node(addr: &str) -> Map<String, Value> {
    let mut m = Map::new();
    m.insert(addr.to_string(), json!(1));
    m
}

// ---------- HTTP helpers ----------

fn client() -> Result<Client> {
    Ok(Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()?)
}

/// GET 当前路由的 upstream.nodes；不存在返回 None
async fn fetch_nodes(
    client: &Client,
    cfg: &ApisixConfig,
    route_id: &str,
) -> Result<Option<Map<String, Value>>> {
    let url = cfg.route_url(route_id);
    let resp = client
        .get(&url)
        .header("X-API-KEY", &cfg.admin_key)
        .send()
        .await
        .context("连接 APISIX 失败")?;

    if resp.status().as_u16() == 404 {
        return Ok(None);
    }
    if !resp.status().is_success() {
        return Ok(None);
    }
    let body: Value = resp.json().await.unwrap_or(Value::Null);
    // 兼容 APISIX 真实返回 {value:{upstream:{nodes}}} 与直接返回 {upstream:{nodes}}
    let nodes = body
        .pointer("/value/upstream/nodes")
        .or_else(|| body.pointer("/upstream/nodes"))
        .and_then(|v| v.as_object())
        .cloned();
    Ok(nodes)
}

async fn put_route(client: &Client, cfg: &ApisixConfig, route_id: &str, body: &Value) -> Result<()> {
    let url = cfg.route_url(route_id);
    let resp = client
        .put(&url)
        .header("X-API-KEY", &cfg.admin_key)
        .header("Content-Type", "application/json")
        .json(body)
        .send()
        .await
        .context("连接 APISIX 失败")?;
    let status = resp.status();
    if status.is_success() {
        Ok(())
    } else {
        let text = resp.text().await.unwrap_or_default();
        bail!("更新路由 {route_id} 失败: HTTP {status} {text}")
    }
}

async fn delete_route(client: &Client, cfg: &ApisixConfig, route_id: &str) -> Result<()> {
    let url = cfg.route_url(route_id);
    let resp = client
        .delete(&url)
        .header("X-API-KEY", &cfg.admin_key)
        .send()
        .await
        .context("连接 APISIX 失败")?;
    if resp.status().is_success() || resp.status().as_u16() == 404 {
        Ok(())
    } else {
        let text = resp.text().await.unwrap_or_default();
        bail!("删除路由 {route_id} 失败: {text}")
    }
}

// ---------- 多实例注册（推荐） ----------

/// 注册服务实例（多实例自动发现）。
///
/// 同一 `service_id` 的多实例共享路由，节点自动合并：
/// - 首个实例：创建路由
/// - 后续实例：GET 现有节点 → 追加自己 → PUT
pub async fn register_instance(cfg: &ApisixConfig, inst: &ServiceInstance) -> Result<()> {
    if !cfg.enabled {
        return Ok(());
    }
    register_instance_inner(cfg, inst, false).await
}

/// 注册 gRPC-Web 实例（`/web/` 前缀 + grpc-web 插件）
pub async fn register_instance_web(cfg: &ApisixConfig, inst: &ServiceInstance) -> Result<()> {
    if !cfg.enabled {
        return Ok(());
    }
    register_instance_inner(cfg, inst, true).await
}

async fn register_instance_inner(
    cfg: &ApisixConfig,
    inst: &ServiceInstance,
    web: bool,
) -> Result<()> {
    let client = client()?;
    let (route_id, uri, priority) = if web {
        (
            format!("{}-{}-web", cfg.prefix, inst.service_id),
            format!("/web/{}.*", inst.package),
            20,
        )
    } else {
        (
            format!("{}-{}", cfg.prefix, inst.service_id),
            format!("/{}.*", inst.package),
            10,
        )
    };

    // 合并节点：保留其他实例，追加自己
    let mut nodes = fetch_nodes(&client, cfg, &route_id)
        .await?
        .unwrap_or_default();
    let is_new = nodes.is_empty();
    nodes.insert(inst.addr.clone(), json!(1));

    let mut body = base_route_json(
        &format!("{} ({})", inst.service_id, inst.package),
        &route_id,
        &[uri],
        priority,
        &{
            let mut m = HashMap::new();
            m.insert("managed-by".into(), "cool-apisix".into());
            m.insert("service".into(), inst.service_id.clone());
            m.insert("package".into(), inst.package.clone());
            if web {
                m.insert("protocol".into(), "grpc-web".into());
            }
            m
        },
    );
    body["upstream"] = upstream_json(&cfg.scheme, &nodes);
    if web {
        body["plugins"] = json!({
            "proxy-rewrite": {
                "scheme": cfg.scheme,
                "regex_uri": ["/web/(.*)", "/$1"]
            },
            "grpc-web": {}
        });
    } else {
        body["plugins"] = json!({ "proxy-rewrite": { "scheme": cfg.scheme } });
    }

    put_route(&client, cfg, &route_id, &body).await?;

    tracing::info!(
        target: "apisix",
        route_id = %route_id,
        instance = %inst.addr,
        total_nodes = nodes.len(),
        new_route = is_new,
        web,
        "instance registered"
    );
    Ok(())
}

/// 摘除服务实例（实例下线时调用）。
///
/// 只移除本实例节点；若上游已空则删除整个路由。
pub async fn deregister_instance(cfg: &ApisixConfig, inst: &ServiceInstance) -> Result<()> {
    if !cfg.enabled {
        return Ok(());
    }
    deregister_instance_inner(cfg, inst, false).await
}

/// 摘除 gRPC-Web 实例
pub async fn deregister_instance_web(cfg: &ApisixConfig, inst: &ServiceInstance) -> Result<()> {
    if !cfg.enabled {
        return Ok(());
    }
    deregister_instance_inner(cfg, inst, true).await
}

async fn deregister_instance_inner(
    cfg: &ApisixConfig,
    inst: &ServiceInstance,
    web: bool,
) -> Result<()> {
    let client = client()?;
    let route_id = if web {
        format!("{}-{}-web", cfg.prefix, inst.service_id)
    } else {
        format!("{}-{}", cfg.prefix, inst.service_id)
    };

    let Some(mut nodes) = fetch_nodes(&client, cfg, &route_id).await? else {
        tracing::info!(target: "apisix", %route_id, "route not found, nothing to deregister");
        return Ok(());
    };
    nodes.remove(&inst.addr);

    if nodes.is_empty() {
        // 最后一个实例下线 → 删除路由
        delete_route(&client, cfg, &route_id).await?;
        tracing::info!(target: "apisix", %route_id, instance = %inst.addr, "last instance, route deleted");
        return Ok(());
    }

    // 保留其他实例节点
    let uri = if web {
        format!("/web/{}.*", inst.package)
    } else {
        format!("/{}.*", inst.package)
    };
    let priority = if web { 20 } else { 10 };
    let mut body = base_route_json(
        &format!("{} ({})", inst.service_id, inst.package),
        &route_id,
        &[uri],
        priority,
        &{
            let mut m = HashMap::new();
            m.insert("managed-by".into(), "cool-apisix".into());
            m.insert("service".into(), inst.service_id.clone());
            m
        },
    );
    body["upstream"] = upstream_json(&cfg.scheme, &nodes);
    if web {
        body["plugins"] = json!({
            "proxy-rewrite": {
                "scheme": cfg.scheme,
                "regex_uri": ["/web/(.*)", "/$1"]
            },
            "grpc-web": {}
        });
    } else {
        body["plugins"] = json!({ "proxy-rewrite": { "scheme": cfg.scheme } });
    }
    put_route(&client, cfg, &route_id, &body).await?;
    tracing::info!(
        target: "apisix",
        route_id = %route_id,
        instance = %inst.addr,
        remaining = nodes.len(),
        "instance deregistered"
    );
    Ok(())
}

// ---------- 便捷入口（单实例 / 兼容） ----------

/// 为微服务自动注册路由（单实例快捷方式）。
pub async fn auto_register(cfg: &ApisixConfig, service_id: &str, package: &str, port: u16) -> Result<()> {
    let inst = ServiceInstance {
        service_id: service_id.to_string(),
        package: package.to_string(),
        addr: format!("{}:{}", cfg.upstream_host, port),
    };
    register_instance(cfg, &inst).await
}

/// 为微服务注册 gRPC-Web 路由（单实例快捷方式）
pub async fn auto_register_web(cfg: &ApisixConfig, service_id: &str, package: &str, port: u16) -> Result<()> {
    let inst = ServiceInstance {
        service_id: service_id.to_string(),
        package: package.to_string(),
        addr: format!("{}:{}", cfg.upstream_host, port),
    };
    register_instance_web(cfg, &inst).await
}

/// 批量注册路由（整表覆盖，仅用于单实例场景）
pub async fn register_routes(cfg: &ApisixConfig, routes: &[ServiceRoute]) -> Result<()> {
    if !cfg.enabled {
        return Ok(());
    }
    let client = client()?;
    for route in routes {
        let mut body = base_route_json(
            &route.name,
            &route.id,
            &route.uris,
            route.priority,
            &route.labels,
        );
        body["upstream"] = upstream_json(&route.scheme, &single_node(&route.upstream_addr));
        body["plugins"] = json!({ "proxy-rewrite": { "scheme": route.scheme } });
        put_route(&client, cfg, &route.id, &body).await?;
        tracing::info!(target: "apisix", route_id = %route.id, "route registered");
    }
    Ok(())
}

/// 批量注册 gRPC-Web 路由
pub async fn register_routes_web(cfg: &ApisixConfig, routes: &[ServiceRoute]) -> Result<()> {
    if !cfg.enabled {
        return Ok(());
    }
    let client = client()?;
    for route in routes {
        let mut body = base_route_json(
            &route.name,
            &route.id,
            &route.uris,
            route.priority,
            &route.labels,
        );
        body["upstream"] = upstream_json(&route.scheme, &single_node(&route.upstream_addr));
        body["plugins"] = json!({
            "proxy-rewrite": {
                "scheme": route.scheme,
                "regex_uri": ["/web/(.*)", "/$1"]
            },
            "grpc-web": {}
        });
        put_route(&client, cfg, &route.id, &body).await?;
    }
    Ok(())
}

/// 删除路由
pub async fn deregister_route(cfg: &ApisixConfig, route_id: &str) -> Result<()> {
    if !cfg.enabled {
        return Ok(());
    }
    let client = client()?;
    delete_route(&client, cfg, route_id).await?;
    tracing::info!(target: "apisix", route_id, "route deregistered");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn grpc_route_shapes() {
        let r = ServiceRoute::grpc("base-service", "base.v1", "127.0.0.1:8001");
        assert_eq!(r.id, "base-service");
        assert_eq!(r.uris, vec!["/base.v1.*"]);
        assert_eq!(r.scheme, "grpc");
    }

    #[test]
    fn instance_route_ids() {
        let cfg = ApisixConfig::default();
        let inst = ServiceInstance {
            service_id: "base-service".into(),
            package: "base.v1".into(),
            addr: "10.0.0.1:8001".into(),
        };
        assert_eq!(format!("{}-{}", cfg.prefix, inst.service_id), "cool-base-service");
    }

    #[test]
    fn single_node_map() {
        let n = single_node("10.0.0.1:8001");
        assert_eq!(n.get("10.0.0.1:8001"), Some(&json!(1)));
    }

    #[tokio::test]
    async fn disabled_skips() {
        let cfg = ApisixConfig { enabled: false, ..Default::default() };
        let inst = ServiceInstance {
            service_id: "s".into(),
            package: "p".into(),
            addr: "1:1".into(),
        };
        assert!(register_instance(&cfg, &inst).await.is_ok());
        assert!(deregister_instance(&cfg, &inst).await.is_ok());
    }
}
