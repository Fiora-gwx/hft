//! SPB1 container parse (EVT-131, EVT-132).

#![allow(
    clippy::disallowed_types,
    clippy::disallowed_macros,
    clippy::disallowed_methods
)] // allow-list: test code, not a hot path
#![allow(clippy::type_complexity)]

#[path = "common/spb.rs"]
mod spb;

use hft_event::*;
use hft_types::GroupId;
use spb::*;
use zerocopy::IntoBytes;

fn parse(b: &[u8]) -> Result<(), ParamsErr> {
    let (v, n) = aligned(b);
    parse_spb1(&v.as_bytes()[..n]).map(|_| ())
}

#[test]
fn evt_131_parse_ok() {
    let b = blob_b();
    assert_eq!(b.len(), 224);
    let (store, n) = aligned(&b);
    let bytes = &store.as_bytes()[..n];
    let v = parse_spb1(bytes).unwrap();
    assert_eq!(v.hdr.strategy.0, 3);
    assert_eq!(v.groups.len(), 2);
    assert_eq!(v.groups[1].id, GroupId(0x0300_0002));
    assert_eq!(v.body.as_ptr(), bytes[208..].as_ptr());
    assert_eq!(v.body, &(0..16).collect::<Vec<u8>>()[..]);
    // The refdata check_group view in caller storage.
    let mut legs = [hft_refdata::LegView {
        inst: hft_types::InstId(0),
        venue: hft_types::VenueId(0),
    }; 4];
    let gv = v.groups[0].view(&mut legs);
    assert_eq!(gv.legs.len(), 2);
    assert_eq!(gv.legs[1].inst.0, 20);
}

fn put(b: &mut [u8], o: usize, v: &[u8]) {
    b[o..o + v.len()].copy_from_slice(v);
}

const G0: usize = 48;
const G1: usize = 128;

fn leg_at(g: usize, i: usize) -> usize {
    g + 8 + 16 * i
}

#[test]
fn evt_132_parse_errors() {
    use ParamsErr::*;
    let b = blob_b();
    let m = |f: &dyn Fn(&mut Vec<u8>)| {
        let mut x = b.clone();
        f(&mut x);
        parse(&x)
    };
    let nmax = GroupId::N_MAX;
    let rows: Vec<(&str, Result<(), ParamsErr>, Result<(), ParamsErr>)> = vec![
        ("EVT-132.1", parse(&b[..47]), Err(Truncated)),
        (
            "EVT-132.2",
            m(&|x| put(x, 12, &3u16.to_le_bytes())),
            Err(Truncated),
        ),
        (
            "EVT-132.3",
            m(&|x| {
                put(x, 32, &12u32.to_le_bytes());
                x.truncate(220);
            }),
            Err(BadLen),
        ),
        ("EVT-132.4", m(&|x| x.extend([0u8; 8])), Err(BadLen)),
        ("EVT-132.5", m(&|x| x[10] = 0), Err(BadHdr)),
        ("EVT-132.5", m(&|x| x[10] = 3), Err(BadHdr)),
        ("EVT-132.6", m(&|x| x[11] = 1), Err(Padding)),
        ("EVT-132.6", m(&|x| x[14] = 1), Err(Padding)),
        (
            "EVT-132.7",
            m(&|x| put(x, leg_at(G1, 1), &5u32.to_le_bytes())),
            Err(Padding),
        ),
        ("EVT-132.8", m(&|x| x[G0 + 72] = 1), Err(Padding)),
        ("EVT-132.9", m(&|x| x[leg_at(G0, 0) + 12] = 1), Err(Padding)),
        (
            "EVT-132.10",
            m(&|x| put(x, G0, &0x0400_0001u32.to_le_bytes())),
            Err(BadGroup),
        ),
        (
            "EVT-132.11",
            m(&|x| put(x, G0, &0u32.to_le_bytes())),
            Err(BadGroup),
        ),
        (
            "EVT-132.11",
            m(&|x| put(x, G0, &u32::MAX.to_le_bytes())),
            Err(BadGroup),
        ),
        (
            "EVT-132.12",
            m(&|x| put(x, G0, &0x0300_0000u32.to_le_bytes())),
            Err(BadGroup),
        ),
        (
            "EVT-132.13",
            m(&|x| put(x, G0, &0x03FF_FFFFu32.to_le_bytes())),
            Err(BadGroup),
        ),
        (
            "EVT-132.14",
            m(&|x| {
                put(x, G0, &0x03FF_FFFEu32.to_le_bytes());
                put(x, 36, &nmax.to_le_bytes());
            }),
            Ok(()),
        ),
        ("EVT-132.15", m(&|x| x[G0 + 6] = 0), Err(BadGroup)),
        ("EVT-132.15", m(&|x| x[G0 + 6] = 5), Err(BadGroup)),
        (
            "EVT-132.16",
            m(&|x| {
                x[G0 + 6] = 4;
                for i in 2..4 {
                    put(x, leg_at(G0, i), leg(40 + i as u32, LegDef::CEX, 0).as_bytes());
                }
            }),
            Ok(()),
        ),
        ("EVT-132.17", m(&|x| x[G0 + 7] = 2), Err(BadGroup)),
        (
            "EVT-132.18",
            m(&|x| put(x, G1, &0x0300_0001u32.to_le_bytes())),
            Err(DupGroup),
        ),
        (
            "EVT-132.19",
            m(&|x| put(x, leg_at(G0, 0), &0u32.to_le_bytes())),
            Err(BadLeg),
        ),
        ("EVT-132.20", m(&|x| x[leg_at(G0, 0) + 5] = 0), Err(BadLeg)),
        ("EVT-132.20", m(&|x| x[leg_at(G0, 0) + 5] = 3), Err(BadLeg)),
        (
            "EVT-132.21",
            m(&|x| put(x, leg_at(G0, 0) + 8, &1u32.to_le_bytes())),
            Err(BadLeg),
        ),
        (
            "EVT-132.22",
            m(&|x| {
                x[leg_at(G0, 0) + 5] = 2;
                put(x, leg_at(G0, 0) + 8, &7u32.to_le_bytes());
            }),
            Ok(()),
        ),
        ("EVT-132.23", parse(&blob(&hdr(3, 0, 0, 2), &[], &[])), Ok(())),
        (
            "EVT-132.24",
            m(&|x| put(x, 36, &1u32.to_le_bytes())),
            Err(BadGroup),
        ),
        ("EVT-132.25", m(&|x| put(x, 36, &3u32.to_le_bytes())), Ok(())),
        ("EVT-132.26", m(&|x| put(x, 36, &nmax.to_le_bytes())), Ok(())),
        (
            "EVT-132.27",
            m(&|x| put(x, 36, &(nmax + 1).to_le_bytes())),
            Err(BadHdr),
        ),
        ("EVT-132.28", parse(&blob(&hdr(3, 0, 0, 7), &[], &[])), Ok(())),
    ];
    for (row, got, want) in rows {
        assert_eq!(got, want, "{row}");
    }
    let (store, _) = aligned(&blob(&hdr(3, 0, 0, 7), &[], &[]));
    let v = parse_spb1(&store.as_bytes()[..48]).unwrap();
    assert!(v.groups.is_empty() && v.body.is_empty(), "EVT-132.23");
}
