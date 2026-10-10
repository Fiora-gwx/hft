//! Views and view types (SDK-043, SDK-056, SDK-062, SDK-124, SDK-130–SDK-139, SDK-209; SDK-140 is
//! the separate `sdk_alloc` target).

use hft_event::{
    BalanceReport, ConnRole, ConnStateKind, ControlCmd, EventKind, L2SnapHdr, Level, MarketHdr, RecordFlags,
};
use hft_risk::{InstClassRow, PreVec};
use hft_sequencer::SnapshotSel;
use hft_sequencer::driver::live::RingId;
use hft_strategy_sdk::{
    BboView, BookStatus, EvMask, EventRef, ExecUpdate, ExecUpdateKind, IntentClass, MarginLevel, OrderRef,
    OrderStateView, Part, RiskEvent, RxTsSrc,
};
use hft_types::{AccountId, AssetId, CapSource, GroupId, InstId, KillLevel, Qty, Side, StrategyId, Tif, Tsc};
use zerocopy::{FromZeros, IntoBytes};

use super::*;
use crate::common::*;

/// Reads `f` inside s1's `on_event` of the next a1 Bbo (bid 50 000, ask 50 001).
pub fn in_event<T: 'static>(h: &mut H, f: impl Fn(&Hx<'_>) -> T + 'static) -> T {
    in_event_at(h, "50000", "50001", f)
}

/// Reads `f` inside s1's `on_event` of the next a1 Bbo (`bid`, `ask`).
pub fn in_event_at<T: 'static>(h: &mut H, bid: &str, ask: &str, f: impl Fn(&Hx<'_>) -> T + 'static) -> T {
    let c: Cap<T> = cap();
    let c2 = c.clone();
    set_hook(move |hx, ev, _| {
        if let EventRef::Bbo { inst, .. } = ev
            && *inst == A1
            && hx.s.strategy == S1
        {
            c2.borrow_mut().push(f(hx));
        }
    });
    h.quote(A1, bid, ask);
    h.run();
    clear_hook();
    let v = c.borrow_mut().pop();
    v.expect("read in on_event")
}

/// The `OrderRef` of a decision.
pub fn oref(d: &hft_event::OrderDecision) -> OrderRef {
    OrderRef {
        venue: d.venue,
        account: d.account,
        _pad: [0; 2],
        slot: d.slot.0,
        r#gen: d.r#gen,
    }
}

/// `SeqRig` whose first `BalanceReport`s (the ones that initialize the ledger's cash; later
/// reports are only drift checks) are `bals` (before `steady`'s own).
pub fn rig_balances(bals: &[(AccountId, hft_types::VenueId, AssetId, &str)]) -> H {
    let mut h = b_gt3().boot();
    for (a, v, asset, free) in bals {
        let br = BalanceReport {
            venue: *v,
            account: *a,
            asset: *asset,
            _pad: 0,
            free: q(free),
            locked: Qty(0),
            ex_ts_ns: 0,
            rx_tsc: Tsc(h.now()),
        };
        h.push(RingId::Rep(0), &br);
    }
    h.steady();
    clear_log();
    h
}

/// `SeqRig` with B0's `RiskParams` edited by `f`.
pub fn rig_risk(f: impl FnOnce(&mut hft_risk::RiskParams)) -> H {
    let mut b = b_gt3();
    let rd = b.rd.load();
    let mut rk = b0_risk(&rd);
    f(&mut rk);
    b.risk = Some(rk);
    let mut h = b.boot();
    h.steady();
    clear_log();
    h
}

/// acct-1's exposure gate off (`naked_gate_min` = 1 000 000 USDT): one-legged positions built
/// leg by leg do not trip `EXPOSURE_GATE`.
pub fn no_naked_gate(rk: &mut hft_risk::RiskParams) {
    let mut accs: Vec<_> = rk.accounts.iter().copied().collect();
    for a in accs.iter_mut() {
        if a.account == AC1 {
            a.default_group.naked_gate_min = n("1000000");
        }
    }
    rk.accounts = PreVec::from_vec(accs);
}

// ------------------------------------------------------------------------------- books

/// The L2 inst of the book cases: B0 filler inst 15 (FILL spot on A, in no group). The start
/// gate (`Gate(Feeds)`) waits for the L2 books of group legs to be Live, so a group leg's book
/// cannot be `Uninit` in the steady rig.
const L2I: InstId = InstId(15);

/// `SeqRig` with an L2 book for inst 15 (`l2_depth = 5`, DepthTopN) and s1 subscribed to a1 BBO
/// and inst 15 BOOK (gix 0).
fn rig_book() -> H {
    let mut b = b_gt3();
    if let Some(s) = b.rd.insts.iter_mut().find(|s| s.id == L2I) {
        s.l2_depth = 5;
    }
    set_subs(
        S1,
        vec![
            (A1, 0, EvMask::BBO, Part::Fast),
            (B1, 0, EvMask::BBO, Part::Fast),
            (L2I, 0, EvMask::BOOK, Part::Fast),
        ],
    );
    let mut h = b.boot();
    h.steady();
    clear_log();
    h
}

