---
description: "sequencer 在每笔成交、外部成交、资金费、余额与保证金报告、参考行情事件上调用它维护按 (account, venue) 的事件溯源权益数据(现金、现货库存、永续持仓、保证金)与按 (account, group) 的 PnL;同一份代码经 hft-py 给 Python 做盈亏报表"
kind: crate
tier: pure
scope: core
status: impl
---

# hft-ledger

## 概要

`hft-ledger` 拥有**权益与盈亏记账**:每 (account, venue) 的各资产现金、现货库存(按 group 归属、未归属或基线)、每 (account, inst) 的永续/现货持仓与成本、保证金数据(交易所报告值、按 mark 的估计、两种强平距离与最旧标记价时刻)、按估值规则(参数)得出的账户合计权益、每 (account, group) 的 PnL 分解与轮次账、USDT 换算率表;这些是 SDK `PortfolioView` 的数据源(视图类型与方法签名归 [hft-strategy-sdk](../hft-strategy-sdk/README.md),字段语义以本页为准)。它**只由事件驱动**:成交增量 `FillDelta`(sequencer 从 [hft-oms](../hft-oms/README.md) 的输出转换)、费用 `FeeDelta`(OMS 的 `FeeOut`)、外来成交 `ExternalFill`、`FundingSettle`(交易所实收或 sequencer 合成)、`BalanceReport`、`MarginReport`、参考行情与命令(`AdoptExchange`、`SetBalanceBaseline`、`RecordTransfer`、`WriteOffResidual`、模拟账户命令);交易所推送的仓位与余额只作初始化与校验;资金费的逐笔差异由对账的 `Funding` 条目自动校正入账(不进启动闸门,无历史的条目在比对窗口结束后按交易所金额入账,不需人工)。外来成交与资金费推送在本 crate 按键去重(持久,进快照),重订阅回放不会重复入账;对账 `AUTO_EXTERNAL` 合成成交的构造是本 crate 的函数(sequencer 与冷读者共用),其入账量记 `auto_held`,之后补推的真实强平/ADL 先抵扣它;资金费的结算时刻先经 [hft-types `funding_settle_norm_ns`](../hft-types/README.md#资金费结算时刻归一化) 归一化再比较。账本里的时刻一律是引擎时间 `engine_ns`(跨重启连续、停机不计),不存 TSC。现金按 (account, venue) 记:一个账户在同一交易所只有一个 venue(refdata `SameExchangeMarkets`,[hft-refdata](../hft-refdata/README.md)),所以不存在同一钱包在两个 venue 各记一份的情形,本 crate 不建模余额域。**引擎不计算"可用资金"、杠杆或资金占用率**:策略从 `PortfolioView` 自己算。调用方:[hft-sequencer](../hft-sequencer/README.md)(热进程,单写者)与 [hft-py](../hft-py/README.md)(Python 读 journal 重建同一本账)。它是 `pure`:无 I/O、无时钟、记账路径无 f64,所以热进程、回放与研究侧口径必然一致([07 §5](../../design/07-策略层.md))。

## 目录

- [使用本组件](#使用本组件)
- [实现说明](#实现说明)
- [延迟与资源契约](#延迟与资源契约)
- [测试与验收](#测试与验收)
- [依赖](#依赖)
- [延伸阅读](#延伸阅读)
- [已知限制与后续工作](#已知限制与后续工作)

## 使用本组件

```rust
pub struct Ledger { /* 见实现说明 */ }

impl Ledger {
    /// caps:EngineConfig 的容量段(`max_subs_per_inst` 决定归属候选表与持有者反向索引的行宽);容量在这里一次分配。
    pub fn new(rd: &RefData, cfg: &LedgerConfig, caps: &Capacities) -> Result<Ledger, LedgerConfigError>;

    // ── 事件入口(热路径,O(1) 或 O(该 inst 的持有者数 ≤ max_subs_per_inst))──
    // 时间参数 t 一律是引擎时间 engine_ns(SDK `Now.engine_ns`,i64;算法唯一定义在 event-model 时间戳语义);本 crate 不接触 TSC。
    pub fn on_groups_changed(&mut self, s: StrategyInstanceId, groups: &GroupTable);     // SetParams 应用后:实例 s 绑定的账户重建 (account, inst) → [(group, leg_ix)] 候选表
    pub fn set_cell_shadow(&mut self, a: AccountId, g: GroupId, shadow: bool);             // cell 有效模式进入/离开 Shadow(外来成交归属只在非 Shadow cell 中找)
    pub fn on_run_start(&mut self, run_start_ens: i64);                                    // 本次运行起点(当前 EngineClock.base_engine_ns):早于它的换算率、保证金报告与标记价视为过期
    pub fn apply_fill(&mut self, f: &FillDelta, t: i64) -> FillApplied;                   // CEX 成交增量;净量复算不一致时带 NetAdjust
    pub fn apply_spot_fill(&mut self, f: &FillDelta, notional: i128, t: i64) -> Result<FillApplied, SettlementError>; // 现货原始毛成交名义,单位 px_exp + qty_exp
    pub fn apply_settled_fill(&mut self, f: &FillDelta, settled: PerpCashSettlement, t: i64) -> Result<FillApplied, SettlementError>; // 永续成交与其已知现金结算额
    pub fn apply_fee(&mut self, f: &FeeDelta, t: i64) -> FeeApplied;                      // 晚到手续费(OMS FeeOut);扣基础资产时带 NetAdjust
    /// 强平、ADL、人工、交割、未知:去重 + auto_held 抵扣 + 归属 + 入账(见"外部成交");auto_recon = 对账 AUTO_EXTERNAL 合成的本体。
    /// None = 重复(已入账过),或已被 auto_held 全部抵扣。
    pub fn apply_external(&mut self, e: &ExternalFill, auto_recon: bool, t: i64) -> Option<ExternalApplied>;
    /// 本笔外来永续成交的完整现金事实;不接受未证明的 auto_held 重叠。
    pub fn apply_settled_external(&mut self, e: &ExternalFill, t: i64) -> Result<Option<ExternalApplied>, SettlementError>;
    pub fn apply_reconciled_external(&mut self, e: &ExternalFill, t: i64) -> Result<Option<ExternalApplied>, SettlementError>;
    pub fn validate_reconcile_batch(&self, batch: &ReconcileBatchView) -> Result<(), SettlementError>;
    /// AUTO_EXTERNAL 合成成交的唯一实现(纯函数):sequencer 复核通过后调用,冷读者经 journal 消费路径调用同一函数(见"外部成交"第 6 步)。
    /// engine_minus_exch = 引擎仓位 − 交易所仓位(有符号,≠ 0)。
    pub fn auto_external_fill(a: AccountId, v: VenueId, inst: InstId, engine_minus_exch: Qty, mark_px: Px, report_id: u64) -> ExternalFill;
    /// 冷读者:从 hft-oms 的 Note{OmsForeignFill} 构造 OMS 防御路径的外来成交(纯函数;其他码 → None;见"外部成交"第 7 步)。
    pub fn external_from_oms_note(n: &Note) -> Option<ExternalFill>;
    pub fn apply_funding(&mut self, f: &FundingSettle, t: i64) -> Option<PnlDelta>;       // 入账:PerSymbolPush 的实收(按 (inst, settle_ts) 去重,重复 → None),或 synth = 1 的合成
    pub fn apply_funding_push(&mut self, f: &FundingSettle, t: i64) -> FundingPush;       // BalanceOnlyPush 所的 INST_NONE 推送:去重后入钱包、记已入钱包额、与合成额配对校验
    pub fn synth_funding(&self, a: AccountId, inst: InstId, mark: Px, rate: Rate, settle_ts_ns: i64) -> Option<FundingSettle>; // 纯计算,sequencer 调用
    /// 对账报告中的一条 Funding 条目(交易所资金费流水;`inst = INST_NONE` 为 BalanceOnlyPush 所的钱包条目);now_ns = 该报告记录的事件时间(UTC)。
    /// 满足滞后与窗口条件且有差额时构造 AdjustDelta{kind: Funding} 并入账;无历史的条目先挂起,见"资金费对账校正"。
    pub fn reconcile_funding(&mut self, it: &FundingReconItem, now_ns: i64, t: i64) -> FundingRecon;
    /// 每组对账报告处理完后调用一次(该 (account, venue)):把已移出比对窗口的挂起项(NoHistory)按交易所金额入账。
    pub fn funding_window_sweep(&mut self, a: AccountId, v: VenueId, now_ns: i64, t: i64, out: &mut FixedVec<FundingRecon, 64>);
    pub fn on_ref_px(&mut self, inst: InstId, bid: Px, ask: Px, t: i64);                  // 换算率 + 现货估值
    pub fn on_mark(&mut self, inst: InstId, mark: Px, t: i64, touched: &mut FixedVec<(AccountId, VenueId), 4>); // 永续未实现、保证金估计、强平距离与最旧标记价时刻
    pub fn on_funding_interval(&mut self, inst: InstId, interval_s: u32);                 // 该 inst 的实际结算周期(`Funding.interval_s ≠ 0`);资金费时刻归一化用,见"资金费时刻归一化"
    /// 从快照起步后(sequencer 在各组件 restore 之后、处理任何 Input 之前调用一次):按 hft-sequencer 节 10 的每 inst 最新标记价与其 engine_ns
    /// 重建不进快照的派生缓存(持有者未实现缓存、保证金估计、强平距离、最旧标记价时刻);不改任何 PnL 累计。冷路径,O(持仓数)。
    pub fn restore_marks(&mut self, marks: &[(InstId, Px, i64)]);
    pub fn on_balance(&mut self, b: &BalanceReport, quiet: bool, t: i64) -> BalanceVerdict; // 首条 → 初始化;其后只校验
    pub fn on_margin(&mut self, m: &MarginReportView<'_>, t: i64);                        // 保存报告值,重算估计与强平距离
    pub fn on_day_roll(&mut self, t: i64);                                                 // 只重置当日 PnL 基线;日亏锁存在 hft-risk
    pub fn apply_adjust(&mut self, a: &AdjustDelta, t: i64) -> PnlDelta;                  // AdoptExchange 的仓位或余额条目、WriteOffResidual 的核销(Funding 类只经资金费对账校正产生)
    pub fn set_balance_baseline(&mut self, a: AccountId, v: VenueId, cmd_seq: u64, out: &mut FixedVec<AssetId, 64>) -> BaselineResult;
    pub fn apply_transfer(&mut self, tr: &TransferDelta, t: i64);                         // RecordTransfer、SimTransfer:资金流动,不计 PnL
    pub fn sim_reset(&mut self, a: AccountId, init: &SimInit, t: i64);                    // SimReset:只对 Sim 账户

    // ── 只读(PortfolioView 与风控视图的数据源)──
    pub fn venue_book(&self, a: AccountId, v: VenueId) -> &VenueBook;
    pub fn pos(&self, a: AccountId, i: InstId) -> &InstPos;
    pub fn inventory(&self, a: AccountId, v: VenueId, asset: AssetId, g: GroupId) -> Qty;
    pub fn margin(&self, a: AccountId, v: VenueId) -> &MarginState;
    pub fn equity(&self, a: AccountId) -> Equity;
    pub fn group(&self, a: AccountId, g: GroupId) -> &GroupBook;
    pub fn account(&self, a: AccountId) -> &AccountBook;      // day_pnl、held_notional
    pub fn take_upnl_changed(&mut self) -> Option<AccountId>; // 上次 on_ref_px/on_mark 改变了未实现的下一个账户(升序)
    pub fn rate(&self, asset: AssetId) -> RateView;           // 换算率与年龄
    pub fn portfolio(&self, a: AccountId) -> Portfolio<'_>;    // SDK `PortfolioRead` 的实现(sequencer 视图适配器的 `Views::portfolio` 返回它)
    pub fn funding_grid(&self, v: VenueId, inst: InstId) -> (u32, u32); // 资金费时刻归一化的 (interval_s, anchor_s);inst = INST_NONE 取该 venue 的最细网格
    pub fn snapshot(&self, w: &mut SnapshotWriter);           // 一次写完本 crate 的全部节:1、13、22(按此顺序;各组件的写出顺序归 hft-sequencer 快照)
    pub fn restore(r: &mut SnapshotReader, rd: &RefData, cfg: &LedgerConfig, caps: &Capacities) -> Result<Ledger, RestoreError>;
}

pub struct FillApplied { pub pnl: PnlDelta, pub net_adjust: Option<NetAdjust> }     // net_adjust:净量复算与 OMS 填写值不一致时的差额
pub struct FeeApplied  { pub pnl: PnlDelta, pub net_adjust: Option<NetAdjust> }
pub struct NetAdjust   { pub account: AccountId, pub venue: VenueId, pub group: GroupId, pub inst: InstId, pub delta: Qty } // sequencer 同一事件内交 OMS net_adjust 与执行层 on_leg_adjust
pub enum FundingPush   { Booked { check: FundingCheck }, Duplicate }                    // FundingCheck = Ok | Mismatch{residual}
pub struct PnlDelta    { pub account: AccountId, pub group: GroupId, pub d: PnlBreakdown } // 本次入账的增量(GROUP_NONE = 账户级);sequencer 据此调 risk.on_pnl
pub enum BalanceVerdict { Initialized /* 该资产首条报告 */, Reported /* quiet = false:只更新 rep_* */, Ok, Drift { asset: AssetId, diff: Qty /* 报告 − total */ } }
pub enum BaselineResult { Set { n_assets: u16, notional: Notional }, NoBalanceView /* 该 (account, venue) 尚无 BalanceReport */ }
pub struct RateView    { pub bid: Px, pub ask: Px, pub ens: i64, pub stale: bool }      // 换算率所用参考行情与其 engine_ns;stale = RATE_STALE
pub enum LedgerConfigError { TooManyAssets { venue: VenueId, n: u16 } }                // 某 venue 的资产数 > MAX_ASSETS_PER_VENUE
```

**journal 冷消费者 `JournalLedger`**(读 journal 的冷读者——reconciler 的引擎视图、hft-py——都用它,不另写入账实现;规则见"冷消费者"):

```rust
pub struct JournalLedger { /* 内含一个 Ledger、槽映射、当前 epoch、计数 */ }
impl JournalLedger {
    /// 从一条 StateSnapshot 起步:按本 crate 的节恢复 Ledger,按 hft-oms 节 2/12 恢复槽与墓碑水位,节 3/25 恢复 epoch 与引擎时钟。
    pub fn from_snapshot(r: &mut SnapshotReader, rd: &RefData, cfg: &LedgerConfig, caps: &Capacities) -> Result<JournalLedger, RestoreError>;
    /// 逐条喂 journal 记录(Input、Output、Meta 都可以;与入账无关的 kind 忽略)。同一事件的记录按 journal 顺序喂。
    pub fn apply(&mut self, rec: &RecordView<'_>) -> Result<(), JournalApplyError>;
    pub fn ledger(&self) -> &Ledger;                 // 持仓、库存、余额、PnL 的查询沿用 Ledger 的只读接口
    pub fn stats(&self) -> &JournalLedgerStats;      // { unmapped_fills, map_evicted, epochs_seen }
}
```

### 成交增量与净量

```rust
#[repr(C)]
pub struct FillDelta {                  // sequencer 由 OMS 的 FillOut 构造(外来成交走 apply_external);进程内类型,布局为草案,72 B
    pub account: AccountId, pub venue: VenueId, pub side: Side,
    pub class: u8,                       // Open=1 Reduce=2 Hedge=3 Flatten=4 Reversal=5(链上冲回)Adjust=6(AdoptExchange)External=7
    pub flags: u8,                       // 来自 OMS FillOut:PX_EST=1 LATE=2(墓碑迟到成交)OVERFILL=4 EXT_DEDUP=8(已扣除先前按外来成交入账的部分)
    pub leg_ix: u8, pub _pad: [u8; 2],
    pub inst: InstId, pub group: GroupId,
    pub qty: Qty,                        // 本次成交毛量(> 0);Reversal 时为被冲回的量,side 取原成交的反向
    pub net_qty: Qty,                    // 本次成交使基础资产持有量变化的绝对值(规则见下;永续 = qty)
    pub px: Px,                          // 增量均价
    pub fee: i64, pub fee_asset: AssetId,// fee_asset = ASSET_NONE:回报未带费用,之后由 FeeOnly 补
    pub _pad2: u16,
    pub slot: u32, pub gen: u32, pub _pad3: u32,
    pub seq: u64,
}
```

**净量规则(唯一定义)**:令 `base` 为 inst 的基础资产,`fee_base = fee`(若 `fee_asset == base`)否则 0。现货买入 `net_qty = qty − fee_base`;现货卖出 `net_qty = qty + fee_base`;永续 `net_qty = qty`。手续费扣哪个资产由 refdata `VenueCaps.fee_asset_rule` 预先说明(`Quote` | `ReceivedAsset` | `ReceivedAssetUnlessDiscount{token}` | `Configured(asset)`),回报里的实际 `fee_asset` 优先。现货持仓、库存、对冲量、平仓量一律按 `net_qty`;OMS 的 LegTracker 与执行层使用同一值。`net_qty` 由 OMS 在处理成交回报时按本规则填写(费用未随成交到达时先按 `net_qty = qty` 计,不预估);`apply_fill` 按同一规则复算,不一致时写 `Note{NetQtyMismatch}`(P1)、以复算值记账,并在 `FillApplied.net_adjust` 返回差额(`delta` = 该腿有符号持仓的补差 `(复算值 − OMS 值) × side.sign()`,与 OMS `net_adjust` 的 `leg.net += delta` 同一口径),sequencer 同一事件内交 OMS `net_adjust` 与执行层 `on_leg_adjust`——LegTracker 与账本不分叉。

`FeeDelta { account, venue, inst, group, side, fee, fee_asset, seq }`(sequencer 由 OMS 的 `FeeOut` 构造;槽、墓碑与 gen 不符的归属由 OMS 完成):手续费入 `fee_cex` 并从费用资产的现金扣除(扣哪一桶见[仓位与已实现](#仓位与已实现平均成本法)第 4 步);费用资产是该 inst 的基础资产时,库存与持仓按净量规则补差(买入 `−fee`、卖出再多减 `fee`,有符号持仓的补差买卖都是 `delta = −fee`),并返回 `FeeApplied { pnl, net_adjust: Some(NetAdjust { account, venue, group, inst, delta }) }`,sequencer 同一事件内交 OMS `net_adjust`(LegTracker 与对冲余数)与执行层 `on_leg_adjust`。无归属的费用只记账户级。

`apply_spot_fill` 使用 OMS 给出的原始增量名义结算现货现金,只在计价资产单位上取整一次;成本与分析用已实现仍用 `FillDelta.px`。它先验证品种存在且为现货、venue 与账户绑定、正数量、正价格和正名义;错误分别返回 `UnknownInstrument`、`NotSpot`、`VenueMismatch`、`AccountVenue`、`InvalidQuantity`、`InvalidPrice`、`InvalidNotional`,包括时间在内的状态不变。身份与去重由调用方提前完成;`notional` 不能由钱包差额倒推。

`PerpCashSettlement { asset: AssetId, amount: Qty }` 是已匹配到本次成交增量的实际已实现现金,按 `asset.qty_exp` 计,收入为正,不含手续费、资金费或划转。已知零额仍按零入钱包。`apply_settled_fill` 先验证品种存在且为永续、venue 与品种相符、账户绑定该 venue、结算资产为保证金/计价资产、`qty = net_qty > 0`、价格正数;失败返回 `SettlementError::{UnknownInstrument, NotPerpetual, VenueMismatch, AccountVenue, WrongAsset, InvalidQuantity, InvalidPrice}`,包括时间在内的全部状态不变。成交身份和去重由调用方在此之前完成,不能把余额差额当成结算事实传入。

### 权益数据(`PortfolioView` 的字段语义)

```rust
pub struct VenueBook {                   // 每 (account, venue)
    pub cash: FixedVec<CashEntry, MAX_ASSETS_PER_VENUE>,  // 每资产一条:计价/保证金资产与现货基础资产都在这里
    pub inv: FixedVec<InvEntry, MAX_INV_PER_VENUE>,       // 现货库存的 group 归属
    pub margin: MarginState,             // 只对永续 venue 有意义(VenueId 即产品线)
    pub summary: VenueEquityView,        // SDK 视图类型:每 venue 汇总行,随每次变化增量维护
    pub vflags: u8,                      // INIT=1(全部资产已初始化) STALE_RATE=2 DRIFT=4
}
pub struct CashEntry {
    pub asset: AssetId, pub bflags: u8,  // UNINIT=1 NON_STRATEGY=2(已登记基线)
    pub total: Qty,                      // 事件溯源:初始化值 + 成交(净额)+ 手续费 + 资金费 + 外部成交 + 划转 + 采纳
    pub baseline: Qty,                   // 非策略资产基线(SetBalanceBaseline),不归任何 group,默认不计入合计权益
    pub unassigned: Qty,                 // 基础资产中既非基线也不属于任何 group 的部分(可为负,见手续费扣桶规则;核销残余也进这里)
    pub rep_free: Qty, pub rep_locked: Qty, pub rep_ex_ts_ns: i64, pub rep_ens: i64,  // 最近一条 BalanceReport(校验与展示;rep_ens = 到达时的 engine_ns)
}
pub struct InvEntry { pub asset: AssetId, pub group: GroupId, pub qty: Qty }   // (asset, group) → 归属该 group 的现货库存
pub struct MarginState {
    pub has_report: bool,
    pub rep_margin_balance: Notional, pub rep_maint: Notional, pub rep_avail: Notional,
    pub rep_ratio: Rate,                 // 统一口径:维持保证金 / 保证金余额(各所换算在 venue-*)
    pub rep_ens: i64,                    // 报告到达时的 engine_ns
    pub rep_perp_notional: Notional,     // 报告到达时该 venue 的永续总名义 Σ|qty|×mark(维持保证金随名义放大的基准)
    pub items: FixedVec<MarginItem, 64>, // (inst, liq_px, adl_quantile)
    pub wallet: Notional,                // 保证金资产的事件溯源余额,USDT
    pub upnl: Notional,                  // Σ 永续未实现(按估值规则的永续价)
    pub est_margin_balance: Notional,    // wallet + upnl
    pub est_ratio: Rate,                 // 估计维持保证金 / est_margin_balance(无报告时 0)
    pub rep_liq_dist: Rate,              // 按交易所 liq_px 的强平距离(定义见"保证金数据";Rate::MAX = 无可用项)
    pub est_liq_dist: Rate,              // 估计的强平距离(同上;Rate::MAX = 无永续持仓)
    pub mark_oldest_ens: i64,            // 该 venue 永续持仓中最旧的标记价时刻(engine_ns;i64::MAX = 无永续持仓);hft-risk 据此判标记价断流
    pub lflags: u8,                      // LIQ_PX_MISSING = 1(有持仓但报告未给其 liq_px)
}
pub struct Equity { pub total: Notional, pub by_venue: FixedVec<(VenueId, Notional), 16>, pub valid: bool }
```

- **不变量**:基础资产 `total = baseline + unassigned + Σ inv(asset, ·)`;计价/保证金资产 `total` 不按 group 归属(账户在该 venue 的现金)。
- **永续持仓**:每 (account, inst) 的 `InstPos`(有符号数量、成本、已实现),入场价 = 成本 / |qty|,未实现按估值规则的永续价(默认 mark)。**账户级无 group 持仓** = `InstPos.qty` − Σ 各 group 该 inst 腿的净量(外来成交归属不到 group 或超出该腿减到 0 的部分、或残余被核销时产生);MarginStress L3 与 `Kill{flatten}` 对它的减仓归 [hft-exec](../hft-exec/README.md)。
- **合计权益**(USDT):Σ venue [ Σ 资产 价值(计价资产按换算率,基础资产按估值规则的现货价)+ Σ 永续未实现 ];是否计入 `baseline` 与 `unassigned` 由 `valuation` 参数决定。`valid` = 账户的每个 venue 都已初始化且所需换算率未过期;风控的日亏额度与账户名义上限读它([hft-risk](../hft-risk/README.md#当日亏损锁存))。
- 可卖现货量、可用保证金比例、资金占用等**派生量不在本 crate**:执行层与策略从 `inv`、`rep_free`、OMS 在途量与 `MarginState` 自己算。

**视图字段对照**(SDK 视图类型的写者是本 crate;方法签名以 [hft-strategy-sdk](../hft-strategy-sdk/README.md) 为准):

| SDK 视图 | 本 crate 的来源 |
|---|---|
| `CashView { free, locked }` | `locked = rep_locked`;`free = max(0, total − rep_locked)`(事件溯源的总额减交易所报告的冻结量) |
| `InventoryView { total, assigned, unassigned, baseline }` | `CashEntry.total`、`Σ inv(asset, ·)`、`unassigned`、`baseline` |
| `inventory_of(v, g)` | `Σ inv(·, g)`(该 group 在该 venue 的现货净量) |
| `PerpPosView { qty, entry_px, upnl, liq_px, adl_quantile }` | `InstPos.qty`、`cost / |qty|`、按估值规则的未实现;`liq_px`、`adl_quantile` 取该 venue 最近一条 `MarginReport` 的同 inst 条目(无则 0) |
| `MarginView` | `MarginState` 的报告值;`src_tsc = now.tsc_of(rep_ens)`(早于本次启动饱和为 0,由 sequencer 的视图适配器换算);`stale` 由视图适配器判定:无报告、`rep_ens` 早于本次运行起点、或超过 hft-risk 的 `margin_stale_ms` |
| `VenueEquityView`(每 venue 汇总行,增量维护) | `equity`、`quote_cash`、`spot_value`、`perp_upnl`、`margin_balance`(= `est_margin_balance`)、`flags`(`RATE_STALE`、`MARGIN_STALE`、`BALANCE_UNRECONCILED` = 最近一次余额校验有 `Drift`)、`as_of_tsc`(= `now.tsc_of` 最近一次变化的 engine_ns) |
| `equity_total()` / `Equity` | Σ `VenueEquityView.equity`;`Equity.valid` 见上 |
| `GroupPosView`(每 (account, group),增量维护) | 各腿净量、入场价、未实现;`flags.ENTRY_ADOPTED` = 该 group 的腿含 `AdoptExchange` 采纳量;`held_notional`、累计 PnL 摘要、`round_start_ens`(= `RoundBook.open_ens`)、`last_fill_ens`(engine_ns,可能早于本次启动) |

### 配置(`LedgerConfig`,`EngineConfig` blob 的 `ledger` 段)

| 名称 | 类型 | 默认 | 含义 |
|---|---|---|---|
| `ref_inst[asset]` | InstId | refdata | 资产 → USDT 换算用的参考 inst(如 USDC→`USDCUSDT` 现货、BNB→`BNBUSDT`) |
| `par_assets` | 资产集 | USDT | 恒按 1.0 计价的资产 |
| `stable_par_fallback` | 资产集 | USDC、USD1、FDUSD | 参考行情缺失/过期时按 1.0 并置 `RATE_STALE` |
| `rate_max_age_ms` | u32 | 60000 | 超过即 `RATE_STALE` |
| `valuation` | `Valuation` | `{ spot: Bid, perp: Mark, include_baseline: false, include_unassigned: true }` | 合计权益与未实现 PnL 的估值规则(`spot ∈ {Bid, Mid}`,`perp ∈ {Mark, ExitSide}`) |
| `round_dust_notional` | Notional | 5 USDT | 轮次开始/结束判定线(只用于[轮次](#轮次round);执行层与风控的尘埃线是 hft-exec `ExecConfig.dust_notional`,不是同一个量) |
| `balance_check_quiet_ms` | u32 | 2000 | 余额校验只在该 (account, venue) 此时长内无成交、无相关非终态单时进行 |
| `funding_tol_abs` / `funding_tol_rel` | Notional / Rate | 0.05 USDT / 0.5% | 合成资金费与推送/流水的容差(待标定) |
| `funding_recon_lag_ms` | u32 | 600 000 | 结算时刻早于"报告事件时间 − 本值"的资金费差额才自动校正(给推送与流水留到达时间) |
| `funding_recon_window_ms` | u64 | 172 800 000(48 h) | 自动校正与逐笔入账记录的比对窗口;更早的条目不再比对。无历史(`NoHistory`)的条目挂起到移出本窗口时按交易所金额入账 |
| `funding_hist_cap` | u32 | 4096 | 每个有永续市场的 (account, venue) 的逐笔资金费记录容量(窗口内;满时淘汰最旧并推进水位线,见下) |
| `funding_push_seen_cap` | u16 | 64 | 每个 `BalanceOnlyPush` 的 (account, venue) 记住的已入账推送 `(settle_ts_ns, amount)` 条数(推送去重) |
| `ext_dedup_cap` | u32 | 1024 | 每 (account, venue) 记住的已入账外来成交去重键条数(FIFO 淘汰) |
| `chain_accounting`、`reorg_resolve_ms` | enum、u32 | `Finalized`、30000 | 仅 feature `chain`(链上腿,optional) |

本表全部字段在 `EngineConfig` blob 的 `ledger` 段(随引导块以 `ConfigBlob{Engine}` 写进 journal)。本 crate 为该段提供的验证器(由 hft-refdata `EngineConfig::validate` 调用)对每个 `ref_inst` 调 `RefData::check_ref_inst`,不通过 → `RefInstNotSubscribed`(规则归 [hft-refdata](../hft-refdata/README.md))。reconciler 从 journal 的同一 `ConfigBlob` 读 `funding_recon_lag_ms` 与 `funding_recon_window_ms`,只对结算时刻落在 `[报告时刻 − window, 报告时刻 − lag]` 内的资金费出 `Funding` 条目(最近 `funding_recon_lag_ms` 内的结算不比对)。持有者反向索引与归属候选表的行宽是 `Capacities.max_subs_per_inst`(ECF1 段 3,语义归 [hft-sequencer](../hft-sequencer/README.md))。每 venue 的资产与库存容量不是配置,见[数据结构](#数据结构)。

## 实现说明

<details><summary>实现要点</summary>

### 数据结构

```rust
pub struct InstPos {                     // 每 (account, inst)
    pub qty: Qty,                        // 有符号:多为正;现货 = 净量口径的持有量
    pub cost: i128,                      // 持仓成本(报价资产定点,平均成本法)
    pub realized_quote: i128,            // 该 inst 已实现(报价资产)
    pub fees_quote: i64,
    pub last_fill_ens: i64,              // engine_ns
}
pub struct GroupBook {                   // 每 (account, group)
    pub pnl: PnlBreakdown,               // 累计(自账户启用)
    pub day: PnlBreakdown,               // 当日(UTC)
    pub round: RoundBook,
    pub held_notional: Notional,         // Σ |腿名义|,风控名义检查用
    pub chain_pending: FixedVec<PendingChain, 8>, // 仅 feature `chain`
}
pub struct RoundBook {
    pub round_id: u32, pub open_seq: u64, pub open_ens: i64,   // 当前轮次开始的 engine_ns(跨重启连续)
    pub tag: u32,                        // 当前轮次第一笔开仓意图的 tag(策略自定义,不透明)
    pub legs: [LegAgg; 4],               // 每腿:开仓 qty/notional、平仓 qty/notional、fee、gas
    pub pnl: PnlBreakdown,               // 当前轮次
    pub peak_notional: Notional,
}
pub struct AccountBook { pub day: PnlBreakdown, pub held_notional: Notional, pub upnl: Notional,
                         pub funding_residual: Notional /* 已入钱包的资金费 − 已归属到 group 的资金费,账户级 */ }

#[repr(C)]
pub struct PnlBreakdown {               // 全部为 USDT 的 hft-types::Notional(1e-6)
    pub price_realized: Notional,       // 买卖价差实现
    pub funding: Notional,              // 资金费(收为正;含合成)
    pub fee_cex: Notional,              // 交易所手续费(付为负)
    pub gas: Notional, pub bribe: Notional, // 链上腿(feature `chain`;关闭时恒 0)
    pub unrealized: Notional,           // 按估值规则的浮动
    pub adjust: Notional,               // AdoptExchange 采纳的人工调整(单列,不进价差)
    pub external: Notional,             // ExternalFill(强平、ADL、人工、交割)带来的已实现与费用(单列)
}
impl PnlBreakdown { pub fn net_realized(&self) -> Notional; pub fn net(&self) -> Notional; }
```

- 稠密下标:`(account_ix, inst_ix)`、`(account_ix, group_ix)`、`(account_ix, venue_ix)` 由 refdata 与 `Capacities` 给出;容量在 `new` 时分配,之后零分配。每 venue 的资产与库存条数是本 crate 的**编译期常量** `MAX_ASSETS_PER_VENUE` = 64、`MAX_INV_PER_VENUE` = 128(`FixedVec` 容量,不在 EngineConfig;refdata 加载时校验每个 venue 的资产数不超过它,超出 → `LedgerConfigError`)。
- 所有金额为定点整数:单 inst 内的成本与已实现用报价资产的 i128;汇总到 group/account 时换算为 USDT 的 `Notional`。换算 = 金额 × 汇率(i128 中间积),**向零截断,截断误差进 `rounding` 计数**。费用取整方向沿用 hft-types 的记账规则(支出 `Ceil`、收入 `Floor`)。
- inst → 持有它的 (account, group) 的稠密反向索引,行宽 `Capacities.max_subs_per_inst`;`on_ref_px`/`on_mark` 只更新这些。

### 仓位与已实现(平均成本法)

对成交增量 `(side, q = net_qty, p)`,令 `s = +1 买 / −1 卖`、`Q = pos.qty`:

1. 同向加仓(`Q == 0` 或 `sign(Q) == s`):`qty += s×q`,`cost += p×q`(`q` 是净量:扣在基础资产上的手续费不进成本,只在第 4 步按换算率记入 `fee_cex`——同一笔费用不同时体现为更高的单位成本与 `fee_cex`;晚到的基础资产手续费按平均成本移出同量,两条路径结果相同)。
2. 反向减仓且 `q ≤ |Q|`:移出成本 `removed = cost × q / |Q|`(i128 一次相除,`TowardZero`;`q = |Q|` 时恰为 `cost`,全平不留残余成本);`realized += sign(Q) × (p × q − removed)`(即 `(p − avg) × q × sign(Q)`,`avg = cost / |Q|`);`cost −= removed`;`qty += s×q`。`cost` 存绝对值(多空都 ≥ 0);`cost` 与 `realized_quote` 的指数是该 inst 的 `px_exp + qty_exp`(`Px × Qty` 的 i128 积本身,不取整)。
3. 穿越零(`q > |Q|`):拆成"平掉 |Q|"(规则 2)+"新开 q − |Q|"(规则 1)。
4. 手续费:`fee` 按 `fee_asset` 当时换算率入 `fee_cex`(折扣代币等非 USDT 资产照样换算),并从该资产的 `CashEntry.total` 扣除。**扣哪一桶**(保持基础资产不变量):计价/保证金资产 → 只扣 `total`;本 inst 的基础资产 → 按净量规则体现在该 group 的库存与持仓上(第 5 步);其他基础资产(如平台币折扣)→ 先扣该资产的 `unassigned`,不足部分扣 `baseline`(保持 `NON_STRATEGY`),仍不足则 `unassigned` 记为负并写 `Note{InvShortfall}`(P1);**从不动其他 group 的 `inv`**。
5. 现金与库存:现货每笔成交按原始毛成交名义换算到计价资产最小单位;`apply_spot_fill` 直接使用传入的 i128 名义,其他入口用 `p × qty`,买入支出 `Ceil`、卖出收入 `Floor`,损失精度时递增 `rounding`;独立于成本与分析用已实现的取整。买入 → 计价资产 `total` 扣款、基础资产 `total += net_qty`、`inv(base, group) += net_qty`;卖出反之(库存从该 group 的 `inv` 扣,不足部分从 `unassigned` 扣并写 `Note{InvShortfall}`);永续 → 已实现与费用进保证金资产 `total` 与 `MarginState.wallet`。
6. 已实现与手续费同时累加到 group 的累计、当日、当前轮次三处。

`apply_settled_fill` 仅以实际结算额替代第 5 步的永续现金已实现项,费用仍单独扣一次。数量、按 journal 顺序的平均成本、分析用已实现与轮次账沿用上述规则;这些分析量不是交易所钱包的结算来源。`apply_fill` 保留无结算事实时的既有计算语义。账户权益与强平距离仍使用本地成本估计未实现盈亏,不能把钱包逐单位一致外推为成本、未实现盈亏或强平价也与交易所一致。

`InstPos` 与库存**只由成交、外部成交、采纳与核销更新**(核销只改归属,不改 `InstPos` 与 `total`);`PositionReport`、`BalanceReport`(初始化之后)与 `ReconcileReport` 只做比较,差异交风控分级(单写者 + 事件溯源,[01 §4](../../design/01-总体架构.md))。

### 余额初始化、校验与基线

1. **初始化**:某 (account, venue, asset) 首次收到 `BalanceReport`(gw 会话建立时的余额快照;模拟账户由 sim-gw 按 `SimAccountCfg.initial_balances` 给出)且条目为 `UNINIT` → `total = free + locked`;计价/保证金资产直接为现金,其他资产记入 `unassigned`;写 `Note{BalanceInit}`。账户的全部 venue 都初始化后 `Equity.valid` 才可能为真。
2. **校验**:其后的 `BalanceReport` 只更新 `rep_*`;`quiet = true`(sequencer 判定:该 (account, venue) `balance_check_quiet_ms` 内无成交、相关 inst 无非终态单)时比较 `free + locked` 与 `total`,差 ≥ 该资产的 `AssetMeta.dust`([hft-refdata](../hft-refdata/README.md#数据结构))→ `BalanceVerdict::Drift{asset, diff}`。每次实际校验写 `Note{BalanceCheck}`(Ledger 的 n = 16),仅记录原始报告和 quiet 闸门通过的事实,不携带期望差额或结果;字段归 [Note 登记表](../hft-event/README.md#note-来源与码登记表)。首次初始化与 quiet=false 不写。漂移告警归 `Note{BalanceDrift}`,分级由 reconciler 报告 + hft-risk 完成,本 crate 不改账。
3. **基线**(`SetBalanceBaseline{account, venue}`):对该 venue 上**不属于任何 group** 的非计价资产,把条目改为 `baseline = 最近报告值(free + locked)`、`unassigned = 0`、`total = baseline`,置 `NON_STRATEGY`;差额记入 `adjust`;写 `Note{BaselineSet}`(v[] = 资产数、合计名义)。返回涉及的资产列表,sequencer 据此向 hft-risk 报告对账闸门的处置(`Disposition::Baselined`)。属于某 group 的资产与计价/保证金资产不受影响(需要 `AdoptExchange`)。

### 外部成交(`ExternalFill`)

`ExternalFill`(强平、ADL、人工、交割、未知;识别方法在各 `venue-*`)由 `apply_external` 去重、抵扣、归属并入账:

1. **去重**:键 = [hft-venue-proto 契约 22](../hft-venue-proto/README.md#实现方必须遵守的契约) 的 `trade_id > 0` 时 `(inst, trade_id)`,否则 `(inst, exch_order_id, fill_seq)`,在 (account, venue) 内比较。本 crate 对每 (account, venue) 保存最近 `ext_dedup_cap` 个已入账的键(FIFO 淘汰,进快照节 13 的 `ExtSeen` 行);键已存在 → 返回 `None`,不入账、不产出任何输出。gw 的 `TradeIdRing` 不进快照,进程重启或重订阅后交易所回放的同一笔成交在这里被挡住。sequencer 合成的两类 `cause = Unknown` 本体按同一规则去重:OMS 防御路径的 `ForeignFill` 带 `trade_id` 或以 `cum_filled` 为 `fill_seq`(键的取值归 [hft-sequencer](../hft-sequencer/README.md));对账 `AUTO_EXTERNAL` 的本体由 `auto_external_fill` 构造(见下),键为 `(inst, 0, report_id)`,同一报告组不会入账两次。
2. **`auto_held` 抵扣**:`auto_recon = true` 的本体入账后,`auto_held[(account, inst, side)] += qty`(进快照节 13 的 `AutoHeld` 行)。之后同一 (account, inst)、同方向、`cause ∈ {Adl, Liquidation, Unknown}` 且 `auto_recon = false` 的外来成交先抵扣:`off = min(qty, auto_held)`,`auto_held −= off`,只把 `qty − off` 按下面的步骤入账(手续费按入账比例 `fee × (qty − off) / qty`,i128,`Ceil`),并写 `Note{source = Ledger, code = LedgerAutoHeldOffset}`(P0,`v = [venue | account << 8, inst, side, qty, off, 抵扣后的 auto_held]`);`qty − off = 0` → 返回 `None`(这段仓位变化已由对账入过账,LegTracker、执行层与策略都不再变化),去重键照常记入。用户流断开期间漏掉、之后由交易所补推的真实强平/ADL 因此不重复入账。
3. **归属**:按 `on_groups_changed` 建立的 `(account, inst) → [(group, leg_ix)]` 候选表,**只在非 Shadow cell 中找**(有效模式为 Shadow 的 cell 不交易,由 `set_cell_shadow` 维护;一个账户只绑定一个策略实例,sequencer 的 `InstOwned` 规则保证同一 (account, inst) 至多被一个非 Shadow cell 引用):恰好一个候选 → 该 (group, leg_ix),**以该腿减到 0 为限**——成交使该腿已成交净量向 0 变化时,至多归属该净量的绝对值,超出部分记 `GROUP_NONE`(腿不越过零,LegTracker 不出现幻影仓位);零个 → `GROUP_NONE`;多于一个(防御:不应出现)→ `GROUP_NONE` 并在注记置 `AMBIGUOUS`。
4. **入账**:按 `class = External` 记一笔成交(净量按净量规则):持仓、库存、现金与保证金钱包照常更新;记 `GROUP_NONE` 的部分(含第 3 步的超出部分)现货记入 `unassigned`、永续记入账户级无 group 持仓;已实现与费用记入 `PnlBreakdown.external`(不进 `price_realized`)。
5. **输出**:`ExternalApplied { group, leg_ix, net_qty: Qty /* 有符号,归属到 group 的部分 */, none_qty: Qty /* 有符号,记 GROUP_NONE 的部分 */, notional: Notional /* |net_qty + none_qty| × px 折 USDT */ }`;写 `Note{source = Ledger, code = ExternalFill}`,级别按 `cause`:`Manual` P1,其余 P0(登记归 hft-event);`v = [venue | account << 8 | cause << 16 | aflags << 24, inst, side, qty, px, exch_order_id, group, leg_ix]`,`aflags`:`AMBIGUOUS = 1`、`AUTO_RECON = 2`(`auto_recon = true` 的本体)。sequencer 据此调 OMS `external_adjust`(LegTracker;`cause = Unknown` 时 OMS 记下 `exch_order_id` 以便之后的槽成交去重)、执行层 `on_external`、向策略投递 `EventRef::External`(两部分的处理归 [hft-sequencer](../hft-sequencer/README.md))。
6. **`AUTO_EXTERNAL` 合成与冷读者**:`auto_external_fill(a, v, inst, d, mark_px, report_id)`(`d` = `engine_qty − exch_qty`)构造 `ExternalFill{cause = Unknown, side = d > 0 ? Sell : Buy, qty = |d|, px = mark_px, fee = 0, fee_asset = ASSET_NONE, trade_id = 0, exch_order_id = 0, fill_seq = report_id}`,其余字段 0。这笔成交在 `ReconcileReport` 这条 Input 内由 sequencer 确定地合成,**不另写记录**([hft-sequencer](../hft-sequencer/README.md))。冷读者(reconciler 的引擎视图、hft-py)经本 crate 消费 journal 时:Input `ExternalFill` 按第 1–5 步入账;在同一事件(同一 `EVENT_END` 之内)读到带 `AUTO_RECON` 的 `Note{ExternalFill}`(表明 sequencer 的复核已通过)时,以注记的 `venue`、`account`、`inst`、`side` 与 `qty`(得出 `d`)、`px` 和该事件 Input 的 `report_id` 调同一 `auto_external_fill`,再 `apply_external(.., auto_recon = true, ..)`——函数与去重键相同,所以冷读者的账本与 `auto_held` 与热进程一致。不带 `AUTO_RECON` 的 `Note{ExternalFill}` 是热进程对已入账成交的记录(本体是 Input `ExternalFill` 或下一步的注记),冷读者不据它入账。
7. **OMS 防御路径的外来成交与冷读者**(`ForeignFill`,带成交的 `SLOT_UNRESOLVED` 回报与 `StaleGen`):sequencer 处理该 Input 时把 hft-oms 的 `ForeignFill` 构造成 `ExternalFill{cause = Unknown}`,按第 1–5 步入账(`auto_recon = false`),**不另写 Input**;hft-oms 在同一事件内写一条带足入账字段的 Output 类 `Note{OmsForeignFill}`(字段与 `v[]` 布局唯一定义在 [hft-oms 回报匹配与成交增量](../hft-oms/README.md#回报匹配与成交增量) 第 8 条)。冷读者在同一事件内按注记在 journal 中的顺序,对每条 `Note{OmsForeignFill}` 调 `Ledger::external_from_oms_note(&note)` 得到 `ExternalFill`(`cause = Unknown`,`inst = Note.inst`,其余字段按 hft-oms 的布局解出,未列出的字段为 0),再 `apply_external(.., auto_recon = false, ..)`。旧配置下该函数构造的 `ExternalFill` 与 sequencer 由 `ForeignFill` 构造的逐字段相等,去重键相同,所以冷读者与热进程的账本、去重环逐值相同;同一事件没有其他会改变该 (account, inst) 持仓的入账(一条回报只产生槽成交或 `ForeignFill` 之一)。严格结算配置的现金字段由冷消费者从原始 Input 和自身水位补足,不能只读注记入账。

`apply_settled_external` 校验永续品种、venue、账户绑定、正数量/价格与 `cash_qty = qty` 后,按第 1 步的键去重。缺失或部分覆盖返回 `MissingCash`,负覆盖、超量覆盖或无覆盖却带金额返回 `CashCoverage`;其余身份/数量/价格错误沿用 `SettlementError`。错误不改变时间、去重键或账本。重复键返回 `Ok(None)`,不推进时间;相同成交键的原始载荷一致性由协议保证,本去重环不保存载荷副本。归属可以分成 group 与 GROUP_NONE,完整 `cash_amount` 仅入钱包一次,不按数量拆分现金;费用仍单独入一次。需要抵扣 `auto_held` 而缺少该部分现金事实时返回 `ExternalOverlap`,不做数量比例猜测。

**与本系统订单成交的去重**:gw 对成交按契约 22 的键去重,同一笔只报一次;无法归属到槽的成交 gw 先暂存 `gw.unmatched_hold_ms`(默认 5 000,[hft-venue](../hft-venue/README.md))仍无法归属才发 `ExternalFill{cause = Unknown}`;`cause = Unknown` 的外来成交之后若仍被 `QueryResult` 或迟到回报认定为本系统订单的成交,OMS 在产出 `FillOut` 时扣除已按外来成交入账的量并置 `EXT_DEDUP`([hft-oms](../hft-oms/README.md)),本 crate 收到的 `FillDelta` 已是去重后的量;先按外来入账的那部分 PnL 留在 `external` 列。OMS 防御路径的 `ForeignFill` 由 sequencer 构造成 `ExternalFill{cause = Unknown}` 走同一入口(同样过第 1 步去重)。

`PERP_CASH` 对账用原始成交,不采用标记价估算。`validate_reconcile_batch` 在全组处理前核对完整现金覆盖、账户/品种/方向、费用资产、原始成交键、每品种凭据数量之和与报告差量,已入账键和批内重复键不得解释新差额。缺凭据、错误方向、重复键、容量不足均失败。`apply_reconciled_external` 只入账一次,按原始键保留待确认凭据,与普通去重环分开;每 (account,venue) 上限为 ext_dedup_cap,启动预分配,满时 ReceiptCapacity,不能淘汰。

迟到 ExternalFill 命中待确认键时,按执行身份(venue、account、inst、订单号、trade_id;无 trade_id 时含 fill_seq)与金额字段(数量、价格、方向、费用、费用资产、现金覆盖/金额)比较;交易所时间 `ex_ts_ns` 与 `rx_tsc` 是送达事实,REST 凭据与 WS 推送可以不同,不参与比较;Unknown、ADL、Liquidation 之间的原因细分不改变资金身份。`apply_settled_external` 遇到不一致时不入账、不消耗待确认凭据、不推进时间,返回 `Ok(None)` 并写 P0 `Note{ReceiptMismatch}`(`v[5]` 位图:数量 1、价格 2、费用 4、费用资产 8、现金 16、方向 32、身份 64、原因 128,常量 `receipt_field`),不停机,由对账与人工裁决;`apply_reconciled_external` 对同键异额的新凭据仍返回 ReceiptConflict(批量路径在预检阶段已拒绝重复键)。完全匹配只减少该笔 auto_held,不再次动钱包、仓位或费用。不同键照常入账,即使同方向仍有待确认记录;仅有旧数量而无原始凭据的重叠仍报 ExternalOverlap。SimReset 清除本账户待确认凭据。普通去重环绕回不淘汰待确认记录,确认后把原始键重新放回普通环。

防御成交验证由 [LED-080](TESTS.md#led-080-oms-防御路径的外来成交冷读者账本与热进程逐值相同)约束,严格原件补账验证由 [LED-081](TESTS.md#led-081-auto_external冷读者账本与-auto_held-与热进程逐值相同)约束。真实 PairOnFill 开平仓、用户流缺口与迟到确认的场景、种子入口和非空观察要求归 [testkit 世界模型](../hft-testkit/README.md#dst-的世界模型),测试证据不能替代生产历史来源验证。

### 冷消费者(`JournalLedger`)

默认关闭的 `testkit` feature 提供只读 `JournalLedger::observed_notes()`。它保留最近一次成功消费记录时由冷账本自身产生的注记,上限为现有 `NOTE_BUF`,下一条成功记录覆盖;不复制 journal 的注记答案。诊断调用方须拒绝 `notes_dropped() != 0`,不能把溢出当作空列表。生产构建不含该缓冲,不改变消费结果、journal schema 或快照格式。

`FillDelta`/`FeeDelta` 不写 journal(它们是 OMS 处理 `OrderEvent` 的同一事件内的派生量),冷读者按下表从 journal 记录重建同样的调用,得到与引擎相同的持仓、库存与余额:

恢复 OMS 节 2 时接受行版本 1–3;版本 1 以 `filled` 初始化费用水位,版本 2/3 读取 `fee_cum`,版本 3 读取现金覆盖量与累计额。节 12 版本 1–5 的墓碑保留完整订单身份,版本 2–5 读取现金水位,版本 5 读取墓碑费用水位 `fee_cum`(1–4 取 `filled`);版本 3/4 的 ForeignCash 行按 [OMS 格式](../hft-oms/README.md#快照行格式节-2312草案实现时以代码与编译期断言为准)恢复外来累计结算与原始来源,版本 4 的 ForeignReceipt 独立校验并恢复单笔金融事实;缺来源、重复与非法原件拒绝恢复,旧版不补造原件。旧版现金水位补零。`PERP_CASH` 下已成交量必须全部覆盖,否则拒绝恢复。节 3 版本 1 提供当前 epoch,节 25 版本 1 提供时钟;首次 Boot 前的全零时钟行表示尚未开始,等待后续 EngineClock,零频率但其余字段非零的行拒读,关键帧后的新订单无须等下一次 EpochStart。未知版本返回 `VersionUnsupported`,重复身份或快照映射超容量返回 `BadValue`。

`apply` 遇到无法证明的入账返回 `JournalApplyError`,之后持续返回首个错误,调用方必须停止读取。`PERP_CASH` 的普通订单按完整身份及累计覆盖量计算现金增量,调用 `apply_settled_fill`;同水位不同金额、缺失/越界覆盖、减法溢出、找不到订单或重复登记身份都失败。FeeOnly 与查询后的补费不推进现金水位。失败成交不改账本或水位;未启用该能力的回报沿用本地计算。原始回报身份与登记品种不符时也明确报错。

`PERP_CASH` 的原始 ExternalFill 使用 `apply_settled_external`,错误映射为 `Settlement`;非零订单号保留原始来源;已有任一种来源的订单不能再由普通槽入账,SimReset 清除被重置账户的来源和水位。未映射 Fill/PartialFill 的完整累计结算先与冷端自己的外来水位相减,等待同事件 OmsForeignFill,逐字段核对身份、账户绑定的策略、方向、路径、数量、价格、费用及累计量,保留字段必须为零,通过后调用 `apply_settled_external`。注记不携带现金答案;重复、缺失、错误或跨事件注记报 ForeignEvidence 并锁存。每次正量入账后才推进水位,重复/旧累计量不记账。冷端来源与水位总数上限为 SLOT_MAP_CAP,满时 ForeignCapacity;缺原始凭据的 AUTO_EXTERNAL 或同订单跨来源重叠仍返回 `UnsupportedExternalCash`;未验证的外来抵扣不得按数量比例分配现金或退回估算。此边界限制开关的启用范围,见已知限制。

唯一已证明的跨来源重复是 [OMS 单笔原件契约](../hft-oms/README.md#回报匹配与成交增量) 的首次 Unknown 成交。冷端只从实际完整入账的原始 Input 保存金融事实,独立核对后来的累计前缀与全部金融字段,不从 OmsForeignFill 或热账本取答案。匹配时不记账、不等待入账注记,伪造该注记仍报 ForeignEvidence。原件的完全重复也在普通去重环绕回后保持无账务变化;不同成交键永久清除该订单的单笔证明,旧快照不能因后续重复生成证明。

| 记录 | 处理 |
|---|---|
| `Note{EpochStart}` | 记当前 epoch(`v[0]`) |
| `StateSnapshot` | 每个快照保留一份从自身已入账事实起步、清空非持久视图后继续消费后缀的冷副本。Recovery 只从该副本取得时钟、漂移标志与逐 venue 更新时间,其余派生金额从主冷账本的事实重算;不读新快照的仓位/余额覆盖累计状态。普通快照不重置主视图;副本最多一层,随下一个快照替换 |
| `OrderDecision{action = New}`(Output,`reject = 0` 且未置 `SHADOW`) | 槽映射登记 `(venue, account, epoch, slot, gen) → (inst, side, group)`;容量 = Σ 各表 `slots` × (`tombstone_gens` + 1)(OMS 配置),满时淘汰最早登记的一项(`map_evicted` +1) |
| `OrderEvent`(Input) | 按 `(ev.venue, ev.account, ev.epoch, ev.slot, ev.gen)` 查映射;命中后按 hft-oms 同一品种规则:`ev.inst = INST_NONE` 按映射品种处理,另一非零品种在 `PERP_CASH` 品种上报 `Identity`,其余忽略该回报;`cum_filled` 大于该槽已记累计量时,增量 = `cum_filled − 已记`,增量名义 = `avg_px × cum_filled − 已记名义`,手续费按 [hft-oms 回报匹配与成交增量](../hft-oms/README.md#回报匹配与成交增量) 与 [成交净量](../hft-oms/README.md#成交净量b4) 的同一口径,构造 `FillDelta`/`FeeDelta`(`PERP_CASH` 使用上述结算入口;现货普通槽将增量名义直接交 `apply_spot_fill`,墓碑仍用其增量价;`seq` = 该 `OrderEvent` 的 seq,与引擎由 OMS `FillOut`/`FeeOut` 带出的值相同;group 轮次的 `open_seq` 取它)调 `apply_fill`/`apply_fee`;`cum_filled` 不增(重复、乱序)→ 不入账,例外:`PartialFill`/`Fill` 的 `cum_filled` 大于该槽的费用水位(此前成交类回报的最大 `cum_filled`,以及任何随增量入账了非零费用的回报的 `cum_filled`;数量已由非成交回报带来,hft-oms 回报匹配与成交增量第 9 条)→ 只按 `FeeDelta` 入账其费用;映射缺失 → `unmapped_fills` +1,不入账(这类成交由 OMS 的 `Note{OmsForeignFill}` 另行给出,见下行) |
| `ReconcileBatch`(Input) | 体不可解码的批次引擎从不应用(只写 `ReconcileRejected`,无 `ReconcileCash`),冷读者同样忽略;可解码的批次仅从原始凭据重建;同事件必须有唯一 ReconcileCash 与每笔应用的 ExternalFill 注记,字段逐项核对,缺失/重复/跨事件证据报 ReconcileEvidence 并锁存;金额不从注记或后续快照取值 |
| `ExternalFill`(Input) | venue 或品种要求 `PERP_CASH` 时调 `apply_settled_external(e, t)`;其他情况调 `apply_external(e, false, t)` |
| `ReconcileReport` 中带 `AUTO_RECON` 注记的 `AUTO_EXTERNAL` 条目 | "外部成交"第 6 步 |
| `Note{OmsForeignFill}` | "外部成交"第 7 步(`external_from_oms_note`) |
| `FundingSettle`(推送与 `synth = 1` 的 Output) | 按 `VenueCaps.funding_settle` 调 `apply_funding` / `apply_funding_push` |
| `Note{Adjusted}`、`Note{FundingCorrected}`(未置 `NO_HISTORY`)| 还原 `AdjustDelta` 调 `apply_adjust` |
| `BalanceReport`、`MarginReport`、`Bbo`/`MarkPrice`、`Applied` 的 `RecordTransfer`/`SimTransfer`/`SimReset`/`SetBalanceBaseline` 命令、`Timer{Sys, DayRoll}` | 与 sequencer 相同的入口;BalanceReport 先以 quiet=false 更新报告字段,保留同事件原始报告 |
| `Note{BalanceCheck}` | 必须匹配同事件、尚未使用的已初始化 BalanceReport 的账户、venue、资产、free、locked、交易所时刻和引擎时刻;否则 BalanceEvidence 并锁存失败。用冷账本自身钱包重新执行 quiet=true 校验,得到漂移标志;EVENT_END 清除待匹配报告。没有该事实的旧日志只更新报告字段,不能证明漂移标志相等 |

`t` 取该记录的 `Now.engine_ns`(由记录头 tsc 与本段的 `EngineClock` 按 [event-model 时间戳语义](../../docs/subsystems/event-model.md) 计算)。**等价断言**:在每条 `StateSnapshot` 记录处,`JournalLedger` 的持仓、现货库存与余额(`pos`、`inventory`、`venue_book`)等于从该快照恢复的引擎 Ledger 的同一读数(属性测试与 DST 每个快照点检查)。

真实引擎 DST 的逐快照冷检查与未映射成交解释由 [hft-testkit](../hft-testkit/README.md#6-dst-框架) 执行,覆盖 `InstPos` 的时间字段和完整 `VenueBook`(现金归属及报告字段、保证金、权益汇总、标志)。持久字段与快照比较;未持久化的派生字段与同一完成边界的真实引擎只读视图比较,不能用恢复时清空的缓存代替参考。现货未映射计数包含重复交付,重复必须由已提交原件和同事件零增量注记共同解释。验收归 [LED-267](TESTS.md#led-267-dst-中每个快照点-journalledger-等于引擎-ledger)。

### 资金费

按 refdata `VenueCaps.funding_settle`:

| 方式 | 入账 | 校验与去重 |
|---|---|---|
| `PerSymbolPush` | 交易所推送的 `FundingSettle{inst, synth = 0}` 经 `apply_funding`:group `funding` 与保证金钱包 | 同一 (account, venue, inst, settle_ts) 已有推送入账(`FundHist` 记录带 `PUSHED`)→ 重复,返回 `None` |
| `BalanceOnlyPush` | sequencer 在结算时刻按"结算时刻持仓 × mark × 费率"合成每 inst 的 `FundingSettle{synth = 1}`(Output),`apply_funding` 只计入 group 的 `funding`(PnL 归属),**不动钱包**;交易所推送的 `FundingSettle{inst = INST_NONE}` 经 `apply_funding_push`:**钱包按推送实额入账**,并记入该 (account, venue, settle_ts) 的**已入钱包额**(`FundHist` 中 `inst = INST_NONE` 的记录,`booked += amount`) | 推送去重:同一 (account, venue) 最近 `funding_push_seen_cap` 条已入账推送中已有相同 `(settle_ts_ns, amount)` → `Duplicate`,不入账;配对校验:已入钱包额 − Σ 同 (account, venue, settle_ts) 合成额 = 残差,计入 `AccountBook.funding_residual`;`|残差| > max(funding_tol_abs, funding_tol_rel × |实额|)` → `FundingCheck::Mismatch`:推送一侧完成配对时由 `apply_funding_push` 返回、sequencer 写 `Note{FundingMismatch}`(P1);合成一侧后到、在 `apply_funding` 中完成配对时(该入口不返回校验结果),本 crate 直接写同一注记。推送与合成到达顺序任意,未配对项留在待配对表(≤ 16 条/(account, venue);满时淘汰 `settle_ts` 最早的一项,不对它做配对校验,它的钱包与归属差由对账条目校正);推送丢失、重复或误判重复都由对账的钱包条目校正(见下节) |
| `RateOnly` | 同上合成,`apply_funding` 计入 group `funding` **且**计入钱包(估计) | 交易所流水由 reconciler 逐笔对照,差额经 `Funding` 条目[自动校正](#资金费对账校正) |

表中的 `settle_ts` 一律是[归一化](#资金费时刻归一化)后的值。`synth_funding(a, inst, mark, rate, settle_ts_ns)` 只做计算:`amount = −pos_qty × mark × rate`(空头在正费率时收入为正),`pos_qty` 取调用时刻的持仓;i128 中算出后取整一次到 `Notional`(收入 `Floor`、支出按绝对值 `Ceil`,hft-types 记账规则);`pos_qty = 0` → `None`(不产生合成记录);何时调用(结算时刻的 `Timer`)归 [hft-sequencer](../hft-sequencer/README.md#资金费合成)。

**逐笔入账记录**:每次资金费入账(`PerSymbolPush` 的推送实额、`BalanceOnlyPush`/`RateOnly` 的合成额、`BalanceOnlyPush` 推送的已入钱包额)同时在该 (account, venue) 的 `FundHist` 环里记一条 `FundRec { inst, group, hflags, settle_ts_ns, booked, corrected }`(`booked` = 入账额,`corrected` = 之后自动校正的累计额,`hflags.PUSHED` = 由交易所推送入账;`inst = INST_NONE` 的记录是 `BalanceOnlyPush` 所该结算时刻的钱包账);环按 `settle_ts_ns` 有序,窗口 `funding_recon_window_ms`,容量 `funding_hist_cap`。另存水位线 `hist_from_ns`:该 (account, venue) 自此时刻起的每一笔入账都在环里(首次启用 = 该 (account, venue) 首条 `BalanceReport` 初始化时,取该报告的 `ex_ts_ns`;环满淘汰最旧记录时取被淘汰记录的 `settle_ts_ns + 1`;从不含 `FundMark` 行的快照恢复时取恢复后该 (account, venue) 首次 `reconcile_funding` 的 `now_ns`——`restore` 不接触 UTC)。

### 资金费时刻归一化

推送(交易所事件时刻)、合成(结算边界)与交易所流水(reconciler 的 `Funding` 条目)给出的同一笔结算时刻可能差几百毫秒到几秒;本 crate 按 `settle_ts` 精确配对与去重,归一化只用 hft-types 的唯一实现 [`funding_settle_norm_ns(ts_ns, interval_s, anchor_s)`](../hft-types/README.md#资金费结算时刻归一化)(取最近的结算边界;本 crate 没有自己的归一化)。本节只定义本 crate 传给它的周期与锚(`funding_grid`):

- **周期表**:每 inst 一项 `(interval_s, anchor_s)`,`new` 时取 refdata `InstMeta.funding_interval_s`、`funding_anchor_s`;`on_funding_interval(inst, interval_s)`(sequencer 在每条 `interval_s ≠ 0` 的 `Funding` Input 上调用)改为实际周期。周期表不进快照:从快照起步后 sequencer 以节 10 的 `interval_s`(≠ 0)逐 inst 调 `on_funding_interval`(与 `restore_marks` 同时)。
- **取值**:inst ≠ `INST_NONE` → 该 inst 的项;`INST_NONE`(`BalanceOnlyPush` 的推送与钱包条目)→ 该 venue 全部永续 inst 中周期最短的一项(各 inst 的结算边界都落在它的网格上,所以与各 inst 的合成记录配对一致)。
- 本 crate 的入口(`apply_funding`、`apply_funding_push`、`reconcile_funding`、`synth_funding`)在比较、配对、去重与写 `FundHist`/`FundSeen`/`FundNoHist` 之前对传入的结算时刻按 `funding_grid` 调用它(已归一化的值再调一次不变);进快照的 `settle_ts_ns` 都是归一化后的值。reconciler 与 hft-py 用本 crate 重建账本(同样喂 `Funding` 事件),reconciler 归一化交易所流水时取同一 `funding_grid`。
- 依据:各所资金费在固定周期的结算边界结算,推送与流水相对结算边界的偏差远小于半个周期(待核实(方法:各所测试网持仓跨一次结算,抓推送原文与资金费流水的时间戳,统计与结算边界的偏差分布))。

### 资金费对账校正

资金费差额**只告警、不进启动闸门**(hft-risk 不对 `Funding` 条目分级,[对账分级](../hft-risk/README.md#对账分级全系统唯一一次);reconciler 报出的结算资产余额差已扣除同组 `Funding` 条目的差额,[reconciler](../../apps/reconciler/README.md)),账本按交易所流水自动收敛,不需要人工。对 `BalanceOnlyPush` 所,reconciler 出两类条目:每 (inst, settle_ts) 的**归属条目**(`engine_amount` = 合成入账额)与每 settle_ts 的**钱包条目**(`inst = INST_NONE`,`exch_amount` = 该结算时刻交易所流水各 inst 之和,`engine_amount` = 本 crate 的已入钱包额 `booked + corrected`;口径归 reconciler)。sequencer 在每条 `Funding` 条目到达时就调用本节入口,不等同组末条:

```rust
pub struct FundingReconItem {           // sequencer 从 ReconcileReport{item = Funding} 投影(布局归 hft-event)
    pub account: AccountId, pub venue: VenueId, pub inst: InstId, // inst = INST_NONE:BalanceOnlyPush 所的钱包条目
    pub settle_ts_ns: i64,              // = 报告的 exch_ts_ns(入口内归一化)
    pub exch_amount: Notional,          // 交易所资金费流水中该笔的金额(收入为正,以保证金/结算资产计)
    pub engine_amount: Notional,        // reconciler 按 journal 算出的引擎已入账金额(它也没有记录时为 0)
    pub report_id: u64,
}
pub enum FundingRecon { Matched, TooRecent, OutsideWindow, NoHistoryHeld, NotApplicable /* BalanceOnlyPush 所的 inst ≠ INST_NONE 条目(第 0 步) */,
                        Corrected { adj: AdjustDelta, pnl: PnlDelta } }   // 窗口结束后入账的挂起项也以 Corrected 返回
```

`reconcile_funding(it, now_ns, t)`,`now_ns` = 该报告记录的事件时间(由记录头 tsc 与段头校准对换算,回放确定):

0. `VenueCaps.funding_settle = BalanceOnlyPush` 的所只有账户级钱包条目(`inst = INST_NONE`,条目口径归 [reconciler](../../apps/reconciler/README.md#资金费比对));该所 `inst ≠ INST_NONE` 的条目 → `NotApplicable`(不动、不进挂起表、不写 Note)。
1. `it.settle_ts_ns > now_ns − funding_recon_lag_ms` → `TooRecent`(不动;下一组报告再看)。
2. `it.settle_ts_ns < now_ns − funding_recon_window_ms` → `OutsideWindow`(不动)。
3. `it.settle_ts_ns < hist_from_ns` → **无历史**:本账本无法确知这笔是否入过账(水位线之前)。登记或更新该 (account, venue) 的挂起表 `NoHistPend`(键 (inst, settle_ts_ns),≤ 64 条,后到的报告覆盖金额),写 `Note{FundingCorrected}`(P1,`flags = NO_HISTORY`,未入账),返回 `NoHistoryHeld`。窗口期内这笔差额由 reconciler 从余额条目中扣除,不进 `Recon` 与闸门。
4. `booked_total = booked + corrected`(环中无该 (inst, settle_ts) 的记录 → 0,并新建一条 `booked = 0` 的记录);`diff = exch_amount − booked_total`;`|diff|` < 结算资产的 `AssetMeta.dust`(资产级尘埃线,唯一来源 [hft-refdata](../hft-refdata/README.md#数据结构))→ `Matched`。
5. 否则构造 `AdjustDelta { kind: Funding, inst, asset = 结算资产, group = 记录中的 group(新建记录取当前非 Shadow 持有 group,无则 GROUP_NONE;钱包条目为 GROUP_NONE), qty = diff, settle_ts_ns, report_id }`,按 `VenueCaps.funding_settle` 与条目类型入账:
   - `PerSymbolPush`/`RateOnly` → 结算资产 `CashEntry.total` 与 `MarginState.wallet` 加 `diff`,该 group(或账户级)`PnlBreakdown.funding` 加 `diff`;
   - `BalanceOnlyPush` 的钱包条目(`inst = INST_NONE`)→ 结算资产 `CashEntry.total` 与 `MarginState.wallet` 加 `diff`,`AccountBook.funding_residual` 加 `diff`(钱包按交易所流水收敛;推送丢失、重复或误判重复都在这里校正);

   记录 `corrected += diff`;写 `Note{source = Ledger, code = FundingCorrected}`(P1;`v = [venue | account << 8, inst, settle_ts_ns, exch_amount, booked_total, diff, group, flags]`,`flags`:`NO_HISTORY = 1`(第 3 步,挂起未入账)、`NEW_RECORD = 2`(环中原无记录,如推送丢失)、`WINDOW_END = 4`(挂起项在窗口结束后入账,见下));返回 `Corrected`。

**无历史条目在窗口结束后入账**(`funding_window_sweep(a, v, now_ns, t)`,sequencer 在每组对账报告的末条到达后对该 (account, venue) 调一次):挂起表中 `settle_ts_ns < now_ns − funding_recon_window_ms` 的项——reconciler 此后不再报告它,它对余额条目的扣除也随之消失——按交易所金额对齐入账:入账额 `diff = exch_amount − engine_amount`(最近一次报告的值;reconciler 也无记录时即全额),按第 5 步的方式构造 `AdjustDelta{Funding}` 入账(归属条目与钱包条目按各自规则),写 `Note{FundingCorrected}`(P1,`flags = NO_HISTORY | WINDOW_END`),删除该项并返回 `Corrected`。若这笔其实早已入账(重复),此后的余额对账会以差额报出(Warn → `Recon`),由人工 `AdoptExchange` 收敛。挂起表满时结算时刻最早的一项立即按同一规则入账(不丢弃),`reconcile_funding` 这一次返回该项的 `Corrected`(新条目照常挂起并写注记)。

同一笔被重复报告时第二次 `diff` 为 0(`Matched`),所以与 reconciler 的轮次、报告在途重叠无关。全部 `FundingCorrected` 注记用登记级别 P1,以 `flags` 区分情形;是否告警归 [watchtower](../../apps/watchtower/README.md):`|diff| > max(1 USDT, 1% × |exch_amount|)`(`funding_diff_alert`)→ P1,它读 `Note{FundingCorrected}` 与报告条目。

### 保证金数据

- `on_margin(m, t)`:保存 `MarginReport` 的报告值与条目(`inst → liq_px, adl_quantile`,每 venue ≤ 64 条)、`rep_ens = t`,记下此刻该 venue 的永续总名义 `rep_perp_notional`,重算下列估计。
- `on_mark(inst, mark, t)`:更新持有该 inst 的 (account, venue) 的 `upnl`、`est_margin_balance`、`est_ratio`、`rep_liq_dist`、`est_liq_dist`、该持仓的标记价时刻与 `mark_oldest_ens`,把受影响的 (account, venue) 写进 `touched`,sequencer 随后调 `risk.on_margin`。
- **强平距离(唯一定义;分档、阈值与动作归 [hft-risk](../hft-risk/README.md#保证金分档marginstress),本 crate 只给数据)**。对该 (account, venue) 每个 `qty_i ≠ 0` 的永续持仓,`m_i` = 估值规则的永续价(默认 mark):
  1. **按交易所强平价**:`d_i = (m_i − L_i) / m_i`(多头)或 `(L_i − m_i) / m_i`(空头),`L_i` = 最近一条 `MarginReport` 中该 inst 的 `liq_px`,已越过时取 0;`rep_liq_dist = min_i d_i`。`L_i = 0`(交易所未给)的持仓不参与并置 `LIQ_PX_MISSING`;没有可用项 → `Rate::MAX`。
  2. **估计**:设全部永续持仓同时向不利方向变动同一比例 x,保证金余额降到维持保证金时的 x:`est_liq_dist = max(0, (MB − M) / (N + M))`,`MB = est_margin_balance`,`N = Σ |qty_i| × m_i`,`M = rep_maint × max(1, N / rep_perp_notional)`(维持保证金随名义只放大不缩小)。单个空头且维持保证金率不变时它就是精确的强平距离;对多头与多持仓偏保守。无永续持仓 → `Rate::MAX`;无报告 → 不计算,保持 `Rate::MAX`(hft-risk 以无报告判数据过期,不读它)。
  3. `est_ratio = M / MB`(`MB ≤ 0` 时 `Rate::MAX`),与报告比率同一口径(维持 / 保证金余额)。本节的比率(`d_i`、`est_liq_dist`、`est_ratio`)在 i128 中算出后按 `HalfEven` 取到 `Rate` 最小单位(1e-9)。
  4. **最旧标记价时刻** `mark_oldest_ens = min_i(该持仓最近一次 on_mark 的 t)`(无永续持仓 → `i64::MAX`;持仓尚无标记价 → 0);hft-risk 以它判"标记价断流 → L1"。
  5. 增量维护:每持仓的 `d_i` 与标记价时刻缓存;`on_mark` 只重算该 inst,当它原是最小者且变大时才重扫该 venue 的持仓(≤ 64 项);`N` 与 `MB` 按差量更新。
- 换算率、保证金报告与标记价早于本次运行起点(`on_run_start`)时,`RATE_STALE`/`MARGIN_STALE` 置位直到本次运行收到新数据(引擎时间不计停机,停机前的数据不能当新的用)。
- **派生量与快照**:各 inst 的估值价(最近 bid/ask、标记价与其 engine_ns、最近成交价)进快照节 13 的 `Px` 行,`restore` 按它重建持有者的未实现与名义缓存,所以恢复后的缓存与快照里的账户、group 合计一致(估值价缺失时缓存为 0 而合计不为 0,第一次重估会把整额当差量重复计入);`est_*` 与两种强平距离不进快照。从快照起步后 sequencer 调一次 `restore_marks`(用 hft-sequencer 节 10 的每 inst 最新标记价与其 engine_ns)补上更新的标记价并重建派生量,结果与连续运行逐值相同,因此从中途关键帧起步的验证回放与 hft-risk 的定档一致。

### 人工干预的采纳(`AdoptExchange`)

`AdoptExchange{account, venue, inst | asset, report_id, group?, entry_px?}`(升级命令,[control-plane](../../docs/subsystems/control-plane.md))让 sequencer 从该组已生效的 `ReconcileReport` 中取差额,构造(布局为草案,实现时以代码与编译期断言为准):

```rust
#[repr(C)]
pub struct AdjustDelta {                 // Position/Balance 由 sequencer 按 AdoptExchange 构造;Funding 由 reconcile_funding 构造;
                                         // WriteOff 由 sequencer 按 WriteOffResidual 构造;都不经 OMS(不建槽)
    pub account: AccountId, pub venue: VenueId,
    pub kind: u8 /* Position=1 Balance=2 Funding=3 WriteOff=4 */, pub px_src: u8 /* Cmd=1 Mark=2 Bid=3;Funding 与 WriteOff 为 0 */,
    pub inst: InstId,                    // Position/Funding/WriteOff:该 inst(Funding 钱包条目为 INST_NONE);Balance:该资产在该 venue 的现货 inst(无则 INST_NONE)
    pub asset: AssetId, pub _pad: u16,   // Balance:资产;Funding:结算资产;WriteOff:现货腿为基础资产,永续腿为 ASSET_NONE
    pub group: GroupId,                  // 目标 group;GROUP_NONE = unassigned / 账户级(Position 时默认该 (account, inst) 的持有 group);WriteOff:被核销的 group
    pub qty: Qty,                        // 有符号:交易所 − 引擎(Funding:资金费金额差,以结算资产计;WriteOff:多出的那一腿移出该 group 的已成交裸量)
    pub px: Px,                          // 入场价:命令给出则用之(px_src = Cmd),否则对账时刻 mark(永续)或 bid(现货);Funding 与 WriteOff 为 0
    pub report_id: u64, pub cmd_seq: u64,// Funding:cmd_seq = 0;WriteOff:report_id = 0
    pub settle_ts_ns: i64,               // Funding:该笔资金费的结算时刻(归一化后);其余 0
}                                        // 56 B
```

- `Position`:按平均成本法把 `qty` 记为一笔 `class = Adjust` 的成交。
- `Balance`:该资产 `total += qty`;`group ≠ GROUP_NONE` 时 `inv(asset, group) += qty` 且该现货 inst 的 `InstPos` 按 `px` 记成本(可用于采纳一条现货腿并带入场价);否则进 `unassigned`。
- 这两种都把产生的已实现部分记入 `PnlBreakdown.adjust`(不参与轮次的价差);写 `Note{source=Ledger, code=Adjusted}`;sequencer 以同量调整 LegTracker(`group ≠ GROUP_NONE` 时)并向 hft-risk 报告处置(`Disposition::Adopted`)。
- `Funding`:只由[资金费对账校正](#资金费对账校正)产生,计入 `PnlBreakdown.funding`(它就是资金费,不是人工调整)或只动钱包与残差(钱包条目),不动持仓、库存与 LegTracker,不报告处置(资金费条目不进闸门)。交易所侧真实的成交价与手续费若之后补到,以 `Annotate` 留痕,不再改账。
- `WriteOff`(`AdjustDelta::WriteOff`):只由 Admin 命令 `WriteOffResidual{account, group}` 产生(适用于静止的 group,`Broken` 或非 `Broken`;语义与前置条件归 control-plane,执行归 [hft-sequencer](../hft-sequencer/README.md#命令路径)):sequencer 按 OMS 残余核销(`write_off_residual`)返回的单条 `WriteOff{venue, inst, leg_ix, qty}` 构造——账本只动**多出的那一腿**(单腿,选腿规则与清零全部腿余数归 [hft-oms](../hft-oms/README.md#残余核销));`qty = 0`(无裸量、只清余数)时账不变,只写下面的注记。账本只把这条腿的已成交裸量 `qty` **从该 group 移到 unassigned**(现货进 `unassigned`,永续成为账户级无 group 持仓),交易所上的真实仓位不变,所以账本仍与交易所一致:现货腿 `inv(base, group) −= qty`、`unassigned += qty`(基础资产 `total` 不变;`qty > 0` 时至多移出该 group 实际持有的库存——卖出超过库存的部分早已从 `unassigned` 扣除,`qty < 0` 时从 `unassigned` 移入该 group);永续腿该 group 该 inst 的腿净量减 `qty`,`InstPos` 不变,这部分随之成为账户级无 group 持仓(MarginStress L3 与 `Kill{flatten}` 照常覆盖它)。不产生已实现 PnL、不改现金与成本;group 的 `held_notional` 与轮次按新的腿净量更新(残余低于 `round_dust_notional` 时轮次随之结束)。写 `Note{source = Ledger, code = ResidualWrittenOff}`(P1;`v = [account | venue << 8, group, inst, qty, notional, cmd_seq]`,`notional` 按当时估值价折 USDT)。不报告对账处置。

### 划转与模拟账户

- `TransferDelta{account, from_venue, to_venue, asset, amount, kind: Record | Sim}`:`RecordTransfer`(人工划转登记)与 `SimTransfer`(模拟划转)都只移动现金,不计 PnL;`Record` 时余额在到账前的差额由 reconciler 以 `ext_ref` 核对,本 crate 照登记移动。
- `sim_reset(a, init)`:只对 `AccountMeta.kind = Sim` 的账户;清空该账户全部持仓、库存、PnL、轮次、外来成交与资金费推送的去重记录与 `auto_held`,现金按 `init` 重置;写 `Note{SimReset}`。前置条件由 sequencer 校验([hft-sequencer 命令路径](../hft-sequencer/README.md#命令路径))。

### 未实现与当日 PnL(增量维护)

`on_ref_px`/`on_mark` 只更新持有该 inst 的 (account, group):`new_upnl = qty × (估值价 − avg)` 换算 USDT,`delta` 同步加到 `GroupBook.day.unrealized` 与 `AccountBook.upnl`;不遍历所有 group。`day` 在 `on_day_roll`(UTC 日切的 `Timer`)时清零(未实现以当时值为新基线)——这只是 PnL 基线;日亏熔断是 hft-risk 的锁存 cap,日切不清。`restore_marks` 只重建持有者的 `upnl` 缓存,不产生 `delta`。`on_ref_px`/`on_mark` 入口清空、重估中记下未实现有变化的账户(按账户号的位图,瞬态,不进快照),sequencer 随即以 `take_upnl_changed()`(账户号升序逐个取出)对这些账户调 `risk.on_pnl`——当日亏损熔断看含未实现的当日 PnL([hft-risk 当日亏损](../hft-risk/README.md#当日亏损锁存))。

### 轮次(round)

- 腿名义 = |该腿净量| × 该 inst 当前估值价(现货按 `valuation.spot`,永续按 `valuation.perp`)折 USDT。
- 开始:group 的各腿名义从全部 `< round_dust_notional` 到任一腿 `≥ round_dust_notional`。
- 结束:所有腿 `< round_dust_notional`(残余由策略或执行层处理,或被核销)。结束时写 `Note{RoundClosed}`:`Note.group` = 该 group,内容为各腿开/平均价与量、`PnlBreakdown`、持仓时长(不带策略 `tag`:成交增量与 OMS 槽都不带它)。入场/出场价差等策略口径的指标由策略的报表插件从各腿均价计算,不在本 crate。
- 成交、外来成交或基础资产费用使轮次结束时,先完成本次调用的数量、已实现与费用入账,再判定结束并生成注记。同笔费用计入该轮,结束后轮次累计清零,不带入下一轮(LED-274)。
- 均价按当前轮次的配对成交算,不用交易所 `entryPrice`(加仓再部分平仓会使交易所入场价偏离轮次口径)。

### USDT 统一计价

- 换算率只来自 journal 中的参考行情事件(`Bbo`),**不查外部 API**([07 §5](../../design/07-策略层.md))。每资产存 `(bid, ask, ens)`,换算率 = `Px::mid_floor(bid, ask)`(`par_assets` 恒为 1;基础资产在合计权益里的估值价另按 `valuation.spot`);`rate_max_age_ms` 超时(按 engine_ns:本 crate 没有时钟,以最近一次带 `t` 的入口调用的 `t` 为当前时刻,`t − ens > rate_max_age_ms` 即超时)或早于本次运行起点 → `RATE_STALE`,稳定币按 1.0 回退,其他资产沿用旧值并置 `stale` 位(报表显示,风控照用,`Equity.valid = false`)。
- 已实现一律按**事件发生时**的换算率入账(不回溯重估);未实现与合计权益按当前换算率随 `on_ref_px` 更新。

### 链上腿(optional,feature `chain`)

启用链上 venue 时使用;规则保留如下,外部 venue 协议见 [external-venue-protocol](../../docs/subsystems/external-venue-protocol.md)。

| `TxState` | 持仓(风控/策略看到的"暴露") | 会计(PnL、日报、轮次) |
|---|---|---|
| `IncludedOk`(≥ 2 源一致) | 立即计入(`amount_out`,对冲在同一事件) | 记入 `chain_pending`,不入已实现 |
| `Finalized` | 不变 | 从 `chain_pending` 转入;gas 与出价按终局事件时刻的换算率入账 |
| `IncludedReverted`(未曾 `IncludedOk`) | 不计入 | 只记 gas |
| `Dropped`(未曾 `IncludedOk`) | 不计入 | 无 |
| `Unknown` | 不变 | 不变 |

终局前重组:`Unknown{REORG}` → 暴露不回退、CEX 对冲不动,`chain_pending` 项标 `REORG_SUSPECT`、写 `Note{ChainReorg}`(P1);重新 `IncludedOk` → 按 `amount_out` 差额调整暴露;收敛为 `Dropped`/`IncludedReverted` → 按原 `amount_out` 冲回(`FillDelta.class = Reversal`)、写 `Note{ChainReorgNaked}`(P0);`REORG_SUSPECT` 超 `reorg_resolve_ms` → P0 + `Reorg` cap。本 crate 只信 `TxLifecycle` 的 `amount_in/amount_out/gas_cost`,**不从报价推断成交量**;链上腿成交量一律是基础资产数量(Buy 取 `amount_out`,Sell 取 `amount_in`)。

### 快照行格式(节 1、13、22;草案,实现时以代码与编译期断言为准)

`StateSnapshot` 的分节表与 `SectionHdr` 归 [journal-format](../../docs/subsystems/journal-format.md#快照记录结构);本 crate 定义节 1、13、22 的行(当前行版本:节 1 v2、节 13 无待确认凭据时 v8、有凭据时 v9,节 22 v2;恢复时 v8 的节 13 出现待确认凭据行 `AUTO_CASH` 即 `BadValue`)。全部 `#[repr(C)]`、小端、按所列键升序;i128 以 `[u64; 2]` 小端存放。`snapshot(&self, w)` 按节 1 → 13 → 22 一次写完。

```rust
// 节 1 Positions:每 (account, inst) 一行,只写 qty、cost、realized、fees、last_fill_ens 任一非零的项(last_fill_ens 进入 BalanceReport 的 quiet 判定,平仓后的持仓也须保留);键 (account, inst)
#[repr(C)] pub struct PositionRow {                                // 64 B = InstPos + 键
    pub account: AccountId, pub _pad: [u8; 3], pub inst: InstId,
    pub qty: Qty, pub cost: [u64; 2], pub realized_quote: [u64; 2], pub fees_quote: i64, pub last_fill_ens: i64,
}

// 节 13 LedgerGroups 与节 22 LedgerInventory:每行以 LedgerRowHdr 开头;所有 *_ens 字段为 engine_ns。
// 节 13 顺序 Account* → Rate* → Px* → (Group 行紧跟其 Pending 行)* → Margin* → FundPending* → (FundMark 行紧跟其 FundHist 行)* → FundNoHist*
//        → FundSeen* → ExtSeen* → AutoHeld* → AutoCash*;节 22 顺序 Cash* → Inv*
#[repr(C)] pub struct LedgerRowHdr { pub kind: u8 /* Account=1 Rate=2 Group=3 Pending=4 Cash=5 Inv=6 Margin=7 FundPending=8 FundMark=9 FundHist=10 FundNoHist=11 FundSeen=12 ExtSeen=13 AutoHeld=14 Px=15 AutoCash=16 */,
                                     pub account: AccountId, pub venue: VenueId, pub _pad: u8,
                                     pub id: u32 /* Rate/Cash/Inv:asset;Group/Pending:group;FundHist/FundNoHist/ExtSeen/AutoHeld/Px:inst;其余 0 */ } // 8 B
// Account:    { day: PnlBreakdown, held_notional, upnl, funding_residual }                                     88 B
// Rate:       { bid: Px, ask: Px, ens: i64, rflags: u8 /* RATE_STALE=1 */, _pad: [u8; 7] }                     32 B
// Group:      { pnl, day: PnlBreakdown, held_notional, peak_notional, round_id: u32, tag: u32, gflags: u8 /* REORG=1 STALE=2 ADOPTED=4 ROUND_OPEN=8 */,
//               _pad: [u8; 7], open_seq: u64, open_ens: i64, round_pnl: PnlBreakdown, legs: [LegAgg; 4],
//               last_fill_ens: i64, leg_inst: [u32; 4], leg_net: [Qty; 4], leg_cost: [[u64; 2]; 4] }              552 B;各腿已成交净量与成本随行持久
// Pending:    { intent_id: u64, block: u64, amount_in, amount_out, gas_cost, bribe: Qty, inst: InstId, pflags: u8 /* REORG_SUSPECT=1 */,
//               _pad: [u8; 3] }                                                                                  56 B(feature chain)
// Cash:       { total, baseline, unassigned, rep_free, rep_locked: Qty, rep_ex_ts_ns: i64, rep_ens: i64, bflags: u8, _pad: [u8; 7] } 64 B;键 (account, venue, asset)
// Inv:        { group: GroupId, _pad: u32, qty: Qty }                                                            16 B;键 (account, venue, asset, group)
// Margin:     { rep_margin_balance, rep_maint, rep_avail: Notional, rep_ratio: Rate, rep_ens: i64, wallet: Notional,          // 6 × 8 = 48
//               rep_perp_notional: Notional, n_items: u16, has_report: u8, _pad: [u8; 5] }                                     // 64 B
//               + n_items × { inst: InstId, adl_quantile: u8, _pad: [u8; 3], liq_px: Px }                                     // 16 B/项
// FundPending:{ settle_ts_ns: i64, synth_sum: Notional, pushed: Notional, state: u8 /* SynthOnly=1 PushOnly=2 */, _pad: [u8; 7] } 32 B
// FundMark:   { hist_from_ns: i64, n: u32, _pad: u32 }                                                            16 B;每个有永续市场的 (account, venue) 一行
// FundHist:   { group: GroupId, hflags: u8 /* PUSHED=1 */, _pad: [u8; 3], settle_ts_ns: i64, booked: Notional, corrected: Notional } 32 B;键 (account, venue, inst, settle_ts_ns);inst = INST_NONE 为钱包账
// FundNoHist: { settle_ts_ns: i64, exch_amount: Notional, engine_amount: Notional, report_id: u64 }               32 B;键 (account, venue, inst, settle_ts_ns);无历史的挂起项
// FundSeen:   { settle_ts_ns: i64, amount: Notional }                                                             16 B;BalanceOnlyPush 已入账推送,按入账顺序(FIFO)
// ExtSeen:    { kind: u8 /* TradeId=1 OrderFill=2 */, _pad: [u8; 7], a: u64 /* trade_id 或 exch_order_id */, b: u64 /* 0 或 fill_seq */ } 24 B;外来成交去重键,按入账顺序(FIFO)
// AutoHeld:   { buy: Qty, sell: Qty }                                                                         16 B;键 (account, venue, inst);AUTO_RECON 入账量的待抵扣余量,只写非零项
// Px:         { bid, ask, mark, last_px: Px, mark_ens: i64, pflags: u8 /* BBO=1 MARK=2 */, _pad: [u8; 7] }        48 B;键 inst;每个有报价、标记价或成交价的 inst
#[repr(C)] pub struct LegAgg { pub open_qty: Qty, pub open_notional: Notional, pub close_qty: Qty,
                               pub close_notional: Notional, pub fee: Notional, pub gas: Notional }                  // 48 B
```

`PnlBreakdown` 为 64 B(8 × `Notional`)。`est_*` 与两种强平距离不进快照,持有者的 `upnl` 与名义缓存由 `restore` 按 `Px` 行重建(见[保证金数据](#保证金数据))。所列字节数均不含 8 B 的 `LedgerRowHdr`;`AccountId`、`VenueId` 各 u8,`InstId`、`GroupId` 各 u32(hft-types),每个结构旁有 `size_of` 断言。时刻字段只存 engine_ns(恢复后原样有效,不平移),不存 TSC;资金费的 `settle_ts_ns` 都是归一化后的值。快照中没有某 (account, venue) 的 `FundMark` 行时,`hist_from_ns` 按[资金费](#资金费)"逐笔入账记录"的恢复规则取值(此前结算的差额走无历史路径:挂起、窗口结束后入账)。

### 规划源文件

| 文件 | 职责 |
|---|---|
| `src/lib.rs` | 模块与公开导出 |
| `src/ledger.rs` | `Ledger`:`new` 按 refdata 建的静态表、动态账本、查询与只读接口 |
| `src/types.rs` | `InstPos`、`GroupBook`、`RoundBook`、`PnlBreakdown`、`FillDelta`、`AdjustDelta`、`VenueBook` 系列与结果类型 |
| `src/config.rs` | `LedgerConfig`、ECF1 `ledger` 段的行格式与验证器、`Capacities` |
| `src/fixed.rs` | `FixedVec` 与 i128 定点辅助 |
| `src/position.rs` | 平均成本法、穿越零、净量复算与 `NetAdjust`、成交与费用入口 |
| `src/cash.rs` | 现金、库存归属、手续费扣桶、余额初始化/校验/基线、划转、模拟重置 |
| `src/rates.rs` | 换算率表、过期、估值价、每 venue 汇总行、`on_ref_px` |
| `src/funding.rs` | 结算周期表与 `funding_grid`(调 hft-types `funding_settle_norm_ns`)、资金费入账与推送去重、合成计算、推送配对与残差、已入钱包额、逐笔记录环与水位线、对账自动校正、无历史挂起与窗口结束入账 |
| `src/attrib.rs` | `auto_held` 抵扣、归属候选表(含以腿减到 0 为限)、非 Shadow 过滤、`auto_external_fill`、`external_from_oms_note`、外来成交入口 |
| `src/dedup.rs` | 外来成交去重环(FIFO + 按键的开放寻址索引) |
| `src/adjust.rs` | `AdjustDelta` 四种 kind 的入账(采纳、资金费校正、核销) |
| `src/margin.rs` | 保证金报告与估计、两种强平距离、最旧标记价时刻、`restore_marks` |
| `src/rounds.rs` | 轮次开始/结束与 `RoundClosed` |
| `src/notes.rs` | 本 crate 的 Note 码 |
| `src/views.rs` | SDK 视图类型的重导出与 `Portfolio`(`PortfolioRead` 的实现) |
| `src/journal.rs` | `JournalLedger`(冷消费者) |
| `src/snapshot.rs` | 快照(一次写完三节)与恢复 |
| `src/chain.rs` | 链上两本账、重组冲回(feature `chain`,未实现) |

</details>

节 13 的 AutoCash 行以 account/venue/inst 为头部键,同账户内按补账顺序写入。80 B 体依序为 side:u8、cause:u8、fee_asset:u16、零填充:u32、qty、px、fee、exch_order_id、trade_id、ex_ts_ns、fill_seq、cash_qty、cash_amount(末九项各 8 B)。不存 rx_tsc。恢复验证完整身份与现金覆盖、重复键、容量,每品种同向凭据数量合计必须等于 AutoHeld;写回的版本、行数和字节必须与输入一致。

## 延迟与资源契约

在热进程中位于分段 `decided_tsc − proc_tsc`(成交处理)与行情处理路径上。

| 入口 | p50 | p99 | 基准名 |
|---|---|---|---|
| `apply_fill`(含现金、库存、净量复算) | ≤ 150 ns | ≤ 400 ns | `ledger/apply_fill` |
| `apply_spot_fill`(含校验、原始名义、现金与费用) | ≤ 150 ns | ≤ 400 ns | `ledger/apply_spot_fill` |
| `apply_settled_fill`(含品种/账户/资产校验、持仓、结算与费用) | ≤ 150 ns | ≤ 400 ns | `ledger/apply_settled_fill` |
| `apply_external`(含去重与归属) | ≤ 200 ns | ≤ 500 ns | `ledger/apply_external` |
| `apply_reconciled_external`(一笔新凭据,空待确认表) | ≤ 500 ns | ≤ 2000 ns | `ledger/apply_reconciled_external` |
| `validate_reconcile_batch`(2 报告、1 凭据) | ≤ 500 ns | ≤ 2000 ns | `ledger/validate_reconcile_batch` |
| `apply_settled_external`(完整现金事实,含去重与归属) | ≤ 200 ns | ≤ 500 ns | `ledger/apply_settled_external` |
| `apply_funding_push`(去重环 ≤ 64 项线性查找 + 入账) | ≤ 200 ns | ≤ 600 ns | `ledger/apply_funding_push` |
| `reconcile_funding`(环内二分查找 + 入账;不在 tick-to-trade 路径) | ≤ 300 ns | ≤ 1 µs | `ledger/reconcile_funding` |
| `funding_window_sweep`(挂起表 ≤ 64 项;每组对账报告一次) | ≤ 500 ns | ≤ 3 µs | `ledger/funding_window_sweep` |
| `on_ref_px`(持有该 inst 的 ≤ `max_subs_per_inst` 个 group) | ≤ 60 ns | ≤ 200 ns | `ledger/on_ref_px` |
| `on_mark`(含保证金估计、强平距离与最旧标记价;最小值重扫 ≤ 64 项只在原最小者变大时发生) | ≤ 100 ns | ≤ 400 ns | `ledger/on_mark` |
| `equity`(≤ 4 venue × 64 资产) | ≤ 400 ns | ≤ 1 µs | `ledger/equity`(只在日切、`on_pnl` 与视图读取时调用,结果按 rate/mark 变更计数缓存) |
| `apply_chain`(feature `chain`) | ≤ 150 ns | ≤ 400 ns | `ledger/apply_chain` |
| Python 重建 1 天 journal(hft-py) | — | — | 吞吐 ≥ 5 M 事件/分钟(`ledger/replay_day`) |

零分配(容量由 refdata、`EngineConfig` 与编译期常量决定);无系统调用;只被 sequencer 线程写。外来成交去重环用按键的开放寻址索引(O(1));逐笔资金费记录环每个有永续市场的 (account, venue) `funding_hist_cap × 32 B`(默认 128 KiB)、无历史挂起表 64 × 32 B、外来成交去重环 `ext_dedup_cap × 24 B`(默认 24 KiB)、每个永续 (account, venue) 的待确认原始凭据 `ext_dedup_cap × 96 B`(默认 96 KiB)、资金费周期表每 inst 8 B,都在 `new` 时分配(`on_funding_interval` O(1),`INST_NONE` 的 `funding_grid` 按 venue 的永续 inst 数线性扫描,每次结算一次,冷路径);快照只写窗口内的记录与去重环。

## 测试与验收

用例与验收见 [TESTS.md](TESTS.md)。LED-266 的 journal 属性驱动使用与 sequencer 相同的现货原始名义入口并保留 CI 回归种子;累计均价取整的固定反例归 LED-301,真实引擎的逐快照核对归 LED-267。

## 依赖

- 内部:`hft-types`(含资金费时刻归一化 `funding_settle_norm_ns`)、`hft-refdata`(资产、参考 inst、合约乘数、`VenueCaps`、`AccountMeta`、`Capacities`、资金费默认周期与锚)、`hft-event`(`TxLifecycle`、`FundingSettle`、`BalanceReport`、`MarginReport`、`ExternalFill`、`ReconcileReport` 的 `Funding` 条目、`Note`)、`hft-strategy-sdk`(只用类型:`GroupTable`,以及视图类型 `VenueEquityView`、`GroupPosView`、`CashView`、`InventoryView`、`PerpPosView`、`MarginView`——本 crate 是这些视图数据的唯一写者)。
- 外部:无。
- 禁止:I/O、`std::time`、f64(报表侧的浮点格式化在 Python/`hft-py` 里做)、`hft-oms`/`hft-risk`/`hft-exec`(`FillDelta`、`AdjustDelta`、`NetAdjust` 是本 crate 的类型,由 sequencer 转换)、任何 `strategies/*`。
- 被依赖:hft-sequencer、hft-py、reconciler(引擎视图,同一记账代码)。资金费结算时刻的归一化函数在 hft-types(`funding_settle_norm_ns`),本 crate 只调用它。

## 延伸阅读

- [round8 裁决](../../design/_work/round8-decisions.md)(R8-3 解耦、R8-4 账户与资金、R8-8 不做同所现货 × 永续与资金费只告警、R8-11);评审证据 [review-2026-10-02](../../design/_work/review-2026-10-02-主策略适配与执行正确性.md) §1 表第 3 行、§3 P0-3/4/6,[review-2026-10-03](../../design/_work/review-2026-10-03-M0独立评审.md) P0-1、P1-5、P2,[M0-b 评审](../../design/_work/review-2026-10-03-M0b独立评审.md) P0-3、P1-3、P1-7,[M0-c 五路汇总](../../design/_work/review-2026-10-03-M0c五路汇总.md)(M-P1-6、V1 P2-4/10/11、V3 #21、#50),[M0-d 五路汇总](../../design/_work/review-2026-10-03-M0d五路汇总.md)(§4-13 核销残余与 ADL 自动入账、M-P2 资金费时刻归一化、W3 P1-16),[M0-e 五路汇总](../../design/_work/review-2026-10-03-M0e五路汇总.md)(X1 P1-C、P2-8,X3 P0-3)
- Agent Note:[冷账本槽身份](../../.agents/notes/implemented/bug-fix/2026-10-10-journal-slot-identity.md)
- Agent Note:[冷账本完整读视图](../../.agents/notes/implemented/bug-fix/2026-10-12-cold-ledger-read-views.md)
- Agent Note:[永续钱包的结算事实](../../.agents/notes/implemented/architecture/2026-10-12-perp-cash-settlement.md)
- Agent Note:[按原始凭据对账](../../.agents/notes/implemented/architecture/2026-10-13-reconcile-cash-receipts.md)
- Agent Note:[AUTO_EXTERNAL 前置条件与 auto_held](../../.agents/notes/proposed/architecture/2026-10-03-auto-external-guard.md)
- [07 §5 仓位与盈亏记账](../../design/07-策略层.md)、[09 §4.3 对账与会计口径](../../design/09-可观测性、风控与控制面.md)
- [hft-strategy-sdk](../hft-strategy-sdk/README.md)(`PortfolioView`)、[hft-oms](../hft-oms/README.md)、[hft-risk](../hft-risk/README.md)、[hft-exec](../hft-exec/README.md)、[hft-sim](../hft-sim/README.md)、[hft-py](../hft-py/README.md)

## 已知限制与后续工作

- 普通订单与原始外来成交的 `PERP_CASH` 已接到 sequencer 与 JournalLedger;测试世界的能力与资金验收归 [hft-testkit](../hft-testkit/README.md#6-dst-框架),生产协议尚未启用。外来归属重叠、已过保留期的系统订单重新归属、生产对账服务的原始历史查询与模拟恢复装配仍待接通;LED-080/081 的专属场景仅覆盖其明确列出的原件关系,不代表多笔原件与累计量部分重叠已获证明,未证明的严格结算路径明确失败。冷读者不完整镜像 OMS 的状态机与回收时机,跨回收的增量均价、未接受的非成交回报及外来归属仍需专门验证;普通成交用例不代表全部冷热一致性验收。
- 平均成本法之外(FIFO 税务口径)不支持;需要时在 hft-py 侧另算。
- 不建模余额域(同一钱包被多个产品线共用):这种情形被"一个账户在同一交易所只用一个市场"排除([hft-refdata](../hft-refdata/README.md))。若将来放开同所现货 × 永续,需要先把现金改为按余额域记账(触发条件:决定放开该规则)。
- 资金费自动校正依赖交易所流水:`funding_recon_window_ms`(48 h)之外的差额不再比对;水位线之前(无历史)的差额挂起到窗口结束才入账,若其实早已入账会造成一次重复入账,由余额对账报出、人工 `AdoptExchange` 收敛;`RateOnly` 所在校正之前钱包是估计值,两次对账之间余额对账条目可能出现等于未校正资金费的差额(由 reconciler 与 watchtower 处理口径)。
- 资金费时刻按结算边界归一化:推送或流水的时刻偏离结算边界超过半个周期会被归到相邻的结算;交易所改周期后、带新周期的 `Funding` 到达之前的那次结算仍按旧周期归一化,可能与相邻结算合并,差额由对账的 `Funding` 条目校正;`INST_NONE` 按该 venue 最短周期归一化,要求各 inst 的结算边界都落在它的网格上(已知各所成立)。
- `BalanceOnlyPush` 推送按 `(settle_ts_ns, amount)` 去重:同一结算时刻两笔金额恰好相同的不同推送会被当作重复,差额由下一轮对账的钱包条目补回(至多晚一个对账周期)。
- **旧能力的数量型 `auto_held` 不过期**:未启用 PERP_CASH 时,交易所不补推漏掉的强平/ADL 时,`auto_held` 会抵扣之后同向的下一笔真实强平/ADL(或无法匹配的未知成交),账本少记该笔,由下一轮对账报出(再次满足条件时 `AUTO_EXTERNAL` 补记,否则人工 `AdoptExchange`)。可选扩展:按时间或对账结果清除 `auto_held`(触发条件:测试网断流演练证实某所不补推)。
- **任何 venue 启用 PERP_CASH 之前的阻塞项**:冷读者的外来现金来源与水位表(`journal_foreign`,上限 `SLOT_MAP_CAP`)与 hft-oms 的 `foreign_cash` 表一样只增不减,没有按订单终结回收的规则,写满后 `ForeignCapacity`;启用前必须与 hft-oms 一起定义回收条件并补用例([hft-oms](../hft-oms/README.md#已知限制与后续工作))。
- 外来成交去重环有容量(`ext_dedup_cap`):早于环内最旧键的同一笔成交被交易所再次回放时会重复入账,由余额对账报出、人工处理(触发条件极少:回放窗口远小于 1 024 笔外来成交)。
- 核销只改归属,不改账:被核销的残余仍是交易所上的真实仓位,作为账户级无 group 持仓(永续)或 `unassigned`(现货)留在账上,计入权益;永续部分由 L3/`Kill{flatten}` 覆盖,现货部分由人工处理(例如以 `SetBalanceBaseline` 记为基线)。
- 现金的 free/locked 拆分只来自交易所报告(`rep_free`/`rep_locked`),引擎侧不模拟各所的冻结规则;执行层按"报告 free − 本系统在途"保守估算可用量。
- 多资产保证金(联合保证金)模式下,保证金钱包按换算率折 USDT,各资产的折扣率(haircut)不在本 crate;保证金率以交易所报告为准,估计值只在两次报告之间补充。强平距离的估计不建模分级维持保证金率与交易所的部分强平规则,因此 hft-risk 与交易所 `liq_px` 取更保守者。
- 池子费与链上冲击不单独分解(feature `chain`,在成交价里)。
- Solana 是可选扩展(触发条件:有明确的 Solana 机会与预算);接入时按同样的包含/终局规则核对会计口径。

### 开发备忘

<details><summary>给实现者</summary>

- **植入缺陷(feature `planted`,只在测试中)**:`src/planted.rs` 按线程打开的开关:晚到的纯费用回报不入余额(`ignore-fee`)。只由 hft-testkit 的 `planted` feature 打开,生产清单、`seq` 与 `dst-bin` 都不含它([TK-178](../hft-testkit/TESTS.md#tk-178-植入缺陷不进入任何生产构建));真实引擎 DST 发现各缺陷的验收归 [TK-179](../hft-testkit/TESTS.md#tk-179-真实引擎的植入缺陷在有界种子内被不变量发现) 与 TK-053.21–TK-053.23。
- **待核实**:各所现货成交回报是否总带手续费与手续费资产(不带的所 `fee_asset = ASSET_NONE`,之后必须有 `FeeOnly`),方法:测试网/模拟盘账户逐所下单比对回报与成交历史;各所余额推送是否包含 locked,方法同上;`BalanceOnlyPush` 所的资金费推送在同一结算时刻是一笔合计还是每 inst 一笔(决定推送去重键是否足够),方法:测试网持仓两个以上 inst 跨过一次结算,抓用户流原文;各所资金费推送与流水时刻相对结算边界的偏差,方法同上(决定按结算边界归一化是否足够);三所私有流断开期间发生的强平/ADL 在重连后是否补推(决定 `auto_held` 是否会长期残留),方法:测试网断流演练。
- 实现顺序:position + 净量(含 `NetAdjust`)→ cash/库存/余额初始化/手续费扣桶 → rates → equity → funding(含结算周期表与 `funding_grid`、推送去重、已入钱包额、无历史挂起)→ margin(含强平距离、最旧标记价、`restore_marks`)→ external(去重环)/adjust(采纳、核销)/baseline → rounds → 快照 → hft-py 绑定;chain 两本账随 feature 最后。
</details>
