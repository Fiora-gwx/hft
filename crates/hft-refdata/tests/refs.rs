//! Load step 4: references, stable quotes, single binding per instance (REF-017 – REF-019).

#![allow(
    clippy::disallowed_types,
    clippy::disallowed_macros,
    clippy::disallowed_methods
)] // allow-list: test code, not a hot path

mod common;

use common::*;
use hft_refdata::LoadErrCode::{DanglingRef, InstanceMultiBound, QuoteNotStable};
use hft_refdata::{
    AccountVenueSpec, AssetClass, ChainToken, Endpoint, EndpointEnv, EndpointRole, Exchange, FeeAssetRule,
    FeeOverrideSpec, InstKind, LimitBucket, LimitScope, MaintKind, RateLimit, SimSpec, StaleGuardKind,
};
use hft_types::{AccountId, AssetId, InstId, StrategyId, VenueId};

const NOWHERE: VenueId = VenueId::GATE_USDT_FUT; // legal VenueId, absent from F0's Venues

fn chain_bsc(s: &mut Spec) {
    s.add_venue(venue(
        VenueId::CHAIN_BSC,
        Exchange::Chain,
        InstKind::Spot,
        StaleGuardKind::None,
        0,
    ));
}

#[test]
fn ref_017_dangling_refs() {
    let check = |case: &str, f: &dyn Fn(&mut Spec), want: Option<(u16, u32)>| {
        let mut s = f0_spec();
        f(&mut s);
        let got = outcome(load(&s.build()));
        let want: Vec<_> = want
            .map(|(sec, row)| e(DanglingRef, sec, row))
            .into_iter()
            .collect();
        assert_eq!(got, want, "{case}");
    };
    let ov = |a: u8, v: VenueId| FeeOverrideSpec {
        account: AccountId(a),
        venue: v,
        inst: InstId(0),
        fee: fee(1, 1),
    };
    check("REF-017.1", &|s| s.inst_mut(1).venue = NOWHERE, Some((3, 0)));
    check("REF-017.2", &|s| s.inst_mut(1).asset = AssetId(9), Some((3, 0)));
    check("REF-017.3", &|s| s.inst_mut(1).quote = AssetId(9), Some((3, 0)));
    check("REF-017.4", &|s| s.fee_overrides.push(ov(9, V1)), Some((6, 0)));
    check(
        "REF-017.5",
        &|s| s.fee_overrides.push(ov(1, NOWHERE)),
        Some((6, 0)),
    );
    check(
        "REF-017.6",
        &|s| s.maint.push(window(V6, 99, 1, 2, MaintKind::Delisting)),
        Some((9, 0)),
    );
    check(
        "REF-017.7",
        &|s| {
            s.maint
                .push(window(NOWHERE, 0, 1, 2, MaintKind::VenueMaintenance))
        },
        Some((9, 0)),
    );
    check(
        "REF-017.8",
        &|s| {
            let ep = Endpoint {
                role: EndpointRole::Rest,
                env: EndpointEnv::Prod,
                url: "https://x".into(),
                conn_group: 0,
            };
            s.endpoints.push((NOWHERE, ep));
        },
        Some((8, 3)),
    );
    check(
        "REF-017.9",
        &|s| {
            let l = RateLimit {
                scope: LimitScope::Ip,
                bucket: LimitBucket::Weight,
                limit: 1,
                window_ms: 1,
                default_cost: 1,
            };
            s.limits.push((NOWHERE, l));
        },
        Some((7, 2)),
    );
    check(
        "REF-017.10",
        &|s| {
            chain_bsc(s);
            s.tokens.push(ChainToken {
                asset: AssetId(9),
                ..usdt_bsc_token()
            });
        },
        Some((10, 0)),
    );
    check(
        "REF-017.11",
        &|s| {
            s.tokens.push(ChainToken {
                chain: V2,
                ..usdt_bsc_token()
            })
        },
        Some((10, 0)),
    );
    check(
        "REF-017.12",
        &|s| {
            chain_bsc(s);
            s.tokens.push(usdt_bsc_token());
        },
        None,
    );
    check("REF-017.13", &|s| s.aliases[0].inst = InstId(99), Some((11, 0)));
    check("REF-017.14", &|s| s.aliases[0].inst = InstId(1), Some((11, 0)));
    check(
        "REF-017.15",
        &|s| s.add_caps(perp_caps(VenueId::BITGET_UTA_USDT_FUT)),
        Some((13, 4)),
    );
    check(
        "REF-017.16",
        &|s| s.caps[0].fee_asset_rule = FeeAssetRule::Configured(AssetId(9)),
        Some((13, 0)),
    );
    check(
        "REF-017.17",
        &|s| s.caps[1].fee_asset_rule = FeeAssetRule::ReceivedAssetUnlessDiscount { token: AssetId(9) },
        Some((13, 1)),
    );
    check(
        "REF-017.18",
        &|s| s.account_mut(1).bound_instance = StrategyId(9),
        Some((4, 0)),
    );
    let mut s = f0_spec();
    s.account_mut(1).bound_instance = StrategyId(0);
    let rd = ok(&s.build());
    assert_eq!(rd.account_of(StrategyId(1)), None, "REF-017.19");
    check(
        "REF-017.20",
        &|s| {
            s.account_venues.push(AccountVenueSpec {
                account: AccountId(9),
                venue: V1,
                uid: "x".into(),
            })
        },
        Some((14, 2)),
    );
    check(
        "REF-017.21",
        &|s| {
            s.account_venues.push(AccountVenueSpec {
                account: AccountId(1),
                venue: NOWHERE,
                uid: "x".into(),
            })
        },
        Some((14, 1)),
    );
    check(
        "REF-017.22",
        &|s| {
            s.sims[0] = SimSpec {
                account: AccountId(9),
                ..s.sims[0]
            }
        },
        Some((16, 0)),
    );
    check(
        "REF-017.23",
        &|s| s.sim_balances[0].asset = AssetId(9),
        Some((17, 0)),
    );
    check(
        "REF-017.24",
        &|s| s.account_mut(1).venues |= vbits(&[3]),
        Some((4, 0)),
    );
}

#[test]
fn ref_018_quote_stable() {
    let mut s = f0_spec();
    s.inst_mut(2).quote = BTC;
    assert_eq!(errs(&s.build()), vec![e(QuoteNotStable, 3, 1)], "REF-018.1");
    let mut s = f0_spec();
    s.inst_mut(2).quote = USDC;
    ok(&s.build()); // REF-018.2
    let mut s = f0_spec();
    s.asset_mut(2).class = AssetClass::Equity;
    let want: Vec<_> = (0..5).map(|r| e(QuoteNotStable, 3, r)).collect();
    assert_eq!(errs(&s.build()), want, "REF-018.3");
}

#[test]
fn ref_019_instance_multi_bound() {
    let mut s = f0_spec();
    s.account_mut(2).bound_instance = StrategyId(1);
    assert_eq!(errs(&s.build()), vec![e(InstanceMultiBound, 4, 1)], "REF-019.1");
    let mut s = f0_spec();
    s.account_mut(1).bound_instance = StrategyId(0);
    s.account_mut(2).bound_instance = StrategyId(0);
    ok(&s.build()); // REF-019.2
}
