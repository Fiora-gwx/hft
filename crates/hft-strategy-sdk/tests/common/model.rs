//! `ProbeModel` (TargetProbe's Model; TESTS.md "共用夹具") and its variants, the Advice payload
//! `BandAdvice` and the warmup payload encoder.
//!
//! Raw-sample section: instance section tag 1 `{n_ticks: U64 One}`; per group tag 2
//! `{GroupId u32, utc_ns: I64 Prefixed, mid: I64 Prefixed, pct_bin: I64 One, fp: U64 One}` (at most
//! 256 one-second samples; `pct_bin` is the storage-interpretation config, `fp` the group
//! fingerprint). Path-state section: `n: u32`, then per group `{id: u32, _pad: u32, band: i64,
//! band_upd_ns: i64, last_sent: u64}`; `band` is recomputed from the samples on the first model
//! tick of each UTC minute (and while it was never ready).

use hft_strategy_sdk::model_math;
use hft_strategy_sdk::*;
use hft_types::{GroupId, InstId};

use super::params::TP2Params;

/// Sample window per group.
pub const WIN: usize = 256;
/// `BandAdvice.flags`: the band is not ready (no samples).
pub const NOT_READY: u8 = 1;
/// Band period (ms) of the path state.
pub const BAND_PERIOD_MS: i64 = 60_000;

/// Variant selectors of `ProbeModelT`.
pub const K_PROBE: u8 = 0;
pub const K_COLD: u8 = 1;
pub const K_MEDIAN: u8 = 2;
pub const K_UNDER: u8 = 3;
pub const K_COLD_HUGE: u8 = 4;
pub const K_LAYOUT_EXTRA_U64: u8 = 10;
pub const K_LAYOUT_MID_I32: u8 = 11;
pub const K_LAYOUT_NO_GID: u8 = 12;
pub const K_LAYOUT_TAG3: u8 = 13;
pub const K_LAYOUT_BAD_COUNT: u8 = 14;

pub type ProbeModel = ProbeModelT<K_PROBE>;
pub type ColdModel = ProbeModelT<K_COLD>;
pub type ProbeModelB = ProbeModelT<K_MEDIAN>;
pub type UnderModel = ProbeModelT<K_UNDER>;

/// `ProbeModel::LAYOUT`.
pub static PROBE_LAYOUT: ModelLayoutDesc = ModelLayoutDesc {
    sections: &[
        SectionDesc {
            tag: 1,
            name: "ticks",
            scope: SectionScope::Instance,
            elems: &[ElemDesc {
                name: "n_ticks",
                ty: ElemTy::U64,
                len: LenRule::One,
            }],
        },
        SectionDesc {
            tag: 2,
            name: "samples",
            scope: SectionScope::Group,
            elems: &[
                ElemDesc {
                    name: "utc_ns",
                    ty: ElemTy::I64,
                    len: LenRule::Prefixed,
                },
                ElemDesc {
                    name: "mid",
                    ty: ElemTy::I64,
                    len: LenRule::Prefixed,
                },
                ElemDesc {
                    name: "pct_bin",
                    ty: ElemTy::I64,
                    len: LenRule::One,
                },
                ElemDesc {
                    name: "fp",
                    ty: ElemTy::U64,
                    len: LenRule::One,
                },
            ],
        },
    ],
};

/// Advice `local_kind = 1`: `{ver: u8 = 1, flags: u8, _pad: [u8; 6], band: i64}` (16 B).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BandAdvice {
    pub flags: u8,
    pub band: i64,
}

impl AdvicePayload for BandAdvice {
    const LOCAL_KIND: u16 = 1;
    const MAX_LEN: usize = 16;
    fn encode(&self, w: &mut PayloadWriter<'_>) -> Result<(), PayloadFull> {
        w.put_u8(1)?;
        w.put_u8(self.flags)?;
        w.put_bytes(&[0; 6])?;
        w.put_i64(self.band)
    }
    fn decode(b: &[u8]) -> Result<Self, PayloadError> {
        let mut r = PayloadReader::new(b);
        let ver = r.get_u8()?;
        if ver != 1 {
            return Err(PayloadError::Version(ver));
        }
        let flags = r.get_u8()?;
        r.get_bytes(6)?;
        let band = r.get_i64()?;
        r.finish()?;
        Ok(BandAdvice { flags, band })
    }
}

