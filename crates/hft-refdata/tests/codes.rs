//! Code tables: values and builder → load round trips (REF-075); `LoadErrCode` values (REF-076).

mod common;

use common::*;
use hft_refdata::blob::{InstRow, sec};
use hft_refdata::{
    AccountKind, AssetClass, CapFlags, DeadManCap, Endpoint, EndpointEnv, EndpointRole, Exchange,
    FeeAssetRule, FundingSettleKind, InstHot, InstKind, InstStatus, LimitBucket, LimitScope, LiqMarker,
    LoadErrCode, MaintKind, MarginData, MarginMode, MultiAsset, PositionMode, RateLimit, RefData,
    StaleGuardKind, TestnetKind, TsUnit, UidPolicy, VenueCaps, opt_bool_code,
};
use hft_types::{AccountId, AssetId, InstId, VenueId};

fn load_spec(f: impl FnOnce(&mut Spec)) -> RefData {
    let mut s = f0_spec();
    f(&mut s);
    ok(&s.build())
}

fn caps_rt(row: usize, f: impl FnOnce(&mut VenueCaps)) -> VenueCaps {
    let mut s = f0_spec();
    f(&mut s.caps[row]);
    let v = s.caps[row].venue;
    let want = s.caps[row].clone();
    let got = ok(&s.build()).caps(v).clone();
    assert_eq!(got, want);
    got
}

