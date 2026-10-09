//! CTP-100–CTP-139, CTP-176: `allowed` over the 13 baseline principals.
#![allow(
    clippy::type_complexity,
    clippy::disallowed_types,
    clippy::disallowed_methods,
    clippy::disallowed_macros
)] // allow-list: test code, not a hot path

mod common;

use common::*;
use hft_ctl_proto::{ResultCode, allowed};
use serde_json::{Value, json};

/// `want`: one character per principal in baseline order (tgV, tgVg, tgT, tgTg, tgA, tgAg, cliV,
/// cliT, cliA, bot, pipe, recon, sysd); `A` = allowed, `.` = `Forbidden`.
fn check(case: &str, cmd: Value, want: &str) {
    let b = body(cmd);
    assert_eq!(want.len(), 13);
    for (i, (w, c)) in WHO.iter().zip(want.chars()).enumerate() {
        let got = allowed(w.roles, w.principal, w.chat, &b);
        match c {
            'A' => assert_eq!(got, Ok(()), "{case}.{} ({})", i + 1, w.tag),
            _ => {
                let e = got.expect_err(&format!("{case}.{} ({})", i + 1, w.tag));
                assert_eq!(e.code(), ResultCode::Forbidden, "{case}.{}", i + 1);
            }
        }
    }
}

fn kill(level: &str, extra: Value) -> Value {
    let mut c = json!({"type":"Kill","level":level});
    if level == "Venue" {
        c["venue"] = json!("bitget-spot");
    }
    for (k, v) in extra.as_object().unwrap() {
        c[k] = v.clone();
    }
    c
}

fn set_mode(mode: &str, group: &str) -> Value {
    json!({"type":"SetMode","strategy":"s1-main","group":group,"mode":mode})
}

fn cmd(name: &str) -> Value {
    core_cmds().into_iter().find(|(n, _, _)| *n == name).unwrap().2
}

