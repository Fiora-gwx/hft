//! Configuration checks and defaults (JNL-011, JNL-012).

mod common;

use common::harness::*;
use hft_journal::*;

fn try_open(f: impl FnOnce(&mut JournalConfig)) -> Result<(), OpenErr> {
    let mut cfg = mem_c0();
    f(&mut cfg);
    let mut h = H::bare(cfg, false);
    h.open().map(|_| ())
}

#[test]
fn jnl_011_config_validation() {
    type Row = (
        &'static str,
        Box<dyn FnOnce(&mut JournalConfig)>,
        Result<(), OpenErr>,
    );
    let rows: Vec<Row> = vec![
        ("JNL-011.1", Box::new(|_| {}), Ok(())),
        (
            "JNL-011.2",
            Box::new(|c| (c.size_idle_pct, c.size_valve_pct) = (89, 90)),
            Ok(()),
        ),
        (
            "JNL-011.3",
            Box::new(|c| (c.size_idle_pct, c.size_valve_pct) = (90, 90)),
            Err(OpenErr::BadConfig),
        ),
        (
            "JNL-011.4",
            Box::new(|c| (c.size_idle_pct, c.size_valve_pct) = (91, 90)),
            Err(OpenErr::BadConfig),
        ),
        (
            "JNL-011.5",
            Box::new(|c| c.size_idle_pct = 0),
            Err(OpenErr::BadConfig),
        ),
        ("JNL-011.6", Box::new(|c| c.size_idle_pct = 1), Ok(())),
        ("JNL-011.7", Box::new(|c| c.size_valve_pct = 99), Ok(())),
        (
            "JNL-011.8",
            Box::new(|c| c.size_valve_pct = 100),
            Err(OpenErr::BadConfig),
        ),
        (
            "JNL-011.9",
            Box::new(|c| c.segment_bytes = 1_048_577),
            Err(OpenErr::BadConfig),
        ),
        (
            "JNL-011.10",
            Box::new(|c| c.segment_bytes = 4096),
            Err(OpenErr::BadConfig),
        ),
        ("JNL-011.11", Box::new(|c| c.segment_bytes = 8192), Ok(())),
        (
            "JNL-011.12",
            Box::new(|c| (c.page, c.segment_bytes) = (PageSize::Huge2M, 3 << 20)),
            Err(OpenErr::BadConfig),
        ),
        (
            "JNL-011.14",
            Box::new(|c| c.name = JournalName("a".repeat(33))),
            Err(OpenErr::BadConfig),
        ),
    ];
    for (row, f, want) in rows {
        assert_eq!(try_open(f), want, "{row}");
    }
    for s in ["Prod", "test", ""] {
        assert!(s.parse::<InstanceName>().is_err(), "JNL-011.13 {s:?}");
    }
    assert_eq!(
        "prod".parse::<InstanceName>(),
        Ok(InstanceName::Prod),
        "JNL-011.13"
    );
    assert_eq!("sim".parse::<InstanceName>(), Ok(InstanceName::Sim), "JNL-011.13");
}

#[test]
fn jnl_012_defaults() {
    assert_eq!(
        (DEFAULT_SIZE_IDLE_PCT, DEFAULT_SIZE_VALVE_PCT),
        (75, 90),
        "JNL-012.1"
    );
    assert_eq!(
        PersistConfig::default(),
        PersistConfig {
            chunk_bytes: 4_194_304,
            sync_bytes: 1_048_576,
            sync_interval_ms: 20
        },
        "JNL-012.2"
    );
    let c = ReaderConfig::new("h".into(), "c".into(), InstanceName::Prod, "seq".into());
    assert_eq!(
        (c.reassembly_cap, c.tail_poll_us, c.start, c.end, c.on_gap),
        (268_435_456, 50, StartAt::Oldest, EndAt::Follow, OnGap::Error),
        "JNL-012.3"
    );
    let k = |j| {
        let r = ColdRetention::for_kind(j);
        (r.keep_days, r.require_uploaded)
    };
    assert_eq!(
        [k(JournalKind::Seq), k(JournalKind::Chain), k(JournalKind::Raw)],
        [(7, true), (7, true), (3, true)],
        "JNL-012.4"
    );
    assert_eq!(RETIRE_GRACE_MS, 2000, "JNL-012.5");
}
