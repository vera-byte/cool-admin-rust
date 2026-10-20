//! BaseSysDepartmentService：部门管理（树形）

use cool_core::error::CoolError;
use cool_core::ok_code;
use cool_proto::base::v1::base_sys_department_service_server::BaseSysDepartmentService;
use cool_proto::base::v1::{
    DeptAddReq, DeptAddRes, DeptDeleteReq, DeptDeleteRes, DeptInfoReq, DeptInfoRes, DeptListReq,
    DeptListRes, DeptTreeReq, DeptTreeRes, DeptUpdateReq, DeptUpdateRes, SysDepartment,
};
use sqlx::{MySqlPool, Row};
use tonic::{Request, Response, Status};

use super::biz_err;

pub struct BaseSysDepartmentSvc {
    pool: MySqlPool,
}

impl BaseSysDepartmentSvc {
    pub fn new(pool: MySqlPool) -> Self {
        Self { pool }
    }

    async fn flat_list(&self, keyword: &str) -> Result<Vec<SysDepartment>, CoolError> {
        let kw = keyword.trim();
        let sql = if kw.is_empty() {
            "SELECT id, name, parentId, sort FROM base_sys_department WHERE deleteTime IS NULL ORDER BY sort ASC, id ASC LIMIT 500".to_string()
        } else {
            "SELECT id, name, parentId, sort FROM base_sys_department \
             WHERE deleteTime IS NULL AND name LIKE ? ORDER BY sort ASC, id ASC LIMIT 500"
                .to_string()
        };
        let mut q = sqlx::query(&sql);
        if !kw.is_empty() {
            q = q.bind(format!("%{kw}%"));
        }
        let rows = q
            .fetch_all(&self.pool)
            .await
            .map_err(|e| CoolError::Internal(format!("查询失败: {e}")))?;
        Ok(rows
            .iter()
            .map(|r| SysDepartment {
                id: r.try_get("id").unwrap_or_default(),
                name: r.try_get("name").unwrap_or_default(),
                parent_id: r.try_get("parentId").unwrap_or_default(),
                sort: r.try_get("sort").unwrap_or_default(),
                create_time: 0,
                update_time: 0,
                children: vec![],
            })
            .collect())
    }
}

fn build_tree(flat: Vec<SysDepartment>) -> Vec<SysDepartment> {
    let mut by_parent: std::collections::HashMap<i64, Vec<SysDepartment>> =
        std::collections::HashMap::new();
    for d in flat {
        by_parent.entry(d.parent_id).or_default().push(d);
    }
    fn take(
        parent: i64,
        map: &mut std::collections::HashMap<i64, Vec<SysDepartment>>,
    ) -> Vec<SysDepartment> {
        let mut children = map.remove(&parent).unwrap_or_default();
        for c in children.iter_mut() {
            c.children = take(c.id, map);
        }
        children.sort_by(|a, b| a.sort.cmp(&b.sort).then(a.id.cmp(&b.id)));
        children
    }
    take(0, &mut by_parent)
}

#[tonic::async_trait]
impl BaseSysDepartmentService for BaseSysDepartmentSvc {
    async fn list(&self, req: Request<DeptListReq>) -> Result<Response<DeptListRes>, Status> {
        let r = req.into_inner();
        let list = self.flat_list(&r.keyword).await.map_err(biz_err)?;
        let (code, message) = ok_code();
        Ok(Response::new(DeptListRes {
            code,
            message,
            list,
        }))
    }

    async fn tree(&self, req: Request<DeptTreeReq>) -> Result<Response<DeptTreeRes>, Status> {
        let r = req.into_inner();
        let flat = self.flat_list(&r.keyword).await.map_err(biz_err)?;
        let list = build_tree(flat);
        let (code, message) = ok_code();
        Ok(Response::new(DeptTreeRes {
            code,
            message,
            list,
        }))
    }

