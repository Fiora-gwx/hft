//! VEN-008, VEN-010–VEN-012, VEN-014, VEN-017, VEN-018: configuration keys and constructor checks.
#![allow(
    clippy::disallowed_types,
    clippy::disallowed_macros,
    clippy::disallowed_methods
)]

mod support;

use hft_net::ConnId;
use hft_rt::IdlePolicy;
use hft_rt::{Backing, spsc};
use hft_venue::{
    DepthMode, FeedCfg, FeedRoutes, GwCfg, InitErr, RawMode, ReadyPollMode, SnapCfg, SnapDeps, SnapFetchTask,
    ready_poll_on, streams,
};
use support::feedh::*;
use support::gwh::*;
use support::net::Net;
use support::rd::*;
use support::stub::{self, Stub, StubProto};

#[test]
fn ven_008_hot_defaults() {
    let f = FeedCfg::default();
    let g = GwCfg::default();
    assert_eq!(
        (f.bbo_conns, f.bbo_frames_per_poll, f.frames_per_poll),
        (2, 8, 64),
        "VEN-008.1"
    );
    assert_eq!(
        (f.ready_poll, f.bbo_ping_ms, f.bbo_dead_ms, f.bbo_refresh_grace_ms),
        (ReadyPollMode::Auto, 1000, 3000, 500),
        "VEN-008.2"
    );
    assert_eq!(
        (f.depth, f.topn_levels, f.snapshot_part_levels),
        (DepthMode::TopN, 5, 64),
        "VEN-008.3"
    );
    assert_eq!(
        (f.resync_slots, f.resync_buf, f.snapshot_backoff_ms),
        (4, 1024, (100, 5000)),
        "VEN-008.4"
    );
    assert_eq!(
        (f.raw_dups, f.raw_sample_ms, f.ring_full_park_us),
        (false, 1000, 200),
        "VEN-008.5"
    );
    assert_eq!(f.rotate_margin_s, 3600, "VEN-008.6");
    assert_eq!(
        (
            g.cmds_per_loop,
            g.cmds_per_account_per_loop,
            g.aux_every,
            g.rest_idle_check_ms
        ),
        (16, 8, 8, 1000),
        "VEN-008.7"
    );
    assert_eq!(
        (
            g.rotate_wait_max_ms,
            g.warm_interval_ns,
            g.inflight_timeout_ms,
            g.stale_guard_ms
        ),
        (60_000, 0, 3000, 1500),
        "VEN-008.8"
    );
    let _ = RawMode::On;
}

#[test]
fn ven_010_ready_poll_on() {
    let rows = [
        ("VEN-010.1", ReadyPollMode::Auto, IdlePolicy::Spin, 3, true),
        ("VEN-010.2", ReadyPollMode::Auto, IdlePolicy::Spin, 2, false),
        ("VEN-010.3", ReadyPollMode::Auto, IdlePolicy::Spin, 4, true),
        (
            "VEN-010.4",
            ReadyPollMode::Auto,
            IdlePolicy::SpinThenPark {
                spin_us: 50,
                park_us: 1000,
            },
            12,
            false,
        ),
        (
            "VEN-010.5",
            ReadyPollMode::Auto,
            IdlePolicy::Park { park_us: 1000 },
            12,
            false,
        ),
        ("VEN-010.6", ReadyPollMode::Off, IdlePolicy::Spin, 12, false),
    ];
    for (row, m, p, n, want) in rows {
        assert_eq!(ready_poll_on(m, p, n), want, "{row}");
    }
}

fn feed_err(cfg: FeedCfg) -> Result<(), InitErr> {
    try_feed::<StubProto>(cfg, Caps::default(), Net::new(0, 0), rd_stub()).map(|_| ())
}

fn snap_err(cfg: SnapCfg) -> Result<(), InitErr> {
    let (_, rx) = spsc(64, Backing::Heap).unwrap();
    let (rp, _) = spsc(64, Backing::Heap).unwrap();
    let (bp, _) = spsc(64, Backing::Heap).unwrap();
    let (op, _) = hft_rt::byte_spsc(1 << 18, Backing::Heap).unwrap();
    let deps = SnapDeps {
        rd: rd_stub(),
        reqs: [None, None],
        out: [Some(op), None],
        handoff: rx,
        retire: rp,
        build: bp,
        clock: clock(),
    };
    SnapFetchTask::<StubProto>::new(cfg, deps, ()).map(|_| ())
}

