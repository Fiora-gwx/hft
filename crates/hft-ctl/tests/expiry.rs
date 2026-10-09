//! CTL-115.
#![allow(
    clippy::type_complexity,
    clippy::disallowed_types,
    clippy::disallowed_methods,
    clippy::disallowed_macros
)] // allow-list: test code, not a hot path

use crate::common::*;
use serde_json::json;

#[test]
fn ctl_115_expired_on_arrival() {
    let rows: [(&str, i64, bool); 3] = [
        ("CTL-115.1", -1, true),
        ("CTL-115.2", 0, true),
        ("CTL-115.3", 1, false),
    ];
    for (row, off, written) in rows {
        let mut g = G0::new();
        let mut v = g.req(
            "cli:tom",
            Some("acct-a"),
            json!({"type":"HedgeFix","group":"16777317"}),
        );
        v["issued_at_ns"] = json!(UTC0);
        v["expires_at_ns"] = json!(UTC0 + 30 * S as i64);
        g.at((30 * S as i64 + off) as u64);
        let r = g.send_and_wait(&v);
        g.take_ring();
        let recs = g.seen.clone();
        if written {
            let (h, _) = G0::<CtlTestSet>::cmds(&recs)[0].cmd();
            assert_eq!(h.expires_tsc.0, 30_000_000_000, "{row}");
        } else {
            assert_eq!((status(&r).as_str(), code(&r)), ("ok", "Expired"), "{row}");
            assert!(G0::<CtlTestSet>::cmds(&recs).is_empty(), "{row}");
            assert_eq!(only_audit(&recs).result_code, 3, "{row}");
        }
    }
}
