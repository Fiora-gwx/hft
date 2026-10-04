//! ECF1 `EngineConfig`: container, section views, validators, versions (REF-066 – REF-069).

#![allow(
    clippy::disallowed_types,
    clippy::disallowed_macros,
    clippy::disallowed_methods
)] // allow-list: test code, not a hot path

mod common;

use std::cell::RefCell;

use common::*;
use hft_refdata::LoadErrCode::{
    AlreadyActive, NoValidator, RefInstNotSubscribed, Structure, VersionNotIncreasing,
};
use hft_refdata::blob::{RawSection, SECTION_OPTIONAL};
use hft_refdata::{EcfBuilder, EngineConfig, EngineSection, EngineValidator, LoadErr, RefData};
use hft_types::HostClass;

fn ecf_errs(b: &[u8], cur: Option<&EngineConfig>) -> Vec<LoadErr> {
    EngineConfig::load(b, cur)
        .err()
        .map(|r| r.errors)
        .unwrap_or_default()
}

#[test]
fn ref_066_ecf1_container() {
    let base = e0();
    let host = |code: u8| {
        let mut b = base.clone();
        b[48] = code;
        b
    };
    let want = [
        HostClass::Dev2,
        HostClass::C4,
        HostClass::C8,
        HostClass::Metal,
        HostClass::C16,
        HostClass::C32,
    ];
    for (code, h) in (1u8..=6).zip(want) {
        let ec = EngineConfig::load(&host(code), None).expect("REF-066.1");
        assert_eq!(ec.host_class(), h, "REF-066.1 code {code}");
    }
    assert_eq!(ecf_errs(&host(0), None), vec![e(Structure, 0, 0)], "REF-066.2");
    assert_eq!(ecf_errs(&host(7), None), vec![e(Structure, 0, 0)], "REF-066.3");
    assert_eq!(ecf_errs(&f0(), None), vec![e(Structure, 0, 0)], "REF-066.4");
    let mut b = base.clone();
    b[8..10].copy_from_slice(&2u16.to_le_bytes());
    assert_eq!(ecf_errs(&b, None), vec![e(Structure, 0, 0)], "REF-066.5");
    let with8 = |flags: u32| {
        let mut r = raw(&base);
        r.sections.push(RawSection {
            id: 8,
            row_ver: 1,
            row_size: 8,
            count: 1,
            flags,
            data: vec![8; 8],
        });
        r.encode()
    };
    assert_eq!(ecf_errs(&with8(0), None), vec![e(Structure, 8, 0)], "REF-066.6");
    let ec = EngineConfig::load(&with8(SECTION_OPTIONAL), None).expect("REF-066.7");
    let e0c = EngineConfig::load(&base, None).expect("E0");
    assert!(EngineSection::try_from(8u16).is_err(), "REF-066.7");
    for s in EngineSection::ALL {
        assert_eq!(ec.section(s), e0c.section(s), "REF-066.7 section {s:?}");
    }
}

/// E0 with section 1 = 3 rows (row_ver 2, row_size 40, bytes 0..120) and without section 6.
fn e0_variant() -> Vec<u8> {
    let mut b = EcfBuilder::new(3, HostClass::C8);
    b.created_utc_ns(CREATED);
    for s in EngineSection::ALL {
        match s {
            EngineSection::Oms => {
                b.section(s, 2, 40, 3, (0u8..120).collect());
            }
            EngineSection::Conn => {}
            _ => {
                b.section(s, 1, 8, 1, vec![s as u8; 8]);
            }
        }
    }
    b.finish()
}

#[test]
fn ref_067_section_view() {
    let ec = EngineConfig::load(&e0_variant(), None).expect("E0 variant");
    let v = ec.section(EngineSection::Oms).expect("REF-067.1");
    assert_eq!((v.row_ver, v.row_size, v.count), (2, 40, 3), "REF-067.1");
    assert_eq!(v.bytes, &(0u8..120).collect::<Vec<_>>()[..], "REF-067.1");
    assert!(ec.section(EngineSection::Conn).is_none(), "REF-067.2");
    let codes: Vec<u16> = EngineSection::ALL.iter().map(|s| *s as u16).collect();
    assert_eq!(codes, vec![1, 2, 3, 4, 5, 6, 7], "REF-067.3");
}

