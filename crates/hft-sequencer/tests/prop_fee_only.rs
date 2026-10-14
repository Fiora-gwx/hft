//! Scenario B-12 (docs/tests/scenarios.md): a fee-only report adds its fee and corrects the
//! net in ledger and LegTracker by the same amount, never moving a slot; on the real engine.

use hft_event::{EventKind, NoteLevel, OrderDecision, OrderEventKind};
use hft_oms::notes;
use hft_sequencer::driver::live::RingId;
use hft_strategy_sdk::EventRef;
use hft_types::{AssetId, Px, Qty, Side};
use proptest::prelude::*;

use crate::common::*;

/// When a deferred `FeeOnly` arrives.
#[derive(Copy, Clone, Debug)]
enum When {
    /// Right after the order's fills.
    Now,
    /// After the next order.
    Later,
    /// After the slot was recycled (quarantine over, sweep ran).
    Recycled,
    /// After the tombstone expired (`tombstone_keep_ms`).
    Expired,
}

#[derive(Clone, Debug)]
struct Ord {
    buy: bool,
    lots: i64,
    fills: u8,
    /// No fee on the fills; one `FeeOnly` instead.
    feeless: bool,
    fee_bp: i64,
    btc: bool,
    when: When,
    wrong_gen: bool,
}

fn ord() -> impl Strategy<Value = Ord> {
    (
        any::<bool>(),
        1i64..=40,
        1u8..=4,
        any::<bool>(),
        0i64..=10,
        any::<bool>(),
        prop_oneof![
            Just(When::Now),
            Just(When::Later),
            Just(When::Recycled),
            Just(When::Expired)
        ],
        proptest::bool::weighted(0.1),
    )
        .prop_map(|(buy, lots, fills, feeless, fee_bp, btc, when, wrong_gen)| {
            // Expired-tombstone and wrong-gen fees only in USDT (scenarios.md B-12 生成).
            let btc = btc && !wrong_gen && !matches!(when, When::Expired);
            Ord {
                buy,
                lots,
                fills,
                feeless,
                fee_bp,
                btc,
                when,
                wrong_gen,
            }
        })
}

const LOT: i64 = 10_000; // a1 0.0001

/// s1 places an a1 IOC at the touch (`Open` buy / `Reduce` sell, g1).
fn place(h: &mut H, buy: bool, qty: Qty) -> OrderDecision {
    set_hook(move |hx, ev, ctx| {
        if let EventRef::Bbo { inst, .. } = ev
            && *inst == A1
            && hx.s.strategy == S1
        {
            let b = hx.v.bbo(A1);
            let (side, p, class) = if buy {
                (Side::Buy, b.ask_px, hft_strategy_sdk::IntentClass::Open)
            } else {
                (Side::Sell, b.bid_px, hft_strategy_sdk::IntentClass::Reduce)
            };
            let mut it = intent(hx, A1, side, hft_types::Tif::Ioc, class, p, qty);
            it.group = G1;
            let _ = ctx.place(it);
        }
    });
    let m = h.mark();
    h.quote(A1, "50000", "50001");
    h.run();
    clear_hook();
    let d = decisions(&h.since(m))
        .into_iter()
        .find(|d| d.1.inst == A1)
        .expect("decision")
        .1;
    assert_eq!(d.reject, 0, "{d:?}");
    d
}

fn fill_part(h: &mut H, d: &OrderDecision, part: Qty, cum: Qty, fee: Qty, asset: AssetId) {
    let what = if cum.0 >= d.qty.0 {
        OrderEventKind::Fill
    } else {
        OrderEventKind::PartialFill
    };
    let mut ev = order_event(d, what, h.epoch());
    ev.qty = part;
    ev.cum_filled = cum;
    ev.px = d.px;
    ev.avg_px = d.px;
    ev.fee = fee;
    ev.fee_asset = asset;
    h.push(RingId::Rep(0), &ev);
    h.run();
}

