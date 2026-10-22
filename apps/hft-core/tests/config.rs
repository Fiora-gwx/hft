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
    let template: hft_core::config::Config =
        toml::from_str(include_str!("../config/binance-testnet.toml")).unwrap();
    for venue in template.venues.values() {
        hft_core::venues::registered(&venue.protocol, hft_types::VenueId(venue.id)).unwrap();
    }
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
        let dual: hft_core::config::Config =
            toml::from_str(include_str!("../config/dual-testnet.toml")).unwrap();
        assert_eq!(dual.venues.len(), 2);
        for venue in dual.venues.values() {
            hft_core::venues::registered(&venue.protocol, hft_types::VenueId(venue.id)).unwrap();
        }
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
    use zerocopy::{FromBytes, FromZeros, TryFromBytes};
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
    let (sim_orders, mut sim_rx_orders) = ring(1024).unwrap();
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
        startup: Default::default(),
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
        hft_ctl::GatewayConfig::new(
            l.cfg.paths.ctl_socket,
            hft_ctl::PrincipalTable::from_parts(
                vec![(
                    1,
                    hft_ctl::UidKind::Cli {
                        login: "paper".into(),
                    },
                )],
                vec![(
                    "cli:paper".into(),
                    hft_ctl_proto::RoleSet::from_roles(&[hft_ctl_proto::Role::Admin]),
                )],
            ),
            256,
            1024,
            4,
        ),
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
        hft_core::paper::reply_parts(&paper_rd, 1, &r.msg, r.bulk, |kind, parts| {
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
    let request = hft_ctl_proto::CmdRequest {
        v: hft_ctl_proto::PROTO_VERSION,
        cmd_id: hft_ctl_proto::Uuid::from_bytes([1; 16]),
        issuer: "cli:paper".into(),
        issued_at_ns: 0,
        expires_at_ns: 10_000_000_000,
        account: hft_ctl_proto::AccountSel::Name("paper".into()),
        preview: false,
        confirmed: true,
        expect_state_version: Some(seq.engine().risk.mode.ctl_state_version()),
        view_account: None,
        chat_type: None,
        cmd: hft_ctl_proto::CmdBody::SetMode {
            strategy: Some("paper-probe".into()),
            group: None,
            account: None,
            mode: hft_types::RunMode::Live,
        },
    };
    let reply = ctl.on_request(1, 1, &request.to_json(), hft_rt::tsc_now());
    assert!(matches!(reply, hft_ctl::Reply::Wait), "{reply:?}");
    let mut cursor = 0;
    let mut sent = false;
    let mut acked = false;
    let mut canceled = false;
    // Drive the same registry strategy, live SPSC adapter and pure exchange without OS sockets.
    for _ in 0..4000 {
        feed.poll(hft_rt::tsc_now()).unwrap();
        assert!(!matches!(seq.step(), StepOutcome::Fatal(_)));
        for record in seq.driver().io.journal.records().skip(cursor) {
            use hft_event::EventKind as K;
            cursor += 1;
            let hdr = hft_event::RecordHeader {
                seq: record.seq,
                tsc: record.tsc,
                ..hft_event::RecordHeader::new_zeroed()
            };
            match record.kind {
                K::BBO => {
                    let b = hft_event::Bbo::read_from_bytes(&record.body).unwrap();
                    exchange.on_record(&hdr, hft_sim::SimInput::Market(hft_sim::MarketEventRef::Bbo(&b)));
                }
                K::ORDER_DECISION => {
                    let d = hft_event::OrderDecision::try_read_from_bytes(&record.body).unwrap();
                    if d.reject == 0 && record.flags.0 & hft_event::RecordFlags::SHADOW.0 == 0 {
                        let cmd = sim_rx_orders
                            .try_pop()
                            .expect("decision must publish its command");
                        assert_eq!(cmd.decision_seq, record.seq);
                        let report = hft_core::paper::written(&cmd, d.venue, hft_rt::tsc_now()).unwrap();
                        io::rep_parts(&hft_event::RepMsg::SendReport(report), &[], |kind, parts| {
                            assert!(io::write_record(&mut sim_tx, kind, parts));
                        });
                        exchange.on_command(&cmd, cmd.decided_tsc);
                    }
                }
                K::SEND_REPORT => sent = true,
                K::ORDER_EVENT => {
                    let e = hft_event::OrderEvent::try_read_from_bytes(&record.body).unwrap();
                    acked |= e.what == hft_event::OrderEventKind::Ack;
                    canceled |= e.what == hft_event::OrderEventKind::CancelAck;
                    assert_eq!(e.epoch, 1);
                }
                _ => {}
            }
        }
        let now = hft_rt::tsc_now();
        exchange.advance(now);
        exchange.drain_due(now, |r| {
            hft_core::paper::reply_parts(&paper_rd, 1, &r.msg, r.bulk, |kind, parts| {
                assert!(io::write_record(&mut sim_tx, kind, parts));
            });
        });
        if sent && acked && canceled {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(1));
    }
    assert!(
        sent && acked && canceled,
        "sent={sent}, acked={acked}, canceled={canceled}"
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

#[test]
fn core_012_latency_ring_capacity_is_validated_without_starting() {
    let d = tempfile::tempdir().unwrap();
    let path = support::write(d.path(), &d.path().join("hot"), None);
    let original = std::fs::read_to_string(&path).unwrap();
    for (capacity, valid) in [(4096, true), (3, false), (128, false), (131072, false)] {
        std::fs::write(
            &path,
            original.replace(
                "[journal]",
                &format!("[journal]\nlatency_trace_capacity = {capacity}"),
            ),
        )
        .unwrap();
        assert_eq!(hft_core::config::load(&path, true).is_ok(), valid);
    }
}

#[test]
fn core_012_receive_source_survives_feed_adapter() {
    use hft_event::{Bbo, EventKind, FeedMsg, RecordFlags};
    use hft_types::Tsc;
    use zerocopy::FromZeros;
    let (mut tx, mut rx) = hft_core::runtime::byte_ring(4096).unwrap();
    let mut slot = hft_sequencer::InputSlot::with_capacity(4096);
    for flags in [RecordFlags::HW_TS, RecordFlags::SW_TS, RecordFlags(0)] {
        assert!(hft_core::io::feed(
            FeedMsg::Bbo(Bbo::new_zeroed(), flags),
            Tsc(7),
            &mut slot,
            &mut rx
        ));
        assert_eq!(slot.flags, flags);
        tx.reserve(16).unwrap().fill(7);
        tx.commit();
        assert!(hft_core::io::feed(
            FeedMsg::Bulk {
                kind: EventKind::L2_BATCH,
                len: 16,
                flags
            },
            Tsc(9),
            &mut slot,
            &mut rx
        ));
        assert_eq!(slot.flags, flags);
        assert_eq!(slot.body(), &[7; 16]);
    }
}
#[test]
fn core_421_dry_run_live_feed_venue() {
    let d = tempfile::tempdir().unwrap();
    let path = support::write(d.path(), &d.path().join("hot"), None);
    let base: toml::Value = toml::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    let codes = |cfg: &toml::Value, dry: bool| -> Vec<String> {
        std::fs::write(&path, toml::to_string(cfg).unwrap()).unwrap();
        match hft_core::config::load(&path, dry) {
            Ok(_) => Vec::new(),
            Err(e) => e.iter().map(|e| e.code.to_string()).collect(),
        }
    };
    // .1 live_feed without a source address or a feed thread.
    let mut c = base.clone();
    c["venues"]["public"]
        .as_table_mut()
        .unwrap()
        .insert("live_feed".into(), true.into());
    let got = codes(&c, true);
    for code in ["MissingField", "ThreadPlan"] {
        assert!(got.iter().any(|g| g == code), "CORE-421.1 {code}: {got:?}");
    }
    // .2 source address and threads added; a paper quote on the live venue is still rejected.
    let v = c["venues"]["public"].as_table_mut().unwrap();
    v.insert("src_ip".into(), "127.0.0.1".into());
    v.insert("bind_device".into(), "lo".into());
    for (name, task) in [("feed", "feed:public"), ("build", "conn-builder:public")] {
        let mut t = toml::map::Map::new();
        t.insert("name".into(), name.into());
        t.insert("task".into(), task.into());
        t.insert("park_us".into(), 100.into());
        t.insert("nice".into(), 0.into());
        c["threads"].as_array_mut().unwrap().push(t.into());
    }
    let got = codes(&c, true);
    assert!(got.iter().any(|g| g == "SimConfig"), "CORE-421.2: {got:?}");
    // .3 without quotes on it the venue is complete; its raw feed journal is reserved.
    c["sim"].as_table_mut().unwrap().remove("quotes");
    let got = codes(&c, true);
    for code in ["MissingField", "ThreadPlan", "SimConfig", "LiveFeed"] {
        assert!(!got.iter().any(|g| g == code), "CORE-421.3 {code}: {got:?}");
    }
    let parsed = hft_core::config::parse(&path).unwrap();
    assert_eq!(
        hft_core::budget::journals(&parsed, true),
        ["seq", "raw-feed-public"],
        "CORE-421.3"
    );
    // .4 network mode rejects the option.
    assert!(codes(&c, false).iter().any(|g| g == "LiveFeed"), "CORE-421.4");
    // .5 the polymarket-clob protocol is built only with its Cargo feature.
    let r = hft_core::venues::registered("polymarket-clob", hft_types::VenueId::POLYMARKET_CLOB);
    if cfg!(feature = "polymarket") {
        assert!(r.is_ok(), "CORE-421.5");
    } else {
        assert_eq!(r.unwrap_err().code, "VenueNotBuilt", "CORE-421.5");
    }
}
#[cfg(feature = "polymarket")]
#[test]
fn core_422_polymarket_sim_config_loads() {
    let d = tempfile::tempdir().unwrap();
    let path = support::write_polymarket_sim(d.path(), &d.path().join("hot"), None, None, "127.0.0.1", "lo");
    let l = match hft_core::config::load(&path, true) {
        Ok(l) => l,
        Err(e) => panic!("CORE-422.1: {e:?}"),
    };
    let v = l.rd.venue(hft_types::VenueId::POLYMARKET_CLOB);
    assert_eq!(v.ws_max_streams_per_conn, 2, "CORE-422.2");
    assert_eq!(
        l.rd.insts_of_venue(hft_types::VenueId::POLYMARKET_CLOB).len(),
        16,
        "CORE-422.2"
    );
    let i = l.rd.inst(hft_types::InstId(1));
    assert_eq!((i.tick, i.px_exp), (1, -3), "CORE-422.2");
}
#[test]
fn core_423_feed_raw_modes() {
    use hft_core::config::RawKind;
    use hft_venue::RawMode;
    // .1 defaults: network mode on, --dry-run off; an explicit value wins.
    assert_eq!(RawKind::resolve(None, false), RawMode::On, "CORE-423.1");
    assert_eq!(RawKind::resolve(None, true), RawMode::Off, "CORE-423.1");
    assert_eq!(
        RawKind::resolve(Some(RawKind::Sampled), true),
        RawMode::Sampled,
        "CORE-423.1"
    );
    assert_eq!(
        RawKind::resolve(Some(RawKind::On), true),
        RawMode::On,
        "CORE-423.1"
    );
    let d = tempfile::tempdir().unwrap();
    let path = support::write(d.path(), &d.path().join("hot"), None);
    let text = std::fs::read_to_string(&path).unwrap();
    // .2 `venues.<v>.feed.raw` parses.
    let with = text.replace(
        "[sim]",
        "[venues.public.feed.raw]\nbbo = 'off'\nmd = 'sampled'\n[sim]",
    );
    std::fs::write(&path, &with).unwrap();
    let c = hft_core::config::parse(&path).unwrap();
    let v = &c.venues["public"];
    assert_eq!(
        (v.feed.raw.bbo, v.feed.raw.md),
        (Some(RawKind::Off), Some(RawKind::Sampled)),
        "CORE-423.2"
    );
    // .3 an unknown mode or key is a parse error.
    for bad in ["md = 'all'", "dups = true"] {
        std::fs::write(
            &path,
            text.replace("[sim]", &format!("[venues.public.feed.raw]\n{bad}\n[sim]")),
        )
        .unwrap();
        let e = hft_core::config::parse(&path).unwrap_err();
        assert!(
            e.iter().any(|e| e.code == "ConfigParse"),
            "CORE-423.3 {bad}: {e:?}"
        );
    }
}

#[test]
fn core_424_feed_depth_conns() {
    let d = tempfile::tempdir().unwrap();
    let path = support::write(d.path(), &d.path().join("hot"), None);
    let text = std::fs::read_to_string(&path).unwrap();
    // .1 absent → None (the assembly default applies).
    let c = hft_core::config::parse(&path).unwrap();
    assert_eq!(c.venues["public"].feed.depth_conns, None, "CORE-424.1");
    // .2 `venues.<v>.feed.depth_conns` parses next to `feed.raw`.
    std::fs::write(
        &path,
        text.replace(
            "[sim]",
            "[venues.public.feed]
depth_conns = 2
[venues.public.feed.raw]
md = 'off'
[sim]",
        ),
    )
    .unwrap();
    let c = hft_core::config::parse(&path).unwrap();
    assert_eq!(c.venues["public"].feed.depth_conns, Some(2), "CORE-424.2");
    // .3 not a u8 → ConfigParse.
    std::fs::write(
        &path,
        text.replace(
            "[sim]",
            "[venues.public.feed]
depth_conns = 300
[sim]",
        ),
    )
    .unwrap();
    let e = hft_core::config::parse(&path).unwrap_err();
    assert!(e.iter().any(|e| e.code == "ConfigParse"), "CORE-424.3: {e:?}");
    // .4 the Polymarket sim configuration asks for two depth connections.
    #[cfg(feature = "polymarket")]
    {
        let pm =
            support::write_polymarket_sim(d.path(), &d.path().join("hot2"), None, None, "127.0.0.1", "lo");
        let c = hft_core::config::parse(&pm).unwrap();
        assert_eq!(c.venues["pm"].feed.depth_conns, Some(2), "CORE-424.4");
    }
}

#[test]
fn core_425_sim_maker_fill() {
    use hft_core::config::MakerFillKind;
    let d = tempfile::tempdir().unwrap();
    let path = support::write(d.path(), &d.path().join("hot"), None);
    let text = std::fs::read_to_string(&path).unwrap();
    // .1 absent → trade_through.
    let c = hft_core::config::parse(&path).unwrap();
    assert_eq!(
        c.sim.as_ref().map(|p| p.maker_fill),
        Some(MakerFillKind::TradeThrough),
        "CORE-425.1"
    );
    // .2 `queue`.
    std::fs::write(
        &path,
        text.replace(
            "[sim]
",
            "[sim]
maker_fill = 'queue'
",
        ),
    )
    .unwrap();
    let c = hft_core::config::parse(&path).unwrap();
    assert_eq!(
        c.sim.as_ref().map(|p| p.maker_fill),
        Some(MakerFillKind::Queue),
        "CORE-425.2"
    );
    // .3 an unknown model → ConfigParse.
    std::fs::write(
        &path,
        text.replace(
            "[sim]
",
            "[sim]
maker_fill = 'prob'
",
        ),
    )
    .unwrap();
    let e = hft_core::config::parse(&path).unwrap_err();
    assert!(e.iter().any(|e| e.code == "ConfigParse"), "CORE-425.3: {e:?}");
}

#[cfg(feature = "polymarket")]
#[test]
fn core_426_polymarket_maker_sim_config_loads() {
    use hft_types::{InstId, VenueId};
    let d = tempfile::tempdir().unwrap();
    let path =
        support::write_polymarket_maker_sim(d.path(), &d.path().join("hot"), None, None, "127.0.0.1", "lo");
    let l = match hft_core::config::load(&path, true) {
        Ok(l) => l,
        Err(e) => panic!("CORE-426.1: {e:?}"),
    };
    // .2 three BTC slot pairs on polymarket-clob and the BTCUSDT reference on binance-spot.
    assert_eq!(
        l.rd.insts_of_venue(VenueId::POLYMARKET_CLOB).len(),
        6,
        "CORE-426.2"
    );
    assert_eq!(
        l.rd.insts_of_venue(VenueId::BINANCE_SPOT),
        [InstId(7)],
        "CORE-426.2"
    );
    assert_eq!(
        l.rd.inst_meta(InstId(3)).symbol.as_ref(),
        "btc-up-1",
        "CORE-426.2"
    );
    // .3 both venues live; the sim fills resting orders by queue position.
    assert!(l.cfg.venues.values().all(|v| v.live_feed), "CORE-426.3");
    assert_eq!(
        l.cfg.sim.as_ref().map(|p| p.maker_fill),
        Some(hft_core::config::MakerFillKind::Queue),
        "CORE-426.3"
    );
}
#[cfg(feature = "polymarket")]
#[test]
fn core_430_production_orders_polymarket_only() {
    let codes = |path: &std::path::Path| -> Vec<String> {
        hft_core::config::load(path, false)
            .err()
            .unwrap_or_default()
            .into_iter()
            .map(|e| e.code.to_string())
            .collect()
    };
    let d = tempfile::tempdir().unwrap();
    let live = support::PmLive {
        uid: "0f5e8d2a-1111-2222-3333-".into(),
        keys_dir: d.path().join("keys"),
    };
    let hot = d.path().join("hot");
    let path = support::write_polymarket_maker_live(d.path(), &hot, None, None, "127.0.0.1", "lo", &live);
    // .1 the live Polymarket maker: production orders accepted.
    assert_eq!(codes(&path), Vec::<String>::new(), "CORE-430.1");
    let text = std::fs::read_to_string(&path).unwrap();
    // .2 the same accounts (is_prod) with testnet orders.
    std::fs::write(
        &path,
        text.replace("order_env = \"prod\"", "order_env = \"testnet\""),
    )
    .unwrap();
    assert!(
        codes(&path).contains(&"EnvAccountMismatch".to_string()),
        "CORE-430.2"
    );
    // .3 production orders need production market data.
    std::fs::write(
        &path,
        text.replace("feed_env = \"prod\"", "feed_env = \"testnet\""),
    )
    .unwrap();
    assert!(codes(&path).contains(&"EnvCombo".to_string()), "CORE-430.3");
    // .4 a Binance account with production orders: refused.
    let d2 = tempfile::tempdir().unwrap();
    let bn = support::write_live(d2.path(), &d2.path().join("hot"), true);
    let t = std::fs::read_to_string(&bn).unwrap();
    assert!(t.contains("order_env = \"testnet\""));
    std::fs::write(&bn, t.replace("order_env = \"testnet\"", "order_env = \"prod\"")).unwrap();
    assert!(codes(&bn).contains(&"ProdOrderVenue".to_string()), "CORE-430.4");
}
