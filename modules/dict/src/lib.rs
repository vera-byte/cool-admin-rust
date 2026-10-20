//! 字典模块（对齐 cool-admin-go modules/dict）

pub mod handler;
pub mod model;
pub mod service;

use sqlx::MySqlPool;

pub use handler::{DictInfoSvc, DictTypeSvc};

pub fn build_services(pool: MySqlPool) -> (DictTypeSvc, DictInfoSvc) {
    (DictTypeSvc::new(pool.clone()), DictInfoSvc::new(pool))
}
