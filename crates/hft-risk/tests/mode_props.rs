//! Mode invariants under arbitrary input sequences (RSK-270).
#![allow(
    clippy::disallowed_types,
    clippy::disallowed_macros,
    clippy::disallowed_methods,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic
)] // allow-list: test code, not a hot path

mod common;
#[path = "model/mode_ref.rs"]
mod mode_ref;

use common::*;
use hft_event::{CapRecord, CmdScope, NoteLevel};
use hft_risk::mode::{
    CellKey, Disposition, GateEvent, KillCmd, ModeErr, ModeInput, ReconItemKey, RecoveryFlags, StorageKind,
    VenueNoticeCmd, mismatch,
};
use hft_risk::snapshot::{ModeRow, SECTION_MODE_TABLE, mode_row_flags};
use hft_risk::{IntentView, Origin};
use hft_types::{
    CapSource, GROUP_ALL, GroupId, InstId, KillLevel, RejectBits, RunMode, Side, StrategyId, VenueId,
};
use proptest::prelude::*;

thread_local! {
    static BASE: W = {
        let mut w = r0();
        w.mv.set_pos(1, B1, "-0.02");
        w
    };
}

const G4: GroupId = GroupId(0x0100_0003);

fn scopes(k: u8) -> CmdScope {
    match k % 8 {
        0 => sc_any(),
        1 => sc_acct(1),
        2 => sc_acct(2),
        3 => sc_av(1, B),
        4 => sc_ag(1, G1),
        5 => sc_cell(1, 1, G1),
        6 => sc_vi(B, B1),
        _ => sc_acct(3),
    }
}
const SOURCES: [CapSource; 15] = [
    CapSource::KILL,
    CapSource::DAILY_LOSS,
    CapSource::RECON,
    CapSource::UNKNOWN_TIMEOUT,
    CapSource::BROKEN,
    CapSource::REORG,
    CapSource::RECOVERY,
    CapSource::HALTED,
    CapSource::DRAINING,
    CapSource::CLOCK_BAD,
    CapSource::HEDGE_FAILURE,
    CapSource::MARGIN_STRESS,
    CapSource::STORAGE,
    CapSource::DELISTING,
    CapSource::ACCOUNT_CFG,
];

