//! dict 微服务：独立进程，sa-token 分布式会话鉴权。

use anyhow::Result;
use cool_proto::dict::v1::dict_info_service_server::DictInfoServiceServer;
use cool_proto::dict::v1::dict_type_service_server::DictTypeServiceServer;
use tonic::transport::Server;
use tracing::info;

#[tokio::main]
async fn main() -> Result<()> {
    let cfg = match cool_micro::MicroConfig::load(None) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("配置加载失败: {e:#}");
            std::process::exit(1);
        }
    };
    cool_micro::init_tracing(&cfg.server.log_level);
    // 混沌注入：CPU/内存压力（COOL_TEST_CHAOS_*）
    cool_micro::spawn_resource_pressure();
    info!(service = %cfg.service.id, addr = %cfg.server.address, "starting dict-service");

    let pool = cool_db::connect(&cfg.db_config()).await?;
    if cfg.cool.auto_migrate {
        app_dict::migrate(&pool).await?;
    }
    let session = cool_micro::init_distributed(&cfg).await?;

    // HTTP 健康端点（APISIX 主动检查用，端口 = gRPC 端口 + 10000）
    let grpc_port = cfg.socket_addr()?.port();
    cool_micro::spawn_health_server(grpc_port + 10000).await;

    let (svc_a, svc_b) = app_dict::build_services(pool.clone());

    // 测试用延迟注入（COOL_TEST_RESPONSE_DELAY_MS）
    let mut router = Server::builder()
        .layer(cool_micro::DelayLayer::from_env().unwrap_or_default())
        .layer(cool_core::LoggingLayer)
        .layer(cool_micro::ServiceAuthLayer::new(
            cfg.sa_token.enable,
            session.state.clone(),
            session.dsm.clone(),
        ))
        .add_service(DictTypeServiceServer::new(svc_a))
        .add_service(DictInfoServiceServer::new(svc_b));

    if cfg.server.reflection {
        let refl = tonic_reflection::server::Builder::configure()
            .register_encoded_file_descriptor_set(cool_proto::REFLECTION_DESCRIPTOR)
            .build_v1()
            .map_err(|e| anyhow::anyhow!("{e}"))?;
        router = router.add_service(refl);
    }

    let addr = cfg.socket_addr()?;
    info!(%addr, "dict-service listening");
    router.serve(addr).await?;
    Ok(())
}
