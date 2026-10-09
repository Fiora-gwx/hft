//! Snapshot sections of this crate (README "快照行格式"): 4 `RiskState` (cap records),
//! 14 `ModeTable`, 17 `RiskCounters`, 18 `Gates`, 23 `MarginState`; `Risk::snapshot` writes them
//! in this order, `Risk::restore` checks every row before building the state.
// zerocopy's derive helpers of the row structs carry no Debug impl.
#![allow(missing_debug_implementations)]

use hft_event::CapRecord;
use hft_refdata::RefData;
use hft_strategy_sdk::{RestoreError, SnapshotReader, SnapshotWriter};
use hft_types::{AccountId, CapSource, GroupId, InstId, Notional, Qty, Rate, RunMode, StrategyId, VenueId};
use zerocopy::{FromBytes, IntoBytes};

use crate::bucket::Buckets;
use crate::margin::MState;
use crate::mode::{CellKey, Effective, ReconItemKey};
use crate::pnl_gate::{AcctPnl, ReconKeyRow};
use crate::types::{Capacities, ExecBounds, GwMap, ModeCfg};
use crate::{Risk, RiskParams};

pub const SECTION_RISK_STATE: u16 = 4;
pub const SECTION_MODE_TABLE: u16 = 14;
pub const SECTION_RISK_COUNTERS: u16 = 17;
pub const SECTION_GATES: u16 = 18;
pub const SECTION_MARGIN: u16 = 23;

pub const RISK_STATE_VERSION: u16 = 1;
pub const MODE_TABLE_VERSION: u16 = 2;
pub const RISK_COUNTERS_VERSION: u16 = 4;
pub const GATES_VERSION: u16 = 5;
pub const MARGIN_VERSION: u16 = 3;

/// `RiskRowHdr.kind` values (4 and 5 reserved).
pub mod row_kind {
    pub const ACCT: u8 = 1;
    pub const BUCKET: u8 = 2;
    pub const RECON: u8 = 3;
    pub const RECON_GATE: u8 = 6;
    pub const LEV: u8 = 7;
    pub const USER_STREAM: u8 = 8;
}

/// `ModeRow.flags`.
pub mod mode_row_flags {
    /// The row is the (strategy, account) `default_mode` (`group = GROUP_NONE`).
    pub const DEFAULT: u8 = 1;
    /// Desired Shadow already effective.
    pub const LANDED: u8 = 2;
}

/// `GateRow.gflags`.
pub mod gate_flags {
    /// The account's kill self-test passed in this run.
    pub const KILL_SELFTEST: u64 = 1;
    /// `StorageCritical` already produced for the current Storage cap (global; same in every row).
    pub const STORAGE_DRAIN_SENT: u64 = 2;
}

