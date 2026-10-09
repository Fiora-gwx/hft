//! CTP-090–CTP-092 and the consistency of `PERM_ROWS` text with its semantics.
#![allow(
    clippy::type_complexity,
    clippy::disallowed_types,
    clippy::disallowed_methods,
    clippy::disallowed_macros
)] // allow-list: test code, not a hot path

mod common;

use common::*;
use hft_ctl_proto::perms::{OPTIONAL_ROWS, PERM_ROWS, PRINCIPALS, render_rows};
use hft_ctl_proto::{CmdBody, CmdClass, Principal, Role, RoleSet, allowed, class_of};
use serde_json::{Value, json};

fn class(c: Value) -> CmdClass {
    class_of(&body(c))
}

fn named(name: &str) -> Value {
    core_cmds().into_iter().find(|(n, _, _)| *n == name).unwrap().2
}

#[test]
fn ctp_090_class_of() {
    use CmdClass::*;
    let rows: Vec<(&str, Value, CmdClass)> = vec![
        ("CTP-090.1", named("GetSchema"), ReadOnly),
        ("CTP-090.2", json!({"type":"Kill","level":"Global"}), Downgrade),
        (
            "CTP-090.3",
            json!({"type":"Kill","level":"Account","hard":true}),
            Downgrade,
        ),
        (
            "CTP-090.4",
            json!({"type":"Kill","level":"Venue","venue":"bitget-spot","cancel_resting":false}),
            Downgrade,
        ),
        (
            "CTP-090.5",
            json!({"type":"Kill","level":"Account","flatten":true}),
            Upgrade,
        ),
        ("CTP-090.6", named("SetModeLive"), Upgrade),
        ("CTP-090.7", named("SetModeCloseOnly"), Downgrade),
        ("CTP-090.8", named("SetModeShadow"), Downgrade),
        ("CTP-090.9", named("Resume"), Upgrade),
        ("CTP-090.10", named("SetParams"), Neutral),
        ("CTP-090.11", named("SetRisk"), Neutral),
        ("CTP-090.12", named("ReplaceRefdata"), Neutral),
        ("CTP-090.13", named("CancelAll"), Downgrade),
        ("CTP-090.14", named("Drain"), Downgrade),
        ("CTP-090.15", named("HedgeFix"), Neutral),
        ("CTP-090.16", named("WriteOffResidual"), Upgrade),
        ("CTP-090.17", named("SmokeOrder"), Upgrade),
        ("CTP-090.18", named("StrategyCmd"), Upgrade),
        ("CTP-090.19", named("StrategyCmdRo"), Neutral),
        ("CTP-090.20", named("InjectWarmup"), Neutral),
        ("CTP-090.21", named("AdoptExchange"), Upgrade),
        ("CTP-090.22", named("SetBalanceBaseline"), Upgrade),
        ("CTP-090.23", named("VenueNoticeDelisting"), Downgrade),
        ("CTP-090.24", named("VenueNoticeFunding"), Neutral),
        ("CTP-090.25", named("VenueNoticeMaint"), Neutral),
        ("CTP-090.26", named("SimReset"), Neutral),
        ("CTP-090.27", named("SimTransfer"), Neutral),
        ("CTP-090.28", named("RecordTransfer"), Neutral),
        ("CTP-090.29", named("AnnotateOpsNote"), Neutral),
        ("CTP-090.29", named("AnnotateLocalWrite"), Neutral),
        ("CTP-090.29", named("AnnotateIncident"), Neutral),
        ("CTP-090.30", named("ReconcileReport"), Neutral),
        ("CTP-090.31", named("SimFault"), Neutral),
        (
            "CTP-090.31",
            json!({"type":"SimFault","venue":"binance-usdm","fault":"Off","delay_ms":"0"}),
            Neutral,
        ),
        (
            "CTP-090.31",
            json!({"type":"RotateConn","venue":"bitget-spot"}),
            Neutral,
        ),
        (
            "CTP-090.31",
            json!({"type":"Resubscribe","venue":"bitget-spot"}),
            Neutral,
        ),
        ("CTP-090.31", json!({"type":"SnapshotNow"}), Neutral),
    ];
    for (row, c, want) in rows {
        assert_eq!(class(c), want, "{row}");
    }
}

#[test]
fn ctp_091_role_codes() {
    assert_eq!((Role::Viewer as u8, Role::Viewer.bit()), (1, 0b0001), "CTP-091.1");
    assert_eq!((Role::Trader as u8, Role::Trader.bit()), (2, 0b0010), "CTP-091.2");
    assert_eq!((Role::Admin as u8, Role::Admin.bit()), (4, 0b1000), "CTP-091.3");
    assert!(Role::try_from(3).is_err(), "CTP-091.4");
    assert!(Role::try_from(0).is_err(), "CTP-091.5");
    assert!(Role::try_from(5).is_err(), "CTP-091.6");
    assert!(RoleSet::from_bits(0b0100).is_err(), "CTP-091.7");
    assert_eq!(RoleSet::from_bits(0b1011).map(RoleSet::bits), Ok(0b1011));
    for r in Role::ALL {
        assert_eq!(Role::try_from(r as u8), Ok(r));
        assert_eq!(RoleSet::from_bits(r.bit()), Ok(RoleSet::from_roles(&[r])));
    }
}

