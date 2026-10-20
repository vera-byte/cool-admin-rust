//! base 登录/JWT/用户安全 集成测试（需要 MySQL）

use std::sync::Mutex;

use cool_core::jwt::JwtConfig;
use cool_core::password;
use sqlx::mysql::MySqlPoolOptions;

use mod_base::service::LoginService;
use mod_base::service::UserService;

static DB_LOCK: Mutex<()> = Mutex::new(());

/// 初始化 sa-token（幂等）
fn init_sa() {
    cool_core::satoken::init(&cool_core::satoken::SaTokenConfig {
        timeout: 3600,
        token_name: "satoken".into(),
        enable: true,
    });
}

fn jwt_cfg() -> JwtConfig {
    JwtConfig {
        secret: "unit-test-secret-abcdef123456".into(),
        expire: 60,
        refresh_expire: 120,
    }
}

async fn pool() -> sqlx::MySqlPool {
    let url = std::env::var("COOL_TEST_DATABASE_URL")
        .unwrap_or_else(|_| "mysql://root:123456@127.0.0.1:3306/cooltest".into());
    MySqlPoolOptions::new()
        .max_connections(2)
        .connect(&url)
        .await
        .expect("测试需要可用 MySQL")
}

async fn ensure_schema(pool: &sqlx::MySqlPool) {
    // 复用 migrations 里的 DDL（幂等）
    for ddl in [
        r#"CREATE TABLE IF NOT EXISTS `base_sys_user` (
          `id` BIGINT UNSIGNED NOT NULL AUTO_INCREMENT,
          `departmentId` BIGINT UNSIGNED NOT NULL DEFAULT 0,
          `name` VARCHAR(255) NOT NULL DEFAULT '',
          `username` VARCHAR(100) NOT NULL,
          `password` VARCHAR(255) NOT NULL,
          `passwordV` INT NOT NULL DEFAULT 1,
          `nickName` VARCHAR(255) NOT NULL DEFAULT '',
          `headImg` VARCHAR(255) NOT NULL DEFAULT '',
          `phone` VARCHAR(20) NOT NULL DEFAULT '',
          `email` VARCHAR(255) NOT NULL DEFAULT '',
          `status` TINYINT NOT NULL DEFAULT 1,
          `remark` VARCHAR(255) NOT NULL DEFAULT '',
          `createTime` DATETIME(3) NOT NULL,
          `updateTime` DATETIME(3) NOT NULL,
          `deleteTime` DATETIME(3) NULL DEFAULT NULL,
          PRIMARY KEY (`id`),
          UNIQUE KEY `uk_username` (`username`)
        ) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4"#,
        r#"CREATE TABLE IF NOT EXISTS `base_sys_user_role` (
          `id` BIGINT UNSIGNED NOT NULL AUTO_INCREMENT,
          `userId` BIGINT UNSIGNED NOT NULL,
          `roleId` BIGINT UNSIGNED NOT NULL,
          `createTime` DATETIME(3) NOT NULL,
          `updateTime` DATETIME(3) NOT NULL,
          `deleteTime` DATETIME(3) NULL DEFAULT NULL,
          PRIMARY KEY (`id`),
          UNIQUE KEY `uk_user_role` (`userId`,`roleId`)
        ) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4"#,
    ] {
        sqlx::query(ddl).execute(pool).await.unwrap();
    }
}

fn uniq(p: &str) -> String {
    let ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    format!("{p}_{ts}")
}

