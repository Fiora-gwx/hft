//! Read-only views (TESTS.md "只读视图": SEQ-019–SEQ-029).

use hft_event::{EventKind, MarketHdr, RecordFlags};
use hft_risk::RiskView;
use hft_sequencer::driver::live::RingId;
use hft_strategy_sdk::{EventRef, Now, Views};
use hft_types::{AccountId, ConnId, InstId, Side, Tsc};
use zerocopy::{FromZeros, IntoBytes};

use crate::common::*;

fn now_on_bbo(h: &mut H, i: hft_types::InstId) -> Now {
    let c: Cap<Now> = cap();
    let c2 = c.clone();
    set_hook(move |hx, ev, _| {
        if let EventRef::Bbo { inst, .. } = ev
            && *inst == i
            && hx.s.strategy == S1
        {
            c2.borrow_mut().push(hx.v.now());
        }
    });
    h.quote(i, "50000", "50002");
    h.run();
    clear_hook();

    c.borrow().last().copied().expect("delivered")
}

#[test]
fn seq_019_now_fields() {
    reset_script();
    let mut h = b0().boot();
    h.steady();
    let ec = h
        .recs()
        .into_iter()
        .find(|r| r.kind == EventKind::ENGINE_CLOCK)
        .unwrap();
    let ep = h
        .recs()
        .into_iter()
        .find(|r| r.kind == EventKind::NOTE && r.seq > ec.seq)
        .unwrap();
    h.at(ec.tsc + 2_000_000);
    let m = h.mark();
    let n = now_on_bbo(&mut h, A1);
    let bbo = h.since(m).into_iter().find(|r| r.kind == EventKind::BBO).unwrap();
    assert_eq!(n.tsc, Tsc(ec.tsc + 2_000_000));
    assert_eq!(n.seq, bbo.seq);
    assert_eq!(n.engine_ns, 2_000_000);
    assert_eq!(n.boot_seq, ep.seq);
    assert_eq!(n.tsc_per_ms, 1_000_000);
    assert_eq!(n.utc_ns, UTC0 + (ec.tsc + 2_000_000 - T0) as i64);
    assert!(n.clock_ok);
}

#[test]
fn seq_020_clock_ok_from_timer_only() {
    reset_script();
    let mut h = b0().boot();
    h.steady();
    let mut cs = hft_event::ClockStatus::new_zeroed();
    cs.state = 2;
    h.push(RingId::Hk, &cs);
    h.run();
    assert!(now_on_bbo(&mut h, A1).clock_ok);
    assert_eq!(h.mode(S1, AC1, G1), hft_risk::mode::Effective::Live);
    h.trace_on();
    h.sys(9);
    assert!(h.trace().contains(&"apply.Clock"));
    assert!(!now_on_bbo(&mut h, A1).clock_ok);
    h.sys(8);
    assert!(now_on_bbo(&mut h, A1).clock_ok);
}

fn rx_of(h: &H, i: InstId) -> Tsc {
    let e = h.s.engine();
    e.market.bbo_view(&e.books, i).rx_tsc
}

fn bbo_rx(i: InstId, rx: u64, upd: u64) -> hft_event::Bbo {
    bbo(i, inst_venue(i), 0, "50000", "50002", upd, rx)
}

#[test]
fn seq_021_rx_tsc_visibility() {
    reset_script();
    let mut b = b0();
    b.params.retain(|p| p.strategy == S2);
    let mut h = b.boot();
    h.step();
    // .1 nothing received in this run.
    assert_eq!(rx_of(&h, A1), Tsc(0));
    // .2
    h.push(RingId::Bbo(0), &bbo_rx(A1, 5_000, 10));
    h.run();
    assert_eq!(rx_of(&h, A1), Tsc(5_000));
    // .3 keyframe.
    let kf = h.keyframe();
    assert_eq!(rx_of(&h, A1), Tsc(0));
    // .4
    h.push(RingId::Bbo(0), &bbo_rx(A1, 9_000, 11));
    h.run();
    assert_eq!(rx_of(&h, A1), Tsc(9_000));
    // .5 verify from the keyframe reproduces the run.
    let (kh, _) = snap(&kf);
    let m = h.verify_from(Some(kf.seq));
    assert!(m.is_empty(), "{m:?} (at_seq {})", kh.at_seq);
}

