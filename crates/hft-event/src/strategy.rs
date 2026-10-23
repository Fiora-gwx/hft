//! Strategy payload containers: `Advice` (`AdviceHdr`, `AdviceRecord`) and the SPB1 container
//! (`StrategyParamsHdr` + `GroupSpec` + opaque body; `parse_spb1`). Payload contents belong to the
//! strategy crates (hft-strategy-sdk); this crate never parses them.
// zerocopy's derive helpers inside `event_body!` expansions carry no Debug impl.
#![allow(missing_debug_implementations)]

use hft_refdata::{GroupSpecView, LegView};
use hft_types::{AccountId, AssetId, GroupId, InstId, StrategyId, Tsc, VenueId};
use zerocopy::{FromBytes, IntoBytes, TryFromBytes};

use crate::bytes::{put, round8, u8_at, u16_at, u32_at, u64_at};
use crate::codes;
use crate::macros::{layout, wire_enum};
use crate::schema::FieldTy;
use crate::{DecodeErr, EncodeErr, EventKind};

crate::event_body! {
    /// `Advice` (0x0503, Var) prefix; logical body = `AdviceHdr` + `payload[len]` (zero-padded to 8).
    pub struct AdviceHdr : var(EventKind::ADVICE, 1, 4_096), FromBytes {
        pub strategy: StrategyId,
        /// 0 = not account-specific.
        pub account: AccountId,
        pub local_kind: u16,
        /// `GROUP_NONE` = not group-specific; `GROUP_ALL` illegal.
        pub group: GroupId,
        pub len: u16,
        pub _pad: [u8; 6],
        pub basis_seq: u64,
        pub basis_tsc: Tsc,
    }
}
layout!(AdviceHdr, 32, 8; strategy @ 0, account @ 1, local_kind @ 2, group @ 4, len @ 8, _pad @ 10,
    basis_seq @ 16, basis_tsc @ 24);

/// Upper bound of `AdviceHdr.len`; larger outputs are split by the strategy.
pub const ADVICE_MAX_PAYLOAD: usize = 4096;

/// Advice body as carried in the advice ring (`AdviceHdr` + payload, padded to 8 B).
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub struct AdviceRecord<'a> {
    pub hdr: AdviceHdr,
    pub payload: &'a [u8],
}

impl<'a> AdviceRecord<'a> {
    /// `32 + payload.len()` rounded up to 8.
    #[inline]
    pub fn encoded_len(&self) -> usize {
        round8(32 + self.payload.len())
    }

    /// Writes header, payload and zero padding; returns `encoded_len`.
    pub fn encode_into(&self, out: &mut [u8]) -> Result<usize, EncodeErr> {
        if self.payload.len() > ADVICE_MAX_PAYLOAD {
            return Err(EncodeErr::TooLong);
        }
        if usize::from(self.hdr.len) != self.payload.len() {
            return Err(EncodeErr::Count);
        }
        let n = self.encoded_len();
        let Some(dst) = out.get_mut(..n) else {
            return Err(EncodeErr::BufTooSmall);
        };
        put(dst, 0, self.hdr.as_bytes());
        put(dst, 32, self.payload);
        if let Some(pad) = dst.get_mut(32 + self.payload.len()..) {
            pad.fill(0);
        }
        Ok(n)
    }

    /// Reads one advice body (`b` may be longer; the payload is `b[32..32 + len]`).
    pub fn read(b: &'a [u8]) -> Result<Self, DecodeErr> {
        let (hdr, rest) = AdviceHdr::read_from_prefix(b).map_err(|_| DecodeErr::Short)?;
        let len = usize::from(hdr.len);
        if len > ADVICE_MAX_PAYLOAD {
            return Err(DecodeErr::TooLong);
        }
        let payload = rest.get(..len).ok_or(DecodeErr::Short)?;
        Ok(AdviceRecord { hdr, payload })
    }
}

wire_enum!(
    /// `StrategyParamsHdr.source`.
    ParamSource: u8 = "ParamSource" { Pipeline = 1, Manual = 2 }
);

