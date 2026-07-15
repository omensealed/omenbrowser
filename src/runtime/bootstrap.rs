//! Application-owned Tokio runtime construction.

use super::thread_policy;

pub const APP_ASYNC_THREAD_NAME: &str = "omen-main-async";
pub const MAX_BLOCKING_THREADS: usize = 8;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AppRuntimePolicy {
    pub async_worker_threads: usize,
    pub max_blocking_threads: usize,
    pub async_thread_name: &'static str,
}

/// Return the effective bootstrap policy for the current host.
pub fn app_runtime_policy() -> AppRuntimePolicy {
    AppRuntimePolicy {
        async_worker_threads: thread_policy::async_worker_threads(),
        max_blocking_threads: MAX_BLOCKING_THREADS,
        async_thread_name: APP_ASYNC_THREAD_NAME,
    }
}

/// Build the process runtime used by the compatibility entrypoint.
pub fn build_app_runtime() -> std::io::Result<tokio::runtime::Runtime> {
    let policy = app_runtime_policy();
    tokio::runtime::Builder::new_multi_thread()
        .thread_name(policy.async_thread_name)
        .worker_threads(policy.async_worker_threads)
        .max_blocking_threads(policy.max_blocking_threads)
        .enable_all()
        .build()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bootstrap_policy_preserves_reviewed_bounds_and_names() {
        let policy = app_runtime_policy();
        assert!((1..=thread_policy::MAX_ASYNC_WORKERS).contains(&policy.async_worker_threads));
        assert_eq!(policy.max_blocking_threads, 8);
        assert_eq!(policy.async_thread_name, "omen-main-async");
    }

    #[test]
    fn built_runtime_executes_on_the_named_worker_pool() {
        let runtime = build_app_runtime().expect("build application runtime");
        let thread_name = runtime.block_on(async {
            tokio::spawn(async {
                tokio::task::yield_now().await;
                std::thread::current().name().map(str::to_owned)
            })
            .await
            .expect("join named worker")
        });
        assert_eq!(thread_name.as_deref(), Some(APP_ASYNC_THREAD_NAME));
    }
}
