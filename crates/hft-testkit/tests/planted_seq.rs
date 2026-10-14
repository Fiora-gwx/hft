//! TK-179: planted defects of the real engine crates (feature `planted`) are caught by the DST on
//! the real sequencer within a bounded seed set, and the same seed passes without the defect.

use hft_testkit::dst::faults::{Fault, FaultRates};
use hft_testkit::dst::invariants::InvId;
use hft_testkit::dst::world::{VP, VS};
use hft_testkit::dst::{
    DstConfig, FailKind, InputRec, MarketSource, RerunPolicy, RunRecord, Source, run_record,
};
use hft_testkit::scenarios::{SeqEngine, suites};

/// Seeds tried per row, from 1.
pub const MAX_SEEDS: u64 = 16;
/// Steps per run.
pub const STEPS: u64 = 2_000;

/// The suite `name` configuration of `seed`, `STEPS` long, without the INV-9 rerun.
pub fn suite_cfg(name: &str, seed: u64) -> DstConfig {
    let s = suites().into_iter().find(|s| s.name == name).unwrap();
    let mut c = s.config(seed);
    c.steps = STEPS;
    c.rerun = RerunPolicy::Never;
    c
}

/// Core suite with a group position limit below one first-leg lot: every opening is over it.
fn tight_limit(seed: u64) -> DstConfig {
    let mut c = suite_cfg("core", seed);
    for g in &mut c.world.groups {
        g.max_pos_qty = 50_000;
    }
    c
}

/// Storage suite (crashes, clean restarts) with a rebuild check every 200 steps.
fn rebuilds(seed: u64) -> DstConfig {
    let mut c = suite_cfg("storage", seed);
    c.rebuild_every = 200;
    c
}

/// Core suite with every fill's fee split into a late fee-only report on both venues.
fn late_fees(seed: u64) -> DstConfig {
    let mut c = suite_cfg("core", seed);
    for k in 0..200u64 {
        for venue in [VS, VP] {
            c.scripted
                .push((k * 1_000_000_000, Fault::LateFeeOnly { venue, delay_ms: 50 }));
        }
    }
    c
}

/// Core suite where the hedge leg's market data (spot `VS`; the probe opens on the perp) goes
/// down right after the opening, so the second leg cannot be priced and the cell turns Broken at
/// its hedge deadline; Broken cells start their bounded auto fix after 2 s.
fn broken(seed: u64) -> DstConfig {
    let mut c = suite_cfg("core", seed);
    c.world.risk.broken_auto_fix_ms = 2_000;
    c.scripted.push((
        45_000_000,
        Fault::ConnDown {
            venue: VS,
            conn: hft_testkit::dst::faults::ConnKind::Md,
            down_ms: 20_000,
            resync: false,
        },
    ));
    c
}

/// [`broken`] with a reduce notional cap below one perp lot: the bounded auto fix cannot place a
/// slice and ends `Failed` while the cell stays Broken.
fn broken_fails(seed: u64) -> DstConfig {
    let mut c = broken(seed);
    c.world.risk.max_reduce_notional = 30_000_000;
    c
}

/// A probe-target `PairOnFill` on g1 (first leg `S1` buy on spot, second leg `P1` sell on the
/// perp, whose venue supports reduceOnly) whose first second leg the venue rejects: the link
/// retries with a `SecondRetry`, which must not carry reduceOnly (an opening short with
/// reduceOnly is refused). On a spot hedge leg the sequencer never sends reduceOnly (hft-risk
/// `classify` folds in `VenueCaps.reduce_only`), so the defect needs the perp hedge leg.
fn retry(seed: u64) -> DstConfig {
    use hft_strategy_sdk::params::{RawParams, SchemaView, encode_body};
    use hft_testkit::strategies::TargetParams;
    let mut c = suite_cfg("core", seed);
    c.faults = FaultRates::off();
    c.world.accounts.truncate(1);
    c.world.groups.truncate(1);
    c.world.strategies.truncate(1);
    if let Some(i) = c
        .world
        .insts
        .iter_mut()
        .find(|i| i.id == hft_testkit::dst::world::S1)
    {
        i.lot = 100_000;
        i.min_qty = 100_000;
    }
    let schema = &TargetParams::SCHEMA;
    let defaults = (0..schema.n_fields())
        .map(|ix| {
            let f = schema.field(ix).expect("probe-target field");
            if f.name == "t_pair_ms" { 1_000 } else { f.default }
        })
        .collect();
    c.world.strategies[0].kind_id = 0xFF02;
    c.world.strategies[0].params = encode_body(
        schema,
        1,
        &RawParams {
            defaults,
            overrides: Vec::new(),
        },
    )
    .expect("probe-target parameters");
    c.scripted = vec![(0, Fault::SecondLegReject { venue: VP })];
    c
}