/// The five levels per side of the snapshots below (tick 0.01; bids descending, asks ascending).
fn snap_levels() -> (Vec<Level>, Vec<Level>) {
    let bids = (0..5)
        .map(|k| Level {
            px: hft_types::Px(px("10").0 - k),
            qty: q(&format!("{}", k + 1)),
        })
        .collect();
    let asks = (0..5)
        .map(|k| Level {
            px: hft_types::Px(px("10.01").0 + k),
            qty: q(&format!("{}", k + 2)),
        })
        .collect();
    (bids, asks)
}

/// One `L2Snapshot{origin = TopN}` of inst 15 on the md ring, then run.
fn l2_topn(h: &mut H, bids: &[Level], asks: &[Level]) {
    let mut hd = L2SnapHdr::new_zeroed();
    hd.h = MarketHdr::new_zeroed();
    hd.h.venue = A;
    hd.h.inst = L2I;
    hd.h.rx_tsc = h.now();
    hd.snap_id = 1;
    hd.origin = L2SnapHdr::TOP_N;
    hd.n_bids = bids.len() as u16;
    hd.n_asks = asks.len() as u16;
    let mut body = hd.as_bytes().to_vec();
    for l in bids.iter().chain(asks.iter()) {
        body.extend_from_slice(l.as_bytes());
    }
    h.push_bytes(RingId::Md(0), EventKind::L2_SNAPSHOT, body);
    h.run();
}

#[test]
fn sdk_043_book_once_per_message() {
    let mut h = rig_book();
    let got: Cap<(u16, InstId, BookStatus, usize, [Level; 8])> = cap();
    let g2 = got.clone();
    set_hook(move |hx, ev, _| {
        if let EventRef::Book { inst, gix } = ev
            && hx.s.strategy == S1
        {
            let mut buf = [Level {
                px: hft_types::Px(0),
                qty: Qty(0),
            }; 8];
            let n = hx.v.book_levels(*inst, Side::Buy, &mut buf);
            g2.borrow_mut()
                .push((*gix, *inst, hx.v.book_status(*inst), n, buf));
        }
    });
    let (bids, asks) = snap_levels();
    l2_topn(&mut h, &bids, &asks);
    clear_hook();
    let g = got.borrow();
    assert_eq!(g.len(), 1, "one Book delivery per message");
    let (gix, inst, st, n, buf) = g[0];
    assert_eq!((gix, inst), (0, L2I));
    assert_eq!(st, BookStatus::Live);
    assert_eq!(n, 5);
    assert_eq!(&buf[..5], &bids[..]);
    assert_eq!(
        fast(S1).iter().filter(|x| x.kind == "Book").count(),
        1,
        "{:?}",
        fast(S1)
    );
}

#[test]
fn sdk_135_book_read() {
    let mut h = rig_book();
    // Step 1: before the snapshot.
    let st0 = in_event(&mut h, |hx| hx.v.book_status(L2I));
    assert_eq!(st0, BookStatus::Uninit);
    // Step 2: after the 5-level snapshot.
    let (bids, asks) = snap_levels();
    l2_topn(&mut h, &bids, &asks);
    let (best, n3, b3, n8, b8, st_a1) = in_event(&mut h, |hx| {
        let z = Level {
            px: hft_types::Px(0),
            qty: Qty(0),
        };
        let mut b3 = [z; 3];
        let mut b8 = [z; 8];
        let n3 = hx.v.book_levels(L2I, Side::Buy, &mut b3);
        let n8 = hx.v.book_levels(L2I, Side::Sell, &mut b8);
        (
            hx.v.book_best(L2I, Side::Buy),
            n3,
            b3,
            n8,
            b8,
            hx.v.book_status(A1),
        )
    });
    assert_eq!(best, Some(bids[0]));
    assert_eq!(n3, 3);
    assert_eq!(&b3[..], &bids[..3]);
    assert_eq!(n8, 5);
    assert_eq!(&b8[..5], &asks[..]);
    // Step 3: a1 has no L2 subscription.
    assert_eq!(st_a1, BookStatus::Uninit);
}

// ------------------------------------------------------------------------------- state point

