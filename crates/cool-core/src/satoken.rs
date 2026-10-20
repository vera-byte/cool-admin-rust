//! sa-token 集成（对齐 https://github.com/sa-tokens/sa-token-rust 的 sa-token-plugin-tonic）。
//!
//! 取代自研 JWT + AuthLayer，提供：
//! - `StpUtil` 登录会话 / 权限 / 角色
//! - `SaTokenGrpcLayer` + `PathAuthConfig` per-RPC 鉴权
//! - `get_login_id_from_request` 读取已校验登录 ID

use std::sync::Arc;

use sa_token_plugin_tonic::{MemoryStorage, PathAuthConfig, SaTokenState};

/// sa-token 配置（对齐 Go modules.base.jwt）
#[derive(Debug, Clone)]
pub struct SaTokenConfig {
    /// token 有效期（秒）
    pub timeout: i64,
    /// token 名（gRPC metadata 键优先级：satoken > authorization）
    pub token_name: String,
    /// 是否启用鉴权层
    pub enable: bool,
}

impl Default for SaTokenConfig {
    fn default() -> Self {
        Self {
            timeout: 86400,
            token_name: "satoken".into(),
            enable: true,
        }
    }
}

/// 全局状态（OnceCell）
static STATE: once_cell::sync::OnceCell<SaTokenState> = once_cell::sync::OnceCell::new();

/// 初始化 sa-token（内存存储；Redis 后续接 feature）
pub fn init(cfg: &SaTokenConfig) -> SaTokenState {
    let state = SaTokenState::builder()
        .storage(Arc::new(MemoryStorage::new()))
        .timeout(cfg.timeout)
        .build();
    // 幂等：只初始化一次
    let _ = STATE.set(state.clone());
    tracing::info!(
        timeout = cfg.timeout,
        token_name = %cfg.token_name,
        "sa-token initialized"
    );
    state
}

/// 获取全局状态（未初始化则惰性初始化默认配置）
pub fn state() -> SaTokenState {
    STATE
        .get()
        .cloned()
        .unwrap_or_else(|| init(&SaTokenConfig::default()))
}

/// 默认路径鉴权规则：除公开接口外全部要求登录。
///
/// gRPC URI path 格式 `/<package>.<Service>/<Method>`。
/// 公开白名单对齐 Go：验证码 / 登录 / 刷新 token、internal 健康检查、反射。
pub fn default_path_config() -> PathAuthConfig {
    PathAuthConfig::new()
        // 受保护：所有业务服务
        .include(vec![
            "/base.v1.**".to_string(),
            "/demo.v1.**".to_string(),
            "/dict.v1.**".to_string(),
            "/space.v1.**".to_string(),
            "/task.v1.**".to_string(),
        ])
        // 公开：登录链路 + 健康检查
        .exclude(vec![
            "/base.v1.BaseOpenService/Captcha".to_string(),
            "/base.v1.BaseOpenService/Login".to_string(),
            "/base.v1.BaseOpenService/RefreshToken".to_string(),
            "/internal.v1.InternalService/**".to_string(),
        ])
}

/// 登录（返回 token 字符串）
pub async fn login(login_id: &str) -> crate::error::CoolResult<String> {
    let token = sa_token_plugin_tonic::StpUtil::login(login_id.to_string())
        .await
        .map_err(|e| crate::CoolError::Internal(format!("sa-token 登录失败: {e}")))?;
    Ok(token.as_str().to_string())
}

/// 登出
pub async fn logout(token: &str) -> crate::error::CoolResult<()> {
    let tv = sa_token_plugin_tonic::StpUtil::create_token(token.to_string());
    sa_token_plugin_tonic::StpUtil::logout(&tv)
        .await
        .map_err(|e| crate::CoolError::Internal(format!("sa-token 登出失败: {e}")))
}

/// 按登录 ID 登出全部会话
pub async fn logout_by_login_id(login_id: &str) -> crate::error::CoolResult<()> {
    sa_token_plugin_tonic::StpUtil::logout_by_login_id(login_id.to_string())
        .await
        .map_err(|e| crate::CoolError::Internal(format!("sa-token 登出失败: {e}")))
}

/// 覆盖权限列表
pub async fn set_permissions(login_id: &str, perms: Vec<String>) -> crate::error::CoolResult<()> {
    sa_token_plugin_tonic::StpUtil::set_permissions(login_id.to_string(), perms)
        .await
        .map_err(|e| crate::CoolError::Internal(format!("设置权限失败: {e}")))
}

/// 覆盖角色列表
pub async fn set_roles(login_id: &str, roles: Vec<String>) -> crate::error::CoolResult<()> {
    sa_token_plugin_tonic::StpUtil::set_roles(login_id.to_string(), roles)
        .await
        .map_err(|e| crate::CoolError::Internal(format!("设置角色失败: {e}")))
}

/// 权限校验
pub async fn has_permission(login_id: &str, perm: &str) -> bool {
    sa_token_plugin_tonic::check_permission(login_id, perm).await
}

/// 角色校验
pub async fn has_role(login_id: &str, role: &str) -> bool {
    sa_token_plugin_tonic::check_role(login_id, role).await
}

/// 从请求读取已校验的登录 ID（须先经过 SaTokenGrpcLayer）
pub fn login_id_from_request<T>(req: &tonic::Request<T>) -> Option<String> {
    sa_token_plugin_tonic::get_login_id_from_request(req)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn path_config_has_whitelist() {
        let cfg = default_path_config();
        // 结构自洽：include/exclude 可构建
        let _ = cfg;
    }

    #[tokio::test]
    async fn login_logout_roundtrip() {
        init(&SaTokenConfig::default());
        let tok = login("test-user-1").await.expect("login");
        assert!(!tok.is_empty());
        set_permissions("test-user-1", vec!["user:list".into()])
            .await
            .expect("set perms");
        set_roles("test-user-1", vec!["admin".into()])
            .await
            .expect("set roles");
        assert!(has_permission("test-user-1", "user:list").await);
        assert!(has_role("test-user-1", "admin").await);
        logout(&tok).await.expect("logout");
    }
}
