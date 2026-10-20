//! cool-admin-rust gRPC 服务入口。
//!
//! 启动流程：配置 → tracing → DB/migrate → cache → 文件驱动 → 函数注册 →
//! Redis 监听 → 任务引导 → 注册服务 → serve

mod config;
mod eps;
mod services;
mod state;

use std::sync::Arc;

use anyhow::{Result, bail};
use tonic::transport::Server;
use tracing::{error, info};

use cool_core::{LoggingLayer, init_tracing};
use cool_proto::REFLECTION_DESCRIPTOR;
use cool_proto::base::v1::base_comm_service_server::BaseCommServiceServer;
use cool_proto::base::v1::base_open_service_server::BaseOpenServiceServer;
use cool_proto::base::v1::base_sys_department_service_server::BaseSysDepartmentServiceServer;
use cool_proto::base::v1::base_sys_log_service_server::BaseSysLogServiceServer;
use cool_proto::base::v1::base_sys_menu_service_server::BaseSysMenuServiceServer;
use cool_proto::base::v1::base_sys_role_service_server::BaseSysRoleServiceServer;
use cool_proto::base::v1::base_sys_user_service_server::BaseSysUserServiceServer;
use cool_proto::demo::v1::demo_sample_service_server::DemoSampleServiceServer;
use cool_proto::dict::v1::dict_info_service_server::DictInfoServiceServer;
use cool_proto::dict::v1::dict_type_service_server::DictTypeServiceServer;
use cool_proto::internal::v1::internal_service_server::InternalServiceServer;
use cool_proto::space::v1::space_info_service_server::SpaceInfoServiceServer;
use cool_proto::space::v1::space_type_service_server::SpaceTypeServiceServer;
use cool_proto::task::v1::task_info_service_server::TaskInfoServiceServer;

use config::AppConfig;
use services::internal::InternalSvc;
use state::AppState;

