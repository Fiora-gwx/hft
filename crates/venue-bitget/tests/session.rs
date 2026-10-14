//! BGT-001–BGT-006, BGT-060–BGT-062, BGT-170–BGT-173, BGT-190, BGT-200, BGT-201: identity, routes,
//! endpoints, login, defaults, heartbeat and subscriptions.

use crate::support;

use hft_refdata::{
    CapFlags, DeadManCap, FeeAssetRule, FundingSettleKind, LimitBucket, LimitScope, LiqMarker, MarginData,
    MarginMode, PositionMode, TestnetKind,
};
use hft_types::{AccountId, MarketKind, VenueId};
use hft_venue_proto::{
    DeadManOut, DeadManSpec, EncodeErr, EndpointSpec, HeartbeatSpec, LoginResult, LoginStep, PingKind,
    ProtoErr, RateRule, Role, RouteId, RouteKind, StaleGuard, StreamKind, SubFrames, SubSpec, VenueProtocol,
};
use support::{BGB, F_BTC, F_ETH, S_BGB, S_BTC, WC, cfg, key, rd};
use venue_bitget::{BitgetSpot, BitgetUsdtFutures as Fut};

#[test]
fn bgt_191_unconditional_ping_survives() {
    for spec in [Fut::heartbeat(Role::Md), BitgetSpot::heartbeat(Role::Md)] {
        let mut last_ping = 0;
        let mut sent = Vec::new();
        for now in 1..=600_000u32 {
            // A busy server sends market data every 100 ms. Inbound activity cannot reset ping.
            let data_arrived = now % 100 == 0;
            if now - last_ping >= spec.interval_ms && (spec.unconditional || !data_arrived) {
                let mut frame = [0; 4];
                assert_eq!(Fut::app_ping(Role::Md, i64::from(now), &mut frame), 4);
                assert_eq!(&frame, b"ping");
                sent.push(now);
                last_ping = now; // server immediately answers pong
            }
            assert!(now - last_ping < 120_000);
        }
        assert_eq!(sent, (1..=24).map(|n| n * 25_000).collect::<Vec<_>>());
    }
}

#[test]
fn bgt_193_bbo_probe_budget() {
    for routes in [Fut::routes(), BitgetSpot::routes()] {
        let route = routes.iter().find(|r| r.kind == RouteKind::Bbo).unwrap();
        let probe_interval_ms = 1000;
        let probe_per_s = 1000 / probe_interval_ms;
        let subscriptions = u32::from(route.max_in_msgs_per_s) / 2 - probe_per_s;
        assert_eq!(subscriptions, 4);
        assert!(subscriptions + probe_per_s <= u32::from(route.max_in_msgs_per_s));
    }
}

#[test]
fn bgt_001_identity_consts() {
    assert_eq!(Fut::NAME, "bitget-usdt-futures", "BGT-001.1");
    assert_eq!(
        (Fut::VENUE, Fut::MARKET),
        (VenueId::BITGET_UTA_USDT_FUT, MarketKind::LinearPerp),
        "BGT-001.2"
    );
    assert_eq!(BitgetSpot::NAME, "bitget-spot", "BGT-001.3");
    assert_eq!(
        (BitgetSpot::VENUE, BitgetSpot::MARKET),
        (VenueId::BITGET_UTA_SPOT, MarketKind::Spot),
        "BGT-001.4"
    );
    assert_eq!(
        (Fut::ORDER_FRAME_CAP, BitgetSpot::ORDER_FRAME_CAP),
        (2048, 2048),
        "BGT-001.5"
    );
    assert_eq!(Fut::VENUE.market(), Some(Fut::MARKET), "BGT-001.6");
    assert_eq!(BitgetSpot::VENUE.market(), Some(BitgetSpot::MARKET), "BGT-001.6");
    const { assert!(!Fut::USER_STREAM && !BitgetSpot::USER_STREAM) };
}

