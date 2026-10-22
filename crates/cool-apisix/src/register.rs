//! APISIX 服务注册：Service 资源 + 路由绑定 + 服务发现。
//!
//! **架构**（对齐 APISIX 最佳实践）：
//!
//! ```text
//! APISIX Service（每微服务一个）
//!   ├── upstream：service_name + discovery_type（DNS/K8s/Nacos/ConsulKV/Eureka）
//!   ├── plugins：proxy-rewrite / grpc-web
//!   └── 路由 ×2（原生 gRPC + gRPC-Web）绑定 service_id
//! ```
//!
//! 多实例由**服务发现**自动管理（无需手工合并节点）。

use std::collections::BTreeMap;

use anyhow::{bail, Context, Result};
use reqwest::Client;
use serde_json::{json, Value};

use crate::config::ApisixConfig;

// ==================== 服务发现 ====================

/// 服务发现类型（APISIX discovery_type）
#[derive(Debug, Clone)]
pub enum ServiceDiscovery {
    /// 静态节点（显式 host:port 列表，兼容旧模式）
    Static {
        nodes: BTreeMap<String, u32>,
    },
    /// DNS 服务发现（A/SRV 记录）
    /// service_name 形如 `dict-service.local` 或 `dict-service.default.svc.cluster.local`
    Dns {
        service_name: String,
    },
    /// Kubernetes EndpointSlice / Endpoints
    Kubernetes {
        service_name: String,
        namespace: String,
    },
    /// Nacos 服务发现
    Nacos {
        service_name: String,
        group: String,
    },
    /// Consul KV 服务发现
    ConsulKv {
        service_name: String,
    },
    /// Eureka
    Eureka {
        service_name: String,
    },
}

impl ServiceDiscovery {
    /// discovery_type 标识（APISIX 配置字段）
    pub fn discovery_type(&self) -> &'static str {
        match self {
            Self::Static { .. } => "",
            Self::Dns { .. } => "dns",
            Self::Kubernetes { .. } => "kubernetes",
            Self::Nacos { .. } => "nacos",
            Self::ConsulKv { .. } => "consul_kv",
            Self::Eureka { .. } => "eureka",
        }
    }

    /// service_name（APISIX 配置字段）
    pub fn service_name(&self) -> &str {
        match self {
            Self::Static { .. } => "",
            Self::Dns { service_name }
            | Self::Kubernetes { service_name, .. }
            | Self::Nacos { service_name, .. }
            | Self::ConsulKv { service_name }
            | Self::Eureka { service_name } => service_name,
        }
    }

    /// 是否为服务发现模式
    pub fn is_discovery(&self) -> bool {
        !matches!(self, Self::Static { .. })
    }

    /// 生成 APISIX upstream JSON 片段（nodes 或 service_name/discovery_type）
    fn upstream_fields(&self) -> Value {
        match self {
            Self::Static { nodes } => {
                let map: serde_json::Map<String, Value> = nodes
                    .iter()
                    .map(|(k, w)| (k.clone(), json!(*w)))
                    .collect();
                json!({ "nodes": Value::Object(map) })
            }
            Self::Dns { service_name } => {
                json!({ "service_name": service_name, "discovery_type": "dns" })
            }
            Self::Kubernetes { service_name, namespace } => {
                // K8s：service_name 形如 "svc.namespace" 或 "svc"
                let full = if namespace.is_empty() {
                    service_name.clone()
                } else {
                    format!("{service_name}.{namespace}")
                };
                json!({ "service_name": full, "discovery_type": "kubernetes" })
            }
            Self::Nacos { service_name, group } => {
                let mut v = json!({ "service_name": service_name, "discovery_type": "nacos" });
                if !group.is_empty() {
                    v["discovery_args"] = json!({ "group_name": group });
                }
                v
            }
            Self::ConsulKv { service_name } => {
                json!({ "service_name": service_name, "discovery_type": "consul_kv" })
            }
            Self::Eureka { service_name } => {
                json!({ "service_name": service_name, "discovery_type": "eureka" })
            }
        }
    }
}

// ==================== 微服务定义 ====================

/// 微服务（映射为一个 APISIX Service + 2 条路由）
#[derive(Debug, Clone)]
pub struct MicroService {
    /// APISIX Service 资源 ID（如 "dict-service"）
    pub id: String,
    /// 显示名
    pub name: String,
    /// proto 包名（如 "dict.v1"），决定路由 URI
    pub package: String,
    /// 上游 scheme：grpc | http
    pub scheme: String,
    /// 服务发现配置
    pub discovery: ServiceDiscovery,
    /// 匹配优先级
    pub priority: i32,
}

