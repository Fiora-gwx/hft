//! VEN-040–VEN-046, VEN-050–VEN-053, VEN-056, VEN-084, VEN-085: the feed round.
#![allow(
    clippy::disallowed_types,
    clippy::disallowed_macros,
    clippy::disallowed_methods
)]

use crate::support;

use hft_event::{FeedCmd, FeedMsg};
use hft_net::ConnId;
use hft_rt::PollTask;
use hft_venue::{FeedCfg, FeedRoutes, streams};
use proptest::prelude::*;
use support::feedh::*;
use support::net::Ev;
use support::rd::*;
use support::stub;

#[test]
fn ven_040_cmds_per_poll() {
    let mut h = feed(f0());
    for i in 1..=12 {
        h.sub(p(i), streams::BBO);
    }
    let ls = h.poll(100);
    assert_eq!((ls.cmds, ls.did_work), (8, true));
    assert_eq!(h.cmd.free_hint(), 256 - 4, "4 left in feedctl");
    let ls = h.poll(200);
    assert_eq!(ls.cmds, 4);
    assert_eq!(h.cmd.free_hint(), 256);
}

fn ready6() -> (Fh, u64) {
    let mut cfg = FeedCfg {
        routes: FeedRoutes::Bbo,
        ready: true,
        bbo_conns: 6,
        ..f0()
    };
    cfg.max_sub_insts = 50;
    let mut h = feed(cfg);
    // Everything ready while connecting.
    for i in 0..6 {
        h.net.set_ready(ConnId(i), true);
    }
    let mut now = 100;
    h.up(&[S1], streams::BBO, &mut now);
    for i in 0..6 {
        h.net.set_ready(ConnId(i), false);
    }
    h.clear();
    (h, now)
}

#[test]
fn ven_041_ready_probe_reads_ready_only() {
    let (mut h, now) = ready6();
    let rec0: Vec<u64> = (0..6)
        .map(|i| h.rt.conn(ConnId(i)).unwrap().sys().recvmsg)
        .collect();
    let ep0 = h.rt.ready_sys().unwrap().epoll_pwait;
    h.net.push(ConnId(1), &b("S1", 1));
    h.net.push(ConnId(4), &b("S1", 2));
    h.net.set_ready(ConnId(1), true);
    h.net.set_ready(ConnId(4), true);
    let t = now + 1000;
    h.poll(t);
    let s = h.rt.ready_sys().unwrap();
    assert_eq!((s.epoll_pwait - ep0, s.last_epoll_timeout), (1, 0));
    for i in 0..6u8 {
        let d = h.rt.conn(ConnId(i)).unwrap().sys().recvmsg - rec0[i as usize];
        let want = if i == 1 || i == 4 { 1 } else { 0 };
        assert_eq!(d, want, "conn {i}");
        if want == 0 {
            assert_eq!(
                h.rt.conn(ConnId(i)).unwrap().test_empty_tsc(),
                Some(t),
                "note_empty on conn {i}"
            );
        }
    }
}

#[test]
fn ven_042_note_empty_gap() {
    let (mut h, _) = ready6();
    stub::take_log();
    h.poll(1_000_000);
    h.net.at(ConnId(2), 1_200_000, Ev::Text(b("S1", 11)));
    h.net.set_ready(ConnId(2), true);
    h.poll(1_250_000);
    let v = bbos(&h.drain_bbo());
    assert_eq!(v.len(), 1);
    assert_eq!(v[0].h.rx_empty_gap_ns, 250_000);
    assert_eq!(
        stub::calls("decode_md")
            .last()
            .unwrap()
            .rx
            .unwrap()
            .rx_empty_gap_ns,
        250_000
    );
}

