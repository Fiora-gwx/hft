//! Timer code packing (EVT-110 – EVT-113).
#![allow(clippy::type_complexity)]

use hft_event::*;
use hft_types::*;
use proptest::prelude::*;

const G: GroupId = GroupId(0x0300_0001);

fn v(n: u8) -> VenueId {
    VenueId(n)
}
fn a(n: u8) -> AccountId {
    AccountId(n)
}

#[test]
fn evt_110_timer_encode_table() {
    use SysTimer::*;
    let n = GROUP_NONE;
    let rows: Vec<(&str, TimerCode, (u16, u32, GroupId))> = vec![
        ("EVT-110.1", TimerCode::Sys(Snapshot), (1, 1, n)),
        ("EVT-110.2", TimerCode::Sys(Roll), (1, 2, n)),
        (
            "EVT-110.3",
            TimerCode::Sys(DayRoll { day: 20_000 }),
            (1, 0x004E_2003, n),
        ),
        (
            "EVT-110.4",
            TimerCode::Sys(DeadMan {
                venue: v(1),
                account: a(3),
            }),
            (1, 0x0003_0104, n),
        ),
        (
            "EVT-110.5",
            TimerCode::Sys(ConnRotate {
                venue: v(5),
                account: a(0),
            }),
            (1, 0x0000_0505, n),
        ),
        ("EVT-110.6", TimerCode::Sys(AdvisorTick), (1, 6, n)),
        ("EVT-110.7", TimerCode::Sys(SnapshotNow), (1, 7, n)),
        ("EVT-110.8", TimerCode::Sys(ClockOk), (1, 8, n)),
        ("EVT-110.8", TimerCode::Sys(ClockBad), (1, 9, n)),
        ("EVT-110.9", TimerCode::Sys(ConflateReport), (1, 0x0A, n)),
        (
            "EVT-110.10",
            TimerCode::Sys(SessionKeepalive {
                venue: v(2),
                account: a(4),
            }),
            (1, 0x0004_020B, n),
        ),
        (
            "EVT-110.11",
            TimerCode::Sys(PersistLag { level: 1 }),
            (1, 0x010C, n),
        ),
        ("EVT-110.12", TimerCode::Sys(DiskLow { level: 2 }), (1, 0x020D, n)),
        ("EVT-110.13", TimerCode::Sys(StorageOk), (1, 0x0E, n)),
        (
            "EVT-110.14",
            TimerCode::Sys(FundingSettle { venue: v(10) }),
            (1, 0x0A0F, n),
        ),
        (
            "EVT-110.15",
            TimerCode::Sys(KillSelfTestOk { account: a(7) }),
            (1, 0x0007_0010, n),
        ),
        ("EVT-110.16", TimerCode::Sys(NotifyContinue), (1, 0x11, n)),
        (
            "EVT-110.17",
            TimerCode::Sys(RefdataRecheck { arg: 0 }),
            (1, 0x12, n),
        ),
        (
            "EVT-110.17",
            TimerCode::Sys(RefdataRecheck { arg: 5 }),
            (1, 0x0512, n),
        ),
        (
            "EVT-110.18",
            TimerCode::Keyframe {
                inst: InstId(123_456),
            },
            (2, 123_456, n),
        ),
        ("EVT-110.19", TimerCode::Oms(OmsTimer::Continue), (3, 1, n)),
        ("EVT-110.19", TimerCode::Oms(OmsTimer::Sweep), (3, 2, n)),
        (
            "EVT-110.20",
            TimerCode::Oms(OmsTimer::UnknownQuery {
                table_ix: 7,
                slot: 1_000,
            }),
            (3, 0x03E8_0703, n),
        ),
        (
            "EVT-110.21",
            TimerCode::Oms(OmsTimer::HedgeRetry { link_id: 0x12_3456 }),
            (3, 0x1234_5604, n),
        ),
        (
            "EVT-110.22",
            TimerCode::Oms(OmsTimer::SmokeTtl {
                account: a(2),
                venue: v(6),
            }),
            (3, 0x0006_0205, n),
        ),
        (
            "EVT-110.25",
            TimerCode::Exec {
                kind: 1,
                account: a(2),
                group: n,
            },
            (5, 0x0201, n),
        ),
        (
            "EVT-110.26",
            TimerCode::Strategy {
                strategy: StrategyId(3),
                tag: 0xDEAD_BEEF,
                group: GroupId(0x0300_0005),
            },
            (19, 0xDEAD_BEEF, GroupId(0x0300_0005)),
        ),
        (
            "EVT-110.27",
            TimerCode::Strategy {
                strategy: StrategyId(1),
                tag: 0,
                group: n,
            },
            (17, 0, n),
        ),
        (
            "EVT-110.27",
            TimerCode::Strategy {
                strategy: StrategyId(254),
                tag: 0,
                group: n,
            },
            (270, 0, n),
        ),
    ];
    for (row, c, want) in rows {
        assert_eq!(c.encode(), Ok(want), "{row}");
    }
    for k in 1..=3u8 {
        let c = TimerCode::Risk {
            kind: k,
            account: a(3),
            venue: v(1),
        };
        assert_eq!(
            c.encode(),
            Ok((4, 0x0001_0300 | u32::from(k), GROUP_NONE)),
            "EVT-110.23"
        );
    }
    for k in 1..=6u8 {
        let c = TimerCode::Exec {
            kind: k,
            account: a(2),
            group: G,
        };
        assert_eq!(c.encode(), Ok((5, 0x0200 | u32::from(k), G)), "EVT-110.24");
    }
}

