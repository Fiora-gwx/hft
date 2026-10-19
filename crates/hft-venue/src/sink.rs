//! The feed task's `MdSink` (README §2 step 5.4, §1 "环满"): collects the events of one frame and
//! publishes them only when the frame decoded completely — a frame over `FRAME_EVENTS_MAX` events
//! publishes nothing. `L2Snapshot` parts are cut to `feed.snapshot_part_levels` levels and written as
//! variable bodies (`L2SnapHdr` + levels) for the bulk ring; an incremental `L2Batch` is written as
//! one variable body (`L2BatchHdr` + `[Level; n_bids + n_asks]`, bids first). A batch whose level
//! calls do not match its header counts, or whose sides are out of order, drops the frame.

use hft_event::{
    Bbo, EventKind, FeedMsg, Funding, IndexPrice, L2BatchHdr, L2SnapHdr, L2SnapshotPart, Level, Liquidation,
    MarkPrice, Trade,
};
use hft_types::{Px, Qty, Side};
use hft_venue_proto::MdSink;
use zerocopy::IntoBytes;

use crate::FRAME_EVENTS_MAX;

/// Bytes of the bulk scratch (all parts of one frame).
const BULK_SCRATCH: usize = 1 << 20;

#[derive(Debug)]
pub(crate) struct FeedSink {
    pub evs: Box<[FeedMsg]>,
    pub n: usize,
    pub rx_flags: hft_event::RecordFlags,
    /// More events than allowed: the frame is dropped as a decode error.
    pub over: bool,
    /// Bulk bodies of `FeedMsg::Bulk` events, back to back.
    pub bulk: Box<[u8]>,
    pub bulk_used: usize,
    /// Events allowed for this frame (Bbo route 1, others `FRAME_EVENTS_MAX`).
    pub limit: usize,
    pub part_levels: usize,
    /// Incremental-depth batches written.
    pub l2_batches: u64,
    /// Open batch: bulk offset of its header, levels still expected per side.
    batch: Option<(usize, u16, u16)>,
}

impl FeedSink {
    pub fn new(part_levels: u16) -> FeedSink {
        FeedSink {
            evs: vec![
                FeedMsg::Bulk {
                    flags: hft_event::RecordFlags(0),
                    kind: EventKind::BBO,
                    len: 0
                };
                FRAME_EVENTS_MAX
            ]
            .into_boxed_slice(),
            n: 0,
            rx_flags: hft_event::RecordFlags(0),
            over: false,
            bulk: vec![0u8; BULK_SCRATCH].into_boxed_slice(),
            bulk_used: 0,
            limit: FRAME_EVENTS_MAX,
            part_levels: usize::from(part_levels.max(1)),
            l2_batches: 0,
            batch: None,
        }
    }

    pub fn begin(&mut self, limit: usize) {
        self.n = 0;
        self.rx_flags = hft_event::RecordFlags(0);
        self.over = false;
        self.bulk_used = 0;
        self.limit = limit.min(FRAME_EVENTS_MAX);
        self.batch = None;
    }

    fn put_bytes(&mut self, b: &[u8]) -> bool {
        let end = self.bulk_used + b.len();
        match self.bulk.get_mut(self.bulk_used..end) {
            Some(d) => {
                d.copy_from_slice(b);
                self.bulk_used = end;
                true
            }
            None => {
                self.over = true;
                false
            }
        }
    }

    fn push(&mut self, m: FeedMsg) {
        if self.n >= self.limit {
            self.over = true;
            return;
        }
        if let Some(s) = self.evs.get_mut(self.n) {
            *s = m;
            self.n += 1;
        } else {
            self.over = true;
        }
    }

    /// One snapshot part (≤ `part_levels` levels) as a bulk body.
    fn put_part(&mut self, hdr: &L2SnapHdr, bids: &[Level], asks: &[Level]) {
        let mut h = *hdr;
        h.n_bids = bids.len() as u16;
        h.n_asks = asks.len() as u16;
        let len = size_of::<L2SnapHdr>() + (bids.len() + asks.len()) * size_of::<Level>();
        let end = self.bulk_used + len;
        let Some(dst) = self.bulk.get_mut(self.bulk_used..end) else {
            self.over = true;
            return;
        };
        let mut o = 0;
        for piece in [h.as_bytes(), bids.as_bytes(), asks.as_bytes()] {
            if let Some(d) = dst.get_mut(o..o + piece.len()) {
                d.copy_from_slice(piece);
            }
            o += piece.len();
        }
        self.bulk_used = end;
        self.push(FeedMsg::Bulk {
            flags: self.rx_flags,
            kind: EventKind::L2_SNAPSHOT,
            len: len as u32,
        });
    }
}

impl MdSink for FeedSink {
    fn bbo(&mut self, e: &Bbo) {
        self.push(FeedMsg::Bbo(*e, self.rx_flags));
    }
    fn l2_begin(&mut self, h: &L2BatchHdr) {
        if self.batch.is_some() {
            self.over = true;
            return;
        }
        let at = self.bulk_used;
        if self.put_bytes(h.as_bytes()) {
            self.batch = Some((at, h.n_bids, h.n_asks));
        }
    }
    fn l2_level(&mut self, side: Side, px: Px, qty: Qty) {
        let Some((at, nb, na)) = self.batch else {
            self.over = true;
            return;
        };
        // Bids first: a bid after the first ask, or more levels than announced, is malformed.
        let next = match side {
            Side::Buy if nb > 0 => (at, nb - 1, na),
            Side::Sell if nb == 0 && na > 0 => (at, nb, na - 1),
            _ => {
                self.over = true;
                return;
            }
        };
        if self.put_bytes(Level { px, qty }.as_bytes()) {
            self.batch = Some(next);
        }
    }
    fn l2_end(&mut self) {
        match self.batch.take() {
            Some((at, 0, 0)) => {
                self.l2_batches += 1;
                self.push(FeedMsg::Bulk {
                    flags: self.rx_flags,
                    kind: EventKind::L2_BATCH,
                    len: (self.bulk_used - at) as u32,
                });
            }
            _ => self.over = true,
        }
    }
    fn l2_snapshot(&mut self, part: &L2SnapshotPart<'_>) {
        let k = self.part_levels;
        if part.bids.len() + part.asks.len() <= k {
            self.put_part(&part.hdr, part.bids, part.asks);
            return;
        }
        // Re-cut to `k` levels per part: bids first, then asks (one side may span parts).
        let (mut bi, mut ai) = (0usize, 0usize);
        while bi < part.bids.len() || ai < part.asks.len() {
            let nb = (part.bids.len() - bi).min(k);
            let na = (part.asks.len() - ai).min(k - nb);
            let b = part.bids.get(bi..bi + nb).unwrap_or(&[]);
            let a = part.asks.get(ai..ai + na).unwrap_or(&[]);
            self.put_part(&part.hdr, b, a);
            bi += nb;
            ai += na;
            if nb == 0 && na == 0 {
                break;
            }
        }
    }
    fn trade(&mut self, e: &Trade) {
        self.push(FeedMsg::Trade(*e, self.rx_flags));
    }
    fn mark(&mut self, e: &MarkPrice) {
        self.push(FeedMsg::MarkPrice(*e, self.rx_flags));
    }
    fn index(&mut self, e: &IndexPrice) {
        self.push(FeedMsg::IndexPrice(*e, self.rx_flags));
    }
    fn funding(&mut self, e: &Funding) {
        self.push(FeedMsg::Funding(*e, self.rx_flags));
    }
    fn liquidation(&mut self, e: &Liquidation) {
        self.push(FeedMsg::Liquidation(*e, self.rx_flags));
    }
}
