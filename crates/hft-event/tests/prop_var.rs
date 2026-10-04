//! Split / reassemble round trip (EVT-163).

#![allow(
    clippy::disallowed_types,
    clippy::disallowed_macros,
    clippy::disallowed_methods
)] // allow-list: test code, not a hot path

use hft_event::*;
use proptest::prelude::*;

fn body(len: usize, seed: u64) -> Vec<u8> {
    let mut x = seed | 1;
    (0..len)
        .map(|_| {
            x ^= x << 13;
            x ^= x >> 7;
            x ^= x << 17;
            x as u8
        })
        .collect()
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(300))]
    #[test]
    fn evt_163_reassemble_roundtrip(
        groups in prop::collection::vec((0usize..=300 * 1024, any::<u64>(), 0usize..3, any::<bool>()), 1..=5),
    ) {
        let mut buf = vec![0u8; 512 * 1024];
        let mut r = Reassembler::new(&mut buf);
        let mut out = vec![0u8; 65_536];
        let mut seq = 1u64;
        for (len, seed, k, crc) in groups {
            let l = body(len, seed);
            let kind = [EventKind::CONFIG_BLOB, EventKind::STATE_SNAPSHOT, EventKind::CONTROL_CMD][k];
            let parts: Vec<_> = PartIter::new(&l).unwrap().collect();
            let n_parts = parts.len();
            for (i, (ph, chunk, mut f)) in parts.into_iter().enumerate() {
                if crc {
                    f |= RecordFlags::CRC;
                }
                let n = encode_part(&mut out, seq, 42, f, kind, &ph, chunk).unwrap() as usize;
                seq += 1;
                let v = decode_record(&out[..n]).unwrap();
                let got = r.push(&v).unwrap().map(<[u8]>::to_vec);
                if i + 1 < n_parts {
                    prop_assert!(got.is_none());
                } else {
                    prop_assert_eq!(got.as_deref(), Some(&l[..]));
                }
            }
        }
    }
}
