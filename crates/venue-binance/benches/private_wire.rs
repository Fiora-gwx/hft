//! Private order/fill decode histogram. Setup, assertions and sink destruction are untimed.
//! Synthetic payloads; shared ARM CI is report-only, never the production acceptance host.
use hft_venue_proto::{MsgOp, VenueProtocol};
use std::{hint::black_box, time::Instant};
#[path = "support/sink.rs"]
mod sink;
#[path = "../tests/support/mod.rs"]
mod support;
use venue_binance::GwCtx;
type Protocol = venue_binance::Usdm;
const PAYLOAD: &str = r#"{"e":"ORDER_TRADE_UPDATE","o":{"s":"BTCUSDT","c":"1000301020002a000000073d","S":"BUY","o":"LIMIT","x":"TRADE","X":"PARTIALLY_FILLED","i":325078477,"l":"0.001","z":"0.003","L":"65000.1","ap":"65000.05","n":"0.0130","N":"USDT","rp":"0","T":1790000000010,"t":9001}}"#;

fn main() {
    let rd = support::rd();
    let long = format!("{{\"metadata\":\"{}\",{}", "x".repeat(2048), &PAYLOAD[1..]);
    for (shape, payload) in [("normal", PAYLOAD), ("padded-2k", long.as_str())] {
        // The long fixture keeps dispatch separate so we measure the same selected decoder.
        let class = Protocol::classify_report(MsgOp::Text, PAYLOAD.as_bytes());
        for duplicate in [false, true] {
            let mut samples = Vec::with_capacity(20_000);
            for n in 0..22_000 {
                let mut ctx = GwCtx::default();
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