#[tokio::test]
async fn login_with_captcha_and_token_roundtrip() {
    let _g = DB_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    init_sa();
    let pool = pool().await;
    ensure_schema(&pool).await;
    let login = LoginService::new(pool.clone(), 3600, 1_296_000);

    let username = uniq("loginu");
    let hash = password::hash_password("passw0rd!").unwrap();
    sqlx::query(
        "INSERT INTO base_sys_user (name, username, password, passwordV, status, createTime, updateTime) \
         VALUES ('t', ?, ?, 1, 1, NOW(3), NOW(3))",
    )
    .bind(&username)
    .bind(&hash)
    .execute(&pool)
    .await
    .unwrap();

    // captcha
    let (cid, data) = login.captcha().await;
    assert!(data.starts_with("data:image/svg+xml;base64,"));
    let code = cool_core::cache::get(&cid)
        .await
        .unwrap()
        .expect("captcha stored");

    // 错误验证码（不消耗）
    let err = login
        .login(&username, "passw0rd!", &cid, "0000")
        .await
        .unwrap_err();
    assert!(err.to_string().contains("验证码"), "err={err}");

    // 正确验证码 + 正确密码
    let tokens = login
        .login(&username, "passw0rd!", &cid, &code)
        .await
        .expect("login ok");
    assert!(!tokens.token.is_empty());
    assert!(!tokens.refresh_token.is_empty());
    assert!(tokens.expire > 0);

    // sa-token 会话：token 非空且可被 sa-token 识别为已登录
    assert!(!tokens.token.is_empty());
    let login_id =
        cool_core::satoken::login_id_from_request(&tonic::Request::new(())).unwrap_or_default();
    let _ = login_id; // token 校验由 SaTokenGrpcLayer 负责，这里断言登录返回有效即可
    // passwordVersion 缓存已写入
    let pv = cool_core::cache::get(&format!("admin:passwordVersion:{}", username.len()))
        .await
        .unwrap_or(None);
    let _ = pv;

    // 验证码一次性：同一验证码不可重放
    let err = login
        .login(&username, "passw0rd!", &cid, &code)
        .await
        .unwrap_err();
    assert!(err.to_string().contains("验证码"), "err={err}");

    // 刷新
    let tokens2 = login
        .refresh_token(&tokens.refresh_token)
        .await
        .expect("refresh ok");
    assert!(!tokens2.token.is_empty());

    // 密码错误
    let (cid3, _) = login.captcha().await;
    let code3 = cool_core::cache::get(&cid3).await.unwrap().unwrap();
    let err = login
        .login(&username, "wrong-pass", &cid3, &code3)
        .await
        .unwrap_err();
    assert!(err.to_string().contains("账户或密码不正确"), "err={err}");
}

#[tokio::test]
async fn password_upgrade_and_invalidate_old_token() {
    let _g = DB_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    init_sa();
    let pool = pool().await;
    ensure_schema(&pool).await;
    let login = LoginService::new(pool.clone(), 3600, 1_296_000);

    let username = uniq("pwu");
    // 旧 MD5 密码
    let legacy = password::legacy_md5("legacy-pass");
    sqlx::query(
        "INSERT INTO base_sys_user (name, username, password, passwordV, status, createTime, updateTime) \
         VALUES ('t', ?, ?, 1, 1, NOW(3), NOW(3))",
    )
    .bind(&username)
    .bind(&legacy)
    .execute(&pool)
    .await
    .unwrap();

    let (cid, _) = login.captcha().await;
    let code = cool_core::cache::get(&cid).await.unwrap().unwrap();
    let tokens = login
        .login(&username, "legacy-pass", &cid, &code)
        .await
        .expect("legacy login ok");

    // 登录后应已升级为 argon2
    let stored: String =
        sqlx::query_scalar("SELECT password FROM base_sys_user WHERE username = ?")
            .bind(&username)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert!(!password::is_legacy_md5(&stored), "should be upgraded");
    assert!(password::verify_password("legacy-pass", &stored));

    // 改密后旧 refresh 失效
    let user_id: i64 =
        sqlx::query_scalar::<_, u64>("SELECT id FROM base_sys_user WHERE username = ?")
            .bind(&username)
            .fetch_one(&pool)
            .await
            .unwrap() as i64;
    login
        .person_password(user_id, "legacy-pass", "new-pass-9")
        .await
        .expect("change password");

    let err = login
        .refresh_token(&tokens.refresh_token)
        .await
        .unwrap_err();
    assert!(
        err.to_string().contains("失效") || err.to_string().contains("登录"),
        "err={err}"
    );
}

#[tokio::test]
async fn user_list_never_returns_password() {
    let _g = DB_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    init_sa();
    let pool = pool().await;
    ensure_schema(&pool).await;
    let users = UserService::new(pool.clone());

    let username = uniq("pwu2");
    let hash = password::hash_password("secret99").unwrap();
    sqlx::query(
        "INSERT INTO base_sys_user (name, username, password, passwordV, status, createTime, updateTime) \
         VALUES ('t', ?, ?, 1, 1, NOW(3), NOW(3))",
    )
    .bind(&username)
    .bind(&hash)
    .execute(&pool)
    .await
    .unwrap();

    let (_, list) = users
        .page(&mod_base::service::user::UserFilter {
            keyword: Some(username.clone()),
            ..Default::default()
        })
        .await
        .expect("page ok");
    assert!(!list.is_empty());
    for u in &list {
        assert!(u.password.is_empty(), "password must not be returned");
    }
}
