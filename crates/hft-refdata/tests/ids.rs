//! Load step 3: dense ids, venue-keyed tables, unknown venues, instance id range
//! (REF-013 – REF-016).

#![allow(
    clippy::disallowed_types,
    clippy::disallowed_macros,
    clippy::disallowed_methods
)] // allow-list: test code, not a hot path

use crate::common;

use common::*;
use hft_refdata::LoadErrCode::{Ids, InstanceIdRange, UnknownVenue};
use hft_refdata::{Exchange, InstKind, InstanceSpec, StaleGuardKind};
use hft_types::{AccountId, InstId, StrategyId, VenueId};

#[test]
fn ref_013_dense_ids() {
    ok(&f0()); // REF-013.1
    let assets = |ids: [u16; 5]| {
        let mut s = f0_spec();
        renumber_assets(&mut s, &ids);
        s.build()
    };
    assert_eq!(errs(&assets([1, 2, 4, 5, 6])), vec![e(Ids, 1, 2)], "REF-013.2");
    assert_eq!(errs(&assets([0, 1, 2, 3, 4])), vec![e(Ids, 1, 0)], "REF-013.3");
    assert_eq!(errs(&assets([2, 1, 3, 4, 5])), vec![e(Ids, 1, 0)], "REF-013.4");
    assert_eq!(errs(&assets([1, 1, 2, 3, 4])), vec![e(Ids, 1, 1)], "REF-013.5");

    let mut s = f0_spec();
    s.insts[3].id = InstId(5);
    s.insts[4].id = InstId(6);
    assert_eq!(errs(&s.build()), vec![e(Ids, 3, 3)], "REF-013.6");

    let mut s = f0_spec();
    s.account_mut(2).id = AccountId(3);
    s.sims[0].account = AccountId(3);
    for b in &mut s.sim_balances {
        b.account = AccountId(3);
    }
    assert_eq!(errs(&s.build()), vec![e(Ids, 4, 1)], "REF-013.7");

    let mut s = f0_spec();
    s.instances[2].id = StrategyId(4);
    assert_eq!(errs(&s.build()), vec![e(Ids, 15, 2)], "REF-013.8");
}

#[test]
fn ref_014_venue_keyed_tables() {
    ok(&f0()); // REF-014.1
    let mut s = f0_spec();
    s.venues.swap(0, 1);
    assert_eq!(errs(&s.build()), vec![e(Ids, 2, 0)], "REF-014.2");
    let mut s = f0_spec();
    s.venues[3].id = VenueId(5);
    assert_eq!(errs(&s.build()), vec![e(Ids, 2, 3)], "REF-014.3");
    let mut s = f0_spec();
    let c = s.caps[0].clone();
    s.caps.insert(1, c);
    assert_eq!(errs(&s.build()), vec![e(Ids, 13, 1)], "REF-014.4");
    let mut s = f0_spec();
    s.caps.swap(0, 1);
    assert_eq!(errs(&s.build()), vec![e(Ids, 13, 0)], "REF-014.5");
}

#[test]
fn ref_015_unknown_venue() {
    let with = |id: u8, exchange: Exchange, product: InstKind| {
        let mut s = f0_spec();
        s.add_venue(venue(VenueId(id), exchange, product, StaleGuardKind::None, 32));
        s.build()
    };
    assert_eq!(
        errs(&with(0, Exchange::Gate, InstKind::Spot)),
        vec![e(UnknownVenue, 2, 0)],
        "REF-015.1"
    );
    assert_eq!(
        errs(&with(7, Exchange::Binance, InstKind::Perp)),
        vec![e(UnknownVenue, 2, 4)],
        "REF-015.2"
    );
    assert_eq!(
        errs(&with(15, Exchange::Gate, InstKind::Perp)),
        vec![e(UnknownVenue, 2, 4)],
        "REF-015.3"
    );
    assert_eq!(
        errs(&with(34, Exchange::Chain, InstKind::Spot)),
        vec![e(UnknownVenue, 2, 4)],
        "REF-015.4"
    );
    ok(&with(3, Exchange::Gate, InstKind::Perp)); // REF-015.5
    ok(&with(33, Exchange::Chain, InstKind::Spot)); // REF-015.6
}

#[test]
fn ref_016_instance_id_range() {
    let with = |n: u16| {
        let mut s = f0_spec();
        s.instances = (1..=n)
            .map(|k| InstanceSpec {
                id: StrategyId(k as u8),
                kind_id: 0xFF00,
                name: format!("i{k}").into(),
            })
            .collect();
        s.build()
    };
    ok(&with(253)); // REF-016.1
    let rd = ok(&with(254));
    assert!(rd.instance(StrategyId(254)).is_some(), "REF-016.2");
    assert_eq!(errs(&with(255)), vec![e(InstanceIdRange, 15, 254)], "REF-016.3");
}
