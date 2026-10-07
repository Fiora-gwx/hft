//! `session.logon` signature: Ed25519 over `apiKey=<k>&timestamp=<ms>`, standard base64
//! (README "下单:WS API"). Warm thread only; the key is borrowed, never stored (contract 7).

use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use ed25519_dalek::{Signer, SigningKey};

use crate::wr::Wr;

/// PKCS#8 v1 prefix of an Ed25519 private key (RFC 8410): the 32-byte seed follows.
const PKCS8_PREFIX: [u8; 16] = [
    0x30, 0x2e, 0x02, 0x01, 0x00, 0x30, 0x05, 0x06, 0x03, 0x2b, 0x65, 0x70, 0x04, 0x22, 0x04, 0x20,
];

/// The 32-byte seed of a secret given raw (32 bytes) or as PKCS#8 DER (48 bytes).
pub(crate) fn seed(secret: &[u8]) -> Option<[u8; 32]> {
    let raw = match secret.len() {
        32 => secret,
        48 if secret.get(..16) == Some(&PKCS8_PREFIX[..]) => secret.get(16..)?,
        _ => return None,
    };
    let mut s = [0u8; 32];
    s.copy_from_slice(raw);
    Some(s)
}

/// Base64 signature of `msg` (88 characters).
pub(crate) fn ed25519_b64(secret: &[u8], msg: &[u8]) -> Option<[u8; 88]> {
    let key = SigningKey::from_bytes(&seed(secret)?);
    let sig = key.sign(msg).to_bytes();
    let mut out = [0u8; 88];
    let n = STANDARD.encode_slice(sig, &mut out).ok()?;
    (n == 88).then_some(out)
}

/// The logon payload `apiKey=<k>&timestamp=<ms>` into `buf`; returns its length.
pub(crate) fn logon_payload(api_key: &[u8], now_ms: i64, buf: &mut [u8]) -> Option<usize> {
    let mut w = Wr::new(buf);
    w.bytes(b"apiKey=")
        .bytes(api_key)
        .bytes(b"&timestamp=")
        .i64(now_ms);
    w.finish()
}
