//! raw journal appends (README §1 "每个行情任务是它那份 raw journal 的唯一写者", §2 step 5.5, §4):
//! `RawRx`/`RawTx` = `RawHdr` + payload, record header `tsc` = the frame's `rx_tsc` (or `sent_tsc`),
//! `HW_TS`/`SW_TS` from the frame's `ts_flags`. Rolls at frame boundaries when `needs_roll` asks;
//! `NextNotReady` keeps the current segment; an append error drops that raw record and counts
//! `raw_append_err` (rings and sends are unaffected).

use hft_event::{EventKind, RawHdr, RecordFlags};
use hft_journal::{AppendErr, JournalWriter, RollInfo, RollReason};
use hft_net::{ConnId, TsFlags};
use hft_rt::CalibPair;
use hft_types::{Tsc, VenueId};
use zerocopy::IntoBytes;

use crate::config::Ticks;

/// `RawHdr.what` codes (hft-event `RAW_WHAT`).
pub mod what {
    pub const WS_TEXT: u8 = 1;
    pub const WS_BINARY: u8 = 2;
    pub const REST_RESPONSE: u8 = 3;
    pub const PING: u8 = 4;
    pub const PONG: u8 = 5;
    pub const CONNECT: u8 = 6;
    pub const DISCONNECT: u8 = 7;
    pub const SUB_ACK: u8 = 8;
    pub const WS_CLOSE: u8 = 9;
}

/// Counters of one raw writer.
#[derive(Copy, Clone, Debug, Default, Eq, PartialEq)]
pub struct RawCounters {
    pub records: u64,
    /// `raw_append_err{journal, err}`: Full, TooLarge, Sealed, InBoot.
    pub append_err: [u64; 4],
    pub rolls: u64,
    pub roll_not_ready: u64,
}

/// One externally visible effect, in order (tests enable the trace to check "ring before raw" and
/// similar orderings; off in production).
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum Effect {
    /// A record went to (or was queued for) the bbo ring.
    Bbo,
    /// A record went to (or was queued for) the md ring.
    Md,
    /// A record went to (or was queued for) the rep ring.
    Rep,
    /// A frame was sent on a connection.
    Send(ConnId),
    /// A raw record with this `what` (RawRx) …
    RawRx(u8),
    /// … or a RawTx.
    RawTx,
}

/// The single writer of one raw journal.
#[derive(Debug)]
pub struct RawLog {
    w: JournalWriter,
    venue: VenueId,
    ticks: Ticks,
    /// Segment-header UTC of a size roll (the runtime's published line).
    utc: hft_rt::UtcClock,
    pub(crate) n: RawCounters,
    pub(crate) trace: Option<Vec<Effect>>,
}

impl RawLog {
    /// Shutdown after the owning runtime has stopped producing records.
    pub(crate) fn seal_and_close(self) -> Result<(), hft_journal::RollErr> {
        self.w.seal_and_close()
    }
    /// A raw writer for `venue` (the runtimes build their own; public for benchmarks and tools).
    pub fn open(w: JournalWriter, venue: VenueId, clock: hft_rt::UtcClock) -> RawLog {
        RawLog::new(w, venue, clock)
    }

    pub(crate) fn new(w: JournalWriter, venue: VenueId, utc: hft_rt::UtcClock) -> RawLog {
        RawLog {
            w,
            venue,
            ticks: Ticks::of(&utc),
            utc,
            n: RawCounters::default(),
            trace: None,
        }
    }

    #[inline]
    pub(crate) fn note(&mut self, e: Effect) {
        if let Some(t) = self.trace.as_mut() {
            t.push(e);
        }
    }

    pub fn counters(&self) -> RawCounters {
        self.n
    }

    /// Rolls if the writer asks to (raw journals only have event boundaries).
    fn maybe_roll(&mut self, tsc: u64) {
        if self.w.needs_roll(Tsc(tsc)).is_none() {
            return;
        }
        let utc = self.utc.utc_ns(Tsc(tsc));
        let info = RollInfo {
            utc_ns: utc,
            calib: CalibPair {
                tsc: Tsc(tsc),
                realtime_ns: utc,
                phc_ns: 0,
                err_ns: self.utc.line().err_ns,
            },
            tsc_hz_milli: self.ticks.hz_milli(),
            configs: &[],
        };
        match self.w.roll(RollReason::Size, &info) {
            Ok(b) => {
                let _ = b.finish();
                self.n.rolls += 1;
            }
            Err(_) => self.n.roll_not_ready += 1,
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn put(
        &mut self,
        kind: EventKind,
        conn: ConnId,
        what: u8,
        rflags: u8,
        stream: u32,
        hw_rx_ns: i64,
        ts_flags: u32,
        payload: &[u8],
        tsc: u64,
    ) {
        self.maybe_roll(tsc);
        let h = RawHdr {
            venue: self.venue,
            conn,
            what,
            rflags,
            stream,
            hw_rx_ns,
            payload_len: payload.len() as u32,
            _pad: 0,
        };
        let mut f = 0u16;
        if ts_flags & TsFlags::HW != 0 {
            f |= RecordFlags::HW_TS.0;
        }
        if ts_flags & TsFlags::SW != 0 {
            f |= RecordFlags::SW_TS.0;
        }
        let pieces: [&[u8]; 2] = [h.as_bytes(), payload];
        match self.w.append_var(kind, RecordFlags(f), Tsc(tsc), &pieces) {
            Ok(seq) => {
                self.n.records += 1;
                self.w.publish();
                hft_rt::latency_trace::stamp(
                    hft_rt::latency_trace::Stage::RawRecord,
                    [seq.0, u64::from(kind.0), tsc, u64::from(what)],
                );
            }
            Err(e) => {
                let i = match e {
                    AppendErr::Full => 0,
                    AppendErr::TooLarge => 1,
                    AppendErr::Sealed => 2,
                    AppendErr::InBoot => 3,
                };
                if let Some(c) = self.n.append_err.get_mut(i) {
                    *c += 1;
                }
            }
        }
    }

    /// `RawRx` of a received frame or control marker.
    #[allow(clippy::too_many_arguments)]
    pub fn rx(
        &mut self,
        conn: ConnId,
        what: u8,
        rflags: u8,
        stream: u32,
        hw_rx_ns: i64,
        ts_flags: u32,
        payload: &[u8],
        tsc: u64,
    ) {
        self.note(Effect::RawRx(what));
        self.put(
            EventKind::RAW_RX,
            conn,
            what,
            rflags,
            stream,
            hw_rx_ns,
            ts_flags,
            payload,
            tsc,
        );
    }

    /// `RawTx` of a sent frame (`rflags = REDACTED` → metadata only).
    pub fn tx(&mut self, conn: ConnId, what: u8, rflags: u8, payload: &[u8], tsc: u64) {
        self.note(Effect::RawTx);
        self.put(EventKind::RAW_TX, conn, what, rflags, 0, 0, 0, payload, tsc);
    }
}
