//! Synthetic BGT-312/325 cases and independently calculated, sanitized demo captures.
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
                    if let Some((bit, success)) = T::private_ack(b) {
                        vec![serde_json::json!({"bit":bit,"success":success})]
                    } else {
                        vec![serde_json::json!({"control":"SubAck","ok":ok,"code":code})]
                    }
                }
                other => panic!("unexpected control: {other:?}"),
            }
        });
    }
    check::<BitgetUsdtFutures>();
    check::<BitgetSpot>();
}

fn capture(channel: &str, name: &str) -> (Vec<u8>, Vec<serde_json::Value>) {
    product_capture("bitget-usdt-futures", channel, name)
}

fn product_capture(product: &str, channel: &str, name: &str) -> (Vec<u8>, Vec<serde_json::Value>) {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures")
        .join(product)
        .join(channel)
        .join(name);
    let input = std::fs::read(path.with_extension("in")).unwrap();
    let expected = std::fs::read_to_string(path.with_extension("expected.jsonl"))
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    (input, expected)
}

fn compare(expected: &[serde_json::Value], actual: &[serde_json::Value]) {
    assert_eq!(expected.len(), actual.len(), "{actual:?}");
    for (want, got) in expected.iter().zip(actual) {
        for (key, value) in want.as_object().unwrap() {
            assert_eq!(&got[key], value, "{key}: {got}");
        }
    }
}

#[test]
fn bgt_310_login_result() {
    for name in ["ok_seconds", "ok_milliseconds", "rejected"] {
        let (input, expected) = capture("login", name);
        let actual = match BitgetUsdtFutures::login_result(&input) {
            hft_venue_proto::LoginResult::Ok => serde_json::json!({"login":"Ok"}),
            hft_venue_proto::LoginResult::Rejected(code) => {
                serde_json::json!({"login":"Rejected","code":code})
            }
            other => panic!("{other:?}"),
        };
        compare(&expected, &[actual]);
    }
}

#[test]
fn bgt_326_account_settings() {
    use hft_venue_proto::account::{AccountProtocol, Query, State};
    let (input, expected) = capture("rest.account_settings", "basic_oneway");
    let mut state = State::default();
    BitgetUsdtFutures::account_decode(
        account_rd(false),
        ACCT,
        &Query::Settings,
        200,
        &input,
        1,
        &mut state,
    )
    .unwrap();
    let c = state.config.unwrap();
    let mut actual = vec![
        serde_json::json!({"kind":"AccountConfigReport", "position_mode":c.position_mode,
        "margin_mode":c.margin_mode,"multi_asset":c.multi_asset,"uid_keys":c.uid_keys,"mismatches":c.mismatches}),
    ];
    actual.extend(state.leverage.iter().map(
        |l| serde_json::json!({"kind":"LeverageItem","inst":l.inst.0,"leverage":l.leverage.0.to_string()}),
    ));
    compare(&expected, &actual);
    for mode in ["classic", "upgrading", "switching", "unknown"] {
        let mut malformed: serde_json::Value = serde_json::from_slice(&input).unwrap();
        malformed["data"]["accountMode"] = mode.into();
        assert!(
            BitgetUsdtFutures::account_decode(
                account_rd(false),
                ACCT,
                &Query::Settings,
                200,
                &serde_json::to_vec(&malformed).unwrap(),
                1,
                &mut state
            )
            .is_err()
        );
        assert_eq!(state.config.unwrap().uid_keys, 3);
    }
}

#[test]
fn bgt_328_account_info() {
    use hft_venue_proto::account::{AccountProtocol, Query, State};
    let (input, expected) = capture("rest.account_info", "demo_unknown_permissions");
    // Unknown permissions must fail for both identity policies; a working login proves no permissions.
    for unverifiable in [false, true] {
        let mut state = State::default();
        let error = BitgetUsdtFutures::account_decode(
            account_rd(unverifiable),
            ACCT,
            &Query::Permissions,
            200,
            &input,
            1,
            &mut state,
        )
        .unwrap_err();
        compare(&expected, &[serde_json::json!({"error":format!("{error:?}")})]);
        assert!(state.config.is_none());
    }
}

