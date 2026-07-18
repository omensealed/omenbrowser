use std::collections::BTreeMap;
use std::fs::{File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, LazyLock};

use rand_core::RngCore;
use serde::{Deserialize, Serialize};
use tokio::sync::{Mutex, Semaphore};

use crate::error::{AppError, AppResult};
use crate::messaging::NativeLxmfReplyTicket;
use crate::storage::files::atomic_replace;

pub const LXMF_TICKET_BYTES: usize = 16;
pub const LXMF_TICKET_EXPIRY_SECS: f64 = 21.0 * 24.0 * 60.0 * 60.0;
pub const LXMF_TICKET_RENEW_SECS: f64 = 14.0 * 24.0 * 60.0 * 60.0;
pub const LXMF_TICKET_INTERVAL_SECS: f64 = 24.0 * 60.0 * 60.0;
pub const LXMF_TICKET_GRACE_SECS: f64 = 5.0 * 24.0 * 60.0 * 60.0;
const ISSUED_TICKET_MAX_PEERS: usize = 256;
const ISSUED_TICKET_FILE_MAX_BYTES: u64 = 128 * 1024;
const ISSUED_TICKET_BLOCKING_JOBS: usize = 2;
const ISSUED_TICKET_FILE_NAME: &str = "omen_lxmf_issued_tickets.json";
const ISSUED_TICKET_STATE_VERSION: u8 = 1;

static ISSUED_TICKET_BLOCKING_GATE: LazyLock<Arc<Semaphore>> =
    LazyLock::new(|| Arc::new(Semaphore::new(ISSUED_TICKET_BLOCKING_JOBS)));
static ISSUED_TICKET_DECISION_LOCK: LazyLock<Mutex<()>> = LazyLock::new(|| Mutex::new(()));
static ISSUED_TICKET_TEMP_SEQUENCE: AtomicU64 = AtomicU64::new(1);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NativeLxmfTicketIssueState {
    NotRequested,
    IncludedNew,
    IncludedReused,
    SuppressedInterval,
}

impl NativeLxmfTicketIssueState {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::NotRequested => "not_requested",
            Self::IncludedNew => "included_new",
            Self::IncludedReused => "included_reused",
            Self::SuppressedInterval => "suppressed_interval",
        }
    }
}

#[derive(Clone, PartialEq)]
pub struct NativeLxmfTicketIssueDecision {
    pub state: NativeLxmfTicketIssueState,
    pub ticket: Option<NativeLxmfReplyTicket>,
}

impl std::fmt::Debug for NativeLxmfTicketIssueDecision {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("NativeLxmfTicketIssueDecision")
            .field("state", &self.state)
            .field("ticket", &self.ticket.as_ref().map(|_| "<redacted>"))
            .finish()
    }
}

