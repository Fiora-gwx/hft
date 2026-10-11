fn main() {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    let code = replayer::run::<hft_strategy_registry::Strategies>(
        &args,
        &mut std::io::stdout(),
        &mut std::io::stderr(),
    );
    std::process::exit(code);
}
