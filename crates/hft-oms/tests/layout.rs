#![allow(
    clippy::disallowed_types,
    clippy::disallowed_macros,
    clippy::disallowed_methods
)] // allow-list: test code
#![allow(clippy::type_complexity, clippy::too_many_arguments)]
use crate::common;

use core::mem::{align_of, offset_of, size_of};

use common::*;
use hft_event::{OrderAction, OrderEventKind};
use hft_oms::*;
use hft_strategy_sdk::{LegView, SnapshotReader, SnapshotWriter};
use hft_types::Tif;

#[test]
fn oms_010_order_slot_layout() {
    assert_eq!(size_of::<OrderSlot>(), 128);
    assert_eq!(align_of::<OrderSlot>(), 64);
    let offs = [
        (offset_of!(OrderSlot, state), 0),
        (offset_of!(OrderSlot, prev_state), 3),
        (offset_of!(OrderSlot, oflags), 4),
        (offset_of!(OrderSlot, r#gen), 8),
        (offset_of!(OrderSlot, slot_id), 12),
        (offset_of!(OrderSlot, inst), 16),
        (offset_of!(OrderSlot, query_count), 21),
        (offset_of!(OrderSlot, epoch), 22),
        (offset_of!(OrderSlot, group), 24),
        (offset_of!(OrderSlot, link_id), 28),
        (offset_of!(OrderSlot, qty), 32),
        (offset_of!(OrderSlot, filled), 40),
        (offset_of!(OrderSlot, px), 48),
        (offset_of!(OrderSlot, exch_order_id), 56),
        (offset_of!(OrderSlot, filled_notional), 64),
        (offset_of!(OrderSlot, pending_px), 80),
        (offset_of!(OrderSlot, fee_accum), 96),
        (offset_of!(OrderSlot, fee_asset), 104),
        (offset_of!(OrderSlot, sflags), 106),
        (offset_of!(OrderSlot, reject_code), 108),
        (offset_of!(OrderSlot, t_state), 112),
        (offset_of!(OrderSlot, seq_last), 120),
    ];
    for (i, (got, want)) in offs.iter().enumerate() {
        assert_eq!(got, want, "field {i}");
    }
}

#[test]
fn oms_011_slot_ext_layout() {
    assert_eq!((size_of::<SlotExt>(), align_of::<SlotExt>()), (64, 32));
    assert_eq!(
        [
            offset_of!(SlotExt, net_filled),
            offset_of!(SlotExt, tag),
            offset_of!(SlotExt, leg_ix),
            offset_of!(SlotExt, role),
            offset_of!(SlotExt, origin),
            offset_of!(SlotExt, class),
            offset_of!(SlotExt, lev_x100),
            offset_of!(SlotExt, max_rest_ms),
            offset_of!(SlotExt, fee_cum)
        ],
        [0, 8, 12, 13, 14, 15, 16, 20, 32]
    );
}

#[test]
fn oms_012_slot_stamps_layout() {
    assert_eq!(size_of::<SlotStamps>(), 80);
    assert_eq!(
        [
            offset_of!(SlotStamps, trig_ex_ts_ns),
            offset_of!(SlotStamps, trig_sw_rx_ns),
            offset_of!(SlotStamps, trig_rx_tsc),
            offset_of!(SlotStamps, decided_tsc),
            offset_of!(SlotStamps, sent_tsc),
            offset_of!(SlotStamps, trigger_seq),
            offset_of!(SlotStamps, trig_kind),
            offset_of!(SlotStamps, op)
        ],
        [0, 24, 32, 48, 56, 64, 72, 74]
    );
}

#[test]
fn oms_013_tomb_layout() {
    assert_eq!(size_of::<Tomb>(), 56);
    assert_eq!(
        [
            offset_of!(Tomb, r#gen),
            offset_of!(Tomb, epoch),
            offset_of!(Tomb, oflags),
            offset_of!(Tomb, inst),
            offset_of!(Tomb, group),
            offset_of!(Tomb, side),
            offset_of!(Tomb, leg_ix),
            offset_of!(Tomb, strategy),
            offset_of!(Tomb, role),
            offset_of!(Tomb, link_id),
            offset_of!(Tomb, filled),
            offset_of!(Tomb, net_filled),
            offset_of!(Tomb, exch_order_id),
            offset_of!(Tomb, freed_tsc)
        ],
        [0, 4, 6, 8, 12, 16, 17, 18, 19, 20, 24, 32, 40, 48]
    );
}

#[test]
fn oms_014_link_entry_layout() {
    assert_eq!((size_of::<LinkEntry>(), align_of::<LinkEntry>()), (80, 8));
    assert_eq!(
        [
            offset_of!(LinkEntry, link_id),
            offset_of!(LinkEntry, group),
            offset_of!(LinkEntry, account),
            offset_of!(LinkEntry, state),
            offset_of!(LinkEntry, retries),
            offset_of!(LinkEntry, flags),
            offset_of!(LinkEntry, plan),
            offset_of!(LinkEntry, first),
            offset_of!(LinkEntry, held_ms),
            offset_of!(LinkEntry, first_net),
            offset_of!(LinkEntry, hedge_req),
            offset_of!(LinkEntry, hedge_filled),
            offset_of!(LinkEntry, hedge_open),
            offset_of!(LinkEntry, first_fill_tsc),
            offset_of!(LinkEntry, ref_px)
        ],
        [0, 4, 8, 9, 10, 11, 12, 16, 28, 32, 40, 48, 56, 64, 72]
    );
}

#[test]
fn oms_015_out_item_layout() {
    assert_eq!(size_of::<OutItem>(), 32);
    assert_eq!(
        [
            offset_of!(OutItem, kind),
            offset_of!(OutItem, action),
            offset_of!(OutItem, from),
            offset_of!(OutItem, cause),
            offset_of!(OutItem, link_id),
            offset_of!(OutItem, cause_seq),
            offset_of!(OutItem, slot),
            offset_of!(OutItem, _pad)
        ],
        [0, 1, 2, 3, 4, 8, 16, 28]
    );
}

#[test]
fn oms_016_leg_state_layout() {
    assert_eq!(size_of::<LegState>(), 192);
    assert_eq!(size_of::<LegView>(), 160);
    assert_eq!(
        [
            offset_of!(LegState, view),
            offset_of!(LegState, tolerance),
            offset_of!(LegState, last_fill_tsc),
            offset_of!(LegState, last_link),
            offset_of!(LegState, broken_links),
            offset_of!(LegState, pending_slots),
            offset_of!(LegState, unknown_slots)
        ],
        [0, 160, 168, 176, 180, 182, 184]
    );
}

#[test]
fn oms_017_snapshot_row_sizes() {
    assert_eq!(
        (size_of::<TableRow>(), offset_of!(TableRow, smoke_slot)),
        (32, 28),
        "OMS-017.1"
    );
    assert_eq!(size_of::<LinkRowHdr>(), 8, "OMS-017.2");
    assert_eq!(
        (
            size_of::<OmsCountersHdr>(),
            offset_of!(OmsCountersHdr, resume_hold_until)
        ),
        (16, 8),
        "OMS-017.3"
    );
    // One non-free slot, one link, one group, one tombstone, one OutItem.
    let mut h = H::with(|c| c.slots.set(VG, A1, 1));
    let (r, _l) = place_first(&mut h, q("0.012"), Tif::Ioc, plan_l1(), 0);
    let z = h.place_ok(
        &intent(
            VG,
            A1,
            Z,
            hft_types::Side::Buy,
            Tif::Gtc,
            p("60000.0"),
            q("0.001"),
            g2(),
        ),
        0,
    );
    h.what(z, OrderEventKind::CancelAck, 0);
    h.sweep(6000);
    h.o.test_enqueue_decision(r, OrderAction::Query, 1);
    let mut w = SnapshotWriter::new(1 << 20);
    h.o.snapshot(&mut w);
    let rd = SnapshotReader::new(w.as_bytes());
    let s2 = rd.find_section(SECTION_ORDER_SLOTS).unwrap().unwrap();
    assert_eq!(SLOT_ROW_LEN, 192, "OMS-017.4");
    assert_eq!(s2.body.len(), 192 * s2.count as usize, "OMS-017.4");
    assert_eq!(s2.version, 2, "OMS-017.4");
    let s12 = rd.find_section(SECTION_OMS_LINKS).unwrap().unwrap();
    let mut seen = [0usize; 6];
    let mut b = s12.body;
    while b.len() >= 8 {
        let kind = b[0];
        let len = u32::from_le_bytes([b[4], b[5], b[6], b[7]]) as usize;
        if kind == 0 {
            break;
        }
        seen[kind as usize] = len;
        b = &b[8 + len..];
    }
    assert_eq!(seen[1], 80, "OMS-017.5");
    assert_eq!(seen[2], 64, "OMS-017.6");
    assert_eq!(seen[3], 200, "OMS-017.7");
    assert_eq!(seen[4], 64, "OMS-017.8");
    assert_eq!(seen[5], 32, "OMS-017.9");
}
