//! dict 集成测试
use sqlx::mysql::MySqlPoolOptions;
use std::sync::Mutex;

static DB_LOCK: Mutex<()> = Mutex::new(());

async fn pool() -> sqlx::MySqlPool {
    let url = std::env::var("COOL_TEST_DATABASE_URL")
        .unwrap_or_else(|_| "mysql://root:123456@127.0.0.1:3306/cooltest".into());
    MySqlPoolOptions::new()
        .max_connections(2)
        .connect(&url)
        .await
        .expect("需要 MySQL")
}

async fn ensure_tables(pool: &sqlx::MySqlPool) {
    for ddl in [
        r#"CREATE TABLE IF NOT EXISTS `dict_type` (
          `id` BIGINT UNSIGNED NOT NULL AUTO_INCREMENT,
          `name` VARCHAR(255) NOT NULL, `key` VARCHAR(255) NOT NULL,
          `createTime` DATETIME(3) NOT NULL, `updateTime` DATETIME(3) NOT NULL,
          `deleteTime` DATETIME(3) NULL, PRIMARY KEY (`id`), UNIQUE KEY `uk_key` (`key`)
        ) ENGINE=InnoDB"#,
        r#"CREATE TABLE IF NOT EXISTS `dict_info` (
          `id` BIGINT UNSIGNED NOT NULL AUTO_INCREMENT,
          `typeId` BIGINT UNSIGNED NOT NULL, `name` VARCHAR(255) NOT NULL,
          `orderNum` INT NOT NULL DEFAULT 0, `remark` VARCHAR(255) NULL,
          `parentId` BIGINT UNSIGNED NULL,
          `createTime` DATETIME(3) NOT NULL, `updateTime` DATETIME(3) NOT NULL,
          `deleteTime` DATETIME(3) NULL, PRIMARY KEY (`id`)
        ) ENGINE=InnoDB"#,
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
async fn dict_data_aggregation_and_cascade_delete() {
    let _g = DB_LOCK.lock().unwrap();
    let pool = pool().await;
    ensure_tables(&pool).await;

    let key = uniq("dk");
    sqlx::query(
        "INSERT INTO dict_type (name, `key`, createTime, updateTime) VALUES ('t', ?, NOW(3), NOW(3))",
    )
    .bind(&key)
    .execute(&pool)
    .await
    .unwrap();
    let tid: u64 = sqlx::query_scalar("SELECT id FROM dict_type WHERE `key` = ?")
        .bind(&key)
        .fetch_one(&pool)
        .await
        .unwrap();

    let parent_name = uniq("p");
    let child_name = uniq("c");
    sqlx::query(
        "INSERT INTO dict_info (typeId, name, orderNum, createTime, updateTime) VALUES (?, ?, 1, NOW(3), NOW(3))",
    )
    .bind(tid)
    .bind(&parent_name)
    .execute(&pool)
    .await
    .unwrap();
    let pid: u64 = sqlx::query_scalar("SELECT id FROM dict_info WHERE name = ?")
        .bind(&parent_name)
        .fetch_one(&pool)
        .await
        .unwrap();
    sqlx::query(
        "INSERT INTO dict_info (typeId, name, orderNum, parentId, createTime, updateTime) VALUES (?, ?, 2, ?, NOW(3), NOW(3))",
    )
    .bind(tid)
    .bind(&child_name)
    .bind(pid)
    .execute(&pool)
    .await
    .unwrap();

    let svc = app_dict::service::new_dict_info_service(pool.clone());
    let map = svc.data(&[key.clone()]).await.unwrap();
    let items = map.get(&key).expect("key in map");
    assert_eq!(items.len(), 2);

    // 递归删除父 → 子也删
    svc.delete_cascade(&[pid as i64]).await.unwrap();
    let left: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM dict_info WHERE typeId = ? AND deleteTime IS NULL",
    )
    .bind(tid)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(left, 0, "children should be cascaded");
}
