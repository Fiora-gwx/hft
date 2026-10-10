//! Event delivery to the fast part and the Model (SDK-040–SDK-055, SDK-144).

use hft_event::{
    ConnRole, ConnStateKind, ControlCmd, EventKind, ExternalCause, ExternalFill, Funding, IndexPrice,
    L2SnapHdr, Level, Liquidation, MarkPrice, MarketHdr, Note, NoteSource, Trade,
};
use hft_sequencer::advisor::AdvisorCfg;
use hft_sequencer::codes::{note, seq_note, status};
use hft_sequencer::driver::live::RingId;
use hft_strategy_sdk::params::encode_cmd;
use hft_strategy_sdk::{CmdView, EvMask, EventRef, ExecSpec, GIX_NONE, IntentClass, OwnEvMask, Part};
use hft_types::{AccountId, AssetId, GROUP_NONE, GroupId, InstId, Qty, Side, StrategyId, Tif, Tsc};
use zerocopy::{FromZeros, IntoBytes};

use super::*;
use crate::common::*;

fn hdr(i: InstId, t: u64) -> MarketHdr {
    let mut h = MarketHdr::new_zeroed();
    h.venue = inst_venue(i);
    h.inst = i;
    h.rx_tsc = t;
    h
}

/// A `StrategyCmd` of s1 (payload padded into the fixed 256 B array).
pub fn strategy_cmd(
    instance: StrategyId,
    group: GroupId,
    reduce_only: u8,
    payload: &[u8],
) -> ControlCmd<'static> {
    let mut p = [0u8; 256];
    p[..payload.len()].copy_from_slice(payload);
    ControlCmd::StrategyCmd {
        instance,
        reduce_only,
        len: payload.len() as u16,
        group,
        payload: p,
    }
}

/// op 1 `open` with `qty` lots.
pub fn op1(qty: i64) -> Vec<u8> {
    encode_cmd(&CMDS[0], &[qty]).expect("op 1")
}

/// `(kind, Debug text, seq)` of the Order/Exec/Manual deliveries of one part.
fn own_events(gs: &[Got]) -> Vec<(&'static str, String, u64)> {
    gs.iter()
        .filter(|g| matches!(g.kind, "Order" | "Exec" | "Manual"))
        .map(|g| (g.kind, g.text.clone(), g.now.seq))
        .collect()
}

/// The SDK-040 script: an IOC with ack + two fills, a TakerSlice target with one slice filled,
/// a `StrategyCmd`. Returns the harness.
fn own_scenario(mask: Option<u8>) -> H {
    let b = b_gt3();
    if let Some(m) = mask {
        SCRIPT.with(|x| x.borrow_mut().model_own.push((S1, m)));
    }
    let mut h = b.boot();
    h.steady();
    clear_log();
    // 1. IOC 0.001 a1, ack, partial, full (naked notional stays under the exposure gate's
    // minimum, so the slice below is not gated).
    let trig = h.upd + 1;
    add_hook(move |hx, ev, ctx| {
        if let EventRef::Bbo { inst, b, .. } = ev
            && *inst == A1
            && hx.s.strategy == S1
            && b.update_id == trig
        {
            let mut it = order_in(
                hx,
                GT1,
                A1,
                Side::Buy,
                Tif::Ioc,
                IntentClass::Open,
                b.ask_px,
                q("0.001"),
            );
            it.tag = 11;
            let _ = ctx.place(it);
        }
    });
    let m = h.mark();
    h.quote(A1, "50000", "50001");
    h.run();
    let d = decs(&h.since(m)).into_iter().find(|d| d.inst == A1).expect("IOC");
    h.ack(&d);
    h.fill(&d, q("0.0004"), q("0.0004"), d.px);
    h.fill(&d, q("0.0006"), q("0.001"), d.px);
    // 2. TakerSlice target on GT1 (spot leg to 0.0013), one slice filled. Hooks trigger on a
    // fixed update id so the advisor's mirror replays the same intents.
    let trig2 = h.upd + 1;
    add_hook(move |hx, ev, ctx| {
        if let EventRef::Bbo { inst, b, .. } = ev
            && *inst == A1
            && hx.s.strategy == S1
            && b.update_id == trig2
        {
            let mut t = target(AC1, GT1, &[(0, q("0.0013"))], ExecSpec::TakerSlice(slice0()));
            t.tag = 22;
            let _ = ctx.target(t);
        }
    });
    let m = h.mark();
    h.quote(A1, "50000", "50002");
    h.run();
    let s = decs(&h.since(m))
        .into_iter()
        .find(|d| d.inst == A1 && d.slot.0 != 0)
        .unwrap_or_else(|| panic!("slice: {:?} {:?}", execs(&h.since(m)), decs(&h.since(m))));
    h.ack(&s);
    h.fill(&s, s.qty, s.qty, s.px);
    // 3. StrategyCmd op 1 on GT2.
    let id = h.cmd_confirmed(AC1, &strategy_cmd(S1, GT2, 0, &op1(5)));
    h.run();
    assert_eq!(h.res(id).0, status::APPLIED);
    h
}

