//! `hft-cfg` binary: see the library (`hft_cfg::run`) and tools/README.md "hft-cfg".

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let code = hft_cfg::run(&args, &mut std::io::stdout(), &mut std::io::stderr());
    std::process::exit(code);
}
