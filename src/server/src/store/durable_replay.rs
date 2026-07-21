use rusqlite::OptionalExtension;

use super::{append_event_in_transaction, OmenchatStore, ServerRoomEvent, ServerRoomEventKind};
use crate::error::{ServerError, ServerResult};
use crate::protocol::codec::decode_frame;
use crate::protocol::{ClientInstanceId, MutationId, RequestHash, RoomId, UserId};

/// Maximum encoded origin response retained for one durable mutation.
pub const MAX_DURABLE_RESULT_BYTES: usize = 64 * 1024;
const MAX_IDENTITY_HASH_BYTES: usize = 64;
const RETENTION_AGE_SECONDS: i64 = 30 * 24 * 60 * 60;
const MAX_GLOBAL_ITEMS: i64 = 100_000;
const MAX_GLOBAL_BYTES: i64 = 64 * 1024 * 1024;
const MAX_IDENTITY_ITEMS: i64 = 10_000;
const MAX_IDENTITY_BYTES: i64 = 8 * 1024 * 1024;
const MAX_GLOBAL_CLIENT_INSTANCES: i64 = 100_000;
const MAX_IDENTITY_CLIENT_INSTANCES: i64 = 1_024;
const MAX_PRUNED_PER_COMMIT: usize = 128;

#[derive(Clone, Copy)]
/// Replay identity scoped to one authenticated Reticulum identity and client.
pub struct DurableMutationKey<'a> {
    pub identity_hash: &'a [u8],
    pub client_instance_id: ClientInstanceId,
    pub mutation_id: MutationId,
}

#[derive(Clone, Debug, PartialEq, Eq)]
/// Result of atomically admitting a durable mutation result.
pub enum DurableReplayCommit {
    Stored {
        result_frame: Vec<u8>,
        pruned: usize,
    },
    Replayed {
        result_frame: Vec<u8>,
    },
    Conflict,
    Expired,
}

#[derive(Clone, Debug, PartialEq, Eq)]
/// Result of atomically appending one room event and retaining its origin reply.
/// Replays intentionally omit the event so callers cannot broadcast it twice.
pub enum DurableRoomEventCommit<A> {
    Stored {
        result_frame: Vec<u8>,
        event: ServerRoomEvent,
        admission: A,
        pruned: usize,
    },
    Replayed {
        result_frame: Vec<u8>,
    },
    Conflict,
    Expired,
}

#[derive(Clone, Copy)]
struct RetentionLimits {
    age_seconds: i64,
    global_items: i64,
    global_bytes: i64,
    identity_items: i64,
    identity_bytes: i64,
    global_client_instances: i64,
    identity_client_instances: i64,
    max_pruned: usize,
}

const PRODUCTION_LIMITS: RetentionLimits = RetentionLimits {
    age_seconds: RETENTION_AGE_SECONDS,
    global_items: MAX_GLOBAL_ITEMS,
    global_bytes: MAX_GLOBAL_BYTES,
    identity_items: MAX_IDENTITY_ITEMS,
    identity_bytes: MAX_IDENTITY_BYTES,
    global_client_instances: MAX_GLOBAL_CLIENT_INSTANCES,
    identity_client_instances: MAX_IDENTITY_CLIENT_INSTANCES,
    max_pruned: MAX_PRUNED_PER_COMMIT,
};

impl OmenchatStore {
    /// Appends a room event and retains its exact encoded origin response in
    /// one SQLite transaction. The finisher must be deterministic apart from
    /// acquiring its returned cancellation-safe admission guard, and must not
    /// cause irreversible external side effects. Live permission, rate, and
    /// broadcast policy remain outside this dormant persistence boundary.
    pub fn commit_durable_room_event_result<F, A>(
        &self,
        key: DurableMutationKey<'_>,
        request_hash: RequestHash,
        room_id: RoomId,
        actor_user_id: Option<UserId>,
        kind: ServerRoomEventKind,
        finish_result: F,
    ) -> ServerResult<DurableRoomEventCommit<A>>
    where
        F: FnOnce(&ServerRoomEvent) -> ServerResult<(A, Vec<u8>)>,
    {
        let mut stored_event = None;
        let mut stored_admission = None;
        let commit = self.commit_durable_mutation_result(key, request_hash, |transaction| {
            let event = append_event_in_transaction(transaction, room_id, actor_user_id, kind)?;
            let (admission, result_frame) = finish_result(&event)?;
            stored_event = Some(event);
            stored_admission = Some(admission);
            Ok(result_frame)
        })?;
        match commit {
            DurableReplayCommit::Stored {
                result_frame,
                pruned,
            } => Ok(DurableRoomEventCommit::Stored {
                result_frame,
                event: stored_event.ok_or_else(|| {
                    ServerError::Message(
                        "durable room event committed without its stored event".into(),
                    )
                })?,
                admission: stored_admission.ok_or_else(|| {
                    ServerError::Message(
                        "durable room event committed without its admission guard".into(),
                    )
                })?,
                pruned,
            }),
            DurableReplayCommit::Replayed { result_frame } => {
                Ok(DurableRoomEventCommit::Replayed { result_frame })
            }
            DurableReplayCommit::Conflict => Ok(DurableRoomEventCommit::Conflict),
            DurableReplayCommit::Expired => Ok(DurableRoomEventCommit::Expired),
        }
    }