impl MicroService {
    /// 构造 gRPC 微服务（服务发现模式）
    pub fn grpc_discovery(id: &str, package: &str, discovery: ServiceDiscovery) -> Self {
        Self {
            id: id.to_string(),
            name: format!("{id} ({package})"),
            package: package.to_string(),
            scheme: "grpc".into(),
            discovery,
            priority: 10,
        }
    }

    /// 便捷：DNS 服务发现
    pub fn grpc_dns(id: &str, package: &str, service_name: &str) -> Self {
        Self::grpc_discovery(id, package, ServiceDiscovery::Dns {
            service_name: service_name.to_string(),
        })
    }

    /// 便捷：Kubernetes 服务发现
    pub fn grpc_k8s(id: &str, package: &str, service_name: &str, namespace: &str) -> Self {
        Self::grpc_discovery(id, package, ServiceDiscovery::Kubernetes {
            service_name: service_name.to_string(),
            namespace: namespace.to_string(),
        })
    }

    /// 便捷：Nacos 服务发现
    pub fn grpc_nacos(id: &str, package: &str, service_name: &str, group: &str) -> Self {
        Self::grpc_discovery(id, package, ServiceDiscovery::Nacos {
            service_name: service_name.to_string(),
            group: group.to_string(),
        })
    }

    /// 便捷：ConsulKV 服务发现
    pub fn grpc_consul(id: &str, package: &str, service_name: &str) -> Self {
        Self::grpc_discovery(id, package, ServiceDiscovery::ConsulKv {
            service_name: service_name.to_string(),
        })
    }

    /// 便捷：静态节点（兼容旧模式）
    pub fn grpc_static(id: &str, package: &str, addr: &str) -> Self {
        let mut nodes = BTreeMap::new();
        nodes.insert(addr.to_string(), 1u32);
        Self::grpc_discovery(id, package, ServiceDiscovery::Static { nodes })
    }

    /// 原生 gRPC 路由 ID（与 Service id 对应，避免重复前缀）
    pub fn route_id(&self, _prefix: &str) -> String {
        self.id.clone()
    }

    /// gRPC-Web 路由 ID
    pub fn web_route_id(&self, _prefix: &str) -> String {
        format!("{}-web", self.id)
    }

    /// 原生 gRPC 路由 URI
    pub fn route_uri(&self) -> String {
        format!("/{}.*", self.package)
    }

    /// gRPC-Web 路由 URI
    pub fn web_route_uri(&self) -> String {
        format!("/web/{}.*", self.package)
    }
}

// ==================== HTTP helpers ====================

fn client() -> Result<Client> {
    Ok(Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()?)
}

/// APISIX Service 资源 URL
fn service_url(cfg: &ApisixConfig, id: &str) -> String {
    let base = cfg.admin_url.trim_end_matches('/');
    let base = if base.ends_with("/admin") {
        base.to_string()
    } else {
        format!("{base}/apisix/admin")
    };
    format!("{base}/services/{id}")
}

/// APISIX Route 资源 URL
fn route_url(cfg: &ApisixConfig, id: &str) -> String {
    cfg.route_url(id)
}

async fn put_json(client: &Client, cfg: &ApisixConfig, url: &str, body: &Value) -> Result<()> {
    let resp = client
        .put(url)
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
        bail!("PUT {url} 失败: HTTP {status} {text}")
    }
}

async fn delete(client: &Client, cfg: &ApisixConfig, url: &str) -> Result<()> {
    let resp = client
        .delete(url)
        .header("X-API-KEY", &cfg.admin_key)
        .send()
        .await
        .context("连接 APISIX 失败")?;
    if resp.status().is_success() || resp.status().as_u16() == 404 {
        Ok(())
    } else {
        let text = resp.text().await.unwrap_or_default();
        bail!("DELETE {url} 失败: {text}")
    }
}

// ==================== Service 注册 ====================

/// 生成 APISIX Service 资源体（含 upstream + plugins）
pub fn service_body(svc: &MicroService) -> Value {
    let mut upstream = json!({
        "type": "roundrobin",
        "scheme": svc.scheme,
        "timeout": { "connect": 5, "send": 30, "read": 30 },
        "retries": 2,
        "checks": {
            "active": {
                "type": "tcp",
                "timeout": 1,
                "healthy": { "interval": 5, "successes": 1 },
                "unhealthy": { "interval": 2, "failures": 3 }
            },
            "passive": {
                "healthy": { "successes": 2 },
                "unhealthy": { "http_failures": 2, "tcp_failures": 2 }
            }
        }
    });
    // 合并服务发现字段（nodes 或 service_name/discovery_type）
    if let Some(fields) = svc.discovery.upstream_fields().as_object() {
        for (k, v) in fields {
            upstream[k] = v.clone();
        }
    }

    let mut plugins = json!({
        "proxy-rewrite": { "scheme": svc.scheme }
    });
    if svc.scheme == "grpc" {
        // gRPC-Web 由独立路由挂载，Service 统一 proxy-rewrite
    }
    let _ = &mut plugins;

    json!({
        "name": svc.name,
        "desc": format!("auto-registered by cool-apisix for {}", svc.id),
        "upstream": upstream,
        "plugins": { "proxy-rewrite": { "scheme": svc.scheme } }
    })
}

