//! TLS: the rustls handshake driver (warm thread) and the record layer used after it (README §1–§4).
//!
//! The handshake runs on a rustls `UnbufferedClientConnection` over the connection's own rx/tx
//! buffers. When it reaches `WriteTraffic` the session is converted with
//! `dangerous_into_kernel_connection` (rustls "kernel connection" API): this module then seals and
//! opens records itself, in place on the connection buffers, with aws-lc-rs AEAD keys built from the
//! extracted traffic secrets. rustls keeps the key schedule (`update_rx_secret`/`update_tx_secret`
//! for KeyUpdate) and the session-ticket store.
//!
//! **The only record-encryption sites of the crate** (hot-path-rules T6, README §3): `seal_app_record`
//! (every application record: `Conn::send_frame`, control frames, the Upgrade request) and
//! `encode_handshake_or_keyupdate` (our KeyUpdate; handshake flights are encoded by rustls and
//! copied by `encode_handshake_flight`). There is no scratch session
//! (`warm` is a no-op while the optional TLS warm-up is off).

use aws_lc_rs::aead::{AES_128_GCM, AES_256_GCM, Aad, CHACHA20_POLY1305, LessSafeKey, Nonce, UnboundKey};
use rustls::client::{ClientConnectionData, UnbufferedClientConnection};
use rustls::kernel::KernelConnection;
use rustls::unbuffered::{ConnectionState, UnbufferedStatus};
use rustls::{ConnectionTrafficSecrets, ProtocolVersion};

use crate::types::TlsInfo;

/// Record header.
pub(crate) const REC_HDR: usize = 5;
/// Largest plaintext of one record.
pub(crate) const MAX_PLAIN: usize = 16_384;
/// Largest accepted record body (TLS 1.3: plaintext + content type + padding + tag ≤ 2^14 + 256).
pub(crate) const MAX_BODY: usize = MAX_PLAIN + 256;
/// AEAD tag.
const TAG: usize = 16;

pub(crate) const CT_ALERT: u8 = 21;
pub(crate) const CT_HANDSHAKE: u8 = 22;
pub(crate) const CT_APP: u8 = 23;
const CT_CCS: u8 = 20;

/// Handshake message types handled after the handshake.
const HS_NEW_SESSION_TICKET: u8 = 4;
const HS_KEY_UPDATE: u8 = 24;

/// Re-key the send direction (TLS 1.3 KeyUpdate) before this many records under one key.
const TX_REKEY_AT: u64 = 1 << 23;

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
enum Alg {
    Aes128Gcm,
    Aes256Gcm,
    Chacha,
}

/// One direction's AEAD state.
struct Dir {
    key: LessSafeKey,
    iv: [u8; 12],
    seq: u64,
    alg: Alg,
}

impl Dir {
    fn from_secrets(seq: u64, s: ConnectionTrafficSecrets) -> Option<Dir> {
        let (alg, key, iv) = match s {
            ConnectionTrafficSecrets::Aes128Gcm { key, iv } => (Alg::Aes128Gcm, key, iv),
            ConnectionTrafficSecrets::Aes256Gcm { key, iv } => (Alg::Aes256Gcm, key, iv),
            ConnectionTrafficSecrets::Chacha20Poly1305 { key, iv } => (Alg::Chacha, key, iv),
            _ => return None,
        };
        let a = match alg {
            Alg::Aes128Gcm => &AES_128_GCM,
            Alg::Aes256Gcm => &AES_256_GCM,
            Alg::Chacha => &CHACHA20_POLY1305,
        };
        let key = LessSafeKey::new(UnboundKey::new(a, key.as_ref()).ok()?);
        let mut ivb = [0u8; 12];
        if iv.as_ref().len() != 12 {
            return None;
        }
        ivb.copy_from_slice(iv.as_ref());
        Some(Dir {
            key,
            iv: ivb,
            seq,
            alg,
        })
    }

