//! BIN-077: request ids restored from responses.

use crate::support;

use hft_event::GwOp;
use hft_venue_proto::{MsgOp, ReqRef, VenueProtocol};
use support::{RX, RepSink, rd};
use venue_binance::{GwCtx, Usdm, usdm};

fn req(id: &str) -> (usdm::ReportClass, Option<ReqRef>) {
    // Minimal legal 200 answer (the query form needs status and quantities).
    let p = format!(
        r#"{{"id":"{id}","status":200,"result":{{"symbol":"BTCUSDT","status":"NEW","executedQty":"0","avgPrice":"0"}}}}"#
    );
    let c = Usdm::classify_report(MsgOp::Text, &p.as_bytes()[..p.len().min(64)]);
    if c != usdm::ReportClass::WsResp {
        return (c, None);
    }
    let mut sink = RepSink::default();
    let o = Usdm::decode_report(rd(), &mut GwCtx::default(), c, p.as_bytes(), &RX, &mut sink).unwrap();
    (c, o.req)
}

#[test]
fn bin_077_request_id() {
    let r = |slot, r#gen, op| Some(ReqRef { slot, r#gen, op });
    let rows = [
        ("BIN-077.1", "p0000002a00000007", r(42, 7, GwOp::Place)),
        ("BIN-077.2", "c0000002a00000007", r(42, 7, GwOp::Cancel)),
        ("BIN-077.3", "m0000002a00000007", r(42, 7, GwOp::Amend)),
        ("BIN-077.4", "q0000002a00000007", r(42, 7, GwOp::Query)),
        ("BIN-077.5", "x0000000000000000", r(0, 0, GwOp::CancelAll)),
        (
            "BIN-077.6",
            "pffffffffffffffff",
            r(0xFFFF_FFFF, 0xFFFF_FFFF, GwOp::Place),
        ),
    ];
    for (row, id, want) in rows {
        assert_eq!(req(id).1, want, "{row}");
    }
    for (row, id) in [
        ("BIN-077.7", "p0000002a0000007"),
        ("BIN-077.8", "p0000002A00000007"),
        ("BIN-077.9", "z0000002a00000007"),
    ] {
        assert_eq!(req(id), (usdm::ReportClass::Unknown, None), "{row}");
    }
}
