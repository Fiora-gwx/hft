//! CTP-140–CTP-162: request → record/cmd_hash goldens, response goldens and frame goldens under
#![allow(
    clippy::type_complexity,
    clippy::disallowed_types,
    clippy::disallowed_methods,
    clippy::disallowed_macros
)] // allow-list: test code, not a hot path
//! `fixtures/ctl/`. `HFT_REGEN_GOLDEN=1` rewrites them (and `fixtures/refdata/ctl-p0.blob`).

mod common;

use std::fs;
use std::path::{Path, PathBuf};

use common::*;
use hft_ctl_proto::response::{raw_of, render_preview};
use hft_ctl_proto::{
    CmdResponse, EngineInfo, Hex32, ResultCode, TransportStatus, Uuid, encode_frame, normalize,
    parse_request, render_detail,
};
use hft_event::CmdPreview;
use hft_types::{AccountId, InstId, Qty, VenueId};
use serde_json::{Value, json};
use zerocopy::IntoBytes;

fn regen() -> bool {
    std::env::var_os("HFT_REGEN_GOLDEN").is_some()
}

fn dir() -> PathBuf {
    fixtures().join("ctl")
}

fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

/// Writes (regen) or compares one golden file.
fn golden(path: &Path, bytes: &[u8]) {
    if regen() {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, bytes).unwrap();
    }
    let want = fs::read(path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    assert_eq!(
        String::from_utf8_lossy(bytes),
        String::from_utf8_lossy(&want),
        "{}",
        path.display()
    );
}

/// One request golden: the canonical request JSON, then its record and cmd_hash.
fn case(name: &str, account: Option<&str>, cmd: Value) {
    case_with(name, e0(account, cmd));
}

fn case_with(name: &str, v: Value) {
    let canon = parse(&v).to_json();
    let req_path = dir().join(format!("{name}.request.json"));
    golden(&req_path, &canon);
    // The checked-in file parses back to the same canonical bytes.
    let file = fs::read(&req_path).unwrap();
    let r = parse_request(&file).unwrap();
    assert_eq!(r.to_json(), file, "{name}: canonical form");
    let n = normalize(&r, rd()).unwrap_or_else(|e| panic!("{name}: {e:?}"));
    if n.record.is_empty() {
        let lines: String = n.reports.iter().map(|x| hex(x.as_bytes()) + "\n").collect();
        golden(&dir().join(format!("{name}.record.hex")), lines.as_bytes());
        assert!(
            !dir().join(format!("{name}.cmd_hash.hex")).exists(),
            "{name}: no cmd_hash"
        );
    } else {
        golden(
            &dir().join(format!("{name}.record.hex")),
            (hex(&n.record) + "\n").as_bytes(),
        );
        golden(
            &dir().join(format!("{name}.cmd_hash.hex")),
            (hex(&n.cmd_hash) + "\n").as_bytes(),
        );
    }
}

#[test]
fn ctp_140_golden_set_mode() {
    golden(&fixtures().join("refdata/ctl-p0.blob"), &p0::p0_blob());
    case(
        "set_mode_live_group",
        Some("acct-a"),
        json!({"type":"SetMode","strategy":"s1-main","group":"16777317","mode":"Live"}),
    );
    case(
        "set_mode_closeonly_all",
        Some("*"),
        json!({"type":"SetMode","strategy":"*","group":"*","mode":"CloseOnly"}),
    );
    case(
        "set_mode_shadow_sim",
        Some("sim-a"),
        json!({"type":"SetMode","strategy":"s9-sim","group":"50331749","mode":"Shadow"}),
    );
}

#[test]
fn ctp_141_golden_set_params() {
    let b = spb1_b64(1, 1, 42, 16);
    case(
        "set_params_base",
        Some("acct-a"),
        json!({"type":"SetParams","strategy":"s1-main","blob_b64":b}),
    );
    case(
        "set_params_base_version",
        Some("acct-a"),
        json!({"type":"SetParams","strategy":"s1-main","blob_b64":b,"base_version":"41"}),
    );
}

