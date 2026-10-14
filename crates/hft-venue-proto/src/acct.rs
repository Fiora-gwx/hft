//! Bounded account-query extension. Contexts belong exclusively to the warm account task.
use crate::{DecodeErr, EncodeErr, ReportSink, RestReqBuf, VenueProtocol, WallClock};
use hft_refdata::RefData;
use hft_types::AccountId;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum AcctQueryKind {
    Config = 1,
    Margin = 2,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AcctQuery {
    pub kind: AcctQueryKind,
    pub step: u8,
    pub every: u8,
    pub push_fallback: bool,
    pub weight: u16,
}
impl AcctQuery {
    pub fn selected(self, poll_no: u32, fallback: bool) -> bool {
        self.every != 0 && poll_no.is_multiple_of(u32::from(self.every)) && (!fallback || self.push_fallback)
    }
}

/// Explicit extension so a protocol without bounded account decoding cannot accidentally satisfy
/// the account startup gate. Implementations must not allocate after context construction.
pub trait AcctProtocol: VenueProtocol {
    type AcctCtx: Default;
    type AcctScratch: Default;
    fn acct_queries(kind: AcctQueryKind) -> &'static [AcctQuery];
    fn acct_request(
        rd: &RefData,
        t: &Self::Templates,
        account: AccountId,
        query: AcctQuery,
        clock: &WallClock,
        out: &mut RestReqBuf,
    ) -> Result<(), EncodeErr>;
    /// Borrow the account's REST credential for this request only. Templates and contexts must
    /// never retain it. Unsigned test protocols may use the compatibility implementation.
    fn acct_request_signed(
        rd: &RefData,
        t: &Self::Templates,
        credential: &crate::Credential,
        account: AccountId,
        query: AcctQuery,
        clock: &WallClock,
        out: &mut RestReqBuf,
    ) -> Result<(), EncodeErr> {
        let _ = credential;
        Self::acct_request(rd, t, account, query, clock, out)
    }
    fn acct_decode(
        rd: &RefData,
        ctx: &mut Self::AcctCtx,
        account: AccountId,
        query: AcctQuery,
        status: u16,
        body: &[u8],
        scratch: &mut Self::AcctScratch,
    ) -> Result<(), DecodeErr>;
    fn acct_finish(
        rd: &RefData,
        ctx: &mut Self::AcctCtx,
        account: AccountId,
        kind: AcctQueryKind,
        scratch: &Self::AcctScratch,
        out: &mut impl ReportSink,
    ) -> Result<(), DecodeErr>;
}
