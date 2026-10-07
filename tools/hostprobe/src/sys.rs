//! The real host (Linux): files under `/`, IMDSv2 over HTTP, `SIOCETHTOOL` ioctls,
//! `bpf(BPF_PROG_QUERY)`, an nftables netlink dump and `getifaddrs`. Read-only: nothing here writes
//! host state (the nftables query is skipped when `nf_tables` is not loaded, so it never autoloads
//! a module).

use std::ffi::CStr;
use std::io::{Read, Write};
use std::net::{Ipv4Addr, SocketAddr, TcpStream};
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};
use std::time::Duration;

use serde_json::{Map, Value, json};

use crate::host::Host;

/// The machine hostprobe runs on.
#[derive(Debug, Default)]
pub struct SysHost {
    token: Option<String>,
    /// First IMDS connection failure; later requests fail fast with it.
    imds_err: Option<String>,
}

impl SysHost {
    pub fn new() -> SysHost {
        SysHost::default()
    }
}

fn io(e: std::io::Error) -> String {
    match e.kind() {
        std::io::ErrorKind::NotFound => "not found".to_string(),
        std::io::ErrorKind::PermissionDenied => "permission denied".to_string(),
        _ => e.to_string(),
    }
}

fn errno() -> String {
    std::io::Error::last_os_error().to_string()
}

/// One HTTP/1.1 request to IMDS; returns `(status, body)`.
fn http(method: &str, path: &str, headers: &[(&str, &str)]) -> Result<(u16, String), String> {
    let addr = SocketAddr::from((Ipv4Addr::new(169, 254, 169, 254), 80));
    let mut s = TcpStream::connect_timeout(&addr, Duration::from_secs(1))
        .map_err(|e| format!("connect 169.254.169.254:80: {e}"))?;
    let _ = s.set_read_timeout(Some(Duration::from_secs(2)));
    let mut req = format!("{method} {path} HTTP/1.1\r\nHost: 169.254.169.254\r\nConnection: close\r\n");
    for (k, v) in headers {
        req += &format!("{k}: {v}\r\n");
    }
    req += "Content-Length: 0\r\n\r\n";
    s.write_all(req.as_bytes()).map_err(|e| e.to_string())?;
    let mut buf = Vec::new();
    s.read_to_end(&mut buf).map_err(|e| e.to_string())?;
    let text = String::from_utf8_lossy(&buf).into_owned();
    let (head, body) = text.split_once("\r\n\r\n").unwrap_or((&text, ""));
    let status = head
        .split_whitespace()
        .nth(1)
        .and_then(|c| c.parse().ok())
        .ok_or("bad HTTP response")?;
    Ok((status, body.to_string()))
}

#[repr(C)]
struct IfReq {
    name: [u8; 16],
    data: *mut libc::c_void,
    _pad: [u8; 16],
}

const SIOCETHTOOL: libc::c_ulong = 0x8946;

struct Eth {
    fd: OwnedFd,
    name: [u8; 16],
}

impl Eth {
    fn open(ifname: &str) -> Result<Eth, String> {
        if ifname.len() >= 16 {
            return Err("interface name too long".into());
        }
        // SAFETY: plain socket(2) call; the returned descriptor is owned by `OwnedFd` below.
        let fd = unsafe { libc::socket(libc::AF_INET, libc::SOCK_DGRAM, 0) };
        if fd < 0 {
            return Err(format!("socket: {}", errno()));
        }
        // SAFETY: `fd` is a fresh, valid descriptor that nothing else owns.
        let fd = unsafe { OwnedFd::from_raw_fd(fd) };
        let mut name = [0u8; 16];
        name[..ifname.len()].copy_from_slice(ifname.as_bytes());
        Ok(Eth { fd, name })
    }

    /// `SIOCETHTOOL` with `buf` as the command structure (first u32 = command).
    fn call(&self, buf: &mut [u8]) -> Result<(), i32> {
        let mut req = IfReq {
            name: self.name,
            data: buf.as_mut_ptr().cast(),
            _pad: [0; 16],
        };
        // SAFETY: `req` is a valid ifreq whose data pointer refers to `buf`, which is large enough
        // for the command written into its first four bytes (callers size it per command).
        let r = unsafe { libc::ioctl(self.fd.as_raw_fd(), SIOCETHTOOL as _, &mut req as *mut IfReq) };
        if r < 0 {
            Err(std::io::Error::last_os_error().raw_os_error().unwrap_or(0))
        } else {
            Ok(())
        }
    }

