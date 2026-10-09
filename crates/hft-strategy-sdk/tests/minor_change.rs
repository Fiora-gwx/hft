//! Gate ⑦ minor-change check (SDK-110, SDK-111).

#![allow(
    clippy::disallowed_types,
    clippy::disallowed_macros,
    clippy::disallowed_methods
)] // allow-list: test code, not a hot path
#![allow(clippy::type_complexity)] // table-driven rows

use crate::common;

use common::*;
use hft_strategy_sdk::params::*;
use hft_types::{GROUP_NONE, GroupId};

fn base() -> (RawParams, Vec<GroupId>) {
    (raw(tp_defaults(), &[(1, 0, 5_000_000)]), vec![G1, G2])
}

fn check(b: &(RawParams, Vec<GroupId>), c: &(RawParams, Vec<GroupId>)) -> Result<(), MajorChange> {
    minor_change_check(&TP, (&b.0, &b.1), (&c.0, &c.1))
}

fn err(field_ix: u16, group: GroupId) -> Result<(), MajorChange> {
    Err(MajorChange { field_ix, group })
}

#[test]
fn sdk_110_minor_change_check() {
    type Edit = fn(&mut (RawParams, Vec<GroupId>));
    let rows: [(u32, Option<Edit>, Edit, Result<(), MajorChange>); 17] = [
        (1, None, |_| {}, Ok(())),
        (2, None, |c| c.0.defaults[0] = 5_500_000, Ok(())),
        (3, None, |c| c.0.defaults[0] = 5_500_001, err(0, GROUP_NONE)),
        (4, None, |c| c.0.defaults[0] = 4_500_000, Ok(())),
        (5, None, |c| c.0.defaults[0] = 4_499_999, err(0, GROUP_NONE)),
        (
            6,
            Some(|b| b.0.defaults[0] = 0),
            |c| c.0.defaults[0] = 1,
            err(0, GROUP_NONE),
        ),
        (7, None, |c| c.0.defaults[1] = 21_000, Ok(())),
        (8, None, |c| c.0.defaults[1] = 21_001, err(1, GROUP_NONE)),
        (9, None, |c| c.0.defaults[1] = 18_999, err(1, GROUP_NONE)),
        (
            10,
            None,
            |c| c.0.overrides.push(ParamOverride::new(0, 3, 1_000_000_000_000)),
            Ok(()),
        ),
        (11, None, |c| c.0.defaults[2] = 100_001, err(2, GROUP_NONE)),
        (12, None, |c| c.0.overrides[0].value = 6_000_000, err(0, G2)),
        (
            13,
            None,
            |c| {
                c.1 = vec![G2, G1];
                c.0.overrides = vec![ParamOverride::new(0, 0, 5_000_000)];
            },
            Ok(()),
        ),
        (
            14,
            None,
            |c| {
                c.1.push(G3);
                c.0.overrides.push(ParamOverride::new(2, 0, 9_000_000));
            },
            Ok(()),
        ),
        (
            15,
            None,
            |c| {
                c.1 = vec![G2];
                c.0.overrides = vec![ParamOverride::new(0, 0, 5_000_000)];
            },
            Ok(()),
        ),
        (
            16,
            None,
            |c| {
                c.0.defaults[1] = 30_000;
                c.0.overrides[0].value = 6_000_000;
            },
            err(0, G2),
        ),
        (
            17,
            None,
            |c| {
                c.0.overrides = vec![
                    ParamOverride::new(0, 0, 6_000_000),
                    ParamOverride::new(1, 0, 6_000_000),
                ];
            },
            err(0, G1),
        ),
    ];
    for (row, edit_base, edit_cur, want) in rows {
        let mut b = base();
        if let Some(e) = edit_base {
            e(&mut b);
        }
        let mut c = b.clone();
        edit_cur(&mut c);
        assert_eq!(check(&b, &c), want, "SDK-110.{row}");
    }
}

#[test]
fn sdk_111_cumulative_against_base() {
    let b = (raw(tp_defaults(), &[]), vec![G1]);
    let mut a = b.clone();
    a.0.defaults[0] = 5_400_000;
    assert_eq!(check(&b, &a), Ok(()));
    let mut v = b.clone();
    v.0.defaults[0] = 5_800_000;
    assert_eq!(check(&b, &v), err(0, GROUP_NONE));
}
