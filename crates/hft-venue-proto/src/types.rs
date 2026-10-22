//! Helper types of the trait signatures (README "辅助类型"). Types only; fixed size, `Copy` and
//! heap-free except the output buffers of the warm-thread methods (`login`, `DeadManOut.reqs`) and
//! `SubFrames` (preallocated once, reused without allocating).

use hft_event::GwOp;
use hft_types::{AccountId, ClientOrderId, ConnId, InstId};

/// Connection role; codes owned by event-model (`Md = 1`, `Order = 2`, `UserStream = 3`, `Rest = 4`).
pub type Role = hft_event::ConnRole;

/// Market-data route of a connection; 0 = the default route.
#[derive(Copy, Clone, Eq, PartialEq, Debug, Default)]
pub struct RouteId(pub u8);

/// WebSocket message type of a frame.
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub enum MsgOp {
    Text,
    Binary,
}

/// Receive metadata of one message, filled by the runtime (48 B, offsets fixed; VPR-001).
#[repr(C)]
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub struct RxMeta {
    /// TSC when `recvmsg` finished the segment carrying the frame's last byte.
    pub rx_tsc: u64,
    /// hft-net `Timestamps::best_rx().0` (HW → SW fallback already done by the caller; 0 = none).
    pub hw_rx_ns: i64,
    /// Kernel software receive stamp; copied into the event's `sw_rx`.
    pub sw_rx_ns: i64,
    /// ≠ 0: the runtime requires this message's `L2Batch.prev_id` to be this value.
    pub prev_override: u64,
    /// Connection that delivered the message; copied into `MarketHdr.conn`.
    pub conn: ConnId,
    /// Account of reports and account queries; market messages do not read it.
    pub account: AccountId,
    pub _pad: [u8; 2],
    /// hft-net `Timestamps.flags` (`TsFlags`) unchanged.
    pub ts_flags: u32,
    /// hft-net `Timestamps.rx_empty_gap_ns` unchanged; copied into `MarketHdr.rx_empty_gap_ns`.
    pub rx_empty_gap_ns: u32,
    pub _pad2: u32,
}
const _: () = assert!(size_of::<RxMeta>() == 48 && align_of::<RxMeta>() == 8);

impl Default for RxMeta {
    /// All zero: no time points, connection 0.
    fn default() -> Self {
        RxMeta {
            rx_tsc: 0,
            hw_rx_ns: 0,
            sw_rx_ns: 0,
            prev_override: 0,
            conn: ConnId(0),
            account: AccountId(0),
            _pad: [0; 2],
            ts_flags: 0,
            rx_empty_gap_ns: 0,
            _pad2: 0,
        }
    }
}

/// Result of one `decode_md` call.
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub struct MdOutcome {
    /// Events written to the sink.
    pub events: u16,
    pub inst: InstId,
}

/// Protocol decode error (not `hft_event::DecodeErr`).
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub enum DecodeErr {
    /// The message ended before a required field.
    Truncated,
    /// Structure or field type mismatch.
    Shape,
    /// Number parse failure (contract 4).
    Number,
    Overflow,
    /// Called with a class this method does not handle.
    UnknownClass,
}

/// Encode error of the order and request builders.
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub enum EncodeErr {
    /// Output buffer or a field too small.
    FieldOverflow,
    /// The inst does not belong to this venue.
    UnknownInst,
    /// This venue needs the exchange id and `TargetRef.exch_order_id = 0`.
    UnknownTarget,
    Unsupported(GwOp),
}

/// Session, subscription and request builder error.
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub enum ProtoErr {
    /// Output buffer too small.
    BufFull,
    /// This venue does not support the role, route or request.
    Unsupported,
    /// `VenueCfg` lacks a required item.
    Cfg,
}

// ---------------------------------------------------------------- routes and streams

