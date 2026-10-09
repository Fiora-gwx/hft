//! CTL-010–CTL-014 (socket rows run on Linux).
#![allow(
    clippy::type_complexity,
    clippy::disallowed_types,
    clippy::disallowed_methods,
    clippy::disallowed_macros
)] // allow-list: test code, not a hot path

mod common;

use common::*;
use hft_types::Tsc;
use serde_json::{Value, json};

fn hedge() -> Value {
    json!({"type":"HedgeFix","group":"16777317"})
}

#[test]
fn ctl_013_next_deadline() {
    let mut g = G0::new();
    g.paused = true;
    let a = g.req("cli:tom", Some("acct-a"), hedge());
    assert!(g.send_v(&a).is_none());
    g.step();
    g.at(1_000 * MS);
    let b = g.req("cli:tom", Some("acct-a"), hedge());
    assert!(g.send_v(&b).is_none());
    g.step();
    assert_eq!(g.core.next_deadline(), Some(Tsc(5_000 * MS)));
    let (id_a, seq_a) = g.unanswered.remove(0);
    g.result(id_a, seq_a, Outcome::APPLIED);
    assert_eq!(g.core.next_deadline(), Some(Tsc(6_000 * MS)));
    g.answer_all();
    assert_eq!(g.core.next_deadline(), None);
}

#[test]
fn ctl_014_pending_capacity() {
    let mut cfg = config(std::env::temp_dir().join("ctl-014.sock"));
    cfg.result_wait_ms = 2_000_000_000;
    let mut g = G0::with_cfg(p0::p0(), 262_144, g0_active(), cfg);
    g.paused = true;
    // Different issuers keep the per-issuer rate limit out of the way.
    let issuers = ["cli:tom", "cli:alice", "tg:1002", "tg:1003", "tg:1004", "ops_bot"];
    let mut k = 0usize;
    let mut next = |g: &mut G0| {
        let issuer = issuers[k % 2];
        k += 1;
        // Rate windows: spread the requests over time.
        g.at(g.now + 6 * S);
        g.req(issuer, Some("acct-a"), hedge())
    };
    for _ in 0..62 {
        let v = next(&mut g);
        assert!(g.send_v(&v).is_none());
    }
    let v = next(&mut g);
    assert!(g.send_v(&v).is_none(), "CTL-014.1");
    assert_eq!(g.core.pending_len(), 63, "CTL-014.1");
    let v = next(&mut g);
    assert!(g.send_v(&v).is_none(), "CTL-014.2");
    assert_eq!(g.core.pending_len(), 64, "CTL-014.2");
    g.step();
    let n = g.seen.len();
    let v = next(&mut g);
    let r = g.send_v(&v).unwrap();
    assert_eq!((status(&r).as_str(), code(&r)), ("busy", "Busy"), "CTL-014.3");
    g.step();
    let recs = &g.seen[n..];
    assert!(G0::<CtlTestSet>::cmds(recs).is_empty(), "CTL-014.3");
    assert_eq!(only_audit(recs).result_code, 31, "CTL-014.3");
    let (id, seq) = g.unanswered.remove(0);
    g.result(id, seq, Outcome::APPLIED);
    let v = next(&mut g);
    assert!(g.send_v(&v).is_none(), "CTL-014.4");
    let _ = issuers[2..].len();
}

#[cfg(target_os = "linux")]
mod linux {
    use std::io::{Read, Write};
    use std::time::Instant;

    use hft_ctl_proto::encode_frame;
    use hft_rt::{PollTask, WaitFds};
    use proptest::prelude::*;
    use serde_json::json;

    use super::common::sock::Sock;
    use super::common::*;

    fn frame(v: &serde_json::Value) -> Vec<u8> {
        let mut f = Vec::new();
        encode_frame(&serde_json::to_vec(v).unwrap(), &mut f);
        f
    }

    fn req(issuer: &str) -> serde_json::Value {
        let g = G0::new();
        g.req(
            issuer,
            Some("acct-a"),
            json!({"type":"HedgeFix","group":"16777317"}),
        )
    }

    fn read_frame(c: &mut std::os::unix::net::UnixStream) -> Vec<u8> {
        let mut out = Vec::new();
        c.read_to_end(&mut out).unwrap();
        out
    }

    #[test]
    fn ctl_011_one_request_per_connection() {
        let mut s = Sock::new("011", |_| {});
        let mut c = s.connect_as(1201);
        c.write_all(&frame(&req("cli:tom"))).unwrap();
        s.poll_until(1_000, |s| s.gw.conn_count() == 0);
        let out = read_frame(&mut c);
        let n = u32::from_be_bytes(out[..4].try_into().unwrap()) as usize;
        assert_eq!(out.len(), 4 + n, "exactly one response frame, then EOF");
        let _ = c.write_all(&frame(&req("cli:tom")));
        for _ in 0..10 {
            s.poll();
        }
        assert_eq!(s.cmds().len(), 1);
    }

    #[test]
    fn ctl_012_wait_fds() {
        let mut s = Sock::new("012", |_| {});
        s.paused = true;
        let mut cs = Vec::new();
        for _ in 0..3 {
            let mut c = s.connect_as(1201);
            c.write_all(&frame(&req("cli:tom"))).unwrap();
            cs.push(c);
        }
        s.poll_until(1_000, |s| s.cmds().len() == 3);
        let mut w = WaitFds::new();
        s.gw.wait_fds(&mut w);
        assert_eq!(w.as_slice().len(), 4);
        s.paused = false;
        s.answer_all();
        s.poll_until(1_000, |s| s.gw.conn_count() == 0);
        let mut w = WaitFds::new();
        s.gw.wait_fds(&mut w);
        assert_eq!(w.as_slice().len(), 1);
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(200))]
        #[test]
        fn ctl_010_poll_once_never_blocks(
            n in 0usize..64,
            cuts in proptest::collection::vec(1usize..4096, 64),
            kinds in proptest::collection::vec(0u8..10, 64),
        ) {
            let run = || {
                let mut s = Sock::new("010", |_| {});
                let mut conns = Vec::new();
                for &kind in kinds.iter().take(n) {
                    let c = s.connect_as(1201);
                    c.set_nonblocking(true).unwrap();
                    conns.push((c, frame(&req("cli:tom")), 0usize, kind));
                }
                let mut times = Vec::new();
                for _round in 0..8 {
                    for (i, (c, f, off, kind)) in conns.iter_mut().enumerate() {
                        match kind {
                            // 20 %: header only.
                            0 | 1 => {
                                if *off == 0 {
                                    let _ = c.write(&f[..4]);
                                    *off = 4;
                                }
                            }
                            // 10 %: close half-way.
                            2 => {
                                if *off == 0 {
                                    let _ = c.write(&f[..f.len() / 2]);
                                    *off = f.len();
                                    let _ = c.shutdown(std::net::Shutdown::Both);
                                }
                            }
                            _ => {
                                let end = (*off + cuts[i]).min(f.len());
                                if *off < end
                                    && let Ok(w) = c.write(&f[*off..end])
                                {
                                    *off += w;
                                }
                            }
                        }
                    }
                    let t = Instant::now();
                    let did = s.gw.poll_once(s.now()).did_work;
                    times.push(t.elapsed().as_nanos() as u64);
                    let _ = did;
                    s.poll();
                }
                times
            };
            let (a, b, c) = (run(), run(), run());
            for i in 0..a.len() {
                let m = a[i].min(b[i]).min(c[i]);
                prop_assert!(m <= 1_000_000 * if cfg!(debug_assertions) { 20 } else { 1 }, "poll_once took {m} ns");
            }
        }
    }
}