/// The Model's deliveries of s1 when `AdvRig` replays the whole journal of `h`.
fn model_run(h: &H) -> Vec<Got> {
    clear_log();
    let mut a = adv(h, MemCkpt::default());
    catch_up(&mut a, h);
    model(S1)
}

#[test]
fn sdk_040_model_own_mirrors_fast() {
    let h = own_scenario(Some(OwnEvMask::ORDER.0 | OwnEvMask::EXEC.0 | OwnEvMask::MANUAL.0));
    let f = own_events(&fast(S1));
    let kinds = |v: &[(&str, String, u64)], k: &str| v.iter().filter(|x| x.0 == k).count();
    assert_eq!(kinds(&f, "Order"), 3, "{f:?}");
    assert!(kinds(&f, "Exec") >= 1);
    assert_eq!(kinds(&f, "Manual"), 1);
    let m1 = model_run(&h);
    let mo = own_events(&m1);
    assert_eq!(mo, f);
    // A second replay from the start gives the same Model call record.
    let m2 = model_run(&h);
    let t1: Vec<(&str, u16, u64, String)> = m1
        .iter()
        .map(|g| (g.kind, g.gix, g.now.seq, g.text.clone()))
        .collect();
    let t2: Vec<(&str, u16, u64, String)> = m2
        .iter()
        .map(|g| (g.kind, g.gix, g.now.seq, g.text.clone()))
        .collect();
    assert_eq!(t1, t2);
}

#[test]
fn sdk_041_model_own_mask_respected() {
    let h = own_scenario(None);
    assert!(own_events(&fast(S1)).iter().any(|x| x.0 == "Order"));
    assert!(own_events(&model_run(&h)).is_empty());
    let h = own_scenario(Some(OwnEvMask::EXEC.0));
    let m = own_events(&model_run(&h));
    assert!(!m.is_empty());
    assert!(m.iter().all(|x| x.0 == "Exec"), "{m:?}");
}

const ALL7: u8 = EvMask::BBO.0
    | EvMask::BOOK.0
    | EvMask::TRADE.0
    | EvMask::FUNDING.0
    | EvMask::MARK.0
    | EvMask::INDEX.0
    | EvMask::LIQUIDATION.0;

/// One run of SDK-042: b2 (`I2` of the case; in no group, so its L2 book does not hold up the
/// cells' `SetMode Live`) subscribed with `sub` (None = not subscribed); returns the variants
/// the fast part and the Model got for b2.
fn variants(sub: Option<(u8, Part)>) -> (Vec<&'static str>, Vec<&'static str>) {
    reset_script();
    let mut b = b0();
    if let Some(s) = b.rd.insts.iter_mut().find(|s| s.id == B2) {
        s.l2_depth = 5;
    }
    let mut subs = vec![(A1, 0, EvMask::BBO, Part::Fast)];
    if let Some((m, p)) = sub {
        subs.push((B2, 0, EvMask(m), p));
    }
    set_subs(S1, subs);
    let mut h = b.boot();
    h.steady();
    let mut a = adv(&h, MemCkpt::default());
    catch_up(&mut a, &h);
    clear_log();
    let t = h.now();
    h.quote(B2, "3000.1", "3000.6");
    h.run();
    let mut body = L2SnapHdr {
        h: hdr(B2, t),
        snap_id: 9,
        origin: L2SnapHdr::TOP_N,
        _pad: [0; 3],
        n_bids: 5,
        n_asks: 5,
    }
    .as_bytes()
    .to_vec();
    for k in 0..5i64 {
        body.extend_from_slice(
            Level {
                px: hft_types::Px(300_010 - k),
                qty: q("1"),
            }
            .as_bytes(),
        );
    }
    for k in 0..5i64 {
        body.extend_from_slice(
            Level {
                px: hft_types::Px(300_060 + k),
                qty: q("1"),
            }
            .as_bytes(),
        );
    }
    h.push_bytes(RingId::Md(1), EventKind::L2_SNAPSHOT, body);
    h.run();
    h.push(
        RingId::Md(1),
        &Trade {
            h: hdr(B2, t),
            px: px("3000.2"),
            qty: q("0.01"),
            trade_id: 1,
            taker_side: Side::Buy,
            _pad: [0; 7],
        },
    );
    h.run();
    h.funding_md(B2, "0.0001", UTC0 + 3_600_000_000_000, 28_800);
    h.mark_md(B2, "3000.3");
    h.push(
        RingId::Md(1),
        &IndexPrice {
            h: hdr(B2, t),
            px: px("3000.2"),
        },
    );
    h.run();
    h.push(
        RingId::Md(1),
        &Liquidation {
            h: hdr(B2, t),
            px: px("3000"),
            qty: q("0.5"),
            side: Side::Sell,
            _pad: [0; 7],
        },
    );
    h.run();
    let fast_v: Vec<&str> = fast(S1)
        .into_iter()
        .filter(|g| g.inst == B2)
        .map(|g| g.kind)
        .collect();
    clear_log();
    catch_up(&mut a, &h);
    let model_v: Vec<&str> = model(S1)
        .into_iter()
        .filter(|g| g.inst == B2)
        .map(|g| g.kind)
        .collect();
    (fast_v, model_v)
}

