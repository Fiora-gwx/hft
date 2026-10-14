use crate::support::{
    gwh::*,
    rd::*,
    stub::{self, StubProto},
};
use hft_event::{ExternalCause, OrderEventKind, RepMsg};
use hft_net::ConnId;
use hft_types::{ClientOrderId, Qty, SlotId};

const P: ConnId = ConnId(112);
const T: u64 = 3_000_000;
pub(crate) fn measure_unmatched_fill_hold_and_timeout() {
    let rd = rd_with(&RdOpts {
        conn_age_s: 0,
        ..Default::default()
    });
    let mut h = gw_rd::<StubProto, 2048>(g0(), &[A1], Default::default(), rd).unwrap();
    h.open(1000, 2);
    h.reps();
    stub::log_enabled(false);
    for id in [P, ConnId(113)] {
        h.rt.conn(id).unwrap().script().unwrap().sent.reserve(1 << 20);
    }
    for n in 0..101_000u64 {
        let t = T + n * 6_000_000_000;
        h.net.push(P, format!("O||EX-{n}|Fill|1|1|100|0|0|0").as_bytes());
        h.net.flush(&mut h.rt, t);
        let scope = hft_testkit::CountingAlloc::scope();
        h.rt.poll_once(t);
        let held = scope.allocs();
        assert_eq!(h.rt.held(A1), 1);
        h.reps();
        let scope = hft_testkit::CountingAlloc::scope();
        h.rt.poll_once(t + 5_000_000_001);
        let released = scope.allocs();
        assert_eq!(external(&h.reps()).len(), 1);
        if n >= 1000 {
            assert_eq!((held, released), (0, 0));
        }
        for id in [P, ConnId(113)] {
            h.rt.conn(id).unwrap().script().unwrap().sent.clear();
        }
    }
    stub::log_enabled(true);
}
fn cid(slot: u32) -> String {
    stub::cid_text(&ClientOrderId {
        epoch: 5,
        venue: V7,
        account: A1,
        slot,
        r#gen: 1,
    })
}
fn external(v: &[RepMsg]) -> Vec<hft_event::ExternalFill> {
    v.iter()
        .filter_map(|m| {
            if let RepMsg::ExternalFill(e) = m {
                Some(*e)
            } else {
                None
            }
        })
        .collect()
}
#[test]
fn ven_216_hold_released_by_ack() {
    let mut h = gw::<StubProto>();
    let c = h.place(A1, 90, 1, S1, false);
    h.cmd(c);
    h.poll(T);
    h.reps();
    h.net.push(P, b"O||EX-5|PartialFill|1|1|100|0|0|0");
    h.net.push(P, b"O||EX-5|PartialFill|2|3|100|0|0|0");
    h.poll(T + 1);
    assert!(events(&h.reps()).is_empty());
    assert_eq!(h.rt.held(A1), 2);
    h.net
        .push(P, format!("O|{}|EX-5|Ack|0|0|0|0|0|0", cid(90)).as_bytes());
    h.poll(T + 3_000_000_000);
    let e = events(&h.reps());
    assert_eq!(
        e.iter()
            .map(|e| (e.what, e.slot, e.cum_filled))
            .collect::<Vec<_>>(),
        [
            (OrderEventKind::Ack, SlotId(90), Qty(0)),
            (OrderEventKind::PartialFill, SlotId(90), Qty(1)),
            (OrderEventKind::PartialFill, SlotId(90), Qty(3))
        ]
    );
    assert_eq!(h.rt.held(A1), 0);
}
#[test]
fn ven_217_hold_timeout() {
    let mut h = gw::<StubProto>();
    h.net.push(P, b"O||EX-6|Fill|2000|2000|10150|10000|2|0|777");
    h.poll(T);
    assert!(external(&h.reps()).is_empty());
    h.poll(T + 5_000_000_000);
    assert!(external(&h.reps()).is_empty());
    h.poll(T + 5_000_000_001);
    let e = external(&h.reps());
    assert_eq!(e.len(), 1);
    assert_eq!(
        (
            e[0].cause,
            e[0].account,
            e[0].inst,
            e[0].qty,
            e[0].px.0,
            e[0].fee,
            e[0].fee_asset.0,
            e[0].ex_ts_ns
        ),
        (
            ExternalCause::Unknown,
            A1,
            S1,
            Qty(2000),
            10150,
            Qty(10000),
            2,
            777
        )
    );
    assert_eq!(e[0].trade_id, 0);
    assert_eq!(e[0].fill_seq, 2000);
    h.poll(T + 6_000_000_000);
    assert!(external(&h.reps()).is_empty());
    h.net.push(P, b"O||EX-trade|Fill|1|3|10150|2|2|55");
    h.poll(T + 6_000_000_001);
    h.reps();
    h.poll(T + 11_000_000_002);
    let e = external(&h.reps());
    assert_eq!((e[0].trade_id, e[0].fill_seq), (55, 3));
}
#[test]
fn ven_218_fee_only_timeout() {
    let mut h = gw::<StubProto>();
    h.net.push(P, b"O||EX-7|FeeOnly|0|0|0|2000000|3|0");
    h.poll(T);
    assert!(events(&h.reps()).is_empty());
    h.poll(T + 5_000_000_001);
    let r = h.reps();
    assert!(external(&r).is_empty());
    let e = events(&r);
    assert_eq!(e.len(), 1);
    assert_eq!(
        (e[0].what, e[0].slot, e[0].fee, e[0].fee_asset.0),
        (OrderEventKind::FeeOnly, SlotId::UNRESOLVED, Qty(2000000), 3)
    );
}
#[test]
fn ven_219_hold_full() {
    let mut h = gw::<StubProto>();
    for n in 0..256 {
        h.net.push(P, format!("O||EX-{n}|Fill|1|1|100|0|0|0").as_bytes());
        h.poll(T);
        assert!(external(&h.reps()).is_empty());
    }
    assert_eq!(h.rt.held(A1), 256);
    assert_eq!(h.rt.counters().hold_evicted, 0);
    let a2 = h.rt.primary(A2).unwrap();
    h.net.push(a2, b"O||EX-other|Fill|1|1|100|0|0|0");
    h.poll(T);
    assert!(external(&h.reps()).is_empty());
    assert_eq!(h.rt.held(A2), 1);
    h.net.push(P, b"O||EX-256|Fill|1|1|100|0|0|0");
    h.poll(T);
    let r = external(&h.reps());
    assert_eq!(r.len(), 1);
    assert_eq!(r[0].exch_order_id, hft_venue::exch_id_value(b"EX-0"));
    assert_eq!(h.rt.held(A1), 256);
    assert_eq!(h.rt.counters().hold_evicted, 1);
}
#[test]
fn ven_220_immediate_paths() {
    let mut h = gw::<StubProto>();
    for frame in [
        b"O|web-1|EX-8|Ack|0|0|0|0|0|0".as_slice(),
        b"O||EX-8|CancelAck|0|0|0|0|0|0",
    ] {
        h.net.push(P, frame);
        h.poll(T);
        let e = events(&h.reps());
        assert_eq!(e.len(), 1);
        assert_eq!(e[0].slot, SlotId::UNRESOLVED);
        assert_eq!(h.rt.held(A1), 0);
    }
    h.net.push(P, b"X");
    h.poll(T);
    let e = external(&h.reps());
    assert_eq!(e.len(), 1);
    assert_eq!(
        (e[0].cause, e[0].trade_id, e[0].qty),
        (ExternalCause::Liquidation, 9, Qty(2))
    );
    assert_eq!(h.rt.held(A1), 0);
}