#[test]
fn sdk_056_views_state_point() {
    let mut h = rig();
    let c: Cap<(BboView, BboView, Qty)> = cap();
    let c2 = c.clone();
    set_hook(move |hx, ev, ctx| {
        if let EventRef::Bbo { inst, b, .. } = ev
            && *inst == A1
            && hx.s.strategy == S1
            && b.bid_px == px("65000")
        {
            let seen = hx.v.bbo(A1);
            let it = order_in(
                hx,
                GT1,
                A1,
                Side::Buy,
                Tif::Ioc,
                IntentClass::Open,
                b.ask_px,
                q("0.01"),
            );
            let _ = ctx.place(it);
            let after = hx.v.legs(AC1, GT1).leg[0].inflight_immediate;
            c2.borrow_mut().push((seen, *b, after));
        }
    });
    h.quote(A1, "65000", "65001");
    h.run();
    clear_hook();
    let (seen, ev_b, after) = c.borrow()[0];
    assert_eq!(seen, ev_b);
    assert_eq!((seen.bid_px.0, seen.ask_px.0), (6_500_000, 6_500_100));
    assert_eq!(after, Qty(0), "intents are processed after on_event");
    // Step 2: the next event sees the in-flight IOC.
    let next = in_event(&mut h, |hx| hx.v.legs(AC1, GT1).leg[0].inflight_immediate);
    assert_eq!(next, Qty(1_000_000));
}

// ------------------------------------------------------------------------------- portfolio

#[test]
fn sdk_062_cash_per_venue() {
    let mut h = rig_balances(&[(AC1, A, USDT, "10000"), (AC1, B, USDT, "5000")]);
    let qx = q("0.01");
    set_hook(move |hx, ev, ctx| {
        if let EventRef::Bbo { inst, b, .. } = ev
            && *inst == A1
            && hx.s.strategy == S1
            && b.ask_px == px("65000")
        {
            let it = order_in(hx, GT1, A1, Side::Buy, Tif::Ioc, IntentClass::Open, b.ask_px, qx);
            let _ = ctx.place(it);
        }
    });
    let m = h.mark();
    h.quote(A1, "64999", "65000");
    h.run();
    clear_hook();
    let d = decs(&h.since(m))
        .into_iter()
        .find(|d| d.strategy == S1 && d.inst == A1)
        .expect("decision");
    assert_eq!(d.reject, 0, "{d:?}");
    h.ack(&d);
    h.fill(&d, qx, qx, px("65000"));
    let (ca, cb, vs) = in_event(&mut h, |hx| {
        (
            hx.v.pf_cash(AC1, A, USDT),
            hx.v.pf_cash(AC1, B, USDT),
            hx.v.pf_venues(AC1),
        )
    });
    assert_eq!(ca.free, Qty(935_000_000_000), "{ca:?}");
    assert_eq!(cb.free, Qty(500_000_000_000), "{cb:?}");
    assert!(vs.contains(A) && vs.contains(B));
    assert_eq!(vs.len(), 2);
}

#[test]
fn sdk_137_equity_total_sum() {
    let mut h = rig_balances(&[(AC1, A, USDT, "10000"), (AC1, A, BTC, "0.1")]);
    open_in(&mut h, S1, GT1, B1, Side::Buy, "0.1");
    h.mark_md(B1, "65100");
    h.margin_report(AC1, "5000", "0", "5000");
    h.run();
    h.quote(A1, "65000", "65000.01");
    h.run();
    let (tot, va, vb, acc) = in_event_at(&mut h, "65000", "65000.01", |hx| {
        (
            hx.v.pf_equity_total(AC1),
            hx.v.pf_venue(AC1, A).equity,
            hx.v.pf_venue(AC1, B).equity,
            hx.v.pf_account(AC1),
        )
    });
    assert_eq!(tot.0, va.0 + vb.0);
    assert_eq!(va.0, 16_500_000_000, "{va:?}");
    assert_eq!(acc, AC1);
}

// ------------------------------------------------------------------------------- quote health

#[test]
fn sdk_124_quiet_book_stays_live() {
    let mut h = rig();
    let reads: Cap<(u64, bool, Tsc)> = cap();
    let r2 = reads.clone();
    set_hook(move |hx, ev, ctx| {
        if hx.s.strategy != S1 {
            return;
        }
        let now = hx.v.now().tsc.0;
        match ev {
            EventRef::Bbo { inst, .. } if *inst == B1 => {
                let _ = ctx.timer_at(Tsc(now + 100 * MS), 7, GT1);
            }
            EventRef::Timer { tag: 7, .. } => {
                r2.borrow_mut()
                    .push((now, hx.v.quote_ok(AC1, B1), hx.v.bbo(B1).rx_tsc));
                let _ = ctx.timer_at(Tsc(now + 100 * MS), 7, GT1);
            }
            _ => {}
        }
    });
    let t0 = h.now();
    h.quote(B1, "50010", "50011");
    h.run();
    let alive = |h: &mut H| {
        let t = h.now();
        h.push(
            RingId::Rep(0),
            &conn(
                B,
                0,
                ConnStateKind::Alive,
                ConnRole::Md,
                InstId(0),
                AccountId(0),
                t,
            ),
        );
    };
    // Step 1: 10 s, an Alive every 500 ms.
    for k in 1..=100u64 {
        h.at(t0 + k * 100 * MS);
        if k % 5 == 0 {
            alive(&mut h);
        }
        h.run();
    }
    let step1: Vec<_> = reads.borrow().clone();
    assert_eq!(step1.len(), 100);
    assert!(step1.iter().all(|r| r.1), "{step1:?}");
    assert!(step1.iter().all(|r| r.2 == Tsc(t0)));
    // Step 2: no more Alive for 2.6 s.
    let last_alive = t0 + 100 * 100 * MS;
    for k in 101..=126u64 {
        h.at(t0 + k * 100 * MS);
        h.run();
    }
    clear_hook();
    let step2: Vec<_> = reads.borrow()[100..].to_vec();
    assert_eq!(step2.len(), 26);
    for (t, ok, _) in &step2 {
        assert_eq!(
            *ok,
            *t - last_alive <= 2_000 * MS,
            "read at +{} ms",
            (*t - last_alive) / MS
        );
    }
    assert!(step2.iter().any(|r| r.0 - last_alive == 2_000 * MS && r.1));
    assert!(step2.iter().any(|r| r.0 - last_alive == 2_100 * MS && !r.1));
}

