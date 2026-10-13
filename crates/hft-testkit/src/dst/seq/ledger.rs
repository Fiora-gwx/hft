//! LED-267: an incremental cold ledger over the real engine's committed journal.
//! This observer owns no engine state. A restart discards it and reconsumes the retained log.

use std::collections::BTreeSet;

use hft_event::{
    BlobHdr, BlobKind, BlobRef, EventKind, Note, NoteSource, OrderAction, OrderDecision, OrderEvent,
    RecordFlags, RecordHeader, RecordView, SnapshotHdr,
};
use hft_ledger::{JournalLedger, Ledger, VenueBook};
use hft_refdata::RefData;
use hft_sequencer::driver::replay::Log;
use hft_sequencer::journal::MemRecord;
use hft_sequencer::{EngineSetup, recover_event_end};
use hft_strategy_sdk::{SnapshotReader, SnapshotWriter};
use sha2::{Digest, Sha256};
use zerocopy::TryFromBytes;

use super::reconcile::ReceiptKey;

/// Counts actual comparisons, including the initial restore, and explained unknown fills.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct Counts {
    pub snapshots: u64,
    pub unmapped_fills: u64,
    pub auto_held_offsets: u64,
}

/// Diagnostic bound, independent of production snapshot capacities.
pub const EVENT_LEDGER_CAP: usize = 4 << 20;

pub fn event_sections(ledger: &Ledger) -> Result<Vec<u8>, String> {
    let mut w = SnapshotWriter::new(EVENT_LEDGER_CAP);
    ledger.snapshot(&mut w);
    w.status()
        .map_err(|_| "ledger event observation capacity exceeded")?;
    Ok(w.as_bytes().to_vec())
}

/// Complete ledger read API at one event boundary. The bounded snapshot-observation buffer
/// owns these rows; they are never written to the journal or added to HashTrail.
#[derive(Clone, Debug)]
pub struct Readings {
    positions: Vec<((u8, u32), hft_ledger::InstPos)>,
    books: Vec<((u8, u8), VenueBook)>,
}

impl Readings {
    pub fn of(ledger: &Ledger, rd: &RefData) -> Self {
        let mut positions = Vec::new();
        let mut books = Vec::new();
        for &(account, venue, _) in rd.account_venues() {
            for &inst in rd.insts_of_venue(venue) {
                positions.push(((account.0, inst.0), *ledger.pos(account, inst)));
            }
            books.push(((account.0, venue.0), ledger.venue_book(account, venue).clone()));
        }
        positions.sort_by_key(|r| r.0);
        books.sort_by_key(|r| r.0);
        Self { positions, books }
    }

    fn compare(&self, expected: &Self) -> Result<(), String> {
        if self.positions != expected.positions {
            return Err(format!(
                "positions cold={:?} live={:?}",
                self.positions, expected.positions
            ));
        }
        if self.books.len() != expected.books.len() {
            return Err("missing account/venue observations".into());
        }
        for ((key, a), (expected_key, e)) in self.books.iter().zip(&expected.books) {
            if key != expected_key {
                return Err(format!("account/venue cold={key:?} live={expected_key:?}"));
            }
            compare_book(a, e, true).map_err(|e| format!("account/venue={key:?}: {e}"))?;
        }
        Ok(())
    }
}

impl PartialEq for Readings {
    fn eq(&self, other: &Self) -> bool {
        self.compare(other).is_ok()
    }
}
impl Eq for Readings {}

/// Append-only journal observer. Use a new observer after a recovery truncation. A failed
/// observer stays failed; neither a later snapshot nor a repeated call can erase a mismatch.
#[derive(Debug, Default)]
pub struct Check {
    state: Option<State>,
    error: Option<String>,
}

#[derive(Debug)]
struct State {
    cold: JournalLedger,
    rd: RefData,
    setup: EngineSetup,
    refs: (BlobRef, BlobRef),
    cursor: usize,
    last: MemRecord,
    evidence: Evidence,
    offsets: Offsets,
    counts: Counts,
    /// Retained execution identities are independent of the hot ledger's evicting FIFO.
    booked: BTreeSet<ReceiptKey>,
    receipts: Vec<hft_event::ExternalFill>,
}

type OrderKey = (u8, u8, u16, u32, u32, u32);
const EVENT_EVIDENCE_CAP: usize = hft_sequencer::audit::CAPACITY;

#[derive(Debug, Default)]
struct Evidence {
    epoch: u16,
    shadow: BTreeSet<OrderKey>,
    pending: Vec<(u64, OrderEvent)>,
    notes: Vec<Note>,
    duplicates: Vec<Note>,
    booked: BTreeSet<[u64; 16]>,
}

// Client identity and delivery clocks may differ between copies. All exchange identity
// and financial facts must be identical to a previously explained, committed receipt.
fn receipt_facts(e: &OrderEvent) -> [u64; 16] {
    [
        u64::from(e.venue.0),
        u64::from(e.account.0),
        u64::from(e.inst.0),
        u64::from(e.detail),
        e.exch_order_id,
        e.trade_id,
        e.qty.0 as u64,
        e.cum_filled.0 as u64,
        e.px.0 as u64,
        e.avg_px.0 as u64,
        e.fee.0 as u64,
        u64::from(e.fee_asset.0),
        e.cash_qty.0 as u64,
        e.cash_sum.0 as u64,
        e.ex_ts_ns as u64,
        e.what as u64,
    ]
}