    /// Runs a future durable mutation and publishes its exact encoded response
    /// in the same SQLite transaction. The callback must perform SQLite work
    /// only through the supplied transaction and must not cause external side
    /// effects; otherwise rollback cannot preserve the durability contract.
    ///
    /// This boundary is intentionally not connected to live session handling
    /// until negotiation, client intents, and expired-key behavior are ready.
    pub fn commit_durable_mutation_result<F>(
        &self,
        key: DurableMutationKey<'_>,
        request_hash: RequestHash,
        build_result: F,
    ) -> ServerResult<DurableReplayCommit>
    where
        F: FnOnce(&rusqlite::Transaction<'_>) -> ServerResult<Vec<u8>>,
    {
        self.commit_durable_mutation_result_with_limits(
            key,
            request_hash,
            super::current_unix_seconds(),
            PRODUCTION_LIMITS,
            build_result,
        )
    }

    fn commit_durable_mutation_result_with_limits<F>(
        &self,
        key: DurableMutationKey<'_>,
        request_hash: RequestHash,
        now: i64,
        limits: RetentionLimits,
        build_result: F,
    ) -> ServerResult<DurableReplayCommit>
    where
        F: FnOnce(&rusqlite::Transaction<'_>) -> ServerResult<Vec<u8>>,
    {
        validate_key(key)?;
        validate_limits(limits)?;

        let transaction = rusqlite::Transaction::new_unchecked(
            &self.connection,
            rusqlite::TransactionBehavior::Immediate,
        )?;
        if ensure_client_instance(&transaction, key, now, limits)? {
            transaction.commit()?;
            return Ok(DurableReplayCommit::Expired);
        }
        let existing = transaction
            .query_row(
                "SELECT request_hash, result_frame
                 FROM durable_mutation_results
                 WHERE identity_hash = ?1 AND client_instance_id = ?2 AND mutation_id = ?3",
                (
                    key.identity_hash,
                    key.client_instance_id.as_bytes().as_slice(),
                    key.mutation_id.as_bytes().as_slice(),
                ),
                |row| Ok((row.get::<_, Vec<u8>>(0)?, row.get::<_, Vec<u8>>(1)?)),
            )
            .optional()?;

        if let Some((stored_hash, result_frame)) = existing {
            let stored_hash = RequestHash::try_from(stored_hash.as_slice()).map_err(|_| {
                ServerError::Message(
                    "stored durable mutation request hash is invalid; refusing replay".into(),
                )
            })?;
            if stored_hash != request_hash {
                transaction.commit()?;
                return Ok(DurableReplayCommit::Conflict);
            }
            validate_result_frame(&result_frame)?;
            transaction.execute(
                "UPDATE durable_mutation_results SET last_seen_at = ?4
                 WHERE identity_hash = ?1 AND client_instance_id = ?2 AND mutation_id = ?3",
                (
                    key.identity_hash,
                    key.client_instance_id.as_bytes().as_slice(),
                    key.mutation_id.as_bytes().as_slice(),
                    now,
                ),
            )?;
            transaction.execute(
                "UPDATE durable_mutation_clients SET last_seen_at = ?3
                 WHERE identity_hash = ?1 AND client_instance_id = ?2",
                (
                    key.identity_hash,
                    key.client_instance_id.as_bytes().as_slice(),
                    now,
                ),
            )?;
            transaction.commit()?;
            return Ok(DurableReplayCommit::Replayed { result_frame });
        }

        let reserved_bytes = retained_bytes(key.identity_hash.len(), MAX_DURABLE_RESULT_BYTES)?;
        let pruned =
            prune_for_admission(&transaction, key.identity_hash, reserved_bytes, now, limits)?;
        if client_instance_is_retired(&transaction, key)? {
            transaction.commit()?;
            return Ok(DurableReplayCommit::Expired);
        }

        let result_frame = build_result(&transaction)?;
        validate_result_frame(&result_frame)?;
        let retained_bytes = retained_bytes(key.identity_hash.len(), result_frame.len())?;

        transaction.execute(
            "INSERT INTO durable_mutation_results(
               identity_hash, client_instance_id, mutation_id, request_hash,
               result_frame, retained_bytes, created_at, last_seen_at
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?7)",
            (
                key.identity_hash,
                key.client_instance_id.as_bytes().as_slice(),
                key.mutation_id.as_bytes().as_slice(),
                request_hash.as_bytes().as_slice(),
                result_frame.as_slice(),
                retained_bytes,
                now,
            ),
        )?;
        transaction.commit()?;
        Ok(DurableReplayCommit::Stored {
            result_frame,
            pruned,
        })
    }
}

fn validate_key(key: DurableMutationKey<'_>) -> ServerResult<()> {
    if key.identity_hash.is_empty() || key.identity_hash.len() > MAX_IDENTITY_HASH_BYTES {
        return Err(ServerError::Message(format!(
            "durable mutation identity hash must contain 1..={MAX_IDENTITY_HASH_BYTES} bytes"
        )));
    }
    Ok(())
}

fn validate_limits(limits: RetentionLimits) -> ServerResult<()> {
    if limits.age_seconds < 0
        || limits.global_items <= 0
        || limits.global_bytes <= 0
        || limits.identity_items <= 0
        || limits.identity_bytes <= 0
        || limits.global_client_instances <= 0
        || limits.identity_client_instances <= 0
        || limits.max_pruned > MAX_PRUNED_PER_COMMIT
    {
        return Err(ServerError::Message(
            "durable mutation replay retention limits are invalid".into(),
        ));
    }
    Ok(())
}

fn validate_result_frame(result_frame: &[u8]) -> ServerResult<()> {
    if result_frame.is_empty() || result_frame.len() > MAX_DURABLE_RESULT_BYTES {
        return Err(ServerError::Message(format!(
            "durable mutation result must contain 1..={MAX_DURABLE_RESULT_BYTES} bytes"
        )));
    }
    decode_frame(result_frame).map_err(|_| {
        ServerError::Message("durable mutation result is not a valid bounded OMENchat frame".into())
    })?;
    Ok(())
}

fn retained_bytes(identity_bytes: usize, result_bytes: usize) -> ServerResult<i64> {
    identity_bytes
        .checked_add(16 + 16 + 32)
        .and_then(|bytes| bytes.checked_add(result_bytes))
        .and_then(|bytes| i64::try_from(bytes).ok())
        .ok_or_else(|| ServerError::Message("durable mutation retained-byte overflow".into()))
}

fn ensure_client_instance(
    transaction: &rusqlite::Transaction<'_>,
    key: DurableMutationKey<'_>,
    now: i64,
    limits: RetentionLimits,
) -> ServerResult<bool> {
    let existing = transaction
        .query_row(
            "SELECT retired_at FROM durable_mutation_clients
             WHERE identity_hash = ?1 AND client_instance_id = ?2",
            (
                key.identity_hash,
                key.client_instance_id.as_bytes().as_slice(),
            ),
            |row| row.get::<_, Option<i64>>(0),
        )
        .optional()?;
    if let Some(retired_at) = existing {
        transaction.execute(
            "UPDATE durable_mutation_clients SET last_seen_at = ?3
             WHERE identity_hash = ?1 AND client_instance_id = ?2",
            (
                key.identity_hash,
                key.client_instance_id.as_bytes().as_slice(),
                now,
            ),
        )?;
        return Ok(retired_at.is_some());
    }

    let global: i64 =
        transaction.query_row("SELECT COUNT(*) FROM durable_mutation_clients", [], |row| {
            row.get(0)
        })?;
    let identity: i64 = transaction.query_row(
        "SELECT COUNT(*) FROM durable_mutation_clients WHERE identity_hash = ?1",
        [key.identity_hash],
        |row| row.get(0),
    )?;
    if global.saturating_add(1) > limits.global_client_instances
        || identity.saturating_add(1) > limits.identity_client_instances
    {
        return Err(ServerError::Message(
            "durable mutation client-instance capacity is exhausted".into(),
        ));
    }
    transaction.execute(
        "INSERT INTO durable_mutation_clients(
           identity_hash, client_instance_id, first_seen_at, last_seen_at, retired_at
         ) VALUES (?1, ?2, ?3, ?3, NULL)",
        (
            key.identity_hash,
            key.client_instance_id.as_bytes().as_slice(),
            now,
        ),
    )?;
    Ok(false)
}

