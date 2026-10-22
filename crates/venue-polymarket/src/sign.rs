//! Order and API signatures of CLOB V2 (README "签名"): EIP-712 `Order` digests for the CTF
//! Exchange, the L1 `ClobAuth` digest, secp256k1 signatures as `r ‖ s ‖ v` (v = 27 + recovery id),
//! L2 request HMACs and EIP-55 addresses. Pure functions; no allocation.

use base64::Engine;
use base64::engine::general_purpose::URL_SAFE;
use hmac::{Hmac, Mac};
use k256::ecdsa::SigningKey;
use sha2::Sha256;
use sha3::{Digest, Keccak256};

/// Polygon mainnet.
pub const CHAIN_ID: u64 = 137;
/// CTF Exchange V2 (`verifyingContract` of order signatures).
pub const EXCHANGE_V2: [u8; 20] = hex20("E111180000d2663C0091e4f400237545B87B996B");
/// `EIP712Domain` type of the exchange domain.
const DOMAIN4: &[u8] = b"EIP712Domain(string name,string version,uint256 chainId,address verifyingContract)";
/// `EIP712Domain` type of the auth domain (no contract).
const DOMAIN3: &[u8] = b"EIP712Domain(string name,string version,uint256 chainId)";
const ORDER_TYPE: &[u8] = b"Order(uint256 salt,address maker,address signer,uint256 tokenId,uint256 makerAmount,uint256 takerAmount,uint8 side,uint8 signatureType,uint256 timestamp,bytes32 metadata,bytes32 builder)";
const AUTH_TYPE: &[u8] = b"ClobAuth(address address,string timestamp,uint256 nonce,string message)";
const AUTH_MESSAGE: &[u8] = b"This message attests that I control the given wallet";

/// Signature failures.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum SignErr {
    /// The private key is not 32 bytes of hex (optional `0x`) or not a valid scalar.
    BadKey,
    /// The API secret is not URL-safe base64.
    BadSecret,
    /// An output buffer is too small.
    Short,
}

/// keccak-256.
pub fn keccak(data: &[u8]) -> [u8; 32] {
    Keccak256::digest(data).into()
}

/// A secp256k1 wallet (EOA): signing key and its address.
#[derive(Clone)]
pub struct Wallet {
    key: SigningKey,
    address: [u8; 20],
}

impl core::fmt::Debug for Wallet {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        // Never the key.
        f.debug_struct("Wallet")
            .field("address", &self.address)
            .finish_non_exhaustive()
    }
}

impl Wallet {
    /// From 32 bytes of hex (`0x` optional, surrounding whitespace ignored).
    pub fn from_hex(s: &str) -> Result<Wallet, SignErr> {
        let s = s.trim();
        let s = s.strip_prefix("0x").unwrap_or(s);
        let mut k = [0u8; 32];
        if s.len() != 64 || !decode_hex(s.as_bytes(), &mut k) {
            return Err(SignErr::BadKey);
        }
        let key = SigningKey::from_slice(&k).map_err(|_| SignErr::BadKey)?;
        k.fill(0);
        let point = key.verifying_key().to_encoded_point(false);
        let h = keccak(point.as_bytes().get(1..).unwrap_or(&[]));
        let mut address = [0u8; 20];
        address.copy_from_slice(&h[12..]);
        Ok(Wallet { key, address })
    }

    pub fn address(&self) -> [u8; 20] {
        self.address
    }

    /// `r ‖ s ‖ v` over a 32-byte digest (RFC 6979 nonces; low-s; v = 27 + recovery id).
    pub fn sign(&self, digest: &[u8; 32]) -> [u8; 65] {
        let mut out = [0u8; 65];
        if let Ok((sig, rid)) = self.key.sign_prehash_recoverable(digest) {
            let (sig, rid) = match sig.normalize_s() {
                Some(n) => (
                    n,
                    k256::ecdsa::RecoveryId::new(!rid.is_y_odd(), rid.is_x_reduced()),
                ),
                None => (sig, rid),
            };
            out[..64].copy_from_slice(&sig.to_bytes());
            out[64] = 27 + rid.to_byte();
        }
        out
    }
}