    /// `iv ^ seq` (seq as a 96-bit big-endian integer).
    #[inline]
    fn nonce(&self) -> [u8; 12] {
        let mut n = self.iv;
        let s = self.seq.to_be_bytes();
        for (a, b) in n[4..].iter_mut().zip(s.iter()) {
            *a ^= b;
        }
        n
    }
}

/// Where the plaintext of an opened record lies inside the record slice.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub(crate) struct Opened {
    pub ctype: u8,
    pub off: usize,
    pub len: usize,
}

/// Outcome of processing post-handshake handshake messages.
#[derive(Copy, Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct HsOutcome {
    /// The peer's KeyUpdate asked for ours.
    pub reply_key_update: bool,
    /// A KeyUpdate was processed.
    pub key_update: bool,
    /// Malformed or unexpected message.
    pub proto: bool,
}

/// The record layer after the handshake.
pub(crate) struct RecordLayer {
    v13: bool,
    rx: Dir,
    tx: Dir,
    kernel: KernelConnection<ClientConnectionData>,
    pub info: TlsInfo,
    /// Records opened (test hook, NET-089).
    pub opened: u64,
    /// NewSessionTicket messages seen.
    pub tickets: u32,
    /// Whether tickets are handed to rustls (warm thread) or only counted (hot thread).
    pub store_tickets: bool,
    /// Bytes of a post-handshake message fragmented across records still to skip.
    hs_skip: usize,
}

impl std::fmt::Debug for RecordLayer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RecordLayer")
            .field("v13", &self.v13)
            .field("info", &self.info)
            .field("rx_seq", &self.rx.seq)
            .field("tx_seq", &self.tx.seq)
            .finish()
    }
}

/// The connection's TLS state.
#[derive(Default)]
pub(crate) enum Tls {
    #[default]
    None,
    Handshaking(Box<UnbufferedClientConnection>),
    Ready(Box<RecordLayer>),
}

impl std::fmt::Debug for Tls {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Tls::None => f.write_str("None"),
            Tls::Handshaking(_) => f.write_str("Handshaking"),
            Tls::Ready(r) => r.fmt(f),
        }
    }
}

/// Handshake failure as reported in `BuildFail`.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub(crate) enum HsFail {
    Alert(u8),
    Tls,
}

/// One handshake step's result.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub(crate) enum HsStep {
    /// Waiting for peer bytes (the caller sends what was encoded into tx, then reads).
    NeedRead,
    Done(TlsInfo),
    Failed(HsFail),
}

fn map_err(e: &rustls::Error) -> HsFail {
    match e {
        rustls::Error::AlertReceived(d) => HsFail::Alert(u8::from(*d)),
        _ => HsFail::Tls,
    }
}

/// Drives the handshake as far as the buffered peer bytes `rx[*cs..ce]` allow; encoded flights
/// are appended at `tx[*te..]`. On completion converts `tls` into the record layer.
pub(crate) fn handshake_step(
    tls: &mut Tls,
    rx: &mut [u8],
    cs: &mut usize,
    ce: usize,
    tx: &mut [u8],
    te: &mut usize,
) -> HsStep {
    let Tls::Handshaking(uc) = tls else {
        return HsStep::Failed(HsFail::Tls);
    };
    // Bounded: every iteration either encodes a flight, consumes peer bytes, or returns.
    for _ in 0..256 {
        let Some(incoming) = rx.get_mut(*cs..ce) else {
            return HsStep::Failed(HsFail::Tls);
        };
        let UnbufferedStatus { discard, state } = uc.process_tls_records(incoming);
        enum Next {
            Again,
            Wait,
            Complete,
            Fail(HsFail),
        }
        let next = match state {
            Err(e) => Next::Fail(map_err(&e)),
            Ok(ConnectionState::EncodeTlsData(mut s)) => {
                let out = tx.get_mut(*te..).unwrap_or(&mut []);
                match encode_handshake_flight(&mut s, out) {
                    Some(n) => {
                        *te += n;
                        Next::Again
                    }
                    None => Next::Fail(HsFail::Tls),
                }
            }
            Ok(ConnectionState::TransmitTlsData(t)) => {
                // The flight is in our tx buffer; the caller transmits it exactly once.
                t.done();
                Next::Again
            }
            Ok(ConnectionState::BlockedHandshake) => Next::Wait,
            Ok(ConnectionState::WriteTraffic(_)) => Next::Complete,
            Ok(_) => Next::Fail(HsFail::Tls),
        };
        *cs += discard;
        match next {
            Next::Again => {}
            Next::Wait => return HsStep::NeedRead,
            Next::Fail(f) => return HsStep::Failed(f),
            Next::Complete => return finish_handshake(tls),
        }
    }
    HsStep::Failed(HsFail::Tls)
}

