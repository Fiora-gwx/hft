//! Source scans: no second parameter encoder and no second booking implementation in this crate
//! (TESTS.md PY-113, PY-132).

use std::path::{Path, PathBuf};

fn crate_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

/// `(file name, content)` of every file with extension `ext` under `dir`.
fn sources(dir: &Path, ext: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    let Ok(rd) = std::fs::read_dir(dir) else {
        return out;
    };
    for e in rd.flatten() {
        let p = e.path();
        if p.is_dir() {
            out.extend(sources(&p, ext));
        } else if p.extension().is_some_and(|x| x == ext) {
            out.push((
                p.display().to_string(),
                std::fs::read_to_string(&p).unwrap_or_default(),
            ));
        }
    }
    out
}

/// Code lines (comments dropped).
fn code(text: &str) -> String {
    text.lines()
        .map(|l| l.split("//").next().unwrap_or(""))
        .collect::<Vec<_>>()
        .join("\n")
}

/// PY-132 step 1 on Rust sources: struct literals of the SDK body types; `to_le_bytes` in the
/// params code.
fn encoder_hits(files: &[(String, String)]) -> Vec<String> {
    let mut hits = Vec::new();
    for (name, text) in files {
        let c = code(text);
        for pat in ["ParamsBodyHdr {", "ParamOverride {"] {
            if c.contains(pat) {
                hits.push(format!("{name}: {pat}"));
            }
        }
        if name.ends_with("params.rs") && c.contains("to_le_bytes") {
            hits.push(format!("{name}: to_le_bytes"));
        }
    }
    hits
}

#[test]
fn py_132_no_second_encoder() {
    let rs = sources(&crate_dir().join("src"), "rs");
    assert!(!rs.is_empty());
    assert_eq!(encoder_hits(&rs), Vec::<String>::new());
    for (name, text) in sources(&crate_dir().join("python").join("hft_py"), "py") {
        let c: String = text
            .lines()
            .map(|l| l.split('#').next().unwrap_or(""))
            .collect::<Vec<_>>()
            .join("\n");
        assert!(!c.contains("struct.pack") && !c.contains("to_bytes("), "{name}");
    }
    let params = std::fs::read_to_string(crate_dir().join("src").join("params.rs")).unwrap_or_default();
    let p = code(&params);
    for call in ["encode_body(", "decode_body(", "sdk_encode_cmd("] {
        assert!(p.contains(call), "src/params.rs does not call {call}");
    }
    assert!(
        p.contains("encode_cmd as sdk_encode_cmd"),
        "encode_cmd is the SDK's"
    );
    // The scan catches a second encoder: a patched copy of params.rs fails it.
    let patched = format!(
        "{params}\nfn x() -> ParamOverride {{ ParamOverride {{ group_ix: 0, field_ix: 0, _pad: 0, value: 0 }} }}\n"
    );
    assert!(!encoder_hits(&[("src/params.rs".into(), patched)]).is_empty());
}

/// PY-113 step 2: arithmetic on position or inventory fields, or per-kind booking calls.
fn booking_hits(files: &[(String, String)]) -> Vec<String> {
    let mut hits = Vec::new();
    for (name, text) in files {
        let c = code(text);
        for pat in [
            ".qty +=",
            ".qty -=",
            ".qty.0 +=",
            ".qty.0 -=",
            ".cost +=",
            ".cost -=",
            ".realized_quote +=",
            ".fees_quote +=",
            "apply_fill(",
            "apply_fee(",
            "apply_external(",
            "apply_funding(",
            "apply_adjust(",
            "apply_transfer(",
        ] {
            if c.contains(pat) {
                hits.push(format!("{name}: {pat}"));
            }
        }
    }
    hits
}

#[test]
fn py_113_no_second_booking() {
    let rs = sources(&crate_dir().join("src"), "rs");
    let ledger = std::fs::read_to_string(crate_dir().join("src").join("ledger.rs")).unwrap_or_default();
    assert!(
        code(&ledger).contains("JournalLedger"),
        "src/ledger.rs must drive hft_ledger::JournalLedger"
    );
    assert_eq!(booking_hits(&rs), Vec::<String>::new());
    let py = sources(&crate_dir().join("python").join("hft_py"), "py");
    assert_eq!(booking_hits(&py), Vec::<String>::new());
    let patched =
        format!("{ledger}\nfn x(pos: &mut hft_ledger::InstPos) {{ pos.qty += hft_types::Qty(1); }}\n");
    assert!(!booking_hits(&[("src/ledger.rs".into(), patched)]).is_empty());
}
