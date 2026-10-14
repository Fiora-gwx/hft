//! `build --kind refdata` (tools/TESTS.md TOOL-131): account, Sim account and instance checks,
//! `uid_policy` with the Admin sign-off, exponents, unknown keys.

mod common;

use common::schemas::schema_dir;
use common::*;

/// Edits `(file, from, to)` of the `refdata-basic` copy; `to` may append (`from` kept in it).
type Edit = (&'static str, &'static str, &'static str);

const PC_LIVE: &str = "label = \"pc-live\"";
const SPARE: &str = "label = \"spare\"\nkind = \"live\"\nis_prod = false\n";
const SPARE_UID: &str = "venue_uids = { bitget-uta-spot = \"6001\" }";
const SIM: &str = "kind = \"sim\"\n";
const SIM_SECTION: &str = "fee_tier = 0\nlatency_model = 0";
const BALANCES: &str = "initial_balances = [ { venue = \"binance-usdm\", asset = \"USDT\", qty = \"10000\" },\n                     { venue = \"bitget-uta-spot\", asset = \"USDT\", qty = \"10000\" } ]\n";

/// Extra Live accounts on bitget-uta-spot (ids from 5), appended to accounts.toml.
fn extra_accounts(n: usize) -> String {
    (0..n)
        .map(|k| {
            format!(
                "\n[[account]]\nid = {}\nlabel = \"extra-{k}\"\nkind = \"live\"\nis_prod = false\nvenues = [\"bitget-uta-spot\"]\nvenue_uids = {{ bitget-uta-spot = \"77{k:02}\" }}\n",
                5 + k
            )
        })
        .collect()
}

struct Row {
    id: &'static str,
    edits: &'static [Edit],
    append: Option<String>,
    sim_models: bool,
    exit: i32,
    msg: &'static str,
}

const R: Row = Row {
    id: "",
    edits: &[],
    append: None,
    sim_models: false,
    exit: 1,
    msg: "",
};

fn build(row: &Row, json: bool) -> Out {
    let dir = scratch(&format!("tool_131/{}{}", row.id, if json { "-json" } else { "" }));
    let schemas = schema_dir(&dir);
    let d = dir.join("in");
    std::fs::create_dir_all(&d).unwrap();
    for e in std::fs::read_dir(fixtures().join("refdata-basic")).unwrap() {
        let e = e.unwrap();
        std::fs::copy(e.path(), d.join(e.file_name())).unwrap();
    }
    for (file, from, to) in row.edits {
        let p = d.join(file);
        let t = std::fs::read_to_string(&p).unwrap();
        assert!(t.contains(from), "{}: {file}: `{from}` not found", row.id);
        std::fs::write(&p, t.replacen(from, to, 1)).unwrap();
    }
    if let Some(a) = &row.append {
        let p = d.join("accounts.toml");
        let t = std::fs::read_to_string(&p).unwrap();
        std::fs::write(&p, t + a).unwrap();
    }
    std::fs::write(dir.join("hft-admins.txt"), "# Admin GitHub logins\nowner1\n").unwrap();
    std::fs::write(
        dir.join("core.toml"),
        "[sim]\nlatency_models = [ { name = \"lan\" }, { name = \"wan\" } ]\nmargin_models = [ { name = \"std\" } ]\n",
    )
    .unwrap();
    let out = dir.join("rd.blob");
    let mut args = vec![
        "build".to_string(),
        "--kind".into(),
        "refdata".into(),
        "--in".into(),
        p(&d).into(),
        "--out".into(),
        p(&out).into(),
        "--schemas".into(),
        p(&schemas).into(),
        "--admins".into(),
        p(&dir.join("hft-admins.txt")).into(),
    ];
    if row.sim_models {
        args.push("--sim-models".into());
        args.push(p(&dir.join("core.toml")).into());
    }
    if json {
        args.push("--json".into());
    }
    let refs: Vec<&str> = args.iter().map(String::as_str).collect();
    let r = run(&refs);
    assert_eq!(out.exists(), r.code == 0, "{}: blob only on success", row.id);
    r
}

#[test]
fn tool_131_account_validation() {
    let unverifiable_spare: &[Edit] = &[
        (
            "accounts.toml",
            SPARE,
            "label = \"spare\"\nkind = \"live\"\nis_prod = false\nuid_policy = \"unverifiable\"\nuid_policy_signoff = \"owner1\"\n",
        ),
        (
            "accounts.toml",
            SPARE_UID,
            "venue_uids = { bitget-uta-spot = \"\" }",
        ),
    ];
    let rows = vec![
        Row {
            id: "TOOL-131.1",
            edits: &[("accounts.toml", "label = \"sim-pc-a\"", "label = \"pc-sim-a\"")],
            msg: "AccountNamePrefix",
            ..R
        },
        Row {
            id: "TOOL-131.2",
            edits: &[("accounts.toml", "label = \"spare\"", "label = \"sim-live\"")],
            msg: "AccountNamePrefix",
            ..R
        },
        Row {
            id: "TOOL-131.3",
            edits: &[(
                "accounts.toml",
                "venues = [\"binance-usdm\", \"bitget-uta-spot\"]\nvenue_uids = { binance-usdm = \"123456789\",",
                "venues = [\"binance-usdm\", \"bitget-uta-spot\", \"binance-spot\"]\nvenue_uids = { binance-spot = \"424242\", binance-usdm = \"123456789\",",
            )],
            msg: "SameExchangeMarkets",
            ..R
        },
        Row {
            id: "TOOL-131.4",
            edits: &[(
                "accounts.toml",
                "venues = [\"binance-usdm\", \"bitget-uta-spot\"]\n[account.sim]",
                "venues = [\"binance-usdm\", \"bitget-uta-spot\", \"okx-spot\", \"okx-swap\"]\n[account.sim]",
            )],
            msg: "SameExchangeMarkets",
            ..R
        },
        Row {
            id: "TOOL-131.5",
            edits: &[(
                "accounts.toml",
                SPARE,
                "label = \"spare\"\nkind = \"live\"\nis_prod = false\nbound_instance = \"pc-main\"\n",
            )],
            msg: "InstanceMultiBound",
            ..R
        },
        Row {
            id: "TOOL-131.6",
            edits: &[("accounts.toml", ", bitget-uta-spot = \"987654321\"", "")],
            msg: "MissingVenueUid",
            ..R
        },
        Row {
            id: "TOOL-131.7",
            edits: &[(
                "accounts.toml",
                PC_LIVE,
                "label = \"pc-live\"\nuid_policy = \"unverifiable\"\nuid_policy_signoff = \"owner1\"",
            )],
            msg: "UidPolicyProd",
            ..R
        },
        Row {
            id: "TOOL-131.8",
            edits: &[(
                "accounts.toml",
                SIM,
                "kind = \"sim\"\nuid_policy = \"unverifiable\"\n",
            )],
            msg: "UidPolicyProd",
            ..R
        },
        Row {
            id: "TOOL-131.9",
            edits: unverifiable_spare,
            exit: 0,
            msg: "signed off by owner1",
            ..R
        },
        Row {
            id: "TOOL-131.10",
            edits: &[
                (
                    "accounts.toml",
                    SPARE,
                    "label = \"spare\"\nkind = \"live\"\nis_prod = false\nuid_policy = \"unverifiable\"\n",
                ),
                (
                    "accounts.toml",
                    SPARE_UID,
                    "venue_uids = { bitget-uta-spot = \"\" }",
                ),
            ],
            msg: "needs uid_policy_signoff",
            ..R
        },
        Row {
            id: "TOOL-131.11",
            edits: &[
                (
                    "accounts.toml",
                    SPARE,
                    "label = \"spare\"\nkind = \"live\"\nis_prod = false\nuid_policy = \"unverifiable\"\nuid_policy_signoff = \"someone\"\n",
                ),
                (
                    "accounts.toml",
                    SPARE_UID,
                    "venue_uids = { bitget-uta-spot = \"\" }",
                ),
            ],
            msg: "`someone` is not in the Admin list",
            ..R
        },
        Row {
            id: "TOOL-131.12",
            edits: &[(
                "accounts.toml",
                SPARE,
                "label = \"spare\"\nkind = \"live\"\nis_prod = false\nuid_policy = \"Unverifiable\"\nuid_policy_signoff = \"owner1\"\n",
            )],
            msg: "is not one of \"verified\", \"unverifiable\"",
            ..R
        },
        Row {
            id: "TOOL-131.13",
            edits: &[(
                "accounts.toml",
                SPARE_UID,
                "venue_uids = { bitget-uta-spot = \"\" }",
            )],
            msg: "MissingVenueUid",
            ..R
        },
        // spare, pc-live + 10 extra = 12 Live accounts on bitget-uta-spot.
        Row {
            id: "TOOL-131.14",
            append: Some(extra_accounts(10)),
            exit: 0,
            ..R
        },
        Row {
            id: "TOOL-131.15",
            append: Some(extra_accounts(11)),
            msg: "TooManyAccountsPerVenue",
            ..R
        },
        Row {
            id: "TOOL-131.16",
            edits: &[("accounts.toml", SIM, "kind = \"sim\"\nis_prod = true\n")],
            msg: "SimIsProd",
            ..R
        },
        Row {
            id: "TOOL-131.17",
            edits: &[(
                "accounts.toml",
                SIM,
                "kind = \"sim\"\nvenue_uids = { binance-usdm = \"999\" }\n",
            )],
            msg: "SimHasUid",
            ..R
        },
        Row {
            id: "TOOL-131.18",
            edits: &[("accounts.toml", BALANCES, "")],
            msg: "SimCfg",
            ..R
        },
        Row {
            id: "TOOL-131.19",
            edits: &[(
                "accounts.toml",
                "{ venue = \"bitget-uta-spot\", asset = \"USDT\", qty = \"10000\" } ]",
                "{ venue = \"okx-swap\", asset = \"USDT\", qty = \"10000\" } ]",
            )],
            msg: "SimCfg",
            ..R
        },
        Row {
            id: "TOOL-131.20",
            edits: &[("accounts.toml", "fee_tier = 0", "fee_tier = 9")],
            msg: "MissingFee",
            ..R
        },
        Row {
            id: "TOOL-131.21",
            edits: &[("accounts.toml", SIM_SECTION, "fee_tier = 0\nlatency_model = 2")],
            sim_models: true,
            msg: "latency_model = 2",
            ..R
        },
        Row {
            id: "TOOL-131.22",
            edits: &[("accounts.toml", SIM_SECTION, "fee_tier = 0\nlatency_model = 1")],
            sim_models: true,
            exit: 0,
            ..R
        },
        Row {
            id: "TOOL-131.23",
            edits: &[("accounts.toml", SIM_SECTION, "fee_tier = 0\nlatency_model = 2")],
            exit: 0,
            ..R
        },
        Row {
            id: "TOOL-131.24",
            edits: &[(
                "accounts.toml",
                "bound_instance = \"pc-sim-a\"",
                "bound_instance = \"nosuch\"",
            )],
            msg: "DanglingRef",
            ..R
        },
        Row {
            id: "TOOL-131.25",
            edits: &[
                ("instances.toml", "name = \"pc-b\"", "name = \"pc-main\""),
                ("accounts.toml", "bound_instance = \"pc-b\"\n", ""),
            ],
            msg: "Strings",
            ..R
        },
        Row {
            id: "TOOL-131.26",
            edits: &[(
                "accounts.toml",
                "binance-spot = \"5002\"",
                "binance-spot = \"123456789\"",
            )],
            msg: "DuplicateVenueUid",
            ..R
        },
        Row {
            id: "TOOL-131.27",
            edits: &[(
                "accounts.toml",
                "label = \"pc-b-live\"\nkind = \"live\"\nis_prod = true",
                "label = \"pc-b-live\"\nkind = \"live\"\nis_prod = false",
            )],
            append: Some(spare2("binance-usdm")),
            exit: 0,
            ..R
        },
        Row {
            id: "TOOL-131.32",
            edits: &[(
                "accounts.toml",
                "label = \"pc-b-live\"\nkind = \"live\"\nis_prod = true",
                "label = \"pc-b-live\"\nkind = \"live\"\nis_prod = false",
            )],
            append: Some(spare2("binance-spot")),
            msg: "DuplicateVenueUid",
            ..R
        },
        Row {
            id: "TOOL-131.33",
            append: Some(spare2("binance-usdm")),
            msg: "DuplicateVenueUid",
            ..R
        },
        Row {
            id: "TOOL-131.28",
            edits: &[(
                "insts-bitget-uta-spot.toml",
                "asset = \"AIN\"\n",
                "asset = \"AIN\"\nqty_exp = -3\n",
            )],
            msg: "asset AIN",
            ..R
        },
        Row {
            id: "TOOL-131.29",
            edits: &[(
                "assets.toml",
                "px_exp = -8\nqty_exp = -2",
                "px_exp = -4\nqty_exp = -2",
            )],
            exit: 0,
            ..R
        },
        Row {
            id: "TOOL-131.30",
            edits: &[(
                "assets.toml",
                "px_exp = -8\nqty_exp = -2",
                "px_exp = -3\nqty_exp = -2",
            )],
            msg: "NotionalExp",
            ..R
        },
        Row {
            id: "TOOL-131.31",
            edits: &[("accounts.toml", PC_LIVE, "label = \"pc-live\"\nleverage = 5")],
            msg: "unknown key `leverage`",
            ..R
        },
    ];
    for row in &rows {
        let r = build(row, false);
        assert_eq!(r.code, row.exit, "{}: {}{}", row.id, r.stderr, r.stdout);
        let text = if row.exit == 0 { &r.stdout } else { &r.stderr };
        assert!(text.contains(row.msg), "{}: lacks `{}`: {text}", row.id, row.msg);
    }
    // TOOL-131.27: exactly one DuplicateVenueUid warning naming both accounts and the UID, in
    // the text report and in --json `warnings`.
    let row = rows.iter().find(|r| r.id == "TOOL-131.27").unwrap();
    let t = build(row, false);
    assert_eq!(
        t.stdout
            .matches("warning: DuplicateVenueUid accounts pc-b-live, spare2 uid 5002")
            .count(),
        1,
        "{}",
        t.stdout
    );
    let j = build(row, true);
    let v: serde_json::Value = serde_json::from_str(&j.stdout).unwrap();
    let w = v["warnings"].as_array().unwrap();
    assert_eq!(w.len(), 1, "{}", j.stdout);
    assert_eq!(w[0]["code"], "DuplicateVenueUid");
    assert_eq!(w[0]["accounts"], serde_json::json!(["pc-b-live", "spare2"]));
    assert_eq!(w[0]["uid"], "5002");
    // TOOL-131.9: the sign-off is in the --json report as well.
    let row = rows.iter().find(|r| r.id == "TOOL-131.9").unwrap();
    let j = build(row, true);
    let v: serde_json::Value = serde_json::from_str(&j.stdout).unwrap();
    assert_eq!(
        v["signoffs"],
        serde_json::json!([{"account": "spare", "signoff": "owner1"}])
    );
}

/// A non-prod Live account `spare2` (id 5) whose UID on `venue` equals pc-b-live's binance-spot UID.
fn spare2(venue: &str) -> String {
    format!(
        "\n[[account]]\nid = 5\nlabel = \"spare2\"\nkind = \"live\"\nis_prod = false\nvenues = [\"{venue}\"]\nvenue_uids = {{ {venue} = \"5002\" }}\n"
    )
}

/// `refdata-basic` with `edits` built to `dir/<tag>.blob` (`--against` optional).
fn build_basic(
    dir: &std::path::Path,
    tag: &str,
    edits: &[(&str, &str, &str)],
    against: Option<&std::path::Path>,
) -> (Out, std::path::PathBuf) {
    let schemas = dir.join("schemas");
    if !schemas.exists() {
        schema_dir(dir);
    }
    let d = dir.join(tag);
    std::fs::create_dir_all(&d).unwrap();
    for e in std::fs::read_dir(fixtures().join("refdata-basic")).unwrap() {
        let e = e.unwrap();
        std::fs::copy(e.path(), d.join(e.file_name())).unwrap();
    }
    for (file, from, to) in edits {
        let f = d.join(file);
        let t = std::fs::read_to_string(&f).unwrap();
        assert!(t.contains(from), "{tag}: {file}: `{from}` not found");
        std::fs::write(&f, t.replacen(from, to, 1)).unwrap();
    }
    let out = dir.join(format!("{tag}.blob"));
    let mut args = vec![
        "build",
        "--kind",
        "refdata",
        "--in",
        p(&d),
        "--out",
        p(&out),
        "--schemas",
        p(&schemas),
    ];
    if let Some(a) = against {
        args.push("--against");
        args.push(p(a));
    }
    (run(&args), out)
}

#[test]
fn tool_129_roundtrip_and_load() {
    let dir = scratch("tool_129");
    let (r, out) = build_basic(&dir, "rd", &[], None);
    assert_eq!(r.code, 0, "{}", r.stderr);
    let blob = std::fs::read(&out).unwrap();
    let rd = hft_refdata::RefData::load(&blob, None).unwrap();
    // Re-encoding the loaded blob through the same builder (dump → build) gives the same bytes.
    std::fs::write(dir.join("dump.toml"), hft_refdata::dump(&rd)).unwrap();
    let again = dir.join("again.blob");
    let r = run(&[
        "build",
        "--kind",
        "refdata",
        "--in",
        p(&dir.join("dump.toml")),
        "--out",
        p(&again),
    ]);
    assert_eq!(r.code, 0, "{}", r.stderr);
    assert_eq!(std::fs::read(&again).unwrap(), blob);
    let s = run(&["show", p(&out)]);
    assert_eq!(s.code, 0, "{}", s.stderr);
    for want in [
        "[accounts.pc-live]",
        "[accounts.pc-b-live]",
        "[accounts.spare]",
        "[accounts.sim-pc-a]",
        "BITGET_UTA_SPOT = \"987654321\"",
        "[instances.pc-main]",
        "[instances.pc-sim-a]",
        "[insts.\"BINANCE_USDM:AINUSDT\"]",
        "[insts.\"OKX_SWAP:AIN-USDT-SWAP\"]",
        "lot = \"0.01\"",
        "qty_unit_shift = 1",
        "kind_id = 65280",
    ] {
        assert!(s.stdout.contains(want), "show lacks `{want}`");
    }
}

#[test]
fn tool_132_immutables() {
    let dir = scratch("tool_132");
    let v7 = ("assets.toml", "version = 1\n", "version = 7\n");
    let v8 = ("assets.toml", "version = 1\n", "version = 8\n");
    let (r, cur) = build_basic(&dir, "cur", &[v7], None);
    assert_eq!(r.code, 0, "{}", r.stderr);
    let new_inst = (
        "insts-other.toml",
        "[insts.\"okx-swap:AIN-USDT-SWAP\"]",
        "[insts.\"binance-spot:BTCUSDT\"]\nid = 8\nstatus = \"trading\"\nkind = \"spot\"\nasset = \"BTC\"\nquote = \"USDT\"\ntick = \"0.01\"\nlot = \"0.00001\"\nmin_qty = \"0.00001\"\nmax_qty = \"1000\"\nmin_notional = \"5\"\nbook_window = 4096\nl2_depth = 20\n\n[insts.\"okx-swap:AIN-USDT-SWAP\"]",
    );
    let pc_live_sim = (
        "accounts.toml",
        "label = \"pc-live\"\nkind = \"live\"\nis_prod = true\nbound_instance = \"pc-main\"\nvenues = [\"binance-usdm\", \"bitget-uta-spot\"]\nvenue_uids = { binance-usdm = \"123456789\", bitget-uta-spot = \"987654321\" }",
        "label = \"sim-live\"\nkind = \"sim\"\nbound_instance = \"pc-main\"\nvenues = [\"binance-usdm\", \"bitget-uta-spot\"]\n[account.sim]\nfee_tier = 0\nlatency_model = 0\nmargin_model = 0\ninitial_balances = [ { venue = \"binance-usdm\", asset = \"USDT\", qty = \"1\" } ]",
    );
    // The okx-swap inst row (the last InstId) is the tail of insts-other.toml.
    let other = std::fs::read_to_string(fixtures().join("refdata-basic/insts-other.toml")).unwrap();
    let okx_row = &other[other.find("[insts.\"okx-swap:AIN-USDT-SWAP\"]").unwrap()..];
    type Row132<'a> = (&'a str, Vec<(&'a str, &'a str, &'a str)>, i32, &'a str);
    let rows: Vec<Row132<'_>> = vec![
        (
            "TOOL-132.1",
            vec![
                v8,
                (
                    "assets.toml",
                    "px_exp = -8\nqty_exp = -2",
                    "px_exp = -8\nqty_exp = -3",
                ),
            ],
            1,
            "AssetChanged",
        ),
        (
            "TOOL-132.2",
            vec![
                v8,
                ("insts-other.toml", "qty_unit_shift = 1", "qty_unit_shift = 2"),
            ],
            1,
            "MultiplierChanged",
        ),
        ("TOOL-132.3", vec![v8, pc_live_sim], 1, "AccountKindChanged"),
        (
            "TOOL-132.4",
            vec![
                v8,
                (
                    "accounts.toml",
                    "is_prod = true\nbound_instance = \"pc-main\"",
                    "is_prod = false\nbound_instance = \"pc-main\"",
                ),
            ],
            1,
            "AccountKindChanged",
        ),
        (
            "TOOL-132.5",
            vec![
                v8,
                (
                    "accounts.toml",
                    "bitget-uta-spot = \"987654321\"",
                    "bitget-uta-spot = \"987654322\"",
                ),
            ],
            1,
            "UidChanged",
        ),
        (
            "TOOL-132.6",
            vec![
                v8,
                (
                    "instances.toml",
                    "name = \"pc-main\"\nkind = \"probe-taker\"",
                    "name = \"pc-main\"\nkind_id = 65281",
                ),
            ],
            1,
            "InstanceChanged",
        ),
        (
            "TOOL-132.7",
            vec![
                v8,
                ("instances.toml", "name = \"pc-main\"", "name = \"pc-main2\""),
                (
                    "accounts.toml",
                    "bound_instance = \"pc-main\"",
                    "bound_instance = \"pc-main2\"",
                ),
            ],
            1,
            "InstanceChanged",
        ),
        (
            "TOOL-132.8",
            vec![v8, ("insts-other.toml", okx_row, "")],
            1,
            "InstRemoved",
        ),
        ("TOOL-132.9", vec![v7], 0, "AlreadyActive"),
        ("TOOL-132.10", vec![v7, new_inst], 1, "VersionNotIncreasing"),
        ("TOOL-132.11", vec![v8, new_inst], 0, ""),
    ];
    for (id, edits, exit, msg) in rows {
        let (r, out) = build_basic(&dir, id, &edits, Some(&cur));
        assert_eq!(r.code, exit, "{id}: {}{}", r.stderr, r.stdout);
        let text = if exit == 0 { &r.stdout } else { &r.stderr };
        assert!(text.contains(msg), "{id}: lacks `{msg}`: {text}");
        assert_eq!(out.exists(), exit == 0, "{id}");
    }
}

