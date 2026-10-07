//! NET-060–NET-066: client frames — `MaskedFrame::seal`, its debug assertions, `NotSealed`, mask keys.
#![allow(
    clippy::disallowed_types,
    clippy::disallowed_macros,
    clippy::disallowed_methods
)] // allow-list: test code, not a hot path

mod support;

use hft_net::sock::ScriptIo;
use hft_net::{ConnId, MaskedFrame, Opcode, SendErr};
use proptest::prelude::*;
use support::mempeer::{Opts, pair, pair_with};
use support::ws_ref;
use support::xorshift::XorShift64Star;

#[test]
#[allow(clippy::type_complexity)]
fn net_060_seal_header_encoding() {
    let rows: [(&str, Opcode, usize, Vec<u8>, u16, usize); 4] = [
        (
            "NET-060.1",
            Opcode::Text,
            0,
            vec![0x81, 0x80, 0x11, 0x22, 0x33, 0x44],
            8,
            6,
        ),
        (
            "NET-060.2",
            Opcode::Text,
            125,
            vec![0x81, 0xFD, 0x11, 0x22, 0x33, 0x44],
            8,
            131,
        ),
        (
            "NET-060.3",
            Opcode::Text,
            126,
            vec![0x81, 0xFE, 0x00, 0x7E, 0x11, 0x22, 0x33, 0x44],
            6,
            134,
        ),
        (
            "NET-060.4",
            Opcode::Binary,
            2034,
            vec![0x82, 0xFE, 0x07, 0xF2, 0x11, 0x22, 0x33, 0x44],
            6,
            2042,
        ),
    ];
    for (row, op, len, head, start, total) in rows {
        let mut f = MaskedFrame::<2048>::new();
        f.seal(op, len, 0x1122_3344);
        assert_eq!(&f.as_bytes()[..head.len()], &head[..], "{row}");
        assert_eq!(f.start(), start, "{row}");
        assert_eq!(f.as_bytes().len(), total, "{row}");
    }
}

fn check_seal<const C: usize>(len: usize, key: u32, op: Opcode, seed: u64) -> Result<(), TestCaseError> {
    let mut f = Box::new(MaskedFrame::<C>::new());
    let mut x = XorShift64Star(seed | 1);
    let payload: Vec<u8> = (0..len).map(|_| x.next_u32() as u8).collect();
    f.body_mut()[..len].copy_from_slice(&payload);
    f.seal(op, len, key);
    let want = ws_ref::client_frame(op as u8, &payload, key);
    prop_assert_eq!(f.as_bytes(), &want[..]);
    let (parsed, used) = ws_ref::parse_client(f.as_bytes());
    prop_assert_eq!(used, want.len());
    prop_assert_eq!(&parsed[0].payload, &payload);
    Ok(())
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(10_000))]
    #[test]
    fn net_061_seal_matches_reference(
        small in any::<bool>(),
        len_a in 0usize..=2034,
        len_b in 0usize..=8178,
        key in any::<u32>(),
        text in any::<bool>(),
        seed in any::<u64>(),
    ) {
        let op = if text { Opcode::Text } else { Opcode::Binary };
        if small {
            check_seal::<2048>(len_a, key, op, seed)?;
        } else {
            check_seal::<8192>(len_b, key, op, seed)?;
        }
    }
}

#[test]
#[cfg(debug_assertions)]
#[should_panic(expected = "sealed")]
fn net_062_body_mut_after_seal_asserts() {
    let mut f = MaskedFrame::<2048>::new();
    f.seal(Opcode::Text, 10, 1);
    let _ = f.body_mut();
}

#[test]
#[cfg(debug_assertions)]
fn net_063_seal_len_boundary() {
    for (row, len) in [("NET-063.1", 2033usize), ("NET-063.2", 2034)] {
        let mut f = MaskedFrame::<2048>::new();
        f.seal(Opcode::Text, len, 5);
        assert_eq!(f.sealed(), 1, "{row}");
        assert_eq!(f.as_bytes().len(), len + 8, "{row}");
    }
    let r = std::panic::catch_unwind(|| {
        let mut f = MaskedFrame::<2048>::new();
        f.seal(Opcode::Text, 2035, 5);
    });
    assert!(r.is_err(), "NET-063.3");
}

#[test]
#[cfg(debug_assertions)]
fn net_064_double_seal_asserts() {
    let r = std::panic::catch_unwind(|| {
        let mut f = MaskedFrame::<2048>::new();
        f.seal(Opcode::Text, 3, 7);
        f.seal(Opcode::Text, 3, 7);
    });
    assert!(r.is_err(), "step 1");
    let mut f = MaskedFrame::<2048>::new();
    f.seal(Opcode::Text, 3, 7);
    f.unseal();
    f.body_mut()[..2].copy_from_slice(b"ok");
    f.seal(Opcode::Text, 2, 9);
    assert_eq!(f.sealed(), 1);
    let (p, _) = ws_ref::parse_client(f.as_bytes());
    assert_eq!(p[0].payload, b"ok");
}

#[test]
fn net_065_unsealed_frame_rejected() {
    let (mut conn, mut peer) = pair();
    let mut f = MaskedFrame::<2048>::new();
    f.body_mut()[..10].copy_from_slice(b"0123456789");
    let before = conn.sys().sendmsg;
    assert_eq!(conn.send_frame(&f), Err(SendErr::NotSealed));
    assert_eq!(conn.sys().sendmsg, before);
    assert_eq!(conn.test_tx_seq(), Some(0));
    // The next record the server receives still decrypts as sequence 0.
    conn.send_text(b"after", 0).unwrap();
    peer.pump(&mut conn);
    assert_eq!(peer.recs.len(), 1);
    assert_eq!(peer.recs[0].index, 0);
}

#[test]
fn net_066_mask_key_sequence() {
    let (mut c1, mut peer) = pair_with(Opts {
        id: ConnId(1),
        script: ScriptIo::new(),
        ..Opts::default()
    });
    let (mut c2, _p2) = pair_with(Opts {
        id: ConnId(2),
        ..Opts::default()
    });
    c1.test_set_mask_seed(1);
    c2.test_set_mask_seed(2);
    let mut r = XorShift64Star(1);
    let want: Vec<u32> = (0..1002).map(|_| r.next_u32()).collect();
    let got1: Vec<u32> = (0..1000).map(|_| c1.next_mask()).collect();
    let got2: Vec<u32> = (0..1000).map(|_| c2.next_mask()).collect();
    assert_eq!(&got1[..], &want[..1000]);
    assert_ne!(got1[0], got2[0]);
    c1.send_text(b"a", 0).unwrap();
    c1.send_text(b"b", 0).unwrap();
    peer.pump(&mut c1);
    let fr = peer.client_frames();
    assert_eq!(fr.len(), 2);
    assert_eq!((fr[0].key, fr[1].key), (want[1000], want[1001]));
    assert_eq!(
        (fr[0].payload.as_slice(), fr[1].payload.as_slice()),
        (&b"a"[..], &b"b"[..])
    );
}
