//! `Throttle` (README "Throttle"): cooldown, in-flight, consecutive-failure breaker, rate-limit
//! backoff mirror and halted mirror, released per action class. `halted` only blocks opens;
//! Flatten and hard actions bypass cooldown and in-flight.

use hft_strategy_sdk::{RestoreError, SnapshotReader, SnapshotWriter};

use crate::units::{deadline, ms_to_ns};

/// Action class.
#[repr(u8)]
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub enum FireKind {
    Open = 0,
    /// Regular reduction.
    Close = 1,
    Flatten = 2,
    /// Hard action.
    Hard = 3,
}

/// Why `can_fire` refused.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub enum ThrottleReason {
    InFlight,
    Cooldown,
    Halted,
    Backoff,
    HardRearm,
}

/// How an open/close flow ended.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub enum Outcome {
    Filled,
    NoFill,
    /// Execution failure (exchange reject, external execution failure).
    Failed,
    /// Both legs sent together and reversed.
    Rollback,
    /// Gate reject (`RejectBits` gate bits such as KILLED, CLOSE_ONLY, VENUE_NOT_READY, RATE,
    /// LINK_STALE, LINK_FUNDS).
    Gated,
}

/// What the strategy should do after `on_outcome`.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub enum OutcomeAction {
    None,
    RequestHalt,
    EnterFallback,
}

/// Throttle parameters; `max_consec_fail = 0` = no breaker.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Hash)]
pub struct ThrottleParams {
    pub cooldown_ms: u32,
    pub max_consec_fail: u8,
    pub fallback_ms: u32,
    pub hard_rearm_ms: u32,
}

/// [`ThrottleParams`] with the three durations in ns.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Hash)]
pub struct ThrottleParamsNs {
    pub cooldown_ns: i64,
    pub max_consec_fail: u8,
    pub fallback_ns: i64,
    pub hard_rearm_ns: i64,
}

impl ThrottleParams {
    #[inline]
    pub const fn ns(&self) -> ThrottleParamsNs {
        ThrottleParamsNs {
            cooldown_ns: ms_to_ns(self.cooldown_ms),
            max_consec_fail: self.max_consec_fail,
            fallback_ns: ms_to_ns(self.fallback_ms),
            hard_rearm_ns: ms_to_ns(self.hard_rearm_ms),
        }
    }
}

/// Throttle state; every deadline is engine_ns. `Default` = all deadlines 0, flags false, count 0.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Hash)]
pub struct Throttle {
    cooldown_until_ens: i64,
    in_flight: bool,
    consec_fail: u8,
    fallback_until_ens: i64,
    backoff_until_ens: i64,
    halted: bool,
    hard_rearm_until_ens: i64,
}

impl Throttle {
    /// Release table (README): Open ← in-flight, cooldown, halted, backoff; Close ← in-flight,
    /// cooldown, backoff; Flatten/Hard ← hard re-arm only. First hit in that column order;
    /// deadlines hit while `now_ens < deadline`. The fallback period is not checked here.
    #[inline]
    pub fn can_fire(&self, k: FireKind, now_ens: i64) -> Result<(), ThrottleReason> {
        match k {
            FireKind::Open | FireKind::Close => {
                if self.in_flight {
                    return Err(ThrottleReason::InFlight);
                }
                if now_ens < self.cooldown_until_ens {
                    return Err(ThrottleReason::Cooldown);
                }
                if k == FireKind::Open && self.halted {
                    return Err(ThrottleReason::Halted);
                }
                if now_ens < self.backoff_until_ens {
                    return Err(ThrottleReason::Backoff);
                }
                Ok(())
            }
            FireKind::Flatten | FireKind::Hard => {
                if now_ens < self.hard_rearm_until_ens {
                    return Err(ThrottleReason::HardRearm);
                }
                Ok(())
            }
        }
    }

    /// Open/Close: in flight and cooldown from `now_ens`; Flatten/Hard: only the re-arm deadline.
    #[inline]
    pub fn on_fire(&mut self, k: FireKind, now_ens: i64, p: &ThrottleParamsNs) {
        match k {
            FireKind::Open | FireKind::Close => {
                self.in_flight = true;
                self.cooldown_until_ens = deadline(now_ens, p.cooldown_ns);
            }
            FireKind::Flatten | FireKind::Hard => {
                self.hard_rearm_until_ens = deadline(now_ens, p.hard_rearm_ns);
            }
        }
    }