#[derive(Clone)]
pub struct NativeLxmfTicketIssuer {
    path: PathBuf,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct IssuedTicketEntry {
    ticket: Vec<u8>,
    expires: f64,
    last_included_at: f64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct IssuedTicketState {
    version: u8,
    peers: BTreeMap<String, IssuedTicketEntry>,
}

impl Default for IssuedTicketState {
    fn default() -> Self {
        Self {
            version: ISSUED_TICKET_STATE_VERSION,
            peers: BTreeMap::new(),
        }
    }
}

impl NativeLxmfTicketIssuer {
    pub fn new(storage_root: &Path) -> Self {
        Self {
            path: storage_root.join(ISSUED_TICKET_FILE_NAME),
        }
    }

    pub async fn prepare(
        &self,
        peer_hash: &str,
        requested: bool,
        now: f64,
    ) -> AppResult<NativeLxmfTicketIssueDecision> {
        if !requested {
            return Ok(NativeLxmfTicketIssueDecision {
                state: NativeLxmfTicketIssueState::NotRequested,
                ticket: None,
            });
        }
        validate_peer_hash(peer_hash)?;
        let peer_hash = peer_hash.to_ascii_lowercase();
        if !now.is_finite() || now < 0.0 {
            return Err(AppError::Runtime(
                "LXMF ticket issue time is invalid".into(),
            ));
        }

        let _decision = ISSUED_TICKET_DECISION_LOCK.lock().await;
        let permit = ISSUED_TICKET_BLOCKING_GATE
            .clone()
            .acquire_owned()
            .await
            .map_err(|_| AppError::Runtime("LXMF ticket persistence gate closed".into()))?;
        let path = self.path.clone();
        tokio::task::spawn_blocking(move || {
            let _permit = permit;
            prepare_blocking(&path, &peer_hash, now)
        })
        .await
        .map_err(|error| {
            AppError::Runtime(format!("LXMF ticket persistence task failed: {error}"))
        })?
    }
}

fn prepare_blocking(
    path: &Path,
    peer_hash: &str,
    now: f64,
) -> AppResult<NativeLxmfTicketIssueDecision> {
    let mut state = load_state(path)?;
    state
        .peers
        .retain(|_, entry| entry.expires + LXMF_TICKET_GRACE_SECS >= now);

    if let Some(entry) = state.peers.get(peer_hash) {
        let elapsed = now - entry.last_included_at;
        if elapsed < LXMF_TICKET_INTERVAL_SECS {
            return Ok(NativeLxmfTicketIssueDecision {
                state: NativeLxmfTicketIssueState::SuppressedInterval,
                ticket: None,
            });
        }
    }

    let (ticket, issue_state) = match state.peers.get(peer_hash) {
        Some(entry) if entry.expires - now > LXMF_TICKET_RENEW_SECS => (
            NativeLxmfReplyTicket {
                ticket: entry.ticket.clone(),
                expires: entry.expires,
            },
            NativeLxmfTicketIssueState::IncludedReused,
        ),
        _ => {
            let mut bytes = vec![0_u8; LXMF_TICKET_BYTES];
            rand_core::OsRng.fill_bytes(&mut bytes);
            (
                NativeLxmfReplyTicket {
                    ticket: bytes,
                    expires: now + LXMF_TICKET_EXPIRY_SECS,
                },
                NativeLxmfTicketIssueState::IncludedNew,
            )
        }
    };

    if !state.peers.contains_key(peer_hash) && state.peers.len() >= ISSUED_TICKET_MAX_PEERS {
        let oldest = state
            .peers
            .iter()
            .min_by(|left, right| left.1.last_included_at.total_cmp(&right.1.last_included_at))
            .map(|(peer, _)| peer.clone())
            .ok_or_else(|| AppError::Runtime("LXMF ticket peer eviction failed".into()))?;
        state.peers.remove(&oldest);
    }
    state.peers.insert(
        peer_hash.to_string(),
        IssuedTicketEntry {
            ticket: ticket.ticket.clone(),
            expires: ticket.expires,
            last_included_at: now,
        },
    );
    validate_state(&state)?;
    save_state(path, &state)?;
    Ok(NativeLxmfTicketIssueDecision {
        state: issue_state,
        ticket: Some(ticket),
    })
}

fn load_state(path: &Path) -> AppResult<IssuedTicketState> {
    let parent = path
        .parent()
        .ok_or_else(|| AppError::Runtime("LXMF ticket state path has no parent".into()))?;
    ensure_private_directory(parent)?;
    match std::fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_symlink() || !metadata.is_file() => {
            return Err(AppError::Runtime(
                "LXMF ticket state must be a regular non-symlink file".into(),
            ));
        }
        Ok(metadata) if metadata.len() > ISSUED_TICKET_FILE_MAX_BYTES => {
            return Err(AppError::Runtime(format!(
                "LXMF ticket state exceeds the {ISSUED_TICKET_FILE_MAX_BYTES} byte limit"
            )));
        }
        Ok(_) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok(IssuedTicketState::default());
        }
        Err(error) => return Err(error.into()),
    }
    let file = File::open(path)?;
    let mut bytes = Vec::new();
    file.take(ISSUED_TICKET_FILE_MAX_BYTES + 1)
        .read_to_end(&mut bytes)?;
    if bytes.len() as u64 > ISSUED_TICKET_FILE_MAX_BYTES {
        return Err(AppError::Runtime(
            "LXMF ticket state grew beyond its byte limit".into(),
        ));
    }
    let state: IssuedTicketState = serde_json::from_slice(&bytes)
        .map_err(|_| AppError::Runtime("LXMF ticket state is invalid JSON".into()))?;
    validate_state(&state)?;
    Ok(state)
}

fn validate_state(state: &IssuedTicketState) -> AppResult<()> {
    if state.version != ISSUED_TICKET_STATE_VERSION {
        return Err(AppError::Runtime(
            "LXMF ticket state version is unsupported".into(),
        ));
    }
    if state.peers.len() > ISSUED_TICKET_MAX_PEERS {
        return Err(AppError::Runtime(
            "LXMF ticket state exceeds its peer limit".into(),
        ));
    }
    for (peer, entry) in &state.peers {
        validate_peer_hash(peer)?;
        if entry.ticket.len() != LXMF_TICKET_BYTES
            || !entry.expires.is_finite()
            || !entry.last_included_at.is_finite()
            || entry.expires < 0.0
            || entry.last_included_at < 0.0
            || entry.expires > entry.last_included_at + LXMF_TICKET_EXPIRY_SECS
        {
            return Err(AppError::Runtime(
                "LXMF ticket state contains an invalid entry".into(),
            ));
        }
    }
    Ok(())
}

