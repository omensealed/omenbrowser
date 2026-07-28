use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use crate::error::{ServerError, ServerResult};
use crate::protocol::{RoomId, UserId, ROOM_SLOW_MODE_MAX_SECONDS};
use crate::store::slow_mode::{
    SLOW_MODE_ADMISSION_MAX_GLOBAL, SLOW_MODE_ADMISSION_MAX_PER_ROOM,
    SLOW_MODE_ADMISSION_PRUNE_BATCH,
};

type SlowModeKey = (RoomId, UserId);
type SlowModeDeadlines = Arc<Mutex<BTreeMap<SlowModeKey, Instant>>>;

#[derive(Debug, Default)]
pub(super) struct SlowModeOwner {
    deadlines: SlowModeDeadlines,
}

pub(super) enum SlowModeMonotonicAdmission {
    Disabled,
    Admitted(SlowModeReservation),
    Rejected { retry_after_seconds: u32 },
    Saturated,
}

pub(super) struct SlowModeReservation {
    deadlines: SlowModeDeadlines,
    key: SlowModeKey,
    prior_deadline: Option<Instant>,
    reserved_deadline: Instant,
    active: bool,
}

impl SlowModeReservation {
    pub(super) fn commit(mut self) {
        self.active = false;
    }
}

impl Drop for SlowModeReservation {
    fn drop(&mut self) {
        if !self.active {
            return;
        }
        let Ok(mut deadlines) = self.deadlines.lock() else {
            return;
        };
        if deadlines.get(&self.key) != Some(&self.reserved_deadline) {
            return;
        }
        match self.prior_deadline {
            Some(prior_deadline) => {
                deadlines.insert(self.key, prior_deadline);
            }
            None => {
                deadlines.remove(&self.key);
            }
        }
    }
}

#[derive(Clone, Copy)]
struct SlowModeOwnerLimits {
    per_room: usize,
    global: usize,
    prune_batch: usize,
}

const PRODUCTION_LIMITS: SlowModeOwnerLimits = SlowModeOwnerLimits {
    per_room: SLOW_MODE_ADMISSION_MAX_PER_ROOM,
    global: SLOW_MODE_ADMISSION_MAX_GLOBAL,
    prune_batch: SLOW_MODE_ADMISSION_PRUNE_BATCH,
};

impl SlowModeOwner {
    pub(super) fn reserve(
        &self,
        room_id: RoomId,
        user_id: UserId,
        seconds: u32,
        now: Instant,
    ) -> ServerResult<SlowModeMonotonicAdmission> {
        self.reserve_with_limits(room_id, user_id, seconds, now, PRODUCTION_LIMITS)
    }

    fn reserve_with_limits(
        &self,
        room_id: RoomId,
        user_id: UserId,
        seconds: u32,
        now: Instant,
        limits: SlowModeOwnerLimits,
    ) -> ServerResult<SlowModeMonotonicAdmission> {
        if room_id == 0 || user_id == 0 || seconds > ROOM_SLOW_MODE_MAX_SECONDS {
            return Err(ServerError::Message(
                "invalid monotonic slow-mode reservation".into(),
            ));
        }
        if seconds == 0 {
            return Ok(SlowModeMonotonicAdmission::Disabled);
        }

        let key = (room_id, user_id);
        let mut deadlines = self
            .deadlines
            .lock()
            .map_err(|_| ServerError::Message("slow-mode deadline lock poisoned".into()))?;
        if let Some(deadline) = deadlines.get(&key).copied() {
            if deadline > now {
                return Ok(SlowModeMonotonicAdmission::Rejected {
                    retry_after_seconds: retry_after_seconds(deadline, now),
                });
            }
            deadlines.remove(&key);
        }

        let expired = deadlines
            .iter()
            .filter_map(|(key, deadline)| (*deadline <= now).then_some(*key))
            .take(limits.prune_batch)
            .collect::<Vec<_>>();
        for expired_key in expired {
            deadlines.remove(&expired_key);
        }
        let room_items = deadlines
            .keys()
            .filter(|(entry_room_id, _)| *entry_room_id == room_id)
            .count();
        if room_items >= limits.per_room || deadlines.len() >= limits.global {
            return Ok(SlowModeMonotonicAdmission::Saturated);
        }

        let reserved_deadline = now
            .checked_add(Duration::from_secs(u64::from(seconds)))
            .ok_or_else(|| ServerError::Message("monotonic slow-mode deadline overflow".into()))?;
        let prior_deadline = deadlines.insert(key, reserved_deadline);
        drop(deadlines);
        Ok(SlowModeMonotonicAdmission::Admitted(SlowModeReservation {
            deadlines: Arc::clone(&self.deadlines),
            key,
            prior_deadline,
            reserved_deadline,
            active: true,
        }))
    }

