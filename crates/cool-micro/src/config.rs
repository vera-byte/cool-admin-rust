//! 微服务配置（共享 yaml + 服务级覆盖）。

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde::Deserialize;

/// 单个微服务配置
#[derive(Debug, Clone, Deserialize)]
pub struct MicroConfig {
    pub service: ServiceIdentityConfig,
    pub server: MicroServerConfig,
    pub database: MicroDbConfig,
    #[serde(default)]
    pub redis: MicroRedisConfig,
    #[serde(default)]
    pub sa_token: SaTokenSection,
    #[serde(default)]
    pub cool: CoolSection,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ServiceIdentityConfig {
    /// 服务 ID，如 base-service
    pub id: String,
    /// 服务名，如 base
    pub name: String,
    /// 服务凭证密钥（服务间调用）
    pub secret: String,
    /// 依赖的上游服务 ID（用于 ServiceCredential 传递）
    #[serde(default)]
    pub depends_on: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct MicroServerConfig {
    pub address: String,
    #[serde(default = "default_true")]
    pub reflection: bool,
    #[serde(default = "default_max_msg")]
    pub max_decoding_message_size: usize,
    #[serde(default = "default_log_level")]
    pub log_level: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct MicroDbConfig {
    pub default: DbEntry,
}

#[derive(Debug, Clone, Deserialize)]
pub struct DbEntry {
    #[serde(rename = "type", default = "default_db_type")]
    pub db_type: String,
    pub host: String,
    pub port: u16,
    pub user: String,
    pub pass: String,
    pub name: String,
    #[serde(default = "default_max_conn")]
    pub max_connections: u32,
    #[serde(default = "default_min_conn")]
    pub min_connections: u32,
    #[serde(default = "default_conn_timeout")]
    pub connect_timeout_secs: u64,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct MicroRedisConfig {
    #[serde(default)]
    pub address: String,
    #[serde(default)]
    pub db: i64,
    #[serde(default)]
    pub password: Option<String>,
}

impl MicroRedisConfig {
    pub fn enabled(&self) -> bool {
        !self.address.trim().is_empty()
    }

    pub fn url(&self) -> String {
        let mut url = if self.address.contains("://") {
            self.address.clone()
        } else {
            format!("redis://{}", self.address)
        };
        if let Some(pw) = self.password.as_deref()
            && !pw.is_empty()
        {
            let rest = url.trim_start_matches("redis://").to_string();
            url = format!("redis://:{pw}@{rest}");
        }
        if self.db != 0 {
            url = format!("{url}/{}", self.db);
        }
        url
    }
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct SaTokenSection {
    /// token 名（gRPC metadata 键）
    #[serde(default = "default_token_name")]
    pub token_name: String,
    /// 会话有效期（秒）
    #[serde(default = "default_timeout")]
    pub timeout: i64,
    /// Redis 键前缀（多服务必须一致）
    #[serde(default = "default_key_prefix")]
    pub key_prefix: String,
    /// 是否启用鉴权层
    #[serde(default = "default_true")]
    pub enable: bool,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct CoolSection {
    #[serde(default = "default_mode")]
    pub mode: String,
    #[serde(default = "default_true")]
    pub auto_migrate: bool,
}

fn default_true() -> bool {
    true
}
fn default_max_msg() -> usize {
    104_857_600
}
fn default_log_level() -> String {
    "info".into()
}
fn default_db_type() -> String {
    "mysql".into()
}
fn default_max_conn() -> u32 {
    20
}
fn default_min_conn() -> u32 {
    1
}
fn default_conn_timeout() -> u64 {
    5
}
fn default_token_name() -> String {
    "satoken".into()
}
fn default_timeout() -> i64 {
    3600
}
fn default_key_prefix() -> String {
    "sa:".into()
}
fn default_mode() -> String {
    "dev".into()
}

impl MicroConfig {
    pub fn default_path() -> PathBuf {
        PathBuf::from("manifest/config/micro.yaml")
    }

    /// 加载配置；`MICRO_CONFIG` 可指定路径；`COOL_*`/`MICRO_*` 环境变量覆盖。
    pub fn load(path: Option<&Path>) -> Result<Self> {
        let path = path
            .map(|p| p.to_path_buf())
            .or_else(|| std::env::var_os("MICRO_CONFIG").map(PathBuf::from))
            .unwrap_or_else(Self::default_path);

        let cfg = config::Config::builder()
            .add_source(config::File::from(path.clone()).required(true))
            .add_source(
                config::Environment::with_prefix("MICRO")
                    .prefix_separator("_")
                    .separator("__"),
            )
            .build()
            .with_context(|| format!("加载配置失败: {}", path.display()))?;

        cfg.try_deserialize().context("解析配置失败")
    }

    pub fn socket_addr(&self) -> Result<std::net::SocketAddr> {
        self.server
            .address
            .parse()
            .with_context(|| format!("非法监听地址: {}", self.server.address))
    }

    pub fn db_config(&self) -> cool_db::DbConfig {
        let d = &self.database.default;
        cool_db::DbConfig {
            host: d.host.clone(),
            port: d.port,
            user: d.user.clone(),
            pass: d.pass.clone(),
            name: d.name.clone(),
            max_connections: d.max_connections,
            min_connections: d.min_connections,
            connect_timeout_secs: d.connect_timeout_secs,
        }
    }
}
