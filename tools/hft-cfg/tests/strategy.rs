//! `build --kind strategy` (tools/TESTS.md TOOL-119, TOOL-120, TOOL-124, TOOL-125, TOOL-143,
//! TOOL-182): schema-generic TOML → SPB1, validation, GroupId continuation and `group_ctr_hwm`.

mod common;

use common::schemas::FILE;
use common::strat::*;
use common::*;
use hft_event::parse_spb1;
use hft_strategy_sdk::params::{OwnedSchema, decode_body};

#[test]
fn tool_119_toml_blob_show_roundtrip() {
    let env = Env::new("tool_119");
    let (b, out) = env.ok("a", &base_toml(), None);
    assert_eq!(b[0], 1, "strategy");
    assert_eq!(b[1], 3, "account");
    assert_eq!(hdr_u16(&b, 2), 65280, "kind_id");
    assert_eq!(hdr_u32(&b, 4), 65280, "schema_id");
    assert_eq!(hdr_u16(&b, 8), 2, "schema_ver");
    assert_eq!(b[10], 2, "source = Manual");
    assert_eq!(hdr_u16(&b, 12), 1, "n_groups");
    assert_eq!(hdr_u64(&b, 16), 1, "version");
    let file = std::fs::read_to_string(env.schemas.join(FILE)).unwrap();
    let schema = OwnedSchema::from_json(&file).unwrap();
    assert_eq!(hdr_u64(&b, 40), schema.schema_digest(2), "schema_digest");
    assert_eq!(group_ids(&b), vec![0x0100_0065]);
    let al = hft_cfg::util::aligned(&b);
    let v = parse_spb1(hft_cfg::util::aligned_bytes(&al, b.len())).unwrap();
    let raw = decode_body(&schema, 2, 1, v.body).unwrap();
    assert_eq!(raw.defaults[0], 500_000, "spread_th default raw");
    assert_eq!(raw.overrides.len(), 1);
    assert_eq!(
        (raw.overrides[0].field_ix, raw.overrides[0].value),
        (1, 500_000),
        "max_qty = 5000 at qty_exp -2"
    );

    let r = run(&[
        "show",
        p(&out),
        "--schemas",
        p(&env.schemas),
        "--refdata",
        p(&env.refdata),
    ]);
    assert_eq!(r.code, 0, "{}", r.stderr);
    for want in [
        "spread_th = 0.05%",
        "[[groups]] id = 0x01000065",
        "  max_qty = 5000",
        "legs = [binance-usdm:AINUSDT, bitget-uta-spot:AINUSDT]",
        "asset = AIN",
    ] {
        assert!(r.stdout.contains(want), "missing `{want}` in\n{}", r.stdout);
    }
}

#[test]
fn tool_120_deterministic_encoding() {
    let env = Env::new("tool_120");
    let (a, _) = env.ok("a", &base_toml(), None);
    let (b, _) = env.ok("b", &base_toml(), None);
    assert_eq!(a, b);
    assert_eq!(
        hdr_u64(&a, 24),
        0,
        "generated_at_ns without a TOML value is 0, not the clock"
    );
}

/// One TOOL-124 row: edits of the refdata fixture, of the schema JSON and of the strategy TOML.
struct Row {
    id: &'static str,
    refdata: &'static [(&'static str, &'static str, &'static str)],
    non_prod_only: bool,
    toml: &'static [(&'static str, &'static str)],
    append: &'static str,
    exit: i32,
    msg: &'static str,
}

const R: Row = Row {
    id: "",
    refdata: &[],
    non_prod_only: false,
    toml: &[],
    append: "",
    exit: 1,
    msg: "",
};

const N101: &str = "n = 101";

