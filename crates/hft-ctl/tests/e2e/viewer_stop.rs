//! CTL-205.

use hft_event::CmdResult;
use hft_types::CapSource;

use super::*;

const A: Option<&str> = Some("acct-a");

fn stop() -> Value {
    json!({"type":"Kill","level":"Account"})
}

/// Preview by `tg:1001` at T, confirmation (no `expect_state_version`) `after_ms` later.
fn confirm_after(after_ms: u64) -> CmdResponse {
    let mut r = e2e();
    let p = r.preview("tg:1001", A, stop());
    want(&p, "Applied", None, "preview");
    r.tick_ms(after_ms);
    r.confirm("tg:1001", A, stop(), None)
}

#[test]
fn ctl_205_viewer_emergency_stop() {
    // .1 the preview.
    let mut r = e2e();
    let v = r.version();
    let p = r.preview("tg:1001", A, stop());
    want(&p, "Applied", None, "CTL-205.1");
    let jr = r.jres(p.cmd_id).expect("CTL-205.1 CmdResult");
    assert_eq!(
        jr.rflags,
        CmdResult::PREVIEW | CmdResult::NEEDS_CONFIRM | CmdResult::PREVIEW_TTL,
        "CTL-205.1 rflags"
    );
    let pv = rflags(&p).expect("CTL-205.1 preview block");
    assert_eq!(pv["confirm_within_s"], json!(60), "CTL-205.1 {pv}");
    assert!(!r.has_cap(CapSource::KILL, ACCT_A), "CTL-205.1 cap");
    assert_eq!(r.version(), v, "CTL-205.1 version");
    assert_eq!(p.ctl_state_version, Some(v), "CTL-205.1 version");

    // .2 / .3 the two sides of the TTL.
    want(&confirm_after(60_000), "Applied", None, "CTL-205.2");
    want(&confirm_after(59_999), "Applied", None, "CTL-205.3");

    // .4 16 newer previews evict the oldest.
    let mut r = e2e();
    want(
        &r.preview("tg:1001", A, stop()),
        "Applied",
        None,
        "CTL-205.4 preview",
    );
    for _ in 0..16 {
        let x = r.preview("tg:1005", Some("*"), json!({"type":"Kill","level":"Global"}));
        want(&x, "Applied", None, "CTL-205.4 other previews");
    }
    let c = r.confirm("tg:1001", A, stop(), None);
    want(&c, "ConfirmInvalid", Some("NoValidPreview"), "CTL-205.4");
    assert!(!r.has_cap(CapSource::KILL, ACCT_A), "CTL-205.4 cap");
}
