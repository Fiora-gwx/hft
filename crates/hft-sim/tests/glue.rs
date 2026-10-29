//! Journal glue (README "journal 衔接") and `reseed_account`.
use crate::common::*;
use hft_event::{
    Bbo, ConnRole, ConnState, ConnStateKind, EventKind, GwCmd, GwOp, L2SnapHdr, OrderEventKind, RepMsg,
    SendStatus,
};
use hft_sim::glue::{Fed, feed_record, reply_parts, written};
use hft_sim::{Dist, LatencyModel};
use hft_types::{AccountId, InstId, Px, Qty, Side, SlotId, Tif, Tsc, VenueId};
use zerocopy::{FromZeros, IntoBytes};

fn hdr(kind: EventKind, t: u64) -> hft_event::RecordHeader {
    hft_event::RecordHeader {
        len: 0,
        kind: kind.0,
        flags: 0,
        seq: 1,
        tsc: t,
    }
}

fn cmd(op: GwOp, slot: u32, target: u32) -> GwCmd {
    GwCmd {
        decision_seq: 7,
        decided_tsc: Tsc(5),
        px: Px(0),
        qty: Qty(0),
        inst: BTC_S1,
        slot: SlotId(slot),
        r#gen: 3,
        target_slot: SlotId(target),
        op,
        account: A9,
        side: Side::Buy,
        tif: Tif::Gtc,
        oflags: 0,
        _pad: [0; 2],
        arg: 0,
        arg2: 0,
    }
}

#[test]
fn sim_230_journal_glue() {
    let mut s = Sim::std();
    let mut levels = Vec::new();
    // .1 a Bbo body, also one written before its last field existed, is market data: a taker buy
    // at the ask fills at the replica's ask.
    let b = Bbo {
        h: mhdr(S1, BTC_S1, 0),
        bid_px: px("59000"),
        bid_qty: q("1"),
        ask_px: px("60000"),
        ask_qty: q("1"),
        update_id: 0,
    };
    let full = b.as_bytes();
    assert_eq!(
        feed_record(
            &mut s.ex,
            &hdr(EventKind::BBO, ms(1)),
            EventKind::BBO,
            full,
            &mut levels
        ),
        Fed::Market,
        "SIM-230.1"
    );
    assert_eq!(
        feed_record(
            &mut s.ex,
            &hdr(EventKind::BBO, ms(2)),
            EventKind::BBO,
            &full[..full.len() - 8],
            &mut levels
        ),
        Fed::Market,
        "SIM-230.1"
    );
    s.place(A9, BTC_S1, Side::Buy, "0.1", "60000", Tif::Ioc, ms(3));
    s.run_to(ms(20));
    let fill = s.out.iter().find_map(|r| match &r.msg {
        RepMsg::OrderEvent(e, _) if e.what == OrderEventKind::Fill => Some(*e),
        _ => None,
    });
    assert_eq!(fill.map(|e| e.px), Some(px("60000")), "SIM-230.1");
    // .2 an engine keyframe snapshot is skipped; any other record only advances time.
    let mut kf = L2SnapHdr::new_zeroed();
    kf.origin = L2SnapHdr::KEYFRAME;
    assert_eq!(
        feed_record(
            &mut s.ex,
            &hdr(EventKind::L2_SNAPSHOT, ms(21)),
            EventKind::L2_SNAPSHOT,
            kf.as_bytes(),
            &mut levels
        ),
        Fed::Skipped,
        "SIM-230.2"
    );
    assert_eq!(
        feed_record(
            &mut s.ex,
            &hdr(EventKind::NOTE, ms(22)),
            EventKind::NOTE,
            &[0; 8],
            &mut levels
        ),
        Fed::Clock,
        "SIM-230.2"
    );
    // .3 Written of an order command at `now`; target fields of a cancel; none for other ops.
    let w = written(&cmd(GwOp::Place, 4, 0), S1, Tsc(9)).unwrap();
    assert_eq!(
        (
            w.status,
            w.slot,
            w.r#gen,
            w.target_slot,
            w.sent_tsc,
            w.decision_seq,
            w.venue
        ),
        (SendStatus::Written, SlotId(4), 3, SlotId(0), Tsc(9), 7, S1),
        "SIM-230.3"
    );
    let w = written(&cmd(GwOp::Cancel, 4, 4), S1, Tsc(9)).unwrap();
    assert_eq!(
        (w.slot, w.target_slot, w.target_gen),
        (SlotId(4), SlotId(4), 3),
        "SIM-230.3"
    );
    assert!(
        written(&cmd(GwOp::SimReset, 0, 0), S1, Tsc(9)).is_none(),
        "SIM-230.3"
    );
    // .4 an account-wide ConnState becomes one record per venue of the account; an OrderEvent
    // gets the engine epoch.
    let rd = rd_sim();
    let c = ConnState {
        venue: VenueId(0),
        conn: 0,
        state: ConnStateKind::Up,
        role: ConnRole::Order,
        inst: InstId(0),
        reason: 0,
        account: A9,
        _pad2: [0; 3],
        rx_tsc: 0,
    };
    let mut venues = Vec::new();
    reply_parts(&rd, 5, &RepMsg::ConnState(c), &[], |k, p| {
        assert_eq!(k, EventKind::CONN_STATE);
        let (c, _) = <ConnState as zerocopy::TryFromBytes>::try_read_from_prefix(p[0]).unwrap();
        venues.push(c.venue);
    });
    assert_eq!(venues, vec![S1, P2], "SIM-230.4");
    let e = fill.unwrap();
    let mut epoch = None;
    reply_parts(
        &rd,
        5,
        &RepMsg::OrderEvent(e, hft_event::RecordFlags(0)),
        &[],
        |k, p| {
            assert_eq!(k, EventKind::ORDER_EVENT);
            let (o, _) =
                <hft_event::OrderEvent as zerocopy::TryFromBytes>::try_read_from_prefix(p[0]).unwrap();
            epoch = Some(o.epoch);
        },
    );
    assert_eq!(epoch, Some(5), "SIM-230.4");
}

