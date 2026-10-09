//! `cargo xtask journal-verify` (tools/TESTS.md TOOL-083, TOOL-084). The 4-segment `seq` journal is
//! written in-process with hft-journal's writer (heap hot segments, cold copies on disk): segments 1–3
//! rolled and sealed, segment 4 open; every segment holds `Bbo` events after its boot block.

use crate::common;

use std::path::{Path, PathBuf};

use common::Fixture;
use hft_event::{Bbo, EventKind, MarketHdr, RecordFlags, RecordHeader};
use hft_journal::*;
use hft_rt::{Backing, CalibPair, spsc};
use hft_types::{InstId, Px, Qty, Tsc, VenueId};
use zerocopy::{FromBytes, FromZeros};

/// 2026-10-01T00:00:00Z.
const UTC0: i64 = 1_790_812_800_000_000_000;

fn bbo(i: u64) -> Bbo {
    let mut h = MarketHdr::new_zeroed();
    h.inst = InstId(1);
    h.ex_ts_ns = UTC0 + i as i64;
    Bbo {
        h,
        bid_px: Px(100),
        bid_qty: Qty(5),
        ask_px: Px(101),
        ask_qty: Qty(5),
        update_id: i,
    }
}

/// Writes the journal under `f`; returns the cold journal directory.
fn journal(f: &Fixture) -> PathBuf {
    let hot = f.path("hot");
    testing::mem_root(&hot);
    let cfg = JournalConfig {
        name: "seq".into(),
        kind: JournalKind::Seq,
        venue: VenueId(0),
        instance: InstanceName::Prod,
        hot_dir: hot.clone(),
        cold_root: f.path("cold"),
        segment_bytes: 1 << 20,
        page: PageSize::Normal,
        size_idle_pct: DEFAULT_SIZE_IDLE_PCT,
        size_valve_pct: DEFAULT_SIZE_VALVE_PCT,
        hot_keep: 8,
    };
    let (ptx, prx) = spsc(4, Backing::Heap).unwrap();
    let (rtx, rrx) = spsc(1024, Backing::Heap).unwrap();
    let env = WriterEnv {
        writer_epoch: 1,
        build_git: [0; 20],
        host_profile_sha256: [0; 32],
        prepared: prx,
        retired: rtx,
    };
    let (mut w, rep) = JournalWriter::open(&cfg, env).unwrap();
    let mut prep = SegmentPreparer::new(
        &cfg,
        PrepEnv {
            writer_epoch: 1,
            build_git: [0; 20],
            host_profile_sha256: [0; 32],
            to_writer: ptx,
        },
    );
    let mut pers = Persister::new(PersistConfig::default());
    pers.add(&cfg);
    let _ret = HotRetirer::new(&cfg, rrx);
    let mut mono = 0u64;
    let configs: Vec<hft_event::BlobRef> = Vec::new();
    let roll = |w: &mut JournalWriter, prep: &mut SegmentPreparer, reason: RollReason| {
        assert_ne!(prep.poll(), PrepState::Failed);
        let info = RollInfo {
            utc_ns: UTC0,
            calib: CalibPair::default(),
            tsc_hz_milli: 0,
            configs: &configs,
        };
        w.roll(reason, &info).unwrap().finish().unwrap();
    };
    roll(&mut w, &mut prep, rep.boot_reason);
    for seg in 0..4u64 {
        if seg > 0 {
            roll(&mut w, &mut prep, RollReason::Hourly);
        }
        for i in 0..100 {
            w.append(RecordFlags(0), Tsc(i), &bbo(seg * 100 + i)).unwrap();
            w.end_event();
            w.publish();
        }
    }
    for _ in 0..10_000 {
        mono += 50_000_000;
        let p = pers.poll(mono, UTC0);
        if p.bytes_copied == 0 && p.sealed == 0 && p.syncs == 0 {
            break;
        }
    }
    std::mem::forget(w);
    cfg.cold_journal_dir()
}

/// Cold file of segment `no` with extension `ext`.
fn seg_file(dir: &Path, no: u64, ext: &str) -> PathBuf {
    walkdir::WalkDir::new(dir)
        .into_iter()
        .flatten()
        .map(|e| e.into_path())
        .find(|p| {
            let n = p.file_name().unwrap().to_string_lossy().into_owned();
            n.ends_with(&format!(".{ext}"))
                && n.split('-').next().and_then(|x| x.parse::<u64>().ok()) == Some(no)
        })
        .unwrap_or_else(|| panic!("segment {no} .{ext} not found"))
}

/// `(offset, header)` of every record of a segment image.
fn records(b: &[u8]) -> Vec<(usize, RecordHeader)> {
    let mut out = Vec::new();
    let mut off = HEADER_LEN;
    while let Some(x) = b.get(off..off + 24) {
        let h = RecordHeader::read_from_bytes(x).unwrap();
        if h.len == 0 {
            break;
        }
        out.push((off, h));
        off += h.len as usize;
    }
    out
}

fn patch(p: &Path, off: usize, bytes: &[u8]) {
    let mut b = std::fs::read(p).unwrap();
    b[off..off + bytes.len()].copy_from_slice(bytes);
    std::fs::write(p, b).unwrap();
}

/// Rewrites the CRC (last 4 bytes) of the record at `off`.
fn fix_crc(p: &Path, off: usize) {
    let b = std::fs::read(p).unwrap();
    let len = u32::from_le_bytes(b[off..off + 4].try_into().unwrap()) as usize;
    let crc = crc32c::crc32c(&b[off..off + len - 4]);
    patch(p, off + len - 4, &crc.to_le_bytes());
}

