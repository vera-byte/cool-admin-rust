//! sa-token 分布式会话初始化。
//!
//! 多个微服务共享同一 Redis 存储 → 用户 token 跨服务可校验；
//! `DistributedSessionManager` 管理服务凭证与共享业务会话属性。

use std::sync::Arc;
use std::time::Duration;

use anyhow::Result;
use chrono::Utc;
use sa_token_core::{
    DistributedSessionManager, SaStorageDistributedStorage, ServiceCredential,
};
use sa_token_plugin_tonic::SaTokenState;
use sa_token_storage_redis::{RedisConfig, RedisStorage};

use crate::config::MicroConfig;

/// 服务身份（用于 ServiceCredential）
#[derive(Debug, Clone)]
pub struct ServiceIdentity {
    pub id: String,
    pub name: String,
    pub secret: String,
}

/// 微服务会话上下文
pub struct MicroSession {
    pub state: SaTokenState,
    pub dsm: Arc<DistributedSessionManager>,
    pub identity: ServiceIdentity,
}

/// 初始化 sa-token + 分布式会话。
///
/// - **Redis 存储**：多服务共享登录态（分布式 session 核心）
/// - **DistributedSessionManager**：服务凭证 + 共享业务属性
/// - 每个服务以自己的 `service_id` 注册凭证
pub async fn init_distributed(cfg: &MicroConfig) -> Result<MicroSession> {
    // 1) Redis 存储（多服务必须同一 key_prefix）
    let key_prefix = cfg.sa_token.key_prefix.clone();
    let redis_cfg = parse_redis(&cfg.redis);
    let storage = match RedisStorage::from_config(redis_cfg, key_prefix.clone()).await {
        Ok(s) => Arc::new(s) as Arc<dyn sa_token_adapter::SaStorage>,
        Err(e) => {
            tracing::warn!("redis storage unavailable, fallback memory: {e}");
            Arc::new(sa_token_plugin_tonic::MemoryStorage::new())
                as Arc<dyn sa_token_adapter::SaStorage>
        }
    };

    // 2) SaTokenState（用户会话）
    let state = SaTokenState::builder()
        .storage(storage.clone())
        .timeout(cfg.sa_token.timeout)
        .build();

    // 3) 分布式会话管理器（服务凭证 + 共享属性）
    let dsm = DistributedSessionManager::new(
        Arc::new(SaStorageDistributedStorage::new(
            storage.clone(),
            key_prefix.clone(),
        )),
        cfg.service.id.clone(),
        Duration::from_secs(cfg.sa_token.timeout.max(60) as u64),
    );

    // 4) 注册本服务凭证（供其他服务 verify_service）
    let cred = ServiceCredential {
        service_id: cfg.service.id.clone(),
        service_name: cfg.service.name.clone(),
        secret_key: cfg.service.secret.clone(),
        created_at: Utc::now(),
        permissions: vec![],
    };
    if let Err(e) = dsm.register_service(cred).await {
        tracing::warn!("register service credential failed: {e}");
    }

    // 5) 自校验
    match dsm.verify_service(&cfg.service.id, &cfg.service.secret).await {
        Ok(_) => tracing::info!(
            service = %cfg.service.id,
            "distributed session ready (credential verified)"
        ),
        Err(e) => tracing::warn!("service credential verify failed: {e}"),
    }

    tracing::info!(
        service = %cfg.service.id,
        key_prefix = %key_prefix,
        timeout = cfg.sa_token.timeout,
        "sa-token distributed session initialized"
    );

    Ok(MicroSession {
        state,
        dsm: Arc::new(dsm),
        identity: ServiceIdentity {
            id: cfg.service.id.clone(),
            name: cfg.service.name.clone(),
            secret: cfg.service.secret.clone(),
        },
    })
}

/// 解析 Redis 配置
fn parse_redis(r: &crate::config::MicroRedisConfig) -> RedisConfig {
    let raw = r
        .address
        .trim()
        .trim_start_matches("redis://")
        .trim_end_matches('/');
    let (host, port) = match raw.rsplit_once(':') {
        Some((h, p)) if !h.is_empty() && p.chars().all(|c| c.is_ascii_digit()) => {
            (h.to_string(), p.parse().unwrap_or(6379))
        }
        _ => (raw.to_string(), 6379),
    };
    RedisConfig {
        host,
        port,
        password: r.password.clone(),
        database: r.db.clamp(0, 255) as u8,
        pool_size: 10,
    }
}
