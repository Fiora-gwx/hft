//! `hft_py.ledger` and `hft_py.pnl` (README "2. ledger / pnl"): hft-ledger's journal cold consumer
//! `JournalLedger` driven over the journal. This module only feeds records and reads the ledger's
//! views; every booking happens inside hft-ledger.

use std::collections::BTreeMap;
use std::sync::Arc;

use hft_event::{EventKind, GroupSpec, PartHdr, RecordFlags, RecordHeader, RecordView};
use hft_journal::StartAt;
use hft_ledger::{Capacities, InstPos, JournalLedger, Ledger, LedgerConfig, PnlBreakdown};
use hft_refdata::{AccountKind, EngineConfig, RefData};
use hft_strategy_sdk::{GroupTable, SnapshotReader, SnapshotWriter};
use hft_types::{AccountId, GROUP_NONE, GroupId, StrategyId, VenueId};
use zerocopy::IntoBytes;

use crate::config::{Change, Config, Tracker, blob_kind};
use crate::error::{Error, Res};
use crate::journal::{Journal, Rec, Step};

/// `GroupTable` capacity per strategy instance.
pub const GROUP_CAP: u16 = 1024;

/// Engine capacities used for the cold ledger. ECF1 section 3 (`CapacitiesRow`) has no decoder
/// yet (its row format belongs to hft-sequencer), so these fixed values are used; they bound
/// nothing hft-py reads beyond the ledger's own tables.
pub fn capacities(rd: &RefData) -> Capacities {
    Capacities {
        max_insts: (rd.inst_count() as u32 + 1).max(4096),
        max_cells_total: 8192,
        max_timers: 256,
        recent_cmd_ids: 64,
        max_subs_per_inst: 8,
        max_groups_per_instance: GROUP_CAP,
        max_instances: 254,
    }
}

/// Snapshot section 1 (`Positions`, hft-ledger).
const SECTION_POSITIONS: u16 = 1;

/// Sections of a `StateSnapshot` logical body (after the 96-byte `SnapshotHdr`).
fn sections(body: &[u8]) -> &[u8] {
    body.get(96..).unwrap_or(&[])
}

fn has_ledger(body: &[u8]) -> bool {
    let r = SnapshotReader::new(sections(body));
    matches!(r.find_section(SECTION_POSITIONS), Ok(Some(_)))
}

/// Refdata, ledger config, capacities and group tables of a configuration.
struct Setup {
    rd: Arc<RefData>,
    cfg: LedgerConfig,
    caps: Capacities,
}

fn setup(c: &Config) -> Res<Setup> {
    let rd = c
        .blobs
        .get(&(blob_kind::REFDATA, 0, 0))
        .ok_or_else(|| Error::value("the journal has no refdata in effect at the ledger start point"))?;
    let rd = RefData::load(&rd.content, None)
        .map_err(|e| Error::value(format!("refdata blob does not load: {e:?}")))?;
    let cfg = c
        .blobs
        .get(&(blob_kind::ENGINE, 0, 0))
        .and_then(|b| EngineConfig::load(&b.content, None).ok())
        .and_then(|ec| LedgerConfig::from_engine(&ec).ok())
        .unwrap_or_default();
    let caps = capacities(&rd);
    Ok(Setup {
        rd: Arc::new(rd),
        cfg,
        caps,
    })
}

/// `(strategy, groups)` of every strategy-params blob in `c`.
fn group_tables(c: &Config) -> Vec<(StrategyId, Vec<GroupSpec>)> {
    c.blobs
        .values()
        .filter(|b| b.kind == blob_kind::STRATEGY_PARAMS)
        .filter_map(|b| {
            crate::params::parse_container(&b.content)
                .ok()
                .map(|(h, g, _)| (h.strategy, g))
        })
        .collect()
}

/// Per-(account, group) PnL keys tracked for deltas.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct CellKey {
    pub account: u8,
    pub group: u32,
}

/// One event's PnL change of one cell.
#[derive(Clone, Copy, Debug)]
pub struct Delta {
    pub key: CellKey,
    pub round: u32,
    pub proc_ns: i64,
    pub d: PnlBreakdown,
}

