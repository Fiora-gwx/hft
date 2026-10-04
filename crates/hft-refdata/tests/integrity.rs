//! Load step 9: account integrity (REF-031 – REF-038).

mod common;

use common::*;
use hft_refdata::LoadErrCode::{
    DuplicateVenueUid, MissingFee, MissingVenueUid, SameExchangeMarkets, SimCfg, SimHasUid, SimIsProd,
    TooManyAccountsPerVenue, UidPolicyProd,
};
use hft_refdata::{
    AccountKind, AccountSpec, AccountVenueSpec, FeeOverrideSpec, FeeSpec, InstStatus, SimAccountCfg,
    SimBalance, SimSpec, UidPolicy,
};
use hft_types::{AccountId, InstId, Qty, StrategyId, VenueId};

fn av(account: u8, venue: u8, uid: &str) -> AccountVenueSpec {
    AccountVenueSpec {
        account: AccountId(account),
        venue: VenueId(venue),
        uid: uid.into(),
    }
}

fn live(id: u8, label: &str, is_prod: bool, venues: &[u8]) -> AccountSpec {
    AccountSpec {
        id: AccountId(id),
        is_prod,
        venues: vbits(venues),
        label: label.into(),
        ..AccountSpec::default()
    }
}

#[test]
fn ref_031_same_exchange_markets() {
    let mut s = f0_spec();
    s.account_mut(1).venues = vbits(&[5, 6]);
    s.account_venues.push(av(1, 5, "1005"));
    assert_eq!(errs(&s.build()), vec![e(SameExchangeMarkets, 4, 0)], "REF-031.1");

    let mut s = f0_spec();
    s.account_mut(2).venues = vbits(&[1, 2]);
    s.sim_balances[1].venue = V1;
    s.fees.push(FeeSpec {
        venue: V2,
        tier: 1,
        fee: fee(100_000, 400_000),
    });
    assert_eq!(errs(&s.build()), vec![e(SameExchangeMarkets, 4, 1)], "REF-031.2");

    let mut s = f0_spec();
    s.account_mut(1).venues = vbits(&[1, 5]);
    s.account_venues.push(av(1, 5, "1005"));
    ok(&s.build()); // REF-031.3

    let mut s = f0_spec();
    s.account_mut(1).venues = vbits(&[2, 6]);
    s.account_venues.push(av(1, 2, "1002"));
    ok(&s.build()); // REF-031.4
}

#[test]
fn ref_032_duplicate_venue_uid() {
    type Acc<'a> = Option<(bool, u8, &'a str, UidPolicy)>;
    let with = |a3: Acc<'_>, a4: Acc<'_>| {
        let mut s = f0_spec();
        for (id, a) in [(3u8, a3), (4u8, a4)] {
            if let Some((prod, v, uid, policy)) = a {
                let mut x = live(id, if id == 3 { "acct-b" } else { "acct-c" }, prod, &[v]);
                x.uid_policy = policy;
                s.accounts.push(x);
                s.account_venues.push(av(id, v, uid));
            }
        }
        load(&s.build())
    };
    let v = UidPolicy::Verified;
    let dvu = |row: u32| vec![e(DuplicateVenueUid, 14, row)];
    assert_eq!(
        outcome(with(Some((true, 6, "2001", v)), None)),
        dvu(2),
        "REF-032.1"
    );
    assert_eq!(
        outcome(with(Some((false, 6, "2001", v)), None)),
        dvu(2),
        "REF-032.2"
    );
    let rd = with(Some((false, 2, "9001", v)), Some((false, 1, "9001", v))).expect("REF-032.3");
    assert_eq!(rd.warnings(), &dvu(3)[..], "REF-032.3");
    assert_eq!(
        outcome(with(Some((false, 2, "9001", v)), Some((false, 2, "9001", v)))),
        dvu(3),
        "REF-032.4"
    );
    let rd = with(Some((true, 5, "1001", v)), None).expect("REF-032.5");
    assert!(rd.warnings().is_empty(), "REF-032.5");
    let u = UidPolicy::Unverifiable;
    let rd = with(Some((false, 2, "", u)), Some((false, 1, "", u))).expect("REF-032.6");
    assert!(rd.warnings().is_empty(), "REF-032.6");
}

