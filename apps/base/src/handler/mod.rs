//! gRPC handler 层

pub mod comm;
pub mod department;
pub mod log;
pub mod menu;
pub mod open;
pub mod role;
pub mod user;

pub use comm::BaseCommSvc;
pub use department::BaseSysDepartmentSvc;
pub use log::BaseSysLogSvc;
pub use menu::BaseSysMenuSvc;
pub use open::BaseOpenSvc;
pub use role::BaseSysRoleSvc;
pub use user::BaseSysUserSvc;

use cool_core::CoolError;
use tonic::Status;

/// 业务错误 → tonic Status
pub fn biz_err(e: CoolError) -> Status {
    match e {
        CoolError::Biz(_) => Status::invalid_argument(e.to_string()),
        CoolError::InvalidArgument(_) => Status::invalid_argument(e.to_string()),
        CoolError::Unauthenticated(_) => Status::unauthenticated(e.to_string()),
        CoolError::PermissionDenied(_) => Status::permission_denied(e.to_string()),
        CoolError::NotFound(_) => Status::not_found(e.to_string()),
        other => Status::internal(other.to_string()),
    }
}
