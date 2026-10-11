use crate::common::*;
use hft_event::{BlobHdr, BlobKind, EventKind};
use hft_refdata::{AccountKind, EcfBuilder, EngineConfig, EngineSection};
use hft_sequencer::driver::live::LiveDriver;
use hft_sequencer::{BootSource, EngineSetup, Sequencer};
use hft_types::{AccountId, HostClass, VenueId};

fn full_setup(rd: &hft_refdata::RefData) -> EngineSetup {
    let mut s = b0_setup(rd);
    s.keys_required.clear();
    for id in 1..=u8::MAX {
        let a = rd.account(AccountId(id));
        if a.id.0 == 0 || a.kind == AccountKind::Sim {
            continue;
        }
        for v in 1..64u8 {
            if a.venues & (1u64 << v) != 0 {
                s.keys_required.push((a.id, VenueId(v), 3));
            }
        }
    }
    s
}

#[test]
fn seq_376_complete_config_roundtrip() {
    let rd = b0_rd().load();
    let mut s = full_setup(&rd);
    s.gws.venues.reverse(); // gw indexes are kill-word identities, not a set to re-sort.
    s.oms.hedge_deadline_ms = 12_345;
    s.oms.slots.set(A, AC1, 47);
    s.ledger.funding_tol_abs.0 += 41;
    s.exec.accounts[0].params = Some(hft_exec::ExecParams {
        broken_auto_fix_ms: 456_789,
        ..s.exec.default
    });
    s.advisor_max_lag_ms = 6789;
    s.drain_timeout_ms = 3456;
    let b = s.encode_blob(7, HostClass::Dev2, UTC0);
    let q = EngineSetup::decode_blob(&b, &rd, s.clock, s.oms.engine_epoch, s.base_engine_ns).unwrap();
    assert_eq!(q.caps, s.caps);
    assert_eq!(q.seq, s.seq);
    assert_eq!(q.oms, s.oms);
    assert_eq!(q.ledger, s.ledger);
    assert_eq!(q.exec, s.exec);
    assert_eq!(q.gws.venues, s.gws.venues);
    assert_eq!(q.feeds, s.feeds);
    assert_eq!(q.keys_required, s.keys_required);
    assert_eq!(
        (q.role_prod, q.advisor_max_lag_ms, q.drain_timeout_ms),
        (s.role_prod, s.advisor_max_lag_ms, s.drain_timeout_ms)
    );
    assert_eq!(b, q.encode_blob(7, HostClass::Dev2, UTC0));
}

#[test]
fn seq_377_missing_config_does_not_use_defaults() {
    let rd = b0_rd().load();
    let s = full_setup(&rd);
    let ec = EngineConfig::load(&s.encode_blob(1, HostClass::Dev2, UTC0), None).unwrap();
    for missing in [
        EngineSection::Oms,
        EngineSection::Ledger,
        EngineSection::Capacities,
        EngineSection::Sequencer,
        EngineSection::Exec,
        EngineSection::Assembly,
    ] {
        let mut b = EcfBuilder::new(1, HostClass::Dev2);
        for tag in EngineSection::ALL {
            if tag == missing {
                continue;
            }
            if let Some(v) = ec.section(tag) {
                b.section(tag, v.row_ver, v.row_size, v.count, v.bytes.to_vec());
            }
        }
        assert!(
            EngineSetup::decode_blob(&b.finish(), &rd, s.clock, 1, 0).is_err(),
            "missing {missing:?}"
        );
    }
    for mutation in 0..5 {
        let mut s = s.clone();
        match mutation {
            0 => s.keys_required.clear(),
            1 => s.gws.venues.push(s.gws.venues[0]),
            2 => s.keys_required[0].2 = 4,
            3 => {
                s.exec.accounts.remove(0);
            }
            _ => s.feeds.0 |= 1u64 << 63,
        };
        assert!(
            EngineSetup::decode_blob(&s.encode_blob(1, HostClass::Dev2, UTC0), &rd, s.clock, 1, 0).is_err(),
            "mutation {mutation}"
        );
    }
}

#[test]
fn seq_378_replay_config_only_from_journal() {
    let b = b0();
    let rd = b.rd.load();
    let setup = full_setup(&rd);
    let mut configs = b.configs(&rd);
    configs.engine_blob = setup.encode_blob(1, HostClass::Dev2, UTC0);
    configs.risk_blob = configs.risk.encode_blob().unwrap();
    configs.risk_sha256 = sha(&configs.risk_blob);
    let heartbeat = hb();
    let drv = LiveDriver::new(TestIo::<TestSet>::new(setup.clock), b.live);
    let s = Sequencer::boot(BootSource::Empty { configs }, &setup, drv, heartbeat).unwrap();
    let mut h = H {
        s,
        rd,
        setup,
        cmd_no: 0,
        hb: heartbeat,
        upd: 1,
    };
    h.steady();
    h.keyframe();
    for k in 0..12 {
        h.quote(A1, "50000", if k % 2 == 0 { "50003" } else { "50004" });
        h.run();
    }
    h.keyframe();
    // Drop every assembler configuration value. Decode fresh from the journal's original boot
    // block, then use the same VerifyDriver as replayer, including all output snapshots.
    let recs = h.recs();
    let mut rd_blob = None;
    let mut ec_blob = None;
    for r in &recs {
        if r.kind != EventKind::CONFIG_BLOB {
            continue;
        }
        let bh = r.read::<BlobHdr>().unwrap();
        let data = r.body[72..72 + bh.blob.len as usize].to_vec();
        match bh.blob.kind {
            BlobKind::Refdata => rd_blob = Some(data),
            BlobKind::Engine => ec_blob = Some(data),
            _ => {}
        }
    }
    let rd = hft_refdata::RefData::load(&rd_blob.unwrap(), None).unwrap();
    let clock = h.j().segments[0].clock;
    let setup = EngineSetup::decode_blob(&ec_blob.unwrap(), &rd, clock, 1, 0).unwrap();
    assert!(verify_journal::<TestSet>(h.j(), &setup, None).is_empty());
    // A physically complete configuration carries all three component sections, not a marker.
    assert!(
        recs.iter()
            .filter(|r| r.kind == EventKind::CONFIG_BLOB)
            .any(|r| r.body.windows(8).any(|w| w == hft_risk::RISK_BLOB_MAGIC))
    );
}