#[test]
fn sdk_130_rx_ts_src_projection() {
    let mut h = rig();
    let hw = 1_700_000_000_123_456_789i64;
    let rows = [
        ("SDK-130.1", RecordFlags::HW_TS, hw, RxTsSrc::Hw),
        ("SDK-130.2", RecordFlags::SW_TS, hw, RxTsSrc::Sw),
        ("SDK-130.3", RecordFlags(0), 0, RxTsSrc::None),
    ];
    for (row, flags, ns, want) in rows {
        clear_log();
        h.upd += 1;
        let mut b = bbo(A1, A, 0, "50000", "50002", h.upd, h.now());
        b.h.hw_rx_ns = ns;
        h.push_e(
            RingId::Bbo(0),
            Elem {
                kind: EventKind::BBO,
                flags,
                body: b.as_bytes().to_vec(),
            },
        );
        h.run();
        let g = fast(S1)
            .into_iter()
            .find(|g| g.kind == "Bbo" && g.inst == A1)
            .and_then(|g| g.bbo)
            .expect(row);
        assert_eq!((g.rx_ts_src, g.hw_rx_ns), (want, ns), "{row}");
    }
}

// ------------------------------------------------------------------------------- leverage

fn lev_report(h: &mut H, inst: InstId, lev: i64) {
    let l = hft_event::LeverageItem {
        inst,
        _pad: 0,
        leverage: hft_types::Rate(lev),
    };
    let hd = cfg_hdr(AC1, B, 1);
    let mut body = hd.as_bytes().to_vec();
    body.extend_from_slice(l.as_bytes());
    h.push_bytes(RingId::Rep(0), EventKind::ACCOUNT_CONFIG_REPORT, body);
    h.run();
}

#[test]
fn sdk_131_leverage_x100() {
    let rows: [(&str, i64, Option<u32>); 5] = [
        ("SDK-131.1", 20_000_000_000, Some(2_000)),
        ("SDK-131.2", 2_500_000_000, Some(250)),
        ("SDK-131.3", 2_509_000_000, Some(250)),
        ("SDK-131.4", 9_999_999, Some(0)),
        ("SDK-131.5", 0, None),
    ];
    for (row, lev, want) in rows {
        let mut h = rig();
        lev_report(&mut h, B1, lev);
        let got = in_event(&mut h, |hx| hx.v.leverage(AC1, B1));
        assert_eq!(got, want, "{row}");
    }
    // .6 no report yet (booted, before the steady state's AccountConfigReport).
    let mut h = b_gt3().boot();
    h.step();
    assert_eq!(
        hft_strategy_sdk::Views::leverage(&h.s.engine().seq_views(), AC1, B1),
        None,
        "SDK-131.6"
    );
    // .7 20x on the perp, query the spot inst.
    let mut h = rig();
    lev_report(&mut h, B1, 20_000_000_000);
    let got = in_event(&mut h, |hx| hx.v.leverage(AC1, A1));
    assert_eq!(got, None, "SDK-131.7");
}

// ------------------------------------------------------------------------------- risk

#[test]
fn sdk_132_risk_for_per_class() {
    let mut b = b_gt3();
    // hft-risk `ExecRiskMismatch`: the engine's force-slice cross cap must not exceed the
    // smallest `rr_max_cross` of the account's insts (class A: 0.002).
    b.setup_edit
        .push(|s| s.exec.default.force_slice.max_cross = hft_types::Rate(2_000_000));
    let rd = b.rd.load();
    let mut rk = b0_risk(&rd);
    rk.classes = hft_risk::FixedVec::new();
    let _ = rk.classes.push(band("0.005", "0.001", "0.002"));
    let _ = rk.classes.push(band("0.02", "0.005", "0.01"));
    rk.risk_class = PreVec::from_vec(vec![InstClassRow { inst: B1, class: 1 }]);
    rk.l3_max_cross = hft_types::Rate(0);
    b.risk = Some(rk);
    let mut h = b.boot();
    h.steady();
    let (ra, rb) = in_event(&mut h, |hx| (hx.v.risk_for(AC1, A1), hx.v.risk_for(AC1, B1)));
    assert_eq!(
        (ra.rr_max_cross.0, ra.ioc_max_cross.0, ra.max_px_dev.0),
        (2_000_000, 1_000_000, 5_000_000)
    );
    assert_eq!(
        (rb.rr_max_cross.0, rb.ioc_max_cross.0, rb.max_px_dev.0),
        (10_000_000, 5_000_000, 20_000_000)
    );
    assert_eq!((ra.l3_max_cross.0, rb.l3_max_cross.0), (0, 0));
}

