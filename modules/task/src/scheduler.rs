//! 任务调度器：cron 解析 + tokio 任务循环（对齐 Go gcron 语义）。
//!
//! - 6 段 cron（秒 分 时 日 月 周）+ `@every` + `@yearly/@monthly/@weekly/@daily/@hourly`
//! - 每任务一个 tokio task，CancellationToken 控制启停
//! - startDate 门闩：到点前跳过
//! - singleton：AtomicBool 防重叠

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use chrono::{DateTime, Utc};
use cron::Schedule;
use once_cell::sync::Lazy;
use tokio_util::sync::CancellationToken;

use cool_core::error::{CoolError, CoolResult};

/// cron 规格
#[derive(Debug, Clone)]
pub enum CronSpec {
    /// cron 表达式调度（体积大，Box 以收窄枚举）
    Schedule(Box<Schedule>),
    Every(Duration),
}

/// 解析 cron 表达式（对齐 Go robfig/cron：Seconds|Descriptors|@every）
pub fn parse_cron(s: &str) -> CoolResult<CronSpec> {
    let s = s.trim();
    if s.is_empty() {
        return Err(CoolError::invalid_argument("cron 表达式为空"));
    }
    // @every Ns/m/h
    if let Some(rest) = s.strip_prefix("@every ") {
        let d = parse_duration(rest.trim())?;
        return Ok(CronSpec::Every(d));
    }
    // 描述符展开（对齐 robfig Descriptors）
    let expanded = match s {
        "@yearly" | "@annually" => "0 0 0 1 1 *",
        "@monthly" => "0 0 0 1 * *",
        "@weekly" => "0 0 0 * * 0",
        "@daily" | "@midnight" => "0 0 0 * * *",
        "@hourly" => "0 0 * * * *",
        other => other,
    };
    // cron crate 默认 7 段（含秒），使用带秒的表达式
    let sched = expanded
        .parse::<Schedule>()
        .map_err(|e| CoolError::invalid_argument(format!("cron 表达式非法: {e}")))?;
    Ok(CronSpec::Schedule(Box::new(sched)))
}

fn parse_duration(s: &str) -> CoolResult<Duration> {
    let s = s.trim();
    if let Some(ms) = s.strip_suffix("ms") {
        let n: u64 = ms
            .trim()
            .parse()
            .map_err(|_| CoolError::invalid_argument("时长非法"))?;
        return Ok(Duration::from_millis(n));
    }
    if let Some(secs) = s.strip_suffix('s') {
        let n: u64 = secs
            .trim()
            .parse()
            .map_err(|_| CoolError::invalid_argument("时长非法"))?;
        return Ok(Duration::from_secs(n));
    }
    if let Some(mins) = s.strip_suffix('m') {
        let n: u64 = mins
            .trim()
            .parse()
            .map_err(|_| CoolError::invalid_argument("时长非法"))?;
        return Ok(Duration::from_secs(n * 60));
    }
    if let Some(hrs) = s.strip_suffix('h') {
        let n: u64 = hrs
            .trim()
            .parse()
            .map_err(|_| CoolError::invalid_argument("时长非法"))?;
        return Ok(Duration::from_secs(n * 3600));
    }
    // 纯数字按秒
    if let Ok(n) = s.parse::<u64>() {
        return Ok(Duration::from_secs(n));
    }
    Err(CoolError::invalid_argument(format!("无法解析时长: {s}")))
}

/// 计算下次运行时间（用于 nextRunTime 落库）
pub fn next_run_time(spec: &CronSpec, from: DateTime<Utc>) -> Option<DateTime<Utc>> {
    match spec {
        CronSpec::Schedule(s) => s.after(&from).next(),
        CronSpec::Every(d) => Some(from + chrono::Duration::from_std(*d).ok()?),
    }
}

struct JobHandle {
    cancel: CancellationToken,
}

static JOBS: Lazy<Mutex<HashMap<String, JobHandle>>> = Lazy::new(|| Mutex::new(HashMap::new()));

/// 停用任务
pub fn disable_task(id: &str) {
    if let Some(h) = JOBS.lock().unwrap().remove(id) {
        h.cancel.cancel();
        tracing::info!(target: "task", id, "job stopped");
    }
}

