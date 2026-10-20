//! 任务服务：record / once / log / set_next_run_time + 生命周期

use std::collections::HashMap;
use std::sync::Arc;

use chrono::{DateTime, TimeZone, Utc};
use cool_core::CrudService;
use cool_core::error::{CoolError, CoolResult};
use cool_core::repo::{CrudConfig, CrudRepo};
use serde_json::json;
use sqlx::{MySqlPool, Row};

use crate::model::TaskInfo;
use crate::scheduler::{self, CronSpec};

pub struct TaskInfoService {
    repo: CrudRepo<TaskInfo>,
    pool: MySqlPool,
}

impl TaskInfoService {
    pub fn new(pool: MySqlPool) -> Self {
        let config = CrudConfig {
            unique_key: {
                let mut m = HashMap::new();
                m.insert("name".into(), "任务名称不能重复".into());
                m
            },
            not_null_key: {
                let mut m = HashMap::new();
                m.insert("name".into(), "任务名称不能为空".into());
                m
            },
            list_query_op: service_query(),
            page_query_op: service_query(),
            ..Default::default()
        };
        Self {
            repo: CrudRepo::<TaskInfo>::new(pool.clone(), config),
            pool,
        }
    }

    pub fn pool(&self) -> &MySqlPool {
        &self.pool
    }

    /// 启动引导：status=1 的任务加载进调度器（本地）
    pub async fn bootstrap(&self) {
        let rows = sqlx::query("SELECT id FROM task_info WHERE status = 1 AND deleteTime IS NULL")
            .fetch_all(&self.pool)
            .await
            .unwrap_or_default();
        for r in rows {
            let id: i64 = r.try_get::<u64, _>("id").unwrap_or_default() as i64;
            if let Err(e) = self.add_task(id).await {
                tracing::warn!(target: "task", id, "bootstrap failed: {e}");
            }
        }
    }

    /// 加载任务到调度器（对齐 TaskAddTask）
    pub async fn add_task(&self, id: i64) -> CoolResult<()> {
        let row = self.load(id).await?;
        self.enable_from_row(&row).await
    }

    /// 启用（对齐 TaskStartFunc）：status=1 + 调度
    pub async fn start_task(&self, id: i64) -> CoolResult<()> {
        sqlx::query("UPDATE task_info SET status = 1, updateTime = NOW(3) WHERE id = ? AND deleteTime IS NULL")
            .bind(id)
            .execute(&self.pool)
            .await
            .map_err(|e| CoolError::Internal(format!("更新状态失败: {e}")))?;
        let row = self.load(id).await?;
        self.enable_from_row(&row).await
    }

    /// 停用（对齐 TaskStopFunc）
    pub async fn stop_task(&self, id: i64) -> CoolResult<()> {
        sqlx::query("UPDATE task_info SET status = 0, updateTime = NOW(3) WHERE id = ? AND deleteTime IS NULL")
            .bind(id)
            .execute(&self.pool)
            .await
            .map_err(|e| CoolError::Internal(format!("更新状态失败: {e}")))?;
        scheduler::disable_task(&id.to_string());
        Ok(())
    }

    async fn load(&self, id: i64) -> CoolResult<HashMap<String, serde_json::Value>> {
        let row = sqlx::query(
            "SELECT id, name, cron, service, status, startDate, every, taskType FROM task_info \
             WHERE id = ? AND deleteTime IS NULL",
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| CoolError::Internal(format!("查询任务失败: {e}")))?
        .ok_or_else(|| CoolError::biz("任务不存在"))?;

        let mut m = HashMap::new();
        m.insert(
            "id".into(),
            json!(row.try_get::<u64, _>("id").unwrap_or_default() as i64),
        );
        m.insert(
            "name".into(),
            json!(row.try_get::<String, _>("name").unwrap_or_default()),
        );
        m.insert(
            "cron".into(),
            json!(row.try_get::<String, _>("cron").unwrap_or_default()),
        );
        m.insert(
            "service".into(),
            json!(row.try_get::<String, _>("service").unwrap_or_default()),
        );
        m.insert(
            "status".into(),
            json!(row.try_get::<i32, _>("status").unwrap_or_default()),
        );
        m.insert(
            "every".into(),
            json!(row.try_get::<i32, _>("every").unwrap_or_default()),
        );
        m.insert(
            "taskType".into(),
            json!(row.try_get::<i32, _>("taskType").unwrap_or_default()),
        );
        // startDate 可能是 DATETIME
        let sd: Option<chrono::NaiveDateTime> = row.try_get("startDate").ok().flatten();
        m.insert(
            "startDateMs".into(),
            json!(sd.map(|d| d.and_utc().timestamp_millis()).unwrap_or(0)),
        );
        Ok(m)
    }

