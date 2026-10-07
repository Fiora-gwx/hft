//! VEN-100, VEN-103, VEN-104: dedup.
#![allow(
    clippy::disallowed_types,
    clippy::disallowed_macros,
    clippy::disallowed_methods
)]

mod support;

use hft_event::ConnStateKind;
use hft_net::ConnId;
use hft_venue::codes;
use hft_venue_proto::StreamKind;
use proptest::prelude::*;
use support::feedh::*;
use support::net::Ev;
use support::rd::*;

#[test]
fn ven_100_monotonic() {
    let (mut h, mut now) = bbo_up(|_| {}, &[S1]);
    h.rt.set_last_fwd(S1, StreamKind::Bbo, 100);
    for (row, u, fwd, last) in [
        ("VEN-100.1", 99u64, false, 100u64),
        ("VEN-100.2", 100, false, 100),
        ("VEN-100.3", 101, true, 101),
        ("VEN-100.4", 105, true, 105),
        ("VEN-100.5", 103, false, 105),
    ] {
        let d0 = h.rt.counters().dup;
        h.net.push(ConnId(0), &b("S1", u));
        now += 1;
        h.poll(now);
        let got = bbos(&h.drain_bbo());
        assert_eq!(got.len(), usize::from(fwd), "{row}");
        assert_eq!(h.rt.counters().dup - d0, u64::from(!fwd), "{row}");
        assert_eq!(h.rt.inst_seq(S1).unwrap().last_fwd[0], last, "{row}");
    }
}

/// Delivery plan: (connection, id) in arrival order.
fn plan(ids: &[u64], drops: [u8; 2], delays: &[(u8, u8)], cut: Option<(u8, usize)>) -> Vec<(u8, u64)> {
    let mut ev: Vec<(u64, u8, u64)> = Vec::new();
    for (k, &id) in ids.iter().enumerate() {
        let (d0, d1) = delays.get(k % delays.len().max(1)).copied().unwrap_or((0, 0));
        for (c, d) in [(0u8, d0), (1u8, d1)] {
            if (id.wrapping_mul(2654435761).wrapping_add(c as u64 * 97)) % 100 < u64::from(drops[c as usize])
            {
                continue;
            }
            if let Some((cc, at)) = cut
                && cc == c
                && k >= at
            {
                continue;
            }
            ev.push((k as u64 + u64::from(d), c, id));
        }
    }
    ev.sort_by_key(|e| (e.0, e.1));
    ev.into_iter().map(|(_, c, id)| (c, id)).collect()
}

proptest! {
    #![proptest_config(ProptestConfig { cases: 2000, .. ProptestConfig::default() })]
    #[test]
    fn ven_103_bbo_race_dedup(
        gaps in prop::collection::vec(1u64..5, 1..2000),
        drops in [0u8..=20, 0u8..=20],
        delays in prop::collection::vec((0u8..=50, 0u8..=50), 1..16),
        cut in prop::option::of((0u8..2, 0usize..2000)),
    ) {
        let mut ids = Vec::with_capacity(gaps.len());
        let mut x = 0u64;
        for g in gaps {
            x += g;
            ids.push(x);
        }
        let arrivals = plan(&ids, drops, &delays, cut);
        let (mut h, mut now) = bbo_up(|_| {}, &[S1]);
        // Deliver in arrival order; a run of arrivals on one connection is one read, drained fully
        // (frame cap) before the next connection's run arrives.
        let mut out: Vec<u64> = Vec::new();
        let mut i = 0;
        while i < arrivals.len() {
            let c = arrivals[i].0;
            let mut j = i;
            while j < arrivals.len() && arrivals[j].0 == c {
                h.net.at(ConnId(c), 0, Ev::Text(b("S1", arrivals[j].1)));
                j += 1;
            }
            for _ in 0..((j - i) / 8 + 1) {
                now += 1;
                h.poll(now);
                out.extend(bbos(&h.drain_bbo()).iter().map(|b| b.update_id));
            }
            i = j;
        }
        if let Some((cc, _)) = cut {
            // The cut connection disconnects after its last delivery; the other one carries on.
            h.net.at(ConnId(cc), 0, Ev::Rst);
            now += 1;
            h.poll(now);
            out.extend(bbos(&h.drain_bbo()).iter().map(|b| b.update_id));
        }
        let mut model = Vec::new();
        let mut max = 0;
        for (_, id) in &arrivals {
            if *id > max {
                max = *id;
                model.push(*id);
            }
        }
        prop_assert_eq!(out, model);
    }
}

#[test]
fn ven_104_race_single_continues() {
    let (mut h, mut now) = bbo_up(|c| c.bbo_conns = 2, &[S1]);
    for u in 1..=5 {
        h.net.push(ConnId(0), &b("S1", u));
        h.net.push(ConnId(1), &b("S1", u));
        now += 1;
        h.poll(now);
    }
    h.net.at(ConnId(0), 0, Ev::Rst);
    for u in 6..=10 {
        h.net.push(ConnId(1), &b("S1", u));
        now += 1;
        h.poll(now);
    }
    let v = h.drain_bbo();
    let u: Vec<u64> = bbos(&v).iter().map(|b| b.update_id).collect();
    assert_eq!(u, (1..=10).collect::<Vec<_>>());
    let n = states(&v)
        .iter()
        .filter(|s| s.state == ConnStateKind::Degraded && s.reason == codes::RACE_SINGLE)
        .count();
    assert_eq!(n, 1);
}
