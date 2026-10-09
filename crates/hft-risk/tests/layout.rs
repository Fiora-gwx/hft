//! Snapshot row layouts and code values (RSK-008, RSK-009; crates/hft-journal/TESTS.md JNL-005,
//! JNL-006: the section 14 and 18 rows defined in docs/subsystems/journal-format.md).
#![allow(
    clippy::disallowed_types,
    clippy::disallowed_macros,
    clippy::disallowed_methods
)] // allow-list: test code, not a hot path

use crate::common;
use common::*;

use core::mem::{align_of, offset_of, size_of};
use hft_risk::mode::{CapLevel, RecoveryFlags, cfg_params, kill_params, recon_flags};
use hft_risk::snapshot::*;
use hft_risk::{Origin, RiskTimer, eq_flags, mflags};
use hft_strategy_sdk::{SnapshotReader, SnapshotWriter};
use hft_types::{AccountId, VenueId};
use zerocopy::IntoBytes;

#[test]
fn rsk_008_snapshot_row_layouts() {
    assert_eq!((size_of::<RiskRowHdr>(), align_of::<RiskRowHdr>()), (8, 4));
    assert_eq!(
        [
            offset_of!(RiskRowHdr, kind),
            offset_of!(RiskRowHdr, account),
            offset_of!(RiskRowHdr, venue),
            offset_of!(RiskRowHdr, _pad),
            offset_of!(RiskRowHdr, inst)
        ],
        [0, 1, 2, 3, 4]
    );
    assert_eq!(size_of::<RiskAcctRow>(), 40);
    assert_eq!(
        [
            offset_of!(RiskAcctRow, day_pnl_base),
            offset_of!(RiskAcctRow, loss_trigger),
            offset_of!(RiskAcctRow, loss_quota),
            offset_of!(RiskAcctRow, equity_sod),
            offset_of!(RiskAcctRow, resumes_today),
            offset_of!(RiskAcctRow, eq_flags),
            offset_of!(RiskAcctRow, _pad)
        ],
        [0, 8, 16, 24, 32, 33, 34]
    );
    assert_eq!(size_of::<RiskBucketRow>(), 40);
    assert_eq!(
        [
            offset_of!(RiskBucketRow, tokens_10s),
            offset_of!(RiskBucketRow, tokens_min),
            offset_of!(RiskBucketRow, last_ens),
            offset_of!(RiskBucketRow, backoff_until_ens),
            offset_of!(RiskBucketRow, backoff_k),
            offset_of!(RiskBucketRow, _pad)
        ],
        [0, 8, 16, 24, 32, 36]
    );
    assert_eq!(
        (size_of::<RiskLevRow>(), offset_of!(RiskLevRow, leverage)),
        (8, 0)
    );
    assert_eq!(size_of::<ReconRow>(), 16);
    assert_eq!(
        [
            offset_of!(ReconRow, ignore_streak),
            offset_of!(ReconRow, warn_streak),
            offset_of!(ReconRow, _pad),
            offset_of!(ReconRow, last_diff)
        ],
        [0, 1, 2, 8]
    );
    assert_eq!(size_of::<ReconGateRowHdr>(), 32);
    assert_eq!(
        [
            offset_of!(ReconGateRowHdr, report_id),
            offset_of!(ReconGateRowHdr, disposed_mask),
            offset_of!(ReconGateRowHdr, n_items),
            offset_of!(ReconGateRowHdr, flags),
            offset_of!(ReconGateRowHdr, _pad),
            offset_of!(ReconGateRowHdr, view_seq)
        ],
        [0, 8, 16, 18, 20, 24]
    );
    assert_eq!(size_of::<hft_risk::mode::ReconItemKey>(), 8);
    assert_eq!(size_of::<RiskUsRow>(), 24);
    assert_eq!(
        [
            offset_of!(RiskUsRow, us_up_seq),
            offset_of!(RiskUsRow, prev_view_seq),
            offset_of!(RiskUsRow, uflags),
            offset_of!(RiskUsRow, _pad)
        ],
        [0, 8, 16, 17]
    );
    assert_eq!(size_of::<MarginRow>(), 96);
    assert_eq!(
        [
            offset_of!(MarginRow, account),
            offset_of!(MarginRow, venue),
            offset_of!(MarginRow, level),
            offset_of!(MarginRow, mflags),
            offset_of!(MarginRow, _pad),
            offset_of!(MarginRow, report_seq),
            offset_of!(MarginRow, report_ens),
            offset_of!(MarginRow, reported_ratio),
            offset_of!(MarginRow, est_ratio),
            offset_of!(MarginRow, rep_liq_dist),
            offset_of!(MarginRow, est_liq_dist),
            offset_of!(MarginRow, l2_since_ens),
            offset_of!(MarginRow, deadline_ens),
            offset_of!(MarginRow, next_force_ens),
            offset_of!(MarginRow, mark_ens),
            offset_of!(MarginRow, stale_due_ens)
        ],
        [0, 1, 2, 3, 4, 8, 16, 24, 32, 40, 48, 56, 64, 72, 80, 88]
    );
    // Field sums equal the sizes (no implicit padding): zerocopy's IntoBytes derive already
    // rejects padding at compile time; recheck the sums.
    assert_eq!(1 + 1 + 1 + 1 + 4, size_of::<RiskRowHdr>());
    assert_eq!(4 * 8 + 1 + 1 + 6, size_of::<RiskAcctRow>());
    assert_eq!(4 * 8 + 4 + 4, size_of::<RiskBucketRow>());
    assert_eq!(1 + 1 + 6 + 8, size_of::<ReconRow>());
    assert_eq!(8 + 8 + 2 + 2 + 4 + 8, size_of::<ReconGateRowHdr>());
    assert_eq!(8 + 8 + 1 + 7, size_of::<RiskUsRow>());
    assert_eq!(4 + 4 + 11 * 8, size_of::<MarginRow>());
}

