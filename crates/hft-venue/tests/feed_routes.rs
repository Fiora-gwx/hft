use crate::support::{feedh::*, rd::*, stub};
use hft_event::FeedMsg;
use hft_net::ConnId;
use hft_types::InstId;
use hft_venue::{FeedCfg, FeedRoutes, streams};

#[test]
fn ven_020_bbo_task_only_bbo() {
    let mut h = feed(FeedCfg {
        routes: FeedRoutes::Bbo,
        ..f0()
    });
    let mut t = 100;
    h.up(
        &[S1],
        streams::BBO | streams::BOOK | streams::TRADE | streams::MARK,
        &mut t,
    );
    assert_eq!(h.rt.conn_ids().collect::<Vec<_>>(), [ConnId(0), ConnId(1)]);
    assert!(h.md.is_none() && h.bulk.is_none());
    h.clear();
    for id in [ConnId(0), ConnId(1)] {
        h.net.push(id, &b("S1", 1));
        h.net.push(id, b"T|S1|1|100|1|B");
        h.net.push(id, b"D|S1|1|1|100 1|101 1");
    }
    h.poll(t + 1);
    assert_eq!(bbos(&h.drain_bbo()).len(), 1);
    assert_eq!(h.rt.counters().unknown_class, 4);
    assert!(h.all_builds.iter().all(|r| r.conn.0 < 8));
}
#[test]
fn ven_021_md_task_only_md() {
    let mut h = feed(FeedCfg {
        routes: FeedRoutes::Md,
        ..f0()
    });
    let mut t = 100;
    h.up(
        &[S1],
        streams::BBO | streams::BOOK | streams::TRADE | streams::MARK,
        &mut t,
    );
    assert_eq!(h.rt.conn_ids().collect::<Vec<_>>(), [ConnId(8), ConnId(9)]);
    assert!(h.bbo.is_none());
    h.clear();
    h.net.push(ConnId(8), b"D|S1|1|1|100 1|101 1");
    h.net.push(ConnId(9), b"T|S1|1|100|1|B");
    h.net.push(ConnId(9), b"M|S1|1000|100");
    h.net.push(ConnId(9), &b("S1", 1));
    h.poll(t + 1);
    let e: Vec<_> = h
        .drain_md()
        .into_iter()
        .filter(|e| !matches!(e, FeedMsg::ConnState(_)))
        .collect();
    assert_eq!(e.len(), 3);
    assert!(matches!(e[0], FeedMsg::Bulk { .. }));
    assert!(matches!(e[1], FeedMsg::Trade(_, _)));
    assert!(matches!(e[2], FeedMsg::MarkPrice(_, _)));
    assert!(h.bulk.as_mut().unwrap().peek().is_some());
    assert_eq!(h.rt.counters().unknown_class, 1);
}

#[test]
fn ven_021_md_routes_keep_trade_and_ticker_separate_after_reconnect() {
    type Split = stub::Stub<2048, { stub::V_MD_SPLIT }>;
    let mut cfg = f0();
    cfg.routes = FeedRoutes::Md;
    cfg.endpoints.md_routes[2] = Some(hft_net::EndpointId(2));
    cfg.endpoints.md_routes[3] = Some(hft_net::EndpointId(3));
    let mut h = feed_with::<Split>(cfg, Caps::default());
    let mut t = 100;
    h.up(
        &[S1],
        streams::TRADE | streams::MARK | streams::INDEX | streams::FUNDING,
        &mut t,
    );
    assert_eq!(h.rt.conn_ids().collect::<Vec<_>>(), [ConnId(8), ConnId(9)]);
    assert_eq!(h.all_builds[0].endpoint, hft_net::EndpointId(2));
    assert_eq!(h.all_builds[1].endpoint, hft_net::EndpointId(3));
    for reconnect in [false, true] {
        if reconnect {
            h.net.at(ConnId(9), 0, crate::support::net::Ev::Eof);
            h.poll(t + 1);
            t += 10_000_000_000;
            h.up(&[], 0, &mut t);
        }
        h.clear();
        let ticker = h.all_builds.last().unwrap();
        assert_eq!(ticker.endpoint, hft_net::EndpointId(3));
        let ticker = ticker.conn;
        h.net.push(ConnId(8), format!("T|S1|{t}|100|1|B").as_bytes());
        h.net.push(ticker, format!("M|S1|{t}|100").as_bytes());
        h.net.push(ticker, format!("F|S1|{t}|0.0001|2000").as_bytes());
        h.poll(t + 1);
        let events = h.drain_md();
        assert!(events.iter().any(|e| matches!(e, FeedMsg::Trade(_, _))));
        assert!(
            events.iter().any(|e| matches!(e, FeedMsg::MarkPrice(_, _))),
            "reconnect={reconnect}, events={events:?}, builds={:?}, counts={:?}",
            h.all_builds,
            h.rt.counters()
        );
        assert!(events.iter().any(|e| matches!(e, FeedMsg::Funding(_, _))));
        assert_eq!(h.rt.counters().unknown_class, 0);
    }
}
#[test]
fn ven_027_unknown_inst() {
    let mut h = feed(f0());
    h.sub(InstId(9999), streams::BBO);
    h.poll(0);
    assert!(h.all_builds.is_empty());
    assert!(h.drain_bbo().is_empty());
    assert!(h.drain_md().is_empty());
    assert_eq!(h.rt.counters().cmd_bad_inst, 1);
}

