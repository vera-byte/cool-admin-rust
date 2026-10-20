//! DemoSample 服务层：配置 UniqueKey / QueryOp，覆写钩子示例。

use std::collections::HashMap;

use cool_core::CrudService;
use cool_core::model::Model;
use cool_core::query::QueryOp;
use cool_core::repo::{CrudConfig, CrudRepo};
use cool_core::{CoolResult, FieldMap};
use sqlx::MySqlPool;
use tracing::info;

use crate::model::DemoSample;

/// DemoSample CRUD 服务
pub struct DemoSampleService {
    repo: CrudRepo<DemoSample>,
}

pub fn new_demo_sample_service(pool: MySqlPool) -> DemoSampleService {
    let mut unique_key = HashMap::new();
    unique_key.insert("name".to_string(), "名称已存在".to_string());

    let list_query_op = QueryOp {
        field_eq: vec!["name".into()],
        key_word_field: vec!["name".into(), "remark".into()],
        add_orderby: {
            let mut m = HashMap::new();
            m.insert("id".to_string(), "desc".to_string());
            m
        },
        select: None,
    };
    let page_query_op = list_query_op.clone();

    let config = CrudConfig {
        unique_key,
        not_null_key: {
            let mut m = HashMap::new();
            m.insert("name".to_string(), "名称不能为空".to_string());
            m
        },
        list_query_op,
        page_query_op,
        info_ignore_property: vec![],
    };

    DemoSampleService {
        repo: CrudRepo::<DemoSample>::new(pool, config),
    }
}

impl DemoSampleService {
    pub fn table_name() -> &'static str {
        <DemoSample as Model>::table_name()
    }
}

#[async_trait::async_trait]
impl CrudService for DemoSampleService {
    type Model = DemoSample;

    fn repo(&self) -> &CrudRepo<Self::Model> {
        &self.repo
    }

    async fn modify_before(&self, method: &str, data: &FieldMap) -> CoolResult<()> {
        info!(target: "demo", %method, keys = ?data.keys().collect::<Vec<_>>(), "modify_before");
        Ok(())
    }

    async fn modify_after(&self, method: &str, _data: &FieldMap) -> CoolResult<()> {
        info!(target: "demo", %method, "modify_after");
        Ok(())
    }
}
