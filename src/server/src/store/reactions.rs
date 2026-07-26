use super::{current_unix_seconds, OmenchatStore};
use crate::error::{ServerError, ServerResult};
use crate::protocol::{
    EventId, ReactionAction, ReactionEvent, ReactionRequest, ReactionSnapshot,
    ReactionSnapshotEntry, ReactionToken, RoomId, UserId, REACTION_SNAPSHOT_MAX_ENTRIES,
    REACTION_SNAPSHOT_MAX_TARGETS,
};

pub(crate) const MAX_ACTIVE_TOKENS_PER_ACTOR_TARGET: i64 = 3;
pub(crate) const MAX_ACTIVE_ROWS_PER_TARGET: i64 = 128;
pub(crate) const MAX_ACTIVE_ROWS_PER_ROOM: i64 = 4_096;
pub(crate) const MAX_ACTIVE_BYTES_PER_ROOM: i64 = 128 * 1024;
pub(crate) const MAX_ACTIVE_ROWS_GLOBAL: i64 = 65_536;
pub(crate) const MAX_ACTIVE_BYTES_GLOBAL: i64 = 2 * 1024 * 1024;
pub(crate) const MAX_AUDIT_ROWS_PER_ROOM: i64 = 8_192;
pub(crate) const MAX_AUDIT_BYTES_PER_ROOM: i64 = 512 * 1024;
pub(crate) const MAX_AUDIT_ROWS_GLOBAL: i64 = 131_072;
pub(crate) const MAX_AUDIT_BYTES_GLOBAL: i64 = 8 * 1024 * 1024;
pub(crate) const AUDIT_RETENTION_AGE_SECONDS: i64 = 90 * 24 * 60 * 60;
pub(crate) const MAX_AUDIT_PRUNED_PER_MUTATION: usize = 64;

const ACTIVE_FIXED_RETAINED_BYTES: i64 = 24;
const AUDIT_FIXED_RETAINED_BYTES: i64 = 33;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum ReactionMutationResult {
    Changed(ReactionEvent),
    Unchanged,
    TargetUnavailable,
    Saturated,
}

impl OmenchatStore {
    pub(crate) fn apply_reaction_mutation(
        transaction: &rusqlite::Transaction<'_>,
        room_id: RoomId,
        actor_user_id: UserId,
        request: ReactionRequest,
    ) -> ServerResult<ReactionMutationResult> {
        apply_reaction_mutation_at(
            transaction,
            room_id,
            actor_user_id,
            request,
            current_unix_seconds(),
        )
    }