#[test]
fn ctp_142_golden_set_risk() {
    let r = b64(&bytes(2_048, 42));
    case(
        "set_risk_account",
        Some("acct-a"),
        json!({"type":"SetRisk","blob_b64":r}),
    );
    case("set_risk_all", Some("*"), json!({"type":"SetRisk","blob_b64":r}));
}

#[test]
fn ctp_143_golden_kill() {
    case(
        "kill_global_default",
        None,
        json!({"type":"Kill","level":"Global"}),
    );
    case(
        "kill_account_default",
        Some("acct-a"),
        json!({"type":"Kill","level":"Account"}),
    );
    case(
        "kill_venue_account",
        Some("acct-a"),
        json!({"type":"Kill","level":"Venue","venue":"bitget-spot"}),
    );
    case(
        "kill_account_hard",
        Some("acct-a"),
        json!({"type":"Kill","level":"Account","hard":true}),
    );
    case(
        "kill_global_hard",
        None,
        json!({"type":"Kill","level":"Global","hard":true}),
    );
    case(
        "kill_account_flatten",
        Some("acct-a"),
        json!({"type":"Kill","level":"Account","flatten":true}),
    );
    case(
        "kill_account_keep_resting",
        Some("acct-a"),
        json!({"type":"Kill","level":"Account","cancel_resting":false}),
    );
}

#[test]
fn ctp_144_golden_resume() {
    case(
        "resume_kill_account",
        Some("acct-a"),
        json!({"type":"Resume","source":"Kill"}),
    );
    case(
        "resume_kill_all",
        Some("*"),
        json!({"type":"Resume","source":"Kill"}),
    );
    case(
        "resume_recon_venue",
        Some("acct-a"),
        json!({"type":"Resume","source":"Recon","venue":"bitget-spot"}),
    );
    case(
        "resume_halted_group",
        Some("acct-a"),
        json!({"type":"Resume","source":"Halted","strategy":"s1-main","group":"16777317"}),
    );
    case(
        "resume_broken_group",
        Some("acct-a"),
        json!({"type":"Resume","source":"Broken","group":"16777317"}),
    );
    case(
        "resume_delisting_inst",
        Some("*"),
        json!({"type":"Resume","source":"Delisting","venue":"binance-spot","inst":"binance-spot:BTCUSDT"}),
    );
}

#[test]
fn ctp_145_golden_cancel_all() {
    case(
        "cancel_all_account_venue_inst",
        Some("acct-a"),
        json!({"type":"CancelAll","venue":"bitget-spot","inst":"bitget-spot:BTCUSDT"}),
    );
    case("cancel_all_all", Some("*"), json!({"type":"CancelAll"}));
}

#[test]
fn ctp_146_golden_drain() {
    case("drain_default", None, json!({"type":"Drain"}));
    case(
        "drain_keep_resting",
        None,
        json!({"type":"Drain","cancel_resting":false}),
    );
}

#[test]
fn ctp_147_golden_hedge_fix() {
    case(
        "hedge_fix_group",
        Some("acct-a"),
        json!({"type":"HedgeFix","group":"16777317"}),
    );
}

#[test]
fn ctp_148_golden_write_off_residual() {
    case(
        "write_off_residual_group",
        Some("acct-a"),
        json!({"type":"WriteOffResidual","group":"16777317"}),
    );
}

#[test]
fn ctp_149_golden_smoke_order() {
    let so = |extra: Value| {
        let mut c =
            json!({"type":"SmokeOrder","venue":"bitget-spot","inst":"bitget-spot:BTCUSDT","side":"Buy"});
        for (k, v) in extra.as_object().unwrap() {
            c[k] = v.clone();
        }
        c
    };
    case("smoke_order_ttl_default", Some("acct-a"), so(json!({})));
    case(
        "smoke_order_ttl_max",
        Some("acct-a"),
        so(json!({"ttl_ms":"60000"})),
    );
    case(
        "smoke_order_sell",
        Some("acct-a"),
        so(json!({"side":"Sell","ttl_ms":"10000"})),
    );
}

