//! cool-tools：cool-admin-rust 开发工具。
//!
//! 对齐 cool-admin-go/cool-tools：init / run / build / module / version。

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use anyhow::{Context, Result, bail};
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
    /// 创建新项目（脚手架）
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
    /// 生成模块骨架
    Module {
        /// 模块名（snake_case）
        name: String,
        /// 模块所在目录（默认 modules）
        #[arg(long, default_value = "modules")]
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
        Commands::Module { name, dir } => gen_module(&name, &dir)?,
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

    std::fs::create_dir_all(root.join("proto"))?;
    std::fs::create_dir_all(root.join("migrations"))?;
    std::fs::create_dir_all(root.join("manifest/config"))?;
    std::fs::create_dir_all(root.join("src"))?;

    // 顶层 Cargo.toml（最小可运行骨架）
    std::fs::write(
        root.join("Cargo.toml"),
        format!(
            r#"[package]
name = "{name}"
version = "0.1.0"
edition = "2024"

[[bin]]
name = "{name}"
path = "src/main.rs"

[dependencies]
cool-proto = {{ version = "0.1.0" }}
cool-core = {{ version = "0.1.0" }}
cool-db = {{ version = "0.1.0" }}
tokio = {{ version = "1", features = ["full"] }}
tonic = "0.12"
tonic-reflection = "0.12"
tracing = "0.1"
tracing-subscriber = {{ version = "0.3", features = ["env-filter"] }}
anyhow = "1"
"#
        ),
    )?;

    std::fs::write(
        root.join("src/main.rs"),
        r#"//! cool-admin-rust 脚手架入口（由 cool-tools init 生成）

fn main() {
    tracing_subscriber::fmt().init();
    tracing::info!("hello from cool-admin-rust project");
    // TODO: 装配 grpc 服务（参考 services/server/src/main.rs）
}
"#,
    )?;

    std::fs::write(
        root.join("manifest/config/config.yaml"),
        r#"server:
  address: "0.0.0.0:8001"
  reflection: true

database:
  default:
    type: "mysql"
    host: "127.0.0.1"
    port: 3306
    user: "root"
    pass: "123456"
    name: "cooltest"

cool:
  mode: "dev"
  autoMigrate: true
"#,
    )?;

    std::fs::write(
        root.join("README.md"),
        format!(
            r#"# {name}

由 `cool-tools init` 生成的 cool-admin-rust 项目骨架。

## 启动

```bash
cd {name}
cargo run
```

参照 [cool-admin-rust](../cool-admin-rust) 补齐 modules / proto。
"#
        ),
    )?;

    println!("完成。下一步：cd {name} && cargo run");
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

// ---------------- module ----------------

fn gen_module(name: &str, dir: &Path) -> Result<()> {
    if !name.chars().all(|c| c.is_ascii_lowercase() || c == '_') {
        bail!("模块名须为 snake_case: {name}");
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
        bail!("模块已存在: {}", root.display());
    }
    std::fs::create_dir_all(root.join("src/model"))?;
    std::fs::create_dir_all(root.join("src/service"))?;
    std::fs::create_dir_all(root.join("src/handler"))?;

    std::fs::write(
        root.join("Cargo.toml"),
        format!(
            r#"[package]
name = "mod-{name}"
version.workspace = true
edition.workspace = true
license.workspace = true

[dependencies]
cool-proto = {{ workspace = true }}
cool-core = {{ workspace = true }}
cool-db = {{ workspace = true }}
tokio = {{ workspace = true }}
tonic = {{ workspace = true }}
sqlx = {{ workspace = true }}
serde_json = {{ workspace = true }}
async-trait = "0.1"
tracing = {{ workspace = true }}
"#
        ),
    )?;

    std::fs::write(
        root.join("src/lib.rs"),
        format!(
            r#"//! {name} 模块（由 cool-tools module 生成）

pub mod handler;
pub mod model;
pub mod service;

pub use handler::{pascal}Svc;

use sqlx::MySqlPool;

pub fn build_services(pool: MySqlPool) -> {pascal}Svc {{
    {pascal}Svc::new(pool)
}}
"#
        ),
    )?;

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

    std::fs::write(
        root.join("src/handler/mod.rs"),
        format!(
            r#"//! {pascal} handler 骨架

use sqlx::MySqlPool;

pub struct {pascal}Svc;

impl {pascal}Svc {{
    pub fn new(_pool: MySqlPool) -> Self {{ Self }}
}}

// TODO: 实现 cool_proto::{name}.v1::{pascal}Service
"#
        ),
    )?;

    println!("生成模块 {}", root.display());
    println!("记得：1) 把 mod-{name} 加入 workspace members");
    println!("     2) 在 proto/{name}/v1/ 添加 .proto 并登记 build.rs");
    println!("     3) 在 server main.rs 注册服务");
    Ok(())
}
