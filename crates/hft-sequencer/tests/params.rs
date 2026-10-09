//! Sections 3 and 5 (TESTS.md "环与容量": SEQ-001–SEQ-005).

use hft_refdata::{EngineConfig, LoadErrCode, RefData};
use hft_sequencer::params::{CAPS_VALIDATOR, SEQ_VALIDATOR, capacities, seq_params};
use hft_sequencer::{CapacitiesRow, RingClass, RingLimit, SeqParamsRow};
use zerocopy::IntoBytes;

use crate::common::*;

#[test]
fn seq_001_seq_params_defaults() {
    let d = SeqParamsRow::default();
    let rl = |p, t| RingLimit {
        per_ring: p,
        total: t,
    };
    assert_eq!(d.rings[1], rl(4, 4));
    assert_eq!(d.rings[2], rl(4, 4));
    assert_eq!(d.rings[3], rl(32, 64));
    assert_eq!(d.rings[4].per_ring, 32);
    assert_eq!(d.rings[5], rl(32, 128));
    assert_eq!(d.rings[6], rl(16, 16));
    assert_eq!(d.rings[0], rl(0, 0));
    assert_eq!(d.rings[7], rl(0, 0));
    assert_eq!(d.md_round_ns, 50_000);
    assert_eq!(d.sim_max_per_round, 32);
    assert_eq!(d.ctl_bytes_per_round, 8_192);
    assert_eq!(d.conflate_backlog, 64);
    assert_eq!(d.notify_per_continue, 64);
    assert_eq!(d.keyframe_period_ms, 300_000);
    assert_eq!(d.keyframe_max_events, 1_500_000);
    assert_eq!(d.replay_per_round, 1_024);
    assert_eq!(d.auto_resume_after_clean_drain, 1);
    // .17: keyframe_max_events × 60 s ÷ 20 M events/min ≤ 5 s (in ms: 4 500).
    let ms = u64::from(d.keyframe_max_events) * 60_000 / 20_000_000;
    assert_eq!(ms, 4_500);
    assert!(ms <= 5_000);
}

#[test]
fn seq_002_seq_params_layout() {
    use core::mem::{align_of, offset_of, size_of};
    assert_eq!(size_of::<RingLimit>(), 4);
    assert_eq!(align_of::<RingLimit>(), 2);
    assert_eq!(size_of::<SeqParamsRow>(), 72);
    assert_eq!(align_of::<SeqParamsRow>(), 4);
    assert_eq!(offset_of!(SeqParamsRow, rings), 0);
    assert_eq!(offset_of!(SeqParamsRow, md_round_ns), 32);
    assert_eq!(offset_of!(SeqParamsRow, ctl_bytes_per_round), 36);
    assert_eq!(offset_of!(SeqParamsRow, keyframe_period_ms), 40);
    assert_eq!(offset_of!(SeqParamsRow, keyframe_max_events), 44);
    assert_eq!(offset_of!(SeqParamsRow, advisor_mem_budget_mib), 48);
    assert_eq!(offset_of!(SeqParamsRow, advisor_ckpt_max_mib), 52);
    assert_eq!(offset_of!(SeqParamsRow, sim_max_per_round), 56);
    assert_eq!(offset_of!(SeqParamsRow, conflate_backlog), 58);
    assert_eq!(offset_of!(SeqParamsRow, notify_per_continue), 60);
    assert_eq!(offset_of!(SeqParamsRow, replay_per_round), 62);
    assert_eq!(offset_of!(SeqParamsRow, auto_resume_after_clean_drain), 64);
    assert_eq!(offset_of!(SeqParamsRow, allow_missing_standby), 65);
    assert_eq!(offset_of!(SeqParamsRow, _pad), 66);
}

fn rd() -> RefData {
    b0_rd().load()
}

fn load_validate(blob: &[u8], rd: &RefData) -> Result<EngineConfig, LoadErrCode> {
    let ec = EngineConfig::load(blob, None).map_err(|r| r.code())?;
    ec.validate(rd, &[CAPS_VALIDATOR, SEQ_VALIDATOR])
        .map_err(|r| r.code())?;
    Ok(ec)
}