#[test]
fn bgt_329_margin_poll() {
    use hft_venue_proto::account::{AccountProtocol, Query, State};
    let mut state = State::default();
    for name in ["long", "short", "empty"] {
        let (input, expected) = capture("rest.positions", name);
        BitgetUsdtFutures::account_decode(
            account_rd(false),
            ACCT,
            &Query::Positions,
            200,
            &input,
            1,
            &mut state,
        )
        .unwrap();
        let actual: Vec<_> = state.positions.iter().filter(|p| p.qty.0 != 0).map(|p| serde_json::json!({
            "kind":"PositionReport","inst":p.inst.0,"qty":p.qty.0.to_string(),"entry_px":p.entry_px.0.to_string(),
            "upnl":p.upnl.0.to_string(),"liq_px":p.liq_px.0.to_string()})).collect();
        compare(&expected, &actual);
    }
    // Missing list is malformed; only an explicit null is an empty position set.
    let (long, _) = capture("rest.positions", "long");
    BitgetUsdtFutures::account_decode(
        account_rd(false),
        ACCT,
        &Query::Positions,
        200,
        &long,
        1,
        &mut state,
    )
    .unwrap();
    assert!(
        BitgetUsdtFutures::account_decode(
            account_rd(false),
            ACCT,
            &Query::Positions,
            200,
            br#"{"code":"00000","data":{}}"#,
            1,
            &mut state
        )
        .is_err()
    );
    assert_eq!(state.positions.iter().filter(|p| p.qty.0 != 0).count(), 1);
    let (input, expected) = capture("rest.account_assets", "ok");
    BitgetUsdtFutures::account_decode(
        account_rd(false),
        ACCT,
        &Query::Balances,
        200,
        &input,
        1,
        &mut state,
    )
    .unwrap();
    state.balances.sort_by_key(|b| b.asset.0);
    let mut actual: Vec<_> = state
        .balances
        .iter()
        .map(|b| {
            serde_json::json!({"kind":"BalanceReport",
        "asset":b.asset.0,"free":b.free.0.to_string(),"locked":b.locked.0.to_string()})
        })
        .collect();
    let m = state.margin.unwrap();
    actual.push(serde_json::json!({"kind":"MarginReport","margin_balance":m.margin_balance.0.to_string(),
        "maint_margin":m.maint_margin.0.to_string(),"avail_margin":m.avail_margin.0.to_string(),"margin_ratio":m.margin_ratio.0.to_string()}));
    compare(&expected, &actual);
}

fn market(channel: &str, name: &str, route: u8) {
    product_market::<BitgetUsdtFutures>(channel, name, route);
}

fn product_market<T: VenueProtocol<FeedCtx = FeedCtx, GwCtx = GwCtx>>(channel: &str, name: &str, route: u8) {
    use hft_venue_proto::testkit::{MdRec, VecSink};
    use serde_json::json;
    let (input, expected) = product_capture(T::NAME, channel, name);
    let mut sink = VecSink::default();
    let mut ctx = FeedCtx::new();
    let mut events = 0;
    for frame in input.split(|b| *b == b'\n').filter(|f| !f.is_empty()) {
        let class = T::classify(RouteId(route), MsgOp::Text, &frame[..frame.len().min(64)]);
        let result = T::decode_md(rd_spot_capture(), &mut ctx, class, frame, &RX, &mut sink).unwrap();
        events += usize::from(result.events);
    }
    assert_eq!(events, expected.len());
    let actual: Vec<_> = sink.md.iter().map(|e| {
        let mut value = match e {
            MdRec::Bbo(b) => json!({"kind":"Bbo","bid_px":b.bid_px.0.to_string(),"bid_qty":b.bid_qty.0.to_string(),
                "ask_px":b.ask_px.0.to_string(),"ask_qty":b.ask_qty.0.to_string(),"update_id":b.update_id}),
            MdRec::Trade(t) => json!({"kind":"Trade","trade_id":t.trade_id,"px":t.px.0.to_string(),
                "qty":t.qty.0.to_string(),"side":format!("{:?}",t.taker_side).to_lowercase()}),
            MdRec::Mark(m) => json!({"kind":"MarkPrice","px":m.px.0.to_string()}),
            MdRec::Index(m) => json!({"kind":"IndexPrice","px":m.px.0.to_string()}),
            MdRec::Funding(f) => json!({"kind":"Funding","rate":f.rate.0.to_string(),"next_ts_ns":f.next_funding_ns,"interval_s":f.interval_s}),
            other => panic!("unexpected market event: {other:?}"),
        };
        let h = e.market_hdr().unwrap();
        value["inst"] = h.inst.0.into();
        value["ex_ts_ns"] = h.ex_ts_ns.into();
        value["ex_send_ns"] = h.ex_send_ns.into();
        value
    }).collect();
    compare(&expected, &actual);
}

