---
description: "测试基建:金样运行器、proptest 生成器、假时钟、SDK 策略测试桩(MockViews/StrategyHarness)、解耦门槛夹具(两个测试策略)、确定性模拟测试(DST)框架、场景套件与服务器上运行的 hft-dst 运行器、状态哈希对比;最小撮合器就是 hft-sim;只出现在 dev-dependencies、xtask 与验证包"
kind: crate
tier: dev
scope: core
status: spec
---

# hft-testkit

## 概要

`hft-testkit` 拥有全仓共用的测试机制:把 [fixtures/](../../fixtures/README.md) 的解码金样跑在任一 `VenueProtocol` 上;为 OMS/风控/执行层/sequencer 生成事件流与"乱序/重复/丢失"扰动的 proptest 策略;可手动推进的假时钟;**策略测试桩**(实现 SDK `Views` 的 `MockViews` 与单策略驱动 `StrategyHarness`);**解耦门槛夹具**(两个只依赖 SDK 的测试策略与 CI 检查);**确定性模拟测试(DST)框架**(种子驱动的假交易所 + 故障注入 × 真实的 sequencer/OMS/风控/执行层/ledger,对策略集泛型,失败打印种子)与以测试策略运行的**场景套件**;**`hft-dst` 运行器**(同一组套件编成独立二进制,随验证包在服务器上累计新种子,门槛 ④);按组件定位分歧的状态哈希工具。它不拥有业务逻辑与撮合模型:**最小撮合器就是 [`hft-sim`](../hft-sim/README.md) 的 `SimExchange`**(一套代码,实时模拟账户、`replayer sim` 与 DST 共用),本 crate 只在其外加 venue 行为层(限速、拒单码、死手、推送形态)与故障注入。它只出现在各 crate 的 `[dev-dependencies]`、`tools/xtask`(`cargo xtask dst`、`replay-verify`、`scaffold-gate`)与自己的 `hft-dst` 二进制(只进验证包,不进生产归档);**它不依赖 `hft-strategy-registry` 与任何生产策略**——需要真实策略的集成测试放在 `strategies/registry/tests/` 或 `apps/hft-core/tests/`,由它们以 dev 依赖使用本 crate。测试层级与门槛的全仓定义在 [testing.md](../../docs/testing.md)。

## 目录

