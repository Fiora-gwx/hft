//! Generates the Polymarket market-data sim configuration (`--dry-run`, `live_feed` venue,
//! book-observer on 16 slot insts), with no credentials.
#[path = "../tests/support/mod.rs"]
mod support;
fn main() {
    let usage = "usage: pm_sim_config OUTPUT_DIR TMPFS_DIR SRC_IP BIND_DEVICE [USER] [PIPELINE_USER]";
    let mut args = std::env::args().skip(1);
    let root = std::path::PathBuf::from(args.next().expect(usage));
    let hot = std::path::PathBuf::from(args.next().expect(usage));
    let src_ip = args.next().expect(usage);
    let dev = args.next().expect(usage);
    let user = args.next();
    let pipeline = args.next();
    let path =
        support::write_polymarket_sim(&root, &hot, user.as_deref(), pipeline.as_deref(), &src_ip, &dev);
    println!("{}", path.display());
}