/// Ack due of one non-crossing GTC buy decided at `t`.
fn ack_due(s: &mut Sim, t: u64) -> u64 {
    s.out.clear();
    s.place(A9, BTC_S1, Side::Buy, "0.1", "50000", Tif::Gtc, t);
    s.run_to(t + ms(1_000));
    s.out
        .iter()
        .find_map(|r| match &r.msg {
            RepMsg::OrderEvent(e, _) if e.what == OrderEventKind::Ack => Some(r.due - t),
            _ => None,
        })
        .unwrap()
}

#[test]
fn sim_231_reseed_restarts_the_account_stream() {
    let d = Dist::from_points(&[(0, 1_000), (10_000, 90_000)]);
    let mut setup = setup_sim();
    setup.latency_models = vec![LatencyModel::Empirical { entry: d, resp: d }];
    let mut a = Sim::with(&setup, &rd_sim());
    let mut b = Sim::with(&setup, &rd_sim());
    for s in [&mut a, &mut b] {
        s.bbo(ms(1), BTC_S1, ("59000", "1"), ("60000", "1"));
    }
    // Different numbers of earlier draws.
    for k in 0..3 {
        ack_due(&mut a, ms(10 + 1_000 * k));
    }
    let first_b = ack_due(&mut b, ms(10));
    // .1 without a reseed the next draws differ (a has drawn more).
    let (na, nb) = (ack_due(&mut a, ms(5_000)), ack_due(&mut b, ms(5_000)));
    assert!(na != nb || first_b != nb, "SIM-231.1");
    // .2 after the same reseed both draw the same latencies.
    a.ex.reseed_account(A9, 777);
    b.ex.reseed_account(A9, 777);
    for k in 0..4 {
        let t = ms(10_000 + 1_000 * k);
        assert_eq!(ack_due(&mut a, t), ack_due(&mut b, t), "SIM-231.2");
    }
    // .3 an unknown account is ignored.
    a.ex.reseed_account(AccountId(77), 1);
}