fn client_instance_is_retired(
    transaction: &rusqlite::Transaction<'_>,
    key: DurableMutationKey<'_>,
) -> ServerResult<bool> {
    transaction
        .query_row(
            "SELECT retired_at IS NOT NULL FROM durable_mutation_clients
             WHERE identity_hash = ?1 AND client_instance_id = ?2",
            (
                key.identity_hash,
                key.client_instance_id.as_bytes().as_slice(),
            ),
            |row| row.get(0),
        )
        .map_err(Into::into)
}

fn prune_for_admission(
    transaction: &rusqlite::Transaction<'_>,
    identity_hash: &[u8],
    incoming_bytes: i64,
    now: i64,
    limits: RetentionLimits,
) -> ServerResult<usize> {
    let cutoff = now.saturating_sub(limits.age_seconds);
    let mut pruned = 0usize;

    while pruned < limits.max_pruned {
        let removed = retire_and_delete_oldest(transaction, None, Some(cutoff), now)?;
        if removed == 0 {
            break;
        }
        pruned += removed;
    }

    loop {
        let (global_items, global_bytes) = replay_usage(transaction, None)?;
        let (identity_items, identity_bytes) = replay_usage(transaction, Some(identity_hash))?;
        let identity_over = identity_items.saturating_add(1) > limits.identity_items
            || identity_bytes.saturating_add(incoming_bytes) > limits.identity_bytes;
        let global_over = global_items.saturating_add(1) > limits.global_items
            || global_bytes.saturating_add(incoming_bytes) > limits.global_bytes;
        if !identity_over && !global_over {
            return Ok(pruned);
        }
        if pruned >= limits.max_pruned {
            return Err(ServerError::Message(
                "durable mutation replay retention capacity is exhausted".into(),
            ));
        }

        let removed = if identity_over {
            retire_and_delete_oldest(transaction, Some(identity_hash), None, now)?
        } else {
            retire_and_delete_oldest(transaction, None, None, now)?
        };
        if removed == 0 {
            return Err(ServerError::Message(
                "durable mutation replay retention capacity is exhausted".into(),
            ));
        }
        pruned += removed;
    }
}

