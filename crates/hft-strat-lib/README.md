---
description: "策略 crate 可选复用的通用构件:快速部分的定点 O(1) 判断(两腿价差与成本线、触发价、确认窗口、脏数据过滤、节流、L2 VWAP、资金费统计)与慢速部分(Model)的窗口分位统计和领先-滞后估计;不含任何策略的规则数字"
kind: crate
tier: pure
scope: core
status: spec
---

# hft-strat-lib

## 概要

`hft-strat-lib` 是 `strategies/*` 下各策略 crate **可选依赖的通用构件库**:把"两腿价差 + 成本线""持续成立才确认""报价连接不健康即不做(安静 ≠ 过期)""冷却与连续失败熔断""按 L2 算 VWAP""窗口分位""资金费统计""领先-滞后"这些与具体策略无关的规则写成可单测、可回放的纯 Rust 构件。它只依赖 L0 与 [hft-strategy-sdk](../hft-strategy-sdk/README.md);**引擎 crate 不依赖它**。它不拥有策略状态(构件是策略状态里的字段类型或纯函数)、不做 I/O、不读时钟(时间由调用方从事件传入),也**不含任何策略的阈值**:所有阈值都是调用参数,默认值由使用它的策略在自己的参数 schema 里给出。构件分两类:**快速部分可用**(定点、无 f64、无分配、O(1) 或有界 O(k)),由策略的 `Strategy::on_event` 在 sequencer 线程调用;**只给 Model 用**(允许 f64 与启动时分配),由策略的 `Model` 在 advisor 线程驱动,结果由策略自己编码成 Advice 载荷。

## 目录