fn finish_handshake(tls: &mut Tls) -> HsStep {
    let Tls::Handshaking(uc) = std::mem::take(tls) else {
        return HsStep::Failed(HsFail::Tls);
    };
    let version = uc.protocol_version().map(u16::from).unwrap_or(0);
    let suite = uc
        .negotiated_cipher_suite()
        .map(|s| u16::from(s.suite()))
        .unwrap_or(0);
    let group = uc
        .negotiated_key_exchange_group()
        .map(|g| u16::from(g.name()))
        .unwrap_or(0);
    let info = TlsInfo {
        version,
        suite,
        group,
    };
    let v13 = uc.protocol_version() == Some(ProtocolVersion::TLSv1_3);
    let Ok((secrets, kernel)) = uc.dangerous_into_kernel_connection() else {
        return HsStep::Failed(HsFail::Tls);
    };
    let (Some(rx), Some(tx)) = (
        Dir::from_secrets(secrets.rx.0, secrets.rx.1),
        Dir::from_secrets(secrets.tx.0, secrets.tx.1),
    ) else {
        return HsStep::Failed(HsFail::Tls);
    };
    *tls = Tls::Ready(Box::new(RecordLayer {
        v13,
        rx,
        tx,
        kernel,
        info,
        opened: 0,
        tickets: 0,
        store_tickets: true,
        hs_skip: 0,
    }));
    HsStep::Done(info)
}

/// Copies one handshake flight chunk rustls encoded (warm thread; part of the
/// `encode_handshake_or_keyupdate` entry). Returns the bytes written.
fn encode_handshake_flight(
    s: &mut rustls::unbuffered::EncodeTlsData<'_, ClientConnectionData>,
    out: &mut [u8],
) -> Option<usize> {
    s.encode(out).ok()
}

impl RecordLayer {
    #[cfg(feature = "test-io")]
    pub fn tx_seq(&self) -> u64 {
        self.tx.seq
    }

    /// Ciphertext bytes of one record carrying `n` plaintext bytes.
    #[inline]
    fn rec_overhead(&self) -> usize {
        if self.v13 {
            REC_HDR + 1 + TAG
        } else if self.tx.alg == Alg::Chacha {
            REC_HDR + TAG
        } else {
            REC_HDR + 8 + TAG
        }
    }

    /// Upper bound of the ciphertext for `n` application bytes (records of at most 2^14 bytes).
    #[inline]
    pub fn sealed_len(&self, n: usize) -> usize {
        let recs = n.div_ceil(MAX_PLAIN).max(1);
        n + recs * self.rec_overhead()
    }

    /// Upper bound of our KeyUpdate record.
    pub fn key_update_len(&self) -> usize {
        REC_HDR + 5 + 1 + TAG
    }

