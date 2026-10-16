//! BIN-064–BIN-071, BIN-073, BIN-075, BIN-076: order frames.
#![allow(clippy::disallowed_types)] // serde_json::Value: reference parser of the frames under test

use crate::support;

use hft_event::{GwCmd, GwOp, OrderFlags};
use hft_types::{ClientOrderId, InstId, Px, Qty, Side, SlotId, Tif, Tsc, VenueId};
use hft_venue_proto::{EncodeErr, OrderChannel, TargetRef, VenueProtocol, WallClock};
use support::{ACCT, S_BTC, U_BTC, WC, cfg, cid, hm1, rd};
use venue_binance::{Spot, Templates, Usdm};

fn tpl<P: VenueProtocol<Templates = Templates>>(stale: u32) -> Templates {
    P::build_templates(&cfg(stale), rd(), ACCT, &hm1()).unwrap()
}

fn cmd(op: GwOp, inst: InstId) -> GwCmd {
    GwCmd {
        decision_seq: 0,
        decided_tsc: Tsc(0),
        px: Px(0),
        qty: Qty(0),
        inst,
        slot: SlotId(0),
        r#gen: 0,
        target_slot: SlotId(0),
        op,
        account: ACCT,
        side: Side::Buy,
        tif: Tif::Gtc,
        oflags: 0,
        _pad: [0; 2],
        arg: 0,
        arg2: 0,
    }
}

/// BIN-064's `c`.
fn place_u() -> GwCmd {
    GwCmd {
        slot: SlotId(42),
        r#gen: 7,
        tif: Tif::Ioc,
        px: Px(6_500_010),
        qty: Qty(400_000),
        ..cmd(GwOp::Place, U_BTC)
    }
}

fn tgt(id: ClientOrderId, exch: u64) -> TargetRef {
    TargetRef {
        id,
        exch_order_id: exch,
    }
}

fn enc<P: VenueProtocol<Templates = Templates>>(
    c: &GwCmd,
    t: &TargetRef,
    wc: &WallClock,
    stale: u32,
) -> String {
    let mut body = [0u8; 2048];
    let n = P::encode(rd(), &tpl::<P>(stale), c, t, wc, &mut body).unwrap();
    String::from_utf8(body[..n].to_vec()).unwrap()
}

const U: VenueId = VenueId::BINANCE_USDM;
const S: VenueId = VenueId::BINANCE_SPOT;

#[test]
fn bin_065_position_close_is_exact_reduce_only_market() {
    for side in [Side::Buy, Side::Sell] {
        let c = GwCmd {
            side,
            qty: Qty(1),
            oflags: (OrderFlags::REDUCE_ONLY | OrderFlags::POSITION_CLOSE).0,
            ..place_u()
        };
        let f = enc::<Usdm>(&c, &tgt(cid(U, 3, 42, 7), 0), &WC, 1500);
        let j: serde_json::Value = serde_json::from_str(&f).unwrap();
        let p = &j["params"];
        assert_eq!(p["type"], "MARKET");
        assert_eq!(p["quantity"], "0.00000001");
        assert_eq!(p["reduceOnly"], "true");
        for absent in ["price", "timeInForce", "closePosition"] {
            assert!(p.get(absent).is_none());
        }
    }
}
const PLACE_U: &str = r#"{"id":"p0000002a00000007","method":"order.place","params":{"symbol":"BTCUSDT","side":"BUY","type":"LIMIT","timeInForce":"IOC","price":"65000.1","quantity":"0.004","newClientOrderId":"1000301020002a000000073d","newOrderRespType":"ACK","timestamp":1790000000000,"recvWindow":1500}}"#;

#[test]
fn bin_064_usdm_place_frame() {
    let f = enc::<Usdm>(&place_u(), &tgt(cid(U, 3, 42, 7), 0), &WC, 1500);
    assert_eq!(f, PLACE_U);
    for s in ["reduceOnly", "apiKey", "signature"] {
        assert!(!f.contains(s));
    }
}

