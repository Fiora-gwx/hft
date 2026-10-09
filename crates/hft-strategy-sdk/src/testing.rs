//! Test stubs (README "测试桩(testing 特性)"; only in `[dev-dependencies]`): `MockViews`,
//! `CtxHarness`, `ModelHarness`, the layout checks `check_layout`/`layout_json` and the owned
//! event builders `ev`. They let a strategy test its fast part and Model without assembling the
//! engine. `MockViews` never matches orders: it only replays the state the test sets.
//!
//! Harness assertions (unregistered Note codes, declarations exceeded) fail with `assert!`.

mod ev_impl;
mod harness;
mod mock;

pub use harness::{AllocMeter, Ckpt, CtxHarness, ModelHarness, Recorded, assert_within_declaration};
pub use mock::{BookFixture, MockBook, MockPortfolio, MockViews, PortfolioFixture, RefFixture};

/// Owned event builders; `.as_ref()` gives the `EventRef` a strategy receives.
pub mod ev {
    pub use super::ev_impl::*;
}

use core::fmt::Write as _;

use crate::Model;
use crate::layout::{ElemTy, LenRule, ModelLayoutDesc, SectionDesc, SectionScope};
use crate::snapshot::{PutTy, SnapshotWriter, WriteRec};
use crate::strategy::SnapshotProgress;

/// What `check_layout` found different from `Model::LAYOUT`.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub struct LayoutMismatch {
    /// Index of the offending record in the recorded call sequence.
    pub at: usize,
    pub why: &'static str,
}

const fn put_matches(t: ElemTy, p: PutTy) -> bool {
    matches!(
        (t, p),
        (ElemTy::U8, PutTy::U8)
            | (ElemTy::U16, PutTy::U16)
            | (ElemTy::U32, PutTy::U32)
            | (ElemTy::U64, PutTy::U64)
            | (ElemTy::I32, PutTy::I32)
            | (ElemTy::I64, PutTy::I64)
            | (ElemTy::I128, PutTy::I128)
            | (ElemTy::F64Bits, PutTy::U64)
            | (ElemTy::Bytes, PutTy::Bytes)
    )
}

fn mismatch(at: usize, why: &'static str) -> LayoutMismatch {
    LayoutMismatch { at, why }
}

/// Checks the puts of one section (`recs` = everything up to the next `section`).
fn check_section(
    s: &SectionDesc,
    recs: &[WriteRec],
    base: usize,
    counts: &[u32],
) -> Result<(), LayoutMismatch> {
    let mut i = 0usize;
    let mut ci = 0usize;
    let next = |i: &mut usize| -> Option<(PutTy, usize)> {
        let r = recs.get(*i)?;
        *i += 1;
        match *r {
            WriteRec::Put(t, n) => Some((t, n)),
            WriteRec::Section(_) => None,
        }
    };
    if s.scope == SectionScope::Group {
        match next(&mut i) {
            Some((PutTy::U32, 4)) => {}
            _ => {
                return Err(mismatch(
                    base + i,
                    "group section does not start with a u32 GroupId",
                ));
            }
        }
    }
    for e in s.elems {
        let n = match e.len {
            LenRule::One => 1u64,
            LenRule::Fixed(n) => u64::from(n),
            LenRule::Prefixed => {
                if !matches!(next(&mut i), Some((PutTy::U32, 4))) {
                    return Err(mismatch(base + i, "Prefixed element without a u32 count"));
                }
                let c = counts.get(ci).copied().unwrap_or(0);
                ci += 1;
                u64::from(c)
            }
        };
        if e.ty == ElemTy::Bytes && e.len == LenRule::Prefixed {
            // One put_bytes of `count` bytes (may be omitted when the count is 0).
            match recs.get(i).copied() {
                Some(WriteRec::Put(PutTy::Bytes, len)) if len as u64 == n => i += 1,
                _ if n == 0 => {}
                _ => return Err(mismatch(base + i, "Prefixed Bytes length differs from its count")),
            }
            continue;
        }
        for _ in 0..n {
            match next(&mut i) {
                Some((p, _)) if put_matches(e.ty, p) => {}
                Some(_) => return Err(mismatch(base + i, "element type differs from LAYOUT")),
                None => return Err(mismatch(base + i, "fewer elements than LAYOUT")),
            }
        }
    }
    if i != recs.len() {
        return Err(mismatch(base + i, "more writes than LAYOUT describes"));
    }
    Ok(())
}