/// Every command body of the domain the principal may issue (by name).
fn allowed_set(principal: Principal, roles: RoleSet) -> Vec<&'static str> {
    let mut cmds = core_cmds();
    cmds.push((
        "KillGlobalHard",
        None,
        json!({"type":"Kill","level":"Global","hard":true}),
    ));
    cmds.push((
        "KillAccountFlatten",
        Some("acct-a"),
        json!({"type":"Kill","level":"Account","flatten":true}),
    ));
    cmds.push((
        "SetModeShadowAll",
        Some("acct-a"),
        json!({"type":"SetMode","group":"*","mode":"Shadow"}),
    ));
    cmds.push((
        "SetModeCloseOnlyAll",
        Some("acct-a"),
        json!({"type":"SetMode","group":"*","mode":"CloseOnly"}),
    ));
    cmds.into_iter()
        .filter(|(_, _, c)| {
            allowed(
                roles,
                principal,
                hft_ctl_proto::ChatType::Private,
                &body(c.clone()),
            )
            .is_ok()
        })
        .map(|(n, _, _)| n)
        .collect()
}

#[test]
fn ctp_092_principal_table() {
    let find = |issuer: &str| PRINCIPALS.iter().find(|p| p.issuer == issuer).unwrap();
    let tg = find("tg:<from.id>");
    assert_eq!(
        (tg.uid, tg.principal),
        (Some("hft-opsbot"), Principal::Tg),
        "CTP-092.1"
    );
    // Viewer + Trader + Admin minus the cli-only commands.
    let all = RoleSet::VIEWER.union(RoleSet::TRADER).union(RoleSet::ADMIN);
    let tg_set = allowed_set(Principal::Tg, all);
    let cli_set = allowed_set(Principal::Cli, all);
    let cli_only: Vec<_> = cli_set.iter().filter(|n| !tg_set.contains(n)).copied().collect();
    assert_eq!(
        cli_only,
        ["SetParams", "SetRisk", "ReplaceRefdata", "SimFault"],
        "CTP-092.1"
    );
    let bot = find("ops_bot");
    assert_eq!(
        (bot.uid, bot.principal),
        (Some("hft-opsbot"), Principal::OpsBot),
        "CTP-092.2"
    );
    assert_eq!(
        allowed_set(Principal::OpsBot, RoleSet::EMPTY),
        ["KillGlobal", "KillAccount", "AnnotateOpsNote"],
        "CTP-092.2"
    );
    let cli = find("cli:<login>");
    assert_eq!((cli.uid, cli.principal), (None, Principal::Cli), "CTP-092.3");
    assert_eq!(
        allowed_set(Principal::Cli, RoleSet::TRADER),
        allowed_set(Principal::Cli, RoleSet::TRADER)
    );
    assert!(
        allowed_set(Principal::Cli, RoleSet::VIEWER).len()
            < allowed_set(Principal::Cli, RoleSet::ADMIN).len(),
        "CTP-092.3"
    );
    let rec = find("reconciler");
    assert_eq!(
        (rec.uid, rec.principal),
        (Some("hft-recon"), Principal::Reconciler),
        "CTP-092.4"
    );
    assert_eq!(
        allowed_set(Principal::Reconciler, RoleSet::EMPTY),
        ["ReconcileReport"],
        "CTP-092.4"
    );
    let pipe = find("pipeline");
    assert_eq!(
        (pipe.uid, pipe.principal),
        (Some("hft-pipeline"), Principal::Pipeline),
        "CTP-092.5"
    );
    assert_eq!(
        allowed_set(Principal::Pipeline, RoleSet::EMPTY),
        [
            "GetSchema",
            "SetModeCloseOnly",
            "SetModeShadow",
            "SetParams",
            "InjectWarmup"
        ],
        "CTP-092.5"
    );
    let sysd = find("cli:systemd");
    assert_eq!(
        (sysd.uid, sysd.principal, sysd.via_socket),
        (None, Principal::Systemd, false),
        "CTP-092.6"
    );
    assert_eq!(
        allowed_set(Principal::Systemd, RoleSet::EMPTY),
        ["KillGlobal", "KillAccount", "CancelAll", "Drain"],
        "CTP-092.6"
    );
    for p in PRINCIPALS {
        assert!(!["watchtower", "treasury"].contains(&p.name), "CTP-092.7");
        assert!(
            !p.issuer.contains("watchtower") && !p.issuer.contains("treasury"),
            "CTP-092.7"
        );
    }
    // Issuer forms map to their principal kinds.
    assert_eq!(Principal::of_issuer("tg:1002"), Some(Principal::Tg));
    assert_eq!(Principal::of_issuer("tg:abc"), None);
    assert_eq!(Principal::of_issuer("cli:alice"), Some(Principal::Cli));
    assert_eq!(Principal::of_issuer("cli:systemd"), Some(Principal::Systemd));
    assert_eq!(Principal::of_issuer("watchtower"), None);
    perm_rows_text_matches_semantics();
}

/// The README text columns of every row agree with the row's semantics.
fn perm_rows_text_matches_semantics() {
    for r in PERM_ROWS {
        let tick = |s: &str| s.starts_with('✓');
        assert_eq!(tick(r.text[3]), r.roles.contains(Role::Viewer), "{}", r.text[0]);
        assert_eq!(tick(r.text[4]), r.roles.contains(Role::Trader), "{}", r.text[0]);
        assert_eq!(tick(r.text[5]), r.roles.contains(Role::Admin), "{}", r.text[0]);
        let cli_mark = r.text[3..6].iter().any(|c| c.contains("仅 `cli:<login>`"));
        assert_eq!(cli_mark, r.cli_only, "{}", r.text[0]);
        assert_eq!(tick(r.text[6]), r.group_chat, "{}", r.text[0]);
        assert_eq!(r.text[1], "core", "{}", r.text[0]);
        assert_eq!(r.text[7] == "—", r.procs.is_empty(), "{}", r.text[0]);
    }
    assert!(OPTIONAL_ROWS.iter().all(|r| r[1] == "optional"));
    assert_eq!(render_rows().len(), PERM_ROWS.len() + OPTIONAL_ROWS.len());
    let _ = CmdBody::SnapshotNow { account: None };
}
