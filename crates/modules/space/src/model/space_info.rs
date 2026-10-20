use cool_core::model::{ColumnMeta, Model};

pub struct SpaceInfo;

const COLS: &[ColumnMeta] = &[
    ColumnMeta::read("id"),
    ColumnMeta::rw("url"),
    ColumnMeta::rw("type"),
    ColumnMeta::rw("classifyId"),
    ColumnMeta::computed("createTime"),
    ColumnMeta::computed("updateTime"),
    ColumnMeta::computed("deleteTime"),
];

impl Model for SpaceInfo {
    fn table_name() -> &'static str {
        "space_info"
    }
    fn columns() -> &'static [ColumnMeta] {
        COLS
    }
}
