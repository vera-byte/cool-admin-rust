//! 查询条件构造与排序白名单。
//!
//! 对齐 cool-admin-go `cool/service.go` 的 `QueryOp` / `safeOrderBy`。

use std::collections::HashMap;

use serde_json::Value;

use crate::model::{FieldMap, safe_ident};

/// 排序字段白名单：字母/数字/下划线/点，支持逗号分隔多字段（防 ORDER BY 注入）
pub fn safe_order_by(order: &str, sort: &str) -> Option<String> {
    let order = order.replace('`', "").trim().to_string();
    let sort = sort.trim().to_lowercase();
    if order.is_empty() || (sort != "asc" && sort != "desc") {
        return None;
    }
    if !is_safe_order_fields(&order) {
        return None;
    }
    Some(format!("{order} {sort}"))
}

fn is_safe_order_fields(order: &str) -> bool {
    // 对齐 Go 正则 ^[A-Za-z0-9_.]+(,[A-Za-z0-9_.]+)*$（不允许空格）
    !order.is_empty()
        && !order.contains(char::is_whitespace)
        && order.split(',').all(|part| {
            !part.is_empty()
                && part
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '.')
        })
}

/// 等值过滤
pub type EqMap = HashMap<String, Value>;

/// 列表/分页查询入参
#[derive(Debug, Clone, Default)]
pub struct QueryInput {
    /// 模糊搜索关键字（作用于 KeyWordField）
    pub keyword: Option<String>,
    /// 等值过滤
    pub eq: EqMap,
    pub order: Option<String>,
    pub sort: Option<String>,
    pub page: i64,
    pub size: i64,
    pub is_export: bool,
    pub max_export_limit: i64,
}

/// 查询配置（对齐 Go QueryOp）
#[derive(Debug, Clone, Default)]
pub struct QueryOp {
    /// 可等值过滤的字段
    pub field_eq: Vec<String>,
    /// 模糊搜索字段
    pub key_word_field: Vec<String>,
    /// 默认排序 字段 → asc/desc
    pub add_orderby: HashMap<String, String>,
    /// 查询列（None = *）
    pub select: Option<String>,
}

/// 构造 WHERE 子句片段（不含 WHERE 关键字），返回 (sql, args)
/// 过滤条件：deleteTime IS NULL + eq + keyword LIKE
pub fn build_where(
    op: &QueryOp,
    input: &QueryInput,
    allowed: &dyn Fn(&str) -> bool,
) -> (String, Vec<Value>) {
    let mut parts: Vec<String> = vec!["`deleteTime` IS NULL".into()];
    let mut args: Vec<Value> = Vec::new();

    for (k, v) in &input.eq {
        if !safe_ident(k) || !op.field_eq.iter().any(|f| f == k) || !allowed(k) {
            continue;
        }
        parts.push(format!("`{k}` = ?"));
        args.push(v.clone());
    }

    if let Some(kw) = input.keyword.as_deref() {
        let kw = kw.trim();
        if !kw.is_empty() && !op.key_word_field.is_empty() {
            let likes: Vec<String> = op
                .key_word_field
                .iter()
                .filter(|f| safe_ident(f) && allowed(f))
                .map(|f| {
                    args.push(Value::String(format!("%{kw}%")));
                    format!("`{f}` LIKE ?")
                })
                .collect();
            if !likes.is_empty() {
                parts.push(format!("({})", likes.join(" OR ")));
            }
        }
    }

    (parts.join(" AND "), args)
}

/// ORDER BY 片段（含默认排序回退）
pub fn build_order(
    op: &QueryOp,
    input: &QueryInput,
    allowed: &dyn Fn(&str) -> bool,
) -> Option<String> {
    if let (Some(order), Some(sort)) = (input.order.as_deref(), input.sort.as_deref())
        && let Some(ok) = safe_order_by(order, sort)
    {
        // 校验每个字段可过滤
        let fields_ok = ok
            .split_whitespace()
            .next()
            .map(|fields| {
                fields
                    .split(',')
                    .all(|f| safe_ident(f.trim()) && allowed(f.trim()))
            })
            .unwrap_or(false);
        if fields_ok {
            return Some(ok);
        }
    }
    // 默认排序
    let mut pairs: Vec<String> = op
        .add_orderby
        .iter()
        .filter(|(f, _)| safe_ident(f) && allowed(f))
        .map(|(f, s)| {
            let s = if s.eq_ignore_ascii_case("desc") {
                "DESC"
            } else {
                "ASC"
            };
            format!("`{f}` {s}")
        })
        .collect();
    if pairs.is_empty() {
        None
    } else {
        pairs.sort();
        Some(pairs.join(", "))
    }
}

