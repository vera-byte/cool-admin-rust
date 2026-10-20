//! 通用 CRUD 仓库：基于 SQLx 的 Add/Delete/Update/Info/List/Page。
//!
//! 对齐 cool-admin-go `cool/service.go` 的 Service* 方法语义：
//! - 软删除（deleteTime）
//! - UniqueKey / NotNullKey 校验
//! - 排序白名单
//! - 分页 / 导出上限

use std::collections::HashMap;

use chrono::{DateTime, NaiveDateTime, Utc};
use serde_json::{Value, json};
use sqlx::{Column, MySql, Pool, Row};
use tracing::debug;

use crate::error::{CoolError, CoolResult};
use crate::model::{
    COL_CREATE_TIME, COL_DELETE_TIME, COL_ID, COL_UPDATE_TIME, FieldMap, Model, safe_ident,
};
use crate::query::{EqMap, QueryInput, QueryOp, build_order, build_where};

/// CRUD 配置（对齐 Go Service 结构体）
#[derive(Debug, Clone, Default)]
pub struct CrudConfig {
    /// 唯一键 字段 → 错误提示
    pub unique_key: HashMap<String, String>,
    /// 非空键 字段 → 错误提示
    pub not_null_key: HashMap<String, String>,
    pub list_query_op: QueryOp,
    pub page_query_op: QueryOp,
    /// Info 时忽略的字段
    pub info_ignore_property: Vec<String>,
}

/// 通用 CRUD 仓库
pub struct CrudRepo<M: Model> {
    pool: Pool<MySql>,
    config: CrudConfig,
    _marker: std::marker::PhantomData<M>,
}

impl<M: Model> CrudRepo<M> {
    pub fn new(pool: Pool<MySql>, config: CrudConfig) -> Self {
        Self {
            pool,
            config,
            _marker: std::marker::PhantomData,
        }
    }

    pub fn pool(&self) -> &Pool<MySql> {
        &self.pool
    }

    pub fn config(&self) -> &CrudConfig {
        &self.config
    }

    fn table(&self) -> String {
        format!("`{}`", M::table_name())
    }

