//! 字典服务：DictType / DictInfo（含 Data 聚合 + 递归删除）

use std::collections::HashMap;

use cool_core::model::FieldMap;
use cool_core::query::{QueryInput, QueryOp};
use cool_core::repo::{CrudConfig, CrudRepo};
use cool_core::{CoolError, CoolResult, CrudService};
use serde_json::json;
use sqlx::{MySqlPool, Row};

use crate::model::{DictInfo, DictType};

// ---------------- DictType ----------------

pub struct DictTypeService {
    repo: CrudRepo<DictType>,
}

pub fn new_dict_type_service(pool: MySqlPool) -> DictTypeService {
    let config = CrudConfig {
        unique_key: {
            let mut m = HashMap::new();
            m.insert("key".into(), "字典类型标识已存在".into());
            m
        },
        not_null_key: {
            let mut m = HashMap::new();
            m.insert("name".into(), "名称不能为空".into());
            m.insert("key".into(), "标识不能为空".into());
            m
        },
        list_query_op: QueryOp {
            key_word_field: vec!["name".into()],
            add_orderby: {
                let mut m = HashMap::new();
                m.insert("id".into(), "asc".into());
                m
            },
            ..Default::default()
        },
        page_query_op: QueryOp {
            key_word_field: vec!["name".into()],
            add_orderby: {
                let mut m = HashMap::new();
                m.insert("id".into(), "asc".into());
                m
            },
            ..Default::default()
        },
        ..Default::default()
    };
    DictTypeService {
        repo: CrudRepo::<DictType>::new(pool, config),
    }
}

#[async_trait::async_trait]
impl CrudService for DictTypeService {
    type Model = DictType;
    fn repo(&self) -> &CrudRepo<Self::Model> {
        &self.repo
    }
}

// ---------------- DictInfo ----------------

pub struct DictInfoService {
    repo: CrudRepo<DictInfo>,
}

pub fn new_dict_info_service(pool: MySqlPool) -> DictInfoService {
    let config = CrudConfig {
        not_null_key: {
            let mut m = HashMap::new();
            m.insert("name".into(), "名称不能为空".into());
            m
        },
        list_query_op: QueryOp {
            field_eq: vec!["typeId".into()],
            key_word_field: vec!["name".into()],
            add_orderby: {
                let mut m = HashMap::new();
                m.insert("createTime".into(), "asc".into());
                m
            },
            ..Default::default()
        },
        page_query_op: QueryOp {
            field_eq: vec!["typeId".into()],
            key_word_field: vec!["name".into()],
            add_orderby: {
                let mut m = HashMap::new();
                m.insert("createTime".into(), "asc".into());
                m
            },
            ..Default::default()
        },
        ..Default::default()
    };
    DictInfoService {
        repo: CrudRepo::<DictInfo>::new(pool, config),
    }
}

impl DictInfoService {
    /// 按类型 key 聚合（对齐 Go Data(types)）
    /// 返回 map<typeKey, Vec<{id,name,parentId,typeId}>>
    /// Go 有过滤列名 bug（用了不存在的 `type`），此处按 `key` 过滤。
    pub async fn data(&self, types: &[String]) -> CoolResult<HashMap<String, Vec<FieldMap>>> {
        // 查类型
        let type_rows = if types.is_empty() {
            sqlx::query(
                "SELECT id, name, `key` FROM dict_type WHERE deleteTime IS NULL ORDER BY id ASC",
            )
            .fetch_all(self.repo.pool())
            .await
            .map_err(|e| CoolError::Internal(format!("查询字典类型失败: {e}")))?
        } else {
            let ph = vec!["?"; types.len()].join(", ");
            let sql = format!(
                "SELECT id, name, `key` FROM dict_type WHERE deleteTime IS NULL AND `key` IN ({ph}) ORDER BY id ASC"
            );
            let mut q = sqlx::query(&sql);
            for t in types {
                q = q.bind(t);
            }
            q.fetch_all(self.repo.pool())
                .await
                .map_err(|e| CoolError::Internal(format!("查询字典类型失败: {e}")))?
        };

        let mut out: HashMap<String, Vec<FieldMap>> = HashMap::new();
        for tr in type_rows {
            let tid: i64 = tr.try_get::<u64, _>("id").unwrap_or_default() as i64;
            let key: String = tr.try_get("key").unwrap_or_default();
            let items = sqlx::query(
                "SELECT id, name, parentId, typeId FROM dict_info \
                 WHERE typeId = ? AND deleteTime IS NULL ORDER BY orderNum ASC, id ASC",
            )
            .bind(tid)
            .fetch_all(self.repo.pool())
            .await
            .map_err(|e| CoolError::Internal(format!("查询字典条目失败: {e}")))?;

            let list = items
                .iter()
                .map(|r| {
                    let mut m = FieldMap::new();
                    m.insert(
                        "id".into(),
                        json!(r.try_get::<u64, _>("id").unwrap_or_default() as i64),
                    );
                    m.insert(
                        "name".into(),
                        json!(r.try_get::<String, _>("name").unwrap_or_default()),
                    );
                    m.insert(
                        "parentId".into(),
                        json!(
                            r.try_get::<Option<u64>, _>("parentId")
                                .unwrap_or(None)
                                .map(|v| v as i64)
                                .unwrap_or(0)
                        ),
                    );
                    m.insert("typeId".into(), json!(tid));
                    m
                })
                .collect::<Vec<_>>();
            out.insert(key, list);
        }
        Ok(out)
    }

    /// 删除后递归软删子条目（对齐 Go delChildDict）
    pub async fn delete_cascade(&self, ids: &[i64]) -> CoolResult<u64> {
        // 先收集全部子孙 id
        let mut all: Vec<i64> = ids.to_vec();
        let mut frontier = ids.to_vec();
        while !frontier.is_empty() {
            let ph = vec!["?"; frontier.len()].join(", ");
            let sql =
                format!("SELECT id FROM dict_info WHERE parentId IN ({ph}) AND deleteTime IS NULL");
            let mut q = sqlx::query(&sql);
            for id in &frontier {
                q = q.bind(*id);
            }
            let rows = q
                .fetch_all(self.repo.pool())
                .await
                .map_err(|e| CoolError::Internal(format!("查询子条目失败: {e}")))?;
            frontier = rows
                .iter()
                .filter_map(|r| r.try_get::<u64, _>("id").ok())
                .map(|v| v as i64)
                .filter(|id| !all.contains(id))
                .collect();
            all.extend(frontier.clone());
        }
        // 一次性软删（含子孙）
        self.repo.delete(&all).await
    }
}

#[async_trait::async_trait]
impl CrudService for DictInfoService {
    type Model = DictInfo;
    fn repo(&self) -> &CrudRepo<Self::Model> {
        &self.repo
    }
}

/// 供 handler 使用的 QueryInput 构造
pub fn info_query(keyword: &str, type_id: Option<i64>) -> QueryInput {
    let mut eq = HashMap::new();
    if let Some(t) = type_id {
        eq.insert("typeId".into(), json!(t));
    }
    QueryInput {
        keyword: if keyword.is_empty() {
            None
        } else {
            Some(keyword.into())
        },
        eq,
        page: 1,
        size: 0,
        ..Default::default()
    }
}