thread_local! {
    static LOG: RefCell<Vec<u16>> = const { RefCell::new(Vec::new()) };
}

fn log(s: u16) {
    LOG.with(|l| l.borrow_mut().push(s));
}

fn taken() -> Vec<u16> {
    LOG.with(|l| std::mem::take(&mut *l.borrow_mut()))
}

macro_rules! ok_validator {
    ($name:ident, $sec:expr) => {
        fn $name(_: &EngineConfig, _: &RefData) -> Result<(), LoadErr> {
            log($sec);
            Ok(())
        }
    };
}
ok_validator!(v1, 1);
ok_validator!(v2, 2);
ok_validator!(v3, 3);
ok_validator!(v4, 4);
ok_validator!(v5, 5);
ok_validator!(v6, 6);
ok_validator!(v7, 7);

fn v2_err(_: &EngineConfig, _: &RefData) -> Result<(), LoadErr> {
    log(2);
    Err(LoadErr {
        code: RefInstNotSubscribed,
        section: 2,
        row: 3,
    })
}

type Check = fn(&EngineConfig, &RefData) -> Result<(), LoadErr>;

fn validators(skip: &[u16], replace2: Option<Check>) -> Vec<EngineValidator> {
    let fns: [Check; 7] = [v1, v2, v3, v4, v5, v6, v7];
    EngineSection::ALL
        .iter()
        .zip(fns)
        .filter(|(s, _)| !skip.contains(&(**s as u16)))
        .map(|(s, f)| EngineValidator {
            section: *s,
            check: if *s == EngineSection::Ledger {
                replace2.unwrap_or(f)
            } else {
                f
            },
        })
        .collect()
}

#[test]
fn ref_068_validate() {
    let rd = ok(&f0());
    let ec = EngineConfig::load(&e0(), None).expect("E0");
    let run = |ec: &EngineConfig, vs: &[EngineValidator]| {
        ec.validate(&rd, vs).err().map(|r| r.errors).unwrap_or_default()
    };
    taken();
    assert_eq!(run(&ec, &validators(&[], None)), vec![], "REF-068.1");
    assert_eq!(taken(), vec![1, 2, 3, 4, 5, 6, 7], "REF-068.1");
    assert_eq!(
        run(&ec, &validators(&[5], None)),
        vec![e(NoValidator, 5, 0)],
        "REF-068.2"
    );
    assert_eq!(taken(), vec![1, 2, 3, 4, 6, 7], "REF-068.2");
    let mut b = e0_builder().finish_raw();
    b.sections.retain(|s| s.id != 6);
    let ec6 = EngineConfig::load(&b.encode(), None).expect("E0 without 6");
    assert_eq!(run(&ec6, &validators(&[6], None)), vec![], "REF-068.3");
    taken();
    let err2 = e(RefInstNotSubscribed, 2, 3);
    assert_eq!(run(&ec, &validators(&[], Some(v2_err))), vec![err2], "REF-068.4");
    assert_eq!(
        run(&ec, &validators(&[5], Some(v2_err))),
        vec![err2, e(NoValidator, 5, 0)],
        "REF-068.5"
    );
}

/// E0 with section 3 rewritten (smaller capacity) and the given version.
fn shrunk(version: u64) -> Vec<u8> {
    let mut r = e0_builder().finish_raw();
    r.version = version;
    if let Some(s) = r.section_mut(3) {
        s.data = vec![1; 8];
    }
    r.encode()
}

#[test]
fn ref_069_ecf1_versions() {
    let cur = EngineConfig::load(&e0(), None).expect("E0");
    assert_eq!(
        ecf_errs(&e0(), Some(&cur)),
        vec![e(AlreadyActive, 0, 0)],
        "REF-069.1"
    );
    assert_eq!(
        ecf_errs(&shrunk(3), Some(&cur)),
        vec![e(VersionNotIncreasing, 0, 0)],
        "REF-069.2"
    );
    assert_eq!(
        ecf_errs(&shrunk(2), Some(&cur)),
        vec![e(VersionNotIncreasing, 0, 0)],
        "REF-069.3"
    );
    assert_eq!(ecf_errs(&shrunk(4), Some(&cur)), vec![], "REF-069.4");
    assert_eq!(ecf_errs(&shrunk(1), None), vec![], "REF-069.5");
}
