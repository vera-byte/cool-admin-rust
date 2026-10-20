//! dict/task/file 集成测试（需要 MySQL）

use std::sync::Mutex;

use sqlx::mysql::MySqlPoolOptions;

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
        r#"CREATE TABLE IF NOT EXISTS `task_info` (
          `id` BIGINT UNSIGNED NOT NULL AUTO_INCREMENT,
          `jobId` VARCHAR(255) NOT NULL DEFAULT '', `repeatConf` VARCHAR(255) NOT NULL DEFAULT '',
          `name` VARCHAR(255) NOT NULL, `cron` VARCHAR(255) NOT NULL DEFAULT '',
          `limit` INT NOT NULL DEFAULT 0, `every` INT NOT NULL DEFAULT 0,
          `remark` VARCHAR(255) NOT NULL DEFAULT '', `status` TINYINT NOT NULL DEFAULT 0,
          `startDate` DATETIME(3) NULL, `endDate` DATETIME(3) NULL,
          `data` VARCHAR(255) NOT NULL DEFAULT '', `service` VARCHAR(255) NOT NULL DEFAULT '',
          `type` TINYINT NOT NULL DEFAULT 0, `nextRunTime` DATETIME(3) NULL,
          `taskType` TINYINT NOT NULL DEFAULT 0,
          `createTime` DATETIME(3) NOT NULL, `updateTime` DATETIME(3) NOT NULL,
          `deleteTime` DATETIME(3) NULL, PRIMARY KEY (`id`), UNIQUE KEY `uk_name` (`name`)
        ) ENGINE=InnoDB"#,
        r#"CREATE TABLE IF NOT EXISTS `task_log` (
          `id` BIGINT UNSIGNED NOT NULL AUTO_INCREMENT,
          `taskId` BIGINT UNSIGNED NOT NULL, `status` TINYINT NOT NULL DEFAULT 0,
          `detail` TEXT, `createTime` DATETIME(3) NOT NULL, `updateTime` DATETIME(3) NOT NULL,
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
async fn task_record_pruning_keeps_20_success() {
    let _g = DB_LOCK.lock().unwrap();
    let pool = pool().await;
    ensure_tables(&pool).await;

    let name = uniq("tk");
    sqlx::query(
        "INSERT INTO task_info (name, cron, service, status, createTime, updateTime) VALUES (?, '@every 60s', 'TaskTest(x)', 0, NOW(3), NOW(3))",
    )
    .bind(&name)
    .execute(&pool)
    .await
    .unwrap();
    let tid: u64 = sqlx::query_scalar("SELECT id FROM task_info WHERE name = ?")
        .bind(&name)
        .fetch_one(&pool)
        .await
        .unwrap();

    let svc = mod_task::service::TaskInfoService::new(pool.clone());
    for i in 0..25 {
        svc.record(tid as i64, 1, &format!("ok{i}")).await;
    }
    svc.record(tid as i64, 0, "fail1").await;
    svc.record(tid as i64, 0, "fail2").await;

    let ok: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM task_log WHERE taskId = ? AND status = 1 AND deleteTime IS NULL",
    )
    .bind(tid)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(ok, 20, "keep only 20 success");

    let fail: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM task_log WHERE taskId = ? AND status = 0 AND deleteTime IS NULL",
    )
    .bind(tid)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(fail, 2, "failures never pruned");
}

#[tokio::test]
async fn task_parse_cron_and_log_page() {
    let _g = DB_LOCK.lock().unwrap();
    let pool = pool().await;
    ensure_tables(&pool).await;

    // cron 解析
    assert!(mod_task::scheduler::parse_cron("@every 5s").is_ok());
    assert!(mod_task::scheduler::parse_cron("0 0 12 * * *").is_ok());
    assert!(mod_task::scheduler::parse_cron("@hourly").is_ok());
    assert!(mod_task::scheduler::parse_cron("bad").is_err());

    let name = uniq("tl");
    sqlx::query(
        "INSERT INTO task_info (name, cron, service, status, createTime, updateTime) VALUES (?, '@every 60s', 'TaskTest(x)', 0, NOW(3), NOW(3))",
    )
    .bind(&name)
    .execute(&pool)
    .await
    .unwrap();
    let tid: u64 = sqlx::query_scalar("SELECT id FROM task_info WHERE name = ?")
        .bind(&name)
        .fetch_one(&pool)
        .await
        .unwrap();

    let svc = mod_task::service::TaskInfoService::new(pool.clone());
    svc.record(tid as i64, 1, "hello").await;
    let (total, list) = svc.log_page(Some(tid as i64), None, 1, 10).await.unwrap();
    assert_eq!(total, 1);
    assert_eq!(
        list[0].get("detail").and_then(|v| v.as_str()),
        Some("hello")
    );
}
