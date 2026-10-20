//! 运行时共享状态

use cool_db::DbPool;

pub struct AppState {
    pub db: Option<DbPool>,
    pub mode: String,
    pub db_required: bool,
}
