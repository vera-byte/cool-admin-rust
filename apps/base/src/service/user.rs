//! 用户管理服务（含安全项：密码不回传、username 唯一）

use cool_core::error::{CoolError, CoolResult};
use cool_core::password;
use sqlx::{MySql, Pool, Row};
use tracing::info;

use super::login::UserRow;

pub struct UserService {
    pool: Pool<MySql>,
}

#[derive(Debug, Clone, Default)]
pub struct UserFilter {
    pub keyword: Option<String>,
    pub department_id: Option<i64>,
    pub status: Option<i32>,
    pub page: i64,
    pub size: i64,
    pub order: Option<String>,
    pub sort: Option<String>,
    /// 数据范围：允许的部门 ID 列表（None=不限制）
    pub scope_departments: Option<Vec<i64>>,
}

impl UserService {
    pub fn new(pool: Pool<MySql>) -> Self {
        Self { pool }
    }

    pub fn pool(&self) -> &Pool<MySql> {
        &self.pool
    }

    /// 分页（列表不返回 password）
    pub async fn page(&self, f: &UserFilter) -> CoolResult<(i64, Vec<UserRow>)> {
        let mut wheres = vec!["deleteTime IS NULL".to_string()];
        let mut args: Vec<String> = Vec::new();

        if let Some(kw) = f.keyword.as_deref()
            && !kw.is_empty()
        {
            wheres.push("(name LIKE ? OR username LIKE ? OR nickName LIKE ?)".into());
            let like = format!("%{kw}%");
            args.push(like.clone());
            args.push(like.clone());
            args.push(like);
        }
        if let Some(d) = f.department_id {
            wheres.push("departmentId = ?".into());
            args.push(d.to_string());
        }
        if let Some(s) = f.status {
            wheres.push("status = ?".into());
            args.push(s.to_string());
        }
        // 部门数据范围（对齐 Go：普通管理员无法越权查看其他部门用户）
        if let Some(depts) = &f.scope_departments {
            if depts.is_empty() {
                wheres.push("1 = 0".into()); // 无可见部门 → 空集
            } else {
                let ph = vec!["?"; depts.len()].join(", ");
                wheres.push(format!("departmentId IN ({ph})"));
                for d in depts {
                    args.push(d.to_string());
                }
            }
        }

        let where_sql = wheres.join(" AND ");

        let count_sql = format!("SELECT COUNT(*) FROM base_sys_user WHERE {where_sql}");
        let mut cq = sqlx::query_scalar::<_, i64>(&count_sql);
        for a in &args {
            cq = cq.bind(a);
        }
        let total = cq
            .fetch_one(&self.pool)
            .await
            .map_err(|e| CoolError::Internal(format!("统计失败: {e}")))?;

        let page = f.page.max(1);
        let size = if f.size <= 0 { 15 } else { f.size.min(100) };
        let offset = (page - 1) * size;

        // 排序白名单
        let order = crate::service::safe_order_clause(f.order.as_deref(), f.sort.as_deref())
            .unwrap_or_else(|| "id DESC".into());

        let list_sql = format!(
            "SELECT id, departmentId as departmentid, name, username, '' as password, \
             passwordV as passwordv, nickName as nickname, headImg as headimg, phone, email, \
             status, remark \
             FROM base_sys_user WHERE {where_sql} ORDER BY {order} LIMIT {size} OFFSET {offset}"
        );
        let mut q = sqlx::query(&list_sql);
        for a in &args {
            q = q.bind(a);
        }
        let rows = q
            .fetch_all(&self.pool)
            .await
            .map_err(|e| CoolError::Internal(format!("查询失败: {e}")))?;

        let list = rows
            .iter()
            .map(super::login::map_user_pub)
            .collect::<CoolResult<Vec<_>>>()?;
        Ok((total, list))
    }

