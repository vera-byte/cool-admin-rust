//! 空间模块（对齐 cool-admin-go modules/space）：纯 CRUD，上传走 BaseComm。

pub mod handler;
pub mod model;
pub mod service;

use sqlx::MySqlPool;

pub use handler::{SpaceInfoSvc, SpaceTypeSvc};

pub fn build_services(pool: MySqlPool) -> (SpaceTypeSvc, SpaceInfoSvc) {
    (SpaceTypeSvc::new(pool.clone()), SpaceInfoSvc::new(pool))
}