fn add(a: &mut PnlBreakdown, b: &PnlBreakdown) {
    a.price_realized.0 = a.price_realized.0.saturating_add(b.price_realized.0);
    a.funding.0 = a.funding.0.saturating_add(b.funding.0);
    a.fee_cex.0 = a.fee_cex.0.saturating_add(b.fee_cex.0);
    a.gas.0 = a.gas.0.saturating_add(b.gas.0);
    a.bribe.0 = a.bribe.0.saturating_add(b.bribe.0);
    a.unrealized.0 = a.unrealized.0.saturating_add(b.unrealized.0);
    a.adjust.0 = a.adjust.0.saturating_add(b.adjust.0);
    a.external.0 = a.external.0.saturating_add(b.external.0);
}

fn sub(a: &PnlBreakdown, b: &PnlBreakdown) -> PnlBreakdown {
    let mut d = *a;
    d.price_realized.0 = a.price_realized.0.saturating_sub(b.price_realized.0);
    d.funding.0 = a.funding.0.saturating_sub(b.funding.0);
    d.fee_cex.0 = a.fee_cex.0.saturating_sub(b.fee_cex.0);
    d.gas.0 = a.gas.0.saturating_sub(b.gas.0);
    d.bribe.0 = a.bribe.0.saturating_sub(b.bribe.0);
    d.unrealized.0 = a.unrealized.0.saturating_sub(b.unrealized.0);
    d.adjust.0 = a.adjust.0.saturating_sub(b.adjust.0);
    d.external.0 = a.external.0.saturating_sub(b.external.0);
    d
}

/// Result of a ledger run.
pub struct Run {
    pub jl: JournalLedger,
    pub rd: Arc<RefData>,
    /// Per-event deltas inside `[t0, t1)`.
    pub deltas: Vec<Delta>,
    /// Accounts the run tracked.
    pub accounts: Vec<u8>,
}

impl std::fmt::Debug for Run {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Run")
            .field("deltas", &self.deltas.len())
            .finish_non_exhaustive()
    }
}

/// The record handed to `JournalLedger::apply`: Multi records it reads raw (ControlCmd) get a
/// single-part `PartHdr` in front of the logical body.
fn feed(jl: &mut JournalLedger, rec: &Rec<'_>, scratch: &mut Vec<u8>) -> Res<()> {
    let multi = matches!(
        hft_event::kind_info(EventKind(rec.kind)).map(|i| i.layout),
        Some(hft_event::Layout::Multi { .. })
    );
    let body: &[u8] = if multi && rec.kind == EventKind::CONTROL_CMD.0 {
        let n = rec.body.len() as u32;
        let ph = PartHdr {
            total_len: n,
            chunk_len: n,
            part: 0,
            parts: 1,
            _rsv: 0,
        };
        scratch.clear();
        scratch.extend_from_slice(ph.as_bytes());
        scratch.extend_from_slice(rec.body);
        scratch
    } else {
        rec.body
    };
    let hdr = RecordHeader {
        len: (24 + body.len()) as u32,
        kind: rec.kind,
        flags: rec.flags,
        seq: rec.seq,
        tsc: rec.tsc,
    };
    jl.apply(&RecordView { hdr, body })
        .map_err(|e| Error::journal(format!("ledger settlement: {e:?}"), None, Some(rec.seq)))
}

/// The latest ledger snapshot at or before `t0` (proc time): `(segment, seq)`.
fn snapshot_before(j: &Journal, t0: i64) -> Res<Option<(u64, u64)>> {
    let mut sc = j.scan(j.start())?;
    let mut best = None;
    while let Step::Rec(r) = sc.step()? {
        if r.kind == EventKind::STATE_SNAPSHOT.0 && r.proc_ns <= t0 && has_ledger(r.body) {
            best = Some((r.segment, r.seq));
        }
    }
    Ok(best)
}

fn fresh(s: &Setup) -> Res<JournalLedger> {
    let l = Ledger::new(&s.rd, &s.cfg, &s.caps).map_err(|e| Error::value(format!("ledger config: {e:?}")))?;
    let mut w = SnapshotWriter::new(64 << 20);
    l.snapshot(&mut w);
    let bytes = w.as_bytes().to_vec();
    let mut r = SnapshotReader::new(&bytes);
    JournalLedger::from_snapshot(&mut r, &s.rd, &s.cfg, &s.caps)
        .map_err(|e| Error::value(format!("ledger start: {e:?}")))
}