/// [`late_fees`] without INV-2, whose FeeOnly clause otherwise reports the defect first.
fn late_fees_no_i2(seed: u64) -> DstConfig {
    let mut c = late_fees(seed);
    c.invariants = hft_testkit::dst::InvariantSet::all().without(InvId::I2);
    c
}

/// One row: id, planted defect, configuration, the invariant its first failure must report.
type Row = (&'static str, &'static str, fn(u64) -> DstConfig, InvId);

pub const ROWS: &[Row] = &[
    ("TK-179.1", "risk-no-exposure-limit", tight_limit, InvId::I3),
    ("TK-179.2", "snapshot-field", rebuilds, InvId::I6),
    ("TK-179.3", "ignore-fee", late_fees, InvId::I2),
    ("TK-179.4", "ignore-fee", late_fees_no_i2, InvId::I16),
    ("TK-179.5", "broken-never-autofix", broken, InvId::I20),
    ("TK-179.6", "broken-repeat-autofix", broken_fails, InvId::I20),
];

/// First seed in `1..=MAX_SEEDS` whose run of `make(seed)` with `planted` fails; the failure.
pub fn first_failure(planted: &'static str, make: fn(u64) -> DstConfig) -> Option<(u64, FailKind, String)> {
    for seed in 1..=MAX_SEEDS {
        let mut c = make(seed);
        c.planted = vec![planted];
        let r = run_record::<SeqEngine>(&c);
        if let Some(f) = r.outcome.failure() {
            return Some((seed, f.invariant, f.message.clone()));
        }
    }
    None
}

/// Link role of a second leg / of its retry in `OrderDecision.link_role`.
const ROLE_SECOND: u8 = 2;
const ROLE_SECOND_RETRY: u8 = 3;
const SECOND_NS: u64 = 1_000_000_000;

/// `Note.code` of `OmsHedgePxStale` (`source << 24 | n`).
fn hedge_px_stale(n: &hft_event::Note) -> bool {
    n.source == hft_event::NoteSource::Oms && n.code & 0xFF_FFFF == 17
}

/// QF-1 world: one account and group, no random faults, recorded quotes every 100 ms on the perp
/// `P1` (the probe's first leg) and a single spot `S1` quote at 100 ms: the hedge book stays quiet
/// while `VS` keeps answering pings. At 12 s the perp bid lifts over the spot ask and the probe
/// opens, 11.9 s after the last spot quote.
pub fn quiet_hedge_book(seed: u64) -> DstConfig {
    use hft_testkit::dst::world::{P1, S1};
    use zerocopy::FromZeros;
    let mut c = suite_cfg("core", seed);
    c.faults = FaultRates::off();
    c.world.accounts.truncate(1);
    c.world.groups.truncate(1);
    c.world.strategies.truncate(1);
    c.horizon_ns = 30 * SECOND_NS;
    let mut inputs = Vec::new();
    for tick in 1..=290u64 {
        let ns = tick * SECOND_NS / 10;
        for i in &c.world.insts {
            if i.id == S1 && tick > 1 {
                continue;
            }
            // The perp trades 0.03 % under the spot ask until 12 s, then 0.15 % over it.
            let lift = match (i.id == P1, ns >= 12 * SECOND_NS) {
                (false, _) => 0,
                (true, false) => -380_000,
                (true, true) => 800_000,
            };
            let mut h = hft_event::MarketHdr::new_zeroed();
            h.inst = i.id;
            h.venue = i.venue;
            h.ex_ts_ns = ns as i64;
            let b = hft_event::Bbo {
                h,
                bid_px: hft_types::Px(i.bid + lift),
                bid_qty: hft_types::Qty(i.depth),
                ask_px: hft_types::Px(i.ask + lift),
                ask_qty: hft_types::Qty(i.depth),
                update_id: tick,
            };
            inputs.push((ns, InputRec::of(&b, Source::Market)));
        }
    }
    inputs.sort_by_key(|r| r.0);
    c.market = MarketSource::Recorded(inputs);
    c
}

