//! Per-account private subscription ownership. Subscription state never changes trading state.
use hft_types::ConnId;
#[derive(Clone, Copy, Debug)]
pub(crate) struct Candidate {
    pub id: ConnId,
    pub expected: u64,
    pub seen: u64,
    pub since: u64,
}
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct Private {
    pub owner: Option<ConnId>,
    pub candidate: Option<Candidate>,
    pub retry_at: u64,
    pub ever_ready: bool,
    pub retiring: Option<ConnId>,
}
impl Private {
    pub fn accepts(&self, id: ConnId) -> bool {
        self.owner == Some(id) || self.retiring == Some(id) || self.candidate.is_some_and(|c| c.id == id)
    }
    pub fn begin(&mut self, id: ConnId, expected: u64, now: u64) {
        self.candidate = Some(Candidate {
            id,
            expected,
            seen: 0,
            since: now,
        });
    }
    pub fn ack(&mut self, id: ConnId, bit: u8) -> bool {
        let Some(c) = &mut self.candidate else {
            return false;
        };
        if c.id != id || bit >= 64 || c.expected & (1u64 << bit) == 0 {
            return false;
        }
        c.seen |= 1u64 << bit;
        c.seen == c.expected
    }
    pub fn ready(&self, id: ConnId) -> bool {
        self.candidate.is_some_and(|c| c.id == id && c.expected == c.seen)
    }
}

/// Warm-thread preparation. Only public subscription text and session UID may be attached.
pub fn prepare<P: hft_venue_proto::VenueProtocol>(
    conn: &mut hft_net::Conn,
    cfg: &hft_venue_proto::VenueCfg,
    account: hft_types::AccountId,
    session: u8,
    uid: [u8; 24],
) -> Result<(), hft_venue_proto::ProtoErr> {
    use hft_venue_proto::{ProtoErr, SubFrames};
    let mut data = Vec::with_capacity(16_384);
    data.extend_from_slice(b"PRIV0001");
    data.push(session);
    data.extend_from_slice(&uid);
    let mut frames = SubFrames::with_capacity(8192, 8);
    for unsub in [false, true] {
        frames.clear();
        P::private_subscribe(cfg, account, unsub, &mut frames)?;
        data.push(frames.len() as u8);
        for k in 0..frames.len() {
            let (bytes, _) = frames.get(k).ok_or(ProtoErr::Cfg)?;
            data.extend_from_slice(&(bytes.len() as u16).to_le_bytes());
            data.extend_from_slice(bytes);
        }
    }
    if !conn.set_handoff_data(&data) {
        return Err(ProtoErr::BufFull);
    }
    Ok(())
}
pub(crate) fn copy_frames(data: &[u8], unsub: bool, out: &mut hft_venue_proto::SubFrames) -> bool {
    if data.get(..8) != Some(b"PRIV0001") {
        return false;
    }
    let mut pos = 33;
    for which in [false, true] {
        let Some(n) = data.get(pos).copied() else {
            return false;
        };
        pos += 1;
        for _ in 0..n {
            let Some(len) = data.get(pos..pos + 2) else {
                return false;
            };
            pos += 2;
            let len = u16::from_le_bytes([len[0], len[1]]) as usize;
            let Some(bytes) = data.get(pos..pos + len) else {
                return false;
            };
            pos += len;
            if which == unsub && out.push(bytes, 1).is_err() {
                return false;
            }
        }
    }
    true
}
