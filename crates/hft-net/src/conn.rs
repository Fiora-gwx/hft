//! `Conn`: one TCP + TLS (+ WS) connection — receive buffer with its read positions and read-stamp
//! table, `next_frame`, the send path and the control-frame queue (README §1, §2, §7).
//!
//! Receive buffer layout (one buffer, `rx[..cap]`):
//! `[consumed | plaintext ps..pe | consumed record bytes pe..cs | ciphertext cs..ce | free ce..cap]`.
//! Records are opened in place one at a time, only when the next frame needs them, and their
//! plaintext is moved down to `pe`, so a frame spanning records is contiguous.

use hft_rt::latency_trace::{self as lt, Context as TraceContext, Stage as TraceStage};

use std::cell::Cell;
use std::marker::PhantomData;

use hft_event::ConnRole;
use hft_rt::TscClock;
use hft_types::{ConnId, Tsc, VenueId};

use crate::config::ConnCfg;
use crate::sock::{Sock, SysCount, errno};
use crate::tls::{self, CT_ALERT, CT_APP, CT_HANDSHAKE, MAX_BODY, REC_HDR, Tls};
use crate::types::{ConnPhase, ConnStats, ErrBits, Opcode, SendErr, SendOk, Timestamps, TsFlags};
use crate::ws::{self, Head, MaskedFrame, OP_BINARY, OP_CLOSE, OP_CONT, OP_PING, OP_PONG, OP_TEXT};

/// Read-stamp table capacity (README §1 step 1).
const READS: usize = 16;
/// Plaintext segment table capacity (one segment per opened application record).
const SEGS: usize = 8;
/// Queued Pongs per connection.
const PONGS: usize = 4;
/// `send_text` scratch frame capacity.
pub const TEXT_CAP: usize = 8192;

/// One server frame. `payload` borrows the connection's plaintext (or the fragment scratch for a
/// reassembled message) until the next `next_frame` or `poll_rx` on the same connection.
#[derive(Debug)]
pub struct Frame<'a> {
    pub opcode: Opcode,
    pub payload: &'a [u8],
    pub ts: Timestamps,
    /// Assembled from fragments (one copy).
    pub reassembled: bool,
}

/// Stamps of one `recvmsg` that returned data.
#[derive(Copy, Clone, Debug, Default)]
struct ReadRec {
    /// Absolute ciphertext-stream offsets `[start, end)`.
    start: u64,
    end: u64,
    bytes: u32,
    merged: bool,
    hw: i64,
    sw: i64,
    flags: u32,
    gap: u32,
    confirmed: bool,
    /// `rx_tsc` of the (oldest) read; `rx_tsc` of the newest read in a merged entry.
    first_rx_tsc: u64,
    rx_tsc: u64,
}

/// The stamps a frame takes from the read holding its first byte.
#[derive(Copy, Clone, Debug, Default)]
struct Src {
    hw: i64,
    sw: i64,
    flags: u32,
    gap: u32,
    confirmed: bool,
    multi: bool,
    first_rx_tsc: u64,
}

/// Plaintext of one (or, merged, several consecutive) application records.
#[derive(Copy, Clone, Debug, Default)]
struct Seg {
    pstart: u64,
    pend: u64,
    first: Src,
    /// `rx_tsc` of the read holding the record's last byte.
    done_tsc: u64,
    decrypted_tsc: u64,
}

/// The connection. Owned by exactly one thread at a time (not `Sync`); moves between threads only
/// as an arena slot (`Handoff`/`RetireMsg`).
pub struct Conn {
    pub(crate) id: ConnId,
    pub(crate) venue: VenueId,
    pub(crate) role: ConnRole,
    pub(crate) phase: ConnPhase,
    pub(crate) sock: Sock,
    pub(crate) tls: Tls,
    pub(crate) clock: TscClock,
    pub(crate) cfg: ConnCfg,
    // ---- receive
    rx: Box<[u8]>,
    ps: usize,
    pe: usize,
    cs: usize,
    ce: usize,
    plain_abs: u64,
    cipher_abs: u64,
    reads: [ReadRec; READS],
    nreads: usize,
    segs: [Seg; SEGS],
    nsegs: usize,
    empty_tsc: Option<u64>,
    read_data_round: bool,
    frag: Box<[u8]>,
    frag_len: usize,
    frag_on: bool,
    frag_op: Opcode,
    frag_first: Src,
    pongs: [[u8; 125]; PONGS],
    pong_len: [u8; PONGS],
    npongs: usize,
    close_echo: Option<(u8, [u8; 2])>,
    ping_tsc: Option<u64>,
    pub(crate) last_rx_tsc: u64,
    last_pong_tsc: u64,
    pub(crate) opened_tsc: u64,
    pub(crate) napi_id: u32,
    pub(crate) mss: u32,
    last_errno: i32,
    err: ErrBits,
    // ---- send
    tx: Box<[u8]>,
    ts: usize,
    te: usize,
    trace_tx_instance: u64,
    trace_tx_consumed: u64,
    mask: u64,
    scratch: Option<Box<MaskedFrame<TEXT_CAP>>>,
    stats: ConnStats,
    handoff_data: Vec<u8>,
    _not_sync: PhantomData<Cell<()>>,
}

impl std::fmt::Debug for Conn {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Conn")
            .field("id", &self.id)
            .field("venue", &self.venue)
            .field("role", &self.role)
            .field("phase", &self.phase)
            .field("err", &self.err)
            .field("tls", &self.tls)
            .finish()
    }
}

#[inline]
fn fatal_errno_bit(e: i32) -> ErrBits {
    match e {
        errno::ECONNRESET | errno::EPIPE | errno::ETIMEDOUT => ErrBits::RESET,
        _ => ErrBits::IO,
    }
}

/// Whether a fatal bit moves the phase to `Dead` (transport failures). `TX_BACKLOG` and the
/// upper-layer marks only flag the connection for retirement.
const DEAD_BITS: ErrBits = ErrBits::PEER_EOF
    .union(ErrBits::RESET)
    .union(ErrBits::IO)
    .union(ErrBits::TLS_ALERT)
    .union(ErrBits::TLS_PROTO)
    .union(ErrBits::WS_PROTO)
    .union(ErrBits::RX_FULL)
    .union(ErrBits::HTTP_PROTO);

enum Dec {
    Record,
    NoRecord,
    Fatal,
}

