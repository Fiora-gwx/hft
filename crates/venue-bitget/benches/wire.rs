//! Per-message wire profile, using the existing HistBench (report only on development hosts).
use hft_testkit::bench::HistBench;
use hft_venue_proto::testkit::VecSink;
use hft_venue_proto::{MsgOp, RouteId, VenueProtocol};
use std::hint::black_box;
#[path = "../tests/support/mod.rs"]
mod support;

fn sample(name: &str, f: impl FnMut()) {
    let h = HistBench::smoke(name).iters(50_000).warmup(5_000).run(f).unwrap();
    println!(
        "{}",
        serde_json::json!({"name":name,"n":h.n,"warmup":h.warmup,
        "p50_ns":h.p50_ns,"p99_ns":h.p99_ns,"p999_ns":h.p999_ns,"max_ns":h.max_ns})
    );
}
fn md<P: VenueProtocol>(
    name: &str,
    route: u8,
    payload: &[u8],
    rd: &hft_refdata::RefData,
    mut ctx: P::FeedCtx,
) {
    let class = P::classify(RouteId(route), MsgOp::Text, payload.get(..64).unwrap_or(payload));
    let mut out = VecSink::default();
    P::decode_md(rd, &mut ctx, class, payload, &support::RX, &mut out).unwrap();
    assert!(!out.md.is_empty(), "{name}");
    out.md.reserve(128);
    sample(&format!("wire/{name}/peek"), || {
        black_box(P::peek(rd, &ctx, class, black_box(payload))).unwrap();
    });
    sample(&format!("wire/{name}/decode"), || {
        out.md.clear();
        black_box(P::decode_md(
            rd,
            &mut ctx,
            class,
            black_box(payload),
            &support::RX,
            &mut out,
        ))
        .unwrap();
        black_box(&out.md);
    });
}

fn main() {
    use venue_bitget::{BitgetSpot, BitgetUsdtFutures as Fut, FeedCtx};
    let bbo = support::b1v3("123456789", "\"1790000000123\"", "1790000000125");
    md::<Fut>("bitget/bbo", 0, bbo.as_bytes(), support::rd(), FeedCtx::new());
    md::<BitgetSpot>(
        "bitget/spot-bbo",
        0,
        bbo.replace("usdt-futures", "spot").as_bytes(),
        support::rd(),
        FeedCtx::new(),
    );
    md::<Fut>(
        "bitget/depth",
        1,
        bbo.replace("books1", "books5").as_bytes(),
        support::rd(),
        FeedCtx::new(),
    );
    md::<Fut>("bitget/trade", 2, br#"{"arg":{"topic":"publicTrade","symbol":"BTCUSDT"},"action":"update","data":[{"i":"12345","p":"65000.1","v":"0.004","S":"Buy","T":"1790000000123"}]}"#, support::rd(), FeedCtx::new());
    md::<Fut>("bitget/funding-ticker", 3, br#"{"arg":{"topic":"ticker","symbol":"BTCUSDT"},"data":[{"markPrice":"65000.1","indexPrice":"65000.2","fundingRate":"0.0001","nextFundingTime":"1790028800000","ts":"1790000000123"}]}"#, support::rd(), FeedCtx::new());
}