/// A snapshot with one section-17 row of `kind`, or one margin row with `level`.
fn bad_snapshot(kind: Option<u8>, level: Option<u8>) -> Vec<u8> {
    let mut w = SnapshotWriter::new(1 << 16);
    if let Some(k) = kind {
        w.begin_section(SECTION_RISK_COUNTERS, RISK_COUNTERS_VERSION);
        let h = RiskRowHdr {
            kind: k,
            account: 1,
            venue: B.0,
            _pad: 0,
            inst: 0,
        };
        w.put_bytes(h.as_bytes());
        w.put_bytes(&[0u8; 40]);
        w.add_rows(1);
    }
    if let Some(l) = level {
        w.begin_section(SECTION_MARGIN, MARGIN_VERSION);
        w.put_row(&MarginRow {
            account: 1,
            venue: B.0,
            level: l,
            ..MarginRow::default()
        });
    }
    w.end_section();
    w.as_bytes().to_vec()
}

#[test]
fn rsk_009_code_values() {
    // .1
    assert_eq!(
        [
            row_kind::ACCT,
            row_kind::BUCKET,
            row_kind::RECON,
            row_kind::RECON_GATE,
            row_kind::LEV,
            row_kind::USER_STREAM
        ],
        [1, 2, 3, 6, 7, 8]
    );
    // .2
    assert_eq!(
        [
            mflags::TRUNCATED,
            mflags::RATIO_L2,
            mflags::DEADLINE_L3,
            mflags::HAS_REPORT
        ],
        [1, 2, 4, 8]
    );
    // .3
    assert_eq!((eq_flags::SOD_VALID, us_flags::PREV_US_DOWN), (1, 1));
    // .4
    assert_eq!(
        [
            recon_flags::TRUNCATED,
            recon_flags::INFLIGHT,
            recon_flags::USERSTREAM_DOWN,
            recon_flags::OVERFLOW
        ],
        [1, 2, 4, 8]
    );
    // .5
    assert_eq!(
        [
            Origin::Strategy as u8,
            Origin::Exec as u8,
            Origin::Oms as u8,
            Origin::Command as u8
        ],
        [1, 2, 3, 4]
    );
    // .6
    assert_eq!(
        [
            CapLevel::None as u8,
            CapLevel::CloseOnly as u8,
            CapLevel::Hard as u8
        ],
        [0, 1, 3]
    );
    // .7
    assert_eq!((cfg_params::MISMATCH, cfg_params::LEVERAGE), (1, 2));
    assert_eq!((kill_params::CANCEL_RESTING, kill_params::FLATTEN), (1, 2));
    // .8
    assert_eq!(
        [
            RecoveryFlags::TAIL_LOST,
            RecoveryFlags::RECOVER_MISMATCH,
            RecoveryFlags::POISON,
            RecoveryFlags::FROM_SNAPSHOT,
            RecoveryFlags::CLEAN_DRAIN
        ],
        [1, 2, 4, 8, 16]
    );
    // .9
    let (a, v) = (AccountId(1), VenueId(2));
    assert_eq!(RiskTimer::MarginStale { a, v }.timer_id(), 131_329);
    assert_eq!(RiskTimer::MarginDeadline { a, v }.timer_id(), 131_330);
    assert_eq!(RiskTimer::MarginForce { a, v }.timer_id(), 131_331);
    for t in [
        RiskTimer::MarginStale { a, v },
        RiskTimer::MarginDeadline { a, v },
        RiskTimer::MarginForce { a, v },
    ] {
        assert_eq!(RiskTimer::from_timer_id(t.timer_id()), Some(t));
    }
    // .10
    let w = r0();
    for (kind, level) in [(Some(4u8), None), (Some(9), None), (None, Some(4u8))] {
        let bytes = bad_snapshot(kind, level);
        let mut rd = SnapshotReader::new(&bytes);
        let res = hft_risk::Risk::restore(&mut rd, &w.p, w.rd, &gws_for(w.rd), &CAPS, &exec_bounds(3), MC);
        assert!(res.is_err(), "RSK-009.10 kind {kind:?} level {level:?}");
    }
}

