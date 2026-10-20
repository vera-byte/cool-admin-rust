//! BaseSysRoleService：角色管理（含菜单/部门关联）

use cool_core::error::CoolError;
use cool_core::ok_code;
use cool_proto::base::v1::base_sys_role_service_server::BaseSysRoleService;
use cool_proto::base::v1::{
    RoleAddReq, RoleAddRes, RoleDeleteReq, RoleDeleteRes, RoleInfoReq, RoleInfoRes, RoleListReq,
    RoleListRes, RolePageReq, RolePageRes, RoleUpdateReq, RoleUpdateRes, SysRole,
};
use sqlx::{MySqlPool, Row};
use tonic::{Request, Response, Status};

use super::biz_err;

pub struct BaseSysRoleSvc {
    pool: MySqlPool,
}

impl BaseSysRoleSvc {
    pub fn new(pool: MySqlPool) -> Self {
        Self { pool }
    }

    async fn menu_ids(&self, role_id: i64) -> Vec<i64> {
        sqlx::query("SELECT menuId FROM base_sys_role_menu WHERE roleId = ? AND deleteTime IS NULL")
            .bind(role_id)
            .fetch_all(&self.pool)
            .await
            .unwrap_or_default()
            .iter()
            .filter_map(|r| r.try_get::<i64, _>("menuId").ok())
            .collect()
    }

    async fn department_ids(&self, role_id: i64) -> Vec<i64> {
        sqlx::query(
            "SELECT departmentId FROM base_sys_role_department WHERE roleId = ? AND deleteTime IS NULL",
        )
        .bind(role_id)
        .fetch_all(&self.pool)
        .await
        .unwrap_or_default()
        .iter()
        .filter_map(|r| r.try_get::<i64, _>("departmentId").ok())
        .collect()
    }

    async fn replace_menu(&self, role_id: i64, ids: &[i64]) -> Result<(), CoolError> {
        sqlx::query(
            "UPDATE base_sys_role_menu SET deleteTime = NOW(3) WHERE roleId = ? AND deleteTime IS NULL",
        )
        .bind(role_id)
        .execute(&self.pool)
        .await
        .map_err(|e| CoolError::Internal(format!("清空菜单关联失败: {e}")))?;
        for id in ids {
            sqlx::query(
                "INSERT INTO base_sys_role_menu (roleId, menuId, createTime, updateTime) VALUES (?, ?, NOW(3), NOW(3))",
            )
            .bind(role_id)
            .bind(*id)
            .execute(&self.pool)
            .await
            .map_err(|e| CoolError::Internal(format!("写入菜单关联失败: {e}")))?;
        }
        Ok(())
    }

    async fn replace_department(&self, role_id: i64, ids: &[i64]) -> Result<(), CoolError> {
        sqlx::query(
            "UPDATE base_sys_role_department SET deleteTime = NOW(3) WHERE roleId = ? AND deleteTime IS NULL",
        )
        .bind(role_id)
        .execute(&self.pool)
        .await
        .map_err(|e| CoolError::Internal(format!("清空部门关联失败: {e}")))?;
        for id in ids {
            sqlx::query(
                "INSERT INTO base_sys_role_department (roleId, departmentId, createTime, updateTime) VALUES (?, ?, NOW(3), NOW(3))",
            )
            .bind(role_id)
            .bind(*id)
            .execute(&self.pool)
            .await
            .map_err(|e| CoolError::Internal(format!("写入部门关联失败: {e}")))?;
        }
        Ok(())
    }

    async fn load_role(&self, id: i64) -> Result<Option<SysRole>, CoolError> {
        let row = sqlx::query(
            "SELECT id, name, label, remark FROM base_sys_role WHERE id = ? AND deleteTime IS NULL",
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| CoolError::Internal(format!("查询失败: {e}")))?;

        match row {
            Some(r) => Ok(Some(SysRole {
                id,
                name: r.try_get("name").unwrap_or_default(),
                label: r.try_get("label").unwrap_or_default(),
                remark: r.try_get("remark").unwrap_or_default(),
                create_time: 0,
                update_time: 0,
                menu_ids: self.menu_ids(id).await,
                department_ids: self.department_ids(id).await,
            })),
            None => Ok(None),
        }
    }

