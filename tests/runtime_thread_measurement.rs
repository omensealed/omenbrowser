use std::path::PathBuf;
use std::time::Instant;

use omenbrowser_rs::storage::files::atomic_write_new_bounded;

const ASYNC_TASKS: usize = 5_000;
const BLOCKING_WRITES: usize = 32;
const BLOCKING_WRITE_BYTES: usize = 256 * 1024;

fn temp_dir(label: &str) -> PathBuf {
    let path = std::env::temp_dir().join(format!(
        "omenbrowser-runtime-measure-{label}-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&path);
    std::fs::create_dir_all(&path).expect("measurement root");
    path
}

fn percentile(samples: &mut [u64], percentile: usize) -> u64 {
    samples.sort_unstable();
    samples[(samples.len() * percentile).div_ceil(100) - 1]
}

fn measure_runtime(label: &str, worker_threads: Option<usize>, blocking_threads: Option<usize>) {
    let mut builder = tokio::runtime::Builder::new_multi_thread();
    builder.enable_all().thread_name(format!("measure-{label}"));
    if let Some(worker_threads) = worker_threads {
        builder.worker_threads(worker_threads);
    }
    if let Some(blocking_threads) = blocking_threads {
        builder.max_blocking_threads(blocking_threads);
    }
    let runtime = builder.build().expect("measurement runtime");
    let workers = runtime.metrics().num_workers();
    let root = temp_dir(label);
    let (mut task_latency, queued_depth, blocking_elapsed) = runtime.block_on(async {
        let mut tasks = Vec::with_capacity(ASYNC_TASKS);
        for _ in 0..ASYNC_TASKS {
            let queued = Instant::now();
            tasks.push(tokio::spawn(async move {
                tokio::task::yield_now().await;
                queued.elapsed().as_nanos() as u64
            }));
        }
        let queued_depth = tokio::runtime::Handle::current()
            .metrics()
            .global_queue_depth();
        let mut task_latency = Vec::with_capacity(ASYNC_TASKS);
        for task in tasks {
            task_latency.push(task.await.expect("measurement task"));
        }

        let started = Instant::now();
        let mut writes = tokio::task::JoinSet::new();
        for index in 0..BLOCKING_WRITES {
            let path = root.join(format!("write-{index}.bin"));
            writes.spawn(atomic_write_new_bounded(
                path,
                vec![index as u8; BLOCKING_WRITE_BYTES],
            ));
        }
        while let Some(write) = writes.join_next().await {
            write
                .expect("blocking measurement task")
                .expect("blocking measurement write");
        }
        (task_latency, queued_depth, started.elapsed())
    });
    let median = percentile(&mut task_latency, 50);
    let p95 = percentile(&mut task_latency, 95);
    println!("runtime_policy={label}");
    println!("runtime_workers={workers}");
    println!("async_tasks={ASYNC_TASKS}");
    println!("async_global_queue_depth_after_spawn={queued_depth}");
    println!("async_latency_median_ns={median}");
    println!("async_latency_p95_ns={p95}");
    println!("bounded_blocking_writes={BLOCKING_WRITES}");
    println!("bounded_blocking_write_bytes={BLOCKING_WRITE_BYTES}");
    println!(
        "bounded_blocking_elapsed_ns={}",
        blocking_elapsed.as_nanos()
    );
    std::fs::remove_dir_all(root).expect("measurement cleanup");
}

#[test]
#[ignore = "release-mode runtime thread measurement"]
fn measure_runtime_thread_policies() {
    println!(
        "available_parallelism={}",
        std::thread::available_parallelism().map_or(1, std::num::NonZeroUsize::get)
    );
    measure_runtime("legacy-fixed-4-workers-8-blocking", Some(4), Some(8));
    measure_runtime(
        "adaptive-max-4-workers-8-blocking",
        Some(omenbrowser_rs::runtime::thread_policy::async_worker_threads()),
        Some(8),
    );
    measure_runtime("tokio-host-default", None, None);
}
