//! Advice payloads, staleness and `AdviceOut` capacity (SDK-171, SDK-172, SDK-173, SDK-211).

#![allow(
    clippy::disallowed_types,
    clippy::disallowed_macros,
    clippy::disallowed_methods
)] // allow-list: test code, not a hot path

mod common;

use common::*;
use hft_strategy_sdk::testing::{CtxHarness, MockViews, ModelHarness, ev};
use hft_strategy_sdk::*;
use hft_types::{GROUP_ALL, Tsc};

/// `MockViews` at `now.tsc = tsc`, `boot_seq = 100`, `tsc_per_ms` ticks per ms.
fn views(tsc: u64, tsc_per_ms: u64) -> MockViews {
    let mut v = gt3_views();
    let clk = EngineClockAnchor {
        base_engine_ns: 0,
        base_tsc: Tsc(0),
        tsc_hz_milli: tsc_per_ms * 1_000_000,
        utc_tsc: Tsc(0),
        utc_ns: UTC0,
    };
    v.at(Now::new(Tsc(tsc), 200, 100, true, clk));
    assert_eq!(Views::now(&v).tsc_per_ms, tsc_per_ms);
    v
}

fn probe(v: &MockViews) -> TargetProbe {
    TargetProbe::new(s3(0xFF02), &tp2(3, &[]), &gt3(), &v.refview()).unwrap()
}

const P: BandAdvice = BandAdvice {
    flags: 0,
    band: 6_500_000,
};

#[test]
fn sdk_171_payload_roundtrip_unknown_version() {
    let mut buf = [0u8; BandAdvice::MAX_LEN];
    let mut w = PayloadWriter::new(&mut buf, BandAdvice::MAX_LEN);
    P.encode(&mut w).unwrap();
    assert_eq!((w.len(), w.as_bytes()[0]), (16, 1), "step 1");
    let bytes = w.as_bytes().to_vec();
    assert_eq!(BandAdvice::decode(&bytes), Ok(P), "step 1");
    assert_eq!(ADVICE_MAX_AGE_MS, 1_000);
    let v = views(10_000, 1);
    let mut s = probe(&v);
    let mut h = CtxHarness::new(&TargetProbe::DESC);
    let r = fast(&mut h, &mut s, &ev::advice(0, &P, 150, Tsc(9_500)), &v);
    assert_eq!(r.targets.len(), 1, "step 2");
    assert_eq!(r.targets[0].group, G1);
    let mut b2 = bytes.clone();
    b2[0] = 2;
    assert!(matches!(BandAdvice::decode(&b2), Err(PayloadError::Version(2))));
    let r = fast(&mut h, &mut s, &ev::advice_raw(0, 1, &b2, 150, Tsc(9_500)), &v);
    assert!(r.targets.is_empty(), "step 3");
}

/// The rows' ages ("29_000_000_000 = 1 000 ms before 30_000_000_000") need 1 000 000 ticks per
/// ms; the TESTS.md preamble's 3 000 000 would make row .2 fresh (333 ms), so the rows win.
#[test]
fn sdk_172_advice_staleness() {
    let v = views(30_000_000_000, 1_000_000);
    let rows: [(&str, u64, u64, bool); 4] = [
        ("SDK-172.1", 150, 29_000_000_000, true),
        ("SDK-172.2", 150, 28_999_999_999, false),
        ("SDK-172.3", 99, 30_000_000_000, false),
        ("SDK-172.4", 100, 29_500_000_000, true),
    ];
    for (row, seq, tsc, fresh) in rows {
        let mut s = probe(&v);
        let mut h = CtxHarness::new(&TargetProbe::DESC);
        let r = fast(&mut h, &mut s, &ev::advice(0, &P, seq, Tsc(tsc)), &v);
        assert_eq!(r.targets.len(), usize::from(fresh), "{row}");
    }
}

/// Emits `n` payloads of `len` bytes in one run; returns (results, outputs, full count).
fn emit(
    n: usize,
    len: usize,
    group: hft_types::GroupId,
    kind: u16,
) -> (Vec<Result<(), AdviceFull>>, usize, u32) {
    let mut h = ModelHarness::new();
    let bytes = vec![7u8; len];
    let mut res = Vec::new();
    let out = h.run(|o| {
        for _ in 0..n {
            res.push(o.emit_raw(group, kind, &bytes));
        }
    });
    (res, out.len(), h.full_count())
}

#[test]
fn sdk_173_advice_out_capacity() {
    let (r, n, full) = emit(128, 8, G1, 1);
    assert!(r.iter().all(Result::is_ok) && n == 128 && full == 0, "SDK-173.1");
    let (r, n, full) = emit(129, 8, G1, 1);
    assert_eq!((r[128], n, full), (Err(AdviceFull), 128, 1), "SDK-173.2");
    assert!(r[..128].iter().all(Result::is_ok), "SDK-173.2");
    let (r, n, _) = emit(32, 4_096, G1, 1);
    assert!(r.iter().all(Result::is_ok) && n == 32, "SDK-173.3");
    let (r, n, _) = emit(33, 4_096, G1, 1);
    assert_eq!((r[32], n), (Err(AdviceFull), 32), "SDK-173.4");
    let (r, n, _) = emit(1, 4_097, G1, 1);
    assert_eq!((r[0], n), (Err(AdviceFull), 0), "SDK-173.5");
    assert_eq!(emit(1, 8, G1, 0).0[0], Err(AdviceFull), "SDK-173.6");
    assert_eq!(emit(1, 8, G1, 0xFF00).0[0], Err(AdviceFull), "SDK-173.7");
    assert_eq!(emit(1, 8, G1, 0xFEFF).0[0], Ok(()), "SDK-173.8");
    assert_eq!(emit(1, 8, GROUP_ALL, 1).0[0], Err(AdviceFull), "SDK-173.9");
}

/// `BandAdvice` with one extra byte (SDK-211.2).
struct Long;

impl AdvicePayload for Long {
    const LOCAL_KIND: u16 = 1;
    const MAX_LEN: usize = 16;
    fn encode(&self, w: &mut PayloadWriter<'_>) -> Result<(), PayloadFull> {
        P.encode(w)?;
        w.put_u8(0)
    }
    fn decode(_: &[u8]) -> Result<Self, PayloadError> {
        Ok(Long)
    }
}

#[test]
fn sdk_211_payload_bounds() {
    let mut buf = [0u8; 32];
    let mut w = PayloadWriter::new(&mut buf, BandAdvice::MAX_LEN);
    assert_eq!(P.encode(&mut w), Ok(()), "SDK-211.1");
    let bytes = w.as_bytes().to_vec();
    let mut buf = [0u8; 32];
    let mut w = PayloadWriter::new(&mut buf, Long::MAX_LEN);
    assert_eq!(Long.encode(&mut w), Err(PayloadFull), "SDK-211.2");
    assert!(BandAdvice::decode(&bytes[..15]).is_err(), "SDK-211.3");
    assert!(BandAdvice::decode(&[]).is_err(), "SDK-211.4");
}
