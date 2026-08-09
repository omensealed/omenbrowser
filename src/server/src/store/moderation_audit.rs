use rusqlite::OptionalExtension;

use super::{current_unix_seconds, OmenchatStore, ServerUser};
use crate::error::{ServerError, ServerResult};
use crate::protocol::{
    EventId, ModerationAuditAction, ModerationAuditPage, ModerationAuditRecord, RoomId,
    MODERATION_AUDIT_DISPLAY_NAME_MAX_BYTES, MODERATION_AUDIT_PAGE_MAX_ENTRIES,
};

pub const MAX_MODERATION_AUDIT_ROWS_PER_ROOM: i64 = 2_048;
pub const MAX_MODERATION_AUDIT_ROWS_GLOBAL: i64 = 8_192;
pub const MAX_MODERATION_AUDIT_BYTES_GLOBAL: i64 = 4 * 1024 * 1024;
pub const MODERATION_AUDIT_RETENTION_AGE_SECONDS: i64 = 365 * 24 * 60 * 60;
pub const MAX_MODERATION_AUDIT_PRUNED_PER_MUTATION: usize = 64;

const MODERATION_AUDIT_RETAINED_ROW_OVERHEAD_BYTES: i64 = 64;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ModerationAuditAdmission {
    Stored(EventId),
    Saturated,
}

impl OmenchatStore {
    pub(crate) fn append_durable_moderation_audit(
        transaction: &rusqlite::Transaction<'_>,
        room_id: RoomId,
        actor: &ServerUser,
        target: &ServerUser,
        action: ModerationAuditAction,
        result_role_bits: Option<u64>,
        result_status_bits: Option<u32>,
    ) -> ServerResult<ModerationAuditAdmission> {
        append_moderation_audit_at(
            transaction,
            room_id,
            actor,
            target,
            action,
            result_role_bits,
            result_status_bits,
            current_unix_seconds(),
        )
    }

    pub fn moderation_audit_page(
        &self,
        room_id: RoomId,
        before_audit_id: Option<EventId>,
        limit: u16,
    ) -> ServerResult<ModerationAuditPage> {
        if room_id == 0 {
            return Err(ServerError::Message(
                "moderation audit room id must be nonzero".into(),
            ));
        }
        let limit = usize::from(limit);
        if limit == 0 || limit > MODERATION_AUDIT_PAGE_MAX_ENTRIES {
            return Err(ServerError::Message(format!(
                "moderation audit page limit must be between 1 and {MODERATION_AUDIT_PAGE_MAX_ENTRIES}"
            )));
        }
        if before_audit_id == Some(0) {
            return Err(ServerError::Message(
                "moderation audit cursor must be nonzero".into(),
            ));
        }
        let cursor = before_audit_id
            .map(sqlite_id)
            .transpose()?
            .unwrap_or(i64::MAX);
        let mut statement = self.connection.prepare(
            "SELECT audit_id, room_id, actor_user_id, actor_display_name,
                    target_user_id, target_display_name, action_kind,
                    committed_at, result_role_bits, result_status_bits
             FROM moderation_audit_events
             WHERE room_id = ?1 AND audit_id < ?2
             ORDER BY audit_id DESC
             LIMIT ?3",
        )?;
        let records = statement
            .query_map((room_id, cursor, limit as i64), record_from_row)?
            .collect::<Result<Vec<_>, _>>()?;
        let page = ModerationAuditPage { records };
        page.clone().into_frame_values().map_err(|error| {
            ServerError::Message(format!("stored moderation audit page is invalid: {error}"))
        })?;
        page.validate_room(room_id).map_err(|error| {
            ServerError::Message(format!("stored moderation audit room mismatch: {error}"))
        })?;
        Ok(page)
    }

    #[cfg(test)]
    pub(crate) fn moderation_audit_row_count(&self) -> ServerResult<i64> {
        self.connection
            .query_row("SELECT COUNT(*) FROM moderation_audit_events", [], |row| {
                row.get(0)
            })
            .map_err(Into::into)
    }
}

