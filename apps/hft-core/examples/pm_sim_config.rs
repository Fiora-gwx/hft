//! Generates the Polymarket sim configuration (`--dry-run`, `live_feed` venues), with no
//! credentials: book-observer on 16 slot insts, or with `--maker` the binary-maker simulation
//! (three BTC slot pairs plus the Binance spot BTCUSDT reference, queue fill model).
#[path = "../tests/support/mod.rs"]
mod support;
fn main() {
    let usage =
        "usage: pm_sim_config [--maker] OUTPUT_DIR TMPFS_DIR SRC_IP BIND_DEVICE [USER] [PIPELINE_USER]";
    let all: Vec<String> = std::env::args().skip(1).collect();
    let maker = all.iter().any(|a| a == "--maker");
    let mut args = all.into_iter().filter(|a| a != "--maker");
    let root = std::path::PathBuf::from(args.next().expect(usage));
    let hot = std::path::PathBuf::from(args.next().expect(usage));
    let src_ip = args.next().expect(usage);
    let dev = args.next().expect(usage);
    let user = args.next();
    let pipeline = args.next();
    let write = if maker {
        support::write_polymarket_maker_sim
    } else {
        support::write_polymarket_sim
    };
    let path = write(&root, &hot, user.as_deref(), pipeline.as_deref(), &src_ip, &dev);
    println!("{}", path.display());
}