fn arb_group() -> impl Strategy<Value = GroupId> {
    prop_oneof![
        Just(GROUP_NONE),
        (1u8..=254, 1u32..=GroupId::N_MAX).prop_map(|(s, n)| GroupId::compose(StrategyId(s), n).unwrap())
    ]
}

fn arb_sys() -> impl Strategy<Value = SysTimer> {
    use SysTimer::*;
    let va = (any::<u8>(), any::<u8>());
    prop_oneof![
        Just(Snapshot),
        Just(Roll),
        (0u32..1 << 24).prop_map(|day| DayRoll { day }),
        va.prop_map(|(x, y)| DeadMan {
            venue: v(x),
            account: a(y)
        }),
        va.prop_map(|(x, y)| ConnRotate {
            venue: v(x),
            account: a(y)
        }),
        Just(AdvisorTick),
        Just(SnapshotNow),
        Just(ClockOk),
        Just(ClockBad),
        Just(ConflateReport),
        va.prop_map(|(x, y)| SessionKeepalive {
            venue: v(x),
            account: a(y)
        }),
        (1u8..=2).prop_map(|level| PersistLag { level }),
        (1u8..=2).prop_map(|level| DiskLow { level }),
        Just(StorageOk),
        any::<u8>().prop_map(|x| FundingSettle { venue: v(x) }),
        any::<u8>().prop_map(|x| KillSelfTestOk { account: a(x) }),
        Just(NotifyContinue),
        (0u8..=5).prop_map(|arg| RefdataRecheck { arg }),
    ]
}

fn arb_code() -> impl Strategy<Value = TimerCode> {
    prop_oneof![
        arb_sys().prop_map(TimerCode::Sys),
        any::<u32>().prop_map(|i| TimerCode::Keyframe { inst: InstId(i) }),
        prop_oneof![
            Just(OmsTimer::Continue),
            Just(OmsTimer::Sweep),
            (0u16..256, 0u32..65_536).prop_map(|(table_ix, slot)| OmsTimer::UnknownQuery { table_ix, slot }),
            (0u32..1 << 24).prop_map(|link_id| OmsTimer::HedgeRetry { link_id }),
            (any::<u8>(), any::<u8>()).prop_map(|(x, y)| OmsTimer::SmokeTtl {
                account: a(x),
                venue: v(y)
            }),
        ]
        .prop_map(TimerCode::Oms),
        (1u8..=3, any::<u8>(), any::<u8>()).prop_map(|(kind, x, y)| TimerCode::Risk {
            kind,
            account: a(x),
            venue: v(y)
        }),
        (1u8..=6, any::<u8>(), arb_group()).prop_map(|(kind, x, group)| TimerCode::Exec {
            kind,
            account: a(x),
            group
        }),
        (1u8..=254, any::<u32>(), arb_group()).prop_map(|(s, tag, group)| TimerCode::Strategy {
            strategy: StrategyId(s),
            tag,
            group
        }),
    ]
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(5_000))]
    #[test]
    fn evt_111_timer_roundtrip(c in arb_code()) {
        let (o, t, g) = c.encode().unwrap();
        if !matches!(c, TimerCode::Exec { .. } | TimerCode::Strategy { .. }) {
            prop_assert_eq!(g, GROUP_NONE);
        }
        prop_assert_eq!(TimerCode::decode(o, t, g), Ok(c));
    }
}

