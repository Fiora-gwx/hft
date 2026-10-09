//! ECF1 section 3 (`CapacitiesRow`) and section 5 (`SeqParamsRow`): row layouts, defaults, decoding
//! and the semantic validators (README "环与容量").

use hft_refdata::{
    DEFAULT_MAX_UNIVERSE_INSTS, EngineConfig, EngineSection, EngineValidator, LoadErr, LoadErrCode, RefData,
};
use zerocopy::{FromBytes, Immutable, IntoBytes, KnownLayout};

/// Ring classes indexing `SeqParamsRow.rings` (0 and 7 reserved; rep-sim uses
/// `sim_max_per_round`).
#[repr(u8)]
#[derive(Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Debug)]
pub enum RingClass {
    Ctl = 1,
    Hk = 2,
    Rep = 3,
    Chain = 4,
    Bbo = 5,
    Advice = 6,
}

impl TryFrom<u8> for RingClass {
    type Error = u8;
    fn try_from(v: u8) -> Result<RingClass, u8> {
        Ok(match v {
            1 => RingClass::Ctl,
            2 => RingClass::Hk,
            3 => RingClass::Rep,
            4 => RingClass::Chain,
            5 => RingClass::Bbo,
            6 => RingClass::Advice,
            _ => return Err(v),
        })
    }
}

/// Per-round take limits of one ring class: per ring and in total.
#[repr(C)]
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug, Default, FromBytes, IntoBytes, KnownLayout, Immutable)]
pub struct RingLimit {
    pub per_ring: u16,
    pub total: u16,
}

/// Section 5 row (72 B, row version 1; fields only appended).
#[repr(C)]
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug, FromBytes, IntoBytes, KnownLayout, Immutable)]
pub struct SeqParamsRow {
    /// Index = `RingClass`.
    pub rings: [RingLimit; 8],
    pub md_round_ns: u32,
    pub ctl_bytes_per_round: u32,
    pub keyframe_period_ms: u32,
    pub keyframe_max_events: u32,
    pub advisor_mem_budget_mib: u32,
    pub advisor_ckpt_max_mib: u32,
    pub sim_max_per_round: u16,
    pub conflate_backlog: u16,
    pub notify_per_continue: u16,
    pub replay_per_round: u16,
    /// bool as 0/1.
    pub auto_resume_after_clean_drain: u8,
    /// bool as 0/1.
    pub allow_missing_standby: u8,
    pub _pad: [u8; 6],
}

const _: () = assert!(core::mem::size_of::<RingLimit>() == 4);
const _: () = assert!(core::mem::size_of::<SeqParamsRow>() == 72);
const _: () = assert!(core::mem::offset_of!(SeqParamsRow, md_round_ns) == 32);
const _: () = assert!(core::mem::offset_of!(SeqParamsRow, advisor_mem_budget_mib) == 48);
const _: () = assert!(core::mem::offset_of!(SeqParamsRow, sim_max_per_round) == 56);
const _: () = assert!(core::mem::offset_of!(SeqParamsRow, auto_resume_after_clean_drain) == 64);
const _: () = assert!(core::mem::offset_of!(SeqParamsRow, _pad) == 66);

/// Section 5 row version.
pub const SEQ_PARAMS_ROW_VERSION: u16 = 1;
/// Section 3 row version.
pub const CAPACITIES_ROW_VERSION: u16 = 1;

impl Default for SeqParamsRow {
    fn default() -> SeqParamsRow {
        let mut rings = [RingLimit::default(); 8];
        rings[RingClass::Ctl as usize] = RingLimit {
            per_ring: 4,
            total: 4,
        };
        rings[RingClass::Hk as usize] = RingLimit {
            per_ring: 4,
            total: 4,
        };
        rings[RingClass::Rep as usize] = RingLimit {
            per_ring: 32,
            total: 64,
        };
        rings[RingClass::Chain as usize] = RingLimit {
            per_ring: 32,
            total: 0,
        };
        rings[RingClass::Bbo as usize] = RingLimit {
            per_ring: 32,
            total: 128,
        };
        rings[RingClass::Advice as usize] = RingLimit {
            per_ring: 16,
            total: 16,
        };
        SeqParamsRow {
            rings,
            md_round_ns: 50_000,
            ctl_bytes_per_round: 8_192,
            keyframe_period_ms: 300_000,
            keyframe_max_events: 1_500_000,
            advisor_mem_budget_mib: 0,
            advisor_ckpt_max_mib: 0,
            sim_max_per_round: 32,
            conflate_backlog: 64,
            notify_per_continue: 64,
            replay_per_round: 1_024,
            auto_resume_after_clean_drain: 1,
            allow_missing_standby: 0,
            _pad: [0; 6],
        }
    }
}

