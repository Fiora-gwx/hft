//! TK-010, TK-011, TK-021: market_stream and generator determinism.

use std::collections::HashMap;

use hft_event::{ConnStateKind, L2SnapHdr};
use hft_testkit::r#gen::market::{BBO_CONN, REASON_BBO_REFRESH, REASON_BBO_SILENT};
use hft_testkit::seed;
use hft_testkit::{
    GapPolicy, MarketStreamCfg, MdEvent, Perturb, QuietPolicy, market_stream, market_stream_with, perturb,
};
use hft_types::{InstId, VenueId};
use proptest::prelude::*;

fn gap_policy() -> impl Strategy<Value = GapPolicy> {
    prop_oneof![
        Just(GapPolicy::NoSeq),
        Just(GapPolicy::Contiguous),
        (1u32..=100).prop_map(|max_skip| GapPolicy::Skips { max_skip }),
    ]
}

fn stream_cfg() -> impl Strategy<Value = MarketStreamCfg> {
    (
        1usize..=4,
        1usize..=5000,
        gap_policy(),
        0u8..3,
        1u64..=30_000,
        1u64..=30_000,
        0usize..4,
    )
        .prop_map(|(k, n, gap, q, from_ms, len_ms, pick)| {
            let insts: Vec<InstId> = (1..=k as u32).map(InstId).collect();
            let mut cfg = MarketStreamCfg::new(VenueId(1), &insts, n, gap);
            let to_ms = from_ms + len_ms;
            cfg.quiet = match q {
                0 => QuietPolicy::None,
                1 => QuietPolicy::Still {
                    inst: insts[pick % k],
                    from_ms,
                    to_ms,
                },
                _ => QuietPolicy::Silent { from_ms, to_ms },
            };
            cfg
        })
}

fn check_stream(cfg: &MarketStreamCfg, evs: &[MdEvent]) -> Result<(), TestCaseError> {
    let mut last_rx: HashMap<u8, u64> = HashMap::new();
    let mut last_uid: HashMap<InstId, u64> = HashMap::new();
    for (i, e) in evs.iter().enumerate() {
        let prev = last_rx.insert(e.conn(), e.rx_tsc()).unwrap_or(0);
        prop_assert!(
            e.rx_tsc() >= prev,
            "event {i}: rx_tsc decreased on conn {}",
            e.conn()
        );
        match e {
            MdEvent::Bbo(b) => {
                prop_assert!(b.bid_px < b.ask_px, "event {i}: crossed {b:?}");
                prop_assert!(b.bid_qty.0 > 0 && b.ask_qty.0 > 0, "event {i}: qty {b:?}");
                let last = last_uid.insert(b.h.inst, b.update_id).unwrap_or(0);
                match cfg.gap {
                    GapPolicy::NoSeq => prop_assert_eq!(b.update_id, 0),
                    GapPolicy::Contiguous => prop_assert_eq!(b.update_id, last + 1),
                    GapPolicy::Skips { max_skip } => {
                        prop_assert!(b.update_id > last && b.update_id <= last + 1 + u64::from(max_skip))
                    }
                }
            }
            MdEvent::L2Snapshot(s) => {
                prop_assert_eq!(s.hdr.origin, L2SnapHdr::TOP_N);
                prop_assert_eq!(usize::from(s.hdr.n_bids), s.bids.len());
                prop_assert_eq!(usize::from(s.hdr.n_asks), s.asks.len());
                prop_assert!(
                    s.bids.windows(2).all(|w| w[0].px > w[1].px),
                    "event {i}: bids {s:?}"
                );
                prop_assert!(
                    s.asks.windows(2).all(|w| w[0].px < w[1].px),
                    "event {i}: asks {s:?}"
                );
                prop_assert!(s.bids.iter().chain(&s.asks).all(|l| l.qty.0 > 0 && l.px.0 > 0));
            }
            MdEvent::Funding(f) => {
                let period = i64::from(f.interval_s) * 1_000_000_000;
                prop_assert!(period > 0);
                prop_assert_eq!(
                    f.next_funding_ns % period,
                    0,
                    "event {}: next not on the period",
                    i
                );
                prop_assert!(f.next_funding_ns > f.h.ex_ts_ns && f.next_funding_ns - f.h.ex_ts_ns <= period);
            }
            MdEvent::Trade(t) => prop_assert!(t.qty.0 > 0 && t.px.0 > 0),
            MdEvent::Mark(m) => prop_assert!(m.px.0 > 0),
            MdEvent::Conn(_) => {}
        }
    }
    Ok(())
}

proptest! {
    #![proptest_config(ProptestConfig { cases: 256, ..ProptestConfig::default() })]

    #[test]
    fn tk_010_market_stream_valid(
        (cfg, evs) in stream_cfg().prop_flat_map(|c| (Just(c.clone()), market_stream_with(c)))
    ) {
        check_stream(&cfg, &evs)?;
    }
}

const MS: u64 = 1_000_000;
const X: InstId = InstId(1);

fn tk011_stream(quiet: QuietPolicy) -> Vec<MdEvent> {
    let mut cfg = MarketStreamCfg::new(VenueId(1), &[X], 3000, GapPolicy::Contiguous);
    cfg.bbo_ping_ms = 1000;
    cfg.tsc_hz = 1_000_000_000;
    cfg.start_tsc = 0;
    cfg.dt_ms = (5, 20);
    cfg.quiet = quiet;
    seed::sample(0x7C11, &market_stream_with(cfg))
}

