//! Queries on a loaded snapshot (REF-050, REF-051, REF-053 – REF-057).

#![allow(
    clippy::disallowed_types,
    clippy::disallowed_macros,
    clippy::disallowed_methods
)] // allow-list: test code, not a hot path

mod common;

use common::*;
use hft_refdata::{
    AccountKind, AccountSpec, AccountVenueSpec, CapFlags, DeadManCap, EndpointEnv, EndpointRole, Exchange,
    FundingSettleKind, InstKind, InstStatus, LimitBucket, LimitScope, MaintKind, RateLimit, SimSpec,
    StaleGuardKind, TsUnit, UidPolicy,
};
use hft_types::{AccountId, AssetId, InstId, Qty, StrategyId, VenueId};

#[test]
fn ref_050_inst_lookup() {
    let rd = ok(&f0());
    let h = rd.inst(InstId(1));
    assert_eq!(
        (h.venue, h.status, h.kind, h.flags),
        (VenueId(1), InstStatus::Trading, InstKind::Perp, 0),
        "REF-050.1"
    );
    assert_eq!(
        (h.px_exp, h.qty_exp, h.px_wire_exp, h.qty_wire_exp),
        (-8, -8, -8, -8),
        "REF-050.1"
    );
    assert_eq!(
        (h.asset, h.quote, h.book_window),
        (AssetId(1), AssetId(2), 4096),
        "REF-050.1"
    );
    assert_eq!(
        (h.tick, h.lot, h.min_qty, h.max_qty),
        (10_000_000, 100_000, Qty(100_000), Qty(100_000_000_000)),
        "REF-050.1"
    );
    assert_eq!(rd.inst(InstId(0)).status, InstStatus::Invalid, "REF-050.2");
    assert_eq!(rd.inst(InstId(0)).status as u8, 0, "REF-050.2");
    assert_eq!(rd.inst(InstId(6)).status, InstStatus::Invalid, "REF-050.3");
    assert_eq!(rd.inst(InstId(u32::MAX)).status, InstStatus::Invalid, "REF-050.4");
    let m = rd.inst_meta(InstId(5));
    assert_eq!(
        (&*m.symbol, m.px_unit_shift, m.qty_unit_shift),
        ("1000PEPEUSDT", 3, 3),
        "REF-050.5"
    );
    assert_eq!(
        (&*m.tick_str, &*m.lot_str, &*m.min_qty_str),
        ("0.0000001", "1", "1"),
        "REF-050.5"
    );
    assert_eq!(
        (
            m.funding_interval_s,
            m.funding_anchor_s,
            m.max_leverage,
            m.delivery_utc_ns
        ),
        (28_800, 0, 125, 0),
        "REF-050.5"
    );
}