/// Warmup `local_kind = 1` chunk: `{ver 1, _pad[3], group u32, n u16, _pad2[6], (utc_ns, mid) × n}`.
pub fn warm_chunk(group: GroupId, samples: &[(i64, i64)]) -> Vec<u8> {
    let mut b = vec![1u8, 0, 0, 0];
    b.extend_from_slice(&group.0.to_le_bytes());
    b.extend_from_slice(&(samples.len() as u16).to_le_bytes());
    b.extend_from_slice(&[0; 6]);
    for (t, m) in samples {
        b.extend_from_slice(&t.to_le_bytes());
        b.extend_from_slice(&m.to_le_bytes());
    }
    b
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MGroup {
    pub id: GroupId,
    pub fp: u64,
    pub leg0: InstId,
    pub pct_bin: i64,
    /// Restored from a checkpoint and not yet reconciled by `on_params`.
    pub restored: bool,
    /// `(utc second, mid)` ascending, ≤ `WIN`.
    pub s: Vec<(i64, i64)>,
    pub band: i64,
    pub band_upd_ns: i64,
    pub last_sent: u64,
}

impl MGroup {
    fn fresh(id: GroupId, fp: u64, leg0: InstId, pct_bin: i64) -> MGroup {
        MGroup {
            id,
            fp,
            leg0,
            pct_bin,
            restored: false,
            s: Vec::with_capacity(WIN + 1),
            band: 0,
            band_upd_ns: 0,
            last_sent: 0,
        }
    }
    /// Live samples overwrite; warmup samples only fill empty seconds. Keeps the newest `WIN`.
    fn insert(&mut self, t: i64, mid: i64, live: bool) {
        match self.s.binary_search_by_key(&t, |x| x.0) {
            Ok(i) => {
                if live && let Some(x) = self.s.get_mut(i) {
                    x.1 = mid;
                }
            }
            Err(i) => {
                if self.s.len() >= WIN && i == 0 {
                    return;
                }
                self.s.insert(i, (t, mid));
                if self.s.len() > WIN {
                    self.s.remove(0);
                }
            }
        }
    }
}

/// One group of a raw-sample section being restored: id, samples, `pct_bin`, fingerprint.
type SavedGroup = (GroupId, Vec<(i64, i64)>, i64, u64);

/// `ProbeModel` with variant `K` (see the `K_*` constants).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProbeModelT<const K: u8> {
    pub n_ticks: u64,
    pub groups: Vec<MGroup>,
}

impl<const K: u8> ProbeModelT<K> {
    /// The statistic of the samples: mean (median for `K_MEDIAN`), HalfEven; `None` when empty.
    pub fn stat(s: &[(i64, i64)]) -> Option<i64> {
        if K == K_MEDIAN {
            if s.is_empty() {
                return None;
            }
            let mut v: Vec<i64> = s.iter().map(|x| x.1).collect();
            v.sort_unstable();
            let n = v.len();
            if n % 2 == 1 {
                return Some(v[n / 2]);
            }
            return model_math::round_even((v[n / 2 - 1] as f64 + v[n / 2] as f64) / 2.0);
        }
        let sum: i128 = s.iter().map(|x| i128::from(x.1)).sum();
        model_math::round_even(sum as f64 / s.len() as f64)
    }

    fn tick(&mut self, t: i64, out: &mut AdviceOut<'_>) {
        self.n_ticks += 1;
        let minute = t.div_euclid(BAND_PERIOD_MS * 1_000_000);
        for g in &mut self.groups {
            let due = g.band_upd_ns == 0 || g.band_upd_ns.div_euclid(BAND_PERIOD_MS * 1_000_000) != minute;
            if due && let Some(b) = Self::stat(&g.s) {
                g.band = b;
                g.band_upd_ns = t;
            }
            let a = BandAdvice {
                flags: if g.band_upd_ns == 0 { NOT_READY } else { 0 },
                band: g.band,
            };
            if out.emit(g.id, &a).is_ok() {
                g.last_sent = (g.band as u64) ^ (u64::from(a.flags) << 56);
            }
        }
    }

    fn rec_len(g: &MGroup) -> usize {
        4 + 4 + 8 * g.s.len() + 4 + 8 * g.s.len() + 8 + 8
    }

