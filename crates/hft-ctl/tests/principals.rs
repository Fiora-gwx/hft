//! CTL-304–CTL-306.
#![allow(
    clippy::type_complexity,
    clippy::disallowed_types,
    clippy::disallowed_methods,
    clippy::disallowed_macros
)] // allow-list: test code, not a hot path

use crate::common::*;
use hft_ctl::{PrincipalTable, PrincipalsErr};
use hft_ctl_proto::{Principal, Role, RoleSet};
use serde_json::json;

const FILE: &str = r#"[uids]
"hft-opsbot" = "ops_bot"
"hft-pipeline" = "pipeline"
"hft-recon" = "reconciler"
"alice" = "cli"

[issuers]
"tg:1002" = ["Trader"]
"cli:alice" = ["Admin"]
"#;

fn passwd(user: &str) -> Option<u32> {
    match user {
        "hft-opsbot" => Some(1100),
        "hft-pipeline" => Some(1101),
        "hft-recon" => Some(1102),
        "alice" => Some(1200),
        "tom" => Some(1201),
        _ => None,
    }
}

fn tmp(name: &str, text: &str) -> std::path::PathBuf {
    let p = std::env::temp_dir().join(format!("ctl-principals-{name}-{}.toml", std::process::id()));
    std::fs::write(&p, text).unwrap();
    p
}

#[test]
fn ctl_304_load_principals() {
    let t = PrincipalTable::load_with(&tmp("ok", FILE), passwd).unwrap();
    let tg = t.resolve(1100, "tg:1002").unwrap();
    assert_eq!(
        (tg.principal, tg.roles),
        (Principal::Tg, RoleSet::from_roles(&[Role::Trader]))
    );
    assert_eq!(t.resolve(1100, "ops_bot").unwrap().principal, Principal::OpsBot);
    let a = t.resolve(1200, "cli:alice").unwrap();
    assert_eq!(
        (a.principal, a.roles),
        (Principal::Cli, RoleSet::from_roles(&[Role::Admin]))
    );
    assert!(t.resolve(1200, "cli:tom").is_none());
    assert_eq!(t.roles("cli:alice"), Some(RoleSet::ADMIN));
    assert_eq!(t.roles("tg:1002"), Some(RoleSet::TRADER));
}

#[test]
fn ctl_305_load_errors() {
    let missing = std::env::temp_dir().join("ctl-principals-does-not-exist.toml");
    assert!(
        matches!(
            PrincipalTable::load_with(&missing, passwd),
            Err(PrincipalsErr::Io(_))
        ),
        "CTL-305.1"
    );
    let rows: Vec<(&str, String, fn(&PrincipalsErr) -> bool)> = vec![
        ("CTL-305.2", FILE.replacen("[uids]", "[uids", 1), |e| {
            matches!(e, PrincipalsErr::Parse(_))
        }),
        ("CTL-305.3", format!("extra = 1\n{FILE}"), |e| {
            matches!(e, PrincipalsErr::Parse(_))
        }),
        (
            "CTL-305.4",
            FILE.replacen(
                "\"alice\" = \"cli\"",
                "\"alice\" = \"cli\"\n\"nobody-xyz\" = \"cli\"",
                1,
            ),
            |e| matches!(e, PrincipalsErr::UnknownUser(_)),
        ),
        (
            "CTL-305.5",
            FILE.replacen(
                "\"hft-recon\" = \"reconciler\"",
                "\"hft-recon\" = \"watchtower\"",
                1,
            ),
            |e| matches!(e, PrincipalsErr::UnknownPrincipal(_)),
        ),
        (
            "CTL-305.6",
            FILE.replacen("\"tg:1002\" = [\"Trader\"]", "\"tg:1002\" = [\"Boss\"]", 1),
            |e| matches!(e, PrincipalsErr::UnknownRole(_)),
        ),
        ("CTL-305.7", format!("{FILE}\"tg:abc\" = [\"Viewer\"]\n"), |e| {
            matches!(e, PrincipalsErr::BadIssuer(_))
        }),
        ("CTL-305.8", format!("{FILE}\"cli:bob\" = [\"Trader\"]\n"), |e| {
            matches!(e, PrincipalsErr::BadIssuer(_))
        }),
        ("CTL-305.9", format!("{FILE}\"tg:1002\" = [\"Viewer\"]\n"), |e| {
            matches!(e, PrincipalsErr::DupEntry(_))
        }),
    ];
    for (row, text, ok) in rows {
        let e = PrincipalTable::load_with(&tmp(&row.replace('.', "_"), &text), passwd).unwrap_err();
        assert!(ok(&e), "{row}: {e:?}");
    }
}

#[test]
fn ctl_306_no_reload() {
    let text = format!("{FILE}\"cli:tom\" = [\"Trader\"]\n").replacen(
        "\"alice\" = \"cli\"",
        "\"alice\" = \"cli\"\n\"tom\" = \"cli\"",
        1,
    );
    let path = tmp("reload", &text);
    let table = PrincipalTable::load_with(&path, passwd).unwrap();
    let rd = p0::p0();
    let mut cfg = config(std::env::temp_dir().join("ctl-306.sock"));
    cfg.principals = table;
    let mut g = G0::with_cfg(rd, 262_144, g0_active(), cfg);
    let note = json!({"type":"Annotate","kind":"OpsNote","text":"x"});
    let r = g.send("cli:tom", Some("*"), note.clone());
    assert_eq!(code(&r), "Applied");
    // Remove the cli:tom line; the gateway keeps what it loaded at startup.
    std::fs::write(&path, text.replace("\"cli:tom\" = [\"Trader\"]\n", "")).unwrap();
    let r = g.send("cli:tom", Some("*"), note);
    assert_eq!(code(&r), "Applied");
    assert_eq!(G0::<CtlTestSet>::cmds(&g.seen).len(), 2);
}