    async fn info(&self, req: Request<DeptInfoReq>) -> Result<Response<DeptInfoRes>, Status> {
        let id = req.into_inner().id;
        let row = sqlx::query(
            "SELECT id, name, parentId, sort FROM base_sys_department WHERE id = ? AND deleteTime IS NULL",
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| CoolError::Internal(format!("查询失败: {e}")))
        .map_err(biz_err)?;
        let data = row.map(|r| SysDepartment {
            id: r.try_get("id").unwrap_or_default(),
            name: r.try_get("name").unwrap_or_default(),
            parent_id: r.try_get("parentId").unwrap_or_default(),
            sort: r.try_get("sort").unwrap_or_default(),
            create_time: 0,
            update_time: 0,
            children: vec![],
        });
        let (code, message) = ok_code();
        Ok(Response::new(DeptInfoRes {
            code,
            message,
            data,
        }))
    }

    async fn add(&self, req: Request<DeptAddReq>) -> Result<Response<DeptAddRes>, Status> {
        let r = req.into_inner();
        if r.name.trim().is_empty() {
            return Err(Status::invalid_argument("名称不能为空"));
        }
        let res = sqlx::query(
            "INSERT INTO base_sys_department (name, parentId, sort, createTime, updateTime) VALUES (?, ?, ?, NOW(3), NOW(3))",
        )
        .bind(&r.name)
        .bind(r.parent_id)
        .bind(r.sort)
        .execute(&self.pool)
        .await
        .map_err(|e| CoolError::Internal(format!("新增失败: {e}")))
        .map_err(biz_err)?;
        let id = res.last_insert_id() as i64;
        let (code, message) = ok_code();
        Ok(Response::new(DeptAddRes {
            code,
            message,
            data: Some(SysDepartment {
                id,
                name: r.name,
                parent_id: r.parent_id,
                sort: r.sort,
                create_time: 0,
                update_time: 0,
                children: vec![],
            }),
        }))
    }

    async fn update(&self, req: Request<DeptUpdateReq>) -> Result<Response<DeptUpdateRes>, Status> {
        let r = req.into_inner();
        let mut sets: Vec<String> = Vec::new();
        let mut args: Vec<String> = Vec::new();
        if let Some(v) = &r.name {
            sets.push("name = ?".into());
            args.push(v.clone());
        }
        if let Some(v) = r.parent_id {
            sets.push("parentId = ?".into());
            args.push(v.to_string());
        }
        if let Some(v) = r.sort {
            sets.push("sort = ?".into());
            args.push(v.to_string());
        }
        if sets.is_empty() {
            return Err(Status::invalid_argument("没有可更新的字段"));
        }
        sets.push("updateTime = NOW(3)".into());
        let sql = format!(
            "UPDATE base_sys_department SET {} WHERE id = ? AND deleteTime IS NULL",
            sets.join(", ")
        );
        let mut q = sqlx::query(&sql);
        for a in &args {
            q = q.bind(a);
        }
        q = q.bind(r.id);
        let res = q
            .execute(&self.pool)
            .await
            .map_err(|e| CoolError::Internal(format!("更新失败: {e}")))
            .map_err(biz_err)?;
        let (code, message) = ok_code();
        Ok(Response::new(DeptUpdateRes {
            code,
            message,
            affected: res.rows_affected() as i64,
        }))
    }

    async fn delete(&self, req: Request<DeptDeleteReq>) -> Result<Response<DeptDeleteRes>, Status> {
        let ids = req.into_inner().ids;
        if ids.is_empty() {
            return Err(Status::invalid_argument("请选择要删除的数据"));
        }
        let placeholders = vec!["?"; ids.len()].join(", ");
        let sql = format!(
            "UPDATE base_sys_department SET deleteTime = NOW(3), updateTime = NOW(3) \
             WHERE id IN ({placeholders}) AND deleteTime IS NULL"
        );
        let mut q = sqlx::query(&sql);
        for id in &ids {
            q = q.bind(*id);
        }
        let res = q
            .execute(&self.pool)
            .await
            .map_err(|e| CoolError::Internal(format!("删除失败: {e}")))
            .map_err(biz_err)?;
        let (code, message) = ok_code();
        Ok(Response::new(DeptDeleteRes {
            code,
            message,
            affected: res.rows_affected() as i64,
        }))
    }
}
