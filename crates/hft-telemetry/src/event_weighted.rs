//! Event-weighted offline statistics over every market record of a seq journal, burst/calm strata,
//! the `kernel_rx` upper bound and the stratified probe composition (README "按事件加权的离线统计";
//! formulas and thresholds: testing §8.2 item 9, latency-budget §2.1 and §6).

use std::collections::{BTreeMap, HashMap, VecDeque};

use hdrhistogram::Histogram;
use hft_event::{ClockStatus, EventKind, HostProfile, LatencySample, MarketHdr, PartHdr, RecordFlags};
use hft_journal::{JournalReader, Next};
use hft_refdata::RefData;
use hft_types::{InstId, VenueId};
use serde::Serialize;

use crate::clock::{Bracket, CalQueue, ClockCal, ClockStatusRow, Domain};
use crate::hist::{Acc, HistParams, P99_MIN_N, Quantiles};
use crate::stages::{Stage, TriggerClass, TsSrc, trigger_class};
use crate::{Dataset, HistKey, InstClass, TelemetryError, acct_kind, inst_class, read_body};

/// Configuration of `event_weighted` / `stratified_probe`.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub struct EventWeightedCfg {
    /// Inclusive seq range of the market records counted (or converted by the caller from UTC).
    pub from_seq: u64,
    pub to_seq: u64,
    /// latency-budget §2.1 threshold (same as `TelemetryConfig`).
    pub cal_max_err_ns: u32,
    /// Burst/calm threshold (value owned by testing §8.2, passed by the caller).
    pub burst_gap_ns: u32,
    /// Key cardinality limit; samples of later new keys are dropped and counted `series_overflow`.
    pub max_series: u32,
}

/// Burst / calm stratum of a market record (testing §8.2 item 9).
#[derive(Copy, Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Hash, Serialize)]
pub enum Stratum {
    Burst,
    Calm,
}

impl Stratum {
    fn ix(self) -> usize {
        match self {
            Stratum::Burst => 0,
            Stratum::Calm => 1,
        }
    }
}

/// Stratum of one market record: `frames_in_read` = market records on the same connection
/// `(venue, MarketHdr.conn)` with the same `rx_tsc` (itself included, ≥ 1). Burst when ≥ 2 or
/// when `rx_empty_gap_ns > burst_gap_ns`; calm otherwise.
pub fn stratum(frames_in_read: u16, rx_empty_gap_ns: u32, burst_gap_ns: u32) -> Stratum {
    if frames_in_read >= 2 || rx_empty_gap_ns > burst_gap_ns {
        Stratum::Burst
    } else {
        Stratum::Calm
    }
}

/// Key of the event-weighted histograms; its dataset is always `Dataset::EventWeighted`.
#[derive(Copy, Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub struct EwKey {
    pub venue: VenueId,
    pub kind: EventKind,
    /// `kernel_rx` | `ingest` | `feed_host` | `nic_to_kernel` | `kernel_to_user`.
    pub stage: Stage,
    pub ts_src: TsSrc,
    pub inst_class: InstClass,
    pub stratum: Stratum,
}

impl EwKey {
    /// The dataset label of every event-weighted series.
    pub const fn dataset(&self) -> Dataset {
        Dataset::EventWeighted
    }
}

impl Serialize for EwKey {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeStruct;
        let mut st = s.serialize_struct("EwKey", 6)?;
        st.serialize_field("venue", &self.venue.0)?;
        st.serialize_field("kind", &self.kind.0)?;
        st.serialize_field("stage", &self.stage)?;
        st.serialize_field("ts_src", &self.ts_src)?;
        st.serialize_field("inst_class", &self.inst_class)?;
        st.serialize_field("stratum", &self.stratum)?;
        st.end()
    }
}

/// One recorded sample (for `event_weighted_with`): `hi` = a `kernel_rx` upper-bound sample.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub struct EwSample {
    pub seq: u64,
    pub key: EwKey,
    pub value: i64,
    pub hi: bool,
}