    pub async fn info(&self, id: i64) -> CoolResult<Option<UserRow>> {
        let row = sqlx::query(
            "SELECT id, departmentId as departmentid, name, username, '' as password, \
             passwordV as passwordv, nickName as nickname, headImg as headimg, phone, email, \
             status, remark \
             FROM base_sys_user WHERE id = ? AND deleteTime IS NULL LIMIT 1",
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| CoolError::Internal(format!("查询失败: {e}")))?;

        match row {
            Some(r) => Ok(Some(super::login::map_user_pub(&r)?)),
            None => Ok(None),
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn add(
        &self,
        department_id: i64,
        name: &str,
        username: &str,
        password_plain: &str,
        nick_name: &str,
        phone: &str,
        email: &str,
        status: i32,
        remark: &str,
        role_ids: &[i64],
    ) -> CoolResult<i64> {
        if username.trim().is_empty() {
            return Err(CoolError::biz("用户名不能为空"));
        }
        if password_plain.len() < 6 {
            return Err(CoolError::biz("密码长度不能少于6位"));
        }
        let exists: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM base_sys_user WHERE username = ? AND deleteTime IS NULL",
        )
        .bind(username)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| CoolError::Internal(format!("校验失败: {e}")))?;
        if exists > 0 {
            return Err(CoolError::biz("用户名已存在"));
        }

        let hash = password::hash_password(password_plain)?;
        let r = sqlx::query(
            "INSERT INTO base_sys_user \
             (departmentId, name, username, password, passwordV, nickName, phone, email, status, remark, createTime, updateTime) \
             VALUES (?, ?, ?, ?, 1, ?, ?, ?, ?, ?, NOW(3), NOW(3))",
        )
        .bind(department_id)
        .bind(name)
        .bind(username)
        .bind(hash)
        .bind(nick_name)
        .bind(phone)
        .bind(email)
        .bind(status)
        .bind(remark)
        .execute(&self.pool)
        .await
        .map_err(|e| CoolError::Internal(format!("新增失败: {e}")))?;

        let id = r.last_insert_id() as i64;
        self.set_roles(id, role_ids).await?;
        Ok(id)
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn update(
        &self,
        id: i64,
        department_id: Option<i64>,
        name: Option<&str>,
        password_plain: Option<&str>,
        nick_name: Option<&str>,
        phone: Option<&str>,
        email: Option<&str>,
        status: Option<i32>,
        remark: Option<&str>,
        role_ids: Option<&[i64]>,
    ) -> CoolResult<u64> {
        let mut sets: Vec<String> = Vec::new();
        let mut args: Vec<String> = Vec::new();

        if let Some(v) = department_id {
            sets.push("departmentId = ?".into());
            args.push(v.to_string());
        }
        if let Some(v) = name {
            sets.push("name = ?".into());
            args.push(v.into());
        }
        if let Some(p) = password_plain {
            if p.len() < 6 {
                return Err(CoolError::biz("密码长度不能少于6位"));
            }
            let hash = password::hash_password(p)?;
            sets.push("password = ?".into());
            args.push(hash);
            // 改密使旧 token 失效
            sets.push("passwordV = passwordV + 1".into());
        }
        if let Some(v) = nick_name {
            sets.push("nickName = ?".into());
            args.push(v.into());
        }
        if let Some(v) = phone {
            sets.push("phone = ?".into());
            args.push(v.into());
        }
        if let Some(v) = email {
            sets.push("email = ?".into());
            args.push(v.into());
        }
        if let Some(v) = status {
            sets.push("status = ?".into());
            args.push(v.to_string());
        }
        if let Some(v) = remark {
            sets.push("remark = ?".into());
            args.push(v.into());
        }

        if sets.is_empty() && role_ids.is_none() {
            return Err(CoolError::invalid_argument("没有可更新的字段"));
        }

        let mut affected = 0u64;
        if !sets.is_empty() {
            sets.push("updateTime = NOW(3)".into());
            let sql = format!(
                "UPDATE base_sys_user SET {} WHERE id = ? AND deleteTime IS NULL",
                sets.join(", ")
            );
            let mut q = sqlx::query(&sql);
            for a in &args {
                q = q.bind(a);
            }
            q = q.bind(id);
            let r = q
                .execute(&self.pool)
                .await
                .map_err(|e| CoolError::Internal(format!("更新失败: {e}")))?;
            affected = r.rows_affected();
        }

        if let Some(roles) = role_ids {
            self.set_roles(id, roles).await?;
        }
        Ok(affected)
    }

    pub async fn delete(&self, ids: &[i64]) -> CoolResult<u64> {
        if ids.is_empty() {
            return Err(CoolError::invalid_argument("请选择要删除的数据"));
        }
        let placeholders = vec!["?"; ids.len()].join(", ");
        let sql = format!(
            "UPDATE base_sys_user SET deleteTime = NOW(3), updateTime = NOW(3) \
             WHERE id IN ({placeholders}) AND deleteTime IS NULL"
        );
        let mut q = sqlx::query(&sql);
        for id in ids {
            q = q.bind(*id);
        }
        let r = q
            .execute(&self.pool)
            .await
            .map_err(|e| CoolError::Internal(format!("删除失败: {e}")))?;
        Ok(r.rows_affected())
    }

    /// 设置用户角色关联
    pub async fn set_roles(&self, user_id: i64, role_ids: &[i64]) -> CoolResult<()> {
        sqlx::query("UPDATE base_sys_user_role SET deleteTime = NOW(3) WHERE userId = ? AND deleteTime IS NULL")
            .bind(user_id)
            .execute(&self.pool)
            .await
            .map_err(|e| CoolError::Internal(format!("清空角色失败: {e}")))?;

        for rid in role_ids {
            sqlx::query(
                "INSERT INTO base_sys_user_role (userId, roleId, createTime, updateTime) \
                 VALUES (?, ?, NOW(3), NOW(3))",
            )
            .bind(user_id)
            .bind(rid)
            .execute(&self.pool)
            .await
            .map_err(|e| CoolError::Internal(format!("设置角色失败: {e}")))?;
        }
        info!(user_id, roles = ?role_ids, "user roles updated");
        Ok(())
    }

    pub async fn role_ids(&self, user_id: i64) -> CoolResult<Vec<i64>> {
        let rows = sqlx::query(
            "SELECT roleId FROM base_sys_user_role WHERE userId = ? AND deleteTime IS NULL",
        )
        .bind(user_id)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| CoolError::Internal(format!("查询角色失败: {e}")))?;
        Ok(rows
            .iter()
            .filter_map(|r| r.try_get::<i64, _>("roleId").ok())
            .collect())
    }