#[test]
fn tool_124_validation_counterexamples() {
    let rows = [
        Row {
            id: "TOOL-124.1",
            refdata: &[("accounts.toml", "bound_instance = \"pc-main\"\n", "")],
            msg: "AccountBound",
            ..R
        },
        Row {
            id: "TOOL-124.2",
            refdata: &[
                ("accounts.toml", "bound_instance = \"pc-b\"\n", ""),
                (
                    "accounts.toml",
                    "bound_instance = \"pc-main\"",
                    "bound_instance = \"pc-b\"",
                ),
            ],
            msg: "AccountBound",
            ..R
        },
        Row {
            id: "TOOL-124.3",
            toml: &[("id = 65280, version = 2", "id = 65281, version = 1")],
            msg: "KindMismatch",
            ..R
        },
        Row {
            id: "TOOL-124.4",
            non_prod_only: true,
            msg: "NonProdOnly",
            ..R
        },
        Row {
            id: "TOOL-124.5",
            non_prod_only: true,
            refdata: &[(
                "accounts.toml",
                "is_prod = true\nbound_instance = \"pc-main\"",
                "is_prod = false\nbound_instance = \"pc-main\"",
            )],
            exit: 0,
            ..R
        },
        Row {
            id: "TOOL-124.6",
            append: "cooldown_ms = \"500\"\n",
            msg: "NotPerGroup (field `cooldown_ms`",
            ..R
        },
        Row {
            id: "TOOL-124.7",
            toml: &[
                ("id = 65280, version = 2", "id = 65280, version = 1"),
                ("[defaults]\n", "[defaults]\nmode = \"b\"\n"),
            ],
            msg: "field `mode` does not exist in schema version 1",
            ..R
        },
        Row {
            id: "TOOL-124.8",
            toml: &[("[defaults]\n", "[defaults]\nmax_qty = \"5000\"\n")],
            msg: "max_qty",
            ..R
        },
        Row {
            id: "TOOL-124.9",
            toml: &[("[defaults]\n", "[defaults]\nno_such = \"1\"\n")],
            msg: "unknown field `no_such`",
            ..R
        },
        Row {
            id: "TOOL-124.10",
            toml: &[("[defaults]\n", "[defaults]\ncooldown_ms = \"60000\"\n")],
            exit: 0,
            ..R
        },
        Row {
            id: "TOOL-124.11",
            toml: &[("[defaults]\n", "[defaults]\ncooldown_ms = \"60001\"\n")],
            msg: "OutOfRange: field `cooldown_ms`",
            ..R
        },
        Row {
            id: "TOOL-124.12",
            toml: &[("spread_th = \"0.0005\"", "spread_th = \"0\"")],
            exit: 0,
            ..R
        },
        Row {
            id: "TOOL-124.13",
            toml: &[("spread_th = \"0.0005\"", "spread_th = \"-0.000000001\"")],
            msg: "OutOfRange: field `spread_th`",
            ..R
        },
        Row {
            id: "TOOL-124.14",
            toml: &[("[defaults]\n", "[defaults]\nmode = \"c\"\n")],
            msg: "BadEnum: field `mode`",
            ..R
        },
        Row {
            id: "TOOL-124.15",
            toml: &[("max_qty = \"5000\"", "max_qty = \"5000.001\"")],
            msg: "parse_exact",
            ..R
        },
        Row {
            id: "TOOL-124.16",
            toml: &[("binance-usdm:AINUSDT", "binance-usdm:NOSUCH")],
            msg: "LegNotTradable",
            ..R
        },
        Row {
            id: "TOOL-124.17",
            refdata: &[(
                "insts-binance-usdm.toml",
                "status = \"trading\"",
                "status = \"halted\"",
            )],
            msg: "LegNotTradable",
            ..R
        },
        Row {
            id: "TOOL-124.18",
            refdata: &[(
                "insts-binance-usdm.toml",
                "status = \"trading\"",
                "status = \"reduce_only\"",
            )],
            exit: 0,
            ..R
        },
        Row {
            id: "TOOL-124.19",
            toml: &[("binance-usdm:AINUSDT", "okx-swap:AIN-USDT-SWAP")],
            msg: "LegVenueNotInAccount",
            ..R
        },
        Row {
            id: "TOOL-124.20",
            toml: &[("binance-usdm:AINUSDT", "binance-usdm:BTCUSDT")],
            msg: "LegAssetMismatch",
            ..R
        },
        Row {
            id: "TOOL-124.21",
            toml: &[("bitget-uta-spot:AINUSDT", "bitget-uta-spot:AINUSDC")],
            msg: "QuoteMismatch",
            ..R
        },
        Row {
            id: "TOOL-124.22",
            toml: &[("bitget-uta-spot:AINUSDT", "binance-spot:AINUSDT")],
            msg: "LegVenueNotInAccount",
            ..R
        },
        Row {
            id: "TOOL-124.23",
            toml: &[(N101, "n = 0")],
            msg: "n = 0",
            ..R
        },
        Row {
            id: "TOOL-124.24",
            toml: &[(N101, "n = 1")],
            exit: 0,
            ..R
        },
        Row {
            id: "TOOL-124.25",
            toml: &[(N101, "n = 16777214")],
            exit: 0,
            ..R
        },
        Row {
            id: "TOOL-124.26",
            toml: &[(N101, "n = 16777215")],
            msg: "n = 16777215",
            ..R
        },
        Row {
            id: "TOOL-124.27",
            append: "\n[[groups]]\nn = 101\nasset = \"AIN\"\nopen_allowed = true\nlegs = [ { inst = \"binance-usdm:AINUSDT\" }, { inst = \"bitget-uta-spot:AINUSDT\" } ]\n",
            msg: "duplicate GroupId 0x01000065",
            ..R
        },
        Row {
            id: "TOOL-124.28",
            toml: &[("account = \"pc-live\"", "account = \"nosuch\"")],
            msg: "DanglingRef account",
            ..R
        },
    ];
    for row in rows {
        let env = Env::with_refdata(&format!("tool_124_{}", row.id), row.refdata);
        if row.non_prod_only {
            let f = env.schemas.join(FILE);
            let t = std::fs::read_to_string(&f).unwrap();
            std::fs::write(&f, t.replace("\"non_prod_only\":false", "\"non_prod_only\":true")).unwrap();
        }
        let mut toml = base_toml();
        for (from, to) in row.toml {
            toml = toml.replacen(from, to, 1);
        }
        toml.push_str(row.append);
        let (r, out) = env.build("s", &toml, None);
        assert_eq!(r.code, row.exit, "{}: {}", row.id, r.stderr);
        if row.exit == 0 {
            let b = std::fs::read(&out).unwrap();
            match row.id {
                "TOOL-124.24" => assert_eq!(group_ids(&b), vec![0x0100_0001], "{}", row.id),
                "TOOL-124.25" => assert_eq!(group_ids(&b), vec![0x01FF_FFFE], "{}", row.id),
                _ => {}
            }
        } else {
            assert!(!out.exists(), "{}: no blob", row.id);
            assert!(
                r.stderr.contains(row.msg),
                "{}: stderr lacks `{}`: {}",
                row.id,
                row.msg,
                r.stderr
            );
        }
    }
}

