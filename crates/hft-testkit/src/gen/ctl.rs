//! `control_cmds` (README "2. proptest 生成器"): random but type-correct `ControlCmd`s with a
//! principal and role, restricted to an allowed set of variants.

use hft_event::{CmdHdr, CmdScope, ControlCmd};
use hft_types::{
    AccountId, AssetId, CapSource, GroupId, InstId, KillLevel, Qty, RunMode, Side, StrategyId, VenueId,
};
use proptest::prelude::*;

use crate::dst::actors::encode_cmd;
use crate::dst::sut::InputRec;

/// Generated command variants (the registered coverage list of TK-014 included).
#[derive(Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Debug)]
pub enum CmdVariant {
    SetMode,
    Kill,
    /// `Kill{Account, hard}`.
    KillHardAccount,
    /// Viewer emergency stop (`Kill` by a Viewer principal).
    ViewerEmergency,
    /// `Kill{flatten}` while the start-up gate is closed.
    KillFlattenGateClosed,
    /// `Kill{flatten}` during `DRAINING`.
    KillFlattenDraining,
    Resume,
    SetParams,
    /// `SetParams` whose two legs are on one exchange (illegal `GroupSpec`).
    SameVenueSetParams,
    HedgeFix,
    AdoptExchange,
    SetBalanceBaseline,
    CancelAll,
    VenueNotice,
    SimReset,
    SimTransfer,
    InjectWarmup,
    StrategyCmd,
    SmokeOrder,
    /// `WriteOffResidual` on a quiet, non-Broken group.
    WriteOffResidualQuiet,
}

impl CmdVariant {
    pub const ALL: [CmdVariant; 20] = [
        CmdVariant::SetMode,
        CmdVariant::Kill,
        CmdVariant::KillHardAccount,
        CmdVariant::ViewerEmergency,
        CmdVariant::KillFlattenGateClosed,
        CmdVariant::KillFlattenDraining,
        CmdVariant::Resume,
        CmdVariant::SetParams,
        CmdVariant::SameVenueSetParams,
        CmdVariant::HedgeFix,
        CmdVariant::AdoptExchange,
        CmdVariant::SetBalanceBaseline,
        CmdVariant::CancelAll,
        CmdVariant::VenueNotice,
        CmdVariant::SimReset,
        CmdVariant::SimTransfer,
        CmdVariant::InjectWarmup,
        CmdVariant::StrategyCmd,
        CmdVariant::SmokeOrder,
        CmdVariant::WriteOffResidualQuiet,
    ];
}

/// One generated command: variant, principal, role and the encoded `ControlCmd` Input.
#[derive(Clone, Debug)]
pub struct GenCmd {
    pub variant: CmdVariant,
    pub issuer: &'static str,
    pub role: u8,
    pub input: InputRec,
}

fn scope(a: AccountId) -> CmdScope {
    CmdScope {
        strategy: StrategyId(0),
        venue: VenueId(0),
        account: a,
        _pad: 0,
        group: hft_types::GROUP_ALL,
        inst: InstId(0),
        _pad2: 0,
    }
}

fn blob() -> hft_event::BlobRef {
    hft_event::BlobRef {
        kind: hft_event::BlobKind::StrategyParams,
        encoding: 0,
        len: 0,
        version: 1,
        sha256: [0; 32],
    }
}

