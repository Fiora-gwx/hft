//! CTL-160.
#![allow(
    clippy::type_complexity,
    clippy::disallowed_types,
    clippy::disallowed_methods,
    clippy::disallowed_macros
)] // allow-list: test code, not a hot path

use crate::common::*;
use hft_ctl::ParsedConfig;
use serde_json::json;

#[test]
fn ctl_160_generic_over_strategy_set() {
    let sp = json!({"type":"SetParams","strategy":"s1-main","blob_b64":b64(&Spb::s1(42).bytes())});
    // The full set.
    let mut g = G0::new();
    assert_eq!(
        code(&g.send(
            "cli:tom",
            None,
            json!({"type":"GetSchema","strategy_kind":"probe-taker"})
        )),
        "Applied"
    );
    assert_eq!(code(&g.send("cli:tom", Some("acct-a"), sp.clone())), "Applied");
    // A set with probe-taker only.
    let active: Vec<ParsedConfig<CtlTestSetMin>> = Vec::new();
    let mut m = G0::<CtlTestSetMin>::with(p0::p0(), 262_144, active);
    assert_eq!(
        code(&m.send(
            "cli:tom",
            None,
            json!({"type":"GetSchema","strategy_kind":"probe-taker"})
        )),
        "Applied"
    );
    assert_eq!(code(&m.send("cli:tom", Some("acct-a"), sp)), "Applied");
    let r = m.send("cli:tom", None, json!({"type":"GetSchema","kind_id":"65282"}));
    assert_eq!(
        (code(&r), detail_name(&r).as_str()),
        ("InvalidArgs", "UnknownName")
    );
}
