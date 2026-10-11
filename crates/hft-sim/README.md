---
description: "模拟撮合 + 模拟账户:hft-core 的 sim-gw 任务用它在实时行情上为 Sim 账户撮合吃单、维护余额/仓位/保证金/资金费;replayer sim 与 hft-testkit 用同一撮合器;提供上线门槛的压力用例变换;不用于历史回测评估收益"
kind: crate
tier: warm
scope: core
status: impl
---

# hft-sim

## 概要

`hft-sim` 拥有**模拟交易所**:对 `AccountMeta.kind = Sim` 的账户,在真实 venue 的行情上按"下单时刻 + 模拟延迟"逐档撮合吃单,并维护该账户在每个 venue 的现金、现货库存、永续仓位、全仓保证金、资金费与强平——即模拟账户的**交易所真值**。它由两部分组成:**纯撮合核心** `SimExchange`(默认 feature,`pure`:无 I/O、无线程、不读时钟,时间全部来自调用方传入的事件时间),以及 `task` feature 下的 **warm 任务** `SimGwTask`(由 [apps/hft-core](../../apps/hft-core/README.md#线程计划) 装配到 warm 线程,不占热核)。同一个 `SimExchange` 被三处使用:hft-core 的 sim-gw(实时纸面交易)、[replayer](../../apps/replayer/README.md) 的 `sim` 模式(用录下的 journal 行情重演排障)、[hft-testkit](../hft-testkit/README.md) 的 DST 与假交易所(最小撮合器就是本 crate)。它拥有延迟模型与保证金模型的**定义与语义**(`LatencyModel`、`MarginModel`;取值表在 hft-core 配置 `[sim.latency_models]`/`[sim.margin_models]`);它**不拥有** `SimAccountCfg` 的类型与编码(在 [hft-refdata](../hft-refdata/README.md) 的 `AccountMeta`,两个模型字段只是上述表的下标)、引擎侧的账本([hft-ledger](../hft-ledger/README.md))、报告的计算([hft-telemetry](../hft-telemetry/README.md) 的 `account_report`)与控制命令([control-plane](../../docs/subsystems/control-plane.md))。**它不用于历史回测评估收益**:模拟账户在实时行情上运行,重演只用于排障、复现与压力用例。

**适用范围**:`sim` 实例的线程都不 `Spin`(毫秒级 park,[hft-core 线程计划](../../apps/hft-core/README.md#线程计划)),用自己的行情连接,下单与回报延迟取抽样模型(建议初值 2 ms + 2 ms)。因此它**评估不了**依赖 10–20 ms 跨所窗口的吃单类策略的成交与收益(到达时刻、竞争与队列都不真实),只适合验证以分钟级调仓、吃单完成对冲的策略的规则、风控路径、资金口径与保证金行为;研究吞吐受实时限制(每组参数要连续运行数周,dev2 上每个研究中的策略只有 `sim_pool` 个模拟实例)。同一段已录行情上两组参数的相对比较用 [replayer `sim --ab`](../../apps/replayer/README.md#参数相对比较--ab默认关闭)(默认关闭,只给差值)。

## 目录

- [使用本组件](#使用本组件)
  - [模拟账户是什么](#模拟账户是什么)
  - [纯核心 API](#纯核心-api)
  - [sim-gw 任务(`task` feature)](#sim-gw-任务task-feature)
  - [重启恢复](#重启恢复)
  - [配置](#配置)
  - [产生的事件](#产生的事件)
  - [拒单规则](#拒单规则)
  - [报告字段](#报告字段)
  - [预置模拟实例池](#预置模拟实例池)
  - [压力用例(上线门槛必过项)](#压力用例上线门槛必过项)
  - [与正式实盘并行校准](#与正式实盘并行校准)
- [实现说明](#实现说明)
- [延迟与资源契约](#延迟与资源契约)
- [测试与验收](#测试与验收)
- [依赖](#依赖)
- [延伸阅读](#延伸阅读)
- [已知限制与后续工作](#已知限制与后续工作)

## 使用本组件

### 模拟账户是什么

- **模拟是账户类型,不是 `RunMode`**。策略在 Sim 账户上以正常的 `Shadow/CloseOnly/Live` 运行,风控、执行层、OMS、ledger 的代码路径与实盘完全相同;sequencer 按 `AccountMeta.kind` 把该账户的 `OutMsg` 路由到 `out-sim` 环而不是某个 `out-<venue>`。
- 一个 Sim 账户与 Live 账户一样跨多个交易所,**每个交易所至多一个 venue**(同一账户在同一交易所只用一个市场,refdata `SameExchangeMarkets`,`SimExchange::new` 再查一次);它在 venue V 上的订单对 **V 的真实行情**撮合(行情由所在 hft-core 实例的 feed 订阅提供,不为模拟另开连接)。
- 回报是与实盘**同一组事件**(`OrderEvent`、`FundingSettle`、`MarginReport`、`ExternalFill`…,不加新 kind),经 `rep-sim` 环进入 sequencer 并记为 journal 的 Input——回放确定,`replayer verify` 对含 Sim 账户的 journal 同样逐字节成立。`rep-sim` 是 sequencer 输入环中**优先级最低**的一个,每轮至多 `sim_max_per_round` 条,同一事件派发时实盘实例先于模拟实例(优先级、规则与该参数的取值都归 [hft-sequencer](../hft-sequencer/README.md),参数在 EngineConfig 的 ECF1 段 5);实盘繁忙时模拟回报晚于其 `due_tsc` 交付,滞后计入 `sim_tail_lag_ms`。
- kill 对 Sim 账户与实盘相同,经 [hft-risk](../hft-risk/README.md) 的检查链生效(Sim 账户不占 kill 字位,没有 gw);本 crate 不做 kill 判定。
- **放在哪个实例**([hft-core](../../apps/hft-core/README.md#实例角色prod-与-sim)):Sim 账户只出现在 `sim` 实例(配置 `instance.name = "sim"`:只含 Sim 账户、不持任何交易所 key、自己的行情连接与 journal,同一二进制),sim-gw 是其中一个 warm 任务;`prod` 实例从不含 Sim 账户。dev2 上常驻的就是 `sim` 实例;统一升级后 `sim` 实例与 `prod` 实例同机运行,只用非隔离核、在自己的 slice、网卡(ENI-C)与出口 IP 下(隔离规则归 hft-core)。
- Sim 账户不持有交易所 key;[reconciler](../../apps/reconciler/README.md) 与 [kill-switch](../../apps/kill-switch/README.md) 跳过它;ledger 与模拟真值的一致性由 sim-gw 周期推送的真值报告与 watchtower 的 `SimDrift` 规则检查。
- 可同时开多个 Sim 账户(不同策略参数各绑一个账户,一个账户只绑定一个策略实例);账户之间互不影响:各自的余额、仓位、已吃流动性记忆与随机数子流,只共享只读的簿副本。

### 纯核心 API

```rust
/// 纯核心:无 I/O、无线程、不读系统时钟;状态全部定容预分配、可哈希。
pub struct SimExchange { /* 每 Sim 账户一个 SimAccount;每 (venue, inst) 一个簿副本;待撮合指令堆;待交付回报堆;RNG 子流 */ }

impl SimExchange {
    /// 能力表(`VenueCaps`)、费率行(`Fees`/`FeeOverrides`)、限速与分档维持保证金率都取自 `rd`;启动回报
    /// (每账户 `ConnState{Up}`、每 (账户, venue) `AccountConfigReport`、首轮真值报告)以 `due_tsc = 0` 入堆。
    pub fn new(setup: &SimSetup, rd: &RefData) -> Result<SimExchange, SimCfgErr>;
    /// 行情与时间推进,按 journal 顺序送入:Bbo、L2Snapshot{origin = TopN}(分档深度,整替换)、Trade、Funding、Mark,
    /// 启用增量深度时另有 L2Batch 与多段 L2Snapshot;`VenueNotice` 控制命令;以及任意记录头(只取 tsc 推进时间)。
    pub fn on_record(&mut self, hdr: &RecordHeader, ev: SimInput<'_>);
    /// out-sim 环上的指令;decided_tsc 取该指令对应 OrderDecision 的 decided_tsc(见 sim-gw 任务第 2 步)。
    pub fn on_command(&mut self, cmd: &GwCmd, decided_tsc: Tsc);
    /// 已消费到的事件时间:执行 arrive_tsc < t 的指令、资金费结算、保证金检查。
    pub fn advance(&mut self, t: Tsc);
    /// 交付 due_tsc ≤ now 的回报,按 (due_tsc, gen_seq) 升序。
    pub fn drain_due<F: FnMut(SimReply<'_>)>(&mut self, now: Tsc, sink: F);
    pub fn next_due(&self) -> Option<Tsc>;
    pub fn pending_replies(&self) -> usize;
    /// 模拟真值视图(DST 不变量、SimDrift 规则、测试用):余额、仓位、保证金、强平价、订单表、记忆、Sim 计数。
    pub fn account(&self, a: AccountId) -> Option<SimAccountView<'_>>;
    pub fn counters(&self) -> SimCounters;
    /// 从恢复后的引擎状态初始化该账户的模拟真值(已吃流动性记忆为空):hft-core 重启与 replayer sim 从快照起步共用,见"重启恢复"。
    pub fn seed_account_from(&mut self, a: AccountId, s: &SimSeedState) -> Result<(), SimCfgErr>;
    /// 压力用例的 GwDown 故障:在事件时间 t 把 (a, venue) 的模拟下单连接置 Down/Up(产出 ConnState),
    /// Down 期间到达的订单类指令按实盘 gw 在下单连接 Down 时的同一回报形态处理(形态归 hft-venue)。
    pub fn set_order_conn(&mut self, a: AccountId, venue: VenueId, up: bool, t: Tsc);
    /// DST 的外来成交(强平、ADL、人工):在当前事件时间把 inst 的 qty 按 px 记入账户 a,与一笔无手续费的吃单成交同样
    /// 结算仓位与均价、已实现盈亏和现货余额;不产出回报(推送由调用方的 venue 行为层决定)。
    pub fn external_fill(&mut self, a: AccountId, inst: InstId, side: Side, qty: Qty, px: Px);
    pub fn state_hash(&self) -> u64;
}

pub struct SimSetup {                                                 // 冷路径输入;长度由 new 按 MAX_* 校验
    pub tuning: SimTuning,
    pub tsc_hz_milli: u64,                                            // µs/ms 参数换算到事件时间(TSC)的频率:hft-core 取本次运行的 TSC 频率,replayer 取 journal 的 EngineClock 行
    pub latency_models: Vec<LatencyModel>,                            // hft-core [sim.latency_models],下标 = 出现顺序;≤ MAX_LATENCY_MODELS
    pub margin_models: Vec<MarginModel>,                              // hft-core [sim.margin_models];≤ MAX_MARGIN_MODELS
    pub accounts: Vec<(AccountId, SimAccountCfg)>,                    // ≤ MAX_SIM_ACCOUNTS
}
pub enum MarketEventRef<'a> { Bbo(&'a Bbo), L2Snapshot(L2SnapshotPart<'a>), L2Batch(L2BatchView<'a>), Trade(&'a Trade),
                              Funding(&'a Funding), Mark(&'a MarkPrice), Index(&'a IndexPrice) }
pub enum SimInput<'a> {
    Market(MarketEventRef<'a>),
    VenueNotice { venue: VenueId, kind: u8, inst: InstId },   // ControlCmd::VenueNotice;kind = 1(Delisting)使 inst(0 = 该 venue 全部)只减仓
    Clock,
}
/// RepMsg 与实盘 gw 产出的元素同构(归 hft-event);bulk = MarginReport 的 [MarginItem; n] 或 AccountConfigReport 的
/// [LeverageItem; n_lev] 字节(与 rep-sim-bulk 同一拆分)。
pub struct SimReply<'a> { pub account: AccountId, pub due_tsc: Tsc, pub gen_seq: u64, pub msg: RepMsg, pub bulk: &'a [u8] }
pub const SIM_CODE_BASE: u32 = 0x1000_0000;   // 模拟拒单的 OrderEvent.reject_code = SIM_CODE_BASE + n(n 见"拒单规则")
pub const MAX_SIM_ACCOUNTS: usize = 16;
pub const MAX_LATENCY_MODELS: usize = 16;
pub const MAX_MARGIN_MODELS: usize = 16;
pub const CMDS_PER_POLL: usize = 256;          // sim-gw
pub const REPLY_RESERVE: usize = 4_096;        // sim-gw
```

- **Sim 账户的 venue 集合** = refdata `AccountMeta.venues` ∪ `initial_balances` 中出现的 venue(refdata 加载已保证后者是前者的子集);`SameExchangeMarkets` 按这个集合判定。
- 泛型使用,不做 trait object;`SimExchange` 不依赖 hft-sequencer、hft-oms、hft-ledger、hft-risk——它是**独立实现的交易所真值**,ledger 的缺陷不会被模拟同样复制。
- `SimCfgErr`:`UnknownVenue`;`UnknownAsset`(`initial_balances` 引用 refdata 不存在的资产);`FeeTier`(refdata `Fees` 中没有 (venue, `fee_tier`) 这一行);`LatencyModelIndex` / `MarginModelIndex`(下标从 0 起,超出对应表);`BadDist`(`Empirical` 分位点不单调);`MarginMode`(该 venue 的 `VenueCaps.required_margin_mode` 不是全仓——模拟只实现全仓);`SameExchangeMarkets`(同一 Sim 账户的 venue 集合含同一交易所的两个 VenueId);`TooManyAccounts`(超过 `MAX_SIM_ACCOUNTS`);`MmrTiersMissing`(压力用例要求的分档维持保证金率在 refdata 中缺失,只由 `StressTransform::new` 返回)。hft-core 启动步 2 用 `SimExchange::new` 干校验,任一错误退出 2。

### sim-gw 任务(`task` feature)

```rust
pub struct SimGwTask {
    ex: SimExchange,
    cmds: hft_rt::Consumer<GwCmd>,      // out-sim:sequencer → sim-gw
    rep: hft_rt::Producer<RepMsg>,      // rep-sim:sim-gw → sequencer(回报优先级)
    rep_bulk: hft_rt::ByteProducer,     // rep-sim-bulk:MarginReport 条目与 AccountConfigReport 杠杆条目(与实盘 gw 同一机制)
    tail: hft_journal::JournalReader,   // 同进程尾随 seq journal(预映射段槽位,与 advisor 相同)
    decisions: DecisionTable,           // (account, slot, gen) → decided_tsc,容量 4096,按 seq 淘汰
}
impl hft_rt::PollTask for SimGwTask {
    /// 每次调用工作量 ≤ 200 µs;从不阻塞或睡眠。
    fn poll_once(&mut self, now: Tsc) -> bool;
    fn next_deadline(&self) -> Option<Tsc>;   // 最早的待交付回报
}
```

`poll_once` 的固定顺序:

1. 尾随 seq journal 到 `committed_len`(每次 ≤ 4,096 条):行情记录 → `on_record`;Sim 账户的 `OrderDecision` → 记入 `decisions`。
2. 取 `out-sim` 指令(每次至多 `CMDS_PER_POLL` = 256 条):订单类以 `decisions` 中同 `(account, slot, gen)` 的 `decided_tsc` 调 `on_command`(`OrderDecision` 在 `OutMsg` 入环之前已提交,[journal-format 提交游标](../../docs/subsystems/journal-format.md),所以第 1 步之后一定查得到);模拟命令(`SimReset`、`SimTransfer`,`GwCmd` 变体名以 [hft-event](../hft-event/README.md) 为准)以已消费的最后一条记录的 `tsc` 为时刻;维护类(`DeadManRefresh`、`Rotate`、`ResetRate`、`Resubscribe`、`SetEpoch`、`SessionKeepalive`、`MirrorRestore`)在模拟交易所没有对应状态,取出即丢弃、不回报、只计数 `maint_ignored`。
3. `advance(已消费最后一条记录的 tsc)`。若 `tuning.quiet_ms` 内 journal 无新记录而有指令等待,以本地 TSC 推进并计数 `quiet_advance`(这类撮合在重演中不保证逐笔一致)。
4. `drain_due(now)` → 推入 `rep-sim`;环满时留在待交付堆,下一轮重试,**从不丢弃**。
5. **回报洪峰**:待交付堆(容量 16,384)剩余 < `REPLY_RESERVE` = 4,096 项时,本次调用跳过第 2、3 步(不取指令、不推进撮合),只做第 1、4 步,计数 `reply_backpressure`;指令留在 `out-sim` 环里(环满时 sequencer 按出环满处理,规则归 [hft-sequencer](../hft-sequencer/README.md))。撮合与回报的 `due_tsc` 只取决于事件时间,跳过只推迟交付,不改变撮合结果。

**受控故障注入(`SimFault`)**:用于 Broken 处置等实盘式演练([DRL-12](../../docs/tests/drills.md))。命令由 Admin 经控制面发出(参数与校验归 [control-plane `SimFault`](../../docs/subsystems/control-plane.md#命令集),编码归 [hft-event](../hft-event/README.md#控制命令编码)),只对 `AccountMeta.kind = Sim` 的账户、只在非 prod 的 hft-core 实例上被接受(复核归 [hft-sequencer 命令路径](../hft-sequencer/README.md#命令路径));sequencer 经 `out-sim` 下发 `GwCmd{SimFault{account, venue, fault, delay_ms}}`,sim-gw 在第 2 步收到时生效。故障按 (account, venue) 记一条,持续到同一 (account, venue) 的下一条 `SimFault`(新故障替换旧故障,`Off` 撤销)或 sim-gw 重启;生效与撤销的时刻按事件时间计(收到该指令时已消费最后一条记录的 tsc),所以 `replayer sim` 重演结果相同:

| `fault` | `delay_ms` | 效果 |
|---|---|---|
| `Off` | 0 | 撤销该 (account, venue) 的故障;原故障为 `Disconnect` 时先推 `ConnState{Up}` 再交付积压回报;当前无故障时无动作 |
| `RejectNew` | 0 | 该账户在该 venue 之后到达的新单指令在撮合前以 `Injected`(12)拒绝;撤单、改单、查询不受影响 |
| `Disconnect` | 0 | 立即向 `rep-sim` 推该账户该 venue 的 `ConnState{Down}`(`role = Order` 与 `UserStream`);故障期间该账户在该 venue 的指令一律回 `SendReport{status = NoConn}`、不撮合,已撮合的回报留在待交付堆;撤销或被替换时推 `ConnState{Up}`,随后交付积压回报 |
| `DelayReports` | 1..=60 000 | 故障期间该账户在该 venue 产生的回报 `due_tsc` 另加 `delay_ms` |

`delay_ms` 越界(`DelayReports` 不在 1..=60 000,其余不为 0)、账户不是 Sim 或 venue 不在该账户内时:不生效,计数 `sim_fault_ignored`(sequencer 已按命令路径拒绝,这里是防御)。故障状态不进快照、不由"重启恢复"重建:sim-gw 重启后全部故障清除。

sim-gw 读 TSC 只用于第 3 步的静默推进与第 4 步的交付时刻;撮合本身只依赖 journal 事件时间与种子,因此 `replayer sim` 用同一种子可逐笔复现(无 `quiet_advance` 的区间)。

### 重启恢复

hft-sim **不占快照节**,`SimExchange` 的状态也不写进 journal:模拟回报本身是 Input,恢复后的 ledger 与 OMS 已包含 journal 中全部已交付的模拟回报,所以模拟账户的交易所真值由该实例的快照与 journal 确定地恢复。干净 Drain 重启与非干净重启(崩溃、护栏停机超时、整机重启)用同一套步骤;`replayer sim` 从快照起步时用同一函数。

恢复所读的状态是 hft-core 启动步 5 结束时(快照 + 尾部重放 + `recovery_end` 之后)的引擎状态,也就是步 5 写出的 `StateSnapshot{purpose = Recovery}` 中的下列节(节号与归属见 [journal-format](../../docs/subsystems/journal-format.md#快照记录结构)):

| 模拟真值 | 来源节 | 取值 |
|---|---|---|
| 每 (Sim 账户, venue, asset) 的余额 | 节 22 `Cash` 行([hft-ledger](../hft-ledger/README.md)) | `total`(各资产总额;`locked` = 下面恢复出的挂单锁定,`free = total − locked`) |
| 每 (Sim 账户, 永续 venue) 的钱包 | 节 22 该 venue 保证金资产的 `Cash` 行 | `wallet = total` |
| 每 (Sim 账户, 永续 inst) 的仓位 | 节 1 `PositionRow` | `qty`;入场价 = `cost / |qty|`(与 hft-ledger `PerpPosView.entry_px` 同一换算) |
| 订单表 | 节 2 非终态槽([hft-oms](../hft-oms/README.md);`recovery_end` 已把它们置 `Unknown`,按 `prev_state` 判定) | `tif ∈ {GTC, PostOnly}` 且 `prev_state ∈ {Live, PendingCancel, PendingAmend}` → 挂单,剩余量 `qty − filled`,按 §8 锁定;其余(IOC/FOK,或 `PendingNew`)→ 终态记录:`filled = qty` 记 `Filled`,否则记 `Expired`,`cum_filled = filled`,`avg_px = filled_notional / filled`(`filled = 0` 时 0) |
| 每永续 inst 的下次结算时刻、费率与 mark | 节 10 `MarketLast`([hft-sequencer](../hft-sequencer/README.md)) | `next_funding_ns`、`funding_rate`、`mark_px`(§6 的 T、费率与结算用 mark 在本次运行收到新 `Funding`/`Mark` 前取这里的值) |

步骤(装配处负责调用:hft-core 在步 5 之后、步 6 启动 sim-gw 之前;replayer sim 在起点快照处):

1. `SimExchange::new(setup)`(全部账户先按 `initial_balances` 建立)。
2. 对每个 Sim 账户按 `AccountId` 升序:ledger 中该账户至少一个 venue 已初始化(`VenueBook.vflags` 含 `INIT`)→ 由上表构造 `SimSeedState` 调 `seed_account_from(a, &s)`;否则(首次启动、首条真值报告交付前崩溃)不调用,保持 `initial_balances`。
3. `seed_account_from` 的效果:余额、钱包、仓位与订单表按上表替换;已吃流动性记忆、待撮合指令堆、待交付回报堆为空;在途 `SimTransfer` 不恢复(ledger 在命令时刻已移动现金,视为已到账);该账户 RNG 子流重新取 `splitmix64(seed ⊕ account_id)` 的初始状态;`new` 入堆的启动回报(`ConnState{Up}`、`AccountConfigReport`)保留。
4. 受控故障注入的状态不恢复(重启即清除,见"受控故障注入")。重启前已撮合而未交付的回报随进程丢失;它们的成交不在 ledger 里,也不在恢复后的真值里(两边一致)。OMS 对恢复后的 `Unknown` 槽发 `Query`,模拟交易所按订单表回 `QueryResult`(终态记录 → `Filled`/`Expired`,挂单 → `New`/`PartiallyFilled`),槽随之收敛,不产生新成交;模拟账户不出现 `NotFound` 形态的恢复查询(reconciler 不覆盖 Sim 账户,`NotFound` 会使槽停在 `Unknown`)。

```rust
pub struct SimSeedState {                        // 装配处从恢复后的引擎状态填写;本 crate 不依赖 hft-ledger/hft-oms
    pub balances: FixedVec<(VenueId, AssetId, Qty), 256>,           // 节 22 Cash.total
    pub positions: FixedVec<(InstId, Qty, Px), 64>,                 // 节 1:qty、入场价
    pub orders: FixedVec<SimSeedOrder, 1024>,                       // 节 2 非终态槽
    pub funding: FixedVec<(InstId, i64, Rate, Px), 64>,             // 节 10:next_funding_ns、funding_rate、mark_px
}
pub struct SimSeedOrder { pub venue: VenueId, pub inst: InstId, pub slot: SlotId, pub gen: u32, pub side: Side, pub tif: Tif,
                          pub prev_state: u8 /* hft-oms OrderState 码 */, pub qty: Qty, pub filled: Qty, pub px: Px, pub avg_px: Px, pub exch_order_id: u64 }
```

`seed_account_from` 的错误:`UnknownVenue`(余额或仓位引用该账户 venue 集合之外的 venue)、`UnknownAsset`;同一输入两次 seed 的 `state_hash` 相同。

### 配置

`SimAccountCfg = { initial_balances, latency_model: u16, fee_tier: u8, margin_model: u16 }`(每个 Sim 账户一份;类型与编码在 [hft-refdata](../hft-refdata/README.md) 的 `AccountMeta.kind`):

| 字段 | 含义 | 来源 |
|---|---|---|
| `initial_balances` | `[(venue, asset, qty)]`:各 venue 的初始现金与现货库存;`SimReset` 回到这里 | 必填 |
| `latency_model: u16` | hft-core 配置 `[sim.latency_models]` 的下标(见下表的 `LatencyModel`) | 必填 |
| `fee_tier: u8` | 选 refdata `Fees` 中 (venue, tier) 那一行(Live 账户用档 0);费率与实盘同一张表,语义归 hft-refdata | 必填 |
| `margin_model: u16` | hft-core 配置 `[sim.margin_models]` 的下标(见下表的 `MarginModel`) | 必填 |

**模型定义**(本 crate 唯一定义;取值在 hft-core 配置的两张表里,下标 = 在表中出现的顺序,从 0 起;存在 Sim 账户时两张表各至少一项,没有默认表):

```rust
pub enum LatencyModel {
    Fixed { entry_us: u32, resp_us: u32 },
    Empirical { entry: Dist, resp: Dist },      // 分位点表,线性插值
}
pub struct Dist { pub n: u8, pub q_bp: [u16; 64], pub us: [u32; 64] }   // n ≤ 64;q_bp 严格递增、首 0 末 10 000;us 单调不减
pub struct MarginModel { pub mmr: MmrSource, pub leverage: u8, pub liq_fee: Rate, pub liq_slippage_bp: u16 }
pub enum MmrSource { RefdataTiers, Flat(Rate) }
```

| 模型 | 含义 | 建议初值 |
|---|---|---|
| `LatencyModel` | `entry` = 决策到交易所撮合,`resp` = 撮合到回报进入我们;按指令到达顺序从账户子流抽样 | `Fixed{2000, 2000}`(按 [现状测量 §3.1](../../design/现状测量与升级分析-2026-10-01.md) 的 RTT 1–1.5 ms 加交易所内部处理的保守值),**待校准**(见下) |
| `MarginModel` | 维持保证金率来源、杠杆(初始保证金 = 名义 / `leverage`)、强平费率、强平成交相对 mark 的滑点 | `{Flat(0.005), 5, 0.005, 50}`,待标定;`leverage` 取运营将为对应实盘账户在交易所设置的杠杆(设置步骤见 [launch](../../docs/runbooks/launch.md)),使模拟的保证金占用与强平距离和实盘同口径 |

`SimTuning`(全实例一份,在 [hft-core 配置](../../apps/hft-core/README.md#配置文件字段) 的 `[sim]` 表):

| 键 | 类型 | 含义 | 默认 |
|---|---|---|---|
| `seed` | u64 | 每账户 RNG 子流种子 = `splitmix64(seed ⊕ account_id)` | 必填(`SimTuning::with_seed(seed)` 是唯一构造器,无 `Default`) |
| `consume_decay_ms` | u32 | 已吃流动性记忆的线性衰减时长 | 1 000(待标定) |
| `take_avail_bp` | u16 | 每档可见量中可被我们吃到的比例(10000 = 全部;竞争折扣) | 10 000(待校准) |
| `quiet_ms` | u32 | journal 静默多久后以本地 TSC 推进 | 50 |
| `report_interval_s` | u16 | 真值报告(`PositionReport`/`BalanceReport`)周期 | 60 |
| `transfer_delay_ms` | u32 | `SimTransfer` 到账延迟 | 300 000(待标定) |
| `rate_limit` | bool | 按 refdata `RateLimitTable` 中 `Orders` 桶、范围 Ip/Uid/Account 的行计数(按到达时刻的固定窗口,到达第 10 条规则的新单计入),超限以 `RateLimited` 回拒 | true |
| `late_fee` | bool | 模拟"成交后手续费另行到达"(`FeeOnly`)的所 | false(DST 打开) |
| `maker_fill` | `TradeThrough` \| `Queue`(需 `queue-hftbacktest` feature) | 挂单成交模型 | `TradeThrough` |

改 `SimAccountCfg`(含换模型下标)= 改 refdata([tools/hft-cfg](../../tools/README.md))→ `ReplaceRefdata`(Drain + 重启);改 `SimTuning` 或两张模型表 = 改 hft-core 配置后重启。`[sim]` 全表随启动写入 journal(写法归 [hft-core 配置文件字段](../../apps/hft-core/README.md#配置文件字段)),[replayer sim](../../apps/replayer/README.md#sim) 从 journal 取同一份,重演不读当前配置文件。

### 产生的事件

sim-gw 按该 venue 的 [`VenueCaps`](../hft-refdata/README.md) 模仿交易所的推送形态,使引擎走与实盘相同的路径:

| 事件 | 何时 | 形态 |
|---|---|---|
| `SendReport{Written}` | sim-gw 取到订单类 `GwCmd`(由 `task` 层产出,纯核心不产出) | `sent_tsc` = 取出时刻 |
| `SendReport{NoConn}` | 下单连接 Down(`set_order_conn`)期间到达的订单类指令 | `due_tsc` = 到达时刻 |
| `OrderEvent` | 撮合与终态 | `OrderEventKind`(码值归 [hft-event](../hft-event/README.md)):`Ack` → 每档一条 `PartialFill`/`Fill`(累计量、均价、`fee`、`fee_asset`)→ IOC 余量 `Expired`;拒单为 `Reject`(`reject_code = SIM_CODE_BASE + n`,n 见[拒单规则](#拒单规则));`late_fee` 的延迟费用为 `FeeOnly`;`ex_ts_ns` = 撮合时所用行情记录的交易所时间 |
| `OrderEvent`(撤、改、查) | 该指令的 `arrive_tsc` | 按订单表(每账户 1,024 项:挂单与最近终结的订单,满时淘汰最早终结的记录):`Cancel` 目标是挂单 → `CancelAck`(解锁),否则 `CancelReject`;`CancelAll` 对范围内(`inst ≠ 0` 为该 inst,`inst = 0` 为该账户全部)每个挂单一条 `CancelAck`;`Amend` 目标是挂单且新价量通过拒单规则第 2–4、7–9 条 → `AmendAck`,否则 `AmendReject`;`Query` → `QueryResult`(`order_status`:挂单 New/PartiallyFilled,终态记录按其终态,订单表中没有 → NotFound;带 `cum_filled` 与 `avg_px`) |
| `FundingSettle` | 结算时刻 | `funding_settle = PerSymbolPush`:每 inst 一条;`BalanceOnlyPush`:`inst = INST_NONE` 的合计;`RateOnly`:不推送(由 sequencer 按规则合成,[hft-event](../hft-event/README.md)) |
| `MarginReport` | `margin_data = Push`:比率与上一条报告之差的绝对值 ≥ 0.01 或距上一条 1 s;`Poll{interval_ms}`:每 `interval_ms` | 保证金余额、维持、可用、比率、每 inst 强平价(按 §7 的公式算,与模拟强平的触发条件同一口径;ADL 档恒 0) |
| `ExternalFill{cause = Liquidation}` | 模拟强平 | 每 inst 一条,带强平费 |
| `AccountConfigReport` | 启动与 `SimReset` 后 | 恒与 `VenueCaps` 要求一致(`mismatches = 0`,`uid_keys = 0`:Sim 账户没有 key,`IDENTITY` 对它视为满足);永续 venue 对每个可交易 inst 一条 `LeverageItem{leverage = MarginModel.leverage}`,hft-risk 按它折算保证金 |
| `PositionReport` / `BalanceReport` | 事件时间 0(随启动回报)、此后每 `report_interval_s`,以及每次 `SimTransfer`/`SimReset` | 模拟真值,供 ledger 校验与 `SimDrift`;每个非零余额/仓位一条,上一条报告非零而现在为零的再报一次;`due_tsc` = 该事件时刻(不加 resp 延迟) |
| `ConnState{Up}` | 启动时每账户一次 | `venue = 0`、`conn = 0`、`role = Order`;`set_order_conn` 另按 (账户, venue) 推 Down/Up |

### 拒单规则

按 refdata(`InstMeta`)与 `VenueCaps` 判定,在撮合前依次检查,首个不满足即拒(`reject_code = SIM_CODE_BASE + n`):

| # | 检查 | 来源 | 原因(n) |
|---|---|---|---|
| 1 | inst 可交易(未下架、未因 `VenueNotice{Delisting}` 只减仓——只减仓时只拒增仓) | refdata、控制命令 | `NotTrading`(1) |
| 2 | 数量是 lot 整数倍且 ≥ 最小数量 | `InstMeta` | `Lot`(2) |
| 3 | 价格在 tick 上 | `InstMeta` | `Tick`(3) |
| 4 | 名义 ≥ 最小名义 | `InstMeta` | `MinNotional`(4) |
| 5 | 价格在该所价格限制带内:`参考价 × (1 − down) ≤ px ≤ 参考价 × (1 + up)`,参考价 = 永续的 mark、现货的撮合视图中间价(`InstMeta` 的交易所价格限制带,字段归 [hft-refdata](../hft-refdata/README.md#数据结构);该 inst 未配置或无参考价时跳过本项) | `InstMeta` | `PriceLimit`(5) |
| 6 | `reduce_only = true` 时该所支持;且不会增仓、当前有反向仓位 | `VenueCaps.reduce_only`、模拟仓位 | `Unsupported`(6)/ `ReduceOnly`(7) |
| 7 | `post_only` 不会立即成交 | 簿副本 | `PostOnly`(8) |
| 8 | 现货买:可用计价资产 ≥ 名义(手续费扣计价资产时含费);现货卖:可用基础资产 ≥ 数量 | 模拟余额 | `InsufficientBalance`(9) |
| 9 | 永续增仓:可用保证金 ≥ 新增初始保证金 | 模拟保证金 | `InsufficientMargin`(10) |
| 10 | 限速(`rate_limit = true`) | `RateLimitTable` | `RateLimited`(11) |
| 11 | 该 (account, venue) 处于受控故障注入的 `RejectNew`(见"受控故障注入") | `SimFault{RejectNew}` | `Injected`(12) |

各所的边界取值以 refdata 为准;与真实交易所一致性是验收项(见[测试与验收](#测试与验收))。

### 报告字段

每个账户(Sim 与 Live **同口径**,便于校准对比)每 UTC 日一行 `AccountDaily`,另按 group 分行;计算在 [hft-telemetry `account_report`](../hft-telemetry/README.md#账户报告account_report),由 [watchtower](../../apps/watchtower/README.md) 写日报、[journal-archiver](../../apps/journal-archiver/README.md) 写 `account_daily` 数据集,ops_bot 展示并对 Sim 账户标注"模拟":

| 字段 | 定义 |
|---|---|
| `pnl_total` | 期末权益 − 期初权益 − 净划入(权益按 [hft-ledger 权益视图](../hft-ledger/README.md) 的估值规则) |
| `pnl_funding` | Σ `FundingSettle`(含合成) |
| `pnl_basis` | 已对冲部分(每 group 两腿净量的较小者)的价格 PnL |
| `pnl_directional` | 未对冲部分的价格 PnL |
| `pnl_fee` | Σ 手续费(按成交时价格折 USDT) |
| `slippage_cost` | Σ (成交价 − 到达价) × 数量 × 方向;到达价 = `OrderDecision.decided_tsc` 时刻的中间价(诊断项,已含在价格 PnL 中) |
| `fill_rate` | IOC 成交量 / 下单量;另按拒单原因计数 |
| `leg_gap_p50/p99` | 第一腿成交 → 第二腿发出;第一腿成交 → 第二腿成交 |
| `naked_max`、`naked_twa`、`naked_over_s` | 裸敞口名义的最大值、时间加权均值、超过 `naked_usd` 的秒数 |
| `margin_ratio_max`、`margin_usage_avg` | 保证金率最大值;初始保证金 / 保证金余额的均值 |
| `turnover` | 成交名义合计 |
| Sim 专有 | `top_only_fills`(只有 BBO 的 inst 上的成交数)、`quiet_advance`、`liquidity_memory_hits`、`liquidations` |

### 预置模拟实例池

新增策略实例或 Sim 账户属于 refdata 变更(Drain + 重启,见 [hft-refdata](../hft-refdata/README.md))。为了在研究期频繁换参数而不重启:refdata 为每个正在研究的策略预置 `sim_pool` 个模拟实例(dev2 上 `sim_pool = 2`),每个绑定一个独立的 Sim 账户;换参数用 `SetParams`(热替换),清空重来用 `SimReset{account}`,两者都不重启。池子大小与 Sim 账户的初始资金只在计划内重启时改。池子在 `sim` 实例里(dev2 与统一升级主机都是,见[模拟账户是什么](#模拟账户是什么))。模拟实例计入该实例的 `max_instances` 与 `max_cells_total`,内存与检查点体积计入同一节的内存计划;dev2 的 `sim` 按"2 个研究中的策略 × `sim_pool` 2 × 每实例 ≤ 32 group"的缩小规模定容(取值与余量归 [hft-core 容量表](../../apps/hft-core/README.md#内存与-journal-段按主机类唯一归属)),满规模的模拟长跑在统一升级主机的 `sim` 实例上做。

### 压力用例(上线门槛必过项)

压力用例是 [testing §8](../../docs/testing.md#8-上线门槛) 门槛 ⑦ 的一部分,分两类:**门槛用例**(必过,判据只有"零强平",`l3-path` 另要求走完 L1/L2/L3 路径)与**非门槛用例**(只报告,用于发现弱点)。全部用例都在计划用于正式实盘的最终参数下运行,参数或代码变化后用最终参数重跑(计时与重跑规则归 testing ⑦)。减仓必须由引擎的保证金分档(到强平距离 `liq_dist` 的 L1/L2/L3,规则与默认值归 [hft-risk](../hft-risk/README.md))在强平之前完成。本 crate 提供确定性的行情变换、故障描述与判定;用例由 [replayer sim](../../apps/replayer/README.md#sim) 的 `--stress` 运行,引擎与模拟撮合看到的是同一份变换后的行情,故障由 replayer 按本节描述注入。

**前提**:被测账户交易的每个永续 inst 在 refdata 中有分档维持保证金率(`InstMeta` 的分档表,字段归 [hft-refdata](../hft-refdata/README.md#数据结构)),且被测账户的 `MarginModel.mmr = RefdataTiers`;否则 `StressTransform::new` 返回 `SimCfgErr::MmrTiersMissing`,用例拒绝运行(不退化为固定费率)。

```rust
pub struct StressSpec {
    pub case: StressCase,
    pub path: PricePath,
    pub scope: StressScope,                  // 门槛用例 = AllInsts(全部资产同向同幅,跨所两腿同涨)
    pub qty_scale_bp: u16,                   // 盘口量缩放:10 000 = 不变
    pub faults: FixedVec<StressFault, 4>,
    pub leverage: LeverageChoice,            // 被测账户 MarginModel.leverage 的取法
}
pub enum StressCase { OneSided30, L3Path, BasisWiden, Fast30, GwDown, ThinBook, CoreRestart, Custom }
// replayer --stress 的名字见下表;Custom = replayer --stress-spec 从文件读入的规格,只报告,不是门槛
pub enum PricePath {
    /// 价格乘以 f(t) = 1 + pct × clamp((t − t0) / ramp, 0, 1);ramp 之后保持 hold。
    OneSided { pct_bp: i32 /* +3000 = +30% */, ramp_ms: u64, hold_ms: u64 },
    /// 现货 inst 按 pct_bp、永续 inst 按 pct_bp + extra_bp 的同形斜坡(两腿价差同向扩大)。
    BasisWiden { pct_bp: i32, extra_bp: i32, ramp_ms: u64, hold_ms: u64 },
}
pub enum StressScope {
    AllInsts,                // 全部已订阅 inst(各 venue 同向同幅)
    Asset(AssetId),          // 基础资产为该资产的 inst(跨 venue 同向同幅)
    Venue(VenueId),          // 只有该 venue 的 inst 变价,其他 venue 的行情原样通过(跨所两腿的价差随之扩大)
}
pub enum StressFault {
    /// t0 + at_ms 起被测账户在 venue 的模拟下单连接 Down dur_ms(SimExchange::set_order_conn);
    /// venue = None 时取起点快照中被测账户永续名义最大的 venue。
    GwDown { venue: Option<VenueId>, at_ms: u64, dur_ms: u64 },
    /// t0 + at_ms 处引擎按崩溃重启:丢弃未提交尾部、从最近快照恢复;down_ms 内引擎收不到行情
    /// (模拟交易所照常推进,可能强平);恢复后按启动规则继续(Recovery 是否自动解除与实盘相同,见 apps/hft-core 启动步 8)。
    CoreRestart { at_ms: u64, down_ms: u64 },
}
pub enum LeverageChoice {
    Operating,   // 运营将为对应实盘账户在交易所设置的杠杆
    RiskCap,     // RiskParams.max_perp_leverage_x100 允许的上限,向下取整到整数倍
}
pub struct StressTransform { /* spec、t0、step_ms、每 inst 的原始簿副本 */ }
impl StressTransform {
    /// tested:被测账户交易的 inst;其中永续 inst 缺分档维持保证金率 → MmrTiersMissing。
    pub fn new(spec: &StressSpec, t0: Tsc, step_ms: u32 /* 默认 1_000 */, rd: &RefData, tested: &[InstId]) -> Result<Self, SimCfgErr>;
    /// 输入一条录下的行情记录,输出 0..n 条变换后的记录:scope 内 inst 的 Bbo、Trade、MarkPrice、IndexPrice 的价格乘该 inst
    /// 的 f(t) 后按 tick 取整(买价向下、卖价向上,保证 bid < ask),scope 外的记录原样输出;分档深度快照(TopN)逐档按同一 f
    /// 变换;启用增量深度时 L2 以 step_ms 为步长,每步开头对每个已订阅 inst 输出一份按当前 f 变换的 L2Snapshot,步内增量按
    /// 同一 f 变换;qty_scale_bp ≠ 10 000 时 Bbo 与 L2 各档数量同乘并向下取整到 lot(不足 1 lot 的 L2 档删除,Bbo 至少保留
    /// 1 lot);资金费率、交易所时间不变。
    pub fn apply(&mut self, hdr: &RecordHeader, ev: SimInput<'_>, out: &mut impl FnMut(&RecordHeader, SimInput<'_>));
}
pub struct StressVerdict {
    pub case: StressCase,
    pub gate: bool,                 // 门槛用例
    pub passed: bool,               // 按下表判定;非门槛用例恒为 true,只报告
    pub liquidations: u32,          // SimExchange 产出的 ExternalFill{Liquidation} 条数
    pub min_liq_dist: Rate,         // 运行期间各 (account, venue) 的最小 |liq_px − mark| / mark(按本 crate 的真值算)
    pub first_level_tsc: [Tsc; 3],  // hft-risk 首次进入 L1/L2/L3 的时刻(从 journal 的 Note/cap 读出,0 = 未进入)
    pub deleverage_steps: u32,      // ForceDeleverage 执行步数
    pub end_exposure_frac: Rate,    // 结束时永续名义 / 开始时永续名义
    pub escalated: bool,            // l3-path:是否改用了 +45% 的第二次运行
}
```

| `--stress` 名 | 门槛 | 价格路径(ramp / hold) | 盘口量 | 故障 | 杠杆 | 通过条件 |
|---|---|---|---|---|---|---|
| `one-sided-30` | 必过 | `OneSided` +30%(1 h / 30 min) | 不变 | — | `Operating` | `liquidations = 0` |
| `l3-path` | 必过 | `OneSided` +30%(1 h / 30 min);该次运行未进入 L3 时,改用 `Operating` 杠杆、+45%(1 h / 30 min)再运行一次(`escalated`) | 不变 | — | `RiskCap` | 采用的那次运行中 L1、L2、L3 依次出现,且每次运行都 `liquidations = 0`;两次都未进入 L3 → 不通过(用例未覆盖 L3 路径) |
| `basis-widen` | 否 | `BasisWiden` 现货 +30%、永续另加 300 bp(1 h / 30 min;`extra_bp` 待标定) | 不变 | — | `Operating` | 只报告 |
| `fast-30` | 否 | `OneSided` +30%(15 min / 30 min) | 不变 | — | `Operating` | 只报告 |
| `gw-down` | 否 | `OneSided` +30%(1 h / 30 min) | 不变 | `GwDown{None, 20 min, 10 min}` | `Operating` | 只报告 |
| `thin-book` | 否 | `OneSided` +30%(1 h / 30 min) | × 0.2 | — | `Operating` | 只报告 |
| `core-restart` | 否 | `OneSided` +30%(1 h / 30 min) | 不变 | `CoreRestart{30 min, 60 s}` | `Operating` | 只报告 |

1. **起点**:从门槛 ⑦ 长跑 journal 中选一个快照,条件是被测 Sim 账户的永续名义 ≥ 其参数允许上限的 90%(没有这样的点时,先以同参数运行到满仓再取快照);参数(`StrategyParams`、`RiskParams`)取计划用于正式实盘的最终版本,`MarginModel` 的杠杆按上表、`mmr` 用 `RefdataTiers`。全部用例从同一起点运行。
2. **变换**:`t0` = 起点之后第一条行情记录;门槛用例 `scope = AllInsts`;`step_ms` 1 000;其余按上表。`StressScope::Venue(v)`(只让一个 venue 单独变价)用于策略特有的非门槛场景,例如两条腿都是永续时"某一所单独暴涨";它经 `replayer sim --stress-spec`(`case = Custom`)运行,规格文件的字段与 `StressSpec` 一一对应(venue 写协议名),`StressTransform::new` 对不存在的 venue 返回 `SimCfgErr::UnknownVenue`。
3. **运行**:`replayer sim --journal <⑦ 长跑的 journal> --from-seq <快照> --stress gate --advice recompute --accounts <被测账户> --out <dir>`(门槛用例;`--stress all` 另跑非门槛用例),变换后的行情同时喂给 sequencer 与 `SimExchange`。
4. **判定**:按上表;报告每个用例的 `StressVerdict` 全部字段与分档时间线。门槛用例出现任何一条 `ExternalFill{Liquidation}` 或 `l3-path` 未通过 → 门槛 ⑦ 不通过,先调参数(`liq_dist_*`、仓位上限、杠杆)再用最终参数重跑全部用例。非门槛用例的强平与异常写进门槛记录,由人逐条说明。
5. 同一输入与种子两次运行的 `StressVerdict` 逐字段相等(确定性)。

<a id="与实盘并行校准"></a>

### 与正式实盘并行校准

正式实盘开始后,为每个实盘账户在同机的 `sim` 实例里开一个**同参数、同 venue** 的 Sim 账户并行运行(配对在 watchtower 配置 `calib_pairs`,按 `(instance, account)` 写:实盘账户在 `prod` 实例、Sim 账户在 `sim` 实例),日报对比:

| 指标 | 实盘来源 | 模拟来源 | 目标差异(待标定) | 回写 |
|---|---|---|---|---|
| IOC 成交率 | 实盘 `OrderEvent` | 同 | ≤ 5 pp | `take_avail_bp` |
| 相对到达价滑点(中位、p90) | 同上 | 同 | ≤ 2 bp | `consume_decay_ms`、`take_avail_bp` |
| 下单 → 回报延迟 | 实盘 `LatencySample`(`order_rtt`) | `LatencyModel` 抽样 | 分位点误差 ≤ 20% | 在 `[sim.latency_models]` 追加一项 `Empirical{…}`(由实盘分布拟合),把配对 Sim 账户的 `latency_model` 下标指向它 |
| 两腿间隔 | 实盘 `leg_gap` | 同 | p50 比值在 0.5–2 | 同上 |
| 资金费 | 交易所资金费流水 | 模拟结算 | 相对误差 ≤ 1e-6 | 结算规则(见开发备忘) |

不达标时模拟结果只能用于方向性观察与回归对比,不得据此决定放量。

## 实现说明

<details><summary>实现要点</summary>

### 1. 时间

- 事件时间 = journal 记录头 `tsc`(sequencer 处理时刻,即策略看到它的时刻);交易所时间取记录体 `ex_ts_ns`。
- 指令:`arrive_tsc = decided_tsc + sample(entry)`;在**所有 `tsc ≤ arrive_tsc` 的行情记录应用之后**的簿副本上撮合(即在看到第一条 `tsc > arrive_tsc` 的记录时执行);回报 `due_tsc = 撮合时刻 + sample(resp)`;同一订单的回报按产生顺序交付(`due_tsc` 取不小于该订单上一条回报的值,`FeeOnly` 除外);交易所推送类回报(`FundingSettle`、`MarginReport`、`ExternalFill`)= 事件时刻 + `sample(resp)`。
- 同一账户的指令按 `(arrive_tsc, gen_seq)` 全序执行;延迟抽样按指令到达顺序从该账户子流取数,不受其他账户影响。

### 2. 簿副本

- 每 (venue, inst) 一个 [`hft-book`](../hft-book/README.md) `Book`:订阅分档深度的 inst 由 `L2Snapshot{origin = TopN}` 整替换维护(`apply_snapshot`);启用增量深度时由 `L2Batch`/`L2SnapshotPart` 维护;同一 inst 的 BBO 另存。
- 撮合视图:顶档取 BBO 与簿顶档中交易所时间更新者;更深档取簿中价格劣于该顶档的档位。只订阅 BBO 的 inst 只有一档可吃,超出部分按未成交处理并计 `top_only_fills`;分档深度只有前 N 档,吃穿第 N 档的部分同样按未成交处理(结论偏保守)。
- 簿不在 `Live`(重同步中)时,该 inst 的指令以 `NotTrading` 拒绝并计数(与实盘"重同步前不下单"一致)。

### 3. 吃单撮合

1. 取撮合视图;对手方向从最优档开始,直到限价。
2. 每档可得量 = `max(0, 可见量 × take_avail_bp / 10000 − 记忆剩余量(该档, arrive_tsc))`,向下取整到 lot。
3. 逐档成交 `min(剩余, 可得量)`,价格 = 档位价;每档产生一条成交回报。
4. 余量:IOC → `Expired`;FOK 总可得 < 数量 → 整单 `Expired` 无成交;可成交的 GTC 限价单余量转为挂单(只供预留的 MakerFirst)。
5. 记忆:对每个被吃的 (账户, inst, 方向, 价) 记 `{qty, t0 = arrive_tsc}`,剩余量在 `consume_decay_ms` 内线性衰减到 0;每 (账户, inst, 方向) 一个 64 项定长环,过期项回收;环满且无过期项时覆盖 `t0` 最早的一项。记忆按账户独立。

### 4. 挂单(预留给 MakerFirst)

- `TradeThrough`(默认,保守):挂买单在价 P,只有出现成交价 < P 的 `Trade` 或卖一 < P 时才成交,数量 = min(剩余, 穿价成交量);挂卖单对称。不建模排队位置。
- `Queue`(`queue-hftbacktest` feature,可选):移植 hftbacktest `rust-v0.9.4` 的 `ProbQueueModel` 族(先合入上游 PR #323),`powf/ln` 用 `libm` 保证跨平台逐位一致;源码在 `vendor/hftbacktest/`,`PATCHES.md` 列出与上游差异。触发条件:MakerFirst 执行算法立项。

### 5. 手续费与扣费资产

费率 = 该账户的 `FeeOverrides`(inst 行 > venue 行)> refdata `Fees` 中 (venue, `fee_tier`) 行的 maker/taker(`new` 时从 refdata blob 解析);`fee = 成交名义 × 费率`,按扣费资产换算后取整一次,向上取整到该资产的最小单位(支出 `Ceil`,与 [hft-types 取整规则](../hft-types/README.md#运算) 同向);扣费资产按 `VenueCaps.fee_asset_rule`:

| 规则 | 买 | 卖 |
|---|---|---|
| `Quote` | 计价资产 | 计价资产 |
| `ReceivedAsset` | 基础资产(`fee` = qty × 费率;到账量 = qty − fee) | 计价资产 |
| `ReceivedAssetUnlessDiscount{token}` | 账户 `token` 的 free 余额 ≥ 按 token 计的费用时扣 `token`(费用 = 成交名义 × 费率 ÷ 撮合时该 venue 上 `token` 现货 inst 的簿副本中间价;没有该 inst 的行情时按 `ReceivedAsset`),否则同 `ReceivedAsset` | 同左(否则扣计价资产) |
| `Configured(asset)` | 该资产 | 该资产 |

折扣不另设参数:模拟账户要模拟平台币折扣时,把它的 `fee_tier` 指向费率已含折扣的那一行。

回报只带 `fee` 与 `fee_asset`(与 venue 一样);净量由 ledger 计算(`FillDelta.net_qty`)。`late_fee = true` 时,手续费在成交回报中为 0,另在 `due_tsc + 50 ms` 发一条 `FeeOnly` 回报。

### 6. 资金费

- 结算时刻 T 取该 inst 最近 `Funding` 事件给出的下次结算时间(实际结算周期随事件变化,无需单独处理周期变化)。
- 当该 venue 第一条 `ex_ts_ns ≥ T` 的行情记录被消费时结算:金额 = −仓位 × mark(T 前最后一条 `Mark`)× 费率(T 前最后一条 `Funding` 的费率;正费率多头付),i128 中算出后取整一次到计价资产最小单位:收入 `Floor`、支出按绝对值 `Ceil`;仓位为 0 的 inst 不结算、不推送。
- 推送形态按 `funding_settle`(见[产生的事件](#产生的事件));余额变化在任何形态下都进入模拟真值。

### 7. 永续全仓保证金与强平

钱包按 **(Sim 账户, venue)** 建,每个一个、互不占用:永续 venue 是一个全仓保证金账户(本节),现货 venue 是资产余额表(§8)。同一 Sim 账户不会有同一交易所的两个 venue(`SimCfgErr::SameExchangeMarkets`),所以同所现货与永续共用钱包的情形不存在,本 crate 不建模余额域。

每 (Sim 账户, 永续 venue) 的全仓保证金账户:

- `wallet`(计价资产);`unrealized = Σ qty × (mark − entry)`;`margin_balance = wallet + Σ unrealized`。
- 仓位按平均成本记账,口径与 [hft-ledger 仓位与已实现](../hft-ledger/README.md#仓位与已实现平均成本法) 相同:减仓移出成本 = `cost × c / |qty|`(i128 一次相除,向零取整;全平时恰为 `cost`),已实现盈亏换算到计价资产最小单位时向零取整后记入 `wallet`(同一笔成交两边逐单位相等,DST 不变量 16 据此比较)。
- `maint = Σ |qty| × mark × mmr(inst, 名义档)`;`initial = Σ |qty| × mark / leverage`;`avail = margin_balance − initial − 挂单占用`。
- `margin_ratio = maint / margin_balance`(与 [权益视图](../hft-ledger/README.md) 统一口径)。
- **强平价**(写进 `MarginReport` 每 inst 条目,与下一条的强平触发同一口径):对持仓 `q_i`(带符号)的 inst i,固定其他 inst 的 mark,解 `margin_balance(m) = maint(m)` 得
  `liq_px_i = (W + U_{≠i} − M_{≠i} − q_i × entry_i) / (|q_i| × mmr_i − q_i)`,其中 `W` = wallet,`U_{≠i}`/`M_{≠i}` = 其他 inst 的未实现盈亏与维持保证金;分母为 0 或结果 ≤ 0 → 0(无强平价);结果取整到价格最小单位,空头向上、多头向下(使 mark 到达 `liq_px` 时触发条件成立)。`mmr` 用 `RefdataTiers` 时维持保证金随名义分段跳变,`liq_px` 取触发条件(下一条)成立的边界价:在以首档与末档费率求得的两个根之间对整数价格二分(空头取最小的触发价,多头取最大的触发价),确定且与触发判定同一函数。`RefdataTiers` 下未登记分档的 inst 维持保证金率按 0 计(实时模拟应改用 `Flat`;压力用例由 `MmrTiersMissing` 拒绝)。全部用 i128 定点,不经 f64。
- 每条 `Mark` 后检查:`margin_ratio ≥ 1`(判定为 `maint ≥ margin_balance`,i128 精确比较,不经比率取整)→ 该 venue 全部永续仓位按 `mark × (1 ∓ liq_slippage_bp / 10 000)`(向不利方向取整到价格最小单位)平仓,扣 `liq_fee`,每 inst 一条 `ExternalFill{Liquidation}`;之后 `MarginReport`。只有一个永续持仓时,mark 到达 `liq_px` 的那一条 `Mark` 恰好触发(属性测试),所以 hft-risk 由 `MarginReport` 算出的到强平距离(`liq_dist`,定义归 [hft-risk](../hft-risk/README.md))就是模拟真值的距离。
- 不模拟 ADL、联合保证金/多资产模式、逐仓;该 venue 的 `VenueCaps.required_margin_mode` 不是全仓时 `SimCfgErr::MarginMode`。

### 8. 现货余额与划转

- 每 (Sim 账户, venue, asset) `{free, locked}`;IOC 在 `arrive_tsc` 锁定、成交即结算;挂单锁定直到终态。锁定额 = 拒单规则第 8 条检查用的金额(现货买:限价名义,手续费扣计价资产时另加按 taker 费率算的费用;现货卖:数量);部分成交按比例释放。
- `SimTransfer{account, from_venue, to_venue, asset, amount}`:sequencer 已按 ledger 可用余额校验;sim 在命令时刻从源 venue 扣减、`transfer_delay_ms` 后在目标 venue 入账,两端各发 `BalanceReport`。
- `SimReset{account}`:撤销该账户全部挂单(各发 `CancelAck`)、余额恢复 `initial_balances`、仓位与记忆清零、发真值报告;前置条件与 ledger 侧的同步清零分别归 [control-plane](../../docs/subsystems/control-plane.md) 与 [hft-ledger](../hft-ledger/README.md)。

### 9. 确定性

- 随机数:`rand_chacha::ChaCha8Rng`,每账户子流 `splitmix64(seed ⊕ account_id)`;新增账户不扰动已有账户。
- 无 `HashMap` 迭代依赖:账户、(venue, inst)、槽位全部按稠密下标;堆按 `(time, gen_seq)` 全序。
- 同一 `(SimSetup, refdata, 输入记录与指令序列)` → 同一回报序列与 `state_hash` 轨迹,跨 x86_64/aarch64 一致。

### 10. 不变量

- 资金守恒:每 (账户, venue, asset) 的余额变化 = Σ 成交 ± 手续费 ± 资金费 ± 划转 ± 强平费,逐笔可追溯。独立核算按回报的 `gen_seq` 还原成交产生顺序;不同订单的回报延迟可使交付顺序不同,平均成本与已实现盈亏不能按交付顺序重算。交付时序仍按实际收到的顺序单独检查。
- 仓位 = Σ 成交(含强平);任何一档成交量 ≤ 可见量 × `take_avail_bp` − 记忆剩余量。
- 回报 `due_tsc ≥` 触发它的指令的 `decided_tsc`;同一订单的回报按交易所时间单调;成交总量 ≤ 下单量。

### 11. 源文件

| 文件 | 职责 |
|---|---|
| `src/lib.rs`、`src/types.rs` | `SimExchange`(构造、公开入口、回报堆)、`SimSetup`、`SimTuning`、`SimReply`、模型与错误类型 |
| `src/state.rs` | 预分配的内部行:inst(簿副本、资金费与 mark)、venue、账户(余额、仓位、订单表、记忆)、待撮合与待交付项 |
| `src/market.rs` | 事件时间推进、行情记录、撮合视图(BBO + L2 合成)、`TradeThrough` 挂单成交、资金费与 mark 触发 |
| `src/exec.rs` | 指令执行:拒单规则、吃单撮合与已吃流动性记忆、订单表、撤/改/查 |
| `src/account.rs` | 现货结算与锁定、手续费资产、永续仓位、全仓保证金与强平价、强平、资金费、划转、重置、seed、真值报告 |
| `src/models.rs` | `LatencyModel`/`Dist` 校验、延迟抽样、账户子流 |
| `src/view.rs`、`src/hash.rs`、`src/fx.rs` | `SimAccountView`、`state_hash`、i128 定点辅助 |
| `src/stress.rs`(M4) | `StressSpec`、`StressCase`、`PricePath`、`StressFault`、`StressTransform`、`StressVerdict` |
| `src/task.rs`(`task` feature,M4) | `SimGwTask`、`DecisionTable` |
| `vendor/hftbacktest/`(`queue-hftbacktest` feature) | 上游模型层原文件 + `LICENSE`(MIT)+ `PATCHES.md` |

</details>

## 延迟与资源契约

不在 tick-to-trade 路径上(warm 线程;纯核心另被冷工具与测试使用)。约束是**跟得上 journal、不抢热核**:

| 入口 | 目标 | 基准名 |
|---|---|---|
| `on_record(L2Batch)`(含簿副本更新) | ≤ 1 µs 均摊/档 | `sim/on_l2` |
| `on_command(Place)` + 吃单撮合(≤ 10 档) | ≤ 2 µs | `sim/place_match` |
| `SimGwTask::poll_once` | 单次工作量 ≤ 200 µs;sim-gw 落后 journal 末尾只记录(sim-gw 只在 `sim` 实例里运行,不设延迟验收;同机时 `sim` 实例只用非隔离核,滞后进日报) | `sim/poll_once`;指标 `sim_tail_lag_ms` |
| `replayer sim` 吞吐 | ≥ `replayer verify` 吞吐的 50% | `sim/replay_day` |

- 分配:`SimExchange::new` 之后零分配(全部定容:`MAX_SIM_ACCOUNTS`、每账户订单表 1,024、记忆环 64 项、待交付堆 16,384)。
- 内存:簿副本与 hft-book 容量相同,只为 Sim 账户实际交易的 (venue, inst) 建;sim-gw 总内存上限按主机类见 [hft-core 内存预算](../../apps/hft-core/README.md#内存与-journal-段按主机类唯一归属)。
- 系统调用:`SimGwTask` 只有环与 journal 读者的共享内存访问,无系统调用。

## 测试与验收

用例与验收见 [TESTS.md](TESTS.md)。

## 依赖

| 依赖 | 用途 | feature |
|---|---|---|
| [`hft-types`](../hft-types/README.md)、[`hft-refdata`](../hft-refdata/README.md)、[`hft-event`](../hft-event/README.md) | 定点类型、`InstMeta`/`VenueCaps`/`FeeSchedule`/`SimAccountCfg`、事件体与 `GwCmd`/`RepMsg` | 默认 |
| [`hft-book`](../hft-book/README.md) | 簿副本 | 默认 |
| `rand_chacha` | 确定性随机 | 默认 |
| [`hft-rt`](../hft-rt/README.md)(环、`PollTask`)、[`hft-journal`](../hft-journal/README.md)(只用 `JournalReader`) | sim-gw 任务 | `task` |
| `libm`、vendored hftbacktest 模型层(MIT) | 排队模型 | `queue-hftbacktest`(可选) |

禁止:依赖 [hft-sequencer](../hft-sequencer/README.md)、[hft-oms](../hft-oms/README.md)、[hft-ledger](../hft-ledger/README.md)、[hft-risk](../hft-risk/README.md)、[hft-exec](../hft-exec/README.md)、[hft-strategy-sdk](../hft-strategy-sdk/README.md) 与任何 `strategies/*`;默认 feature 下禁止 I/O、线程、`std::time`;任何 feature 下禁止 tokio。

## 延伸阅读

- [07 策略层](../../design/07-策略层.md)、[08 数据面与回放](../../design/08-数据面与回放.md)(回放 = 确定性验证与排障)
- [round8-decisions](../../design/_work/round8-decisions.md)(R8-5:不做回测系统,做模拟系统)
- [research/04 §1.2、§7](../../design/research/04-oss-frameworks-and-patterns.md)(hftbacktest 评估)、[research/06 §4](../../design/research/06-data-plane-replay-observability.md)(延迟与队列建模的陷阱)
- [apps/hft-core 线程计划](../../apps/hft-core/README.md#线程计划)、[replayer](../../apps/replayer/README.md)、[hft-testkit](../hft-testkit/README.md)、[testing.md](../../docs/testing.md)
- Agent Note:[sim-accounts-not-backtest](../../.agents/notes/proposed/architecture/2026-10-02-sim-accounts-not-backtest.md)

## 已知限制与后续工作

- **M2 实现范围**:纯核心 `SimExchange`;`task` feature(`SimGwTask`、受控故障注入 `SimFault`,`on_command` 目前忽略 `SimFault`)、压力用例(`stress.rs`)与 `queue-hftbacktest` 随 M4 落地。每个被跟踪的 inst(Sim 账户各 venue 的全部 inst)在 `new` 时分配一本 hft-book `L2Book`(窗口取 refdata `book_window`)。
- **无市场冲击**:我们的模拟单不改变真实行情;已吃流动性记忆只近似"同一档被吃掉";单笔名义相对深度大时结论偏乐观,以 `take_avail_bp` 校准与 `top_only_fills` 计数约束。
- **延迟是抽样的**:交易所内部排队与跨所时钟差不可见;校准前的模拟成交率与滑点只作方向性参考。
- **挂单不建排队位置**(默认 `TradeThrough`,偏保守);需要时启用 `queue-hftbacktest`(触发条件:MakerFirst 立项)。
- 不模拟 ADL、联合保证金/多资产、逐仓、现货借币(只做"空永续、多现货"方向);自成交保护、RPI 等交易所特殊撮合只按 refdata 声明的规则。
- 链上腿(可选扩展,链上策略迁移时启用)的模拟不在本 crate 的 core 范围;启用时另加 `chain` 模块(包含率、块延迟、真实池状态上的成交),设计沿用本 crate 的确定性规则。

### 开发备忘

<details><summary>给实现者</summary>

- **待核实**:各 core venue 的实际结算费率是否等于结算前最后一次推送的预测费率。方法:测试网/模拟盘(结算资金费的所)与正式实盘开始后实盘账户的资金费流水(Binance `/fapi/v1/income`、OKX 账单、Bitget 账单,端点见各 `venue-*`)与同时刻 journal 中最后一条 `Funding` 比对 7 天;不一致时改为结算后从流水回补(经 reconciler 校正),并在本节写明。
- **待核实**:各所价格限制带(如 Binance `PERCENT_PRICE`)与维持保证金率分档是否进入 refdata;没有时 `PriceLimit` 检查跳过,实时模拟的 `mmr` 用 `Flat` 并在 `AccountDaily` 标注,压力用例则拒绝运行(`MmrTiersMissing`)。方法:与 hft-refdata `InstMeta` 字段对照。
- **待核实(方法:Binance 官方文档与测试网账户的 BNB 抵扣开关实测)**:`ReceivedAssetUnlessDiscount` 的抵扣是否需要账户侧开关(Binance BNB 抵扣);若需要,`SimAccountCfg` 增加开关字段(改 hft-refdata)。
- 实现顺序:簿副本 + 吃单 + `Fixed` 延迟 + 现货余额(先让 testkit 的 DST 用上)→ 拒单规则 → 手续费 → 永续保证金与资金费 → `SimGwTask` → 强平 → 真值报告 → `Empirical` 延迟 → `queue-hftbacktest`(可选)。
- **待核实**:`sim` 实例与 `prod` 实例同机但各有行情连接与出口 IP,两者看到的行情时刻仍可能不同;校准表中"下单 → 回报延迟"一项用 sim 实例自己的 `rx_tsc` 口径拟合。方法:同机运行第一周对同一 inst 比较两实例 journal 的 `ex_ts_ns → rx_tsc` 分布。
- **待标定**:压力用例的起点选择(满仓 90%)与 `step_ms`;`basis-widen` 的 `extra_bp`、`gw-down` 与 `core-restart` 的故障时刻与时长(按测试网与实盘观察到的断连、重启时长更新)。
- 开放问题:多个 Sim 账户交易同一 inst 时记忆是否应共享(模拟"我们自己的多个账户互相竞争")。当前按账户独立;若多账户并行比较参数时结论因此偏乐观,再加 `shared_memory_group`。

</details>