macro_rules! row {
    ($(#[$m:meta])* pub struct $name:ident { $($(#[$fm:meta])* pub $f:ident : $t:ty),* $(,)? }) => {
        $(#[$m])*
        #[repr(C)]
        #[derive(Copy, Clone, Eq, PartialEq, Debug, Default, zerocopy::FromBytes, zerocopy::IntoBytes,
            zerocopy::KnownLayout, zerocopy::Immutable)]
        pub struct $name { $($(#[$fm])* pub $f: $t),* }
    };
}

row! {
    /// Header of every section-17 row (8 B).
    pub struct RiskRowHdr { pub kind: u8, pub account: u8, pub venue: u8, pub _pad: u8, pub inst: u32 }
}
const _: () = assert!(size_of::<RiskRowHdr>() == 8 && align_of::<RiskRowHdr>() == 4);
const _: () = assert!(core::mem::offset_of!(RiskRowHdr, kind) == 0);
const _: () = assert!(core::mem::offset_of!(RiskRowHdr, account) == 1);
const _: () = assert!(core::mem::offset_of!(RiskRowHdr, venue) == 2);
const _: () = assert!(core::mem::offset_of!(RiskRowHdr, _pad) == 3);
const _: () = assert!(core::mem::offset_of!(RiskRowHdr, inst) == 4);

row! {
    /// Daily loss of one account (40 B).
    pub struct RiskAcctRow {
        pub day_pnl_base: i64, pub loss_trigger: i64, pub loss_quota: i64, pub equity_sod: i64,
        pub resumes_today: u8, pub eq_flags: u8, pub _pad: [u8; 6],
    }
}
const _: () = assert!(size_of::<RiskAcctRow>() == 40);
const _: () = assert!(core::mem::offset_of!(RiskAcctRow, loss_trigger) == 8);
const _: () = assert!(core::mem::offset_of!(RiskAcctRow, loss_quota) == 16);
const _: () = assert!(core::mem::offset_of!(RiskAcctRow, equity_sod) == 24);
const _: () = assert!(core::mem::offset_of!(RiskAcctRow, resumes_today) == 32);
const _: () = assert!(core::mem::offset_of!(RiskAcctRow, eq_flags) == 33);
const _: () = assert!(core::mem::offset_of!(RiskAcctRow, _pad) == 34);

row! {
    /// Token bucket (40 B; nano-tokens, engine_ns). `inst = 0`: (venue, account) windows;
    /// `inst ≠ 0`: the (account, inst) 1 s bucket in `tokens_10s`.
    pub struct RiskBucketRow {
        pub tokens_10s: i64, pub tokens_min: i64, pub last_ens: i64, pub backoff_until_ens: i64,
        pub backoff_k: u32, pub _pad: u32,
    }
}
const _: () = assert!(size_of::<RiskBucketRow>() == 40);
const _: () = assert!(core::mem::offset_of!(RiskBucketRow, tokens_min) == 8);
const _: () = assert!(core::mem::offset_of!(RiskBucketRow, last_ens) == 16);
const _: () = assert!(core::mem::offset_of!(RiskBucketRow, backoff_until_ens) == 24);
const _: () = assert!(core::mem::offset_of!(RiskBucketRow, backoff_k) == 32);
const _: () = assert!(core::mem::offset_of!(RiskBucketRow, _pad) == 36);

row! {
    /// Known effective leverage of (account, venue, inst) (8 B).
    pub struct RiskLevRow { pub leverage: i64 }
}
const _: () = assert!(size_of::<RiskLevRow>() == 8);

row! {
    /// Reconciliation counters (16 B): `inst = 0` → (account, venue) Ignore streak; otherwise
    /// one key (`ASSET_KEY` flag for balances) with its Warn streak and last difference.
    pub struct ReconRow { pub ignore_streak: u8, pub warn_streak: u8, pub _pad: [u8; 6], pub last_diff: i64 }
}
const _: () = assert!(size_of::<ReconRow>() == 16);
const _: () = assert!(core::mem::offset_of!(ReconRow, warn_streak) == 1);
const _: () = assert!(core::mem::offset_of!(ReconRow, _pad) == 2);
const _: () = assert!(core::mem::offset_of!(ReconRow, last_diff) == 8);

row! {
    /// Latest reconciliation group of (account, venue) (32 B) + `n_items` (≤ 64) keys of 8 B.
    pub struct ReconGateRowHdr {
        pub report_id: u64, pub disposed_mask: u64, pub n_items: u16, pub flags: u16, pub _pad: u32,
        pub view_seq: u64,
    }
}
const _: () = assert!(size_of::<ReconGateRowHdr>() == 32);
const _: () = assert!(core::mem::offset_of!(ReconGateRowHdr, disposed_mask) == 8);
const _: () = assert!(core::mem::offset_of!(ReconGateRowHdr, n_items) == 16);
const _: () = assert!(core::mem::offset_of!(ReconGateRowHdr, flags) == 18);
const _: () = assert!(core::mem::offset_of!(ReconGateRowHdr, _pad) == 20);
const _: () = assert!(core::mem::offset_of!(ReconGateRowHdr, view_seq) == 24);

row! {
    /// User-stream row of (account, venue) (24 B).
    pub struct RiskUsRow { pub us_up_seq: u64, pub prev_view_seq: u64, pub uflags: u8, pub _pad: [u8; 7] }
}
const _: () = assert!(size_of::<RiskUsRow>() == 24);
const _: () = assert!(core::mem::offset_of!(RiskUsRow, prev_view_seq) == 8);
const _: () = assert!(core::mem::offset_of!(RiskUsRow, uflags) == 16);
const _: () = assert!(core::mem::offset_of!(RiskUsRow, _pad) == 17);

row! {
    /// Margin state of (account, venue) (row version 3, 88 B; times are engine_ns).
    pub struct MarginRow {
        pub account: u8, pub venue: u8, pub level: u8, pub mflags: u8, pub _pad: u32,
        pub report_seq: u64, pub report_ens: i64, pub reported_ratio: i64, pub est_ratio: i64,
        pub rep_liq_dist: i64, pub est_liq_dist: i64,
        pub l2_since_ens: i64, pub deadline_ens: i64, pub next_force_ens: i64,
        pub mark_ens: i64,
    }
}
const _: () = assert!(size_of::<MarginRow>() == 88);
const _: () = assert!(core::mem::offset_of!(MarginRow, venue) == 1);
const _: () = assert!(core::mem::offset_of!(MarginRow, level) == 2);
const _: () = assert!(core::mem::offset_of!(MarginRow, mflags) == 3);
const _: () = assert!(core::mem::offset_of!(MarginRow, _pad) == 4);
const _: () = assert!(core::mem::offset_of!(MarginRow, report_seq) == 8);
const _: () = assert!(core::mem::offset_of!(MarginRow, report_ens) == 16);
const _: () = assert!(core::mem::offset_of!(MarginRow, reported_ratio) == 24);
const _: () = assert!(core::mem::offset_of!(MarginRow, est_ratio) == 32);
const _: () = assert!(core::mem::offset_of!(MarginRow, rep_liq_dist) == 40);
const _: () = assert!(core::mem::offset_of!(MarginRow, est_liq_dist) == 48);
const _: () = assert!(core::mem::offset_of!(MarginRow, l2_since_ens) == 56);
const _: () = assert!(core::mem::offset_of!(MarginRow, deadline_ens) == 64);
const _: () = assert!(core::mem::offset_of!(MarginRow, next_force_ens) == 72);
const _: () = assert!(core::mem::offset_of!(MarginRow, mark_ens) == 80);

row! {
    /// Desired mode of a cell, or a (strategy, account) `default_mode` (8 B, row version 2).
    pub struct ModeRow { pub strategy: u8, pub account: u8, pub mode: u8, pub flags: u8, pub group: u32 }
}
const _: () = assert!(size_of::<ModeRow>() == 8);
const _: () = assert!(core::mem::offset_of!(ModeRow, mode) == 2);
const _: () = assert!(core::mem::offset_of!(ModeRow, flags) == 3);
const _: () = assert!(core::mem::offset_of!(ModeRow, group) == 4);

row! {
    /// Gate state of an account (64 B, row version 5).
    pub struct GateRow {
        pub account: u8, pub released: u8, pub clean_restart: u8, pub recovery_flags: u8, pub gate_bits: u32,
        pub reconciled_venues: u64, pub cfg_ok_venues: u64, pub identity_ok_venues: u64, pub conns_ok_venues: u64,
        pub ctl_state_version: u64, pub standby_ok_venues: u64,
        pub gflags: u64,
    }
}
const _: () = assert!(size_of::<GateRow>() == 64);
const _: () = assert!(core::mem::offset_of!(GateRow, gate_bits) == 4);
const _: () = assert!(core::mem::offset_of!(GateRow, reconciled_venues) == 8);
const _: () = assert!(core::mem::offset_of!(GateRow, ctl_state_version) == 40);
const _: () = assert!(core::mem::offset_of!(GateRow, standby_ok_venues) == 48);
const _: () = assert!(core::mem::offset_of!(GateRow, gflags) == 56);

fn hdr(kind: u8, a: AccountId, v: VenueId, inst: u32) -> RiskRowHdr {
    RiskRowHdr {
        kind,
        account: a.0,
        venue: v.0,
        _pad: 0,
        inst,
    }
}

fn mode_code(m: RunMode) -> u8 {
    m as u8
}
fn mode_of(c: u8) -> Option<RunMode> {
    match c {
        1 => Some(RunMode::Shadow),
        3 => Some(RunMode::CloseOnly),
        4 => Some(RunMode::Live),
        _ => None,
    }
}

impl Risk {
    /// Writes sections 4, 14, 17, 18 and 23 (in this order).
    pub fn snapshot(&self, w: &mut SnapshotWriter) {
        let m = &self.mode;
        let s = &self.s;
        w.begin_section(SECTION_RISK_STATE, RISK_STATE_VERSION);
        for r in m.caps.iter() {
            w.put_row(r);
        }
        w.begin_section(SECTION_MODE_TABLE, MODE_TABLE_VERSION);
        for d in m.defaults.iter() {
            w.put_row(&ModeRow {
                strategy: d.strategy.0,
                account: d.account.0,
                mode: mode_code(d.mode),
                flags: mode_row_flags::DEFAULT,
                group: 0,
            });
        }
        for c in m.cells.iter() {
            w.put_row(&ModeRow {
                strategy: c.key.strategy.0,
                account: c.key.account.0,
                mode: mode_code(c.desired),
                flags: if c.landed { mode_row_flags::LANDED } else { 0 },
                group: c.key.group.0,
            });
        }
        w.begin_section(SECTION_RISK_COUNTERS, RISK_COUNTERS_VERSION);
        let n = s.n_accounts;
        let put = |w: &mut SnapshotWriter, h: RiskRowHdr, body: &[u8]| {
            w.put_bytes(h.as_bytes());
            w.put_bytes(body);
            w.add_rows(1);
        };
        for a in 1..=n {
            let acc = AccountId(a as u8);
            let p = s.pnl.get(a).copied().unwrap_or_default();
            let row = RiskAcctRow {
                day_pnl_base: p.day_pnl_base.0,
                loss_trigger: p.loss_trigger.0,
                loss_quota: p.loss_quota.0,
                equity_sod: p.equity_sod.0,
                resumes_today: p.resumes_today,
                eq_flags: p.eq_flags,
                _pad: [0; 6],
            };
            put(w, hdr(row_kind::ACCT, acc, VenueId(0), 0), row.as_bytes());
        }
        for a in 1..=n {
            let acc = AccountId(a as u8);
            for v in 0u8..64 {
                if s.venues(acc) & (1u64 << v) == 0 {
                    continue;
                }
                let b = s
                    .buckets
                    .venue
                    .get(Buckets::vix(acc, VenueId(v)))
                    .copied()
                    .unwrap_or_default();
                let row = RiskBucketRow {
                    tokens_10s: b.tokens_10s,
                    tokens_min: b.tokens_min,
                    last_ens: b.last_ens,
                    backoff_until_ens: b.backoff_until_ens,
                    backoff_k: b.backoff_k,
                    _pad: 0,
                };
                put(w, hdr(row_kind::BUCKET, acc, VenueId(v), 0), row.as_bytes());
                for i in 1..s.insts.len() {
                    if s.insts.get(i).is_none_or(|f| f.venue != VenueId(v)) {
                        continue;
                    }
                    let ib = s
                        .buckets
                        .inst
                        .get(s.buckets.iix(acc, InstId(i as u32)))
                        .copied()
                        .unwrap_or_default();
                    let row = RiskBucketRow {
                        tokens_10s: ib.tokens,
                        tokens_min: 0,
                        last_ens: ib.last_ens,
                        backoff_until_ens: 0,
                        backoff_k: 0,
                        _pad: 0,
                    };
                    put(
                        w,
                        hdr(row_kind::BUCKET, acc, VenueId(v), i as u32),
                        row.as_bytes(),
                    );
                }
            }
        }
        for a in 1..=n {
            let acc = AccountId(a as u8);
            for v in 0u8..64 {
                let st = s.recon.streak(acc, VenueId(v));
                if st != 0 {
                    let row = ReconRow {
                        ignore_streak: st,
                        ..ReconRow::default()
                    };
                    put(w, hdr(row_kind::RECON, acc, VenueId(v), 0), row.as_bytes());
                }
            }
        }
        for k in s.recon.keys.iter() {
            let row = ReconRow {
                ignore_streak: 0,
                warn_streak: k.warn_streak,
                _pad: [0; 6],
                last_diff: k.last_diff.0,
            };
            put(w, hdr(row_kind::RECON, k.account, k.venue, k.key), row.as_bytes());
        }
        for a in 1..=n {
            let acc = AccountId(a as u8);
            for v in 0u8..64 {
                let Some(r) = m.recon_row(acc, VenueId(v)) else {
                    continue;
                };
                if !r.present {
                    continue;
                }
                let h = ReconGateRowHdr {
                    report_id: r.report_id,
                    disposed_mask: r.disposed_mask,
                    n_items: r.n_items,
                    flags: r.flags,
                    _pad: 0,
                    view_seq: r.view_seq,
                };
                w.put_bytes(hdr(row_kind::RECON_GATE, acc, VenueId(v), 0).as_bytes());
                w.put_bytes(h.as_bytes());
                for key in r.keys.iter().take(usize::from(r.n_items).min(64)) {
                    w.put_bytes(key.as_bytes());
                }
                w.add_rows(1);
            }
        }
        for a in 1..=n {
            let acc = AccountId(a as u8);
            for i in 1..s.insts.len() {
                if let Some(l) = s.lev.get(acc, InstId(i as u32)) {
                    let v = s.insts.get(i).map_or(VenueId(0), |f| f.venue);
                    put(
                        w,
                        hdr(row_kind::LEV, acc, v, i as u32),
                        RiskLevRow { leverage: l.0 }.as_bytes(),
                    );
                }
            }
        }
        for a in 1..=n {
            let acc = AccountId(a as u8);
            for v in 0u8..64 {
                let u = m.us_row(acc, VenueId(v));
                if u.us_up_seq != 0 || u.prev_view_seq != 0 || u.uflags != 0 {
                    let row = RiskUsRow {
                        us_up_seq: u.us_up_seq,
                        prev_view_seq: u.prev_view_seq,
                        uflags: u.uflags,
                        _pad: [0; 7],
                    };
                    put(w, hdr(row_kind::USER_STREAM, acc, VenueId(v), 0), row.as_bytes());
                }
            }
        }
        w.begin_section(SECTION_GATES, GATES_VERSION);
        for a in 1..=n {
            let Some(g) = m.gates.get(a) else { continue };
            let mut gf = 0u64;
            if g.selftest {
                gf |= gate_flags::KILL_SELFTEST;
            }
            if m.storage_drain_sent {
                gf |= gate_flags::STORAGE_DRAIN_SENT;
            }
            w.put_row(&GateRow {
                account: a as u8,
                released: u8::from(g.released),
                clean_restart: u8::from(g.clean_restart),
                recovery_flags: g.recovery_flags,
                gate_bits: g.bits,
                reconciled_venues: g.reconciled,
                cfg_ok_venues: g.cfg_ok,
                identity_ok_venues: g.identity_ok,
                conns_ok_venues: g.conns_ok,
                ctl_state_version: m.ctl_state_version,
                standby_ok_venues: g.standby_ok,
                gflags: gf,
            });
        }
        w.begin_section(SECTION_MARGIN, MARGIN_VERSION);
        for a in 1..=n {
            for v in 0u8..64 {
                let Some(st) = s.margin.get(a * 64 + usize::from(v)) else {
                    continue;
                };
                if !st.present {
                    continue;
                }
                w.put_row(&MarginRow {
                    account: a as u8,
                    venue: v,
                    level: st.level,
                    mflags: st.mflags,
                    _pad: 0,
                    report_seq: 0,
                    report_ens: st.report_ens,
                    reported_ratio: st.reported_ratio.0,
                    est_ratio: st.est_ratio.0,
                    rep_liq_dist: st.rep_liq_dist.0,
                    est_liq_dist: st.est_liq_dist.0,
                    l2_since_ens: st.l2_since_ens,
                    deadline_ens: st.deadline_ens,
                    next_force_ens: st.next_force_ens,
                    mark_ens: st.mark_ens,
                });
            }
        }
        w.end_section();
    }

    /// Rebuilds a `Risk` from sections 4, 14, 17, 18 and 23 (missing sections = empty state).
    #[allow(clippy::too_many_arguments)]
    pub fn restore(
        r: &mut SnapshotReader,
        p: &RiskParams,
        rd: &RefData,
        gws: &GwMap,
        caps: &Capacities,
        exec: &[ExecBounds],
        mc: ModeCfg,
    ) -> Result<Risk, RestoreError> {
        let mut risk = Risk::new(p, rd, gws, caps, exec, mc).map_err(|_| RestoreError::BadValue)?;
        let sec = |n: u16, ver: u16| -> Result<Option<hft_strategy_sdk::SectionView<'_>>, RestoreError> {
            let s = r.find_section(n)?;
            if let Some(v) = &s
                && v.version != ver
            {
                return Err(RestoreError::VersionUnsupported);
            }
            Ok(s)
        };
        let s4 = sec(SECTION_RISK_STATE, RISK_STATE_VERSION)?;
        let s14 = sec(SECTION_MODE_TABLE, MODE_TABLE_VERSION)?;
        let s17 = sec(SECTION_RISK_COUNTERS, RISK_COUNTERS_VERSION)?;
        let s18 = sec(SECTION_GATES, GATES_VERSION)?;
        let s23 = sec(SECTION_MARGIN, MARGIN_VERSION)?;
        let n_acc = risk.s.n_accounts;
        let n_insts = risk.s.insts.len().saturating_sub(1);
        let acct_ok = |a: u8| a != 0 && usize::from(a) <= n_acc;

        // Section 4: caps.
        if let Some(s) = &s4 {
            let mut rd4 = s.reader();
            let mut rows = 0usize;
            for _ in 0..s.count {
                let c: CapRecord = rd4.get_row()?;
                if c.source.0 == 0 || c.source.0 > 15 {
                    return Err(RestoreError::BadValue);
                }
                if c.source == CapSource::DELISTING
                    && (c.scope.inst.0 as usize > n_insts
                        || (c.scope.inst.0 != 0 && risk.s.inst(c.scope.inst).is_none()))
                {
                    continue;
                }
                rows += 1;
                if rows > usize::from(caps.max_caps) || !risk.mode.caps.push(c) {
                    return Err(RestoreError::Capacity);
                }
            }
            risk.mode.caps.as_mut_slice().sort_unstable_by_key(|c| {
                (
                    c.source.0,
                    c.scope.strategy.0,
                    c.scope.venue.0,
                    c.scope.account.0,
                    c.scope.group.0,
                    c.scope.inst.0,
                )
            });
        }

        // Section 14: mode table.
        if let Some(s) = &s14 {
            let mut x = s.reader();
            for _ in 0..s.count {
                let row: ModeRow = x.get_row()?;
                let mode = mode_of(row.mode).ok_or(RestoreError::BadValue)?;
                if !acct_ok(row.account) {
                    return Err(RestoreError::BadValue);
                }
                if row.flags & mode_row_flags::DEFAULT != 0 {
                    let d = crate::mode::DefaultRow {
                        strategy: StrategyId(row.strategy),
                        account: AccountId(row.account),
                        mode,
                    };
                    if !risk.mode.defaults.push(d) {
                        return Err(RestoreError::Capacity);
                    }
                } else {
                    let c = crate::mode::CellRow {
                        key: CellKey {
                            strategy: StrategyId(row.strategy),
                            account: AccountId(row.account),
                            group: GroupId(row.group),
                        },
                        desired: mode,
                        landed: row.flags & mode_row_flags::LANDED != 0,
                        eff: Effective::Live,
                        mask: 0,
                    };
                    if !risk.mode.cells.push(c) {
                        return Err(RestoreError::Capacity);
                    }
                }
            }
            risk.mode.cells.as_mut_slice().sort_unstable_by_key(|c| c.key);
            risk.mode
                .defaults
                .as_mut_slice()
                .sort_unstable_by_key(|d| (d.strategy, d.account));
        }

        // Section 17: counters.
        if let Some(s) = &s17 {
            let mut x = s.reader();
            for _ in 0..s.count {
                let h: RiskRowHdr = x.get_row()?;
                let (acc, v) = (AccountId(h.account), VenueId(h.venue));
                if !acct_ok(h.account) || h.venue >= 64 {
                    return Err(RestoreError::BadValue);
                }
                match h.kind {
                    row_kind::ACCT => {
                        let b: RiskAcctRow = x.get_row()?;
                        if let Some(p) = risk.s.pnl.get_mut(usize::from(h.account)) {
                            *p = AcctPnl {
                                day_pnl_base: Notional(b.day_pnl_base),
                                loss_trigger: Notional(b.loss_trigger),
                                loss_quota: Notional(b.loss_quota),
                                equity_sod: Notional(b.equity_sod),
                                resumes_today: b.resumes_today,
                                eq_flags: b.eq_flags,
                                latched: false,
                            };
                        }
                    }
                    row_kind::BUCKET => {
                        let b: RiskBucketRow = x.get_row()?;
                        if h.inst == 0 {
                            if let Some(t) = risk.s.buckets.venue.get_mut(Buckets::vix(acc, v)) {
                                t.tokens_10s = b.tokens_10s;
                                t.tokens_min = b.tokens_min;
                                t.last_ens = b.last_ens;
                                t.backoff_until_ens = b.backoff_until_ens;
                                t.backoff_k = b.backoff_k;
                            }
                        } else if (h.inst as usize) <= n_insts {
                            let ix = risk.s.buckets.iix(acc, InstId(h.inst));
                            if let Some(t) = risk.s.buckets.inst.get_mut(ix) {
                                t.tokens = b.tokens_10s;
                                t.last_ens = b.last_ens;
                            }
                        }
                    }
                    row_kind::RECON => {
                        let b: ReconRow = x.get_row()?;
                        if h.inst == 0 {
                            risk.s.recon.set_streak(acc, v, b.ignore_streak);
                        } else if !risk.s.recon.keys.push(ReconKeyRow {
                            account: acc,
                            venue: v,
                            key: h.inst,
                            warn_streak: b.warn_streak,
                            last_diff: Qty(b.last_diff),
                            seen: false,
                        }) {
                            return Err(RestoreError::Capacity);
                        }
                    }
                    row_kind::RECON_GATE => {
                        let g: ReconGateRowHdr = x.get_row()?;
                        if g.n_items > 64 && g.flags & crate::mode::recon_flags::OVERFLOW == 0 {
                            return Err(RestoreError::BadValue);
                        }
                        let k = usize::from(g.n_items).min(64);
                        let mut row = crate::mode::ReconGateRow {
                            present: true,
                            report_id: g.report_id,
                            disposed_mask: g.disposed_mask,
                            n_items: g.n_items,
                            flags: g.flags,
                            view_seq: g.view_seq,
                            ..crate::mode::ReconGateRow::default()
                        };
                        for slot in row.keys.iter_mut().take(k) {
                            let b = x.get_bytes(8)?;
                            *slot = ReconItemKey::read_from_bytes(b).map_err(|_| RestoreError::Truncated)?;
                        }
                        if let Some(t) = risk
                            .mode
                            .recon
                            .get_mut(usize::from(h.account) * 64 + usize::from(h.venue))
                        {
                            *t = row;
                        }
                    }
                    row_kind::LEV => {
                        let b: RiskLevRow = x.get_row()?;
                        if (h.inst as usize) <= n_insts {
                            risk.s.lev.set(acc, InstId(h.inst), Rate(b.leverage));
                        }
                    }
                    row_kind::USER_STREAM => {
                        let b: RiskUsRow = x.get_row()?;
                        if let Some(u) = risk.mode.us_row_mut(acc, v) {
                            u.us_up_seq = b.us_up_seq;
                            u.prev_view_seq = b.prev_view_seq;
                            u.uflags = b.uflags;
                        }
                    }
                    _ => return Err(RestoreError::BadValue),
                }
            }
            risk.s
                .recon
                .keys
                .as_mut_slice()
                .sort_unstable_by_key(|k| (k.account, k.venue, k.key));
        }

        // Section 18: gates.
        if let Some(s) = &s18 {
            let mut x = s.reader();
            for _ in 0..s.count {
                let g: GateRow = x.get_row()?;
                if !acct_ok(g.account)
                    || g.released > 1
                    || g.clean_restart > 1
                    || g.gate_bits > crate::mode::gate_bits::ALL
                {
                    return Err(RestoreError::BadValue);
                }
                risk.mode.ctl_state_version = g.ctl_state_version;
                risk.mode.storage_drain_sent = g.gflags & gate_flags::STORAGE_DRAIN_SENT != 0;
                if let Some(t) = risk.mode.gates.get_mut(usize::from(g.account)) {
                    *t = crate::mode::GateState {
                        bits: g.gate_bits,
                        released: g.released != 0,
                        clean_restart: g.clean_restart != 0,
                        recovery_flags: g.recovery_flags,
                        reconciled: g.reconciled_venues,
                        cfg_ok: g.cfg_ok_venues,
                        identity_ok: g.identity_ok_venues,
                        conns_ok: g.conns_ok_venues,
                        standby_ok: g.standby_ok_venues,
                        selftest: g.gflags & gate_flags::KILL_SELFTEST != 0,
                    };
                }
            }
        }

        // Section 23: margin.
        if let Some(s) = &s23 {
            let mut x = s.reader();
            for _ in 0..s.count {
                let m: MarginRow = x.get_row()?;
                if !acct_ok(m.account) || m.venue >= 64 || m.level > 3 {
                    return Err(RestoreError::BadValue);
                }
                let lim = risk.s.lim().acct(AccountId(m.account)).margin;
                if let Some(t) = risk
                    .s
                    .margin
                    .get_mut(usize::from(m.account) * 64 + usize::from(m.venue))
                {
                    *t = MState {
                        present: true,
                        level: m.level,
                        mflags: m.mflags,
                        report_ens: m.report_ens,
                        reported_ratio: Rate(m.reported_ratio),
                        est_ratio: Rate(m.est_ratio),
                        rep_liq_dist: Rate(m.rep_liq_dist),
                        est_liq_dist: Rate(m.est_liq_dist),
                        l2_since_ens: m.l2_since_ens,
                        deadline_ens: m.deadline_ens,
                        next_force_ens: m.next_force_ens,
                        mark_ens: m.mark_ens,
                        stale_due_ens: 0,
                    };
                    // The `MarginStale` deadline in the heap is derived, not stored.
                    t.stale_due_ens = t.stale_deadline(&lim);
                }
            }
        }

        // Derived state.
        risk.mode.recompute_caps();
        risk.mode.recompute_effective();
        risk.mode.caps_dirty = false;
        risk.mode.table_dirty = false;
        for a in 1..=n_acc {
            let acc = AccountId(a as u8);
            let has = risk
                .mode
                .cap(CapSource::DAILY_LOSS, &crate::mode::account_scope(acc))
                .is_some();
            if let Some(p) = risk.s.pnl.get_mut(a) {
                p.latched = has;
            }
        }
        Ok(risk)
    }
}
