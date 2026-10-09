//! Layout and code tables (JNL-001 – JNL-004, JNL-008).

#![allow(
    clippy::disallowed_types,
    clippy::disallowed_macros,
    clippy::disallowed_methods
)] // allow-list: test code, not a hot path

use crate::common;

use std::mem::{align_of, offset_of, size_of};

use common::harness::*;
use hft_event::{PartHdr, SectionHdr, SnapshotHdr};
use hft_journal::*;
use hft_types::Tsc;

#[test]
fn jnl_001_segment_header_layout() {
    type S = SegmentHeader;
    assert_eq!((size_of::<S>(), align_of::<S>()), (4096, 8), "JNL-001.1");
    assert_eq!(
        [
            offset_of!(S, magic),
            offset_of!(S, format_version),
            offset_of!(S, journal_kind),
            offset_of!(S, venue),
            offset_of!(S, header_len)
        ],
        [0, 8, 10, 11, 12],
        "JNL-001.2"
    );
    assert_eq!(
        [
            offset_of!(S, journal_name),
            offset_of!(S, instance),
            offset_of!(S, segment_no),
            offset_of!(S, first_seq),
            offset_of!(S, prev_last_seq)
        ],
        [16, 48, 64, 72, 80],
        "JNL-001.3"
    );
    assert_eq!(
        [
            offset_of!(S, created_utc_ns),
            offset_of!(S, created_tsc),
            offset_of!(S, segment_bytes),
            offset_of!(S, writer_epoch),
            offset_of!(S, roll_reason),
            offset_of!(S, flags),
            offset_of!(S, _pad0)
        ],
        [88, 96, 104, 112, 116, 117, 118],
        "JNL-001.4"
    );
    assert_eq!(
        [
            offset_of!(S, tsc_hz_milli),
            offset_of!(S, calib_tsc),
            offset_of!(S, calib_realtime_ns),
            offset_of!(S, calib_phc_ns),
            offset_of!(S, calib_err_ns),
            offset_of!(S, clock_max_error_ns)
        ],
        [120, 128, 136, 144, 152, 156],
        "JNL-001.5"
    );
    assert_eq!(
        [
            offset_of!(S, prev_header_sha256),
            offset_of!(S, host_profile_sha256),
            offset_of!(S, build_git),
            offset_of!(S, _pad1),
            offset_of!(S, n_schema),
            offset_of!(S, n_config),
            offset_of!(S, _pad2)
        ],
        [160, 192, 224, 244, 248, 250, 252],
        "JNL-001.6"
    );
    assert_eq!(size_of::<SchemaEntry>(), 8, "JNL-001.7");
    assert_eq!(
        [
            offset_of!(S, schema),
            offset_of!(S, config_digest),
            offset_of!(S, _rsv),
            offset_of!(S, header_crc32c)
        ],
        [256, 1280, 1312, 3068],
        "JNL-001.7"
    );
    assert_eq!(size_of::<[SchemaEntry; 128]>(), 1024, "JNL-001.7");
    assert_eq!(size_of::<[u8; 1756]>(), 3068 - 1312, "JNL-001.7");
    assert_eq!(
        [
            offset_of!(S, committed_len),
            offset_of!(S, last_seq),
            offset_of!(S, state),
            offset_of!(S, _w)
        ],
        [3072, 3080, 3088, 3092],
        "JNL-001.8"
    );
    assert_eq!(
        [
            offset_of!(S, persisted_len),
            offset_of!(S, synced_len),
            offset_of!(S, retired),
            offset_of!(S, _p),
            offset_of!(S, _tail)
        ],
        [3136, 3144, 3152, 3156, 3200],
        "JNL-001.9"
    );
    assert_eq!(size_of::<SegmentHeaderFixed>(), 3072, "JNL-001.10");
    assert_eq!(offset_of!(SegmentHeaderFixed, header_crc32c), 3068, "JNL-001.10");
}

#[test]
fn jnl_002_header_crc_scope() {
    let mut h = H::open0(mem_c0());
    for i in 0..10 {
        h.w().append(F0, Tsc(i), &tin(i as u8)).unwrap();
        h.w().end_event();
    }
    h.w().publish();
    let p = h.seg_path(1);
    let img = h.seg(1);
    assert_eq!(u32_at(&img, 3068), crc32c::crc32c(&img[..3068]), "JNL-002.1");
    let mut r = h.reader(StartAt::Oldest);
    assert_eq!(drain(&mut r).len(), 12, "JNL-002.1");
    drop(r);
    let jdir = h.hot_jdir();
    for (row, off, header_issue) in [
        ("JNL-002.2", 64usize, true),
        ("JNL-002.3", 3067, true),
        ("JNL-002.4", 3080, false),
        ("JNL-002.5", 3160, false),
    ] {
        let orig = img[off];
        hft_journal::testing::poke(&p, off, &[orig ^ 1]).unwrap();
        let mut r = h.reader(StartAt::Oldest);
        let rep = verify(&jdir, &VerifyOpts::default());
        let hdr_issues: Vec<_> = rep.issues.iter().filter(|i| i.item == 1).copied().collect();
        if header_issue {
            assert!(matches!(r.poll(), Next::Err(ReadErr::Corrupt)), "{row}");
            assert_eq!(
                hdr_issues,
                vec![VerifyIssue {
                    segment_no: 1,
                    offset: 0,
                    item: 1
                }],
                "{row}"
            );
        } else {
            assert_eq!(drain(&mut r).len(), 12, "{row}");
            assert!(hdr_issues.is_empty(), "{row}");
        }
        drop(r);
        hft_journal::testing::poke(&p, off, &[orig]).unwrap();
    }
}

