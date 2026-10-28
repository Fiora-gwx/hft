//! `non_prod_only` of the registered kinds (TESTS.md REG-012).
#![allow(
    clippy::disallowed_types,
    clippy::disallowed_macros,
    clippy::disallowed_methods,
    clippy::type_complexity
)] // allow-list: test code

use hft_strategy_registry::Strategies;
use hft_strategy_sdk::StrategySet;

#[path = "probe/common.rs"]
mod common;

#[test]
fn reg_012_flags() {
    let np: Vec<u16> = Strategies::kinds()
        .iter()
        .filter(|d| d.non_prod_only)
        .map(|d| d.kind_id)
        .collect();
    assert_eq!(np, vec![5]);
}

#[test]
fn reg_013_set_params_binding() {
    use base64::Engine as _;
    use hft_ctl::{Core, GatewayConfig, PrincipalTable, Reply, UidKind, VecRing};
    use hft_ctl_proto::{EngineInfo, Role, RoleSet};
    use hft_event::{BlobHdr, CmdHdr, CmdResult, CmdScope, ControlCmd, EventKind};
    use hft_refdata::{
        RefData,
        blob::{AccountRow, RawBlob, sec},
    };
    use hft_rt::{CalibPair, Heartbeat, Mailbox, TscUtc, UtcClock};
    use hft_sequencer::driver::live::{LiveCfg, LiveDriver};
    use hft_sequencer::{BootSource, MemJournal, ParsedConfig, Sequencer, StepOutcome};
    use hft_testkit::{dst::seq::build, seq_rig::RigIo};
    use hft_types::{AccountId, GroupId, StrategyId, Tsc};
    use zerocopy::{FromBytes, FromZeros, IntoBytes};

    for (formal, sim) in [(true, false), (false, false), (false, true)] {
        let w = common::world(0, false, 30_000_000);
        let mut raw = RawBlob::parse(
            &build::refdata_blob_with_names(
                &w,
                sim,
                &[(StrategyId(1), "probe-test")],
                &[(AccountId(1), "acct-test")],
            )
            .unwrap(),
        )
        .unwrap();
        let section = raw.section_mut(sec::ACCOUNTS).unwrap();
        let mut account: AccountRow = section.read_row(0).unwrap();
        account.is_prod = u8::from(formal);
        assert!(section.write_row(0, &account));
        let rd_blob = raw.encode();
        let rd = RefData::load(&rd_blob, None).unwrap();
        let mut setup = build::engine_setup(&w, 1_000_000_000, 1);
        setup.role_prod = !sim;
        let mut configs = build::config_set::<Strategies>(&w, &rd, rd_blob.clone(), &setup).unwrap();
        configs.params.clear();
        let pair = CalibPair {
            tsc: Tsc(0),
            realtime_ns: build::UTC0,
            phc_ns: 0,
            err_ns: 0,
        };
        let io = RigIo::<Strategies> {
            j: MemJournal::new(setup.clock),
            tsc: 0,
            rings: Default::default(),
            n_venues: w.venues.len() as u8,
            out: vec![],
            mailbox: vec![],
            kills: vec![],
            calib: pair,
            hz: 1_000_000_000_000,
        };
        let hb = Box::leak(Box::new(Heartbeat::new()));
        let mut seq = Sequencer::boot(
            BootSource::Empty { configs },
            &setup,
            LiveDriver::new(io, LiveCfg::default()),
            hb,
        )
        .unwrap();
        // A journalled clock observation supplies a nonzero confirmation version.
        let (owner, timer_id, group) = hft_event::TimerCode::Sys(hft_event::SysTimer::ClockBad)
            .encode()
            .unwrap();
        let timer = hft_event::Timer {
            owner,
            timer_id,
            group,
            _pad: 0,
            _pad2: 0,
            deadline_tsc: 0,
        };
        seq.driver_mut()
            .io
            .rings
            .entry((1, 0))
            .or_default()
            .push_back((EventKind::TIMER, timer.as_bytes().to_vec()));
        for _ in 0..100 {
            if seq.step() == StepOutcome::Idle {
                break;
            }
        }
        assert!(seq.engine().risk.mode.ctl_state_version() > 0);
        // Use the frozen single-leg quote form; the world helper normally emits a pair.
        let bytes = build::spb_blob::<Strategies>(&w, &w.strategies[0], 2).unwrap();
        let hlen = std::mem::size_of::<hft_event::StrategyParamsHdr>();
        let glen = std::mem::size_of::<hft_event::GroupSpec>();
        let mut group = hft_event::GroupSpec::read_from_bytes(&bytes[hlen..hlen + glen]).unwrap();
        group.n_legs = 1;
        group.legs[1] = hft_event::LegDef::new_zeroed();
        let mut blob = bytes;
        blob[hlen..hlen + glen].copy_from_slice(group.as_bytes());
        let p = build::parse_spb::<Strategies>(&blob, &rd).unwrap();
        let blob_ref = build::spb_ref(&blob, 2);
        let (send, mut take) = Box::leak(Box::new(Mailbox::new())).split();
        let (_retire_send, retire_take) = Box::leak(Box::new(Mailbox::new())).split();
        let principals = PrincipalTable::from_parts(
            vec![(
                1200,
                UidKind::Cli {
                    login: "tester".into(),
                },
            )],
            vec![("cli:tester".into(), RoleSet::from_roles(&[Role::Admin]))],
        );
        let mut ctl = Core::<Strategies, _>::new(
            GatewayConfig::new("unused.sock".into(), principals, 256, 1024, 8),
            RefData::load(&rd_blob, None).unwrap(),
            VecRing::with_free(262_144),
            send,
            retire_take,
            |_, _| Ok(()),
            |b, _, rd| hft_risk::RiskParams::decode_blob(b, rd).map_err(|_| 1),
            vec![],
            UtcClock::fixed(TscUtc {
                base_tsc: Tsc(0),
                base_utc_ns: build::UTC0,
                mult: 1,
                shift: 0,
                err_ns: 0,
            }),
            EngineInfo {
                version: "test".into(),
                epoch: 1,
                refdata: "test".into(),
                params: Default::default(),
                risk: 1,
            },
        );
        let request = serde_json::json!({
            "v": 2, "cmd_id": "11111111-1111-4111-8111-111111111111", "issuer": "cli:tester",
            "issued_at_ns": build::UTC0, "expires_at_ns": build::UTC0 + 30_000_000_000i64,
            "account": "acct-test", "confirmed": true,
            "expect_state_version": seq.engine().risk.mode.ctl_state_version(),
            "cmd": {"type": "SetParams", "strategy": "probe-test",
                "blob_b64": base64::engine::general_purpose::STANDARD.encode(&blob)},
        });
        let reply = ctl.on_request(1, 1200, &serde_json::to_vec(&request).unwrap(), Tsc(0));
        if formal {
            let Reply::Now(response) = reply else {
                panic!("formal account passed ctl")
            };
            let value = serde_json::to_value(response).unwrap();
            assert_eq!(value["code"], "InvalidArgs", "{value}");
            assert_eq!(value["detail"]["name"], "ParamInvalid", "{value}");
            assert_eq!(value["detail"]["arg"], "NonProdOnly", "{value}");
            assert!(take.try_take().is_none());
            assert!(!ctl.ring().out.iter().any(|r| r.0 == EventKind::CONFIG_BLOB));
        } else {
            assert!(matches!(reply, Reply::Wait), "{reply:?}");
            seq.driver_mut().io.mailbox.push(take.try_take().unwrap());
            for (kind, _, mut bytes) in std::mem::take(&mut ctl.ring_mut().out) {
                if kind == EventKind::CONTROL_CMD || kind == EventKind::CONFIG_BLOB {
                    let (part, body) = hft_event::PartHdr::read_from_prefix(&bytes).unwrap();
                    assert_eq!(part.parts, 1);
                    bytes = body[..part.chunk_len as usize].to_vec();
                }
                seq.driver_mut()
                    .io
                    .rings
                    .entry((0, 0))
                    .or_default()
                    .push_back((kind, bytes));
            }
        }
        if formal {
            // Defence in depth: bypass the cold validator and still reject in the sequencer.
            seq.driver_mut()
                .io
                .mailbox
                .push(ParsedConfig::StrategyParams(Box::new(p)));
            let scope = CmdScope {
                strategy: StrategyId(1),
                account: AccountId(1),
                group: GroupId(0),
                ..CmdScope::new_zeroed()
            };
            let mut bytes = BlobHdr {
                blob: blob_ref,
                scope,
                boot: 0,
                _pad: [0; 7],
            }
            .as_bytes()
            .to_vec();
            bytes.extend_from_slice(&blob);
            let queue = seq.driver_mut().io.rings.entry((0, 0)).or_default();
            queue.push_back((EventKind::CONFIG_BLOB, bytes));
            let mut hdr = CmdHdr::zeroed();
            hdr.cmd_id = [0x11; 16];
            hdr.role = CmdHdr::ROLE_ADMIN;
            hdr.account = AccountId(1);
            hdr.cmd_ver = 1;
            hdr.confirmed = 1;
            hdr.issuer[..10].copy_from_slice(b"cli:tester");
            hdr.expect_state_version = seq.engine().risk.mode.ctl_state_version();
            let mut bytes = vec![0; 8192];
            let n = ControlCmd::SetParams {
                scope,
                blob: blob_ref,
            }
            .encode(&hdr, &mut bytes)
            .unwrap();
            bytes.truncate(n);
            seq.driver_mut()
                .io
                .rings
                .entry((0, 0))
                .or_default()
                .push_back((EventKind::CONTROL_CMD, bytes));
        }
        let mut idle = false;
        for _ in 0..100 {
            match seq.step() {
                StepOutcome::Idle => {
                    idle = true;
                    break;
                }
                StepOutcome::Busy => (),
                other => panic!("unexpected outcome {other:?}"),
            }
        }
        assert!(idle);
        let result = seq
            .driver()
            .io
            .j
            .records()
            .filter(|r| r.kind == EventKind::CMD_RESULT)
            .filter_map(|r| r.read::<CmdResult>())
            .last()
            .unwrap();
        if formal {
            assert_eq!(result.status, hft_ctl_proto::ResultCode::InvalidArgs as u8);
            assert_eq!(result.detail as u16, hft_ctl_proto::codes::detail::PARAM_INVALID);
            assert_eq!((result.detail >> 16) & 0xf, 3);
            assert!(seq.instances().get(StrategyId(1)).unwrap().cur.is_none());
            let slot = seq.instances().get(StrategyId(1)).unwrap();
            assert!(slot.inst.is_none());
            assert_eq!((slot.n_create, slot.n_on_params), (0, 0));
            assert_eq!(result.params_version, 0);
            assert_eq!(result.effect_count, 0);
        } else {
            assert_eq!(
                result.status,
                hft_ctl_proto::ResultCode::Applied as u8,
                "sim={sim}: {result:?}"
            );
            assert!(seq.instances().get(StrategyId(1)).unwrap().cur.is_some());
            assert!(ctl.on_result(&result).is_some());
        }
    }
}
