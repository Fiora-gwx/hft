//! Transposition of record bodies into Arrow columns against hft-event's typed decoding
//! (TESTS.md PY-041).

use std::sync::Arc;

use hft_event::{FieldDesc, FieldTy, IdKind, KindInfo, Layout};
use hft_py::arrow_map::{Batch, Builder, ColSrc, Column, Exps, KindMap, Row, kind_map, plan};
use proptest::prelude::*;
use zerocopy::TryFromBytes;

/// Deterministic bytes for every leaf; enum fields get a listed value, padding stays 0.
fn fill(fields: &[FieldDesc], base: usize, b: &mut [u8], rnd: &mut impl FnMut() -> u64) {
    for f in fields {
        let off = base + usize::from(f.offset);
        let w = usize::from(f.ty.width());
        match f.ty {
            FieldTy::Pad(_) => {}
            FieldTy::Nested(inner) => fill(inner, off, b, rnd),
            FieldTy::Enum(d) => {
                let v = d
                    .items
                    .get((rnd() as usize) % d.items.len().max(1))
                    .map_or(0, |x| x.value);
                b[off..off + w].copy_from_slice(&u64::from(v).to_le_bytes()[..w]);
            }
            FieldTy::Id(IdKind::Kind) => b[off..off + 2].copy_from_slice(&0x0101u16.to_le_bytes()),
            _ => {
                for x in &mut b[off..off + w] {
                    *x = rnd() as u8;
                }
            }
        }
    }
}

/// Whether hft-event's typed decoding accepts `prefix` as the kind's body/prefix struct.
fn typed_ok(info: &KindInfo, prefix: &[u8]) -> bool {
    use hft_event::*;
    macro_rules! t {
        ($($k:ident => $ty:ty),* $(,)?) => {
            match info.kind {
                $(EventKind::$k => <$ty>::try_read_from_bytes(prefix).is_ok(),)*
                _ => true,
            }
        };
    }
    t!(
        BBO => Bbo, L2_DELTA => L2Delta, L2_SNAPSHOT => L2SnapHdr, TRADE => Trade, FUNDING => Funding,
        MARK_PRICE => MarkPrice, INDEX_PRICE => IndexPrice, LIQUIDATION => Liquidation, L2_BATCH => L2BatchHdr,
        ORDER_DECISION => OrderDecision, SEND_REPORT => SendReport, ORDER_EVENT => OrderEvent,
        POSITION_REPORT => PositionReport, BALANCE_REPORT => BalanceReport, FUNDING_SETTLE => FundingSettle,
        EXTERNAL_FILL => ExternalFill, MARGIN_REPORT => MarginReportHdr, ACCOUNT_CONFIG_REPORT => AccountConfigReportHdr,
        EXEC_DECISION => ExecDecision, CHAIN_HEAD => ChainHead, POOL_QUOTE => PoolQuote, SWAP_DECISION => SwapDecision,
        TX_LIFECYCLE => TxLifecycle, CONTROL_CMD => CmdHdr, CMD_RESULT => CmdResult, CONFIG_BLOB => BlobHdr,
        RECONCILE_REPORT => ReconcileReport, TIMER => Timer, CONN_STATE => ConnState, ADVICE => AdviceHdr,
        STATE_SNAPSHOT => SnapshotHdr, LATENCY_SAMPLE => LatencySample, NOTE => Note, HOST_PROFILE => HostProfile,
        CLOCK_STATUS => ClockStatus, SEGMENT_ROLL => SegmentRoll, ENGINE_CLOCK => EngineClock, UTC_ANCHOR => UtcAnchor,
        RAW_RX => RawHdr,
        RAW_TX => RawHdr,
    )
}

fn le(b: &[u8], off: usize, w: usize, signed: bool) -> i128 {
    let mut x = [0u8; 8];
    x[..w].copy_from_slice(&b[off..off + w]);
    let u = u64::from_le_bytes(x);
    if signed {
        let sh = 64 - 8 * w as u32;
        i128::from(((u << sh) as i64) >> sh)
    } else {
        i128::from(u)
    }
}

