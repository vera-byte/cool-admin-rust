//! DemoSample CRUD 集成测试（需要 MySQL，默认连 127.0.0.1:3306 cooltest/123456）。
//!
//! 运行：`cargo test -p mod-demo -- --nocapture`
//! 无数据库时用例会快速失败（skip 不可用，直接 assert 连接成功）。

use cool_core::CrudService;
use cool_core::query::QueryInput;
use serde_json::json;
use sqlx::mysql::MySqlPoolOptions;
use std::sync::Mutex;

use mod_demo::new_demo_sample_service;

/// 集成测试串行锁，避免并行 setup_table 相互干扰
static DB_LOCK: Mutex<()> = Mutex::new(());

async fn pool() -> sqlx::MySqlPool {
    let url = std::env::var("COOL_TEST_DATABASE_URL")
        .unwrap_or_else(|_| "mysql://root:123456@127.0.0.1:3306/cooltest".into());
    MySqlPoolOptions::new()
        .max_connections(2)
        .connect(&url)
        .await
        .expect("测试需要可用 MySQL，可设 COOL_TEST_DATABASE_URL")
}

async fn setup_table(pool: &sqlx::MySqlPool) {
    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS `demo_sample` (
          `id` BIGINT UNSIGNED NOT NULL AUTO_INCREMENT,
          `name` VARCHAR(255) NOT NULL DEFAULT '' COMMENT '名称',
          `remark` VARCHAR(500) NOT NULL DEFAULT '' COMMENT '备注',
          `createTime` DATETIME(3) NOT NULL,
          `updateTime` DATETIME(3) NOT NULL,
          `deleteTime` DATETIME(3) NULL DEFAULT NULL,
          PRIMARY KEY (`id`),
          KEY `idx_deleteTime` (`deleteTime`)
        ) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4
        "#,
    )
    .execute(pool)
    .await
    .unwrap();
    sqlx::query("DELETE FROM `demo_sample`")
        .execute(pool)
        .await
        .unwrap();
}

fn uniq(prefix: &str) -> String {
    let ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    format!("{prefix}_{ts}")
}

#[tokio::test]
async fn crud_add_info_update_list_page_delete() {
    let _guard = DB_LOCK.lock().unwrap();
    let pool = pool().await;
    setup_table(&pool).await;
    let svc = new_demo_sample_service(pool);

    let name_a = uniq("alpha");
    let name_b = uniq("beta");
    let kw = format!("kw_{}", name_a);

    // Add
    let mut data = cool_core::FieldMap::new();
    data.insert("name".into(), json!(&name_a));
    data.insert("remark".into(), json!(&kw));
    let row = svc.service_add(data).await.expect("add should succeed");
    let id = row.get("id").and_then(|v| v.as_i64()).expect("id");
    assert_eq!(
        row.get("name").and_then(|v| v.as_str()),
        Some(name_a.as_str())
    );

    // UniqueKey: 同名再加应失败
    let mut dup = cool_core::FieldMap::new();
    dup.insert("name".into(), json!(&name_a));
    let err = svc.service_add(dup).await.unwrap_err();
    assert!(err.to_string().contains("名称已存在"), "err={err}");

    // NotNullKey
    let empty = cool_core::FieldMap::new();
    let err = svc.service_add(empty).await.unwrap_err();
    assert!(err.to_string().contains("名称不能为空"), "err={err}");

    // Info
    let got = svc.service_info(id).await.unwrap().expect("row");
    assert_eq!(
        got.get("remark").and_then(|v| v.as_str()),
        Some(kw.as_str())
    );

    // Update
    let mut up = cool_core::FieldMap::new();
    up.insert("remark".into(), json!("updated"));
    let n = svc.service_update(id, up).await.unwrap();
    assert_eq!(n, 1);
    let got = svc.service_info(id).await.unwrap().unwrap();
    assert_eq!(got.get("remark").and_then(|v| v.as_str()), Some("updated"));

    // List + keyword
    let mut data2 = cool_core::FieldMap::new();
    data2.insert("name".into(), json!(&name_b));
    data2.insert("remark".into(), json!(format!("search_{kw}")));
    svc.service_add(data2).await.unwrap();

    let list = svc
        .service_list(QueryInput {
            keyword: Some(format!("search_{kw}")),
            ..Default::default()
        })
        .await
        .unwrap();
    assert_eq!(list.len(), 1);
    assert_eq!(
        list[0].get("name").and_then(|v| v.as_str()),
        Some(name_b.as_str())
    );

    // List + eq filter
    let list = svc
        .service_list(QueryInput {
            eq: [("name".into(), json!(&name_a))].into_iter().collect(),
            ..Default::default()
        })
        .await
        .unwrap();
    assert_eq!(list.len(), 1);

    // Page
    let page = svc
        .service_page(QueryInput {
            page: 1,
            size: 100,
            ..Default::default()
        })
        .await
        .unwrap();
    assert!(page.total >= 2);
    assert!(!page.list.is_empty());

    // Delete（软删）
    let n = svc.service_delete(&[id]).await.unwrap();
    assert_eq!(n, 1);
    assert!(svc.service_info(id).await.unwrap().is_none());

    // 再删同一条 → 0 行
    let n = svc.service_delete(&[id]).await.unwrap();
    assert_eq!(n, 0);
}

#[tokio::test]
async fn update_rejects_illegal_column() {
    let _guard = DB_LOCK.lock().unwrap();
    let pool = pool().await;
    setup_table(&pool).await;
    let svc = new_demo_sample_service(pool);

    let mut data = cool_core::FieldMap::new();
    data.insert("name".into(), json!(uniq("ux")));
    let row = svc.service_add(data).await.unwrap();
    let id = row.get("id").and_then(|v| v.as_i64()).unwrap();

    let mut evil = cool_core::FieldMap::new();
    evil.insert("evilCol".into(), json!("hack")); // 非法列
    let err = svc.service_update(id, evil).await.unwrap_err();
    assert!(err.to_string().contains("非法字段"), "err={err}");
}
