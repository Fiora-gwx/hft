//! Strict, immutable offline input. Paths are supplied by a cold caller; filenames do not
//! encode identity. Validation maps one segment at a time and never opens a writer.

use std::path::{Path, PathBuf};

use hft_event::RecordFlags;
use sha2::{Digest, Sha256};
use zerocopy::{FromBytes, IntoBytes};

use crate::config::{InstanceName, JournalKind, JournalName, ReadErr, SegState, VerifyOpts};
use crate::fs;
use crate::layout::{
    FORMAT_VERSION, HEADER_FIXED_LEN, HEADER_LEN, OFF_COMMITTED, OFF_STATE, RECOVERY_GAP, SEAL_MAGIC,
    SEGMENT_MAGIC, SealRecord, SegmentHeaderFixed,
};
use crate::scan;

/// An offline input failure, before the caller receives any record.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FileOpenError {
    pub path: PathBuf,
    pub problem: FileProblem,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FileProblem {
    Empty,
    Io,
    Header,
    Format(u16),
    Identity,
    Chain,
    Boundary,
    Seal,
    /// Items 3/4 of the journal verifier, including the byte offset in the file.
    Verify {
        item: u8,
        offset: u64,
    },
    Read {
        error: ReadErr,
        offset: u64,
    },
    ReassemblyDisabled,
}

/// Validated identity and physical extent of one immutable offline segment. Entries are in
/// `first_seq` order. The original path is used directly, without staging copies or renaming.
#[derive(Clone, Debug)]
pub struct FileSegment {
    pub path: PathBuf,
    pub header: SegmentHeaderFixed,
    pub file_bytes: u64,
    /// End of the complete End record; excludes zero capacity padding of an offline writer.
    pub end_offset: u64,
    pub last_seq: u64,
    pub(crate) digest: [u8; 32],
    end_other: u64,
}

pub(crate) fn error(path: &Path, problem: FileProblem) -> FileOpenError {
    FileOpenError {
        path: path.to_path_buf(),
        problem,
    }
}

fn u64_at(b: &[u8], offset: usize) -> u64 {
    b.get(offset..offset + 8)
        .map_or(0, |x| u64::from_le_bytes(x.try_into().unwrap_or([0; 8])))
}

pub(crate) fn identity(h: &SegmentHeaderFixed) -> Option<(InstanceName, JournalName)> {
    let instance = [InstanceName::Prod, InstanceName::Sim]
        .into_iter()
        .find(|i| i.to_header() == h.instance)?;
    let len = h.journal_name.iter().position(|b| *b == 0).unwrap_or(32);
    if h.journal_name[len..].iter().any(|b| *b != 0) {
        return None;
    }
    let name = JournalName::from(std::str::from_utf8(&h.journal_name[..len]).ok()?);
    name.is_valid().then_some((instance, name))
}

