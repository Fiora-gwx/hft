//! Versioned, bounded process-local account records. Signed requests are never serialized.
use hft_event::*;
use hft_types::{AccountId, ConnId};
use hft_venue_proto::{AcctQueryKind, ExchIdHint, ReportSink};
use zerocopy::{FromBytes, IntoBytes, TryFromBytes};

pub const HEADER: usize = 80;
pub const RAW: u8 = 1;
pub const DONE: u8 = 2;
pub const FAILED: u8 = 3;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
#[repr(u8)]
pub enum AcctFail {
    #[default]
    None = 0,
    Timeout = 1,
    Http = 2,
    Decode = 3,
    TooLarge = 4,
    RateLimited = 5,
}
#[derive(Clone, Copy, Debug)]
pub struct AcctRecHdr {
    pub seq_no: u32,
    pub account: AccountId,
    pub what: u8,
    pub kind: AcctQueryKind,
    pub step: u8,
    pub part: u8,
    pub parts: u8,
    pub fail: AcctFail,
    pub conn: ConnId,
    pub http_status: u16,
    pub sent_tsc: u64,
    pub ts: hft_net::Timestamps,
    pub req_len: u32,
    pub body_len: u32,
}
impl AcctRecHdr {
    pub fn write(self, bytes: &mut [u8]) -> Option<()> {
        let b = bytes.get_mut(..HEADER)?;
        b.fill(0);
        b[..8].copy_from_slice(b"ACCTv001");
        b[8..12].copy_from_slice(&self.seq_no.to_le_bytes());
        b[12..20].copy_from_slice(&[
            self.account.0,
            self.what,
            self.kind as u8,
            self.step,
            self.part,
            self.parts,
            self.fail as u8,
            self.conn.0,
        ]);
        b[20..22].copy_from_slice(&self.http_status.to_le_bytes());
        b[24..32].copy_from_slice(&self.sent_tsc.to_le_bytes());
        b[32..40].copy_from_slice(&self.ts.rx_tsc.to_le_bytes());
        b[40..48].copy_from_slice(&self.ts.hw_rx_ns.to_le_bytes());
        b[48..56].copy_from_slice(&self.ts.sw_rx_ns.to_le_bytes());
        b[56..60].copy_from_slice(&self.ts.rx_empty_gap_ns.to_le_bytes());
        b[64..68].copy_from_slice(&self.req_len.to_le_bytes());
        b[68..72].copy_from_slice(&self.body_len.to_le_bytes());
        b[72..76].copy_from_slice(&self.ts.flags.to_le_bytes());
        Some(())
    }
    pub fn read(b: &[u8]) -> Option<Self> {
        if b.get(..8)? != b"ACCTv001" || b.len() < HEADER {
            return None;
        }
        let u32_at = |i| Some(u32::from_le_bytes(b.get(i..i + 4)?.try_into().ok()?));
        let u64_at = |i| Some(u64::from_le_bytes(b.get(i..i + 8)?.try_into().ok()?));
        let kind = match b[14] {
            1 => AcctQueryKind::Config,
            2 => AcctQueryKind::Margin,
            _ => return None,
        };
        let fail = match b[18] {
            0 => AcctFail::None,
            1 => AcctFail::Timeout,
            2 => AcctFail::Http,
            3 => AcctFail::Decode,
            4 => AcctFail::TooLarge,
            5 => AcctFail::RateLimited,
            _ => return None,
        };
        let r = Self {
            seq_no: u32_at(8)?,
            account: AccountId(b[12]),
            what: b[13],
            kind,
            step: b[15],
            part: b[16],
            parts: b[17],
            fail,
            conn: ConnId(b[19]),
            http_status: u16::from_le_bytes(b[20..22].try_into().ok()?),
            sent_tsc: u64_at(24)?,
            ts: hft_net::Timestamps {
                rx_tsc: u64_at(32)?,
                hw_rx_ns: u64_at(40)? as i64,
                sw_rx_ns: u64_at(48)? as i64,
                rx_empty_gap_ns: u32_at(56)?,
                flags: u32_at(72)?,
            },
            req_len: u32_at(64)?,
            body_len: u32_at(68)?,
        };
        (b.len() == HEADER + r.req_len as usize + r.body_len as usize).then_some(r)
    }
}

