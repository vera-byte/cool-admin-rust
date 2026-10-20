//! CRUD 服务层：钩子 + 默认实现（对齐 Go `cool/service.go` 的 IService）。
//!
//! 决策 D2：trait 默认实现为主；业务通过覆写钩子/配置扩展。

use async_trait::async_trait;

use crate::error::CoolResult;
use crate::model::FieldMap;
use crate::model::Model;
use crate::query::QueryInput;
use crate::repo::{CrudConfig, CrudRepo, PageResult};

/// CRUD 服务接口
#[async_trait]
pub trait CrudService: Send + Sync {
    type Model: Model;

    /// 仓库
    fn repo(&self) -> &CrudRepo<Self::Model>;

    /// 配置（一般在构造时传入 repo）
    fn config(&self) -> &CrudConfig {
        self.repo().config()
    }

    /// Add/Update/Delete 前钩子
    async fn modify_before(&self, _method: &str, _data: &FieldMap) -> CoolResult<()> {
        Ok(())
    }

    /// Add/Update/Delete 后钩子
    async fn modify_after(&self, _method: &str, _data: &FieldMap) -> CoolResult<()> {
        Ok(())
    }

    /// 新增
    async fn service_add(&self, mut data: FieldMap) -> CoolResult<FieldMap> {
        self.modify_before("Add", &data).await?;
        let row = self.repo().add(&data).await?;
        data.extend(row.clone());
        self.modify_after("Add", &data).await?;
        Ok(row)
    }

    /// 软删除
    async fn service_delete(&self, ids: &[i64]) -> CoolResult<u64> {
        let data: FieldMap = [("ids".into(), serde_json::json!(ids))]
            .into_iter()
            .collect();
        self.modify_before("Delete", &data).await?;
        let n = self.repo().delete(ids).await?;
        self.modify_after("Delete", &data).await?;
        Ok(n)
    }

    /// 更新
    async fn service_update(&self, id: i64, data: FieldMap) -> CoolResult<u64> {
        self.modify_before("Update", &data).await?;
        let n = self.repo().update(id, &data).await?;
        self.modify_after("Update", &data).await?;
        Ok(n)
    }

    /// 详情
    async fn service_info(&self, id: i64) -> CoolResult<Option<FieldMap>> {
        self.repo().info(id).await
    }

    /// 列表
    async fn service_list(&self, input: QueryInput) -> CoolResult<Vec<FieldMap>> {
        self.repo().list(&input).await
    }

    /// 分页
    async fn service_page(&self, input: QueryInput) -> CoolResult<PageResult> {
        let (total, list) = self.repo().page(&input).await?;
        Ok(PageResult { total, list })
    }
}
