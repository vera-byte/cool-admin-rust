//! BaseOpenService：验证码 / 登录 / 刷新 / 退出 / 个人信息

use cool_core::AuthUser;
use cool_core::ok_code;
use cool_proto::base::v1::base_open_service_server::BaseOpenService;
use cool_proto::base::v1::{
    CaptchaData, CaptchaReq, CaptchaRes, EpsAppReq, EpsAppRes, EpsReq, EpsRes, LoginReq, LoginRes,
    LogoutReq, LogoutRes, PersonInfoData, PersonInfoReq, PersonInfoRes, PersonPasswordReq,
    PersonPasswordRes, PersonUpdateReq, PersonUpdateRes, RefreshTokenReq, RefreshTokenRes,
    TokenData,
};
use sqlx::MySqlPool;
use tonic::{Request, Response, Status};

use crate::BaseConfig;
use crate::service::LoginService;

pub struct BaseOpenSvc {
    login: LoginService,
}

impl BaseOpenSvc {
    pub fn new(pool: MySqlPool, cfg: BaseConfig) -> Self {
        Self {
            login: LoginService::new(pool, cfg.jwt),
        }
    }
}

fn auth_user_id<T>(req: &Request<T>) -> i64 {
    req.extensions()
        .get::<AuthUser>()
        .map(|u| u.user_id)
        .unwrap_or(0)
}

fn token_data(t: &cool_core::jwt::TokenResult) -> TokenData {
    TokenData {
        expire: t.expire as i64,
        token: t.token.clone(),
        refresh_expire: t.refresh_expire as i64,
        refresh_token: t.refresh_token.clone(),
    }
}

#[tonic::async_trait]
impl BaseOpenService for BaseOpenSvc {
    async fn captcha(&self, _req: Request<CaptchaReq>) -> Result<Response<CaptchaRes>, Status> {
        let (id, data) = self.login.captcha().await;
        let (code, message) = ok_code();
        Ok(Response::new(CaptchaRes {
            code,
            message,
            data: Some(CaptchaData {
                captcha_id: id,
                data,
            }),
        }))
    }

    async fn login(&self, req: Request<LoginReq>) -> Result<Response<LoginRes>, Status> {
        let r = req.into_inner();
        let tokens = self
            .login
            .login(&r.username, &r.password, &r.captcha_id, &r.verify_code)
            .await
            .map_err(super::biz_err)?;
        let (code, message) = ok_code();
        Ok(Response::new(LoginRes {
            code,
            message,
            data: Some(token_data(&tokens)),
        }))
    }

    async fn refresh_token(
        &self,
        req: Request<RefreshTokenReq>,
    ) -> Result<Response<RefreshTokenRes>, Status> {
        let r = req.into_inner();
        let tokens = self
            .login
            .refresh_token(&r.refresh_token)
            .await
            .map_err(super::biz_err)?;
        let (code, message) = ok_code();
        Ok(Response::new(RefreshTokenRes {
            code,
            message,
            data: Some(token_data(&tokens)),
        }))
    }

    async fn logout(&self, req: Request<LogoutReq>) -> Result<Response<LogoutRes>, Status> {
        // 从扩展读取 user_id（AuthLayer 注入）
        let user_id = auth_user_id(&req);
        if user_id > 0 {
            self.login.logout(user_id).await;
        }
        let (code, message) = ok_code();
        Ok(Response::new(LogoutRes { code, message }))
    }

    async fn person_info(
        &self,
        req: Request<PersonInfoReq>,
    ) -> Result<Response<PersonInfoRes>, Status> {
        let user_id = auth_user_id(&req);
        if user_id <= 0 {
            return Err(Status::unauthenticated("未登录"));
        }
        let u = self
            .login
            .person_info(user_id)
            .await
            .map_err(super::biz_err)?;
        let (code, message) = ok_code();
        Ok(Response::new(PersonInfoRes {
            code,
            message,
            data: Some(PersonInfoData {
                id: u.id,
                department_id: u.departmentid,
                name: u.name,
                username: u.username,
                nick_name: u.nickname,
                head_img: u.headimg,
                phone: u.phone,
                email: u.email,
                status: u.status,
                remark: u.remark,
                role_ids: self
                    .login
                    .role_ids(user_id)
                    .await
                    .unwrap_or_default()
                    .iter()
                    .filter_map(|v| v.parse::<i64>().ok())
                    .collect(),
                role_names: vec![],
            }),
        }))
    }

    async fn person_update(
        &self,
        req: Request<PersonUpdateReq>,
    ) -> Result<Response<PersonUpdateRes>, Status> {
        // 只能改自己：user_id 来自 token，不接受客户端传入
        let user_id = auth_user_id(&req);
        if user_id <= 0 {
            return Err(Status::unauthenticated("未登录"));
        }
        let r = req.into_inner();
        let mut sets = Vec::new();
        if let Some(v) = r.name {
            sets.push(("name".to_string(), v));
        }
        if let Some(v) = r.nick_name {
            sets.push(("nickName".to_string(), v));
        }
        if let Some(v) = r.head_img {
            sets.push(("headImg".to_string(), v));
        }
        if let Some(v) = r.phone {
            sets.push(("phone".to_string(), v));
        }
        if let Some(v) = r.email {
            sets.push(("email".to_string(), v));
        }
        if let Some(v) = r.remark {
            sets.push(("remark".to_string(), v));
        }
        self.login
            .person_update(user_id, &sets)
            .await
            .map_err(super::biz_err)?;
        let (code, message) = ok_code();
        Ok(Response::new(PersonUpdateRes { code, message }))
    }

    async fn person_password(
        &self,
        req: Request<PersonPasswordReq>,
    ) -> Result<Response<PersonPasswordRes>, Status> {
        let user_id = auth_user_id(&req);
        if user_id <= 0 {
            return Err(Status::unauthenticated("未登录"));
        }
        let r = req.into_inner();
        self.login
            .person_password(user_id, &r.password, &r.new_password)
            .await
            .map_err(super::biz_err)?;
        let (code, message) = ok_code();
        Ok(Response::new(PersonPasswordRes { code, message }))
    }

    async fn eps(&self, _req: Request<EpsReq>) -> Result<Response<EpsRes>, Status> {
        let v = cool_core::eps::admin_eps();
        let (code, message) = ok_code();
        let mut data = std::collections::HashMap::new();
        if let Some(obj) = v.as_object() {
            for (k, val) in obj {
                data.insert(k.clone(), serde_json::to_string(val).unwrap_or_default());
            }
        }
        Ok(Response::new(EpsRes {
            code,
            message,
            data,
        }))
    }

    async fn eps_app(&self, _req: Request<EpsAppReq>) -> Result<Response<EpsAppRes>, Status> {
        let v = cool_core::eps::app_eps();
        let (code, message) = ok_code();
        Ok(Response::new(EpsAppRes {
            code,
            message,
            data: serde_json::to_string(&v).unwrap_or_default(),
        }))
    }
}