    async fn list_roles(&self, keyword: &str, limit: i64) -> Result<Vec<SysRole>, CoolError> {
        let kw = keyword.trim();
        let sql = if kw.is_empty() {
            "SELECT id, name, label, remark FROM base_sys_role WHERE deleteTime IS NULL ORDER BY id DESC LIMIT ?"
                .to_string()
        } else {
            "SELECT id, name, label, remark FROM base_sys_role \
             WHERE deleteTime IS NULL AND (name LIKE ? OR label LIKE ?) \
             ORDER BY id DESC LIMIT ?"
                .to_string()
        };

        let mut q = sqlx::query(&sql);
        if !kw.is_empty() {
            let like = format!("%{kw}%");
            q = q.bind(like.clone()).bind(like);
        }
        q = q.bind(limit);
        let rows = q
            .fetch_all(&self.pool)
            .await
            .map_err(|e| CoolError::Internal(format!("查询失败: {e}")))?;

        let mut list = Vec::new();
        for r in rows {
            let id: i64 = r.try_get("id").unwrap_or_default();
            list.push(SysRole {
                id,
                name: r.try_get("name").unwrap_or_default(),
                label: r.try_get("label").unwrap_or_default(),
                remark: r.try_get("remark").unwrap_or_default(),
                create_time: 0,
                update_time: 0,
                menu_ids: self.menu_ids(id).await,
                department_ids: self.department_ids(id).await,
            });
        }
        Ok(list)
    }
}

#[tonic::async_trait]
impl BaseSysRoleService for BaseSysRoleSvc {
    async fn page(&self, req: Request<RolePageReq>) -> Result<Response<RolePageRes>, Status> {
        let r = req.into_inner();
        let page = if r.page <= 0 { 1 } else { r.page as i64 };
        let size = if r.size <= 0 { 15 } else { r.size as i64 };
        let offset = (page - 1) * size;

        let kw = r.keyword.trim();
        let (where_sql, has_kw) = if kw.is_empty() {
            ("deleteTime IS NULL".to_string(), false)
        } else {
            (
                "deleteTime IS NULL AND (name LIKE ? OR label LIKE ?)".to_string(),
                true,
            )
        };

        let count_sql = format!("SELECT COUNT(*) FROM base_sys_role WHERE {where_sql}");
        let mut cq = sqlx::query_scalar::<_, i64>(&count_sql);
        if has_kw {
            let like = format!("%{kw}%");
            cq = cq.bind(like.clone()).bind(like);
        }
        let total = cq
            .fetch_one(&self.pool)
            .await
            .map_err(|e| CoolError::Internal(format!("统计失败: {e}")))
            .map_err(biz_err)?;

        let order = crate::service::safe_order_clause(Some(&r.order), Some(&r.sort))
            .unwrap_or_else(|| "id DESC".into());
        let sql = format!(
            "SELECT id, name, label, remark FROM base_sys_role WHERE {where_sql} ORDER BY {order} LIMIT {size} OFFSET {offset}"
        );
        let mut q = sqlx::query(&sql);
        if has_kw {
            let like = format!("%{kw}%");
            q = q.bind(like.clone()).bind(like);
        }
        let rows = q
            .fetch_all(&self.pool)
            .await
            .map_err(|e| CoolError::Internal(format!("查询失败: {e}")))
            .map_err(biz_err)?;

        let mut list = Vec::new();
        for r in rows {
            let id: i64 = r.try_get("id").unwrap_or_default();
            list.push(SysRole {
                id,
                name: r.try_get("name").unwrap_or_default(),
                label: r.try_get("label").unwrap_or_default(),
                remark: r.try_get("remark").unwrap_or_default(),
                create_time: 0,
                update_time: 0,
                menu_ids: self.menu_ids(id).await,
                department_ids: self.department_ids(id).await,
            });
        }

        let (code, message) = ok_code();
        Ok(Response::new(RolePageRes {
            code,
            message,
            total,
            list,
        }))
    }

