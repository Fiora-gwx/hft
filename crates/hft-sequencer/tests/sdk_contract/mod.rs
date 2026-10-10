//! hft-strategy-sdk engine-side contract cases (crates/hft-strategy-sdk/TESTS.md "引擎侧契约夹具"):
//! `SeqRig` = this crate's B0 harness (`crate::common`), `RecProbe`/`RecModel` = its scripted
//! `TestSet` (per-call log, hooks issue the intents), `AdvRig` = the real advisor over the same
//! journal with an in-memory `CheckpointStore`. The SDK fixture names map onto B0 as listed in
//! that section (S3 → s1, A1 → acct-1, I1 → a1, I2 → b1, `GT3` → s1's three groups).

pub mod advice;
pub mod advisor;
pub mod checkpoint;
pub mod codes;
pub mod ctx;
pub mod delivery;
pub mod group_reset;
pub mod harness;
pub mod intent;
pub mod lifecycle;
pub mod mode;
pub mod replay;
pub mod subs;
pub mod target;
pub mod views;
pub mod warmup;

use hft_event::{EventKind, OrderDecision};
use hft_sequencer::advisor::{Advisor, AdvisorCfg};
use hft_sequencer::driver::replay::Log;
use hft_strategy_sdk::{EventRef, IntentClass, OrderIntent};
use hft_types::{GroupId, InstId, Px, Qty, Side, StrategyId, Tif};

pub use crate::advisor::{MemCkpt, Ring, committed};
use crate::common::*;

/// The three groups of s1 (`GT3`), gix 0, 1, 2. One (account, inst) belongs to at most one
/// non-Shadow cell of an instance (hft-sequencer `InstOwned`), so each group has its own legs:
/// GT1 = a1 + b1 (BTC), GT2 = a2 + b2 (ETH), GT3 = f14 + f23 (FILL).
pub const GT1: GroupId = G1;
pub const GT2: GroupId = GroupId(0x0100_0002);
pub const GT3: GroupId = GroupId(0x0100_0003);
/// FILL spot on A (a B0 filler inst) and the FILL perpetual on B added by `b_gt3`.
pub const F14: InstId = InstId(14);
pub const F23: InstId = InstId(23);

pub fn gspec_fill(id: GroupId) -> hft_event::GroupSpec {
    group(id, FILL, &[leg(F14, A), leg(F23, B)])
}

/// s1's parameters with the three `GT3` groups.
pub fn gt3(version: u64) -> Spb {
    Spb::new(
        S1,
        AC1,
        TAKER,
        version,
        vec![gspec(GT1), gspec_eth(GT2), gspec_fill(GT3)],
    )
}

/// B0 with s1 on `GT3` (inst 23 added), script reset.
pub fn b_gt3() -> B0 {
    reset_script();
    let mut b = b0();
    b.rd.insts.push(inst_spec(
        F23,
        B,
        "F23_USDT",
        true,
        FILL,
        ("0.01", 1),
        ("0.001", 100_000),
    ));
    b.caps.max_timers = 512;
    b.params = vec![gt3(1), Spb::s2(1), Spb::s9(1)];
    b
}

/// `SeqRig`: B0 with s1 on `GT3`, booted and steady, logs cleared.
pub fn rig() -> H {
    let mut h = b_gt3().boot();
    h.steady();
    clear_log();
    h
}

/// Deliveries to the fast part of `s`.
pub fn fast(s: StrategyId) -> Vec<Got> {
    log().into_iter().filter(|g| g.s == s && !g.model).collect()
}

/// Deliveries to the Model of `s`.
pub fn model(s: StrategyId) -> Vec<Got> {
    log().into_iter().filter(|g| g.s == s && g.model).collect()
}

/// Method calls of `s` recorded since index `from` of `calls()`.
pub fn calls_of(s: StrategyId, from: usize) -> Vec<&'static str> {
    calls()
        .into_iter()
        .skip(from)
        .filter(|c| c.0 == s)
        .map(|c| c.1)
        .collect()
}

/// `OrderDecision`s of a record slice.
pub fn decs(recs: &[hft_sequencer::MemRecord]) -> Vec<OrderDecision> {
    decisions(recs).into_iter().map(|d| d.1).collect()
}