    async fn enable_from_row(&self, row: &HashMap<String, serde_json::Value>) -> CoolResult<()> {
        let id = row.get("id").and_then(|v| v.as_i64()).unwrap_or_default();
        let funcstring = row
            .get("service")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_string();
        let cron = row
            .get("cron")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_string();
        let task_type = row
            .get("taskType")
            .and_then(|v| v.as_i64())
            .unwrap_or_default();
        let every = row
            .get("every")
            .and_then(|v| v.as_i64())
            .unwrap_or_default();
        let start_ms = row
            .get("startDateMs")
            .and_then(|v| v.as_i64())
            .unwrap_or_default();

        if funcstring.trim().is_empty() {
            return Err(CoolError::biz("任务未配置函数(service)"));
        }

        // 校验函数存在（对齐 Go EnableTask）
        let (fname, _) = cool_func::parse_funcstring(&funcstring);
        if cool_func::get(&fname).is_none() {
            return Err(CoolError::biz(format!("函数不存在:{fname}")));
        }

        let spec = if task_type == 1 && every > 0 {
            CronSpec::Every(std::time::Duration::from_millis(every as u64))
        } else {
            scheduler::parse_cron(&cron)?
        };

        let start_date = if start_ms > 0 {
            Utc.timestamp_millis_opt(start_ms).single()
        } else {
            None
        };

        // singleton 需要知道函数属性
        let f = cool_func::get(&fname);
        let singleton = f.as_ref().map(|f| f.is_singleton()).unwrap_or(false);

        let pool = self.pool.clone();
        let id2 = id;
        scheduler::enable_task(id, spec, start_date, singleton, move || {
            let pool = pool.clone();
            let funcstring = funcstring.clone();
            Box::pin(async move {
                let svc = TaskInfoService::new(pool);
                let r = cool_func::run_func(&funcstring).await;
                match &r {
                    Ok(_) => svc.record(id2, 1, "ok").await,
                    Err(e) => svc.record(id2, 0, &e.to_string()).await,
                };
                r
            })
        });

        Ok(())
    }

    /// 记录日志 + 成功日志保留最新 20 条（对齐 Go Record）
    pub async fn record(&self, task_id: i64, status: i32, detail: &str) {
        if let Err(e) = sqlx::query(
            "INSERT INTO task_log (taskId, status, detail, createTime, updateTime) VALUES (?, ?, ?, NOW(3), NOW(3))",
        )
        .bind(task_id)
        .bind(status)
        .bind(detail)
        .execute(&self.pool)
        .await
        {
            tracing::warn!(target: "task", "record log failed: {e}");
            return;
        }
        if status == 1 {
            // 找第 20 条成功日志 id，小于它的软删
            let cutoff: Option<u64> = sqlx::query_scalar(
                "SELECT id FROM task_log WHERE taskId = ? AND status = 1 AND deleteTime IS NULL \
                 ORDER BY id DESC LIMIT 1 OFFSET 19",
            )
            .bind(task_id)
            .fetch_optional(&self.pool)
            .await
            .unwrap_or(None);
            if let Some(cut) = cutoff {
                let _ = sqlx::query(
                    "UPDATE task_log SET deleteTime = NOW(3) WHERE taskId = ? AND status = 1 \
                     AND deleteTime IS NULL AND id < ?",
                )
                .bind(task_id)
                .bind(cut as i64)
                .execute(&self.pool)
                .await;
            }
        }
    }

    /// 立即执行一次（对齐 Once）
    pub async fn once(&self, id: i64) -> CoolResult<()> {
        let row = self.load(id).await?;
        let funcstring = row
            .get("service")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_string();
        if funcstring.trim().is_empty() {
            return Err(CoolError::biz("任务未配置函数(service)"));
        }
        cool_func::cluster_run_func(&funcstring).await
    }