impl SeqParamsRow {
    /// The limit of one ring class.
    pub fn ring(&self, c: RingClass) -> RingLimit {
        self.rings.get(c as usize).copied().unwrap_or_default()
    }
    /// The semantic check of README "段 3、段 5 的语义校验" (section-5 part).
    pub fn check(&self) -> Result<(), LoadErrCode> {
        let bad = |b: bool| if b { Err(LoadErrCode::ValueRange) } else { Ok(()) };
        bad(self.auto_resume_after_clean_drain > 1 || self.allow_missing_standby > 1)?;
        bad(self._pad.iter().any(|b| *b != 0))?;
        for ix in [0usize, 7] {
            let r = self.rings.get(ix).copied().unwrap_or_default();
            bad(r.per_ring != 0 || r.total != 0)?;
        }
        for ix in 1u8..=6 {
            let r = self.rings.get(usize::from(ix)).copied().unwrap_or_default();
            bad(r.per_ring == 0)?;
            if ix != RingClass::Chain as u8 {
                bad(r.per_ring > r.total)?;
            }
        }
        bad(self.md_round_ns == 0
            || self.ctl_bytes_per_round == 0
            || self.sim_max_per_round == 0
            || self.notify_per_continue == 0
            || self.keyframe_period_ms == 0
            || self.keyframe_max_events == 0
            || self.replay_per_round == 0)
    }
}

/// Section 3 row (24 B, row version 1).
#[repr(C)]
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug, Default, FromBytes, IntoBytes, KnownLayout, Immutable)]
pub struct CapacitiesRow {
    pub max_insts: u32,
    pub max_cells_total: u32,
    pub max_timers: u32,
    pub recent_cmd_ids: u32,
    pub max_subs_per_inst: u16,
    pub max_groups_per_instance: u16,
    pub max_instances: u16,
    pub _pad: u16,
}

const _: () = assert!(core::mem::size_of::<CapacitiesRow>() == 24);
const _: () = assert!(core::mem::offset_of!(CapacitiesRow, max_subs_per_inst) == 16);
const _: () = assert!(core::mem::offset_of!(CapacitiesRow, _pad) == 22);

impl CapacitiesRow {
    /// Section-3 semantic check: every capacity non-zero, `max_insts ≤ max_universe_insts`.
    pub fn check(&self, max_universe_insts: u32) -> Result<(), LoadErrCode> {
        if self.max_insts == 0
            || self.max_cells_total == 0
            || self.max_timers == 0
            || self.recent_cmd_ids == 0
            || self.max_subs_per_inst == 0
            || self.max_groups_per_instance == 0
            || self.max_instances == 0
            || self._pad != 0
            || self.max_insts > max_universe_insts
        {
            return Err(LoadErrCode::ValueRange);
        }
        Ok(())
    }
    /// The ledger's mirror of this row.
    pub fn to_ledger(&self) -> hft_ledger::Capacities {
        hft_ledger::Capacities {
            max_insts: self.max_insts,
            max_cells_total: self.max_cells_total,
            max_timers: self.max_timers,
            recent_cmd_ids: self.recent_cmd_ids,
            max_subs_per_inst: self.max_subs_per_inst,
            max_groups_per_instance: self.max_groups_per_instance,
            max_instances: self.max_instances,
        }
    }
    /// Per-input notification bound `U = max_cells_total + 128`.
    pub fn notify_unit(&self) -> u32 {
        self.max_cells_total
            .saturating_add(crate::notify::NOTIFY_QUEUE_CAP as u32)
    }
}

/// Reads the single row of a section with prefix reading (`row_size ≥` the known size).
fn one_row<T: FromBytes + KnownLayout + Immutable + Copy>(
    ec: &EngineConfig,
    s: EngineSection,
) -> Result<T, LoadErrCode> {
    let v = ec.section(s).ok_or(LoadErrCode::Structure)?;
    let size = core::mem::size_of::<T>();
    if v.count != 1 || (v.row_size as usize) < size {
        return Err(LoadErrCode::Structure);
    }
    let bytes = v.bytes.get(..size).ok_or(LoadErrCode::Structure)?;
    T::read_from_bytes(bytes).map_err(|_| LoadErrCode::Structure)
}

/// Decodes section 5 (exactly one row, prefix read when the row is longer).
pub fn seq_params(ec: &EngineConfig) -> Result<SeqParamsRow, LoadErrCode> {
    one_row::<SeqParamsRow>(ec, EngineSection::Sequencer)
}

/// Decodes section 3.
pub fn capacities(ec: &EngineConfig) -> Result<CapacitiesRow, LoadErrCode> {
    one_row::<CapacitiesRow>(ec, EngineSection::Capacities)
}

fn check_seq_section(ec: &EngineConfig, _rd: &RefData) -> Result<(), LoadErr> {
    let sec = EngineSection::Sequencer as u16;
    let row = seq_params(ec).map_err(|c| LoadErr::new(c, sec, 0))?;
    row.check().map_err(|c| LoadErr::new(c, sec, 0))
}

fn check_caps_section(ec: &EngineConfig, _rd: &RefData) -> Result<(), LoadErr> {
    let sec = EngineSection::Capacities as u16;
    let row = capacities(ec).map_err(|c| LoadErr::new(c, sec, 0))?;
    row.check(DEFAULT_MAX_UNIVERSE_INSTS)
        .map_err(|c| LoadErr::new(c, sec, 0))
}

/// The `EngineValidator` of section 5.
pub const SEQ_VALIDATOR: EngineValidator = EngineValidator {
    section: EngineSection::Sequencer,
    check: check_seq_section,
};

/// The `EngineValidator` of section 3.
pub const CAPS_VALIDATOR: EngineValidator = EngineValidator {
    section: EngineSection::Capacities,
    check: check_caps_section,
};

/// Both validators of this crate.
pub const VALIDATORS: [EngineValidator; 2] = [CAPS_VALIDATOR, SEQ_VALIDATOR];
