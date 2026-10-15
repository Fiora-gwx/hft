//! Process assembly; business state belongs exclusively to hft-sequencer.
pub mod budget;
pub mod config;
mod config_shape;
pub mod io;
mod latency_trace;
pub mod paper;
pub mod recovery;
pub mod runtime;
pub mod secrets;
#[cfg(target_os = "linux")]
pub mod signals;
pub mod startup;
pub mod storage;
pub mod venues;

pub use hft_strategy_registry::Strategies;

#[derive(Debug)]
pub struct Error {
    pub code: &'static str,
    pub detail: String,
    pub exit: u8,
}
impl Error {
    pub fn config(code: &'static str, detail: impl Into<String>) -> Self {
        Self {
            code,
            detail: detail.into(),
            exit: 2,
        }
    }
    pub fn runtime(code: &'static str, detail: impl Into<String>, exit: u8) -> Self {
        Self {
            code,
            detail: detail.into(),
            exit,
        }
    }
}
impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "hft-core: error[{}] {}", self.code, self.detail)
    }
}
impl std::error::Error for Error {}

pub fn exit_code(cause: hft_sequencer::DoneCause) -> u8 {
    use hft_sequencer::DoneCause::*;
    match cause {
        Drain => 0,
        ReplaceRefdata => 75,
        StorageCritical => 3,
        IdentityMismatch => 2,
    }
}