#[test]
fn evt_112_timer_encode_errors() {
    use SysTimer::*;
    use TimerErr::*;
    let st = |s: u8, group| TimerCode::Strategy {
        strategy: StrategyId(s),
        tag: 1,
        group,
    };
    let rows: Vec<(&str, TimerCode, Result<(u16, u32, GroupId), TimerErr>)> = vec![
        ("EVT-112.1", st(0, GROUP_NONE), Err(Owner)),
        ("EVT-112.2", st(255, GROUP_NONE), Err(Owner)),
        (
            "EVT-112.3",
            TimerCode::Risk {
                kind: 0,
                account: a(1),
                venue: v(1),
            },
            Err(Id),
        ),
        (
            "EVT-112.3",
            TimerCode::Risk {
                kind: 4,
                account: a(1),
                venue: v(1),
            },
            Err(Id),
        ),
        (
            "EVT-112.4",
            TimerCode::Exec {
                kind: 0,
                account: a(1),
                group: G,
            },
            Err(Id),
        ),
        (
            "EVT-112.4",
            TimerCode::Exec {
                kind: 7,
                account: a(1),
                group: G,
            },
            Err(Id),
        ),
        (
            "EVT-112.5",
            TimerCode::Sys(DayRoll { day: 0xFF_FFFF }),
            Ok((1, 0xFFFF_FF03, GROUP_NONE)),
        ),
        (
            "EVT-112.6",
            TimerCode::Sys(DayRoll { day: 0x100_0000 }),
            Err(Range),
        ),
        (
            "EVT-112.7",
            TimerCode::Oms(OmsTimer::UnknownQuery {
                table_ix: 255,
                slot: 65_535,
            }),
            Ok((3, 0xFFFF_FF03, GROUP_NONE)),
        ),
        (
            "EVT-112.8",
            TimerCode::Oms(OmsTimer::UnknownQuery {
                table_ix: 256,
                slot: 0,
            }),
            Err(Range),
        ),
        (
            "EVT-112.9",
            TimerCode::Oms(OmsTimer::UnknownQuery {
                table_ix: 0,
                slot: 65_536,
            }),
            Err(Range),
        ),
        (
            "EVT-112.10",
            TimerCode::Oms(OmsTimer::HedgeRetry { link_id: 0xFF_FFFF }),
            Ok((3, 0xFFFF_FF04, GROUP_NONE)),
        ),
        (
            "EVT-112.11",
            TimerCode::Oms(OmsTimer::HedgeRetry { link_id: 0x100_0000 }),
            Err(Range),
        ),
        ("EVT-112.12", TimerCode::Sys(PersistLag { level: 0 }), Err(Range)),
        ("EVT-112.12", TimerCode::Sys(PersistLag { level: 3 }), Err(Range)),
        ("EVT-112.13", TimerCode::Sys(DiskLow { level: 0 }), Err(Range)),
        ("EVT-112.13", TimerCode::Sys(DiskLow { level: 3 }), Err(Range)),
        (
            "EVT-112.14",
            TimerCode::Sys(RefdataRecheck { arg: 6 }),
            Err(Range),
        ),
        (
            "EVT-112.15",
            TimerCode::Exec {
                kind: 1,
                account: a(2),
                group: GROUP_ALL,
            },
            Err(Group),
        ),
        ("EVT-112.16", st(3, GROUP_ALL), Err(Group)),
    ];
    for (row, c, want) in rows {
        assert_eq!(c.encode(), want, "{row}");
    }
}

#[test]
fn evt_113_timer_decode_errors() {
    use TimerErr::*;
    let d = |o, t, g: u32| TimerCode::decode(o, t, GroupId(g));
    let rows: Vec<(&str, Result<TimerCode, TimerErr>, Result<TimerCode, TimerErr>)> = vec![
        ("EVT-113.1", d(0, 1, 0), Err(Owner)),
        ("EVT-113.2", d(6, 1, 0), Err(Owner)),
        ("EVT-113.3", d(16, 1, 0), Err(Owner)),
        ("EVT-113.4", d(271, 1, 0), Err(Owner)),
        ("EVT-113.5", d(0xFFFF, 1, 0), Err(Owner)),
        (
            "EVT-113.6",
            d(17, 0, 0),
            Ok(TimerCode::Strategy {
                strategy: StrategyId(1),
                tag: 0,
                group: GROUP_NONE,
            }),
        ),
        (
            "EVT-113.7",
            d(270, 9, 0x0300_0001),
            Ok(TimerCode::Strategy {
                strategy: StrategyId(254),
                tag: 9,
                group: GroupId(0x0300_0001),
            }),
        ),
        ("EVT-113.8", d(1, 0, 0), Err(Id)),
        ("EVT-113.8", d(1, 19, 0), Err(Id)),
        ("EVT-113.9", d(1, 0x0101, 0), Err(Id)),
        ("EVT-113.10", d(1, 0x0007_0110, 0), Err(Id)),
        ("EVT-113.11", d(1, 0x0100_0505, 0), Err(Id)),
        ("EVT-113.12", d(1, 0x030C, 0), Err(Range)),
        ("EVT-113.12", d(1, 0x000C, 0), Err(Range)),
        ("EVT-113.13", d(1, 0x0612, 0), Err(Range)),
        ("EVT-113.14", d(3, 0, 0), Err(Id)),
        ("EVT-113.14", d(3, 6, 0), Err(Id)),
        ("EVT-113.15", d(3, 0x0100_0205, 0), Err(Id)),
        ("EVT-113.16", d(4, 4, 0), Err(Id)),
        ("EVT-113.16", d(4, 0x0100_0301, 0), Err(Id)),
        ("EVT-113.17", d(5, 7, 0), Err(Id)),
        ("EVT-113.17", d(5, 0x0001_0201, 0), Err(Id)),
        ("EVT-113.18", d(1, 1, 5), Err(Group)),
        ("EVT-113.19", d(5, 0x0201, u32::MAX), Err(Group)),
        ("EVT-113.19", d(19, 1, u32::MAX), Err(Group)),
        ("EVT-113.20", d(0, 19, 5), Err(Owner)),
        ("EVT-113.21", d(1, 19, 5), Err(Id)),
        ("EVT-113.22", d(1, 0x030C, 5), Err(Range)),
        ("EVT-113.23", d(2, 0, 7), Err(Group)),
    ];
    for (row, got, want) in rows {
        assert_eq!(got, want, "{row}");
    }
}
