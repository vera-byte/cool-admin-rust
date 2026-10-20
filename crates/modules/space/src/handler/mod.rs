//! space handlers

use cool_core::CrudService;
use cool_core::model::FieldMap;
use cool_core::ok_code;
use cool_core::query::QueryInput;
use cool_proto::common::v1::{IdReq, IdsReq};
use cool_proto::space::v1::space_info_service_server::SpaceInfoService as SpaceInfoRpc;
use cool_proto::space::v1::space_type_service_server::SpaceTypeService as SpaceTypeRpc;
use cool_proto::space::v1::*;
use serde_json::{Value, json};
use sqlx::MySqlPool;
use tonic::{Request, Response, Status};

use crate::service::{
    SpaceInfoService, SpaceTypeService, new_space_info_service, new_space_type_service,
};

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

// ---------------- SpaceType ----------------

pub struct SpaceTypeSvc {
    svc: SpaceTypeService,
}

impl SpaceTypeSvc {
    pub fn new(pool: MySqlPool) -> Self {
        Self {
            svc: new_space_type_service(pool),
        }
    }
    fn to_proto(m: &FieldMap) -> SpaceType {
        SpaceType {
            id: g_i64(m, "id"),
            name: g_str(m, "name"),
            parent_id: g_i64(m, "parentId"),
            create_time: g_i64(m, "createTime"),
            update_time: g_i64(m, "updateTime"),
        }
    }
}

#[tonic::async_trait]
impl SpaceTypeRpc for SpaceTypeSvc {
    async fn add(
        &self,
        req: Request<SpaceTypeAddReq>,
    ) -> Result<Response<SpaceTypeAddRes>, Status> {
        let r = req.into_inner();
        let mut d = FieldMap::new();
        d.insert("name".into(), json!(r.name));
        d.insert("parentId".into(), json!(r.parent_id));
        let row = self.svc.service_add(d).await.map_err(biz)?;
        let (code, message) = ok_code();
        Ok(Response::new(SpaceTypeAddRes {
            code,
            message,
            data: Some(Self::to_proto(&row)),
        }))
    }
    async fn delete(&self, req: Request<IdsReq>) -> Result<Response<SpaceTypeDeleteRes>, Status> {
        let ids = req.into_inner().ids;
        let n = self.svc.service_delete(&ids).await.map_err(biz)?;
        let (code, message) = ok_code();
        Ok(Response::new(SpaceTypeDeleteRes {
            code,
            message,
            affected: n as i64,
        }))
    }
    async fn update(
        &self,
        req: Request<SpaceTypeUpdateReq>,
    ) -> Result<Response<SpaceTypeUpdateRes>, Status> {
        let r = req.into_inner();
        let mut d = FieldMap::new();
        if let Some(v) = r.name {
            d.insert("name".into(), json!(v));
        }
        if let Some(v) = r.parent_id {
            d.insert("parentId".into(), json!(v));
        }
        let n = self.svc.service_update(r.id, d).await.map_err(biz)?;
        let (code, message) = ok_code();
        Ok(Response::new(SpaceTypeUpdateRes {
            code,
            message,
            affected: n as i64,
        }))
    }
    async fn info(&self, req: Request<IdReq>) -> Result<Response<SpaceTypeInfoRes>, Status> {
        let id = req.into_inner().id;
        let row = self.svc.service_info(id).await.map_err(biz)?;
        let (code, message) = ok_code();
        Ok(Response::new(SpaceTypeInfoRes {
            code,
            message,
            data: row.as_ref().map(Self::to_proto),
        }))
    }
    async fn list(
        &self,
        req: Request<SpaceTypeListReq>,
    ) -> Result<Response<SpaceTypeListRes>, Status> {
        let r = req.into_inner();
        let rows = self
            .svc
            .service_list(QueryInput {
                keyword: Some(r.keyword).filter(|s| !s.is_empty()),
                ..Default::default()
            })
            .await
            .map_err(biz)?;
        let (code, message) = ok_code();
        Ok(Response::new(SpaceTypeListRes {
            code,
            message,
            list: rows.iter().map(Self::to_proto).collect(),
        }))
    }
    async fn page(
        &self,
        req: Request<SpaceTypePageReq>,
    ) -> Result<Response<SpaceTypePageRes>, Status> {
        let r = req.into_inner();
        let rows = self
            .svc
            .service_page(QueryInput {
                keyword: Some(r.keyword).filter(|s| !s.is_empty()),
                page: r.page as i64,
                size: r.size as i64,
                ..Default::default()
            })
            .await
            .map_err(biz)?;
        let (code, message) = ok_code();
        Ok(Response::new(SpaceTypePageRes {
            code,
            message,
            total: rows.total,
            list: rows.list.iter().map(Self::to_proto).collect(),
        }))
    }
}

