//! Offline verification (`cargo xtask journal-verify`, journal-archiver): journal-format
//! "校验与缺段检测" items 1–6.

#![allow(clippy::disallowed_types)] // allow-list: offline verification

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use hft_event::{EventKind, RecordFlags};
use sha2::Sha256;
use zerocopy::FromBytes;

use crate::config::{JournalKind, RollMark, VerifyIssue, VerifyOpts, VerifyReport};
use crate::fs;
use crate::layout::{
    FORMAT_VERSION, HEADER_FIXED_LEN, HEADER_LEN, OFF_COMMITTED, OFF_SYNCED, RECOVERY_GAP, SEAL_MAGIC,
    SEGMENT_MAGIC, SealRecord, SegmentHeaderFixed,
};
use crate::scan::{self, KindRules, hash_masked};

struct Found {
    path: PathBuf,
    seal: Option<PathBuf>,
    cold: bool,
}

/// Segment files below `dir`: hot style (`<no>.jrn` directly in `dir`) and cold style
/// (`<yyyymmdd>/<no>-<first>.jrn`).
fn find(dir: &Path) -> BTreeMap<u64, Found> {
    let mut m = BTreeMap::new();
    for (no, p) in fs::hot_list(dir) {
        m.insert(
            no,
            Found {
                path: p,
                seal: None,
                cold: false,
            },
        );
    }
    for (no, c) in fs::cold_list(dir) {
        let seal = c.seal();
        m.insert(
            no,
            Found {
                path: c.jrn(),
                seal: seal.exists().then_some(seal),
                cold: true,
            },
        );
    }
    m
}

fn u64_at(b: &[u8], o: usize) -> u64 {
    b.get(o..o + 8)
        .map_or(0, |x| u64::from_le_bytes(x.try_into().unwrap_or([0; 8])))
}

/// What the record scan of one segment found.
#[derive(Default)]
struct SegScan {
    last_seq: Option<u64>,
    last_is_end: bool,
}

/// Checks the journal directory `dir` (cold `<cold_root>/<instance>/<journal>` or a hot journal
/// directory).
pub fn verify(dir: &Path, opts: &VerifyOpts) -> VerifyReport {
    let mut rep = VerifyReport::default();
    let segs = find(dir);
    // Item 6: missing ranges.
    let mut prev_no: Option<u64> = None;
    for no in segs.keys() {
        if let Some(p) = prev_no
            && *no > p + 1
        {
            rep.missing.push((p + 1, no - 1));
        }
        prev_no = Some(*no);
    }
    let mut prev: Option<(u64, SegmentHeaderFixed, Option<u64>)> = None;
    for (no, f) in &segs {
        let issue = |item: u8, offset: u64| VerifyIssue {
            segment_no: *no,
            offset,
            item,
        };
        let Ok(b) = fs::read_file(&f.path) else {
            rep.issues.push(issue(1, 0));
            prev = None;
            continue;
        };
        // Item 1: header.
        let hdr = b
            .get(..HEADER_FIXED_LEN)
            .and_then(|x| SegmentHeaderFixed::read_from_bytes(x).ok())
            .filter(|h| {
                b.len() >= HEADER_LEN
                    && h.magic == SEGMENT_MAGIC
                    && h.format_version == FORMAT_VERSION
                    && h.header_crc32c == h.compute_crc()
            });
        let Some(h) = hdr else {
            rep.issues.push(issue(1, 0));
            prev = None;
            continue;
        };
        let kind = JournalKind::from_u8(h.journal_kind).unwrap_or(JournalKind::Seq);
        // Item 2: chain.
        if h.flags & RECOVERY_GAP != 0 {
            rep.recovery_gaps.push(*no);
        } else if let Some((pno, ph, plast)) = &prev
            && *pno + 1 == *no
        {
            let sha_ok = h.prev_header_sha256 == scan::header_sha(zerocopy::IntoBytes::as_bytes(ph));
            let seq_ok = h.first_seq == h.prev_last_seq + 1 && plast.is_none_or(|l| l == h.prev_last_seq);
            if !(sha_ok && seq_ok) {
                rep.issues.push(issue(2, 0));
            }
        }
        // Readable end.
        let bound = if !f.cold {
            (u64_at(&b, OFF_COMMITTED) as usize).min(b.len())
        } else if f.seal.is_some() {
            b.len()
        } else {
            (u64_at(&b, OFF_SYNCED) as usize).min(b.len())
        };
        let data = b.get(..bound).unwrap_or(&[]);
        let sc = scan_records(data, kind, opts, |item, off| {
            rep.issues.push(issue(item, off as u64))
        });
        // Item 5: seal.
        match &f.seal {
            Some(sp) => {
                let ok = std::fs::read(sp)
                    .ok()
                    .and_then(|x| SealRecord::read_from_bytes(&x).ok())
                    .is_some_and(|s| {
                        let mut hsh = <Sha256 as sha2::Digest>::new();
                        hash_masked(&mut hsh, 0, &b);
                        let sha: [u8; 32] = sha2::Digest::finalize(hsh).into();
                        s.magic == SEAL_MAGIC
                            && s.crc32c == s.compute_crc()
                            && s.sha256 == sha
                            && s.len == b.len() as u64
                            && Some(s.last_seq) == sc.last_seq
                            && s.segment_no == *no
                            && sc.last_is_end
                    });
                if !ok {
                    rep.issues.push(issue(5, 0));
                }
            }
            None => rep.unsealed.push(*no),
        }
        prev = Some((*no, h, sc.last_seq));
    }
    rep
}

