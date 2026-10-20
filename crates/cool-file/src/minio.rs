//! MinIO 驱动（S3 兼容）。对象键 `uploads/Ymd/{key||rand16}`。
//! URL `{scheme}://{endpoint}/{bucket}/{key}`（近似 Go info.Location）。
//! `get_mode().0` 恒为 "local"（前端兼容，对齐 Go）。

use aws_sdk_s3::Client;
use aws_sdk_s3::config::{Credentials, Region};
use aws_sdk_s3::primitives::ByteStream;
use bytes::Bytes;

use cool_core::error::{CoolError, CoolResult};

use crate::{FileConfig, FileDriver, UploadRequest, ext_of, object_key, random_name};

pub struct MinioDriver {
    cfg: FileConfig,
    client: Client,
}

impl MinioDriver {
    pub async fn new(cfg: FileConfig) -> CoolResult<Self> {
        let creds = Credentials::new(
            &cfg.oss.access_key_id,
            &cfg.oss.secret_access_key,
            None,
            None,
            "cool-admin",
        );
        let endpoint = if cfg.oss.use_ssl {
            format!(
                "https://{}",
                cfg.oss.endpoint.trim_start_matches("https://")
            )
        } else {
            format!("http://{}", cfg.oss.endpoint.trim_start_matches("http://"))
        };
        let conf = aws_sdk_s3::Config::builder()
            .behavior_version_latest()
            .endpoint_url(endpoint)
            .region(Region::new(cfg.oss.location.clone()))
            .credentials_provider(creds)
            .force_path_style(true)
            .build();
        let client = Client::from_conf(conf);

        // 启动时确保存储桶存在（对齐 Go）
        let head = client
            .head_bucket()
            .bucket(&cfg.oss.bucket_name)
            .send()
            .await;
        if head.is_err() {
            let _ = client
                .create_bucket()
                .bucket(&cfg.oss.bucket_name)
                .send()
                .await
                .map_err(|e| CoolError::Internal(format!("创建 bucket 失败: {e}")))?;
            tracing::info!(bucket = %cfg.oss.bucket_name, "minio bucket created");
        }

        Ok(Self { cfg, client })
    }

    fn public_url(&self, key: &str) -> String {
        let scheme = if self.cfg.oss.use_ssl {
            "https"
        } else {
            "http"
        };
        format!(
            "{}://{}/{}/{}",
            scheme,
            self.cfg.oss.endpoint.trim_end_matches('/'),
            self.cfg.oss.bucket_name,
            key
        )
    }
}

#[async_trait::async_trait]
impl FileDriver for MinioDriver {
    async fn upload(&self, req: UploadRequest) -> CoolResult<String> {
        let name = req.key.clone().unwrap_or_else(|| {
            if req.filename.is_empty() {
                random_name(&ext_of(""))
            } else {
                random_name(&ext_of(&req.filename))
            }
        });
        let key = object_key(&name);

        self.client
            .put_object()
            .bucket(&self.cfg.oss.bucket_name)
            .key(&key)
            .body(ByteStream::from(req.data))
            .send()
            .await
            .map_err(|e| CoolError::Internal(format!("minio 上传失败: {e}")))?;

        Ok(self.public_url(&key))
    }

    async fn download(&self, path: &str) -> CoolResult<(String, Bytes)> {
        // 传 key（uploads/...）或从中截取
        let key = if path.starts_with("uploads/") {
            path.to_string()
        } else {
            // URL 形态：http://endpoint/bucket/uploads/...
            match path.find("uploads/") {
                Some(i) => path[i..].to_string(),
                None => path.trim_start_matches('/').to_string(),
            }
        };
        let out = self
            .client
            .get_object()
            .bucket(&self.cfg.oss.bucket_name)
            .key(&key)
            .send()
            .await
            .map_err(|e| CoolError::Internal(format!("minio 下载失败: {e}")))?;
        let data = out
            .body
            .collect()
            .await
            .map_err(|e| CoolError::Internal(format!("读取对象失败: {e}")))?
            .into_bytes();
        let filename = key.rsplit('/').next().unwrap_or(&key).to_string();
        Ok((filename, data))
    }

    fn get_mode(&self) -> (String, String) {
        // Go 硬编码 "local" 让前端走代理；保持兼容
        ("local".into(), "minio".into())
    }
}