- [使用本组件](#使用本组件)
  - [1. 金样](#1-金样)
  - [2. proptest 生成器](#2-proptest-生成器)
  - [3. 假时钟](#3-假时钟)
  - [4. 策略测试桩](#4-策略测试桩)
  - [5. 解耦门槛夹具](#5-解耦门槛夹具)
  - [6. DST 框架](#6-dst-框架)
  - [7. 状态哈希对比](#7-状态哈希对比)
  - [8. hft-dst 运行器](#8-hft-dst-运行器)
  - [9. 分配计数与基准驱动](#9-分配计数与基准驱动)
- [实现说明](#实现说明)
- [延迟与资源契约](#延迟与资源契约)
- [测试与验收](#测试与验收)
- [依赖](#依赖)
- [延伸阅读](#延伸阅读)
- [已知限制与后续工作](#已知限制与后续工作)

## 使用本组件

### 1. 金样

```rust
pub struct GoldenCase { pub id: CaseId, pub venue: VenueId, pub channel: &'static str,
                        pub input: Vec<RawFrame>, pub expected: Vec<OwnedEvent>, pub meta: CaseMeta }
pub fn golden_cases(venue: &str, channel: &str) -> impl Iterator<Item = GoldenCase>;
pub fn run_decoder_case<P: VenueProtocol>(p: &mut P, case: &GoldenCase) -> Result<(), GoldenDiff>;

// 每个 venue crate 的 tests/golden.rs 每个协议一行:
hft_testkit::golden_suite!(venue_binance::Usdm, "binance-usdm");
hft_testkit::golden_suite!(venue_binance::Spot, "binance-spot");
```

- 目录布局、来源规则与覆盖要求归 [fixtures/](../../fixtures/README.md);本 crate 只按该布局读取。core 套件:`binance-usdm`、`binance-spot`、`okx-swap`、`okx-spot`、`bitget-usdt-futures`、`bitget-spot`;其余 venue 在启用时加一行。
- `GoldenDiff` 给出第一条不一致的帧、事件序号与逐字段差异(定点值同时显示原值与按指数换算的十进制)。
- **没有"用当前输出覆盖期望"的开关**:金样不对就修 fixture 来源,不修归一化逻辑去迁就([testing §1](../../docs/testing.md#1-原则));期望只能由 `tools/` 的抽取工具从 raw 捕获产出并经人工复核。
- 同一运行器也跑 `fixtures/sim/` 的模拟撮合金样(`run_sim_case`:记录 + 指令脚本 → 期望回报序列)。

### 2. proptest 生成器

| 生成器 | 产出 | 主要用户 |
|---|---|---|
| `market_stream(venue, insts, n, GapPolicy)` | 合法的 core 行情序列:`Bbo`/`Trade`/`Funding`/`Mark` 与分档快照 `L2Snapshot{origin = TopN}`(整替换,无序号链);增量深度启用时(可选扩展)另有 `L2Batch`/`L2SnapshotPart`(序号连续,按策略插入缺口/重复);`Funding` 可改变结算周期;`QuietPolicy` 让某 inst 的 BBO 长时间不变而连接照常有 pong/心跳回帧,或让连接停止回任何帧、进入重同步 | hft-book、hft-venue 重同步与连接健康、hft-sim |
| `order_lifecycle(venue_rules)` | 某 venue 语义下合法的回报序列:Ack → PartialFill* → Fill/CancelAck/Expired,含 fill-before-ack、cancel-reject-because-filled、amend 竞态、只带费用的回报 | hft-oms |
| `perturb(seq, Perturb{reorder_window, dup_p, drop_p, delay})` | 对任意回报序列施加乱序(窗口内)、重复、丢失、延迟 | hft-oms、hft-sequencer |
| `control_cmds(allowed)` | 随机但类型合法的 `ControlCmd`(`SetMode`/`Kill`(含按账户 `hard`;含 Viewer 主体的紧急停止;含闸门未放行与 `DRAINING` 期间的 `Kill{flatten}`)/`Resume`/`SetParams`(含两腿同所的非法 `GroupSpec`)/`HedgeFix`/`AdoptExchange`/`SetBalanceBaseline`/`CancelAll`/`VenueNotice`/`SimReset`/`SimTransfer`/`InjectWarmup`/`StrategyCmd`/`SmokeOrder`/`WriteOffResidual`(含静止的非 Broken group)…) | hft-sequencer、hft-risk |
| `fills_with_fee_asset(rule)` | 按 `VenueCaps.fee_asset_rule` 的成交(含基础币扣费、晚到费用) | hft-ledger、hft-exec、hft-oms |
| `margin_path(account)` | 到强平距离 `liq_dist` 随 mark 变化穿越 `liq_dist_block_open`/`liq_dist_reduce`/`liq_dist_force`(含滞回 `liq_dist_hyst` 两侧)的路径;含交易所 `liq_px` 与 ledger 估算不一致、维持/保证金比率 ≥ 0.8 而 `liq_dist` 宽松、数据过期段 | hft-risk、hft-exec |
| `one_sided_move(inst, pct, over_ms, shape)` | mark 与 BBO 在 `over_ms` 内向一个方向单边移动 `pct`(`Linear`;`Steps{every_ms}`:每 `every_ms` 一次等幅跳变;`Gap{at_ms, gap_pct}`:其余部分线性,在 `at_ms` 处另有一次幅度为起始价 × `gap_pct` 的跳变;三种形状的终值都是起始价 × (1 + `pct`));作用于一个 inst,即只让它所在的一所变价,多所同向时对每个 inst 各调一次;testing MS-2 用 `pct = 30%`、`over_ms = 3 600 000`,方向取对被测账户永续净持仓不利的一侧 | hft-sim 压力用例、DST |
| `external_fills(cause)` | 强平/ADL/人工/交割成交,自身订单识别标记按 venue;可选"用户流缺口"形态:成交只改变假交易所真值、不推送,只能由 `ReconActor` 的仓位差发现;另一变体在用户流重连后补推同一笔真实成交(检验 hft-ledger 的 `auto_held` 抵扣) | hft-ledger、hft-oms、hft-exec、reconciler |
| `account_config(caps)` | `AccountConfigReport` 序列(一致、启动不一致、运行中变化,含实际杠杆变化与现货借币开启) | hft-risk |
| `funding_flows(account)` | 交易所逐笔资金费流水与本地入账(推送或合成)的配对序列,含差额、缺笔、多笔与结算时刻落在 `funding_recon_lag_ms` 内的笔 | hft-ledger、reconciler |
| 可选扩展(`chain` feature,链上策略启用时) | `chain_evidence`、`nonce_ops`、`intent_dups`、`nonce_slot_races`、`ext_link_faults`(语义见 [hft-chain-evm](../hft-chain-evm/README.md)) | hft-chain-evm |

状态机测试用 `proptest-state-machine`:参考模型(纯内存、朴素实现)与被测系统同步步进并比较不变量([research/04 §7](../../design/research/04-oss-frameworks-and-patterns.md))。

```rust
pub fn market_stream(venue: VenueId, insts: &[InstId], n: usize, gap: GapPolicy) -> impl Strategy<Value = Vec<MdEvent>>;
pub fn market_stream_with(cfg: MarketStreamCfg) -> impl Strategy<Value = Vec<MdEvent>>;   // 另含 quiet、bbo_ping_ms、tsc_hz、步间隔等
pub enum MdEvent { Bbo(Bbo), Trade(Trade), Funding(Funding), Mark(MarkPrice), L2Snapshot(OwnedL2Snapshot), Conn(ConnState) }
pub enum GapPolicy { NoSeq, Contiguous, Skips { max_skip: u32 } }          // core:每 inst 的 Bbo.update_id
pub enum QuietPolicy { None, Still { inst, from_ms, to_ms }, Silent { from_ms, to_ms } }   // 窗口 [from_ms, to_ms),0 < from_ms
pub fn perturb<T: Clone>(seq: Vec<T>, p: Perturb) -> impl Strategy<Value = Vec<Delivery<T>>>;  // Delivery{item, index, dup, delay_ns}
pub mod seed { pub fn derive(seed: u64, label: &str) -> u64; pub fn runner(seed: u64, cases: u32) -> TestRunner; pub fn sample<S>(seed: u64, s: &S) -> S::Value; }
```

- **`market_stream` 的连接模型**:一条 BBO 连接(槽 0)承载全部 inst 的 `Bbo`,一条行情连接(`MAX_BBO_CONN_SLOTS`)承载成交、标记价、资金费与分档快照;t = 0 两条连接 `Up`、每个 inst 加入 BBO 连接(成员 `Up`);两条连接每 `bbo_ping_ms` 探测一次,回帧按 [hft-venue §3a](../hft-venue/README.md) 第 3 条产出 `Alive`。全部事件按虚拟时间全序。
- **`QuietPolicy`**:`Still` 让该 inst 在窗口内没有 `Bbo`(盘口不变),连接照常回帧;`Silent` 让 BBO 连接在窗口内没有任何事件,在 `to_ms` 依次产出**静默恢复序列** `Degraded{BBO_SILENT}`、`Down{inst = 0}`、`Up{inst = 0}`、每个 inst 的 `Up{inst}` 与 `Resync{inst, BBO_REFRESH}`(原因码归 hft-venue),之后该连接才有 `Bbo`。增量深度的序号缺口与重复属可选扩展。
- **随机性**:生成器是纯 proptest 策略,不读系统时钟与全局随机源;`seed::runner` 的 ChaCha 密钥由种子经 `xxh3` 按标签派生(`seed::derive` 同法派生子流),同一种子跨进程、跨主机逐字节相同。

### 3. 假时钟

```rust
pub struct ManualTsc { /* 实现 hft-rt 的 TSC 读取 trait ClockReads(含 &ManualTsc);固定 tsc_hz;advance_ns 取 ⌊ns × tsc_hz / 10^9⌋ */ }
impl ManualTsc { pub fn new(tsc_hz: u64, start: u64) -> Self; pub fn advance_ns(&self, ns: u64); pub fn set(&self, tsc: u64); }
pub struct ManualUtc { /* tsc → UTC 的固定校准对,供 ClockStatus/段头测试 */ }
impl ManualUtc { pub fn new(pair_tsc: u64, pair_utc_ns: i64, tsc_hz: u64) -> Self; pub fn utc_ns(&self, tsc: u64) -> i64; pub fn tsc_utc(&self) -> TscUtc; }
```

只给读 TSC 的组件(feed/gw 运行时、housekeeping、sim-gw 的交付时刻)用;sequencer 与策略只从事件取时间,不需要时钟替身。

### 4. 策略测试桩

策略 crate 的单元与场景测试只依赖 SDK 与本节(不起 sequencer)。`MockViews` 只有一份实现:[hft-strategy-sdk](../hft-strategy-sdk/README.md) `testing` 特性的 `testing::MockViews`(写入方法与 `RefFixture` 归 SDK),本 crate 原样 re-export;本节加单策略驱动与 Model 驱动。本节在本 crate 的 `sdk-mock` 特性下(它打开 SDK 的 `testing`,所以只在 `[dev-dependencies]` 启用)。

```rust
pub use hft_strategy_sdk::testing::{MockViews, PortfolioFixture, RefFixture};

/// 单策略驱动:一个事件进,按调用顺序捕获 Ctx 的全部输出。
pub struct StrategyHarness<S: Strategy> { /* 策略实例、MockViews、CtxBufs、GroupTable、参数 */ }
impl<S: Strategy> StrategyHarness<S> {
    pub fn new(params: &S::Params, groups: &GroupTable, rd: RefFixture, account: AccountId) -> Result<Self, ParamError>; // 先 ParamSet::validate
    pub fn from_body(body: &[u8], schema_ver: u16, groups: &GroupTable, rd: RefFixture, account: AccountId) -> Result<Self, ParamError>; // 经 ParamSet::decode
    pub fn bind_clock(&mut self, c: &'static ManualTsc);                // 之后 feed 中任何读取即失败
    pub fn feed(&mut self, ev: &EventRef<'_>) -> CtxCapture;            // 订单意图、目标意图、撤改、定时器、halt,之后是 Note
    pub fn on_params(&mut self, p: &S::Params) -> Result<(), ParamError>; // validate 失败时策略不被调用
    pub fn snapshot(&self) -> Vec<u8>;
    pub fn snapshot_roundtrip(&mut self) -> Result<(), RestoreError>;   // snapshot → 新实例 restore → 再 snapshot 字节相等
    pub fn views_mut(&mut self) -> &mut MockViews;
}
pub struct CtxCapture { pub items: Vec<Captured>, pub handles: Vec<IntentHandle>, pub full: u32 }
pub enum Captured { Place(OrderIntent), Target(TargetIntent), ClearTarget(AccountId, GroupId), Cancel(CancelReq),
                    Timer(TimerReq), CancelTimer(TimerId), Halt(GroupId, u16), Note(NoteReq) }
pub struct ModelHarness<S: Strategy> { /* S::Model 实例 + MockViews + AdviceBufs */ }
impl<S: Strategy> ModelHarness<S> {
    pub fn new(params: &S::Params, groups: &GroupTable, rd: RefFixture, account: AccountId) -> Self;
    pub fn feed(&mut self, ev: &EventRef<'_>) -> Vec<(GroupId, u16, Vec<u8>)>;   // (group, local_kind, 载荷)
}
```

- `CtxCapture.items` 按调用顺序列出操作(SDK 的 `CtxBufs::ops()`);Note 在 SDK 中单独成表、不记相对顺序,排在操作之后。`full` 为本次的 `CtxFull` 次数。
- 每次 `feed` 检查:不分配(测试二进制以 `CountingAlloc` 为全局分配器时;失败信息含"分配次数 N")、不读时钟(绑定了 `ManualTsc` 时,其读取计数不变;失败信息含"读取了时钟")、不 panic(panic 时以 0 起的事件序号与 panic 信息报告)。

### 5. 解耦门槛夹具

[testing §8](../../docs/testing.md#8-上线门槛) 门槛 ⑧ 的执行者:**新增一个策略时引擎 crate 的 diff 为零**。

- **两个测试策略**(本 crate 的 `src/strategies/` 模块,只引用 [hft-strategy-sdk](../hft-strategy-sdk/README.md) 与 [hft-strat-lib](../hft-strat-lib/README.md),TK-036 检查;不另设 workspace 包,依赖图不变):
  - `probe-taker`(kind_id 0xFF01):腿 A 买一高出腿 B 卖一超过 `spread_th`(`(A bid − B ask) × 10^9 / B ask`,Rate raw,严格大于)时卖 A,带 `LegLink`(OnFill,第二腿买 B);同一事件内另设一个持有定时器并写一条 Note(码 `NOTE_OPENED`);收到 `close_on` 选中的 `Exec`/`External`/`Risk` 事件时设 0/0 的退出目标;行使 OMS 链接、风控两腿检查与对冲重试。参数 `spread_th`(per_group)、`qty_lots`(per_group)、`close_on`(位:1 Exec、2 External、4 Risk)、`hold_ms`(`check`:0 < `hold_ms` < 10 为 `Invalid`)。
  - `probe-target`(kind_id 0xFF02):在引擎时间 `t_slice_ms`、`t_pair_ms`、`t_simul_ms` 各发一次 `TargetIntent`(依次 `TakerSlice`、`PairOnFill`、`Simultaneous`;0 = 不发),`need_advice` 时只在缓存的 Advice 未过期时发、否则跳过该点;`Model` 每 `advice_every` 个事件产出一条 16 B 不透明 `Advice`,`mem_bytes`/`snapshot_bytes` 由参数 `model_mem_kib`/`model_ckpt_kib` 声明;行使执行层、advisor 路径与 schema 驱动参数。
  - **场景用到的固定能力**(跨组件用例按名字引用,改动须同时改引用它的用例):`probe-target` 的参数 schema 含结构字段 `win`(`FieldDesc.structural = true`)与非结构字段 `thr`、参数 `advice_max_age_ms`(快速部分用 hft-strat-lib `Expiring` 缓存 Advice,目标点到时缓存中没有未过期 Advice 就跳过该点),命令描述符 `op 1`(`set_thr`)与 `op 2`(`exit_all`);`TestStrategies` 另含 `probe-np`——与 `probe-taker` 同一代码、`StrategyDesc.non_prod_only = true`、kind_id 不同;测试构建以 `ProbeBuild{build_git, desc_version, model_layout_version, model_path_version, model_mem_bytes, model_ckpt_bytes}` 在装配时注入版本、布局与 Model 的内存与检查点大小,两组不同取值即两个"构建"(版本变化冷启动与检查点沿用/丢弃的场景用它,不另编二进制)。
- 装配:本 crate 的 `src/strategies/mod.rs` 用 SDK 的 `strategy_set!` 生成只含测试策略的 `TestStrategies`(`probe-taker`、`probe-target`、`probe-np`(0xFF03);kind_id 在 0xFF00–0xFFFF,只在测试构建中出现);本 crate 的 DST 与场景测试经它驱动真实 sequencer。**不依赖 registry**:生产策略的唯一列表在 [hft-strategy-registry](../../strategies/registry/README.md),以生产策略运行的 DST 场景与压力用例写在 `strategies/registry/tests/`(以 registry 的 `Strategies` 实例化本 crate 的泛型 DST 框架)或 `apps/hft-core/tests/`。
- 门槛 ⑧ 由两个 xtask 子命令共同执行,都通过才绿(子命令的步骤、参数、触发与"引擎侧零改动"检查的路径范围唯一归属 [tools](../../tools/README.md#cargo-xtask-scaffold-gate);本节只写本 crate 提供的夹具与门槛 ⑧ 对它的要求):
  - **A. `cargo xtask scaffold-gate`(模板探针;改动 `crates/`、`apps/`、`strategies/`、`tools/` 的 PR)**,本 crate 提供:
    1. **探针的形态**:探针 `ci-probe` 由 `cargo xtask new-strategy ci-probe --kind-id <0xFF00–0xFFFF 测试段> --with-model --with-pipeline` 生成——带一个声明 `mem_bytes` > 1 MiB/group 的 `Model`(行使 advisor、检查点分块与按 `mem_bytes` 的装配预算)与一个 pipeline 插件(行使 schema 驱动的参数产出),模板内容归 [adding-a-strategy](../../docs/cookbook/adding-a-strategy.md);
    2. **场景与 DST 短集**:`ci-probe` 自带的 `StrategyHarness`/`ModelHarness` 场景(模板生成,dev 依赖本 crate),以及在 registry 临时副本 `tests/` 里以含 `CiProbe` 的 `Strategies` 实例化本 crate 泛型 DST 框架的短集(100 种子);
    3. **一次装配检查**:以 registry 临时副本构建的 `hft-core --check`,加载一份把 `ci-probe` 绑定到一个 Sim 账户的 `sim` 实例测试配置(由 scaffold-gate 生成,配置字段归 hft-core),装配通过——含按各 Model 的 `mem_bytes`/`snapshot_bytes` 求和的预算校验(规则归 [hft-core](../../apps/hft-core/README.md));
    4. 构建复用 PR 作业的主 target 目录(只增量编译受影响的 crate),不另起 target 目录。
  - **B. `cargo xtask decoupling-gate`(PR 差异检查,不构建;每个 PR,评审或标签变化时由 `decoupling.yml` 单独重跑)**:PR 的 `git diff --name-only base..head` 同时含 `strategies/**` 与引擎 crate(`crates/*`)或 app(`apps/*`)路径即失败(判定细则归 [tools](../../tools/README.md#cargo-xtask-decoupling-gate));豁免条件(`engine-change` 标签 + Admin 批准评审)与摘要格式归 [development](../../docs/development.md#质量门quality-gatesyml)。
- 任一步失败即红,报告第一处被触碰的引擎文件或违规依赖边。

### 6. DST 框架

```rust
pub struct World<E> { /* 虚拟时间、按 (virtual_ns, gen_seq) 全序的事件堆 */ }
pub fn actor_rng(seed: u64, a: ActorId) -> ChaCha8Rng;   // ChaCha8Rng::seed_from_u64(seed::derive(seed, actor 标签))

/// 被测系统的边界:引擎适配器 SeqSut<S: StrategySet>(以 DstDriver 单线程驱动真实 Sequencer<DstDriver, S>,
/// 把各组件调试视图抄进 Obs)实现它;ToyEngine 是框架自测引擎。
pub trait Sut {
    type Matcher: Matcher;                               // hft_sim::SimExchange(引擎运行)| ToyMatcher(框架自测)
    fn boot(ctx: &BootCtx<'_>) -> Result<Self, BootFail>;
    fn process(&mut self, now_ns: u64, tsc: u64, input: &InputRec, out: &mut StepOut);   // 一步 = 一条 Input 及其全部 Output
    fn next_deadline(&self) -> Option<u64>;
    fn fire_timers(&mut self, now_ns: u64, tsc: u64, out: &mut StepOut) -> bool;
    fn observe(&self, obs: &mut Obs);                    // 只读;src/dst/obs.rs 的行即适配器契约
    fn hashes(&self) -> ComponentHashes;
    fn rebuilt_hashes(&self) -> Option<ComponentHashes>; // INV-6:最近快照 + 其后 Input 重建
    fn crash_and_recover(&mut self, p: CrashPoint, input: Option<&InputRec>) -> Result<(), BootFail>;
    fn clean_restart(&mut self) -> Result<(), BootFail>;
    /* seq()、journal()、is_sim() */
}

pub struct DstConfig {                   // 与引擎无关的数据;引擎是 run 的类型参数
    pub seed: u64,
    pub suite: &'static str,
    pub steps: u64,                      // 步数上限
    pub horizon_ns: u64,                 // 虚拟时间上限,也是故障计划的范围
    pub world: WorldSpec,                // venue、inst、账户(Live/Sim)、策略绑定(kind_id、参数 blob、RunMode)、group、风控参数;WorldSpec::w1() = scenarios §2.2 W1
    pub venues: Vec<MockVenueCfg>,       // venue 行为层(限速、拒单码、死手、bbo_ping_ms、自身强平标记、时延)
    pub market: MarketSource,            // Generated(MarketGen:种子化随机游走)| Recorded(Vec<(virtual_ns, InputRec)>)
    pub faults: FaultRates,              // 每类别开关 + 每个故障槽(slot_ms)的千分概率;取值由各套件的 DstSuite 给出,没有全局缺省
    pub scripted: Vec<(u64, Fault)>,     // 场景的 `故障` 脚本,并入生成的计划
    pub plan: Option<FaultPlan>,         // 显式计划(shrink、按记录复现);None = 由种子生成
    pub invariants: InvariantSet,
    pub hash_every: u64, pub rebuild_every: u64, pub rerun: RerunPolicy,   // HashTrail 采样、INV-6 间隔、INV-9 复跑
    pub actors: ActorSet, pub planted: Vec<&'static str>, pub tsc_hz: u64, pub trace: bool,
}

pub fn run<E: Sut>(cfg: &DstConfig) -> DstOutcome;          // Pass{state_hash_trail} | Fail(DstFailure{seed, suite, step, invariant, message, trace, fault_schedule, journal_tail})
pub fn run_record<E: Sut>(cfg: &DstConfig) -> RunRecord;    // 另含 journal、出环帧、交付的 Input、计划、HashTrail、最终 Obs(场景断言用)
pub fn shrink<E: Sut>(cfg: &DstConfig, fail: &DstFailure, budget_s: u32) -> DstFailure;   // 删减故障计划得到最小复现

/// 一个 dst 用例:Scenario::new("<编号>", 标题) 注册(test-trace 以该调用所在函数为位置)。
pub struct Scenario { pub id, pub title, pub suite: SuiteSel /* All | Named */, pub seeds: SeedSet /* Standard | Pr(n) | Fixed */,
                      pub steps: Option<u64>, pub invariants: Option<InvariantSet>,
                      pub configure: Option<fn(&mut DstConfig)>, pub assert: Option<fn(&RunRecord) -> Result<(), String>> }
pub struct DstSuite { pub name, pub title, pub faults: FaultRates, pub steps: u64, pub base: fn(u64) -> DstConfig, pub must_pass: &'static [&'static str] }
```

**场景套件**:`src/scenarios/<主题>.rs` 定义以 `TestStrategies` 运行的全部套件与场景(全仓 `dst`/`integ` 用例的 `实现` 落点,场景以用例编号注册)(`DstSuite`:名称、市场源、故障率、策略设置、必过用例 QF-1/MS-2(测试策略版)/MS-3 等,[testing §3.4、§4](../../docs/testing.md#34-dst));`hft_testkit::scenarios::{suites(), scenarios()}` 是唯一清单:仓库根 `tests/dst/main.rs` 只是 cargo test 入口(本 crate 的 `[[test]] dst` 目标,每个套件一个 `#[test]`,跑该套件场景的短种子区间),`cargo xtask dst` 与 [`hft-dst`](#8-hft-dst-运行器) 调用同一组套件,保证 PR、CI 每夜与服务器累计跑的是同一份场景。套件:`core`、`modes`、`storage`。在 sequencer 适配器落地之前,三个入口以 `ToyEngine` 运行(框架自测引擎:两腿 taker、OMS 槽表、OnFill 对冲与余数、ledger、kill 与模式、Broken 自动平仓、L3、Drain,带 `toy::PLANTED` 植入缺陷;不是生产引擎的模型)。

`cargo xtask dst --seeds N …` 是开发与 CI 的入口(签名与参数唯一归属 [tools](../../tools/README.md#cargo-xtask-dst));失败时打印且只打印一行可复制的复现命令(种子为 16 位十六进制,`--steps` = 违反所在步 + 1,`--suite` 为该种子实际运行的套件):

```
DST FAILED seed=0x9e3779b97f4a7c15 step=48213 invariant=I3(exposure) -> cargo xtask dst --seeds 1 --seed 0x9e3779b97f4a7c15 --suite core --steps 48214 --trace
```

**套件选择**:不指定套件时,一个种子运行的套件 = 按名字排序的全部套件中下标为 `xxh3_64(seed 的 8 字节小端) mod 套件数` 的那个;`cargo xtask dst` 与 `hft-dst run` 用同一函数 `seed::suite_index`。**缺省步数** 10,000(两个入口相同,`seed::DST_DEFAULT_STEPS`)。

**故障目录**(每项有速率参数与开关;期望行为由不变量检查,具体场景的期望写在 [testing §3.4、§4.3](../../docs/testing.md#34-dst)):

| 类别 | 故障 |
|---|---|
| 连接 | 行情/下单连接断开与重连、重连后必须重同步、单向静默(灰色故障:活着但延迟 ×100) |
| 回报 | 乱序、重复、延迟(重尾)、fill-before-ack、cancel-reject-because-filled、部分成交、只带费用的晚到回报、旧代数(`StaleGen`)成交、回收槽后的迟到成交 |
| 拒绝 | 限速 429、5xx、超时(发送后无回报)、未登记错误码、无过期保护的所返回 NotFound、第二腿被拒 |
| 两腿与断腿 | 第二腿盘口长时间不变而连接健康(testing QF-1,期望照常对冲)、第二腿 BBO 连接停止回帧超过 `conn_liveness_ms` 或处于重同步、第二腿可用计价资产或保证金不足、两腿 lot 不齐(含 `LotMismatch` 的 `SetParams`)、第二腿对手价偏离超过 `max_slip`(≤ 与 > `rr_max_cross` 两类)、第二腿持续被拒使 cell `Broken` 且长时间无人工 `HedgeFix`(驱动有界自动平仓,另有自动平仓本身被拒的变体;另有对冲腿所在 venue 断线或其闸门位长期不满足、而多出那一腿所在 venue 健康的变体)、成交回报不带 client id(在 `gw.unmatched_hold_ms` 内或之后才归属到槽) |
| 行情 | 交易所时间戳乱序、跨所时钟偏移、资金费结算周期变化、分档快照(TopN)的整替换与连接重建;增量深度启用时(可选扩展)另有序号缺口、快照与增量错位 |
| 账户与保证金 | 到强平距离路径(经 `margin_path`)、单边 +30%(1 h 内)行情(经 `one_sided_move`,testing MS-2,含只让一所变价)、L3 期间另一腿 venue 的 BBO 连接停止回帧 30 s 或其下单连接断开(testing MS-3,期望永续腿照常按步减)、L3 时策略在该 inst 上挂满 reduce-only 平仓单(testing MS-5)、交易所实际杠杆变化(含超过 `max_perp_leverage_x100` 与读不到杠杆)、保证金数据过期、自身强平与 ADL 成交(`ExternalFill`)、用户流缺口里漏掉的 ADL(只经 `ReconActor` 的仓位差发现,含重连后补推同一笔真实成交,testing AX-1)、人工/网页成交(期望 `manual_hold`)、`AccountConfigReport` 启动不一致与运行中变化、交易所 UID 与 `AccountMeta.venue_uid` 不符、全仓资金费只推余额(不带 inst)、基础币扣费、交易所逐笔资金费与本地入账不一致(经 `ReconActor`,`funding_flows`)、L3 发生时存在 `Broken` cell、`Broken` 后残余介于尘埃线与最小下单量之间(testing BK-6) |
| 用户流 | 未知事件类型(`VenueCaps.ignorable_user_events` 内与外各一类) |
| 控制 | 命令乱序/重复/过期、`SetParams` 与 `Kill` 交错、按账户 `Kill{hard}`、`Kill{Account, flatten}` 时存在 `Broken` cell、闸门未放行与 `DRAINING` 期间的 `Kill{flatten}`、flatten 锁期间策略发远离 0 的目标、Viewer 紧急停止(含越权参数、预览与确认之间状态变化、确认超过 TTL)、两腿同所的 `SetParams`、`VenueNotice{Delisting}`、`SimReset`/`SimTransfer`、`InjectWarmup` 分块乱序与超长载荷、`SmokeOrder`(含 Shadow 下、HARD 下、同一 (account, venue) 第二张)、`WriteOffResidual`(含静止的非 Broken group)、`Resume{Kill}` 解除非 HARD 与 HARD 的 Kill(期望只有后者产生 `resolve_broken`) |
| 模型与模拟 | advisor 落后(Advice 以旧依据连续到达,含跨启动的旧 `basis_seq`)、模拟回报洪峰(`rep-sim` 每轮数百条,检验 `sim_max_per_round` 与实盘优先) |
| 存储与进程 | persist 滞后与磁盘低水位(`Timer{PersistLag|DiskLow}`)、任意时刻崩溃后 `RecoverDriver` 重启、同一输入连续崩溃(毒丸)、恢复时输出不一致、停机时长超过 `hedge_deadline_ms` 后冷启动(有未完成链接)、补腿冷却或外来成交计时中崩溃(testing BK-5)、L3 中途重启且现货所断线(testing MS-6)、`stop_core` 写下停机标记后的干净重启(期望启动时写 `Kill`;标记已作废为 `.aborted` 时不写)、干净重启时 group 带容差内残余(期望无 `Broken`、无 `HedgeFailure`)、只含 Sim 账户的实例非干净重启(期望无 `RecoverMismatch` 与毒丸时自动解除 `Recovery`)、换 `boot_id` 与 TSC 频率后冷启动(检验 `engine_ns` 连续)、Drain 期间在途链接、IOC 与未到达的 `MarginStress`/`Hard` 引擎目标 |
| 链上(可选扩展,`chain` feature) | builder/公共 RPC 传输错误与限速封禁、回执源矛盾、重组、终局停摆、泄露或重组回灌后迟到上链、钱包外部交易(语义与期望归 [hft-chain-evm](../hft-chain-evm/README.md)) |

**不变量**(每步检查,`InvariantSet` 里可单独开关)。**定义与断言以 [docs/tests/invariants.md](../../docs/tests/invariants.md) 为准**:检查器 `I<n>` 就是 `INV-n`,在 `src/dst/invariants.rs`:`inv_<n>_<描述>()` 以 `Scenario::new("INV-n", …)` 登记该不变量的常设用例(全部套件、标准种子集),`CHECKERS` 表给出每个编号的步检查与结束检查;检查器只读该步的 journal 记录、出环帧、`StepInfo`(决策时刻的调试视图事实、成交证据、Note 名)与步前步后的 `Obs`,所以开关检查器不改变运行。本框架按编号逐条实现 `I1`–`I20`,下表只写检查方式:

| # | 检查方式 |
|---|---|
| I1 | [`hft-ledger`](../hft-ledger/README.md) 逐笔归因到成交证据(`OrderEvent` 成交、`ExternalFill`;链上为 `TxLifecycle{IncludedOk}`) |
| I2 | 每条回报匹配槽、匹配墓碑或产生告警事件 |
| I3 | 每步读风控调试视图,对比 `RiskParams` |
| I4 | 事件轨迹:`Unknown` 的 group 在收敛前无开仓/反向意图 |
| I5 | 出环帧检查 `RISK_REDUCING`/`HARD` 位与账户位 |
| I6 | 每 N 步从最近快照 + 后续事件重建,比较状态哈希 |
| I7 | OMS 调试视图:占用槽数 = 在途订单数,空闲槽数不单调下降 |
| I8 | `LegLink` 发出量之和 vs 触发它的累计**净**成交量 |
| I9 | 每次运行抽 5% 种子重跑,比较意图流与 `HashTrail`;一致性集(`fixtures/dst/hashtrail.toml` 登记的种子)的 `HashTrail` 与金样逐字节比较:PR 核对前若干个、每夜与版本标签核对全部(版本标签在同一提交上于 x86_64 与 aarch64 各跑一次;种子数归 [testing §6](../../docs/testing.md#6-ci-运行矩阵),命令归 [tools](../../tools/README.md#cargo-xtask-dst)) |
| I10 | (可选扩展,chain)每钱包在途 nonce ≤ 1 |
| I11–I14 | `hft-risk::mode` 调试视图:有效模式重算 = 增量值;`RISK_REDUCING` 只被 HARD 拦;cap 变化不产生开仓;门控拒绝不计 `consec_fail` |
| I15 | 每笔对冲类出环单(第二腿、`SecondRetry`、`HedgeFix`、残余修剪、外来成交默认处理、命令补腿、自动有界平仓):数量 ≤ 出环帧的 `hedge_cap_qty`(基数 + 1 lot),名义 ≤ `max_hedge_notional`(自动有界平仓再 ≤ `max_reduce_notional`);Flatten 类出环单名义 ≤ `max_reduce_notional` |
| I16 | 无在途时 ledger 的仓位、现货库存、余额 = `SimExchange::account()` 真值(手续费与资金费到最小单位) |
| I17 | MarginStress L3 的出环单每腿都朝 0、不越过 0;Broken 与 `SerialFallback` 的 cell 永续腿到达引擎目标前没有现货腿减仓单;每步引擎目标的绝对值 ≤ 上一步基准 × (1 − `force_step`) 按 lot 向零取整 |
| I18 | `Drain` 正常返回 `Done`(非 `DrainTimeout`)时无 Open/Hedging 链接、无非终态 IOC、无 HedgeRetry 定时器、`exec.drain_ready()` 为真 |
| I19 | 出环的第二腿、`SecondRetry`、`HedgeFix`、残余修剪、自动有界平仓单:数量 ≤ 发出时 LegTracker 调试视图中该方向的裸量;L3/flatten 减仓单:数量 ≤ 发出时该腿 \|仓位\| − 同向即时在途;同一笔成交(去重键按 [hft-venue-proto 契约 22](../hft-venue-proto/README.md#实现方必须遵守的契约),在账户内)不出现两次对冲 |
| I20 | 执行层调试视图:每个 `Broken` cell 记录进入时刻与计时状态(按引擎运行时间;`manual_hold` 与恢复期的处理——不满足期间不计时、放行时重给完整窗口——唯一归属 [hft-exec Broken 处置](../hft-exec/README.md#broken-处置));非 HARD、无人工 `HedgeFix` 时,计时满 `broken_auto_fix_ms` 后恰好开始一次自动有界平仓,开始前的等待与开始后的门控拒绝、零成交不产生终态,每个 Broken 期至多一个终态(`ExecAutoFixDone` 或 `ExecAutoFixFailed`);L3 或 `engine_flatten` 期间 `Broken` cell 的永续腿有 reduce-only 减仓单 |

### 7. 状态哈希对比

```rust
pub struct HashTrail { pub every_n: u64, pub points: Vec<(u64 /*seq*/, ComponentHashes)> }
pub struct ComponentHashes { pub book: u64, pub oms: u64, pub risk: u64, pub ledger: u64, pub exec: u64, pub strategies: u64, pub all: u128 }
pub fn diff_trails(a: &HashTrail, b: &HashTrail) -> Option<Divergence>;   // 第一处不一致的 seq 与组件(写出顺序在前者);缺点时 component = None
pub fn bisect_divergence(run: impl FnMut(u64) -> (ComponentHashes, ComponentHashes), lo: u64, hi: u64) -> u64;   // run(seq) 返回两侧在 seq 处的哈希;lo 处相同、hi 处不同,返回 (lo, hi] 中第一处不同
pub fn component_hashes(snapshot_logical_body: &[u8]) -> Result<ComponentHashes, SnapErr>;
```

组件哈希取自 [`hft-sequencer`](../hft-sequencer/README.md) 快照的分组件段(顺序由 sequencer 固定):每个组件 = 其各节(`SectionHdr` + 节体,按写出顺序拼接)的 xxh3-64;节到组件的对应(节号归 [journal-format](../../docs/subsystems/journal-format.md#快照记录结构)):ledger 1、13、22;oms 2、3、12;risk 4、14、17、18、23;strategies 5;book 6、7;exec 21;其余节只进 `all`。`all` 即快照头的 `state_hash`(xxh3-128);`purpose = Keyframe` 的头写 0,读者对除节 16 外的全部节按写出顺序算 xxh3-128。组件的"写出顺序"按其最小节号:ledger、oms、risk、strategies、book、exec。DST 的 `HashTrail` 以已处理 Input 的序号为点(每 `hash_every` 个),`replay-verify` 以 journal seq 为点。本 crate 不定义新的哈希协议,只提供采样、比较与二分。`cargo xtask replay-verify` 用它报告"第一处分歧在 seq N 的 oms 组件"。

### 8. hft-dst 运行器

`hft-dst` 是本 crate 的二进制目标(`src/bin/hft-dst.rs`,feature `dst-bin`):把 DST 框架、`TestStrategies` 与 `src/scenarios/` 的全部套件编成一个不依赖 cargo 的独立程序。CI 以 `ci` 剖面(断言与溢出检查开启、panic 为 unwind,取值归 [development 构建剖面](../../docs/development.md#构建剖面))在 `ubuntu-24.04-arm` 上构建,放进验证包 `hft-verify-<ver>-aarch64.tar.zst` 的 `bin/hft-dst`;服务器上只由 `hft-verify` 调用(驱动、种子区间分配、运行记录与账本归 [tools](../../tools/README.md),账本规则归 [testing §6.1](../../docs/testing.md#61-服务器上的累计运行)),不编译、不联网。

```text
hft-dst run   --seeds <from>..<to> [--suite <name>|all] [--steps 10000] [--deadline-s N] [--checkpoint-seeds 1000] --out <result.json> --crash-dir <dir>
hft-dst repro --seed 0x… --suite <name> --steps N [--trace] --crash-dir <dir>
hft-dst list                                          # 编进本二进制的套件、必过用例与各自的故障率
hft-dst hashtrail --seed 0x… --suite <name> --steps N # 一行:trail <32 位十六进制> points=<n>
```

- **`list`**:每个套件一行,以 `suite <名称>` 开头(其后是 `must_pass=…`、`steps=…`、`slot_ms=…`、`faults=<类别>:<千分率>,…`);`cargo xtask dst` 的套件表就取这些行。
- **`hashtrail`**:以该种子、套件与步数运行一次(不做 INV-9 复跑),标准输出恰一行 `trail <xxh3-128(HashTrail::to_bytes()) 的 32 位小写十六进制> points=<点数>`,退出 0;有违反时 stderr 打印复现行、退出 1;参数错误退出 2。`cargo xtask dst --hashtrail check|write` 用它核对或重写 `fixtures/dst/hashtrail.toml`(`[[seed]]` 表:`seed = "0x<16 位十六进制>"`、`suite`、`steps`、`trail`,登记顺序即文件顺序)。

- **`run`**:按升序逐个运行区间内的种子(每个种子轮转套件,由种子派生选择);到 `--deadline-s` 时在当前种子结束后停止。结果 `DstRunResult`(JSON,规范化字段顺序):`{bin_version, git_sha, profile, suites, seed_from, seed_to_done, seeds_run, steps, violations: [{seed, suite, step, invariant}], elapsed_s}`;`seed_to_done` 之后的种子视为未运行(不计入,也不算失败)。每完成 `checkpoint_seeds`(默认 1,000)个种子就把当前结果原子地写到 `--out`(先写 `.new` 再 rename);收到 `SIGINT`/`SIGTERM` 时在当前种子结束后写出结果并退出,被强杀时最近一次落盘的结果仍可计入。
- **进行中种子标记**:每个种子开始前,`run` 原子地写 `--crash-dir/.inflight.json` = `{seed, suite, steps, started_utc}`(先写 `.new`、`fsync`、再 rename);下一个种子开始时覆盖,正常退出时删除。它以隐藏文件命名,journal-archiver 的 files source 不上传它(会被改写的文件上传会 `Conflict`,[journal-archiver](../../apps/journal-archiver/README.md#文件归档files-source))。进程被信号终止时该文件指明当时在跑的种子:以致命信号结束(如 `SIGABRT`/`SIGSEGV`:双重 panic、栈溢出、内存错误)时 `hft-verify` 按它记失败种子,被强杀或中止时该种子由下一次运行重跑(信号的分类表归 [tools](../../tools/README.md#hft-verify),清零规则归 [testing §6.1](../../docs/testing.md#61-服务器上的累计运行))。
- **失败**:任一不变量违反,或被测代码在某个种子内 panic(每个种子在 `catch_unwind` 内运行,panic 记为 `invariant = "panic"` 的违反)→ 该种子的故障表、违反前 200 条事件的 journal 片段与一行复现命令写进 `--crash-dir/<seed>/`;`run` 继续跑剩余种子(一次运行内的全部违反都记录),退出码 1。
- **确定性**:同一二进制、同一种子、同一套件 → 同一结果与 `HashTrail`(与 `cargo xtask dst` 在 CI 上的同种子运行逐字节相同,不变量 9);不读系统时钟(`--deadline-s` 只决定在哪个种子之后停止,不影响任何种子的结果)、不读环境变量、不访问网络。
- **退出码**:0 全部种子零违反;1 有违反;2 参数或环境错误(区间非法、输出目录不可写)。
- 只含 `TestStrategies`;以已注册生产策略运行的 DST(`strategies/registry/tests/`)不在 `hft-dst` 里,它们随 PR 与 CI 每夜运行,不计入门槛 ④ 的累计。

### 9. 分配计数与基准驱动

[hot-path-rules M1](../../docs/hot-path-rules.md#内存m) 的零分配测试与 [benchmarks](../../benchmarks/README.md#实现说明) 的基准驱动在本 crate 实现;热身次数、绑核与输出格式归 benchmarks。

```rust
/// 全局分配器包装:按线程计数 alloc/realloc 调用(dealloc 不计);测试二进制以 #[global_allocator] 安装。
pub struct CountingAlloc;
impl CountingAlloc { pub fn scope() -> AllocScope;            // 从此刻起计本线程的分配次数
                     pub fn is_installed() -> bool; }         // 是否已装为全局分配器(本线程一次探测分配被计入)
pub struct AllocScope { /* … */ }
impl AllocScope { pub fn allocs(&self) -> u64; }               // 本线程自 scope() 以来的分配次数;其他线程的分配不计入
/// Criterion 之外的直方图基准驱动(HistBench):每次迭代前后各读一次 TSC,记进预分配的 hdrhistogram,同时以 CountingAlloc 计分配。
pub struct HistBench { /* name、迭代与热身次数、是否冒烟模式 */ }
impl HistBench { pub fn smoke(name: &str) -> HistBench;                       // 热身 1 000、迭代 10 000、不绑核
                 pub fn reference(name: &str, cpu: u16, iters: u64) -> HistBench; // 热身 100 000、绑到 cpu
                 pub fn run(&self, f: impl FnMut()) -> Result<BenchOutput, String>; } // 闭包共调用 warmup + iters 次
/// n、warmup、p50/p90/p99/p99.9/max(ns)、allocs_after_warmup(热身后本线程的分配次数)、pinned、直方图本身。
pub struct BenchOutput { /* … */ }
```

## 实现说明

<details><summary>实现要点</summary>

### DST 的世界模型

1. **单线程、虚拟时间**:`World` 持有按 `(virtual_ns, gen_seq)` 全序的事件堆;每步弹出一个事件交给目标 actor,actor 产生的新事件按其延迟压回堆。没有真实线程、socket、时钟。
2. **actor**:
   - `SeqActor`(`Sut` 的引擎适配器 `SeqSut<S>`):真实 [`hft-sequencer`](../hft-sequencer/README.md)(以 `S` 实例化;本 crate 的场景为 `TestStrategies`),用 `DstDriver` 单线程驱动——它实现 sequencer 的 `Driver` trait:`take` 交出 `World` 投递的 Input、`out.tsc` 与 `run_tsc`/`measure` 都是虚拟时间对应的 tsc(`stamp()`),`emit` 进收集缓冲,`journal` 是内存段 `MemJournal`;签名中没有随机数源;Live 账户的出环帧交给 `MockVenue`,Sim 账户的交给 `SimActor`。
   - `MockVenue`:`hft-sim::SimExchange` + venue 行为层(限速令牌桶、拒单码表、死手倒计时、`VenueCaps` 推送形态、自身强平订单标记、行情连接按 `bbo_ping_ms` 回 pong——连接健康输入的事件形态与 hft-venue 相同,归 hft-event),回报经 `FaultPlan` 扰动后回到 `SeqActor`。它在 DST 里扮演"真实交易所",模拟真值同时是不变量 I16 的参照。
   - `SimActor`:与 hft-core 的 sim-gw 相同的 `SimExchange` 用法(不经故障注入),覆盖 Sim 账户路径;`CrashFault` 之后按 hft-sim 规定的恢复步骤由快照与 journal 重建模拟交易所状态(余额、仓位、挂单;规则归 [hft-sim](../hft-sim/README.md)),不保留崩溃前的内存对象。
   - `AdvisorActor`:按 sequencer 的 advisor 规则驱动策略的 `Model`,产出带依据(`basis_seq` 等,字段归 hft-event)的 `Advice` 送回 `SeqActor`;可注入"落后"(按种子延迟交付并成批追赶;或按 `AdvisorLagFault{from_ens, to_ens, lag_ms}` 在引擎时间 [from_ens, to_ens) 内使 Model 处理 journal 落后 `lag_ms`,其间产出的 Advice 同样推迟交付)。
   - `CtlActor`:按 `gen::control_cmds` 注入命令(带主体与角色,含 Viewer);`ReconActor`:按真值生成 `ReconcileReport`(可注入差异,含按 `funding_flows` 的逐笔资金费条目;仓位条目的 `AUTO_EXTERNAL` 与 `Summary.sflags` 按 [reconciler](../../apps/reconciler/README.md#reconcilereport) 的置位条件填写,含两轮无 `USERSTREAM_DOWN` 与用户流稳定一个周期);`HkActor`:注入 `ClockStatus`、persist/磁盘状态;`MarketActor`:生成或回放行情。
   - 可选扩展 `MockChain`(`chain` feature):`hft-sim` 链上模型 + 证据源;被测对象是 `hft-chain-evm` 时驱动真实 `ChainSeq`。
3. **崩溃与毒丸**:`CrashFault{at_step, repeat, point}` 在某输入处理中途丢弃 sequencer 实例,只留 journal(内存段),再以 `RecoverDriver` 重建;`repeat = 2` 时同一输入再次崩溃,检验隔离路径。`point` 选崩溃位置:`SeqInput`(缺省,上述)、`SimMatchedUndelivered`(sim-gw 已撮合、回报仍在待交付堆)、`SimRepQueued`(回报已入 `rep-sim` 环、sequencer 未取走)、`BeforeFirstReport`(某 Sim 账户的第一条余额报告被处理之前);后三种同时丢弃 `SimActor` 的内存对象(整进程崩溃),按 hft-sim 的重启恢复步骤重建。`CleanRestartFault{at_step}` 做一次干净 Drain 重启。
4. **故障计划**:运行开始时由种子一次性生成完整故障时间表(`Vec<(virtual_ns, Fault)>`),`shrink` 可以逐项删除并重跑。
5. **随机性唯一来源**:每个 actor 一条 `ChaCha8Rng::seed_from_u64(seed::derive(seed, <actor 标签>))`,增删一个 actor 不改变其他 actor 的流;被测代码需要的随机数必须由事件提供,DST 不向被测代码暴露 RNG。
6. **检查**:每步后运行启用的不变量检查器(只读访问调试视图);失败即停,把违反前 200 条事件(按 `EVENT_END` 计)写成 journal 记录格式的片段(`journal.bin`,24 B 记录头 + 体 + 补零,`hft_event::decode_record` 可逐条读出;replayer 读入片段随 replayer 落地)、故障表(`faults.txt`,每行 `<virtual_ns> <类别> <故障>`)与复现行(`repro.txt`)存到 `<目录>/<种子 16 位十六进制>/`(`cargo xtask dst` 为 `target/dst/`);失败种子加入 `fixtures/dst-seeds/` 回归集。
7. **收缩**:"删除一段故障 → 重跑 → 仍以同一不变量失败则保留删除",段长从一半起逐次减半到单项,直到不能再删或超出预算(墙钟秒;预算只决定收缩走多远,每次重跑本身确定)。

### 金样运行器

1. 读 `meta.toml` 得 venue、通道、协议版本、捕获时间;按 fixtures 布局读原始帧。
2. 用被测 `VenueProtocol` 的纯解码函数逐帧解码(不经网络与运行时)。
3. 与 `expected` 逐事件比较;`rx_tsc` 等本地字段忽略。

### 源文件

| 文件 | 职责 |
|---|---|
| `src/golden.rs` | `GoldenCase`、加载、`run_decoder_case`、`run_sim_case`、`golden_suite!` |
| `src/gen/{market.rs, orders.rs, perturb.rs, ctl.rs, fees.rs, margin.rs, external.rs}` | proptest 策略(模块名 `r#gen`:`gen` 是 2024 版关键字;`fees.rs` 含 `funding_flows`,`margin.rs` 含 `one_sided_move`,`external.rs` 含 `account_config`);`gen/chain/*`(`chain` feature) |
| `src/clock.rs` | `ManualTsc`、`ManualUtc` |
| `src/seed.rs` | 种子派生与确定性 `TestRunner` |
| `src/sdk_mock.rs` | `MockViews`、`StrategyHarness`、`ModelHarness`、`CtxCapture` |
| `src/strategies/{mod.rs, probe_taker.rs, probe_target.rs}` | `TestStrategies` 与测试策略 |
| `src/dst/{world.rs, driver.rs, sut.rs, obs.rs, actors.rs, venue_behavior.rs, faults.rs, invariants.rs, run.rs, scenario.rs, shrink.rs, report.rs, toy.rs}` | DST 框架(`sut.rs` 引擎边界、`obs.rs` 调试视图行、`run.rs` 主循环、`scenario.rs` 场景与套件、`toy.rs` 框架自测引擎) |
| `src/scenarios/<主题>.rs` | 以 `TestStrategies` 运行的场景套件与按用例编号注册的场景(仓库根 `tests/dst/` 只是 cargo test 入口) |
| `src/bin/hft-dst.rs`(feature `dst-bin`) | `hft-dst` 运行器:`run`/`repro`/`list`、`DstRunResult` 输出与周期落盘、进行中种子标记、每种子的 `catch_unwind` |
| `src/hash.rs` | `HashTrail`、`diff_trails`、`bisect_divergence` |
| `src/alloc.rs`、`src/bench.rs` | `CountingAlloc`、`HistBench` |
| `src/chain_mocks/*`(`chain` feature) | 公共 RPC 端点集合、节点、builder、`SwapGuard` 语义模型 |

</details>

## 延迟与资源契约

dev 组件,无热路径约束。运行时间预算(GitHub 托管 `ubuntu-latest`,单核;CI 的时机与总时长归 [testing §6](../../docs/testing.md#6-ci-运行矩阵)):

| 任务 | 预算 |
|---|---|
| 每个 PR:`cargo xtask dst` 冒烟(200 个由提交 SHA 派生的种子,步数等参数归 [tools](../../tools/README.md#cargo-xtask-dst))+ `fixtures/dst-seeds/` 全集 | ≤ 3 分钟(PR 总目标见 [testing §6](../../docs/testing.md#6-ci-运行矩阵)) |
| 每夜回归冒烟:`cargo xtask dst`,种子由 UTC 日期派生,种子数与步数取 `tools/xtask/dst-nightly.toml`(参数归 tools;不计入门槛 ④) | ≤ 6 分钟(单核;整个每夜作业 ≤ 15 分钟含构建,[testing §6](../../docs/testing.md#6-ci-运行矩阵)) |
| 一致性集 `HashTrail` 核对(`cargo xtask dst --hashtrail check`;PR、每夜与版本标签各自核对的种子数归 [testing §6](../../docs/testing.md#6-ci-运行矩阵);版本标签在同一提交上于 x86_64 与 aarch64 各跑一次) | 全部登记种子每侧 ≤ 10 分钟 |
| 服务器:`hft-dst run`(`ci` 剖面,单核,由 `hft-verify` 按预算调用) | 吞吐待实测(按 `hft-dst` 默认步数每种子 0.1–0.7 s 估算);门槛 ④ 的达成时间估算见 [testing §6.1](../../docs/testing.md#61-服务器上的累计运行) |
| `cargo xtask scaffold-gate`(改动 `crates/`、`apps/`、`strategies/`、`tools/` 的 PR;复用主 target 目录,含一次 `hft-core --check` 装配) | ≤ 4 分钟(增量构建) |
| 全部金样(core 六个协议 + `fixtures/sim/`) | ≤ 30 秒 |
| DST 单步开销(不含被测代码) | ≤ 2 µs |

## 测试与验收

用例与验收见 [TESTS.md](TESTS.md)。

## 依赖

| 依赖 | 用途 |
|---|---|
| [`hft-sim`](../hft-sim/README.md)(默认 feature) | 最小撮合器与模拟真值 |
| [`hft-sequencer`](../hft-sequencer/README.md)、[`hft-oms`](../hft-oms/README.md)、[`hft-risk`](../hft-risk/README.md)、[`hft-ledger`](../hft-ledger/README.md)、[`hft-exec`](../hft-exec/README.md) | 被测对象与不变量所需的调试视图 |
| [`hft-strategy-sdk`](../hft-strategy-sdk/README.md)、[`hft-strat-lib`](../hft-strat-lib/README.md) | 测试策略、`MockViews` 实现的 trait |
| [`hft-venue-proto`](../hft-venue-proto/README.md)、[`hft-event`](../hft-event/README.md)、[`hft-refdata`](../hft-refdata/README.md)、[`hft-rt`](../hft-rt/README.md) | 类型与 trait |
| [`hft-chain-evm`](../hft-chain-evm/README.md) | 仅 `chain` feature(可选扩展) |
| `proptest`、`proptest-state-machine`、`rand_chacha`、`rand_core`、`xxhash-rust` | 生成、状态机测试、随机、哈希 |
| `hdrhistogram`(7.x,不开序列化与 sync 特性) | `HistBench` 的直方图 |

只允许出现在 `[dev-dependencies]`、`tools/xtask` 与本 crate 自己的 `hft-dst` 二进制(只进验证包,不进生产归档);任何非 dev 依赖它的 crate 由 `cargo xtask ci` 拒绝([crates/AGENTS.md](../AGENTS.md))。**禁止**依赖 [hft-strategy-registry](../../strategies/registry/README.md) 与任何 `strategies/*`:两个测试策略在本 crate 内;`strategies/registry` 与 `apps/hft-core` 以 dev 依赖使用本 crate 跑生产策略的集成测试(方向单一,无依赖环)。

## 延伸阅读

- [testing.md](../../docs/testing.md)(测试层级、不变量与门槛的唯一定义)、[fixtures/](../../fixtures/README.md)
- [hft-strategy-sdk](../hft-strategy-sdk/README.md)、[strategies/AGENTS.md](../../strategies/AGENTS.md)、[cookbook/adding-a-strategy](../../docs/cookbook/adding-a-strategy.md)
- [13 O8](../../design/13-设计评审与优化.md)(fuzz 与 DST)、[10 §5](../../design/10-语言分工与代码组织.md)(测试金字塔)、[research/04 §7](../../design/research/04-oss-frameworks-and-patterns.md)
- Agent Note:[strategy-sdk-decoupling](../../.agents/notes/proposed/architecture/2026-10-02-strategy-sdk-decoupling.md)、[fuzz-and-deterministic-simulation](../../.agents/notes/proposed/testing/2026-09-30-fuzz-and-deterministic-simulation.md)

## 已知限制与后续工作

- DST 只覆盖进程内逻辑;真实 socket、TLS、内核行为的故障由交易所测试网冒烟与混沌测试覆盖,wire 级 [`mock-exchange`](../../apps/mock-exchange/README.md) 是可选扩展(冻结)。
- 控制面(tokio 的 ctl-gateway 客户端侧)的网络混沌不在本框架;需要时用 `turmoil` 单独测。
- 多进程场景(可选扩展:hft-core ↔ chain-core)只在集成环境测;DST 以 `SeqActor` + 真实 `ChainSeq` 同进程模拟链路。
- `MockVenue` 的交易所行为层是"规格中的交易所",与真实交易所的差异只能由金样、测试网/模拟盘联调与正式实盘的运行监控发现。
- 当前构建:三个 DST 入口以 `ToyEngine` + `ToyMatcher` 运行;引擎适配器 `SeqSut<S>`(真实 sequencer、`SimExchange` 撮合、调试视图到 `Obs` 的抄录)、`run_sim_case`(TK-009)、`ProbeBuild` 注入(TK-095)与模板探针的构建检查(TK-042)随 hft-sequencer 与模拟撮合金样夹具落地;`hft-dst` 的 `SIGINT`/`SIGTERM` 处理属 M6。

### 开发备忘

<details><summary>给实现者</summary>

- **依赖 hft-sequencer 的快照分段**:组件级哈希要求快照按组件分段且段边界可定位(含执行层段)。
- **依赖被测 crate 的调试视图**:各 crate 以 `#[cfg(any(test, feature = "testkit"))]` 暴露 `debug_view()`,不进入生产构建。
- **避免 dev 依赖环**:被测 crate 的单元测试只用本 crate 中**不引用被测类型**的部分(生成器、假时钟、金样、`MockViews`);DST 与跨组件测试放在本 crate 的 `tests/` 或 xtask 里。
- 引擎适配器 `SeqSut<S>`(随 hft-sequencer 落地):`DstDriver` 实现 sequencer 的 `Driver`(`take` 交出 `offer` 的 Input,`out.tsc = stamp()`;`run_tsc`/`measure` = `stamp()`;`emit` 进收集缓冲;`journal` = `MemJournal`),`Matcher` 为 `hft_sim::SimExchange`,`observe` 把各组件 `debug_view()` 抄进 `Obs`、`StepInfo`。
- 实现顺序:假时钟与金样运行器 → 生成器 → `MockViews`/`StrategyHarness` → 两个测试策略与 `scaffold-gate` → 状态哈希工具 → DST 世界与 OMS/风控/执行层不变量 → 存储与毒丸故障 → `shrink` → 场景套件迁入 `src/scenarios/` 与 `hft-dst` → (可选)链上 mock。

</details>