#[test]
fn sdk_042_market_variants_by_mask_and_part() {
    let all = vec!["Bbo", "Book", "Trade", "Funding", "Mark", "Index", "Liquidation"];
    let rows: [(u32, Option<(u8, Part)>, Vec<&str>, Vec<&str>); 6] = [
        (1, Some((ALL7, Part::Fast)), all.clone(), vec![]),
        (2, Some((ALL7, Part::Model)), vec![], all.clone()),
        (3, Some((ALL7, Part::Both)), all.clone(), all.clone()),
        (4, Some((EvMask::BBO.0, Part::Both)), vec!["Bbo"], vec!["Bbo"]),
        (
            5,
            Some((EvMask::TRADE.0 | EvMask::LIQUIDATION.0, Part::Fast)),
            vec!["Trade", "Liquidation"],
            vec![],
        ),
        (6, None, vec![], vec![]),
    ];
    for (row, sub, want_f, want_m) in rows {
        let (f, m) = variants(sub);
        assert_eq!(f, want_f, "SDK-042.{row} fast");
        assert_eq!(m, want_m, "SDK-042.{row} model");
    }
}

#[test]
fn sdk_044_engine_only_feed_not_delivered() {
    reset_script();
    // Plain B0: s1 has G1 (a1 + b1); b2 is in no group of any instance.
    let mut h = b0().boot();
    h.steady();
    let mut a = adv(&h, MemCkpt::default());
    catch_up(&mut a, &h);
    // Account-level (GROUP_NONE) b2 position of acct-1: an unattributable external fill.
    let x = ExternalFill {
        venue: B,
        account: AC1,
        side: Side::Buy,
        cause: ExternalCause::Manual,
        inst: B2,
        qty: q("0.5"),
        px: px("3000"),
        fee: Qty(0),
        fee_asset: AssetId(0),
        _pad: [0; 6],
        exch_order_id: 0,
        trade_id: 501,
        ex_ts_ns: 0,
        rx_tsc: Tsc(h.now()),
        fill_seq: 0,
    };
    h.push(RingId::Rep(0), &x);
    h.run();
    let upnl0 = {
        use hft_strategy_sdk::PortfolioRead;
        h.s.engine().ledger.portfolio(AC1).perp(B, B2)
    };
    assert_eq!(upnl0.qty, q("0.5"));
    clear_log();
    h.mark_md(B2, "3100");
    h.funding_md(B2, "0.0001", UTC0 + 3_600_000_000_000, 28_800);
    h.quote(B2, "3099", "3101");
    h.run();
    let upnl1 = {
        use hft_strategy_sdk::PortfolioRead;
        h.s.engine().ledger.portfolio(AC1).perp(B, B2).upnl
    };
    assert_ne!(upnl1, upnl0.upnl, "the ledger revalued b2 at the new mark");
    assert!(!log().iter().any(|g| g.s == S1 && g.inst == B2), "{:?}", log());
    clear_log();
    catch_up(&mut a, &h);
    assert!(!model(S1).iter().any(|g| g.inst == B2));
}

