//! demo 演示模块（对齐 cool-admin-go modules/demo）

pub mod handler;
pub mod model;
pub mod service;

pub use handler::DemoSampleSvc;
pub use model::DemoSample;
pub use service::{DemoSampleService, new_demo_sample_service};

use sqlx::MySqlPool;

/// 注册/构建模块服务
pub fn build_services(pool: MySqlPool) -> DemoSampleSvc {
    let svc = new_demo_sample_service(pool);
    DemoSampleSvc::new(svc)
}
