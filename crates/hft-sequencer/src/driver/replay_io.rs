//! Offline JournalReader -> logical replay log. Transport decoding and segment-chain checks
//! stay in hft-journal; only the replay representation and its explicit bounds live here.

use hft_event::{DecodeErr, EventKind, Layout, RecordFlags, classify, kind_info};
use hft_journal::{GapInfo, JournalKind, JournalReader, Next, ReadErr};

use super::replay::Log;
use crate::{MemRecord, SegClock};

/// Bounds on retained logical input, in addition to JournalReader's reassembly cap. These are
/// supplied by the cold caller; reaching either bound returns an error, never a partial log.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct LogReadLimits {
    pub records: usize,
    pub body_bytes: usize,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LogReadError {
    Read(ReadErr),
    Gap(GapInfo),
    Body {
        seq: u64,
        error: DecodeErr,
    },
    /// Missing/wrong journal identity or schema incompatible with this executable.
    Header {
        segment: u64,
        kind: EventKind,
    },
    Limit,
    /// The reader was following a live tail or could not reach its fixed complete boundary.
    Pending,
    Empty,
}

impl Log {
    /// Read through JournalReader's fixed LastCompleteEvent boundary. The caller configures
    /// Oldest/segment-boot start, Error-on-gap and nonzero reassembly capacity. Physical seq
    /// numbers, event flags and each segment's calibration are preserved. No current config,
    /// process clock, writer, truncation or recovery mutation is consulted.
    pub fn from_reader(reader: &mut JournalReader, limits: LogReadLimits) -> Result<Self, LogReadError> {
        let mut log = Log::default();
        let mut bytes = 0usize;
        let mut last_segment = None;
        loop {
            let record = match reader.poll() {
                Next::Record(r) => {
                    let body = r.exact_body().map_err(|error| LogReadError::Body {
                        seq: r.hdr.seq,
                        error,
                    })?;
                    copy_record(
                        &log,
                        limits,
                        &mut bytes,
                        r.hdr.seq,
                        EventKind(r.hdr.kind),
                        RecordFlags(r.hdr.flags),
                        r.hdr.tsc,
                        body,
                    )?
                }
                Next::Event(e) => {
                    if !matches!(kind_info(e.kind).map(|k| k.layout), Some(Layout::Multi { prefix }) if e.body.len() >= usize::from(prefix))
                    {
                        return Err(LogReadError::Body {
                            seq: e.seq,
                            error: DecodeErr::BadPart,
                        });
                    }
                    copy_record(&log, limits, &mut bytes, e.seq, e.kind, e.flags, e.tsc, e.body)?
                }
                Next::End => {
                    return if log.recs.is_empty() {
                        Err(LogReadError::Empty)
                    } else {
                        Ok(log)
                    };
                }
                Next::Err(e) => return Err(LogReadError::Read(e)),
                Next::Gap(g) => return Err(LogReadError::Gap(g)),
                Next::Pending => return Err(LogReadError::Pending),
            };
            let pos = reader.position();
            if pos.seq > record.seq {
                log.part_ends.insert(record.seq, pos.seq);
            }
            let bad = || LogReadError::Header {
                segment: pos.segment_no,
                kind: record.kind,
            };
            let header = reader.segment_header().ok_or_else(bad)?;
            if header.segment_no != pos.segment_no || header.journal_kind != JournalKind::Seq as u8 {
                return Err(bad());
            }
            let info = kind_info(record.kind).ok_or_else(bad)?;
            let size = match info.layout {
                Layout::Fixed { size } => size,
                Layout::Var { prefix, .. } | Layout::Multi { prefix } => prefix,
            };
            let entries = header
                .schema
                .get(..usize::from(header.n_schema))
                .ok_or_else(bad)?;
            let mut schema = entries.iter().filter(|s| s.kind == record.kind.0);
            let row = schema.next().ok_or_else(bad)?;
            if row.version != info.schema_version || row.size != u32::from(size) || schema.next().is_some() {
                return Err(bad());
            }
            if last_segment != Some(pos.segment_no) {
                log.seg_start.push(log.recs.len());
                log.segs.push(SegClock {
                    tsc_hz_milli: header.tsc_hz_milli,
                    calib_tsc: header.calib_tsc,
                    calib_realtime_ns: header.calib_realtime_ns,
                });
                last_segment = Some(pos.segment_no);
            }
            log.recs.push(record);
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn copy_record(
    log: &Log,
    limits: LogReadLimits,
    bytes: &mut usize,
    seq: u64,
    kind: EventKind,
    flags: RecordFlags,
    tsc: u64,
    body: &[u8],
) -> Result<MemRecord, LogReadError> {
    let next = bytes.checked_add(body.len()).ok_or(LogReadError::Limit)?;
    if log.recs.len() >= limits.records || next > limits.body_bytes {
        return Err(LogReadError::Limit);
    }
    classify(kind, body).map_err(|error| LogReadError::Body { seq, error })?;
    *bytes = next;
    Ok(MemRecord {
        seq,
        kind,
        flags,
        tsc,
        body: body.to_vec(),
    })
}
