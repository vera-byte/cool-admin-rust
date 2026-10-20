//! 配置加载：manifest/config/config.yaml + 环境变量覆盖（COOL_ 前缀）。
//!
//! YAML 为 camelCase 拼写（与 cool-admin-go 对齐），结构体字段用 rename/alias 绑定，
//! 避免 config-rs 静默落到默认值。

#![allow(dead_code)]

use std::net::SocketAddr;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
pub struct AppConfig {
    pub server: ServerConfig,
    pub database: DatabaseSection,
    #[serde(default)]
    pub redis: RedisSection,
    #[serde(default)]
    pub cool: CoolSection,
    #[serde(default)]
    pub modules: ModulesSection,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ServerConfig {
    pub address: String,
    #[serde(default = "default_true")]
    pub reflection: bool,
    #[serde(default = "default_max_msg")]
    pub max_decoding_message_size: usize,
    #[serde(default = "default_log_level")]
    pub log_level: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct DatabaseSection {
    pub default: DbEntry,
}

#[derive(Debug, Clone, Deserialize)]
pub struct DbEntry {
    #[serde(rename = "type", alias = "type", default = "default_db_type")]
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

// ---------- redis ----------

#[derive(Debug, Clone, Deserialize, Default)]
pub struct RedisSection {
    #[serde(default)]
    pub cool: RedisEntry,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct RedisEntry {
    /// 空字符串表示未启用 Redis
    #[serde(default)]
    pub address: String,
    #[serde(default)]
    pub db: i64,
    #[serde(default)]
    pub password: Option<String>,
}

impl RedisEntry {
    pub fn enabled(&self) -> bool {
        !self.address.trim().is_empty()
    }
}

// ---------- cool ----------

#[derive(Debug, Clone, Deserialize)]
pub struct CoolSection {
    #[serde(default = "default_mode")]
    pub mode: String,
    #[serde(
        rename = "autoMigrate",
        alias = "auto_migrate",
        default = "default_true"
    )]
    pub auto_migrate: bool,
    #[serde(default = "default_true")]
    pub eps: bool,
    #[serde(rename = "dbRequired", alias = "db_required", default)]
    pub db_required: bool,
    #[serde(default)]
    pub file: FileSection,
}

impl Default for CoolSection {
    fn default() -> Self {
        Self {
            mode: default_mode(),
            auto_migrate: true,
            eps: true,
            db_required: false,
            file: FileSection::default(),
        }
    }
}

// ---------- file ----------

#[derive(Debug, Clone, Deserialize)]
pub struct FileSection {
    /// local | minio | oss
    #[serde(default = "default_file_mode")]
    pub mode: String,
    #[serde(default = "default_file_domain")]
    pub domain: String,
    #[serde(default)]
    pub oss: OssSection,
}

impl Default for FileSection {
    fn default() -> Self {
        Self {
            mode: default_file_mode(),
            domain: default_file_domain(),
            oss: OssSection::default(),
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct OssSection {
    #[serde(default = "default_oss_endpoint")]
    pub endpoint: String,
    #[serde(rename = "accessKeyID", alias = "access_key_id", default)]
    pub access_key_id: String,
    #[serde(rename = "secretAccessKey", alias = "secret_access_key", default)]
    pub secret_access_key: String,
    #[serde(
        rename = "bucketName",
        alias = "bucket_name",
        default = "default_oss_bucket"
    )]
    pub bucket_name: String,
    #[serde(rename = "useSSL", alias = "use_ssl", default)]
    pub use_ssl: bool,
    #[serde(default = "default_oss_location")]
    pub location: String,
}

impl Default for OssSection {
    fn default() -> Self {
        Self {
            endpoint: default_oss_endpoint(),
            access_key_id: String::new(),
            secret_access_key: String::new(),
            bucket_name: default_oss_bucket(),
            use_ssl: false,
            location: default_oss_location(),
        }
    }
}

// ---------- modules ----------

#[derive(Debug, Clone, Deserialize, Default)]
pub struct ModulesSection {
    #[serde(default)]
    pub base: BaseModuleSection,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct BaseModuleSection {
    #[serde(default)]
    pub jwt: JwtSection,
    #[serde(default)]
    pub middleware: MiddlewareSection,
}

#[derive(Debug, Clone, Deserialize)]
pub struct JwtSection {
    #[serde(default)]
    pub sso: bool,
    #[serde(default = "default_jwt_secret")]
    pub secret: String,
    /// YAML 里嵌套在 token: 下
    #[serde(default)]
    pub token: JwtTokenSection,
    /// 兼容扁平写法（YAML 无 token 嵌套时）
    #[serde(rename = "expire", alias = "expire", skip_deserializing)]
    pub _expire_flat: Option<u64>,
}

impl JwtSection {
    pub fn expire(&self) -> u64 {
        self.token.expire
    }
    pub fn refresh_expire(&self) -> u64 {
        self.token.refresh_expire
    }
}

impl Default for JwtSection {
    fn default() -> Self {
        Self {
            sso: false,
            secret: default_jwt_secret(),
            token: JwtTokenSection::default(),
            _expire_flat: None,
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct JwtTokenSection {
    #[serde(default = "default_jwt_expire")]
    pub expire: u64,
    #[serde(
        rename = "refreshExpire",
        alias = "refresh_expire",
        default = "default_jwt_refresh"
    )]
    pub refresh_expire: u64,
}

impl Default for JwtTokenSection {
    fn default() -> Self {
        Self {
            expire: default_jwt_expire(),
            refresh_expire: default_jwt_refresh(),
        }
    }
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct MiddlewareSection {
    #[serde(default)]
    pub authority: Switch,
    #[serde(default)]
    pub log: Switch,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Switch {
    #[serde(default = "default_true")]
    pub enable: bool,
}

impl Default for Switch {
    fn default() -> Self {
        Self { enable: true }
    }
}

// ---------- defaults ----------

fn default_true() -> bool {
    true
}
fn default_max_msg() -> usize {
    104_857_600
}
fn default_log_level() -> String {
    "info".into()
}
fn default_mode() -> String {
    "dev".into()
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
fn default_jwt_secret() -> String {
    "cool-base88776655".into()
}
fn default_jwt_expire() -> u64 {
    3600
}
fn default_jwt_refresh() -> u64 {
    1_296_000
}
fn default_file_mode() -> String {
    "local".into()
}
fn default_file_domain() -> String {
    "http://127.0.0.1:8002".into()
}
fn default_oss_endpoint() -> String {
    "127.0.0.1:9000".into()
}
fn default_oss_bucket() -> String {
    "cool-admin-rust".into()
}
fn default_oss_location() -> String {
    "us-east-1".into()
}

// ---------- loader ----------

impl AppConfig {
    /// 默认配置文件路径：`manifest/config/config.yaml`（相对当前工作目录）
    pub fn default_path() -> PathBuf {
        PathBuf::from("manifest/config/config.yaml")
    }

    /// 从文件加载；`COOL_CONFIG` 可指定路径。
    /// 环境变量覆盖：`COOL_SERVER__ADDRESS` 等（config crate 的 `__` 分隔）。
    pub fn load(path: Option<&Path>) -> Result<Self> {
        let path = path
            .map(|p| p.to_path_buf())
            .or_else(|| std::env::var_os("COOL_CONFIG").map(PathBuf::from))
            .unwrap_or_else(Self::default_path);

        let builder = config::Config::builder()
            .add_source(config::File::from(path.clone()).required(true))
            .add_source(
                config::Environment::with_prefix("COOL")
                    .prefix_separator("_")
                    .separator("__"),
            );

        let cfg = builder
            .build()
            .with_context(|| format!("加载配置失败: {}", path.display()))?;

        let app: AppConfig = cfg.try_deserialize().context("解析配置结构失败")?;
        Ok(app)
    }

    pub fn socket_addr(&self) -> Result<SocketAddr> {
        self.server
            .address
            .parse()
            .with_context(|| format!("非法监听地址: {}", self.server.address))
    }

    pub fn authority_enabled(&self) -> bool {
        self.modules.base.middleware.authority.enable
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

    pub fn jwt_config(&self) -> cool_core::jwt::JwtConfig {
        cool_core::jwt::JwtConfig {
            secret: self.modules.base.jwt.secret.clone(),
            expire: self.modules.base.jwt.expire(),
            refresh_expire: self.modules.base.jwt.refresh_expire(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_minimal_yaml() {
        let yaml = r#"
server:
  address: "0.0.0.0:8001"
database:
  default:
    host: "127.0.0.1"
    port: 3306
    user: root
    pass: p
    name: n
cool:
  mode: dev
"#;
        let cfg: AppConfig = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(cfg.server.address, "0.0.0.0:8001");
        assert!(cfg.cool.auto_migrate);
        assert!(cfg.authority_enabled());
    }

    #[test]
    fn parse_camel_case_binds() {
        let yaml = r#"
server:
  address: "0.0.0.0:8001"
database:
  default:
    host: 127.0.0.1
    port: 3306
    user: root
    pass: p
    name: n
redis:
  cool:
    address: "127.0.0.1:6379"
    db: 0
cool:
  autoMigrate: false
  dbRequired: true
  file:
    mode: "minio"
    domain: "http://127.0.0.1:8002"
    oss:
      accessKeyID: "ak"
      secretAccessKey: "sk"
      bucketName: "bkt"
      useSSL: true
modules:
  base:
    jwt:
      secret: "unit-test-secret-123456"
      token:
        expire: 10
        refreshExpire: 1296000
"#;
        let cfg: AppConfig = serde_yaml::from_str(yaml).unwrap();
        assert!(!cfg.cool.auto_migrate, "autoMigrate must bind");
        assert!(cfg.cool.db_required, "dbRequired must bind");
        assert_eq!(cfg.cool.file.mode, "minio");
        assert_eq!(cfg.cool.file.oss.access_key_id, "ak");
        assert_eq!(cfg.cool.file.oss.bucket_name, "bkt");
        assert!(cfg.cool.file.oss.use_ssl);
        assert!(cfg.redis.cool.enabled());
        assert_eq!(cfg.modules.base.jwt.expire(), 10);
        assert_eq!(cfg.modules.base.jwt.refresh_expire(), 1_296_000);
    }

    #[test]
    fn load_real_config_file() {
        // 从仓库根加载真实配置（cwd = workspace root when run via cargo）
        let path = Path::new("manifest/config/config.yaml");
        if !path.exists() {
            eprintln!("skip: config.yaml not found at {:?}", path);
            return;
        }
        let cfg = AppConfig::load(Some(path)).expect("load real config");
        assert_eq!(cfg.cool.file.mode, "local");
        assert!(cfg.redis.cool.enabled());
        assert_eq!(cfg.modules.base.jwt.expire(), 10, "token.expire=10 in yaml");
    }
}
