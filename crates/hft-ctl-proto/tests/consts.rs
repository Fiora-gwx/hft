//! CTP-001.
#![allow(
    clippy::type_complexity,
    clippy::disallowed_types,
    clippy::disallowed_methods,
    clippy::disallowed_macros
)] // allow-list: test code, not a hot path

use base64::Engine as _;
use hft_ctl_proto::{MAX_BLOB, MAX_FRAME, PROTO_VERSION, VIEWER_CONFIRM_TTL_MS};

#[test]
fn ctp_001_protocol_constants() {
    assert_eq!(PROTO_VERSION, 2, "CTP-001.1");
    assert_eq!(MAX_FRAME, 1_048_576, "CTP-001.2");
    assert_eq!(MAX_FRAME, 1 << 20, "CTP-001.2");
    assert_eq!(MAX_BLOB, 786_432, "CTP-001.3");
    assert_eq!(MAX_BLOB, MAX_FRAME * 3 / 4, "CTP-001.3");
    assert_eq!(VIEWER_CONFIRM_TTL_MS, 60_000, "CTP-001.4");
    let b = vec![0u8; MAX_BLOB];
    let s = base64::engine::general_purpose::STANDARD.encode(&b);
    assert_eq!(s.len(), 1_048_576, "CTP-001.5");
}