    fn words(&self, cmd: u32, n: usize) -> Option<Vec<u32>> {
        let mut b = vec![0u8; n * 4];
        b[..4].copy_from_slice(&cmd.to_ne_bytes());
        self.call(&mut b).ok()?;
        Some(
            b.chunks(4)
                .map(|c| u32::from_ne_bytes([c[0], c[1], c[2], c[3]]))
                .collect(),
        )
    }

    fn value(&self, cmd: u32) -> Option<u32> {
        self.words(cmd, 2).map(|w| w[1])
    }
}

fn cstr(b: &[u8]) -> String {
    let end = b.iter().position(|c| *c == 0).unwrap_or(b.len());
    String::from_utf8_lossy(&b[..end]).into_owned()
}

const UNREADABLE: &str = "unreadable";

fn ethtool_info(ifname: &str) -> Result<Value, String> {
    let e = Eth::open(ifname)?;
    let u = || json!(UNREADABLE);
    let mut m = Map::new();
    // ETHTOOL_GDRVINFO: driver[32] at 4, version[32] at 36.
    let mut d = vec![0u8; 196];
    d[..4].copy_from_slice(&0x3u32.to_ne_bytes());
    match e.call(&mut d) {
        Ok(()) => {
            m.insert("driver".into(), json!(cstr(&d[4..36])));
            m.insert("driver_version".into(), json!(cstr(&d[36..68])));
        }
        Err(code) => return Err(format!("SIOCETHTOOL GDRVINFO {ifname}: errno {code}")),
    }
    // ETHTOOL_GCHANNELS: max_combined = [4], combined_count = [8].
    match e.words(0x3c, 9) {
        Some(w) => {
            m.insert("queues".into(), json!(w[8]));
            m.insert("max_queues".into(), json!(w[4]));
        }
        None => {
            m.insert("queues".into(), u());
            m.insert("max_queues".into(), u());
        }
    }
    // ETHTOOL_GRXFHINDIR with size 0 returns the table size.
    m.insert(
        "rss_indir_size".into(),
        e.words(0x38, 2).map(|w| json!(w[1])).unwrap_or_else(u),
    );
    // ETHTOOL_GCOALESCE: rx_usecs [1], tx_usecs [5], adaptive rx [10], adaptive tx [11].
    m.insert(
        "coalesce".into(),
        e.words(0x0e, 23)
            .map(|w| json!({"rx_usecs": w[1], "tx_usecs": w[5], "adaptive_rx": w[10] != 0, "adaptive_tx": w[11] != 0}))
            .unwrap_or_else(u),
    );
    // ETHTOOL_GRINGPARAM: rx_pending [5], tx_pending [8].
    m.insert(
        "ring".into(),
        e.words(0x10, 9)
            .map(|w| json!({"rx": w[5], "tx": w[8], "rx_max": w[1], "tx_max": w[4]}))
            .unwrap_or_else(u),
    );
    // Legacy offload getters; LRO is bit 15 of ETHTOOL_GFLAGS.
    let b = |x: Option<u32>| x.map(|v| json!(v != 0)).unwrap_or_else(u);
    m.insert(
        "offload".into(),
        json!({
            "gro": b(e.value(0x2b)),
            "tso": b(e.value(0x1e)),
            "gso": b(e.value(0x23)),
            "sg": b(e.value(0x18)),
            "lro": b(e.value(0x25).map(|f| f & (1 << 15))),
        }),
    );
    // ETHTOOL_GRXCLSRLCNT: struct ethtool_rxnfc, rule_cnt at offset 168.
    let mut nfc = vec![0u8; 256];
    nfc[..4].copy_from_slice(&0x2eu32.to_ne_bytes());
    m.insert(
        "ntuple_rules".into(),
        match e.call(&mut nfc) {
            Ok(()) => json!(u32::from_ne_bytes([nfc[168], nfc[169], nfc[170], nfc[171]])),
            Err(libc::EOPNOTSUPP) => json!("unsupported"),
            Err(_) => u(),
        },
    );
    // ETHTOOL_GET_TS_INFO: so_timestamping [1], phc_index [2].
    match e.words(0x41, 12) {
        Some(w) => {
            m.insert("hw_rx_timestamp".into(), json!(w[1] & (1 << 2) != 0));
            m.insert("phc_index".into(), json!(w[2] as i32));
        }
        None => {
            m.insert("hw_rx_timestamp".into(), u());
            m.insert("phc_index".into(), u());
        }
    }
    m.insert("allowance_exceeded".into(), allowance(&e).unwrap_or_else(u));
    Ok(Value::Object(m))
}

