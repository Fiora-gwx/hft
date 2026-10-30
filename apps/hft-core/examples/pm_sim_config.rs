//! `pm_sim_config`: the Polymarket configuration generator under its earlier name; a shim over
//! [`hft_pmcfg::cli`], the same command line and output as the `hft-pmcfg` tool
//! (tools/hft-pmcfg/README.md "命令行"): no mode flag = the book-observer sim, `--maker` = the
//! binary-maker sim, `--live UID KEYS_DIR` = the live maker (with `--day-loss-carry`,
//! `--daily-loss`, `--daily-loss-resumes`, `--no-presign`, `--live-account UID KEYS_DIR`,
//! `--account LABEL:UID:ROLE KEYS_DIR`), `--param NAME=RAW`; then
//! `OUTPUT_DIR TMPFS_DIR SRC_IP BIND_DEVICE [USER] [PIPELINE_USER]`.
fn main() -> std::process::ExitCode {
    hft_pmcfg::cli::main("pm_sim_config")
}