#[allow(clippy::too_many_arguments)]
fn append_moderation_audit_at(
    transaction: &rusqlite::Transaction<'_>,
    room_id: RoomId,
    actor: &ServerUser,
    target: &ServerUser,
    action: ModerationAuditAction,
    result_role_bits: Option<u64>,
    result_status_bits: Option<u32>,
    now: i64,
) -> ServerResult<ModerationAuditAdmission> {
    let retained_bytes = retained_bytes(&actor.display_name, &target.display_name)?;
    validate_record_shape(
        room_id,
        actor,
        target,
        action,
        result_role_bits,
        result_status_bits,
        now,
    )?;

    let mut pruned = prune_expired(transaction, now)?;
    while !has_capacity(transaction, room_id, retained_bytes)? {
        if pruned >= MAX_MODERATION_AUDIT_PRUNED_PER_MUTATION {
            return Ok(ModerationAuditAdmission::Saturated);
        }
        let deleted = delete_oldest_for_capacity(transaction, room_id)?;
        if deleted == 0 {
            return Ok(ModerationAuditAdmission::Saturated);
        }
        pruned += deleted;
    }

    transaction.execute(
        "INSERT INTO moderation_audit_events(
           room_id, actor_user_id, actor_display_name,
           target_user_id, target_display_name, action_kind,
           result_role_bits, result_status_bits, committed_at, retained_bytes
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
        (
            room_id,
            actor.user_id,
            &actor.display_name,
            target.user_id,
            &target.display_name,
            action as u8,
            result_role_bits.map(sqlite_u64).transpose()?,
            result_status_bits,
            now,
            retained_bytes,
        ),
    )?;
    let audit_id = transaction.last_insert_rowid();
    if audit_id <= 0 {
        return Err(ServerError::Message(
            "moderation audit identifier space is exhausted".into(),
        ));
    }
    Ok(ModerationAuditAdmission::Stored(
        u64::try_from(audit_id)
            .map_err(|_| ServerError::Message("stored moderation audit id is invalid".into()))?,
    ))
}

#[allow(clippy::too_many_arguments)]
fn validate_record_shape(
    room_id: RoomId,
    actor: &ServerUser,
    target: &ServerUser,
    action: ModerationAuditAction,
    result_role_bits: Option<u64>,
    result_status_bits: Option<u32>,
    now: i64,
) -> ServerResult<()> {
    ModerationAuditRecord {
        audit_id: 1,
        room_id,
        actor_user_id: actor.user_id,
        actor_display_name_at_action: actor.display_name.clone(),
        target_user_id: Some(target.user_id),
        target_display_name_at_action: Some(target.display_name.clone()),
        action,
        committed_at_unix: now,
        result_role_bits,
        result_status_bits,
    }
    .into_frame_value()
    .map(|_| ())
    .map_err(|error| ServerError::Message(format!("invalid moderation audit record: {error}")))
}

fn retained_bytes(actor_display_name: &str, target_display_name: &str) -> ServerResult<i64> {
    if actor_display_name.is_empty()
        || target_display_name.is_empty()
        || actor_display_name.len() > MODERATION_AUDIT_DISPLAY_NAME_MAX_BYTES
        || target_display_name.len() > MODERATION_AUDIT_DISPLAY_NAME_MAX_BYTES
    {
        return Err(ServerError::Message(
            "moderation audit display name is outside protocol bounds".into(),
        ));
    }
    MODERATION_AUDIT_RETAINED_ROW_OVERHEAD_BYTES
        .checked_add(actor_display_name.len() as i64)
        .and_then(|bytes| bytes.checked_add(target_display_name.len() as i64))
        .ok_or_else(|| ServerError::Message("moderation audit byte accounting overflow".into()))
}

fn prune_expired(transaction: &rusqlite::Transaction<'_>, now: i64) -> ServerResult<usize> {
    let cutoff = now.saturating_sub(MODERATION_AUDIT_RETENTION_AGE_SECONDS);
    transaction
        .execute(
            "DELETE FROM moderation_audit_events
             WHERE audit_id IN (
               SELECT audit_id FROM moderation_audit_events
               WHERE committed_at < ?1
               ORDER BY committed_at, audit_id
               LIMIT ?2
             )",
            (cutoff, MAX_MODERATION_AUDIT_PRUNED_PER_MUTATION as i64),
        )
        .map_err(Into::into)
}

