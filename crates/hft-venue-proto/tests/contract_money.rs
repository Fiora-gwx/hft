use hft_event::*;
use hft_refdata::{FundingSettleKind, RefData};
use hft_types::*;
use hft_venue_proto::{EncodeErr, Role};
use hft_venue_proto::{testkit::*, *};
use zerocopy::FromZeros;
#[path = "fake_venue.rs"]
mod fake_venue;
use fake_venue::{fake_rd, money::ReportCtx};
type P = fake_venue::Fake<false, 4>;
type FakeSpot = fake_venue::Fake<true, 4>;
#[global_allocator]
static ALLOC: hft_testkit::CountingAlloc = hft_testkit::CountingAlloc;

fn cid() -> ClientOrderId {
    ClientOrderId {
        epoch: 4,
        venue: P::VENUE,
        account: AccountId(1),
        slot: 42,
        r#gen: 7,
    }
}
fn cid_text() -> String {
    let mut b = [0; 48];
    let n = P::render_cid(&cid(), &mut b);
    String::from_utf8(b[..n].to_vec()).unwrap()
}
fn rx() -> RxMeta {
    RxMeta {
        account: AccountId(1),
        ..Default::default()
    }
}
fn order(what: OrderEventKind, qty: i64, cum: i64, detail: u8) -> OrderEvent {
    OrderEvent {
        venue: P::VENUE,
        account: AccountId(1),
        what,
        detail,
        slot: SlotId(42),
        r#gen: 7,
        inst: InstId(201),
        px: Px(100),
        qty: Qty(qty),
        cum_filled: Qty(cum),
        fee: Qty(3),
        fee_asset: AssetId(2),
        epoch: 4,
        reject_code: 0,
        exch_order_id: 99,
        ex_ts_ns: 10,
        hw_rx_ns: 0,
        rx_tsc: 0,
        avg_px: Px(if cum > 0 { 100 } else { 0 }),
        cash_qty: Qty(0),
        cash_sum: Qty(0),
        trade_id: 77,
    }
}
fn wire(kind: &str, qty: i64, cum: i64, extra: &str) -> String {
    format!(
        r#"{{"e":"{kind}","cid":"{}","i":201,"order":99,"id":77,"q":{qty},"cum":{cum},"avg":{},"px":100,"fee":3,"asset":2,"T":10{extra}}}"#,
        cid_text(),
        if cum > 0 { 100 } else { 0 }
    )
}
fn sample<'a>(name: &'a str, body: &'a [u8], expected: &'a [ReportRec]) -> ReportSample<'a> {
    ReportSample {
        name,
        op: MsgOp::Text,
        payload: body,
        rx: rx(),
        expected,
        outcome: ReportOutcome {
            events: expected.len() as u16,
            ..Default::default()
        },
    }
}

