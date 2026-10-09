//! CTL-130–CTL-136 (Linux: real socket, real `SO_PEERCRED`).
#![allow(
    clippy::type_complexity,
    clippy::disallowed_types,
    clippy::disallowed_methods,
    clippy::disallowed_macros
)] // allow-list: test code, not a hot path

mod common;

#[cfg(target_os = "linux")]
mod linux {
    use std::time::Duration;

    use hft_ctl::UidKind;
    use hft_ctl::client::{ClientError, CtlClient};
    use hft_ctl_proto::{AccountSel, ChatType, CmdBody, ResultCode, TransportStatus};
    use hft_event::ControlCmd;
    use hft_rt::PollTask;

    use super::common::sock::{Sock, principals_with_self};
    use super::common::*;

    /// Runs `f` on a client thread while this thread polls the gateway.
    fn drive<R: Send>(s: &mut Sock, f: impl FnOnce() -> R + Send) -> R {
        std::thread::scope(|sc| {
            let h = sc.spawn(f);
            while !h.is_finished() {
                s.poll();
                std::thread::sleep(Duration::from_micros(200));
            }
            h.join().unwrap()
        })
    }

    fn sock(tag: &str, kind: UidKind, edit: impl FnMut(&mut hft_ctl::GatewayConfig)) -> Sock {
        let mut edit = edit;
        Sock::new(tag, move |c| {
            c.principals = principals_with_self(kind.clone());
            edit(c);
        })
    }

    fn tom() -> UidKind {
        UidKind::Cli { login: "tom".into() }
    }

    fn hedge(c: &CtlClient) -> hft_ctl_proto::CmdRequest {
        c.request(
            AccountSel::Name("acct-a".into()),
            CmdBody::HedgeFix {
                group: Some("16777317".into()),
                account: None,
            },
        )
    }

    #[test]
    fn ctl_130_send() {
        let mut s = sock("130", tom(), |_| {});
        let c = CtlClient::new(&s.socket, "cli:tom");
        let r = drive(&mut s, || c.send(&hedge(&c), Duration::from_secs(1))).unwrap();
        assert_eq!(
            (r.status, r.code),
            (TransportStatus::Ok, Some(ResultCode::Applied))
        );
        assert_eq!(s.cmds().len(), 1);
        // The gateway stops polling: the client times out.
        let t = std::time::Instant::now();
        let e = c.send(&hedge(&c), Duration::from_millis(200)).unwrap_err();
        assert!(matches!(e, ClientError::Timeout), "{e:?}");
        assert!(t.elapsed() >= Duration::from_millis(200));
    }

    #[test]
    fn ctl_131_preview_confirm() {
        let mut s = sock("131", tom(), |_| {});
        let c = CtlClient::new(&s.socket, "cli:tom");
        let req = c.request(
            AccountSel::Name("acct-a".into()),
            CmdBody::SetMode {
                strategy: Some("s1-main".into()),
                group: Some("16777317".into()),
                account: None,
                mode: hft_types::RunMode::Live,
            },
        );
        let p = drive(&mut s, || c.preview(&req)).unwrap();
        assert_eq!(p.ctl_state_version, Some(33));
        assert!(p.preview.is_some());
        let cf = drive(&mut s, || c.confirm(&req, Some(33))).unwrap();
        let cmds = s.cmds();
        let (h1, c1) = cmds[0].cmd();
        let (h2, c2) = cmds[1].cmd();
        assert_eq!((h1.hflags, h1.confirmed), (1, 0));
        assert_ne!(h1.cmd_id, h2.cmd_id);
        assert_eq!(c1, c2);
        assert_eq!((h2.confirmed, h2.expect_state_version), (1, 33));
        assert_eq!(p.cmd_hash, cf.cmd_hash);
        // Viewer emergency stop: confirm without a state version.
        let mut s = sock("131v", UidKind::OpsBot, |_| {});
        let v = CtlClient::new(&s.socket, "tg:1001").with_chat_type(ChatType::Private);
        let kill = v.request(
            AccountSel::Name("acct-a".into()),
            CmdBody::Kill {
                level: hft_types::KillLevel::Account,
                venue: None,
                account: None,
                cancel_resting: None,
                flatten: None,
                hard: None,
            },
        );
        drive(&mut s, || v.preview(&kill)).unwrap();
        drive(&mut s, || v.confirm(&kill, None)).unwrap();
        let (h, _) = s.cmds()[1].cmd();
        assert_eq!((h.confirmed, h.expect_state_version), (1, 0));
    }

    #[test]
    fn ctl_132_result_resends() {
        let mut s = sock("132", tom(), |c| c.result_wait_ms = 200);
        s.paused = true;
        let c = CtlClient::new(&s.socket, "cli:tom");
        let req = hedge(&c);
        let r = drive(&mut s, || c.send(&req, Duration::from_secs(5))).unwrap();
        assert_eq!(r.status, TransportStatus::Accepted);
        let r = drive(&mut s, || c.result(req.cmd_id)).unwrap();
        assert_eq!(r.status, TransportStatus::Accepted, "CTL-132.1");
        assert_eq!(s.cmds().len(), 1, "CTL-132.1");
        s.paused = false;
        s.answer_all();
        s.poll();
        let r = drive(&mut s, || c.result(req.cmd_id)).unwrap();
        assert_eq!(
            (r.status, r.code),
            (TransportStatus::Ok, Some(ResultCode::Applied)),
            "CTL-132.2"
        );
        assert_eq!(s.cmds().len(), 1, "CTL-132.2");
        let before = s.gw.conn_count();
        let e = c.result(hft_ctl_proto::Uuid::from_u128(77)).unwrap_err();
        assert!(matches!(e, ClientError::UnknownCmdId), "CTL-132.3");
        s.gw.poll_once(s.now());
        assert_eq!(s.gw.conn_count(), before, "CTL-132.3: no connection");
    }

