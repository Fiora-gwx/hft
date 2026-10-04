//! Codec round trips (EVT-011, EVT-025).

use hft_event::*;
use proptest::prelude::*;
use zerocopy::IntoBytes;

/// Makes random bytes a legal body: pads 0, enum fields a legal code (chosen by the random byte).
fn sanitize(out: &mut [u8], base: usize, fields: &[FieldDesc]) {
    for f in fields {
        let o = base + f.offset as usize;
        let w = f.ty.width() as usize;
        match f.ty {
            FieldTy::Pad(_) => out[o..o + w].fill(0),
            FieldTy::Nested(inner) => sanitize(out, o, inner),
            FieldTy::Enum(d) => {
                let pick = d.items[out[o] as usize % d.items.len()].value as u64;
                out[o..o + w].copy_from_slice(&pick.to_le_bytes()[..w]);
            }
            FieldTy::Bits(d) => {
                let mask = d.items.iter().fold(0u64, |m, i| m | u64::from(i.value));
                let mut v = [0u8; 8];
                v[..w].copy_from_slice(&out[o..o + w]);
                let x = u64::from_le_bytes(v) & mask;
                out[o..o + w].copy_from_slice(&x.to_le_bytes()[..w]);
            }
            _ => {}
        }
    }
}

fn legal(fields: &[FieldDesc], raw: &[u8]) -> Vec<u8> {
    let size = fields.last().map_or(0, |l| (l.offset + l.ty.width()) as usize);
    let mut b = raw[..size].to_vec();
    sanitize(&mut b, 0, fields);
    b
}

fn flags_from(bits: u8) -> RecordFlags {
    // Subsets of {CONFLATED, SHADOW, HW_TS, SW_TS, CRC, EVENT_END} (PART excluded).
    RecordFlags(u16::from(bits) & 0x006F)
}

macro_rules! roundtrip_fixed {
    ($ix:expr, $raw:expr, $seq:expr, $tsc:expr, $flags:expr; $($t:ty),* $(,)?) => {{
        let types: &[&str] = &[$(stringify!($t)),*];
        let ix = $ix % types.len();
        let mut k = 0usize;
        $(
            if ix == k {
                let bytes = legal(<$t>::FIELDS, $raw);
                let b: $t = zerocopy::TryFromBytes::try_read_from_bytes(&bytes[..]).unwrap();
                let mut out = vec![0u64; 40];
                let out: &mut [u8] = out.as_mut_bytes();
                let crc = $flags.contains(RecordFlags::CRC);
                let n = encode_fixed(out, $seq, $tsc, $flags, &b) as usize;
                prop_assert_eq!(n as u32, record_len(size_of::<$t>(), crc));
                let r = decode_record(&out[..n]).unwrap();
                prop_assert_eq!(r.read::<$t>().unwrap(), b);
                prop_assert_eq!(*r.view::<$t>().unwrap(), b);
                prop_assert_eq!(r.hdr.kind, <$t as Body>::KIND.0);
                prop_assert_eq!((r.hdr.seq, r.hdr.tsc, r.hdr.flags), ($seq, $tsc, $flags.0));
                if crc {
                    let c = crc32c::crc32c(&out[..n - 4]);
                    prop_assert_eq!(&out[n - 4..n], &c.to_le_bytes()[..]);
                }
            }
            k += 1;
        )*
        let _ = k;
    }};
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(256 * 29))]
    #[test]
    fn evt_011_fixed_roundtrip(
        ix in 0usize..29,
        raw in prop::collection::vec(any::<u8>(), 176),
        seq in any::<u64>(),
        tsc in any::<u64>(),
        fb in any::<u8>(),
    ) {
        let flags = flags_from(fb);
        roundtrip_fixed!(ix, &raw, seq, tsc, flags;
            Bbo, L2Delta, Trade, Funding, MarkPrice, IndexPrice, Liquidation, OrderDecision, SendReport, OrderEvent,
            PositionReport, BalanceReport, FundingSettle, ExternalFill, ExecDecision, ChainHead, PoolQuote,
            SwapDecision, TxLifecycle, CmdResult, ReconcileReport, Timer, ConnState, LatencySample, Note, HostProfile,
            ClockStatus, SegmentRoll, EngineClock);
    }
}

fn check_var<P: Body + PartialEq + core::fmt::Debug>(
    p: P,
    tail: &[u8],
    crc: bool,
) -> Result<(), TestCaseError> {
    let flags = if crc { RecordFlags::CRC } else { RecordFlags(0) };
    let mut store = vec![0u64; 9000];
    let out: &mut [u8] = store.as_mut_bytes();
    let n = encode_var(out, 9, 10, flags, &p, tail).unwrap() as usize;
    prop_assert_eq!(n as u32, record_len(size_of::<P>() + tail.len(), crc));
    let body_end = size_of::<P>() + tail.len() + 24;
    let pad_end = if crc { n - 4 } else { n };
    prop_assert!(out[body_end..pad_end].iter().all(|&b| b == 0));
    let r = decode_record(&out[..n]).unwrap();
    let (pp, t) = r.var::<P>().unwrap();
    prop_assert_eq!(pp, &p);
    prop_assert_eq!(t, tail);
    Ok(())
}

fn prefix<P: Body>(raw: &[u8], fields: &[FieldDesc]) -> P {
    P::try_read_from_bytes(&legal(fields, raw)[..]).unwrap()
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(256 * 4))]
    #[test]
    fn evt_025_var_roundtrip(
        which in 0usize..4,
        raw in prop::collection::vec(any::<u8>(), 80),
        count in 0usize..=4096,
        seed in any::<u8>(),
        crc in any::<bool>(),
    ) {
        let bytes = |n: usize| (0..n).map(|i| (i as u8).wrapping_mul(31) ^ seed).collect::<Vec<u8>>();
        match which {
            0 => {
                let mut h: AdviceHdr = prefix(&raw, AdviceHdr::FIELDS);
                h.len = count as u16;
                check_var(h, &bytes(count), crc)?;
            }
            1 => {
                let n = count % 1025;
                let mut h: MarginReportHdr = prefix(&raw, MarginReportHdr::FIELDS);
                h.n = n as u16;
                check_var(h, &bytes(16 * n), crc)?;
            }
            2 => {
                let n = count % 1025;
                let mut h: AccountConfigReportHdr = prefix(&raw, AccountConfigReportHdr::FIELDS);
                h.n_lev = n as u16;
                check_var(h, &bytes(16 * n), crc)?;
            }
            _ => {
                let n = count % 4001;
                let mut h: L2BatchHdr = prefix(&raw, L2BatchHdr::FIELDS);
                h.n_bids = (n / 3) as u16;
                h.n_asks = (n - n / 3) as u16;
                check_var(h, &bytes(16 * n), crc)?;
            }
        }
    }
}