#[test]
fn ref_051_caps_account_asset_venue() {
    let rd = ok(&f0());
    let c = rd.caps(VenueId(1));
    assert_eq!(c, &perp_caps(VenueId(1)), "REF-051.1");
    assert_eq!(c.flags.0, 7, "REF-051.1");
    assert_eq!(c.funding_settle, FundingSettleKind::PerSymbolPush, "REF-051.1");
    assert_eq!(
        &*c.ignorable_user_events,
        &["TRADE_LITE".into()] as &[Box<str>],
        "REF-051.1"
    );
    assert_eq!(&*c.liq_marker_arg, "autoclose-", "REF-051.1");
    for (case, v) in [("REF-051.2", 3u8), ("REF-051.3", 0)] {
        let n = rd.caps(VenueId(v));
        assert_eq!((n.flags, n.dms), (CapFlags(0), DeadManCap::None), "{case}");
        assert_eq!(
            (n.dms_min_ms, n.dms_max_ms, n.dms_timeout_ms, n.dms_refresh_ms),
            (0, 0, 0, 0),
            "{case}"
        );
    }
    let a = rd.account(AccountId(1));
    assert_eq!(a.kind, AccountKind::Live, "REF-051.4");
    assert_eq!(
        (a.bound_instance, a.is_prod, a.venues),
        (StrategyId(1), true, (1 << 1) | (1 << 6)),
        "REF-051.4"
    );
    assert_eq!(
        (&*a.label, a.uid_policy),
        ("acct-a", UidPolicy::Verified),
        "REF-051.4"
    );
    for (case, id) in [("REF-051.5", 0u8), ("REF-051.6", 3)] {
        let n = rd.account(AccountId(id));
        assert_eq!(
            (n.bound_instance, n.venues, n.is_prod),
            (StrategyId(0), 0, false),
            "{case}"
        );
    }
    let p = rd.asset(AssetId(5));
    assert_eq!(
        (&*p.name, p.class),
        ("PEPE", hft_refdata::AssetClass::Crypto),
        "REF-051.7"
    );
    assert_eq!(
        (p.exp.px_exp, p.exp.qty_exp, p.px_max.0),
        (-12, 0, 1_000_000_000_000),
        "REF-051.7"
    );
    let v = rd.venue(VenueId(6));
    assert_eq!(
        (v.exchange, v.product, v.stale_guard),
        (Exchange::Okx, InstKind::Spot, StaleGuardKind::ExpireAt),
        "REF-051.8"
    );
    assert_eq!(
        (v.client_id_max_len, v.ws_max_conn_age_s, v.ts_unit),
        (32, 86_400, TsUnit::Ms),
        "REF-051.8"
    );
}

#[test]
fn ref_053_uid_instance_binding() {
    let rd = ok(&f0());
    assert_eq!(rd.venue_uid(AccountId(1), VenueId(1)), Some("1001"), "REF-053.1");
    assert_eq!(rd.venue_uid(AccountId(1), VenueId(6)), Some("2001"), "REF-053.2");
    assert_eq!(rd.venue_uid(AccountId(1), VenueId(5)), None, "REF-053.3");
    assert_eq!(rd.venue_uid(AccountId(2), VenueId(1)), None, "REF-053.4");
    let i = rd.instance(StrategyId(1)).expect("REF-053.5");
    assert_eq!((i.kind_id, &*i.name), (0xFF00, "pc-main"), "REF-053.5");
    assert!(rd.instance(StrategyId(9)).is_none(), "REF-053.6");
    assert_eq!(rd.account_of(StrategyId(1)), Some(AccountId(1)), "REF-053.7");
    assert_eq!(rd.account_of(StrategyId(2)), Some(AccountId(2)), "REF-053.7");
    assert_eq!(rd.account_of(StrategyId(3)), None, "REF-053.8");
}

#[test]
fn ref_054_lists() {
    let rd = ok(&f0());
    let ids = |v: &[InstId]| v.iter().map(|i| i.0).collect::<Vec<_>>();
    assert_eq!(ids(rd.insts_of_asset(BTC)), vec![1, 2, 3, 4], "REF-054.1");
    assert!(rd.insts_of_asset(ETH).is_empty(), "REF-054.2");
    assert_eq!(ids(rd.insts_of_venue(VenueId(1))), vec![1, 5], "REF-054.3");
    assert!(rd.insts_of_venue(VenueId(3)).is_empty(), "REF-054.4");
    let want = [
        RateLimit {
            scope: LimitScope::Account,
            bucket: LimitBucket::Orders,
            limit: 300,
            window_ms: 10_000,
            default_cost: 1,
        },
        RateLimit {
            scope: LimitScope::Ip,
            bucket: LimitBucket::Weight,
            limit: 2_400,
            window_ms: 60_000,
            default_cost: 1,
        },
    ];
    assert_eq!(rd.rate_limits(VenueId(1)), &want, "REF-054.5");
    assert!(rd.rate_limits(VenueId(2)).is_empty(), "REF-054.6");
    let roles = |env| {
        rd.endpoints(VenueId(1), env)
            .iter()
            .map(|e| e.role)
            .collect::<Vec<_>>()
    };
    assert_eq!(
        roles(EndpointEnv::Prod),
        vec![EndpointRole::OrderWs, EndpointRole::MdPublic],
        "REF-054.7"
    );
    assert_eq!(
        roles(EndpointEnv::Testnet),
        vec![EndpointRole::MdPublic],
        "REF-054.8"
    );
    assert!(roles(EndpointEnv::Demo).is_empty(), "REF-054.8");
}

