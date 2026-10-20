//! 鉴权中间件：Bearer JWT 校验（M2）。
//!
//! 基于 tower Layer 实现，可读取 gRPC 方法路径（`:path`），用于白名单放行。
//! 白名单方法直接放行；其余校验 `authorization: Bearer <JWT>` 并注入用户信息。
//! 语义对齐 Go 版 `modules/base` + 中间件（含 refreshToken 由业务层校验缓存）。

use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};

use http::{Request, StatusCode};
use http_body_util::{BodyExt, Empty};
use tonic::body::BoxBody;
use tower::{Layer, Service};

use crate::jwt::{JwtConfig, verify as verify_jwt};

fn empty_body() -> BoxBody {
    Empty::new()
        .map_err(|e: std::convert::Infallible| match e {})
        .boxed_unsync()
}

/// 返回 gRPC 可识别的未认证响应
fn unauthorized_response(msg: &str) -> http::Response<BoxBody> {
    let encoded: String = utf8_percent_encode(msg);
    http::Response::builder()
        .status(StatusCode::OK)
        .header("content-type", "application/grpc")
        .header("grpc-status", "16") // UNAUTHENTICATED
        .header("grpc-message", encoded)
        .body(empty_body())
        .unwrap_or_else(|_| http::Response::new(empty_body()))
}

/// 简易百分号编码（grpc-message 要求）
fn utf8_percent_encode(s: &str) -> String {
    let mut out = String::new();
    for b in s.as_bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(*b as char)
            }
            b' ' => out.push('+'),
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

/// 无需鉴权的方法全名
pub const AUTH_WHITELIST: &[&str] = &[
    "internal.v1.InternalService/Hello",
    "internal.v1.InternalService/Health",
    "base.v1.BaseOpenService/Captcha",
    "base.v1.BaseOpenService/Login",
    "base.v1.BaseOpenService/RefreshToken",
    "grpc.reflection.v1alpha.ServerReflection/ServerReflectionInfo",
    "grpc.reflection.v1.ServerReflection/ServerReflectionInfo",
];

/// 鉴权后注入的用户信息（供 handler 读取）
#[derive(Debug, Clone)]
pub struct AuthUser {
    pub user_id: i64,
    pub username: String,
    pub password_version: i32,
    pub role_ids: Vec<String>,
}

/// 鉴权 Layer
#[derive(Clone)]
pub struct AuthLayer {
    /// 是否启用鉴权
    pub enable: bool,
    /// JWT 配置（enable=true 时必填）
    pub jwt: JwtConfig,
}

impl AuthLayer {
    pub fn new(enable: bool, jwt: JwtConfig) -> Self {
        Self { enable, jwt }
    }
}

impl<S> Layer<S> for AuthLayer {
    type Service = AuthService<S>;

    fn layer(&self, inner: S) -> Self::Service {
        AuthService {
            inner,
            enable: self.enable,
            jwt: self.jwt.clone(),
        }
    }
}

#[derive(Clone)]
pub struct AuthService<S> {
    inner: S,
    enable: bool,
    jwt: JwtConfig,
}

fn method_path(uri: &http::Uri) -> String {
    uri.path().trim_start_matches('/').to_string()
}

fn bearer_token(headers: &http::HeaderMap) -> Option<String> {
    headers
        .get(http::header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string())
        .and_then(|s| {
            s.strip_prefix("Bearer ")
                .or_else(|| s.strip_prefix("bearer "))
                .map(|t| t.to_string())
        })
}

impl<S> Service<Request<BoxBody>> for AuthService<S>
where
    S: Service<Request<BoxBody>, Response = http::Response<BoxBody>> + Clone + Send + 'static,
    S::Future: Send + 'static,
    S::Error: Into<Box<dyn std::error::Error + Send + Sync>> + 'static,
{
    type Response = http::Response<BoxBody>;
    type Error = Box<dyn std::error::Error + Send + Sync>;
    type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>> + Send>>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx).map_err(Into::into)
    }

    fn call(&mut self, req: Request<BoxBody>) -> Self::Future {
        let path = method_path(req.uri());

        let allowed = !self.enable
            || AUTH_WHITELIST.contains(&path.as_str())
            || path.starts_with("grpc.reflection.");

        if allowed {
            let mut inner = self.inner.clone();
            let fut = inner.call(req);
            return Box::pin(async move { fut.await.map_err(Into::into) });
        }

        let Some(token) = bearer_token(req.headers()) else {
            tracing::debug!(target: "grpc.auth", %path, "missing bearer");
            return Box::pin(async move { Ok(unauthorized_response("缺少有效的访问令牌")) });
        };

        match verify_jwt(&self.jwt, &token) {
            Ok(claims) => {
                let user = AuthUser {
                    user_id: claims.user_id,
                    username: claims.username,
                    password_version: claims.password_version,
                    role_ids: claims.role_ids,
                };
                let (mut parts, body) = req.into_parts();
                // 通过 extensions 传递给 handler（tonic Request 会带 extensions）
                parts.extensions.insert(user);
                let req = Request::from_parts(parts, body);
                let mut inner = self.inner.clone();
                let fut = inner.call(req);
                Box::pin(async move { fut.await.map_err(Into::into) })
            }
            Err(_) => {
                tracing::debug!(target: "grpc.auth", %path, "invalid jwt");
                Box::pin(async move {
                    Ok(unauthorized_response("登录状态已失效，请重新登录"))
                })
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn method_path_strips_slash() {
        let uri: http::Uri = "/base.v1.BaseOpenService/Login".parse().unwrap();
        assert_eq!(method_path(&uri), "base.v1.BaseOpenService/Login");
    }

    #[test]
    fn whitelist_contains_login() {
        assert!(AUTH_WHITELIST.contains(&"base.v1.BaseOpenService/Login"));
        assert!(AUTH_WHITELIST.contains(&"base.v1.BaseOpenService/Captcha"));
        assert!(AUTH_WHITELIST.contains(&"base.v1.BaseOpenService/RefreshToken"));
    }

    #[test]
    fn bearer_parses() {
        let mut headers = http::HeaderMap::new();
        headers.insert(
            http::header::AUTHORIZATION,
            http::HeaderValue::from_static("Bearer tok123"),
        );
        assert_eq!(bearer_token(&headers).as_deref(), Some("tok123"));
    }

    #[test]
    fn bearer_none_when_missing() {
        let headers = http::HeaderMap::new();
        assert!(bearer_token(&headers).is_none());
    }

    #[test]
    fn percent_encode() {
        assert_eq!(utf8_percent_encode("abc-123"), "abc-123");
        assert!(utf8_percent_encode("失效").contains("%"));
    }
}
