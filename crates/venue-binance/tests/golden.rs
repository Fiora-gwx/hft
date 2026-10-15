//! Independently calculated, sanitized demo captures. The selected wire-derived fields are
//! reviewed in the adjacent expected files; this is not a claim of complete M3 coverage.
use crate::support::{RX, RepSink, rd};
use hft_event::GwOp;
use hft_types::{AccountId, InstId};
use hft_venue_proto::account::{AccountProtocol, Query, State};
use hft_venue_proto::{LoginResult, MsgOp, RestOrigin, RestWhat, VenueProtocol};
use serde_json::{Value, json};
use std::{fs, path::PathBuf};
use venue_binance::{GwCtx, Spot, Usdm};

fn fixture(product: &str, channel: &str, name: &str) -> (Vec<u8>, Vec<Value>) {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures")
        .join(format!("binance-{product}"))
        .join(channel)
        .join(name);
    let input = fs::read(path.with_extension("in")).unwrap();
    let expected = fs::read_to_string(path.with_extension("expected.jsonl"))
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    (input, expected)
}

fn compare(expected: &[Value], actual: &[Value], label: &str) {
    assert_eq!(actual.len(), expected.len(), "{label}: {actual:?}");
    for (want, got) in expected.iter().zip(actual) {
        for (key, value) in want.as_object().unwrap() {
            assert_eq!(&got[key], value, "{label}: field {key}; actual {got}");
        }
    }
}

fn login<P: VenueProtocol>(product: &str, channel: &str, name: &str) {
    let (input, expected) = fixture(product, channel, name);
    let actual = match P::login_result(&input) {
        LoginResult::Ok => json!({"login":"Ok"}),
        LoginResult::Rejected(code) => json!({"login":"Rejected","code":code}),
        other => panic!("{product}/{channel}/{name}: {other:?}"),
    };
    compare(&expected, &[actual], name);
}

fn report<P: VenueProtocol<GwCtx = GwCtx>>(product: &str, channel: &str, name: &str) {
    let (input, expected) = fixture(product, channel, name);
    let rx = hft_venue_proto::RxMeta {
        account: AccountId(if product == "spot" { 1 } else { 2 }),
        ..RX
    };
    let mut sink = RepSink::default();
    let mut ctx = GwCtx::default();
    let mut outcome = hft_venue_proto::ReportOutcome::default();
    let mut events = 0;
    for input in input.split(|b| *b == b'\n').filter(|p| !p.is_empty()) {
        outcome = if channel.starts_with("rest.") {
            let origin = RestOrigin {
                what: if channel == "rest.countdownCancelAll" {
                    RestWhat::DeadMan
                } else {
                    RestWhat::Order
                },
                req: Some(hft_venue_proto::ReqRef {
                    slot: 0,
                    r#gen: 0,
                    op: GwOp::CancelAll,
                }),
                inst: InstId(0),
            };
            P::decode_rest(rd(), &mut ctx, &origin, 200, input, &rx, &mut sink)
        } else {
            let class = P::classify_report(MsgOp::Text, &input[..input.len().min(64)]);
            P::decode_report(rd(), &mut ctx, class, input, &rx, &mut sink)
        }
        .unwrap_or_else(|e| panic!("{product}/{channel}/{name}: {e:?}"));
        events += usize::from(outcome.events);
    }
    assert!(sink.ext.is_empty(), "unexpected external fill: {sink:?}");
    assert!(sink.positions.is_empty() && sink.funding.is_empty());
    assert_eq!(events, sink.orders.len() + sink.balances.len());
    for event in &sink.orders {
        if matches!(
            event.what,
            hft_event::OrderEventKind::Fill | hft_event::OrderEventKind::PartialFill
        ) {
            // Existing real executionReport fixtures preserve the exchange side independently.
            let wire: Value =
                serde_json::from_slice(input.split(|b| *b == b'\n').find(|p| !p.is_empty()).unwrap())
                    .unwrap();
            let side = wire.get("event").unwrap_or(&wire)["S"].as_str().unwrap();
            assert_eq!(event.detail, if side == "BUY" { 1 } else { 2 });
        }
    }
    let mut actual: Vec<Value> = sink
        .orders
        .iter()
        .map(|e| {
            json!({
                "kind":"OrderEvent", "what":format!("{:?}",e.what), "venue":e.venue.0,
                "account":e.account.0, "epoch":e.epoch, "slot":e.slot.0, "gen":e.r#gen,
                "inst":e.inst.0, "exch_order_id":e.exch_order_id, "detail":e.detail,
                "reject_code":e.reject_code, "ex_ts_ns":e.ex_ts_ns, "qty":e.qty.0.to_string(),
                "cum_filled":e.cum_filled.0.to_string(), "px":e.px.0.to_string(),
                "avg_px":e.avg_px.0.to_string(), "fee":e.fee.0.to_string(), "fee_asset":e.fee_asset.0
            })
        })
        .collect();
    actual.extend(sink.balances.iter().map(|e| {
        json!({
            "kind":"BalanceReport", "asset":e.asset.0, "free":e.free.0.to_string(),
            "locked":e.locked.0.to_string(), "ex_ts_ns":e.ex_ts_ns
        })
    }));
    if let Some(req) = outcome.outcome_unknown {
        actual.push(json!({"outcome_unknown":{"slot":req.slot,"gen":req.r#gen,
            "op":format!("{:?}",req.op)},"raw_code":outcome.raw_code}));
    }
    if channel.starts_with("rest.") {
        actual.push(json!({"events":outcome.events,"raw_code":outcome.raw_code,
            "ctl":format!("{:?}",outcome.ctl)}));
    } else if outcome.ctl != hft_venue_proto::ReportCtl::None {
        actual.push(json!({"ctl":format!("{:?}",outcome.ctl)}));
    }
    compare(&expected, &actual, &format!("{product}/{channel}/{name}"));
}

