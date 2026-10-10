//! CTL-154, CTL-155, CTL-156.

use std::time::{Duration, Instant};

use super::*;

const A: Option<&str> = Some("acct-a");

fn hedge_fix() -> Value {
    json!({"type":"HedgeFix","group":"16777317"})
}

#[test]
fn ctl_154_processed_next_step() {
    let mut r = e2e();
    let req = r.req("cli:tom", A, hedge_fix());
    let id = uuid(&req);
    let t = r.start(Call::Send(req));
    // The gateway writes the command (no sequencer round yet).
    for _ in 0..1_000 {
        if !r.ctl.borrow().recs.is_empty() {
            break;
        }
        r.gateway_poll();
        r.front.as_mut().expect("gateway").idle_wait();
    }
    let t_w = r.ctl.borrow().last_write_tsc;
    assert!(!r.ctl.borrow().recs.is_empty(), "CTL-154: not written");
    r.adv(10_000);
    let m = r.mark();
    let before = r.now();
    let o = r.s.step();
    let after = r.now();
    assert!(!matches!(o, StepOutcome::Fatal(_)), "CTL-154 {o:?}");
    let cmd = r
        .kinds_since(m, EventKind::CONTROL_CMD)
        .into_iter()
        .find(|x| x.body.get(..16) == Some(id.as_bytes().as_slice()))
        .expect("CTL-154: ControlCmd in this step");
    let res = r.results_since(m);
    assert!(
        res.iter().any(|x| x.cmd_id == *id.as_bytes()),
        "CTL-154: CmdResult in this step"
    );
    assert!(cmd.tsc - t_w <= (after - t_w).max(before - t_w), "CTL-154 tsc");
    assert_eq!(
        cmd.tsc - t_w,
        10_000,
        "CTL-154: taken in the first step after the write"
    );
    let x = r.finish(t);
    assert_eq!(x.status, hft_ctl_proto::TransportStatus::Ok, "CTL-154");
}

#[test]
fn ctl_155_end_to_end_p99() {
    let mut r = e2e();
    let mut lat: Vec<Duration> = Vec::with_capacity(1_000);
    for _ in 0..1_000 {
        // 5 s apart: under `write_rate_per_min = 12`.
        r.adv_ms(5_000);
        let t0 = Instant::now();
        let x = r.send("cli:tom", A, hedge_fix());
        lat.push(t0.elapsed());
        assert_eq!(
            x.status,
            hft_ctl_proto::TransportStatus::Ok,
            "CTL-155 {}",
            show(&x)
        );
    }
    lat.sort();
    let p99 = lat[989];
    assert!(p99 <= Duration::from_millis(50), "CTL-155 p99 {p99:?}");
}

/// 10 s of quotes every 100 ms (each a probe-taker signal on g2) with fills; the journal's
/// `OrderDecision`s of that window.
fn trade_10s(r: &mut E2e) -> usize {
    let m = r.mark();
    for _ in 0..100 {
        r.adv_ms(100);
        r.taker_signal();
        r.quote(I_BN_USDM, "50000", "50001");
        r.quote(I_BG_SPOT, "50000", "50001");
        r.settle();
    }
    r.decisions_since(m).len()
}

#[test]
fn ctl_156_gateway_down_trading_continues() {
    let mut r = e2e();
    let sock = r.gateway_down();
    let down = trade_10s(&mut r);
    let mut c = e2e();
    let up = trade_10s(&mut c);
    assert!(down > 0, "CTL-156: no orders");
    assert_eq!(down, up, "CTL-156");
    #[cfg(target_os = "linux")]
    {
        let p = sock.expect("socket path");
        let e = front::connect_error(&p).expect("CTL-156: the client reached a gateway");
        let refused = matches!(&e, hft_ctl::client::ClientError::Io(io) if io.kind() == std::io::ErrorKind::ConnectionRefused)
            || matches!(e, hft_ctl::client::ClientError::Timeout);
        assert!(refused, "CTL-156 {e:?}");
        if let Some(d) = p.parent() {
            let _ = std::fs::remove_dir_all(d);
        }
    }
    #[cfg(not(target_os = "linux"))]
    let _ = sock;
}
