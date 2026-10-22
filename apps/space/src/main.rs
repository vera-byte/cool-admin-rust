//! space 微服务：独立进程，sa-token 分布式会话鉴权。

use anyhow::Result;
use cool_proto::space::v1::space_info_service_server::SpaceInfoServiceServer;
use cool_proto::space::v1::space_type_service_server::SpaceTypeServiceServer;
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
    info!(service = %cfg.service.id, addr = %cfg.server.address, "starting space-service");

    let pool = cool_db::connect(&cfg.db_config()).await?;
    if cfg.cool.auto_migrate {
        app_space::migrate(&pool).await?;
    }
    let session = cool_micro::init_distributed(&cfg).await?;

    let (svc_a, svc_b) = app_space::build_services(pool.clone());

    let mut router = Server::builder()
        .layer(cool_core::LoggingLayer)
        .layer(cool_micro::ServiceAuthLayer::new(
            cfg.sa_token.enable,
            session.state.clone(),
            session.dsm.clone(),
        ))
        .add_service(SpaceTypeServiceServer::new(svc_a))
        .add_service(SpaceInfoServiceServer::new(svc_b));

    if cfg.server.reflection {
        let refl = tonic_reflection::server::Builder::configure()
            .register_encoded_file_descriptor_set(cool_proto::REFLECTION_DESCRIPTOR)
            .build_v1()
            .map_err(|e| anyhow::anyhow!("{e}"))?;
        router = router.add_service(refl);
    }

    let addr = cfg.socket_addr()?;
    info!(%addr, "space-service listening");
    router.serve(addr).await?;
    Ok(())
}
