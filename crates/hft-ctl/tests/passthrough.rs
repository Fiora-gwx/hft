//! CTL-120.
#![allow(
    clippy::type_complexity,
    clippy::disallowed_types,
    clippy::disallowed_methods,
    clippy::disallowed_macros
)] // allow-list: test code, not a hot path

use crate::common::*;
use serde_json::json;

#[test]
fn ctl_120_confirm_fields_passthrough() {
    let rows: [(&str, bool, Option<bool>, Option<u64>, (u8, u8, u64)); 4] = [
        ("CTL-120.1", true, None, None, (1, 0, 0)),
        ("CTL-120.2", false, Some(true), Some(12), (0, 1, 12)),
        ("CTL-120.3", false, None, None, (0, 0, 0)),
        ("CTL-120.4", false, Some(false), Some(5), (0, 0, 5)),
    ];
    for (row, preview, confirmed, esv, want) in rows {
        let mut g = G0::new();
        let mut v = g.req(
            "tg:1002",
            Some("acct-a"),
            json!({"type":"SetMode","strategy":"s1-main","group":"16777317","mode":"Live"}),
        );
        if preview {
            v["preview"] = json!(true);
        }
        if let Some(c) = confirmed {
            v["confirmed"] = json!(c);
        }
        if let Some(e) = esv {
            v["expect_state_version"] = json!(e);
        }
        g.send_and_wait(&v);
        let (h, _) = G0::<CtlTestSet>::cmds(&g.seen)[0].cmd();
        assert_eq!((h.hflags, h.confirmed, h.expect_state_version), want, "{row}");
    }
}