/// Complete report batch, preallocated at task startup. Overflow invalidates the entire sequence.
#[derive(Debug)]
pub(crate) struct Reports {
    pub bytes: Vec<u8>,
    pub failed: bool,
    pub count: usize,
}
impl Reports {
    pub fn new(cap: usize) -> Self {
        Self {
            bytes: Vec::with_capacity(cap),
            failed: false,
            count: 0,
        }
    }
    pub fn clear(&mut self) {
        self.bytes.clear();
        self.failed = false;
        self.count = 0;
    }
    pub fn session(&mut self, req: crate::AcctReq, rd: &hft_refdata::RefData) {
        if req.kind != AcctQueryKind::Config {
            return;
        }
        let mut pos = 0;
        let mut configs = 0;
        while pos < self.bytes.len() {
            let Some((msg, _, n)) = report(&self.bytes[pos..]) else {
                self.failed = true;
                return;
            };
            if let RepMsg::AccountConfigReport(mut h) = msg {
                configs += 1;
                if rd.account(req.account).uid_policy == hft_refdata::UidPolicy::Verified {
                    let end = h.venue_uid.iter().position(|b| *b == 0).unwrap_or(24);
                    if end == 0 || h.uid_keys & 2 == 0 {
                        h.mismatches |= 256;
                    } else if rd.venue_uid(req.account, h.venue).map(str::as_bytes)
                        != Some(&h.venue_uid[..end])
                    {
                        h.mismatches |= 128;
                    }
                }
                if req.session == 0 && h.uid_keys & 2 != 0 && h.venue_uid.iter().any(|b| *b != 0) {
                    h.uid_keys |= 1;
                }
                if req.session == 2 {
                    h.uid_keys &= !1;
                    h.mismatches |= 256;
                } else if req.session == 1 {
                    let expected = rd.venue_uid(req.account, h.venue).map(str::as_bytes);
                    let uid_end = req.session_uid.iter().position(|b| *b == 0).unwrap_or(24);
                    if uid_end == 0 {
                        h.uid_keys &= !1;
                        if rd.account(req.account).uid_policy != hft_refdata::UidPolicy::Unverifiable {
                            h.mismatches |= 256;
                        }
                    } else {
                        h.uid_keys |= 1;
                        if (rd.account(req.account).uid_policy == hft_refdata::UidPolicy::Verified
                            && expected != Some(&req.session_uid[..uid_end]))
                            || h.venue_uid != req.session_uid
                        {
                            h.mismatches |= 128;
                        }
                    }
                }
                self.bytes[pos + 4..pos + 68].copy_from_slice(h.as_bytes());
            }
            pos += n;
        }
        if configs != 1 {
            self.failed = true;
        }
    }
    fn push(&mut self, tag: u16, head: &[u8], tail: &[u8]) {
        let len = head.len() + tail.len();
        if self.count == 64 || len > u16::MAX as usize || self.bytes.len() + 4 + len > self.bytes.capacity() {
            self.failed = true;
            return;
        }
        self.bytes.extend_from_slice(&tag.to_le_bytes());
        self.bytes.extend_from_slice(&(len as u16).to_le_bytes());
        self.bytes.extend_from_slice(head);
        self.bytes.extend_from_slice(tail);
        self.count += 1;
    }
}
impl ReportSink for Reports {
    fn order(&mut self, e: &OrderEvent) {
        self.push(1, e.as_bytes(), &[]);
    }
    fn position(&mut self, e: &PositionReport) {
        self.push(2, e.as_bytes(), &[]);
    }
    fn balance(&mut self, e: &BalanceReport) {
        self.push(3, e.as_bytes(), &[]);
    }
    fn funding_settle(&mut self, e: &FundingSettle) {
        self.push(4, e.as_bytes(), &[]);
    }
    fn external_fill(&mut self, e: &ExternalFill) {
        self.push(5, e.as_bytes(), &[]);
    }
    fn margin(&mut self, h: &MarginReportHdr, i: &[MarginItem]) {
        if usize::from(h.n) != i.len() || h.n > MarginReportHdr::MAX_ITEMS {
            self.failed = true;
            return;
        }
        self.push(6, h.as_bytes(), i.as_bytes());
    }
    fn account_config(&mut self, h: &AccountConfigReportHdr, i: &[LeverageItem]) {
        if usize::from(h.n_lev) != i.len() || h.n_lev > AccountConfigReportHdr::MAX_ITEMS {
            self.failed = true;
            return;
        }
        self.push(7, h.as_bytes(), i.as_bytes());
    }
    fn exch_id(&mut self, _: &[u8], _: Option<&hft_types::ClientOrderId>) -> ExchIdHint {
        ExchIdHint::Unknown
    }
}

pub(crate) fn report(b: &[u8]) -> Option<(RepMsg, &[u8], usize)> {
    let tag = u16::from_le_bytes(b.get(..2)?.try_into().ok()?);
    let n = u16::from_le_bytes(b.get(2..4)?.try_into().ok()?) as usize;
    let body = b.get(4..4 + n)?;
    let (m, tail) = match tag {
        1 => (
            RepMsg::OrderEvent(
                OrderEvent::try_read_from_bytes(body).ok()?,
                hft_event::RecordFlags(0),
            ),
            &[][..],
        ),
        2 => (
            RepMsg::PositionReport(PositionReport::try_read_from_bytes(body).ok()?),
            &[][..],
        ),
        3 => (
            RepMsg::BalanceReport(BalanceReport::read_from_bytes(body).ok()?),
            &[][..],
        ),
        4 => (
            RepMsg::FundingSettle(FundingSettle::read_from_bytes(body).ok()?),
            &[][..],
        ),
        5 => (
            RepMsg::ExternalFill(ExternalFill::try_read_from_bytes(body).ok()?),
            &[][..],
        ),
        6 => {
            let (h, tail) = body.split_at_checked(64)?;
            let h = MarginReportHdr::read_from_bytes(h).ok()?;
            if h.n > MarginReportHdr::MAX_ITEMS || tail.len() != usize::from(h.n) * 16 {
                return None;
            }
            (RepMsg::MarginReport(h), tail)
        }
        7 => {
            let (h, tail) = body.split_at_checked(64)?;
            let h = AccountConfigReportHdr::read_from_bytes(h).ok()?;
            if h.n_lev > AccountConfigReportHdr::MAX_ITEMS || tail.len() != usize::from(h.n_lev) * 16 {
                return None;
            }
            (RepMsg::AccountConfigReport(h), tail)
        }
        _ => return None,
    };
    Some((m, tail, 4 + n))
}
