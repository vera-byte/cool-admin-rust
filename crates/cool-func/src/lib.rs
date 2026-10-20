//! 分布式函数（对齐 cool-admin-go `cool/func.go`）。
//!
//! - `CoolFunc`：函数接口（IsSingleton / IsAllWorker）
//! - `run_func("Name(arg)")`：本地执行；非 all_worker 需 master 选举通过
//! - `cluster_run_func`：Redis 模式 PUBLISH `cool:func`，否则本地执行
//! - `listen_func`：SUBSCRIBE `cool:func` 循环执行

use std::collections::HashMap;
use std::sync::Arc;
use std::sync::RwLock;
use std::time::Duration;

use once_cell::sync::Lazy;

use cool_core::cache;
use cool_core::error::{CoolError, CoolResult};
use cool_core::instance_id;

/// 函数频道（对齐 Go `cool:func`）
pub const CHANNEL: &str = "cool:func";
/// master 标记键（对齐 Go `cool:masterflag`）
pub const MASTER_KEY: &str = "cool:masterflag";
/// master 标记 TTL
pub const MASTER_TTL: Duration = Duration::from_secs(60);

// ---------------- trait ----------------

/// 可注册的函数（对齐 Go CoolFunc）
#[async_trait::async_trait]
pub trait CoolFunc: Send + Sync {
    async fn func(&self, param: &str) -> CoolResult<()>;
    /// 单例：调度器内防重叠
    fn is_singleton(&self) -> bool {
        false
    }
    /// 所有 worker 都执行；false 时仅 master 执行
    fn is_all_worker(&self) -> bool {
        true
    }
}

// ---------------- registry ----------------

type FuncMap = HashMap<String, Arc<dyn CoolFunc>>;

static FUNCS: Lazy<RwLock<FuncMap>> = Lazy::new(|| RwLock::new(HashMap::new()));

/// 注册函数
pub fn register(name: &str, f: Arc<dyn CoolFunc>) {
    tracing::info!(target: "cool.func", name, "register func");
    FUNCS.write().unwrap().insert(name.to_string(), f);
}

/// 查找函数
pub fn get(name: &str) -> Option<Arc<dyn CoolFunc>> {
    FUNCS.read().unwrap().get(name).cloned()
}

/// 解析 "Name(param)" → (Name, param)；无括号时 param 为空串
pub fn parse_funcstring(s: &str) -> (String, String) {
    let s = s.trim();
    match s.find('(') {
        Some(i) => {
            let name = s[..i].trim().to_string();
            let rest = &s[i + 1..];
            let param = match rest.rfind(')') {
                Some(j) => rest[..j].to_string(),
                None => rest.to_string(),
            };
            (name, param)
        }
        None => (s.to_string(), String::new()),
    }
}

/// 是否 master（对齐 Go：MustGetOrSet cool:masterflag）
pub async fn is_master() -> bool {
    let me = instance_id().to_string();
    match cache::get_or_set(MASTER_KEY, me.clone(), MASTER_TTL).await {
        Ok(v) => v == me,
        Err(_) => false,
    }
}

/// 本地执行（对齐 Go RunFunc）
pub async fn run_func(funcstring: &str) -> CoolResult<()> {
    let (name, param) = parse_funcstring(funcstring);
    let Some(f) = get(&name) else {
        return Err(CoolError::biz(format!("函数不存在:{name}")));
    };
    if !f.is_all_worker() && !is_master().await {
        tracing::debug!(target: "cool.func", name, "not master, skip");
        return Ok(());
    }
    tracing::debug!(target: "cool.func", name, %param, "run_func");
    f.func(&param).await
}

/// 集群执行：Redis 模式广播，否则本地（对齐 Go ClusterRunFunc）
pub async fn cluster_run_func(funcstring: &str) -> CoolResult<()> {
    if cache::is_redis_mode() {
        publish(CHANNEL, funcstring).await
    } else {
        run_func(funcstring).await
    }
}

/// PUBLISH 到频道（要求 Redis 模式）
pub async fn publish(channel: &str, message: &str) -> CoolResult<()> {
    let url = redis_url();
    let client = redis::Client::open(url.as_str())
        .map_err(|e| CoolError::Internal(format!("redis 配置错误: {e}")))?;
    let mut conn = client
        .get_multiplexed_async_connection()
        .await
        .map_err(|e| CoolError::Internal(format!("redis 连接失败: {e}")))?;
    let _: i64 = redis::cmd("PUBLISH")
        .arg(channel)
        .arg(message)
        .query_async(&mut conn)
        .await
        .map_err(|e| CoolError::Internal(format!("redis PUBLISH 失败: {e}")))?;
    Ok(())
}