#[test]
fn ref_075_code_tables() {
    let codes = |all: &[u8]| all.to_vec();
    // 075.1 InstStatus
    assert_eq!(
        codes(&InstStatus::ALL.iter().map(|x| *x as u8).collect::<Vec<_>>()),
        vec![0, 1, 2, 3, 4, 5, 6],
        "REF-075.1"
    );
    for st in &InstStatus::ALL[1..] {
        assert_eq!(
            load_spec(|s| s.inst_mut(3).status = *st).inst(InstId(3)).status,
            *st,
            "REF-075.1 {st:?}"
        );
    }
    // 075.2 InstKind (Delivery never loads: checked on the encoded row)
    assert_eq!(
        InstKind::ALL.iter().map(|x| *x as u8).collect::<Vec<_>>(),
        vec![1, 2, 3],
        "REF-075.2"
    );
    assert_eq!(ok(&f0()).inst(InstId(2)).kind, InstKind::Spot, "REF-075.2");
    assert_eq!(ok(&f0()).inst(InstId(1)).kind, InstKind::Perp, "REF-075.2");
    let mut s = f0_spec();
    s.inst_mut(1).kind = InstKind::Delivery;
    let row: InstRow = get_row(&raw(&s.build()), sec::INSTS, 0);
    assert_eq!(row.kind, 3, "REF-075.2");
    // 075.3 MaintKind
    assert_eq!(
        MaintKind::ALL.iter().map(|x| *x as u8).collect::<Vec<_>>(),
        vec![1, 2, 3],
        "REF-075.3"
    );
    for k in MaintKind::ALL {
        let rd = load_spec(|s| s.maint.push(window(V6, 0, 0, 10, *k)));
        assert_eq!(
            rd.maintenance(InstId(4), 5).map(|w| w.kind),
            Some(*k),
            "REF-075.3"
        );
    }
    // 075.4 AssetClass
    assert_eq!(
        AssetClass::ALL.iter().map(|x| *x as u8).collect::<Vec<_>>(),
        vec![1, 2, 3, 4],
        "REF-075.4"
    );
    for c in AssetClass::ALL {
        assert_eq!(
            load_spec(|s| s.asset_mut(3).class = *c).asset(ETH).class,
            *c,
            "REF-075.4"
        );
    }
    // 075.5 / 075.6 LimitScope, LimitBucket
    assert_eq!(
        LimitScope::ALL.iter().map(|x| *x as u8).collect::<Vec<_>>(),
        vec![1, 2, 3, 4, 5],
        "REF-075.5"
    );
    assert_eq!(
        LimitBucket::ALL.iter().map(|x| *x as u8).collect::<Vec<_>>(),
        vec![1, 2, 3, 4, 5],
        "REF-075.6"
    );
    for (scope, bucket) in LimitScope::ALL.iter().zip(LimitBucket::ALL) {
        let l = RateLimit {
            scope: *scope,
            bucket: *bucket,
            limit: 1,
            window_ms: 2,
            default_cost: 3,
        };
        assert_eq!(
            load_spec(|s| s.limits = vec![(V2, l)]).rate_limits(V2),
            &[l],
            "REF-075.5/6"
        );
    }
    // 075.7 / 075.8 EndpointRole, EndpointEnv, TsUnit
    assert_eq!(
        EndpointRole::ALL.iter().map(|x| *x as u8).collect::<Vec<_>>(),
        vec![1, 2, 3, 4, 5, 6],
        "REF-075.7"
    );
    assert_eq!(
        EndpointEnv::ALL.iter().map(|x| *x as u8).collect::<Vec<_>>(),
        vec![1, 2, 3],
        "REF-075.8"
    );
    assert_eq!(
        TsUnit::ALL.iter().map(|x| *x as u8).collect::<Vec<_>>(),
        vec![1, 2, 3],
        "REF-075.8"
    );
    for role in EndpointRole::ALL {
        for env in EndpointEnv::ALL {
            let ep = Endpoint {
                role: *role,
                env: *env,
                url: "x".into(),
                conn_group: 1,
            };
            let rd = load_spec(|s| s.endpoints = vec![(V2, ep.clone())]);
            assert_eq!(rd.endpoints(V2, *env), &[ep], "REF-075.7/8");
        }
    }
    for t in TsUnit::ALL {
        assert_eq!(
            load_spec(|s| s.venue_mut(2).ts_unit = *t).venue(V2).ts_unit,
            *t,
            "REF-075.8"
        );
    }
    // 075.9 StaleGuardKind
    assert_eq!(
        StaleGuardKind::ALL.iter().map(|x| *x as u8).collect::<Vec<_>>(),
        vec![1, 2, 3],
        "REF-075.9"
    );
    for g in StaleGuardKind::ALL {
        assert_eq!(
            load_spec(|s| s.venue_mut(2).stale_guard = *g)
                .venue(V2)
                .stale_guard,
            *g,
            "REF-075.9"
        );
    }
    // 075.10 Exchange (on a venue without accounts or insts)
    assert_eq!(
        Exchange::ALL.iter().map(|x| *x as u8).collect::<Vec<_>>(),
        vec![1, 2, 3, 4, 5, 6, 7, 32],
        "REF-075.10"
    );
    for x in Exchange::ALL {
        let rd = load_spec(|s| {
            s.add_venue(venue(
                VenueId::GATE_USDT_FUT,
                *x,
                InstKind::Perp,
                StaleGuardKind::None,
                28,
            ))
        });
        assert_eq!(rd.venue(VenueId::GATE_USDT_FUT).exchange, *x, "REF-075.10");
    }
    // 075.11 UidPolicy, AccountKind
    assert_eq!(
        (UidPolicy::Verified as u8, UidPolicy::Unverifiable as u8),
        (1, 2),
        "REF-075.11"
    );
    let rd = ok(&f0());
    assert_eq!(
        (AccountKind::Live.code(), rd.account(AccountId(1)).kind.code()),
        (1, 1),
        "REF-075.11"
    );
    assert_eq!(rd.account(AccountId(2)).kind.code(), 2, "REF-075.11");
    let rd = load_spec(|s| {
        s.account_mut(1).is_prod = false;
        s.account_mut(1).uid_policy = UidPolicy::Unverifiable;
    });
    assert_eq!(
        rd.account(AccountId(1)).uid_policy,
        UidPolicy::Unverifiable,
        "REF-075.11"
    );
    // 075.12 FundingSettleKind
    use FundingSettleKind as F;
    assert_eq!(
        F::ALL.iter().map(|x| *x as u8).collect::<Vec<_>>(),
        vec![1, 2, 3, 4],
        "REF-075.12"
    );
    for f in [F::PerSymbolPush, F::BalanceOnlyPush, F::RateOnly] {
        caps_rt(0, |c| c.funding_settle = f);
    }
    caps_rt(1, |c| c.funding_settle = F::NotApplicable);
    // 075.13 MarginData
    let md = [
        MarginData::Push,
        MarginData::Poll { interval_ms: 1_000 },
        MarginData::NotApplicable,
    ];
    assert_eq!(
        md.iter().map(|m| m.code()).collect::<Vec<_>>(),
        vec![1, 2, 3],
        "REF-075.13"
    );
    caps_rt(0, |c| c.margin_data = md[0]);
    caps_rt(0, |c| c.margin_data = md[1]);
    caps_rt(1, |c| c.margin_data = md[2]);
    // 075.14 PositionMode, MarginMode, MultiAsset
    assert_eq!(
        PositionMode::ALL.iter().map(|x| *x as u8).collect::<Vec<_>>(),
        vec![1, 2, 3],
        "REF-075.14"
    );
    assert_eq!(
        MarginMode::ALL.iter().map(|x| *x as u8).collect::<Vec<_>>(),
        vec![1, 2, 3, 4],
        "REF-075.14"
    );
    assert_eq!(
        MultiAsset::ALL.iter().map(|x| *x as u8).collect::<Vec<_>>(),
        vec![1, 2, 3],
        "REF-075.14"
    );
    for p in PositionMode::ALL {
        caps_rt(0, |c| c.required_position_mode = *p);
    }
    for m in [MarginMode::NotApplicable, MarginMode::Cross, MarginMode::Isolated] {
        caps_rt(0, |c| c.required_margin_mode = m);
    }
    // 075.15 – 075.17
    assert_eq!(
        LiqMarker::ALL.iter().map(|x| *x as u8).collect::<Vec<_>>(),
        vec![1, 2, 3, 4],
        "REF-075.15"
    );
    for l in LiqMarker::ALL {
        caps_rt(0, |c| c.own_liquidation_marker = *l);
    }
    assert_eq!(
        DeadManCap::ALL.iter().map(|x| *x as u8).collect::<Vec<_>>(),
        vec![1, 2, 3, 4, 5],
        "REF-075.16"
    );
    for d in DeadManCap::ALL {
        caps_rt(2, |c| {
            c.dms = *d;
            if *d != DeadManCap::None {
                (c.dms_min_ms, c.dms_max_ms, c.dms_timeout_ms, c.dms_refresh_ms) =
                    (10_000, 120_000, 20_000, 10_000);
            }
        });
    }
    assert_eq!(
        TestnetKind::ALL.iter().map(|x| *x as u8).collect::<Vec<_>>(),
        vec![1, 2, 3],
        "REF-075.17"
    );
    for t in TestnetKind::ALL {
        caps_rt(0, |c| c.testnet = *t);
    }
    // 075.18 Option<bool>
    assert_eq!(
        [
            opt_bool_code(None),
            opt_bool_code(Some(false)),
            opt_bool_code(Some(true))
        ],
        [1, 2, 3],
        "REF-075.18"
    );
    for v in [None, Some(false), Some(true)] {
        caps_rt(0, |c| c.required_multi_asset = v);
    }
    // 075.19 CapFlags
    let bits = [
        CapFlags::AMEND,
        CapFlags::POST_ONLY,
        CapFlags::IOC,
        CapFlags::FOK,
        CapFlags::BATCH_CANCEL,
    ];
    assert_eq!(
        bits.iter().map(|b| b.0).collect::<Vec<_>>(),
        vec![1, 2, 4, 8, 16],
        "REF-075.19"
    );
    for (i, a) in bits.iter().enumerate() {
        for b in &bits[i + 1..] {
            assert_eq!(a.0 & b.0, 0, "REF-075.19");
        }
    }
    caps_rt(0, |c| c.flags = CapFlags(31));
    // 075.20 InstHot.flags
    assert_eq!(InstHot::NO_AMEND, 1, "REF-075.20");
    assert_eq!(
        load_spec(|s| s.inst_mut(1).flags = InstHot::NO_AMEND)
            .inst(InstId(1))
            .flags,
        1,
        "REF-075.20"
    );
    // 075.21 FeeAssetRule
    let rules = [
        FeeAssetRule::Quote,
        FeeAssetRule::ReceivedAsset,
        FeeAssetRule::ReceivedAssetUnlessDiscount { token: BTC },
        FeeAssetRule::Configured(USDT),
    ];
    assert_eq!(
        rules.iter().map(|r| r.code()).collect::<Vec<_>>(),
        vec![1, 2, 3, 4],
        "REF-075.21"
    );
    for r in rules {
        assert_eq!(
            caps_rt(1, |c| c.fee_asset_rule = r).fee_asset_rule,
            r,
            "REF-075.21"
        );
    }
    let _ = AssetId(0);
}