    async fn list(&self, req: Request<RoleListReq>) -> Result<Response<RoleListRes>, Status> {
        let r = req.into_inner();
        let list = self.list_roles(&r.keyword, 200).await.map_err(biz_err)?;
        let (code, message) = ok_code();
        Ok(Response::new(RoleListRes {
            code,
            message,
            list,
        }))
    }

    async fn info(&self, req: Request<RoleInfoReq>) -> Result<Response<RoleInfoRes>, Status> {
        let id = req.into_inner().id;
        let data = self.load_role(id).await.map_err(biz_err)?;
        let (code, message) = ok_code();
        Ok(Response::new(RoleInfoRes {
            code,
            message,
            data,
        }))
    }

    async fn add(&self, req: Request<RoleAddReq>) -> Result<Response<RoleAddRes>, Status> {
        let r = req.into_inner();
        if r.name.trim().is_empty() {
            return Err(Status::invalid_argument("角色名不能为空"));
        }
        let exists: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM base_sys_role WHERE name = ? AND deleteTime IS NULL",
        )
        .bind(&r.name)
        .fetch_one(&self.pool)
        .await
        .unwrap_or(0);
        if exists > 0 {
            return Err(Status::invalid_argument("角色名已存在"));
        }
        let res = sqlx::query(
            "INSERT INTO base_sys_role (name, label, remark, createTime, updateTime) VALUES (?, ?, ?, NOW(3), NOW(3))",
        )
        .bind(&r.name)
        .bind(&r.label)
        .bind(&r.remark)
        .execute(&self.pool)
        .await
        .map_err(|e| CoolError::Internal(format!("新增失败: {e}")))
        .map_err(biz_err)?;
        let id = res.last_insert_id() as i64;

        self.replace_menu(id, &r.menu_ids).await.map_err(biz_err)?;
        self.replace_department(id, &r.department_ids)
            .await
            .map_err(biz_err)?;

        let data = self.load_role(id).await.map_err(biz_err)?;
        let (code, message) = ok_code();
        Ok(Response::new(RoleAddRes {
            code,
            message,
            data,
        }))
    }

    async fn update(&self, req: Request<RoleUpdateReq>) -> Result<Response<RoleUpdateRes>, Status> {
        let r = req.into_inner();
        let mut sets: Vec<String> = Vec::new();
        let mut args: Vec<String> = Vec::new();
        if let Some(v) = &r.name {
            sets.push("name = ?".into());
            args.push(v.clone());
        }
        if let Some(v) = &r.label {
            sets.push("label = ?".into());
            args.push(v.clone());
        }
        if let Some(v) = &r.remark {
            sets.push("remark = ?".into());
            args.push(v.clone());
        }
        let mut affected = 0i64;
        if !sets.is_empty() {
            sets.push("updateTime = NOW(3)".into());
            let sql = format!(
                "UPDATE base_sys_role SET {} WHERE id = ? AND deleteTime IS NULL",
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
            affected = res.rows_affected() as i64;
        }
        if !r.menu_ids.is_empty() {
            self.replace_menu(r.id, &r.menu_ids)
                .await
                .map_err(biz_err)?;
        }
        if !r.department_ids.is_empty() {
            self.replace_department(r.id, &r.department_ids)
                .await
                .map_err(biz_err)?;
        }
        let (code, message) = ok_code();
        Ok(Response::new(RoleUpdateRes {
            code,
            message,
            affected,
        }))
    }

    async fn delete(&self, req: Request<RoleDeleteReq>) -> Result<Response<RoleDeleteRes>, Status> {
        let ids = req.into_inner().ids;
        if ids.is_empty() {
            return Err(Status::invalid_argument("请选择要删除的数据"));
        }
        let placeholders = vec!["?"; ids.len()].join(", ");
        let sql = format!(
            "UPDATE base_sys_role SET deleteTime = NOW(3), updateTime = NOW(3) \
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
        Ok(Response::new(RoleDeleteRes {
            code,
            message,
            affected: res.rows_affected() as i64,
        }))
    }
}
