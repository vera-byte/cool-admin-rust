use cool_core::model::{ColumnMeta, Model};

pub struct DictType;

const COLS: &[ColumnMeta] = &[
    ColumnMeta::read("id"),
    ColumnMeta::rw("name"),
    ColumnMeta::rw("key"),
    ColumnMeta::computed("createTime"),
    ColumnMeta::computed("updateTime"),
    ColumnMeta::computed("deleteTime"),
];

impl Model for DictType {
    fn table_name() -> &'static str {
        "dict_type"
    }
    fn columns() -> &'static [ColumnMeta] {
        COLS
    }
}
