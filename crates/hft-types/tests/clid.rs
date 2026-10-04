//! `ClientOrderId` encode/decode (TYP-135 – TYP-137, TYP-140 – TYP-142).

#![allow(
    clippy::disallowed_types,
    clippy::disallowed_macros,
    clippy::disallowed_methods
)] // allow-list: test code, not a hot path

use hft_types::{AccountId, CidErr, ClientOrderId, VenueId};

fn cid(epoch: u16, venue: u8, account: u8, slot: u32, r#gen: u32) -> ClientOrderId {
    ClientOrderId {
        epoch,
        venue: VenueId(venue),
        account: AccountId(account),
        slot,
        r#gen,
    }
}

fn enc(x: &ClientOrderId) -> String {
    let mut out = [0u8; 24];
    x.encode(&mut out);
    String::from_utf8(out.to_vec()).unwrap()
}

#[test]
fn typ_135_cid_encode_golden() {
    let rows = [
        ("TYP-135.1", cid(1, 1, 1, 5, 7), "100010101000050000000702"),
        ("TYP-135.2", cid(0, 0, 0, 0, 0), "100000000000000000000010"),
        (
            "TYP-135.3",
            cid(0x7FFF, 255, 255, 0xF_FFFF, 0xFFFF_FFFF),
            "17ffffffffffffffffffff17",
        ),
        (
            "TYP-135.4",
            cid(0x1234, 10, 3, 0xA_BCDE, 0xDEAD_BEEF),
            "112340a03abcdedeadbeefa8",
        ),
        ("TYP-135.5", cid(5, 99, 200, 1, 2), "1000563c80000100000002f9"),
        ("TYP-135.6", cid(0x8001, 1, 1, 5, 7), "100010101000050000000702"),
    ];
    for (row, x, want) in rows {
        assert_eq!(enc(&x), want, "{row}");
    }
}

#[test]
fn typ_136_cid_decode_golden() {
    let rows = [
        ("TYP-136.1", "100010101000050000000702", cid(1, 1, 1, 5, 7)),
        ("TYP-136.2", "100000000000000000000010", cid(0, 0, 0, 0, 0)),
        (
            "TYP-136.3",
            "17ffffffffffffffffffff17",
            cid(0x7FFF, 255, 255, 0xF_FFFF, 0xFFFF_FFFF),
        ),
        (
            "TYP-136.4",
            "112340a03abcdedeadbeefa8",
            cid(0x1234, 10, 3, 0xA_BCDE, 0xDEAD_BEEF),
        ),
        ("TYP-136.5", "1000563c80000100000002f9", cid(5, 99, 200, 1, 2)),
    ];
    for (row, text, want) in rows {
        assert_eq!(ClientOrderId::decode(text.as_bytes()), Ok(want), "{row}");
    }
}

#[test]
fn typ_137_cid_decode_errors() {
    let mut c3 = b"100010101000050000000702".to_vec();
    c3[0] = 0xC3;
    let rows: Vec<(&str, Vec<u8>, CidErr)> = vec![
        ("TYP-137.1", b"".to_vec(), CidErr::Length),
        ("TYP-137.2", b"10001010100005000000070".to_vec(), CidErr::Length),
        ("TYP-137.3", b"1000101010000500000007020".to_vec(), CidErr::Length),
        ("TYP-137.4", b"1000101010000500000007A2".to_vec(), CidErr::Charset),
        ("TYP-137.5", b"10001010100005000000070g".to_vec(), CidErr::Charset),
        ("TYP-137.6", c3, CidErr::Charset),
        ("TYP-137.7", b"200010101000050000000732".to_vec(), CidErr::Version),
        ("TYP-137.8", b"000010101000050000000712".to_vec(), CidErr::Version),
        ("TYP-137.9", b"100010101000050000000703".to_vec(), CidErr::Check),
        ("TYP-137.10", b"100010101000050000000602".to_vec(), CidErr::Check),
        (
            "TYP-137.11",
            b"18001010100005000000070a".to_vec(),
            CidErr::Reserved,
        ),
        (
            "TYP-137.12",
            b"200010101000050000000702".to_vec(),
            CidErr::Version,
        ),
        ("TYP-137.13", b"180010101000050000000702".to_vec(), CidErr::Check),
        (
            "TYP-137.14",
            b"10001010100005000000070G".to_vec(),
            CidErr::Charset,
        ),
    ];
    for (row, text, want) in rows {
        assert_eq!(ClientOrderId::decode(&text), Err(want), "{row}");
    }
}

#[test]
#[cfg_attr(debug_assertions, should_panic)]
fn typ_140_slot_over_20_bits() {
    let x = cid(1, 1, 1, 0x10_0005, 7);
    let mut out = [0u8; 24];
    x.encode(&mut out);
    assert_eq!(&out, b"100010101000050000000702");
    assert_eq!(ClientOrderId::decode(&out).map(|c| c.slot), Ok(5));
}

#[test]
fn typ_141_decode_has_no_semantics() {
    let a = ClientOrderId::decode(b"1000563c80000100000002f9").unwrap();
    assert_eq!((a.venue, a.account), (VenueId(99), AccountId(200)));
    let b = ClientOrderId::decode(b"17ffffffffffffffffffff17").unwrap();
    assert_eq!(b.epoch, 0x7FFF);
}

#[test]
fn typ_142_cid_err_codes() {
    assert_eq!(CidErr::Length as u8, 1, "TYP-142.1");
    assert_eq!(CidErr::Charset as u8, 2, "TYP-142.2");
    assert_eq!(CidErr::Version as u8, 3, "TYP-142.3");
    assert_eq!(CidErr::Check as u8, 4, "TYP-142.4");
    assert_eq!(CidErr::Reserved as u8, 5, "TYP-142.5");
    assert_eq!(ClientOrderId::RESERVED_BIT, 0x8000, "TYP-142.6");
}
