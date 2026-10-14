//! Generates a self-contained offline smoke configuration, with no credentials.
#[path = "../tests/support/mod.rs"]
mod support;
fn main() {
    let mut args = std::env::args().skip(1);
    let root = std::path::PathBuf::from(
        args.next()
            .expect("usage: paper_config OUTPUT_DIR TMPFS_DIR [USER]"),
    );
    let hot = std::path::PathBuf::from(
        args.next()
            .expect("TMPFS_DIR required, e.g. /dev/shm/hft-paper-1"),
    );
    let path = support::write(&root, &hot, args.next().as_deref());
    println!("{}", path.display());
}
