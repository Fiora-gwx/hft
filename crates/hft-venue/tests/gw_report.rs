//! VEN-213, VEN-223, VEN-224, VEN-229: reports.
#![allow(
    clippy::disallowed_types,
    clippy::disallowed_macros,
    clippy::disallowed_methods
)]

use crate::support;

use hft_event::{ConnRole, ConnStateKind, OrderEventKind, RepMsg, SendStatus};
use hft_net::ConnId;
use hft_types::{ClientOrderId, Qty, SlotId};
use hft_venue::{codes, exch_id_value};
use support::gwh::*;
use support::rd::*;
use support::stub::{self, StubProto};

const P: ConnId = ConnId(112);

#[test]
fn ven_213_cid_resolves() {
    let mut h = gw::<StubProto>();
    let id = ClientOrderId {
        epoch: 5,
        venue: V7,
        account: A1,
        slot: 70,
        r#gen: 2,
    };
    h.net.push(
        P,
        format!("O|{}|EX-1|Ack|0|0|0|0|0|0", stub::cid_text(&id)).as_bytes(),
    );
    h.poll(3_000_000);
    let e = events(&h.reps());
    assert_eq!(e.len(), 1);
    assert_eq!(
        (e[0].what, e[0].epoch, e[0].slot, e[0].r#gen, e[0].exch_order_id),
        (OrderEventKind::Ack, 5, SlotId(70), 2, exch_id_value(b"EX-1"))
    );
}

#[test]
fn ven_223_outcome_unknown() {
    for (row, code, unknown, raw) in [
        ("VEN-223.1", -1007i64, true, 0xFFFF_FC11u32),
        ("VEN-223.2", -9999, true, 0xFFFF_D8F1),
        ("VEN-223.3", -2010, false, 0xFFFF_F826),
    ] {
        let mut h = gw::<StubProto>();
        let c = h.place(A1, 95, 1, S1, false);
        h.cmd(c);
        h.poll(3_000_000);
        h.reps();
        assert_eq!(h.rt.inflight(P), 1);
        h.net.push(P, format!("R|95-1-1-5|{code}").as_bytes());
        h.poll(3_000_001);
        let e = events(&h.reps());
        assert_eq!(e.len(), 1, "{row}");
        if unknown {
            assert_eq!(
                (e[0].what, e[0].slot, e[0].r#gen, e[0].reject_code),
                (OrderEventKind::OutcomeUnknown, SlotId(95), 1, raw),
                "{row}"
            );
            assert_eq!(e[0].what as u8, 11);
            assert_eq!(
                (e[0].px.0, e[0].qty, e[0].cum_filled),
                (0, Qty(0), Qty(0)),
                "{row}"
            );
        } else {
            assert_eq!(
                (e[0].what, e[0].reject_code),
                (OrderEventKind::Reject, raw),
                "{row}"
            );
        }
        assert_eq!(h.rt.inflight(P), 0, "VEN-223.4 ({row})");
    }
}

#[test]
fn ven_224_rate_obs_correction() {
    let mut h = gw::<StubProto>();
    for s in [1, 2] {
        let c = h.place(A1, s, 1, S1, false);
        h.cmd(c);
    }
    h.poll(3_000_000);
    h.reps();
    h.net.push(P, b"R|1-1-1-5|0|9|10|1000");
    h.poll(3_000_001);
    for s in [3, 4] {
        let c = h.place(A1, s, 1, S1, false);
        h.cmd(c);
    }
    h.poll(3_000_002);
    assert_eq!(
        statuses(&h.reps()),
        vec![SendStatus::Written, SendStatus::RateLimited]
    );
    // Step 3: an observation of another window (daily 2147 / 200000) is scaled to the local
    // 10 / 1000 ms bucket by its fill ratio (→ 1), so both orders pass.
    let mut h = gw::<StubProto>();
    let c = h.place(A1, 1, 1, S1, false);
    h.cmd(c);
    h.poll(3_000_000);
    h.reps();
    h.net.push(P, b"R|1-1-1-5|0|2147|200000|86400000");
    h.poll(3_000_001);
    for s in [3, 4] {
        let c = h.place(A1, s, 1, S1, false);
        h.cmd(c);
    }
    h.poll(3_000_002);
    assert_eq!(
        statuses(&h.reps()),
        vec![SendStatus::Written, SendStatus::Written],
        "VEN-224 step 3"
    );
}

#[test]
fn ven_229_inflight_timeout() {
    let mut h = gw::<StubProto>();
    let t0 = 3_000_000u64;
    let c = h.place(A1, 97, 1, S1, false);
    h.cmd(c);
    h.poll(t0);
    h.reps();
    let degraded = |v: &[RepMsg]| {
        conn_states(v)
            .into_iter()
            .filter(|s| s.state == ConnStateKind::Degraded && s.reason == codes::INFLIGHT_TIMEOUT)
            .collect::<Vec<_>>()
    };
    h.poll(t0 + 3_000_000_000);
    assert!(degraded(&h.reps()).is_empty(), "VEN-229.1");
    assert_eq!(h.rt.inflight(P), 1, "VEN-229.1");
    h.poll(t0 + 3_000_000_001);
    let d = degraded(&h.reps());
    assert_eq!(d.len(), 1, "VEN-229.2");
    assert_eq!(
        (d[0].account, d[0].role, d[0].conn, d[0].reason),
        (A1, ConnRole::Order, 112, 10),
        "VEN-229.2"
    );
    assert_eq!(h.rt.inflight(P), 0, "VEN-229.2");
    h.poll(t0 + 10_000_000_000);
    assert!(degraded(&h.reps()).is_empty(), "VEN-229.3");
    h.net.push(P, b"R|97-1-1-5|-2010");
    h.poll(t0 + 11_000_000_000);
    let e = events(&h.reps());
    assert_eq!(e.len(), 1, "VEN-229.4: decoded into the ring");
    assert_eq!(h.rt.inflight(P), 0, "VEN-229.4");
}
