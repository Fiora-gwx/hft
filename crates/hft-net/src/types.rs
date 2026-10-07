//! Value types of the public API (README "签名草案"): phases, timestamps, error bits, send results,
//! the TX-timestamp records and the epoll error type.

use hft_types::{ConnId, VenueId};

/// Connection phase. `Closing`/`Dead` connections neither read nor write; only `retire` remains.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum ConnPhase {
    Free = 0,
    TcpConnecting = 1,
    TlsHandshake = 2,
    WsUpgrade = 3,
    Open = 4,
    Closing = 5,
    Dead = 6,
}

/// `Timestamps::flags` bits.
#[derive(Copy, Clone, Debug)]
pub struct TsFlags;

impl TsFlags {
    /// `hw_rx_ns` holds a hardware (PHC) receive stamp.
    pub const HW: u32 = 1;
    /// `sw_rx_ns` holds a kernel software receive stamp.
    pub const SW: u32 = 2;
    /// The stamps may come from a later packet than the frame's first one (README §7).
    pub const MULTI_SKB: u32 = 4;
}

/// Receive stamps of one frame (README §7). 32 B, no implicit padding.
#[repr(C)]
#[derive(Copy, Clone, Default, Debug, Eq, PartialEq)]
pub struct Timestamps {
    /// `SO_TIMESTAMPING` ts[2] (PHC domain); 0 = not available.
    pub hw_rx_ns: i64,
    /// ts[0], kernel software stamp (`CLOCK_REALTIME`); 0 = not available.
    pub sw_rx_ns: i64,
    /// TSC read right after the `recvmsg` that completed the frame.
    pub rx_tsc: u64,
    /// `TsFlags` bits.
    pub flags: u32,
    /// Upper bound of the queueing wait before the frame's first read (README §7); 0 = no confirmation.
    pub rx_empty_gap_ns: u32,
}

const _: () = assert!(size_of::<Timestamps>() == 32);
const _: () = assert!(align_of::<Timestamps>() == 8);
const _: () = assert!(std::mem::offset_of!(Timestamps, rx_tsc) == 16);
const _: () = assert!(std::mem::offset_of!(Timestamps, rx_empty_gap_ns) == 28);

impl Timestamps {
    /// The event's `hw_rx_ns`: HW when present, else SW with `true` (the caller sets
    /// `RecordFlags::SW_TS`); `(0, false)` when neither is present.
    #[inline]
    pub fn best_rx(&self) -> (i64, bool) {
        if self.flags & TsFlags::HW != 0 {
            (self.hw_rx_ns, false)
        } else if self.flags & TsFlags::SW != 0 {
            (self.sw_rx_ns, true)
        } else {
            (0, false)
        }
    }
}

/// Data-frame opcode; control frames are handled inside `Conn` and never surface.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum Opcode {
    Text = 1,
    Binary = 2,
}

bitflags::bitflags! {
    /// Connection error/event bits, accumulated until `clear_err` and handled once per loop.
    #[derive(Copy, Clone, Default, Debug, Eq, PartialEq)]
    pub struct ErrBits: u32 {
        // Fatal (the connection must retire).
        const PEER_EOF     = 1 << 0;
        const RESET        = 1 << 1;
        const IO           = 1 << 2;
        const TLS_ALERT    = 1 << 3;
        const TLS_PROTO    = 1 << 4;
        const WS_PROTO     = 1 << 5;
        const WS_CLOSE     = 1 << 6;
        const RX_FULL      = 1 << 7;
        const TX_BACKLOG   = 1 << 8;
        const PING_TIMEOUT = 1 << 9;
        const IDLE_TIMEOUT = 1 << 10;
        const HTTP_PROTO   = 1 << 11;
        // Not fatal.
        const TX_PARTIAL   = 1 << 16;
        const TS_MISSING   = 1 << 17;
        const WRONG_QUEUE  = 1 << 18;
        const KEY_UPDATE   = 1 << 19;
        const ROTATE_DUE   = 1 << 20;
        const SERVER_HINT  = 1 << 21;
    }
}

impl ErrBits {
    /// The fatal bits: the low 16.
    pub const FATAL: ErrBits = ErrBits::from_bits_retain(0x0000_FFFF);
    /// Bits an upper layer may set through `Conn::mark`.
    pub const MARKABLE: ErrBits = ErrBits::PING_TIMEOUT
        .union(ErrBits::IDLE_TIMEOUT)
        .union(ErrBits::SERVER_HINT);
}

/// Result of a send that produced ciphertext.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub struct SendOk {
    /// TSC right after `sendmsg` returned.
    pub sent_tsc: u64,
    /// Ciphertext bytes still waiting in tx.
    pub queued: u32,
}

/// Send failure. Only `Io` means ciphertext was produced.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum SendErr {
    /// Not encrypted; safe to use the standby connection.
    NotOpen(ConnPhase),
    /// Not encrypted; the connection also carries `TX_BACKLOG`.
    TxBacklog,
    /// The frame was not sealed (caller error); not encrypted.
    NotSealed,
    /// The record layer refused (unreachable state); the connection carries `TLS_PROTO`.
    Encrypt,
    /// Encrypted: the ciphertext stays in tx, the connection retires; treat the order as `Unknown`.
    Io(i32),
}

