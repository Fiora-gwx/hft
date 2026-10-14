mod support;
#[test]
fn core_012_check_rejects_bad_hash_and_inline_credentials() {
    let d = tempfile::tempdir().unwrap();
    let path = support::write(d.path(), &d.path().join("hot"), None);
    let loaded = hft_core::config::load(&path, true).unwrap();
    assert_eq!(loaded.configs.params.len(), 1);
    assert!(!loaded.setup.role_prod);
    let io = hft_testkit::seq_rig::RigIo::<hft_core::Strategies> {
        j: hft_sequencer::MemJournal::new(loaded.setup.clock),
        tsc: 1,
        rings: Default::default(),
        n_venues: 1,
        out: vec![],
        mailbox: vec![],
        kills: vec![],
        calib: hft_rt::CalibPair {
            tsc: hft_types::Tsc(0),
            realtime_ns: 0,
            ..Default::default()
        },
        hz: loaded.setup.clock.tsc_hz_milli,
    };
    let heartbeat = Box::leak(Box::new(hft_rt::Heartbeat::new()));
    let mut seq = hft_sequencer::Sequencer::boot(
        hft_sequencer::BootSource::Empty {
            configs: loaded.configs,
        },
        &loaded.setup,
        hft_sequencer::driver::LiveDriver::new(
            io,
            hft_sequencer::driver::live::LiveCfg {
                feed_stage_cap: 1024,
                ..Default::default()
            },
        ),
        heartbeat,
    )
    .unwrap();
    assert!(!matches!(seq.step(), hft_sequencer::StepOutcome::Fatal(_)));
    let original = std::fs::read_to_string(&path).unwrap();
    std::fs::write(d.path().join("refdata.bin"), b"corrupt").unwrap();
    let errors = hft_core::config::load(&path, true).unwrap_err();
    assert!(errors.iter().any(|e| e.code == "BlobHash"));
    std::fs::write(
        &path,
        original.replace(
            "max_sub_insts = 1",
            "max_sub_insts = 1\napi_key = 'SECRET_SENTINEL'",
        ),
    )
    .unwrap();
    let errors = hft_core::config::load(&path, true).unwrap_err();
    assert!(errors.iter().any(|e| e.code == "ConfigParse"));
    assert!(!format!("{errors:?}").contains("SECRET_SENTINEL"));
}
#[test]
fn core_045_rejects_live_accounts_in_paper_and_unbuilt_venues() {
    let d = tempfile::tempdir().unwrap();
    let path = support::write(d.path(), &d.path().join("hot"), None);
    assert!(
        hft_core::config::load(&path, false)
            .unwrap_err()
            .iter()
            .any(|e| e.code == "EnvCombo")
    );
    let text = std::fs::read_to_string(&path)
        .unwrap()
        .replace("binance-spot", "bitget-spot");
    std::fs::write(&path, text).unwrap();
    assert!(
        hft_core::config::load(&path, true)
            .unwrap_err()
            .iter()
            .any(|e| e.code == "VenueNotBuilt")
    );
}
#[test]
fn core_002_version_lists_registry() {
    let out = std::process::Command::new(env!("CARGO_BIN_EXE_hft-core"))
        .arg("--version")
        .output()
        .unwrap();
    assert!(out.status.success());
    assert!(String::from_utf8(out.stdout).unwrap().contains("kind_id=5"));
}
#[test]
fn core_012_atomic_control_batch_has_no_partial_publication() {
    use hft_ctl::CtlRing;
    use hft_event::{EventKind, RecordFlags};
    let (tx, rx) = hft_core::runtime::byte_ring(262144).unwrap();
    let mut tx = hft_core::io::AtomicCtl(tx);
    let records = vec![
        (EventKind::NOTE, RecordFlags(0), vec![7; 32]),
        (EventKind::CONTROL_CMD, RecordFlags(0), vec![9; 64]),
    ];
    assert!(tx.write_all(&records));
    let mut rx = hft_core::io::Inputs::new(rx);
    let mut slot = hft_sequencer::InputSlot::with_capacity(262144);
    assert!(rx.pop(hft_types::Tsc(1), &mut slot));
    assert_eq!(slot.kind, EventKind::NOTE);
    assert!(rx.pop(hft_types::Tsc(1), &mut slot));
    assert_eq!(slot.kind, EventKind::CONTROL_CMD);
    assert!(!rx.pending());
    let oversized = vec![(EventKind::NOTE, RecordFlags(0), vec![0; 262144])];
    assert!(!tx.write_all(&oversized));
    assert!(!rx.pending());
}
