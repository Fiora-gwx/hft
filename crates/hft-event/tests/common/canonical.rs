//! Canonical sample record of every kind (EVT-173 input; EVT-200 corpus): every field non-zero and
//! increasing in field order, enums at their largest legal code; `seq = 1`, `tsc = 2`,
//! `flags = EVENT_END` (+ `CRC` per the kind's policy); Multi kinds are single-part records.
#![allow(dead_code)]

use hft_event::*;

use super::canonical as canonical_bytes;

fn flags(k: EventKind) -> RecordFlags {
    let crc = kind_info(k).map(|i| i.crc) == Some(CrcPolicy::Always);
    if crc {
        RecordFlags::EVENT_END | RecordFlags::CRC
    } else {
        RecordFlags::EVENT_END
    }
}

fn val<B: Body>(fields: &[FieldDesc]) -> B {
    B::try_read_from_bytes(&canonical_bytes(fields)[..]).unwrap()
}

fn fixed<B: Body>(fields: &[FieldDesc]) -> Vec<u8> {
    let mut out = vec![0u8; 512];
    let n = encode_fixed(&mut out, 1, 2, flags(B::KIND), &val::<B>(fields));
    out.truncate(n as usize);
    out
}

fn tail(n: usize) -> Vec<u8> {
    (0..n).map(|i| (i as u8).wrapping_add(1)).collect()
}

fn var<P: Body>(fields: &[FieldDesc], l: usize) -> Vec<u8> {
    let mut out = vec![0u8; 70_000];
    let n = encode_var(&mut out, 1, 2, flags(P::KIND), &val::<P>(fields), &tail(l)).unwrap();
    out.truncate(n as usize);
    out
}

fn multi(kind: EventKind, fields: &[FieldDesc], l: usize) -> Vec<u8> {
    let mut logical = canonical_bytes(fields);
    logical.extend(tail(l));
    let (ph, chunk, f) = PartIter::new(&logical).unwrap().next().unwrap();
    let mut out = vec![0u8; 70_000];
    let n = encode_part(&mut out, 1, 2, flags(kind) | f, kind, &ph, chunk).unwrap();
    out.truncate(n as usize);
    out
}

fn count(fields: &[FieldDesc], name: &str) -> usize {
    let b = canonical_bytes(fields);
    let f = fields.iter().find(|f| f.name == name).unwrap();
    let o = f.offset as usize;
    match f.ty.width() {
        2 => u16::from_le_bytes([b[o], b[o + 1]]) as usize,
        _ => u32::from_le_bytes(b[o..o + 4].try_into().unwrap()) as usize,
    }
}

macro_rules! fixed_all {
    ($($t:ty),* $(,)?) => { vec![$((<$t as Body>::KIND, fixed::<$t>(<$t>::FIELDS))),*] };
}

/// `(kind, record bytes)` for all 39 production kinds, ascending by number.
pub fn records() -> Vec<(EventKind, Vec<u8>)> {
    let mut v = fixed_all!(
        Bbo,
        L2Delta,
        Trade,
        Funding,
        MarkPrice,
        IndexPrice,
        Liquidation,
        OrderDecision,
        SendReport,
        OrderEvent,
        PositionReport,
        BalanceReport,
        FundingSettle,
        ExternalFill,
        ExecDecision,
        ChainHead,
        PoolQuote,
        SwapDecision,
        TxLifecycle,
        CmdResult,
        ReconcileReport,
        Timer,
        ConnState,
        LatencySample,
        Note,
        HostProfile,
        ClockStatus,
        SegmentRoll,
        EngineClock
    );
    let snap = L2SnapHdr::FIELDS;
    let blob = BlobHdr::FIELDS;
    let blob_len = {
        let b = canonical_bytes(blob);
        u32::from_le_bytes(b[4..8].try_into().unwrap()) as usize
    };
    v.push((
        EventKind::L2_SNAPSHOT,
        multi(
            EventKind::L2_SNAPSHOT,
            snap,
            16 * (count(snap, "n_bids") + count(snap, "n_asks")),
        ),
    ));
    let bh = L2BatchHdr::FIELDS;
    v.push((
        EventKind::L2_BATCH,
        var::<L2BatchHdr>(bh, 16 * (count(bh, "n_bids") + count(bh, "n_asks"))),
    ));
    v.push((
        EventKind::MARGIN_REPORT,
        var::<MarginReportHdr>(MarginReportHdr::FIELDS, 16 * count(MarginReportHdr::FIELDS, "n")),
    ));
    v.push((
        EventKind::ACCOUNT_CONFIG_REPORT,
        var::<AccountConfigReportHdr>(
            AccountConfigReportHdr::FIELDS,
            16 * count(AccountConfigReportHdr::FIELDS, "n_lev"),
        ),
    ));
    v.push((
        EventKind::CONTROL_CMD,
        multi(
            EventKind::CONTROL_CMD,
            CmdHdr::FIELDS,
            count(CmdHdr::FIELDS, "payload_len"),
        ),
    ));
    v.push((
        EventKind::CONFIG_BLOB,
        multi(EventKind::CONFIG_BLOB, blob, blob_len),
    ));
    v.push((
        EventKind::ADVICE,
        var::<AdviceHdr>(AdviceHdr::FIELDS, count(AdviceHdr::FIELDS, "len")),
    ));
    v.push((
        EventKind::STATE_SNAPSHOT,
        multi(EventKind::STATE_SNAPSHOT, SnapshotHdr::FIELDS, 0),
    ));
    v.push((
        EventKind::RAW_RX,
        multi(
            EventKind::RAW_RX,
            RawHdr::FIELDS,
            count(RawHdr::FIELDS, "payload_len"),
        ),
    ));
    v.push((
        EventKind::RAW_TX,
        multi(
            EventKind::RAW_TX,
            RawHdr::FIELDS,
            count(RawHdr::FIELDS, "payload_len"),
        ),
    ));
    v.sort_by_key(|(k, _)| k.0);
    v
}

/// Fixture file name of a kind.
pub fn file_name(k: EventKind) -> String {
    format!("{}.bin", kind_info(k).unwrap().name)
}
