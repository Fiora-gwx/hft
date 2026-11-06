//! Shared libFuzzer entry and deterministic truncation smoke test (CBX-016). No network.
use crate::support::{RX, rd};
use hft_event::{
    AccountConfigReportHdr, BalanceReport, ExternalFill, FundingSettle, LeverageItem, MarginItem,
    MarginReportHdr, OrderEvent, PositionReport,
};
use hft_types::ClientOrderId;
use hft_venue_proto::{ExchIdHint, MsgOp, ReportSink, VenueProtocol, testkit::VecSink};
use venue_coinbase::{CoinbaseSpot, FeedCtx};

/// A `ReportSink` that drops everything (the venue has no reports).
pub struct NullReports;

impl ReportSink for NullReports {
    fn order(&mut self, _e: &OrderEvent) {}
    fn position(&mut self, _e: &PositionReport) {}
    fn balance(&mut self, _e: &BalanceReport) {}
    fn funding_settle(&mut self, _e: &FundingSettle) {}
    fn external_fill(&mut self, _e: &ExternalFill) {}
    fn margin(&mut self, _hdr: &MarginReportHdr, _items: &[MarginItem]) {}
    fn account_config(&mut self, _hdr: &AccountConfigReportHdr, _lev: &[LeverageItem]) {}
    fn exch_id(&mut self, _key: &[u8], _owner: Option<&ClientOrderId>) -> ExchIdHint {
        ExchIdHint::Unknown
    }
}

/// Every line of `data` through classify, control, peek and decode on both routes.
pub fn run(data: &[u8]) {
    if data.len() > 262_144 {
        return;
    }
    let mut md = VecSink::default();
    let mut rep = NullReports;
    for payload in data.split(|b| *b == b'\n').take(8) {
        let head = payload.get(..64).unwrap_or(payload);
        for route in CoinbaseSpot::routes() {
            let c = CoinbaseSpot::classify(route.id, MsgOp::Text, head);
            let _ = CoinbaseSpot::control(c, payload);
            let _ = CoinbaseSpot::peek(rd(), &FeedCtx, c, payload);
            let _ = CoinbaseSpot::decode_md(rd(), &mut FeedCtx, c, payload, &RX, &mut md);
        }
        let c = CoinbaseSpot::classify_report(MsgOp::Text, head);
        let _ = CoinbaseSpot::decode_report(rd(), &mut (), c, payload, &RX, &mut rep);
        let _ = venue_coinbase::iso8601_ns(payload);
    }
    for e in &md.md {
        if let Some(h) = e.market_hdr() {
            assert_eq!((h.rx_tsc, h.conn), (RX.rx_tsc, RX.conn));
        }
    }
}

#[test]
fn cbx_016_decoder_truncation_smoke() {
    for (mt, case) in [("ticker", "normal"), ("match", "taker_buy"), ("last_match", "on_subscribe"), ("heartbeat", "normal")] {
        let (sample, _) = crate::support::fixture(mt, case);
        for n in 0..=sample.len() {
            run(&sample[..n]);
        }
    }
    for n in 0..256u16 {
        run(&[n as u8; 31]);
    }
}