#[tokio::main]
async fn main() -> Result<()> {
    let cfg = match AppConfig::load(None) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("配置加载失败: {e:#}");
            std::process::exit(1);
        }
    };

    init_tracing(&cfg.server.log_level);
    eps::register_all();

    // sa-token 初始化（会话/权限）
    let sa_cfg = cool_core::satoken::SaTokenConfig {
        timeout: cfg.modules.base.jwt.expire() as i64,
        token_name: "satoken".into(),
        enable: cfg.authority_enabled(),
    };
    let sa_state = cool_core::satoken::init(&sa_cfg);

    info!(mode = %cfg.cool.mode, addr = %cfg.server.address, "starting cool-admin-rust");
    info!(instance = cool_core::instance_id(), "instance id");

    // 数据库：失败可降级
    let db = cool_db::try_init(&cfg.db_config(), cfg.cool.auto_migrate).await;
    if db.is_none() && cfg.cool.db_required {
        error!("database required but unavailable");
        bail!("database required but unavailable");
    }

    // 缓存：Redis 或内存
    let redis_url = if cfg.redis.cool.enabled() {
        let mut url = if cfg.redis.cool.address.contains("://") {
            cfg.redis.cool.address.clone()
        } else {
            format!("redis://{}", cfg.redis.cool.address)
        };
        if let Some(pw) = cfg.redis.cool.password.as_deref()
            && !pw.is_empty()
        {
            // redis://:pass@host:port
            let rest = url.trim_start_matches("redis://");
            url = format!("redis://:{pw}@{rest}");
        }
        if cfg.redis.cool.db != 0 {
            url = format!("{url}/{}", cfg.redis.cool.db);
        }
        Some(url.clone())
    } else {
        None
    };
    cool_core::cache::init_store(redis_url.as_deref()).await;
    if let Some(u) = &redis_url {
        cool_func::set_redis_url(u);
    }

    // 文件驱动
    let file_cfg = cool_file::FileConfig {
        mode: cfg.cool.file.mode.clone(),
        domain: cfg.cool.file.domain.clone(),
        oss: cool_file::ObjectConfig {
            endpoint: cfg.cool.file.oss.endpoint.clone(),
            access_key_id: cfg.cool.file.oss.access_key_id.clone(),
            secret_access_key: cfg.cool.file.oss.secret_access_key.clone(),
            bucket_name: cfg.cool.file.oss.bucket_name.clone(),
            use_ssl: cfg.cool.file.oss.use_ssl,
            location: cfg.cool.file.oss.location.clone(),
        },
        root: "public".into(),
    };
    let driver = match cool_file::new_driver(&file_cfg).await {
        Ok(d) => Some(d),
        Err(e) => {
            error!("file driver init failed: {e}");
            None
        }
    };

    // JWT 弱密钥检查
    let jwt_cfg = cfg.jwt_config();
    if let Err(e) = cool_core::jwt::validate_secret(&jwt_cfg.secret) {
        error!("JWT secret 不安全: {e}");
    }

    let state = Arc::new(AppState {
        db: db.clone(),
        mode: cfg.cool.mode.clone(),
        db_required: cfg.cool.db_required,
    });

    // Redis pub/sub 监听（分布式函数）
    if redis_url.is_some() {
        info!("spawn cool:func listener");
        tokio::spawn(async { cool_func::listen_func().await });
    }

    let addr = cfg.socket_addr()?;

    // ---- 服务装配（Router 链式 add_service，可选服务跳过）----
    let internal = InternalServiceServer::new(InternalSvc {
        state: state.clone(),
    });

    // tower 层：后添加的更靠外。请求流：Logging → SaTokenGrpcLayer → handler
    let sa_layer = sa_token_plugin_tonic::SaTokenGrpcLayer::with_path_auth(
        sa_state,
        cool_core::satoken::default_path_config(),
    );
    let mut router = Server::builder()
        .layer(sa_layer)
        .layer(LoggingLayer)
        .add_service(internal);

    if let Some(pool) = db {
        info!("register demo/base/dict/space/task services");

        // demo
        router = router.add_service(
            DemoSampleServiceServer::new(mod_demo::build_services(pool.clone()))
                .max_decoding_message_size(cfg.server.max_decoding_message_size)
                .max_encoding_message_size(cfg.server.max_decoding_message_size),
        );

        // base
        let base = mod_base::build_services(
            pool.clone(),
            mod_base::BaseConfig {
                jwt: jwt_cfg.clone(),
                authority_enable: cfg.authority_enabled(),
            },
        );
        router = router
            .add_service(BaseOpenServiceServer::new(base.open))
            .add_service(BaseSysUserServiceServer::new(base.user))
            .add_service(BaseSysRoleServiceServer::new(base.role))
            .add_service(BaseSysDepartmentServiceServer::new(base.department))
            .add_service(BaseSysMenuServiceServer::new(base.menu))
            .add_service(BaseSysLogServiceServer::new(base.log));

        // dict
        let (dict_type, dict_info) = mod_dict::build_services(pool.clone());
        router = router
            .add_service(DictTypeServiceServer::new(dict_type))
            .add_service(DictInfoServiceServer::new(dict_info));

        // space
        let (space_type, space_info) = mod_space::build_services(pool.clone());
        router = router
            .add_service(SpaceTypeServiceServer::new(space_type))
            .add_service(SpaceInfoServiceServer::new(space_info));

        // task（含内置函数注册 + 引导）
        let task_svc = mod_task::build_services(pool.clone()).await;
        router = router.add_service(TaskInfoServiceServer::new(task_svc));
    }

    // base comm（文件，不依赖 DB）
    if let Some(d) = driver {
        info!("register base.v1.BaseCommService");
        router = router.add_service(BaseCommServiceServer::new(mod_base::build_comm(d)));
    }

    if cfg.server.reflection {
        info!("gRPC reflection enabled");
        let reflection = tonic_reflection::server::Builder::configure()
            .register_encoded_file_descriptor_set(REFLECTION_DESCRIPTOR)
            .build_v1()
            .map_err(|e| anyhow::anyhow!("reflection v1: {e}"))?;
        let reflection_v1alpha = tonic_reflection::server::Builder::configure()
            .register_encoded_file_descriptor_set(REFLECTION_DESCRIPTOR)
            .build_v1alpha()
            .map_err(|e| anyhow::anyhow!("reflection v1alpha: {e}"))?;

        info!(%addr, "gRPC server listening");
        router
            .add_service(reflection)
            .add_service(reflection_v1alpha)
            .serve_with_shutdown(addr, shutdown_signal())
            .await?;
    } else {
        info!(%addr, "gRPC server listening (no reflection)");
        router.serve_with_shutdown(addr, shutdown_signal()).await?;
    }

    info!("server stopped");
    Ok(())
}

async fn shutdown_signal() {
    let ctrl_c = async {
        tokio::signal::ctrl_c()
            .await
            .expect("failed to install Ctrl+C handler");
    };

    #[cfg(unix)]
    let terminate = async {
        tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .expect("failed to install signal handler")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {},
        _ = terminate => {},
    }
    info!("shutdown signal received");
}
