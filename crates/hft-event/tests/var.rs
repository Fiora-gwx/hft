//! Multi prefixes, splitting and reassembly (EVT-161, EVT-162, EVT-164, EVT-165).

use hft_event::*;
use zerocopy::{FromZeros, IntoBytes};

fn l2(origin: u8, bids: u16, asks: u16, tail: usize) -> Vec<u8> {
    let mut h = L2SnapHdr::new_zeroed();
    h.origin = origin;
    h.n_bids = bids;
    h.n_asks = asks;
    let mut b = h.as_bytes().to_vec();
    b.extend(vec![1u8; tail]);
    b
}

fn blob(len: u32, boot: u8, kind: u16, tail: usize) -> Vec<u8> {
    let mut b = vec![0u8; 72];
    b[0..2].copy_from_slice(&kind.to_le_bytes());
    b[4..8].copy_from_slice(&len.to_le_bytes());
    b[64] = boot;
    b.extend(vec![2u8; tail]);
    b
}

fn raw(plen: u32, tail: usize) -> Vec<u8> {
    let mut h = RawHdr::new_zeroed();
    h.payload_len = plen;
    let mut b = h.as_bytes().to_vec();
    b.extend(vec![3u8; tail]);
    b
}

#[test]
fn evt_161_multi_view() {
    use DecodeErr::*;
    let v = l2(3, 2, 1, 48);
    let (h, t) = multi_view::<L2SnapHdr>(&v).unwrap();
    assert_eq!((h.n_bids, t.len()), (2, 48), "EVT-161.1");
    assert_eq!(
        multi_view::<L2SnapHdr>(&l2(0, 2, 1, 48)).unwrap_err(),
        InvalidEnum,
        "EVT-161.2"
    );
    assert_eq!(
        multi_view::<L2SnapHdr>(&l2(4, 2, 1, 48)).unwrap_err(),
        InvalidEnum,
        "EVT-161.2"
    );
    assert_eq!(
        multi_view::<L2SnapHdr>(&l2(3, 2, 1, 32)).unwrap_err(),
        Short,
        "EVT-161.3"
    );
    assert_eq!(
        multi_view::<L2SnapHdr>(&l2(3, 2, 1, 64)).unwrap_err(),
        TooLong,
        "EVT-161.3"
    );
    assert_eq!(
        multi_view::<L2SnapHdr>(&l2(3, 0, 0, 0)[..71]).unwrap_err(),
        Short,
        "EVT-161.4"
    );
    assert!(multi_view::<BlobHdr>(&blob(100, 1, 1, 100)).is_ok(), "EVT-161.5");
    assert_eq!(
        multi_view::<BlobHdr>(&blob(100, 1, 1, 99)).unwrap_err(),
        Short,
        "EVT-161.6"
    );
    assert_eq!(
        multi_view::<BlobHdr>(&blob(100, 2, 1, 100)).unwrap_err(),
        InvalidEnum,
        "EVT-161.7"
    );
    assert_eq!(
        multi_view::<BlobHdr>(&blob(100, 1, 0, 100)).unwrap_err(),
        InvalidEnum,
        "EVT-161.8"
    );
    assert_eq!(
        multi_view::<BlobHdr>(&blob(100, 1, 7, 100)).unwrap_err(),
        InvalidEnum,
        "EVT-161.8"
    );
    assert!(multi_view::<RawHdr>(&raw(0, 0)).is_ok(), "EVT-161.9");
    assert_eq!(
        multi_view::<RawHdr>(&raw(10, 11)).unwrap_err(),
        TooLong,
        "EVT-161.10"
    );
}

