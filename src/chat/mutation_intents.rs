use std::fs::{self, OpenOptions};
use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::{anyhow, Context};
use omenchat_protocol::{
    canonical_mutation_request_hash, ChatOp, ClientInstanceId, Frame, FrameBody, MutationId,
    RequestHash, CLIENT_INSTANCE_ID_BYTES,
};
use rand_core::RngCore;
use rusqlite::OptionalExtension;

use super::client_instance::ClientInstanceIdStore;
use super::codec::{decode_frame, encode_frame};
use super::model::CHAT_SERVER_DESTINATION_MAX_BYTES;

pub const MAX_OUTBOUND_MUTATION_INTENTS: i64 = 4_096;
pub const MAX_OUTBOUND_MUTATION_INTENT_BYTES: i64 = 16 * 1024 * 1024;
pub const MAX_OUTBOUND_MUTATION_INTENT_BYTES_EACH: usize = 64 * 1024;
const MAX_IDENTITY_HASH_BYTES: usize = 64;
const MAX_CORRELATION_ID_BYTES: usize = 256;
const MAX_INTENT_LIFETIME_SECONDS: i64 = 30 * 24 * 60 * 60;
const SQLITE_BUSY_TIMEOUT: Duration = Duration::from_millis(250);
const INTENT_DATABASE: &str = "mutation-intents.sqlite";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(i64)]
pub enum OutboundMutationState {
    Prepared = 0,
    SentUncertain = 1,
    Acknowledged = 2,
    Conflict = 3,
    Expired = 4,
    Abandoned = 5,
}

#[derive(Clone, Debug, PartialEq)]
pub struct OutboundMutationIntent {
    pub server_destination: String,
    pub authenticated_identity_hash: Vec<u8>,
    pub client_instance_id: ClientInstanceId,
    pub mutation_id: MutationId,
    pub request_hash: RequestHash,
    pub op: ChatOp,
    pub room_id: Option<u32>,
    pub body: FrameBody,
    pub state: OutboundMutationState,
    pub created_at: i64,
    pub expires_at: i64,
    pub correlation_id: Option<String>,
}

pub struct PrepareOutboundMutation<'a> {
    pub server_destination: &'a str,
    pub authenticated_identity_hash: &'a [u8],
    pub client_instance_id: ClientInstanceId,
    pub op: ChatOp,
    pub room_id: Option<u32>,
    pub body: FrameBody,
    pub created_at: i64,
    pub expires_at: i64,
    pub correlation_id: Option<&'a str>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct OwnedPrepareOutboundMutation {
    pub server_destination: String,
    pub authenticated_identity_hash: Vec<u8>,
    pub client_instance_id: ClientInstanceId,
    pub op: ChatOp,
    pub room_id: Option<u32>,
    pub body: FrameBody,
    pub created_at: i64,
    pub expires_at: i64,
    pub correlation_id: Option<String>,
}