/// 注册/更新 APISIX Service（幂等 PUT）
pub async fn register_service(cfg: &ApisixConfig, svc: &MicroService) -> Result<()> {
    if !cfg.enabled {
        return Ok(());
    }
    let client = client()?;
    let url = service_url(cfg, &svc.id);
    let body = service_body(svc);
    put_json(&client, cfg, &url, &body).await?;
    tracing::info!(
        target: "apisix",
        service_id = %svc.id,
        discovery = svc.discovery.discovery_type(),
        service_name = svc.discovery.service_name(),
        "APISIX service registered"
    );
    Ok(())
}

/// 删除 APISIX Service
pub async fn deregister_service(cfg: &ApisixConfig, service_id: &str) -> Result<()> {
    if !cfg.enabled {
        return Ok(());
    }
    let client = client()?;
    let url = service_url(cfg, service_id);
    delete(&client, cfg, &url).await?;
    tracing::info!(target: "apisix", service_id, "APISIX service deregistered");
    Ok(())
}

// ==================== 路由绑定 ====================

/// 路由体：绑定 service_id（不内联 upstream）
fn route_body(route_id: &str, name: &str, uris: &[String], priority: i32, service_id: &str, labels: &[(&str, &str)]) -> Value {
    let mut label_map = serde_json::Map::new();
    for (k, v) in labels {
        label_map.insert(k.to_string(), Value::String(v.to_string()));
    }
    json!({
        "name": name,
        "desc": format!("auto-registered by cool-apisix for {route_id}"),
        "uris": uris,
        "priority": priority,
        "status": 1,
        "service_id": service_id,
        "labels": Value::Object(label_map)
    })
}

/// 绑定原生 gRPC 路由到 Service
pub async fn bind_route(cfg: &ApisixConfig, svc: &MicroService) -> Result<()> {
    if !cfg.enabled {
        return Ok(());
    }
    let client = client()?;
    let route_id = svc.route_id(&cfg.prefix);
    let mut body = route_body(
        &route_id,
        &format!("{} gRPC", svc.name),
        &[svc.route_uri()],
        svc.priority,
        &svc.id,
        &[
            ("managed-by", "cool-apisix"),
            ("service", &svc.id),
            ("package", &svc.package),
            ("protocol", "grpc"),
        ],
    );
    // 与 Service 保持一致的 proxy-rewrite（scheme）
    body["plugins"] = json!({ "proxy-rewrite": { "scheme": svc.scheme } });
    put_json(&client, cfg, &route_url(cfg, &route_id), &body).await?;
    tracing::info!(target: "apisix", route_id = %route_id, service_id = %svc.id, "gRPC route bound");
    Ok(())
}

/// 绑定 gRPC-Web 路由到 Service（/web/ 前缀 + grpc-web 插件 + 路径重写）
pub async fn bind_route_web(cfg: &ApisixConfig, svc: &MicroService) -> Result<()> {
    if !cfg.enabled {
        return Ok(());
    }
    let client = client()?;
    let route_id = svc.web_route_id(&cfg.prefix);
    let mut body = route_body(
        &route_id,
        &format!("{} gRPC-Web", svc.name),
        &[svc.web_route_uri()],
        svc.priority + 10,
        &svc.id,
        &[
            ("managed-by", "cool-apisix"),
            ("service", &svc.id),
            ("package", &svc.package),
            ("protocol", "grpc-web"),
        ],
    );
    // gRPC-Web 路由需剥离 /web/ 前缀
    body["plugins"] = json!({
        "proxy-rewrite": {
            "regex_uri": ["/web/(.*)", "/$1"]
        },
        "grpc-web": {}
    });
    put_json(&client, cfg, &route_url(cfg, &route_id), &body).await?;
    tracing::info!(target: "apisix", route_id = %route_id, service_id = %svc.id, "gRPC-Web route bound");
    Ok(())
}

