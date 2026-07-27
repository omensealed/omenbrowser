use super::{current_unix_seconds, OmenchatStore};
use crate::error::{ServerError, ServerResult};
use crate::protocol::{
    EventId, PinAction, PinEvent, PinRequest, PinSnapshot, PinSnapshotEntry, RoomId, UserId,
    ROOM_PIN_SNAPSHOT_MAX_ENTRIES, ROOM_PIN_SNAPSHOT_MAX_TARGETS,
};
use rusqlite::OptionalExtension;

pub const MAX_ACTIVE_PINS_PER_ROOM: i64 = 64;
pub const MAX_ACTIVE_PINS_GLOBAL: i64 = 4_096;
pub const MAX_ACTIVE_PIN_BYTES_GLOBAL: i64 = 1024 * 1024;
pub const MAX_PIN_AUDIT_ROWS_PER_ROOM: i64 = 1_024;
pub const MAX_PIN_AUDIT_BYTES_PER_ROOM: i64 = 256 * 1024;
pub const MAX_PIN_AUDIT_ROWS_GLOBAL: i64 = 16_384;
pub const MAX_PIN_AUDIT_BYTES_GLOBAL: i64 = 4 * 1024 * 1024;
pub const PIN_AUDIT_RETENTION_AGE_SECONDS: i64 = 180 * 24 * 60 * 60;
pub const MAX_PIN_AUDIT_PRUNED_PER_MUTATION: usize = 64;

const ACTIVE_PIN_RETAINED_BYTES: i64 = 32;
const PIN_AUDIT_RETAINED_BYTES: i64 = 41;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PinMutationResult {
    Changed(PinEvent),
    Unchanged,
    TargetUnavailable,
    Saturated,
}

impl OmenchatStore {
    /// Applies one dormant pin mutation inside the caller-owned transaction.
    ///
    /// The future durable-mutation executor must use this transaction boundary
    /// so the pin state, audit row, and replay result either commit together or
    /// all roll back. This method does not activate protocol capability
    /// negotiation or live fanout.
    pub fn apply_pin_mutation(
        transaction: &rusqlite::Transaction<'_>,
        room_id: RoomId,
        actor_user_id: UserId,
        request: PinRequest,
    ) -> ServerResult<PinMutationResult> {
        apply_pin_mutation_at(
            transaction,
            room_id,
            actor_user_id,
            request,
            current_unix_seconds(),
        )
    }

    pub fn pin_snapshot(
        &self,
        room_id: RoomId,
        target_event_ids: &[EventId],
    ) -> ServerResult<PinSnapshot> {
        validate_snapshot_targets(target_event_ids)?;
        if target_event_ids.is_empty() {
            return Ok(PinSnapshot {
                target_event_ids: Vec::new(),
                entries: Vec::new(),
            });
        }

        let placeholders = std::iter::repeat_n("?", target_event_ids.len())
            .collect::<Vec<_>>()
            .join(",");
        let sql = format!(
            "SELECT target_event_id, pin_event_id, actor_user_id, pinned_at
             FROM room_pins
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
                        ServerError::Message("pin snapshot target id does not fit SQLite".into())
                    })
                })
                .collect::<ServerResult<Vec<_>>>()?,
        );
        parameters.push((ROOM_PIN_SNAPSHOT_MAX_ENTRIES + 1) as i64);

        let mut statement = self.connection.prepare(&sql)?;
        let rows = statement
            .query_map(rusqlite::params_from_iter(parameters), |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, i64>(1)?,
                    row.get::<_, i64>(2)?,
                    row.get::<_, i64>(3)?,
                ))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        if rows.len() > ROOM_PIN_SNAPSHOT_MAX_ENTRIES {
            return Err(ServerError::Message(format!(
                "pin snapshot exceeds {ROOM_PIN_SNAPSHOT_MAX_ENTRIES} entries"
            )));
        }
        let entries = rows
            .into_iter()
            .map(
                |(target_event_id, pin_event_id, actor_user_id, pinned_at_unix)| {
                    Ok(PinSnapshotEntry {
                        target_event_id: u64::try_from(target_event_id).map_err(|_| {
                            ServerError::Message("stored pin target event id is invalid".into())
                        })?,
                        pin_event_id: u64::try_from(pin_event_id).map_err(|_| {
                            ServerError::Message("stored pin event id is invalid".into())
                        })?,
                        actor_user_id: u32::try_from(actor_user_id).map_err(|_| {
                            ServerError::Message("stored pin actor user id is invalid".into())
                        })?,
                        pinned_at_unix,
                    })
                },
            )
            .collect::<ServerResult<Vec<_>>>()?;
        let snapshot = PinSnapshot {
            target_event_ids: target_event_ids.to_vec(),
            entries,
        };
        snapshot.clone().into_frame_body().map_err(|error| {
            ServerError::Message(format!("stored pin snapshot is invalid: {error}"))
        })?;
        Ok(snapshot)
    }

    #[cfg(test)]
    pub(crate) fn pin_row_counts(&self) -> ServerResult<(i64, i64)> {
        Ok((
            self.connection
                .query_row("SELECT COUNT(*) FROM room_pins", [], |row| row.get(0))?,
            self.connection
                .query_row("SELECT COUNT(*) FROM room_pin_events", [], |row| row.get(0))?,
        ))
    }
}

