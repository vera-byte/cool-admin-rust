//! 安全回归测试（对齐 cool-admin-go 1.5.12 加固清单）。
//!
//! 每个用例对应一条历史漏洞/加固项，防止回退。

use cool_core::model::{Model, safe_ident};
use cool_core::query::safe_order_by;
use cool_core::{jwt, password};

// ---------- 1. ORDER BY 注入 ----------

#[test]
fn sec_order_by_injection_blocked() {
    // 1.5.12: 列表排序参数 ORDER BY 存在 SQL 注入风险
    assert_eq!(safe_order_by("id; DROP TABLE user", "asc"), None);
    assert_eq!(safe_order_by("(SELECT 1)", "asc"), None);
    assert_eq!(safe_order_by("id--", "asc"), None);
    assert_eq!(safe_order_by("id, name", "asc"), None); // 不允许空格
    assert_eq!(safe_order_by("id", "asc;--"), None);
    // 合法仍可用
    assert!(safe_order_by("createTime", "desc").is_some());
}

// ---------- 2. 列名/标识符白名单 ----------

#[test]
fn sec_identifier_whitelist() {
    assert!(!safe_ident("name; --"));
    assert!(!safe_ident("a b"));
    assert!(!safe_ident("`x`"));
    assert!(!safe_ident("1=1"));
    assert!(safe_ident("createTime"));
}

// ---------- 3. JWT 弱密钥黑名单 ----------

#[test]
fn sec_jwt_weak_secret_blocked() {
    // 1.5.12: 扩充 JWT 弱密钥黑名单
    assert!(jwt::validate_secret("cool-base88776655-dev").is_err());
    assert!(jwt::validate_secret("changeme").is_err());
    assert!(jwt::validate_secret("short").is_err());
    assert!(jwt::validate_secret("unit-test-secret-123456").is_ok());
}

// ---------- 4. 密码哈希与旧哈希升级 ----------

#[test]
fn sec_password_hash_and_legacy_upgrade() {
    // argon2 不可逆
    let h = password::hash_password("Sup3rSecret!").unwrap();
    assert!(h.starts_with("$argon2"), "must be argon2: {h}");
    assert!(!h.contains("Sup3rSecret!"));

    // 旧 MD5 识别 + 登录后标记需升级（决策 D5）
    let legacy = password::legacy_md5("123456");
    assert!(password::is_legacy_md5(&legacy));
    let (ok, rehash) = password::verify_any("123456", &legacy);
    assert!(ok && rehash);

    // 错误密码不通过
    let (ok, _) = password::verify_any("wrong", &legacy);
    assert!(!ok);
}

// ---------- 5. token 篡改/过期 ----------

#[test]
fn sec_jwt_tamper_and_expiry() {
    let cfg = jwt::JwtConfig {
        secret: "unit-test-secret-123456".into(),
        expire: 60,
        refresh_expire: 120,
    };
    let tok = jwt::access_token(&cfg, 1, "admin", &[], 1).unwrap();
    assert!(jwt::verify(&cfg, &tok).is_ok());

    // 篡改
    let mut tampered = tok.clone();
    tampered.push('x');
    assert!(jwt::verify(&cfg, &tampered).is_err());

    // 换密钥
    let other = jwt::JwtConfig {
        secret: "different-secret-99999".into(),
        ..Default::default()
    };
    assert!(jwt::verify(&other, &tok).is_err());

    // 过期（jsonwebtoken 默认 leeway=60s，需回拨超过 leeway）
    let expired = jwt::JwtConfig {
        secret: cfg.secret.clone(),
        expire: 0,
        refresh_expire: 0,
    };
    let now = jwt::Claims::now_secs();
    let claims = jwt::Claims {
        is_refresh: false,
        role_ids: vec![],
        username: "admin".into(),
        user_id: 1,
        password_version: 1,
        exp: now - 300,
        iat: now - 360,
    };
    let tok = jwt::sign(&expired, &claims).unwrap();
    assert!(
        jwt::verify(&expired, &tok).is_err(),
        "expired token must fail"
    );
}

// ---------- 6. refreshToken 必须带 is_refresh ----------

#[test]
fn sec_refresh_token_flag_required() {
    let cfg = jwt::JwtConfig {
        secret: "unit-test-secret-123456".into(),
        expire: 60,
        refresh_expire: 120,
    };
    let access = jwt::access_token(&cfg, 1, "admin", &[], 1).unwrap();
    let claims = jwt::verify(&cfg, &access).unwrap();
    assert!(!claims.is_refresh, "access token must not be refresh");
}

// ---------- 7. 密码不回传（模型层字段约定） ----------

#[test]
fn sec_password_field_not_writable() {
    struct M;
    const COLS: &[cool_core::ColumnMeta] = &[
        cool_core::ColumnMeta::read("id"),
        cool_core::ColumnMeta::rw("name"),
        cool_core::ColumnMeta::computed("createTime"),
    ];
    impl Model for M {
        fn table_name() -> &'static str {
            "t"
        }
        fn columns() -> &'static [cool_core::ColumnMeta] {
            COLS
        }
    }
    // password 不在列中 → 任何写入都会被拒（列白名单）
    assert!(!M::is_writable("password"));
    assert!(!M::is_writable("deleteTime"));
}

// ---------- 8. 文件路径穿越（cool-file） ----------

#[tokio::test]
async fn sec_file_path_traversal_blocked() {
    use cool_file::local::LocalDriver;
    use cool_file::{FileConfig, FileDriver};
    let d = LocalDriver::new(FileConfig::default());
    let e = d
        .download("public/uploads/../../../../etc/passwd")
        .await
        .unwrap_err();
    assert!(e.to_string().contains("非法路径"), "err={e}");
}
