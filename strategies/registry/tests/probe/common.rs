//! Real non-production strategy instances; virtual-time exchange responses only.
#![allow(dead_code)]
use hft_event::{Bbo, GwCmd, MarketHdr};
use hft_strategy_registry::Strategies;
use hft_strategy_sdk::params::{RawParams, encode_body};
use hft_testkit::dst::{
    SeqSut, SimMatcher,
    sut::{BootCtx, FrameRec, InputRec, Source, StepOut, Sut},
    venue_behavior::{Matcher, rep_input},
    world::{ACCT_A, P1, S1, VenueMarket, WorldSpec},
};
use hft_types::{InstId, Px, Qty, VenueId};
use latency_probe::ProbeParams;
use zerocopy::FromZeros;

pub fn world(mode: i64, xgw: bool, far: i64) -> WorldSpec {
    let mut w = WorldSpec::w1();
    w.accounts.truncate(1);
    w.groups.truncate(1);
    w.strategies.truncate(1);
    // The two registered perpetual product IDs; the strategy sees only VenueCaps.
    for (v, id) in w.venues.iter_mut().zip([VenueId(1), VenueId(10)]) {
        v.id = id;
        v.market = VenueMarket::Perp;
        v.perp_cash = true;
        v.reduce_only = true;
        v.fee_ppm = 0;
    }
    for (i, v) in w.insts.iter_mut().zip(&w.venues) {
        i.venue = v.id;
        i.perp = true;
        i.bid = 600_000_000;
        i.ask = 600_001_000;
        i.tick = 1_000;
        i.lot = 100_000;
        i.min_qty = 100_000;
    }
    w.accounts[0].venues = w.venues.iter().map(|v| v.id).collect();
    for (b, v) in w.accounts[0].balances.iter_mut().zip(&w.venues) {
        b.0 = v.id;
    }
    let mut raw = RawParams {
        defaults: ProbeParams::SCHEMA.fields.iter().map(|f| f.default).collect(),
        overrides: vec![],
    };
    raw.defaults[0] = 1;
    raw.defaults[1] = mode;
    raw.defaults[3] = far;
    raw.defaults[6] = i64::from(xgw);
    w.strategies[0].kind_id = 5;
    w.strategies[0].params = encode_body(&ProbeParams::SCHEMA, 1, &raw).unwrap();
    w
}

pub struct Rig {
    pub sut: SeqSut<Strategies>,
    pub venues: Vec<(VenueId, SimMatcher)>,
    pub now: u64,
    pub sent: Vec<FrameRec>,
    update: u64,
}
impl Rig {
    pub fn new(w: &WorldSpec) -> Self {
        let sut = SeqSut::boot(&BootCtx {
            seed: 7,
            world: w,
            planted: &[],
            tsc_hz: 1_000_000_000,
        })
        .unwrap();
        Self {
            sut,
            venues: w
                .venues
                .iter()
                .map(|v| (v.id, SimMatcher::new(w, v.id, 7)))
                .collect(),
            now: 10_000_000,
            sent: vec![],
            update: 100,
        }
    }
    fn output(&mut self, out: StepOut) {
        for f in out.frames {
            let c: GwCmd = f.cmd;
            let venue = self.sut.world().inst(c.inst).map(|i| i.venue);
            // SetEpoch/connection control are already handled by SeqSut's gateway adapter.
            if let Some(v) = venue {
                self.venues
                    .iter_mut()
                    .find(|x| x.0 == v)
                    .unwrap()
                    .1
                    .on_command(&c, self.now);
            }
            self.sent.push(f);
        }
    }
    pub fn feed(&mut self, inst: InstId) {
        let i = self.sut.world().inst(inst).unwrap().clone();
        self.venues.iter_mut().find(|v| v.0 == i.venue).unwrap().1.on_bbo(
            inst,
            (i.bid, i.depth),
            (i.ask, i.depth),
            self.now,
        );
        self.update += 1;
        let mut h = MarketHdr::new_zeroed();
        h.venue = i.venue;
        h.inst = inst;
        h.rx_tsc = self.now;
        let b = Bbo {
            h,
            bid_px: Px(i.bid),
            ask_px: Px(i.ask),
            bid_qty: Qty(i.depth),
            ask_qty: Qty(i.depth),
            update_id: self.update,
        };
        let mut out = StepOut::default();
        self.sut
            .process(self.now, self.now, &InputRec::of(&b, Source::Market), &mut out);
        self.output(out);
    }
    pub fn advance(&mut self, until: u64) {
        // 2 ms resolution covers SimExchange's 1 ms entry + 1 ms reply delays.
        while self.now < until {
            self.now = (self.now + 2_000_000).min(until);
            let mut replies = vec![];
            for (id, venue) in &mut self.venues {
                let mut rows = vec![];
                venue.drain_due(self.now, &mut rows);
                replies.extend(rows.into_iter().map(|r| (*id, r)));
            }
            for (v, r) in replies {
                let mut out = StepOut::default();
                self.sut
                    .process(self.now, self.now, &rep_input(&r.rep, Source::Venue(v)), &mut out);
                self.output(out);
            }
            let mut out = StepOut::default();
            self.sut.fire_timers(self.now, self.now, &mut out);
            self.output(out);
        }
    }
    pub fn run(&mut self, seconds: u64) {
        for _ in 0..seconds * 10 {
            self.feed(S1);
            self.feed(P1);
            self.advance(self.now + 100_000_000);
        }
    }
    pub fn positions(&self) -> (i64, i64) {
        let l = &self.sut.sequencer().engine().ledger;
        (l.pos(ACCT_A, S1).qty.0, l.pos(ACCT_A, P1).qty.0)
    }
    pub fn replay(&self) {
        assert_eq!(self.sut.rebuilt_hashes(), Ok(Some(self.sut.hashes())));
        assert!(self.sut.verify_journal().unwrap().is_some());
    }
}
