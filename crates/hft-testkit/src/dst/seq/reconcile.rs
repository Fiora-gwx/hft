//! Cold, committed receipt selection for the DST reconciler. Exchange history supplies cash;
//! neither a hot wallet delta nor the resulting position supplies an expected cash amount.

use std::collections::{BTreeMap, BTreeSet};

use hft_event::{
    Bbo, ConnRole, ConnState, ConnStateKind, EventKind, ExternalCause, ExternalFill, FundingSettle, Note,
    NoteSource, OrderDecision, OrderEvent, ReconcileBatchView, ReconcileReport,
};
use hft_ledger::ExtKey;
use hft_sequencer::driver::replay::Log;
use hft_strategy_sdk::StrategySet;
use hft_types::{AccountId, InstId, Px, Qty, RoundMode, VenueId};
use zerocopy::{FromZeros, IntoBytes};

use super::{SeqSut, ledger::Check};
use crate::dst::actors::ReconActor;
use crate::dst::obs::TruthRow;
use crate::dst::sut::{InputRec, Source, Sut};
use crate::dst::venue_behavior::{EXTERNAL_HISTORY_CAP, ExternalReceipt};
use crate::dst::world::AccountKind;

/// A complete identity, including the namespace in which an exchange assigns its IDs.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub(crate) struct ReceiptKey(pub u8, pub u8, pub u32, pub u8, pub u64, pub u64);

impl ReceiptKey {
    pub(crate) fn new(e: &ExternalFill) -> Result<Self, String> {
        let k = if e.trade_id != 0 && e.fill_seq == 0 {
            ExtKey {
                inst: e.inst.0,
                kind: 1,
                a: e.trade_id,
                b: 0,
            }
        } else if e.trade_id == 0 && e.exch_order_id != 0 && e.fill_seq != 0 {
            ExtKey {
                inst: e.inst.0,
                kind: 2,
                a: e.exch_order_id,
                b: e.fill_seq,
            }
        } else {
            return Err("reconcile: incomplete or ambiguous original execution identity".into());
        };
        Ok(Self::from_ext(e.account, e.venue, k))
    }

    pub(crate) fn from_ext(a: AccountId, v: VenueId, k: ExtKey) -> Self {
        Self(a.0, v.0, k.inst, k.kind, k.a, k.b)
    }
}

/// Append-only, cold reconciliation reader. It is independent of enabled invariant checkers.
/// Recreate it after journal truncation; an unchanged committed prefix is required otherwise.
#[derive(Debug, Default)]
pub struct ReceiptReader {
    cold: Check,
}

impl ReceiptReader {
    /// Select every missing original execution for all AUTO_EXTERNAL rows in one report group.
    /// A missing, excess, conflicting or already-booked receipt cannot become a cash estimate.
    /// Reports must refer to this exact committed view, not a virtual timestamp or a future seq.
    pub fn select<S: StrategySet>(
        &mut self,
        sut: &SeqSut<S>,
        reports: &[ReconcileReport],
        history: &[ExternalReceipt],
    ) -> Result<InputRec, String> {
        let log = Log::from_segments(&sut.sequencer().driver().io.j.segments);
        self.select_log(&log, reports, history)
    }

