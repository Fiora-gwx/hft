//! `cargo xtask fixtures check | scrub` (tools/TESTS.md TOOL-074; rules: fixtures/README.md 敏感信息清洗).

use crate::common;

use common::Fixture;

const IN: &str = "fixtures/binance-usdm/order_ack.in";

/// A small fixtures tree without secrets (a pointer with a SHA-256, a meta file, an input).
fn fixtures_clean(tag: &str) -> Fixture {
    let f = Fixture::empty(tag);
    f.write(
        "fixtures/replay/x.pointer.toml",
        "uri = \"s3://b/hft-fixtures/x/ba78.zst\"\n\
         sha256 = \"ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad\"\nbytes = 3\n",
    );
    f.write(
        "fixtures/binance-usdm/order_ack.meta.toml",
        "source = \"synthetic\"\nreviewed_by = \"owner\"\nscrubbed = true\n",
    );
    f.write(
        IN,
        "{\"e\":\"ORDER_TRADE_UPDATE\",\"o\":{\"s\":\"BTCUSDT\",\"c\":\"cid-1\"}}\n",
    );
    f.write("fixtures/README.md", "说明:`apiKey`、`signature` 不入库。\n");
    f
}

#[test]
fn tool_074_check_secrets() {
    // TOOL-074.1
    let f = fixtures_clean("fx-1");
    let r = f.xtask(&["fixtures", "check"]);
    assert_eq!(r.code, 0, "TOOL-074.1: {}", r.stderr);
    // TOOL-074.2
    let f = fixtures_clean("fx-2");
    let prefix = "{\"e\":\"listen\",";
    let key = "a".repeat(32) + &"0123456789abcdef".repeat(2);
    f.write(IN, &format!("{prefix}\"apiKey\":\"{key}\",\"x\":1}}\n"));
    let r = f.xtask(&["fixtures", "check"]);
    assert_eq!(r.code, 1, "TOOL-074.2: {}", r.stderr);
    let off = prefix.len();
    assert!(
        r.stderr.contains(&format!("{IN} @ {off}")),
        "TOOL-074.2: want file and offset {off}: {}",
        r.stderr
    );
    let j = r.json.unwrap();
    assert!(
        j["findings"]
            .as_array()
            .unwrap()
            .iter()
            .any(|x| x["file"] == IN && x["offset"] == off)
    );
    // TOOL-074.3
    let s = f.xtask(&["fixtures", "scrub"]);
    assert_eq!(s.code, 0, "TOOL-074.3 scrub: {}", s.stderr);
    assert_eq!(f.read(IN), "{\"e\":\"listen\",\"x\":1}\n");
    let r = f.xtask(&["fixtures", "check"]);
    assert_eq!(r.code, 0, "TOOL-074.3: {}", r.stderr);
}
