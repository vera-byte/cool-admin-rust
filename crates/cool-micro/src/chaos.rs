//! 资源耗尽混沌注入（测试用）。
//!
//! 环境变量：
//! - `COOL_TEST_CHAOS_CPU_THREADS=N`：启动 N 个忙循环线程吃满 CPU
//! - `COOL_TEST_CHAOS_MEM_MB=N`：分配 N MB 内存制造内存压力
//!
//! 用于模拟实例 CPU/内存打满，验证熔断降级。**生产环境勿开启。**

/// 请求级阻塞延迟（`COOL_TEST_CHAOS_BLOCK_MS`）：在 handler 内同步 sleep，
/// 占满 tokio worker 线程，模拟「线程池耗尽 / CPU 打满」导致的降级。
pub fn block_ms_from_env() -> u64 {
    std::env::var("COOL_TEST_CHAOS_BLOCK_MS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(0)
}

/// 按环境变量启动资源压力（后台线程）
pub fn spawn_resource_pressure() {
    let cpu_threads: usize = std::env::var("COOL_TEST_CHAOS_CPU_THREADS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(0);
    let mem_mb: usize = std::env::var("COOL_TEST_CHAOS_MEM_MB")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(0);

    if cpu_threads == 0 && mem_mb == 0 {
        return;
    }

    tracing::warn!(
        cpu_threads,
        mem_mb,
        "COOL_TEST_CHAOS enabled: resource exhaustion injection"
    );

    // CPU 压力：忙循环
    for i in 0..cpu_threads {
        std::thread::spawn(move || {
            tracing::info!(thread = i, "chaos cpu hog started");
            let mut x = 1u64;
            loop {
                // 故意做无意义计算吃 CPU
                x = x.wrapping_mul(6364136223846793005).wrapping_add(1);
                std::hint::black_box(x);
            }
        });
    }

    // 内存压力：分配并持有
    if mem_mb > 0 {
        std::thread::spawn(move || {
            tracing::info!(mem_mb, "chaos memory hog started");
            let mut hold: Vec<Vec<u8>> = Vec::new();
            for _ in 0..mem_mb {
                // 每块 1MB，写入字节防止优化掉
                let mut block = vec![0u8; 1024 * 1024];
                for (i, b) in block.iter_mut().enumerate() {
                    *b = (i % 256) as u8;
                }
                hold.push(block);
                // 小憩让出 CPU，模拟渐进式 OOM
                if hold.len().is_multiple_of(50) {
                    std::thread::sleep(std::time::Duration::from_millis(10));
                }
            }
            // 持有不释放
            std::thread::sleep(std::time::Duration::from_secs(u64::MAX));
            drop(hold);
        });
    }
}
