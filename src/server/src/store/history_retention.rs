use rusqlite::params_from_iter;

use super::{
    current_unix_seconds, ensure_room_event_sequence, room_event_retained_bytes,
    room_history_usage_on, OmenchatStore, HISTORY_REPLY_RETAINED_BYTES,
};
use crate::error::{ServerError, ServerResult};
use crate::protocol::{EventId, RoomId};

pub const MAX_COMPACTED_EVENTS_PER_TRANSACTION: usize = 64;
pub const MAX_COMPACTION_DEPENDENT_ROWS: usize = 20_000;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct RoomHistoryCompaction {
    pub removed_events: usize,
    pub removed_event_bytes: u64,
    pub cleared_reply_references: usize,
    pub removed_reaction_state: usize,
    pub removed_reaction_audit: usize,
    pub removed_revision_state: usize,
    pub removed_revision_audit: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Candidate {
    event_id: i64,
    retained_bytes: u64,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct DependencyCounts {
    replies: usize,
    reaction_state: usize,
    reaction_audit: usize,
    revision_state: usize,
    revision_audit: usize,
}

impl DependencyCounts {
    fn total(self) -> usize {
        self.replies
            .saturating_add(self.reaction_state)
            .saturating_add(self.reaction_audit)
            .saturating_add(self.revision_state)
            .saturating_add(self.revision_audit)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum CompactionBoundary {
    ReplyCleanup,
    ReactionCleanup,
    RevisionCleanup,
    EventDelete,
    LedgerUpdate,
    Commit,
}

impl OmenchatStore {
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
        ensure_room_event_sequence(&transaction, room_id)?;
        let usage = room_history_usage_on(&transaction, room_id)?;
        if !usage.backfill_complete {
            return Err(ServerError::Message(format!(
                "room {room_id} history usage backfill is incomplete"
            )));
        }

        let mut candidates = load_candidates(&transaction, room_id, through, limit)?;
        if candidates.is_empty() {
            transaction.commit()?;
            return Ok(RoomHistoryCompaction::default());
        }
        let dependencies = loop {
            let event_ids = candidates
                .iter()
                .map(|candidate| candidate.event_id)
                .collect::<Vec<_>>();
            let dependencies = dependency_counts(&transaction, room_id, &event_ids)?;
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
            clear_surviving_reply_references(&transaction, room_id, &event_ids)?;
        if cleared_reply_references != dependencies.replies {
            return Err(ServerError::Message(format!(
                "room {room_id} reply projections changed during compaction"
            )));
        }
        hook(CompactionBoundary::ReplyCleanup)?;
        let removed_reaction_state =
            delete_target_rows(&transaction, "room_reactions", room_id, &event_ids)?;
        let removed_reaction_audit =
            delete_target_rows(&transaction, "room_reaction_events", room_id, &event_ids)?;
        if removed_reaction_state != dependencies.reaction_state
            || removed_reaction_audit != dependencies.reaction_audit
        {
            return Err(ServerError::Message(format!(
                "room {room_id} reaction projections changed during compaction"
            )));
        }
        hook(CompactionBoundary::ReactionCleanup)?;
        let removed_revision_state = delete_target_rows(
            &transaction,
            "room_message_revision_state",
            room_id,
            &event_ids,
        )?;
        let removed_revision_audit = delete_target_rows(
            &transaction,
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
        let removed_events = delete_events(&transaction, room_id, &event_ids)?;
        if removed_events != candidates.len() {
            return Err(ServerError::Message(format!(
                "room {room_id} history changed during compaction"
            )));
        }
        hook(CompactionBoundary::EventDelete)?;

        let removed_event_bytes = candidates.iter().try_fold(0u64, |total, candidate| {
            total.checked_add(candidate.retained_bytes).ok_or_else(|| {
                ServerError::Message("room history compaction bytes overflowed".into())
            })
        })?;
        let cleared_reply_bytes = u64::try_from(cleared_reply_references)
            .ok()
            .and_then(|count| count.checked_mul(HISTORY_REPLY_RETAINED_BYTES))
            .ok_or_else(|| {
                ServerError::Message("room history reply-byte accounting overflowed".into())
            })?;
        update_usage(
            &transaction,
            room_id,
            removed_events,
            removed_event_bytes,
            cleared_reply_bytes,
        )?;
        hook(CompactionBoundary::LedgerUpdate)?;
        hook(CompactionBoundary::Commit)?;
        transaction.commit()?;

        Ok(RoomHistoryCompaction {
            removed_events,
            removed_event_bytes,
            cleared_reply_references,
            removed_reaction_state,
            removed_reaction_audit,
            removed_revision_state,
            removed_revision_audit,
        })
    }
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
        )
        .expect("reply");
        transaction.commit().expect("commit");
        (store, room.room_id)
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
                 ) VALUES ({room_id}, 1, 1, 7, 1, X'656469746564', 1, 1, 62);"
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
