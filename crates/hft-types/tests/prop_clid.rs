//! Property tests for `ClientOrderId` (TYP-138, TYP-139).

#![allow(
    clippy::disallowed_types,
    clippy::disallowed_macros,
    clippy::disallowed_methods
)] // allow-list: test code, not a hot path

use hft_types::*;
use proptest::prelude::*;

fn cid() -> impl Strategy<Value = ClientOrderId> {
    let epoch = prop_oneof![0u16..=0x7FFF, prop::sample::select(vec![0u16, 0x7FFF])];
    let slot = prop_oneof![0u32..=0xF_FFFF, prop::sample::select(vec![0u32, 0xF_FFFF])];
    let r#gen = prop_oneof![any::<u32>(), prop::sample::select(vec![0u32, u32::MAX])];
    (epoch, any::<u8>(), any::<u8>(), slot, r#gen).prop_map(|(epoch, v, a, slot, r#gen)| ClientOrderId {
        epoch,
        venue: VenueId(v),
        account: AccountId(a),
        slot,
        r#gen,
    })
}

fn encode(x: &ClientOrderId) -> [u8; 24] {
    let mut out = [0u8; 24];
    x.encode(&mut out);
    out
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(10_000))]

    #[test]
    fn typ_138_cid_round_trip(x in cid()) {
        let t = encode(&x);
        prop_assert!(t.iter().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(b)));
        prop_assert_eq!(t[0], b'1');
        prop_assert_eq!(ClientOrderId::decode(&t), Ok(x));
    }

    #[test]
    fn typ_139_single_bit_flip_detected(x in cid(), bit in 0u32..96) {
        let t = encode(&x);
        let v = u128::from_str_radix(std::str::from_utf8(&t).unwrap(), 16).unwrap();
        let flipped = format!("{:024x}", v ^ (1u128 << bit));
        let want = if (92..96).contains(&bit) { CidErr::Version } else { CidErr::Check };
        prop_assert_eq!(ClientOrderId::decode(flipped.as_bytes()), Err(want));
    }
}
