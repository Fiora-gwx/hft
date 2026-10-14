//! Seeded producer schedules against the actual LiveDriver, with virtual processing costs.
use crate::common::*;
use hft_event::{ControlCmd, EventKind, L2BatchHdr, Level, Role};
use hft_sequencer::{MemRecord, StepOutcome, driver::live::RingId};
use hft_types::{GroupId, InstId, KillLevel, Qty, Tsc};
use zerocopy::{FromZeros, IntoBytes};

const MD_INST: InstId = InstId(15);

fn seeds() -> Vec<u64> {
    let identity = std::env::var("HFT_DST_SEED_ID")
        .or_else(|_| std::env::var("GITHUB_SHA"))
        .unwrap_or_else(|_| {
            let o = std::process::Command::new("git")
                .args(["rev-parse", "HEAD"])
                .output()
                .unwrap();
            assert!(o.status.success());
            String::from_utf8(o.stdout).unwrap()
        });
    let start = xxhash_rust::xxh3::xxh3_64(identity.trim().as_bytes());
    let mut out: Vec<_> = (0..200).map(|i| start.wrapping_add(i)).collect();
    for file in [
        include_str!("../../../fixtures/dst-seeds/core.txt"),
        include_str!("../../../fixtures/dst-seeds/modes.txt"),
        include_str!("../../../fixtures/dst-seeds/storage.txt"),
    ] {
        for line in file
            .lines()
            .filter(|s| !s.trim().is_empty() && !s.starts_with('#'))
        {
            let s = line.split_whitespace().next().unwrap();
            out.push(u64::from_str_radix(s.trim_start_matches("0x"), 16).unwrap());
        }
    }
    out.sort_unstable();
    out.dedup();
    out
}

fn batch(id: u64, at: u64) -> Elem {
    let mut x = L2BatchHdr::new_zeroed();
    x.h.inst = MD_INST;
    x.h.venue = A;
    x.h.rx_tsc = at;
    x.prev_id = id - 1;
    x.last_id = id;
    x.n_bids = 1;
    x.n_asks = 1;
    let mut body = x.as_bytes().to_vec();
    for price in ["50000", "50002"] {
        body.extend_from_slice(
            Level {
                px: px(price),
                qty: Qty(100_000 + id as i64),
            }
            .as_bytes(),
        );
    }
    Elem {
        kind: EventKind::L2_BATCH,
        flags: hft_event::RecordFlags(0),
        body,
    }
}

fn baseline(full_batches: bool) -> H {
    reset_script();
    let mut b = b0();
    b.rd.insts.iter_mut().find(|i| i.id == MD_INST).unwrap().l2_depth = 5;
    if full_batches {
        b.seq.conflate_backlog = u16::MAX;
    }
    let mut h = b.boot();
    h.steady();
    // Cross a real system deadline during the backlog; neither run injects a synthetic Timer.
    let deadline =
        h.s.engine()
            .heap
            .sorted()
            .iter()
            .map(|x| x.deadline)
            .min()
            .unwrap();
    h.at(deadline.saturating_sub(250_000).max(h.now()));
    let now = h.now();
    let mut snap = hft_event::L2SnapHdr::new_zeroed();
    snap.h.inst = MD_INST;
    snap.h.venue = A;
    snap.h.rx_tsc = now;
    snap.snap_id = 1;
    snap.origin = hft_event::L2SnapHdr::EXCHANGE;
    snap.n_bids = 1;
    snap.n_asks = 1;
    let mut body = snap.as_bytes().to_vec();
    for price in ["50000", "50002"] {
        body.extend_from_slice(
            Level {
                px: px(price),
                qty: q("1"),
            }
            .as_bytes(),
        );
    }
    h.push_bytes(RingId::Md(0), EventKind::L2_SNAPSHOT, body);
    h.run();
    for k in 2..2002 {
        h.push_e(RingId::Md(0), batch(k, now));
    }
    for k in 0..128 {
        h.push(RingId::Bbo(0), &bbo(A1, A, 0, "50000", "50002", 10_000 + k, now));
    }
    for k in 0..16 {
        let mut a = hft_event::AdviceHdr::new_zeroed();
        a.strategy = S2;
        a.account = AC2;
        a.group = G2;
        a.local_kind = 7;
        a.basis_seq = k;
        h.push_bytes(RingId::Advice, EventKind::ADVICE, a.as_bytes().to_vec());
    }
    h
}