fn run(f: &Fixture, dir: &Path, extra: &[&str]) -> common::Run {
    let d = dir.display().to_string();
    let mut args = vec!["journal-verify", d.as_str()];
    args.extend_from_slice(extra);
    f.xtask(&args)
}

/// TOOL-083.3's change: flip the lowest bit of `len` of segment 2's record at offset 4096 (the boot
/// block's `Begin`; `seq` journal records carry a CRC only for CRC kinds, this one has none).
fn corrupt_seg2(dir: &Path) {
    let p = seg_file(dir, 2, "jrn");
    let b = std::fs::read(&p).unwrap();
    let (off, h) = records(&b)[0];
    assert_eq!(off, 4096);
    assert_eq!(EventKind(h.kind), EventKind::SEGMENT_ROLL);
    patch(&p, off, &[b[off] ^ 0x01]);
}

#[test]
fn tool_083_corruption_reports() {
    // TOOL-083.1
    let f = Fixture::empty("jv-1");
    let dir = journal(&f);
    let r = run(&f, &dir, &[]);
    assert_eq!(r.code, 0, "TOOL-083.1: {}", r.stderr);
    let j = r.json.unwrap();
    assert_eq!(j["segments"].as_array().unwrap().len(), 4, "TOOL-083.1: {j}");
    assert_eq!(j["unsealed"], serde_json::json!([4]), "TOOL-083.1");

    // TOOL-083.2: sealed segment 3 truncated at 8192.
    let f = Fixture::empty("jv-2");
    let dir = journal(&f);
    let p = seg_file(&dir, 3, "jrn");
    assert!(seg_file(&dir, 3, "seal").exists());
    let b = std::fs::read(&p).unwrap();
    assert!(b.len() > 8192);
    std::fs::write(&p, &b[..8192]).unwrap();
    let r = run(&f, &dir, &[]);
    assert_eq!(r.code, 1, "TOOL-083.2: {}", r.stderr);
    assert!(
        r.stderr.contains("segment 3 offset 8192"),
        "TOOL-083.2: {}",
        r.stderr
    );

    // TOOL-083.3: one bit flipped in segment 2's record at offset 4096.
    let f = Fixture::empty("jv-3");
    let dir = journal(&f);
    corrupt_seg2(&dir);
    let r = run(&f, &dir, &[]);
    assert_eq!(r.code, 1, "TOOL-083.3: {}", r.stderr);
    assert!(
        r.stderr.contains("segment 2 offset 4096"),
        "TOOL-083.3: {}",
        r.stderr
    );

    // TOOL-083.4: segment 2 deleted.
    let f = Fixture::empty("jv-4");
    let dir = journal(&f);
    std::fs::remove_file(seg_file(&dir, 2, "jrn")).unwrap();
    std::fs::remove_file(seg_file(&dir, 2, "seal")).unwrap();
    let r = run(&f, &dir, &[]);
    assert_eq!(r.code, 1, "TOOL-083.4: {}", r.stderr);
    assert!(
        r.stderr.contains("missing segments 2..=2"),
        "TOOL-083.4: {}",
        r.stderr
    );

    // TOOL-083.5: segment 1's boot block without BootDone (the mark rewritten to Begin, CRC fixed).
    let f = Fixture::empty("jv-5");
    let dir = journal(&f);
    let p = seg_file(&dir, 1, "jrn");
    let b = std::fs::read(&p).unwrap();
    let (off, _) = records(&b)
        .into_iter()
        .find(|(o, h)| {
            EventKind(h.kind) == EventKind::SEGMENT_ROLL && b[o + 24 + 32] == RollMark::BootDone as u8
        })
        .expect("BootDone");
    patch(&p, off + 24 + 32, &[RollMark::Begin as u8]);
    fix_crc(&p, off);
    let r = run(&f, &dir, &[]);
    assert_eq!(r.code, 1, "TOOL-083.5: {}", r.stderr);
    assert!(
        r.stderr.contains("segment 1 offset 4096: boot block"),
        "TOOL-083.5: {}",
        r.stderr
    );

    // TOOL-083.6: last segment marked RECOVERY_GAP, its predecessor's tail missing.
    let f = Fixture::empty("jv-6");
    let dir = journal(&f);
    let p = seg_file(&dir, 4, "jrn");
    let b = std::fs::read(&p).unwrap();
    let first = u64::from_le_bytes(b[72..80].try_into().unwrap());
    patch(&p, 72, &(first + 1).to_le_bytes());
    patch(&p, 117, &[RECOVERY_GAP]);
    let b = std::fs::read(&p).unwrap();
    patch(&p, 3068, &crc32c::crc32c(&b[..3068]).to_le_bytes());
    let r = run(&f, &dir, &[]);
    assert_eq!(r.code, 0, "TOOL-083.6: {}", r.stderr);
    assert!(r.stderr.contains("可能丢失的尾部"), "TOOL-083.6: {}", r.stderr);
    assert_eq!(r.json.unwrap()["recovery_gaps"], serde_json::json!([4]));
}

#[test]
fn tool_084_from_segment_json() {
    let f = Fixture::empty("jv-084");
    let dir = journal(&f);
    corrupt_seg2(&dir);
    let r = run(&f, &dir, &["--from-segment", "3", "--json"]);
    assert_eq!(r.code, 0, "{}", r.stderr);
    let j: serde_json::Value = serde_json::from_str(&r.stdout).unwrap();
    let nos: Vec<u64> = j["segments"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| s["segment_no"].as_u64().unwrap())
        .collect();
    assert_eq!(nos, vec![3, 4]);
}