#[test]
fn vpr_084_cumulative_fills_queries_and_average() {
    let rd = fake_rd();
    let a = wire("fill", 2, 2, ",\"partial\":1");
    let b = wire("fill", 3, 5, "").replace("\"id\":77", "\"id\":78");
    let q = wire("query", 0, 5, ",\"status\":3");
    let ea = [ReportRec::Order(order(
        OrderEventKind::PartialFill,
        2,
        2,
        Side::Buy as u8,
    ))];
    let mut eb = order(OrderEventKind::Fill, 3, 5, Side::Buy as u8);
    eb.trade_id = 78;
    let eb = [ReportRec::Order(eb)];
    let eq = [ReportRec::Order(order(OrderEventKind::QueryResult, 0, 5, 3))];
    check_cum::<P>(
        &[
            sample("partial", a.as_bytes(), &ea),
            sample("complete", b.as_bytes(), &eb),
            sample("query", q.as_bytes(), &eq),
        ],
        &rd,
        &mut ReportCtx::default(),
    )
    .unwrap();
    let bad = q.replace("\"avg\":100", "\"avg\":0");
    assert!(
        check_cum::<P>(
            &[sample("missing average", bad.as_bytes(), &eq)],
            &rd,
            &mut ReportCtx::default()
        )
        .is_err()
    );
}
#[test]
fn vpr_094_actual_fees_rebates_and_late_fee() {
    let rd = fake_rd();
    for (fee, asset, deviates) in [(3, 2, false), (-3, 2, false), (3, 1, true), (0, 0, false)] {
        let b = wire("fill", 2, 2, "")
            .replace("\"fee\":3", &format!("\"fee\":{fee}"))
            .replace("\"asset\":2", &format!("\"asset\":{asset}"));
        let mut e = order(OrderEventKind::Fill, 2, 2, Side::Buy as u8);
        e.fee = Qty(fee);
        e.fee_asset = AssetId(asset);
        let want = [ReportRec::Order(e)];
        let mut s = sample("actual fee", b.as_bytes(), &want);
        s.outcome.fee_rule_deviation = deviates;
        check_fee_asset::<P>(&[s], &rd, &mut ReportCtx::default()).unwrap();
    }
    let b = wire("fee", 0, 2, "");
    let want = [ReportRec::Order(order(OrderEventKind::FeeOnly, 0, 2, 0))];
    check_fee_asset::<P>(
        &[sample("late fee", b.as_bytes(), &want)],
        &rd,
        &mut ReportCtx::default(),
    )
    .unwrap();
}
#[test]
fn vpr_097_client_identity_comes_from_wire() {
    let rd = fake_rd();
    let id = cid();
    check_cid::<P>(&[id], &[b"foreign", b"hf", b"", b"hf000"]).unwrap();
    let mut old = id;
    old.epoch = 3;
    let mut text = [0; 48];
    let n = P::render_cid(&old, &mut text);
    let b = wire("fill", 2, 2, "").replace(&cid_text(), std::str::from_utf8(&text[..n]).unwrap());
    let mut e = order(OrderEventKind::Fill, 2, 2, 1);
    e.epoch = 3;
    check_reports::<P>(
        13,
        &[sample("previous epoch", b.as_bytes(), &[ReportRec::Order(e)])],
        &rd,
        &mut ReportCtx::default(),
    )
    .unwrap();
    let b = wire("fill", 2, 2, "").replace(&format!("\"cid\":\"{}\",", cid_text()), "");
    let mut sink = VecSink::default();
    P::decode_report(&rd, &mut ReportCtx::default(), 5, b.as_bytes(), &rx(), &mut sink).unwrap();
    assert!(matches!(sink.reports[0],ReportRec::Order(e) if e.slot==SlotId::UNRESOLVED && e.epoch==0));
    assert_eq!(sink.id_calls, vec![(b"99".to_vec(), None)]);
}
#[test]
fn vpr_100_funding_capability_scope_and_no_synthesis() {
    let rd = fake_rd();
    for (mode, kind, inst) in [
        (1, FundingSettleKind::PerSymbolPush, 201),
        (2, FundingSettleKind::BalanceOnlyPush, 0),
        (3, FundingSettleKind::RateOnly, 0),
    ] {
        let b = format!(r#"{{"e":"funding","mode":{mode},"i":201,"amount":-1250000,"T":100,"disc":7}}"#);
        let mut f = FundingSettle::new_zeroed();
        f.venue = P::VENUE;
        f.account = AccountId(1);
        f.inst = InstId(inst);
        f.amount = Notional(-1250000);
        f.settle_ts_ns = 100;
        let expected = if mode == 3 {
            vec![]
        } else {
            vec![ReportRec::Funding(f)]
        };
        check_funding::<P>(
            &[sample("funding", b.as_bytes(), &expected)],
            &rd,
            &mut ReportCtx::default(),
            kind,
        )
        .unwrap();
    }
}
#[test]
fn vpr_106_external_marker_precedes_system_client_id() {
    let rd = fake_rd();
    for (marker, cid_value, cause) in [
        (1, cid_text(), ExternalCause::Liquidation),
        (2, String::new(), ExternalCause::Adl),
        (0, "web_9".into(), ExternalCause::Manual),
        (0, "zz".into(), ExternalCause::Unknown),
    ] {
        let b = wire("fill", 2, 2, &format!(",\"marker\":{marker},\"side\":2"))
            .replace(&cid_text(), &cid_value)
            .replace("\"id\":77", "\"id\":-1");
        let e = ExternalFill {
            venue: P::VENUE,
            account: AccountId(1),
            side: Side::Sell,
            cause,
            inst: InstId(201),
            qty: Qty(2),
            px: Px(100),
            fee: Qty(3),
            fee_asset: AssetId(2),
            _pad: [0; 6],
            exch_order_id: 99,
            trade_id: 0,
            ex_ts_ns: 10,
            rx_tsc: Tsc(0),
            fill_seq: 2,
            cash_qty: Qty(0),
            cash_amount: Qty(0),
        };
        check_external_fill::<P>(
            &[sample("external", b.as_bytes(), &[ReportRec::External(e)])],
            &rd,
            &mut ReportCtx::default(),
        )
        .unwrap();
    }
}
#[test]
fn vpr_110_trade_fee_funding_and_balance_dedup() {
    let rd = fake_rd();
    let b = wire("fill", 2, 2, "");
    let want = [ReportRec::Order(order(OrderEventKind::Fill, 2, 2, 1))];
    let b2 = b.replace("\"i\":201", "\"i\":202");
    let mut e = order(OrderEventKind::Fill, 2, 2, 1);
    e.inst = InstId(202);
    let want2 = [ReportRec::Order(e)];
    let fee = wire("fee", 0, 2, "");
    let fee_e = [ReportRec::Order(order(OrderEventKind::FeeOnly, 0, 2, 0))];
    check_dedup::<P>(
        &[
            sample("fill", b.as_bytes(), &want),
            sample("repeat", b.as_bytes(), &[]),
            sample("other inst", b2.as_bytes(), &want2),
            sample("fee", fee.as_bytes(), &fee_e),
            sample("repeat fee", fee.as_bytes(), &[]),
        ],
        &rd,
        &mut ReportCtx::default(),
    )
    .unwrap();
    let mut ctx = ReportCtx::default();
    for (cum, count) in [(2, 1), (2, 0), (3, 1)] {
        let b = wire("fill", 1, cum, "").replace("\"id\":77", "\"id\":-1");
        let mut e = order(OrderEventKind::Fill, 1, cum, 1);
        e.trade_id = 0;
        let expected = if count == 1 {
            vec![ReportRec::Order(e)]
        } else {
            vec![]
        };
        check_dedup::<P>(&[sample("fallback", b.as_bytes(), &expected)], &rd, &mut ctx).unwrap();
    }
    let mut ctx = ReportCtx::default();
    for (disc, count) in [(1, 1), (1, 0), (2, 1)] {
        let b = format!(r#"{{"e":"funding","mode":1,"i":201,"amount":-2,"T":100,"disc":{disc}}}"#);
        let mut f = FundingSettle::new_zeroed();
        f.venue = P::VENUE;
        f.account = AccountId(1);
        f.inst = InstId(201);
        f.amount = Notional(-2);
        f.settle_ts_ns = 100;
        let expected = if count == 1 {
            vec![ReportRec::Funding(f)]
        } else {
            vec![]
        };
        check_dedup::<P>(&[sample("funding", b.as_bytes(), &expected)], &rd, &mut ctx).unwrap();
    }
    let mut bal = BalanceReport::new_zeroed();
    bal.venue = P::VENUE;
    bal.account = AccountId(1);
    bal.asset = AssetId(2);
    let e = [ReportRec::Balance(bal)];
    let s = sample("balance", br#"{"e":"balance"}"#, &e);
    check_dedup::<P>(&[s.clone(), s], &rd, &mut ctx).unwrap();
}
#[test]
fn vpr_123_not_found_is_query_result() {
    let b = wire("query", 0, 0, ",\"status\":7");
    let e = [ReportRec::Order(order(OrderEventKind::QueryResult, 0, 0, 7))];
    check_cum::<P>(
        &[sample("NotFound", b.as_bytes(), &e)],
        &fake_rd(),
        &mut ReportCtx::default(),
    )
    .unwrap();
}
#[test]
fn vpr_124_detail_preserves_fill_side_and_query_status() {
    let rd = fake_rd();
    for (kind, what, extra, detail) in [
        ("query", OrderEventKind::QueryResult, ",\"status\":2", 2),
        ("query", OrderEventKind::QueryResult, ",\"status\":7", 7),
        ("cancel", OrderEventKind::CancelReject, ",\"code\":-2011", 1),
        ("amend", OrderEventKind::AmendReject, ",\"code\":-2011", 1),
        ("cancel", OrderEventKind::CancelReject, ",\"code\":-2010", 0),
        ("fill", OrderEventKind::Fill, ",\"side\":1", 1),
        ("fill", OrderEventKind::Fill, ",\"side\":2", 2),
    ] {
        let b = wire(kind, 2, 2, extra);
        let expected = [ReportRec::Order(order(what, 2, 2, detail))];
        check_detail::<P>(
            &[sample("detail", b.as_bytes(), &expected)],
            &rd,
            &mut ReportCtx::default(),
        )
        .unwrap();
    }
}
#[test]
fn vpr_126_rest_origin_and_transport_failures() {
    let rd = fake_rd();
    let b = wire("query", 0, 2, ",\"status\":2");
    let req = ReqRef {
        slot: 44,
        r#gen: 9,
        op: GwOp::Query,
    };
    let origin = RestOrigin {
        what: RestWhat::Order,
        req: Some(req),
        inst: InstId(201),
    };
    let mut e = order(OrderEventKind::QueryResult, 0, 2, 2);
    e.slot = SlotId(44);
    e.r#gen = 9;
    check_rest::<P>(
        &[RestSample {
            name: "query",
            origin,
            status: 200,
            body: b.as_bytes(),
            rx: rx(),
            expected: &[ReportRec::Order(e)],
            outcome: ReportOutcome {
                req: Some(req),
                events: 1,
                ..Default::default()
            },
        }],
        &rd,
        &mut ReportCtx::default(),
    )
    .unwrap();
    let second = b.replace(
        &cid_text(),
        {
            let mut id = cid();
            id.slot = 43;
            let mut out = [0; 48];
            let n = P::render_cid(&id, &mut out);
            std::str::from_utf8(&out[..n]).unwrap().to_owned()
        }
        .as_str(),
    );
    let foreign = b.replace(&cid_text(), "external");
    let sweep = format!("{b}\n{second}\n{foreign}");
    let first = order(OrderEventKind::QueryResult, 0, 2, 2);
    let mut second = first;
    second.slot = SlotId(43);
    check_rest::<P>(
        &[RestSample {
            name: "sweep",
            origin: RestOrigin {
                what: RestWhat::Sweep,
                req: None,
                inst: InstId(201),
            },
            status: 200,
            body: sweep.as_bytes(),
            rx: rx(),
            expected: &[ReportRec::Order(first), ReportRec::Order(second)],
            outcome: ReportOutcome {
                events: 2,
                ..Default::default()
            },
        }],
        &rd,
        &mut ReportCtx::default(),
    )
    .unwrap();
    for what in [RestWhat::Keepalive, RestWhat::DeadMan] {
        check_rest::<P>(
            &[RestSample {
                name: "maintenance",
                origin: RestOrigin {
                    what,
                    req: None,
                    inst: InstId(0),
                },
                status: 200,
                body: b"ok",
                rx: rx(),
                expected: &[],
                outcome: Default::default(),
            }],
            &rd,
            &mut ReportCtx::default(),
        )
        .unwrap();
    }
}
#[test]
fn vpr_128_control_hints_and_rest_rate_limit() {
    let rd = fake_rd();
    for (body, ctl) in [
        (br#"{"e":"ended"}"#.as_slice(), ReportCtl::UserStreamEnded),
        (br#"{"e":"shutdown"}"#, ReportCtl::ServerShutdown),
    ] {
        let mut s = sample("control", body, &[]);
        s.outcome.ctl = ctl;
        check_ctl::<P>(&[s], &rd, &mut ReportCtx::default()).unwrap();
    }
    for (code, ctl) in [("-1021", ReportCtl::ClockSkew), ("-1015", ReportCtl::RateLimited)] {
        let body = format!("{{\"e\":\"response\",\"code\":\"{code}\"}}");
        let mut e = order(OrderEventKind::Reject, 0, 0, 0);
        e.px = Px(0);
        e.fee = Qty(0);
        e.fee_asset = AssetId(0);
        e.exch_order_id = 0;
        e.trade_id = 0;
        e.ex_ts_ns = 0;
        e.reject_code = raw_code_str(code.as_bytes());
        let expected = [ReportRec::Order(e)];
        let mut row = sample("business reject", body.as_bytes(), &expected);
        row.outcome.req = Some(ReqRef {
            slot: 42,
            r#gen: 7,
            op: GwOp::Place,
        });
        row.outcome.raw_code = e.reject_code;
        row.outcome.ctl = ctl;
        check_ctl::<P>(&[row], &rd, &mut ReportCtx::default()).unwrap();
    }
    check_rest::<P>(
        &[RestSample {
            name: "expired keepalive",
            origin: RestOrigin {
                what: RestWhat::Keepalive,
                req: None,
                inst: InstId(0),
            },
            status: 200,
            body: b"ended",
            rx: rx(),
            expected: &[],
            outcome: ReportOutcome {
                ctl: ReportCtl::UserStreamEnded,
                ..Default::default()
            },
        }],
        &rd,
        &mut ReportCtx::default(),
    )
    .unwrap();
    let origin = RestOrigin {
        what: RestWhat::Order,
        req: Some(ReqRef {
            slot: 42,
            r#gen: 7,
            op: GwOp::Query,
        }),
        inst: InstId(201),
    };
    check_rest::<P>(
        &[RestSample {
            name: "429",
            origin,
            status: 429,
            body: b"",
            rx: rx(),
            expected: &[],
            outcome: ReportOutcome {
                ctl: ReportCtl::RateLimited,
                outcome_unknown: origin.req,
                raw_code: raw_code_http(429),
                ..Default::default()
            },
        }],
        &rd,
        &mut ReportCtx::default(),
    )
    .unwrap();
}

const CONFIG: &str = r#"{"pos":0,"margin":0,"multi":0,"discount":0,"type":0,"borrow":0,"withdraw":0,"verified":1,"uid":"U1","lev1":5000000000,"lev2":3000000000}"#;
fn config_expected(spot: bool, mismatch: u32, uid: &[u8], keys: u8) -> Vec<ReportRec> {
    let mut h = AccountConfigReportHdr::new_zeroed();
    h.venue = P::VENUE;
    h.account = AccountId(1);
    h.n_lev = if spot { 0 } else { 2 };
    h.mismatches = mismatch;
    h.uid_keys = keys;
    h.venue_uid[..uid.len()].copy_from_slice(uid);
    let tail = if spot {
        vec![]
    } else {
        vec![
            LeverageItem {
                inst: InstId(201),
                _pad: 0,
                leverage: Rate(5_000_000_000),
            },
            LeverageItem {
                inst: InstId(202),
                _pad: 0,
                leverage: Rate(3_000_000_000),
            },
        ]
    };
    vec![ReportRec::Config(h, tail)]
}
fn account_case<Px: AcctProtocol<Templates = ()>>(
    body: &[u8],
    kind: AcctQueryKind,
    expected: &[ReportRec],
    check: impl FnOnce(&AcctCase<'_>, &RefData),
) {
    let q = Px::acct_queries(kind)[0];
    let mut request = RestReqBuf::default();
    request
        .path_query
        .push(if kind == AcctQueryKind::Config {
            b"/config?signature=fixture"
        } else {
            b"/margin?signature=fixture"
        })
        .unwrap();
    request.redacted = true;
    let samples = [AcctSample {
        name: "account",
        query: q,
        status: 200,
        body,
        request: &request,
    }];
    let case = AcctCase {
        name: "account",
        kind,
        account: AccountId(1),
        samples: &samples,
        expected,
    };
    check(&case, &fake_rd());
}
#[test]
fn vpr_102_config_mismatch_bits_and_observed_leverage() {
    for (field, bit) in [
        ("pos", 1),
        ("margin", 2),
        ("multi", 4),
        ("discount", 8),
        ("type", 16),
        ("borrow", 32),
        ("withdraw", 64),
    ] {
        let b = CONFIG.replace(&format!("\"{field}\":0"), &format!("\"{field}\":1"));
        let mut expected = config_expected(false, bit, b"U1", 2);
        if let ReportRec::Config(h, _) = &mut expected[0] {
            match field {
                "pos" => h.position_mode = 1,
                "margin" => h.margin_mode = 1,
                "multi" => h.multi_asset = 1,
                _ => {}
            }
        }
        account_case::<P>(b.as_bytes(), AcctQueryKind::Config, &expected, |c, rd| {
            check_account_config::<P>(std::slice::from_ref(c), rd, &(), &Default::default()).unwrap()
        });
    }
    let b = CONFIG.replace("U1", "U2");
    let e = config_expected(false, 128, b"U2", 2);
    account_case::<P>(b.as_bytes(), AcctQueryKind::Config, &e, |c, rd| {
        check_account_config::<P>(std::slice::from_ref(c), rd, &(), &Default::default()).unwrap()
    });
    let b = CONFIG.replace(",\"lev1\":5000000000", "");
    let mut e = config_expected(false, 0, b"U1", 2);
    if let ReportRec::Config(_, t) = &mut e[0] {
        t[0].leverage = Rate(0);
    }
    account_case::<P>(b.as_bytes(), AcctQueryKind::Config, &e, |c, rd| {
        check_account_config::<P>(std::slice::from_ref(c), rd, &(), &Default::default()).unwrap()
    });
}
#[test]
fn vpr_137_uid_policy_missing_and_empty() {
    for (verified, missing) in [(false, false), (false, true), (true, false), (true, true)] {
        let mut b = CONFIG.replace(
            "\"verified\":1",
            if verified {
                "\"verified\":1"
            } else {
                "\"verified\":0"
            },
        );
        b = b.replace("\"uid\":\"U1\",", if missing { "" } else { "\"uid\":\"\"," });
        let e = config_expected(false, if verified { 256 } else { 0 }, b"", 0);
        account_case::<P>(b.as_bytes(), AcctQueryKind::Config, &e, |c, rd| {
            check_account_config::<P>(std::slice::from_ref(c), rd, &(), &Default::default()).unwrap()
        });
    }
}
#[test]
fn vpr_139_spot_cash_wallet_requires_account_type() {
    for (cash, kind, expected) in [(true, 0, 0), (true, 1, 16), (false, 0, 256)] {
        let b = CONFIG
            .replace("\"borrow\":0,", if cash { "\"cash_only\":1," } else { "" })
            .replace("\"type\":0", &format!("\"type\":{kind}"));
        let e = config_expected(true, expected, b"U1", 2);
        account_case::<FakeSpot>(b.as_bytes(), AcctQueryKind::Config, &e, |c, rd| {
            check_account_config::<FakeSpot>(std::slice::from_ref(c), rd, &(), &Default::default()).unwrap()
        });
    }
}
#[test]
fn vpr_104_margin_ratio_rounds_up_and_adl_is_normalized() {
    assert!(FakeSpot::acct_queries(AcctQueryKind::Margin).is_empty());
    for (balance, adl, max, ratio, quantile) in [
        (3, 2, 5, 333_333_334, 40),
        (4, 2, 3, 250_000_000, 66),
        (0, 0, 0, i64::MAX, 255),
        (-5, 0, 0, i64::MAX, 255),
    ] {
        let b = format!(
            r#"{{"balance":{balance},"maint":1,"adl":{adl},"max_adl":{max},"ignored_exchange_ratio":0.5}}"#
        );
        let mut h = MarginReportHdr::new_zeroed();
        h.venue = P::VENUE;
        h.account = AccountId(1);
        h.n = 1;
        h.margin_balance = Notional(balance);
        h.maint_margin = Notional(1);
        h.margin_ratio = Rate(ratio);
        let mut pos = PositionReport::new_zeroed();
        pos.venue = P::VENUE;
        pos.account = AccountId(1);
        pos.inst = InstId(201);
        let e = [
            ReportRec::Margin(
                h,
                vec![MarginItem {
                    inst: InstId(201),
                    adl_quantile: quantile,
                    _pad: [0; 3],
                    liq_px: Px(0),
                }],
            ),
            ReportRec::Position(pos),
        ];
        account_case::<P>(b.as_bytes(), AcctQueryKind::Margin, &e, |c, rd| {
            check_margin::<P>(std::slice::from_ref(c), rd, &(), &Default::default()).unwrap()
        });
    }
}
#[test]
fn vpr_120_account_and_report_contexts_are_independent() {
    let e = config_expected(false, 0, b"U1", 2);
    let b = wire("fill", 2, 2, "");
    let report = sample("interleave", b.as_bytes(), &[]);
    account_case::<P>(CONFIG.as_bytes(), AcctQueryKind::Config, &e, |c, rd| {
        check_acct_isolation::<P>(c, &[report], rd, &(), &Default::default(), ReportCtx::default).unwrap()
    });
}

fn command(op: GwOp) -> (GwCmd, TargetRef) {
    (
        GwCmd {
            decision_seq: 1,
            decided_tsc: Tsc(1),
            px: Px(100),
            qty: Qty(2),
            inst: InstId(201),
            slot: SlotId(42),
            r#gen: 7,
            target_slot: SlotId(42),
            op,
            account: AccountId(1),
            side: Side::Buy,
            tif: Tif::Gtc,
            oflags: 0,
            _pad: [0; 2],
            arg: 0,
            arg2: 0,
        },
        TargetRef {
            id: cid(),
            exch_order_id: 99,
        },
    )
}
fn req(path: &[u8], headers: &[u8], redacted: bool) -> RestReqBuf {
    let mut r = RestReqBuf::default();
    r.path_query.push(path).unwrap();
    r.headers.push(headers).unwrap();
    r.redacted = redacted;
    r
}
#[test]
fn vpr_048_golden_encoders_compare_every_wire_field() {
    let rd = fake_rd();
    let clock = WallClock::default();
    let mut buf = [0; 4096];
    for op in [GwOp::Place, GwOp::Cancel, GwOp::Amend, GwOp::CancelAll] {
        let (c, t) = command(op);
        let expected = format!("{{\"op\":{},\"cid\":\"{}\",\"time\":0}}", op as u8, cid_text());
        GoldenDriver::<P>::run_encoded("order", expected.as_bytes(), &mut buf, |b| {
            P::encode(&rd, &(), &c, &t, &clock, b)
        })
        .unwrap();
    }
    let (c, t) = command(GwOp::Place);
    let one = format!(
        "{{\"op\":{},\"cid\":\"{}\",\"time\":0}}",
        GwOp::Place as u8,
        cid_text()
    );
    let two = format!("{one}\n{one}");
    GoldenDriver::<P>::run_encoded("batch", two.as_bytes(), &mut buf, |b| {
        P::encode_batch(&rd, &(), &[c, c], &[t, t], &clock, b)
    })
    .unwrap();
    assert_eq!(
        fake_venue::FakePerp::encode_batch(&rd, &(), &[c, c], &[t, t], &clock, &mut buf),
        Err(EncodeErr::Unsupported(GwOp::Place))
    );
    let mut canonical = [0; 24];
    cid().encode(&mut canonical);
    let mut expected = b"hf".to_vec();
    expected.extend(canonical);
    GoldenDriver::<P>::run_encoded("cid", &expected, &mut buf, |b| {
        let mut text = [0; 48];
        let n = P::render_cid(&cid(), &mut text);
        b[..n].copy_from_slice(&text[..n]);
        Ok(n)
    })
    .unwrap();
    let cfg = VenueCfg {
        env: VenueEnv::Testnet,
        stale_guard_ms: 1000,
    };
    let cred = Credential::default();
    let mut login = Vec::new();
    assert_eq!(
        P::login(Role::Order, &cfg, &cred, 0, 0, &mut login),
        Ok(LoginStep::Frame)
    );
    assert_eq!(login, b"login:fixture-signature");
    let mut frames = SubFrames::with_capacity(256, 4);
    let sub = SubSpec {
        inst: InstId(201),
        stream: StreamKind::Bbo,
        levels: 0,
        route: RouteId(0),
    };
    P::subscribe(&rd, &[sub], false, &cfg, &mut frames).unwrap();
    P::private_subscribe(&cfg, AccountId(1), false, &mut frames).unwrap();
    assert_eq!(frames.get(0), Some((b"sub:md".as_slice(), 1)));
    assert_eq!(frames.get(1), Some((b"sub:private".as_slice(), 1)));
    let mut endpoint = EndpointSpec::default();
    P::endpoint(Role::Md, RouteId(0), &cfg, None, None, 0, &mut endpoint).unwrap();
    assert_eq!(endpoint.path_query.as_slice(), b"/ws?product=perp");
    assert_eq!(endpoint.headers.as_slice(), b"X-Fixture: session\r\n");
    let mut requests = Vec::new();
    let mut r = RestReqBuf::default();
    P::snapshot_request(&rd, &Default::default(), InstId(201), &mut r).unwrap();
    requests.push((r, req(b"/depth", b"", false)));
    let mut r = RestReqBuf::default();
    P::bbo_snapshot_request(&rd, &Default::default(), &[InstId(201)], &mut r).unwrap();
    requests.push((r, req(b"/bbo", b"", false)));
    let mut r = RestReqBuf::default();
    P::acct_request(
        &rd,
        &(),
        AccountId(1),
        P::acct_queries(AcctQueryKind::Config)[0],
        &clock,
        &mut r,
    )
    .unwrap();
    requests.push((r, req(b"/config?signature=fixture", b"", true)));
    let mut r = RestReqBuf::default();
    P::session_identity_request(&cfg, &cred, 0, &mut r).unwrap();
    requests.push((
        r,
        req(b"/identity", b"Authorization: fixture-signature\r\n", true),
    ));
    let (c, t) = command(GwOp::Query);
    assert_eq!(
        P::encode(&rd, &(), &c, &t, &clock, &mut buf),
        Err(EncodeErr::Unsupported(GwOp::Query))
    );
    let mut r = RestReqBuf::default();
    P::encode_rest(&rd, &(), &c, &t, &clock, &mut r).unwrap();
    requests.push((r, req(b"/order?signature=fixture", b"", true)));
    let mut r = RestReqBuf::default();
    P::session_keepalive_request(&rd, &(), &clock, &mut r).unwrap();
    requests.push((r, req(b"/keepalive", b"Api-Key: fixture\r\n", true)));
    for (kind, path) in [
        (RoKind::OpenOrders, b"/open".as_slice()),
        (RoKind::Fills, b"/fills"),
        (RoKind::FundingFlow, b"/funding"),
        (RoKind::Balance, b"/balance"),
        (RoKind::Positions, b"/positions"),
    ] {
        let mut r = RestReqBuf::default();
        P::ro_request(
            &rd,
            &(),
            AccountId(1),
            &RoQuery {
                kind,
                inst: InstId(201),
                cursor: 0,
                since_ms: 0,
                limit: 10,
            },
            &clock,
            &mut r,
        )
        .unwrap();
        requests.push((r, req(path, b"", true)));
    }
    for (got, want) in requests {
        GoldenDriver::<P>::compare_request("request", &got, &want).unwrap();
    }
    let mut d = DeadManOut::default();
    P::dead_man_refresh(&rd, &(), 60000, &[], &clock, &mut d).unwrap();
    assert_eq!(d.frame.as_slice(), b"deadman:60000");
    assert!(d.reqs.is_empty());
}
#[test]
fn vpr_080_session_secrets_redaction_and_heartbeat() {
    let rd = fake_rd();
    let cfg = VenueCfg {
        env: VenueEnv::Testnet,
        stale_guard_ms: 1000,
    };
    let clock = WallClock::default();
    let secret = [0xA5; 32];
    let cred = Credential {
        secret: SecretBuf(ArrayBuf::from_bytes(&secret).unwrap()),
        ..Default::default()
    };
    let mut login = Vec::new();
    P::login(Role::Order, &cfg, &cred, 0, 0, &mut login).unwrap();
    let mut requests = Vec::new();
    let mut r = RestReqBuf::default();
    P::session_identity_request(&cfg, &cred, 0, &mut r).unwrap();
    requests.push(r);
    let mut r = RestReqBuf::default();
    P::acct_request_signed(
        &rd,
        &(),
        &cred,
        AccountId(1),
        P::acct_queries(AcctQueryKind::Config)[0],
        &clock,
        &mut r,
    )
    .unwrap();
    requests.push(r);
    let (c, t) = command(GwOp::Query);
    let mut r = RestReqBuf::default();
    P::encode_rest_signed(&rd, &(), &cred, &c, &t, &clock, &mut r).unwrap();
    requests.push(r);
    let mut r = RestReqBuf::default();
    P::session_keepalive_request_signed(&rd, &(), &cred, &clock, &mut r).unwrap();
    requests.push(r);
    let mut r = RestReqBuf::default();
    P::snapshot_request(&rd, &Default::default(), InstId(201), &mut r).unwrap();
    let refs: Vec<_> = requests
        .iter()
        .map(|r| ("signed", r, true))
        .chain([("public", &r, false)])
        .collect();
    check_session(&refs, &[("login", &login)], &secret).unwrap();
    for role in [Role::Md, Role::Order, Role::UserStream] {
        let mut b = [0; 128];
        assert_eq!(
            P::heartbeat(role).ping == PingKind::WsPing,
            P::app_ping(role, 0, &mut b) == 0
        );
    }
    assert!(P::bootstrap(Role::Md, &cfg, None).is_none());
    for route in P::routes() {
        P::endpoint(
            Role::Md,
            route.id,
            &cfg,
            None,
            None,
            0,
            &mut EndpointSpec::default(),
        )
        .unwrap();
    }
    let leaked = req(b"/signed", &secret, true);
    assert!(check_session(&[("leak", &leaked, true)], &[], &secret).is_err());
}
fn bbo(inst: u32, id: u64) -> MdRec {
    let mut b = Bbo::new_zeroed();
    b.h.venue = P::VENUE;
    b.h.inst = InstId(inst);
    b.bid_px = Px(100_00000000);
    b.ask_px = Px(101_00000000);
    b.bid_qty = Qty(1_00000000);
    b.ask_qty = Qty(2_00000000);
    b.update_id = id;
    MdRec::Bbo(b)
}
const BBO: &[u8] = br#"{"e":"bbo","i":201,"u":7,"b":"100","B":"1","a":"101","A":"2","T":0}"#;
#[test]
fn vpr_114_refresh_batch_identity_and_capacity() {
    let rd = fake_rd();
    let two = [BBO, br#"{"i":202,"u":8,"b":"100","B":"1","a":"101","A":"2"}"#].join(&b'\n');
    let want = [bbo(201, 7), bbo(202, 8)];
    let insts = [InstId(201), InstId(202)];
    let samples = [BboSnapshotSample {
        name: "batch",
        insts: &insts,
        body: &two,
        rx: RxMeta::default(),
        expected: &want,
    }];
    check_bbo_refresh::<P>(&samples, &rd, &mut Default::default()).unwrap();
    let mut r = req(b"untouched", b"", false);
    let before = r;
    assert_eq!(
        P::bbo_snapshot_request(
            &rd,
            &Default::default(),
            &[InstId(201), InstId(202), InstId(203)],
            &mut r
        ),
        Err(ProtoErr::BufFull)
    );
    assert_eq!(r, before);
    let one = br#"{"b":"100","B":"1","a":"101","A":"2"}"#;
    check_bbo_refresh::<P>(
        &[BboSnapshotSample {
            name: "singleton",
            insts: &[InstId(202)],
            body: one,
            rx: RxMeta::default(),
            expected: &[bbo(202, 0)],
        }],
        &rd,
        &mut Default::default(),
    )
    .unwrap();
    assert_eq!(FakeSpot::bbo_probe(), PingKind::WsPing);
    assert_eq!(
        FakeSpot::bbo_snapshot_request(
            &rd,
            &Default::default(),
            &[InstId(101)],
            &mut RestReqBuf::default()
        ),
        Ok(false)
    );
    check_bbo_refresh::<FakeSpot>(&[], &rd, &mut Default::default()).unwrap();
}
#[test]
fn vpr_116_snapshot_context_isolation_and_thousand_levels() {
    let rd = fake_rd();
    let md = MdSample {
        name: "bbo",
        route: RouteId(0),
        op: MsgOp::Text,
        payload: BBO,
        rx: RxMeta::default(),
    };
    check_snapshot_isolation::<P>(
        &[BboSnapshotSample {
            name: "bbo snapshot",
            insts: &[InstId(201)],
            body: BBO,
            rx: RxMeta::default(),
            expected: &[bbo(201, 7)],
        }],
        &[md],
        &rd,
        Default::default,
    )
    .unwrap();
    let levels = (0..1000)
        .map(|i| format!("[\"{}\",\"1\"]", 2000 - i))
        .collect::<Vec<_>>()
        .join(",");
    let body = format!("{{\"e\":\"depth\",\"i\":201,\"u\":55,\"T\":0,\"b\":[{levels}],\"a\":[{levels}]}}");
    let mut warm = Default::default();
    for _ in 0..10000 {
        P::decode_md(
            &rd,
            &mut warm,
            fake_venue::FakeMd::Bbo,
            BBO,
            &RxMeta::default(),
            &mut VecSink::default(),
        )
        .unwrap();
    }
    let (mut a, mut b) = (VecSink::default(), VecSink::default());
    let info = P::decode_snapshot(
        &rd,
        &mut warm,
        InstId(201),
        body.as_bytes(),
        &RxMeta::default(),
        &mut a,
    )
    .unwrap();
    P::decode_snapshot(
        &rd,
        &mut Default::default(),
        InstId(201),
        body.as_bytes(),
        &RxMeta::default(),
        &mut b,
    )
    .unwrap();
    assert_eq!(a.md, b.md);
    assert_eq!(
        info,
        SnapshotInfo {
            inst: InstId(201),
            snap_id: 55,
            levels: 1000
        }
    );
}
#[derive(Default)]
struct FixedSink {
    calls: usize,
    foreign: bool,
}
impl MdSink for FixedSink {
    fn bbo(&mut self, _: &Bbo) {
        self.calls += 1;
    }
    fn l2_begin(&mut self, _: &L2BatchHdr) {}
    fn l2_level(&mut self, _: Side, _: Px, _: Qty) {}
    fn l2_end(&mut self) {}
    fn l2_snapshot(&mut self, _: &L2SnapshotPart<'_>) {
        self.calls += 1;
    }
    fn trade(&mut self, _: &Trade) {
        self.calls += 1;
    }
    fn mark(&mut self, _: &MarkPrice) {
        self.calls += 1;
    }
    fn index(&mut self, _: &IndexPrice) {}
    fn funding(&mut self, _: &Funding) {}
    fn liquidation(&mut self, _: &Liquidation) {}
}
impl ReportSink for FixedSink {
    fn order(&mut self, _: &OrderEvent) {
        self.calls += 1;
    }
    fn position(&mut self, p: &PositionReport) {
        self.foreign |= p.inst != InstId(201);
        self.calls += 1;
    }
    fn balance(&mut self, _: &BalanceReport) {}
    fn funding_settle(&mut self, _: &FundingSettle) {}
    fn external_fill(&mut self, _: &ExternalFill) {}
    fn margin(&mut self, _: &MarginReportHdr, t: &[MarginItem]) {
        self.foreign |= t.iter().any(|i| i.inst != InstId(201));
    }
    fn account_config(&mut self, _: &AccountConfigReportHdr, _: &[LeverageItem]) {}
    fn exch_id(&mut self, _: &[u8], _: Option<&ClientOrderId>) -> ExchIdHint {
        ExchIdHint::Unknown
    }
}
#[test]
fn vpr_074_hot_methods_and_large_account_decode_do_not_allocate() {
    let rd = fake_rd();
    let depth = fake_venue::depth20_payload(201);
    let fill = wire("fill", 2, 2, "");
    let mut payload = String::from(r#"{"balance":3,"maint":1,"adl":1,"max_adl":5,"foreign":["#);
    for i in 0..200 {
        if i != 0 {
            payload.push(',');
        }
        payload.push_str(&format!(
            "{{\"symbol\":\"UNKNOWN_{i}\",\"padding\":\"{}\"}}",
            "x".repeat(210)
        ));
    }
    payload.push_str("]}");
    payload.push_str(&" ".repeat((50 * 1024usize).saturating_sub(payload.len())));
    let mut fc = Default::default();
    let mut gc = ReportCtx::default();
    let mut ac = 0;
    let mut scratch = fake_venue::money::Scratch::default();
    let mut sink = FixedSink::default();
    let (cmd, tgt) = command(GwOp::Place);
    let mut out = [0; 2048];
    let scope = hft_testkit::CountingAlloc::scope();
    check_hot_alloc(
        || {
            for body in [BBO, depth.as_bytes()] {
                let c = P::classify(RouteId(0), MsgOp::Text, &body[..64.min(body.len())]);
                P::peek(&rd, &fc, c, body).unwrap();
                P::decode_md(&rd, &mut fc, c, body, &rx(), &mut sink).unwrap();
            }
            P::encode(&rd, &(), &cmd, &tgt, &WallClock::default(), &mut out).unwrap();
            let c = P::classify_report(MsgOp::Text, &fill.as_bytes()[..64]);
            P::decode_report(&rd, &mut gc, c, fill.as_bytes(), &rx(), &mut sink).unwrap();
            P::acct_decode(
                &rd,
                &mut ac,
                AccountId(1),
                P::acct_queries(AcctQueryKind::Margin)[0],
                200,
                payload.as_bytes(),
                &mut scratch,
            )
            .unwrap();
            P::acct_finish(
                &rd,
                &mut ac,
                AccountId(1),
                AcctQueryKind::Margin,
                &scratch,
                &mut sink,
            )
            .unwrap();
        },
        || scope.allocs(),
    )
    .unwrap();
    assert!(!sink.foreign);
    assert!(sink.calls > 0);
    assert!(
        check_hot_alloc(
            || {
                std::hint::black_box(Box::new(1));
            },
            || scope.allocs()
        )
        .is_err()
    );
}
