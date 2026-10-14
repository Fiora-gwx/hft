//! Account sequence goldens compare request fields and the complete report batch independently.
use super::{GoldenDriver, GoldenFail, ReportRec, VecSink};
use crate::{AcctProtocol, AcctQuery, AcctQueryKind, RestReqBuf, WallClock};
use hft_refdata::RefData;
use hft_types::AccountId;
use zerocopy::IntoBytes;

#[derive(Debug)]
pub struct AcctSample<'a> {
    pub name: &'a str,
    pub query: AcctQuery,
    pub status: u16,
    pub body: &'a [u8],
    pub request: &'a RestReqBuf,
}
impl ReportRec {
    pub fn bytes(&self) -> Vec<u8> {
        let (tag, mut body) = match self {
            Self::Order(e) => (1, e.as_bytes().to_vec()),
            Self::Position(e) => (2, e.as_bytes().to_vec()),
            Self::Balance(e) => (3, e.as_bytes().to_vec()),
            Self::Funding(e) => (4, e.as_bytes().to_vec()),
            Self::External(e) => (5, e.as_bytes().to_vec()),
            Self::Margin(h, tail) => {
                let mut b = h.as_bytes().to_vec();
                b.extend_from_slice(tail.as_slice().as_bytes());
                (6, b)
            }
            Self::Config(h, tail) => {
                let mut b = h.as_bytes().to_vec();
                b.extend_from_slice(tail.as_slice().as_bytes());
                (7, b)
            }
        };
        body.insert(0, tag);
        body
    }
}
fn request_fields(r: &RestReqBuf) -> Vec<Vec<u8>> {
    vec![
        vec![r.endpoint, r.method as u8, u8::from(r.redacted)],
        r.path_query.as_slice().to_vec(),
        r.headers.as_slice().to_vec(),
        r.body.as_slice().to_vec(),
    ]
}
impl<P: crate::VenueProtocol> GoldenDriver<P> {
    pub fn compare_request(name: &str, actual: &RestReqBuf, expected: &RestReqBuf) -> Result<(), GoldenFail> {
        Self::compare(name, &request_fields(actual), &request_fields(expected))
    }
}
impl<P: AcctProtocol> GoldenDriver<P> {
    /// Each expected request and report must be authored independently of the implementation.
    #[allow(clippy::too_many_arguments)]
    pub fn run_acct(
        kind: AcctQueryKind,
        samples: &[AcctSample<'_>],
        rd: &RefData,
        account: AccountId,
        templates: &P::Templates,
        clock: &WallClock,
        ctx: &mut P::AcctCtx,
        expected: &[ReportRec],
    ) -> Result<(), GoldenFail> {
        let mut scratch = P::AcctScratch::default();
        let fail = |name: &str| GoldenFail {
            sample: name.into(),
            record: 0,
            offset: 0,
        };
        for s in samples {
            if s.query.kind != kind || !P::acct_queries(kind).contains(&s.query) {
                return Err(fail(s.name));
            }
            let mut r = RestReqBuf::default();
            P::acct_request(rd, templates, account, s.query, clock, &mut r).map_err(|_| fail(s.name))?;
            Self::compare(s.name, &request_fields(&r), &request_fields(s.request))?;
            P::acct_decode(rd, ctx, account, s.query, s.status, s.body, &mut scratch)
                .map_err(|_| fail(s.name))?;
        }
        let mut sink = VecSink::default();
        P::acct_finish(rd, ctx, account, kind, &scratch, &mut sink).map_err(|_| fail("acct_finish"))?;
        Self::compare(
            "acct_finish",
            &sink.reports.iter().map(ReportRec::bytes).collect::<Vec<_>>(),
            &expected.iter().map(ReportRec::bytes).collect::<Vec<_>>(),
        )
    }
}
