//! 服务层公共工具

pub mod login;
pub mod user;

pub use login::LoginService;
pub use user::UserService;

/// 排序白名单（复用 cool-core 语义）
pub fn safe_order_clause(order: Option<&str>, sort: Option<&str>) -> Option<String> {
    let order = order.unwrap_or("");
    let sort = sort.unwrap_or("desc");
    cool_core::safe_order_by(order, sort)
}
