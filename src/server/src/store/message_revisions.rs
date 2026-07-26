use rusqlite::OptionalExtension;

use super::{current_unix_seconds, OmenchatStore};
use crate::error::{ServerError, ServerResult};
use crate::protocol::{
    EventId, MessageRevisionAction, MessageRevisionEvent, MessageRevisionRequest,
    MessageRevisionSnapshot, MessageRevisionSnapshotEntry, RoomId, UserId,
    MESSAGE_REVISION_MAX_NUMBER, MESSAGE_REVISION_SNAPSHOT_MAX_ENTRIES,
    MESSAGE_REVISION_SNAPSHOT_MAX_TARGETS,
};

pub(crate) const MAX_CORRECTIONS_PER_TARGET: u64 = 8;
pub(crate) const MAX_CORRECTION_ROWS_PER_ROOM: i64 = 3_072;
pub(crate) const MAX_CORRECTION_BYTES_PER_ROOM: i64 = 6 * 1024 * 1024;
pub(crate) const MAX_CORRECTION_ROWS_GLOBAL: i64 = 49_152;
pub(crate) const MAX_CORRECTION_BYTES_GLOBAL: i64 = 96 * 1024 * 1024;
pub(crate) const MAX_STATE_ROWS_PER_ROOM: i64 = 4_096;
pub(crate) const MAX_STATE_BYTES_PER_ROOM: i64 = 8 * 1024 * 1024;
pub(crate) const MAX_STATE_ROWS_GLOBAL: i64 = 65_536;
pub(crate) const MAX_STATE_BYTES_GLOBAL: i64 = 128 * 1024 * 1024;
pub(crate) const MAX_AUDIT_ROWS_PER_ROOM: i64 = 8_192;
pub(crate) const MAX_AUDIT_BYTES_PER_ROOM: i64 = 8 * 1024 * 1024;
pub(crate) const MAX_AUDIT_ROWS_GLOBAL: i64 = 131_072;
pub(crate) const MAX_AUDIT_BYTES_GLOBAL: i64 = 128 * 1024 * 1024;
pub(crate) const AUDIT_RETENTION_AGE_SECONDS: i64 = 365 * 24 * 60 * 60;
pub(crate) const MAX_AUDIT_PRUNED_PER_MUTATION: usize = 64;

