//! BaseSysMenuService：菜单权限（树形 + Perms）

use cool_core::error::CoolError;
use cool_core::ok_code;
use cool_proto::base::v1::base_sys_menu_service_server::BaseSysMenuService;
use cool_proto::base::v1::{
    MenuAddReq, MenuAddRes, MenuDeleteReq, MenuDeleteRes, MenuInfoReq, MenuInfoRes, MenuListReq,
    MenuListRes, MenuPermsData, MenuPermsReq, MenuPermsRes, MenuTreeReq, MenuTreeRes,
    MenuUpdateReq, MenuUpdateRes, SysMenu,
};
use sqlx::{MySqlPool, Row};
use tonic::{Request, Response, Status};

use super::biz_err;

pub struct BaseSysMenuSvc {
    pool: MySqlPool,
}

impl BaseSysMenuSvc {
    pub fn new(pool: MySqlPool) -> Self {
        Self { pool }
    }

    async fn flat_list(&self, keyword: &str) -> Result<Vec<SysMenu>, CoolError> {
        let kw = keyword.trim();
        let sql = if kw.is_empty() {
            "SELECT * FROM base_sys_menu WHERE deleteTime IS NULL ORDER BY orderNum ASC, id ASC LIMIT 1000".to_string()
        } else {
            "SELECT * FROM base_sys_menu WHERE deleteTime IS NULL AND name LIKE ? \
             ORDER BY orderNum ASC, id ASC LIMIT 1000"
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
        Ok(rows.iter().map(row_to_menu).collect())
    }

    /// 超管（roleIds 含 1）返回全部菜单；否则按角色关联过滤
    async fn perms_for_user(&self, user_id: i64) -> Result<(Vec<SysMenu>, Vec<String>), CoolError> {
        let role_rows = sqlx::query(
            "SELECT roleId FROM base_sys_user_role WHERE userId = ? AND deleteTime IS NULL",
        )
        .bind(user_id)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| CoolError::Internal(format!("查询角色失败: {e}")))?;
        let role_ids: Vec<i64> = role_rows
            .iter()
            .filter_map(|r| r.try_get::<i64, _>("roleId").ok())
            .collect();

        let is_admin = role_ids.contains(&1);
        let all = self.flat_list("").await?;

        let menus = if is_admin {
            all
        } else if role_ids.is_empty() {
            vec![]
        } else {
            let placeholders = vec!["?"; role_ids.len()].join(", ");
            let sql = format!(
                "SELECT DISTINCT menuId FROM base_sys_role_menu \
                 WHERE roleId IN ({placeholders}) AND deleteTime IS NULL"
            );
            let mut q = sqlx::query(&sql);
            for id in &role_ids {
                q = q.bind(*id);
            }
            let rows = q
                .fetch_all(&self.pool)
                .await
                .map_err(|e| CoolError::Internal(format!("查询菜单失败: {e}")))?;
            let allowed: std::collections::HashSet<i64> = rows
                .iter()
                .filter_map(|r| r.try_get::<i64, _>("menuId").ok())
                .collect();
            all.into_iter()
                .filter(|m| allowed.contains(&m.id))
                .collect()
        };

        let perms: Vec<String> = menus
            .iter()
            .filter(|m| !m.perms.is_empty())
            .map(|m| m.perms.clone())
            .collect();
        Ok((menus, perms))
    }
}

fn row_to_menu(r: &sqlx::mysql::MySqlRow) -> SysMenu {
    SysMenu {
        id: r.try_get("id").unwrap_or_default(),
        name: r.try_get("name").unwrap_or_default(),
        parent_id: r.try_get("parentId").unwrap_or_default(),
        router: r.try_get("router").unwrap_or_default(),
        perms: r.try_get("perms").unwrap_or_default(),
        r#type: r.try_get("type").unwrap_or_default(),
        icon: r.try_get("icon").unwrap_or_default(),
        order_num: r.try_get("orderNum").unwrap_or_default(),
        view_path: r.try_get("viewPath").unwrap_or_default(),
        keep_alive: r.try_get("keepAlive").unwrap_or_default(),
        is_show: r.try_get("isShow").unwrap_or_default(),
        is_auth: r.try_get("isAuth").unwrap_or_default(),
        remark: r.try_get("remark").unwrap_or_default(),
        create_time: 0,
        update_time: 0,
        children: vec![],
    }
}

fn build_tree(flat: Vec<SysMenu>) -> Vec<SysMenu> {
    use std::collections::HashMap;
    let mut map: HashMap<i64, Vec<SysMenu>> = HashMap::new();
    for m in flat {
        map.entry(m.parent_id).or_default().push(m);
    }
    fn take(parent: i64, map: &mut HashMap<i64, Vec<SysMenu>>) -> Vec<SysMenu> {
        let mut children = map.remove(&parent).unwrap_or_default();
        for c in children.iter_mut() {
            c.children = take(c.id, map);
        }
        children.sort_by(|a, b| a.order_num.cmp(&b.order_num).then(a.id.cmp(&b.id)));
        children
    }
    take(0, &mut map)
}

#[tonic::async_trait]
impl BaseSysMenuService for BaseSysMenuSvc {
    async fn list(&self, req: Request<MenuListReq>) -> Result<Response<MenuListRes>, Status> {
        let r = req.into_inner();
        let list = self.flat_list(&r.keyword).await.map_err(biz_err)?;
        let (code, message) = ok_code();
        Ok(Response::new(MenuListRes {
            code,
            message,
            list,
        }))
    }

