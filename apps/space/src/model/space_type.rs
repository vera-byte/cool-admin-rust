use cool_core::model::{ColumnMeta, Model};

pub struct SpaceType;

const COLS: &[ColumnMeta] = &[
    ColumnMeta::read("id"),
    ColumnMeta::rw("name"),
    ColumnMeta::rw("parentId"),
    ColumnMeta::computed("createTime"),
    ColumnMeta::computed("updateTime"),
    ColumnMeta::computed("deleteTime"),
];

impl Model for SpaceType {
    fn table_name() -> &'static str {
        "space_type"
    }
    fn columns() -> &'static [ColumnMeta] {
        COLS
    }
}