impl Conn {
    /// A free connection with buffers for `cfg` (startup, warm thread).
    pub(crate) fn new_free(cfg: ConnCfg, role: ConnRole, clock: TscClock) -> Conn {
        let rx = prefault(vec![0u8; cfg.rx_buf as usize].into_boxed_slice());
        let tx = prefault(vec![0u8; cfg.tx_buf as usize].into_boxed_slice());
        let frag = prefault(vec![0u8; cfg.max_msg as usize].into_boxed_slice());
        Conn {
            id: ConnId(0),
            venue: VenueId(0),
            role,
            phase: ConnPhase::Free,
            sock: Sock::default(),
            tls: Tls::None,
            clock,
            cfg,
            rx,
            ps: 0,
            pe: 0,
            cs: 0,
            ce: 0,
            plain_abs: 0,
            cipher_abs: 0,
            reads: [ReadRec::default(); READS],
            nreads: 0,
            segs: [Seg::default(); SEGS],
            nsegs: 0,
            empty_tsc: None,
            read_data_round: false,
            frag,
            frag_len: 0,
            frag_on: false,
            frag_op: Opcode::Text,
            frag_first: Src::default(),
            pongs: [[0; 125]; PONGS],
            pong_len: [0; PONGS],
            npongs: 0,
            close_echo: None,
            ping_tsc: None,
            last_rx_tsc: 0,
            last_pong_tsc: 0,
            opened_tsc: 0,
            napi_id: 0,
            mss: 1448,
            last_errno: 0,
            err: ErrBits::empty(),
            tx,
            ts: 0,
            te: 0,
            trace_tx_instance: 0,
            trace_tx_consumed: 0,
            mask: 0x9E37_79B9_7F4A_7C15,
            scratch: Some(Box::new(MaskedFrame::new())),
            stats: ConnStats::default(),
            handoff_data: Vec::new(),
            _not_sync: PhantomData,
        }
    }

    /// Clears every per-connection state for a new build (warm thread; buffers are kept).
    pub(crate) fn reset(&mut self, id: ConnId, venue: VenueId, mask_seed: u64) {
        self.id = id;
        self.venue = venue;
        self.phase = ConnPhase::TcpConnecting;
        self.sock = Sock::default();
        self.tls = Tls::None;
        self.ps = 0;
        self.pe = 0;
        self.cs = 0;
        self.ce = 0;
        self.plain_abs = 0;
        self.cipher_abs = 0;
        self.nreads = 0;
        self.nsegs = 0;
        self.empty_tsc = None;
        self.read_data_round = false;
        self.frag_len = 0;
        self.frag_on = false;
        self.npongs = 0;
        self.close_echo = None;
        self.ping_tsc = None;
        self.last_rx_tsc = 0;
        self.last_pong_tsc = 0;
        self.opened_tsc = 0;
        self.napi_id = 0;
        self.mss = 1448;
        self.last_errno = 0;
        self.err = ErrBits::empty();
        self.ts = 0;
        self.te = 0;
        self.mask = if mask_seed == 0 {
            0x9E37_79B9_7F4A_7C15
        } else {
            mask_seed
        };
        if let Some(f) = self.scratch.as_deref_mut() {
            f.unseal();
        }
        self.stats = ConnStats::default();
        self.handoff_data.fill(0);
        self.handoff_data.clear();
    }

    /// Warm-thread preparation only. Opaque, non-secret protocol metadata travels with the arena
    /// slot; the hot owner borrows it without allocating. Reset erases it before slot reuse.
    pub fn set_handoff_data(&mut self, bytes: &[u8]) -> bool {
        if bytes.len() > 16_384 {
            return false;
        }
        self.handoff_data.clear();
        self.handoff_data.extend_from_slice(bytes);
        true
    }
    pub fn handoff_data(&self) -> &[u8] {
        &self.handoff_data
    }

    /// The hot thread took the connection over (`ConnPool::install`): queue-empty confirmations
    /// made on the warm thread do not carry over; tickets are only counted from now on.
    pub(crate) fn on_install(&mut self) {
        self.empty_tsc = None;
        if let Tls::Ready(rl) = &mut self.tls {
            rl.store_tickets = false;
        }
    }

    /// `ConnPool::rotate_due` marks the connection.
    pub(crate) fn set_rotate_due(&mut self) {
        self.err |= ErrBits::ROTATE_DUE;
    }

    #[inline]
    fn set_err(&mut self, bits: ErrBits) {
        self.err |= bits;
        if bits.intersects(DEAD_BITS) && self.phase != ConnPhase::Free {
            self.phase = ConnPhase::Dead;
        }
    }

    #[inline]
    fn fatal_errno(&mut self, e: i32) {
        self.last_errno = e;
        self.set_err(fatal_errno_bit(e));
    }

    #[inline]
    fn cap(&self) -> usize {
        (self.cfg.rx_buf as usize).min(self.rx.len())
    }

    #[inline]
    fn tx_cap(&self) -> usize {
        (self.cfg.tx_buf as usize).min(self.tx.len())
    }

    #[inline]
    fn delta(&self, from: u64, to: u64) -> u64 {
        self.clock.delta_ns(Tsc(from), Tsc(to))
    }

    #[inline]
    fn confirm_empty(&mut self, t: u64) {
        self.empty_tsc = Some(match self.empty_tsc {
            Some(e) if e > t => e,
            _ => t,
        });
    }

    // ------------------------------------------------------------------ receive

    /// Non-blocking read (README §1 step 1): compaction, then at most one `recvmsg` over all free
    /// space; a short read or `EAGAIN` confirms the queue empty. No system call when the phase is
    /// not `Open` or there is no free space. Returns the bytes read.
    #[inline]
    pub fn poll_rx(&mut self, _now_tsc: u64) -> u32 {
        if self.phase != ConnPhase::Open {
            return 0;
        }
        self.recv_once()
    }

    /// Drain buffered work before receiving again. A partial WS frame or TLS record still
    /// needs a read; only a complete frame/record (or a detectable framing error) defers it.
    /// Like `poll_rx`, this never decrypts and returns only newly received bytes.
    #[inline]
    pub fn poll_rx_if_needed(&mut self, _now_tsc: u64) -> u32 {
        if self.phase != ConnPhase::Open {
            return 0;
        }
        let plain = &self.rx[self.ps..self.pe];
        match ws::parse_head(plain) {
            Head::Proto => return 0,
            Head::Ok(h) if h.len <= plain.len().saturating_sub(h.hdr_len) as u64 => return 0,
            _ => {}
        }
        let avail = self.ce - self.cs;
        if avail >= REC_HDR {
            let body = u16::from_be_bytes([self.rx[self.cs + 3], self.rx[self.cs + 4]]) as usize;
            if body > MAX_BODY || REC_HDR + body <= avail {
                return 0;
            }
        }
        self.recv_once()
    }

