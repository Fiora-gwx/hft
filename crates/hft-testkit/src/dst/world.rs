//! The DST world (README "DST 的世界模型" 1, 5): virtual time, the event heap ordered by
//! `(virtual_ns, gen_seq)`, one ChaCha8 stream per actor, and the world fixture `WorldSpec`
//! (docs/tests/scenarios.md §2.2 W1) the engine and the actors are built from.

use std::cmp::Reverse;
use std::collections::BinaryHeap;

use hft_types::{AccountId, AssetId, GroupId, InstId, RunMode, StrategyId, VenueId};
use rand_chacha::ChaCha8Rng;
use rand_core::{RngCore, SeedableRng};

use crate::seed;

/// The actors of a world; each has its own random stream (`ActorRngs`).
#[derive(Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Debug)]
pub enum ActorId {
    Market,
    /// `MockVenue` of the venue with this raw id.
    Venue(u8),
    Sim,
    Ctl,
    Recon,
    Hk,
    Advisor,
    Faults,
}

impl ActorId {
    /// The derivation label of the actor's stream.
    pub fn label(self) -> String {
        match self {
            ActorId::Market => "dst/market".into(),
            ActorId::Venue(v) => format!("dst/venue/{v}"),
            ActorId::Sim => "dst/sim".into(),
            ActorId::Ctl => "dst/ctl".into(),
            ActorId::Recon => "dst/recon".into(),
            ActorId::Hk => "dst/hk".into(),
            ActorId::Advisor => "dst/advisor".into(),
            ActorId::Faults => "dst/faults".into(),
        }
    }
}

/// `ChaCha8Rng::seed_from_u64(seed::derive(seed, actor label))`: adding or removing one actor never
/// changes another actor's stream (TK-047).
pub fn actor_rng(seed: u64, a: ActorId) -> ChaCha8Rng {
    ChaCha8Rng::seed_from_u64(seed::derive(seed, &a.label()))
}

/// Small helpers over a ChaCha stream (no floating point in decisions that reach the engine).
pub trait RngExt {
    /// Uniform in `[0, n)`; `n = 0` → 0.
    fn below(&mut self, n: u64) -> u64;
    /// `true` with probability `num / den`.
    fn chance(&mut self, num: u64, den: u64) -> bool;
    /// Uniform in `[lo, hi]`.
    fn range(&mut self, lo: u64, hi: u64) -> u64;
}

impl<R: RngCore> RngExt for R {
    fn below(&mut self, n: u64) -> u64 {
        if n == 0 {
            return 0;
        }
        // Multiply-shift (Lemire) without rejection: the bias is < n / 2^64, irrelevant here.
        ((u128::from(self.next_u64()) * u128::from(n)) >> 64) as u64
    }
    fn chance(&mut self, num: u64, den: u64) -> bool {
        den != 0 && self.below(den) < num
    }
    fn range(&mut self, lo: u64, hi: u64) -> u64 {
        if hi <= lo {
            return lo;
        }
        lo + self.below(hi - lo + 1)
    }
}

/// One scheduled world event.
#[derive(Clone, Debug)]
pub struct Scheduled<E> {
    pub at: u64,
    pub gen_seq: u64,
    pub ev: E,
}

/// Min-heap of events by `(virtual_ns, gen_seq)`; `gen_seq` is assigned at push and unique, so the
/// pop order is total and independent of the payload.
#[derive(Debug)]
pub struct World<E> {
    heap: BinaryHeap<Reverse<(u64, u64, usize)>>,
    slots: Vec<Option<E>>,
    free: Vec<usize>,
    next_gen: u64,
    now: u64,
    seed: u64,
}