#[test]
fn ven_043_leftover_frames_without_ready() {
    let (mut h, now) = ready6();
    let frames: Vec<Ev> = (1..=12).map(|u| Ev::Text(b("S1", u))).collect();
    for f in frames {
        h.net.at(ConnId(0), 0, f);
    }
    h.net.set_ready(ConnId(0), true);
    h.poll(now + 10);
    let u1: Vec<u64> = bbos(&h.drain_bbo()).iter().map(|x| x.update_id).collect();
    assert_eq!(u1, (1..=8).collect::<Vec<_>>());
    h.net.set_ready(ConnId(0), false);
    let r0 = h.rt.conn(ConnId(0)).unwrap().sys().recvmsg;
    h.poll(now + 20);
    let u2: Vec<u64> = bbos(&h.drain_bbo()).iter().map(|x| x.update_id).collect();
    assert_eq!(u2, (9..=12).collect::<Vec<_>>());
    assert_eq!(h.rt.conn(ConnId(0)).unwrap().sys().recvmsg - r0, 0);
}

#[test]
fn ven_044_spurious_ready_one_read() {
    let (mut h, now) = ready6();
    h.net.set_ready(ConnId(1), true);
    let r0 = h.rt.conn(ConnId(1)).unwrap().sys().recvmsg;
    let ls = h.poll(now + 10);
    assert_eq!(h.rt.conn(ConnId(1)).unwrap().sys().recvmsg - r0, 1);
    assert!(h.drain_bbo().is_empty());
    assert!(!ls.did_work);
}

#[test]
fn ven_045_no_ready_poll_all_once() {
    let (mut h, now) = bbo_up(|c| c.bbo_conns = 4, &[S1]);
    assert!(h.rt.ready_sys().is_none(), "no epoll_pwait at all");
    let r0: Vec<u64> = (0..4)
        .map(|i| h.rt.conn(ConnId(i)).unwrap().sys().recvmsg)
        .collect();
    // 300 B on connection 2 (one short read).
    let mut f = b("S1", 1);
    f.resize(300, b'0');
    let mut f2 = b"B|S1|1|100.00|1.000|100.01|".to_vec();
    f2.extend(std::iter::repeat_n(b'0', 300 - f2.len()));
    let _ = f;
    h.net.at(ConnId(2), 0, Ev::Text(f2));
    h.poll(now + 10);
    for i in 0..4u8 {
        assert_eq!(
            h.rt.conn(ConnId(i)).unwrap().sys().recvmsg - r0[i as usize],
            1,
            "conn {i}"
        );
    }
}

#[test]
fn ven_046_feed_syscalls() {
    let cfg = FeedCfg {
        routes: FeedRoutes::All,
        ready: true,
        ..f0()
    };
    let mut h = feed(cfg);
    for i in 0..64 {
        h.net.set_ready(ConnId(i), true);
    }
    let mut now = 100;
    h.up(
        &[S1, S2, S3],
        streams::BBO | streams::BOOK | streams::TRADE,
        &mut now,
    );
    let ids: Vec<ConnId> = h.rt.conn_ids().collect();
    assert_eq!(ids.len(), 4, "2 BBO + depth + trades: {ids:?}");
    let s0: Vec<_> = ids.iter().map(|&i| *h.rt.conn(i).unwrap().sys()).collect();
    let e0 = *h.rt.ready_sys().unwrap();
    let mut seq = 0u64;
    for k in 0..10_000u64 {
        now += 100_000;
        if k % 10 == 0 {
            seq += 1;
            for &id in &ids {
                let f = match id.0 {
                    0 | 1 => b("S1", seq),
                    _ if id == ids[2] => format!("D|S1|{seq}|1|100.00 1.000|100.01 1.000").into_bytes(),
                    _ => format!("T|S1|{seq}|100.00|1.000|B").into_bytes(),
                };
                h.net.at(id, now, Ev::Text(f));
            }
        }
        if k % 10_000 == 5000 {
            for &id in &ids {
                h.net.at(id, now, Ev::Pong(vec![0; 8]));
            }
        }
        let ep = h.rt.ready_sys().unwrap().epoll_pwait;
        h.poll(now);
        let s = h.rt.ready_sys().unwrap();
        assert!(s.epoll_pwait - ep <= 1 && s.last_epoll_timeout == 0);
    }
    for (k, &id) in ids.iter().enumerate() {
        let s = h.rt.conn(id).unwrap().sys();
        let d_total = s.total() - s0[k].total();
        let d_io = (s.recvmsg - s0[k].recvmsg) + (s.sendmsg - s0[k].sendmsg);
        assert_eq!(d_total, d_io, "conn {id:?}: only recvmsg/sendmsg");
        assert_eq!(s.epoll_ctl, s0[k].epoll_ctl);
        for f in h.net.sent(&mut h.rt, id) {
            let t = String::from_utf8_lossy(&f.payload).into_owned();
            assert!(
                f.opcode == 9 || f.opcode == 10 || t.starts_with("SUB|") || t.starts_with("PING|"),
                "{id:?}: {t}"
            );
        }
    }
    let e = h.rt.ready_sys().unwrap();
    assert_eq!(e.epoll_ctl, e0.epoll_ctl);
    assert_eq!(e.total() - e0.total(), e.epoll_pwait - e0.epoll_pwait);
}

