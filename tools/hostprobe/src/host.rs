//! Access to the host: every read goes through [`Host`], so the collectors run unchanged against the
//! real machine ([`crate::sys`], Linux) or a fixture root ([`FixtureHost`], any platform).
//!
//! Fixture root layout (tools/README.md "hostprobe"; `--root <dir>` replaces `/`):
//! - ordinary files at their absolute path below the root (`<root>/proc/cmdline`); a `:` in a path
//!   component is stored as `_` (`/run/systemd/units/invocation:x.service`);
//! - `<root>/.hostprobe/` holds the substitutes for what is not a file:
//!   `imds/<path>` (IMDSv2 answers; a directory listing is the file `index` in that directory),
//!   `exec/<program> <args…>.out` (stdout of an allowed external program), `ethtool/<ifname>.json`
//!   (the `SIOCETHTOOL` answers as JSON), `bpf/connect4` and `bpf/connect6` (one program name per
//!   line), `nft/tables` (one `<family> <name>` per line), `ifaddrs` (`<ifname> <ipv4>` per line);
//! - `<root>/.hostprobe/euid` (decimal) sets the effective uid (default 0); a non-root fixture refuses
//!   IMDS connections and BPF queries like a locked-down host.

use std::path::{Path, PathBuf};

use serde_json::Value;

/// Read-only view of a host. Paths are absolute (`/proc/cmdline`). Every method records what it
/// touched (see [`FixtureHost::trace`]).
pub trait Host {
    /// Text content of a file.
    fn read(&mut self, path: &str) -> Result<String, String>;
    /// Raw bytes of a file (character devices such as `/dev/cpu_dma_latency`).
    fn read_bytes(&mut self, path: &str) -> Result<Vec<u8>, String>;
    /// Entry names of a directory, sorted.
    fn list(&mut self, path: &str) -> Result<Vec<String>, String>;
    /// The path exists.
    fn exists(&mut self, path: &str) -> bool;
    /// Effective uid.
    fn euid(&self) -> u32;
    /// IMDSv2 `GET /latest/<path>` (`meta-data/instance-type`); a path ending in `/` is a listing.
    fn imds(&mut self, path: &str) -> Result<String, String>;
    /// Runs an allowed external program (only `chronyc` for collect, `rtla` for osnoise); returns
    /// stdout.
    fn exec(&mut self, program: &str, args: &[&str]) -> Result<String, String>;
    /// NIC facts read with the `SIOCETHTOOL` ioctl (keys: `driver`, `driver_version`, `queues`,
    /// `max_queues`, `rss_indir_size`, `coalesce`, `ring`, `offload`, `ntuple_rules`,
    /// `hw_rx_timestamp`, `phc_index`, `allowance_exceeded`).
    fn ethtool(&mut self, ifname: &str) -> Result<Value, String>;
    /// Names of the BPF programs attached to the root cgroup at `connect4`/`connect6`
    /// (`bpf(BPF_PROG_QUERY)`).
    fn bpf_connect_progs(&mut self, v6: bool) -> Result<Vec<String>, String>;
    /// Loaded nftables tables as `<family> <name>` (netlink query).
    fn nft_tables(&mut self) -> Result<Vec<String>, String>;
    /// IPv4 addresses configured on interfaces: `(ifname, address)`.
    fn if_addrs(&mut self) -> Result<Vec<(String, String)>, String>;
}

/// What a host access touched, in order.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Trace {
    /// Files and directories read (absolute host paths).
    pub files: Vec<String>,
    /// External programs started (`program arg1 arg2`).
    pub execs: Vec<String>,
    /// `SIOCETHTOOL` reads (`SIOCETHTOOL <ifname>`), BPF and netlink queries.
    pub ioctls: Vec<String>,
    /// IMDS paths requested.
    pub imds: Vec<String>,
}

/// A fixture root standing in for `/` (module docs).
#[derive(Debug, Clone)]
pub struct FixtureHost {
    pub root: PathBuf,
    pub euid: u32,
    pub trace: Trace,
}

impl FixtureHost {
    /// Fixture at `root`; the uid comes from `.hostprobe/euid` (default 0).
    pub fn new(root: &Path) -> FixtureHost {
        let euid = std::fs::read_to_string(root.join(".hostprobe/euid"))
            .ok()
            .and_then(|s| s.trim().parse().ok())
            .unwrap_or(0);
        FixtureHost {
            root: root.to_path_buf(),
            euid,
            trace: Trace::default(),
        }
    }

