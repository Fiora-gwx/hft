//! Read-only, cold diagnostics for two already compared snapshot bodies. This does not decide
//! replay success: byte comparison remains authoritative, including when hashes collide.
#![allow(clippy::disallowed_types, clippy::disallowed_macros)] // registered cold replay-io diagnostics; never called by a live driver

use std::collections::BTreeSet;

use hft_event::schema::{FieldDesc, FieldTy};
use hft_event::{CapRecord, SectionHdr, SnapshotHdr};
use hft_strategy_sdk::SnapshotReader;
use xxhash_rust::xxh3::Xxh3;
use zerocopy::FromBytes;

/// Same component priority as the testkit HashTrail contract. ClockCal is measurement
/// metadata and never identifies a state component; unknown section owners are not guessed.
const COMPONENTS: [&str; 6] = [
    "hft-ledger",
    "hft-oms",
    "hft-risk",
    "strategies",
    "hft-book",
    "hft-exec",
];

fn component(section: u16) -> Option<usize> {
    match section {
        1 | 13 | 22 => Some(0),
        2 | 3 | 12 => Some(1),
        4 | 14 | 17 | 18 | 23 => Some(2),
        5 => Some(3),
        6 | 7 => Some(4),
        21 => Some(5),
        _ => None,
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Side {
    Logged,
    Regenerated,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum InvalidSnapshot {
    Header,
    SectionFraming,
    DuplicateSection,
    SectionCount,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DiagnosticError {
    pub side: Side,
    /// Offset in the logical body, excluding kind and transport/part headers.
    pub offset: usize,
    pub reason: InvalidSnapshot,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ComponentHash {
    pub component: &'static str,
    pub logged: u64,
    pub regenerated: u64,
}

/// Only the first differing field of the first differing component is retained. Offsets are
/// into each original logical body; slices borrow the caller's bytes, never a running engine.
#[derive(Debug)]
pub struct FieldDifference<'a> {
    pub component: Option<&'static str>,
    pub section: Option<u16>,
    pub row: Option<usize>,
    pub field: String,
    /// `field`, `opaque`, `unknown_row_version`, `invalid_row_layout`, or `missing_section`.
    pub layout: &'static str,
    pub logged_offset: Option<usize>,
    pub regenerated_offset: Option<usize>,
    pub logged: &'a [u8],
    pub regenerated: &'a [u8],
}

#[derive(Debug)]
pub struct SnapshotDifference<'a> {
    pub logged_state_hash: [u8; 16],
    pub regenerated_state_hash: [u8; 16],
    pub components: [ComponentHash; 6],
    pub first: Option<FieldDifference<'a>>,
}

#[derive(Clone, Copy, Debug)]
struct Frame<'a> {
    header: SectionHdr,
    offset: usize,
    bytes: &'a [u8],
}

fn frames(body: &[u8], side: Side) -> Result<Vec<Frame<'_>>, DiagnosticError> {
    let error = |offset, reason| DiagnosticError { side, offset, reason };
    let (hdr, sections) =
        SnapshotHdr::read_from_prefix(body).map_err(|_| error(0, InvalidSnapshot::Header))?;
    let mut reader = SnapshotReader::new(sections);
    let mut seen = BTreeSet::new();
    // Do not reserve according to an untrusted header count. Each pushed item consumes at
    // least one complete SectionHdr in the caller's already bounded input.
    let mut out = Vec::new();
    loop {
        let start = reader.pos();
        let offset = size_of::<SnapshotHdr>() + start;
        let next = reader
            .next_section()
            .map_err(|_| error(offset, InvalidSnapshot::SectionFraming))?;
        let Some(section) = next else { break };
        if !seen.insert(section.section) {
            return Err(error(offset, InvalidSnapshot::DuplicateSection));
        }
        let bytes = &sections[start..reader.pos()];
        let (header, _) = SectionHdr::read_from_prefix(bytes)
            .map_err(|_| error(offset, InvalidSnapshot::SectionFraming))?;
        out.push(Frame {
            header,
            offset,
            bytes,
        });
    }
    if out.is_empty() || out.len() != hdr.n_sections as usize {
        return Err(error(4, InvalidSnapshot::SectionCount));
    }
    Ok(out)
}

fn hashes(frames: &[Frame<'_>]) -> [u64; 6] {
    let mut hashers: [Xxh3; 6] = core::array::from_fn(|_| Xxh3::new());
    let mut present = [false; 6];
    for frame in frames {
        if let Some(c) = component(frame.header.section) {
            hashers[c].update(frame.bytes);
            present[c] = true;
        }
    }
    core::array::from_fn(|i| if present[i] { hashers[i].digest() } else { 0 })
}

fn leaf(fields: &[FieldDesc], offset: usize) -> Option<(String, usize, usize)> {
    for field in fields {
        let start = usize::from(field.offset);
        let end = start + usize::from(field.ty.width());
        if !(start..end).contains(&offset) {
            continue;
        }
        if let FieldTy::Nested(nested) = field.ty
            && let Some((name, at, width)) = leaf(nested, offset - start)
        {
            return Some((format!("{}.{name}", field.name), start + at, width));
        }
        return Some((field.name.into(), start, end - start));
    }
    None
}

fn first_byte(a: &[u8], b: &[u8]) -> Option<usize> {
    a.iter()
        .zip(b)
        .position(|(a, b)| a != b)
        .or_else(|| (a.len() != b.len()).then_some(a.len().min(b.len())))
}

fn bytes_at(body: &[u8], start: usize, width: usize) -> &[u8] {
    body.get(start..start.saturating_add(width).min(body.len()))
        .unwrap_or(&[])
}

fn differing_frames<'a>(
    a: Option<Frame<'a>>,
    b: Option<Frame<'a>>,
    owner: Option<&'static str>,
) -> FieldDifference<'a> {
    let section = a.or(b).map(|f| f.header.section);
    let mut diff = FieldDifference {
        component: owner,
        section,
        row: None,
        field: format!("sections[{}].presence", section.unwrap_or(0)),
        layout: "missing_section",
        logged_offset: a.map(|f| f.offset),
        regenerated_offset: b.map(|f| f.offset),
        logged: a.map_or(&[], |f| &f.bytes[..size_of::<SectionHdr>()]),
        regenerated: b.map_or(&[], |f| &f.bytes[..size_of::<SectionHdr>()]),
    };
    let (Some(a), Some(b)) = (a, b) else { return diff };
    let at = first_byte(a.bytes, b.bytes).unwrap_or(0);
    let (field, start, width, layout) = if at < size_of::<SectionHdr>() {
        let (name, start, width) = leaf(SectionHdr::FIELDS, at).unwrap_or(("header".into(), at, 1));
        (
            format!("sections[{}].{name}", a.header.section),
            start,
            width,
            "field",
        )
    } else {
        let body_at = at - size_of::<SectionHdr>();
        let mut layout = "opaque";
        if a.header.section == hft_risk::snapshot::SECTION_RISK_STATE {
            layout = if a.header.version != hft_risk::snapshot::RISK_STATE_VERSION {
                "unknown_row_version"
            } else if usize::try_from(a.header.count)
                .ok()
                .and_then(|n| n.checked_mul(size_of::<CapRecord>()))
                != Some(a.bytes.len() - size_of::<SectionHdr>())
            {
                "invalid_row_layout"
            } else {
                "field"
            };
            if layout == "field" {
                let row = body_at / size_of::<CapRecord>();
                if let Some((name, off, width)) = leaf(CapRecord::FIELDS, body_at % size_of::<CapRecord>()) {
                    diff.row = Some(row);
                    let start = size_of::<SectionHdr>() + row * size_of::<CapRecord>() + off;
                    diff.field = format!("sections[{}].rows[{row}].{name}", a.header.section);
                    diff.layout = layout;
                    diff.logged_offset = Some(a.offset + start);
                    diff.regenerated_offset = Some(b.offset + start);
                    diff.logged = bytes_at(a.bytes, start, width);
                    diff.regenerated = bytes_at(b.bytes, start, width);
                    return diff;
                }
            }
        }
        (
            format!("sections[{}].body[{body_at}]", a.header.section),
            at,
            1,
            layout,
        )
    };
    diff.field = field;
    diff.layout = layout;
    diff.logged_offset = Some(a.offset + start);
    diff.regenerated_offset = Some(b.offset + start);
    diff.logged = bytes_at(a.bytes, start, width);
    diff.regenerated = bytes_at(b.bytes, start, width);
    diff
}

