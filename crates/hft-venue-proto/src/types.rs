//! Helper types of the trait signatures (README "辅助类型"); M1.5 part. Types only.

use hft_types::{AccountId, ConnId, InstId};

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
