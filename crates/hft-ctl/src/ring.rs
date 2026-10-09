//! The ctl ring as the gateway sees it: a request's records go in all together or not at all.

use hft_event::{CtlRecord, EventKind, RecordFlags};
use hft_rt::ByteProducer;

/// One record to write: kind, flags (PART …) and body.
pub type Rec = (EventKind, RecordFlags, Vec<u8>);

/// Encoded ring length of a record (8 B header + body, rounded up to 8).
pub fn rec_len(r: &Rec) -> usize {
    CtlRecord {
        kind: r.0,
        flags: r.1,
        body: &r.2,
    }
    .encoded_len()
}

/// Writer side of the ctl ring.
pub trait CtlRing {
    /// Whether every record (encoded lengths) fits now.
    fn fits(&mut self, lens: &[usize]) -> bool;
    /// Writes the records in order; `false` = nothing was written (all-or-nothing).
    fn write_all(&mut self, recs: &[Rec]) -> bool;
}

fn encode(r: &Rec, out: &mut [u8]) -> bool {
    CtlRecord {
        kind: r.0,
        flags: r.1,
        body: &r.2,
    }
    .encode_into(out)
    .is_ok()
}

/// The production ring. `ByteProducer` exposes no free-space query, so `fits` is optimistic and
/// a request whose later records do not fit is cut after the records already committed (the
/// first record decides `Busy`); an `hft-rt` free-space query makes it exact (hft-ctl README
/// "已知限制").
impl CtlRing for ByteProducer {
    fn fits(&mut self, _lens: &[usize]) -> bool {
        true
    }

    fn write_all(&mut self, recs: &[Rec]) -> bool {
        for (i, r) in recs.iter().enumerate() {
            let n = rec_len(r);
            let Some(buf) = self.reserve(n as u32) else {
                return i > 0;
            };
            if !encode(r, buf) {
                return i > 0;
            }
            self.commit();
        }
        true
    }
}

/// An in-memory ring with an explicit free-space budget (tests and the PollTask harness).
#[derive(Debug, Default)]
pub struct VecRing {
    /// Remaining bytes.
    pub free: usize,
    /// Written records in order.
    pub out: Vec<(EventKind, RecordFlags, Vec<u8>)>,
    /// Requests refused for lack of space.
    pub refused: usize,
}

impl VecRing {
    pub fn with_free(free: usize) -> VecRing {
        VecRing {
            free,
            out: Vec::new(),
            refused: 0,
        }
    }
}

impl CtlRing for VecRing {
    fn fits(&mut self, lens: &[usize]) -> bool {
        let ok = lens.iter().sum::<usize>() <= self.free;
        if !ok {
            self.refused += 1;
        }
        ok
    }

    fn write_all(&mut self, recs: &[Rec]) -> bool {
        let lens: Vec<usize> = recs.iter().map(rec_len).collect();
        if !self.fits(&lens) {
            return false;
        }
        self.free -= lens.iter().sum::<usize>();
        self.out.extend(recs.iter().cloned());
        true
    }
}