/// Report of one dataset (segment-head `HostProfile`).
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct EventWeightedReport {
    pub hist: Vec<(EwKey, Quantiles)>,
    /// `kernel_rx` upper bound = `rx_empty_gap_ns` + `nic_to_kernel` (HW); SW keys hold
    /// `rx_empty_gap_ns` only (no NIC → softirq part, see `kernel_rx_hi_sw_gap_only`).
    pub kernel_rx_hi: Vec<(EwKey, Quantiles)>,
    /// `kernel_rx_hi` has SW keys, whose bound does not include the NIC → softirq part.
    pub kernel_rx_hi_sw_gap_only: bool,
    pub n: u64,
    pub missing: u64,
    /// Records with `rx_empty_gap_ns = 0` (no upper bound).
    pub gap_missing: u64,
    pub negative: u64,
    pub unsynced_cal: u64,
    /// `(stratum, unsynced_cal, n)` for Burst then Calm.
    pub unsynced_cal_by_stratum: [(Stratum, u64, u64); 2],
    /// Distribution of the per-record error bound (key stage = `kernel_rx`).
    pub cal_err: Vec<(EwKey, Quantiles)>,
    pub series_overflow: u64,
    #[serde(serialize_with = "hex32")]
    pub host_profile: [u8; 32],
    pub release: String,
}

fn hex32<S: serde::Serializer>(v: &[u8; 32], s: S) -> Result<S::Ok, S::Error> {
    let h: String = v.iter().map(|b| format!("{b:02x}")).collect();
    s.serialize_str(&h)
}

/// Market kinds counted (records with `MarketHdr`: Bbo, Trade, Mark/Index, Funding, L2Snapshot,
/// L2Batch).
fn counted_kind(k: EventKind) -> bool {
    matches!(
        k,
        EventKind::BBO
            | EventKind::TRADE
            | EventKind::MARK_PRICE
            | EventKind::INDEX_PRICE
            | EventKind::FUNDING
            | EventKind::L2_SNAPSHOT
            | EventKind::L2_BATCH
    )
}

/// One market record waiting for its stratum and calibration.
#[derive(Copy, Clone, Debug)]
struct Rec {
    seq: u64,
    kind: EventKind,
    /// Record-header `tsc` = `proc_tsc`.
    tsc: u64,
    flags: u16,
    h: MarketHdr,
}

impl Rec {
    fn anchor(&self) -> u64 {
        if self.h.rx_tsc != 0 {
            self.h.rx_tsc
        } else {
            self.tsc
        }
    }
    fn group(&self) -> (u8, u8, u64) {
        (self.h.venue.0, self.h.conn.0, self.h.rx_tsc)
    }
}

/// A finalized market record.
#[derive(Copy, Clone, Debug)]
struct Final {
    rec: Rec,
    stratum: Stratum,
    cal: ClockCal,
    /// A real interpolation interval (`false`: none, or the journal ended before the next row).
    bracketed: bool,
}