/// A fee of `bp` (0.01 %) on `qty` at `px` in BTC or USDT (both qty exponent −8).
fn fee_of(qty: Qty, px: Px, bp: i64, btc: bool) -> Qty {
    if btc {
        Qty(qty.0 * bp / 10_000)
    } else {
        // qty (1e-8 BTC) × px (1e-2 USDT) = 1e-10 USDT → 1e-8 USDT units.
        Qty((i128::from(qty.0) * i128::from(px.0) / 100 * i128::from(bp) / 10_000) as i64)
    }
}

/// One pending `FeeOnly`.
#[derive(Copy, Clone, Debug)]
struct Pending {
    d: OrderDecision,
    fee: Qty,
    btc: bool,
    wrong_gen: bool,
    expired: bool,
}

/// Delivers one `FeeOnly` and checks it; returns the base-asset correction it made.
fn fee_only(h: &mut H, p: &Pending) -> Result<i64, TestCaseError> {
    let mut ev = order_event(&p.d, OrderEventKind::FeeOnly, h.epoch());
    if p.wrong_gen {
        ev.r#gen = p.d.r#gen.wrapping_add(7);
    }
    ev.cum_filled = p.d.qty;
    ev.fee = p.fee;
    ev.fee_asset = if p.btc { BTC } else { USDT };
    let pos0 = h.s.engine().ledger.pos(AC1, A1).qty.0;
    let net0 = h.s.engine().oms.legs(AC1, G1).view.leg[0].net.0;
    h.trace_on();
    let m = h.mark();
    h.push(RingId::Rep(0), &ev);
    h.run();
    // The FeeOnly Input and its Outputs (up to the next Input).
    let all = h.since(m);
    let at = all
        .iter()
        .position(|r| r.kind == EventKind::ORDER_EVENT)
        .expect("FeeOnly Input");
    let end = all[at + 1..]
        .iter()
        .position(|r| r.role() == hft_event::Role::Input)
        .map_or(all.len(), |k| at + 1 + k);
    let recs = &all[at..end];
    // No slot moves.
    let transition = notes::code_of(notes::TRANSITION);
    let moved: Vec<(u64, u64, u64, u64)> = recs
        .iter()
        .filter_map(|r| r.read::<hft_event::Note>())
        // Existing slots only: an executor order the same Input places (a HedgeFix of the group's
        // excess leg after the correction) is a new slot, not a FeeOnly transition.
        .filter(|n| n.code == transition && (n.v[0] >> 16) & 0xFF != 0)
        .map(|n| {
            (
                n.v[1],
                (n.v[0] >> 16) & 0xFF,
                (n.v[0] >> 24) & 0xFF,
                (n.v[0] >> 32) & 0xFF,
            )
        })
        .collect();
    prop_assert!(
        moved.is_empty(),
        "FeeOnly moved a slot (slot, from, to, cause) {moved:?}: {p:?}"
    );
    prop_assert_eq!(
        recs.iter().filter(|r| r.kind == EventKind::ORDER_EVENT).count(),
        1
    );
    // Exactly one FeeOut (one `apply_fee`).
    prop_assert_eq!(h.s.engine().trace.count("ledger.apply_fee"), 1, "{:?}", p);
    let pos1 = h.s.engine().ledger.pos(AC1, A1).qty.0;
    let net1 = h.s.engine().oms.legs(AC1, G1).view.leg[0].net.0;
    let unattributed = notes::code_of(notes::FEE_UNATTRIBUTED);
    let un: Vec<_> = recs
        .iter()
        .filter_map(|r| r.read::<hft_event::Note>())
        .filter(|n| n.code == unattributed)
        .collect();
    if p.wrong_gen || p.expired {
        prop_assert_eq!(un.len(), 1, "unattributed {:?}", p);
        prop_assert_eq!(un[0].level, NoteLevel::P1);
        prop_assert_eq!((pos1, net1), (pos0, net0), "no correction {:?}", p);
        return Ok(0);
    }
    prop_assert!(un.is_empty(), "{:?}", p);
    let delta = if p.btc { -p.fee.0 } else { 0 };
    // Ledger and LegTracker corrected by the same amount in this Input (buy and sell: −fee).
    prop_assert_eq!(pos1 - pos0, delta, "ledger {:?}", p);
    prop_assert_eq!(net1 - net0, delta, "LegTracker {:?}", p);
    Ok(delta)
}