crate::event_body! {
    /// SPB1 header (48 B).
    pub struct StrategyParamsHdr : plain, TryFromBytes {
        pub strategy: StrategyId,
        pub account: AccountId,
        pub kind_id: u16,
        pub schema_id: u32,
        pub schema_ver: u16,
        pub source: ParamSource,
        pub _pad: u8,
        pub n_groups: u16,
        pub _pad2: u16,
        pub version: u64,
        pub generated_at_ns: i64,
        /// Multiple of 8.
        pub body_len: u32,
        /// Largest group counter ever allocated by this instance (≤ `GroupId::N_MAX`).
        pub group_ctr_hwm: u32,
        pub schema_digest: u64,
    }
}
layout!(StrategyParamsHdr, 48, 8; strategy @ 0, account @ 1, kind_id @ 2, schema_id @ 4, schema_ver @ 8,
    source @ 10, _pad @ 11, n_groups @ 12, _pad2 @ 14, version @ 16, generated_at_ns @ 24, body_len @ 32,
    group_ctr_hwm @ 36, schema_digest @ 40);

crate::event_body! {
    /// One leg of a `GroupSpec` (16 B).
    pub struct LegDef : plain, FromBytes {
        pub inst: InstId,
        pub venue: VenueId,
        pub kind: u8 = FieldTy::Enum(&codes::LEG_KIND),
        pub _pad: u16,
        /// Chain legs only (optional); Cex legs 0.
        pub route: u32,
        pub _pad2: u32,
    }
}
layout!(LegDef, 16, 4; inst @ 0, venue @ 4, kind @ 5, _pad @ 6, route @ 8, _pad2 @ 12);

impl LegDef {
    pub const CEX: u8 = 1;
    pub const CHAIN: u8 = 2;
}

crate::event_body! {
    /// One group of a strategy instance on one account (80 B).
    pub struct GroupSpec : plain, FromBytes {
        pub id: GroupId,
        pub asset: AssetId,
        pub n_legs: u8,
        pub open_allowed: u8,
        /// First `n_legs` valid, the rest all zero.
        pub legs: [LegDef; 4],
        pub _rsv: u64,
    }
}
layout!(GroupSpec, 80, 8; id @ 0, asset @ 4, n_legs @ 6, open_allowed @ 7, legs @ 8, _rsv @ 72);

impl GroupSpec {
    /// hft-refdata `check_group` view: `asset` and the first `n_legs` legs' `(inst, venue)`, built
    /// in caller storage (`GroupSpecView` borrows a `LegView` slice that `GroupSpec` does not hold).
    pub fn view<'a>(&self, legs: &'a mut [LegView; 4]) -> GroupSpecView<'a> {
        let n = usize::from(self.n_legs).min(4);
        for (d, s) in legs.iter_mut().zip(self.legs.iter()) {
            *d = LegView {
                inst: s.inst,
                venue: s.venue,
            };
        }
        GroupSpecView {
            asset: self.asset,
            open_allowed: self.open_allowed != 0,
            legs: legs.get(..n).unwrap_or(&[]),
        }
    }
}

/// Zero-copy view of a parsed SPB1 blob; `body` is returned untouched.
#[derive(Copy, Clone, Debug)]
pub struct SpbView<'a> {
    pub hdr: &'a StrategyParamsHdr,
    pub groups: &'a [GroupSpec],
    pub body: &'a [u8],
}

/// Container-level SPB1 errors.
#[repr(u8)]
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
pub enum ParamsErr {
    Truncated = 1,
    BadLen = 2,
    BadGroup = 3,
    BadLeg = 4,
    Padding = 5,
    DupGroup = 6,
    BadHdr = 7,
    /// The blob is not 8-byte aligned (zero-copy view impossible).
    Misaligned = 8,
}

const SPB_HDR: usize = 48;
const SPB_GROUP: usize = 80;

fn leg_off(g: usize, i: usize) -> usize {
    g + 8 + 16 * i
}

/// Number of legs whose padding is checked as "used": `n_legs` when legal, else all four
/// (an illegal `n_legs` is reported as `BadGroup` in step 2, not as `Padding`).
fn used_legs(n_legs: u8) -> usize {
    if (1..=4).contains(&n_legs) {
        usize::from(n_legs)
    } else {
        4
    }
}

