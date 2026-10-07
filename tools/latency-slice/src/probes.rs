//! host probe modes for hft-net NET-371–NET-374 (README "host 探测模式").

use std::time::{Duration, Instant};

use hft_event::ConnRole;
use hft_net::{BuildEvent, BuildReq, ConnId, ConnPool, Connector, ErrBits, JobId, PoolMode, RotatePolicy};
use hft_venue::MD_FIRST;
use hft_venue_proto::VenueProtocol;
use venue_binance::{Spot, Usdm};

use crate::config::{Config, Product};
use crate::hk::calibrate;
use crate::live::{self, EP_BBO};
use crate::threads::utc_ns;

fn connector<P: VenueProtocol>(cfg: &Config, n: u16) -> Result<(Connector, hft_rt::TscClock), String> {
    let net = live::net_config::<P>(cfg)?;
    let clock = calibrate()?;
    let arena = live::arena(&net, clock, n + 2);
    Ok((
        Connector::new(arena, &net).map_err(|e| format!("connector: {e:?}"))?,
        clock,
    ))
}

/// Builds `n` market-data connections; returns `(conn, job)` pairs and the per-build outcome
/// (`Ok(napi_id)` or the failure text), handing every ready connection to `on_ready`.
fn build<P: VenueProtocol>(
    c: &mut Connector,
    n: u16,
    mut on_ready: impl FnMut(&mut Connector, JobId, ConnId, u32),
) -> Result<(u16, u16), String> {
    let mut jobs: Vec<(JobId, ConnId)> = Vec::new();
    for k in 0..n {
        let id = ConnId(MD_FIRST.saturating_add(u8::try_from(k).map_err(|_| "too many connections")?));
        let req = BuildReq {
            venue: P::VENUE,
            conn: id,
            role: ConnRole::Md,
            endpoint: EP_BBO,
            src_ip_idx: 0,
            rx_queue: 0,
            deadline_ns: utc_ns() + 20_000_000_000,
        };
        let j = c.submit(req).map_err(|e| format!("submit: {e:?}"))?;
        jobs.push((j, id));
    }
    let (mut ok, mut failed) = (0u16, 0u16);
    let t0 = Instant::now();
    while usize::from(ok + failed) < jobs.len() && t0.elapsed() < Duration::from_secs(30) {
        match c.drive(utc_ns()) {
            Some(BuildEvent::Ready { job, napi_id, .. }) => {
                if let Some(&(_, id)) = jobs.iter().find(|(j, _)| *j == job) {
                    ok += 1;
                    on_ready(c, job, id, napi_id);
                }
            }
            Some(BuildEvent::Failed { job, stage, why }) => {
                if let Some(&(_, id)) = jobs.iter().find(|(j, _)| *j == job) {
                    failed += 1;
                    println!("conn={} failed stage={stage:?} why={why:?}", id.0);
                }
            }
            None => std::thread::sleep(Duration::from_micros(200)),
        }
    }
    Ok((ok, failed))
}

/// Hands a ready connection over and gives it back to the connector.
fn release(c: &mut Connector, job: JobId) {
    if let Ok(h) = c.handoff(job) {
        let id = h.id();
        let mut p = ConnPool::<2>::new(PoolMode::Race, 1, RotatePolicy::default());
        if p.install(h, None).is_ok()
            && let Ok(m) = p.retire(id, ErrBits::empty())
        {
            c.reclaim(m);
        }
    }
}

/// `--build-only <n> [--print-napi]` (NET-371).
pub fn build_only(cfg: &Config, n: u16, print_napi: bool) -> i32 {
    let r = match cfg.product {
        Product::Usdm => build_only_p::<Usdm>(cfg, n, print_napi),
        Product::Spot => build_only_p::<Spot>(cfg, n, print_napi),
    };
    match r {
        Ok(code) => code,
        Err(e) => {
            eprintln!("latency-slice: {e}");
            3
        }
    }
}

fn build_only_p<P: VenueProtocol>(cfg: &Config, n: u16, print_napi: bool) -> Result<i32, String> {
    let (mut c, _) = connector::<P>(cfg, n)?;
    let mut ready = Vec::new();
    let (ok, failed) = build::<P>(&mut c, n, |c, job, id, napi| {
        let ifindex = c.napi_ifindex(napi);
        if print_napi {
            println!(
                "conn={} napi_id={napi} ifindex={}",
                id.0,
                ifindex.map_or_else(|| "none".to_string(), |i| i.to_string())
            );
        }
        ready.push(job);
    })?;
    for j in ready {
        release(&mut c, j);
    }
    println!("built={ok} failed={failed}");
    Ok(if failed == 0 && ok == n { 0 } else { 1 })
}

