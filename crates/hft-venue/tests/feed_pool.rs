//! VEN-079: `Up` at hand-over, paced subscription frames.
#![allow(
    clippy::disallowed_types,
    clippy::disallowed_macros,
    clippy::disallowed_methods
)]

mod support;

use hft_event::{ConnStateKind, FeedMsg};
use hft_net::ConnId;
use hft_types::InstId;
use hft_venue::{Effect, FeedCfg, FeedRoutes, streams};
use support::feedh::*;
use support::rd::*;

#[test]
fn ven_079_install_up_and_paced_subs() {
    let mut h = feed(FeedCfg {
        routes: FeedRoutes::Bbo,
        bbo_ping_ms: 1000,
        ..f0()
    });
    for n in 1..=50 {
        h.sub(p(n), streams::BBO);
    }
    h.poll(0);
    h.complete(ConnId(0));
    h.rt.trace(true);
    h.poll(0);
    let tr = h.rt.take_trace();
    let up = tr.iter().position(|e| *e == Effect::Bbo).unwrap();
    let sub = tr.iter().position(|e| *e == Effect::Send(ConnId(0))).unwrap();
    assert!(up < sub, "Up before any subscription frame: {tr:?}");
    let v = h.drain_bbo();
    assert!(
        matches!(v[0], FeedMsg::ConnState(s) if s.state == ConnStateKind::Up && s.conn == 0 && s.inst == InstId(0))
    );
    // Send times of subscription frames and probes.
    let mut subs: Vec<u64> = Vec::new();
    let mut probes: Vec<u64> = Vec::new();
    let mut t = 0u64;
    loop {
        for f in h.net.sent(&mut h.rt, ConnId(0)) {
            if f.opcode == 9 {
                probes.push(t);
            } else if f.payload.starts_with(b"SUB|") {
                subs.push(t);
            }
        }
        t += 10_000_000;
        if t >= 3_000_000_000 {
            break;
        }
        h.poll(t);
    }
    assert_eq!(subs.len(), 5, "{subs:?}");
    for (k, &s) in subs.iter().enumerate() {
        assert!(s >= k as u64 * 250_000_000, "frame {k} at {s}");
    }
    for w in (0..3_000_000_000u64).step_by(10_000_000) {
        let n_sub = subs.iter().filter(|&&s| s >= w && s < w + 1_000_000_000).count();
        let n_probe = probes
            .iter()
            .filter(|&&s| s >= w && s < w + 1_000_000_000)
            .count();
        assert!(n_sub <= 4, "window {w}: {n_sub} subscription frames");
        assert!(n_sub + n_probe <= 5, "window {w}: {n_sub} + {n_probe}");
    }
}
