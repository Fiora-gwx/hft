//! CTP-172: `normalize` of a frame-sized `SetParams` (`blob_b64` decodes to `MAX_BLOB` bytes).

#![allow(
    clippy::disallowed_types,
    clippy::disallowed_macros,
    clippy::disallowed_methods
)] // allow-list: bench code, not a hot path

#[path = "../tests/common/p0.rs"]
mod p0;

use std::hint::black_box;

use base64::Engine as _;
use criterion::{Criterion, criterion_group, criterion_main};
use hft_ctl_proto::{MAX_BLOB, normalize, parse_request};

fn request() -> Vec<u8> {
    let mut b = vec![0u8; MAX_BLOB];
    b[0] = 1; // strategy s1-main
    b[1] = 1; // account acct-a
    let blob = base64::engine::general_purpose::STANDARD.encode(&b);
    format!(
        r#"{{"v":2,"cmd_id":"00000000-0000-4000-8000-000000000001","issuer":"cli:alice","issued_at_ns":1790000000000000000,"expires_at_ns":1790000030000000000,"account":"acct-a","cmd":{{"type":"SetParams","strategy":"s1-main","blob_b64":"{blob}"}}}}"#
    )
    .into_bytes()
}

fn ctp_172_normalize_1m(c: &mut Criterion) {
    let rd = p0::p0();
    let req = parse_request(&request()).unwrap();
    assert!(normalize(&req, &rd).is_ok());
    c.bench_function("ctlproto/normalize_1m", |b| {
        b.iter(|| normalize(black_box(&req), &rd).is_ok())
    });
}

criterion_group!(benches, ctp_172_normalize_1m);
criterion_main!(benches);
