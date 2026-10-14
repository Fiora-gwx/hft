//! Independent invariants layered on exact wire-fixture comparisons.
use super::*;
use crate::{AcctProtocol, AcctQueryKind, RestOrigin, RestReqBuf, WallClock};
use hft_event::OrderEventKind as K;
use hft_types::{AccountId, MarketKind, Rate, Side};

fn failure(contract: u8, sample: &str, detail: &str) -> ContractFail {
    ContractFail {
        contract,
        sample: sample.into(),
        detail: detail.into(),
    }
}

/// Allocation observation is supplied by the test binary's thread-local allocator. The fixture
/// closure owns preallocated sinks; allocations by assertion/formatting code are outside the window.
pub fn check_hot_alloc(mut run: impl FnMut(), count: impl Fn() -> u64) -> Result<(), ContractFail> {
    run();
    let before = count();
    run();
    if count() != before {
        return Err(failure(5, "hot methods", "allocation after warmup"));
    }
    Ok(())
}

pub fn check_detail<P: VenueProtocol>(
    samples: &[ReportSample<'_>],
    rd: &RefData,
    ctx: &mut P::GwCtx,
) -> Result<(), ContractFail> {
    for r in check_reports::<P>(28, samples, rd, ctx)? {
        if let ReportRec::Order(e) = r {
            let valid = match e.what {
                K::Fill | K::PartialFill => [Side::Buy as u8, Side::Sell as u8].contains(&e.detail),
                K::QueryResult => (1..=7).contains(&e.detail),
                K::CancelReject | K::AmendReject => e.detail <= 1,
                _ => e.detail == 0,
            };
            if !valid {
                return Err(failure(
                    28,
                    "detail",
                    "status/reject class/fill side missing or invalid",
                ));
            }
        }
    }
    Ok(())
}

pub fn check_fee_asset<P: VenueProtocol>(
    samples: &[ReportSample<'_>],
    rd: &RefData,
    ctx: &mut P::GwCtx,
) -> Result<(), ContractFail> {
    for r in check_reports::<P>(19, samples, rd, ctx)? {
        let (fee, asset) = match r {
            ReportRec::Order(e) => {
                if e.what == K::FeeOnly && e.qty.0 != 0 {
                    return Err(failure(12, "late fee", "FeeOnly changed quantity"));
                }
                if matches!(e.what, K::Fill | K::PartialFill) && e.qty.0 == 0 {
                    return Err(failure(12, "late fee", "zero-quantity fill"));
                }
                (e.fee, e.fee_asset)
            }
            ReportRec::External(e) => (e.fee, e.fee_asset),
            _ => continue,
        };
        if fee.0 != 0 && asset.0 == 0 {
            return Err(failure(19, "fee", "nonzero fee without actual asset"));
        }
    }
    Ok(())
}

pub fn check_funding<P: VenueProtocol>(
    samples: &[ReportSample<'_>],
    rd: &RefData,
    ctx: &mut P::GwCtx,
    kind: hft_refdata::FundingSettleKind,
) -> Result<(), ContractFail> {
    use hft_refdata::FundingSettleKind as F;
    for r in check_reports::<P>(14, samples, rd, ctx)? {
        if let ReportRec::Funding(e) = r
            && (e.synth != 0
                || e.settle_ts_ns <= 0
                || match kind {
                    F::PerSymbolPush => e.inst.0 == 0,
                    F::BalanceOnlyPush => e.inst.0 != 0,
                    _ => true,
                })
        {
            return Err(failure(
                14,
                "funding",
                "synthesized, undated or wrong-scope settlement",
            ));
        }
    }
    Ok(())
}

pub fn check_external_fill<P: VenueProtocol>(
    samples: &[ReportSample<'_>],
    rd: &RefData,
    ctx: &mut P::GwCtx,
) -> Result<(), ContractFail> {
    for r in check_reports::<P>(18, samples, rd, ctx)? {
        if let ReportRec::External(e) = r
            && (e.qty.0 <= 0 || e.px.0 <= 0 || e.inst.0 == 0)
        {
            return Err(failure(18, "external fill", "missing execution facts"));
        }
    }
    Ok(())
}

/// Fixtures specify both repeated and distinct identities, including the same ID on different
/// instruments, nonpositive IDs with cumulative fallback, late fees and funding discriminators.
pub fn check_dedup<P: VenueProtocol>(
    samples: &[ReportSample<'_>],
    rd: &RefData,
    ctx: &mut P::GwCtx,
) -> Result<(), ContractFail> {
    check_reports::<P>(22, samples, rd, ctx).map(|_| ())
}
pub fn check_ctl<P: VenueProtocol>(
    samples: &[ReportSample<'_>],
    rd: &RefData,
    ctx: &mut P::GwCtx,
) -> Result<(), ContractFail> {
    check_reports::<P>(30, samples, rd, ctx).map(|_| ())
}

/// A REST fixture retains the actual request origin; response identity may not come from current
/// gateway state. `expected` is independently authored, never obtained from the decoder.
#[derive(Debug)]
pub struct RestSample<'a> {
    pub name: &'a str,
    pub origin: RestOrigin,
    pub status: u16,
    pub body: &'a [u8],
    pub rx: RxMeta,
    pub expected: &'a [ReportRec],
    pub outcome: crate::ReportOutcome,
}
pub fn check_rest<P: VenueProtocol>(
    samples: &[RestSample<'_>],
    rd: &RefData,
    ctx: &mut P::GwCtx,
) -> Result<(), ContractFail> {
    for s in samples {
        let mut sink = VecSink::default();
        let got = P::decode_rest(rd, ctx, &s.origin, s.status, s.body, &s.rx, &mut sink)
            .map_err(|_| failure(29, s.name, "REST decode failed"))?;
        if got != s.outcome || sink.reports != s.expected || usize::from(got.events) != sink.reports.len() {
            return Err(failure(
                29,
                s.name,
                "REST records/origin/outcome differ from evidence",
            ));
        }
    }
    Ok(())
}

#[derive(Debug)]
pub struct AcctCase<'a> {
    pub name: &'a str,
    pub kind: AcctQueryKind,
    pub account: AccountId,
    pub samples: &'a [AcctSample<'a>],
    pub expected: &'a [ReportRec],
}
pub fn check_account_config<P: AcctProtocol>(
    cases: &[AcctCase<'_>],
    rd: &RefData,
    t: &P::Templates,
    clock: &WallClock,
) -> Result<(), ContractFail> {
    for c in cases {
        GoldenDriver::<P>::run_acct(
            c.kind,
            c.samples,
            rd,
            c.account,
            t,
            clock,
            &mut P::AcctCtx::default(),
            c.expected,
        )
        .map_err(|_| failure(16, c.name, "account sequence differs from evidence"))?;
        let mut configs = 0;
        for r in c.expected {
            if let ReportRec::Config(h, tail) = r {
                configs += 1;
                if usize::from(h.n_lev) != tail.len() || (P::MARKET == MarketKind::Spot && !tail.is_empty()) {
                    return Err(failure(16, c.name, "leverage count or product mismatch"));
                }
                if h.account != c.account || h.venue != P::VENUE {
                    return Err(failure(16, c.name, "wrong account/venue"));
                }
            }
        }
        if c.kind != AcctQueryKind::Config || configs != 1 {
            return Err(failure(16, c.name, "expected one complete Config"));
        }
    }
    Ok(())
}
pub fn check_margin<P: AcctProtocol>(
    cases: &[AcctCase<'_>],
    rd: &RefData,
    t: &P::Templates,
    clock: &WallClock,
) -> Result<(), ContractFail> {
    for c in cases {
        GoldenDriver::<P>::run_acct(
            c.kind,
            c.samples,
            rd,
            c.account,
            t,
            clock,
            &mut P::AcctCtx::default(),
            c.expected,
        )
        .map_err(|_| failure(17, c.name, "margin sequence differs from evidence"))?;
        for r in c.expected {
            if let ReportRec::Margin(h, items) = r {
                let expected = if h.margin_balance.0 <= 0 {
                    Rate::MAX.0
                } else {
                    let n = i128::from(h.maint_margin.0) * 1_000_000_000;
                    let d = i128::from(h.margin_balance.0);
                    i64::try_from((n + d - 1) / d).unwrap_or(i64::MAX)
                };
                if P::MARKET == MarketKind::Spot
                    || h.margin_ratio.0 != expected
                    || usize::from(h.n) != items.len()
                    || items
                        .iter()
                        .any(|i| i.adl_quantile > 100 && i.adl_quantile != 255)
                {
                    return Err(failure(17, c.name, "invalid ratio, count, ADL or spot margin"));
                }
            }
        }
    }
    Ok(())
}

/// Run account history and private-report interleavings with independent contexts. An exact
/// expected batch is rechecked after every warm sequence, including after private pushes.
pub fn check_acct_isolation<P: AcctProtocol>(
    case: &AcctCase<'_>,
    reports: &[ReportSample<'_>],
    rd: &RefData,
    t: &P::Templates,
    clock: &WallClock,
    fresh_gw: impl Fn() -> P::GwCtx,
) -> Result<(), ContractFail> {
    let mut ctx = P::AcctCtx::default();
    let (mut gw, mut baseline) = (fresh_gw(), fresh_gw());
    for _ in 0..1000 {
        GoldenDriver::<P>::run_acct(
            case.kind,
            case.samples,
            rd,
            case.account,
            t,
            clock,
            &mut ctx,
            case.expected,
        )
        .map_err(|_| failure(27, case.name, "account context depends on prior sequence"))?;
    }
    for _ in 0..10_000 {
        for s in reports {
            let c = P::classify_report(s.op, s.payload.get(..64).unwrap_or(s.payload));
            let mut out = VecSink::default();
            let mut reference = VecSink::default();
            let actual = P::decode_report(rd, &mut gw, c, s.payload, &s.rx, &mut out)
                .map_err(|_| failure(27, s.name, "interleaved report failed"))?;
            let expected = P::decode_report(rd, &mut baseline, c, s.payload, &s.rx, &mut reference)
                .map_err(|_| failure(27, s.name, "baseline report failed"))?;
            if actual != expected || out.reports != reference.reports {
                return Err(failure(27, s.name, "account history changed private report"));
            }
        }
    }
    GoldenDriver::<P>::run_acct(
        case.kind,
        case.samples,
        rd,
        case.account,
        t,
        clock,
        &mut ctx,
        case.expected,
    )
    .map_err(|_| failure(27, case.name, "account decode depends on gateway context"))
}

#[derive(Debug)]
pub struct BboSnapshotSample<'a> {
    pub name: &'a str,
    pub insts: &'a [hft_types::InstId],
    pub body: &'a [u8],
    pub rx: RxMeta,
    pub expected: &'a [MdRec],
}
pub fn check_bbo_refresh<P: VenueProtocol>(
    samples: &[BboSnapshotSample<'_>],
    rd: &RefData,
    ctx: &mut P::FeedCtx,
) -> Result<(), ContractFail> {
    let mut ping = [0; 512];
    let n = P::app_ping(crate::Role::Md, 0, &mut ping);
    if (P::bbo_probe() == crate::PingKind::WsPing) != (n == 0) || n > ping.len() {
        return Err(failure(24, "probe", "probe frame and kind disagree"));
    }
    for s in samples {
        let mut req = RestReqBuf::default();
        if P::bbo_snapshot_request(rd, ctx, s.insts, &mut req) != Ok(true)
            || req.redacted
            || req.path_query.is_empty()
        {
            return Err(failure(24, s.name, "missing public snapshot request"));
        }
        let mut out = VecSink::default();
        let n = P::decode_bbo_snapshot(rd, ctx, s.insts, s.body, &s.rx, &mut out)
            .map_err(|_| failure(24, s.name, "snapshot decode failed"))?;
        if out.md != s.expected
            || usize::from(n) != out.md.len()
            || out
                .md
                .iter()
                .any(|r| !matches!(r, MdRec::Bbo(b) if s.insts.contains(&b.h.inst)))
        {
            return Err(failure(24, s.name, "wrong BBO identity/count/bytes"));
        }
    }
    Ok(())
}
pub fn check_snapshot_isolation<P: VenueProtocol>(
    snapshots: &[BboSnapshotSample<'_>],
    md: &[MdSample<'_>],
    rd: &RefData,
    fresh: impl Fn() -> P::FeedCtx,
) -> Result<(), ContractFail> {
    let mut warm = fresh();
    for _ in 0..10_000 {
        for s in md {
            let c = P::classify(s.route, s.op, s.payload.get(..64).unwrap_or(s.payload));
            P::decode_md(rd, &mut warm, c, s.payload, &s.rx, &mut VecSink::default())
                .map_err(|_| failure(25, s.name, "market-data decode failed"))?;
        }
    }
    check_bbo_refresh::<P>(snapshots, rd, &mut fresh())?;
    check_bbo_refresh::<P>(snapshots, rd, &mut warm)?;
    for s in md {
        let c = P::classify(s.route, s.op, s.payload.get(..64).unwrap_or(s.payload));
        let (mut a, mut b) = (VecSink::default(), VecSink::default());
        P::decode_md(rd, &mut fresh(), c, s.payload, &s.rx, &mut a)
            .map_err(|_| failure(25, s.name, "fresh decode failed"))?;
        P::decode_md(rd, &mut warm, c, s.payload, &s.rx, &mut b)
            .map_err(|_| failure(25, s.name, "warm decode failed"))?;
        if a.md != b.md {
            return Err(failure(25, s.name, "snapshot changed market-data output"));
        }
    }
    Ok(())
}

/// Check secrets and redaction on independently identified request families. API keys may be in
/// their transport headers; the sentinel is private signing material, which must never be emitted.
pub fn check_session(
    requests: &[(&str, &RestReqBuf, bool)],
    frames: &[(&str, &[u8])],
    secret: &[u8],
) -> Result<(), ContractFail> {
    if secret.is_empty() {
        return Err(failure(7, "secret", "empty sentinel"));
    }
    for (name, r, signed) in requests {
        if r.redacted != *signed {
            return Err(failure(7, name, "incorrect redaction"));
        }
        for b in [r.path_query.as_slice(), r.headers.as_slice(), r.body.as_slice()] {
            if b.windows(secret.len()).any(|s| s == secret) {
                return Err(failure(7, name, "private signing material in request"));
            }
        }
    }
    for (name, b) in frames {
        if b.windows(secret.len()).any(|s| s == secret) {
            return Err(failure(7, name, "private signing material in frame"));
        }
    }
    Ok(())
}
