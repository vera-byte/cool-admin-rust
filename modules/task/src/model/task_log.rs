use cool_core::model::{ColumnMeta, Model};

pub struct TaskLog;

const COLS: &[ColumnMeta] = &[
    ColumnMeta::read("id"),
    ColumnMeta::rw("taskId"),
    ColumnMeta::rw("status"),
    ColumnMeta::rw("detail"),
    ColumnMeta::computed("createTime"),
    ColumnMeta::computed("updateTime"),
    ColumnMeta::computed("deleteTime"),
];

impl Model for TaskLog {
    fn table_name() -> &'static str {
        "task_log"
    }
    fn columns() -> &'static [ColumnMeta] {
        COLS
    }
}
