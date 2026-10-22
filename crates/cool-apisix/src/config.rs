//! APISIX 连接配置。

use serde::Deserialize;

/// APISIX Admin API 配置
#[derive(Debug, Clone, Deserialize)]
pub struct ApisixConfig {
    /// Admin API 地址，如 http://127.0.0.1:9180
    #[serde(default = "default_admin_url")]
    pub admin_url: String,
    /// Admin API Key（X-API-KEY 头）
    #[serde(default = "default_admin_key")]
    pub admin_key: String,
    /// 是否启用路由注册
    #[serde(default = "default_true")]
    pub enabled: bool,
    /// 上游服务地址（本服务对外可达地址）
    #[serde(default = "default_upstream_host")]
    pub upstream_host: String,
    /// gRPC 代理 scheme：http | grpc
    #[serde(default = "default_scheme")]
    pub scheme: String,
    /// 路由前缀标签（用于批量管理）
    #[serde(default = "default_prefix")]
    pub prefix: String,
}

impl Default for ApisixConfig {
    fn default() -> Self {
        Self {
            admin_url: default_admin_url(),
            admin_key: default_admin_key(),
            enabled: true,
            upstream_host: default_upstream_host(),
            scheme: default_scheme(),
            prefix: default_prefix(),
        }
    }
}

impl ApisixConfig {
    /// Admin API 路由端点（自动补全 /apisix/admin 前缀）
    pub fn routes_url(&self) -> String {
        let base = self.admin_url.trim_end_matches('/');
        if base.ends_with("/admin") {
            // 已含 /apisix/admin 或 /admin 前缀
            format!("{base}/routes")
        } else {
            format!("{base}/apisix/admin/routes")
        }
    }

    /// 单条路由端点
    pub fn route_url(&self, id: &str) -> String {
        format!("{}/{}", self.routes_url(), id)
    }
}

fn default_admin_url() -> String {
    "http://127.0.0.1:9180".into()
}
fn default_admin_key() -> String {
    // APISIX 默认 dev key，生产必须更换
    "edd1c9f034335f136f87ad84b625c8f1".into()
}
fn default_true() -> bool {
    true
}
fn default_upstream_host() -> String {
    "127.0.0.1".into()
}
fn default_scheme() -> String {
    "grpc".into()
}
fn default_prefix() -> String {
    "cool".into()
}

impl ApisixConfig {
    /// 从 micro.yaml 的 `apisix` 段或环境变量加载。
    ///
    /// 环境变量：APISIX_ADMIN_URL / APISIX_ADMIN_KEY / APISIX_ENABLED / APISIX_UPSTREAM_HOST
    pub fn load() -> Self {
        let mut cfg = ApisixConfig::default();
        if let Ok(v) = std::env::var("APISIX_ADMIN_URL") {
            cfg.admin_url = v;
        }
        if let Ok(v) = std::env::var("APISIX_ADMIN_KEY") {
            cfg.admin_key = v;
        }
        if let Ok(v) = std::env::var("APISIX_ENABLED") {
            cfg.enabled = !matches!(v.as_str(), "0" | "false" | "False");
        }
        if let Ok(v) = std::env::var("APISIX_UPSTREAM_HOST") {
            cfg.upstream_host = v;
        }
        if let Ok(v) = std::env::var("APISIX_SCHEME") {
            cfg.scheme = v;
        }
        cfg
    }

    /// 从服务配置的 apisix 段构建（供 cool-micro MicroConfig 使用）
    pub fn from_section(section: &crate::config::ApisixSection) -> Self {
        Self {
            admin_url: section.admin_url.clone(),
            admin_key: section.admin_key.clone(),
            enabled: section.enabled,
            upstream_host: section.upstream_host.clone(),
            scheme: section.scheme.clone(),
            prefix: section.prefix.clone(),
        }
    }
}

/// 可嵌入 MicroConfig 的 apisix 段
#[derive(Debug, Clone, Deserialize)]
pub struct ApisixSection {
    #[serde(default = "default_admin_url")]
    pub admin_url: String,
    #[serde(default = "default_admin_key")]
    pub admin_key: String,
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default = "default_upstream_host")]
    pub upstream_host: String,
    #[serde(default = "default_scheme")]
    pub scheme: String,
    #[serde(default = "default_prefix")]
    pub prefix: String,
}

impl Default for ApisixSection {
    fn default() -> Self {
        Self {
            admin_url: default_admin_url(),
            admin_key: default_admin_key(),
            enabled: false, // 默认关闭，显式开启才注册
            upstream_host: default_upstream_host(),
            scheme: default_scheme(),
            prefix: default_prefix(),
        }
    }
}
