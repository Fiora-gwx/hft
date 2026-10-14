use hft_testkit::golden::{GoldenCase, golden_cases, run_decoder_case};
use serde_json::{Value, json};
use std::path::Path;

fn cases() -> Vec<GoldenCase> {
    golden_cases(
        &Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/golden"),
        "stub-venue",
        "bbo",
    )
    .unwrap()
}
fn decode(input: &[u8]) -> Vec<Value> {
    let px: i64 = std::str::from_utf8(input).unwrap().parse().unwrap();
    vec![json!({"kind":"Bbo", "bid_px":px.to_string(), "rx_tsc":123, "hw_rx_ns":456})]
}
#[test]
fn tk_001_loads_cases_by_layout() {
    let c = cases();
    assert_eq!(c.len(), 2);
    assert_eq!(c[0].venue, "stub-venue");
    assert_eq!(c[0].channel, "bbo");
    assert_eq!(c[0].meta["protocol_version"].as_str(), Some("1"));
    assert_eq!(c[0].input.len(), 2);
    assert_eq!(c[0].expected.len(), 2);
    assert_eq!(c[1].input.len(), 1);
    assert!(golden_cases(Path::new("missing-golden-root"), "stub-venue", "bbo").is_err());
}
#[test]
fn tk_002_matching_case_ok() {
    for c in cases() {
        run_decoder_case(&c, decode).unwrap();
    }
}
#[test]
fn tk_003_field_diff_reported() {
    let mut c = cases().remove(0);
    c.expected[1][0]["bid_px"] = json!("6512346");
    let diff = run_decoder_case(&c, decode).unwrap_err();
    assert_eq!((diff.frame, diff.event, diff.field.as_str()), (1, 0, "bid_px"));
    assert!(diff.expected.contains("65123.46"));
    assert!(diff.actual.contains("65123.45"));
}
#[test]
fn tk_004_event_count_mismatch() {
    let mut c = cases().remove(0);
    c.expected[1].clear();
    let diff = run_decoder_case(&c, decode).unwrap_err();
    assert_eq!(
        (diff.frame, diff.event, diff.expected.as_str()),
        (1, 0, "<missing>")
    );
    let mut c = cases().remove(0);
    let extra = c.expected[0][0].clone();
    c.expected[0].push(extra);
    let diff = run_decoder_case(&c, decode).unwrap_err();
    assert_eq!(
        (diff.frame, diff.event, diff.actual.as_str()),
        (0, 1, "<missing>")
    );
}
#[test]
fn tk_005_local_fields_ignored() {
    let mut c = cases().remove(0);
    for frame in &mut c.expected {
        for row in frame {
            row["rx_tsc"] = json!(999);
            row["hw_rx_ns"] = json!(-999);
        }
    }
    run_decoder_case(&c, decode).unwrap();
    c.expected[0][0]["ex_ts_ns"] = json!(1);
    assert_eq!(run_decoder_case(&c, decode).unwrap_err().field, "ex_ts_ns");
}
#[test]
fn tk_006_never_overwrites_expected() {
    // Environment mutation in a multithreaded Rust process is unsafe: exercise bless variables
    // in a child test process instead. Every test file remains byte-identical after failure.
    if std::env::var_os("HFT_GOLDEN_CHILD").is_none() {
        let result = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "golden::tk_006_never_overwrites_expected",
                "--exact",
                "--test-threads=1",
            ])
            .env("HFT_GOLDEN_CHILD", "1")
            .env("UPDATE_EXPECT", "1")
            .env("BLESS", "1")
            .env("INSTA_UPDATE", "always")
            .status()
            .unwrap();
        assert!(result.success());
        return;
    }
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/golden/stub-venue/bbo/a/sample.expected.jsonl");
    let before = std::fs::read(&path).unwrap();
    let mut c = cases().remove(0);
    c.expected[1][0]["bid_px"] = json!("6512346");
    assert!(run_decoder_case(&c, decode).is_err());
    assert_eq!(std::fs::read(path).unwrap(), before);
    let source = include_str!("../src/golden.rs");
    for forbidden in ["fs::write", "File::create", "OpenOptions", "std::env"] {
        assert!(!source.contains(forbidden));
    }
}