#[test]
fn sdk_045_order_only_for_placed() {
    let mut h = rig();
    let out = cap::<String>();
    let o2 = out.clone();
    let trig = h.upd + 1;
    let mut placed = None;
    add_hook(move |hx, ev, ctx| {
        if let EventRef::Bbo { inst, b, .. } = ev
            && *inst == A1
            && hx.s.strategy == S1
            && b.update_id == trig
        {
            let mut it = order_in(
                hx,
                GT1,
                A1,
                Side::Buy,
                Tif::Ioc,
                IntentClass::Open,
                b.ask_px,
                q("0.01"),
            );
            it.tag = 11;
            let hd = ctx.place(it).expect("room");
            placed = Some(hd);
            let mut t = target(AC1, GT2, &[(0, q("0.01"))], ExecSpec::TakerSlice(slice0()));
            t.tag = 22;
            let _ = ctx.target(t);
        } else if hx.s.strategy == S1
            && let Some(hd) = placed.take()
        {
            o2.borrow_mut().push(format!("{:?}", ctx.outcome(hd)));
        }
    });
    let m = h.mark();
    h.quote(A1, "50000", "50001");
    h.run();
    let ds = decs(&h.since(m));
    let d = *ds.iter().find(|d| d.inst == A1).expect("IOC");
    let s = *ds.iter().find(|d| d.inst == A2 && d.slot.0 != 0).expect("slice");
    clear_log();
    h.quote(A1, "50000", "50001");
    h.run();
    h.ack(&d);
    h.fill(&d, d.qty, d.qty, d.px);
    h.ack(&s);
    h.fill(&s, s.qty, s.qty, s.px);
    let outcome = out.borrow().first().cloned().expect("outcome read");
    assert!(
        outcome.contains(&format!("slot: {}", d.slot.0)) && outcome.contains("Placed"),
        "{outcome}"
    );
    let ord: Vec<Got> = fast(S1).into_iter().filter(|g| g.kind == "Order").collect();
    assert_eq!(ord.len(), 2, "{ord:?}");
    assert!(ord.iter().all(|g| g.gix == 0 && g.text.contains("tag: 11")));
    assert!(ord[0].text.contains("Acked"));
    assert!(ord[1].text.contains("Filled") && ord[1].text.contains("done: true"));
    assert!(
        ord.iter()
            .all(|g| g.text.contains(&format!("slot: {}", d.slot.0)))
    );
    let ex: Vec<Got> = fast(S1).into_iter().filter(|g| g.kind == "Exec").collect();
    assert!(
        ex.iter()
            .any(|g| g.gix == 1 && g.text.contains("tag: 22") && g.text.contains("Progress { leg_ix: 0")),
        "{ex:?}"
    );
    assert!(
        !fast(S1)
            .iter()
            .any(|g| g.kind == "Order" && g.text.contains("tag: 22"))
    );
}

fn ext(h: &H, inst: InstId, side: Side, cause: ExternalCause, qty: &str, trade_id: u64) -> ExternalFill {
    ExternalFill {
        venue: inst_venue(inst),
        account: AC1,
        side,
        cause,
        inst,
        qty: q(qty),
        px: if inst == B2 || inst == A2 {
            px("3000")
        } else {
            px("50000")
        },
        fee: Qty(0),
        fee_asset: AssetId(0),
        _pad: [0; 6],
        exch_order_id: 0,
        trade_id,
        ex_ts_ns: 0,
        rx_tsc: Tsc(h.now()),
        fill_seq: 0,
    }
}

#[test]
fn sdk_046_external_fast_only() {
    let b = b_gt3();
    SCRIPT.with(|x| {
        x.borrow_mut()
            .model_own
            .push((S1, OwnEvMask::ORDER.0 | OwnEvMask::EXEC.0 | OwnEvMask::MANUAL.0))
    });
    let mut h = b.boot();
    h.steady();
    open_in(&mut h, S1, GT2, B2, Side::Sell, "0.02");
    let mut a = adv(&h, MemCkpt::default());
    catch_up(&mut a, &h);
    clear_log();
    // 1. Attributable to GT2 (its leg 1, b2).
    let x = ext(&h, B2, Side::Buy, ExternalCause::Liquidation, "0.02", 601);
    h.push(RingId::Rep(0), &x);
    h.run();
    // 2. a3 is in no group of s1.
    let x = ext(&h, A3, Side::Sell, ExternalCause::Manual, "0.001", 602);
    h.push(RingId::Rep(0), &x);
    h.run();
    let e: Vec<Got> = fast(S1).into_iter().filter(|g| g.kind == "External").collect();
    assert_eq!(e.len(), 2, "{e:?}");
    assert_eq!(e[0].gix, 1);
    assert!(e[0].text.contains(&format!("group: {:?}", GT2)) && e[0].text.contains("Liquidation"));
    assert!(!e[0].text.contains("deadline_tsc: Tsc(0)"), "{}", e[0].text);
    assert_eq!(e[1].gix, GIX_NONE);
    assert!(e[1].text.contains(&format!("group: {:?}", GROUP_NONE)));
    assert!(e[1].text.contains("deadline_tsc: Tsc(0)"), "{}", e[1].text);
    catch_up(&mut a, &h);
    assert!(!model(S1).iter().any(|g| g.kind == "External"));
}

