//! CTL-069; CTL-121 (over the E2E rig).
#![allow(
    clippy::type_complexity,
    clippy::disallowed_types,
    clippy::disallowed_methods,
    clippy::disallowed_macros
)] // allow-list: test code, not a hot path

use crate::common::*;
use hft_ctl_proto::{ResultCode, TransportStatus};
use proptest::prelude::*;
use serde_json::{Value, json};

const ISSUERS: &[&str] = &[
    "tg:1001",
    "tg:1002",
    "tg:1003",
    "tg:1004",
    "tg:9999",
    "ops_bot",
    "pipeline",
    "reconciler",
    "cli:alice",
    "cli:tom",
    "cli:vera",
    "cli:systemd",
];
const ACCOUNTS: &[Option<&str>] = &[
    None,
    Some("*"),
    Some("acct-a"),
    Some("acct-b"),
    Some("sim-a"),
    Some("nope"),
];

fn cmds() -> Vec<Value> {
    vec![
        json!({"type":"Kill","level":"Global"}),
        json!({"type":"Kill","level":"Account","hard":true}),
        json!({"type":"Kill","level":"Venue","venue":"bitget-spot","flatten":true}),
        json!({"type":"SetMode","strategy":"s1-main","group":"16777317","mode":"Live"}),
        json!({"type":"SetMode","strategy":"s2-main","group":"*","mode":"CloseOnly"}),
        json!({"type":"HedgeFix","group":"16777317"}),
        json!({"type":"HedgeFix","group":"none"}),
        json!({"type":"SmokeOrder","venue":"bitget-spot","inst":"bitget-spot:BTCUSDT","side":"Buy","ttl_ms":"60001"}),
        json!({"type":"SmokeOrder","venue":"bitget-spot","inst":"bitget-spot:BTCUSDT","side":"Buy"}),
        json!({"type":"SimReset"}),
        json!({"type":"VenueNotice","venue":"binance-usdm","kind":"FundingInterval","new_s":"3601"}),
        json!({"type":"Annotate","kind":"OpsNote","text":"x"}),
        json!({"type":"StrategyCmd","strategy":"s1-main","group":"none","reduce_only":true,"payload_b64":b64(&[1u8; 300])}),
        json!({"type":"InjectWarmup","strategy":"s2-main","local_kind":"3","chunk_no":"0","payload_b64":b64(&[1u8; 64])}),
        json!({"type":"SetParams","strategy":"s1-main","blob_b64":b64(&Spb::s1(42).bytes())}),
        json!({"type":"SetRisk","blob_b64":b64(&risk_blob(0))}),
        json!({"type":"GetSchema","strategy_kind":"probe-taker"}),
        json!({"type":"ReconcileReport","report_id":"1","items":[{"item":"Summary"}]}),
        json!({"type":"Foo"}),
    ]
}

#[derive(Debug, Clone)]
struct Case {
    issuer: usize,
    account: usize,
    cmd: usize,
    group_chat: bool,
    confirmed: bool,
    esv: Option<u64>,
    expired: bool,
    ring_full: bool,
    mailbox_busy: bool,
}

fn case() -> impl Strategy<Value = Case> {
    (
        0..ISSUERS.len(),
        0..ACCOUNTS.len(),
        0..cmds().len(),
        any::<bool>(),
        any::<bool>(),
        proptest::option::of(0u64..3),
        prop::bool::weighted(0.1),
        prop::bool::weighted(0.1),
        prop::bool::weighted(0.2),
    )
        .prop_map(
            |(issuer, account, cmd, group_chat, confirmed, esv, expired, ring_full, mailbox_busy)| Case {
                issuer,
                account,
                cmd,
                group_chat,
                confirmed,
                esv,
                expired,
                ring_full,
                mailbox_busy,
            },
        )
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(500))]
    #[test]
    fn ctl_069_rejections_write_audit_only(cases in proptest::collection::vec(case(), 1..=20)) {
        let mut g = G0::new();
        g.paused = true;
        let all = cmds();
        for c in cases {
            g.take_ring();
            // Leave the Mailbox as earlier requests filled it, or empty it.
            if !c.mailbox_busy {
                while g.mb_take.try_take().is_some() {}
            }
            g.take_ring();
            let free_before = g.core.ring().free;
            if c.ring_full {
                g.core.ring_mut().free = 0;
            }
            let issuer = ISSUERS[c.issuer];
            let mut v = g.req(issuer, ACCOUNTS[c.account], all[c.cmd].clone());
            if c.group_chat && issuer.starts_with("tg:") {
                v["chat_type"] = json!("group");
            }
            if c.confirmed {
                v["confirmed"] = json!(true);
            }
            if let Some(e) = c.esv {
                v["expect_state_version"] = json!(e);
            }
            if c.expired {
                v["issued_at_ns"] = json!(g.utc() - 40_000_000_000);
                v["expires_at_ns"] = json!(g.utc() - 10_000_000_000);
            }
            let uid = uid_of(issuer);
            let r = g.send_raw(uid, &serde_json::to_vec(&v).unwrap());
            let recs = g.take_ring();
            let gateway_reject = match &r {
                None => false,
                Some(r) => !(r.status == TransportStatus::Ok && r.code == Some(ResultCode::Applied)),
            };
            if gateway_reject {
                let r = r.unwrap();
                let wrote = recs.iter().any(|x| !x.is_note());
                prop_assert!(!wrote, "rejected request wrote commands: {:?}", r);
                if c.ring_full {
                    prop_assert!(recs.is_empty());
                } else {
                    let notes = G0::<CtlTestSet>::notes(&recs);
                    prop_assert_eq!(notes.len(), 1);
                    let want = r.code.map_or(0, |c| c as u8);
                    prop_assert_eq!(notes[0].result_code, want);
                }
                if !c.mailbox_busy {
                    prop_assert!(g.mb_take.try_take().is_none());
                }
            }
            g.core.ring_mut().free = free_before.max(262_144);
            g.step();
            g.answer_all();
        }
    }
}

