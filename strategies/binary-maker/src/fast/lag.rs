//! Venue-load gates (README "规则" 11): the feed lag of the slot insts' market-data messages
//! (kernel receive stamp − venue time), its 90th percentile and median per receive second, and the
//! medians of those per minute. Integer, exact: quantiles are kept ×10 (per second) and ×100 (per
//! minute) in nanoseconds.

/// Lag samples kept per second (more are not counted, `overflow`).
pub const PER_SEC: usize = 1024;
const NS_S: i64 = 1_000_000_000;

/// Linear-interpolation quantile `q10 / 10` of the sorted `v` (pandas' default), ×10: with
/// h = q10·(n − 1), lo = ⌊h / 10⌋, fr = h mod 10: 10·v[lo] + fr·(v[lo + 1] − v[lo]).
pub fn quantile_x10(v: &[i64], q10: i64) -> Option<i64> {
    let n = i64::try_from(v.len()).ok().filter(|n| *n > 0)?;
    let h = q10 * (n - 1);
    let lo = usize::try_from(h / 10).ok()?;
    let fr = h % 10;
    let a = *v.get(lo)?;
    let b = if fr > 0 { *v.get(lo + 1)? } else { a };
    Some(
        a.saturating_mul(10)
            .saturating_add(fr.saturating_mul(b.saturating_sub(a))),
    )
}

#[derive(Debug)]
pub struct LagGate {
    /// Receive second being collected and its samples (ns).
    sec: i64,
    buf: Box<[i64]>,
    n: usize,
    /// Minute of the per-second values in `p90`/`p50` (×10 ns).
    min: i64,
    p90: [i64; 60],
    p50: [i64; 60],
    nm: usize,
    /// Last complete minute and its medians of the per-second p90 and median (×100 ns).
    pub done_min: i64,
    pub done_p90: i64,
    pub done_p50: i64,
    /// Samples of a second already closed (dropped).
    pub late: u64,
    /// Samples beyond `PER_SEC` in one second (dropped).
    pub overflow: u64,
}

impl Default for LagGate {
    fn default() -> LagGate {
        LagGate::new()
    }
}

impl LagGate {
    /// Allocates the per-second buffer (command path).
    pub fn new() -> LagGate {
        LagGate {
            sec: i64::MIN,
            buf: vec![0; PER_SEC].into_boxed_slice(),
            n: 0,
            min: i64::MIN,
            p90: [0; 60],
            p50: [0; 60],
            nm: 0,
            done_min: i64::MIN,
            done_p90: 0,
            done_p50: 0,
            late: 0,
            overflow: 0,
        }
    }

    /// One message received at `rx_ns` (UTC) with lag `lag_ns`.
    pub fn push(&mut self, rx_ns: i64, lag_ns: i64) {
        let s = rx_ns.div_euclid(NS_S);
        if s < self.sec {
            self.late = self.late.wrapping_add(1);
            return;
        }
        if s > self.sec {
            self.close_second();
            self.sec = s;
        }
        match self.buf.get_mut(self.n) {
            Some(x) => {
                *x = lag_ns;
                self.n += 1;
            }
            None => self.overflow = self.overflow.wrapping_add(1),
        }
    }

    /// Strategy time `utc_ns`: closes the collected second once UTC is past it and the minute once
    /// UTC is past that.
    pub fn tick(&mut self, utc_ns: i64) {
        let s = utc_ns.div_euclid(NS_S);
        if s > self.sec {
            self.close_second();
            self.sec = s;
        }
        if s.div_euclid(60) > self.min && self.nm > 0 {
            self.close_minute();
        }
    }

    fn close_second(&mut self) {
        if self.n == 0 {
            return;
        }
        let n = self.n;
        self.n = 0;
        let Some(v) = self.buf.get_mut(..n) else { return };
        v.sort_unstable();
        let (Some(p90), Some(p50)) = (quantile_x10(v, 9), quantile_x10(v, 5)) else {
            return;
        };
        let m = self.sec.div_euclid(60);
        if m != self.min {
            self.close_minute();
            self.min = m;
        }
        if let (Some(a), Some(b)) = (self.p90.get_mut(self.nm), self.p50.get_mut(self.nm)) {
            *a = p90;
            *b = p50;
            self.nm += 1;
        }
    }

    fn close_minute(&mut self) {
        let nm = self.nm;
        self.nm = 0;
        if nm == 0 {
            return;
        }
        let (Some(a), Some(b)) = (self.p90.get_mut(..nm), self.p50.get_mut(..nm)) else {
            return;
        };
        a.sort_unstable();
        b.sort_unstable();
        if let (Some(x), Some(y)) = (quantile_x10(a, 5), quantile_x10(b, 5)) {
            self.done_min = self.min;
            self.done_p90 = x;
            self.done_p50 = y;
        }
    }

    /// (load, slow) at strategy time `utc_ns`: the previous UTC minute's median of the per-second
    /// p90 ≥ `load_ns`, its median of the per-second median > `slow_ns` (a threshold of 0 never
    /// gates); neither without that minute.
    pub fn gates(&self, utc_ns: i64, load_ns: u32, slow_ns: u32) -> (bool, bool) {
        if self.done_min != utc_ns.div_euclid(60 * NS_S) - 1 {
            return (false, false);
        }
        let load = load_ns > 0 && self.done_p90 >= i64::from(load_ns) * 100;
        let slow = slow_ns > 0 && self.done_p50 > i64::from(slow_ns) * 100;
        (load, slow)
    }
}