/// Scanner events.
enum Ev<'a> {
    /// First `HostProfile` of the current dataset.
    Profile {
        hash: [u8; 32],
        release: String,
    },
    /// The `HostProfile` changed: the current dataset ended (after `End`), a new one starts.
    NewDataset {
        hash: [u8; 32],
        release: String,
    },
    Market(Final),
    Sample {
        s: LatencySample,
    },
    /// A `ClockStatus` arrived (after finalizing what it completes).
    Clock(&'a CalQueue),
    /// End of the dataset (journal end or profile change); everything pending was finalized.
    End(&'a CalQueue),
}

/// Text of a fixed-size NUL-padded byte field.
fn text(b: &[u8]) -> String {
    let end = b.iter().position(|&c| c == 0).unwrap_or(b.len());
    String::from_utf8_lossy(&b[..end]).into_owned()
}

struct Scan {
    cfg: EventWeightedCfg,
    cal: CalQueue,
    pending: VecDeque<Rec>,
    groups: HashMap<(u8, u8, u64), u16>,
    hz_milli: u64,
    profile: Option<[u8; 32]>,
}

impl Scan {
    fn new(cfg: &EventWeightedCfg) -> Scan {
        Scan {
            cfg: *cfg,
            cal: CalQueue::default(),
            pending: VecDeque::new(),
            groups: HashMap::new(),
            hz_milli: 0,
            profile: None,
        }
    }

    /// A calibration carrying only the nominal frequency (pure TSC segments still convert).
    fn hz_only(&self) -> ClockCal {
        let hz = self.cal.last().map_or(self.hz_milli, |r| {
            if r.tsc_hz_milli != 0 {
                r.tsc_hz_milli
            } else {
                self.hz_milli
            }
        });
        ClockCal::new(
            None,
            ClockStatusRow {
                tsc_hz_milli: hz,
                ..ClockStatusRow::default()
            },
            None,
            self.cfg.cal_max_err_ns,
        )
    }

    /// Finalizes pending records with anchor ≤ `upto` (`None` = all, the dataset ends).
    fn finalize<F: FnMut(Ev<'_>)>(&mut self, upto: Option<u64>, f: &mut F) {
        let (done, keep): (Vec<Rec>, Vec<Rec>) = std::mem::take(&mut self.pending)
            .into_iter()
            .partition(|r| upto.is_none_or(|u| r.anchor() <= u));
        self.pending = keep.into();
        let frames: Vec<u16> = done
            .iter()
            .map(|r| self.groups.get(&r.group()).copied().unwrap_or(1).max(1))
            .collect();
        for r in &done {
            if let Some(c) = self.groups.get_mut(&r.group()) {
                *c = c.saturating_sub(1);
                if *c == 0 {
                    self.groups.remove(&r.group());
                }
            }
        }
        for (rec, fr) in done.into_iter().zip(frames) {
            let (cal, bracketed) = match self.cal.bracket(rec.anchor(), self.cfg.cal_max_err_ns) {
                Bracket::Cal(c) => (c, true),
                Bracket::Wait => (
                    self.cal
                        .open_tail(self.cfg.cal_max_err_ns)
                        .unwrap_or_else(|| self.hz_only()),
                    false,
                ),
                Bracket::None => (self.hz_only(), false),
            };
            let st = stratum(fr, rec.h.rx_empty_gap_ns, self.cfg.burst_gap_ns);
            f(Ev::Market(Final {
                rec,
                stratum: st,
                cal,
                bracketed,
            }));
        }
    }

    fn end<F: FnMut(Ev<'_>)>(&mut self, f: &mut F) {
        self.finalize(None, f);
        f(Ev::End(&self.cal));
    }

    fn record<F: FnMut(Ev<'_>)>(
        &mut self,
        seq: u64,
        kind: EventKind,
        tsc: u64,
        flags: u16,
        body: &[u8],
        f: &mut F,
    ) {
        match kind {
            EventKind::HOST_PROFILE => {
                let Some(h) = read_body::<HostProfile>(body) else {
                    return;
                };
                if h.tsc_hz_milli != 0 {
                    self.hz_milli = h.tsc_hz_milli;
                }
                let release = text(&h.release);
                match self.profile {
                    None => {
                        self.profile = Some(h.profile_sha256);
                        f(Ev::Profile {
                            hash: h.profile_sha256,
                            release,
                        });
                    }
                    Some(p) if p != h.profile_sha256 => {
                        self.end(f);
                        self.cal.clear();
                        self.groups.clear();
                        self.profile = Some(h.profile_sha256);
                        f(Ev::NewDataset {
                            hash: h.profile_sha256,
                            release,
                        });
                    }
                    Some(_) => {}
                }
            }
            EventKind::CLOCK_STATUS => {
                if let Some(c) = read_body::<ClockStatus>(body) {
                    self.cal.push(ClockStatusRow::from(&c));
                    // Records before the previous row can no longer gain group members or a
                    // better interval.
                    if let Some(prev) = self.cal.second_last().map(|r| r.tsc) {
                        self.finalize(Some(prev), f);
                    }
                    f(Ev::Clock(&self.cal));
                }
            }
            EventKind::LATENCY_SAMPLE => {
                if let Some(s) = read_body::<LatencySample>(body) {
                    f(Ev::Sample { s });
                }
            }
            k if counted_kind(k) && (self.cfg.from_seq..=self.cfg.to_seq).contains(&seq) => {
                if let Some(h) = read_body::<MarketHdr>(body) {
                    let r = Rec {
                        seq,
                        kind: k,
                        tsc,
                        flags,
                        h,
                    };
                    *self.groups.entry(r.group()).or_insert(0) += 1;
                    self.pending.push_back(r);
                }
            }
            _ => {}
        }
    }

    /// Reads the journal to its current end (or past `to_seq` once nothing is pending).
    fn run<F: FnMut(Ev<'_>)>(mut self, rd: &mut JournalReader, mut f: F) -> Result<(), TelemetryError> {
        loop {
            match rd.poll() {
                Next::Record(v) => {
                    let k = EventKind(v.hdr.kind);
                    let mut body = v.body;
                    if k == EventKind::L2_SNAPSHOT || RecordFlags(v.hdr.flags).contains(RecordFlags::PART) {
                        // An unreassembled part: only part 0 carries the prefix.
                        match read_body::<PartHdr>(body) {
                            Some(p) if p.part == 0 => body = body.get(16..).unwrap_or(&[]),
                            _ => continue,
                        }
                    }
                    if v.hdr.seq > self.cfg.to_seq && self.pending.is_empty() {
                        break;
                    }
                    self.record(v.hdr.seq, k, v.hdr.tsc, v.hdr.flags, body, &mut f);
                }
                Next::Event(e) => {
                    if e.seq > self.cfg.to_seq && self.pending.is_empty() {
                        break;
                    }
                    self.record(e.seq, e.kind, e.tsc, e.flags.0, e.body, &mut f);
                }
                Next::Pending | Next::End => break,
                Next::Gap(g) => return Err(TelemetryError::Gap(format!("{g:?}"))),
                Next::Err(e) => return Err(TelemetryError::Read(format!("{e:?}"))),
            }
        }
        self.end(&mut f);
        Ok(())
    }
}

/// Per-dataset accumulator of `event_weighted`.
struct EwAcc {
    p: HistParams,
    max_series: u32,
    hist: BTreeMap<EwKey, Acc>,
    hi: BTreeMap<EwKey, Acc>,
    cal_err: BTreeMap<EwKey, Acc>,
    sw_hi: bool,
    n: u64,
    missing: u64,
    gap_missing: u64,
    negative: u64,
    unsynced_cal: u64,
    by: [(u64, u64); 2],
    series_overflow: u64,
    host_profile: [u8; 32],
    release: String,
    seen_profile: bool,
}

const EW_PARAMS: HistParams = HistParams {
    sig_digits: 3,
    min_ns: 1,
    max_ns: 10_000_000_000,
};

impl EwAcc {
    fn new(max_series: u32) -> EwAcc {
        EwAcc {
            p: EW_PARAMS,
            max_series,
            hist: BTreeMap::new(),
            hi: BTreeMap::new(),
            cal_err: BTreeMap::new(),
            sw_hi: false,
            n: 0,
            missing: 0,
            gap_missing: 0,
            negative: 0,
            unsynced_cal: 0,
            by: [(0, 0); 2],
            series_overflow: 0,
            host_profile: [0; 32],
            release: String::new(),
            seen_profile: false,
        }
    }

    fn keys(&self) -> usize {
        self.hist.len() + self.hi.len() + self.cal_err.len()
    }

    /// Records into `which` (0 hist, 1 hi, 2 cal_err); `false` when the key was refused.
    fn put(&mut self, which: u8, k: EwKey, v: i64) -> bool {
        let full = self.keys() >= self.max_series as usize;
        let p = self.p;
        let m = match which {
            0 => &mut self.hist,
            1 => &mut self.hi,
            _ => &mut self.cal_err,
        };
        if !m.contains_key(&k) && full {
            self.series_overflow += 1;
            return false;
        }
        m.entry(k).or_insert_with(|| Acc::new(&p)).record(v);
        true
    }

    fn report(self) -> EventWeightedReport {
        let q = |m: BTreeMap<EwKey, Acc>| -> Vec<(EwKey, Quantiles)> {
            m.into_iter()
                .filter_map(|(k, a)| a.quantiles().map(|q| (k, q)))
                .collect()
        };
        EventWeightedReport {
            hist: q(self.hist),
            kernel_rx_hi: q(self.hi),
            kernel_rx_hi_sw_gap_only: self.sw_hi,
            n: self.n,
            missing: self.missing,
            gap_missing: self.gap_missing,
            negative: self.negative,
            unsynced_cal: self.unsynced_cal,
            unsynced_cal_by_stratum: [
                (Stratum::Burst, self.by[0].0, self.by[0].1),
                (Stratum::Calm, self.by[1].0, self.by[1].1),
            ],
            cal_err: q(self.cal_err),
            series_overflow: self.series_overflow,
            host_profile: self.host_profile,
            release: self.release,
        }
    }
}

/// `ts_src` of a market record: SW when the record carries `SW_TS`, missing when `hw_rx_ns = 0`.
fn rec_ts_src(r: &Rec) -> TsSrc {
    if r.h.hw_rx_ns == 0 {
        TsSrc::Missing
    } else if RecordFlags(r.flags).contains(RecordFlags::SW_TS) {
        TsSrc::Sw
    } else {
        TsSrc::Hw
    }
}

fn on_market(acc: &mut EwAcc, f: &Final, rdata: &RefData, visit: &mut impl FnMut(&EwSample)) {
    let r = &f.rec;
    let h = &r.h;
    acc.n += 1;
    let si = f.stratum.ix();
    acc.by[si].1 += 1;
    let ts_src = rec_ts_src(r);
    let key = |stage| EwKey {
        venue: h.venue,
        kind: r.kind,
        stage,
        ts_src,
        inst_class: inst_class(rdata, h.inst),
        stratum: f.stratum,
    };
    let cal = &f.cal;
    let ok = f.bracketed && cal.ok();
    if !ok {
        acc.unsynced_cal += 1;
        acc.by[si].0 += 1;
    }
    if cal.b.is_some() {
        acc.put(2, key(Stage::KernelRx), i64::from(cal.err_bound_ns()));
    }
    let mut emit = |acc: &mut EwAcc, stage: Stage, v: Option<i64>, hi: bool| match v {
        None => acc.missing += 1,
        Some(v) if v < 0 => acc.negative += 1,
        Some(v) => {
            let k = key(stage);
            if acc.put(u8::from(hi), k, v) {
                visit(&EwSample {
                    seq: r.seq,
                    key: k,
                    value: v,
                    hi,
                });
            }
        }
    };
    let hw = (h.hw_rx_ns != 0).then_some(h.hw_rx_ns);
    let rx = (h.rx_tsc != 0).then_some(h.rx_tsc);
    let sw = (h.sw_rx_ns != 0).then_some(h.sw_rx_ns);
    let ingest = rx.map(|x| cal.tsc_ns(x, r.tsc));
    emit(acc, Stage::Ingest, ingest, false);
    if h.rx_empty_gap_ns == 0 {
        acc.gap_missing += 1;
    }
    if !ok {
        return;
    }
    let dom = if ts_src == TsSrc::Sw {
        Domain::Realtime
    } else {
        Domain::Phc
    };
    let kernel_rx = hw.zip(rx).map(|(hv, x)| cal.to_domain(x, dom) - hv);
    let nic = match ts_src {
        TsSrc::Hw => hw
            .zip(sw)
            .map(|(hv, w)| cal.convert(w, Domain::Realtime, dom) - hv),
        // SW data set: hw_rx is the kernel stamp itself; there is no NIC → softirq part.
        _ => None,
    };
    let k2u = rx.zip(sw).map(|(x, w)| cal.to_domain(x, Domain::Realtime) - w);
    emit(acc, Stage::KernelRx, kernel_rx, false);
    emit(
        acc,
        Stage::FeedHost,
        kernel_rx.zip(ingest).map(|(k, i)| k + i),
        false,
    );
    if ts_src == TsSrc::Hw {
        emit(acc, Stage::NicToKernel, nic, false);
    }
    emit(acc, Stage::KernelToUser, k2u, false);
    let gap = i64::from(h.rx_empty_gap_ns);
    if gap != 0 && kernel_rx.is_some() {
        match ts_src {
            TsSrc::Hw => {
                if let Some(n) = nic {
                    emit(acc, Stage::KernelRx, Some(gap + n), true);
                }
            }
            TsSrc::Sw => {
                acc.sw_hi = true;
                emit(acc, Stage::KernelRx, Some(gap), true);
            }
            TsSrc::Missing => {}
        }
    }
}

/// `event_weighted` with a visitor called for every recorded sample (in seq order per dataset).
pub fn event_weighted_with(
    rd: &mut JournalReader,
    rdata: &RefData,
    cfg: &EventWeightedCfg,
    mut visit: impl FnMut(&EwSample),
) -> Result<Vec<EventWeightedReport>, TelemetryError> {
    let mut out = Vec::new();
    let mut acc = EwAcc::new(cfg.max_series);
    Scan::new(cfg).run(rd, |ev| match ev {
        Ev::Profile { hash, release } => {
            acc.host_profile = hash;
            acc.release = release;
            acc.seen_profile = true;
        }
        Ev::NewDataset { hash, release } => {
            let mut next = EwAcc::new(cfg.max_series);
            next.host_profile = hash;
            next.release = release;
            next.seen_profile = true;
            let done = std::mem::replace(&mut acc, next);
            out.push(done.report());
        }
        Ev::Market(f) => on_market(&mut acc, &f, rdata, &mut visit),
        Ev::Sample { .. } | Ev::Clock(_) | Ev::End(_) => {}
    })?;
    if acc.seen_profile || acc.n > 0 {
        out.push(acc.report());
    }
    Ok(out)
}

/// Offline statistics over every market record (README "按事件加权的离线统计"): one report per
/// dataset (segment-head `HostProfile`), in seq order. Read-only: no journal change, no file.
pub fn event_weighted(
    rd: &mut JournalReader,
    rdata: &RefData,
    cfg: &EventWeightedCfg,
) -> Result<Vec<EventWeightedReport>, TelemetryError> {
    event_weighted_with(rd, rdata, cfg, |_| {})
}

/// Result of `stratified_probe`.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct StratifiedReport {
    pub groups: Vec<StratifiedGroup>,
}