/// `SeqRig` with acct-1's `margin_stale_ms` = `ms`.
fn rig_margin_stale(ms: u32) -> H {
    rig_risk(|rk| {
        let mut accs: Vec<_> = rk.accounts.iter().copied().collect();
        for a in accs.iter_mut() {
            if a.account == AC1 {
                a.margin.margin_stale_ms = ms;
            }
        }
        rk.accounts = PreVec::from_vec(accs);
    })
}

#[test]
fn sdk_138_margin_stale() {
    let mut h = rig_margin_stale(10_000);
    h.margin_report(AC1, "100000", "0", "100000");
    h.run();
    let e_tsc = h.now();
    let (e, m0) = in_event(&mut h, |hx| (hx.v.now().engine_ns, hx.v.pf_margin(AC1, B)));
    assert_eq!(m0.src_ens, e);
    let rows = [
        ("SDK-138.1", 9_999u64, false),
        ("SDK-138.2", 10_000, false),
        ("SDK-138.3", 10_001, true),
    ];
    for (row, d, stale) in rows {
        h.at(e_tsc + d * MS);
        let (now, m) = in_event(&mut h, |hx| (hx.v.now().engine_ns, hx.v.pf_margin(AC1, B)));
        assert_eq!(now - e, (d * MS) as i64, "{row}");
        assert_eq!(m.src_ens, e, "{row}");
        assert_eq!(m.stale, stale, "{row}");
    }
}

#[test]
fn sdk_139_venue_ready() {
    let mut h = rig();
    // Step 1.
    assert!(in_event(&mut h, |hx| hx.v.venue_ready(AC1, A)));
    // Step 2: an IOC goes out, the order connections drop without a receipt.
    set_hook(|hx, ev, ctx| {
        if let EventRef::Bbo { inst, b, .. } = ev
            && *inst == A1
            && hx.s.strategy == S1
            && b.ask_px == px("50009")
        {
            let _ = ctx.place(order_in(
                hx,
                GT1,
                A1,
                Side::Buy,
                Tif::Ioc,
                IntentClass::Open,
                b.ask_px,
                q("0.001"),
            ));
        }
    });
    let m = h.mark();
    h.quote(A1, "50000", "50009");
    h.run();
    clear_hook();
    let d = decs(&h.since(m))
        .into_iter()
        .find(|d| d.strategy == S1 && d.inst == A1)
        .expect("decision");
    assert_eq!(d.reject, 0, "{d:?}");
    let t = h.now();
    for k in [0u8, 1] {
        h.push(
            RingId::Rep(0),
            &conn(A, k, ConnStateKind::Down, ConnRole::Order, InstId(0), AC1, t),
        );
    }
    h.run();
    let r = oref(&d);
    let (ready, st) = in_event(&mut h, move |hx| {
        (hx.v.venue_ready(AC1, A), hx.v.order(r).map(|o| o.state))
    });
    assert_eq!(st, Some(OrderStateView::Unknown));
    assert!(!ready);
    // Step 3: the order connection comes back, the query converges to Filled.
    for k in [0u8, 1] {
        h.push(
            RingId::Rep(0),
            &conn(A, k, ConnStateKind::Up, ConnRole::Order, InstId(0), AC1, t),
        );
    }
    h.run();
    let mut ev = order_event(&d, hft_event::OrderEventKind::QueryResult, h.epoch());
    ev.detail = 3;
    ev.qty = d.qty;
    ev.cum_filled = d.qty;
    ev.px = d.px;
    ev.avg_px = d.px;
    h.push(RingId::Rep(0), &ev);
    h.run();
    let (ready, st) = in_event(&mut h, move |hx| {
        (hx.v.venue_ready(AC1, A), hx.v.order(r).map(|o| o.state))
    });
    assert_eq!(st, Some(OrderStateView::Filled));
    assert!(ready);
}

