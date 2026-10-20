//! space services

use std::collections::HashMap;

use cool_core::CrudService;
use cool_core::query::QueryOp;
use cool_core::repo::{CrudConfig, CrudRepo};
use sqlx::MySqlPool;

use crate::model::{SpaceInfo, SpaceType};

pub struct SpaceTypeService {
    repo: CrudRepo<SpaceType>,
}

pub fn new_space_type_service(pool: MySqlPool) -> SpaceTypeService {
    let config = CrudConfig {
        not_null_key: {
            let mut m = HashMap::new();
            m.insert("name".into(), "名称不能为空".into());
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
            ..Default::default()
        },
        ..Default::default()
    };
    SpaceTypeService {
        repo: CrudRepo::<SpaceType>::new(pool, config),
    }
}

#[async_trait::async_trait]
impl CrudService for SpaceTypeService {
    type Model = SpaceType;
    fn repo(&self) -> &CrudRepo<Self::Model> {
        &self.repo
    }
}

pub struct SpaceInfoService {
    repo: CrudRepo<SpaceInfo>,
}

pub fn new_space_info_service(pool: MySqlPool) -> SpaceInfoService {
    let config = CrudConfig {
        not_null_key: {
            let mut m = HashMap::new();
            m.insert("url".into(), "地址不能为空".into());
            m
        },
        list_query_op: QueryOp {
            field_eq: vec!["type".into(), "classifyId".into()],
            add_orderby: {
                let mut m = HashMap::new();
                m.insert("id".into(), "desc".into());
                m
            },
            ..Default::default()
        },
        page_query_op: QueryOp {
            field_eq: vec!["type".into(), "classifyId".into()],
            add_orderby: {
                let mut m = HashMap::new();
                m.insert("id".into(), "desc".into());
                m
            },
            ..Default::default()
        },
        ..Default::default()
    };
    SpaceInfoService {
        repo: CrudRepo::<SpaceInfo>::new(pool, config),
    }
}

#[async_trait::async_trait]
impl CrudService for SpaceInfoService {
    type Model = SpaceInfo;
    fn repo(&self) -> &CrudRepo<Self::Model> {
        &self.repo
    }
}
