//! Synthetic cases explicitly allowed by BGT-312/325. Private-stream goldens await captures.
use crate::support::*;
use hft_venue_proto::{MsgOp, RouteId, VenueProtocol};
use venue_bitget::{BitgetSpot, BitgetUsdtFutures, FeedCtx, GwCtx};

fn fixtures<T: VenueProtocol<FeedCtx = FeedCtx, GwCtx = GwCtx>>(
    channel: &str,
    mut decode: impl FnMut(&[u8]) -> Vec<serde_json::Value>,
) {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures");
    let cases = hft_testkit::golden::golden_cases(&root, T::NAME, channel).unwrap();
    assert!(!cases.is_empty(), "{channel}");
    for case in cases {
        hft_testkit::golden::run_decoder_case(&case, &mut decode).unwrap();
    }
}
fn pong<T: VenueProtocol<FeedCtx = FeedCtx, GwCtx = GwCtx>>() {
    fixtures::<T>("pong", |b| {
        let c = T::classify(RouteId(0), MsgOp::Text, b);
        // Runtime consumes control frames before market-data decoding.
        let report = T::classify_report(MsgOp::Text, b);
        let out = T::decode_report(rd(), &mut GwCtx::new(), report, b, &RX, &mut RepSink::default()).unwrap();
        vec![serde_json::json!({"control":format!("{:?}",T::control(c,b).unwrap()),"events":out.events})]
    });
}
fn unknown<T: VenueProtocol<FeedCtx = FeedCtx, GwCtx = GwCtx>>() {
    fixtures::<T>("unknown", |b| {
        let c = T::classify_report(MsgOp::Text, &b[..b.len().min(64)]);
        let out = T::decode_report(rd(), &mut GwCtx::new(), c, b, &RX, &mut RepSink::default()).unwrap();
        vec![serde_json::json!({"skip":format!("{:?}",out.skip),"events":out.events})]
    });
}
#[test]
fn bgt_312_pong() {
    pong::<BitgetUsdtFutures>();
    pong::<BitgetSpot>();
}
#[test]
fn bgt_325_unknown_events() {
    unknown::<BitgetUsdtFutures>();
    unknown::<BitgetSpot>();
}

#[test]
fn bgt_311_subscribe_ack() {
    fn check<T: VenueProtocol<FeedCtx = FeedCtx, GwCtx = GwCtx>>() {
        fixtures::<T>("subscribe", |b| {
            let c = T::classify(RouteId(0), MsgOp::Text, &b[..b.len().min(64)]);
            match T::control(c, b).unwrap() {
                hft_venue_proto::ControlHint::SubAck { ok, code } => {
                    vec![serde_json::json!({"control":"SubAck","ok":ok,"code":code})]
                }
                other => panic!("unexpected control: {other:?}"),
            }
        });
    }
    check::<BitgetUsdtFutures>();
    check::<BitgetSpot>();
}