fn burst(all: bool) -> (Fh, u64) {
    let mut h = feed(FeedCfg {
        routes: if all { FeedRoutes::All } else { FeedRoutes::Md },
        ..f0()
    });
    let mut t = 100;
    h.up(&[S1], streams::BBO | streams::BOOK | streams::TRADE, &mut t);
    h.clear();
    stub::take_log();
    if all {
        for n in 1..=10 {
            for id in [ConnId(0), ConnId(1)] {
                h.net.push(id, &b("S1", n));
            }
        }
    }
    for n in 1..=100 {
        h.net
            .push(ConnId(8), format!("D|S1|{n}|1|100 1|101 1").as_bytes());
        h.net.push(ConnId(9), format!("T|S1|{n}|100|1|B").as_bytes());
    }
    (h, t)
}
fn classes() -> Vec<u8> {
    stub::take_log()
        .into_iter()
        .filter(|c| c.name == "classify")
        .map(|c| c.arg.as_bytes()[0])
        .collect()
}
#[test]
fn ven_054_md_conn_by_conn() {
    let (mut h, t) = burst(false);
    assert_eq!(h.poll(t + 1).frames, 128);
    assert_eq!(classes(), [vec![b'D'; 64], vec![b'T'; 64]].concat());
    h.poll(t + 2);
    assert_eq!(classes(), [vec![b'D'; 36], vec![b'T'; 36]].concat());
}
#[test]
fn ven_055_all_routes_order() {
    let (mut h, t) = burst(true);
    assert_eq!(h.poll(t + 1).frames, 144);
    assert_eq!(
        classes(),
        [vec![b'B'; 16], vec![b'D'; 64], vec![b'T'; 64]].concat()
    );
}

#[test]
fn ven_073_md_full_bbo_continues() {
    // A physical md ring must fit FRAME_EVENTS_MAX; two complete frames fill this valid ring.
    let mut h = feed_with::<stub::StubProto>(
        f0(),
        Caps {
            md: 128,
            ..Caps::default()
        },
    );
    let mut t = 100;
    h.sub(S1, streams::BBO | streams::BOOK | streams::TRADE);
    for _ in 0..1000 {
        h.poll(t);
        let built = h.complete_builds();
        let ids: Vec<_> = h.rt.conn_ids().collect();
        let mut acks = 0;
        for id in ids {
            acks += h.ack(id);
        }
        h.clear();
        if built.is_empty() && acks == 0 && h.builds.is_empty() && h.rt.pending_subs() == 0 {
            break;
        }
        t += 1000;
    }
    assert_eq!(h.rt.pending_subs(), 0);
    assert_eq!(h.rt.conn_ids().count(), 4);
    for n in [1, 65] {
        h.net.push(ConnId(9), format!("T|S1|{n}|100|1|B|64").as_bytes());
    }
    h.poll(t + 1);
    let before = h.rt.conn(ConnId(8)).unwrap().sys().recvmsg;
    for n in 1..=5 {
        h.net
            .push(ConnId(8), format!("D|S1|{n}|1|100 1|101 1").as_bytes());
    }
    for n in 1..=3 {
        h.net.push(ConnId(0), &b("S1", n));
    }
    let ls = h.poll(t + 2);
    assert!(ls.did_work);
    assert_eq!(ls.ring_full, 1);
    assert_eq!(bbos(&h.drain_bbo()).len(), 3);
    assert_eq!(h.rt.conn(ConnId(8)).unwrap().sys().recvmsg, before);
    assert!(h.drain_md().iter().all(|m| matches!(m, FeedMsg::Trade(_, _))));
}
#[test]
fn ven_078_depth_drop_isolated() {
    use hft_event::ConnStateKind;
    let mut h = feed(f0());
    let mut t = 100;
    h.up(&[S1, S2], streams::BBO | streams::BOOK, &mut t);
    h.clear();
    h.net.at(ConnId(8), 0, crate::support::net::Ev::Eof);
    h.net.push(ConnId(0), &b("S1", 40));
    h.poll(t + 1);
    let bbo = h.drain_bbo();
    assert_eq!(bbos(&bbo).len(), 1);
    assert!(states(&bbo).is_empty());
    let md = states(&h.drain_md());
    assert!(md.iter().any(|s| s.state == ConnStateKind::Down && s.conn == 8));
    for inst in [S1, S2] {
        assert!(
            md.iter()
                .any(|s| s.state == ConnStateKind::Resync && s.inst == inst && s.reason == 15)
        );
    }
}