#[test]
fn bin_307_usdm_logon() {
    for name in ["ok", "bad_signature", "clock_skew"] {
        login::<Usdm>("usdm", "session.logon", name);
    }
}

#[test]
fn bin_308_usdm_place_responses() {
    for name in [
        "err_-2019",
        "err_-1111",
        "err_-4164",
        "err_-4061",
        "unregistered_-4118",
    ] {
        report::<Usdm>("usdm", "order.place", name);
    }
}

#[test]
fn bin_310_usdm_cancel_responses() {
    report::<Usdm>("usdm", "order.cancel", "err_-2011_unknown");
}

#[test]
fn bin_311_usdm_order_status() {
    report::<Usdm>("usdm", "order.status", "not_found_-2013");
}

#[test]
fn bin_312_usdm_cancel_all_dead_man() {
    for channel in ["rest.allOpenOrders", "rest.countdownCancelAll"] {
        report::<Usdm>("usdm", channel, "ok");
    }
}

#[test]
fn bin_324_spot_session() {
    for name in ["ok", "bad_signature", "clock_skew"] {
        login::<Spot>("spot", "session.logon", name);
    }
    for name in ["ok", "error"] {
        let (input, expected) = fixture("spot", "userDataStream.subscribe", name);
        let (bit, success) = Spot::private_ack(&input).unwrap();
        compare(
            &expected,
            &[json!({"private_ack":{"bit":bit,"success":success}})],
            name,
        );
    }
}

#[test]
fn bin_325_spot_order_responses() {
    report::<Spot>("spot", "order.test", "rate_limited_429");
    for name in ["ack", "err_-1013_filter"] {
        report::<Spot>("spot", "order.place", name);
    }
    for name in ["ok", "err_-2011", "err_-2011_filled"] {
        report::<Spot>("spot", "order.cancel", name);
    }
}

#[test]
fn bin_326_spot_amend_status() {
    report::<Spot>("spot", "order.amend.keepPriority", "ok");
    report::<Spot>("spot", "order.amend.keepPriority", "err_increase_qty");
    for name in ["new", "filled", "canceled"] {
        report::<Spot>("spot", "order.status", name);
    }
}

#[test]
fn bin_327_spot_execution_report() {
    for name in [
        "new",
        "filled",
        "canceled",
        "expired",
        "replaced",
        "fee_base_asset",
        "partially_filled",
        "partial_then_filled",
        "dup_trade",
    ] {
        report::<Spot>("spot", "executionReport", name);
    }
}

#[test]
fn bin_328_spot_user_stream_misc() {
    report::<Spot>("spot", "outboundAccountPosition", "normal");
}

fn account_balances(state: &State) -> Vec<Value> {
    let mut rows: Vec<_> = state.balances.iter().collect();
    rows.sort_by_key(|b| b.asset.0);
    rows.into_iter()
        .map(|b| {
            json!({"kind":"BalanceReport", "asset":b.asset.0,
        "free":b.free.0.to_string(), "locked":b.locked.0.to_string()})
        })
        .collect()
}

#[test]
fn bin_318_usdm_config_queries() {
    let (input, expected) = fixture("usdm", "rest.accountConfig", "missing_uid");
    let mut state = State::default();
    let error =
        Usdm::account_decode(rd(), AccountId(2), &Query::Settings, 200, &input, 10, &mut state).unwrap_err();
    compare(
        &expected,
        &[json!({"error":format!("{error:?}")})],
        "verified UID is required",
    );
    assert!(state.config.is_none());
}

#[test]
fn bin_319_usdm_margin_poll() {
    let (input, expected) = fixture("usdm", "rest.fapi_v3_account", "margin");
    let mut state = State::default();
    Usdm::account_decode(rd(), AccountId(2), &Query::Balances, 200, &input, 10, &mut state).unwrap();
    let m = state.margin.unwrap();
    let mut actual = account_balances(&state);
    actual.push(
        json!({"kind":"MarginReport", "margin_balance":m.margin_balance.0.to_string(),
        "maint_margin":m.maint_margin.0.to_string(), "avail_margin":m.avail_margin.0.to_string(),
        "margin_ratio":m.margin_ratio.0.to_string()}),
    );
    compare(&expected, &actual, "empty futures wallet");
    let (input, expected) = fixture("usdm", "rest.positionRisk", "empty");
    Usdm::account_decode(rd(), AccountId(2), &Query::Positions, 200, &input, 10, &mut state).unwrap();
    compare(
        &expected,
        &[
            json!({"positions_nonzero":state.positions.iter().filter(|p|p.qty.0!=0).count(),
        "margin_items":state.margin_items.len()}),
        ],
        "no positions",
    );
}