/// Reads the `u32` counts written before each `Prefixed` element (from the bytes).
fn prefixed_counts(s: &SectionDesc, bytes: &[u8], recs: &[WriteRec], mut off: usize) -> Vec<u32> {
    let mut out = Vec::new();
    let mut i = 0usize;
    let take = |i: &mut usize, off: &mut usize| -> Option<(PutTy, usize, usize)> {
        let r = recs.get(*i)?;
        *i += 1;
        match *r {
            WriteRec::Put(t, n) => {
                let at = *off;
                *off += n;
                Some((t, n, at))
            }
            WriteRec::Section(_) => None,
        }
    };
    if s.scope == SectionScope::Group {
        let _ = take(&mut i, &mut off);
    }
    for e in s.elems {
        let n = match e.len {
            LenRule::One => 1u64,
            LenRule::Fixed(n) => u64::from(n),
            LenRule::Prefixed => {
                let Some((_, 4, at)) = take(&mut i, &mut off) else {
                    return out;
                };
                let c = bytes
                    .get(at..at + 4)
                    .and_then(|b| <[u8; 4]>::try_from(b).ok())
                    .map_or(0, u32::from_le_bytes);
                out.push(c);
                u64::from(c)
            }
        };
        if e.ty == ElemTy::Bytes && e.len == LenRule::Prefixed {
            if n > 0 || matches!(recs.get(i), Some(WriteRec::Put(PutTy::Bytes, 0))) {
                let _ = take(&mut i, &mut off);
            }
            continue;
        }
        for _ in 0..n {
            if take(&mut i, &mut off).is_none() {
                return out;
            }
        }
    }
    out
}

/// Runs `snapshot_chunk` with a recording writer until `Done` and checks that the written
/// sections, element types and lengths match `M::LAYOUT`: every `section(tag)` is described; group
/// sections start with a u32 GroupId; `Prefixed` counts equal the element counts; nothing is
/// written outside a section.
pub fn check_layout<M: Model>(m: &M) -> Result<(), LayoutMismatch> {
    let mut w = SnapshotWriter::recording(64 << 20);
    let mut recs: Vec<WriteRec> = Vec::new();
    let mut bytes: Vec<u8> = Vec::new();
    let mut from = 0u16;
    for _ in 0..=u32::from(u16::MAX) {
        w.clear();
        let p = m.snapshot_chunk(from, &mut w, u32::MAX);
        recs.extend_from_slice(w.records());
        bytes.extend_from_slice(w.as_bytes());
        match p {
            SnapshotProgress::Done => break,
            SnapshotProgress::More(n) if n > from => from = n,
            SnapshotProgress::More(_) => return Err(mismatch(recs.len(), "snapshot_chunk does not advance")),
        }
    }
    let layout = M::LAYOUT;
    let mut i = 0usize;
    let mut off = 0usize;
    let mut first = true;
    while i < recs.len() {
        let WriteRec::Section(tag) = recs.get(i).copied().unwrap_or(WriteRec::Section(0)) else {
            return Err(mismatch(i, "bytes written outside a section"));
        };
        let Some(s) = layout.sections.iter().find(|s| s.tag == tag) else {
            return Err(mismatch(i, "section tag not in LAYOUT"));
        };
        if s.scope == SectionScope::Instance && !first {
            return Err(mismatch(i, "instance section not at the head of the first chunk"));
        }
        first = false;
        let end = recs
            .iter()
            .skip(i + 1)
            .position(|r| matches!(r, WriteRec::Section(_)))
            .map_or(recs.len(), |p| i + 1 + p);
        let body = recs.get(i + 1..end).unwrap_or(&[]);
        let counts = prefixed_counts(s, &bytes, body, off);
        check_section(s, body, i + 1, &counts)?;
        off += body
            .iter()
            .map(|r| match r {
                WriteRec::Put(_, n) => *n,
                WriteRec::Section(_) => 0,
            })
            .sum::<usize>();
        i = end;
    }
    Ok(())
}

/// Canonical JSON of a layout (the layout golden): keys in field order, `ElemTy` and
/// `SectionScope` as variant names, `LenRule` as `"one"`, `{"fixed":n}`, `"prefixed"`; no
/// whitespace.
pub fn layout_json(d: &ModelLayoutDesc) -> String {
    let mut o = String::from("{\"sections\":[");
    for (si, s) in d.sections.iter().enumerate() {
        if si > 0 {
            o.push(',');
        }
        let scope = match s.scope {
            SectionScope::Instance => "Instance",
            SectionScope::Group => "Group",
        };
        let _ = write!(o, "{{\"tag\":{},\"name\":", s.tag);
        json_str(&mut o, s.name);
        let _ = write!(o, ",\"scope\":\"{scope}\",\"elems\":[");
        for (ei, e) in s.elems.iter().enumerate() {
            if ei > 0 {
                o.push(',');
            }
            o.push_str("{\"name\":");
            json_str(&mut o, e.name);
            let _ = write!(o, ",\"ty\":\"{:?}\",\"len\":", e.ty);
            match e.len {
                LenRule::One => o.push_str("\"one\""),
                LenRule::Fixed(n) => {
                    let _ = write!(o, "{{\"fixed\":{n}}}");
                }
                LenRule::Prefixed => o.push_str("\"prefixed\""),
            }
            o.push('}');
        }
        o.push_str("]}");
    }
    o.push_str("]}");
    o
}

fn json_str(o: &mut String, s: &str) {
    o.push('"');
    for c in s.chars() {
        match c {
            '"' => o.push_str("\\\""),
            '\\' => o.push_str("\\\\"),
            c if (c as u32) < 0x20 => {
                let _ = write!(o, "\\u{:04x}", c as u32);
            }
            c => o.push(c),
        }
    }
    o.push('"');
}
