//! 微服务鉴权层。
//!
//! 两种凭证（与 sa-token 分布式会话配合）：
//! 1. **用户 token**：`satoken` / `authorization: Bearer` → 共享 Redis 存储校验
//! 2. **服务凭证**：`x-service-id` + `x-service-secret` → DistributedSessionManager.verify_service
//!
//! 两者任一通过即可访问受保护 RPC（服务间调用无需用户 token）。

use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};

use http::{Request, Response, StatusCode};
use http_body_util::{BodyExt, Empty};
use sa_token_core::DistributedSessionManager;
use sa_token_plugin_tonic::SaTokenState;
use tonic::body::BoxBody;
use tower::{Layer, Service};

pub const SERVICE_HEADER_ID: &str = "x-service-id";
pub const SERVICE_HEADER_SECRET: &str = "x-service-secret";

fn empty_body() -> BoxBody {
    Empty::new()
        .map_err(|e: std::convert::Infallible| match e {})
        .boxed_unsync()
}

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

fn reject(msg: &str) -> Response<BoxBody> {
    let encoded = utf8_percent_encode(msg);
    http::Response::builder()
        .status(StatusCode::OK)
        .header("content-type", "application/grpc")
        .header("grpc-status", "16")
        .header("grpc-message", encoded)
        .body(empty_body())
        .unwrap_or_else(|_| http::Response::new(empty_body()))
}

/// 公开接口（免鉴权），格式 `<package>.<Service>/<Method>`（无前导斜杠）
pub const PUBLIC_PATHS: &[&str] = &[
    "base.v1.BaseOpenService/Captcha",
    "base.v1.BaseOpenService/Login",
    "base.v1.BaseOpenService/RefreshToken",
];

fn is_public(path: &str) -> bool {
    let p = path.trim_start_matches('/');
    PUBLIC_PATHS.contains(&p)
        || p.starts_with("internal.v1.InternalService/")
        || p.starts_with("grpc.reflection.")
}

/// 服务鉴权 Layer
#[derive(Clone)]
pub struct ServiceAuthLayer {
    pub enable: bool,
    pub state: SaTokenState,
    pub dsm: Arc<DistributedSessionManager>,
}

impl ServiceAuthLayer {
    pub fn new(enable: bool, state: SaTokenState, dsm: Arc<DistributedSessionManager>) -> Self {
        Self {
            enable,
            state,
            dsm,
        }
    }
}

impl<S> Layer<S> for ServiceAuthLayer {
    type Service = ServiceAuthMiddleware<S>;

    fn layer(&self, inner: S) -> Self::Service {
        ServiceAuthMiddleware {
            inner,
            enable: self.enable,
            state: self.state.clone(),
            dsm: self.dsm.clone(),
        }
    }
}

#[derive(Clone)]
pub struct ServiceAuthMiddleware<S> {
    inner: S,
    enable: bool,
    state: SaTokenState,
    dsm: Arc<DistributedSessionManager>,
}

fn header_str(headers: &http::HeaderMap, name: &str) -> Option<String> {
    headers
        .get(name)
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string())
}

fn bearer_token(headers: &http::HeaderMap) -> Option<String> {
    if let Some(v) = header_str(headers, "satoken") {
        let v = v.trim().to_string();
        return Some(
            v.strip_prefix("Bearer ")
                .or_else(|| v.strip_prefix("bearer "))
                .unwrap_or(&v)
                .to_string(),
        );
    }
    header_str(headers, "authorization").and_then(|s| {
        s.strip_prefix("Bearer ")
            .or_else(|| s.strip_prefix("bearer "))
            .map(|t| t.to_string())
    })
}

impl<S> Service<Request<BoxBody>> for ServiceAuthMiddleware<S>
where
    S: Service<Request<BoxBody>, Response = Response<BoxBody>> + Clone + Send + 'static,
    S::Future: Send + 'static,
    S::Error: Into<Box<dyn std::error::Error + Send + Sync>> + 'static,
{
    type Response = Response<BoxBody>;
    type Error = Box<dyn std::error::Error + Send + Sync>;
    type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>> + Send>>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx).map_err(Into::into)
    }

    fn call(&mut self, req: Request<BoxBody>) -> Self::Future {
        let path = req.uri().path().trim_start_matches('/').to_string();

        if !self.enable || is_public(&path) {
            let mut inner = self.inner.clone();
            let fut = inner.call(req);
            return Box::pin(async move { fut.await.map_err(Into::into) });
        }

        // 1) 服务凭证（服务间调用）
        let svc_id = header_str(req.headers(), SERVICE_HEADER_ID);
        let svc_secret = header_str(req.headers(), SERVICE_HEADER_SECRET);
        if let (Some(id), Some(secret)) = (svc_id, svc_secret) {
            let dsm = self.dsm.clone();
            let mut inner = self.inner.clone();
            return Box::pin(async move {
                match dsm.verify_service(&id, &secret).await {
                    Ok(cred) => {
                        tracing::debug!(target: "micro.auth", %path, service = %cred.service_id, "service credential ok");
                        let fut = inner.call(req);
                        fut.await.map_err(Into::into)
                    }
                    Err(e) => {
                        tracing::warn!(target: "micro.auth", %path, "service credential rejected: {e}");
                        Ok(reject("服务凭证无效"))
                    }
                }
            });
        }

        // 2) 用户 token（共享 Redis 分布式会话）
        let Some(token) = bearer_token(req.headers()) else {
            tracing::debug!(target: "micro.auth", %path, "missing credentials");
            return Box::pin(async move { Ok(reject("缺少有效的访问令牌")) });
        };

        let state = self.state.clone();
        let mut inner = self.inner.clone();
        Box::pin(async move {
            let tv = sa_token_plugin_tonic::StpUtil::create_token(token);
            let ok = {
                let _ = &state;
                sa_token_plugin_tonic::StpUtil::is_login(&tv).await
            };
            if !ok {
                tracing::debug!(target: "micro.auth", %path, "invalid user token");
                return Ok(reject("登录状态已失效，请重新登录"));
            }
            // 取 login_id 注入扩展
            let login_id = sa_token_plugin_tonic::StpUtil::get_login_id(&tv)
                .await
                .ok();
            if let Some(lid) = login_id {
                let (mut parts, body) = req.into_parts();
                parts.extensions.insert(LoginId(lid));
                let req = Request::from_parts(parts, body);
                let fut = inner.call(req);
                fut.await.map_err(Into::into)
            } else {
                let fut = inner.call(req);
                fut.await.map_err(Into::into)
            }
        })
    }
}

/// 注入的登录 ID
#[derive(Debug, Clone)]
pub struct LoginId(pub String);