/// An unsigned V2 order (README "签名"): amounts in 1e-6 units, `side` 0 = buy, 1 = sell.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub struct OrderV2 {
    pub salt: u64,
    pub maker: [u8; 20],
    pub signer: [u8; 20],
    /// uint256, big-endian.
    pub token_id: [u8; 32],
    pub maker_amount: u64,
    pub taker_amount: u64,
    pub side: u8,
    pub signature_type: u8,
    /// Creation time, unix ms.
    pub timestamp_ms: u64,
    pub metadata: [u8; 32],
    pub builder: [u8; 32],
}

fn word_u64(v: u64) -> [u8; 32] {
    let mut w = [0u8; 32];
    w[24..].copy_from_slice(&v.to_be_bytes());
    w
}

fn word_addr(a: &[u8; 20]) -> [u8; 32] {
    let mut w = [0u8; 32];
    w[12..].copy_from_slice(a);
    w
}

/// Domain separator of the exchange (`name`, `version`, chain, contract).
pub fn exchange_domain(version: &str, chain: u64, contract: &[u8; 20]) -> [u8; 32] {
    let mut h = Keccak256::new();
    h.update(keccak(DOMAIN4));
    h.update(keccak(b"Polymarket CTF Exchange"));
    h.update(keccak(version.as_bytes()));
    h.update(word_u64(chain));
    h.update(word_addr(contract));
    h.finalize().into()
}

/// The EIP-712 digest of `o` under `domain`.
pub fn order_digest(domain: &[u8; 32], o: &OrderV2) -> [u8; 32] {
    let mut h = Keccak256::new();
    h.update(keccak(ORDER_TYPE));
    h.update(word_u64(o.salt));
    h.update(word_addr(&o.maker));
    h.update(word_addr(&o.signer));
    h.update(o.token_id);
    h.update(word_u64(o.maker_amount));
    h.update(word_u64(o.taker_amount));
    h.update(word_u64(u64::from(o.side)));
    h.update(word_u64(u64::from(o.signature_type)));
    h.update(word_u64(o.timestamp_ms));
    h.update(o.metadata);
    h.update(o.builder);
    let s: [u8; 32] = h.finalize().into();
    typed(domain, &s)
}

fn typed(domain: &[u8; 32], s: &[u8; 32]) -> [u8; 32] {
    let mut h = Keccak256::new();
    h.update([0x19, 0x01]);
    h.update(domain);
    h.update(s);
    h.finalize().into()
}

/// The L1 `ClobAuth` digest (domain `ClobAuthDomain` version 1) for `address`, unix seconds
/// `timestamp` and `nonce`.
pub fn auth_digest(chain: u64, address: &[u8; 20], timestamp: u64, nonce: u64) -> [u8; 32] {
    let mut d = Keccak256::new();
    d.update(keccak(DOMAIN3));
    d.update(keccak(b"ClobAuthDomain"));
    d.update(keccak(b"1"));
    d.update(word_u64(chain));
    let domain: [u8; 32] = d.finalize().into();
    let mut ts = [0u8; 20];
    let ts = dec_u64(timestamp, &mut ts);
    let mut h = Keccak256::new();
    h.update(keccak(AUTH_TYPE));
    h.update(word_addr(address));
    h.update(keccak(ts));
    h.update(word_u64(nonce));
    h.update(keccak(AUTH_MESSAGE));
    let s: [u8; 32] = h.finalize().into();
    typed(&domain, &s)
}

/// L2 request signature: URL-safe base64 (padded) of HMAC-SHA256 over
/// `timestamp ‖ method ‖ path ‖ body` keyed by the URL-safe-base64-decoded `secret`.
pub fn l2_hmac<'a>(
    secret: &str,
    timestamp: &str,
    method: &str,
    path: &str,
    body: &str,
    out: &'a mut [u8; 44],
) -> Result<&'a str, SignErr> {
    let mut key = [0u8; 96];
    let n = URL_SAFE
        .decode_slice(secret.as_bytes(), &mut key)
        .map_err(|_| SignErr::BadSecret)?;
    let mut m =
        <Hmac<Sha256> as Mac>::new_from_slice(key.get(..n).unwrap_or(&[])).map_err(|_| SignErr::BadSecret)?;
    key.fill(0);
    m.update(timestamp.as_bytes());
    m.update(method.as_bytes());
    m.update(path.as_bytes());
    m.update(body.as_bytes());
    let tag = m.finalize().into_bytes();
    let w = URL_SAFE.encode_slice(tag, out).map_err(|_| SignErr::Short)?;
    core::str::from_utf8(out.get(..w).unwrap_or(&[])).map_err(|_| SignErr::Short)
}

