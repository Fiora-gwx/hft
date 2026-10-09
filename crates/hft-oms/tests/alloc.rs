#![allow(
    clippy::disallowed_types,
    clippy::disallowed_macros,
    clippy::disallowed_methods
)] // allow-list: test code
#![allow(clippy::type_complexity, clippy::too_many_arguments)]
mod common;

use common::*;
use hft_event::{ConnRole, ConnStateKind, GwOp, OrderEventKind, SendStatus};
use hft_oms::*;
use hft_strategy_sdk::{IntentClass, LinkRole, OrderRef, SnapshotWriter};
use hft_testkit::CountingAlloc;
use hft_types::{Qty, RejectBits, Side, Tif, Tsc};

#[global_allocator]
static ALLOC: CountingAlloc = CountingAlloc;

#[test]
fn oms_006_zero_alloc_runtime() {
    assert!(CountingAlloc::is_installed());
    let mut h = H::with(|c| c.reuse_quarantine_ms = 0);
    let mut w = SnapshotWriter::new(1 << 22);
    let intents = [buy_x(), sell_y(), first_leg(q("0.012"), Tif::Ioc, plan_l1())];
    let plan = plan_l1();
    let book = bbo(p("59990.0"), p("60010.0"), ms(1));
    let mut refs = [OrderRef::new(VS, A1, 0, 0); 64];
    let mut nref = 0usize;
    let mut link = 0u32;
    let mut seed: u64 = 0x5EED;
    let scope = CountingAlloc::scope();
    for i in 0..10_000u64 {
        seed = seed
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        let pick = (seed >> 33) as usize;
        let t = i;
        let r = refs[pick % 64];
        let live = r.r#gen != 0;
        match pick % 14 {
            0 | 1 => {
                if let Ok(x) = h.place(&intents[pick % 3], t) {
                    if intents[pick % 3].intent.link.some != 0 {
                        link = h.slot(x).link_id;
                    }
                    refs[nref % 64] = x;
                    nref += 1;
                }
            }
            2 if live => {
                let _ = h.cancel(r, t);
            }
            3 if live => {
                let _ = h.amend(r, p("60100.0"), q("0.008"), t);
            }
            4 if live => {
                let w = [
                    OrderEventKind::Ack,
                    OrderEventKind::CancelAck,
                    OrderEventKind::OutcomeUnknown,
                ][pick % 3];
                h.what(r, w, t);
            }
            5 if live => h.fill(r, q("0.004"), p("60000.00"), t),
            6 if live => h.send(
                r.venue,
                r.account,
                GwOp::Place,
                r.slot,
                r.r#gen,
                SendStatus::Written,
                t,
                t,
            ),
            7 => {
                let st = if pick.is_multiple_of(2) {
                    ConnStateKind::Down
                } else {
                    ConnStateKind::Up
                };
                h.conn(VS, A1, ConnRole::Order, st, t);
            }
            8 if live => {
                let e = recon(r, 4, 7, 0, Qty(0));
                h.recon(&e, t);
            }
            9 => h.sweep(t),
            10 if live => h.uq(r, t),
            11 => {
                if link != 0 {
                    h.retry(link, t);
                    h.out.clear();
                    h.o.hedge_gated(
                        link,
                        HedgeGate::Risk(RejectBits::KILLED),
                        Tsc(0),
                        ms(t),
                        1,
                        &mut h.out,
                    );
                }
            }
            12 => {
                h.cont();
                let req = HedgeRequest {
                    link_id: link,
                    role: LinkRole::Second,
                    venue: VX,
                    account: A1,
                    side: Side::Sell,
                    reduce_only: false,
                    inst: Y,
                    group: g1(),
                    strategy: hft_types::StrategyId(1),
                    qty: q("0.012"),
                    basis: q("0.012"),
                    hedge_cap_qty: q("0.013"),
                    ref_px: p("60010.0"),
                    peer_venue: VS,
                    peer_account: A1,
                    trigger_seq: 1,
                };
                let _ = price_hedge(&req, &plan, &book, ms(1), &lim(), 10, MS);
                let _ = ApprovedIntent::hedge(&req, &plan, HedgePx::default(), IntentClass::Hedge);
            }
            _ => {
                w.clear();
                h.o.snapshot(&mut w);
            }
        }
    }
    assert_eq!(scope.allocs(), 0);
}
