//! DemoSample gRPC Handler（对齐 cool-admin-go controller：Api 白名单 + 路由）

use std::collections::HashMap;

use cool_core::model::FieldMap;
use cool_core::query::QueryInput;
use cool_core::repo::PageResult;
use cool_core::{CoolError, CrudService, ok_code};
use cool_proto::common::v1::{IdReq, IdsReq};
use cool_proto::demo::v1::demo_sample_service_server::DemoSampleService as DemoSampleRpc;
use cool_proto::demo::v1::{
    DemoSample, DemoSampleAddReq, DemoSampleAddRes, DemoSampleDeleteRes, DemoSampleInfoRes,
    DemoSampleListReq, DemoSampleListRes, DemoSamplePageReq, DemoSamplePageRes,
    DemoSampleUpdateReq, DemoSampleUpdateRes, DemoSampleWelcomeReq, DemoSampleWelcomeRes,
    WelcomeData,
};
use serde_json::{Value, json};
use tonic::{Request, Response, Status};

use crate::service::DemoSampleService;

/// Api 白名单（对齐 Go `Api: []string{"Add","Delete","Update","Info","List","Page"}`）
pub const API_WHITELIST: &[&str] = &["Add", "Delete", "Update", "Info", "List", "Page", "Welcome"];

/// gRPC 服务包装
pub struct DemoSampleSvc {
    svc: DemoSampleService,
}

impl DemoSampleSvc {
    pub fn new(svc: DemoSampleService) -> Self {
        Self { svc }
    }

    #[allow(clippy::result_large_err)] // tonic::Status 体积固定，返回 Result<(), Status> 为 gRPC 惯例
    fn check_api(&self, method: &str) -> Result<(), Status> {
        if API_WHITELIST.contains(&method) {
            Ok(())
        } else {
            Err(Status::not_found(format!("接口未开放: {method}")))
        }
    }
}

// ---------- 映射：FieldMap ↔ proto ----------

fn map_to_proto(m: &FieldMap) -> DemoSample {
    DemoSample {
        id: m.get("id").and_then(Value::as_i64).unwrap_or_default(),
        name: m
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string(),
        remark: m
            .get("remark")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string(),
        create_time: time_millis(m.get("createTime")),
        update_time: time_millis(m.get("updateTime")),
    }
}

fn time_millis(v: Option<&Value>) -> i64 {
    match v {
        Some(Value::Number(n)) => n.as_i64().unwrap_or_default(),
        Some(Value::String(s)) => s.parse().unwrap_or_default(),
        _ => 0,
    }
}

fn ok_headers() -> (i32, String) {
    ok_code()
}

fn biz_err(e: CoolError) -> Status {
    // 业务错误也可按需映射为响应体 code=1001；此处统一为传输错误便于客户端区分
    match e {
        CoolError::Biz(_) => Status::invalid_argument(e.to_string()),
        CoolError::InvalidArgument(_) => Status::invalid_argument(e.to_string()),
        CoolError::Unauthenticated(_) => Status::unauthenticated(e.to_string()),
        CoolError::PermissionDenied(_) => Status::permission_denied(e.to_string()),
        CoolError::NotFound(_) => Status::not_found(e.to_string()),
        other => Status::internal(other.to_string()),
    }
}

fn to_query_input(keyword: &str, order: &str, sort: &str, name: Option<String>) -> QueryInput {
    let mut eq = HashMap::new();
    if let Some(n) = name {
        eq.insert("name".into(), json!(n));
    }
    QueryInput {
        keyword: if keyword.is_empty() {
            None
        } else {
            Some(keyword.to_string())
        },
        eq,
        order: if order.is_empty() {
            None
        } else {
            Some(order.to_string())
        },
        sort: if sort.is_empty() {
            None
        } else {
            Some(sort.to_string())
        },
        page: 1,
        size: 0,
        is_export: false,
        max_export_limit: 0,
    }
}

