//! `Timer` owner/timer_id/group packing (README "定时器编码"; the only implementation).

use hft_types::{AccountId, GROUP_ALL, GROUP_NONE, GroupId, InstId, StrategyId, VenueId};

/// Strategy owner = 16 + StrategyId (17..=270); 0 and 6..=16 unassigned.
pub const TIMER_OWNER_STRATEGY_BASE: u16 = 16;

const OWNER_SYS: u16 = 1;
const OWNER_KEYFRAME: u16 = 2;
const OWNER_OMS: u16 = 3;
const OWNER_RISK: u16 = 4;
const OWNER_EXEC: u16 = 5;

/// Decoded timer code.
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
pub enum TimerCode {
    Sys(SysTimer),
    Keyframe {
        inst: InstId,
    },
    Oms(OmsTimer),
    Risk {
        kind: u8,
        account: AccountId,
        venue: VenueId,
    },
    Exec {
        kind: u8,
        account: AccountId,
        group: GroupId,
    },
    Strategy {
        strategy: StrategyId,
        tag: u32,
        group: GroupId,
    },
}

/// `owner = Sys` ids 1..=18.
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
pub enum SysTimer {
    Snapshot,
    Roll,
    DayRoll { day: u32 },
    DeadMan { venue: VenueId, account: AccountId },
    ConnRotate { venue: VenueId, account: AccountId },
    AdvisorTick,
    SnapshotNow,
    ClockOk,
    ClockBad,
    ConflateReport,
    SessionKeepalive { venue: VenueId, account: AccountId },
    PersistLag { level: u8 },
    DiskLow { level: u8 },
    StorageOk,
    FundingSettle { venue: VenueId },
    KillSelfTestOk { account: AccountId },
    NotifyContinue,
    RefdataRecheck { arg: u8 },
}

/// `owner = Oms` ids 1..=5.
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
pub enum OmsTimer {
    Continue,
    Sweep,
    UnknownQuery { table_ix: u16, slot: u32 },
    HedgeRetry { link_id: u32 },
    SmokeTtl { account: AccountId, venue: VenueId },
}

/// Packing errors (checked in this order).
#[repr(u8)]
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
pub enum TimerErr {
    Owner = 1,
    Id = 2,
    Range = 3,
    Group = 4,
}

const fn va(id: u32, venue: VenueId, account: AccountId) -> u32 {
    id | (venue.0 as u32) << 8 | (account.0 as u32) << 16
}

impl SysTimer {
    fn encode(self) -> Result<u32, TimerErr> {
        Ok(match self {
            SysTimer::Snapshot => 1,
            SysTimer::Roll => 2,
            SysTimer::DayRoll { day } => {
                if day >= 1 << 24 {
                    return Err(TimerErr::Range);
                }
                3 | day << 8
            }
            SysTimer::DeadMan { venue, account } => va(4, venue, account),
            SysTimer::ConnRotate { venue, account } => va(5, venue, account),
            SysTimer::AdvisorTick => 6,
            SysTimer::SnapshotNow => 7,
            SysTimer::ClockOk => 8,
            SysTimer::ClockBad => 9,
            SysTimer::ConflateReport => 10,
            SysTimer::SessionKeepalive { venue, account } => va(11, venue, account),
            SysTimer::PersistLag { level } | SysTimer::DiskLow { level } => {
                if !matches!(level, 1 | 2) {
                    return Err(TimerErr::Range);
                }
                let id = if matches!(self, SysTimer::PersistLag { .. }) {
                    12
                } else {
                    13
                };
                id | u32::from(level) << 8
            }
            SysTimer::StorageOk => 14,
            SysTimer::FundingSettle { venue } => 15 | u32::from(venue.0) << 8,
            SysTimer::KillSelfTestOk { account } => 16 | u32::from(account.0) << 16,
            SysTimer::NotifyContinue => 17,
            SysTimer::RefdataRecheck { arg } => {
                if arg > 5 {
                    return Err(TimerErr::Range);
                }
                18 | u32::from(arg) << 8
            }
        })
    }

    fn decode(t: u32) -> Result<SysTimer, TimerErr> {
        let id = t & 0xFF;
        let b1 = (t >> 8) as u8;
        let b2 = (t >> 16) as u8;
        // Bits that must be zero for each id (`Id` otherwise).
        let unused = match id {
            1 | 2 | 6..=10 | 14 | 17 => 0xFFFF_FF00,
            3 => 0,
            4 | 5 | 11 => 0xFF00_0000,
            12 | 13 | 15 | 18 => 0xFFFF_0000,
            16 => 0xFF00_FF00,
            _ => return Err(TimerErr::Id),
        };
        if t & unused != 0 {
            return Err(TimerErr::Id);
        }
        let (v, a) = (VenueId(b1), AccountId(b2));
        Ok(match id {
            1 => SysTimer::Snapshot,
            2 => SysTimer::Roll,
            3 => SysTimer::DayRoll { day: t >> 8 },
            4 => SysTimer::DeadMan { venue: v, account: a },
            5 => SysTimer::ConnRotate { venue: v, account: a },
            6 => SysTimer::AdvisorTick,
            7 => SysTimer::SnapshotNow,
            8 => SysTimer::ClockOk,
            9 => SysTimer::ClockBad,
            10 => SysTimer::ConflateReport,
            11 => SysTimer::SessionKeepalive { venue: v, account: a },
            12 | 13 => {
                if !matches!(b1, 1 | 2) {
                    return Err(TimerErr::Range);
                }
                if id == 12 {
                    SysTimer::PersistLag { level: b1 }
                } else {
                    SysTimer::DiskLow { level: b1 }
                }
            }
            14 => SysTimer::StorageOk,
            15 => SysTimer::FundingSettle { venue: v },
            16 => SysTimer::KillSelfTestOk { account: a },
            17 => SysTimer::NotifyContinue,
            _ => {
                if b1 > 5 {
                    return Err(TimerErr::Range);
                }
                SysTimer::RefdataRecheck { arg: b1 }
            }
        })
    }
}