fn pair_difference<'a>(
    a: impl Iterator<Item = Frame<'a>>,
    b: impl Iterator<Item = Frame<'a>>,
    owner: Option<&'static str>,
) -> Option<FieldDifference<'a>> {
    let mut a = a;
    let mut b = b;
    loop {
        match (a.next(), b.next()) {
            (None, None) => return None,
            (Some(a), Some(b)) if a.bytes == b.bytes => {}
            (a, b) => return Some(differing_frames(a, b, owner)),
        }
    }
}

/// Locate one difference, using the six HashTrail components in their documented priority,
/// then the remaining sections, their framing, and finally SnapshotHdr. Inputs must be the
/// masked logical bodies retained by ReplaySink (no kind/PartHdr). Unknown row layouts remain
/// byte offsets; only the current RiskState/CapRecord schema receives row/field names.
/// Bytes are scanned once per fixed component; memory is bounded by the actual section count. Hashes use
/// the existing snapshot protocol; ClockCal is excluded from state/component hashes.
pub fn snapshot_difference<'a>(
    logged: &'a [u8],
    regenerated: &'a [u8],
) -> Result<SnapshotDifference<'a>, DiagnosticError> {
    let a = frames(logged, Side::Logged)?;
    let b = frames(regenerated, Side::Regenerated)?;
    let ha = hashes(&a);
    let hb = hashes(&b);
    let mut out = SnapshotDifference {
        logged_state_hash: crate::snapshot::state_hash(&logged[size_of::<SnapshotHdr>()..]),
        regenerated_state_hash: crate::snapshot::state_hash(&regenerated[size_of::<SnapshotHdr>()..]),
        components: core::array::from_fn(|i| ComponentHash {
            component: COMPONENTS[i],
            logged: ha[i],
            regenerated: hb[i],
        }),
        first: None,
    };
    for (c, owner) in COMPONENTS.iter().enumerate() {
        let select = |f: &&Frame<'_>| component(f.header.section) == Some(c);
        if let Some(d) = pair_difference(
            a.iter().filter(select).copied(),
            b.iter().filter(select).copied(),
            Some(owner),
        ) {
            out.first = Some(d);
            return Ok(out);
        }
    }
    // Includes section-order changes between components and unassigned state sections. A
    // difference confined to ClockCal is metadata, not a false state-component diagnosis.
    let state = |f: &&Frame<'_>| f.header.section != crate::snapshot::SECTION_CLOCK_CAL;
    if let Some(mut d) = pair_difference(
        a.iter().filter(state).copied(),
        b.iter().filter(state).copied(),
        None,
    ) {
        if matches!(d.section, Some(8 | 9 | 10 | 15 | 19 | 20 | 24 | 25 | 26 | 27))
            || d.field.ends_with(".section")
        {
            d.component = Some("hft-sequencer");
        }
        out.first = Some(d);
    } else if let Some(d) = pair_difference(a.iter().copied(), b.iter().copied(), None) {
        out.first = Some(d);
    } else if let Some(at) = first_byte(
        &logged[..size_of::<SnapshotHdr>()],
        &regenerated[..size_of::<SnapshotHdr>()],
    ) {
        let (name, start, width) = leaf(SnapshotHdr::FIELDS, at).unwrap_or(("header".into(), at, 1));
        out.first = Some(FieldDifference {
            component: None,
            section: None,
            row: None,
            field: name,
            layout: "field",
            logged_offset: Some(start),
            regenerated_offset: Some(start),
            logged: bytes_at(logged, start, width),
            regenerated: bytes_at(regenerated, start, width),
        });
    }
    Ok(out)
}