#[test]
fn bgt_300_fut_books1() {
    market("books1", "v3_normal", 0);
}

#[test]
fn bgt_305_fut_trade() {
    market("publicTrade", "v3_snapshot", 2);
    market("publicTrade", "v3_update", 2);
}

#[test]
fn bgt_307_fut_ticker() {
    market("ticker", "v3_normal", 3);
    market("ticker", "funding_rollover", 3);
}

fn report(channel: &str, name: &str) {
    product_report::<BitgetUsdtFutures>(rd(), channel, name);
}

fn product_report<T: VenueProtocol<FeedCtx = FeedCtx, GwCtx = GwCtx>>(
    data: &hft_refdata::RefData,
    channel: &str,
    name: &str,
) {
    use serde_json::json;
    let (input, expected) = product_capture(T::NAME, channel, name);
    let mut sink = RepSink::default();
    let class = T::classify_report(MsgOp::Text, &input[..input.len().min(64)]);
    let result = T::decode_report(data, &mut GwCtx::new(), class, &input, &RX, &mut sink).unwrap();
    assert!(sink.ext.is_empty());
    assert_eq!(sink.funding, 0);
    assert_eq!(usize::from(result.events), sink.events());
    let mut actual: Vec<_> = sink
        .orders
        .iter()
        .map(|e| {
            json!({"kind":"OrderEvent","what":format!("{:?}",e.what),
        "epoch":e.epoch,"slot":e.slot.0,"gen":e.r#gen,"inst":e.inst.0,"detail":e.detail,
        "qty":e.qty.0.to_string(),"cum_filled":e.cum_filled.0.to_string(),"px":e.px.0.to_string(),
        "avg_px":e.avg_px.0.to_string(),"fee":e.fee.0.to_string(),"fee_asset":e.fee_asset.0,
        "exch_order_id":e.exch_order_id,"trade_id":e.trade_id,"ex_ts_ns":e.ex_ts_ns})
        })
        .collect();
    actual.extend(sink.pos.iter().map(|p| {
        json!({"kind":"PositionReport","inst":p.inst.0,
        "qty":p.qty.0.to_string(),"entry_px":p.entry_px.0.to_string(),"upnl":p.upnl.0.to_string(),
        "liq_px":p.liq_px.0.to_string(),"ex_ts_ns":p.ex_ts_ns})
    }));
    actual.extend(sink.bal.iter().map(|b| {
        json!({"kind":"BalanceReport","asset":b.asset.0,
        "free":b.free.0.to_string(),"locked":b.locked.0.to_string(),"ex_ts_ns":b.ex_ts_ns})
    }));
    compare(&expected, &actual);
}

#[test]
fn bgt_317_fut_orders() {
    for name in [
        "post_only_live",
        "canceled",
        "ioc_expired",
        "filled_long",
        "closed_long",
        "filled_short",
        "closed_short",
    ] {
        report("orders", name);
    }
}

#[test]
fn bgt_322_positions() {
    for name in ["long", "short", "flat"] {
        report("positions", name);
    }
}

#[test]
fn bgt_323_fut_account() {
    report("account", "usdt_only");
    report("account", "funding_balance");
}

#[test]
fn bgt_318_spot_orders() {
    for name in [
        "live",
        "canceled",
        "ioc_expired",
        "partially_filled",
        "filled",
        "sell_partial",
        "sell_filled",
    ] {
        product_report::<BitgetSpot>(rd_spot_capture(), "orders", name);
    }
    // The real stream sends equal-cumulative partial/terminal frames; overlapping delivery
    // cannot charge the same fill or fee again.
    let mut ctx = GwCtx::new();
    let mut sink = RepSink::default();
    for name in ["partially_filled", "filled", "partially_filled", "filled"] {
        let (input, _) = product_capture(BitgetSpot::NAME, "orders", name);
        let class = BitgetSpot::classify_report(MsgOp::Text, &input[..64]);
        BitgetSpot::decode_report(rd_spot_capture(), &mut ctx, class, &input, &RX, &mut sink).unwrap();
    }
    assert_eq!(sink.orders.len(), 1);
    assert_eq!(sink.orders[0].qty.0, 500_000);
    assert_eq!(sink.orders[0].fee.0, 500);
}

