mod common;
#[allow(dead_code, clippy::all)]
#[path = "../../../crates/hft-refdata/tests/common/f0.rs"]
mod f0;

use common::*;
use hft_refdata::RefData;
use hft_risk::{AcctSpec, BandSpec, GroupLimits, ParamsBuilder, RiskParams};
use hft_types::{AccountId, Notional, Qty, Rate};

#[test]
fn tool_135_roundtrip_and_diff() {
    let dir = scratch("tool-135");
    let refblob = f0::f0();
    let rd = RefData::load(&refblob, None).unwrap();
    let rd_path = dir.join("refdata.blob");
    std::fs::write(&rd_path, refblob).unwrap();
    let mut builder = ParamsBuilder::new(&rd);
    builder.rate_headroom = Rate(100_000_000);
    builder.classes.push(BandSpec {
        max_px_dev: Rate(30_000_000),
        ioc_max_cross: Rate(10_000_000),
        rr_max_cross: None,
    });
    for id in [1, 2] {
        builder.accounts.push((
            AccountId(id),
            AcctSpec {
                max_order_notional: Notional(100_000_000),
                max_account_notional: Notional(1_000_000_000),
                daily_loss_abs: Notional(50_000_000),
                daily_loss_kill: Rate(20_000_000),
                default_group: GroupLimits {
                    max_group_notional: Notional(500_000_000),
                    max_pos_qty: Qty(1_000_000),
                    naked_gate: Rate(10_000_000),
                    naked_gate_min: Notional(1_000_000),
                },
                ..AcctSpec::default()
            },
        ));
    }
    let original = builder.build();
    let old = dir.join("cur.blob");
    std::fs::write(&old, original.encode_blob().unwrap()).unwrap();
    let shown = run(&["show", p(&old)]);
    assert_eq!(shown.code, 0, "{}", shown.stderr);
    let mut doc: toml::Table = shown.stdout.parse().unwrap();
    assert_eq!(doc["classes"][0]["rr_max_cross"].as_integer(), Some(20_000_000));
    assert_eq!(
        doc["accounts"][0]["max_hedge_notional"].as_integer(),
        Some(200_000_000)
    );
    doc["version"] = 2i64.into();
    doc["accounts"][0]["max_order_notional"] = 200_000_000i64.into();
    doc["accounts"][1]["funds_buffer"] = 100_000_000i64.into();
    let input = dir.join("risk.toml");
    let output = dir.join("r.blob");
    std::fs::write(&input, toml::to_string(&doc).unwrap()).unwrap();
    let build = |against: &str| {
        run(&[
            "build",
            "--kind",
            "risk",
            "--in",
            p(&input),
            "--refdata",
            p(&rd_path),
            "--out",
            p(&output),
            "--against",
            against,
            "--json",
        ])
    };
    let got = build(p(&old));
    assert_eq!(got.code, 0, "{}", got.stderr);
    let mut expected = original.clone();
    expected.version = 2;
    expected.accounts.as_mut_slice()[0].max_order_notional = Notional(200_000_000);
    expected.accounts.as_mut_slice()[1].funds_buffer = Rate(100_000_000);
    let bytes = std::fs::read(&output).unwrap();
    assert_eq!(bytes, expected.encode_blob().unwrap());
    assert_eq!(RiskParams::decode_blob(&bytes, &rd).unwrap(), expected);
    let report: serde_json::Value = serde_json::from_str(&got.stdout).unwrap();
    let changes = report["changes"].as_array().unwrap();
    for (path, class) in [
        ("risk.accounts[1].max_order_notional", "loosen"),
        ("risk.accounts[2].funds_buffer", "tighten"),
    ] {
        assert!(
            changes.iter().any(|c| c["path"] == path && c["class"] == class),
            "{report}"
        );
    }
    let shown = run(&["show", p(&output)]);
    assert_eq!(shown.code, 0, "{}", shown.stderr);
    assert_eq!(shown.stdout.parse::<toml::Table>().unwrap(), doc);
    let diff = run(&["diff", "--old", p(&old), "--new", p(&output), "--json"]);
    assert_eq!(diff.code, 0, "{}", diff.stderr);
    assert_eq!(diff.stdout, got.stdout);
    let mut fewer = expected.clone();
    fewer.accounts.truncate(1);
    let removed = hft_cfg::risk::diff(&bytes, &fewer.encode_blob().unwrap()).unwrap();
    assert!(
        removed.json["changes"]
            .as_array()
            .unwrap()
            .iter()
            .all(|c| c["class"] == "changed")
    );

    // Rejection is atomic: unknown fields, missing references, row duplication and excessive
    // classes must never produce a partial blob or silently accept the builder's defaults.
    for mutation in 0..5 {
        let mut bad = doc.clone();
        match mutation {
            0 => {
                bad.insert("typo".into(), 1i64.into());
            }
            1 => {
                bad["accounts"][0]["account"] = 99i64.into();
            }
            2 => {
                bad["venues"].as_array_mut().unwrap().pop();
            }
            3 => {
                let duplicate = bad["accounts"][0].clone();
                bad["accounts"].as_array_mut().unwrap().push(duplicate);
            }
            _ => {
                let band = bad["classes"][0].clone();
                bad["classes"] = toml::Value::Array(vec![band; 9]);
            }
        }
        std::fs::write(&input, toml::to_string(&bad).unwrap()).unwrap();
        let r = build(p(&old));
        assert_eq!(r.code, 1, "mutation {mutation}: {}", r.stderr);
        assert_eq!(std::fs::read(&output).unwrap(), bytes);
    }
    let corrupt = dir.join("bad.blob");
    std::fs::write(&corrupt, &bytes[..bytes.len() - 1]).unwrap();
    assert_eq!(run(&["show", p(&corrupt)]).code, 1);
}