const STATE_FIXED_RETAINED_BYTES: i64 = 48;
const AUDIT_FIXED_RETAINED_BYTES: i64 = 56;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct MessageRevisionActorPolicy {
    pub is_moderator: bool,
    pub is_muted: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct MessageRevisionMutation {
    pub event: MessageRevisionEvent,
    pub reactions_cleared: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum MessageRevisionMutationResult {
    Changed(MessageRevisionMutation),
    Unchanged,
    TargetUnavailable,
    PermissionDenied,
    AlreadyTombstoned,
    CorrectionLimitReached,
    Saturated,
}

#[derive(Clone)]
struct StoredRevisionState {
    action: MessageRevisionAction,
    replacement: Option<String>,
    revision_number: u64,
    retained_bytes: i64,
}

impl OmenchatStore {
    pub(crate) fn apply_message_revision_mutation(
        transaction: &rusqlite::Transaction<'_>,
        room_id: RoomId,
        actor_user_id: UserId,
        actor_display_name: Option<&str>,
        policy: MessageRevisionActorPolicy,
        request: MessageRevisionRequest,
        max_message_bytes: usize,
    ) -> ServerResult<MessageRevisionMutationResult> {
        apply_message_revision_mutation_at(
            transaction,
            room_id,
            actor_user_id,
            actor_display_name,
            policy,
            request,
            max_message_bytes,
            current_unix_seconds(),
        )
    }

    pub(crate) fn message_revision_snapshot(
        &self,
        room_id: RoomId,
        target_event_ids: &[EventId],
    ) -> ServerResult<MessageRevisionSnapshot> {
        validate_snapshot_targets(target_event_ids)?;
        if target_event_ids.is_empty() {
            return Ok(MessageRevisionSnapshot {
                target_event_ids: Vec::new(),
                entries: Vec::new(),
            });
        }

        let placeholders = std::iter::repeat_n("?", target_event_ids.len())
            .collect::<Vec<_>>()
            .join(",");
        let sql = format!(
            "SELECT target_event_id, latest_revision_event_id, revision_action,
                    actor_user_id, at, replacement_body, revision_number
             FROM room_message_revision_state
             WHERE room_id = ? AND target_event_id IN ({placeholders})
             ORDER BY target_event_id
             LIMIT ?"
        );
        let mut parameters = Vec::with_capacity(target_event_ids.len() + 2);
        parameters.push(i64::from(room_id));
        parameters.extend(
            target_event_ids
                .iter()
                .map(|event_id| {
                    i64::try_from(*event_id).map_err(|_| {
                        ServerError::Message(
                            "message revision snapshot target id does not fit SQLite".into(),
                        )
                    })
                })
                .collect::<ServerResult<Vec<_>>>()?,
        );
        parameters.push((MESSAGE_REVISION_SNAPSHOT_MAX_ENTRIES + 1) as i64);

        let mut statement = self.connection.prepare(&sql)?;
        let rows = statement
            .query_map(rusqlite::params_from_iter(parameters), |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, i64>(1)?,
                    row.get::<_, i64>(2)?,
                    row.get::<_, i64>(3)?,
                    row.get::<_, i64>(4)?,
                    row.get::<_, Option<Vec<u8>>>(5)?,
                    row.get::<_, i64>(6)?,
                ))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        if rows.len() > MESSAGE_REVISION_SNAPSHOT_MAX_ENTRIES {
            return Err(ServerError::Message(format!(
                "message revision snapshot exceeds {MESSAGE_REVISION_SNAPSHOT_MAX_ENTRIES} entries"
            )));
        }
        let entries = rows
            .into_iter()
            .map(
                |(
                    target_event_id,
                    latest_revision_event_id,
                    action,
                    actor_user_id,
                    at_unix,
                    replacement,
                    revision_number,
                )| {
                    Ok(MessageRevisionSnapshotEntry {
                        target_event_id: stored_event_id(target_event_id)?,
                        latest_revision_event_id: stored_event_id(latest_revision_event_id)?,
                        action: stored_action(action)?,
                        actor_user_id: u32::try_from(actor_user_id).map_err(|_| {
                            ServerError::Message(
                                "stored message revision actor user id is invalid".into(),
                            )
                        })?,
                        at_unix,
                        replacement: decode_replacement(replacement)?,
                        revision_number: u64::try_from(revision_number).map_err(|_| {
                            ServerError::Message("stored message revision number is invalid".into())
                        })?,
                    })
                },
            )
            .collect::<ServerResult<Vec<_>>>()?;
        let snapshot = MessageRevisionSnapshot {
            target_event_ids: target_event_ids.to_vec(),
            entries,
        };
        snapshot.clone().into_frame_body().map_err(|error| {
            ServerError::Message(format!(
                "stored message revision snapshot is invalid: {error}"
            ))
        })?;
        Ok(snapshot)
    }

    #[cfg(test)]
    pub(crate) fn message_revision_row_counts(&self) -> ServerResult<(i64, i64)> {
        Ok((
            self.connection.query_row(
                "SELECT COUNT(*) FROM room_message_revision_state",
                [],
                |row| row.get(0),
            )?,
            self.connection.query_row(
                "SELECT COUNT(*) FROM room_message_revision_events",
                [],
                |row| row.get(0),
            )?,
        ))
    }
}

#[allow(clippy::too_many_arguments)]
fn apply_message_revision_mutation_at(
    transaction: &rusqlite::Transaction<'_>,
    room_id: RoomId,
    actor_user_id: UserId,
    actor_display_name: Option<&str>,
    policy: MessageRevisionActorPolicy,
    request: MessageRevisionRequest,
    max_message_bytes: usize,
    now: i64,
) -> ServerResult<MessageRevisionMutationResult> {
    if request
        .replacement
        .as_ref()
        .is_some_and(|replacement| replacement.len() > max_message_bytes)
    {
        return Ok(MessageRevisionMutationResult::Saturated);
    }
    let target_event_id = i64::try_from(request.target_event_id).map_err(|_| {
        ServerError::Message("message revision target event id does not fit SQLite".into())
    })?;
    let target_actor = transaction
        .query_row(
            "SELECT actor_user_id FROM room_events
             WHERE room_id = ?1 AND event_id = ?2 AND event_kind = 1
               AND deleted = 0 AND actor_user_id IS NOT NULL",
            (room_id, target_event_id),
            |row| row.get::<_, i64>(0),
        )
        .optional()?;
    let Some(target_actor) = target_actor.and_then(|value| u32::try_from(value).ok()) else {
        return Ok(MessageRevisionMutationResult::TargetUnavailable);
    };
    let owns_target = target_actor == actor_user_id;
    let authorized = match request.action {
        MessageRevisionAction::Correct => owns_target && !policy.is_muted,
        MessageRevisionAction::Tombstone => owns_target || policy.is_moderator,
    };
    if !authorized {
        return Ok(MessageRevisionMutationResult::PermissionDenied);
    }

    let current = load_current_state(transaction, room_id, target_event_id)?;
    if current
        .as_ref()
        .is_some_and(|state| state.action == MessageRevisionAction::Tombstone)
    {
        return Ok(MessageRevisionMutationResult::AlreadyTombstoned);
    }
    if request.action == MessageRevisionAction::Correct
        && current
            .as_ref()
            .is_some_and(|state| state.revision_number >= MAX_CORRECTIONS_PER_TARGET)
    {
        return Ok(MessageRevisionMutationResult::CorrectionLimitReached);
    }

    let effective_body = match current.as_ref() {
        Some(state) => state.replacement.clone(),
        None => transaction
            .query_row(
                "SELECT payload FROM room_events
                 WHERE room_id = ?1 AND event_id = ?2",
                (room_id, target_event_id),
                |row| row.get::<_, Vec<u8>>(0),
            )
            .optional()?
            .map(|body| {
                String::from_utf8(body).map_err(|_| {
                    ServerError::Message("stored room message body is not UTF-8".into())
                })
            })
            .transpose()?,
    };
    if request.action == MessageRevisionAction::Correct
        && request.replacement.as_deref() == effective_body.as_deref()
    {
        return Ok(MessageRevisionMutationResult::Unchanged);
    }

    let revision_number = current
        .as_ref()
        .map_or(1, |state| state.revision_number.saturating_add(1));
    if revision_number > MESSAGE_REVISION_MAX_NUMBER {
        return Ok(MessageRevisionMutationResult::CorrectionLimitReached);
    }
    let replacement_bytes = request
        .replacement
        .as_ref()
        .map_or(0_i64, |value| value.len() as i64);
    let state_bytes = STATE_FIXED_RETAINED_BYTES.saturating_add(replacement_bytes);
    if !state_has_capacity(
        transaction,
        room_id,
        request.action,
        current.as_ref(),
        state_bytes,
    )? {
        return Ok(MessageRevisionMutationResult::Saturated);
    }

    let mut pruned = prune_expired_audit_rows(transaction, now)?;
    let audit_bytes = AUDIT_FIXED_RETAINED_BYTES.saturating_add(replacement_bytes);
    if !ensure_audit_capacity(transaction, room_id, audit_bytes, &mut pruned)? {
        return Ok(MessageRevisionMutationResult::Saturated);
    }

    let revision_event_id = next_revision_event_id(transaction, room_id)?;
    let replacement = request.replacement.as_ref().map(|value| value.as_bytes());
    transaction.execute(
        "INSERT INTO room_message_revision_state(
           room_id, target_event_id, latest_revision_event_id, revision_action,
           actor_user_id, replacement_body, revision_number, at, retained_bytes
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
         ON CONFLICT(room_id, target_event_id) DO UPDATE SET
           latest_revision_event_id = excluded.latest_revision_event_id,
           revision_action = excluded.revision_action,
           actor_user_id = excluded.actor_user_id,
           replacement_body = excluded.replacement_body,
           revision_number = excluded.revision_number,
           at = excluded.at,
           retained_bytes = excluded.retained_bytes",
        (
            room_id,
            target_event_id,
            revision_event_id,
            request.action as u8,
            actor_user_id,
            replacement,
            revision_number as i64,
            now,
            state_bytes,
        ),
    )?;
    transaction.execute(
        "INSERT INTO room_message_revision_events(
           room_id, revision_event_id, target_event_id, actor_user_id,
           revision_action, replacement_body, revision_number, at, retained_bytes
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
        (
            room_id,
            revision_event_id,
            target_event_id,
            actor_user_id,
            request.action as u8,
            replacement,
            revision_number as i64,
            now,
            audit_bytes,
        ),
    )?;
    let reactions_cleared = if request.action == MessageRevisionAction::Tombstone {
        transaction.execute(
            "DELETE FROM room_reactions
             WHERE room_id = ?1 AND target_event_id = ?2",
            (room_id, target_event_id),
        )?
    } else {
        0
    };

    Ok(MessageRevisionMutationResult::Changed(
        MessageRevisionMutation {
            event: MessageRevisionEvent {
                revision_event_id: revision_event_id as u64,
                target_event_id: request.target_event_id,
                action: request.action,
                actor_user_id,
                at_unix: now,
                replacement: request.replacement,
                revision_number,
                actor_display_name: actor_display_name.map(str::to_owned),
            },
            reactions_cleared,
        },
    ))
}

fn load_current_state(
    transaction: &rusqlite::Transaction<'_>,
    room_id: RoomId,
    target_event_id: i64,
) -> ServerResult<Option<StoredRevisionState>> {
    transaction
        .query_row(
            "SELECT revision_action, replacement_body, revision_number, retained_bytes
             FROM room_message_revision_state
             WHERE room_id = ?1 AND target_event_id = ?2",
            (room_id, target_event_id),
            |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, Option<Vec<u8>>>(1)?,
                    row.get::<_, i64>(2)?,
                    row.get::<_, i64>(3)?,
                ))
            },
        )
        .optional()?
        .map(|(action, replacement, revision_number, retained_bytes)| {
            Ok(StoredRevisionState {
                action: stored_action(action)?,
                replacement: decode_replacement(replacement)?,
                revision_number: u64::try_from(revision_number).map_err(|_| {
                    ServerError::Message("stored message revision number is invalid".into())
                })?,
                retained_bytes,
            })
        })
        .transpose()
}

