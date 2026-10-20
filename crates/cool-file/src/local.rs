//! 本地文件驱动：`public/uploads/Ymd/<自动名>`，URL `{domain}/public/uploads/...`。
//! 对齐 contrib/files/local/local.go；静态目录由部署层 nginx 提供。

use std::path::PathBuf;

use bytes::Bytes;
use tokio::io::AsyncWriteExt;

use cool_core::error::{CoolError, CoolResult};

use crate::{FileConfig, FileDriver, UploadRequest, ext_of, random_name};

pub struct LocalDriver {
    cfg: FileConfig,
}

impl LocalDriver {
    pub fn new(cfg: FileConfig) -> Self {
        Self { cfg }
    }

    fn dir_for_today(&self) -> String {
        chrono::Local::now().format("%Y%m%d").to_string()
    }

    fn save_path(&self, dir: &str, name: &str) -> PathBuf {
        PathBuf::from(&self.cfg.root)
            .join("uploads")
            .join(dir)
            .join(name)
    }

    fn public_url(&self, dir: &str, name: &str) -> String {
        format!(
            "{}/public/uploads/{}/{}",
            self.cfg.domain.trim_end_matches('/'),
            dir,
            name
        )
    }
}

#[async_trait::async_trait]
impl FileDriver for LocalDriver {
    async fn upload(&self, req: UploadRequest) -> CoolResult<String> {
        let dir = self.dir_for_today();
        let ext = ext_of(&req.filename);
        // 自动命名，冲突时重命名（对齐 Go Save(dir, true)）
        let mut name = if req.filename.is_empty() {
            random_name(&ext)
        } else {
            // Go 的 Save(_, true) 生成新名字；保持一致：始终自动名
            random_name(&ext)
        };
        let mut path = self.save_path(&dir, &name);
        let mut i = 1;
        while path.exists() {
            let stem = name
                .rsplit_once('.')
                .map(|(s, _)| s.to_string())
                .unwrap_or(name.clone());
            name = if ext.is_empty() {
                format!("{stem}-{i}")
            } else {
                format!("{stem}-{i}.{ext}")
            };
            path = self.save_path(&dir, &name);
            i += 1;
        }

        tokio::fs::create_dir_all(path.parent().unwrap())
            .await
            .map_err(|e| CoolError::Internal(format!("创建目录失败: {e}")))?;
        let mut f = tokio::fs::File::create(&path)
            .await
            .map_err(|e| CoolError::Internal(format!("创建文件失败: {e}")))?;
        f.write_all(&req.data)
            .await
            .map_err(|e| CoolError::Internal(format!("写入文件失败: {e}")))?;
        f.flush()
            .await
            .map_err(|e| CoolError::Internal(format!("刷盘失败: {e}")))?;

        Ok(self.public_url(&dir, &name))
    }

    async fn download(&self, path: &str) -> CoolResult<(String, Bytes)> {
        // path 支持：完整公开 URL 后缀 /public/uploads/... 或相对 uploads/...
        let rel = path
            .trim_start_matches('/')
            .replace("/public/uploads/", "uploads/")
            .replace("public/uploads/", "uploads/");
        // 防目录穿越
        if rel.contains("..") {
            return Err(CoolError::invalid_argument("非法路径"));
        }
        let full = PathBuf::from(&self.cfg.root).join(&rel);
        let data = tokio::fs::read(&full)
            .await
            .map_err(|e| CoolError::Internal(format!("读取文件失败: {e}")))?;
        let filename = full
            .file_name()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_default();
        Ok((filename, Bytes::from(data)))
    }

    fn get_mode(&self) -> (String, String) {
        (self.cfg.mode.clone(), "local".into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::FileConfig;

    #[tokio::test]
    async fn upload_and_download_roundtrip() {
        let tmp = std::env::temp_dir().join(format!("cool-file-test-{}", std::process::id()));
        let _ = tokio::fs::remove_dir_all(&tmp).await;
        let cfg = FileConfig {
            mode: "local".into(),
            domain: "http://127.0.0.1:8002".into(),
            root: tmp.to_string_lossy().to_string(),
            ..Default::default()
        };
        let d = LocalDriver::new(cfg.clone());
        let url = d
            .upload(UploadRequest {
                filename: "hello.txt".into(),
                key: None,
                data: Bytes::from_static(b"hello"),
            })
            .await
            .unwrap();
        assert!(
            url.starts_with("http://127.0.0.1:8002/public/uploads/"),
            "url={url}"
        );
        assert!(url.ends_with(".txt"));

        let rel = url.trim_start_matches(&format!("{}/", cfg.domain.trim_end_matches('/')));
        let (name, data) = d.download(rel).await.unwrap();
        assert!(name.ends_with(".txt"));
        assert_eq!(&data[..], b"hello");

        let _ = tokio::fs::remove_dir_all(&tmp).await;
    }

    #[tokio::test]
    async fn download_rejects_traversal() {
        let d = LocalDriver::new(FileConfig::default());
        let e = d
            .download("public/uploads/../../etc/passwd")
            .await
            .unwrap_err();
        assert!(e.to_string().contains("非法路径"), "err={e}");
    }
}