fn save_state(path: &Path, state: &IssuedTicketState) -> AppResult<()> {
    let bytes = serde_json::to_vec(state)
        .map_err(|_| AppError::Runtime("LXMF ticket state encoding failed".into()))?;
    if bytes.len() as u64 > ISSUED_TICKET_FILE_MAX_BYTES {
        return Err(AppError::Runtime(
            "LXMF ticket state encoding exceeds its byte limit".into(),
        ));
    }
    let parent = path
        .parent()
        .ok_or_else(|| AppError::Runtime("LXMF ticket state path has no parent".into()))?;
    ensure_private_directory(parent)?;
    let sequence = ISSUED_TICKET_TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    let temporary = parent.join(format!(
        ".{ISSUED_TICKET_FILE_NAME}.{}.{}.tmp",
        std::process::id(),
        sequence
    ));
    let result = (|| -> std::io::Result<()> {
        let mut options = OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut file = options.open(&temporary)?;
        file.write_all(&bytes)?;
        file.flush()?;
        file.sync_all()?;
        drop(file);
        match std::fs::symlink_metadata(path) {
            Ok(metadata) if metadata.file_type().is_symlink() || !metadata.is_file() => {
                return Err(std::io::Error::other(
                    "ticket destination must be a regular non-symlink file",
                ));
            }
            Ok(_) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error),
        }
        atomic_replace(&temporary, path)?;
        #[cfg(unix)]
        File::open(parent)?.sync_all()?;
        Ok(())
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&temporary);
    }
    result.map_err(Into::into)
}

fn ensure_private_directory(path: &Path) -> AppResult<()> {
    match std::fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_symlink() || !metadata.is_dir() => {
            return Err(AppError::Runtime(
                "LXMF ticket storage root must be a real directory".into(),
            ));
        }
        Ok(_) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            let mut builder = std::fs::DirBuilder::new();
            builder.recursive(true);
            #[cfg(unix)]
            {
                use std::os::unix::fs::DirBuilderExt;
                builder.mode(0o700);
            }
            builder.create(path)?;
        }
        Err(error) => return Err(error.into()),
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o700))?;
    }
    Ok(())
}

