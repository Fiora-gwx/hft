//! Test facilities for venue crates (feature `testkit`; README "实现说明"): `VecSink` and the
//! contract checks. M1.5 part: `VecSink` for `MdSink` and the contract-3 assertions of
//! `check_md_basics` (receive time points, `rx_empty_gap_ns` and `conn` copied into `MarketHdr`);
//! contracts 2 and 4 and the other checks arrive with M3.

use hft_event::{
    Bbo, Funding, IndexPrice, L2BatchHdr, L2SnapHdr, L2SnapshotPart, Level, Liquidation, MarkPrice,
    MarketHdr, Trade,
};
use hft_refdata::RefData;
use hft_types::{Px, Qty, Side};

use crate::{MdSink, MsgOp, RouteId, RxMeta, VenueProtocol};

/// One `MdSink` call, recorded with owned copies of its inputs.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MdRec {
    Bbo(Bbo),
    L2Begin(L2BatchHdr),
    L2Level(Side, Px, Qty),
    L2End,
    L2Snapshot {
        hdr: L2SnapHdr,
        bids: Vec<Level>,
        asks: Vec<Level>,
        part: u16,
        parts: u16,
    },
    Trade(Trade),
    Mark(MarkPrice),
    Index(IndexPrice),
    Funding(Funding),
    Liquidation(Liquidation),
}

impl MdRec {
    /// `MarketHdr` of the recorded event (`None` for `L2Level`/`L2End`).
    pub fn market_hdr(&self) -> Option<&MarketHdr> {
        match self {
            MdRec::Bbo(e) => Some(&e.h),
            MdRec::L2Begin(e) => Some(&e.h),
            MdRec::L2Snapshot { hdr, .. } => Some(&hdr.h),
            MdRec::Trade(e) => Some(&e.h),
            MdRec::Mark(e) => Some(&e.h),
            MdRec::Index(e) => Some(&e.h),
            MdRec::Funding(e) => Some(&e.h),
            MdRec::Liquidation(e) => Some(&e.h),
            MdRec::L2Level(..) | MdRec::L2End => None,
        }
    }
}

/// Records every sink call in order.
#[derive(Clone, Debug, Default)]
pub struct VecSink {
    pub md: Vec<MdRec>,
}

impl MdSink for VecSink {
    fn bbo(&mut self, e: &Bbo) {
        self.md.push(MdRec::Bbo(*e));
    }
    fn l2_begin(&mut self, h: &L2BatchHdr) {
        self.md.push(MdRec::L2Begin(*h));
    }
    fn l2_level(&mut self, side: Side, px: Px, qty: Qty) {
        self.md.push(MdRec::L2Level(side, px, qty));
    }
    fn l2_end(&mut self) {
        self.md.push(MdRec::L2End);
    }
    fn l2_snapshot(&mut self, part: &L2SnapshotPart<'_>) {
        self.md.push(MdRec::L2Snapshot {
            hdr: part.hdr,
            bids: part.bids.to_vec(),
            asks: part.asks.to_vec(),
            part: part.part,
            parts: part.parts,
        });
    }
    fn trade(&mut self, e: &Trade) {
        self.md.push(MdRec::Trade(*e));
    }
    fn mark(&mut self, e: &MarkPrice) {
        self.md.push(MdRec::Mark(*e));
    }
    fn index(&mut self, e: &IndexPrice) {
        self.md.push(MdRec::Index(*e));
    }
    fn funding(&mut self, e: &Funding) {
        self.md.push(MdRec::Funding(*e));
    }
    fn liquidation(&mut self, e: &Liquidation) {
        self.md.push(MdRec::Liquidation(*e));
    }
}

/// A contract check failure: `contract` = contract number (README "实现方必须遵守的契约").
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ContractFail {
    pub contract: u8,
    pub sample: String,
    pub detail: String,
}

/// One market-data sample supplied by the venue's `tests/contract.rs`.
#[derive(Clone, Debug)]
pub struct MdSample<'a> {
    pub name: &'a str,
    pub route: RouteId,
    pub op: MsgOp,
    pub payload: &'a [u8],
    pub rx: RxMeta,
}

/// Decodes each sample with a fresh `VecSink` and returns the recorded calls (decode errors are
/// contract failures of contract 3: every sample must decode).
pub fn decode_samples<P: VenueProtocol>(
    samples: &[MdSample<'_>],
    rd: &RefData,
    ctx: &mut P::FeedCtx,
) -> Result<Vec<Vec<MdRec>>, ContractFail> {
    let mut out = Vec::with_capacity(samples.len());
    for s in samples {
        let c = P::classify(s.route, s.op, s.payload.get(..64).unwrap_or(s.payload));
        let mut sink = VecSink::default();
        P::decode_md(rd, ctx, c, s.payload, &s.rx, &mut sink).map_err(|e| ContractFail {
            contract: 3,
            sample: s.name.to_string(),
            detail: format!("decode_md failed: {e:?}"),
        })?;
        out.push(sink.md);
    }
    Ok(out)
}

/// `check_md_basics`, contract-3 part: every event of every sample carries `RxMeta`'s `rx_tsc`,
/// `hw_rx_ns`, `sw_rx_ns`, `rx_empty_gap_ns` and `conn` unchanged in its `MarketHdr` (in particular
/// `hw_rx_ns = 0` stays 0: no `sw_rx_ns` substitution), and every sample yields at least one event.
pub fn check_md_basics<P: VenueProtocol>(
    samples: &[MdSample<'_>],
    rd: &RefData,
    ctx: &mut P::FeedCtx,
) -> Result<(), ContractFail> {
    let recs = decode_samples::<P>(samples, rd, ctx)?;
    for (s, recs) in samples.iter().zip(&recs) {
        let fail = |detail: String| ContractFail {
            contract: 3,
            sample: s.name.to_string(),
            detail,
        };
        let hdrs: Vec<&MarketHdr> = recs.iter().filter_map(MdRec::market_hdr).collect();
        if hdrs.is_empty() {
            return Err(fail("no market event".to_string()));
        }
        for h in hdrs {
            let got = (h.rx_tsc, h.hw_rx_ns, h.sw_rx_ns, h.rx_empty_gap_ns, h.conn);
            let want = (
                s.rx.rx_tsc,
                s.rx.hw_rx_ns,
                s.rx.sw_rx_ns,
                s.rx.rx_empty_gap_ns,
                s.rx.conn,
            );
            if got != want {
                return Err(fail(format!(
                    "MarketHdr (rx_tsc, hw_rx_ns, sw_rx_ns, rx_empty_gap_ns, conn) = {got:?}, RxMeta has {want:?}"
                )));
            }
        }
    }
    Ok(())
}