#[test]
fn ven_065_event_rings() {
    let mut h = feed(f0());
    let mut t = 100;
    h.up(&[S1], 127, &mut t);
    h.clear();
    h.net.push(ConnId(0), &b("S1", 1));
    h.net.push(ConnId(8), b"D|S1|10|1|100 1|101 1");
    for p in [
        b"T|S1|77|100|1|B".as_slice(),
        b"M|S1|1000|100",
        b"I|S1|1000|100",
        b"F|S1|1000|0.0001|2000",
        b"L|S1|1000|100|1",
    ] {
        h.net.push(ConnId(9), p);
    }
    h.poll(t + 1);
    assert_eq!(bbos(&h.drain_bbo()).len(), 1);
    let e: Vec<_> = h
        .drain_md()
        .into_iter()
        .filter(|e| !matches!(e, FeedMsg::ConnState(_)))
        .collect();
    assert_eq!(e.len(), 6);
    assert!(matches!(e[0], FeedMsg::Bulk { .. }));
    assert!(matches!(e[1], FeedMsg::Trade(_, _)));
    assert!(matches!(e[2], FeedMsg::MarkPrice(_, _)));
    assert!(matches!(e[3], FeedMsg::IndexPrice(_, _)));
    assert!(matches!(e[4], FeedMsg::Funding(_, _)));
    assert!(matches!(e[5], FeedMsg::Liquidation(_, _)));
    assert!(h.bulk.as_mut().unwrap().peek().is_some());
}

#[test]
fn ven_396_bind_wire_rebinds_and_resubscribes() {
    use hft_event::{ConnStateKind, FeedCmd};
    let mut h = feed(f0());
    let mut t = 100;
    h.up(&[S1], streams::BOOK, &mut t);
    let depth = ConnId(8);
    h.clear();
    let parts = |h: &mut crate::support::feedh::Fh, id: [u8; 32], upto: u8| {
        for p in 0..upto {
            let mut chunk = [0u8; 8];
            chunk.copy_from_slice(&id[usize::from(p) * 8..usize::from(p) * 8 + 8]);
            h.cmd
                .try_push(FeedCmd::BindWire {
                    inst: S1,
                    part: p,
                    chunk,
                })
                .unwrap();
        }
    };
    // .1 four parts: bound once, then UNSUB and SUB of the depth stream on its connection, and
    // the book waits for a snapshot (Resync NEW_SUB).
    let id: [u8; 32] = core::array::from_fn(|k| k as u8);
    parts(&mut h, id, 4);
    // Subscription frames are paced: give the queue time to drain.
    for _ in 0..4 {
        t += 1_000_000_000;
        h.poll(t);
    }
    assert_eq!(h.rt.ctx().bound, vec![(S1, id)], "VEN-396.1");
    let sent = h.net.sent_text(&mut h.rt, depth);
    let unsub = sent
        .iter()
        .position(|f| f.starts_with("UNSUB|") && f.contains(&format!("{}d", S1.0)));
    let sub = sent
        .iter()
        .rposition(|f| f.starts_with("SUB|") && f.contains(&format!("{}d", S1.0)));
    assert!(
        matches!((unsub, sub), (Some(u), Some(s)) if u < s),
        "VEN-396.1 {sent:?}"
    );
    let st = states(&h.drain_md());
    assert!(
        st.iter().any(|s| s.state == ConnStateKind::Resync
            && s.inst == S1
            && s.reason == hft_venue::codes::NEW_SUB),
        "VEN-396.1 {st:?}"
    );
    assert_eq!(h.rt.counters().binds, 1, "VEN-396.1");
    // .2 a missing part: part 3 alone does not bind.
    let before = h.rt.counters().cmd_ignored;
    h.cmd
        .try_push(FeedCmd::BindWire {
            inst: S1,
            part: 3,
            chunk: [9; 8],
        })
        .unwrap();
    t += 10;
    h.poll(t);
    assert_eq!(h.rt.ctx().bound.len(), 1, "VEN-396.2");
    assert_eq!(h.rt.counters().cmd_ignored, before + 1, "VEN-396.2");
    // .3 refused by the protocol: counted, no resubscription.
    h.clear();
    parts(&mut h, [0xFF; 32], 4);
    t += 10;
    h.poll(t);
    assert_eq!(h.rt.counters().bind_failed, 1, "VEN-396.3");
    assert!(
        h.net
            .sent_text(&mut h.rt, depth)
            .iter()
            .all(|f| !f.starts_with("UNSUB|")),
        "VEN-396.3"
    );
}
