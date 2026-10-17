//! Private order/fill decode histogram. Setup, assertions and sink destruction are untimed.
//! Synthetic payloads; shared ARM CI is report-only, never the production acceptance host.
use hft_venue_proto::{MsgOp, VenueProtocol};
use std::{hint::black_box, time::Instant};
#[path = "support/sink.rs"]
mod sink;
#[path = "../tests/support/mod.rs"]
mod support;
use venue_bitget::GwCtx;
type Protocol = venue_bitget::BitgetUsdtFutures;
const PAYLOAD: &str = r#"{"action":"snapshot","arg":{"instType":"UTA","topic":"order"},"data":[{"category":"usdt-futures","symbol":"BTCUSDT","orderId":"1234567890123","clientOid":"100030a020002a000000078d","price":"65000.1","qty":"0.004","tradeSide":"buy_single","orderType":"limit","timeInForce":"gtc","side":"buy","cumExecQty":"0.004","cumExecValue":"260","avgPrice":"65000","orderStatus":"filled","cancelReason":"","feeDetail":[{"feeCoin":"USDT","fee":"0.04"}],"createdTime":"1790000000400","updatedTime":"1790000000500"}],"ts":1790000000501}"#;

fn main() {
    let rd = support::rd();
    let long = format!("{{\"metadata\":\"{}\",{}", "x".repeat(2048), &PAYLOAD[1..]);
    for (shape, payload) in [("normal", PAYLOAD), ("padded-2k", long.as_str())] {
        // The long fixture keeps dispatch separate so we measure the same selected decoder.
        let class = Protocol::classify_report(MsgOp::Text, PAYLOAD.as_bytes());
        for duplicate in [false, true] {
            let mut samples = Vec::with_capacity(20_000);
            for n in 0..22_000 {
                let mut ctx = GwCtx::new();
                let mut sink = sink::CountSink::default();
                if duplicate {
                    assert_eq!(
                        Protocol::decode_report(
                            rd,
                            &mut ctx,
                            class,
                            payload.as_bytes(),
                            &support::RX,
                            &mut sink
                        )
                        .unwrap()
                        .events,
                        1
                    );
                }
                let start = Instant::now();
                let out = black_box(Protocol::decode_report(
                    rd,
                    &mut ctx,
                    class,
                    black_box(payload.as_bytes()),
                    &support::RX,
                    &mut sink,
                ));
                let elapsed = start.elapsed().as_nanos() as u64;
                assert_eq!(out.unwrap().events, u16::from(!duplicate));
                black_box(&sink);
                if n >= 2_000 {
                    samples.push(elapsed);
                }
            }
            samples.sort_unstable();
            let q = |n: usize| samples[(samples.len() * n).div_ceil(1000) - 1];
            println!(
                "{}",
                serde_json::json!({"shape":shape,"bytes":payload.len(),"duplicate":duplicate,"n":samples.len(),"p50_ns":q(500),"p99_ns":q(990),"p999_ns":q(999),"max_ns":samples.last()})
            );
        }
    }
}
