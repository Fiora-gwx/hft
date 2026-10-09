//! `external_fills` and `account_config` (README "2. proptest 生成器").

use hft_event::{AccountConfigReportHdr, ExternalCause, LeverageItem};
use hft_types::{AccountId, InstId, Rate, Side, VenueId};

/// Shape of an external fill.
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
pub enum ExternalShape {
    /// Own liquidation, pushed with the venue's liquidation marker.
    LiquidationPushed,
    /// ADL missing from the user stream (only the truth changes).
    AdlGap,
    /// ADL missing, then pushed again after the user stream reconnects (same dedup key).
    AdlGapReplay,
}

/// One pushed external fill (venue-level view).
#[derive(Clone, Eq, PartialEq, Hash, Debug)]
pub struct PushedFill {
    pub cause: ExternalCause,
    pub client_id: String,
    pub side: Side,
    pub qty: i64,
    pub trade_id: u64,
    /// Pushed after the user stream reconnected.
    pub after_reconnect: bool,
}

/// Effect of one external fill on an account holding `pos` lots of a perp (`lot` raw).
#[derive(Clone, Eq, PartialEq, Hash, Debug)]
pub struct ExternalCase {
    pub truth_delta: i64,
    pub pushes: Vec<PushedFill>,
}

/// The case of `shape` closing a position of `pos` (signed raw); `marker` = the venue's own
/// liquidation client-id prefix.
pub fn external_fills(shape: ExternalShape, pos: i64, marker: &str, trade_id: u64) -> ExternalCase {
    let side = if pos > 0 { Side::Sell } else { Side::Buy };
    let qty = pos.abs();
    let push = |cause, client_id: String, after| PushedFill {
        cause,
        client_id,
        side,
        qty,
        trade_id,
        after_reconnect: after,
    };
    match shape {
        ExternalShape::LiquidationPushed => ExternalCase {
            truth_delta: -pos,
            pushes: vec![push(
                ExternalCause::Liquidation,
                format!("{marker}{trade_id}"),
                false,
            )],
        },
        ExternalShape::AdlGap => ExternalCase {
            truth_delta: -pos,
            pushes: Vec::new(),
        },
        ExternalShape::AdlGapReplay => ExternalCase {
            truth_delta: -pos,
            pushes: vec![push(ExternalCause::Adl, String::new(), true)],
        },
    }
}

/// Account-configuration sequence classes.
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
pub enum AcctCfgShape {
    Consistent,
    StartupMismatch,
    /// Effective leverage changes at report `at`.
    LeverageChange,
    /// Spot auto-borrow switches on at report `at`.
    AutoBorrowOn,
}

/// `CfgMismatch` bits (hft-event `codes::CFG_MISMATCH`).
pub const MISMATCH_POS_MODE: u32 = 1;
pub const MISMATCH_AUTO_BORROW: u32 = 32;

/// `n` reports of one (account, venue) with one perp leverage item; the change happens at `at`.
pub fn account_config(
    shape: AcctCfgShape,
    account: AccountId,
    venue: VenueId,
    inst: InstId,
    n: usize,
    at: usize,
) -> Vec<(AccountConfigReportHdr, Vec<LeverageItem>)> {
    (0..n)
        .map(|i| {
            let mut h: AccountConfigReportHdr = zerocopy::FromZeros::new_zeroed();
            h.venue = venue;
            h.account = account;
            h.position_mode = 1;
            h.margin_mode = 1;
            h.n_lev = 1;
            h.mismatches = match shape {
                AcctCfgShape::StartupMismatch if i == 0 => MISMATCH_POS_MODE,
                AcctCfgShape::AutoBorrowOn if i >= at => MISMATCH_AUTO_BORROW,
                _ => 0,
            };
            // Rate raw (10^-9): 5× = 5 × 10^9, 10× after a change.
            let lev = if shape == AcctCfgShape::LeverageChange && i >= at {
                10_000_000_000
            } else {
                5_000_000_000
            };
            let item = LeverageItem {
                inst,
                _pad: 0,
                leverage: Rate(lev),
            };
            (h, vec![item])
        })
        .collect()
}
