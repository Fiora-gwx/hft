//! Pair-cycle world shared by the OMS and executor DST cases (crates/hft-oms/TESTS.md
//! OMS-400–OMS-405, crates/hft-exec/TESTS.md EXE-500, EXE-505): W1 with account `a` alone, its
//! instance a `probe-target` that receives a `pair_lots` command (PairOnFill: `S1` buy first,
//! `P1` sell second, ratio 1) and a `pair_exit` command in turn, so OnFill links open and close
//! throughout the run while the case's faults are active. `S1` trades in 0.0001 lots and `P1` in
//! 0.001: the pair lot is 0.001.

use hft_event::{
    EventKind, ExecDecision, Note, NoteSource, OrderAction, OrderDecision, OrderEvent, OrderEventKind,
};
use hft_strategy_sdk::params::{RawParams, SchemaView, encode_body};
use hft_types::{AccountId, InstId};

use crate::dst::invariants::{EndView, InvState, check_end};
use crate::dst::world::{ACCT_A, P1, S1};
use crate::dst::{DstConfig, FaultRates, InvId, InvariantSet, RunRecord};
use crate::strategies::TargetParams;

pub(super) const SECOND: u64 = 1_000_000_000;
/// `S1` lot (0.0001 BTC) and `P1` lot (0.001 BTC) at qty exponent −8.
pub(super) const S1_LOT: i64 = 10_000;
pub(super) const P1_LOT: i64 = 100_000;
/// `OrderDecision.link_role` codes.
pub(super) const ROLE_FIRST: u8 = 1;
pub(super) const ROLE_SECOND: u8 = 2;
pub(super) const ROLE_SECOND_RETRY: u8 = 3;

/// The pair-cycle world: `cycles` rounds of `pair_lots(lots(k))` at `1 + 2k` s and `pair_exit`
/// at `2 + 2k` s; no faults (the case adds them).
pub(super) fn pair_cycles(c: &mut DstConfig, cycles: u64, lots: fn(u64, u64) -> i64) {
    c.horizon_ns = (2 * cycles + 8) * SECOND;
    c.faults = FaultRates::off();
    c.scripted.clear();
    c.world.accounts.truncate(1);
    c.world.groups.truncate(1);
    c.world.strategies.truncate(1);
    if let Some(i) = c.world.insts.iter_mut().find(|i| i.id == S1) {
        i.lot = S1_LOT;
        i.min_qty = S1_LOT;
    }
    let schema = &TargetParams::SCHEMA;
    let defaults = (0..schema.n_fields())
        .map(|ix| schema.field(ix).map_or(0, |f| f.default))
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
    c.confirmed_commands.clear();
    for k in 0..cycles {
        let open = (1 + 2 * k) * SECOND;
        let n = lots(c.seed, k);
        c.confirmed_commands
            .push((open, super::ledger::command(c, open, 2 * k + 1, Some(n))));
        let exit = (2 + 2 * k) * SECOND;
        c.confirmed_commands
            .push((exit, super::ledger::command(c, exit, 2 * k + 2, None)));
    }
}

/// Pair sizes of 10–40 first-leg lots (a multiple of the pair lot), seed- and cycle-derived.
pub(super) fn lots_10_40(seed: u64, k: u64) -> i64 {
    10 * (1 + (crate::seed::derive(seed, &format!("pair/{k}")) % 4) as i64)
}

/// Every Input-carrying record of the run: `(seq, record index)`.
pub(super) fn decisions(r: &RunRecord) -> Vec<(u64, u64, OrderDecision)> {
    r.journal
        .iter()
        .filter_map(|j| j.try_read::<OrderDecision>().map(|d| (j.seq, j.tsc, d)))
        .collect()
}

pub(super) fn events(r: &RunRecord) -> Vec<(u64, u64, OrderEvent)> {
    r.journal
        .iter()
        .filter_map(|j| j.try_read::<OrderEvent>().map(|e| (j.seq, j.tsc, e)))
        .collect()
}

pub(super) fn notes(r: &RunRecord, source: NoteSource, code: u32) -> Vec<(u64, Note)> {
    r.journal
        .iter()
        .filter(|j| j.kind == EventKind::NOTE)
        .filter_map(|j| j.try_read::<Note>().map(|n| (j.seq, n)))
        .filter(|(_, n)| n.source == source && n.code & 0xFFFF == code)
        .collect()
}

pub(super) fn exec_decisions(r: &RunRecord) -> Vec<(u64, ExecDecision)> {
    r.journal
        .iter()
        .filter_map(|j| j.read::<ExecDecision>().map(|d| (j.seq, d)))
        .collect()
}

/// Accepted new orders of `a` on `inst` with `role`.
pub(super) fn accepted(
    r: &RunRecord,
    a: AccountId,
    inst: InstId,
    role: u8,
) -> Vec<(u64, u64, OrderDecision)> {
    decisions(r)
        .into_iter()
        .filter(|(_, _, d)| {
            d.account == a
                && d.inst == inst
                && d.action == OrderAction::New
                && d.reject == 0
                && d.link_role == role
        })
        .collect()
}

/// Fill reports (with an increment) of the slots in `orders`.
pub(super) fn fills_of(r: &RunRecord, orders: &[(u64, u64, OrderDecision)]) -> Vec<(u64, u64, OrderEvent)> {
    events(r)
        .into_iter()
        .filter(|(_, _, e)| {
            matches!(e.what, OrderEventKind::PartialFill | OrderEventKind::Fill)
                && orders
                    .iter()
                    .any(|(_, _, d)| d.venue == e.venue && d.slot == e.slot && d.r#gen == e.r#gen)
        })
        .collect()
}

pub(super) fn require(ok: bool, message: &str) -> Result<(), String> {
    if ok { Ok(()) } else { Err(message.into()) }
}

/// The run reached at least `cycles` real pair openings: first legs and second legs that filled.
pub(super) fn require_pairs(r: &RunRecord, cycles: usize) -> Result<(), String> {
    let first = accepted(r, ACCT_A, S1, ROLE_FIRST);
    let second: Vec<_> = accepted(r, ACCT_A, P1, ROLE_SECOND)
        .into_iter()
        .chain(accepted(r, ACCT_A, P1, ROLE_SECOND_RETRY))
        .collect();
    require(
        fills_of(r, &first).len() >= cycles,
        &format!(
            "fewer than {cycles} first-leg fills ({})",
            fills_of(r, &first).len()
        ),
    )?;
    require(
        fills_of(r, &second).len() >= cycles,
        &format!(
            "fewer than {cycles} second-leg fills ({})",
            fills_of(r, &second).len()
        ),
    )
}

/// The end-of-run INV-16 comparison of the engine's books with the exchange truth (after the
/// final reconciliation round of the run).
pub(super) fn require_truth(r: &RunRecord) -> Result<(), String> {
    require(!r.truth.is_empty(), "no final exchange truth")?;
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
