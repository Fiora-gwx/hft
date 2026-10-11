//! CTL-214.

use hft_types::CapSource;

use super::*;

#[test]
fn ctl_214_expired_in_ring() {
    // Margin data stays fresh over the 30 s pause (no stale-margin cap moves the version).
    let mut r = E2e::with(Opts {
        risk: RiskDoc {
            margin_stale_ms: 120_000,
            ..RiskDoc::start()
        },
        ..Opts::default()
    });
    // The verify replay starts from this keyframe.
    r.sys_timer(1);
    r.settle();
    let v = r.version();
    let req = r.req(
        "cli:tom",
        Some("acct-a"),
        json!({"type":"Kill","level":"Account"}),
    );
    let id = uuid(&req);
    // The gateway writes the command; the sequencer is paused for 30 001 ms.
    r.paused = true;
    let t = r.start(Call::Send(req));
    for _ in 0..100_000 {
        if r.ctl.borrow().recs.len() == 2 {
            break;
        }
        if !r.round() {
            r.front.as_mut().expect("gateway is down").idle_wait();
        }
    }
    assert_eq!(
        r.ctl.borrow().recs.len(),
        2,
        "CTL-214: ControlCmd and audit Note in the ring"
    );
    r.adv_ms(30_001);
    let first = r.finish(t);
    r.paused = false;
    r.settle();
    let res = r.jres(id).expect("CTL-214 CmdResult");
    assert_eq!(res.status, hft_sequencer::codes::status::EXPIRED, "CTL-214");
    assert!(!r.has_cap(CapSource::KILL, ACCT_A), "CTL-214 cap");
    assert_eq!(res.ctl_state_version, v, "CTL-214 version");
    assert_eq!(r.version(), v, "CTL-214 version");
    // The client: `ok`/Expired, or `accepted` first and Expired from `result()`.
    let seen = if first.status == hft_ctl_proto::TransportStatus::Accepted {
        r.result("cli:tom", id)
    } else {
        first
    };
    want(&seen, "Expired", None, "CTL-214 client");
    let m = r.verify();
    assert!(m.is_empty(), "CTL-214 verify: {m:?}");
}