/// 解绑路由
pub async fn unbind_route(cfg: &ApisixConfig, route_id: &str) -> Result<()> {
    if !cfg.enabled {
        return Ok(());
    }
    let client = client()?;
    delete(&client, cfg, &route_url(cfg, route_id)).await?;
    Ok(())
}

// ==================== 一键注册 ====================

/// 一键注册微服务：创建 APISIX Service + 绑定 gRPC 路由 + gRPC-Web 路由
pub async fn register_microservice(cfg: &ApisixConfig, svc: &MicroService) -> Result<()> {
    register_service(cfg, svc).await?;
    bind_route(cfg, svc).await?;
    bind_route_web(cfg, svc).await?;
    tracing::info!(target: "apisix", service_id = %svc.id, "microservice fully registered");
    Ok(())
}

/// 一键注销微服务：删除路由 + Service
pub async fn deregister_microservice(cfg: &ApisixConfig, svc: &MicroService) -> Result<()> {
    unbind_route(cfg, &svc.web_route_id(&cfg.prefix)).await?;
    unbind_route(cfg, &svc.route_id(&cfg.prefix)).await?;
    deregister_service(cfg, &svc.id).await?;
    tracing::info!(target: "apisix", service_id = %svc.id, "microservice deregistered");
    Ok(())
}

// ==================== 兼容 API（旧节点合并模式） ====================

/// 服务实例（静态节点模式，兼容旧 API）
#[derive(Debug, Clone)]
pub struct ServiceInstance {
    pub service_id: String,
    pub package: String,
    pub addr: String,
}

/// 旧路由定义（兼容）
#[derive(Debug, Clone)]
pub struct ServiceRoute {
    pub id: String,
    pub name: String,
    pub uris: Vec<String>,
    pub upstream_addr: String,
    pub scheme: String,
    pub priority: i32,
    pub labels: std::collections::HashMap<String, String>,
}

impl ServiceRoute {
    pub fn grpc(service_id: &str, package: &str, addr: &str) -> Self {
        Self {
            id: service_id.to_string(),
            name: format!("{service_id} routes"),
            uris: vec![format!("/{package}.*")],
            upstream_addr: addr.to_string(),
            scheme: "grpc".into(),
            priority: 10,
            labels: {
                let mut m = std::collections::HashMap::new();
                m.insert("managed-by".into(), "cool-apisix".into());
                m.insert("service".into(), service_id.to_string());
                m
            },
        }
    }
}

/// 静态节点注册（兼容旧模式：内联 nodes）
pub async fn register_instance(cfg: &ApisixConfig, inst: &ServiceInstance) -> Result<()> {
    let mut nodes = BTreeMap::new();
    nodes.insert(inst.addr.clone(), 1u32);
    let svc = MicroService {
        id: format!("{}-{}", cfg.prefix, inst.service_id),
        name: format!("{} ({})", inst.service_id, inst.package),
        package: inst.package.clone(),
        scheme: "grpc".into(),
        discovery: ServiceDiscovery::Static { nodes },
        priority: 10,
    };
    register_service(cfg, &svc).await?;
    bind_route(cfg, &svc).await?;
    Ok(())
}

/// 静态节点 gRPC-Web 注册（兼容）
pub async fn register_instance_web(cfg: &ApisixConfig, inst: &ServiceInstance) -> Result<()> {
    let mut nodes = BTreeMap::new();
    nodes.insert(inst.addr.clone(), 1u32);
    let svc = MicroService {
        id: format!("{}-{}-web", cfg.prefix, inst.service_id),
        name: format!("{} ({}) web", inst.service_id, inst.package),
        package: inst.package.clone(),
        scheme: "grpc".into(),
        discovery: ServiceDiscovery::Static { nodes },
        priority: 20,
    };
    register_service(cfg, &svc).await?;
    bind_route_web(cfg, &svc).await?;
    Ok(())
}

/// 静态节点摘除（兼容）
pub async fn deregister_instance(cfg: &ApisixConfig, inst: &ServiceInstance) -> Result<()> {
    let svc_id = format!("{}-{}", cfg.prefix, inst.service_id);
    let route_id = format!("{}-{}", cfg.prefix, inst.service_id);
    unbind_route(cfg, &route_id).await?;
    deregister_service(cfg, &svc_id).await?;
    Ok(())
}

/// 静态节点摘除 gRPC-Web（兼容）
pub async fn deregister_instance_web(cfg: &ApisixConfig, inst: &ServiceInstance) -> Result<()> {
    let route_id = format!("{}-{}-web", cfg.prefix, inst.service_id);
    let svc_id = format!("{}-{}-web", cfg.prefix, inst.service_id);
    unbind_route(cfg, &route_id).await?;
    deregister_service(cfg, &svc_id).await?;
    Ok(())
}

