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
        clock: hft_rt::UtcClock::fixed(clock.tsc_utc(&clock.base)),
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
        hft_rt::UtcClock::fixed(clock.tsc_utc(&clock.base)),
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
    // .3 the book of a slot inst tolerates 3 consecutive locked or crossed messages.
    assert_eq!(v.crossed_tolerance_default, 3, "CORE-422.3");
    assert_eq!(i.crossed_tolerance, 3, "CORE-422.3");
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
        day_loss_carry: None,
        daily_loss_abs: None,
        daily_loss_max_resumes: None,
        presign: true,
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

/// The live maker's startup blob with `version + 1` and `clip_shares` changed, and (`open`) the
/// reference group's `open_allowed` set.
#[cfg(feature = "polymarket")]
fn pm_live_blob(blob: &[u8], open: Option<u8>) -> Vec<u8> {
    use hft_event::{GroupSpec, StrategyParamsHdr};
    use hft_strategy_sdk::StrategySet;
    use zerocopy::{IntoBytes, TryFromBytes};
    let mut b = blob.to_vec();
    let (mut h, _) = StrategyParamsHdr::try_read_from_prefix(&b).unwrap();
    h.version += 1;
    let hl = h.as_bytes().len();
    b[..hl].copy_from_slice(h.as_bytes());
    let desc = hft_core::Strategies::kinds()
        .iter()
        .find(|d| d.kind_id == h.kind_id)
        .unwrap();
    let ix = hft_strategy_sdk::params::field_ix(desc.schema, "clip_shares").unwrap();
    let at = b.len() - h.body_len as usize + 8 + 8 * usize::from(ix);
    let v = i64::from_le_bytes(b[at..at + 8].try_into().unwrap()) + 1;
    b[at..at + 8].copy_from_slice(&v.to_le_bytes());
    if let Some(o) = open {
        let gl = std::mem::size_of::<GroupSpec>();
        b[hl + (usize::from(h.n_groups) - 1) * gl + 7] = o;
    }
    b
}