// ---------------- SpaceInfo ----------------

pub struct SpaceInfoSvc {
    svc: SpaceInfoService,
}

impl SpaceInfoSvc {
    pub fn new(pool: MySqlPool) -> Self {
        Self {
            svc: new_space_info_service(pool),
        }
    }
    fn to_proto(m: &FieldMap) -> SpaceInfo {
        SpaceInfo {
            id: g_i64(m, "id"),
            url: g_str(m, "url"),
            r#type: g_str(m, "type"),
            classify_id: g_i64(m, "classifyId"),
            create_time: g_i64(m, "createTime"),
            update_time: g_i64(m, "updateTime"),
        }
    }
}

fn info_query(keyword: &str, ty: Option<String>, classify_id: Option<i64>) -> QueryInput {
    let mut eq = std::collections::HashMap::new();
    if let Some(t) = ty {
        eq.insert("type".into(), json!(t));
    }
    if let Some(c) = classify_id {
        eq.insert("classifyId".into(), json!(c));
    }
    QueryInput {
        keyword: Some(keyword.to_string()).filter(|s| !s.is_empty()),
        eq,
        page: 1,
        size: 0,
        ..Default::default()
    }
}

#[tonic::async_trait]
impl SpaceInfoRpc for SpaceInfoSvc {
    async fn add(
        &self,
        req: Request<SpaceInfoAddReq>,
    ) -> Result<Response<SpaceInfoAddRes>, Status> {
        let r = req.into_inner();
        let mut d = FieldMap::new();
        d.insert("url".into(), json!(r.url));
        d.insert("type".into(), json!(r.r#type));
        d.insert("classifyId".into(), json!(r.classify_id));
        let row = self.svc.service_add(d).await.map_err(biz)?;
        let (code, message) = ok_code();
        Ok(Response::new(SpaceInfoAddRes {
            code,
            message,
            data: Some(Self::to_proto(&row)),
        }))
    }
    async fn delete(&self, req: Request<IdsReq>) -> Result<Response<SpaceInfoDeleteRes>, Status> {
        let ids = req.into_inner().ids;
        let n = self.svc.service_delete(&ids).await.map_err(biz)?;
        let (code, message) = ok_code();
        Ok(Response::new(SpaceInfoDeleteRes {
            code,
            message,
            affected: n as i64,
        }))
    }
    async fn update(
        &self,
        req: Request<SpaceInfoUpdateReq>,
    ) -> Result<Response<SpaceInfoUpdateRes>, Status> {
        let r = req.into_inner();
        let mut d = FieldMap::new();
        if let Some(v) = r.url {
            d.insert("url".into(), json!(v));
        }
        if let Some(v) = r.r#type {
            d.insert("type".into(), json!(v));
        }
        if let Some(v) = r.classify_id {
            d.insert("classifyId".into(), json!(v));
        }
        let n = self.svc.service_update(r.id, d).await.map_err(biz)?;
        let (code, message) = ok_code();
        Ok(Response::new(SpaceInfoUpdateRes {
            code,
            message,
            affected: n as i64,
        }))
    }
    async fn info(&self, req: Request<IdReq>) -> Result<Response<SpaceInfoInfoRes>, Status> {
        let id = req.into_inner().id;
        let row = self.svc.service_info(id).await.map_err(biz)?;
        let (code, message) = ok_code();
        Ok(Response::new(SpaceInfoInfoRes {
            code,
            message,
            data: row.as_ref().map(Self::to_proto),
        }))
    }
    async fn list(
        &self,
        req: Request<SpaceInfoListReq>,
    ) -> Result<Response<SpaceInfoListRes>, Status> {
        let r = req.into_inner();
        let rows = self
            .svc
            .service_list(info_query(&r.keyword, r.r#type, r.classify_id))
            .await
            .map_err(biz)?;
        let (code, message) = ok_code();
        Ok(Response::new(SpaceInfoListRes {
            code,
            message,
            list: rows.iter().map(Self::to_proto).collect(),
        }))
    }
    async fn page(
        &self,
        req: Request<SpaceInfoPageReq>,
    ) -> Result<Response<SpaceInfoPageRes>, Status> {
        let r = req.into_inner();
        let mut q = info_query(&r.keyword, r.r#type, r.classify_id);
        q.page = r.page as i64;
        q.size = r.size as i64;
        let rows = self.svc.service_page(q).await.map_err(biz)?;
        let (code, message) = ok_code();
        Ok(Response::new(SpaceInfoPageRes {
            code,
            message,
            total: rows.total,
            list: rows.list.iter().map(Self::to_proto).collect(),
        }))
    }
}
