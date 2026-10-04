//! Load step 2: string references and symbol syntax (REF-012).

mod common;

use common::*;
use hft_refdata::LoadErrCode::Strings;
use hft_refdata::blob::{CapsRow, InstRow, sec};
use hft_types::{InstId, VenueId};

/// F0 with inst 1's symbol re-pointed at `bytes` appended to the end of the Strings section.
fn symbol_bytes(bytes: &[u8], extra_len: u32) -> Vec<u8> {
    let mut r = raw(&f0());
    let at = push_string(&mut r, bytes);
    edit_row::<InstRow, _>(&mut r, sec::INSTS, 0, |x| {
        x.symbol = hft_refdata::blob::StrRef {
            off: at.off,
            len: at.len + extra_len,
        }
    });
    r.encode()
}

fn symbol(s: &str) -> Vec<u8> {
    let mut spec = f0_spec();
    spec.inst_mut(1).symbol = s.into();
    spec.build()
}

#[test]
fn ref_012_strings() {
    let b = symbol_bytes(b"BTCUSDT", 0);
    let r = raw(&b);
    let row: InstRow = get_row(&r, sec::INSTS, 0);
    assert_eq!(
        row.symbol.off + row.symbol.len,
        r.section(sec::STRINGS).map_or(0, |s| s.data.len() as u32),
        "REF-012.1"
    );
    ok(&b);
    assert_eq!(
        errs(&symbol_bytes(b"BTCUSDT", 1)),
        vec![e(Strings, 3, 0)],
        "REF-012.2"
    );
    assert_eq!(
        errs(&symbol_bytes(&[0xFF, 0xFE], 0)),
        vec![e(Strings, 3, 0)],
        "REF-012.3"
    );
    assert_eq!(errs(&symbol("")), vec![e(Strings, 3, 0)], "REF-012.4");
    let rd = ok(&symbol("A"));
    assert_eq!(rd.symbols(VenueId(1)).get(b"A"), Some(InstId(1)), "REF-012.5");
    ok(&symbol(&"A".repeat(27))); // REF-012.6
    assert_eq!(
        errs(&symbol(&"A".repeat(28))),
        vec![e(Strings, 3, 0)],
        "REF-012.7"
    );
    assert_eq!(errs(&symbol("BTC USDT")), vec![e(Strings, 3, 0)], "REF-012.8");
    assert_eq!(errs(&symbol("\tBTCUSDT")), vec![e(Strings, 3, 0)], "REF-012.9");
    assert_eq!(
        errs(&symbol("BTC\u{7f}USDT")),
        vec![e(Strings, 3, 0)],
        "REF-012.10"
    );

    let mut spec = f0_spec();
    spec.aliases[0].alias = "B".repeat(28);
    assert_eq!(errs(&spec.build()), vec![e(Strings, 11, 0)], "REF-012.11");

    let mut r = raw(&f0());
    let n_refs = r.section(sec::STR_REFS).map_or(0, |s| s.count);
    edit_row::<CapsRow, _>(&mut r, sec::VENUE_CAPS, 0, |x| {
        x.events_off = n_refs - x.events_count + 1
    });
    assert_eq!(errs(&r.encode()), vec![e(Strings, 13, 0)], "REF-012.12");

    let mut spec = f0_spec();
    spec.instances[1].name = "pc-main".into();
    assert_eq!(errs(&spec.build()), vec![e(Strings, 15, 1)], "REF-012.13");

    let mut spec = f0_spec();
    spec.account_mut(2).label = "acct-a".into();
    assert_eq!(errs(&spec.build()), vec![e(Strings, 4, 1)], "REF-012.14");
}
