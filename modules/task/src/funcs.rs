//! 内置函数（对齐 modules/task/funcs/*）

use std::sync::Arc;

use cool_core::CoolResult;
use cool_func::CoolFunc;
use sqlx::MySqlPool;

use crate::service::TaskInfoService;

/// TaskStartFunc(id)：status=1 并启用调度
struct TaskStartFunc {
    svc: Arc<TaskInfoService>,
}

#[async_trait::async_trait]
impl CoolFunc for TaskStartFunc {
    async fn func(&self, param: &str) -> CoolResult<()> {
        self.svc
            .start_task(param.trim().parse().unwrap_or_default())
            .await
    }
    fn is_all_worker(&self) -> bool {
        true
    }
}

/// TaskStopFunc(id)：status=0 并停用调度
struct TaskStopFunc {
    svc: Arc<TaskInfoService>,
}

#[async_trait::async_trait]
impl CoolFunc for TaskStopFunc {
    async fn func(&self, param: &str) -> CoolResult<()> {
        self.svc
            .stop_task(param.trim().parse().unwrap_or_default())
            .await
    }
    fn is_all_worker(&self) -> bool {
        true
    }
}

/// TaskAddTask(id)：加载进调度器（不改 status）
struct TaskAddTask {
    svc: Arc<TaskInfoService>,
}

#[async_trait::async_trait]
impl CoolFunc for TaskAddTask {
    async fn func(&self, param: &str) -> CoolResult<()> {
        self.svc
            .add_task(param.trim().parse().unwrap_or_default())
            .await
    }
    fn is_all_worker(&self) -> bool {
        true
    }
}

/// TaskTest(param)：演示函数
struct TaskTest;

#[async_trait::async_trait]
impl CoolFunc for TaskTest {
    async fn func(&self, param: &str) -> CoolResult<()> {
        tracing::info!(target: "task", %param, "TaskTest executed");
        Ok(())
    }
    fn is_all_worker(&self) -> bool {
        true
    }
}

/// 注册内置函数；pool 用于构建共享 TaskInfoService
pub fn register_builtins(pool: MySqlPool) {
    let svc = Arc::new(TaskInfoService::new(pool));
    cool_func::register(
        "TaskStartFunc",
        Arc::new(TaskStartFunc { svc: svc.clone() }),
    );
    cool_func::register("TaskStopFunc", Arc::new(TaskStopFunc { svc: svc.clone() }));
    cool_func::register("TaskAddTask", Arc::new(TaskAddTask { svc: svc.clone() }));
    cool_func::register("TaskTest", Arc::new(TaskTest));
    // 把共享 service 存起来供 handler 用
    SERVICE.set(svc).ok();
}

static SERVICE: once_cell::sync::OnceCell<Arc<TaskInfoService>> = once_cell::sync::OnceCell::new();

/// 获取共享 TaskInfoService
pub fn shared_service() -> Option<Arc<TaskInfoService>> {
    SERVICE.get().cloned()
}
