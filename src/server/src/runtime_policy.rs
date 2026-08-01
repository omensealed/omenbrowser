use crate::error::{ServerError, ServerResult};

pub(crate) const SERVER_ASYNC_WORKER_MAX: usize = 4;
pub(crate) const SERVER_BLOCKING_THREAD_MAX: usize = 8;
pub(crate) const HEADLESS_THREAD_NAME: &str = "omenchatd-headless";
#[cfg(any(feature = "tui", test))]
pub(crate) const TUI_THREAD_NAME: &str = "omenchatd-tui";

pub(crate) fn async_worker_count(available: usize) -> usize {
    available.clamp(1, SERVER_ASYNC_WORKER_MAX)
}

pub(crate) fn build_runtime(thread_name: &'static str) -> ServerResult<tokio::runtime::Runtime> {
    let available = std::thread::available_parallelism()
        .map(usize::from)
        .unwrap_or(1);
    tokio::runtime::Builder::new_multi_thread()
        .worker_threads(async_worker_count(available))
        .max_blocking_threads(SERVER_BLOCKING_THREAD_MAX)
        .thread_name(thread_name)
        .enable_all()
        .build()
        .map_err(|error| ServerError::Message(format!("tokio runtime failed: {error}")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn server_worker_policy_is_bounded_and_names_are_stable() {
        assert_eq!(async_worker_count(0), 1);
        assert_eq!(async_worker_count(1), 1);
        assert_eq!(async_worker_count(2), 2);
        assert_eq!(async_worker_count(64), 4);
        assert_eq!(SERVER_BLOCKING_THREAD_MAX, 8);
        assert_eq!(HEADLESS_THREAD_NAME, "omenchatd-headless");
        assert_eq!(TUI_THREAD_NAME, "omenchatd-tui");
    }

    #[test]
    fn server_runtime_executes_on_named_worker() {
        let runtime = build_runtime(HEADLESS_THREAD_NAME).expect("runtime");
        let name = runtime.block_on(async {
            tokio::spawn(async { std::thread::current().name().map(str::to_owned) })
                .await
                .expect("worker")
        });
        assert_eq!(name.as_deref(), Some(HEADLESS_THREAD_NAME));
    }
}
