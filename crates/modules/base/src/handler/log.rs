//! BaseSysLogService：操作日志（只读）

use cool_core::error::CoolError;
use cool_core::ok_code;
use cool_proto::base::v1::base_sys_log_service_server::BaseSysLogService;
use cool_proto::base::v1::{LogPageReq, LogPageRes, SysLog};
use sqlx::{MySqlPool, Row};
use tonic::{Request, Response, Status};

use super::biz_err;

pub struct BaseSysLogSvc {
    pool: MySqlPool,
}

impl BaseSysLogSvc {
    pub fn new(pool: MySqlPool) -> Self {
        Self { pool }
    }
}

#[tonic::async_trait]
impl BaseSysLogService for BaseSysLogSvc {
    async fn page(&self, req: Request<LogPageReq>) -> Result<Response<LogPageRes>, Status> {
        let r = req.into_inner();
        let page = if r.page <= 0 { 1 } else { r.page as i64 };
        let size = if r.size <= 0 { 15 } else { r.size as i64 };
        let offset = (page - 1) * size;

        let mut wheres = vec!["deleteTime IS NULL".to_string()];
        let mut args: Vec<String> = Vec::new();
        if let Some(uid) = r.user_id {
            wheres.push("userId = ?".into());
            args.push(uid.to_string());
        }
        if !r.keyword.trim().is_empty() {
            wheres.push("(username LIKE ? OR action LIKE ? OR method LIKE ?)".into());
            let like = format!("%{}%", r.keyword.trim());
            args.push(like.clone());
            args.push(like.clone());
            args.push(like);
        }
        let where_sql = wheres.join(" AND ");

        let count_sql = format!("SELECT COUNT(*) FROM base_sys_log WHERE {where_sql}");
        let mut cq = sqlx::query_scalar::<_, i64>(&count_sql);
        for a in &args {
            cq = cq.bind(a);
        }
        let total = cq
            .fetch_one(&self.pool)
            .await
            .map_err(|e| CoolError::Internal(format!("统计失败: {e}")))
            .map_err(biz_err)?;

        let sql = format!(
            "SELECT id, userId, username, action, method, params, ip, result, createTime \
             FROM base_sys_log WHERE {where_sql} ORDER BY id DESC LIMIT {size} OFFSET {offset}"
        );
        let mut q = sqlx::query(&sql);
        for a in &args {
            q = q.bind(a);
        }
        let rows = q
            .fetch_all(&self.pool)
            .await
            .map_err(|e| CoolError::Internal(format!("查询失败: {e}")))
            .map_err(biz_err)?;

        let list = rows
            .iter()
            .map(|r| SysLog {
                id: r.try_get("id").unwrap_or_default(),
                user_id: r.try_get("userId").unwrap_or_default(),
                username: r.try_get("username").unwrap_or_default(),
                action: r.try_get("action").unwrap_or_default(),
                method: r.try_get("method").unwrap_or_default(),
                params: r.try_get("params").unwrap_or_default(),
                ip: r.try_get("ip").unwrap_or_default(),
                result: r.try_get("result").unwrap_or_default(),
                create_time: 0,
            })
            .collect();

        let (code, message) = ok_code();
        Ok(Response::new(LogPageRes {
            code,
            message,
            total,
            list,
        }))
    }
}
