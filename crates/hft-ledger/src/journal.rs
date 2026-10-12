//! `JournalLedger`: the journal cold consumer (reconciler engine view, hft-py). It rebuilds the
//! calls the sequencer made from journal records and books them through the same `Ledger`
//! (README "冷消费者").

use hft_event::{
    BalanceReport, Bbo, CmdKind, CmdResult, ControlCmd, EngineClock, EventKind, ExternalFill, Funding,
    FundingSettle, MarginReportHdr, MarkPrice, Note, NoteSource, OrderAction, OrderDecision, OrderEvent,
    OrderEventKind, PartHdr, ReconcileReport, RecordFlags, RecordView, SnapshotHdr, SysTimer, Timer,
    TimerCode,
};
use hft_refdata::{AccountKind, FundingSettleKind, RefData, SimBalance};
use hft_strategy_sdk::{RestoreError, SnapshotReader};
use hft_types::{
    ASSET_NONE, AccountId, AssetId, GroupId, INST_NONE, InstId, Notional, Px, Qty, RoundMode, Side, VenueId,
};
use zerocopy::FromBytes;

use crate::config::{Capacities, LedgerConfig};
use crate::fixed::{div_round, mix64, sat64, words_i128};
use crate::ledger::Ledger;
use crate::margin::MarginReportView;
use crate::notes::code;
use crate::types::*;

/// Slot-map capacity (≥ Σ OMS table slots × (`tombstone_gens` + 1) of any supported host class).
pub const SLOT_MAP_CAP: usize = 1 << 16;
/// hft-oms snapshot section 2 (`OrderSlots`) and its row length (row versions 2/3; version 1 rows
/// are 160 B and have no `SlotExt.fee_cum`).
pub const OMS_SECTION_SLOTS: u16 = 2;
pub const OMS_SLOT_ROW_LEN: usize = 192;

/// Counters of the cold consumer.
#[derive(Copy, Clone, Eq, PartialEq, Debug, Default)]
pub struct JournalLedgerStats {
    /// Fill reports whose `(venue, account, epoch, slot, gen)` had no registered order (not booked).
    pub unmapped_fills: u64,
    /// Registrations evicted because the slot map was full.
    pub map_evicted: u64,
    pub epochs_seen: u64,
}

#[derive(Copy, Clone, Debug)]
struct SlotEntry {
    epoch: u16,
    slot: u32,
    r#gen: u32,
    account: AccountId,
    venue: VenueId,
    inst: InstId,
    side: Side,
    group: GroupId,
    order_px: Px,
    cum: i64,
    /// Σ px × qty of the booked part (inst `px_exp + qty_exp`).
    notional: i128,
    /// `cum_filled` of the last `PartialFill`/`Fill` (hft-oms `SlotExt.fee_cum`).
    fee_cum: i64,
    cash_qty: i64,
    cash_sum: i64,
    tomb: bool,
}

#[derive(Copy, Clone, Eq, PartialEq, Debug)]
struct SlotKey {
    venue: VenueId,
    account: AccountId,
    epoch: u16,
    slot: u32,
    r#gen: u32,
}

impl SlotEntry {
    fn key(&self) -> SlotKey {
        SlotKey {
            venue: self.venue,
            account: self.account,
            epoch: self.epoch,
            slot: self.slot,
            r#gen: self.r#gen,
        }
    }
}

/// FIFO slot map with an open-addressing index.
#[derive(Clone, Debug)]
struct SlotMap {
    cap: usize,
    ring: Vec<SlotEntry>,
    head: usize,
    len: usize,
    index: Vec<u32>,
}

const EMPTY: u32 = 0;
const TOMB: u32 = u32::MAX;

impl SlotMap {
    fn new(cap: usize) -> SlotMap {
        SlotMap {
            cap,
            ring: Vec::new(),
            head: 0,
            len: 0,
            index: vec![EMPTY; (cap * 2).next_power_of_two()],
        }
    }

