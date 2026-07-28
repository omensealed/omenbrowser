use rusqlite::OptionalExtension;

use crate::error::{ServerError, ServerResult};
use crate::protocol::{RoomId, UserId, ROOM_SLOW_MODE_MAX_SECONDS};

pub const SLOW_MODE_ADMISSION_MAX_PER_ROOM: usize = 4_096;
pub const SLOW_MODE_ADMISSION_MAX_GLOBAL: usize = 16_384;
pub const SLOW_MODE_ADMISSION_PRUNE_BATCH: usize = 64;
pub const SLOW_MODE_ADMISSION_LOGICAL_BYTES: u64 = 32;
pub const SLOW_MODE_ADMISSION_MAX_LOGICAL_BYTES: u64 =
    SLOW_MODE_ADMISSION_MAX_GLOBAL as u64 * SLOW_MODE_ADMISSION_LOGICAL_BYTES;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SlowModeAdmission {
    Disabled,
    Admitted { not_before_unix: i64, pruned: usize },
    Rejected { retry_after_seconds: u32 },
    RoomNotFound,
    Saturated,
}

#[derive(Clone, Copy)]
struct SlowModeLimits {
    per_room: usize,
    global: usize,
    prune_batch: usize,
}

const PRODUCTION_LIMITS: SlowModeLimits = SlowModeLimits {
    per_room: SLOW_MODE_ADMISSION_MAX_PER_ROOM,
    global: SLOW_MODE_ADMISSION_MAX_GLOBAL,
    prune_batch: SLOW_MODE_ADMISSION_PRUNE_BATCH,
};

pub fn admit_room_publication(
    transaction: &rusqlite::Transaction<'_>,
    room_id: RoomId,
    user_id: UserId,
    now_unix: i64,
) -> ServerResult<SlowModeAdmission> {
    admit_room_publication_with_limits(transaction, room_id, user_id, now_unix, PRODUCTION_LIMITS)
}

