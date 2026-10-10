//! CTL-215.

use super::*;

#[test]
fn ctl_215_retry_after_gateway_restart() {
    let mut r = e2e();
    let v0 = r.version();
    let req = r.req(
        "cli:tom",
        Some("acct-a"),
        json!({"type":"SetMode","strategy":"s1-main","group":"16777317","mode":"CloseOnly"}),
    );
    let id = uuid(&req);
    want(&r.send_v(req), "Applied", None, "CTL-215 step 1");
    assert_eq!(r.version(), v0 + 1, "CTL-215 step 1");
    // A new gateway: empty de-duplication cache.
    r.restart_gateway();
    let m = r.mark();
    let x = r.result("cli:tom", id);
    want(&x, "Duplicate", None, "CTL-215 step 2");
    assert_eq!(x.status, hft_ctl_proto::TransportStatus::Ok, "CTL-215");
    let cmds = r.kinds_since(m, EventKind::CONTROL_CMD);
    assert_eq!(cmds.len(), 1, "CTL-215: one ControlCmd written");
    let res = r.results_since(m);
    assert_eq!(res.len(), 1, "CTL-215");
    assert_eq!(res[0].cmd_id, *id.as_bytes(), "CTL-215");
    assert_eq!(res[0].status, hft_sequencer::codes::status::DUPLICATE, "CTL-215");
    assert_eq!(r.version(), v0 + 1, "CTL-215 version");
}