#[test]
fn tk_011_quiet_policy() {
    let (from, to) = (1_000 * MS, 12_000 * MS);
    let in_window = |e: &MdEvent| (from..to).contains(&e.rx_tsc());

    // TK-011.1: book unchanged, connection healthy.
    let evs = tk011_stream(QuietPolicy::Still {
        inst: X,
        from_ms: 1_000,
        to_ms: 12_000,
    });
    assert!(
        !evs.iter()
            .any(|e| in_window(e) && matches!(e, MdEvent::Bbo(b) if b.h.inst == X)),
        "TK-011.1: Bbo of X inside the window"
    );
    let alive: Vec<u64> = evs
        .iter()
        .filter(|e| in_window(e))
        .filter_map(|e| match e {
            MdEvent::Conn(c) if c.conn == BBO_CONN && c.state == ConnStateKind::Alive => Some(c.rx_tsc),
            _ => None,
        })
        .collect();
    assert!(alive.len() >= 10, "TK-011.1: {} Alive in the window", alive.len());
    assert!(
        alive.windows(2).all(|w| w[1] - w[0] == 1_000 * MS),
        "TK-011.1: spacing {alive:?}"
    );
    assert!(
        evs.iter()
            .any(|e| e.rx_tsc() >= to && matches!(e, MdEvent::Bbo(b) if b.h.inst == X)),
        "TK-011.1: no Bbo of X after the window"
    );

    // TK-011.2: the connection stops answering.
    let evs = tk011_stream(QuietPolicy::Silent {
        from_ms: 1_000,
        to_ms: 12_000,
    });
    assert!(
        !evs.iter().any(|e| in_window(e) && e.conn() == BBO_CONN),
        "TK-011.2: BBO connection event inside the window"
    );
    let after: Vec<&MdEvent> = evs
        .iter()
        .filter(|e| e.rx_tsc() >= to && e.conn() == BBO_CONN)
        .collect();
    let expect = [
        (ConnStateKind::Degraded, InstId(0), REASON_BBO_SILENT),
        (ConnStateKind::Down, InstId(0), 0),
        (ConnStateKind::Up, InstId(0), 0),
        (ConnStateKind::Up, X, 0),
        (ConnStateKind::Resync, X, REASON_BBO_REFRESH),
    ];
    assert!(
        after.len() > expect.len(),
        "TK-011.2: stream ends before recovery"
    );
    for (k, (state, inst, reason)) in expect.iter().enumerate() {
        match after[k] {
            MdEvent::Conn(c) => {
                assert_eq!(
                    (c.state, c.inst, c.reason, c.rx_tsc),
                    (*state, *inst, *reason, to),
                    "TK-011.2 #{k}"
                )
            }
            e => panic!("TK-011.2 #{k}: expected ConnState, got {e:?}"),
        }
    }
    let first_bbo = after.iter().position(|e| matches!(e, MdEvent::Bbo(_)));
    assert!(
        first_bbo.is_some_and(|p| p >= expect.len()),
        "TK-011.2: Bbo before the resync sequence"
    );
}

fn tk021_bytes(seed_v: u64) -> Vec<u8> {
    let insts = [InstId(1), InstId(2), InstId(3)];
    let stream = seed::sample(
        seed_v,
        &market_stream(VenueId(2), &insts, 2000, GapPolicy::Skips { max_skip: 5 }),
    );
    let mut out = Vec::new();
    for e in &stream {
        e.write_bytes(&mut out);
    }
    let p = Perturb {
        reorder_window: 4,
        dup_p: 0.1,
        drop_p: 0.1,
        delay_ns: 1000,
    };
    let seq: Vec<u32> = (0..500).collect();
    for d in seed::sample(seed::derive(seed_v, "perturb"), &perturb(seq, p)) {
        out.extend_from_slice(&d.item.to_le_bytes());
        out.extend_from_slice(&(d.index as u64).to_le_bytes());
        out.push(u8::from(d.dup));
        out.extend_from_slice(&d.delay_ns.to_le_bytes());
    }
    out
}

const TK021_ENV: &str = "HFT_TESTKIT_TK021_OUT";
const TK021_SEED: u64 = 0x0021_5eed;

#[test]
fn tk_021_generators_deterministic() {
    if let Ok(path) = std::env::var(TK021_ENV) {
        std::fs::write(path, tk021_bytes(TK021_SEED)).expect("write child output");
        return;
    }
    let exe = std::env::current_exe().expect("test binary path");
    let dir = std::path::Path::new(env!("CARGO_TARGET_TMPDIR")).join("tk_021");
    std::fs::create_dir_all(&dir).expect("tmp dir");
    let mut outs = Vec::new();
    for k in 0..2 {
        let path = dir.join(format!("proc{k}.bin"));
        let _ = std::fs::remove_file(&path);
        let st = std::process::Command::new(&exe)
            .args(["--exact", "tk_021_generators_deterministic", "--test-threads=1"])
            .env(TK021_ENV, &path)
            .status()
            .expect("spawn child test process");
        assert!(st.success(), "child process {k} failed: {st}");
        outs.push(std::fs::read(&path).expect("child output"));
    }
    assert!(!outs[0].is_empty());
    assert!(outs[0] == outs[1], "TK-021: the two processes' outputs differ");
}
