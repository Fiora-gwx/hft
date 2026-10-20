//! ControlCmd encoding (EVT-151, EVT-152, EVT-154 – EVT-158).

#![allow(
    clippy::disallowed_types,
    clippy::disallowed_macros,
    clippy::disallowed_methods
)] // allow-list: test code, not a hot path
#![allow(clippy::type_complexity)]

#[path = "common/cmds.rs"]
mod cmds;

use cmds::*;
use hft_event::*;
use hft_types::*;
use zerocopy::IntoBytes;

fn bare(cmd: u16, ver: u16, payload: &[u8]) -> Vec<u8> {
    let mut h = hdr();
    h.cmd = CmdKind(cmd);
    h.cmd_ver = ver;
    h.payload_len = payload.len() as u32;
    let mut b = h.as_bytes().to_vec();
    b.extend_from_slice(payload);
    b
}

fn set_plen(b: &mut [u8], n: u32) {
    b[76..80].copy_from_slice(&n.to_le_bytes());
}

#[test]
fn evt_151_cmd_kind_numbers() {
    let names = [
        (0x0001, "SetMode", "EVT-151.1"),
        (0x0002, "SetParams", "EVT-151.1"),
        (0x0003, "SetRisk", "EVT-151.1"),
        (0x0004, "Kill", "EVT-151.1"),
        (0x0006, "RotateConn", "EVT-151.2"),
        (0x0007, "Resubscribe", "EVT-151.2"),
        (0x0008, "SnapshotNow", "EVT-151.2"),
        (0x0009, "ReplaceRefdata", "EVT-151.2"),
        (0x000A, "Resume", "EVT-151.2"),
        (0x000E, "HedgeFix", "EVT-151.3"),
        (0x000F, "CancelAll", "EVT-151.3"),
        (0x0010, "Drain", "EVT-151.3"),
        (0x0011, "RecordTransfer", "EVT-151.3"),
        (0x0012, "Annotate", "EVT-151.3"),
        (0x0013, "AdoptExchange", "EVT-151.3"),
        (0x0014, "SimReset", "EVT-151.4"),
        (0x0015, "SimTransfer", "EVT-151.4"),
        (0x0016, "VenueNotice", "EVT-151.4"),
        (0x0017, "SetBalanceBaseline", "EVT-151.4"),
        (0x0018, "StrategyCmd", "EVT-151.4"),
        (0x0019, "InjectWarmup", "EVT-151.4"),
        (0x001A, "SmokeOrder", "EVT-151.4"),
        (0x001B, "WriteOffResidual", "EVT-151.4"),
        (0x001C, "SimFault", "EVT-151.9"),
        (0x001D, "BindSlot", "EVT-151.10"),
    ];
    let w = [1u8; 10];
    let samples = all_samples(&w);
    assert_eq!(samples.len(), names.len());
    for ((n, name, row), c) in names.iter().zip(&samples) {
        assert_eq!(CmdKind(*n).name(), Some(*name), "{row}");
        assert_eq!(c.kind(), CmdKind(*n), "{row} {name}");
    }
    for (row, n) in [
        ("EVT-151.5", 0x0005u16),
        ("EVT-151.6", 0x000B),
        ("EVT-151.6", 0x000C),
        ("EVT-151.6", 0x000D),
        ("EVT-151.7", 0x0000),
        ("EVT-151.8", 0x001E),
    ] {
        assert_eq!(CmdKind(n).name(), None, "{row}");
        assert_eq!(
            ControlCmd::decode(&bare(n, 1, &[])).unwrap_err(),
            DecodeErr::UnknownKind,
            "{row}"
        );
    }
}

/// Payload bytes of an encoded command.
fn payload_of(c: &ControlCmd<'_>) -> (u32, Vec<u8>) {
    let b = encode(c);
    let plen = u32::from_le_bytes(b[76..80].try_into().unwrap());
    (plen, b[128..].to_vec())
}

fn at(p: &[u8], o: usize, v: &[u8]) -> bool {
    &p[o..o + v.len()] == v
}

