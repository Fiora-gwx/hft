//! SPB1 round trip with an opaque body (EVT-133).

#![allow(
    clippy::disallowed_types,
    clippy::disallowed_macros,
    clippy::disallowed_methods
)] // allow-list: test code, not a hot path

#[path = "common/spb.rs"]
mod spb;

use hft_event::*;
use hft_types::GroupId;
use proptest::prelude::*;
use spb::*;
use zerocopy::IntoBytes;

fn arb_blob() -> impl Strategy<Value = Vec<u8>> {
    (1u8..=254, 1usize..=500, 0usize..=8192, any::<bool>()).prop_flat_map(|(s, n, body_words, ff)| {
        (
            Just(s),
            proptest::collection::btree_set(1u32..=GroupId::N_MAX, n),
            proptest::collection::vec((1usize..=4, any::<[u32; 4]>(), 0u8..=1), n),
            proptest::collection::vec(any::<u8>(), body_words * 8),
            Just(ff),
            any::<u32>(),
        )
            .prop_map(|(s, ctrs, gs, mut body, ff, hwm_pick)| {
                if ff {
                    body.fill(0xFF);
                }
                let ctrs: Vec<u32> = ctrs.into_iter().collect();
                let max = *ctrs.iter().max().unwrap();
                let hwm = max + hwm_pick % (GroupId::N_MAX - max + 1);
                // Swap the first two ids so both duplicate-check paths (ascending / general) run.
                let mut order = ctrs.clone();
                if order.len() > 2 {
                    order.swap(0, 1);
                }
                let groups: Vec<GroupSpec> = order
                    .iter()
                    .zip(gs)
                    .map(|(c, (nl, insts, open))| {
                        let legs: Vec<LegDef> = insts[..nl]
                            .iter()
                            .map(|i| leg((*i).max(1), LegDef::CEX, 0))
                            .collect();
                        group(u32::from(s) << 24 | c, &legs, open)
                    })
                    .collect();
                blob(
                    &hdr(s, groups.len() as u16, body.len() as u32, hwm),
                    &groups,
                    &body,
                )
            })
    })
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(500))]
    #[test]
    fn evt_133_spb1_roundtrip_opaque_body(b in arb_blob()) {
        let (store, n) = aligned(&b);
        let bytes = &store.as_bytes()[..n];
        let v = parse_spb1(bytes).unwrap();
        let mut back = v.hdr.as_bytes().to_vec();
        back.extend_from_slice(v.groups.as_bytes());
        back.extend_from_slice(v.body);
        prop_assert_eq!(&back, &b);
        prop_assert_eq!(v.body.as_ptr(), bytes[n - v.body.len()..].as_ptr());
    }
}
