//! Fast-part behavior of the test strategies (SDK-015, SDK-022, SDK-023, SDK-029, SDK-128).

#![allow(
    clippy::disallowed_types,
    clippy::disallowed_macros,
    clippy::disallowed_methods
)] // allow-list: test code, not a hot path

use crate::common;

use common::*;
use hft_strategy_sdk::testing::{CtxHarness, MockViews, Recorded, ev};
use hft_strategy_sdk::*;
use hft_types::{AssetId, GroupId, Px, Qty, RejectBits, RunMode, Tsc};
use proptest::prelude::{ProptestConfig, TestCaseError, any};
use proptest::{prop_assert_eq, proptest};

fn snap<S: Strategy>(s: &S) -> Vec<u8> {
    let mut w = SnapshotWriter::new(262_144);
    s.snapshot(&mut w);
    w.as_bytes().to_vec()
}

#[test]
fn sdk_015_overflow_drops_group_action() {
    let v = clocked_views();
    let mut s = EchoProbe::new(s3(0xFF01), &tp(3, &[]), &gt3(), &v.refview()).unwrap();
    let mut h = CtxHarness::new(&EchoProbe::DESC);
    let b = BboView {
        bid_px: Px(i64::MAX),
        bid_qty: Qty(1),
        ask_px: Px(i64::MAX),
        ask_qty: Qty(1),
        ..BboView::EMPTY
    };
    let r = h.run(|ctx| {
        s.on_event(&ev::bbo(I1, 0, b).as_ref(), &v, ctx);
        s.on_event(&ev::bbo(I1, 1, b).as_ref(), &v, ctx);
    });
    assert!(r.places.is_empty());
    let n: Vec<(u32, NoteLevel, GroupId)> = r.notes.iter().map(|n| (n.code, n.level, n.group)).collect();
    assert_eq!(
        n,
        vec![
            (ECHO_OVERFLOW, NoteLevel::P2, G1),
            (ECHO_OVERFLOW, NoteLevel::P2, G2)
        ]
    );
}

/// Valid version-2 body (no `fills`/`timer_hits`) for `groups`.
fn v2_body(groups: &[EchoGroup]) -> Vec<u8> {
    let mut b = Vec::new();
    b.extend_from_slice(&(groups.len() as u32).to_le_bytes());
    b.extend_from_slice(&[2, 0, 0, 0]);
    for g in groups {
        b.extend_from_slice(&g.id.0.to_le_bytes());
        b.extend_from_slice(&0u32.to_le_bytes());
        b.extend_from_slice(&g.fp.to_le_bytes());
        b.extend_from_slice(&(ENS0 + 7).to_le_bytes());
    }
    b
}

#[test]
fn sdk_022_restore_version_window() {
    let v = clocked_views();
    let t = gt3();
    let fresh = || EchoProbe::new(s3(0xFF01), &tp(3, &[]), &t, &v.refview()).unwrap();
    let mut src = fresh();
    src.mode = 1;
    for g in &mut src.groups {
        g.fills = 42;
        g.cooldown_until_ens = ENS0 + 9;
    }
    let v3 = snap(&src);
    let v2 = v2_body(&src.groups);
    let mut s = fresh();
    assert_eq!(
        s.restore(&mut SnapshotReader::with_desc_version(&v3, 3)),
        Ok(()),
        "SDK-022.1"
    );
    assert_eq!(snap(&s), v3, "SDK-022.1 state restored");
    let mut s = fresh();
    assert_eq!(
        s.restore(&mut SnapshotReader::with_desc_version(&v2, 2)),
        Ok(()),
        "SDK-022.2"
    );
    assert_eq!(
        (s.mode, s.groups[0].cooldown_until_ens, s.groups[0].fills),
        (2, ENS0 + 7, 0),
        "SDK-022.2"
    );
    for (row, ver, body) in [("SDK-022.3", 1u16, &v2), ("SDK-022.4", 4, &v3)] {
        let mut s = fresh();
        let before = snap(&s);
        assert_eq!(
            s.restore(&mut SnapshotReader::with_desc_version(body, ver)),
            Err(RestoreError::VersionUnsupported),
            "{row}"
        );
        assert_eq!(snap(&s), before, "{row} state unchanged");
    }
}