#[test]
fn ref_055_maintenance() {
    let mut s = f0_spec();
    let w1 = window(V6, 0, 1_000, 2_000, MaintKind::VenueMaintenance);
    let w2 = window(V6, 4, 1_500, 2_500, MaintKind::Delisting);
    let w3 = window(V6, 4, 1_600, 3_000, MaintKind::MarketClosed);
    s.maint = vec![w1, w2, w3];
    let rd = ok(&s.build());
    let cases = [
        ("REF-055.1", 4, 999, None),
        ("REF-055.2", 4, 1_000, Some(w1)),
        ("REF-055.3", 4, 1_499, Some(w1)),
        ("REF-055.4", 4, 1_500, Some(w2)),
        ("REF-055.5", 4, 1_700, Some(w2)),
        ("REF-055.6", 4, 2_500, Some(w3)),
        ("REF-055.7", 4, 2_999, Some(w3)),
        ("REF-055.8", 4, 3_000, None),
        ("REF-055.9", 1, 1_500, None),
    ];
    for (case, inst, t, want) in cases {
        assert_eq!(rd.maintenance(InstId(inst), t).copied(), want, "{case}");
    }
}

#[test]
fn ref_056_token() {
    let mut s = f0_spec();
    s.add_venue(venue(
        VenueId::CHAIN_BSC,
        Exchange::Chain,
        InstKind::Spot,
        StaleGuardKind::None,
        0,
    ));
    let tk = usdt_bsc_token();
    s.tokens.push(tk);
    let rd = ok(&s.build());
    let got = rd.token(VenueId::CHAIN_BSC, USDT).expect("REF-056.1");
    assert_eq!(
        (got.chain, got.asset, got.decimals),
        (VenueId::CHAIN_BSC, USDT, 18),
        "REF-056.1"
    );
    assert_eq!(got.address, tk.address, "REF-056.1");
    assert!(rd.token(VenueId::CHAIN_BSC, BTC).is_none(), "REF-056.2");
    assert!(rd.token(VenueId(2), USDT).is_none(), "REF-056.3");
}

#[test]
fn ref_057_venue_account_ix() {
    let mut s = f0_spec();
    let acc = |id: u8, sim: bool, label: &str| AccountSpec {
        id: AccountId(id),
        sim,
        is_prod: false,
        venues: vbits(&[6]),
        label: label.into(),
        ..AccountSpec::default()
    };
    s.accounts.push(acc(3, false, "acct-3"));
    s.accounts.push(acc(4, true, "sim-4"));
    s.accounts.push(acc(5, false, "acct-5"));
    s.account_venues.push(AccountVenueSpec {
        account: AccountId(3),
        venue: V6,
        uid: "2003".into(),
    });
    s.account_venues.push(AccountVenueSpec {
        account: AccountId(5),
        venue: V6,
        uid: "2005".into(),
    });
    s.sims.push(SimSpec {
        account: AccountId(4),
        fee_tier: 1,
        latency_model: 0,
        margin_model: 0,
    });
    s.sim_balances.push(sim_usdt(4, V6));
    let rd = ok(&s.build());
    let ix = |v: u8, a: u8| rd.venue_account_ix(VenueId(v), AccountId(a));
    assert_eq!(ix(6, 1), Some(0), "REF-057.1");
    assert_eq!(ix(6, 3), Some(1), "REF-057.2");
    assert_eq!(ix(6, 5), Some(2), "REF-057.3");
    assert_eq!((ix(6, 4), ix(6, 2)), (None, None), "REF-057.4");
    assert_eq!(ix(1, 3), None, "REF-057.5");
    assert_eq!(ix(1, 1), Some(0), "REF-057.6");
}
