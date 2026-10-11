//! CI's replay test assembly. The shipped binary continues to use only the production registry.
fn main() {
    let mut args = std::env::args().skip(1).collect::<Vec<_>>();
    let path_v2 = args.first().is_some_and(|s| s == "--fixture-path-v2");
    if path_v2 {
        args.remove(0);
    }
    let warmup = args.iter().position(|s| s == "--fixture-warmup");
    if let Some(i) = warmup {
        args.remove(i);
    }
    let code = if warmup.is_some() {
        replayer::run::<hft_testkit::strategies::ReplayWarmupStrategies>(
            &args,
            &mut std::io::stdout(),
            &mut std::io::stderr(),
        )
    } else if path_v2 {
        replayer::run::<hft_testkit::strategies::ReplayPathV2Strategies>(
            &args,
            &mut std::io::stdout(),
            &mut std::io::stderr(),
        )
    } else {
        replayer::run::<hft_testkit::strategies::ReplayStrategies>(
            &args,
            &mut std::io::stdout(),
            &mut std::io::stderr(),
        )
    };
    std::process::exit(code);
}
