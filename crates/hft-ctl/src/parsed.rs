//! Parsed large objects handed to the sequencer through the single-slot `Mailbox`.
//!
//! The type belongs to hft-sequencer (`ParsedConfig<S>`, README "依赖"); until that crate exists it
//! is defined here with the fields the hft-ctl README lists, and moves there unchanged.

use hft_refdata::{LoadReport, RefData};
use hft_risk::RiskParams;
use hft_strategy_sdk::{GroupTable, ResetPreview, StrategySet};
use hft_types::{AccountId, StrategyId};

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

/// One Mailbox object.
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