/// `ExecDecision`s of a record slice.
pub fn execs(recs: &[hft_sequencer::MemRecord]) -> Vec<hft_event::ExecDecision> {
    recs.iter()
        .filter(|r| r.kind == EventKind::EXEC_DECISION)
        .filter_map(|r| r.read::<hft_event::ExecDecision>())
        .collect()
}

/// An order intent of the hook's instance in group `g`.
pub fn order_in(
    hx: &Hx<'_>,
    g: GroupId,
    inst: InstId,
    side: Side,
    tif: Tif,
    class: IntentClass,
    p: Px,
    qty: Qty,
) -> OrderIntent {
    let mut it = intent(hx, inst, side, tif, class, p, qty);
    it.group = g;
    it
}

/// The touch on the aggressive side of `side`.
pub fn touch(hx: &Hx<'_>, inst: InstId, side: Side) -> Px {
    let b = hx.v.bbo(inst);
    if side == Side::Buy { b.ask_px } else { b.bid_px }
}

/// Opens `qty` of `inst` for `s` in group `g` with an IOC at the touch (acked and filled);
/// returns the decision.
pub fn open_in(h: &mut H, s: StrategyId, g: GroupId, inst: InstId, side: Side, qty: &str) -> OrderDecision {
    let qx = q(qty);
    let trigger = h.upd + 1;
    add_hook(move |hx, ev, ctx| {
        if let EventRef::Bbo { inst: i, b, .. } = ev
            && *i == inst
            && hx.s.strategy == s
            && b.update_id == trigger
        {
            let p = touch(hx, inst, side);
            let _ = ctx.place(order_in(hx, g, inst, side, Tif::Ioc, IntentClass::Open, p, qx));
        }
    });
    let m = h.mark();
    let (bid, ask) = if inst == B1 {
        ("50010", "50011")
    } else if inst == A1 {
        ("50000", "50001")
    } else {
        ("3000", "3000.5")
    };
    h.quote(inst, bid, ask);
    h.run();
    let d = decs(&h.since(m))
        .into_iter()
        .find(|d| d.inst == inst && d.strategy == s)
        .expect("decision");
    assert_eq!(d.reject, 0, "{d:?}");
    h.ack(&d);
    h.fill(&d, d.qty, d.qty, d.px);
    d
}

// ------------------------------------------------------------------------------- AdvRig

pub type Adv = Advisor<TestSet, MemCkpt, Ring>;

/// Advisor configuration of `AdvRig` (one shard: s1).
pub fn adv_cfg() -> AdvisorCfg {
    AdvisorCfg {
        batch: 64,
        checkpoint_ms: 0,
        chunk_bytes: 1 << 20,
        max_lag_ms: 5_000,
        shard: Some(vec![S1]),
    }
}

/// Starts `AdvRig` over the committed journal of `h`.
pub fn adv(h: &H, ckpt: MemCkpt) -> Adv {
    adv_with(h, ckpt, adv_cfg())
}

pub fn adv_with(h: &H, ckpt: MemCkpt, cfg: AdvisorCfg) -> Adv {
    match Advisor::start(&committed(h.j()), &h.setup, cfg, ckpt, Ring::big(), hb()) {
        Ok(a) => a,
        Err(e) => panic!("advisor start: {e:?}"),
    }
}

/// Polls until the mirror caught up with the committed journal.
pub fn catch_up(a: &mut Adv, h: &H) {
    let log = committed(h.j());
    catch_up_log(a, &log);
}

pub fn catch_up_log(a: &mut Adv, log: &Log) {
    for _ in 0..100_000 {
        let busy = a.poll_once(log);
        if !busy && !a.checkpointing() {
            break;
        }
    }
}

/// Moves every Advice/Note the advisor pushed since `from` onto the sequencer's advice ring and
/// runs; returns the new `from`.
pub fn feed_advice(a: &Adv, h: &mut H, from: usize) -> usize {
    let recs: Vec<(EventKind, Vec<u8>)> = a.ring.recs.iter().skip(from).cloned().collect();
    for (k, b) in &recs {
        h.push_bytes(hft_sequencer::driver::live::RingId::Advice, *k, b.clone());
    }
    h.run();
    from + recs.len()
}

/// A checkpoint of the shard taken now; returns the store.
pub fn checkpoint_now(a: &mut Adv) -> MemCkpt {
    a.begin_checkpoints();
    while a.checkpointing() {
        a.poll_once(&Log::default());
    }
    core::mem::take(&mut a.ckpt)
}
