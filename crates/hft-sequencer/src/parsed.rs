//! Parsed large objects (`ParsedConfig<S>`, README "装配与运行"): built by ctl-gateway in the live
//! system (Mailbox), and by the replay drivers from the logged `ConfigBlob` with the same pure
//! functions ([`parse_params`]).

use hft_event::{BlobKind, BlobRef, GroupSpec, parse_spb1};
use hft_refdata::{LegView, LoadErrCode, LoadReport, RefData};
use hft_risk::RiskParams;
use hft_strategy_sdk::{GroupTable, ParamError, RefView, ResetPreview, StrategySet};
use hft_types::{AccountId, InstId, StrategyId, VenueId};

use crate::codes::detail;

/// Decoded `SetParams` content.
pub struct ParsedParams<S: StrategySet> {
    /// SHA-256 of the SPB1 blob (= `BlobRef.sha256`).
    pub sha256: [u8; 32],
    pub strategy: StrategyId,
    pub account: AccountId,
    pub version: u64,
    pub kind_id: u16,
    pub params: S::Params,
    pub groups: GroupTable,
    pub model_mem: u64,
    pub model_ckpt: u64,
    pub structural_changed: bool,
    pub reset_preview: ResetPreview,
}

/// One Mailbox object (ctl-gateway → sequencer; returned through the reverse Mailbox).
pub enum ParsedConfig<S: StrategySet> {
    StrategyParams(Box<ParsedParams<S>>),
    Risk {
        sha256: [u8; 32],
        params: Box<RiskParams>,
    },
    /// `RefData::load(new blob, current)`; errors are passed on for the sequencer's recheck.
    Refdata {
        sha256: [u8; 32],
        result: Box<Result<RefData, LoadReport>>,
    },
}

impl<S: StrategySet> core::fmt::Debug for ParsedParams<S> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("ParsedParams")
            .field("strategy", &self.strategy)
            .field("account", &self.account)
            .field("version", &self.version)
            .field("model_mem", &self.model_mem)
            .field("structural_changed", &self.structural_changed)
            .finish_non_exhaustive()
    }
}

impl<S: StrategySet> ParsedConfig<S> {
    /// SHA-256 of the blob the object was parsed from.
    pub fn sha256(&self) -> &[u8; 32] {
        match self {
            ParsedConfig::StrategyParams(p) => &p.sha256,
            ParsedConfig::Risk { sha256, .. } | ParsedConfig::Refdata { sha256, .. } => sha256,
        }
    }
}

impl<S: StrategySet> core::fmt::Debug for ParsedConfig<S> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        let kind = match self {
            ParsedConfig::StrategyParams(_) => "StrategyParams",
            ParsedConfig::Risk { .. } => "Risk",
            ParsedConfig::Refdata { .. } => "Refdata",
        };
        f.debug_struct("ParsedConfig")
            .field("kind", &kind)
            .finish_non_exhaustive()
    }
}

/// `RiskParams` decoding for `SetRisk` (encoding owned outside this crate, injected at assembly);
/// `Err(detail)` = InvalidArgs detail.
pub type RiskDecode = fn(&[u8], AccountId, &RefData) -> Result<RiskParams, u16>;

/// The current parameters of an instance (the comparison base of a `SetParams`).
pub struct ParamBase<'a, S: StrategySet> {
    pub params: &'a S::Params,
    pub groups: &'a GroupTable,
}

impl<S: StrategySet> Clone for ParamBase<'_, S> {
    fn clone(&self) -> Self {
        *self
    }
}
impl<S: StrategySet> Copy for ParamBase<'_, S> {}
impl<S: StrategySet> core::fmt::Debug for ParamBase<'_, S> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str("ParamBase")
    }
}

/// `ParamInvalid{Strategy}` arg of a strategy `ParamError`.
pub fn strategy_param_detail(e: &ParamError) -> u32 {
    let field = match e {
        ParamError::OutOfRange { field_ix, .. }
        | ParamError::BadEnum { field_ix }
        | ParamError::NotPerGroup { field_ix }
        | ParamError::Invalid { field_ix, .. }
        | ParamError::BadText { field_ix } => *field_ix,
        _ => detail::PARAM_FIELD_NONE,
    };
    detail::pack(
        detail::PARAM_INVALID,
        detail::param_arg(detail::PARAM_KIND_STRATEGY, field),
    )
}