    #[test]
    fn ctl_133_get_schema() {
        let mut s = sock("133", UidKind::Cli { login: "vera".into() }, |_| {});
        let c = CtlClient::new(&s.socket, "cli:vera");
        let v = drive(&mut s, || c.get_schema("probe-taker")).unwrap();
        let want: serde_json::Value = serde_json::from_str(
            &<CtlTestSet as hft_strategy_sdk::StrategySet>::schema_json(0xFF01).unwrap(),
        )
        .unwrap();
        assert_eq!(v, want);
        match drive(&mut s, || c.get_schema("nope")) {
            Err(ClientError::Rejected(r)) => {
                assert_eq!(r.code, Some(ResultCode::InvalidArgs));
                assert_eq!(r.detail.unwrap().name, "UnknownName");
            }
            o => panic!("{o:?}"),
        }
    }

    fn chunk(k: usize) -> Vec<u8> {
        let n = 1 + (k * 7) % 4096;
        bytes_of(n, k as u64)
    }

    fn bytes_of(n: usize, seed: u64) -> Vec<u8> {
        let mut x = seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1;
        (0..n)
            .map(|_| {
                x ^= x << 13;
                x ^= x >> 7;
                x ^= x << 17;
                x as u8
            })
            .collect()
    }

    #[test]
    fn ctl_134_inject_warmup_chunks() {
        let mut s = sock("134", UidKind::Pipeline, |_| {});
        let c = CtlClient::new(&s.socket, "pipeline");
        let chunks: Vec<Vec<u8>> = (0..600).map(chunk).collect();
        let refs: Vec<&[u8]> = chunks.iter().map(Vec::as_slice).collect();
        let n = std::thread::scope(|sc| {
            let h = sc.spawn(|| c.inject_warmup("s2-main", 3, &refs));
            let mut busy_done = 0;
            while !h.is_finished() {
                // Chunks 100 and 101 meet a full ring once each.
                let written = s.cmds().len();
                if (written == 100 || written == 101) && busy_done < written - 99 {
                    let before = s.gw.core().ring().refused;
                    s.gw.core_mut().ring_mut().free = 0;
                    while s.gw.core().ring().refused == before && !h.is_finished() {
                        s.gw.core_mut().ring_mut().free = 0;
                        s.poll();
                        std::thread::sleep(Duration::from_micros(200));
                    }
                    s.gw.core_mut().ring_mut().free = 262_144;
                    busy_done += 1;
                }
                s.poll();
                std::thread::sleep(Duration::from_micros(200));
            }
            h.join().unwrap()
        })
        .unwrap();
        assert_eq!(n, 600);
        let cmds = s.cmds();
        assert_eq!(cmds.len(), 600);
        for (k, r) in cmds.iter().enumerate() {
            match r.cmd().1 {
                ControlCmd::InjectWarmup {
                    chunk_no,
                    last,
                    local_kind,
                    payload,
                    ..
                } => {
                    assert_eq!((chunk_no as usize, local_kind), (k, 3));
                    assert_eq!(last == 1, k == 599);
                    assert_eq!(payload, &chunks[k][..]);
                }
                c => panic!("{c:?}"),
            }
        }
    }

    #[test]
    fn ctl_135_chunk_too_long() {
        let mut s = sock("135", UidKind::Pipeline, |_| {});
        let c = CtlClient::new(&s.socket, "pipeline");
        let a = vec![1u8; 100];
        let b = vec![1u8; 4097];
        let e = c.inject_warmup("s2-main", 3, &[&a, &b]).unwrap_err();
        assert!(matches!(e, ClientError::ChunkTooLong), "CTL-135.1");
        s.gw.poll_once(s.now());
        assert_eq!((s.gw.conn_count(), s.cmds().len()), (0, 0), "CTL-135.1");
        let x = vec![2u8; 4096];
        let y = vec![3u8; 4095];
        assert_eq!(
            drive(&mut s, || c.inject_warmup("s2-main", 3, &[&x])).unwrap(),
            1,
            "CTL-135.2"
        );
        assert_eq!(
            drive(&mut s, || c.inject_warmup("s2-main", 3, &[&y, &x])).unwrap(),
            2,
            "CTL-135.3"
        );
    }

    #[test]
    fn ctl_136_stop_on_rejected_chunk() {
        let mut s = sock("136", UidKind::Pipeline, |_| {});
        s.script.push((
            |c| matches!(c, ControlCmd::InjectWarmup { chunk_no: 3, .. }),
            Outcome {
                status: 20,
                detail: 119,
                ..Outcome::APPLIED
            },
        ));
        let c = CtlClient::new(&s.socket, "pipeline");
        let chunks: Vec<Vec<u8>> = (0..10).map(|k| vec![k as u8; 16]).collect();
        let refs: Vec<&[u8]> = chunks.iter().map(Vec::as_slice).collect();
        assert_eq!(drive(&mut s, || c.inject_warmup("s2-main", 3, &refs)).unwrap(), 3);
        let nos: Vec<u32> = s
            .cmds()
            .iter()
            .map(|r| match r.cmd().1 {
                ControlCmd::InjectWarmup { chunk_no, .. } => chunk_no,
                _ => u32::MAX,
            })
            .collect();
        assert_eq!(nos, [0, 1, 2, 3]);
    }
}
