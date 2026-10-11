use crate::common::*;
use hft_event::{BlobHdr, BlobKind, BlobRef, ControlCmd, EventKind, RecordFlags};
use hft_sequencer::driver::replay::{Log, ReplayDriver, ReplayMode};
use hft_sequencer::driver::{Driver, ParseCx};
use hft_sequencer::{ConfigErr, MemRecord, ParsedConfig};
use zerocopy::IntoBytes;

fn blob_record(reference: BlobRef, bytes: &[u8]) -> MemRecord {
    let h = BlobHdr {
        blob: reference,
        scope: scope(S1, AC1, hft_types::GroupId(0)),
        boot: 0,
        _pad: [0; 7],
    };
    let mut body = h.as_bytes().to_vec();
    body.extend_from_slice(bytes);
    MemRecord {
        seq: 1,
        kind: EventKind::CONFIG_BLOB,
        flags: RecordFlags::EVENT_END,
        tsc: 1,
        body,
    }
}

fn log(records: Vec<MemRecord>) -> Log {
    Log {
        recs: records,
        ..Log::default()
    }
}

#[test]
fn seq_388_replay_config_is_visible_only_in_consumed_prefix() {
    let rd = b0_rd().load();
    let p = Spb::s1(2);
    let reference = p.blob_ref();
    let bytes = p.bytes();
    let record = blob_record(reference, &bytes);
    let cx = ParseCx::<TestSet> {
        rd: &rd,
        max_groups: 8,
        base: None,
        risk_account: AC1,
        risk_decode: Some(risk_decode),
    };
    for mode in [ReplayMode::Verify, ReplayMode::Recover, ReplayMode::Mirror] {
        let mut d = ReplayDriver::new(mode, log(vec![record.clone()]), 0, 1);
        assert!(matches!(d.take_parsed(&reference, &cx), Err(ConfigErr::Missing)));
        assert!(d.blob(&reference.sha256).is_none());
        <ReplayDriver as Driver<TestSet>>::start_at(&mut d, 1);
        assert!(matches!(
            d.take_parsed(&reference, &cx),
            Ok(ParsedConfig::StrategyParams(_))
        ));
        assert_eq!(d.blob(&reference.sha256), Some(bytes.as_slice()));
        <ReplayDriver as Driver<TestSet>>::start_at(&mut d, 0);
        assert!(matches!(d.take_parsed(&reference, &cx), Err(ConfigErr::Missing)));

        // Appending a mirror batch cannot grant early access to any record in that batch.
        let mut d = ReplayDriver::new(mode, Log::default(), 0, 1);
        d.extend(std::slice::from_ref(&record), &[], &[]);
        assert!(matches!(d.take_parsed(&reference, &cx), Err(ConfigErr::Missing)));
        <ReplayDriver as Driver<TestSet>>::start_at(&mut d, 1);
        assert!(d.take_parsed(&reference, &cx).is_ok());

        for row in 0..5 {
            let mut wanted = reference;
            match row {
                0 => wanted.kind = BlobKind::Refdata,
                1 => wanted.encoding += 1,
                2 => wanted.len += 1,
                3 => wanted.version += 1,
                _ => wanted.sha256[0] ^= 1,
            }
            assert!(
                matches!(d.take_parsed(&wanted, &cx), Err(ConfigErr::Missing)),
                "{mode:?} row {row}"
            );
        }
        for damage in 0..2 {
            let mut bad = record.clone();
            if damage == 0 {
                bad.body[size_of::<BlobHdr>()] ^= 1;
            } else {
                bad.body.pop();
            }
            // An older valid copy must not hide a later corrupt copy of the same reference.
            bad.seq = 2;
            let mut d = ReplayDriver::new(mode, log(vec![record.clone(), bad]), 2, 1);
            assert!(
                d.take_parsed(&reference, &cx).is_err(),
                "{mode:?} damage {damage}"
            );
            assert!(d.blob(&reference.sha256).is_none());
        }
        // Equal content hashes do not erase the identity of an earlier reference.
        let mut other = reference;
        other.version += 1;
        let mut d = ReplayDriver::new(mode, log(vec![record.clone(), blob_record(other, &bytes)]), 2, 1);
        assert!(d.take_parsed(&reference, &cx).is_ok());
        assert!(d.take_parsed(&other, &cx).is_ok());
        d.end = 0;
        assert!(matches!(d.take_parsed(&reference, &cx), Err(ConfigErr::Missing)));
    }
}

#[test]
fn seq_389_verify_rejects_a_command_satisfied_by_future_config() {
    for future in [false, true] {
        reset_script();
        let mut h = b0().boot();
        h.steady();
        let p = Spb::s1(2);
        let reference = p.blob_ref();
        let bytes = p.bytes();
        let sc = scope(S1, AC1, hft_types::GroupId(0));
        // Deliberately bypass the gateway's ConfigBlob-before-command rule for the negative
        // row. Its ready Mailbox object lets the live fixture log Applied either way.
        let parsed =
            hft_sequencer::parsed::parse_params::<TestSet>(&bytes, reference.sha256, &h.rd, 8, None).unwrap();
        h.io()
            .mailbox
            .push(ParsedConfig::StrategyParams(Box::new(parsed)));
        if !future {
            h.blob_input(reference, sc, &bytes);
            h.run();
        }
        let id = h.cmd_confirmed(
            AC1,
            &ControlCmd::SetParams {
                scope: sc,
                blob: reference,
            },
        );
        h.run();
        assert_eq!(h.res(id).0, hft_sequencer::codes::status::APPLIED);
        if future {
            h.blob_input(reference, sc, &bytes);
            h.run();
        }
        let result_seq = h
            .j()
            .records()
            .find(|r| {
                r.kind == EventKind::CMD_RESULT
                    && r.read::<hft_event::CmdResult>().is_some_and(|c| c.cmd_id == id)
            })
            .unwrap()
            .seq;
        let mismatches = h.verify();
        if future {
            assert!(
                mismatches
                    .iter()
                    .any(|m| m.seq == result_seq && m.kind == EventKind::CMD_RESULT)
            );
        } else {
            assert!(mismatches.is_empty(), "{mismatches:?}");
        }
    }
}
