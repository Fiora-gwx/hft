//! Reusable evidence-driven protocol checks. Callers supply independent wire fixtures.
use super::*;
use crate::{DeadManSpec, DecodeErr, EncodeErr, StaleGuard};
use hft_refdata::{DeadManCap, VenueCaps};
use hft_types::ClientOrderId;

fn fail(contract: u8, sample: &str, detail: &str) -> ContractFail {
    ContractFail {
        contract,
        sample: sample.into(),
        detail: detail.into(),
    }
}

/// Check the countdown arithmetic in u64 so overflow cannot turn an invalid deadline valid.
pub fn check_dead_man(spec: DeadManSpec, caps: &VenueCaps) -> Result<(), ContractFail> {
    let (kind, min, max, timeout, refresh) = match spec {
        DeadManSpec::None => (DeadManCap::None, 0, 0, 0, 0),
        DeadManSpec::CountdownPerInst {
            min_ms,
            timeout_ms,
            refresh_ms,
            ..
        } => (DeadManCap::CountdownPerInst, min_ms, 0, timeout_ms, refresh_ms),
        DeadManSpec::CountdownAccount {
            min_ms,
            max_ms,
            timeout_ms,
            refresh_ms,
            ..
        } => (
            DeadManCap::CountdownAccount,
            min_ms,
            max_ms,
            timeout_ms,
            refresh_ms,
        ),
        DeadManSpec::OnDisconnect { window_ms } => (DeadManCap::OnDisconnect, 0, 0, window_ms, 0),
        DeadManSpec::Schedule {
            min_ahead_ms,
            timeout_ms,
            refresh_ms,
            ..
        } => (DeadManCap::Schedule, min_ahead_ms, 0, timeout_ms, refresh_ms),
    };
    if (kind, min, max, timeout, refresh)
        != (
            caps.dms,
            caps.dms_min_ms,
            caps.dms_max_ms,
            caps.dms_timeout_ms,
            caps.dms_refresh_ms,
        )
    {
        return Err(fail(15, "dead_man", "capability fields differ from protocol"));
    }
    if matches!(
        kind,
        DeadManCap::CountdownPerInst | DeadManCap::CountdownAccount | DeadManCap::Schedule
    ) && (u64::from(timeout) < u64::from(min).max(u64::from(refresh) * 2)
        || refresh == 0
        || (max != 0 && timeout > max))
    {
        return Err(fail(11, "dead_man", "invalid countdown deadline"));
    }
    Ok(())
}

pub fn check_default_caps<P: VenueProtocol>(caps: &VenueCaps) -> Result<(), ContractFail> {
    if caps.venue != P::VENUE || caps.market != P::MARKET {
        return Err(fail(15, "default_caps", "venue or market mismatch"));
    }
    check_dead_man(P::dead_man(), caps)?;
    if matches!(
        P::stale_guard(),
        StaleGuard::RecvWindow { default_ms: 0 } | StaleGuard::ExpireAt { default_ms: 0 }
    ) {
        return Err(fail(10, "stale_guard", "zero protection window"));
    }
    for rule in P::rate_rules() {
        if rule.window_ms == 0 {
            return Err(fail(15, "rate_rules", "zero rate window"));
        }
    }
    Ok(())
}

pub fn check_cid<P: VenueProtocol>(ids: &[ClientOrderId], foreign: &[&[u8]]) -> Result<(), ContractFail> {
    for id in ids {
        let mut out = [0; 48];
        let n = P::render_cid(id, &mut out);
        let Some(text) = out.get(..n).filter(|s| !s.is_empty()) else {
            return Err(fail(13, "client id", "empty or overlong encoded id"));
        };
        if P::parse_cid(text) != Some(*id) {
            return Err(fail(13, "client id", "round trip lost identity fields"));
        }
    }
    for text in foreign {
        if P::parse_cid(text).is_some() {
            return Err(fail(13, "foreign id", "accepted non-system id"));
        }
    }
    Ok(())
}

/// Compare `peek`'s identity and sequence with independently specified observations. Include a
/// corrupt-price fixture with unchanged IDs to prove `peek` does not parse price fields.
pub fn check_peek<P: VenueProtocol>(
    samples: &[(MdSample<'_>, crate::Peek)],
    rd: &RefData,
    ctx: &P::FeedCtx,
) -> Result<(), ContractFail> {
    for (s, want) in samples {
        let class = P::classify(s.route, s.op, s.payload.get(..64).unwrap_or(s.payload));
        if P::peek(rd, ctx, class, s.payload) != Ok(*want) {
            return Err(fail(2, s.name, "peek differs from independent identity/sequence"));
        }
    }
    Ok(())
}

pub fn check_decode_errors<P: VenueProtocol>(
    samples: &[(MdSample<'_>, DecodeErr)],
    rd: &RefData,
    ctx: &mut P::FeedCtx,
) -> Result<(), ContractFail> {
    for (s, want) in samples {
        let class = P::classify(s.route, s.op, s.payload.get(..64).unwrap_or(s.payload));
        let mut out = VecSink::default();
        if P::decode_md(rd, ctx, class, s.payload, &s.rx, &mut out) != Err(*want) || !out.md.is_empty() {
            return Err(fail(4, s.name, "wrong error category or partial events on error"));
        }
    }
    Ok(())
}

impl<P: VenueProtocol> GoldenDriver<P> {
    /// The expected bytes are a reviewed wire golden, independent of the encoder under test.
    pub fn run_encoded(
        sample: &str,
        expected: &[u8],
        buf: &mut [u8],
        encode: impl FnOnce(&mut [u8]) -> Result<usize, EncodeErr>,
    ) -> Result<(), GoldenFail> {
        let n = encode(buf).map_err(|_| GoldenFail {
            sample: sample.into(),
            record: 0,
            offset: 0,
        })?;
        let got = buf.get(..n).ok_or_else(|| GoldenFail {
            sample: sample.into(),
            record: 0,
            offset: buf.len(),
        })?;
        Self::compare(sample, &[got.to_vec()], &[expected.to_vec()])
    }
}