impl<E> World<E> {
    pub fn new(seed: u64) -> World<E> {
        World {
            heap: BinaryHeap::new(),
            slots: Vec::new(),
            free: Vec::new(),
            next_gen: 0,
            now: 0,
            seed,
        }
    }
    pub fn seed(&self) -> u64 {
        self.seed
    }
    /// Current virtual time (time of the last popped event).
    pub fn now(&self) -> u64 {
        self.now
    }
    /// Schedules `ev` at `at` (not earlier than now) with the next `gen_seq`; returns it.
    pub fn schedule(&mut self, at: u64, ev: E) -> u64 {
        let g = self.next_gen;
        self.push_at(at.max(self.now), g, ev);
        g
    }
    /// Schedules `ev` after `delay_ns`.
    pub fn after(&mut self, delay_ns: u64, ev: E) -> u64 {
        let at = self.now.saturating_add(delay_ns);
        self.schedule(at, ev)
    }
    /// Pushes with an explicit `gen_seq` (tests; the counter moves past it).
    pub fn push_at(&mut self, at: u64, gen_seq: u64, ev: E) {
        self.next_gen = self.next_gen.max(gen_seq + 1);
        let ix = match self.free.pop() {
            Some(i) => {
                self.slots[i] = Some(ev);
                i
            }
            None => {
                self.slots.push(Some(ev));
                self.slots.len() - 1
            }
        };
        self.heap.push(Reverse((at, gen_seq, ix)));
    }
    /// Pops the earliest event and advances virtual time to it.
    pub fn pop(&mut self) -> Option<Scheduled<E>> {
        let Reverse((at, gen_seq, ix)) = self.heap.pop()?;
        let ev = self.slots[ix].take()?;
        self.free.push(ix);
        self.now = self.now.max(at);
        Some(Scheduled { at, gen_seq, ev })
    }
    /// Time of the earliest event.
    pub fn peek_time(&self) -> Option<u64> {
        self.heap.peek().map(|Reverse((at, _, _))| *at)
    }
    pub fn len(&self) -> usize {
        self.heap.len()
    }
    pub fn is_empty(&self) -> bool {
        self.heap.is_empty()
    }
}

/// Venue market of a world venue.
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
pub enum VenueMarket {
    Spot,
    Perp,
}

/// One venue of the world fixture.
#[derive(Clone, Eq, PartialEq, Hash, Debug)]
pub struct VenueSpec {
    pub id: VenueId,
    pub market: VenueMarket,
    /// Base-asset fee on both buy and sell. SeqSut supports a spot venue with one common base
    /// asset through VenueCaps::Configured; unsupported worlds fail before matching starts.
    pub fee_in_base: bool,
    /// The venue supplies complete actual perpetual execution cash (PERP_CASH). W1's real
    /// SimExchange provides it; legacy-record counterexamples disable the capability explicitly.
    pub perp_cash: bool,
    /// Venue supports `reduceOnly`.
    pub reduce_only: bool,
    /// Fee rate in units of 10^-6 (1000 = 0.1%).
    pub fee_ppm: i64,
}

/// One inst of the world fixture (raw fixed-point values at the asset's exponents).
#[derive(Clone, Eq, PartialEq, Hash, Debug)]
pub struct InstSpec {
    pub id: InstId,
    pub venue: VenueId,
    pub base: AssetId,
    pub quote: AssetId,
    pub perp: bool,
    pub tick: i64,
    pub lot: i64,
    pub min_qty: i64,
    pub bid: i64,
    pub ask: i64,
    pub depth: i64,
}

/// Live or Sim account.
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
pub enum AccountKind {
    Live,
    Sim,
}

/// One account and its starting balances.
#[derive(Clone, Eq, PartialEq, Hash, Debug)]
pub struct AccountSpec {
    pub id: AccountId,
    pub kind: AccountKind,
    pub venues: Vec<VenueId>,
    /// (venue, asset, raw amount).
    pub balances: Vec<(VenueId, AssetId, i128)>,
}

/// One strategy instance binding (SDK-independent data; the adapter decodes `params` with the
/// strategy set's schema).
#[derive(Clone, Eq, PartialEq, Hash, Debug)]
pub struct StrategyBinding {
    pub instance: StrategyId,
    pub account: AccountId,
    pub kind_id: u16,
    /// Parameter body (SDK schema encoding).
    pub params: Vec<u8>,
    pub mode: RunMode,
}

/// One two-leg group: first leg, second (hedge) leg, ratio and position limit.
#[derive(Clone, Eq, PartialEq, Hash, Debug)]
pub struct GroupSpecW {
    pub account: AccountId,
    pub instance: StrategyId,
    pub group: GroupId,
    pub first: InstId,
    pub second: InstId,
    pub ratio_num: i64,
    pub ratio_den: i64,
    pub max_pos_qty: i128,
}