fn gw_err(cfg: GwCfg) -> Result<(), InitErr> {
    gw_with::<StubProto, 2048>(cfg, &[A1, A2], GhOpts::default()).map(|_| ())
}

#[test]
fn ven_011_zero_keys() {
    let f = |m: fn(&mut FeedCfg)| {
        let mut c = f0();
        m(&mut c);
        feed_err(c)
    };
    assert_eq!(
        f(|c| c.bbo_frames_per_poll = 0),
        Err(InitErr::Cfg("feed.bbo_frames_per_poll")),
        "VEN-011.1"
    );
    assert_eq!(f(|c| c.bbo_frames_per_poll = 1), Ok(()), "VEN-011.1");
    assert_eq!(
        f(|c| c.frames_per_poll = 0),
        Err(InitErr::Cfg("feed.frames_per_poll")),
        "VEN-011.2"
    );
    assert_eq!(f(|c| c.frames_per_poll = 1), Ok(()), "VEN-011.2");
    assert_eq!(
        f(|c| c.bbo_conns = 0),
        Err(InitErr::Cfg("feed.bbo_conns")),
        "VEN-011.3"
    );
    assert_eq!(f(|c| c.bbo_conns = 1), Ok(()), "VEN-011.3");
    assert_eq!(
        f(|c| c.bbo_ping_ms = 0),
        Err(InitErr::Cfg("feed.bbo_ping_ms")),
        "VEN-011.4"
    );
    assert_eq!(
        f(|c| c.bbo_dead_ms = 0),
        Err(InitErr::Cfg("feed.bbo_dead_ms")),
        "VEN-011.4"
    );
    assert_eq!(
        snap_err(SnapCfg {
            parallel: 0,
            ..SnapCfg::default()
        }),
        Err(InitErr::Cfg("snap.parallel")),
        "VEN-011.5"
    );
    assert_eq!(
        snap_err(SnapCfg {
            parallel: 1,
            ..SnapCfg::default()
        }),
        Ok(()),
        "VEN-011.5"
    );
    let g = |m: fn(&mut GwCfg)| {
        let mut c = g0();
        m(&mut c);
        gw_err(c)
    };
    assert_eq!(
        g(|c| c.cmds_per_loop = 0),
        Err(InitErr::Cfg("gw.cmds_per_loop")),
        "VEN-011.6"
    );
    assert_eq!(
        g(|c| c.cmds_per_account_per_loop = 0),
        Err(InitErr::Cfg("gw.cmds_per_account_per_loop")),
        "VEN-011.6"
    );
    assert_eq!(
        g(|c| c.aux_every = 0),
        Err(InitErr::Cfg("gw.aux_every")),
        "VEN-011.6"
    );
    assert_eq!(
        g(|c| {
            c.cmds_per_loop = 1;
            c.cmds_per_account_per_loop = 1;
            c.aux_every = 1;
        }),
        Ok(()),
        "VEN-011.6"
    );
    assert_eq!(
        g(|c| c.unmatched_hold_cap = 0),
        Err(InitErr::Cfg("gw.unmatched_hold_cap")),
        "VEN-011.7"
    );
    assert_eq!(
        g(|c| c.acct_query_degraded_after = 0),
        Err(InitErr::Cfg("gw.acct_query_degraded_after")),
        "VEN-011.7"
    );
    assert_eq!(
        g(|c| c.report_decode_down_after = 0),
        Err(InitErr::Cfg("gw.report_decode_down_after")),
        "VEN-011.7"
    );
    assert_eq!(
        g(|c| {
            c.unmatched_hold_cap = 1;
            c.acct_query_degraded_after = 1;
            c.report_decode_down_after = 1;
        }),
        Ok(()),
        "VEN-011.7"
    );
}

