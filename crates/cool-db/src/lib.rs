//! 数据层：SQLx 连接池与迁移（决策 D3：迁移 SQL 管理表结构）。

use std::time::Duration;

use cool_core::{CoolError, CoolResult};
use sqlx::mysql::MySqlPoolOptions;
use sqlx::{MySql, Pool};
use tracing::{info, warn};

/// 当前仅支持 MySQL；pgsql/sqlite 以 feature 扩展（见方案 §8）
pub type DbPool = Pool<MySql>;

/// 数据库连接配置（对齐 manifest/config/config.yaml 的 database.default）
#[derive(Debug, Clone)]
pub struct DbConfig {
    pub host: String,
    pub port: u16,
    pub user: String,
    pub pass: String,
    pub name: String,
    pub max_connections: u32,
    pub min_connections: u32,
    pub connect_timeout_secs: u64,
}

impl Default for DbConfig {
    fn default() -> Self {
        Self {
            host: "127.0.0.1".into(),
            port: 3306,
            user: "root".into(),
            pass: "123456".into(),
            name: "cooltest".into(),
            max_connections: 20,
            min_connections: 1,
            connect_timeout_secs: 5,
        }
    }
}

impl DbConfig {
    pub fn mysql_url(&self) -> String {
        format!(
            "mysql://{}:{}@{}:{}/{}",
            self.user, self.pass, self.host, self.port, self.name
        )
    }
}

/// 建立连接池（带超时，失败返回错误由上层决定是否降级）
pub async fn connect(cfg: &DbConfig) -> CoolResult<DbPool> {
    let url = cfg.mysql_url();
    info!(host = %cfg.host, db = %cfg.name, "connecting database");

    let pool = MySqlPoolOptions::new()
        .max_connections(cfg.max_connections)
        .min_connections(cfg.min_connections)
        .acquire_timeout(Duration::from_secs(cfg.connect_timeout_secs))
        .connect(&url)
        .await
        .map_err(|e| CoolError::Internal(format!("数据库连接失败: {e}")))?;

    Ok(pool)
}

/// 迁移由各业务模块自行执行（`mod_x::migrate`），版本号全局唯一。
/// 本函数保留签名兼容旧调用，实际委托给模块层后不应再使用。
#[deprecated(note = "迁移归各模块：调用 mod_x::migrate(pool)")]
pub async fn migrate(_pool: &DbPool) -> CoolResult<()> {
    warn!("cool_db::migrate 已废弃，请改用各模块的 migrate()");
    Ok(())
}

/// 尽力初始化：失败仅告警，返回 None（保证 Hello/Health 无 DB 也能起）。
/// 生产环境建议将 cool.dbRequired 置 true 以强制失败退出。
pub async fn try_init(cfg: &DbConfig, _auto_migrate: bool) -> Option<DbPool> {
    match connect(cfg).await {
        Ok(pool) => {
            Some(pool)
        }
        Err(e) => {
            warn!("database unavailable, continue without db: {e}");
            None
        }
    }
}

/// 探活
pub async fn healthy(pool: Option<&DbPool>) -> bool {
    match pool {
        Some(p) => sqlx::query("SELECT 1").execute(p).await.is_ok(),
        None => false,
    }
}