#[test]
fn evt_162_part_iter_bounds() {
    let parts = |n: usize| {
        PartIter::new(&vec![0u8; n]).map(|it| it.map(|(h, c, f)| (h, c.len(), f)).collect::<Vec<_>>())
    };
    let p = parts(0).unwrap();
    assert_eq!(p.len(), 1, "EVT-162.1");
    assert_eq!(
        (p[0].0.total_len, p[0].0.chunk_len, p[0].0.part, p[0].0.parts),
        (0, 0, 0, 1),
        "EVT-162.1"
    );
    assert!(!p[0].2.contains(RecordFlags::PART), "EVT-162.1");
    let p = parts(65_488).unwrap();
    assert_eq!(
        (p.len(), p[0].1, p[0].2.contains(RecordFlags::PART)),
        (1, 65_488, false),
        "EVT-162.2"
    );
    let p = parts(65_489).unwrap();
    assert_eq!(p.len(), 2, "EVT-162.3");
    assert_eq!((p[0].1, p[1].1), (65_488, 1), "EVT-162.3");
    assert!(
        p[0].2.contains(RecordFlags::PART) && !p[1].2.contains(RecordFlags::PART),
        "EVT-162.3"
    );
    assert!(
        p.iter().all(|x| x.0.parts == 2 && x.0.total_len == 65_489),
        "EVT-162.3"
    );
    let p = parts(130_976).unwrap();
    assert_eq!((p.len(), p[0].1, p[1].1), (2, 65_488, 65_488), "EVT-162.4");
    // 65 535 × 65 488 B: PartIter only slices, the zeroed pages are never touched.
    let max = 65_535usize * 65_488;
    let big = vec![0u8; max + 1];
    let it = PartIter::new(&big[..max]).unwrap();
    assert_eq!(it.parts(), 65_535, "EVT-162.5");
    let last = it.last().unwrap();
    assert_eq!(last.0.chunk_len, 65_488, "EVT-162.5");
    assert_eq!(PartIter::new(&big).unwrap_err(), EncodeErr::TooLong, "EVT-162.6");
    assert_eq!(record_len(16 + 65_488, true), 65_536, "EVT-162.7");
}

fn rec(kind: EventKind, seq: u64, tsc: u64, flags: RecordFlags, ph: &PartHdr, chunk: &[u8]) -> Vec<u8> {
    let mut out = vec![0u8; 65_536];
    let n = encode_part(&mut out, seq, tsc, flags, kind, ph, chunk).unwrap();
    out.truncate(n as usize);
    out
}

fn segments(logical: &[u8], seq0: u64) -> Vec<Vec<u8>> {
    PartIter::new(logical)
        .unwrap()
        .enumerate()
        .map(|(i, (ph, c, f))| rec(EventKind::CONFIG_BLOB, seq0 + i as u64, 5, f, &ph, c))
        .collect()
}

fn logical(n: usize, salt: u8) -> Vec<u8> {
    (0..n).map(|i| (i as u8) ^ salt).collect()
}

/// Pushes every record; returns the last result (or the first error).
fn push_all(r: &mut Reassembler<'_>, recs: &[Vec<u8>]) -> Result<Option<Vec<u8>>, DecodeErr> {
    let mut last = None;
    for x in recs {
        let v = decode_record(x).unwrap();
        last = r.push(&v)?.map(<[u8]>::to_vec);
    }
    Ok(last)
}

