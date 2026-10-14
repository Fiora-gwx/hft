//! Shared libFuzzer entry and deterministic truncation smoke test. No network.
use crate::support::{RX, RepSink, rd};
use hft_types::InstId;
use hft_venue_proto::{
    MsgOp, RouteId,
    account::{AccountProtocol, Query, State},
    testkit::VecSink,
};
pub fn run<P: AccountProtocol>(data: &[u8], inst: InstId, mut feed: P::FeedCtx, mut gw: P::GwCtx) {
    // Bound cold response allocations and exercise consecutive private messages in one context.
    if data.len() > 262_144 {
        return;
    }
    let mut md = VecSink::default();
    let mut rep = RepSink::default();
    for payload in data.split(|b| *b == b'\n').take(8) {
        let head = payload.get(..64).unwrap_or(payload);
        for route in P::routes() {
            let c = P::classify(route.id, MsgOp::Text, head);
            let _ = P::peek(rd(), &feed, c, payload);
            let _ = P::decode_md(rd(), &mut feed, c, payload, &RX, &mut md);
        }
        let c = P::classify_report(MsgOp::Text, head);
        let _ = P::decode_report(rd(), &mut gw, c, payload, &RX, &mut rep);
        let _ = P::decode_bbo_snapshot(rd(), &mut feed, &[inst], payload, &RX, &mut md);
        for q in [
            Query::Settings,
            Query::Permissions,
            Query::Balances,
            Query::Positions,
        ] {
            let _ = P::account_decode(rd(), RX.account, &q, 200, payload, 1000, &mut State::default());
        }
    }
    for e in &md.md {
        if let Some(h) = e.market_hdr() {
            assert_eq!(h.rx_tsc, RX.rx_tsc);
            assert_eq!(h.conn, RX.conn);
        }
    }
    for e in &rep.orders {
        assert!(e.cum_filled.0 <= 0 || e.avg_px.0 > 0);
    }
    let _ = RouteId(0);
}

#[test]
fn bgt_400_decoder_truncation_smoke() {
    let sample: &[u8] = br#"{"arg":{"topic":"books1","symbol":"BTCUSDT"},"data":[{"b":[["65000.1","1"]],"a":[["65000.2","2"]],"seq":1,"ts":"1000"}]}"#;
    for n in 0..=sample.len() {
        run::<venue_bitget::BitgetUsdtFutures>(
            &sample[..n],
            crate::support::F_BTC,
            venue_bitget::FeedCtx::new(),
            venue_bitget::GwCtx::new(),
        );
    }
    for n in 0..256u16 {
        run::<venue_bitget::BitgetSpot>(
            &[n as u8; 31],
            crate::support::S_BTC,
            venue_bitget::FeedCtx::new(),
            venue_bitget::GwCtx::new(),
        );
    }
}
