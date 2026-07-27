use rusqlite::params_from_iter;

use super::{
    current_unix_seconds, ensure_room_event_sequence, room_event_retained_bytes,
    room_history_usage_on, OmenchatStore, HISTORY_REPLY_RETAINED_BYTES,
};
use crate::error::{ServerError, ServerResult};
use crate::protocol::{EventId, RoomId};

pub const MAX_COMPACTED_EVENTS_PER_TRANSACTION: usize = 64;
pub const MAX_COMPACTION_DEPENDENT_ROWS: usize = 20_000;
pub const MAX_HISTORY_MAINTENANCE_STATUS_ROOMS: usize = 256;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct RoomHistoryRetentionPolicy {
    pub enabled: bool,
    pub max_age_days: u64,
    pub max_events_per_room: u64,
    pub max_bytes_per_room: u64,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct RoomHistoryCompaction {
    pub removed_events: usize,
    pub removed_event_bytes: u64,
    pub cleared_reply_references: usize,
    pub removed_reaction_state: usize,
    pub removed_reaction_audit: usize,
    pub removed_revision_state: usize,
    pub removed_revision_audit: usize,
    pub removed_pin_state: usize,
    pub removed_pin_audit: usize,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct RoomHistoryMaintenanceStatus {
    pub inspected_rooms: usize,
    pub more_rooms: bool,
    pub complete_ledgers: usize,
    pub incomplete_ledgers: usize,
    pub missing_ledgers: usize,
    pub accounted_events: u64,
    pub accounted_bytes: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Candidate {
    event_id: i64,
    retained_bytes: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct AdmissionCandidate {
    event_id: i64,
    retained_bytes: u64,
    at_unix: i64,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct DependencyCounts {
    replies: usize,
    reaction_state: usize,
    reaction_audit: usize,
    revision_state: usize,
    revision_audit: usize,
    pin_state: usize,
    pin_audit: usize,
}

impl DependencyCounts {
    fn total(self) -> usize {
        self.replies
            .saturating_add(self.reaction_state)
            .saturating_add(self.reaction_audit)
            .saturating_add(self.revision_state)
            .saturating_add(self.revision_audit)
            .saturating_add(self.pin_state)
            .saturating_add(self.pin_audit)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum CompactionBoundary {
    ReplyCleanup,
    ReactionCleanup,
    RevisionCleanup,
    PinCleanup,
    EventDelete,
    LedgerUpdate,
    Commit,
}

impl OmenchatStore {
    pub fn room_history_maintenance_status(
        &self,
        max_rooms: usize,
    ) -> ServerResult<RoomHistoryMaintenanceStatus> {
        if max_rooms == 0 {
            return Err(ServerError::Message(
                "room history maintenance status requires a positive room limit".into(),
            ));
        }
        let limit = max_rooms.min(MAX_HISTORY_MAINTENANCE_STATUS_ROOMS);
        let sql_limit = i64::try_from(limit + 1)
            .map_err(|_| ServerError::Message("room status limit does not fit SQLite".into()))?;
        let mut statement = self.connection.prepare(
            "SELECT u.event_count, u.retained_bytes, u.backfill_complete
             FROM rooms AS r
             LEFT JOIN room_history_usage AS u ON u.room_id = r.room_id
             ORDER BY r.room_id
             LIMIT ?1",
        )?;
        let mut rows = statement.query([sql_limit])?;
        let mut status = RoomHistoryMaintenanceStatus::default();
        while let Some(row) = rows.next()? {
            if status.inspected_rooms == limit {
                status.more_rooms = true;
                break;
            }
            status.inspected_rooms += 1;
            let event_count = row.get::<_, Option<i64>>(0)?;
            let retained_bytes = row.get::<_, Option<i64>>(1)?;
            let complete = row.get::<_, Option<bool>>(2)?;
            match (event_count, retained_bytes, complete) {
                (None, None, None) => status.missing_ledgers += 1,
                (Some(event_count), Some(retained_bytes), Some(complete)) => {
                    let event_count = u64::try_from(event_count).map_err(|_| {
                        ServerError::Message(
                            "room history maintenance found a negative event count".into(),
                        )
                    })?;
                    let retained_bytes = u64::try_from(retained_bytes).map_err(|_| {
                        ServerError::Message(
                            "room history maintenance found negative retained bytes".into(),
                        )
                    })?;
                    status.accounted_events = status
                        .accounted_events
                        .checked_add(event_count)
                        .ok_or_else(|| {
                            ServerError::Message(
                                "room history maintenance event count overflowed".into(),
                            )
                        })?;
                    status.accounted_bytes = status
                        .accounted_bytes
                        .checked_add(retained_bytes)
                        .ok_or_else(|| {
                            ServerError::Message(
                                "room history maintenance byte count overflowed".into(),
                            )
                        })?;
                    if complete {
                        status.complete_ledgers += 1;
                    } else {
                        status.incomplete_ledgers += 1;
                    }
                }
                _ => {
                    return Err(ServerError::Message(
                        "room history maintenance found a partial usage row".into(),
                    ));
                }
            }
        }
        Ok(status)
    }

    pub fn compact_room_history_through(
        &self,
        room_id: RoomId,
        through_event_id: EventId,
        max_events: usize,
    ) -> ServerResult<RoomHistoryCompaction> {
        self.compact_room_history_through_with_hook(room_id, through_event_id, max_events, |_| {
            Ok(())
        })
    }

    fn compact_room_history_through_with_hook<H>(
        &self,
        room_id: RoomId,
        through_event_id: EventId,
        max_events: usize,
        mut hook: H,
    ) -> ServerResult<RoomHistoryCompaction>
    where
        H: FnMut(CompactionBoundary) -> ServerResult<()>,
    {
        if through_event_id == 0 {
            return Err(ServerError::Message(
                "room history compaction requires a nonzero event boundary".into(),
            ));
        }
        if max_events == 0 {
            return Err(ServerError::Message(
                "room history compaction requires a positive event limit".into(),
            ));
        }
        let through = i64::try_from(through_event_id).map_err(|_| {
            ServerError::Message("room history compaction boundary does not fit SQLite".into())
        })?;
        let limit = max_events.min(MAX_COMPACTED_EVENTS_PER_TRANSACTION);
        let transaction = rusqlite::Transaction::new_unchecked(
            &self.connection,
            rusqlite::TransactionBehavior::Immediate,
        )?;
        let result = compact_room_history_through_in_transaction(
            &transaction,
            room_id,
            through,
            limit,
            &mut hook,
        )?;
        hook(CompactionBoundary::Commit)?;
        transaction.commit()?;
        Ok(result)
    }
}

fn compact_room_history_through_in_transaction<H>(
    transaction: &rusqlite::Transaction<'_>,
    room_id: RoomId,
    through_event_id: i64,
    limit: usize,
    hook: &mut H,
) -> ServerResult<RoomHistoryCompaction>
where
    H: FnMut(CompactionBoundary) -> ServerResult<()>,
{
    ensure_room_event_sequence(transaction, room_id)?;
    let usage = room_history_usage_on(transaction, room_id)?;
    if !usage.backfill_complete {
        return Err(ServerError::Message(format!(
            "room {room_id} history usage backfill is incomplete"
        )));
    }

    let mut candidates = load_candidates(transaction, room_id, through_event_id, limit)?;
    if candidates.is_empty() {
        return Ok(RoomHistoryCompaction::default());
    }
    let dependencies = loop {
        let event_ids = candidates
            .iter()
            .map(|candidate| candidate.event_id)
            .collect::<Vec<_>>();
        let dependencies = dependency_counts(transaction, room_id, &event_ids)?;
        if dependencies.total() <= MAX_COMPACTION_DEPENDENT_ROWS {
            break dependencies;
        }
        if candidates.len() == 1 {
            return Err(ServerError::Message(format!(
                    "room {room_id} event {} has {} dependent projections; compaction limit is {MAX_COMPACTION_DEPENDENT_ROWS}",
                    candidates[0].event_id,
                    dependencies.total()
                )));
        }
        candidates.pop();
    };
    let event_ids = candidates
        .iter()
        .map(|candidate| candidate.event_id)
        .collect::<Vec<_>>();

    let cleared_reply_references =
        clear_surviving_reply_references(transaction, room_id, &event_ids)?;
    if cleared_reply_references != dependencies.replies {
        return Err(ServerError::Message(format!(
            "room {room_id} reply projections changed during compaction"
        )));
    }
    hook(CompactionBoundary::ReplyCleanup)?;
    let removed_reaction_state =
        delete_target_rows(transaction, "room_reactions", room_id, &event_ids)?;
    let removed_reaction_audit =
        delete_target_rows(transaction, "room_reaction_events", room_id, &event_ids)?;
    if removed_reaction_state != dependencies.reaction_state
        || removed_reaction_audit != dependencies.reaction_audit
    {
        return Err(ServerError::Message(format!(
            "room {room_id} reaction projections changed during compaction"
        )));
    }
    hook(CompactionBoundary::ReactionCleanup)?;
    let removed_revision_state = delete_target_rows(
        transaction,
        "room_message_revision_state",
        room_id,
        &event_ids,
    )?;
    let removed_revision_audit = delete_target_rows(
        transaction,
        "room_message_revision_events",
        room_id,
        &event_ids,
    )?;
    if removed_revision_state != dependencies.revision_state
        || removed_revision_audit != dependencies.revision_audit
    {
        return Err(ServerError::Message(format!(
            "room {room_id} revision projections changed during compaction"
        )));
    }
    hook(CompactionBoundary::RevisionCleanup)?;
    let removed_pin_state = delete_target_rows(transaction, "room_pins", room_id, &event_ids)?;
    let removed_pin_audit =
        delete_target_rows(transaction, "room_pin_events", room_id, &event_ids)?;
    if removed_pin_state != dependencies.pin_state || removed_pin_audit != dependencies.pin_audit {
        return Err(ServerError::Message(format!(
            "room {room_id} pin projections changed during compaction"
        )));
    }
    hook(CompactionBoundary::PinCleanup)?;
    let removed_events = delete_events(transaction, room_id, &event_ids)?;
    if removed_events != candidates.len() {
        return Err(ServerError::Message(format!(
            "room {room_id} history changed during compaction"
        )));
    }
    hook(CompactionBoundary::EventDelete)?;

    let removed_event_bytes = candidates.iter().try_fold(0u64, |total, candidate| {
        total
            .checked_add(candidate.retained_bytes)
            .ok_or_else(|| ServerError::Message("room history compaction bytes overflowed".into()))
    })?;
    let cleared_reply_bytes = u64::try_from(cleared_reply_references)
        .ok()
        .and_then(|count| count.checked_mul(HISTORY_REPLY_RETAINED_BYTES))
        .ok_or_else(|| {
            ServerError::Message("room history reply-byte accounting overflowed".into())
        })?;
    update_usage(
        transaction,
        room_id,
        removed_events,
        removed_event_bytes,
        cleared_reply_bytes,
    )?;
    hook(CompactionBoundary::LedgerUpdate)?;

    Ok(RoomHistoryCompaction {
        removed_events,
        removed_event_bytes,
        cleared_reply_references,
        removed_reaction_state,
        removed_reaction_audit,
        removed_revision_state,
        removed_revision_audit,
        removed_pin_state,
        removed_pin_audit,
    })
}

pub(super) fn enforce_room_history_policy_in_transaction(
    transaction: &rusqlite::Transaction<'_>,
    room_id: RoomId,
    newest_event_id: EventId,
    policy: RoomHistoryRetentionPolicy,
) -> ServerResult<RoomHistoryCompaction> {
    if !policy.enabled {
        return Ok(RoomHistoryCompaction::default());
    }
    if policy.max_age_days == 0 || policy.max_events_per_room == 0 || policy.max_bytes_per_room == 0
    {
        return Err(ServerError::Message(
            "enabled room history retention requires positive age, item, and byte limits".into(),
        ));
    }
    let usage = room_history_usage_on(transaction, room_id)?;
    if !usage.backfill_complete {
        return Err(ServerError::Message(format!(
            "room {room_id} history admission is blocked until usage backfill completes"
        )));
    }
    let max_age_seconds = policy
        .max_age_days
        .checked_mul(24 * 60 * 60)
        .and_then(|seconds| i64::try_from(seconds).ok())
        .ok_or_else(|| ServerError::Message("room history age limit overflowed".into()))?;
    let cutoff = current_unix_seconds().saturating_sub(max_age_seconds);
    let newest = i64::try_from(newest_event_id)
        .map_err(|_| ServerError::Message("newest event ID does not fit SQLite".into()))?;
    let candidates = load_admission_candidates(
        transaction,
        room_id,
        newest,
        MAX_COMPACTED_EVENTS_PER_TRANSACTION,
    )?;
    let mut projected_events = usage.event_count;
    let mut projected_bytes = usage.retained_bytes;
    let mut selected = Vec::new();
    for candidate in candidates {
        let age_expired = candidate.at_unix <= cutoff;
        let over_items = projected_events > policy.max_events_per_room;
        let over_bytes = projected_bytes > policy.max_bytes_per_room && projected_events > 1;
        if !age_expired && !over_items && !over_bytes {
            break;
        }
        selected.push(candidate);
        projected_events = projected_events.saturating_sub(1);
        projected_bytes = projected_bytes.saturating_sub(candidate.retained_bytes);
    }

    if selected.is_empty() {
        return Ok(RoomHistoryCompaction::default());
    }
    let through = selected
        .last()
        .map(|candidate| candidate.event_id)
        .ok_or_else(|| ServerError::Message("room history selection was empty".into()))?;
    let mut no_hook = |_| Ok(());
    let compacted = compact_room_history_through_in_transaction(
        transaction,
        room_id,
        through,
        selected.len(),
        &mut no_hook,
    )?;
    verify_room_history_policy_after_compaction(transaction, room_id, newest, cutoff, policy)?;
    Ok(compacted)
}

fn load_admission_candidates(
    transaction: &rusqlite::Transaction<'_>,
    room_id: RoomId,
    newest_event_id: i64,
    limit: usize,
) -> ServerResult<Vec<AdmissionCandidate>> {
    let mut statement = transaction.prepare(
        "SELECT event_id, COALESCE(length(payload), 0),
                reply_to_event_id IS NOT NULL,
                COALESCE(length(mention_user_ids), 0), at
         FROM room_events
         WHERE room_id = ?1 AND event_id <> ?2
         ORDER BY event_id
         LIMIT ?3",
    )?;
    let rows = statement
        .query_map((room_id, newest_event_id, limit as i64), |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, i64>(1)?,
                row.get::<_, bool>(2)?,
                row.get::<_, i64>(3)?,
                row.get::<_, i64>(4)?,
            ))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    rows.into_iter()
        .map(
            |(event_id, payload_bytes, has_reply, mention_bytes, at_unix)| {
                let payload_bytes = usize::try_from(payload_bytes).map_err(|_| {
                    ServerError::Message("stored room event payload length is invalid".into())
                })?;
                let mention_bytes = usize::try_from(mention_bytes).map_err(|_| {
                    ServerError::Message("stored room event mention length is invalid".into())
                })?;
                Ok(AdmissionCandidate {
                    event_id,
                    retained_bytes: room_event_retained_bytes(
                        payload_bytes,
                        has_reply,
                        mention_bytes,
                    )?,
                    at_unix,
                })
            },
        )
        .collect()
}

fn verify_room_history_policy_after_compaction(
    transaction: &rusqlite::Transaction<'_>,
    room_id: RoomId,
    newest_event_id: i64,
    cutoff: i64,
    policy: RoomHistoryRetentionPolicy,
) -> ServerResult<()> {
    let usage = room_history_usage_on(transaction, room_id)?;
    let oversized_single =
        usage.event_count == 1 && usage.retained_bytes > policy.max_bytes_per_room;
    let expired_remain: bool = transaction.query_row(
        "SELECT EXISTS(
           SELECT 1 FROM room_events
           WHERE room_id = ?1 AND event_id <> ?2 AND at <= ?3
         )",
        (room_id, newest_event_id, cutoff),
        |row| row.get(0),
    )?;
    if usage.event_count > policy.max_events_per_room
        || (usage.retained_bytes > policy.max_bytes_per_room && !oversized_single)
        || expired_remain
    {
        return Err(ServerError::Message(format!(
            "room {room_id} history retention requires more than one bounded compaction batch"
        )));
    }
    Ok(())
}

fn load_candidates(
    transaction: &rusqlite::Transaction<'_>,
    room_id: RoomId,
    through_event_id: i64,
    limit: usize,
) -> ServerResult<Vec<Candidate>> {
    let mut statement = transaction.prepare(
        "SELECT event_id, COALESCE(length(payload), 0),
                reply_to_event_id IS NOT NULL,
                COALESCE(length(mention_user_ids), 0)
         FROM room_events
         WHERE room_id = ?1 AND event_id <= ?2
         ORDER BY event_id
         LIMIT ?3",
    )?;
    let rows = statement
        .query_map((room_id, through_event_id, limit as i64), |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, i64>(1)?,
                row.get::<_, bool>(2)?,
                row.get::<_, i64>(3)?,
            ))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    rows.into_iter()
        .map(|(event_id, payload_bytes, has_reply, mention_bytes)| {
            let payload_bytes = usize::try_from(payload_bytes).map_err(|_| {
                ServerError::Message("stored room event payload length is invalid".into())
            })?;
            let mention_bytes = usize::try_from(mention_bytes).map_err(|_| {
                ServerError::Message("stored room event mention length is invalid".into())
            })?;
            Ok(Candidate {
                event_id,
                retained_bytes: room_event_retained_bytes(payload_bytes, has_reply, mention_bytes)?,
            })
        })
        .collect()
}

fn dependency_counts(
    transaction: &rusqlite::Transaction<'_>,
    room_id: RoomId,
    event_ids: &[i64],
) -> ServerResult<DependencyCounts> {
    Ok(DependencyCounts {
        replies: count_surviving_replies(transaction, room_id, event_ids)?,
        reaction_state: count_target_rows(transaction, "room_reactions", room_id, event_ids)?,
        reaction_audit: count_target_rows(transaction, "room_reaction_events", room_id, event_ids)?,
        revision_state: count_target_rows(
            transaction,
            "room_message_revision_state",
            room_id,
            event_ids,
        )?,
        revision_audit: count_target_rows(
            transaction,
            "room_message_revision_events",
            room_id,
            event_ids,
        )?,
        pin_state: count_target_rows(transaction, "room_pins", room_id, event_ids)?,
        pin_audit: count_target_rows(transaction, "room_pin_events", room_id, event_ids)?,
    })
}

fn placeholders(count: usize) -> String {
    std::iter::repeat_n("?", count)
        .collect::<Vec<_>>()
        .join(",")
}

fn room_and_ids(room_id: RoomId, event_ids: &[i64]) -> Vec<i64> {
    let mut parameters = Vec::with_capacity(event_ids.len() + 1);
    parameters.push(i64::from(room_id));
    parameters.extend_from_slice(event_ids);
    parameters
}

fn count_target_rows(
    transaction: &rusqlite::Transaction<'_>,
    table: &str,
    room_id: RoomId,
    event_ids: &[i64],
) -> ServerResult<usize> {
    debug_assert!(matches!(
        table,
        "room_reactions"
            | "room_reaction_events"
            | "room_message_revision_state"
            | "room_message_revision_events"
            | "room_pins"
            | "room_pin_events"
    ));
    let sql = format!(
        "SELECT COUNT(*) FROM {table}
         WHERE room_id = ? AND target_event_id IN ({})",
        placeholders(event_ids.len())
    );
    let count = transaction.query_row(
        &sql,
        params_from_iter(room_and_ids(room_id, event_ids)),
        |row| row.get::<_, i64>(0),
    )?;
    usize::try_from(count)
        .map_err(|_| ServerError::Message("dependent projection count is invalid".into()))
}

fn count_surviving_replies(
    transaction: &rusqlite::Transaction<'_>,
    room_id: RoomId,
    event_ids: &[i64],
) -> ServerResult<usize> {
    let marks = placeholders(event_ids.len());
    let sql = format!(
        "SELECT COUNT(*) FROM room_events
         WHERE room_id = ?
           AND reply_to_event_id IN ({marks})
           AND event_id NOT IN ({marks})"
    );
    let mut parameters = room_and_ids(room_id, event_ids);
    parameters.extend_from_slice(event_ids);
    let count = transaction.query_row(&sql, params_from_iter(parameters), |row| {
        row.get::<_, i64>(0)
    })?;
    usize::try_from(count)
        .map_err(|_| ServerError::Message("dependent reply count is invalid".into()))
}

fn clear_surviving_reply_references(
    transaction: &rusqlite::Transaction<'_>,
    room_id: RoomId,
    event_ids: &[i64],
) -> ServerResult<usize> {
    let marks = placeholders(event_ids.len());
    let sql = format!(
        "UPDATE room_events SET reply_to_event_id = NULL
         WHERE room_id = ?
           AND reply_to_event_id IN ({marks})
           AND event_id NOT IN ({marks})"
    );
    let mut parameters = room_and_ids(room_id, event_ids);
    parameters.extend_from_slice(event_ids);
    transaction
        .execute(&sql, params_from_iter(parameters))
        .map_err(Into::into)
}

fn delete_target_rows(
    transaction: &rusqlite::Transaction<'_>,
    table: &str,
    room_id: RoomId,
    event_ids: &[i64],
) -> ServerResult<usize> {
    debug_assert!(matches!(
        table,
        "room_reactions"
            | "room_reaction_events"
            | "room_message_revision_state"
            | "room_message_revision_events"
            | "room_pins"
            | "room_pin_events"
    ));
    let sql = format!(
        "DELETE FROM {table}
         WHERE room_id = ? AND target_event_id IN ({})",
        placeholders(event_ids.len())
    );
    transaction
        .execute(&sql, params_from_iter(room_and_ids(room_id, event_ids)))
        .map_err(Into::into)
}

fn delete_events(
    transaction: &rusqlite::Transaction<'_>,
    room_id: RoomId,
    event_ids: &[i64],
) -> ServerResult<usize> {
    let sql = format!(
        "DELETE FROM room_events
         WHERE room_id = ? AND event_id IN ({})",
        placeholders(event_ids.len())
    );
    transaction
        .execute(&sql, params_from_iter(room_and_ids(room_id, event_ids)))
        .map_err(Into::into)
}

fn update_usage(
    transaction: &rusqlite::Transaction<'_>,
    room_id: RoomId,
    removed_events: usize,
    removed_event_bytes: u64,
    cleared_reply_bytes: u64,
) -> ServerResult<()> {
    let removed_events = i64::try_from(removed_events)
        .map_err(|_| ServerError::Message("compacted event count does not fit SQLite".into()))?;
    let removed_bytes = removed_event_bytes
        .checked_add(cleared_reply_bytes)
        .and_then(|bytes| i64::try_from(bytes).ok())
        .ok_or_else(|| ServerError::Message("compacted bytes do not fit SQLite".into()))?;
    let changed = transaction.execute(
        "UPDATE room_history_usage
         SET event_count = event_count - ?2,
             retained_bytes = retained_bytes - ?3,
             last_compacted_at = ?4
         WHERE room_id = ?1 AND backfill_complete = 1
           AND event_count >= ?2 AND retained_bytes >= ?3",
        (
            room_id,
            removed_events,
            removed_bytes,
            current_unix_seconds(),
        ),
    )?;
    if changed != 1 {
        return Err(ServerError::Message(format!(
            "room {room_id} history usage does not cover the compaction batch"
        )));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::RichMessageEventMetadata;
    use crate::store::{
        append_event_with_metadata_in_transaction, ServerRoomEventKind,
        HISTORY_EVENT_FIXED_RETAINED_BYTES,
    };

    fn message(body: &str) -> ServerRoomEventKind {
        ServerRoomEventKind::Message { body: body.into() }
    }

    fn active_policy(
        max_events_per_room: u64,
        max_bytes_per_room: u64,
    ) -> RoomHistoryRetentionPolicy {
        RoomHistoryRetentionPolicy {
            enabled: true,
            max_age_days: 3_650,
            max_events_per_room,
            max_bytes_per_room,
        }
    }

    fn seeded_store() -> (OmenchatStore, RoomId) {
        let store = OmenchatStore::in_memory().expect("store");
        let room = store.ensure_room("retention", None).expect("room");
        store
            .append_event(room.room_id, None, message("original"))
            .expect("original");
        let transaction = store
            .connection
            .unchecked_transaction()
            .expect("transaction");
        append_event_with_metadata_in_transaction(
            &transaction,
            room.room_id,
            None,
            message("reply"),
            Some(RichMessageEventMetadata {
                reply_to_event_id: Some(1),
                mentioned_user_ids: vec![7],
            }),
            RoomHistoryRetentionPolicy::default(),
        )
        .expect("reply");
        transaction.commit().expect("commit");
        (store, room.room_id)
    }

    #[test]
    fn maintenance_status_is_read_only_bounded_and_reports_missing_ledgers() {
        let store = OmenchatStore::in_memory().expect("store");
        let tracked = store.ensure_room("tracked", None).expect("tracked room");
        store
            .append_event(tracked.room_id, None, message("accounted"))
            .expect("tracked event");
        let before = store
            .room_history_usage(tracked.room_id)
            .expect("usage")
            .expect("usage row");

        let status = store
            .room_history_maintenance_status(10)
            .expect("maintenance status");
        assert_eq!(status.inspected_rooms, 2);
        assert!(!status.more_rooms);
        assert_eq!(status.complete_ledgers, 1);
        assert_eq!(status.incomplete_ledgers, 0);
        assert_eq!(status.missing_ledgers, 1);
        assert_eq!(status.accounted_events, 1);
        assert_eq!(status.accounted_bytes, before.retained_bytes);
        assert_eq!(
            store
                .room_history_usage(tracked.room_id)
                .expect("usage after status")
                .expect("usage row after status"),
            before
        );

        let bounded = store
            .room_history_maintenance_status(1)
            .expect("bounded status");
        assert_eq!(bounded.inspected_rooms, 1);
        assert!(bounded.more_rooms);
        assert!(store.room_history_maintenance_status(0).is_err());

        for index in 0..MAX_HISTORY_MAINTENANCE_STATUS_ROOMS {
            store
                .ensure_room(&format!("status-{index}"), None)
                .expect("additional room");
        }
        let hard_bounded = store
            .room_history_maintenance_status(usize::MAX)
            .expect("hard-bounded status");
        assert_eq!(
            hard_bounded.inspected_rooms,
            MAX_HISTORY_MAINTENANCE_STATUS_ROOMS
        );
        assert!(hard_bounded.more_rooms);
    }

    #[test]
    fn disabled_policy_preserves_history_and_item_policy_compacts_on_admission() {
        let store = OmenchatStore::in_memory().expect("store");
        for body in ["one", "two", "three"] {
            store.append_event(1, None, message(body)).expect("append");
        }
        assert_eq!(
            store.latest_events(1, 10).expect("disabled history").len(),
            3
        );

        let store = store.with_room_history_retention(active_policy(3, u64::MAX));
        let appended = store
            .append_event(1, None, message("four"))
            .expect("bounded append");
        assert_eq!(appended.event_id, 4);
        assert_eq!(
            store
                .latest_events(1, 10)
                .expect("retained history")
                .iter()
                .map(|event| event.event_id)
                .collect::<Vec<_>>(),
            vec![2, 3, 4]
        );
        let usage = store
            .room_history_usage(1)
            .expect("usage")
            .expect("usage row");
        assert_eq!(usage.event_count, 3);
    }

    #[test]
    fn byte_age_and_oversized_single_policies_are_independent() {
        let store = OmenchatStore::in_memory()
            .expect("store")
            .with_room_history_retention(active_policy(10, 140));
        store
            .append_event(1, None, message(&"a".repeat(60)))
            .expect("first");
        store
            .append_event(1, None, message(&"b".repeat(60)))
            .expect("byte-triggered append");
        assert_eq!(store.latest_events(1, 10).expect("byte history").len(), 1);
        assert_eq!(
            store.latest_events(1, 10).expect("byte history")[0].event_id,
            2
        );

        store
            .connection
            .execute("UPDATE room_events SET at = 1 WHERE room_id = 1", [])
            .expect("age first event");
        let aged_policy = RoomHistoryRetentionPolicy {
            max_age_days: 1,
            max_bytes_per_room: u64::MAX,
            ..active_policy(10, u64::MAX)
        };
        let store = store.with_room_history_retention(aged_policy);
        store
            .append_event(1, None, message("fresh"))
            .expect("age-triggered append");
        assert_eq!(
            store
                .latest_events(1, 10)
                .expect("age history")
                .iter()
                .map(|event| event.event_id)
                .collect::<Vec<_>>(),
            vec![3]
        );

        let store = OmenchatStore::in_memory()
            .expect("oversized store")
            .with_room_history_retention(active_policy(10, 1));
        store
            .append_event(1, None, message("oversized"))
            .expect("single oversized event is retained");
        store
            .append_event(1, None, message("replacement"))
            .expect("next oversized event replaces oldest");
        assert_eq!(
            store.latest_events(1, 10).expect("oversized history")[0].event_id,
            2
        );
    }

    #[test]
    fn incomplete_accounting_and_multi_batch_saturation_roll_back_admission() {
        let store = OmenchatStore::in_memory().expect("store");
        store
            .append_event(1, None, message("existing"))
            .expect("seed");
        let transaction = store
            .connection
            .unchecked_transaction()
            .expect("legacy rows");
        for event_id in 2..=300 {
            transaction
                .execute(
                    "INSERT INTO room_events(
                       room_id, event_id, event_kind, at, payload
                     ) VALUES (1, ?1, 1, 1, X'78')",
                    [event_id],
                )
                .expect("legacy event");
        }
        transaction
            .execute(
                "UPDATE room_event_sequences SET last_event_id = 300 WHERE room_id = 1",
                [],
            )
            .expect("legacy sequence");
        transaction
            .execute(
                "UPDATE room_history_usage
                 SET event_count = 0, retained_bytes = 0,
                     backfill_through_event_id = 0,
                     backfill_target_event_id = 300,
                     backfill_complete = 0
                 WHERE room_id = 1",
                [],
            )
            .expect("mark incomplete");
        transaction.commit().expect("commit legacy fixture");
        let store = store.with_room_history_retention(active_policy(1, u64::MAX));
        let before = store.latest_events(1, 400).expect("before incomplete");
        let error = store
            .append_event(1, None, message("blocked"))
            .expect_err("incomplete accounting must block")
            .to_string();
        assert!(error.contains("backfill completes"));
        assert_eq!(
            store.latest_events(1, 400).expect("after incomplete"),
            before
        );

        let store = OmenchatStore::in_memory().expect("saturation store");
        for index in 0..66 {
            store
                .append_event(1, None, message(&format!("event-{index}")))
                .expect("seed saturation");
        }
        let before = store.latest_events(1, 100).expect("before saturation");
        let before_usage = store
            .room_history_usage(1)
            .expect("usage")
            .expect("usage row");
        let store = store.with_room_history_retention(active_policy(1, u64::MAX));
        let error = store
            .append_event(1, None, message("must-roll-back"))
            .expect_err("more than one compaction batch must fail")
            .to_string();
        assert!(error.contains("more than one bounded compaction batch"));
        assert_eq!(
            store.latest_events(1, 100).expect("after saturation"),
            before
        );
        assert_eq!(
            store
                .room_history_usage(1)
                .expect("usage after")
                .expect("usage row after"),
            before_usage
        );
    }

    fn insert_dependencies(store: &OmenchatStore, room_id: RoomId) {
        store
            .connection
            .execute_batch(&format!(
                "INSERT INTO room_reactions(
                   room_id, target_event_id, actor_user_id, reaction_token, created_at
                 ) VALUES ({room_id}, 1, 7, 'heart', 1);
                 INSERT INTO room_reaction_events(
                   room_id, reaction_event_id, target_event_id, actor_user_id,
                   reaction_token, reaction_action, at, retained_bytes
                 ) VALUES ({room_id}, 1, 1, 7, 'heart', 1, 1, 32);
                 INSERT INTO room_message_revision_state(
                   room_id, target_event_id, latest_revision_event_id,
                   revision_action, actor_user_id, replacement_body,
                   revision_number, at, retained_bytes
                 ) VALUES ({room_id}, 1, 1, 1, 7, X'656469746564', 1, 1, 54);
                 INSERT INTO room_message_revision_events(
                   room_id, revision_event_id, target_event_id, actor_user_id,
                   revision_action, replacement_body, revision_number, at,
                   retained_bytes
                 ) VALUES ({room_id}, 1, 1, 7, 1, X'656469746564', 1, 1, 62);
                 INSERT INTO room_pin_events(
                   pin_event_id, room_id, target_event_id, actor_user_id,
                   pin_action, at, retained_bytes
                 ) VALUES (1, {room_id}, 1, 7, 1, 1, 41);
                 INSERT INTO room_pins(
                   room_id, target_event_id, pin_event_id, actor_user_id,
                   pinned_at, retained_bytes
                 ) VALUES ({room_id}, 1, 1, 7, 1, 32);"
            ))
            .expect("dependent projections");
    }

    #[test]
    fn compaction_removes_target_projections_and_clears_only_surviving_reply_reference() {
        let (store, room_id) = seeded_store();
        insert_dependencies(&store, room_id);
        store
            .connection
            .execute(
                "INSERT INTO upload_files(
                   resource_id, room_id, actor_user_id, filename, byte_len, path, created_at
                 ) VALUES ('kept', ?1, 7, 'kept.bin', 1, '/isolated/kept.bin', 1)",
                [room_id],
            )
            .expect("upload ledger");
        store
            .connection
            .execute(
                "INSERT INTO durable_mutation_results(
                   identity_hash, client_instance_id, mutation_id, request_hash,
                   result_frame, retained_bytes, created_at, last_seen_at
                 ) VALUES (X'01', zeroblob(16), zeroblob(16), zeroblob(32),
                           X'02', 1, 1, 1)",
                [],
            )
            .expect("durable replay");

        let before = store
            .room_history_usage(room_id)
            .expect("usage")
            .expect("usage row");
        let result = store
            .compact_room_history_through(room_id, 1, 64)
            .expect("compact");
        assert_eq!(
            result,
            RoomHistoryCompaction {
                removed_events: 1,
                removed_event_bytes: HISTORY_EVENT_FIXED_RETAINED_BYTES + "original".len() as u64,
                cleared_reply_references: 1,
                removed_reaction_state: 1,
                removed_reaction_audit: 1,
                removed_revision_state: 1,
                removed_revision_audit: 1,
                removed_pin_state: 1,
                removed_pin_audit: 1,
            }
        );
        let history = store.latest_events(room_id, 10).expect("history");
        assert_eq!(history.len(), 1);
        assert_eq!(history[0].event_id, 2);
        let metadata = history[0].metadata.as_ref().expect("mention retained");
        assert_eq!(metadata.reply_to_event_id, None);
        assert_eq!(metadata.mentioned_user_ids, vec![7]);
        for table in [
            "room_reactions",
            "room_reaction_events",
            "room_message_revision_state",
            "room_message_revision_events",
            "room_pins",
            "room_pin_events",
        ] {
            assert_eq!(
                store
                    .connection
                    .query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |row| {
                        row.get::<_, i64>(0)
                    })
                    .expect("projection count"),
                0
            );
        }
        assert_eq!(
            store
                .connection
                .query_row("SELECT COUNT(*) FROM upload_files", [], |row| {
                    row.get::<_, i64>(0)
                })
                .expect("upload retained"),
            1
        );
        assert_eq!(
            store
                .connection
                .query_row("SELECT COUNT(*) FROM durable_mutation_results", [], |row| {
                    row.get::<_, i64>(0)
                })
                .expect("replay retained"),
            1
        );
        let after = store
            .room_history_usage(room_id)
            .expect("usage")
            .expect("usage row");
        assert_eq!(after.event_count, 1);
        assert_eq!(
            after.retained_bytes,
            before.retained_bytes - result.removed_event_bytes - HISTORY_REPLY_RETAINED_BYTES
        );
        assert_eq!(
            store
                .append_event(room_id, None, message("after"))
                .expect("append after compaction")
                .event_id,
            3
        );
    }

    #[test]
    fn compaction_batch_is_bounded_and_selected_replies_are_not_double_accounted() {
        let store = OmenchatStore::in_memory().expect("store");
        let room = store.ensure_room("batch", None).expect("room");
        for index in 0..70 {
            store
                .append_event(room.room_id, None, message(&format!("event-{index}")))
                .expect("append");
        }
        let first = store
            .compact_room_history_through(room.room_id, 70, usize::MAX)
            .expect("first bounded batch");
        assert_eq!(first.removed_events, MAX_COMPACTED_EVENTS_PER_TRANSACTION);
        let second = store
            .compact_room_history_through(room.room_id, 70, usize::MAX)
            .expect("second batch");
        assert_eq!(second.removed_events, 6);
        let usage = store
            .room_history_usage(room.room_id)
            .expect("usage")
            .expect("usage row");
        assert_eq!(usage.event_count, 0);
        assert_eq!(usage.retained_bytes, 0);

        let (store, room_id) = seeded_store();
        let result = store
            .compact_room_history_through(room_id, 2, 2)
            .expect("compact original and reply");
        assert_eq!(result.removed_events, 2);
        assert_eq!(result.cleared_reply_references, 0);
        let usage = store
            .room_history_usage(room_id)
            .expect("usage")
            .expect("usage row");
        assert_eq!(usage.event_count, 0);
        assert_eq!(usage.retained_bytes, 0);
    }

    #[test]
    fn incomplete_usage_and_excessive_dependency_work_shrinks_then_fails_closed() {
        let (store, room_id) = seeded_store();
        store
            .connection
            .execute(
                "UPDATE room_history_usage SET backfill_complete = 0 WHERE room_id = ?1",
                [room_id],
            )
            .expect("mark incomplete");
        let error = store
            .compact_room_history_through(room_id, 1, 1)
            .expect_err("incomplete ledger must fail")
            .to_string();
        assert!(error.contains("backfill is incomplete"));
        assert_eq!(store.latest_events(room_id, 10).expect("history").len(), 2);

        store
            .connection
            .execute(
                "UPDATE room_history_usage SET backfill_complete = 1 WHERE room_id = ?1",
                [room_id],
            )
            .expect("mark complete");
        let transaction = store
            .connection
            .unchecked_transaction()
            .expect("transaction");
        for index in 0..=MAX_COMPACTION_DEPENDENT_ROWS {
            let event_id = 3 + index as i64;
            transaction
                .execute(
                    "INSERT INTO room_events(
                       room_id, event_id, event_kind, at, payload, reply_to_event_id
                     ) VALUES (?1, ?2, 1, 1, X'78', 2)",
                    (room_id, event_id),
                )
                .expect("dependent reply");
        }
        transaction.commit().expect("commit dependents");
        let result = store
            .compact_room_history_through(room_id, 2, 2)
            .expect("batch should shrink to the independent oldest event");
        assert_eq!(result.removed_events, 1);
        assert_eq!(result.cleared_reply_references, 1);
        assert_eq!(
            store
                .latest_events(room_id, 2)
                .expect("retained newest events")
                .last()
                .map(|event| event.event_id),
            Some(2 + MAX_COMPACTION_DEPENDENT_ROWS as u64 + 1)
        );
        let error = store
            .compact_room_history_through(room_id, 2, 1)
            .expect_err("excessive dependency work must fail")
            .to_string();
        assert!(error.contains("dependent projections"));
        assert_eq!(
            store
                .connection
                .query_row(
                    "SELECT COUNT(*) FROM room_events WHERE room_id = ?1 AND event_id = 2",
                    [room_id],
                    |row| row.get::<_, i64>(0)
                )
                .expect("pathological original retained"),
            1
        );
    }

    #[test]
    fn every_compaction_fault_boundary_rolls_back_all_dependencies_and_ledger() {
        for boundary in [
            CompactionBoundary::ReplyCleanup,
            CompactionBoundary::ReactionCleanup,
            CompactionBoundary::RevisionCleanup,
            CompactionBoundary::PinCleanup,
            CompactionBoundary::EventDelete,
            CompactionBoundary::LedgerUpdate,
            CompactionBoundary::Commit,
        ] {
            let (store, room_id) = seeded_store();
            insert_dependencies(&store, room_id);
            let before = store
                .room_history_usage(room_id)
                .expect("usage")
                .expect("usage row");
            let error = store
                .compact_room_history_through_with_hook(room_id, 1, 1, |observed| {
                    if observed == boundary {
                        Err(ServerError::Message(format!(
                            "injected compaction fault at {observed:?}"
                        )))
                    } else {
                        Ok(())
                    }
                })
                .expect_err("injected failure")
                .to_string();
            assert!(error.contains("injected compaction fault"));
            assert_eq!(store.latest_events(room_id, 10).expect("history").len(), 2);
            assert_eq!(
                store.latest_events(room_id, 10).expect("history")[1]
                    .metadata
                    .as_ref()
                    .and_then(|metadata| metadata.reply_to_event_id),
                Some(1)
            );
            for table in [
                "room_reactions",
                "room_reaction_events",
                "room_message_revision_state",
                "room_message_revision_events",
                "room_pins",
                "room_pin_events",
            ] {
                assert_eq!(
                    store
                        .connection
                        .query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |row| {
                            row.get::<_, i64>(0)
                        })
                        .expect("projection retained"),
                    1
                );
            }
            assert_eq!(
                store
                    .room_history_usage(room_id)
                    .expect("usage")
                    .expect("usage row"),
                before
            );
        }
    }
}
