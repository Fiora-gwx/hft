# 第八轮规格修订简报(M0;所有修订子代理先读)

本简报是第八轮修订的**唯一权威命名与接口来源**。各子代理只改自己清单里的文件;凡涉及跨文件的类型、事件、命令、参数名,一律按本简报写,不要自行另起名字。若本简报与旧规格冲突,以本简报为准;若本简报没覆盖、你又必须决定,按"最小、通用、与本简报精神一致"决定,并在最终报告里列出。

## 0. 必读(按顺序)

1. `E:\trading\architecture-v2\_work\round8-decisions.md`(用户裁决,R8-1 … R8-7)
2. 本简报
3. `E:\trading\architecture-v2\_work\round8-问题清单与修改制作方案.md`(问题编号 A1–A5、B1–B6、C1–C15、D、E;工作包 WP1–WP8)
4. `E:\trading\architecture-v2\_work\review-2026-10-02-主策略适配与执行正确性.md`(评审证据与出处)
5. `E:\trading\architecture-v2\_work\spec-brief.md`(原写作规则:中文为主、标识符英文、README 模板、不写函数体、一件事实一个归属地、相对链接、大文件用 Write 工具)——仍然适用,下文另有说明的除外
6. `E:\trading\hft\crates\AGENTS.md`(README 模板)、`E:\trading\hft\docs\architecture.md`(现状地图)

**修订前的完整副本**在 `E:\trading\_backup\round8-pre\`(`hft\` 与 `architecture-v2\`)。读旧内容(尤其是被拆分或删除的文件)一律读备份,不要依赖工作区里可能正被别人改的文件。

## 1. 修订原则

- **延迟优先**(R8-2):热路径预算、无分配、无系统调用、无 `dyn`、快速部分无 f64 等规则全部保留。新增的 SDK、执行层也要给出热路径预算。
- **策略与工程解耦**(R8-3):引擎 crate 的 README 里不得出现策略族名(PortfolioCarry、Cdex、CrossTaker、MakerTaker、Carry 等)或某个策略的具体规则数字;它们只出现在 `strategies/*` 与设计章节 07。引擎只提供通用机制和由参数给出的阈值。
- **交易所无关**:策略与执行层不出现交易所名字;交易所差异写进 `VenueCaps`(§5)与各 `venue-*`。
- **ccex 首要**(R8-1):链上相关组件转 optional,但不删除;外部 venue 协议保留。cdex(D/E)整体搬到 `strategies/cdex`(optional)。
- **保留好的设计**:Unknown 语义、`(epoch, slot, gen)` 订单 id、LegTracker 把在途算进裸敞口、期望模式与 cap 分离、journal 纪律(Input/Output/Meta)、每 gw 的 kill 字、RISK_REDUCING 放行、死手 + 独立 kill-switch。只修漏洞,不推倒。
- **scope 语义更新**:`core` = 首次上线(新子账户上线 ccex 策略)必须具备、必须通过上线门槛;`optional` = 可选扩展,写触发条件,不写周次。
- **裁决出处少写**:正文不要到处插"第 N 轮裁决 X"。本轮的出处只在"延伸阅读"里链一次 `round8-decisions.md`。顺手删掉你文件里明显的历史噪音(如"round5 M3"式括注)时,保留规则本身。
- **重复事实只留一个归属地**:改到某个事实时,确认它的归属文件(见 §11),其他地方改成链接。
- 交易所行为没有官方出处的,一律标"待核实"并写核实方法(本机连不上 developers.binance.com、OKX、Bitget 文档站)。

## 2. 组件变更总表

| 组件 | 变化 | scope |
|---|---|---|
| `crates/hft-strategy-sdk`(新) | 策略唯一依赖的稳定接口:`Strategy`、`Model`、`ParamSet`/`ParamSchemaDesc`、`Views`、`Ctx`、`EventRef`、`OrderIntent`、`TargetIntent`、`ExecSpec`、视图类型、`StrategySet` trait 与 `strategy_set!` 宏、运行模式下的意图行为表 | core |
| `crates/hft-exec`(新) | 执行层:目标跟踪、吃单切片、两腿 OnFill、Simultaneous、MakerFirst(预留);引擎默认减仓 | core |
| `strategies/`(新顶层目录) | 一策略一 crate;`strategies/AGENTS.md` 规则 | — |
| `strategies/portfolio-carry`(新) | 首个 ccex 策略(结构取自老板雏形:打分 → 分配 → 目标 → 调仓;公式全部参数化;不照搬雏形缺陷) | core |
| `strategies/cdex`(新) | 原 hft-strategies 的 D、E 与 hft-strat-lib 中 cdex 专用构件(Sizer 因子、ClosePolicy、FundingGate 常数、SwapPicker 等) | optional |
| `strategies/cross-taker`(新) | 原 A 类 | optional |
| `strategies/maker-taker`(新) | 原 B 类 | optional |
| `crates/hft-strategies` | **删除**(内容拆到 SDK 与 `strategies/*`;删除由主控在合并时执行,子代理不要删) | — |
| `crates/hft-strat-lib` | 只留通用构件 | core |
| `crates/hft-sim` | 改定位:模拟撮合 + 模拟账户(实时行情纸面交易;journal 重演排障);hftbacktest 队列模型降为可选 feature | core |
| `apps/replayer` | verify(core)+ 模拟重演排障(core);假设回放/参数扫描删除 | core |
| `venue-okx`、`venue-bitget` | 升为 core(现货 + USDT 永续) | core |
| `venue-binance` | USDT-M 永续 + 现货(JSON);现货 SBE 为可选 | core |
| `venue-gate`、`venue-bybit`、`venue-kucoin`、`venue-hyperliquid` | optional | optional |
| `apps/chain-core`、`apps/chain-grpc-relay`、`crates/hft-chain-evm`、`crates/hft-ext-proto`、`crates/hft-amm`、`crates/hft-chain-solana`、`contracts/` | optional(保留规格,frontmatter 与概要标明"cdex 迁移时启用") | optional |
| `python/treasury` | core:监控 + 划转建议,不自动划转 | core |
| `python/research` | optional:不做回测;可做模拟结果分析 | optional |
| `python/pipeline` | core 框架 + 每策略插件;cdex 插件 optional | core |
| `apps/mock-exchange` | optional | optional |
| `docs/runbooks/migration.md` | optional(cdex 迁移时用);新增 `docs/runbooks/launch.md`(core) | — |

## 3. 策略 SDK(`hft-strategy-sdk`)接口草图(权威)

依赖方向:SDK 只依赖 L0(`hft-types`、`hft-refdata`、`hft-event`)。L1 引擎 crate(`hft-oms`、`hft-ledger`、`hft-risk`、`hft-exec`)**可以**依赖 SDK(为了用 SDK 定义的视图类型与意图类型);`hft-sequencer` 依赖 SDK 与 L1。**任何引擎 crate 都不依赖 `strategies/*`**;策略 crate 只依赖 SDK 与 `hft-strat-lib`(后者也只依赖 L0 + SDK)。

```rust
/// 静态元数据。kind_id 全局稳定、只追加;schema 描述参数。
pub struct StrategyDesc { pub name: &'static str, pub kind_id: u16, pub version: u32, pub schema: &'static ParamSchemaDesc }

pub trait Strategy: Sized {
    const DESC: StrategyDesc;
    type Params: ParamSet;            // 已解码、已校验的定点参数(快速部分用)
    type Model: Model;                // 慢速部分;无则用 NoModel
    fn new(inst: StrategyInstance, p: &Self::Params, groups: &GroupTable, rd: &RefView) -> Result<Self, ParamError>;
    fn subscriptions(&self, groups: &GroupTable, out: &mut SubsBuilder);
    /// 唯一热路径入口:不得分配、阻塞、读系统时钟、panic;无 f64。
    fn on_event<V: Views>(&mut self, ev: &EventRef<'_>, v: &V, ctx: &mut Ctx<'_>);
    fn on_params(&mut self, p: &Self::Params, groups: &GroupTable, rd: &RefView) -> Result<(), ParamError>;
    fn snapshot(&self, w: &mut SnapshotWriter);
    fn restore(&mut self, r: &mut SnapshotReader) -> Result<(), RestoreError>;
}

/// 慢速部分:在 advisor 线程(warm)运行;允许 f64 与分配;必须确定(只用事件时间,无 I/O、无系统时钟、无 HashMap 迭代);
/// 输入 = 它订阅的 journal 事件;输出 = Advice(不透明载荷,进入 journal,回放从日志读)。
pub trait Model: Sized {
    fn new(inst: StrategyInstance, p: &<Self as Model>::Params, rd: &RefView) -> Self; // Params 与所属 Strategy 相同(实现时用关联类型绑定)
    fn on_event<V: ModelViews>(&mut self, ev: &EventRef<'_>, v: &V, out: &mut AdviceOut<'_>);
    fn snapshot(&self, w: &mut SnapshotWriter);
    fn restore(&mut self, r: &mut SnapshotReader) -> Result<(), RestoreError>;
}

pub trait ParamSet: Sized {
    fn decode(body: &[u8], schema_ver: u16) -> Result<Self, ParamError>;          // schema 序编码,见 §6
    fn validate(&self, groups: &GroupTable, rd: &RefView) -> Result<(), ParamError>;
}
pub struct ParamSchemaDesc { pub id: u32, pub version: u16, pub fields: &'static [FieldDesc] }
pub struct FieldDesc { pub name: &'static str, pub unit: Unit, pub min: i64, pub max: i64, pub default: i64,
                       pub per_group: bool, pub doc: &'static str }
pub enum Unit { Rate /*1e-9*/, Notional /*USDT 1e-6*/, Qty, Px, DurationMs, Count, Bool, Enum(&'static [&'static str]) }