#[test]
fn sdk_047_risk_mode_conn_fast_only() {
    let b = b_gt3();
    set_subs(S1, vec![(A1, 0, EvMask::BBO, Part::Both)]);
    SCRIPT.with(|x| {
        x.borrow_mut()
            .model_own
            .push((S1, OwnEvMask::ORDER.0 | OwnEvMask::EXEC.0 | OwnEvMask::MANUAL.0))
    });
    let mut h = b.boot();
    h.steady();
    let mut a = adv(&h, MemCkpt::default());
    catch_up(&mut a, &h);
    clear_log();
    // 1. Margin ratio 0.90 on B → MarginStress L2.
    let mut hd = hft_event::MarginReportHdr::new_zeroed();
    hd.venue = B;
    hd.account = AC1;
    hd.source = 1;
    hd.margin_balance = n("100000");
    hd.maint_margin = n("90000");
    hd.avail_margin = n("10000");
    hd.margin_ratio = r("0.90");
    hd.rx_tsc = Tsc(h.now());
    h.push_bytes(RingId::Rep(0), EventKind::MARGIN_REPORT, hd.as_bytes().to_vec());
    h.run();
    let risk: Vec<Got> = fast(S1).into_iter().filter(|g| g.kind == "Risk").collect();
    assert!(
        risk.iter()
            .any(|g| g.text.contains("MarginStress") && g.text.contains("level: L2")),
        "{risk:?}"
    );
    // 2. SetMode CloseOnly of GT2.
    h.cmd(
        AC1,
        &ControlCmd::SetMode {
            scope: scope(S1, AC1, GT2),
            mode: hft_types::RunMode::CloseOnly,
        },
    );
    h.run();
    assert!(
        fast(S1).iter().any(|g| g.kind == "Mode"
            && g.gix == 1
            && g.text.contains("old: Live")
            && g.text.contains("new: CloseOnly")),
        "{:?}",
        fast(S1)
    );
    // 3. A's BBO connection down.
    let t = h.now();
    h.push(
        RingId::Rep(0),
        &conn(
            A,
            0,
            ConnStateKind::Down,
            ConnRole::Md,
            InstId(0),
            AccountId(0),
            t,
        ),
    );
    h.run();
    assert!(
        fast(S1).iter().any(|g| g.kind == "Conn"
            && g.text.contains(&format!("venue: {A:?}"))
            && g.text.contains("Down"))
    );
    catch_up(&mut a, &h);
    assert!(
        !model(S1)
            .iter()
            .any(|g| matches!(g.kind, "Risk" | "Mode" | "Conn"))
    );
}

#[test]
fn sdk_048_conn_by_venue_subscription() {
    let b = b_gt3();
    // s1 subscribes a1 (venue A); s2 only b1 (venue B).
    set_subs(S1, vec![(A1, 0, EvMask::BBO, Part::Fast)]);
    set_subs(S2, vec![(B1, 0, EvMask::BBO, Part::Fast)]);
    let mut h = b.boot();
    h.steady();
    clear_log();
    let t = h.now();
    h.push(
        RingId::Rep(0),
        &conn(
            A,
            0,
            ConnStateKind::Resync,
            ConnRole::Md,
            InstId(0),
            AccountId(0),
            t,
        ),
    );
    h.run();
    let c1: Vec<Got> = fast(S1).into_iter().filter(|g| g.kind == "Conn").collect();
    assert_eq!(c1.len(), 1, "{c1:?}");
    assert!(c1[0].text.contains("Resync"));
    assert!(!fast(S2).iter().any(|g| g.kind == "Conn"));
    clear_log();
    h.push(
        RingId::Rep(0),
        &conn(
            A,
            0,
            ConnStateKind::Alive,
            ConnRole::Md,
            InstId(0),
            AccountId(0),
            t,
        ),
    );
    h.run();
    assert!(!log().iter().any(|g| g.kind == "Conn"));
}

