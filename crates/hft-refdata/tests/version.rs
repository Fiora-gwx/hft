//! Load step 10: cross-version compatibility (REF-039 – REF-044).

#![allow(
    clippy::disallowed_types,
    clippy::disallowed_macros,
    clippy::disallowed_methods
)] // allow-list: test code, not a hot path

mod common;

use common::*;
use hft_refdata::LoadErrCode::{
    AccountKindChanged, AlreadyActive, AssetChanged, InstRemoved, InstanceChanged, MultiplierChanged,
    UidChanged, VersionNotIncreasing,
};
use hft_refdata::{AccountVenueSpec, InstKind, InstStatus, InstanceSpec, LoadErr, RefData};
use hft_types::{AccountId, AssetExp, InstId, StrategyId, VenueId};

fn cur() -> RefData {
    ok(&f0())
}

/// F0 v11 with `f` applied, loaded against F0.
fn v11(f: impl FnOnce(&mut Spec)) -> Vec<LoadErr> {
    let mut s = f0_spec();
    s.version = 11;
    f(&mut s);
    outcome(RefData::load(&s.build(), Some(&cur())))
}

#[test]
fn ref_039_already_active() {
    let c = cur();
    let r = RefData::load(&f0(), Some(&c)).expect_err("same blob");
    assert_eq!(r.errors, vec![e(AlreadyActive, 0, 0)]);
    assert_eq!(r.code(), AlreadyActive);
    assert_eq!(r.code() as u16, 30);
}

#[test]
fn ref_040_version_increasing() {
    let c = cur();
    let with = |v: u64, current: Option<&RefData>| {
        let mut s = f0_spec();
        s.inst_mut(1).max_leverage = 100;
        s.version = v;
        outcome(RefData::load(&s.build(), current))
    };
    assert_eq!(
        with(10, Some(&c)),
        vec![e(VersionNotIncreasing, 0, 0)],
        "REF-040.1"
    );
    assert_eq!(
        with(9, Some(&c)),
        vec![e(VersionNotIncreasing, 0, 0)],
        "REF-040.2"
    );
    assert_eq!(with(11, Some(&c)), vec![], "REF-040.3");
    assert_eq!(with(1, None), vec![], "REF-040.4");
}

#[test]
fn ref_041_inst_cross_version() {
    let removed = |row: u32| vec![e(InstRemoved, 3, row)];
    assert_eq!(
        v11(|s| s.insts.retain(|i| i.id != InstId(5))),
        removed(4),
        "REF-041.1"
    );
    assert_eq!(
        v11(|s| s.inst_mut(5).symbol = "1000PEPEUSDC".into()),
        removed(4),
        "REF-041.2"
    );
    assert_eq!(v11(|s| s.inst_mut(5).venue = V5), removed(4), "REF-041.3");
    assert_eq!(v11(|s| s.inst_mut(2).asset = ETH), removed(1), "REF-041.4");
    assert_eq!(
        v11(|s| s.inst_mut(5).status = InstStatus::Delisted),
        vec![],
        "REF-041.5"
    );
    let add = |s: &mut Spec| {
        s.insts.push(inst(
            6,
            "ETHUSDT",
            V1,
            InstKind::Perp,
            ETH,
            ("0.10", 10_000_000),
            ("0.001", 100_000),
            ("0.001", 100_000),
        ))
    };
    assert_eq!(v11(add), vec![], "REF-041.6");
}

#[test]
fn ref_042_asset_and_multiplier() {
    assert_eq!(
        v11(|s| s.asset_mut(1).name = "XBT".into()),
        vec![e(AssetChanged, 1, 0)],
        "REF-042.1"
    );
    assert_eq!(
        v11(|s| s.asset_mut(3).exp = AssetExp {
            px_exp: -9,
            qty_exp: -8
        }),
        vec![e(AssetChanged, 1, 2)],
        "REF-042.2"
    );
    let drop_pepe = |s: &mut Spec| {
        s.insts.retain(|i| i.id != InstId(5));
        s.assets.retain(|a| a.id != PEPE);
    };
    assert_eq!(
        v11(drop_pepe),
        vec![e(InstRemoved, 3, 4), e(AssetChanged, 1, 4)],
        "REF-042.3"
    );
    let qty_shift = |s: &mut Spec| {
        let i = s.inst_mut(5);
        i.qty_unit_shift = 4;
        i.lot_str = "0.1".into();
        i.min_qty_str = "0.1".into();
    };
    assert_eq!(v11(qty_shift), vec![e(MultiplierChanged, 3, 4)], "REF-042.4");
    let px_shift = |s: &mut Spec| {
        let i = s.inst_mut(5);
        i.px_unit_shift = 4;
        i.tick_str = "0.000001".into();
    };
    assert_eq!(v11(px_shift), vec![e(MultiplierChanged, 3, 4)], "REF-042.5");
}

#[test]
fn ref_043_account_cross_version() {
    let av = |a: u8, v: u8, uid: &str| AccountVenueSpec {
        account: AccountId(a),
        venue: VenueId(v),
        uid: uid.into(),
    };
    assert_eq!(
        v11(|s| s.account_mut(1).is_prod = false),
        vec![e(AccountKindChanged, 4, 0)],
        "REF-043.1"
    );
    let to_live = |s: &mut Spec| {
        s.account_mut(2).sim = false;
        s.account_venues.push(av(2, 1, "3001"));
        s.account_venues.push(av(2, 6, "3002"));
        s.sims.clear();
        s.sim_balances.clear();
    };
    assert_eq!(v11(to_live), vec![e(AccountKindChanged, 4, 1)], "REF-043.2");
    let drop2 = |s: &mut Spec| {
        s.accounts.retain(|a| a.id != AccountId(2));
        s.sims.clear();
        s.sim_balances.clear();
    };
    assert_eq!(v11(drop2), vec![e(AccountKindChanged, 4, 1)], "REF-043.3");
    assert_eq!(
        v11(|s| s.account_venues[1].uid = "2002".into()),
        vec![e(UidChanged, 14, 1)],
        "REF-043.4"
    );
    let move_venue = |s: &mut Spec| {
        s.account_mut(1).venues = vbits(&[1, 5]);
        s.account_venues[1] = av(1, 5, "3001");
    };
    assert_eq!(v11(move_venue), vec![], "REF-043.5");
    assert_eq!(
        v11(|s| s.account_mut(1).bound_instance = StrategyId(3)),
        vec![],
        "REF-043.6"
    );
}

#[test]
fn ref_044_instance_cross_version() {
    assert_eq!(
        v11(|s| s.instances[0].kind_id = 0xFF01),
        vec![e(InstanceChanged, 15, 0)],
        "REF-044.1"
    );
    assert_eq!(
        v11(|s| s.instances[0].name = "pc-main-2".into()),
        vec![e(InstanceChanged, 15, 0)],
        "REF-044.2"
    );
    assert_eq!(
        v11(|s| {
            s.instances.pop();
        }),
        vec![e(InstanceChanged, 15, 2)],
        "REF-044.3"
    );
    let add = |s: &mut Spec| {
        s.instances.push(InstanceSpec {
            id: StrategyId(4),
            kind_id: 0xFF01,
            name: "probe-b".into(),
        })
    };
    assert_eq!(v11(add), vec![], "REF-044.4");
}