/// `--idle-polls <n>` (NET-372).
pub fn idle_polls(cfg: &Config, n: u64) -> i32 {
    let r = match cfg.product {
        Product::Usdm => idle_polls_p::<Usdm>(cfg, n),
        Product::Spot => idle_polls_p::<Spot>(cfg, n),
    };
    match r {
        Ok(code) => code,
        Err(e) => {
            eprintln!("latency-slice: {e}");
            3
        }
    }
}

fn idle_polls_p<P: VenueProtocol>(cfg: &Config, n: u64) -> Result<i32, String> {
    let (mut c, clock) = connector::<P>(cfg, 1)?;
    let mut job = None;
    let (ok, _) = build::<P>(&mut c, 1, |_, j, _, _| job = Some(j))?;
    let Some(job) = job.filter(|_| ok == 1) else {
        return Err("connection not built".into());
    };
    let h = c.handoff(job).map_err(|e| format!("handoff: {e:?}"))?;
    let id = h.id();
    let mut pool = ConnPool::<2>::new(PoolMode::Race, 1, RotatePolicy::default());
    pool.install(h, None).map_err(|e| format!("install: {e:?}"))?;
    let conn = pool.get(id).ok_or("no connection")?;
    let sys0 = conn.sys().recvmsg;
    let mut nonempty = 0u64;
    let t0 = hft_rt::tsc_now();
    for _ in 0..n {
        let now = hft_rt::tsc_now().0;
        if conn.poll_rx(now) > 0 {
            nonempty += 1;
        }
        while conn.next_frame().is_some() {}
        conn.finish_poll(now);
    }
    let t1 = hft_rt::tsc_now();
    let recvmsg = conn.sys().recvmsg - sys0;
    let ns = clock.delta_ns(t0, t1);
    println!(
        "polls={n} nonempty={nonempty} ns_per_poll={:.1} recvmsg={recvmsg}",
        ns as f64 / n.max(1) as f64
    );
    if let Ok(m) = pool.retire(id, ErrBits::empty()) {
        c.reclaim(m);
    }
    Ok(0)
}

/// `--two-packet-stamp-probe <n>` (NET-373).
pub fn two_packet(n: u32) -> i32 {
    #[cfg(target_os = "linux")]
    {
        match loopback::two_packet(n) {
            Ok(()) => 0,
            Err(e) => {
                eprintln!("latency-slice: {e}");
                3
            }
        }
    }
    #[cfg(not(target_os = "linux"))]
    {
        let _ = n;
        eprintln!("latency-slice: --two-packet-stamp-probe needs Linux");
        2
    }
}

/// `--loopback-stream-minutes <n>` (NET-374).
pub fn loopback_stream(minutes: u32) -> i32 {
    #[cfg(target_os = "linux")]
    {
        match loopback::stream(minutes) {
            Ok(()) => 0,
            Err(e) => {
                eprintln!("latency-slice: {e}");
                3
            }
        }
    }
    #[cfg(not(target_os = "linux"))]
    {
        let _ = minutes;
        eprintln!("latency-slice: --loopback-stream-minutes needs Linux");
        2
    }
}

#[cfg(target_os = "linux")]
mod loopback {
    use std::io::Write;
    use std::net::{TcpListener, TcpStream};
    use std::os::fd::AsRawFd;
    use std::time::{Duration, Instant};

    use hft_net::cmsg::{CMSG_BUF, parse_timestamping};

    use crate::threads::utc_ns;

    fn pair() -> Result<(TcpStream, TcpStream), String> {
        let l = TcpListener::bind("127.0.0.1:0").map_err(|e| e.to_string())?;
        let a = l.local_addr().map_err(|e| e.to_string())?;
        let rx = TcpStream::connect(a).map_err(|e| e.to_string())?;
        let (tx, _) = l.accept().map_err(|e| e.to_string())?;
        tx.set_nodelay(true).map_err(|e| e.to_string())?;
        rx.set_nonblocking(true).map_err(|e| e.to_string())?;
        Ok((tx, rx))
    }