#[test]
fn ref_076_load_err_codes() {
    use LoadErrCode::*;
    assert_eq!(size_of::<LoadErrCode>(), 2);
    let rows: [(&str, &[LoadErrCode], u16); 7] = [
        (
            "REF-076.1",
            &[
                Structure,
                Strings,
                Ids,
                UnknownVenue,
                InstanceIdRange,
                DanglingRef,
                QuoteNotStable,
                InstanceMultiBound,
            ],
            1,
        ),
        (
            "REF-076.2",
            &[
                MissingCaps,
                CapsMarketMismatch,
                SpotBorrowRequired,
                DeadManRange,
                CapsValue,
            ],
            9,
        ),
        (
            "REF-076.3",
            &[
                ExpRange,
                NotionalExp,
                UnsupportedKind,
                TickMismatch,
                AssetExpTooCoarse,
                ValueRange,
                DuplicateSymbol,
            ],
            14,
        ),
        (
            "REF-076.4",
            &[
                SameExchangeMarkets,
                DuplicateVenueUid,
                MissingFee,
                MissingVenueUid,
                TooManyAccountsPerVenue,
                SimIsProd,
                SimHasUid,
                SimCfg,
                UidPolicyProd,
            ],
            21,
        ),
        (
            "REF-076.5",
            &[
                AlreadyActive,
                VersionNotIncreasing,
                InstRemoved,
                AssetChanged,
                MultiplierChanged,
                AccountKindChanged,
                UidChanged,
                InstanceChanged,
            ],
            30,
        ),
        (
            "REF-076.6",
            &[
                AccountBound,
                KindMismatch,
                LegNotTradable,
                LegVenueNotInAccount,
                LegAssetMismatch,
                QuoteMismatch,
                SameExchangeLegs,
            ],
            38,
        ),
        (
            "REF-076.7",
            &[
                NoValidator,
                HostClassMismatch,
                RefInstNotSubscribed,
                UnknownStrategyKind,
                CapsExceedProtocol,
                StaleGuardMismatch,
                BindingChangedWithPosition,
            ],
            45,
        ),
    ];
    for (case, codes, first) in rows {
        for (k, c) in codes.iter().enumerate() {
            assert_eq!(*c as u16, first + k as u16, "{case} {c:?}");
            assert_eq!(LoadErrCode::try_from(*c as u16), Ok(*c), "{case} {c:?}");
        }
    }
    assert!(LoadErrCode::try_from(0u16).is_err(), "REF-076.8");
    assert!(LoadErrCode::try_from(52u16).is_err(), "REF-076.8");
}