    /// `poll_rx` without the phase check (handshake, Upgrade and ticket wait on the warm thread).
    pub(crate) fn recv_once(&mut self) -> u32 {
        self.compact();
        let cap = self.cap();
        let free = cap.saturating_sub(self.ce);
        if free == 0 {
            return 0;
        }
        let ce = self.ce;
        let Some(dst) = self.rx.get_mut(ce..cap) else {
            return 0;
        };
        let r = self.sock.recv(dst);
        if r.ret > 0 {
            lt::set_context(TraceContext {
                venue_conn: u64::from(self.venue.0) | (u64::from(self.id.0) << 8),
                ..TraceContext::default()
            });
            lt::at(
                TraceStage::Read,
                r.post,
                [
                    r.stamp.hw_rx_ns as u64,
                    r.stamp.sw_rx_ns as u64,
                    r.ret as u64,
                    u64::from(r.stamp.flags),
                ],
            );
            let n = r.ret as usize;
            let (gap, confirmed) = match self.empty_tsc {
                Some(e) => (self.delta(e, r.post).min(u32::MAX as u64) as u32, true),
                None => (0, false),
            };
            let start = self.cipher_abs + (self.ce - self.cs) as u64;
            self.push_read(ReadRec {
                start,
                end: start + n as u64,
                bytes: n as u32,
                merged: false,
                hw: r.stamp.hw_rx_ns,
                sw: r.stamp.sw_rx_ns,
                flags: r.stamp.flags,
                gap,
                confirmed,
                first_rx_tsc: r.post,
                rx_tsc: r.post,
            });
            self.stats.ts_reads += 1;
            if r.stamp.flags & TsFlags::HW != 0 {
                self.stats.hw_ts_reads += 1;
            } else if self.cfg.ts_expect_hw {
                self.err |= ErrBits::TS_MISSING;
            }
            if n < free {
                self.confirm_empty(r.pre);
            }
            self.ce += n;
            self.read_data_round = true;
            n as u32
        } else if r.ret == 0 {
            self.set_err(ErrBits::PEER_EOF);
            0
        } else {
            let e = (-r.ret) as i32;
            if e == errno::EAGAIN {
                self.confirm_empty(r.pre);
            } else {
                self.fatal_errno(e);
            }
            0
        }
    }

    /// Moves unconsumed plaintext and ciphertext to the buffer head.
    #[inline]
    fn compact(&mut self) {
        let plen = self.pe - self.ps;
        let clen = self.ce - self.cs;
        if self.ps == 0 && self.pe == self.cs {
            return;
        }
        if plen == 0 && clen == 0 {
            self.ps = 0;
            self.pe = 0;
            self.cs = 0;
            self.ce = 0;
            return;
        }
        if plen > 0 {
            self.rx.copy_within(self.ps..self.pe, 0);
        }
        if clen > 0 {
            self.rx.copy_within(self.cs..self.ce, plen);
        }
        self.ps = 0;
        self.pe = plen;
        self.cs = plen;
        self.ce = plen + clen;
    }

    fn push_read(&mut self, r: ReadRec) {
        if self.nreads == READS {
            // Merge the two oldest: keep the earlier stamps, mark MULTI_SKB.
            let a = self.reads[0];
            let b = self.reads[1];
            self.reads[0] = ReadRec {
                start: a.start,
                end: b.end,
                bytes: a.bytes.saturating_add(b.bytes),
                merged: true,
                hw: a.hw,
                sw: a.sw,
                flags: a.flags,
                gap: a.gap,
                confirmed: a.confirmed,
                first_rx_tsc: a.first_rx_tsc,
                rx_tsc: b.rx_tsc,
            };
            self.reads.copy_within(2..READS, 1);
            self.nreads -= 1;
        }
        if let Some(s) = self.reads.get_mut(self.nreads) {
            *s = r;
            self.nreads += 1;
        }
    }

    /// The read holding absolute ciphertext offset `off`.
    #[inline]
    fn read_at(&self, off: u64) -> ReadRec {
        self.reads
            .iter()
            .take(self.nreads)
            .find(|r| r.start <= off && off < r.end)
            .copied()
            .unwrap_or_default()
    }

    fn retire_reads(&mut self) {
        let mut k = 0;
        while k < self.nreads && self.reads[k].end <= self.cipher_abs {
            k += 1;
        }
        if k > 0 {
            self.reads.copy_within(k..READS, 0);
            self.nreads -= k;
        }
    }

    fn push_seg(&mut self, s: Seg) {
        if self.nsegs == SEGS {
            // Frames start only in the first or the newest segment: merge the second and third.
            let a = self.segs[1];
            let b = self.segs[2];
            self.segs[1] = Seg {
                pstart: a.pstart,
                pend: b.pend,
                first: a.first,
                done_tsc: b.done_tsc,
                decrypted_tsc: b.decrypted_tsc,
            };
            self.segs.copy_within(3..SEGS, 2);
            self.nsegs -= 1;
        }
        if let Some(d) = self.segs.get_mut(self.nsegs) {
            *d = s;
            self.nsegs += 1;
        }
    }

    fn retire_segs(&mut self) {
        let mut k = 0;
        while k < self.nsegs && self.segs[k].pend <= self.plain_abs {
            k += 1;
        }
        if k > 0 {
            self.segs.copy_within(k..SEGS, 0);
            self.nsegs -= k;
        }
    }

    #[inline]
    fn seg_at(&self, off: u64) -> Seg {
        self.segs
            .iter()
            .take(self.nsegs)
            .find(|s| s.pstart <= off && off < s.pend)
            .copied()
            .unwrap_or_default()
    }

    #[inline]
    fn src_of(&self, r: &ReadRec) -> Src {
        Src {
            hw: r.hw,
            sw: r.sw,
            flags: r.flags,
            gap: r.gap,
            confirmed: r.confirmed,
            multi: r.merged || r.bytes > self.mss,
            first_rx_tsc: r.first_rx_tsc,
        }
    }