#[test]
fn sdk_049_notice_both_parts() {
    let b = b_gt3();
    set_subs(
        S1,
        vec![(A1, 0, EvMask::BBO, Part::Fast), (A1, 2, EvMask::BBO, Part::Fast)],
    );
    let mut h = b.boot();
    h.steady();
    let mut a = adv(&h, MemCkpt::default());
    catch_up(&mut a, &h);
    clear_log();
    let at = UTC0 + 3_600_000_000_000;
    h.cmd_confirmed(
        AC1,
        &ControlCmd::VenueNotice {
            venue: A,
            kind: 1,
            _pad: 0,
            inst: A1,
            a: at,
            b: 0,
        },
    );
    h.run();
    let f: Vec<Got> = fast(S1).into_iter().filter(|g| g.kind == "Notice").collect();
    assert_eq!(
        f.iter().map(|g| (g.inst, g.gix)).collect::<Vec<_>>(),
        vec![(A1, 0), (A1, 2)]
    );
    assert!(
        f[0].text.contains("Delisting") && f[0].text.contains(&format!("{at}")),
        "{}",
        f[0].text
    );
    clear_log();
    catch_up(&mut a, &h);
    let m: Vec<Got> = model(S1).into_iter().filter(|g| g.kind == "Notice").collect();
    assert_eq!(
        m.iter().map(|g| (g.inst, g.gix)).collect::<Vec<_>>(),
        vec![(A1, 0), (A1, 2)]
    );
}

/// Registers a 5 s timer (`tag`) in group `g` on the next a1 Bbo; returns the TSC of that Bbo.
fn timer_in(h: &mut H, g: GroupId, tag: u32) -> u64 {
    set_hook(move |hx, ev, ctx| {
        if let EventRef::Bbo { inst, .. } = ev
            && *inst == A1
            && hx.s.strategy == S1
        {
            let now = hx.v.now();
            let _ = ctx.timer_at(
                now.tsc_of(now.engine_ns + hft_strategy_sdk::time::ms(5_000)),
                tag,
                g,
            );
        }
    });
    let t0 = h.now();
    h.quote(A1, "50000", "50002");
    h.run();
    clear_hook();
    t0
}

/// s1 drops GT2 (`thr` bumped).
fn drop_gt2(h: &mut H) -> [u8; 16] {
    let mut p = Spb::new(S1, AC1, TAKER, 2, vec![gspec(GT1), gspec_fill(GT3)]);
    p.values[F_THR] = 6;
    let id = h.set_params(&p);
    h.run();
    assert_eq!(h.res(id), (status::APPLIED, 0));
    id
}

#[test]
fn sdk_050_timer_by_group_id() {
    let mut h = rig();
    let mut a = adv(&h, MemCkpt::default());
    let t0 = timer_in(&mut h, GT3, 7);
    h.at(t0 + SEC);
    drop_gt2(&mut h);
    clear_log();
    h.at(t0 + 5 * SEC);
    h.run();
    h.at(t0 + 5 * SEC + MS);
    h.run();
    let t: Vec<Got> = fast(S1)
        .into_iter()
        .filter(|g| g.kind == "Timer" && g.text.contains("tag: 7,"))
        .collect();
    assert_eq!(t.len(), 1, "{t:?}");
    assert_eq!(t[0].gix, 1);
    assert!(t[0].text.contains(&format!("group: {GT3:?}")));
    clear_log();
    catch_up(&mut a, &h);
    assert!(
        !model(S1)
            .iter()
            .any(|g| g.kind == "Timer" && g.text.contains("tag: 7,"))
    );
}

#[test]
fn sdk_051_removed_group_timer_cancelled() {
    let mut h = rig();
    let t0 = timer_in(&mut h, GT2, 8);
    h.at(t0 + SEC);
    let m = h.mark();
    let id = drop_gt2(&mut h);
    // The Note is written while processing the SetParams Input.
    let recs = h.since(m);
    let cmd_ix = recs
        .iter()
        .position(|r| {
            r.kind == EventKind::CMD_RESULT
                && r.read::<hft_event::CmdResult>().is_some_and(|c| c.cmd_id == id)
        })
        .expect("result");
    let input_ix = recs[..cmd_ix]
        .iter()
        .rposition(|r| r.kind == EventKind::CONTROL_CMD)
        .expect("command");
    let notes: Vec<Note> = recs[input_ix..cmd_ix]
        .iter()
        .filter(|r| r.kind == EventKind::NOTE)
        .filter_map(|r| r.read::<Note>())
        .filter(|n| n.source == NoteSource::Sequencer && n.group == GT2)
        .collect();
    assert_eq!(notes.len(), 1, "{notes:?}");
    assert_eq!(notes[0].code, seq_note(note::TIMERS_CANCELLED));
    clear_log();
    for k in 2..=10 {
        h.at(t0 + k * SEC);
        h.run();
    }
    assert!(
        !fast(S1)
            .iter()
            .any(|g| g.kind == "Timer" && g.text.contains("tag: 8,"))
    );
}

