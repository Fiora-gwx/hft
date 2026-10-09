//! Token buckets, reserve, refill, backoff and restart (RSK-036–RSK-040).
#![allow(
    clippy::disallowed_types,
    clippy::disallowed_macros,
    clippy::disallowed_methods
)] // allow-list: test code, not a hot path

mod common;
use common::*;

use hft_event::NoteLevel;
use hft_risk::{AccountScope, IntentView};
use hft_types::{RejectBits, Side, Tif};

const TOKEN: i64 = 1_000_000_000;

/// N: non-risk-reducing Open on a1.
fn n_it() -> IntentView {
    open(1, G1, A1, Side::Buy, "0.0001", "50000.00", Tif::Gtc)
}
/// H: risk-reducing Hedge on a1.
fn h_it() -> IntentView {
    hedge(1, G1, A1, Side::Buy, "0.0001", "50001.00", "0.0001")
}
/// Risk-reducing commits on (B, 1).
fn hb_it() -> IntentView {
    hedge(1, G1, B1, Side::Sell, "0.001", "49995.0", "0.001")
}
fn link_l() -> IntentView {
    link_open(
        1,
        G1,
        A1,
        Side::Buy,
        "0.01",
        "50000.00",
        (1, B, B1, Side::Sell, "0.01", "495"),
    )
}

fn check(w: &mut W, it: &IntentView, want: Option<RejectBits>, id: &str) {
    let r = w.eval(it).map(|_| ());
    match want {
        Some(b) => assert_eq!(r, Err(b), "{id}"),
        None => assert_eq!(r, Ok(()), "{id}"),
    }
}

fn r0_limits(l10: u32, l60: u32) -> W {
    let mut sp = r0_spec();
    sp.limits.retain(|(v, _)| *v != A);
    sp.limits.extend(order_limits(A, l10, l60));
    let rd = load(&sp);
    world(rd, p0_for(rd, 3), MC, CAPS)
}

#[test]
fn rsk_036_rr_reserve() {
    use RejectBits as R;
    let mut w = r0();
    w.commits(&h_it(), 79);
    check(&mut w, &n_it(), None, "RSK-036.1");
    let mut w = r0();
    w.commits(&h_it(), 80);
    check(&mut w, &n_it(), Some(R::RATE), "RSK-036.2 N");
    check(&mut w, &h_it(), None, "RSK-036.2 H");
    let mut w = r0();
    w.commits(&h_it(), 99);
    check(&mut w, &h_it(), None, "RSK-036.3");
    w.commits(&h_it(), 1);
    check(&mut w, &h_it(), Some(R::RATE), "RSK-036.4");
    {
        let mut w = r0_limits(1_000, 600);
        w.commits(&h_it(), 479);
        check(&mut w, &n_it(), None, "RSK-036.5 a");
        w.commits(&h_it(), 1);
        check(&mut w, &n_it(), Some(R::RATE), "RSK-036.5 b");
    }
    {
        let mut w = r0_with(|p| with_venue(&p, A, 1, |v| v.inst_bucket_per_s = 5));
        w.commits(&h_it(), 5);
        check(&mut w, &h_it(), Some(R::RATE), "RSK-036.6 a");
        let it = open(1, G3, A2, Side::Buy, "0.001", "2500.00", Tif::Gtc);
        check(&mut w, &it, None, "RSK-036.6 b");
    }
    {
        let mut w = r0();
        w.hit(A, 1);
        w.now += MS;
        check(&mut w, &n_it(), Some(R::RATE_BACKOFF), "RSK-036.7 N");
        check(&mut w, &h_it(), Some(R::RATE_BACKOFF), "RSK-036.7 H");
    }
    {
        let mut w = r0();
        w.hit(B, 1);
        check(&mut w, &link_l(), Some(R::RATE_BACKOFF), "RSK-036.8");
    }
    {
        let mut w = r0();
        w.commits(&hb_it(), 79);
        let r = w.eval_commit(&link_l());
        assert!(r.is_ok(), "RSK-036.9 first: {r:?}");
        check(&mut w, &link_l(), None, "RSK-036.9 second");
        let rows = section17(&w.snapshot());
        assert_eq!(
            bucket_row(&rows, 1, B, 0).unwrap().tokens_10s,
            21 * TOKEN,
            "RSK-036.9 (B, 1) untouched"
        );
    }
    {
        let mut w = r0();
        w.commits(&hb_it(), 80);
        check(&mut w, &link_l(), Some(R::RATE), "RSK-036.10");
    }
    {
        let mut w = r0();
        let it = open(3, GS, A1, Side::Buy, "0.0001", "50000.00", Tif::Gtc);
        w.commits(&it, 80);
        check(&mut w, &it, Some(R::RATE), "RSK-036.11");
    }
    {
        let mut w = r0();
        w.commits(&h_it(), 80);
        w.hit(B, 1);
        check(&mut w, &link_l(), Some(R::RATE), "RSK-036.12");
    }
    {
        let mut w = r0();
        w.commits(&hb_it(), 80);
        w.hit(B, 1);
        check(&mut w, &link_l(), Some(R::RATE_BACKOFF), "RSK-036.13");
    }
    {
        let mut w = r0();
        w.hit(A, 1);
        w.commits(&hb_it(), 80);
        check(&mut w, &link_l(), Some(R::RATE_BACKOFF), "RSK-036.14");
    }
}

/// Commits H at the current time until `RATE`; returns the number admitted.
fn exhaust(w: &mut W) -> usize {
    let mut k = 0;
    loop {
        match w.eval_commit(&h_it()) {
            Ok(_) => k += 1,
            Err(b) => {
                assert_eq!(b, RejectBits::RATE);
                return k;
            }
        }
        assert!(k < 10_000);
    }
}

