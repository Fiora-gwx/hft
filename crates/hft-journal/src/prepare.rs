//! `SegmentPreparer`: keeps one prepared (created, pre-faulted, `[P]` header filled) next segment
//! per journal (README "预创建").

use std::path::PathBuf;

use hft_event::{KINDS, Layout};
use hft_rt::Producer;
use hft_types::VenueId;

use crate::config::{JournalConfig, JournalKind, PrepState, SegState};
use crate::fs;
use crate::layout::FsKind;
use crate::layout::{FORMAT_VERSION, HEADER_LEN, SEGMENT_MAGIC, SchemaEntry, SegmentHeaderFixed};
use crate::oplog::{self, Op};
use crate::scan::{header_sha, read_header};
use crate::writer::PreparedSegment;

/// What the preparer needs besides the journal configuration.
#[derive(Debug)]
pub struct PrepEnv {
    pub writer_epoch: u32,
    pub build_git: [u8; 20],
    pub host_profile_sha256: [u8; 32],
    pub to_writer: Producer<PreparedSegment>,
}

/// Prepares the next segment of one journal on the persist thread.
#[derive(Debug)]
pub struct SegmentPreparer {
    cfg: JournalConfig,
    hot_jdir: PathBuf,
    cold_jdir: PathBuf,
    env: PrepEnv,
}

/// The `[P]` fields of a new segment header.
pub(crate) fn p_header(
    cfg: &JournalConfig,
    epoch: u32,
    git: [u8; 20],
    host: [u8; 32],
    no: u64,
    prev_sha: [u8; 32],
) -> SegmentHeaderFixed {
    let mut h = SegmentHeaderFixed::zeroed();
    h.magic = SEGMENT_MAGIC;
    h.format_version = FORMAT_VERSION;
    h.journal_kind = cfg.kind as u8;
    h.venue = if cfg.kind == JournalKind::Raw {
        cfg.venue
    } else {
        VenueId(0)
    };
    h.header_len = HEADER_LEN as u32;
    h.journal_name = cfg.name.to_header();
    h.instance = cfg.instance.to_header();
    h.segment_no = no;
    h.segment_bytes = cfg.segment_bytes;
    h.writer_epoch = epoch;
    h.prev_header_sha256 = prev_sha;
    h.host_profile_sha256 = host;
    h.build_git = git;
    let mut n = 0u16;
    for (slot, k) in h.schema.iter_mut().zip(KINDS.iter()) {
        let size = match k.layout {
            Layout::Fixed { size } => u32::from(size),
            Layout::Var { prefix, .. } | Layout::Multi { prefix } => u32::from(prefix),
        };
        *slot = SchemaEntry {
            kind: k.kind.0,
            version: k.schema_version,
            size,
        };
        n += 1;
    }
    h.n_schema = n;
    h
}

impl SegmentPreparer {
    pub fn new(cfg: &JournalConfig, env: PrepEnv) -> SegmentPreparer {
        SegmentPreparer {
            cfg: cfg.clone(),
            hot_jdir: cfg.hot_journal_dir(),
            cold_jdir: cfg.cold_journal_dir(),
            env,
        }
    }

    /// Ensures a prepared next segment exists (README `PrepState`).
    pub fn poll(&mut self) -> PrepState {
        match fs::ram_fs(&self.cfg.hot_dir) {
            Ok(FsKind::Tmpfs | FsKind::Hugetlbfs { .. }) => {}
            _ => return PrepState::Failed,
        }
        if fs::ensure_hot_dir(&self.hot_jdir).is_err() {
            return PrepState::Failed;
        }
        // Newest segment of ours (foreign files are ignored).
        let mut latest: Option<(u64, SegmentHeaderFixed)> = None;
        for (no, p) in fs::hot_list(&self.hot_jdir).iter().rev() {
            if let Some((f, st)) = read_header(p) {
                if st == SegState::Preparing as u32 {
                    return PrepState::Ready;
                }
                latest = Some((*no, f));
                break;
            }
        }
        if latest.is_none()
            && let Some((no, c)) = fs::cold_list(&self.cold_jdir).into_iter().next_back()
            && let Some((f, _)) = read_header(&c.jrn())
        {
            latest = Some((no, f));
        }
        let (no, prev_sha) = match &latest {
            Some((no, f)) => (no + 1, header_sha(zerocopy::IntoBytes::as_bytes(f))),
            None => (1, [0u8; 32]),
        };
        let path = self.hot_jdir.join(fs::hot_seg_name(no));
        let staging = self.hot_jdir.join(fs::hot_staging_name(no));
        let Ok(len) = usize::try_from(self.cfg.segment_bytes) else {
            return PrepState::Failed;
        };
        let Ok(map) = fs::hot_create(&staging, len, self.cfg.page) else {
            return PrepState::Failed;
        };
        let h = p_header(
            &self.cfg,
            self.env.writer_epoch,
            self.env.build_git,
            self.env.host_profile_sha256,
            no,
            prev_sha,
        );
        map.write_fixed(&h);
        // A crash during create/truncate/populate/header initialization leaves only the private
        // staging name. Recovery can discard it without guessing whether a .jrn is committed.
        if fs::hot_publish(&staging, &path).is_err() {
            fs::release(Some(&staging), map);
            let _ = fs::hot_unlink(&staging);
            return PrepState::Failed;
        }
        if self
            .env
            .to_writer
            .try_push(PreparedSegment { map, segment_no: no })
            .is_err()
        {
            let _ = fs::hot_unlink(&path);
            fs::release(Some(&path), map);
            return PrepState::Failed;
        }
        oplog::record(Op::Prepare(no));
        PrepState::Prepared
    }
}
