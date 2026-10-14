//! Heartbeats of the non-BBO connections (README §5): application pings per `HeartbeatSpec`
//! (`unconditional = false` only after an interval without inbound data), the 5 s WS Ping RTT probe
//! of depth and order connections, `pong_timeout_ms` (an unanswered ping) and `idle_timeout_ms`
//! (no inbound data frame) → `PING_TIMEOUT` / `IDLE_TIMEOUT`, handled as fatal. A 0 in a spec field
//! disables that part.

use hft_net::ErrBits;
use hft_venue_proto::{HeartbeatSpec, PingKind};

use crate::config::Ticks;

/// What to send now.
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub(crate) enum HbSend {
    None,
    App,
    WsPing,
}

#[derive(Copy, Clone, Debug)]
pub(crate) struct Hb {
    ping: PingKind,
    unconditional: bool,
    interval: u64,
    pong_to: u64,
    idle_to: u64,
    rtt: u64,
    next_ping: u64,
    next_rtt: u64,
    pong_wait: Option<u64>,
}

/// RTT probe period of depth and order connections.
const RTT_MS: u64 = 5000;

impl Hb {
    pub fn new(spec: &HeartbeatSpec, t: &Ticks, now: u64, rtt_probe: bool) -> Hb {
        let interval = t.ms(u64::from(spec.interval_ms));
        let rtt = if rtt_probe { t.ms(RTT_MS) } else { 0 };
        Hb {
            ping: spec.ping,
            unconditional: spec.unconditional,
            interval,
            pong_to: t.ms(u64::from(spec.pong_timeout_ms)),
            idle_to: t.ms(u64::from(spec.idle_timeout_ms)),
            rtt,
            next_ping: now.saturating_add(interval),
            next_rtt: now.saturating_add(rtt),
            pong_wait: None,
        }
    }

    /// A pong (WS or application) arrived.
    pub fn on_pong(&mut self) {
        self.pong_wait = None;
    }

    /// What is due at `now`; `last_data` = rx_tsc of the last inbound data frame.
    pub fn due(&mut self, now: u64, last_data: u64) -> HbSend {
        if self.interval > 0 && now >= self.next_ping {
            self.next_ping = now.max(self.next_ping).saturating_add(self.interval);
            let quiet = now.saturating_sub(last_data) >= self.interval;
            if !self.unconditional && !quiet {
                self.next_ping = last_data.saturating_add(self.interval);
            }
            if self.unconditional || quiet {
                if self.pong_wait.is_none() && self.pong_to > 0 {
                    self.pong_wait = Some(now);
                }
                return match self.ping {
                    PingKind::WsPing => HbSend::WsPing,
                    PingKind::AppText | PingKind::AppJson => HbSend::App,
                };
            }
        }
        if self.rtt > 0 && now >= self.next_rtt {
            self.next_rtt = now.saturating_add(self.rtt);
            return HbSend::WsPing;
        }
        HbSend::None
    }

    /// Timeout bits at `now` (empty = healthy).
    pub fn check(&self, now: u64, last_data: u64) -> ErrBits {
        let mut b = ErrBits::empty();
        if let Some(t) = self.pong_wait
            && now.saturating_sub(t) > self.pong_to
        {
            b |= ErrBits::PING_TIMEOUT;
        }
        if self.idle_to > 0 && now.saturating_sub(last_data) > self.idle_to {
            b |= ErrBits::IDLE_TIMEOUT;
        }
        b
    }

    /// Earliest instant something above changes.
    pub fn next_deadline(&self, last_data: u64) -> Option<u64> {
        let mut d: Option<u64> = None;
        let mut put = |x: u64| d = Some(d.map_or(x, |y: u64| y.min(x)));
        if self.interval > 0 {
            put(self.next_ping);
        }
        if self.rtt > 0 {
            put(self.next_rtt);
        }
        if let Some(t) = self.pong_wait {
            put(t.saturating_add(self.pong_to).saturating_add(1));
        }
        if self.idle_to > 0 {
            put(last_data.saturating_add(self.idle_to).saturating_add(1));
        }
        d
    }
}