fn admit_room_publication_with_limits(
    transaction: &rusqlite::Transaction<'_>,
    room_id: RoomId,
    user_id: UserId,
    now_unix: i64,
    limits: SlowModeLimits,
) -> ServerResult<SlowModeAdmission> {
    if room_id == 0 || user_id == 0 || now_unix < 0 {
        return Err(ServerError::Message(
            "slow-mode admission requires nonzero room/user ids and nonnegative time".into(),
        ));
    }
    let slow_mode_seconds = transaction
        .query_row(
            "SELECT slow_mode_seconds
             FROM rooms
             WHERE room_id = ?1 AND archived = 0",
            [room_id],
            |row| row.get::<_, i64>(0),
        )
        .optional()?;
    let Some(slow_mode_seconds) = slow_mode_seconds else {
        return Ok(SlowModeAdmission::RoomNotFound);
    };
    let slow_mode_seconds = u32::try_from(slow_mode_seconds)
        .map_err(|_| ServerError::Message("stored room slow-mode interval is invalid".into()))?;
    if slow_mode_seconds > ROOM_SLOW_MODE_MAX_SECONDS {
        return Err(ServerError::Message(
            "stored room slow-mode interval exceeds the protocol bound".into(),
        ));
    }
    if slow_mode_seconds == 0 {
        return Ok(SlowModeAdmission::Disabled);
    }

    let existing_deadline = transaction
        .query_row(
            "SELECT not_before_unix
             FROM room_slow_mode_admissions
             WHERE room_id = ?1 AND user_id = ?2",
            (room_id, user_id),
            |row| row.get::<_, i64>(0),
        )
        .optional()?;
    if let Some(not_before_unix) = existing_deadline.filter(|deadline| *deadline > now_unix) {
        let remaining = not_before_unix.saturating_sub(now_unix);
        return Ok(SlowModeAdmission::Rejected {
            retry_after_seconds: u32::try_from(remaining)
                .unwrap_or(ROOM_SLOW_MODE_MAX_SECONDS)
                .min(ROOM_SLOW_MODE_MAX_SECONDS),
        });
    }

    let pruned = transaction.execute(
        "DELETE FROM room_slow_mode_admissions
         WHERE rowid IN (
           SELECT rowid
           FROM room_slow_mode_admissions
           WHERE not_before_unix <= ?1
           ORDER BY not_before_unix, room_id, user_id
           LIMIT ?2
         )",
        (now_unix, limits.prune_batch as i64),
    )?;
    let existing_after_prune: bool = transaction.query_row(
        "SELECT EXISTS(
           SELECT 1 FROM room_slow_mode_admissions
           WHERE room_id = ?1 AND user_id = ?2
         )",
        (room_id, user_id),
        |row| row.get(0),
    )?;
    if !existing_after_prune {
        let room_count: i64 = transaction.query_row(
            "SELECT COUNT(*) FROM room_slow_mode_admissions WHERE room_id = ?1",
            [room_id],
            |row| row.get(0),
        )?;
        let global_count: i64 = transaction.query_row(
            "SELECT COUNT(*) FROM room_slow_mode_admissions",
            [],
            |row| row.get(0),
        )?;
        if usize::try_from(room_count).unwrap_or(usize::MAX) >= limits.per_room
            || usize::try_from(global_count).unwrap_or(usize::MAX) >= limits.global
        {
            return Ok(SlowModeAdmission::Saturated);
        }
    }

    let not_before_unix = now_unix
        .checked_add(i64::from(slow_mode_seconds))
        .ok_or_else(|| ServerError::Message("slow-mode deadline overflow".into()))?;
    transaction.execute(
        "INSERT INTO room_slow_mode_admissions(
           room_id, user_id, not_before_unix, updated_at
         ) VALUES (?1, ?2, ?3, ?4)
         ON CONFLICT(room_id, user_id) DO UPDATE SET
           not_before_unix = excluded.not_before_unix,
           updated_at = excluded.updated_at",
        (room_id, user_id, not_before_unix, now_unix),
    )?;
    Ok(SlowModeAdmission::Admitted {
        not_before_unix,
        pruned,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::OmenchatStore;

    fn configured_store(seconds: u32) -> (OmenchatStore, RoomId) {
        let store = OmenchatStore::in_memory().expect("store");
        let room = store.ensure_room("slow-room", None).expect("room");
        store
            .connection
            .execute(
                "UPDATE rooms SET slow_mode_seconds = ?1 WHERE room_id = ?2",
                (seconds, room.room_id),
            )
            .expect("slow-mode fixture");
        (store, room.room_id)
    }

    fn admit(
        store: &OmenchatStore,
        room_id: RoomId,
        user_id: UserId,
        now_unix: i64,
    ) -> SlowModeAdmission {
        let transaction = store
            .connection
            .unchecked_transaction()
            .expect("transaction");
        let result =
            admit_room_publication(&transaction, room_id, user_id, now_unix).expect("admission");
        transaction.commit().expect("commit admission");
        result
    }

    #[test]
    fn disabled_room_retains_no_admission_state() {
        let (store, room_id) = configured_store(0);
        assert_eq!(admit(&store, room_id, 7, 100), SlowModeAdmission::Disabled);
        let rows: i64 = store
            .connection
            .query_row(
                "SELECT COUNT(*) FROM room_slow_mode_admissions",
                [],
                |row| row.get(0),
            )
            .expect("admission rows");
        assert_eq!(rows, 0);
    }

    #[test]
    fn admission_rejects_until_deadline_and_rollback_consumes_nothing() {
        let (store, room_id) = configured_store(30);
        assert_eq!(
            admit(&store, room_id, 7, 100),
            SlowModeAdmission::Admitted {
                not_before_unix: 130,
                pruned: 0,
            }
        );
        assert_eq!(
            admit(&store, room_id, 7, 110),
            SlowModeAdmission::Rejected {
                retry_after_seconds: 20,
            }
        );

        let transaction = store
            .connection
            .unchecked_transaction()
            .expect("transaction");
        assert_eq!(
            admit_room_publication(&transaction, room_id, 7, 130).expect("admission"),
            SlowModeAdmission::Admitted {
                not_before_unix: 160,
                pruned: 1,
            }
        );
        transaction.rollback().expect("rollback");
        assert_eq!(
            admit(&store, room_id, 7, 130),
            SlowModeAdmission::Admitted {
                not_before_unix: 160,
                pruned: 1,
            }
        );
    }

    #[test]
    fn bounded_pruning_precedes_fail_closed_saturation() {
        let (store, room_id) = configured_store(30);
        let transaction = store
            .connection
            .unchecked_transaction()
            .expect("transaction");
        for user_id in 1..=3 {
            transaction
                .execute(
                    "INSERT INTO room_slow_mode_admissions(
                       room_id, user_id, not_before_unix, updated_at
                     ) VALUES (?1, ?2, 5, 1)",
                    (room_id, user_id),
                )
                .expect("expired admission");
        }
        let limits = SlowModeLimits {
            per_room: 2,
            global: 2,
            prune_batch: 2,
        };
        assert_eq!(
            admit_room_publication_with_limits(&transaction, room_id, 9, 10, limits)
                .expect("bounded admission"),
            SlowModeAdmission::Admitted {
                not_before_unix: 40,
                pruned: 2,
            }
        );
        transaction
            .execute(
                "UPDATE room_slow_mode_admissions
                 SET not_before_unix = 100, updated_at = 10
                 WHERE room_id = ?1 AND user_id = 3",
                [room_id],
            )
            .expect("make remaining admission active");
        assert_eq!(
            admit_room_publication_with_limits(&transaction, room_id, 10, 10, limits)
                .expect("saturated admission"),
            SlowModeAdmission::Saturated
        );
        transaction.rollback().expect("rollback");
    }
}