/// Container-level SPB1 parse (README "容器级校验", steps 1–3 in order, first error wins).
pub fn parse_spb1(blob: &[u8]) -> Result<SpbView<'_>, ParamsErr> {
    if blob.len() < SPB_HDR {
        return Err(ParamsErr::Truncated);
    }
    let strategy = u8_at(blob, 0);
    let n_groups = usize::from(u16_at(blob, 12));
    let body_len = u32_at(blob, 32) as usize;
    let gend = SPB_HDR + SPB_GROUP * n_groups;
    if blob.len() < gend {
        return Err(ParamsErr::Truncated);
    }
    if !body_len.is_multiple_of(8) || blob.len() != gend.saturating_add(body_len) {
        return Err(ParamsErr::BadLen);
    }
    let hwm = u32_at(blob, 36);
    if !matches!(u8_at(blob, 10), 1 | 2) || hwm > GroupId::N_MAX {
        return Err(ParamsErr::BadHdr);
    }
    // Step 1: padding and unused legs.
    if u8_at(blob, 11) != 0 || u16_at(blob, 14) != 0 {
        return Err(ParamsErr::Padding);
    }
    for gi in 0..n_groups {
        let g = SPB_HDR + SPB_GROUP * gi;
        if u64_at(blob, g + 72) != 0 {
            return Err(ParamsErr::Padding);
        }
        let used = used_legs(u8_at(blob, g + 6));
        for li in 0..4 {
            let l = leg_off(g, li);
            let bad = if li < used {
                u16_at(blob, l + 6) != 0 || u32_at(blob, l + 12) != 0
            } else {
                u64_at(blob, l) != 0 || u64_at(blob, l + 8) != 0
            };
            if bad {
                return Err(ParamsErr::Padding);
            }
        }
    }
    // Step 2: groups.
    for gi in 0..n_groups {
        let g = SPB_HDR + SPB_GROUP * gi;
        let id = u32_at(blob, g);
        let ctr = id & 0xFF_FFFF;
        let ok = (id >> 24) as u8 == strategy
            && (1..=GroupId::N_MAX).contains(&ctr)
            && (1..=4).contains(&u8_at(blob, g + 6))
            && u8_at(blob, g + 7) <= 1
            && ctr <= hwm;
        if !ok {
            return Err(ParamsErr::BadGroup);
        }
    }
    if has_dup(blob, n_groups) {
        return Err(ParamsErr::DupGroup);
    }
    // Step 3: used legs.
    for gi in 0..n_groups {
        let g = SPB_HDR + SPB_GROUP * gi;
        for li in 0..usize::from(u8_at(blob, g + 6)) {
            let l = leg_off(g, li);
            let kind = u8_at(blob, l + 5);
            let bad = u32_at(blob, l) == 0
                || !matches!(kind, LegDef::CEX | LegDef::CHAIN)
                || (kind == LegDef::CEX && u32_at(blob, l + 8) != 0);
            if bad {
                return Err(ParamsErr::BadLeg);
            }
        }
    }
    let (hb, rest) = blob.split_at_checked(SPB_HDR).ok_or(ParamsErr::Truncated)?;
    let (gb, body) = rest
        .split_at_checked(gend - SPB_HDR)
        .ok_or(ParamsErr::Truncated)?;
    let hdr = StrategyParamsHdr::try_ref_from_bytes(hb).map_err(|_| ParamsErr::Misaligned)?;
    let groups = <[GroupSpec]>::ref_from_bytes(gb).map_err(|_| ParamsErr::Misaligned)?;
    Ok(SpbView { hdr, groups, body })
}

/// Duplicate group id: O(n) when ids ascend (the canonical encoder's order), else O(n²); no allocation.
fn has_dup(blob: &[u8], n: usize) -> bool {
    let id = |i: usize| u32_at(blob, SPB_HDR + SPB_GROUP * i);
    if (1..n).all(|i| id(i - 1) < id(i)) {
        return false;
    }
    (0..n).any(|i| (i + 1..n).any(|j| id(i) == id(j)))
}