    /// End of an open/close flow: clears in-flight; `Failed` counts (saturating at 255) and asks
    /// for Halted at `max_consec_fail`; `Filled`/`NoFill` reset the count; `Gated` neither counts
    /// nor resets; `Rollback` starts the fallback period.
    pub fn on_outcome(&mut self, o: Outcome, now_ens: i64, p: &ThrottleParamsNs) -> OutcomeAction {
        self.in_flight = false;
        match o {
            Outcome::Failed => {
                self.consec_fail = self.consec_fail.saturating_add(1);
                if p.max_consec_fail > 0 && self.consec_fail >= p.max_consec_fail {
                    OutcomeAction::RequestHalt
                } else {
                    OutcomeAction::None
                }
            }
            Outcome::Filled | Outcome::NoFill => {
                self.consec_fail = 0;
                OutcomeAction::None
            }
            Outcome::Gated => OutcomeAction::None,
            Outcome::Rollback => {
                self.fallback_until_ens = deadline(now_ens, p.fallback_ns);
                OutcomeAction::EnterFallback
            }
        }
    }

    /// Mirror of the engine Halted cap (SDK Risk `CapSet`/`CapCleared`).
    #[inline]
    pub fn set_halted(&mut self, on: bool) {
        self.halted = on;
    }

    /// Mirror of the rate-limit backoff (overwrites the deadline). The SDK exposes only the
    /// boolean `InstRiskView.rate_backoff`: true → `set_backoff(i64::MAX)`, false → `set_backoff(0)`.
    #[inline]
    pub fn set_backoff(&mut self, until_ens: i64) {
        self.backoff_until_ens = until_ens;
    }

    /// `now_ens < fallback_until_ens`.
    #[inline]
    pub fn in_fallback(&self, now_ens: i64) -> bool {
        now_ens < self.fallback_until_ens
    }

    /// Mode event: clears the cooldown and fallback deadlines; everything else unchanged.
    #[inline]
    pub fn reset_open_state(&mut self) {
        self.cooldown_until_ens = 0;
        self.fallback_until_ens = 0;
    }

    /// Consecutive failures so far (saturating at 255).
    #[inline]
    pub fn consec_fail(&self) -> u8 {
        self.consec_fail
    }

    /// Whether the halted mirror is set.
    #[inline]
    pub fn halted(&self) -> bool {
        self.halted
    }

    /// 35 B in field order: i64, u8, u8, i64, i64, u8, i64.
    pub fn snapshot(&self, w: &mut SnapshotWriter) {
        w.put_i64(self.cooldown_until_ens);
        w.put_u8(u8::from(self.in_flight));
        w.put_u8(self.consec_fail);
        w.put_i64(self.fallback_until_ens);
        w.put_i64(self.backoff_until_ens);
        w.put_u8(u8::from(self.halted));
        w.put_i64(self.hard_rearm_until_ens);
    }

    /// On error (truncated, flag byte > 1) `self` is unchanged.
    pub fn restore(&mut self, r: &mut SnapshotReader<'_>) -> Result<(), RestoreError> {
        let cooldown_until_ens = r.get_i64()?;
        let in_flight = get_bool(r)?;
        let consec_fail = r.get_u8()?;
        let fallback_until_ens = r.get_i64()?;
        let backoff_until_ens = r.get_i64()?;
        let halted = get_bool(r)?;
        let hard_rearm_until_ens = r.get_i64()?;
        *self = Throttle {
            cooldown_until_ens,
            in_flight,
            consec_fail,
            fallback_until_ens,
            backoff_until_ens,
            halted,
            hard_rearm_until_ens,
        };
        Ok(())
    }
}

fn get_bool(r: &mut SnapshotReader<'_>) -> Result<bool, RestoreError> {
    match r.get_u8()? {
        0 => Ok(false),
        1 => Ok(true),
        _ => Err(RestoreError::BadValue),
    }
}
