//! CTP-080–CTP-082.
#![allow(
    clippy::type_complexity,
    clippy::disallowed_types,
    clippy::disallowed_methods,
    clippy::disallowed_macros
)] // allow-list: test code, not a hot path

mod common;

use common::*;
use hft_ctl_proto::{CmdBody, ConfirmRule, Role, check_confirm_fields, needs_confirm_static};
use proptest::prelude::*;
use serde_json::{Value, json};

#[test]
fn ctp_080_check_confirm_fields() {
    use ConfirmRule::*;
    let rows: [(&str, ConfirmRule, bool, bool, Option<u64>, bool); 12] = [
        ("CTP-080.1", Always, false, true, Some(5), true),
        ("CTP-080.2", Always, false, true, None, false),
        ("CTP-080.3", Always, false, true, Some(0), false),
        ("CTP-080.4", Always, false, true, Some(1), true),
        ("CTP-080.5", IfLoosening, false, true, None, false),
        ("CTP-080.6", Never, false, true, None, false),
        ("CTP-080.7", Never, false, false, None, true),
        ("CTP-080.8", Always, false, false, None, true),
        ("CTP-080.9", Always, true, false, None, true),
        ("CTP-080.10", PreviewBound, false, true, None, true),
        ("CTP-080.11", PreviewBound, false, true, Some(7), true),
        ("CTP-080.12", PreviewBound, false, true, Some(0), true),
    ];
    for (row, rule, preview, confirmed, esv, want_ok) in rows {
        let mut r = req(
            Some("acct-a"),
            json!({"type":"SetMode","strategy":"s1-main","group":"*","mode":"Live"}),
        );
        r.preview = preview;
        r.confirmed = confirmed;
        r.expect_state_version = esv;
        let got = check_confirm_fields(rule, &r);
        assert_eq!(got.is_ok(), want_ok, "{row}");
        if let Err(e) = got {
            // Rendered as invalid_request without a detail.
            assert_eq!(e.detail, 0, "{row}");
        }
    }
}

fn kill(level: &str, extra: Value) -> Value {
    let mut c = json!({"type":"Kill","level":level});
    if level == "Venue" {
        c["venue"] = json!("binance-spot");
    }
    for (k, v) in extra.as_object().unwrap() {
        c[k] = v.clone();
    }
    c
}

