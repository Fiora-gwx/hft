//! `parse_spb1` with 500 groups (EVT-195).

use std::hint::black_box;

use criterion::{Criterion, criterion_group, criterion_main};
use hft_event::*;
use hft_types::*;
use zerocopy::IntoBytes;

fn blob500() -> Vec<u64> {
    let n = 500u32;
    let hdr = StrategyParamsHdr {
        strategy: StrategyId(3),
        account: AccountId(2),
        kind_id: 1,
        schema_id: 1,
        schema_ver: 1,
        source: ParamSource::Pipeline,
        _pad: 0,
        n_groups: n as u16,
        _pad2: 0,
        version: 1,
        generated_at_ns: 0,
        body_len: 4096,
        group_ctr_hwm: n,
        schema_digest: 0,
    };
    let leg = |inst| LegDef {
        inst: InstId(inst),
        venue: VenueId(1),
        kind: LegDef::CEX,
        _pad: 0,
        route: 0,
        _pad2: 0,
    };
    let none = LegDef {
        inst: InstId(0),
        venue: VenueId(0),
        kind: 0,
        _pad: 0,
        route: 0,
        _pad2: 0,
    };
    let mut b = hdr.as_bytes().to_vec();
    for i in 1..=n {
        let g = GroupSpec {
            id: GroupId::compose(StrategyId(3), i).unwrap(),
            asset: AssetId(1),
            n_legs: 2,
            open_allowed: 1,
            legs: [leg(i), leg(i + 1000), none, none],
            _rsv: 0,
        };
        b.extend_from_slice(g.as_bytes());
    }
    b.extend(vec![0u8; 4096]);
    let mut v = vec![0u64; b.len() / 8];
    v.as_mut_bytes().copy_from_slice(&b);
    v
}

fn evt_195_spb1_parse_500(c: &mut Criterion) {
    let blob = blob500();
    let bytes = blob.as_bytes();
    assert!(parse_spb1(bytes).is_ok());
    c.bench_function("event_spb1_parse_500", |b| {
        b.iter(|| parse_spb1(black_box(bytes)).is_ok())
    });
}

criterion_group!(benches, evt_195_spb1_parse_500);
criterion_main!(benches);
