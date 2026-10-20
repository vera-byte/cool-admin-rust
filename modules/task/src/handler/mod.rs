//! TaskInfoService handler：CRUD + Start/Stop/Once/Log

use cool_core::CrudService;
use cool_core::model::FieldMap;
use cool_core::ok_code;
use cool_core::query::QueryInput;
use cool_proto::common::v1::{IdReq, IdsReq};
use cool_proto::task::v1::task_info_service_server::TaskInfoService as TaskInfoRpc;
use cool_proto::task::v1::*;
use serde_json::{Value, json};
use tonic::{Request, Response, Status};

use crate::service::TaskInfoService;

fn biz(e: cool_core::CoolError) -> Status {
    match e {
        cool_core::CoolError::Biz(_) | cool_core::CoolError::InvalidArgument(_) => {
            Status::invalid_argument(e.to_string())
        }
        cool_core::CoolError::NotFound(_) => Status::not_found(e.to_string()),
        other => Status::internal(other.to_string()),
    }
}

fn g_i64(m: &FieldMap, k: &str) -> i64 {
    m.get(k).and_then(Value::as_i64).unwrap_or_default()
}
fn g_str(m: &FieldMap, k: &str) -> String {
    m.get(k)
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string()
}

pub struct TaskInfoSvc {
    svc: TaskInfoService,
}

impl TaskInfoSvc {
    pub fn new(svc: TaskInfoService) -> Self {
        Self { svc }
    }

    fn to_proto(m: &FieldMap) -> TaskInfo {
        TaskInfo {
            id: g_i64(m, "id"),
            job_id: g_str(m, "jobId"),
            repeat_conf: g_str(m, "repeatConf"),
            name: g_str(m, "name"),
            cron: g_str(m, "cron"),
            r#limit: g_i64(m, "limit") as i32,
            every: g_i64(m, "every") as i32,
            remark: g_str(m, "remark"),
            status: g_i64(m, "status") as i32,
            start_date: g_i64(m, "startDate"),
            end_date: g_i64(m, "endDate"),
            data: g_str(m, "data"),
            service: g_str(m, "service"),
            r#type: g_i64(m, "type") as i32,
            next_run_time: g_i64(m, "nextRunTime"),
            task_type: g_i64(m, "taskType") as i32,
            create_time: g_i64(m, "createTime"),
            update_time: g_i64(m, "updateTime"),
        }
    }
}

