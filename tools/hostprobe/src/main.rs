//! `hostprobe` binary; see the library docs and tools/README.md "hostprobe".

use std::process::ExitCode;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    ExitCode::from(hostprobe::run(&args) as u8)
}