#[test]
fn ctp_150_golden_adopt_exchange() {
    case(
        "adopt_exchange_position",
        Some("acct-a"),
        json!({"type":"AdoptExchange","venue":"binance-usdm","report_id":"7","item":"Position","inst":"binance-usdm:BTCUSDT","entry_px":"65000.5"}),
    );
    case(
        "adopt_exchange_balance_group",
        Some("acct-a"),
        json!({"type":"AdoptExchange","venue":"bitget-spot","report_id":"7","item":"Balance","asset":"BTC","group":"16777317"}),
    );
    case(
        "adopt_exchange_balance_unassigned",
        Some("acct-a"),
        json!({"type":"AdoptExchange","venue":"bitget-spot","report_id":"7","item":"Balance","asset":"BTC"}),
    );
}

#[test]
fn ctp_151_golden_set_balance_baseline() {
    case(
        "set_balance_baseline_venue",
        Some("acct-a"),
        json!({"type":"SetBalanceBaseline","venue":"bitget-spot"}),
    );
}

#[test]
fn ctp_152_golden_venue_notice() {
    case(
        "venue_notice_delisting_inst",
        None,
        json!({"type":"VenueNotice","venue":"binance-spot","inst":"binance-spot:BTCUSDT","kind":"Delisting","at_ns":"1790100000000000000"}),
    );
    case(
        "venue_notice_delisting_venue",
        None,
        json!({"type":"VenueNotice","venue":"binance-spot","kind":"Delisting","at_ns":"1790100000000000000"}),
    );
    case(
        "venue_notice_funding_interval",
        Some("*"),
        json!({"type":"VenueNotice","venue":"binance-usdm","kind":"FundingInterval","new_s":"14400"}),
    );
    case(
        "venue_notice_maintenance",
        Some("*"),
        json!({"type":"VenueNotice","venue":"binance-usdm","kind":"Maintenance","from_ns":"1790100000000000000","to_ns":"1790103600000000000"}),
    );
}

#[test]
fn ctp_153_golden_sim_reset() {
    case("sim_reset_sim_account", Some("sim-a"), json!({"type":"SimReset"}));
}

#[test]
fn ctp_154_golden_sim_transfer() {
    case(
        "sim_transfer_btc",
        Some("sim-a"),
        json!({"type":"SimTransfer","from_venue":"binance-usdm","to_venue":"bitget-spot","asset":"BTC","amount":"0.5"}),
    );
}

#[test]
fn ctp_155_golden_record_transfer() {
    case(
        "record_transfer_usdt",
        Some("acct-a"),
        json!({"type":"RecordTransfer","from":"binance-usdm","to":"bitget-spot","asset":"USDT","amount":"100","ext_ref":"TX123"}),
    );
}

#[test]
fn ctp_156_golden_annotate() {
    case(
        "annotate_ops_note",
        Some("*"),
        json!({"type":"Annotate","kind":"OpsNote","text":"hi"}),
    );
    case(
        "annotate_local_write_digest",
        Some("*"),
        json!({"type":"Annotate","kind":"LocalWrite","text":"x","digest":"ab".repeat(32)}),
    );
    case(
        "annotate_incident",
        Some("acct-a"),
        json!({"type":"Annotate","kind":"Incident","text":"交易所维护,已手工撤单"}),
    );
}

#[test]
fn ctp_157_golden_replace_refdata() {
    case(
        "replace_refdata_v8",
        Some("acct-a"),
        json!({"type":"ReplaceRefdata","blob_b64":b64(&bytes(30_000, 50)),"version":"8"}),
    );
}

#[test]
fn ctp_158_golden_strategy_cmd() {
    case(
        "strategy_cmd_open_group",
        Some("acct-a"),
        json!({"type":"StrategyCmd","strategy":"s1-main","group":"16777317","reduce_only":false,"payload_b64":"AQID"}),
    );
    case(
        "strategy_cmd_reduce_only_none",
        Some("acct-a"),
        json!({"type":"StrategyCmd","strategy":"s1-main","group":"none","reduce_only":true,"payload_b64":"Ag=="}),
    );
}