    /// T6 entry: seals `data` as application-data record(s) into `out` (the caller checked
    /// `sealed_len`). Advances the send sequence number once per record. Returns bytes written.
    #[inline]
    pub fn seal_app_record(&mut self, data: &[u8], out: &mut [u8]) -> Option<usize> {
        if self.v13 && self.tx.seq >= TX_REKEY_AT {
            let n = self.encode_handshake_or_keyupdate(false, out)?;
            return Some(n + self.seal_app_record(data, out.get_mut(n..)?)?);
        }
        let mut w = 0usize;
        let mut rest = data;
        loop {
            let take = rest.len().min(MAX_PLAIN);
            let (chunk, tail) = rest.split_at(take);
            w += self.seal_one(CT_APP, chunk, out.get_mut(w..)?)?;
            rest = tail;
            if rest.is_empty() {
                break;
            }
        }
        Some(w)
    }

    /// One record of content type `ct` (inner type in TLS 1.3).
    #[inline]
    fn seal_one(&mut self, ct: u8, p: &[u8], out: &mut [u8]) -> Option<usize> {
        let n = p.len();
        let nonce = self.tx.nonce();
        if self.v13 {
            let body = n + 1 + TAG;
            let total = REC_HDR + body;
            let o = out.get_mut(..total)?;
            o[0] = CT_APP;
            o[1] = 3;
            o[2] = 3;
            o[3..5].copy_from_slice(&(body as u16).to_be_bytes());
            let hdr = [o[0], o[1], o[2], o[3], o[4]];
            o[REC_HDR..REC_HDR + n].copy_from_slice(p);
            o[REC_HDR + n] = ct;
            let (head, tag_area) = o.split_at_mut(REC_HDR + n + 1);
            let tag = self
                .tx
                .key
                .seal_in_place_separate_tag(
                    Nonce::assume_unique_for_key(nonce),
                    Aad::from(hdr),
                    &mut head[REC_HDR..],
                )
                .ok()?;
            tag_area.copy_from_slice(tag.as_ref());
            self.tx.seq += 1;
            Some(total)
        } else {
            let explicit = if self.tx.alg == Alg::Chacha { 0 } else { 8 };
            let body = explicit + n + TAG;
            let total = REC_HDR + body;
            let o = out.get_mut(..total)?;
            o[0] = ct;
            o[1] = 3;
            o[2] = 3;
            o[3..5].copy_from_slice(&(body as u16).to_be_bytes());
            if explicit == 8 {
                o[REC_HDR..REC_HDR + 8].copy_from_slice(&nonce[4..]);
            }
            let aad = tls12_aad(self.tx.seq, ct, n);
            let start = REC_HDR + explicit;
            o[start..start + n].copy_from_slice(p);
            let (head, tag_area) = o.split_at_mut(start + n);
            let tag = self
                .tx
                .key
                .seal_in_place_separate_tag(
                    Nonce::assume_unique_for_key(nonce),
                    Aad::from(aad),
                    &mut head[start..],
                )
                .ok()?;
            tag_area.copy_from_slice(tag.as_ref());
            self.tx.seq += 1;
            Some(total)
        }
    }

    /// T6 entry: our KeyUpdate (TLS 1.3), then the send key moves on.
    pub fn encode_handshake_or_keyupdate(&mut self, request: bool, out: &mut [u8]) -> Option<usize> {
        if !self.v13 {
            return None;
        }
        let msg = [HS_KEY_UPDATE, 0, 0, 1, u8::from(request)];
        let n = self.seal_one(CT_HANDSHAKE, &msg, out)?;
        let (seq, s) = self.kernel.update_tx_secret().ok()?;
        self.tx = Dir::from_secrets(seq, s)?;
        Some(n)
    }