/// 监听函数频道（对齐 Go ListenFunc）；调用方 spawn。
/// Redis 未启用时直接返回。
pub async fn listen_func() {
    if !cache::is_redis_mode() {
        tracing::debug!(target: "cool.func", "listen_func skipped (no redis)");
        return;
    }
    let url = redis_url();
    loop {
        match run_listener(&url).await {
            Ok(()) => break,
            Err(e) => {
                tracing::warn!(target: "cool.func", "listener error: {e}; retry in 10s");
                tokio::time::sleep(Duration::from_secs(10)).await;
            }
        }
    }
}

async fn run_listener(url: &str) -> CoolResult<()> {
    let client = redis::Client::open(url)
        .map_err(|e| CoolError::Internal(format!("redis 配置错误: {e}")))?;
    let mut pubsub = client
        .get_async_pubsub()
        .await
        .map_err(|e| CoolError::Internal(format!("redis pubsub 连接失败: {e}")))?;
    pubsub
        .subscribe(CHANNEL)
        .await
        .map_err(|e| CoolError::Internal(format!("redis SUBSCRIBE 失败: {e}")))?;
    tracing::info!(target: "cool.func", channel = CHANNEL, "listening");

    let mut stream = pubsub.on_message();
    use futures_util::StreamExt;
    while let Some(msg) = stream.next().await {
        let payload: String = msg.get_payload().unwrap_or_default();
        if payload.is_empty() {
            continue;
        }
        if let Err(e) = run_func(&payload).await {
            tracing::warn!(target: "cool.func", "run from pubsub failed: {e}");
        }
    }
    Ok(())
}

/// Redis URL（与 cache 相同配置）；从环境变量或全局设置读取。
/// 简化：由 `set_redis_url` 在启动时注入。
static REDIS_URL: Lazy<RwLock<String>> = Lazy::new(|| RwLock::new(String::new()));

pub fn set_redis_url(url: &str) {
    *REDIS_URL.write().unwrap() = url.to_string();
}

fn redis_url() -> String {
    let u = REDIS_URL.read().unwrap().clone();
    if u.is_empty() {
        "redis://127.0.0.1:6379".into()
    } else {
        // 支持 host:port 简写
        if u.contains("://") {
            u
        } else {
            format!("redis://{u}")
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Echo;
    #[async_trait::async_trait]
    impl CoolFunc for Echo {
        async fn func(&self, param: &str) -> CoolResult<()> {
            assert_eq!(param, "hi");
            Ok(())
        }
    }

    struct MasterOnly;
    #[async_trait::async_trait]
    impl CoolFunc for MasterOnly {
        async fn func(&self, _param: &str) -> CoolResult<()> {
            Ok(())
        }
        fn is_all_worker(&self) -> bool {
            false
        }
    }

    #[test]
    fn parse_funcstring_cases() {
        assert_eq!(
            parse_funcstring("TaskTest(hello)"),
            ("TaskTest".into(), "hello".into())
        );
        assert_eq!(
            parse_funcstring("TaskTest(a,b)"),
            ("TaskTest".into(), "a,b".into())
        );
        assert_eq!(
            parse_funcstring("TaskTest()"),
            ("TaskTest".into(), "".into())
        );
        assert_eq!(parse_funcstring("NoParen"), ("NoParen".into(), "".into()));
        assert_eq!(parse_funcstring("  X( y )  "), ("X".into(), " y ".into()));
    }

    #[tokio::test]
    async fn run_func_registered() {
        register("Echo", Arc::new(Echo));
        run_func("Echo(hi)").await.unwrap();
    }

    #[tokio::test]
    async fn run_func_missing() {
        let e = run_func("Nope(x)").await.unwrap_err();
        assert!(e.to_string().contains("函数不存在"));
    }

    #[tokio::test]
    async fn master_gate_allows_all_worker() {
        register("Echo2", Arc::new(Echo));
        run_func("Echo2(hi)").await.unwrap();
    }
}