/// Parses an SPB1 blob the way ctl-gateway does (README "命令路径" `SetParams`; the gateway
/// checks binding, legs, schema and decodes; the sequencer rechecks). Pure; cold path.
/// `Err` = the `CmdResult.detail` (status `InvalidArgs`).
pub fn parse_params<S: StrategySet>(
    blob: &[u8],
    sha256: [u8; 32],
    rd: &RefData,
    max_groups: u16,
    base: Option<ParamBase<'_, S>>,
) -> Result<ParsedParams<S>, u32> {
    let mut buf = vec![0u64; blob.len().div_ceil(8)];
    zerocopy::IntoBytes::as_mut_bytes(buf.as_mut_slice())
        .get_mut(..blob.len())
        .ok_or(0u32)?
        .copy_from_slice(blob);
    let bytes = zerocopy::IntoBytes::as_bytes(buf.as_slice())
        .get(..blob.len())
        .ok_or(0u32)?;
    let v = parse_spb1(bytes).map_err(|e| {
        u32::from(match e {
            hft_event::ParamsErr::DupGroup => detail::GROUP_ID_DUP,
            _ => detail::BAD_GROUP,
        })
    })?;
    let kind = v.hdr.kind_id;
    let strategy = v.hdr.strategy;
    let account = v.hdr.account;
    S::kinds()
        .iter()
        .find(|d| d.kind_id == kind)
        .ok_or(u32::from(detail::SCHEMA_UNSUPPORTED))?;
    let ver = v.hdr.schema_ver;
    let groups = GroupTable::new(v.groups, max_groups);
    let params = S::decode_params(kind, v.body, ver, &groups).map_err(|e| strategy_param_detail(&e))?;
    S::validate_params(&params, &groups, &RefView::new(rd, account))
        .map_err(|e| strategy_param_detail(&e))?;
    let n_groups = u16::try_from(v.groups.len()).unwrap_or(u16::MAX);
    let model_mem = S::model_mem_bytes(&params, n_groups);
    let model_ckpt = S::model_snapshot_bytes(&params, n_groups);
    let (reset_preview, structural_changed) = match base {
        Some(b) => {
            let rp = S::reset_preview(b.params, b.groups, &params, &groups);
            let kept: Vec<GroupSpec> = v
                .groups
                .iter()
                .filter(|g| b.groups.gix_of(g.id).is_some())
                .copied()
                .collect();
            let gt_kept = GroupTable::new(&kept, max_groups);
            let sc = match S::decode_params(kind, v.body, ver, &gt_kept) {
                Ok(pk) => S::reset_preview(b.params, b.groups, &pk, &gt_kept).reset_groups > 0,
                Err(_) => rp.reset_groups > 0,
            };
            (rp, sc)
        }
        None => (ResetPreview::default(), false),
    };
    Ok(ParsedParams {
        sha256,
        strategy,
        account,
        version: v.hdr.version,
        kind_id: kind,
        params,
        groups,
        model_mem,
        model_ckpt,
        structural_changed,
        reset_preview,
    })
}

/// Parses a logged `ConfigBlob` content into a Mailbox object (replay drivers).
#[allow(clippy::too_many_arguments)]
pub fn parse_blob<S: StrategySet>(
    r: &BlobRef,
    content: &[u8],
    rd: &RefData,
    max_groups: u16,
    base: Option<ParamBase<'_, S>>,
    risk_account: AccountId,
    risk_decode: Option<RiskDecode>,
) -> Result<ParsedConfig<S>, u32> {
    match r.kind {
        BlobKind::StrategyParams => parse_params::<S>(content, r.sha256, rd, max_groups, base)
            .map(|p| ParsedConfig::StrategyParams(Box::new(p))),
        BlobKind::RiskParams => {
            let d = risk_decode.ok_or(u32::from(detail::BAD_VALUE))?;
            let p = d(content, risk_account, rd).map_err(u32::from)?;
            Ok(ParsedConfig::Risk {
                sha256: r.sha256,
                params: Box::new(p),
            })
        }
        BlobKind::Refdata => Ok(ParsedConfig::Refdata {
            sha256: r.sha256,
            result: Box::new(RefData::load(content, Some(rd))),
        }),
        _ => Err(u32::from(detail::UNKNOWN_DISCRIMINANT)),
    }
}

/// `check_group` over every GroupSpec of a table; `Err` = CmdResult detail.
pub fn check_groups(rd: &RefData, account: AccountId, groups: &GroupTable) -> Result<(), u32> {
    for (_, g) in groups.iter() {
        let mut legs = [LegView {
            inst: InstId(0),
            venue: VenueId(0),
        }; 4];
        if let Err(e) = rd.check_group(account, &g.view(&mut legs)) {
            return Err(if e.code == LoadErrCode::SameExchangeLegs {
                u32::from(detail::SAME_EXCHANGE_LEGS)
            } else {
                detail::pack(detail::GROUP_CHECK, e.code as u16)
            });
        }
    }
    Ok(())
}