#[test]
fn seq_022_hw_rx_and_sw_ts() {
    reset_script();
    let mut h = b0().boot();
    h.steady();
    let mut b = bbo_rx(A1, 100, 1_000);
    b.h.hw_rx_ns = 111;
    h.push(RingId::Bbo(0), &b);
    h.run();
    let row = |h: &H| *h.s.engine().market.row(A1).unwrap();
    assert_eq!(row(&h).bbo_hw_rx_ns, 111);
    assert_eq!(row(&h).bbo_flags & 2, 0);
    let mut b = bbo_rx(A1, 101, 1_001);
    b.h.hw_rx_ns = 222;
    h.push_e(
        RingId::Bbo(0),
        Elem {
            kind: EventKind::BBO,
            flags: RecordFlags::SW_TS,
            body: b.as_bytes().to_vec(),
        },
    );
    h.run();
    assert_eq!(row(&h).bbo_hw_rx_ns, 222);
    assert_eq!(row(&h).bbo_flags & 2, 2);
    let mut b = bbo_rx(A1, 102, 999);
    b.h.hw_rx_ns = 333;
    h.push(RingId::Bbo(0), &b);
    h.run();
    assert_eq!(row(&h).bbo_hw_rx_ns, 222);
    assert_eq!(row(&h).bbo_flags & 2, 2);
}

#[test]
fn seq_023_funding_upd_tsc_visibility() {
    reset_script();
    let mut h = b0().boot();
    h.steady();
    let mut f = hft_event::Funding::new_zeroed();
    f.h = MarketHdr::new_zeroed();
    f.h.venue = B;
    f.h.inst = B1;
    f.h.rx_tsc = 7_000;
    f.rate = r("0.0001");
    f.next_funding_ns = UTC0 + 3_600_000_000_000;
    f.interval_s = 28_800;
    h.push(RingId::Md(1), &f);
    h.run();
    let fv = |h: &H| h.s.engine().market.funding[B1.0 as usize];
    assert_eq!(fv(&h).upd_tsc, Tsc(7_000));
    let before = fv(&h);
    h.keyframe();
    let after = fv(&h);
    assert_eq!(after.upd_tsc, Tsc(0));
    assert_eq!(after.rate, before.rate);
    assert_eq!(after.next_settle_ns, before.next_settle_ns);
}

fn lev_report(h: &mut H, x: &str) {
    let l = hft_event::LeverageItem {
        inst: B1,
        _pad: 0,
        leverage: r(x),
    };
    let hd = cfg_hdr(AC1, B, 1);
    let mut body = hd.as_bytes().to_vec();
    body.extend_from_slice(l.as_bytes());
    h.push_bytes(RingId::Rep(0), EventKind::ACCOUNT_CONFIG_REPORT, body);
    h.run();
}

#[test]
fn seq_024_leverage_x100_floor() {
    reset_script();
    let mut h = b0().boot();
    h.step();
    assert_eq!(h.s.engine().seq_views().leverage(AC1, B1), None);
    for (x, want) in [("10.00", 1000), ("12.349", 1234), ("12.3401", 1234), ("0.009", 0)] {
        lev_report(&mut h, x);
        assert_eq!(h.s.engine().seq_views().leverage(AC1, B1), Some(want), "{x}");
    }
}

#[test]
fn seq_027_open_qty_immediate() {
    reset_script();
    let mut h = b0().boot();
    h.steady();
    let m = h.mark();
    h.quote(B1, "50010", "50011");
    set_hook(|hx, ev, ctx| {
        if let EventRef::Bbo { inst, .. } = ev
            && *inst == B1
            && hx.s.strategy == S1
        {
            let b = hx.v.bbo(B1);
            let mut ioc = intent(
                hx,
                B1,
                Side::Sell,
                hft_types::Tif::Ioc,
                hft_strategy_sdk::IntentClass::Open,
                b.bid_px,
                q("0.002"),
            );
            ioc.group = G1;
            let _ = ctx.place(ioc);
            let mut gtc = intent(
                hx,
                B1,
                Side::Sell,
                hft_types::Tif::Gtc,
                hft_strategy_sdk::IntentClass::Open,
                hft_types::Px(b.ask_px.0 + 1_000),
                q("0.005"),
            );
            gtc.group = G1;
            let _ = ctx.place(gtc);
        }
    });
    h.run();
    clear_hook();
    let ds = decisions(&h.since(m));
    let gtc = ds
        .iter()
        .find(|d| d.1.tif == hft_types::Tif::Gtc)
        .expect("gtc decision")
        .1;
    h.ack(&gtc);
    let ra = h.s.engine().risk_adapter();
    assert_eq!(ra.open_qty(AC1, B1, Side::Sell), q("0.007"));
    assert_eq!(ra.open_qty_immediate(AC1, B1, Side::Sell), q("0.002"));
}

