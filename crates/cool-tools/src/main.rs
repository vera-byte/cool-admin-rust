//! cool-tools：cool-admin-rust 开发工具。
//!
//! 对齐 apps/ 微服务架构：init / run / build / app / version

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use anyhow::{bail, Context, Result};
use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "cool-tools", version, about = "cool-admin-rust 开发工具")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// 打印版本
    Version,
    /// 创建新项目（apps/ 微服务架构脚手架）
    Init {
        /// 项目名（目录）
        name: String,
        /// 目标目录（默认当前目录）
        #[arg(long, default_value = ".")]
        dir: PathBuf,
    },
    /// 启动开发服务（cargo run）
    Run {
        /// 额外参数透传给 cargo
        #[arg(last = true)]
        args: Vec<String>,
    },
    /// 构建发布版本（cargo build --release）
    Build {
        #[arg(long)]
        release: bool,
    },
    /// 生成业务应用骨架（apps/<name>/ 自包含：proto + manifest + migrations + src）
    App {
        /// 应用名（snake_case）
        name: String,
        /// 应用根目录（默认 apps）
        #[arg(long, default_value = "apps")]
        dir: PathBuf,
    },
}

fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    let cli = Cli::parse();
    match cli.command {
        Commands::Version => {
            println!("cool-tools {}", env!("CARGO_PKG_VERSION"));
            println!("cool-admin-rust {}", env!("CARGO_PKG_VERSION"));
        }
        Commands::Init { name, dir } => init_project(&name, &dir)?,
        Commands::Run { args } => run_dev(&args)?,
        Commands::Build { release } => build(release)?,
        Commands::App { name, dir } => gen_app(&name, &dir)?,
    }
    Ok(())
}

// ---------------- init ----------------

fn init_project(name: &str, dir: &Path) -> Result<()> {
    let root = dir.join(name);
    if root.exists() {
        bail!("目录已存在: {}", root.display());
    }
    println!("创建项目 {}", root.display());

    // apps/ 微服务架构布局
    for d in [
        "proto/common/v1",
        "proto/internal/v1",
        "crates",
        "apps",
        "docs",
        "scripts",
    ] {
        std::fs::create_dir_all(root.join(d))?;
    }

    // workspace Cargo.toml
    std::fs::write(
        root.join("Cargo.toml"),
        format!(
            r#"[workspace]
resolver = "2"
members = [
    "crates/*",
    "apps/*",
]

[workspace.package]
version = "0.1.0"
edition = "2024"
license = "MIT"

[workspace.dependencies]
tokio = {{ version = "1", features = ["full"] }}
tonic = "0.12"
tonic-reflection = "0.12"
tonic-build = "0.12"
prost = "0.13"
sqlx = {{ version = "0.8", features = ["runtime-tokio", "tls-rustls", "mysql", "chrono", "migrate"] }}
tracing = "0.1"
tracing-subscriber = {{ version = "0.3", features = ["env-filter"] }}
anyhow = "1"
serde = {{ version = "1", features = ["derive"] }}
serde_json = "1"
chrono = {{ version = "0.4", features = ["serde"] }}
"#
        ),
    )?;

    // 共享 proto
    std::fs::write(
        root.join("proto/common/v1/common.proto"),
        r#"syntax = "proto3";
package common.v1;

// 业务码：1000 成功 / 1001 失败
message BaseRes {
  int32 code = 1;
  string message = 2;
}

message IdsReq {
  repeated int64 ids = 1;
}

message IdReq {
  int64 id = 1;
}

message Empty {}
"#,
    )?;

    std::fs::write(
        root.join("proto/internal/v1/internal.proto"),
        r#"syntax = "proto3";
package internal.v1;

service InternalService {
  rpc Health(HealthReq) returns (HealthRes);
}

message HealthReq {}
message HealthRes {
  int32 code = 1;
  string message = 2;
  string status = 3;
}
"#,
    )?;

    // 生成一个示例应用
    gen_app("hello", &root.join("apps"))?;

    // README
    std::fs::write(
        root.join("README.md"),
        format!(
            r#"# {name}

由 `cool-tools init` 生成的 cool-admin-rust 微服务项目。

## 目录结构

```text
apps/       # 业务应用（自包含：proto + manifest + migrations + src）
crates/     # 基础库
proto/      # 共享契约（common.v1 / internal.v1）
```

## 启动

```bash
# 生成更多应用
cool-tools app order

# 启动
MICRO_CONFIG=apps/hello/manifest/config.yaml cargo run -p app-hello
```
"#
        ),
    )?;

    println!("完成。下一步：cd {name} && cool-tools app <name>");
    Ok(())
}

