//! NET-325: `ReadyPoll::probe` with 12 registered connections, none ready and two ready (Linux).
#![allow(
    clippy::disallowed_types,
    clippy::disallowed_macros,
    clippy::disallowed_methods
)] // allow-list: test code, not a hot path

#[path = "../tests/support/mod.rs"]
#[allow(dead_code, unused_imports, clippy::all)]
mod support;

use std::time::Duration;

use criterion::{Criterion, criterion_group, criterion_main};

fn net_325_ready_probe(c: &mut Criterion) {
    let name = "net_ready_probe";
    #[cfg(target_os = "linux")]
    {
        use hft_net::*;
        use support::looppeer::*;
        let peer = LoopPeer::start(LoopOpts::default());
        let mut k = connector(peer.port, &[(ConnRole::Md, 12)]);
        let mut rp = ReadyPoll::new().unwrap();
        let hd = rp.handle();
        let mut p = pool();
        let mut pcs = Vec::new();
        for id in 1..=12u8 {
            let (h, pc) = open(&mut k, &peer, id, ConnRole::Md);
            let id = p.install(h, None).unwrap();
            hd.add(p.get(id).unwrap()).unwrap();
            pcs.push(pc);
        }
        let mut out = ConnSet::default();
        let mut g = c.benchmark_group(name);
        g.bench_function("idle", |b| b.iter(|| std::hint::black_box(rp.probe(&mut out))));
        pcs[2].text(b"r");
        pcs[8].text(b"r");
        std::thread::sleep(Duration::from_millis(5));
        g.bench_function("two_ready", |b| {
            b.iter(|| std::hint::black_box(rp.probe(&mut out)))
        });
        g.finish();
    }
    #[cfg(not(target_os = "linux"))]
    {
        let _ = (c, name);
    }
}

criterion_group!(
    name = benches;
    config = Criterion::default().sample_size(20).measurement_time(Duration::from_secs(2)).warm_up_time(Duration::from_millis(300));
    targets = net_325_ready_probe
);
criterion_main!(benches);
