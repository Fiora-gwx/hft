//! Immutable offline input, independent of filenames and live-reader tail recovery.
use crate::common::harness::*;
use hft_event::EventKind;
use hft_journal::*;
use hft_types::Tsc;
use std::path::PathBuf;
use zerocopy::{FromBytes, IntoBytes};

const CAP: usize = 100_000;

struct Bundle {
    bytes: Vec<Vec<u8>>,
    seals: Vec<Vec<u8>>,
    padded: Vec<u8>,
}
impl Bundle {
    fn new() -> Self {
        let mut h = H::open0(mem_c0());
        for _ in 0..3 {
            h.events(2);
            h.w().append(F0, Tsc(9), &tcrc(7)).unwrap();
            h.w().end_event();
            h.w()
                .append_var(EventKind::T_MULTI, F0, Tsc(10), &[&multi_body(70_001)])
                .unwrap();
            h.w().end_event();
            h.roll(RollReason::Hourly);
        }
        let padded = h.seg(1);
        h.persist_all();
        Self {
            bytes: (1..=3)
                .map(|no| std::fs::read(h.cold_stem(no).with_extension("jrn")).unwrap())
                .collect(),
            seals: (1..=3)
                .map(|no| std::fs::read(h.cold_stem(no).with_extension("seal")).unwrap())
                .collect(),
            padded,
        }
    }
    fn write(&self, sealed: bool) -> Vec<PathBuf> {
        let dir = disk_dir("strict-input");
        ["z-first.bin", "a-second.bin", "middle-third.bin"]
            .iter()
            .enumerate()
            .map(|(i, name)| {
                let p = dir.join(name);
                std::fs::write(&p, &self.bytes[i]).unwrap();
                if sealed {
                    std::fs::write(p.with_extension("seal"), &self.seals[i]).unwrap();
                }
                p
            })
            .collect()
    }
}

fn set_header(b: &mut [u8], change: impl FnOnce(&mut SegmentHeaderFixed)) {
    let mut h = fixed(b);
    change(&mut h);
    h.header_crc32c = h.compute_crc();
    b[..3072].copy_from_slice(h.as_bytes());
}
fn set_seal(b: &mut [u8], change: impl FnOnce(&mut SealRecord)) {
    let mut s = SealRecord::read_from_bytes(b).unwrap();
    change(&mut s);
    s.crc32c = s.compute_crc();
    b.copy_from_slice(s.as_bytes());
}
fn problem(paths: &[PathBuf], cap: usize) -> FileProblem {
    JournalReader::open_files(paths, cap)
        .expect_err("bad input must fail before delivering any record")
        .problem
}
fn complete(r: &mut JournalReader) -> Vec<(u64, u16, Vec<u8>)> {
    let mut v = Vec::new();
    loop {
        match r.poll() {
            Next::Record(x) => v.push((x.hdr.seq, x.hdr.kind, x.body.to_vec())),
            Next::Event(x) => v.push((x.seq, x.kind.0, x.body.to_vec())),
            Next::End => return v,
            other => panic!("unexpected {other:?}"),
        }
    }
}

#[test]
fn jnl_130_published_capture_is_owned_read_only_and_bounded() {
    let cfg = mem_c0();
    let out = disk_dir("capture-writer");
    let mut w = JournalWriter::open_offline(&cfg, &out).unwrap();
    assert!(testing::published_bytes(&w).is_none());
    let info = RollInfo {
        utc_ns: UTC0,
        calib: hft_rt::CalibPair {
            tsc: Tsc(0),
            realtime_ns: UTC0,
            phc_ns: 0,
            err_ns: 0,
        },
        tsc_hz_milli: 1_000_000_000_000,
        configs: &[],
    };
    w.roll(RollReason::Startup, &info).unwrap().finish().unwrap();
    let boot = testing::published_bytes(&w).unwrap();
    let seq = w.append(F0, Tsc(1), &tin(7)).unwrap();
    assert_eq!(testing::published_bytes(&w).unwrap(), boot);
    w.end_event();
    assert_eq!(testing::published_bytes(&w).unwrap(), boot, "end is not publish");
    w.publish();
    let published = testing::published_bytes(&w).unwrap();
    assert_eq!(published.len(), u64_at(&published, 3072) as usize);
    assert!(published.len() > boot.len());
    let mut copy = testing::published_bytes(&w).unwrap();
    copy[0] ^= 1;
    assert_ne!(copy, published);
    assert_eq!(testing::published_bytes(&w).unwrap(), published);
    assert!(
        !out.join("prod/seq/0000000001.jrn").exists(),
        "capture performed IO"
    );

    w.append(F0, Tsc(2), &tin(8)).unwrap(); // unfinished event must stay outside the capture
    w.publish();
    assert_eq!(testing::published_bytes(&w).unwrap(), published);
    let root = disk_dir("captured-prefix");
    let dir = root.join("prod/seq");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("0000000001.jrn"), &published).unwrap();
    let mut c = ReaderConfig::new(
        root.clone(),
        root.join("absent"),
        InstanceName::Prod,
        "seq".into(),
    );
    c.end = EndAt::LastCompleteEvent;
    let mut reader = JournalReader::open(&c).unwrap();
    let got = complete(&mut reader);
    assert_eq!(got.last().unwrap().0, seq.0);
    assert_eq!(got.iter().filter(|r| r.1 == EventKind::T_IN.0).count(), 1);
    w.end_event();
    w.publish();
    w.seal_and_close().unwrap();
    let sealed = std::fs::read(out.join("prod/seq/0000000001.jrn")).unwrap();
    assert_eq!(&sealed[4096..published.len()], &published[4096..]);
}

