//! Layout tests (TYP-001 – TYP-003, TYP-150).

use std::mem::{align_of, offset_of, size_of};

use hft_types::*;
use zerocopy::FromBytes;

const _: () = assert!(size_of::<AssetExp>() == 2);
const _: () = assert!(offset_of!(AssetExp, px_exp) == 0);
const _: () = assert!(offset_of!(AssetExp, qty_exp) == 1);

#[test]
fn typ_001_asset_exp_layout() {
    assert_eq!(size_of::<AssetExp>(), 2);
    assert_eq!(align_of::<AssetExp>(), 1);
    assert_eq!(offset_of!(AssetExp, px_exp), 0);
    assert_eq!(offset_of!(AssetExp, qty_exp), 1);
    let e = AssetExp::read_from_bytes(&[0xFC, 0xF8]).unwrap();
    assert_eq!(
        e,
        AssetExp {
            px_exp: -4,
            qty_exp: -8
        }
    );
}

#[test]
fn typ_002_numeric_newtypes() {
    assert_eq!(
        (size_of::<Px>(), align_of::<Px>(), Px(-5).0),
        (8, 8, -5),
        "TYP-002.1"
    );
    assert_eq!(
        (size_of::<Qty>(), align_of::<Qty>(), Qty(-5).0),
        (8, 8, -5),
        "TYP-002.2"
    );
    assert_eq!(
        (size_of::<Notional>(), align_of::<Notional>()),
        (8, 8),
        "TYP-002.3"
    );
    assert_eq!((size_of::<Rate>(), align_of::<Rate>()), (8, 8), "TYP-002.4");
}

#[test]
fn typ_003_max_raw_constants() {
    assert_eq!(MAX_RAW, 4_611_686_018_427_387_903);
    assert_eq!(MAX_RAW, (1i64 << 62) - 1);
    assert_eq!(Rate::MAX, Rate(i64::MAX));
    assert_eq!(MAX_RAW.checked_add(MAX_RAW), Some(9_223_372_036_854_775_806));
    assert_eq!((-MAX_RAW).checked_sub(MAX_RAW), Some(-9_223_372_036_854_775_806));
}

/// One `#[repr(..)]` item found in the source: its name, the attribute lines above it and
/// the text that follows it.
struct Item {
    name: String,
    attrs: String,
    after: String,
}

fn items(src: &str, repr: &str, kw: &str) -> Vec<Item> {
    let lines: Vec<&str> = src.lines().collect();
    let mut out = Vec::new();
    for (i, l) in lines.iter().enumerate() {
        if l.trim() != repr {
            continue;
        }
        // Attributes (and doc comments) run until the item line.
        let mut j = i;
        let mut attrs = String::new();
        while j < lines.len() && !lines[j].trim_start().starts_with(&format!("pub {kw} ")) {
            attrs.push_str(lines[j]);
            attrs.push('\n');
            j += 1;
        }
        let Some(item) = lines.get(j) else { continue };
        let name: String = item
            .trim_start()
            .trim_start_matches(&format!("pub {kw} "))
            .chars()
            .take_while(|c| c.is_alphanumeric() || *c == '_')
            .collect();
        let after = lines[j..].iter().take(40).copied().collect::<Vec<_>>().join("\n");
        out.push(Item { name, attrs, after });
    }
    out
}

fn derives(attrs: &str) -> Vec<String> {
    attrs
        .split(|c: char| !(c.is_alphanumeric() || c == '_' || c == ':'))
        .map(|t| t.rsplit("::").next().unwrap_or(t).to_string())
        .collect()
}

#[test]
fn typ_150_layout_asserts_present() {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut structs = Vec::new();
    let mut enums = Vec::new();
    for entry in std::fs::read_dir(&dir).unwrap() {
        let src = std::fs::read_to_string(entry.unwrap().path()).unwrap();
        structs.extend(items(&src, "#[repr(C)]", "struct"));
        enums.extend(items(&src, "#[repr(u8)]", "enum"));
    }

    let names: Vec<&str> = structs.iter().map(|s| s.name.as_str()).collect();
    assert_eq!(names, ["AssetExp"], "repr(C) structs");
    for s in &structs {
        let n = &s.name;
        assert!(
            s.after.contains(&format!("assert!(size_of::<{n}>() == ")),
            "{n}: size assert"
        );
        for f in ["px_exp", "qty_exp"] {
            assert!(
                s.after.contains(&format!("offset_of!({n}, {f}) == ")),
                "{n}: offset assert {f}"
            );
        }
    }

    let mut names: Vec<&str> = enums.iter().map(|e| e.name.as_str()).collect();
    names.sort();
    let mut want = [
        "RoundMode",
        "Side",
        "Tif",
        "MarketKind",
        "HostClass",
        "RunMode",
        "KillLevel",
        "CidErr",
    ];
    want.sort();
    assert_eq!(names, want, "repr(u8) enums");
    for e in &enums {
        let d = derives(&e.attrs);
        let wire =
            ["Side", "Tif", "MarketKind", "HostClass", "RunMode", "KillLevel"].contains(&e.name.as_str());
        if wire {
            assert!(
                d.iter().any(|t| t == "TryFromBytes"),
                "{}: derives TryFromBytes",
                e.name
            );
            assert!(
                !d.iter().any(|t| t == "FromBytes"),
                "{}: must not derive FromBytes",
                e.name
            );
        }
        let n = &e.name;
        assert!(
            e.after.contains(&format!("assert!(size_of::<{n}>() == 1)")),
            "{n}: size assert"
        );
    }
}
