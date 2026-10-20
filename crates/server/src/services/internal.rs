//! InternalService：Hello / Health（M0 验收）

use std::sync::Arc;

use cool_core::{instance_id, ok_code, response};
use cool_proto::internal::v1::internal_service_server::InternalService;
use cool_proto::internal::v1::{HealthData, HealthReq, HealthRes, HelloData, HelloReq, HelloRes};
use tonic::{Request, Response, Status};

use crate::state::AppState;

pub struct InternalSvc {
    pub state: Arc<AppState>,
}

#[tonic::async_trait]
impl InternalService for InternalSvc {
    async fn hello(&self, req: Request<HelloReq>) -> Result<Response<HelloRes>, Status> {
        let name = req.into_inner().name;
        let name = if name.is_empty() {
            "CoolAdmin".to_string()
        } else {
            name
        };
        let (code, message) = ok_code();
        let reply = HelloRes {
            code,
            message,
            data: Some(HelloData {
                greeting: format!("Hello, {name}! Welcome to cool-admin-rust (gRPC)."),
                instance_id: instance_id().to_string(),
            }),
        };
        Ok(Response::new(reply))
    }

    async fn health(&self, _req: Request<HealthReq>) -> Result<Response<HealthRes>, Status> {
        let db_ok = cool_db::healthy(self.state.db.as_ref()).await;
        let (code, message) = if db_ok || !self.state.db_required {
            ok_code()
        } else {
            response::fail_code("database unavailable")
        };

        let reply = HealthRes {
            code,
            message,
            data: Some(HealthData {
                status: if db_ok {
                    "up".into()
                } else {
                    "degraded".into()
                },
                database: db_ok,
                redis: false, // M3 接入
                mode: self.state.mode.clone(),
                instance_id: instance_id().to_string(),
            }),
        };
        Ok(Response::new(reply))
    }
}