    #[cfg(test)]
    pub(super) fn retained_items(&self) -> usize {
        self.deadlines
            .lock()
            .map(|deadlines| deadlines.len())
            .unwrap_or(usize::MAX)
    }
}

fn retry_after_seconds(deadline: Instant, now: Instant) -> u32 {
    let remaining = deadline.saturating_duration_since(now);
    let rounded = remaining
        .as_secs()
        .saturating_add(u64::from(remaining.subsec_nanos() != 0));
    u32::try_from(rounded)
        .unwrap_or(ROOM_SLOW_MODE_MAX_SECONDS)
        .clamp(1, ROOM_SLOW_MODE_MAX_SECONDS)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reservation_serializes_one_key_and_drop_restores_capacity() {
        let owner = SlowModeOwner::default();
        let now = Instant::now();
        let first = match owner.reserve(1, 1, 30, now).expect("first reservation") {
            SlowModeMonotonicAdmission::Admitted(reservation) => reservation,
            _ => panic!("first reservation was not admitted"),
        };
        assert!(matches!(
            owner.reserve(1, 1, 30, now).expect("competing reservation"),
            SlowModeMonotonicAdmission::Rejected {
                retry_after_seconds: 30
            }
        ));
        drop(first);
        assert_eq!(owner.retained_items(), 0);
        assert!(matches!(
            owner
                .reserve(1, 1, 30, now)
                .expect("reservation after rollback"),
            SlowModeMonotonicAdmission::Admitted(_)
        ));
    }

    #[test]
    fn committed_deadline_survives_backward_monotonic_observation() {
        let owner = SlowModeOwner::default();
        let now = Instant::now();
        let reservation = match owner.reserve(1, 1, 30, now).expect("reservation") {
            SlowModeMonotonicAdmission::Admitted(reservation) => reservation,
            _ => panic!("reservation was not admitted"),
        };
        reservation.commit();
        assert!(matches!(
            owner
                .reserve(
                    1,
                    1,
                    30,
                    now.checked_sub(Duration::from_secs(5)).unwrap_or(now)
                )
                .expect("backward observation"),
            SlowModeMonotonicAdmission::Rejected {
                retry_after_seconds: 35
            }
        ));
    }

    #[test]
    fn pruning_is_bounded_and_active_capacity_fails_closed() {
        let owner = SlowModeOwner::default();
        let now = Instant::now();
        let limits = SlowModeOwnerLimits {
            per_room: 2,
            global: 2,
            prune_batch: 1,
        };
        for user_id in 1..=2 {
            let reservation = match owner
                .reserve_with_limits(1, user_id, 1, now, limits)
                .expect("fixture reservation")
            {
                SlowModeMonotonicAdmission::Admitted(reservation) => reservation,
                _ => panic!("fixture reservation was not admitted"),
            };
            reservation.commit();
        }
        assert!(matches!(
            owner
                .reserve_with_limits(1, 3, 1, now, limits)
                .expect("saturated reservation"),
            SlowModeMonotonicAdmission::Saturated
        ));
        let admitted = match owner
            .reserve_with_limits(1, 3, 1, now + Duration::from_secs(1), limits)
            .expect("bounded prune admission")
        {
            SlowModeMonotonicAdmission::Admitted(reservation) => reservation,
            _ => panic!("reservation after bounded prune was not admitted"),
        };
        admitted.commit();
        assert_eq!(owner.retained_items(), 2);
    }
}
