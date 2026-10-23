//! `NetConfig`: the host/connection configuration carried by hft-core (README "配置项").

use std::net::Ipv4Addr;
use std::path::PathBuf;

use hft_event::ConnRole;

/// One endpoint (`endpoints[]`).
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct Endpoint {
    /// HTTP `Host` header and, when `ips` is empty, the name resolved on the warm thread.
    pub host: String,
    /// TLS server name (SNI and certificate check).
    pub sni: String,
    pub port: u16,
    /// Request target of the WS Upgrade (`/ws?x=1`); unused by Rest connections.
    pub path: String,
    /// Addresses pinned at deploy time; empty → resolve `host`.
    pub ips: Vec<Ipv4Addr>,
    /// Extra Upgrade request headers given by the upper layer (e.g. `X-MBX-APIKEY`).
    pub upgrade_headers: Vec<(String, String)>,
}

/// Per-role byte sizes.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub struct RoleBytes {
    pub md: u32,
    pub order: u32,
    pub user_stream: u32,
    pub rest: u32,
}

impl RoleBytes {
    pub fn get(&self, role: ConnRole) -> u32 {
        match role {
            ConnRole::Md => self.md,
            ConnRole::Order => self.order,
            ConnRole::UserStream => self.user_stream,
            ConnRole::Rest => self.rest,
        }
    }

    /// All four roles set to `n`.
    pub fn all(n: u32) -> RoleBytes {
        RoleBytes {
            md: n,
            order: n,
            user_stream: n,
            rest: n,
        }
    }
}

/// `quickack_after_read`: per traffic kind (Md → `md`; Order and UserStream → `order`).
#[derive(Copy, Clone, Debug, Default, Eq, PartialEq)]
pub struct Quickack {
    pub md: bool,
    pub order: bool,
}

impl Quickack {
    pub fn for_role(&self, role: ConnRole) -> bool {
        match role {
            ConnRole::Md => self.md,
            ConnRole::Order | ConnRole::UserStream => self.order,
            ConnRole::Rest => false,
        }
    }
}

/// `tls_mode`.
#[derive(Copy, Clone, Debug, Default, Eq, PartialEq)]
pub enum TlsMode {
    /// rustls handshake, then this crate's in-place record layer (README §1).
    #[default]
    Unbuffered,
    /// rustls `ClientConnection` (M3; not built yet).
    Buffered,
}

/// The configuration (README "配置项"); `Default` gives the table's defaults.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NetConfig {
    pub endpoints: Vec<Endpoint>,
    /// Source address → interface name (`SO_BINDTODEVICE`).
    pub bind_device: Vec<(Ipv4Addr, String)>,
    pub src_ips: Vec<Ipv4Addr>,
    pub src_port_range: (u16, u16),
    pub rss_plan: Option<PathBuf>,
    pub tx_timestamps: bool,
    /// Extra trusted CA (PEM); loaded only when `endpoint_override` is set.
    pub extra_ca: Option<PathBuf>,
    /// The hft-core configuration carries `endpoint_override` (mock-exchange loopback runs).
    pub endpoint_override: bool,
    pub rx_buf: RoleBytes,
    pub tx_buf: u32,
    pub max_msg: u32,
    pub so_rcvbuf: u32,
    pub so_sndbuf: u32,
    pub tcp_user_timeout_ms: u32,
    pub quickack_after_read: Quickack,
    pub connect_timeout_ms: u32,
    pub tls_timeout_ms: u32,
    pub upgrade_timeout_ms: u32,
    /// Failed WS builds on the same endpoint/source IP wait exponentially; REST is not retried.
    pub reconnect_backoff_ms: (u32, u32),
    pub rotate_stagger_s: u32,
    pub warm_interval_ns: u64,
    /// A REST request without a complete response this long after `submit` fails with
    /// `RestErr::Timeout` (the connection retires; a late answer is never read). Must not exceed
    /// the OMS `ack_timeout_ms` of the venue.
    pub rest_timeout_ms: u32,
    /// A pooled REST connection with no request for this long is retired by `idle_check` and
    /// rebuilt off the hot thread before it is used again (one per check, only while the pool is
    /// full); 0 = never.
    pub rest_max_idle_ms: u32,
    pub ts_expect_hw: bool,
    pub tls_mode: TlsMode,
}

impl Default for NetConfig {
    fn default() -> Self {
        NetConfig {
            endpoints: Vec::new(),
            bind_device: Vec::new(),
            src_ips: Vec::new(),
            src_port_range: (40_000, 40_999),
            rss_plan: None,
            tx_timestamps: false,
            extra_ca: None,
            endpoint_override: false,
            rx_buf: RoleBytes {
                md: 256 * 1024,
                order: 64 * 1024,
                user_stream: 64 * 1024,
                rest: 1024 * 1024,
            },
            tx_buf: 16 * 1024,
            max_msg: 256 * 1024,
            so_rcvbuf: 262_144,
            so_sndbuf: 65_536,
            tcp_user_timeout_ms: 5000,
            quickack_after_read: Quickack {
                md: false,
                order: false,
            },
            connect_timeout_ms: 3000,
            tls_timeout_ms: 3000,
            upgrade_timeout_ms: 3000,
            reconnect_backoff_ms: (100, 30_000),
            rotate_stagger_s: 3600,
            warm_interval_ns: 0,
            rest_timeout_ms: 2000,
            rest_max_idle_ms: 0,
            ts_expect_hw: true,
            tls_mode: TlsMode::Unbuffered,
        }
    }
}

/// The per-connection subset of `NetConfig` a `Conn` keeps.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub struct ConnCfg {
    pub rx_buf: u32,
    pub tx_buf: u32,
    pub max_msg: u32,
    pub ts_expect_hw: bool,
    pub quickack: bool,
}

impl ConnCfg {
    pub fn from_net(cfg: &NetConfig, role: ConnRole) -> ConnCfg {
        ConnCfg {
            rx_buf: cfg.rx_buf.get(role),
            tx_buf: cfg.tx_buf,
            max_msg: cfg.max_msg,
            ts_expect_hw: cfg.ts_expect_hw,
            quickack: cfg.quickack_after_read.for_role(role),
        }
    }
}