#[test]
fn evt_152_payload_sizes() {
    let w = [9u8; 10];
    let s = all_samples(&w);
    let sc = scope();
    let br = blob_ref();
    let checks: Vec<(&str, usize, u32, Vec<(usize, Vec<u8>)>)> = vec![
        (
            "EVT-152.1",
            0,
            24,
            vec![(0, sc.as_bytes().to_vec()), (16, vec![RunMode::CloseOnly as u8])],
        ),
        (
            "EVT-152.2",
            1,
            64,
            vec![(0, sc.as_bytes().to_vec()), (16, br.as_bytes().to_vec())],
        ),
        ("EVT-152.3", 2, 48, vec![(0, br.as_bytes().to_vec())]),
        ("EVT-152.3", 7, 48, vec![(0, br.as_bytes().to_vec())]),
        (
            "EVT-152.4",
            3,
            24,
            vec![(0, sc.as_bytes().to_vec()), (16, vec![3, 1, 1, 1])],
        ),
        ("EVT-152.5", 4, 8, vec![(0, vec![5, 2])]),
        (
            "EVT-152.6",
            5,
            8,
            vec![(0, vec![5]), (4, 77u32.to_le_bytes().to_vec())],
        ),
        ("EVT-152.7", 6, 0, vec![]),
        (
            "EVT-152.8",
            8,
            24,
            vec![(0, sc.as_bytes().to_vec()), (16, vec![3])],
        ),
        ("EVT-152.9", 9, 16, vec![(0, sc.as_bytes().to_vec())]),
        ("EVT-152.9", 10, 16, vec![(0, sc.as_bytes().to_vec())]),
        ("EVT-152.10", 11, 8, vec![(0, vec![1])]),
        (
            "EVT-152.11",
            12,
            80,
            vec![
                (0, vec![2, 1, 2]),
                (4, 9u16.to_le_bytes().to_vec()),
                (8, 123i64.to_le_bytes().to_vec()),
                (16, vec![0x41; 64]),
            ],
        ),
        (
            "EVT-152.12",
            13,
            296,
            vec![
                (0, 3u16.to_le_bytes().to_vec()),
                (2, 5u16.to_le_bytes().to_vec()),
                (4, vec![9; 32]),
                (36, vec![0x61; 256]),
            ],
        ),
        (
            "EVT-152.13",
            14,
            32,
            vec![
                (0, vec![2, 5]),
                (4, 77u32.to_le_bytes().to_vec()),
                (8, 88u64.to_le_bytes().to_vec()),
                (16, vec![1]),
                (18, 4u16.to_le_bytes().to_vec()),
                (20, 0x0300_0001u32.to_le_bytes().to_vec()),
                (24, 1234i64.to_le_bytes().to_vec()),
            ],
        ),
        ("EVT-152.14", 15, 8, vec![(0, vec![2])]),
        (
            "EVT-152.15",
            16,
            16,
            vec![
                (0, vec![2, 1, 2]),
                (4, 9u16.to_le_bytes().to_vec()),
                (8, 5i64.to_le_bytes().to_vec()),
            ],
        ),
        (
            "EVT-152.16",
            17,
            24,
            vec![
                (0, vec![5, 2]),
                (4, 77u32.to_le_bytes().to_vec()),
                (8, 28_800i64.to_le_bytes().to_vec()),
                (16, (-1i64).to_le_bytes().to_vec()),
            ],
        ),
        ("EVT-152.17", 18, 8, vec![(0, vec![2, 5])]),
        (
            "EVT-152.18",
            19,
            264,
            vec![
                (0, vec![3, 1]),
                (2, 3u16.to_le_bytes().to_vec()),
                (4, 0x0300_0002u32.to_le_bytes().to_vec()),
                (8, vec![1, 2, 3]),
            ],
        ),
        (
            "EVT-152.19",
            20,
            32,
            vec![
                (0, vec![3, 1]),
                (2, 4u16.to_le_bytes().to_vec()),
                (4, 6u32.to_le_bytes().to_vec()),
                (8, 10u16.to_le_bytes().to_vec()),
                (16, vec![9; 10]),
                (26, vec![0; 6]),
            ],
        ),
        (
            "EVT-152.20",
            21,
            16,
            vec![
                (0, vec![2, 5]),
                (4, 77u32.to_le_bytes().to_vec()),
                (8, vec![2]),
                (12, 5000u32.to_le_bytes().to_vec()),
            ],
        ),
        (
            "EVT-152.21",
            22,
            8,
            vec![(0, vec![2]), (4, 0x0300_0001u32.to_le_bytes().to_vec())],
        ),
        (
            "EVT-152.22",
            23,
            8,
            vec![(0, vec![2, 5, 4]), (4, 3000u32.to_le_bytes().to_vec())],
        ),
    ];
    for (row, i, size, fields) in checks {
        let (plen, p) = payload_of(&s[i]);
        assert_eq!(plen, size, "{row}");
        assert_eq!(p.len(), size as usize, "{row}");
        for (o, v) in fields {
            assert!(at(&p, o, &v), "{row} offset {o}");
        }
    }
}

fn dec(b: &[u8]) -> Result<(), DecodeErr> {
    ControlCmd::decode(b).map(|_| ())
}