/// Items 3 and 4 over `[4096, len)`; reports through `issue(item, offset)`.
fn scan_records(b: &[u8], kind: JournalKind, opts: &VerifyOpts, mut issue: impl FnMut(u8, usize)) -> SegScan {
    let rules = KindRules {
        journal: kind,
        require_registered: true,
        production: opts.production,
    };
    let mut out = SegScan::default();
    let mut off = HEADER_LEN;
    let mut seq: Option<u64> = None;
    // Multi-part group: (start offset, next part, parts).
    let mut group: Option<(usize, u16, u16)> = None;
    // Boot block: records counted from Begin; BootDone seen; n_books declared; keyframes seen.
    let mut n_rec: u32 = 0;
    let mut boot_open = false;
    let mut boot_ok = !kind.has_boot();
    let mut first = true;
    let mut n_books: Option<u32> = None;
    let mut keyframes: u32 = 0;
    let mut item3 = false;
    while off < b.len() {
        let Some(buf) = b.get(off..) else { break };
        let want = seq.map_or_else(
            || {
                buf.get(8..16)
                    .map_or(0, |x| u64::from_le_bytes(x.try_into().unwrap_or([0; 8])))
            },
            |s| s + 1,
        );
        let v = match scan::check(buf, want, rules) {
            Ok(Some(v)) => v,
            Ok(None) | Err(_) => {
                issue(3, off);
                item3 = true;
                break;
            }
        };
        let k = EventKind(v.hdr.kind);
        let flags = RecordFlags(v.hdr.flags);
        // Groups.
        if scan::is_multi(v.hdr.kind) {
            let Some(ph) = scan::part_hdr(v.body) else {
                issue(3, off);
                item3 = true;
                break;
            };
            let bad = match group {
                None => ph.part != 0,
                Some((_, next, parts)) => ph.part != next || ph.parts != parts,
            };
            let last = ph.part.wrapping_add(1) == ph.parts;
            if bad || flags.contains(RecordFlags::PART) == last {
                issue(3, off);
                item3 = true;
                break;
            }
            group = if last {
                None
            } else {
                Some((group.map_or(off, |g| g.0), ph.part + 1, ph.parts))
            };
        } else if let Some((g, _, _)) = group {
            issue(3, g);
            item3 = true;
            break;
        }
        // Boot block.
        if kind.has_boot() {
            if first && !(k == EventKind::SEGMENT_ROLL && v.body.get(32) == Some(&(RollMark::Begin as u8))) {
                issue(4, off);
                boot_ok = true;
            } else if first {
                boot_open = true;
            }
            if boot_open {
                n_rec += 1;
                if k == EventKind::STATE_SNAPSHOT
                    && let Some(ph) = scan::part_hdr(v.body)
                    && ph.part == 0
                    && matches!(v.body.get(16).copied(), Some(1) | Some(4))
                {
                    let nb = v
                        .body
                        .get(16 + 32..16 + 36)
                        .map_or(0, |x| u32::from_le_bytes(x.try_into().unwrap_or([0; 4])));
                    n_books = Some(nb);
                }
                if k == EventKind::L2_SNAPSHOT
                    && let Some(ph) = scan::part_hdr(v.body)
                    && ph.part == 0
                    && v.body.get(16 + 64) == Some(&2)
                {
                    keyframes += 1;
                }
                if k == EventKind::SEGMENT_ROLL && v.body.get(32) == Some(&(RollMark::BootDone as u8)) {
                    boot_open = false;
                    boot_ok = true;
                    let br = v
                        .body
                        .get(36..40)
                        .map_or(0, |x| u32::from_le_bytes(x.try_into().unwrap_or([0; 4])));
                    if br != n_rec || n_books.is_some_and(|n| n != keyframes) {
                        issue(4, off);
                    }
                }
            }
        }
        first = false;
        out.last_seq = Some(v.hdr.seq);
        out.last_is_end = k == EventKind::SEGMENT_ROLL && v.body.get(32) == Some(&(RollMark::End as u8));
        seq = Some(v.hdr.seq);
        off += v.hdr.len as usize;
    }
    if !item3 && let Some((g, _, _)) = group {
        issue(3, g);
    }
    if !boot_ok && b.len() > HEADER_LEN {
        issue(4, HEADER_LEN);
    }
    out
}