// ------------------------------------------------------------------------------- CTL-121

mod state_free {
    use std::cell::RefCell;
    use std::collections::HashMap;

    use hft_ctl_proto::{ResultCode, TransportStatus};
    use proptest::prelude::*;
    use serde_json::{Value, json};

    use crate::common::b64;
    use crate::e2e::world::{Spb, rd_spec, risk_blob, tf};
    use crate::e2e::{E2e, Opts, uuid};

    /// The legal requests of every core command: (issuer, account, cmd).
    fn requests() -> Vec<(&'static str, Option<&'static str>, Value)> {
        let mut thr = Spb::s1(42);
        thr.values[tf::THR] = 7;
        let mut rd2 = rd_spec();
        rd2.version = 2;
        let tom = "cli:tom";
        let alice = "cli:alice";
        let a = Some("acct-a");
        let all = Some("*");
        let sim = Some("sim-a");
        vec![
            (
                tom,
                a,
                json!({"type":"SetMode","strategy":"s1-main","group":"16777317","mode":"CloseOnly"}),
            ),
            (
                tom,
                a,
                json!({"type":"SetMode","strategy":"s1-main","group":"16777317","mode":"Live"}),
            ),
            (tom, a, json!({"type":"Kill","level":"Account"})),
            (alice, all, json!({"type":"Kill","level":"Global","hard":true})),
            (tom, a, json!({"type":"Kill","level":"Account","flatten":true})),
            (tom, a, json!({"type":"Resume","source":"Kill"})),
            (tom, all, json!({"type":"Drain","cancel_resting":false})),
            (tom, a, json!({"type":"CancelAll"})),
            (tom, a, json!({"type":"HedgeFix","group":"16777317"})),
            (alice, a, json!({"type":"WriteOffResidual","group":"16777317"})),
            (
                alice,
                a,
                json!({"type":"SmokeOrder","venue":"bitget-spot","inst":"bitget-spot:BTCUSDT","side":"Buy","ttl_ms":"30000"}),
            ),
            (tom, a, json!({"type":"SetBalanceBaseline","venue":"bitget-spot"})),
            (
                tom,
                a,
                json!({"type":"AdoptExchange","venue":"binance-usdm","report_id":"7","item":"Position","inst":"binance-usdm:BTCUSDT"}),
            ),
            (
                tom,
                all,
                json!({"type":"VenueNotice","venue":"bitget-spot","inst":"bitget-spot:BTCUSDT","kind":"Delisting","at_ns":"1790086400000000000"}),
            ),
            (
                tom,
                all,
                json!({"type":"VenueNotice","venue":"binance-usdm","kind":"FundingInterval","new_s":"14400"}),
            ),
            (tom, sim, json!({"type":"SimReset"})),
            (
                tom,
                sim,
                json!({"type":"SimTransfer","from_venue":"binance-usdm","to_venue":"bitget-spot","asset":"USDT","amount":"100"}),
            ),
            (
                tom,
                a,
                json!({"type":"RecordTransfer","from":"binance-usdm","to":"bitget-spot","asset":"USDT","amount":"100","ext_ref":"TX1"}),
            ),
            (tom, all, json!({"type":"Annotate","kind":"Incident","text":"y"})),
            (
                tom,
                a,
                json!({"type":"SetParams","strategy":"s1-main","blob_b64":b64(&thr.bytes())}),
            ),
            (
                alice,
                a,
                json!({"type":"SetRisk","blob_b64":b64(&risk_blob(8, 2_000, 0))}),
            ),
            (
                tom,
                a,
                json!({"type":"StrategyCmd","strategy":"s1-main","group":"16777317","reduce_only":true,
                       "payload_b64":b64(&crate::e2e::strategy_channel_op2())}),
            ),
            (
                "pipeline",
                None,
                json!({"type":"InjectWarmup","strategy":"s1-main","local_kind":"3","chunk_no":"0","payload_b64":b64(&[1u8; 64])}),
            ),
            (
                alice,
                all,
                json!({"type":"ReplaceRefdata","blob_b64":b64(&rd2.build()),"version":"2"}),
            ),
        ]
    }