/// Three independent Poisson streams, mean spacing 200 us, precomputed from the seed.
fn schedule(h: &mut H, seed: u64) {
    let base = h.now();
    for stream in 0..3 {
        let mut at = base;
        for k in 0..256 {
            let bits = hft_testkit::seed::derive(seed, &format!("arrival/{stream}/{k}"));
            let u = ((bits >> 11) as f64 + 0.5) / ((1u64 << 53) as f64);
            at += (-u.ln() * 200_000.0).ceil().max(1.0) as u64;
            if at > base + 5_000_000 {
                break;
            }
            let (ring, e) = match stream {
                0 => (
                    RingId::Bbo(1),
                    elem(&bbo(B1, B, 0, "50010", "50012", 100_000 + k, at)),
                ),
                1 => (
                    RingId::Rep(1),
                    elem(&hft_event::BalanceReport {
                        venue: B,
                        account: AC1,
                        asset: USDT,
                        _pad: 0,
                        free: q("100000"),
                        locked: Qty(0),
                        ex_ts_ns: at as i64,
                        rx_tsc: Tsc(at),
                    }),
                ),
                _ => {
                    let (e, _) = h.cmd_elem(
                        "cli:test",
                        4,
                        AC1,
                        &ControlCmd::Kill {
                            scope: scope(S1, AC1, GroupId(0)),
                            level: KillLevel::Account,
                            flatten: false,
                            cancel_resting: false,
                            hard: false,
                        },
                        |_| {},
                    );
                    (RingId::Ctl, e)
                }
            };
            h.io().timed_arrivals.push((at, ring, e));
        }
    }
    h.io().timed_arrivals.sort_by_key(|a| a.0);
}

fn drain(h: &mut H) -> Vec<usize> {
    let mut stops = Vec::new();
    for _ in 0..20_000 {
        let outcome = h.step();
        stops.push(h.io_ref().rings.get(&RingId::Md(0)).map_or(0, |q| q.len()));
        if outcome == StepOutcome::Idle {
            assert!(h.io_ref().timed_arrivals.is_empty());
            assert!(h.io_ref().rings.values().all(|q| q.is_empty()));
            return stops;
        }
        assert_eq!(outcome, StepOutcome::Busy);
    }
    panic!("producer/consumer did not finish");
}

fn run(seed: u64, scale: u64) -> (Vec<MemRecord>, Vec<usize>) {
    let mut h = baseline(true);
    schedule(&mut h, seed);
    h.io().tsc_per_take = 5_000 * scale;
    let mark = h.mark();
    let stops = drain(&mut h);
    h.keyframe();
    assert!(
        h.verify().is_empty(),
        "seed={seed:#x}, scale={scale}: INV-6 / VerifyDriver"
    );
    (h.since(mark), stops)
}

#[test]
fn seq_076_preempt_positions_verify() {
    for seed in seeds() {
        let (fast, stops_fast) = run(seed, 1);
        let (slow, stops_slow) = run(seed, 3);
        let (again, _) = run(seed, 1);
        assert_eq!(fast, again, "seed={seed:#x}: INV-9 exact rerun");
        let inputs = |r: &[MemRecord]| {
            r.iter()
                .filter(|r| r.role() == Role::Input)
                .map(|r| (r.kind, r.body.clone()))
                .collect::<Vec<_>>()
        };
        assert_ne!(inputs(&fast), inputs(&slow), "seed={seed:#x}: same intake order");
        assert_ne!(stops_fast, stops_slow, "seed={seed:#x}: same md stop positions");
        let timers = |r: &[MemRecord]| {
            r.iter()
                .filter(|r| r.role() == Role::Input)
                .enumerate()
                .filter(|(_, r)| r.kind == EventKind::TIMER)
                .map(|(i, _)| i)
                .collect::<Vec<_>>()
        };
        assert!(!timers(&fast).is_empty());
        assert_ne!(
            timers(&fast),
            timers(&slow),
            "seed={seed:#x}: same Timer positions"
        );
    }
}

#[test]
#[ignore = "SEQ-063 quota continuation conflicts with SEQ-077; owner decision pending"]
fn seq_077_preempt_wait_bounds() {
    for seed in std::iter::once(0x784b9e117907309c).chain(seeds()) {
        let mut h = baseline(false);
        schedule(&mut h, seed);
        h.io().tsc_per_take = 5_000;
        drain(&mut h);
        let inputs: Vec<_> = h.j().records().filter(|r| r.role() == Role::Input).collect();
        assert!(
            inputs
                .iter()
                .any(|r| r.flags.contains(hft_event::RecordFlags::CONFLATED))
        );
        for (at, _, ring, expected) in &h.io_ref().injected {
            let end = inputs
                .iter()
                .position(|r| r.kind == expected.kind && r.body == expected.body)
                .unwrap_or_else(|| panic!("seed={seed:#x}: arrival {at} {ring:?} not journaled"));
            let blocked = inputs[..end]
                .iter()
                .filter(|r| r.tsc >= *at)
                .filter(|r| {
                    r.kind == EventKind::L2_BATCH
                        || (*ring != RingId::Bbo(1) && [EventKind::BBO, EventKind::ADVICE].contains(&r.kind))
                })
                .count();
            assert!(
                blocked <= 1,
                "seed={seed:#x}: {ring:?} blocked by {blocked} lower-priority Inputs"
            );
        }
        h.keyframe();
        assert!(h.verify().is_empty(), "seed={seed:#x}");
    }
}