#[test]
fn jnl_127_original_files_sorted_and_read_only() {
    let bundle = Bundle::new();
    for sealed in [false, true] {
        let paths = bundle.write(sealed);
        let shuffled = vec![paths[2].clone(), paths[0].clone(), paths[1].clone()];
        let mut reader = JournalReader::open_files(&shuffled, CAP).unwrap();
        let files = reader.file_segments().unwrap();
        assert_eq!(files.iter().map(|f| f.path.clone()).collect::<Vec<_>>(), paths);
        for (i, f) in files.iter().enumerate() {
            assert_eq!(f.header.segment_no, i as u64 + 1);
            assert_eq!(f.header.first_seq, fixed(&bundle.bytes[i]).first_seq);
            assert_eq!(f.file_bytes, bundle.bytes[i].len() as u64);
            assert_eq!(f.end_offset, f.file_bytes);
        }
        let all = complete(&mut reader);
        let last = reader.last_complete_event().unwrap();
        assert_eq!(reader.position(), last);
        assert!(all.windows(2).all(|v| v[0].0 < v[1].0));
        assert_eq!(all.iter().filter(|r| r.1 == EventKind::T_MULTI.0).count(), 3);
        for r in all.iter().filter(|r| r.1 == EventKind::T_MULTI.0) {
            assert_eq!(r.2, multi_body(70_001));
        }
        let mut one = JournalReader::open_files(&paths[..1], CAP).unwrap();
        let prefix = complete(&mut one);
        assert!(all.starts_with(&prefix));
        let mut tail = JournalReader::open_files(&paths[1..], CAP).unwrap();
        assert_eq!(complete(&mut tail), all[prefix.len()..]);
        for (i, p) in paths.iter().enumerate() {
            assert_eq!(std::fs::read(p).unwrap(), bundle.bytes[i]);
            if sealed {
                assert_eq!(std::fs::read(p.with_extension("seal")).unwrap(), bundle.seals[i]);
            }
        }
    }
    // A finalized full-capacity file may have zero reserve after committed_len, without a seal.
    let path = disk_dir("padded-input").join("capture");
    std::fs::write(&path, &bundle.padded).unwrap();
    let mut r = JournalReader::open_files(std::slice::from_ref(&path), CAP).unwrap();
    let f = &r.file_segments().unwrap()[0];
    assert!(f.end_offset < f.file_bytes);
    assert_eq!(f.end_offset, bundle.bytes[0].len() as u64);
    assert_eq!(
        complete(&mut r)
            .iter()
            .filter(|r| r.1 == EventKind::T_MULTI.0)
            .count(),
        1
    );
    assert_eq!(std::fs::read(path).unwrap(), bundle.padded);
}

