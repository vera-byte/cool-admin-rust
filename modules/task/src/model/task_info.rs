use cool_core::model::{ColumnMeta, Model};

pub struct TaskInfo;

const COLS: &[ColumnMeta] = &[
    ColumnMeta::read("id"),
    ColumnMeta::computed("jobId"),
    ColumnMeta::rw("repeatConf"),
    ColumnMeta::rw("name"),
    ColumnMeta::rw("cron"),
    ColumnMeta::rw("limit"),
    ColumnMeta::rw("every"),
    ColumnMeta::rw("remark"),
    ColumnMeta::rw("status"),
    ColumnMeta::rw("startDate"),
    ColumnMeta::rw("endDate"),
    ColumnMeta::rw("data"),
    ColumnMeta::rw("service"),
    ColumnMeta::rw("type"),
    ColumnMeta::computed("nextRunTime"),
    ColumnMeta::rw("taskType"),
    ColumnMeta::computed("createTime"),
    ColumnMeta::computed("updateTime"),
    ColumnMeta::computed("deleteTime"),
];

impl Model for TaskInfo {
    fn table_name() -> &'static str {
        "task_info"
    }
    fn columns() -> &'static [ColumnMeta] {
        COLS
    }
}
