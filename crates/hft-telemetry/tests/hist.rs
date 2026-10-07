//! Cardinality control and datasets (TEL-022, TEL-023).

mod common;

use common::*;
use hft_telemetry::{
    AcctKind, CounterName, Dataset, EventWeightedCfg, HistKey, Stage, TelemetryConfig, TsSrc, WindowReport,
    event_weighted,
};
use hft_types::AccountId;

fn tick_keys(w: &WindowReport) -> Vec<(HistKey, u64)> {
    w.hist
        .iter()
        .filter(|(k, _)| k.stage == Stage::TickToTrade)
        .map(|(k, q)| (*k, q.count))
        .collect()
}

#[test]
fn tel_022_series_overflow() {
    // Four samples whose keys differ only in `strategy`, each producing one segment (`decide`:
    // Timer trigger, Cancel action, no `sent_tsc`).
    let c = TelemetryConfig {
        max_series: 3,
        ..cfg()
    };
    let mut f = Feed::new(&c);
    f.tel.window(0);
    f.clock(&std_a());
    for strat in 1..=4u8 {
        let mut s = std_sample();
        s.trigger_kind = hft_event::EventKind::TIMER.0;
        s.trigger_seq = f.rec(s.proc_tsc.0, 0, &timer());
        s.strategy = hft_types::StrategyId(strat);
        s.action = 1;
        set_src(&mut s, 6, 0);
        s.sent_tsc = hft_types::Tsc(0);
        f.sample(&s);
    }
    f.clock(&std_b());
    let w = f.tel.window(10_000_000_000).unwrap();
    let regular: Vec<&HistKey> = w.hist.iter().map(|(k, _)| k).filter(|k| !k.other).collect();
    let other: Vec<_> = w.hist.iter().filter(|(k, _)| k.other).collect();
    assert_eq!(regular.len(), 3, "{:?}", w.hist);
    assert_eq!(other.len(), 1);
    assert_eq!(other[0].1.count, 1);
    let overflow: u64 = w
        .counters
        .iter()
        .filter(|(k, _)| k.name == CounterName::SeriesOverflow)
        .map(|(_, n)| n)
        .sum();
    assert_eq!(overflow, 1);
    // No inst dimension in the key.
    let v = serde_json::to_value(regular[0]).unwrap();
    assert!(v.as_object().unwrap().keys().all(|k| k != "inst"), "{v}");
}

#[test]
fn tel_023_datasets_separate() {
    // TEL-023.1: HW and SW.
    let mut f = Feed::new(&cfg());
    f.tel.window(0);
    f.std_with(std_sample());
    let mut sw = std_sample();
    set_src(&mut sw, 2, 2);
    f.std_with(sw);
    let w = f.tel.window(10_000_000_000).unwrap();
    let k = tick_keys(&w);
    assert_eq!(k.len(), 2, "TEL-023.1 {k:?}");
    assert!(
        k.iter().any(|(k, n)| k.ts_src == TsSrc::Hw && *n == 1),
        "TEL-023.1"
    );
    assert!(
        k.iter().any(|(k, n)| k.ts_src == TsSrc::Sw && *n == 1),
        "TEL-023.1"
    );

    // TEL-023.2: live vs verification replay.
    let run = |ds: Dataset| {
        let mut f = Feed::new(&TelemetryConfig { dataset: ds, ..cfg() });
        f.tel.window(0);
        f.std_with(std_sample());
        tick_keys(&f.tel.window(10_000_000_000).unwrap())
    };
    let (live, replay) = (run(Dataset::Live), run(Dataset::VerifyReplay));
    assert_eq!((live.len(), replay.len()), (1, 1), "TEL-023.2");
    assert_ne!(live[0].0, replay[0].0, "TEL-023.2");
    assert_eq!(
        (live[0].0.dataset, replay[0].0.dataset),
        (Dataset::Live, Dataset::VerifyReplay)
    );

    // TEL-023.3: live vs event-weighted offline statistics.
    let mut j = Jw::new([1; 32], 1);
    j.clock(&std_a());
    j.input(
        T0 + 415_000,
        hft_event::RecordFlags::HW_TS.0,
        &bbo(mhdr(P0 + 400_000, P0 + 405_000, T0 + 412_000, 0)),
    );
    j.clock(&std_b());
    let ew = event_weighted(
        &mut j.reader(),
        &rd(),
        &EventWeightedCfg {
            from_seq: 0,
            to_seq: u64::MAX,
            cal_max_err_ns: 1_000,
            burst_gap_ns: 50_000,
            max_series: 1_000,
        },
    )
    .unwrap();
    let ew_kernel: Vec<_> = ew[0]
        .hist
        .iter()
        .filter(|(k, _)| k.stage == Stage::KernelRx)
        .collect();
    assert_eq!(ew_kernel.len(), 1, "TEL-023.3");
    assert_eq!(ew_kernel[0].0.dataset(), Dataset::EventWeighted, "TEL-023.3");
    assert_ne!(ew_kernel[0].0.dataset(), live[0].0.dataset, "TEL-023.3");

    // TEL-023.4: Live vs Sim accounts.
    let mut f = Feed::new(&cfg());
    f.tel.window(0);
    f.std_with(std_sample());
    let mut sim = std_sample();
    sim.account = AccountId(2);
    f.std_with(sim);
    let k = tick_keys(&f.tel.window(10_000_000_000).unwrap());
    assert_eq!(k.len(), 2, "TEL-023.4");
    assert!(
        k.iter().any(|(k, n)| k.account_kind == AcctKind::Live && *n == 1),
        "TEL-023.4"
    );
    assert!(
        k.iter().any(|(k, n)| k.account_kind == AcctKind::Sim && *n == 1),
        "TEL-023.4"
    );
}
