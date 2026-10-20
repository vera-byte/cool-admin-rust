use cool_core::model::{ColumnMeta, Model};

pub struct DictInfo;

const COLS: &[ColumnMeta] = &[
    ColumnMeta::read("id"),
    ColumnMeta::rw("typeId"),
    ColumnMeta::rw("name"),
    ColumnMeta::rw("orderNum"),
    ColumnMeta::rw("remark"),
    ColumnMeta::rw("parentId"),
    ColumnMeta::computed("createTime"),
    ColumnMeta::computed("updateTime"),
    ColumnMeta::computed("deleteTime"),
];

impl Model for DictInfo {
    fn table_name() -> &'static str {
        "dict_info"
    }
    fn columns() -> &'static [ColumnMeta] {
        COLS
    }
}
