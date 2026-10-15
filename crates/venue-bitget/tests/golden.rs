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
                    vec![serde_json::json!({"control":"SubAck","ok":ok,"code":code})]
                }
                other => panic!("unexpected control: {other:?}"),
            }
        });
    }
    check::<BitgetUsdtFutures>();
    check::<BitgetSpot>();
}

fn capture(channel: &str, name: &str) -> (Vec<u8>, Vec<serde_json::Value>) {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/bitget-usdt-futures")
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
fn bgt_310_login() {
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