#[test]
fn sdk_029_state_follows_group_id() {
    let mut v = clocked_views();
    let mut s = EchoProbe::new(s3(0xFF01), &tp(3, &[]), &gt3(), &v.refview()).unwrap();
    let mut h = CtxHarness::new(&EchoProbe::DESC);
    // G3 fires at gix 2 at engine_ns 1e12.
    assert_eq!(Views::now(&v).engine_ns, 1_000_000_000_000);
    let r = fast(&mut h, &mut s, &ev::bbo(I1, 2, quote(6_500_000)), &v);
    assert_eq!(r.places.len(), 1);
    assert_eq!(r.places[0].group, G3);
    let t2 = table(&[gspec(G3), gspec(G1)]);
    assert_eq!(s.on_params(&tp(2, &[]), &t2, &v.refview()), Ok(()), "step 1");
    v.at_engine_ns(ENS0 + 10_000_000_000);
    let r = fast(&mut h, &mut s, &ev::bbo(I1, 0, quote(6_500_000)), &v);
    assert!(r.places.is_empty(), "step 2: G3's cooldown kept");
    v.at_engine_ns(ENS0 + 20_000_000_000);
    let r = fast(&mut h, &mut s, &ev::bbo(I1, 0, quote(6_500_000)), &v);
    assert_eq!(r.places.len(), 1, "step 3");
    assert_eq!(r.places[0].group, G3);
}

#[test]
fn sdk_128_cooldown_survives_restart() {
    let mut v = clocked_views();
    let t = gt3();
    let mut s = EchoProbe::new(s3(0xFF01), &tp(3, &[]), &t, &v.refview()).unwrap();
    let mut h = CtxHarness::new(&EchoProbe::DESC);
    let fire_tsc = 3_000_000_007u64;
    v.at_tsc(Tsc(fire_tsc));
    let e = Views::now(&v).engine_ns;
    let r = fast(&mut h, &mut s, &ev::bbo(I1, 0, quote(6_500_000)), &v);
    assert_eq!(r.places.len(), 1);
    let b = snap(&s);
    let has = |x: [u8; 8]| b.windows(8).any(|w| w == x);
    assert!(
        has((e + 20_000_000_000).to_le_bytes()),
        "B holds the engine_ns deadline"
    );
    assert!(!has(fire_tsc.to_le_bytes()), "B holds no TSC");
    assert!(!has(r.timers[0].at.0.to_le_bytes()), "B holds no timer TSC");
    // Restart at E + 5 s after 1 h of downtime.
    v.at_engine_ns(e + 5_000_000_000);
    let utc = Views::now(&v).utc_ns;
    v.restart_after(Tsc(7), 900, 3_600_000_000_000);
    let n = Views::now(&v);
    assert_eq!(
        (n.tsc, n.engine_ns, n.utc_ns, n.boot_seq),
        (Tsc(7), e + 5_000_000_000, utc + 3_600_000_000_000, 900)
    );
    let mut s2 = EchoProbe::new(s3(0xFF01), &tp(3, &[]), &t, &v.refview()).unwrap();
    s2.restore(&mut SnapshotReader::with_desc_version(&b, 3)).unwrap();
    let mut h2 = CtxHarness::new(&EchoProbe::DESC);
    v.at_engine_ns(e + 19_999_000_000);
    let r = fast(&mut h2, &mut s2, &ev::bbo(I1, 0, quote(6_500_000)), &v);
    assert!(r.places.is_empty(), "step 2");
    v.at_engine_ns(e + 20_000_000_000);
    let r = fast(&mut h2, &mut s2, &ev::bbo(I1, 0, quote(6_500_000)), &v);
    assert_eq!((r.places.len(), r.places[0].group), (1, G1), "step 3");
}

/// One generated event: kind, gix, value.
type Step = (u8, u16, i64);