/// MS-2 world (test-strategy version, scaled to the W1 lots): one account; the probe opens one
/// 20-lot perp short (`P1`, 1 200 USDT) hedged by spot, `close_on = 0` (only the engine reduces);
/// the perp venue holds 360 USDT (margin / notional = 0.30). Both quotes rise 30 % linearly over
/// 600 s, then hold.
pub fn one_sided_30(seed: u64) -> DstConfig {
    use hft_strategy_sdk::params::{RawParams, SchemaView, encode_body};
    use hft_testkit::dst::world::{P1, USDT};
    use hft_testkit::strategies::TakerParams;
    use zerocopy::FromZeros;
    let mut c = suite_cfg("core", seed);
    c.faults = FaultRates::off();
    c.world.accounts.truncate(1);
    c.world.groups.truncate(1);
    c.world.strategies.truncate(1);
    c.world.groups[0].max_pos_qty = 2_000_000;
    for b in &mut c.world.accounts[0].balances {
        if b.0 == VP && b.1 == USDT {
            b.2 = 360_000_000;
        }
    }
    let schema = &TakerParams::SCHEMA;
    let defaults = (0..schema.n_fields())
        .map(|ix| {
            let f = schema.field(ix).expect("taker field");
            match f.name {
                "qty_lots" => 20,
                "close_on" => 0,
                _ => f.default,
            }
        })
        .collect();
    c.world.strategies[0].kind_id = 0xFF01;
    c.world.strategies[0].params = encode_body(
        schema,
        1,
        &RawParams {
            defaults,
            overrides: Vec::new(),
        },
    )
    .expect("taker parameters");
    const RISE_S: u64 = 600;
    const END_S: u64 = 720;
    c.horizon_ns = END_S * SECOND_NS;
    c.steps = 60_000;
    let mut inputs = Vec::new();
    for tick in 1..=END_S * 10 {
        let ns = tick * SECOND_NS / 10;
        let up_bp = i64::try_from((tick.min(RISE_S * 10) * 3_000) / (RISE_S * 10)).unwrap();
        for i in &c.world.insts {
            // The perp opens at a 0.15 % premium over the spot ask.
            let (bid, ask) = if i.id == P1 {
                (600_900_000, 600_901_000)
            } else {
                (i.bid, i.ask)
            };
            let scale = |px: i64| {
                let v = px + px * up_bp / 10_000;
                v - v.rem_euclid(i.tick)
            };
            let mut h = hft_event::MarketHdr::new_zeroed();
            h.inst = i.id;
            h.venue = i.venue;
            h.ex_ts_ns = ns as i64;
            let b = hft_event::Bbo {
                h,
                bid_px: hft_types::Px(scale(bid)),
                bid_qty: hft_types::Qty(i.depth),
                ask_px: hft_types::Px(scale(ask)),
                ask_qty: hft_types::Qty(i.depth),
                update_id: tick,
            };
            inputs.push((ns, InputRec::of(&b, Source::Market)));
            if i.id == P1 && tick % 10 == 0 {
                // Mark = perp mid, once a second.
                let m = hft_event::MarkPrice {
                    h,
                    px: hft_types::Px((scale(bid) + scale(ask)) / 2),
                };
                inputs.push((ns, InputRec::of(&m, Source::Market)));
            }
        }
    }
    c.market = MarketSource::Recorded(inputs);
    c
}

