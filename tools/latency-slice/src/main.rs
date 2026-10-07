//! `latency-slice` binary: see the library and tools/latency-slice/README.md "命令行".

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    std::process::exit(latency_slice::main_with(&args));
}