    /// Opens one complete record `rec` (header + body) in place.
    #[inline]
    pub fn open_record(&mut self, rec: &mut [u8]) -> Option<Opened> {
        if rec.len() < REC_HDR {
            return None;
        }
        let outer = rec[0];
        let body_len = rec.len() - REC_HDR;
        if self.v13 {
            match outer {
                CT_APP => {}
                // A plaintext alert after the handshake is still an alert.
                CT_ALERT => {
                    return Some(Opened {
                        ctype: CT_ALERT,
                        off: REC_HDR,
                        len: body_len,
                    });
                }
                _ => return None,
            }
            if body_len < TAG + 1 {
                return None;
            }
            let hdr = [rec[0], rec[1], rec[2], rec[3], rec[4]];
            let nonce = self.rx.nonce();
            let plain_len = self
                .rx
                .key
                .open_in_place(
                    Nonce::assume_unique_for_key(nonce),
                    Aad::from(hdr),
                    &mut rec[REC_HDR..],
                )
                .ok()?
                .len();
            self.rx.seq += 1;
            self.opened += 1;
            // Strip the zero padding; the last non-zero byte is the content type.
            let p = &rec[REC_HDR..REC_HDR + plain_len];
            let k = p.iter().rposition(|&b| b != 0)?;
            Some(Opened {
                ctype: p[k],
                off: REC_HDR,
                len: k,
            })
        } else {
            if outer == CT_CCS {
                return None;
            }
            let explicit = if self.rx.alg == Alg::Chacha { 0 } else { 8 };
            if body_len < explicit + TAG {
                return None;
            }
            let n = body_len - explicit - TAG;
            let nonce = if explicit == 8 {
                let mut x = [0u8; 12];
                x[..4].copy_from_slice(&self.rx.iv[..4]);
                x[4..].copy_from_slice(&rec[REC_HDR..REC_HDR + 8]);
                x
            } else {
                self.rx.nonce()
            };
            let aad = tls12_aad(self.rx.seq, outer, n);
            let start = REC_HDR + explicit;
            let plain_len = self
                .rx
                .key
                .open_in_place(
                    Nonce::assume_unique_for_key(nonce),
                    Aad::from(aad),
                    &mut rec[start..],
                )
                .ok()?
                .len();
            self.rx.seq += 1;
            self.opened += 1;
            Some(Opened {
                ctype: outer,
                off: start,
                len: plain_len,
            })
        }
    }

    /// Post-handshake handshake messages of one record (TLS 1.3): NewSessionTicket (stored on the
    /// warm thread, counted always) and KeyUpdate (the receive key moves on).
    pub fn handshake_msgs(&mut self, mut m: &[u8]) -> HsOutcome {
        let mut out = HsOutcome::default();
        if !self.v13 {
            // TLS 1.2 renegotiation requests are ignored.
            return out;
        }
        if self.hs_skip > 0 {
            let k = self.hs_skip.min(m.len());
            self.hs_skip -= k;
            m = &m[k..];
        }
        // Bounded by the record length: every message consumes at least 4 bytes.
        while !m.is_empty() {
            let [typ, a, b, c, rest @ ..] = m else {
                out.proto = true;
                return out;
            };
            let len = u32::from_be_bytes([0, *a, *b, *c]) as usize;
            match *typ {
                HS_NEW_SESSION_TICKET => {
                    self.tickets = self.tickets.saturating_add(1);
                    if rest.len() < len {
                        self.hs_skip = len - rest.len();
                        return out;
                    }
                    if self.store_tickets {
                        let _ = self.kernel.handle_new_session_ticket(&rest[..len]);
                    }
                    m = &rest[len..];
                }
                HS_KEY_UPDATE => {
                    if len != 1 || rest.is_empty() || rest[0] > 1 {
                        out.proto = true;
                        return out;
                    }
                    match self
                        .kernel
                        .update_rx_secret()
                        .ok()
                        .and_then(|(seq, s)| Dir::from_secrets(seq, s))
                    {
                        Some(d) => self.rx = d,
                        None => {
                            out.proto = true;
                            return out;
                        }
                    }
                    out.key_update = true;
                    out.reply_key_update |= rest[0] == 1;
                    m = &rest[1..];
                }
                _ => {
                    out.proto = true;
                    return out;
                }
            }
        }
        out
    }
}