fn state_has_capacity(
    transaction: &rusqlite::Transaction<'_>,
    room_id: RoomId,
    action: MessageRevisionAction,
    current: Option<&StoredRevisionState>,
    incoming_bytes: i64,
) -> ServerResult<bool> {
    let current_rows = i64::from(current.is_some());
    let current_bytes = current.map_or(0, |state| state.retained_bytes);
    let (room_rows, room_bytes) = state_usage(transaction, Some(room_id), None)?;
    let (global_rows, global_bytes) = state_usage(transaction, None, None)?;
    let next_room_rows = room_rows.saturating_sub(current_rows).saturating_add(1);
    let next_global_rows = global_rows.saturating_sub(current_rows).saturating_add(1);
    let next_room_bytes = room_bytes
        .saturating_sub(current_bytes)
        .saturating_add(incoming_bytes);
    let next_global_bytes = global_bytes
        .saturating_sub(current_bytes)
        .saturating_add(incoming_bytes);
    if next_room_rows > MAX_STATE_ROWS_PER_ROOM
        || next_room_bytes > MAX_STATE_BYTES_PER_ROOM
        || next_global_rows > MAX_STATE_ROWS_GLOBAL
        || next_global_bytes > MAX_STATE_BYTES_GLOBAL
    {
        return Ok(false);
    }
    if action == MessageRevisionAction::Tombstone {
        return Ok(true);
    }

    let current_correction =
        i64::from(current.is_some_and(|state| state.action == MessageRevisionAction::Correct));
    let current_correction_bytes = current
        .filter(|state| state.action == MessageRevisionAction::Correct)
        .map_or(0, |state| state.retained_bytes);
    let (room_corrections, room_correction_bytes) = state_usage(
        transaction,
        Some(room_id),
        Some(MessageRevisionAction::Correct),
    )?;
    let (global_corrections, global_correction_bytes) =
        state_usage(transaction, None, Some(MessageRevisionAction::Correct))?;
    Ok(room_corrections
        .saturating_sub(current_correction)
        .saturating_add(1)
        <= MAX_CORRECTION_ROWS_PER_ROOM
        && room_correction_bytes
            .saturating_sub(current_correction_bytes)
            .saturating_add(incoming_bytes)
            <= MAX_CORRECTION_BYTES_PER_ROOM
        && global_corrections
            .saturating_sub(current_correction)
            .saturating_add(1)
            <= MAX_CORRECTION_ROWS_GLOBAL
        && global_correction_bytes
            .saturating_sub(current_correction_bytes)
            .saturating_add(incoming_bytes)
            <= MAX_CORRECTION_BYTES_GLOBAL)
}