    fn enable_rx_sw_stamps(fd: i32) -> Result<(), String> {
        let v: libc::c_int = libc::SOF_TIMESTAMPING_RX_SOFTWARE as libc::c_int
            | libc::SOF_TIMESTAMPING_SOFTWARE as libc::c_int;
        // SAFETY: valid fd and a pointer to an owned int of the given size.
        let r = unsafe {
            libc::setsockopt(
                fd,
                libc::SOL_SOCKET,
                libc::SO_TIMESTAMPING,
                (&v as *const libc::c_int).cast(),
                std::mem::size_of::<libc::c_int>() as libc::socklen_t,
            )
        };
        if r != 0 {
            return Err(format!("SO_TIMESTAMPING: {}", std::io::Error::last_os_error()));
        }
        Ok(())
    }

    /// One non-blocking `recvmsg` into `buf`; `(bytes, sw_rx_ns)`.
    fn recv(fd: i32, buf: &mut [u8]) -> (isize, i64) {
        let mut ctl = [0u8; CMSG_BUF];
        let mut iov = libc::iovec {
            iov_base: buf.as_mut_ptr().cast(),
            iov_len: buf.len(),
        };
        // SAFETY: zeroed msghdr is valid; the pointers below refer to live owned buffers.
        let mut m: libc::msghdr = unsafe { std::mem::zeroed() };
        m.msg_iov = &mut iov;
        m.msg_iovlen = 1;
        m.msg_control = ctl.as_mut_ptr().cast();
        m.msg_controllen = ctl.len() as _;
        // SAFETY: `m` describes owned buffers that outlive the call.
        let r = unsafe { libc::recvmsg(fd, &mut m, libc::MSG_DONTWAIT) };
        if r <= 0 {
            return (r, 0);
        }
        let n = (m.msg_controllen as usize).min(ctl.len());
        let st = parse_timestamping(&ctl[..n], m.msg_flags);
        (r, st.sw_rx_ns)
    }

    pub fn two_packet(n: u32) -> Result<(), String> {
        let netem_ns: i64 = std::env::var("HFT_SLICE_NETEM_US")
            .ok()
            .and_then(|v| v.parse::<i64>().ok())
            .unwrap_or(1_000)
            * 1_000;
        let (mut tx, rx) = pair()?;
        let fd = rx.as_raw_fd();
        enable_rx_sw_stamps(fd)?;
        let pkt = [0x5Au8; 200];
        let mut buf = vec![0u8; 4096];
        let mut within = 0u32;
        for i in 0..n {
            let t1 = utc_ns();
            tx.write_all(&pkt).map_err(|e| e.to_string())?;
            std::thread::sleep(Duration::from_millis(1));
            let t2 = utc_ns();
            tx.write_all(&pkt).map_err(|e| e.to_string())?;
            std::thread::sleep(Duration::from_millis(3));
            let (bytes, sw) = recv(fd, &mut buf);
            // Drain anything left.
            while recv(fd, &mut buf).0 > 0 {}
            if (sw - (t2 + netem_ns)).abs() <= 50_000 {
                within += 1;
            }
            println!("i={i} sw_rx_ns={sw} t1_ns={t1} t2_ns={t2} bytes={bytes}");
        }
        println!(
            "within_50us_of_second={:.2}",
            f64::from(within) * 100.0 / f64::from(n.max(1))
        );
        Ok(())
    }

    pub fn stream(minutes: u32) -> Result<(), String> {
        let (mut tx, rx) = pair()?;
        let fd = rx.as_raw_fd();
        let dur = Duration::from_secs(u64::from(minutes) * 60).max(Duration::from_millis(200));
        let stop = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let s2 = std::sync::Arc::clone(&stop);
        let w = std::thread::spawn(move || {
            let chunk = vec![0xA5u8; 65_536];
            while !s2.load(std::sync::atomic::Ordering::Relaxed) {
                if tx.write_all(&chunk).is_err() {
                    break;
                }
            }
        });
        let mut buf = vec![0u8; 65_536];
        let (mut reads, mut short, mut bytes) = (0u64, 0u64, 0u64);
        let t0 = Instant::now();
        while t0.elapsed() < dur {
            let (r, _) = recv(fd, &mut buf);
            if r > 0 {
                reads += 1;
                bytes += r as u64;
                if (r as usize) < buf.len() {
                    short += 1;
                }
            }
        }
        stop.store(true, std::sync::atomic::Ordering::Relaxed);
        drop(rx);
        let _ = w.join();
        println!("reads={reads} short_reads={short} bytes={bytes}");
        Ok(())
    }
}