fn has_capacity(
    transaction: &rusqlite::Transaction<'_>,
    room_id: RoomId,
    retained_bytes: i64,
) -> ServerResult<bool> {
    let room_rows: i64 = transaction.query_row(
        "SELECT COUNT(*) FROM moderation_audit_events WHERE room_id = ?1",
        [room_id],
        |row| row.get(0),
    )?;
    let (global_rows, global_bytes): (i64, i64) = transaction.query_row(
        "SELECT COUNT(*), COALESCE(SUM(retained_bytes), 0)
         FROM moderation_audit_events",
        [],
        |row| Ok((row.get(0)?, row.get(1)?)),
    )?;
    Ok(room_rows < MAX_MODERATION_AUDIT_ROWS_PER_ROOM
        && global_rows < MAX_MODERATION_AUDIT_ROWS_GLOBAL
        && global_bytes
            .checked_add(retained_bytes)
            .is_some_and(|bytes| bytes <= MAX_MODERATION_AUDIT_BYTES_GLOBAL))
}

fn delete_oldest_for_capacity(
    transaction: &rusqlite::Transaction<'_>,
    room_id: RoomId,
) -> ServerResult<usize> {
    let room_rows: i64 = transaction.query_row(
        "SELECT COUNT(*) FROM moderation_audit_events WHERE room_id = ?1",
        [room_id],
        |row| row.get(0),
    )?;
    let audit_id: Option<i64> = if room_rows >= MAX_MODERATION_AUDIT_ROWS_PER_ROOM {
        transaction
            .query_row(
                "SELECT audit_id FROM moderation_audit_events
                 WHERE room_id = ?1 ORDER BY audit_id LIMIT 1",
                [room_id],
                |row| row.get(0),
            )
            .optional()?
    } else {
        transaction
            .query_row(
                "SELECT audit_id FROM moderation_audit_events
                 ORDER BY audit_id LIMIT 1",
                [],
                |row| row.get(0),
            )
            .optional()?
    };
    match audit_id {
        Some(audit_id) => transaction
            .execute(
                "DELETE FROM moderation_audit_events WHERE audit_id = ?1",
                [audit_id],
            )
            .map_err(Into::into),
        None => Ok(0),
    }
}

fn record_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<ModerationAuditRecord> {
    let audit_id = row.get::<_, i64>(0)?;
    let room_id = row.get::<_, i64>(1)?;
    let actor_user_id = row.get::<_, i64>(2)?;
    let target_user_id = row.get::<_, i64>(4)?;
    let action_kind = row.get::<_, i64>(6)?;
    let result_role_bits = row.get::<_, Option<i64>>(8)?;
    let result_status_bits = row.get::<_, Option<i64>>(9)?;
    Ok(ModerationAuditRecord {
        audit_id: u64::try_from(audit_id).map_err(|error| conversion_error(0, error))?,
        room_id: u32::try_from(room_id).map_err(|error| conversion_error(1, error))?,
        actor_user_id: u32::try_from(actor_user_id).map_err(|error| conversion_error(2, error))?,
        actor_display_name_at_action: row.get(3)?,
        target_user_id: Some(
            u32::try_from(target_user_id).map_err(|error| conversion_error(4, error))?,
        ),
        target_display_name_at_action: Some(row.get(5)?),
        action: ModerationAuditAction::try_from(
            u64::try_from(action_kind).map_err(|error| conversion_error(6, error))?,
        )
        .map_err(|error| {
            rusqlite::Error::FromSqlConversionFailure(
                6,
                rusqlite::types::Type::Integer,
                Box::new(error),
            )
        })?,
        committed_at_unix: row.get(7)?,
        result_role_bits: result_role_bits
            .map(|bits| u64::try_from(bits).map_err(|error| conversion_error(8, error)))
            .transpose()?,
        result_status_bits: result_status_bits
            .map(|bits| u32::try_from(bits).map_err(|error| conversion_error(9, error)))
            .transpose()?,
    })
}

fn conversion_error(
    column: usize,
    error: impl std::error::Error + Send + Sync + 'static,
) -> rusqlite::Error {
    rusqlite::Error::FromSqlConversionFailure(
        column,
        rusqlite::types::Type::Integer,
        Box::new(error),
    )
}

