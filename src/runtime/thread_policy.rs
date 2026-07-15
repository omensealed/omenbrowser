use std::num::NonZeroUsize;

/// Upper bound chosen to preserve the measured high-core desktop behavior
/// without oversubscribing small systems.
pub const MAX_ASYNC_WORKERS: usize = 4;

fn async_worker_threads_for(available: usize) -> usize {
    available.clamp(1, MAX_ASYNC_WORKERS)
}

pub fn async_worker_threads() -> usize {
    async_worker_threads_for(
        std::thread::available_parallelism()
            .map(NonZeroUsize::get)
            .unwrap_or(1),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn worker_policy_never_oversubscribes_known_parallelism() {
        assert_eq!(async_worker_threads_for(0), 1);
        assert_eq!(async_worker_threads_for(1), 1);
        assert_eq!(async_worker_threads_for(2), 2);
        assert_eq!(async_worker_threads_for(4), 4);
        assert_eq!(async_worker_threads_for(64), 4);
    }

    #[test]
    fn detected_policy_is_nonzero_and_clamped() {
        let workers = async_worker_threads();
        assert!((1..=MAX_ASYNC_WORKERS).contains(&workers));
    }
}