impl OmsTimer {
    fn encode(self) -> Result<u32, TimerErr> {
        Ok(match self {
            OmsTimer::Continue => 1,
            OmsTimer::Sweep => 2,
            OmsTimer::UnknownQuery { table_ix, slot } => {
                if table_ix >= 256 || slot >= 65_536 {
                    return Err(TimerErr::Range);
                }
                3 | u32::from(table_ix) << 8 | slot << 16
            }
            OmsTimer::HedgeRetry { link_id } => {
                if link_id >= 1 << 24 {
                    return Err(TimerErr::Range);
                }
                4 | link_id << 8
            }
            OmsTimer::SmokeTtl { account, venue } => 5 | u32::from(account.0) << 8 | u32::from(venue.0) << 16,
        })
    }

    fn decode(t: u32) -> Result<OmsTimer, TimerErr> {
        let id = t & 0xFF;
        let unused = match id {
            1 | 2 => 0xFFFF_FF00,
            3 | 4 => 0,
            5 => 0xFF00_0000,
            _ => return Err(TimerErr::Id),
        };
        if t & unused != 0 {
            return Err(TimerErr::Id);
        }
        Ok(match id {
            1 => OmsTimer::Continue,
            2 => OmsTimer::Sweep,
            3 => OmsTimer::UnknownQuery {
                table_ix: ((t >> 8) & 0xFF) as u16,
                slot: t >> 16,
            },
            4 => OmsTimer::HedgeRetry { link_id: t >> 8 },
            _ => OmsTimer::SmokeTtl {
                account: AccountId((t >> 8) as u8),
                venue: VenueId((t >> 16) as u8),
            },
        })
    }
}

impl TimerCode {
    /// `(owner, timer_id, group)`; owners other than Exec/Strategy return `GROUP_NONE`.
    pub fn encode(&self) -> Result<(u16, u32, GroupId), TimerErr> {
        match *self {
            TimerCode::Sys(s) => Ok((OWNER_SYS, s.encode()?, GROUP_NONE)),
            TimerCode::Keyframe { inst } => Ok((OWNER_KEYFRAME, inst.0, GROUP_NONE)),
            TimerCode::Oms(o) => Ok((OWNER_OMS, o.encode()?, GROUP_NONE)),
            TimerCode::Risk { kind, account, venue } => {
                if !(1..=3).contains(&kind) {
                    return Err(TimerErr::Id);
                }
                Ok((
                    OWNER_RISK,
                    u32::from(kind) | u32::from(account.0) << 8 | u32::from(venue.0) << 16,
                    GROUP_NONE,
                ))
            }
            TimerCode::Exec { kind, account, group } => {
                if !(1..=6).contains(&kind) {
                    return Err(TimerErr::Id);
                }
                if group == GROUP_ALL {
                    return Err(TimerErr::Group);
                }
                Ok((OWNER_EXEC, u32::from(kind) | u32::from(account.0) << 8, group))
            }
            TimerCode::Strategy { strategy, tag, group } => {
                if !(1..=254).contains(&strategy.0) {
                    return Err(TimerErr::Owner);
                }
                if group == GROUP_ALL {
                    return Err(TimerErr::Group);
                }
                Ok((TIMER_OWNER_STRATEGY_BASE + u16::from(strategy.0), tag, group))
            }
        }
    }

    /// Inverse of `encode`; checks Owner → Id → Range → Group.
    pub fn decode(owner: u16, timer_id: u32, group: GroupId) -> Result<TimerCode, TimerErr> {
        let grouped = matches!(owner, OWNER_EXEC | 17..=270);
        let code = match owner {
            OWNER_SYS => TimerCode::Sys(SysTimer::decode(timer_id)?),
            OWNER_KEYFRAME => TimerCode::Keyframe {
                inst: InstId(timer_id),
            },
            OWNER_OMS => TimerCode::Oms(OmsTimer::decode(timer_id)?),
            OWNER_RISK => {
                let kind = (timer_id & 0xFF) as u8;
                if !(1..=3).contains(&kind) || timer_id >> 24 != 0 {
                    return Err(TimerErr::Id);
                }
                TimerCode::Risk {
                    kind,
                    account: AccountId((timer_id >> 8) as u8),
                    venue: VenueId((timer_id >> 16) as u8),
                }
            }
            OWNER_EXEC => {
                let kind = (timer_id & 0xFF) as u8;
                if !(1..=6).contains(&kind) || timer_id >> 16 != 0 {
                    return Err(TimerErr::Id);
                }
                TimerCode::Exec {
                    kind,
                    account: AccountId((timer_id >> 8) as u8),
                    group,
                }
            }
            17..=270 => TimerCode::Strategy {
                strategy: StrategyId((owner - TIMER_OWNER_STRATEGY_BASE) as u8),
                tag: timer_id,
                group,
            },
            _ => return Err(TimerErr::Owner),
        };
        if group == GROUP_ALL || (!grouped && group != GROUP_NONE) {
            return Err(TimerErr::Group);
        }
        Ok(code)
    }
}
