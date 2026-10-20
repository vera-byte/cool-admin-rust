//! base 权限体系模块（对齐 cool-admin-go modules/base）

pub mod funcs;
pub mod handler;
pub mod service;

pub use handler::{
    BaseCommSvc, BaseOpenSvc, BaseSysDepartmentSvc, BaseSysLogSvc, BaseSysMenuSvc, BaseSysRoleSvc,
    BaseSysUserSvc,
};

use cool_core::jwt::JwtConfig;
use sqlx::MySqlPool;

/// base 模块配置
#[derive(Debug, Clone)]
pub struct BaseConfig {
    pub jwt: JwtConfig,
    /// 是否启用鉴权中间件
    pub authority_enable: bool,
}

impl Default for BaseConfig {
    fn default() -> Self {
        Self {
            jwt: JwtConfig::default(),
            authority_enable: true,
        }
    }
}

/// base 服务集合
pub struct BaseServices {
    pub open: BaseOpenSvc,
    pub user: BaseSysUserSvc,
    pub role: BaseSysRoleSvc,
    pub department: BaseSysDepartmentSvc,
    pub menu: BaseSysMenuSvc,
    pub log: BaseSysLogSvc,
}

/// 构建 base 模块全部服务（含内置函数注册）
pub fn build_services(pool: MySqlPool, cfg: BaseConfig) -> BaseServices {
    funcs::register(pool.clone());
    BaseServices {
        open: handler::open::BaseOpenSvc::new(pool.clone(), cfg.clone()),
        user: handler::user::BaseSysUserSvc::new(pool.clone()),
        role: handler::role::BaseSysRoleSvc::new(pool.clone()),
        department: handler::department::BaseSysDepartmentSvc::new(pool.clone()),
        menu: handler::menu::BaseSysMenuSvc::new(pool.clone()),
        log: handler::log::BaseSysLogSvc::new(pool.clone()),
    }
}

/// 构建 BaseComm（文件上传）服务
pub fn build_comm(driver: std::sync::Arc<dyn cool_file::FileDriver>) -> BaseCommSvc {
    BaseCommSvc::new(driver)
}
