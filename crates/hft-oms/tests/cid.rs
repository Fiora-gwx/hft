#![allow(
    clippy::disallowed_types,
    clippy::disallowed_macros,
    clippy::disallowed_methods
)] // allow-list: test code
#![allow(clippy::type_complexity, clippy::too_many_arguments)]
use crate::common;

use common::*;
use hft_event::OrderEventKind;
use hft_oms::*;
use hft_strategy_sdk::{SnapshotReader, SnapshotWriter};
use hft_types::{AccountId, ClientOrderId, VenueId};

#[test]
fn oms_030_epoch_low15() {
    for (row, e, want) in [
        ("OMS-030.1", 7u32, 7u16),
        ("OMS-030.2", 0x7FFF, 0x7FFF),
        ("OMS-030.3", 0x8000, 0),
        ("OMS-030.4", 0x8005, 5),
    ] {
        let mut c = p0();
        c.engine_epoch = e;
        let mut h = H::from_cfg(&c, rd());
        let s = h.place_ok(&buy_x(), 0);
        assert_eq!(h.slot(s).epoch, want, "{row}");
        if e == 7 {
            let id = ClientOrderId {
                epoch: 7,
                venue: VenueId(2),
                account: AccountId(1),
                slot: 1,
                r#gen: 1,
            };
            let mut b = [0u8; 24];
            id.encode(&mut b);
            assert_eq!(ClientOrderId::decode(&b), Ok(id), "{row}");
        }
    }
}

#[test]
fn oms_031_match_by_epoch() {
    let mut c = p0();
    c.engine_epoch = 4;
    let mut h = H::from_cfg(&c, rd());
    let _ = h.place_ok(&buy_x(), 0);
    let r0 = h.place_ok(&buy_x(), 0);
    h.o.test_slot_mut(VS, A1, r0.slot, |sl, _| sl.r#gen = 3);
    let r = hft_strategy_sdk::OrderRef::new(VS, A1, 2, 3);
    h.what(r, OrderEventKind::OutcomeUnknown, 1);
    assert_eq!(h.state(r), OrderState::Unknown);
    let mut w = SnapshotWriter::new(1 << 20);
    h.o.snapshot(&mut w);
    let mut c9 = p0();
    c9.engine_epoch = 9;
    let o = Oms::restore(&mut SnapshotReader::new(w.as_bytes()), &c9, rd()).unwrap();
    let mut h = H {
        o,
        out: OmsOut::new(),
        seq: 1000,
    };
    h.conn(
        VS,
        A1,
        hft_event::ConnRole::Order,
        hft_event::ConnStateKind::Up,
        2,
    );
    let n = h.place_ok(&buy_x(), 2);
    assert_eq!(h.slot(n).epoch, 9);
    // .1 QueryResult with the old epoch converges r.
    h.query(r, 1, q("0"), p("0"), 3);
    assert_eq!(h.state(r), OrderState::Live, "OMS-031.1");
    // .2 Ack{epoch 9, slot 2, gen 3}: StaleGen, no Note.
    let stale0 = h.o.stats().stale_gen;
    let mut e = h.oe(r, OrderEventKind::Ack);
    e.epoch = 9;
    h.ev(&e, 4);
    assert_eq!(h.o.stats().stale_gen, stale0 + 1, "OMS-031.2");
    assert!(h.out.notes.is_empty(), "OMS-031.2");
    // .3 the new slot matches with epoch 9.
    h.ack(n, 1, 5);
    assert_eq!(h.state(n), OrderState::Live, "OMS-031.3");
    // .4 epoch 4 does not match the new slot.
    let n2 = h.place_ok(&buy_x(), 6);
    let mut e = h.oe(n2, OrderEventKind::Ack);
    e.epoch = 4;
    let s0 = h.o.stats().stale_gen;
    h.ev(&e, 7);
    assert_eq!(h.state(n2), OrderState::PendingNew, "OMS-031.4");
    assert_eq!(h.o.stats().stale_gen, s0 + 1, "OMS-031.4");
}

#[test]
fn oms_033_cid_fits_venue_limits() {
    let id = ClientOrderId {
        epoch: 0x7FFF,
        venue: VenueId(14),
        account: AccountId(255),
        slot: 65_535,
        r#gen: u32::MAX,
    };
    let mut b = [0u8; 24];
    id.encode(&mut b);
    let s = core::str::from_utf8(&b).unwrap();
    assert!(
        s.len() <= 36
            && s.bytes()
                .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c)),
        "OMS-033.1"
    );
    assert!(
        s.len() <= 32 && s.bytes().all(|c| c.is_ascii_alphanumeric()),
        "OMS-033.2"
    );
    assert!(s.len() <= 40, "OMS-033.3");
    assert!(s.len() <= 36, "OMS-033.4");
    let gate = format!("t-{s}");
    assert_eq!(gate.len(), 26, "OMS-033.5");
    assert!(gate.len() - 2 <= 28, "OMS-033.5");
    assert!(s.len() <= 40, "OMS-033.6");
    let hl = format!("0x00000001{s}");
    assert_eq!(hl.len(), 34, "OMS-033.7");
    assert!(
        hl[2..].bytes().all(|c| c.is_ascii_hexdigit()) && hl[2..].len() == 32,
        "OMS-033.7"
    );
}