impl OwnedPrepareOutboundMutation {
    pub fn as_borrowed(&self) -> PrepareOutboundMutation<'_> {
        PrepareOutboundMutation {
            server_destination: &self.server_destination,
            authenticated_identity_hash: &self.authenticated_identity_hash,
            client_instance_id: self.client_instance_id,
            op: self.op,
            room_id: self.room_id,
            body: self.body.clone(),
            created_at: self.created_at,
            expires_at: self.expires_at,
            correlation_id: self.correlation_id.as_deref(),
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum IntentTransition {
    Updated(OutboundMutationIntent),
    Missing,
    StateMismatch { current: OutboundMutationState },
}

pub struct MutationIntentStore {
    path: PathBuf,
    connection: rusqlite::Connection,
}

#[derive(Clone, Copy)]
struct IntentLimits {
    items: i64,
    bytes: i64,
    each_bytes: usize,
}

const PRODUCTION_LIMITS: IntentLimits = IntentLimits {
    items: MAX_OUTBOUND_MUTATION_INTENTS,
    bytes: MAX_OUTBOUND_MUTATION_INTENT_BYTES,
    each_bytes: MAX_OUTBOUND_MUTATION_INTENT_BYTES_EACH,
};

pub(crate) fn queued_prepare_bytes(
    request: &OwnedPrepareOutboundMutation,
) -> anyhow::Result<usize> {
    let borrowed = request.as_borrowed();
    validate_prepare_request(&borrowed, PRODUCTION_LIMITS)?;
    canonical_mutation_request_hash(borrowed.op, borrowed.room_id, &borrowed.body)
        .context("validate queued OMENchat mutation intent")?;
    let frame = encode_frame(&Frame::new(
        borrowed.op,
        0,
        borrowed.room_id,
        borrowed.body.clone(),
    ))
    .context("encode queued OMENchat mutation intent")?;
    let retained = retained_bytes(&borrowed, frame.len())?;
    if frame.len() > MAX_OUTBOUND_MUTATION_INTENT_BYTES_EACH
        || retained > MAX_OUTBOUND_MUTATION_INTENT_BYTES_EACH as i64
    {
        return Err(anyhow!(
            "queued OMENchat mutation intent exceeds its byte limit"
        ));
    }
    usize::try_from(retained).context("queued OMENchat mutation intent byte overflow")
}

impl MutationIntentStore {
    pub fn open_for_identity_storage_root(root: impl AsRef<Path>) -> anyhow::Result<Self> {
        let directory = root.as_ref().join("omenchat");
        ensure_private_directory(&directory)?;
        let path = directory.join(INTENT_DATABASE);
        reserve_or_validate_private_database(&path)?;
        let connection = rusqlite::Connection::open(&path)?;
        connection.busy_timeout(SQLITE_BUSY_TIMEOUT)?;
        connection.pragma_update(None, "foreign_keys", true)?;
        connection.pragma_update(None, "synchronous", "FULL")?;
        connection.execute_batch(
            "CREATE TABLE IF NOT EXISTS mutation_intents (
               mutation_id BLOB PRIMARY KEY CHECK(length(mutation_id) = 16),
               client_instance_id BLOB NOT NULL CHECK(length(client_instance_id) = 16),
               server_destination TEXT NOT NULL,
               authenticated_identity_hash BLOB NOT NULL,
               request_hash BLOB NOT NULL CHECK(length(request_hash) = 32),
               op INTEGER NOT NULL,
               room_id INTEGER,
               request_frame BLOB NOT NULL,
               state INTEGER NOT NULL,
               created_at INTEGER NOT NULL,
               expires_at INTEGER NOT NULL,
               correlation_id TEXT,
               retained_bytes INTEGER NOT NULL CHECK(retained_bytes >= 0)
             );
             CREATE INDEX IF NOT EXISTS idx_mutation_intents_state_created
             ON mutation_intents(state, created_at, mutation_id);",
        )?;
        Ok(Self { path, connection })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Persists a prepared intent before returning it to a future transport
    /// owner. This function does not transmit or retry the mutation.
    pub fn persist_prepared(
        &self,
        request: PrepareOutboundMutation<'_>,
    ) -> anyhow::Result<OutboundMutationIntent> {
        let mut mutation = [0_u8; 16];
        rand_core::OsRng.fill_bytes(&mut mutation);
        self.persist_prepared_with_id(request, MutationId::new(mutation), PRODUCTION_LIMITS)
    }

    pub fn load(&self, mutation_id: MutationId) -> anyhow::Result<Option<OutboundMutationIntent>> {
        let transaction = rusqlite::Transaction::new_unchecked(
            &self.connection,
            rusqlite::TransactionBehavior::Deferred,
        )?;
        let lengths = transaction
            .query_row(
                "SELECT length(client_instance_id),
                        length(CAST(server_destination AS BLOB)),
                        length(authenticated_identity_hash), length(request_hash),
                        length(request_frame),
                        COALESCE(length(CAST(correlation_id AS BLOB)), 0),
                        retained_bytes
                 FROM mutation_intents WHERE mutation_id = ?1",
                [mutation_id.as_bytes().as_slice()],
                |row| {
                    Ok(IntentLengths {
                        client_instance_id: row.get(0)?,
                        server_destination: row.get(1)?,
                        identity_hash: row.get(2)?,
                        request_hash: row.get(3)?,
                        request_frame: row.get(4)?,
                        correlation_id: row.get(5)?,
                        retained_bytes: row.get(6)?,
                    })
                },
            )
            .optional()?;
        let Some(lengths) = lengths else {
            transaction.commit()?;
            return Ok(None);
        };
        validate_stored_lengths(lengths)?;
        let row = transaction.query_row(
            "SELECT client_instance_id, server_destination,
                        authenticated_identity_hash, request_hash, op, room_id,
                        request_frame, state, created_at, expires_at, correlation_id,
                        retained_bytes
                 FROM mutation_intents WHERE mutation_id = ?1",
            [mutation_id.as_bytes().as_slice()],
            |row| {
                Ok(StoredIntent {
                    client_instance_id: row.get(0)?,
                    server_destination: row.get(1)?,
                    authenticated_identity_hash: row.get(2)?,
                    request_hash: row.get(3)?,
                    op: row.get(4)?,
                    room_id: row.get(5)?,
                    request_frame: row.get(6)?,
                    state: row.get(7)?,
                    created_at: row.get(8)?,
                    expires_at: row.get(9)?,
                    correlation_id: row.get(10)?,
                    retained_bytes: row.get(11)?,
                })
            },
        )?;
        let intent = decode_stored_intent(mutation_id, row)?;
        transaction.commit()?;
        Ok(Some(intent))
    }

    pub fn transition(
        &self,
        mutation_id: MutationId,
        expected: OutboundMutationState,
        next: OutboundMutationState,
    ) -> anyhow::Result<IntentTransition> {
        if !allowed_transition(expected, next) {
            return Err(anyhow!("invalid OMENchat mutation intent state transition"));
        }
        let transaction = rusqlite::Transaction::new_unchecked(
            &self.connection,
            rusqlite::TransactionBehavior::Immediate,
        )?;
        let changed = transaction.execute(
            "UPDATE mutation_intents SET state = ?3
             WHERE mutation_id = ?1 AND state = ?2",
            (
                mutation_id.as_bytes().as_slice(),
                expected as i64,
                next as i64,
            ),
        )?;
        transaction.commit()?;
        if changed == 1 {
            return self
                .load(mutation_id)?
                .map(IntentTransition::Updated)
                .ok_or_else(|| anyhow!("updated OMENchat mutation intent disappeared"));
        }
        Ok(match self.load(mutation_id)? {
            Some(intent) => IntentTransition::StateMismatch {
                current: intent.state,
            },
            None => IntentTransition::Missing,
        })
    }

    pub fn recover_nonterminal(&self) -> anyhow::Result<Vec<OutboundMutationIntent>> {
        let (count, retained_bytes): (i64, i64) = self.connection.query_row(
            "SELECT COUNT(*), COALESCE(SUM(retained_bytes), 0)
             FROM mutation_intents WHERE state IN (0, 1)",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )?;
        if count > MAX_OUTBOUND_MUTATION_INTENTS
            || retained_bytes > MAX_OUTBOUND_MUTATION_INTENT_BYTES
        {
            return Err(anyhow!(
                "stored OMENchat nonterminal mutation intents exceed recovery bounds"
            ));
        }
        let mut statement = self.connection.prepare(
            "SELECT mutation_id FROM mutation_intents
             WHERE state IN (0, 1)
             ORDER BY created_at, mutation_id
             LIMIT ?1",
        )?;
        let ids = statement
            .query_map([MAX_OUTBOUND_MUTATION_INTENTS + 1], |row| {
                row.get::<_, Vec<u8>>(0)
            })?
            .collect::<Result<Vec<_>, _>>()?;
        drop(statement);
        if ids.len() > MAX_OUTBOUND_MUTATION_INTENTS as usize {
            return Err(anyhow!(
                "stored OMENchat nonterminal mutation intent count exceeds recovery bounds"
            ));
        }
        let mut intents = Vec::with_capacity(ids.len());
        for id in ids {
            let mutation_id = MutationId::try_from(id.as_slice())
                .context("invalid stored OMENchat mutation id")?;
            let intent = self
                .load(mutation_id)?
                .ok_or_else(|| anyhow!("OMENchat mutation intent disappeared during recovery"))?;
            intents.push(intent);
        }
        Ok(intents)
    }

    pub fn prune_terminal(&self, now: i64) -> anyhow::Result<usize> {
        let cutoff = now.saturating_sub(MAX_INTENT_LIFETIME_SECONDS);
        let transaction = rusqlite::Transaction::new_unchecked(
            &self.connection,
            rusqlite::TransactionBehavior::Immediate,
        )?;
        let removed = transaction.execute(
            "DELETE FROM mutation_intents WHERE mutation_id IN (
               SELECT mutation_id FROM mutation_intents
               WHERE state IN (2, 3, 4, 5) AND created_at < ?1
               ORDER BY created_at, mutation_id
               LIMIT 128
             )",
            [cutoff],
        )?;
        transaction.commit()?;
        Ok(removed)
    }

    /// Rotates the future durable-mutation client instance only while an
    /// immediate SQLite transaction excludes concurrent intent admission and
    /// proves that no prepared or uncertain intent can be orphaned.
    ///
    /// This boundary is intentionally not called by production startup or
    /// networking while durable-mutation negotiation remains inactive.
    pub fn rotate_client_instance_if_quiescent(
        &self,
        instance_store: &ClientInstanceIdStore,
        expected: ClientInstanceId,
    ) -> anyhow::Result<ClientInstanceId> {
        self.rotate_client_instance_if_quiescent_with(instance_store, expected, || {
            let mut replacement = [0_u8; CLIENT_INSTANCE_ID_BYTES];
            rand_core::OsRng.fill_bytes(&mut replacement);
            ClientInstanceId::new(replacement)
        })
    }

    fn rotate_client_instance_if_quiescent_with(
        &self,
        instance_store: &ClientInstanceIdStore,
        expected: ClientInstanceId,
        generate: impl FnOnce() -> ClientInstanceId,
    ) -> anyhow::Result<ClientInstanceId> {
        let transaction = rusqlite::Transaction::new_unchecked(
            &self.connection,
            rusqlite::TransactionBehavior::Immediate,
        )?;
        let nonterminal: i64 = transaction.query_row(
            "SELECT COUNT(*) FROM mutation_intents WHERE state IN (0, 1)",
            [],
            |row| row.get(0),
        )?;
        if nonterminal != 0 {
            return Err(anyhow!(
                "OMENchat client instance cannot rotate while prepared or uncertain intents exist"
            ));
        }
        let replacement = generate();
        instance_store
            .replace_expected(expected, replacement)
            .context("rotate OMENchat client instance")?;
        transaction.commit()?;
        Ok(replacement)
    }

    fn persist_prepared_with_id(
        &self,
        request: PrepareOutboundMutation<'_>,
        mutation_id: MutationId,
        limits: IntentLimits,
    ) -> anyhow::Result<OutboundMutationIntent> {
        validate_prepare_request(&request, limits)?;
        let request_hash =
            canonical_mutation_request_hash(request.op, request.room_id, &request.body)
                .context("canonicalize OMENchat mutation intent")?;
        let request_frame = encode_frame(&Frame::new(
            request.op,
            0,
            request.room_id,
            request.body.clone(),
        ))
        .context("encode OMENchat mutation intent")?;
        let retained_bytes = retained_bytes(&request, request_frame.len())?;
        if request_frame.len() > limits.each_bytes || retained_bytes > limits.each_bytes as i64 {
            return Err(anyhow!(
                "OMENchat mutation intent exceeds the {}-byte per-intent limit",
                limits.each_bytes
            ));
        }

        let transaction = rusqlite::Transaction::new_unchecked(
            &self.connection,
            rusqlite::TransactionBehavior::Immediate,
        )?;
        let (items, bytes): (i64, i64) = transaction.query_row(
            "SELECT COUNT(*), COALESCE(SUM(retained_bytes), 0) FROM mutation_intents",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )?;
        if items.saturating_add(1) > limits.items
            || bytes.saturating_add(retained_bytes) > limits.bytes
        {
            return Err(anyhow!(
                "OMENchat mutation intent capacity is exhausted; uncertain intents were preserved"
            ));
        }
        transaction.execute(
            "INSERT INTO mutation_intents(
               mutation_id, client_instance_id, server_destination,
               authenticated_identity_hash, request_hash, op, room_id,
               request_frame, state, created_at, expires_at, correlation_id,
               retained_bytes
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, 0, ?9, ?10, ?11, ?12)",
            rusqlite::params![
                mutation_id.as_bytes().as_slice(),
                request.client_instance_id.as_bytes().as_slice(),
                request.server_destination,
                request.authenticated_identity_hash,
                request_hash.as_bytes().as_slice(),
                request.op as u16 as i64,
                request.room_id.map(i64::from),
                request_frame,
                request.created_at,
                request.expires_at,
                request.correlation_id,
                retained_bytes,
            ],
        )?;
        transaction.commit()?;
        Ok(OutboundMutationIntent {
            server_destination: request.server_destination.to_owned(),
            authenticated_identity_hash: request.authenticated_identity_hash.to_vec(),
            client_instance_id: request.client_instance_id,
            mutation_id,
            request_hash,
            op: request.op,
            room_id: request.room_id,
            body: request.body,
            state: OutboundMutationState::Prepared,
            created_at: request.created_at,
            expires_at: request.expires_at,
            correlation_id: request.correlation_id.map(str::to_owned),
        })
    }
}

struct StoredIntent {
    client_instance_id: Vec<u8>,
    server_destination: String,
    authenticated_identity_hash: Vec<u8>,
    request_hash: Vec<u8>,
    op: i64,
    room_id: Option<i64>,
    request_frame: Vec<u8>,
    state: i64,
    created_at: i64,
    expires_at: i64,
    correlation_id: Option<String>,
    retained_bytes: i64,
}

#[derive(Clone, Copy)]
struct IntentLengths {
    client_instance_id: i64,
    server_destination: i64,
    identity_hash: i64,
    request_hash: i64,
    request_frame: i64,
    correlation_id: i64,
    retained_bytes: i64,
}

fn validate_stored_lengths(lengths: IntentLengths) -> anyhow::Result<()> {
    if lengths.client_instance_id != 16
        || !(1..=CHAT_SERVER_DESTINATION_MAX_BYTES as i64).contains(&lengths.server_destination)
        || !(1..=MAX_IDENTITY_HASH_BYTES as i64).contains(&lengths.identity_hash)
        || lengths.request_hash != 32
        || !(1..=MAX_OUTBOUND_MUTATION_INTENT_BYTES_EACH as i64).contains(&lengths.request_frame)
        || !(0..=MAX_CORRELATION_ID_BYTES as i64).contains(&lengths.correlation_id)
        || !(1..=MAX_OUTBOUND_MUTATION_INTENT_BYTES_EACH as i64).contains(&lengths.retained_bytes)
    {
        return Err(anyhow!(
            "stored OMENchat mutation intent exceeds fixed recovery bounds"
        ));
    }
    Ok(())
}

fn decode_stored_intent(
    mutation_id: MutationId,
    stored: StoredIntent,
) -> anyhow::Result<OutboundMutationIntent> {
    let client_instance_id = ClientInstanceId::try_from(stored.client_instance_id.as_slice())?;
    let request_hash = RequestHash::try_from(stored.request_hash.as_slice())?;
    let op = ChatOp::try_from(u64::try_from(stored.op).context("negative stored operation")?)?;
    let room_id = stored
        .room_id
        .map(u32::try_from)
        .transpose()
        .context("invalid stored room id")?;
    let state = match stored.state {
        0 => OutboundMutationState::Prepared,
        1 => OutboundMutationState::SentUncertain,
        2 => OutboundMutationState::Acknowledged,
        3 => OutboundMutationState::Conflict,
        4 => OutboundMutationState::Expired,
        5 => OutboundMutationState::Abandoned,
        _ => return Err(anyhow!("invalid stored OMENchat mutation intent state")),
    };
    let frame = decode_frame(&stored.request_frame).context("decode stored mutation intent")?;
    if frame.seq != 0 || frame.op != op || frame.room_id != room_id {
        return Err(anyhow!(
            "stored OMENchat mutation intent metadata does not match its frame"
        ));
    }
    let actual_hash = canonical_mutation_request_hash(op, room_id, &frame.body)?;
    if actual_hash != request_hash {
        return Err(anyhow!(
            "stored OMENchat mutation intent hash does not match its frame"
        ));
    }
    validate_loaded_fields(
        &stored.server_destination,
        &stored.authenticated_identity_hash,
        stored.correlation_id.as_deref(),
        stored.created_at,
        stored.expires_at,
    )?;
    let actual_retained_bytes = stored
        .server_destination
        .len()
        .checked_add(stored.authenticated_identity_hash.len())
        .and_then(|bytes| bytes.checked_add(16 + 16 + 32))
        .and_then(|bytes| bytes.checked_add(stored.request_frame.len()))
        .and_then(|bytes| bytes.checked_add(stored.correlation_id.as_ref().map_or(0, String::len)))
        .and_then(|bytes| i64::try_from(bytes).ok())
        .ok_or_else(|| anyhow!("stored OMENchat mutation intent retained-byte overflow"))?;
    if actual_retained_bytes != stored.retained_bytes {
        return Err(anyhow!(
            "stored OMENchat mutation intent retained-byte accounting is invalid"
        ));
    }
    Ok(OutboundMutationIntent {
        server_destination: stored.server_destination,
        authenticated_identity_hash: stored.authenticated_identity_hash,
        client_instance_id,
        mutation_id,
        request_hash,
        op,
        room_id,
        body: frame.body,
        state,
        created_at: stored.created_at,
        expires_at: stored.expires_at,
        correlation_id: stored.correlation_id,
    })
}

fn validate_prepare_request(
    request: &PrepareOutboundMutation<'_>,
    limits: IntentLimits,
) -> anyhow::Result<()> {
    if limits.items <= 0 || limits.bytes <= 0 || limits.each_bytes == 0 {
        return Err(anyhow!("OMENchat mutation intent limits are invalid"));
    }
    if !matches!(
        request.op,
        ChatOp::RoomMessage
            | ChatOp::RoomAction
            | ChatOp::RoomNotice
            | ChatOp::PartRoom
            | ChatOp::Command
    ) {
        return Err(anyhow!(
            "OMENchat operation is not eligible for durable mutation intent"
        ));
    }
    validate_loaded_fields(
        request.server_destination,
        request.authenticated_identity_hash,
        request.correlation_id,
        request.created_at,
        request.expires_at,
    )
}

fn allowed_transition(expected: OutboundMutationState, next: OutboundMutationState) -> bool {
    matches!(
        (expected, next),
        (
            OutboundMutationState::Prepared,
            OutboundMutationState::SentUncertain
                | OutboundMutationState::Expired
                | OutboundMutationState::Abandoned
        ) | (
            OutboundMutationState::SentUncertain,
            OutboundMutationState::Acknowledged
                | OutboundMutationState::Conflict
                | OutboundMutationState::Expired
                | OutboundMutationState::Abandoned
        )
    )
}

fn validate_loaded_fields(
    server_destination: &str,
    identity_hash: &[u8],
    correlation_id: Option<&str>,
    created_at: i64,
    expires_at: i64,
) -> anyhow::Result<()> {
    if server_destination.is_empty() || server_destination.len() > CHAT_SERVER_DESTINATION_MAX_BYTES
    {
        return Err(anyhow!(
            "OMENchat mutation intent server destination is invalid"
        ));
    }
    if identity_hash.is_empty() || identity_hash.len() > MAX_IDENTITY_HASH_BYTES {
        return Err(anyhow!(
            "OMENchat mutation intent identity binding is invalid"
        ));
    }
    if correlation_id.is_some_and(|value| value.len() > MAX_CORRELATION_ID_BYTES) {
        return Err(anyhow!(
            "OMENchat mutation intent correlation id is too large"
        ));
    }
    if expires_at <= created_at
        || expires_at.saturating_sub(created_at) > MAX_INTENT_LIFETIME_SECONDS
    {
        return Err(anyhow!("OMENchat mutation intent expiry is invalid"));
    }
    Ok(())
}

fn retained_bytes(
    request: &PrepareOutboundMutation<'_>,
    frame_bytes: usize,
) -> anyhow::Result<i64> {
    request
        .server_destination
        .len()
        .checked_add(request.authenticated_identity_hash.len())
        .and_then(|bytes| bytes.checked_add(16 + 16 + 32))
        .and_then(|bytes| bytes.checked_add(frame_bytes))
        .and_then(|bytes| bytes.checked_add(request.correlation_id.map_or(0, str::len)))
        .and_then(|bytes| i64::try_from(bytes).ok())
        .ok_or_else(|| anyhow!("OMENchat mutation intent retained-byte overflow"))
}

fn ensure_private_directory(path: &Path) -> anyhow::Result<()> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_dir() => {
            validate_private_mode(path, &metadata, true)
        }
        Ok(_) => Err(anyhow!(
            "OMENchat mutation intent parent must be a directory"
        )),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            let root = path.parent().context("missing OMENchat identity root")?;
            if !fs::symlink_metadata(root)?.file_type().is_dir() {
                return Err(anyhow!(
                    "OMENchat identity storage root must be a directory"
                ));
            }
            let builder = fs::DirBuilder::new();
            #[cfg(unix)]
            let mut builder = builder;
            #[cfg(unix)]
            {
                use std::os::unix::fs::DirBuilderExt;
                builder.mode(0o700);
            }
            builder.create(path)?;
            let metadata = fs::symlink_metadata(path)?;
            validate_private_mode(path, &metadata, true)
        }
        Err(error) => Err(error.into()),
    }
}

