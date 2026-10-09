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
    // Full test name inside whichever test binary this module is compiled into.
    let test_name = match module_path!().split_once("::") {
        Some((_, m)) => format!("{m}::tk_021_generators_deterministic"),
        None => "tk_021_generators_deterministic".to_string(),
    };
    let mut outs = Vec::new();
    for k in 0..2 {
        let path = dir.join(format!("proc{k}.bin"));
        let _ = std::fs::remove_file(&path);
        let st = std::process::Command::new(&exe)
            .args(["--exact", &test_name, "--test-threads=1"])
            .env(TK021_ENV, &path)
            .status()
            .expect("spawn child test process");
        assert!(st.success(), "child process {k} failed: {st}");
        outs.push(std::fs::read(&path).expect("child output"));
    }
    assert!(!outs[0].is_empty());
    assert!(outs[0] == outs[1], "TK-021: the two processes' outputs differ");
}

// ------------------------------------------------------------------------------------- M2 generators

mod m2 {
    use std::collections::BTreeSet;

    use hft_event::{ExternalCause, OrderEventKind};
    use hft_testkit::r#gen::ctl::{CmdVariant, control_cmds};
    use hft_testkit::r#gen::external::{
        AcctCfgShape, ExternalShape, MISMATCH_AUTO_BORROW, account_config, external_fills,
    };
    use hft_testkit::r#gen::fees::{FeeAsset, FeeRule, FundingShape, fill_with_fee, funding_flows};
    use hft_testkit::r#gen::margin::{MarginThresholds, MoveShape, margin_path, move_px, one_sided_move};
    use hft_testkit::r#gen::orders::{Shape, VenueRules, is_terminal, order_lifecycle};
    use hft_testkit::{Perturb, perturb, seed};
    use hft_types::{AccountId, InstId, Side, VenueId};
    use proptest::prelude::*;
    use proptest::strategy::ValueTree;

    #[test]
    fn tk_012_order_lifecycle_valid() {
        let mut seen = BTreeSet::new();
        for (k, rules) in VenueRules::CORE.iter().enumerate() {
            let mut r = seed::runner(0x12 + k as u64, 10_000);
            for _ in 0..3_334 {
                let l = order_lifecycle(*rules).new_tree(&mut r).expect("tree").current();
                let terminals = l.events.iter().filter(|e| is_terminal(e, l.order_qty)).count();
                assert_eq!(terminals, 1, "{l:?}");
                assert!(l.events.iter().all(|e| e.cum <= l.order_qty), "{l:?}");
                let mut cum = 0;
                for e in &l.events {
                    if matches!(e.what, OrderEventKind::PartialFill | OrderEventKind::Fill) {
                        cum += e.qty;
                        assert_eq!(cum, e.cum, "{l:?}");
                    }
                }
                seen.insert(l.shape);
            }
        }
        for s in [
            Shape::FillBeforeAck,
            Shape::CancelRejectFilled,
            Shape::AmendRace,
            Shape::FeeOnly,
        ] {
            assert!(seen.contains(&s), "{s:?} never generated");
        }
    }

    proptest! {
        #![proptest_config(ProptestConfig { cases: 1_000, failure_persistence: None, ..ProptestConfig::default() })]
        #[test]
        fn tk_013_perturb_bounds(
            seq in proptest::collection::vec(any::<u16>(), 0..=500),
            win in 0usize..=16,
            dup in 0u32..=500,
            drop in 0u32..=500,
            s in any::<u64>(),
        ) {
            let p = Perturb { reorder_window: win, dup_p: f64::from(dup) / 1_000.0, drop_p: f64::from(drop) / 1_000.0, delay_ns: 0 };
            let out = seed::sample(s, &perturb(seq.clone(), p));
            let idx: Vec<usize> = out.iter().filter(|d| !d.dup).map(|d| d.index).collect();
            for d in &out {
                prop_assert_eq!(&seq[d.index], &d.item);
            }
            let mut sorted = idx.clone();
            sorted.sort_unstable();
            sorted.dedup();
            prop_assert_eq!(sorted.len(), idx.len(), "an index twice without the dup flag");
            // Window: an item never overtakes one more than `win` places after it.
            for (k, i) in idx.iter().enumerate() {
                for j in &idx[k + 1..] {
                    prop_assert!(*j + win >= *i, "item {} delivered before {} beyond the window", j, i);
                }
            }
            let id = seed::sample(s, &perturb(seq.clone(), Perturb::NONE));
            prop_assert_eq!(id.iter().map(|d| d.item).collect::<Vec<_>>(), seq);
            prop_assert!(id.iter().all(|d| !d.dup && d.delay_ns == 0));
        }
    }

    #[test]
    fn tk_014_control_cmds_allowed() {
        let mut r = seed::runner(0x14, 1);
        for mask in 1u32..(1 << 10) {
            if mask % 37 != 0 {
                continue;
            }
            let allowed: Vec<CmdVariant> = CmdVariant::ALL
                .iter()
                .enumerate()
                .filter(|(i, _)| mask & (1 << (i % 10)) != 0)
                .map(|(_, v)| *v)
                .collect();
            let cmds = control_cmds(allowed.clone())
                .new_tree(&mut r)
                .expect("tree")
                .current();
            for c in &cmds {
                assert!(allowed.contains(&c.variant));
                assert!(
                    hft_event::ControlCmd::decode(&c.input.body).is_ok(),
                    "{:?} decodes",
                    c.variant
                );
            }
        }
        let mut seen = BTreeSet::new();
        let s = control_cmds(CmdVariant::ALL.to_vec());
        let mut r = seed::runner(0x1414, 10_000);
        for _ in 0..10_000 {
            for c in s.new_tree(&mut r).expect("tree").current() {
                seen.insert(c.variant);
            }
        }
        for v in [
            CmdVariant::KillHardAccount,
            CmdVariant::ViewerEmergency,
            CmdVariant::KillFlattenGateClosed,
            CmdVariant::KillFlattenDraining,
            CmdVariant::SameVenueSetParams,
            CmdVariant::WriteOffResidualQuiet,
            CmdVariant::SmokeOrder,
            CmdVariant::InjectWarmup,
            CmdVariant::SimReset,
            CmdVariant::SimTransfer,
        ] {
            assert!(seen.contains(&v), "{v:?}");
        }
    }

    #[test]
    fn tk_015_fee_asset_rules() {
        // Gross 10 lots (lot 1 000 raw), fee rate 0.1 % (1 000 ppm), px 60 000.00 (raw, exponent −4).
        let (gross, px) = (10_000, 600_000_000);
        let b = fill_with_fee(FeeRule::Base, gross, px, 1_000, 1_000_000);
        assert_eq!((b[0].fee_asset, b[0].fee), (FeeAsset::Base, 10), "TK-015.1");
        let q = fill_with_fee(FeeRule::Quote, gross, px, 1_000, 1_000_000);
        assert_eq!(q[0].fee_asset, FeeAsset::Quote, "TK-015.2");
        let l = fill_with_fee(FeeRule::LateQuote, gross, px, 1_000, 1_000_000);
        assert_eq!(l.len(), 2, "TK-015.3");
        assert_eq!((l[0].qty, l[0].fee), (gross, 0));
        assert_eq!(l[1].qty, 0);
        assert_eq!(
            l[0].fee + l[1].fee,
            gross * px / 1_000_000 / 1_000,
            "TK-015.3 fee sum"
        );
    }

    #[test]
    fn tk_016_margin_path_coverage() {
        let t = MarginThresholds::TK016;
        let mut r = seed::runner(0x16, 1_000);
        let (mut mis, mut ratio, mut stale) = (0, 0, 0);
        let mut crossed = [false; 6];
        for _ in 0..1_000 {
            let p = margin_path(t).new_tree(&mut r).expect("tree").current();
            assert!((100..=2_000).contains(&p.len()));
            for (j, th) in [t.block_open, t.reduce, t.force].iter().enumerate() {
                for (k, side) in [th - t.hyst, th + t.hyst].iter().enumerate() {
                    if p.iter().any(|s| s.liq_dist < *side) && p.iter().any(|s| s.liq_dist > *side) {
                        crossed[j * 2 + k] = true;
                    }
                }
            }
            mis += usize::from(p.iter().any(|s| s.liq_dist != s.ledger_liq_dist));
            ratio += usize::from(
                p.iter()
                    .any(|s| s.ratio >= 800_000_000 && s.liq_dist > t.block_open),
            );
            stale += usize::from(p.iter().any(|s| s.stale));
        }
        assert!(crossed.iter().all(|c| *c), "{crossed:?}");
        assert!(mis > 0 && ratio > 0 && stale > 0);
    }

    #[test]
    fn tk_017_one_sided_move() {
        let (x, h) = (1_000_000, 3_600_000u64); // 100.00 (raw, exponent −4)
        assert_eq!(
            move_px(x, 3_000, h, MoveShape::Linear, h),
            1_300_000,
            "TK-017.1 end"
        );
        assert_eq!(
            move_px(x, 3_000, h, MoveShape::Linear, h / 2),
            1_150_000,
            "TK-017.1 half"
        );
        let pts = one_sided_move(x, 3_000, h, MoveShape::Linear, 60_000);
        assert!(pts.windows(2).all(|w| w[1].1 >= w[0].1), "one direction");
        let s = one_sided_move(x, 3_000, h, MoveShape::Steps { every_ms: 600_000 }, 1_000);
        let jumps: Vec<i64> = s
            .windows(2)
            .map(|w| w[1].1 - w[0].1)
            .filter(|d| *d != 0)
            .collect();
        assert_eq!(jumps, vec![50_000; 6], "TK-017.2");
        let g = MoveShape::Gap {
            at_ms: 1_000_000,
            gap_bp: 1_000,
        };
        let jump = move_px(x, 3_000, h, g, 1_000_000) - move_px(x, 3_000, h, g, 999_999);
        let lin = move_px(x, 2_000, h, MoveShape::Linear, 1_000_000)
            - move_px(x, 2_000, h, MoveShape::Linear, 999_999);
        assert_eq!(jump - lin, 100_000, "TK-017.3 jump 10.00");
        assert_eq!(move_px(x, 3_000, h, g, h), 1_300_000, "TK-017.3 end");
        assert_eq!(move_px(x, -3_000, h, MoveShape::Linear, h), 700_000, "TK-017.4");
    }

    #[test]
    fn tk_018_external_fill_shapes() {
        let pos = 10 * 1_000;
        let l = external_fills(ExternalShape::LiquidationPushed, pos, "autoclose-", 7);
        assert_eq!(l.truth_delta, -pos, "TK-018.1");
        assert_eq!(l.pushes.len(), 1);
        assert!(
            l.pushes[0].client_id.starts_with("autoclose-")
                && l.pushes[0].cause == ExternalCause::Liquidation
        );
        assert_eq!(l.pushes[0].side, Side::Sell);
        let g = external_fills(ExternalShape::AdlGap, pos, "autoclose-", 8);
        assert_eq!((g.truth_delta, g.pushes.len()), (-pos, 0), "TK-018.2");
        let r = external_fills(ExternalShape::AdlGapReplay, pos, "autoclose-", 9);
        assert_eq!(r.truth_delta, -pos, "TK-018.3");
        assert!(r.pushes.len() == 1 && r.pushes[0].after_reconnect && r.pushes[0].trade_id == 9);
    }

    #[test]
    fn tk_019_account_config_shapes() {
        let (a, v, i) = (AccountId(1), VenueId(2), InstId(2));
        let c = account_config(AcctCfgShape::Consistent, a, v, i, 5, 2);
        assert!(c.iter().all(|(h, _)| h.mismatches == 0), "TK-019.1");
        let s = account_config(AcctCfgShape::StartupMismatch, a, v, i, 5, 2);
        assert_ne!(s[0].0.mismatches, 0, "TK-019.2");
        let l = account_config(AcctCfgShape::LeverageChange, a, v, i, 5, 2);
        assert_eq!(l[0].1[0].leverage, l[1].1[0].leverage);
        assert_ne!(l[1].1[0].leverage, l[2].1[0].leverage, "TK-019.3");
        assert!(l.iter().all(|(h, _)| h.mismatches == 0));
        let b = account_config(AcctCfgShape::AutoBorrowOn, a, v, i, 5, 2);
        assert_eq!(b[1].0.mismatches & MISMATCH_AUTO_BORROW, 0);
        assert_ne!(b[2].0.mismatches & MISMATCH_AUTO_BORROW, 0, "TK-019.4");
    }

    #[test]
    fn tk_020_funding_flows_shapes() {
        let mut seen = BTreeSet::new();
        let mut r = seed::runner(0x20, 1_000);
        for _ in 0..1_000 {
            let v = funding_flows(10_000_000, 600_000)
                .new_tree(&mut r)
                .expect("tree")
                .current();
            assert!((1..=50).contains(&v.len()));
            for p in v {
                seen.insert(p.shape);
                match p.shape {
                    FundingShape::Paired => assert_eq!(p.exchange, p.local),
                    FundingShape::WithinLag => assert!(10_000_000 - p.settle_ms < 600_000),
                    _ => {}
                }
            }
        }
        assert_eq!(seen.len(), 5);
    }
}