/// Builds the command of `v` for account `a` (`id` = cmd_id seed, `at` = issue time).
pub fn build(v: CmdVariant, a: AccountId, id: u64, at: u64) -> GenCmd {
    use CmdVariant as V;
    let warm = [0u8; 8];
    let (issuer, role) = match v {
        V::ViewerEmergency => ("viewer", CmdHdr::ROLE_VIEWER),
        V::SmokeOrder | V::SimReset | V::SimTransfer | V::InjectWarmup | V::AdoptExchange => {
            ("admin", CmdHdr::ROLE_ADMIN)
        }
        _ => ("trader", CmdHdr::ROLE_TRADER),
    };
    let kill = |flatten: bool, hard: bool, level: KillLevel| ControlCmd::Kill {
        scope: scope(if level == KillLevel::Global {
            AccountId(0)
        } else {
            a
        }),
        level,
        flatten,
        cancel_resting: true,
        hard,
    };
    let c = match v {
        V::SetMode => ControlCmd::SetMode {
            scope: scope(a),
            mode: RunMode::CloseOnly,
        },
        V::Kill => kill(false, false, KillLevel::Account),
        V::KillHardAccount => kill(false, true, KillLevel::Account),
        V::ViewerEmergency => kill(false, false, KillLevel::Global),
        V::KillFlattenGateClosed | V::KillFlattenDraining => kill(true, false, KillLevel::Account),
        V::Resume => ControlCmd::Resume {
            scope: scope(a),
            source: CapSource::KILL,
        },
        V::SetParams | V::SameVenueSetParams => ControlCmd::SetParams {
            scope: scope(a),
            blob: blob(),
        },
        V::HedgeFix => ControlCmd::HedgeFix { scope: scope(a) },
        V::AdoptExchange => ControlCmd::AdoptExchange {
            account: a,
            venue: VenueId(1),
            inst: InstId(1),
            report_id: 1,
            item: 1,
            _pad: 0,
            asset: AssetId(0),
            group: GroupId(0),
            entry_px: hft_types::Px(0),
        },
        V::SetBalanceBaseline => ControlCmd::SetBalanceBaseline {
            account: a,
            venue: VenueId(1),
        },
        V::CancelAll => ControlCmd::CancelAll { scope: scope(a) },
        V::VenueNotice => ControlCmd::VenueNotice {
            venue: VenueId(1),
            kind: 1,
            _pad: 0,
            inst: InstId(1),
            a: 0,
            b: 0,
        },
        V::SimReset => ControlCmd::SimReset { account: a },
        V::SimTransfer => ControlCmd::SimTransfer {
            account: a,
            from: VenueId(1),
            to: VenueId(2),
            asset: AssetId(2),
            amount: Qty(1_000_000),
        },
        V::InjectWarmup => ControlCmd::InjectWarmup {
            instance: StrategyId(1),
            last: 1,
            local_kind: 1,
            chunk_no: 0,
            len: warm.len() as u16,
            _pad: [0; 6],
            payload: &warm,
        },
        V::StrategyCmd => ControlCmd::StrategyCmd {
            instance: StrategyId(1),
            reduce_only: 0,
            len: 0,
            group: GroupId(0),
            payload: [0; 256],
        },
        V::SmokeOrder => ControlCmd::SmokeOrder {
            account: a,
            venue: VenueId(1),
            _pad: 0,
            inst: InstId(1),
            side: Side::Buy,
            _pad2: [0; 3],
            ttl_ms: 1_000,
        },
        V::WriteOffResidualQuiet => ControlCmd::WriteOffResidual {
            account: a,
            _pad: [0; 3],
            group: GroupId(0x0100_0001),
        },
    };
    GenCmd {
        variant: v,
        issuer,
        role,
        input: encode_cmd(&c, issuer, role, a, at, id),
    }
}

/// Sequences of commands whose variants are drawn from `allowed` (non-empty).
pub fn control_cmds(allowed: Vec<CmdVariant>) -> impl Strategy<Value = Vec<GenCmd>> {
    assert!(!allowed.is_empty(), "control_cmds: empty allowed set");
    let n = allowed.len();
    proptest::collection::vec((0..n, 1u8..=2, any::<u64>()), 1..8).prop_map(move |v| {
        v.into_iter()
            .enumerate()
            .map(|(k, (ix, a, id))| build(allowed[ix], AccountId(a), id, k as u64 * 1_000_000))
            .collect()
    })
}