fn from_snap(s: &Setup, body: &[u8]) -> Res<JournalLedger> {
    let mut r = SnapshotReader::new(sections(body));
    JournalLedger::from_snapshot(&mut r, &s.rd, &s.cfg, &s.caps)
        .map_err(|e| Error::journal(format!("ledger snapshot does not restore: {e:?}"), None, None))
}

fn set_groups(jl: &mut JournalLedger, c: &Config) {
    for (st, groups) in group_tables(c) {
        jl.ledger_mut()
            .on_groups_changed(st, &GroupTable::new(&groups, GROUP_CAP));
    }
}

/// Runs the cold ledger over `[t0, t1)` (README "2. ledger / pnl"); `accounts` = tracked accounts
/// (`None` = every account of the refdata).
pub fn run(j: &Journal, accounts: Option<Vec<u8>>, t0: Option<i64>, t1: Option<i64>) -> Res<Run> {
    let snap = match t0 {
        Some(t) => snapshot_before(j, t)?,
        None => None,
    };
    let start = match snap {
        Some((seg, _)) => StartAt::SegmentBoot(seg),
        None => j.start(),
    };
    let mut sc = j.scan(start)?;
    let mut tracker = Tracker::default();
    let mut jl: Option<JournalLedger> = None;
    let mut rd: Option<Arc<RefData>> = None;
    let mut tracked: Vec<u8> = accounts.clone().unwrap_or_default();
    let mut deltas: Vec<Delta> = Vec::new();
    let mut groups: Vec<CellKey> = Vec::new();
    let mut before: Vec<(CellKey, PnlBreakdown, u32)> = Vec::new();
    let mut ev_first: Option<i64> = None;
    let mut scratch = Vec::new();
    let refresh_groups = |c: &Config, tracked: &[u8], groups: &mut Vec<CellKey>| {
        groups.clear();
        for a in tracked {
            groups.push(CellKey {
                account: *a,
                group: GROUP_NONE.0,
            });
            for (_, gs) in group_tables(c) {
                for g in gs {
                    groups.push(CellKey {
                        account: *a,
                        group: g.id.0,
                    });
                }
            }
        }
        groups.sort();
        groups.dedup();
    };
    while let Step::Rec(rec) = sc.step()? {
        let change = tracker.feed(&rec);
        let snap_here = snap.is_some_and(|(_, s)| s == rec.seq);
        let boot_here = snap.is_none() && jl.is_none() && change == Change::Boot;
        let boot_snap = snap.is_none()
            && jl.is_none()
            && rec.in_boot
            && rec.kind == EventKind::STATE_SNAPSHOT.0
            && has_ledger(rec.body);
        if jl.is_none() {
            if snap_here || boot_snap || boot_here {
                let c = tracker.current();
                let s = setup(&c)?;
                let mut l = if snap_here || boot_snap {
                    from_snap(&s, rec.body)?
                } else {
                    fresh(&s)?
                };
                set_groups(&mut l, &c);
                if accounts.is_none() {
                    tracked = (1..=254u8)
                        .filter(|a| s.rd.account(AccountId(*a)).id == AccountId(*a))
                        .collect();
                }
                refresh_groups(&c, &tracked, &mut groups);
                rd = Some(s.rd.clone());
                jl = Some(l);
            }
            continue;
        }
        let Some(l) = jl.as_mut() else { continue };
        if let Change::Applied(b) = &change
            && b.kind == blob_kind::STRATEGY_PARAMS
        {
            let c = tracker.current();
            set_groups(l, &c);
            refresh_groups(&c, &tracked, &mut groups);
        }
        if change == Change::Boot {
            let c = tracker.current();
            set_groups(l, &c);
            refresh_groups(&c, &tracked, &mut groups);
        }
        if ev_first.is_none() {
            if t1.is_some_and(|t| rec.proc_ns >= t) && !rec.in_boot {
                break;
            }
            ev_first = Some(rec.proc_ns);
            before.clear();
            for k in &groups {
                let g = l.ledger().group(AccountId(k.account), GroupId(k.group));
                before.push((*k, g.pnl, g.round.round_id));
            }
        }
        feed(l, &rec, &mut scratch)?;
        if RecordFlags(rec.flags).contains(RecordFlags::EVENT_END) {
            let at = ev_first.take().unwrap_or(rec.proc_ns);
            let inside = t0.is_none_or(|t| at >= t) && t1.is_none_or(|t| at < t);
            if inside {
                for (k, p, _) in &before {
                    let g = l.ledger().group(AccountId(k.account), GroupId(k.group));
                    let d = sub(&g.pnl, p);
                    if d != PnlBreakdown::zero() {
                        deltas.push(Delta {
                            key: *k,
                            round: g.round.round_id,
                            proc_ns: at,
                            d,
                        });
                    }
                }
            }
        }
    }
    let (Some(jl), Some(rd)) = (jl, rd) else {
        return Err(Error::journal(
            "no complete boot block or ledger snapshot to start the ledger from",
            Some(j.spec.path.clone()),
            None,
        ));
    };
    Ok(Run {
        jl,
        rd,
        deltas,
        accounts: tracked,
    })
}