    pub(crate) fn select_log(
        &mut self,
        log: &Log,
        reports: &[ReconcileReport],
        history: &[ExternalReceipt],
    ) -> Result<InputRec, String> {
        self.cold.check(log)?;
        let (ledger, booked, view_seq) = self.cold.receipt_view()?;
        let first = reports.first().ok_or("reconcile: empty report group")?;
        if reports.iter().any(|r| r.view_seq != view_seq) {
            return Err(format!(
                "reconcile: reports must use committed view_seq {view_seq}"
            ));
        }
        if history.len() > EXTERNAL_HISTORY_CAP {
            return Err("reconcile: exchange history exceeds its retained capacity".into());
        }
        let mut originals = BTreeMap::new();
        for r in history {
            let key = ReceiptKey::new(&r.fill)?;
            if let Some(previous) = originals.insert(key, r) {
                // Pagination overlap is harmless only for byte-identical original facts.
                if previous.fill.as_bytes() != r.fill.as_bytes() {
                    return Err(format!("reconcile: conflicting history for {key:?}"));
                }
            }
        }
        let mut insts = BTreeSet::new();
        let mut selected = Vec::new();
        for report in reports
            .iter()
            .filter(|r| r.item == 1 && r.sflags & ReconcileReport::AUTO_EXTERNAL != 0)
        {
            if (report.account, report.venue) != (first.account, first.venue) || !insts.insert(report.inst.0)
            {
                return Err("reconcile: mixed account/venue or duplicate position".into());
            }
            let current = ledger.pos(report.account, report.inst).qty;
            if current != report.engine_qty {
                return Err("reconcile: position does not match the committed cold view".into());
            }
            let delta = i128::from(report.exch_qty.0) - i128::from(current.0);
            if delta == 0 || !same_sign_shrink(current.0, report.exch_qty.0) {
                return Err("reconcile: AUTO_EXTERNAL requires a same-sign position reduction".into());
            }
            let mut sum = 0i128;
            // Keep exchange history order. Cash is already settled, but attribution still has
            // an order. BTreeMap above validates identities; it must not reorder executions.
            let mut used = BTreeSet::new();
            for receipt in history {
                let e = &receipt.fill;
                if (e.account, e.venue, e.inst) != (report.account, report.venue, report.inst) {
                    continue;
                }
                let key = ReceiptKey::new(e)?;
                if booked.contains(&key) || !used.insert(key) {
                    continue;
                }
                if !matches!(
                    e.cause,
                    ExternalCause::Unknown | ExternalCause::Adl | ExternalCause::Liquidation
                ) || e.qty.0 <= 0
                    || e.cash_qty != e.qty
                    || e._pad != [0; 6]
                    || i128::from(e.side.sign()) != delta.signum()
                {
                    return Err(format!(
                        "reconcile: ineligible or incomplete original receipt {key:?}"
                    ));
                }
                sum += i128::from(e.side.sign()) * i128::from(e.qty.0);
                selected.push(receipt.clone());
            }
            if sum != delta {
                return Err(format!(
                    "reconcile: inst {} receipts explain {sum}, position delta is {delta}",
                    report.inst.0
                ));
            }
        }
        if insts.is_empty() {
            return Err("reconcile: no AUTO_EXTERNAL positions".into());
        }
        let input = ReconActor::settled_input(reports, &selected).map_err(String::from)?;
        // The independent ledger validates assets, cash coverage, aggregate quantities and
        // receipt capacity. It never mutates itself while constructing the proposed Input.
        let batch = ReconcileBatchView::read(&input.body).map_err(|e| format!("reconcile: {e:?}"))?;
        ledger
            .validate_reconcile_batch(&batch)
            .map_err(|e| format!("reconcile: {e:?}"))?;
        Ok(input)
    }

    /// Committed quantities for every requested instrument, including exchange-side zeros.
    pub fn positions<S: StrategySet>(
        &mut self,
        sut: &SeqSut<S>,
        account: AccountId,
        insts: &[InstId],
    ) -> Result<(u64, Vec<(InstId, i64)>), String> {
        let account_spec = sut.world().account(account).ok_or("reconcile: unknown account")?;
        if insts.iter().any(|i| {
            !sut.world()
                .inst(*i)
                .is_some_and(|i| account_spec.venues.contains(&i.venue))
        }) {
            return Err("reconcile: unknown instrument or account binding".into());
        }
        let log = Log::from_segments(&sut.sequencer().driver().io.j.segments);
        self.cold.check(&log)?;
        let (ledger, _, seq) = self.cold.receipt_view()?;
        Ok((
            seq,
            insts.iter().map(|&i| (i, ledger.pos(account, i).qty.0)).collect(),
        ))
    }
}

fn same_sign_shrink(engine: i64, exchange: i64) -> bool {
    (exchange == 0 || exchange.signum() == engine.signum()) && exchange.unsigned_abs() < engine.unsigned_abs()
}

