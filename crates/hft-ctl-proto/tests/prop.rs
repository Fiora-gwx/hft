//! CTP-055, CTP-057, CTP-058, CTP-094, CTP-095, CTP-175.
#![allow(
    clippy::type_complexity,
    clippy::disallowed_types,
    clippy::disallowed_methods,
    clippy::disallowed_macros
)] // allow-list: test code, not a hot path

mod common;

use common::*;
use hft_ctl_proto::{
    CmdResponse, EngineInfo, Normalized, ResultCode, TransportStatus, Uuid, allowed, class_of,
    needs_confirm_static, normalize, parse_request,
};
use hft_event::ControlCmd;
use proptest::prelude::*;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

fn decoded(n: &Normalized) -> (hft_event::CmdHdr, ControlCmd<'_>) {
    ControlCmd::decode(&n.record).unwrap()
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(200))]
    #[test]
    fn ctp_055_blob_parts_roundtrip(which in 0u8..3, len in 16usize..=786_432, seed in any::<u64>()) {
        let (acct, cmd, blob) = match which {
            0 => {
                let len = len.max(128);
                let mut b = spb1(1, 1, 42, 0);
                b.extend(bytes(len - b.len(), seed));
                let n = b.len() - 128;
                // body_len must cover the tail (container-valid header).
                b[32..36].copy_from_slice(&(n as u32).to_le_bytes());
                (Some("acct-a"), json!({"type":"SetParams","strategy":"s1-main","blob_b64":b64(&b)}), b)
            }
            1 => {
                let b = bytes(len, seed);
                (Some("acct-a"), json!({"type":"SetRisk","blob_b64":b64(&b)}), b)
            }
            _ => {
                let b = bytes(len, seed);
                (Some("acct-a"), json!({"type":"ReplaceRefdata","blob_b64":b64(&b),"version":"8"}), b)
            }
        };
        let n = ok(acct, cmd);
        let cat: Vec<u8> = n.blobs.iter().flat_map(|p| p.content().to_vec()).collect();
        prop_assert_eq!(&cat, &blob);
        prop_assert!(n.blobs.len() <= 64);
        let r = match decoded(&n).1 {
            ControlCmd::SetParams { blob, .. } | ControlCmd::SetRisk { blob } | ControlCmd::ReplaceRefdata { blob } => blob,
            c => panic!("{c:?}"),
        };
        prop_assert_eq!(r.len as usize, blob.len());
        prop_assert_eq!(r.sha256, <[u8; 32]>::from(Sha256::digest(&blob)));
        prop_assert!(n.payload().len() <= 64);
    }
}

/// Field values the request generator draws from: legal names, `*`, unknown names, out-of-range numbers.
const ACCOUNTS: &[Option<&str>] = &[
    None,
    Some("*"),
    Some("acct-a"),
    Some("acct-b"),
    Some("sim-a"),
    Some("nope"),
    Some("2"),
];
const MUTATIONS: &[(&str, &str)] = &[
    ("strategy", "s2-main"),
    ("strategy", "nope"),
    ("strategy", "*"),
    ("group", "*"),
    ("group", "none"),
    ("group", "33554533"),
    ("group", "4294967296"),
    ("venue", "kraken"),
    ("venue", "binance-spot"),
    ("inst", "binance-usdm:NOPE"),
    ("asset", "XYZ"),
    ("amount", "0.000000001"),
    ("ttl_ms", "60001"),
    ("local_kind", "65536"),
    ("chunk_no", "4294967296"),
    ("payload_b64", "%%"),
    ("blob_b64", "!!"),
];

fn gen_request() -> impl Strategy<Value = Value> {
    let n = core_cmds().len();
    (0..n, 0..ACCOUNTS.len(), proptest::option::of(0..MUTATIONS.len())).prop_map(|(c, a, m)| {
        let (_, _, mut cmd) = core_cmds().swap_remove(c);
        if let Some(m) = m {
            let (k, v) = MUTATIONS[m];
            if cmd.get(k).is_some() {
                cmd[k] = json!(v);
            }
        }
        e0(ACCOUNTS[a], cmd)
    })
}

