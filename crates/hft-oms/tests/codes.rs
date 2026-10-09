#![allow(
    clippy::disallowed_types,
    clippy::disallowed_macros,
    clippy::disallowed_methods
)] // allow-list: test code
#![allow(clippy::type_complexity, clippy::too_many_arguments)]
use hft_oms::*;

#[test]
fn oms_018_order_state_codes() {
    use OrderState::*;
    let all = [
        Free,
        PendingNew,
        Live,
        PendingCancel,
        PendingAmend,
        Filled,
        Cancelled,
        Rejected,
        Unknown,
    ];
    for (i, s) in all.iter().enumerate() {
        assert_eq!(OrderState::try_from(i as u8), Ok(*s), "OMS-018.1");
        assert_eq!(*s as u8, i as u8, "OMS-018.1");
    }
    assert!(OrderState::try_from(9).is_err(), "OMS-018.2");
    assert!(OrderState::try_from(255).is_err(), "OMS-018.3");
}

#[test]
fn oms_019_transition_cause_codes() {
    use TransitionCause::*;
    let all = [
        Place,
        CancelReq,
        AmendReq,
        Ack,
        Reject,
        Fill,
        CancelAck,
        Expired,
        CancelReject,
        AmendAck,
        AmendReject,
        QueryResult,
        Reconcile,
        OutcomeUnknown,
        ConnDown,
        Timeout,
        LocalSendFail,
        WriteError,
        ReportDecode,
        RecoveryEnd,
        ForceUnknown,
        NotFoundExpire,
        Recycle,
        SimReset,
        SmokeTtl,
        FeeOnly,
    ];
    for (i, c) in all.iter().enumerate() {
        let v = i as u8 + 1;
        assert_eq!(TransitionCause::try_from(v), Ok(*c), "OMS-019.1");
        assert_eq!(*c as u8, v, "OMS-019.1");
    }
    assert!(TransitionCause::try_from(0).is_err(), "OMS-019.2");
    assert!(TransitionCause::try_from(27).is_err(), "OMS-019.3");
}

#[test]
fn oms_020_link_broken_codes() {
    use LinkBroken::*;
    let all = [
        None,
        RetryExhausted,
        Deadline,
        HedgeLimit,
        Overfill,
        ReverseFailed,
        ResumeTimeout,
    ];
    for (i, c) in all.iter().enumerate() {
        assert_eq!(LinkBroken::try_from(i as u8), Ok(*c), "OMS-020.1");
        assert_eq!(*c as u8, i as u8, "OMS-020.1");
    }
    assert!(LinkBroken::try_from(7).is_err(), "OMS-020.2");
}