/// 只读视图(静态分派;引擎实现,测试用 mock 实现)。
pub trait Views {
    fn now(&self) -> Now;                                   // 当前事件时间(记录头 tsc、ex_ts、seq),不是系统时钟
    fn bbo(&self, i: InstId) -> &BboView;
    fn book(&self, i: InstId) -> BookView<'_>;
    fn funding(&self, i: InstId) -> &FundingView;           // 实时费率、下次结算时刻、实际结算周期
    fn mark(&self, i: InstId) -> Px;
    fn portfolio(&self, a: AccountId) -> PortfolioView<'_>; // §4
    fn group_pos(&self, a: AccountId, g: GroupId) -> &GroupPosView;
    fn legs(&self, a: AccountId, g: GroupId) -> &LegView;   // 各腿净量、在途、残余、naked
    fn target(&self, a: AccountId, g: GroupId) -> Option<&TargetView>; // 执行层当前目标与进度
    fn order(&self, h: OrderRef) -> Option<OrderView>;
    fn inst(&self, i: InstId) -> &InstMeta;
    fn caps(&self, v: VenueId, m: MarketKind) -> &VenueCaps; // §5
    fn mode(&self, a: AccountId, g: GroupId) -> RunMode;
    fn risk(&self, a: AccountId) -> RiskStateView;          // 生效 cap、保证金档位
}
pub trait ModelViews: Views {}                              // 模型侧视图(从 journal 重建的同一组状态)

pub struct Ctx<'a> { /* 固定容量缓冲;sequencer 在每个输入事件后排空 */ }
impl<'a> Ctx<'a> {
    pub fn place(&mut self, it: OrderIntent) -> Result<IntentHandle, CtxFull>;
    pub fn target(&mut self, t: TargetIntent) -> Result<IntentHandle, CtxFull>;   // 替换该 (account, group) 的当前目标
    pub fn clear_target(&mut self, a: AccountId, g: GroupId) -> Result<(), CtxFull>;
    pub fn cancel(&mut self, h: OrderRef) -> Result<(), CtxFull>;
    pub fn amend(&mut self, h: OrderRef, px: Px, qty: Qty) -> Result<(), CtxFull>;
    pub fn timer_at(&mut self, at: Tsc, tag: u32, group: GroupId) -> Result<TimerId, CtxFull>;
    pub fn cancel_timer(&mut self, id: TimerId);
    pub fn note(&mut self, code: u32, level: u8, inst: InstId, v: [u64; 8]);
}