#[test]
fn seq_029_view_sources() {
    reset_script();
    let mut h = b0().boot();
    h.steady();
    let e = h.s.engine();
    let v = e.seq_views();
    assert_eq!(
        hft_sequencer::views::eff_mode_pub(e.risk.mode.effective(hft_risk::mode::CellKey {
            strategy: S1,
            account: AC1,
            group: G1
        })),
        v.mode(AC1, G1)
    );
    assert_eq!(v.venue_ready(AC1, B), e.oms.venue_ready(B, AC1));
    assert_eq!(*v.legs(AC1, G1), e.oms.legs(AC1, G1).view);
    assert_eq!(v.funding(B1).rate, e.market.row(B1).unwrap().funding_rate);
    assert_eq!(v.mark(B1), e.market.row(B1).unwrap().mark_px);
    assert_eq!(v.inst(B1).lot, e.rd.inst(B1).lot);
    let _ = (AccountId(0), ConnId(0));
}

#[test]
fn seq_029_bbo_group_cache_matches_ledger_after_fills_and_recovery() {
    reset_script();
    let mut b = b0();
    let extra = hft_types::GroupId(0x0100_0002);
    b.params[0].groups.push(gspec_eth(extra));
    let mut h = b.boot();
    h.steady();
    let d = h.order_at_touch(AC1, A1, Side::Buy, "0.001");
    h.fill(&d, q("0.001"), q("0.001"), d.px);
    let kf = h.keyframe().seq;
    for (i, bid, ask) in [
        (A1, "51000", "51001"),
        (A2, "3100", "3101"),
        (B1, "51010", "51011"),
        (A1, "52000", "52001"),
    ] {
        h.quote(i, bid, ask);
        h.run();
        let e = h.s.engine();
        for slot in &h.s.instances().slots {
            for (_, group) in slot.groups().unwrap().iter() {
                assert_eq!(
                    *e.seq_views().group_pos(slot.account, group.id),
                    e.ledger.group_pos(slot.account, group.id)
                );
            }
        }
    }
    h.keyframe();
    assert!(h.verify_from(Some(kf)).is_empty());
    let mut h = h
        .crash_restart(
            plan(hft_sequencer::SnapshotSel::Latest, true, 1, 0),
            Rerun::default(),
        )
        .unwrap();
    h.bring_up(false);
    for i in [A1, A2, B1] {
        h.quote(i, "53000", "53001");
        h.run();
        let e = h.s.engine();
        for slot in &h.s.instances().slots {
            for (_, group) in slot.groups().unwrap().iter() {
                assert_eq!(
                    *e.seq_views().group_pos(slot.account, group.id),
                    e.ledger.group_pos(slot.account, group.id)
                );
            }
        }
    }
}

/// Account 1: 1 000 USDT free at A with a resting a1 buy of 300 USDT; at B a reported available
/// margin of 800 USDT with a resting b1 Open sell whose margin at 3× is 150 USDT.
#[test]
fn seq_026_affordability_reads() {
    reset_script();
    let mut h = b0().boot();
    // The first balance of (A, 1, USDT) initialises the ledger's cash; steady's later report
    // only reconciles.
    let br = hft_event::BalanceReport {
        venue: A,
        account: AC1,
        asset: USDT,
        _pad: 0,
        free: q("1000"),
        locked: hft_types::Qty(0),
        ex_ts_ns: 0,
        rx_tsc: Tsc(h.now()),
    };
    h.push(RingId::Rep(0), &br);
    h.steady();
    h.margin_report(AC1, "800", "0", "800");
    h.quote(B1, "49990", "49991");
    h.run();
    let m = h.mark();
    set_hook(|hx, ev, ctx| {
        if let EventRef::Bbo { inst, .. } = ev
            && *inst == A1
            && hx.s.strategy == S1
        {
            let mut buy = intent(
                hx,
                A1,
                Side::Buy,
                hft_types::Tif::Gtc,
                hft_strategy_sdk::IntentClass::Open,
                px("50000"),
                q("0.006"),
            );
            buy.group = G1;
            let _ = ctx.place(buy);
            let mut sell = intent(
                hx,
                B1,
                Side::Sell,
                hft_types::Tif::Gtc,
                hft_strategy_sdk::IntentClass::Open,
                px("50000"),
                q("0.009"),
            );
            sell.group = G1;
            let _ = ctx.place(sell);
        }
    });
    h.quote(A1, "50000", "50003");
    h.run();
    clear_hook();
    let ds = decisions(&h.since(m));
    assert_eq!(ds.len(), 2, "{ds:?}");
    for (_, d) in &ds {
        assert_eq!(d.reject, 0, "{d:?}");
        h.ack(d);
    }
    let ra = h.s.engine().risk_adapter();
    assert_eq!(ra.avail_quote(AC1, A, USDT), Some(n("700")));
    assert_eq!(ra.avail_margin(AC1, B), Some(n("650")));
}