#[test]
fn jnl_128_all_input_validated_before_delivery() {
    let bundle = Bundle::new();
    assert_eq!(problem(&[], CAP), FileProblem::Empty);
    for index in 0..3 {
        for mode in 0..12 {
            let paths = bundle.write(false);
            let mut b = bundle.bytes[index].clone();
            let recs = records(&b, b.len());
            let last = recs.last().unwrap().0;
            match mode {
                0 => b[0] ^= 1,
                1 => b[3071] ^= 1,
                2 => b.truncate(4000),
                3 => set_header(&mut b, |h| h.first_seq = 0),
                4 => b.truncate(last),
                5 => b.truncate(b.len() - 1),
                6 => b.extend_from_slice(&[0; 8]),
                7 => {
                    let at = recs.iter().find(|(_, h)| h.kind == EventKind::T_CRC.0).unwrap().0;
                    b[at + 24] ^= 1;
                }
                8 => set_header(&mut b, |h| h.first_seq += 1),
                9 => set_header(&mut b, |h| h.segment_no = u64::MAX),
                10 => {
                    b[4096 + 8..4096 + 16].copy_from_slice(&u64::MAX.to_le_bytes());
                }
                11 => {
                    b[last + 6..last + 8].copy_from_slice(&0u16.to_le_bytes());
                }
                _ => unreachable!(),
            }
            std::fs::write(&paths[index], b).unwrap();
            assert!(
                JournalReader::open_files(&paths, CAP).is_err(),
                "segment {index}, mutation {mode}"
            );
        }
    }
    let paths = bundle.write(false);
    assert_eq!(
        problem(&[paths[0].clone(), paths[2].clone()], CAP),
        FileProblem::Chain
    );
    assert_eq!(
        problem(&[paths[0].clone(), paths[0].clone()], CAP),
        FileProblem::Chain
    );
    for mode in 0..9 {
        let paths = bundle.write(false);
        let mut b = bundle.bytes[1].clone();
        set_header(&mut b, |h| match mode {
            0 => h.prev_header_sha256[0] ^= 1,
            1 => h.prev_last_seq += 1,
            2 => h.segment_no += 1,
            3 => h.instance = InstanceName::Sim.to_header(),
            4 => h.journal_name = JournalName::from("another").to_header(),
            5 => h.venue.0 += 1,
            6 => h.journal_kind = 255,
            7 => h.instance[15] = 1,
            8 => h.journal_name[31] = b'x',
            _ => unreachable!(),
        });
        std::fs::write(&paths[1], b).unwrap();
        assert_eq!(
            problem(&paths, CAP),
            if mode < 3 {
                FileProblem::Chain
            } else {
                FileProblem::Identity
            },
            "mode {mode}"
        );
    }
    for mode in 0..8 {
        let paths = bundle.write(true);
        let mut seal = bundle.seals[2].clone();
        match mode {
            0 => {
                seal[124] ^= 1;
            }
            1 => {
                seal.push(0);
            }
            _ => set_seal(&mut seal, |s| match mode {
                2 => s.sha256[0] ^= 1,
                3 => s.len -= 1,
                4 => s.segment_no -= 1,
                5 => s.first_seq += 1,
                6 => s.last_seq -= 1,
                7 => s.magic[0] ^= 1,
                _ => unreachable!(),
            }),
        }
        std::fs::write(paths[2].with_extension("seal"), seal).unwrap();
        assert_eq!(problem(&paths, CAP), FileProblem::Seal, "seal {mode}");
    }
    for mode in 0..4 {
        let path = disk_dir("padded-invalid").join("input");
        let mut b = bundle.padded.clone();
        match mode {
            0 => {
                *b.last_mut().unwrap() = 1;
            }
            1 => b[3072..3080].copy_from_slice(&u64::MAX.to_le_bytes()),
            2 => b[3072..3080].copy_from_slice(&0u64.to_le_bytes()),
            3 => b[3088..3092].copy_from_slice(&(SegState::Open as u32).to_le_bytes()),
            _ => unreachable!(),
        }
        std::fs::write(&path, b).unwrap();
        assert!(JournalReader::open_files(&[path], CAP).is_err());
    }
}

#[test]
fn jnl_129_preflight_reassembly_and_reopen_integrity() {
    let bundle = Bundle::new();
    let paths = bundle.write(false);
    assert_eq!(problem(&paths, 0), FileProblem::ReassemblyDisabled);
    assert!(matches!(
        problem(&paths, 70_000),
        FileProblem::Read {
            error: ReadErr::ReassemblyCap,
            ..
        }
    ));
    assert!(JournalReader::open_files(&paths, 70_001).is_ok());
    for mode in 0..6 {
        let paths = bundle.write(false);
        let mut b = bundle.bytes[2].clone();
        let parts: Vec<_> = records(&b, b.len())
            .into_iter()
            .filter(|(_, h)| h.kind == EventKind::T_MULTI.0)
            .collect();
        assert_eq!(parts.len(), 2);
        let p = parts[1].0;
        match mode {
            0 => b[p + 24..p + 28].copy_from_slice(&70_000u32.to_le_bytes()),
            1 => b[p + 16..p + 24].copy_from_slice(&99u64.to_le_bytes()),
            2 => b[p + 28..p + 32].copy_from_slice(&u32::MAX.to_le_bytes()),
            3 => b[p + 32..p + 34].copy_from_slice(&0u16.to_le_bytes()),
            4 => b[p + 34..p + 36].copy_from_slice(&0u16.to_le_bytes()),
            5 => b[p + 4..p + 6].copy_from_slice(&EventKind::T_IN.0.to_le_bytes()),
            _ => unreachable!(),
        }
        std::fs::write(&paths[2], b).unwrap();
        assert!(
            JournalReader::open_files(&paths, CAP).is_err(),
            "multipart {mode}"
        );
    }
    // Reopening a later file also checks its full digest, including records without a CRC.
    for deleted in [false, true] {
        let paths = bundle.write(false);
        let mut r = JournalReader::open_files(&paths, CAP).unwrap();
        if deleted {
            std::fs::remove_file(&paths[1]).unwrap();
        } else {
            let mut b = bundle.bytes[1].clone();
            let at = records(&b, b.len())
                .iter()
                .find(|(_, h)| h.kind == EventKind::T_IN.0)
                .unwrap()
                .0;
            b[at + 24] ^= 1;
            std::fs::write(&paths[1], b).unwrap();
        }
        loop {
            match r.poll() {
                Next::Record(_) | Next::Event(_) => assert_eq!(r.position().segment_no, 1),
                Next::Err(ReadErr::Corrupt) if !deleted => break,
                Next::Gap(g) if deleted => {
                    assert_eq!(g.reason, GapReason::Deleted);
                    break;
                }
                other => panic!("unexpected {other:?}"),
            }
        }
    }
}