- [使用本组件](#使用本组件)
  - [单位约定](#单位约定)
  - [构件总表](#构件总表)
  - [快速部分可用的构件](#快速部分可用的构件)
  - [只给 Model 用的构件](#只给-model-用的构件)
  - [输出与 Advice 载荷](#输出与-advice-载荷)
  - [参数](#参数)
- [实现说明](#实现说明)
- [延迟与资源契约](#延迟与资源契约)
- [测试与验收](#测试与验收)
- [依赖](#依赖)
- [延伸阅读](#延伸阅读)
- [已知限制与后续工作](#已知限制与后续工作)

## 使用本组件

### 单位约定

- 价差、阈值、成本、比例一律 `hft-types::Rate`(i64,1e-9;**1 bp = 100,000,1% = 10,000,000**)。价差用 `Px::ratio(base)` 求(HalfEven),触发价用 `Px::mul_rate`。不用 bp 整数:bp 整数对 0.5 bp 级的边不够分辨;`Rate` 同为整数、同样无 f64。
- 价格/数量为 `hft-types` 的 `Px`/`Qty`,同一资产在所有 venue 用同一指数([13 R4](../../design/13-设计评审与优化.md)),跨 venue 比较不换算;乘积用 i128。
- 时间(三条时间轴的定义见 [SDK 时间轴](../hft-strategy-sdk/README.md#时间轴);时间一律由调用方从 `Views::now()` 与事件传入,本库不读时钟):
  - **有状态的快速部分构件**(`TriggerPx`、`ConfirmWindow`、`Throttle`、`Expiring`)内部存的时刻一律是 **`engine_ns`**(i64,跨重启连续、停机不计),调用方传 `Now.engine_ns`;时长参数以 ms 给出,换成 ns(`*ParamsNs` 结构,`× 1_000_000`,参数变化时重算)。它们会进策略快照,所以**不存 TSC**。
  - **无状态的单事件判断**(`DirtyFilter`)比较的是本次启动内的行情时刻(SDK `BboView` 的 `*_tsc`),用 TSC 与 `DirtyParamsTsc`(× `Now.tsc_per_ms`)。
  - **资金费与 Model 侧构件**用 UTC ns(`Now.utc_ns` 或事件体里的接收/交易所时间换算的 UTC)。

### 构件总表

| 构件 | 快速部分可用 | 只给 Model | f64 | 复杂度 | 用途 |
|---|---|---|---|---|---|
| `SpreadCalc` + `CostLine` | 是 | 也可用 | 否 | O(1)(`CostLine` 合计在参数变化时预算) | 两腿价差、成本线、净边;费率从 refdata 取 |
| `TriggerPx` | 是 | — | 否 | 慢侧更新 O(1),快侧比较 1 次 | 预计算触发价:快侧 tick 只做一次整数比较 |
| `ConfirmWindow` | 是 | — | 否 | O(1) | 条件在 N 次更新、T ms、慢侧序号跨度内持续成立才确认 |
| `DirtyFilter` | 是 | — | 否 | O(1) | 报价不健康(CEX 腿按连接健康,安静 ≠ 过期;外部报价腿按最近报价时刻)、重同步、异常价差、连续无价 |
| `Throttle` | 是 | — | 否 | O(1) | 冷却、在途、连续失败、限速退避、halted 镜像;按动作类别放行 |
| `ImpactModel::vwap` | 是 | 也可用 | 否 | O(k),k ≤ `max_levels`(≤ 64) | 用 L2 算给定数量的 VWAP 与冲击 |
| `FundingStats` | 是 | 也可用 | 否 | `observe` O(1),`summary` O(n),n ≤ 256;`prefill_settled` O(256)(只在 Model) | 资金费预测/已实现/最差/正负期数;检测实际结算周期变化;接受预热注入的历史结算 |
| `Expiring<T>` | 是 | — | 否 | O(1) | 带有效期的值(缓存 Advice 解码结果:跨启动按 `basis_seq < boot_seq` 过期,启动内有效期从 `basis_tsc` 起算) |
| `PercentileStats` | 否 | 是 | 否(整数直方图;但有启动分配与 µs 级查询) | 采样 O(窗口数),查询 O(块数 + 256),预热每样本 O(窗口数) | 窗口与采样间隔参数化的买卖两侧分位;接受预热注入的历史样本;快照只存 `PctConfig`、末次观测与样本环 |
| `LeadLag` | 否 | 是 | 是 | `observe` O(对侧 ±max_lag 内的变动数),`estimate` O(L) | 两个 venue 同一资产的领先-滞后估计 |

腿敞口与在途量不在本库:策略从 SDK 的 `Views::legs()`(`LegView`,数据由 [hft-oms](../hft-oms/README.md) 维护)读取。执行算法(切片、两腿 OnFill、目标跟踪)在 [hft-exec](../hft-exec/README.md)。

### 快速部分可用的构件

以下构件满足 [hot-path-rules](../../docs/hot-path-rules.md):无分配、无系统调用、无 `dyn`、无 f64、无 panic 路径(溢出用 i128 中间量或饱和并返回错误枚举)。

```rust
// ---- 价差与成本线 ----
pub struct SpreadCalc;
impl SpreadCalc {
    /// 开向价差:卖 fast(按 bid)、买 slow(按 buy_px_slow)= (bid_fast − buy_px_slow) / buy_px_slow。
    /// buy_px_slow 由调用方给:slow 为 CEX 时是 ask;为外部报价时是"按数量报出的买入均价"(费与冲击已含在内)。
    pub fn buy(bid_fast: Px, buy_px_slow: Px) -> Rate;
    /// 平向价差:买回 fast(按 ask)、卖 slow = (ask_fast − sell_px_slow) / sell_px_slow。
    pub fn sell(ask_fast: Px, sell_px_slow: Px) -> Rate;
    pub fn net(spread: Rate, cost: Rate) -> Rate;                  // spread − cost(饱和减)
}

/// 成本项:每项一个 Rate 与用途位。策略决定有哪些项、各项取值(参数或 refdata),本库只做按位求和。
#[repr(C)] pub struct CostItem { pub rate: Rate, pub label: u16 /* 策略自定,只用于 Note 与面板 */, pub flags: u8, pub _pad: [u8; 5] } // 16 B
pub mod cost_flags {
    pub const ROUND_TRIP: u8 = 1;   // 计入往返成本(与 rt_floor 取大)
    pub const FIXED: u8 = 2;        // 往返之外的固定加项(执行成本、余量)
    pub const OUT_OF_QUOTE: u8 = 4; // 两侧报价里没有包含、平仓时仍要付的成本(保本线用)
}
#[repr(C)] pub struct CostLine { pub items: [CostItem; 8], pub n: u8, pub _pad: [u8; 7], pub rt_floor: Rate,
                                 /* 以下三项在 set() 时预算 */ rt: Rate, open_floor: Rate, out_of_quote: Rate }
impl CostLine {
    pub fn set(&mut self, items: &[CostItem], rt_floor: Rate) -> Result<(), CostErr>;   // n ≤ 8;参数变化时调用;Err 时 self 不变
    pub fn round_trip(&self) -> Rate;     // max(rt_floor, Σ ROUND_TRIP 项)
    pub fn open_floor(&self) -> Rate;     // round_trip() + Σ FIXED 项
    pub fn out_of_quote(&self) -> Rate;   // Σ OUT_OF_QUOTE 项
}
pub enum CostErr { TooMany /* items.len() > 8 */, BadFlags /* 含未定义的位,或 ROUND_TRIP 与 FIXED 同时置位 */, Overflow /* 任一合计超出 i64 */ }
pub enum Liquidity { Taker, Maker }
pub struct FeeLeg { pub inst: InstId, pub account: AccountId, pub liq: Liquidity }
/// 两腿手续费往返 = Σ 各腿 (开 + 平) 费率 = 2 × Σ fee(leg)。fee 取 SDK `RefView::fee(account, inst)`(`FeeView`,来自 refdata `FeeSchedule` 的账户费率档)
/// (taker 或 maker;maker 为负 = 返佣)。只在 on_params / 参数变化时调用,不在 tick 上。i128 求和,超出 i64 时饱和。
/// UnknownInst:inst 不在 refdata;NoFeeTier:该 (account, inst) 没有费率(account 不是 rd.account(),或 inst 的 venue 不在该账户 venues 内)。
pub fn fee_round_trip(rd: &RefView<'_>, legs: &[FeeLeg]) -> Result<Rate, FeeErr>;   // UnknownInst | NoFeeTier;第一个出错的腿决定错误

// ---- 预计算触发价 ----
#[derive(Clone, Copy, Default)]    // Default = 失效(valid = false)
pub struct TriggerPx { pub buy_bid_fast: Px, pub sell_ask_fast: Px, pub valid: bool, pub slow_ens: i64 /* engine_ns */ }
impl TriggerPx {
    /// 慢侧(另一腿 BBO、外部报价、参数或 advice 改变 θ)变化时调用;θ 已含成本线与策略阈值。
    /// buy_px_slow ≤ 0、sell_px_slow ≤ 0 或 tick_fast ≤ 0 → 等同 invalidate()。
    pub fn update(&mut self, buy_px_slow: Px, sell_px_slow: Px, theta_open: Rate, theta_close: Rate, tick_fast: Px, now_ens: i64);
    pub fn invalidate(&mut self);
    #[inline] pub fn hit_buy(&self, bid_fast: Px) -> bool;    // valid && bid_fast >= buy_bid_fast
    #[inline] pub fn hit_sell(&self, ask_fast: Px) -> bool;   // valid && ask_fast <= sell_ask_fast
    pub fn snapshot(&self, w: &mut SnapshotWriter);           // buy_bid_fast i64、sell_ask_fast i64、valid u8、slow_ens i64(25 B)
    pub fn restore(&mut self, r: &mut SnapshotReader) -> Result<(), RestoreError>;   // 读回字段后 valid = false(见实现说明第 5 条)
}

// ---- 确认窗口 ----
#[derive(Default)]                 // Default = Idle、计数 0
pub struct ConfirmWindow { since_ens: i64, n_updates: u16, n_slow: u16, first_slow_seq: u64, last_slow_seq: u64, state: Confirm }
pub struct ConfirmParams { pub min_ms: u32, pub min_updates: u16, pub min_slow_updates: u16, pub min_slow_span: u8 }
pub struct ConfirmParamsNs { /* min_ms 换算为 ns,其余同上 */ }
impl ConfirmParams { pub fn ns(&self) -> ConfirmParamsNs; }
#[derive(Default)] pub enum Confirm { #[default] Idle, Pending, Confirmed }
impl ConfirmWindow {
    /// cond:本次评估条件是否成立;from_slow:本次评估由慢侧更新触发;slow_seq:慢侧序号(外部报价为块号,CEX 慢侧为 update_id,无则 0)。
    pub fn observe(&mut self, cond: bool, from_slow: bool, slow_seq: u64, now_ens: i64, p: &ConfirmParamsNs) -> Confirm;
    pub fn reset(&mut self);
    pub fn state(&self) -> Confirm;
    pub fn snapshot(&self, w: &mut SnapshotWriter);           // since_ens i64、n_updates u16、n_slow u16、first_slow_seq u64、last_slow_seq u64、state u8(29 B)
    pub fn restore(&mut self, r: &mut SnapshotReader) -> Result<(), RestoreError>;
}

// ---- 脏数据过滤(无状态;安静 ≠ 过期)----
pub struct DirtyFilter;
/// 一条腿的报价健康输入。CEX 腿与引擎用同一判定(SDK `BboView::quote_live`):看连接,不看报价最近一次变化。
#[derive(Clone, Copy)]
pub struct LegFresh { pub live_tsc: Tsc, pub conn_ok: bool, pub resync: bool }
impl LegFresh {
    /// CEX 腿:live_tsc = b.conn_live_tsc,conn_ok = b.conn_up,resync = b.resync。
    pub fn from_bbo(b: &BboView) -> LegFresh;
    /// 外部报价腿(按块或按请求产出的报价,没有新报价即是过期):live_tsc = 最近一条报价的时刻。
    pub fn from_quote(quote_tsc: Tsc, conn_ok: bool, resync: bool) -> LegFresh;
}
pub struct Freshness {
    pub fast: LegFresh, pub slow: LegFresh,
    pub no_quote_streak: u16,     // 慢侧连续报不出价的次数(调用方维护)
}
pub struct DirtyParams { pub stale_fast_ms: u32, pub stale_slow_ms: u32, pub max_abs_spread: Rate, pub junk_after: u16 }
pub struct DirtyParamsTsc { /* 两个 ms 换算为 tsc(× Now.tsc_per_ms),其余同上 */ }
impl DirtyParams { pub fn tsc(&self, tsc_per_ms: u64) -> DirtyParamsTsc; }
pub enum DirtyReason { Stale, Resync, AbsSpread, Junk }
impl DirtyFilter {
    pub fn check(spread: Rate, fr: &Freshness, now: Tsc, p: &DirtyParamsTsc) -> Result<(), DirtyReason>;
}

// ---- 节流(halted 只拦开仓;Flatten 与硬动作绕过 cooldown 与 in_flight)----
#[repr(u8)] pub enum FireKind { Open = 0, Close = 1, Flatten = 2, Hard = 3 }
pub enum ThrottleReason { InFlight, Cooldown, Halted, Backoff, HardRearm }
pub enum Outcome { Filled, NoFill, Failed, Rollback, Gated }      // Gated = 门控拒绝(hft-types RejectBits 的门控位,如 KILLED、CLOSE_ONLY、VENUE_NOT_READY、RATE、LINK_STALE、LINK_FUNDS)
pub enum OutcomeAction { None, RequestHalt, EnterFallback }
#[derive(Default)]                 // Default = 全部截止为 0、标志为假、计数 0
pub struct Throttle { cooldown_until_ens: i64, in_flight: bool, consec_fail: u8, fallback_until_ens: i64,
                      backoff_until_ens: i64, halted: bool, hard_rearm_until_ens: i64 }   // 时刻全部是 engine_ns
pub struct ThrottleParams { pub cooldown_ms: u32, pub max_consec_fail: u8, pub fallback_ms: u32, pub hard_rearm_ms: u32 }
pub struct ThrottleParamsNs { /* 三个 ms 换算为 ns */ }
impl ThrottleParams { pub fn ns(&self) -> ThrottleParamsNs; }
impl Throttle {
    pub fn can_fire(&self, k: FireKind, now_ens: i64) -> Result<(), ThrottleReason>;
    pub fn on_fire(&mut self, k: FireKind, now_ens: i64, p: &ThrottleParamsNs);
    /// 一个开/平流程终结时调用(由 Order/Exec/Swap 更新推进);返回值告诉策略是否该请求 Halted 或进入回退期。
    pub fn on_outcome(&mut self, o: Outcome, now_ens: i64, p: &ThrottleParamsNs) -> OutcomeAction;
    pub fn set_halted(&mut self, on: bool);        // 镜像引擎 Halted cap(SDK Risk 事件 CapSet/CapCleared)
    /// 镜像限速退避(覆盖为 until_ens)。SDK 只给布尔读数 InstRiskView.rate_backoff(退避截止归 hft-risk,不暴露),
    /// 所以调用方每个事件按它镜像:为真 → set_backoff(i64::MAX),为假 → set_backoff(0)。
    pub fn set_backoff(&mut self, until_ens: i64);
    pub fn in_fallback(&self, now_ens: i64) -> bool;
    pub fn reset_open_state(&mut self);            // Mode 事件:cooldown_until_ens 与 fallback_until_ens 置 0;其余字段不变
    pub fn halted(&self) -> bool;                  // halted 镜像的当前值
    pub fn consec_fail(&self) -> u8;               // 当前连续失败数
    pub fn snapshot(&self, w: &mut SnapshotWriter); // 按上面字段声明顺序:i64、u8、u8、i64、i64、u8、i64(35 B)
    pub fn restore(&mut self, r: &mut SnapshotReader) -> Result<(), RestoreError>;
}

// ---- L2 VWAP ----
pub struct ImpactModel;
pub struct VwapParams { pub book_frac: Rate, pub max_levels: u8 }
pub enum Vwap { Top(Px), Walked { vwap: Px, levels: u8, impact: Rate }, Insufficient { avail: Qty } }
impl ImpactModel {
    /// 按吃单方向 side 吃 qty(`Buy` = 买入、吃卖盘;`Sell` = 卖出、吃买盘):顶档量 ≥ qty / book_frac → Top;否则逐档累加至 max_levels;不够 → Insufficient。
    /// book 是 SDK 的簿视图(`V::Book<'_>`,实现 `BookRead`;簿数据由 hft-book 维护);status ≠ Live 或该侧无档 → Insufficient{avail: 0}。
    /// 只订阅 BBO 的 inst 的簿视图为 Uninit(SDK Views::book),得到 Insufficient{avail: 0};深度来自分档快照流时簿只有 N 档(见 SDK EvMask::BOOK),超出 N 档的量按 Insufficient 处理。逐档读取用 `BookRead::levels` 拷入调用方栈上的 `[Level; 64]`。
    pub fn vwap<B: BookRead>(book: &B, side: Side, qty: Qty, p: &VwapParams) -> Vwap;
}

// ---- 资金费统计 ----
#[repr(C)] pub struct SettleRec { pub at_ns: i64, pub rate: Rate, pub flags: u32, pub _pad: u32 }   // flags: funding_flags::GAP
pub mod funding_flags {             // SettleRec.flags 与 FundingSummary.flags 共用的位(随 Advice 载荷原样传出,只追加)
    pub const INTERVAL_CHANGED: u32 = 1; pub const STALE: u32 = 2; pub const GAP: u32 = 4; pub const NO_DATA: u32 = 8; pub const TRUNCATED: u32 = 16;
}
#[repr(C)] pub struct FundingSummary {
    pub predicted: Rate,           // 当期预测费率(最新 Funding 事件)
    pub predicted_daily: Rate,     // predicted × 86400 / interval_s
    pub realized_sum: Rate,        // 窗口内已结算费率之和(每期一个费率,不乘仓位)
    pub realized_daily: Rate,      // realized_sum 折日化:realized_sum × 86400 / 窗口秒数
    pub min_rate: Rate, pub max_rate: Rate,    // 窗口内单期最差/最好(空头视角最差 = min_rate)
    pub n_settles: u16, pub n_positive: u16, pub n_negative: u16, pub neg_streak: u16,  // neg_streak = 最近连续负费率期数
    pub interval_s: u32,           // 当前实际结算周期(秒)
    pub flags: u32,                // INTERVAL_CHANGED | STALE | GAP | NO_DATA | TRUNCATED(环已满且最旧记录晚于窗口起点:窗口内的结算没有全部保留)
    pub next_settle_ns: i64,
    pub interval_changed_at_ns: i64,
}
pub enum FundingObs { Normal, Settled(SettleRec), IntervalChanged { old_s: u32, new_s: u32 },
                      SettledChanged { rec: SettleRec, old_s: u32, new_s: u32 } }   // 同一事件既结算又改周期
pub const FUNDING_RING: usize = 256;   // 覆盖 1 h 周期 10 天、8 h 周期 85 天
pub struct FundingStats { ring: [SettleRec; FUNDING_RING], head: u16, len: u16, last_rate: Rate, last_next_ns: i64,
                          interval_s: u32, interval_changed_at_ns: i64, last_event_ns: i64 }
impl FundingStats {
    pub fn new() -> FundingStats;                                            // 空环,观测状态全 0
    pub fn observe(&mut self, f: &FundingView, now_ns: i64) -> FundingObs;  // 每个 Funding 事件调用一次;now_ns = Now.utc_ns
    pub fn summary(&self, window_ns: i64, now_ns: i64, stale_ns: i64) -> FundingSummary;
    pub fn daily(rate: Rate, interval_s: u32) -> Rate;                       // rate × 86400 / interval_s(i128,HalfEven,超出 i64 饱和到 ±i64::MAX);interval_s = 0 → Rate(0)
    /// 只在 Model 中调用(预热注入):插入历史结算记录;at_ns 已在环中的忽略(不覆盖实时检测到的结算),
    /// 环满时保留最新的 FUNDING_RING 条;幂等、与调用先后无关。返回本次调用后仍在环中的新插入条数。
    pub fn prefill_settled(&mut self, recs: &[SettleRec]) -> u16;
    pub fn snapshot(&self, w: &mut SnapshotWriter);                          // 环与观测状态(全部是 UTC ns 与费率)
    pub fn restore(&mut self, r: &mut SnapshotReader) -> Result<(), RestoreError>;
}

// ---- 带有效期的值 ----
pub struct Expiring<T: Copy> { v: T, valid_until_ens: i64, basis_seq: u64, set: bool }
pub enum Fresh<'a, T> { Fresh(&'a T), Stale(&'a T), Missing }
impl<T: Copy> Expiring<T> {
    pub fn new(init: T) -> Self;    // 未设置:set = false、valid_until_ens = 0、basis_seq = 0(get → Missing);init 只占位
    /// 缓存 Advice(SDK Advice 载荷的两条过期规则):valid_until_ens = now.ens_of(basis_tsc) + valid_ms(按依据时刻,不按到达时刻)。
    pub fn put_advice(&mut self, v: T, basis_seq: u64, basis_tsc: Tsc, valid_ms: u32, now: &Now);
    /// 其他来源:直接给 engine_ns 截止;basis_seq 取 now.seq(本次启动内有效)。
    pub fn put(&mut self, v: T, valid_until_ens: i64, basis_seq: u64);
    /// 未 put 过 → Missing;basis_seq < now.boot_seq(上次启动的依据)或 now.engine_ns > valid_until_ens → Stale。是否"过期即保守"由策略决定。
    pub fn get(&self, now: &Now) -> Fresh<'_, T>;
    /// set u8、valid_until_ens i64、basis_seq u64(17 B),随后由 put 写 v;restore 用 get 读 v(set = 0 时不写、不读 v)。
    pub fn snapshot(&self, w: &mut SnapshotWriter, put: fn(&T, &mut SnapshotWriter));
    pub fn restore(&mut self, r: &mut SnapshotReader, get: fn(&mut SnapshotReader) -> Result<T, RestoreError>) -> Result<(), RestoreError>;
}
```

### 只给 Model 用的构件

只在策略的 `Model`(advisor 线程,warm)里使用。允许启动时分配与 f64;必须确定:只用调用方传入的事件时间,不读时钟、不迭代 `HashMap`、不做 I/O([hft-strategy-sdk Model](../hft-strategy-sdk/README.md))。

```rust
// ---- 窗口分位统计(买卖两侧)----
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct PctConfig {
    pub sample_ms: u32,                 // 采样间隔(事件时间)                                   [存储解释]
    pub windows_ms: [u64; 4], pub n_windows: u8,   // 查询窗口,升序;最大者决定环形缓冲长度   [存储解释]
    pub lo: Rate, pub hi: Rate,         // 直方图范围;越界样本夹到端箱并计数                     [存储解释]
    pub bin: Rate,                      // 箱宽                                                   [存储解释]
    pub fill_max_ms: u32,               // 相邻观测间隔 ≤ 它时,空采样点用上一值补齐;否则记缺口(只影响此后的补齐)
    pub sides: u8,                      // 2 = 买卖两侧;1 = 只建 Buy 侧(observe 的 sell 参数被忽略,内存减半)   [存储解释]
}
impl PctConfig {
    /// 标 [存储解释] 的字段全部相同 → true(windows_ms 只比前 n_windows 项);决定已存箱号能否按新配置解释。使用方据此判定按 group 重置。
    pub fn layout_eq(&self, other: &PctConfig) -> bool;
}
/// new 与 restore 的配置校验:Sample = sample_ms 为 0;Windows = n_windows ∉ 1..=4,或前 n_windows 项不严格升序、不是 sample_ms 的正整数倍;
/// Range = lo ≥ hi、bin ≤ 0、(hi − lo) 不是 bin 的整数倍或 n_bins > i32::MAX;Sides = sides ∉ {1, 2}。
pub enum PctErr { Sample, Windows, Range, Sides }
#[repr(u8)] pub enum PctSide { Buy = 0, Sell = 1 }
pub struct Coverage { pub samples: u32, pub expected: u32, pub gaps: u32, pub clamped: u32 }
pub struct PercentileStats { /* cfg: PctConfig;last: (ts_ns: i64, buy: Rate, sell: Rate)(末次观测:当前采样区间尚未提交的值,也是补齐用的上一值;
                               无观测时 ts_ns = 0);ring: Box<[[i32; 2]]>(每采样点两侧的箱号,MISSING 哨兵);hist: Box<[TwoLevelHist]>(窗口 × 侧) */ }
impl PercentileStats {
    pub fn new(cfg: &PctConfig) -> Result<Self, PctErr>;           // 唯一分配点
    pub fn mem_bytes(cfg: &PctConfig) -> u64;                       // 上界(环 + 直方图 + 块计数),供 Model::mem_bytes 求和
    pub fn snapshot_bytes(cfg: &PctConfig) -> u64;                  // 上界(PctConfig + 末次观测 + 环),供 Model::snapshot_bytes 求和
    pub const LAYOUT_ELEMS: &'static [ElemDesc];                    // snapshot 的元素序列,供使用方拼进自己的 ModelLayoutDesc
    pub fn config(&self) -> &PctConfig;
    pub fn set_fill_max_ms(&mut self, ms: u32);                     // 只改 fill_max_ms(唯一可原地更新的配置项);环与直方图不变
    /// 预热(Model 收到预热注入时调用;可在 observe 之前或之后):samples = (采样点时刻 ns, buy, sell),
    /// 只填写环中当前窗口内仍为 MISSING 的采样点,已有值(实时或先前预热)的点不覆盖;幂等、与调用先后无关。
    /// fill_ms > sample_ms 时一个样本覆盖 [ts, ts + fill_ms) 内的各采样点(粗粒度来源,如 1 分钟 K 线);返回填入的点数。
    /// 不因样本内容失败(ts_ns ≤ 0 或落在窗口外的样本跳过);返回 Result 只为接口稳定。
    pub fn prefill<I: Iterator<Item = (i64, Rate, Rate)>>(&mut self, samples: I, fill_ms: u32, now_ns: i64) -> Result<u32, PctErr>;
    pub fn observe(&mut self, ts_ns: i64, buy: Rate, sell: Rate);
    pub fn value(&self, side: PctSide, window_ix: u8, q_ppm: u32) -> Option<Rate>;  // 分位 q 处的值;窗口内无样本 → None(样本是否够由调用方按 coverage 判)
    pub fn rank(&self, side: PctSide, window_ix: u8, x: Rate) -> Option<u32>;       // 样本中 ≤ x 的比例(ppm);"≥ x 的比例" = 1e6 − rank(x − 1)
    pub fn coverage(&self, window_ix: u8) -> Coverage;
    pub fn snapshot(&self, w: &mut SnapshotWriter);                 // 依次写 PctConfig、末次观测、样本环;直方图是派生状态,不写
    /// 读出快照里的 PctConfig、末次观测与样本环并重建直方图;快照的 PctConfig 与 self 的不同时,以快照为准(随后由使用方按 layout_eq 判定是否重置)。
    pub fn restore(&mut self, r: &mut SnapshotReader) -> Result<(), RestoreError>;
}

// ---- 领先-滞后 ----
pub struct LeadLagCfg { pub bucket_ns: u32, pub max_lag_ns: u32, pub window_ms: u32, pub period_ms: u32, pub min_samples: u32,
                        pub max_changes: u32 /* 每腿窗口内最多保留的 mid 变动数;超出丢最旧并计数 */ }
#[repr(C)] pub struct LeadLagEst {
    pub lead_leg: u8,              // 0 = a 领先,1 = b 领先,0xFF = 无显著领先
    pub _pad: [u8; 3], pub n_samples: u32,
    pub lag_ns: i64,               // 峰值位置
    pub window_ns: i64,            // 可用窗口(峰值宽度)
    pub corr_peak_ppm: i32, pub confidence_ppm: i32,
    pub computed_at_ns: i64,
}
pub struct LeadLag { /* 每腿一个 (bucket, sign) 变动环与上一个 mid(last_mid: [Px; 2],0 = 无)+ 每个滞后一个相关累加器(增量维护) */ }
impl LeadLag {
    pub fn new(cfg: &LeadLagCfg) -> Self;
    pub fn mem_bytes(cfg: &LeadLagCfg) -> u64;                      // 上界:两条变动环 + L 个累加器
    pub fn snapshot_bytes(cfg: &LeadLagCfg) -> u64;                 // 上界:两条变动环 + 每腿上一个 mid(16 B)
    pub const LAYOUT_ELEMS: &'static [ElemDesc];
    /// 只记 mid 变动的符号。rx_ns 为 UTC ns:一律取 Now::utc_of(BboView.rx_tsc)(两腿同一时钟、同一校准对,确定);
    /// 不用 BboView.hw_rx_ns(PHC 域且可能退化为软件戳,口径见 SDK 时间轴)。rx_tsc = 0(未知,SDK BboView)时调用方不调用。
    pub fn observe(&mut self, leg: u8, rx_ns: i64, mid: Px);
    pub fn estimate(&mut self, now_ns: i64) -> Option<LeadLagEst>;  // 每 period_ms 调一次(先按 now_ns 滑出过期变动);样本 < min_samples → None
    pub fn dropped(&self) -> u64;                                   // 因超过 max_changes 丢弃的变动累计数(不进快照)
    pub fn snapshot(&self, w: &mut SnapshotWriter);                 // 写两条变动环与每腿上一个 mid(原始样本);累加器在 restore 时由环重算
    pub fn restore(&mut self, r: &mut SnapshotReader) -> Result<(), RestoreError>;
}
```

### 输出与 Advice 载荷

- 本库的输出结构(`FundingSummary`、`SettleRec`、`LeadLagEst`,以及 `PercentileStats::value` 的 `Rate`)都是整数、`#[repr(C)]` 定长,策略可以把它们原样放进自己的 Advice 载荷。**Advice 的 `local_kind` 与载荷布局由各策略自定**,没有全局登记;编码与长度上限见 [hft-strategy-sdk](../hft-strategy-sdk/README.md) 与 [hft-event](../hft-event/README.md)。
- f64 只出现在 `LeadLag` 内部;超越函数与 f64 → 整数转换只用 SDK 的 `model_math`(`libm` 实现、HalfEven、饱和;全仓唯一的浮点边界),本库不另写转换函数。
- "过期即保守"是策略规则:本库只提供 `Expiring<T>` 判断新鲜度,不规定过期后做什么。
- 快照:有状态构件的快照字节只含 engine_ns 或 UTC ns 时刻,不含 TSC;恢复后按 `Now.engine_ns` 继续(停机期间不计),换机或 TSC 重置不影响结果。快速部分构件的快照按代码块注释中的字段顺序逐字段写出(小端、无填充);各构件的 `restore` 出错(字节不足、配置非法)时返回 `Err` 且 self 不变(先解码到临时值再整体替换)。
- **Model 构件与 SDK 检查点纪律**([SDK Model 检查点](../hft-strategy-sdk/README.md#model-检查点)):`PercentileStats`、`LeadLag`、`FundingStats` 的快照含原始样本、决定存储解释的配置,以及后续采样要用的观测状态(`PercentileStats` 的末次观测、`LeadLag` 每腿上一个 mid、`FundingStats` 的检测状态),属于检查点的**原始样本节**;直方图、累加器等可精确重算的派生状态在 `restore` 时重算。所以构件在任意位置快照 → 恢复后继续喂同样的输入,状态与查询结果都与连续运行逐位相同,构件不需要路径状态节。各构件给出 `mem_bytes`/`snapshot_bytes` 上界与 `LAYOUT_ELEMS`,使用方把它们按 group 求和、拼进自己的 `Model::mem_bytes`/`snapshot_bytes`/`LAYOUT`。使用方由查询结果派生、按时刻更新的状态(分位带及其更新时刻、平滑窗等)写进自己 Model 的**路径状态节**,本库不提供。
- **结构参数与按 group 重置**([SDK 参数变化时的 group 重置](../hft-strategy-sdk/README.md#参数变化时的-group-重置)):使用方在 `Model::on_params` 中按新参数为每个 group 构造 `PctConfig`,`!stats.config().layout_eq(&new)` → 该 group 的 `PercentileStats` 以新配置重建(空)并计入返回的 `ModelParamEffect`;只有 `fill_max_ms` 变化 → 原地更新,不重置。映射到 [存储解释] 字段的策略参数在 schema 里标 `structural`。

### 参数

本库没有自己的配置文件与默认值。下表是各构件的参数结构;**取值由使用它的策略在自己的参数 schema(`ParamSchemaDesc`)里定义并给出默认值**,在 `on_params` 时换算成 `*Ns`(有状态构件)或 `*Tsc`(`DirtyFilter`)形式。

| 结构 | 字段 | 类型 | 含义 |
|---|---|---|---|
| `CostLine` | `items[≤ 8]`、`rt_floor` | `CostItem`、Rate | 成本项与往返下限;手续费项建议用 `fee_round_trip` 从 refdata 求 |
| `ConfirmParams` | `min_ms`、`min_updates`、`min_slow_updates`、`min_slow_span` | ms、u16、u16、u8 | 0 = 不要求该项;全 0 = 不确认(第一次成立即 `Confirmed`) |
| `DirtyParams` | `stale_fast_ms`、`stale_slow_ms`、`max_abs_spread`、`junk_after` | ms、ms、Rate、u16 | 0 = 关闭该项(`max_abs_spread ≤ 0` 同样关闭);CEX 腿的 `stale_*_ms` 是**连接静默上限**(不是报价不变的时长),外部报价腿是最近报价的最大年龄 |
| `ThrottleParams` | `cooldown_ms`、`max_consec_fail`、`fallback_ms`、`hard_rearm_ms` | ms、u8、ms、ms | `max_consec_fail = 0` = 不熔断 |
| `VwapParams` | `book_frac`、`max_levels` | Rate、u8 | `max_levels ≤ 64` |
| `PctConfig` | 见上 | — | 标 [存储解释] 的字段对应的策略参数是结构参数(`FieldDesc.structural`);内存见[延迟与资源契约](#延迟与资源契约) |
| `LeadLagCfg` | `bucket_ns`、`max_lag_ns`、`window_ms`、`period_ms`、`min_samples`、`max_changes` | — | 标定方法见开发备忘 |

## 实现说明

<details><summary>实现要点</summary>

### SpreadCalc 与 CostLine

- `buy = bid_fast.ratio(buy_px_slow)`,`sell = ask_fast.ratio(sell_px_slow)`(hft-types,i128 中间积,HalfEven)。两个价差的分母都是慢侧价格,这是现有引擎的价差定义,所有策略统一。
- 慢侧价格若已包含某些费用(例如外部报价按数量给出、已含池子费与冲击),对应成本项只置 `ROUND_TRIP`(计入开仓成本线)而不置 `OUT_OF_QUOTE`;CEX 腿的手续费不在 BBO 里,置 `ROUND_TRIP | OUT_OF_QUOTE`。"哪一项在报价里"由策略决定,本库只按位求和,避免重复计算。
- `fee_round_trip`:费率经 SDK `RefView::try_inst`/`try_fee` 取自 refdata 的 `FeeSchedule`(`try_inst` 为 None → `UnknownInst`,`try_fee` 为 None → `NoFeeTier`)(按账户费率档;使用折扣代币时的费率已在该档中配置)。[`VenueCaps.fee_asset_rule`](../hft-refdata/README.md) 只决定手续费扣哪个资产、影响成交净量(`FillDelta.net_qty`,归 [hft-ledger](../hft-ledger/README.md)),**不改变费率**,所以本库不读它。
- `CostLine::set` 预算三个合计,`round_trip/open_floor/out_of_quote` 都是读字段。`FIXED` 是往返之外的加项,与 `ROUND_TRIP` 互斥(同置 → `BadFlags`),所以 `open_floor` 中每项至多计入一次;`OUT_OF_QUOTE` 可与二者之一同置。合计用 i128,超出 i64 → `Overflow`。

### TriggerPx(预计算触发价)

"快侧价越过慢侧推出的整数价位"是所有两腿吃单类判断的形状:

1. 慢侧事件到来:`buy_bid_fast = ceil_to_tick(buy_px_slow.mul_rate(θ_open))`,`sell_ask_fast = floor_to_tick(sell_px_slow.mul_rate(θ_close))`(`mul_rate` = px × (1 + r),i128 中间量)。精度统一 1e-9:θ 是 `Rate`,乘积在 i128 中精确,只在最后取整到 fast 的 tick 一次,**取整方向永远是更难触发的一侧**;不经过任何 1e-6 或 bp 中间量。
2. 快侧 tick:一次整数比较;不命中直接返回(绝大多数 tick 的全部策略开销)。
3. 命中后调用方才做 `DirtyFilter`、`ConfirmWindow`、深度与仓位计算。
4. 同一机制可用于其他"价格越过某价位"的判断(例如 mark 越过硬动作价位):价位由 Model 预算,快速部分只做整数比较。
5. 慢侧失效(`DirtyFilter` 判 Stale/Resync)时调用方 `invalidate()`,`hit_*` 恒为 false。`restore` 后在第一次慢侧更新前也保持 invalid(快照里的触发价依据的是重启前的行情;`TriggerPx::restore` 读回字段后置 `valid = false`)。

### ConfirmWindow

条件需在 ≥ `min_updates` 个独立更新、跨 ≥ `min_ms`、期间 ≥ `min_slow_updates` 次**慢侧**更新达标、且慢侧序号跨度 ≥ `min_slow_span`(外部报价为块数)才 `Confirmed`;任一次 `cond == false`、`DirtyFilter` 拒绝或一侧过期 → `reset`([07 §3](../../design/07-策略层.md)、[06 §3](../../design/06-链上执行层.md))。

1. `cond && Idle` → `since_ens = now_ens, n_updates = 1, n_slow = from_slow as u16, first_slow_seq = last_slow_seq = slow_seq` → `Pending`。
2. `cond && (Pending || Confirmed)` → `n_updates += 1; n_slow += from_slow; last_slow_seq = max(last_slow_seq, slow_seq)`。
3. 第 1、2 步之后判定:`now_ens − since_ens ≥ min_ns && n_updates ≥ min_updates && n_slow ≥ min_slow_updates && last_slow_seq − first_slow_seq + 1 ≥ min_slow_span` → `Confirmed`(调用方开火后 `reset`);已 `Confirmed` 的窗口在 `cond` 为真时保持 `Confirmed`。快照里的 `since_ens` 是 engine_ns,恢复后的计时不受 TSC 重置影响;但确认依据的是重启前的行情,所以策略在 `restore` 时对全部确认窗口 `reset`(本库的规则,策略测试覆盖)。
4. `!cond` → `reset`(回到 `Idle`)。
5. 计数饱和于 `u16::MAX`,不回绕。

### DirtyFilter

按顺序检查,第一个命中的原因返回:① `!fast.conn_ok` 或 `now − fast.live_tsc > stale_fast_ms` → `Stale`;② `!slow.conn_ok` 或 `now − slow.live_tsc > stale_slow_ms` → `Stale`;③ `fast.resync` 或 `slow.resync`(序号断裂、等快照)→ `Resync`;④ `|spread| > max_abs_spread` → `AbsSpread`;⑤ `no_quote_streak ≥ junk_after` → `Junk`。参数为 0 只关闭该项的比较(`stale_*_ms = 0` 时 `conn_ok = false` 仍判 `Stale`);`now < live_tsc` 时年龄按 0 计。

**安静 ≠ 过期**:CEX 腿的 `LegFresh` 一律由 `LegFresh::from_bbo` 构造,`live_tsc` 取 SDK `BboView.conn_live_tsc`(该 inst 的 BBO 成员连接最近一次活性证据的 `rx_tsc`;活性证据与聚合规则只在 [hft-sequencer 连接活性](../hft-sequencer/README.md#连接活性)定义),与引擎对冲腿、风险递减单、链接腿检查用的是同一判定(SDK `BboView::quote_live`)。一个 inst 的盘口几秒、几十秒不变,只要连接健康就不是过期;本库不提供"按该 inst 最后一次变化判过期"的口径。外部报价腿(按块产出)没有新报价才是真的过期,用 `from_quote`。每个原因由调用方各自计数(`Ctx::note` 定期汇总)。

### Throttle

`can_fire(k, now)` 的放行表(✗ = 该原因会拦):

| 动作类别 | `in_flight` | `cooldown` | `halted` | `backoff` | `hard_rearm` |
|---|---|---|---|---|---|
| `Open` | ✗ | ✗ | ✗ | ✗ | — |
| `Close`(常规减仓) | ✗ | ✗ | 放行 | ✗ | — |
| `Flatten` | 放行 | 放行 | 放行 | 放行 | ✗ |
| `Hard`(硬动作) | 放行 | 放行 | 放行 | 放行 | ✗ |

- 按表的列序检查,返回第一个命中的原因。截止类原因在 `now_ens < *_until_ens` 时命中(`now_ens` 等于截止时刻即放行);`in_fallback(now)` 同样是 `now < fallback_until_ens`,`can_fire` 不看回退期(是否回退期内不开仓由策略决定)。
- `halted` 只拦开仓;Flatten 与硬动作绕过 `cooldown` 与 `in_flight`。为防止每个 tick 重复发同一硬动作,同一 group 两次 Flatten/Hard 之间至少隔 `hard_rearm_ms`(只作用于 Flatten/Hard,不影响开平)。
- 限速退避只镜像引擎令牌桶的拒绝,避免在退避期产生注定被拒的意图;风险递减单在引擎侧有预留份额(`rr_reserve`,[hft-risk](../hft-risk/README.md)),所以 Flatten/Hard 不受策略侧退避影响。
- `on_fire(Open | Close)`:`in_flight = true`、`cooldown_until_ens = now_ens + cooldown_ns`;`on_fire(Flatten | Hard)`:只设 `hard_rearm_until_ens`。
- `on_outcome`:清 `in_flight`;`Failed`(交易所拒单、外部执行失败等**执行结果**)→ `consec_fail += 1`(饱和于 255),`max_consec_fail > 0` 且 `consec_fail ≥ max_consec_fail` → 返回 `RequestHalt`;`Filled`/`NoFill` → `consec_fail = 0`;`Gated` 既不计数也不清零;`Rollback`(两腿同发后反向成交)→ `fallback_until_ens = now_ens + fallback_ns`,返回 `EnterFallback`,`consec_fail` 不变。其余情况返回 `None`。
- 全部截止时刻是 engine_ns:重启后冷却、回退期、退避按停机不计继续,不会因 TSC 重新计数而提前放行或永久锁住。`in_flight` 随快照恢复,之后由恢复后的订单与目标终态经 `on_outcome` 清除(OMS 的 Unknown 收敛归引擎)。
- `halted` 只由 `set_halted` 改,策略在收到引擎 cap 变化事件时镜像;本库不直接请求 cap。

### ImpactModel::vwap

1. `top_qty ≥ qty / book_frac`(按 `top_qty × book_frac ≥ qty × 10^9` 在 i128 中比较)→ `Top(top_px)`;`book_frac ≤ 0` 时跳过本步。本步不成立且 `qty ≤ 0` → `Walked{vwap = top_px, levels = 1, impact = 0}`(不读逐档)。
2. 否则从顶档逐档累加 `Σ px_i × q_i`(最后一档只取剩余量)到满足 qty 或到 `max_levels`(> 64 按 64;为 0 时只做第 1 步);满足 → `Walked{vwap = Σ/qty(按吃单方向取整到更不利一侧:吃卖盘 Ceil、吃买盘 Floor), levels = 用到的档数, impact = |vwap.ratio(top)|(HalfEven)}`;不满足 → `Insufficient{avail = 已遍历各档量之和}`。
3. 中间积 i128;经 `BookRead::levels` 把至多 `max_levels` 档拷到栈上数组(≤ 64 × 16 B),不持有簿的引用。

### FundingStats

输入是同一 inst 的 `Funding` 事件(SDK `FundingView`:`rate`、`next_settle_ns`、`interval_s`、`prev_interval_s`)。时间用 UTC ns:结算时刻取事件里的 `next_settle_ns`,"现在"取调用方传入的 `Now.utc_ns`(`clock_ok = false` 时调用方不调用 `summary` 的时效判断,按 `STALE` 处理)。

1. **结算检测**:`f.next_settle_ns > last_next_ns` 且 `last_next_ns ≠ 0` → 把 `last_next_ns` 时刻记为一次结算,费率取跳变前最后一次观测到的 `last_rate`,写入 `FUNDING_RING`(256)项环形缓冲(环中已有同一 `at_ns` 的预热记录时替换它:实时优先),返回 `Settled`。若跳变幅度 ≥ 2 个周期(漏了中间推送;周期取本事件的 `interval_s`,为 0 时不判)→ 只记一条并置 `GAP`(无法得知中间各期费率)。`f.next_settle_ns < last_next_ns`(回退)的事件只更新 `last_event_ns`,返回 `Normal`;其余事件之后 `last_rate = f.rate`、`last_next_ns = f.next_settle_ns`、`last_event_ns = now_ns`。
2. **周期**:`interval_s` 取 `FundingView.interval_s`(sequencer 已按事件给出实际周期);为 0(该所不推)时在结算跳变上取观测值 `next_settle_ns − last_next_ns`(秒,按 60 s 的整数倍 HalfEven 取整)。上次看到的值为 0(尚无)时只记录、不算变化;新值 ≠ 上次看到的值 → 返回 `IntervalChanged{old_s, new_s}`(同一事件也检测到结算时返回 `SettledChanged{rec, old_s, new_s}`)、记 `interval_changed_at_ns = now_ns` 并在之后的 `summary.flags` 置 `INTERVAL_CHANGED`(调用方据此改日化口径、可选择 `Ctx::note` 告警)。控制命令 `VenueNotice{FundingInterval}` 走引擎事件路径([control-plane](../../docs/subsystems/control-plane.md)),本构件只从行情事件检测。
3. **汇总**:`summary(window_ns, now_ns, stale_ns)` 在环中取 `at_ns ≥ now_ns − window_ns` 的记录,算和、最差/最好(无记录时为 0)、正/负期数(费率为 0 的不计)、最近连续负期数(从最新一条往回数);`predicted = last_rate`,`predicted_daily = daily(last_rate, interval_s)`;`realized_daily = realized_sum × 86 400 × 10^9 / window_ns`(i128,HalfEven;`window_ns ≤ 0` → 0);`next_settle_ns = last_next_ns`;标志:`interval_changed_at_ns ≠ 0` 且 `≥ now_ns − window_ns` → `INTERVAL_CHANGED`;窗口内任一记录带 `GAP` → `GAP`;`last_event_ns ≠ 0` 且 `now_ns − last_event_ns > stale_ns`,或 `last_event_ns = 0` 而环非空 → `STALE`;`last_event_ns = 0` 且环为空 → `NO_DATA`;环已满且最旧记录的 `at_ns > now_ns − window_ns` → `TRUNCATED`(窗口长于环能保留的期数,调用方据此缩短窗口或按"样本不足"处理,不静默截断)。
4. "已结算费率 = 跳变前最后预测值"是近似,与交易所公布的结算费率是否一致需逐所核实(见开发备忘);账户实收资金费另有 `FundingSettle` 事件,归 [hft-ledger](../hft-ledger/README.md)。
5. **预热**:`prefill_settled` 把预热注入的历史结算(来自交易所资金费历史;只取 `at_ns` 与费率构成 `SettleRec`,载荷里的其他字段如结算周期由使用方处理,不进环)按 `at_ns` 有序插入环;与环中已有记录同一 `at_ns` 的忽略(实时检测优先)。插入后 `summary` 照常计入这些记录;`last_next_ns`、`interval_s`、`last_event_ns` 只由实时 `observe` 更新,所以第一次实时 `Funding` 事件之前 `summary` 带 `STALE`(已有记录,不再是 `NO_DATA`),就绪与否仍由调用方按标志判断。

### PercentileStats

1. **采样**(确定性):采样点 k 覆盖事件时间 `[k × sample_ms, (k + 1) × sample_ms)`,取该区间内**最后一次**观测的 (buy, sell)。观测跨入新区间时提交上一区间;中间空的区间,若相邻两次观测间隔 ≤ `fill_max_ms` 则用上一值补齐,否则记 MISSING(不进直方图,计入 `gaps`)。`ts_ns ≤ 0` 或早于末次观测的观测忽略;与末次观测同一 `ts_ns` 的替换其值。提交值与补齐值都是实时样本(覆盖该点的预热值);记 MISSING 的点若已有预热值则保留预热值。买卖两侧分别采样、分别建分布(调用方传入的就是两个方向的价差,不用 mid 代替)。
2. **存储**:环形缓冲长度 `L = max(windows_ms) / sample_ms`,每点存各侧的箱号(i32;MISSING 哨兵;`sides = 1` 时只存一侧)。样本 x 先夹到 `[lo, hi]`,箱号 = `(x − lo) / bin` 按 HalfEven 取整,箱中心 = `lo + 箱号 × bin`。每个 (窗口, 侧) 一个**两级直方图**:细箱 `n_bins = (hi − lo) / bin + 1` 个 u32 计数,块 = 256 个细箱,块计数另存。
3. **更新**:新样本对每个窗口的直方图 +1;对每个窗口,离开该窗口的那个旧样本(环中距今 `window / sample_ms` 处)−1。O(窗口数)。
4. **查询**:n = 该窗口该侧的样本数;目标秩 `r = max(1, ⌈q_ppm × n / 10^6⌉)`(`q_ppm > 10^6` 按 10^6);n = 0、`window_ix ≥ n_windows` 或 `sides = 1` 时查 `Sell` → `None`;先扫块计数累加到 r 所在块,再扫块内细箱;返回该箱中心值。O(块数 + 256)。误差 ≤ 半个箱宽(越界样本除外,`clamped` 计数)。
5. `rank(x)`:同样两级扫描求箱中心 ≤ x 的样本数,除以窗口样本数(ppm,HalfEven);`None` 的条件同第 4 条。`coverage(ix)`:`expected` = 该窗口的采样点数 `W = windows_ms[ix] / sample_ms`,`samples` = 其中有值的点数,`gaps = W − samples`(含首次观测之前的点),`clamped` = 其中任一侧落在端箱(箱号 0 或 `n_bins − 1`)的点数;`ix ≥ n_windows` 时四项全为 0;全部由环算出,所以快照恢复后逐位相同。
6. **预热**(对应 SDK 的[预热注入](../hft-strategy-sdk/README.md#预热注入)):`prefill` 的样本按采样点划分(同 `observe`),每个样本覆盖从 `ts` 所在采样点起的 `max(1, ⌊fill_ms / sample_ms⌋)` 个采样点(`ts` 对齐时即 `[ts, ts + max(fill_ms, sample_ms))`);对每个落在当前窗口内(以已提交的最新采样点与 `now_ns` 所在采样点中较晚者为窗口末端 E,窗口为最近 L 个点 `(E − L, E]`)且仍为 MISSING 的点,写入箱号并对包含它的各窗口直方图 +1,`gaps` 相应减少;已有值的点不动;调用之后环的末端为 E(各窗口随之滑动,与 `observe` 提交到 E 相同)。`observe` 提交的实时样本**总是**写入其采样点(若该点是预热值则先从直方图减去),所以"实时优先、预热只补空",最终状态与预热和实时样本的先后顺序无关。数据来源(collector、交易所 K 线、journal 归档)与载荷格式由使用方策略决定,本构件只保证"同样输入集合得同样状态"。
7. **快照**:`snapshot` 先写 `PctConfig`(决定箱号的解释),再写末次观测(`ts_ns`、`buy`、`sell`:恢复后下一次 `observe` 据它提交当前区间并按 `fill_max_ms` 补齐,与连续运行相同),再写环形缓冲(箱号;`n_bins < 65 535` 时每点每侧按 u16 编码,MISSING = `u16::MAX`,否则 i32);直方图是派生状态,`restore` 时由环重建(配置不合法、环中有 ≥ `n_bins` 的非 MISSING 箱号或字节不足 → `Err`)。快照里的配置与使用方新参数推出的配置 `layout_eq` 不成立时,使用方重置该 group(见[输出与 Advice 载荷](#输出与-advice-载荷)),不按新配置解释旧箱号。

### LeadLag

对同一资产的两个 venue:以 `rx_ns` 对 mid 变动取符号(+1/−1/0),按 `bucket_ns` 分箱;在滞后 `[−max_lag, +max_lag]` 上对 `window_ms` 滚动窗口算互相关。**增量维护**:只存非零变动;一腿新变动到来时,与对侧 ±max_lag 内的每个变动相乘、累加到对应滞后的累加器;变动滑出窗口时减去它当初贡献的乘积(同一对只在较晚者到来时加、较早者离开时减,窗口精确)。累加器是整数(符号乘积 ±1),只有 `estimate` 的归一化与显著度用 f64(`model_math`)。`estimate` 只做 O(L) 的峰值搜索,L = 2J + 1,J = ⌊max_lag_ns / bucket_ns⌋(`bucket_ns = 0` 按 1 处理,不 panic;取值合理性由使用方的 `check` 保证)。每 `period_ms` 输出(定义唯一,跨平台逐位相同):

- **变动与配对**:变动 = 该腿 mid 相对上一个 mid 的符号 s ∈ {+1, −1}(相等不记;每腿第一次观测只记 mid;`leg > 1`、`mid ≤ 0` 或 `rx_ns` 早于该腿最后一个变动的观测忽略,忽略时不更新上一个 mid);分箱 b(t) = ⌊t / bucket_ns⌋;`acc[j]`(j ∈ [−J, J])= Σ s_a × s_b,对窗口内满足 b(t_b) − b(t_a) = j 的全部 (a 的变动, b 的变动) 对。`estimate` 先删去 `t < now_ns − window_ms × 10^6` 的变动;某腿保留的变动数超过 `max_changes` 时在 `observe` 中丢最旧的一个(减去它的贡献,`dropped()` 加 1)。
- **归一化**:n_a、n_b = 窗口内两腿的变动数;`n_samples = min(n_a, n_b)`,`< min_samples` 或为 0 → `None`;c(j) = acc[j] / √(n_a × n_b)(f64)。
- **峰值**:j* = argmax c(j)(并列取 |j| 较小者,再取负者);`lag_ns = j* × bucket_ns`;`corr_peak_ppm` = c(j*) × 10^6;`confidence_ppm` = (c(j*) − μ) / σ × 10^6,μ、σ 为其余 L − 1 个 c(j) 的均值与总体标准差(σ = 0 时 c(j*) > μ 取 `i32::MAX`,否则 0);ppm 值经 `model_math::to_notional`(同为 × 10^6、HalfEven)取得并饱和到 i32;μ、σ 按 j 升序累加。
- **方向与窗口**:c(j*) > 0 且 j* > 0 → `lead_leg = 0`(a 先变、b 跟随),c(j*) > 0 且 j* < 0 → 1,其余(含 j* = 0)→ `0xFF`;`window_ns` = 包含 j* 且 c(j) ≥ c(j*) / 2 的最长连续段的 j 个数 × `bucket_ns`;`computed_at_ns = now_ns`。

时间戳是事件体里的接收时间(Input 记录的一部分),所以计算本身确定。快照写两条变动环与每腿上一个 mid(原始样本;恢复后下一次 `observe` 据它判变动符号,与连续运行相同),累加器在 `restore` 时按环重算(O(窗口内变动数 × 对侧 ±max_lag 内的变动数),只在 advisor 启动时)。

### 规划源文件

| 文件 | 职责 |
|---|---|
| `src/units.rs` | `*Params` → `*ParamsNs`(有状态构件)/ `DirtyParamsTsc` 换算辅助、百分比常量(`Rate` 本身来自 hft-types) |
| `src/spread.rs`、`src/cost.rs` | `SpreadCalc`、`CostLine`、`fee_round_trip` |
| `src/trigger.rs`、`src/confirm.rs`、`src/dirty.rs`、`src/throttle.rs` | 快速部分判断构件 |
| `src/impact.rs` | `ImpactModel::vwap` |
| `src/funding.rs` | `FundingStats` |
| `src/expiring.rs` | `Expiring<T>` |
| `src/model/pct.rs` | `PercentileStats`(两级直方图) |
| `src/model/leadlag.rs` | `LeadLag` |
| `tests/*.rs` | 单元、表驱动、属性测试与参考实现(每个用例的文件与函数名见 [TESTS.md](TESTS.md)) |
| `benches/` | 见延迟契约 |

</details>

## 延迟与资源契约

快速部分构件的开销计入分段 `decided_tsc − proc_tsc` 内的策略份额(整段预算见 [latency-budget](../../docs/latency-budget.md) 与 [hft-strategy-sdk](../hft-strategy-sdk/README.md));Model 构件在 advisor 线程,不计入 tick-to-trade。测量:Criterion,在[参考机](../../benchmarks/README.md#参考机与-ci)(与升级主机同型的按需租用实例)的隔离核上判定;dev2 与 CI 只做功能验证、不做延迟验收。

| 构件 | p50 | p99 | 基准名 |
|---|---|---|---|
| `SpreadCalc::buy` | ≤ 8 ns | ≤ 20 ns | `stratlib/spread` |
| `CostLine::open_floor`(读预算值) | ≤ 2 ns | ≤ 5 ns | `stratlib/cost_read` |
| `TriggerPx::update` / `hit_*` | ≤ 20 / 2 ns | ≤ 40 / 5 ns | `stratlib/trigger_update`、`stratlib/trigger_hit` |
| `ConfirmWindow::observe` | ≤ 5 ns | ≤ 15 ns | `stratlib/confirm` |
| `DirtyFilter::check` | ≤ 5 ns | ≤ 15 ns | `stratlib/dirty` |
| `Throttle::can_fire` | ≤ 3 ns | ≤ 8 ns | `stratlib/throttle` |
| `ImpactModel::vwap`(20 档) | ≤ 80 ns | ≤ 150 ns | `stratlib/vwap20` |
| `FundingStats::observe` / `summary`(256 期) | ≤ 15 / 500 ns | ≤ 40 / 1 µs | `stratlib/funding_observe`、`stratlib/funding_summary` |
| `Expiring::get` | ≤ 2 ns | ≤ 5 ns | `stratlib/expiring` |
| `PercentileStats::observe`(4 窗口) | ≤ 60 ns | ≤ 200 ns | `stratlib/pct_observe` |
| `PercentileStats::prefill`(一块 ≈ 500 个采样点,1 窗口两侧) | ≤ 20 µs | ≤ 50 µs | `stratlib/pct_prefill_chunk` |
| `PercentileStats::value` / `rank`(60,001 箱) | ≤ 3 µs | ≤ 10 µs | `stratlib/pct_query` |
| `PercentileStats::value` / `rank`(6,001 箱) | ≤ 0.5 µs | ≤ 1.5 µs | `stratlib/pct_query_small` |
| `LeadLag::observe`(±5 ms 内对侧 ≤ 50 个变动) | ≤ 1 µs | ≤ 5 µs | `stratlib/leadlag_observe` |
| `LeadLag::estimate`(L = 201) | ≤ 20 µs | ≤ 50 µs | `stratlib/leadlag_estimate` |

- 快速部分构件零分配、无系统调用、无共享可变状态;截止时刻的加法(`now_ens + *_ns`)饱和于 `i64::MAX`;每个结构 ≤ 128 B(`CostLine` 168 B 与 `FundingStats`(256 × 24 B 环 ≈ 6 KiB)除外;`Expiring<T>` 另加 `T`)。
- `mem_bytes`/`snapshot_bytes` 的上界公式与下面的估算同一口径(各构件 `benches/` 旁的单测断言实际分配 ≤ `mem_bytes`)。
- `PercentileStats` 快照大小 = 配置的逐字段编码 66 B(`SnapshotWriter` 按字段写出,无对齐填充;不是内存布局)+ 末次观测 24 B + 环 `L × sides × 2 B`(u16 编码时,`4 B` 时为 i32)。
- `PercentileStats` 内存 = 环 `L × sides × 4 B` + 直方图 `n_windows × sides × (n_bins + n_bins / 256) × 4 B`;查询耗时随箱数线性(块数 + 256)。例:4 个窗口、两侧、±30% 范围、0.001% 箱宽(60,001 箱)→ 直方图约 1.9 MiB,72 h 窗口、10 s 采样 → 环约 200 KiB;1 个窗口、两侧、0.01% 箱宽(6,001 箱)、168 h / 5 s → 环约 945 KiB、直方图约 47 KiB。使用方按 group 数在 Model 启动时一次分配,并在自己的 README 写出总量;箱宽是参数,内存紧张时用更粗的箱。
- `LeadLag` 缓冲:每腿 `max_changes` × 12 B 的变动环 + L 个 i64 累加器,启动时分配(例:每腿 60,000 个变动 ≈ 700 KiB)。

## 测试与验收

用例与验收见 [TESTS.md](TESTS.md)。

## 依赖

- 内部:`hft-types`(`Px`、`Qty`、`Rate`、`Tsc`、`Side`、id 类型)、`hft-refdata`(`InstMeta` 的 tick)、`hft-event`(`Level`)、[`hft-strategy-sdk`](../hft-strategy-sdk/README.md)(`BookRead`、`BboView`、`FundingView`、`Now`(`utc_of`)、`RefView`/`FeeView`、`SnapshotWriter`/`SnapshotReader`、`ElemDesc`、`model_math`)。
- 外部:无(直方图与相关计算自实现);dev 依赖 `proptest`、`criterion`。
- 禁止:任何 L1 引擎 crate(`hft-oms`、`hft-risk`、`hft-ledger`、`hft-exec`、`hft-book`)、`hft-sequencer`、`hft-journal`、`hft-rt`、任何 `strategies/*`;I/O、`std::time`、`HashMap` 迭代、`dyn Trait`;快速部分构件中的 f64(`clippy.toml` 对 `src/` 除 `src/model/` 外禁止 `f64`/`f32`)。

## 延伸阅读

- [07 策略层](../../design/07-策略层.md)(公共构件与策略包)、[13 R10 warm-compute 车道](../../design/13-设计评审与优化.md)
- [hft-strategy-sdk](../hft-strategy-sdk/README.md)(`Strategy`/`Model`、视图、时间轴、Advice 载荷)、[strategies/AGENTS.md](../../strategies/AGENTS.md)(策略包规则)
- Agent Note:[报价新鲜度 = 连接健康](../../.agents/notes/proposed/architecture/2026-10-03-quote-freshness-conn-health.md)、[引擎时间 engine_ns](../../.agents/notes/proposed/architecture/2026-10-03-engine-time.md)
- [hot-path-rules](../../docs/hot-path-rules.md)、[latency-budget](../../docs/latency-budget.md)
- 第八轮裁决:[round8-decisions](../../design/_work/round8-decisions.md)

## 已知限制与后续工作

- 波动闸门(短窗口价格波幅上限)不实现;需要时作为 `DirtyFilter` 的新原因加入(调用方提供波幅)。
- L3/队列位置模型不在本库。可选扩展:随先挂单后对冲类策略与 `hft-sim` 的挂单撮合一起引入(触发条件:决定启用挂单类策略)。
- 预热数据的来源与载荷格式不在本库:由使用方策略定义载荷、pipeline 插件生产、经 SDK 预热注入送达 Model,本库只提供 `prefill`/`prefill_settled`;没有检查点也没有预热时,依赖它的策略须按"样本不足"处理。
- `FundingStats` 只看行情推送;账户实收与结算周期公告分别归 hft-ledger 与控制命令 `VenueNotice`。

### 开发备忘

<details><summary>给实现者</summary>

- 各 core venue 的 `Funding` 推送在结算瞬间的行为(跳变前最后一次推送的费率是否等于交易所公布的结算费率;推送是否带结算周期字段):待核实(方法:在服务器上录一次跨结算时刻的行情 journal,与交易所资金费历史 REST 接口逐期比对;结论写入对应 `venue-*` README 与本节)。
- `LeadLag` 的 `bucket_ns` 与 `max_lag_ns`:待标定(方法:在服务器录行情 journal,按实测的 mid 变动间隔分布离线标定;本机连不上交易所)。
- `PercentileStats` 若 60,001 箱查询在 aarch64 上超预算,先把块大小从 256 调到 512 再测,不改接口。
- 实现顺序:units → spread/cost/trigger/confirm/dirty/throttle → vwap → funding → expiring → model/pct → model/leadlag → 属性测试与基准。
</details>