    /// Stamps of a frame whose first byte came from `first` and whose last record completed at `done_tsc`.
    #[inline]
    fn compose(&self, first: Src, done_tsc: u64) -> Timestamps {
        let gap = if first.confirmed {
            (first.gap as u64)
                .saturating_add(self.delta(first.first_rx_tsc, done_tsc))
                .min(u32::MAX as u64) as u32
        } else {
            0
        };
        let mut flags = first.flags & (TsFlags::HW | TsFlags::SW);
        if first.multi {
            flags |= TsFlags::MULTI_SKB;
        }
        Timestamps {
            hw_rx_ns: first.hw,
            sw_rx_ns: first.sw,
            rx_tsc: done_tsc,
            flags,
            rx_empty_gap_ns: gap,
        }
    }

    /// Opens the next complete record, if any.
    fn decrypt_one(&mut self) -> Dec {
        let avail = self.ce - self.cs;
        let cap = self.cap();
        let plen = self.pe - self.ps;
        if avail < REC_HDR {
            if plen + avail >= cap {
                self.set_err(ErrBits::RX_FULL);
                return Dec::Fatal;
            }
            return Dec::NoRecord;
        }
        let cs = self.cs;
        let body = u16::from_be_bytes([self.rx[cs + 3], self.rx[cs + 4]]) as usize;
        if body > MAX_BODY {
            self.set_err(ErrBits::TLS_PROTO);
            return Dec::Fatal;
        }
        let total = REC_HDR + body;
        if total > avail {
            if total > cap - plen {
                self.set_err(ErrBits::RX_FULL);
                return Dec::Fatal;
            }
            return Dec::NoRecord;
        }
        let rec_first = self.read_at(self.cipher_abs);
        let rec_last = self.read_at(self.cipher_abs + total as u64 - 1);
        lt::set_context(TraceContext {
            venue_conn: u64::from(self.venue.0) | (u64::from(self.id.0) << 8),
            ..TraceContext::default()
        });
        lt::stamp(
            TraceStage::DecryptStart,
            [rec_last.rx_tsc, self.cipher_abs, total as u64, 0],
        );
        let opened = match &mut self.tls {
            Tls::Ready(rl) => rl.open_record(&mut self.rx[cs..cs + total]),
            Tls::Handshaking(_) | Tls::None => {
                // A session still in its handshake on a hot thread (rustls BlockedHandshake).
                self.set_err(ErrBits::TLS_PROTO);
                return Dec::Fatal;
            }
        };
        let decrypted_tsc = if lt::enabled() { hft_rt::tsc_ordered().0 } else { 0 };
        lt::at(
            TraceStage::DecryptEnd,
            decrypted_tsc,
            [rec_last.rx_tsc, self.cipher_abs, total as u64, 0],
        );
        let Some(o) = opened else {
            self.set_err(ErrBits::TLS_PROTO);
            return Dec::Fatal;
        };
        match o.ctype {
            CT_APP => {
                if o.len > 0 {
                    let from = cs + o.off;
                    self.rx.copy_within(from..from + o.len, self.pe);
                    let pstart = self.plain_abs + (self.pe - self.ps) as u64;
                    let first = self.src_of(&rec_first);
                    self.push_seg(Seg {
                        pstart,
                        pend: pstart + o.len as u64,
                        first,
                        done_tsc: rec_last.rx_tsc,
                        decrypted_tsc,
                    });
                    self.pe += o.len;
                }
            }
            CT_HANDSHAKE => {
                let out = match &mut self.tls {
                    Tls::Ready(rl) => rl.handshake_msgs(&self.rx[cs + o.off..cs + o.off + o.len]),
                    _ => tls::HsOutcome::default(),
                };
                if out.proto {
                    self.set_err(ErrBits::TLS_PROTO);
                    return Dec::Fatal;
                }
                if out.key_update {
                    self.err |= ErrBits::KEY_UPDATE;
                }
                if out.reply_key_update && !self.queue_key_update() {
                    return Dec::Fatal;
                }
            }
            CT_ALERT => {
                self.set_err(ErrBits::TLS_ALERT);
                return Dec::Fatal;
            }
            _ => {
                self.set_err(ErrBits::TLS_PROTO);
                return Dec::Fatal;
            }
        }
        self.cs += total;
        self.cipher_abs += total as u64;
        self.retire_reads();
        Dec::Record
    }

    /// Our KeyUpdate goes into tx now and leaves with the next flush (`finish_poll`).
    fn queue_key_update(&mut self) -> bool {
        let need = match &self.tls {
            Tls::Ready(rl) => rl.key_update_len(),
            _ => return false,
        };
        let cap = self.tx_cap();
        if self.te - self.ts + need > cap {
            self.err |= ErrBits::TX_BACKLOG;
            return false;
        }
        if self.te + need > cap {
            self.compact_tx();
        }
        let te = self.te;
        let Tls::Ready(rl) = &mut self.tls else {
            return false;
        };
        match rl.encode_handshake_or_keyupdate(false, &mut self.tx[te..te + need]) {
            Some(n) => {
                self.trace_queued(n);
                self.te += n;
                true
            }
            None => {
                self.set_err(ErrBits::TLS_PROTO);
                false
            }
        }
    }

