//! Hot-path entries allocate nothing after `new` (RSK-241).
#![allow(
    clippy::disallowed_types,
    clippy::disallowed_macros,
    clippy::disallowed_methods
)] // allow-list: test code, not a hot path

mod common;
use common::*;

use hft_risk::mode::ModeInput;
use hft_risk::{EquityView, LegSummary, Origin, ReconcileGroup, Risk, RiskTimer};
use hft_strategy_sdk::IntentClass;
use hft_testkit::alloc::CountingAlloc;
use hft_types::{CapSource, Notional, Rate, Side, Tif};

#[global_allocator]
static ALLOC: CountingAlloc = CountingAlloc;

#[test]
fn rsk_241_zero_alloc() {
    assert!(CountingAlloc::is_installed());
    let mut w = r0();
    // Everything the loop needs is built before counting starts.
    let open_it = open(1, G1, A1, Side::Buy, "0.0001", "50000.00", Tif::Gtc);
    let link_it = link_open(
        1,
        G1,
        A1,
        Side::Buy,
        "0.0001",
        "50000.00",
        (1, B, B1, Side::Sell, "0.001", "50"),
    );
    let hedge_it = hedge(1, G1, B1, Side::Sell, "0.001", "49995.0", "0.001");
    let d = draft(&open_it, IntentClass::Open);
    let pos = w.mv.pos_facts(1, A1);
    let caps = w.rd.caps(A);
    let eq = EquityView {
        total: n("200000"),
        valid: true,
    };
    let legs = LegSummary {
        naked_ratio: Rate(0),
        naked_notional: Notional(0),
        naked_filled_notional: Notional(0),
        quiescent: false,
    };
    let h = hdr(1, B, 0, KEYS);
    let levs: Vec<_> = (0..1_024)
        .map(|k| lev(if k % 2 == 0 { B1 } else { B2 }, "3.00"))
        .collect();
    let recon = vec![summary(1, B, 99_001, 1, 0, 0)];
    let kill_in = kill_acct(2, false);
    let resume_in = resume(CapSource::KILL, sc_acct(2));
    let sample = msample("0.50", "0.10", T0);
    let minus1 = n("-1");
    w.mm.sync(&w.risk);
    w.mv.tsc = w.now as u64;
    let c = cell(1, 1, G1);
    let mut seq = 10_000u64;
    let _ = Origin::Strategy;

    let sc = CountingAlloc::scope();
    for k in 0..10_000i64 {
        let t = T0 + k * 100 * MS;
        w.mv.tsc = t as u64;
        let now = now_at(t);
        std::hint::black_box(Risk::classify(&d, Origin::Strategy, &pos, caps));
        for it in [&open_it, &link_it, &hedge_it] {
            if let Ok(a) = w.risk.evaluate(it, &w.mv, &now) {
                w.risk.commit(&a, &now);
            }
        }
        for inp in [kill_in, resume_in] {
            let _ = w.risk.mode.apply(inp, seq, &w.mm, &mut w.out);
            seq += 1;
        }
        std::hint::black_box(w.risk.mode.effective(c));
        std::hint::black_box(w.risk.on_pnl(acc(1), minus1, eq));
        std::hint::black_box(w.risk.on_legs(acc(1), G1, &legs));
        w.risk.on_margin(acc(1), B, &sample, &now, &mut w.mo);
        w.risk
            .on_timer(RiskTimer::MarginStale { a: acc(1), v: B }, &now, &mut w.mo);
        w.risk.on_account_cfg(&h, &levs);
        std::hint::black_box(w.risk.hedge_px_limits(acc(1), B, B1));
        std::hint::black_box(w.risk.leverage(acc(1), B, B1));
        std::hint::black_box(w.risk.view(acc(1), &now));
        std::hint::black_box(w.risk.view_for(acc(1), A1, &now));
        std::hint::black_box(w.risk.on_reconcile(&ReconcileGroup { items: &recon }));
    }
    assert_eq!(sc.allocs(), 0);
    let _: Option<ModeInput<'static>> = None;
}
