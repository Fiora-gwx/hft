//! Out-ring fan-out (TESTS.md SEQ-218, SEQ-219).

use hft_event::{ControlCmd, GwOp};
use hft_sequencer::types::OutRing;
use hft_strategy_sdk::{EventRef, IntentClass};
use hft_types::{GroupId, KillLevel, Side, StrategyId, Tif};

use crate::common::*;

/// B0 with `slots` OMS slots per table and batch cancel on both venues when `batch`.
fn boot(slots: u32, batch: bool) -> H {
    reset_script();
    let mut b = b0();
    b.caps.max_timers = 4_096;
    b.rd.limits = order_limits(A, 1_000, 6_000);
    b.rd.limits.extend(order_limits(B, 1_000, 6_000));
    if batch {
        for c in b.rd.caps.iter_mut() {
            c.flags = c.flags | hft_refdata::CapFlags::BATCH_CANCEL;
        }
    }
    b.setup_edit.push(match slots {
        64 => |s: &mut hft_sequencer::EngineSetup| s.oms.slots = hft_oms::PerTable::new(64),
        _ => |s: &mut hft_sequencer::EngineSetup| s.oms.slots = hft_oms::PerTable::new(256),
    });
    let mut h = b.boot();
    h.steady();
    h
}

/// `n` resting GTC buys of s1 on `inst` (acked), in batches of 16 per Bbo.
fn rest(h: &mut H, inst: hft_types::InstId, n: usize) -> Vec<hft_event::OrderDecision> {
    let mut all = Vec::new();
    let mut left = n;
    let mut k = 0u64;
    while left > 0 {
        let m = left.min(16);
        set_hook(move |hx, ev, ctx| {
            if let EventRef::Bbo { inst: i, .. } = ev
                && *i == inst
                && hx.s.strategy == S1
            {
                let b = hx.v.bbo(inst);
                for _ in 0..m {
                    let _ = ctx.place(intent(
                        hx,
                        inst,
                        Side::Buy,
                        Tif::Gtc,
                        IntentClass::Open,
                        hft_types::Px(b.bid_px.0 - 1_000),
                        q("0.001"),
                    ));
                }
            }
        });
        let mk = h.mark();
        k += 1;
        let ask = if k.is_multiple_of(2) { "50001" } else { "50002" };
        if inst == A1 {
            h.quote(A1, "50000", ask);
        } else {
            h.quote(B1, "50010", if k.is_multiple_of(2) { "50011" } else { "50012" });
        }
        h.run();
        clear_hook();
        let ds: Vec<_> = decisions(&h.since(mk)).into_iter().map(|d| d.1).collect();
        assert_eq!(ds.len(), m, "{ds:?}");
        for d in &ds {
            assert_eq!(d.reject, 0, "{d:?}");
            h.ack(d);
        }
        all.extend(ds);
        left -= m;
    }
    all
}

#[test]
fn seq_218_k_per_event() {
    // 40 per-slot cancels from one Input: 16 + 16 + 8 over three rounds.
    let mut h = boot(64, false);
    rest(&mut h, A1, 40);
    let n0 = h.io_ref().out.len();
    // One Kill (no batch cancel on the venue): 40 per-slot cancels from one OMS call.
    let c = ControlCmd::Kill {
        scope: scope(StrategyId(0), AC1, GroupId(0)),
        level: KillLevel::Account,
        flatten: false,
        cancel_resting: true,
        hard: false,
    };
    h.cmd_confirmed(AC1, &c);
    let mut per_round = Vec::new();
    let mut last = n0;
    for _ in 0..6 {
        h.step();
        let n = h.io_ref().out.len();
        per_round.push(n - last);
        last = n;
    }
    let busy: Vec<usize> = per_round.iter().copied().filter(|n| *n > 0).collect();
    assert_eq!(busy, vec![16, 16, 8], "{per_round:?}");
    assert!(!h.s.view().wants_continue());
    let cancels = h.io_ref().out[n0..]
        .iter()
        .filter(|(_, c)| c.op == GwOp::Cancel)
        .count();
    assert_eq!(cancels, 40);
}

#[test]
fn seq_219_kill_uses_cancel_all() {
    let mut h = boot(256, true);
    rest(&mut h, A1, 120);
    rest(&mut h, B1, 80);
    let n = h.io_ref().out.len();
    let c = ControlCmd::Kill {
        scope: scope(StrategyId(0), AC1, GroupId(0)),
        level: KillLevel::Account,
        flatten: false,
        cancel_resting: true,
        hard: false,
    };
    h.cmd_confirmed(AC1, &c);
    h.run();
    let out: Vec<_> = h.io_ref().out[n..].to_vec();
    let all: Vec<_> = out
        .iter()
        .filter(|(_, c)| matches!(c.op, GwOp::Cancel | GwOp::CancelAll))
        .collect();
    assert_eq!(all.len(), 2, "{all:?}");
    for v in [A, B] {
        assert!(
            all.iter()
                .any(|(r, c)| *r == OutRing::Venue(v) && c.op == GwOp::CancelAll)
        );
    }
}