    /// 写 nextRunTime
    pub async fn set_next_run_time(&self, id: i64, cron: &str) -> CoolResult<()> {
        if let Ok(spec) = scheduler::parse_cron(cron)
            && let Some(t) = scheduler::next_run_time(&spec, Utc::now())
        {
            let naive = t.naive_utc();
            let _ = sqlx::query(
                "UPDATE task_info SET nextRunTime = ? WHERE id = ? AND deleteTime IS NULL",
            )
            .bind(naive)
            .bind(id)
            .execute(&self.pool)
            .await;
        }
        Ok(())
    }

    /// 任务日志分页（LEFT JOIN task_info）
    pub async fn log_page(
        &self,
        id: Option<i64>,
        status: Option<i32>,
        page: i64,
        size: i64,
    ) -> CoolResult<(i64, Vec<HashMap<String, serde_json::Value>>)> {
        let mut wheres = vec!["l.deleteTime IS NULL".to_string()];
        let mut args: Vec<String> = Vec::new();
        if let Some(i) = id {
            wheres.push("l.taskId = ?".into());
            args.push(i.to_string());
        }
        if let Some(s) = status {
            wheres.push("l.status = ?".into());
            args.push(s.to_string());
        }
        let where_sql = wheres.join(" AND ");

        let count_sql = format!("SELECT COUNT(*) FROM task_log l WHERE {where_sql}");
        let mut cq = sqlx::query_scalar::<_, i64>(&count_sql);
        for a in &args {
            cq = cq.bind(a);
        }
        let total = cq
            .fetch_one(&self.pool)
            .await
            .map_err(|e| CoolError::Internal(format!("统计失败: {e}")))?;

        let page = page.max(1);
        let size = if size <= 0 { 15 } else { size };
        let offset = (page - 1) * size;
        let sql = format!(
            "SELECT l.id, l.taskId, l.status, l.detail, l.createTime, i.name AS taskName \
             FROM task_log l LEFT JOIN task_info i ON i.id = l.taskId \
             WHERE {where_sql} ORDER BY l.id DESC LIMIT {size} OFFSET {offset}"
        );
        let mut q = sqlx::query(&sql);
        for a in &args {
            q = q.bind(a);
        }
        let rows = q
            .fetch_all(&self.pool)
            .await
            .map_err(|e| CoolError::Internal(format!("查询日志失败: {e}")))?;

        let list = rows
            .iter()
            .map(|r| {
                let mut m = HashMap::new();
                m.insert(
                    "id".into(),
                    json!(r.try_get::<u64, _>("id").unwrap_or_default() as i64),
                );
                m.insert(
                    "taskId".into(),
                    json!(r.try_get::<u64, _>("taskId").unwrap_or_default() as i64),
                );
                m.insert(
                    "status".into(),
                    json!(r.try_get::<i32, _>("status").unwrap_or_default()),
                );
                m.insert(
                    "detail".into(),
                    json!(r.try_get::<String, _>("detail").unwrap_or_default()),
                );
                m.insert(
                    "createTimeMs".into(),
                    json!(
                        r.try_get::<Option<chrono::NaiveDateTime>, _>("createTime")
                            .unwrap_or(None)
                            .map(|d| d.and_utc().timestamp_millis())
                            .unwrap_or(0)
                    ),
                );
                m.insert(
                    "taskName".into(),
                    json!(
                        r.try_get::<Option<String>, _>("taskName")
                            .unwrap_or(None)
                            .unwrap_or_default()
                    ),
                );
                m
            })
            .collect();
        Ok((total, list))
    }
}

fn _unused(_: DateTime<Utc>) {}

pub fn service_query() -> cool_core::query::QueryOp {
    cool_core::query::QueryOp {
        field_eq: vec!["status".into(), "type".into()],
        key_word_field: vec!["name".into()],
        add_orderby: {
            let mut m = std::collections::HashMap::new();
            m.insert("id".into(), "desc".into());
            m
        },
        ..Default::default()
    }
}

#[async_trait::async_trait]
impl CrudService for TaskInfoService {
    type Model = TaskInfo;
    fn repo(&self) -> &CrudRepo<Self::Model> {
        &self.repo
    }
}

// 供 funcs 用的 Arc 包装已在 funcs.rs
pub type SharedTaskService = Arc<TaskInfoService>;