fn validate_snapshot_targets(target_event_ids: &[EventId]) -> ServerResult<()> {
    if target_event_ids.len() > ROOM_PIN_SNAPSHOT_MAX_TARGETS {
        return Err(ServerError::Message(format!(
            "pin snapshot exceeds {ROOM_PIN_SNAPSHOT_MAX_TARGETS} targets"
        )));
    }
    if target_event_ids.contains(&0)
        || target_event_ids.windows(2).any(|pair| pair[0] >= pair[1])
        || target_event_ids
            .iter()
            .any(|event_id| i64::try_from(*event_id).is_err())
    {
        return Err(ServerError::Message(
            "pin snapshot target ids must be sorted, unique, nonzero SQLite event ids".into(),
        ));
    }
    Ok(())
}

fn apply_pin_mutation_at(
    transaction: &rusqlite::Transaction<'_>,
    room_id: RoomId,
    actor_user_id: UserId,
    request: PinRequest,
    now: i64,
) -> ServerResult<PinMutationResult> {
    if actor_user_id == 0 {
        return Err(ServerError::Message(
            "pin actor user id must be nonzero".into(),
        ));
    }
    let target_event_id = i64::try_from(request.target_event_id)
        .map_err(|_| ServerError::Message("pin target event id does not fit SQLite".into()))?;
    let current_pin_event_id = transaction
        .query_row(
            "SELECT pin_event_id FROM room_pins
             WHERE room_id = ?1 AND target_event_id = ?2",
            (room_id, target_event_id),
            |row| row.get::<_, i64>(0),
        )
        .optional()?;
    let changed = match request.action {
        PinAction::Pin => current_pin_event_id.is_none(),
        PinAction::Unpin => current_pin_event_id.is_some(),
    };

    let target_eligible = transaction.query_row(
        "SELECT EXISTS(
           SELECT 1 FROM room_events AS e
           WHERE e.room_id = ?1 AND e.event_id = ?2 AND e.deleted = 0
             AND e.event_kind IN (1, 2, 3, 5)
             AND NOT EXISTS(
               SELECT 1 FROM room_message_revision_state AS r
               WHERE r.room_id = e.room_id AND r.target_event_id = e.event_id
                 AND r.revision_action = 2
             )
         )",
        (room_id, target_event_id),
        |row| row.get::<_, bool>(0),
    )?;
    if request.action == PinAction::Pin && !target_eligible {
        return Ok(PinMutationResult::TargetUnavailable);
    }
    if request.action == PinAction::Unpin && current_pin_event_id.is_none() && !target_eligible {
        return Ok(PinMutationResult::TargetUnavailable);
    }

    let mut pruned = prune_expired_pin_audit(transaction, now)?;
    if !changed {
        return Ok(PinMutationResult::Unchanged);
    }
    if request.action == PinAction::Pin && !active_pin_add_has_capacity(transaction, room_id)? {
        return Ok(PinMutationResult::Saturated);
    }
    let releasable_pin_event_id = (request.action == PinAction::Unpin)
        .then_some(current_pin_event_id)
        .flatten();
    if !ensure_pin_audit_capacity(transaction, room_id, releasable_pin_event_id, &mut pruned)? {
        return Ok(PinMutationResult::Saturated);
    }

    transaction.execute(
        "INSERT INTO room_pin_events(
           room_id, target_event_id, actor_user_id, pin_action, at, retained_bytes
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        (
            room_id,
            target_event_id,
            actor_user_id,
            request.action as u8,
            now,
            PIN_AUDIT_RETAINED_BYTES,
        ),
    )?;
    let pin_event_id = transaction.last_insert_rowid();
    if pin_event_id <= 0 {
        return Err(ServerError::Message(
            "pin event identifier space is exhausted".into(),
        ));
    }

    match request.action {
        PinAction::Pin => {
            transaction.execute(
                "INSERT INTO room_pins(
                   room_id, target_event_id, pin_event_id, actor_user_id,
                   pinned_at, retained_bytes
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                (
                    room_id,
                    target_event_id,
                    pin_event_id,
                    actor_user_id,
                    now,
                    ACTIVE_PIN_RETAINED_BYTES,
                ),
            )?;
        }
        PinAction::Unpin => {
            let deleted = transaction.execute(
                "DELETE FROM room_pins
                 WHERE room_id = ?1 AND target_event_id = ?2",
                (room_id, target_event_id),
            )?;
            if deleted != 1 {
                return Err(ServerError::Message(
                    "pin state changed during mutation".into(),
                ));
            }
        }
    }

    Ok(PinMutationResult::Changed(PinEvent {
        pin_event_id: pin_event_id as u64,
        target_event_id: request.target_event_id,
        action: request.action,
        actor_user_id,
        at_unix: now,
    }))
}