/// The blob in effect: version 1 with `groups`, then version 7 against it with `kept`.
fn current(env: &Env, tag: &str, first: &[String], kept: &[String]) -> std::path::PathBuf {
    let v1 = toml_with(1, first);
    let (_, p1) = env.ok(&format!("{tag}-v1"), &v1, None);
    let v7 = toml_with(7, kept);
    env.ok(&format!("{tag}-v7"), &v7, Some(&p1)).1
}

#[test]
fn tool_125_groupid_continuation() {
    let env = Env::new("tool_125");
    let g = |n, legs| group(n, if legs == BTC_LEGS { "BTC" } else { "AIN" }, legs);
    let rev = [BTC_LEGS[1], BTC_LEGS[0]];
    let three = [g(103, BTC_LEGS), g(105, AIN_LEGS)];
    let cur = current(&env, "cur", &three, &three);
    let cur_b = std::fs::read(&cur).unwrap();
    assert_eq!((hdr_u64(&cur_b, 16), hwm(&cur_b)), (7, 105));
    assert_eq!(group_ids(&cur_b), vec![0x0100_0065, 0x0100_0067, 0x0100_0069]);
    let open_false = three[0].replace("open_allowed = true", "open_allowed = false");
    let cases: Vec<(&str, Vec<String>, i32, &str)> = vec![
        (
            "TOOL-125.1",
            vec![three[0].clone(), three[1].clone(), g(106, AIN_LEGS)],
            0,
            "",
        ),
        (
            "TOOL-125.2",
            vec![three[0].clone(), three[1].clone(), g(104, AIN_LEGS)],
            1,
            "n = 104 is not greater than H = 105",
        ),
        (
            "TOOL-125.3",
            vec![three[0].clone(), three[1].clone(), g(100, AIN_LEGS)],
            1,
            "n = 100 is not greater than H = 105",
        ),
        (
            "TOOL-125.4",
            vec![group(103, "BTC", rev), three[1].clone()],
            1,
            "GroupId 0x01000067: fingerprint",
        ),
        ("TOOL-125.5", vec![open_false, three[1].clone()], 0, ""),
        // TOOL-125.6 in one step: dropping 103 and adding n = 103 with other legs is the same
        // TOML as changing 103's legs, so the fingerprint rule rejects it.
        (
            "TOOL-125.6",
            vec![group(103, "BTC", rev), three[1].clone()],
            1,
            "GroupId 0x01000067: fingerprint",
        ),
    ];
    for (id, groups, exit, msg) in cases {
        let (r, out) = env.build(id, &toml_with(8, &groups), Some(&cur));
        assert_eq!(r.code, exit, "{id}: {}", r.stderr);
        assert!(r.stderr.contains(msg), "{id}: {}", r.stderr);
        assert_eq!(out.exists(), exit == 0, "{id}");
    }
    // TOOL-125.6 in two steps: 103 removed first, then n = 103 re-added with other legs.
    let removed = env
        .ok("TOOL-125.6-rm", &toml_with(8, &[three[1].clone()]), Some(&cur))
        .1;
    let (r, _) = env.build(
        "TOOL-125.6-readd",
        &toml_with(9, &[three[1].clone(), group(103, "BTC", rev)]),
        Some(&removed),
    );
    assert_eq!(r.code, 1, "TOOL-125.6: {}", r.stderr);
    assert!(
        r.stderr.contains("n = 103 is not greater than H = 105"),
        "TOOL-125.6: {}",
        r.stderr
    );
    // TOOL-125.7 (journal directory as --against) needs hft-journal; a directory is refused with exit 2.
    // TOOL-125.8
    let (r, out) = env.build("TOOL-125.8", &toml_with(8, &[g(104, AIN_LEGS)]), None);
    assert_eq!(r.code, 2, "TOOL-125.8: {}", r.stderr);
    assert!(!out.exists());
    // TOOL-125.9: in effect 101, 103 (105 removed earlier), hwm 105.
    let cur9 = current(&env, "cur9", &three, &[three[0].clone()]);
    let c9 = std::fs::read(&cur9).unwrap();
    assert_eq!((group_ids(&c9), hwm(&c9)), (vec![0x0100_0065, 0x0100_0067], 105));
    let (r, _) = env.build(
        "TOOL-125.9",
        &toml_with(8, &[three[0].clone(), g(105, AIN_LEGS)]),
        Some(&cur9),
    );
    assert_eq!(r.code, 1, "TOOL-125.9: {}", r.stderr);
    assert!(
        r.stderr.contains("n = 105 is not greater than H = 105"),
        "TOOL-125.9: {}",
        r.stderr
    );
}