/// Risk and execution parameters the scenarios set explicitly (docs/tests §4 rule 2).
#[derive(Clone, Eq, PartialEq, Hash, Debug)]
pub struct RiskSpec {
    pub max_hedge_notional: i128,
    pub max_reduce_notional: i128,
    pub broken_auto_fix_ms: i64,
    pub hedge_retry_ms: i64,
    pub hedge_deadline_ms: i64,
    pub force_step_bp: i64,
    pub conn_liveness_ms: i64,
    pub unknown_blocks_open: bool,
}

/// The world fixture.
#[derive(Clone, Eq, PartialEq, Hash, Debug)]
pub struct WorldSpec {
    pub venues: Vec<VenueSpec>,
    pub insts: Vec<InstSpec>,
    pub accounts: Vec<AccountSpec>,
    pub strategies: Vec<StrategyBinding>,
    pub groups: Vec<GroupSpecW>,
    pub risk: RiskSpec,
    /// Optional fixture override, recorded in the engine configuration. None keeps the OMS default.
    pub oms_tombstone_keep_ms: Option<u32>,
    /// Optional OMS slots per (account, venue) table; None keeps the DST default 64.
    pub oms_slots: Option<u32>,
    /// Reserve Multi-record physical seq spans during execution for file replay fixtures.
    pub physical_journal: bool,
    /// Read committed journal Inputs through the real Advisor instead of synthetic advice.
    pub model_advice: bool,
    /// Live margin data: every generated perpetual quote also publishes the mid as the mark price,
    /// the startup account snapshot lists every spot base asset (zero included), and after each
    /// engine restart and every 5 s the venues send their margin snapshot of every account (as a
    /// gateway does after reconnecting and on its poll). Off = the world before 2026-10-14 (no marks), kept for
    /// counterexamples recorded in it. Off in W1 until the engine failures it surfaces are fixed
    /// ([`WorldSpec::w1_trading`]).
    pub margin_feed: bool,
    /// The takers exit on their hold timer and reopen after a cool-down (`close_on` bit 3), so a
    /// run trades in cycles instead of holding its first position. Off in W1 for now: the cycles
    /// expose engine failures still open (docs: hft-testkit README "DST 的世界模型").
    pub taker_cycles: bool,
    /// ECF1 `role_prod`: the world is a prod instance (its Sim accounts are Shadow variants;
    /// `SimFault` is refused). Off = the sim-role world every recorded counterexample ran in.
    pub role_prod: bool,
}

/// `VS`, `VP`, `VQ` (scenarios §2.2): Binance spot, OKX USDT swap, Bitget spot (fixed `VenueId`s).
pub const VS: VenueId = VenueId::BINANCE_SPOT;
pub const VP: VenueId = VenueId::OKX_SWAP;
pub const VQ: VenueId = VenueId::BITGET_UTA_SPOT;
/// UTC of virtual time 0 (the engine adapter's clock calibration and `HkActor`'s `ClockStatus`).
pub const UTC0: i64 = 1_790_814_600_000_000_000;
/// `S1` BTC-USDT spot @ VS, `P1` BTC-USDT perp @ VP.
pub const S1: InstId = InstId(1);
pub const P1: InstId = InstId(2);
/// BTC (`px_exp = −4`, `qty_exp = −8`) and USDT (amount exponent −6).
pub const BTC: AssetId = AssetId(1);
pub const USDT: AssetId = AssetId(2);
/// Accounts `a`, `b`.
pub const ACCT_A: AccountId = AccountId(1);
pub const ACCT_B: AccountId = AccountId(2);
/// Groups `g1` (instance 1, account a), `g2` (instance 2, account b).
pub const G1: GroupId = GroupId(0x0100_0001);
pub const G2: GroupId = GroupId(0x0200_0001);

