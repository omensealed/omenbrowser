use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::{mpsc, Arc};
use std::time::{Duration, Instant};

use crate::error::{ServerError, ServerResult};
use crate::protocol::{RoomId, UserId};
use crate::store::{OmenchatStore, RoomHistoryUsage, ServerAdminUser, ServerRoom, ServerUser};

const ADMIN_DATABASE_QUEUE_ITEMS: usize = 16;
const ADMIN_DATABASE_OPEN_TIMEOUT: Duration = Duration::from_secs(30);
const ADMIN_DATABASE_RESPONSE_TIMEOUT: Duration = Duration::from_secs(6);

type AdminDatabaseJob = Box<dyn FnOnce(&OmenchatStore) + Send + 'static>;

#[derive(Default)]
struct AdminDatabaseMetrics {
    queued: AtomicUsize,
    in_flight: AtomicUsize,
    completed: AtomicU64,
    rejected: AtomicU64,
    total_micros: AtomicU64,
    max_micros: AtomicU64,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct AdminDatabaseMetricsSnapshot {
    pub queued: usize,
    pub in_flight: usize,
    pub completed: u64,
    pub rejected: u64,
    pub average_micros: u64,
    pub max_micros: u64,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct AdminUploadLedgerInspection {
    pub tracked_files: usize,
    pub tracked_bytes: u64,
    pub disk_files: usize,
    pub disk_bytes: u64,
    pub missing: usize,
    pub mismatched: usize,
    pub orphans: usize,
    pub unsafe_paths: usize,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct AdminUploadLedgerRepair {
    pub removed_missing: usize,
    pub removed_unsafe: usize,
    pub preserved_orphans: usize,
}

#[derive(Clone, Copy)]
enum AdminDatabaseOpenMode {
    Normal,
    ReadOnly,
    ExistingMaintenance,
}

/// Bounded, single-owner access to the administrative SQLite connection.
///
/// The handle never moves a `rusqlite::Connection` onto a caller thread. Work
/// is admitted with `try_send`, so overload is explicit and cannot create an
/// unbounded waiter or blocking-task queue.
#[derive(Clone)]
pub struct AdminDatabase {
    jobs: mpsc::SyncSender<AdminDatabaseJob>,
    metrics: Arc<AdminDatabaseMetrics>,
}

pub struct AdminDatabaseResponse<R> {
    receiver: mpsc::Receiver<ServerResult<R>>,
    deadline: Instant,
}

impl<R> AdminDatabaseResponse<R> {
    /// Poll a submitted operation without blocking the caller thread.
    pub fn try_recv(&self) -> Option<ServerResult<R>> {
        match self.receiver.try_recv() {
            Ok(result) => Some(result),
            Err(mpsc::TryRecvError::Empty) if Instant::now() < self.deadline => None,
            Err(mpsc::TryRecvError::Empty) => Some(Err(ServerError::Message(format!(
                "administrative database operation exceeded {} seconds",
                ADMIN_DATABASE_RESPONSE_TIMEOUT.as_secs()
            )))),
            Err(mpsc::TryRecvError::Disconnected) => Some(Err(ServerError::Message(
                "administrative database worker stopped before replying".into(),
            ))),
        }
    }
}

impl AdminDatabase {
    pub fn open(path: impl AsRef<Path>) -> ServerResult<Self> {
        Self::open_with_mode(path.as_ref(), AdminDatabaseOpenMode::Normal)
    }

    pub fn open_read_only(path: impl AsRef<Path>) -> ServerResult<Self> {
        Self::open_with_mode(path.as_ref(), AdminDatabaseOpenMode::ReadOnly)
    }

    pub fn open_existing_for_maintenance(path: impl AsRef<Path>) -> ServerResult<Self> {
        Self::open_with_mode(path.as_ref(), AdminDatabaseOpenMode::ExistingMaintenance)
    }

    fn open_with_mode(path: &Path, mode: AdminDatabaseOpenMode) -> ServerResult<Self> {
        let (jobs, receiver) = mpsc::sync_channel(ADMIN_DATABASE_QUEUE_ITEMS);
        let (ready_tx, ready_rx) = mpsc::sync_channel(1);
        let metrics = Arc::new(AdminDatabaseMetrics::default());
        let worker_metrics = metrics.clone();
        let path = path.to_path_buf();
        std::thread::Builder::new()
            .name("omenchatd-admin-db".into())
            .spawn(move || {
                let opened = match mode {
                    AdminDatabaseOpenMode::Normal => OmenchatStore::open(path),
                    AdminDatabaseOpenMode::ReadOnly => OmenchatStore::open_read_only(path),
                    AdminDatabaseOpenMode::ExistingMaintenance => {
                        OmenchatStore::open_existing_for_maintenance(path)
                    }
                };
                match opened {
                    Ok(store) => {
                        let _ = ready_tx.send(Ok(()));
                        run_worker(store, receiver, &worker_metrics);
                    }
                    Err(error) => {
                        let _ = ready_tx.send(Err(error));
                    }
                }
            })?;
        match ready_rx.recv_timeout(ADMIN_DATABASE_OPEN_TIMEOUT) {
            Ok(Ok(())) => Ok(Self { jobs, metrics }),
            Ok(Err(error)) => Err(error),
            Err(mpsc::RecvTimeoutError::Timeout) => Err(ServerError::Message(format!(
                "administrative database open exceeded {} seconds",
                ADMIN_DATABASE_OPEN_TIMEOUT.as_secs()
            ))),
            Err(mpsc::RecvTimeoutError::Disconnected) => Err(ServerError::Message(
                "administrative database worker stopped during open".into(),
            )),
        }
    }

    pub fn list_rooms(&self) -> ServerResult<Vec<ServerRoom>> {
        self.call(OmenchatStore::list_rooms)
    }

    pub fn request_list_rooms(&self) -> ServerResult<AdminDatabaseResponse<Vec<ServerRoom>>> {
        self.request(OmenchatStore::list_rooms)
    }

    pub fn create_room(&self, name: String, topic: Option<String>) -> ServerResult<ServerRoom> {
        self.call(move |store| store.create_room(&name, topic.as_deref()))
    }

    pub fn request_create_room(
        &self,
        name: String,
        topic: Option<String>,
    ) -> ServerResult<AdminDatabaseResponse<ServerRoom>> {
        self.request(move |store| store.create_room(&name, topic.as_deref()))
    }

    pub fn update_room_topic(
        &self,
        room_id: RoomId,
        topic: Option<String>,
    ) -> ServerResult<ServerRoom> {
        self.call(move |store| store.update_room_topic(room_id, topic.as_deref()))
    }

    pub fn request_update_room_topic(
        &self,
        room_id: RoomId,
        topic: Option<String>,
    ) -> ServerResult<AdminDatabaseResponse<ServerRoom>> {
        self.request(move |store| store.update_room_topic(room_id, topic.as_deref()))
    }

    pub fn archive_room(&self, room_id: RoomId) -> ServerResult<()> {
        self.call(move |store| store.archive_room(room_id))
    }

    pub fn advance_room_history_usage(&self, room_id: RoomId) -> ServerResult<RoomHistoryUsage> {
        self.call(move |store| store.advance_room_history_usage(room_id))
    }

    pub fn request_archive_room(&self, room_id: RoomId) -> ServerResult<AdminDatabaseResponse<()>> {
        self.request(move |store| store.archive_room(room_id))
    }

    pub fn list_users(&self) -> ServerResult<Vec<ServerAdminUser>> {
        self.call(OmenchatStore::administrative_users)
    }

    pub fn set_user_status_flag(
        &self,
        user_id: UserId,
        flag: u32,
        enabled: bool,
    ) -> ServerResult<ServerUser> {
        self.call(move |store| store.set_user_status_flag(user_id, flag, enabled))
    }

    pub fn set_user_role_bits(&self, user_id: UserId, role_bits: u64) -> ServerResult<ServerUser> {
        self.call(move |store| store.set_user_role_bits(user_id, role_bits))
    }

    pub fn set_user_role_flag(
        &self,
        user_id: UserId,
        flag: u64,
        enabled: bool,
    ) -> ServerResult<ServerUser> {
        self.call(move |store| store.set_user_role_flag(user_id, flag, enabled))
    }

    pub fn delete_users(&self, user_ids: Vec<UserId>) -> ServerResult<usize> {
        self.call(move |store| store.delete_users(&user_ids))
    }

    pub fn inspect_upload_ledgers(
        &self,
        upload_root: PathBuf,
    ) -> ServerResult<AdminUploadLedgerInspection> {
        self.call(move |store| inspect_upload_ledgers(store, &upload_root))
    }

    /// Run an explicitly confirmed offline repair to completion.
    ///
    /// Unlike interactive calls, this intentionally has no response timeout:
    /// returning a timeout while the owned worker later commits a repair would
    /// give the operator an ambiguous result.
    pub fn repair_upload_ledgers(
        &self,
        upload_root: PathBuf,
    ) -> ServerResult<AdminUploadLedgerRepair> {
        self.call_until_complete(move |store| repair_upload_ledgers(store, &upload_root))
    }

    pub fn request_list_users(&self) -> ServerResult<AdminDatabaseResponse<Vec<ServerAdminUser>>> {
        self.request(OmenchatStore::administrative_users)
    }

    pub fn request_set_user_status_flag(
        &self,
        user_id: UserId,
        flag: u32,
        enabled: bool,
    ) -> ServerResult<AdminDatabaseResponse<ServerUser>> {
        self.request(move |store| store.set_user_status_flag(user_id, flag, enabled))
    }

    pub fn request_set_user_role_bits(
        &self,
        user_id: UserId,
        role_bits: u64,
    ) -> ServerResult<AdminDatabaseResponse<ServerUser>> {
        self.request(move |store| store.set_user_role_bits(user_id, role_bits))
    }

    pub fn request_set_user_role_flag(
        &self,
        user_id: UserId,
        flag: u64,
        enabled: bool,
    ) -> ServerResult<AdminDatabaseResponse<ServerUser>> {
        self.request(move |store| store.set_user_role_flag(user_id, flag, enabled))
    }

    pub fn request_delete_users(
        &self,
        user_ids: Vec<UserId>,
    ) -> ServerResult<AdminDatabaseResponse<usize>> {
        self.request(move |store| store.delete_users(&user_ids))
    }

    pub fn metrics(&self) -> AdminDatabaseMetricsSnapshot {
        let completed = self.metrics.completed.load(Ordering::Relaxed);
        let total_micros = self.metrics.total_micros.load(Ordering::Relaxed);
        AdminDatabaseMetricsSnapshot {
            queued: self.metrics.queued.load(Ordering::Acquire),
            in_flight: self.metrics.in_flight.load(Ordering::Acquire),
            completed,
            rejected: self.metrics.rejected.load(Ordering::Relaxed),
            average_micros: total_micros.checked_div(completed).unwrap_or(0),
            max_micros: self.metrics.max_micros.load(Ordering::Relaxed),
        }
    }

    fn call<R, F>(&self, operation: F) -> ServerResult<R>
    where
        R: Send + 'static,
        F: FnOnce(&OmenchatStore) -> ServerResult<R> + Send + 'static,
    {
        let response = self.submit(operation)?;
        match response.recv_timeout(ADMIN_DATABASE_RESPONSE_TIMEOUT) {
            Ok(result) => result,
            Err(mpsc::RecvTimeoutError::Timeout) => Err(ServerError::Message(format!(
                "administrative database operation exceeded {} seconds",
                ADMIN_DATABASE_RESPONSE_TIMEOUT.as_secs()
            ))),
            Err(mpsc::RecvTimeoutError::Disconnected) => Err(ServerError::Message(
                "administrative database worker stopped before replying".into(),
            )),
        }
    }

    fn call_until_complete<R, F>(&self, operation: F) -> ServerResult<R>
    where
        R: Send + 'static,
        F: FnOnce(&OmenchatStore) -> ServerResult<R> + Send + 'static,
    {
        self.submit(operation)?.recv().map_err(|_| {
            ServerError::Message("administrative database worker stopped before replying".into())
        })?
    }

    fn request<R, F>(&self, operation: F) -> ServerResult<AdminDatabaseResponse<R>>
    where
        R: Send + 'static,
        F: FnOnce(&OmenchatStore) -> ServerResult<R> + Send + 'static,
    {
        Ok(AdminDatabaseResponse {
            receiver: self.submit(operation)?,
            deadline: Instant::now() + ADMIN_DATABASE_RESPONSE_TIMEOUT,
        })
    }

    fn submit<R, F>(&self, operation: F) -> ServerResult<mpsc::Receiver<ServerResult<R>>>
    where
        R: Send + 'static,
        F: FnOnce(&OmenchatStore) -> ServerResult<R> + Send + 'static,
    {
        let (response_tx, response_rx) = mpsc::sync_channel(1);
        let job = Box::new(move |store: &OmenchatStore| {
            let _ = response_tx.send(operation(store));
        });
        self.metrics.queued.fetch_add(1, Ordering::AcqRel);
        match self.jobs.try_send(job) {
            Ok(()) => Ok(response_rx),
            Err(mpsc::TrySendError::Full(_)) => {
                self.metrics.queued.fetch_sub(1, Ordering::AcqRel);
                self.metrics.rejected.fetch_add(1, Ordering::Relaxed);
                Err(ServerError::Message(
                    "administrative database queue is busy".into(),
                ))
            }
            Err(mpsc::TrySendError::Disconnected(_)) => {
                self.metrics.queued.fetch_sub(1, Ordering::AcqRel);
                self.metrics.rejected.fetch_add(1, Ordering::Relaxed);
                Err(ServerError::Message(
                    "administrative database worker is stopped".into(),
                ))
            }
        }
    }
}

fn inspect_upload_ledgers(
    store: &OmenchatStore,
    upload_root: &Path,
) -> ServerResult<AdminUploadLedgerInspection> {
    let mut aggregate = AdminUploadLedgerInspection::default();
    for user in store.users()? {
        let identity_dir =
            crate::upload::upload_identity_dir_for_root(upload_root, &user.identity_hash);
        let report = store.reconcile_upload_ledger(user.user_id, &identity_dir)?;
        aggregate.tracked_files = aggregate.tracked_files.saturating_add(report.tracked_files);
        aggregate.tracked_bytes = aggregate.tracked_bytes.saturating_add(report.tracked_bytes);
        aggregate.disk_files = aggregate.disk_files.saturating_add(report.disk_files);
        aggregate.disk_bytes = aggregate.disk_bytes.saturating_add(report.disk_bytes);
        aggregate.missing = aggregate.missing.saturating_add(report.missing_paths.len());
        aggregate.mismatched = aggregate
            .mismatched
            .saturating_add(report.mismatched_paths.len());
        aggregate.orphans = aggregate.orphans.saturating_add(report.orphan_paths.len());
        aggregate.unsafe_paths = aggregate
            .unsafe_paths
            .saturating_add(report.unsafe_paths.len());
    }
    Ok(aggregate)
}

fn repair_upload_ledgers(
    store: &OmenchatStore,
    upload_root: &Path,
) -> ServerResult<AdminUploadLedgerRepair> {
    let mut aggregate = AdminUploadLedgerRepair::default();
    for user in store.users()? {
        let identity_dir =
            crate::upload::upload_identity_dir_for_root(upload_root, &user.identity_hash);
        let repair = store.repair_upload_ledger_records(user.user_id, &identity_dir)?;
        aggregate.removed_missing = aggregate
            .removed_missing
            .saturating_add(repair.removed_missing_records);
        aggregate.removed_unsafe = aggregate
            .removed_unsafe
            .saturating_add(repair.removed_unsafe_records);
        aggregate.preserved_orphans = aggregate
            .preserved_orphans
            .saturating_add(repair.preserved_orphan_paths);
    }
    Ok(aggregate)
}

fn run_worker(
    store: OmenchatStore,
    receiver: mpsc::Receiver<AdminDatabaseJob>,
    metrics: &AdminDatabaseMetrics,
) {
    while let Ok(job) = receiver.recv() {
        metrics.queued.fetch_sub(1, Ordering::AcqRel);
        metrics.in_flight.store(1, Ordering::Release);
        let started = Instant::now();
        job(&store);
        let elapsed = started.elapsed().as_micros().try_into().unwrap_or(u64::MAX);
        metrics.total_micros.fetch_add(elapsed, Ordering::Relaxed);
        metrics.max_micros.fetch_max(elapsed, Ordering::Relaxed);
        metrics.completed.fetch_add(1, Ordering::Relaxed);
        metrics.in_flight.store(0, Ordering::Release);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Barrier;

    fn isolated_path(label: &str) -> std::path::PathBuf {
        std::env::temp_dir().join(format!(
            "omenchatd-admin-db-{label}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("clock")
                .as_nanos()
        ))
    }

    fn await_response<R>(response: &AdminDatabaseResponse<R>) -> ServerResult<R> {
        let deadline = Instant::now() + Duration::from_secs(2);
        loop {
            if let Some(result) = response.try_recv() {
                return result;
            }
            assert!(
                Instant::now() < deadline,
                "administrative database test response exceeded deadline"
            );
            std::thread::yield_now();
        }
    }

    fn await_completed(database: &AdminDatabase, expected: u64) {
        let deadline = Instant::now() + Duration::from_secs(2);
        while database.metrics().completed < expected {
            assert!(
                Instant::now() < deadline,
                "administrative database metrics did not settle"
            );
            std::thread::yield_now();
        }
    }

    #[test]
    fn typed_room_operations_run_on_owned_worker_connection() {
        let root = isolated_path("rooms");
        let config = crate::config::ServerConfig::for_root(root.clone());
        crate::config::init_files(&config).expect("initialize server home");
        let database = AdminDatabase::open(&config.database_path).expect("admin database");

        let room = database
            .create_room("ops".into(), Some("Operations".into()))
            .expect("create room");
        assert_eq!(room.name, "ops");
        let updated = database
            .update_room_topic(room.room_id, Some("Incidents".into()))
            .expect("update topic");
        assert_eq!(updated.topic.as_deref(), Some("Incidents"));
        database.archive_room(room.room_id).expect("archive room");
        assert!(!database
            .list_rooms()
            .expect("list rooms")
            .iter()
            .any(|candidate| candidate.room_id == room.room_id));
        await_completed(&database, 4);
        let metrics = database.metrics();
        assert_eq!(metrics.completed, 4);
        assert_eq!(metrics.rejected, 0);
        assert_eq!(metrics.queued, 0);
        assert_eq!(metrics.in_flight, 0);

        drop(database);
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn typed_user_operations_run_on_owned_worker_connection() {
        let root = isolated_path("users");
        let config = crate::config::ServerConfig::for_root(root.clone());
        crate::config::init_files(&config).expect("initialize server home");
        let store = OmenchatStore::open(&config.database_path).expect("seed store");
        let user = store
            .ensure_user(b"peer-a", "Alice", Some("lxmf-a"))
            .expect("seed user");
        drop(store);
        let database = AdminDatabase::open(&config.database_path).expect("admin database");

        let users = database.list_users().expect("list users");
        assert_eq!(users.len(), 1);
        assert_eq!(users[0].user.display_name, "Alice");
        let muted = await_response(
            &database
                .request_set_user_status_flag(user.user_id, 2, true)
                .expect("request mute"),
        )
        .expect("mute user");
        assert_eq!(muted.status_bits & 2, 2);
        let moderator = await_response(
            &database
                .request_set_user_role_bits(user.user_id, 2)
                .expect("request role"),
        )
        .expect("set role");
        assert_eq!(moderator.role_bits, 2);
        let trusted_moderator = await_response(
            &database
                .request_set_user_role_flag(user.user_id, 1, true)
                .expect("request trusted flag"),
        )
        .expect("set trusted flag");
        assert_eq!(trusted_moderator.role_bits, 3);
        let deleted = await_response(
            &database
                .request_delete_users(vec![user.user_id])
                .expect("request delete"),
        )
        .expect("delete user");
        assert_eq!(deleted, 1);
        assert!(database.list_users().expect("list after delete").is_empty());

        drop(database);
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn upload_ledger_inspection_is_read_only_and_confirmed_repair_uses_existing_mode() {
        let root = isolated_path("upload-maintenance");
        let config = crate::config::ServerConfig::for_root(root.clone());
        crate::config::init_files(&config).expect("initialize server home");
        let store = OmenchatStore::open(&config.database_path).expect("seed store");
        let user = store
            .ensure_user(b"peer-upload", "Uploader", None)
            .expect("seed user");
        let missing = crate::upload::upload_identity_dir(&config, b"peer-upload").join("gone.bin");
        store
            .record_upload_file(crate::store::RecordUploadFile {
                resource_id: "missing-resource",
                room_id: 1,
                actor_user_id: user.user_id,
                filename: "gone.bin",
                content_type: None,
                byte_len: 7,
                path: &missing,
            })
            .expect("seed missing ledger row");
        drop(store);

        let read_only = AdminDatabase::open_read_only(&config.database_path).expect("read-only");
        let inspection = read_only
            .inspect_upload_ledgers(config.upload_cache_path())
            .expect("inspect ledger");
        assert_eq!(inspection.tracked_files, 1);
        assert_eq!(inspection.missing, 1);
        assert!(read_only
            .set_user_status_flag(user.user_id, 1, true)
            .expect_err("read-only actor must reject mutation")
            .to_string()
            .contains("readonly"));
        drop(read_only);

        let maintenance = AdminDatabase::open_existing_for_maintenance(&config.database_path)
            .expect("maintenance actor");
        let repair = maintenance
            .repair_upload_ledgers(config.upload_cache_path())
            .expect("repair ledger");
        assert_eq!(repair.removed_missing, 1);
        assert_eq!(repair.removed_unsafe, 0);
        assert_eq!(repair.preserved_orphans, 0);
        let inspection = maintenance
            .inspect_upload_ledgers(config.upload_cache_path())
            .expect("inspect repaired ledger");
        assert_eq!(inspection.tracked_files, 0);
        assert_eq!(inspection.missing, 0);

        drop(maintenance);
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn bounded_queue_rejects_overload_without_waiter_growth() {
        let root = isolated_path("bounded");
        std::fs::create_dir_all(&root).expect("root");
        let database = AdminDatabase::open(root.join("omenchat.sqlite")).expect("database");
        let entered = Arc::new(Barrier::new(2));
        let release = Arc::new(Barrier::new(2));
        let entered_worker = entered.clone();
        let release_worker = release.clone();
        let first = database
            .submit(move |_| {
                entered_worker.wait();
                release_worker.wait();
                Ok(())
            })
            .expect("blocking job");
        entered.wait();

        let mut queued = Vec::new();
        for _ in 0..ADMIN_DATABASE_QUEUE_ITEMS {
            queued.push(database.submit(|_| Ok(())).expect("queue slot"));
        }
        let error = database
            .submit(|_| Ok(()))
            .expect_err("queue must reject overload");
        assert!(error.to_string().contains("queue is busy"));
        assert_eq!(database.metrics().queued, ADMIN_DATABASE_QUEUE_ITEMS);
        assert_eq!(database.metrics().in_flight, 1);

        release.wait();
        first.recv().expect("first reply").expect("first result");
        for response in queued {
            response
                .recv()
                .expect("queued reply")
                .expect("queued result");
        }
        let metrics = database.metrics();
        assert_eq!(metrics.completed, (ADMIN_DATABASE_QUEUE_ITEMS + 1) as u64);
        assert_eq!(metrics.rejected, 1);
        assert_eq!(metrics.queued, 0);
        assert_eq!(metrics.in_flight, 0);

        drop(database);
        let _ = std::fs::remove_dir_all(root);
    }
}
