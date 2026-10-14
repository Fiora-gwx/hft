---
description: "策略唯一依赖的稳定接口:快速部分 Strategy、慢速部分 Model(检查点键、原始样本节与路径状态节、分块快照、内存与快照声明、按 group 重置、预热注入、本实例回报订阅)、参数 schema 与 ParamSet(含 SPB1 body 布局与字段属性)、Note 码表/报表字段/策略命令描述符、只读 Views、意图收集器 Ctx、订单意图与目标意图、投递给策略的事件与视图类型、StrategySet 装配宏,以及运行模式下意图的处理规则"
kind: crate
tier: pure
scope: core
status: spec
---

# hft-strategy-sdk

## 概要

`hft-strategy-sdk` 是策略与引擎之间**唯一的稳定接口**。策略 crate(`strategies/<name>`)只依赖它与通用构件 [hft-strat-lib](../hft-strat-lib/README.md);引擎 crate 用它定义的视图类型与意图类型和策略对话,**任何引擎 crate 都不依赖策略 crate**。它定义:快速部分 `Strategy`(sequencer 线程,热路径)、慢速部分 `Model`(advisor 线程,warm;检查点键、原始样本节与路径状态节、分块快照、内存与快照大小声明、参数变化时的 group 重置、预热注入的契约)、参数 schema 与 `ParamSet` 解码(含 SPB1 body 的字节布局与字段属性)、随 schema 导出的策略描述符(Note 码表、报表字段、策略命令载荷)、只读视图 `Views`/`ModelViews`、意图收集器 `Ctx`、`OrderIntent` 与 `TargetIntent`/`ExecSpec`、投递给策略的 `EventRef` 与各视图类型、装配用的 `StrategySet` trait 与 `strategy_set!` 宏、运行模式下意图的行为表。它不拥有状态、不实现算法:执行算法在 [hft-exec](../hft-exec/README.md),模式状态在 [hft-risk `mode`](../hft-risk/README.md),权益数据在 [hft-ledger](../hft-ledger/README.md),订单与 LegTracker 在 [hft-oms](../hft-oms/README.md)。本 crate 是 `pure` tier:无 I/O、无线程、无时钟;其中的 trait 方法由引擎在热路径上静态分派调用。新增策略 = 新 crate + registry 里一行,引擎 crate 与 app(含 hft-core 的规格与容量表)零改动:Model 的内存与检查点大小由策略经 `Model::mem_bytes`/`snapshot_bytes` 声明,hft-core 按声明求和;唯一例外是 group 数超过每实例上限 `max_groups_per_instance`,先单独提容量改动([cookbook](../../docs/cookbook/adding-a-strategy.md))。

## 目录