    fn h(key: SlotKey) -> usize {
        let table_epoch_slot = u64::from(key.venue.0) << 56
            | u64::from(key.account.0) << 48
            | u64::from(key.epoch) << 32
            | u64::from(key.slot);
        mix64(table_epoch_slot ^ mix64(u64::from(key.r#gen))) as usize
    }

    fn find(&self, key: SlotKey) -> Option<usize> {
        let mask = self.index.len() - 1;
        let mut i = Self::h(key) & mask;
        for _ in 0..self.index.len() {
            match self.index.get(i).copied().unwrap_or(EMPTY) {
                EMPTY => return None,
                TOMB => {}
                v => {
                    let e = self.ring.get(v as usize - 1)?;
                    if e.key() == key {
                        return Some(i);
                    }
                }
            }
            i = (i + 1) & mask;
        }
        None
    }

    fn get_mut(&mut self, key: SlotKey) -> Option<&mut SlotEntry> {
        let i = self.find(key)?;
        let v = *self.index.get(i)?;
        self.ring.get_mut(v as usize - 1)
    }

    fn place(&mut self, e: &SlotEntry, pos: usize) {
        let mask = self.index.len() - 1;
        let mut i = Self::h(e.key()) & mask;
        while !matches!(self.index.get(i).copied(), Some(EMPTY) | Some(TOMB) | None) {
            i = (i + 1) & mask;
        }
        if let Some(s) = self.index.get_mut(i) {
            *s = pos as u32 + 1;
        }
    }

    /// Registers `e`; returns whether an older entry was evicted.
    fn insert(&mut self, e: SlotEntry) -> bool {
        if let Some(i) = self.find(e.key()) {
            if let Some(v) = self.index.get(i).copied()
                && let Some(x) = self.ring.get_mut(v as usize - 1)
            {
                *x = e;
            }
            return false;
        }
        let mut evicted = false;
        let pos = if self.len < self.cap {
            let pos = (self.head + self.len) % self.cap;
            if pos == self.ring.len() {
                self.ring.push(e);
            } else if let Some(x) = self.ring.get_mut(pos) {
                *x = e;
            }
            self.len += 1;
            pos
        } else {
            let pos = self.head;
            if let Some(old) = self.ring.get(pos).copied()
                && let Some(i) = self.find(old.key())
                && let Some(s) = self.index.get_mut(i)
            {
                *s = TOMB;
            }
            if let Some(x) = self.ring.get_mut(pos) {
                *x = e;
            }
            self.head = (self.head + 1) % self.cap;
            evicted = true;
            pos
        };
        self.place(&e, pos);
        evicted
    }
}

#[derive(Copy, Clone, Debug)]
struct Clock {
    base: i64,
    hz_milli: u64,
    tsc: u64,
}

#[derive(Copy, Clone, Debug)]
enum PendingCmd {
    Transfer(TransferDelta),
    SimReset(AccountId),
    Baseline(AccountId, VenueId),
}

/// The journal cold consumer.
#[derive(Clone, Debug)]
pub struct JournalLedger {
    ledger: Ledger,
    map: SlotMap,
    epoch: u16,
    stats: JournalLedgerStats,
    clock: Option<Clock>,
    last_t: i64,
    failed: Option<JournalApplyError>,
    report_id: Option<u64>,
    cmd: Option<(u64, PendingCmd)>,
    sims: Vec<(AccountId, Vec<SimBalance>)>,
}

impl JournalLedger {
    /// Starts from a `StateSnapshot`: the ledger from this crate's sections, the slot map from
    /// hft-oms section 2 (non-free slots).
    pub fn from_snapshot(
        r: &mut SnapshotReader<'_>,
        rd: &RefData,
        cfg: &LedgerConfig,
        caps: &Capacities,
    ) -> Result<JournalLedger, RestoreError> {
        let ledger = Ledger::restore(r, rd, cfg, caps)?;
        let mut map = SlotMap::new(SLOT_MAP_CAP);
        if let Some(s) = r.find_section(OMS_SECTION_SLOTS)? {
            let len = match s.version {
                1 => 160,
                2 | 3 => OMS_SLOT_ROW_LEN,
                _ => return Err(RestoreError::VersionUnsupported),
            };
            for k in 0..s.count as usize {
                let row = s
                    .body
                    .get(k * len..(k + 1) * len)
                    .ok_or(RestoreError::Truncated)?;
                let u8_at = |o: usize| row.get(o).copied().unwrap_or(0);
                let u16_at = |o: usize| u16::from_le_bytes([u8_at(o), u8_at(o + 1)]);
                let u32_at =
                    |o: usize| u32::from_le_bytes([u8_at(o), u8_at(o + 1), u8_at(o + 2), u8_at(o + 3)]);
                let u64_at = |o: usize| u64::from(u32_at(o)) | u64::from(u32_at(o + 4)) << 32;
                let side = match u8_at(1) {
                    1 => Side::Buy,
                    2 => Side::Sell,
                    _ => return Err(RestoreError::BadValue),
                };
                let entry = SlotEntry {
                    epoch: u16_at(22),
                    slot: u32_at(12),
                    r#gen: u32_at(8),
                    account: AccountId(u8_at(7)),
                    venue: VenueId(u8_at(6)),
                    inst: InstId(u32_at(16)),
                    side,
                    group: GroupId(u32_at(24)),
                    order_px: Px(u64_at(48) as i64),
                    cum: u64_at(40) as i64,
                    notional: words_i128([u64_at(64), u64_at(72)]),
                    fee_cum: if s.version == 1 {
                        u64_at(40) as i64
                    } else {
                        u64_at(160) as i64
                    },
                    cash_qty: if s.version >= 3 { u64_at(168) as i64 } else { 0 },
                    cash_sum: if s.version >= 3 { u64_at(176) as i64 } else { 0 },
                    tomb: false,
                };
                validate_cursor(&ledger, &entry)?;
                if map.find(entry.key()).is_some() {
                    return Err(RestoreError::BadValue);
                }
                if map.insert(entry) {
                    return Err(RestoreError::BadValue);
                }
            }
        }
        // Recycled generations must retain their cash watermark too. OMS section 12 has
        // an 8-byte row header; non-tomb rows do not belong to this consumer.
        if let Some(section) = r.find_section(12)? {
            if !(1..=2).contains(&section.version) {
                return Err(RestoreError::VersionUnsupported);
            }
            let mut rr = section.reader();
            for _ in 0..section.count {
                let kind = rr.get_u8()?;
                let pad = rr.get_bytes(3)?;
                if pad != [0; 3] {
                    return Err(RestoreError::BadValue);
                }
                let len = rr.get_u32()? as usize;
                let row = rr.get_bytes(len)?;
                if kind != 4 {
                    continue;
                }
                let want = if section.version == 1 { 64 } else { 80 };
                if row.len() != want {
                    return Err(RestoreError::BadValue);
                }
                let u8_at = |o: usize| row.get(o).copied().unwrap_or(0);
                let u16_at = |o: usize| u16::from_le_bytes([u8_at(o), u8_at(o + 1)]);
                let u32_at =
                    |o: usize| u32::from_le_bytes([u8_at(o), u8_at(o + 1), u8_at(o + 2), u8_at(o + 3)]);
                let u64_at = |o: usize| u64::from(u32_at(o)) | u64::from(u32_at(o + 4)) << 32;
                let side = match u8_at(24) {
                    1 => Side::Buy,
                    2 => Side::Sell,
                    _ => return Err(RestoreError::BadValue),
                };
                let entry = SlotEntry {
                    epoch: u16_at(12),
                    slot: u32_at(4),
                    r#gen: u32_at(8),
                    account: AccountId(u8_at(1)),
                    venue: VenueId(u8_at(0)),
                    inst: InstId(u32_at(16)),
                    side,
                    group: GroupId(u32_at(20)),
                    order_px: Px(0),
                    cum: u64_at(32) as i64,
                    notional: 0,
                    fee_cum: u64_at(32) as i64,
                    cash_qty: if section.version == 2 {
                        u64_at(64) as i64
                    } else {
                        0
                    },
                    cash_sum: if section.version == 2 {
                        u64_at(72) as i64
                    } else {
                        0
                    },
                    tomb: true,
                };
                validate_cursor(&ledger, &entry)?;
                if map.find(entry.key()).is_some() {
                    return Err(RestoreError::BadValue);
                }
                if map.insert(entry) {
                    return Err(RestoreError::BadValue);
                }
            }
        }
        // A keyframe may be followed by new orders before the next EpochStart/EngineClock.
        // Their identity and time must come from the same committed snapshot.
        let epoch = if let Some(section) = r.find_section(3)? {
            if section.version != 1 {
                return Err(RestoreError::VersionUnsupported);
            }
            if section.body.len() < 16 {
                return Err(RestoreError::Truncated);
            }
            (section.reader().get_u32()? & 0x7fff) as u16
        } else {
            0
        };
        let clock = if let Some(section) = r.find_section(25)? {
            if section.version != 1 {
                return Err(RestoreError::VersionUnsupported);
            }
            if section.count != 1 || section.body.len() != 32 {
                return Err(RestoreError::BadValue);
            }
            let mut rr = section.reader();
            let base = rr.get_i64()?;
            let tsc = rr.get_u64()?;
            let hz_milli = rr.get_u64()?;
            let clock_seq = rr.get_u64()?;
            if hz_milli == 0 {
                // The initial Boot snapshot precedes EngineClock. Only its all-zero
                // anchor is unstarted; a partial or corrupt zero-frequency row is invalid.
                if base != 0 || tsc != 0 || clock_seq != 0 {
                    return Err(RestoreError::BadValue);
                }
                None
            } else {
                Some(Clock { base, tsc, hz_milli })
            }
        } else {
            None
        };
        let last_t = ledger.now;
        let mut sims = Vec::new();
        let mut a = 1u8;
        while rd.account(AccountId(a)).id == AccountId(a) {
            if let AccountKind::Sim(c) = &rd.account(AccountId(a)).kind {
                sims.push((AccountId(a), c.initial_balances.to_vec()));
            }
            a = a.wrapping_add(1);
            if a == 0 {
                break;
            }
        }
        Ok(JournalLedger {
            ledger,
            map,
            epoch,
            stats: JournalLedgerStats::default(),
            clock,
            last_t,
            failed: None,
            report_id: None,
            cmd: None,
            sims,
        })
    }

    /// Positions, inventories, balances and PnL (the `Ledger` read API).
    pub fn ledger(&self) -> &Ledger {
        &self.ledger
    }

    /// Mutable access (groups and Shadow cells from the journal's `SetParams`/mode changes).
    pub fn ledger_mut(&mut self) -> &mut Ledger {
        &mut self.ledger
    }

    pub fn stats(&self) -> &JournalLedgerStats {
        &self.stats
    }

    /// Engine time of a record (event-model "引擎时间"); before any `EngineClock` the last value.
    fn t_of(&self, tsc: u64) -> i64 {
        match self.clock {
            Some(c) if c.hz_milli > 0 => {
                let d = i128::from(tsc.wrapping_sub(c.tsc) as i64).max(0) as u128;
                let ns = d.saturating_mul(1_000_000_000_000) / u128::from(c.hz_milli);
                c.base.saturating_add(i64::try_from(ns).unwrap_or(i64::MAX))
            }
            _ => self.last_t,
        }
    }

    /// Feeds one journal record (Input, Output or Meta; unrelated kinds are ignored). Records of
    /// one event are fed in journal order.
    pub fn apply(&mut self, rec: &RecordView<'_>) -> Result<(), JournalApplyError> {
        if let Some(error) = self.failed {
            return Err(error);
        }
        let result = self.apply_record(rec);
        if let Err(error) = result {
            self.failed = Some(error);
        }
        result
    }

    fn apply_record(&mut self, rec: &RecordView<'_>) -> Result<(), JournalApplyError> {
        let t = self.t_of(rec.hdr.tsc);
        self.last_t = t;
        let kind = EventKind(rec.hdr.kind);
        let flags = RecordFlags(rec.hdr.flags);
        match kind {
            EventKind::STATE_SNAPSHOT => {
                // SnapshotPurpose::Recovery = 4 (hft-event's wire registry). Ordinary
                // keyframes/rolls do not restart the ledger's process-local read views.
                if let Ok(s) = rec.read::<SnapshotHdr>()
                    && s.purpose == 4
                {
                    self.ledger.restore_read_views();
                }
            }
            EventKind::ENGINE_CLOCK => {
                if let Ok(c) = rec.read::<EngineClock>() {
                    self.clock = Some(Clock {
                        base: c.base_engine_ns,
                        hz_milli: c.tsc_hz_milli,
                        tsc: rec.hdr.tsc,
                    });
                    self.last_t = c.base_engine_ns;
                    self.ledger.on_run_start(c.base_engine_ns);
                }
            }
            EventKind::NOTE => {
                if let Ok(n) = rec.read::<Note>() {
                    let cash_inst = match (n.source, n.code) {
                        (NoteSource::Oms, code::OMS_FOREIGN_FILL) => Some(n.inst),
                        (NoteSource::Ledger, code::EXTERNAL_FILL)
                            if (n.v[0] >> 24) & aflags::AUTO_RECON != 0 =>
                        {
                            Some(InstId(n.v[1] as u32))
                        }
                        _ => None,
                    };
                    if cash_inst
                        .and_then(|i| self.ledger.inst(i))
                        .is_some_and(|i| i.perp_cash)
                    {
                        return Err(JournalApplyError::UnsupportedExternalCash);
                    }
                    self.on_note(&n, t);
                }
            }
            EventKind::ORDER_DECISION => {
                if let Ok(d) = rec.read::<OrderDecision>() {
                    self.on_decision(&d, flags)?;
                }
            }
            EventKind::ORDER_EVENT => {
                if let Ok(e) = rec.read::<OrderEvent>() {
                    self.on_order_event(&e, rec.hdr.seq, t)?;
                }
            }
            EventKind::EXTERNAL_FILL => {
                if let Ok(e) = rec.read::<ExternalFill>() {
                    if self.ledger.venue_s(e.venue).is_some_and(|v| v.perp_cash)
                        || self.ledger.inst(e.inst).is_some_and(|i| i.perp_cash)
                    {
                        self.ledger
                            .apply_settled_external(&e, t)
                            .map_err(JournalApplyError::Settlement)?;
                    } else {
                        self.ledger.apply_external(&e, false, t);
                    }
                }
            }
            EventKind::RECONCILE_REPORT => {
                if let Ok(r) = rec.read::<ReconcileReport>() {
                    self.report_id = Some(r.report_id);
                }
            }
            EventKind::FUNDING_SETTLE => {
                if let Ok(f) = rec.read::<FundingSettle>() {
                    let kind = self.ledger.venue_s(f.venue).map(|v| v.funding);
                    if kind == Some(FundingSettleKind::BalanceOnlyPush) && f.synth == 0 && f.inst == INST_NONE
                    {
                        self.ledger.apply_funding_push(&f, t);
                    } else {
                        self.ledger.apply_funding(&f, t);
                    }
                }
            }
            EventKind::BALANCE_REPORT => {
                if let Ok(b) = rec.read::<BalanceReport>() {
                    self.ledger.on_balance(&b, false, t);
                }
            }
            EventKind::MARGIN_REPORT => {
                let size = size_of::<MarginReportHdr>();
                if let Some(pb) = rec.body.get(..size)
                    && let Ok(h) = MarginReportHdr::read_from_bytes(pb)
                {
                    let n = usize::from(h.n) * 16;
                    let items = rec.body.get(size..size + n).unwrap_or(&[]);
                    self.ledger.on_margin(&MarginReportView { hdr: h, items }, t);
                }
            }
            EventKind::BBO => {
                if let Ok(b) = rec.read::<Bbo>() {
                    self.ledger.on_ref_px(b.h.inst, b.bid_px, b.ask_px, t);
                }
            }
            EventKind::MARK_PRICE => {
                if let Ok(m) = rec.read::<MarkPrice>() {
                    let mut touched = crate::fixed::FixedVec::new();
                    self.ledger.on_mark(m.h.inst, m.px, t, &mut touched);
                }
            }
            EventKind::FUNDING => {
                if let Ok(f) = rec.read::<Funding>() {
                    self.ledger.on_funding_interval(f.h.inst, f.interval_s);
                }
            }
            EventKind::TIMER => {
                if let Ok(x) = rec.read::<Timer>()
                    && let Ok(TimerCode::Sys(SysTimer::DayRoll { .. })) =
                        TimerCode::decode(x.owner, x.timer_id, x.group)
                {
                    self.ledger.on_day_roll(t);
                }
            }
            EventKind::CONTROL_CMD => self.on_cmd(rec),
            EventKind::CMD_RESULT => {
                if let Ok(r) = rec.read::<CmdResult>() {
                    self.on_cmd_result(&r, t);
                }
            }
            _ => {}
        }
        if flags.contains(RecordFlags::EVENT_END) {
            self.report_id = None;
        }
        self.ledger.clear_notes();
        Ok(())
    }

    fn on_note(&mut self, n: &Note, t: i64) {
        match (n.source, n.code) {
            (NoteSource::Sequencer, code::EPOCH_START) => {
                self.epoch = (n.v[0] & 0x7FFF) as u16;
                self.stats.epochs_seen = self.stats.epochs_seen.saturating_add(1);
            }
            (NoteSource::Oms, code::OMS_FOREIGN_FILL) => {
                if let Some(e) = Ledger::external_from_oms_note(n) {
                    self.ledger.apply_external(&e, false, t);
                }
            }
            (NoteSource::Ledger, code::EXTERNAL_FILL) => {
                let aflags = (n.v[0] >> 24) & 0xFF;
                if aflags & aflags::AUTO_RECON == 0 {
                    return;
                }
                let Some(rid) = self.report_id else { return };
                let venue = VenueId((n.v[0] & 0xFF) as u8);
                let account = AccountId(((n.v[0] >> 8) & 0xFF) as u8);
                let qty = n.v[3] as i64;
                let d = if n.v[2] == Side::Sell as u64 {
                    qty
                } else {
                    qty.saturating_neg()
                };
                let e = Ledger::auto_external_fill(
                    account,
                    venue,
                    InstId(n.v[1] as u32),
                    Qty(d),
                    Px(n.v[4] as i64),
                    rid,
                );
                self.ledger.apply_external(&e, true, t);
            }
            (NoteSource::Ledger, code::ADJUSTED) => {
                let kind = (n.v[6] & 0xFF) as u8;
                let a = AdjustDelta {
                    account: AccountId((n.v[0] & 0xFF) as u8),
                    venue: VenueId(((n.v[0] >> 8) & 0xFF) as u8),
                    kind,
                    px_src: ((n.v[6] >> 8) & 0xFF) as u8,
                    inst: n.inst,
                    asset: if kind == AdjustKind::Balance as u8 {
                        AssetId(n.v[1] as u16)
                    } else {
                        ASSET_NONE
                    },
                    _pad: 0,
                    group: n.group,
                    qty: Qty(n.v[3] as i64),
                    px: Px(n.v[4] as i64),
                    report_id: n.v[2],
                    cmd_seq: n.v[7],
                    settle_ts_ns: 0,
                };
                self.ledger.apply_adjust(&a, t);
            }
            (NoteSource::Ledger, code::FUNDING_CORRECTED) => {
                let flags = n.v[7];
                if flags & fc_flags::NO_HISTORY != 0 && flags & fc_flags::WINDOW_END == 0 {
                    return;
                }
                let venue = VenueId((n.v[0] & 0xFF) as u8);
                let account = AccountId(((n.v[0] >> 8) & 0xFF) as u8);
                let diff = Notional(n.v[5] as i64);
                let a = AdjustDelta {
                    account,
                    venue,
                    kind: AdjustKind::Funding as u8,
                    px_src: 0,
                    inst: InstId(n.v[1] as u32),
                    asset: self.ledger.settle_asset(venue),
                    _pad: 0,
                    group: GroupId(n.v[6] as u32),
                    qty: Qty(self.ledger.notional_to_asset(venue, diff)),
                    px: Px(0),
                    report_id: 0,
                    cmd_seq: 0,
                    settle_ts_ns: n.v[2] as i64,
                };
                self.ledger.apply_adjust(&a, t);
            }
            (NoteSource::Ledger, code::RESIDUAL_WRITTEN_OFF) => {
                let a = AdjustDelta {
                    account: AccountId((n.v[0] & 0xFF) as u8),
                    venue: VenueId(((n.v[0] >> 8) & 0xFF) as u8),
                    kind: AdjustKind::WriteOff as u8,
                    px_src: 0,
                    inst: InstId(n.v[2] as u32),
                    asset: ASSET_NONE,
                    _pad: 0,
                    group: GroupId(n.v[1] as u32),
                    qty: Qty(n.v[3] as i64),
                    px: Px(0),
                    report_id: 0,
                    cmd_seq: n.v[5],
                    settle_ts_ns: 0,
                };
                self.ledger.apply_adjust(&a, t);
            }
            _ => {}
        }
    }

    fn on_decision(&mut self, d: &OrderDecision, flags: RecordFlags) -> Result<(), JournalApplyError> {
        if d.action != OrderAction::New || d.reject != 0 || flags.contains(RecordFlags::SHADOW) {
            return Ok(());
        }
        let entry = SlotEntry {
            epoch: self.epoch,
            slot: d.slot.0,
            r#gen: d.r#gen,
            account: d.account,
            venue: d.venue,
            inst: d.inst,
            side: d.side,
            group: d.group,
            order_px: d.px,
            cum: 0,
            notional: 0,
            fee_cum: 0,
            cash_qty: 0,
            cash_sum: 0,
            tomb: false,
        };
        if self.ledger.inst(d.inst).is_some_and(|i| i.perp_cash) && self.map.find(entry.key()).is_some() {
            return Err(JournalApplyError::Identity);
        }
        if self.map.insert(entry) {
            self.stats.map_evicted = self.stats.map_evicted.saturating_add(1);
        }
        Ok(())
    }

    /// `seq` = the `OrderEvent`'s seq: `FillDelta.seq`/`FeeDelta.seq` as the engine sets them (OMS
    /// `FillOut.seq`/`FeeOut` of the same Input; the round's `open_seq` comes from it).
    fn on_order_event(&mut self, ev: &OrderEvent, seq: u64, t: i64) -> Result<(), JournalApplyError> {
        let key = SlotKey {
            venue: ev.venue,
            account: ev.account,
            epoch: ev.epoch,
            slot: ev.slot.0,
            r#gen: ev.r#gen,
        };
        let Some(e) = self.map.get_mut(key).copied() else {
            if self.ledger.inst(ev.inst).is_some_and(|i| i.perp_cash)
                && (ev.cum_filled.0 != 0
                    || ev.cash_qty.0 != 0
                    || ev.cash_sum.0 != 0
                    || ev.what == OrderEventKind::FeeOnly)
            {
                return Err(JournalApplyError::Unmapped);
            }
            if ev.cum_filled.0 > 0 && ev.what != OrderEventKind::FeeOnly {
                self.stats.unmapped_fills = self.stats.unmapped_fills.saturating_add(1);
            }
            return Ok(());
        };
        let s = self
            .ledger
            .inst(ev.inst)
            .copied()
            .ok_or(JournalApplyError::Identity)?;
        if e.inst != ev.inst || s.venue != e.venue {
            return Err(JournalApplyError::Identity);
        }
        let cash = cash_increment(s.perp_cash, &e, ev)?;
        let fee = FeeDelta {
            account: e.account,
            venue: e.venue,
            inst: e.inst,
            group: e.group,
            side: e.side,
            fee: ev.fee.0,
            fee_asset: ev.fee_asset,
            seq,
        };
        if ev.what == OrderEventKind::FeeOnly {
            self.ledger.apply_fee(&fee, t);
            return Ok(());
        }
        // These reports never carry a booked fill in the OMS state machine.
        if !e.tomb
            && matches!(
                ev.what,
                OrderEventKind::Ack | OrderEventKind::Reject | OrderEventKind::OutcomeUnknown
            )
        {
            return Ok(());
        }
        let fill_report = matches!(ev.what, OrderEventKind::PartialFill | OrderEventKind::Fill);
        let fresh_fee = fill_report && ev.cum_filled.0 > e.fee_cum;
        let d = ev.cum_filled.0.saturating_sub(e.cum);
        if d <= 0 {
            if !e.tomb && fresh_fee && ev.fee.0 != 0 {
                self.ledger.apply_fee(&fee, t);
            }
            if fresh_fee && let Some(x) = self.map.get_mut(key) {
                x.fee_cum = ev.cum_filled.0;
            }
            return Ok(());
        }
        let fallback = if ev.what == OrderEventKind::QueryResult {
            e.order_px
        } else {
            ev.px
        };
        let total = if ev.avg_px.0 != 0 {
            i128::from(ev.avg_px.0) * i128::from(ev.cum_filled.0)
        } else {
            e.notional + i128::from(fallback.0) * i128::from(d)
        };
        let px = if e.tomb {
            if ev.px.0 != 0 { ev.px } else { ev.avg_px }
        } else {
            Px(sat64(div_round(
                total - e.notional,
                i128::from(d),
                RoundMode::HalfEven,
            )))
        };
        let fee_base = if !s.perp && ev.fee_asset == s.base {
            ev.fee.0
        } else {
            0
        };
        let net = if s.perp {
            d
        } else if e.side == Side::Buy {
            d.saturating_sub(fee_base)
        } else {
            d.saturating_add(fee_base)
        };
        let fd = FillDelta {
            account: e.account,
            venue: e.venue,
            side: e.side,
            class: FillClass::Open as u8,
            flags: 0,
            leg_ix: u8::MAX,
            _pad: [0; 2],
            inst: e.inst,
            group: e.group,
            qty: Qty(d),
            net_qty: Qty(net),
            px,
            fee: ev.fee.0,
            fee_asset: ev.fee_asset,
            _pad2: 0,
            slot: ev.slot.0,
            r#gen: ev.r#gen,
            _pad3: 0,
            seq,
        };
        if let Some(amount) = cash {
            self.ledger
                .apply_settled_fill(
                    &fd,
                    PerpCashSettlement {
                        asset: s.quote,
                        amount,
                    },
                    t,
                )
                .map_err(JournalApplyError::Settlement)?;
        } else {
            self.ledger.apply_fill(&fd, t);
        }
        // The watermark follows a successful booking, never the other way around.
        if let Some(x) = self.map.get_mut(key) {
            x.cum = ev.cum_filled.0;
            x.notional = total;
            if fresh_fee {
                x.fee_cum = ev.cum_filled.0;
            }
            if cash.is_some() {
                x.cash_qty = ev.cash_qty.0;
                x.cash_sum = ev.cash_sum.0;
            }
        }
        Ok(())
    }

    fn on_cmd(&mut self, rec: &RecordView<'_>) {
        let Some(ph) = rec.body.get(..16).and_then(|b| PartHdr::read_from_bytes(b).ok()) else {
            return;
        };
        if ph.parts != 1 {
            return;
        }
        let Some(logical) = rec.body.get(16..16 + ph.chunk_len as usize) else {
            return;
        };
        let Ok((_, cmd)) = ControlCmd::decode(logical) else {
            return;
        };
        let p = match cmd {
            ControlCmd::RecordTransfer {
                account,
                from,
                to,
                asset,
                amount,
                ..
            } => Some(PendingCmd::Transfer(TransferDelta {
                account,
                from_venue: from,
                to_venue: to,
                asset,
                amount,
                kind: TransferKind::Record,
            })),
            ControlCmd::SimTransfer {
                account,
                from,
                to,
                asset,
                amount,
            } => Some(PendingCmd::Transfer(TransferDelta {
                account,
                from_venue: from,
                to_venue: to,
                asset,
                amount,
                kind: TransferKind::Sim,
            })),
            ControlCmd::SimReset { account } => Some(PendingCmd::SimReset(account)),
            ControlCmd::SetBalanceBaseline { account, venue } => Some(PendingCmd::Baseline(account, venue)),
            _ => None,
        };
        self.cmd = p.map(|p| (rec.hdr.seq, p));
    }

    fn on_cmd_result(&mut self, r: &CmdResult, t: i64) {
        let Some((seq, p)) = self.cmd else { return };
        if r.cmd_seq != seq {
            return;
        }
        self.cmd = None;
        if r.status != 0 {
            return;
        }
        let _ = CmdKind::SIM_RESET;
        match p {
            PendingCmd::Transfer(tr) => self.ledger.apply_transfer(&tr, t),
            PendingCmd::SimReset(a) => {
                let init = self
                    .sims
                    .iter()
                    .find(|(x, _)| *x == a)
                    .map(|(_, b)| b.clone())
                    .unwrap_or_default();
                self.ledger.sim_reset(a, &init, t);
            }
            PendingCmd::Baseline(a, v) => {
                let mut out = crate::fixed::FixedVec::new();
                self.ledger.set_balance_baseline(a, v, seq, &mut out);
            }
        }
    }
}

/// Snapshot cash coverage must describe the same already-booked quantity.
fn validate_cursor(ledger: &Ledger, e: &SlotEntry) -> Result<(), RestoreError> {
    let inst = ledger.inst(e.inst).ok_or(RestoreError::BadValue)?;
    if inst.venue != e.venue
        || e.cash_qty < 0
        || e.cash_qty > e.cum
        || (e.cash_qty == 0 && e.cash_sum != 0)
        || (inst.perp_cash && e.cash_qty != e.cum)
    {
        return Err(RestoreError::BadValue);
    }
    Ok(())
}

fn cash_increment(
    required: bool,
    prev: &SlotEntry,
    e: &OrderEvent,
) -> Result<Option<Qty>, JournalApplyError> {
    if !required {
        return Ok(None);
    }
    if e.cum_filled.0 < 0
        || e.cash_qty.0 < 0
        || e.cash_qty.0 > e.cum_filled.0
        || (e.cash_qty.0 == 0 && e.cash_sum.0 != 0)
    {
        return Err(JournalApplyError::CashCoverage);
    }
    if e.cum_filled.0 < prev.cum {
        return Ok(Some(Qty(0)));
    }
    if e.cash_qty != e.cum_filled || prev.cash_qty != prev.cum {
        return Err(JournalApplyError::MissingCash);
    }
    if e.cum_filled.0 == prev.cum && e.cash_sum.0 != prev.cash_sum {
        return Err(JournalApplyError::CashConflict);
    }
    e.cash_sum
        .0
        .checked_sub(prev.cash_sum)
        .map(|x| Some(Qty(x)))
        .ok_or(JournalApplyError::CashOverflow)
}