    pub(crate) fn reaction_snapshot(
        &self,
        room_id: RoomId,
        target_event_ids: &[EventId],
    ) -> ServerResult<ReactionSnapshot> {
        if target_event_ids.len() > REACTION_SNAPSHOT_MAX_TARGETS {
            return Err(ServerError::Message(format!(
                "reaction snapshot exceeds {REACTION_SNAPSHOT_MAX_TARGETS} targets"
            )));
        }
        if target_event_ids.contains(&0)
            || target_event_ids.windows(2).any(|pair| pair[0] >= pair[1])
            || target_event_ids
                .iter()
                .any(|event_id| i64::try_from(*event_id).is_err())
        {
            return Err(ServerError::Message(
                "reaction snapshot target ids must be sorted, unique, nonzero SQLite event ids"
                    .into(),
            ));
        }
        if target_event_ids.is_empty() {
            return Ok(ReactionSnapshot {
                target_event_ids: Vec::new(),
                entries: Vec::new(),
            });
        }

        let placeholders = std::iter::repeat_n("?", target_event_ids.len())
            .collect::<Vec<_>>()
            .join(",");
        let sql = format!(
            "SELECT target_event_id, actor_user_id, reaction_token, created_at
             FROM room_reactions
             WHERE room_id = ? AND target_event_id IN ({placeholders})
             ORDER BY target_event_id, reaction_token, actor_user_id
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
                            "reaction snapshot target id does not fit SQLite".into(),
                        )
                    })
                })
                .collect::<ServerResult<Vec<_>>>()?,
        );
        parameters.push((REACTION_SNAPSHOT_MAX_ENTRIES + 1) as i64);

        let mut statement = self.connection.prepare(&sql)?;
        let entries = statement
            .query_map(rusqlite::params_from_iter(parameters), |row| {
                let target_event_id = row.get::<_, i64>(0)?;
                let actor_user_id = row.get::<_, i64>(1)?;
                let token = row.get::<_, String>(2)?;
                let created_at_unix = row.get::<_, i64>(3)?;
                Ok((target_event_id, actor_user_id, token, created_at_unix))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        if entries.len() > REACTION_SNAPSHOT_MAX_ENTRIES {
            return Err(ServerError::Message(format!(
                "reaction snapshot exceeds {REACTION_SNAPSHOT_MAX_ENTRIES} entries"
            )));
        }
        let entries = entries
            .into_iter()
            .map(|(target_event_id, actor_user_id, token, created_at_unix)| {
                let target_event_id = u64::try_from(target_event_id).map_err(|_| {
                    ServerError::Message("stored reaction target event id is invalid".into())
                })?;
                let actor_user_id = u32::try_from(actor_user_id).map_err(|_| {
                    ServerError::Message("stored reaction actor user id is invalid".into())
                })?;
                let token = ReactionToken::try_from(token.as_str())
                    .map_err(|_| ServerError::Message("stored reaction token is invalid".into()))?;
                Ok(ReactionSnapshotEntry {
                    target_event_id,
                    actor_user_id,
                    token,
                    created_at_unix,
                })
            })
            .collect::<ServerResult<Vec<_>>>()?;
        let snapshot = ReactionSnapshot {
            target_event_ids: target_event_ids.to_vec(),
            entries,
        };
        snapshot.clone().into_frame_body().map_err(|error| {
            ServerError::Message(format!("stored reaction snapshot is invalid: {error}"))
        })?;
        Ok(snapshot)
    }

    #[cfg(test)]
    pub(crate) fn reaction_row_counts(&self) -> ServerResult<(i64, i64)> {
        Ok((
            self.connection
                .query_row("SELECT COUNT(*) FROM room_reactions", [], |row| row.get(0))?,
            self.connection
                .query_row("SELECT COUNT(*) FROM room_reaction_events", [], |row| {
                    row.get(0)
                })?,
        ))
    }
}

fn apply_reaction_mutation_at(
    transaction: &rusqlite::Transaction<'_>,
    room_id: RoomId,
    actor_user_id: UserId,
    request: ReactionRequest,
    now: i64,
) -> ServerResult<ReactionMutationResult> {
    let target_event_id = i64::try_from(request.target_event_id)
        .map_err(|_| ServerError::Message("reaction target event id does not fit SQLite".into()))?;
    let eligible = transaction.query_row(
        "SELECT EXISTS(
           SELECT 1 FROM room_events
           WHERE room_id = ?1 AND event_id = ?2 AND deleted = 0
             AND event_kind IN (1, 2, 3, 5)
         )",
        (room_id, target_event_id),
        |row| row.get::<_, bool>(0),
    )?;
    if !eligible {
        return Ok(ReactionMutationResult::TargetUnavailable);
    }

    let token = request.token.as_str();
    let exists = transaction.query_row(
        "SELECT EXISTS(
           SELECT 1 FROM room_reactions
           WHERE room_id = ?1 AND target_event_id = ?2
             AND actor_user_id = ?3 AND reaction_token = ?4
         )",
        (room_id, target_event_id, actor_user_id, token),
        |row| row.get::<_, bool>(0),
    )?;
    let changed = match request.action {
        ReactionAction::Add => !exists,
        ReactionAction::Remove => exists,
    };

    let mut pruned = prune_expired_audit_rows(transaction, now)?;
    if !changed {
        return Ok(ReactionMutationResult::Unchanged);
    }
    if !ensure_audit_capacity(transaction, room_id, token.len(), &mut pruned)? {
        return Ok(ReactionMutationResult::Saturated);
    }
    if request.action == ReactionAction::Add
        && !active_add_has_capacity(
            transaction,
            room_id,
            target_event_id,
            actor_user_id,
            token.len(),
        )?
    {
        return Ok(ReactionMutationResult::Saturated);
    }

    match request.action {
        ReactionAction::Add => {
            transaction.execute(
                "INSERT INTO room_reactions(
                   room_id, target_event_id, actor_user_id, reaction_token, created_at
                 ) VALUES (?1, ?2, ?3, ?4, ?5)",
                (room_id, target_event_id, actor_user_id, token, now),
            )?;
        }
        ReactionAction::Remove => {
            transaction.execute(
                "DELETE FROM room_reactions
                 WHERE room_id = ?1 AND target_event_id = ?2
                   AND actor_user_id = ?3 AND reaction_token = ?4",
                (room_id, target_event_id, actor_user_id, token),
            )?;
        }
    }

    let reaction_event_id: i64 = transaction.query_row(
        "SELECT COALESCE(MAX(reaction_event_id), 0) + 1
         FROM room_reaction_events WHERE room_id = ?1",
        [room_id],
        |row| row.get(0),
    )?;
    let retained_bytes = AUDIT_FIXED_RETAINED_BYTES.saturating_add(token.len() as i64);
    transaction.execute(
        "INSERT INTO room_reaction_events(
           room_id, reaction_event_id, target_event_id, actor_user_id,
           reaction_token, reaction_action, at, retained_bytes
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        (
            room_id,
            reaction_event_id,
            target_event_id,
            actor_user_id,
            token,
            request.action as u8,
            now,
            retained_bytes,
        ),
    )?;

    Ok(ReactionMutationResult::Changed(ReactionEvent {
        reaction_event_id: reaction_event_id as u64,
        target_event_id: request.target_event_id,
        actor_user_id,
        token: request.token,
        action: request.action,
        at_unix: now,
    }))
}

