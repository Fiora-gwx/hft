//! Authenticated funding history, decoded on the warm account-fetch lane.
use hft_refdata::RefData;
use hft_types::{InstId, Notional};

use crate::{ArrayBuf, Credential, DecodeErr, ProtoErr, RestReqBuf, VenueCfg, VenueProtocol, WallClock};

/// A fixed inclusive exchange-time window. Pagination must not change either endpoint.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FundingQuery {
    pub start_ms: i64,
    pub end_ms: i64,
    /// One-based page number (venues with cursor pagination ignore it).
    pub page: u32,
    pub cursor: ArrayBuf<64>,
}

/// Exchange bill identity, before aggregation into one settlement total.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FundingBill {
    pub id: u64,
    pub inst: InstId,
    pub time_ns: i64,
    /// Signed cash movement; positive means received. Not a rate or wallet balance.
    pub amount: Notional,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FundingPage {
    pub bills: Vec<FundingBill>,
    /// `None` is the only proof that the window has been exhausted.
    pub next: Option<FundingQuery>,
}

/// Optional pure protocol extension. The caller owns HTTP, pacing, retries and journal ingress.
/// No callbacks or partial output: any malformed row rejects the entire page.
pub trait FundingHistoryProtocol: VenueProtocol {
    fn funding_request(
        cfg: &VenueCfg,
        cred: &Credential,
        query: &FundingQuery,
        clock: &WallClock,
        out: &mut RestReqBuf,
    ) -> Result<(), ProtoErr>;

    fn funding_decode(
        rd: &RefData,
        query: &FundingQuery,
        status: u16,
        body: &[u8],
    ) -> Result<FundingPage, DecodeErr>;
}
