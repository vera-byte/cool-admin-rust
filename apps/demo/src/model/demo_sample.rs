//! DemoSample 模型（表 demo_sample）

use cool_core::model::{ColumnMeta, Model};

/// 表 demo_sample
pub struct DemoSample;

/// 列元信息（const 便于静态引用）
const DEMO_SAMPLE_COLUMNS: &[ColumnMeta] = &[
    ColumnMeta::read("id"),
    ColumnMeta::rw("name"),
    ColumnMeta::rw("remark"),
    ColumnMeta::computed("createTime"),
    ColumnMeta::computed("updateTime"),
    ColumnMeta::computed("deleteTime"),
];

impl Model for DemoSample {
    fn table_name() -> &'static str {
        "demo_sample"
    }

    fn group_name() -> &'static str {
        "default"
    }

    fn columns() -> &'static [ColumnMeta] {
        DEMO_SAMPLE_COLUMNS
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cool_core::model::Model;

    #[test]
    fn demo_sample_meta() {
        assert_eq!(DemoSample::table_name(), "demo_sample");
        assert!(DemoSample::is_writable("name"));
        assert!(DemoSample::is_writable("remark"));
        assert!(!DemoSample::is_writable("id"));
        assert!(!DemoSample::is_writable("createTime"));
        assert!(!DemoSample::is_writable("deleteTime"));
        assert!(DemoSample::is_filterable("name"));
    }
}