#[test]
fn evt_164_reassemble_errors() {
    let l = logical(150_000, 0x5A);
    let parts: Vec<_> = PartIter::new(&l).unwrap().collect();
    assert_eq!(parts.len(), 3);
    let base = |i: usize| parts[i];
    let mk = |kind: EventKind, seq: u64, tsc: u64, f: RecordFlags, ph: PartHdr, c: &[u8]| {
        rec(kind, seq, tsc, f, &ph, c)
    };
    let normal = |i: usize| {
        let (ph, c, f) = base(i);
        mk(EventKind::CONFIG_BLOB, 100 + i as u64, 5, f, ph, c)
    };
    let mut buf = vec![0u8; 256 * 1024];
    let mut run = |recs: Vec<Vec<u8>>| {
        let mut r = Reassembler::new(&mut buf);
        push_all(&mut r, &recs)
    };
    let all = |patch: &dyn Fn(usize) -> Option<Vec<u8>>| {
        (0..3)
            .map(|i| patch(i).unwrap_or_else(|| normal(i)))
            .collect::<Vec<_>>()
    };
    use DecodeErr::BadPart;
    assert_eq!(run(vec![normal(1)]), Err(BadPart), "EVT-164.1");
    let (p1, c1, f1) = base(1);
    assert_eq!(
        run(all(&|i| (i == 1).then(|| mk(
            EventKind::STATE_SNAPSHOT,
            101,
            5,
            f1,
            p1,
            c1
        )))),
        Err(BadPart),
        "EVT-164.2"
    );
    assert_eq!(
        run(all(&|i| (i == 1).then(|| mk(
            EventKind::CONFIG_BLOB,
            101,
            6,
            f1,
            p1,
            c1
        )))),
        Err(BadPart),
        "EVT-164.3"
    );
    assert_eq!(
        run(all(&|i| (i == 1).then(|| mk(
            EventKind::CONFIG_BLOB,
            102,
            5,
            f1,
            p1,
            c1
        )))),
        Err(BadPart),
        "EVT-164.4"
    );
    let (p2, c2, _) = base(2);
    assert_eq!(
        run(all(&|i| (i == 2).then(|| mk(
            EventKind::CONFIG_BLOB,
            102,
            5,
            RecordFlags::PART,
            p2,
            c2
        )))),
        Err(BadPart),
        "EVT-164.5"
    );
    let (p0, c0, _) = base(0);
    assert_eq!(
        run(all(&|i| (i == 0).then(|| mk(
            EventKind::CONFIG_BLOB,
            100,
            5,
            RecordFlags(0),
            p0,
            c0
        )))),
        Err(BadPart),
        "EVT-164.6"
    );
    assert_eq!(
        run(all(&|i| (i == 1).then(|| mk(
            EventKind::CONFIG_BLOB,
            101,
            5,
            f1,
            PartHdr { parts: 4, ..p1 },
            c1
        )))),
        Err(BadPart),
        "EVT-164.7"
    );
    assert_eq!(
        run(all(&|i| (i == 1).then(|| mk(
            EventKind::CONFIG_BLOB,
            101,
            5,
            f1,
            PartHdr {
                total_len: 150_001,
                ..p1
            },
            c1
        )))),
        Err(BadPart),
        "EVT-164.8"
    );
    let short = &c2[..c2.len() - 8];
    assert_eq!(
        run(all(&|i| {
            (i == 2).then(|| {
                mk(
                    EventKind::CONFIG_BLOB,
                    102,
                    5,
                    RecordFlags(0),
                    PartHdr {
                        chunk_len: p2.chunk_len - 8,
                        ..p2
                    },
                    short,
                )
            })
        })),
        Err(BadPart),
        "EVT-164.9"
    );
    let mut over = normal(0);
    let cl = p0.chunk_len + 8;
    over[24 + 4..24 + 8].copy_from_slice(&cl.to_le_bytes());
    assert_eq!(run(vec![over]), Err(BadPart), "EVT-164.10");
    let l3 = logical(300_000, 1);
    let big: Vec<_> = segments(&l3, 100);
    let mut small = vec![0u8; 256 * 1024];
    let mut r = Reassembler::new(&mut small);
    assert_eq!(
        r.push(&decode_record(&big[0]).unwrap()).map(|_| ()),
        Err(DecodeErr::TooLong),
        "EVT-164.11"
    );
    let one = logical(100, 2);
    let s = segments(&one, 7);
    assert_eq!(run(s), Ok(Some(one)), "EVT-164.12");
}

#[test]
fn evt_165_reset_and_recover() {
    let a = logical(150_000, 0x11);
    let sa = segments(&a, 100);
    let b = logical(80_000, 0x22);
    let sb = segments(&b, 200);
    let mut buf = vec![0u8; 256 * 1024];
    {
        let mut r = Reassembler::new(&mut buf);
        assert_eq!(r.push(&decode_record(&sa[0]).unwrap()), Ok(None));
        let parts: Vec<_> = PartIter::new(&a).unwrap().collect();
        let (p2, c2, _) = parts[2];
        let bad = rec(EventKind::CONFIG_BLOB, 102, 5, RecordFlags::PART, &p2, c2);
        assert_eq!(r.push(&decode_record(&bad).unwrap()), Err(DecodeErr::BadPart));
        assert_eq!(push_all(&mut r, &sb), Ok(Some(b.clone())), "step 1");
    }
    let c = logical(64, 0x33);
    let sc = segments(&c, 300);
    let mut r = Reassembler::new(&mut buf);
    assert_eq!(r.push(&decode_record(&sa[0]).unwrap()), Ok(None));
    r.reset();
    assert_eq!(push_all(&mut r, &sc), Ok(Some(c)), "step 2");
}
