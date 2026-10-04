//! F0 loads; identity is the SHA-256 of the bytes (REF-003).

mod common;

use common::*;
use hft_types::{InstId, VenueId};
use sha2::{Digest, Sha256};

#[test]
fn ref_003_f0_loads() {
    let b = f0();
    let rd = ok(&b);
    assert_eq!(rd.version(), 10);
    let want: [u8; 32] = Sha256::digest(&b).into();
    assert_eq!(rd.sha256(), &want);
    assert!(rd.warnings().is_empty());
    assert_eq!(rd.symbols(VenueId(1)).get(b"BTCUSDT"), Some(InstId(1)));
}