#[test]
fn ref_033_missing_venue_uid() {
    let mut s = f0_spec();
    s.account_venues
        .retain(|r| !(r.account == AccountId(1) && r.venue == V6));
    assert_single(&s.build(), MissingVenueUid, "REF-033.1");
    let mut s = f0_spec();
    s.account_venues[1].uid = String::new();
    assert_single(&s.build(), MissingVenueUid, "REF-033.2");
    s.account_mut(1).is_prod = false;
    s.account_mut(1).uid_policy = UidPolicy::Unverifiable;
    let rd = ok(&s.build());
    assert_eq!(rd.venue_uid(AccountId(1), V6), None, "REF-033.3");
    assert_eq!(rd.venue_uid(AccountId(1), V1), Some("1001"), "REF-033.3");
}

#[test]
fn ref_034_uid_policy_prod() {
    let mut s = f0_spec();
    s.account_mut(1).uid_policy = UidPolicy::Unverifiable;
    assert_eq!(errs(&s.build()), vec![e(UidPolicyProd, 4, 0)], "REF-034.1");
    let mut s = f0_spec();
    s.account_mut(2).uid_policy = UidPolicy::Unverifiable;
    assert_eq!(errs(&s.build()), vec![e(UidPolicyProd, 4, 1)], "REF-034.2");
    let mut s = f0_spec();
    s.account_mut(1).is_prod = false;
    s.account_mut(1).uid_policy = UidPolicy::Unverifiable;
    let rd = ok(&s.build());
    assert_eq!(
        rd.account(AccountId(1)).uid_policy,
        UidPolicy::Unverifiable,
        "REF-034.3"
    );
    // F0 builds account 1 with `..AccountSpec::default()`, i.e. without writing uid_policy.
    let rd = ok(&f0());
    assert_eq!(
        rd.account(AccountId(1)).uid_policy,
        UidPolicy::Verified,
        "REF-034.4"
    );
    assert_eq!(rd.account(AccountId(1)).uid_policy as u8, 1, "REF-034.4");
}

#[test]
fn ref_035_missing_fee() {
    let without_6_0 = || {
        let mut s = f0_spec();
        s.fees.retain(|f| !(f.venue == V6 && f.tier == 0));
        s
    };
    assert_single(&without_6_0().build(), MissingFee, "REF-035.1");
    let ov = |inst: u32| FeeOverrideSpec {
        account: AccountId(1),
        venue: V6,
        inst: InstId(inst),
        fee: fee(300_000, 700_000),
    };
    let mut s = without_6_0();
    s.fee_overrides.push(ov(0));
    ok(&s.build()); // REF-035.2
    let mut s = without_6_0();
    s.fee_overrides.push(ov(4));
    ok(&s.build()); // REF-035.3
    let mut s = without_6_0();
    s.inst_mut(4).status = InstStatus::Halted;
    ok(&s.build()); // REF-035.4
    let mut s = without_6_0();
    s.inst_mut(4).status = InstStatus::ReduceOnly;
    assert_eq!(
        load(&s.build()).expect_err("REF-035.5").code(),
        MissingFee,
        "REF-035.5"
    );
    let mut s = f0_spec();
    s.fees.retain(|f| !(f.venue == V6 && f.tier == 1));
    assert_eq!(
        load(&s.build()).expect_err("REF-035.6").code(),
        MissingFee,
        "REF-035.6"
    );
    let mut s = f0_spec();
    s.sims[0].fee_tier = 2;
    assert_eq!(
        load(&s.build()).expect_err("REF-035.7").code(),
        MissingFee,
        "REF-035.7"
    );
}

