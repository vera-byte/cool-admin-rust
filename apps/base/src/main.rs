//! base 微服务：登录 / 用户 / 角色 / 部门 / 菜单 / 日志 / 文件 / EPS。
//!
//! 独立进程启动，鉴权走 sa-token 分布式会话。

use anyhow::Result;
use cool_proto::base::v1::base_comm_service_server::BaseCommServiceServer;
use cool_proto::base::v1::base_open_service_server::BaseOpenServiceServer;
use cool_proto::base::v1::base_sys_department_service_server::BaseSysDepartmentServiceServer;
use cool_proto::base::v1::base_sys_log_service_server::BaseSysLogServiceServer;
use cool_proto::base::v1::base_sys_menu_service_server::BaseSysMenuServiceServer;
use cool_proto::base::v1::base_sys_role_service_server::BaseSysRoleServiceServer;
use cool_proto::base::v1::base_sys_user_service_server::BaseSysUserServiceServer;
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
    info!(service = %cfg.service.id, addr = %cfg.server.address, "starting base-service");

    // 数据库 + 迁移
    let pool = cool_db::connect(&cfg.db_config()).await?;
    if cfg.cool.auto_migrate {
        app_base::migrate(&pool).await?;
    }

    // sa-token 分布式会话（共享 Redis）
    let session = cool_micro::init_distributed(&cfg).await?;

    // 文件驱动（local 默认）
    let file_cfg = cool_file::FileConfig::default();
    let driver = cool_file::new_driver(&file_cfg).await.ok();

    // 内置函数
    app_base::funcs::register(pool.clone());
    if cfg.redis.enabled() {
        cool_func::set_redis_url(&cfg.redis.url());
    }

    // 业务服务
    let base = app_base::build_services(
        pool.clone(),
        app_base::BaseConfig {
            jwt: cool_core::jwt::JwtConfig::default(),
            authority_enable: cfg.sa_token.enable,
        },
    );

    let mut router = Server::builder()
        .layer(cool_core::LoggingLayer)
        .layer(cool_micro::ServiceAuthLayer::new(
            cfg.sa_token.enable,
            session.state.clone(),
            session.dsm.clone(),
        ))
        .add_service(BaseOpenServiceServer::new(base.open))
        .add_service(BaseSysUserServiceServer::new(base.user))
        .add_service(BaseSysRoleServiceServer::new(base.role))
        .add_service(BaseSysDepartmentServiceServer::new(base.department))
        .add_service(BaseSysMenuServiceServer::new(base.menu))
        .add_service(BaseSysLogServiceServer::new(base.log));

    if let Some(d) = driver {
        router = router.add_service(BaseCommServiceServer::new(app_base::build_comm(d)));
    }

    if cfg.server.reflection {
        let refl = tonic_reflection::server::Builder::configure()
            .register_encoded_file_descriptor_set(cool_proto::REFLECTION_DESCRIPTOR)
            .build_v1()
            .map_err(|e| anyhow::anyhow!("{e}"))?;
        let refl_alpha = tonic_reflection::server::Builder::configure()
            .register_encoded_file_descriptor_set(cool_proto::REFLECTION_DESCRIPTOR)
            .build_v1alpha()
            .map_err(|e| anyhow::anyhow!("{e}"))?;
        router = router.add_service(refl).add_service(refl_alpha);
    }

    // APISIX 路由自动注册（按需：仅引入 cool-apisix 的服务生效）
    if std::env::var("APISIX_ENABLED").map(|v| v != "0").unwrap_or(false) {
        let apisix_cfg = cool_apisix::ApisixConfig::load();
        let svc = cool_apisix::build_static_service(&apisix_cfg, "base-service", "base.v1", 8001);
        if let Err(e) = cool_apisix::register_microservice(&apisix_cfg, &svc).await {
            tracing::warn!("apisix register failed: {e}");
        }
    }

    let addr = cfg.socket_addr()?;
    info!(%addr, "base-service listening");
    router.serve(addr).await?;
    Ok(())
}