    /// Takes the next complete frame (README §1 steps 2–4); control frames are handled inside.
    /// No system call.
    pub fn next_frame(&mut self) -> Option<Frame<'_>> {
        if self.phase != ConnPhase::Open {
            return None;
        }
        // Bounded: each iteration consumes a frame or a record, or returns.
        loop {
            let head = ws::parse_head(&self.rx[self.ps..self.pe]);
            let h = match head {
                Head::Proto => {
                    self.set_err(ErrBits::WS_PROTO);
                    return None;
                }
                Head::NeedMore => match self.decrypt_one() {
                    Dec::Record => continue,
                    Dec::NoRecord | Dec::Fatal => return None,
                },
                Head::Ok(h) => h,
            };
            let data = matches!(h.opcode, OP_TEXT | OP_BINARY | OP_CONT);
            if data {
                let so_far = if h.opcode == OP_CONT {
                    self.frag_len as u64
                } else {
                    0
                };
                if so_far + h.len > self.cfg.max_msg as u64 {
                    self.set_err(ErrBits::RX_FULL);
                    return None;
                }
                if h.opcode == OP_CONT && !self.frag_on {
                    self.set_err(ErrBits::WS_PROTO);
                    return None;
                }
                if h.opcode != OP_CONT && self.frag_on {
                    self.set_err(ErrBits::WS_PROTO);
                    return None;
                }
            }
            let flen = h.hdr_len + h.len as usize;
            if self.pe - self.ps < flen {
                match self.decrypt_one() {
                    Dec::Record => continue,
                    Dec::NoRecord | Dec::Fatal => return None,
                }
            }
            // Complete frame at [ps, ps + flen).
            let a = self.plain_abs;
            let b = a + flen as u64;
            let first_seg = self.seg_at(a);
            let last_seg = self.seg_at(b - 1);
            let ts = self.compose(first_seg.first, last_seg.done_tsc);
            let p0 = self.ps + h.hdr_len;
            let p1 = self.ps + flen;
            self.ps = p1;
            self.plain_abs = b;
            self.retire_segs();
            self.last_rx_tsc = ts.rx_tsc;
            match h.opcode {
                OP_PING => {
                    self.queue_pong(p0, p1);
                }
                OP_PONG => {
                    self.last_pong_tsc = ts.rx_tsc;
                    if let Some(t) = self.ping_tsc
                        && self.rx[p0..p1] == t.to_le_bytes()
                    {
                        let d = self.delta(t, ts.rx_tsc);
                        self.stats.rtt.record(d);
                        self.ping_tsc = None;
                    }
                }
                OP_CLOSE => {
                    let n = p1 - p0;
                    if n == 1 {
                        self.set_err(ErrBits::WS_PROTO);
                        return None;
                    }
                    let mut code = [0u8; 2];
                    if n >= 2 {
                        code.copy_from_slice(&self.rx[p0..p0 + 2]);
                    }
                    self.close_echo = Some((n.min(2) as u8, code));
                    self.err |= ErrBits::WS_CLOSE;
                    self.phase = ConnPhase::Closing;
                    return None;
                }
                OP_TEXT | OP_BINARY => {
                    let op = if h.opcode == OP_TEXT {
                        Opcode::Text
                    } else {
                        Opcode::Binary
                    };
                    if h.fin {
                        return Some(self.deliver(op, p0, p1, ts, false, last_seg.decrypted_tsc));
                    }
                    self.frag_on = true;
                    self.frag_op = op;
                    self.frag_first = first_seg.first;
                    self.frag_len = 0;
                    self.append_frag(p0, p1);
                }
                _ => {
                    // OP_CONT
                    self.append_frag(p0, p1);
                    if h.fin {
                        self.frag_on = false;
                        let ts = self.compose(self.frag_first, last_seg.done_tsc);
                        self.stats.reassembled += 1;
                        return Some(self.deliver(
                            self.frag_op,
                            0,
                            self.frag_len,
                            ts,
                            true,
                            last_seg.decrypted_tsc,
                        ));
                    }
                }
            }
        }
    }

    fn append_frag(&mut self, p0: usize, p1: usize) {
        let n = p1 - p0;
        let at = self.frag_len;
        if let Some(d) = self.frag.get_mut(at..at + n) {
            d.copy_from_slice(&self.rx[p0..p1]);
            self.frag_len += n;
        }
    }

    #[inline]
    fn deliver(
        &mut self,
        opcode: Opcode,
        a: usize,
        b: usize,
        ts: Timestamps,
        reassembled: bool,
        decrypted_tsc: u64,
    ) -> Frame<'_> {
        lt::frame(
            self.venue.0,
            self.id.0,
            [ts.rx_tsc, ts.hw_rx_ns as u64, ts.sw_rx_ns as u64, decrypted_tsc],
        );
        self.stats.frames += 1;
        self.stats.bytes += (b - a) as u64;
        if ts.flags & TsFlags::MULTI_SKB != 0 {
            self.stats.multi_skb += 1;
        }
        let payload = if reassembled {
            &self.frag[a..b]
        } else {
            &self.rx[a..b]
        };
        Frame {
            opcode,
            payload,
            ts,
            reassembled,
        }
    }

    fn queue_pong(&mut self, p0: usize, p1: usize) {
        if self.npongs == PONGS {
            self.npongs = 0;
        }
        let n = p1 - p0;
        let k = self.npongs;
        self.pongs[k][..n].copy_from_slice(&self.rx[p0..p1]);
        self.pong_len[k] = n as u8;
        self.npongs += 1;
    }

    /// Sends queued Pong/Close frames and leftover ciphertext; `TCP_QUICKACK` after a round that
    /// read data when configured. Call once at the end of each loop.
    pub fn finish_poll(&mut self, _now_tsc: u64) {
        if matches!(self.phase, ConnPhase::Open | ConnPhase::Closing) {
            for i in 0..self.npongs {
                let n = self.pong_len[i] as usize;
                let mut payload = [0u8; 125];
                payload[..n].copy_from_slice(&self.pongs[i][..n]);
                self.seal_control(OP_PONG, &payload[..n]);
            }
            self.npongs = 0;
            let closing = self.phase == ConnPhase::Closing;
            if let Some((n, code)) = self.close_echo.take() {
                self.seal_control(OP_CLOSE, &code[..n as usize]);
            }
            if self.ts < self.te {
                let _ = self.flush_inner();
            }
            if closing && self.phase == ConnPhase::Closing {
                self.phase = ConnPhase::Dead;
            }
            if self.cfg.quickack && self.read_data_round && self.phase == ConnPhase::Open {
                let _ = self.sock.setsockopt_i32(
                    crate::sock::opt::IPPROTO_TCP,
                    crate::sock::opt::TCP_QUICKACK,
                    1,
                );
            }
        }
        self.read_data_round = false;
    }

    /// Masks and encrypts one control frame into tx (no system call).
    fn seal_control(&mut self, opcode: u8, payload: &[u8]) {
        let mut f = [0u8; 14 + 125];
        let n = payload.len().min(125);
        let key = self.next_mask().to_be_bytes();
        let end = 14 + n;
        f[14..end].copy_from_slice(&payload[..n]);
        ws::apply_mask(&mut f[14..end], key);
        let hl = ws::write_client_head(&mut f, 14, opcode, n, key);
        let _ = self.encrypt_into_tx(&f[14 - hl..end]);
    }

    /// The caller confirms the receive queue was empty at `tsc` (README §7).
    #[inline]
    pub fn note_empty(&mut self, tsc: u64) {
        self.confirm_empty(tsc);
    }

    /// Bits accumulated since the last `clear_err`.
    #[inline]
    pub fn err(&self) -> ErrBits {
        self.err
    }

    #[inline]
    pub fn clear_err(&mut self) {
        self.err = ErrBits::empty();
    }

    /// Upper-layer bits: only `PING_TIMEOUT`, `IDLE_TIMEOUT`, `SERVER_HINT`; others are a caller
    /// error (debug_assert) and ignored. The phase does not change.
    #[inline]
    pub fn mark(&mut self, bits: ErrBits) {
        debug_assert!(
            ErrBits::MARKABLE.contains(bits),
            "Conn::mark: only PING_TIMEOUT, IDLE_TIMEOUT, SERVER_HINT"
        );
        self.err |= bits & ErrBits::MARKABLE;
    }

    // ------------------------------------------------------------------ send

    /// Encrypts the sealed frame once (the real record sequence number moves here) and
    /// `sendmsg`s; `sent_tsc` is the TSC right after `sendmsg` returned. A short write leaves the
    /// rest in tx for `finish_poll`/`flush`; ciphertext is never dropped or re-encrypted.
    #[inline]
    pub fn send_frame<const C: usize>(&mut self, f: &MaskedFrame<C>) -> Result<SendOk, SendErr> {
        if !f.is_sealed() {
            return Err(SendErr::NotSealed);
        }
        if self.phase != ConnPhase::Open {
            return Err(SendErr::NotOpen(self.phase));
        }
        let start_left = self.te - self.ts;
        let old_end = self.encrypt_into_tx(f.as_bytes())?;
        self.send_tx(start_left > 0, old_end)
    }

    /// Subscriptions, application pings and other non-order text (internal scratch frame of
    /// `TEXT_CAP` bytes; a longer payload is a caller error: debug_assert, `NotSealed`).
    pub fn send_text(&mut self, payload: &[u8], _now_tsc: u64) -> Result<SendOk, SendErr> {
        self.send_scratch(OP_TEXT, payload)
    }

    /// WS Ping with the 8-byte little-endian `now_tsc` as payload; its Pong yields an RTT sample.
    pub fn send_ping(&mut self, now_tsc: u64) -> Result<SendOk, SendErr> {
        let r = self.send_scratch(OP_PING, &now_tsc.to_le_bytes());
        if r.is_ok() {
            self.ping_tsc = Some(now_tsc);
        }
        r
    }

    fn send_scratch(&mut self, opcode: u8, payload: &[u8]) -> Result<SendOk, SendErr> {
        if self.phase != ConnPhase::Open {
            return Err(SendErr::NotOpen(self.phase));
        }
        let max = TEXT_CAP - MaskedFrame::<TEXT_CAP>::HDR_MAX;
        debug_assert!(
            payload.len() <= max,
            "send_text payload exceeds the scratch frame"
        );
        if payload.len() > max {
            return Err(SendErr::NotSealed);
        }
        let Some(mut f) = self.scratch.take() else {
            return Err(SendErr::NotSealed);
        };
        let key = self.next_mask();
        f.unseal();
        f.body_mut()[..payload.len()].copy_from_slice(payload);
        f.seal_raw(opcode, payload.len(), key);
        let r = self.send_frame(&f);
        self.scratch = Some(f);
        r
    }

    /// Encrypts `data` at the end of tx. Returns where the new ciphertext starts.
    fn encrypt_into_tx(&mut self, data: &[u8]) -> Result<usize, SendErr> {
        let need = match &self.tls {
            Tls::Ready(rl) => rl.sealed_len(data.len()),
            _ => {
                self.set_err(ErrBits::TLS_PROTO);
                return Err(SendErr::Encrypt);
            }
        };
        let cap = self.tx_cap();
        if self.te - self.ts + need > cap {
            self.err |= ErrBits::TX_BACKLOG;
            return Err(SendErr::TxBacklog);
        }
        if self.sock.take_encrypt_failure() {
            self.set_err(ErrBits::TLS_PROTO);
            return Err(SendErr::Encrypt);
        }
        if self.te + need > cap {
            self.compact_tx();
        }
        let te = self.te;
        let Tls::Ready(rl) = &mut self.tls else {
            return Err(SendErr::Encrypt);
        };
        lt::stamp(
            TraceStage::EncryptStart,
            [
                u64::from(self.venue.0),
                u64::from(self.id.0),
                data.len() as u64,
                0,
            ],
        );
        let sealed = rl.seal_app_record(data, &mut self.tx[te..te + need]);
        lt::stamp(
            TraceStage::EncryptEnd,
            [
                u64::from(self.venue.0),
                u64::from(self.id.0),
                data.len() as u64,
                0,
            ],
        );
        match sealed {
            Some(n) => {
                self.trace_queued(n);
                self.te += n;
                Ok(te)
            }
            None => {
                self.set_err(ErrBits::TLS_PROTO);
                Err(SendErr::Encrypt)
            }
        }
    }

    fn compact_tx(&mut self) {
        if self.ts > 0 {
            self.tx.copy_within(self.ts..self.te, 0);
            self.te -= self.ts;
            self.ts = 0;
        }
    }

    /// `sendmsg` of tx: two iovecs (`[old remainder, new record]`) when a remainder was waiting.
    #[inline]
    fn send_tx(&mut self, had_left: bool, split: usize) -> Result<SendOk, SendErr> {
        let (ts, te) = (self.ts, self.te);
        let out = if had_left && split > ts && split < te {
            self.sock.send(&[&self.tx[ts..split], &self.tx[split..te]])
        } else {
            self.sock.send(&[&self.tx[ts..te]])
        };
        lt::at(
            TraceStage::Send,
            out.post,
            [
                u64::from(self.venue.0),
                u64::from(self.id.0),
                out.ret as u64,
                (te - ts) as u64,
            ],
        );
        self.trace_write(out.ret, out.post, te - ts);
        self.after_send(out.ret, out.post)
    }

    fn trace_queued(&mut self, n: usize) {
        if lt::enabled() {
            if self.trace_tx_instance == 0 {
                self.trace_tx_instance = hft_rt::tsc_ordered().0;
            }
            let mut context = lt::context();
            context.frame = self.trace_tx_instance;
            context.venue_conn = u64::from(self.venue.0) | (u64::from(self.id.0) << 8);
            let _scope = lt::scope(context);
            let start = self.trace_tx_consumed + (self.te - self.ts) as u64;
            lt::stamp(TraceStage::TxQueued, [start, start + n as u64, 0, 0]);
        }
    }

    fn trace_write(&mut self, ret: isize, post: u64, requested: usize) {
        if lt::enabled() {
            let _scope = lt::scope(TraceContext {
                frame: self.trace_tx_instance,
                venue_conn: u64::from(self.venue.0) | (u64::from(self.id.0) << 8),
                ..TraceContext::default()
            });
            lt::at(
                TraceStage::TxWrite,
                post,
                [self.trace_tx_consumed, ret as u64, requested as u64, 0],
            );
            if ret > 0 {
                self.trace_tx_consumed += ret as u64;
            }
        }
    }

    fn after_send(&mut self, ret: isize, post: u64) -> Result<SendOk, SendErr> {
        if ret >= 0 {
            self.ts += ret as usize;
            if self.ts >= self.te {
                self.ts = 0;
                self.te = 0;
            } else {
                self.err |= ErrBits::TX_PARTIAL;
                self.stats.short_writes += 1;
            }
            Ok(SendOk {
                sent_tsc: post,
                queued: (self.te - self.ts) as u32,
            })
        } else {
            let e = (-ret) as i32;
            if e == errno::EAGAIN {
                self.err |= ErrBits::TX_PARTIAL;
                self.stats.short_writes += 1;
                Ok(SendOk {
                    sent_tsc: post,
                    queued: (self.te - self.ts) as u32,
                })
            } else {
                self.fatal_errno(e);
                Err(SendErr::Io(e))
            }
        }
    }

    /// Writes the remaining ciphertext; returns the bytes still waiting.
    pub fn flush(&mut self) -> Result<u32, SendErr> {
        if !matches!(
            self.phase,
            ConnPhase::Open | ConnPhase::Closing | ConnPhase::WsUpgrade | ConnPhase::TlsHandshake
        ) {
            return Err(SendErr::NotOpen(self.phase));
        }
        if self.ts == self.te {
            return Ok(0);
        }
        self.flush_inner().map(|o| o.queued)
    }

    pub(crate) fn flush_inner(&mut self) -> Result<SendOk, SendErr> {
        let out = self.sock.send(&[&self.tx[self.ts..self.te]]);
        self.trace_write(out.ret, out.post, self.te - self.ts);
        self.after_send(out.ret, out.post)
    }

    /// Next value of this connection's xorshift64* mask-key sequence.
    #[inline]
    pub fn next_mask(&mut self) -> u32 {
        let mut x = self.mask;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.mask = x;
        (x.wrapping_mul(0x2545_F491_4F6C_DD1D) >> 32) as u32
    }

    /// Cache warm-up through a scratch TLS session (optional extension, off): a no-op that makes
    /// no system call and touches no state of this connection.
    #[inline]
    pub fn warm<const C: usize>(&mut self, _f: &MaskedFrame<C>) {}

    /// Scratch TLS sessions held for `warm`: 0 while the optional warm-up is off (no session is
    /// ever created).
    #[inline]
    pub fn scratch_sessions(&self) -> u32 {
        0
    }

    // ------------------------------------------------------------------ queries

    #[inline]
    pub fn id(&self) -> ConnId {
        self.id
    }

    #[inline]
    pub fn venue(&self) -> VenueId {
        self.venue
    }

    #[inline]
    pub fn role(&self) -> ConnRole {
        self.role
    }

    #[inline]
    pub fn phase(&self) -> ConnPhase {
        self.phase
    }

    #[inline]
    pub fn opened_tsc(&self) -> u64 {
        self.opened_tsc
    }

    /// `rx_tsc` of the last complete WS frame of any kind.
    #[inline]
    pub fn last_rx_tsc(&self) -> u64 {
        self.last_rx_tsc
    }

    /// `rx_tsc` of the last WS Pong (any payload); 0 = none since the connection opened.
    #[inline]
    pub fn last_pong_tsc(&self) -> u64 {
        self.last_pong_tsc
    }

    /// Received bytes not yet consumed: plaintext not taken by `next_frame` plus ciphertext not yet
    /// decrypted (no system call; `has_pending` of the upper layers).
    #[inline]
    pub fn rx_buffered(&self) -> usize {
        (self.pe - self.ps) + (self.ce - self.cs)
    }

    /// `SO_INCOMING_NAPI_ID` read once after the handshake; 0 = unavailable.
    #[inline]
    pub fn napi_id(&self) -> u32 {
        self.napi_id
    }

    /// errno of the last `IO`/`RESET`; 0 = none.
    #[inline]
    pub fn last_errno(&self) -> i32 {
        self.last_errno
    }

    #[inline]
    pub fn stats(&self) -> &ConnStats {
        &self.stats
    }

    /// System calls made through this connection.
    #[inline]
    pub fn sys(&self) -> &SysCount {
        &self.sock.sys
    }

    /// The socket fd (-1 for a scripted or closed connection).
    #[inline]
    pub fn raw_fd(&self) -> i32 {
        #[cfg(feature = "test-io")]
        if let Some(fd) = self.sock.script.as_ref().and_then(|s| s.wait_fd) {
            return fd;
        }
        self.sock.fd
    }

    /// `getsockname`: the local address and port.
    pub fn local_addr(&mut self) -> Result<(std::net::Ipv4Addr, u16), i32> {
        self.sock.getsockname()
    }

    /// Receive / send buffer capacities in effect.
    #[inline]
    pub fn buf_caps(&self) -> (u32, u32) {
        (self.cap() as u32, self.tx_cap() as u32)
    }

    /// Ciphertext bytes waiting in tx.
    #[inline]
    pub fn tx_pending(&self) -> usize {
        self.te - self.ts
    }

    /// Negotiated TLS parameters (zero before the handshake completed).
    pub fn tls_info(&self) -> crate::types::TlsInfo {
        match &self.tls {
            Tls::Ready(rl) => rl.info,
            _ => crate::types::TlsInfo::default(),
        }
    }

    // ------------------------------------------------------------------ warm-thread helpers

    /// Encrypts `data` as application data into tx and flushes (Upgrade request).
    pub(crate) fn send_plain(&mut self, data: &[u8]) -> Result<(), SendErr> {
        self.encrypt_into_tx(data)?;
        self.flush_inner().map(|_| ())
    }

    /// The unconsumed plaintext.
    pub(crate) fn plaintext(&self) -> &[u8] {
        &self.rx[self.ps..self.pe]
    }

    /// HTTP uses the same first-record / completion-read timestamp rule as WS.
    pub(crate) fn plaintext_stamps(&self) -> Timestamps {
        let first = self.seg_at(self.plain_abs);
        let last = self.seg_at(self.plain_abs + (self.pe - self.ps).saturating_sub(1) as u64);
        self.compose(first.first, last.done_tsc)
    }

    /// Consumes `n` plaintext bytes (the Upgrade response).
    pub(crate) fn consume_plain(&mut self, n: usize) {
        let n = n.min(self.pe - self.ps);
        self.ps += n;
        self.plain_abs += n as u64;
        self.retire_segs();
    }

    /// Opens one record if complete; `Some(true)` = one opened, `Some(false)` = none complete,
    /// `None` = fatal.
    pub(crate) fn open_one(&mut self) -> Option<bool> {
        match self.decrypt_one() {
            Dec::Record => Some(true),
            Dec::NoRecord => Some(false),
            Dec::Fatal => None,
        }
    }

    /// Whether unconsumed plaintext is waiting.
    pub(crate) fn has_plain(&self) -> bool {
        self.pe > self.ps
    }

    /// Tickets seen by the record layer.
    pub(crate) fn tickets(&self) -> u32 {
        match &self.tls {
            Tls::Ready(rl) => rl.tickets,
            _ => 0,
        }
    }

    /// One handshake step over the connection's own buffers, then transmits the encoded flight.
    pub(crate) fn handshake_io(&mut self) -> tls::HsStep {
        let te0 = self.te;
        let mut cs = self.cs;
        let mut te = self.te;
        let tx_cap = self.tx_cap();
        let step = tls::handshake_step(
            &mut self.tls,
            &mut self.rx[..],
            &mut cs,
            self.ce,
            &mut self.tx[..tx_cap],
            &mut te,
        );
        let consumed = cs - self.cs;
        self.cs = cs;
        self.cipher_abs += consumed as u64;
        self.retire_reads();
        self.te = te;
        if self.te > te0 || self.ts < self.te {
            match self.flush_inner() {
                Ok(_) => {}
                Err(_) => return tls::HsStep::Failed(tls::HsFail::Tls),
            }
        }
        step
    }
}