#[test]
fn seq_003_seq_params_roundtrip() {
    let rd = rd();
    let row = b0_seq();
    // .1
    let ec = load_validate(&engine_blob(&b0_caps(), &row), &rd).unwrap();
    let got = seq_params(&ec).unwrap();
    assert_eq!(got.as_bytes(), row.as_bytes());
    assert_eq!(got, row);
    // .2 row_size 80 with 8 trailing 0xAA.
    let mut long = row.as_bytes().to_vec();
    long.extend_from_slice(&[0xAA; 8]);
    let ec = load_validate(&engine_blob_rows(1, b0_caps().as_bytes(), &long, 1, 80), &rd).unwrap();
    assert_eq!(seq_params(&ec).unwrap(), row);
    // .3 two rows.
    let mut two = row.as_bytes().to_vec();
    two.extend_from_slice(row.as_bytes());
    assert_eq!(
        load_validate(&engine_blob_rows(1, b0_caps().as_bytes(), &two, 2, 72), &rd).err(),
        Some(LoadErrCode::Structure)
    );
    // .4 zero rows.
    assert_eq!(
        load_validate(&engine_blob_rows(1, b0_caps().as_bytes(), &[], 0, 72), &rd).err(),
        Some(LoadErrCode::Structure)
    );
}

#[test]
fn seq_004_engine_validator() {
    let rd = rd();
    let check = |s: SeqParamsRow, c: CapacitiesRow| load_validate(&engine_blob(&c, &s), &rd).map(|_| ());
    let vr = Err(LoadErrCode::ValueRange);
    let s = |f: &dyn Fn(&mut SeqParamsRow)| {
        let mut x = b0_seq();
        f(&mut x);
        x
    };
    let c = |f: &dyn Fn(&mut CapacitiesRow)| {
        let mut x = b0_caps();
        f(&mut x);
        x
    };
    let rl = |p, t| RingLimit {
        per_ring: p,
        total: t,
    };
    let caps = b0_caps();
    assert_eq!(check(s(&|x| x.auto_resume_after_clean_drain = 2), caps), vr);
    assert_eq!(check(s(&|x| x.allow_missing_standby = 1), caps), Ok(()));
    assert_eq!(check(s(&|x| x._pad[0] = 1), caps), vr);
    assert_eq!(check(s(&|x| x.rings[0] = rl(1, 1)), caps), vr);
    assert_eq!(check(s(&|x| x.rings[7] = rl(0, 1)), caps), vr);
    assert_eq!(check(s(&|x| x.rings[5] = rl(0, 128)), caps), vr);
    assert_eq!(check(s(&|x| x.rings[5] = rl(127, 128)), caps), Ok(()));
    assert_eq!(check(s(&|x| x.rings[5] = rl(128, 128)), caps), Ok(()));
    assert_eq!(check(s(&|x| x.rings[5] = rl(129, 128)), caps), vr);
    assert_eq!(check(s(&|x| x.rings[4] = rl(32, 0)), caps), Ok(()));
    assert_eq!(check(s(&|x| x.md_round_ns = 0), caps), vr);
    assert_eq!(check(s(&|x| x.md_round_ns = 1), caps), Ok(()));
    assert_eq!(check(s(&|x| x.keyframe_max_events = 0), caps), vr);
    assert_eq!(check(s(&|x| x.replay_per_round = 0), caps), vr);
    assert_eq!(check(s(&|x| x.ctl_bytes_per_round = 0), caps), vr);
    let seq = b0_seq();
    assert_eq!(check(seq, c(&|x| x.max_insts = 8191)), Ok(()));
    assert_eq!(check(seq, c(&|x| x.max_insts = 8192)), Ok(()));
    assert_eq!(check(seq, c(&|x| x.max_insts = 8193)), vr);
    assert_eq!(check(seq, c(&|x| x.max_timers = 0)), vr);
    let _ = capacities;
}

#[test]
fn seq_005_ring_class_codes() {
    assert_eq!(RingClass::try_from(1u8), Ok(RingClass::Ctl));
    assert_eq!(RingClass::try_from(2u8), Ok(RingClass::Hk));
    assert_eq!(RingClass::try_from(3u8), Ok(RingClass::Rep));
    assert_eq!(RingClass::try_from(4u8), Ok(RingClass::Chain));
    assert_eq!(RingClass::try_from(5u8), Ok(RingClass::Bbo));
    assert_eq!(RingClass::try_from(6u8), Ok(RingClass::Advice));
    assert!(RingClass::try_from(0u8).is_err());
    assert!(RingClass::try_from(7u8).is_err());
    assert!(RingClass::try_from(8u8).is_err());
}