#[test]
fn jnl_003_seal_record_layout() {
    type S = SealRecord;
    assert_eq!(size_of::<S>(), 128, "JNL-003.1");
    assert_eq!(
        [
            offset_of!(S, magic),
            offset_of!(S, segment_no),
            offset_of!(S, first_seq),
            offset_of!(S, last_seq),
            offset_of!(S, len),
            offset_of!(S, sealed_utc_ns)
        ],
        [0, 8, 16, 24, 32, 40],
        "JNL-003.2"
    );
    assert_eq!(
        [
            offset_of!(S, sha256),
            offset_of!(S, reason),
            offset_of!(S, flags),
            offset_of!(S, _pad),
            offset_of!(S, _rsv),
            offset_of!(S, crc32c)
        ],
        [48, 80, 81, 82, 84, 124],
        "JNL-003.3"
    );
    assert_eq!(SEAL_MAGIC, *b"HFTSEAL1", "JNL-003.4");
}

#[test]
fn jnl_004_part_snapshot_section_layout() {
    assert_eq!(size_of::<PartHdr>(), 16, "JNL-004.1");
    assert_eq!(
        [
            offset_of!(PartHdr, total_len),
            offset_of!(PartHdr, chunk_len),
            offset_of!(PartHdr, part),
            offset_of!(PartHdr, parts),
            offset_of!(PartHdr, _rsv)
        ],
        [0, 4, 8, 10, 12],
        "JNL-004.1"
    );
    type N = SnapshotHdr;
    assert_eq!(size_of::<N>(), 96, "JNL-004.2");
    assert_eq!(
        [
            offset_of!(N, purpose),
            offset_of!(N, _pad),
            offset_of!(N, n_sections),
            offset_of!(N, at_seq),
            offset_of!(N, state_hash)
        ],
        [0, 1, 4, 8, 16],
        "JNL-004.2"
    );
    assert_eq!(
        [
            offset_of!(N, n_books),
            offset_of!(N, _pad2),
            offset_of!(N, out_hash),
            offset_of!(N, snap_tsc),
            offset_of!(N, boot_id),
            offset_of!(N, snap_utc_ns),
            offset_of!(N, engine_ns)
        ],
        [32, 36, 40, 56, 64, 80, 88],
        "JNL-004.3"
    );
    assert_eq!(size_of::<SectionHdr>(), 16, "JNL-004.4");
    assert_eq!(
        [
            offset_of!(SectionHdr, section),
            offset_of!(SectionHdr, version),
            offset_of!(SectionHdr, count),
            offset_of!(SectionHdr, len),
            offset_of!(SectionHdr, _rsv)
        ],
        [0, 2, 4, 8, 12],
        "JNL-004.4"
    );
    let purposes: Vec<(&str, u32)> = hft_event::codes::SNAPSHOT_PURPOSE
        .items
        .iter()
        .map(|i| (i.name, i.value))
        .collect();
    assert_eq!(
        purposes,
        vec![
            ("Boot", 1),
            ("Keyframe", 2),
            ("OnDemand", 3),
            ("Recovery", 4),
            ("Drain", 5)
        ],
        "JNL-004.5"
    );
}

#[test]
fn jnl_008_code_tables() {
    assert_eq!(
        [
            JournalKind::Raw as u8,
            JournalKind::Seq as u8,
            JournalKind::Chain as u8
        ],
        [1, 2, 3],
        "JNL-008.1"
    );
    assert_eq!(
        [
            SegState::Preparing as u32,
            SegState::Open as u32,
            SegState::Sealed as u32
        ],
        [0, 1, 2],
        "JNL-008.2"
    );
    assert_eq!(RECOVERY_GAP, 1, "JNL-008.3");
    assert_eq!(
        [InstanceName::Prod as u8, InstanceName::Sim as u8],
        [1, 2],
        "JNL-008.4"
    );
    assert_eq!(
        [
            AppendErr::Full as u8,
            AppendErr::TooLarge as u8,
            AppendErr::Sealed as u8,
            AppendErr::InBoot as u8
        ],
        [1, 2, 3, 4],
        "JNL-008.5"
    );
    assert_eq!(
        [
            RollErr::NextNotReady as u8,
            RollErr::Io as u8,
            RollErr::InBoot as u8
        ],
        [1, 2, 3],
        "JNL-008.5"
    );
    assert_eq!(
        [RollAt::IdleRound as u8, RollAt::EventBoundary as u8],
        [1, 2],
        "JNL-008.5"
    );
    assert_eq!(
        [
            OpenErr::DiskBacked as u8,
            OpenErr::OfflineTarget as u8,
            OpenErr::BadConfig as u8,
            OpenErr::Io as u8,
            OpenErr::ChainBroken as u8,
            OpenErr::ColdHotConflict as u8,
            OpenErr::SeqUnavailable as u8,
            OpenErr::NotFound as u8
        ],
        [1, 2, 3, 4, 5, 6, 7, 8],
        "JNL-008.6"
    );
    assert_eq!(
        [
            ReadErr::Corrupt as u8,
            ReadErr::BadCrc as u8,
            ReadErr::ReassemblyCap as u8,
            ReadErr::DevKind as u8
        ],
        [1, 2, 3, 4],
        "JNL-008.7"
    );
    assert_eq!(
        [
            GapReason::MissingSegment as u8,
            GapReason::RecoveryGap as u8,
            GapReason::ChainBroken as u8,
            GapReason::Deleted as u8
        ],
        [1, 2, 3, 4],
        "JNL-008.7"
    );
    assert_eq!(
        [
            PrepState::Ready as u8,
            PrepState::Prepared as u8,
            PrepState::Failed as u8
        ],
        [1, 2, 3],
        "JNL-008.7"
    );
}