pub enum EventRef<'a> {
    Bbo { inst: InstId, gix: u16, b: &'a BboView }, Book { inst: InstId, gix: u16 }, Trade { inst: InstId, gix: u16, t: &'a TradeView },
    Funding { inst: InstId, gix: u16, f: &'a FundingView }, Mark { inst: InstId, gix: u16, px: Px },
    Order { gix: u16, u: &'a OrderUpdate },          // Acked | Filled(d) | Cancelled | Rejected | Unknown | Aged
    Exec { gix: u16, u: &'a ExecUpdate },            // 目标进度:Progress | Done | Stuck(reason) | Broken
    External { gix: u16, f: &'a ExternalFillView },  // 强平、ADL、人工、交割
    Risk { a: AccountId, e: &'a RiskEvent },         // MarginStress{level, deadline_tsc} | HedgeBroken{group} | CapSet/CapCleared
    Notice { inst: InstId, gix: u16, n: &'a VenueNoticeView }, // 下架、结算周期变化、维护
    Timer { tag: u32, group: GroupId, gix: u16 },
    Advice { gix: u16, local_kind: u16, payload: &'a [u8] },  // 本策略模型产出的不透明载荷
    Mode { gix: u16, old: RunMode, new: RunMode },
    Conn { venue: VenueId, state: ConnStateKind },
    Swap { gix: u16, u: &'a SwapUpdate },            // 仅链上(optional)策略使用
}

/// 装配:apps/hft-core、apps/replayer、hft-testkit 用同一宏生成同一组静态分派的策略枚举。
/// 新增策略 = 新 crate + 在装配处宏调用里加一行;引擎 crate 零改动。
pub trait StrategySet { /* kinds()、create(kind_id, …)、decode_params(kind_id, …)、按枚举分派的 on_event/model 方法 */ }
// strategy_set!{ PortfolioCarry, /* Cdex, CrossTaker, … */ }
```

- `OrderIntent` 保留原字段(venue、account、side、tif、class、flags、inst、group、px、qty、link、tag、max_rest_ms),定义移入 SDK。`SwapIntent` 移入 SDK 的 `chain` 模块(optional feature)。
- 运行模式下"策略意图怎么处理"的行为表(Shadow/CloseOnly/Live、`open_allowed`、ShadowDeferred、Mode 事件)从旧 hft-strategies 移入 SDK README;模式状态仍归 `hft-risk::mode`。
- `RunMode` 不变。**模拟不是 RunMode,是账户类型**(§7)。

### 3.1 目标意图(`TargetIntent`,SDK 定义,`hft-exec` 执行)

```rust
pub struct TargetIntent {
    pub account: AccountId, pub group: GroupId,
    pub legs: [LegTarget; 4], pub n_legs: u8,
    pub reason: TargetReason,       // Rebalance | Exit | MarginStress | Hard | Manual
    pub urgency: Urgency,           // Passive | Normal | Urgent
    pub band: Rate,                 // 不交易带:|target − actual| / max(|target|, floor) ≤ band 时不动
    pub exec: ExecSpec,
    pub tag: u32,
}
pub struct LegTarget { pub leg_ix: u8, pub qty: Qty }   // 该腿目标净量(带符号,资产单位)
pub enum ExecSpec {
    TakerSlice(TakerSliceParams),    // 单笔 = min(|差额|, 弱侧可见深度 × depth_frac, max_chunk_notional),≥ min_chunk;同 group 两笔间隔 ≥ cooldown_ms
    PairOnFill(PairParams),          // 两腿:先下 first_leg,按其成交净量下另一腿 IOC;first_leg = ByLiquidity | PerpFirst | SpotFirst | Fixed(ix)
    Simultaneous(SimulParams),       // 两腿同发,失败反向(on_fail = Reverse | Hold)
    MakerFirst(MakerParams),         // 预留(optional):先挂单,成交后对冲
}
```

- 目标在被替换或清除前一直有效;执行层每个相关事件(BBO、成交、定时器)推进一次,O(腿数)。
- 增仓部分受模式门与风控 Open 规则约束;减仓部分按 Reduce/Flatten 分类。
- 腿顺序规则:`reason ∈ {MarginStress, Hard}` 时强制 `PerpFirst`(先减永续);`Rebalance/Exit` 开仓默认 `ByLiquidity`(流动性差的先),可由参数覆盖。
- 进度经 `EventRef::Exec` 通知策略;执行层的每次决定写 Output 记录 `ExecDecision`(§6)。

## 4. 权益视图(`PortfolioView`,SDK 定义视图,`hft-ledger` 维护数据)

每个账户、每个 venue:各资产现金(free/locked)、**现货库存**(资产 → 数量,按 group 归属或 unassigned)、永续持仓(inst → qty、entry、按 mark 的未实现)、保证金余额(= wallet + Σ未实现)、维持保证金、可用保证金、保证金率(统一口径:维持保证金 / 保证金余额;各所换算方法写在 `venue-*`)、合计权益(USDT;现货按 bid、永续按 mark;估值规则是参数)。**引擎不计算"可用资金"或杠杆公式**——删除旧 `CapitalView`(`wallet × leverage × deployable`),由策略自己从视图算。

## 5. 交易所能力表(`VenueCaps`,`hft-refdata` 定义,`venue-*` 提供默认值,refdata 可覆盖)

按 `(venue, MarketKind ∈ {Spot, LinearPerp})`:

| 字段 | 含义 |
|---|---|
| `fee_asset_rule` | `Quote` \| `ReceivedAsset` \| `ReceivedAssetUnlessDiscount{token}` \| `Configured(asset)`:手续费扣哪个资产(决定成交净量) |
| `reduce_only` | 是否支持 reduceOnly |
| `required_position_mode` / `required_margin_mode` | 我们要求的账户配置(如单向持仓、全仓);启动核验 |
| `funding_settle` | `PerSymbolPush` \| `BalanceOnlyPush` \| `RateOnly`:资金费实收推送方式(后两者由 sequencer 按"结算时刻持仓 × mark × 费率"合成每 inst 的 `FundingSettle{synth=1}`,推送与流水只做校验) |
| `margin_data` | `Push` \| `Poll{interval_ms}`:保证金率、强平价、ADL 档的来源 |
| `own_liquidation_marker` | 识别自身强平/ADL 订单的方法(由 venue crate 实现,README 写明;待核实) |
| `ignorable_user_events` | 用户流中可忽略的事件名列表 |
| `amend`、`post_only`、`ioc`、`batch_cancel`、`dms` | 能力位 |
| `testnet` | `None` \| `Testnet{…}` \| `Demo{…}`(待核实) |

`AccountMeta` 增加:`kind: AccountKind { Live, Sim(SimAccountCfg) }`、`venue_uid`(交易所账户 UID,启动时核验,C9)、`bound_instance: StrategyInstanceId`(**一个账户只绑定一个策略实例**,R8-4)。

## 6. 事件与编码变更(`hft-event` 归属)

- **Advice** 改为变长不透明载荷:`AdviceHdr { strategy: StrategyId, account: AccountId, group: GroupId /* 或 GROUP_ALL */, local_kind: u16, len: u16 }` + `payload[len]`,`len ≤ 4096`(更大的拆成多条,`local_kind` 自行约定)。引擎不解释载荷;删除全局 `AdviceKind` 登记与 `v: [i64; 6]`。
- **StrategyParams** 改为:`StrategyParamsHdr { strategy: StrategyId, kind_id: u16, account: AccountId, schema_id: u32, schema_ver: u16, version: u64, generated_at_ns: i64, source: ParamSource, n_groups: u16 }` + `[GroupSpec; n_groups]` + `body`。`GroupSpec { id: GroupId, asset: AssetId, n_legs: u8, legs: [LegDef; 4], open_allowed: bool }` 由引擎理解(订阅、InstOwned、账户绑定、LegTracker、ledger 归属);`body` 由策略的 `ParamSet::decode` 解码。删除 `ParamsBody` 枚举。
- **body 的规范编码**("schema 序编码"):defaults = 按 `FieldDesc` 顺序的 i64 小端;per-group 覆盖 = `(group_ix: u16, field_ix: u16, value: i64)` 三元组表。Python 工具按导出的 schema(JSON)生成同样字节。
- 新事件(编号在执行段追加,只追加):
  - `ExternalFill { venue, account, inst, side, qty, px, fee, fee_asset, cause: Liquidation | Adl | Manual | Delivery | Unknown, exch_order_id, ex_ts_ns, rx_tsc }`(Input)
  - `MarginReport { venue, account, margin_balance, maint_margin, avail_margin, margin_ratio: Rate, n, items: [(inst, liq_px, adl_quantile)] }`(Input;变长或分条)
  - `AccountConfigReport { venue, account, position_mode, margin_mode, multi_asset, mismatches: bitset }`(Input)
  - `ExecDecision`(Output:目标、选择的算法、切片、原因)
  - `VenueNotice` 走控制命令(§8),落 journal 的是 `ControlCmd`。
- `FundingSettle.inst` 允许为 `INST_NONE`(仅余额推送的所);合成规则见 §5。
- `FillDelta`(ledger 类型)增加 `net_qty`:手续费资产 = 基础资产时,`net_qty = qty − fee`(买入)。
- 账户类型为 Sim 的回报与实盘回报**同一组事件**,不加新 kind;按 `AccountMeta.kind` 区分。

## 7. 模拟系统(`hft-sim` 归属;`apps/hft-core` 装配;`apps/replayer` 重演)

- **模拟账户**:`AccountKind::Sim(SimAccountCfg { initial_balances, latency_model, fee_tier, margin_model })`,绑定某个真实 venue 的行情。策略在模拟账户上以正常 `RunMode` 运行;sequencer 把该账户的 `OutMsg` 路由到 `sim` 出环,由 **sim-gw 任务**(warm 线程,不占热核)处理;回报经同一回报环进入并记 journal(Input)→ 回放确定。
- **撮合**:吃单在"下单时刻 + 模拟延迟"(按实测的往返/回执分布抽样,可配置)对实时 L2/BBO 逐档成交;**已吃流动性记忆**(同一档被我们吃掉的量在 `consume_decay_ms` 内从可见量中扣除);部分成交;按 refdata 与 `VenueCaps` 执行拒单规则(步长、最小名义、价格限制、reduceOnly);手续费与扣费资产;资金费在结算时刻按真实费率结算;永续全仓保证金与强平计算;现货余额;跨所划转用控制命令模拟。挂单撮合(保守:成交穿价才成交)预留给 MakerFirst。
- **共用撮合器**:`hft-testkit` 的最小撮合器就是 `hft-sim` 的撮合器(一套代码)。hftbacktest 队列模型降为 `hft-sim` 的可选 feature。
- **多账户并行**:可同时开多个模拟账户跑不同参数;报告(PnL 分解:资金费、基差、手续费、相对到达价的滑点;成交率;两腿间隔;裸敞口;保证金占用)由 journal-archiver/watchtower 产出、ops_bot 展示(标注"模拟")。
- **校准**:小额实盘开始后,同参数模拟账户与实盘并行,量化滑点、成交率、延迟差异,回写模拟参数。
- **重演排障**:`replayer sim` 模式把录下的 journal 行情喂给模拟网关 + 同一策略,用于排障与复现,**不用于评估收益**(R8-6 第 1 条)。`replayer verify` 不变。
- reconciler、kill-switch 跳过模拟账户;模拟账户不持有交易所 key。

## 8. 控制命令变更(`docs/subsystems/control-plane.md` 归属)

- `SetParams`:携带 §6 的 blob;ctl-gateway 先通用解析头与 `GroupSpec`,body 用装配处的 `StrategySet::decode_params` 校验;校验 `account` 已绑定到该策略实例(否则 `InvalidArgs{AccountBound}`)。
- 新增:`GetSchema{strategy_kind}`(只读,返回 schema JSON);`SimReset{account}`、`SimTransfer{account, from_venue, to_venue, asset, amount}`(只对 Sim 账户);`VenueNotice{venue, inst | *, kind: Delisting{at_ns} | FundingInterval{new_s} | Maintenance{from_ns, to_ns}}`(Delisting → 该 inst 的 `Delisting` cap:CloseOnly + 通知策略);`SetBalanceBaseline{account, venue}`(把当前非策略资产登记为基线)。
- `AdoptExchange` 扩展:可采纳**余额**条目(现货库存)与可选入场价。
- `Kill` 允许 `{Account(a), hard}`(按账户 HARD,C8)。
- 二次确认协议以 control-plane 现有的 PREVIEW + `expect_state_version` 为准(删除 Prepare→Commit{token} 的说法)。
- ops_bot 角色:`Admin`、`Trader`、`Viewer`;**老板 = Viewer(只读)**。

## 9. 风控与执行正确性修订(B、C 项的规范解法)

| 编号 | 规范解法 | 归属文件 |
|---|---|---|
| B1 | `IntentView` 增加 `link_venue`、`link_account`;带 LegLink 的 Open 对两腿 venue 同时执行 #3(cap/kill)、#4(`venue_ready`、Unknown 门)、#8(令牌桶余量);同账户任一 group `Broken` → 账户级 `HedgeFailure` cap 挡该账户全部 Open,`Resume{HedgeFailure}` 解除 | hft-risk;hft-oms(提供 link 信息) |
| B2 | 第二腿重试保留 `role = Second`(新增 `LinkRole::SecondRetry` 仅用于计数),`reduce_only` 继承 `HedgePlan.reduce_only`;`HedgeFix` 只用于"平掉多出的一腿";classify 表加一行"开仓链接的重试:Hedge、RISK_REDUCING 是、reduceOnly 否" | hft-oms、hft-risk |
| B3 | venue gw 按 `margin_data` 轮询/接收保证金数据 → `MarginReport`;自身强平/ADL/人工成交 → `ExternalFill`(不再只记 ForeignOrder Note);ledger 入账、LegTracker 更新、报 P0、策略收到 `External`;策略在 `external_deadline_ms` 内未处理 → 执行层按"平掉多出的一腿"默认处理 | venue-*、hft-event、hft-ledger、hft-oms、hft-exec |
| B4 | `VenueCaps.fee_asset_rule` + `FillDelta.net_qty`;仓位/库存/对冲量按净量;费用晚到(FeeOnly)时 ledger 与 LegTracker 补差;现货卖出量取 min(库存, 可用余额) | hft-refdata、hft-ledger、hft-exec、hft-oms、venue-* |
| B5 | 启动闸门 `RECONCILED` 要求本轮全部 `Ignore`,或每项差异已 `AdoptExchange`/`Ack`;出现 `tail_lost` 或 RecoverMismatch 时禁用 `auto_resume_after_clean_drain`;HARD 解除同样要求 | hft-risk、apps/reconciler |
| B6 | 检查链 #5(价格带,风险递减单只查越价上限)、#7(仓位)、#8(速率)对全部类别生效;令牌桶为 RISK_REDUCING 预留份额 `rr_reserve`;对冲量 ≤ 触发成交净量 × ratio + 1 lot,名义 ≤ `max_hedge_notional`;超限 → `Broken` + P0,不发单 | hft-risk、hft-exec、hft-oms |
| A5 | `MarginStress` cap(按账户):L1 `margin_ratio ≥ margin_block_open`(默认 0.60)→ 禁开;L2 `≥ margin_reduce`(0.70)→ `RiskEvent::MarginStress` 通知策略,`deleverage_deadline_ms`(30 000)内须减仓;L3 `≥ margin_force`(0.80)或截止已过 → 执行层按比例减永续敞口(每步 `force_step` 20%,`PerpFirst`,reason = MarginStress);数据过期(`margin_stale_ms`)视为 L1。默认值标"待标定" | hft-risk、hft-exec、SDK(事件)、watchtower(P0 规则) |
| C1 | 对冲腿 NotFound 不定 Rejected,置 Unknown 直到成交历史确认;回收槽保留墓碑(最近 `tombstone_gens` 代),迟到成交照常入 ledger/LegTracker 并报 P0 | hft-oms |
| C2 | 恢复时决策不一致 → 该 (venue, account) 全部置 Unknown、闸门严格化、报 P0 | hft-sequencer |
| C3 | watchtower 对账规则改为读 `Recon` cap 与带名义阈值的 Warn;`StaleGenFill`、`ExternalFill`、有成交的 `ForeignOrder` 报 P0 | apps/watchtower |
| C4 | 自动 HedgeFix 复用重试计次、冷却与截止;HARD 下抑制 | hft-oms、hft-exec |
| C5 | ClockBad、Halted 只拦 Open;硬动作与 Flatten 绕过 cooldown 与 in_flight | hft-risk、hft-strat-lib |
| C6 | 启动闸门新增 `ACCOUNT_CFG`(`AccountConfigReport` 与 `VenueCaps` 要求一致);运行中配置变化 → cap | hft-risk、venue-*、hft-venue-proto |
| C7 | ledger 现货库存一等化;`SetBalanceBaseline`;`AdoptExchange` 支持余额 | hft-ledger、control-plane、apps/reconciler |
| C8 | 按账户 HARD;同一 seq 连续崩溃 `poison_crash_limit`(2)次 → 隔离该输入、从最近快照起步并按 `tail_lost` 处理;提供带严格对账的 `--recover-from-snapshot`;版本回滚路径写清 | hft-risk、hft-sequencer、apps/hft-core |
| C9 | 启动闸门新增 `IDENTITY`:按 key 查交易所 UID 与 `AccountMeta.venue_uid` 比对;逐账户自检 kill 位 | apps/hft-core、hft-risk、hft-refdata |
| C10 | housekeeping 写 `Timer{PersistLag|DiskLow}` Input → 条件型 cap `Storage`(禁开;超阈值 Drain) | hft-sequencer、hft-risk、apps/hft-core |
| C11 | Drain 等待全部 Open/Hedging 链接结束、全部 IOC 终态、HedgeRetry 计时器清空 | hft-sequencer |
| C12 | `VenueCaps.ignorable_user_events`;未知事件计数不当解码失败 | hft-venue-proto、hft-venue、venue-* |
| C13 | Binance 全仓资金费推送不带 symbol(待核实)→ `funding_settle = BalanceOnlyPush` | venue-binance |
| C14 | 腿顺序按 §3.1 规则 | hft-exec |
| C15 | `VenueNotice` 命令(§8);`Funding` 事件携带实际结算周期,`hft-strat-lib` 的 FundingStats 检测变化 | control-plane、hft-strat-lib |

评审子报告中的 P2(第二腿余数按 link 而非 group、"强制向上取整到 lot"与 hft-types"不向上凑"矛盾、对冲定价不检查 mid 年龄、日亏额度的 wallet 来源、Gate 晚到手续费 IdTable 已删除、QueryResult avg_px 为 0)由对应归属文件顺手修正。

## 10. 文档矛盾清单(D 类,按归属文件修正)

| # | 主题 | 修正 |
|---|---|---|
| 1 | `hft-rt` 的 `role: Primary/Canary` 与 `create(..,role)` | 删除 role |
| 2 | `hft-journal` 的 `instance "prod"|"shadow"` | 删除 "shadow" |
| 3 | `现状测量` §5.1 "cdex 与新框架同 placement group 通信 < 50 µs" | 删除过渡接口说法 |
| 4 | `待测量`、`现状测量` 的 "CEX-DEX 维持现有做法" | 改为与 R8-1 一致 |
| 5 | `runbooks/bsc-node.md` 的"第 9–12 周"、新钱包;`mock-exchange` 引用 "11 §2" | 删周次;bsc-node 标 optional |
| 6 | `12` 章 RunMode 含 Armed | 改为 Shadow/CloseOnly/Live |
| 7 | `architecture.md` "c8 一任务一线程"、persist "独占一个 warm 核" | 按 hft-core 线程计划(persist 独占线程) |
| 8 | `architecture.md` "每 tick 走到加密前一步 + scratch 预热" | 改为可选(默认关) |
| 9 | `01` 章环优先级"控制>回报>advice>行情"、housekeeping 做持久化 | 改为 ctl > 回报 > bbo > advice > md;persist 独占线程 |
| 10 | `latency-budget` metal 按 Onload 验收 | 改为 bake-off 实测 |
| 11 | dev2 hft-core 内存 600 MB vs 900 MB | 唯一归属 `apps/hft-core`,其他处链接 |
| 12 | dev2 整机预算 4.0 GB > 3.8 GB | hft-core 重算并加 dev2 护栏(§12) |
| 13 | c8 journal 段 20 GiB > 16 GB 物理内存 | 段大小按主机类在 `apps/hft-core` 定义(唯一归属),journal-format 与 hft-journal 只写公式并链接 |
| 14 | `现状测量` "dev2 放不下 journal" | 改为 tmpfs 小段 |
| 15 | chain journal 存储方式 | chain 已 optional,journal-format 标明 |
| 16 | `crates/AGENTS.md` chain-evm 依赖 | 按 hft-chain-evm README |
| 17 | pipeline 依赖 hft-amm | 删除 |
| 18 | testkit 用 hft-sim / "最小撮合器"缺失 | 共用撮合器(§7) |
| 19 | benchmarks 把 c8 连续 7 夜写成门槛 | 延迟验收只在升级后(M7) |
| 20 | hft-refdata "Python 流水线生成" | hft-cfg 是唯一来源 |
| 21 | hft-py wheel 只出 x86_64 | aarch64 |
| 22 | venue-gate 期货"主用" | Gate 已 optional;现货优先 |
| 23 | reconciler 对 Gate 现货 | 按 VenueCaps 覆盖各 core venue 现货 + 永续 |
| 24 | C 的范围(round7 vs strategies) | 由 R8 的策略拆分取代 |
| 25 | Shadow 7 天 vs 14 天 | 上线门槛改为"模拟账户 ≥ 14 天"(§12) |
| 26 | 小额实盘范围含 A | A 已 optional |
| 27 | Shadow 期共用账户 | 无共享账户(R8-4),删除相关风险 |
| 28 | 二次确认两套协议 | PREVIEW + `expect_state_version` |
| 29 | incident-response "watchtower 已全局 CloseOnly" | watchtower 只告警;改为 Storage cap(C10)或人工 |
| 30 | watchtower `ReconcileMismatch` 条件 | 见 C3 |
| 31 | hft-event:RejectBits 归属、"L2Batch 以后再加"、SegmentRoll 是 Output 还是 Meta | RejectBits 在 hft-types;L2Batch 已定义;SegmentRoll 为 Meta(hft-sequencer 同步改) |
| 32 | maxBlock 默认值 2 vs 3 | chain optional;以 hft-chain-evm(3)为准,SDK 的 SwapIntent 链接过去 |
| 33 | `06` 章对冲 ≤ 1 ms vs 00 章 ≤ 20 µs;defensive-patterns "公共 RPC 不在执行路径" | 统一为 ≤ 20 µs;free profile 说法改为"optional 链上" |

## 11. 事实的唯一归属(本轮涉及的)

| 事实 | 归属 |
|---|---|
| SDK 接口、意图类型、目标意图、视图类型、运行模式下的意图行为表 | `crates/hft-strategy-sdk/README.md` |
| 执行算法、腿顺序、对冲量上限、默认减仓 | `crates/hft-exec/README.md` |
| `VenueCaps`、`AccountMeta`(含 kind/uid/bound_instance) | `crates/hft-refdata/README.md` |
| 事件体(Advice、StrategyParamsHdr、GroupSpec、ExternalFill、MarginReport、AccountConfigReport、ExecDecision) | `crates/hft-event/README.md`(event-model.md 列编号与分类) |
| 控制命令与权限、角色 | `docs/subsystems/control-plane.md` |
| cap 与有效模式、MarginStress 档位、启动闸门位 | `crates/hft-risk/README.md` |
| 权益视图数据、净量、余额基线 | `crates/hft-ledger/README.md` |
| 模拟撮合与模拟账户 | `crates/hft-sim/README.md` |
| 主机类、线程计划(含 sim-gw、advisor 承载 Model)、dev2 内存与 journal 段大小、dev2 护栏 | `apps/hft-core/README.md` |
| 上线门槛 | `docs/testing.md` §8 |
| 仓库与 CI、发布 | `deploy/README.md`、`docs/development.md` |
| 首个策略的规则与参数 | `strategies/portfolio-carry/README.md` |

## 12. 开发与上线(`apps/hft-core`、`deploy`、`docs/development.md`、`docs/testing.md`、`runbooks/launch.md`)

- **仓库**:M1 开始时把 `E:\trading\hft` 复制到 `E:\trading` 之外的新目录(具体路径在 M1 定),建私有 GitHub 仓库。本地只写代码,不要求能编译运行;**CI(aarch64 Linux)是唯一构建与测试环境**;服务器只安装 CI 产出的发布制品,**不在服务器编译**。不用 WSL2。
- **dev2 护栏**(现有服务器同时跑 cdex 正式账户):`hft.slice`(systemd)限 `CPUQuota`、`MemoryMax`、`MemorySwapMax=0`、`Nice`、`IOWeight`;线程计划 `dev2` 不忙等(SpinThenPark/park);journal tmpfs 段大小上限;守护脚本在 cdex 健康异常(PSI、空闲内存、cdex 心跳)持续超阈值时停止 hft 进程并告警;cdex 优先。具体数值"待标定(M3 前实测)"。
- **上线门槛**(`docs/testing.md` §8,取代"迁移前门槛"):① 单元/属性;② 金样:Binance、OKX、Bitget 的现货与 USDT 永续每种订阅消息、每种回报、每个"结果未知"码;③ fuzz 每个 core 解码器 24 h;④ DST 每夜 ≥ 50 000 种子连续 7 夜;⑤ 恢复与混沌;⑥ 交易所测试网/模拟盘(有则用,逐所待核实)连续 3 天冒烟;⑦ **模拟账户**在实时行情上连续运行 ≥ 14 天、每日报告、无 P0、无未解释 P1、`replay verify` 每夜通过;⑧ **解耦门槛**:CI 中新增第二个测试策略,引擎 crate diff 为零;⑨ 新子账户小额实盘 ≥ 20 笔(单笔上限在上线时按最小名义设定),账本与交易所一致、无裸腿;⑩ 延迟验收只在统一升级主机后(M7);⑪ 运维:ops_bot 工作流(含老板 Viewer)、kill 与死手演练。删除"与旧引擎离线信号对照"。
- **launch.md**:新子账户开通(交易 key 无提现权限 + IP 白名单、`venue_uid` 登记、账户配置核验)→ 模拟账户运行 → 小额实盘 → 逐步放量;回退 = Drain。

## 13. 链接映射(被拆/移的文件)

| 旧链接目标 | 新目标 |
|---|---|
| `crates/hft-strategies/README.md`(接口、意图、MarketState/Ctx、模式行为表) | `crates/hft-strategy-sdk/README.md` |
| `crates/hft-strategies/README.md` 的 C 节 | `strategies/portfolio-carry/README.md` |
| `crates/hft-strategies/README.md` 的 D、E 节 | `strategies/cdex/README.md` |
| `crates/hft-strategies/README.md` 的 A 节 | `strategies/cross-taker/README.md` |
| `crates/hft-strategies/README.md` 的 B 节 | `strategies/maker-taker/README.md` |
| `hft-strat-lib` 的 Sizer、ClosePolicy、FundingGate、SwapPicker、SpreadStats(72 h cdex 口径) | `strategies/cdex/README.md`(通用分位统计留在 hft-strat-lib,名为 `PercentileStats`) |
| `docs/runbooks/migration.md` 作为"上线流程" | `docs/runbooks/launch.md`(migration 保留,标 optional) |

相对路径:从 `hft/strategies/<x>/README.md` 到设计文档是 `../../../architecture-v2/…`,到 crate 是 `../../crates/<y>/README.md`,到 docs 是 `../../docs/…`。

## 14. 工作分配(只改自己清单里的文件)

| 代号 | 文件 |
|---|---|
| W1 | `architecture-v2/README.md`、`00`–`13` 全部章节(13 追加 §14 第八轮摘要)、`现状测量与升级分析-2026-10-01.md`、`待测量与待决定事项-2026-10-01.md` |
| W2a | 新建 `hft/crates/hft-strategy-sdk/README.md`、`hft/strategies/AGENTS.md`、`hft/strategies/portfolio-carry/README.md`;重写 `hft/docs/cookbook/adding-a-strategy.md` |
| W2b | `hft/crates/hft-strat-lib/README.md`;新建 `hft/strategies/cdex/README.md`、`hft/strategies/cross-taker/README.md`、`hft/strategies/maker-taker/README.md` |
| W3a | 新建 `hft/crates/hft-exec/README.md`;`hft/crates/hft-oms/README.md` |
| W3b | `hft/crates/hft-risk/README.md`、`hft/crates/hft-ledger/README.md`、`hft/crates/hft-sequencer/README.md` |
| W4 | `hft/crates/{hft-types,hft-event,hft-refdata,hft-ctl-proto,hft-ctl}/README.md`;`hft/docs/subsystems/{README,event-model,control-plane,journal-format,external-venue-protocol}.md`;`hft/docs/cookbook/{adding-an-event-kind,adding-a-control-command}.md`;`hft/tools/README.md` |
| W5 | `hft/crates/{hft-venue-proto,hft-venue,hft-net,venue-binance,venue-okx,venue-bitget,venue-gate,venue-bybit,venue-kucoin,venue-hyperliquid}/README.md`;`hft/docs/cookbook/adding-a-venue.md` |
| W6 | `hft/crates/{hft-sim,hft-testkit,hft-telemetry}/README.md`;`hft/apps/{hft-core,replayer,reconciler,watchtower,kill-switch,journal-archiver,mock-exchange}/README.md`;`hft/docs/testing.md`;`hft/benchmarks/{README,AGENTS}.md`;`hft/fixtures/README.md` |
| W7a | `hft/python/{AGENTS.md,ops_bot/README.md,pipeline/README.md,treasury/README.md,research/README.md}`;`hft/deploy/{README,AGENTS}.md`;`hft/docs/development.md`;`hft/docs/runbooks/*.md`(含新建 `launch.md`);`hft/docs/postmortem/README.md`;`hft/.agents/notes/**`(新 ADR + 索引 + 旧 ADR 状态行) |
| W7b | `hft/{AGENTS.md,CLAUDE.md,README.md}`;`hft/docs/{AGENTS.md,architecture.md,glossary.md,defensive-patterns.md,hot-path-rules.md,latency-budget.md}`;`hft/crates/AGENTS.md`;`hft/apps/AGENTS.md`;`hft/docs/cookbook/adding-a-crate.md`;`hft/crates/{hft-journal,hft-rt,hft-book,hft-py,hft-chain-evm,hft-ext-proto,hft-amm,hft-chain-solana}/README.md`;`hft/apps/{chain-core,chain-grpc-relay}/README.md`;`hft/contracts/README.md`;`architecture-v2/_work/check_hft.py` |

W7a 新建的 ADR(`hft/.agents/notes/proposed/architecture/2026-10-02-*.md`,按 notes 的 AGENTS.md 格式):`ccex-first-scope`、`strategy-sdk-decoupling`、`exec-layer-targets`、`sim-accounts-not-backtest`、`venue-caps`、`margin-stress`、`one-account-per-strategy-instance`;并在受影响的旧 ADR(`chain-core-free-profile`、`build-test-migrate`、`one-exchange-account-one-system` 等)加一行状态说明(被哪条取代或收窄),更新 notes 索引。

W7b 更新 `check_hft.py`:组件清单(新增 hft-strategy-sdk、hft-exec、strategies/*,移除 hft-strategies)、`scope` 合法值、额外检查(Canary/实例位残留、"第 N 周"、引擎 README 出现策略族名、`hft-strategies` 链接残留)。

## 15a. 主控补充裁决(修订过程中)

1. **L1 引擎 crate 彼此不依赖**(保留 `crates/AGENTS.md` 原规则,取代 §3 中"L1 可依赖 SDK 以外的 L1"的含义):`hft-exec` 只依赖 L0 + `hft-strategy-sdk`,经 SDK 视图类型读取 OMS/ledger 状态,由 `hft-sequencer` 装配;它产出的下单请求由 sequencer 交风控 → OMS。L1 crate 依赖 SDK(为视图与意图类型)是允许的。
2. **ops_bot / ctl 角色**统一为 `Admin`、`Trader`、`Viewer`(原 hft-ctl-proto 的 Observer/Trader/Ops 改名);老板 = Viewer。
3. **参数规范编解码**放在 `hft-strategy-sdk` 的纯模块 `params`;`hft-cfg` 与 `hft-py` 包装它;`hft-cfg schema export` 导出各策略 schema(JSON)供 Python 使用;`hft-event` 只定义线上头 `StrategyParamsHdr`/`GroupSpec`,body 视为字节。
4. **锚点**:上线门槛 = `docs/testing.md` 的 `## 8. 上线门槛`(`#8-上线门槛`);dev2 内存与 journal 段 = `apps/hft-core` 的"内存与 journal 段(按主机类唯一归属)"节(`#内存与-journal-段按主机类唯一归属`);11 章新节号:§1 交付范围、§3 里程碑、§4 上线门槛。
5. **策略 crate 目录约定**由 `strategies/AGENTS.md` 定义(快速部分、Model、参数 schema 的模块划分);tier:SDK = pure,`strategies/*` = hot。
6. **`VenueId` 按产品线**(一个交易所的现货与 USDT 永续是两个 VenueId,`VenueMeta.product` 给出 MarketKind),所以 `VenueCaps` 以 `VenueId` 为键,SDK 视图为 `caps(v: VenueId)`;现货的持仓模式、保证金模式、资金费推送、保证金数据用显式 `NotApplicable`;`VenueCaps` 增加 `required_multi_asset: Option<bool>`。
7. **未匹配成交**:gw 暂存无法归属到槽的成交,超时后发 `ExternalFill{cause: Unknown}`;若之后经查询归属到槽,OMS 与 ledger 按 `(venue, account, exch_order_id, trade_id)` 去重,不重复入账。
9. **策略注册表唯一**:新 crate `strategies/registry`(`hft-strategy-registry`)只含一处 `strategy_set!` 调用与 `export_schemas()`;`apps/hft-core`、`apps/replayer`、`hft-testkit` 依赖它。新增策略 = 新 crate + 注册表一行。发布制品附带 `hft-core schemas --out <dir>` 导出的 schema JSON;`hft-cfg`、`hft-py`、Python 只读 JSON 并只链接 SDK 的 `params` 编解码,不链接任何策略 crate。
10. **Model 状态**:Advice 是 Input,回放与验证不需要 Model 状态;Model 状态只用于重启后免预热。advisor 按间隔为每个策略实例写 **model checkpoint 文件**(journal 之外,原子改名,头含 kind_id/schema/二进制版本 + 已消费的最后 seq);重启 = 读 checkpoint + 从该 seq 重放 journal;缺失或版本不符 → Model 从空起步(样本不足)。热段保留须覆盖 checkpoint 间隔。
11. **合并时的修正**(主控):`Timer` 记录 16 B → 24 B,新增 `group: GroupId`(Exec 与 Strategy 定时器使用;`GroupId` 是 u32,放不进 32 位 `timer_id`),快照节 8 `TimerRow` 行版本 2;OMS 新增 `force_unknown(venue, account)`、`sim_reset(account)`,hft-exec 新增 `clear_account(account)`;`IDENTITY` 闸门由 Input `Timer{Sys, KillSelfTestOk}` 驱动;SDK `RiskStateView` 增加 `max_hedge_notional`、`max_reduce_notional`,`decode`/`validate` 在 gateway warm 线程运行、p99 ≤ 2 ms;`GateRow` 增加 `recovery_flags`、`cfg_ok_venues`、`identity_ok_venues`(40 B);参数无效统一为 `InvalidArgs{ParamInvalid}`;hft-core 内存表加入 advisor 镜像状态(dev2 合计 800 MiB,hft.slice 1,568 MiB);hft-sim 增加"预置模拟实例池"(每个研究中的策略预置 4 个模拟实例,换参与清空不重启);规则 N3 为 SDK `model_math` 留唯一浮点例外。
8. W5 定名:`AccountConfigReport.mismatches` 位 `POS_MODE MARGIN_MODE MULTI_ASSET FEE_RULE ACCT_TYPE AUTO_BORROW KEY_PERM UID UNREADABLE`;`MarginReport` 中 `adl_quantile` 归一到 0–100(255 = 未知),`margin_ratio = maint / margin_balance`(分母 ≤ 0 时为 `Rate::MAX`);`ConnState.role = Rest`、原因 `ACCT_QUERY`。

## 15. 最终报告(简短)

1. 改了/新建了哪些文件(路径 + 大小级别)。
2. 本简报中你负责的 A/B/C/D 编号逐条:已落到哪个文件哪一节。
3. 与其他工作包可能不一致的地方(你写了什么名字或数字,别人需要同步)。
4. 你认为仍缺或有风险的地方(≤ 5 条)。