#[derive(Clone, Copy, Debug)]
struct Previous {
    seq: u64,
    engine: i64,
    exchange: i64,
    eligible: bool,
}

/// Previous rounds are process-local and are discarded when the journal records a new epoch.
#[derive(Debug, Default)]
pub(crate) struct CommittedRecon {
    reader: ReceiptReader,
    epoch_seq: u64,
    previous: BTreeMap<(u8, u8, u32), Previous>,
    skipped: BTreeMap<(u8, u8, u32), (u64, u8)>,
}

impl ReconActor {
    /// Real-engine reconciliation, including full-close positions absent from exchange truth.
    /// The DST's REST snapshot is synchronous, so S0 == S1. The default lag bound is the
    /// reconciler's documented 2,000 ms; tests may supply another explicit bound.
    pub fn round_committed<S: StrategySet>(
        &mut self,
        sut: &SeqSut<S>,
        truth: &[TruthRow],
        history: &[ExternalReceipt],
        lag_ns: u64,
    ) -> Result<Vec<InputRec>, String> {
        if !self.funding_diffs.is_empty() {
            return Err("reconcile: real funding rounds require original per-settlement facts".into());
        }
        self.committed.round(sut, truth, history, lag_ns)
    }
}

impl CommittedRecon {
    fn round<S: StrategySet>(
        &mut self,
        sut: &SeqSut<S>,
        truth: &[TruthRow],
        history: &[ExternalReceipt],
        lag_ns: u64,
    ) -> Result<Vec<InputRec>, String> {
        let journal = sut.journal();
        let view_seq = journal.last().ok_or("reconcile: empty journal")?.seq;
        let view_tsc = journal.last().ok_or("reconcile: empty journal")?.tsc;
        let epoch_seq = journal
            .iter()
            .rev()
            .find_map(|r| {
                r.try_read::<Note>()
                    .filter(|n| {
                        n.source == NoteSource::Sequencer && n.code == hft_ledger::notes::code::EPOCH_START
                    })
                    .map(|_| r.seq)
            })
            .ok_or("reconcile: missing epoch observation")?;
        if epoch_seq != self.epoch_seq {
            *self = Self {
                epoch_seq,
                ..Self::default()
            };
        }
        let log = Log::from_segments(&sut.sequencer().driver().io.j.segments);
        self.reader.cold.check(&log)?;
        let (ledger, _, cold_seq) = self.reader.cold.receipt_view()?;
        if cold_seq != view_seq {
            return Err("reconcile: journal mirror is not committed".into());
        }
        let lag_tsc = u64::try_from(u128::from(lag_ns) * u128::from(sut.tsc_hz()) / 1_000_000_000)
            .map_err(|_| "reconcile: lag exceeds clock range")?;
        let cutoff = view_tsc.saturating_sub(lag_tsc);
        let e = sut.sequencer().engine();
        let mut next = BTreeMap::new();
        let mut skipped = BTreeMap::new();
        let mut groups = Vec::new();
        let mut report_id = journal
            .iter()
            .filter_map(|r| {
                if r.kind == EventKind::RECONCILE_BATCH {
                    ReconcileBatchView::read(&r.body).ok().map(|b| b.hdr.report_id)
                } else {
                    r.read::<ReconcileReport>().map(|x| x.report_id)
                }
            })
            .max()
            .unwrap_or(0);
        for account in sut
            .world()
            .accounts
            .iter()
            .filter(|a| a.kind == AccountKind::Live)
        {
            for &venue in &account.venues {
                let mut matches = truth
                    .iter()
                    .filter(|t| t.account == account.id && t.venue == venue);
                let t = matches
                    .next()
                    .ok_or("reconcile: missing account/venue exchange truth")?;
                if matches.next().is_some() {
                    return Err("reconcile: duplicate account/venue truth".into());
                }
                let mut exchange = BTreeMap::new();
                for &(inst, qty) in &t.positions {
                    if !sut.world().inst(inst).is_some_and(|i| i.venue == venue && i.perp)
                        || exchange.insert(inst.0, qty).is_some()
                    {
                        return Err("reconcile: unknown or duplicate exchange position".into());
                    }
                }
                let mut up_seq = 0;
                let mut stream_down = true;
                let mut recent_down = false;
                let mut activity = BTreeMap::<u32, (u64, u64)>::new();
                let mut prices = BTreeMap::new();
                for r in journal {
                    if let Some(c) = r.try_read::<ConnState>()
                        && c.role == ConnRole::UserStream
                        && c.account == account.id
                        && c.venue == venue
                    {
                        stream_down = c.state != ConnStateKind::Up;
                        if stream_down {
                            recent_down |= r.tsc > cutoff;
                        } else {
                            up_seq = r.seq;
                        }
                    }
                    if let Some(b) = r.read::<Bbo>()
                        && b.h.venue == venue
                    {
                        prices.insert(b.h.inst.0, Px::mid_floor(b.bid_px, b.ask_px));
                    }
                    let key = if let Some(d) = r.try_read::<OrderDecision>() {
                        Some((d.account, d.venue, d.inst))
                    } else if let Some(f) = r.try_read::<OrderEvent>() {
                        Some((f.account, f.venue, f.inst))
                    } else if let Some(f) = r.try_read::<ExternalFill>() {
                        Some((f.account, f.venue, f.inst))
                    } else {
                        r.read::<FundingSettle>().map(|f| (f.account, f.venue, f.inst))
                    };
                    if let Some((a, v, i)) = key
                        && a == account.id
                        && v == venue
                    {
                        // Funding blocks the lag window, but does not by itself invalidate
                        // position stability across otherwise quiet adjacent rounds.
                        let previous = activity.get(&i.0).copied().unwrap_or((0, 0));
                        let position_seq = if r.kind == EventKind::FUNDING_SETTLE {
                            previous.0
                        } else {
                            r.seq
                        };
                        activity.insert(i.0, (position_seq, r.tsc));
                    }
                }
                let down = stream_down || recent_down;
                let table = e
                    .oms
                    .table_view(venue, account.id)
                    .ok_or("reconcile: missing OMS table observation")?;
                let mut unknown = table.unknown_count != 0;
                let mut busy = BTreeSet::new();
                for slot in 1..=table.n_slots {
                    let (s, _, _) = e
                        .oms
                        .raw_slot(venue, account.id, slot)
                        .ok_or("reconcile: missing OMS slot observation")?;
                    let flags = hft_oms::SlotFlags::CANCEL_SENT
                        | hft_oms::SlotFlags::AMEND_SENT
                        | hft_oms::SlotFlags::VERIFY;
                    unknown |=
                        s.state == hft_oms::OrderState::Unknown || s.sflags.has(hft_oms::SlotFlags::VERIFY);
                    if matches!(
                        s.state,
                        hft_oms::OrderState::PendingNew
                            | hft_oms::OrderState::PendingCancel
                            | hft_oms::OrderState::PendingAmend
                            | hft_oms::OrderState::Unknown
                    ) || (s.state != hft_oms::OrderState::Free && s.sflags.0 & flags != 0)
                    {
                        busy.insert(s.inst.0);
                    }
                    if s.r#gen != 0 {
                        let mut generation = s.r#gen;
                        for _ in 0..=e.setup.oms.tombstone_gens {
                            if let Some(tomb) = e.oms.tomb(venue, account.id, slot, generation)
                                && e.now.engine_ns.saturating_sub(e.now.ens_of(tomb.freed_tsc))
                                    < i64::from(e.setup.oms.tombstone_keep_ms) * 1_000_000
                            {
                                busy.insert(tomb.inst.0);
                            }
                            generation = if generation == 1 { u32::MAX } else { generation - 1 };
                        }
                    }
                }
                report_id = report_id.checked_add(1).ok_or("reconcile: report ID exhausted")?;
                let mut summary = ReconcileReport::new_zeroed();
                summary.report_id = report_id;
                summary.account = account.id;
                summary.venue = venue;
                summary.item = 5;
                summary.view_seq = view_seq;
                summary.sflags = if down { ReconcileReport::USERSTREAM_DOWN } else { 0 };
                let mut reports = vec![summary];
                for inst in sut.world().insts.iter().filter(|i| i.venue == venue && i.perp) {
                    let key = (account.id.0, venue.0, inst.id.0);
                    let act = [activity.get(&inst.id.0), activity.get(&0)]
                        .into_iter()
                        .flatten()
                        .copied()
                        .fold((0u64, 0u64), |(seq, tsc), row| (seq.max(row.0), tsc.max(row.1)));
                    let blocked = busy.contains(&inst.id.0) || act.1 > cutoff;
                    let mut force_report = false;
                    if blocked {
                        let count = self.skipped.get(&key).map_or(1, |&(seq, n)| {
                            if seq < view_seq { n.saturating_add(1) } else { n }
                        });
                        skipped.insert(key, (view_seq, count));
                        force_report = count >= 3;
                        if !force_report {
                            reports[0].skipped_inflight = reports[0].skipped_inflight.saturating_add(1);
                        }
                    }
                    let eng = ledger.pos(account.id, inst.id).qty.0;
                    let ex = i64::try_from(exchange.get(&inst.id.0).copied().unwrap_or(0))
                        .map_err(|_| "reconcile: exchange position overflows quantity")?;
                    if eng == ex || (blocked && !force_report) {
                        continue;
                    }
                    let eligible = !down && !unknown && !blocked && same_sign_shrink(eng, ex);
                    let auto = eligible
                        && self.previous.get(&key).is_some_and(|p| {
                            p.eligible
                                && p.seq < view_seq
                                && p.engine == eng
                                && p.exchange == ex
                                && act.0 <= p.seq
                                && up_seq != 0
                                && up_seq < p.seq
                        });
                    next.insert(
                        key,
                        Previous {
                            seq: view_seq,
                            engine: eng,
                            exchange: ex,
                            eligible,
                        },
                    );
                    let mut row = summary;
                    row.item = 1;
                    row.inst = inst.id;
                    row.severity = 1;
                    row.engine_qty = Qty(eng);
                    row.exch_qty = Qty(ex);
                    row.sflags = if auto { ReconcileReport::AUTO_EXTERNAL } else { 0 };
                    let qty = i64::try_from((i128::from(eng) - i128::from(ex)).abs())
                        .map_err(|_| "reconcile: position delta overflows quantity")?;
                    let price = *prices
                        .get(&inst.id.0)
                        .ok_or("reconcile: missing recorded price")?;
                    let (diff, err) = price.mul_qty(
                        Qty(qty),
                        super::build::asset_exp(sut.world(), inst.base),
                        RoundMode::Ceil,
                    );
                    if err != hft_types::FxErr::OK {
                        return Err("reconcile: position notional overflows".into());
                    }
                    row.diff = diff;
                    reports.push(row);
                }
                if reports.len() > 256 {
                    return Err("reconcile: report group capacity exceeded".into());
                }
                reports[0].severity = u8::from(reports.len() > 1);
                let count = reports.len() as u16;
                for (idx, r) in reports.iter_mut().enumerate() {
                    r.idx = idx as u16;
                    r.count = count;
                }
                groups.push(reports);
            }
        }
        let mut inputs = Vec::new();
        for reports in groups {
            if reports
                .iter()
                .any(|r| r.sflags & ReconcileReport::AUTO_EXTERNAL != 0)
            {
                let venue_history: Vec<_> = history
                    .iter()
                    .filter(|r| r.fill.venue == reports[0].venue)
                    .cloned()
                    .collect();
                inputs.push(self.reader.select_log(&log, &reports, &venue_history)?);
            } else {
                inputs.extend(reports.iter().map(|r| InputRec::of(r, Source::Recon)));
            }
        }
        self.previous = next;
        self.skipped = skipped;
        Ok(inputs)
    }
}