#[test]
fn sdk_052_advice_to_producer_fast_only() {
    let b = b_gt3();
    SCRIPT.with(|x| x.borrow_mut().model_tick.push((S1, 1_000)));
    let mut h = b.boot();
    h.steady();
    let mut a = adv_with(
        &h,
        MemCkpt::default(),
        AdvisorCfg {
            shard: None,
            ..adv_cfg()
        },
    );
    catch_up(&mut a, &h);
    set_mhook(|hx, ev, out| {
        if let EventRef::Timer { tag, .. } = ev
            && *tag == hft_strategy_sdk::MODEL_TICK
            && hx.s.strategy == S1
        {
            let _ = out.emit_raw(GT2, 1, &[1, 0, 0, 0, 0, 0, 0, 0]);
            let _ = out.emit_raw(GROUP_NONE, 2, &[1]);
        }
    });
    let t = h.now();
    let m = h.mark();
    for k in 1..=2u64 {
        h.at(t + k * SEC);
        h.run();
    }
    let n0 = a.ring.recs.len();
    catch_up(&mut a, &h);
    SCRIPT.with(|x| x.borrow_mut().mhook = None);
    let tick_seq = model(S1)
        .iter()
        .find(|g| g.kind == "Timer")
        .map(|g| g.now.seq)
        .expect("model tick");
    let tick_tsc = h
        .since(m)
        .iter()
        .find(|r| r.seq == tick_seq)
        .map(|r| r.tsc)
        .expect("record");
    clear_log();
    feed_advice(&a, &mut h, n0);
    let adv1: Vec<Got> = fast(S1).into_iter().filter(|g| g.kind == "Advice").collect();
    assert_eq!(adv1.len(), 2, "{adv1:?}");
    assert_eq!(adv1[0].gix, 1);
    assert!(
        adv1[0].text.contains("local_kind: 1") && adv1[0].text.contains("payload: [1, 0, 0, 0, 0, 0, 0, 0]")
    );
    assert_eq!(adv1[1].gix, GIX_NONE);
    assert!(adv1[1].text.contains("local_kind: 2") && adv1[1].text.contains("payload: [1]"));
    for g in &adv1 {
        assert!(g.text.contains(&format!("basis_seq: {tick_seq}")), "{}", g.text);
        assert!(
            g.text.contains(&format!("basis_tsc: Tsc({tick_tsc})")),
            "{}",
            g.text
        );
    }
    assert!(!log().iter().any(|g| g.kind == "Advice" && (g.s != S1 || g.model)));
}

#[test]
fn sdk_053_manual_by_group() {
    let mut h = rig();
    let got = cap::<(u16, bool, u8, Option<i64>, u64)>();
    let g2 = got.clone();
    add_hook(move |hx, ev, _ctx| {
        if let EventRef::Manual {
            gix,
            payload,
            reduce_only,
        } = ev
            && hx.s.strategy == S1
        {
            let v = CmdView::parse(payload).expect("payload");
            g2.borrow_mut()
                .push((*gix, *reduce_only, v.op(), v.val(0), hx.v.now().seq));
        }
    });
    let m = h.mark();
    h.cmd_confirmed(AC1, &strategy_cmd(S1, GT2, 0, &op1(5)));
    h.run();
    let seq1 = h
        .kinds_since(m, EventKind::CONTROL_CMD)
        .last()
        .map(|r| r.seq)
        .expect("cmd");
    let m = h.mark();
    h.cmd_confirmed(AC1, &strategy_cmd(S1, GROUP_NONE, 0, &op1(5)));
    h.run();
    let seq2 = h
        .kinds_since(m, EventKind::CONTROL_CMD)
        .last()
        .map(|r| r.seq)
        .expect("cmd");
    let g = got.borrow().clone();
    assert_eq!(
        g,
        vec![(1, false, 1, Some(5), seq1), (GIX_NONE, false, 1, Some(5), seq2)]
    );
}