/// MS-2 assertions: the engine reduced the perp short before any liquidation; no exchange-side
/// perp execution (this world injects no ADL or manual fills: any is the venue's liquidation)
/// reached the journal or the truth.
pub fn ms_2_no_liquidation(r: &RunRecord) -> Result<(), String> {
    use hft_testkit::dst::world::P1;
    let liquidated = r
        .journal
        .iter()
        .filter_map(|j| j.try_read::<hft_event::ExternalFill>())
        .any(|x| x.inst == P1);
    // The exchange side closed the perp short without an engine order (liquidation): the truth
    // position is flat while the engine still holds the short.
    let truth = r
        .truth
        .iter()
        .flat_map(|t| t.positions.iter())
        .filter(|(i, _)| *i == P1)
        .map(|(_, q)| *q)
        .sum::<i128>();
    let engine = r
        .obs
        .positions
        .iter()
        .filter(|p| p.inst == P1)
        .map(|p| p.qty)
        .sum::<i128>();
    if liquidated || (truth == 0 && engine != 0) {
        return Err(format!(
            "the exchange liquidated the account (truth perp {truth}, engine perp {engine})"
        ));
    }
    let opened = r
        .journal
        .iter()
        .filter_map(|j| j.try_read::<hft_event::OrderEvent>())
        .any(|e| e.inst == P1 && e.cum_filled.0 >= 2_000_000);
    if !opened {
        return Err("the 20-lot perp short never opened".into());
    }
    let left = r
        .obs
        .positions
        .iter()
        .filter(|p| p.inst == P1)
        .map(|p| p.qty)
        .sum::<i128>();
    if left.abs() >= 2_000_000 {
        return Err(format!("the engine never reduced the perp short ({left})"));
    }
    Ok(())
}

/// QF-1 core assertions on a quiet hedge book: the opening's fill Input sends the second leg (no
/// later retry), no `OmsHedgePxStale`, no Broken cell.
pub fn qf_1_hedged_in_fill_input(r: &RunRecord) -> Result<(), String> {
    use hft_testkit::dst::world::{P1, S1};
    let fill = r
        .journal
        .iter()
        .find(|j| {
            j.try_read::<hft_event::OrderEvent>().is_some_and(|e| {
                e.inst == P1
                    && matches!(
                        e.what,
                        hft_event::OrderEventKind::Fill | hft_event::OrderEventKind::PartialFill
                    )
            })
        })
        .ok_or("the opening never filled")?;
    let second = r
        .journal
        .iter()
        .filter_map(|j| j.try_read::<hft_event::OrderDecision>().map(|d| (j, d)))
        .find(|(_, d)| d.inst == S1 && d.action == hft_event::OrderAction::New && d.link_role == ROLE_SECOND)
        .ok_or("no second leg was sent")?;
    if second.1.reject != 0 || second.0.seq < fill.seq || second.0.tsc != fill.tsc {
        return Err(format!(
            "second leg seq {} at {} (reject {:#x}) is not part of the fill Input seq {} at {}",
            second.0.seq, second.0.tsc, second.1.reject, fill.seq, fill.tsc
        ));
    }
    if r.journal
        .iter()
        .filter_map(|j| j.try_read::<hft_event::Note>())
        .any(|n| hedge_px_stale(&n))
    {
        return Err("OmsHedgePxStale on a quiet but connected hedge book".into());
    }
    if r.obs.exec.iter().any(|c| c.broken) {
        return Err("a cell is Broken".into());
    }
    Ok(())
}

