//! BIN-092, BIN-093, BIN-095: the two code tables.

use crate::support;

use hft_event::{GwOp, OrderEventKind};
use hft_refdata::LimitBucket;
use hft_types::SlotId;
use hft_venue_proto::{MsgOp, RateObs, ReportCtl, ReportOutcome, ReqRef, VenueProtocol};
use support::{RX, RepSink, rd};
use venue_binance::{GwCtx, Spot, Usdm};

fn resp(status: u16, code: Option<i64>) -> String {
    match code {
        Some(c) => {
            format!(r#"{{"id":"p0000002a00000007","status":{status},"error":{{"code":{c},"msg":"x"}}}}"#)
        }
        None => format!(r#"{{"id":"p0000002a00000007","status":{status}}}"#),
    }
}

fn dec<P: VenueProtocol<GwCtx = GwCtx>>(p: &str) -> (ReportOutcome, RepSink) {
    let mut s = RepSink::default();
    let c = P::classify_report(MsgOp::Text, &p.as_bytes()[..p.len().min(64)]);
    let o = P::decode_report(rd(), &mut GwCtx, c, p.as_bytes(), &RX, &mut s).unwrap();
    (o, s)
}

#[derive(Copy, Clone, Debug, PartialEq)]
enum W {
    Reject(u32),
    Unknown(u32),
}

const PLACE: ReqRef = ReqRef {
    slot: 42,
    r#gen: 7,
    op: GwOp::Place,
};

fn check(row: &str, o: &ReportOutcome, s: &RepSink, w: W) {
    match w {
        W::Reject(raw) => {
            assert_eq!(s.orders.len(), 1, "{row}");
            let e = s.orders[0];
            assert_eq!(
                (e.what, e.slot, e.r#gen, e.epoch, e.reject_code),
                (OrderEventKind::Reject, SlotId(42), 7, 0, raw),
                "{row}"
            );
            assert_eq!(o.outcome_unknown, None, "{row}");
            assert_eq!(o.raw_code, raw, "{row}");
        }
        W::Unknown(raw) => {
            assert!(s.orders.is_empty(), "{row}");
            assert_eq!(o.outcome_unknown, Some(PLACE), "{row}");
            assert_eq!(o.raw_code, raw, "{row}");
        }
    }
}

#[test]
fn bin_092_error_code_table() {
    let full = |bucket, limit, window_ms| {
        Some(RateObs {
            bucket,
            used: limit,
            limit,
            window_ms,
        })
    };
    type Row = (&'static str, u16, Option<i64>, W, ReportCtl, Option<RateObs>);
    let rows: [Row; 21] = [
        (
            "BIN-092.1",
            400,
            Some(-1013),
            W::Reject(0xFFFF_FC0B),
            ReportCtl::None,
            None,
        ),
        (
            "BIN-092.2",
            400,
            Some(-1021),
            W::Reject(0xFFFF_FC03),
            ReportCtl::ClockSkew,
            None,
        ),
        (
            "BIN-092.3",
            400,
            Some(-1111),
            W::Reject(0xFFFF_FBA9),
            ReportCtl::None,
            None,
        ),
        (
            "BIN-092.4",
            400,
            Some(-2010),
            W::Reject(0xFFFF_F826),
            ReportCtl::None,
            None,
        ),
        (
            "BIN-092.5",
            400,
            Some(-2019),
            W::Reject(0xFFFF_F81D),
            ReportCtl::None,
            None,
        ),
        (
            "BIN-092.6",
            400,
            Some(-2022),
            W::Reject(0xFFFF_F81A),
            ReportCtl::None,
            None,
        ),
        (
            "BIN-092.7",
            400,
            Some(-4164),
            W::Reject(0xFFFF_EFBC),
            ReportCtl::None,
            None,
        ),
        (
            "BIN-092.8",
            400,
            Some(-5022),
            W::Reject(0xFFFF_EC62),
            ReportCtl::None,
            None,
        ),
        (
            "BIN-092.9",
            400,
            Some(-4061),
            W::Reject(0xFFFF_F023),
            ReportCtl::None,
            None,
        ),
        (
            "BIN-092.10",
            429,
            Some(-1003),
            W::Reject(0xFFFF_FC15),
            ReportCtl::RateLimited,
            full(LimitBucket::Weight, 2400, 60_000),
        ),
        (
            "BIN-092.11",
            429,
            Some(-1015),
            W::Reject(0xFFFF_FC09),
            ReportCtl::RateLimited,
            full(LimitBucket::Orders, 300, 10_000),
        ),
        (
            "BIN-092.12",
            429,
            None,
            W::Reject(0x2000_01AD),
            ReportCtl::RateLimited,
            full(LimitBucket::Weight, 2400, 60_000),
        ),
        (
            "BIN-092.13",
            418,
            None,
            W::Reject(0x2000_01A2),
            ReportCtl::RateLimited,
            full(LimitBucket::Weight, 2400, 60_000),
        ),
        (
            "BIN-092.14",
            408,
            Some(-1007),
            W::Unknown(0xFFFF_FC11),
            ReportCtl::None,
            None,
        ),
        (
            "BIN-092.15",
            500,
            Some(-1001),
            W::Unknown(0xFFFF_FC17),
            ReportCtl::None,
            None,
        ),
        (
            "BIN-092.16",
            503,
            Some(-1008),
            W::Unknown(0xFFFF_FC10),
            ReportCtl::None,
            None,
        ),
        (
            "BIN-092.17",
            500,
            None,
            W::Unknown(0x2000_01F4),
            ReportCtl::None,
            None,
        ),
        (
            "BIN-092.18",
            502,
            None,
            W::Unknown(0x2000_01F6),
            ReportCtl::None,
            None,
        ),
        (
            "BIN-092.19",
            503,
            None,
            W::Unknown(0x2000_01F7),
            ReportCtl::None,
            None,
        ),
        (
            "BIN-092.20",
            400,
            Some(-9999),
            W::Unknown(0xFFFF_D8F1),
            ReportCtl::None,
            None,
        ),
        (
            "BIN-092.21",
            400,
            Some(-1121),
            W::Unknown(0xFFFF_FB9F),
            ReportCtl::None,
            None,
        ),
    ];
    for (row, st, code, w, ctl, rate) in rows {
        let (o, s) = dec::<Usdm>(&resp(st, code));
        check(row, &o, &s, w);
        assert_eq!(o.ctl, ctl, "{row} (BIN-092.22)");
        if rate.is_some() {
            assert_eq!(o.rate, rate, "{row}");
        }
    }
    // Same as .5 on spot: both tables are shared by the two lines.
    let (o, s) = dec::<Spot>(&resp(400, Some(-2019)));
    check("BIN-092.22 (spot)", &o, &s, W::Reject(0xFFFF_F81D));
}

#[test]
fn bin_093_cancel_unknown_order() {
    let (o, s) = dec::<Usdm>(
        r#"{"id":"c0000002a00000007","status":400,"error":{"code":-2011,"msg":"Unknown order sent."}}"#,
    );
    assert_eq!(s.orders.len(), 1);
    let e = s.orders[0];
    assert_eq!(
        (e.what, e.slot, e.r#gen, e.reject_code, e.detail),
        (OrderEventKind::CancelReject, SlotId(42), 7, 0xFFFF_F825, 1)
    );
    assert_eq!(
        (o.req, o.outcome_unknown, o.raw_code),
        (
            Some(ReqRef {
                slot: 42,
                r#gen: 7,
                op: GwOp::Cancel
            }),
            None,
            0xFFFF_F825
        )
    );
}

#[test]
fn bin_095_query_not_found() {
    let (o, s) = dec::<Usdm>(
        r#"{"id":"q0000002a00000007","status":400,"error":{"code":-2013,"msg":"Order does not exist."}}"#,
    );
    assert_eq!(s.orders.len(), 1);
    let e = s.orders[0];
    assert_eq!(
        (
            e.what,
            e.slot,
            e.r#gen,
            e.cum_filled.0,
            e.avg_px.0,
            e.reject_code,
            e.detail
        ),
        (OrderEventKind::QueryResult, SlotId(42), 7, 0, 0, 0xFFFF_F823, 7)
    );
    assert_eq!(o.outcome_unknown, None);
}