#[tonic::async_trait]
impl DemoSampleRpc for DemoSampleSvc {
    async fn add(
        &self,
        req: Request<DemoSampleAddReq>,
    ) -> Result<Response<DemoSampleAddRes>, Status> {
        self.check_api("Add")?;
        let r = req.into_inner();
        let mut data = FieldMap::new();
        data.insert("name".into(), json!(r.name));
        data.insert("remark".into(), json!(r.remark));

        let row = self.svc.service_add(data).await.map_err(biz_err)?;
        let (code, message) = ok_headers();
        Ok(Response::new(DemoSampleAddRes {
            code,
            message,
            data: Some(map_to_proto(&row)),
        }))
    }

    async fn delete(&self, req: Request<IdsReq>) -> Result<Response<DemoSampleDeleteRes>, Status> {
        self.check_api("Delete")?;
        let ids = req.into_inner().ids;
        let affected = self.svc.service_delete(&ids).await.map_err(biz_err)?;
        let (code, message) = ok_headers();
        Ok(Response::new(DemoSampleDeleteRes {
            code,
            message,
            affected: affected as i64,
        }))
    }

    async fn update(
        &self,
        req: Request<DemoSampleUpdateReq>,
    ) -> Result<Response<DemoSampleUpdateRes>, Status> {
        self.check_api("Update")?;
        let r = req.into_inner();
        let mut data = FieldMap::new();
        if let Some(n) = r.name {
            data.insert("name".into(), json!(n));
        }
        if let Some(remark) = r.remark {
            data.insert("remark".into(), json!(remark));
        }
        let affected = self.svc.service_update(r.id, data).await.map_err(biz_err)?;
        let (code, message) = ok_headers();
        Ok(Response::new(DemoSampleUpdateRes {
            code,
            message,
            affected: affected as i64,
        }))
    }

    async fn info(&self, req: Request<IdReq>) -> Result<Response<DemoSampleInfoRes>, Status> {
        self.check_api("Info")?;
        let id = req.into_inner().id;
        let row = self.svc.service_info(id).await.map_err(biz_err)?;
        let (code, message) = ok_headers();
        Ok(Response::new(DemoSampleInfoRes {
            code,
            message,
            data: row.as_ref().map(map_to_proto),
        }))
    }

    async fn list(
        &self,
        req: Request<DemoSampleListReq>,
    ) -> Result<Response<DemoSampleListRes>, Status> {
        self.check_api("List")?;
        let r = req.into_inner();
        let input = to_query_input(&r.keyword, &r.order, &r.sort, r.name);
        let rows = self.svc.service_list(input).await.map_err(biz_err)?;
        let (code, message) = ok_headers();
        Ok(Response::new(DemoSampleListRes {
            code,
            message,
            list: rows.iter().map(map_to_proto).collect(),
        }))
    }

    async fn page(
        &self,
        req: Request<DemoSamplePageReq>,
    ) -> Result<Response<DemoSamplePageRes>, Status> {
        self.check_api("Page")?;
        let r = req.into_inner();
        let mut input = to_query_input(&r.keyword, &r.order, &r.sort, r.name);
        input.page = r.page as i64;
        input.size = r.size as i64;
        input.is_export = r.is_export;
        input.max_export_limit = r.max_export_limit as i64;

        let PageResult { total, list } = self.svc.service_page(input).await.map_err(biz_err)?;
        let (code, message) = ok_headers();
        Ok(Response::new(DemoSamplePageRes {
            code,
            message,
            total,
            list: list.iter().map(map_to_proto).collect(),
        }))
    }

    async fn welcome(
        &self,
        _req: Request<DemoSampleWelcomeReq>,
    ) -> Result<Response<DemoSampleWelcomeRes>, Status> {
        self.check_api("Welcome")?;
        let (code, message) = ok_headers();
        Ok(Response::new(DemoSampleWelcomeRes {
            code,
            message,
            data: Some(WelcomeData {
                name: "Cool Admin Go".into(),
                age: 0,
            }),
        }))
    }
}