#[test]
fn tool_130_caps_overrides() {
    use hft_types::VenueId;
    use hft_venue_proto::VenueProtocol;
    let dir = scratch("tool_130");
    let caps = std::fs::read_to_string(fixtures().join("refdata-basic/caps.toml")).unwrap();
    let mut value: toml::Value = toml::from_str(&caps).unwrap();
    value["caps"].as_table_mut().unwrap().remove("binance-usdm");
    let defaults = toml::to_string(&value).unwrap();
    let overrides = std::fs::read_to_string(fixtures().join("refdata-basic/venue_caps.toml")).unwrap();
    let base = venue_binance::Usdm::default_caps(|_| None);
    let disabled = format!(
        "flags = {}\nreason = \"disable amend\"",
        base.flags.0 & !hft_refdata::CapFlags::AMEND.0
    );
    let unsupported = format!("flags = {}", base.flags.0 | (1 << 30));
    let events = base
        .ignorable_user_events
        .iter()
        .map(|s| format!("\"{s}\""))
        .collect::<Vec<_>>()
        .join(",");
    let appended = format!("ignorable_user_events = [{events},\"LOCAL_NOTE\"]");
    let cases = [
        ("1", "", 0, ""),
        ("2", disabled.as_str(), 0, ""),
        ("3", unsupported.as_str(), 1, "not supported"),
        ("4", "required_margin_mode = \"cross\"", 1, "reason"),
        ("5", "ignorable_user_events = []", 1, "may only append"),
        ("6", appended.as_str(), 0, ""),
        (
            "7",
            "dms_timeout_ms = 19999\ndms_refresh_ms = 10000\nreason = \"boundary\"",
            1,
            "DeadManRange",
        ),
        (
            "8",
            "dms_timeout_ms = 20000\ndms_refresh_ms = 10000\nreason = \"boundary\"",
            0,
            "",
        ),
        (
            "10",
            "required_spot_borrow = false\nreason = \"invalid perp\"",
            1,
            "CapsMarketMismatch",
        ),
        (
            "11",
            "required_margin_mode = \"mixed\"\nreason = \"invalid mode\"",
            1,
            "CapsMarketMismatch",
        ),
        (
            "12",
            "margin_data = { kind = \"poll\", interval_ms = 0 }\nreason = \"invalid poll\"",
            1,
            "CapsValue",
        ),
    ];
    for (id, over, exit, msg) in cases {
        let over = format!("[venue_caps.binance-usdm]\n{over}\n");
        let (r, out) = build_basic(
            &dir,
            id,
            &[
                ("caps.toml", &caps, &defaults),
                ("venue_caps.toml", &overrides, &over),
            ],
            None,
        );
        assert_eq!(r.code, exit, "{id}: {}{}", r.stderr, r.stdout);
        assert!(r.stderr.contains(msg), "{id}: {}", r.stderr);
        assert_eq!(out.exists(), exit == 0);
        if exit == 0 {
            let blob = std::fs::read(&out).unwrap();
            let rd = hft_refdata::RefData::load(&blob, None).unwrap();
            let c = rd.caps(VenueId::BINANCE_USDM);
            match id {
                "1" => assert_eq!(c, &base),
                "2" => assert_eq!(c.flags.0 & hft_refdata::CapFlags::AMEND.0, 0),
                "6" => assert_eq!(
                    c.ignorable_user_events.last().map(|s| s.as_ref()),
                    Some("LOCAL_NOTE")
                ),
                "8" => assert_eq!((c.dms_timeout_ms, c.dms_refresh_ms), (20_000, 10_000)),
                _ => unreachable!(),
            }
            let dump = dir.join(format!("{id}-dump.toml"));
            std::fs::write(&dump, hft_refdata::dump(&rd)).unwrap();
            let again = dir.join(format!("{id}-again.blob"));
            let r = run(&["build", "--kind", "refdata", "--in", p(&dump), "--out", p(&again)]);
            assert_eq!(r.code, 0, "{}", r.stderr);
            assert_eq!(std::fs::read(again).unwrap(), blob);
        }
    }
    for (tag, invalid) in [
        ("non-table", "[venue_caps]\nbinance-usdm = 3\n"),
        (
            "duplicate-alias",
            "[venue_caps.binance-usdm]\n[venue_caps.BINANCE_USDM]\n",
        ),
    ] {
        let (r, out) = build_basic(
            &dir,
            tag,
            &[
                ("caps.toml", &caps, &defaults),
                ("venue_caps.toml", &overrides, invalid),
            ],
            None,
        );
        assert_eq!(r.code, 1, "{tag}: {}", r.stderr);
        assert!(!out.exists());
    }
    let (r, out) = build_basic(
        &dir,
        "9",
        &[(
            "venue_caps.toml",
            &overrides,
            "[venue_caps.bitget-uta-spot]\nrequired_spot_borrow = true\nreason = \"invalid borrow\"\n",
        )],
        None,
    );
    assert_eq!(r.code, 1);
    assert!(r.stderr.contains("SpotBorrowRequired"), "{}", r.stderr);
    assert!(!out.exists());
}
