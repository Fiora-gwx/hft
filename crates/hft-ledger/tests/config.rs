//! LED-001–LED-004, LED-269: configuration defaults, capacity constants, allocation-free entries,
//! ledger-section validation.

mod common;

use common::*;
use hft_event::ExternalCause;
use hft_ledger::{
    FixedVec, FundingReconItem, LEDGER_ROW_VERSION, LEDGER_VALIDATOR, Ledger, LedgerConfig,
    LedgerConfigError, MAX_ASSETS_PER_VENUE, MAX_INV_PER_VENUE, PerpVal, SpotVal,
};
use hft_refdata::{EcfBuilder, EngineConfig, EngineSection, LoadErrCode, RefData};
use hft_testkit::CountingAlloc;
use hft_types::{HostClass, INST_NONE, InstId, Notional, Px, Rate};
use zerocopy::IntoBytes;

#[global_allocator]
static ALLOC: CountingAlloc = CountingAlloc;

#[test]
fn led_001_config_defaults() {
    let c = LedgerConfig::default();
    let names = |v: &FixedVec<hft_ledger::AssetName, 8>| -> Vec<String> {
        v.iter()
            .map(|n| {
                String::from_utf8_lossy(n.as_bytes())
                    .trim_end_matches('\0')
                    .to_string()
            })
            .collect()
    };
    assert_eq!(names(&c.par_assets), ["USDT"], "LED-001.1");
    assert_eq!(
        names(&c.stable_par_fallback),
        ["USDC", "USD1", "FDUSD"],
        "LED-001.2"
    );
    assert_eq!(c.rate_max_age_ms, 60_000, "LED-001.3");
    assert_eq!(c.valuation.spot, SpotVal::Bid, "LED-001.4");
    assert_eq!(c.valuation.perp, PerpVal::Mark, "LED-001.4");
    assert!(!c.valuation.include_baseline, "LED-001.4");
    assert!(c.valuation.include_unassigned, "LED-001.4");
    assert_eq!(c.round_dust_notional, Notional(5_000_000), "LED-001.5");
    assert_eq!(c.balance_check_quiet_ms, 2_000, "LED-001.6");
    assert_eq!(c.funding_tol_abs, Notional(50_000), "LED-001.7");
    assert_eq!(c.funding_tol_rel, Rate(5_000_000), "LED-001.7");
    assert_eq!(c.funding_recon_lag_ms, 600_000, "LED-001.8");
    assert_eq!(c.funding_recon_window_ms, 172_800_000, "LED-001.9");
    assert_eq!(c.funding_hist_cap, 4_096, "LED-001.10");
    assert_eq!(c.funding_push_seen_cap, 64, "LED-001.11");
    assert_eq!(c.ext_dedup_cap, 1_024, "LED-001.12");
    assert_eq!(c.ref_inst_for(rd_lf(), USDC), USDC_S1, "LED-001.13");
}

#[test]
fn led_002_capacity_constants() {
    assert_eq!(MAX_ASSETS_PER_VENUE, 64);
    assert_eq!(MAX_INV_PER_VENUE, 128);
    // Neither the config nor its ECF1 row carries a capacity field of the same name.
    let dbg = format!(
        "{:?} {:?}",
        LedgerConfig::default(),
        LedgerConfig::default().to_row()
    );
    assert!(!dbg.contains("max_assets") && !dbg.contains("max_inv"), "{dbg}");
}

/// `S1` lists USDT, BTC, BNB and USDC; `extra` more assets bring it to `4 + extra`.
fn new_with_s1_assets(n_assets: u16) -> Result<Ledger, LedgerConfigError> {
    let rd = rd_with(&RdOpts {
        s1_extra_assets: n_assets - 4,
        ..RdOpts::default()
    });
    Ledger::new(rd, &cfg_lf(), &caps_lf())
}

#[test]
fn led_003_assets_per_venue_limit() {
    assert!(new_with_s1_assets(63).is_ok(), "LED-003.1");
    assert!(new_with_s1_assets(64).is_ok(), "LED-003.2");
    assert_eq!(
        new_with_s1_assets(65).err(),
        Some(LedgerConfigError::TooManyAssets { venue: S1, n: 65 }),
        "LED-003.3"
    );
}

/// Small deterministic generator (no allocation).
struct Lcg(u64);
impl Lcg {
    fn next(&mut self) -> u64 {
        self.0 = self
            .0
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        self.0 >> 33
    }
    fn range(&mut self, lo: i64, hi: i64) -> i64 {
        lo + (self.next() % (hi - lo + 1) as u64) as i64
    }
}

