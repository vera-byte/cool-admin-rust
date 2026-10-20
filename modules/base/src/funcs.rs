//! BaseFuncClearLog(all)：清理过期操作日志（对齐 Go base 模块）。
//!
//! - `all=true`：清空全部
//! - `all=false`：删除 logKeep 天前的日志（base_sys_conf.logKeep，默认 7）
//!
//! `is_singleton=true`、`is_all_worker=false` —— 仅 master 执行，调度器防重叠。

use cool_core::CoolResult;
use cool_func::CoolFunc;
use sqlx::MySqlPool;

pub struct BaseFuncClearLog {
    pool: MySqlPool,
}

impl BaseFuncClearLog {
    pub fn new(pool: MySqlPool) -> Self {
        Self { pool }
    }

    async fn log_keep_days(&self) -> i64 {
        let v: Option<String> =
            sqlx::query_scalar("SELECT value FROM base_sys_conf WHERE `key` = 'logKeep' LIMIT 1")
                .fetch_optional(&self.pool)
                .await
                .ok()
                .flatten();
        v.and_then(|s| s.parse().ok()).unwrap_or(7)
    }
}

#[async_trait::async_trait]
impl CoolFunc for BaseFuncClearLog {
    async fn func(&self, param: &str) -> CoolResult<()> {
        let all = param.trim().eq_ignore_ascii_case("true") || param.trim() == "1";
        if all {
            let n =
                sqlx::query("UPDATE base_sys_log SET deleteTime = NOW(3) WHERE deleteTime IS NULL")
                    .execute(&self.pool)
                    .await
                    .map_err(|e| cool_core::CoolError::Internal(format!("清空日志失败: {e}")))?
                    .rows_affected();
            tracing::info!(target: "base.func", n, "clear all logs");
        } else {
            let days = self.log_keep_days().await;
            let n = sqlx::query(
                "UPDATE base_sys_log SET deleteTime = NOW(3) \
                 WHERE deleteTime IS NULL AND createTime < NOW(3) - INTERVAL ? DAY",
            )
            .bind(days)
            .execute(&self.pool)
            .await
            .map_err(|e| cool_core::CoolError::Internal(format!("清理日志失败: {e}")))?
            .rows_affected();
            tracing::info!(target: "base.func", n, days, "clear expired logs");
        }
        Ok(())
    }

    fn is_singleton(&self) -> bool {
        true
    }

    fn is_all_worker(&self) -> bool {
        false
    }
}

/// 注册到全局函数表
pub fn register(pool: MySqlPool) {
    cool_func::register(
        "BaseFuncClearLog",
        std::sync::Arc::new(BaseFuncClearLog::new(pool)),
    );
}