#[test]
fn ref_036_accounts_per_venue() {
    let with = |n: u8, sim_b: bool| {
        let mut s = f0_spec();
        s.account_mut(1).venues = vbits(&[1]);
        s.account_venues
            .retain(|r| !(r.account == AccountId(1) && r.venue == V6));
        for k in 0..n {
            let id = 3 + k;
            s.accounts.push(live(id, &format!("l{k}"), true, &[6]));
            s.account_venues.push(av(id, 6, &format!("u{k}")));
        }
        if sim_b {
            let id = 3 + n;
            s.accounts.push(AccountSpec {
                id: AccountId(id),
                sim: true,
                venues: vbits(&[6]),
                label: "sim-b".into(),
                ..AccountSpec::default()
            });
            s.sims.push(SimSpec {
                account: AccountId(id),
                fee_tier: 1,
                latency_model: 0,
                margin_model: 0,
            });
            s.sim_balances.push(sim_usdt(id, V6));
        }
        load(&s.build())
    };
    assert!(with(11, false).is_ok(), "REF-036.1");
    let rd = with(12, false).expect("REF-036.2");
    assert_eq!(rd.venue_account_ix(V6, AccountId(14)), Some(11), "REF-036.2");
    assert_eq!(
        with(13, false).expect_err("REF-036.3").code(),
        TooManyAccountsPerVenue,
        "REF-036.3"
    );
    assert!(with(12, true).is_ok(), "REF-036.4");
}

#[test]
fn ref_037_sim_account_rules() {
    let edit = |f: &dyn Fn(&mut Spec)| {
        let mut s = f0_spec();
        f(&mut s);
        load(&s.build())
    };
    assert_eq!(
        outcome(edit(&|s| s.account_mut(2).is_prod = true)),
        vec![e(SimIsProd, 4, 1)],
        "REF-037.1"
    );
    assert_eq!(
        outcome(edit(&|s| s.account_venues.push(av(2, 1, "5001")))),
        vec![e(SimHasUid, 14, 2)],
        "REF-037.2"
    );
    let es = outcome(edit(&|s| s.sims.clear()));
    assert!(es.len() == 1 && es[0].code == SimCfg, "REF-037.3: {es:?}");
    assert_eq!(
        outcome(edit(&|s| s.sims.push(s.sims[0]))),
        vec![e(SimCfg, 16, 1)],
        "REF-037.4"
    );
    let es = outcome(edit(&|s| s.sim_balances.clear()));
    assert!(es.len() == 1 && es[0].code == SimCfg, "REF-037.5: {es:?}");
    assert_eq!(
        outcome(edit(&|s| s.sim_balances[0].venue = V5)),
        vec![e(SimCfg, 17, 0)],
        "REF-037.6"
    );
    let live_sim = SimSpec {
        account: AccountId(1),
        fee_tier: 0,
        latency_model: 0,
        margin_model: 0,
    };
    assert_eq!(
        outcome(edit(&|s| s.sims.push(live_sim))),
        vec![e(SimCfg, 16, 0)],
        "REF-037.7"
    );
    let live_bal = hft_refdata::SimBalanceSpec {
        account: AccountId(1),
        venue: V1,
        asset: USDT,
        qty: Qty(1),
    };
    assert_eq!(
        outcome(edit(&|s| s.sim_balances.push(live_bal))),
        vec![e(SimCfg, 17, 0)],
        "REF-037.8"
    );
}

#[test]
fn ref_038_sim_model_index_unchecked() {
    let mut s = f0_spec();
    s.sims[0].latency_model = 65_535;
    s.sims[0].margin_model = 65_535;
    let rd = ok(&s.build());
    let want = AccountKind::Sim(SimAccountCfg {
        initial_balances: vec![
            SimBalance {
                venue: V1,
                asset: USDT,
                qty: Qty(1_000_000_000_000),
            },
            SimBalance {
                venue: V6,
                asset: USDT,
                qty: Qty(1_000_000_000_000),
            },
        ]
        .into(),
        latency_model: 65_535,
        fee_tier: 1,
        margin_model: 65_535,
    });
    assert_eq!(rd.account(AccountId(2)).kind, want);
    let _ = StrategyId(0);
}