    /// 查询用户可见的部门 ID 列表（角色关联部门 + 自身部门）
    /// 管理员（userId=1 或角色含 1）返回 None 表示不限制
    pub async fn scope_departments(&self, user_id: i64, role_ids: &[i64]) -> Option<Vec<i64>> {
        if user_id == 1 || role_ids.contains(&1) {
            return None; // 超管
        }
        let mut depts: Vec<i64> = Vec::new();
        // 自身部门
        if let Ok(Some(d)) = sqlx::query_scalar::<_, u64>(
            "SELECT departmentId FROM base_sys_user WHERE id = ? AND deleteTime IS NULL",
        )
        .bind(user_id)
        .fetch_optional(&self.pool)
        .await
        {
            depts.push(d as i64);
        }
        // 角色关联部门
        if !role_ids.is_empty() {
            let ph = vec!["?"; role_ids.len()].join(", ");
            let sql = format!(
                "SELECT DISTINCT departmentId FROM base_sys_role_department                  WHERE roleId IN ({ph}) AND deleteTime IS NULL"
            );
            let mut q = sqlx::query(&sql);
            for r in role_ids {
                q = q.bind(*r);
            }
            if let Ok(rows) = q.fetch_all(&self.pool).await {
                for r in rows {
                    if let Ok(d) = r.try_get::<u64, _>("departmentId") {
                        depts.push(d as i64);
                    }
                }
            }
        }
        depts.sort_unstable();
        depts.dedup();
        Some(depts)
    }
}