    async fn tree(&self, req: Request<MenuTreeReq>) -> Result<Response<MenuTreeRes>, Status> {
        let r = req.into_inner();
        let flat = self.flat_list(&r.keyword).await.map_err(biz_err)?;
        let list = build_tree(flat);
        let (code, message) = ok_code();
        Ok(Response::new(MenuTreeRes {
            code,
            message,
            list,
        }))
    }

    async fn info(&self, req: Request<MenuInfoReq>) -> Result<Response<MenuInfoRes>, Status> {
        let id = req.into_inner().id;
        let row = sqlx::query("SELECT * FROM base_sys_menu WHERE id = ? AND deleteTime IS NULL")
            .bind(id)
            .fetch_optional(&self.pool)
            .await
            .map_err(|e| CoolError::Internal(format!("查询失败: {e}")))
            .map_err(biz_err)?;
        let data = row.as_ref().map(row_to_menu);
        let (code, message) = ok_code();
        Ok(Response::new(MenuInfoRes {
            code,
            message,
            data,
        }))
    }

    async fn add(&self, req: Request<MenuAddReq>) -> Result<Response<MenuAddRes>, Status> {
        let r = req.into_inner();
        if r.name.trim().is_empty() {
            return Err(Status::invalid_argument("名称不能为空"));
        }
        let res = sqlx::query(
            "INSERT INTO base_sys_menu (name, parentId, router, perms, type, icon, orderNum, viewPath, keepAlive, isShow, isAuth, remark, createTime, updateTime) \
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, NOW(3), NOW(3))",
        )
        .bind(&r.name)
        .bind(r.parent_id)
        .bind(&r.router)
        .bind(&r.perms)
        .bind(r.r#type)
        .bind(&r.icon)
        .bind(r.order_num)
        .bind(&r.view_path)
        .bind(r.keep_alive)
        .bind(r.is_show)
        .bind(r.is_auth)
        .bind(&r.remark)
        .execute(&self.pool)
        .await
        .map_err(|e| CoolError::Internal(format!("新增失败: {e}")))
        .map_err(biz_err)?;
        let id = res.last_insert_id() as i64;
        let (code, message) = ok_code();
        Ok(Response::new(MenuAddRes {
            code,
            message,
            data: Some(SysMenu {
                id,
                name: r.name,
                parent_id: r.parent_id,
                router: r.router,
                perms: r.perms,
                r#type: r.r#type,
                icon: r.icon,
                order_num: r.order_num,
                view_path: r.view_path,
                keep_alive: r.keep_alive,
                is_show: r.is_show,
                is_auth: r.is_auth,
                remark: r.remark,
                create_time: 0,
                update_time: 0,
                children: vec![],
            }),
        }))
    }

    async fn update(&self, req: Request<MenuUpdateReq>) -> Result<Response<MenuUpdateRes>, Status> {
        let r = req.into_inner();
        let mut sets: Vec<String> = Vec::new();
        let mut args: Vec<String> = Vec::new();
        macro_rules! push_str {
            ($opt:expr, $col:expr) => {
                if let Some(v) = &$opt {
                    sets.push(format!("`{}` = ?", $col));
                    args.push(v.clone());
                }
            };
        }
        macro_rules! push_num {
            ($opt:expr, $col:expr) => {
                if let Some(v) = $opt {
                    sets.push(format!("`{}` = ?", $col));
                    args.push(v.to_string());
                }
            };
        }
        push_str!(r.name, "name");
        push_num!(r.parent_id, "parentId");
        push_str!(r.router, "router");
        push_str!(r.perms, "perms");
        push_num!(r.r#type, "type");
        push_str!(r.icon, "icon");
        push_num!(r.order_num, "orderNum");
        push_str!(r.view_path, "viewPath");
        push_num!(r.keep_alive, "keepAlive");
        push_num!(r.is_show, "isShow");
        push_num!(r.is_auth, "isAuth");
        push_str!(r.remark, "remark");

        if sets.is_empty() {
            return Err(Status::invalid_argument("没有可更新的字段"));
        }
        sets.push("updateTime = NOW(3)".into());
        let sql = format!(
            "UPDATE base_sys_menu SET {} WHERE id = ? AND deleteTime IS NULL",
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
        Ok(Response::new(MenuUpdateRes {
            code,
            message,
            affected: res.rows_affected() as i64,
        }))
    }

    async fn delete(&self, req: Request<MenuDeleteReq>) -> Result<Response<MenuDeleteRes>, Status> {
        let ids = req.into_inner().ids;
        if ids.is_empty() {
            return Err(Status::invalid_argument("请选择要删除的数据"));
        }
        let placeholders = vec!["?"; ids.len()].join(", ");
        let sql = format!(
            "UPDATE base_sys_menu SET deleteTime = NOW(3), updateTime = NOW(3) \
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
        Ok(Response::new(MenuDeleteRes {
            code,
            message,
            affected: res.rows_affected() as i64,
        }))
    }

    async fn perms(&self, req: Request<MenuPermsReq>) -> Result<Response<MenuPermsRes>, Status> {
        let user_id = cool_core::satoken::login_id_from_request(&req)
            .and_then(|s| s.parse::<i64>().ok())
            .unwrap_or(0);
        if user_id <= 0 {
            return Err(Status::unauthenticated("未登录"));
        }
        let (menus, perms) = self.perms_for_user(user_id).await.map_err(biz_err)?;
        let list = build_tree(menus);
        let (code, message) = ok_code();
        Ok(Response::new(MenuPermsRes {
            code,
            message,
            data: Some(MenuPermsData { list, perms }),
        }))
    }
}