    /// What the gateway did with a request: written (the sequencer answered with a `CmdResult`),
    /// or refused with a status and code (no `CmdResult`).
    #[derive(Clone, Debug, PartialEq, Eq)]
    enum Disp {
        Written,
        Refused(TransportStatus, Option<ResultCode>),
    }

    /// Engine states: 0 = S0, 1 = no start gate released, 2 = Draining, 3 = HARD `Kill{Global}`,
    /// 4 = Kill cap on acct-a, 5 = Recon cap on (acct-a, bitget-spot); then `bumps` SetMode
    /// pairs on sim-a (`ctl_state_version` + 2 each).
    fn rig(state: u8, bumps: u8) -> E2e {
        let mut r = if state == 1 {
            E2e::with(Opts {
                release: Vec::new(),
                ..Opts::default()
            })
        } else {
            crate::e2e::e2e()
        };
        match state {
            2 => {
                r.hold_drain();
                r.send(
                    "cli:tom",
                    Some("*"),
                    json!({"type":"Drain","cancel_resting":false}),
                );
            }
            3 => {
                r.send(
                    "cli:alice",
                    Some("*"),
                    json!({"type":"Kill","level":"Global","hard":true}),
                );
            }
            4 => {
                r.send(
                    "cli:tom",
                    Some("acct-a"),
                    json!({"type":"Kill","level":"Account"}),
                );
            }
            5 => {
                for id in [11u64, 12] {
                    let items = json!([
                        {"venue":"bitget-spot","account":"acct-a","item":"Balance","asset":"BTC",
                         "engine_qty":"0","exch_qty":"0.01","diff":"500","severity":"Warn"},
                        {"venue":"bitget-spot","account":"acct-a","item":"Summary"}
                    ]);
                    r.report(id, items);
                }
            }
            _ => {}
        }
        for _ in 0..bumps {
            r.send(
                "cli:tom",
                Some("sim-a"),
                json!({"type":"SetMode","strategy":"s9-sim","mode":"CloseOnly"}),
            );
            r.send(
                "cli:tom",
                Some("sim-a"),
                json!({"type":"SetMode","strategy":"s9-sim","mode":"Shadow"}),
            );
        }
        r
    }

    fn disp(r: &mut E2e, k: usize) -> Disp {
        let (issuer, account, cmd) = requests().swap_remove(k);
        let v = r.req(issuer, account, cmd);
        let id = uuid(&v);
        let x = r.send_v(v);
        if r.jres(id).is_some() {
            Disp::Written
        } else {
            Disp::Refused(x.status, x.code)
        }
    }

    thread_local! {
        static S0: RefCell<HashMap<usize, Disp>> = RefCell::new(HashMap::new());
    }

    /// The disposition of request `k` in S0.
    fn reference(k: usize) -> Disp {
        if let Some(d) = S0.with(|m| m.borrow().get(&k).cloned()) {
            return d;
        }
        let d = disp(&mut rig(0, 0), k);
        S0.with(|m| m.borrow_mut().insert(k, d.clone()));
        d
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(200))]
        #[test]
        fn ctl_121_gateway_state_free(state in 0u8..6, bumps in 0u8..3, k in 0..24usize) {
            let want = reference(k);
            let mut r = rig(state, bumps);
            let got = disp(&mut r, k);
            prop_assert_eq!(&got, &want, "request {} in state {}/{}", k, state, bumps);
            // A refusal is the gateway's own: never a state code.
            if let Disp::Refused(_, Some(c)) = got {
                prop_assert!(!matches!(
                    c,
                    ResultCode::NeedsConfirm | ResultCode::ConfirmInvalid | ResultCode::PreconditionFailed
                ));
            }
        }
    }
}