fn inspect(path: &Path, b: &[u8]) -> Result<FileSegment, FileOpenError> {
    let fail = |p| error(path, p);
    let h = b
        .get(..HEADER_FIXED_LEN)
        .and_then(|x| SegmentHeaderFixed::read_from_bytes(x).ok())
        .ok_or_else(|| fail(FileProblem::Header))?;
    if b.len() < HEADER_LEN
        || h.magic != SEGMENT_MAGIC
        || h.header_len != HEADER_LEN as u32
        || h.header_crc32c != h.compute_crc()
        || h.segment_no == 0
        || h.first_seq == 0
        || h.segment_no == u64::MAX
        || h.segment_bytes < b.len() as u64
    {
        return Err(fail(FileProblem::Header));
    }
    if h.format_version != FORMAT_VERSION {
        return Err(fail(FileProblem::Format(h.format_version)));
    }
    let kind = JournalKind::from_u8(h.journal_kind).ok_or_else(|| fail(FileProblem::Identity))?;
    identity(&h).ok_or_else(|| fail(FileProblem::Identity))?;
    // A full-capacity offline writer file has a final committed boundary and zero reserve.
    // A compact cold copy is read to physical EOF, independent of its stale shared rows.
    let state = b
        .get(OFF_STATE..OFF_STATE + 4)
        .map_or(0, |x| u32::from_le_bytes(x.try_into().unwrap_or([0; 4])));
    let bound = if b.len() as u64 == h.segment_bytes && state == SegState::Sealed as u32 {
        let end = usize::try_from(u64_at(b, OFF_COMMITTED)).map_err(|_| fail(FileProblem::Boundary))?;
        if end < HEADER_LEN || end > b.len() || b[end..].iter().any(|v| *v != 0) {
            return Err(fail(FileProblem::Boundary));
        }
        end
    } else {
        b.len()
    };
    let mut issue = None;
    let sc = crate::verify::scan_records(
        &b[..bound],
        kind,
        &VerifyOpts {
            production: !scan::dev_kinds_known(),
        },
        |item, offset| {
            issue.get_or_insert(FileProblem::Verify {
                item,
                offset: offset as u64,
            });
        },
    );
    if let Some(problem) = issue {
        return Err(fail(problem));
    }
    let last = sc.last_seq.ok_or_else(|| fail(FileProblem::Boundary))?;
    if sc.first_seq != Some(h.first_seq)
        || !sc.last_is_end
        || !RecordFlags(sc.last_flags).contains(RecordFlags::EVENT_END)
        || RecordFlags(sc.last_flags).contains(RecordFlags::PART)
        || last == u64::MAX
    {
        return Err(fail(FileProblem::Boundary));
    }
    let seal_path = path.with_extension("seal");
    if seal_path.try_exists().map_err(|_| fail(FileProblem::Io))? {
        let size = std::fs::metadata(&seal_path)
            .map_err(|_| fail(FileProblem::Io))?
            .len();
        if size != size_of::<SealRecord>() as u64 {
            return Err(fail(FileProblem::Seal));
        }
        let bytes =
            fs::read_prefix(&seal_path, size_of::<SealRecord>()).map_err(|_| fail(FileProblem::Io))?;
        let s = SealRecord::read_from_bytes(&bytes).map_err(|_| fail(FileProblem::Seal))?;
        let mut hash = Sha256::new();
        scan::hash_masked(&mut hash, 0, b);
        let sha: [u8; 32] = hash.finalize().into();
        if s.magic != SEAL_MAGIC
            || s.crc32c != s.compute_crc()
            || s.sha256 != sha
            || s.len != b.len() as u64
            || s.segment_no != h.segment_no
            || s.first_seq != h.first_seq
            || s.last_seq != last
            || bound != b.len()
        {
            return Err(fail(FileProblem::Seal));
        }
    }
    Ok(FileSegment {
        path: path.to_path_buf(),
        header: h,
        file_bytes: b.len() as u64,
        end_offset: bound as u64,
        last_seq: last,
        digest: Sha256::digest(b).into(),
        end_other: sc.end_other,
    })
}

pub(crate) fn load(paths: &[PathBuf]) -> Result<Vec<FileSegment>, FileOpenError> {
    if paths.is_empty() {
        return Err(error(Path::new(""), FileProblem::Empty));
    }
    let mut files = Vec::with_capacity(paths.len());
    for path in paths {
        let mapped = fs::cold_open(path).map_err(|_| error(path, FileProblem::Io))?;
        let result = inspect(path, mapped.map.bytes(0, mapped.map.len()));
        fs::release(None, mapped.map);
        files.push(result?);
    }
    files.sort_by_key(|s| s.header.first_seq);
    for pair in files.windows(2) {
        let (a, b) = (&pair[0], &pair[1]);
        let (p, h) = (&a.header, &b.header);
        if p.instance != h.instance
            || p.journal_name != h.journal_name
            || p.journal_kind != h.journal_kind
            || p.venue != h.venue
        {
            return Err(error(&b.path, FileProblem::Identity));
        }
        if p.segment_no.checked_add(1) != Some(h.segment_no)
            || p.first_seq >= h.first_seq
            || h.prev_header_sha256 != scan::header_sha(p.as_bytes())
            || h.prev_last_seq != a.last_seq
            || (h.flags & RECOVERY_GAP == 0 && a.last_seq.checked_add(1) != Some(h.first_seq))
            || (a.end_other != 0 && a.end_other != h.segment_no)
        {
            return Err(error(&b.path, FileProblem::Chain));
        }
    }
    Ok(files)
}
