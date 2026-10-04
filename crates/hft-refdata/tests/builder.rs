//! The encoder is deterministic (REF-004).

mod common;

use common::*;
use sha2::{Digest, Sha256};

#[test]
fn ref_004_builder_deterministic() {
    let a = f0_spec().builder().finish();
    let b = f0_spec().builder().finish();
    assert_eq!(a, b);
    assert_eq!(Sha256::digest(&a), Sha256::digest(&b));
}
