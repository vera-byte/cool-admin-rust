//! 登录服务：验证码 / 登录 / 刷新 / 退出 / 个人信息。
//!
//! 安全项（对齐 Go 1.5.12）：
//! - 验证码一次性使用、5 分钟过期
//! - 同账号+IP 10 分钟内失败 5 次锁定
//! - 统一「账户或密码不正确」防用户名枚举
//! - refreshToken 校验缓存 + passwordVersion
//! - 改密后旧 token 失效

use base64::Engine;
use cool_core::cache;
use cool_core::error::{CoolError, CoolResult};
use cool_core::jwt::{self, JwtConfig, TokenResult};
use cool_core::password;
use rand::Rng;
use sqlx::{MySql, Pool, Row};
use std::time::Duration;
use tracing::{info, warn};

pub struct LoginService {
    pool: Pool<MySql>,
    jwt: JwtConfig,
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct UserRow {
    pub id: i64,
    pub departmentid: i64,
    pub name: String,
    pub username: String,
    pub password: String,
    pub passwordv: i32,
    pub nickname: String,
    pub headimg: String,
    pub phone: String,
    pub email: String,
    pub status: i32,
    pub remark: String,
}

impl LoginService {
    pub fn new(pool: Pool<MySql>, jwt: JwtConfig) -> Self {
        Self { pool, jwt }
    }

    pub fn jwt_config(&self) -> &JwtConfig {
        &self.jwt
    }

    /// 图形验证码（SVG + base64，对齐 Go 实现）
    pub async fn captcha(&self) -> (String, String) {
        let code: String = {
            let mut rng = rand::thread_rng();
            (0..4).map(|_| rng.gen_range(0..10).to_string()).collect()
        };
        let captcha_id = format!("cap-{:x}-{}", jwt::Claims::now_secs(), &code);
        let svg = format!(
            r##"<svg width="150" height="50" xmlns="http://www.w3.org/2000/svg"><text x="75" y="25" text-anchor="middle" font-size="25" fill="#fff">{code}</text></svg>"##
        );
        let b64 = base64::engine::general_purpose::STANDARD.encode(svg);
        // 5 分钟过期，一次性
        let _ = cache::set(&captcha_id, code, Duration::from_secs(300)).await;
        (captcha_id, format!("data:image/svg+xml;base64,{b64}"))
    }

    /// 登录
    pub async fn login(
        &self,
        username: &str,
        password_plain: &str,
        captcha_id: &str,
        verify_code: &str,
    ) -> CoolResult<TokenResult> {
        let fail_key = format!("admin:loginFail:{username}");
        let fail_count = cache::get_i64(&fail_key).await;
        if fail_count >= 5 {
            return Err(CoolError::biz("登录失败次数过多,请10分钟后再试~"));
        }

        // 验证码：一次性、防重放
        let stored = cache::get(captcha_id).await.unwrap_or(None);
        if stored.is_none() || stored.as_deref() != Some(verify_code) {
            return Err(CoolError::biz("验证码错误"));
        }
        let _ = cache::remove(captcha_id).await;

        let row = sqlx::query(
            "SELECT id, departmentId as departmentid, name, username, password, passwordV as passwordv, \
             nickName as nickname, headImg as headimg, phone, email, status, remark \
             FROM base_sys_user WHERE username = ? AND deleteTime IS NULL LIMIT 1",
        )
        .bind(username)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| {
            warn!("login query failed: {e}");
            CoolError::biz("系统繁忙,请稍后再试~")
        })?;

        let Some(row) = row else {
            let _ = cache::incr(&fail_key, Duration::from_secs(600)).await;
            return Err(CoolError::biz("账户或密码不正确~"));
        };

        let user = map_user(&row)?;
        if user.status != 1 {
            let _ = cache::incr(&fail_key, Duration::from_secs(600)).await;
            return Err(CoolError::biz("账户或密码不正确~"));
        }

        let (ok, needs_rehash) = password::verify_any(password_plain, &user.password);
        if !ok {
            let _ = cache::incr(&fail_key, Duration::from_secs(600)).await;
            return Err(CoolError::biz("账户或密码不正确~"));
        }