    fn allowed(&self) -> impl Fn(&str) -> bool + '_ {
        move |c: &str| M::is_filterable(c) || c == COL_ID
    }

    // ---------- Add ----------

    pub async fn add(&self, data: &FieldMap) -> CoolResult<FieldMap> {
        // NotNull
        for (k, msg) in &self.config.not_null_key {
            let missing = match data.get(k) {
                None => true,
                Some(Value::Null) => true,
                Some(Value::String(s)) if s.is_empty() => true,
                _ => false,
            };
            if missing {
                return Err(CoolError::biz(msg.clone()));
            }
        }

        // Unique
        for (k, msg) in &self.config.unique_key {
            if let Some(v) = data.get(k)
                && !v.is_null()
            {
                let count: i64 = sqlx::query_scalar(&format!(
                    "SELECT COUNT(*) FROM {} WHERE `{k}` = ? AND `{COL_DELETE_TIME}` IS NULL",
                    self.table()
                ))
                .bind(json_to_sql(v)?)
                .fetch_one(&self.pool)
                .await
                .map_err(|e| CoolError::Internal(format!("唯一键校验失败: {e}")))?;
                if count > 0 {
                    return Err(CoolError::biz(msg.clone()));
                }
            }
        }

        // 列白名单
        let mut cols: Vec<String> = Vec::new();
        let mut vals: Vec<Value> = Vec::new();
        for (k, v) in data {
            if k == COL_ID || k == COL_CREATE_TIME || k == COL_UPDATE_TIME || k == COL_DELETE_TIME {
                continue;
            }
            if !safe_ident(k) || !M::is_writable(k) {
                return Err(CoolError::invalid_argument(format!("非法字段: {k}")));
            }
            cols.push(k.clone());
            vals.push(v.clone());
        }

        let mut col_sql: Vec<String> = cols.iter().map(|c| format!("`{c}`")).collect();
        col_sql.push(format!("`{COL_CREATE_TIME}`"));
        col_sql.push(format!("`{COL_UPDATE_TIME}`"));
        let mut placeholders: Vec<String> = vec!["?".into(); cols.len()];
        placeholders.push("NOW(3)".into());
        placeholders.push("NOW(3)".into());

        let sql = format!(
            "INSERT INTO {} ({}) VALUES ({})",
            self.table(),
            col_sql.join(", "),
            placeholders.join(", ")
        );
        debug!(target: "crud", %sql, "add");
        let mut q = sqlx::query(&sql);
        for v in &vals {
            if v.is_null() {
                q = q.bind(Option::<String>::None);
            } else {
                q = q.bind(json_to_sql(v)?);
            }
        }
        let result = q
            .execute(&self.pool)
            .await
            .map_err(|e| CoolError::Internal(format!("新增失败: {e}")))?;

        let id = result.last_insert_id();
        self.info(id as i64)
            .await?
            .ok_or_else(|| CoolError::internal("新增后读取记录失败"))
    }

    // ---------- Delete（软删） ----------

    pub async fn delete(&self, ids: &[i64]) -> CoolResult<u64> {
        if ids.is_empty() {
            return Err(CoolError::invalid_argument("请选择要删除的数据"));
        }
        let placeholders = vec!["?"; ids.len()].join(", ");
        let sql = format!(
            "UPDATE {} SET `{COL_DELETE_TIME}` = NOW(3), `{COL_UPDATE_TIME}` = NOW(3) \
             WHERE `{COL_ID}` IN ({placeholders}) AND `{COL_DELETE_TIME}` IS NULL",
            self.table()
        );
        let mut q = sqlx::query(&sql);
        for id in ids {
            q = q.bind(*id);
        }
        let result = q
            .execute(&self.pool)
            .await
            .map_err(|e| CoolError::Internal(format!("删除失败: {e}")))?;
        Ok(result.rows_affected())
    }

    // ---------- Update ----------

    pub async fn update(&self, id: i64, data: &FieldMap) -> CoolResult<u64> {
        // Unique（排除自身）
        for (k, msg) in &self.config.unique_key {
            if let Some(v) = data.get(k)
                && !v.is_null()
            {
                let count: i64 = sqlx::query_scalar(&format!(
                    "SELECT COUNT(*) FROM {} WHERE `{k}` = ? AND `{COL_ID}` != ? AND `{COL_DELETE_TIME}` IS NULL",
                    self.table()
                ))
                .bind(json_to_sql(v)?)
                .bind(id)
                .fetch_one(&self.pool)
                .await
                .map_err(|e| CoolError::Internal(format!("唯一键校验失败: {e}")))?;
                if count > 0 {
                    return Err(CoolError::biz(msg.clone()));
                }
            }
        }

        let mut sets: Vec<String> = Vec::new();
        let mut vals: Vec<Value> = Vec::new();
        for (k, v) in data {
            if k == COL_ID || k == COL_CREATE_TIME || k == COL_UPDATE_TIME || k == COL_DELETE_TIME {
                continue;
            }
            if !safe_ident(k) || !M::is_writable(k) {
                return Err(CoolError::invalid_argument(format!("非法字段: {k}")));
            }
            sets.push(format!("`{k}` = ?"));
            vals.push(v.clone());
        }
        if sets.is_empty() {
            return Err(CoolError::invalid_argument("没有可更新的字段"));
        }
        sets.push(format!("`{COL_UPDATE_TIME}` = NOW(3)"));

        let sql = format!(
            "UPDATE {} SET {} WHERE `{COL_ID}` = ? AND `{COL_DELETE_TIME}` IS NULL",
            self.table(),
            sets.join(", ")
        );
        let mut q = sqlx::query(&sql);
        for v in &vals {
            q = q.bind(json_to_sql(v)?);
        }
        q = q.bind(id);
        let result = q
            .execute(&self.pool)
            .await
            .map_err(|e| CoolError::Internal(format!("更新失败: {e}")))?;
        Ok(result.rows_affected())
    }

    // ---------- Info ----------

    pub async fn info(&self, id: i64) -> CoolResult<Option<FieldMap>> {
        let sql = format!(
            "SELECT * FROM {} WHERE `{COL_ID}` = ? AND `{COL_DELETE_TIME}` IS NULL LIMIT 1",
            self.table()
        );
        let row = sqlx::query(&sql)
            .bind(id)
            .fetch_optional(&self.pool)
            .await
            .map_err(|e| CoolError::Internal(format!("查询失败: {e}")))?;

        match row {
            Some(r) => {
                let mut map = row_to_map(&r)?;
                for ign in &self.config.info_ignore_property {
                    map.remove(ign);
                }
                Ok(Some(map))
            }
            None => Ok(None),
        }
    }

    // ---------- List ----------

    pub async fn list(&self, input: &QueryInput) -> CoolResult<Vec<FieldMap>> {
        let op = &self.config.list_query_op;
        let (where_sql, args) = build_where(op, input, &self.allowed());
        let mut sql = format!("SELECT * FROM {} WHERE {where_sql}", self.table());
        if let Some(order) = build_order(op, input, &self.allowed()) {
            sql.push_str(" ORDER BY ");
            sql.push_str(&order);
        }
        debug!(target: "crud", %sql, "list");
        let mut q = sqlx::query(&sql);
        for a in args {
            q = q.bind(json_to_sql(&a)?);
        }
        let rows = q
            .fetch_all(&self.pool)
            .await
            .map_err(|e| CoolError::Internal(format!("列表查询失败: {e}")))?;
        rows.iter().map(row_to_map).collect()
    }

    // ---------- Page ----------

    pub async fn page(&self, input: &QueryInput) -> CoolResult<(i64, Vec<FieldMap>)> {
        let op = &self.config.page_query_op;
        let (where_sql, args) = build_where(op, input, &self.allowed());

        let count_sql = format!("SELECT COUNT(*) FROM {} WHERE {where_sql}", self.table());
        let mut cq = sqlx::query_scalar::<_, i64>(&count_sql);
        for a in &args {
            cq = cq.bind(json_to_sql(a)?);
        }
        let total = cq
            .fetch_one(&self.pool)
            .await
            .map_err(|e| CoolError::Internal(format!("分页统计失败: {e}")))?;

        let mut size = if input.size <= 0 { 15 } else { input.size };
        let page = if input.page <= 0 { 1 } else { input.page };

        // 导出上限（对齐 Go PageReq）
        if input.is_export {
            let max = input.max_export_limit;
            if max > 0 && size > max {
                size = max;
            }
            if max <= 0 {
                // 不限制
            }
        }

        let offset = (page - 1) * size;
        let mut sql = format!("SELECT * FROM {} WHERE {where_sql}", self.table());
        if let Some(order) = build_order(op, input, &self.allowed()) {
            sql.push_str(" ORDER BY ");
            sql.push_str(&order);
        }
        sql.push_str(&format!(" LIMIT {size} OFFSET {offset}"));

        let mut q = sqlx::query(&sql);
        for a in args {
            q = q.bind(json_to_sql(&a)?);
        }
        let rows = q
            .fetch_all(&self.pool)
            .await
            .map_err(|e| CoolError::Internal(format!("分页查询失败: {e}")))?;
        let list = rows
            .iter()
            .map(row_to_map)
            .collect::<CoolResult<Vec<_>>>()?;
        Ok((total, list))
    }
}