#[test]
fn sdk_133_order_gen_mismatch_none() {
    let mut h = rig();
    set_hook(|hx, ev, ctx| {
        if let EventRef::Bbo { inst, b, .. } = ev
            && *inst == A1
            && hx.s.strategy == S1
            && b.ask_px == px("50007")
        {
            let mut it = order_in(
                hx,
                GT1,
                A1,
                Side::Buy,
                Tif::Gtc,
                IntentClass::Open,
                px("49900"),
                q("0.001"),
            );
            it.tag = 77;
            let _ = ctx.place(it);
        }
    });
    let m = h.mark();
    h.quote(A1, "50000", "50007");
    h.run();
    clear_hook();
    let d = decs(&h.since(m))
        .into_iter()
        .find(|d| d.strategy == S1 && d.inst == A1)
        .expect("decision");
    h.ack(&d);
    let r = oref(&d);
    // Step 1.
    let o = in_event(&mut h, move |hx| hx.v.order(r)).expect("order view");
    assert_eq!(o.state, OrderStateView::Live);
    assert_eq!(o.group, GT1);
    assert_eq!(o.class, IntentClass::Open);
    assert_eq!(o.tag, 77);
    // Step 2: cancel, CancelAck, then a new order reuses the slot.
    set_hook(move |hx, ev, ctx| {
        if let EventRef::Bbo { inst, b, .. } = ev
            && *inst == A1
            && hx.s.strategy == S1
            && b.ask_px == px("50008")
        {
            let _ = ctx.cancel(r);
        }
    });
    h.quote(A1, "50000", "50008");
    h.run();
    clear_hook();
    let ca = order_event(&d, hft_event::OrderEventKind::CancelAck, h.epoch());
    h.push(RingId::Rep(0), &ca);
    h.run();
    // The terminal slot goes back to the tail of the FIFO free list after the OMS reuse
    // quarantine (sweep); new GTCs then walk the free list until one lands on it.
    let t0 = h.now();
    for k in 1..=7u64 {
        h.at(t0 + k * 1_000 * MS);
        h.run();
    }
    let mut d2 = None;
    for k in 0..16i64 {
        let ask = px("50010").0 + k;
        set_hook(move |hx, ev, ctx| {
            if let EventRef::Bbo { inst, b, .. } = ev
                && *inst == A1
                && hx.s.strategy == S1
                && b.ask_px.0 == ask
            {
                let _ = ctx.place(order_in(
                    hx,
                    GT1,
                    A1,
                    Side::Buy,
                    Tif::Gtc,
                    IntentClass::Open,
                    px("49900"),
                    q("0.001"),
                ));
            }
        });
        let m = h.mark();
        let a = format!("{}.{:02}", ask / 100, ask % 100);
        h.quote(A1, "50000", &a);
        h.run();
        clear_hook();
        let x = decs(&h.since(m))
            .into_iter()
            .find(|x| x.strategy == S1 && x.inst == A1 && x.action == hft_event::OrderAction::New)
            .expect("decision");
        assert_eq!(x.reject, 0, "{x:?}");
        if x.slot == d.slot {
            d2 = Some(x);
            break;
        }
    }
    let d2 = d2.expect("the freed slot is reused");
    assert_eq!(d2.r#gen, d.r#gen + 1);
    assert_eq!(in_event(&mut h, move |hx| hx.v.order(r)), None);
}

#[test]
fn sdk_134_target_view_and_model_side() {
    reset_script();
    let mut b = b_gt3();
    // s1's Model sees a2 Bbos (GT2, gix 1).
    set_subs(
        S1,
        vec![
            (A1, 0, EvMask::BBO, Part::Fast),
            (B1, 0, EvMask::BBO, Part::Fast),
            (A2, 1, EvMask::BBO, Part::Both),
            (B2, 1, EvMask::BBO, Part::Fast),
        ],
    );
    b.caps.max_timers = 512;
    let mut h = b.boot();
    h.steady();
    clear_log();
    let mut t = target(
        AC1,
        GT2,
        &[(0, q("0.01"))],
        hft_strategy_sdk::ExecSpec::TakerSlice(slice0()),
    );
    t.tag = 5;
    set_hook(move |hx, ev, ctx| {
        if let EventRef::Bbo { inst, b, .. } = ev
            && *inst == A2
            && hx.s.strategy == S1
            && b.ask_px == px("3000.7")
        {
            let _ = ctx.target(t);
            let _ = ctx.place(order_in(
                hx,
                GT2,
                A2,
                Side::Buy,
                Tif::Gtc,
                IntentClass::Open,
                px("2990"),
                q("0.01"),
            ));
        }
    });
    let m = h.mark();
    h.quote(A2, "3000", "3000.7");
    h.run();
    clear_hook();
    let d = decs(&h.since(m))
        .into_iter()
        .find(|x| x.strategy == S1 && x.inst == A2 && x.tif == Tif::Gtc)
        .expect("gtc decision");
    let r = oref(&d);
    // Step 1: the next a2 Bbo, fast part and Model.
    let fast_seen: Cap<Option<hft_strategy_sdk::TargetView>> = cap();
    let f2 = fast_seen.clone();
    set_hook(move |hx, ev, _| {
        if let EventRef::Bbo { inst, b, .. } = ev
            && *inst == A2
            && hx.s.strategy == S1
            && b.ask_px == px("3000.8")
        {
            f2.borrow_mut().push(hx.v.target(AC1, GT2));
        }
    });
    let model_seen: Cap<(u64, bool, bool)> = cap();
    let m2 = model_seen.clone();
    set_mhook(move |hx, ev, _| {
        if let EventRef::Bbo { inst, b, .. } = ev
            && *inst == A2
            && hx.s.strategy == S1
            && b.ask_px == px("3000.8")
        {
            m2.borrow_mut().push((
                hx.v.now().seq,
                hx.v.target(AC1, GT2).is_none(),
                hx.v.order(r).is_none(),
            ));
        }
    });
    let m = h.mark();
    h.quote(A2, "3000", "3000.8");
    h.run();
    clear_hook();
    let bbo_seq = h
        .since(m)
        .iter()
        .find(|x| x.kind == EventKind::BBO)
        .map(|x| x.seq)
        .expect("bbo");
    let tv = fast_seen.borrow()[0].expect("target view");
    assert_eq!(tv.intent.tag, 5);
    assert_eq!(tv.origin, hft_strategy_sdk::TargetOrigin::Strategy);
    let mut a = adv(&h, MemCkpt::default());
    catch_up(&mut a, &h);
    let ms = model_seen.borrow().clone();
    assert_eq!(ms, vec![(bbo_seq, true, true)]);
    // Step 2: clear_target.
    set_hook(|hx, ev, ctx| {
        if let EventRef::Bbo { inst, b, .. } = ev
            && *inst == A2
            && hx.s.strategy == S1
            && b.ask_px == px("3000.9")
        {
            let _ = ctx.clear_target(AC1, GT2);
        }
    });
    h.quote(A2, "3000", "3000.9");
    h.run();
    clear_hook();
    assert_eq!(in_event(&mut h, |hx| hx.v.target(AC1, GT2)), None);
}

// ------------------------------------------------------------------------------- funding

#[test]
fn sdk_136_funding_view() {
    reset_script();
    let mut b = b_gt3();
    set_subs(
        S1,
        vec![
            (A1, 0, EvMask::BBO, Part::Fast),
            (B1, 0, EvMask(EvMask::BBO.0 | EvMask::FUNDING.0), Part::Fast),
        ],
    );
    b.caps.max_timers = 512;
    let mut h = b.boot();
    h.steady();
    clear_log();
    let seen: Cap<hft_strategy_sdk::FundingView> = cap();
    let s2 = seen.clone();
    set_hook(move |hx, ev, _| {
        if let EventRef::Funding { inst, .. } = ev
            && *inst == B1
            && hx.s.strategy == S1
        {
            s2.borrow_mut().push(hx.v.funding(B1));
        }
    });
    let next = UTC0 + 3_600_000_000_000;
    let t1 = h.now();
    h.funding_md(B1, "0.0001", next, 28_800);
    h.adv(5 * MS);
    let t2 = h.now();
    h.funding_md(B1, "0.0001", next, 14_400);
    clear_hook();
    let v = seen.borrow().clone();
    assert_eq!(v.len(), 2);
    assert_eq!(
        (v[0].interval_s, v[0].prev_interval_s, v[0].upd_tsc),
        (28_800, 28_800, Tsc(t1))
    );
    assert_eq!(
        (v[1].interval_s, v[1].prev_interval_s, v[1].upd_tsc),
        (14_400, 28_800, Tsc(t2))
    );
    // Step 3: keyframe, cold start from it.
    let k = h.keyframe();
    let h2 =
        H::crash_restart(h, plan(SnapshotSel::AtSeq(k.seq), false, 1, 0), Rerun::default()).expect("restart");
    let f = *hft_strategy_sdk::Views::funding(&h2.s.engine().seq_views(), B1);
    assert_eq!(
        (f.interval_s, f.prev_interval_s, f.upd_tsc),
        (14_400, 28_800, Tsc(0))
    );
}

// ------------------------------------------------------------------------------- risk events

/// Captures s1's `Risk` and `Exec` deliveries.
fn risk_capture() -> (Cap<(AccountId, RiskEvent)>, Cap<(u16, ExecUpdate)>) {
    let re: Cap<(AccountId, RiskEvent)> = cap();
    let xe: Cap<(u16, ExecUpdate)> = cap();
    let (r2, x2) = (re.clone(), xe.clone());
    add_hook(move |hx, ev, _| {
        if hx.s.strategy != S1 {
            return;
        }
        match ev {
            EventRef::Risk { a, e } => r2.borrow_mut().push((*a, **e)),
            EventRef::Exec { gix, u } => x2.borrow_mut().push((*gix, **u)),
            _ => {}
        }
    });
    (re, xe)
}

fn kill_flatten(a: AccountId) -> ControlCmd<'static> {
    ControlCmd::Kill {
        scope: scope(StrategyId(0), a, GroupId(0)),
        level: KillLevel::Account,
        flatten: true,
        cancel_resting: true,
        hard: false,
    }
}