fn validate_peer_hash(peer_hash: &str) -> AppResult<()> {
    if peer_hash.len() != 32 || !peer_hash.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(AppError::Runtime(
            "LXMF ticket peer must be a 32-character hexadecimal destination".into(),
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn root(case: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "omen-lxmf-ticket-issuer-{case}-{}-{}",
            std::process::id(),
            ISSUED_TICKET_TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ))
    }

    const PEER: &str = "00112233445566778899aabbccddeeff";

    #[test]
    fn issue_decision_debug_redacts_ticket_material() {
        let decision = NativeLxmfTicketIssueDecision {
            state: NativeLxmfTicketIssueState::IncludedNew,
            ticket: Some(NativeLxmfReplyTicket {
                ticket: vec![0xaa; LXMF_TICKET_BYTES],
                expires: 123_456.0,
            }),
        };

        let debug = format!("{decision:?}");
        assert!(debug.contains("IncludedNew"));
        assert!(debug.contains("<redacted>"));
        assert!(!debug.contains("170"));
        assert!(!debug.contains("123456"));
    }

    #[tokio::test]
    async fn issuer_throttles_reuses_renews_and_survives_restart() {
        let root = root("lifecycle");
        let issuer = NativeLxmfTicketIssuer::new(&root);
        let first = issuer.prepare(PEER, true, 1_000_000.0).await.expect("new");
        assert_eq!(first.state, NativeLxmfTicketIssueState::IncludedNew);
        let ticket = first.ticket.expect("new ticket");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(root.join(ISSUED_TICKET_FILE_NAME))
                .expect("ticket state metadata")
                .permissions()
                .mode()
                & 0o777;
            assert_eq!(mode, 0o600);
        }

        let throttled = issuer
            .prepare(PEER, true, 1_000_001.0)
            .await
            .expect("throttle");
        assert_eq!(
            throttled.state,
            NativeLxmfTicketIssueState::SuppressedInterval
        );
        assert!(throttled.ticket.is_none());

        let restarted = NativeLxmfTicketIssuer::new(&root);
        let reused = restarted
            .prepare(PEER, true, 1_000_000.0 + LXMF_TICKET_INTERVAL_SECS + 1.0)
            .await
            .expect("reuse after restart");
        assert_eq!(reused.state, NativeLxmfTicketIssueState::IncludedReused);
        assert_eq!(
            reused.ticket.as_ref().map(|value| &value.ticket),
            Some(&ticket.ticket)
        );

        let renewed = restarted
            .prepare(PEER, true, 1_000_000.0 + 8.0 * 24.0 * 60.0 * 60.0)
            .await
            .expect("renew near expiry");
        assert_eq!(renewed.state, NativeLxmfTicketIssueState::IncludedNew);
        assert_ne!(renewed.ticket.expect("renewed").ticket, ticket.ticket);
        let _ = std::fs::remove_dir_all(root);
    }

    #[tokio::test]
    async fn issuer_evicts_oldest_peer_at_the_item_limit() {
        let root = root("capacity");
        let path = root.join(ISSUED_TICKET_FILE_NAME);
        let peers = (0..ISSUED_TICKET_MAX_PEERS)
            .map(|index| {
                (
                    format!("{index:032x}"),
                    IssuedTicketEntry {
                        ticket: vec![index as u8; LXMF_TICKET_BYTES],
                        expires: 4_000_000.0 + LXMF_TICKET_EXPIRY_SECS,
                        last_included_at: 4_000_000.0 + index as f64,
                    },
                )
            })
            .collect();
        save_state(
            &path,
            &IssuedTicketState {
                version: ISSUED_TICKET_STATE_VERSION,
                peers,
            },
        )
        .expect("bounded initial state");

        let issuer = NativeLxmfTicketIssuer::new(&root);
        let new_peer = format!("{:032x}", ISSUED_TICKET_MAX_PEERS);
        assert_eq!(
            issuer
                .prepare(
                    new_peer.as_str(),
                    true,
                    4_000_000.0 + LXMF_TICKET_INTERVAL_SECS,
                )
                .await
                .expect("new peer")
                .state,
            NativeLxmfTicketIssueState::IncludedNew
        );
        let state = load_state(&path).expect("reloaded bounded state");
        assert_eq!(state.peers.len(), ISSUED_TICKET_MAX_PEERS);
        assert!(!state.peers.contains_key(&format!("{:032x}", 0)));
        assert!(state.peers.contains_key(&new_peer));
        let _ = std::fs::remove_dir_all(root);
    }

    #[tokio::test]
    async fn issuer_serializes_concurrent_requests() {
        let root = root("concurrent");
        let mut jobs = Vec::new();
        for index in 0..16 {
            let issuer = NativeLxmfTicketIssuer::new(&root);
            jobs.push(tokio::spawn(async move {
                issuer
                    .prepare(
                        if index % 2 == 0 {
                            PEER
                        } else {
                            "00112233445566778899AABBCCDDEEFF"
                        },
                        true,
                        2_000_000.0,
                    )
                    .await
                    .expect("decision")
            }));
        }
        let mut included = 0;
        let mut suppressed = 0;
        for job in jobs {
            match job.await.expect("join").state {
                NativeLxmfTicketIssueState::IncludedNew => included += 1,
                NativeLxmfTicketIssueState::SuppressedInterval => suppressed += 1,
                state => panic!("unexpected concurrent state: {state:?}"),
            }
        }
        assert_eq!(included, 1);
        assert_eq!(suppressed, 15);
        let _ = std::fs::remove_dir_all(root);
    }

    #[tokio::test]
    async fn corrupt_or_symlinked_state_is_rejected_without_replacement() {
        let root = root("unsafe");
        std::fs::create_dir_all(&root).expect("root");
        let path = root.join(ISSUED_TICKET_FILE_NAME);
        std::fs::write(&path, b"not-json").expect("corrupt state");
        let issuer = NativeLxmfTicketIssuer::new(&root);
        assert!(issuer.prepare(PEER, true, 3_000_000.0).await.is_err());
        assert_eq!(std::fs::read(&path).expect("preserved"), b"not-json");

        #[cfg(unix)]
        {
            use std::os::unix::fs::symlink;
            std::fs::remove_file(&path).expect("remove owned corrupt fixture");
            let referent = root.join("referent");
            std::fs::write(&referent, b"private").expect("referent");
            symlink(&referent, &path).expect("symlink");
            assert!(issuer.prepare(PEER, true, 3_000_001.0).await.is_err());
            assert_eq!(std::fs::read(&referent).expect("unchanged"), b"private");
        }
        let _ = std::fs::remove_dir_all(root);
    }
}
