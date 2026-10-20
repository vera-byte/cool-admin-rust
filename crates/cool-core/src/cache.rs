//! 缓存抽象：进程内 / Redis（对齐 Go `cool.CacheManager`）。
//!
//! 键名约定（对齐 cool-admin-go）：
//! - `admin:token:{userId}` / `admin:token:refresh:{userId}` / `admin:passwordVersion:{userId}`
//! - `admin:loginFail:{username}` / `admin:perms:{userId}`
//! - `cool:masterflag` / `cool:func`（pub/sub 频道）

use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use std::time::{Duration, Instant};

use once_cell::sync::Lazy;

use crate::error::{CoolError, CoolResult};

// ---------------- trait ----------------

/// 缓存后端
#[async_trait::async_trait]
pub trait CacheStore: Send + Sync {
    async fn set(&self, key: &str, value: String, ttl: Duration) -> CoolResult<()>;
    async fn get(&self, key: &str) -> CoolResult<Option<String>>;
    async fn remove(&self, key: &str) -> CoolResult<()>;
    /// 自增（键不存在视为 0），返回新值；ttl 只在首次创建时设置
    async fn incr(&self, key: &str, ttl: Duration) -> CoolResult<i64>;
    /// 读取，不存在则写入并返回写入值（对齐 Go MustGetOrSet）
    async fn get_or_set(&self, key: &str, value: String, ttl: Duration) -> CoolResult<String>;
}

// ---------------- MemStore ----------------

struct Entry {
    value: String,
    expires_at: Option<Instant>,
}

#[derive(Default)]
pub struct MemStore {
    map: RwLock<HashMap<String, Entry>>,
}

impl MemStore {
    pub fn new() -> Self {
        Self::default()
    }
}

#[async_trait::async_trait]
impl CacheStore for MemStore {
    async fn set(&self, key: &str, value: String, ttl: Duration) -> CoolResult<()> {
        let expires_at = if ttl.is_zero() {
            None
        } else {
            Some(Instant::now() + ttl)
        };
        self.map
            .write()
            .unwrap()
            .insert(key.to_string(), Entry { value, expires_at });
        Ok(())
    }

    async fn get(&self, key: &str) -> CoolResult<Option<String>> {
        let map = self.map.read().unwrap();
        let Some(e) = map.get(key) else {
            return Ok(None);
        };
        if let Some(exp) = e.expires_at
            && Instant::now() > exp
        {
            return Ok(None);
        }
        Ok(Some(e.value.clone()))
    }

    async fn remove(&self, key: &str) -> CoolResult<()> {
        self.map.write().unwrap().remove(key);
        Ok(())
    }

    async fn incr(&self, key: &str, ttl: Duration) -> CoolResult<i64> {
        let mut map = self.map.write().unwrap();
        let now = Instant::now();
        let entry = map.entry(key.to_string()).or_insert_with(|| Entry {
            value: "0".into(),
            expires_at: if ttl.is_zero() { None } else { Some(now + ttl) },
        });
        if let Some(exp) = entry.expires_at
            && now > exp
        {
            entry.value = "0".into();
            entry.expires_at = if ttl.is_zero() { None } else { Some(now + ttl) };
        }
        let n: i64 = entry.value.parse().unwrap_or(0) + 1;
        entry.value = n.to_string();
        Ok(n)
    }

    async fn get_or_set(&self, key: &str, value: String, ttl: Duration) -> CoolResult<String> {
        if let Some(v) = self.get(key).await? {
            return Ok(v);
        }
        self.set(key, value.clone(), ttl).await?;
        Ok(value)
    }
}

// ---------------- RedisStore ----------------

pub struct RedisStore {
    conn: redis::aio::ConnectionManager,
}

impl RedisStore {
    pub async fn connect(url: &str) -> CoolResult<Self> {
        let client = redis::Client::open(url)
            .map_err(|e| CoolError::Internal(format!("redis 配置错误: {e}")))?;
        let conn = client
            .get_connection_manager()
            .await
            .map_err(|e| CoolError::Internal(format!("redis 连接失败: {e}")))?;
        Ok(Self { conn })
    }

    fn ttl_secs(ttl: Duration) -> Option<i64> {
        if ttl.is_zero() {
            None
        } else {
            Some(ttl.as_secs().max(1) as i64)
        }
    }
}

#[async_trait::async_trait]
impl CacheStore for RedisStore {
    async fn set(&self, key: &str, value: String, ttl: Duration) -> CoolResult<()> {
        let mut conn = self.conn.clone();
        match Self::ttl_secs(ttl) {
            Some(secs) => {
                redis::cmd("SETEX")
                    .arg(key)
                    .arg(secs)
                    .arg(&value)
                    .exec_async(&mut conn)
                    .await
            }
            None => {
                redis::cmd("SET")
                    .arg(key)
                    .arg(&value)
                    .exec_async(&mut conn)
                    .await
            }
        }
        .map_err(|e| CoolError::Internal(format!("redis SET 失败: {e}")))?;
        Ok(())
    }

    async fn get(&self, key: &str) -> CoolResult<Option<String>> {
        let mut conn = self.conn.clone();
        let v: Option<String> = redis::cmd("GET")
            .arg(key)
            .query_async(&mut conn)
            .await
            .map_err(|e| CoolError::Internal(format!("redis GET 失败: {e}")))?;
        Ok(v)
    }

