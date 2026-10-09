#![allow(
    clippy::disallowed_types,
    clippy::disallowed_macros,
    clippy::disallowed_methods
)] // allow-list: test code
#![allow(clippy::type_complexity, clippy::too_many_arguments)]
use crate::common;

use std::sync::OnceLock;

use common::*;
use hft_oms::*;
use hft_strategy_sdk::{SnapshotReader, SnapshotWriter};
use proptest::prelude::*;

fn base() -> &'static [u8] {
    static B: OnceLock<Vec<u8>> = OnceLock::new();
    B.get_or_init(|| {
        let (h, _, _) = mixed();
        let mut w = SnapshotWriter::new(1 << 22);
        h.o.snapshot(&mut w);
        w.as_bytes().to_vec()
    })
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(2_000))]

    #[test]
    fn oms_389_restore_never_panics(
        flips in prop::collection::vec((any::<prop::sample::Index>(), any::<u8>()), 1..=16),
        cut in prop::option::of(any::<prop::sample::Index>()),
        tail in prop::collection::vec(any::<u8>(), 0..=64),
    ) {
        let mut b = base().to_vec();
        for (i, x) in flips {
            let at = i.index(b.len());
            b[at] ^= x | 1;
        }
        if let Some(c) = cut {
            b.truncate(c.index(b.len() + 1));
        }
        b.extend_from_slice(&tail);
        let r = std::panic::catch_unwind(|| Oms::restore(&mut SnapshotReader::new(&b), &p0(), rd()).is_ok());
        prop_assert!(r.is_ok(), "restore panicked");
    }
}