/// 启用任务：先停旧的再起新的（对齐 gcron Remove-then-Add）
/// 回调：`on_fire` 返回 Ok 记成功、Err 记失败
pub fn enable_task<F>(
    id: i64,
    spec: CronSpec,
    start_date: Option<DateTime<Utc>>,
    singleton: bool,
    on_fire: F,
) where
    F: Fn() -> futures_util::future::BoxFuture<'static, CoolResult<()>> + Send + Sync + 'static,
{
    let key = id.to_string();
    disable_task(&key);
    let cancel = CancellationToken::new();
    let token = cancel.clone();
    let running = Arc::new(AtomicBool::new(false));
    let on_fire = Arc::new(on_fire);

    tokio::spawn(async move {
        loop {
            let now = Utc::now();
            if let Some(sd) = start_date
                && now < sd
            {
                // 未到 startDate，睡到起点再评估
                let wait = (sd - now).to_std().unwrap_or(Duration::from_secs(1));
                tokio::select! {
                    _ = token.cancelled() => break,
                    _ = tokio::time::sleep(wait.min(Duration::from_secs(60))) => continue,
                }
            }

            let sleep_dur = match &spec {
                CronSpec::Every(d) => *d,
                CronSpec::Schedule(s) => s
                    .after(&now)
                    .next()
                    .map(|t| (t - now).to_std().unwrap_or(Duration::from_secs(1)))
                    .unwrap_or(Duration::from_secs(60)),
            };

            tokio::select! {
                _ = token.cancelled() => break,
                _ = tokio::time::sleep(sleep_dur) => {}
            }

            if singleton {
                if running.swap(true, Ordering::SeqCst) {
                    tracing::warn!(target: "task", id, "skip overlap (singleton)");
                    continue;
                }
                let r = on_fire().await;
                running.store(false, Ordering::SeqCst);
                if let Err(e) = r {
                    tracing::warn!(target: "task", id, "fire error: {e}");
                }
            } else if let Err(e) = on_fire().await {
                tracing::warn!(target: "task", id, "fire error: {e}");
            }
        }
    });

    JOBS.lock().unwrap().insert(key, JobHandle { cancel });
    tracing::info!(target: "task", id, "job enabled");
}

/// 当前调度中的任务 id 列表
pub fn active_jobs() -> Vec<String> {
    JOBS.lock().unwrap().keys().cloned().collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{TimeZone, Timelike};

    #[test]
    fn parse_every() {
        assert!(matches!(
            parse_cron("@every 5s").unwrap(),
            CronSpec::Every(_)
        ));
        assert!(matches!(
            parse_cron("@every 100ms").unwrap(),
            CronSpec::Every(_)
        ));
        assert!(matches!(
            parse_cron("@every 2m").unwrap(),
            CronSpec::Every(_)
        ));
    }

    #[test]
    fn parse_descriptors() {
        assert!(matches!(
            parse_cron("@hourly").unwrap(),
            CronSpec::Schedule(_)
        ));
        assert!(matches!(
            parse_cron("@daily").unwrap(),
            CronSpec::Schedule(_)
        ));
        assert!(matches!(
            parse_cron("@yearly").unwrap(),
            CronSpec::Schedule(_)
        ));
    }

    #[test]
    fn parse_six_field() {
        assert!(matches!(
            parse_cron("0 0 12 * * *").unwrap(),
            CronSpec::Schedule(_)
        ));
        assert!(matches!(
            parse_cron("1 2 3 * * *").unwrap(),
            CronSpec::Schedule(_)
        ));
    }

    #[test]
    fn parse_invalid() {
        assert!(parse_cron("").is_err());
        assert!(parse_cron("not-a-cron").is_err());
        assert!(parse_cron("@every xyz").is_err());
    }

    #[test]
    fn next_run_time_every() {
        let from = Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).unwrap();
        let n = next_run_time(&CronSpec::Every(Duration::from_secs(5)), from).unwrap();
        assert_eq!(n.timestamp() - from.timestamp(), 5);
    }

    #[test]
    fn next_run_time_hourly() {
        let from = Utc.with_ymd_and_hms(2026, 1, 1, 0, 30, 0).unwrap();
        let n = next_run_time(&parse_cron("@hourly").unwrap(), from).unwrap();
        assert_eq!(n.hour(), 1);
    }
}