/// `(conn, frame)` of each classify call: conn 0 carries S1 (`u` = frame), conn 1 S2.
fn classify_order() -> Vec<(u8, u64)> {
    stub::take_log()
        .into_iter()
        .filter(|c| c.name == "classify")
        .filter_map(|c| {
            let f: Vec<&str> = c.arg.split('|').collect();
            let conn = match *f.get(1)? {
                "S1" => 0,
                "S2" => 1,
                "S3" => 2,
                _ => return None,
            };
            Some((conn, f.get(2)?.parse().ok()?))
        })
        .collect()
}

#[test]
fn ven_050_bbo_frame_round_robin() {
    let (mut h, now) = bbo_up(|c| c.bbo_frames_per_poll = 8, &[S1, S2]);
    for k in 1..=20 {
        h.net.at(ConnId(0), 0, Ev::Text(b("S1", k)));
        h.net.at(ConnId(1), 0, Ev::Text(b("S2", k)));
    }
    let mut rounds = Vec::new();
    for r in 0..3 {
        h.poll(now + 10 + r);
        rounds.push(classify_order());
    }
    let inter = |first: u8, ks: std::ops::RangeInclusive<u64>| {
        let mut v = Vec::new();
        for k in ks {
            v.push((first, k));
            v.push((1 - first, k));
        }
        v
    };
    // The first round's start depends on earlier rounds; it then alternates.
    let s0 = rounds[0][0].0;
    assert_eq!(rounds[0], inter(s0, 1..=8));
    assert_eq!(rounds[1], inter(1 - s0, 9..=16));
    assert_eq!(rounds[2], inter(s0, 17..=20));
}

#[test]
fn ven_051_single_backlog_cap() {
    let (mut h, now) = bbo_up(|c| c.bbo_frames_per_poll = 8, &[S1]);
    for k in 1..=20 {
        h.net.at(ConnId(0), 0, Ev::Text(b("S1", k)));
    }
    let ls = h.poll(now + 10);
    assert_eq!(classify_order(), (1..=8).map(|k| (0, k)).collect::<Vec<_>>());
    assert_eq!(ls.frames, 8);
    let reads = h.rt.conn(ConnId(0)).unwrap().sys().recvmsg;
    let ls = h.poll(now + 20);
    assert_eq!(ls.frames, 8, "12 were left: 8 now");
    assert_eq!(h.rt.conn(ConnId(0)).unwrap().sys().recvmsg, reads);
}