#[test]
fn bin_329_spot_config_queries() {
    let (input, expected) = fixture("spot", "rest.api_v3_account", "spot_with_uid");
    let mut state = State::default();
    Spot::account_decode(rd(), AccountId(1), &Query::Settings, 200, &input, 10, &mut state).unwrap();
    Spot::account_decode(rd(), AccountId(1), &Query::Balances, 200, &input, 10, &mut state).unwrap();
    let cfg = state.config.unwrap();
    let end = cfg
        .venue_uid
        .iter()
        .position(|b| *b == 0)
        .unwrap_or(cfg.venue_uid.len());
    let mut actual = vec![json!({"kind":"AccountConfigReport",
        "venue_uid":std::str::from_utf8(&cfg.venue_uid[..end]).unwrap(),
        "position_mode":cfg.position_mode,"margin_mode":cfg.margin_mode,
        "multi_asset":cfg.multi_asset,"uid_keys":cfg.uid_keys})];
    actual.extend(account_balances(&state));
    compare(&expected, &actual, "spot account UID and balances");
    for channel in ["rest.apiRestrictions", "rest.bnbBurn"] {
        let (input, expected) = fixture("spot", channel, "unavailable_http_404");
        let before = format!("{state:?}");
        let error = Spot::account_decode(
            rd(),
            AccountId(1),
            &Query::Permissions,
            404,
            &input,
            10,
            &mut state,
        )
        .unwrap_err();
        compare(&expected, &[json!({"error":format!("{error:?}")})], channel);
        assert_eq!(format!("{state:?}"), before);
    }
}

fn market<P: VenueProtocol<FeedCtx = venue_binance::FeedCtx>>(product: &str, channel: &str, name: &str) {
    use hft_venue_proto::testkit::{MdRec, VecSink};
    let (input, expected) = fixture(product, channel, name);
    let data = if channel == "markPriceUpdate" {
        crate::support::rd_market()
    } else {
        rd()
    };
    let route = hft_venue_proto::RouteId(if channel == "bookTicker" { 1 } else { 3 });
    let mut ctx = if product == "spot" {
        venue_binance::FeedCtx::SPOT
    } else {
        venue_binance::FeedCtx::USDM
    };
    let class = P::classify(route, MsgOp::Text, &input[..input.len().min(64)]);
    let mut sink = VecSink::default();
    let result = P::decode_md(data, &mut ctx, class, &input, &RX, &mut sink).unwrap();
    assert_eq!(usize::from(result.events), expected.len());
    let actual: Vec<_> = sink
        .md
        .iter()
        .map(|e| {
            let mut row = match e {
                MdRec::Bbo(b) => json!({"kind":"Bbo", "bid_px":b.bid_px.0.to_string(),
                "bid_qty":b.bid_qty.0.to_string(),"ask_px":b.ask_px.0.to_string(),
                "ask_qty":b.ask_qty.0.to_string(),"update_id":b.update_id}),
                MdRec::Trade(t) => json!({"kind":"Trade","px":t.px.0.to_string(),
                "qty":t.qty.0.to_string(),"trade_id":t.trade_id,"taker_side":format!("{:?}",t.taker_side)}),
                MdRec::Mark(m) => json!({"kind":"MarkPrice","px":m.px.0.to_string()}),
                MdRec::Index(m) => json!({"kind":"IndexPrice","px":m.px.0.to_string()}),
                MdRec::Funding(f) => json!({"kind":"Funding","rate":f.rate.0.to_string(),
                "next_funding_ns":f.next_funding_ns,"interval_s":f.interval_s}),
                other => panic!("unexpected market event: {other:?}"),
            };
            let h = e.market_hdr().unwrap();
            row["venue"] = h.venue.0.into();
            row["inst"] = h.inst.0.into();
            row["ex_ts_ns"] = h.ex_ts_ns.into();
            row["ex_send_ns"] = h.ex_send_ns.into();
            row
        })
        .collect();
    compare(&expected, &actual, &format!("{product}/{channel}/{name}"));
}

#[test]
fn bin_300_usdm_bookticker() {
    market::<Usdm>("usdm", "bookTicker", "normal");
}
#[test]
fn bin_302_usdm_aggtrade() {
    for name in ["taker_buy", "taker_sell"] {
        market::<Usdm>("usdm", "aggTrade", name);
    }
}
#[test]
fn bin_303_usdm_markprice() {
    market::<Usdm>("usdm", "markPriceUpdate", "normal");
}
#[test]
fn bin_320_spot_bookticker() {
    market::<Spot>("spot", "bookTicker", "normal");
}
#[test]
fn bin_322_spot_trade() {
    for name in ["taker_buy", "taker_sell"] {
        market::<Spot>("spot", "trade", name);
    }
}
