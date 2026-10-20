//! 阿里云 OSS 驱动（S3 兼容 PutObject）。
//! URL `https://{bucket}.{endpoint}/{key}`（对齐 Go 固定 https）。
//! `get_mode().0` 恒为 "local"（前端兼容）。

use aws_sdk_s3::Client;
use aws_sdk_s3::config::{Credentials, Region};
use aws_sdk_s3::primitives::ByteStream;
use bytes::Bytes;

use cool_core::error::{CoolError, CoolResult};

use crate::{FileConfig, FileDriver, UploadRequest, ext_of, object_key, random_name};

pub struct OssDriver {
    cfg: FileConfig,
    client: Client,
}

impl OssDriver {
    pub async fn new(cfg: FileConfig) -> CoolResult<Self> {
        let creds = Credentials::new(
            &cfg.oss.access_key_id,
            &cfg.oss.secret_access_key,
            None,
            None,
            "cool-admin",
        );
        let endpoint = format!(
            "https://{}",
            cfg.oss.endpoint.trim_start_matches("https://")
        );
        let conf = aws_sdk_s3::Config::builder()
            .behavior_version_latest()
            .endpoint_url(endpoint)
            .region(Region::new(cfg.oss.location.clone()))
            .credentials_provider(creds)
            .build();
        let client = Client::from_conf(conf);
        Ok(Self { cfg, client })
    }

    /// 对齐 Go：`https://{bucketName}.{oss.endpoint}/{fullPath}`
    fn public_url(&self, key: &str) -> String {
        format!(
            "https://{}.{}/{}",
            self.cfg.oss.bucket_name,
            self.cfg
                .oss
                .endpoint
                .trim_start_matches("https://")
                .trim_end_matches('/'),
            key
        )
    }
}

#[async_trait::async_trait]
impl FileDriver for OssDriver {
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
            .map_err(|e| CoolError::Internal(format!("oss 上传失败: {e}")))?;

        Ok(self.public_url(&key))
    }

    async fn download(&self, path: &str) -> CoolResult<(String, Bytes)> {
        let key = if path.starts_with("uploads/") {
            path.to_string()
        } else {
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
            .map_err(|e| CoolError::Internal(format!("oss 下载失败: {e}")))?;
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
        ("local".into(), "oss".into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn oss_url_shape() {
        let cfg = FileConfig {
            mode: "oss".into(),
            oss: crate::ObjectConfig {
                endpoint: "oss-cn-hangzhou.aliyuncs.com".into(),
                bucket_name: "bkt".into(),
                ..Default::default()
            },
            ..Default::default()
        };
        // 不连网，仅验证 URL 拼装
        let d = OssDriver {
            cfg: cfg.clone(),
            client: Client::from_conf(
                aws_sdk_s3::Config::builder()
                    .behavior_version_latest()
                    .region(Region::new("oss-cn-hangzhou"))
                    .credentials_provider(Credentials::new("a", "b", None, None, "t"))
                    .build(),
            ),
        };
        assert_eq!(
            d.public_url("uploads/20260101/a.png"),
            "https://bkt.oss-cn-hangzhou.aliyuncs.com/uploads/20260101/a.png"
        );
    }
}
