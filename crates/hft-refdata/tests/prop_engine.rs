//! REF-070: `EcfBuilder` → `EngineConfig::load` round-trips every section view byte for byte;
//! encoding is deterministic, `total_len` matches and sections are 8-byte aligned.

#![allow(
    clippy::disallowed_types,
    clippy::disallowed_macros,
    clippy::disallowed_methods
)] // allow-list: test code, not a hot path

use crate::common;

use common::*;
use hft_refdata::{EcfBuilder, EngineConfig, EngineSection};
use hft_types::HostClass;
use proptest::prelude::*;

#[derive(Clone, Debug)]
struct Sec {
    id: EngineSection,
    row_ver: u16,
    row_size: u32,
    bytes: Vec<u8>,
}

fn section() -> impl Strategy<Value = Sec> {
    (0usize..7, 1u16..=4, 1u32..=32, 0usize..=64).prop_flat_map(|(ix, row_ver, words, count)| {
        let row_size = words * 8;
        prop::collection::vec(any::<u8>(), row_size as usize * count).prop_map(move |bytes| Sec {
            id: EngineSection::ALL[ix],
            row_ver,
            row_size,
            bytes,
        })
    })
}

fn host() -> impl Strategy<Value = HostClass> {
    prop::sample::select(vec![
        HostClass::Dev2,
        HostClass::C4,
        HostClass::C8,
        HostClass::Metal,
        HostClass::C16,
        HostClass::C32,
    ])
}

fn encode(version: u64, h: HostClass, secs: &[Sec]) -> Vec<u8> {
    let mut b = EcfBuilder::new(version, h);
    for s in secs {
        let count = (s.bytes.len() / s.row_size as usize) as u32;
        b.section(s.id, s.row_ver, s.row_size, count, s.bytes.clone());
    }
    b.finish()
}

proptest! {
    #![proptest_config(ProptestConfig { cases: 512, ..ProptestConfig::default() })]

    #[test]
    fn ref_070_ecf_roundtrip(version in 1u64.., h in host(), secs in prop::collection::vec(section(), 0..=7)) {
        // One section per id (the first generated one wins).
        let mut uniq: Vec<Sec> = Vec::new();
        for s in secs {
            if !uniq.iter().any(|u| u.id == s.id) {
                uniq.push(s);
            }
        }
        let b = encode(version, h, &uniq);
        prop_assert_eq!(&b, &encode(version, h, &uniq));
        prop_assert_eq!(u64::from_le_bytes(b[40..48].try_into().unwrap_or([0; 8])), b.len() as u64);
        let n = u16::from_le_bytes([b[10], b[11]]) as usize;
        for i in 0..n {
            prop_assert_eq!(dir_entry(&b, i).offset % 8, 0);
        }
        let ec = EngineConfig::load(&b, None);
        prop_assert!(ec.is_ok(), "{:?}", ec.err());
        let ec = ec.unwrap_or_else(|_| unreachable!());
        prop_assert_eq!(ec.host_class(), h);
        for id in EngineSection::ALL {
            let want = uniq.iter().find(|s| s.id == id);
            let got = ec.section(id);
            prop_assert_eq!(got.is_some(), want.is_some());
            if let (Some(g), Some(w)) = (got, want) {
                prop_assert_eq!(g.row_ver, w.row_ver);
                prop_assert_eq!(g.row_size, w.row_size);
                prop_assert_eq!(g.count as usize, w.bytes.len() / w.row_size as usize);
                prop_assert_eq!(g.bytes, &w.bytes[..]);
            }
        }
    }
}