/// SEQ-447: every L2 batch, exchange or top-N L2 snapshot and trade leaves its `MarketHdr` stamps
/// for `Views::md_count`/`md_stamp` (exchange snapshots too, although they deliver no Book event);
/// the last 16 are kept.
#[test]
fn seq_447_md_stamps() {
    use hft_event::{L2BatchHdr, L2SnapHdr, Level};
    use hft_strategy_sdk::MdStampView;
    reset_script();
    let mut h = b0().boot();
    h.steady();
    let lv = |p: &str| Level {
        px: px(p),
        qty: q("1"),
    };
    let hdr = |h: &H, ex: i64, sw: i64| {
        let mut m = MarketHdr::new_zeroed();
        m.venue = A;
        m.inst = A1;
        m.rx_tsc = h.now();
        m.ex_ts_ns = ex;
        m.sw_rx_ns = sw;
        m
    };
    // .1 an exchange snapshot, a batch in sequence and a trade.
    let mut sn = L2SnapHdr::new_zeroed();
    sn.h = hdr(&h, 1_000_000_000, 1_010_000_000);
    sn.snap_id = 1;
    sn.origin = L2SnapHdr::EXCHANGE;
    sn.n_bids = 1;
    sn.n_asks = 1;
    let mut body = sn.as_bytes().to_vec();
    body.extend_from_slice(lv("50000").as_bytes());
    body.extend_from_slice(lv("50002").as_bytes());
    h.push_bytes(RingId::Md(0), EventKind::L2_SNAPSHOT, body);
    h.run();
    let mut bt = L2BatchHdr::new_zeroed();
    bt.h = hdr(&h, 2_000_000_000, 2_012_000_000);
    bt.prev_id = 1;
    bt.last_id = 2;
    bt.n_bids = 1;
    let mut body = bt.as_bytes().to_vec();
    body.extend_from_slice(lv("50001").as_bytes());
    h.push_bytes(RingId::Md(0), EventKind::L2_BATCH, body);
    h.run();
    let mut t = trade(A1, 1);
    t.h = hdr(&h, 3_000_000_000, 3_013_000_000);
    h.push(RingId::Md(0), &t);
    h.run();
    let c: Cap<(u64, Vec<Option<MdStampView>>)> = cap();
    let c2 = c.clone();
    set_hook(move |hx, ev, _| {
        if let EventRef::Bbo { inst, .. } = ev
            && *inst == A1
            && hx.s.strategy == S1
        {
            let n = hx.v.md_count(A1);
            c2.borrow_mut()
                .push((n, (0..=n + 1).map(|k| hx.v.md_stamp(A1, k)).collect()));
        }
    });
    h.quote(A1, "50000", "50002");
    h.run();
    clear_hook();
    let (n, s) = c.borrow().last().cloned().expect("delivered");
    assert_eq!(n, 3, "SEQ-447.1");
    let got: Vec<_> = s[1..=3]
        .iter()
        .map(|m| m.map(|m| (m.kind, m.ex_ts_ns, m.sw_rx_ns)))
        .collect();
    assert_eq!(
        got,
        vec![
            Some((MdStampView::MD_SNAPSHOT, 1_000_000_000, 1_010_000_000)),
            Some((MdStampView::MD_BATCH, 2_000_000_000, 2_012_000_000)),
            Some((MdStampView::MD_TRADE, 3_000_000_000, 3_013_000_000)),
        ],
        "SEQ-447.1"
    );
    assert!(s[1].is_some_and(|m| m.rx_tsc.0 > 0), "SEQ-447.1 rx_tsc");
    assert_eq!((s[0], s[4]), (None, None), "SEQ-447.1 out of range");
    // .2 17 more trades: 20 in all, the last 16 kept.
    for k in 0..17 {
        let mut t = trade(A1, 2 + k);
        t.h = hdr(&h, 4_000_000_000 + k as i64, 0);
        h.push(RingId::Md(0), &t);
        h.run();
    }
    let m = &h.s.engine().market;
    assert_eq!(m.md_count(A1), 20, "SEQ-447.2");
    assert_eq!(m.md_stamp(A1, 4), None, "SEQ-447.2 dropped");
    assert_eq!(
        m.md_stamp(A1, 5).map(|x| (x.kind, x.ex_ts_ns)),
        Some((MdStampView::MD_TRADE, 4_000_000_001)),
        "SEQ-447.2 oldest kept"
    );
    assert_eq!(
        m.md_stamp(A1, 20).map(|x| x.ex_ts_ns),
        Some(4_000_000_016),
        "SEQ-447.2"
    );
    assert_eq!(m.md_count(B1), 0, "SEQ-447.2 other inst");
}