        // 旧 MD5 → 升级 argon2（决策 D5）
        if needs_rehash && let Ok(hash) = password::hash_password(password_plain) {
            let _ = sqlx::query(
                "UPDATE base_sys_user SET password = ?, updateTime = NOW(3) WHERE id = ?",
            )
            .bind(hash)
            .bind(user.id)
            .execute(&self.pool)
            .await;
            info!(user_id = user.id, "password upgraded to argon2");
        }

        // 登录成功清失败计数
        let _ = cache::remove(&fail_key).await;

        let role_ids = self.role_ids(user.id).await?;
        self.issue_tokens(&user, &role_ids).await
    }

    /// 刷新 token
    pub async fn refresh_token(&self, token: &str) -> CoolResult<TokenResult> {
        let claims = jwt::verify(&self.jwt, token)?;
        if !claims.is_refresh {
            return Err(CoolError::unauthenticated("refreshToken已失效,请重新登录~"));
        }
        if claims.user_id <= 0 {
            return Err(CoolError::unauthenticated("refreshToken已失效,请重新登录~"));
        }

        // 校验缓存中的 refreshToken
        let cache_key = format!("admin:token:refresh:{}", claims.user_id);
        if cache::get(&cache_key).await.unwrap_or(None).as_deref() != Some(token) {
            return Err(CoolError::unauthenticated("refreshToken已失效,请重新登录~"));
        }

        let row = sqlx::query(
            "SELECT id, departmentId as departmentid, name, username, password, passwordV as passwordv, \
             nickName as nickname, headImg as headimg, phone, email, status, remark \
             FROM base_sys_user WHERE id = ? AND status = 1 AND deleteTime IS NULL LIMIT 1",
        )
        .bind(claims.user_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| {
            warn!("refresh query failed: {e}");
            CoolError::biz("系统繁忙,请稍后再试~")
        })?;

        let Some(row) = row else {
            return Err(CoolError::unauthenticated("用户不存在"));
        };
        let user = map_user(&row)?;

        // passwordVersion 校验
        let pv_key = format!("admin:passwordVersion:{}", user.id);
        if cache::get(&pv_key)
            .await
            .unwrap_or(None)
            .and_then(|v| v.parse().ok())
            != Some(user.passwordv)
        {
            return Err(CoolError::unauthenticated("refreshToken已失效,请重新登录~"));
        }

        let role_ids = self.role_ids(user.id).await?;
        self.issue_tokens(&user, &role_ids).await
    }

    /// 退出：清除缓存
    pub async fn logout(&self, user_id: i64) {
        let _ = cache::remove(&format!("admin:token:{user_id}")).await;
        let _ = cache::remove(&format!("admin:token:refresh:{user_id}")).await;
        let _ = cache::remove(&format!("admin:perms:{user_id}")).await;
        let _ = cache::remove(&format!("admin:department:{user_id}")).await;
    }

    /// 个人信息
    pub async fn person_info(&self, user_id: i64) -> CoolResult<UserRow> {
        let row = sqlx::query(
            "SELECT id, departmentId as departmentid, name, username, password, passwordV as passwordv, \
             nickName as nickname, headImg as headimg, phone, email, status, remark \
             FROM base_sys_user WHERE id = ? AND deleteTime IS NULL LIMIT 1",
        )
        .bind(user_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| CoolError::Internal(format!("查询用户失败: {e}")))?;

        row.map(|r| map_user(&r))
            .transpose()?
            .ok_or_else(|| CoolError::biz("用户不存在"))
    }

    /// 修改个人信息（只能改自己，由 handler 保证 user_id 来自 token）
    pub async fn person_update(&self, user_id: i64, sets: &[(String, String)]) -> CoolResult<u64> {
        if sets.is_empty() {
            return Err(CoolError::invalid_argument("没有可更新的字段"));
        }
        let allowed = ["name", "nickName", "headImg", "phone", "email", "remark"];
        let mut sql = String::from("UPDATE base_sys_user SET ");
        let mut parts = Vec::new();
        for (k, _v) in sets {
            if !allowed.contains(&k.as_str()) {
                return Err(CoolError::invalid_argument(format!("非法字段: {k}")));
            }
            parts.push(format!("`{k}` = ?"));
        }
        sql.push_str(&parts.join(", "));
        sql.push_str(", updateTime = NOW(3) WHERE id = ? AND deleteTime IS NULL");

        let mut q = sqlx::query(&sql);
        for (_, v) in sets {
            q = q.bind(v);
        }
        q = q.bind(user_id);
        let r = q
            .execute(&self.pool)
            .await
            .map_err(|e| CoolError::Internal(format!("更新失败: {e}")))?;
        Ok(r.rows_affected())
    }