fn retire_and_delete_oldest(
    transaction: &rusqlite::Transaction<'_>,
    identity_hash: Option<&[u8]>,
    older_than: Option<i64>,
    now: i64,
) -> ServerResult<usize> {
    let candidate = match (identity_hash, older_than) {
        (Some(identity_hash), None) => transaction
            .query_row(
                "SELECT identity_hash, client_instance_id, mutation_id
                 FROM durable_mutation_results WHERE identity_hash = ?1
                 ORDER BY created_at, client_instance_id, mutation_id LIMIT 1",
                [identity_hash],
                replay_candidate,
            )
            .optional()?,
        (None, Some(cutoff)) => transaction
            .query_row(
                "SELECT identity_hash, client_instance_id, mutation_id
                 FROM durable_mutation_results WHERE created_at < ?1
                 ORDER BY created_at, identity_hash, client_instance_id, mutation_id LIMIT 1",
                [cutoff],
                replay_candidate,
            )
            .optional()?,
        (None, None) => transaction
            .query_row(
                "SELECT identity_hash, client_instance_id, mutation_id
                 FROM durable_mutation_results
                 ORDER BY created_at, identity_hash, client_instance_id, mutation_id LIMIT 1",
                [],
                replay_candidate,
            )
            .optional()?,
        (Some(_), Some(_)) => {
            return Err(ServerError::Message(
                "durable mutation prune selector is invalid".into(),
            ))
        }
    };
    let Some((identity_hash, client_instance_id, mutation_id)) = candidate else {
        return Ok(0);
    };
    if identity_hash.is_empty() || identity_hash.len() > MAX_IDENTITY_HASH_BYTES {
        return Err(ServerError::Message(
            "stored durable mutation identity hash is invalid; refusing retention work".into(),
        ));
    }
    let client_instance_id =
        ClientInstanceId::try_from(client_instance_id.as_slice()).map_err(|_| {
            ServerError::Message(
                "stored durable mutation client instance is invalid; refusing retention work"
                    .into(),
            )
        })?;
    let mutation_id = MutationId::try_from(mutation_id.as_slice()).map_err(|_| {
        ServerError::Message(
            "stored durable mutation identifier is invalid; refusing retention work".into(),
        )
    })?;
    transaction.execute(
        "INSERT INTO durable_mutation_clients(
           identity_hash, client_instance_id, first_seen_at, last_seen_at, retired_at
         ) VALUES (?1, ?2, ?3, ?3, ?3)
         ON CONFLICT(identity_hash, client_instance_id) DO UPDATE SET
           last_seen_at = MAX(durable_mutation_clients.last_seen_at, excluded.last_seen_at),
           retired_at = COALESCE(durable_mutation_clients.retired_at, excluded.retired_at)",
        (
            identity_hash.as_slice(),
            client_instance_id.as_bytes().as_slice(),
            now,
        ),
    )?;
    transaction
        .execute(
            "DELETE FROM durable_mutation_results
             WHERE identity_hash = ?1 AND client_instance_id = ?2 AND mutation_id = ?3",
            (
                identity_hash.as_slice(),
                client_instance_id.as_bytes().as_slice(),
                mutation_id.as_bytes().as_slice(),
            ),
        )
        .map_err(Into::into)
}

fn replay_candidate(row: &rusqlite::Row<'_>) -> rusqlite::Result<(Vec<u8>, Vec<u8>, Vec<u8>)> {
    Ok((row.get(0)?, row.get(1)?, row.get(2)?))
}