#[test]
fn tool_143_decimal_without_f64() {
    let env = Env::new("tool_143");
    let toml = toml_with(
        1,
        &[group(102, "BTC", BTC_LEGS) + "[groups.overrides]\nmax_qty = \"0.30000001\"\n"],
    );
    let (b, out) = env.ok("s", &toml, None);
    let schema = OwnedSchema::from_json(&std::fs::read_to_string(env.schemas.join(FILE)).unwrap()).unwrap();
    let al = hft_cfg::util::aligned(&b);
    let v = parse_spb1(hft_cfg::util::aligned_bytes(&al, b.len())).unwrap();
    let raw = decode_body(&schema, 2, 2, v.body).unwrap();
    let o = raw
        .overrides
        .iter()
        .find(|o| o.group_ix == 1 && o.field_ix == 1)
        .unwrap();
    assert_eq!(o.value, 30_000_001, "0.30000001 at qty_exp -8");
    assert!(out.exists());
}

#[test]
fn tool_182_group_ctr_hwm() {
    let env = Env::new("tool_182");
    let g = |n| group(n, "AIN", AIN_LEGS);
    let g103 = group(103, "BTC", BTC_LEGS);
    // In effect: 101, 103 with hwm 110 (105–110 removed in earlier versions).
    let cur = current(
        &env,
        "cur",
        &[g103.clone(), g(105), g(110)],
        std::slice::from_ref(&g103),
    );
    let c = std::fs::read(&cur).unwrap();
    assert_eq!(
        (hdr_u64(&c, 16), group_ids(&c), hwm(&c)),
        (7, vec![0x0100_0065, 0x0100_0067], 110)
    );
    type Row182 = (&'static str, Vec<String>, i32, Option<u32>, &'static str);
    let rows: Vec<Row182> = vec![
        ("TOOL-182.1", vec![g103.clone()], 0, Some(110), ""),
        (
            "TOOL-182.2",
            vec![g103.clone(), g(110)],
            1,
            None,
            "n = 110 is not greater than H = 110",
        ),
        ("TOOL-182.3", vec![g103.clone(), g(111)], 0, Some(111), ""),
        (
            "TOOL-182.4",
            vec![g103.clone(), g(109)],
            1,
            None,
            "n = 109 is not greater than H = 110",
        ),
        ("TOOL-182.5", vec![], 0, Some(110), ""),
    ];
    for (id, groups, exit, want_hwm, msg) in rows {
        let (r, out) = env.build(id, &toml_with(8, &groups), Some(&cur));
        assert_eq!(r.code, exit, "{id}: {}", r.stderr);
        assert!(r.stderr.contains(msg), "{id}: {}", r.stderr);
        if let Some(h) = want_hwm {
            let b = std::fs::read(&out).unwrap();
            assert_eq!(hwm(&b), h, "{id}");
            let s = run(&["show", p(&out)]);
            assert!(
                s.stdout.contains(&format!("group_ctr_hwm = {h}")),
                "{id}: {}",
                s.stdout
            );
        } else {
            assert!(!out.exists(), "{id}");
        }
    }
    // TOOL-182.6: a blob in effect whose group_ctr_hwm (0) is below its counters fails the SPB1
    // container check (BadGroup) and is refused as --against.
    let mut bad = c.clone();
    bad[36..40].copy_from_slice(&0u32.to_le_bytes());
    let badp = env.dir.join("bad.blob");
    std::fs::write(&badp, &bad).unwrap();
    let (r, out) = env.build("TOOL-182.6", &toml_with(8, &[g103.clone(), g(102)]), Some(&badp));
    assert_eq!(r.code, 1, "TOOL-182.6: {}", r.stderr);
    assert!(r.stderr.contains("BadGroup"), "TOOL-182.6: {}", r.stderr);
    assert!(!out.exists());
    // TOOL-182.7
    let first = base_toml().replace("n = 101", "n = 1") + &g(2);
    let (b, _) = env.ok("TOOL-182.7", &first, None);
    assert_eq!(hwm(&b), 2, "TOOL-182.7");
    // TOOL-182.8
    let (r, out) = env.build("TOOL-182.8", &toml_with(2, &[]), None);
    assert_eq!(r.code, 2, "TOOL-182.8: {}", r.stderr);
    assert!(!out.exists());
    // TOOL-182.9
    let t = toml_with(8, &[g103.clone(), g(111)]);
    let (a, _) = env.ok("TOOL-182.9a", &t, Some(&cur));
    let (b, _) = env.ok("TOOL-182.9b", &t, Some(&cur));
    assert_eq!(a, b, "TOOL-182.9");
    assert_eq!(hwm(&a), 111);
}