#[cfg(feature = "polymarket")]
#[test]
fn core_431_live_maker_set_params_accepts_the_startup_shape() {
    use hft_ctl_proto::{AccountSel, CmdBody, CmdRequest, PROTO_VERSION, ResultCode, Uuid};
    use hft_event::{CmdResult, EventKind};
    use zerocopy::FromBytes;
    let d = tempfile::tempdir().unwrap();
    let live = support::PmLive {
        uid: "0f5e8d2a-1111-2222-3333-".into(),
        keys_dir: d.path().join("keys"),
        day_loss_carry: None,
        daily_loss_abs: None,
        daily_loss_max_resumes: None,
        presign: true,
    };
    let path = support::write_polymarket_maker_live(
        d.path(),
        &d.path().join("hot"),
        None,
        None,
        "127.0.0.1",
        "lo",
        &live,
    );
    let l = hft_core::config::load(&path, false).unwrap();
    let blob = l.configs.params[0].0.clone();
    let rd = hft_refdata::RefData::load(l.rd.blob(), None).unwrap();
    // The startup blob's reference group is on binance-spot, outside pm-live's venues.
    assert_eq!(
        rd.account(hft_types::AccountId(1)).venues,
        1u64 << hft_types::VenueId::POLYMARKET_CLOB.0,
        "CORE-431"
    );
    let active = hft_sequencer::parsed::parse_params::<hft_core::Strategies>(
        &blob,
        hft_core::config::digest(&blob),
        &rd,
        l.setup.caps.max_groups_per_instance,
        None,
    )
    .map(|p| hft_sequencer::ParsedConfig::StrategyParams(Box::new(p)))
    .unwrap();
    let io = hft_testkit::seq_rig::RigIo::<hft_core::Strategies> {
        j: hft_sequencer::MemJournal::new(l.setup.clock),
        tsc: 1,
        rings: Default::default(),
        n_venues: 2,
        out: vec![],
        mailbox: vec![],
        kills: vec![],
        calib: hft_rt::CalibPair {
            tsc: hft_types::Tsc(0),
            realtime_ns: 0,
            ..Default::default()
        },
        hz: l.setup.clock.tsc_hz_milli,
    };
    let heartbeat = Box::leak(Box::new(hft_rt::Heartbeat::new()));
    let mut seq = hft_sequencer::Sequencer::boot(
        hft_sequencer::BootSource::Empty { configs: l.configs },
        &l.setup,
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
    let clock = hft_rt::TscClock::from_hz_milli(
        l.setup.clock.tsc_hz_milli,
        hft_rt::CalibPair {
            tsc: hft_types::Tsc(0),
            realtime_ns: 0,
            ..Default::default()
        },
    );
    let (parsed_tx, mut parsed_rx) = Box::leak(Box::new(hft_rt::Mailbox::new())).split();
    let (_retired_tx, retired_rx) = Box::leak(Box::new(hft_rt::Mailbox::new())).split();
    let mut ctl = hft_ctl::Core::<hft_core::Strategies, _>::new(
        hft_ctl::GatewayConfig::new(
            l.cfg.paths.ctl_socket,
            hft_ctl::PrincipalTable::from_parts(
                vec![(1, hft_ctl::UidKind::Cli { login: "ops".into() })],
                vec![(
                    "cli:ops".into(),
                    hft_ctl_proto::RoleSet::from_roles(&[hft_ctl_proto::Role::Admin]),
                )],
            ),
            256,
            1024,
            l.setup.caps.max_groups_per_instance,
        ),
        rd,
        hft_ctl::VecRing::with_free(1 << 20),
        parsed_tx,
        retired_rx,
        |_, _| Err(1),
        hft_sequencer::config_blob::decode_risk_blob,
        vec![active],
        hft_rt::UtcClock::fixed(clock.tsc_utc(&clock.base)),
        Default::default(),
    );
    // One SetParams through the gateway and, if it was written, the sequencer.
    let mut n = 0u8;
    let mut send = |blob: &[u8]| -> (Option<ResultCode>, Option<CmdResult>) {
        n += 1;
        let request = CmdRequest {
            v: PROTO_VERSION,
            cmd_id: Uuid::from_bytes([n; 16]),
            issuer: "cli:ops".into(),
            issued_at_ns: 0,
            expires_at_ns: 10_000_000_000,
            account: AccountSel::Name("pm-live".into()),
            preview: false,
            confirmed: false,
            expect_state_version: None,
            view_account: None,
            chat_type: None,
            cmd: CmdBody::SetParams {
                strategy: "pm-maker".into(),
                account: None,
                blob_b64: base64::Engine::encode(&base64::engine::general_purpose::STANDARD, blob),
                base_version: None,
            },
        };
        if let hft_ctl::Reply::Now(r) = ctl.on_request(1, 1, &request.to_json(), hft_types::Tsc(1)) {
            return (r.code, None);
        }
        let io = &mut seq.driver_mut().io;
        // The rig ring carries logical bodies: drop the one-part Multi prefix.
        for (kind, _, mut body) in ctl.ring_mut().out.drain(..) {
            if kind == EventKind::CONFIG_BLOB || kind == EventKind::CONTROL_CMD {
                let (part, _) = hft_event::PartHdr::read_from_prefix(&body).unwrap();
                assert_eq!(part.parts, 1);
                body.drain(..16);
            }
            io.rings.entry((0, 0)).or_default().push_back((kind, body));
        }
        while let Some(p) = parsed_rx.try_take() {
            io.mailbox.push(p);
        }
        for _ in 0..64 {
            assert!(!matches!(seq.step(), hft_sequencer::StepOutcome::Fatal(_)));
        }
        let res = seq
            .driver()
            .io
            .j
            .records()
            .filter(|r| r.kind == EventKind::CMD_RESULT)
            .filter_map(|r| CmdResult::read_from_bytes(&r.body).ok())
            .find(|c| c.cmd_id == [n; 16]);
        (None, res)
    };
    // .1 the running blob, version + 1 and one field changed: written and Applied.
    let (now, res) = send(&pm_live_blob(&blob, None));
    assert_eq!(now, None, "CORE-431.1");
    let res = res.expect("CORE-431.1: no CmdResult");
    assert_eq!(
        (res.status, res.detail, res.params_version),
        (ResultCode::Applied as u8, 0, 2),
        "CORE-431.1"
    );
    // .2 the reference group allowed to open: refused by the gateway.
    let (now, res) = send(&pm_live_blob(&pm_live_blob(&blob, None), Some(1)));
    assert_eq!(now, Some(ResultCode::InvalidArgs), "CORE-431.2");
    assert!(res.is_none(), "CORE-431.2");
    // .3 startup refuses the same shape (one group rule for both).
    let bad = pm_live_blob(&blob, Some(1));
    let file = d.path().join("strategy_params.bin");
    let text = std::fs::read_to_string(&path).unwrap().replace(
        &hft_core::config::hex(&hft_core::config::digest(&std::fs::read(&file).unwrap())),
        &hft_core::config::hex(&hft_core::config::digest(&bad)),
    );
    std::fs::write(&file, &bad).unwrap();
    std::fs::write(&path, text).unwrap();
    let e = hft_core::config::load(&path, false).unwrap_err();
    assert!(e.iter().any(|e| e.code == "StrategyParams"), "CORE-431.3: {e:?}");
}

#[cfg(feature = "polymarket")]
#[test]
fn core_432_polymarket_expiry_window_is_the_gtd_lifetime() {
    use hft_venue_proto::VenueProtocol;
    let hft_venue_proto::StaleGuard::ExpireAt { default_ms } =
        venue_polymarket::PolymarketClob::stale_guard()
    else {
        panic!("CORE-432.1 {:?}", venue_polymarket::PolymarketClob::stale_guard());
    };
    let v = hft_types::VenueId::POLYMARKET_CLOB;
    let d = tempfile::tempdir().unwrap();
    let live = support::PmLive {
        uid: "0f5e8d2a-1111-2222-3333-".into(),
        keys_dir: d.path().join("keys"),
        day_loss_carry: None,
        daily_loss_abs: None,
        daily_loss_max_resumes: None,
        presign: true,
    };
    let hot = d.path().join("hot");
    let live_path =
        support::write_polymarket_maker_live(d.path(), &hot, None, None, "127.0.0.1", "lo", &live);
    let d2 = tempfile::tempdir().unwrap();
    let sim_path =
        support::write_polymarket_maker_sim(d2.path(), &d2.path().join("hot"), None, None, "127.0.0.1", "lo");
    for (path, dry) in [(live_path, false), (sim_path, true)] {
        let l = match hft_core::config::load(&path, dry) {
            Ok(l) => l,
            Err(e) => panic!("CORE-432 dry={dry}: {e:?}"),
        };
        assert_eq!(
            l.rd.venue(v).stale_guard,
            hft_refdata::StaleGuardKind::ExpireAt,
            "CORE-432.1"
        );
        assert_eq!(
            l.setup.oms.expire_window_ms.get(v),
            Some(default_ms),
            "CORE-432.2"
        );
        assert_eq!(default_ms, 300_000, "CORE-432.2");
        assert_eq!(l.setup.oms.not_found_grace_of(v, &l.rd), 300_500, "CORE-432.2");
        assert_eq!(l.setup.oms.unknown_alarm_ms, 30_000, "CORE-432.3");
    }
}

#[test]
fn core_441_shared_account_ip() {
    use hft_core::config::shared_account_ip;
    use std::net::Ipv4Addr;
    let (a, b) = (Ipv4Addr::new(10, 0, 1, 10), Ipv4Addr::new(10, 0, 1, 11));
    // .1–.3 the rule.
    assert_eq!(
        shared_account_ip("v", false, &[(1, a), (2, a)]).map(|e| e.code),
        Some("SharedAccountIp"),
        "CORE-441.1"
    );
    assert!(
        shared_account_ip("v", false, &[(1, a), (2, b)]).is_none(),
        "CORE-441.2"
    );
    assert!(shared_account_ip("v", false, &[(1, a)]).is_none(), "CORE-441.2");
    assert!(
        shared_account_ip("v", true, &[(1, a), (2, a)]).is_none(),
        "CORE-441.3"
    );
    // .4 the protocols' declarations.
    assert!(!hft_core::venues::shared_egress_ip("binance-spot"), "CORE-441.4");
    assert!(!hft_core::venues::shared_egress_ip("binance-usdm"), "CORE-441.4");
    #[cfg(feature = "polymarket")]
    assert!(
        hft_core::venues::shared_egress_ip("polymarket-clob"),
        "CORE-441.4"
    );
    // .5 two live accounts of a Binance venue on the one `src_ip`.
    let d = tempfile::tempdir().unwrap();
    let path = support::write(d.path(), &d.path().join("hot"), None);
    let text = std::fs::read_to_string(&path).unwrap();
    let account = |n: u8| {
        format!(
            "[[venues.public.accounts]]\naccount = {n}\napi_key_path = 'k'\nsession_seed_path = 's'\nrest_api_key_path = 'k'\nrest_secret_path = 's'\n"
        )
    };
    let two = text.replace(
        "[sim]",
        &format!(
            "src_ip = '127.0.0.1'\nbind_device = 'lo'\n{}{}[sim]",
            account(1),
            account(2)
        ),
    );
    std::fs::write(&path, two).unwrap();
    let codes: Vec<_> = hft_core::config::load(&path, false)
        .unwrap_err()
        .iter()
        .map(|e| e.code)
        .collect();
    assert!(
        codes.contains(&"AccountCapacity") && codes.contains(&"SharedAccountIp"),
        "CORE-441.5 {codes:?}"
    );
    // .6 the live Polymarket configuration with a second account: only the assembly's capacity.
    #[cfg(feature = "polymarket")]
    {
        let live = support::PmLive {
            uid: "0f5e8d2a-1111-2222-3333-".into(),
            keys_dir: d.path().join("keys"),
            day_loss_carry: None,
            daily_loss_abs: None,
            daily_loss_max_resumes: None,
            presign: true,
        };
        let pm = support::write_polymarket_maker_live(
            d.path(),
            &d.path().join("hot2"),
            None,
            None,
            "127.0.0.1",
            "lo",
            &live,
        );
        let mut config: toml::Value = toml::from_str(&std::fs::read_to_string(&pm).unwrap()).unwrap();
        let accounts = config["venues"]["pm"]["accounts"].as_array_mut().unwrap();
        let mut second = accounts[0].clone();
        second
            .as_table_mut()
            .unwrap()
            .insert("account".into(), toml::Value::Integer(2));
        accounts.push(second);
        std::fs::write(&pm, toml::to_string(&config).unwrap()).unwrap();
        let codes: Vec<_> = hft_core::config::load(&pm, false)
            .unwrap_err()
            .iter()
            .map(|e| e.code)
            .collect();
        assert!(codes.contains(&"AccountCapacity"), "CORE-441.6 {codes:?}");
        assert!(!codes.contains(&"SharedAccountIp"), "CORE-441.6 {codes:?}");
    }
}

#[test]
fn core_433_venue_rest_timeout_and_max_idle() {
    let d = tempfile::tempdir().unwrap();
    let path = support::write(d.path(), &d.path().join("hot"), None);
    let text = std::fs::read_to_string(&path).unwrap();
    let net_of = |o: &hft_core::config::NetOpts| {
        let mut n = hft_net::NetConfig::default();
        o.apply(&mut n);
        (n.rest_timeout_ms, n.rest_max_idle_ms)
    };
    // .1 absent: the NetConfig defaults.
    let c = hft_core::config::parse(&path).unwrap();
    assert_eq!(net_of(&c.venues["public"].net), (2000, 0), "CORE-433.1");
    // .2 configured values replace them.
    let with = |extra: &str| text.replace("[sim]", &format!("[venues.public.net]\n{extra}\n[sim]"));
    std::fs::write(&path, with("rest_timeout_ms = 3000\nrest_max_idle_ms = 20000")).unwrap();
    let c = hft_core::config::parse(&path).unwrap();
    assert_eq!(net_of(&c.venues["public"].net), (3000, 20_000), "CORE-433.2");
    // .3 a zero timeout or an unknown key is refused.
    for (extra, code) in [
        ("rest_timeout_ms = 0", "BadValue"),
        ("idle_ms = 1", "ConfigParse"),
    ] {
        std::fs::write(&path, with(extra)).unwrap();
        let e = hft_core::config::load(&path, true).unwrap_err();
        assert!(e.iter().any(|e| e.code == code), "CORE-433.3 {extra}: {e:?}");
    }
    // .4 the live Polymarket configuration: 5000 ms (= OMS ack timeout) and 25 s idle age.
    #[cfg(feature = "polymarket")]
    {
        let v = hft_types::VenueId::POLYMARKET_CLOB;
        let live = support::PmLive {
            uid: "0f5e8d2a-1111-2222-3333-".into(),
            keys_dir: d.path().join("keys"),
            day_loss_carry: None,
            daily_loss_abs: None,
            daily_loss_max_resumes: None,
            presign: true,
        };
        let pm = support::write_polymarket_maker_live(
            d.path(),
            &d.path().join("hot2"),
            None,
            None,
            "127.0.0.1",
            "lo",
            &live,
        );
        let l = match hft_core::config::load(&pm, false) {
            Ok(l) => l,
            Err(e) => panic!("CORE-433.4 {e:?}"),
        };
        let o = l.cfg.venues["pm"].net;
        assert_eq!(net_of(&o), (5000, 25_000), "CORE-433.4");
        assert_eq!(l.setup.oms.ack_timeout_ms.get(v), 5000, "CORE-433.4");
        // .5 a REST timeout above the OMS ack timeout of a venue with a gateway is refused.
        let t = std::fs::read_to_string(&pm).unwrap();
        assert!(t.contains("rest_timeout_ms = 5000"), "CORE-433.5");
        std::fs::write(&pm, t.replace("rest_timeout_ms = 5000", "rest_timeout_ms = 5001")).unwrap();
        let e = hft_core::config::load(&pm, false).unwrap_err();
        assert!(
            e.iter().any(|e| e.code == "RestTimeoutAboveAck"),
            "CORE-433.5 {e:?}"
        );
    }
}

#[test]
fn core_440_gw_rest_conns_and_cancel_reserve() {
    let d = tempfile::tempdir().unwrap();
    let path = support::write(d.path(), &d.path().join("hot"), None);
    let text = std::fs::read_to_string(&path).unwrap();
    let effective = |c: &hft_core::config::Config, v: &str| {
        let venue = &c.venues[v];
        venue
            .gw
            .resolve(hft_core::venues::gw_rest_defaults(&venue.protocol).unwrap())
    };
    // .1 absent: the protocol's defaults.
    let c = hft_core::config::parse(&path).unwrap();
    assert_eq!(
        hft_core::venues::gw_rest_defaults("binance-spot"),
        Some((2, 0)),
        "CORE-440.1"
    );
    assert_eq!(effective(&c, "public"), (2, 0), "CORE-440.1");
    // .2 configured values replace them.
    let with = |extra: &str| text.replace("[sim]", &format!("[venues.public.gw]\n{extra}\n[sim]"));
    std::fs::write(&path, with("rest_conns = 4\nrest_cancel_reserve = 1")).unwrap();
    let l = match hft_core::config::load(&path, true) {
        Ok(l) => l,
        Err(e) => panic!("CORE-440.2 {e:?}"),
    };
    assert_eq!(effective(&l.cfg, "public"), (4, 1), "CORE-440.2");
    // .3 out of range or an unknown key is refused.
    for (extra, code) in [
        ("rest_conns = 0", "BadValue"),
        ("rest_conns = 17", "BadValue"),
        ("rest_conns = 2\nrest_cancel_reserve = 2", "BadValue"),
        ("conns = 4", "ConfigParse"),
    ] {
        std::fs::write(&path, with(extra)).unwrap();
        let e = hft_core::config::load(&path, true).unwrap_err();
        assert!(e.iter().any(|e| e.code == code), "CORE-440.3 {extra}: {e:?}");
    }
    // .4 the live Polymarket configuration: eight connections, one kept for cancels.
    #[cfg(feature = "polymarket")]
    {
        let live = support::PmLive {
            uid: "0f5e8d2a-1111-2222-3333-".into(),
            keys_dir: d.path().join("keys"),
            day_loss_carry: None,
            daily_loss_abs: None,
            daily_loss_max_resumes: None,
            presign: true,
        };
        let pm = support::write_polymarket_maker_live(
            d.path(),
            &d.path().join("hot2"),
            None,
            None,
            "127.0.0.1",
            "lo",
            &live,
        );
        let l = match hft_core::config::load(&pm, false) {
            Ok(l) => l,
            Err(e) => panic!("CORE-440.4 {e:?}"),
        };
        assert_eq!(
            hft_core::venues::gw_rest_defaults("polymarket-clob"),
            Some((8, 1)),
            "CORE-440.4"
        );
        assert_eq!(effective(&l.cfg, "pm"), (8, 1), "CORE-440.4");
    }
}

/// The live Polymarket configuration values USDC at par (the daily loss counts USDC losses), its
/// daily loss is 15 USDC, and `--day-loss-carry` becomes the ECF1 carry row.
#[cfg(feature = "polymarket")]
#[test]
fn core_434_live_polymarket_daily_loss_in_usdc() {
    use hft_ledger::{AssetName, FillClass, FillDelta, FixedVec, Ledger, LedgerConfigError};
    use hft_types::{AccountId, Notional, Px, Qty, Rate, Side};
    let v = hft_types::VenueId::POLYMARKET_CLOB;
    let a = AccountId(1);
    // 2026-10-24.
    let day = 20_750u32;
    let d = tempfile::tempdir().unwrap();
    let live = support::PmLive {
        uid: "0f5e8d2a-1111-2222-3333-".into(),
        keys_dir: d.path().join("keys"),
        day_loss_carry: Some((day, -28_510_000)),
        daily_loss_abs: None,
        daily_loss_max_resumes: None,
        presign: true,
    };
    let path = support::write_polymarket_maker_live(
        d.path(),
        &d.path().join("hot"),
        None,
        None,
        "127.0.0.1",
        "lo",
        &live,
    );
    let l = match hft_core::config::load(&path, false) {
        Ok(l) => l,
        Err(e) => panic!("CORE-434: {e:?}"),
    };
    let slot = l.rd.insts_of_venue(v)[0];
    let usdc = l.rd.inst(slot).quote;
    assert_eq!(&*l.rd.asset(usdc).name, "USDC");
    // .1 USDC at par; the same config without it is refused by the ledger.
    assert!(
        l.setup.ledger.par_assets.iter().any(|n| n.matches("USDC")),
        "CORE-434.1"
    );
    let caps = l.setup.caps.to_ledger();
    let mut ledger = Ledger::new(&l.rd, &l.setup.ledger, &caps).expect("CORE-434.1");
    let mut old = l.setup.ledger.clone();
    let mut par = FixedVec::new();
    par.push(AssetName::new("USDT"));
    old.par_assets = par;
    assert_eq!(
        Ledger::new(&l.rd, &old, &caps).err(),
        Some(LedgerConfigError::NoUsdtRate { asset: usdc }),
        "CORE-434.1"
    );
    // .2 daily loss 15 USDC, not reduced by the equity fraction.
    let acct = l
        .configs
        .risk
        .accounts
        .iter()
        .find(|x| x.account == a)
        .expect("pm-live limits");
    assert_eq!(
        (acct.daily_loss_abs, acct.daily_loss_kill),
        (Notional(15_000_000), Rate(1_000_000_000)),
        "CORE-434.2"
    );
    // .3 on the live refdata, 10 shares bought at 0.60 and delivered at 0: day −6 USDC.
    ledger.on_run_start(0);
    let f = FillDelta {
        account: a,
        venue: v,
        side: Side::Buy,
        class: FillClass::Open as u8,
        flags: 0,
        leg_ix: 0,
        _pad: [0; 2],
        inst: slot,
        group: hft_types::GROUP_NONE,
        qty: Qty(10_000_000),
        net_qty: Qty(10_000_000),
        px: Px(600),
        fee: 0,
        fee_asset: hft_types::ASSET_NONE,
        _pad2: 0,
        slot: 0,
        r#gen: 0,
        _pad3: 0,
        seq: 0,
    };
    assert_eq!(l.rd.inst(slot).px_exp, -3, "CORE-434.3 token price exponent");
    ledger.apply_fill(&f, 1);
    let x = hft_event::ExternalFill {
        venue: v,
        account: a,
        side: Side::Sell,
        cause: hft_event::ExternalCause::Delivery,
        inst: slot,
        qty: Qty(10_000_000),
        px: Px(0),
        fee: Qty(0),
        fee_asset: hft_types::ASSET_NONE,
        _pad: [0; 6],
        exch_order_id: 0,
        trade_id: 1,
        ex_ts_ns: 0,
        rx_tsc: hft_types::Tsc(0),
        fill_seq: 0,
        cash_qty: Qty(0),
        cash_amount: Qty(0),
    };
    ledger.apply_external(&x, false, 2).expect("CORE-434.3 booked");
    assert_eq!(ledger.account(a).day.net(), Notional(-6_000_000), "CORE-434.3");
    // .4 the carry row.
    assert_eq!(
        l.setup.day_pnl_carry,
        vec![(a, day, Notional(-28_510_000))],
        "CORE-434.4"
    );
}

/// `pm_sim_config --daily-loss USDC --daily-loss-resumes N`: the live account's daily loss and
/// resume count are the chosen values (defaults 15 and 1), and a carried loss beyond the chosen
/// limit sets DailyLoss at the first day roll.
#[cfg(feature = "polymarket")]
#[test]
fn core_435_live_polymarket_daily_loss_option() {
    use hft_types::{AccountId, CapSource, Notional, Rate};
    use zerocopy::IntoBytes;
    let a = AccountId(1);
    // 2026-10-24.
    let day = 20_750u32;
    let load = |abs: Option<i64>, resumes: Option<u8>, carry: Option<i64>| {
        let d = tempfile::tempdir().unwrap();
        let live = support::PmLive {
            uid: "0f5e8d2a-1111-2222-3333-".into(),
            keys_dir: d.path().join("keys"),
            day_loss_carry: carry.map(|p| (day, p)),
            daily_loss_abs: abs,
            daily_loss_max_resumes: resumes,
            presign: true,
        };
        let path = support::write_polymarket_maker_live(
            d.path(),
            &d.path().join("hot"),
            None,
            None,
            "127.0.0.1",
            "lo",
            &live,
        );
        match hft_core::config::load(&path, false) {
            Ok(l) => l,
            Err(e) => panic!("CORE-435: {e:?}"),
        }
    };
    let limits = |l: &hft_core::config::Loaded| {
        let x = l
            .configs
            .risk
            .accounts
            .iter()
            .find(|x| x.account == a)
            .expect("pm-live limits");
        (x.daily_loss_abs, x.daily_loss_kill, x.daily_loss_max_resumes)
    };
    // .1 defaults.
    assert_eq!(
        limits(&load(None, None, None)),
        (Notional(15_000_000), Rate(1_000_000_000), 1),
        "CORE-435.1"
    );
    // .2 chosen values.
    assert_eq!(
        limits(&load(Some(7_500_000), Some(3), None)),
        (Notional(7_500_000), Rate(1_000_000_000), 3),
        "CORE-435.2"
    );
    assert_eq!(limits(&load(None, Some(0), None)).2, 0, "CORE-435.2");
    // .3/.4 boot on the generated config and roll into the carried day.
    let boot_and_roll = |l: hft_core::config::Loaded| {
        let io = hft_testkit::seq_rig::RigIo::<hft_core::Strategies> {
            j: hft_sequencer::MemJournal::new(l.setup.clock),
            tsc: 1,
            rings: Default::default(),
            n_venues: 2,
            out: vec![],
            mailbox: vec![],
            kills: vec![],
            calib: hft_rt::CalibPair {
                tsc: hft_types::Tsc(0),
                realtime_ns: 0,
                ..Default::default()
            },
            hz: l.setup.clock.tsc_hz_milli,
        };
        let heartbeat = Box::leak(Box::new(hft_rt::Heartbeat::new()));
        let mut seq = hft_sequencer::Sequencer::boot(
            hft_sequencer::BootSource::Empty { configs: l.configs },
            &l.setup,
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
        // The DayRoll system timer (owner 1, id 3 | day << 8) on the housekeeping ring.
        let t = hft_event::Timer {
            timer_id: 3 | (day << 8),
            owner: 1,
            _pad: 0,
            group: hft_types::GroupId(0),
            _pad2: 0,
            deadline_tsc: 1,
        };
        seq.driver_mut()
            .io
            .rings
            .entry((1, 0))
            .or_default()
            .push_back((hft_event::EventKind::TIMER, t.as_bytes().to_vec()));
        for _ in 0..64 {
            assert!(!matches!(seq.step(), hft_sequencer::StepOutcome::Fatal(_)));
        }
        let e = seq.engine();
        let capped = e
            .risk
            .mode
            .caps()
            .iter()
            .any(|c| c.source == CapSource::DAILY_LOSS && c.scope.account == a);
        (e.last_day_rolled, e.ledger.account(a).day.net(), capped)
    };
    // .3 −8 carried into a chosen 7.5 limit: DailyLoss right after the day roll.
    assert_eq!(
        boot_and_roll(load(Some(7_500_000), None, Some(-8_000_000))),
        (day, Notional(-8_000_000), true),
        "CORE-435.3"
    );
    // .4 the same carry under the default 15: no cap.
    assert_eq!(
        boot_and_roll(load(None, None, Some(-8_000_000))),
        (day, Notional(-8_000_000), false),
        "CORE-435.4"
    );
}

/// Raw default value of binary-maker field `name` in a `StrategyParams` blob.
#[cfg(feature = "polymarket")]
fn maker_field(blob: &[u8], name: &str) -> i64 {
    use hft_event::StrategyParamsHdr;
    use hft_strategy_sdk::StrategySet;
    use zerocopy::TryFromBytes;
    let (h, _) = StrategyParamsHdr::try_read_from_prefix(blob).unwrap();
    let desc = hft_core::Strategies::kinds()
        .iter()
        .find(|d| d.kind_id == h.kind_id)
        .unwrap();
    let ix = hft_strategy_sdk::params::field_ix(desc.schema, name).unwrap();
    let at = blob.len() - h.body_len as usize + 8 + 8 * usize::from(ix);
    i64::from_le_bytes(blob[at..at + 8].try_into().unwrap())
}

#[cfg(feature = "polymarket")]
#[test]
fn core_439_polymarket_maker_param_values() {
    let set =
        |kv: &[(&str, i64)]| -> Vec<(String, i64)> { kv.iter().map(|(k, v)| (k.to_string(), *v)).collect() };
    let load = |path: &std::path::Path| match hft_core::config::load(path, true) {
        Ok(l) => l.configs.params[0].0.clone(),
        Err(e) => panic!("CORE-439: {e:?}"),
    };
    // .1 the maker simulation: values set after the defaults.
    let d = tempfile::tempdir().unwrap();
    let path = support::write_polymarket_maker_sim_with(
        d.path(),
        &d.path().join("hot"),
        None,
        None,
        "127.0.0.1",
        "lo",
        &set(&[("pair_margin", -20_000_000), ("fv_limit", 1)]),
    );
    let b = load(&path);
    assert_eq!(
        (
            maker_field(&b, "pair_margin"),
            maker_field(&b, "fv_limit"),
            maker_field(&b, "enabled")
        ),
        (-20_000_000, 1, 1),
        "CORE-439.1"
    );
    // .2 the live maker: after PM_LIVE_PARAMS (clip_shares 5 → 7; inv_max_shares stays 10).
    let d = tempfile::tempdir().unwrap();
    let live = support::PmLive {
        uid: "0f5e8d2a-1111-2222-3333-".into(),
        keys_dir: d.path().join("keys"),
        day_loss_carry: None,
        daily_loss_abs: None,
        daily_loss_max_resumes: None,
        presign: true,
    };
    let path = support::write_polymarket_maker_live_with(
        d.path(),
        &d.path().join("hot"),
        None,
        None,
        "127.0.0.1",
        "lo",
        &live,
        &set(&[("clip_shares", 7)]),
    );
    let b = match hft_core::config::load(&path, false) {
        Ok(l) => l.configs.params[0].0.clone(),
        Err(e) => panic!("CORE-439.2: {e:?}"),
    };
    assert_eq!(
        (maker_field(&b, "clip_shares"), maker_field(&b, "inv_max_shares")),
        (7, 10),
        "CORE-439.2"
    );
    // .3 refused: not NAME=RAW, not an integer, an unknown field, outside the schema range.
    assert!(support::parse_param("clip_shares").is_err(), "CORE-439.3");
    assert!(support::parse_param("clip_shares=0.5").is_err(), "CORE-439.3");
    assert_eq!(
        support::parse_param(" clip_shares = 7 "),
        Ok(("clip_shares".into(), 7)),
        "CORE-439.3"
    );
    assert!(
        support::check_maker_params(&set(&[("no_such_field", 1)])).is_err(),
        "CORE-439.3"
    );
    assert!(
        support::check_maker_params(&set(&[("clip_shares", 0)])).is_err(),
        "CORE-439.3"
    );
    assert!(
        support::check_maker_params(&set(&[("pair_margin", -100_000_001)])).is_err(),
        "CORE-439.3"
    );
    assert_eq!(
        support::check_maker_params(&set(&[("pair_margin", -100_000_000)])),
        Ok(()),
        "CORE-439.3"
    );
}

/// `venues.<v>.presign`: decimal step, Polymarket only, values in range.
#[test]
fn core_445_venue_presign() {
    use hft_core::config::PresignOpts;
    // .1 the step text in 1e-6 USDC.
    let step = |s: &str| {
        PresignOpts {
            below: 3,
            above: 1,
            step: s.into(),
            max_age_ms: 30_000,
        }
        .step_e6()
    };
    assert_eq!(
        ["0.01", "1", ".5", "0", "0.0000001", "-1", "", "0.0a"].map(step),
        [
            Some(10_000),
            Some(1_000_000),
            Some(500_000),
            None,
            None,
            None,
            None,
            None
        ],
        "CORE-445.1"
    );
    let table = |below: u32, above: u32, step: &str, age: u32| {
        format!(
            "\n[venues.VENUE.presign]\nbelow = {below}\nabove = {above}\nstep = \"{step}\"\nmax_age_ms = {age}\n"
        )
    };
    // .2 a venue other than polymarket-clob refuses it.
    let d = tempfile::tempdir().unwrap();
    let path = support::write(d.path(), &d.path().join("hot"), None);
    let text = std::fs::read_to_string(&path).unwrap();
    std::fs::write(
        &path,
        format!("{text}{}", table(3, 1, "0.01", 30_000).replace("VENUE", "public")),
    )
    .unwrap();
    let e = hft_core::config::load(&path, true).unwrap_err();
    assert!(e.iter().any(|e| e.code == "BadValue"), "CORE-445.2 {e:?}");
    // .3 the live Polymarket configuration without its default table: in range loads, out of
    // range is refused.
    #[cfg(feature = "polymarket")]
    {
        let live = support::PmLive {
            uid: "0f5e8d2a-1111-2222-3333-".into(),
            keys_dir: d.path().join("keys"),
            day_loss_carry: None,
            daily_loss_abs: None,
            daily_loss_max_resumes: None,
            presign: false,
        };
        let pm = support::write_polymarket_maker_live(
            d.path(),
            &d.path().join("hot2"),
            None,
            None,
            "127.0.0.1",
            "lo",
            &live,
        );
        let base = std::fs::read_to_string(&pm).unwrap();
        let with = |t: String| std::fs::write(&pm, format!("{base}{}", t.replace("VENUE", "pm"))).unwrap();
        with(table(3, 1, "0.01", 30_000));
        let l = match hft_core::config::load(&pm, false) {
            Ok(l) => l,
            Err(e) => panic!("CORE-445.3 {e:?}"),
        };
        let p = l.cfg.venues["pm"].presign.clone().unwrap();
        assert_eq!(
            (p.below, p.above, p.step_e6(), p.max_age_ms),
            (3, 1, Some(10_000), 30_000),
            "CORE-445.3"
        );
        for (below, above, step, age) in [
            (4, 4, "0.01", 30_000),
            (3, 1, "0", 30_000),
            (3, 1, "0.01", 999),
            (3, 1, "0.01", 240_001),
        ] {
            with(table(below, above, step, age));
            let e = hft_core::config::load(&pm, false).unwrap_err();
            assert!(
                e.iter().any(|e| e.code == "BadValue"),
                "CORE-445.3 {below} {above} {step} {age}: {e:?}"
            );
        }
        // .4 the live configuration pre-signs by default (2, 2, 0.01, 60 000); `presign: false`
        // (`pm_sim_config --live --no-presign`) writes no table.
        assert!(!base.contains("presign"), "CORE-445.4");
        let on = support::PmLive {
            presign: true,
            ..live
        };
        let pm = support::write_polymarket_maker_live(
            d.path(),
            &d.path().join("hot3"),
            None,
            None,
            "127.0.0.1",
            "lo",
            &on,
        );
        let l = match hft_core::config::load(&pm, false) {
            Ok(l) => l,
            Err(e) => panic!("CORE-445.4 {e:?}"),
        };
        let p = l.cfg.venues["pm"].presign.clone().unwrap();
        assert_eq!(
            (p.below, p.above, p.step_e6(), p.max_age_ms),
            (2, 2, Some(10_000), 60_000),
            "CORE-445.4"
        );
    }
}
