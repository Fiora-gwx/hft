//! Streaming seq-journal source (README "读取"): logical records one at a time, the calibration of
//! the segment each record belongs to, and a lookahead the driver uses for segment-roll values.

use std::collections::VecDeque;
use std::path::PathBuf;

use hft_event::{EventKind, Layout, RecordFlags, classify, kind_info};
use hft_journal::{GapReason, JournalKind, JournalReader, Next};
use hft_sequencer::{MemRecord, SegClock};

/// Why the source stopped early.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SourceErr {
    Open(String),
    Read(String),
    Gap(String),
    /// A record whose segment is not a seq journal, or a body that does not classify.
    Body {
        seq: u64,
        what: String,
    },
    Empty,
}

impl core::fmt::Display for SourceErr {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{self:?}")
    }
}

/// One record with the index of its segment (into `Source::segs`).
#[derive(Clone, Debug)]
pub struct Rec {
    pub r: MemRecord,
    pub seg: usize,
}

/// The journal reader plus a lookahead queue.
pub struct Source {
    reader: JournalReader,
    ahead: VecDeque<Rec>,
    /// Calibration of every segment entered so far, in order.
    pub segs: Vec<SegClock>,
    seg_no: Option<u64>,
    done: bool,
    /// Records read from the journal (lookahead included).
    pub read: u64,
}

impl core::fmt::Debug for Source {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("Source")
            .field("segs", &self.segs.len())
            .field("read", &self.read)
            .finish_non_exhaustive()
    }
}

impl Source {
    /// Immutable seq segment files in chain order (a recovery gap marked in a header is allowed,
    /// as in `replayer verify`).
    pub fn open(files: &[PathBuf], reassembly: usize) -> Result<Source, SourceErr> {
        let reader = JournalReader::open_files_recoverable(files, reassembly)
            .map_err(|e| SourceErr::Open(format!("{}: {:?}", e.path.display(), e.problem)))?;
        Ok(Source {
            reader,
            ahead: VecDeque::with_capacity(1024),
            segs: Vec::new(),
            seg_no: None,
            done: false,
            read: 0,
        })
    }

    fn pull(&mut self) -> Result<Option<Rec>, SourceErr> {
        if self.done {
            return Ok(None);
        }
        let (seq, kind, flags, tsc, body) = match self.reader.poll() {
            Next::Record(r) => {
                let body = r.exact_body().map_err(|e| SourceErr::Body {
                    seq: r.hdr.seq,
                    what: format!("{e:?}"),
                })?;
                (
                    r.hdr.seq,
                    EventKind(r.hdr.kind),
                    RecordFlags(r.hdr.flags),
                    r.hdr.tsc,
                    body.to_vec(),
                )
            }
            Next::Event(e) => {
                if !matches!(kind_info(e.kind).map(|k| k.layout), Some(Layout::Multi { prefix }) if e.body.len() >= usize::from(prefix))
                {
                    return Err(SourceErr::Body {
                        seq: e.seq,
                        what: "bad part".into(),
                    });
                }
                (e.seq, e.kind, e.flags, e.tsc, e.body.to_vec())
            }
            Next::End | Next::Pending => {
                self.done = true;
                return Ok(None);
            }
            Next::Gap(g) if g.reason == GapReason::RecoveryGap => return self.pull(),
            Next::Gap(g) => return Err(SourceErr::Gap(format!("{g:?}"))),
            Next::Err(e) => return Err(SourceErr::Read(format!("{e:?}"))),
        };
        classify(kind, &body).map_err(|e| SourceErr::Body {
            seq,
            what: format!("{e:?}"),
        })?;
        let pos = self.reader.position();
        let h = self.reader.segment_header().ok_or(SourceErr::Body {
            seq,
            what: "no segment header".into(),
        })?;
        if h.journal_kind != JournalKind::Seq as u8 {
            return Err(SourceErr::Body {
                seq,
                what: "not a seq journal".into(),
            });
        }
        if self.seg_no != Some(pos.segment_no) {
            self.seg_no = Some(pos.segment_no);
            self.segs.push(SegClock {
                tsc_hz_milli: h.tsc_hz_milli,
                calib_tsc: h.calib_tsc,
                calib_realtime_ns: h.calib_realtime_ns,
            });
        }
        self.read += 1;
        Ok(Some(Rec {
            r: MemRecord {
                seq,
                kind,
                flags,
                tsc,
                body,
            },
            seg: self.segs.len() - 1,
        }))
    }

    /// The next record (lookahead first).
    pub fn next_rec(&mut self) -> Result<Option<Rec>, SourceErr> {
        if let Some(r) = self.ahead.pop_front() {
            return Ok(Some(r));
        }
        self.pull()
    }

    /// The next record without consuming it.
    pub fn peek(&mut self) -> Result<Option<&Rec>, SourceErr> {
        if self.ahead.is_empty()
            && let Some(r) = self.pull()?
        {
            self.ahead.push_back(r);
        }
        Ok(self.ahead.front())
    }

    /// Reads ahead until a record of a segment after `seg` is buffered (or the end); returns that
    /// segment's clock and its leading Meta records (the new segment's boot block sources).
    pub fn next_segment(&mut self, seg: usize) -> Result<Option<(SegClock, Vec<MemRecord>)>, SourceErr> {
        loop {
            if let Some(i) = self.ahead.iter().position(|r| r.seg > seg) {
                let s = self.ahead[i].seg;
                let meta: Vec<MemRecord> = self
                    .ahead
                    .iter()
                    .skip(i)
                    .take_while(|r| r.seg == s && r.r.role() == hft_event::Role::Meta)
                    .map(|r| r.r.clone())
                    .collect();
                // The leading Meta run may continue past the buffer.
                if self.ahead.len() == i + meta.len()
                    && !self.done
                    && let Some(r) = self.pull()?
                {
                    self.ahead.push_back(r);
                    continue;
                }
                return Ok(self.segs.get(s).map(|c| (*c, meta)));
            }
            match self.pull()? {
                Some(r) => self.ahead.push_back(r),
                None => return Ok(None),
            }
        }
    }
}
