//! Warm AccountProtocol margin boundaries; the frozen allocation-free acct_* path is separate.
use crate::support::*;
use hft_venue_proto::account::{AccountProtocol, Query, State};
use venue_binance::Usdm;

fn margin(equity: &str, maintenance: &str) -> State {
    let body = serde_json::json!({
        "totalMarginBalance": equity, "totalMaintMargin": maintenance,
        "availableBalance": "0", "marginRatio": "0.5", "assets": []
    });
    let mut state = State::default();
    Usdm::account_decode(
        rd(),
        ACCT,
        &Query::Balances,
        200,
        &serde_json::to_vec(&body).unwrap(),
        10,
        &mut state,
    )
    .unwrap();
    state
}

#[test]
fn bin_149_margin_ratio_boundaries() {
    for (equity, maintenance, ratio) in [
        ("0", "250", i64::MAX),
        ("-10", "250", i64::MAX),
        ("0.000001", "0", 0),
        ("250", "250", 1_000_000_000),
        ("125", "250", 2_000_000_000),
        ("10000", "250", 25_000_000),
    ] {
        let s = margin(equity, maintenance);
        let m = s.margin.unwrap();
        assert_eq!(m.margin_ratio.0, ratio);
        assert_eq!(m.ex_ts_ns, 10_000_000);
        assert!(s.balances.iter().all(|b| b.free.0 == 0 && b.locked.0 == 0));
    }
}

proptest::proptest! {
    #![proptest_config(proptest::test_runner::Config::with_cases(1000))]
    #[test]
    fn bin_151_margin_i128_reference(equity in 1i64..1_000_000_000, maintenance in 0i64..1_000_000_000) {
        let s = margin(&equity.to_string(), &maintenance.to_string());
        let want = (i128::from(maintenance) * 1_000_000_000 / i128::from(equity)).min(i128::from(i64::MAX)) as i64;
        proptest::prop_assert_eq!(s.margin.unwrap().margin_ratio.0, want);
    }
}

#[test]
fn bin_148_position_baseline_and_missing_assets() {
    let body = br#"[{"symbol":"BTCUSDT","positionSide":"BOTH","positionAmt":"-0.001","entryPrice":"65000.125","unRealizedProfit":"-1.0000001","liquidationPrice":"70000.555"}]"#;
    let mut s = margin("10000", "250");
    Usdm::account_decode(rd(), ACCT, &Query::Positions, 200, body, 12, &mut s).unwrap();
    let p = s.positions.iter().find(|p| p.inst == U_BTC).unwrap();
    assert_eq!(
        (p.qty.0, p.entry_px.0, p.upnl.0, p.liq_px.0),
        (-100_000, 6_500_012, -1_000_000, 7_000_056)
    );
    assert_eq!(s.margin_items.len(), 1);
    assert_eq!(s.margin_items[0].adl_quantile, 255);
    Usdm::account_decode(rd(), ACCT, &Query::Positions, 200, b"[]", 13, &mut s).unwrap();
    assert!(s.positions.iter().all(|p| p.qty.0 == 0));
    assert!(s.margin_items.is_empty());
    for status in [403, 429, 500, 503] {
        assert!(
            Usdm::account_decode(
                rd(),
                ACCT,
                &Query::Positions,
                status,
                body,
                14,
                &mut State::default()
            )
            .is_err()
        );
    }
}

#[test]
fn bin_213_unverifiable_only_exempts_missing_uid() {
    let settings = |uid: serde_json::Value| {
        serde_json::to_vec(&serde_json::json!({
            "canTrade": true, "dualSidePosition": false, "multiAssetsMargin": false, "uid": uid
        }))
        .unwrap()
    };
    let rd = account_rd(true);
    let mut s = State::default();
    Usdm::account_decode(
        &rd,
        ACCT,
        &Query::Settings,
        200,
        &settings(serde_json::Value::Null),
        1,
        &mut s,
    )
    .unwrap();
    assert_eq!(
        (s.config.unwrap().venue_uid, s.config.unwrap().uid_keys),
        ([0; 24], 0)
    );
    for uid in [serde_json::json!(12345), serde_json::json!("acct-0001")] {
        Usdm::account_decode(&rd, ACCT, &Query::Settings, 200, &settings(uid), 1, &mut s).unwrap();
        assert_ne!(s.config.unwrap().venue_uid, [0; 24]);
        assert_ne!(s.config.unwrap().uid_keys, 0);
    }
    Usdm::account_decode(
        &rd,
        ACCT,
        &Query::Settings,
        200,
        &settings(serde_json::json!("acct-0001")),
        1,
        &mut s,
    )
    .unwrap();
    assert!(
        Usdm::account_decode(
            &rd,
            ACCT,
            &Query::Permissions,
            200,
            &settings(serde_json::json!("acct-0002")),
            1,
            &mut s
        )
        .is_err()
    );
    let verified = account_rd(false);
    Usdm::account_decode(
        &verified,
        ACCT,
        &Query::Settings,
        200,
        &settings(serde_json::json!("acct-0001")),
        1,
        &mut s,
    )
    .unwrap();
    for uid in [
        serde_json::Value::Null,
        serde_json::json!("wrong"),
        serde_json::json!("1234567890123456789012345"),
    ] {
        assert!(
            Usdm::account_decode(&verified, ACCT, &Query::Settings, 200, &settings(uid), 1, &mut s).is_err()
        );
    }
    let mut bad: serde_json::Value = serde_json::from_slice(&settings(serde_json::Value::Null)).unwrap();
    bad["multiAssetsMargin"] = true.into();
    assert!(
        Usdm::account_decode(
            &rd,
            ACCT,
            &Query::Settings,
            200,
            &serde_json::to_vec(&bad).unwrap(),
            1,
            &mut s
        )
        .is_err()
    );
}

#[test]
fn bin_149_negative_maintenance_is_not_a_safe_ratio() {
    let mut s = State::default();
    let body =
        br#"{"totalMarginBalance":"100","totalMaintMargin":"-1","availableBalance":"100","assets":[]}"#;
    assert!(Usdm::account_decode(rd(), ACCT, &Query::Balances, 200, body, 1, &mut s).is_err());
    assert!(s.margin.is_none());
}