/// Kind of a market-data route; BBO, TopN depth and the other streams use different connections.
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub enum RouteKind {
    /// bookTicker-like; carried by feed-bbo, events to `bbo-<venue>`; may race two connections.
    Bbo,
    /// TopN snapshot stream (contract 26); carried by feed-md.
    DepthTopN,
    /// Trades, mark/index price, funding, public liquidations; carried by feed-md.
    Md,
}

/// One stream of an inst.
#[repr(u8)]
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub enum StreamKind {
    Bbo = 1,
    DepthTopN = 2,
    /// Incremental depth (optional extension).
    DepthIncr = 3,
    Trade = 4,
    Mark = 5,
    Index = 6,
    Funding = 7,
    Liquidation = 8,
}

/// One market-data route; `max_streams`/`max_in_msgs_per_s` are protocol defaults (the effective
/// values are refdata `VenueMeta`); `topn_levels` ascending (DepthTopN only, else empty).
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub struct RouteSpec {
    pub id: RouteId,
    pub kind: RouteKind,
    pub name: &'static str,
    /// Only Bbo routes may be raced.
    pub race: bool,
    pub max_streams: u16,
    pub max_in_msgs_per_s: u16,
    pub topn_levels: &'static [u8],
}

/// One stream to (un)subscribe.
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub struct SubSpec {
    pub inst: InstId,
    pub stream: StreamKind,
    /// DepthTopN levels; 0 otherwise.
    pub levels: u8,
    pub route: RouteId,
}

/// Normalized sequence ids (same meaning as `L2Batch.prev_id/last_id`).
#[repr(C)]
#[derive(Copy, Clone, Eq, PartialEq, Debug, Default)]
pub struct SeqIds {
    pub prev_id: u64,
    pub last_id: u64,
}
const _: () = assert!(size_of::<SeqIds>() == 16);

/// Raw exchange sequence ids (`align_first` only).
#[repr(C)]
#[derive(Copy, Clone, Eq, PartialEq, Debug, Default)]
pub struct RawSeq {
    pub first: u64,
    pub last: u64,
    pub prev: u64,
    pub has_prev: bool,
    pub _pad: [u8; 7],
}
const _: () = assert!(size_of::<RawSeq>() == 32);

/// `peek` result: inst (0 = not a symbol of this venue), stream and normalized ids. For `ByTime`
/// streams `ids.last_id` carries the exchange time (ms, or the venue's unit; only compared).
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub struct Peek {
    pub inst: InstId,
    pub stream: StreamKind,
    pub ids: SeqIds,
    pub raw: RawSeq,
}

/// Dedup and gap rule of a class.
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub enum SeqRule {
    /// No sequence: forward in arrival order.
    None,
    /// Monotonic id: forward when `last_id` > forwarded.
    Monotonic,
    /// No id, monotonic exchange time.
    ByTime,
    /// Incremental depth (optional extension): continuous ⇔ `prev_id == last forwarded`.
    Chain,
    /// Every message a full snapshot: Monotonic dedup, no gaps.
    Snapshot,
}

/// First increment after a snapshot (incremental depth).
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub enum Align {
    Drop,
    Apply,
    Gap,
}

#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub enum ResyncMethod {
    RestSnapshot,
    Resubscribe,
    None,
}

#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub struct SnapshotInfo {
    pub inst: InstId,
    pub snap_id: u64,
    pub levels: u32,
}

// ---------------------------------------------------------------- heartbeat, expiry, dead man

#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub enum PingKind {
    WsPing,
    AppText,
    AppJson,
}

#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub struct HeartbeatSpec {
    pub ping: PingKind,
    pub interval_ms: u32,
    /// Send every interval even with inbound data.
    pub unconditional: bool,
    pub pong_timeout_ms: u32,
    pub idle_timeout_ms: u32,
}

/// Request expiry protection (contract 10).
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub enum StaleGuard {
    None,
    RecvWindow { default_ms: u32 },
    ExpireAt { default_ms: u32 },
}

#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub enum Via {
    Ws,
    Rest,
}

