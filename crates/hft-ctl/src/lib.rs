//! `hft-ctl`: the ctl-gateway (warm task: Unix socket, authentication by peer uid and issuer,
//! permission caps, normalization through hft-ctl-proto, gateway-side command checks,
//! all-or-nothing writes to the ctl ring with the audit Note, the `ParsedConfig` Mailbox and
//! `CmdResult` return) and the Rust client. Specification: `crates/hft-ctl/README.md`.
#![deny(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::todo,
    clippy::unimplemented
)]

pub mod config;
pub mod core;
pub mod parsed;
pub mod principals;
pub mod ring;

#[cfg(target_os = "linux")]
pub mod gateway;

#[cfg(unix)]
pub mod client;

pub use config::GatewayConfig;
pub use core::{ConnId, Core, InternalCmd, RefdataAssemblyCheck, Reply, RiskDecode};
pub use parsed::{ParsedConfig, ParsedParams};
pub use principals::{PrincipalTable, PrincipalsErr, UidKind};
pub use ring::{CtlRing, VecRing};

/// Gateway-side command checks (the error mapping of README "gateway 侧的命令校验").
pub mod validate {
    pub use crate::core::{EventErr, FUNDING_INTERVALS_S, detail_of_event_err};
}
