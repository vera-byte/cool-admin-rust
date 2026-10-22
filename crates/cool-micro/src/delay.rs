//! 测试用响应延迟注入（混沌工程）。
//!
//! 设置环境变量 `COOL_TEST_RESPONSE_DELAY_MS` 后，所有请求延迟该毫秒数返回。
//! 用于模拟慢实例、验证超时与健康检查行为。**生产环境勿开启。**

use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};
use std::time::Duration;

use http::{Request, Response};
use tonic::body::BoxBody;
use tower::{Layer, Service};

/// 延迟 Layer
#[derive(Clone, Default)]
pub struct DelayLayer {
    pub delay: Duration,
}

impl DelayLayer {
    pub fn from_env() -> Option<Self> {
        std::env::var("COOL_TEST_RESPONSE_DELAY_MS")
            .ok()
            .and_then(|v| v.parse::<u64>().ok())
            .map(|ms| {
                tracing::warn!(ms, "COOL_TEST_RESPONSE_DELAY_MS enabled (chaos testing)");
                DelayLayer {
                    delay: Duration::from_millis(ms),
                }
            })
    }
}

impl<S> Layer<S> for DelayLayer {
    type Service = DelayService<S>;
    fn layer(&self, inner: S) -> Self::Service {
        DelayService {
            inner,
            delay: self.delay,
        }
    }
}

#[derive(Clone)]
pub struct DelayService<S> {
    inner: S,
    delay: Duration,
}

impl<S> Service<Request<BoxBody>> for DelayService<S>
where
    S: Service<Request<BoxBody>, Response = Response<BoxBody>> + Clone + Send + 'static,
    S::Future: Send + 'static,
    S::Error: Send + 'static,
{
    type Response = Response<BoxBody>;
    type Error = S::Error;
    type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>> + Send>>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx)
    }

    fn call(&mut self, req: Request<BoxBody>) -> Self::Future {
        let delay = self.delay;
        let mut inner = self.inner.clone();
        Box::pin(async move {
            if !delay.is_zero() {
                tokio::time::sleep(delay).await;
            }
            inner.call(req).await
        })
    }
}