fn margin_ratio(h: &mut H, ratio: &str) {
    let mut hd = hft_event::MarginReportHdr::new_zeroed();
    hd.venue = B;
    hd.account = AC1;
    hd.source = 1;
    hd.margin_balance = n("100000");
    hd.maint_margin = n("1000");
    hd.avail_margin = n("90000");
    hd.margin_ratio = r(ratio);
    hd.rx_tsc = Tsc(h.now());
    h.push_bytes(RingId::Rep(0), EventKind::MARGIN_REPORT, hd.as_bytes().to_vec());
    h.run();
}

#[test]
fn sdk_209_risk_events_and_state() {
    // .1 G1 link Broken{Deadline}.
    {
        let mut h = rig();
        let (re, xe) = risk_capture();
        set_hook(|hx, ev, ctx| {
            if let EventRef::Bbo { inst, b, .. } = ev
                && *inst == A1
                && hx.s.strategy == S1
                && b.ask_px == px("50006")
            {
                let _ = ctx.place(pair_first(hx, q("0.01")));
            }
        });
        let m = h.mark();
        h.quote(A1, "50000", "50006");
        h.run();
        clear_hook();
        let d = decs(&h.since(m))
            .into_iter()
            .find(|d| d.strategy == S1 && d.link_role == 1)
            .expect("first leg");
        assert_eq!(d.reject, 0, "{d:?}");
        h.ack(&d);
        h.fill(&d, d.qty, d.qty, d.px);
        // The second leg gets no receipt; run past hedge_deadline_ms (3 000 ms).
        let t0 = h.now();
        for k in 1..=60u64 {
            h.at(t0 + k * 100 * MS);
            h.run();
        }
        let re = re.borrow();
        assert!(
            re.iter()
                .any(|(a, e)| *a == AC1 && *e == RiskEvent::HedgeBroken { group: GT1 }),
            "SDK-209.1 {re:?}"
        );
        let xe = xe.borrow();
        assert!(
            xe.iter().any(|(g, u)| *g == 0
                && u.kind == ExecUpdateKind::Broken(hft_strategy_sdk::BrokenReason::LinkBroken)),
            "SDK-209.1 {xe:?}"
        );
    }
    // .2 / .3 Kill{Account, flatten} then Resume{Kill}.
    {
        let mut h = rig();
        let (re, _) = risk_capture();
        let id = h.cmd_confirmed(AC1, &kill_flatten(AC1));
        h.run();
        assert_eq!(h.res(id).0, 0, "SDK-209.2 kill");
        let st = in_event(&mut h, |hx| hx.v.risk(AC1));
        assert!(
            re.borrow().iter().any(|(_, e)| matches!(e,
                RiskEvent::CapSet { source, .. } if *source == CapSource::KILL)),
            "SDK-209.2 {:?}",
            re.borrow()
        );
        assert!(st.flatten_lock && st.open_blocked, "SDK-209.2 {st:?}");
        re.borrow_mut().clear();
        let id = h.cmd_confirmed(
            AC1,
            &ControlCmd::Resume {
                scope: scope(StrategyId(0), AC1, GroupId(0)),
                source: CapSource::KILL,
            },
        );
        h.run();
        assert_eq!(h.res(id).0, 0, "SDK-209.3 resume");
        let st = in_event(&mut h, |hx| hx.v.risk(AC1));
        assert!(
            re.borrow().iter().any(|(_, e)| matches!(e,
                RiskEvent::CapCleared { source, .. } if *source == CapSource::KILL)),
            "SDK-209.3 {:?}",
            re.borrow()
        );
        assert!(!st.flatten_lock && !st.open_blocked, "SDK-209.3 {st:?}");
    }
    // .4 / .5 MarginStress L2 on B, then back to the safe zone.
    {
        let mut h = rig();
        let (re, _) = risk_capture();
        margin_ratio(&mut h, "0.85");
        let st = in_event(&mut h, |hx| hx.v.risk(AC1));
        assert!(
            re.borrow().iter().any(|(_, e)| matches!(e,
                RiskEvent::MarginStress { venue, level: MarginLevel::L2, deadline_tsc, .. }
                    if *venue == B && deadline_tsc.0 > 0)),
            "SDK-209.4 {:?}",
            re.borrow()
        );
        assert_eq!(st.margin_level, MarginLevel::L2, "SDK-209.4");
        re.borrow_mut().clear();
        margin_ratio(&mut h, "0");
        let st = in_event(&mut h, |hx| hx.v.risk(AC1));
        assert!(
            re.borrow().iter().any(|(_, e)| matches!(e,
                RiskEvent::CapCleared { source, .. } if *source == CapSource::MARGIN_STRESS)),
            "SDK-209.5 {:?}",
            re.borrow()
        );
        assert_eq!(st.margin_level, MarginLevel::None, "SDK-209.5");
    }
    // .6 no event.
    {
        let mut h = rig();
        let st = in_event(&mut h, |hx| hx.v.risk(AC1));
        assert_eq!(st.conn_liveness_ms, 2_000, "SDK-209.6");
        assert_eq!(
            st.dust_notional,
            hft_exec::ExecParams::default().dust_notional,
            "SDK-209.6"
        );
    }
}