    fn write_group(g: &MGroup, w: &mut SnapshotWriter) {
        w.section(2);
        if K != K_LAYOUT_NO_GID {
            w.put_u32(g.id.0);
        }
        w.put_u32(g.s.len() as u32);
        for (t, _) in &g.s {
            w.put_i64(*t);
        }
        let n = if K == K_LAYOUT_BAD_COUNT {
            g.s.len() + 1
        } else {
            g.s.len()
        };
        w.put_u32(n as u32);
        for (_, m) in &g.s {
            if K == K_LAYOUT_MID_I32 {
                w.put_i32(*m as i32);
            } else {
                w.put_i64(*m);
            }
        }
        w.put_i64(g.pct_bin);
        w.put_u64(g.fp);
        if K == K_LAYOUT_EXTRA_U64 {
            w.put_u64(0);
        }
        if K == K_LAYOUT_TAG3 {
            w.section(3);
            w.put_u8(0);
        }
    }
}

impl<const K: u8> Model for ProbeModelT<K> {
    type Params = TP2Params;
    const LAYOUT_VERSION: u32 = 1;
    const LAYOUT: &'static ModelLayoutDesc = &PROBE_LAYOUT;
    const PATH_VERSION: u32 = if K == K_MEDIAN { 2 } else { 1 };
    const WARMUP: bool = !matches!(K, K_COLD | K_COLD_HUGE);

