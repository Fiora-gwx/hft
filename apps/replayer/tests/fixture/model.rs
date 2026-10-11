//! Real advisor -> Advice ring -> sequencer -> file writer fixture. Checkpoint bytes come
//! from the same advisor that produced the recorded Advice; no expected Advice is fabricated.

use super::*;
use hft_sequencer::advisor::{
    AdviceRing, Advisor, AdvisorCfg, CheckpointStore, CkptHdr, CkptIoErr, checkpoint, ckpt_checksum,
};
use hft_types::StrategyId;

#[derive(Default)]
pub(super) struct Store {
    current: Option<(CkptHdr, Vec<u8>)>,
    pub(super) loaded: BTreeMap<StrategyId, (CkptHdr, Vec<u8>)>,
    pub(super) committed: BTreeMap<StrategyId, (CkptHdr, Vec<u8>)>,
}
impl CheckpointStore for Store {
    fn load(&mut self, s: StrategyId, out: &mut Vec<u8>) -> Option<CkptHdr> {
        let (h, bytes) = self.loaded.get(&s)?;
        out.extend_from_slice(bytes);
        Some(*h)
    }
    fn begin(&mut self, _: StrategyId, h: &CkptHdr) -> Result<(), CkptIoErr> {
        self.current = Some((*h, Vec::new()));
        Ok(())
    }
    fn write_chunk(&mut self, bytes: &[u8]) -> Result<(), CkptIoErr> {
        self.current.as_mut().ok_or(CkptIoErr)?.1.extend_from_slice(bytes);
        Ok(())
    }
    fn commit(&mut self) -> Result<(), CkptIoErr> {
        let (mut h, b) = self.current.take().ok_or(CkptIoErr)?;
        h.len = b.len() as u64;
        h.checksum = ckpt_checksum(&b);
        self.committed.insert(h.strategy, (h, b));
        Ok(())
    }
}
#[derive(Default)]
pub(super) struct Ring(pub(super) Vec<(EventKind, Vec<u8>)>);
impl AdviceRing for Ring {
    fn free(&self) -> usize {
        1 << 24
    }
    fn push(&mut self, kind: EventKind, bytes: &[u8]) -> bool {
        self.0.push((kind, bytes.to_vec()));
        true
    }
}

pub(super) fn published<S: StrategySet>(s: &mut Sequencer<LiveDriver<S, Io<S>>, S>, f: &Fixture) -> Log {
    s.driver_mut().io.writer.publish();
    let active = f
        .file
        .parent()
        .unwrap()
        .join(format!("{:010}.jrn", s.driver().io.writer.segment_no()));
    std::fs::write(
        &active,
        hft_journal::testing::published_bytes(&s.driver().io.writer).unwrap(),
    )
    .unwrap();
    read(f)
}

pub(super) fn poll<S: StrategySet>(a: &mut Advisor<S, Store, Ring>, log: &Log) {
    for _ in 0..10000 {
        if !a.poll_once(log) && !a.checkpointing() {
            assert!(a.failure.is_none(), "advisor catch-up failed: {:?}", a.failure);
            return;
        }
    }
    panic!("advisor did not finish");
}

pub(super) fn drain<S: StrategySet>(
    a: &mut Advisor<S, Store, Ring>,
    s: &mut Sequencer<LiveDriver<S, Io<S>>, S>,
) {
    for (kind, body) in core::mem::take(&mut a.ring.0) {
        input(s, (5, 0), kind, body);
    }
    settle(s);
}

pub(super) fn exercise<S: StrategySet>(
    s: &mut Sequencer<LiveDriver<S, Io<S>>, S>,
    f: &Fixture,
    world: &WorldSpec,
    setup: &hft_sequencer::EngineSetup,
) {
    let log = published(s, f);
    let mut a = Advisor::start(
        &log,
        setup,
        AdvisorCfg {
            batch: 1,
            checkpoint_ms: 0,
            ..Default::default()
        },
        Store::default(),
        Ring::default(),
        Box::leak(Box::new(Heartbeat::new())),
    )
    .unwrap();
    poll(&mut a, &log);
    drain(&mut a, s);
    std::fs::create_dir(f.root.join("checkpoints")).unwrap();
    let first_tsc = s.driver().io.rings.tsc;
    let first_ens = s.engine().last_ens;
    let hz_milli = s.driver().io.rings.hz;
    for second in 1..=12u64 {
        let t = first_tsc + (u128::from(second) * u128::from(hz_milli) / 1000) as u64;
        let utc = s.driver().io.utc_of(Tsc(t));
        s.driver_mut().io.rings.tsc = t;
        s.driver_mut().io.rings.calib.tsc = Tsc(t);
        s.driver_mut().io.rings.calib.realtime_ns = utc;
        let status = s.driver().io.clock_status();
        input(s, (1, 0), EventKind::CLOCK_STATUS, status.as_bytes().to_vec());
        for (vi, venue) in world.venues.iter().enumerate() {
            for i in world.insts.iter().filter(|i| i.venue == venue.id) {
                let mut h = MarketHdr::new_zeroed();
                h.venue = venue.id;
                h.inst = i.id;
                h.rx_tsc = t;
                let b = Bbo {
                    h,
                    bid_px: Px(i.bid + second as i64 * i.tick),
                    bid_qty: Qty(i.depth),
                    ask_px: Px(i.ask + second as i64 * i.tick),
                    ask_qty: Qty(i.depth),
                    update_id: second,
                };
                input(s, (4, vi as u8), EventKind::BBO, b.as_bytes().to_vec());
            }
        }
        settle(s);
        let log = published(s, f);
        poll(&mut a, &log);
        drain(&mut a, s);
        if second == 6 {
            let log = published(s, f);
            poll(&mut a, &log);
            a.begin_checkpoints();
            poll(&mut a, &log);
            assert!(!a.ckpt.committed.is_empty());
            for (h, body) in a.ckpt.committed.values() {
                let name = s.engine().rd.instance(h.strategy).unwrap().name.as_ref();
                std::fs::write(
                    f.root.join("checkpoints").join(format!("{name}.ckpt")),
                    checkpoint::encode(h, body, 1 << 20).unwrap(),
                )
                .unwrap();
            }
            drain(&mut a, s);
            command(s, 60, &ControlCmd::SnapshotNow);
            settle(s);
        }
    }
    assert_eq!(s.engine().last_ens - first_ens, 12_000_000_000);
    let log = published(s, f);
    assert!(log.recs.iter().any(|r| r.kind == EventKind::ADVICE));
}
