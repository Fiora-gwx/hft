use crate::support::*;
use hft_event::GwOp;
use hft_types::InstId;
use hft_venue_proto::*;
use venue_binance::{GwCtx, Usdm};

#[test]
fn bin_081_decode_rest() {
    let mut ctx = GwCtx::default();
    let req = ReqRef {
        slot: 0,
        r#gen: 0,
        op: GwOp::CancelAll,
    };
    let mut sink = RepSink::default();
    for (what, status, body, ctl, code) in [
        (
            RestWhat::Order,
            200,
            br#"{"code":200}"#.as_slice(),
            ReportCtl::None,
            0,
        ),
        (
            RestWhat::Order,
            429,
            br#"{"code":-1003}"#,
            ReportCtl::RateLimited,
            (-1003i32) as u32,
        ),
        (
            RestWhat::DeadMan,
            200,
            br#"{"symbol":"BTCUSDT","countdownTime":"30000"}"#,
            ReportCtl::None,
            0,
        ),
        (
            RestWhat::DeadMan,
            200,
            br#"{"symbol":"BTCUSDT","countdownTime":30000}"#,
            ReportCtl::None,
            0,
        ),
        (
            RestWhat::Keepalive,
            200,
            br#"{"listenKey":"fixture-only"}"#,
            ReportCtl::None,
            0,
        ),
        (
            RestWhat::Keepalive,
            400,
            br#"{"code":-1125}"#,
            ReportCtl::UserStreamEnded,
            (-1125i32) as u32,
        ),
    ] {
        let origin = RestOrigin {
            what,
            req: (what == RestWhat::Order).then_some(req),
            inst: InstId(0),
        };
        let out = Usdm::decode_rest(rd(), &mut ctx, &origin, status, body, &RX, &mut sink).unwrap();
        assert_eq!((out.events, out.ctl, out.raw_code), (0, ctl, code));
    }
    let origin = RestOrigin {
        what: RestWhat::Order,
        req: Some(req),
        inst: InstId(0),
    };
    assert_eq!(
        Usdm::decode_rest(rd(), &mut ctx, &origin, 200, b"[", &RX, &mut sink),
        Err(DecodeErr::Truncated)
    );
    let out = Usdm::decode_rest(rd(), &mut ctx, &origin, 503, b"unavailable", &RX, &mut sink).unwrap();
    assert_eq!(out.outcome_unknown, Some(req));
    assert_eq!(out.events, 0);
}