fn outcome(v: &Value) -> Option<Result<(Vec<u8>, [u8; 32], Vec<Vec<u8>>, Vec<Vec<u8>>), u32>> {
    let r = parse_request(&serde_json::to_vec(v).unwrap()).ok()?;
    Some(
        normalize(&r, rd())
            .map(|n| {
                let parts = n.blobs.iter().map(|p| p.body.clone()).collect();
                let recs = n.ring_records().into_iter().map(|(_, _, b)| b).collect();
                (n.record, n.cmd_hash, parts, recs)
            })
            .map_err(|e| e.raw()),
    )
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(1_000))]
    #[test]
    fn ctp_057_normalize_deterministic(a in gen_request(), b in gen_request()) {
        let first = outcome(&a);
        let _ = outcome(&b);
        // SAFETY: test-only variable that no code reads; tests in this binary do not read env.
        unsafe { std::env::set_var("HFT_CTP_057", format!("{first:?}").len().to_string()) };
        let second = outcome(&a);
        prop_assert_eq!(first, second);
    }

    #[test]
    fn ctp_058_hdr_account_matches_payload(v in gen_request()) {
        let Ok(r) = parse_request(&serde_json::to_vec(&v).unwrap()) else { return Ok(()) };
        let Ok(n) = normalize(&r, rd()) else { return Ok(()) };
        if n.record.is_empty() {
            return Ok(());
        }
        let (h, c) = decoded(&n);
        let a = h.account;
        match c {
            ControlCmd::SetMode { scope, .. }
            | ControlCmd::SetParams { scope, .. }
            | ControlCmd::Kill { scope, .. }
            | ControlCmd::Resume { scope, .. }
            | ControlCmd::HedgeFix { scope }
            | ControlCmd::CancelAll { scope } => prop_assert_eq!(scope.account, a),
            ControlCmd::AdoptExchange { account, .. }
            | ControlCmd::SimReset { account }
            | ControlCmd::SimTransfer { account, .. }
            | ControlCmd::SetBalanceBaseline { account, .. }
            | ControlCmd::RecordTransfer { account, .. }
            | ControlCmd::SmokeOrder { account, .. }
            | ControlCmd::WriteOffResidual { account, .. }
            | ControlCmd::SimFault { account, .. } => prop_assert_eq!(account, a),
            ControlCmd::InjectWarmup { .. } => prop_assert_eq!(a.0, 0),
            _ => {}
        }
    }
}

fn perm_inputs() -> Vec<hft_ctl_proto::CmdBody> {
    core_cmds().into_iter().map(|(_, _, c)| body(c)).collect()
}

type PermOut = (bool, hft_ctl_proto::CmdClass, hft_ctl_proto::ConfirmRule);

fn perm_out(w: &Who, b: &hft_ctl_proto::CmdBody) -> PermOut {
    let role = hft_ctl_proto::effective_role(w.roles, w.principal, w.chat, b);
    (
        allowed(w.roles, w.principal, w.chat, b).is_ok(),
        class_of(b),
        needs_confirm_static(role, w.principal, b),
    )
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(500))]
    #[test]
    fn ctp_094_perms_stateless(calls in proptest::collection::vec((0usize..13, any::<prop::sample::Index>()), 1..=200)) {
        let dom = perm_inputs();
        let reference: Vec<Vec<PermOut>> = WHO.iter().map(|w| dom.iter().map(|b| perm_out(w, b)).collect()).collect();
        for (w, ix) in calls {
            let i = ix.index(dom.len());
            prop_assert_eq!(perm_out(&WHO[w], &dom[i]), reference[w][i]);
        }
        let h = std::thread::spawn(move || perm_out(&WHO[0], &perm_inputs()[1]));
        prop_assert_eq!(h.join().unwrap(), reference[0][1]);
    }

    #[test]
    fn ctp_095_sim_account_same_perms(w in 0usize..13, ix in any::<prop::sample::Index>()) {
        let cmds: Vec<Value> = core_cmds().into_iter().filter(|(_, a, _)| a.is_some()).map(|(_, _, c)| c).collect();
        let mut c = cmds[ix.index(cmds.len())].clone();
        if c.get("type") == Some(&json!("ReconcileReport")) || c.get("type") == Some(&json!("GetSchema")) {
            return Ok(());
        }
        c["account"] = json!("acct-a");
        let live = body(c.clone());
        c["account"] = json!("sim-a");
        let sim = body(c);
        prop_assert_eq!(perm_out(&WHO[w], &live), perm_out(&WHO[w], &sim));
    }
}

fn json_value() -> impl Strategy<Value = Value> {
    let leaf = prop_oneof![
        Just(Value::Null),
        any::<bool>().prop_map(Value::Bool),
        any::<i64>().prop_map(|v| json!(v)),
        any::<u64>().prop_map(|v| json!(v)),
        ".{0,64}".prop_map(Value::String),
    ];
    leaf.prop_recursive(6, 256, 16, |inner| {
        prop_oneof![
            proptest::collection::vec(inner.clone(), 0..16).prop_map(Value::Array),
            proptest::collection::btree_map(".{0,8}", inner, 0..16)
                .prop_map(|m| Value::Object(m.into_iter().collect())),
        ]
    })
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(500))]
    #[test]
    fn ctp_175_schema_body_passthrough(v in json_value()) {
        let mut r = CmdResponse::new(Uuid::nil(), TransportStatus::Ok, Some(ResultCode::Applied), EngineInfo::default());
        r.body = hft_ctl_proto::response::raw_of(&v);
        let bytes = r.to_json();
        let back = CmdResponse::from_json(&bytes).unwrap();
        let got: Value = serde_json::from_str(back.body.get()).unwrap();
        prop_assert_eq!(&got, &v);
        let want = serde_json::to_vec(&v).unwrap();
        prop_assert_eq!(back.body.get().as_bytes(), &want[..]);
        let text = String::from_utf8(bytes).unwrap();
        let needle = format!("\"body\":{}", String::from_utf8(want).unwrap());
        prop_assert!(text.contains(&needle), "body not passed through");
    }
}