#[test]
fn bgt_002_fut_routes() {
    let r = Fut::routes();
    let by = |n: &str| r.iter().find(|x| x.name == n).unwrap();
    let b = by("books1");
    assert_eq!(
        (b.kind, b.race, b.max_streams, b.max_in_msgs_per_s, b.topn_levels),
        (RouteKind::Bbo, true, 1000, 10, &[][..]),
        "BGT-002.1"
    );
    let t = by("public-topn");
    assert_eq!(
        (t.kind, t.race, t.topn_levels),
        (RouteKind::DepthTopN, false, &[5u8, 50][..]),
        "BGT-002.2"
    );
    let tr = by("public-trade");
    assert_eq!(
        (tr.kind, tr.race, tr.topn_levels),
        (RouteKind::Md, false, &[][..]),
        "BGT-002.3"
    );
    let tk = by("public-ticker");
    assert_eq!((tk.kind, tk.race), (RouteKind::Md, false), "BGT-002.4");
    let bk = by("public-books");
    assert_eq!((bk.kind, bk.race), (RouteKind::Md, false), "BGT-002.6");
    assert_eq!(r.len(), 5, "BGT-002.5");
    assert_eq!(
        r.iter().filter(|x| x.kind == RouteKind::Bbo).count(),
        1,
        "BGT-002.5"
    );
    assert_eq!(
        r.iter().filter(|x| x.kind == RouteKind::DepthTopN).count(),
        1,
        "BGT-002.5"
    );
    assert!(
        r.iter().all(|x| x.race == (x.kind == RouteKind::Bbo)),
        "BGT-002.5"
    );
}

#[test]
fn bgt_003_spot_routes() {
    let r = BitgetSpot::routes();
    let names: Vec<&str> = r.iter().map(|x| x.name).collect();
    assert_eq!(
        names,
        ["books1", "public-topn", "public-trade", "public-books"],
        "BGT-003.4"
    );
    assert_eq!(
        (r[0].kind, r[0].race, r[0].max_in_msgs_per_s),
        (RouteKind::Bbo, true, 10),
        "BGT-003.1"
    );
    assert_eq!(r[1].topn_levels, &[5, 50], "BGT-003.2");
    assert_eq!(r[2].kind, RouteKind::Md, "BGT-003.3");
}

fn ep<P: VenueProtocol>(role: Role, route: u8, cred: bool) -> Result<String, ProtoErr> {
    let mut out = EndpointSpec::default();
    let k = key();
    let r = P::endpoint(
        role,
        RouteId(route),
        &cfg(),
        cred.then_some(&k),
        None,
        1_790_000_000_000,
        &mut out,
    );
    assert!(out.headers.is_empty());
    r.map(|_| String::from_utf8(out.path_query.as_slice().to_vec()).unwrap())
}

#[test]
fn bgt_004_endpoint_paths() {
    assert_eq!(
        ep::<Fut>(Role::Md, 0, false).unwrap(),
        "/v3/ws/public",
        "BGT-004.1"
    );
    assert_eq!(
        ep::<Fut>(Role::Md, 1, false).unwrap(),
        "/v3/ws/public",
        "BGT-004.2"
    );
    assert_eq!(
        ep::<Fut>(Role::Md, 3, false).unwrap(),
        "/v3/ws/public",
        "BGT-004.3"
    );
    assert_eq!(
        ep::<Fut>(Role::Order, 0, true).unwrap(),
        "/v3/ws/private",
        "BGT-004.4"
    );
    assert_eq!(
        ep::<BitgetSpot>(Role::Md, 2, false).unwrap(),
        "/v3/ws/public",
        "BGT-004.5"
    );
    assert_eq!(
        ep::<BitgetSpot>(Role::Order, 0, true).unwrap(),
        "/v3/ws/private",
        "BGT-004.6"
    );
    assert_eq!(
        ep::<BitgetSpot>(Role::Md, 3, false),
        Err(ProtoErr::Unsupported),
        "spot has no ticker route"
    );
}