#[test]
fn sdk_055_warmup_routing() {
    for both in [false, true] {
        let b = b_gt3();
        if both {
            set_warmup_both(S1);
        }
        let mut h = b.boot();
        h.steady();
        let mut a = adv(&h, MemCkpt::default());
        catch_up(&mut a, &h);
        clear_log();
        let payload: Vec<u8> = (0..48u8).collect();
        let m = h.mark();
        let id = h.cmd(
            AccountId(0),
            &ControlCmd::InjectWarmup {
                instance: S1,
                last: 1,
                local_kind: 1,
                chunk_no: 0,
                len: 48,
                _pad: [0; 6],
                payload: &payload,
            },
        );
        h.run();
        assert_eq!(h.res(id).0, status::APPLIED);
        let cmd_seq = h
            .kinds_since(m, EventKind::CONTROL_CMD)
            .last()
            .map(|r| r.seq)
            .expect("cmd");
        let f: Vec<Got> = fast(S1).into_iter().filter(|g| g.kind == "Warmup").collect();
        if both {
            assert_eq!(f.len(), 1);
            assert_eq!(f[0].now.seq, cmd_seq);
        } else {
            assert!(f.is_empty());
        }
        clear_log();
        catch_up(&mut a, &h);
        let w: Vec<Got> = model(S1).into_iter().filter(|g| g.kind == "Warmup").collect();
        assert_eq!(w.len(), 1, "both = {both}");
        assert!(
            w[0].text.contains("local_kind: 1")
                && w[0].text.contains("chunk_no: 0")
                && w[0].text.contains("last: true")
        );
        assert!(w[0].text.contains(&format!("{:?}", payload.as_slice())));
    }
}

#[test]
fn sdk_144_exec_inflight_immediate_only() {
    let mut h = rig();
    // A resting acked GTC 0.05 a1 buy of s1 in GT1.
    let trig = h.upd + 1;
    add_hook(move |hx, ev, ctx| {
        if let EventRef::Bbo { inst, b, .. } = ev
            && *inst == A1
            && hx.s.strategy == S1
            && b.update_id == trig
        {
            let it = order_in(
                hx,
                GT1,
                A1,
                Side::Buy,
                Tif::Gtc,
                IntentClass::Open,
                px("49000"),
                q("0.05"),
            );
            let _ = ctx.place(it);
        }
    });
    let m = h.mark();
    h.quote(A1, "50000", "50001");
    h.run();
    let g = decs(&h.since(m)).into_iter().find(|d| d.inst == A1).expect("GTC");
    h.ack(&g);
    // PairOnFill target on GT1: +0.01 spot, −0.01 perp.
    let m = h.mark();
    set_hook(|hx, ev, ctx| {
        if let EventRef::Bbo { inst, .. } = ev
            && *inst == A1
            && hx.s.strategy == S1
        {
            let t = target(
                AC1,
                GT1,
                &[(0, q("0.01")), (1, q("-0.01"))],
                ExecSpec::PairOnFill(pair0()),
            );
            let _ = ctx.target(t);
        }
    });
    h.quote(A1, "50000", "50002");
    h.run();
    clear_hook();
    let first = decs(&h.since(m))
        .into_iter()
        .find(|d| d.slot.0 != 0 && d.slot != g.slot)
        .expect("first leg");
    h.ack(&first);
    clear_log();
    let m2 = h.mark();
    h.fill(&first, q("0.01"), q("0.01"), first.px);
    let second = decs(&h.since(m2))
        .into_iter()
        .find(|d| d.inst != first.inst)
        .expect("second leg");
    assert_eq!(second.qty, q("0.01"));
    let p = fast(S1)
        .into_iter()
        .find(|g| g.kind == "Exec" && g.text.contains("Progress"))
        .expect("Exec Progress");
    let (fi, si) = if first.inst == A1 { (0, 1) } else { (1, 0) };
    // Signed like `LegTrack.inflight_immediate`: the second leg sells.
    let mut inflight = [0i64; 4];
    inflight[si] = if second.side == Side::Sell {
        -q("0.01").0
    } else {
        q("0.01").0
    };
    let _ = fi;
    let want = format!(
        "inflight: [Qty({}), Qty({}), Qty({}), Qty({})]",
        inflight[0], inflight[1], inflight[2], inflight[3]
    );
    assert!(p.text.contains(&want), "{}", p.text);
    assert!(p.text.contains("origin: Strategy"), "{}", p.text);
}
