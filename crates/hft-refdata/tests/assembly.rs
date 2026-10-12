//! `check_assembly` against the compiled-in protocol and strategy facts
//! (REF-061 – REF-063, REF-093).

#![allow(
    clippy::disallowed_types,
    clippy::disallowed_macros,
    clippy::disallowed_methods
)] // allow-list: test code, not a hot path

use crate::common;

use common::*;
use hft_refdata::LoadErrCode::{
    CapsExceedProtocol, CapsMarketMismatch, HostClassMismatch, StaleGuardMismatch, UnknownStrategyKind,
    UnknownVenue, ValueRange,
};
use hft_refdata::{
    AssemblyFacts, CapFlags, DEFAULT_MAX_UNIVERSE_INSTS, DeadManCap, EngineConfig, Exchange, FeeSpec,
    InstKind, LoadErr, ProtocolFacts, RefData, StaleGuardKind, check_assembly,
};
use hft_types::{HostClass, MarketKind, VenueId};

/// P0: protocol facts equal to the snapshot's own caps rows and stale guards.
fn p0(rd: &RefData) -> Vec<ProtocolFacts> {
    rd.venues_in_order()
        .iter()
        .filter(|v| rd.caps_row(**v).is_some())
        .map(|&v| ProtocolFacts {
            venue: v,
            market: rd.caps(v).market,
            caps: rd.caps(v).clone(),
            stale_guard: rd.venue(v).stale_guard,
        })
        .collect()
}

fn ec() -> EngineConfig {
    EngineConfig::load(&e0(), None).expect("E0")
}

fn run(rd: &RefData, host: HostClass, max: u32, protocols: &[ProtocolFacts], kinds: &[u16]) -> Vec<LoadErr> {
    let f = AssemblyFacts {
        host,
        max_universe_insts: max,
        protocols,
        kinds,
    };
    check_assembly(rd, &ec(), &f)
        .err()
        .map(|r| r.errors)
        .unwrap_or_default()
}

const KINDS: &[u16] = &[0xFF00, 0xFF01];

#[test]
fn ref_061_assembly_ok() {
    let rd = ok(&f0());
    let p = p0(&rd);
    let f = AssemblyFacts {
        host: HostClass::C8,
        max_universe_insts: 8192,
        protocols: &p,
        kinds: KINDS,
    };
    assert_eq!(check_assembly(&rd, &ec(), &f), Ok(()));
}

#[test]
fn ref_062_assembly_rejects() {
    let rd = ok(&f0());
    let p = p0(&rd);
    let c8 = HostClass::C8;
    assert_eq!(
        run(&rd, HostClass::C16, 8192, &p, KINDS),
        vec![e(HostClassMismatch, 0, 0)],
        "REF-062.1"
    );
    let es = run(&rd, c8, 4, &p, KINDS);
    assert!(es.len() == 1 && es[0].code == ValueRange, "REF-062.2: {es:?}");
    assert_eq!(run(&rd, c8, 5, &p, KINDS), vec![], "REF-062.3");
    let no6: Vec<_> = p.iter().filter(|x| x.venue != V6).cloned().collect();
    assert_eq!(
        run(&rd, c8, 8192, &no6, KINDS),
        vec![e(UnknownVenue, 2, 3)],
        "REF-062.4"
    );

    let mut s = f0_spec();
    s.add_venue(venue(
        VenueId::GATE_USDT_FUT,
        Exchange::Gate,
        InstKind::Perp,
        StaleGuardKind::None,
        28,
    ));
    assert_eq!(run(&ok(&s.build()), c8, 8192, &p, KINDS), vec![], "REF-062.5");

    let mut s = f0_spec();
    let gs = VenueId::GATE_SPOT;
    s.add_venue(venue(
        gs,
        Exchange::Gate,
        InstKind::Spot,
        StaleGuardKind::None,
        28,
    ));
    s.add_caps(spot_caps(gs, None));
    s.fees.push(FeeSpec {
        venue: gs,
        tier: 0,
        fee: fee(200_000, 500_000),
    });
    s.insts.push(inst(
        6,
        "BTC_USDT",
        gs,
        InstKind::Spot,
        BTC,
        ("0.1", 10_000_000),
        ("0.00001", 1_000),
        ("0.00001", 1_000),
    ));
    assert_eq!(
        run(&ok(&s.build()), c8, 8192, &p, KINDS),
        vec![e(UnknownVenue, 2, 2)],
        "REF-062.6"
    );

    let edit = |v: VenueId, f: &dyn Fn(&mut ProtocolFacts)| {
        let mut q = p.clone();
        for x in q.iter_mut().filter(|x| x.venue == v) {
            f(x);
        }
        q
    };
    let q = edit(V1, &|x| x.market = MarketKind::Spot);
    assert_eq!(
        run(&rd, c8, 8192, &q, KINDS),
        vec![e(CapsMarketMismatch, 13, 0)],
        "REF-062.7"
    );
    let q = edit(V1, &|x| x.caps.flags = CapFlags::POST_ONLY | CapFlags::IOC);
    assert_eq!(
        run(&rd, c8, 8192, &q, KINDS),
        vec![e(CapsExceedProtocol, 13, 0)],
        "REF-062.8"
    );
    let q = edit(V2, &|x| {
        x.caps.flags = CapFlags::AMEND | CapFlags::POST_ONLY | CapFlags::IOC | CapFlags::FOK
    });
    assert_eq!(run(&rd, c8, 8192, &q, KINDS), vec![], "REF-062.9");
    let q = edit(V5, &|x| x.caps.dms = DeadManCap::CountdownAccount);
    assert_eq!(
        run(&rd, c8, 8192, &q, KINDS),
        vec![e(CapsExceedProtocol, 13, 2)],
        "REF-062.10"
    );
    let q = edit(V1, &|x| x.stale_guard = StaleGuardKind::ExpireAt);
    assert_eq!(
        run(&rd, c8, 8192, &q, KINDS),
        vec![e(StaleGuardMismatch, 2, 0)],
        "REF-062.11"
    );
    assert_eq!(
        run(&rd, c8, 8192, &p, &[0xFF00]),
        vec![e(UnknownStrategyKind, 15, 2)],
        "REF-062.12"
    );
    assert_eq!(
        run(&rd, HostClass::C16, 8192, &p, &[0xFF00]),
        vec![e(HostClassMismatch, 0, 0), e(UnknownStrategyKind, 15, 2)],
        "REF-062.13"
    );
}