fn patched(c: &ControlCmd<'_>, f: impl Fn(&mut Vec<u8>)) -> Result<(), DecodeErr> {
    let mut b = encode(c);
    f(&mut b);
    dec(&b)
}

#[test]
fn evt_154_decode_errors() {
    use DecodeErr::*;
    let w = [7u8; 10];
    let s = all_samples(&w);
    let (set_mode, set_params, kill, resume, drain, smoke) = (&s[0], &s[1], &s[3], &s[8], &s[11], &s[21]);
    let sc = strategy_cmd(3);
    let wu = warmup(&w);
    let big = [1u8; 4096];
    let wbig = warmup(&big);
    let rows: Vec<(&str, Result<(), DecodeErr>, Result<(), DecodeErr>)> = vec![
        ("EVT-154.1", dec(&encode(set_mode)[..127]), Err(Short)),
        ("EVT-154.2", patched(set_mode, |b| b[70] = 2), Err(UnknownKind)),
        ("EVT-154.3", patched(kill, |b| b[70] = 3), Err(UnknownKind)),
        ("EVT-154.4", patched(set_mode, |b| b.extend([0; 8])), Err(TooLong)),
        (
            "EVT-154.5",
            patched(set_mode, |b| {
                b.truncate(144);
                set_plen(b, 16);
            }),
            Err(Short),
        ),
        (
            "EVT-154.6",
            patched(set_mode, |b| {
                b.extend([0; 8]);
                set_plen(b, 32);
            }),
            Err(TooLong),
        ),
        ("EVT-154.7", patched(set_mode, |b| b[144] = 2), Err(InvalidEnum)),
        ("EVT-154.7", patched(set_mode, |b| b[144] = 0), Err(InvalidEnum)),
        ("EVT-154.7", patched(set_mode, |b| b[144] = 5), Err(InvalidEnum)),
        ("EVT-154.8", patched(kill, |b| b[144] = 1), Err(InvalidEnum)),
        ("EVT-154.8", patched(kill, |b| b[144] = 5), Err(InvalidEnum)),
        ("EVT-154.9", patched(kill, |b| b[145] = 2), Err(InvalidEnum)),
        ("EVT-154.10", patched(resume, |b| b[144] = 0), Err(InvalidEnum)),
        ("EVT-154.10", patched(resume, |b| b[144] = 16), Err(InvalidEnum)),
        ("EVT-154.11", patched(resume, |b| b[144] = 9), Ok(())),
        (
            "EVT-154.12",
            patched(&sc, |b| b[130..132].copy_from_slice(&257u16.to_le_bytes())),
            Err(TooLong),
        ),
        ("EVT-154.13", patched(&sc, |b| b[128 + 8 + 3] = 1), Err(TooLong)),
        ("EVT-154.14", dec(&encode(&sc)), Ok(())),
        ("EVT-154.15", patched(&sc, |b| b[129] = 2), Err(InvalidEnum)),
        (
            "EVT-154.16",
            patched(&wbig, |b| {
                b[136..138].copy_from_slice(&4097u16.to_le_bytes());
                b.extend([0; 8]);
                set_plen(b, 4120);
            }),
            Err(TooLong),
        ),
        ("EVT-154.17", dec(&encode(&wu)), Ok(())),
        (
            "EVT-154.18",
            patched(&wu, |b| {
                b.truncate(152);
                set_plen(b, 24);
            }),
            Err(Short),
        ),
        (
            "EVT-154.19",
            patched(&wu, |b| {
                b.extend([0; 8]);
                set_plen(b, 40);
            }),
            Err(TooLong),
        ),
        ("EVT-154.20", patched(&wu, |b| b[129] = 2), Err(InvalidEnum)),
        ("EVT-154.21", patched(smoke, |b| b[136] = 0), Err(InvalidEnum)),
        ("EVT-154.22", patched(drain, |b| b[128] = 2), Err(InvalidEnum)),
        (
            "EVT-154.23",
            patched(set_params, |b| b[144..146].copy_from_slice(&0u16.to_le_bytes())),
            Err(InvalidEnum),
        ),
        (
            "EVT-154.23",
            patched(set_params, |b| b[144..146].copy_from_slice(&7u16.to_le_bytes())),
            Err(InvalidEnum),
        ),
        (
            "EVT-154.24",
            patched(set_mode, |b| b[128 + 17] = 1),
            Err(InvalidEnum),
        ),
        (
            "EVT-154.24",
            patched(&wu, |b| b[128 + 16 + 10] = 1),
            Err(InvalidEnum),
        ),
    ];
    for (row, got, want) in rows {
        assert_eq!(got, want, "{row}");
    }
}