fn run_case(orders: &[Ord]) -> Result<(), TestCaseError> {
    reset_script();
    let mut b = b0();
    b.caps.max_timers = 8_192;
    b.setup_edit.push(|s| s.oms.slots = hft_oms::PerTable::new(128));
    let mut h = b.boot();
    h.steady();
    // Inventory to sell from: a hedged a1 +0.15 / b1 −0.15 in g1 (a linked open), so the
    // group's exposure gate (#9) does not block the generated opens.
    set_hook(|hx, ev, ctx| {
        if let EventRef::Bbo { inst, .. } = ev
            && *inst == A1
            && hx.s.strategy == S1
        {
            let _ = ctx.place(pair_first(hx, Qty(1_500 * LOT)));
        }
    });
    let m = h.mark();
    h.quote(A1, "50000", "50001");
    h.run();
    clear_hook();
    let seed = decisions(&h.since(m))[0].1;
    h.ack(&seed);
    let m = h.mark();
    fill_part(&mut h, &seed, seed.qty, seed.qty, Qty(0), USDT);
    let second = decisions(&h.since(m))
        .into_iter()
        .find(|d| d.1.inst == B1)
        .expect("second leg")
        .1;
    h.ack(&second);
    h.fill(&second, second.qty, second.qty, second.px);
    let mut reference = seed.qty.0;
    let mut later: Vec<Pending> = Vec::new();
    let mut recycled: Vec<Pending> = Vec::new();
    let mut expired: Vec<Pending> = Vec::new();
    for o in orders {
        let qty = Qty(o.lots * LOT);
        let d = place(&mut h, o.buy, qty);
        h.ack(&d);
        // Split into `fills` increments.
        let n = i64::from(o.fills).min(o.lots);
        let mut cum = 0i64;
        let mut fee_sum = 0i64;
        for k in 0..n {
            let part = if k == n - 1 { qty.0 - cum } else { qty.0 / n };
            cum += part;
            let (fee, asset) = if o.feeless {
                (Qty(0), AssetId(0))
            } else if o.buy {
                // Received asset: BTC on a buy, USDT on a sell.
                (fee_of(Qty(part), d.px, o.fee_bp, true), BTC)
            } else {
                (fee_of(Qty(part), d.px, o.fee_bp, false), USDT)
            };
            if asset == BTC {
                fee_sum += fee.0;
            }
            fill_part(&mut h, &d, Qty(part), Qty(cum), fee, asset);
        }
        reference += if o.buy { qty.0 - fee_sum } else { -qty.0 - fee_sum };
        for p in later.drain(..) {
            reference += fee_only(&mut h, &p)?;
        }
        if o.feeless {
            let p = Pending {
                d,
                fee: fee_of(qty, d.px, o.fee_bp, o.btc),
                btc: o.btc,
                wrong_gen: o.wrong_gen,
                expired: matches!(o.when, When::Expired),
            };
            match o.when {
                When::Now => reference += fee_only(&mut h, &p)?,
                When::Later => later.push(p),
                When::Recycled => recycled.push(p),
                When::Expired => expired.push(p),
            }
        }
    }
    for p in later.drain(..) {
        reference += fee_only(&mut h, &p)?;
    }
    // Quarantine over: the sweep recycles the slots.
    h.adv(6 * SEC);
    h.run();
    for p in recycled.drain(..) {
        reference += fee_only(&mut h, &p)?;
    }
    // Tombstones expire.
    h.adv(3_601 * SEC);
    h.run();
    for p in expired.drain(..) {
        reference += fee_only(&mut h, &p)?;
    }
    let pos = h.s.engine().ledger.pos(AC1, A1).qty.0;
    let net = h.s.engine().oms.legs(AC1, G1).view.leg[0].net.0;
    prop_assert_eq!(pos, net, "ledger = LegTracker");
    prop_assert_eq!(pos, reference, "naive net");
    Ok(())
}

proptest! {
    #![proptest_config(ProptestConfig { cases: 1_000, ..ProptestConfig::default() })]
    #[test]
    fn b_12_fee_only_adjusts_net_without_transition(orders in proptest::collection::vec(ord(), 1..=32)) {
        run_case(&orders)?;
    }
}
