//! JWT 签发与校验（对齐 cool-admin-go `cool.Claims` + modules/base 登录）。
//!
//! 安全项（对齐 Go 1.5.12）：
//! - 弱密钥黑名单
//! - refreshToken 校验登录缓存
//! - passwordVersion：改密后旧 token 失效

use std::collections::HashSet;
use std::time::{SystemTime, UNIX_EPOCH};

use jsonwebtoken::{Algorithm, DecodingKey, EncodingKey, Header, Validation, decode, encode};
use serde::{Deserialize, Serialize};

use crate::error::{CoolError, CoolResult};

/// JWT claims（字段名对齐 Go 版）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Claims {
    /// 是否为 refreshToken
    #[serde(default, rename = "isRefresh")]
    pub is_refresh: bool,
    #[serde(default, rename = "roleIds")]
    pub role_ids: Vec<String>,
    #[serde(default, rename = "username")]
    pub username: String,
    #[serde(default, rename = "userId")]
    pub user_id: i64,
    /// 密码版本，改密后递增使旧 token 失效
    #[serde(default, rename = "passwordVersion")]
    pub password_version: i32,
    /// exp（秒）
    #[serde(rename = "exp")]
    pub exp: u64,
    /// iat（秒）
    #[serde(rename = "iat")]
    pub iat: u64,
}

impl Claims {
    pub fn now_secs() -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0)
    }
}

/// JWT 配置
#[derive(Debug, Clone)]
pub struct JwtConfig {
    pub secret: String,
    /// 访问 token 有效期（秒）
    pub expire: u64,
    /// 刷新 token 有效期（秒）
    pub refresh_expire: u64,
}

impl Default for JwtConfig {
    fn default() -> Self {
        Self {
            secret: "cool-base88776655".into(),
            expire: 10, // 对齐 config.yaml 注释单位（开发方便）；生产建议 2*3600
            refresh_expire: 1_296_000,
        }
    }
}

/// 弱密钥黑名单（对齐 Go 1.5.12 加固）
const WEAK_SECRETS: &[&str] = &[
    "secret",
    "123456",
    "12345678",
    "password",
    "admin",
    "test",
    "cool-admin",
    "cool-base88776655-dev",
    "changeme",
    "jwt-secret",
];

/// 密钥强度检查：过短或黑名单精确命中均拒绝
pub fn validate_secret(secret: &str) -> CoolResult<()> {
    let s = secret.trim();
    if s.len() < 16 {
        return Err(CoolError::Internal("JWT secret 太短（至少 16 位）".into()));
    }
    let lower = s.to_lowercase();
    if WEAK_SECRETS.iter().any(|w| lower == *w) {
        return Err(CoolError::Internal("JWT secret 过于简单，已被禁止".into()));
    }
    Ok(())
}

/// 签发 token
pub fn sign(cfg: &JwtConfig, claims: &Claims) -> CoolResult<String> {
    encode(
        &Header::new(Algorithm::HS256),
        claims,
        &EncodingKey::from_secret(cfg.secret.as_bytes()),
    )
    .map_err(|e| CoolError::Internal(format!("JWT 签发失败: {e}")))
}

/// 校验 token，返回 claims
pub fn verify(cfg: &JwtConfig, token: &str) -> CoolResult<Claims> {
    let mut validation = Validation::new(Algorithm::HS256);
    validation.validate_exp = true;
    validation.required_spec_claims = HashSet::from(["exp".to_string()]);

    let data = decode::<Claims>(
        token,
        &DecodingKey::from_secret(cfg.secret.as_bytes()),
        &validation,
    )
    .map_err(|_| CoolError::unauthenticated("登录状态已失效，请重新登录"))?;

    Ok(data.claims)
}

/// 生成访问 token
pub fn access_token(
    cfg: &JwtConfig,
    user_id: i64,
    username: &str,
    role_ids: &[String],
    password_version: i32,
) -> CoolResult<String> {
    let now = Claims::now_secs();
    let claims = Claims {
        is_refresh: false,
        role_ids: role_ids.to_vec(),
        username: username.to_string(),
        user_id,
        password_version,
        exp: now + cfg.expire,
        iat: now,
    };
    sign(cfg, &claims)
}

/// 生成刷新 token
pub fn refresh_token(
    cfg: &JwtConfig,
    user_id: i64,
    username: &str,
    role_ids: &[String],
    password_version: i32,
) -> CoolResult<String> {
    let now = Claims::now_secs();
    let claims = Claims {
        is_refresh: true,
        role_ids: role_ids.to_vec(),
        username: username.to_string(),
        user_id,
        password_version,
        exp: now + cfg.refresh_expire,
        iat: now,
    };
    sign(cfg, &claims)
}

/// Token 结果（对齐 Go TokenResult）
#[derive(Debug, Clone, Serialize)]
pub struct TokenResult {
    #[serde(rename = "expire")]
    pub expire: u64,
    pub token: String,
    #[serde(rename = "refreshExpire")]
    pub refresh_expire: u64,
    #[serde(rename = "refreshToken")]
    pub refresh_token: String,
}

pub fn issue_tokens(
    cfg: &JwtConfig,
    user_id: i64,
    username: &str,
    role_ids: &[String],
    password_version: i32,
) -> CoolResult<TokenResult> {
    Ok(TokenResult {
        expire: cfg.expire,
        token: access_token(cfg, user_id, username, role_ids, password_version)?,
        refresh_expire: cfg.refresh_expire,
        refresh_token: refresh_token(cfg, user_id, username, role_ids, password_version)?,
    })
}

/// 过期剩余秒数（辅助）
pub fn remaining_secs(exp: u64) -> i64 {
    let now = Claims::now_secs();
    exp as i64 - now as i64
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sign_and_verify_roundtrip() {
        let cfg = JwtConfig {
            secret: "unit-test-secret-123456".into(),
            expire: 60,
            refresh_expire: 120,
        };
        let tok = access_token(&cfg, 1, "admin", &["1".into()], 1).unwrap();
        let claims = verify(&cfg, &tok).unwrap();
        assert_eq!(claims.user_id, 1);
        assert_eq!(claims.username, "admin");
        assert!(!claims.is_refresh);
        assert_eq!(claims.password_version, 1);
    }

    #[test]
    fn refresh_token_flag() {
        let cfg = JwtConfig {
            secret: "unit-test-secret-123456".into(),
            expire: 60,
            refresh_expire: 120,
        };
        let tok = refresh_token(&cfg, 2, "u", &[], 3).unwrap();
        let claims = verify(&cfg, &tok).unwrap();
        assert!(claims.is_refresh);
        assert_eq!(claims.password_version, 3);
    }

    #[test]
    fn reject_tampered_token() {
        let cfg = JwtConfig {
            secret: "unit-test-secret-123456".into(),
            ..Default::default()
        };
        let tok = access_token(&cfg, 1, "admin", &[], 1).unwrap();
        let bad = format!("{tok}x");
        assert!(verify(&cfg, &bad).is_err());
    }

    #[test]
    fn weak_secret_rejected() {
        assert!(validate_secret("123456").is_err()); // 过短
        assert!(validate_secret("short").is_err()); // 过短
        assert!(validate_secret("cool-base88776655-dev").is_err()); // 黑名单精确命中
        assert!(validate_secret("unit-test-secret-123456").is_ok());
    }
}
