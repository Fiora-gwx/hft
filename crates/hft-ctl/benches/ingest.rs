//! CTL-150–CTL-153: request arrival → ctl ring (gateway core; the socket read is not timed).

#![allow(
    clippy::disallowed_types,
    clippy::disallowed_macros,
    clippy::disallowed_methods
)] // allow-list: bench code, not a hot path

#[path = "../tests/common/mod.rs"]
mod common;

use std::hint::black_box;

use common::*;
use criterion::{BatchSize, Criterion, criterion_group, criterion_main};
use hft_types::Tsc;
use serde_json::{Value, json};

fn harness(max_groups: u16) -> G0 {
    let mut cfg = config(std::env::temp_dir().join("ctl-bench.sock"));
    cfg.write_rate_per_min = u16::MAX;
    cfg.warmup_rate_per_s = u16::MAX;
    cfg.max_groups_per_instance = max_groups;
    G0::with_cfg(p0::p0(), 1 << 30, g0_active(), cfg)
}

fn body(issuer: &str, account: Option<&str>, cmd: &Value) -> Vec<u8> {
    let mut v = json!({
        "v": 2,
        "cmd_id": new_id().hyphenated().to_string(),
        "issuer": issuer,
        "issued_at_ns": UTC0,
        "expires_at_ns": UTC0 + 30 * S as i64,
        "cmd": cmd,
    });
    if let Some(a) = account {
        v["account"] = json!(a);
    }
    serde_json::to_vec(&v).unwrap()
}

/// One request through the core, then the fake sequencer clears it (ring, Mailbox, pending).
fn ingest(g: &mut G0, uid: u32, b: &[u8]) {
    let r = g.core.on_request(1, uid, black_box(b), Tsc(0));
    black_box(&r);
    g.core.ring_mut().out.clear();
    g.core.ring_mut().free = 1 << 30;
    while g.mb_take.try_take().is_some() {}
    let id = hft_ctl_proto::parse_request(b).unwrap().cmd_id;
    g.result(*id.as_bytes(), 1, Outcome::APPLIED);
}

fn bench_cmd(
    c: &mut Criterion,
    name: &str,
    issuer: &str,
    uid: u32,
    account: Option<&str>,
    cmd: Value,
    max_groups: u16,
) {
    let mut g = harness(max_groups);
    c.bench_function(name, |b| {
        b.iter_batched(
            || body(issuer, account, &cmd),
            |req| ingest(&mut g, uid, &req),
            BatchSize::SmallInput,
        )
    });
}

fn ctl_150_ingest(c: &mut Criterion) {
    bench_cmd(
        c,
        "ctl/ingest",
        "cli:tom",
        1201,
        Some("acct-a"),
        json!({"type":"HedgeFix","group":"16777317"}),
        8,
    );
}

fn ctl_151_ingest_setparams_500(c: &mut Criterion) {
    let mut s = Spb::s1(42);
    s.groups = (0..500u32).map(|k| g_s1((1 << 24) | (101 + k))).collect();
    let cmd = json!({"type":"SetParams","strategy":"s1-main","blob_b64":b64(&s.bytes())});
    bench_cmd(
        c,
        "ctl/ingest_setparams_500",
        "cli:tom",
        1201,
        Some("acct-a"),
        cmd,
        512,
    );
}

fn ctl_152_get_schema(c: &mut Criterion) {
    bench_cmd(
        c,
        "ctl/get_schema",
        "cli:vera",
        1202,
        None,
        json!({"type":"GetSchema","strategy_kind":"probe-target"}),
        8,
    );
}

fn ctl_153_ingest_warmup_4k(c: &mut Criterion) {
    let cmd = json!({"type":"InjectWarmup","strategy":"s2-main","local_kind":"3","chunk_no":"0","payload_b64":b64(&[5u8; 4096])});
    bench_cmd(c, "ctl/ingest_warmup_4k", "pipeline", 1101, None, cmd, 8);
}

criterion_group!(
    benches,
    ctl_150_ingest,
    ctl_151_ingest_setparams_500,
    ctl_152_get_schema,
    ctl_153_ingest_warmup_4k
);
criterion_main!(benches);