    fn mem_bytes(_: &TP2Params, n: u16) -> u64 {
        if K == K_UNDER {
            1
        } else {
            u64::from(n) * 8_192 + 65_536
        }
    }
    fn snapshot_bytes(_: &TP2Params, n: u16) -> u64 {
        if K == K_UNDER {
            1
        } else {
            256 + u64::from(n) * 4_352
        }
    }
    fn path_settle_ms(_: &TP2Params) -> u64 {
        60_000
    }
    fn cold_start_ms(_: &TP2Params, gix: u16) -> u64 {
        if K == K_COLD_HUGE {
            5_000_000_000
        } else {
            100_000 * (u64::from(gix) + 1)
        }
    }
    fn new(_: StrategyInstance, p: &TP2Params, groups: &GroupTable, _: &RefView<'_>) -> Self {
        let mut v = Vec::with_capacity(usize::from(groups.capacity()));
        for (gix, g) in groups.iter() {
            v.push(MGroup::fresh(
                g.id,
                g.fingerprint(),
                g.legs[0].inst,
                p.at(gix).pct_bin.0,
            ));
        }
        ProbeModelT {
            n_ticks: 0,
            groups: v,
        }
    }
    fn on_params(&mut self, p: &TP2Params, groups: &GroupTable, _: &RefView<'_>) -> ModelParamEffect {
        let mut resets = GroupResets::new(groups.capacity());
        let mut n_reset = 0usize;
        let mut old: Vec<Option<MGroup>> = self.groups.drain(..).map(Some).collect();
        let mut next = Vec::with_capacity(usize::from(groups.capacity()));
        for (gix, g) in groups.iter() {
            let fp = g.fingerprint();
            let pb = p.at(gix).pct_bin.0;
            let prev = old
                .iter_mut()
                .find(|o| o.as_ref().is_some_and(|o| o.id == g.id))
                .and_then(Option::take);
            let cause = match &prev {
                None => Some(ResetCause::Fingerprint),
                Some(o) if o.fp != fp => Some(ResetCause::Fingerprint),
                Some(o) if o.pct_bin != pb => Some(if o.restored {
                    ResetCause::PctConfig
                } else {
                    ResetCause::Structural
                }),
                Some(_) => None,
            };
            match (cause, prev) {
                (None, Some(mut o)) => {
                    o.restored = false;
                    o.leg0 = g.legs[0].inst;
                    next.push(o);
                }
                (c, _) => {
                    if let Some(c) = c {
                        resets.set(gix, c);
                    }
                    n_reset += 1;
                    next.push(MGroup::fresh(g.id, fp, g.legs[0].inst, pb));
                }
            }
        }
        self.groups = next;
        if n_reset == 0 {
            ModelParamEffect::None
        } else if n_reset == usize::from(groups.len()) {
            ModelParamEffect::ResetAll
        } else {
            ModelParamEffect::ResetGroups(resets)
        }
    }
    fn on_event<V: ModelViews>(&mut self, ev: &EventRef<'_>, v: &V, out: &mut AdviceOut<'_>) {
        match *ev {
            EventRef::Bbo { gix, b, .. } => {
                let now = v.now();
                let t = now.utc_ns.div_euclid(1_000_000_000) * 1_000_000_000;
                let mid = ((i128::from(b.bid_px.0) + i128::from(b.ask_px.0)) / 2) as i64;
                if let Some(g) = self.groups.get_mut(usize::from(gix)) {
                    g.insert(t, mid, true);
                }
            }
            EventRef::Timer { tag: MODEL_TICK, .. } => self.tick(v.now().utc_ns, out),
            EventRef::Warmup {
                local_kind: 1,
                payload,
                ..
            } => {
                let mut r = PayloadReader::new(payload);
                let Ok(1) = r.get_u8() else { return };
                if r.get_bytes(3).is_err() {
                    return;
                }
                let (Ok(gid), Ok(n)) = (r.get_u32(), r.get_u16()) else {
                    return;
                };
                if r.get_bytes(6).is_err() {
                    return;
                }
                let mut samples = Vec::with_capacity(usize::from(n));
                for _ in 0..n {
                    let (Ok(t), Ok(m)) = (r.get_i64(), r.get_i64()) else {
                        return;
                    };
                    samples.push((t, m));
                }
                if r.finish().is_err() {
                    return;
                }
                if let Some(g) = self.groups.iter_mut().find(|g| g.id.0 == gid) {
                    for (t, m) in samples {
                        g.insert(t, m, false);
                    }
                }
            }
            _ => {}
        }
    }
    fn snapshot_path(&self, w: &mut SnapshotWriter) {
        w.put_u32(self.groups.len() as u32);
        for g in &self.groups {
            w.put_u32(g.id.0);
            w.put_u32(0);
            w.put_i64(g.band);
            w.put_i64(g.band_upd_ns);
            w.put_u64(g.last_sent);
        }
    }
    fn restore_path(&mut self, r: &mut SnapshotReader<'_>) -> Result<(), RestoreError> {
        let n = r.get_u32()?;
        let mut rows = Vec::new();
        for _ in 0..n {
            let id = GroupId(r.get_u32()?);
            r.get_u32()?;
            rows.push((id, r.get_i64()?, r.get_i64()?, r.get_u64()?));
        }
        if !r.is_empty() {
            return Err(RestoreError::BadValue);
        }
        for (id, band, upd, last) in rows {
            if let Some(g) = self.groups.iter_mut().find(|g| g.id == id) {
                g.band = band;
                g.band_upd_ns = upd;
                g.last_sent = last;
            }
        }
        Ok(())
    }
    fn snapshot_chunk(&self, from_gix: u16, w: &mut SnapshotWriter, max_bytes: u32) -> SnapshotProgress {
        let mut used = 0usize;
        if from_gix == 0 {
            w.section(1);
            w.put_u64(self.n_ticks);
            used = 8;
        }
        let mut wrote = 0usize;
        let mut gix = usize::from(from_gix);
        while let Some(g) = self.groups.get(gix) {
            let r = Self::rec_len(g);
            if wrote > 0 && used + r > max_bytes as usize {
                return SnapshotProgress::More(gix as u16);
            }
            Self::write_group(g, w);
            used += r;
            wrote += 1;
            gix += 1;
        }
        SnapshotProgress::Done
    }
    fn restore(&mut self, r: &mut SnapshotReader<'_>) -> Result<(), RestoreError> {
        let n_ticks = r.get_u64()?;
        let mut saved: Vec<SavedGroup> = Vec::new();
        while !r.is_empty() {
            let id = GroupId(r.get_u32()?);
            let n = r.get_u32()? as usize;
            if n > WIN {
                return Err(RestoreError::BadValue);
            }
            let mut ts = Vec::with_capacity(n);
            for _ in 0..n {
                ts.push(r.get_i64()?);
            }
            if r.get_u32()? as usize != n {
                return Err(RestoreError::BadValue);
            }
            let mut s = Vec::with_capacity(WIN + 1);
            for t in ts {
                s.push((t, r.get_i64()?));
            }
            let pct_bin = r.get_i64()?;
            let fp = r.get_u64()?;
            saved.push((id, s, pct_bin, fp));
        }
        self.n_ticks = n_ticks;
        for (id, s, pct_bin, fp) in saved {
            if let Some(g) = self.groups.iter_mut().find(|g| g.id == id) {
                g.band = Self::stat(&s).unwrap_or(0);
                g.band_upd_ns = 0;
                g.last_sent = 0;
                g.s = s;
                g.pct_bin = pct_bin;
                g.fp = fp;
                g.restored = true;
            }
        }
        Ok(())
    }
}