/// A generated input (owning the data that `ModeInput` borrows).
#[derive(Clone, Debug)]
enum Gen {
    In(ModeInput<'static>),
    Notice(VenueNoticeCmd),
    Cfg(hft_event::AccountConfigReportHdr),
    Groups(bool),
}

fn gen_input(kind: u8, a: u8, b: u8, c: u8, d: bool, e: bool) -> Gen {
    let modes = [RunMode::Shadow, RunMode::CloseOnly, RunMode::Live];
    let cells = [
        sc_cell(1, 1, G1),
        sc_cell(1, 1, GROUP_ALL),
        sc_cell(2, 2, G2),
        sc_any(),
    ];
    match kind % 14 {
        0 => Gen::In(ModeInput::SetMode {
            scope: cells[usize::from(a % 4)],
            mode: modes[usize::from(b % 3)],
        }),
        1 => {
            let level = [KillLevel::Global, KillLevel::Account, KillLevel::Venue][usize::from(a % 3)];
            let scope = match level {
                KillLevel::Global => sc_any(),
                KillLevel::Account => sc_acct(1 + b % 3),
                KillLevel::Venue => {
                    let mut s = sc_any();
                    s.venue = [A, B, C][usize::from(b % 3)];
                    s.account = acc(c % 3);
                    s
                }
            };
            Gen::In(ModeInput::Kill(KillCmd {
                level,
                scope,
                cancel_resting: c.is_multiple_of(2),
                flatten: d,
                hard: e,
            }))
        }
        2 => Gen::In(ModeInput::Resume {
            source: SOURCES[usize::from(a % 15)],
            scope: scopes(b),
        }),
        3 => {
            let s = SOURCES[usize::from(a % 15)];
            if s == CapSource::DELISTING {
                // Delisting only comes from the VenueNotice command.
                return gen_input(8, 0, b, c, d, e);
            }
            let (scope, level, cf, p) = match s {
                CapSource::KILL => (
                    sc_acct(1),
                    KillLevel::Account as u8,
                    if d { CapRecord::HARD } else { 0 },
                    1,
                ),
                CapSource::DAILY_LOSS => (
                    sc_acct(1 + b % 2),
                    0,
                    0,
                    (-(i64::from(c) + 1) * 1_000_000_000) as u64,
                ),
                CapSource::RECON | CapSource::UNKNOWN_TIMEOUT => (sc_av(1 + b % 2, B), 0, 0, 0),
                CapSource::BROKEN => (sc_ag(1, G1), 0, 0, 0),
                CapSource::REORG | CapSource::RECOVERY | CapSource::HEDGE_FAILURE => {
                    (sc_acct(1 + b % 2), 0, 0, 0)
                }
                CapSource::HALTED => (sc_cell(1, 1, G1), 0, 0, 0),
                CapSource::DRAINING => (sc_any(), 0, CapRecord::NON_CLEARABLE, 1),
                CapSource::CLOCK_BAD => (sc_any(), 0, CapRecord::CONDITIONAL, 0),
                CapSource::MARGIN_STRESS => (sc_acct(1), 1 + b % 3, CapRecord::CONDITIONAL, 1 << B.0),
                CapSource::STORAGE => (sc_any(), 0, CapRecord::CONDITIONAL, 1),
                CapSource::DELISTING => (sc_vi(B, B1), 0, 0, 1),
                _ => (sc_av(1, B), 0, CapRecord::CONDITIONAL, 1 + u64::from(c % 2)),
            };
            Gen::In(ModeInput::Auto(rec(s, level, cf, scope, p)))
        }
        4 => Gen::In(ModeInput::AutoClear {
            source: [
                CapSource::BROKEN,
                CapSource::MARGIN_STRESS,
                CapSource::ACCOUNT_CFG,
            ][usize::from(a % 3)],
            scope: [sc_ag(1, G1), sc_acct(1), sc_av(1, B)][usize::from(a % 3)],
        }),
        5 => Gen::In(ModeInput::Clock { ok: d }),
        6 => Gen::In(ModeInput::Storage {
            kind: [StorageKind::PersistLag, StorageKind::DiskLow, StorageKind::Both][usize::from(a % 3)],
            level: b % 3,
        }),
        7 => Gen::In(ModeInput::StorageRecheck),
        8 => Gen::Notice(VenueNoticeCmd {
            venue: B,
            kind: 1 + a % 3,
            inst: if d { InstId(0) } else { B1 },
            a: 1_700_000_000_000_000_000,
            b: 0,
        }),
        9 => {
            let acct = acc(1 + a % 3);
            Gen::In(ModeInput::Gate(match b % 6 {
                0 => GateEvent::Feeds { account: acct, ok: d },
                1 => GateEvent::OrderConns {
                    account: acct,
                    venue: [A, B, C][usize::from(c % 3)],
                    primary_up: d,
                    standby_up: e,
                },
                2 => GateEvent::Unknown {
                    account: acct,
                    count: u32::from(c % 2),
                },
                3 => GateEvent::Clock { ok: d },
                4 => GateEvent::KillSelfTest { account: acct },
                _ => GateEvent::Disposed {
                    account: acct,
                    venue: B,
                    report_id: u64::from(c),
                    item: ReconItemKey::new(ReconItemKey::POSITION, B1.0),
                    how: Disposition::Adopted,
                },
            }))
        }
        10 => {
            let m = [0, mismatch::MARGIN_MODE, mismatch::UID, mismatch::UNREADABLE][usize::from(a % 4)];
            Gen::Cfg(hdr(1 + b % 2, [A, B][usize::from(c % 2)], m, KEYS))
        }
        11 => Gen::In(ModeInput::CellFlat {
            cell: [cell(1, 1, G1), cell(2, 2, G2)][usize::from(a % 2)],
            flat: d,
        }),
        12 => Gen::Groups(d),
        _ => Gen::In(ModeInput::RecoveryFlags {
            account: acc(1 + a % 3),
            flags: RecoveryFlags(b & 31),
        }),
    }
}

/// Desired-mode rows of the mode table (without the LANDED flag).
fn table(w: &W) -> Vec<(u8, u8, u32, u8, u8)> {
    let bytes = w.snapshot();
    let r = hft_strategy_sdk::SnapshotReader::new(&bytes);
    let s = r.find_section(SECTION_MODE_TABLE).unwrap().unwrap();
    let mut x = s.reader();
    (0..s.count)
        .map(|_| {
            let m: ModeRow = x.get_row().unwrap();
            (
                m.strategy,
                m.account,
                m.group,
                m.mode,
                m.flags & mode_row_flags::DEFAULT,
            )
        })
        .collect()
}

fn rr_intents(w: &W) -> Vec<IntentView> {
    vec![
        hedge(1, G1, B1, Side::Sell, "0.01", "49995.0", "0.01"),
        w.reduce(1, G1, B1, Side::Buy, "0.01", "50005.0"),
        w.flatten(1, G1, B1, Side::Buy, "0.01", "50005.0", Origin::Exec),
    ]
}

const GATED_BITS: [RejectBits; 9] = [
    RejectBits::HARD,
    RejectBits::CLOCK,
    RejectBits::CLOSE_ONLY,
    RejectBits::KILLED,
    RejectBits::DRAINING,
    RejectBits::DAILY_LOSS,
    RejectBits::RECON_DEVIATION,
    RejectBits::VENUE_NOT_READY,
    RejectBits::UNKNOWN_PENDING,
];

fn p1_source(s: CapSource) -> Option<bool> {
    match s {
        CapSource::KILL => None,
        CapSource::DELISTING | CapSource::RECOVERY | CapSource::DRAINING => Some(false),
        _ => Some(true),
    }
}

#[derive(Clone, Debug)]
struct Step {
    input: (u8, u8, u8, u8, bool, bool),
    view: (bool, bool, bool, bool, bool, bool),
}

fn step() -> impl Strategy<Value = Step> {
    (
        (
            any::<u8>(),
            any::<u8>(),
            any::<u8>(),
            any::<u8>(),
            any::<bool>(),
            any::<bool>(),
        ),
        (
            any::<bool>(),
            any::<bool>(),
            any::<bool>(),
            any::<bool>(),
            any::<bool>(),
            any::<bool>(),
        ),
    )
        .prop_map(|(input, view)| Step { input, view })
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(1_000))]
    #[test]
    fn rsk_270_mode_invariants(steps in prop::collection::vec(step(), 300)) {
        let mut w = BASE.with(|b| b.clone());
        let gws = gws_for(w.rd).venues;
        for st in &steps {
            // Random ModeView readings.
            let (flat, naked, recon, daily, unknown, pending) = st.view;
            w.mm.not_flat.clear();
            if !flat {
                w.mm.not_flat.insert(cell(1, 1, G1));
            }
            w.mm.naked = Some(naked);
            w.mm.recon = Some(recon);
            w.mm.daily = Some(daily);
            w.mm.unknown.insert((1, B.0), u32::from(unknown));
            w.mm.pending = pending;
            let (k, a, b, c, d, e) = st.input;
            let g = gen_input(k, a, b, c, d, e);
            let caps0 = w.risk.mode.caps().to_vec();
            let table0 = table(&w);
            let v0 = w.risk.mode.ctl_state_version();
            let snap0 = w.snapshot();
            let bits0: Vec<(u32, bool)> = (1..=3u8).map(|x| (w.risk.mode.gate_bits(acc(x)), w.risk.mode.released(acc(x)))).collect();
            let notice;
            let hdr_v;
            let inp: ModeInput<'_> = match &g {
                Gen::In(i) => *i,
                Gen::Notice(n) => {
                    notice = *n;
                    ModeInput::Notice(&notice)
                }
                Gen::Cfg(h) => {
                    hdr_v = *h;
                    w.risk.on_account_cfg(&hdr_v, &w.levs_for(hdr_v.venue));
                    ModeInput::Gate(GateEvent::AccountCfg { hdr: &hdr_v, keys_required: KEYS })
                }
                Gen::Groups(add) => {
                    let mut specs = w.specs(1);
                    if *add {
                        specs.push(gspec(G4, BTC, &[(A1, A), (C1, C)]));
                    }
                    w.risk.on_group_specs(acc(1), &specs);
                    if *add {
                        ModeInput::GroupsChanged { added: &[CellKey { strategy: StrategyId(1), account: acc(1), group: G4 }], removed: &[] }
                    } else {
                        ModeInput::GroupsChanged { added: &[], removed: &[CellKey { strategy: StrategyId(1), account: acc(1), group: G4 }] }
                    }
                }
            };
            let r = w.apply(inp);
            let caps1 = w.risk.mode.caps().to_vec();
            let v1 = w.risk.mode.ctl_state_version();
            // ④
            if let ModeInput::Kill(kc) = inp {
                let invalid = (kc.hard && kc.level == KillLevel::Venue)
                    || (kc.flatten && kc.level == KillLevel::Global)
                    || (kc.flatten && kc.hard);
                if invalid {
                    prop_assert_eq!(r, Err(ModeErr::InvalidScope), "④ {:?}", kc);
                }
            }
            match r {
                Err(_) => {
                    // ⑪, and nothing changes.
                    prop_assert_eq!(w.snapshot(), snap0, "⑪ {:?}", inp);
                    prop_assert_eq!(v1, v0);
                }
                Ok(()) => {
                    // ①
                    let t1 = table(&w);
                    if !matches!(inp, ModeInput::SetMode { .. } | ModeInput::GroupsChanged { .. }) {
                        prop_assert_eq!(&t1, &table0, "① {:?}", inp);
                    }
                    // ②
                    if let ModeInput::Resume { source, scope } = inp {
                        prop_assert_eq!(caps0.len(), caps1.len() + 1, "② {:?}", inp);
                        let gone: Vec<_> = caps0.iter().filter(|x| !caps1.contains(x)).collect();
                        prop_assert_eq!(gone.len(), 1);
                        prop_assert!(gone[0].source == source && gone[0].scope == scope, "② {:?}", gone[0]);
                    }
                    // ⑤ (Kill: HARD and params bits only grow; DailyLoss: the higher trigger;
                    // AccountCfg: the union of reasons)
                    let merge_target = match inp {
                        ModeInput::Kill(kc) => Some((CapSource::KILL, kc.scope)),
                        ModeInput::Auto(rr) => Some((rr.source, rr.scope)),
                        _ => None,
                    };
                    if let Some((src, sc)) = merge_target
                        && let (Some(old), Some(new)) = (
                            caps0.iter().find(|x| x.source == src && x.scope == sc),
                            caps1.iter().find(|x| x.source == src && x.scope == sc),
                        )
                    {
                        match src {
                            CapSource::KILL => {
                                prop_assert_eq!(new.cflags & old.cflags, old.cflags, "⑤ kill flags");
                                prop_assert_eq!(new.params[0] & old.params[0], old.params[0], "⑤ kill params");
                            }
                            CapSource::DAILY_LOSS => {
                                prop_assert!(new.params[0] as i64 >= old.params[0] as i64, "⑤ daily");
                            }
                            CapSource::ACCOUNT_CFG => {
                                prop_assert_eq!(new.params[0] & old.params[0], old.params[0], "⑤ cfg");
                            }
                            _ => {}
                        }
                    }
                    // ⑥
                    let changed = caps0 != caps1 || t1 != table0;
                    prop_assert_eq!(v1 - v0, u64::from(changed), "⑥ {:?}", inp);
                    // ⑧
                    if let ModeInput::Kill(kc) = inp {
                        // Judged on the merged record: a Kill merging into a HARD record stays HARD.
                        let hard_rec = caps1
                            .iter()
                            .find(|x| x.source == CapSource::KILL && x.scope == kc.scope)
                            .is_some_and(|x| x.cflags & CapRecord::HARD != 0);
                        for cn in w.out.cancels.iter() {
                            prop_assert_eq!(cn.open_only, !hard_rec, "⑧ {:?}", kc);
                        }
                    }
                    // ⑨
                    for nt in w.out.notes.iter() {
                        let n = note_n(nt);
                        prop_assert!([N_CAP_SET, N_CAP_CLEARED, N_RECOVERY_HELD_SIM, N_IDENTITY_UNVERIFIED].contains(&n), "⑨ code {}", n);
                        if n == N_CAP_CLEARED {
                            prop_assert_eq!(nt.level, NoteLevel::Info, "⑨ CapCleared");
                        }
                        if n == N_RECOVERY_HELD_SIM || n == N_IDENTITY_UNVERIFIED {
                            prop_assert_eq!(nt.level, NoteLevel::P1, "⑨");
                        }
                        if n == N_CAP_SET {
                            let src = hft_types::CapSource(nt.v[0] as u8);
                            if let Some(p1) = p1_source(src) {
                                prop_assert_eq!(nt.level == NoteLevel::P1, p1, "⑨ CapSet {:?}", src);
                            }
                        }
                    }
                    // ⑩
                    let mut listed: Vec<u8> = w.out.gate_changes.iter().map(|x| x.account.0).collect();
                    listed.sort_unstable();
                    let before_len = listed.len();
                    listed.dedup();
                    prop_assert_eq!(before_len, listed.len(), "⑩ duplicate");
                    for x in 1..=3u8 {
                        let now_b = (w.risk.mode.gate_bits(acc(x)), w.risk.mode.released(acc(x)));
                        let was = bits0[usize::from(x - 1)];
                        prop_assert_eq!(listed.contains(&x), now_b != was, "⑩ account {}", x);
                        if let Some(gc) = w.out.gate_changes.iter().find(|y| y.account.0 == x) {
                            prop_assert_eq!((gc.old_bits, gc.new_bits, gc.released), (was.0, now_b.0, now_b.1));
                        }
                    }
                    // ⑫
                    prop_assert!(w.out.cells.len() <= w.out.cells.capacity());
                    prop_assert!(w.out.cancels.len() <= w.out.cancels.capacity());
                    prop_assert!(w.out.risk_events.len() <= w.out.risk_events.capacity());
                }
            }
            // ③
            prop_assert_eq!(
                w.risk.mode.kill_bits().iter().map(|x| x.0).collect::<Vec<_>>(),
                mode_ref::kill_bits(&caps1, &gws),
                "③"
            );
            // ⑦
            let hard = caps1.iter().any(|x| x.source == CapSource::KILL && x.cflags & CapRecord::HARD != 0);
            if !hard {
                for it in rr_intents(&w) {
                    if let Err(bit) = w.eval(&it) {
                        prop_assert!(!GATED_BITS.contains(&bit), "⑦ {:?} got {:?}", it.class, bit);
                    }
                }
            }
            let _ = VenueId(0);
        }
    }
}