/// Dead-man switch (README comment: invariant `timeout_ms ≥ max(min_ms, 2 × refresh_ms)`).
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub enum DeadManSpec {
    None,
    CountdownPerInst {
        min_ms: u32,
        timeout_ms: u32,
        refresh_ms: u32,
        via: Via,
    },
    CountdownAccount {
        min_ms: u32,
        max_ms: u32,
        timeout_ms: u32,
        refresh_ms: u32,
        via: Via,
    },
    OnDisconnect {
        window_ms: u32,
    },
    Schedule {
        min_ahead_ms: u32,
        max_triggers_per_day: u16,
        timeout_ms: u32,
        refresh_ms: u32,
    },
}

/// Wall clock for frame fields, converted from the TSC by the gw (no system clock).
#[derive(Copy, Clone, Eq, PartialEq, Debug, Default)]
pub struct WallClock {
    pub unix_ms: i64,
    pub unix_us: i64,
}

/// Target of Cancel/Amend/Query, taken from the gw's slot mirror.
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub struct TargetRef {
    pub id: ClientOrderId,
    /// 0 = unknown.
    pub exch_order_id: u64,
}

// ---------------------------------------------------------------- configuration and credentials

#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub enum VenueEnv {
    Prod,
    Testnet,
}

/// Configuration given to the protocol (hosts and endpoints come from refdata, not here).
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub struct VenueCfg {
    pub env: VenueEnv,
    /// Expiry window of request fields = hft-venue `gw.stale_guard_ms`.
    pub stale_guard_ms: u32,
}

/// Fixed-capacity byte buffer (no heap).
#[derive(Copy, Clone, Eq, PartialEq)]
pub struct ArrayBuf<const N: usize> {
    len: usize,
    buf: [u8; N],
}

impl<const N: usize> Default for ArrayBuf<N> {
    fn default() -> Self {
        ArrayBuf { len: 0, buf: [0; N] }
    }
}

impl<const N: usize> core::fmt::Debug for ArrayBuf<N> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("ArrayBuf").field("len", &self.len).finish()
    }
}

impl<const N: usize> ArrayBuf<N> {
    pub const CAP: usize = N;

    pub fn new() -> Self {
        Self::default()
    }

    /// From bytes; longer than `N` → `None`.
    pub fn from_bytes(b: &[u8]) -> Option<Self> {
        let mut a = Self::default();
        a.push(b).ok().map(|_| a)
    }

    pub fn as_slice(&self) -> &[u8] {
        self.buf.get(..self.len).unwrap_or(&[])
    }

    pub fn len(&self) -> usize {
        self.len
    }

    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    pub fn clear(&mut self) {
        self.len = 0;
    }

    /// Appends; does not fit → `Err(BufFull)` and the buffer is unchanged.
    pub fn push(&mut self, b: &[u8]) -> Result<(), ProtoErr> {
        let end = self.len.checked_add(b.len()).ok_or(ProtoErr::BufFull)?;
        let dst = self.buf.get_mut(self.len..end).ok_or(ProtoErr::BufFull)?;
        dst.copy_from_slice(b);
        self.len = end;
        Ok(())
    }
}

/// Secret key material; `Debug` never prints it.
#[derive(Copy, Clone, Default, Eq, PartialEq)]
pub struct SecretBuf(pub ArrayBuf<256>);

impl core::fmt::Debug for SecretBuf {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str("SecretBuf(<redacted>)")
    }
}

#[derive(Copy, Clone, Eq, PartialEq, Debug, Default)]
pub enum SignAlg {
    #[default]
    HmacSha256,
    Ed25519,
    /// Polymarket: the secret holds the wallet key and the API secret (venue README).
    Secp256k1,
}

/// One API credential, borrowed by the venue (contract 7).
#[derive(Copy, Clone, Eq, PartialEq, Default)]
pub struct Credential {
    pub api_key: ArrayBuf<128>,
    pub secret: SecretBuf,
    /// OKX, Bitget; empty elsewhere.
    pub passphrase: ArrayBuf<64>,
    pub alg: SignAlg,
}