#[test]
fn evt_155_old_cmd_versions() {
    let mut p = scope().as_bytes().to_vec();
    p.extend([3, 1, 1, 0, 0, 0, 0, 0]);
    let k = |ver: u16, b19: u8| {
        let mut q = p.clone();
        q[19] = b19;
        let b = bare(4, ver, &q);
        ControlCmd::decode(&b).map(|(_, c)| format!("{c:?}"))
    };
    let want = |hard| {
        format!(
            "{:?}",
            ControlCmd::Kill {
                scope: scope(),
                level: KillLevel::Account,
                flatten: true,
                cancel_resting: true,
                hard,
            }
        )
    };
    assert_eq!(k(1, 0), Ok(want(false)), "EVT-155.1");
    assert_eq!(k(1, 1), Ok(want(false)), "EVT-155.2");
    assert_eq!(k(2, 1), Ok(want(true)), "EVT-155.3");
    let mut v1 = vec![2u8, 5, 0, 0];
    v1.extend(77u32.to_le_bytes());
    v1.extend(88u64.to_le_bytes());
    let b1 = bare(0x13, 1, &v1);
    let got = ControlCmd::decode(&b1).unwrap().1;
    assert_eq!(
        got,
        ControlCmd::AdoptExchange {
            account: AccountId(2),
            venue: VenueId(5),
            inst: InstId(77),
            report_id: 88,
            item: 0,
            _pad: 0,
            asset: AssetId(0),
            group: GROUP_NONE,
            entry_px: Px(0),
        },
        "EVT-155.4"
    );
    let w = [0u8; 1];
    let v2 = encode(&all_samples(&w)[14]);
    match ControlCmd::decode(&v2).unwrap().1 {
        ControlCmd::AdoptExchange { item, .. } => assert_eq!(item, 1, "EVT-155.5"),
        c => panic!("EVT-155.5 {c:?}"),
    }
    let mut v1_32 = v1.clone();
    v1_32.extend([0u8; 16]);
    assert_eq!(dec(&bare(0x13, 1, &v1_32)), Err(DecodeErr::TooLong), "EVT-155.6");
}

#[test]
fn evt_156_encode_fills_header() {
    let w = [1u8; 10];
    let s = all_samples(&w);
    let h = hdr();
    for (c, cmd, ver, plen) in [
        (&s[3], 4u16, 2u16, 24u32),
        (&s[14], 0x13, 2, 32),
        (&s[0], 1, 1, 24),
        (&s[20], 0x19, 1, 32),
    ] {
        let b = encode(c);
        let (got, _) = ControlCmd::decode(&b).unwrap();
        assert_eq!((got.cmd, got.cmd_ver, got.payload_len), (CmdKind(cmd), ver, plen));
        let mut expect = h;
        expect.cmd = CmdKind(cmd);
        expect.cmd_ver = ver;
        expect.payload_len = plen;
        assert_eq!(got, expect, "other envelope fields unchanged");
    }
}

#[test]
fn evt_157_encode_errors() {
    let w = [0u8; 1];
    let set_mode = &all_samples(&w)[0];
    let mut out = vec![0u8; 8192];
    assert_eq!(
        set_mode.encode(&hdr(), &mut out[..151]),
        Err(EncodeErr::BufTooSmall),
        "EVT-157.1"
    );
    assert_eq!(set_mode.encode(&hdr(), &mut out[..152]), Ok(152), "EVT-157.2");
    assert_eq!(
        strategy_cmd(257).encode(&hdr(), &mut out),
        Err(EncodeErr::TooLong),
        "EVT-157.3"
    );
    let big = [1u8; 4097];
    assert_eq!(
        warmup(&big).encode(&hdr(), &mut out),
        Err(EncodeErr::TooLong),
        "EVT-157.4"
    );
    let ten = [1u8; 10];
    let c = ControlCmd::InjectWarmup {
        instance: StrategyId(3),
        last: 0,
        local_kind: 1,
        chunk_no: 0,
        len: 11,
        _pad: [0; 6],
        payload: &ten,
    };
    assert_eq!(c.encode(&hdr(), &mut out), Err(EncodeErr::TooLong), "EVT-157.5");
}

#[test]
fn evt_158_max_warmup_single_part() {
    let big = [3u8; 4096];
    let logical = encode(&warmup(&big));
    assert_eq!(logical.len(), 4240);
    let parts: Vec<_> = PartIter::new(&logical).unwrap().collect();
    assert_eq!(parts.len(), 1);
    let (ph, chunk, flags) = parts[0];
    assert_eq!((ph.parts, flags.contains(RecordFlags::PART)), (1, false));
    assert_eq!(record_len(16 + chunk.len(), true), 4288);
}