#[test]
fn ctp_159_golden_inject_warmup() {
    case(
        "inject_warmup_chunk0",
        None,
        json!({"type":"InjectWarmup","strategy":"s2-main","local_kind":"3","chunk_no":"0","last":false,"payload_b64":b64(&bytes(100, 1))}),
    );
    case(
        "inject_warmup_last_chunk",
        None,
        json!({"type":"InjectWarmup","strategy":"s2-main","local_kind":"3","chunk_no":"1","last":true,"payload_b64":b64(&bytes(37, 2))}),
    );
}

#[test]
fn ctp_160_golden_reconcile_report() {
    let mut v = e0(
        None,
        json!({"type":"ReconcileReport","report_id":"7","items":[
            {"venue":"binance-usdm","account":"acct-a","item":"Position","severity":"Ignore","inst":"binance-usdm:BTCUSDT","engine_qty":"0","exch_qty":"0","view_seq":"100"},
            {"venue":"bitget-spot","account":"acct-a","item":"Balance","severity":"Warn","asset":"BTC","engine_qty":"0.01","exch_qty":"0.02","view_seq":"100"},
            {"venue":"bitget-spot","account":"acct-a","item":"Summary","sflags":["USERSTREAM_DOWN"],"skipped_inflight":"1"}
        ]}),
    );
    v["issuer"] = json!("reconciler");
    case_with("reconcile_report_three_items", v);
}

/// Response golden input (`.in.toml`).
fn build_response(t: &toml::Table) -> CmdResponse {
    let s = |k: &str| t.get(k).and_then(|v| v.as_str()).map(str::to_owned);
    let i = |k: &str| t.get(k).and_then(|v| v.as_integer());
    let status: TransportStatus = serde_json::from_value(json!(s("status").unwrap())).unwrap();
    let code = s("code").map(|c| ResultCode::from_name(&c).unwrap());
    let cmd_id = Uuid::parse_str(&s("cmd_id").unwrap()).unwrap();
    let e = t.get("engine").and_then(|v| v.as_table()).unwrap();
    let engine = EngineInfo {
        version: e["version"].as_str().unwrap().to_owned(),
        epoch: e["epoch"].as_integer().unwrap() as u32,
        refdata: e["refdata"].as_str().unwrap().to_owned(),
        params: e
            .get("params")
            .and_then(|p| p.as_table())
            .map(|p| {
                p.iter()
                    .map(|(k, v)| (k.clone(), v.as_integer().unwrap() as u64))
                    .collect()
            })
            .unwrap_or_default(),
        risk: e["risk"].as_integer().unwrap() as u64,
    };
    let mut r = CmdResponse::new(cmd_id, status, code, engine);
    r.detail = i("detail").and_then(|d| render_detail(d as u32));
    r.seq = i("seq").map(|v| v as u64);
    r.cmd_hash = s("cmd_hash").map(|h| Hex32::from_hex(&h).unwrap());
    r.ctl_state_version = i("ctl_state_version").map(|v| v as u64);
    if let Some(p) = t.get("preview").and_then(|v| v.as_table()) {
        let g = |k: &str| p.get(k).and_then(|v| v.as_integer()).unwrap_or(0);
        let mut cp = <CmdPreview as zerocopy::FromZeros>::new_zeroed();
        cp.target_account = AccountId(g("target_account") as u8);
        cp.target_kind = g("target_kind") as u8;
        cp.target_mode = g("target_mode") as u8;
        cp.affected_cells = g("affected_cells") as u32;
        cp.open_orders = g("open_orders") as u32;
        cp.wo_venue = VenueId(g("wo_venue") as u8);
        cp.wo_leg_ix = g("wo_leg_ix") as u8;
        cp.wo_inst = InstId(g("wo_inst") as u32);
        cp.wo_qty = Qty(g("wo_qty"));
        r.preview = render_preview(&cp, i("rflags").unwrap_or(0) as u32, rd());
    }
    if let Some(b) = s("body") {
        r.body = raw_of(&serde_json::from_str::<Value>(&b).unwrap());
    }
    r
}