impl WorldSpec {
    /// World W1 (scenarios §2.2): VS spot / VP perp, S1 / P1, accounts a and b with groups g1, g2
    /// (first leg S1 buy, second leg P1 sell, ratio 1). Strategy kind ids and parameter bodies are
    /// left to the suite (`StrategyBinding.params` empty = the strategy's defaults).
    pub fn w1() -> WorldSpec {
        let venues = vec![
            VenueSpec {
                id: VS,
                market: VenueMarket::Spot,
                fee_in_base: false,
                perp_cash: false,
                reduce_only: false,
                fee_ppm: 1000,
            },
            VenueSpec {
                id: VP,
                market: VenueMarket::Perp,
                fee_in_base: false,
                perp_cash: true,
                reduce_only: true,
                fee_ppm: 500,
            },
        ];
        // px_exp −4: 59 999.99 = 599 999 900; tick 0.01 = 100; lot 0.00001 BTC = 1 000 (qty_exp −8).
        let insts = vec![
            InstSpec {
                id: S1,
                venue: VS,
                base: BTC,
                quote: USDT,
                perp: false,
                tick: 100,
                lot: 1_000,
                min_qty: 1_000,
                bid: 599_999_900,
                ask: 600_000_000,
                depth: 200_000_000,
            },
            InstSpec {
                id: P1,
                venue: VP,
                base: BTC,
                quote: USDT,
                perp: true,
                tick: 1_000,
                lot: 100_000,
                min_qty: 100_000,
                bid: 600_100_000,
                ask: 600_101_000,
                depth: 500_000_000,
            },
        ];
        let acct = |id| AccountSpec {
            id,
            kind: AccountKind::Live,
            venues: vec![VS, VP],
            // USDT amounts at exponent −6.
            balances: vec![(VS, USDT, 100_000_000_000), (VP, USDT, 50_000_000_000)],
        };
        WorldSpec {
            venues,
            insts,
            accounts: vec![acct(ACCT_A), acct(ACCT_B)],
            strategies: vec![
                StrategyBinding {
                    instance: StrategyId(1),
                    account: ACCT_A,
                    kind_id: 0,
                    params: Vec::new(),
                    mode: RunMode::Live,
                },
                StrategyBinding {
                    instance: StrategyId(2),
                    account: ACCT_B,
                    kind_id: 0,
                    params: Vec::new(),
                    mode: RunMode::Live,
                },
            ],
            groups: vec![
                GroupSpecW {
                    account: ACCT_A,
                    instance: StrategyId(1),
                    group: G1,
                    first: S1,
                    second: P1,
                    ratio_num: 1,
                    ratio_den: 1,
                    max_pos_qty: 10_000_000,
                },
                GroupSpecW {
                    account: ACCT_B,
                    instance: StrategyId(2),
                    group: G2,
                    first: S1,
                    second: P1,
                    ratio_num: 1,
                    ratio_den: 1,
                    max_pos_qty: 10_000_000,
                },
            ],
            risk: RiskSpec {
                max_hedge_notional: 50_000_000_000,
                max_reduce_notional: 50_000_000_000,
                broken_auto_fix_ms: 60_000,
                hedge_retry_ms: 100,
                hedge_deadline_ms: 3_000,
                force_step_bp: 2_500,
                conn_liveness_ms: 1_000,
                unknown_blocks_open: true,
            },
            oms_tombstone_keep_ms: None,
            oms_slots: None,
            physical_journal: false,
            model_advice: false,
            margin_feed: false,
            taker_cycles: false,
            role_prod: false,
        }
    }

    /// W1 as before 2026-10-14 (no marks or restart margin snapshots, takers hold their first
    /// position): the world of counterexamples recorded then (TK-121, TK-123, TK-127 …). Equal
    /// to [`WorldSpec::w1`]; standard suites use [`WorldSpec::w1_trading`].
    pub fn w1_legacy() -> WorldSpec {
        let mut w = WorldSpec::w1();
        w.margin_feed = false;
        w.taker_cycles = false;
        w
    }

    /// W1 with live margin data (`margin_feed`) and, with `cycles`, cycling takers: a world that
    /// trades through the whole run. This is the standard suite/CI world.
    pub fn w1_trading(cycles: bool) -> WorldSpec {
        let mut w = WorldSpec::w1();
        w.margin_feed = true;
        w.taker_cycles = cycles;
        w
    }

    /// W1-SIM: W1 with account `s` (`AccountKind::Sim`) instead of a and b.
    pub fn w1_sim() -> WorldSpec {
        let mut w = WorldSpec::w1();
        w.accounts.truncate(1);
        w.accounts[0].kind = AccountKind::Sim;
        w.strategies.truncate(1);
        w.groups.truncate(1);
        w
    }

    pub fn inst(&self, i: InstId) -> Option<&InstSpec> {
        self.insts.iter().find(|s| s.id == i)
    }
    pub fn venue(&self, v: VenueId) -> Option<&VenueSpec> {
        self.venues.iter().find(|s| s.id == v)
    }
    pub fn account(&self, a: AccountId) -> Option<&AccountSpec> {
        self.accounts.iter().find(|s| s.id == a)
    }
}
