//! ControlCmd round trip (EVT-153).
#![allow(clippy::large_enum_variant)]

use hft_event::*;
use hft_types::*;
use proptest::prelude::*;
use zerocopy::FromBytes;

fn arb_scope() -> impl Strategy<Value = CmdScope> {
    (
        any::<[u8; 3]>(),
        prop_oneof![
            Just(GROUP_ALL),
            (1u8..=254, 1u32..=GroupId::N_MAX).prop_map(|(s, n)| GroupId::compose(StrategyId(s), n).unwrap())
        ],
        any::<u32>(),
    )
        .prop_map(|(b, group, inst)| CmdScope {
            strategy: StrategyId(b[0]),
            venue: VenueId(b[1]),
            account: AccountId(b[2]),
            _pad: 0,
            group,
            inst: InstId(inst),
            _pad2: 0,
        })
}

fn arb_blob() -> impl Strategy<Value = BlobRef> {
    (
        prop::sample::select(BlobKind::ALL.to_vec()),
        any::<u16>(),
        any::<u32>(),
        any::<u64>(),
        any::<[u8; 32]>(),
    )
        .prop_map(|(kind, encoding, len, version, sha256)| BlobRef {
            kind,
            encoding,
            len,
            version,
            sha256,
        })
}

#[derive(Debug, Clone)]
enum Gen {
    Cmd(ControlCmd<'static>),
    Warmup {
        instance: u8,
        last: u8,
        local_kind: u16,
        chunk_no: u32,
        payload: Vec<u8>,
    },
}

fn arb_cmd() -> impl Strategy<Value = Gen> {
    let c = |s: BoxedStrategy<ControlCmd<'static>>| s.prop_map(Gen::Cmd);
    let b = any::<bool>;
    prop_oneof![
        c((
            arb_scope(),
            prop::sample::select(vec![RunMode::Shadow, RunMode::CloseOnly, RunMode::Live])
        )
            .prop_map(|(scope, mode)| ControlCmd::SetMode { scope, mode })
            .boxed()),
        c((arb_scope(), arb_blob())
            .prop_map(|(scope, blob)| ControlCmd::SetParams { scope, blob })
            .boxed()),
        c(arb_blob().prop_map(|blob| ControlCmd::SetRisk { blob }).boxed()),
        c((
            arb_scope(),
            prop::sample::select(vec![KillLevel::Venue, KillLevel::Account, KillLevel::Global]),
            b(),
            b(),
            b()
        )
            .prop_map(|(scope, level, flatten, cancel_resting, hard)| ControlCmd::Kill {
                scope,
                level,
                flatten,
                cancel_resting,
                hard
            })
            .boxed()),
        c(any::<[u8; 2]>()
            .prop_map(|x| ControlCmd::RotateConn {
                venue: VenueId(x[0]),
                account: AccountId(x[1])
            })
            .boxed()),
        c((any::<u8>(), any::<u32>())
            .prop_map(|(v, i)| ControlCmd::Resubscribe {
                venue: VenueId(v),
                inst: InstId(i)
            })
            .boxed()),
        c(Just(ControlCmd::SnapshotNow).boxed()),
        c(arb_blob()
            .prop_map(|blob| ControlCmd::ReplaceRefdata { blob })
            .boxed()),
        c((arb_scope(), 1u8..=15)
            .prop_map(|(scope, s)| ControlCmd::Resume {
                scope,
                source: CapSource(s)
            })
            .boxed()),
        c(arb_scope()
            .prop_map(|scope| ControlCmd::HedgeFix { scope })
            .boxed()),
        c(arb_scope()
            .prop_map(|scope| ControlCmd::CancelAll { scope })
            .boxed()),
        c(b()
            .prop_map(|cancel_resting| ControlCmd::Drain { cancel_resting })
            .boxed()),
        c((
            any::<[u8; 3]>(),
            any::<u16>(),
            any::<i64>(),
            prop::array::uniform32(any::<u8>())
        )
            .prop_map(|(x, asset, amount, r)| {
                let mut reference = [0u8; 64];
                reference[..32].copy_from_slice(&r);
                ControlCmd::RecordTransfer {
                    account: AccountId(x[0]),
                    from: VenueId(x[1]),
                    to: VenueId(x[2]),
                    asset: AssetId(asset),
                    amount: Qty(amount),
                    reference,
                }
            })
            .boxed()),
        c((1u16..=3, 0u16..=256, any::<[u8; 32]>(), any::<u8>())
            .prop_map(|(kind, text_len, digest, t)| ControlCmd::Annotate {
                kind,
                text_len,
                digest,
                text: [t; 256]
            })
            .boxed()),
        c((
            any::<[u8; 2]>(),
            any::<u32>(),
            any::<u64>(),
            0u8..=1,
            any::<u8>(),
            any::<u16>(),
            any::<u32>(),
            any::<i64>()
        )
            .prop_map(
                |(x, inst, report_id, item, pad, asset, group, px)| ControlCmd::AdoptExchange {
                    account: AccountId(x[0]),
                    venue: VenueId(x[1]),
                    inst: InstId(inst),
                    report_id,
                    item,
                    _pad: pad,
                    asset: AssetId(asset),
                    group: GroupId(group),
                    entry_px: Px(px)
                }
            )
            .boxed()),
        c(any::<u8>()
            .prop_map(|a| ControlCmd::SimReset {
                account: AccountId(a)
            })
            .boxed()),
        c((any::<[u8; 3]>(), any::<u16>(), any::<i64>())
            .prop_map(|(x, asset, amount)| ControlCmd::SimTransfer {
                account: AccountId(x[0]),
                from: VenueId(x[1]),
                to: VenueId(x[2]),
                asset: AssetId(asset),
                amount: Qty(amount)
            })
            .boxed()),
        c((
            any::<u8>(),
            1u8..=3,
            any::<u16>(),
            any::<u32>(),
            any::<i64>(),
            any::<i64>()
        )
            .prop_map(|(v, kind, p, inst, a, bb)| ControlCmd::VenueNotice {
                venue: VenueId(v),
                kind,
                _pad: p,
                inst: InstId(inst),
                a,
                b: bb
            })
            .boxed()),
        c(any::<[u8; 2]>()
            .prop_map(|x| ControlCmd::SetBalanceBaseline {
                account: AccountId(x[0]),
                venue: VenueId(x[1])
            })
            .boxed()),
        c((any::<u8>(), 0u8..=1, 0u16..=256, any::<u32>(), any::<u8>())
            .prop_map(|(i, ro, len, group, fillb)| {
                let mut payload = [0u8; 256];
                payload[..usize::from(len)].fill(fillb);
                ControlCmd::StrategyCmd {
                    instance: StrategyId(i),
                    reduce_only: ro,
                    len,
                    group: GroupId(group),
                    payload,
                }
            })
            .boxed()),
        (
            any::<u8>(),
            0u8..=1,
            any::<u16>(),
            any::<u32>(),
            prop::collection::vec(any::<u8>(), 0..=4096)
        )
            .prop_map(|(instance, last, local_kind, chunk_no, payload)| Gen::Warmup {
                instance,
                last,
                local_kind,
                chunk_no,
                payload
            }),
        c((
            any::<[u8; 2]>(),
            any::<u16>(),
            any::<u32>(),
            prop::sample::select(vec![Side::Buy, Side::Sell]),
            any::<[u8; 3]>(),
            any::<u32>()
        )
            .prop_map(|(x, p, inst, side, p2, ttl_ms)| ControlCmd::SmokeOrder {
                account: AccountId(x[0]),
                venue: VenueId(x[1]),
                _pad: p,
                inst: InstId(inst),
                side,
                _pad2: p2,
                ttl_ms
            })
            .boxed()),
        c((any::<u8>(), any::<[u8; 3]>(), any::<u32>())
            .prop_map(|(a, p, g)| ControlCmd::WriteOffResidual {
                account: AccountId(a),
                _pad: p,
                group: GroupId(g)
            })
            .boxed()),
        c((any::<[u8; 2]>(), 1u8..=4, any::<u8>(), any::<u32>())
            .prop_map(|(x, fault, p, delay_ms)| ControlCmd::SimFault {
                account: AccountId(x[0]),
                venue: VenueId(x[1]),
                fault,
                _pad: p,
                delay_ms
            })
            .boxed()),
    ]
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(2_000))]
    #[test]
    fn evt_153_cmd_roundtrip(g in arb_cmd(), hb in prop::collection::vec(any::<u8>(), 128)) {
        let h = CmdHdr::read_from_bytes(&hb).unwrap();
        let cmd = match &g {
            Gen::Cmd(c) => *c,
            Gen::Warmup { instance, last, local_kind, chunk_no, payload } => ControlCmd::InjectWarmup {
                instance: StrategyId(*instance), last: *last, local_kind: *local_kind, chunk_no: *chunk_no,
                len: payload.len() as u16, _pad: [0; 6], payload,
            },
        };
        let mut out = vec![0u8; 8192];
        let n = cmd.encode(&h, &mut out).unwrap();
        let (h2, c2) = ControlCmd::decode(&out[..n]).unwrap();
        prop_assert_eq!(c2, cmd);
        prop_assert_eq!(n, 128 + h2.payload_len as usize);
        let mut hx = h;
        hx.cmd = h2.cmd;
        hx.cmd_ver = h2.cmd_ver;
        hx.payload_len = h2.payload_len;
        prop_assert_eq!(h2, hx);
    }
}
