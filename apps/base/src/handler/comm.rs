//! BaseCommService：上传模式 / 上传 / 下载（对齐 /admin/base/comm/*）

use bytes::Bytes;
use cool_core::ok_code;
use cool_file::{FileDriver, UploadRequest};
use cool_proto::base::v1::base_comm_service_server::BaseCommService;
use cool_proto::base::v1::*;
use std::sync::Arc;
use tonic::{Request, Response, Status};

use super::biz_err;

pub struct BaseCommSvc {
    driver: Arc<dyn FileDriver>,
}

impl BaseCommSvc {
    pub fn new(driver: Arc<dyn FileDriver>) -> Self {
        Self { driver }
    }
}

#[tonic::async_trait]
impl BaseCommService for BaseCommSvc {
    async fn upload_mode(
        &self,
        _req: Request<UploadModeReq>,
    ) -> Result<Response<UploadModeRes>, Status> {
        let (mode, ty) = self.driver.get_mode();
        let (code, message) = ok_code();
        Ok(Response::new(UploadModeRes {
            code,
            message,
            data: Some(UploadModeData { mode, r#type: ty }),
        }))
    }

    async fn upload(&self, req: Request<UploadReq>) -> Result<Response<UploadRes>, Status> {
        let r = req.into_inner();
        if r.data.is_empty() {
            return Err(Status::invalid_argument("文件内容为空"));
        }
        let url = self
            .driver
            .upload(UploadRequest {
                filename: r.filename.clone(),
                key: r.key,
                data: Bytes::from(r.data),
            })
            .await
            .map_err(biz_err)?;
        // path：去掉 domain 前缀，保留对象键
        let path = match url.find("/public/uploads/") {
            Some(i) => url[i + 1..].to_string(),
            None => match url.find("uploads/") {
                Some(i) => url[i..].to_string(),
                None => url.clone(),
            },
        };
        let (code, message) = ok_code();
        Ok(Response::new(UploadRes {
            code,
            message,
            data: Some(UploadResData { url, path }),
        }))
    }

    async fn download(&self, req: Request<DownloadReq>) -> Result<Response<DownloadRes>, Status> {
        let path = req.into_inner().path;
        let (filename, data) = self.driver.download(&path).await.map_err(biz_err)?;
        let (code, message) = ok_code();
        Ok(Response::new(DownloadRes {
            code,
            message,
            data: Some(DownloadResData {
                filename,
                data: data.to_vec(),
            }),
        }))
    }
}