#[tonic::async_trait]
impl TaskInfoRpc for TaskInfoSvc {
    async fn add(&self, req: Request<TaskAddReq>) -> Result<Response<TaskAddRes>, Status> {
        let r = req.into_inner();
        let mut d = FieldMap::new();
        d.insert("name".into(), json!(r.name));
        d.insert("cron".into(), json!(r.cron));
        d.insert("service".into(), json!(r.service));
        d.insert("remark".into(), json!(r.remark));
        d.insert("status".into(), json!(r.status));
        d.insert("type".into(), json!(r.r#type));
        d.insert("taskType".into(), json!(r.task_type));
        d.insert("every".into(), json!(r.every));
        if r.start_date > 0 {
            d.insert("startDate".into(), json!(r.start_date));
        }
        if r.end_date > 0 {
            d.insert("endDate".into(), json!(r.end_date));
        }
        d.insert("data".into(), json!(r.data));

        let row = self.svc.service_add(d).await.map_err(biz)?;
        let id = g_i64(&row, "id");

        // 钩子：status=1 → 集群加载调度（对齐 Go ModifyAfter）
        if r.status == 1 {
            let _ = cool_func::cluster_run_func(&format!("TaskAddTask({id})")).await;
        }

        let (code, message) = ok_code();
        Ok(Response::new(TaskAddRes {
            code,
            message,
            data: Some(Self::to_proto(&row)),
        }))
    }

    async fn delete(&self, req: Request<IdsReq>) -> Result<Response<TaskDeleteRes>, Status> {
        let ids = req.into_inner().ids;
        // 先停调度
        for id in &ids {
            let _ = cool_func::cluster_run_func(&format!("TaskStopFunc({id})")).await;
        }
        let n = self.svc.service_delete(&ids).await.map_err(biz)?;
        let (code, message) = ok_code();
        Ok(Response::new(TaskDeleteRes {
            code,
            message,
            affected: n as i64,
        }))
    }

    async fn update(&self, req: Request<TaskUpdateReq>) -> Result<Response<TaskUpdateRes>, Status> {
        let r = req.into_inner();
        let mut d = FieldMap::new();
        if let Some(v) = r.name {
            d.insert("name".into(), json!(v));
        }
        if let Some(v) = r.cron {
            d.insert("cron".into(), json!(v));
        }
        if let Some(v) = r.service {
            d.insert("service".into(), json!(v));
        }
        if let Some(v) = r.remark {
            d.insert("remark".into(), json!(v));
        }
        if let Some(v) = r.status {
            d.insert("status".into(), json!(v));
        }
        if let Some(v) = r.r#type {
            d.insert("type".into(), json!(v));
        }
        if let Some(v) = r.task_type {
            d.insert("taskType".into(), json!(v));
        }
        if let Some(v) = r.every {
            d.insert("every".into(), json!(v));
        }
        if let Some(v) = r.start_date
            && v > 0
        {
            d.insert("startDate".into(), json!(v));
        }
        if let Some(v) = r.end_date
            && v > 0
        {
            d.insert("endDate".into(), json!(v));
        }
        if let Some(v) = r.data {
            d.insert("data".into(), json!(v));
        }

        let n = self.svc.service_update(r.id, d).await.map_err(biz)?;

        // 钩子：按 status 启停（对齐 Go）
        if r.status == Some(1) {
            let _ = cool_func::cluster_run_func(&format!("TaskStartFunc({})", r.id)).await;
        } else if r.status.is_some() {
            let _ = cool_func::cluster_run_func(&format!("TaskStopFunc({})", r.id)).await;
        }

        let (code, message) = ok_code();
        Ok(Response::new(TaskUpdateRes {
            code,
            message,
            affected: n as i64,
        }))
    }

    async fn info(&self, req: Request<IdReq>) -> Result<Response<TaskInfoRes>, Status> {
        let id = req.into_inner().id;
        let row = self.svc.service_info(id).await.map_err(biz)?;
        let (code, message) = ok_code();
        Ok(Response::new(TaskInfoRes {
            code,
            message,
            data: row.as_ref().map(Self::to_proto),
        }))
    }

    async fn list(&self, req: Request<TaskListReq>) -> Result<Response<TaskListRes>, Status> {
        let r = req.into_inner();
        let mut eq = std::collections::HashMap::new();
        if let Some(s) = r.status {
            eq.insert("status".into(), json!(s));
        }
        let rows = self
            .svc
            .service_list(QueryInput {
                keyword: Some(r.keyword).filter(|s| !s.is_empty()),
                eq,
                ..Default::default()
            })
            .await
            .map_err(biz)?;
        let (code, message) = ok_code();
        Ok(Response::new(TaskListRes {
            code,
            message,
            list: rows.iter().map(Self::to_proto).collect(),
        }))
    }

    async fn page(&self, req: Request<TaskPageReq>) -> Result<Response<TaskPageRes>, Status> {
        let r = req.into_inner();
        let mut eq = std::collections::HashMap::new();
        if let Some(s) = r.status {
            eq.insert("status".into(), json!(s));
        }
        let rows = self
            .svc
            .service_page(QueryInput {
                keyword: Some(r.keyword).filter(|s| !s.is_empty()),
                eq,
                page: r.page as i64,
                size: r.size as i64,
                ..Default::default()
            })
            .await
            .map_err(biz)?;
        let (code, message) = ok_code();
        Ok(Response::new(TaskPageRes {
            code,
            message,
            total: rows.total,
            list: rows.list.iter().map(Self::to_proto).collect(),
        }))
    }

    async fn start(&self, req: Request<IdReq>) -> Result<Response<TaskOpRes>, Status> {
        let id = req.into_inner().id;
        cool_func::cluster_run_func(&format!("TaskStartFunc({id})"))
            .await
            .map_err(biz)?;
        let (code, message) = ok_code();
        Ok(Response::new(TaskOpRes { code, message }))
    }

    async fn stop(&self, req: Request<IdReq>) -> Result<Response<TaskOpRes>, Status> {
        let id = req.into_inner().id;
        cool_func::cluster_run_func(&format!("TaskStopFunc({id})"))
            .await
            .map_err(biz)?;
        let (code, message) = ok_code();
        Ok(Response::new(TaskOpRes { code, message }))
    }

    async fn once(&self, req: Request<IdReq>) -> Result<Response<TaskOpRes>, Status> {
        let id = req.into_inner().id;
        self.svc.once(id).await.map_err(biz)?;
        let (code, message) = ok_code();
        Ok(Response::new(TaskOpRes { code, message }))
    }

    async fn log(&self, req: Request<TaskLogReq>) -> Result<Response<TaskLogRes>, Status> {
        let r = req.into_inner();
        let (total, list) = self
            .svc
            .log_page(r.id, r.status, r.page as i64, r.size as i64)
            .await
            .map_err(biz)?;
        let (code, message) = ok_code();
        Ok(Response::new(TaskLogRes {
            code,
            message,
            total,
            list: list
                .iter()
                .map(|m| TaskLogEntry {
                    id: g_i64(m, "id"),
                    task_id: g_i64(m, "taskId"),
                    status: g_i64(m, "status") as i32,
                    detail: g_str(m, "detail"),
                    create_time: g_i64(m, "createTimeMs"),
                    task_name: g_str(m, "taskName"),
                })
                .collect(),
        }))
    }
}
