//! `testing::ev` against engine deliveries (SDK-193).

use hft_event::ControlCmd;
use hft_sequencer::codes::status;
use hft_strategy_sdk::testing::ev;
use hft_strategy_sdk::{EvMask, OwnEvMask, Part};
use hft_types::AccountId;

use super::delivery::{op1, strategy_cmd};
use super::*;
use crate::common::*;

#[test]
fn sdk_193_ev_matches_engine() {
    let b = b_gt3();
    set_subs(S1, vec![(A1, 0, EvMask::BBO, Part::Fast)]);
    set_warmup_both(S1);
    SCRIPT.with(|x| {
        x.borrow_mut()
            .model_own
            .push((S1, OwnEvMask::ORDER.0 | OwnEvMask::EXEC.0 | OwnEvMask::MANUAL.0))
    });
    let mut h = b.boot();
    h.steady();
    clear_log();
    h.quote(A1, "50000", "50002");
    h.run();
    let id = h.cmd_confirmed(AC1, &strategy_cmd(S1, hft_types::GROUP_NONE, 0, &op1(5)));
    h.run();
    assert_eq!(h.res(id).0, status::APPLIED);
    let payload: Vec<u8> = (0..48u8).collect();
    let id = h.cmd(
        AccountId(0),
        &ControlCmd::InjectWarmup {
            instance: S1,
            last: 1,
            local_kind: 1,
            chunk_no: 0,
            len: 48,
            _pad: [0; 6],
            payload: &payload,
        },
    );
    h.run();
    assert_eq!(h.res(id).0, status::APPLIED);
    let f = fast(S1);
    let bbo = f.iter().find(|g| g.kind == "Bbo").expect("Bbo");
    let manual = f.iter().find(|g| g.kind == "Manual").expect("Manual");
    let warm = f.iter().find(|g| g.kind == "Warmup").expect("Warmup");
    let b = bbo.bbo.expect("Bbo view");
    assert_eq!(format!("{:?}", ev::bbo(A1, 0, b).as_ref()), bbo.text);
    assert_eq!(format!("{:?}", ev::manual(1, &[5], false).as_ref()), manual.text);
    assert_eq!(
        format!("{:?}", ev::warmup(1, 0, true, &payload).as_ref()),
        warm.text
    );
}