- [使用本组件](#使用本组件)
  - [依赖方向](#依赖方向)
  - [策略包的三部分](#策略包的三部分)
  - [Strategy 与 Model](#strategy-与-model)
  - [参数 schema 与 ParamSet](#参数-schema-与-paramset)
  - [策略描述符:Note、报表、策略命令](#策略描述符note报表策略命令)
  - [Views 与 ModelViews](#views-与-modelviews)
  - [视图类型](#视图类型)
  - [EventRef](#eventref)
  - [Ctx 与订单意图](#ctx-与订单意图)
  - [目标意图](#目标意图)
  - [Advice 载荷](#advice-载荷)
  - [预热注入](#预热注入)
  - [装配:StrategySet 与 strategy_set!](#装配strategyset-与-strategy_set)
  - [运行模式下意图的行为表](#运行模式下意图的行为表)
  - [确定性规则](#确定性规则)
  - [版本与兼容规则](#版本与兼容规则)
  - [chain 特性(optional)](#chain-特性optional)
  - [测试桩(testing 特性)](#测试桩testing-特性)
- [实现说明](#实现说明)
- [延迟与资源契约](#延迟与资源契约)
- [测试与验收](#测试与验收)
- [依赖](#依赖)
- [延伸阅读](#延伸阅读)
- [已知限制与后续工作](#已知限制与后续工作)

## 使用本组件

### 依赖方向

```
L0    hft-types ← hft-refdata ← hft-event
SDK   hft-strategy-sdk → L0(只此)
L1    hft-oms  hft-risk  hft-ledger  hft-exec → L0 + SDK(用 SDK 的视图与意图类型)
构件  hft-strat-lib → L0 + SDK
策略  strategies/<name> → SDK + hft-strat-lib(只此两者;外部库见各策略 README)
装配  hft-sequencer → SDK + L1(对 S: StrategySet 泛型,不认识任何具体策略)
登记  strategies/registry(hft-strategy-registry)→ strategies/*:真实策略的唯一列表(唯一列出真实策略的 strategy_set! 调用)
      apps/hft-core、apps/replayer → hft-strategy-registry(不直接依赖任何策略 crate)
测试  hft-testkit → SDK + 引擎 crate;只带自己的 CI 测试策略(kind_id 0xFF00–0xFFFF),不依赖 registry 与任何 strategies/*
```

- 引擎 crate(L0、L1、hft-sequencer、hft-journal、hft-venue*、hft-ctl*,含它们的 `[dev-dependencies]`)依赖任何 `strategies/*` 或 registry 即为违规;策略 crate 依赖 SDK 与 hft-strat-lib 之外的内部 crate 即为违规。机检:`cargo xtask deps`(`tools/xtask/deps.toml`),规则文本在 [crates/AGENTS.md](../AGENTS.md#依赖方向) 与 [strategies/AGENTS.md](../../strategies/AGENTS.md)。
- 策略用到的 L0 类型与常量(`Px`、`Qty`、`Rate`、`Notional`、`Tsc`、各 ID、`Side`、`Tif`、`GROUP_NONE`、`INST_NONE`、`INST_ALL`、`MAX_BBO_CONN_SLOTS`、`GroupSpec`/`LegDef`、`Level`、`NoteLevel`、`RejectBits`、`RunMode`)由本 crate 重导出(`hft_strategy_sdk::prelude`),策略 crate 的 `Cargo.toml` 只列 SDK 与 hft-strat-lib 两个内部依赖。
- 真实策略只在 [strategies/registry](../../strategies/registry/README.md) 列一次;hft-core 与 replayer 用它导出的同一个 `StrategySet`,不存在两份列表。hft-testkit 用自己的一次 `strategy_set!` 调用装配只含测试策略的 `TestStrategies`(只在测试构建中出现),引擎 crate 的测试用它;解耦门槛在 registry 的补丁副本里加一个 CI 测试策略再构建 hft-core([上线门槛](../../docs/testing.md#8-上线门槛) ⑧);**需要真实策略的引擎级测试**(场景、DST、恢复)放在 `strategies/registry/tests/` 或 `apps/hft-core/tests/`,以 registry 的 `Strategies` 为类型参数驱动 hft-testkit 的通用场景。策略 crate 自己的单测用 `testing` 特性,不需要装配。
- 引擎 crate 的 README 与代码不出现策略族名;引擎只提供通用机制与由参数给出的阈值。

### 策略包的三部分

| 部分 | 运行位置 | 输入 | 输出 | 允许 | 禁止 |
|---|---|---|---|---|---|
| 快速部分(`Strategy`) | sequencer 线程,每个分派给它的事件一次 | `EventRef` + `Views`(只读) | 经 `Ctx`:订单意图、目标意图、撤/改、定时器、Note | 整数定点运算、预分配的固定容量状态 | 分配、阻塞、系统调用、系统时钟、panic、`dyn`、f64/f32、`HashMap` 迭代 |
| 慢速部分(`Model`,可选) | advisor 线程(warm,承载方式见 [apps/hft-core](../../apps/hft-core/README.md)) | 它订阅的 journal 事件(含可选的本实例 `Order`/`Exec`/`Manual`)+ 预热注入(`EventRef::Warmup`)+ `ModelViews` | `Advice`(不透明载荷,经 advice 环进入 sequencer 并被 journal 记录) | f64、有界分配(上界由 `Model::mem_bytes` 声明)、`BTreeMap`/`Vec` | I/O、系统时钟、线程、并行归约、`HashMap` 迭代、非确定的数学库 |
| 参数 schema 与描述符(`ParamSchemaDesc`、`NoteDesc`、`ReportFieldDesc`、`CmdDesc`、`ModelLayoutDesc`) | 静态数据 | — | 字段名、单位、范围、默认值、字段属性、说明;Note 码表、报表字段、策略命令载荷;检查点布局 | — | 运行期修改 |

快速部分是唯一的热路径入口。慢速部分的输出是 journal 的 Input 记录:回放直接读 Advice,不重新运行 Model;replay 与 `replay verify` 永远不需要 Model 状态。Model 仍必须确定(见[确定性规则](#确定性规则)):重启时 advisor 用 [Model 检查点](#model-检查点) + journal 续算,检查点只是省去预热的优化;检查点含路径状态节时,续算结果与连续运行逐字节相同;没有可用检查点时(首次启动、状态布局变化、group 被重置)用[预热注入](#预热注入)把历史样本作为 Input 送进 Model,而不是等实时样本攒满。Python 只做研究与分钟级以上的选币/名单(经 `SetParams`)和预热样本的生产(经 `InjectWarmup`)。

### Strategy 与 Model

```rust
/// 静态元数据。kind_id 全局稳定、只追加(登记表在 strategies/registry);name 为 crate 名。
/// schema、notes、report、cmds 一起随规范 schema JSON 导出(见"策略描述符")。
pub struct StrategyDesc {
    pub name: &'static str,
    pub kind_id: u16,
    pub version: u16,                      // 快速部分快照布局与语义版本(写进快照行头 StrategyRowHdr.desc_version);改快照布局或决策语义 +1
    pub non_prod_only: bool,               // true = 只能绑定 AccountMeta.is_prod = false 的账户(见下)
    pub schema: &'static ParamSchemaDesc,
    pub notes: &'static [NoteDesc],        // 本策略经 Ctx::note 写的全部 Note 码
    pub report: &'static [ReportFieldDesc],// 日报的策略专项字段(由 Note 通用汇总)
    pub cmds: &'static [CmdDesc],          // 支持的 StrategyCmd 载荷;空 = 不接受策略命令
}

/// 一个策略实例 = (strategy, account);一个账户只绑定一个策略实例(AccountMeta.bound_instance,见 hft-refdata)。
/// 实例看不到账户类型(Live/Sim):策略代码在模拟账户与实盘账户上完全相同,不得区分。
#[derive(Clone, Copy)]
pub struct StrategyInstance { pub strategy: StrategyId, pub account: AccountId, pub kind_id: u16 }

pub trait Strategy: Sized {
    const DESC: StrategyDesc;
    type Params: ParamSet;                                   // 已解码、已校验的定点参数
    type Model: Model<Params = Self::Params>;                // 无慢速部分时用 NoModel<Self::Params>

    /// 命令路径(sequencer 线程,非热路径):按 GroupTable::capacity() 一次性预分配全部 group 状态。
    fn new(inst: StrategyInstance, p: &Self::Params, groups: &GroupTable, rd: &RefView<'_>) -> Result<Self, ParamError>;
    /// 加载与每次参数应用后调用;sequencer 据此重建 inst → [(实例, gix)] 的稠密索引。
    fn subscriptions(&self, groups: &GroupTable, out: &mut SubsBuilder);
    /// 唯一热路径入口:不得分配、阻塞、读系统时钟、panic;无 f64。
    fn on_event<V: Views>(&mut self, ev: &EventRef<'_>, v: &V, ctx: &mut Ctx<'_>);
    /// SetParams 应用前调用(命令路径);Err → 旧参数继续生效、CmdResult 带错误。
    /// group 集合可能变化:跨版本以 GroupId 对齐状态,gix 只在一个参数版本内有效;
    /// 新 GroupId 或指纹变化的 group 从空状态起步(见"参数变化时的 group 重置")。
    fn on_params(&mut self, p: &Self::Params, groups: &GroupTable, rd: &RefView<'_>) -> Result<(), ParamError>;
    /// 写本实例全部快速部分状态(关键帧中一次写完,不分片)。
    fn snapshot(&self, w: &mut SnapshotWriter);
    /// r.desc_version() = 写出该节时的 StrategyDesc.version(见"版本与兼容规则")。
    fn restore(&mut self, r: &mut SnapshotReader) -> Result<(), RestoreError>;
}

pub trait Model: Sized {
    type Params: ParamSet;                                   // 与所属 Strategy 相同(由 Strategy::Model 的约束绑定)
    /// 原始样本节的布局版本,是检查点键的一部分;纪律见"Model 检查点"。
    const LAYOUT_VERSION: u32;
    /// 原始样本节的布局描述(节标签、元素类型、长度规则);CI 比对它的规范 JSON 金样。
    const LAYOUT: &'static ModelLayoutDesc;
    /// 路径状态节的版本(不在检查点键里;不符只丢路径节)。0 = 本 Model 没有路径状态,snapshot_path 不写任何字节。
    const PATH_VERSION: u32;
    /// 本 Model 可经预热注入补样本(策略 README 有"预热载荷"表、随包的 pipeline 插件实现 warmup);缺省 false。
    /// 只用于结构参数改动预览的估算(见"参数变化时的 group 重置"),不改变任何运行行为。
    const WARMUP: bool = false;
    /// 堆内存上界(字节):参数 p、n_groups 个 group 时本实例 Model 的全部分配(含 new、on_params、预热与满窗口)。
    /// 纯函数(只看参数与 group 数)、冷路径;使用方见"Model 内存与快照声明"。
    fn mem_bytes(p: &Self::Params, n_groups: u16) -> u64;
    /// 检查点上界(字节):snapshot_path 与全部 snapshot_chunk 输出之和。
    fn snapshot_bytes(p: &Self::Params, n_groups: u16) -> u64;
    /// 路径节被丢弃后,输出与连续运行重新逐字节一致所需的最长引擎时间(ms,停机不计);PATH_VERSION = 0 时为 0。纯函数、冷路径。
    fn path_settle_ms(p: &Self::Params) -> u64;
    /// 该 group 从空状态起步(无预热注入)到满足开仓所需样本的最长事件时间(ms);无 Model 或不依赖样本的为 0。
    /// 纯函数、冷路径;结构参数改动的预览用它(见"参数变化时的 group 重置")。
    fn cold_start_ms(p: &Self::Params, gix: u16) -> u64;
    fn new(inst: StrategyInstance, p: &Self::Params, groups: &GroupTable, rd: &RefView<'_>) -> Self;
    /// advisor 尾随到 Applied 的 SetParams/ReplaceRefdata 后调用;检查点恢复后也会以重放起点的参数调用一次。
    /// 必须幂等(同一参数重复应用结果不变);以 GroupId 对齐状态;返回本次从空状态起步的 group 与原因。
    fn on_params(&mut self, p: &Self::Params, groups: &GroupTable, rd: &RefView<'_>) -> ModelParamEffect;
    /// 行情、模型 tick、预热注入(EventRef::Warmup)、订阅了的本实例 Order/Exec/Manual 都经这里。
    fn on_event<V: ModelViews>(&mut self, ev: &EventRef<'_>, v: &V, out: &mut AdviceOut<'_>);
    /// 路径状态节(检查点的第一块,一次写完):路径依赖、不能由原始样本重算的派生状态
    /// (分位带及其更新时刻、平滑窗、上次发送的载荷摘要等)。PATH_VERSION = 0 时不写。
    fn snapshot_path(&self, w: &mut SnapshotWriter);
    /// 读路径状态节(只在文件中的版本 = PATH_VERSION 时由 advisor 调用,在 restore 之后);
    /// 出错时不得留下部分写入的状态(先解码到临时值再整体替换)。
    fn restore_path(&mut self, r: &mut SnapshotReader) -> Result<(), RestoreError>;
    /// 原始样本节分块写出:from_gix = 0 的块先写实例级节;随后从 from_gix 起按 gix 升序写完整的 group 记录,
    /// 再写一个 group 会使本块超过 max_bytes 时停(每块至少一个 group,无 group 时只写实例级节)。
    fn snapshot_chunk(&self, from_gix: u16, w: &mut SnapshotWriter, max_bytes: u32) -> SnapshotProgress;
    /// 读原始样本节(全部块按顺序拼接,advisor 启动时一次完成);恢复原始样本并重算可重算的派生状态,
    /// 路径状态按原始样本给出近似值(随后若路径节可用,restore_path 覆盖它)。
    fn restore(&mut self, r: &mut SnapshotReader) -> Result<(), RestoreError>;
}
pub enum SnapshotProgress { More(u16 /* 下一块的 from_gix */), Done }
pub enum ModelParamEffect { None, ResetGroups(GroupResets), ResetAll }
/// 重置原因;码值与 hft-event `Note{ModelGroupReset}` 的 v[0] 逐值相同(ResetAll 在该 Note 中记 4)。
#[repr(u8)]
pub enum ResetCause {
    Structural = 1,      // 结构参数(FieldDesc.structural)在该 group 上的取值变化
    Fingerprint = 2,     // 新 GroupId(无旧指纹)或 GroupSpec::fingerprint() 变化
    PctConfig = 3,       // 检查点里保存的存储解释配置(如 hft-strat-lib PctConfig)与当前参数推出的不一致(停机期间改了结构参数)
}
pub struct GroupResets { /* 本参数版本每个 gix 至多一个 ResetCause,容量 = GroupTable::capacity();命令路径分配 */ }
impl GroupResets {
    pub fn new(cap: u16) -> GroupResets;
    pub fn set(&mut self, gix: u16, c: ResetCause);          // 同一 gix 多个原因时保留码值最小者
    pub fn get(&self, gix: u16) -> Option<ResetCause>;
    pub fn iter(&self) -> impl Iterator<Item = (u16, ResetCause)> + '_;   // gix 升序
}

/// 无慢速部分的策略用它:on_event 为空;LAYOUT_VERSION = PATH_VERSION = 0、WARMUP = false、LAYOUT 无节、
/// mem_bytes = snapshot_bytes = path_settle_ms = cold_start_ms = 0,不写检查点。
pub struct NoModel<P>(core::marker::PhantomData<P>);
impl<P: ParamSet> Model for NoModel<P> { type Params = P; const LAYOUT_VERSION: u32 = 0; const PATH_VERSION: u32 = 0; /* … */ }

pub struct GroupTable { /* 本实例本参数版本的 GroupSpec 列表;gix = 在 SetParams blob 中的下标 */ }
impl GroupTable {
    pub fn new(specs: &[GroupSpec], capacity: u16) -> GroupTable;   // 命令路径;长度可超过 capacity,由 ParamSet::decode 返回 TooManyGroups
    pub fn len(&self) -> u16;
    pub fn capacity(&self) -> u16;                           // = max_groups_per_instance(见下)
    pub fn get(&self, gix: u16) -> &GroupSpec;               // GroupSpec / LegDef 的布局在 hft-event;越界 → 全零 GroupSpec
    pub fn gix_of(&self, g: GroupId) -> Option<u16>;         // 有序数组二分;命令路径与跨事件对齐用
    pub fn iter(&self) -> impl Iterator<Item = (u16, &GroupSpec)>;
}
/// GroupSpec 的指纹:FNV-1a 64,覆盖 asset、n_legs 与前 n_legs 条 LegDef 的 (inst, venue, kind, route),不含 open_allowed。
/// 输入字节 = asset(u16 小端)‖ n_legs(u8)‖ 每条腿 inst(u32 小端)‖ venue(u8)‖ kind(u8)‖ route(u32 小端);
/// 偏移基 0xcbf29ce484222325、素数 0x100000001b3(Python 侧与 pipeline 按同一字节计算)。
/// 以扩展 trait 提供(GroupSpec 定义在 hft-event),调用形式 GroupSpec::fingerprint(&g) 或 g.fingerprint()。
pub trait GroupSpecExt { fn fingerprint(&self) -> u64; }
impl GroupSpecExt for GroupSpec { /* … */ }

pub struct RefView<'a> { /* 借用 RefData,只暴露本实例需要的部分 */ }
impl<'a> RefView<'a> {
    pub fn new(rd: &'a RefData, account: AccountId) -> RefView<'a>;   // account = 本实例绑定的账户
    pub fn inst(&self, i: InstId) -> &'a InstHot;            // refdata 热行:市场类型、tick、lot、最小名义、资产、计价资产
    pub fn asset(&self, a: AssetId) -> &'a AssetMeta;
    pub fn insts_of(&self, a: AssetId) -> &'a [InstId];      // refdata 中该资产在全部 venue 上的 inst(InstId 升序),含不在本账户 venues 内的
    pub fn caps(&self, v: VenueId) -> &'a VenueCaps;          // VenueId 按产品线(某所现货、某所 USDT 永续各一个)
    pub fn fee(&self, a: AccountId, i: InstId) -> FeeView;    // { maker: Rate, taker: Rate },来自 FeeSchedule
    pub fn account(&self) -> &'a AccountMeta;                // 只有本实例绑定的账户
    /// 可失败的查询(参数里的 inst 未经装配校验时用,如 hft-strat-lib fee_round_trip):
    /// try_inst:inst 不在 refdata → None;try_fee:inst 不在 refdata、a 不是本实例绑定的账户、
    /// 或 inst 的 venue 不在该账户 venues 内 → None。inst/fee 只用于本实例 GroupSpec 引用或已订阅的 inst(装配时已校验存在),
    /// 对其他输入不保证结果。
    pub fn try_inst(&self, i: InstId) -> Option<&'a InstHot>;
    pub fn try_fee(&self, a: AccountId, i: InstId) -> Option<FeeView>;
}

pub enum Part { Fast, Model, Both }
bitflags! { pub struct EvMask: u8 { const BBO = 1; const BOOK = 2; const TRADE = 4; const FUNDING = 8; const MARK = 16;
                                    const INDEX = 32; const LIQUIDATION = 64; } }
bitflags! { pub struct OwnEvMask: u8 { const ORDER = 1; const EXEC = 2; const MANUAL = 4; } }   // SubsBuilder::model_own
/// Full:该 inst 的订阅条目(全部实例合计)已达 max_subs_per_inst,或需要新的订阅下标而 max_insts 已用尽(容量归 hft-sequencer
/// ECF1 段 3);NoFeed 见 market。两者都由策略处理(不订阅并写 Note),不使 SetParams 失败。
pub enum SubsErr { Full, NoFeed }
/// 引擎侧的订阅限额(sequencer 按 ECF1 段 3 与其他实例已有的条目填写):feeds = 本 hft-core 实例有 feed 的 venue;
/// room[inst] = 本实例对该 inst 还能加的条目数(没有订阅下标且 max_insts 已用尽时为 0)。
pub struct SubsLimits<'a> { pub rd: &'a RefData, pub feeds: VenueSet, pub room: &'a [u16] }
impl<'a> SubsBuilder<'a> {
    pub fn new(lim: SubsLimits<'a>, cap_entries: usize) -> SubsBuilder<'a>;   // 命令路径分配一次;reset(lim) 复用
    /// 行情订阅;同一 inst 被多个 group 订阅时每个 group 各投递一次(按 gix 升序)。inst 可以是 refdata 中任一 inst,
    /// 包括不在 group 腿里、venue 不在本账户 venues 内的(参考行情:只读,不能下单);该 venue 在本 hft-core 实例没有 feed → NoFeed。
    pub fn market(&mut self, inst: InstId, gix: u16, ev: EvMask, part: Part) -> Result<(), SubsErr>;
    /// 模型 tick:advisor 以 journal 中的 Timer{Sys, AdvisorTick} 记录为时间刻度,在该记录的 UTC 时刻(Now.utc_ns 口径;
    /// 记录头没有 UTC 字段)跨过 every_ms 整数倍边界(UTC 对齐)的第一条记录上向 Model 投递(跨过 = journal 中前一条 AdvisorTick
    /// 的 UTC 时刻与本条的 UTC 时刻落在不同的 every_ms 区间;journal 的第一条 AdvisorTick 不投递)
    /// EventRef::Timer{tag: MODEL_TICK, group: GROUP_NONE, gix: GIX_NONE}(确定:刻度来自 journal;与重启、检查点位置无关)。
    /// 分辨率 = AdvisorTick 的间隔(取值归 hft-sequencer 定时器表);every_ms 不是它的整数倍时实际间隔按刻度取整,
    /// SDK 不拒绝——需要精确周期的策略在自己的 check() 里要求整数倍。
    pub fn model_tick(&mut self, every_ms: u32);
    /// Model 订阅本实例的 Order/Exec/Manual(只读;不调用 = 不投递)。值与快速部分收到的同一事件相同,
    /// 由 advisor 镜像从 journal 重建(规则归 hft-sequencer advisor),回放与重启重放结果相同。
    pub fn model_own(&mut self, m: OwnEvMask);
    /// 预热注入的投递范围:不调用 = 只投递给 Model(Part::Model);Part::Both = 快速部分也收(见"预热注入")。
    pub fn warmup(&mut self, part: Part);
    /// 链上报价订阅(chain 特性):该 group 链上腿的 PoolQuote → EventRef::Pool。
    #[cfg(feature = "chain")]
    pub fn pool(&mut self, gix: u16, part: Part) -> Result<(), SubsErr>;
}
```

- 账户级事件(`Order`、`Exec`、`External`、`Risk`、`Mode`、`Advice`、`Manual`、`Warmup`、本实例登记的 `Timer`)按归属投递给快速部分,不需订阅;Model 收 `Order`/`Exec`/`Manual` 需 `SubsBuilder::model_own`(只读,不能据此下单,只经 Advice 影响快速部分),不收 `External`、`Risk`、`Mode`、`Conn`;`Notice` 投递给订阅了该 inst 的 group(不论订阅时的 `Part`,两部分都收);`Conn` 投递给订阅了该 venue 任一 inst 的实例。两部分各收哪些变体以 [EventRef](#eventref) 节的投递表为唯一定义(advisor 按它构造 Model 输入)。
- `EvMask::BOOK` 的数据来自该 inst 的深度路由:默认是分档快照流(`RouteKind::DepthTopN`,每条 `L2Snapshot{origin = TopN}` 整替换,簿只有 N 档),全量增量深度是可选扩展;路由与档数归 [hft-venue](../hft-venue/README.md),策略只经 `BookRead` 读,不区分来源。
- `new`/`on_params`/`restore` 属命令路径(sequencer 线程,不在 tick-to-trade 上),允许按 `capacity()` 分配一次;`on_event` 零分配。被移除 group 的定时器在应用参数时撤销并写 `Note`;有仓位的 group 不得移除(`PreconditionFailed{GroupHasPosition}`,校验归 [hft-sequencer](../hft-sequencer/README.md))。
- **参考行情订阅**:需要不交易的 venue 的行情(领先-滞后的参考所、指数价)时,策略用 `RefView::insts_of(asset)` 找到 inst,再以某个 group 的 gix 调 `market`;事件照常以该 gix 投递。该 venue 须在本实例配置了 feed([apps/hft-core](../../apps/hft-core/README.md) `[venues.<v>]`),feed 的订阅集合随各实例订阅更新(归 [hft-sequencer](../hft-sequencer/README.md))。`NoFeed` 与 `Full` 由策略处理(不订阅并写 Note),不使 `SetParams` 失败。
- **每实例 group 上限 `max_groups_per_instance`**(u16;名字与语义归本 crate):值随 EngineConfig(ECF1)段 3 `Capacities` 下发(段的字段表归 [hft-sequencer](../hft-sequencer/README.md),段登记在 [hft-refdata](../hft-refdata/README.md)),各主机类的取值在 [apps/hft-core 的容量表](../../apps/hft-core/README.md#内存与-journal-段按主机类唯一归属)。`GroupTable::capacity()` 即此值,`n_groups` 超过 → `ParamError::TooManyGroups`;策略按它预分配。全进程的 cell 总数另受 `max_cells_total` 约束,两者不是同一个量。策略需要更多 group 时不在策略改动里改容量表,先单独提 hft-core 容量改动([cookbook](../../docs/cookbook/adding-a-strategy.md))。
- **一个 group 的两条腿不在同一交易所**:同一账户的 venue 集合里不会有两个 `exchange` 相同的 VenueId(refdata 加载校验 `SameExchangeMarkets`,[hft-refdata](../hft-refdata/README.md)),ctl-gateway 对 `SetParams` 另做通用校验(`InvalidArgs{SameExchangeLegs}`)。因此不存在"同所现货 × 永续"的 group,各 venue 的现金与保证金互不共用,策略按 venue 读资金即可(见 [PortfolioView](#视图类型))。
- **`non_prod_only`**:为真的策略只能绑定 `AccountMeta.is_prod = false` 的账户(交易所测试网/模拟盘账户、Sim 账户)。refdata 把它的实例绑定到 `is_prod = true` 的账户 → hft-core 装配退出 2(`ProbeOnProdAccount`,[apps/hft-core](../../apps/hft-core/README.md));对正式账户上该实例的 `SetParams` → `ParamInvalid{NonProdOnly}`(ctl-gateway 先查、sequencer 复核,detail 码归 [control-plane](../../docs/subsystems/control-plane.md))。用于只产生测量样本、不得在正式资金上运行的策略([latency-probe](../../strategies/latency-probe/README.md))。

#### Model 内存与快照声明

- `Model::mem_bytes(p, n_groups)` 与 `Model::snapshot_bytes(p, n_groups)` 是**上界**,只依赖参数与 group 数。使用方:hft-core 装配时对本实例全部已绑定策略实例求和,内存超过 `advisor.mem_budget_mib`、或检查点磁盘(按 `snapshot_bytes`)超过上限 → 退出 2(预算取值与规则在 [apps/hft-core 内存表](../../apps/hft-core/README.md#内存与-journal-段按主机类唯一归属));运行中 `SetParams` 与绑定变更时,ctl-gateway 用 `S::model_mem_bytes`/`S::model_snapshot_bytes` 算出该实例的新值,sequencer 换入后对全部实例求和复核同样两项,超出 → `ParamInvalid{ModelMemBudget}`,旧参数继续生效(求和与复核归 [hft-sequencer 命令路径](../hft-sequencer/README.md))。
- 因此新增策略不改 hft-core 的规格与容量表;声明超出预算时由运维调大 `advisor.mem_budget_mib` 或缩小参数(窗口、采样间隔、箱宽、group 数)。改会影响内存的参数同样经此校验,参数不会把 advisor 推到 OOM。
- 验收:Model 确定性夹具在最大 group 数、满窗口、预热之后用计数分配器断言实际分配 ≤ `mem_bytes`,快照字节 ≤ `snapshot_bytes`(用例见 [TESTS.md](TESTS.md) SDK-063、SDK-064)。

#### Model 检查点

Advice 是 journal 的 Input,回放与 `replay verify` 只读 Advice,**从不需要 Model 状态**;Model 状态只影响重启后的预热时长。因此 Model 状态不进 journal,而是由 advisor 写**检查点文件**。本节规定检查点**键的语义**与 Model 侧的义务;文件格式(文件头布局、`<strategy_instance>.ckpt` 与 `.prev` 回退)、目录、周期 `advisor.model_checkpoint_ms`、磁盘上限归 [apps/hft-core 检查点表](../../apps/hft-core/README.md#内存与-journal-段按主机类唯一归属);读写流程由 advisor([hft-sequencer](../hft-sequencer/README.md))实现。

1. **检查点键**(只有这五项):`kind_id`、`Model::LAYOUT_VERSION`、`schema.id`、策略实例(`strategy`, `account`)、`last_seq`(Model 已消费的最后一条 journal 记录)。**参数版本、`schema.version`、refdata 版本、二进制版本(git SHA)都不在键里**:改参数、加参数字段、换 refdata、发新版都不使检查点失效;参数与 refdata 的变化随重放经 `model_on_params` 重新应用(结构参数的变化按[参数变化时的 group 重置](#参数变化时的-group-重置)处理)。
2. **文件与分块**:文件头携带上述键(布局见 [apps/hft-core](../../apps/hft-core/README.md)),随后依次是**路径状态节**(带写出时的 `PATH_VERSION` 与长度;`snapshot_path` 一次写出,是检查点的第一块)与**原始样本节**(各次 `snapshot_chunk` 输出按顺序拼接)。advisor 每次 `poll_once` 至多写一块,`max_bytes` = `advisor.checkpoint_chunk_kib` × 1024(取值归 [apps/hft-core](../../apps/hft-core/README.md) 配置表);同一份检查点的各块之间整个 advisor 不消费 journal(镜像为各实例共享,只停一个 Model 会让它漏事件;各块对应同一个 `last_seq`,期间落后计入 `AdvisorLag`),规则归 [hft-sequencer advisor](../hft-sequencer/README.md)。两节合计超过 `snapshot_bytes` → 本次作废、上一代保留、P1。写盘(临时文件、fsync、原子改名)在 advisor 线程的冷片段完成,不影响 sequencer。
3. **重启**:读检查点 → 键的前四项与当前实例一致、`last_seq` 仍在可读的 journal(热段或冷副本)内、CRC 正确 → `Model::restore`(原始样本节)→ 文件中路径节的版本 = `Model::PATH_VERSION` 时 `Model::restore_path`;版本不符或 `restore_path` 出错 → **只丢路径节**(派生状态保持 `restore` 按原始样本给出的值,短暂不一致可接受),advisor 写 `Note{ModelPathReset}`(source `Advisor`;v[] 含原因、检查点 `last_seq` 与 `path_settle_ms`,编码在 [hft-event](../hft-event/README.md)),检查点其余部分照常使用 → 以 `last_seq` 处生效的参数与 refdata 调一次 `model_on_params` → advisor 从 `last_seq + 1` 起重放 journal(只投递该 Model 订阅的记录与该实例的 `Warmup`;遇到 Applied 的 `SetParams`/`ReplaceRefdata` 照常调 `model_on_params`)追上实时。重放期间产生的 Advice 照常经 advice 环进入 sequencer;依据上次启动的记录产出的(`basis_seq < Now.boot_seq`)一律过期,依据本次启动记录产出的按 `basis_tsc` 判年龄(见 [Advice 载荷](#advice-载荷)),所以追赶期间的 Advice 不会被当成新的。
4. **丢弃** → `Model::new` 空状态起步,写 `Note{ModelCheckpointDiscarded}`(source `Advisor`,v[1] = 原因,编码在 [hft-event](../hft-event/README.md)):`Missing`(文件缺失)、`Corrupt`(magic/长度/CRC 错)、`KeyMismatch`(`kind_id`、`LAYOUT_VERSION`、`schema.id`、实例任一不同)、`SeqNotInJournal`(`last_seq` 已不可读)、`RestoreErr`(`restore` 返回错误);此后靠[预热注入](#预热注入)或实时样本就绪,策略在就绪前按"样本不足"处理。
5. **检查点的两节**:
   - **原始样本节**(`snapshot_chunk`/`restore`,布局由 `LAYOUT` 与 `LAYOUT_VERSION` 管):全部原始状态,以 `GroupId` 为键;能由原始样本精确重算的派生状态(直方图、排序结构、累加器)不进检查点,在 `restore` 时重算。改打分或统计逻辑不需要提升 `LAYOUT_VERSION`、不丢长窗口统计;只有原始样本的布局或含义变化才需要。
   - **路径状态节**(`snapshot_path`/`restore_path`,版本 `PATH_VERSION`):路径依赖、不能由原始样本精确重算的派生状态——按时刻更新的分位带或阈值及其更新时刻、平滑窗、上次发送的载荷摘要、"状态变化即重算"类触发的基准值等。它们的布局或计算逻辑变化时提升 `PATH_VERSION`,代价只是下次重启丢一次路径节,原始样本与长窗口统计不受影响。
   - 两节合起来使"检查点 → 恢复 → 续算"与连续运行逐字节相同(Advice 字节与 Model 状态),且与分块大小无关。周期性更新的派生状态按 UTC 边界对齐(更新时刻 = 周期的整数倍,模型 tick 同样对齐,见 `SubsBuilder::model_tick`),所以重启前后的更新时刻相同。
6. **版本纪律**:原始样本节的布局(`LAYOUT`)或已存样本的含义变化时必须提升 `LAYOUT_VERSION`(提升 = 接受一次检查点丢弃,用预热注入缩短);不变时不得提升。每个有 Model 的策略提交 `LAYOUT` 的规范 JSON 作为**布局金样**(`fixtures/strategies/<name>/ckpt/layout.json`):金样变化而 `LAYOUT_VERSION` 未升 → CI 失败;`testing::check_layout` 用记录型 writer 跑一遍 `snapshot_chunk`,断言实际写出与 `LAYOUT` 一致(描述不能与代码分叉)。`PATH_VERSION > 0` 的 Model 另提交**路径金样**(`fixtures/strategies/<name>/ckpt/path.bin`:固定事件脚本跑完后 `snapshot_path` 的字节):字节变化而 `PATH_VERSION` 未升 → CI 失败。原始样本节不提交字节金样。
7. **确定性与每日 recompute**:SDK 的 Model 确定性夹具([TESTS.md](TESTS.md) SDK-079、SDK-080)与每日 `--advice recompute`(规则归 [testing](../../docs/testing.md) 与 [replayer](../../apps/replayer/README.md))都要求恢复后的 Advice 与连续运行逐字节一致。唯一例外是写了 `Note{ModelPathReset}` 的那次恢复,比较从**重建点**开始:重建点 `recompute_from_seq` = 该 Note 的检查点 `last_seq`(v[1])之后、引擎时间首次 ≥ `last_seq` 所指记录的引擎时间 + `path_settle_ms`(v[2])× 10^6 的那条 journal 记录的 seq(引擎时间 `engine_ns` 由记录头 `tsc` 与 `EngineClock` 算出、停机不计,公式归 [event-model 时间戳语义](../../docs/subsystems/event-model.md#时间戳语义);停机不计使比较起点只会更晚);`basis_seq < recompute_from_seq` 的 Advice 不比较,其余照常逐字节比较。`path_settle_ms` 是路径状态从近似值收敛到与连续运行一致的上界,由策略按自己的路径状态给出:到下一个周期性更新边界(如分位带的 UTC 边界)的最长等待 + 最长平滑窗长度 + 由它们触发的下一次发送间隔(例:按 UTC 分位带边界重算、按心跳发送的策略,其 `path_settle_ms` 即下一个分位带边界 + 平滑窗长度之后再过一个心跳与分配周期)。replayer 把重建点写进日报的 `recompute_from_seq`([testing §3.5](../../docs/testing.md#35-验证回放))。

```rust
pub struct ModelLayoutDesc { pub sections: &'static [SectionDesc] }   // 规范 JSON:键按字段顺序,testing::layout_json 生成
pub struct SectionDesc {
    pub tag: u16,                        // SnapshotWriter::section(tag);本 Model 内唯一
    pub name: &'static str,
    pub scope: SectionScope,             // Instance:实例级一次(位于 from_gix = 0 的块首);Group:每 group 一次,以 GroupId 开头
    pub elems: &'static [ElemDesc],      // 按写出顺序
}
pub enum SectionScope { Instance, Group }
pub struct ElemDesc { pub name: &'static str, pub ty: ElemTy, pub len: LenRule }
pub enum ElemTy { U8, U16, U32, U64, I32, I64, I128, F64Bits, Bytes }
pub enum LenRule { One, Fixed(u32), Prefixed /* 前置 u32 元素个数 */ }
```

#### 参数变化时的 group 重置

1. **GroupId 由构造保证唯一、永不复用**:`GroupId = (StrategyId << 24) | 实例内计数器`(计数器 1..=0xFF_FFFE 单调递增,StrategyId ≤ 254),所以同一策略的不同实例(含 dev2 上的多个模拟实例)不会撞号;移除后不再分配给别的 group。构造与校验规则归 [hft-types](../hft-types/README.md);新 group 计数器的续号(含 blob 头只增不减的 `group_ctr_hwm`)归 [tools hft-cfg](../../tools/README.md#hft-cfg),pipeline 用同一规则。策略不解析 GroupId 的位,只把它当不透明键。
2. **指纹比对**:快速部分与 Model 的 `on_params` 对每个 GroupId 比对 `GroupSpec::fingerprint()` 与上一参数版本的值(各自随状态保存);新 GroupId 或指纹不一致 → 该 group 的状态从空起步(快速部分的 group 状态、Model 的样本与统计)。sequencer 另记每个 group 的 `spec_seq`(当前指纹生效的那条 `SetParams` 记录的 seq),丢弃 `basis_seq < spec_seq` 的 Advice(规则归 [hft-sequencer](../hft-sequencer/README.md)),所以依据旧资产或旧腿产出的 Advice 不会用在新 group 上。
3. **结构参数**:`FieldDesc.structural = true` 的字段决定已存样本的解释(箱宽、范围、窗口、采样间隔)。结构参数允许随 `SetParams` 热更:Model 把决定存储解释的配置随状态保存(hft-strat-lib 的分位构件把 `PctConfig` 写进快照),`on_params` 比对它与新参数推出的配置,不一致的 group 重置——按 group 覆盖的字段只影响该 group,默认值变化影响全部未覆盖的 group。非结构字段的变化不得重置状态。
   - **结构参数改动的确认与预览**:改动任何 `structural` 字段取值的 `SetParams` 在 Live 实例上属升级类(需确认;类别规则与"Live 实例"的定义归 [control-plane](../../docs/subsystems/control-plane.md));预览显示受影响的 group 数与预计不能开仓的时长,由 ctl-gateway 调 `StrategySet::reset_preview` 算出 `ResetPreview{reset_groups, no_open_ms}`,两值随 `SetParams` 的解析结果交给 sequencer、原样填进 hft-event `CmdPreview` 的同名字段(传递归 [hft-sequencer](../hft-sequencer/README.md);估算口径只在此处):
     - **`reset_groups`** = 新参数 `GroupTable` 中将从空状态起步的 group 数(按 GroupId 与旧参数对齐):新 GroupId、`fingerprint()` 变化、或 `define_params!` 生成的 `structural_changed` 为真(该 group 的结构字段生效值变化,含未覆盖 group 随默认值变化)。与 `Model::on_params` 对同一组新旧参数实际报告的重置个数相同。
     - **`no_open_ms`** = 这些 group 的估算值 `est(g)` 的最大值(无受影响 group 为 0;饱和到 u32::MAX):`Model::WARMUP = false` 时 `est(g) = cold_start_ms(new, gix)`(只靠实时样本攒满);`WARMUP = true` 时 `est(g) = min(cold_start_ms(new, gix), path_settle_ms(new))`(假设 pipeline 按默认的自动预热为这些 group 补齐原始样本,剩下的是路径状态的收敛)。不含预热取数与注入本身的耗时(由 pipeline 的数据源决定);自动预热关闭或失败时实际时长回到 `cold_start_ms`。估算只依赖新旧参数与 SDK 声明,确定、冷路径。
   - pipeline 插件不得拥有 `structural` 字段(声明在 `owned_fields` 里 → pipeline 加载插件时拒绝,规则归 [pipeline](../../python/pipeline/README.md));预热回看窗口由插件从当前生效的策略参数读取,不在插件 policy 里另设。
4. **报告**:`Model::on_params` 返回本次从空状态起步的 group 与原因为 `ResetGroups(GroupResets)`(新 GroupId 与指纹变化记 `Fingerprint`,结构字段取值变化记 `Structural`,检查点里的存储解释配置与当前参数不一致记 `PctConfig`),全部重置为 `ResetAll`,无则 `None`。advisor 对每个被重置的 group 写一条 `Note{Advisor, ModelGroupReset}`(`Note.group` = 该 GroupId,v[0] = 原因码;编码归 [hft-event](../hft-event/README.md));pipeline 框架见到后对这些 group 自动发起预热注入([pipeline](../../python/pipeline/README.md#model-预热)),同一 group 的同一次重置只预热一次(去重规则也归 pipeline)。检查点恢复后的那次 `on_params` 同样报告(通常为 `None`)。

### 参数 schema 与 ParamSet

```rust
pub trait ParamSet: Sized + Send {
    /// ctl-gateway 线程(冷)调用:按 schema 序编码解码 body;可分配(按 group 展开覆盖表)。
    fn decode(body: &[u8], schema_ver: u16, groups: &GroupTable) -> Result<Self, ParamError>;
    /// 跨字段与按 group 形态的校验(腿数、市场类型、资产一致);decode 之后、应用之前。
    fn validate(&self, groups: &GroupTable, rd: &RefView<'_>) -> Result<(), ParamError>;
}

pub struct ParamSchemaDesc { pub id: u32, pub version: u16, pub fields: &'static [FieldDesc] }
pub struct FieldDesc {
    pub name: &'static str, pub unit: Unit,
    pub min: i64, pub max: i64, pub default: i64,           // 原始定点值
    pub per_group: bool,                                     // 是否允许按 group 覆盖
    pub structural: bool,                                    // 改值会改变 Model 已存样本的解释(见"参数变化时的 group 重置")
    pub manual_only: bool,                                   // 只允许人工设定(hft-cfg、人工覆盖项);pipeline 插件不得声明拥有它
    pub relax: Relax,                                        // 哪个方向的改动是放宽
    pub minor_change: MinorChange,                           // 门槛 ⑦ 的小改判定:多大的改动不使模拟账户长跑重新计时
    pub since: u16,                                          // 加入该字段的 schema 版本;字段只追加,since 非递减
    pub doc: &'static str,
}
/// Up = 值变大是放宽(上限类);Down = 值变小是放宽(阈值类);None = 无单调方向。
/// 方向必须在整个取值范围内单调:有"0 = 关闭/不限"等特殊值使方向不单调的字段标 None。
pub enum Relax { Up, Down, None }
/// 门槛 ⑦ 的小改判定:小改 = 不使 ⑦ 重新计时,大改 = 重新计时(计时、证据与 group 增删、open_allowed 的口径归 testing ⑦)。
/// "旧值"= ⑦ 本次计时起点生效的参数版本(基准)中的值,不是上一版的值:多次修改按与基准的累计差判定,分步改不能绕过。
/// 比较单位:默认值与默认值比;两版都有的 group(按 GroupId 对齐,gix 变化不影响)比生效值(有覆盖取覆盖值,否则取默认值)。
pub enum MinorChange {
    None,                    // 任何改动都是大改(缺省)
    RelPct(u16),             // |新 − 旧| ≤ |旧| × n / 100 为小改,否则大改;旧值为 0 时任何改动都是大改
    Abs(i64),                // |新 − 旧| ≤ v(原始定点值)为小改,否则大改
    Any,                     // 任何改动都是小改(名单类、插件每轮更新的打分类)
}
pub enum Unit {
    Rate,                    // 1e-9 无量纲比率:1 bp = 100_000,1% = 10_000_000;百分位、权重、指数、ppm 类比例也用它
    Notional,                // USDT 1e-6
    Qty, Px,                 // 按该 group 资产的指数;只能 per_group = true
    DurationMs, Count, Bool,
    Enum(&'static [&'static str]),                           // 值 = 下标
}

pub enum ParamError {
    SchemaMismatch { want: u32, got: u32 }, SchemaTooNew { have: u16, got: u16 }, BodyLen,
    OverrideOrder { ix: u32 }, Padding,                      // SPB1 body 容器错误(见"SPB1 body 布局")
    Misaligned,                                              // body_view:body 不是 8 B 对齐(decode_body 先复制,不返回它)
    OutOfRange { field_ix: u16, group_ix: u16 }, BadEnum { field_ix: u16 }, NotPerGroup { field_ix: u16 },
    GroupIxOutOfRange { group_ix: u16 }, TooManyGroups,
    GroupShape { gix: u16, code: u16 },                      // 腿数/市场类型/资产不符合本策略要求
    Invalid { field_ix: u16, code: u16 },                    // 策略自定义交叉校验
    BadText { field_ix: u16 },                               // parse_value:文本不能按该字段单位解析(含小数位超过该单位的精度、Qty/Px 缺指数)
    UnknownOp { op: u8 },                                    // decode_cmd:op 不在该策略的 cmds 中(cmds 为空时任何 op)
}
// group_ix 为 0xFFFF(params::GROUP_IX_NONE):错误发生在默认值或命令字段上(不针对 group)。SchemaMismatch:装配处 decode_params 收到集合外的 kind_id
// (want = 0,got = kind_id);gateway 在调用前已按 kind 与 schema 版本拒绝,这是防御路径。
```

- **body 的字节布局**归本 crate 的 `params` 模块(见下节);[hft-event](../hft-event/README.md) 的 `StrategyParams`(SPB1)只定义容器(`StrategyParamsHdr` + `GroupSpec`)并把 body 当不透明字节。`group_ix` 是同一 blob 中 `GroupSpec` 的下标,与 `GroupTable` 的 gix 相同。
- **字段属性的使用方**:`structural` 见上;`manual_only` 与 `relax` 由 [pipeline 框架](../../python/pipeline/README.md)读取——合并参数时人工覆盖项优先于插件输出、插件只能输出 `manual_only = false` 且 `structural = false` 的字段、必需输入失败的一轮只下发不放宽(按 `relax`;`None` 的字段按"可能放宽"处理)的改动;ops_bot 的覆盖项预览按 `relax` 标出"放宽"。合并顺序与预检的规则归 pipeline;人工 `SetParams` 的命令类别归 [control-plane](../../docs/subsystems/control-plane.md)。`minor_change` 由门槛 ⑦ 的计时判定读取(`params::minor_change_check`,工具通用;规则与 `GroupSpec` 增删、`open_allowed` 变化的口径归 [testing ⑦](../../docs/testing.md#8-上线门槛)),策略 README 的参数表写出每个字段的取值,不另写白名单。

#### 参数编解码器(`params` 模块,唯一实现)

"schema 序编码"的**编码与解码只有本模块一份实现**(纯函数、冷路径、可分配):`define_params!` 生成的 `decode`、ctl-gateway、`hft-cfg`(TOML → blob)、`hft-py`(pipeline 经 PyO3 调用)都调用它,不存在第二个编码器。

```rust
pub mod params {
    /// 与具体策略类型无关的原始值表。
    pub struct RawParams { pub defaults: Vec<i64>, pub overrides: Vec<ParamOverride> }
    /// 编解码器读取 schema 的接口:编译进二进制的 &'static ParamSchemaDesc 与从 JSON 还原的 OwnedSchema 都实现它。
    pub trait SchemaView { fn id(&self) -> u32; fn version(&self) -> u16; fn n_fields(&self) -> u16;
                           fn field(&self, ix: u16) -> Option<FieldRef<'_>>; }   // ix ≥ n_fields → None
    /// FieldDesc 的借用形式:名字为 &str,带字段下标 ix(BadText/BadEnum 的 field_ix),单位拆成 UnitKind + 枚举名表。
    pub struct FieldRef<'a> { pub ix: u16, pub name: &'a str, pub unit: UnitKind, pub enum_names: EnumNames<'a>, /* 其余同 FieldDesc */ }
    /// 命令载荷描述的读取接口:CmdDesc 与 OwnedCmd 都实现它(encode_cmd/decode_cmd 对它泛型)。
    pub trait CmdSchema { fn op(&self) -> u8; fn n_fields(&self) -> u16; fn field(&self, ix: u16) -> Option<FieldRef<'_>>; }
    pub struct OwnedSchema { /* 从规范 schema JSON 还原(schema-json 特性) */ }
    impl OwnedSchema {
        /// 任何不合规(JSON 语法、缺键或类型不符、未知取值写法、违反"schema 编译期校验"的各条)→ Err(SchemaMismatch{want: 0, got: 0});
        /// 不读 notes、report、sections 与 display。
        pub fn from_json(s: &str) -> Result<OwnedSchema, ParamError>;
        pub fn cmds(&self) -> &[OwnedCmd];                   // 命令描述符(实现 CmdSchema)
        pub fn schema_digest(&self, ver: u16) -> u64;        // = 导出它的策略的 params::schema_digest
    }
    // 下列函数的 s: &ParamSchemaDesc 实际为 s: &impl SchemaView

    pub fn n_fields_at(s: &ParamSchemaDesc, ver: u16) -> Option<u16>;        // since ≤ ver 的字段数(前缀);ver > s.version → None
    pub fn field_ix(s: &ParamSchemaDesc, name: &str) -> Option<u16>;
    /// 文本 → 原始值:"0.5%"、"50bp"、"1500"、"5m"、"true"、枚举名;Qty/Px 需要该 group 资产的指数。不查范围;枚举名不存在 → BadEnum。
    /// 写法:Rate 为 "<x>%"(×10^7)、"<x>bp"(×10^5)或纯小数(×10^9);Notional 为 USDT 小数;Qty/Px 按资产指数的小数;
    /// DurationMs 为毫秒整数(可带 "ms")或 "<x>s"/"m"/"h"/"d";Count 为整数;Bool 为 true/false/1/0。十进制解析经 hft-types parse_exact(不经浮点)。
    pub fn parse_value(f: FieldRef<'_>, text: &str, exp: Option<AssetExp>) -> Result<i64, ParamError>;
    /// 显示与 schema JSON 用:Rate 写 "<x>%";DurationMs 取能整除的最大单位(d/h/m/s,否则毫秒整数);Qty/Px 缺指数时写原始整数。
    /// 对 [min, max] 内任意值 parse_value(f, &format_value(f, x, exp), exp) == Ok(x)。
    pub fn format_value(f: FieldRef<'_>, raw: i64, exp: Option<AssetExp>) -> String;
    /// 编码:按当前 s.version 写全部字段(raw.defaults 的个数 ≠ 字段数 → BodyLen);覆盖表按 (group_ix, field_ix) 排序,
    /// 完全相同的项合并,同一 (group_ix, field_ix) 值不同 → OverrideOrder{ix = 输入中后出现的那一项的下标};检查范围、per_group、枚举。
    pub fn encode_body(s: &ParamSchemaDesc, n_groups: u16, raw: &RawParams) -> Result<Vec<u8>, ParamError>;
    /// 解码:body_view 做容器校验 → n_fields 必须等于 n_fields_at(schema_ver)(不等 → BodyLen;
    /// 更新的版本 → SchemaTooNew)→ 缺失的新字段取 default → 检查范围、per_group、枚举。
    pub fn decode_body(s: &ParamSchemaDesc, schema_ver: u16, n_groups: u16, body: &[u8]) -> Result<RawParams, ParamError>;
    /// 规范 schema JSON 与摘要(schema-json 特性)。摘要 = SHA-256(按 ver 截取字段前缀后、只含 name/kind_id/schema_id/
    /// schema_version/fields 五个键、且 fields 元素只含编码相关键 ix/name/unit/enum/min/max/default/per_group/since 的规范 JSON)
    /// 前 8 字节(按小端解释为 u64),写入 StrategyParamsHdr.schema_digest;ctl-gateway 比对不一致 → InvalidArgs。
    /// doc、字段属性(structural/manual_only/relax/minor_change)、non_prod_only 与 notes/report/cmds 不进摘要:改它们不影响已编码的参数。
    /// layout = 该策略的 Model::LAYOUT(无 Model 为 ModelLayoutDesc::EMPTY),给出 sections;StrategySet::schema_json 按此调用。
    pub fn schema_json(d: &StrategyDesc, layout: &ModelLayoutDesc, ver: u16) -> String;
    pub fn schema_digest(d: &StrategyDesc, ver: u16) -> u64;
    /// 门槛 ⑦ 的小改判定:按各字段 minor_change(语义见 MinorChange)比较基准(⑦ 计时起点生效的版本)与当前参数
    /// (默认值;两版都有的 group 按 GroupId 对齐比生效值,只在一版里出现的 group 不比较)。
    /// 全部为小改 → Ok;否则返回第一个大改(按 field_ix、再按 GroupId 升序)。纯函数、冷路径。
    pub fn minor_change_check(s: &ParamSchemaDesc, base: (&RawParams, &[GroupId]), cur: (&RawParams, &[GroupId]))
        -> Result<(), MajorChange>;
    pub struct MajorChange { pub field_ix: u16, pub group: GroupId /* GROUP_NONE = 默认值 */ }
    /// StrategyCmd 载荷编解码(见"策略描述符");与参数共用文本解析与范围检查。
    pub fn encode_cmd(d: &impl CmdSchema, vals: &[i64]) -> Result<Vec<u8>, ParamError>;
    /// 按序检查:长度 < 8、ver ≠ 1、n > 31、长度 ≠ 8 + 8n → BodyLen;_pad 非 0 → Padding;未知 op → UnknownOp;
    /// n ≠ 该 op 的字段数 → BodyLen;值越界 → OutOfRange/BadEnum。
    pub fn decode_cmd<C: CmdSchema>(cmds: &[C], payload: &[u8]) -> Result<(u8 /* op */, Vec<i64>), ParamError>;
}
```

#### SPB1 body 布局(草案,实现时以代码与编译期断言为准)

body 紧跟在 SPB1 的 `GroupSpec` 表之后(容器布局归 [hft-event StrategyParams](../hft-event/README.md));小端、8 B 对齐,长度 = `StrategyParamsHdr.body_len`。

```
ParamsBodyHdr (8 B) | defaults: [i64; n_fields] | overrides: [ParamOverride; n_overrides] (16 B each)
```

```rust
pub mod params {
    #[repr(C)]
    pub struct ParamsBodyHdr {           // 8 B
        pub n_fields: u16,               // 0 默认值个数;decode 要求 = n_fields_at(schema_ver)
        pub _pad: u16,                   // 2 必须为 0
        pub n_overrides: u32,            // 4 覆盖表条数
    }
    const _: () = assert!(core::mem::size_of::<ParamsBodyHdr>() == 8);

    #[repr(C)]
    pub struct ParamOverride {           // 16 B;按 group 的覆盖值
        pub group_ix: u16,               //  0 = 同一 blob 中 GroupSpec 的下标(= gix)
        pub field_ix: u16,               //  2 schema 字段下标;该字段必须 per_group = true
        pub _pad: u32,                   //  4 必须为 0
        pub value: i64,                  //  8 原始定点值
    }
    const _: () = assert!(core::mem::size_of::<ParamOverride>() == 16);

    pub struct BodyView<'a> { pub hdr: &'a ParamsBodyHdr, pub defaults: &'a [i64], pub overrides: &'a [ParamOverride] }
    /// 容器级校验(不读 schema):① body.len() = 8 + 8 × n_fields + 16 × n_overrides(BodyLen);
    /// ② 填充为 0(Padding);③ 覆盖表按 (group_ix, field_ix) 严格升序,即已排序且无重复(OverrideOrder{ix});
    /// ④ group_ix < n_groups(GroupIxOutOfRange)。零拷贝(zerocopy),不分配;body 须 8 B 对齐(SPB1 blob 对齐时成立),否则 Misaligned。
    pub fn body_view(body: &[u8], n_groups: u16) -> Result<BodyView<'_>, ParamError>;
}
```

- 字段值的范围、`per_group`、枚举与版本前缀由 `decode_body` 按 schema 检查;`body_view` 只管容器。ctl-gateway 的 SetParams 校验先用 hft-event `parse_spb1` 校验容器与 `GroupSpec`,再经装配处的 `decode_params`(内部即 `body_view` + `decode_body`)解码 body(流程归 [hft-ctl](../hft-ctl/README.md))。
- body 编码将来不兼容地变化时,由本模块定义新编码,hft-event 追加 `ConfigBlob` 的 `encoding` 号区分。

#### schema JSON 与 define_params!

- **规范 schema JSON**(本模块生成,`GetSchema`、hft-cfg、ops_bot、pipeline、watchtower、journal-archiver 只认它):`{"name", "kind_id", "non_prod_only", "schema_id", "schema_version", "fields": [{"ix", "name", "unit", "enum"?, "min", "max", "default", "per_group", "structural", "manual_only", "relax", "minor_change", "since", "doc"}], "notes": [{"code", "name", "level", "v", "doc"}], "report": [{"name", "unit", "agg", "scope", "note", "v_ix", "doc"}], "cmds": [{"op", "name", "reduce_only", "fields": [同 fields 的元素形状], "doc"}], "sections": [{"tag", "name"}]}`(`sections` 是 `Model::LAYOUT` 各节的 `tag` 与 `name`,无 Model 为空数组;gen-schema 据它核对检查点节 `tag` 只追加);键按此顺序、无多余空白、数值为十进制整数(原始定点);`minor_change` 写成字符串 `"none"`、`"any"`、`"rel_pct:<n>"` 或 `"abs:<原始值>"`;单位、`relax`、Note `level`、报表 `agg`/`scope` 写成小写字符串(`"rate"`、`"notional"`、`"qty"`、`"px"`、`"duration_ms"`、`"count"`、`"bool"`、`"enum"`;`"up"`/`"down"`/`"none"`;`"p1"`/`"p2"`/`"info"`;`"count"`/`"sum"`/`"last"`/`"max"`/`"p99"`;`"instance"`/`"group"`);参数字段与命令字段的每个元素在 `doc` 之后另附 `"display":{"min","max","default"}` 子对象,值为 `format_value` 的输出(`Qty`/`Px` 无资产指数,写原始整数)。
- **`decode`/`validate` 的执行位置与预算**:`SetParams` 时它们在 ctl-gateway 的 warm 线程上运行(不在 sequencer 热线程);必须有界、无 I/O,预算 p99 ≤ 2 ms(按 group 数线性,`params/validate` 基准);超预算视为参数无效(`InvalidArgs`),旧参数继续生效。**禁止 panic**:发布剖面 `panic = "abort"`,ctl-gateway 线程上的 panic 使整个 hft-core 进程退出(退出码见 [apps/hft-core](../../apps/hft-core/README.md));越界、溢出、除零一律用 checked 运算返回 `ParamError`(属性测试覆盖,见 [TESTS.md](TESTS.md) SDK-102)。hft-cfg 离线只做 schema 级校验,跨字段校验只在发送前的 PREVIEW 中由 gateway 执行。
- **`define_params!`**:SDK 提供的声明宏,一处声明生成字段结构体、`ParamSchemaDesc` 常量、`ParamSet::decode`(= `params::decode_body` + 类型化 + 按 gix 展开)与按 group 解析后的访问表;交叉校验由策略写一个 `fn check(&self, groups, rd) -> Result<(), ParamError>` 挂入 `validate`。

```rust
hft_strategy_sdk::define_params! {
    pub struct FooParams(schema_id = 7 /* = kind_id */, version = 2) {
        /// 开仓阈值
        open_th:     Rate     [min = 0, max = rate::pct(10), default = rate::bp(50), per_group, relax = Down, minor = RelPct(10)],
        cooldown_ms: DurationMs [min = 0, max = 600_000, default = 20_000, relax = Down],
        pct_bin:     Rate     [min = rate::bp(1) / 10, max = rate::bp(10), default = rate::bp(1), structural],
        mode:        Enum(["a", "b"]) [default = 0, since = 2, manual_only],   // since 缺省为 1;属性缺省为 false / None(minor 缺省 None)
    }
}
// 生成:FooParams { d: FooFields, g: Box<[FooFields]> };p.at(gix).open_th = 覆盖后的值(O(1),热路径可读)。
```

- **schema 编译期校验**(`const` 断言):字段名唯一、`min ≤ default ≤ max`、`Qty`/`Px` 必须 `per_group`、`Enum` 默认下标合法、`since` 非递减且最大值 = `version`、字段数 ≤ 256;`structural` 字段的 `minor_change` 必须为 `None`;`Bool`/`Enum` 字段只能取 `None` 或 `Any`;`RelPct(n)` 要求 1 ≤ n ≤ 100。
- **schema 导出**(只有一种方式):`hft-core schemas --out <dir>` 经 [registry](../../strategies/registry/README.md) 的 `export_schemas()` 为每个策略写 `<kind_id>-<name>.schema.json`(内容 = `params::schema_json` 的输出),另写 `index.json`(格式归 registry);发布制品附带该目录。运行中用 `GetSchema{strategy_kind}` 命令([control-plane](../../docs/subsystems/control-plane.md))取同一内容。`hft-cfg`、ops_bot、pipeline(经 hft-py)、watchtower、journal-archiver 都读这份 JSON,按 schema 与描述符通用处理参数、Note、报表与策略命令,新增策略不改这些工具。
- `SetParams` 的头与 `GroupSpec`(引擎理解的部分:订阅、账户绑定、LegTracker、ledger 归属、两腿不同交易所)由 ctl-gateway 通用解析与校验;body 经装配处的 `StrategySet::decode_params` 解码、`StrategySet::validate_params` 校验;账户未绑定到该实例 → `InvalidArgs{AccountBound}`;需要 sequencer 状态的校验(`GroupId` 全局唯一、同一 (account, inst) 的引用规则、有仓位的 group 不得移除)归 [hft-sequencer](../hft-sequencer/README.md)。

### 策略描述符:Note、报表、策略命令

策略的 Note 码表、日报的策略专项字段与人工命令的载荷都以 `StrategyDesc` 中的静态描述符给出,随规范 schema JSON 导出。引擎与运维工具只按描述符通用处理,**不认识任何具体策略**:watchtower 与 journal-archiver 按 `notes`/`report` 汇总 `Note{source = Strategy}`,生成实盘与模拟账户的日报;ops_bot 与 hft-ctl 按 `cmds` 编码 `StrategyCmd`。

```rust
pub struct NoteDesc {
    pub code: u32,                         // 本策略(kind)内唯一的 Note 码,1..=0xFF_FFFF(0 保留);Note 记录中的完整码按 hft-event 的 source 规则组合
    pub name: &'static str,                // 如 "CashShort"
    pub level: NoteLevel,                  // 策略只用 P1 | P2 | Info;码表 P0 = 1、P1 = 2、P2 = 3、Info = 4 归 hft-event Note.level,本 crate 重导出
    pub v: &'static [&'static str],        // v[0..8] 各自的含义与单位(≤ 8 项;未用的不列)
    pub doc: &'static str,
}
pub struct ReportFieldDesc {
    pub name: &'static str,                // 日报列名,如 "gate_cost_blocked"
    pub unit: Unit,                        // 与参数同一单位体系(Count、Rate、Notional、DurationMs …)
    pub agg: ReportAgg,                    // Count | Sum | Last | Max | P99
    pub scope: ReportScope,                // Instance | Group(按 Note.group 分行)
    pub note: u32,                         // 来源 Note 码(必须在 notes 中)
    pub v_ix: u8,                          // 取该 Note 的 v[v_ix] 作样本(按 unit 解释为 i64);agg = Count 时忽略
    pub doc: &'static str,
}
pub struct CmdDesc {
    pub op: u8,                            // 本策略内唯一,1..=255
    pub name: &'static str,                // ops_bot / hft-ctl 的子命令名,如 "open"
    pub reduce_only: bool,                 // 工具发送该命令时 StrategyCmd.reduce_only 的取值(true = 只用于减仓)
    pub fields: &'static [FieldDesc],      // 载荷字段(与参数同一描述;per_group、structural 必须为 false,minor_change 必须为 None)
    pub doc: &'static str,
}
/// StrategyCmd 载荷 ≤ 256 B:{ver: u8 = 1, op: u8, n: u8, _pad: [u8; 5]} + vals: [i64; n](n ≤ 31,按 fields 顺序)。
pub struct CmdView<'a> { /* 借用载荷 */ }
pub enum CmdError { TooShort /* < 8 B */, BadVersion /* ver ≠ 1 */, TooMany /* n > 31 */, LenMismatch /* 长度 ≠ 8 + 8n */ }
impl<'a> CmdView<'a> {
    pub fn parse(payload: &'a [u8]) -> Result<CmdView<'a>, CmdError>;   // 快速部分调用:无分配、O(1)
    pub fn op(&self) -> u8;
    pub fn val(&self, ix: u8) -> Option<i64>;                           // 原始定点值
}
```

- **编译期校验**(`const` 断言,`define_params!` 同一套机制:策略在 `DESC` 旁写 `const _: () = hft_strategy_sdk::check::validate_desc(&DESC);`;`strategy_set!` 对每个登记的策略再断言一次):`notes` 的 `code` 与 `name` 唯一、`1 ≤ code ≤ 0xFF_FFFF`(`Note.code` 的低 24 位,高 8 位是 source;码 0 保留不用,与 [hft-event Note 码](../hft-event/README.md)的保留规则相同)、`v.len() ≤ 8`、**`level` 不得为 `P0`**(策略 Note 最高 P1;P0 保留给引擎条件——裸腿、Broken、强平类外来成交等由引擎自己的 Note 报出);`report` 的 `name` 唯一、`note` 在 `notes` 中、`v_ix < 8`;`cmds` 的 `op` 与 `name` 唯一、每个 `fields` 满足参数 schema 的同样规则且 ≤ 31 项、`per_group`/`structural` 为 false 且 `minor_change = None`。
- **Note 纪律**:`Ctx::note` 的 `code` 必须在 `DESC.notes` 中、`level` 与描述符一致(`testing::CtxHarness` 在每次记录时断言,所以策略单测会暴露未登记的码);`group` 填该 Note 所属的 GroupId(实例级为 `GROUP_NONE`),`ReportScope::Group` 的报表按它分行;码只追加、永不复用,含义不改。
- **报表**:日报的通用项(PnL 分解、成交率、两腿间隔、裸敞口、保证金占用)由 journal-archiver 从引擎事件算出;`report` 只描述策略专项项。每个字段按 UTC 日、按 `scope` 聚合;`P99` 在日内样本上取;模拟账户与实盘账户分开出报(数据来自各自的 journal)。
- **策略命令**:人工命令 `StrategyCmd{instance, group, payload, reduce_only}`([control-plane](../../docs/subsystems/control-plane.md))由 ctl-gateway 在 warm 线程用 `params::decode_cmd` 按该实例的 `cmds` 校验(未知 `op`、字段越界 → `InvalidArgs`;`cmds` 为空 → `InvalidArgs`);通过后作为 Input 记 journal,经 `EventRef::Manual` 投递给该实例的快速部分(`group = GROUP_NONE` 为实例级)。载荷含义(开平、数量、阈值、模式)只在策略 README 定义,引擎不解释。
- **`reduce_only` 契约**:`reduce_only` 由发送方声明(ops_bot/hft-ctl 取该命令 `CmdDesc.reduce_only`),带它的命令按中性类处理(DRAINING、CloseOnly、启动闸门未放行时也可用),不带的按升级类(二次确认),类别规则归 control-plane。策略处理 `reduce_only = true` 的 `Manual` 时产生的**每个**意图都必须是 Reduce/Flatten 类(订单意图的 `class`、目标意图只减不增):引擎给该次 `on_event` 产生的意图带 `MANUAL_RO` 标记,其中的 Open 类订单意图一律被拒(`CLOSE_ONLY`,判定归 [hft-sequencer](../hft-sequencer/README.md)/[hft-risk](../hft-risk/README.md));会增加任一腿 |仓位| 的目标意图整条被拒(`TargetRejected(ReduceOnlyCmd)`,不部分接受;判定归 [hft-exec 目标的接受与替换](../hft-exec/README.md#目标的接受与替换));结果都经 `Ctx::outcome` 返回;由该命令派生、在之后事件中继续执行的任务(分笔、定时器)也必须只减仓——这是策略的义务,由策略测试覆盖。`reduce_only = false` 的命令产生的意图照常经模式门与检查链。

### Views 与 ModelViews

```rust
/// 只读视图(静态分派)。引擎由 hft-sequencer 的适配器实现;测试用 testing::MockViews。
/// 所有方法 O(1)(book 的逐档读取 O(k);bbo 的成员聚合 ≤ MAX_BBO_CONN_SLOTS 个槽),无分配;返回值反映引擎组件已应用本输入事件、
/// 本次 on_event 的意图尚未处理时的状态(EventRef::Bbo 的 b 与此时的 v.bbo(inst) 相同;意图在 on_event 返回后才处理)。
pub trait Views {
    type Book<'a>: BookRead where Self: 'a;
    type Portfolio<'a>: PortfolioRead where Self: 'a;      // 权益视图(下称 PortfolioView);理由同 Book
    fn now(&self) -> Now;
    fn bbo(&self, i: InstId) -> BboView;                     // 按值:最近报价 + 按成员聚合的 BBO 连接健康(见"报价健康")
    fn book(&self, i: InstId) -> Self::Book<'_>;             // 未订阅 L2 的 inst:status = Uninit
    fn funding(&self, i: InstId) -> &FundingView;            // 实时费率、下次结算时刻、实际结算周期
    fn mark(&self, i: InstId) -> Px;
    fn portfolio(&self, a: AccountId) -> Self::Portfolio<'_>;
    fn group_pos(&self, a: AccountId, g: GroupId) -> &GroupPosView;
    fn legs(&self, a: AccountId, g: GroupId) -> &LegView;    // 各腿净量、在途、残余、naked
    fn target(&self, a: AccountId, g: GroupId) -> Option<&TargetView>;
    fn order(&self, h: OrderRef) -> Option<OrderView>;       // gen 不符 → None
    fn inst(&self, i: InstId) -> &InstHot;                    // refdata 热行(tick、lot、最小名义、资产)
    fn caps(&self, v: VenueId) -> &VenueCaps;                // 交易所差异的唯一入口;VenueId 按产品线,市场类型取自 inst/venue 元数据
    fn fee(&self, a: AccountId, i: InstId) -> FeeView;
    fn mode(&self, a: AccountId, g: GroupId) -> RunMode;     // 有效模式
    fn risk(&self, a: AccountId) -> RiskStateView;
    /// 按 inst 的风控限价读数:该 inst 所属价格带类别的取值(写者 hft-risk)。执行层的切片与引擎单、
    /// 策略自己的越价参数都按它截取,不取账户内最小值。
    fn risk_for(&self, a: AccountId, i: InstId) -> InstRiskView;
    fn venue_ready(&self, a: AccountId, v: VenueId) -> bool;   // 该 (venue, account) 无 Unknown 槽且下单连接 Up(值取 hft-oms venue_ready)
    /// 该账户在永续 inst 上的实际杠杆,百分之一倍(300 = 3x):来自该 (account, venue) 最近的 AccountConfigReport
    /// 的 LeverageItem(Rate 1e-9 → ÷ 10^7,向下取整;与风控杠杆上限读同一来源、同一取整);现货 inst、条目读不到(0)或尚无报告 → None。
    fn leverage(&self, a: AccountId, i: InstId) -> Option<u32>;
    /// 引擎口径的报价健康:bbo(i).quote_live(now.tsc, risk(a).conn_liveness_ms × now.tsc_per_ms)。
    /// 提供的默认方法(实现者不覆盖);执行层、策略要与引擎同一阈值时用它。
    fn quote_ok(&self, a: AccountId, i: InstId) -> bool;
}

/// 模型侧视图:advisor 从 journal 重建的同一组状态,截至 Model 当前处理的那条记录(now() = 该记录)。
/// target()/order() 在模型侧返回 None(模型不参与执行);其余与 Views 语义相同。
pub trait ModelViews: Views {}

#[derive(Clone, Copy)]
pub struct Now {
    pub tsc: Tsc,              // 当前事件记录头 tsc(不是系统时钟);只在本次启动内有意义
    pub engine_ns: i64,        // 引擎时间:跨重启连续、停机不计(见下"时间轴");持久状态里的时刻一律用它
    pub utc_ns: i64,           // 用当前段 journal 记录的时钟校准对换算(确定);clock_ok = false 时不可用于结算时刻判断
    pub seq: u64,
    pub boot_seq: u64,         // 本次进程启动的 EpochStart 记录的 seq;Advice 跨启动过期的唯一判据(basis_seq < boot_seq)
    pub tsc_per_ms: u64,       // 来自 journal 记录的 tsc_hz;毫秒参数 × 它 = tsc 间隔(只用于本次启动内的 TSC 比较)
    pub clock_ok: bool,
    clk: EngineClockAnchor,    // 本次启动的 EngineClock(base_engine_ns、记录 tsc、tsc_hz_milli)与当前段的 UTC 校准对;ens_of/tsc_of/utc_of 用
}
pub struct EngineClockAnchor { pub base_engine_ns: i64, pub base_tsc: Tsc, pub tsc_hz_milli: u64, pub utc_tsc: Tsc, pub utc_ns: i64 }
impl Now {
    /// sequencer/advisor 按当前记录构造:engine_ns = clk.ens_of(tsc)、utc_ns = clk.utc_of(tsc)、tsc_per_ms = tsc_hz_milli / 10^6。
    pub const fn new(tsc: Tsc, seq: u64, boot_seq: u64, clock_ok: bool, clk: EngineClockAnchor) -> Now;
    /// 本次启动内的 TSC 时刻 → engine_ns:与 event-model 时间戳语义同一公式(以 clk 为基准),结果与该时刻记录的 engine_ns 逐位相同。
    pub fn ens_of(&self, t: Tsc) -> i64;
    /// engine_ns → 本次启动的 TSC(登记定时器、与视图里的 *_tsc 比较用;取 engine_ns ≥ e 的最小 TSC);早于本次启动的时刻饱和到 0(只能表示"已过去")。
    pub fn tsc_of(&self, e: i64) -> Tsc;
    /// 本次启动内的 TSC 时刻 → UTC ns:与 utc_ns 同一校准对(确定);clock_ok = false 时同样不可用于时刻判断。
    pub fn utc_of(&self, t: Tsc) -> i64;
}
pub mod time { pub const fn ms(ms: u32) -> i64; }   // 毫秒参数 → engine_ns 间隔(× 1_000_000)

pub trait BookRead {
    fn status(&self) -> BookStatus;                          // Uninit | Live | Stale | Resyncing
    fn upd_tsc(&self) -> Tsc;
    fn best(&self, s: Side) -> Option<Level>;                // Level 在 hft-event
    fn levels(&self, s: Side, out: &mut [Level]) -> usize;   // 由内向外拷出至多 out.len() 档,O(k)
}
```

`Views` 用泛型关联类型表示簿视图(`V::Book<'_>`,下文称 `BookView`),因为 SDK 只依赖 L0、不能引用 hft-book 的类型,又不允许 `dyn`。

#### 时间轴

| 时间轴 | 字段 | 用途 | 跨重启 |
|---|---|---|---|
| TSC | `Now.tsc`、视图里以 `_tsc` 结尾的字段 | 本次启动内的行情年龄、连接健康、Advice 年龄、定时器登记 | 不可比:重启或换机后 TSC 重新计数 |
| 引擎时间 `engine_ns` | `Now.engine_ns`、视图里以 `_ens` 结尾的字段 | 冷却、确认窗口、重入封锁、截止时刻、持仓时长等**跨事件保存**的时刻 | 连续:由每次启动一条的 `EngineClock` Input 与记录头 tsc 算出,停机期间不计;重启、换机、回放结果相同(公式的唯一定义在 [event-model 时间戳语义](../../docs/subsystems/event-model.md#时间戳语义),投影进 `Now` 归 [hft-sequencer](../hft-sequencer/README.md)) |
| UTC | `Now.utc_ns`、`Now::utc_of`、事件里的 `*_ns`(交易所时刻、`next_settle_ns`、`Notice` 的时刻) | 资金费结算、下架与维护窗口、Model 的样本时刻、按接收时刻的信号(`Now::utc_of(rx_tsc)`) | 按钟表时间;`clock_ok = false` 时不可用于时刻判断 |

- **`hw_rx_ns` 不在这三条时间轴上**:它是网卡硬件接收戳(PHC 域),不可用时退化为软件戳(CLOCK_REALTIME 域),都没有为 0(口径唯一定义在 [event-model 时间戳语义](../../docs/subsystems/event-model.md#时间戳语义));来源由 `BboView.rx_ts_src` 区分。它只用于延迟测量与同源比较,策略不把它当 UTC、不与 TSC 或 UTC 混算;按接收时刻的信号一律用 `rx_tsc`(本次启动内)或 `Now::utc_of(rx_tsc)`(确定,跨 venue 同一时钟)。

- **持久状态不存 TSC**:策略快照(`Strategy::snapshot`)、Model 检查点与 hft-strat-lib 构件里的时刻一律存 `engine_ns`(由交易所或命令按 UTC 给出的时刻存 UTC ns),TSC 只在单次启动内使用。视图里的 `_tsc` 字段要跨事件保存时先用 `Now::ens_of`(或 `utc_of`)换算;只按 engine_ns 记账的写者(ledger、risk)对可能早于本次启动的时刻直接给 `_ens` 字段。
- 停机不计意味着:冷却、重入封锁、持仓时长在停机期间不前进(偏保守);按 UTC 定义的窗口(结算、下架、维护)不受影响。

### 视图类型

全部是 SDK 定义的只读投影;数据的唯一写者在注释所列组件。金额为 `Notional`(USDT 1e-6),数量为 `Qty`(资产指数),比率为 `Rate`(1e-9)。

```rust
#[derive(Clone, Copy)]
pub struct BboView {                     // 按值返回(Views::bbo、EventRef::Bbo);写者 sequencer
    // 报价:该 inst 最近一条被应用的 Bbo(簿顶来源 hft-book Top)
    pub bid_px: Px, pub bid_qty: Qty, pub ask_px: Px, pub ask_qty: Qty,
    pub update_id: u64, pub ex_ts_ns: i64,
    pub rx_tsc: Tsc,                     // 该 Bbo 的 MarketHdr.rx_tsc(= hft-book Top.rx_tsc;重复与更旧的 Bbo 不改它)。关键帧(含本次启动的
                                         // 恢复快照)之后、本次运行收到该 inst 的首条 Bbo 之前为 0(快照不存 TSC;规则 rx_from_seq 归
                                         // hft-sequencer 只读视图):策略把 0 当作未知,不据此算报价年龄或接收时刻
    pub hw_rx_ns: i64,                   // 同一条 Bbo 的 MarketHdr.hw_rx_ns(PHC 域,不是 UTC;口径见"时间轴")
    pub rx_ts_src: RxTsSrc,              // hw_rx_ns 的来源(由该 Bbo 记录头 RecordFlags 的 HW_TS/SW_TS 位投影)
    pub seq: u64,                        // 该 Bbo 记录的 seq
    // 连接健康:按该 inst 的 BBO 成员连接在调用时聚合(聚合规则的唯一定义在 hft-sequencer"连接活性",见下"报价健康")
    pub conn_live_tsc: Tsc,              // 成员连接最近一次活性证据的 rx_tsc(本次运行的 TSC);0 = 无
    pub conn_up: bool,                   // 有成员连接 up
    pub resync: bool,                    // 该 inst 的报价在重同步或没有健康成员;为真时 quote_live 为假
}
#[repr(u8)] pub enum RxTsSrc { None = 0, Hw = 1, Sw = 2 }   // 0 = 无接收戳;Hw = 网卡 PHC;Sw = 内核软件戳(REALTIME)
impl BboView {
    /// 报价健康——全系统唯一的判定:conn_up && !resync && now − conn_live_tsc ≤ max_silence(连接 Live、非重同步、静默不超限)。
    /// 报价多久没变不影响结果(安静 ≠ 过期):健康时报价就是该 inst 的最新值。
    pub fn quote_live(&self, now: Tsc, max_silence: u64 /* tsc 间隔 */) -> bool;
}
pub struct FundingView {                 // 写者 sequencer(Funding/MarkPrice 事件)
    pub rate: Rate,                      // 当前(预测)费率,每期口径
    pub next_settle_ns: i64,             // 下次结算 UTC
    pub interval_s: u32,                 // 实际结算周期(取自事件,不取 refdata 默认)
    pub prev_interval_s: u32,            // 上一次不同的周期;≠ interval_s 表示周期刚变化;未变化过 = interval_s。
                                         // 随 sequencer 的 MarketLast 进快照,实盘与从快照起步的回放同值(数据来源归 hft-sequencer)
    pub upd_tsc: Tsc,                    // 该 inst 最近一条被应用的 Funding 的 MarketHdr.rx_tsc;可见性同 BboView.rx_tsc
                                         // (早于本次运行最近一个快照的为 0,规则 rx_from_seq 归 hft-sequencer):0 = 未知,
                                         // 策略按"无数据"处理;新鲜度门由策略按参数判断
}
pub struct TradeView { pub px: Px, pub qty: Qty, pub side: Side, pub ex_ts_ns: i64 }
pub struct LiquidationView { pub px: Px, pub qty: Qty, pub side: Side, pub ex_ts_ns: i64 }   // 公开市场强平流(不是本账户的强平)

/// 权益视图(写者 hft-ledger,数据定义见 hft-ledger;下称 PortfolioView)。引擎不计算"可用资金"或杠杆公式,策略自己从这里算。
/// 按 venue 记现金是准确的:同一账户不含同一交易所的两个市场(refdata `SameExchangeMarkets`),各 venue 的钱包互不共用。
/// 是 trait(与 BookRead 同理:SDK 不能引用 hft-ledger 的类型、不用 dyn),Views::portfolio 返回实现者 V::Portfolio<'_>。
pub trait PortfolioRead {
    fn account(&self) -> AccountId;
    fn venues(&self) -> VenueSet;                                      // 该账户涉及的 venue(AccountMeta.venues 位图)
    fn venue(&self, v: VenueId) -> &VenueEquityView;                   // 每 venue 汇总行,ledger 增量维护
    fn cash(&self, v: VenueId, asset: AssetId) -> CashView;            // { free: Qty, locked: Qty }
    fn inventory(&self, v: VenueId, asset: AssetId) -> InventoryView;
    fn inventory_of(&self, v: VenueId, g: GroupId) -> Qty;             // 归属该 group 的现货净量
    fn perp(&self, v: VenueId, i: InstId) -> PerpPosView;
    fn margin(&self, v: VenueId) -> MarginView;
    fn equity_total(&self) -> Notional;                                // Σ venue.equity
}
pub struct VenueSet(pub u64);            // bit v = VenueId v(与 AccountMeta.venues 同编码)
#[repr(C)]
pub struct VenueEquityView {             // 56 B
    pub venue: VenueId, pub _pad0: u8,
    pub flags: u16,                      // RATE_STALE | MARGIN_STALE | BALANCE_UNRECONCILED
    pub _pad1: u32,
    pub equity: Notional,                // 现货按 bid、永续按 mark(估值规则是 ledger 参数)
    pub quote_cash: Notional, pub spot_value: Notional, pub perp_upnl: Notional,
    pub margin_balance: Notional,        // = wallet + Σ未实现
    pub as_of_ens: i64,                  // 数据时刻(引擎时间 engine_ns,跨重启可比)
}
pub struct InventoryView { pub total: Qty, pub assigned: Qty, pub unassigned: Qty, pub baseline: Qty } // baseline = SetBalanceBaseline 登记的非策略资产
pub struct PerpPosView {
    pub qty: Qty /* 有符号 */, pub entry_px: Px, pub upnl: Notional /* 按 mark */,
    pub liq_px: Px,                      // 强平价(来源 MarginReport;0 = 未知)
    pub adl_quantile: u8,                // ADL 档位(来源 MarginReport;0xFF = 未知)
}
pub struct MarginView {
    pub margin_balance: Notional, pub maint_margin: Notional, pub avail_margin: Notional,
    pub margin_ratio: Rate,              // 统一口径:维持保证金 / 保证金余额;各所换算在 venue-*
    pub src_ens: i64, pub stale: bool,   // 来源 MarginReport 的引擎时间;超过 margin_stale_ms 为 stale
}

pub struct GroupPosView {                // 写者 hft-ledger;每 (account, group)
    pub legs: [LegPos; 4], pub n_legs: u8,
    pub flags: u16,                      // ENTRY_ADOPTED:含 AdoptExchange 采纳量,入场价不完全来自成交
    pub held_notional: Notional,
    pub pnl: PnlSummary,                 // price_realized、funding、fees、unrealized(累计)
    pub round_start_ens: i64, pub last_fill_ens: i64,   // engine_ns(可能早于本次启动;ledger 按 engine_ns 持久化)
}
pub struct LegPos { pub qty: Qty /* 净量:扣除基础币手续费后 */, pub entry_px: Px, pub upnl: Notional }

#[repr(C)]
pub struct LegView {                     // 160 B;写者 hft-oms LegTracker(字段语义与在途划分见 hft-oms"LegTracker")
    pub leg: [LegTrack; 4],              // 4 × 32 B
    pub naked_qty: Qty, pub naked_notional: Notional, pub naked_ratio: Rate,   // 含即时在途,不含挂单(inflight_resting)
    pub n_legs: u8, pub open_links: u8,
    pub has_unknown: bool,               // 该 group 有 Unknown 槽
    pub has_pending: bool,               // 该 group 有未终结的 PendingNew/PendingCancel/PendingAmend 槽(执行层"静默"判定之一)
    pub _pad: [u8; 4],
}
#[repr(C)] pub struct LegTrack {                                      // 32 B
    pub net: Qty,
    // 两类在途的划分唯一定义在 hft-oms LegTracker"在途的两类"表(一个非终态槽的剩余量只计入其中一类)
    pub inflight_immediate: Qty,      // 即时在途:IOC/FOK、Ack 前的 GTC/PostOnly、Unknown 槽;裸量与补腿抑制只看它
    pub inflight_resting: Qty,        // 挂单在途:已 Ack 的 GTC/PostOnly(含 PendingCancel、PendingAmend 期间);不算对冲覆盖
    pub residual: Qty,
}

pub struct TargetView {                  // 写者 hft-exec(目标单元的投影)
    pub intent: TargetIntent,            // 当前生效目标(副本)
    pub origin: TargetOrigin,            // Strategy | Engine(MarginStress | External | Command | KillFlatten | AutoFix)
    pub state: TargetState,              // Idle | Working | Stuck(StuckReason) | Broken(BrokenReason)(Broken 不是终态,见"目标意图")
    pub set_seq: u64, pub set_tsc: Tsc, pub last_slice_tsc: Tsc, pub n_slices: u32,
    pub filled: [Qty; 4],                // 本目标生效以来各腿成交净量(带符号)
}

pub struct OrderView {                   // 写者 hft-oms;只覆盖本实例 place 的订单
    pub r: OrderRef, pub group: GroupId, pub inst: InstId, pub side: Side, pub tif: Tif, pub class: IntentClass,
    pub px: Px, pub qty: Qty, pub filled: Qty, pub state: OrderStateView, pub tag: u32,
}
pub enum OrderStateView { PendingNew, Live, PendingCancel, PendingAmend, Filled, Cancelled, Rejected, Unknown }

pub struct ExternalFillView {            // 来源 hft-event ExternalFill;强平、ADL、人工、交割
    pub venue: VenueId, pub inst: InstId, pub group: GroupId /* 不可归属时 GROUP_NONE */, pub leg_ix: u8,
    pub side: Side, pub qty: Qty, pub px: Px, pub fee: i64, pub fee_asset: AssetId,
    pub cause: ExternalCause,
    pub ex_ts_ns: i64,
    pub deadline_tsc: Tsc,               // 本次启动的 TSC;策略须在此前处理该 group,否则 hft-exec 默认处理(要保存时用 Now::ens_of)。
                                         // 名义值:恢复期抑制期间截止不计时(口径归 hft-exec),实际截止可能晚于它,策略按它处理是保守的;
                                         // 0 = 引擎不做默认处理(如 cause = Manual,执行层对该 cell 的 manual_hold 见"目标意图";规则归 hft-exec)
}
pub use hft_event::ExternalCause;        // { Liquidation = 1, Adl = 2, Manual = 3, Delivery = 4, Unknown = 5 }:就是 hft-event ExternalFill.cause 的类型;
                                         // Unknown 含对账自动补记的漏推送成交(条目标志 AUTO_EXTERNAL:置位条件归 reconciler,在 ReconcileReport
                                         // 这条 Input 内合成归 hft-sequencer,之后补推的真实成交按 auto_held 抵扣归 hft-ledger);
                                         // 策略按 Views::group_pos/legs 的实际仓位处理,不累加 qty

#[non_exhaustive]
pub enum RiskEvent {                     // 来源 hft-risk(cap 变化)
    /// 每 (account, venue) 一条:该 venue 档位变化时投递。档位按到强平价的距离 liq_dist 定(阈值归 hft-risk)。
    MarginStress { venue: VenueId, level: MarginLevel /* L1 | L2 | L3 */,
                   liq_dist: Rate,       // min over 该 venue 永续持仓 |liq_px − mark| / mark;未知 = Rate::MAX
                   ratio: Rate,          // 维持 / 保证金余额(交叉校验)
                   deadline_tsc: Tsc },  // L2 的减仓截止(本次启动的 TSC);到期仍 ≥ L2 → 引擎强制减仓
    HedgeBroken { group: GroupId },
    CapSet { source: CapSource, scope: CapScopeView },
    CapCleared { source: CapSource, scope: CapScopeView },   // MarginStress 解除也走这里
}
pub struct RiskStateView {
    pub margin_level: MarginLevel,       // None | L1 | L2 | L3:该账户各 venue 档位的最大值(档位规则归 hft-risk)
    pub liq_dist: Rate,                  // 该账户各 venue liq_dist 的最小值;未知 = Rate::MAX
    pub margin_ratio: Rate, pub deadline_tsc: Tsc,
    pub open_blocked: bool,              // 任一生效 cap 挡该账户 Open
    pub caps: CapBits, pub hard: bool,
    pub flatten_lock: bool,              // 带 flatten 的 Kill 生效中:只接受更接近 0 的目标与 Reduce/Flatten(hft-risk 清仓锁)
    pub max_hedge_notional: Notional,    // 风控对单笔对冲名义的上限(hft-risk RiskParams);执行层与策略据此确定切片上限
    pub max_reduce_notional: Notional,   // 单笔减仓名义上限
    pub max_order_notional: Notional,    // 单笔 Open 名义上限(检查链 #6;名义 = qty × 限价)
    pub conn_liveness_ms: u32,           // 报价健康的连接静默上限(hft-risk RiskParams;默认值在 hft-risk);引擎各处与 quote_ok 用它
    pub dust_notional: Notional,         // 该账户的尘埃线(唯一定义 hft-exec ExecConfig.dust_notional);策略判断"有仓位"用同一条线
}                                        // 越价上限按 inst 取值,见 Views::risk_for
#[derive(Clone, Copy)]
pub struct InstRiskView {                // 写者 hft-risk:该 inst 所属价格带类别的 PxBand 与 RiskParams(取值与默认值归 hft-risk)
    pub max_px_dev: Rate,                // Open 相对 mid 的价格带(检查链 #5)
    pub ioc_max_cross: Rate,             // Open IOC 相对对手价的越价上限(检查链 #5)
    pub rr_max_cross: Rate,              // 风险递减单相对 mid 的越价上限;引擎单与对冲腿经 hft-types cap_cross_px 用它
    pub l3_max_cross: Rate,              // 可选的更宽 L3 越价上限(RiskParams.l3_max_cross,全局一值;默认 0 = 不启用):> 0 时
                                         // MarginStress/Hard 引擎单的越价上限取 max(rr_max_cross, 它),风控 #5 按同一值放行(规则归 hft-risk)
    pub quote_rate: Rate,                // 该 inst 计价资产 → USDT 的换算率(hft-ledger 换算率;与检查链 #6 折算名义同一口径)
    pub rate_backoff: bool,              // 该 inst 所在 (venue, account) 正处于限速退避(hft-risk 令牌桶)
}

pub struct VenueNoticeView {             // 来源 ControlCmd{VenueNotice}(control-plane)
    pub venue: VenueId, pub inst: InstId /* INST_ALL = 整个 venue(hft-types) */,
    pub kind: NoticeKind,                // Delisting{at_ns} | FundingInterval{new_s} | Maintenance{from_ns, to_ns}
}

```

**报价健康(安静 ≠ 过期)**:报价是否可用只看该 inst 的 BBO 连接健康,不看报价最近一次被更新的时刻(`rx_tsc`)。活性证据来自 journal:行情记录的 `MarketHdr.rx_tsc`,以及安静时每个 pong/心跳一条的 `ConnState{Alive}`(来源与发布规则见 [event-model 关键事件体](../../docs/subsystems/event-model.md#关键事件体);hft-venue 对 BBO 连接每 `bbo_ping_ms` 发一次 WS ping 保证有回帧),所以连接健康时 `now − conn_live_tsc` 不超过 `bbo_ping_ms` 加往返时延,一个 inst 的盘口几十秒不变仍然判为健康。

**成员与聚合**:一个 inst 的 BBO 可以由同一 venue 的多条连接送达(主/备、赛马、轮换),每条连接占 `MAX_BBO_CONN_SLOTS` 个槽之一(常量在 [hft-types](../hft-types/README.md) 声明、本 crate 重导出;槽分配归 [hft-venue](../hft-venue/README.md))。`Views::bbo(i)` 的连接健康字段按该 inst 的成员连接在调用时聚合;成员的维护与聚合规则的唯一定义在 [hft-sequencer 连接活性](../hft-sequencer/README.md#连接活性)。对策略的含义:繁忙连接上的安静 inst 随连接的活性证据保持健康;一条成员连接退役、断开或重同步时,只要另一条成员健康,该 inst 不被判不可用。

**判定只有一处**:引擎的对冲腿定价、风险递减单、Broken 自动平仓、L3 减仓与链接腿检查都用 `BboView::quote_live`,阈值 `conn_liveness_ms`(唯一配置位置与默认值在 [hft-risk](../hft-risk/README.md) `RiskParams`,经 `RiskStateView.conn_liveness_ms` 可读);`Views::quote_ok(a, i)` 就是"引擎阈值下的判定"。策略用同一函数,阈值取引擎值(`quote_ok`)或自己的参数;用自己的参数时取 min(参数, `conn_liveness_ms`)——比引擎宽松的阈值没有意义,因为引擎的对冲与风控按引擎阈值拒绝。报价某一侧为 0(无挂单)由调用方按所需的一侧另查。`rx_tsc` 用于按接收时刻的信号(领先-滞后、机会窗口),不用于判可用性;`hw_rx_ns` 的用途见[时间轴](#时间轴)。

`MarginLevel`、`NoticeKind`、`TargetState`、`TargetOrigin`、`StuckReason`、`BrokenReason`、`OrderStateView` 是 SDK 的视图枚举,引擎组件写入同一类型,不另定义;各值的判定语义归写者组件,线上码归登记者:`TargetOrigin` 的码表在 hft-event `ExecDecision.origin`(含 `Engine(AutoFix)`),与其代码形式 `TargetOriginWire` 的互转在本 crate(`From<TargetOrigin> for TargetOriginWire` 与 `From<TargetOriginWire> for TargetOrigin`,一一对应、无失败分支;非法 u8 由 `TargetOriginWire::try_from` 拒绝);`LinkRole` 的判别值就是 `OrderDecision.link_role` 的线上码(0 表示无链接,不是 `LinkRole` 的值);`StuckReason`、`BrokenReason`、`TargetReject` 与 `ExecDecision.detail` 的全部子码数值与 u32 打包规则唯一登记在 [hft-exec](../hft-exec/README.md),本 crate 的这三个枚举写出显式判别值、与之逐值相同、只追加;`ExternalCause` 就是 hft-event `ExternalFill.cause` 的类型(本 crate 重导出);`MarginLevel` 归 hft-risk(按每 (account, venue) 的到强平距离 `liq_dist` 分档,比率只作交叉校验,数据过期至少 L1)。`GROUP_NONE`(无组 / 不针对 group / 不可归属)、`GROUP_ALL`(只用于命令作用域 `*`,不出现在 Advice、StrategyCmd 与投递给策略的事件里)、`INST_NONE`(无 inst)与 `INST_ALL`(作用域与公告里的"全部 inst")的值在 [hft-types](../hft-types/README.md) 定义。

### EventRef

```rust
#[non_exhaustive]
pub enum EventRef<'a> {
    Bbo { inst: InstId, gix: u16, b: BboView },             // 按值;与本事件处理后的 v.bbo(inst) 相同
    Book { inst: InstId, gix: u16 },                         // 一条 L2Batch 整批应用后,或一条交易所来源的 L2Snapshot(含 origin = TopN)整替换后;从 v.book() 读
    Trade { inst: InstId, gix: u16, t: &'a TradeView },
    Funding { inst: InstId, gix: u16, f: &'a FundingView },
    Mark { inst: InstId, gix: u16, px: Px },
    Index { inst: InstId, gix: u16, px: Px },                // IndexPrice 事件(订阅 EvMask::INDEX)
    Liquidation { inst: InstId, gix: u16, l: &'a LiquidationView }, // 公开市场强平流(订阅 EvMask::LIQUIDATION)
    Order { gix: u16, u: &'a OrderUpdate },
    Exec { gix: u16, u: &'a ExecUpdate },
    External { gix: u16, f: &'a ExternalFillView },          // 不可归属 group 时 gix = GIX_NONE
    Risk { a: AccountId, e: &'a RiskEvent },
    Notice { inst: InstId, gix: u16, n: &'a VenueNoticeView },
    Timer { tag: u32, group: GroupId, gix: u16 },            // 以 GroupId 登记,gix 投递时现查
    Advice {                                                 // 本实例 Model 的产出;group = GROUP_NONE(不针对 group)时 gix = GIX_NONE
        gix: u16, local_kind: u16, payload: &'a [u8],
        basis_seq: u64,                                      // Model 产出它时已消费到的 journal seq(AdviceHdr.basis_seq)
        basis_tsc: Tsc,                                      // 该记录头的 tsc;策略按它判过期,不按到达时刻
    },
    Mode { gix: u16, old: RunMode, new: RunMode },
    Conn { venue: VenueId, state: ConnStateKind },
    Manual { gix: u16, payload: &'a [u8], reduce_only: bool }, // StrategyCmd(≤ 256 B,CmdView::parse 解析);GROUP_NONE → GIX_NONE;
                                                             // 命令记录的 seq = v.now().seq(Note 关联用);reduce_only 的约束见"策略描述符"
    Warmup { local_kind: u16, chunk_no: u32, last: bool, payload: &'a [u8] }, // InjectWarmup 的载荷(≤ 4096 B,策略定义)
    #[cfg(feature = "chain")]
    Pool { gix: u16, q: &'a PoolView },                      // 链上报价(chain 特性)
    #[cfg(feature = "chain")]
    Swap { gix: u16, u: &'a SwapUpdate },
}
pub const GIX_NONE: u16 = u16::MAX;   // 事件不针对某个 group(GROUP_NONE、不可归属的外部成交)

pub struct OrderUpdate { pub r: OrderRef, pub group: GroupId, pub tag: u32, pub kind: OrderUpdateKind }
#[non_exhaustive]
pub enum OrderUpdateKind {
    Acked,
    Filled { d: Qty /* 净量 */, px: Px, fee: i64, fee_asset: AssetId, cum: Qty, done: bool },
    Cancelled,
    Rejected { bits: RejectBits, venue_code: u32 /* 交易所拒单:= OrderEvent.reject_code(各所原始码到它的映射归 hft-venue-proto 契约 8);
                                                   本地发送失败(bits 含 LOCAL_SEND_FAIL):hft-event SendStatus 码值,不是交易所码(hft-oms"SendReport 处理")*/ },
    Unknown, Aged,
}

pub struct ExecUpdate {
    pub group: GroupId, pub tag: u32, pub origin: TargetOrigin, pub kind: ExecUpdateKind,
    pub target: [Qty; 4], pub filled: [Qty; 4],
    pub inflight: [Qty; 4],              // 各腿即时在途(LegTrack.inflight_immediate 口径,不含已 Ack 的挂单)
}
#[non_exhaustive]
pub enum ExecUpdateKind {
    Progress { leg_ix: u8, d: Qty },     // 每笔切片终态且有成交时一次(含引擎订单:origin = Engine(..))
    Done { cleared_by_engine: bool },    // 到达目标(全部腿在带内),或目标被引擎清除;Broken 被解除回到 Idle 时也是它
    Stuck(StuckReason),                  // 进入 Stuck 与原因变化时一条
    Broken(BrokenReason),                // 进入 Broken;不是终态(见"目标意图")
}
/// 判别值与 hft-exec ExecDecision.detail 表 2(Stuck)逐值相同;只追加。
#[repr(u8)] #[non_exhaustive]
pub enum StuckReason { Gated(RejectBits) = 1, Shadow = 2, NoLiquidity = 3, StaleQuote = 4, SliceFails = 5, External = 6,
                       EngineOverride = 7, MinOrder = 8 }
/// 判别值与 hft-exec ExecDecision.detail 表 3(Broken)逐值相同;只追加。
#[repr(u8)] #[non_exhaustive]
pub enum BrokenReason { LinkBroken = 1, ReverseFailed = 2, FixExhausted = 3, HedgeVeto = 4 }
```

| 变体 | 快速部分收到 | Model 收到 |
|---|---|---|
| `Bbo`/`Book`/`Trade`/`Funding`/`Mark`/`Index`/`Liquidation` | 按订阅(`Part::Fast/Both`) | 按订阅(`Part::Model/Both`) |
| `Order` | 本实例 `Ctx::place` 的订单;hft-exec 产生的切片不投递 `Order`,汇总为 `Exec` | `model_own(ORDER)` 时同左(只读) |
| `Exec` | 本实例 group 的目标进度 | `model_own(EXEC)` 时同左(只读) |
| `External`、`Risk`、`Mode`、`Conn` | 本实例账户 / cell | — |
| `Notice` | 订阅了该 inst 的 group | 同左 |
| `Timer` | 本实例登记的定时器 | 模型 tick(`MODEL_TICK`,UTC 对齐) |
| `Advice` | 本实例 Model 的产出 | — |
| `Manual` | `StrategyCmd` 指向的 group(`GROUP_NONE` = 实例级,`gix = GIX_NONE`);`cmds` 为空的策略收不到(gateway 已拒) | `model_own(MANUAL)` 时同左(只读;Model 不产生意图,`reduce_only` 约束不适用) |
| `Warmup` | 只在 `SubsBuilder::warmup(Part::Both)` 时 | 本实例的全部 `InjectWarmup`(有 Model 时) |
| `Pool`、`Swap`(chain 特性) | 订阅了该 group 链上腿的实例 / 本实例的 swap | `Pool` 按订阅 |

### Ctx 与订单意图

```rust
pub struct Ctx<'a> { /* 固定容量缓冲;sequencer 在每个输入事件后按调用顺序排空 */ }
impl<'a> Ctx<'a> {
    pub fn place(&mut self, it: OrderIntent) -> Result<IntentHandle, CtxFull>;
    pub fn target(&mut self, t: TargetIntent) -> Result<IntentHandle, CtxFull>;   // 替换该 (account, group) 的当前目标(逐字段相同 = 不替换,见"目标意图")
    pub fn clear_target(&mut self, a: AccountId, g: GroupId) -> Result<(), CtxFull>; // 执行层停止追踪;已有仓位不动
    pub fn cancel(&mut self, h: OrderRef) -> Result<(), CtxFull>;
    pub fn amend(&mut self, h: OrderRef, px: Px, qty: Qty) -> Result<(), CtxFull>;
    /// at 为本次启动的 TSC(由 engine_ns 截止时刻登记时用 Now::tsc_of);定时器跨重启的持久化与换算归 hft-sequencer。
    pub fn timer_at(&mut self, at: Tsc, tag: u32, group: GroupId) -> Result<TimerId, CtxFull>;
    pub fn cancel_timer(&mut self, id: TimerId);
    /// → Note{source = Strategy};满则丢弃并计数。code 与 level 必须是 DESC.notes 中登记的(见"策略描述符");
    /// group = 该 Note 所属的 GroupId(实例级为 GROUP_NONE),写进 Note.group。
    pub fn note(&mut self, code: u32, level: NoteLevel, group: GroupId, inst: InstId, v: [u64; 8]);
    /// 请求对本实例的一个 cell 设 Halted cap(该 cell 有效模式降为 CloseOnly);只有 Resume{Halted} 解除。
    /// 用于策略判定"无法自动处理"的状态(如连续执行失败);reason 为策略自定义码,进 CapRecord 与 Note。
    pub fn request_halt(&mut self, g: GroupId, reason: u16) -> Result<(), CtxFull>;
    /// 上一次 on_event 中各意图的处理结果(本实例;只对紧邻的上一次调用有效)。
    pub fn outcome(&self, h: IntentHandle) -> Option<IntentOutcome>;
    #[cfg(feature = "chain")]
    pub fn swap(&mut self, it: SwapIntent) -> Result<IntentHandle, CtxFull>;
}
pub enum IntentOutcome {
    Placed(OrderRef), Rejected(RejectBits), Shadowed,                // 订单意图
    TargetAccepted, TargetShadowed, TargetRejected(TargetReject),    // 目标意图
}
/// 目标被拒原因(判定语义与线上码 ExecDecision.detail 的唯一登记在 hft-exec;本枚举与之逐值相同,只追加)。
#[repr(u8)]
pub enum TargetReject { NotOwner = 1, BadLegs = 2, SpecInvalid = 3, AlgoUnsupported = 4, SpotShort = 5, CellBroken = 6,
                        ForcedReduceActive = 7, TooManyLegs = 8, FlattenLocked = 9, LotMismatch = 10,
                        ReduceOnlyCmd = 11 /* reduce_only 命令期间设的目标会增加某腿 |仓位|;逐值与 hft-exec detail 表相同 */ }
```

| 每个输入事件的容量 | 值 | 超出 |
|---|---|---|
| 订单意图 `place` | 16 | `CtxFull`,计数 +1;策略按"本事件放弃该意图"处理 |
| 目标 `target` + `clear_target` | 64 | 同上;需要改更多 group 时用定时器分批 |
| 撤/改 | 16 | 同上 |
| 定时器 | 8 | 同上 |
| Note | 16 | 丢弃并计数 |
| `request_halt` | 4 | 同上 |
| swap(chain 特性) | 4 | 同上 |

**门控拒绝 ≠ 执行失败**:`Rejected(bits)` / `Stuck(Gated(bits))` 中的位来自 hft-types `RejectBits`,是风控、模式、kill、cap、限速等**门控**结果,不是交易所的执行结果;策略的连续失败计数(如 hft-strat-lib `Throttle` 的 `Gated`)不计它们。带两腿链接的第一腿在出环前还要过第二腿检查(判定归 [hft-risk](../hft-risk/README.md)):第二腿报价不健康(`Views::quote_ok` 为假:连接不在 Live、重同步中或静默超过 `conn_liveness_ms`;盘口不变不算)→ `LINK_STALE`,第二腿资金不足(现货买入腿的计价资产或永续腿的可用保证金)→ `LINK_FUNDS`;前者通常短暂(报价恢复即可再试),后者需要资金变化。第二腿的定价与数量上限(超过 `max_slip` 时按风控上限定价而不失败、数量不超过当前裸量)归 [hft-oms](../hft-oms/README.md)。

布局为草案,实现时以代码与编译期断言(`size_of`/`offset_of`)为准;全部 `#[repr(C)]` 结构写出显式 `_pad`(无隐式填充,规则归 [hft-types](../hft-types/README.md))。

```rust
#[repr(C)]
pub struct OrderIntent {                 // 112 B
    pub venue: VenueId, pub account: AccountId, pub side: Side, pub tif: Tif,  // Ioc | Gtc | PostOnly | Fok
    pub class: IntentClass,              // 策略声明的类别;最终分类以 hft-risk classify() 为准
    pub flags: u8,                       // REDUCE_ONLY
    pub _pad: [u8; 2],
    pub inst: InstId, pub group: GroupId,
    pub px: Px, pub qty: Qty,
    pub link: LegLinkOpt,                // 两腿链接(some = 0 为单腿);语义归 hft-oms
    pub tag: u32,                        // 策略自定义通道标签
    pub max_rest_ms: u32,                // GTC/PostOnly 最长挂单时长(OMS sweep 发 Aged)
}
#[repr(u8)] pub enum IntentClass { Open = 1, Reduce = 2, Hedge = 3, Flatten = 4 }

#[repr(C)] pub struct LegLinkOpt { pub some: u8, pub _pad: [u8; 7], pub l: LegLink }   // 72 B
#[repr(C)]
pub struct LegLink {                     // 64 B
    pub group: GroupId,
    pub role: LinkRole,                  // 策略只填 First;链接结构不合法(role、on_fail、tif 的组合)→ Rejected(INVALID),判定 hft-oms link_valid
    pub _pad: [u8; 3],
    pub tolerance: Rate,                 // 该 group 的补腿阈值(两腿差 / 对冲量);LegTracker 记录最近值,hft-exec 据此判定补腿
    pub hedge: HedgePlan,
}
/// 判别值与 hft-event OrderDecision.link_role 逐值相同(0 = 无链接只出现在该记录字段,不是本枚举的值)。
#[repr(u8)] pub enum LinkRole { First = 1, Second = 2, SecondRetry = 3, HedgeFix = 4, Reverse = 5 }
#[repr(C)]
pub struct HedgePlan {                   // 48 B
    pub kind: HedgeKind,                 // 两种链接的第二腿都由 OMS 产出,策略只 place 第一腿
    pub px_mode: PxMode,
    pub on_fail: OnFail,                 // OnFill 默认 Retry;Simultaneous 只取 Reverse | Hold
    pub tif: Tif,                        // 第二腿 Ioc(Simultaneous 可 Fok)
    pub venue: VenueId, pub account: AccountId, pub side: Side,
    pub reduce_only: u8,                 // 1 = 平仓第二腿提示;风控验证可减量,交易所 reduceOnly 仅在支持时设置
    pub inst: InstId,
    pub ratio_num: u32, pub ratio_den: u32,
    pub residual: ResidualPolicy,        // hft-exec 的残余规则读取
    pub _pad: [u8; 3],
    pub px_param: Rate,
    pub max_slip: Rate,                  // 软阈值:对手价偏离超过它时第二腿按风控 rr_max_cross 上限定价照发(语义归 hft-oms)
    pub limit_px: Px,
}
#[repr(u8)] pub enum HedgeKind { None = 0, OnFill = 1, Simultaneous = 2 }
#[repr(u8)] pub enum PxMode { Cross = 1, Mid = 2, Limit = 3 }
#[repr(u8)] pub enum OnFail { Retry = 1, Reverse = 2, Hold = 3 }
#[repr(u8)] pub enum ResidualPolicy { Defer = 1, TrimExcess = 2, Hold = 3 }
#[repr(C)] pub struct OrderRef { pub venue: VenueId, pub account: AccountId, pub _pad: [u8; 2], pub slot: u32, pub gen: u32 } // 12 B;= hft-oms 的槽句柄(代码中写作 r#gen,2024 版保留字)
```

`LegLink`/`HedgePlan`/`OrderRef`/`IntentClass` 的**类型**定义在本 crate(意图词汇);第二腿怎么下(`OnFill` 按第一腿每个成交增量、`Simultaneous` 在第一腿 `place` 的同一事件内)、重试、对冲量上限、`LinkTable` 等**语义**归 [hft-oms](../hft-oms/README.md) 与 [hft-exec](../hft-exec/README.md)。

### 目标意图

策略可以输出订单意图,也可以输出**目标仓位**,由 [hft-exec](../hft-exec/README.md) 负责切片、两腿协调、对冲与补腿。目标在被替换或清除前一直有效;执行层在每个相关事件(BBO、成交、定时器)推进一次,O(腿数)。

```rust
#[repr(C)]
pub struct TargetIntent {                // 232 B
    pub account: AccountId, pub _pad0: [u8; 3],
    pub group: GroupId,
    pub legs: [LegTarget; 4], pub n_legs: u8,
    pub reason: TargetReason,
    pub urgency: Urgency,                // 对冷却与 Stuck 判定的倍率由 hft-exec 定义
    pub _pad1: [u8; 5],
    pub band: Rate,                      // 不交易带:|target − actual| / max(|target|, floor) ≤ band 时该腿不动;
                                         // floor = hft-exec ExecConfig.band_floor_notional / mark
    pub exec: ExecSpec,                  // 136 B
    pub tag: u32,                        // 原样带进切片订单、ExecDecision 与 ExecUpdate
    pub _pad2: u32,
}
#[repr(C)] pub struct LegTarget { pub leg_ix: u8, pub _pad: [u8; 7], pub qty: Qty } // 16 B;该腿目标净量(带符号,资产单位)
/// 判别值与 hft-event ExecDecision.reason / urgency 逐值相同。
#[repr(u8)] pub enum TargetReason { Rebalance = 1, Exit = 2, MarginStress = 3, Hard = 4, Manual = 5 }
#[repr(u8)] pub enum Urgency { Passive = 1, Normal = 2, Urgent = 3 }

/// 136 B:判别字节 + 7 B 对齐填充(repr(C, u8) 的布局,不是字段)+ 128 B 载荷;本类型与含它的 TargetIntent 都不以原始字节
/// 写进 journal(ExecDecision 逐字段记录;快照里的目标按 hft-exec ExecCellRow 的显式行逐字段写出),所以两者都不派生 IntoBytes。
/// 判别值 TakerSlice = 1、PairOnFill = 2、Simultaneous = 3、MakerFirst = 4,与 hft-event ExecDecision.algo 的码相同。
#[repr(C, u8)]
pub enum ExecSpec {
    TakerSlice(TakerSliceParams),        // 各腿独立吃单切片
    PairOnFill(PairParams),              // 两腿:先下 first_leg,按其成交净量下另一腿 IOC
    Simultaneous(SimulParams),           // 两腿同发,失败按 on_fail
    MakerFirst(MakerParams),             // 预留(optional):先挂单,成交后对冲
}
#[repr(C)]
pub struct TakerSliceParams {            // 48 B
    pub depth_frac: Rate,                // 单笔 ≤ 弱侧可见深度 × depth_frac
    pub depth_levels: u8,                // 计入可见深度的档数上限(合法范围与越界的拒绝码归 hft-exec;无深度订阅时只用顶档)
    pub _pad0: [u8; 7],
    pub max_cross: Rate,                 // IOC 限价 = 对手价 × (1 ± max_cross),再按 Views::risk_for 的上限截取(hft-types cap_cross_px);也是可见深度的价格范围
    pub max_chunk_notional: Notional,    // 单笔名义上限(执行时另按 RiskParams 截取,规则归 hft-exec)
    pub min_chunk_notional: Notional,    // 单笔名义下限(本目标最后一笔例外,见 hft-exec)
    pub cooldown_ms: u32,                // 同一 cell 两笔切片的最小间隔
    pub _pad1: u32,
}
#[repr(C)]
pub struct PairParams {                  // 96 B
    pub slice: TakerSliceParams,         // 第一腿切片
    pub first_leg: FirstLeg,
    pub first_leg_ix: u8,                // first_leg = Fixed 时的先手腿号;否则 0
    pub hedge_px_mode: PxMode,
    pub _pad0: [u8; 5],
    pub hedge_cross: Rate, pub hedge_max_slip: Rate,                            // → HedgePlan 的 px_param、max_slip
    pub ratio_num: u32, pub ratio_den: u32,                                     // 第二腿量 = 第一腿成交净量 × num/den
    pub fix_tolerance: Rate,             // → LegLink.tolerance
    pub residual: ResidualPolicy,
    pub _pad1: [u8; 7],
}
#[repr(u8)] pub enum FirstLeg { ByLiquidity = 1, PerpFirst = 2, SpotFirst = 3, Fixed = 4 }
#[repr(C)] pub struct SimulParams {      // 72 B
    pub slice: TakerSliceParams, pub on_fail: SimulOnFail, pub _pad0: [u8; 7],
    pub fix_tolerance: Rate, pub residual: ResidualPolicy, pub _pad1: [u8; 7],
}
#[repr(u8)] pub enum SimulOnFail { Reverse = 1, Hold = 2 }
#[repr(C)] pub struct MakerParams {      // 128 B
    pub offset: Rate, pub requote: Rate, pub max_rest_ms: u32, pub _pad0: u32,
    pub chunk_notional: Notional, pub hedge: PairParams,
}
```

各字段的执行语义(深度口径、最后一笔、残余、补腿阈值、按风控上限截取)以 [hft-exec 执行规格参数](../hft-exec/README.md)为准;本节只定义类型。

- **腿顺序**:`reason ∈ {MarginStress, Hard}` 时执行层强制 `PerpFirst`(先减永续,释放保证金);`Rebalance/Exit` 默认 `ByLiquidity`(流动性差的一腿先),可由 `first_leg` 覆盖。"永续/现货"由 `InstMeta` 的市场类型判断。
- **方向**:现货腿目标必须 ≥ 0(不做现货借币做空)。
- **分类**:目标的增仓部分(|目标| > |实际| 的那部分)受模式门与风控 Open 规则约束;减仓部分按 Reduce/Flatten 分类(见下节)。
- **校验**(hft-exec `set_target`,结果经 `Ctx::outcome` 返回):调用者是该账户绑定的实例且 group 属于它(`NotOwner`)、`leg_ix < n_legs` 且不重复(`BadLegs`)、现货腿 ≥ 0(`SpotShort`)、算法与腿数匹配(`TooManyLegs`、`SpecInvalid`、`AlgoUnsupported`)、cell 非 Broken(`CellBroken`)、MarginStress L3 默认减仓期间只接受减得更多的目标(`ForcedReduceActive`)、账户清仓锁期间(`Kill{Account, flatten}` 之后直到 `Resume{Kill}`)只接受每腿更接近 0 的目标(`FlattenLocked`)、配对算法两腿 lot 无法在 `max_chunk_notional` 内对齐(`LotMismatch`;按原因的豁免归 hft-exec)。判定归 [hft-exec](../hft-exec/README.md)。
- 进度经 `EventRef::Exec` 通知;执行层每次决定写 Output 记录 `ExecDecision`([hft-event](../hft-event/README.md))。切片算法、深度口径、对冲量上限、默认减仓全部归 hft-exec。
- **重发同一目标不会重置失败计数**:与当前目标逐字段相同的 `target` 视为已接受、不替换(规则归 hft-exec),而 `Stuck(SliceFails)` 等失败计数只在目标被替换或清除时复位。策略要"重试"一个卡住的目标,必须改动某个字段(约定:`tag` 的低位放重发次数),或在同一次 `on_event` 里先 `clear_target` 再 `target`。
- **人工外来成交冻结引擎的自动处理**:`External{cause = Manual}` 可使执行层对该 cell 置 `manual_hold`,期间不做补腿、残余修剪、外来成交默认处理与自动有界平仓计时;置位的条件与例外、哪些动作解除它(`clear_target` 不解除)以及 `Note{ExecManualHold}` 只在 [hft-exec 人工保持与恢复期抑制](../hft-exec/README.md#人工保持与恢复期抑制)定义。策略按自己的规则决定是否、何时恢复动作,不依赖某个动作必然解除它。
- **引擎目标先撤策略挂单**:引擎为某腿设 `reason ∈ {MarginStress, Hard}` 的目标时,先撤本账户该 inst 上全部非执行层挂单(含策略 `place` 的 reduce-only 平仓单;OMS 入口 `cancel_resting_on`,规则归 hft-exec 与 [hft-oms](../hft-oms/README.md)),策略照常收到 `Order{Cancelled}`;策略不得假设自己的挂单在 L3 或清仓期间保留。
- **Broken 不是终态,也不冻结引擎减仓**(判定与参数归 [hft-exec](../hft-exec/README.md)):cell 处于 Broken 期间,执行层可能① 在 Broken 持续 `broken_auto_fix_ms`(开始条件与计时口径归 hft-exec,恢复期抑制期间不计时)、期间无人工 `HedgeFix`,并且该 group 已静默(无在途、无 Pending/Unknown 槽,见 `LegView.has_pending`/`has_unknown`)之后,自动做一次有上限的"平掉多出的一腿"(`origin = Engine(AutoFix)`;"一次"指一次可执行的尝试,报价不可用、限速退避、门控拒绝与零成交只等待或重试,口径归 hft-exec;成功写 `Note{ExecAutoFixDone}` P1、cell 回 Idle,判失败写 `Note{ExecAutoFixFailed}` P0、保持 Broken 等人工);② 执行人工 `HedgeFix`(`Engine(Command)`);③ 在 MarginStress L3 与 `Kill{flatten}` 下先减该 group 的永续腿(`Engine(MarginStress)` / `Engine(KillFlatten)`);④ 人工核销后解除 Broken(不下单,见下条)。这些订单的成交照常以 `Exec{Progress}` 报告;Broken 解除、cell 回到 Idle 时投递 `Exec{Done{cleared_by_engine: true}}`。策略因此**不得假设** Broken 期间仓位不变、Broken 只能由人工解除、或解除后一定能开仓:收到 `Done` 后按 `Views::group_pos`/`legs` 的实际仓位重新评估;账户级 `HedgeFailure` cap 在自动平仓成功后仍在,开仓是否被挡看 `RiskStateView.open_blocked` 与 `Risk{CapSet/CapCleared}`。
- **人工核销不经成交改变腿净量**:人工命令 `WriteOffResidual{account, group}` 适用于静止的 group(Broken 或非 Broken 都可),把多出那一腿的已成交裸量从该 group 移到账户级 unassigned,并清零该 group 全部腿的 `residual`;`GroupPosView`/`LegView` 随之变化,不产生订单与成交事件,Broken 的 group 随后按尘埃口径解除(投递 `Exec{Done{cleared_by_engine: true}}`)。"静止"的定义、选哪一腿与 LegTracker 的调整归 [hft-oms 残余核销](../hft-oms/README.md#残余核销),命令的前置条件与确认归 [control-plane](../../docs/subsystems/control-plane.md),记账归 [hft-ledger](../hft-ledger/README.md)。策略不得假设 group 的腿净量只因成交而变化,按视图读取。

### Advice 载荷

```rust
pub trait AdvicePayload: Sized {
    const LOCAL_KIND: u16;                                   // 本策略内唯一;0 与 0xFF00..=0xFFFF 保留
    const MAX_LEN: usize;                                    // ≤ 4096
    fn encode(&self, w: &mut PayloadWriter<'_>) -> Result<(), PayloadFull>;
    fn decode(b: &[u8]) -> Result<Self, PayloadError>;       // 快速部分调用:无分配、O(len)
}
impl AdviceOut<'_> {
    pub fn emit<P: AdvicePayload>(&mut self, group: GroupId /* 或 GROUP_NONE = 不针对 group;GROUP_ALL 非法 */, p: &P) -> Result<(), AdviceFull>;
    pub fn emit_raw(&mut self, group: GroupId, local_kind: u16, bytes: &[u8]) -> Result<(), AdviceFull>;
}
```

- 记录格式 `AdviceHdr { strategy, account, local_kind, group, len, basis_seq, basis_tsc }` + `payload[len]`(字段、顺序与字节布局以 [hft-event](../hft-event/README.md) 为准);引擎不解释载荷。`strategy`、`account`、`group`、`local_kind`、`len` 来自 `emit`;`basis_seq`、`basis_tsc` 由 advisor 填:产出这条 Advice 的 `Model::on_event` 正在处理的那条 journal 记录的 seq 与记录头 tsc(即 `ModelViews::now()`),策略无法也无需设置。`len > 4096` 的数据由策略拆成多条(各自约定 `local_kind`)。
- 载荷只含整数定点值(`Rate`/`Notional`/`Px`/`Qty`/计数);Model 内部的 f64 只在 `encode` 前经 `model_math::to_rate`/`to_notional`(HalfEven、饱和)转换一次,这是唯一的浮点边界。约定载荷首字节为该 `local_kind` 的版本号。
- 每次 `Model::on_event` 的 `AdviceOut` 容量:128 条、合计 128 KiB;超出 → `AdviceFull` 并计数。非法的一条(`local_kind` 为保留值、载荷 > 4096 B、`group = GROUP_ALL`)同样不写入、返回 `AdviceFull` 并计数;`emit::<P>` 对 `P::LOCAL_KIND` 保留值与 `P::MAX_LEN > 4096` 做编译期断言。advice 环满时 advisor 丢弃最旧的同 (instance, group, local_kind) 待发项(不阻塞)。
- **过期按依据判断,不按到达时刻**(Advice 记录没有有效期字段,sequencer 也不给 Advice 打过期标记;判定只在策略):
  1. **跨启动**:`basis_seq < v.now().boot_seq`(上次启动时的数据)→ 过期,直到 Model 产出依据本次启动记录的 Advice。这是跨启动的唯一判据;TSC 不跨启动比较。
  2. **本次启动内**:`age = v.now().tsc − basis_tsc`(Model 依据的数据时刻)超过策略自己的参数 → 过期。
  过期即不可开仓("失效即保守")。这样 advisor 落后、重启后追赶重放、回放时,旧依据的 Advice 都不会被当成新的;advisor 的滞后告警是 `AdvisorLag`(唯一定义在 [hft-event](../hft-event/README.md),滞后按 engine_ns 计)。hft-strat-lib `Expiring::put_advice` 按这两条缓存。
- **group 指纹变化**:依据旧指纹产出的 Advice(`basis_seq < spec_seq`)由 sequencer 丢弃并注记,不投递(见[参数变化时的 group 重置](#参数变化时的-group-重置))。

### 预热注入

依赖长窗口统计的 Model(分位、资金费历史)在没有可用检查点时(首次启动、`LAYOUT_VERSION` 提升、检查点被丢弃)或某些 group 被重置时(`Note{ModelGroupReset}`)需要历史样本。SDK 提供一条**通用的预热注入通道**,把历史样本作为 journal 的 Input 送进 Model,而不是等实时样本攒满:

1. **命令**:`InjectWarmup{instance, local_kind: u16, chunk_no: u32, last: bool, payload ≤ 4096 B}`(主体 Trader 或 pipeline;编码与确认级别归 [hft-event](../hft-event/README.md) 与 [control-plane](../../docs/subsystems/control-plane.md))。它作为 Input 记 journal(sequencer 只校验实例存在与长度);advisor 尾随 journal,在该命令 `Applied` 后把它作为 `EventRef::Warmup{local_kind, chunk_no, last, payload}` 投递给该实例的 Model;策略调用过 `SubsBuilder::warmup(Part::Both)` 时 sequencer 也把同一事件投递给快速部分。回放与重启重放照读,结果确定。
2. **载荷由策略定义**:`local_kind` 是 Warmup 自己的命名空间(与 Advice 的 `local_kind` 无关;0 与 `0xFF00..=0xFFFF` 保留),载荷首字节为版本号、只含整数定点值、自带 `GroupId`(命令里没有 group)。布局写在策略 README 的"预热载荷"表里,生产者是该策略的 pipeline 插件(Python 包 `strategies/<name>/pipeline/`,随策略发布,由 [pipeline 框架](../../python/pipeline/README.md)调度;从交易所 REST K 线、collector 数据或 journal 归档生成样本);策略 crate 与插件各有一份金样字节(同一个 `fixtures/strategies/<name>/warmup/` 文件)互相约束。
3. **分块语义**:同一批注入按 `chunk_no` 从 0 递增,最后一块 `last = true`。每块必须**自身完整、可独立应用**(按块内的 group 与时间范围);插件产出块列表,客户端逐块发送、不切字节、不拼接([hft-ctl](../hft-ctl/README.md))。Model 对块的应用必须**幂等且与先后无关**——只填补尚无数据的时间点,不覆盖已有的实时样本或已注入的样本。因此重复注入、注入与实时样本交错、注入被中断后重来都得到确定且一致的结果;`chunk_no` 不连续只用于诊断(策略可写 Note)。
4. **代价与界限**:每块的 Model 处理须满足模型 tick 的 1 ms 时间片(超出时策略把样本拆成更小的块,由插件控制块大小);注入的数据量计入 journal(策略 README 写出一次完整注入的条数与字节数)。注入只影响 Model 状态与随后的 Advice,不直接产生意图。
5. Model 的检查点覆盖注入后的原始样本,所以注入后写出的检查点即包含预热结果;正常重启不需要再注入。

### 装配:StrategySet 与 strategy_set!

```rust
pub trait StrategySet: 'static {
    type Inst;                           // 枚举:每个变体一个具体策略
    type ModelInst;
    type Params: Send;                   // 枚举:解码后的参数(ctl-gateway → Mailbox → sequencer)
    fn kinds() -> &'static [StrategyDesc];
    fn decode_params(kind_id: u16, body: &[u8], schema_ver: u16, g: &GroupTable) -> Result<Self::Params, ParamError>;
    fn validate_params(p: &Self::Params, g: &GroupTable, rd: &RefView<'_>) -> Result<(), ParamError>;
    fn create(i: StrategyInstance, p: &Self::Params, g: &GroupTable, rd: &RefView<'_>) -> Result<Self::Inst, ParamError>;
    fn create_model(i: StrategyInstance, p: &Self::Params, g: &GroupTable, rd: &RefView<'_>) -> Self::ModelInst;
    fn on_event<V: Views>(s: &mut Self::Inst, ev: &EventRef<'_>, v: &V, ctx: &mut Ctx<'_>);
    fn on_params(s: &mut Self::Inst, p: &Self::Params, g: &GroupTable, rd: &RefView<'_>) -> Result<(), ParamError>;
    fn subscriptions(s: &Self::Inst, g: &GroupTable, out: &mut SubsBuilder);
    fn snapshot(s: &Self::Inst, w: &mut SnapshotWriter);
    fn restore(s: &mut Self::Inst, r: &mut SnapshotReader) -> Result<(), RestoreError>;
    fn model_mem_bytes(p: &Self::Params, n_groups: u16) -> u64;              // ctl-gateway 与 hft-core 装配的预算校验
    fn model_snapshot_bytes(p: &Self::Params, n_groups: u16) -> u64;
    fn has_model(kind_id: u16) -> bool;                                   // NoModel 或集合外为 false;按具体类型判定,不以空布局、零版本或零检查点大小推断
    fn model_layout(kind_id: u16) -> &'static ModelLayoutDesc;
    fn model_layout_version(kind_id: u16) -> u32;                          // = Model::LAYOUT_VERSION;集合外或 NoModel 为 0,不以布局描述的哈希代替
    fn model_on_event<V: ModelViews>(m: &mut Self::ModelInst, ev: &EventRef<'_>, v: &V, out: &mut AdviceOut<'_>);
    fn model_on_params(m: &mut Self::ModelInst, p: &Self::Params, g: &GroupTable, rd: &RefView<'_>) -> ModelParamEffect;
    fn model_snapshot_chunk(m: &Self::ModelInst, from_gix: u16, w: &mut SnapshotWriter, max_bytes: u32) -> SnapshotProgress;
    fn model_restore(m: &mut Self::ModelInst, r: &mut SnapshotReader) -> Result<(), RestoreError>;
    fn model_path_version(kind_id: u16) -> u32;                               // = Model::PATH_VERSION
    fn model_snapshot_path(m: &Self::ModelInst, w: &mut SnapshotWriter);
    fn model_restore_path(m: &mut Self::ModelInst, r: &mut SnapshotReader) -> Result<(), RestoreError>;
    fn model_path_settle_ms(p: &Self::Params) -> u64;                        // advisor 写进 Note{ModelPathReset}
    /// ctl-gateway 的 SetParams 预览(冷):按新旧参数与 GroupTable(以 GroupId 对齐)估算;口径见"参数变化时的 group 重置"第 3 条。
    fn reset_preview(old: &Self::Params, old_g: &GroupTable, new: &Self::Params, new_g: &GroupTable) -> ResetPreview;
    #[cfg(feature = "schema-json")]
    fn schema_json(kind_id: u16) -> Option<String>;          // = params::schema_json(&DESC, Model::LAYOUT, schema.version);GetSchema 与 `hft-core schemas`(经 registry::export_schemas)
    fn cmds(kind_id: u16) -> &'static [CmdDesc];             // ctl-gateway 校验 StrategyCmd 用;集合外的 kind_id → 空
}
/// 与 hft-event CmdPreview 的同名字段同名同宽,原样进入 SetParams 的预览。
pub struct ResetPreview { pub reset_groups: u32, pub no_open_ms: u32 }

// 全仓唯一的调用在 strategies/registry(一行一个策略):
hft_strategy_sdk::strategy_set! {
    pub struct Strategies {
        foo::Foo,
        bar::Bar,
    }
}
```

- 宏生成三个枚举(实例、模型、参数)与 `impl StrategySet`;每个方法是一层 `match` + 内联调用,静态分派、无 `dyn`。编译期断言:`kind_id`、`name`、`schema.id` 各自唯一。行上可带 `#[cfg(...)]`(optional 策略用 Cargo feature 门控)。
- `hft-sequencer` 是 `Sequencer<S: StrategySet>`;advisor 是 `Advisor<S>`;ctl-gateway 用 `S::decode_params` + `S::validate_params` 校验 `SetParams`、用 `S::model_mem_bytes`/`S::model_snapshot_bytes` 算 advisor 预算的输入、用 `S::reset_preview` 生成结构参数改动的预览、用 `S::kinds()` 的 `non_prod_only` 查绑定、用 `S::cmds` 校验 `StrategyCmd`、用 `S::schema_json` 回答 `GetSchema`。
- 列出真实策略的调用只在 [strategies/registry](../../strategies/registry/README.md) 一处,hft-core 与 replayer 因此天然使用同一列表。replayer 遇到列表外的 `kind_id` → `UnknownStrategyKind` 并失败,不跳过。hft-testkit 另有一次只含 CI 测试策略(kind_id 0xFF00–0xFFFF)的调用,不含任何真实策略。本 crate 自己的 `tests/`(宏的分派与编译期测试)另有只含测试策略(kind_id 0xFF00–0xFFFF)的调用。`cargo xtask deps` 检查:除这三处外任何 crate 不得调用 `strategy_set!`;除 registry 外任何 crate 不得依赖 `strategies/*`;引擎 crate(含 dev 依赖)与 hft-testkit 不得依赖 registry。

### 运行模式下意图的行为表

模式状态(期望模式、cap、有效模式)的唯一归属是 [hft-risk `mode`](../hft-risk/README.md);`RunMode` 枚举与线上编码在 [hft-types](../hft-types/README.md)(`Shadow = 1`、`CloseOnly = 3`、`Live = 4`);优先级表在 [control-plane](../../docs/subsystems/control-plane.md)。本表规定**策略发出的意图**在各有效模式下怎么处理(cell = (实例, group))。模拟不是运行模式:模拟账户上的策略照常以这些模式运行。

| 有效模式 | 策略 `place` 的 Open(含不能减少组裸露的无链接 Hedge) | 策略 `place` 的 Reduce/Flatten/风险递减 Hedge | 策略链接产生的第二腿(OMS 发出) | 目标的增仓部分 | 目标的减仓部分 |
|---|---|---|---|---|---|
| `Shadow` | 风控评估后只记录(`SHADOW`) | 同左 | 不经模式门(第一腿只记录,不会有成交) | 目标照常接受;第一笔切片记为 `SHADOW` 决策后 `Stuck(Shadow)`,不重复出切片 | 同左 |
| `CloseOnly` | 拒绝(`CLOSE_ONLY`) | 出环 | 出环(不经模式门) | 不发(`Stuck(Gated(CLOSE_ONLY))`,目标保留) | 执行 |
| `Live` | `open_allowed[group]` 为真才出环,否则只记录(`MODE_SHADOW`) | 出环 | 出环(不经模式门) | `open_allowed` 为真才执行,否则同 Shadow 行 | 执行 |

- **结果回填**:`Shadowed` = 该订单意图只被记录——有效模式 Shadow(决策记录头带 `SHADOW` 位)或 Live 下 `open_allowed = false` 的 Open(`OrderDecision.reject = MODE_SHADOW`);`Rejected(bits)` 只用于门控拒绝,`bits` 不含 `MODE_SHADOW`。目标意图被接受时,该次 `ExecDecision` 带 SHADOW 标志 → `TargetShadowed`,否则 `TargetAccepted`(有效模式 Shadow 时由 hft-exec 标;Live 且 `open_allowed = false` 而目标使某腿离开当前净量的,由 sequencer 补标,见 [hft-sequencer 目标意图](../hft-sequencer/README.md))。
- **模式门的作用范围**:模式门在 sequencer 里、风控之前执行,只作用于策略发出的意图与 hft-exec 为策略目标生成的切片。OMS 自动产生的第二腿、Simultaneous 反向、LegTracker HedgeFix、hft-exec 的引擎默认减仓(MarginStress L3、External 截止后的处理)不经模式门,只受 HARD 约束;已成交的第一腿不会因模式切换而裸着。
- **Shadow 不冻结出场**:期望 Shadow 但该 cell 净仓、在途或 LegTracker 残余非 0 时,有效模式为 `CloseOnly`(`ShadowDeferred`),策略照常走平仓逻辑;归零后 sequencer 自动落到 Shadow 并写 `Note{ShadowDeferred}`。
- **Shadow 评估但不提交**:意图完整调用只读 `risk.evaluate`,按与 Live 相同的顺序短路;`OrderDecision` 带 `SHADOW`,`reject` 记录 0(可放行)或真实首个风控拒绝位,`slot = 0`,结果仍为 `Shadowed`。不扣令牌、不计在途、不分配 OMS 槽、不出环,每笔按当前真实资源评估,不模拟前一笔 Shadow 意图的资源消耗。
- **策略手动对冲**:无链接 Hedge 能否减少组裸露由 [hft-risk 分类](../hft-risk/README.md#分类) 按组净量与在途量验证;CloseOnly 放行风险递减的 Hedge,其余仍按 Open 拒绝。
- **Mode 事件**:有效模式每次变化(来源无论是 `SetMode` 还是 cap)都向该 cell 投递 `EventRef::Mode{old, new}`。降到 `CloseOnly`/`Shadow` 时 sequencer 撤销该 cell 的 Open 类挂单;执行层保留目标但停发增仓切片(见上表),回到 `Live` 后按原目标继续推进。策略不希望恢复后继续增仓时,应在 `Mode` 事件里 `clear_target` 或改发目标,并重置确认窗口等开仓状态。
- **cell 生命周期**:`SetMode{group = *}` 同时写该 (strategy, account) 的 `default_mode`;`SetParams` 新增的 group 取 `default_mode`;移除 group 即删除其 cell。

### 确定性规则

**快速部分**(`Strategy` 的全部方法):

1. 输出只取决于(自身状态、事件、`Views`);时间只取 `v.now()` 与事件字段。跨事件保存的时刻用 `engine_ns`(时长 = `time::ms(参数)`),只在本事件内比较的行情年龄用 TSC(时长 × `now.tsc_per_ms`);快照里不出现 TSC(见[时间轴](#时间轴))。
2. 无 f64/f32:快速部分所在模块带 `#![deny(clippy::float_arithmetic)]`,`cargo xtask hot-check` 检查该属性存在;价差与比例用 `Rate`,金额 `Notional`,乘积 i128,取整方向显式(默认取更保守的一侧,显示类用 HalfEven)。
3. `on_event` 零分配、无系统调用、无 `dyn`、无 panic(用 checked 运算并在溢出时放弃本事件该 group 的动作、写 `Note`)、无 `HashMap`、无全局可变量、无随机数。
4. 跨事件持有的引用以 `GroupId`/`OrderRef`/`TimerId` 为键,不持有 gix 或指针。

**慢速部分**(`Model` 的全部方法):

1. 输入只有投递给它的 `EventRef` 与 `ModelViews`;时间只取 `v.now()`(= 当前 journal 记录)。Model 状态(含检查点)里的时刻存 UTC ns 或 `engine_ns`,不存 TSC。
2. 允许 f64 与有界分配(总量 ≤ `mem_bytes` 的声明);禁止 I/O、系统时钟、线程、rayon 类并行、`HashMap`/`HashSet` 迭代;多 group 的循环与求和按 gix 升序。
3. 超越函数(`powf`、`ln`、`log10`、`exp`)只用 `hft_strategy_sdk::model_math`(纯 Rust 的 `libm` 实现,跨平台同结果);不用 `f64::mul_add`;NaN/Inf 一律视为"未就绪"并在载荷里置标志,不输出。
4. f64 → 定点只经 `model_math::to_*`:`to_rate(x)` = HalfEven(x × 10^9)、`to_notional(x)` = HalfEven(x × 10^6)(乘积按 f64 计算,`to_px`/`to_qty` 按资产指数同理),超出 i64 饱和到 `i64::MIN`/`i64::MAX`;NaN 与 ±∞ 返回 `None`(调用方按"未就绪"处理)。
5. 给定同一检查点(两节)与同一条 journal 记录序列,`AdviceOut` 字节逐字节相同(测试见下);检查点字节与 `snapshot_chunk` 的分块大小无关;路径依赖的派生状态全部进路径状态节,周期性更新按 UTC 边界对齐(见 [Model 检查点](#model-检查点))。

### 版本与兼容规则

- **全部策略与引擎同仓同版本编译**,不存在二进制 ABI;SDK 的版本号随仓库发布,兼容性问题在 CI 编译期暴露。
- **影响 journal 字节的类型**(`OrderIntent`、`TargetIntent`、`ExecSpec` 及其参数、`IntentClass`、`LegLink`/`HedgePlan`、参数 body 编码、Advice 头)按 [hft-event](../hft-event/README.md) 的只追加规则演进:新字段追加在尾部、枚举只追加判别值、不重排不复用;`OrderDecision`/`ExecDecision` 记录体与这些类型的一致性由 `const` 断言检查。
- **纯 API**(`Views`、`Ctx`、`SubsBuilder` 的方法,`EventRef` 的变体):新增是兼容改动(`EventRef` 及事件枚举为 `#[non_exhaustive]`,策略必须有兜底分支);删除或改签名是破坏性改动,须在同一改动里改完所有 `strategies/*` 并写 Agent Note。
- **策略自身**:`StrategyDesc.version` 管快速部分快照布局与决策语义,sequencer 把它写进快照行头 `StrategyRowHdr.desc_version`(布局归 [hft-sequencer](../hft-sequencer/README.md)),恢复时 sequencer 构造该节的 `SnapshotReader` 时填入同一值,`restore` 经 `r.desc_version()` 读取;`restore` 至少接受当前与上一版本,否则返回 `RestoreError::VersionUnsupported`(sequencer 的处理见 hft-sequencer);`Model::LAYOUT_VERSION` 只管原始样本节、`Model::PATH_VERSION` 只管路径状态节(规则见 [Model 检查点](#model-检查点));`ParamSchemaDesc.version` 只追加字段——`decode` 接受旧 `schema_ver` 的 body(缺失字段取 `default`),拒绝更新的(`SchemaTooNew`);字段从不删除或改义,废弃字段保留占位;Advice 与预热载荷用首字节版本号,对未知版本按"无数据"处理。
- `kind_id`、`schema.id`、Advice 与 Warmup 的 `local_kind`、Note 码、`CmdDesc.op`、检查点节的 `tag` 只追加、永不复用(其中 `kind_id`、Note 码、`op` 与检查点节 `tag` 的编号消失或被复用由 `cargo xtask gen-schema` 核对,CI 以 PR 目标分支的合并基点为基线,失败条件归 [hft-event 实现说明](../hft-event/README.md#实现说明));`LAYOUT_VERSION`、`PATH_VERSION` 只增不减;报表字段名不改义(废弃的保留并在 `doc` 标注)。

### chain 特性(optional)

`chain` 特性(链上策略接入时启用)提供 `chain::PoolView`、`chain::SwapIntent`、`chain::SwapUpdate`、`EventRef::Pool`、`EventRef::Swap`、`SubsBuilder::pool` 与 `Ctx::swap`;字段与 hft-event 的 `PoolQuote`、`SwapDecision`、`TxLifecycle` 一一对应。

```rust
#[cfg(feature = "chain")]
pub struct PoolView {                    // 最近一条 PoolQuote 的投影(按报价规模档)
    pub route_id: u32, pub block: u64, pub upd_tsc: Tsc,
    pub n: u8, pub tiers: [PoolTier; 4], // PoolTier { size: Notional, buy_px: Px, sell_px: Px, depth: Qty }(已含池子费与冲击)
    pub gas_cost: Notional,
}
#[cfg(feature = "chain")]
pub struct SwapUpdate { pub r: OrderRef, pub group: GroupId, pub tag: u32, pub state: TxStateView /* 与 TxLifecycle 对应 */, pub amount_out: Qty }
#[cfg(feature = "chain")]
#[repr(C)]
pub struct SwapIntent {                  // 160 B
    pub chain_venue: VenueId, pub account: AccountId /* 钱包 */, pub side: Side,
    pub valid_blocks: u8,                // 默认值以 hft-chain-evm 为准
    pub exact: u8,                       // 0 ExactIn 1 ExactOut(ExactOut 的适用范围见 hft-chain-evm)
    pub _pad0: [u8; 3],
    pub group: GroupId, pub route_id: u32,
    pub amount_in: Qty, pub min_out: Qty, pub amount_out: Qty, pub max_in: Qty,  // 限额只能被 chain-core 收紧
    pub limit_px: Px, pub edge: Notional, pub max_bribe: Notional,
    pub signal_seq: u64, pub link: LegLinkOpt, pub tag: u32, pub _pad1: u32,
}
```

`valid_blocks`(maxBlock)默认值、ExactOut 规则与 SwapGuard 语义归 [hft-chain-evm](../hft-chain-evm/README.md)。未启用 `chain` 时以上类型不存在,ccex 策略不受影响。

### 测试桩(testing 特性)

`testing` 特性(只在 `[dev-dependencies]` 启用)让策略在不装配引擎的情况下单测快速部分与 Model:

```rust
pub mod testing {
    pub struct MockViews { /* 实现 Views 与 ModelViews;全部状态由测试显式设置 */ }
    impl MockViews {
        pub fn new(rd: RefFixture) -> Self;                  // RefFixture:最小 refdata(inst、资产、VenueCaps、费率、账户)
        pub fn at(&mut self, now: Now) -> &mut Self;                   // 含 engine_ns 与 boot_seq
        /// 模拟一次重启:TSC 从 new_tsc 重新计数、engine_ns 连续(停机不计)、boot_seq 前进;行情与连接状态清空。
        pub fn restart(&mut self, new_tsc: Tsc, new_boot_seq: u64) -> &mut Self;
        pub fn bbo(&mut self, i: InstId, bid: (Px, Qty), ask: (Px, Qty)) -> &mut Self;   // 同时把 rx_tsc 设为 now.tsc
        pub fn bbo_rx(&mut self, i: InstId, rx_tsc: Tsc, hw_rx_ns: i64, src: RxTsSrc) -> &mut Self;   // 只改接收时刻
        pub fn conn(&mut self, i: InstId, live_tsc: Tsc, up: bool, resync: bool) -> &mut Self; // 设聚合后的连接健康,报价不变
        pub fn book(&mut self, i: InstId, bids: &[Level], asks: &[Level]) -> &mut Self;
        pub fn funding(&mut self, i: InstId, f: FundingView) -> &mut Self;
        pub fn portfolio(&mut self, a: AccountId, f: impl FnOnce(&mut PortfolioFixture)) -> &mut Self;
        pub fn group_pos(&mut self, a: AccountId, g: GroupId, p: GroupPosView) -> &mut Self;
        pub fn legs(&mut self, a: AccountId, g: GroupId, l: LegView) -> &mut Self;
        pub fn target(&mut self, a: AccountId, g: GroupId, t: Option<TargetView>) -> &mut Self;
        pub fn mode(&mut self, a: AccountId, g: GroupId, m: RunMode) -> &mut Self;
        pub fn risk(&mut self, a: AccountId, r: RiskStateView) -> &mut Self;
        pub fn risk_for(&mut self, a: AccountId, i: InstId, r: InstRiskView) -> &mut Self;
        pub fn venue_ready(&mut self, a: AccountId, v: VenueId, ready: bool) -> &mut Self;
        pub fn leverage(&mut self, a: AccountId, i: InstId, x100: Option<u32>) -> &mut Self;
    }
    pub struct CtxHarness { /* 拥有 Ctx 缓冲与上一事件的 outcome 表 */ }
    impl CtxHarness {
        pub fn new(desc: &'static StrategyDesc) -> Self;              // 记录 note 时断言 code/level 已在 desc.notes 登记
        pub fn run(&mut self, f: impl FnOnce(&mut Ctx<'_>)) -> Recorded; // places、targets、clears、cancels、timers、notes、halts、full 计数
        pub fn set_outcomes(&mut self, o: &[IntentOutcome]);           // 模拟引擎对上一事件意图的处理结果
    }
    pub struct ModelHarness { /* 拥有 AdviceOut 缓冲与计数分配器 */ }
    impl ModelHarness {
        pub fn run(&mut self, f: impl FnOnce(&mut AdviceOut<'_>)) -> Vec<(GroupId, u16, Vec<u8>)>;
        /// 先调 snapshot_path,再按 max_bytes 反复调用 snapshot_chunk 直到 Done;返回两节字节与块数。
        pub fn snapshot_all<M: Model>(&mut self, m: &M, max_bytes: u32) -> Ckpt;
        /// 按 advisor 的顺序恢复:restore(原始样本节);keep_path 为真时再 restore_path(为假模拟 PATH_VERSION 不符)。
        pub fn restore_all<M: Model>(&mut self, m: &mut M, c: &Ckpt, keep_path: bool) -> Result<(), RestoreError>;
        pub fn peak_alloc_bytes(&self) -> u64;                         // 与 Model::mem_bytes 比较
    }
    pub struct Ckpt { pub path: Vec<u8>, pub body: Vec<u8>, pub n_chunks: u32 }
    /// 用记录型 writer 跑 snapshot_chunk,断言实际写出的节、元素类型与长度符合 M::LAYOUT。
    pub fn check_layout<M: Model>(m: &M) -> Result<(), LayoutMismatch>;
    pub fn layout_json(d: &ModelLayoutDesc) -> String;                 // 布局金样的规范 JSON
    pub mod ev { /* 拥有型事件构造器:bbo(..)、order(..)、funding(..)、exec(..)、external(..)、risk(..)、notice(..)、index(..)、liquidation(..)、
                    advice::<P>(.., basis_tsc)、manual(op, vals, reduce_only)(经 params::encode_cmd)、warmup(local_kind, chunk_no, last, bytes)…;
                    .as_ref() → EventRef */ }
}
```

设定器与 `Views` 的读取方法同名:在 `&mut MockViews` 上调用设定器(`let m = &mut v; m.bbo(i, bid, ask)`,或在 `at(..)` 起头的链式调用里),在 `&MockViews` 上或以 `Views::bbo(&v, i)` 调用读取方法(对 `MockViews` 值直接调用时方法解析先找到 `&self` 的读取方法)。

`MockViews` 不撮合:它只回放测试设定的状态。需要成交、两腿、故障注入时用 [hft-testkit](../hft-testkit/README.md) 的场景与 DST(其撮合器即 [hft-sim](../hft-sim/README.md) 的撮合器)。

## 实现说明

<details><summary>实现要点</summary>

### Ctx 的内存与排空

- `Ctx` 借用 sequencer 为每个实例预分配的 `CtxBufs`:每类意图一个定长数组 + 一条按调用顺序的操作序列(`[OpRef; 128]`,`OpRef = (kind, ix)`;Note 不进操作序列,另有自己的数组)。sequencer 在 `on_event` 前调 `CtxBufs::begin()`、之后按 `CtxBufs::ops()`(`(IntentHandle, Op)`,调用顺序)处理,保证"先撤后下""先 clear 后 target"等调用顺序的语义在回放中不变;`notes()` 与 `full_count()` 读 Note 与满计数。
- `IntentHandle(u16)` = 操作序列下标;处理结果经 `CtxBufs::clear_outcomes()` + `set_outcome(h, o)` 写入同一 `CtxBufs` 的 `outcomes` 区,下一次 `on_event` 前保留、再下一次覆盖。
- `cancel_timer` 每事件另有 8 个的容量(超出丢弃并计数,无返回值)。`TimerId(u64)` 由 `CtxBufs` 的计数器分配(`next_timer_id`/`set_next_timer_id`,随 sequencer 的实例状态进快照),跨重启稳定。
- 引擎用 `CtxBufs::ctx_with_timer_quota(remaining, registered)` 借入本轮所有实例共享的剩余配额与本实例按 TimerId 升序的登记表。`timer_at` 在返回成功前预留一格;配额、事件缓冲或 ID 计数器耗尽时返回 `CtxFull`,不消耗 ID。取消已登记或本轮刚登记的 ID 立即释放一格,重复取消、未知 ID 和超出取消缓冲的请求不释放;取消未知 ID 不会预先取消以后分配的同值 ID。操作仍按调用顺序排空,独立 `ctx()` 测试桩只检查事件缓冲。
- 配额登记不分配;取消使用有界事件缓冲和已登记 ID 的二分查找,后者为 O(log 已登记数)。每实例的 `full_count()` 在排空时计入引擎一次。引擎持久化的实现边界见 [sequencer 限制](../hft-sequencer/README.md#已知限制与后续工作)。
- `MODEL_TICK = u32::MAX`:策略不以它登记定时器。
- 容量常量在 `src/ctx.rs`;改容量是 SDK 改动(影响全部策略的预算),在本 README 表格同步。

### 快照节

- `SnapshotWriter`/`SnapshotReader` 是本 crate 的定长字节写入/读取器(小端,`put_u8…put_i128`、`put_bytes`、`put_row`、`section(tag)`):`SnapshotWriter::new(limit)` 在构造时一次分配上限大小的缓冲,`clear()` 复用,之后不再分配;`section(tag)` 是策略与 Model 状态的布局标记(`SectionDesc.tag`),普通 writer 不为它写字节;引擎 L1 组件的节用 `begin_section(section, version)` … `end_section()` 写出,前缀为 hft-event `SectionHdr`(`count` = 节内 `put_row`/`add_rows` 计的行数,`len` = 节体补零到 8 B 后的长度);读者 `next_section()`/`find_section(n)` 按 `SectionHdr` 定位(长度不是 8 的倍数、超出剩余字节或保留字段非 0 → `RestoreError::BadSection`),`get_*` 读过尾 → `Truncated`,`try_row` 读含枚举的行(非法判别值 → `BadValue`);`RestoreError` 另有 `VersionUnsupported`、`Capacity`(`#[non_exhaustive]`)。`SnapshotReader::desc_version(&self) -> u16` 返回构造者填入的节版本(策略节 = 行头 `desc_version`,其他用途为 0)。快速部分每实例上限 `strategy_snapshot_max`(256 KiB),超限 → `SnapshotFull`(配置错误,在最大 group 数的测试里暴露;writer 以上限构造,越界的那次 `put_*` 起不再写入,`SnapshotWriter::status() -> Result<usize /* 已写字节 */, SnapshotFull>` 供调用方在 `snapshot` 返回后检查);Model 的检查点没有固定上限,以 `Model::snapshot_bytes` 的声明为准。引擎 L1 组件的快照节使用同一类型;快照记录的整体结构(`SnapshotHdr` + 分节)归 [journal-format](../../docs/subsystems/journal-format.md)。
- 快速部分的状态进 `StateSnapshot` 的策略节:sequencer 为每个实例写行头 `StrategyRowHdr`(布局与行版本归 [hft-sequencer](../hft-sequencer/README.md),其中 `desc_version` = `StrategyDesc.version`),节体由策略写。关键帧在空闲轮一次写完全部节(不分片;周期与预算归 hft-sequencer 与 [latency-budget](../../docs/latency-budget.md)),所以 `Strategy::snapshot` 必须 O(状态字节)、无分配。Model 的 `snapshot_path`/`restore_path`/`snapshot_chunk`/`restore` 只用于 [Model 检查点](#model-检查点)文件,不进 journal。
- `snapshot_chunk` 的 writer 在 `max_bytes` 处不截断:Model 自己按"再加一个 group 会超出就停"决定块边界;writer 只统计字节数供 advisor 计时与 `snapshot_bytes` 核对。

### define_params! 的展开

1. `FooFields`:每字段一个类型化成员(`Rate`、`Notional`、`u32`、`bool`、枚举),`#[derive(Clone, Copy)]`。
2. `const SCHEMA: ParamSchemaDesc` 与 `const _: () = validate_schema(&SCHEMA)`(编译期校验)。
3. `decode`:`params::decode_body`(`body_view` 容器校验、版本前缀、缺失字段取默认、范围/per_group/枚举检查)→ 把 defaults 转成类型化的 `FooFields` → 克隆到每个 gix → 应用覆盖表。
4. `at(gix) -> &FooFields`;`defaults() -> &FooFields`;`structural_changed(&old, old_gix, gix) -> bool`(比较两版参数在同一 GroupId 上的结构字段,供 `on_params` 判定重置与 `StrategySet::reset_preview` 计数)。

### 参数编解码器的使用方

| 使用方 | 调用 | 线程/进程 |
|---|---|---|
| ctl-gateway | `S::decode_params` → 各策略 `ParamSet::decode` → `params::decode_body`(含 `body_view`);比对 `schema_digest`;`S::validate_params`;`S::model_mem_bytes` 预算 | hft-core 的 ctl-gateway 线程(冷) |
| `hft-cfg` | `field_ix` + `parse_value` 把 TOML 转成 `RawParams` → `encode_body`;`schema_json` 导出 | 冷工具 |
| `hft-py`(pipeline、ops_bot) | PyO3 模块 `params`:`load_schemas`/`load_schema`(= `OwnedSchema::from_json`)、`encode`/`decode`(= `encode_body`/`decode_body`)、`encode_cmd` | Python 冷进程 |
| ctl-gateway(`StrategyCmd`) | `S::cmds` → `decode_cmd` 校验 | hft-core 的 ctl-gateway 线程(冷) |
| `hft-ctl` | `encode_cmd`(人工 `StrategyCmd`) | 冷工具 |
| `define_params!` | `decode_body` | 同 ctl-gateway |

`hft-cfg`、`hft-ctl` 与 `hft-py` **不链接策略 crate**,只链接本 crate 的 `params` 编解码器:它们读取发布制品附带的规范 schema JSON(打包时由 `hft-core schemas --out <dir>` 导出为 `<kind_id>-<name>.schema.json` + `index.json`;运行中也可用 `GetSchema` 取),用 `params::OwnedSchema::from_json` 还原后调用同一编解码器;`schema_digest` 保证与运行中的二进制一致。pipeline 只经 hft-py 编码参数,**不存在纯 Python 的参数或命令编码器**。

### 规划源文件

| 文件 | 职责 |
|---|---|
| `src/lib.rs` | 重导出(含 `prelude` 的 L0 类型);`Strategy`、`Model`、`NoModel`、`StrategyDesc`、`StrategyInstance`、`ModelParamEffect`、`GroupResets`、`ResetCause`、`SnapshotProgress` |
| `src/params.rs`、`src/params_macro.rs` | `ParamSet`、`ParamSchemaDesc`、`FieldDesc`、`FieldRef`、`SchemaView`、`Relax`、`MinorChange`、`Unit`、`ParamError`、`RawParams`、`define_params!`、编译期校验 |
| `src/params/{body.rs,codec.rs,text.rs,schema_json.rs,minor.rs,json.rs,owned.rs}` | SPB1 body 布局与 `body_view`、`params` 编解码器(含命令载荷)、文本解析与格式化、规范 schema JSON 与摘要(`schema-json` 特性)、`minor_change_check`、手写 JSON 读取、`OwnedSchema` |
| `src/desc.rs` | `NoteDesc`、`ReportFieldDesc`、`ReportAgg`、`ReportScope`、`CmdDesc`、`CmdSchema`、`CmdView`、描述符编译期校验 |
| `src/checkpoint.rs`、`src/layout.rs` | 检查点键类型(`CkptKey`:`kind_id`、`layout_version`、`schema_id`、实例、`last_seq`)与键比较;`ModelLayoutDesc` 与规范 JSON;文件格式与读写在 hft-core / advisor |
| `src/group.rs` | `GroupTable`、`GroupSpecExt::fingerprint` |
| `src/views.rs` | `Views`、`ModelViews`、`BookRead`、`Now`、`RefView` |
| `src/view_types.rs` | 全部视图类型与视图枚举(含 `InstRiskView`、`RxTsSrc`)、`BboView::quote_live` |
| `src/event.rs` | `EventRef`、`OrderUpdate`、`ExecUpdate`、`GIX_*` |
| `src/intent.rs` | `OrderIntent`、`IntentClass`、`LegLink`、`HedgePlan`、`OrderRef`、`IntentOutcome` |
| `src/target.rs` | `TargetIntent`、`LegTarget`、`ExecSpec` 及参数、`TargetReason`、`Urgency` |
| `src/ctx.rs` | `Ctx`、`CtxBufs`、容量常量、`SubsBuilder`、`SubsErr`、`OwnEvMask` |
| `src/advice.rs` | `AdvicePayload`、`AdviceOut`、`PayloadWriter/Reader` |
| `src/snapshot.rs` | `SnapshotWriter`、`SnapshotReader`、`RestoreError` |
| `src/set.rs` | `StrategySet`、`strategy_set!`、`ResetPreview` |
| `src/model_math.rs` | `libm` 包装与 f64 → 定点转换(全 crate 唯一允许浮点的模块) |
| `src/chain.rs` | `chain` 特性 |
| `src/testing/*.rs` | `testing` 特性 |
| `tests/set_dispatch.rs`、`tests/params_roundtrip.rs`、`tests/body_view.rs`、`tests/ui/*.rs` | 见测试 |

</details>

## 延迟与资源契约

SDK 自身的入口(在 `decided_tsc − proc_tsc` 分段内,[latency-budget](../../docs/latency-budget.md)):

| 入口 | p50 | p99 | 基准名 |
|---|---|---|---|
| `Ctx::place` | ≤ 5 ns | ≤ 15 ns | `sdk/ctx_place` |
| `Ctx::target` | ≤ 8 ns | ≤ 20 ns | `sdk/ctx_target` |
| `StrategySet` 分派开销(match + 调用) | ≤ 2 ns | ≤ 5 ns | `sdk/set_dispatch` |
| `AdvicePayload::decode`(≤ 256 B 定长布局) | ≤ 20 ns | ≤ 60 ns | `sdk/payload_decode` |
| `CmdView::parse` + 全部 `val` | ≤ 10 ns | ≤ 30 ns | `sdk/cmd_parse` |
| `BboView::quote_live`(一次比较) | ≤ 2 ns | ≤ 5 ns | `sdk/quote_live` |
| `Now::ens_of` / `tsc_of` / `utc_of` | ≤ 3 ns | ≤ 8 ns | `sdk/time_conv` |
| `Views::bbo`(按值,含成员槽聚合)与其余 `Views` 访问器 | 见 hft-sequencer | 见 hft-sequencer | `seq/bbo_view` 等(实现、基准名与数值唯一归属 [hft-sequencer](../hft-sequencer/README.md)) |
| `ParamSet::decode`(64 group × 64 字段,ctl-gateway 线程,冷) | ≤ 50 µs | — | `sdk/params_decode` |
| `params::body_view`(4 096 条覆盖,冷) | ≤ 10 µs | — | `sdk/body_view` |

**每个策略必须在自己 README 登记的入口与默认上限**(在策略 crate 的 `benches/`,`cargo xtask bench-gate` 读取;策略可以更严,放宽需写理由):

| 入口 | p50 | p99 | 基准名 |
|---|---|---|---|
| `on_event` 快侧未命中(绝大多数 tick) | ≤ 15 ns | ≤ 40 ns | `strategies/<kind>/tick_miss` |
| `on_event` 命中 → 订单意图 | ≤ 300 ns | ≤ 1 µs | `strategies/<kind>/tick_fire` |
| `on_event` 慢侧更新(重算触发价) | ≤ 60 ns | ≤ 150 ns | `strategies/<kind>/slow_update` |
| `on_event(Advice)` 解码并缓存 | ≤ 100 ns | ≤ 300 ns | `strategies/<kind>/advice` |
| `on_event` → 闸门 + 构造 `TargetIntent` | ≤ 250 ns | ≤ 800 ns | `strategies/<kind>/target_set` |
| `on_event(Exec)` | ≤ 80 ns | ≤ 250 ns | `strategies/<kind>/exec_update` |
| `on_event(Risk)`,每个受影响 group | ≤ 150 ns | ≤ 400 ns | `strategies/<kind>/risk_event` |
| `Model::on_event` 行情采样(advisor) | ≤ 1 µs | ≤ 5 µs | `model/<kind>/sample` |
| `Model::on_event` 模型 tick 全量计算(advisor) | ≤ 300 µs | ≤ 1 ms | `model/<kind>/tick` |
| `Model::on_event(Warmup)` 应用一块(有预热载荷的策略) | ≤ 300 µs | ≤ 1 ms | `model/<kind>/warmup_chunk` |
| `Model::snapshot_chunk` 一块(advisor 给的 `max_bytes`) | ≤ 300 µs | ≤ 1 ms | `model/<kind>/snapshot_chunk` |
| `Model::snapshot_path`(路径节一次写完,`PATH_VERSION > 0` 的策略) | ≤ 300 µs | ≤ 1 ms | `model/<kind>/snapshot_path` |
| `Model::on_event(Order/Exec/Manual)`(订阅了 `model_own` 的策略) | ≤ 1 µs | ≤ 5 µs | `model/<kind>/own_event` |

- 执行层推进目标的预算(每事件 O(腿数))归 [hft-exec](../hft-exec/README.md);策略预算不含它。
- 未实现慢侧、Advice 或 TargetIntent 的策略可在 `benches/not-applicable.toml` 以 `slow_update`、`advice`、`target_set` 键声明不适用理由;不生成计时样本。其余四个策略入口必须实测;已提供基准的入口不得同时声明不适用。覆盖检查与报告归 [tools](../../tools/README.md#cargo-xtask-bench-gate)。
- 模型 tick 必须在 1 ms 内完成(advisor 的 `poll_once` 时间片);超过的模型按 gix 分片,跨多个 tick 完成。分片由 Model 自己实现,SDK 不提供续算接口:每个 tick 处理哪一片只由该模型 tick 的 UTC 时刻决定(例如 ⌊tick 时刻 / 周期⌋ 对分片数取模),所以分片位置与重启、检查点位置无关,不进状态。检查点写出同样按块分到多次 `poll_once`。
- 内存:`CtxBufs` 每实例约 24 KiB(64 个 232 B 的 `TargetIntent` 为主);`BboView` 按值约 88 B,`Views::bbo` 每次调用读 1 行报价 + 至多 `MAX_BBO_CONN_SLOTS` 个 `ConnLive` 槽(未命中的 tick 只读报价);Model 的堆按 `mem_bytes` 声明计入 hft-core 的 `advisor.mem_budget_mib`;`MockViews` 只在测试。SDK 本身无全局状态、无分配(`define_params!` 的 decode、`GroupResets` 与 `schema_json` 除外,均在冷路径)。

## 测试与验收

用例与验收见 [TESTS.md](TESTS.md)。

## 依赖

- 内部:`hft-types`(定点、ID、`RunMode`、`RejectBits`、`CapSource`)、`hft-refdata`(`InstMeta`、`AssetMeta`、`VenueCaps`、`AccountMeta`、`FeeSchedule`,只读)、`hft-event`(`GroupSpec`、`LegDef`、`Level`、`ConnStateKind`、`NoteLevel`、事件体)。
- 外部:`zerocopy`(布局转换,版本与 hft-event 一致)、`bitflags`、`libm`(纯 Rust 超越函数,保证 Model 跨平台同结果)、`sha2`(只在 `schema-json` 特性,算 `schema_digest`);`trybuild` 只在 dev。
- 禁止:任何 L1 及以上内部 crate(hft-book、hft-oms、hft-risk、hft-ledger、hft-exec、hft-sequencer、hft-journal、hft-rt、hft-venue*)、任何 `strategies/*`、I/O、`std::time`、线程、`tokio`、`serde`(schema JSON 与布局 JSON 手写格式化)。
- 浮点:crate 根带 `#![deny(clippy::float_arithmetic)]`,只有 `model_math` 模块 `#[allow]`;`hot-check` 确认 `model_math` 只被策略的 `src/model/` 引用([hot-path-rules](../../docs/hot-path-rules.md) N3、S5)。

## 延伸阅读

- [07 策略层](../../design/07-策略层.md)、[第八轮裁决](../../design/_work/round8-decisions.md)、[M0-c 五路评审汇总](../../design/_work/review-2026-10-03-M0c五路汇总.md)、[M0-d 五路评审汇总](../../design/_work/review-2026-10-03-M0d五路汇总.md)、[M0-e 五路评审汇总](../../design/_work/review-2026-10-03-M0e五路汇总.md)
- Agent Note:[Model 检查点的路径状态节](../../.agents/notes/proposed/architecture/2026-10-03-model-path-section.md)、[人工外来成交与停机标记](../../.agents/notes/proposed/architecture/2026-10-03-manual-hold-and-stop-marker.md)、[预热注入](../../.agents/notes/proposed/architecture/2026-10-03-warmup-injection.md)、[Broken 自动有界平仓](../../.agents/notes/proposed/architecture/2026-10-03-broken-auto-fix.md)、[不做同所现货 × 永续](../../.agents/notes/proposed/architecture/2026-10-03-no-same-exchange-spot-perp.md)、[报价新鲜度 = 连接健康](../../.agents/notes/proposed/architecture/2026-10-03-quote-freshness-conn-health.md)、[引擎时间 engine_ns](../../.agents/notes/proposed/architecture/2026-10-03-engine-time.md)、[保证金按到强平距离分档](../../.agents/notes/proposed/architecture/2026-10-03-margin-liq-distance.md)、[M1.5 延迟探针](../../.agents/notes/proposed/architecture/2026-10-03-latency-probe-milestone.md)
- [hft-exec](../hft-exec/README.md)(目标执行、`ExecDecision.detail` 子码)、[hft-risk](../hft-risk/README.md)(模式、cap、MarginStress)、[hft-ledger](../hft-ledger/README.md)(权益数据)、[hft-oms](../hft-oms/README.md)(LegLink 语义)、[hft-refdata](../hft-refdata/README.md)(`VenueCaps`、`AccountMeta`)、[hft-event](../hft-event/README.md)(Advice、StrategyParams 容器、ExecDecision、Note 记录)
- [hft-sequencer](../hft-sequencer/README.md)(分派、模式门、Views 适配器、advisor)、[apps/hft-core](../../apps/hft-core/README.md)(advisor 承载 Model、内存预算)、[control-plane](../../docs/subsystems/control-plane.md)(`SetParams`、`GetSchema`)
- [strategies/AGENTS.md](../../strategies/AGENTS.md)、[cookbook:新增策略](../../docs/cookbook/adding-a-strategy.md)、[hot-path-rules](../../docs/hot-path-rules.md)

## 已知限制与后续工作

- 一个实例只对应一个账户;跨账户的组合策略不支持(需要时以多个实例 + 外部协调实现,触发条件:出现跨子账户的策略需求)。
- 每个 (account, group) 同时只有一个目标;一个 group 最多 4 腿。
- `MakerFirst` 是预留接口(optional),执行算法未规格化;启用条件见 hft-exec。
- 不提供随机数;需要随机化的策略用事件 `seq` 派生的确定性哈希。
- 簿视图只到 L2 档位,无 L3/队列位置。
- Model 检查点的原始样本节按 group 分块,单个 group 的记录不跨块拆分:单 group 状态大到一块超过 1 ms 时该块超时(advisor 计数)。可选扩展:单 group 记录跨块续写(触发条件:某策略单 group 检查点 > 1 MiB 且 `model/<kind>/snapshot_chunk` p99 超预算)。路径状态节一次写完,其大小由策略控制在一块的时间片内。
- 结构参数变化只支持按 group 重置后重新预热,不做存量样本的重新分箱。
- 路径节被丢弃(`PATH_VERSION` 提升后的第一次重启)时,`path_settle_ms` 内的 Advice 与连续运行不同,当日 recompute 跳过这一段(见 [Model 检查点](#model-检查点));这是用一次短暂不一致换取原始样本与长窗口统计不随逻辑改动失效。
- 路径状态必须在有限的事件时间内收敛(`path_settle_ms` 有限):无限记忆的派生状态(EWMA 一类)给不出上界,这类 Model 要改用有限窗口,否则丢路径节后的 recompute 无法恢复比较。
- Model 订阅的本实例 `Order`/`Exec`/`Manual` 只读:Model 不能下单或改目标,只能经 Advice 影响快速部分。

### 开发备忘

<details><summary>给实现者</summary>

- `Now.utc_ns`、`utc_of` 与 `tsc_per_ms` 由 sequencer 从 journal 段头的时钟校准对(`tsc_hz` 与段头 UTC/TSC 对;不用 Meta 的 `ClockStatus`,sequencer 不读 Meta)得出,回放确定;`Now.engine_ns` 按 [event-model 时间戳语义](../../docs/subsystems/event-model.md#时间戳语义)的公式由 sequencer 计算;实现都在 hft-sequencer 的 Views 适配器里。
- `BboView` 的报价部分取自 hft-book `Top`,连接健康部分由 hft-sequencer 按成员位图现算(聚合规则归 [hft-sequencer 连接活性](../hft-sequencer/README.md#连接活性));本 crate 只规定字段语义与 `quote_live`。
- `Views::risk_for` 的实现在 hft-sequencer 的 Views 适配器(从 hft-risk 的 inst → 价格带类别表投影,O(1));`model_own` 的事件重建在 advisor 镜像(镜像状态与 sequencer 逐 seq 一致)。
- 实现顺序:`params` + `body_view` + `define_params!` + 金样 → `desc`(描述符与编译期校验、`encode_cmd`/`CmdView`)→ `intent`/`target`/`event`/`view_types` → `ctx` → `group`(指纹)→ `set` 宏 + 两个测试策略 → `advice` → `checkpoint`/`layout`(两节与路径金样)→ `testing` → `schema_json`(含 `minor_change`)→ `chain`。

</details>