/// One probe group: per-stratum sample counts and quantiles (Burst, Calm), the weights (record
/// shares of the two strata in the same window's `event_weighted`) and the composite p50/p99
/// (`None` when a stratum has fewer than `P99_MIN_N` samples).
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct StratifiedGroup {
    pub key: HistKey,
    pub n: [u64; 2],
    pub by_stratum: [Option<Quantiles>; 2],
    pub weight: [f64; 2],
    pub composite_p50_p99: Option<(i64, i64)>,
}

/// Composite quantile (testing §8.2 item 9): the smallest value `v` with
/// `Σ weight_i × F_i(v) ≥ p`, `F_i` = empirical distribution of part `i` (at histogram
/// resolution). Weights are normalized; empty parts are ignored.
pub fn composite_quantile(parts: &[(&Histogram<u64>, f64)], p: f64) -> Option<i64> {
    let parts: Vec<(&Histogram<u64>, f64)> = parts.iter().copied().filter(|(h, _)| !h.is_empty()).collect();
    let wsum: f64 = parts.iter().map(|(_, w)| w).sum();
    if parts.is_empty() || wsum <= 0.0 {
        return None;
    }
    // (value, part, cumulative fraction) steps of every part.
    let mut steps: Vec<(u64, usize, f64)> = Vec::new();
    for (i, (h, _)) in parts.iter().enumerate() {
        let n = h.len() as f64;
        let mut cum = 0u64;
        for v in h.iter_recorded() {
            cum += v.count_at_value();
            steps.push((h.highest_equivalent(v.value_iterated_to()), i, cum as f64 / n));
        }
    }
    steps.sort_by(|a, b| a.0.cmp(&b.0).then(a.1.cmp(&b.1)));
    let mut f = vec![0.0f64; parts.len()];
    let mut i = 0;
    while i < steps.len() {
        let v = steps[i].0;
        while i < steps.len() && steps[i].0 == v {
            f[steps[i].1] = steps[i].2;
            i += 1;
        }
        let total: f64 = parts.iter().zip(&f).map(|((_, w), fi)| w / wsum * fi).sum();
        if total >= p - 1e-9 {
            return Some(v.min(i64::MAX as u64) as i64);
        }
    }
    steps.last().map(|s| s.0.min(i64::MAX as u64) as i64)
}

