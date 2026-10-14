//! `Base64(HMAC-SHA256(secret, prehash))` for the WS login (`ts_s + "GET" + "/user/verify"`) and
//! REST requests (`ts_ms + METHOD + path?query + body`) — README "实现说明" 8. The key is borrowed,
//! never stored (contract 7).

use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use hmac::{Hmac, Mac};
use sha2::Sha256;

/// Base64 HMAC-SHA256 of the concatenation of `parts` (44 characters).
pub(crate) fn hmac_b64(secret: &[u8], parts: &[&[u8]]) -> Option<[u8; 44]> {
    let mut mac = Hmac::<Sha256>::new_from_slice(secret).ok()?;
    for p in parts {
        mac.update(p);
    }
    let tag = mac.finalize().into_bytes();
    let mut out = [0u8; 44];
    let n = STANDARD.encode_slice(tag, &mut out).ok()?;
    (n == 44).then_some(out)
}

/// The REST signature of README "实现说明" 8 (public for BGT-077).
pub fn rest_sign(
    secret: &[u8],
    ts_ms: &[u8],
    method: &[u8],
    path_query: &[u8],
    body: &[u8],
) -> Option<[u8; 44]> {
    hmac_b64(secret, &[ts_ms, method, path_query, body])
}