fn reserve_or_validate_private_database(path: &Path) -> anyhow::Result<()> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_file() => {
            validate_private_mode(path, &metadata, false)
        }
        Ok(_) => Err(anyhow!(
            "OMENchat mutation intent database must be a regular file"
        )),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            let mut options = OpenOptions::new();
            options.write(true).create_new(true);
            #[cfg(unix)]
            {
                use std::os::unix::fs::OpenOptionsExt;
                options.mode(0o600);
            }
            options.open(path)?.sync_all()?;
            Ok(())
        }
        Err(error) => Err(error.into()),
    }
}

fn validate_private_mode(
    path: &Path,
    metadata: &fs::Metadata,
    directory: bool,
) -> anyhow::Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if metadata.permissions().mode() & 0o077 != 0 {
            return Err(anyhow!(
                "OMENchat mutation intent {} permissions must be owner-only: {}",
                if directory { "directory" } else { "database" },
                path.display()
            ));
        }
    }
    #[cfg(not(unix))]
    let _ = (path, metadata, directory);
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::time::Instant;

    use super::*;

    static ROOT_SEQUENCE: AtomicU64 = AtomicU64::new(1);

    fn isolated_root(label: &str) -> PathBuf {
        let root = std::env::temp_dir().join(format!(
            "omenbrowser-mutation-intents-{label}-{}-{}",
            std::process::id(),
            ROOT_SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).expect("identity root");
        root
    }

    fn request<'a>(body: &'a str) -> PrepareOutboundMutation<'a> {
        PrepareOutboundMutation {
            server_destination: "0123456789abcdef",
            authenticated_identity_hash: b"authenticated-peer",
            client_instance_id: ClientInstanceId::new([7; 16]),
            op: ChatOp::RoomMessage,
            room_id: Some(9),
            body: FrameBody::Text(body.into()),
            created_at: 100,
            expires_at: 200,
            correlation_id: Some("local-message-1"),
        }
    }

    fn percentile_micros(samples: &mut [u128], percentile: usize) -> u128 {
        samples.sort_unstable();
        let index = samples
            .len()
            .saturating_mul(percentile)
            .saturating_add(99)
            .checked_div(100)
            .unwrap_or(0)
            .saturating_sub(1)
            .min(samples.len().saturating_sub(1));
        samples[index]
    }

    fn sqlite_file_bytes(path: &Path) -> u64 {
        [
            path.to_path_buf(),
            path.with_extension("sqlite-wal"),
            path.with_extension("sqlite-shm"),
        ]
        .into_iter()
        .filter_map(|candidate| fs::metadata(candidate).ok())
        .map(|metadata| metadata.len())
        .sum()
    }

    #[test]
    fn prepared_intent_is_private_persistent_and_canonically_verified() {
        let root = isolated_root("persist");
        let store = MutationIntentStore::open_for_identity_storage_root(&root).expect("store");
        let intent = store
            .persist_prepared_with_id(
                request("hello"),
                MutationId::new([3; 16]),
                PRODUCTION_LIMITS,
            )
            .expect("persist intent");
        assert_eq!(intent.state, OutboundMutationState::Prepared);
        drop(store);

        let reopened = MutationIntentStore::open_for_identity_storage_root(&root).expect("reopen");
        assert_eq!(
            reopened.load(intent.mutation_id).expect("load"),
            Some(intent)
        );
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                fs::metadata(reopened.path())
                    .expect("metadata")
                    .permissions()
                    .mode()
                    & 0o777,
                0o600
            );
        }
        drop(reopened);
        fs::remove_dir_all(root).expect("cleanup");
    }

    #[test]
    fn capacity_refuses_new_admission_without_removing_existing_intent() {
        let root = isolated_root("capacity");
        let store = MutationIntentStore::open_for_identity_storage_root(&root).expect("store");
        let limits = IntentLimits {
            items: 1,
            bytes: 1_000_000,
            each_bytes: MAX_OUTBOUND_MUTATION_INTENT_BYTES_EACH,
        };
        let first = store
            .persist_prepared_with_id(request("first"), MutationId::new([1; 16]), limits)
            .expect("first intent");
        let error = store
            .persist_prepared_with_id(request("second"), MutationId::new([2; 16]), limits)
            .expect_err("capacity must fail");
        assert!(error.to_string().contains("capacity is exhausted"));
        assert_eq!(
            store.load(first.mutation_id).expect("first remains"),
            Some(first)
        );
        assert!(store
            .load(MutationId::new([2; 16]))
            .expect("second absent")
            .is_none());
        drop(store);
        fs::remove_dir_all(root).expect("cleanup");
    }

    #[test]
    fn transitions_recover_nonterminal_and_prune_only_old_terminal_intents() {
        let root = isolated_root("transitions");
        let store = MutationIntentStore::open_for_identity_storage_root(&root).expect("store");
        let uncertain_id = MutationId::new([10; 16]);
        let abandoned_id = MutationId::new([11; 16]);
        store
            .persist_prepared_with_id(request("uncertain"), uncertain_id, PRODUCTION_LIMITS)
            .expect("uncertain intent");
        store
            .persist_prepared_with_id(request("abandoned"), abandoned_id, PRODUCTION_LIMITS)
            .expect("abandoned intent");
        assert!(matches!(
            store
                .transition(
                    uncertain_id,
                    OutboundMutationState::Prepared,
                    OutboundMutationState::SentUncertain
                )
                .expect("uncertain transition"),
            IntentTransition::Updated(_)
        ));
        assert!(matches!(
            store
                .transition(
                    abandoned_id,
                    OutboundMutationState::Prepared,
                    OutboundMutationState::Abandoned
                )
                .expect("abandoned transition"),
            IntentTransition::Updated(_)
        ));
        let recovered = store.recover_nonterminal().expect("recover");
        assert_eq!(recovered.len(), 1);
        assert_eq!(recovered[0].mutation_id, uncertain_id);

        let removed = store
            .prune_terminal(100 + MAX_INTENT_LIFETIME_SECONDS + 1)
            .expect("terminal prune");
        assert_eq!(removed, 1);
        assert!(store.load(abandoned_id).expect("load abandoned").is_none());
        assert!(store.load(uncertain_id).expect("load uncertain").is_some());

        store
            .transition(
                uncertain_id,
                OutboundMutationState::SentUncertain,
                OutboundMutationState::Acknowledged,
            )
            .expect("acknowledge");
        let error = store
            .transition(
                uncertain_id,
                OutboundMutationState::Acknowledged,
                OutboundMutationState::SentUncertain,
            )
            .expect_err("terminal regression must fail");
        assert!(error
            .to_string()
            .contains("invalid OMENchat mutation intent state transition"));
        assert!(store
            .recover_nonterminal()
            .expect("terminal recovery")
            .is_empty());
        drop(store);
        fs::remove_dir_all(root).expect("cleanup");
    }

    #[test]
    fn client_instance_rotation_requires_quiescence_and_survives_restart() {
        let root = isolated_root("rotation");
        let instance_store = ClientInstanceIdStore::for_identity_storage_root(&root);
        let original = instance_store
            .load_or_create()
            .expect("original client instance");
        let store = MutationIntentStore::open_for_identity_storage_root(&root).expect("store");
        let mutation_id = MutationId::new([12; 16]);
        let mut prepared = request("uncertain");
        prepared.client_instance_id = original;
        store
            .persist_prepared_with_id(prepared, mutation_id, PRODUCTION_LIMITS)
            .expect("prepared intent");

        let prepared_error = store
            .rotate_client_instance_if_quiescent_with(&instance_store, original, || {
                ClientInstanceId::new([8; CLIENT_INSTANCE_ID_BYTES])
            })
            .expect_err("prepared intent must block rotation");
        assert!(prepared_error
            .to_string()
            .contains("prepared or uncertain intents exist"));
        assert_eq!(
            instance_store.load().expect("original remains"),
            Some(original)
        );

        store
            .transition(
                mutation_id,
                OutboundMutationState::Prepared,
                OutboundMutationState::SentUncertain,
            )
            .expect("mark uncertain");
        assert!(store
            .rotate_client_instance_if_quiescent_with(&instance_store, original, || {
                ClientInstanceId::new([8; CLIENT_INSTANCE_ID_BYTES])
            })
            .is_err());
        store
            .transition(
                mutation_id,
                OutboundMutationState::SentUncertain,
                OutboundMutationState::Abandoned,
            )
            .expect("explicitly abandon");

        let replacement = ClientInstanceId::new([8; CLIENT_INSTANCE_ID_BYTES]);
        assert_eq!(
            store
                .rotate_client_instance_if_quiescent_with(&instance_store, original, || {
                    replacement
                })
                .expect("quiescent rotation"),
            replacement
        );
        assert_eq!(
            store
                .load(mutation_id)
                .expect("terminal intent")
                .expect("retained terminal intent")
                .client_instance_id,
            original,
            "rotation must not rewrite historical intent identity"
        );
        drop(store);

        let restarted = ClientInstanceIdStore::for_identity_storage_root(&root);
        assert_eq!(
            restarted.load().expect("restarted client instance"),
            Some(replacement)
        );
        fs::remove_dir_all(root).expect("cleanup");
    }

    #[test]
    fn rotation_lock_excludes_concurrent_intent_admission() {
        let root = isolated_root("rotation-race");
        let instance_store = ClientInstanceIdStore::for_identity_storage_root(&root);
        let original = instance_store
            .load_or_create()
            .expect("original client instance");
        let competing_store =
            MutationIntentStore::open_for_identity_storage_root(&root).expect("competing store");
        let (entered_sender, entered_receiver) = std::sync::mpsc::sync_channel(1);
        let (release_sender, release_receiver) = std::sync::mpsc::sync_channel(1);
        let rotation_root = root.clone();
        let rotation = std::thread::spawn(move || {
            let store = MutationIntentStore::open_for_identity_storage_root(&rotation_root)
                .expect("rotation store");
            let instance_store = ClientInstanceIdStore::for_identity_storage_root(&rotation_root);
            store.rotate_client_instance_if_quiescent_with(&instance_store, original, || {
                entered_sender.send(()).expect("signal rotation lock");
                release_receiver.recv().expect("release rotation lock");
                ClientInstanceId::new([8; CLIENT_INSTANCE_ID_BYTES])
            })
        });

        entered_receiver.recv().expect("rotation acquired lock");
        let mut competing = request("must not race rotation");
        competing.client_instance_id = original;
        let error = competing_store
            .persist_prepared_with_id(competing, MutationId::new([13; 16]), PRODUCTION_LIMITS)
            .expect_err("concurrent admission must not pass rotation lock");
        assert!(error.to_string().contains("database is locked"));
        release_sender.send(()).expect("release rotation");
        let replacement = rotation
            .join()
            .expect("rotation thread")
            .expect("rotation result");
        assert_ne!(replacement, original);
        assert!(competing_store
            .recover_nonterminal()
            .expect("no competing intent")
            .is_empty());
        drop(competing_store);
        fs::remove_dir_all(root).expect("cleanup");
    }

    #[test]
    #[ignore = "explicit isolated durable-intent retention measurement"]
    fn durable_intent_retention_measurement() {
        let items = std::env::var("OMEN_DURABLE_MEASUREMENT_ITEMS")
            .ok()
            .and_then(|value| value.parse::<usize>().ok())
            .unwrap_or(1_024);
        assert!((256..=MAX_OUTBOUND_MUTATION_INTENTS as usize).contains(&items));
        let root = isolated_root("retention-measurement");
        let store = MutationIntentStore::open_for_identity_storage_root(&root).expect("store");
        let body = "m".repeat(128);
        let mut prepare_micros = Vec::with_capacity(items);

        for index in 0..items {
            let mut prepared = request(&body);
            prepared.created_at = 100 + index as i64;
            prepared.expires_at = prepared.created_at + 100;
            prepared.correlation_id = None;
            let started = Instant::now();
            store
                .persist_prepared_with_id(
                    prepared,
                    MutationId::new((index as u128).to_be_bytes()),
                    PRODUCTION_LIMITS,
                )
                .expect("measurement intent");
            prepare_micros.push(started.elapsed().as_micros());
        }

        let recovery_started = Instant::now();
        let recovered = store.recover_nonterminal().expect("measurement recovery");
        let recovery_micros = recovery_started.elapsed().as_micros();
        assert_eq!(recovered.len(), items);

        let transition_started = Instant::now();
        for index in 0..items {
            let outcome = store
                .transition(
                    MutationId::new((index as u128).to_be_bytes()),
                    OutboundMutationState::Prepared,
                    OutboundMutationState::Abandoned,
                )
                .expect("measurement terminal transition");
            assert!(matches!(outcome, IntentTransition::Updated(_)));
        }
        let transition_micros = transition_started.elapsed().as_micros();

        let prune_started = Instant::now();
        let mut pruned = 0usize;
        let mut prune_calls = 0usize;
        loop {
            let removed = store
                .prune_terminal(101 + items as i64 + MAX_INTENT_LIFETIME_SECONDS)
                .expect("measurement prune");
            if removed == 0 {
                break;
            }
            pruned += removed;
            prune_calls += 1;
        }
        let prune_micros = prune_started.elapsed().as_micros();
        assert_eq!(pruned, items);
        assert_eq!(
            store
                .connection
                .query_row("SELECT COUNT(*) FROM mutation_intents", [], |row| row
                    .get::<_, i64>(0))
                .expect("remaining intents"),
            0
        );
        store
            .connection
            .execute_batch("PRAGMA wal_checkpoint(TRUNCATE);")
            .expect("checkpoint measurement database");
        let database_bytes = sqlite_file_bytes(store.path());
        let prepare_max = prepare_micros.iter().copied().max().unwrap_or(0);
        let prepare_p50 = percentile_micros(&mut prepare_micros.clone(), 50);
        let prepare_p95 = percentile_micros(&mut prepare_micros, 95);
        println!(
            "MUTATION_INTENT_MEASUREMENT items={items} recovered={} pruned={pruned} prune_calls={prune_calls} database_bytes={database_bytes} prepare_p50_us={prepare_p50} prepare_p95_us={prepare_p95} prepare_max_us={prepare_max} recovery_us={recovery_micros} transition_total_us={transition_micros} prune_total_us={prune_micros}",
            recovered.len()
        );

        drop(store);
        fs::remove_dir_all(root).expect("cleanup");
    }

    #[test]
    fn corrupted_request_hash_fails_closed_without_rewriting_intent() {
        let root = isolated_root("corrupt");
        let store = MutationIntentStore::open_for_identity_storage_root(&root).expect("store");
        let mutation_id = MutationId::new([4; 16]);
        store
            .persist_prepared_with_id(request("original"), mutation_id, PRODUCTION_LIMITS)
            .expect("intent");
        store
            .connection
            .execute(
                "UPDATE mutation_intents SET request_hash = ?2 WHERE mutation_id = ?1",
                (mutation_id.as_bytes().as_slice(), [9u8; 32].as_slice()),
            )
            .expect("corrupt hash");
        let error = store.load(mutation_id).expect_err("corruption must fail");
        assert!(error.to_string().contains("hash does not match"));
        let stored: Vec<u8> = store
            .connection
            .query_row(
                "SELECT request_hash FROM mutation_intents WHERE mutation_id = ?1",
                [mutation_id.as_bytes().as_slice()],
                |row| row.get(0),
            )
            .expect("stored hash");
        assert_eq!(stored, vec![9; 32]);
        drop(store);
        fs::remove_dir_all(root).expect("cleanup");
    }

    #[test]
    fn oversized_stored_frame_is_rejected_by_recovery_preflight() {
        let root = isolated_root("oversized-recovery");
        let store = MutationIntentStore::open_for_identity_storage_root(&root).expect("store");
        let mutation_id = MutationId::new([6; 16]);
        store
            .persist_prepared_with_id(request("original"), mutation_id, PRODUCTION_LIMITS)
            .expect("intent");
        store
            .connection
            .execute(
                "UPDATE mutation_intents
                 SET request_frame = zeroblob(?2), retained_bytes = ?2
                 WHERE mutation_id = ?1",
                (
                    mutation_id.as_bytes().as_slice(),
                    MAX_OUTBOUND_MUTATION_INTENT_BYTES_EACH as i64 + 1,
                ),
            )
            .expect("oversize frame");
        let error = store
            .load(mutation_id)
            .expect_err("oversized recovery must fail");
        assert!(error.to_string().contains("fixed recovery bounds"));
        let retained: i64 = store
            .connection
            .query_row(
                "SELECT length(request_frame) FROM mutation_intents WHERE mutation_id = ?1",
                [mutation_id.as_bytes().as_slice()],
                |row| row.get(0),
            )
            .expect("stored oversized frame");
        assert_eq!(retained, MAX_OUTBOUND_MUTATION_INTENT_BYTES_EACH as i64 + 1);
        drop(store);
        fs::remove_dir_all(root).expect("cleanup");
    }

    #[cfg(unix)]
    #[test]
    fn symlinked_or_permissive_database_is_rejected() {
        use std::os::unix::fs::{symlink, PermissionsExt};

        let root = isolated_root("unsafe");
        let directory = root.join("omenchat");
        fs::create_dir(&directory).expect("intent directory");
        fs::set_permissions(&directory, fs::Permissions::from_mode(0o700)).expect("directory mode");
        let target = root.join("target.sqlite");
        fs::write(&target, b"untouched").expect("target");
        let database = directory.join(INTENT_DATABASE);
        symlink(&target, &database).expect("database symlink");
        assert!(MutationIntentStore::open_for_identity_storage_root(&root).is_err());
        assert_eq!(fs::read(&target).expect("target remains"), b"untouched");

        fs::remove_file(&database).expect("remove symlink");
        fs::write(&database, []).expect("database");
        fs::set_permissions(&database, fs::Permissions::from_mode(0o644)).expect("database mode");
        assert!(MutationIntentStore::open_for_identity_storage_root(&root).is_err());
        fs::remove_dir_all(root).expect("cleanup");
    }
}