/// A probe sample waiting for its trigger's stratum and calibration.
#[derive(Copy, Clone, Debug)]
struct ProbeSample {
    s: LatencySample,
    need: u64,
}

/// What `stratified_probe` keeps of each finalized market record.
#[derive(Copy, Clone, Debug)]
struct TrigStratum {
    stratum: Stratum,
    kind: EventKind,
    inst: InstId,
}

/// Records remembered for trigger lookup (FIFO bound).
const STRATA_MAX: usize = 1 << 20;

struct ProbeAcc<'r> {
    rdata: &'r RefData,
    cal_max_err_ns: u32,
    strata: HashMap<u64, TrigStratum>,
    order: VecDeque<u64>,
    pending: Vec<ProbeSample>,
    groups: BTreeMap<HistKey, [Acc; 2]>,
}

impl ProbeAcc<'_> {
    fn sample(&mut self, s: LatencySample, from: u64, to: u64) {
        if !(from..=to).contains(&s.trigger_seq) {
            return;
        }
        let k = EventKind(s.trigger_kind);
        if s.trigger_kind != 0 && trigger_class(k) != TriggerClass::Market {
            return;
        }
        let need = [s.rx_tsc.0, s.proc_tsc.0, s.decided_tsc.0, s.sent_tsc.0]
            .into_iter()
            .max()
            .unwrap_or(0);
        self.pending.push(ProbeSample { s, need });
    }

    fn market(&mut self, f: &Final) {
        self.strata.insert(
            f.rec.seq,
            TrigStratum {
                stratum: f.stratum,
                kind: f.rec.kind,
                inst: f.rec.h.inst,
            },
        );
        self.order.push_back(f.rec.seq);
        while self.order.len() > STRATA_MAX {
            if let Some(o) = self.order.pop_front() {
                self.strata.remove(&o);
            }
        }
    }

    /// Computes the pending samples whose trigger stratum and interval are known; `end` = the
    /// dataset ended (the rest is computed without the next row, i.e. not counted).
    fn settle(&mut self, cal: &CalQueue, end: bool) {
        let last = cal.last().map_or(0, |r| r.tsc);
        let pending = std::mem::take(&mut self.pending);
        for p in pending {
            let Some(t) = self.strata.get(&p.s.trigger_seq).copied() else {
                if !end {
                    self.pending.push(p);
                }
                continue;
            };
            let c = match cal.bracket(p.s.rx_tsc.0, self.cal_max_err_ns) {
                Bracket::Cal(c) if last >= p.need => c,
                Bracket::Cal(_) | Bracket::Wait if !end => {
                    self.pending.push(p);
                    continue;
                }
                _ => continue,
            };
            let mut s = p.s;
            s.trigger_kind = t.kind.0;
            let st = crate::stages::stages(&s, &c);
            let Some(v) = st.get(Stage::TickToTrade) else {
                continue;
            };
            if !st.cal_ok || v < 0 {
                continue;
            }
            let key = HistKey {
                stage: Stage::TickToTrade,
                dataset: Dataset::Live,
                venue: s.venue,
                account: s.account,
                account_kind: acct_kind(self.rdata, s.account),
                inst_class: inst_class(self.rdata, t.inst),
                action: s.action,
                strategy: s.strategy,
                trigger_kind: t.kind,
                trigger_sub: 0,
                ts_src: st.ts_src,
                other: false,
            };
            let g = self
                .groups
                .entry(key)
                .or_insert_with(|| [Acc::new(&EW_PARAMS), Acc::new(&EW_PARAMS)]);
            g[t.stratum.ix()].record(v);
        }
    }
}