/// `0x` + EIP-55 mixed-case hex of `a`.
pub fn checksum_address<'a>(a: &[u8; 20], out: &'a mut [u8; 42]) -> &'a str {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut lower = [0u8; 40];
    for (i, b) in a.iter().enumerate() {
        lower[2 * i] = HEX[usize::from(b >> 4)];
        lower[2 * i + 1] = HEX[usize::from(b & 15)];
    }
    let h = keccak(&lower);
    out[0] = b'0';
    out[1] = b'x';
    for (i, c) in lower.iter().enumerate() {
        let nib = (h[i / 2] >> if i % 2 == 0 { 4 } else { 0 }) & 15;
        out[2 + i] = if c.is_ascii_alphabetic() && nib >= 8 {
            c.to_ascii_uppercase()
        } else {
            *c
        };
    }
    core::str::from_utf8(out).unwrap_or("")
}

/// `0x` + lower-case hex of a 65-byte signature.
pub fn hex_signature<'a>(sig: &[u8; 65], out: &'a mut [u8; 132]) -> &'a str {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    out[0] = b'0';
    out[1] = b'x';
    for (i, b) in sig.iter().enumerate() {
        out[2 + 2 * i] = HEX[usize::from(b >> 4)];
        out[3 + 2 * i] = HEX[usize::from(b & 15)];
    }
    core::str::from_utf8(out).unwrap_or("")
}

/// A decimal uint256 (token ids) as 32 big-endian bytes; `None` when not decimal or ≥ 2²⁵⁶.
pub fn parse_u256(s: &str) -> Option<[u8; 32]> {
    if s.is_empty() || s.len() > 78 {
        return None;
    }
    let mut out = [0u8; 32];
    for c in s.bytes() {
        let d = c.checked_sub(b'0').filter(|d| *d <= 9)?;
        // out = out × 10 + d
        let mut carry = u16::from(d);
        for b in out.iter_mut().rev() {
            let v = u16::from(*b) * 10 + carry;
            *b = (v & 0xff) as u8;
            carry = v >> 8;
        }
        if carry != 0 {
            return None;
        }
    }
    Some(out)
}

fn dec_u64(mut v: u64, buf: &mut [u8; 20]) -> &[u8] {
    let mut i = buf.len();
    loop {
        i -= 1;
        buf[i] = b'0' + (v % 10) as u8;
        v /= 10;
        if v == 0 {
            break;
        }
    }
    &buf[i..]
}

fn decode_hex(s: &[u8], out: &mut [u8]) -> bool {
    if s.len() != out.len() * 2 {
        return false;
    }
    let nib = |c: u8| match c {
        b'0'..=b'9' => Some(c - b'0'),
        b'a'..=b'f' => Some(c - b'a' + 10),
        b'A'..=b'F' => Some(c - b'A' + 10),
        _ => None,
    };
    for (i, o) in out.iter_mut().enumerate() {
        match (nib(s[2 * i]), nib(s[2 * i + 1])) {
            (Some(h), Some(l)) => *o = h << 4 | l,
            _ => return false,
        }
    }
    true
}

const fn hex20(s: &str) -> [u8; 20] {
    let b = s.as_bytes();
    let mut out = [0u8; 20];
    let mut i = 0;
    while i < 20 {
        out[i] = nib(b[2 * i]) << 4 | nib(b[2 * i + 1]);
        i += 1;
    }
    out
}

const fn nib(c: u8) -> u8 {
    match c {
        b'0'..=b'9' => c - b'0',
        b'a'..=b'f' => c - b'a' + 10,
        b'A'..=b'F' => c - b'A' + 10,
        _ => 0,
    }
}