/// OS error of the `ReadyPoll` calls.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum NetErr {
    Os(i32),
}

/// `(venue, conn)` key of the TX-timestamp records. 4 B.
#[repr(C)]
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub struct ConnKey {
    pub venue: VenueId,
    pub conn: ConnId,
    pub _pad: u16,
}

const _: () = assert!(size_of::<ConnKey>() == 4);

impl Default for ConnKey {
    fn default() -> Self {
        ConnKey {
            venue: VenueId(0),
            conn: ConnId(0),
            _pad: 0,
        }
    }
}

/// One TX timestamp drained from the error queue (optional extension). 24 B.
#[repr(C)]
#[derive(Copy, Clone, Debug, Default, Eq, PartialEq)]
pub struct TxStamp {
    pub key: ConnKey,
    pub tskey: u32,
    /// SCHED = 1, SW = 2.
    pub kind: u8,
    pub _pad: [u8; 7],
    pub ts_ns: i64,
}

const _: () = assert!(size_of::<TxStamp>() == 24);
const _: () = assert!(std::mem::offset_of!(TxStamp, ts_ns) == 16);

/// What the gw pushes after a send when TX timestamps are on (optional extension). 24 B.
#[repr(C)]
#[derive(Copy, Clone, Debug, Default, Eq, PartialEq)]
pub struct TxTag {
    pub key: ConnKey,
    pub tskey_end: u32,
    pub slot: u32,
    pub r#gen: u32,
    pub sent_tsc: u64,
}

const _: () = assert!(size_of::<TxTag>() == 24);
const _: () = assert!(std::mem::offset_of!(TxTag, sent_tsc) == 16);

/// Index into `NetConfig::endpoints`.
#[derive(Copy, Clone, Debug, Default, Eq, PartialEq)]
pub struct EndpointId(pub u8);

/// The negotiated TLS parameters (IANA codes).
#[derive(Copy, Clone, Debug, Default, Eq, PartialEq)]
pub struct TlsInfo {
    pub version: u16,
    pub suite: u16,
    pub group: u16,
}

/// 256-bit set of `ConnId`s.
#[derive(Copy, Clone, Debug, Default, Eq, PartialEq)]
pub struct ConnSet {
    pub bits: [u64; 4],
}

impl ConnSet {
    #[inline]
    pub fn clear(&mut self) {
        self.bits = [0; 4];
    }

    #[inline]
    pub fn insert(&mut self, id: ConnId) {
        let i = (id.0 >> 6) as usize & 3;
        if let Some(w) = self.bits.get_mut(i) {
            *w |= 1u64 << (id.0 & 63);
        }
    }

    #[inline]
    pub fn contains(&self, id: ConnId) -> bool {
        let i = (id.0 >> 6) as usize & 3;
        self.bits.get(i).is_some_and(|w| w & (1u64 << (id.0 & 63)) != 0)
    }

    #[inline]
    pub fn is_empty(&self) -> bool {
        self.bits == [0; 4]
    }

    /// Number of members.
    #[inline]
    pub fn len(&self) -> u32 {
        self.bits.iter().map(|w| w.count_ones()).sum()
    }

    /// Members in ascending order.
    pub fn iter(&self) -> impl Iterator<Item = ConnId> + '_ {
        (0u16..256).filter_map(move |i| {
            let id = ConnId(i as u8);
            self.contains(id).then_some(id)
        })
    }
}

/// WS Ping → Pong round trips (ns): count, last, min, max, sum and log2 buckets.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub struct RttHist {
    pub count: u64,
    pub last_ns: u64,
    pub min_ns: u64,
    pub max_ns: u64,
    pub sum_ns: u64,
    /// `buckets[k]` counts samples with `floor(log2(ns)) == k` (0 ns in bucket 0).
    pub buckets: [u32; 40],
}

impl Default for RttHist {
    fn default() -> Self {
        RttHist {
            count: 0,
            last_ns: 0,
            min_ns: 0,
            max_ns: 0,
            sum_ns: 0,
            buckets: [0; 40],
        }
    }
}

impl RttHist {
    pub(crate) fn record(&mut self, ns: u64) {
        if self.count == 0 || ns < self.min_ns {
            self.min_ns = ns;
        }
        if ns > self.max_ns {
            self.max_ns = ns;
        }
        self.count = self.count.saturating_add(1);
        self.sum_ns = self.sum_ns.saturating_add(ns);
        self.last_ns = ns;
        let k = (63u32.saturating_sub(ns.leading_zeros())).min(39) as usize;
        if let Some(b) = self.buckets.get_mut(k) {
            *b = b.saturating_add(1);
        }
    }
}

/// Per-connection counters.
#[derive(Copy, Clone, Debug, Default, Eq, PartialEq)]
pub struct ConnStats {
    /// Data frames returned and their payload bytes.
    pub frames: u64,
    pub bytes: u64,
    /// Messages assembled from fragments.
    pub reassembled: u64,
    /// `sendmsg` short writes or `EAGAIN`.
    pub short_writes: u64,
    /// `recvmsg` calls that returned data, and those with a HW stamp.
    pub ts_reads: u64,
    pub hw_ts_reads: u64,
    /// Returned frames carrying `MULTI_SKB`.
    pub multi_skb: u64,
    pub rtt: RttHist,
}
