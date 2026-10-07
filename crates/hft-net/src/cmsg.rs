//! `SCM_TIMESTAMPING` control-message parsing (README §7). The layout is the Linux LP64 one
//! (`cmsghdr` = `{usize len, i32 level, i32 type}`, data aligned to 8, three `{i64 sec, i64 nsec}`
//! timespecs); it is parsed byte-wise so the same code runs (and is tested) on every host.

use crate::types::TsFlags;

/// `SOL_SOCKET` on Linux.
pub const SOL_SOCKET: i32 = 1;
/// `SCM_TIMESTAMPING` (= `SO_TIMESTAMPING_OLD`, the 64-bit-time layout on LP64).
pub const SCM_TIMESTAMPING: i32 = 37;
/// `MSG_CTRUNC` on Linux.
pub const MSG_CTRUNC: i32 = 0x8;
/// `sizeof(struct cmsghdr)` on LP64.
pub const CMSG_HDR: usize = 16;
/// Control buffer size passed to `recvmsg` (README §1: `cmsg_buf[128]`).
pub const CMSG_BUF: usize = 128;

/// Stamps of one read: `(sw_rx_ns, hw_rx_ns, flags)` with `flags` ⊆ `HW | SW`.
#[derive(Copy, Clone, Debug, Default, Eq, PartialEq)]
pub struct RxStamp {
    pub sw_rx_ns: i64,
    pub hw_rx_ns: i64,
    pub flags: u32,
}

#[inline]
fn rd_u64(b: &[u8], at: usize) -> Option<u64> {
    let s = b.get(at..at.checked_add(8)?)?;
    let mut v = [0u8; 8];
    v.copy_from_slice(s);
    Some(u64::from_ne_bytes(v))
}

#[inline]
fn rd_i32(b: &[u8], at: usize) -> Option<i32> {
    let s = b.get(at..at.checked_add(4)?)?;
    let mut v = [0u8; 4];
    v.copy_from_slice(s);
    Some(i32::from_ne_bytes(v))
}

#[inline]
fn ts_ns(b: &[u8], at: usize) -> i64 {
    match (rd_u64(b, at), rd_u64(b, at + 8)) {
        (Some(s), Some(n)) => {
            let (s, n) = (s as i64, n as i64);
            if s == 0 && n == 0 {
                0
            } else {
                s.saturating_mul(1_000_000_000).saturating_add(n)
            }
        }
        _ => 0,
    }
}

/// Parses the control buffer `buf` (its valid length) of one `recvmsg` with `msg_flags`.
/// Missing `SCM_TIMESTAMPING`, `MSG_CTRUNC`, or fewer than three timespecs → all zero; a zero
/// timespec leaves its stamp at 0 and its flag clear. Never panics, never reads out of bounds.
pub fn parse_timestamping(buf: &[u8], msg_flags: i32) -> RxStamp {
    let mut out = RxStamp::default();
    if msg_flags & MSG_CTRUNC != 0 {
        return out;
    }
    let mut off = 0usize;
    // Bounded: every step advances by at least CMSG_HDR bytes.
    while off + CMSG_HDR <= buf.len() {
        let len = match rd_u64(buf, off) {
            Some(l) => l as usize,
            None => break,
        };
        let level = rd_i32(buf, off + 8).unwrap_or(0);
        let typ = rd_i32(buf, off + 12).unwrap_or(0);
        if len < CMSG_HDR || len > buf.len() - off {
            break;
        }
        if level == SOL_SOCKET && typ == SCM_TIMESTAMPING {
            let data = off + CMSG_HDR;
            if len - CMSG_HDR >= 48 {
                let sw = ts_ns(buf, data);
                let hw = ts_ns(buf, data + 32);
                if sw != 0 {
                    out.sw_rx_ns = sw;
                    out.flags |= TsFlags::SW;
                }
                if hw != 0 {
                    out.hw_rx_ns = hw;
                    out.flags |= TsFlags::HW;
                }
            }
            return out;
        }
        // CMSG_NXTHDR: align the length up to 8.
        let step = (len + 7) & !7;
        match off.checked_add(step) {
            Some(n) => off = n,
            None => break,
        }
    }
    out
}

/// Builds one `SCM_TIMESTAMPING` message holding `ts[0..3]` (`(sec, nsec)` each); for scripted
/// I/O and tests. Returns the bytes written into `out` (0 when `out` is too small).
pub fn build_timestamping(out: &mut [u8], ts: [(i64, i64); 3]) -> usize {
    let len = CMSG_HDR + 48;
    let Some(dst) = out.get_mut(..len) else {
        return 0;
    };
    dst.fill(0);
    dst[0..8].copy_from_slice(&(len as u64).to_ne_bytes());
    dst[8..12].copy_from_slice(&SOL_SOCKET.to_ne_bytes());
    dst[12..16].copy_from_slice(&SCM_TIMESTAMPING.to_ne_bytes());
    for (i, (s, n)) in ts.iter().enumerate() {
        let at = CMSG_HDR + i * 16;
        dst[at..at + 8].copy_from_slice(&s.to_ne_bytes());
        dst[at + 8..at + 16].copy_from_slice(&n.to_ne_bytes());
    }
    len
}