fn state_usage(
    transaction: &rusqlite::Transaction<'_>,
    room_id: Option<RoomId>,
    action: Option<MessageRevisionAction>,
) -> ServerResult<(i64, i64)> {
    let result = match (room_id, action) {
        (Some(room_id), Some(action)) => transaction.query_row(
            "SELECT COUNT(*), COALESCE(SUM(retained_bytes), 0)
             FROM room_message_revision_state
             WHERE room_id = ?1 AND revision_action = ?2",
            (room_id, action as u8),
            |row| Ok((row.get(0)?, row.get(1)?)),
        ),
        (Some(room_id), None) => transaction.query_row(
            "SELECT COUNT(*), COALESCE(SUM(retained_bytes), 0)
             FROM room_message_revision_state WHERE room_id = ?1",
            [room_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        ),
        (None, Some(action)) => transaction.query_row(
            "SELECT COUNT(*), COALESCE(SUM(retained_bytes), 0)
             FROM room_message_revision_state WHERE revision_action = ?1",
            [action as u8],
            |row| Ok((row.get(0)?, row.get(1)?)),
        ),
        (None, None) => transaction.query_row(
            "SELECT COUNT(*), COALESCE(SUM(retained_bytes), 0)
             FROM room_message_revision_state",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        ),
    };
    result.map_err(Into::into)
}

fn prune_expired_audit_rows(
    transaction: &rusqlite::Transaction<'_>,
    now: i64,
) -> ServerResult<usize> {
    let cutoff = now.saturating_sub(AUDIT_RETENTION_AGE_SECONDS);
    transaction
        .execute(
            "DELETE FROM room_message_revision_events
             WHERE rowid IN (
               SELECT rowid FROM room_message_revision_events
               WHERE at < ?1
               ORDER BY at, room_id, revision_event_id
               LIMIT ?2
             )",
            (cutoff, MAX_AUDIT_PRUNED_PER_MUTATION as i64),
        )
        .map_err(Into::into)
}

fn ensure_audit_capacity(
    transaction: &rusqlite::Transaction<'_>,
    room_id: RoomId,
    incoming_bytes: i64,
    pruned: &mut usize,
) -> ServerResult<bool> {
    loop {
        let (room_rows, room_bytes) = audit_usage(transaction, Some(room_id))?;
        let (global_rows, global_bytes) = audit_usage(transaction, None)?;
        if room_rows.saturating_add(1) <= MAX_AUDIT_ROWS_PER_ROOM
            && room_bytes.saturating_add(incoming_bytes) <= MAX_AUDIT_BYTES_PER_ROOM
            && global_rows.saturating_add(1) <= MAX_AUDIT_ROWS_GLOBAL
            && global_bytes.saturating_add(incoming_bytes) <= MAX_AUDIT_BYTES_GLOBAL
        {
            return Ok(true);
        }
        if *pruned >= MAX_AUDIT_PRUNED_PER_MUTATION {
            return Ok(false);
        }
        let deleted = transaction.execute(
            "DELETE FROM room_message_revision_events
             WHERE rowid = (
               SELECT rowid FROM room_message_revision_events
               ORDER BY CASE WHEN room_id = ?1 THEN 0 ELSE 1 END,
                        at, room_id, revision_event_id
               LIMIT 1
             )",
            [room_id],
        )?;
        if deleted == 0 {
            return Ok(false);
        }
        *pruned = pruned.saturating_add(deleted);
    }
}

fn audit_usage(
    transaction: &rusqlite::Transaction<'_>,
    room_id: Option<RoomId>,
) -> ServerResult<(i64, i64)> {
    let result = match room_id {
        Some(room_id) => transaction.query_row(
            "SELECT COUNT(*), COALESCE(SUM(retained_bytes), 0)
             FROM room_message_revision_events WHERE room_id = ?1",
            [room_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        ),
        None => transaction.query_row(
            "SELECT COUNT(*), COALESCE(SUM(retained_bytes), 0)
             FROM room_message_revision_events",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        ),
    };
    result.map_err(Into::into)
}

fn next_revision_event_id(
    transaction: &rusqlite::Transaction<'_>,
    room_id: RoomId,
) -> ServerResult<i64> {
    transaction
        .query_row(
            "SELECT MAX(value) + 1 FROM (
               SELECT COALESCE(MAX(revision_event_id), 0) AS value
               FROM room_message_revision_events WHERE room_id = ?1
               UNION ALL
               SELECT COALESCE(MAX(latest_revision_event_id), 0) AS value
               FROM room_message_revision_state WHERE room_id = ?1
             )",
            [room_id],
            |row| row.get(0),
        )
        .map_err(Into::into)
}

fn validate_snapshot_targets(target_event_ids: &[EventId]) -> ServerResult<()> {
    if target_event_ids.len() > MESSAGE_REVISION_SNAPSHOT_MAX_TARGETS {
        return Err(ServerError::Message(format!(
            "message revision snapshot exceeds {MESSAGE_REVISION_SNAPSHOT_MAX_TARGETS} targets"
        )));
    }
    if target_event_ids.contains(&0)
        || target_event_ids.windows(2).any(|pair| pair[0] >= pair[1])
        || target_event_ids
            .iter()
            .any(|event_id| i64::try_from(*event_id).is_err())
    {
        return Err(ServerError::Message(
            "message revision snapshot target ids must be sorted, unique, nonzero SQLite event ids"
                .into(),
        ));
    }
    Ok(())
}

fn stored_action(value: i64) -> ServerResult<MessageRevisionAction> {
    MessageRevisionAction::try_from(value as u64)
        .map_err(|_| ServerError::Message("stored message revision action is invalid".into()))
}

fn stored_event_id(value: i64) -> ServerResult<EventId> {
    u64::try_from(value)
        .ok()
        .filter(|value| *value > 0)
        .ok_or_else(|| ServerError::Message("stored message revision event id is invalid".into()))
}

fn decode_replacement(value: Option<Vec<u8>>) -> ServerResult<Option<String>> {
    value
        .map(|value| {
            String::from_utf8(value).map_err(|_| {
                ServerError::Message("stored message revision replacement is not UTF-8".into())
            })
        })
        .transpose()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::{ReactionAction, ReactionRequest, ReactionToken};
    use crate::store::reactions::ReactionMutationResult;
    use crate::store::ServerRoomEventKind;

    struct Fixture {
        store: OmenchatStore,
        room_id: RoomId,
        author_id: UserId,
        moderator_id: UserId,
        target_event_id: EventId,
    }

    fn setup() -> Fixture {
        let store = OmenchatStore::in_memory().expect("store");
        let room = store
            .ensure_room("message-revisions", Some("test room"))
            .expect("room");
        let author = store
            .ensure_user(b"revision-author", "alice", None)
            .expect("author");
        let moderator = store
            .ensure_user(b"revision-moderator", "moderator", None)
            .expect("moderator");
        store
            .join_room(room.room_id, author.user_id)
            .expect("author join");
        store
            .join_room(room.room_id, moderator.user_id)
            .expect("moderator join");
        let target = store
            .append_event(
                room.room_id,
                Some(author.user_id),
                ServerRoomEventKind::Message {
                    body: "original".into(),
                },
            )
            .expect("target");
        Fixture {
            store,
            room_id: room.room_id,
            author_id: author.user_id,
            moderator_id: moderator.user_id,
            target_event_id: target.event_id,
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn mutate(
        store: &OmenchatStore,
        room_id: RoomId,
        actor_user_id: UserId,
        actor_display_name: &str,
        policy: MessageRevisionActorPolicy,
        target_event_id: EventId,
        action: MessageRevisionAction,
        replacement: Option<&str>,
        now: i64,
    ) -> MessageRevisionMutationResult {
        let transaction = rusqlite::Transaction::new_unchecked(
            &store.connection,
            rusqlite::TransactionBehavior::Immediate,
        )
        .expect("transaction");
        let result = apply_message_revision_mutation_at(
            &transaction,
            room_id,
            actor_user_id,
            Some(actor_display_name),
            policy,
            MessageRevisionRequest {
                target_event_id,
                action,
                replacement: replacement.map(str::to_owned),
            },
            1024,
            now,
        )
        .expect("revision mutation");
        transaction.commit().expect("commit");
        result
    }

    #[test]
    fn author_correction_and_tombstone_preserve_original_and_clear_reactions() {
        let fixture = setup();
        let reaction_transaction = rusqlite::Transaction::new_unchecked(
            &fixture.store.connection,
            rusqlite::TransactionBehavior::Immediate,
        )
        .expect("reaction transaction");
        assert!(matches!(
            OmenchatStore::apply_reaction_mutation(
                &reaction_transaction,
                fixture.room_id,
                fixture.author_id,
                ReactionRequest {
                    target_event_id: fixture.target_event_id,
                    token: ReactionToken::Heart,
                    action: ReactionAction::Add,
                }
            )
            .expect("reaction"),
            ReactionMutationResult::Changed(_)
        ));
        reaction_transaction.commit().expect("reaction commit");

        let corrected = mutate(
            &fixture.store,
            fixture.room_id,
            fixture.author_id,
            "alice",
            MessageRevisionActorPolicy::default(),
            fixture.target_event_id,
            MessageRevisionAction::Correct,
            Some("corrected"),
            100,
        );
        assert!(matches!(
            corrected,
            MessageRevisionMutationResult::Changed(MessageRevisionMutation {
                event: MessageRevisionEvent {
                    revision_event_id: 1,
                    revision_number: 1,
                    action: MessageRevisionAction::Correct,
                    ..
                },
                reactions_cleared: 0,
            })
        ));
        assert_eq!(
            mutate(
                &fixture.store,
                fixture.room_id,
                fixture.author_id,
                "alice",
                MessageRevisionActorPolicy::default(),
                fixture.target_event_id,
                MessageRevisionAction::Correct,
                Some("corrected"),
                101,
            ),
            MessageRevisionMutationResult::Unchanged
        );
        let tombstone = mutate(
            &fixture.store,
            fixture.room_id,
            fixture.author_id,
            "alice",
            MessageRevisionActorPolicy::default(),
            fixture.target_event_id,
            MessageRevisionAction::Tombstone,
            None,
            102,
        );
        assert!(matches!(
            tombstone,
            MessageRevisionMutationResult::Changed(MessageRevisionMutation {
                event: MessageRevisionEvent {
                    revision_event_id: 2,
                    revision_number: 2,
                    action: MessageRevisionAction::Tombstone,
                    ..
                },
                reactions_cleared: 1,
            })
        ));
        assert_eq!(
            fixture
                .store
                .connection
                .query_row(
                    "SELECT CAST(payload AS TEXT) FROM room_events
                     WHERE room_id = ?1 AND event_id = ?2",
                    (fixture.room_id, fixture.target_event_id as i64),
                    |row| row.get::<_, String>(0),
                )
                .expect("immutable original"),
            "original"
        );
        assert_eq!(
            fixture
                .store
                .connection
                .query_row("SELECT COUNT(*) FROM room_reactions", [], |row| {
                    row.get::<_, i64>(0)
                })
                .expect("cleared reactions"),
            0
        );
        let snapshot = fixture
            .store
            .message_revision_snapshot(fixture.room_id, &[fixture.target_event_id])
            .expect("snapshot");
        assert_eq!(snapshot.entries.len(), 1);
        assert_eq!(snapshot.entries[0].action, MessageRevisionAction::Tombstone);
        assert_eq!(snapshot.entries[0].revision_number, 2);
        assert_eq!(snapshot.entries[0].replacement, None);
    }

    #[test]
    fn authorization_distinguishes_authors_moderators_and_muted_authors() {
        let fixture = setup();
        assert_eq!(
            mutate(
                &fixture.store,
                fixture.room_id,
                fixture.moderator_id,
                "moderator",
                MessageRevisionActorPolicy {
                    is_moderator: true,
                    is_muted: false,
                },
                fixture.target_event_id,
                MessageRevisionAction::Correct,
                Some("forged words"),
                100,
            ),
            MessageRevisionMutationResult::PermissionDenied
        );
        assert_eq!(
            mutate(
                &fixture.store,
                fixture.room_id,
                fixture.author_id,
                "alice",
                MessageRevisionActorPolicy {
                    is_moderator: false,
                    is_muted: true,
                },
                fixture.target_event_id,
                MessageRevisionAction::Correct,
                Some("muted edit"),
                101,
            ),
            MessageRevisionMutationResult::PermissionDenied
        );
        assert!(matches!(
            mutate(
                &fixture.store,
                fixture.room_id,
                fixture.moderator_id,
                "moderator",
                MessageRevisionActorPolicy {
                    is_moderator: true,
                    is_muted: false,
                },
                fixture.target_event_id,
                MessageRevisionAction::Tombstone,
                None,
                102,
            ),
            MessageRevisionMutationResult::Changed(_)
        ));

        let second = fixture
            .store
            .append_event(
                fixture.room_id,
                Some(fixture.author_id),
                ServerRoomEventKind::Message {
                    body: "second".into(),
                },
            )
            .expect("second target");
        assert!(matches!(
            mutate(
                &fixture.store,
                fixture.room_id,
                fixture.author_id,
                "alice",
                MessageRevisionActorPolicy {
                    is_moderator: false,
                    is_muted: true,
                },
                second.event_id,
                MessageRevisionAction::Tombstone,
                None,
                103,
            ),
            MessageRevisionMutationResult::Changed(_)
        ));
    }

    #[test]
    fn eight_corrections_then_tombstone_are_the_only_revision_depth() {
        let fixture = setup();
        for number in 1..=MAX_CORRECTIONS_PER_TARGET {
            let result = mutate(
                &fixture.store,
                fixture.room_id,
                fixture.author_id,
                "alice",
                MessageRevisionActorPolicy::default(),
                fixture.target_event_id,
                MessageRevisionAction::Correct,
                Some(&format!("correction-{number}")),
                100 + number as i64,
            );
            assert!(matches!(
                result,
                MessageRevisionMutationResult::Changed(MessageRevisionMutation {
                    event: MessageRevisionEvent {
                        revision_number,
                        ..
                    },
                    ..
                }) if revision_number == number
            ));
        }
        assert_eq!(
            mutate(
                &fixture.store,
                fixture.room_id,
                fixture.author_id,
                "alice",
                MessageRevisionActorPolicy::default(),
                fixture.target_event_id,
                MessageRevisionAction::Correct,
                Some("ninth correction"),
                200,
            ),
            MessageRevisionMutationResult::CorrectionLimitReached
        );
        assert!(matches!(
            mutate(
                &fixture.store,
                fixture.room_id,
                fixture.author_id,
                "alice",
                MessageRevisionActorPolicy::default(),
                fixture.target_event_id,
                MessageRevisionAction::Tombstone,
                None,
                201,
            ),
            MessageRevisionMutationResult::Changed(MessageRevisionMutation {
                event: MessageRevisionEvent {
                    revision_number: 9,
                    ..
                },
                ..
            })
        ));
        assert_eq!(
            mutate(
                &fixture.store,
                fixture.room_id,
                fixture.author_id,
                "alice",
                MessageRevisionActorPolicy::default(),
                fixture.target_event_id,
                MessageRevisionAction::Tombstone,
                None,
                202,
            ),
            MessageRevisionMutationResult::AlreadyTombstoned
        );
        assert_eq!(
            fixture
                .store
                .message_revision_row_counts()
                .expect("revision counts"),
            (1, 9)
        );
    }

    #[test]
    fn unavailable_targets_and_transaction_rollback_leave_no_revision_state() {
        let fixture = setup();
        let action = fixture
            .store
            .append_event(
                fixture.room_id,
                Some(fixture.author_id),
                ServerRoomEventKind::Action {
                    body: "not editable".into(),
                },
            )
            .expect("action");
        assert_eq!(
            mutate(
                &fixture.store,
                fixture.room_id,
                fixture.author_id,
                "alice",
                MessageRevisionActorPolicy::default(),
                action.event_id,
                MessageRevisionAction::Correct,
                Some("no"),
                100,
            ),
            MessageRevisionMutationResult::TargetUnavailable
        );
        fixture
            .store
            .connection
            .execute(
                "UPDATE room_events SET deleted = 1
                 WHERE room_id = ?1 AND event_id = ?2",
                (fixture.room_id, fixture.target_event_id as i64),
            )
            .expect("legacy deleted marker");
        assert_eq!(
            mutate(
                &fixture.store,
                fixture.room_id,
                fixture.author_id,
                "alice",
                MessageRevisionActorPolicy::default(),
                fixture.target_event_id,
                MessageRevisionAction::Tombstone,
                None,
                100,
            ),
            MessageRevisionMutationResult::TargetUnavailable
        );
        fixture
            .store
            .connection
            .execute(
                "UPDATE room_events SET deleted = 0
                 WHERE room_id = ?1 AND event_id = ?2",
                (fixture.room_id, fixture.target_event_id as i64),
            )
            .expect("restore target fixture");
        let other_room = fixture
            .store
            .ensure_room("other-room", None)
            .expect("other room");
        assert_eq!(
            mutate(
                &fixture.store,
                other_room.room_id,
                fixture.author_id,
                "alice",
                MessageRevisionActorPolicy::default(),
                fixture.target_event_id,
                MessageRevisionAction::Correct,
                Some("cross-room"),
                100,
            ),
            MessageRevisionMutationResult::TargetUnavailable
        );

        let transaction = rusqlite::Transaction::new_unchecked(
            &fixture.store.connection,
            rusqlite::TransactionBehavior::Immediate,
        )
        .expect("rollback transaction");
        assert!(matches!(
            apply_message_revision_mutation_at(
                &transaction,
                fixture.room_id,
                fixture.author_id,
                Some("alice"),
                MessageRevisionActorPolicy::default(),
                MessageRevisionRequest {
                    target_event_id: fixture.target_event_id,
                    action: MessageRevisionAction::Correct,
                    replacement: Some("rolled back".into()),
                },
                1024,
                101,
            )
            .expect("rolled-back mutation"),
            MessageRevisionMutationResult::Changed(_)
        ));
        transaction.rollback().expect("rollback");
        assert_eq!(
            fixture
                .store
                .message_revision_row_counts()
                .expect("revision counts"),
            (0, 0)
        );
    }

    #[test]
    fn audit_pruning_is_bounded_and_revision_ids_never_reuse_state_ids() {
        let fixture = setup();
        fixture
            .store
            .connection
            .execute(
                "INSERT INTO room_message_revision_state(
                   room_id, target_event_id, latest_revision_event_id, revision_action,
                   actor_user_id, replacement_body, revision_number, at, retained_bytes
                 ) VALUES (?1, ?2, 1000, 1, ?3, X'6F6C64', 1, 0, 51)",
                (
                    fixture.room_id,
                    fixture.target_event_id as i64,
                    fixture.author_id,
                ),
            )
            .expect("retained state");
        for event_id in 1..=MAX_AUDIT_PRUNED_PER_MUTATION as i64 + 1 {
            fixture
                .store
                .connection
                .execute(
                    "INSERT INTO room_message_revision_events(
                       room_id, revision_event_id, target_event_id, actor_user_id,
                       revision_action, replacement_body, revision_number, at, retained_bytes
                     ) VALUES (?1, ?2, ?3, ?4, 1, X'6F6C64', 1, 0, 59)",
                    (
                        fixture.room_id,
                        event_id,
                        fixture.target_event_id as i64,
                        fixture.author_id,
                    ),
                )
                .expect("old audit row");
        }
        let result = mutate(
            &fixture.store,
            fixture.room_id,
            fixture.author_id,
            "alice",
            MessageRevisionActorPolicy::default(),
            fixture.target_event_id,
            MessageRevisionAction::Correct,
            Some("new"),
            AUDIT_RETENTION_AGE_SECONDS + 1,
        );
        assert!(matches!(
            result,
            MessageRevisionMutationResult::Changed(MessageRevisionMutation {
                event: MessageRevisionEvent {
                    revision_event_id: 1001,
                    revision_number: 2,
                    ..
                },
                ..
            })
        ));
        assert_eq!(
            fixture
                .store
                .connection
                .query_row(
                    "SELECT COUNT(*) FROM room_message_revision_events",
                    [],
                    |row| row.get::<_, i64>(0)
                )
                .expect("bounded audit"),
            2
        );
    }

    #[test]
    fn correction_soft_capacity_reserves_room_for_tombstones() {
        let fixture = setup();
        fixture
            .store
            .connection
            .execute(
                "WITH RECURSIVE sequence(value) AS (
                   SELECT 1
                   UNION ALL
                   SELECT value + 1 FROM sequence WHERE value < ?1
                 )
                 INSERT INTO room_message_revision_state(
                   room_id, target_event_id, latest_revision_event_id, revision_action,
                   actor_user_id, replacement_body, revision_number, at, retained_bytes
                 )
                 SELECT ?2, 10000 + value, 10000 + value, 1, ?3,
                        X'78', 1, 0, 49
                 FROM sequence",
                (
                    MAX_CORRECTION_ROWS_PER_ROOM,
                    fixture.room_id,
                    fixture.author_id,
                ),
            )
            .expect("correction soft-limit fixture");
        assert_eq!(
            mutate(
                &fixture.store,
                fixture.room_id,
                fixture.author_id,
                "alice",
                MessageRevisionActorPolicy::default(),
                fixture.target_event_id,
                MessageRevisionAction::Correct,
                Some("blocked correction"),
                100,
            ),
            MessageRevisionMutationResult::Saturated
        );
        assert!(matches!(
            mutate(
                &fixture.store,
                fixture.room_id,
                fixture.author_id,
                "alice",
                MessageRevisionActorPolicy::default(),
                fixture.target_event_id,
                MessageRevisionAction::Tombstone,
                None,
                101,
            ),
            MessageRevisionMutationResult::Changed(_)
        ));
    }

    #[test]
    fn hard_state_capacity_rejects_tombstone_without_partial_audit() {
        let fixture = setup();
        fixture
            .store
            .connection
            .execute(
                "WITH RECURSIVE sequence(value) AS (
                   SELECT 1
                   UNION ALL
                   SELECT value + 1 FROM sequence WHERE value < ?1
                 )
                 INSERT INTO room_message_revision_state(
                   room_id, target_event_id, latest_revision_event_id, revision_action,
                   actor_user_id, replacement_body, revision_number, at, retained_bytes
                 )
                 SELECT ?2, 10000 + value, 10000 + value, 2, ?3,
                        NULL, 1, 0, 48
                 FROM sequence",
                (MAX_STATE_ROWS_PER_ROOM, fixture.room_id, fixture.author_id),
            )
            .expect("hard state-limit fixture");
        assert_eq!(
            mutate(
                &fixture.store,
                fixture.room_id,
                fixture.author_id,
                "alice",
                MessageRevisionActorPolicy::default(),
                fixture.target_event_id,
                MessageRevisionAction::Tombstone,
                None,
                100,
            ),
            MessageRevisionMutationResult::Saturated
        );
        assert_eq!(
            fixture
                .store
                .message_revision_row_counts()
                .expect("revision counts"),
            (MAX_STATE_ROWS_PER_ROOM, 0)
        );
    }
}