impl core::fmt::Debug for Credential {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("Credential").field("alg", &self.alg).finish()
    }
}

// ---------------------------------------------------------------- REST and session outputs

#[derive(Copy, Clone, Eq, PartialEq, Debug, Default)]
pub enum Method {
    #[default]
    Get,
    Post,
    Put,
    Delete,
}

/// REST request buffer (snapshots, BBO refresh, queries, dead man, account queries).
#[derive(Copy, Clone, Eq, PartialEq, Debug, Default)]
pub struct RestReqBuf {
    /// Index into the venue's REST endpoint table (0 = main endpoint).
    pub endpoint: u8,
    pub method: Method,
    pub path_query: ArrayBuf<1024>,
    pub headers: ArrayBuf<512>,
    /// 2048: a Polymarket order body with its ERC-7739 signature is ~1.4 KiB.
    pub body: ArrayBuf<2048>,
    /// Signed: the runtime records only metadata in `RawTx`.
    pub redacted: bool,
}

/// The REST request run before connecting (listenKey, bullet token).
pub type BootCall = RestReqBuf;

/// `endpoint` output: Upgrade request target and extra headers.
#[derive(Copy, Clone, Eq, PartialEq, Debug, Default)]
pub struct EndpointSpec {
    pub path_query: ArrayBuf<1024>,
    pub headers: ArrayBuf<512>,
}

/// What `login` produced.
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub enum LoginStep {
    /// No login on this role.
    NotNeeded,
    /// `out` holds one login frame; its answer goes to `login_result`.
    Frame,
    /// `out` holds the first frame of a challenge login: on `LoginResult::Continue` the driver calls
    /// `login` again with `step + 1`.
    Handshake,
}

/// Answer to a login frame.
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub enum LoginResult {
    Ok,
    Rejected(u32),
    Continue,
    /// Not a login answer (e.g. a pushed message); keep waiting.
    NotMine,
}

/// One subscription frame of `SubFrames`.
#[derive(Copy, Clone, Eq, PartialEq, Debug, Default)]
pub struct SubFrame {
    pub off: u32,
    pub len: u32,
    /// How many leading entries of the `subs` argument this frame covers.
    pub subs: u16,
}

/// Output of `subscribe`/`private_subscribe`: frames written back to back into one preallocated
/// buffer. Capacity is fixed at construction; a frame that does not fit → `ProtoErr::BufFull`.
#[derive(Clone, Debug)]
pub struct SubFrames {
    buf: Box<[u8]>,
    used: usize,
    frames: Box<[SubFrame]>,
    n: usize,
}

impl SubFrames {
    /// Startup: `bytes` of frame text, at most `frames` frames.
    pub fn with_capacity(bytes: usize, frames: usize) -> SubFrames {
        SubFrames {
            buf: vec![0u8; bytes].into_boxed_slice(),
            used: 0,
            frames: vec![SubFrame::default(); frames].into_boxed_slice(),
            n: 0,
        }
    }

    pub fn clear(&mut self) {
        self.used = 0;
        self.n = 0;
    }

    /// Appends one frame covering `subs` entries.
    pub fn push(&mut self, text: &[u8], subs: u16) -> Result<(), ProtoErr> {
        let end = self.used.checked_add(text.len()).ok_or(ProtoErr::BufFull)?;
        let slot = self.frames.get_mut(self.n).ok_or(ProtoErr::BufFull)?;
        let dst = self.buf.get_mut(self.used..end).ok_or(ProtoErr::BufFull)?;
        dst.copy_from_slice(text);
        *slot = SubFrame {
            off: self.used as u32,
            len: text.len() as u32,
            subs,
        };
        self.used = end;
        self.n += 1;
        Ok(())
    }

    pub fn len(&self) -> usize {
        self.n
    }