/// Writes every page of a fresh buffer once (README §10: pre-touch at startup).
fn prefault(mut b: Box<[u8]>) -> Box<[u8]> {
    let mut i = 0;
    while i < b.len() {
        b[i] = 0;
        i += 4096;
    }
    b
}

// ---------------------------------------------------------------------- test hooks

#[cfg(feature = "test-io")]
impl Conn {
    /// A scripted connection outside any arena (tests).
    #[doc(hidden)]
    pub fn standalone(
        cfg: ConnCfg,
        role: ConnRole,
        id: ConnId,
        clock: TscClock,
        script: crate::sock::ScriptIo,
    ) -> Box<Conn> {
        let mut c = Box::new(Conn::new_free(cfg, role, clock));
        c.reset(id, VenueId(0), 0x9E37_79B9_7F4A_7C15);
        c.sock.script = Some(Box::new(script));
        c.phase = ConnPhase::TlsHandshake;
        c
    }

    #[doc(hidden)]
    pub fn script(&mut self) -> Option<&mut crate::sock::ScriptIo> {
        self.sock.script.as_deref_mut()
    }

    #[doc(hidden)]
    pub fn test_set_phase(&mut self, p: ConnPhase) {
        self.phase = p;
        self.sock.phase = p as u8;
    }

    /// Address range of the receive buffer.
    #[doc(hidden)]
    pub fn test_rx_range(&self) -> (usize, usize) {
        let a = self.rx.as_ptr() as usize;
        (a, a + self.cap())
    }