#[test]
fn ctp_081_needs_confirm_static() {
    use ConfirmRule::*;
    let v = Some(Role::Viewer);
    let t = Some(Role::Trader);
    let a = Some(Role::Admin);
    let rows: Vec<(&str, Option<Role>, &str, Value, ConfirmRule)> = vec![
        ("CTP-081.1", v, "tgV", kill("Global", json!({})), PreviewBound),
        ("CTP-081.2", v, "tgV", kill("Account", json!({})), PreviewBound),
        ("CTP-081.3", v, "cliV", kill("Global", json!({})), PreviewBound),
        ("CTP-081.4", t, "tgT", kill("Global", json!({})), Never),
        ("CTP-081.5", a, "cliA", kill("Account", json!({})), Never),
        (
            "CTP-081.6",
            t,
            "tgT",
            kill("Account", json!({"flatten":true})),
            Always,
        ),
        (
            "CTP-081.7",
            t,
            "tgT",
            kill("Account", json!({"hard":true})),
            Never,
        ),
        (
            "CTP-081.8",
            t,
            "tgT",
            kill("Venue", json!({"cancel_resting":false})),
            Never,
        ),
        ("CTP-081.9", None, "bot", kill("Global", json!({})), Never),
        ("CTP-081.10", None, "sysd", kill("Account", json!({})), Never),
        (
            "CTP-081.11",
            t,
            "tgT",
            json!({"type":"SetMode","mode":"Live"}),
            Always,
        ),
        (
            "CTP-081.12",
            t,
            "tgT",
            json!({"type":"SetMode","mode":"CloseOnly"}),
            Never,
        ),
        (
            "CTP-081.13",
            None,
            "pipe",
            json!({"type":"SetMode","group":"16777317","mode":"Shadow"}),
            Never,
        ),
        (
            "CTP-081.14",
            t,
            "tgT",
            json!({"type":"Resume","source":"Kill"}),
            Always,
        ),
        (
            "CTP-081.15",
            t,
            "cliT",
            json!({"type":"SetParams","strategy":"s1-main","blob_b64":"AA=="}),
            IfLoosening,
        ),
        (
            "CTP-081.16",
            None,
            "pipe",
            json!({"type":"SetParams","strategy":"s1-main","blob_b64":"AA=="}),
            IfLoosening,
        ),
        (
            "CTP-081.17",
            a,
            "cliA",
            json!({"type":"SetRisk","blob_b64":"AA=="}),
            IfLoosening,
        ),
        (
            "CTP-081.18",
            a,
            "cliA",
            json!({"type":"ReplaceRefdata","blob_b64":"AA==","version":"8"}),
            Always,
        ),
        ("CTP-081.19", t, "tgT", json!({"type":"CancelAll"}), Never),
        ("CTP-081.20", None, "sysd", json!({"type":"Drain"}), Never),
        (
            "CTP-081.21",
            t,
            "tgT",
            json!({"type":"HedgeFix","group":"16777317"}),
            Never,
        ),
        (
            "CTP-081.22",
            a,
            "tgA",
            json!({"type":"WriteOffResidual","group":"16777317"}),
            Always,
        ),
        (
            "CTP-081.23",
            a,
            "tgA",
            json!({"type":"SmokeOrder","venue":"bitget-spot","inst":"bitget-spot:BTCUSDT","side":"Buy"}),
            Always,
        ),
        (
            "CTP-081.24",
            t,
            "tgT",
            json!({"type":"StrategyCmd","strategy":"s1-main","group":"none","reduce_only":false,"payload_b64":"AQ=="}),
            Always,
        ),
        (
            "CTP-081.25",
            t,
            "tgT",
            json!({"type":"StrategyCmd","strategy":"s1-main","group":"none","reduce_only":true,"payload_b64":"AQ=="}),
            Never,
        ),
        (
            "CTP-081.26",
            None,
            "pipe",
            json!({"type":"InjectWarmup","strategy":"s2-main","local_kind":"3","chunk_no":"0","payload_b64":"AQ=="}),
            Never,
        ),
        (
            "CTP-081.27",
            t,
            "tgT",
            json!({"type":"AdoptExchange","venue":"binance-usdm","report_id":"7","item":"Position","inst":"binance-usdm:BTCUSDT"}),
            Always,
        ),
        (
            "CTP-081.28",
            t,
            "tgT",
            json!({"type":"SetBalanceBaseline","venue":"bitget-spot"}),
            Always,
        ),
        (
            "CTP-081.29",
            t,
            "tgT",
            json!({"type":"VenueNotice","venue":"binance-spot","kind":"Delisting","at_ns":"1"}),
            Never,
        ),
        (
            "CTP-081.30",
            t,
            "tgT",
            json!({"type":"VenueNotice","venue":"binance-spot","kind":"Maintenance","from_ns":"1","to_ns":"2"}),
            Never,
        ),
        ("CTP-081.31", t, "tgT", json!({"type":"SimReset"}), Always),
        (
            "CTP-081.32",
            t,
            "tgT",
            json!({"type":"SimTransfer","from_venue":"binance-usdm","to_venue":"bitget-spot","asset":"BTC","amount":"1"}),
            Never,
        ),
        (
            "CTP-081.33",
            t,
            "tgT",
            json!({"type":"RecordTransfer","from":"binance-usdm","to":"bitget-spot","asset":"USDT","amount":"1","ext_ref":"x"}),
            Never,
        ),
        (
            "CTP-081.34",
            v,
            "tgV",
            json!({"type":"Annotate","kind":"OpsNote","text":"x"}),
            Never,
        ),
        (
            "CTP-081.35",
            v,
            "tgV",
            json!({"type":"GetSchema","strategy_kind":"probe-taker"}),
            Never,
        ),
    ];
    for (row, role, who, cmd, want) in rows {
        let w = who_by(who);
        assert_eq!(needs_confirm_static(role, w.principal, &body(cmd)), want, "{row}");
    }
}

/// Every core command variant plus every `Kill` parameter combination.
pub fn cmd_domain() -> Vec<CmdBody> {
    let mut v: Vec<Value> = core_cmds().into_iter().map(|(_, _, c)| c).collect();
    for level in ["Global", "Account", "Venue"] {
        for hard in [None, Some(false), Some(true)] {
            for flatten in [None, Some(false), Some(true)] {
                for cr in [None, Some(false), Some(true)] {
                    let mut c = kill(level, json!({}));
                    if let Some(h) = hard {
                        c["hard"] = json!(h);
                    }
                    if let Some(f) = flatten {
                        c["flatten"] = json!(f);
                    }
                    if let Some(x) = cr {
                        c["cancel_resting"] = json!(x);
                    }
                    v.push(c);
                }
            }
        }
    }
    v.into_iter().map(body).collect()
}

fn is_viewer_stop(b: &CmdBody) -> bool {
    match b {
        CmdBody::Kill {
            level,
            hard,
            flatten,
            cancel_resting,
            ..
        } => {
            *level != hft_types::KillLevel::Venue
                && *hard != Some(true)
                && *flatten != Some(true)
                && *cancel_resting != Some(false)
        }
        _ => false,
    }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(2_000))]
    #[test]
    fn ctp_082_preview_bound_only_viewer_kill(
        role_ix in 0usize..4,
        who_ix in 0usize..13,
        cmd_ix in any::<prop::sample::Index>(),
    ) {
        let roles = [Some(Role::Viewer), Some(Role::Trader), Some(Role::Admin), None];
        let role = roles[role_ix];
        let dom = cmd_domain();
        let b = &dom[cmd_ix.index(dom.len())];
        let got = needs_confirm_static(role, WHO[who_ix].principal, b) == ConfirmRule::PreviewBound;
        prop_assert_eq!(got, role == Some(Role::Viewer) && is_viewer_stop(b));
    }
}
