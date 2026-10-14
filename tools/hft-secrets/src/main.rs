fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    std::process::exit(hft_secrets::run(
        &args,
        &mut std::io::stdout().lock(),
        &mut std::io::stderr().lock(),
        hft_secrets::aws::Aws::new,
    ));
}