// ---------- helpers ----------

/// JSON Value → SQL 绑定参数（字符串）
fn json_to_sql(v: &Value) -> CoolResult<String> {
    // 统一以字符串绑定，MySQL 会做隐式转换；Null → 空串由调用方避免
    Ok(match v {
        Value::Null => "NULL".into(), // 不应出现在写入列
        Value::String(s) => s.clone(),
        Value::Number(n) => n.to_string(),
        Value::Bool(b) => if *b { "1" } else { "0" }.into(),
        other => other.to_string(),
    })
}

/// 行 → FieldMap
fn row_to_map(row: &sqlx::mysql::MySqlRow) -> CoolResult<FieldMap> {
    let mut map = FieldMap::new();
    for col in row.columns() {
        let name = col.name();
        if let Ok(Some(v)) = row.try_get::<Option<i64>, _>(name) {
            map.insert(name.to_string(), json!(v));
            continue;
        }
        if let Ok(Some(v)) = row.try_get::<Option<u64>, _>(name) {
            map.insert(name.to_string(), json!(v));
            continue;
        }
        if let Ok(Some(v)) = row.try_get::<Option<f64>, _>(name) {
            map.insert(name.to_string(), json!(v));
            continue;
        }
        if let Ok(Some(v)) = row.try_get::<Option<bool>, _>(name) {
            map.insert(name.to_string(), json!(v));
            continue;
        }
        if let Ok(v) = row.try_get::<Option<String>, _>(name) {
            map.insert(name.to_string(), json!(v));
            continue;
        }
        if let Ok(Some(v)) = row.try_get::<Option<NaiveDateTime>, _>(name) {
            map.insert(name.to_string(), json!(naive_to_millis(v)));
            continue;
        }
        if let Ok(Some(v)) = row.try_get::<Option<DateTime<Utc>>, _>(name) {
            map.insert(name.to_string(), json!(v.timestamp_millis()));
            continue;
        }
        map.insert(name.to_string(), Value::Null);
    }
    Ok(map)
}

fn naive_to_millis(n: chrono::NaiveDateTime) -> i64 {
    n.and_utc().timestamp_millis()
}

/// 分页结果
#[derive(Debug, Clone)]
pub struct PageResult {
    pub total: i64,
    pub list: Vec<FieldMap>,
}

/// EqMap 便捷构造
pub fn eq_map(pairs: &[(&str, Value)]) -> EqMap {
    pairs
        .iter()
        .map(|(k, v)| (k.to_string(), v.clone()))
        .collect()
}