/// ENA `*_allowance_exceeded` counters from the ETH_SS_STATS string set.
fn allowance(e: &Eth) -> Option<Value> {
    // ETHTOOL_GSSET_INFO, sset_mask = 1 << ETH_SS_STATS.
    let mut si = vec![0u8; 24];
    si[..4].copy_from_slice(&0x37u32.to_ne_bytes());
    si[8..16].copy_from_slice(&(1u64 << 1).to_ne_bytes());
    e.call(&mut si).ok()?;
    let n = u32::from_ne_bytes([si[16], si[17], si[18], si[19]]) as usize;
    // ETHTOOL_GSTRINGS: cmd, string_set, len, data[len * 32].
    let mut gs = vec![0u8; 12 + n * 32];
    gs[..4].copy_from_slice(&0x1bu32.to_ne_bytes());
    gs[4..8].copy_from_slice(&1u32.to_ne_bytes());
    gs[8..12].copy_from_slice(&(n as u32).to_ne_bytes());
    e.call(&mut gs).ok()?;
    // ETHTOOL_GSTATS: cmd, n_stats, data[n] (u64).
    let mut st = vec![0u8; 8 + n * 8];
    st[..4].copy_from_slice(&0x1du32.to_ne_bytes());
    st[4..8].copy_from_slice(&(n as u32).to_ne_bytes());
    e.call(&mut st).ok()?;
    let mut m = Map::new();
    for i in 0..n {
        let name = cstr(&gs[12 + i * 32..12 + (i + 1) * 32]);
        if let Some(k) = name.strip_suffix("_allowance_exceeded") {
            let o = 8 + i * 8;
            let mut v = [0u8; 8];
            v.copy_from_slice(&st[o..o + 8]);
            m.insert(k.to_string(), json!(u64::from_ne_bytes(v)));
        }
    }
    Some(Value::Object(m))
}

const BPF_PROG_GET_FD_BY_ID: libc::c_long = 13;
const BPF_OBJ_GET_INFO_BY_FD: libc::c_long = 15;
const BPF_PROG_QUERY: libc::c_long = 16;

fn bpf(cmd: libc::c_long, attr: &mut [u8]) -> Result<libc::c_long, String> {
    // SAFETY: `attr` is a zero-initialised buffer at least as large as the bpf_attr variant of
    // `cmd`; its size is passed so the kernel never reads past it.
    let r = unsafe { libc::syscall(libc::SYS_bpf, cmd, attr.as_mut_ptr(), attr.len()) };
    if r < 0 { Err(errno()) } else { Ok(r) }
}

fn bpf_names(v6: bool) -> Result<Vec<String>, String> {
    let cg = std::fs::File::open("/sys/fs/cgroup").map_err(io)?;
    let mut ids = [0u32; 64];
    let mut a = [0u8; 64];
    a[..4].copy_from_slice(&(cg.as_raw_fd() as u32).to_ne_bytes());
    let at: u32 = if v6 { 11 } else { 10 }; // BPF_CGROUP_INET{4,6}_CONNECT
    a[4..8].copy_from_slice(&at.to_ne_bytes());
    a[16..24].copy_from_slice(&(ids.as_mut_ptr() as u64).to_ne_bytes());
    a[24..28].copy_from_slice(&(ids.len() as u32).to_ne_bytes());
    bpf(BPF_PROG_QUERY, &mut a)?;
    let cnt = u32::from_ne_bytes([a[24], a[25], a[26], a[27]]) as usize;
    let mut out = Vec::new();
    for id in ids.iter().take(cnt.min(ids.len())) {
        let mut g = [0u8; 64];
        g[..4].copy_from_slice(&id.to_ne_bytes());
        let fd = bpf(BPF_PROG_GET_FD_BY_ID, &mut g)?;
        // SAFETY: the kernel returned a fresh descriptor that nothing else owns.
        let fd = unsafe { OwnedFd::from_raw_fd(fd as i32) };
        let mut info = [0u8; 256];
        let mut q = [0u8; 64];
        q[..4].copy_from_slice(&(fd.as_raw_fd() as u32).to_ne_bytes());
        q[4..8].copy_from_slice(&(info.len() as u32).to_ne_bytes());
        q[8..16].copy_from_slice(&(info.as_mut_ptr() as u64).to_ne_bytes());
        bpf(BPF_OBJ_GET_INFO_BY_FD, &mut q)?;
        out.push(cstr(&info[64..80]));
    }
    Ok(out)
}

