//! Generates the Polymarket sim configuration (`--dry-run`, `live_feed` venues), with no
//! credentials: book-observer on 16 slot insts, or with `--maker` the binary-maker simulation
//! (three BTC slot pairs plus the Binance spot BTCUSDT reference, queue fill model). With
//! `--live UID KEYS_DIR` the live maker instead (`order_env = "prod"`, account `pm-live` with venue
//! UID `UID`, credential files in `KEYS_DIR`, micro parameters and risk limits).
#[path = "../tests/support/mod.rs"]
mod support;
fn main() {
    let usage = "usage: pm_sim_config [--maker | --live UID KEYS_DIR] OUTPUT_DIR TMPFS_DIR SRC_IP BIND_DEVICE [USER] [PIPELINE_USER]";
    let mut all: Vec<String> = std::env::args().skip(1).collect();
    let live = all.iter().position(|a| a == "--live").map(|i| {
        let mut v: Vec<String> = all.drain(i..i + 3).collect();
        let keys_dir = std::path::PathBuf::from(v.pop().expect(usage));
        let uid = v.pop().expect(usage);
        support::PmLive { uid, keys_dir }
    });
    let maker = all.iter().any(|a| a == "--maker");
    let mut args = all.into_iter().filter(|a| a != "--maker");
    let root = std::path::PathBuf::from(args.next().expect(usage));
    let hot = std::path::PathBuf::from(args.next().expect(usage));
    let src_ip = args.next().expect(usage);
    let dev = args.next().expect(usage);
    let user = args.next();
    let pipeline = args.next();
    let path = if let Some(l) = &live {
        support::write_polymarket_maker_live(
            &root,
            &hot,
            user.as_deref(),
            pipeline.as_deref(),
            &src_ip,
            &dev,
            l,
        )
    } else {
        let write = if maker {
            support::write_polymarket_maker_sim
        } else {
            support::write_polymarket_sim
        };
        write(&root, &hot, user.as_deref(), pipeline.as_deref(), &src_ip, &dev)
    };
    println!("{}", path.display());
}
