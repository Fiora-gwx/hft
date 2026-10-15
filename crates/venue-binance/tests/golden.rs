//! Independently calculated, sanitized demo captures. The selected wire-derived fields are
//! reviewed in the adjacent expected files; this is not a claim of complete M3 coverage.
use crate::support::{RX, RepSink, rd};
use hft_event::GwOp;
use hft_types::{AccountId, InstId};
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
    let outcome = if channel.starts_with("rest.") {
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
        P::decode_rest(rd(), &mut ctx, &origin, 200, &input, &rx, &mut sink)
    } else {
        let class = P::classify_report(MsgOp::Text, &input[..input.len().min(64)]);
        P::decode_report(rd(), &mut ctx, class, &input, &rx, &mut sink)
    }
    .unwrap_or_else(|e| panic!("{product}/{channel}/{name}: {e:?}"));
    assert!(sink.ext.is_empty(), "unexpected external fill: {sink:?}");
    assert!(sink.positions.is_empty() && sink.funding.is_empty());
    assert_eq!(
        usize::from(outcome.events),
        sink.orders.len() + sink.balances.len()
    );
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
    ] {
        report::<Spot>("spot", "executionReport", name);
    }
}

#[test]
fn bin_328_spot_user_stream_misc() {
    report::<Spot>("spot", "outboundAccountPosition", "normal");
}