#[test]
fn ven_052_start_rotates() {
    let (mut h, now) = bbo_up(
        |c| {
            c.bbo_conns = 3;
            c.bbo_frames_per_poll = 1;
        },
        &[S1, S2, S3],
    );
    for k in 1..=10 {
        h.net.at(ConnId(0), 0, Ev::Text(b("S1", k)));
        h.net.at(ConnId(1), 0, Ev::Text(b("S2", k)));
        h.net.at(ConnId(2), 0, Ev::Text(b("S3", k)));
    }
    let mut firsts = Vec::new();
    for r in 0..4 {
        let ls = h.poll(now + 10 + r);
        assert_eq!(ls.frames, 3);
        firsts.push(classify_order()[0].0);
    }
    // The start rotates by one connection each round (its absolute value depends on earlier rounds).
    let s0 = firsts[0];
    assert_eq!(firsts, vec![s0, (s0 + 1) % 3, (s0 + 2) % 3, s0]);
}

proptest! {
    #![proptest_config(ProptestConfig { cases: 512, .. ProptestConfig::default() })]
    /// Tasks A and B (each `routes = Bbo`, two connections) polled in this order on one thread (one
    /// `TaskLoop` round = A then B). The target frame arrives on B's connection 1 when that
    /// connection has no backlog; frames processed in between ≤ A's 2 × bfpp + B's other connection 1.
    #[test]
    fn ven_053_bbo_wait_bound(bfpp in 1u16..=8, backlog in prop::collection::vec(0u64..=64, 3), at_round in 0usize..6) {
        let (mut ta, now_a) = bbo_up(|c| c.bbo_frames_per_poll = bfpp, &[S1, S2]);
        let (mut tb, now_b) = bbo_up(|c| c.bbo_frames_per_poll = bfpp, &[S3]);
        let mut now = now_a.max(now_b) + 10;
        // A: connection 0 carries S1, connection 1 carries S2; B: connection 0 carries S3.
        for k in 1..=backlog[0] {
            ta.net.at(ConnId(0), 0, Ev::Text(b("S1", k)));
        }
        for k in 1..=backlog[1] {
            ta.net.at(ConnId(1), 0, Ev::Text(b("S2", k)));
        }
        for k in 1..=backlog[2] {
            tb.net.at(ConnId(0), 0, Ev::Text(b("S3", k)));
        }
        stub::take_log();
        let target = 1_000_000u64;
        let mut seen = 0usize;
        let mut counting = false;
        let mut bound_hit = None;
        for r in 0..200 {
            now += 1000;
            if r == at_round {
                tb.net.at(ConnId(1), 0, Ev::Text(b("S3", target)));
                counting = true;
            }
            ta.poll(now);
            tb.poll(now);
            for c in stub::take_log().into_iter().filter(|c| c.name == "classify") {
                if !counting {
                    continue;
                }
                if c.arg.contains(&format!("|{target}")) {
                    bound_hit = Some(seen);
                    counting = false;
                    break;
                }
                seen += 1;
            }
            if bound_hit.is_some() {
                break;
            }
        }
        let n = bound_hit.expect("target processed");
        prop_assert!(n <= 2 * usize::from(bfpp) + 1, "{} > 2×{} + 1", n, bfpp);
    }
}

