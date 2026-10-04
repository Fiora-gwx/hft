//! `ControlCmd::decode` benchmark (EVT-194).

#![allow(
    clippy::disallowed_types,
    clippy::disallowed_macros,
    clippy::disallowed_methods
)] // allow-list: test code, not a hot path

use std::hint::black_box;

use criterion::{Criterion, criterion_group, criterion_main};
use hft_event::*;
use hft_types::*;

fn evt_194_cmd_decode(c: &mut Criterion) {
    let scope = CmdScope {
        strategy: StrategyId(3),
        venue: VenueId(1),
        account: AccountId(2),
        _pad: 0,
        group: GROUP_ALL,
        inst: InstId(0),
        _pad2: 0,
    };
    let cmd = ControlCmd::Kill {
        scope,
        level: KillLevel::Account,
        flatten: false,
        cancel_resting: true,
        hard: false,
    };
    let mut out = vec![0u8; 512];
    let n = cmd.encode(&CmdHdr::zeroed(), &mut out).unwrap();
    let body = &out[..n];
    c.bench_function("event_cmd_decode", |b| {
        b.iter(|| ControlCmd::decode(black_box(body)).is_ok())
    });
}

criterion_group!(benches, evt_194_cmd_decode);
criterion_main!(benches);