fn active_pin_add_has_capacity(
    transaction: &rusqlite::Transaction<'_>,
    room_id: RoomId,
) -> ServerResult<bool> {
    let room_rows: i64 = transaction.query_row(
        "SELECT COUNT(*) FROM room_pins WHERE room_id = ?1",
        [room_id],
        |row| row.get(0),
    )?;
    let (global_rows, global_bytes): (i64, i64) = transaction.query_row(
        "SELECT COUNT(*), COALESCE(SUM(retained_bytes), 0) FROM room_pins",
        [],
        |row| Ok((row.get(0)?, row.get(1)?)),
    )?;
    Ok(room_rows.saturating_add(1) <= MAX_ACTIVE_PINS_PER_ROOM
        && global_rows.saturating_add(1) <= MAX_ACTIVE_PINS_GLOBAL
        && global_bytes.saturating_add(ACTIVE_PIN_RETAINED_BYTES) <= MAX_ACTIVE_PIN_BYTES_GLOBAL)
}

fn prune_expired_pin_audit(
    transaction: &rusqlite::Transaction<'_>,
    now: i64,
) -> ServerResult<usize> {
    let cutoff = now.saturating_sub(PIN_AUDIT_RETENTION_AGE_SECONDS);
    transaction
        .execute(
            "DELETE FROM room_pin_events
             WHERE pin_event_id IN (
               SELECT e.pin_event_id FROM room_pin_events AS e
               WHERE e.at < ?1
                 AND NOT EXISTS(
                   SELECT 1 FROM room_pins AS p
                   WHERE p.pin_event_id = e.pin_event_id
                 )
               ORDER BY e.at, e.room_id, e.pin_event_id
               LIMIT ?2
             )",
            (cutoff, MAX_PIN_AUDIT_PRUNED_PER_MUTATION as i64),
        )
        .map_err(Into::into)
}

fn ensure_pin_audit_capacity(
    transaction: &rusqlite::Transaction<'_>,
    room_id: RoomId,
    releasable_pin_event_id: Option<i64>,
    pruned: &mut usize,
) -> ServerResult<bool> {
    loop {
        let (room_rows, room_bytes) = pin_audit_usage(transaction, Some(room_id))?;
        let (global_rows, global_bytes) = pin_audit_usage(transaction, None)?;
        if room_rows.saturating_add(1) <= MAX_PIN_AUDIT_ROWS_PER_ROOM
            && room_bytes.saturating_add(PIN_AUDIT_RETAINED_BYTES) <= MAX_PIN_AUDIT_BYTES_PER_ROOM
            && global_rows.saturating_add(1) <= MAX_PIN_AUDIT_ROWS_GLOBAL
            && global_bytes.saturating_add(PIN_AUDIT_RETAINED_BYTES) <= MAX_PIN_AUDIT_BYTES_GLOBAL
        {
            return Ok(true);
        }
        if *pruned >= MAX_PIN_AUDIT_PRUNED_PER_MUTATION {
            return Ok(false);
        }
        let deleted = transaction.execute(
            "DELETE FROM room_pin_events
             WHERE pin_event_id = (
               SELECT e.pin_event_id FROM room_pin_events AS e
               WHERE (
                 NOT EXISTS(
                   SELECT 1 FROM room_pins AS p
                   WHERE p.pin_event_id = e.pin_event_id
                 ) OR e.pin_event_id = ?2
               )
               ORDER BY CASE WHEN e.room_id = ?1 THEN 0 ELSE 1 END,
                        e.at, e.room_id, e.pin_event_id
               LIMIT 1
             )",
            (room_id, releasable_pin_event_id),
        )?;
        if deleted == 0 {
            return Ok(false);
        }
        *pruned = pruned.saturating_add(deleted);
    }
}

