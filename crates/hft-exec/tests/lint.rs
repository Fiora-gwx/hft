//! EXE-477: the hot rule set blocks f64, HashMap, std::time and dyn in this crate — clippy
//! (`clippy.toml` = the hot template; `float_arithmetic` denied in `lib.rs`) for HashMap /
//! `Instant::now` / float arithmetic, and the `cargo xtask hot-check` source scan (N3 / N6) for
//! float and trait-object types; this test checks the configuration and the source tree itself.
#![allow(
    clippy::disallowed_types,
    clippy::disallowed_macros,
    clippy::disallowed_methods
)] // allow-list: test code, not a hot path

use std::path::Path;

fn read(p: &Path) -> String {
    std::fs::read_to_string(p).unwrap().replace("\r\n", "\n")
}

/// Source text without `//` comments and string literals' contents.
fn code_of(s: &str) -> String {
    let mut out = String::new();
    for line in s.lines() {
        let mut in_str = false;
        let mut prev = ' ';
        let mut cut = line.len();
        for (i, ch) in line.char_indices() {
            if ch == '"' && prev != '\\' {
                in_str = !in_str;
            }
            if !in_str && ch == '/' && prev == '/' {
                cut = i - 1;
                break;
            }
            prev = ch;
        }
        out.push_str(&line[..cut]);
        out.push('\n');
    }
    out
}

fn words(s: &str) -> Vec<&str> {
    s.split(|c: char| !(c.is_alphanumeric() || c == '_'))
        .filter(|w| !w.is_empty())
        .collect()
}

#[test]
fn exe_477_disallowed() {
    let krate = Path::new(env!("CARGO_MANIFEST_DIR"));
    let root = krate.parent().unwrap().parent().unwrap();
    let cfg = read(&krate.join("clippy.toml"));
    // .2 / .3: clippy's disallowed lists (the hot template, byte-identical).
    assert_eq!(cfg, read(&root.join("tools/lint/clippy-hot.toml")), "EXE-477.2");
    assert!(cfg.contains("path = \"std::collections::HashMap\""), "EXE-477.2");
    assert!(cfg.contains("path = \"std::time::Instant::now\""), "EXE-477.3");
    // .1: float arithmetic denied by clippy; .1 / .4: no float or trait-object type anywhere in src.
    let lib = read(&krate.join("src/lib.rs"));
    assert!(lib.contains("clippy::float_arithmetic"), "EXE-477.1");
    assert!(lib.contains("#![forbid(unsafe_code)]"), "EXE-477.5");
    let mut n = 0;
    for e in std::fs::read_dir(krate.join("src")).unwrap() {
        let p = e.unwrap().path();
        if p.extension().is_some_and(|x| x == "rs") {
            n += 1;
            let code = code_of(&read(&p));
            for w in words(&code) {
                assert!(w != "f64" && w != "f32", "EXE-477.1: {} uses {w}", p.display());
                assert!(w != "dyn", "EXE-477.4: {} uses dyn", p.display());
                assert!(
                    w != "HashMap" && w != "HashSet" && w != "Instant" && w != "SystemTime",
                    "EXE-477.5: {} uses {w}",
                    p.display()
                );
            }
        }
    }
    assert!(n > 5);
}
