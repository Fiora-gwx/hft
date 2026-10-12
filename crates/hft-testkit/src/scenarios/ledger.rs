//! Ledger acceptance against the real engine and the independently settled exchange.
use hft_event::{
    Bbo, CmdHdr, ControlCmd, EventKind, ExecDecision, ExternalFill, MarketHdr, Note, NoteSource, OrderAction,
    OrderDecision, OrderEvent, OrderEventKind, ReconcileBatchView,
};
use hft_strategy_sdk::params::{RawParams, SchemaView, encode_body, encode_cmd};
use hft_types::{Px, Qty, Side};
use zerocopy::FromZeros;

use crate::dst::actors;
use crate::dst::invariants::{EndView, InvState, check_end};
use crate::dst::world::{ACCT_A, G1, P1, S1, VP, VS};
use crate::dst::{
    ConnKind, DstConfig, Fault, FaultRates, InputRec, InvId, InvariantSet, MarketSource, RunRecord, Scenario,
    SeedSet, Source,
};
use crate::strategies::TargetParams;
use crate::strategies::probe_target::CMDS;

pub(super) const SECOND: u64 = 1_000_000_000;
pub(super) const LOT: i64 = 100_000;

/// Independent case inputs. Consecutive even-length ranges include exactly half late replays.
fn variant(seed: u64) -> (bool, bool, i64) {
    (
        seed & 2 != 0,
        seed & 1 == 0,
        2 * (1 + (crate::seed::derive(seed, "led-081/size") % 3) as i64),
    )
}

pub(super) fn command(c: &DstConfig, at: u64, id: u64, lots: Option<i64>) -> InputRec {
    let desc = &CMDS[if lots.is_some() { 2 } else { 3 }];
    let vals: Vec<_> = lots.into_iter().collect();
    let bytes = encode_cmd(desc, &vals).expect("ledger command schema");
    let mut payload = [0; 256];
    payload[..bytes.len()].copy_from_slice(&bytes);
    actors::encode_cmd(
        &ControlCmd::StrategyCmd {
            instance: c.world.strategies[0].instance,
            reduce_only: u8::from(desc.reduce_only),
            group: G1,
            len: bytes.len() as u16,
            payload,
        },
        "ledger-case",
        CmdHdr::ROLE_ADMIN,
        ACCT_A,
        (crate::dst::world::UTC0 as u64) + at,
        id,
    )
}

pub(super) fn configure_base(c: &mut DstConfig) {
    c.horizon_ns = 90 * SECOND + SECOND / 2;
    c.faults = FaultRates::off();
    c.world.accounts.truncate(1);
    c.world.groups.truncate(1);
    c.world.strategies.truncate(1);
    c.world.oms_tombstone_keep_ms = Some(10_000);
    let first = c.world.insts.iter_mut().find(|i| i.id == S1).expect("W1 spot");
    first.lot = LOT;
    first.min_qty = LOT;
    c.world.strategies[0].kind_id = 0xFF02;
    c.world.strategies[0].params = encode_body(
        &TargetParams::SCHEMA,
        1,
        &RawParams {
            defaults: (0..TargetParams::SCHEMA.n_fields())
                .map(|i| TargetParams::SCHEMA.field(i).expect("field").default)
                .collect(),
            overrides: vec![],
        },
    )
    .expect("ledger parameters");
    let mut inputs = Vec::new();
    for tick in 1..=899 {
        let ns = tick * 100_000_000;
        for i in &c.world.insts {
            let mut h = MarketHdr::new_zeroed();
            h.inst = i.id;
            h.venue = i.venue;
            h.ex_ts_ns = ns as i64;
            let b = Bbo {
                h,
                bid_px: Px(i.bid),
                bid_qty: Qty(i.depth),
                ask_px: Px(i.ask),
                ask_qty: Qty(i.depth),
                update_id: tick,
            };
            inputs.push((ns, InputRec::of(&b, Source::Market)));
        }
    }
    inputs.sort_by_key(|r| r.0);
    c.market = MarketSource::Recorded(inputs);
    c.confirmed_commands.clear();
    c.scripted.clear();
}

