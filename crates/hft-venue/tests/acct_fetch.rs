use crate::support::{
    feedh::clock,
    rd::*,
    stub::{self, StubProto},
};
use hft_net::{ConnId, RestErr, Timestamps};
use hft_rt::{Backing, ByteConsumer, PollTask, Producer, byte_spsc, spsc};
use hft_venue::{
    AcctAccount, AcctFail, AcctFetchTask, AcctRecHdr, AcctReq, AcctResponse, AcctTransport, GwCfg,
};
use hft_venue_proto::{AcctQueryKind, RestReqBuf, VenueProtocol};
use std::collections::VecDeque;
#[derive(Default)]
struct Mock {
    replies: VecDeque<Result<(u16, Vec<u8>), RestErr>>,
    current: Option<Result<(u16, Vec<u8>), RestErr>>,
    waiting: bool,
    endpoints: Vec<u8>,
}
impl Mock {
    fn data(values: &[i64]) -> Self {
        Self {
            replies: values
                .iter()
                .map(|v| Ok((200, v.to_le_bytes().to_vec())))
                .collect(),
            ..Self::default()
        }
    }
}
impl AcctTransport for Mock {
    fn submit(&mut self, r: &RestReqBuf, _: u64, _: u64) -> Result<ConnId, RestErr> {
        assert!(!self.waiting);
        self.waiting = true;
        self.endpoints.push(r.endpoint);
        Ok(ConnId(224))
    }
    fn poll(&mut self, now: u64) -> Option<Result<AcctResponse<'_>, RestErr>> {
        if !self.waiting || self.replies.is_empty() {
            return None;
        }
        self.current = self.replies.pop_front();
        self.waiting = false;
        Some(match self.current.as_ref().unwrap() {
            Ok((s, b)) => Ok(AcctResponse {
                status: *s,
                body: b,
                conn: ConnId(224),
                ts: Timestamps {
                    rx_tsc: now,
                    ..Timestamps::default()
                },
            }),
            Err(e) => Err(*e),
        })
    }
    fn idle_check(&mut self, _: u64) {}
    fn abort(&mut self) {
        self.waiting = false;
        self.current = None;
    }
}
fn request(kind: AcctQueryKind) -> AcctReq {
    AcctReq {
        kind,
        account: A1,
        why: 1,
        session: 0,
        seq_no: 7,
        poll_no: 0,
        _pad: 0,
        session_uid: [0; 24],
    }
}
fn worker(mock: Mock, cfg: GwCfg) -> (AcctFetchTask<StubProto, Mock>, Producer<AcctReq>, ByteConsumer) {
    let (tx, rx) = spsc(8, Backing::Heap).unwrap();
    let (out, records) = byte_spsc(1 << 20, Backing::Heap).unwrap();
    let tpl = StubProto::build_templates(&cfg.venue_cfg(), rd_stub(), A1, &Default::default()).unwrap();
    let task = AcctFetchTask::new(
        rd_stub(),
        cfg,
        clock(),
        rx,
        out,
        vec![AcctAccount {
            account: A1,
            templates: tpl,
            transport: mock,
        }],
    )
    .unwrap();
    (task, tx, records)
}
fn drain(task: &mut AcctFetchTask<StubProto, Mock>, out: &mut ByteConsumer) -> Vec<Vec<u8>> {
    let mut records = Vec::new();
    for now in 1..80 {
        task.poll_once(now);
        while let Some(r) = out.peek() {
            records.push(r.to_vec());
            out.advance();
        }
    }
    records
}
#[test]
fn ven_257_failed_sequences_never_publish_partial_reports() {
    for (reply, want) in [
        (Err(RestErr::Timeout), AcctFail::Timeout),
        (Ok((500, vec![0; 8])), AcctFail::Http),
        (Ok((429, vec![0; 8])), AcctFail::RateLimited),
        (Ok((200, vec![0; 2])), AcctFail::Decode),
        (Ok((200, vec![0; 300])), AcctFail::TooLarge),
    ] {
        let mut m = Mock::data(&[1]);
        m.replies.push_back(reply);
        let (mut t, mut tx, mut out) = worker(
            m,
            GwCfg {
                acct_body_max: 256,
                ..GwCfg::default()
            },
        );
        tx.try_push(request(AcctQueryKind::Config)).unwrap();
        let r = drain(&mut t, &mut out);
        let headers: Vec<_> = r.iter().map(|b| AcctRecHdr::read(b).unwrap()).collect();
        assert!(headers.iter().all(|h| h.what != 2));
        assert_eq!(headers.last().unwrap().fail, want);
    }
}
#[test]
fn ven_259_raw_response_chunks_preserve_bytes_and_redact_query() {
    let mut body = vec![42; 50 * 1024];
    body[..8].copy_from_slice(&123i64.to_le_bytes());
    let m = Mock {
        replies: [Ok((200, body.clone())), Ok((200, 2i64.to_le_bytes().to_vec()))].into(),
        ..Mock::default()
    };
    let (mut t, mut tx, mut out) = worker(m, GwCfg::default());
    tx.try_push(request(AcctQueryKind::Margin)).unwrap();
    let records = drain(&mut t, &mut out);
    let mut rebuilt = Vec::new();
    let mut parts = 0;
    for b in &records {
        let h = AcctRecHdr::read(b).unwrap();
        if h.what == 1 && h.step == 0 {
            assert!(h.body_len <= 16384);
            assert_eq!(h.parts, 4);
            assert_eq!(h.part, parts);
            parts += 1;
            if h.part == 0 {
                assert_eq!(&b[81..80 + h.req_len as usize], b"/account");
            } else {
                assert_eq!(h.req_len, 0);
            }
            rebuilt.extend_from_slice(&b[80 + h.req_len as usize..]);
        }
    }
    assert_eq!(rebuilt, body);
    assert!(records.iter().all(|b| !b.windows(9).any(|w| w == b"signature")));
    assert_eq!(AcctRecHdr::read(records.last().unwrap()).unwrap().what, 2);
}
#[test]
fn ven_263_endpoint_indices_follow_query_table() {
    let (mut t, mut tx, mut out) = worker(Mock::data(&[1, 2, 3]), GwCfg::default());
    tx.try_push(request(AcctQueryKind::Config)).unwrap();
    drain(&mut t, &mut out);
    assert_eq!(t.transport_mut(A1).unwrap().endpoints, vec![0, 1, 0]);
}
#[test]
fn ven_264_warm_task_pending_and_idle() {
    let (mut t, mut tx, mut out) = worker(Mock::data(&[1, 2, 3]), GwCfg::default());
    assert!(!t.has_pending());
    tx.try_push(request(AcctQueryKind::Config)).unwrap();
    assert!(t.has_pending());
    drain(&mut t, &mut out);
    assert!(!t.has_pending());
}
#[test]
fn ven_260_done_is_one_complete_config_report() {
    use zerocopy::FromBytes;
    let (mut t, mut tx, mut out) = worker(Mock::data(&[1, 2, 3]), GwCfg::default());
    tx.try_push(request(AcctQueryKind::Config)).unwrap();
    let r = drain(&mut t, &mut out);
    let done: Vec<_> = r
        .iter()
        .filter(|b| AcctRecHdr::read(b).unwrap().what == 2)
        .collect();
    assert_eq!(done.len(), 1);
    let b = done[0];
    assert_eq!(&b[80..84], &[7, 0, 80, 0]);
    let h = hft_event::AccountConfigReportHdr::read_from_bytes(&b[84..148]).unwrap();
    assert_eq!((h.account, h.n_lev, h.uid_keys), (A1, 1, 3));
    let lev = hft_event::LeverageItem::read_from_bytes(&b[148..164]).unwrap();
    assert_eq!((lev.inst, lev.leverage.0), (S1, 5_000_000_000));
}
#[test]
fn ven_259_account_decode_only_called_by_warm_task() {
    let mut h = crate::support::gwh::gw::<StubProto>();
    let (tx, rx) = spsc(8, Backing::Heap).unwrap();
    h.rt.set_account_queries(tx);
    stub::take_log();
    for n in 0..8 {
        h.poll(3_000_000 + n);
    }
    assert!(stub::calls("acct_decode").is_empty());
    drop(rx);
    let (mut t, mut tx, mut out) = worker(Mock::data(&[1, 2, 3]), GwCfg::default());
    tx.try_push(request(AcctQueryKind::Config)).unwrap();
    drain(&mut t, &mut out);
    assert_eq!(stub::calls("acct_decode").len(), 3);
}
#[test]
fn ven_211_gateway_applies_one_record_after_orders() {
    use hft_event::RepMsg;
    use zerocopy::FromBytes;
    let mut h = crate::support::gwh::gw_full::<StubProto>(GwCfg::default(), &[A1], Default::default());
    let (tx, rx) = spsc(8, Backing::Heap).unwrap();
    h.rt.set_account_queries(tx);
    let (out_bulk, mut bulk) = byte_spsc(1 << 20, Backing::Heap).unwrap();
    h.rt.set_rep_bulk(out_bulk);
    let (other, _) = byte_spsc(1 << 20, Backing::Heap).unwrap();
    let out = std::mem::replace(&mut h.acct, other);
    let tpl = StubProto::build_templates(&GwCfg::default().venue_cfg(), rd_stub(), A1, &Default::default())
        .unwrap();
    let mut t = AcctFetchTask::<StubProto, Mock>::new(
        rd_stub(),
        GwCfg::default(),
        clock(),
        rx,
        out,
        vec![AcctAccount {
            account: A1,
            templates: tpl,
            transport: Mock::data(&[1, 2, 3]),
        }],
    )
    .unwrap();
    for n in 0..8 {
        h.poll(3_000_000 + n);
    } // schedules Config
    for n in 0..50 {
        t.poll_once(3_001_000 + n);
    } // three Raw and one Done
    let place = h.place(A1, 20, 1, S1, false);
    h.cmd(place);
    h.poll(3_002_000);
    assert!(h.reps().iter().any(|m| matches!(m, RepMsg::SendReport(_))));
    assert!(bulk.peek().is_none());
    h.poll(3_002_001);
    h.reps();
    h.poll(3_002_002);
    h.reps();
    h.poll(3_002_003);
    assert!(
        h.reps()
            .iter()
            .any(|m| matches!(m,RepMsg::AccountConfigReport(e) if e.n_lev==1))
    );
    let lev = hft_event::LeverageItem::read_from_bytes(bulk.peek().unwrap()).unwrap();
    assert_eq!(lev.inst, S1);
}

#[test]
fn ven_313_config_merges_both_key_identities() {
    use zerocopy::FromBytes;
    for (session, uid, mask, mismatch) in [
        (0, "", 3, 0),
        (1, "101", 3, 0),
        (1, "102", 3, 128),
        (2, "", 2, 256),
    ] {
        let (mut task, mut tx, mut out) = worker(Mock::data(&[1, 2, 3]), GwCfg::default());
        let mut req = request(AcctQueryKind::Config);
        req.session = session;
        req.session_uid[..uid.len()].copy_from_slice(uid.as_bytes());
        tx.try_push(req).unwrap();
        let r = drain(&mut task, &mut out);
        let done = r.iter().find(|b| AcctRecHdr::read(b).unwrap().what == 2).unwrap();
        let h = hft_event::AccountConfigReportHdr::read_from_bytes(&done[84..148]).unwrap();
        assert_eq!((h.uid_keys, h.mismatches), (mask, mismatch));
    }
}
