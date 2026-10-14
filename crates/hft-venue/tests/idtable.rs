use crate::support::{
    gwh::*,
    rd::*,
    stub::{self, StubProto},
};
use hft_event::OrderEventKind;
use hft_net::ConnId;
use hft_types::{ClientOrderId, SlotId};
use hft_venue::{IdErr, IdTable, exch_id_value};

const T: u64 = 3_000_000;
const KEEP: u64 = 120_000_000_000;
fn report(
    h: &mut Gh<StubProto>,
    key: &str,
    generation: Option<u32>,
    what: &str,
    now: u64,
) -> hft_event::OrderEvent {
    let cid = generation
        .map(|g| {
            stub::cid_text(&ClientOrderId {
                epoch: 5,
                venue: V7,
                account: A1,
                slot: 100,
                r#gen: g,
            })
        })
        .unwrap_or_default();
    h.net.push(
        ConnId(112),
        format!("O|{cid}|{key}|{what}|1|1|100|0|0|0").as_bytes(),
    );
    h.poll(now);
    let e = events(&h.reps());
    assert_eq!(e.len(), 1);
    e[0]
}
#[test]
fn ven_290_insert_lookup() {
    let mut h = gw::<StubProto>();
    report(&mut h, "EX-100", Some(3), "Ack", T);
    assert_eq!(h.rt.id_table(A1).unwrap().len(), 1);
    let e = report(&mut h, "EX-100", None, "PartialFill", T + 1);
    assert_eq!(
        (e.what, e.epoch, e.slot, e.r#gen),
        (OrderEventKind::PartialFill, 5, SlotId(100), 3)
    );
    assert_eq!(h.rt.held(A1), 0);
}
#[test]
fn ven_291_tombstone_resolves() {
    let mut h = gw::<StubProto>();
    report(&mut h, "EX-100", Some(3), "Fill", T);
    for (what, dt) in [("FeeOnly", 60_000_000_000), ("PartialFill", 90_000_000_000)] {
        let e = report(&mut h, "EX-100", None, what, T + dt);
        assert_eq!((e.epoch, e.slot, e.r#gen), (5, SlotId(100), 3));
    }
}
#[test]
fn ven_292_new_gen_keeps_old() {
    let mut h = gw::<StubProto>();
    report(&mut h, "EX-100", Some(3), "Fill", T);
    report(&mut h, "EX-200", Some(4), "Ack", T + 1);
    for (key, what, g) in [("EX-100", "FeeOnly", 3), ("EX-200", "Fill", 4)] {
        let e = report(&mut h, key, None, what, T + 2);
        assert_eq!((e.epoch, e.slot, e.r#gen), (5, SlotId(100), g));
    }
}
#[test]
fn ven_293_eviction() {
    for dt in [KEEP - 1, KEEP] {
        let mut table = IdTable::new(16384, 4, KEEP);
        for n in 0..5 {
            table.insert(format!("EX-{n}").as_bytes(), 5, n, 3, T).unwrap();
        }
        for n in 0..4 {
            table.terminal(format!("EX-{n}").as_bytes(), T);
        }
        table.terminal(b"EX-4", T + dt);
        assert_eq!(table.get(b"EX-0").is_some(), dt < KEEP);
        assert_eq!(table.get(b"EX-4").is_some(), dt == KEEP);
        assert_eq!(table.n.tomb_full, u64::from(dt < KEEP));
        for n in 1..4 {
            assert_eq!(table.get(format!("EX-{n}").as_bytes()), Some((5, n, 3)));
        }
    }
    // All keys share a probe cluster. Removing its head moves the new terminal entry.
    let keys: Vec<_> = (0..1000)
        .map(|n| format!("collision-{n}"))
        .filter(|k| xxhash_rust::xxh3::xxh3_64(k.as_bytes()) & 15 == 0)
        .take(3)
        .collect();
    let mut table = IdTable::new(16, 1, KEEP);
    for (n, k) in keys.iter().enumerate() {
        table.insert(k.as_bytes(), 5, n as u32, 3, T).unwrap();
    }
    table.terminal(keys[0].as_bytes(), T);
    table.terminal(keys[1].as_bytes(), T + KEEP);
    table.terminal(keys[2].as_bytes(), T + 2 * KEEP);
    assert_eq!(table.get(keys[0].as_bytes()), None);
    assert_eq!(table.get(keys[1].as_bytes()), None);
    assert_eq!(table.get(keys[2].as_bytes()), Some((5, 2, 3)));
}
#[test]
fn ven_294_full_insert_fails() {
    // A tomb ring of four cannot retain eight terminals: use four retained plus four live.
    let mut table = IdTable::new(16, 4, KEEP);
    for n in 0..8 {
        table.insert(format!("EX-{n}").as_bytes(), 5, n, 3, T).unwrap();
    }
    for n in 0..4 {
        table.terminal(format!("EX-{n}").as_bytes(), T);
    }
    assert_eq!(table.insert(b"new", 5, 9, 3, T + 1_000_000_000), Err(IdErr::Full));
    assert_eq!(table.n.full, 1);
    assert_eq!(table.len(), 8);
    assert_eq!(table.insert(b"new", 5, 9, 3, T + KEEP), Ok(()));
    for n in 1..8 {
        assert_eq!(table.get(format!("EX-{n}").as_bytes()), Some((5, n, 3)));
    }
}
#[test]
fn ven_295_id_forms() {
    let mut h = gw::<StubProto>();
    for key in [
        "8389765412",
        "5f1e8a3c-1111-2222-3333-123456789b2d",
        "1234567890123456789012345678901234567",
    ] {
        let e = report(&mut h, key, Some(3), "Ack", T);
        assert_eq!(
            e.exch_order_id,
            if key == "8389765412" {
                8_389_765_412
            } else {
                xxhash_rust::xxh3::xxh3_64(key.as_bytes())
            }
        );
        assert_eq!(
            h.rt.id_table(A1).unwrap().get(key.as_bytes()).is_some(),
            key.len() <= 36
        );
    }
    assert_eq!(h.rt.id_table(A1).unwrap().n.too_long, 1);
    assert_eq!(
        exch_id_value(b"18446744073709551616"),
        xxhash_rust::xxh3::xxh3_64(b"18446744073709551616")
    );
}