fn active_add_has_capacity(
    transaction: &rusqlite::Transaction<'_>,
    room_id: RoomId,
    target_event_id: i64,
    actor_user_id: UserId,
    token_bytes: usize,
) -> ServerResult<bool> {
    let actor_target_rows: i64 = transaction.query_row(
        "SELECT COUNT(*) FROM room_reactions
         WHERE room_id = ?1 AND target_event_id = ?2 AND actor_user_id = ?3",
        (room_id, target_event_id, actor_user_id),
        |row| row.get(0),
    )?;
    let target_rows: i64 = transaction.query_row(
        "SELECT COUNT(*) FROM room_reactions
         WHERE room_id = ?1 AND target_event_id = ?2",
        (room_id, target_event_id),
        |row| row.get(0),
    )?;
    let (room_rows, room_bytes) = active_usage(transaction, Some(room_id))?;
    let (global_rows, global_bytes) = active_usage(transaction, None)?;
    let incoming_bytes = ACTIVE_FIXED_RETAINED_BYTES.saturating_add(token_bytes as i64);
    Ok(
        actor_target_rows.saturating_add(1) <= MAX_ACTIVE_TOKENS_PER_ACTOR_TARGET
            && target_rows.saturating_add(1) <= MAX_ACTIVE_ROWS_PER_TARGET
            && room_rows.saturating_add(1) <= MAX_ACTIVE_ROWS_PER_ROOM
            && room_bytes.saturating_add(incoming_bytes) <= MAX_ACTIVE_BYTES_PER_ROOM
            && global_rows.saturating_add(1) <= MAX_ACTIVE_ROWS_GLOBAL
            && global_bytes.saturating_add(incoming_bytes) <= MAX_ACTIVE_BYTES_GLOBAL,
    )
}

fn active_usage(
    transaction: &rusqlite::Transaction<'_>,
    room_id: Option<RoomId>,
) -> ServerResult<(i64, i64)> {
    let query = |sql: &str, parameter: Option<RoomId>| {
        if let Some(room_id) = parameter {
            transaction.query_row(sql, [room_id], |row| Ok((row.get(0)?, row.get(1)?)))
        } else {
            transaction.query_row(sql, [], |row| Ok((row.get(0)?, row.get(1)?)))
        }
    };
    match room_id {
        Some(room_id) => query(
            "SELECT COUNT(*), COALESCE(SUM(24 + length(CAST(reaction_token AS BLOB))), 0)
             FROM room_reactions WHERE room_id = ?1",
            Some(room_id),
        ),
        None => query(
            "SELECT COUNT(*), COALESCE(SUM(24 + length(CAST(reaction_token AS BLOB))), 0)
             FROM room_reactions",
            None,
        ),
    }
    .map_err(Into::into)
}