fn check(map: &KindMap, bodies: &[Vec<u8>], b: &Batch) {
    assert_eq!(b.rows, bodies.len());
    let cols = hft_py::arrow_map::all_columns(map, false, false);
    for (ci, (name, src)) in cols.iter().enumerate() {
        let a = &b.cols[ci];
        for (row, body) in bodies.iter().enumerate() {
            match src {
                ColSrc::Seq => assert_eq!(a.int(row), Some(row as i128 + 1), "{name}"),
                ColSrc::Leaf(i) => {
                    let l = &map.leaves[*i];
                    match l.prim {
                        Some(_) => {
                            let signed = matches!(
                                a,
                                Column::I8(_) | Column::I16(_) | Column::I32(_) | Column::I64(_)
                            );
                            assert_eq!(
                                a.int(row),
                                Some(le(body, l.off, l.width, signed)),
                                "{} {name} row {row}",
                                map.info.name
                            );
                        }
                        None => {
                            let Column::Fixed(w, v) = a else {
                                panic!("{name}: not fixed-size binary")
                            };
                            assert_eq!(
                                &v[row * w..(row + 1) * w],
                                &body[l.off..l.off + l.width],
                                "{name}"
                            );
                        }
                    }
                }
                ColSrc::Body => {
                    let Column::Bin(o, d) = a else {
                        panic!("{name}: not binary")
                    };
                    assert_eq!(
                        &d[o[row] as usize..o[row + 1] as usize],
                        body.as_slice(),
                        "{name}"
                    );
                }
                _ => {}
            }
        }
    }
}

fn kinds() -> Vec<&'static KindInfo> {
    hft_event::KINDS
        .iter()
        .filter(|k| k.kind.0 >> 8 != 0x7F)
        .collect()
}

proptest! {
    #![proptest_config(ProptestConfig {
        cases: std::env::var("PROPTEST_CASES").ok().and_then(|v| v.parse().ok()).unwrap_or(2000),
        ..ProptestConfig::default()
    })]
    #[test]
    fn py_041_transpose_matches_decode(ki in 0usize..64, n in 1usize..=2048, tail in 0usize..=64, seed in any::<u64>()) {
        let ks = kinds();
        let info = ks[ki % ks.len()];
        let map = Arc::new(kind_map(info));
        let mut st = seed | 1;
        let mut rnd = || { st ^= st << 13; st ^= st >> 7; st ^= st << 17; st };
        let mut bodies = Vec::with_capacity(n);
        for _ in 0..n {
            let mut b = vec![0u8; map.prefix];
            fill(info.fields, 0, &mut b, &mut rnd);
            if let Some(e) = &map.entries {
                // Count fields → `tail` entries in the first count field, 0 in the others.
                for (j, (o, w)) in e.counts.iter().enumerate() {
                    let v = if j == 0 { tail as u64 } else { 0 };
                    b[*o..*o + *w].copy_from_slice(&v.to_le_bytes()[..*w]);
                }
                // Level, MarginItem and LeverageItem have no enum fields: any bytes decode.
                b.extend((0..tail * e.size).map(|_| rnd() as u8));
            } else if !matches!(info.layout, Layout::Fixed { .. }) {
                b.extend((0..tail * 8).map(|_| rnd() as u8));
            }
            prop_assert!(typed_ok(info, &b[..map.prefix]), "{} body not decodable", info.name);
            bodies.push(b);
        }
        let p = plan(map.clone(), false, false, None, Arc::new(Exps::default())).unwrap();
        let mut bld = Builder::new(p, n);
        for (i, body) in bodies.iter().enumerate() {
            bld.push(&Row { seq: i as u64 + 1, tsc: 0, proc_ns: 0, flags: 0, body, prefix: map.prefix });
        }
        let batch = bld.finish().unwrap();
        check(&map, &bodies, &batch);
    }
}