fn sqlite_id(value: EventId) -> ServerResult<i64> {
    i64::try_from(value)
        .map_err(|_| ServerError::Message("moderation audit id does not fit SQLite".into()))
}

fn sqlite_u64(value: u64) -> ServerResult<i64> {
    i64::try_from(value)
        .map_err(|_| ServerError::Message("moderation audit role bits do not fit SQLite".into()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::{ClientInstanceId, MutationId, RequestHash, UserId};
    use crate::store::durable_replay::DurableMutationKey;

    fn user(user_id: UserId, name: &str) -> ServerUser {
        ServerUser {
            user_id,
            identity_hash: vec![user_id as u8; 16],
            display_name: name.into(),
            role_bits: 0,
            status_bits: 0,
            lxmf_destination: None,
            profile_revision: 0,
            nickname_colour_rgb: None,
        }
    }

    #[test]
    fn append_and_page_are_bounded_newest_first_and_cursor_exclusive() {
        let store = OmenchatStore::in_memory().expect("store");
        let actor = user(2, "Moderator");
        let target = user(3, "Target");
        let transaction = rusqlite::Transaction::new_unchecked(
            &store.connection,
            rusqlite::TransactionBehavior::Immediate,
        )
        .expect("transaction");
        for (action, role, status) in [
            (ModerationAuditAction::Kick, None, None),
            (ModerationAuditAction::Ban, None, Some(1)),
            (ModerationAuditAction::RoleChange, Some(0), None),
        ] {
            assert!(matches!(
                append_moderation_audit_at(
                    &transaction,
                    1,
                    &actor,
                    &target,
                    action,
                    role,
                    status,
                    1_700_000_000,
                )
                .expect("append"),
                ModerationAuditAdmission::Stored(_)
            ));
        }
        transaction.commit().expect("commit");

        let newest = store.moderation_audit_page(1, None, 2).expect("newest");
        assert_eq!(newest.records.len(), 2);
        assert!(newest.records[0].audit_id > newest.records[1].audit_id);
        assert_eq!(
            newest.records[0].result_role_bits,
            Some(0),
            "standard role result must remain representable"
        );
        let older = store
            .moderation_audit_page(1, Some(newest.records[1].audit_id), 2)
            .expect("older");
        assert_eq!(older.records.len(), 1);
    }

    #[test]
    fn invalid_shape_and_transaction_rollback_leave_no_row() {
        let store = OmenchatStore::in_memory().expect("store");
        let actor = user(2, "Moderator");
        let target = user(3, "Target");
        let transaction = rusqlite::Transaction::new_unchecked(
            &store.connection,
            rusqlite::TransactionBehavior::Immediate,
        )
        .expect("transaction");
        let error = append_moderation_audit_at(
            &transaction,
            1,
            &actor,
            &target,
            ModerationAuditAction::Ban,
            None,
            Some(0),
            1_700_000_000,
        )
        .expect_err("ban result must be banned")
        .to_string();
        assert!(error.contains("invalid moderation audit record"));
        drop(transaction);
        assert_eq!(store.moderation_audit_row_count().expect("count"), 0);
    }

    #[test]
    fn mutation_audit_and_replay_result_roll_back_together_on_fault() {
        let store = OmenchatStore::in_memory().expect("store");
        let actor = store
            .ensure_user(b"actor", "Moderator", None)
            .expect("actor");
        let target = store
            .ensure_user(b"target", "Target", None)
            .expect("target");
        let key = DurableMutationKey {
            identity_hash: b"actor",
            client_instance_id: ClientInstanceId::new([3; 16]),
            mutation_id: MutationId::new([4; 16]),
        };
        let error = store
            .commit_durable_mutation_result(key, RequestHash::new([5; 32]), |transaction| {
                let changed = OmenchatStore::set_durable_user_status_flag(
                    transaction,
                    target.user_id,
                    1,
                    true,
                )?;
                assert!(matches!(
                    OmenchatStore::append_durable_moderation_audit(
                        transaction,
                        1,
                        &actor,
                        &changed,
                        ModerationAuditAction::Ban,
                        None,
                        Some(changed.status_bits),
                    )?,
                    ModerationAuditAdmission::Stored(_)
                ));
                Err(ServerError::Message("injected post-audit fault".into()))
            })
            .expect_err("fault must roll back transaction")
            .to_string();
        assert!(error.contains("injected post-audit fault"));
        assert_eq!(
            store
                .user_by_identity(b"target")
                .expect("target query")
                .expect("target")
                .status_bits,
            0
        );
        assert_eq!(store.moderation_audit_row_count().expect("audit count"), 0);
        assert_eq!(
            store
                .connection
                .query_row("SELECT COUNT(*) FROM durable_mutation_results", [], |row| {
                    row.get::<_, i64>(0)
                })
                .expect("replay count"),
            0
        );
    }

    #[test]
    fn expired_pruning_is_incremental_and_autoincrement_ids_do_not_reuse() {
        let store = OmenchatStore::in_memory().expect("store");
        store
            .connection
            .execute_batch(
                "INSERT INTO moderation_audit_events(
                   room_id, actor_user_id, actor_display_name,
                   target_user_id, target_display_name, action_kind,
                   result_role_bits, result_status_bits, committed_at, retained_bytes
                 ) VALUES (1, 2, 'Moderator', 3, 'Target', 1, NULL, NULL, 1, 79);",
            )
            .expect("old row");
        let first_id = store.connection.last_insert_rowid();
        let actor = user(2, "Moderator");
        let target = user(3, "Target");
        let transaction = rusqlite::Transaction::new_unchecked(
            &store.connection,
            rusqlite::TransactionBehavior::Immediate,
        )
        .expect("transaction");
        let inserted = append_moderation_audit_at(
            &transaction,
            1,
            &actor,
            &target,
            ModerationAuditAction::Kick,
            None,
            None,
            MODERATION_AUDIT_RETENTION_AGE_SECONDS + 2,
        )
        .expect("append");
        transaction.commit().expect("commit");
        let ModerationAuditAdmission::Stored(inserted_id) = inserted else {
            panic!("audit unexpectedly saturated");
        };
        assert!(inserted_id > first_id as u64);
        assert_eq!(store.moderation_audit_row_count().expect("count"), 1);
    }

    #[test]
    fn full_room_replaces_only_the_oldest_row_and_stays_bounded() {
        let store = OmenchatStore::in_memory().expect("store");
        store
            .connection
            .execute_batch(
                "WITH RECURSIVE ids(value) AS (
                   SELECT 1
                   UNION ALL
                   SELECT value + 1 FROM ids WHERE value < 2048
                 )
                 INSERT INTO moderation_audit_events(
                   room_id, actor_user_id, actor_display_name,
                   target_user_id, target_display_name, action_kind,
                   result_role_bits, result_status_bits, committed_at, retained_bytes
                 )
                 SELECT 1, 2, 'Moderator', 3, 'Target', 1,
                        NULL, NULL, 1700000000, 79
                 FROM ids;",
            )
            .expect("full room fixture");
        let actor = user(2, "Moderator");
        let target = user(3, "Target");
        let transaction = rusqlite::Transaction::new_unchecked(
            &store.connection,
            rusqlite::TransactionBehavior::Immediate,
        )
        .expect("transaction");
        let inserted = append_moderation_audit_at(
            &transaction,
            1,
            &actor,
            &target,
            ModerationAuditAction::Kick,
            None,
            None,
            1_700_000_001,
        )
        .expect("bounded replacement");
        transaction.commit().expect("commit");
        assert!(matches!(inserted, ModerationAuditAdmission::Stored(_)));
        assert_eq!(
            store.moderation_audit_row_count().expect("bounded count"),
            MAX_MODERATION_AUDIT_ROWS_PER_ROOM
        );
        assert_eq!(
            store
                .connection
                .query_row(
                    "SELECT MIN(audit_id) FROM moderation_audit_events",
                    [],
                    |row| row.get::<_, i64>(0)
                )
                .expect("oldest retained id"),
            2,
            "one admission may replace only the one oldest capacity row"
        );
    }

    #[test]
    #[ignore = "explicit isolated moderation-audit retention measurement"]
    fn moderation_audit_retention_measurement() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        use std::time::Instant;

        static DATABASE_NONCE: AtomicUsize = AtomicUsize::new(0);
        let items = std::env::var("OMEN_MODERATION_AUDIT_MEASUREMENT_ITEMS")
            .ok()
            .and_then(|value| value.parse::<usize>().ok())
            .unwrap_or(MAX_MODERATION_AUDIT_ROWS_PER_ROOM as usize);
        assert!((1..=MAX_MODERATION_AUDIT_ROWS_PER_ROOM as usize).contains(&items));
        let nonce = DATABASE_NONCE.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "omenchat-moderation-audit-measurement-{}-{nonce}.sqlite",
            std::process::id()
        ));
        let store = OmenchatStore::open(&path).expect("measurement store");
        let actor = user(2, "Moderator");
        let target = user(3, "Target");
        let mut append_micros = Vec::with_capacity(items);

        for index in 0..items {
            let started = Instant::now();
            let transaction = rusqlite::Transaction::new_unchecked(
                &store.connection,
                rusqlite::TransactionBehavior::Immediate,
            )
            .expect("measurement transaction");
            assert!(matches!(
                append_moderation_audit_at(
                    &transaction,
                    1,
                    &actor,
                    &target,
                    ModerationAuditAction::Kick,
                    None,
                    None,
                    1_700_000_000 + index as i64,
                )
                .expect("measurement append"),
                ModerationAuditAdmission::Stored(_)
            ));
            transaction.commit().expect("measurement commit");
            append_micros.push(started.elapsed().as_micros());
        }

        let mut page_micros = Vec::new();
        let mut page_count = 0_usize;
        let mut cursor = None;
        loop {
            let started = Instant::now();
            let page = store
                .moderation_audit_page(1, cursor, MODERATION_AUDIT_PAGE_MAX_ENTRIES as u16)
                .expect("measurement page");
            page_micros.push(started.elapsed().as_micros());
            if page.records.is_empty() {
                break;
            }
            page_count = page_count.saturating_add(page.records.len());
            cursor = page.records.last().map(|record| record.audit_id);
            if page.records.len() < MODERATION_AUDIT_PAGE_MAX_ENTRIES {
                break;
            }
        }

        let rows = store
            .moderation_audit_row_count()
            .expect("measurement row count");
        let retained_bytes: i64 = store
            .connection
            .query_row(
                "SELECT COALESCE(SUM(retained_bytes), 0)
                 FROM moderation_audit_events",
                [],
                |row| row.get(0),
            )
            .expect("measurement retained bytes");
        store
            .connection
            .execute_batch("PRAGMA wal_checkpoint(TRUNCATE);")
            .expect("checkpoint measurement database");
        let database_bytes = [
            path.clone(),
            path.with_extension("sqlite-wal"),
            path.with_extension("sqlite-shm"),
        ]
        .into_iter()
        .filter_map(|candidate| std::fs::metadata(candidate).ok())
        .map(|metadata| metadata.len())
        .sum::<u64>();

        let percentile = |samples: &mut Vec<u128>, percent: usize| {
            samples.sort_unstable();
            let index = samples
                .len()
                .saturating_mul(percent)
                .saturating_add(99)
                .checked_div(100)
                .unwrap_or(0)
                .saturating_sub(1)
                .min(samples.len().saturating_sub(1));
            samples[index]
        };
        let append_max = append_micros.iter().copied().max().unwrap_or(0);
        let page_max = page_micros.iter().copied().max().unwrap_or(0);
        let append_p50 = percentile(&mut append_micros.clone(), 50);
        let append_p95 = percentile(&mut append_micros, 95);
        let page_p50 = percentile(&mut page_micros.clone(), 50);
        let page_p95 = percentile(&mut page_micros, 95);

        assert_eq!(rows, items as i64);
        assert_eq!(page_count, items);
        assert!(retained_bytes <= MAX_MODERATION_AUDIT_BYTES_GLOBAL);
        println!(
            "MODERATION_AUDIT_RETENTION_MEASUREMENT items={items} rows={rows} retained_bytes={retained_bytes} database_bytes={database_bytes} pages={} append_p50_us={append_p50} append_p95_us={append_p95} append_max_us={append_max} page_p50_us={page_p50} page_p95_us={page_p95} page_max_us={page_max}",
            page_micros.len()
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