fn pin_audit_usage(
    transaction: &rusqlite::Transaction<'_>,
    room_id: Option<RoomId>,
) -> ServerResult<(i64, i64)> {
    let result = match room_id {
        Some(room_id) => transaction.query_row(
            "SELECT COUNT(*), COALESCE(SUM(retained_bytes), 0)
             FROM room_pin_events WHERE room_id = ?1",
            [room_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        ),
        None => transaction.query_row(
            "SELECT COUNT(*), COALESCE(SUM(retained_bytes), 0)
             FROM room_pin_events",
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
    fn setup(targets: usize) -> (OmenchatStore, RoomId, UserId, Vec<EventId>) {
        let store = OmenchatStore::in_memory().expect("store");
        let room = store.ensure_room("pins", None).expect("room");
        let user = store.ensure_user(b"pin-user", "alice", None).expect("user");
        store.join_room(room.room_id, user.user_id).expect("join");
        let target_event_ids = (0..targets)
            .map(|index| {
                store
                    .append_event(
                        room.room_id,
                        Some(user.user_id),
                        ServerRoomEventKind::Message {
                            body: format!("target {index}"),
                        },
                    )
                    .expect("target")
                    .event_id
            })
            .collect();
        (store, room.room_id, user.user_id, target_event_ids)
    }

    fn apply(
        store: &OmenchatStore,
        room_id: RoomId,
        actor_user_id: UserId,
        target_event_id: EventId,
        action: PinAction,
        now: i64,
    ) -> PinMutationResult {
        let transaction = store
            .connection
            .unchecked_transaction()
            .expect("transaction");
        let result = apply_pin_mutation_at(
            &transaction,
            room_id,
            actor_user_id,
            PinRequest {
                target_event_id,
                action,
            },
            now,
        )
        .expect("pin mutation");
        transaction.commit().expect("commit");
        result
    }

    #[test]
    fn pin_unpin_and_exact_noop_are_transactional_and_snapshot_scoped() {
        let (store, room_id, actor_user_id, targets) = setup(2);
        let first = apply(
            &store,
            room_id,
            actor_user_id,
            targets[0],
            PinAction::Pin,
            10,
        );
        assert!(matches!(
            first,
            PinMutationResult::Changed(PinEvent {
                pin_event_id: 1,
                action: PinAction::Pin,
                ..
            })
        ));
        assert_eq!(
            apply(
                &store,
                room_id,
                actor_user_id,
                targets[0],
                PinAction::Pin,
                11
            ),
            PinMutationResult::Unchanged
        );
        assert_eq!(store.pin_row_counts().expect("counts"), (1, 1));

        let snapshot = store.pin_snapshot(room_id, &targets).expect("snapshot");
        assert_eq!(snapshot.target_event_ids, targets);
        assert_eq!(snapshot.entries.len(), 1);
        assert_eq!(snapshot.entries[0].target_event_id, targets[0]);

        assert!(matches!(
            apply(
                &store,
                room_id,
                actor_user_id,
                targets[0],
                PinAction::Unpin,
                12
            ),
            PinMutationResult::Changed(PinEvent {
                pin_event_id: 2,
                action: PinAction::Unpin,
                ..
            })
        ));
        assert_eq!(store.pin_row_counts().expect("counts"), (0, 2));
        assert!(store
            .pin_snapshot(room_id, &targets)
            .expect("empty replacement")
            .entries
            .is_empty());
    }

    #[test]
    fn pin_rejects_missing_cross_room_and_tombstoned_targets_but_allows_unpin() {
        let (store, room_id, actor_user_id, targets) = setup(1);
        assert_eq!(
            apply(
                &store,
                room_id,
                actor_user_id,
                targets[0] + 100,
                PinAction::Pin,
                10
            ),
            PinMutationResult::TargetUnavailable
        );
        let other = store.ensure_room("other-pins", None).expect("other room");
        assert_eq!(
            apply(
                &store,
                other.room_id,
                actor_user_id,
                targets[0],
                PinAction::Pin,
                10
            ),
            PinMutationResult::TargetUnavailable
        );

        assert!(matches!(
            apply(
                &store,
                room_id,
                actor_user_id,
                targets[0],
                PinAction::Pin,
                10
            ),
            PinMutationResult::Changed(_)
        ));
        store
            .connection
            .execute(
                "INSERT INTO room_message_revision_state(
                   room_id, target_event_id, latest_revision_event_id, revision_action,
                   actor_user_id, replacement_body, revision_number, at, retained_bytes
                 ) VALUES (?1, ?2, 100, 2, ?3, NULL, 1, 11, 32)",
                (room_id, targets[0], actor_user_id),
            )
            .expect("tombstone projection");
        assert!(matches!(
            apply(
                &store,
                room_id,
                actor_user_id,
                targets[0],
                PinAction::Unpin,
                12
            ),
            PinMutationResult::Changed(_)
        ));
        assert_eq!(
            apply(
                &store,
                room_id,
                actor_user_id,
                targets[0],
                PinAction::Pin,
                13
            ),
            PinMutationResult::TargetUnavailable
        );
    }

    #[test]
    fn active_room_limit_fails_closed_while_unpin_remains_possible() {
        let (store, room_id, actor_user_id, targets) = setup(MAX_ACTIVE_PINS_PER_ROOM as usize + 1);
        for target_event_id in targets.iter().take(MAX_ACTIVE_PINS_PER_ROOM as usize) {
            assert!(matches!(
                apply(
                    &store,
                    room_id,
                    actor_user_id,
                    *target_event_id,
                    PinAction::Pin,
                    10
                ),
                PinMutationResult::Changed(_)
            ));
        }
        assert_eq!(
            apply(
                &store,
                room_id,
                actor_user_id,
                targets[MAX_ACTIVE_PINS_PER_ROOM as usize],
                PinAction::Pin,
                11
            ),
            PinMutationResult::Saturated
        );
        assert!(matches!(
            apply(
                &store,
                room_id,
                actor_user_id,
                targets[0],
                PinAction::Unpin,
                12
            ),
            PinMutationResult::Changed(_)
        ));
    }

    #[test]
    fn audit_pruning_is_bounded_and_never_prunes_current_pin_event() {
        let (store, room_id, actor_user_id, targets) = setup(1);
        let current = apply(
            &store,
            room_id,
            actor_user_id,
            targets[0],
            PinAction::Pin,
            1,
        );
        let current_id = match current {
            PinMutationResult::Changed(event) => event.pin_event_id,
            _ => panic!("changed pin"),
        };
        store
            .connection
            .execute_batch(
                "WITH RECURSIVE seq(value) AS (
                   SELECT 1 UNION ALL SELECT value + 1 FROM seq WHERE value < 70
                 )
                 INSERT INTO room_pin_events(
                   room_id, target_event_id, actor_user_id, pin_action, at, retained_bytes
                 )
                 SELECT 999, value, 1, 2, 1, 41 FROM seq;",
            )
            .expect("old audit");
        let transaction = store
            .connection
            .unchecked_transaction()
            .expect("transaction");
        assert_eq!(
            prune_expired_pin_audit(
                &transaction,
                PIN_AUDIT_RETENTION_AGE_SECONDS.saturating_add(2)
            )
            .expect("prune"),
            MAX_PIN_AUDIT_PRUNED_PER_MUTATION
        );
        transaction.commit().expect("commit");
        assert_eq!(
            store
                .connection
                .query_row(
                    "SELECT pin_event_id FROM room_pins WHERE room_id = ?1",
                    [room_id],
                    |row| row.get::<_, i64>(0)
                )
                .expect("current pin"),
            current_id as i64
        );
        assert_eq!(
            store
                .connection
                .query_row(
                    "SELECT 1 FROM room_pin_events WHERE pin_event_id = ?1",
                    [current_id],
                    |row| row.get::<_, i64>(0)
                )
                .optional()
                .expect("audit lookup"),
            Some(1)
        );
    }

    #[test]
    fn autoincrement_pin_event_ids_are_not_reused_after_audit_pruning() {
        let (store, room_id, actor_user_id, targets) = setup(1);
        assert!(matches!(
            apply(
                &store,
                room_id,
                actor_user_id,
                targets[0],
                PinAction::Pin,
                1
            ),
            PinMutationResult::Changed(PinEvent {
                pin_event_id: 1,
                ..
            })
        ));
        assert!(matches!(
            apply(
                &store,
                room_id,
                actor_user_id,
                targets[0],
                PinAction::Unpin,
                2
            ),
            PinMutationResult::Changed(PinEvent {
                pin_event_id: 2,
                ..
            })
        ));
        store
            .connection
            .execute("DELETE FROM room_pin_events", [])
            .expect("prune all audit");
        assert!(matches!(
            apply(
                &store,
                room_id,
                actor_user_id,
                targets[0],
                PinAction::Pin,
                3
            ),
            PinMutationResult::Changed(PinEvent {
                pin_event_id: 3,
                ..
            })
        ));
    }

    #[test]
    fn transaction_rollback_leaves_no_pin_state_or_audit() {
        let (store, room_id, actor_user_id, targets) = setup(1);
        let transaction = store
            .connection
            .unchecked_transaction()
            .expect("transaction");
        assert!(matches!(
            OmenchatStore::apply_pin_mutation(
                &transaction,
                room_id,
                actor_user_id,
                PinRequest {
                    target_event_id: targets[0],
                    action: PinAction::Pin,
                }
            )
            .expect("pin"),
            PinMutationResult::Changed(_)
        ));
        transaction.rollback().expect("rollback");
        assert_eq!(store.pin_row_counts().expect("counts"), (0, 0));
    }

    #[test]
    fn full_room_audit_replaces_one_oldest_eligible_row_without_touching_active_state() {
        let (store, room_id, actor_user_id, targets) = setup(2);
        let first = apply(
            &store,
            room_id,
            actor_user_id,
            targets[0],
            PinAction::Pin,
            10,
        );
        let first_pin_event_id = match first {
            PinMutationResult::Changed(event) => event.pin_event_id,
            _ => panic!("first pin"),
        };
        let transaction = store
            .connection
            .unchecked_transaction()
            .expect("seed audit");
        {
            let mut statement = transaction
                .prepare(
                    "INSERT INTO room_pin_events(
                       room_id, target_event_id, actor_user_id, pin_action, at, retained_bytes
                     ) VALUES (?1, ?2, ?3, 2, 1, ?4)",
                )
                .expect("audit insert");
            for target_event_id in 1..MAX_PIN_AUDIT_ROWS_PER_ROOM {
                statement
                    .execute((
                        room_id,
                        target_event_id + 10_000,
                        actor_user_id,
                        PIN_AUDIT_RETAINED_BYTES,
                    ))
                    .expect("seed audit row");
            }
        }
        transaction.commit().expect("commit audit");
        assert_eq!(store.pin_row_counts().expect("full counts"), (1, 1_024));

        assert!(matches!(
            apply(
                &store,
                room_id,
                actor_user_id,
                targets[1],
                PinAction::Pin,
                11
            ),
            PinMutationResult::Changed(_)
        ));
        assert_eq!(
            store.pin_row_counts().expect("replacement counts"),
            (2, 1_024)
        );
        assert_eq!(
            store
                .connection
                .query_row(
                    "SELECT 1 FROM room_pin_events WHERE pin_event_id = ?1",
                    [first_pin_event_id],
                    |row| row.get::<_, i64>(0)
                )
                .optional()
                .expect("first audit lookup"),
            Some(1)
        );
    }

    #[test]
    fn snapshot_rejects_noncanonical_and_overlarge_target_sets() {
        let (store, room_id, _, _) = setup(0);
        assert!(store.pin_snapshot(room_id, &[2, 1]).is_err());
        assert!(store.pin_snapshot(room_id, &[1, 1]).is_err());
        assert!(store.pin_snapshot(room_id, &[0]).is_err());
        assert!(store
            .pin_snapshot(
                room_id,
                &(1..=(ROOM_PIN_SNAPSHOT_MAX_TARGETS as u64 + 1)).collect::<Vec<_>>()
            )
            .is_err());
    }
}