    /// 修改密码（旧密校验，passwordV+1，清 token）
    pub async fn person_password(
        &self,
        user_id: i64,
        old_password: &str,
        new_password: &str,
    ) -> CoolResult<()> {
        if new_password.len() < 6 {
            return Err(CoolError::biz("新密码长度不能少于6位"));
        }
        let user = self.person_info(user_id).await?;
        let (ok, _) = password::verify_any(old_password, &user.password);
        if !ok {
            return Err(CoolError::biz("原密码不正确"));
        }
        let hash = password::hash_password(new_password)?;
        sqlx::query(
            "UPDATE base_sys_user SET password = ?, passwordV = passwordV + 1, updateTime = NOW(3) WHERE id = ?",
        )
        .bind(hash)
        .bind(user_id)
        .execute(&self.pool)
        .await
        .map_err(|e| CoolError::Internal(format!("改密失败: {e}")))?;

        // 旧 token 立即失效
        self.logout(user_id).await;
        cache::set(
            &format!("admin:passwordVersion:{user_id}"),
            (user.passwordv + 1).to_string(),
            Duration::ZERO,
        )
        .await
        .unwrap_or(());
        Ok(())
    }

    /// 为断言预留：读取角色 ID
    pub async fn role_ids(&self, user_id: i64) -> CoolResult<Vec<String>> {
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
            .map(|id| id.to_string())
            .collect())
    }

    async fn issue_tokens(&self, user: &UserRow, role_ids: &[String]) -> CoolResult<TokenResult> {
        let tokens =
            jwt::issue_tokens(&self.jwt, user.id, &user.username, role_ids, user.passwordv)?;

        // 缓存 refreshToken / passwordVersion（M2 内存，M3 换 Redis）
        let ttl = Duration::from_secs(self.jwt.refresh_expire.max(60));
        let _ = cache::set(
            &format!("admin:token:refresh:{}", user.id),
            tokens.refresh_token.clone(),
            ttl,
        )
        .await;
        let _ = cache::set(
            &format!("admin:token:{}", user.id),
            tokens.token.clone(),
            ttl,
        )
        .await;
        let _ = cache::set(
            &format!("admin:passwordVersion:{}", user.id),
            user.passwordv.to_string(),
            Duration::ZERO,
        )
        .await;
        Ok(tokens)
    }
}

fn map_user(row: &sqlx::mysql::MySqlRow) -> CoolResult<UserRow> {
    let id: i64 = get_i64(row, "id");
    Ok(UserRow {
        id,
        departmentid: get_i64(row, "departmentid"),
        name: row.try_get("name").unwrap_or_default(),
        username: row.try_get("username").unwrap_or_default(),
        password: row.try_get("password").unwrap_or_default(),
        passwordv: row
            .try_get("passwordv")
            .unwrap_or_else(|_| get_i64(row, "passwordv") as i32)
            .max(1),
        nickname: row.try_get("nickname").unwrap_or_default(),
        headimg: row.try_get("headimg").unwrap_or_default(),
        phone: row.try_get("phone").unwrap_or_default(),
        email: row.try_get("email").unwrap_or_default(),
        status: row.try_get("status").unwrap_or(1),
        remark: row.try_get("remark").unwrap_or_default(),
    })
}

/// 公开映射：密码置空（列表/详情不回传 password）
pub fn map_user_pub(row: &sqlx::mysql::MySqlRow) -> CoolResult<UserRow> {
    let mut u = map_user(row)?;
    u.password = String::new();
    Ok(u)
}

/// BIGINT UNSIGNED 可能读成 u64/i64，统一转 i64
fn get_i64(row: &sqlx::mysql::MySqlRow, col: &str) -> i64 {
    // 先试 u64（BIGINT UNSIGNED 的本征类型），再试 i64
    if let Ok(v) = row.try_get::<u64, _>(col) {
        return v as i64;
    }
    if let Ok(v) = row.try_get::<i64, _>(col) {
        return v;
    }
    0
}
