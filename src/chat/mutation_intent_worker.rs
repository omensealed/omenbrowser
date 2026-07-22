use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{mpsc, Arc};

use omenchat_protocol::MutationId;

use super::mutation_intents::{
    queued_prepare_bytes, IntentTransition, MutationIntentStore, OutboundMutationIntent,
    OutboundMutationState, OwnedPrepareOutboundMutation,
};

pub const MUTATION_INTENT_WORKER_QUEUE_ITEMS: usize = 32;
pub const MUTATION_INTENT_WORKER_QUEUE_BYTES: usize = 2 * 1024 * 1024;

pub type IntentWorkerReply<T> = mpsc::Receiver<anyhow::Result<T>>;

pub async fn await_intent_worker_reply<T: Send + 'static>(
    reply: IntentWorkerReply<T>,
) -> anyhow::Result<T> {
    tokio::task::spawn_blocking(move || {
        reply
            .recv()
            .map_err(|_| anyhow::anyhow!("OMENchat mutation intent worker dropped its reply"))?
    })
    .await
    .map_err(|error| anyhow::anyhow!("OMENchat mutation intent reply task failed: {error}"))?
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct IntentWorkerMetrics {
    pub queued: usize,
    pub rejected: usize,
    pub completed: usize,
    pub queued_bytes: usize,
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum IntentWorkerSubmitError {
    #[error("OMENchat mutation intent worker queue is full")]
    Full,
    #[error("OMENchat mutation intent worker is closed")]
    Closed,
    #[error("OMENchat mutation intent worker byte budget is exhausted")]
    ByteBudget,
    #[error("OMENchat mutation intent request is invalid or oversized")]
    Invalid,
}

#[derive(Default)]
struct WorkerCounters {
    queued: AtomicUsize,
    rejected: AtomicUsize,
    completed: AtomicUsize,
    queued_bytes: AtomicUsize,
}

struct QueuedCommand {
    command: WorkerCommand,
    bytes: usize,
}

enum WorkerCommand {
    Prepare {
        request: OwnedPrepareOutboundMutation,
        reply: mpsc::SyncSender<anyhow::Result<OutboundMutationIntent>>,
    },
    Transition {
        mutation_id: MutationId,
        expected: OutboundMutationState,
        next: OutboundMutationState,
        reply: mpsc::SyncSender<anyhow::Result<IntentTransition>>,
    },
    Recover {
        reply: mpsc::SyncSender<anyhow::Result<Vec<OutboundMutationIntent>>>,
    },
    PruneTerminal {
        now: i64,
        reply: mpsc::SyncSender<anyhow::Result<usize>>,
    },
    #[cfg(test)]
    Pause {
        entered: mpsc::SyncSender<()>,
        release: mpsc::Receiver<()>,
    },
    Shutdown,
}

pub struct MutationIntentWorker {
    sender: Option<mpsc::SyncSender<QueuedCommand>>,
    counters: Arc<WorkerCounters>,
    thread: Option<std::thread::JoinHandle<()>>,
}

impl MutationIntentWorker {
    pub fn start(identity_storage_root: impl AsRef<Path>) -> anyhow::Result<Self> {
        Self::start_with_capacity(
            identity_storage_root.as_ref().to_path_buf(),
            MUTATION_INTENT_WORKER_QUEUE_ITEMS,
        )
    }

    fn start_with_capacity(
        identity_storage_root: PathBuf,
        capacity: usize,
    ) -> anyhow::Result<Self> {
        if capacity == 0 || capacity > MUTATION_INTENT_WORKER_QUEUE_ITEMS {
            anyhow::bail!("OMENchat mutation intent worker capacity is invalid");
        }
        let (sender, receiver) = mpsc::sync_channel::<QueuedCommand>(capacity);
        let (ready_sender, ready_receiver) = mpsc::sync_channel(1);
        let counters = Arc::new(WorkerCounters::default());
        let thread_counters = Arc::clone(&counters);
        let thread = std::thread::Builder::new()
            .name("omenchat-intent-store".into())
            .spawn(move || {
                let store = match MutationIntentStore::open_for_identity_storage_root(
                    identity_storage_root,
                ) {
                    Ok(store) => {
                        let _ = ready_sender.send(Ok(()));
                        store
                    }
                    Err(error) => {
                        let _ = ready_sender.send(Err(error));
                        return;
                    }
                };
                while let Ok(queued) = receiver.recv() {
                    if matches!(&queued.command, WorkerCommand::Shutdown) {
                        break;
                    }
                    thread_counters.queued.fetch_sub(1, Ordering::AcqRel);
                    thread_counters
                        .queued_bytes
                        .fetch_sub(queued.bytes, Ordering::AcqRel);
                    match queued.command {
                        WorkerCommand::Prepare { request, reply } => {
                            let result = store.persist_prepared(request.as_borrowed());
                            thread_counters.completed.fetch_add(1, Ordering::Relaxed);
                            let _ = reply.send(result);
                        }
                        WorkerCommand::Transition {
                            mutation_id,
                            expected,
                            next,
                            reply,
                        } => {
                            let result = store.transition(mutation_id, expected, next);
                            thread_counters.completed.fetch_add(1, Ordering::Relaxed);
                            let _ = reply.send(result);
                        }
                        WorkerCommand::Recover { reply } => {
                            let result = store.recover_nonterminal();
                            thread_counters.completed.fetch_add(1, Ordering::Relaxed);
                            let _ = reply.send(result);
                        }
                        WorkerCommand::PruneTerminal { now, reply } => {
                            let result = store.prune_terminal(now);
                            thread_counters.completed.fetch_add(1, Ordering::Relaxed);
                            let _ = reply.send(result);
                        }
                        #[cfg(test)]
                        WorkerCommand::Pause { entered, release } => {
                            let _ = entered.send(());
                            let _ = release.recv();
                            thread_counters.completed.fetch_add(1, Ordering::Relaxed);
                        }
                        WorkerCommand::Shutdown => unreachable!(),
                    }
                }
            })?;
        match ready_receiver.recv() {
            Ok(Ok(())) => Ok(Self {
                sender: Some(sender),
                counters,
                thread: Some(thread),
            }),
            Ok(Err(error)) => {
                let _ = thread.join();
                Err(error)
            }
            Err(_) => {
                let _ = thread.join();
                Err(anyhow::anyhow!(
                    "OMENchat mutation intent worker exited during startup"
                ))
            }
        }
    }

    pub fn try_prepare(
        &self,
        request: OwnedPrepareOutboundMutation,
    ) -> Result<IntentWorkerReply<OutboundMutationIntent>, IntentWorkerSubmitError> {
        let bytes = queued_prepare_bytes(&request).map_err(|_| IntentWorkerSubmitError::Invalid)?;
        let (reply, receive) = mpsc::sync_channel(1);
        self.try_submit(WorkerCommand::Prepare { request, reply }, bytes)?;
        Ok(receive)
    }

    pub fn try_transition(
        &self,
        mutation_id: MutationId,
        expected: OutboundMutationState,
        next: OutboundMutationState,
    ) -> Result<IntentWorkerReply<IntentTransition>, IntentWorkerSubmitError> {
        let (reply, receive) = mpsc::sync_channel(1);
        self.try_submit(
            WorkerCommand::Transition {
                mutation_id,
                expected,
                next,
                reply,
            },
            128,
        )?;
        Ok(receive)
    }

    pub fn try_recover(
        &self,
    ) -> Result<IntentWorkerReply<Vec<OutboundMutationIntent>>, IntentWorkerSubmitError> {
        let (reply, receive) = mpsc::sync_channel(1);
        self.try_submit(WorkerCommand::Recover { reply }, 64)?;
        Ok(receive)
    }

    pub fn try_prune_terminal(
        &self,
        now: i64,
    ) -> Result<IntentWorkerReply<usize>, IntentWorkerSubmitError> {
        let (reply, receive) = mpsc::sync_channel(1);
        self.try_submit(WorkerCommand::PruneTerminal { now, reply }, 64)?;
        Ok(receive)
    }

    pub fn metrics(&self) -> IntentWorkerMetrics {
        IntentWorkerMetrics {
            queued: self.counters.queued.load(Ordering::Relaxed),
            rejected: self.counters.rejected.load(Ordering::Relaxed),
            completed: self.counters.completed.load(Ordering::Relaxed),
            queued_bytes: self.counters.queued_bytes.load(Ordering::Relaxed),
        }
    }

    pub fn shutdown(mut self) -> anyhow::Result<()> {
        self.shutdown_inner()
    }

    fn try_submit(
        &self,
        command: WorkerCommand,
        bytes: usize,
    ) -> Result<(), IntentWorkerSubmitError> {
        let sender = self
            .sender
            .as_ref()
            .ok_or(IntentWorkerSubmitError::Closed)?;
        let mut current_bytes = self.counters.queued_bytes.load(Ordering::Acquire);
        loop {
            let Some(next_bytes) = current_bytes.checked_add(bytes) else {
                self.counters.rejected.fetch_add(1, Ordering::Relaxed);
                return Err(IntentWorkerSubmitError::ByteBudget);
            };
            if next_bytes > MUTATION_INTENT_WORKER_QUEUE_BYTES {
                self.counters.rejected.fetch_add(1, Ordering::Relaxed);
                return Err(IntentWorkerSubmitError::ByteBudget);
            }
            match self.counters.queued_bytes.compare_exchange_weak(
                current_bytes,
                next_bytes,
                Ordering::AcqRel,
                Ordering::Acquire,
            ) {
                Ok(_) => break,
                Err(actual) => current_bytes = actual,
            }
        }
        self.counters.queued.fetch_add(1, Ordering::AcqRel);
        match sender.try_send(QueuedCommand { command, bytes }) {
            Ok(()) => Ok(()),
            Err(mpsc::TrySendError::Full(_)) => {
                self.counters.queued.fetch_sub(1, Ordering::AcqRel);
                self.counters
                    .queued_bytes
                    .fetch_sub(bytes, Ordering::AcqRel);
                self.counters.rejected.fetch_add(1, Ordering::Relaxed);
                Err(IntentWorkerSubmitError::Full)
            }
            Err(mpsc::TrySendError::Disconnected(_)) => {
                self.counters.queued.fetch_sub(1, Ordering::AcqRel);
                self.counters
                    .queued_bytes
                    .fetch_sub(bytes, Ordering::AcqRel);
                self.counters.rejected.fetch_add(1, Ordering::Relaxed);
                Err(IntentWorkerSubmitError::Closed)
            }
        }
    }

    fn shutdown_inner(&mut self) -> anyhow::Result<()> {
        if let Some(sender) = self.sender.take() {
            let _ = sender.send(QueuedCommand {
                command: WorkerCommand::Shutdown,
                bytes: 0,
            });
        }
        if let Some(thread) = self.thread.take() {
            thread
                .join()
                .map_err(|_| anyhow::anyhow!("OMENchat mutation intent worker panicked"))?;
        }
        Ok(())
    }

    #[cfg(test)]
    fn pause(&self) -> (mpsc::Receiver<()>, mpsc::SyncSender<()>) {
        let (entered_sender, entered_receiver) = mpsc::sync_channel(1);
        let (release_sender, release_receiver) = mpsc::sync_channel(1);
        self.try_submit(
            WorkerCommand::Pause {
                entered: entered_sender,
                release: release_receiver,
            },
            1,
        )
        .expect("pause admission");
        (entered_receiver, release_sender)
    }
}

impl Drop for MutationIntentWorker {
    fn drop(&mut self) {
        let _ = self.shutdown_inner();
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicU64, Ordering};

    use omenchat_protocol::{ChatOp, ClientInstanceId, FrameBody};

    use super::*;

    static ROOT_SEQUENCE: AtomicU64 = AtomicU64::new(1);

    fn isolated_root(label: &str) -> PathBuf {
        let root = std::env::temp_dir().join(format!(
            "omenbrowser-intent-worker-{label}-{}-{}",
            std::process::id(),
            ROOT_SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).expect("identity root");
        root
    }

    fn request() -> OwnedPrepareOutboundMutation {
        OwnedPrepareOutboundMutation {
            server_destination: "0123456789abcdef".into(),
            authenticated_identity_hash: b"authenticated-peer".to_vec(),
            client_instance_id: ClientInstanceId::new([7; 16]),
            op: ChatOp::RoomMessage,
            room_id: Some(9),
            body: FrameBody::Text("hello".into()),
            created_at: 100,
            expires_at: 200,
            correlation_id: Some("local-message-1".into()),
        }
    }

    #[tokio::test]
    async fn admitted_reply_is_awaited_off_the_async_worker() {
        let root = isolated_root("async-reply");
        let worker = MutationIntentWorker::start(&root).expect("worker");
        let reply = worker.try_prepare(request()).expect("prepare admitted");

        let intent = await_intent_worker_reply(reply).await.expect("reply");

        assert_eq!(intent.state, OutboundMutationState::Prepared);
        worker.shutdown().expect("shutdown");
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn owned_worker_persists_transitions_recovers_and_joins() {
        let root = isolated_root("lifecycle");
        let worker = MutationIntentWorker::start(&root).expect("worker");
        let intent = worker
            .try_prepare(request())
            .expect("prepare admission")
            .recv()
            .expect("prepare reply")
            .expect("prepare result");
        let transitioned = worker
            .try_transition(
                intent.mutation_id,
                OutboundMutationState::Prepared,
                OutboundMutationState::SentUncertain,
            )
            .expect("transition admission")
            .recv()
            .expect("transition reply")
            .expect("transition result");
        assert!(matches!(
            transitioned,
            IntentTransition::Updated(OutboundMutationIntent {
                state: OutboundMutationState::SentUncertain,
                ..
            })
        ));
        let recovered = worker
            .try_recover()
            .expect("recover admission")
            .recv()
            .expect("recover reply")
            .expect("recover result");
        assert_eq!(recovered.len(), 1);
        assert_eq!(recovered[0].state, OutboundMutationState::SentUncertain);
        assert_eq!(worker.metrics().completed, 3);
        worker.shutdown().expect("joined shutdown");

        let reopened = MutationIntentStore::open_for_identity_storage_root(&root).expect("reopen");
        assert_eq!(
            reopened
                .load(intent.mutation_id)
                .expect("load")
                .expect("intent")
                .state,
            OutboundMutationState::SentUncertain
        );
        drop(reopened);
        std::fs::remove_dir_all(root).expect("cleanup");
    }

    #[test]
    fn bounded_worker_rejects_overload_and_recovers_after_release() {
        let root = isolated_root("overload");
        let worker = MutationIntentWorker::start_with_capacity(root.clone(), 1).expect("worker");
        let (entered, release) = worker.pause();
        entered.recv().expect("worker paused");
        let queued = worker.try_recover().expect("one queued request");
        assert_eq!(
            worker.try_recover().expect_err("queue must reject"),
            IntentWorkerSubmitError::Full
        );
        assert_eq!(worker.metrics().queued, 1);
        assert_eq!(worker.metrics().queued_bytes, 64);
        assert_eq!(worker.metrics().rejected, 1);
        release.send(()).expect("release worker");
        assert!(queued
            .recv()
            .expect("queued reply")
            .expect("queued result")
            .is_empty());
        worker.shutdown().expect("shutdown");
        std::fs::remove_dir_all(root).expect("cleanup");
    }

    #[test]
    fn oversized_prepare_is_rejected_before_queue_admission() {
        let root = isolated_root("oversized");
        let worker = MutationIntentWorker::start(&root).expect("worker");
        let mut oversized = request();
        oversized.body = FrameBody::Text("x".repeat(65 * 1024));
        assert_eq!(
            worker
                .try_prepare(oversized)
                .expect_err("oversized request"),
            IntentWorkerSubmitError::Invalid
        );
        assert_eq!(worker.metrics().queued, 0);
        assert_eq!(worker.metrics().queued_bytes, 0);
        worker.shutdown().expect("shutdown");
        std::fs::remove_dir_all(root).expect("cleanup");
    }
}