/// `"Live"` / `"Sim"` of an account.
pub fn account_kind(rd: &RefData, a: u8) -> Option<&'static str> {
    let m = rd.account(AccountId(a));
    (m.id == AccountId(a)).then_some(match m.kind {
        AccountKind::Live => "Live",
        AccountKind::Sim(_) => "Sim",
    })
}

/// Venues an account is enabled on (refdata bitmap, ascending).
pub fn account_venues(rd: &RefData, a: u8) -> Vec<VenueId> {
    let m = rd.account(AccountId(a));
    (0..64u8)
        .filter(|v| m.venues & (1u64 << v) != 0)
        .map(VenueId)
        .collect()
}

/// Non-zero positions of `a`.
pub fn positions(r: &Run, a: u8) -> Vec<(VenueId, u32, InstPos)> {
    let mut out = Vec::new();
    for v in account_venues(&r.rd, a) {
        for i in r.rd.insts_of_venue(v) {
            let p = *r.jl.ledger().pos(AccountId(a), *i);
            if p.qty.0 != 0 || p.cost != 0 || p.realized_quote != 0 || p.fees_quote != 0 {
                out.push((v, i.0, p));
            }
        }
    }
    out
}

/// Sum of the window's deltas of account `a`.
pub fn breakdown(r: &Run, a: u8) -> PnlBreakdown {
    let mut s = PnlBreakdown::zero();
    for d in r.deltas.iter().filter(|d| d.key.account == a) {
        add(&mut s, &d.d);
    }
    s
}

/// `pnl(by)` keys.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum By {
    Group,
    Account,
    Round,
    Day,
}

impl By {
    pub fn parse(s: &str) -> Res<By> {
        match s {
            "group" => Ok(By::Group),
            "account" => Ok(By::Account),
            "round" => Ok(By::Round),
            "day" => Ok(By::Day),
            _ => Err(Error::value(format!(
                "by `{s}`: expected \"group\", \"account\", \"round\" or \"day\""
            ))),
        }
    }
}

/// One `pnl` row key: `(account, group, round, utc_day)` (unused parts 0).
pub type PnlKey = (u8, u32, u32, i64);

/// `pnl(j, by)` rows, ascending by key.
pub fn pnl_rows(r: &Run, by: By) -> Vec<(PnlKey, PnlBreakdown)> {
    let mut m: BTreeMap<PnlKey, PnlBreakdown> = BTreeMap::new();
    for d in &r.deltas {
        let day = d.proc_ns.div_euclid(86_400_000_000_000);
        let key = match by {
            By::Group => (d.key.account, d.key.group, 0, 0),
            By::Account => (d.key.account, 0, 0, 0),
            By::Round => (d.key.account, d.key.group, d.round, 0),
            By::Day => (d.key.account, 0, 0, day),
        };
        add(m.entry(key).or_insert(PnlBreakdown::zero()), &d.d);
    }
    m.into_iter().collect()
}

/// Ledger sections 1, 13 and 22 of the final state (`_testing.ledger_snapshot`).
pub fn snapshot_bytes(r: &Run) -> Vec<u8> {
    let mut w = SnapshotWriter::new(64 << 20);
    r.jl.ledger().snapshot(&mut w);
    w.as_bytes().to_vec()
}