#[test]
fn led_004_zero_alloc_after_new() {
    assert!(CountingAlloc::is_installed());
    // Small rings so the dedup ring, the funding record ring and the push-seen ring all wrap.
    let mut cfg = cfg_lf();
    cfg.funding_hist_cap = 64;
    cfg.ext_dedup_cap = 64;
    cfg.funding_push_seen_cap = 16;
    let mut l = ledger_with(rd_lf(), &cfg);
    boot_a1(&mut l, 0);
    let mut l2 = ledger_with(rd_funding(hft_refdata::FundingSettleKind::BalanceOnlyPush), &cfg);
    boot_a1(&mut l2, 0);
    let items = margin_items(&[(BTC_P2, px("80000"), 2)]);
    let hdr = margin_hdr(A1, P2, "50000", "500", "49000", "0.01", 1);
    let mut touched = FixedVec::new();
    let mut out = FixedVec::new();
    let mut r = Lcg(7);
    let mut per: Vec<(&str, u64)> = Vec::with_capacity(16);
    for name in [
        "apply_fill",
        "apply_fee",
        "apply_external",
        "apply_funding",
        "apply_funding_push",
        "reconcile_funding",
        "funding_window_sweep",
        "on_ref_px",
        "on_mark",
        "on_margin",
        "equity",
    ] {
        per.push((name, 0));
    }
    macro_rules! m {
        ($k:expr, $e:expr) => {{
            let sc = CountingAlloc::scope();
            let r = $e;
            per[$k].1 += sc.allocs();
            r
        }};
    }
    let fl0 = f(Buy, BTC_S1, "0.001", "60000", "0", USDT, G1, 0);
    let fp0 = f(Buy, BTC_P2, "0.001", "60000", "0.01", USDT, G1, 1);
    let fee0 = fee_delta(Buy, BTC_P2, G1, "0.01", USDT);
    let e0 = ext(ExternalCause::Manual, Buy, BTC_P2, "0.001", "60000");
    let push0 = push(BTC_P2, "1.5", D);
    let synth0 = fs(BTC_P2, "-2", D, 1);
    let wpush0 = push(INST_NONE, "-2.01", D);
    for k in 0..10_000i64 {
        let t = s(1) + k * 1_000_000;
        let p = Px(r.range(300_000_000, 900_000_000));
        let side = if r.next().is_multiple_of(2) { Buy } else { Sell };
        let st = D + (k % 200) * 8 * H;
        let mut fl = fl0;
        fl.side = side;
        fl.px = p;
        m!(0, l.apply_fill(&fl, t));
        let mut fp = fp0;
        fp.side = side;
        fp.px = p;
        m!(0, l.apply_fill(&fp, t));
        let mut fe = fee0;
        fe.side = side;
        m!(1, l.apply_fee(&fe, t));
        let mut e = e0;
        e.side = side;
        e.trade_id = 1 + k as u64;
        m!(2, l.apply_external(&e, false, t));
        let mut pu = push0;
        pu.settle_ts_ns = st;
        m!(3, l.apply_funding(&pu, t));
        let mut sy = synth0;
        sy.settle_ts_ns = st;
        m!(3, l2.apply_funding(&sy, t));
        let mut wp = wpush0;
        wp.settle_ts_ns = st;
        m!(4, l2.apply_funding_push(&wp, t));
        let it = FundingReconItem {
            account: A1,
            venue: P2,
            inst: BTC_P2,
            settle_ts_ns: st,
            exch_amount: Notional(1_500_000 + r.range(-100, 100)),
            engine_amount: Notional(1_500_000),
            report_id: k as u64,
        };
        m!(5, l.reconcile_funding(&it, st + 2 * H, t));
        out.clear();
        m!(6, l.funding_window_sweep(A1, P2, st + 2 * H, t, &mut out));
        m!(7, l.on_ref_px(BTC_S1, p, Px(p.0 + 100), t));
        touched.clear();
        m!(8, l.on_mark(BTC_P2, p, t, &mut touched));
        m!(
            9,
            l.on_margin(&hft_ledger::MarginReportView { hdr, items: &items }, t)
        );
        m!(10, l.equity(A1));
        l.clear_notes();
        l2.clear_notes();
    }
    let bad: Vec<_> = per.iter().filter(|(_, n)| *n != 0).collect();
    assert!(bad.is_empty(), "allocating entries: {bad:?}");
}

/// An `EngineConfig` whose ledger section is `cfg_lf()` with `ref_inst[USDC] = ref_usdc`.
fn engine_with_ref(ref_usdc: InstId) -> EngineConfig {
    let mut c = cfg_lf();
    c.ref_inst.push((USDC, ref_usdc));
    let row = c.to_row();
    let mut b = EcfBuilder::new(1, HostClass::C8);
    b.section(
        EngineSection::Ledger,
        LEDGER_ROW_VERSION,
        512,
        1,
        row.as_bytes().to_vec(),
    );
    match EngineConfig::load(&b.finish(), None) {
        Ok(e) => e,
        Err(e) => panic!("ECF1 rejected: {e:?}"),
    }
}

#[test]
fn led_269_ref_inst_validated() {
    let rd: &RefData = rd_lf();
    let check = |i: InstId| {
        engine_with_ref(i)
            .validate(rd, &[LEDGER_VALIDATOR])
            .map_err(|r| r.errors.iter().map(|e| e.code).collect::<Vec<_>>())
    };
    assert_eq!(check(USDC_S1), Ok(()), "LED-269.1");
    assert_eq!(
        check(InstId(999)),
        Err(vec![LoadErrCode::RefInstNotSubscribed]),
        "LED-269.2"
    );
    assert_eq!(
        check(HALTED),
        Err(vec![LoadErrCode::RefInstNotSubscribed]),
        "LED-269.3"
    );
}