fn configure(c: &mut DstConfig) {
    let (full, replay, lots) = variant(c.seed);
    configure_base(c);
    // Complete a real paired round trip before reopening for ADL. Full ADL may close the
    // second position entirely, so it must not make the required PairOnFill close vacuous.
    c.confirmed_commands.clear();
    for (id, seconds, quantity) in [
        (1, 1, Some(lots)),
        (2, 4, None),
        (3, 7, Some(lots)),
        (4, 75, None),
    ] {
        let at = seconds * SECOND;
        c.confirmed_commands.push((at, command(c, at, id, quantity)));
    }
    c.scripted = vec![
        (
            10 * SECOND,
            Fault::ConnDown {
                venue: VP,
                conn: ConnKind::UserStream,
                down_ms: 5_000,
                resync: false,
            },
        ),
        (
            11 * SECOND + crate::seed::derive(c.seed, "led-081/when") % (2 * SECOND),
            Fault::ReconciledAdl {
                account: ACCT_A,
                inst: P1,
                full,
                replay,
            },
        ),
    ];
}

pub(super) fn require(ok: bool, message: &str) -> Result<(), String> {
    if ok { Ok(()) } else { Err(message.into()) }
}

/// Each paired leg must actually fill its requested quantity in the specified window.
pub(super) fn assert_trade_window(
    r: &RunRecord,
    from: u64,
    to: u64,
    side: Side,
    lots: i64,
) -> Result<(), String> {
    let in_window = |tsc: u64| tsc >= from && tsc < to;
    require(
        r.journal
            .iter()
            .filter(|j| in_window(j.tsc))
            .filter_map(|j| j.read::<ExecDecision>())
            .any(|d| {
                d.account == ACCT_A && d.algo == 2 && d.qty.0 > 0 && d.leg_ix == 0 && d.side == side as u8
            }),
        "missing actual PairOnFill open/close decision",
    )?;
    for (inst, order_side) in [(S1, side), (P1, side.opposite())] {
        let decisions: Vec<_> = r
            .journal
            .iter()
            .filter(|j| in_window(j.tsc))
            .filter_map(|j| j.try_read::<OrderDecision>())
            .filter(|d| {
                d.account == ACCT_A
                    && d.inst == inst
                    && d.side == order_side
                    && d.action == OrderAction::New
                    && d.reject == 0
            })
            .collect();
        require(!decisions.is_empty(), "paired leg has no accepted order")?;
        let filled: i64 = r
            .journal
            .iter()
            .filter(|j| in_window(j.tsc))
            .filter_map(|j| j.try_read::<OrderEvent>())
            .filter(|e| {
                e.account == ACCT_A
                    && e.inst == inst
                    && matches!(e.what, OrderEventKind::PartialFill | OrderEventKind::Fill)
                    && e.qty.0 > 0
                    && decisions
                        .iter()
                        .any(|d| d.venue == e.venue && d.slot == e.slot && d.r#gen == e.r#gen)
            })
            .map(|e| e.qty.0)
            .sum();
        require(
            filled == lots * LOT,
            "paired leg did not actually fill the configured quantity",
        )?;
    }
    Ok(())
}

/// Checks actual execution and non-vacuous event comparisons, in addition to every invariant.
pub fn assert_record(r: &RunRecord) -> Result<(), String> {
    let (full, replay, lots) = variant(r.seed);
    require(
        r.end_ns >= 90 * SECOND,
        "scenario ended before the final delivery window",
    )?;
    require(r.snapshot_checks > 0, "no independent snapshot checks")?;
    for (from, to, side) in [
        (SECOND, 4 * SECOND, Side::Buy),
        (4 * SECOND, 7 * SECOND, Side::Sell),
        (7 * SECOND, 10 * SECOND, Side::Buy),
    ] {
        assert_trade_window(r, from, to, side, lots)?;
    }
    let batches: Vec<_> = r
        .journal
        .iter()
        .filter(|j| j.kind == EventKind::RECONCILE_BATCH)
        .collect();
    require(batches.len() == 1, "expected exactly one committed AUTO batch")?;
    let batch = batches[0];
    require(
        r.ledger_checks.contains(&batch.seq),
        "missing exact ledger check at AUTO boundary",
    )?;
    let b = ReconcileBatchView::read(&batch.body).map_err(|e| format!("batch: {e:?}"))?;
    require(b.hdr.n_fills == 1, "AUTO must contain one original execution")?;
    let original = b.fills().next().ok_or("AUTO receipt missing")?;
    require(
        original.account == ACCT_A
            && original.venue == VP
            && original.inst == P1
            && original.qty.0 == lots * LOT / if full { 1 } else { 2 }
            && original.cash_qty == original.qty,
        "AUTO original identity/quantity/cash coverage differs",
    )?;
    let confirms: Vec<_> = r
        .journal
        .iter()
        .filter_map(|j| j.try_read::<ExternalFill>().map(|e| (j.seq, e)))
        .collect();
    require(
        confirms.len() == usize::from(replay),
        "late original delivery count differs",
    )?;
    for (seq, e) in confirms {
        require(
            seq > batch.seq && r.ledger_checks.contains(&seq),
            "late original lacks its own ledger boundary check",
        )?;
        require(
            e.account == original.account
                && e.venue == original.venue
                && e.inst == original.inst
                && e.trade_id == original.trade_id
                && e.side == original.side
                && e.qty == original.qty
                && e.cash_qty == original.cash_qty
                && e.cash_amount == original.cash_amount
                && e.px == original.px
                && e.fee == original.fee
                && e.fee_asset == original.fee_asset
                && e.ex_ts_ns == original.ex_ts_ns,
            "late execution is not the original receipt",
        )?;
    }
    let offsets = r
        .journal
        .iter()
        .filter_map(|j| j.try_read::<Note>())
        .filter(|n| n.source == NoteSource::Ledger && n.code == hft_ledger::notes::code::AUTO_HELD_OFFSET)
        .count();
    require(
        offsets == usize::from(replay),
        "late receipt must offset exactly once on both ledger consumers",
    )?;
    assert_final(r)
}

/// Final delivery and account observations must permit an exact, unskipped truth comparison.
pub(super) fn assert_final(r: &RunRecord) -> Result<(), String> {
    require(
        r.truth.len() == 2
            && [VS, VP]
                .iter()
                .all(|v| r.truth.iter().any(|t| t.account == ACCT_A && t.venue == *v)),
        "missing final account/venue truth",
    )?;
    require(
        r.truth.iter().all(|t| !t.undelivered && t.excluded.is_empty()),
        "final truth still has in-flight or excluded facts",
    )?;
    require(
        r.obs.slots.iter().all(|s| s.state.is_terminal()),
        "final order still in flight",
    )?;
    require(
        r.obs.required.as_ref().is_some_and(|required| {
            required.accounts.contains(&ACCT_A)
                && [S1, P1]
                    .iter()
                    .all(|i| required.instruments.contains(&(ACCT_A, *i)) && r.obs.inst(*i).is_some())
        }),
        "missing final account/instrument observations",
    )?;
    // The observer stores only nonzero positions; zero is represented by absence.
    require(
        r.obs
            .positions
            .iter()
            .all(|p| p.account != ACCT_A || ![S1, P1].contains(&p.inst) || p.qty == 0),
        "final paired position is not flat",
    )?;
    check_end(
        InvariantSet::only(InvId::I16),
        &mut InvState::default(),
        &EndView {
            now_ns: r.end_ns,
            obs: &r.obs,
            truth: &r.truth,
        },
    )
    .map_err(|(id, msg)| format!("{}: {msg}", id.code()))
}

/// Full paired trading, an unpushed ADL, committed AUTO, and optional original confirmation.
pub fn led_081_auto_external_cold_hot_equal() -> Scenario {
    Scenario::new(
        "LED-081",
        "AUTO and late originals agree at every ledger boundary",
    )
    .suite("core")
    .seeds(SeedSet::Standard)
    .steps(crate::seed::DST_DEFAULT_STEPS)
    .invariants(InvariantSet::all())
    .configure(configure)
    .assert(assert_record)
}
