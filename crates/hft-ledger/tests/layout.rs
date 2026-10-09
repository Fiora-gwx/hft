//! LED-019, LED-020, LED-178, LED-233, LED-234: in-process and snapshot row layouts, codes.

use crate::common;

use std::mem::{align_of, offset_of, size_of};

use common::*;
use hft_ledger::snapshot::{
    AccountRow, AutoHeldRow, CashRow, ExtSeenRow, FundHistRow, FundMarkRow, FundNoHistRow, FundPendingRow,
    FundSeenRow, GroupRow, InvRow, LedgerRowHdr, MarginItemRowSnap, MarginRow, PositionRow, PxRow, RateRow,
    SECTION_LEDGER_GROUPS, row_kind,
};
use hft_ledger::{AdjustDelta, AdjustKind, FillClass, FillDelta, LegAgg, PnlBreakdown, PxSrc, fill_flags};
use hft_strategy_sdk::{RestoreError, SnapshotWriter};
use zerocopy::IntoBytes;

#[test]
fn led_019_filldelta_layout() {
    assert_eq!(size_of::<FillDelta>(), 72);
    assert_eq!(align_of::<FillDelta>(), 8);
    let offs = [
        (offset_of!(FillDelta, account), 0),
        (offset_of!(FillDelta, venue), 1),
        (offset_of!(FillDelta, side), 2),
        (offset_of!(FillDelta, class), 3),
        (offset_of!(FillDelta, flags), 4),
        (offset_of!(FillDelta, leg_ix), 5),
        (offset_of!(FillDelta, inst), 8),
        (offset_of!(FillDelta, group), 12),
        (offset_of!(FillDelta, qty), 16),
        (offset_of!(FillDelta, net_qty), 24),
        (offset_of!(FillDelta, px), 32),
        (offset_of!(FillDelta, fee), 40),
        (offset_of!(FillDelta, fee_asset), 48),
        (offset_of!(FillDelta, slot), 52),
        (offset_of!(FillDelta, r#gen), 56),
        (offset_of!(FillDelta, seq), 64),
    ];
    for (k, (got, want)) in offs.iter().enumerate() {
        assert_eq!(got, want, "field #{k}");
    }
    // account venue side class flags leg_ix _pad inst group qty net_qty px fee fee_asset _pad2
    // slot gen _pad3 seq
    let sum = 1 + 1 + 1 + 1 + 1 + 1 + 2 + 4 + 4 + 8 + 8 + 8 + 8 + 2 + 2 + 4 + 4 + 4 + 8;
    assert_eq!(sum, 72);
}

#[test]
fn led_020_filldelta_codes() {
    for (c, v) in FillClass::ALL.iter().zip(1u8..=7) {
        assert_eq!(*c as u8, v, "LED-020.1");
        assert_eq!(FillClass::try_from(v), Ok(*c), "LED-020.1");
    }
    assert!(FillClass::try_from(0).is_err(), "LED-020.2");
    assert!(FillClass::try_from(8).is_err(), "LED-020.2");
    assert_eq!(
        [
            fill_flags::PX_EST,
            fill_flags::LATE,
            fill_flags::OVERFILL,
            fill_flags::EXT_DEDUP
        ],
        [1, 2, 4, 8],
        "LED-020.3"
    );
}

#[test]
fn led_178_adjust_delta_layout() {
    assert_eq!(
        (size_of::<AdjustDelta>(), align_of::<AdjustDelta>()),
        (56, 8),
        "LED-178.1"
    );
    let offs = [
        offset_of!(AdjustDelta, account),
        offset_of!(AdjustDelta, venue),
        offset_of!(AdjustDelta, kind),
        offset_of!(AdjustDelta, px_src),
        offset_of!(AdjustDelta, inst),
        offset_of!(AdjustDelta, asset),
        offset_of!(AdjustDelta, group),
        offset_of!(AdjustDelta, qty),
        offset_of!(AdjustDelta, px),
        offset_of!(AdjustDelta, report_id),
        offset_of!(AdjustDelta, cmd_seq),
        offset_of!(AdjustDelta, settle_ts_ns),
    ];
    assert_eq!(offs, [0, 1, 2, 3, 4, 8, 12, 16, 24, 32, 40, 48], "LED-178.1");
    let kinds = [
        AdjustKind::Position,
        AdjustKind::Balance,
        AdjustKind::Funding,
        AdjustKind::WriteOff,
    ];
    for (k, v) in kinds.iter().zip(1u8..=4) {
        assert_eq!(*k as u8, v, "LED-178.2");
        assert_eq!(AdjustKind::try_from(v), Ok(*k), "LED-178.2");
    }
    assert!(
        AdjustKind::try_from(0).is_err() && AdjustKind::try_from(5).is_err(),
        "LED-178.2"
    );
    for (s, v) in [PxSrc::Cmd, PxSrc::Mark, PxSrc::Bid].iter().zip(1u8..=3) {
        assert_eq!(*s as u8, v, "LED-178.3");
        assert_eq!(PxSrc::try_from(v), Ok(*s), "LED-178.3");
    }
    assert!(
        PxSrc::try_from(0).is_err(),
        "LED-178.3: 0 is the Funding/WriteOff value, not a source"
    );
}

/// Size of every field (explicit padding included) summed via a zeroed row's byte image.
fn no_implicit_padding<T: IntoBytes + zerocopy::Immutable>(row: &T) -> usize {
    // `IntoBytes` derives only when the struct has no implicit padding, so the byte image length
    // equals the sum of field sizes.
    row.as_bytes().len()
}

#[test]
fn led_233_row_sizes() {
    use zerocopy::FromZeros;
    let sizes = [
        (
            "PositionRow",
            size_of::<PositionRow>(),
            64,
            no_implicit_padding(&PositionRow::new_zeroed()),
        ),
        (
            "LedgerRowHdr",
            size_of::<LedgerRowHdr>(),
            8,
            no_implicit_padding(&LedgerRowHdr::new_zeroed()),
        ),
        (
            "Account",
            size_of::<AccountRow>(),
            88,
            no_implicit_padding(&AccountRow::new_zeroed()),
        ),
        (
            "Rate",
            size_of::<RateRow>(),
            32,
            no_implicit_padding(&RateRow::new_zeroed()),
        ),
        (
            "Group",
            size_of::<GroupRow>(),
            552,
            no_implicit_padding(&GroupRow::new_zeroed()),
        ),
        (
            "Cash",
            size_of::<CashRow>(),
            64,
            no_implicit_padding(&CashRow::new_zeroed()),
        ),
        (
            "Inv",
            size_of::<InvRow>(),
            16,
            no_implicit_padding(&InvRow::new_zeroed()),
        ),
        (
            "Margin",
            size_of::<MarginRow>(),
            64,
            no_implicit_padding(&MarginRow::new_zeroed()),
        ),
        (
            "MarginItem",
            size_of::<MarginItemRowSnap>(),
            16,
            no_implicit_padding(&MarginItemRowSnap::new_zeroed()),
        ),
        (
            "FundPending",
            size_of::<FundPendingRow>(),
            32,
            no_implicit_padding(&FundPendingRow::new_zeroed()),
        ),
        (
            "FundMark",
            size_of::<FundMarkRow>(),
            16,
            no_implicit_padding(&FundMarkRow::new_zeroed()),
        ),
        (
            "FundHist",
            size_of::<FundHistRow>(),
            32,
            no_implicit_padding(&FundHistRow::new_zeroed()),
        ),
        (
            "FundNoHist",
            size_of::<FundNoHistRow>(),
            32,
            no_implicit_padding(&FundNoHistRow::new_zeroed()),
        ),
        (
            "FundSeen",
            size_of::<FundSeenRow>(),
            16,
            no_implicit_padding(&FundSeenRow::new_zeroed()),
        ),
        (
            "ExtSeen",
            size_of::<ExtSeenRow>(),
            24,
            no_implicit_padding(&ExtSeenRow::new_zeroed()),
        ),
        (
            "AutoHeld",
            size_of::<AutoHeldRow>(),
            16,
            no_implicit_padding(&AutoHeldRow::new_zeroed()),
        ),
        (
            "Px",
            size_of::<PxRow>(),
            48,
            no_implicit_padding(&PxRow::new_zeroed()),
        ),
        (
            "LegAgg",
            size_of::<LegAgg>(),
            48,
            no_implicit_padding(&LegAgg::new_zeroed()),
        ),
        (
            "PnlBreakdown",
            size_of::<PnlBreakdown>(),
            64,
            no_implicit_padding(&PnlBreakdown::zero()),
        ),
    ];
    for (name, got, want, image) in sizes {
        assert_eq!(got, want, "LED-233 {name}");
        assert_eq!(image, got, "LED-233.9 {name}");
    }
}

#[test]
fn led_234_row_offsets() {
    assert_eq!(
        [
            offset_of!(PositionRow, account),
            offset_of!(PositionRow, inst),
            offset_of!(PositionRow, qty),
            offset_of!(PositionRow, cost),
            offset_of!(PositionRow, realized_quote),
            offset_of!(PositionRow, fees_quote),
            offset_of!(PositionRow, last_fill_ens),
        ],
        [0, 4, 8, 16, 32, 48, 56],
        "LED-234.1"
    );
    assert_eq!(
        [
            offset_of!(LedgerRowHdr, kind),
            offset_of!(LedgerRowHdr, account),
            offset_of!(LedgerRowHdr, venue),
            offset_of!(LedgerRowHdr, id),
        ],
        [0, 1, 2, 4],
        "LED-234.2"
    );
    assert_eq!(
        [
            offset_of!(FundHistRow, group),
            offset_of!(FundHistRow, hflags),
            offset_of!(FundHistRow, settle_ts_ns),
            offset_of!(FundHistRow, booked),
            offset_of!(FundHistRow, corrected),
        ],
        [0, 4, 8, 16, 24],
        "LED-234.3"
    );
    assert_eq!(
        [
            offset_of!(ExtSeenRow, kind),
            offset_of!(ExtSeenRow, a),
            offset_of!(ExtSeenRow, b)
        ],
        [0, 8, 16],
        "LED-234.4"
    );
    assert_eq!(
        [
            row_kind::ACCOUNT,
            row_kind::RATE,
            row_kind::GROUP,
            row_kind::PENDING,
            row_kind::CASH,
            row_kind::INV,
            row_kind::MARGIN,
            row_kind::FUND_PENDING,
            row_kind::FUND_MARK,
            row_kind::FUND_HIST,
            row_kind::FUND_NO_HIST,
            row_kind::FUND_SEEN,
            row_kind::EXT_SEEN,
            row_kind::AUTO_HELD,
            row_kind::PX,
        ],
        [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15],
        "LED-234.5"
    );
    // Kinds 0 and 16 in section 13 are rejected by `restore`.
    for bad in [0u8, 16] {
        let mut w = SnapshotWriter::new(1 << 16);
        w.begin_section(SECTION_LEDGER_GROUPS, hft_ledger::snapshot::LEDGER_GROUPS_VERSION);
        w.put_row(&LedgerRowHdr {
            kind: bad,
            account: A1,
            venue: hft_types::VenueId(0),
            _pad: 0,
            id: 0,
        });
        w.put_bytes(&[0u8; 88]);
        w.end_section();
        assert_eq!(
            restore(w.as_bytes()).err(),
            Some(RestoreError::BadValue),
            "LED-234.5 kind {bad}"
        );
    }
}