#[test]
fn ctp_100_perm_matrix() {
    check("CTP-100", cmd("GetSchema"), "AAAAAAAAA.A..");
}
#[test]
fn ctp_101_perm_matrix() {
    check("CTP-101", kill("Global", json!({})), "AAAAAAAAAA..A");
}
#[test]
fn ctp_102_perm_matrix() {
    check("CTP-102", kill("Account", json!({})), "AAAAAAAAAA..A");
}
#[test]
fn ctp_103_perm_matrix() {
    check("CTP-103", kill("Venue", json!({})), "..AAAA.AA....");
}
#[test]
fn ctp_104_perm_matrix() {
    check(
        "CTP-104",
        kill("Global", json!({"cancel_resting":false})),
        "..A.A..AA....",
    );
}
#[test]
fn ctp_105_perm_matrix() {
    check(
        "CTP-105",
        kill("Account", json!({"cancel_resting":false})),
        "..A.A..AA....",
    );
}
#[test]
fn ctp_106_perm_matrix() {
    check(
        "CTP-106",
        kill("Venue", json!({"cancel_resting":false})),
        "..A.A..AA....",
    );
}
#[test]
fn ctp_107_perm_matrix() {
    check("CTP-107", kill("Global", json!({"hard":true})), "..A.A..AA....");
}
#[test]
fn ctp_108_perm_matrix() {
    check("CTP-108", kill("Account", json!({"hard":true})), "..A.A..AA....");
}
#[test]
fn ctp_109_perm_matrix() {
    check(
        "CTP-109",
        kill("Account", json!({"flatten":true})),
        "..A.A..AA....",
    );
}
#[test]
fn ctp_110_perm_matrix() {
    check("CTP-110", set_mode("CloseOnly", "16777317"), "..AAAA.AA.A..");
}
#[test]
fn ctp_111_perm_matrix() {
    check("CTP-111", set_mode("CloseOnly", "*"), "..AAAA.AA....");
}
#[test]
fn ctp_112_perm_matrix() {
    check("CTP-112", set_mode("Shadow", "16777317"), "..A.A..AA.A..");
}
#[test]
fn ctp_113_perm_matrix() {
    check("CTP-113", set_mode("Shadow", "*"), "..A.A..AA....");
}
#[test]
fn ctp_114_perm_matrix() {
    check("CTP-114", set_mode("Live", "16777317"), "..A.A..AA....");
}
#[test]
fn ctp_115_perm_matrix() {
    check("CTP-115", cmd("Resume"), "..A.A..AA....");
}
#[test]
fn ctp_116_perm_matrix() {
    check("CTP-116", cmd("SetParams"), ".......AA.A..");
}
#[test]
fn ctp_117_perm_matrix() {
    check("CTP-117", cmd("SetRisk"), "........A....");
}
#[test]
fn ctp_118_perm_matrix() {
    check("CTP-118", cmd("ReplaceRefdata"), "........A....");
}
#[test]
fn ctp_119_perm_matrix() {
    check("CTP-119", cmd("CancelAll"), "..A.A..AA...A");
}
#[test]
fn ctp_120_perm_matrix() {
    check("CTP-120", cmd("Drain"), "..A.A..AA...A");
}
#[test]
fn ctp_121_perm_matrix() {
    check("CTP-121", cmd("HedgeFix"), "..A.A..AA....");
}
#[test]
fn ctp_122_perm_matrix() {
    check("CTP-122", cmd("WriteOffResidual"), "....A...A....");
}
#[test]
fn ctp_123_perm_matrix() {
    check("CTP-123", cmd("SmokeOrder"), "....A...A....");
}
#[test]
fn ctp_124_perm_matrix() {
    check("CTP-124", cmd("StrategyCmd"), "..A.A..AA....");
}
#[test]
fn ctp_125_perm_matrix() {
    check("CTP-125", cmd("StrategyCmdRo"), "..A.A..AA....");
}
#[test]
fn ctp_126_perm_matrix() {
    check("CTP-126", cmd("InjectWarmup"), "..A.A..AA.A..");
}
#[test]
fn ctp_127_perm_matrix() {
    check("CTP-127", cmd("AdoptExchange"), "..A.A..AA....");
}
#[test]
fn ctp_128_perm_matrix() {
    check("CTP-128", cmd("SetBalanceBaseline"), "..A.A..AA....");
}
#[test]
fn ctp_129_perm_matrix() {
    check("CTP-129", cmd("VenueNoticeDelisting"), "..A.A..AA....");
}
#[test]
fn ctp_130_perm_matrix() {
    check("CTP-130", cmd("VenueNoticeFunding"), "..A.A..AA....");
}
#[test]
fn ctp_131_perm_matrix() {
    check("CTP-131", cmd("VenueNoticeMaint"), "..A.A..AA....");
}
#[test]
fn ctp_132_perm_matrix() {
    check("CTP-132", cmd("SimReset"), "..A.A..AA....");
}
#[test]
fn ctp_133_perm_matrix() {
    check("CTP-133", cmd("SimTransfer"), "..A.A..AA....");
}
#[test]
fn ctp_134_perm_matrix() {
    check("CTP-134", cmd("RecordTransfer"), "..A.A..AA....");
}
#[test]
fn ctp_135_perm_matrix() {
    check("CTP-135", cmd("AnnotateOpsNote"), "A.A.A.AAAA...");
}
#[test]
fn ctp_136_perm_matrix() {
    check("CTP-136", cmd("AnnotateLocalWrite"), "A.A.A.AAA....");
}
#[test]
fn ctp_137_perm_matrix() {
    check("CTP-137", cmd("AnnotateIncident"), "..A.A..AA....");
}
#[test]
fn ctp_138_perm_matrix() {
    check("CTP-138", cmd("ReconcileReport"), "...........A.");
}

#[test]
fn ctp_139_optional_forbidden() {
    check(
        "CTP-139.1",
        json!({"type":"RotateConn","venue":"bitget-spot","account":"acct-a"}),
        ".............",
    );
    check(
        "CTP-139.2",
        json!({"type":"Resubscribe","venue":"bitget-spot"}),
        ".............",
    );
    check("CTP-139.3", json!({"type":"SnapshotNow"}), ".............");
}

#[test]
fn ctp_176_perm_matrix() {
    check("CTP-176", cmd("SimFault"), "........A....");
}