#[test]
fn bgt_005_no_user_stream_role() {
    for r in [
        ep::<Fut>(Role::UserStream, 0, true),
        ep::<BitgetSpot>(Role::UserStream, 0, true),
    ] {
        assert_eq!(r, Err(ProtoErr::Unsupported));
    }
    assert!(Fut::bootstrap(Role::UserStream, &cfg(), Some(&key())).is_none());
}

#[test]
fn bgt_060_login_frame() {
    for frame in [
        login::<Fut>(1_538_054_050_000),
        login::<BitgetSpot>(1_538_054_050_000),
    ] {
        assert_eq!(
            frame,
            r#"{"op":"login","args":[{"apiKey":"k","passphrase":"p","timestamp":"1538054050","sign":"QW9NDIxQTfljkfSeZydUIsfx+5D1GgkIbDzvrCplpp4="}]}"#
        );
    }
}

fn login<P: VenueProtocol>(now_ms: i64) -> String {
    let mut out = Vec::new();
    assert_eq!(
        P::login(Role::Order, &cfg(), &key(), 0, now_ms, &mut out),
        Ok(LoginStep::Frame)
    );
    String::from_utf8(out).unwrap()
}

#[test]
fn bgt_061_login_ts_and_roles() {
    assert!(
        login::<Fut>(1_790_000_000_999).contains(r#""timestamp":"1790000000""#),
        "BGT-061.1"
    );
    assert!(
        login::<Fut>(1_790_000_001_000).contains(r#""timestamp":"1790000001""#),
        "BGT-061.2"
    );
    let mut out = Vec::new();
    assert_eq!(
        Fut::login(Role::Md, &cfg(), &key(), 0, 1, &mut out),
        Ok(LoginStep::NotNeeded),
        "BGT-061.3"
    );
    assert!(out.is_empty(), "BGT-061.3");
}

#[test]
fn bgt_062_login_result() {
    let rows = [
        ("BGT-062.1", r#"{"event":"login","code":0}"#, LoginResult::Ok),
        ("BGT-062.2", r#"{"event":"login","code":"0"}"#, LoginResult::Ok),
        (
            "BGT-062.3",
            r#"{"event":"error","code":30005,"msg":"x"}"#,
            LoginResult::Rejected(30005),
        ),
        (
            "BGT-062.4",
            r#"{"event":"error","code":"30012","msg":"x"}"#,
            LoginResult::Rejected(30012),
        ),
        (
            "BGT-062.5",
            r#"{"event":"login","code":30006}"#,
            LoginResult::Rejected(30006),
        ),
        (
            "BGT-062.6",
            Box::leak(support::b1v2(1, 1).into_boxed_str()),
            LoginResult::NotMine,
        ),
        ("BGT-062.7", "pong", LoginResult::NotMine),
        ("BGT-062.8", r#"{"event":"login""#, LoginResult::NotMine),
        (
            "BGT-062.9",
            r#"{"event":"login","code":"0","msg":"","connId":"x"}"#,
            LoginResult::Ok,
        ),
    ];
    for (row, p, want) in rows {
        assert_eq!(Fut::login_result(p.as_bytes()), want, "{row}");
    }
}

#[test]
fn bgt_170_rate_rules() {
    let want = [
        RateRule {
            scope: LimitScope::Uid,
            bucket: LimitBucket::Orders,
            window_ms: 1_000,
            limit: 10,
        },
        RateRule {
            scope: LimitScope::Ip,
            bucket: LimitBucket::NewConns,
            window_ms: 300_000,
            limit: 300,
        },
        RateRule {
            scope: LimitScope::Conn,
            bucket: LimitBucket::WsInMsgs,
            window_ms: 1_000,
            limit: 10,
        },
    ];
    assert_eq!(Fut::rate_rules(), &want);
    assert_eq!(BitgetSpot::rate_rules(), &want);
}

#[test]
fn bgt_171_no_stale_guard_no_dead_man() {
    for (sg, dm) in [
        (Fut::stale_guard(), Fut::dead_man()),
        (BitgetSpot::stale_guard(), BitgetSpot::dead_man()),
    ] {
        assert_eq!((sg, dm), (StaleGuard::None, DeadManSpec::None));
    }
    let t = Fut::build_templates(&cfg(), rd(), AccountId(2), &key()).unwrap();
    let mut out = DeadManOut::default();
    assert_eq!(
        Fut::dead_man_refresh(rd(), &t, 10_000, &[F_BTC], &WC, &mut out),
        Err(EncodeErr::Unsupported(hft_event::GwOp::DeadManRefresh))
    );
}

#[test]
fn bgt_172_fut_default_caps() {
    let c = Fut::default_caps(|n| (n == "BGB").then_some(BGB));
    assert_eq!(c.venue, VenueId::BITGET_UTA_USDT_FUT);
    assert_eq!(c.market, MarketKind::LinearPerp);
    assert_eq!(c.fee_asset_rule, FeeAssetRule::Quote);
    assert!(c.reduce_only);
    assert_eq!(c.required_position_mode, PositionMode::OneWay);
    assert_eq!(c.required_margin_mode, MarginMode::Cross);
    assert_eq!(c.required_multi_asset, Some(false));
    assert_eq!(c.required_spot_borrow, None);
    assert_eq!(c.funding_settle, FundingSettleKind::RateOnly);
    assert_eq!(c.margin_data, MarginData::Poll { interval_ms: 5_000 });
    assert_eq!(c.own_liquidation_marker, LiqMarker::OrderTypeField);
    for v in [
        "burst_close_long",
        "dte_sys_adl_close_short",
        "delivery_close_long",
        "burst_buy_single",
        "dte_sys_adl_sell_in_single_side_mode",
    ] {
        assert!(c.liq_marker_arg.split('|').any(|x| x == v), "{v}");
    }
    assert!(c.ignorable_user_events.is_empty());
    assert!(!c.amend_qty_down_only);
    assert_eq!(
        c.flags,
        CapFlags(CapFlags::POST_ONLY.0 | CapFlags::IOC.0 | CapFlags::FOK.0)
    );
    assert!(
        !c.flags.contains(CapFlags::AMEND),
        "modify-order answers are receipts only"
    );
    assert_eq!(c.dms, DeadManCap::None);
    assert_eq!(
        (c.dms_min_ms, c.dms_max_ms, c.dms_timeout_ms, c.dms_refresh_ms),
        (0, 0, 0, 0)
    );
    assert_eq!(c.testnet, TestnetKind::Demo);
}

#[test]
fn bgt_173_spot_default_caps() {
    let c = BitgetSpot::default_caps(|n| (n == "BGB").then_some(BGB));
    assert_eq!(c.market, MarketKind::Spot);
    assert_eq!(
        c.fee_asset_rule,
        FeeAssetRule::ReceivedAssetUnlessDiscount { token: BGB }
    );
    assert!(!c.reduce_only);
    assert_eq!(c.required_spot_borrow, Some(false));
    assert_eq!(c.funding_settle, FundingSettleKind::NotApplicable);
    assert_eq!(c.own_liquidation_marker, LiqMarker::NotIdentifiable);
    assert!(c.liq_marker_arg.is_empty() && c.ignorable_user_events.is_empty());
    assert_eq!(
        c.flags,
        CapFlags(CapFlags::POST_ONLY.0 | CapFlags::IOC.0 | CapFlags::FOK.0)
    );
    let none = BitgetSpot::default_caps(|_| None);
    assert_eq!(
        none.fee_asset_rule,
        FeeAssetRule::ReceivedAssetUnlessDiscount {
            token: hft_types::AssetId(0)
        }
    );
}

#[test]
fn bgt_190_unconditional_app_ping() {
    let want = HeartbeatSpec {
        ping: PingKind::AppText,
        interval_ms: 25_000,
        unconditional: true,
        pong_timeout_ms: 10_000,
        idle_timeout_ms: 30_000,
    };
    for role in [Role::Md, Role::Order] {
        assert_eq!(Fut::heartbeat(role), want);
        assert_eq!(BitgetSpot::heartbeat(role), want);
        let mut b = [0u8; 16];
        let n = Fut::app_ping(role, 1, &mut b);
        assert_eq!(&b[..n], b"ping");
    }
    assert!(
        want.interval_ms <= 30_000,
        "must ping well inside the ~150 s server reset"
    );
    assert_eq!(Fut::bbo_probe(), PingKind::AppText);
    let mut small = [0u8; 3];
    assert_eq!(Fut::app_ping(Role::Md, 1, &mut small), 0);
}

fn sub(inst: hft_types::InstId, stream: StreamKind, levels: u8) -> SubSpec {
    SubSpec {
        inst,
        stream,
        levels,
        route: RouteId(0),
    }
}

#[test]
fn bgt_200_subscribe_frames() {
    let mut out = SubFrames::with_capacity(1 << 16, 16);
    let subs = [
        sub(F_BTC, StreamKind::Bbo, 0),
        sub(F_BTC, StreamKind::DepthTopN, 5),
        sub(F_BTC, StreamKind::Mark, 0),
        sub(F_BTC, StreamKind::Funding, 0),
        sub(F_BTC, StreamKind::Index, 0),
        sub(F_ETH, StreamKind::Trade, 0),
        sub(F_ETH, StreamKind::DepthTopN, 50),
        sub(F_ETH, StreamKind::DepthIncr, 0),
    ];
    Fut::subscribe(rd(), &subs, false, &cfg(), &mut out).unwrap();
    assert_eq!(out.len(), 1);
    let (f, covered) = out.get(0).unwrap();
    assert_eq!(covered, 8);
    assert_eq!(
        std::str::from_utf8(f).unwrap(),
        concat!(
            r#"{"op":"subscribe","args":[{"instType":"usdt-futures","topic":"books1","symbol":"BTCUSDT"},"#,
            r#"{"instType":"usdt-futures","topic":"books5","symbol":"BTCUSDT"},"#,
            r#"{"instType":"usdt-futures","topic":"ticker","symbol":"BTCUSDT"},"#,
            r#"{"instType":"usdt-futures","topic":"publicTrade","symbol":"ETHUSDT"},"#,
            r#"{"instType":"usdt-futures","topic":"books50","symbol":"ETHUSDT"},"#,
            r#"{"instType":"usdt-futures","topic":"books","symbol":"ETHUSDT"}]}"#
        ),
        "BGT-200.1: ticker merged, no id"
    );
    // ≤ 50 args per frame.
    let many: Vec<SubSpec> = (1..=120)
        .map(|i| sub(hft_types::InstId(i), StreamKind::Bbo, 0))
        .collect();
    out.clear();
    Fut::subscribe(rd(), &many, true, &cfg(), &mut out).unwrap();
    assert_eq!(out.len(), 3, "BGT-200.2");
    let counts: Vec<u16> = (0..3).map(|i| out.get(i).unwrap().1).collect();
    assert_eq!(counts, [50, 50, 20]);
    assert!(
        out.get(0)
            .unwrap()
            .0
            .starts_with(br#"{"op":"unsubscribe","args":["#)
    );
    for i in 0..3 {
        let v: serde_json::Value = serde_json::from_slice(out.get(i).unwrap().0).unwrap();
        assert!(v["args"].as_array().unwrap().len() <= 50);
        assert!(v.get("id").is_none());
    }
    // Refused specs write nothing.
    out.clear();
    for (row, bad, err) in [
        (
            "BGT-200.3",
            sub(F_BTC, StreamKind::DepthTopN, 15),
            ProtoErr::Unsupported,
        ),
        (
            "BGT-200.4",
            sub(F_BTC, StreamKind::Liquidation, 0),
            ProtoErr::Unsupported,
        ),
        ("BGT-200.5", sub(S_BTC, StreamKind::Bbo, 0), ProtoErr::Cfg),
    ] {
        assert_eq!(
            Fut::subscribe(rd(), &[subs[0], bad], false, &cfg(), &mut out),
            Err(err),
            "{row}"
        );
        assert!(out.is_empty(), "{row}");
    }
    assert_eq!(
        BitgetSpot::subscribe(rd(), &[sub(S_BGB, StreamKind::Mark, 0)], false, &cfg(), &mut out),
        Err(ProtoErr::Unsupported),
        "BGT-200.6: spot has no ticker"
    );
    BitgetSpot::subscribe(rd(), &[sub(S_BGB, StreamKind::Bbo, 0)], false, &cfg(), &mut out).unwrap();
    assert_eq!(
        out.get(0).unwrap().0,
        br#"{"op":"subscribe","args":[{"instType":"spot","topic":"books1","symbol":"BGBUSDT"}]}"#
    );
}

#[test]
fn bgt_201_private_subscribe() {
    assert_eq!(Fut::private_channels(), 7);
    assert_eq!(BitgetSpot::private_channels(), 5);
    for (topic, bit) in [("order", 0), ("position", 1), ("account", 2)] {
        for (event, expected) in [
            ("subscribe", Some((bit, true))),
            ("error", Some((bit, false))),
            ("unsubscribe", None),
        ] {
            let body = format!(r#"{{"event":"{event}","arg":{{"instType":"UTA","topic":"{topic}"}}}}"#);
            assert_eq!(Fut::private_ack(body.as_bytes()), expected);
        }
    }
    let mut out = SubFrames::with_capacity(4096, 4);
    Fut::private_subscribe(&cfg(), AccountId(2), false, &mut out).unwrap();
    assert_eq!(
        out.get(0).unwrap().0,
        br#"{"op":"subscribe","args":[{"instType":"UTA","topic":"order"},{"instType":"UTA","topic":"position"},{"instType":"UTA","topic":"account"}]}"#
    );
    out.clear();
    BitgetSpot::private_subscribe(&cfg(), AccountId(3), true, &mut out).unwrap();
    assert_eq!(
        out.get(0).unwrap().0,
        br#"{"op":"unsubscribe","args":[{"instType":"UTA","topic":"order"},{"instType":"UTA","topic":"account"}]}"#
    );
}

#[test]
fn bgt_063_no_secret_retained() {
    let original = key();
    let mut changed = original;
    changed.secret.0.clear();
    changed
        .secret
        .0
        .push(b"unique-secret-marker-32-bytes-long")
        .unwrap();
    changed.passphrase.clear();
    changed.passphrase.push(b"different-passphrase").unwrap();
    let t = Fut::build_templates(&cfg(), rd(), support::ACCT, &original).unwrap();
    let other = Fut::build_templates(&cfg(), rd(), support::ACCT, &changed).unwrap();
    let query = hft_venue_proto::RoQuery {
        kind: hft_venue_proto::RoKind::Fills,
        inst: support::F_BTC,
        limit: 1,
        cursor: 0,
        since_ms: support::WC.unix_ms,
    };
    let mut a = hft_venue_proto::RestReqBuf::default();
    let mut b = hft_venue_proto::RestReqBuf::default();
    Fut::ro_request_signed(
        support::rd(),
        &t,
        &original,
        support::ACCT,
        &query,
        &support::WC,
        &mut a,
    )
    .unwrap();
    Fut::ro_request_signed(
        support::rd(),
        &t,
        &changed,
        support::ACCT,
        &query,
        &support::WC,
        &mut b,
    )
    .unwrap();
    assert!(a.redacted && b.redacted);
    assert!(a.path_query != b.path_query || a.headers != b.headers);
    assert!(Fut::ro_request(support::rd(), &t, support::ACCT, &query, &support::WC, &mut a).is_err());
    assert_eq!(t, other);
    assert!(core::mem::size_of_val(&t) < 32);
    assert!(!format!("{t:?}").contains("unique-secret"));
}