/// Builds the event of `st` (advancing `v`'s clock) for a strategy with `n` groups.
fn event(v: &mut MockViews, n: u16, st: Step) -> ev::Ev {
    let (k, g, x) = st;
    let gix = g % n;
    let gid = GroupId(0x0300_0001 + u32::from(gix));
    let now = Views::now(v);
    v.at_tsc(Tsc(now.tsc.0 + (x.unsigned_abs() % 30_000) * 1_000_000));
    let now = Views::now(v);
    match k % 7 {
        0 => ev::bbo(I1, gix, quote(6_500_000 + x % 40_000)),
        1 => {
            let kind = if x % 3 == 0 {
                OrderUpdateKind::Rejected {
                    bits: RejectBits(1),
                    venue_code: 0,
                }
            } else {
                OrderUpdateKind::Filled {
                    d: Qty(x % 5_000),
                    px: Px(6_500_000),
                    fee: 0,
                    fee_asset: AssetId(3),
                    cum: Qty(x % 5_000),
                    done: x % 2 == 0,
                }
            };
            ev::order(
                gix,
                OrderUpdate {
                    r: OrderRef::new(V1, A1, (x % 64) as u32, 1),
                    group: gid,
                    tag: 0,
                    kind,
                },
            )
        }
        2 => ev::exec(
            gix,
            ExecUpdate {
                group: gid,
                tag: 0,
                origin: TargetOrigin::Strategy,
                kind: ExecUpdateKind::Progress {
                    leg_ix: 0,
                    d: Qty(x % 1_000),
                },
                target: [Qty(0); 4],
                filled: [Qty(0); 4],
                inflight: [Qty(0); 4],
            },
        ),
        3 => ev::timer(COOLDOWN_TAG, gid, gix),
        4 => {
            let op = (x.unsigned_abs() % 3) as u8 + 1;
            ev::manual_at(gix, op, &[x.rem_euclid(3)], op == 2)
        }
        5 => {
            let a = BandAdvice {
                flags: (x % 2) as u8,
                band: x,
            };
            ev::advice(gix, &a, now.boot_seq, now.tsc)
        }
        _ => {
            let new = if x % 2 == 0 {
                RunMode::CloseOnly
            } else {
                RunMode::Live
            };
            ev::mode(gix, RunMode::Live, new)
        }
    }
}

/// Runs `pre`, snapshots, restores a twin, compares bytes, then runs `post` on both.
fn roundtrip<S: Strategy>(
    make: &dyn Fn(&MockViews) -> S,
    desc: &'static StrategyDesc,
    n: u16,
    pre: &[Step],
    post: &[Step],
) -> Result<(), TestCaseError> {
    let mut v = clocked_views();
    let mut a = make(&v);
    let mut ha = CtxHarness::new(desc);
    for st in pre {
        let e = event(&mut v, n, *st);
        fast(&mut ha, &mut a, &e, &v);
    }
    let b1 = snap(&a);
    let mut b = make(&v);
    b.restore(&mut SnapshotReader::with_desc_version(&b1, desc.version))
        .map_err(|e| TestCaseError::fail(format!("restore: {e:?}")))?;
    prop_assert_eq!(&snap(&b), &b1);
    let mut hb = CtxHarness::new(desc);
    hb.set_next_timer_id(ha.next_timer_id());
    for st in post {
        let e = event(&mut v, n, *st);
        let ra: Recorded = fast(&mut ha, &mut a, &e, &v);
        let rb: Recorded = fast(&mut hb, &mut b, &e, &v);
        prop_assert_eq!(ra, rb);
    }
    prop_assert_eq!(snap(&a), snap(&b));
    Ok(())
}

fn steps() -> impl proptest::strategy::Strategy<Value = Vec<Step>> {
    proptest::collection::vec((0u8..7, 0u16..3, -100_000i64..100_000), 0..250)
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(500))]

    #[test]
    fn sdk_023_snapshot_roundtrip(n in 1u16..=3, pre in steps(), post in steps(), echo in any::<bool>()) {
        let specs = [gspec(G1), gspec(G2), gspec(G3)];
        let t = table(&specs[..usize::from(n)]);
        if echo {
            let mk = |v: &MockViews| EchoProbe::new(s3(0xFF01), &tp(n, &[]), &t, &v.refview()).unwrap();
            roundtrip(&mk, &EchoProbe::DESC, n, &pre, &post)?;
        } else {
            let mk = |v: &MockViews| TargetProbe::new(s3(0xFF02), &tp2(n, &[]), &t, &v.refview()).unwrap();
            roundtrip(&mk, &TargetProbe::DESC, n, &pre, &post)?;
        }
    }
}
