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
fn ven_222_report_decode_window_and_recovery() {
    for (script, down) in [
        (vec![(0, false)], false),
        (vec![(0, false), (10, false), (20, false)], true),
        (vec![(0, false), (5, true), (10, false), (20, false)], false),
        (vec![(0, false), (30, false), (61, false)], false),
        (vec![(0, false), (30, false), (60, false)], true),
        (vec![(0, false), (10, false)], false),
    ] {
        let mut cfg = g0();
        cfg.aux_every = 1;
        cfg.report_decode_down_after = 3;
        let mut h = gw_full::<StubProto>(cfg, &[A1], GhOpts::default());
        let mut states = Vec::new();
        for (sec, valid) in &script {
            h.net
                .push(P, if *valid { b"E|listenKeyExtended" } else { b"Z|bad" });
            h.poll(3_000_000 + sec * 1_000_000_000);
            states.extend(conn_states(&h.reps()));
        }
        assert_eq!(h.rt.conn(P).is_none(), down, "{script:?}");
        assert_eq!(states.iter().any(|s| s.state == ConnStateKind::Down), down);
        assert!(
            states
                .iter()
                .any(|s| s.state == ConnStateKind::Degraded && s.reason == codes::REPORT_DECODE)
        );
        assert_eq!(
            h.rt.counters().report_decode_err,
            script.iter().filter(|(_, ok)| !ok).count() as u64
        );
    }
}

#[test]
fn ven_214_epoch_window() {
    for (epoch, accept, account, want) in [
        (70, true, A1, true),
        (5, true, A1, true),
        (69, true, A1, false),
        (5, false, A1, false),
        (70, true, A2, false),
    ] {
        let mut h = gw::<StubProto>();
        h.cmd(hft_event::GwCmd::set_epoch(70, accept, 5));
        h.poll(3_000_000);
        h.reps();
        let id = ClientOrderId {
            epoch,
            venue: V7,
            account,
            slot: 80,
            r#gen: 1,
        };
        h.net.push(
            P,
            format!("O|{}|EX-9|Ack|0|0|0|0|0|0", stub::cid_text(&id)).as_bytes(),
        );
        h.poll(3_000_001);
        let e = events(&h.reps());
        assert_eq!(e.len(), 1);
        assert_eq!(e[0].slot, if want { SlotId(80) } else { SlotId::UNRESOLVED });
        if want {
            assert_eq!(e[0].epoch, epoch);
        }
        assert_eq!(h.rt.id_table(A1).unwrap().get(b"EX-9").is_some(), want);
    }
}
#[test]
fn ven_215_idtable_fallback() {
    let mut h = gw::<StubProto>();
    let id = ClientOrderId {
        epoch: 5,
        venue: V7,
        account: A1,
        slot: 80,
        r#gen: 1,
    };
    h.net.push(
        P,
        format!("O|{}|EX-9|Ack|0|0|0|0|0|0", stub::cid_text(&id)).as_bytes(),
    );
    h.poll(3_000_000);
    h.reps();
    for cid in ["", "web-123"] {
        h.net
            .push(P, format!("O|{cid}|EX-9|CancelAck|0|0|0|0|0|0").as_bytes());
        h.poll(3_000_001);
        let e = events(&h.reps());
        assert_eq!((e[0].epoch, e[0].slot, e[0].r#gen), (5, SlotId(80), 1));
    }
    h.net.push(P, b"O||EX-404|Ack|0|0|0|0|0|0");
    h.poll(3_000_002);
    let e = events(&h.reps());
    assert_eq!(
        (e[0].slot, e[0].exch_order_id),
        (SlotId::UNRESOLVED, exch_id_value(b"EX-404"))
    );
}

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

#[test]
fn ven_242_variable_report_tails_preserve_order_under_backpressure() {
    use hft_rt::{Backing, byte_spsc};
    use zerocopy::FromBytes;
    let mut h = gw::<StubProto>();
    let (mut tx, mut rx) = byte_spsc(512, Backing::Heap).unwrap();
    for _ in 0..16 {
        tx.reserve(24).unwrap().fill(7);
        tx.commit();
    }
    h.rt.set_rep_bulk(tx);
    h.net.push(P, b"V|reports");
    h.poll(3_000_000);
    assert!(h.reps().is_empty(), "header cannot outrun its bulk record");
    while rx.peek().is_some() {
        rx.advance();
    }
    h.poll(3_000_001);
    let r = h.reps();
    assert!(matches!(r.first(), Some(RepMsg::MarginReport(m)) if m.n == 3));
    let first = rx.peek().unwrap();
    assert_eq!(first.len(), 48);
    assert_eq!(
        hft_event::MarginItem::read_from_bytes(&first[..16])
            .unwrap()
            .liq_px
            .0,
        77
    );
    rx.advance();
    h.poll(3_000_002);
    let mut all = r;
    all.extend(h.reps());
    assert!(
        all.iter()
            .any(|m| matches!(m, RepMsg::AccountConfigReport(h) if h.n_lev == 2))
    );
    assert_eq!(rx.peek().unwrap().len(), 32);
    assert_eq!(
        hft_event::LeverageItem::read_from_bytes(&rx.peek().unwrap()[..16])
            .unwrap()
            .leverage
            .0,
        5_000_000_000
    );
    assert!(
        all.iter()
            .any(|m| matches!(m, RepMsg::PositionReport(p) if p.qty.0 == 8))
    );
    assert!(all.iter().any(|m| matches!(m, RepMsg::BalanceReport(_))));
    assert!(all.iter().any(|m| matches!(m, RepMsg::FundingSettle(_))));
}
#[test]
fn ven_243_clock_and_rate_failures_degrade_connection() {
    for (code, reason) in [(-1021, codes::CLOCK_SKEW), (-1015, codes::RATE_LIMIT)] {
        let mut h = gw::<StubProto>();
        let c = h.place(A1, 98, 1, S1, false);
        h.cmd(c);
        h.poll(3000);
        h.reps();
        h.net.push(P, format!("R|98-1-1-5|{code}").as_bytes());
        h.poll(3001);
        let r = h.reps();
        assert_eq!(events(&r)[0].what, OrderEventKind::Reject);
        assert!(
            conn_states(&r)
                .iter()
                .any(|s| s.state == ConnStateKind::Degraded && s.reason == reason)
        );
    }
}
