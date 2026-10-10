//! Group resets on parameter changes (SDK-085, SDK-086).

use hft_event::{EventKind, Note, NoteSource};
use hft_sequencer::codes::{advisor_note, note, seq_note, status};
use hft_strategy_sdk::{EvMask, EventRef, GroupResets, ModelParamEffect, Part, ResetCause};

use super::*;
use crate::common::*;

fn reset_notes(a: &Adv) -> Vec<Note> {
    a.notes
        .iter()
        .copied()
        .chain(a.ring.notes())
        .filter(|n| n.code == hft_event::note_code(NoteSource::Advisor, advisor_note::MODEL_GROUP_RESET))
        .collect()
}

#[test]
fn sdk_085_reset_notes_per_group() {
    let b = b_gt3();
    // version 5 (`thr = 50`): GT1 and GT3 reset (structural); version 6 (`thr = 60`): all.
    let mut r = GroupResets::new(8);
    r.set(0, ResetCause::Structural);
    r.set(2, ResetCause::Structural);
    SCRIPT.with(|x| {
        let mut x = x.borrow_mut();
        x.reset_effect.push((50, ModelParamEffect::ResetGroups(r)));
        x.reset_effect.push((60, ModelParamEffect::ResetAll));
    });
    let mut h = b.boot();
    h.steady();
    let mut a = adv(&h, MemCkpt::default());
    catch_up(&mut a, &h);
    let n0 = reset_notes(&a).len();
    let mut p5 = gt3(5);
    p5.values[F_THR] = 50;
    let id = h.set_params(&p5);
    h.run();
    assert_eq!(h.res(id), (status::APPLIED, 0));
    catch_up(&mut a, &h);
    let n5: Vec<Note> = reset_notes(&a).into_iter().skip(n0).collect();
    assert_eq!(n5.iter().map(|n| n.group).collect::<Vec<_>>(), vec![GT1, GT3]);
    assert!(
        n5.iter().all(|n| n.strategy == S1 && n.v[0] == 1 && n.v[1] == 5),
        "{n5:?}"
    );
    let mut p6 = gt3(6);
    p6.values[F_THR] = 60;
    let id = h.set_params(&p6);
    h.run();
    assert_eq!(h.res(id), (status::APPLIED, 0));
    catch_up(&mut a, &h);
    let n6: Vec<Note> = reset_notes(&a).into_iter().skip(n0 + 2).collect();
    assert_eq!(
        n6.iter().map(|n| n.group).collect::<Vec<_>>(),
        vec![GT1, GT2, GT3]
    );
    assert!(n6.iter().all(|n| n.v[0] == 4 && n.v[1] == 6), "{n6:?}");
}

#[test]
fn sdk_086_stale_fingerprint_advice_dropped() {
    let mut b = b_gt3();
    // A second FILL perpetual on B: GT3's leg 1 moves to it (fingerprint change).
    let f24 = hft_types::InstId(24);
    b.rd.insts.push(inst_spec(
        f24,
        B,
        "F24_USDT",
        true,
        FILL,
        ("0.01", 1),
        ("0.001", 100_000),
    ));
    set_subs(S1, vec![(A1, 0, EvMask::BBO, Part::Model)]);
    let mut h = b.boot();
    h.steady();
    let mut a = adv(&h, MemCkpt::default());
    catch_up(&mut a, &h);
    set_mhook(|hx, ev, out| {
        if let EventRef::Bbo { .. } = ev
            && hx.s.strategy == S1
        {
            let _ = out.emit_raw(GT1, 1, b"g1");
            let _ = out.emit_raw(GT3, 1, b"g3");
        }
    });
    let m = h.mark();
    h.quote(A1, "50000", "50004");
    h.run();
    let basis = h
        .kinds_since(m, EventKind::BBO)
        .last()
        .map(|r| r.seq)
        .expect("Bbo");
    let n0 = a.ring.recs.len();
    catch_up(&mut a, &h);
    SCRIPT.with(|x| x.borrow_mut().mhook = None);
    assert_eq!(a.ring.advice().len(), 2);
    // The advisor is behind: GT3's leg 1 changes before its Advice is processed.
    let p = Spb::new(
        S1,
        AC1,
        TAKER,
        2,
        vec![
            gspec(GT1),
            gspec_eth(GT2),
            group(GT3, FILL, &[leg(F14, A), leg(f24, B)]),
        ],
    );
    let id = h.set_params(&p);
    h.run();
    assert_eq!(h.res(id), (status::APPLIED, 0));
    clear_log();
    let m = h.mark();
    feed_advice(&a, &mut h, n0);
    let got: Vec<Got> = fast(S1).into_iter().filter(|g| g.kind == "Advice").collect();
    assert_eq!(got.len(), 1, "{got:?}");
    assert_eq!(got[0].gix, 0);
    assert!(got[0].text.contains(&format!("basis_seq: {basis}")));
    let stale: Vec<Note> = h
        .notes_since(m)
        .into_iter()
        .filter(|n| n.source == NoteSource::Sequencer && n.code == seq_note(note::ADVICE_STALE))
        .collect();
    assert_eq!(stale.len(), 1, "{stale:?}");
    assert_eq!(stale[0].group, GT3);
}