    fn map(&self, path: &str) -> PathBuf {
        let rel = path.trim_start_matches('/').replace(':', "_");
        self.root.join(rel)
    }

    fn sub(&self, rel: &str) -> PathBuf {
        self.root.join(".hostprobe").join(rel.replace(':', "_"))
    }
}

fn io(e: std::io::Error) -> String {
    match e.kind() {
        std::io::ErrorKind::NotFound => "not found".to_string(),
        std::io::ErrorKind::PermissionDenied => "permission denied".to_string(),
        _ => e.to_string(),
    }
}

impl Host for FixtureHost {
    fn read(&mut self, path: &str) -> Result<String, String> {
        self.trace.files.push(path.to_string());
        std::fs::read_to_string(self.map(path)).map_err(io)
    }

    fn read_bytes(&mut self, path: &str) -> Result<Vec<u8>, String> {
        self.trace.files.push(path.to_string());
        if self.euid != 0 && path.starts_with("/dev/") {
            return Err("permission denied".into());
        }
        std::fs::read(self.map(path)).map_err(io)
    }

    fn list(&mut self, path: &str) -> Result<Vec<String>, String> {
        self.trace.files.push(path.to_string());
        let mut v: Vec<String> = std::fs::read_dir(self.map(path))
            .map_err(io)?
            .flatten()
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .collect();
        v.sort();
        Ok(v)
    }

    fn exists(&mut self, path: &str) -> bool {
        self.trace.files.push(path.to_string());
        self.map(path).exists()
    }

    fn euid(&self) -> u32 {
        self.euid
    }

    fn imds(&mut self, path: &str) -> Result<String, String> {
        self.trace.imds.push(path.to_string());
        if self.euid != 0 {
            return Err("connect 169.254.169.254:80: connection refused".into());
        }
        let p = if path.ends_with('/') || path.is_empty() {
            self.sub(&format!("imds/{path}index"))
        } else {
            self.sub(&format!("imds/{path}"))
        };
        std::fs::read_to_string(p)
            .map(|s| s.trim_end().to_string())
            .map_err(|_| "HTTP 404".to_string())
    }

    fn exec(&mut self, program: &str, args: &[&str]) -> Result<String, String> {
        let line = std::iter::once(program)
            .chain(args.iter().copied())
            .collect::<Vec<_>>()
            .join(" ");
        self.trace.execs.push(line.clone());
        std::fs::read_to_string(self.sub(&format!("exec/{line}.out")))
            .map_err(|_| format!("{program}: not found"))
    }

    fn ethtool(&mut self, ifname: &str) -> Result<Value, String> {
        self.trace.ioctls.push(format!("SIOCETHTOOL {ifname}"));
        let text = std::fs::read_to_string(self.sub(&format!("ethtool/{ifname}.json"))).map_err(io)?;
        serde_json::from_str(&text).map_err(|e| format!("ethtool fixture: {e}"))
    }

    fn bpf_connect_progs(&mut self, v6: bool) -> Result<Vec<String>, String> {
        let at = if v6 { "connect6" } else { "connect4" };
        self.trace.ioctls.push(format!("BPF_PROG_QUERY {at}"));
        if self.euid != 0 {
            return Err("bpf: operation not permitted".into());
        }
        let text = std::fs::read_to_string(self.sub(&format!("bpf/{at}"))).unwrap_or_default();
        Ok(text
            .lines()
            .map(str::trim)
            .filter(|l| !l.is_empty())
            .map(str::to_string)
            .collect())
    }

    fn nft_tables(&mut self) -> Result<Vec<String>, String> {
        self.trace.ioctls.push("NFT_MSG_GETTABLE".into());
        if self.euid != 0 {
            return Err("netlink: operation not permitted".into());
        }
        let text = std::fs::read_to_string(self.sub("nft/tables")).unwrap_or_default();
        Ok(text
            .lines()
            .map(str::trim)
            .filter(|l| !l.is_empty())
            .map(str::to_string)
            .collect())
    }

    fn if_addrs(&mut self) -> Result<Vec<(String, String)>, String> {
        self.trace.ioctls.push("getifaddrs".into());
        let text = std::fs::read_to_string(self.sub("ifaddrs")).map_err(io)?;
        Ok(text
            .lines()
            .filter_map(|l| {
                let mut it = l.split_whitespace();
                Some((it.next()?.to_string(), it.next()?.to_string()))
            })
            .collect())
    }
}