    pub fn is_empty(&self) -> bool {
        self.n == 0
    }

    /// Frame `i`: its text and how many subs it covers.
    pub fn get(&self, i: usize) -> Option<(&[u8], u16)> {
        let f = self.frames.get(..self.n)?.get(i)?;
        let t = self.buf.get(f.off as usize..(f.off + f.len) as usize)?;
        Some((t, f.subs))
    }
}

/// `dead_man_refresh` output: `Via::Ws` → `frame`; `Via::Rest` → one request per item of `reqs`
/// (capacity reserved at startup, never grown on the hot path).
#[derive(Clone, Debug, Default)]
pub struct DeadManOut {
    pub frame: ArrayBuf<1024>,
    pub reqs: Vec<RestReqBuf>,
}

#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub enum OrderChannel {
    Ws,
    Rest,
}

#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub enum RestWhat {
    Order,
    DeadMan,
    Keepalive,
    /// `CANCEL_ALL_SWEEP` open-order list.
    Sweep,
}

/// Origin of a gw REST request, given back to `decode_rest`.
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub struct RestOrigin {
    pub what: RestWhat,
    /// `Order`: the `(slot, gen, op)` of the `GwCmd`.
    pub req: Option<ReqRef>,
    /// 0 = not for one inst.
    pub inst: InstId,
}

// ---------------------------------------------------------------- report outcomes

/// `(slot, gen, op)` decoded from a request id.
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub struct ReqRef {
    pub slot: u32,
    pub r#gen: u32,
    pub op: GwOp,
}

/// Account query to run now (no state change).
#[repr(u8)]
#[derive(Copy, Clone, Eq, PartialEq, Debug, Default)]
pub enum AcctHint {
    #[default]
    None = 0,
    MarginNow = 1,
    ConfigRecheck = 2,
}

/// User-stream messages producing no event (contract 20).
#[repr(u8)]
#[derive(Copy, Clone, Eq, PartialEq, Debug, Default)]
pub enum Skip {
    #[default]
    No = 0,
    Listed = 1,
    Unknown = 2,
}

/// Connection-level hints to the runtime (contract 30).
#[repr(u8)]
#[derive(Copy, Clone, Eq, PartialEq, Debug, Default)]
pub enum ReportCtl {
    #[default]
    None = 0,
    ClockSkew = 1,
    RateLimited = 2,
    UserStreamEnded = 3,
    ServerShutdown = 4,
}

/// Live rate-limit usage reported by a response.
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub struct RateObs {
    pub bucket: hft_refdata::LimitBucket,
    pub used: u32,
    pub limit: u32,
    pub window_ms: u32,
}

/// Official default of one rate limit (`window_ms > 0`, `limit > 0`).
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub struct RateRule {
    pub scope: hft_refdata::LimitScope,
    pub bucket: hft_refdata::LimitBucket,
    pub window_ms: u32,
    pub limit: u32,
}

/// Control frames of a market-data route.
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub enum ControlHint {
    /// Subscription answer; the runtime matches answers to its subscription frames in order.
    SubAck {
        ok: bool,
        code: u32,
    },
    ServerShutdown,
    SessionExpired,
    Maintenance,
    /// Application-level pong.
    Pong,
}

/// Result of `decode_report`/`decode_rest`.
#[derive(Copy, Clone, Eq, PartialEq, Debug, Default)]
pub struct ReportOutcome {
    pub req: Option<ReqRef>,
    pub rate: Option<RateObs>,
    /// 0 = no gateway stamp.
    pub gw_dwell_us: u32,
    pub events: u16,
    /// Response code is "outcome unknown" (incl. unregistered codes, contract 8).
    pub outcome_unknown: Option<ReqRef>,
    /// Raw code of `outcome_unknown` or a Reject (contract 8); 0 = none.
    pub raw_code: u32,
    pub skip: Skip,
    pub hint: AcctHint,
    pub fee_rule_deviation: bool,
    pub ctl: ReportCtl,
}
