//! hostprobe benchmarks (tools/README.md "延迟与资源契约"; TOOL-174): one full `collect` over the
//! `c32-root` fixture root. The bound (≤ 2 s on the host) is in tools/README.md.

use std::hint::black_box;
use std::path::Path;

use criterion::{Criterion, criterion_group, criterion_main};
use hostprobe::collect::{Cfg, collect};
use hostprobe::host::FixtureHost;

fn tool_174_collect_time(c: &mut Criterion) {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/c32-root");
    let cfg = Cfg::default();
    c.bench_function("hostprobe/collect", |b| {
        b.iter(|| {
            let mut h = FixtureHost::new(&root);
            black_box(collect(&mut h, &cfg))
        })
    });
}

criterion_group!(benches, tool_174_collect_time);
criterion_main!(benches);