#[test]
fn ven_012_bbo_slots() {
    let rows = [
        ("VEN-012.1", 300u32, 100u16, 2u8, true),
        ("VEN-012.2", 301, 100, 2, false),
        ("VEN-012.3", 50, 50, 7, true),
        ("VEN-012.4", 50, 50, 8, false),
        ("VEN-012.5", 51, 50, 2, true),
        ("VEN-012.6", 0, 50, 2, true),
    ];
    for (row, insts, spc, conns, ok) in rows {
        let rd = rd_with(&RdOpts {
            streams_per_conn: spc,
            ..RdOpts::default()
        });
        let cfg = FeedCfg {
            routes: FeedRoutes::Bbo,
            max_sub_insts: insts,
            bbo_conns: conns,
            ..f0()
        };
        let r = try_feed::<StubProto>(cfg, Caps::default(), Net::new(0, 0), rd);
        match r {
            Ok(mut h) => {
                assert!(ok, "{row}");
                if row == "VEN-012.6" {
                    for t in 0..5 {
                        h.poll(t);
                    }
                    assert!(h.all_builds.is_empty(), "{row}: no BBO connection");
                }
            }
            Err(e) => {
                assert!(!ok, "{row}");
                assert_eq!(e, InitErr::BboSlots, "{row}");
            }
        }
    }
}

#[test]
fn ven_014_gw_new_rejects() {
    assert_eq!(
        gw_with::<StubProto, 2048>(g0(), &[A1, A9], GhOpts::default()).err(),
        Some(InitErr::SimAccount),
        "VEN-014.1"
    );
    assert!(
        gw_with::<StubProto, 2048>(g0(), &[A1, A2], GhOpts::default()).is_ok(),
        "VEN-014.2"
    );
    assert_eq!(
        gw_with::<Stub<8192, 0>, 2048>(g0(), &[A1, A2], GhOpts::default()).err(),
        Some(InitErr::FrameCap),
        "VEN-014.3"
    );
    assert!(
        gw_with::<Stub<8192, 0>, 8192>(g0(), &[A1, A2], GhOpts::default()).is_ok(),
        "VEN-014.4"
    );
    assert!(
        gw_with::<Stub<2047, 0>, 2048>(g0(), &[A1, A2], GhOpts::default()).is_ok(),
        "VEN-014.5"
    );
}

#[test]
fn ven_017_sub_rate() {
    let rows = [
        ("VEN-017.1", 4u16, 1000u32, FeedRoutes::Bbo, true),
        ("VEN-017.2", 3, 1000, FeedRoutes::Bbo, false),
        ("VEN-017.3", 5, 1000, FeedRoutes::Bbo, true),
        ("VEN-017.4", 10, 250, FeedRoutes::Bbo, true),
        ("VEN-017.5", 10, 249, FeedRoutes::Bbo, false),
        ("VEN-017.6", 2, 1000, FeedRoutes::Md, true),
        ("VEN-017.7", 1, 1000, FeedRoutes::Md, false),
    ];
    for (row, inm, ping, routes, ok) in rows {
        let rd = rd_with(&RdOpts {
            in_msgs_per_s: inm,
            ..RdOpts::default()
        });
        let cfg = FeedCfg {
            routes,
            bbo_ping_ms: ping,
            ..f0()
        };
        let r = try_feed::<StubProto>(cfg, Caps::default(), Net::new(0, 0), rd).map(|_| ());
        assert_eq!(r, if ok { Ok(()) } else { Err(InitErr::SubRate) }, "{row}");
    }
}

#[test]
fn ven_018_refdata_effective_values() {
    // Protocol defaults (default_caps Poll{5000}, routes max_streams 200 / 20 msg/s) differ from
    // refdata (50 streams, 10 msg/s): the runtime uses refdata.
    let mut h = feed(FeedCfg {
        routes: FeedRoutes::Bbo,
        max_sub_insts: 100,
        ..f0()
    });
    stub::take_log();
    for n in 1..=51 {
        h.sub(p(n), streams::BBO);
    }
    for t in 0..10 {
        h.poll(t);
    }
    assert_eq!(h.all_builds.len(), 4, "two groups of two (split at 50, not 200)");
    h.complete_builds();
    let mut times = Vec::new();
    let mut t = 100u64;
    while t < 1_100_000_000 {
        t += 10_000_000;
        h.poll(t);
        for f in h.net.sent_text(&mut h.rt, ConnId(0)) {
            if f.starts_with("SUB|") {
                times.push(t);
            }
        }
    }
    // 10 msg/s → BBO frames every 1000 / (5 − 1) = 250 ms.
    assert_eq!(times.len(), 5);
    for w in times.windows(2) {
        assert!(w[1] - w[0] >= 250_000_000, "{times:?}");
    }
    assert!(
        stub::calls("default_caps").is_empty(),
        "default_caps never called"
    );
    // The gw likewise: constructing it reads refdata only.
    let _g = gw::<StubProto>();
    assert!(stub::calls("default_caps").is_empty());
}