#[test]
fn bgt_324_spot_account() {
    use hft_venue_proto::account::{AccountProtocol, Query, State};
    for name in ["locked", "multi_coin"] {
        product_report::<BitgetSpot>(rd_spot_capture(), "account", name);
    }
    let (input, expected) = product_capture(BitgetSpot::NAME, "rest.account_assets", "locked");
    let mut state = State::default();
    BitgetSpot::account_decode(
        rd_spot_capture(),
        hft_types::AccountId(1),
        &Query::Balances,
        200,
        &input,
        1,
        &mut state,
    )
    .unwrap();
    let actual: Vec<_> = state.balances.iter().filter(|b| b.asset == USDT || b.asset == ETH).map(|b| {
        serde_json::json!({"kind":"BalanceReport","asset":b.asset.0,"free":b.free.0.to_string(),"locked":b.locked.0.to_string()})
    }).collect();
    compare(&expected, &actual);
}

#[test]
fn bgt_321_fill_channel() {
    for name in ["buy", "sell"] {
        let (input, expected) = product_capture(BitgetSpot::NAME, "fill", name);
        let class = BitgetSpot::classify_report(MsgOp::Text, &input[..64]);
        let mut sink = RepSink::default();
        let result = BitgetSpot::decode_report(
            rd_spot_capture(),
            &mut GwCtx::new(),
            class,
            &input,
            &RX,
            &mut sink,
        )
        .unwrap();
        assert_eq!(sink.events(), 0);
        compare(
            &expected,
            &[serde_json::json!({"skip":format!("{:?}",result.skip),"events":result.events})],
        );
    }
}

#[test]
fn bgt_336_ro_funding() {
    use hft_venue_proto::{FundingHistoryProtocol, FundingQuery};
    let query = FundingQuery {
        start_ms: 1_792_018_800_000,
        end_ms: 1_792_051_500_000,
        page: 1,
        cursor: Default::default(),
    };
    for name in ["empty", "trades_only"] {
        let (input, expected) = capture("rest.bills", name);
        let result = BitgetUsdtFutures::funding_decode(rd(), &query, 200, &input).unwrap();
        compare(
            &expected,
            &[serde_json::json!({"bills":result.bills.len(),"has_next":result.next.is_some()})],
        );
    }
    let (input, expected) = capture("rest.bills", "funding");
    let result = BitgetUsdtFutures::funding_decode(rd(), &query, 200, &input).unwrap();
    let mut actual: Vec<_> = result.bills.iter().map(|b| {
        serde_json::json!({"id":b.id,"inst":b.inst.0,"amount":b.amount.0.to_string(),"time_ns":b.time_ns})
    }).collect();
    actual.push(serde_json::json!({"has_next":result.next.is_some()}));
    compare(&expected, &actual);
    for input in [
        br#"{"code":"00000","data":{"cursor":null}}"#.as_slice(),
        br#"{"code":"00000","data":{"list":null,"cursor":"123"}}"#,
    ] {
        assert!(BitgetUsdtFutures::funding_decode(rd(), &query, 200, input).is_err());
    }
}

#[test]
fn bgt_333_order_detail() {
    use hft_event::GwOp;
    use hft_types::InstId;
    use hft_venue_proto::{ReqRef, RestOrigin, RestWhat};
    fn check<T: VenueProtocol<FeedCtx = FeedCtx, GwCtx = GwCtx>>() {
        for name in ["filled", "live", "canceled", "not_found"] {
            let (input, expected) = product_capture(T::NAME, "rest.order_detail", name);
            let e = &expected[0];
            let origin = RestOrigin {
                what: RestWhat::Order,
                inst: InstId(e["inst"].as_u64().unwrap() as u32),
                req: Some(ReqRef {
                    slot: e["slot"].as_u64().unwrap() as u32,
                    r#gen: 1,
                    op: GwOp::Query,
                }),
            };
            let mut sink = RepSink::default();
            let outcome = T::decode_rest(
                rd_spot_capture(),
                &mut GwCtx::new(),
                &origin,
                if name == "not_found" { 400 } else { 200 },
                &input,
                &RX,
                &mut sink,
            )
            .unwrap();
            assert_eq!(outcome.events, 1);
            assert!(outcome.outcome_unknown.is_none());
            let actual: Vec<_> = sink.orders.iter().map(|e| serde_json::json!({
                "kind":format!("{:?}",e.what),"inst":e.inst.0,"epoch":e.epoch,"slot":e.slot.0,"gen":e.r#gen,
                "detail":e.detail,"cum_filled":e.cum_filled.0.to_string(),"avg_px":e.avg_px.0.to_string(),
                "exch_order_id":e.exch_order_id,"ex_ts_ns":e.ex_ts_ns
            })).collect();
            compare(&expected, &actual);
        }
    }
    check::<BitgetUsdtFutures>();
    check::<BitgetSpot>();
}