const RESPONSES: &[(&str, &str)] = &[
    (
        "applied_preview_ttl",
        r#"status = "ok"
code = "Applied"
cmd_id = "00000000-0000-4000-8000-000000000001"
seq = 123456789
cmd_hash = "9f3c000000000000000000000000000000000000000000000000000000000001"
ctl_state_version = 4183
rflags = 7
[preview]
target_account = 1
target_kind = 1
target_mode = 4
affected_cells = 3
open_orders = 2
[engine]
version = "0.4.0"
epoch = 17
refdata = "sha256:00"
risk = 7
params = { "s1-main/acct-a" = 42 }
"#,
    ),
    (
        "accepted",
        r#"status = "accepted"
cmd_id = "00000000-0000-4000-8000-000000000002"
cmd_hash = "9f3c000000000000000000000000000000000000000000000000000000000002"
[engine]
version = "0.4.0"
epoch = 17
refdata = "sha256:00"
risk = 7
"#,
    ),
    (
        "invalid_request_unknown_type",
        r#"status = "invalid_request"
cmd_id = "00000000-0000-4000-8000-000000000003"
detail = 100
[engine]
version = "0.4.0"
epoch = 17
refdata = "sha256:00"
risk = 7
"#,
    ),
    (
        "forbidden",
        r#"status = "ok"
code = "Forbidden"
cmd_id = "00000000-0000-4000-8000-000000000004"
[engine]
version = "0.4.0"
epoch = 17
refdata = "sha256:00"
risk = 7
"#,
    ),
    (
        "param_invalid_strategy_field",
        r#"status = "ok"
code = "InvalidArgs"
cmd_id = "00000000-0000-4000-8000-000000000005"
detail = 3211374
seq = 900
cmd_hash = "9f3c000000000000000000000000000000000000000000000000000000000005"
ctl_state_version = 21
[engine]
version = "0.4.0"
epoch = 17
refdata = "sha256:00"
risk = 7
"#,
    ),
    (
        "refdata_assembly",
        r#"status = "ok"
code = "Rejected"
cmd_id = "00000000-0000-4000-8000-000000000006"
detail = 131474
cmd_hash = "9f3c000000000000000000000000000000000000000000000000000000000006"
[engine]
version = "0.4.0"
epoch = 17
refdata = "sha256:00"
risk = 7
"#,
    ),
    (
        "get_schema",
        r#"status = "ok"
code = "Applied"
cmd_id = "00000000-0000-4000-8000-000000000007"
body = '{"name":"probe-taker","kind_id":65281,"fields":[]}'
[engine]
version = "0.4.0"
epoch = 17
refdata = "sha256:00"
risk = 7
"#,
    ),
];

#[test]
fn ctp_161_golden_responses() {
    let d = dir().join("responses");
    for (name, input) in RESPONSES {
        let in_path = d.join(format!("{name}.in.toml"));
        golden(&in_path, input.as_bytes());
        let t: toml::Table = fs::read_to_string(&in_path).unwrap().parse().unwrap();
        let r = build_response(&t);
        golden(&d.join(format!("{name}.json")), &r.to_json());
    }
}

#[test]
fn ctp_162_golden_frames() {
    let mut n = 0;
    let mut names: Vec<PathBuf> = fs::read_dir(dir())
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| p.to_string_lossy().ends_with(".request.json"))
        .collect();
    names.sort();
    for p in names {
        let json = fs::read(&p).unwrap();
        let stem = p
            .file_name()
            .unwrap()
            .to_string_lossy()
            .replace(".request.json", "");
        let mut frame = Vec::new();
        encode_frame(&json, &mut frame);
        golden(&dir().join(format!("frames/{stem}.frame.bin")), &frame);
        n += 1;
    }
    assert!(n >= 40, "{n} request goldens");
}
