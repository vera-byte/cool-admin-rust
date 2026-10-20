//! task 微服务：定时任务调度，独立进程。

use anyhow::Result;
use cool_proto::task::v1::task_info_service_server::TaskInfoServiceServer;
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
    info!(service = %cfg.service.id, addr = %cfg.server.address, "starting task-service");

    let pool = cool_db::connect(&cfg.db_config()).await?;
    if cfg.cool.auto_migrate {
        app_task::migrate(&pool).await?;
    }
    let session = cool_micro::init_distributed(&cfg).await?;

    if cfg.redis.enabled() {
        cool_func::set_redis_url(&cfg.redis.url());
    }

    // 注册内置函数 + 启动任务调度
    let task_svc = app_task::build_services(pool.clone()).await;

    let mut router = Server::builder()
        .layer(cool_micro::ServiceAuthLayer::new(
            cfg.sa_token.enable,
            session.state.clone(),
            session.dsm.clone(),
        ))
        .add_service(TaskInfoServiceServer::new(task_svc));

    if cfg.server.reflection {
        let refl = tonic_reflection::server::Builder::configure()
            .register_encoded_file_descriptor_set(cool_proto::REFLECTION_DESCRIPTOR)
            .build_v1()
            .map_err(|e| anyhow::anyhow!("{e}"))?;
        router = router.add_service(refl);
    }

    let addr = cfg.socket_addr()?;
    info!(%addr, "task-service listening");
    router.serve(addr).await?;
    Ok(())
}
