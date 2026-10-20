//! BaseSysUserService：用户管理

use cool_core::ok_code;
use cool_proto::base::v1::base_sys_user_service_server::BaseSysUserService;
use cool_proto::base::v1::{
    SysUser, UserAddReq, UserAddRes, UserDeleteReq, UserDeleteRes, UserInfoReq, UserInfoRes,
    UserListReq, UserListRes, UserPageReq, UserPageRes, UserUpdateReq, UserUpdateRes,
};
use sqlx::MySqlPool;
use tonic::{Request, Response, Status};

use crate::service::UserService;
use crate::service::login::UserRow;

pub struct BaseSysUserSvc {
    users: UserService,
}

impl BaseSysUserSvc {
    pub fn new(pool: MySqlPool) -> Self {
        Self {
            users: UserService::new(pool),
        }
    }
}

fn to_proto(u: &UserRow, role_ids: Vec<i64>) -> SysUser {
    SysUser {
        id: u.id,
        department_id: u.departmentid,
        name: u.name.clone(),
        username: u.username.clone(),
        nick_name: u.nickname.clone(),
        head_img: u.headimg.clone(),
        phone: u.phone.clone(),
        email: u.email.clone(),
        status: u.status,
        remark: u.remark.clone(),
        create_time: 0,
        update_time: 0,
        role_ids,
    }
}

#[tonic::async_trait]
impl BaseSysUserService for BaseSysUserSvc {
    async fn page(&self, req: Request<UserPageReq>) -> Result<Response<UserPageRes>, Status> {
        let login_id = cool_core::satoken::login_id_from_request(&req)
            .and_then(|s| s.parse::<i64>().ok())
            .unwrap_or(0);
        let r = req.into_inner();
        let scope = if login_id > 0 {
            let roles: Vec<i64> = self.users.role_ids(login_id).await.unwrap_or_default();
            self.users.scope_departments(login_id, &roles).await
        } else {
            Some(vec![])
        };
        let f = crate::service::user::UserFilter {
            keyword: Some(r.keyword).filter(|s| !s.is_empty()),
            department_id: r.department_id,
            status: r.status,
            page: r.page as i64,
            size: r.size as i64,
            order: Some(r.order).filter(|s| !s.is_empty()),
            sort: Some(r.sort).filter(|s| !s.is_empty()),
            scope_departments: scope,
        };
        let (total, list) = self.users.page(&f).await.map_err(super::biz_err)?;
        let mut out = Vec::new();
        for u in list {
            let roles = self.users.role_ids(u.id).await.unwrap_or_default();
            out.push(to_proto(&u, roles));
        }
        let (code, message) = ok_code();
        Ok(Response::new(UserPageRes {
            code,
            message,
            total,
            list: out,
        }))
    }

    async fn list(&self, req: Request<UserListReq>) -> Result<Response<UserListRes>, Status> {
        let login_id = cool_core::satoken::login_id_from_request(&req)
            .and_then(|s| s.parse::<i64>().ok())
            .unwrap_or(0);
        let r = req.into_inner();
        let scope = if login_id > 0 {
            let roles: Vec<i64> = self.users.role_ids(login_id).await.unwrap_or_default();
            self.users.scope_departments(login_id, &roles).await
        } else {
            Some(vec![])
        };
        let f = crate::service::user::UserFilter {
            keyword: Some(r.keyword).filter(|s| !s.is_empty()),
            department_id: r.department_id,
            page: 1,
            size: 100,
            scope_departments: scope,
            ..Default::default()
        };
        let (_, list) = self.users.page(&f).await.map_err(super::biz_err)?;
        let mut out = Vec::new();
        for u in list {
            let roles = self.users.role_ids(u.id).await.unwrap_or_default();
            out.push(to_proto(&u, roles));
        }
        let (code, message) = ok_code();
        Ok(Response::new(UserListRes {
            code,
            message,
            list: out,
        }))
    }

    async fn info(&self, req: Request<UserInfoReq>) -> Result<Response<UserInfoRes>, Status> {
        let id = req.into_inner().id;
        let row = self.users.info(id).await.map_err(super::biz_err)?;
        let (code, message) = ok_code();
        let data = match row {
            Some(u) => {
                let roles = self.users.role_ids(u.id).await.unwrap_or_default();
                Some(to_proto(&u, roles))
            }
            None => None,
        };
        Ok(Response::new(UserInfoRes {
            code,
            message,
            data,
        }))
    }

    async fn add(&self, req: Request<UserAddReq>) -> Result<Response<UserAddRes>, Status> {
        let r = req.into_inner();
        let id = self
            .users
            .add(
                r.department_id,
                &r.name,
                &r.username,
                &r.password,
                &r.nick_name,
                &r.phone,
                &r.email,
                r.status,
                &r.remark,
                &r.role_ids,
            )
            .await
            .map_err(super::biz_err)?;
        let (code, message) = ok_code();
        let data = self.users.info(id).await.ok().flatten().map(|u| {
            let roles = r.role_ids.clone();
            to_proto(&u, roles)
        });
        Ok(Response::new(UserAddRes {
            code,
            message,
            data,
        }))
    }

    async fn update(&self, req: Request<UserUpdateReq>) -> Result<Response<UserUpdateRes>, Status> {
        let r = req.into_inner();
        let roles = if r.role_ids.is_empty() && r.password.is_none() {
            None
        } else if r.role_ids.is_empty() {
            // 未传角色表示不改
            None
        } else {
            Some(r.role_ids.clone())
        };
        let affected = self
            .users
            .update(
                r.id,
                r.department_id,
                r.name.as_deref(),
                r.password.as_deref(),
                r.nick_name.as_deref(),
                r.phone.as_deref(),
                r.email.as_deref(),
                r.status,
                r.remark.as_deref(),
                roles.as_deref(),
            )
            .await
            .map_err(super::biz_err)?;
        let (code, message) = ok_code();
        Ok(Response::new(UserUpdateRes {
            code,
            message,
            affected: affected as i64,
        }))
    }

    async fn delete(&self, req: Request<UserDeleteReq>) -> Result<Response<UserDeleteRes>, Status> {
        let ids = req.into_inner().ids;
        let affected = self.users.delete(&ids).await.map_err(super::biz_err)?;
        let (code, message) = ok_code();
        Ok(Response::new(UserDeleteRes {
            code,
            message,
            affected: affected as i64,
        }))
    }
}
