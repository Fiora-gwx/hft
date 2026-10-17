//! Owner-approved deployment shape must fail closed on every parameter entry path.
#![cfg(feature = "cross-taker")]
#![allow(
    clippy::disallowed_methods,
    clippy::disallowed_macros,
    clippy::disallowed_types
)]
#[path = "../../portfolio-carry/tests/common/mod.rs"]
mod common;
use common::*;
use hft_strategy_registry::Strategies;
use hft_strategy_sdk::{
    params::{RawParams, encode_body},
    prelude::*,
};
use zerocopy::FromZeros;

fn groups(legs: [(InstId, VenueId); 2]) -> GroupTable {
    let mut g = GroupSpec::new_zeroed();
    g.id = G4;
    g.asset = BTC_ID;
    g.n_legs = 2;
    g.open_allowed = 1;
    for (dst, (inst, venue)) in g.legs.iter_mut().zip(legs) {
        dst.inst = inst;
        dst.venue = venue;
        dst.kind = LegDef::CEX;
    }
    GroupTable::new(&[g], 4)
}

#[test]
fn product_shape_rejects_wrong_venue_market_order_asset_and_missing_reference() {
    let rd = fixture_rd();
    let rv = RefView::new(&rd, A1);
    for kind in [1, 3] {
        let schema = Strategies::kinds()
            .iter()
            .find(|d| d.kind_id == kind)
            .unwrap()
            .schema;
        // Disabled configurations still must conform, including before later SetParams.
        let raw = RawParams {
            defaults: schema.fields.iter().map(|f| f.default).collect(),
            overrides: vec![],
        };
        let body = encode_body(schema, 1, &raw).unwrap();
        let good = groups([(I2, V2), (I1, V1)]);
        let p = Strategies::decode_params(kind, &body, schema.version, &good).unwrap();
        assert!(Strategies::validate_params(&p, &good, &rv).is_ok());
        let instance = StrategyInstance {
            strategy: S3,
            account: A1,
            kind_id: kind,
        };
        let mut strategy = Strategies::create(instance, &p, &good, &rv).unwrap();
        for legs in [
            [(I1, V1), (I2, V2)], // swapped roles
            [(I2, V2), (I3, V3)], // perpetual/perpetual
            [(I3, V3), (I1, V1)], // wrong short venue
            [(I2, V2), (I4, V4)], // wrong spot venue
            [(I5, V2), (I1, V1)], // different base
            [(InstId(u32::MAX), V2), (I1, V1)],
        ] {
            let bad = groups(legs);
            assert!(
                Strategies::validate_params(&p, &bad, &rv).is_err(),
                "kind {kind}, {legs:?}"
            );
            assert!(Strategies::create(instance, &p, &bad, &rv).is_err());
            assert!(Strategies::on_params(&mut strategy, &p, &bad, &rv).is_err());
        }
        assert!(Strategies::on_params(&mut strategy, &p, &good, &rv).is_ok());
    }
}

#[test]
fn cross_taker_negative_edges_are_demo_only_on_every_param_entry() {
    use cross_taker::{CrossTaker, XtParams};
    use hft_strategy_sdk::params::ParamOverride;
    let groups = groups([(I2, V2), (I1, V1)]);
    for is_prod in [false, true] {
        let rd = fixture_rd_with_prod(is_prod);
        let rv = RefView::new(&rd, A1);
        let instance = StrategyInstance {
            strategy: S3,
            account: A1,
            kind_id: 3,
        };
        let mut raw = RawParams {
            defaults: vec![
                1,
                1_000_000,
                0,
                100_000_000,
                25_000_000,
                50_000_000,
                100,
                7_000,
                3,
                0,
            ],
            overrides: vec![],
        };
        let good = XtParams::decode(
            &encode_body(&XtParams::SCHEMA, 1, &raw).unwrap(),
            XtParams::SCHEMA.version,
            &groups,
        )
        .unwrap();
        let mut strategy = CrossTaker::new(instance, &good, &groups, &rv).unwrap();
        for field_ix in [1, 9] {
            for enabled in [0, 1] {
                for overridden in [false, true] {
                    let mut candidate = raw.clone();
                    candidate.defaults[0] = enabled;
                    if overridden {
                        candidate
                            .overrides
                            .push(ParamOverride::new(0, field_ix, -3_000_000));
                    } else {
                        candidate.defaults[usize::from(field_ix)] = -3_000_000;
                    }
                    let body = encode_body(&XtParams::SCHEMA, 1, &candidate).unwrap();
                    let p = XtParams::decode(&body, XtParams::SCHEMA.version, &groups).unwrap();
                    assert_eq!(p.check(&groups, &rv).is_ok(), !is_prod);
                    assert_eq!(p.validate(&groups, &rv).is_ok(), !is_prod);
                    assert_eq!(CrossTaker::new(instance, &p, &groups, &rv).is_ok(), !is_prod);
                    assert_eq!(strategy.on_params(&p, &groups, &rv).is_ok(), !is_prod);
                    let rp = Strategies::decode_params(3, &body, XtParams::SCHEMA.version, &groups).unwrap();
                    assert_eq!(Strategies::validate_params(&rp, &groups, &rv).is_ok(), !is_prod);
                }
            }
        }
        raw.defaults[1] = 0;
        let zero = XtParams::from_raw(&raw, 1);
        assert_eq!(
            zero.check(&groups, &rv).is_ok(),
            !is_prod,
            "production open edge stays strictly positive"
        );
        // A positive override must not hide a forbidden default, even with no groups.
        raw.defaults[1] = -1;
        raw.overrides = vec![ParamOverride::new(0, 1, 1_000_000)];
        let hidden = XtParams::from_raw(&raw, 1);
        assert_eq!(hidden.check(&groups, &rv).is_ok(), !is_prod);
        assert_eq!(hidden.check(&GroupTable::new(&[], 4), &rv).is_ok(), !is_prod);
    }
}