fn nft_family(f: u8) -> &'static str {
    match f {
        1 => "inet",
        2 => "ip",
        3 => "arp",
        5 => "netdev",
        7 => "bridge",
        10 => "ip6",
        _ => "unknown",
    }
}

fn nft_list() -> Result<Vec<String>, String> {
    // Never autoload nf_tables / nfnetlink: no module, no tables.
    if !std::path::Path::new("/sys/module/nf_tables").exists() {
        return Ok(Vec::new());
    }
    // SAFETY: plain socket(2) call; the descriptor is owned by `OwnedFd` below.
    let fd = unsafe {
        libc::socket(
            libc::AF_NETLINK,
            libc::SOCK_RAW | libc::SOCK_CLOEXEC,
            libc::NETLINK_NETFILTER,
        )
    };
    if fd < 0 {
        return Err(format!("netlink socket: {}", errno()));
    }
    // SAFETY: `fd` is a fresh, valid descriptor that nothing else owns.
    let fd = unsafe { OwnedFd::from_raw_fd(fd) };
    let mut msg = [0u8; 20];
    msg[..4].copy_from_slice(&20u32.to_ne_bytes());
    msg[4..6].copy_from_slice(&((10u16 << 8) | 1).to_ne_bytes()); // NFNL_SUBSYS_NFTABLES, NFT_MSG_GETTABLE
    msg[6..8].copy_from_slice(&(1u16 | 0x300).to_ne_bytes()); // NLM_F_REQUEST | NLM_F_DUMP
    msg[8..12].copy_from_slice(&1u32.to_ne_bytes());
    // SAFETY: `msg` is a valid buffer of the given length; the kernel netlink address is zeroed.
    let r = unsafe {
        let mut sa: libc::sockaddr_nl = std::mem::zeroed();
        sa.nl_family = libc::AF_NETLINK as u16;
        libc::sendto(
            fd.as_raw_fd(),
            msg.as_ptr().cast(),
            msg.len(),
            0,
            (&sa as *const libc::sockaddr_nl).cast(),
            std::mem::size_of::<libc::sockaddr_nl>() as u32,
        )
    };
    if r < 0 {
        return Err(format!("netlink send: {}", errno()));
    }
    let mut out = Vec::new();
    let mut buf = vec![0u8; 65536];
    loop {
        // SAFETY: `buf` is a valid writable buffer of the given length.
        let n = unsafe { libc::recv(fd.as_raw_fd(), buf.as_mut_ptr().cast(), buf.len(), 0) };
        if n <= 0 {
            return Err(format!("netlink recv: {}", errno()));
        }
        let mut off = 0usize;
        let n = n as usize;
        while off + 16 <= n {
            let len = u32::from_ne_bytes([buf[off], buf[off + 1], buf[off + 2], buf[off + 3]]) as usize;
            let ty = u16::from_ne_bytes([buf[off + 4], buf[off + 5]]);
            if len < 16 || off + len > n {
                return Ok(out);
            }
            match ty {
                3 => return Ok(out), // NLMSG_DONE
                2 => {
                    let code =
                        i32::from_ne_bytes([buf[off + 16], buf[off + 17], buf[off + 18], buf[off + 19]]);
                    return Err(format!("netlink error {code}"));
                }
                _ => {
                    let fam = buf[off + 16];
                    let mut a = off + 20;
                    while a + 4 <= off + len {
                        let alen = u16::from_ne_bytes([buf[a], buf[a + 1]]) as usize;
                        let aty = u16::from_ne_bytes([buf[a + 2], buf[a + 3]]) & 0x3fff;
                        if alen < 4 {
                            break;
                        }
                        if aty == 1 {
                            out.push(format!("{} {}", nft_family(fam), cstr(&buf[a + 4..a + alen])));
                        }
                        a += (alen + 3) & !3;
                    }
                }
            }
            off += (len + 3) & !3;
        }
    }
}