#[test]
fn rsk_037_lazy_refill() {
    let mut w = r0();
    assert_eq!(exhaust(&mut w), 100);
    w.now = T0 + 99_999_999;
    check(&mut w, &h_it(), Some(RejectBits::RATE), "RSK-037.1");
    w.now = T0 + 100_000_000;
    check(&mut w, &h_it(), None, "RSK-037.2");
    let mut w = r0();
    exhaust(&mut w);
    w.now = T0 + 3_600 * SEC;
    assert_eq!(exhaust(&mut w), 100, "RSK-037.3");
    let mut w = r0();
    let mut p = w.p.clone();
    p.rate_headroom = r("0.1");
    assert_eq!(w.risk.set_params(&p, AccountScope::All), Ok(()));
    assert_eq!(exhaust(&mut w), 90, "RSK-037.4");
    let mut w = r0_limits(1_000, 600);
    assert_eq!(exhaust(&mut w), 600, "RSK-037.5 a");
    w.now = T0 + 99_999_999;
    check(&mut w, &h_it(), Some(RejectBits::RATE), "RSK-037.5 b");
    w.now = T0 + 100_000_000;
    check(&mut w, &h_it(), None, "RSK-037.5 c");
}

#[test]
fn rsk_038_backoff_exponential() {
    let mut w = r0();
    let rows0 = section17(&w.snapshot());
    let k_of = |w: &W| bucket_row(&section17(&w.snapshot()), 1, A, 0).unwrap();
    // Step 1.
    let note = w.hit(A, 1);
    let b = k_of(&w);
    assert_eq!(
        (b.backoff_until_ens, b.backoff_k, b.tokens_10s, b.tokens_min),
        (T0 + 1_000 * MS, 0, 0, 0)
    );
    assert_eq!(note.level, NoteLevel::P2);
    assert_eq!(&note.v[..3], &[258, 0, (T0 + 1_000 * MS) as u64]);
    // Step 2.
    w.now = T0 + 999_999_999;
    check(&mut w, &h_it(), Some(RejectBits::RATE_BACKOFF), "RSK-038 step 2");
    let now = w.nowv();
    assert!(w.risk.view_for(acc(1), A1, &now).rate_backoff);
    assert!(!w.risk.view_for(acc(2), A1, &now).rate_backoff);
    assert!(!w.risk.view_for(acc(1), B1, &now).rate_backoff);
    // Step 3.
    w.now = T0 + SEC;
    check(&mut w, &h_it(), None, "RSK-038 step 3 H");
    check(&mut w, &n_it(), Some(RejectBits::RATE), "RSK-038 step 3 N");
    // Step 4.
    for (at, k, until) in [
        (T0 + 1_500 * MS, 1, T0 + 3_500 * MS),
        (T0 + 4 * SEC, 2, T0 + 8 * SEC),
        (T0 + 9 * SEC, 3, T0 + 17 * SEC),
        (T0 + 18 * SEC, 4, T0 + 26 * SEC),
    ] {
        w.now = at;
        w.hit(A, 1);
        let b = k_of(&w);
        assert_eq!((b.backoff_k, b.backoff_until_ens), (k, until), "step 4 at {at}");
    }
    // Step 5.
    w.now = T0 + 34 * SEC;
    w.hit(A, 1);
    let b = k_of(&w);
    assert_eq!((b.backoff_k, b.backoff_until_ens), (0, T0 + 35 * SEC), "step 5");
    // Step 6.
    let t6 = T0 + 35 * SEC + 7_999_999_999;
    w.now = t6;
    w.hit(A, 1);
    let b = k_of(&w);
    assert_eq!((b.backoff_k, b.backoff_until_ens), (1, t6 + 2_000 * MS), "step 6");
    // (A, 2) and (B, 1) untouched.
    let rows1 = section17(&w.snapshot());
    for (a, v) in [(2u8, A), (1, B)] {
        assert_eq!(
            bucket_row(&rows0, a, v, 0),
            bucket_row(&rows1, a, v, 0),
            "({a}, {v:?})"
        );
    }
}

#[test]
fn rsk_039_only_commit_consumes() {
    let mut w = r0();
    for k in 0..1_000 {
        check(&mut w, &h_it(), None, &format!("RSK-039 evaluate {k}"));
    }
    assert_eq!(exhaust(&mut w), 100);
}

#[test]
fn rsk_040_bucket_survives_restart() {
    let mut w = r0();
    assert_eq!(exhaust(&mut w), 100);
    w.hit(B, 1);
    let snap = w.snapshot();
    w.restore_from(&snap);
    let base = T0 + 50 * MS;
    w.risk.set_run_start(base);
    let at = |w: &mut W, ens: i64, it: &IntentView| {
        w.mv.tsc = (ens - base) as u64;
        let now = now_run(base, ens);
        w.risk.evaluate(it, &w.mv, &now).map(|_| ())
    };
    assert_eq!(
        at(&mut w, T0 + 50 * MS, &h_it()),
        Err(RejectBits::RATE),
        "step 3 a"
    );
    assert_eq!(at(&mut w, T0 + 100 * MS, &h_it()), Ok(()), "step 3 b");
    let hb = hedge(1, G1, B1, Side::Sell, "0.01", "49995.0", "0.01");
    assert_eq!(
        at(&mut w, T0 + 999 * MS, &hb),
        Err(RejectBits::RATE_BACKOFF),
        "step 4 a"
    );
    assert_eq!(at(&mut w, T0 + 1_000 * MS, &hb), Ok(()), "step 4 b");
}