#[test]
fn bgt_301_spot_books1() {
    product_market::<BitgetSpot>("books1", "v3_normal", 0);
}

#[test]
fn bgt_306_spot_trade() {
    product_market::<BitgetSpot>("publicTrade", "v3_snapshot", 2);
}

fn rest_book<T: VenueProtocol<FeedCtx = FeedCtx, GwCtx = GwCtx>>(name: &str, inst: hft_types::InstId) {
    use hft_venue_proto::testkit::{MdRec, VecSink};
    let (input, expected) = product_capture(T::NAME, "rest.bbo_depth", name);
    let mut sink = VecSink::default();
    let result = T::decode_bbo_snapshot(
        rd_spot_capture(),
        &mut FeedCtx::new(),
        &[inst],
        &input,
        &RX,
        &mut sink,
    );
    let actual = match result {
        Ok(events) => {
            assert_eq!(events, 1);
            let MdRec::Bbo(e) = sink.md[0] else {
                panic!("expected BBO")
            };
            serde_json::json!({"kind":"Bbo","inst":e.h.inst.0,"bid_px":e.bid_px.0.to_string(),"bid_qty":e.bid_qty.0.to_string(),
                "ask_px":e.ask_px.0.to_string(),"ask_qty":e.ask_qty.0.to_string(),"update_id":e.update_id,
                "ex_ts_ns":e.h.ex_ts_ns,"ex_send_ns":e.h.ex_send_ns})
        }
        Err(error) => serde_json::json!({"error":format!("{error:?}"),"events":sink.md.len()}),
    };
    compare(&expected, &[actual]);
}

#[test]
fn bgt_308_fut_bbo_snapshot() {
    for name in ["normal", "error_code"] {
        rest_book::<BitgetUsdtFutures>(name, F_ETH);
    }
}

#[test]
fn bgt_309_spot_bbo_snapshot() {
    for name in ["normal", "unknown_symbol"] {
        rest_book::<BitgetSpot>(name, hft_types::InstId(1903));
    }
}

fn command_response<T: VenueProtocol<FeedCtx = FeedCtx, GwCtx = GwCtx>>(channel: &str, name: &str) {
    let (input, expected) = product_capture(T::NAME, channel, name);
    let mut sink = RepSink::default();
    let class = T::classify_report(MsgOp::Text, &input[..input.len().min(64)]);
    let outcome = T::decode_report(
        rd_spot_capture(),
        &mut GwCtx::new(),
        class,
        &input,
        &RX,
        &mut sink,
    )
    .unwrap();
    let req = outcome.req.unwrap();
    let mut actual:Vec<_>=sink.orders.iter().map(|e| serde_json::json!({
        "kind":"OrderEvent","what":format!("{:?}",e.what),"inst":e.inst.0,"epoch":e.epoch,"slot":e.slot.0,"gen":e.r#gen,
        "detail":e.detail,"reject_code":e.reject_code,"exch_order_id":e.exch_order_id,"ex_ts_ns":e.ex_ts_ns
    })).collect();
    actual.push(serde_json::json!({"events":outcome.events,"op":format!("{:?}",req.op),"slot":req.slot,"unknown":outcome.outcome_unknown.is_some(),"raw_code":outcome.raw_code}));
    compare(&expected, &actual);
}

#[test]
fn bgt_313_fut_place_resp() {
    for name in ["ok", "param_error_unregistered", "reduce_only_unregistered"] {
        command_response::<BitgetUsdtFutures>("place-order", name);
    }
}

#[test]
fn bgt_314_spot_place_resp() {
    command_response::<BitgetSpot>("place-order", "ok");
}

#[test]
fn bgt_316_cancel_resp() {
    command_response::<BitgetSpot>("cancel-order", "ok");
    command_response::<BitgetUsdtFutures>("cancel-order", "unknown_order");
}