/// FieldMap → JSON 对象（响应体）
pub fn map_to_json(m: &FieldMap) -> Value {
    Value::Object(m.iter().map(|(k, v)| (k.clone(), v.clone())).collect())
}

/// 尝试从 JSON 对象转 FieldMap
pub fn json_to_map(v: &Value) -> Option<FieldMap> {
    v.as_object()
        .map(|o| o.iter().map(|(k, val)| (k.clone(), val.clone())).collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn allow_all(_: &str) -> bool {
        true
    }

    #[test]
    fn safe_order_by_accepts_valid() {
        assert_eq!(safe_order_by("id", "asc").as_deref(), Some("id asc"));
        // sort 与 Go 一致转小写
        assert_eq!(
            safe_order_by("createTime", "DESC").as_deref(),
            Some("createTime desc")
        );
        assert_eq!(safe_order_by("a.b", "desc").as_deref(), Some("a.b desc"));
        assert_eq!(
            safe_order_by("id,name", "asc").as_deref(),
            Some("id,name asc")
        );
    }

    #[test]
    fn safe_order_by_rejects_injection() {
        assert_eq!(safe_order_by("id; drop table user", "asc"), None);
        assert_eq!(safe_order_by("id--", "asc"), None);
        // 反引号会被剥离（对齐 Go），剥离后 id 合法
        assert_eq!(safe_order_by("id`", "asc").as_deref(), Some("id asc"));
        assert_eq!(safe_order_by("id, name", "asc"), None); // Go 正则不允许空格
        assert_eq!(safe_order_by("1=1", "asc"), None);
        assert_eq!(safe_order_by("", "asc"), None);
        assert_eq!(safe_order_by("id", "asc;--"), None);
        assert_eq!(safe_order_by("id", "evil"), None);
    }

    #[test]
    fn safe_order_by_trims_backticks() {
        assert_eq!(safe_order_by("`id`", "asc").as_deref(), Some("id asc"));
    }

    #[test]
    fn build_where_eq_and_keyword() {
        let op = QueryOp {
            field_eq: vec!["name".into(), "remark".into()],
            key_word_field: vec!["name".into()],
            ..Default::default()
        };
        let mut eq = EqMap::new();
        eq.insert("name".into(), json!("abc"));
        let input = QueryInput {
            keyword: Some("k".into()),
            eq,
            ..Default::default()
        };
        let (sql, args) = build_where(&op, &input, &allow_all);
        assert!(sql.contains("`deleteTime` IS NULL"));
        assert!(sql.contains("`name` = ?"));
        assert!(sql.contains("`name` LIKE ?"));
        assert_eq!(args.len(), 2);
    }

    #[test]
    fn build_where_skips_disallowed_eq() {
        let op = QueryOp {
            field_eq: vec!["name".into()],
            ..Default::default()
        };
        let mut eq = EqMap::new();
        eq.insert("evil".into(), json!("x"));
        eq.insert("name".into(), json!("y"));
        let input = QueryInput {
            eq,
            ..Default::default()
        };
        let (sql, args) = build_where(&op, &input, &allow_all);
        assert!(!sql.contains("evil"));
        assert_eq!(args.len(), 1);
    }

    #[test]
    fn build_order_default_and_override() {
        let mut add_orderby = HashMap::new();
        add_orderby.insert("id".into(), "desc".into());
        let op = QueryOp {
            add_orderby,
            ..Default::default()
        };
        let input = QueryInput::default();
        assert_eq!(
            build_order(&op, &input, &allow_all).as_deref(),
            Some("`id` DESC")
        );

        let input = QueryInput {
            order: Some("name".into()),
            sort: Some("asc".into()),
            ..Default::default()
        };
        assert_eq!(
            build_order(&op, &input, &allow_all).as_deref(),
            Some("name asc")
        );
    }

    #[test]
    fn build_order_rejects_bad_and_falls_back() {
        let mut add_orderby = HashMap::new();
        add_orderby.insert("id".into(), "asc".into());
        let op = QueryOp {
            add_orderby,
            ..Default::default()
        };
        let input = QueryInput {
            order: Some("id;drop".into()),
            sort: Some("asc".into()),
            ..Default::default()
        };
        assert_eq!(
            build_order(&op, &input, &allow_all).as_deref(),
            Some("`id` ASC")
        );
    }
}
