//! gRPC 请求日志拦截/中间件（对齐 Go 版 middleware/log）。
//!
//! M0：记录方法名、耗时、状态码；参数脱敏在 M2 随 base 日志模块完善。

use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};
use std::time::Instant;

use tower::{Layer, Service};
use tracing::{Instrument, Level, info_span};

/// 日志 Layer
#[derive(Clone, Default)]
pub struct LoggingLayer;

impl<S> Layer<S> for LoggingLayer {
    type Service = LoggingService<S>;

    fn layer(&self, inner: S) -> Self::Service {
        LoggingService { inner }
    }
}

#[derive(Clone)]
pub struct LoggingService<S> {
    inner: S,
}

impl<S, Req, Res> Service<Req> for LoggingService<S>
where
    S: Service<Req, Response = Res> + Clone + Send + 'static,
    S::Future: Send + 'static,
    Req: Send + 'static,
{
    type Response = S::Response;
    type Error = S::Error;
    type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>> + Send>>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx)
    }

    fn call(&mut self, req: Req) -> Self::Future {
        let start = Instant::now();
        let mut inner = self.inner.clone();
        let fut = inner.call(req);

        Box::pin(
            async move {
                let result = fut.await;
                let cost = start.elapsed();
                match &result {
                    Ok(_) => {
                        tracing::info!(target: "grpc", cost_ms = cost.as_millis() as u64, "request ok");
                    }
                    Err(_) => {
                        tracing::warn!(target: "grpc", cost_ms = cost.as_millis() as u64, "request error");
                    }
                }
                result
            }
            .instrument(info_span!("grpc_request")),
        )
    }
}

/// 初始化 tracing（可由 RUST_LOG 控制级别）
pub fn init_tracing(default_level: &str) {
    let filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new(default_level));

    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_target(true)
        .with_level(true)
        .init();

    tracing::event!(Level::INFO, "tracing initialized");
}
