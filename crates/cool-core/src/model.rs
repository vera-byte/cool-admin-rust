//! 模型元信息与动态字段表。
//!
//! 对齐 cool-admin-go `cool/model.go` + 通用 CRUD 的 map 入参/出参。

use std::collections::HashMap;

use serde_json::Value;

/// 动态字段集合（列名 → 值）
pub type FieldMap = HashMap<String, Value>;

/// 列元信息（白名单校验用，防 SQL 注入）
#[derive(Debug, Clone)]
pub struct ColumnMeta {
    pub name: &'static str,
    /// 是否允许出现在 Add/Update 写入列中
    pub writable: bool,
    /// 是否允许出现在查询过滤/排序中
    pub filterable: bool,
}

impl ColumnMeta {
    pub const fn rw(name: &'static str) -> Self {
        Self {
            name,
            writable: true,
            filterable: true,
        }
    }

    pub const fn read(name: &'static str) -> Self {
        Self {
            name,
            writable: false,
            filterable: true,
        }
    }

    pub const fn computed(name: &'static str) -> Self {
        Self {
            name,
            writable: false,
            filterable: false,
        }
    }
}

/// 模型元信息（表名 + 列定义）
pub trait Model: Send + Sync {
    /// 表名
    fn table_name() -> &'static str;
    /// 数据源分组，默认 default
    fn group_name() -> &'static str {
        "default"
    }
    /// 列元信息
    fn columns() -> &'static [ColumnMeta];

    fn writable_columns() -> Vec<&'static str> {
        Self::columns()
            .iter()
            .filter(|c| c.writable)
            .map(|c| c.name)
            .collect()
    }

    fn filterable_columns() -> Vec<&'static str> {
        Self::columns()
            .iter()
            .filter(|c| c.filterable)
            .map(|c| c.name)
            .collect()
    }

    fn is_writable(col: &str) -> bool {
        Self::columns().iter().any(|c| c.name == col && c.writable)
    }

    fn is_filterable(col: &str) -> bool {
        Self::columns()
            .iter()
            .any(|c| c.name == col && c.filterable)
    }
}

/// 标识符安全：仅允许字母数字下划线（表名/列名）
pub fn safe_ident(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 64
        && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
}

/// 反引号包裹已校验的标识符
pub fn quote_ident(name: &str) -> String {
    debug_assert!(safe_ident(name), "unsafe identifier: {name}");
    format!("`{name}`")
}

/// 公共时间列名（对齐 cool-admin-go）
pub const COL_ID: &str = "id";
pub const COL_CREATE_TIME: &str = "createTime";
pub const COL_UPDATE_TIME: &str = "updateTime";
pub const COL_DELETE_TIME: &str = "deleteTime";

/// 基础列（所有模型隐含拥有）
pub fn base_columns() -> [ColumnMeta; 4] {
    [
        ColumnMeta::read(COL_ID),
        ColumnMeta::computed(COL_CREATE_TIME),
        ColumnMeta::computed(COL_UPDATE_TIME),
        ColumnMeta::computed(COL_DELETE_TIME),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn safe_ident_rules() {
        assert!(safe_ident("name"));
        assert!(safe_ident("createTime"));
        assert!(safe_ident("a1_b2"));
        assert!(!safe_ident(""));
        assert!(!safe_ident("a b"));
        assert!(!safe_ident("a;drop"));
        assert!(!safe_ident("a`b"));
        assert!(!safe_ident("a-b"));
    }

    #[test]
    fn writable_filterable() {
        struct M;
        const COLS: &[ColumnMeta] = &[
            ColumnMeta::rw("name"),
            ColumnMeta::read("id"),
            ColumnMeta::computed("createTime"),
        ];
        impl Model for M {
            fn table_name() -> &'static str {
                "t"
            }
            fn columns() -> &'static [ColumnMeta] {
                COLS
            }
        }
        assert!(M::is_writable("name"));
        assert!(!M::is_writable("id"));
        assert!(!M::is_writable("createTime"));
        assert!(M::is_filterable("id"));
        assert!(!M::is_filterable("createTime"));
        assert_eq!(M::writable_columns(), vec!["name"]);
    }
}