fn prune_expired_audit_rows(
    transaction: &rusqlite::Transaction<'_>,
    now: i64,
) -> ServerResult<usize> {
    let cutoff = now.saturating_sub(AUDIT_RETENTION_AGE_SECONDS);
    transaction
        .execute(
            "DELETE FROM room_reaction_events
             WHERE rowid IN (
               SELECT rowid FROM room_reaction_events
               WHERE at < ?1
               ORDER BY at, room_id, reaction_event_id
               LIMIT ?2
             )",
            (cutoff, MAX_AUDIT_PRUNED_PER_MUTATION as i64),
        )
        .map_err(Into::into)
}

fn ensure_audit_capacity(
    transaction: &rusqlite::Transaction<'_>,
    room_id: RoomId,
    token_bytes: usize,
    pruned: &mut usize,
) -> ServerResult<bool> {
    let incoming_bytes = AUDIT_FIXED_RETAINED_BYTES.saturating_add(token_bytes as i64);
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
            "DELETE FROM room_reaction_events
             WHERE rowid = (
               SELECT rowid FROM room_reaction_events
               ORDER BY CASE WHEN room_id = ?1 THEN 0 ELSE 1 END,
                        at, room_id, reaction_event_id
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
             FROM room_reaction_events WHERE room_id = ?1",
            [room_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        ),
        None => transaction.query_row(
            "SELECT COUNT(*), COALESCE(SUM(retained_bytes), 0)
             FROM room_reaction_events",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        ),
    };
    result.map_err(Into::into)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::ServerRoomEventKind;

    fn setup() -> (OmenchatStore, RoomId, UserId, EventId) {
        let store = OmenchatStore::in_memory().expect("store");
        let room = store
            .ensure_room("reactions", Some("test room"))
            .expect("room");
        let user = store
            .ensure_user(b"reaction-user", "alice", None)
            .expect("user");
        store.join_room(room.room_id, user.user_id).expect("join");
        let event = store
            .append_event(
                room.room_id,
                Some(user.user_id),
                ServerRoomEventKind::Message {
                    body: "target".into(),
                },
            )
            .expect("event");
        (store, room.room_id, user.user_id, event.event_id)
    }

    fn mutate(
        store: &OmenchatStore,
        room_id: RoomId,
        actor_user_id: UserId,
        target_event_id: EventId,
        token: ReactionToken,
        action: ReactionAction,
        now: i64,
    ) -> ReactionMutationResult {
        let transaction = rusqlite::Transaction::new_unchecked(
            &store.connection,
            rusqlite::TransactionBehavior::Immediate,
        )
        .expect("transaction");
        let result = apply_reaction_mutation_at(
            &transaction,
            room_id,
            actor_user_id,
            ReactionRequest {
                target_event_id,
                token,
                action,
            },
            now,
        )
        .expect("mutation");
        transaction.commit().expect("commit");
        result
    }

    #[test]
    fn add_remove_and_noop_have_exact_state_and_audit_semantics() {
        let (store, room_id, user_id, target_event_id) = setup();
        assert!(matches!(
            mutate(
                &store,
                room_id,
                user_id,
                target_event_id,
                ReactionToken::Heart,
                ReactionAction::Add,
                100
            ),
            ReactionMutationResult::Changed(ReactionEvent {
                reaction_event_id: 1,
                action: ReactionAction::Add,
                ..
            })
        ));
        assert_eq!(
            mutate(
                &store,
                room_id,
                user_id,
                target_event_id,
                ReactionToken::Heart,
                ReactionAction::Add,
                101
            ),
            ReactionMutationResult::Unchanged
        );
        assert!(matches!(
            mutate(
                &store,
                room_id,
                user_id,
                target_event_id,
                ReactionToken::Heart,
                ReactionAction::Remove,
                102
            ),
            ReactionMutationResult::Changed(ReactionEvent {
                reaction_event_id: 2,
                action: ReactionAction::Remove,
                ..
            })
        ));
        assert_eq!(
            mutate(
                &store,
                room_id,
                user_id,
                target_event_id,
                ReactionToken::Heart,
                ReactionAction::Remove,
                103
            ),
            ReactionMutationResult::Unchanged
        );
        assert_eq!(
            store
                .connection
                .query_row("SELECT COUNT(*) FROM room_reactions", [], |row| {
                    row.get::<_, i64>(0)
                })
                .expect("active rows"),
            0
        );
        assert_eq!(
            store
                .connection
                .query_row("SELECT COUNT(*) FROM room_reaction_events", [], |row| {
                    row.get::<_, i64>(0)
                })
                .expect("audit rows"),
            2
        );
    }

    #[test]
    fn actor_target_limit_rejects_fourth_token_but_remove_recovers() {
        let (store, room_id, user_id, target_event_id) = setup();
        for token in [
            ReactionToken::Celebrate,
            ReactionToken::Heart,
            ReactionToken::Laugh,
        ] {
            assert!(matches!(
                mutate(
                    &store,
                    room_id,
                    user_id,
                    target_event_id,
                    token,
                    ReactionAction::Add,
                    100
                ),
                ReactionMutationResult::Changed(_)
            ));
        }
        assert_eq!(
            mutate(
                &store,
                room_id,
                user_id,
                target_event_id,
                ReactionToken::Question,
                ReactionAction::Add,
                101
            ),
            ReactionMutationResult::Saturated
        );
        assert!(matches!(
            mutate(
                &store,
                room_id,
                user_id,
                target_event_id,
                ReactionToken::Heart,
                ReactionAction::Remove,
                102
            ),
            ReactionMutationResult::Changed(_)
        ));
    }

    #[test]
    fn snapshot_is_authoritative_sorted_and_explicit_when_empty() {
        let (store, room_id, user_id, target_event_id) = setup();
        mutate(
            &store,
            room_id,
            user_id,
            target_event_id,
            ReactionToken::Heart,
            ReactionAction::Add,
            100,
        );
        mutate(
            &store,
            room_id,
            user_id,
            target_event_id,
            ReactionToken::Celebrate,
            ReactionAction::Add,
            101,
        );
        let snapshot = store
            .reaction_snapshot(room_id, &[target_event_id])
            .expect("snapshot");
        assert_eq!(snapshot.target_event_ids, vec![target_event_id]);
        assert_eq!(snapshot.entries.len(), 2);
        assert_eq!(snapshot.entries[0].token, ReactionToken::Celebrate);
        assert_eq!(snapshot.entries[1].token, ReactionToken::Heart);

        let empty = store
            .reaction_snapshot(room_id, &[target_event_id + 1])
            .expect("empty snapshot");
        assert_eq!(empty.target_event_ids, vec![target_event_id + 1]);
        assert!(empty.entries.is_empty());
    }

    #[test]
    fn age_pruning_is_incremental_and_never_removes_active_state() {
        let (store, room_id, user_id, target_event_id) = setup();
        for index in 0..=MAX_AUDIT_PRUNED_PER_MUTATION {
            store
                .connection
                .execute(
                    "INSERT INTO room_reaction_events(
                       room_id, reaction_event_id, target_event_id, actor_user_id,
                       reaction_token, reaction_action, at, retained_bytes
                     ) VALUES (?1, ?2, ?3, ?4, 'heart', 1, 0, 38)",
                    (room_id, (index + 1) as i64, target_event_id as i64, user_id),
                )
                .expect("old audit row");
        }
        assert!(matches!(
            mutate(
                &store,
                room_id,
                user_id,
                target_event_id,
                ReactionToken::Heart,
                ReactionAction::Add,
                AUDIT_RETENTION_AGE_SECONDS + 1
            ),
            ReactionMutationResult::Changed(_)
        ));
        let audit_rows: i64 = store
            .connection
            .query_row("SELECT COUNT(*) FROM room_reaction_events", [], |row| {
                row.get(0)
            })
            .expect("audit rows");
        assert_eq!(audit_rows, 2, "64 old rows plus one new row remain bounded");
        assert_eq!(
            store
                .connection
                .query_row("SELECT COUNT(*) FROM room_reactions", [], |row| {
                    row.get::<_, i64>(0)
                })
                .expect("active rows"),
            1
        );
    }
}
