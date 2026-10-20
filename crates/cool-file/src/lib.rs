//! 文件存储驱动（对齐 cool-admin-go `cool/coolfile` + `contrib/files/*`）。
//!
//! - Local：写 `public/uploads/Ymd/<自动名>`，URL `{domain}/public/uploads/...`
//! - Minio：对象键 `uploads/Ymd/{key||rand16}`
//! - OSS：同键，URL `https://{bucket}.{endpoint}/{key}`
//!
//! `get_mode()` 对 minio/oss 仍返回 `("local", ty)` —— 对齐 Go，
//! 现有 Vue 前端依据 `mode=="local"` 走后端代理上传。

pub mod local;
pub mod minio;
pub mod oss;

use std::sync::Arc;

use bytes::Bytes;

use cool_core::error::{CoolError, CoolResult};

/// 上传请求
#[derive(Debug, Clone)]
pub struct UploadRequest {
    pub filename: String,
    /// 自定义对象键（可选，minio/oss 用）
    pub key: Option<String>,
    pub data: Bytes,
}

/// 文件驱动
#[async_trait::async_trait]
pub trait FileDriver: Send + Sync {
    /// 上传，返回公开 URL
    async fn upload(&self, req: UploadRequest) -> CoolResult<String>;
    /// 下载：(filename, bytes)
    async fn download(&self, path: &str) -> CoolResult<(String, Bytes)>;
    /// (mode, type)：mode 对 minio/oss 恒为 "local"（前端兼容）
    fn get_mode(&self) -> (String, String);
}

/// OSS/Minio 共用配置
#[derive(Debug, Clone, Default)]
pub struct ObjectConfig {
    pub endpoint: String,
    pub access_key_id: String,
    pub secret_access_key: String,
    pub bucket_name: String,
    pub use_ssl: bool,
    pub location: String,
}

/// 文件配置
#[derive(Debug, Clone)]
pub struct FileConfig {
    /// local | minio | oss
    pub mode: String,
    pub domain: String,
    pub oss: ObjectConfig,
    /// local 根目录（相对 cwd）
    pub root: String,
}

impl Default for FileConfig {
    fn default() -> Self {
        Self {
            mode: "local".into(),
            domain: "http://127.0.0.1:8002".into(),
            oss: ObjectConfig {
                endpoint: "127.0.0.1:9000".into(),
                access_key_id: String::new(),
                secret_access_key: String::new(),
                bucket_name: "cool-admin-rust".into(),
                use_ssl: false,
                location: "us-east-1".into(),
            },
            root: "public".into(),
        }
    }
}

/// 依据配置创建驱动
pub async fn new_driver(cfg: &FileConfig) -> CoolResult<Arc<dyn FileDriver>> {
    match cfg.mode.as_str() {
        "local" => Ok(Arc::new(local::LocalDriver::new(cfg.clone()))),
        "minio" => {
            let d = minio::MinioDriver::new(cfg.clone()).await?;
            Ok(Arc::new(d))
        }
        "oss" => {
            let d = oss::OssDriver::new(cfg.clone()).await?;
            Ok(Arc::new(d))
        }
        other => Err(CoolError::Internal(format!("未知文件驱动: {other}"))),
    }
}

/// 生成对象键 `uploads/Ymd/{name}`（对齐 Go）
pub fn object_key(name: &str) -> String {
    let dir = chrono::Local::now().format("%Y%m%d").to_string();
    format!("uploads/{dir}/{name}")
}

/// 随机 16 位文件名（对齐 Go grand.S(16)）
pub fn random_name(ext: &str) -> String {
    use rand::Rng;
    const CHARS: &[u8] = b"abcdefghijklmnopqrstuvwxyz0123456789";
    let mut rng = rand::thread_rng();
    let stem: String = (0..16)
        .map(|_| CHARS[rng.gen_range(0..CHARS.len())] as char)
        .collect();
    if ext.is_empty() {
        stem
    } else {
        format!("{stem}.{ext}")
    }
}

/// 从文件名提取扩展名（不含点）
pub fn ext_of(filename: &str) -> String {
    std::path::Path::new(filename)
        .extension()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn object_key_shape() {
        let k = object_key("a.txt");
        assert!(k.starts_with("uploads/"));
        assert!(k.ends_with("/a.txt"));
        assert_eq!(k.matches('/').count(), 2);
    }

    #[test]
    fn ext_of_cases() {
        assert_eq!(ext_of("a.png"), "png");
        assert_eq!(ext_of("noext"), "");
        assert_eq!(ext_of("a.b.tar.gz"), "gz");
    }

    #[test]
    fn random_name_has_ext() {
        let n = random_name("png");
        assert!(n.ends_with(".png"));
        assert_eq!(n.len(), 16 + 4);
    }
}
