//! Sample commands for the ControlCmd tests.
#![allow(dead_code)]

use hft_event::*;
use hft_types::*;

pub fn scope() -> CmdScope {
    CmdScope {
        strategy: StrategyId(3),
        venue: VenueId(5),
        account: AccountId(2),
        _pad: 0,
        group: GroupId(0x0300_0001),
        inst: InstId(77),
        _pad2: 0,
    }
}

pub fn blob_ref() -> BlobRef {
    BlobRef {
        kind: BlobKind::StrategyParams,
        encoding: 1,
        len: 224,
        version: 9,
        sha256: [0x5A; 32],
    }
}

pub fn hdr() -> CmdHdr {
    let mut h = CmdHdr::zeroed();
    h.cmd_id = [1; 16];
    h.issuer = *b"tg:123456789\0\0\0\0\0\0\0\0\0\0\0\0";
    h.issued_at_ns = 11;
    h.expires_at_ns = 22;
    h.expires_tsc = Tsc(33);
    h.peer_uid = 1000;
    h.cmd = CmdKind(0x0001);
    h.cmd_ver = 9;
    h.role = 2;
    h.account = AccountId(2);
    h.confirmed = 1;
    h.hflags = 1;
    h.payload_len = 999;
    h.cmd_hash = [7; 32];
    h.base_version = 44;
    h.expect_state_version = 55;
    h
}

pub fn encode(c: &ControlCmd<'_>) -> Vec<u8> {
    let mut out = vec![0u8; 8192];
    let n = c.encode(&hdr(), &mut out).unwrap();
    out.truncate(n);
    out
}

pub fn strategy_cmd(len: u16) -> ControlCmd<'static> {
    let mut payload = [0u8; 256];
    for (i, b) in payload.iter_mut().take(usize::from(len).min(256)).enumerate() {
        *b = (i as u8).wrapping_add(1);
    }
    ControlCmd::StrategyCmd {
        instance: StrategyId(3),
        reduce_only: 1,
        len,
        group: GroupId(0x0300_0002),
        payload,
    }
}

pub fn warmup(payload: &[u8]) -> ControlCmd<'_> {
    ControlCmd::InjectWarmup {
        instance: StrategyId(3),
        last: 1,
        local_kind: 4,
        chunk_no: 6,
        len: payload.len() as u16,
        _pad: [0; 6],
        payload,
    }
}

/// One sample of every variant (fields non-zero where legal).
pub fn all_samples(w: &[u8]) -> Vec<ControlCmd<'_>> {
    vec![
        ControlCmd::SetMode {
            scope: scope(),
            mode: RunMode::CloseOnly,
        },
        ControlCmd::SetParams {
            scope: scope(),
            blob: blob_ref(),
        },
        ControlCmd::SetRisk { blob: blob_ref() },
        ControlCmd::Kill {
            scope: scope(),
            level: KillLevel::Account,
            flatten: true,
            cancel_resting: true,
            hard: true,
        },
        ControlCmd::RotateConn {
            venue: VenueId(5),
            account: AccountId(2),
        },
        ControlCmd::Resubscribe {
            venue: VenueId(5),
            inst: InstId(77),
        },
        ControlCmd::SnapshotNow,
        ControlCmd::ReplaceRefdata { blob: blob_ref() },
        ControlCmd::Resume {
            scope: scope(),
            source: CapSource::RECON,
        },
        ControlCmd::HedgeFix { scope: scope() },
        ControlCmd::CancelAll { scope: scope() },
        ControlCmd::Drain { cancel_resting: true },
        ControlCmd::RecordTransfer {
            account: AccountId(2),
            from: VenueId(1),
            to: VenueId(2),
            asset: AssetId(9),
            amount: Qty(123),
            reference: [0x41; 64],
        },
        ControlCmd::Annotate {
            kind: 3,
            text_len: 5,
            digest: [9; 32],
            text: [0x61; 256],
        },
        ControlCmd::AdoptExchange {
            account: AccountId(2),
            venue: VenueId(5),
            inst: InstId(77),
            report_id: 88,
            item: 1,
            _pad: 0,
            asset: AssetId(4),
            group: GroupId(0x0300_0001),
            entry_px: Px(1234),
        },
        ControlCmd::SimReset {
            account: AccountId(2),
        },
        ControlCmd::SimTransfer {
            account: AccountId(2),
            from: VenueId(1),
            to: VenueId(2),
            asset: AssetId(9),
            amount: Qty(5),
        },
        ControlCmd::VenueNotice {
            venue: VenueId(5),
            kind: 2,
            _pad: 0,
            inst: InstId(77),
            a: 28_800,
            b: -1,
        },
        ControlCmd::SetBalanceBaseline {
            account: AccountId(2),
            venue: VenueId(5),
        },
        strategy_cmd(3),
        warmup(w),
        ControlCmd::SmokeOrder {
            account: AccountId(2),
            venue: VenueId(5),
            _pad: 0,
            inst: InstId(77),
            side: Side::Sell,
            _pad2: [0; 3],
            ttl_ms: 5000,
        },
        ControlCmd::WriteOffResidual {
            account: AccountId(2),
            _pad: [0; 3],
            group: GroupId(0x0300_0001),
        },
        ControlCmd::SimFault {
            account: AccountId(2),
            venue: VenueId(5),
            fault: 4,
            _pad: 0,
            delay_ms: 3000,
        },
        ControlCmd::BindSlot {
            venue: VenueId(15),
            _pad: [0; 3],
            inst: InstId(7),
            start_s: 1_792_425_300,
            end_s: 1_792_425_600,
            wire_id: [0xA5; 32],
        },
        ControlCmd::Settle {
            venue: VenueId(15),
            _pad: [0; 3],
            inst: InstId(7),
            end_s: 1_792_425_600,
            payout: Px(1000),
        },
    ]
}
