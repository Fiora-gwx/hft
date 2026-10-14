use crate::support::{
    feedh::{b, clock},
    rd::{S1, rd_stub},
    stub::StubProto,
};
use hft_net::{
    BuildReq,
    rest::{Error, Response},
};
use hft_rt::{Backing, byte_spsc, spsc};
use hft_venue::account::Transport;
use hft_venue::{SnapCfg, SnapDeps, SnapFetchTask, SnapRecHdr, SnapReq};
use zerocopy::FromBytes;
#[derive(Debug)]
struct Mock {
    response: Option<Result<Response, Error>>,
    sent: bool,
}
impl Transport for Mock {
    fn start(&mut self, r: BuildReq, wire: &[u8]) -> Result<(), Error> {
        assert!(wire.starts_with(b"GET /bbo HTTP/1.1"));
        assert_eq!(r.role, hft_net::ConnRole::Rest);
        self.sent = true;
        Ok(())
    }
    fn poll(&mut self, _: i64) -> Option<Result<Response, Error>> {
        if self.sent {
            self.sent = false;
            self.response.take()
        } else {
            None
        }
    }
}
#[test]
fn ven_155_snapshot_success_and_failures() {
    for (response, status) in [
        (
            Ok(Response {
                status: 200,
                body: b("S1", 0),
            }),
            SnapRecHdr::OK,
        ),
        (
            Ok(Response {
                status: 429,
                body: b"slow".to_vec(),
            }),
            SnapRecHdr::RATE,
        ),
        (
            Ok(Response {
                status: 200,
                body: b"bad".to_vec(),
            }),
            SnapRecHdr::DECODE,
        ),
        (Err(Error::Timeout), SnapRecHdr::TIMEOUT),
        (
            Ok(Response {
                status: 200,
                body: vec![0; 2000],
            }),
            SnapRecHdr::TOO_LARGE,
        ),
    ] {
        let (mut tx, rx) = spsc(8, Backing::Heap).unwrap();
        let (out, mut tap) = byte_spsc(4096, Backing::Heap).unwrap();
        let mut task = SnapFetchTask::<StubProto, Mock>::new(
            SnapCfg {
                max_record_bytes: 1024,
                ..SnapCfg::default()
            },
            SnapDeps {
                rd: rd_stub(),
                reqs: [Some(rx), None],
                out: [Some(out), None],
                clock: clock(),
            },
            (),
        )
        .unwrap();
        task.attach(
            Mock {
                response: Some(response),
                sent: false,
            },
            "public.test".into(),
        )
        .unwrap();
        tx.try_push(SnapReq {
            kind: SnapReq::BBO,
            _pad: 0,
            attempt: 0,
            inst: S1,
            req_id: 17,
            _pad2: 0,
        })
        .unwrap();
        for n in 0..3 {
            task.poll_once(n);
        }
        let (h, tail) = SnapRecHdr::read_from_prefix(tap.peek().unwrap()).unwrap();
        assert_eq!((h.req_id, h.inst, h.status), (17, S1, status));
        if status == SnapRecHdr::OK {
            let e = hft_event::Bbo::read_from_bytes(&tail[h.body_len as usize..]).unwrap();
            assert_eq!(e.h.inst, S1);
            assert_eq!(e.h.conn.0, hft_venue::SNAP_FIRST);
        }
        tap.advance();
        assert!(tap.peek().is_none());
    }
}