#[inline]
fn tls12_aad(seq: u64, ct: u8, len: usize) -> [u8; 13] {
    let mut a = [0u8; 13];
    a[..8].copy_from_slice(&seq.to_be_bytes());
    a[8] = ct;
    a[9] = 3;
    a[10] = 3;
    a[11..].copy_from_slice(&(len as u16).to_be_bytes());
    a
}

/// Builds the client configuration (warm thread, startup): aws-lc-rs provider, suites in the
/// order AES-128-GCM, AES-256-GCM, ChaCha20 (TLS 1.3 first, TLS 1.2 allowed), key exchange
/// X25519MLKEM768, X25519, P-256, P-384; webpki-roots plus `extra_ca` (only with
/// `endpoint_override`); secret extraction on (kernel connection).
pub(crate) fn client_config(extra_ca: Option<&[u8]>) -> Result<rustls::ClientConfig, ConfigErr> {
    use rustls::crypto::aws_lc_rs::{self as p, cipher_suite as cs, kx_group as kx};
    let provider = rustls::crypto::CryptoProvider {
        cipher_suites: vec![
            cs::TLS13_AES_128_GCM_SHA256,
            cs::TLS13_AES_256_GCM_SHA384,
            cs::TLS13_CHACHA20_POLY1305_SHA256,
            cs::TLS_ECDHE_ECDSA_WITH_AES_128_GCM_SHA256,
            cs::TLS_ECDHE_RSA_WITH_AES_128_GCM_SHA256,
            cs::TLS_ECDHE_ECDSA_WITH_AES_256_GCM_SHA384,
            cs::TLS_ECDHE_RSA_WITH_AES_256_GCM_SHA384,
            cs::TLS_ECDHE_ECDSA_WITH_CHACHA20_POLY1305_SHA256,
            cs::TLS_ECDHE_RSA_WITH_CHACHA20_POLY1305_SHA256,
        ],
        kx_groups: vec![kx::X25519MLKEM768, kx::X25519, kx::SECP256R1, kx::SECP384R1],
        ..p::default_provider()
    };
    let mut roots = rustls::RootCertStore::empty();
    roots.extend(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());
    if let Some(pem) = extra_ca {
        use rustls::pki_types::CertificateDer;
        use rustls::pki_types::pem::PemObject;
        let mut n = 0usize;
        for c in CertificateDer::pem_slice_iter(pem) {
            let c = c.map_err(|_| ConfigErr::ExtraCa)?;
            roots.add(c).map_err(|_| ConfigErr::ExtraCa)?;
            n += 1;
        }
        if n == 0 {
            return Err(ConfigErr::ExtraCa);
        }
    }
    let mut cfg = rustls::ClientConfig::builder_with_provider(provider.into())
        .with_protocol_versions(&[&rustls::version::TLS13, &rustls::version::TLS12])
        .map_err(|_| ConfigErr::Tls)?
        .with_root_certificates(roots)
        .with_no_client_auth();
    cfg.enable_secret_extraction = true;
    Ok(cfg)
}

/// Starts a client handshake toward `sni` (warm thread).
pub(crate) fn start(cfg: &rustls::ClientConfig, sni: &str) -> Option<Tls> {
    let name = rustls::pki_types::ServerName::try_from(sni.to_string()).ok()?;
    let uc = UnbufferedClientConnection::new(cfg.clone().into(), name).ok()?;
    Some(Tls::Handshaking(Box::new(uc)))
}

/// Configuration errors (startup).
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum ConfigErr {
    /// `extra_ca` unreadable or not a PEM certificate.
    ExtraCa,
    /// rustls rejected the protocol/provider combination.
    Tls,
    /// `tls_mode = buffered` (M3).
    BufferedTls,
    /// `warm_interval_ns > 0`: the scratch TLS warm-up is an optional extension not built yet.
    WarmUp,
    /// `tx_timestamps = true`: optional extension not built yet.
    TxTimestamps,
    /// `rss_plan` set: optional extension not built yet.
    RssPlan,
}