/// Probe samples by stratum (README "按事件加权的离线统计"): market-triggered `LatencySample`s
/// whose trigger record is in `[from_seq, to_seq]`, grouped by `HistKey` (stage `tick_to_trade`),
/// split by the trigger record's stratum; weights from `ew` (record shares of the two strata);
/// composite p50/p99 only when both strata have ≥ `P99_MIN_N` samples. Read-only.
pub fn stratified_probe(
    rd: &mut JournalReader,
    rdata: &RefData,
    cfg: &EventWeightedCfg,
    ew: &EventWeightedReport,
) -> Result<StratifiedReport, TelemetryError> {
    let mut acc = ProbeAcc {
        rdata,
        cal_max_err_ns: cfg.cal_max_err_ns,
        strata: HashMap::new(),
        order: VecDeque::new(),
        pending: Vec::new(),
        groups: BTreeMap::new(),
    };
    let (from, to) = (cfg.from_seq, cfg.to_seq);
    Scan::new(cfg).run(rd, |ev| match ev {
        Ev::Market(f) => acc.market(&f),
        Ev::Sample { s } => acc.sample(s, from, to),
        Ev::Clock(c) => acc.settle(c, false),
        Ev::End(c) => acc.settle(c, true),
        Ev::Profile { .. } | Ev::NewDataset { .. } => {}
    })?;
    let nb = ew.unsynced_cal_by_stratum[0].2 as f64;
    let nc = ew.unsynced_cal_by_stratum[1].2 as f64;
    let weight = if nb + nc > 0.0 {
        [nb / (nb + nc), nc / (nb + nc)]
    } else {
        [0.0, 0.0]
    };
    let groups = acc
        .groups
        .into_iter()
        .map(|(key, [b, c])| {
            let n = [b.h.len(), c.h.len()];
            let composite = (n[0] >= P99_MIN_N && n[1] >= P99_MIN_N)
                .then(|| {
                    let parts = [(&b.h, weight[0]), (&c.h, weight[1])];
                    Some((
                        composite_quantile(&parts, 0.5)?,
                        composite_quantile(&parts, 0.99)?,
                    ))
                })
                .flatten();
            StratifiedGroup {
                key,
                n,
                by_stratum: [b.quantiles(), c.quantiles()],
                weight,
                composite_p50_p99: composite,
            }
        })
        .collect();
    Ok(StratifiedReport { groups })
}
