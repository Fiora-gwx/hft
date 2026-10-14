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
            .any(|e| e.code
                == if cfg!(feature = "bitget") {
                    "UnknownVenue"
                } else {
                    "VenueNotBuilt"
                })
    );
    #[cfg(feature = "bitget")]
    {
        hft_core::venues::registered("bitget-spot", hft_types::VenueId::BITGET_UTA_SPOT).unwrap();
        hft_core::venues::registered("bitget-usdt-futures", hft_types::VenueId::BITGET_UTA_USDT_FUT).unwrap();
    }
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
        (EventKind::NOTE, RecordFlags(0), vec![9; 64]),
    ];
    assert!(tx.write_all(&records));
    let mut rx = hft_core::io::Inputs::new(rx);
    let mut slot = hft_sequencer::InputSlot::with_capacity(262144);
    assert!(rx.pop(hft_types::Tsc(1), &mut slot));
    assert_eq!(slot.kind, EventKind::NOTE);
    assert!(rx.pop(hft_types::Tsc(1), &mut slot));
    assert_eq!(slot.body(), &[9; 64]);
    assert!(!rx.pending());
    let oversized = vec![(EventKind::NOTE, RecordFlags(0), vec![0; 262144])];
    assert!(!tx.write_all(&oversized));
    assert!(!rx.pending());
    assert!(hft_core::io::write_record(
        &mut tx.0,
        EventKind::SEND_REPORT,
        &[&[0; 96]]
    ));
    assert!(rx.pop(hft_types::Tsc(2), &mut slot));
    assert_eq!(slot.kind, EventKind::SEND_REPORT);
}

#[test]
fn core_012_control_adapter_reaches_live_sequencer() {
    use hft_core::{
        io,
        runtime::{byte_ring, ring},
    };
    use hft_sequencer::{BootSource, MemJournal, Sequencer, StepOutcome};
    use zerocopy::FromBytes;
    let d = tempfile::tempdir().unwrap();
    let path = support::write(d.path(), &d.path().join("hot"), None);
    let l = hft_core::config::load(&path, true).unwrap();
    let clock = hft_rt::TscClock::from_hz_milli(
        l.setup.clock.tsc_hz_milli,
        hft_rt::CalibPair {
            tsc: hft_rt::tsc_now(),
            realtime_ns: 0,
            ..Default::default()
        },
    );
    let (ctl_tx, ctl_rx) = byte_ring(1048576).unwrap();
    let (mut hk_tx, hk_rx) = ring(1024).unwrap();
    let (mut sim_tx, sim_rx) = byte_ring(1048576).unwrap();
    let (sim_orders, _sim_rx) = ring(1024).unwrap();
    let (parsed_tx, parsed_rx) = Box::leak(Box::new(hft_rt::Mailbox::new())).split();
    let (retired_tx, retired_rx) = Box::leak(Box::new(hft_rt::Mailbox::new())).split();
    let (bbo_tx, bbo) = ring(1024).unwrap();
    let (_md_tx, md) = ring(1024).unwrap();
    let (_bulk_tx, bulk) = byte_ring(1048576).unwrap();
    let (commands, cmd_rx) = ring(1024).unwrap();
    let paper_rd = hft_refdata::RefData::load(l.rd.blob(), None).unwrap();
    let paper = l.cfg.sim.as_ref().unwrap();
    let mut exchange = hft_core::paper::exchange(paper, &paper_rd, clock.hz_milli).unwrap();
    let mut feed = hft_core::paper::Feed {
        venue: hft_types::VenueId::BINANCE_SPOT,
        out: bbo_tx,
        commands: cmd_rx,
        quotes: paper.quotes.clone(),
        subscribed: vec![false; paper.quotes.len()],
        clock,
        interval_ticks: 0,
        next: 0,
        update: 0,
    };
    let env = io::Io {
        journal: MemJournal::new(l.setup.clock),
        clock,
        ctl: io::Inputs::new(ctl_rx),
        hk: hk_rx,
        sim_reports: io::Inputs::new(sim_rx),
        sim_orders,
        feeds: vec![io::FeedInput {
            venue: hft_types::VenueId::BINANCE_SPOT,
            bbo,
            md,
            bulk,
            commands,
        }],
        gws: vec![],
        parsed: parsed_rx,
        retired: retired_tx,
        retired_pending: None,
        clock_status: hft_sequencer::snapshot::clock_status(clock.base, clock.hz_milli),
        profile: hft_sequencer::snapshot::host_profile(clock.hz_milli),
        orders: 0,
        cancels: 0,
        reports: 0,
    };
    let mut ctl = hft_ctl::Core::<hft_core::Strategies, _>::new(
        hft_ctl::GatewayConfig::new(l.cfg.paths.ctl_socket, l.principals, 256, 1024, 4),
        l.rd,
        io::AtomicCtl(ctl_tx),
        parsed_tx,
        retired_rx,
        |_, _| Err(1),
        hft_sequencer::config_blob::decode_risk_blob,
        vec![],
        clock.tsc_utc(&clock.base),
        Default::default(),
    );
    let hb = Box::leak(Box::new(hft_rt::Heartbeat::new()));
    let mut seq = Sequencer::boot(
        BootSource::Empty { configs: l.configs },
        &l.setup,
        hft_sequencer::driver::LiveDriver::new(env, Default::default()),
        hb,
    )
    .unwrap();
    seq.step();
    exchange.drain_due(hft_rt::tsc_now(), |r| {
        hft_core::paper::reply_parts(&paper_rd, &r.msg, r.bulk, |kind, parts| {
            assert!(io::write_record(&mut sim_tx, kind, parts));
        });
    });
    let (_, timer_id, _) = hft_event::TimerCode::Sys(hft_event::SysTimer::ClockOk)
        .encode()
        .unwrap();
    hk_tx.try_push(hft_event::HkMsg::Sys { timer_id }).unwrap();
    for _ in 0..16 {
        feed.poll(hft_rt::tsc_now()).unwrap();
        assert!(!matches!(seq.step(), StepOutcome::Fatal(_)));
    }
    assert_eq!(
        seq.engine().risk.mode.gate_bits(hft_types::AccountId(1)),
        hft_risk::mode::gate_bits::ALL
    );
    assert!(ctl.on_internal(
        hft_ctl::InternalCmd::Drain { cancel_resting: true },
        hft_rt::tsc_now()
    ));
    for _ in 0..1024 {
        match seq.step() {
            StepOutcome::Done { .. } => return,
            StepOutcome::Fatal(f) => panic!("{f:?}"),
            _ => {}
        }
        std::thread::sleep(std::time::Duration::from_millis(1));
    }
    let results: Vec<_> = seq
        .driver()
        .io
        .journal
        .records()
        .filter(|r| r.kind == hft_event::EventKind::CMD_RESULT)
        .map(|r| hft_event::CmdResult::read_from_bytes(&r.body).ok())
        .collect();
    panic!(
        "Drain did not complete: {results:?}; {:?}",
        seq.driver_mut().taken_log
    );
}
