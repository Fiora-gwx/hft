//! hft-oms `integ` cases on the real sequencer + hft-risk + hft-ledger (crates/hft-oms/TESTS.md:
//! OMS-109, OMS-227), on the B0 rig.

use hft_event::OrderEventKind;
use hft_sequencer::driver::live::RingId;
use hft_strategy_sdk::{EventRef, LinkRole, OrderIntent};
use hft_types::SlotId;
use hft_types::{Qty, Side};

use crate::common::*;
use crate::scenarios::*;

/// Foreign order 555 of account 1 on A (a1 sell, `trade_id = 0`) reported twice through the
/// defensive path (`slot 0`): the two `ExternalFill{cause Unknown}` the sequencer builds carry
/// `fill_seq` = the cumulative quantity (400 000 and 1 000 000), so the ledger books both.
#[test]
fn oms_109_foreign_fill_seq_distinct() {
    let mut h = steady();
    // Spot inventory to sell from (the foreign sell reduces it).
    h.open_position(AC1, A1, Side::Buy, "0.02");
    let pos0 = h.s.engine().ledger.pos(AC1, A1).qty;
    let foreign = |h: &H, cum: &str| {
        let mut ev = order_event(
            &hft_event::OrderDecision {
                trigger_seq: 0,
                decided_tsc: hft_types::Tsc(0),
                px: px("50000"),
                qty: q("0.01"),
                group: hft_types::GROUP_NONE,
                inst: A1,
                slot: SlotId(0),
                r#gen: 0,
                target_slot: SlotId(0),
                reject: 0,
                strategy: hft_types::StrategyId(0),
                venue: A,
                account: AC1,
                action: hft_event::OrderAction::New,
                side: Side::Sell,
                tif: hft_types::Tif::Ioc,
                oflags: 0,
                reason: 0,
                intent_no: 0,
                req_px: px("50000"),
                link_role: 0,
                _pad: [0; 7],
            },
            OrderEventKind::PartialFill,
            h.epoch(),
        );
        ev.detail = 2; // Sell
        ev.exch_order_id = 555;
        ev.cum_filled = q(cum);
        ev.qty = q(cum);
        ev.trade_id = 0;
        ev
    };
    let note_code = hft_oms::notes::code_of(hft_oms::notes::FOREIGN_FILL);
    let mut seqs = Vec::new();
    for (cum, want_pos) in [("0.004", "0.016"), ("0.01", "0.01")] {
        h.trace_on();
        let m = h.mark();
        let ev = foreign(&h, cum);
        h.push(RingId::Rep(0), &ev);
        h.run();
        // apply_external booked it (Some → the LegTracker adjustment ran).
        assert_eq!(h.s.engine().trace.count("ledger.apply_external"), 1, "cum {cum}");
        assert_eq!(
            h.s.engine().ledger.pos(AC1, A1).qty,
            Qty(pos0.0 - q("0.02").0 + q(want_pos).0),
            "cum {cum}"
        );
        let n: Vec<_> = h
            .notes_since(m)
            .into_iter()
            .filter(|n| n.code == note_code)
            .collect();
        assert_eq!(n.len(), 1, "one OmsForeignFill");
        // v[7] = cum_filled = the ExternalFill's fill_seq (README "ExternalFill" 去重键).
        seqs.push(n[0].v[7]);
    }
    assert_eq!(seqs, vec![400_000, 1_000_000]);
    assert_eq!(h.s.engine().ledger.pos(AC1, A1).qty, Qty(pos0.0 - q("0.01").0));
}

/// The second leg's `HedgePxLimits` come from the hedge order's account and inst — the same
/// `RiskParams` row risk #5 uses: an a1 buy second leg in a 0.005 band, b0 quote 59 710 / 60 290
/// (mid 60 000): `price_hedge` gets `{2 000, 0.005}` and caps at 60 300.00, which #5 lets through.
#[test]
fn oms_227_limits_same_source() {
    reset_script();
    let mut b = b0();
    let rd = b.rd.load();
    let mut rp = b0_risk(&rd);
    rp.classes.push(band("0.03", "0.0025", "0.005"));
    rp.risk_class = hft_risk::PreVec::from_vec(vec![hft_risk::InstClassRow { inst: A1, class: 1 }]);
    b.risk = Some(rp);
    let mut h = b.boot();
    h.steady();
    let lim = h.s.engine().risk.hedge_px_limits(AC1, A, A1);
    assert_eq!((lim.conn_liveness_ms, lim.rr_max_cross), (2_000, r("0.005")));
    // First leg b1 sell 0.01 with an OnFill a1 buy.
    fn first(hx: &Hx<'_>) -> OrderIntent {
        let mut it = b1_sell_open(hx);
        it.qty = q("0.01");
        it.link = hft_strategy_sdk::LegLinkOpt::some(hft_strategy_sdk::LegLink {
            group: G1,
            role: LinkRole::First,
            _pad: [0; 3],
            tolerance: r("0.02"),
            hedge: onfill(hx.s.account, A1, Side::Buy),
        });
        it
    }
    set_hook(|hx, ev, ctx| {
        if let EventRef::Bbo { inst, .. } = ev
            && *inst == B1
            && hx.s.strategy == S1
        {
            let _ = ctx.place(first(hx));
        }
    });
    let m = h.mark();
    h.quote(B1, "50010", "50011");
    h.run();
    clear_hook();
    let f = decs(&h, m).into_iter().find(|d| d.inst == B1).expect("first leg");
    assert_eq!(f.reject, 0, "{f:?}");
    h.ack(&f);
    h.quote(A1, "59710", "60290");
    h.run();
    let m = h.mark();
    h.fill(&f, f.qty, f.qty, f.px);
    let second = decs(&h, m)
        .into_iter()
        .find(|d| d.inst == A1)
        .expect("second leg");
    assert_eq!(second.reject, 0, "risk #5 lets the cap through: {second:?}");
    assert_eq!((second.side, second.px), (Side::Buy, px("60300")));
    // The same price from `price_hedge` with the account's limits.
    let e = h.s.engine();
    let l = last_link(&h, AC1).expect("link");
    let (_, plan) = *e.oms.link_plan(l.link_id).expect("plan");
    let req = hft_oms::HedgeRequest {
        link_id: l.link_id,
        role: LinkRole::Second,
        venue: A,
        account: AC1,
        side: Side::Buy,
        reduce_only: false,
        inst: A1,
        group: G1,
        strategy: S1,
        qty: second.qty,
        basis: second.qty,
        hedge_cap_qty: second.qty,
        ref_px: l.ref_px,
        peer_venue: hft_types::VenueId(0),
        peer_account: hft_types::AccountId(0),
        trigger_seq: 0,
    };
    let bbo = e.market.bbo_view(&e.books, A1);
    let hp = hft_oms::price_hedge(
        &req,
        &plan,
        &bbo,
        e.now.tsc,
        &lim,
        h.rd.inst(A1).tick,
        e.now.tsc_per_ms,
    )
    .expect("priced");
    assert_eq!((hp.px, hp.capped), (px("60300"), true));
}