/// 单实例快捷注册（兼容）
pub async fn auto_register(cfg: &ApisixConfig, service_id: &str, package: &str, port: u16) -> Result<()> {
    let inst = ServiceInstance {
        service_id: service_id.to_string(),
        package: package.to_string(),
        addr: format!("{}:{}", cfg.upstream_host, port),
    };
    register_instance(cfg, &inst).await
}

/// 单实例 gRPC-Web 注册（兼容）
pub async fn auto_register_web(cfg: &ApisixConfig, service_id: &str, package: &str, port: u16) -> Result<()> {
    let inst = ServiceInstance {
        service_id: service_id.to_string(),
        package: package.to_string(),
        addr: format!("{}:{}", cfg.upstream_host, port),
    };
    register_instance_web(cfg, &inst).await
}

/// 批量路由注册（兼容，旧 ServiceRoute）
pub async fn register_routes(cfg: &ApisixConfig, routes: &[ServiceRoute]) -> Result<()> {
    for r in routes {
        let inst = ServiceInstance {
            service_id: r.id.clone(),
            package: r.uris.first().map(|u| u.trim_start_matches('/').trim_end_matches(".*").to_string()).unwrap_or_default(),
            addr: r.upstream_addr.clone(),
        };
        register_instance(cfg, &inst).await?;
    }
    Ok(())
}

/// 批量 gRPC-Web 路由注册（兼容）
pub async fn register_routes_web(cfg: &ApisixConfig, routes: &[ServiceRoute]) -> Result<()> {
    for r in routes {
        let inst = ServiceInstance {
            service_id: r.id.clone(),
            package: r.uris.first().map(|u| u.trim_start_matches("/web/").trim_end_matches(".*").to_string()).unwrap_or_default(),
            addr: r.upstream_addr.clone(),
        };
        register_instance_web(cfg, &inst).await?;
    }
    Ok(())
}

/// 删除路由（兼容）
pub async fn deregister_route(cfg: &ApisixConfig, route_id: &str) -> Result<()> {
    unbind_route(cfg, route_id).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn discovery_types() {
        let dns = ServiceDiscovery::Dns { service_name: "dict-service.local".into() };
        assert_eq!(dns.discovery_type(), "dns");
        assert_eq!(dns.service_name(), "dict-service.local");
        assert!(dns.is_discovery());

        let k8s = ServiceDiscovery::Kubernetes {
            service_name: "dict-service".into(),
            namespace: "default".into(),
        };
        assert_eq!(k8s.discovery_type(), "kubernetes");

        let mut nodes = BTreeMap::new();
        nodes.insert("127.0.0.1:8002".to_string(), 1u32);
        let st = ServiceDiscovery::Static { nodes };
        assert_eq!(st.discovery_type(), "");
        assert!(!st.is_discovery());
    }

    #[test]
    fn microservice_routes() {
        let svc = MicroService::grpc_dns("dict-service", "dict.v1", "dict-service.local");
        assert_eq!(svc.route_id("cool"), "dict-service");
        assert_eq!(svc.web_route_id("cool"), "dict-service-web");
        assert_eq!(svc.route_uri(), "/dict.v1.*");
        assert_eq!(svc.web_route_uri(), "/web/dict.v1.*");
    }

    #[test]
    fn service_body_has_discovery_fields() {
        let svc = MicroService::grpc_k8s("dict-service", "dict.v1", "dict-service", "default");
        let body = service_body(&svc);
        assert_eq!(body["upstream"]["discovery_type"], "kubernetes");
        assert_eq!(body["upstream"]["service_name"], "dict-service.default");
        assert!(body["upstream"].get("nodes").is_none());
        assert!(body["upstream"]["checks"].get("active").is_some());
    }

    #[test]
    fn service_body_static_has_nodes() {
        let svc = MicroService::grpc_static("dict-service", "dict.v1", "127.0.0.1:8002");
        let body = service_body(&svc);
        assert_eq!(body["upstream"]["discovery_type"], Value::Null);
        assert!(body["upstream"]["nodes"].is_object());
    }

    #[test]
    fn route_body_binds_service_id() {
        let body = route_body(
            "cool-dict-service",
            "dict gRPC",
            &["/dict.v1.*".to_string()],
            10,
            "dict-service",
            &[("managed-by", "cool-apisix")],
        );
        assert_eq!(body["service_id"], "dict-service");
        assert!(body.get("upstream").is_none(), "route must not inline upstream");
        assert_eq!(body["uris"][0], "/dict.v1.*");
    }
}
