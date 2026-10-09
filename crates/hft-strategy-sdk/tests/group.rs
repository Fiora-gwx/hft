//! `GroupTable` lookups and the group fingerprint (SDK-027, SDK-028).

#![allow(
    clippy::disallowed_types,
    clippy::disallowed_macros,
    clippy::disallowed_methods
)] // allow-list: test code, not a hot path

use crate::common;

use common::*;
use hft_event::{GroupSpec, LegDef};
use hft_strategy_sdk::{GroupSpecExt, GroupTable};
use hft_types::{AssetId, GROUP_ALL, GROUP_NONE, GroupId, InstId, VenueId};

#[test]
fn sdk_027_group_table_lookup() {
    let t = GroupTable::new(&[gspec(G3), gspec(G1), gspec(G2)], 8);
    assert_eq!((t.len(), t.capacity(), t.get(1).id), (3, 8, G1), "step 1");
    assert_eq!(t.gix_of(G1), Some(1));
    assert_eq!(t.gix_of(G2), Some(2));
    assert_eq!(t.gix_of(G3), Some(0));
    assert_eq!(t.gix_of(GroupId(0x0300_0004)), None);
    assert_eq!(t.gix_of(GROUP_NONE), None);
    assert_eq!(t.gix_of(GROUP_ALL), None);
    let it: Vec<(u16, GroupId)> = t.iter().map(|(g, s)| (g, s.id)).collect();
    assert_eq!(it, vec![(0, G3), (1, G1), (2, G2)]);
}

const ZERO: LegDef = LegDef {
    inst: InstId(0),
    venue: VenueId(0),
    kind: 0,
    _pad: 0,
    route: 0,
    _pad2: 0,
};

fn base() -> GroupSpec {
    let leg = |inst: u32, venue: u8| LegDef {
        inst: InstId(inst),
        venue: VenueId(venue),
        kind: 1,
        _pad: 0,
        route: 0,
        _pad2: 0,
    };
    GroupSpec {
        id: G1,
        asset: AssetId(1),
        n_legs: 2,
        open_allowed: 1,
        legs: [leg(101, 1), leg(102, 2), ZERO, ZERO],
        _rsv: 0,
    }
}

fn fp(f: impl FnOnce(&mut GroupSpec)) -> u64 {
    let mut g = base();
    f(&mut g);
    g.fingerprint()
}

#[test]
fn sdk_028_fingerprint_golden() {
    const B: u64 = 0x64DA_DF7E_A1B0_D2C2;
    assert_eq!(fp(|_| {}), B, "SDK-028.1");
    assert_eq!(fp(|g| g.open_allowed = 0), B, "SDK-028.2");
    assert_eq!(fp(|g| g.id = G2), B, "SDK-028.3");
    assert_eq!(fp(|g| g.legs[3].inst = InstId(999)), B, "SDK-028.4");
    assert_eq!(fp(|g| g._rsv = 7), B, "SDK-028.5");
    assert_eq!(fp(|g| g.asset = AssetId(2)), 0x5E26_12DA_2221_982F, "SDK-028.6");
    assert_eq!(
        fp(|g| {
            g.n_legs = 1;
            g.legs[1] = ZERO;
        }),
        0x86B0_1B84_81B0_D000,
        "SDK-028.7"
    );
    assert_eq!(
        fp(|g| g.legs[1].inst = InstId(105)),
        0x7EE5_476A_A264_BE25,
        "SDK-028.8"
    );
    assert_ne!(fp(|g| g.legs[1].venue = VenueId(3)), B, "SDK-028.9");
    assert_ne!(fp(|g| g.legs[1].kind = 2), B, "SDK-028.10");
    assert_ne!(fp(|g| g.legs[1].route = 1), B, "SDK-028.11");
}