fn duplicate_note(n: &Note, e: &OrderEvent) -> bool {
    if n.inst != e.inst || n.group != hft_types::GROUP_NONE {
        return false;
    }
    let v0 = u64::from(e.venue.0) | u64::from(e.account.0) << 8;
    if e.slot.0 == 0 {
        n.code == hft_oms::notes::code_of(hft_oms::notes::FOREIGN_FILL_VIA_ORDER_EVENT)
            && n.v == [v0, e.exch_order_id, 0, e.cum_filled.0 as u64, 0, 0, 0, 0]
    } else {
        n.code == hft_oms::notes::code_of(hft_oms::notes::STALE_GEN_FILL)
            && n.v
                == [
                    v0,
                    u64::from(e.slot.0),
                    0,
                    u64::from(e.r#gen),
                    u64::from(e.epoch),
                    e.exch_order_id,
                    e.cum_filled.0 as u64,
                    0,
                ]
    }
}

#[derive(Debug, Default)]
struct Offsets {
    cold: Vec<Note>,
    journal: Vec<Note>,
}

impl Offsets {
    fn record(&mut self, r: &MemRecord, generated: &[Note]) -> Result<u64, String> {
        let offset =
            |n: &&Note| n.source == NoteSource::Ledger && n.code == hft_ledger::notes::code::AUTO_HELD_OFFSET;
        if self.cold.len() + generated.iter().filter(offset).count() > EVENT_EVIDENCE_CAP {
            return Err("ledger offset observation capacity exceeded".into());
        }
        self.cold.extend(generated.iter().filter(offset).copied());
        if r.kind == EventKind::NOTE
            && let Some(n) = r.read::<Note>().filter(|n| offset(&n))
        {
            if self.journal.len() == EVENT_EVIDENCE_CAP {
                return Err("ledger offset observation capacity exceeded".into());
            }
            self.journal.push(n);
        }
        if !r.flags.contains(RecordFlags::EVENT_END) {
            return Ok(0);
        }
        if self.cold != self.journal {
            return Err(format!(
                "ledger offset evidence at seq {}: cold={:?} journal={:?}",
                r.seq, self.cold, self.journal
            ));
        }
        let count = self.cold.len() as u64;
        self.cold.clear();
        self.journal.clear();
        Ok(count)
    }
}

impl Check {
    /// Exact sections 1/13/22 at an external-settlement Input boundary. No state is imported
    /// from `expected`; even a one-byte difference is a failure and stays latched.
    pub fn check_event(&mut self, log: &Log, expected: &[u8]) -> Result<Counts, String> {
        let counts = self.check(log)?;
        let (ledger, _, seq) = self.receipt_view()?;
        let actual = event_sections(ledger)?;
        if actual != expected {
            let at = actual
                .iter()
                .zip(expected)
                .position(|(a, b)| a != b)
                .unwrap_or(actual.len().min(expected.len()));
            let e = format!(
                "ledger sections 1/13/22 differ at event boundary {seq}, byte {at} (cold={}, hot={})",
                actual.len(),
                expected.len()
            );
            self.error = Some(e.clone());
            return Err(e);
        }
        Ok(counts)
    }

    pub(crate) fn receipt_view(&self) -> Result<(&Ledger, &BTreeSet<ReceiptKey>, u64), String> {
        if let Some(e) = &self.error {
            return Err(e.clone());
        }
        let s = self
            .state
            .as_ref()
            .ok_or("reconcile: committed cold view unavailable")?;
        if !s.receipts.is_empty() {
            return Err("reconcile: unfinished receipt event".into());
        }
        Ok((s.cold.ledger(), &s.booked, s.last.seq))
    }

    /// The persistent checkpoint and the complete live read API have independent references.
    /// Derived summary timestamps/flags are checked against the live boundary, not a restore
    /// that has deliberately discarded process-local caches.
    pub fn check_boundary(&mut self, log: &Log, live: &Readings) -> Result<Counts, String> {
        let counts = self.check(log)?;
        let state = self.state.as_ref().ok_or("LED-267: cold state unavailable")?;
        if let Err(e) = Readings::of(state.cold.ledger(), &state.rd).compare(live) {
            let e = format!("LED-267 boundary {}: {e}", state.last.seq);
            self.error = Some(e.clone());
            return Err(e);
        }
        Ok(counts)
    }

    /// Check every newly available snapshot and every unmapped-fill increment through a
    /// complete event boundary. Configuration comes only from matching preceding ConfigBlobs.
    pub fn check(&mut self, log: &Log) -> Result<Counts, String> {
        if let Some(error) = &self.error {
            return Err(error.clone());
        }
        let result = self.advance(log);
        if let Err(error) = &result {
            self.error = Some(error.clone());
        }
        result
    }

    fn advance(&mut self, log: &Log) -> Result<Counts, String> {
        let end = recover_event_end(log);
        // The current writer places EVENT_END on the last non-Meta record (SEQ-124).
        // Its event-local measurement tail is published too and cannot change the ledger.
        // Never accept an unfinished Input/Output tail as a completed decision boundary.
        if end == 0 || log.recs[end..].iter().any(|r| r.role() != hft_event::Role::Meta) {
            return Err("LED-267: journal does not end at a complete event".into());
        }
        if self.state.is_none() {
            let ix = log
                .recs
                .iter()
                .position(|r| r.kind == EventKind::STATE_SNAPSHOT)
                .ok_or("LED-267: initial snapshot unavailable")?;
            let r = &log.recs[ix];
            let setup = EngineSetup::from_snapshot(log, r.seq)
                .map_err(|e| format!("LED-267 snapshot {} configuration: {e:?}", r.seq))?;
            let refs = config_refs(r)?;
            let rd = recorded_refdata(log, ix, refs.0)?;
            let mut cold = JournalLedger::from_snapshot(
                &mut SnapshotReader::new(sections(r)?),
                &rd,
                &setup.ledger,
                &setup.caps.to_ledger(),
            )
            .map_err(|e| format!("LED-267 initial snapshot {}: {e:?}", r.seq))?;
            restore_market(cold.ledger_mut(), r)?;
            let mut booked = BTreeSet::new();
            for &(a, v, _) in rd.account_venues() {
                booked.extend(
                    cold.ledger()
                        .ext_seen(a, v)
                        .into_iter()
                        .map(|k| ReceiptKey::from_ext(a, v, k)),
                );
            }
            self.state = Some(State {
                cold,
                rd,
                evidence: Evidence {
                    epoch: (setup.oms.engine_epoch & 0x7fff) as u16,
                    ..Evidence::default()
                },
                offsets: Offsets::default(),
                setup,
                refs,
                cursor: ix + 1,
                last: r.clone(),
                counts: Counts {
                    snapshots: 1,
                    unmapped_fills: 0,
                    auto_held_offsets: 0,
                },
                booked,
                receipts: Vec::new(),
            });
        }
        let state = self.state.as_mut().ok_or("LED-267: cold state unavailable")?;
        if state.cursor > 0 && log.recs.get(state.cursor - 1) != Some(&state.last) {
            return Err("LED-267: consumed journal changed without resetting the observer".into());
        }
        for (ix, r) in log.recs.iter().enumerate().skip(state.cursor) {
            if r.kind == EventKind::STATE_SNAPSHOT {
                if config_refs(r)? != state.refs {
                    return Err(format!(
                        "LED-267 snapshot {}: changed ledger/refdata configuration requires a cold transition",
                        r.seq
                    ));
                }
                let mut expected = Ledger::restore(
                    &mut SnapshotReader::new(sections(r)?),
                    &state.rd,
                    &state.setup.ledger,
                    &state.setup.caps.to_ledger(),
                )
                .map_err(|e| format!("LED-267 snapshot {} restore: {e:?}", r.seq))?;
                restore_market(&mut expected, r)?;
                compare(state.cold.ledger(), &expected, &state.rd)
                    .map_err(|e| format!("LED-267 snapshot {}: {e}", r.seq))?;
                state.counts.snapshots += 1;
            }
            let before = state.cold.stats().unmapped_fills;
            state
                .cold
                .apply(&RecordView {
                    hdr: RecordHeader {
                        len: (24 + r.body.len()) as u32,
                        kind: r.kind.0,
                        flags: r.flags.0,
                        seq: r.seq,
                        tsc: r.tsc,
                    },
                    body: &r.body,
                })
                .map_err(|e| format!("LED-267 seq {} kind {}: {e:?}", r.seq, r.kind.0))?;
            if state.cold.ledger().notes_dropped() != 0 {
                return Err("cold ledger diagnostic notes overflowed".into());
            }
            state.counts.auto_held_offsets += state.offsets.record(r, state.cold.observed_notes())?;
            let delta = state.cold.stats().unmapped_fills - before;
            state.evidence.record(r, delta)?;
            state.counts.unmapped_fills += delta;
            state.capture_receipts(r)?;
            state.cursor = ix + 1;
            state.last = r.clone();
        }
        Ok(state.counts)
    }
}

impl State {
    fn capture_receipts(&mut self, r: &MemRecord) -> Result<(), String> {
        if r.kind == EventKind::EXTERNAL_FILL {
            let fill = hft_event::ExternalFill::try_read_from_bytes(&r.body)
                .map_err(|_| "reconcile: invalid original fill in committed journal")?;
            self.receipts.push(fill);
        } else if r.kind == EventKind::RECONCILE_BATCH {
            let batch = hft_event::ReconcileBatchView::read(&r.body)
                .map_err(|_| "reconcile: invalid receipt group in committed journal")?;
            self.receipts.extend(batch.fills());
        }
        if self.receipts.len() > 256 {
            return Err("reconcile: receipt event capacity exceeded".into());
        }
        if r.flags.contains(RecordFlags::EVENT_END) {
            for e in self.receipts.drain(..) {
                // Only the independently applied cold ledger can prove that an Input booked
                // this key. A rejected batch is journaled too, and must never hide a gap.
                let Ok(key) = ReceiptKey::new(&e) else {
                    continue;
                };
                if self
                    .cold
                    .ledger()
                    .ext_seen(e.account, e.venue)
                    .into_iter()
                    .any(|k| ReceiptKey::from_ext(e.account, e.venue, k) == key)
                {
                    if self.booked.len()
                        >= crate::dst::venue_behavior::EXTERNAL_HISTORY_CAP
                            .saturating_mul(self.rd.account_venues().len())
                        && !self.booked.contains(&key)
                    {
                        return Err("reconcile: retained receipt identity capacity exceeded".into());
                    }
                    self.booked.insert(key);
                }
            }
        }
        Ok(())
    }
}

fn sections(r: &MemRecord) -> Result<&[u8], String> {
    r.body
        .get(size_of::<SnapshotHdr>()..)
        .ok_or_else(|| format!("LED-267 snapshot {}: truncated header", r.seq))
}

// Engine recovery restores mark and funding facts from section 10 after Ledger::restore.
// Only the initial cold state uses this; later snapshots supply the comparison side alone.
fn restore_market(ledger: &mut Ledger, r: &MemRecord) -> Result<(), String> {
    let seq = hft_sequencer::snapshot::read_seq_sections(sections(r)?)
        .map_err(|s| format!("LED-267 snapshot {}: invalid section {s}", r.seq))?;
    let marks: Vec<_> = seq
        .market
        .iter()
        .filter(|r| r.mark_ens != 0 || r.mark_px.0 != 0)
        .map(|r| (r.inst, r.mark_px, r.mark_ens))
        .collect();
    ledger.restore_marks(&marks);
    for row in &seq.market {
        if row.interval_s != 0 {
            ledger.on_funding_interval(row.inst, row.interval_s);
        }
    }
    Ok(())
}

fn config_refs(r: &MemRecord) -> Result<(BlobRef, BlobRef), String> {
    let seq = hft_sequencer::snapshot::read_seq_sections(sections(r)?)
        .map_err(|s| format!("LED-267 snapshot {}: invalid section {s}", r.seq))?;
    let one = |kind| {
        let mut rows = seq.active.iter().filter(|(b, _)| b.kind == kind);
        let b = rows.next().map(|(b, _)| *b);
        match b {
            Some(b) if b.encoding == 1 && rows.next().is_none() => Ok(b),
            _ => Err(format!(
                "LED-267 snapshot {}: missing/duplicate {kind:?} reference",
                r.seq
            )),
        }
    };
    Ok((one(BlobKind::Refdata)?, one(BlobKind::Engine)?))
}

fn recorded_refdata(log: &Log, ix: usize, wanted: BlobRef) -> Result<RefData, String> {
    for r in log.recs[..ix].iter().rev() {
        if r.kind != EventKind::CONFIG_BLOB || r.read::<BlobHdr>().is_none_or(|h| h.blob != wanted) {
            continue;
        }
        let bytes = r
            .body
            .get(size_of::<BlobHdr>()..size_of::<BlobHdr>() + wanted.len as usize)
            .ok_or("LED-267: truncated refdata blob")?;
        let sha: [u8; 32] = Sha256::digest(bytes).into();
        if sha != wanted.sha256 {
            return Err("LED-267: refdata blob hash mismatch".into());
        }
        return RefData::load(bytes, None).map_err(|e| format!("LED-267 refdata: {e:?}"));
    }
    Err("LED-267: preceding refdata blob unavailable".into())
}

fn compare(actual: &Ledger, expected: &Ledger, rd: &RefData) -> Result<(), String> {
    for &(account, venue, _) in rd.account_venues() {
        for &inst in rd.insts_of_venue(venue) {
            let a = actual.pos(account, inst);
            let e = expected.pos(account, inst);
            if a != e {
                return Err(format!(
                    "position account={} venue={} inst={}: cold={a:?} snapshot={e:?}",
                    account.0, venue.0, inst.0
                ));
            }
        }
        compare_book(
            actual.venue_book(account, venue),
            expected.venue_book(account, venue),
            false,
        )
        .map_err(|e| format!("account={} venue={}: {e}", account.0, venue.0))?;
    }
    Ok(())
}

fn compare_book(a: &VenueBook, e: &VenueBook, live: bool) -> Result<(), String> {
    let cash = |b: &VenueBook| {
        let mut rows: Vec<_> = b.cash.iter().copied().collect();
        rows.sort_by_key(|r| r.asset.0);
        rows
    };
    let ac = cash(a);
    let ec = cash(e);
    if ac != ec {
        return Err(format!("cash cold={ac:?} snapshot={ec:?}"));
    }
    // Inventory snapshots omit zero quantities; retain every nonzero (asset, group) row.
    let inv = |b: &VenueBook| {
        let mut rows: Vec<_> = b.inv.iter().copied().filter(|r| r.qty.0 != 0).collect();
        rows.sort_by_key(|r| (r.asset.0, r.group.0));
        rows
    };
    let ai = inv(a);
    let ei = inv(e);
    if ai != ei {
        return Err(format!("inventory cold={ai:?} snapshot={ei:?}"));
    }
    // MarginRow's persisted fields. Recomputed fields are compared with the live observation.
    let persisted = |m: &hft_ledger::MarginState| {
        (
            m.has_report,
            m.rep_margin_balance,
            m.rep_maint,
            m.rep_avail,
            m.rep_ratio,
            m.rep_ens,
            m.rep_perp_notional,
            m.items,
            m.wallet,
        )
    };
    if persisted(&a.margin) != persisted(&e.margin) || (live && a.margin != e.margin) {
        return Err(format!("margin cold={:?} snapshot={:?}", a.margin, e.margin));
    }
    if live && a.summary != e.summary {
        return Err(format!("summary cold={:?} snapshot={:?}", a.summary, e.summary));
    }
    if live && a.vflags != e.vflags {
        return Err(format!("venue flags cold={} snapshot={}", a.vflags, e.vflags));
    }
    Ok(())
}

impl Evidence {
    fn record(&mut self, r: &MemRecord, unmapped: u64) -> Result<(), String> {
        if r.kind == EventKind::ORDER_DECISION {
            let d = r
                .read::<OrderDecision>()
                .ok_or("LED-267: invalid order decision")?;
            if d.action == OrderAction::New && d.reject == 0 {
                let key = (d.venue.0, d.account.0, self.epoch, d.slot.0, d.r#gen, d.inst.0);
                if r.flags.contains(RecordFlags::SHADOW) {
                    // A record-only Shadow decision usually has no allocated identity. Its
                    // zero slot/gen cannot explain an arbitrary unresolved exchange fill.
                    if d.slot.0 != 0 && d.r#gen != 0 {
                        if self.shadow.len() == hft_ledger::SLOT_MAP_CAP && !self.shadow.contains(&key) {
                            return Err("LED-267: Shadow evidence capacity exceeded".into());
                        }
                        self.shadow.insert(key);
                    }
                } else {
                    self.shadow.remove(&key);
                }
            }
        }
        if r.kind == EventKind::NOTE {
            let n = r.read::<Note>().ok_or("LED-267: invalid note")?;
            if n.source == NoteSource::Sequencer && n.code == hft_ledger::notes::code::EPOCH_START {
                self.epoch = (n.v[0] & 0x7fff) as u16;
            }
            if n.source == NoteSource::Oms && n.code == hft_ledger::notes::code::OMS_FOREIGN_FILL {
                if self.notes.len() == EVENT_EVIDENCE_CAP {
                    return Err("LED-267: same-event note capacity exceeded".into());
                }
                self.notes.push(n);
            }
            if n.source == NoteSource::Oms
                && n.v[2] == 0
                && (n.code == hft_oms::notes::code_of(hft_oms::notes::STALE_GEN_FILL)
                    || n.code == hft_oms::notes::code_of(hft_oms::notes::FOREIGN_FILL_VIA_ORDER_EVENT))
            {
                if self.duplicates.len() == EVENT_EVIDENCE_CAP {
                    return Err("LED-267: duplicate note capacity exceeded".into());
                }
                self.duplicates.push(n);
            }
        }
        if unmapped != 0 {
            if unmapped != 1 || r.kind != EventKind::ORDER_EVENT {
                return Err(format!(
                    "LED-267 seq {}: unexplained unmapped counter increment {unmapped}",
                    r.seq
                ));
            }
            let e = r.read::<OrderEvent>().ok_or("LED-267: invalid unmapped fill")?;
            if self.pending.len() == EVENT_EVIDENCE_CAP {
                return Err("LED-267: same-event fill capacity exceeded".into());
            }
            self.pending.push((r.seq, e));
        }
        if r.flags.contains(RecordFlags::EVENT_END) {
            let mut newly_booked = Vec::new();
            for (seq, e) in &self.pending {
                let key = (e.venue.0, e.account.0, e.epoch, e.slot.0, e.r#gen, e.inst.0);
                let found = self.notes.iter().position(|n| {
                    n.inst == e.inst
                        && n.group == hft_types::GROUP_NONE
                        && n.v[0] & 0xff == u64::from(e.venue.0)
                        && (n.v[0] >> 8) & 0xff == u64::from(e.account.0)
                        && (n.v[0] >> 16) & 0xff == u64::from(e.detail)
                        && matches!(e.detail, 1 | 2)
                        && (n.v[0] >> 24) & 0xff == if e.slot.0 == 0 { 1 } else { 2 }
                        && (n.v[0] >> 32) & 0xffff == u64::from(e.fee_asset.0)
                        && n.v[1] > 0
                        && n.v[1] <= e.cum_filled.0 as u64
                        && n.v[4] == e.exch_order_id
                        && n.v[2]
                            == if e.avg_px.0 != 0 {
                                e.avg_px.0 as u64
                            } else {
                                e.px.0 as u64
                            }
                        && n.v[3] == e.fee.0 as u64
                        && n.v[5] == 0
                        && n.v[6] == e.cum_filled.0 as u64
                        && n.v[7] == e.cum_filled.0 as u64
                });
                let Some(ix) = found else {
                    if self.shadow.contains(&key) {
                        continue;
                    }
                    if self.booked.contains(&receipt_facts(e))
                        && let Some(ix) = self.duplicates.iter().position(|n| duplicate_note(n, e))
                    {
                        self.duplicates.remove(ix);
                        continue;
                    }
                    return Err(format!(
                        "LED-267 unmapped fill seq {seq} account={} venue={} epoch={} slot={} gen={} inst={}: missing same-event booking, proven duplicate, or exact Shadow order",
                        e.account.0, e.venue.0, e.epoch, e.slot.0, e.r#gen, e.inst.0
                    ));
                };
                self.notes.remove(ix);
                let facts = receipt_facts(e);
                if self.booked.contains(&facts) || newly_booked.contains(&facts) {
                    return Err("LED-267: previously booked receipt has another booking note".into());
                }
                newly_booked.push(facts);
            }
            if !self.notes.is_empty() {
                return Err("LED-267: OmsForeignFill has no one-to-one cold booking".into());
            }
            if self.booked.len() + newly_booked.len() > crate::dst::venue_behavior::EXTERNAL_HISTORY_CAP {
                return Err("LED-267: booked receipt evidence capacity exceeded".into());
            }
            self.booked.extend(newly_booked);
            self.pending.clear();
            self.notes.clear();
            self.duplicates.clear();
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use hft_event::{Body, NoteLevel, OrderEventKind};
    use hft_ledger::InvEntry;
    use hft_types::{
        AccountId, AssetId, GroupId, InstId, Px, Qty, Side, SlotId, StrategyId, Tif, Tsc, VenueId,
    };

    use crate::dst::sut::{BootCtx, Sut};
    use crate::dst::world::WorldSpec;
    use crate::strategies::TestStrategies;

    fn rec<B: Body>(seq: u64, flags: RecordFlags, b: &B) -> MemRecord {
        MemRecord {
            seq,
            kind: B::KIND,
            flags,
            tsc: seq,
            body: b.as_bytes().to_vec(),
        }
    }

    fn fill() -> OrderEvent {
        OrderEvent {
            venue: VenueId(1),
            account: AccountId(1),
            epoch: 3,
            slot: SlotId(7),
            r#gen: 2,
            inst: InstId(1),
            what: OrderEventKind::PartialFill,
            detail: Side::Buy as u8,
            px: Px(60_000),
            qty: Qty(10),
            cum_filled: Qty(20),
            fee: Qty(1),
            fee_asset: AssetId(2),
            exch_order_id: 73,
            reject_code: 0,
            ex_ts_ns: 0,
            hw_rx_ns: 0,
            rx_tsc: 0,
            avg_px: Px(0),
            cash_qty: Qty(0),
            cash_sum: Qty(0),
            trade_id: 0,
        }
    }

    fn note(e: &OrderEvent) -> Note {
        Note {
            source: NoteSource::Oms,
            code: hft_ledger::notes::code::OMS_FOREIGN_FILL,
            level: NoteLevel::Info,
            inst: e.inst,
            v: [
                u64::from(e.venue.0)
                    | u64::from(e.account.0) << 8
                    | 1 << 16
                    | 2 << 24
                    | u64::from(e.fee_asset.0) << 32,
                10,
                e.px.0 as u64,
                e.fee.0 as u64,
                e.exch_order_id,
                0,
                e.cum_filled.0 as u64,
                e.cum_filled.0 as u64,
            ],
            strategy: StrategyId(1),
            _pad: 0,
            group: hft_types::GROUP_NONE,
        }
    }

    fn unmapped_evidence_cases() {
        let e = fill();
        let mut yes = Evidence::default();
        yes.record(&rec(1, RecordFlags(0), &e), 1).unwrap();
        yes.record(&rec(2, RecordFlags::EVENT_END, &note(&e)), 0).unwrap();
        assert!(yes.pending.is_empty());
        // A previous event, the wrong account/order/cumulative quantity, or a second missing
        // explanation must not cover the input. The fee/price details are checked too.
        for field in [0, 1, 2, 3, 4, 5, 6, 7] {
            let mut bad = Evidence::default();
            bad.record(&rec(1, RecordFlags(0), &e), 1).unwrap();
            let mut n = note(&e);
            if field == 1 {
                n.v[1] = 0;
            } else {
                n.v[field] ^= 1;
            }
            assert!(
                bad.record(&rec(2, RecordFlags::EVENT_END, &n), 0).is_err(),
                "field {field}"
            );
        }
        let mut old = Evidence::default();
        assert!(old.record(&rec(1, RecordFlags::EVENT_END, &note(&e)), 0).is_err());
        let mut duplicate = Evidence::default();
        duplicate.record(&rec(1, RecordFlags(0), &e), 1).unwrap();
        duplicate.record(&rec(2, RecordFlags(0), &note(&e)), 0).unwrap();
        assert!(
            duplicate
                .record(&rec(3, RecordFlags::EVENT_END, &note(&e)), 0)
                .is_err()
        );
        let mut twice = Evidence::default();
        twice.record(&rec(1, RecordFlags(0), &e), 1).unwrap();
        twice.record(&rec(2, RecordFlags(0), &e), 1).unwrap();
        assert!(
            twice
                .record(&rec(3, RecordFlags::EVENT_END, &note(&e)), 0)
                .is_err()
        );

        let d = OrderDecision {
            venue: e.venue,
            account: e.account,
            inst: e.inst,
            slot: e.slot,
            r#gen: e.r#gen,
            action: OrderAction::New,
            side: Side::Buy,
            tif: Tif::Ioc,
            trigger_seq: 0,
            decided_tsc: Tsc(0),
            px: e.px,
            qty: e.qty,
            group: GroupId(1),
            target_slot: SlotId(0),
            reject: 0,
            strategy: StrategyId(1),
            oflags: 0,
            reason: 0,
            intent_no: 0,
            req_px: e.px,
            link_role: 0,
            _pad: [0; 7],
        };
        for (epoch, shadow, pass) in [(3, true, true), (4, true, false), (3, false, false)] {
            let mut evidence = Evidence {
                epoch,
                ..Evidence::default()
            };
            let mut flags = RecordFlags::EVENT_END;
            if shadow {
                flags |= RecordFlags::SHADOW;
            }
            evidence.record(&rec(1, flags, &d), 0).unwrap();
            assert_eq!(
                evidence.record(&rec(2, RecordFlags::EVENT_END, &e), 1).is_ok(),
                pass
            );
        }
        let mut unresolved = e;
        unresolved.slot = SlotId(0);
        unresolved.r#gen = 0;
        let mut no_identity = d;
        no_identity.slot = SlotId(0);
        no_identity.r#gen = 0;
        let mut evidence = Evidence {
            epoch: e.epoch,
            ..Evidence::default()
        };
        evidence
            .record(
                &rec(1, RecordFlags::SHADOW | RecordFlags::EVENT_END, &no_identity),
                0,
            )
            .unwrap();
        assert!(
            evidence
                .record(&rec(2, RecordFlags::EVENT_END, &unresolved), 1)
                .is_err()
        );
        let mut full = Evidence::default();
        for _ in 0..EVENT_EVIDENCE_CAP {
            full.record(&rec(1, RecordFlags(0), &note(&e)), 0).unwrap();
        }
        assert!(
            full.record(&rec(2, RecordFlags(0), &note(&e)), 0)
                .unwrap_err()
                .contains("capacity")
        );
        assert_eq!(full.notes.len(), EVENT_EVIDENCE_CAP);

        // A copied unresolved/stale report is explained by BOTH a previous complete
        // booking and this event's exact zero-increment defensive note.
        for unresolved in [false, true] {
            for damage in 0..19 {
                let mut proof = Evidence::default();
                if damage != 1 {
                    proof.record(&rec(1, RecordFlags(0), &e), 1).unwrap();
                    proof
                        .record(&rec(2, RecordFlags::EVENT_END, &note(&e)), 0)
                        .unwrap();
                }
                let mut copy = e;
                if unresolved {
                    copy.slot = SlotId(0);
                    copy.r#gen = 0;
                    copy.epoch = 0;
                }
                // Change the claimed original facts, keeping the current note internally
                // consistent. None may borrow the previous receipt's booking.
                match damage {
                    2 => copy.fee.0 += 1,
                    3 => copy.trade_id += 1,
                    4 => copy.cash_sum.0 += 1,
                    5 => copy.exch_order_id += 1,
                    6 => copy.qty.0 += 1,
                    7 => copy.cum_filled.0 += 1,
                    8 => copy.px.0 += 1,
                    9 => copy.avg_px.0 += 1,
                    10 => copy.fee_asset.0 += 1,
                    11 => copy.cash_qty.0 += 1,
                    12 => copy.ex_ts_ns += 1,
                    13 => copy.account.0 += 1,
                    14 => copy.inst.0 += 1,
                    15 => copy.detail = Side::Sell as u8,
                    _ => {}
                }
                let mut n = note(&copy);
                let v0 = u64::from(copy.venue.0) | u64::from(copy.account.0) << 8;
                if unresolved {
                    n.code = hft_oms::notes::code_of(hft_oms::notes::FOREIGN_FILL_VIA_ORDER_EVENT);
                    n.v = [v0, copy.exch_order_id, 0, copy.cum_filled.0 as u64, 0, 0, 0, 0];
                } else {
                    n.code = hft_oms::notes::code_of(hft_oms::notes::STALE_GEN_FILL);
                    n.v = [
                        v0,
                        u64::from(copy.slot.0),
                        0,
                        u64::from(copy.r#gen),
                        u64::from(copy.epoch),
                        copy.exch_order_id,
                        copy.cum_filled.0 as u64,
                        0,
                    ];
                }
                if damage == 16 {
                    n.v[4] ^= 1;
                }
                if damage == 17 {
                    n.v[2] = 1;
                }
                if damage == 18 {
                    n = note(&copy);
                }
                proof.record(&rec(3, RecordFlags(0), &copy), 1).unwrap();
                let result = proof.record(&rec(4, RecordFlags::EVENT_END, &n), 0);
                assert_eq!(
                    result.is_ok(),
                    damage == 0,
                    "copy unresolved={unresolved} damage={damage}"
                );
            }
        }
        let mut missing = Evidence::default();
        missing.record(&rec(1, RecordFlags(0), &e), 1).unwrap();
        missing
            .record(&rec(2, RecordFlags::EVENT_END, &note(&e)), 0)
            .unwrap();
        assert!(missing.record(&rec(3, RecordFlags::EVENT_END, &e), 1).is_err());
        let mut full = Evidence::default();
        full.booked
            .extend((0..crate::dst::venue_behavior::EXTERNAL_HISTORY_CAP).map(|i| [i as u64; 16]));
        full.record(&rec(1, RecordFlags(0), &e), 1).unwrap();
        assert!(
            full.record(&rec(2, RecordFlags::EVENT_END, &note(&e)), 0)
                .unwrap_err()
                .contains("capacity")
        );
    }

    #[test]
    fn led_267_full_venue_fields_and_recorded_configuration_are_required() {
        unmapped_evidence_cases();
        let world = WorldSpec::w1();
        let sut = super::super::SeqSut::<TestStrategies>::boot(&BootCtx {
            seed: 1,
            world: &world,
            planted: &[],
            tsc_hz: 3_000_000_000,
        })
        .unwrap();
        let log = Log::from_segments(&sut.sequencer().driver().io.j.segments);
        let mut check = Check::default();
        let live = Readings::of(&sut.sequencer().engine().ledger, &sut.sequencer().engine().rd);
        let count = check.check_boundary(&log, &live).unwrap();
        assert!(count.snapshots > 0);
        let mut meta_tail = log.clone();
        let mut meta = log.recs.last().unwrap().clone();
        meta.seq += 1;
        meta.kind = EventKind::LATENCY_SAMPLE;
        meta.flags = RecordFlags(0);
        meta.body = vec![0; size_of::<hft_event::LatencySample>()];
        meta_tail.recs.push(meta.clone());
        Check::default().check_boundary(&meta_tail, &live).unwrap();
        for kind in [EventKind::BBO, EventKind::ORDER_DECISION] {
            let mut incomplete = meta_tail.clone();
            incomplete.recs.last_mut().unwrap().kind = kind;
            assert!(
                Check::default()
                    .check(&incomplete)
                    .unwrap_err()
                    .contains("complete event"),
                "unfinished {kind:?} tail was accepted"
            );
        }
        assert_eq!(
            check.check(&log).unwrap(),
            count,
            "a repeated boundary is not a second comparison"
        );
        let mut wrong = live.clone();
        wrong.positions[0].1.last_fill_ens += 1;
        assert!(
            Check::default()
                .check_boundary(&log, &wrong)
                .unwrap_err()
                .contains("positions")
        );
        let mut wrong = live.clone();
        wrong.books[0].1.summary.as_of_ens += 1;
        assert!(
            Check::default()
                .check_boundary(&log, &wrong)
                .unwrap_err()
                .contains("summary")
        );
        let mut missing_live = live.clone();
        missing_live.books.pop();
        assert!(
            Check::default()
                .check_boundary(&log, &missing_live)
                .unwrap_err()
                .contains("missing")
        );
        let state = check.state.as_ref().unwrap();
        let (account, venue, _) = &state.rd.account_venues()[0];
        let expected = state.cold.ledger().venue_book(*account, *venue).clone();
        assert!(!expected.cash.is_empty());
        for field in 0..8 {
            let mut actual = expected.clone();
            let cash = actual.cash.get_mut(0).unwrap();
            match field {
                0 => cash.total.0 += 1,
                1 => cash.baseline.0 += 1,
                2 => cash.unassigned.0 += 1,
                3 => cash.rep_free.0 += 1,
                4 => cash.rep_locked.0 += 1,
                5 => cash.rep_ex_ts_ns += 1,
                6 => cash.rep_ens += 1,
                _ => cash.bflags ^= 1,
            }
            assert!(
                compare_book(&actual, &expected, true)
                    .unwrap_err()
                    .starts_with("cash")
            );
        }
        let mut actual = expected.clone();
        assert!(actual.inv.push(InvEntry {
            asset: AssetId(1),
            group: GroupId(999),
            qty: Qty(0)
        }));
        assert!(
            compare_book(&actual, &expected, true).is_ok(),
            "canonical snapshots omit zero inventory"
        );
        let last = actual.inv.len() - 1;
        actual.inv.get_mut(last).unwrap().qty = Qty(1);
        assert!(
            compare_book(&actual, &expected, true)
                .unwrap_err()
                .starts_with("inventory")
        );
        let mut actual = expected.clone();
        actual.margin.rep_ens += 1;
        assert!(
            compare_book(&actual, &expected, true)
                .unwrap_err()
                .starts_with("margin")
        );
        let mut actual = expected.clone();
        actual.summary.as_of_ens += 1;
        assert!(
            compare_book(&actual, &expected, true)
                .unwrap_err()
                .starts_with("summary")
        );
        let mut actual = expected.clone();
        actual.vflags ^= 1;
        assert!(
            compare_book(&actual, &expected, true)
                .unwrap_err()
                .starts_with("venue flags")
        );

        let mut missing = log.clone();
        for r in &mut missing.recs {
            if r.kind == EventKind::CONFIG_BLOB
                && r.read::<BlobHdr>()
                    .is_some_and(|h| h.blob.kind == BlobKind::Refdata)
            {
                r.body[size_of::<BlobHdr>()] ^= 1;
            }
        }
        let mut failed = Check::default();
        let error = failed.check(&missing).unwrap_err();
        assert!(error.contains("configuration"), "{error}");
        assert_eq!(failed.check(&log).unwrap_err(), error, "failure stays latched");
    }
}