/// TK-053.23 assertion: after a 1.5 s hedge-quote outage the `SecondRetry` is accepted and the
/// cell never turns Broken.
pub fn retry_accepted(r: &RunRecord) -> Result<(), String> {
    let retries: Vec<_> = r
        .journal
        .iter()
        .filter_map(|j| j.try_read::<hft_event::OrderDecision>())
        .filter(|d| d.action == hft_event::OrderAction::New && d.link_role == ROLE_SECOND_RETRY)
        .collect();
    if retries.is_empty() {
        return Err("no second-leg retry".into());
    }
    if let Some(d) = retries.iter().find(|d| d.reject != 0) {
        return Err(format!("second-leg retry rejected: {:#x}", d.reject));
    }
    if r.obs.exec.iter().any(|c| c.broken) {
        return Err("a cell is Broken".into());
    }
    Ok(())
}

/// One scenario row of TK-053 on the real engine: planted run fails the assertion (all
/// invariants still hold), the healthy run passes invariants and assertion.
pub fn scenario_row(
    row: &str,
    planted: &'static str,
    make: fn(u64) -> DstConfig,
    assert: fn(&RunRecord) -> Result<(), String>,
) -> Option<String> {
    for seed in 1..=MAX_SEEDS {
        let mut c = make(seed);
        c.planted = vec![planted];
        let bad = run_record::<SeqEngine>(&c);
        // The scenario assertion first; an invariant the defect breaks on the way is reported too.
        let inv = bad.outcome.failure().map(|f| {
            format!(
                "; invariant {} ({})",
                f.invariant.label(),
                f.message.lines().next().unwrap_or("")
            )
        });
        let found = assert(&bad)
            .err()
            .map(|e| format!("{e}{}", inv.clone().unwrap_or_default()))
            .or(inv);
        let found = match found {
            Some(e) => e,
            None => continue,
        };
        eprintln!("{row} {planted}: seed {seed} assertion: {found}");
        let healthy = run_record::<SeqEngine>(&make(seed));
        if let Some(f) = healthy.outcome.failure() {
            return Some(format!(
                "{row}: seed {seed} fails without the defect: {} ({})",
                f.invariant.label(),
                f.message
            ));
        }
        if let Err(e) = assert(&healthy) {
            return Some(format!(
                "{row}: seed {seed} assertion fails without the defect: {e}"
            ));
        }
        return None;
    }
    Some(format!("{row} {planted}: not found within {MAX_SEEDS} seeds"))
}

/// TK-053.21–.23 on the real engine (called by `planted::tk_053_planted_defects_found`).
pub fn tk_053_real_rows() -> Vec<String> {
    [
        scenario_row(
            "TK-053.21",
            "hedge-px-book-age",
            quiet_hedge_book,
            qf_1_hedged_in_fill_input,
        ),
        scenario_row(
            "TK-053.22",
            "margin-ratio-only",
            one_sided_30,
            ms_2_no_liquidation,
        ),
        scenario_row("TK-053.23", "retry-reduce-only", retry, retry_accepted),
    ]
    .into_iter()
    .flatten()
    .collect()
}

#[test]
fn tk_179_real_engine_planted_defects_found() {
    let only = std::env::var("HFT_TK179_ROW").ok();
    let mut misses = Vec::new();
    for (row, planted, make, want) in ROWS {
        if only.as_deref().is_some_and(|o| o != *row) {
            continue;
        }
        match first_failure(planted, *make) {
            Some((seed, kind, message)) => {
                eprintln!("{row} {planted}: seed {seed} {} ({message})", kind.label());
                if kind != FailKind::Inv(*want) {
                    misses.push(format!(
                        "{row} {planted}: seed {seed} reported {} ({message})",
                        kind.label()
                    ));
                    continue;
                }
                // The same seed and configuration pass without the defect.
                let healthy = run_record::<SeqEngine>(&make(seed));
                if let Some(f) = healthy.outcome.failure() {
                    misses.push(format!(
                        "{row} {planted}: seed {seed} also fails without the defect: {} ({})",
                        f.invariant.label(),
                        f.message
                    ));
                }
            }
            None => misses.push(format!("{row} {planted}: not found within {MAX_SEEDS} seeds")),
        }
    }
    assert!(misses.is_empty(), "{misses:#?}");
}
