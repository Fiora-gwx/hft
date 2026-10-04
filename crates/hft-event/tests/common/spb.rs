//! SPB1 blob builder for tests (EVT-131 – EVT-133, EVT-195, EVT-203 corpus).
#![allow(dead_code)]

use hft_event::*;
use hft_types::*;
use zerocopy::IntoBytes;

pub fn leg(inst: u32, kind: u8, route: u32) -> LegDef {
    LegDef {
        inst: InstId(inst),
        venue: VenueId(1),
        kind,
        _pad: 0,
        route,
        _pad2: 0,
    }
}

pub const NO_LEG: LegDef = LegDef {
    inst: InstId(0),
    venue: VenueId(0),
    kind: 0,
    _pad: 0,
    route: 0,
    _pad2: 0,
};

pub fn group(id: u32, legs: &[LegDef], open: u8) -> GroupSpec {
    let mut l = [NO_LEG; 4];
    l[..legs.len()].copy_from_slice(legs);
    GroupSpec {
        id: GroupId(id),
        asset: AssetId(7),
        n_legs: legs.len() as u8,
        open_allowed: open,
        legs: l,
        _rsv: 0,
    }
}

pub fn hdr(strategy: u8, n_groups: u16, body_len: u32, hwm: u32) -> StrategyParamsHdr {
    StrategyParamsHdr {
        strategy: StrategyId(strategy),
        account: AccountId(2),
        kind_id: 0x0101,
        schema_id: 9,
        schema_ver: 1,
        source: ParamSource::Pipeline,
        _pad: 0,
        n_groups,
        _pad2: 0,
        version: 5,
        generated_at_ns: 1_700_000_000_000_000_000,
        body_len,
        group_ctr_hwm: hwm,
        schema_digest: 0xABCD,
    }
}

pub fn blob(h: &StrategyParamsHdr, groups: &[GroupSpec], body: &[u8]) -> Vec<u8> {
    let mut b = h.as_bytes().to_vec();
    for g in groups {
        b.extend_from_slice(g.as_bytes());
    }
    b.extend_from_slice(body);
    b
}

/// EVT-131 blob B (224 B).
pub fn blob_b() -> Vec<u8> {
    let g0 = group(
        0x0300_0001,
        &[leg(10, LegDef::CEX, 0), leg(20, LegDef::CEX, 0)],
        1,
    );
    let g1 = group(0x0300_0002, &[leg(30, LegDef::CEX, 0)], 0);
    let body: Vec<u8> = (0..16).collect();
    blob(&hdr(3, 2, 16, 2), &[g0, g1], &body)
}

/// 8-byte aligned copy (SPB1 views are zero-copy).
pub fn aligned(b: &[u8]) -> (Vec<u64>, usize) {
    let mut v = vec![0u64; b.len().div_ceil(8)];
    v.as_mut_bytes()[..b.len()].copy_from_slice(b);
    (v, b.len())
}
