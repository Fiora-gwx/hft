//! TOOL-175: `hft-cfg build --kind strategy` of a 500-group strategy blob (tools/README.md
//! "延迟与资源契约": ≤ 1 s). Schema directory and refdata are the test fixtures of TOOL-119.

use std::path::{Path, PathBuf};

use criterion::{Criterion, criterion_group, criterion_main};

fn cfg(args: &[&str]) -> (i32, String) {
    let args: Vec<String> = args.iter().map(|s| s.to_string()).collect();
    let (mut o, mut e) = (Vec::new(), Vec::new());
    let code = hft_cfg::run(&args, &mut o, &mut e);
    (code, String::from_utf8_lossy(&e).into_owned())
}

fn s(p: &Path) -> &str {
    p.to_str().unwrap_or_default()
}

fn tool_175_build_strategy_500(c: &mut Criterion) {
    let fx = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
    let dir = std::env::temp_dir().join(format!("hft-cfg-bench-175-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let refdata = dir.join("refdata.blob");
    let (code, err) = cfg(&[
        "build",
        "--kind",
        "refdata",
        "--in",
        s(&fx.join("refdata-basic")),
        "--out",
        s(&refdata),
        "--schemas",
        s(&fx.join("schemas")),
    ]);
    assert_eq!(code, 0, "refdata: {err}");
    let mut toml = String::from(
        "[strategy]\ninstance = \"pc-main\"\naccount = \"pc-live\"\nschema = { id = 65280, version = 2 }\nversion = 1\nsource = \"manual\"\n\n[defaults]\nspread_th = \"0.0005\"\n",
    );
    for n in 1..=500 {
        toml += &format!(
            "\n[[groups]]\nn = {n}\nasset = \"AIN\"\nopen_allowed = true\nlegs = [ {{ inst = \"binance-usdm:AINUSDT\" }}, {{ inst = \"bitget-uta-spot:AINUSDT\" }} ]\n[groups.overrides]\nmax_qty = \"{n}\"\n"
        );
    }
    let input = dir.join("pc-main.pc-live.toml");
    std::fs::write(&input, toml).unwrap();
    let out = dir.join("s.blob");
    let schemas = fx.join("schemas");
    let args = [
        "build",
        "--kind",
        "strategy",
        "--schemas",
        s(&schemas),
        "--refdata",
        s(&refdata),
        "--in",
        s(&input),
        "--out",
        s(&out),
    ];
    let (code, err) = cfg(&args);
    assert_eq!(code, 0, "strategy: {err}");
    c.bench_function("hft-cfg/build_strategy_500", |b| {
        b.iter(|| assert_eq!(cfg(&args).0, 0));
    });
    let _ = std::fs::remove_dir_all(&dir);
}

criterion_group! {
    name = benches;
    config = Criterion::default().sample_size(10);
    targets = tool_175_build_strategy_500
}
criterion_main!(benches);