fn if_addrs_list() -> Result<Vec<(String, String)>, String> {
    let mut head: *mut libc::ifaddrs = std::ptr::null_mut();
    // SAFETY: getifaddrs writes a list head we free with freeifaddrs below.
    if unsafe { libc::getifaddrs(&mut head) } != 0 {
        return Err(format!("getifaddrs: {}", errno()));
    }
    let mut out = Vec::new();
    let mut p = head;
    while !p.is_null() {
        // SAFETY: `p` is a node of the list returned by getifaddrs, valid until freeifaddrs.
        let ifa = unsafe { &*p };
        if !ifa.ifa_addr.is_null() {
            // SAFETY: `ifa_addr` is non-null and points to a sockaddr of at least its family's size.
            let fam = unsafe { (*ifa.ifa_addr).sa_family } as i32;
            if fam == libc::AF_INET {
                // SAFETY: family AF_INET means the address is a sockaddr_in; the name is a C string.
                let (addr, name) = unsafe {
                    let sin = &*(ifa.ifa_addr as *const libc::sockaddr_in);
                    (
                        Ipv4Addr::from(u32::from_be(sin.sin_addr.s_addr)),
                        CStr::from_ptr(ifa.ifa_name).to_string_lossy().into_owned(),
                    )
                };
                out.push((name, addr.to_string()));
            }
        }
        p = ifa.ifa_next;
    }
    // SAFETY: `head` came from getifaddrs and is freed exactly once.
    unsafe { libc::freeifaddrs(head) };
    Ok(out)
}

impl Host for SysHost {
    fn read(&mut self, path: &str) -> Result<String, String> {
        std::fs::read_to_string(path).map_err(io)
    }

    fn read_bytes(&mut self, path: &str) -> Result<Vec<u8>, String> {
        let mut f = std::fs::File::open(path).map_err(io)?;
        let mut b = vec![0u8; 64];
        let n = f.read(&mut b).map_err(io)?;
        b.truncate(n);
        Ok(b)
    }

    fn list(&mut self, path: &str) -> Result<Vec<String>, String> {
        let mut v: Vec<String> = std::fs::read_dir(path)
            .map_err(io)?
            .flatten()
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .collect();
        v.sort();
        Ok(v)
    }

    fn exists(&mut self, path: &str) -> bool {
        std::fs::symlink_metadata(path).is_ok()
    }

    fn euid(&self) -> u32 {
        // SAFETY: geteuid has no preconditions and cannot fail.
        unsafe { libc::geteuid() }
    }

    fn imds(&mut self, path: &str) -> Result<String, String> {
        if let Some(e) = &self.imds_err {
            return Err(e.clone());
        }
        if self.token.is_none() {
            let (st, body) = http(
                "PUT",
                "/latest/api/token",
                &[("X-aws-ec2-metadata-token-ttl-seconds", "300")],
            )
            .inspect_err(|e| self.imds_err = Some(e.clone()))?;
            if st != 200 {
                return Err(format!("token: HTTP {st}"));
            }
            self.token = Some(body.trim().to_string());
        }
        let tok = self.token.clone().unwrap_or_default();
        let (st, body) = http(
            "GET",
            &format!("/latest/{path}"),
            &[("X-aws-ec2-metadata-token", &tok)],
        )?;
        match st {
            200 => Ok(body.trim_end().to_string()),
            404 => Err("HTTP 404".into()),
            s => Err(format!("HTTP {s}")),
        }
    }

    fn exec(&mut self, program: &str, args: &[&str]) -> Result<String, String> {
        let out = std::process::Command::new(program)
            .args(args)
            .stdin(std::process::Stdio::null())
            .output()
            .map_err(|e| format!("{program}: {e}"))?;
        if !out.status.success() {
            return Err(format!(
                "{program} exited {}: {}",
                out.status.code().unwrap_or(-1),
                String::from_utf8_lossy(&out.stderr).trim()
            ));
        }
        Ok(String::from_utf8_lossy(&out.stdout).into_owned())
    }

    fn ethtool(&mut self, ifname: &str) -> Result<Value, String> {
        ethtool_info(ifname)
    }

    fn bpf_connect_progs(&mut self, v6: bool) -> Result<Vec<String>, String> {
        bpf_names(v6)
    }

    fn nft_tables(&mut self) -> Result<Vec<String>, String> {
        nft_list()
    }

    fn if_addrs(&mut self) -> Result<Vec<(String, String)>, String> {
        if_addrs_list()
    }
}
