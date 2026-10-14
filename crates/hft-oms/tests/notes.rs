#![allow(
    clippy::disallowed_types,
    clippy::disallowed_macros,
    clippy::disallowed_methods
)] // allow-list: test code
#![allow(clippy::type_complexity, clippy::too_many_arguments)]
use crate::common;

use common::*;
use hft_event::{NOTE_NAMES, NoteLevel, NoteSource, OrderEventKind, ReconcileReport};
use hft_oms::notes;
use hft_oms::*;
use hft_types::{Notional, Qty, SlotId, Tif};

#[test]
fn oms_023_transition_note_packing() {
    let mut h = H::new();
    let s = h.place_ok(&buy_x(), 0);
    assert_eq!((s.slot, s.r#gen), (1, 1));
    h.seq = 899;
    h.ack(s, 777, 1);
    let n = h.notes(notes::TRANSITION);
    assert_eq!(n.len(), 1);
    let n = n[0];
    assert_eq!(n.level, NoteLevel::Info);
    assert_eq!(n.inst, X);
    assert_eq!(n.group, g1());
    assert_eq!(n.v[0], 17_213_489_410);
    assert_eq!(&n.v[1..8], &[1, 1, 900, 0, 1_000_000, 6_000_000, 777]);
}

#[test]
fn oms_025_note_codes_registered() {
    let mine: Vec<&str> = notes::ALL.iter().map(|c| c.name).collect();
    let scanned: Vec<(u32, &str)> = notes::NOTE_CODES
        .iter()
        .map(|(s, n, nm)| {
            assert_eq!(*s, NoteSource::Oms);
            (*n, *nm)
        })
        .collect();
    assert_eq!(
        scanned,
        notes::ALL.iter().map(|c| (c.n, c.name)).collect::<Vec<_>>(),
        "OMS-025.2"
    );
    assert_eq!(mine.len(), 23, "OMS-025.1");
    let mut reg: Vec<&str> = NOTE_NAMES
        .iter()
        .filter(|n| n.source == NoteSource::Oms)
        .map(|n| n.name)
        .collect();
    let mut m = mine.clone();
    m.sort_unstable();
    reg.sort_unstable();
    assert_eq!(m, reg, "OMS-025.1");
    let mut ns: Vec<u32> = notes::ALL.iter().map(|c| c.n).collect();
    ns.sort_unstable();
    ns.dedup();
    assert_eq!(ns.len(), 23, "OMS-025.2");
    for c in notes::ALL {
        assert_eq!(notes::code_of(*c), 4 << 24 | c.n, "OMS-025.2");
    }
    let lv = |names: &[&str], l: NoteLevel| {
        for nm in names {
            let c = notes::ALL.iter().find(|c| c.name == *nm).unwrap();
            assert_eq!(c.level, l, "OMS-025.3 {nm}");
            let r = NOTE_NAMES.iter().find(|n| n.name == *nm).unwrap();
            assert!(r.level.is_none() || r.level == Some(l), "OMS-025.3 registry {nm}");
        }
    };
    lv(
        &[
            "OmsUnknownStuck",
            "OmsStaleGenFill",
            "OmsLateFill",
            "OmsOverfill",
            "OmsHedgeBroken",
            "OmsResumeTimeout",
        ],
        NoteLevel::P0,
    );
    lv(
        &[
            "OmsUnknown",
            "OmsForeignOrder",
            "OmsForeignFillViaOrderEvent",
            "OmsExtDedup",
            "OmsFillPxEstimated",
            "OmsFeeUnattributed",
            "OmsHedgePxStale",
            "OmsHedgePxGated",
            "OmsHedgeSlipCapped",
            "OmsHedgeNakedCapped",
            "OmsResumeRetry",
            "OmsAnomaly",
        ],
        NoteLevel::P1,
    );
    lv(&["OmsCancelNotSent"], NoteLevel::P2);
    lv(
        &[
            "OmsTransition",
            "OmsVenueReady",
            "OmsNeedsRestQuery",
            "OmsForeignFill",
        ],
        NoteLevel::Info,
    );
}

#[test]
fn oms_026_anomaly_kinds() {
    // .1 Reject contradicts a known fill (OMS-056).
    let mut h = H::new();
    let s = h.place_ok(&buy_x(), 0);
    h.ack(s, 1, 1);
    let mut e = h.oe(s, OrderEventKind::Reject);
    e.reject_code = 2010;
    e.cum_filled = q("0.001");
    e.avg_px = p("60000");
    h.ev(&e, 2);
    let a = h.notes(notes::ANOMALY);
    assert_eq!(
        (a.len(), a[0].level, a[0].v[0]),
        (1, NoteLevel::P1, 1),
        "OMS-026.1"
    );
    // .2 foreign table eviction (OMS-110.2).
    let mut h = H::with(|c| c.foreign_fill_track.set(VS, A1, 2));
    let mut evicts = 0;
    for eid in [501u64, 502, 503] {
        let r0 = h.place_ok(&buy_x(), 0);
        let mut e = h.oe(r0, OrderEventKind::PartialFill);
        e.slot = SlotId(0);
        e.exch_order_id = eid;
        e.cum_filled = q("0.001");
        e.detail = 1;
        h.ev(&e, 1);
        evicts += h
            .notes(notes::ANOMALY)
            .iter()
            .filter(|n| n.v[0] == 2 && n.level == NoteLevel::P1)
            .count();
    }
    assert_eq!(evicts, 1, "OMS-026.2");
    // .3 smoke order filled (OMS-267).
    let mut h = H::new();
    let mut sm = buy_x();
    sm.intent.tif = Tif::PostOnly;
    sm.intent.group = hft_types::GROUP_NONE;
    sm.intent.qty = q("0.0001");
    sm.origin = Origin::Command;
    sm.smoke_ttl_ms = 60_000;
    let k = h.place_ok(&sm, 0);
    h.ack(k, 1, 1);
    h.fill(k, q("0.00005"), p("58200.00"), 2);
    let a = h.notes(notes::ANOMALY);
    assert!(
        a.iter().any(|n| n.v[0] == 3 && n.level == NoteLevel::P0),
        "OMS-026.3"
    );
    // .4 reconciliation entry ignored (OMS-145).
    let mut h = H::new();
    let s = h.place_ok(&buy_x(), 0);
    let e = ReconcileReport {
        report_id: 1,
        venue: VS,
        account: A1,
        item: 4,
        severity: 0,
        inst: X,
        asset: BTC,
        idx: 0,
        count: 1,
        _pad: 0,
        engine_qty: Qty(0),
        exch_qty: Qty(0),
        diff: Notional(0),
        exch_ts_ns: 0,
        slot: SlotId(s.slot),
        order_status: 3,
        history_checked: 1,
        _pad2: [0; 2],
        view_seq: 0,
        r#gen: 99,
        epoch: 7,
        _pad3: 0,
        exch_amount: Notional(0),
        engine_amount: Notional(0),
        sflags: 0,
        skipped_inflight: 0,
        _pad4: 0,
    };
    h.recon(&e, 1);
    let a = h.notes(notes::ANOMALY);
    assert!(
        a.iter().any(|n| n.v[0] == 4 && n.level == NoteLevel::P1),
        "OMS-026.4"
    );
}