#[test]
fn bin_065_usdm_place_variants() {
    let t = tgt(cid(U, 3, 42, 7), 0);
    let f = |c: GwCmd| enc::<Usdm>(&c, &t, &WC, 1500);
    let base = place_u();
    assert!(
        f(GwCmd {
            side: Side::Sell,
            ..base
        })
        .contains(r#""side":"SELL""#),
        "BIN-065.1"
    );
    assert!(
        f(GwCmd {
            tif: Tif::Gtc,
            ..base
        })
        .contains(r#""timeInForce":"GTC""#),
        "BIN-065.2"
    );
    assert!(
        f(GwCmd {
            tif: Tif::Fok,
            ..base
        })
        .contains(r#""timeInForce":"FOK""#),
        "BIN-065.3"
    );
    let po = f(GwCmd {
        tif: Tif::PostOnly,
        ..base
    });
    assert!(po.contains(r#""type":"LIMIT","timeInForce":"GTX""#), "BIN-065.4");
    let ro = f(GwCmd {
        oflags: OrderFlags::REDUCE_ONLY.0,
        ..base
    });
    assert!(
        ro.contains(r#""quantity":"0.004","reduceOnly":"true","newClientOrderId":"#),
        "BIN-065.5"
    );
    let rr = f(GwCmd {
        oflags: OrderFlags::RISK_REDUCING.0,
        ..base
    });
    assert!(!rr.contains("reduceOnly"), "BIN-065.6");
    let smoke = f(GwCmd {
        oflags: OrderFlags::SMOKE.0,
        ..base
    });
    assert_eq!(smoke, PLACE_U, "BIN-065.7");
}

#[test]
fn bin_066_spot_place_variants() {
    let t = tgt(cid(S, 3, 42, 7), 0);
    let base = GwCmd {
        slot: SlotId(42),
        r#gen: 7,
        tif: Tif::Gtc,
        px: Px(6_500_010),
        qty: Qty(100_000),
        ..cmd(GwOp::Place, S_BTC)
    };
    let f = |c: GwCmd| enc::<Spot>(&c, &t, &WC, 1500);
    assert_eq!(
        f(base),
        r#"{"id":"p0000002a00000007","method":"order.place","params":{"symbol":"BTCUSDT","side":"BUY","type":"LIMIT","timeInForce":"GTC","price":"65000.1","quantity":"0.001","newClientOrderId":"1000302020002a000000070d","newOrderRespType":"ACK","selfTradePreventionMode":"EXPIRE_MAKER","timestamp":1790000000000,"recvWindow":1500}}"#,
        "BIN-066.1"
    );
    assert!(
        f(GwCmd {
            tif: Tif::Ioc,
            ..base
        })
        .contains(r#""timeInForce":"IOC""#),
        "BIN-066.2"
    );
    assert!(
        f(GwCmd {
            tif: Tif::Fok,
            ..base
        })
        .contains(r#""timeInForce":"FOK""#),
        "BIN-066.3"
    );
    let po = f(GwCmd {
        tif: Tif::PostOnly,
        ..base
    });
    assert!(
        po.contains(r#""type":"LIMIT_MAKER""#) && !po.contains("timeInForce"),
        "BIN-066.4"
    );
    for (row, fl) in [
        ("BIN-066.5", OrderFlags::REDUCE_ONLY.0),
        (
            "BIN-066.6",
            OrderFlags::REDUCE_ONLY.0 | OrderFlags::RISK_REDUCING.0,
        ),
    ] {
        let mut body = [0u8; 2048];
        let r = Spot::encode(
            rd(),
            &tpl::<Spot>(1500),
            &GwCmd { oflags: fl, ..base },
            &t,
            &WC,
            &mut body,
        );
        assert_eq!(r, Err(EncodeErr::Unsupported(GwOp::Place)), "{row}");
    }
}

#[test]
fn bin_067_shortest_decimal() {
    let t = tgt(cid(U, 3, 42, 7), 0);
    let rows = [
        ("BIN-067.1", 6_500_010, 400_000, "65000.1", "0.004"),
        ("BIN-067.2", 6_500_000, 100_000_000, "65000", "1"),
        ("BIN-067.3", 1, 1, "0.01", "0.00000001"),
        ("BIN-067.4", 6_500_099, 123_456_789, "65000.99", "1.23456789"),
        (
            "BIN-067.5",
            i64::MAX,
            i64::MAX,
            "92233720368547758.07",
            "92233720368.54775807",
        ),
    ];
    for (row, px, qty, ps, qs) in rows {
        let f = enc::<Usdm>(
            &GwCmd {
                px: Px(px),
                qty: Qty(qty),
                ..place_u()
            },
            &t,
            &WC,
            1500,
        );
        let v: serde_json::Value = serde_json::from_str(&f).unwrap();
        assert_eq!(v["params"]["price"], ps, "{row}");
        assert_eq!(v["params"]["quantity"], qs, "{row}");
    }
}

#[test]
fn bin_068_cancel_frame() {
    let c = GwCmd {
        target_slot: SlotId(42),
        ..cmd(GwOp::Cancel, U_BTC)
    };
    let want = r#"{"id":"c0000002a00000007","method":"order.cancel","params":{"symbol":"BTCUSDT","origClientOrderId":"1000301020002a000000073d","timestamp":1790000000000,"recvWindow":1500}}"#;
    assert_eq!(
        enc::<Usdm>(&c, &tgt(cid(U, 3, 42, 7), 325_078_477), &WC, 1500),
        want,
        "BIN-068.1"
    );
    assert_eq!(
        enc::<Usdm>(&c, &tgt(cid(U, 3, 42, 7), 0), &WC, 1500),
        want,
        "BIN-068.2"
    );
    let sc = GwCmd {
        target_slot: SlotId(42),
        ..cmd(GwOp::Cancel, S_BTC)
    };
    let s = enc::<Spot>(&sc, &tgt(cid(S, 3, 42, 7), 0), &WC, 1500);
    assert_eq!(
        s,
        r#"{"id":"c0000002a00000007","method":"order.cancel","params":{"symbol":"BTCUSDT","origClientOrderId":"1000302020002a000000070d","timestamp":1790000000000,"recvWindow":1500}}"#,
        "BIN-068.3"
    );
    assert!(!s.contains("orderId\""), "BIN-068.3");
    let prev = enc::<Usdm>(&c, &tgt(cid(U, 2, 42, 7), 0), &WC, 1500);
    assert!(
        prev.contains(r#""origClientOrderId":"1000201020002a000000072d""#),
        "BIN-068.4"
    );
}

#[test]
fn bin_069_usdm_modify() {
    let c = GwCmd {
        target_slot: SlotId(42),
        side: Side::Buy,
        tif: Tif::Gtc,
        px: Px(6_500_020),
        qty: Qty(300_000),
        ..cmd(GwOp::Amend, U_BTC)
    };
    assert_eq!(
        enc::<Usdm>(&c, &tgt(cid(U, 3, 42, 7), 325_078_477), &WC, 1500),
        r#"{"id":"m0000002a00000007","method":"order.modify","params":{"symbol":"BTCUSDT","side":"BUY","origClientOrderId":"1000301020002a000000073d","price":"65000.2","quantity":"0.003","timestamp":1790000000000,"recvWindow":1500}}"#
    );
}

#[test]
fn bin_070_spot_amend_keep_priority() {
    let t = tgt(cid(S, 3, 42, 7), 0);
    let c = GwCmd {
        target_slot: SlotId(42),
        px: Px(6_500_010),
        qty: Qty(50_000),
        ..cmd(GwOp::Amend, S_BTC)
    };
    let want = r#"{"id":"m0000002a00000007","method":"order.amend.keepPriority","params":{"symbol":"BTCUSDT","origClientOrderId":"1000302020002a000000070d","newQty":"0.0005","timestamp":1790000000000,"recvWindow":1500}}"#;
    assert_eq!(enc::<Spot>(&c, &t, &WC, 1500), want, "BIN-070.1");
    assert_eq!(
        enc::<Spot>(
            &GwCmd {
                px: Px(6_500_020),
                ..c
            },
            &t,
            &WC,
            1500
        ),
        want,
        "BIN-070.2"
    );
    for op in [
        GwOp::Place,
        GwOp::Cancel,
        GwOp::Amend,
        GwOp::Query,
        GwOp::CancelAll,
    ] {
        let mut body = [0u8; 2048];
        if let Ok(n) = Spot::encode(rd(), &tpl::<Spot>(1500), &GwCmd { op, ..c }, &t, &WC, &mut body) {
            assert!(
                !String::from_utf8_lossy(&body[..n]).contains("cancelReplace"),
                "BIN-070.3"
            );
        }
    }
}

#[test]
fn bin_071_query_cancel_all_frames() {
    let q = GwCmd {
        target_slot: SlotId(42),
        ..cmd(GwOp::Query, U_BTC)
    };
    assert_eq!(
        enc::<Usdm>(&q, &tgt(cid(U, 3, 42, 7), 0), &WC, 1500),
        r#"{"id":"q0000002a00000007","method":"order.status","params":{"symbol":"BTCUSDT","origClientOrderId":"1000301020002a000000073d","timestamp":1790000000000,"recvWindow":1500}}"#,
        "BIN-071.1"
    );
    let sq = GwCmd {
        target_slot: SlotId(42),
        ..cmd(GwOp::Query, S_BTC)
    };
    assert_eq!(
        enc::<Spot>(&sq, &tgt(cid(S, 3, 42, 7), 0), &WC, 1500),
        r#"{"id":"q0000002a00000007","method":"order.status","params":{"symbol":"BTCUSDT","origClientOrderId":"1000302020002a000000070d","timestamp":1790000000000,"recvWindow":1500}}"#,
        "BIN-071.2"
    );
    let ca = cmd(GwOp::CancelAll, S_BTC);
    assert_eq!(
        enc::<Spot>(&ca, &tgt(cid(S, 3, 0, 0), 0), &WC, 1500),
        r#"{"id":"x0000000000000000","method":"openOrders.cancelAll","params":{"symbol":"BTCUSDT","timestamp":1790000000000,"recvWindow":1500}}"#,
        "BIN-071.3"
    );
    // USDT-M cancel-all is a REST operation (BIN-072, M3): no WS frame.
    assert_eq!(Usdm::order_channel(GwOp::CancelAll), OrderChannel::Rest);
}

#[test]
fn bin_073_encode_errors() {
    let t = tgt(cid(U, 3, 42, 7), 0);
    let p = place_u();
    let mut body = [0u8; 2048];
    let tp = tpl::<Usdm>(1500);
    let e = |c: &GwCmd, b: &mut [u8]| Usdm::encode(rd(), &tp, c, &t, &WC, b);
    assert_eq!(
        e(&GwCmd { inst: S_BTC, ..p }, &mut body),
        Err(EncodeErr::UnknownInst),
        "BIN-073.1"
    );
    assert_eq!(
        e(
            &GwCmd {
                inst: InstId(9999),
                ..p
            },
            &mut body
        ),
        Err(EncodeErr::UnknownInst),
        "BIN-073.2"
    );
    let len = PLACE_U.len();
    assert_eq!(
        e(&p, &mut body[..len - 1]),
        Err(EncodeErr::FieldOverflow),
        "BIN-073.3"
    );
    assert_eq!(e(&p, &mut body[..len]), Ok(len), "BIN-073.4");
    assert_eq!(
        e(
            &GwCmd {
                op: GwOp::DeadManRefresh,
                ..p
            },
            &mut body
        ),
        Err(EncodeErr::Unsupported(GwOp::DeadManRefresh)),
        "BIN-073.5"
    );
}

#[test]
fn bin_075_timestamp_recv_window() {
    let t = tgt(cid(U, 3, 42, 7), 0);
    let wc1 = WallClock {
        unix_ms: 1_790_000_000_001,
        unix_us: 1_790_000_000_001_000,
    };
    assert!(
        enc::<Usdm>(&place_u(), &t, &WC, 1500).contains(r#""timestamp":1790000000000,"recvWindow":1500"#),
        "BIN-075.1"
    );
    assert!(
        enc::<Usdm>(&place_u(), &t, &wc1, 1500).contains(r#""timestamp":1790000000001"#),
        "BIN-075.2"
    );
    assert!(
        enc::<Usdm>(&place_u(), &t, &WC, 1000).contains(r#""recvWindow":1000"#),
        "BIN-075.3"
    );
}

#[test]
fn bin_076_no_batch_no_per_order_sign() {
    let c1 = place_u();
    let c2 = GwCmd {
        slot: SlotId(43),
        r#gen: 1,
        ..c1
    };
    let t1 = tgt(cid(U, 3, 42, 7), 0);
    let t2 = tgt(cid(U, 3, 43, 1), 0);
    let mut body = [0u8; 2048];
    assert_eq!(
        Usdm::encode_batch(rd(), &tpl::<Usdm>(1500), &[c1, c2], &[t1, t2], &WC, &mut body),
        Err(EncodeErr::Unsupported(GwOp::Place))
    );
    assert_eq!(
        Spot::encode_batch(rd(), &tpl::<Spot>(1500), &[c1, c2], &[t1, t2], &WC, &mut body),
        Err(EncodeErr::Unsupported(GwOp::Place))
    );
    assert!(!Usdm::needs_per_order_signing());
    assert!(!Spot::needs_per_order_signing());
}

#[test]
fn bin_072_usdm_cancel_all_rest() {
    use hft_venue_proto::{Method, ReportCtl, ReqRef, RestOrigin, RestReqBuf, RestWhat};
    let c = cmd(GwOp::CancelAll, U_BTC);
    let target = tgt(cid(U, 3, 0, 0), 0);
    let mut r = RestReqBuf::default();
    Usdm::encode_rest_signed(
        rd(),
        &tpl::<Usdm>(1500),
        &crate::support::hm1(),
        &c,
        &target,
        &WC,
        &mut r,
    )
    .unwrap();
    assert_eq!(r.method, Method::Delete);
    assert!(r.redacted);
    assert_eq!(
        std::str::from_utf8(r.path_query.as_slice()).unwrap(),
        "/fapi/v1/allOpenOrders?symbol=BTCUSDT&timestamp=1790000000000&recvWindow=1500&signature=381745f38cbbfe1f631e935ec6dbf44fe2f5925a7aafd70800b6b6204c139152"
    );
    assert_eq!(
        Usdm::encode_rest_signed(
            rd(),
            &tpl::<Usdm>(1500),
            &crate::support::hm1(),
            &cmd(GwOp::CancelAll, InstId(0)),
            &target,
            &WC,
            &mut r
        ),
        Err(EncodeErr::Unsupported(GwOp::CancelAll))
    );
    let req = RestOrigin {
        what: RestWhat::Order,
        req: Some(ReqRef {
            slot: 0,
            r#gen: 0,
            op: GwOp::CancelAll,
        }),
        inst: U_BTC,
    };
    let mut ctx = venue_binance::GwCtx::default();
    let mut sink = support::RepSink::default();
    let o = Usdm::decode_rest(
        rd(),
        &mut ctx,
        &req,
        200,
        br#"{"code":200}"#,
        &support::RX,
        &mut sink,
    )
    .unwrap();
    assert_eq!(o.events, 0);
    assert!(o.outcome_unknown.is_none());
    let o = Usdm::decode_rest(rd(), &mut ctx, &req, 429, b"", &support::RX, &mut sink).unwrap();
    assert_eq!(o.ctl, ReportCtl::RateLimited);
    assert_eq!(o.outcome_unknown, req.req);
    let o = Usdm::decode_rest(
        rd(),
        &mut ctx,
        &req,
        200,
        br#"{"code":-1021}"#,
        &support::RX,
        &mut sink,
    )
    .unwrap();
    assert_eq!(o.ctl, ReportCtl::ClockSkew);
    assert_eq!(o.outcome_unknown, req.req);
}

#[test]
fn bin_173_deadman_requests_and_response_strings() {
    use hft_venue_proto::{
        DeadManOut, Method,
        account::{AccountProtocol, Query, State},
    };
    let mut out = DeadManOut::default();
    Usdm::dead_man_refresh_signed(
        rd(),
        &tpl::<Usdm>(1500),
        &crate::support::hm1(),
        30_000,
        &[U_BTC, support::U_ETH],
        &WC,
        &mut out,
    )
    .unwrap();
    assert_eq!(out.reqs.len(), 2);
    for r in &out.reqs {
        assert_eq!(r.method, Method::Post);
        assert!(r.redacted);
        assert!(
            std::str::from_utf8(r.path_query.as_slice())
                .unwrap()
                .contains("countdownTime=30000")
        );
    }
    let q = Query::DeadMan {
        symbol: "BTCUSDT".into(),
        timeout_ms: 30_000,
    };
    for body in [
        br#"{"symbol":"BTCUSDT","countdownTime":"30000"}"#.as_slice(),
        br#"{"symbol":"BTCUSDT","countdownTime":30000}"#.as_slice(),
    ] {
        Usdm::account_decode(rd(), ACCT, &q, 200, body, 1, &mut State::default()).unwrap();
    }
    assert!(
        Usdm::account_decode(
            rd(),
            ACCT,
            &q,
            200,
            br#"{"symbol":"BTCUSDT","countdownTime":"0"}"#,
            1,
            &mut State::default()
        )
        .is_err()
    );
}