    async fn remove(&self, key: &str) -> CoolResult<()> {
        let mut conn = self.conn.clone();
        let _: i64 = redis::cmd("DEL")
            .arg(key)
            .query_async(&mut conn)
            .await
            .map_err(|e| CoolError::Internal(format!("redis DEL 失败: {e}")))?;
        Ok(())
    }

    async fn incr(&self, key: &str, ttl: Duration) -> CoolResult<i64> {
        let mut conn = self.conn.clone();
        let n: i64 = redis::cmd("INCR")
            .arg(key)
            .query_async(&mut conn)
            .await
            .map_err(|e| CoolError::Internal(format!("redis INCR 失败: {e}")))?;
        if n == 1
            && let Some(secs) = Self::ttl_secs(ttl)
        {
            let _: i64 = redis::cmd("EXPIRE")
                .arg(key)
                .arg(secs)
                .query_async(&mut conn)
                .await
                .map_err(|e| CoolError::Internal(format!("redis EXPIRE 失败: {e}")))?;
        }
        Ok(n)
    }

    async fn get_or_set(&self, key: &str, value: String, ttl: Duration) -> CoolResult<String> {
        if let Some(v) = self.get(key).await? {
            return Ok(v);
        }
        self.set(key, value.clone(), ttl).await?;
        Ok(value)
    }
}

// ---------------- global store ----------------

static STORE: Lazy<RwLock<Arc<dyn CacheStore>>> =
    Lazy::new(|| RwLock::new(Arc::new(MemStore::new())));
static REDIS_MODE: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// 初始化全局缓存（Redis 可用则用 Redis，否则内存）。
/// Redis 连接限时 2 秒，失败回退内存，避免无 Redis 时启动卡死。
pub async fn init_store(redis_url: Option<&str>) {
    if let Some(url) = redis_url
        && !url.is_empty()
    {
        let connect = RedisStore::connect(url);
        match tokio::time::timeout(Duration::from_secs(2), connect).await {
            Ok(Ok(r)) => {
                tracing::info!("cache: redis store ready");
                *STORE.write().unwrap() = Arc::new(r);
                REDIS_MODE.store(true, std::sync::atomic::Ordering::Relaxed);
                return;
            }
            Ok(Err(e)) => {
                tracing::warn!("cache: redis unavailable, fallback to memory: {e}");
            }
            Err(_) => {
                tracing::warn!("cache: redis connect timeout, fallback to memory");
            }
        }
    }
    tracing::info!("cache: in-memory store");
    *STORE.write().unwrap() = Arc::new(MemStore::new());
    REDIS_MODE.store(false, std::sync::atomic::Ordering::Relaxed);
}

fn store() -> Arc<dyn CacheStore> {
    STORE.read().unwrap().clone()
}

/// 当前是否为 Redis 模式（决定分布式函数走 pub/sub）
pub fn is_redis_mode() -> bool {
    REDIS_MODE.load(std::sync::atomic::Ordering::Relaxed)
}

// 便捷函数（对齐旧 API，全部 async）

pub async fn set(key: &str, value: impl Into<String>, ttl: Duration) -> CoolResult<()> {
    store().set(key, value.into(), ttl).await
}

pub async fn get(key: &str) -> CoolResult<Option<String>> {
    store().get(key).await
}

pub async fn remove(key: &str) -> CoolResult<()> {
    store().remove(key).await
}

pub async fn incr(key: &str, ttl: Duration) -> CoolResult<i64> {
    store().incr(key, ttl).await
}

pub async fn get_i64(key: &str) -> i64 {
    get(key)
        .await
        .ok()
        .flatten()
        .and_then(|v| v.parse().ok())
        .unwrap_or(0)
}

pub async fn get_or_set(key: &str, value: impl Into<String>, ttl: Duration) -> CoolResult<String> {
    store().get_or_set(key, value.into(), ttl).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn mem_set_get_remove() {
        let s = MemStore::new();
        s.set("k1", "v1".into(), Duration::from_secs(60))
            .await
            .unwrap();
        assert_eq!(s.get("k1").await.unwrap().as_deref(), Some("v1"));
        s.remove("k1").await.unwrap();
        assert_eq!(s.get("k1").await.unwrap(), None);
    }

    #[tokio::test]
    async fn mem_ttl_expires() {
        let s = MemStore::new();
        s.set("k2", "v2".into(), Duration::from_millis(10))
            .await
            .unwrap();
        assert_eq!(s.get("k2").await.unwrap().as_deref(), Some("v2"));
        tokio::time::sleep(Duration::from_millis(20)).await;
        assert_eq!(s.get("k2").await.unwrap(), None);
    }

    #[tokio::test]
    async fn mem_incr_and_get_or_set() {
        let s = MemStore::new();
        assert_eq!(s.incr("cnt", Duration::from_secs(60)).await.unwrap(), 1);
        assert_eq!(s.incr("cnt", Duration::from_secs(60)).await.unwrap(), 2);

        let v = s
            .get_or_set("gos", "a".into(), Duration::from_secs(60))
            .await
            .unwrap();
        assert_eq!(v, "a");
        let v = s
            .get_or_set("gos", "b".into(), Duration::from_secs(60))
            .await
            .unwrap();
        assert_eq!(v, "a", "should keep first value");
    }
}
