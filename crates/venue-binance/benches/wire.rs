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
    use venue_binance::{FeedCtx, Spot, Usdm};
    md::<Usdm>(
        "binance/bbo",
        0,
        support::BT1.as_bytes(),
        support::rd(),
        FeedCtx::USDM,
    );
    md::<Spot>(
        "binance/spot-bbo",
        0,
        support::SBT1.as_bytes(),
        support::rd(),
        FeedCtx::SPOT,
    );
    md::<Usdm>(
        "binance/captured-bbo",
        0,
        include_bytes!("../../../fixtures/binance-usdm/bookTicker/normal.in"),
        support::rd_market(),
        FeedCtx::USDM,
    );
    md::<Usdm>(
        "binance/trade",
        2,
        include_bytes!("../../../fixtures/binance-usdm/aggTrade/taker_buy.in"),
        support::rd_market(),
        FeedCtx::USDM,
    );
    md::<Usdm>(
        "binance/funding-mark",
        2,
        include_bytes!("../../../fixtures/binance-usdm/markPriceUpdate/normal.in"),
        support::rd_market(),
        FeedCtx::USDM,
    );
    md::<Usdm>("binance/depth", 2, br#"{"e":"depthUpdate","s":"BTCUSDT","u":123,"T":1790000000123,"E":1790000000125,"b":[["65000.1","1"]],"a":[["65000.2","2"]]}"#, support::rd(), FeedCtx::USDM);
}