#[test]
fn ven_056_frame_cap_boundary() {
    for (row, n, want, left) in [
        ("VEN-056.1", 7u64, 7u16, 0u16),
        ("VEN-056.2", 8, 8, 0),
        ("VEN-056.3", 9, 8, 1),
    ] {
        let (mut h, now) = bbo_up(|c| c.bbo_frames_per_poll = 8, &[S1]);
        for k in 1..=n {
            h.net.at(ConnId(0), 0, Ev::Text(b("S1", k)));
        }
        assert_eq!(h.poll(now + 10).frames, want, "{row}");
        assert_eq!(h.poll(now + 20).frames, left, "{row} left");
    }
    for (row, n, want, left) in [
        ("VEN-056.4", 63u64, 63u16, 0u16),
        ("VEN-056.5", 64, 64, 0),
        ("VEN-056.6", 65, 64, 1),
    ] {
        let mut h = feed(FeedCfg {
            frames_per_poll: 64,
            ..f0()
        });
        let mut now = 100;
        h.up(&[S1], streams::BOOK, &mut now);
        let ids: Vec<ConnId> = h.rt.conn_ids().collect();
        assert_eq!(ids, vec![ConnId(8)], "{row}: the depth connection is 8");
        h.clear();
        for k in 1..=n {
            h.net.at(
                ConnId(8),
                0,
                Ev::Text(format!("D|S1|{k}|1|100.00 1.000|100.01 1.000").into_bytes()),
            );
        }
        assert_eq!(h.poll(now + 10).frames, want, "{row}");
        assert_eq!(h.poll(now + 20).frames, left, "{row} left");
    }
}

#[test]
fn ven_084_loop_stat() {
    let mut h = feed(FeedCfg {
        routes: FeedRoutes::Bbo,
        ..f0()
    });
    h.sub(S1, streams::BBO);
    h.poll(1_000_000_000);
    h.complete_builds();
    h.poll(1_000_000_000);
    h.ack(ConnId(0));
    h.ack(ConnId(1));
    h.poll(1_000_000_001);
    h.clear();
    let now = 1_500_000_000;
    for k in 1..=3 {
        h.net.at(ConnId(0), 0, Ev::Text(b("S1", k)));
    }
    h.cmd
        .try_push(FeedCmd::Subscribe {
            inst: S1,
            streams: streams::BBO,
        })
        .unwrap();
    h.cmd
        .try_push(FeedCmd::Subscribe {
            inst: S1,
            streams: streams::BBO,
        })
        .unwrap();
    let ls = h.poll(now);
    assert_eq!((ls.did_work, ls.frames, ls.cmds, ls.ring_full), (true, 3, 2, 0));
    let ls = h.poll(now + 1);
    assert_eq!(
        (ls.did_work, ls.frames, ls.cmds, ls.next_deadline),
        (false, 0, 0, Some(2_000_000_000))
    );
}

#[test]
fn ven_085_has_pending() {
    let mut h = feed_with::<stub::StubProto>(
        f0(),
        Caps {
            bbo: 16,
            ..Caps::default()
        },
    );
    let mut now = 100;
    h.up(&[S1], streams::BBO, &mut now);
    h.clear();
    h.poll(now + 1);
    assert!(!PollTask::has_pending(&h.rt), "VEN-085.1");
    h.cmd.try_push(FeedCmd::Unsubscribe { inst: S2 }).unwrap();
    assert!(PollTask::has_pending(&h.rt), "VEN-085.2");
    h.poll(now + 2);
    let r = h.snap.as_mut().unwrap().reserve(16).unwrap();
    r.fill(0);
    h.snap.as_mut().unwrap().commit();
    assert!(PollTask::has_pending(&h.rt), "VEN-085.3");
    // .5 then .4: 25 frames; two rounds fill the 16-slot ring, 9 stay in the TLS buffer.
    let mut h = feed_with::<stub::StubProto>(
        f0(),
        Caps {
            bbo: 16,
            ..Caps::default()
        },
    );
    let mut now = 100;
    h.up(&[S1], streams::BBO, &mut now);
    h.clear();
    for k in 1..=25 {
        h.net.at(ConnId(0), 0, Ev::Text(b("S1", k)));
    }
    h.poll(now + 1);
    h.poll(now + 2);
    assert!(!PollTask::has_pending(&h.rt), "VEN-085.5: bbo ring full");
    h.bbo.as_mut().unwrap().try_pop().unwrap();
    assert!(PollTask::has_pending(&h.rt), "VEN-085.4");
    let _ = FeedMsg::Bulk {
        flags: hft_event::RecordFlags(0),
        kind: hft_event::EventKind::BBO,
        len: 0,
    };
}