#[test]
fn jnl_005_mode_row_layout() {
    // JNL-005.1
    assert_eq!(size_of::<ModeRow>(), 8);
    // JNL-005.2
    assert_eq!(
        [
            offset_of!(ModeRow, strategy),
            offset_of!(ModeRow, account),
            offset_of!(ModeRow, mode),
            offset_of!(ModeRow, flags),
            offset_of!(ModeRow, group)
        ],
        [0, 1, 2, 3, 4]
    );
    // JNL-005.3: a DEFAULT row carries GROUP_NONE (0).
    assert_eq!((mode_row_flags::DEFAULT, mode_row_flags::LANDED), (1, 2));
    assert_eq!(hft_types::GROUP_NONE.0, 0);
    assert_eq!(MODE_TABLE_VERSION, 2);
}

#[test]
fn jnl_006_gate_row_layout() {
    // JNL-006.1
    assert_eq!(size_of::<GateRow>(), 64);
    // JNL-006.2
    assert_eq!(
        [
            offset_of!(GateRow, account),
            offset_of!(GateRow, released),
            offset_of!(GateRow, clean_restart),
            offset_of!(GateRow, recovery_flags),
            offset_of!(GateRow, gate_bits)
        ],
        [0, 1, 2, 3, 4]
    );
    // JNL-006.3
    assert_eq!(
        [
            offset_of!(GateRow, reconciled_venues),
            offset_of!(GateRow, cfg_ok_venues),
            offset_of!(GateRow, identity_ok_venues),
            offset_of!(GateRow, conns_ok_venues),
            offset_of!(GateRow, ctl_state_version),
            offset_of!(GateRow, standby_ok_venues),
            offset_of!(GateRow, gflags)
        ],
        [8, 16, 24, 32, 40, 48, 56]
    );
    assert_eq!(GATES_VERSION, 5);
}