#[test]
fn ref_063_max_universe_default() {
    assert_eq!(DEFAULT_MAX_UNIVERSE_INSTS, 8192, "REF-063.1");
    let check = |n: usize| {
        let rd = ok(&universe(n).build());
        let p = p0(&rd);
        run(&rd, HostClass::C8, DEFAULT_MAX_UNIVERSE_INSTS, &p, KINDS)
    };
    assert_eq!(check(8192), vec![], "REF-063.2");
    let es = check(8193);
    assert!(es.len() == 1 && es[0].code == ValueRange, "REF-063.3: {es:?}");
    assert_eq!(check(8191), vec![], "REF-063.4");
}

#[test]
fn ref_093_amend_qty_down_only() {
    let cases: [(&str, bool, bool, bool); 4] = [
        ("REF-093.1", true, true, true),
        ("REF-093.2", true, false, true),
        ("REF-093.3", false, false, true),
        ("REF-093.4", false, true, false),
    ];
    for (case, row, proto, pass) in cases {
        let mut s = f0_spec();
        s.caps_mut(2).amend_qty_down_only = row;
        let rd = ok(&s.build());
        let mut p = p0(&rd);
        for x in p.iter_mut().filter(|x| x.venue == V2) {
            x.caps.amend_qty_down_only = proto;
        }
        let want = if pass {
            vec![]
        } else {
            vec![e(CapsExceedProtocol, 13, 1)]
        };
        assert_eq!(run(&rd, HostClass::C8, 8192, &p, KINDS), want, "{case}");
        assert_eq!(rd.caps(V2).amend_qty_down_only, row, "{case}");
    }
}

#[test]
fn ref_094_perp_cash_capability_is_explicit() {
    let mut s = f0_spec();
    let c = s.caps_mut(1);
    c.flags = c.flags | CapFlags::PERP_CASH;
    let rd = ok(&s.build());
    assert!(rd.caps(V1).flags.contains(CapFlags::PERP_CASH), "REF-094.1");
    let mut protocols = p0(&rd);
    assert!(
        run(&rd, HostClass::C8, 8192, &protocols, KINDS).is_empty(),
        "REF-094.1"
    );
    for p in protocols.iter_mut().filter(|p| p.venue == V1) {
        p.caps.flags = CapFlags(p.caps.flags.0 & !CapFlags::PERP_CASH.0);
    }
    assert_eq!(
        run(&rd, HostClass::C8, 8192, &protocols, KINDS),
        vec![e(CapsExceedProtocol, 13, 0)],
        "REF-094.2"
    );
    let mut s = f0_spec();
    let c = s.caps_mut(2);
    c.flags = c.flags | CapFlags::PERP_CASH;
    assert_single(&s.build(), CapsMarketMismatch, "REF-094.3");
}
