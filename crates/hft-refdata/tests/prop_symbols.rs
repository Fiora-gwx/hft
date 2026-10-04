//! REF-047: on any symbol set of up to 8 192 entries both lookup paths agree with a
//! `BTreeMap` reference, and binary search makes at most ⌈log₂ n⌉ ordered comparisons.

use std::collections::BTreeMap;

use hft_refdata::SymbolIndex;
use hft_types::InstId;
use proptest::prelude::*;

const CHARSET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_-.";

/// Deterministic generator driven by the proptest seed (xorshift64*).
struct Gen(u64);

impl Gen {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        self.0.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }
    fn below(&mut self, n: usize) -> usize {
        (self.next() % n as u64) as usize
    }
    fn symbol(&mut self) -> Vec<u8> {
        let len = 1 + self.below(27);
        (0..len).map(|_| CHARSET[self.below(CHARSET.len())]).collect()
    }
}

fn ceil_log2(n: usize) -> u32 {
    if n <= 1 {
        0
    } else {
        usize::BITS - (n - 1).leading_zeros()
    }
}

proptest! {
    #![proptest_config(ProptestConfig { cases: 256, ..ProptestConfig::default() })]

    #[test]
    fn ref_047_symbol_table_matches_btreemap(seed in 1u64..u64::MAX, n in 1usize..=8192) {
        let mut g = Gen(seed);
        let mut reference: BTreeMap<Vec<u8>, InstId> = BTreeMap::new();
        while reference.len() < n {
            let s = g.symbol();
            let id = InstId(reference.len() as u32 + 1);
            reference.entry(s).or_insert(id);
        }
        let syms: Vec<(&[u8], InstId)> = reference.iter().map(|(k, v)| (k.as_slice(), *v)).collect();
        let t = SymbolIndex::from_entries(&syms, &[]);
        let bound = ceil_log2(n).max(1);
        for (k, v) in &reference {
            prop_assert_eq!(t.get(k), Some(*v));
            let (hit, cmps) = t.bsearch_counted(k);
            prop_assert_eq!(hit, Some(*v));
            prop_assert!(cmps <= bound, "{} comparisons for n = {}", cmps, n);
        }
        let members: Vec<&Vec<u8>> = reference.keys().collect();
        for q in 0..1000 {
            let probe = if q % 2 == 0 {
                let mut m = members[g.below(members.len())].clone();
                let at = g.below(m.len());
                m[at] = CHARSET[g.below(CHARSET.len())];
                m
            } else {
                g.symbol()
            };
            let want = reference.get(&probe).copied();
            prop_assert_eq!(t.get_hashed(&probe), want);
            let (hit, cmps) = t.bsearch_counted(&probe);
            prop_assert_eq!(hit, want);
            prop_assert!(cmps <= bound);
        }
    }
}