// ---------------- run / build ----------------

fn run_dev(args: &[String]) -> Result<()> {
    let mut cmd = Command::new("cargo");
    cmd.arg("run").args(args);
    status(cmd, "cargo run")
}

fn build(release: bool) -> Result<()> {
    let mut cmd = Command::new("cargo");
    cmd.arg("build");
    if release {
        cmd.arg("--release");
    }
    status(cmd, "cargo build")
}

fn status(mut cmd: Command, what: &str) -> Result<()> {
    tracing::info!(%what, "exec");
    let code = cmd
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .status()
        .with_context(|| format!("执行 {what} 失败"))?;
    if !code.success() {
        bail!("{what} 退出码 {code}");
    }
    Ok(())
}

// ---------------- app ----------------

fn gen_app(name: &str, dir: &Path) -> Result<()> {
    if !name
        .chars()
        .all(|c| c.is_ascii_lowercase() || c == '_' || c.is_ascii_digit())
    {
        bail!("应用名须为 snake_case: {name}");
    }
    let pascal = name
        .split('_')
        .filter(|s| !s.is_empty())
        .map(|s| {
            let mut c = s.chars();
            match c.next() {
                Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
                None => String::new(),
            }
        })
        .collect::<String>();

    let root = dir.join(name);
    if root.exists() {
        bail!("应用已存在: {}", root.display());
    }

    // 自包含布局：proto + manifest + migrations + src(lib+bin) + tests
    for d in [
        format!("proto/{name}/v1"),
        "manifest".into(),
        "migrations".into(),
        "src/model".into(),
        "src/service".into(),
        "src/handler".into(),
        "tests".into(),
    ] {
        std::fs::create_dir_all(root.join(d))?;
    }

    // Cargo.toml（lib + bin 双目标）
    std::fs::write(
        root.join("Cargo.toml"),
        format!(
            r#"[package]
name = "app-{name}"
version.workspace = true
edition.workspace = true
license.workspace = true
description = "{name} 业务应用（模块 + 微服务）"

[lib]
name = "app_{name}"
path = "src/lib.rs"

[[bin]]
name = "{name}-service"
path = "src/main.rs"

[dependencies]
cool-proto = {{ path = "../../crates/cool-proto" }}
cool-core = {{ path = "../../crates/cool-core" }}
cool-db = {{ path = "../../crates/cool-db" }}
cool-micro = {{ path = "../../crates/cool-micro" }}
tokio = {{ workspace = true }}
tonic = {{ workspace = true }}
tonic-reflection = {{ workspace = true }}
sqlx = {{ workspace = true }}
serde = {{ workspace = true }}
serde_json = {{ workspace = true }}
chrono = {{ workspace = true }}
async-trait = "0.1"
tracing = {{ workspace = true }}
anyhow = {{ workspace = true }}
"#
        ),
    )?;

    // proto（业务契约）
    std::fs::write(
        root.join(format!("proto/{name}/v1/{name}.proto")),
        format!(
            r#"syntax = "proto3";

package {name}.v1;

import "common/v1/common.proto";

// {pascal} 服务
service {pascal}Service {{
  rpc Add({pascal}AddReq) returns ({pascal}AddRes);
  rpc Delete(common.v1.IdsReq) returns ({pascal}DeleteRes);
  rpc Update({pascal}UpdateReq) returns ({pascal}UpdateRes);
  rpc Info(common.v1.IdReq) returns ({pascal}InfoRes);
  rpc List({pascal}ListReq) returns ({pascal}ListRes);
  rpc Page({pascal}PageReq) returns ({pascal}PageRes);
}}

message {pascal} {{
  int64 id = 1;
  string name = 2;
  int64 create_time = 3;
  int64 update_time = 4;
}}

message {pascal}AddReq {{
  string name = 1;
}}
message {pascal}AddRes {{
  int32 code = 1;
  string message = 2;
  optional {pascal} data = 3;
}}
message {pascal}UpdateReq {{
  int64 id = 1;
  optional string name = 2;
}}
message {pascal}UpdateRes {{
  int32 code = 1;
  string message = 2;
  int64 affected = 3;
}}
message {pascal}DeleteRes {{
  int32 code = 1;
  string message = 2;
  int64 affected = 3;
}}
message {pascal}InfoRes {{
  int32 code = 1;
  string message = 2;
  optional {pascal} data = 3;
}}
message {pascal}ListReq {{
  string keyword = 1;
}}
message {pascal}ListRes {{
  int32 code = 1;
  string message = 2;
  repeated {pascal} list = 3;
}}
message {pascal}PageReq {{
  int32 page = 1;
  int32 size = 2;
  string keyword = 3;
}}
message {pascal}PageRes {{
  int32 code = 1;
  string message = 2;
  int64 total = 3;
  repeated {pascal} list = 4;
}}
"#
        ),
    )?;

    // manifest（应用自有配置）
    std::fs::write(
        root.join("manifest/config.yaml"),
        format!(
            r#"service:
  id: "{name}-service"
  name: "{name}"
  secret: "micro-secret-{name}-001"

server:
  address: "0.0.0.0:8005"
  reflection: true
  log_level: "info"

database:
  default:
    type: "mysql"
    host: "127.0.0.1"
    port: 3306
    user: "root"
    pass: "123456"
    name: "cooltest"

redis:
  address: "127.0.0.1:6379"
  db: 0

sa_token:
  token_name: "satoken"
  timeout: 3600
  key_prefix: "sa:"
  enable: true

cool:
  mode: "dev"
  autoMigrate: true
"#
        ),
    )?;

    // migrations（版本号按应用分段，此处用 9xxx 示例段）
    std::fs::write(
        root.join(format!("migrations/9001_{name}.sql")),
        format!(
            r#"-- {name} 应用迁移（版本号 90xx 为自定义应用预留段）
CREATE TABLE IF NOT EXISTS `{name}` (
  `id` BIGINT UNSIGNED NOT NULL AUTO_INCREMENT,
  `name` VARCHAR(255) NOT NULL DEFAULT '' COMMENT '名称',
  `createTime` DATETIME(3) NOT NULL,
  `updateTime` DATETIME(3) NOT NULL,
  `deleteTime` DATETIME(3) NULL DEFAULT NULL,
  PRIMARY KEY (`id`),
  KEY `idx_deleteTime` (`deleteTime`)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4;
"#
        ),
    )?;

    // src/lib.rs
    std::fs::write(
        root.join("src/lib.rs"),
        format!(
            r#"//! {name} 业务应用（由 cool-tools app 生成）

pub mod handler;
pub mod model;
pub mod service;

use sqlx::MySqlPool;

pub use handler::{pascal}Svc;

pub fn build_services(pool: MySqlPool) -> {pascal}Svc {{
    {pascal}Svc::new(pool)
}}

const MODULE_NAME: &str = "{name}";

/// 本模块迁移目录
fn migrations_dir() -> std::path::PathBuf {{
    std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("migrations")
}}

/// 执行本应用数据库迁移（版本号全局唯一）
pub async fn migrate(pool: &MySqlPool) -> cool_core::CoolResult<()> {{
    let mut migrator = sqlx::migrate::Migrator::new(migrations_dir())
        .await
        .map_err(|e| cool_core::CoolError::Internal(format!("加载迁移失败: {{}}", e)))?;
    migrator.set_ignore_missing(true);
    migrator
        .run(pool)
        .await
        .map_err(|e| cool_core::CoolError::Internal(format!("迁移失败: {{}}", e)))?;
    tracing::info!(target: "migrate", module = MODULE_NAME, "migrations done");
    Ok(())
}}
"#
        ),
    )?;

    // src/main.rs（微服务入口）
    std::fs::write(
        root.join("src/main.rs"),
        format!(
            r#"//! {name} 微服务入口（由 cool-tools app 生成）

use anyhow::Result;
use cool_proto::{name}::v1::{name}_service_server::{pascal}ServiceServer;
use tonic::transport::Server;
use tracing::info;

#[tokio::main]
async fn main() -> Result<()> {{
    let cfg = match cool_micro::MicroConfig::load(None) {{
        Ok(c) => c,
        Err(e) => {{
            eprintln!("配置加载失败: {{e:#}}");
            std::process::exit(1);
        }}
    }};
    cool_micro::init_tracing(&cfg.server.log_level);
    info!(service = %cfg.service.id, addr = %cfg.server.address, "starting {name}-service");

    let pool = cool_db::connect(&cfg.db_config()).await?;
    if cfg.cool.auto_migrate {{
        app_{name}::migrate(&pool).await?;
    }}
    let session = cool_micro::init_distributed(&cfg).await?;

    let svc = app_{name}::build_services(pool.clone());

    let mut router = Server::builder()
        .layer(cool_micro::ServiceAuthLayer::new(
            cfg.sa_token.enable,
            session.state.clone(),
            session.dsm.clone(),
        ))
        .add_service({pascal}ServiceServer::new(svc));

    if cfg.server.reflection {{
        let refl = tonic_reflection::server::Builder::configure()
            .register_encoded_file_descriptor_set(cool_proto::REFLECTION_DESCRIPTOR)
            .build_v1()
            .map_err(|e| anyhow::anyhow!("{{e}}"))?;
        router = router.add_service(refl);
    }}

    let addr = cfg.socket_addr()?;
    info!(%addr, "{name}-service listening");
    router.serve(addr).await?;
    Ok(())
}}
"#
        ),
    )?;

    // src/model
    std::fs::write(
        root.join("src/model/mod.rs"),
        format!(
            r#"pub mod {name};

pub use {name}::{pascal};
"#
        ),
    )?;
    std::fs::write(
        root.join(format!("src/model/{name}.rs")),
        format!(
            r#"use cool_core::model::{{ColumnMeta, Model}};

pub struct {pascal};

const COLS: &[ColumnMeta] = &[
    ColumnMeta::read("id"),
    ColumnMeta::rw("name"),
    ColumnMeta::computed("createTime"),
    ColumnMeta::computed("updateTime"),
    ColumnMeta::computed("deleteTime"),
];

impl Model for {pascal} {{
    fn table_name() -> &'static str {{
        "{name}"
    }}
    fn columns() -> &'static [ColumnMeta] {{
        COLS
    }}
}}
"#
        ),
    )?;

    // src/service
    std::fs::write(
        root.join("src/service/mod.rs"),
        format!(
            r#"use std::collections::HashMap;

use cool_core::query::QueryOp;
use cool_core::repo::{{CrudConfig, CrudRepo}};
use cool_core::CrudService;
use sqlx::MySqlPool;

use crate::model::{pascal};

pub struct {pascal}Service {{
    repo: CrudRepo<{pascal}>,
}}

pub fn new_{name}_service(pool: MySqlPool) -> {pascal}Service {{
    let config = CrudConfig {{
        not_null_key: {{
            let mut m = HashMap::new();
            m.insert("name".into(), "名称不能为空".into());
            m
        }},
        list_query_op: QueryOp {{
            key_word_field: vec!["name".into()],
            ..Default::default()
        }},
        page_query_op: QueryOp {{
            key_word_field: vec!["name".into()],
            ..Default::default()
        }},
        ..Default::default()
    }};
    {pascal}Service {{ repo: CrudRepo::<{pascal}>::new(pool, config) }}
}}

#[async_trait::async_trait]
impl CrudService for {pascal}Service {{
    type Model = {pascal};
    fn repo(&self) -> &CrudRepo<Self::Model> {{ &self.repo }}
}}
"#
        ),
    )?;

    // src/handler
    std::fs::write(
        root.join("src/handler/mod.rs"),
        format!(
            r#"//! {pascal} gRPC Handler

use cool_core::ok_code;
use cool_core::CrudService;
use cool_proto::{name}::v1::*;
use sqlx::MySqlPool;
use tonic::{{Request, Response, Status}};

use crate::service::{{new_{name}_service, {pascal}Service}};

pub struct {pascal}Svc {{
    svc: {pascal}Service,
}}

impl {pascal}Svc {{
    pub fn new(pool: MySqlPool) -> Self {{
        Self {{ svc: new_{name}_service(pool) }}
    }}
}}

#[tonic::async_trait]
impl {name}_service_server::{pascal}Service for {pascal}Svc {{
    async fn add(&self, req: Request<{pascal}AddReq>) -> Result<Response<{pascal}AddRes>, Status> {{
        let r = req.into_inner();
        let mut d = cool_core::FieldMap::new();
        d.insert("name".into(), serde_json::json!(r.name));
        let row = self.svc.service_add(d).await.map_err(|e| Status::internal(e.to_string()))?;
        let (code, message) = ok_code();
        Ok(Response::new({pascal}AddRes {{
            code,
            message,
            data: Some({pascal} {{
                id: row.get("id").and_then(|v| v.as_i64()).unwrap_or_default(),
                name: row.get("name").and_then(|v| v.as_str()).unwrap_or_default().to_string(),
                create_time: 0,
                update_time: 0,
            }}),
        }}))
    }}

    async fn delete(&self, req: Request<cool_proto::common::v1::IdsReq>) -> Result<Response<{pascal}DeleteRes>, Status> {{
        let ids = req.into_inner().ids;
        let n = self.svc.service_delete(&ids).await.map_err(|e| Status::internal(e.to_string()))?;
        let (code, message) = ok_code();
        Ok(Response::new({pascal}DeleteRes {{ code, message, affected: n as i64 }}))
    }}

    async fn update(&self, _req: Request<{pascal}UpdateReq>) -> Result<Response<{pascal}UpdateRes>, Status> {{
        // TODO: 实现更新
        let (code, message) = ok_code();
        Ok(Response::new({pascal}UpdateRes {{ code, message, affected: 0 }}))
    }}

    async fn info(&self, _req: Request<cool_proto::common::v1::IdReq>) -> Result<Response<{pascal}InfoRes>, Status> {{
        let (code, message) = ok_code();
        Ok(Response::new({pascal}InfoRes {{ code, message, data: None }}))
    }}

    async fn list(&self, _req: Request<{pascal}ListReq>) -> Result<Response<{pascal}ListRes>, Status> {{
        let (code, message) = ok_code();
        Ok(Response::new({pascal}ListRes {{ code, message, list: vec![] }}))
    }}

    async fn page(&self, _req: Request<{pascal}PageReq>) -> Result<Response<{pascal}PageRes>, Status> {{
        let (code, message) = ok_code();
        Ok(Response::new({pascal}PageRes {{ code, message, total: 0, list: vec![] }}))
    }}
}}
"#
        ),
    )?;

    println!("生成应用 {}", root.display());
    println!("后续步骤：");
    println!("  1) workspace Cargo.toml 成员加入 apps/{name}（通配 apps/* 已覆盖）");
    println!("  2) crates/cool-proto/build.rs 会自动聚合 apps/*/proto");
    println!("  3) 启动：MICRO_CONFIG=apps/{name}/manifest/config.yaml cargo run -p app-{name}");
    Ok(())
}