    /// Records opened by the record layer.
    #[doc(hidden)]
    pub fn test_records_opened(&self) -> u64 {
        match &self.tls {
            Tls::Ready(rl) => rl.opened,
            _ => 0,
        }
    }

    /// Send-direction record sequence number.
    #[doc(hidden)]
    pub fn test_tx_seq(&self) -> Option<u64> {
        match &self.tls {
            Tls::Ready(rl) => Some(rl.tx_seq()),
            _ => None,
        }
    }

    /// The MSS used for `MULTI_SKB` (normally read with `TCP_MAXSEG` after the handshake).
    #[doc(hidden)]
    pub fn test_set_mss(&mut self, mss: u32) {
        self.mss = mss;
    }

    #[doc(hidden)]
    pub fn test_set_mask_seed(&mut self, seed: u64) {
        self.mask = seed;
    }

    /// Starts a client handshake over the script (tests drive the peer).
    #[doc(hidden)]
    pub fn test_start_tls(&mut self, cfg: &rustls::ClientConfig, sni: &str) -> bool {
        match tls::start(cfg, sni) {
            Some(t) => {
                self.tls = t;
                self.phase = ConnPhase::TlsHandshake;
                true
            }
            None => false,
        }
    }

    /// One handshake step: reads once, processes, transmits. `Some(Ok(info))` when complete.
    #[doc(hidden)]
    pub fn test_handshake_step(&mut self) -> Option<Result<crate::types::TlsInfo, ()>> {
        self.recv_once();
        match self.handshake_io() {
            tls::HsStep::NeedRead => None,
            tls::HsStep::Done(i) => Some(Ok(i)),
            tls::HsStep::Failed(_) => Some(Err(())),
        }
    }

    /// The hot thread's `install` effect without a pool.
    /// The current queue-empty confirmation (`None` = none since `install`).
    #[doc(hidden)]
    pub fn test_empty_tsc(&self) -> Option<u64> {
        self.empty_tsc
    }

    #[doc(hidden)]
    pub fn test_on_install(&mut self) {
        self.on_install();
    }

    /// Zeroes the counters and error bits (after a scripted handshake).
    #[doc(hidden)]
    pub fn test_reset_stats(&mut self) {
        self.stats = ConnStats::default();
        self.handoff_data.fill(0);
        self.handoff_data.clear();
        self.err = ErrBits::empty();
        self.sock.sys = SysCount::default();
        self.read_data_round = false;
    }
}
