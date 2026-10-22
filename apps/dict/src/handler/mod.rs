//! dict handlers

use cool_core::CrudService;
use cool_core::model::FieldMap;
use cool_core::ok_code;
use cool_core::query::QueryInput;
use cool_proto::common::v1::{IdReq, IdsReq};
use cool_proto::dict::v1::dict_info_service_server::DictInfoService as DictInfoRpc;
use cool_proto::dict::v1::dict_type_service_server::DictTypeService as DictTypeRpc;
use cool_proto::dict::v1::*;
use serde_json::{Value, json};
use sqlx::MySqlPool;
use tonic::{Request, Response, Status};

use crate::service::{
    DictInfoService, DictTypeService, new_dict_info_service, new_dict_type_service,
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

// ---------------- DictType ----------------

pub struct DictTypeSvc {
    svc: DictTypeService,
}

impl DictTypeSvc {
    pub fn new(pool: MySqlPool) -> Self {
        Self {
            svc: new_dict_type_service(pool),
        }
    }
    fn to_proto(m: &FieldMap) -> DictType {
        DictType {
            id: g_i64(m, "id"),
            name: g_str(m, "name"),
            key: g_str(m, "key"),
            create_time: g_i64(m, "createTime"),
            update_time: g_i64(m, "updateTime"),
        }
    }
}

#[tonic::async_trait]
impl DictTypeRpc for DictTypeSvc {
    async fn add(&self, req: Request<DictTypeAddReq>) -> Result<Response<DictTypeAddRes>, Status> {
        let r = req.into_inner();
        let mut d = FieldMap::new();
        d.insert("name".into(), json!(r.name));
        d.insert("key".into(), json!(r.key));
        let row = self.svc.service_add(d).await.map_err(biz)?;
        let (code, message) = ok_code();
        Ok(Response::new(DictTypeAddRes {
            code,
            message,
            data: Some(Self::to_proto(&row)),
        }))
    }
    async fn delete(&self, req: Request<IdsReq>) -> Result<Response<DictTypeDeleteRes>, Status> {
        let ids = req.into_inner().ids;
        let n = self.svc.service_delete(&ids).await.map_err(biz)?;
        let (code, message) = ok_code();
        Ok(Response::new(DictTypeDeleteRes {
            code,
            message,
            affected: n as i64,
        }))
    }
    async fn update(
        &self,
        req: Request<DictTypeUpdateReq>,
    ) -> Result<Response<DictTypeUpdateRes>, Status> {
        let r = req.into_inner();
        let mut d = FieldMap::new();
        if let Some(v) = r.name {
            d.insert("name".into(), json!(v));
        }
        if let Some(v) = r.key {
            d.insert("key".into(), json!(v));
        }
        let n = self.svc.service_update(r.id, d).await.map_err(biz)?;
        let (code, message) = ok_code();
        Ok(Response::new(DictTypeUpdateRes {
            code,
            message,
            affected: n as i64,
        }))
    }
    async fn info(&self, req: Request<IdReq>) -> Result<Response<DictTypeInfoRes>, Status> {
        let id = req.into_inner().id;
        let row = self.svc.service_info(id).await.map_err(biz)?;
        let (code, message) = ok_code();
        Ok(Response::new(DictTypeInfoRes {
            code,
            message,
            data: row.as_ref().map(Self::to_proto),
        }))
    }
    async fn list(
        &self,
        req: Request<DictTypeListReq>,
    ) -> Result<Response<DictTypeListRes>, Status> {
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
        Ok(Response::new(DictTypeListRes {
            code,
            message,
            list: rows.iter().map(Self::to_proto).collect(),
        }))
    }
    async fn page(
        &self,
        req: Request<DictTypePageReq>,
    ) -> Result<Response<DictTypePageRes>, Status> {
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
        Ok(Response::new(DictTypePageRes {
            code,
            message,
            total: rows.total,
            list: rows.list.iter().map(Self::to_proto).collect(),
        }))
    }
}

// ---------------- DictInfo ----------------

pub struct DictInfoSvc {
    svc: DictInfoService,
}

impl DictInfoSvc {
    pub fn new(pool: MySqlPool) -> Self {
        Self {
            svc: new_dict_info_service(pool),
        }
    }
    fn to_proto(m: &FieldMap) -> DictInfo {
        DictInfo {
            id: g_i64(m, "id"),
            type_id: g_i64(m, "typeId"),
            name: g_str(m, "name"),
            order_num: g_i64(m, "orderNum") as i32,
            remark: g_str(m, "remark"),
            parent_id: g_i64(m, "parentId"),
            create_time: g_i64(m, "createTime"),
            update_time: g_i64(m, "updateTime"),
        }
    }
}

#[tonic::async_trait]
impl DictInfoRpc for DictInfoSvc {
    async fn add(&self, req: Request<DictInfoAddReq>) -> Result<Response<DictInfoAddRes>, Status> {
        let r = req.into_inner();
        let mut d = FieldMap::new();
        d.insert("typeId".into(), json!(r.type_id));
        d.insert("name".into(), json!(r.name));
        d.insert("orderNum".into(), json!(r.order_num));
        d.insert("remark".into(), json!(r.remark));
        d.insert("parentId".into(), json!(r.parent_id));
        let row = self.svc.service_add(d).await.map_err(biz)?;
        let (code, message) = ok_code();
        Ok(Response::new(DictInfoAddRes {
            code,
            message,
            data: Some(Self::to_proto(&row)),
        }))
    }
    async fn delete(&self, req: Request<IdsReq>) -> Result<Response<DictInfoDeleteRes>, Status> {
        let ids = req.into_inner().ids;
        let n = self.svc.delete_cascade(&ids).await.map_err(biz)?;
        let (code, message) = ok_code();
        Ok(Response::new(DictInfoDeleteRes {
            code,
            message,
            affected: n as i64,
        }))
    }
    async fn update(
        &self,
        req: Request<DictInfoUpdateReq>,
    ) -> Result<Response<DictInfoUpdateRes>, Status> {
        let r = req.into_inner();
        let mut d = FieldMap::new();
        if let Some(v) = r.type_id {
            d.insert("typeId".into(), json!(v));
        }
        if let Some(v) = r.name {
            d.insert("name".into(), json!(v));
        }
        if let Some(v) = r.order_num {
            d.insert("orderNum".into(), json!(v));
        }
        if let Some(v) = r.remark {
            d.insert("remark".into(), json!(v));
        }
        if let Some(v) = r.parent_id {
            d.insert("parentId".into(), json!(v));
        }
        let n = self.svc.service_update(r.id, d).await.map_err(biz)?;
        let (code, message) = ok_code();
        Ok(Response::new(DictInfoUpdateRes {
            code,
            message,
            affected: n as i64,
        }))
    }
    async fn info(&self, req: Request<IdReq>) -> Result<Response<DictInfoInfoRes>, Status> {
        let id = req.into_inner().id;
        let row = self.svc.service_info(id).await.map_err(biz)?;
        let (code, message) = ok_code();
        Ok(Response::new(DictInfoInfoRes {
            code,
            message,
            data: row.as_ref().map(Self::to_proto),
        }))
    }
    async fn list(
        &self,
        req: Request<DictInfoListReq>,
    ) -> Result<Response<DictInfoListRes>, Status> {
        let r = req.into_inner();
        let rows = self
            .svc
            .service_list(crate::service::info_query(&r.keyword, r.type_id))
            .await
            .map_err(biz)?;
        let (code, message) = ok_code();
        Ok(Response::new(DictInfoListRes {
            code,
            message,
            list: rows.iter().map(Self::to_proto).collect(),
        }))
    }
    async fn page(
        &self,
        req: Request<DictInfoPageReq>,
    ) -> Result<Response<DictInfoPageRes>, Status> {
        let r = req.into_inner();
        let mut q = crate::service::info_query(&r.keyword, r.type_id);
        q.page = r.page as i64;
        q.size = r.size as i64;
        let rows = self.svc.service_page(q).await.map_err(biz)?;
        let (code, message) = ok_code();
        Ok(Response::new(DictInfoPageRes {
            code,
            message,
            total: rows.total,
            list: rows.list.iter().map(Self::to_proto).collect(),
        }))
    }
    async fn data(&self, req: Request<DictDataReq>) -> Result<Response<DictDataRes>, Status> {
        // 混沌注入：同步阻塞模拟线程池耗尽/降级
        let block_ms = cool_micro::chaos::block_ms_from_env();
        if block_ms > 0 {
            std::thread::sleep(std::time::Duration::from_millis(block_ms));
        }
        let r = req.into_inner();
        let map = self.svc.data(&r.types).await.map_err(biz)?;
        let mut data = std::collections::HashMap::new();
        for (k, items) in map {
            data.insert(
                k,
                DictDataList {
                    items: items
                        .iter()
                        .map(|m| DictDataItem {
                            id: g_i64(m, "id"),
                            name: g_str(m, "name"),
                            parent_id: g_i64(m, "parentId"),
                            type_id: g_i64(m, "typeId"),
                        })
                        .collect(),
                },
            );
        }
        let (code, message) = ok_code();
        Ok(Response::new(DictDataRes {
            code,
            message,
            data,
        }))
    }
}
