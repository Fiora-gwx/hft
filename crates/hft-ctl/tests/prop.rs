//! CTL-069 (CTL-121 needs the real sequencer: see TESTS.md).
#![allow(
    clippy::type_complexity,
    clippy::disallowed_types,
    clippy::disallowed_methods,
    clippy::disallowed_macros
)] // allow-list: test code, not a hot path

mod common;

use common::*;
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