fn replay_usage(
    transaction: &rusqlite::Transaction<'_>,
    identity_hash: Option<&[u8]>,
) -> ServerResult<(i64, i64)> {
    let usage = match identity_hash {
        Some(identity_hash) => transaction.query_row(
            "SELECT COUNT(*), COALESCE(SUM(retained_bytes), 0)
             FROM durable_mutation_results WHERE identity_hash = ?1",
            [identity_hash],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )?,
        None => transaction.query_row(
            "SELECT COUNT(*), COALESCE(SUM(retained_bytes), 0)
             FROM durable_mutation_results",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )?,
    };
    Ok(usage)
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;
    use std::rc::Rc;
    use std::time::Instant;

    use super::*;
    use crate::protocol::codec::encode_frame;
    use crate::protocol::{ChatOp, Frame, FrameBody, FrameValue};

    fn key_with_client<'a>(
        identity_hash: &'a [u8],
        client_marker: u8,
        mutation_marker: u8,
    ) -> DurableMutationKey<'a> {
        DurableMutationKey {
            identity_hash,
            client_instance_id: ClientInstanceId::new([client_marker; 16]),
            mutation_id: MutationId::new([mutation_marker; 16]),
        }
    }

    fn key<'a>(identity_hash: &'a [u8], marker: u8) -> DurableMutationKey<'a> {
        key_with_client(identity_hash, 7, marker)
    }

    fn request_hash(marker: u8) -> RequestHash {
        RequestHash::new([marker; 32])
    }

    fn result_frame(marker: &str) -> Vec<u8> {
        encode_frame(&Frame::new(
            ChatOp::MessageAck,
            1,
            Some(1),
            FrameBody::Text(marker.into()),
        ))
        .expect("encoded result")
    }

    fn room_event_result(seq: u32, event: &ServerRoomEvent) -> ServerResult<Vec<u8>> {
        let kind = match event.kind {
            ServerRoomEventKind::Message { .. } => 1,
            ServerRoomEventKind::Action { .. } => 2,
            _ => return Err(ServerError::Message("unexpected test event kind".into())),
        };
        encode_frame(&Frame::new(
            ChatOp::MessageAck,
            seq,
            Some(event.room_id),
            FrameBody::Fields(vec![
                FrameValue::U64(event.event_id),
                FrameValue::U64(kind),
                event
                    .actor_user_id
                    .map(|user_id| FrameValue::U64(user_id as u64))
                    .unwrap_or(FrameValue::Nil),
                FrameValue::I64(event.at_unix),
                event
                    .actor_display_name
                    .clone()
                    .map(FrameValue::String)
                    .unwrap_or(FrameValue::Nil),
            ]),
        ))
        .map_err(|error| ServerError::Message(error.to_string()))
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

    fn sqlite_file_bytes(path: &std::path::Path) -> u64 {
        [
            path.to_path_buf(),
            path.with_extension("sqlite-wal"),
            path.with_extension("sqlite-shm"),
        ]
        .into_iter()
        .filter_map(|candidate| std::fs::metadata(candidate).ok())
        .map(|metadata| metadata.len())
        .sum()
    }

    #[test]
    fn exact_duplicate_replays_once_and_different_hash_conflicts() {
        let store = OmenchatStore::in_memory().expect("store");
        let identity = [1; 16];
        let calls = Cell::new(0);
        let expected = result_frame("original");

        let stored = store
            .commit_durable_mutation_result(key(&identity, 1), request_hash(2), |transaction| {
                calls.set(calls.get() + 1);
                transaction.execute(
                    "INSERT INTO server_config(key, value) VALUES ('effect', 'once')",
                    [],
                )?;
                Ok(expected.clone())
            })
            .expect("stored result");
        assert!(matches!(
            stored,
            DurableReplayCommit::Stored { pruned: 0, .. }
        ));

        let replayed = store
            .commit_durable_mutation_result(key(&identity, 1), request_hash(2), |_| {
                panic!("exact duplicate must not execute again")
            })
            .expect("replayed result");
        assert_eq!(
            replayed,
            DurableReplayCommit::Replayed {
                result_frame: expected
            }
        );

        let conflict = store
            .commit_durable_mutation_result(key(&identity, 1), request_hash(3), |_| {
                panic!("conflict must not execute")
            })
            .expect("conflict result");
        assert_eq!(conflict, DurableReplayCommit::Conflict);
        assert_eq!(calls.get(), 1);
    }

    #[test]
    fn durable_room_event_is_atomic_and_replay_cannot_be_rebroadcast() {
        let store = OmenchatStore::in_memory().expect("store");
        let room = store.ensure_room("durable", None).expect("room");
        let user = store
            .ensure_user(&[9; 16], "Durable User", None)
            .expect("user");
        let identity = [4; 16];

        let stored = store
            .commit_durable_room_event_result(
                key(&identity, 1),
                request_hash(1),
                room.room_id,
                Some(user.user_id),
                ServerRoomEventKind::Message {
                    body: "exactly once".into(),
                },
                |event| Ok(((), room_event_result(17, event)?)),
            )
            .expect("stored room event");
        let (stored_result, stored_event) = match stored {
            DurableRoomEventCommit::Stored {
                result_frame,
                event,
                admission: (),
                pruned: 0,
            } => (result_frame, event),
            other => panic!("unexpected first result: {other:?}"),
        };
        assert_eq!(
            stored_event.actor_display_name.as_deref(),
            Some("Durable User")
        );

        let replayed = store
            .commit_durable_room_event_result(
                key(&identity, 1),
                request_hash(1),
                room.room_id,
                Some(user.user_id),
                ServerRoomEventKind::Message {
                    body: "exactly once".into(),
                },
                |_| -> ServerResult<((), Vec<u8>)> {
                    panic!("an exact replay must not append or encode another event")
                },
            )
            .expect("replayed room event");
        assert_eq!(
            replayed,
            DurableRoomEventCommit::Replayed {
                result_frame: stored_result
            }
        );
        let events = store.latest_events(room.room_id, 10).expect("events");
        assert_eq!(events, vec![stored_event]);

        let conflict = store
            .commit_durable_room_event_result(
                key(&identity, 1),
                request_hash(2),
                room.room_id,
                Some(user.user_id),
                ServerRoomEventKind::Message {
                    body: "different content".into(),
                },
                |_| -> ServerResult<((), Vec<u8>)> {
                    panic!("a conflicting replay must not execute")
                },
            )
            .expect("conflicting room event");
        assert_eq!(conflict, DurableRoomEventCommit::Conflict);
        assert_eq!(
            store.latest_events(room.room_id, 10).expect("events").len(),
            1
        );
    }

    #[test]
    fn durable_room_event_rolls_back_when_origin_response_cannot_be_retained() {
        #[derive(Debug)]
        struct DropMarker(Rc<Cell<usize>>);

        impl Drop for DropMarker {
            fn drop(&mut self) {
                self.0.set(self.0.get() + 1);
            }
        }

        let store = OmenchatStore::in_memory().expect("store");
        let room = store.ensure_room("rollback", None).expect("room");
        let identity = [6; 16];
        let drops = Rc::new(Cell::new(0));
        let marker_drops = Rc::clone(&drops);
        let error = store
            .commit_durable_room_event_result(
                key(&identity, 1),
                request_hash(1),
                room.room_id,
                None,
                ServerRoomEventKind::Action {
                    body: "must rollback".into(),
                },
                |_| Ok((DropMarker(marker_drops), vec![0xc0])),
            )
            .expect_err("invalid origin response must roll back event");
        assert!(error.to_string().contains("valid bounded OMENchat frame"));
        assert_eq!(drops.get(), 1, "rollback must release admission guard");
        assert!(store
            .latest_events(room.room_id, 10)
            .expect("events")
            .is_empty());
    }

    #[test]
    fn concurrent_connections_execute_one_exact_mutation() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        use std::sync::{Arc, Barrier};

        static DATABASE_NONCE: AtomicUsize = AtomicUsize::new(0);
        let nonce = DATABASE_NONCE.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "omenchat-durable-replay-concurrent-{}-{nonce}.sqlite",
            std::process::id()
        ));
        drop(OmenchatStore::open(&path).expect("setup store"));

        let barrier = Arc::new(Barrier::new(2));
        let executions = Arc::new(AtomicUsize::new(0));
        let mut threads = Vec::new();
        for _ in 0..2 {
            let barrier = Arc::clone(&barrier);
            let executions = Arc::clone(&executions);
            let path = path.clone();
            threads.push(std::thread::spawn(move || {
                let store = OmenchatStore::open(&path).expect("concurrent store");
                let identity = [5; 16];
                barrier.wait();
                store
                    .commit_durable_mutation_result(key(&identity, 1), request_hash(1), |_| {
                        executions.fetch_add(1, Ordering::SeqCst);
                        Ok(result_frame("one result"))
                    })
                    .expect("concurrent commit")
            }));
        }
        let outcomes: Vec<_> = threads
            .into_iter()
            .map(|thread| thread.join().expect("concurrent thread"))
            .collect();

        assert_eq!(executions.load(Ordering::SeqCst), 1);
        assert_eq!(
            outcomes
                .iter()
                .filter(|outcome| matches!(outcome, DurableReplayCommit::Stored { .. }))
                .count(),
            1
        );
        assert_eq!(
            outcomes
                .iter()
                .filter(|outcome| matches!(outcome, DurableReplayCommit::Replayed { .. }))
                .count(),
            1
        );

        for candidate in [
            path.clone(),
            path.with_extension("sqlite-wal"),
            path.with_extension("sqlite-shm"),
        ] {
            let _ = std::fs::remove_file(candidate);
        }
    }

    #[test]
    fn invalid_result_rolls_back_mutation_and_replay_record() {
        let store = OmenchatStore::in_memory().expect("store");
        let identity = [2; 16];
        let error = store
            .commit_durable_mutation_result(key(&identity, 1), request_hash(1), |transaction| {
                transaction.execute(
                    "INSERT INTO server_config(key, value) VALUES ('must-rollback', 'yes')",
                    [],
                )?;
                Ok(vec![0xc0])
            })
            .expect_err("invalid result must fail");
        assert!(error.to_string().contains("valid bounded OMENchat frame"));

        let effects: i64 = store
            .connection
            .query_row(
                "SELECT COUNT(*) FROM server_config WHERE key = 'must-rollback'",
                [],
                |row| row.get(0),
            )
            .expect("effect count");
        let replay_rows: i64 = store
            .connection
            .query_row("SELECT COUNT(*) FROM durable_mutation_results", [], |row| {
                row.get(0)
            })
            .expect("replay count");
        assert_eq!(effects, 0);
        assert_eq!(replay_rows, 0);
    }

    #[test]
    fn oversized_result_is_rejected_before_replay_publication() {
        let store = OmenchatStore::in_memory().expect("store");
        let identity = [8; 16];
        let error = store
            .commit_durable_mutation_result(key(&identity, 1), request_hash(1), |_| {
                Ok(vec![0; MAX_DURABLE_RESULT_BYTES + 1])
            })
            .expect_err("oversized result must fail");
        assert!(error
            .to_string()
            .contains("durable mutation result must contain"));
        let replay_rows: i64 = store
            .connection
            .query_row("SELECT COUNT(*) FROM durable_mutation_results", [], |row| {
                row.get(0)
            })
            .expect("replay count");
        assert_eq!(replay_rows, 0);
    }

    #[test]
    fn retention_prunes_incrementally_and_never_exceeds_work_ceiling() {
        let store = OmenchatStore::in_memory().expect("store");
        let identity = [3; 16];
        let limits = RetentionLimits {
            age_seconds: 10,
            global_items: 10,
            global_bytes: 1_000_000,
            identity_items: 10,
            identity_bytes: 1_000_000,
            global_client_instances: 100,
            identity_client_instances: 10,
            max_pruned: 2,
        };
        for marker in 1..=3u8 {
            store
                .connection
                .execute(
                    "INSERT INTO durable_mutation_results(
                       identity_hash, client_instance_id, mutation_id, request_hash,
                       result_frame, retained_bytes, created_at, last_seen_at
                     ) VALUES (?1, ?2, ?3, ?4, ?5, 100, 1, 1)",
                    (
                        identity.as_slice(),
                        [7u8; 16].as_slice(),
                        [marker; 16].as_slice(),
                        [marker; 32].as_slice(),
                        result_frame("old"),
                    ),
                )
                .expect("old replay row");
        }

        let committed = store
            .commit_durable_mutation_result_with_limits(
                key_with_client(&identity, 8, 9),
                request_hash(9),
                100,
                limits,
                |_| Ok(result_frame("new")),
            )
            .expect("bounded prune commit");
        assert!(matches!(
            committed,
            DurableReplayCommit::Stored { pruned: 2, .. }
        ));
        let old_rows: i64 = store
            .connection
            .query_row(
                "SELECT COUNT(*) FROM durable_mutation_results WHERE created_at = 1",
                [],
                |row| row.get(0),
            )
            .expect("old row count");
        assert_eq!(
            old_rows, 1,
            "one old row remains for later incremental work"
        );
        let retired: i64 = store
            .connection
            .query_row(
                "SELECT COUNT(*) FROM durable_mutation_clients
                 WHERE identity_hash = ?1 AND client_instance_id = ?2 AND retired_at IS NOT NULL",
                (identity.as_slice(), [7u8; 16].as_slice()),
                |row| row.get(0),
            )
            .expect("retired client count");
        assert_eq!(retired, 1);
    }

    #[test]
    fn capacity_failure_rolls_back_pruning_and_mutation() {
        let store = OmenchatStore::in_memory().expect("store");
        let identity = [4; 16];
        let limits = RetentionLimits {
            age_seconds: 1_000,
            global_items: 1,
            global_bytes: 1_000_000,
            identity_items: 1,
            identity_bytes: 1_000_000,
            global_client_instances: 100,
            identity_client_instances: 10,
            max_pruned: 0,
        };
        store
            .commit_durable_mutation_result_with_limits(
                key(&identity, 1),
                request_hash(1),
                100,
                RetentionLimits {
                    max_pruned: 1,
                    ..limits
                },
                |_| Ok(result_frame("first")),
            )
            .expect("first result");

        let error = store
            .commit_durable_mutation_result_with_limits(
                key(&identity, 2),
                request_hash(2),
                101,
                limits,
                |transaction| {
                    transaction.execute(
                        "INSERT INTO server_config(key, value) VALUES ('capacity-effect', 'no')",
                        [],
                    )?;
                    Ok(result_frame("second"))
                },
            )
            .expect_err("capacity must reject");
        assert!(error.to_string().contains("capacity is exhausted"));
        let replay_rows: i64 = store
            .connection
            .query_row("SELECT COUNT(*) FROM durable_mutation_results", [], |row| {
                row.get(0)
            })
            .expect("replay count");
        let effects: i64 = store
            .connection
            .query_row(
                "SELECT COUNT(*) FROM server_config WHERE key = 'capacity-effect'",
                [],
                |row| row.get(0),
            )
            .expect("effect count");
        assert_eq!(replay_rows, 1);
        assert_eq!(effects, 0);
    }

    #[test]
    fn pruned_client_instance_stays_expired_after_restart() {
        use std::sync::atomic::{AtomicUsize, Ordering};

        static DATABASE_NONCE: AtomicUsize = AtomicUsize::new(0);
        let nonce = DATABASE_NONCE.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "omenchat-durable-replay-expired-{}-{nonce}.sqlite",
            std::process::id()
        ));
        let identity = [9; 16];
        let limits = RetentionLimits {
            age_seconds: 10,
            global_items: 10,
            global_bytes: 1_000_000,
            identity_items: 10,
            identity_bytes: 1_000_000,
            global_client_instances: 100,
            identity_client_instances: 10,
            max_pruned: 2,
        };

        {
            let store = OmenchatStore::open(&path).expect("initial store");
            store
                .commit_durable_mutation_result_with_limits(
                    key_with_client(&identity, 7, 1),
                    request_hash(1),
                    1,
                    RetentionLimits {
                        age_seconds: 1_000,
                        ..limits
                    },
                    |_| Ok(result_frame("old result")),
                )
                .expect("old result");
            let replacement = store
                .commit_durable_mutation_result_with_limits(
                    key_with_client(&identity, 8, 1),
                    request_hash(2),
                    100,
                    limits,
                    |_| Ok(result_frame("replacement")),
                )
                .expect("replacement result");
            assert!(matches!(
                replacement,
                DurableReplayCommit::Stored { pruned: 1, .. }
            ));
        }

        let store = OmenchatStore::open(&path).expect("reopened store");
        let outcome = store
            .commit_durable_mutation_result_with_limits(
                key_with_client(&identity, 7, 2),
                request_hash(3),
                101,
                limits,
                |_| panic!("retired client instance must never execute a mutation"),
            )
            .expect("expired outcome");
        assert_eq!(outcome, DurableReplayCommit::Expired);

        for candidate in [
            path.clone(),
            path.with_extension("sqlite-wal"),
            path.with_extension("sqlite-shm"),
        ] {
            let _ = std::fs::remove_file(candidate);
        }
    }

    #[test]
    fn client_instance_capacity_fails_closed_before_mutation() {
        let store = OmenchatStore::in_memory().expect("store");
        let identity = [10; 16];
        let limits = RetentionLimits {
            age_seconds: 1_000,
            global_items: 10,
            global_bytes: 1_000_000,
            identity_items: 10,
            identity_bytes: 1_000_000,
            global_client_instances: 1,
            identity_client_instances: 1,
            max_pruned: 1,
        };
        store
            .commit_durable_mutation_result_with_limits(
                key_with_client(&identity, 7, 1),
                request_hash(1),
                1,
                limits,
                |_| Ok(result_frame("first")),
            )
            .expect("first client");

        let error = store
            .commit_durable_mutation_result_with_limits(
                key_with_client(&identity, 8, 1),
                request_hash(2),
                2,
                limits,
                |_| panic!("capacity rejection must precede mutation execution"),
            )
            .expect_err("second client must exceed capacity");
        assert!(error
            .to_string()
            .contains("client-instance capacity is exhausted"));
    }

    #[test]
    #[ignore = "explicit isolated durable-replay retention measurement"]
    fn durable_replay_retention_measurement() {
        use std::sync::atomic::{AtomicUsize, Ordering};

        static DATABASE_NONCE: AtomicUsize = AtomicUsize::new(0);
        let items = std::env::var("OMEN_DURABLE_MEASUREMENT_ITEMS")
            .ok()
            .and_then(|value| value.parse::<usize>().ok())
            .unwrap_or(1_024);
        assert!((256..=4_096).contains(&items));
        let retained_items = items / 2;
        let nonce = DATABASE_NONCE.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "omenchat-durable-replay-measurement-{}-{nonce}.sqlite",
            std::process::id()
        ));
        let store = OmenchatStore::open(&path).expect("measurement store");
        let identity = [21; 16];
        let limits = RetentionLimits {
            age_seconds: i64::MAX / 4,
            global_items: retained_items as i64,
            global_bytes: i64::MAX / 4,
            identity_items: retained_items as i64,
            identity_bytes: i64::MAX / 4,
            global_client_instances: items as i64 + 1,
            identity_client_instances: items as i64 + 1,
            max_pruned: MAX_PRUNED_PER_COMMIT,
        };
        let encoded_result = result_frame("measured-result");
        let mut commit_micros = Vec::with_capacity(items);

        for index in 0..items {
            let key = DurableMutationKey {
                identity_hash: &identity,
                client_instance_id: ClientInstanceId::new((index as u128).to_be_bytes()),
                mutation_id: MutationId::new([1; 16]),
            };
            let started = Instant::now();
            store
                .commit_durable_mutation_result_with_limits(
                    key,
                    RequestHash::new([index as u8; 32]),
                    1_000 + index as i64,
                    limits,
                    |_| Ok(encoded_result.clone()),
                )
                .expect("measurement commit");
            commit_micros.push(started.elapsed().as_micros());
        }

        let last_index = items - 1;
        let last_key = DurableMutationKey {
            identity_hash: &identity,
            client_instance_id: ClientInstanceId::new((last_index as u128).to_be_bytes()),
            mutation_id: MutationId::new([1; 16]),
        };
        let mut replay_micros = Vec::with_capacity(256);
        for _ in 0..256 {
            let started = Instant::now();
            let outcome = store
                .commit_durable_mutation_result_with_limits(
                    last_key,
                    RequestHash::new([last_index as u8; 32]),
                    10_000,
                    limits,
                    |_| panic!("exact measurement replay must not execute"),
                )
                .expect("measurement replay");
            assert!(matches!(outcome, DurableReplayCommit::Replayed { .. }));
            replay_micros.push(started.elapsed().as_micros());
        }

        let result_rows: i64 = store
            .connection
            .query_row("SELECT COUNT(*) FROM durable_mutation_results", [], |row| {
                row.get(0)
            })
            .expect("result rows");
        let (client_rows, retired_rows): (i64, i64) = store
            .connection
            .query_row(
                "SELECT COUNT(*), COUNT(retired_at) FROM durable_mutation_clients",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .expect("client rows");
        store
            .connection
            .execute_batch("PRAGMA wal_checkpoint(TRUNCATE);")
            .expect("checkpoint measurement database");
        let database_bytes = sqlite_file_bytes(&path);
        let commit_max = commit_micros.iter().copied().max().unwrap_or(0);
        let replay_max = replay_micros.iter().copied().max().unwrap_or(0);
        let commit_p50 = percentile_micros(&mut commit_micros.clone(), 50);
        let commit_p95 = percentile_micros(&mut commit_micros, 95);
        let replay_p50 = percentile_micros(&mut replay_micros.clone(), 50);
        let replay_p95 = percentile_micros(&mut replay_micros, 95);

        assert_eq!(result_rows, retained_items as i64);
        assert_eq!(client_rows, items as i64);
        assert_eq!(retired_rows, (items - retained_items) as i64);
        println!(
            "DURABLE_REPLAY_MEASUREMENT items={items} retained_items={retained_items} result_rows={result_rows} client_rows={client_rows} retired_rows={retired_rows} database_bytes={database_bytes} commit_p50_us={commit_p50} commit_p95_us={commit_p95} commit_max_us={commit_max} replay_p50_us={replay_p50} replay_p95_us={replay_p95} replay_max_us={replay_max}"
        );

        drop(store);
        for candidate in [
            path.clone(),
            path.with_extension("sqlite-wal"),
            path.with_extension("sqlite-shm"),
        ] {
            let _ = std::fs::remove_file(candidate);
        }
    }
}
