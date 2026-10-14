---
description: "hft-core 的单写者主循环:按优先级逐条取输入(BBO、advice 与 md 逐条处理,每条之后让位于新到的 ctl 与实盘回报,md 另让位于 BBO;到期定时器在条间让位)、写 seq journal、驱动簿/策略集/执行层/风控/OMS/ledger、写出决策;对装配处生成的 StrategySet 泛型(静态分派);同一份代码由实盘、恢复、离线验证回放与 advisor 镜像四种驱动运行"
kind: crate
tier: hot
scope: core
status: spec
---

# hft-sequencer

## 概要

`hft-sequencer` 是**唯一的装配核**([architecture §7](../../docs/architecture.md#7-组件分层)):它拥有全部业务状态的实例(簿、`Oms`、`Risk`(其 `mode` 模块是模式、cap、闸门与 kill 投影的唯一归属)、`Ledger`、`Exec`、策略实例、定时器堆、命令去重表),并定义主循环——按 `ctl > rep(实盘回报)> bbo > advice > md > rep-sim`(rep-sim = 模拟账户回报,最低,每轮 ≤ `sim_max_per_round`)逐条取输入,**BBO、advice 与 md 逐条处理,每条之后先让位于新到的 ctl 与实盘回报(md 另让位于 BBO),到期定时器在条间让位**,先写 seq journal 再分派、把输出(决策、结果、快照、注记)写回 journal 并推到出环。它**对 `S: StrategySet` 泛型**:具体的策略集由装配处提供——生产与重演用 [`strategies/registry`](../../strategies/registry/README.md)(crate `hft-strategy-registry`,只有 apps/hft-core、apps/replayer 依赖它)里唯一一处 SDK `strategy_set!` 宏调用生成的静态分派枚举,测试用 hft-testkit 自己的测试策略集;本 crate(含 `[dev-dependencies]`)不依赖 registry、不认识任何具体策略,新增策略不改本 crate。它也提供 advisor 任务(策略的慢速部分 `Model` 在这里运行)与四种驱动(实盘 / 冷启动恢复 / 离线验证回放 / advisor 镜像)。它**不创建线程**(线程计划在 [apps/hft-core](../../apps/hft-core/README.md))、除写 RAM journal 段外不做 I/O、不读系统时钟;凡是依赖运行环境的决定都由实盘驱动当场写成 Input 记录,因此状态 = f(journal 的 Input 记录)([architecture §9](../../docs/architecture.md#9-回放恢复与确定性)、[08 §5](../../design/08-数据面与回放.md))。一个交易所(子)账户只绑定一个策略实例(`AccountMeta.bound_instance`)。每个 hft-core 进程实例(配置键 `instance.name` 为 `prod` 或 `sim`,同一二进制)有自己的 sequencer 与 journal:`sim` 实例只含 Sim 账户、不加载任何交易所 key;`prod` 实例不含 Sim 账户(dev2 上 `prod` 角色的实例只在测试网/模拟盘联调窗口运行;统一升级后的主机上 `prod` 与 `sim` 同机运行)。实例划分与"实例 × 账户类型"的启动校验归 [apps/hft-core](../../apps/hft-core/README.md);本 crate 的代码对两种实例相同,同一进程混跑实盘与模拟账户时的让位规则(见[模拟账户](#模拟账户))保留为防御。它还维护**引擎时间** `engine_ns`(跨重启连续、停机不计)与 BBO 连接的活性,作为 SDK `Now`/`BboView` 的数据源。

## 目录

- [使用本组件](#使用本组件)
- [实现说明](#实现说明)
- [延迟与资源契约](#延迟与资源契约)
- [测试与验收](#测试与验收)
- [依赖](#依赖)
- [延伸阅读](#延伸阅读)
- [已知限制与后续工作](#已知限制与后续工作)

## 使用本组件

### 装配与运行(由 apps/hft-core、apps/replayer、hft-testkit 调用)


`EngineSetup::encode_blob` 写完整的 ECF1 决策配置,`decode_blob` 仅凭该 blob、同一 journal 的 refdata、时钟与 epoch 还原装配。OMS、ledger、exec 的行格式由各归属 crate 编解码;容量/轮转参数为段 3/5。缺任何必需段即失败,不从本机配置或默认参数补齐。`config_blob::decode_risk_blob` 只调用 [hft-risk RKP1](../hft-risk/README.md#配置riskparams版本化经-setrisk-替换并作为-configblob-写入-journal)。网络连接与环容量段 4/6 由 app 装配,不参与决策回放。`EngineSetup::from_snapshot(log, seq)` 只接受指定快照,从节 9 取全部生效配置引用,核对该快照之前 ConfigBlob 的完整身份和 SHA-256;epoch 取 OMS 节 3,装配时钟取该段头,与引导块 ClockStatus 核对。节 25 必须存在;全零锚点仅允许在 Startup 的 Boot 快照或 Recovery/RecoverFromSnapshot 的 Recovery 快照,且位于该段 BootDone 和任何 Input 之前;判断不读取旧运行被丢弃的尾部。普通滚段与后续快照不得空缺锚点。Recovery 快照允许保留上一运行的频率,其余快照锚点必须与段头同频。缺配置、未来才出现的配置、摘要不符或时钟事实冲突均失败。

**ECF1 段 8 Assembly**(行版本 1)保存决策所需的装配事实,行 16 B 小端 `{field:u16, index:u16, account:u8, venue:u8, reserved:[u8;2], value:u64}`。field 1/2/3 各一行,index/account/venue=0,value 分别为 role_prod(bool)、advisor_max_lag_ms(u32)、drain_timeout_ms(u32);field 4 每个 gw 一行,index 是从 0 连续的 kill 字下标,venue 给 id,account/value=0;field 5 每个 feed venue 一行,index/account/value=0;field 6 每个 Live (account, venue) 一行,index=0,value 为所需 key 槽位图(0–3,含显式空集合)。行按 (field,index,account,venue) 严格递增,reserved=0;缺标量、重复键/gw、未知引用或缺 key 槽位行一律拒绝。只记录槽位图,不记录秘密、签名或秘密引用。epoch、base_engine_ns、TSC 校准取 journal 快照/段头,不写入此配置。

```rust
pub struct Sequencer<D: Driver, S: StrategySet> { /* 全部业务状态 + 驱动 + 策略实例表 */ }

impl<D: Driver, S: StrategySet> Sequencer<D, S> {
    /// cfg:EngineConfig(容量、每轮上限、环容量、K、oms/ledger/exec 段)。
    /// kill 字(`&'static [KillWord]`,每 gw 一个;模拟账户不经 gw)只交给 `LiveDriver`:Risk 只输出每 gw 的 `KillBits`,由驱动写原子字。
    /// hb:实盘为 HeartbeatBlock 区 0;其余驱动传进程内的哑 Heartbeat。
    pub fn boot(boot: BootSource, cfg: &EngineConfig, drv: D, hb: &'static hft_rt::Heartbeat) -> Result<Self, BootError>;
    /// 主循环一轮。非阻塞:按"主循环"的优先级与让位规则**逐条**取输入并立即处理,有每轮上限;没有输入立即返回 Idle;
    /// Busy = 这一轮处理了至少一条 Input(关键帧随它的 `Timer{Sys, Snapshot}` 在同一事件内一次写完)。
    pub fn step(&mut self) -> StepOutcome;            // Busy | Idle | Done { restart: bool, cause: DoneCause } | Fatal(FatalReason)
    /// 驱动在取输入前读取的只读视图(确定性状态派生的请求,不含环境信息)。
    pub fn view(&self) -> DriverView<'_>;             // next_deadline()、wants_continue()、notify_headroom()、snapshot_requested()、
                                                      // next_funding_settle()、draining()、refdata_recheck_due()、stop_engine_ns()、engine_epoch()
}

pub enum BootSource { Journal(RecoverPlan), Empty { configs: ConfigSet } }
pub struct RecoverPlan {
    pub start: SnapshotSel,                     // Latest | DrainTail | AtSeq(u64) | NewestReadable
    pub replay_tail: bool,                      // false = 不重放快照之后的 Input(按 tail_lost 处理)
    pub epoch: u32,                             // hft-core DurableMeta 中已持久化的 engine_epoch(新值的取法见"冷启动恢复"第 5 步)
    pub flags: RecoveryFlags,                   // TAIL_LOST | POISON | FROM_SNAPSHOT(RECOVER_MISMATCH 由恢复过程自己置)
    pub quarantine: FixedVec<InputHash, 4>,     // 运行期毒丸输入(见"毒丸输入隔离";恢复期崩溃由 hft-core boot_guard 处理)
}
/// 退出码映射归 apps/hft-core。
pub enum BootError {
    VersionNeedsDrain,                          // 版本不同且 journal 末尾不是 Drain 快照
    SnapshotUndecodable { section: u16 },       // 快照节不可解,含节 5 的 desc_version 不被策略 restore 接受(RestoreError::VersionUnsupported)
    CapacityShrink { what: CapKind },           // 恢复后的占用超过新 EngineConfig 的容量(缩容流程归 apps/hft-core);
                                                // CapKind = Insts | Cells | Instances | GroupsPerInstance | Timers(按此顺序检查,报第一项)
    TimerCapacity { demand: u32 },              // max_timers ≤ engine_timer_demand(见"定时器"的容量)
    Journal(JournalErr), Config(ConfigErr),
}
pub enum DoneCause { Drain, ReplaceRefdata, StorageCritical, IdentityMismatch }   // 各原因对应的退出码与是否重启归 apps/hft-core;Drain 超时不改原因
pub enum FatalReason { JournalFull, ClockRange, TimerOverflow, OmsCash(hft_oms::CashError), Settlement(hft_ledger::SettlementError), UnsupportedExternalCash, ReconcileEvidence }   // ClockRange:engine_ns 换算的取值范围被突破(见"引擎时间");TimerOverflow:引擎登记方的定时器使堆满(见"定时器");hft-core abort;ReconcileEvidence 只保留给引擎内部不变量(当前 sequencer 不再因 ReconcileBatch 的输入内容产生它,见"每条 Input 的处理顺序"末段)

pub trait Driver {
    type J: JournalSink;
    /// 每轮开始时调用一次:实盘决定这一轮的驱动生成 Input(定时器、续推、关键帧、环境决定,见"主循环")。
    fn begin_round(&mut self, v: &DriverView<'_>);
    /// 交出这一轮下一条要处理的 Input,写进 `out`;这一轮已无可取的输入 → false。
    /// 实盘按"主循环"的优先级与让位规则从环中取,**取出即交出**(不先攒批;只有 BBO 合并时一次取 ≤ 32 条);
    /// 交出每条环输入之前比较 `v.next_deadline()` 与当前 TSC,已到期即结束这一轮(下一轮先交到期定时器);
    /// `out.tsc` = 记录头 tsc:实盘 = 交出该条时的 tsc_now()(即 `proc_tsc`);其余驱动 = 日志值,按 journal 顺序交出,每轮 ≤ `replay_per_round` 条。
    fn take(&mut self, v: &DriverView<'_>, out: &mut InputSlot) -> bool;
    /// 本次运行的 TscClock 读数:实盘与恢复驱动 = tsc_now();验证与镜像驱动 = None(它们从不执行恢复结束)。
    fn run_tsc(&mut self) -> Option<Tsc>;
    fn measure(&mut self) -> Tsc;                     // 测量用 TSC(decided_tsc);不进入任何决策
    /// 出环:Out(OutRing, &GwCmd) | FeedCtl(venue, &FeedCmd) | Chain(&SwapDecision)。OutRing = Venue(v) | Sim。
    /// 恢复/验证/镜像驱动不出环。失败以合成 Input 回灌(见"出环满")。
    fn emit(&mut self, o: &OutRecord<'_>) -> EmitResult;
    /// 取已解析的大对象(StrategyParams、RiskParams、refdata):实盘从 ctl-gateway 的 Mailbox 取并核对 sha256;其余驱动冷路径解析日志里的 ConfigBlob(同一纯函数)。
    fn take_parsed(&mut self, r: &BlobRef) -> Result<ParsedConfig<S>, ConfigErr>;
    fn retire(&mut self, old: ParsedConfig<S>);   // 实盘经反向 Mailbox 交还 ctl-gateway 释放(不发唤醒,ctl-gateway 按 park 超时轮询);其余驱动直接 drop
    fn journal(&mut self) -> &mut Self::J;
}

pub trait JournalSink {
    fn append<B: Body>(&mut self, flags: RecordFlags, tsc: Tsc, body: &B) -> Result<Seq, AppendErr>;   // seq 的唯一来源是写者
    fn append_var(&mut self, kind: EventKind, flags: RecordFlags, tsc: Tsc, pieces: &[&[u8]]) -> Result<Seq, AppendErr>;
    fn end_event(&mut self);                          // 在最近一条已追加、尚未 publish 的非 Meta 记录上置 EVENT_END
    fn publish(&mut self);
    fn roll(&mut self, reason: RollReason, info: &RollInfo) -> Result<BootWriter<'_>, RollErr>;
}
/// ctl-gateway 解析好的大对象(类型归本 crate;hft-ctl 依赖它)。经 Mailbox 交给 sequencer,旧对象经反向 Mailbox 交还释放。
pub enum ParsedConfig<S: StrategySet> {
    StrategyParams { hdr: StrategyParamsHdr, groups: GroupTable, params: S::Params,
                     model_mem: u64,              // S::model_mem_bytes(&params, n_groups)(SDK 声明的 Model 内存上界)
                     model_ckpt: u64,             // S::model_snapshot_bytes(&params, n_groups)(SDK 声明的检查点上界)
                     structural_changed: bool,    // 与当前生效参数相比改动了 SDK `FieldDesc.structural` 字段(比较规则归 SDK)
                     reset_preview: ResetPreview, // S::reset_preview(当前, 新)(SDK);PREVIEW 时填进 CmdPreview.reset_groups / no_open_ms
                     sha256: [u8; 32] },
    RiskParams { p: Box<RiskParams>, sha256: [u8; 32] },
    Refdata { load: Result<Box<RefData>, LoadErrCode>, sha256: [u8; 32] },   // RefData::load(新 blob, 当前):含跨版本兼容校验;重启时才加载生效
}

pub struct LiveDriver    { /* hft-rt 环、ByteConsumer(ctl、advice)、Mailbox、TscClock、环境决定器(含关键帧计时)、隔离集合 */ }
pub struct RecoverDriver { /* 冷启动:读旧 journal 的 Input 直到最后一个 EVENT_END;输出与日志比较;不出环;持有 TscClock 只为 run_tsc */ }
pub struct VerifyDriver  { /* 离线(夜跑/CI):读 journal 的 Input,输出与日志 Output 逐字节比对,不一致 = 失败 */ }
pub struct MirrorDriver  { /* advisor 线程:尾随 seq journal 的 Input,按日志对齐 Output 序号但不比对、不出环、不写 journal */ }
```

`JournalSink` 由 `hft-journal::JournalWriter` 实现(实盘、恢复),验证与镜像驱动实现为按日志对齐序号的哑写者;`EVENT_END` 位与 Input/Output/Meta 分类属于 [hft-event](../hft-event/README.md) 的 `RecordFlags`/`KindInfo.role`。

测试用 `MemJournal::new` 按逻辑记录编号;`with_physical_seq` 在 append 返回前按 `PartIter` 预留全部物理片序号,逻辑体仍完整保存。`MemSegment.part_ends` 与 `Log.part_ends` 保存首片及末片序号,引导块计数包含每一片,滚段 Meta 与真实写者一致。物理模式的独立 Meta 自带 EVENT_END 并可发布,引导块和开放事件内的 Meta 不提前结束事件。崩溃截尾同步清除分片索引,重算保留记录数,从最后保留记录的末片之后续写;调用方先裁到发布边界也适用。此模式只用于测试,不改变生产写者、文件格式和快照格式;`committed_len` 仍是内存记录长度的代理值,不可当作文件字节数。

**离线文件输入**由默认关闭的 `replay-io` feature 提供 `Log::from_reader(&mut JournalReader, LogReadLimits)`。完整文件验证使用 [JournalReader::open_files](../hft-journal/README.md#读者) 预检的固定边界与原路径清单;普通读者须配置 LastCompleteEvent、段首起点、Error-on-gap 和非零重组容量,其恢复截断点本身不证明文件完整。记录数与逻辑体字节数上限分别由 `LogReadLimits.records/body_bytes` 给出。超过上限、Pending、空输入、损坏、缺段、未知 kind 或段头 schema 与当前二进制不符均返回 `LogReadError`,不返回成功的部分日志。物理 seq 与跨片记录的末片 seq、事件标志、多段重组后的业务尾零及各段时钟保持原样;Fixed/Var 经 [exact_body](../hft-event/README.md#体-trait-与编解码) 去除传输填充。读取不打开写者、不修改来源文件,只在冷调用方执行,不进入 LiveDriver 的取环路径。

同一 feature 的 `driver::replay_snapshot::check_snapshot::<S>(&Log, seq)` 在投递输入前自检指定快照。先校验头部、全部节的边界、唯一性与节数,再计算 [state_hash](#状态哈希);非零声明值必须相等,零值仍需自算。装配参数只取该日志,调用正式恢复路径后重新序列化,重算哈希必须一致;差异返回首个不同节号。重建写缓冲上限为原节字节数,超限直接失败。缺快照、损坏、不支持的恢复布局与恢复后丢失状态均失败,不退到另一快照,不修改来源日志或正在运行的引擎。

`driver::snapshot_diff::snapshot_difference(logged, regenerated)` 只在 `replay-io` 中提供,输入是 ReplaySink 已屏蔽测量字段的逻辑快照体。它不参与通过/失败判定,逐字节比较仍是依据。节索引与字段名格式化按文件登记在 tools/xtask/allow-list.toml 的冷回放例外,默认 live 构建不包含此模块。节边界、唯一性与总数须完整合法;错误返回哪一侧、逻辑体偏移和原因。仅按实际读到的节分配索引,不按未经校验的 n_sections 预分配;返回固定六个组件的 xxh3-64、两侧 state_hash 及首个字段差异,不复制整份快照。

组件分组与优先级沿用 [testkit HashTrail](../hft-testkit/README.md#7-状态哈希对比),组件内部仍逐字节比较,哈希碰撞不能隐藏差异。先定位六个组件,再定位 sequencer 自有节和节间顺序,最后检查快照头;节 16 不归入状态组件,未知节不猜归属。当前 RiskState 行版本按 CapRecord 的生成字段表定位行号、嵌套字段、两侧逻辑体偏移及字段原始字节;节头按 SectionHdr 字段表定位。其他行只给节体字节偏移,未知版本标 unknown_row_version,行数量与长度不符标 invalid_row_layout。只保留第一个字段,后续原始差异仍由 ReplaySink 的有界明细保存。该定位证明的是已比较快照处的差异,不推断更早某条 Input 导致分歧。

`driver::verify::verify::<S>(Log, VerifyOptions, &'static Heartbeat)` 装配离线验证。未指定 from_seq 时取首段引导快照(含 Recovery),否则取不晚于该 seq 的最近快照;先自检,再由日志配置装配。普通滚段沿用当前引擎;Startup、Recovery、RecoverFromSnapshot 或 HostProfile.build_git 变化开始独立运行,重新自检和恢复该段引导快照,不沿用上一运行的内存状态。每段须有 Begin、单份 HostProfile、单份用途与原因一致的引导快照及 BootDone;独立运行的首个普通 Input 必须是 EngineClock,损坏边界返回 RunBoundary。`Log.recovery_gaps` 保留段头标记及 Begin 索引、段号、首 seq、前段末 seq,数量受记录数上界约束;序号连续及只给后缀时也保留。标记必须对应 Recovery/RecoverFromSnapshot,前段在输入中时必须匹配其物理末 seq;重复、错位、缺标记的跳号均拒绝。冷 Model 重算在这些边界清除样本来源和历史 Advice 配对,重新要求缺口后的合法起点;检查点正好在前段 End 仍属于缺口前,不能续接。窗口外输入仍用于预热,窗口内 Output 按 seq 闭区间比较;to_seq 落在事件中间时处理完该事件,不比较其后输出。

多生成的 Output 不消费下一个 Input 或下一独立运行的记录;未生成的输出包括运行尾部输出均计分歧。独立运行的 BootDone 至首条 EngineClock 之间不接受普通 Output,不允许在快照起播时跳过。默认首次分歧后停止取输入,keep_going 保留完整输入顺序;Fatal、未消费完的输入和诊断容量不足均不能返回通过。快照的两侧 state_hash 均独立重算,非零声明错误计分歧;零声明不替代状态检查。结果含各运行的起始快照、下一运行边界、输入/Output/快照/分歧计数,以及汇总和分歧体、语义标志;诊断容量以日志记录数为界。LiveDriver 与恢复装配流程不读取这些离线选项。

`replay-io` 的冷规划器校验 RecoverMismatch 注记及其引用的旧尾部,按 [replayer verify](../../apps/replayer/README.md#verify) 的范围比较并单列 `recovery_diffs`;恢复后的普通输出不放行。仅经过验证的启动注记可在从新引导快照起步时略过,它们不由该快照重新生成。非法证据返回 `RecoveryRange`,普通与恢复差异共用诊断容量,溢出失败。默认构建不包含范围规划和恢复明细存储。

同一 feature 的 `verify_with_clock` 可注入仅供诊断的纳秒时钟,从 Input 入 journal 到决定测量点计时,对窗口内每条 OrderDecision 留一个样本;样本容量在启动前按记录数预分配。时钟不供应驱动 TSC,不进入决策、快照或比较字节。统计与 JSON 归 replayer,TOOL-045 验证启用前后的比较结果相同。

同一 feature 的 `driver::recompute` 以独立镜像执行 Model,起点、检查点身份、比较窗口与路径重建点遵循 [replayer verify](../../apps/replayer/README.md#verify)。每次独立运行从其引导快照重建引擎。Model 只在已有合法起点、前缀连续且实例名/编号/账户/种类/参数引用均一致时续用已重算状态;不满足则重新寻找起点,无起点逐名报告。窗口前的检查点与路径重置注记参与预热;原始产出字节在连续前缀内保留,每个运行各自只匹配一次;重启追赶重发旧 basis 的 Advice 仍验证原字节,同一运行内重复记录拒绝。`StrategySet::has_model` 只排除实际 NoModel,空布局和零资源声明仍可带 Model。同一五元键的重复产出只保留最后一条参与配对,覆盖量计入 coalesced;每条产出只匹配一次。诊断记录数、待配对/历史产出与差异载荷合计字节数由 `RecomputeLimits` 限制,溢出返回错误。文件与 JSON 仍归 app;此入口只用于冷回放,不改主循环、journal schema 或快照格式。

回放比较器要求屏蔽测量字段后的逻辑体**长度和内容都相等**;不删体尾零。传输标志按 `NONCMP_FLAGS` 屏蔽,SHADOW 等业务标志仍参与比较。

离线 Model 镜像同样保留原始 Input 顺序。被篡改的 Advice 引出额外 Output(例如错路由注记)时,该输出不能占用下一条 Input 的位置;普通 Output 分歧仍由独立 VerifyDriver 报告。此选项仅由冷重算入口开启,在线 advisor 与恢复驱动沿用各自装配。

恢复、验证与镜像驱动解析配置时,只查当前已消费日志前缀里的 `ConfigBlob`;预读整份文件或追加镜像批次不提前放行配置。引用的 kind、encoding、len、version 与 SHA-256 必须全部相同,内容长度足够且摘要相等后才解码。向前或向后选择回放起点都按该位置重新限定可见范围;同一引用的最近记录损坏直接失败,不退回较早的副本。

### 策略集(`StrategySet`)

`StrategySet` trait、`strategy_set!` 宏、`Strategy`/`Model`/`ParamSet` 的签名归 [hft-strategy-sdk](../hft-strategy-sdk/README.md);本 crate 只经下列入口使用策略(全部静态分派,无 `dyn`):

| 用途 | 入口(签名以 SDK 为准) | 线程 |
|---|---|---|
| 参数解码与校验 | `S::decode_params(kind_id, body, schema_ver, &groups)` + `S::validate_params(&params, &groups, rd)` + `S::model_mem_bytes(&params, n_groups)` + `S::model_snapshot_bytes(&params, n_groups)`;结构字段是否改动按 SDK 规则比较新旧参数 | ctl-gateway(warm),结果经 Mailbox 交给 sequencer |
| 描述符 | `S::kinds()`(`StrategyDesc`:`version` 写进快照行头、`non_prod_only` 绑定检查) | sequencer(命令路径、启动) |
| 创建实例 | `S::create(StrategyInstance, &params, &groups, rd)`(实例的 `kind_id` 由 refdata `InstanceMeta` 给出) | sequencer(命令路径) |
| 订阅 | `subscriptions(&groups, &mut SubsBuilder)`;`SubsBuilder::market` 返回 `SubsErr::Full` 的条件是本 crate 的 `max_subs_per_inst` 与 `max_insts`(ECF1 段 3),判定与 SDK 的 `SubsErr` 注释一致,不使 `SetParams` 失败 | sequencer(命令路径) |
| 事件 | `on_event(&EventRef, &SeqViews, &mut Ctx)` | sequencer(热路径) |
| 参数替换 | `on_params(&params, &groups, rd)`;group 指纹 `GroupSpec::fingerprint()` | sequencer(命令路径) |
| 快照 | `snapshot` / `restore`(节 5 的实例状态字节) | sequencer |
| 慢速部分 | `S::create_model`、`S::model_on_event(&EventRef, &MirrorViews, &mut AdviceOut)`、`S::model_on_params`(返回 `ModelParamEffect`)、`S::model_snapshot_chunk`/`model_restore` | advisor |
| 策略命令与预热注入 | `on_event(EventRef::Manual)`(`StrategyCmd`);`on_event(EventRef::Warmup)`(`InjectWarmup`,策略声明 `Part::Both` 时)与 `S::model_on_event(EventRef::Warmup)` | sequencer / advisor |

实例表:`instances: FixedVec<InstanceSlot<S>, max_instances>`,键 `StrategyId`(= `StrategyInstanceId`,同一 u8 类型,由 refdata 的 `Instances` 段分配);每个实例绑定且只绑定一个账户(`AccountMeta.bound_instance`),`kind_id` 由 refdata `InstanceMeta` 给出、进程生命周期内不变;实例在其第一份 `StrategyParams` 生效时创建。实例按所绑定账户的类型分成**实盘实例**(`AccountKind::Live`)与**模拟实例**(`Sim`);启动时生成固定的派发顺序 `dispatch_order`(先全部实盘实例、后全部模拟实例,各自按 `StrategyId` 升序),refdata 不热替换,所以它在进程生命周期内不变。每个实例另记每个 group 的 `spec_seq`(见 [Advice 路由](#advice-路由))与当前参数的 `model_mem`、`model_ckpt`。

### 只读视图(`SeqViews`,实现 SDK `Views`)

| 方法 | 数据来源 |
|---|---|
| `now()` | `Now{tsc, engine_ns, utc_ns, seq, boot_seq, tsc_per_ms, clock_ok}`:`tsc`/`seq` = 当前 Input 的记录头;`engine_ns` 按当前运行的 `EngineClock` 基准换算(见[引擎时间](#引擎时间engine_ns));`boot_seq` = 本次运行 `Note{EpochStart}` 的 seq(处理 `EngineClock` 时写出;回放读同一记录,确定),用于不跨启动比较 TSC;`tsc_per_ms` 与 `utc_ns` 由当前段头的 `tsc_hz_milli` 与校准对(`calib_tsc`、`calib_realtime_ns`)换算——段头在 journal 里、所有驱动读同一值,所以确定;`clock_ok` = 最近一条 `Timer{ClockOk\|ClockBad}`。不读系统时钟,也不读 Meta 的 `ClockStatus` |
| `bbo(i)`(**按值**) | 报价取 [hft-book](../hft-book/README.md) 的 `Top`;`rx_tsc` 在 `Top.seq > rx_from_seq` 时取 `Top.rx_tsc`,否则 0(`rx_from_seq` = max(`now.boot_seq`, 本次运行最近一个快照的 `at_seq`):hft-book 的快照行不含 `Top.rx_tsc`、从快照恢复后它为 0,所以早于最近快照的接收时刻对策略一律不可见,实盘与从该快照起步的回放看到同一值);`hw_rx_ns` 取本 crate `MarketLast` 的 `bbo_hw_rx_ns`(PHC 域,软件戳回退由 `bbo_flags.SW_TS` 标明,口径归 [event-model](../../docs/subsystems/event-model.md));连接健康按该 inst 的 BBO 成员连接聚合(见[连接活性](#连接活性)) |
| `book` | hft-book 的簿 |
| `funding`、`mark` | 本 crate 的 `MarketLast`(实时费率、下次结算时刻、实际结算周期 `interval_s` 与上一次不同的周期 `prev_interval_s`,都在节 10);`FundingView.upd_tsc` = 最近一条被应用的 `Funding` 的 `MarketHdr.rx_tsc`,按与 `bbo(i).rx_tsc` 相同的 `rx_from_seq` 规则可见(该 tsc 与其 seq 只在内存、不进快照) |
| `portfolio(a)` | [hft-ledger](../hft-ledger/README.md) 的 `VenueBook`/`Equity` + OMS 在途量;**不提供任何资金公式** |
| `group_pos`、`legs` | ledger 的 group 账;[hft-oms](../hft-oms/README.md) 的 LegTracker |
| `target(a, g)` | [hft-exec](../hft-exec/README.md) 的当前目标与进度 |
| `order(h)` | OMS 槽视图 |
| `inst`、`caps(v)`、`fee(a, i)` | [hft-refdata](../hft-refdata/README.md) 的 `InstMeta`、`VenueCaps`(按 `VenueId` 一行,`VenueId` 即产品线)、`FeeSchedule` |
| `leverage(a, i)` | hft-risk 杠杆表(`Risk::leverage`,`Rate` 倍数)换成百分之一倍、**向下取整**(OMS `lev_x100` 用同一换算);未知 → `None` |
| `mode(a, g)` | `risk.mode.effective(cell)` |
| `risk(a)` | `risk.view(a, &now)`:账户级字段——生效 cap、HARD、保证金档位、`liq_dist` 与截止、清仓锁、`max_hedge_notional`、`max_reduce_notional`、`max_order_notional`、`conn_liveness_ms`、`dust_notional`(取自 ExecConfig,hft-risk 已持有同一值) |
| `risk_for(a, i)` | `risk.view_for(a, i, &now)` → SDK `InstRiskView`(字段以 SDK 为准):该 inst 所属价格带类别的 `max_px_dev`、`ioc_max_cross`、`rr_max_cross` 与 `RiskParams.l3_max_cross`(取值规则归 [hft-risk](../hft-risk/README.md));`quote_rate` 取 ledger 换算率(与检查链 #6 同一口径),`rate_backoff` 取 hft-risk 该 (venue, account) 令牌桶的退避状态 |
| `venue_ready(a, v)` | `oms.venue_ready(v, a)`(无 Unknown 槽且下单连接 Up) |

传给 `exec.advance` 的适配器不含执行层自身(`target()` 返回 `None`,避免借用重叠)。advisor 镜像上的同一结构实现 `ModelViews`(按 SDK 规定 `target()`/`order()` 在模型侧返回 `None`)。`portfolio(a).margin(v).stale` 由本适配器按 hft-risk 的 `margin_stale_ms` 判定。传给 `risk.evaluate` 的 `RiskView` 适配器(`src/views.rs`)的 `bbo(i)` 与 `Views::bbo` 是同一投影(报价健康的唯一判定 `BboView::quote_live` 归 SDK),并实现第二腿可负担性的两个读数:`avail_quote` = ledger `CashView.free` − OMS `InflightBook` 在该 (account, venue) 的在途买单名义,`avail_margin` = ledger `MarginState.rep_avail` − OMS `InflightBook.open_margin(a, v)`(增量维护,O(1);定义归 [hft-risk](../hft-risk/README.md));`open_qty`/`pending_reduce` 另有只计即时在途的读数 `open_qty_immediate`/`pending_reduce_immediate`(OMS `InflightBook` 按即时/挂单两类维护;哪类订单用哪种读数归 hft-risk 检查链 #7)。

### 环与容量

`Capacities` 与每轮上限来自 `EngineConfig` blob(启动时一次分配,运行期不再增长)。ECF1 段 3 `capacities` 与段 5 `sequencer` 的**字段与语义**由本 crate 定义;**各主机类的取值唯一归属 [apps/hft-core 容量表](../../apps/hft-core/README.md#内存与-journal-段按主机类唯一归属)**,本页与其他 crate 不重复容量数字(段 5 的默认值与主机类无关,在本表):

| 段 | 字段 | 类型 | 语义 |
|---|---|---|---|
| 3 | `max_insts` | u32 | 本实例订阅的 inst 稠密表容量(CSR 行数、`MarketLast` 行数);订阅 inst 在首次订阅时分到稠密下标,进程生命周期内不回收;≤ refdata `max_universe_insts`(启动校验);超出 → `SetParams` 的 `CapacityExceeded` |
| 3 | `max_cells_total` | u32 | 本实例全部 (account, group) cell 总数:hft-exec 目标单元、OMS/ledger 的 group 表、hft-risk `ModeOutcome` 各向量与 `RiskParams.groups`、本页 `NotifyBacklog` 都按它预分配 |
| 3 | `max_subs_per_inst` | u16 | 每个 inst 被引用的 cell 数上限:本页 CSR 行宽、hft-exec 与 hft-ledger 的 inst 反向索引同用这一个值(`SetParams` 超出 → `CapacityExceeded`) |
| 3 | `max_groups_per_instance` | u16 | 单个策略实例的 group 上限(名字与语义归 [hft-strategy-sdk](../hft-strategy-sdk/README.md)) |
| 3 | `max_timers` | u32 | `TimerHeap` 容量 |
| 3 | `recent_cmd_ids` | u32 | 命令去重表容量(节 15) |
| 3 | `max_instances` | u16 | 本实例的策略实例数(含模拟实例) |
| 5 | `rings` | `[RingLimit; 8]` | 每类环每轮条数上限,下标 = `RingClass`(u8):`Ctl = 1`、`Hk = 2`、`Rep = 3`、`Chain = 4`、`Bbo = 5`、`Advice = 6`;0 与 7 保留(rep-sim 只用 `sim_max_per_round`);`RingLimit { per_ring, total }` = 每环 / 合计;默认 ctl 4/4、hk 4/4、rep 32/64、bbo 32/128、advice 16/16;chain-in 与 rep 在主循环 c 段共用 `Rep.total`,`Chain` 只用 `per_ring`(默认 32);md 不按条数计(见下一行) |
| 5 | `md_round_ns` | u32 | 每轮 md 的时间片,默认 50 000 ns(自这一轮首条 md 被交出起计);这是 [hot-path-rules T10](../../docs/hot-path-rules.md#线程t) "每轮每环至多 K 条"在 md 上的形式:按时间片计,不按条数 |
| 5 | `sim_max_per_round` | u16 | rep-sim 环每轮取出条数上限,默认 32 |
| 5 | `ctl_bytes_per_round` | u32 | 每轮 ctl 记录载荷字节上限(预热块 4 KiB/条),默认 8 192;至少取一条 |
| 5 | `conflate_backlog` | u16 | bbo 环积压超过它时启用 BBO 合并,默认 64 |
| 5 | `notify_per_continue` | u16 | 每个 `Timer{Sys, NotifyContinue}` 投递的积压通知条数上限,默认 64(见[通知队列与续推](#通知队列与续推)) |
| 5 | `keyframe_period_ms` | u32 | 关键帧周期:`Timer{Sys, Snapshot}` 的时间触发与各簿 `Timer{Keyframe, inst}` 的均摊窗口,默认 300 000(见[快照](#快照)) |
| 5 | `keyframe_max_events` | u32 | 关键帧的事件数触发:距上次关键帧的 Input 数达到它时同样到期,默认 1 500 000(触发与最大推迟见[环境决定写成 Input](#环境决定写成-input)) |
| 5 | `replay_per_round` | u16 | 恢复/验证/镜像驱动每轮交出的 Input 数(只为按轮打心跳),默认 1 024 |
| 5 | `advisor_mem_budget_mib` | u32 | 本实例全部策略实例的 Model 内存预算。**运行中的唯一来源**:`tools/hft-cfg` 由 hft-core 配置 `advisor.mem_budget_mib`(取值归 hft-core 内存表)写入;`SetParams` 的复核(本 crate)与 ctl-gateway 的单实例预检都读本字段;hft-core 装配时核对两者相等,不等退出 2(规则归 apps/hft-core) |
| 5 | `advisor_ckpt_max_mib` | u32 | 本实例全部模型检查点的磁盘上限(两代),= hft-core `advisor.checkpoint_max_mib`,写入与核对同上;`SetParams` 复核 2 × Σ `model_ckpt` ≤ 它 |
| 5 | `auto_resume_after_clean_drain` | bool | 默认 true;启动闸门放行时是否按规则自动解除 `Recovery`(干净 Drain 重启,以及只含 Sim 账户实例的非干净重启;语义归 [hft-risk](../hft-risk/README.md#干净-drain-重启自动解除-recovery)) |
| 5 | `allow_missing_standby` | bool | = hft-core `startup.allow_missing_standby`(hft-cfg 写入);启动闸门 `ORDER_CONNS` 是否不要求热备 Up(语义归 [hft-risk 启动闸门](../hft-risk/README.md#启动闸门)) |

- **两个 group 容量,不是同一个量**:`max_cells_total` 是本进程全部实例的 cell 总数;`max_groups_per_instance` 是每个实例的上限。`max_instances` 计入实盘与模拟实例(模拟实例池见 [hft-sim](../hft-sim/README.md))。
- **恢复缩容**:按 `Insts → Cells → Instances → GroupsPerInstance → Timers` 报第一项 `CapacityShrink`,组件恢复暂用足以检查快照占用的容量,不能把缩容误报成快照损坏。成功恢复按新容量运行。定时器同时满足堆占用和 `engine_timer_demand + 已登记策略定时器数` 的容量,策略不能借用尚未登记的引擎预留量;没有策略定时器时仍要求 `max_timers > engine_timer_demand`。
- **容量配置的恢复边界**:新容量与快照引用的 ECF1 段 3 不同时,恢复引导块写版本加一的 Engine ConfigBlob 并更新快照引用,其余 ECF1 节原样保留。版本溢出拒绝启动;容量相同不改配置身份。独立回放由该引用还原新限制,不依赖启动进程的内存。

`Draining` 只约束发起 Drain 的运行。冷恢复结束时,先为全部账户设置 `Recovery` 并复位启动闸门,清空旧运行的 Drain 执行状态,再经 `ModeInput::AutoClear` 移除进程级 `Draining`,结果写入 Recovery 快照;`Kill` 等其他 cap 保留。尾部重放产生的 active、起因、超时起点和复核进度也随旧运行结束,新 Drain 从自己的命令重新开始(SEQ-421)。新运行仍须完成启动闸门和 Recovery 的解除条件;读取快照的验证/镜像驱动不执行该转换。
- **段 3 的行格式**(草案,实现时以代码与编译期断言为准;行版本 1):段 3 只有一行 `CapacitiesRow`(ECF1 段头的 `row_size` = 24),字段语义见上表,只在尾部追加:

```rust
#[repr(C)] pub struct CapacitiesRow {                                                         // 24 B
    pub max_insts: u32, pub max_cells_total: u32, pub max_timers: u32, pub recent_cmd_ids: u32, //  0
    pub max_subs_per_inst: u16, pub max_groups_per_instance: u16, pub max_instances: u16,      // 16
    pub _pad: u16,                                                                              // 22 为 0
}
```
- **段 5 的行格式**(草案,实现时以代码与编译期断言为准;行版本 1):段 5 只有一行 `SeqParamsRow`(ECF1 段头的 `row_size` = 72),字段语义见上表,只在尾部追加:

```rust
#[repr(C)] pub struct RingLimit { pub per_ring: u16, pub total: u16 }                         // 4 B
#[repr(C)] pub struct SeqParamsRow {                                                           // 72 B
    pub rings: [RingLimit; 8],                                                                 //  0 下标 = RingClass
    pub md_round_ns: u32, pub ctl_bytes_per_round: u32,                                        // 32
    pub keyframe_period_ms: u32, pub keyframe_max_events: u32,                                 // 40
    pub advisor_mem_budget_mib: u32, pub advisor_ckpt_max_mib: u32,                            // 48
    pub sim_max_per_round: u16, pub conflate_backlog: u16,                                     // 56
    pub notify_per_continue: u16, pub replay_per_round: u16,                                   // 60
    pub auto_resume_after_clean_drain: u8, pub allow_missing_standby: u8, pub _pad: [u8; 6],   // 64 bool 存 0/1,_pad 为 0
}
```
- **段 3、段 5 的语义校验**(本 crate 提供的 `EngineValidator`,由 hft-refdata `EngineConfig::load` 调用;容器与 `row_size` 校验归 [hft-refdata](../hft-refdata/README.md)):段 5 恰好一行,否则 `LoadErrCode::Structure`;以下任一项 → `LoadErrCode::ValueRange`:bool 字节不是 0/1、`_pad` 非零、保留下标 0 与 7 的 `RingLimit` 非零、已用下标(1–6)的 `per_ring = 0` 或(`Chain` 以外)`per_ring > total`、`md_round_ns`/`ctl_bytes_per_round`/`sim_max_per_round`/`notify_per_continue`/`keyframe_period_ms`/`keyframe_max_events`/`replay_per_round` 为 0;段 3 任一容量为 0,或 `max_insts` > refdata `max_universe_insts`。
- **缩容**:容量只在启动时生效。缩小只经 Drain + 新 EngineConfig 重启;`boot` 恢复快照后发现当前占用(订阅 inst、cell、策略实例、每实例 group 数、定时器)超过新容量 → `BootError::CapacityShrink`(hft-core 退出 2,流程归 [apps/hft-core](../../apps/hft-core/README.md));不超过则按新容量分配并继续。

每事件出环上限 K 在 OMS 段(`max_out_per_event`,[hft-oms](../hft-oms/README.md));各环容量的唯一权威值在 ECF1 段 4 `rings`([apps/hft-core](../../apps/hft-core/README.md));超出容量的 `SetParams` 被拒绝(`CapacityExceeded`)。refdata 不热替换:`ReplaceRefdata` = 复核后写 `ConfigBlob` 再 `Drain`,由 hft-core 重启加载。

| 环 | 方向 | 元素 | 优先级 |
|---|---|---|---|
| `ctl` ×1 | ctl-gateway → sequencer | `CtlRecord`(`ControlCmd`、`ConfigBlob` 分段、`ReconcileReport`、审计 `Note{CtlAudit}`) | 1 |
| `hk` ×1 | housekeeping → sequencer(唯一生产者是 housekeeping 线程;启动路径的 `Note{Boot}` 在 housekeeping 线程启动之前写完,不并发) | `HkMsg`:`ClockStatus`(写入周期归 apps/hft-core housekeeping)、存储状态、`Note{Housekeeping}`、`Note{Boot}`(Meta) | 1(同批) |
| `rep-<venue>` ×M | gw → sequencer | `RepMsg`:`OrderEvent`、`SendReport`、`PositionReport`、`BalanceReport`、`FundingSettle`、`ExternalFill`、`MarginReport(hdr)`(条目在配套的 `rep-<venue>-bulk` 字节环)、`AccountConfigReport`、`ConnState` | 2 |
| `chain-in` ×0–1 | `ext-<chain>` → sequencer(optional) | `PoolQuote`、`TxLifecycle`、`ChainHead` | 2 |
| `bbo-<venue>` ×N | feed-bbo 任务(BBO 连接,`conn` = 槽号 0..`MAX_BBO_CONN_SLOTS`)→ sequencer | `FeedMsg::Bbo`、BBO 连接的 `ConnState` | 3 |
| `advice` ×1–2 | advisor → sequencer(`ByteSpsc`) | `AdviceMsg`:`AdviceHdr` + 不透明载荷(≤ 4096 B);advisor 的 Meta `Note{source = Advisor}`(`AdvisorLag`、`ModelGroupReset`、`AdvisorStats`)也经此环送达(记录形式归 hft-event `AdviceRecord`) | 4 |
| `md-<venue>` ×N | feed-md 任务(成交/标记价/资金费连接,订阅深度时另有分档快照流)→ sequencer | `FeedMsg`:`L2Snapshot{origin = TopN}`(分档快照流)、`L2Batch` 与 REST 快照 `L2Snapshot{origin = Exchange}` 分片(增量深度启用时;以上都经 `md-<venue>-bulk` 字节环)、`Trade`、`Funding`、`MarkPrice`、`IndexPrice`、`Liquidation`、其余 `ConnState` | 5 |
| `rep-sim` ×0–1 | sim-gw([hft-sim](../hft-sim/README.md),warm 线程)→ sequencer | 同 `rep-<venue>`(模拟账户的回报,同一组事件) | 6(最低;每轮 ≤ `sim_max_per_round`) |
| `out-<venue>` ×M | sequencer → gw | `OutMsg` = `GwCmd`(64 B):放行且非 SHADOW 的 `OrderDecision` 的投影;维护类 `DeadManRefresh`、`Rotate`、`ResetRate`、`Resubscribe`、`SetEpoch`、`SessionKeepalive`、`MirrorRestore` | — |
| `out-sim` ×0–1 | sequencer → sim-gw | 模拟账户的 `GwCmd`(同一类型)与 `SimReset`/`SimTransfer` | — |
| `feedctl-<venue>` ×N(BBO 拆成独立任务时另有 `feedctl-<venue>-bbo`) | sequencer → 行情任务 | `FeedCmd`(按连接与路由分发到 feed-bbo/feed-md 的规则归 [hft-venue](../hft-venue/README.md)) | — |
| `chain-out` ×0–1 | sequencer → `ext-<chain>`(optional) | `SwapDecision`、`BboSnapshot` | — |

环元素类型全部在 [hft-event 环元素类型](../hft-event/README.md#环元素类型);环本身来自 [hft-rt](../hft-rt/README.md)。sequencer 与交易所协议无关。optional 的 `chain-in`/`chain-out` 只在链上组件启用时存在,它们的容量行随该组件启用时加入 hft-core 的环容量表(段 4)。

### advisor

advisor 是 warm 任务(`hft_rt::PollTask`,由线程计划调度,[apps/hft-core](../../apps/hft-core/README.md#线程计划) 决定放 1 或 2 个,按 `StrategyId` 分片),承载策略的慢速部分 `Model`:

```rust
pub struct Advisor<S: StrategySet, C: CheckpointStore> {
    reader: JournalReader,                       // 尾随 seq journal(committed_len Acquire)
    mirror: Sequencer<MirrorDriver, S>,          // 镜像核:同一代码重放 Input,得到与 sequencer 同 seq 时刻的状态
    models: FixedVec<ModelSlot<S>, MAX_INSTANCES>, // 本任务负责的实例的 Model(容量 = Capacities.max_instances)
    outbox: AdviceOutbox,                        // 每 (strategy, account, group, local_kind) 最新一条待发
    ring: ByteProducer,                          // advice 环
    ckpt: C,                                     // 检查点文件 I/O(装配处实现)
    ckpt_job: Option<CkptJob>,                   // 进行中的分块检查点(实例、last_seq、下一块的 from_gix)
    lag: LagMeter,                               // 落后量测量(见"advisor 落后量")
}
impl<S: StrategySet, C: CheckpointStore> hft_rt::PollTask for Advisor<S, C> {
    fn poll_once(&mut self, now: Tsc) -> bool;   // 每次 ≤ 1 ms 工作量:一批记录(连同其 Model 调用),或检查点的一块;从不阻塞或睡眠
    fn next_deadline(&self) -> Option<Tsc>;
}
/// 检查点文件的 I/O 由 apps/hft-core 实现(文件名、`CkptHdr` 格式、目录、`.tmp` → fsync → 原子 rename、磁盘上限);
/// 本 crate 只产出 Model 字节与键、按键校验,不做文件 I/O。
pub trait CheckpointStore {
    fn load(&mut self, s: StrategyId, out: &mut CkptBuf) -> Option<CkptHdr>;   // 启动时(冷路径)
    fn begin(&mut self, s: StrategyId, hdr: &CkptHdr) -> Result<(), CkptIoErr>;
    fn write_chunk(&mut self, bytes: &[u8]) -> Result<(), CkptIoErr>;           // 每次 poll_once 至多一块
    fn commit(&mut self) -> Result<(), CkptIoErr>;                               // 失败(含超出磁盘上限)→ 本代作废,下周期重试
}
```

## 实现说明

<details><summary>实现要点</summary>

### 记录分类

| 类 | kind | 规则 |
|---|---|---|
| Input | 行情(`Bbo`、`L2Batch`、`L2Snapshot{origin = TopN \| Exchange}`、`Trade`、`Funding`、`MarkPrice`…)、`OrderEvent`、`SendReport`、`PositionReport`、`BalanceReport`、`FundingSettle`(交易所推送)、`ExternalFill`、`MarginReport`、`AccountConfigReport`、`ControlCmd`、`ConfigBlob`(ctl 环到达的)、`ReconcileReport`、`Timer`(含全部环境类)、`ConnState`、`Advice`;optional:`ChainHead`、`PoolQuote`、`TxLifecycle` | 决定状态;恢复/回放/镜像喂给 sequencer |
| Output | `OrderDecision`、`ExecDecision`、`FundingSettle{synth = 1}`、`CmdResult`、`StateSnapshot`、引导块中的 `ConfigBlob{boot}`/`L2Snapshot{Keyframe}`、`Note`(source = Sequencer/Strategy/Risk/Oms/Ledger/Exec);optional:`SwapDecision` | 由 Input 确定地产生;验证回放逐字节比对 |
| Meta | `SegmentRoll`、`LatencySample`、`Note{Measure}`、`ClockStatus`、`HostProfile`、经输入环到达的 `Note{Venue/CtlAudit/Housekeeping/Advisor/Boot}`、`Note{InputQuarantined}` | 占 seq,**不影响状态、不参与比对**(引导块比对同样屏蔽 Meta);`OrderDecision.decided_tsc`、`SnapshotHdr` 的 `snap_tsc`/`boot_id`/`snap_utc_ns` 等测量字段按 hft-event `MEASURE_MASK` 屏蔽 |

分类表是 hft-event `KindInfo.role` 与 `MEASURE_MASK`。`SegmentRoll` 是结构记录(Meta),滚动决定本身是 Input `Timer{Sys, Roll}`。`ClockStatus` 是 Meta:它对决策状态的影响只经由实盘驱动写出的 `Timer{ClockOk|ClockBad}` Input 发生。记录成功后仅更新节 16 的冷读者校准行及 `from_seq`,不更新 `Now`、业务序号、Input 计数或策略回调。引导块在写出本段 ClockStatus 后采集快照;滚段未就绪时不改变校准行。

恢复与验证按日志顺序保留校准来源,不把 Meta 计入每轮 Input 配额或重放 Input 数;所选快照之后、首条 Input 之前的已消费前缀同样保留来源,未提交尾部按 `EVENT_END` 丢弃。Drain 返回 Done 后,完成检查只看是否仍有 Input;段尾 Meta 不构成未完成,未消费的 Output 仍须报告分歧。回放比对仅屏蔽节 16 的 40 B 行体,节头仍须合法且相同;其余快照字节和哈希声明照常检查。镜像可跳过 Meta,不影响业务状态。

### 主循环(`step`)

```
step():
  heartbeat.beat()
  drv.begin_round(self.view())
  while drv.take(self.view(), &mut slot):     // 交出即处理;交出顺序就是 journal 顺序
      process(slot)                            // Timer{Sys, Snapshot} 的处理就是一次写完关键帧(见"快照")
      该 Input 使 Drain 完成 → journal.publish()(该事件含 Drain 快照,已完整)后返回 Done
  journal.publish()(若尚有未 publish 的完整事件)
  这一轮没有 Input → 立即返回 Idle(让出由线程的 IdlePolicy 决定,见 hft-core 线程计划)
```

实盘驱动 `take` 的交出顺序(每次调用交出一条):

```
a. 驱动生成的 Input(begin_round 决定):进程启动后的首轮只交 EngineClock(见"引擎时间";有停机标记时紧接一条
   cli:systemd 的 Kill,见"命令路径");关键帧可写(条件见下表 Sys{Snapshot})时,这一轮只交 Timer{Sys, Snapshot}(见"快照");
   背压时只交 Timer{Sys, NotifyContinue}(见下);其余轮次按以下顺序:Timer{Sys, RefdataRecheck}(到期时)→
   到期定时器 ≤ 16(按堆键升序)→ OMS 有待发 → Timer{OmsContinue} → 通知积压非空或游标未完 → Timer{Sys, NotifyContinue}
   → 其余环境决定(按下表的行序)
b. ctl(ByteSpsc,≤ 4 条且载荷合计 ≤ ctl_bytes_per_round)与 hk(≤ 4 条)
c. 实盘回报:rep-<venue> 轮转 + chain-in,合计 ≤ 64,每环 ≤ 32
d. bbo-* 轮转,合计 ≤ 128,每环 ≤ 32;积压 > conflate_backlog 的环先合并(见"BBO 合并");
   **每条 Bbo 处理完之后**,下一次 take 先对 ctl 与各实盘 rep 环的环头各做一次 Acquire 读,
   任一非空 → 按 b → c 交出它们(计入同一轮上面的上限),再回到 bbo
e. advice ≤ 16;每条之后做与 d 相同的环头检查
f. md-* 轮转,逐条交出;**每条 md 消息处理完之后**,下一次 take 先对 ctl、各实盘 rep 环、各 bbo 环的环头各做一次 Acquire 读,
   任一非空 → 按 b → c → d 交出它们(计入同一轮上面的上限),再回到 md。
   交出每条 md 之前,自这一轮首条 md 被交出起已过去 ≥ md_round_ns(按 TSC 换算)→ 这一轮不再交 md
g. rep-sim ≤ sim_max_per_round
定时器中途检查:c–g 每交出一条之前,把 view().next_deadline() 与当前 TSC 比较一次;已到期 → 这一轮到此为止
   (take 返回 false),下一轮的 a 交出到期定时器
背压:通知积压余量 < 2U(U = 单个 Input 的通知上界 max_cells_total + 128)时,a 只交 Timer{Sys, NotifyContinue},
   随后只交实盘回报(c),且每交一条之前要求余量 ≥ U;其余环这一轮不取(见"通知队列与续推")
```

"空闲轮"指 `step` 返回 Idle 的一轮,与线程是否随后 park 无关;环境决定表中"在(第一个)空闲轮写"一律指:上一轮为空闲轮、且这一轮开始时全部输入环为空、`wants_continue()` 为假的那一轮,在其 a 段写;线程 park 时超时取 `next_deadline`,环境决定最多推迟 `park_us`,它们仍由实盘驱动当场写成 Input,回放照读。优先级与让位给出以下等待上界(各项处理耗时见[延迟与资源契约](#延迟与资源契约)):

- 实盘回报(对冲腿)最多等**正在处理的那一条** Bbo、Advice 或 md 消息,加一次环头检查;
- BBO 最多等正在应用的那一条 md 消息(`L2Batch` 的全部档、一条 `L2Snapshot{TopN}`,或一片 REST 快照,每片 ≤ hft-venue `feed.snapshot_part_levels` 档)加一次环头检查([latency-budget §2.3](../../docs/latency-budget.md#23-bbo-与深度的通道)、[13 O9](../../design/13-设计评审与优化.md));
- 到期定时器(`HedgeRetry`、`Timer{Exec, Wake}` 等)最多等一条环输入;
- 关键帧一次写完的停顿发生在空闲轮之后;只有持续不空闲到最大推迟时才在事件边界强制写(见[快照](#快照))。

让位与停止条件只决定这一轮取什么、取多少,取出与处理的顺序照常写进 journal,回放只按 journal 顺序,确定性不受影响。**模拟账户的回报排在全部实盘输入之后**,每轮至多 `sim_max_per_round` 条,所以模拟负载再大也只在实盘输入处理完之后占用有界的时间,而每轮都会取一次、不会饿死。恢复、验证与镜像驱动按 journal 顺序交出,每轮 ≤ `replay_per_round` 条。

### 心跳

心跳原语是 [hft-rt](../hft-rt/README.md) 的 `Heartbeat`(单写者,`beat()` ≤ 1 ns),即 `HeartbeatBlock` 的区 0,由 apps/hft-core 创建([HeartbeatBlock](../../apps/hft-core/README.md#heartbeatblock)),以 `&'static` 交给 `Sequencer::boot`;本 crate 不分配、不拥有它。sequencer 每轮 `step` 调一次 `beat()`(恢复、验证、镜像驱动每 `replay_per_round` 条 Input 一次);housekeeping 每 1 s 确认计数前进后才 `sd_notify(WATCHDOG=1)`,kill-switch 读同一块判活([hot-path-rules T7](../../docs/hot-path-rules.md#线程t))。

### 环境决定写成 Input

实盘驱动是唯一接触环境(TSC、UTC、队列积压、空闲、存储)的地方;凡是要影响状态或 Output 的环境判断,它当场写成一条 `Timer` Input 记录,sequencer 只看记录。ms ↔ TSC 换算取当前段头的 `tsc_hz_milli`(回放读同一值)。`Timer` 记录头 `tsc` 在实盘为交出时刻(≥ deadline),`deadline_tsc` 在体内。

| `Timer` | 驱动何时写 | sequencer 做什么 |
|---|---|---|
| 堆内定时器(`owner`/`timer_id` 为登记值,见 [hft-event 定时器编码](../hft-event/README.md#定时器编码)) | TSC ≥ 堆顶 deadline(每轮开头,以及环输入之间的中途检查到期之后的下一轮) | 按 owner 分派:OMS(sweep、Unknown 查询、`HedgeRetry`、冒烟单到期撤单 `oms.on_smoke_ttl(.., keep)`:`keep` = `Draining` 生效且该 Drain 的 `cancel_resting = false`)、Exec、Risk(`MarginStale`/`MarginDeadline`/`MarginForce`)、策略定时器、`Sys{DeadMan}`、`Sys{ConnRotate}`、`Sys{SessionKeepalive}`(用户流会话续期) |
| `Oms{OmsContinue}` | `view().wants_continue()` 报告 OMS 有待发,下一轮的 a 段写(顺序见[主循环](#主循环step)) | 继续发放上一事件未发完的输出 |
| `Sys{NotifyContinue}` | `view().wants_continue()` 报告通知积压或 `naked_groups` 游标未完,下一轮的 a 段写(顺序见[主循环](#主循环step)) | 按原顺序投递至多 `notify_per_continue` 条积压通知,并继续未完的 `resolve_broken` 扫描(见[通知队列与续推](#通知队列与续推)) |
| `Sys{Roll}` | UTC 整点后第一个空闲轮;整点滚动避开资金费结算前后 2 min:该空闲轮的 UTC 落在任一订阅 venue 的 [结算时刻 − 120 s, 结算时刻 + 120 s) 内时不写,推迟到窗口之后的第一个空闲轮。段用量到软阈值后在第一个空闲轮写(`needs_roll` 返回 `RollAt::IdleRound`),到硬上限时在下一个事件边界立即写(`RollAt::EventBoundary`;两级阈值归 [hft-journal](../hft-journal/README.md));段用量触发的两级都不避开结算窗口。任何触发在 `wants_continue()` 为真时都等它变假(至多数轮) | 段滚动 + 引导块(`roll` 返回 `NextNotReady` 的处理见[快照](#快照)) |
| `Sys{Snapshot}` | **到期**:距上次关键帧 ≥ `keyframe_period_ms`,或其后的 Input 数 ≥ `keyframe_max_events`。到期后在"上一轮为空闲轮,且这一轮开始时全部输入环为空、`wants_continue()` 为假"的轮次写,作为这一轮唯一的 Input;**最大推迟**:到期后经过 2 × `keyframe_period_ms` 仍未写成时,不再等空闲轮,在 `wants_continue()` 为假后的下一个事件边界强制写(仍是这一轮唯一的 Input) | 一次写完 `StateSnapshot{purpose=Keyframe}`(见[快照](#快照)) |
| `Keyframe{inst}` | 每个 Live 簿在每个 `keyframe_period_ms` 窗口内一次,均摊 | 该簿的 `L2Snapshot{origin=Keyframe}` |
| `Sys{SnapshotNow}` | `SnapshotNow` 命令被 Applied 之后的第一个空闲轮 | `StateSnapshot{purpose=OnDemand}`(一次写完) |
| `Sys{DayRoll{day}}` | 时钟可信且 UTC 日 > 快照中的 `last_day_rolled`(含启动后首轮) | `day ≤ last_day_rolled` → 忽略;否则 `risk.on_day_roll(day, &ledger)`、`ledger.on_day_roll(now.engine_ns)`,更新 `last_day_rolled` |
| `Sys{ClockOk}` / `Sys{ClockBad}` | housekeeping 的时钟判定翻转 | `risk.mode.apply(Clock{ok})` |
| `Sys{FundingSettle{venue}}` | 时钟可信且 UTC ≥ `view().next_funding_settle()` 给出的该 venue 结算时刻 | [资金费合成](#资金费合成) |
| `Sys{PersistLag{level}}` / `Sys{DiskLow{level}}` / `Sys{StorageOk}` | housekeeping 测得的 persist 滞后 / 冷盘剩余空间跨越档位(1 = 禁开阈值,2 = Drain 阈值;阈值归 [apps/hft-core](../../apps/hft-core/README.md)),两项都回到阈值以下时写 `StorageOk` | `risk.mode.apply(Storage{kind, level})`;见[存储降级](#存储降级) |
| `Sys{KillSelfTestOk{account}}` | hft-core 启动时该账户的 kill 位自检通过(自检本身在 hft-core,结果另以 Meta `Note{KillSelfTest}` 供人读) | `risk.mode.apply(Gate(KillSelfTest{account}))`:启动闸门 `IDENTITY` 的一部分 |
| `Sys{RefdataRecheck}` | `view().refdata_recheck_due()` 为真(`ReplaceRefdata` 发起的 Drain 到达复核点,见 [Drain](#drain) 第 4 步)的下一轮 a 段最先写:先用待生效的 blob 跑一次 ctl-gateway 的装配校验(`RefdataAssemblyCheck`,装配处提供,归 apps/hft-core),结果装进 `timer_id` 的参数位(0 = 通过,否则 = control-plane `RefdataAssembly` 的失败项;id 与编码登记在 [hft-event 定时器编码](../hft-event/README.md#定时器编码)) | Drain 末二次复核(见 [Drain](#drain) 第 4 步) |
| `Sys{ConflateReport}` | 每 1 s | 写 `Note{Conflated}` 计数(驱动把上一报告以来丢弃的 `Bbo` 条数放在该 `Timer` 记录的 `deadline_tsc` 里,回放从日志读出,不依赖驱动状态) |
| `Sys{AdvisorTick}` | 每 1 s | 只记录;advisor 以它为 Model 的时间刻度(SDK `model_tick`) |

`EngineClock`(每次进程启动恰好一条、在任何其他 Input 之前)同样由实盘驱动写成 Input,见[引擎时间](#引擎时间engine_ns)。恢复、验证与镜像驱动不生成这些记录,只从日志读。

### BBO 合并与 CONFLATED

仅对 `Bbo` 且仅在该 bbo 环积压 > `conflate_backlog` 时:驱动从该环一次取出 ≤ 32 条,同一 inst 的多条 `Bbo` 只保留最后一条并置记录头 `CONFLATED`,再逐条交出(逐条之间照常做环头检查与定时器中途检查);被丢弃的 `Bbo` 不进 seq journal(raw journal 里仍有原始帧)、不对任何状态可见,丢弃计数经 `Timer{ConflateReport}` 输出。深度与成交从不合并。

### 引擎时间(engine_ns)

算法与语义的唯一定义在 [event-model 时间戳语义](../../docs/subsystems/event-model.md#时间戳语义)(`EngineClock` 记录布局归 [hft-event](../hft-event/README.md#engineclock0x050ainput));本 crate 负责维护与提供:

1. **基准**:当前运行的 `E = {base_engine_ns, base_tsc, tsc_hz_milli, clock_seq}`(快照节 25 `EngineClockRow`)。处理 `EngineClock` Input 时 `E` 取该记录(`base_tsc` = 其记录头 tsc),同一事件内写 `Note{Sequencer, EpochStart}`;从中途快照起步(恢复、验证、镜像)时 `E` 取快照节 25。运行中的段滚动不改 `E`。
2. **每条 Input**:`Now.engine_ns = E.base_engine_ns + q`,`q = ⌊Δ × 10^12 / H⌋`,`Δ = hdr.tsc − E.base_tsc`,`H = E.tsc_hz_milli`。热路径不做 u128 除法:
   - **乘数与移位**:设定 `E` 时预算 `mult = ⌊2^58 × 10^12 / H⌋`;要求 `mult < 2^64`,即 `H > 10^12 / 64`(TSC 频率 > 15.625 MHz),否则 `Fatal(ClockRange)`。每条 Input 算 `q' = (Δ × mult) >> 58`(u128 乘积 < 2^122)。
   - **Δ 的取值范围**:同一次运行内 TSC 单调,`Δ ∈ [0, 2^58)`;8 GHz 下约 417 天、1 GHz 下约 9 年,超出 → `Fatal(ClockRange)`(实际运行远到不了)。
   - **±1 校正足够**:`mult > 2^58 × 10^12 / H − 1`,所以 `Δ × 10^12 / H − Δ × mult / 2^58 < Δ / 2^58 < 1`,得 `q' ∈ {q − 1, q}`;再以一次 u128 比较 `(q' + 1) × H ≤ Δ × 10^12`(两边 < 2^99)决定是否加 1,结果与定义式逐位相同。
   - 属性测试覆盖 `Δ` 的 0、1、`2^58 − 1` 与随机值,`H` 覆盖 15.625 MHz 以上的边界值到 8 GHz;O(1),计入每条 Input 的固定开销。SDK `Now::ens_of`/`tsc_of` 用同一基准。
3. **交给下一次运行**:`view().stop_engine_ns()` = 最后一条已应用(带 `EVENT_END`)记录的 engine_ns——恢复后即上次运行的末值,`--recover-from-snapshot` 为 `SnapshotHdr.engine_ns`,空 journal 为 0;hft-core 的实盘驱动用它填新运行的 `EngineClock.base_engine_ns`。每个快照头的 `SnapshotHdr.engine_ns` = 其所在位置的 engine_ns。Recovery 快照使用已提交的 `stop_engine_ns`,不读取尚无新 Input 的事件上下文;写完该快照后、尚未消费 EngineClock 就再次崩溃,时间仍须保持。节 25 保留旧运行的时钟基准,由新 EngineClock 切换。
4. **运行起点**:处理 `EngineClock` 或从快照起步时调 `ledger.on_run_start(E.base_engine_ns)` 与 `risk.set_run_start(E.base_engine_ns)`:早于它的换算率、保证金报告与标记价视为过期(引擎时间不计停机,停机前的数据不能当新的用)。
5. **谁用它**:ledger 与 risk 的持久时刻(`*_ens`)、SDK `Now.engine_ns`(策略与 hft-strat-lib 的持久状态)、Viewer 预览的 TTL、advisor 的 `AdvisorLag`。本 crate 的定时器堆与 OMS/exec 的 TSC 量只在一次运行内有效,跨运行按"停机时间不计"平移([定时器](#定时器)),与 engine_ns 同口径。

### 连接活性

报价新鲜度按 **BBO 连接健康**判定(活性证据的来源与发布规则归 [event-model 关键事件体](../../docs/subsystems/event-model.md#关键事件体),槽号与成员事件归 [hft-venue](../hft-venue/README.md);判定式 `BboView::quote_live` 归 SDK,阈值 `conn_liveness_ms` 归 hft-risk)。每个 venue 的 BBO 连接占槽 `0..MAX_BBO_CONN_SLOTS`(常量及其值归 [hft-types](../hft-types/README.md#id-与枚举);`ConnState.conn` / `MarketHdr.conn` 小于它即槽号,连接退役后槽复用)。本 crate 维护:

- 每 (venue, 槽) `ConnLive { up: bool, resync: bool, live_tsc: Tsc }`(快照节 26);
- 每 inst `bbo_member_mask: u8`(bit k = 槽 k 是该 inst 的 BBO 成员连接)与 `bbo_flags.RESYNC`(该 inst 的 BBO 在补帧)(快照节 10)。

这些都只含本次运行的值:处理 `EngineClock` 时全部清零(新运行的连接重新发布 `Up` 与成员事件)。

| Input(BBO 连接,`conn < MAX_BBO_CONN_SLOTS`) | 动作 |
|---|---|
| `Bbo` | 该槽 `live_tsc = max(live_tsc, MarketHdr.rx_tsc)`;置该 inst 在该槽的成员位(防御);报价被应用(非 DUPLICATE/OLDER)时清该 inst 的 `RESYNC`,记 `bbo_hw_rx_ns` 与 `bbo_flags.SW_TS`(记录头是否带 `SW_TS`) |
| `ConnState{Up, inst = 0}` | 整条连接:`up = true`、`resync = false`、`live_tsc = rx_tsc` |
| `ConnState{Up, inst ≠ 0}` | 置该 inst 在该槽的成员位;该槽 `live_tsc` 前移到 `rx_tsc` |
| `ConnState{Down, inst ≠ 0}` | 清该 inst 在该槽的成员位(退订) |
| `ConnState{Down, inst = 0}` | 连接退役:`up = false`(活性证据作废);清该 venue 全部 inst 在该槽的成员位(O(该 venue 订阅的 inst 数),只在连接退役时;槽随后可被新连接复用) |
| `ConnState{Alive}` | `live_tsc = max(live_tsc, rx_tsc)`;只做这一步,不入队给策略 |
| `ConnState{Resync}` | `inst = 0`:该槽 `resync = true`,直到该槽的下一条连接级 `Up`;`inst ≠ 0`(BBO 连接上只有 `BBO_REFRESH`):置该 inst 的 `RESYNC`,该 inst 的下一条被应用的 `Bbo` 清除 |
| 其他 `ConnStateKind`(`Rotated`、`Degraded`) | 不改连接活性(按"引擎处理"表的其余规则处理) |

深度/成交/标记价连接(`conn ≥ MAX_BBO_CONN_SLOTS`)的记录不改上表任何状态,它们的健康体现为簿状态(`Resyncing`);这些连接的 `ConnState{Alive}` 同样不入队给策略、不改任何状态(只供 telemetry 计数)。

`Views::bbo(i)` **按值**投影:报价取 hft-book `Top`;`conn_up` = 任一成员槽 `up`;`conn_live_tsc` = `up ∧ ¬resync` 成员槽 `live_tsc` 的最大值,没有这样的成员时取 `up` 成员的最大值(无 `up` 成员为 0);`resync` = 没有任何 `up ∧ ¬resync` 的成员槽 ∨ 该 inst 的 `RESYNC` ∨ 该 inst 的簿处于 `Resyncing`;`rx_tsc`/`hw_rx_ns` 见[只读视图](#只读视图seqviews实现-sdk-views)。成员 ≤ `MAX_BBO_CONN_SLOTS` 个,投影 O(成员数)。hft-risk 的 `RiskView::bbo`、OMS `price_hedge`、hft-exec 读的是同一投影。因此繁忙连接上的安静 inst 随连接的存活证据保持健康;一条成员连接退役、断开或在补帧时,其他成员仍 up 且未补帧的 inst 不被判不可用。

### 每条 Input 的处理顺序(`process`)

`testkit` feature 在每条实际 Input 的分派前与完成后向驱动提供只读观察,包含实盘驱动生成的定时器;观察不进入 journal、快照与生产构建。驱动只读业务状态,诊断缓冲丢失须使 DST 失败。验收归 [TK-070](../hft-testkit/TESTS.md#tk-070-每步只读访问调试视图) 与 [TK-106](../hft-testkit/TESTS.md#tk-106-真实引擎的基础币费用配置与对冲余数观察)。

`Driver::observe_boundary` / `LiveIo::observe_boundary` 在完整 Input 与启动完成时提供只读 `audit::StateView`。测试驱动可独立序列化该边界的全部组件,与当次 journal 快照重建结果比较,覆盖同一轮内的多个快照和恢复。`ledger()`/`refdata()` 只返回共享引用,供冷账本检查逐项核对同边界的完整视图,包括未持久化的派生字段。该接口仅在 `testkit` feature 下存在,序列化无生产热路径预算,不改变事件和快照字节;验收归 [TK-115](../hft-testkit/TESTS.md#tk-115-每条快照在其完成边界检查-inv-6) 与 [LED-267](../hft-ledger/TESTS.md#led-267-dst-中每个快照点-journalledger-等于引擎-ledger)。

该 feature 在意图计入自身在途量前复制 `DecisionFacts`(group Unknown 取 LegView,链接 Unknown 独立扫描该链接的订单槽),以最终 journal seq 关联每条 New 决策(含拒绝与 Shadow)。每个 Input 的待关联与完成观察各至多 512 条,溢出或序号缺失显式失败(SEQ-373);只读事实供 [TK-107](../hft-testkit/TESTS.md#tk-107-真实引擎逐事件观察与反例) 重算参考量。

OMS 对冲的 `place` 调用前另复制完整链接身份、重试计数、目标账户/venue/inst 与空闲槽数;调用失败时保存原错误及所属 Input seq,拒绝的 OrderDecision 由意图管线记录。失败观察每个 Input 至多 512 条,溢出失败,下一 Input 清空;检查器按链接实例逐次核对计数,验收见 [TK-121](../hft-testkit/TESTS.md#tk-121-对冲-place-失败必须有同一链接实例的原始证据)。

`Engine::restore_observations` 只在 `testkit` 下提供冷恢复调用记录,覆盖实例 Model 预算、ledger/risk 的组变更、标记价缓存和非零资金费周期。每次引擎恢复至多 512 条,不随 Input 清空;溢出返回 `RestoreObservationError::Overflow`,不返回截断后的成功列表。记录在调用完成处采集,不影响业务返回值、journal 或快照;SEQ-250 在首条 Input 前核对次数、入参与派生结果,SEQ-373 检查容量边界。

1. `seq = journal.append(flags, slot.tsc, body)`——**先记录,后生效**;seq 只取自写者返回值;`slot.tsc` 是驱动交出该条时的 TSC(实盘)或日志值,批内没有排队等待,所以 `ingest`(`proc_tsc − rx_tsc`)含环内排队、`decide` 从这里开始([latency-budget](../../docs/latency-budget.md))。
2. 过期检查(仅 `ControlCmd`):`hdr.tsc > expires_tsc` → 只写 `CmdResult{Expired}`。
3. **引擎处理**(按 kind 分派,见下表):更新簿、OMS、ledger、风控;OMS 的第二腿/重试/反向单经意图管线出决策(对冲快路径排在本步最前);执行层按其[调用顺序](../hft-exec/README.md)更新进度;待投递给策略的通知进 `NotifyQueue`(固定容量 128 = OMS 16 + 执行层 8 + 风控 8 + 模式 64 + 行情订阅 8 + 其他 24,编译期断言;某分区满时其后的通知按产生顺序进 `NotifyBacklog`,**从不丢弃**,见[通知队列与续推](#通知队列与续推))。
   - **OMS 输出的排空**:直接遍历池内缓冲,回报后的 group 列表使用按 `max_cells_total` 启动分配的双缓冲,嵌套回调的新增触及保留到下一轮;每次调用 OMS 带 `out` 的入口之后先检查 `OmsOut.cash_error` 、已知现金或现货原始名义成交的正价格,以及现货原始名义为正;失败即 `Fatal(OmsCash/Settlement)`,不发对冲、不入账。校验通过才排空(嵌套调用同样处理),顺序:`hedges` 中非 Reverse(→ 对冲定价 → 意图管线,对冲快路径最前)→ `fills`(转 `FillDelta`;`cash = Some(amount)` 以品种 quote 资产交 `apply_settled_fill`,包括已知零额;现货 `notional = Some(n)` 交 `ledger.apply_spot_fill`,保留价格取整前的金额;其余交 `ledger.apply_fill`;返回 `net_adjust` 时同一事件内调 `oms.net_adjust` 与 `exec.on_leg_adjust`)→ `fees`(转 `FeeDelta` 交 `ledger.apply_fee`,同上)→ Reverse 请求(当前 Input 的成交/费用入账后再做可减量检查)→ `updates`(`EXEC` 槽交 `exec.on_order_update`;`SMOKE` 槽不投递给任何人;其余入队给槽的拥有者策略)→ `links`(投影为 `LinkNotice` 调 `exec.on_link`)→ `foreign_fills`(按 `ExternalFill` 行处理)→ `decisions`/`swaps`(追加 Output 并按"意图管线"第 7 步出环)→ `timers`(登记或撤销堆条目)→ `notes`、`latency`(追加)。
   - **账本注记**:成交与费用产生的 `Note`(含 `RoundClosed`)在上述注记阶段取走并追加到当前 Input 的 Output;BBO、L2 更新和标记价驱动的估值调用在返回后立即排空注记。排空直接遍历已分配缓冲并清空,不创建临时 Vec;注记不跨 `EVENT_END` 或快照保留,后续对账不重复写出(SEQ-382、SEQ-383)。已证明重复、没有成交输出的 OMS 路径告警同样排空并参与回放,归 [OMS 回报匹配](../hft-oms/README.md#回报匹配与成交增量)(SEQ-419)。
   - **之后**才对本 Input 涉及的账户与 group 调 `risk.on_pnl`/`risk.on_legs`、对链接 `Broken` 调 `apply(Auto(Broken))`,并执行由此产生的 `ModeOutcome`。**同一 Input 内,`exec.on_risk` 一律在全部 `exec.on_order_update` 与 `exec.on_link` 之后**:执行层先看到最后一片的回报,再看到 `CapCleared{Broken}`,自动有界平仓的完成判定因此不丢。
4. **策略投递**:按入队顺序投递通知;行情事件按 CSR 索引投递给订阅者。每个实例一个 `Ctx`。**同一事件派发时实盘实例先于模拟实例**:CSR 行内与 `NotifyQueue` 按 `dispatch_order` 排序(先实盘、后模拟),所以实盘实例的意图先进管线、先出环。
5. **Ctx 应用**:按投递顺序处理每个 `Ctx`:目标(`target`/`clear_target`)→ `exec.set_target(owner, &t, &views, now, &mut out)`/`exec.clear_target`(签名以 hft-exec 为准);订单意图 → 意图管线,并对该 (account, group) 调 `exec.note_strategy_action`;撤单/改单 → `oms.cancel`/`oms.amend`(传本 Input 的 `TrigStamps`:取自本 Input 的记录头与事件体,字段与缺失时取 0 的规则归 [hft-oms](../hft-oms/README.md));`request_halt(group, reason)` → `apply(Auto(Halted{strategy, account, group}))`;定时器 → 堆;注记 → journal。
6. **执行层推进**:引擎处理与 Ctx 应用只调执行层的登记函数(`touch_inst`、`on_order_update`、`on_link`、`on_external`、`on_leg_adjust`、`on_risk`、`on_gate_open`、`force_deleverage`、`engine_flatten`、`on_timer`、`set_target`…);随后若 `exec.has_pending()`,调一次 `exec.advance(&views, now, &mut out)`,推进全部待推进 cell(顺序:实盘账户的 cell 先于模拟账户的,再按 (account, group) 升序;排序由 hft-exec 实现)。**每次调用执行层任何带 `out` 的入口(登记函数与 `advance`)之后立即排空其 `ExecOut`**(直接遍历池内已有缓冲;嵌套调用另取缓冲,不复制为临时 Vec),不留到事件末尾(两次调用之间执行层的输出缓冲必须为空);排空顺序:`decisions` 追加(Output `ExecDecision`)、`cancel_resting` 逐条调 `oms.cancel_resting_on(account, inst, &mut out)`(撤该 inst 上本账户全部非 EXEC 挂单;其 `OmsOut` 按上面的规则排空)、`orders` 逐条进意图管线(结果经 `exec.on_place_result` 回填)、`cancels` → OMS、`broken` → `apply(Auto(Broken))`、`timers` → 堆、`notify` → 拥有者策略、`notes`。
7. **Broken 的解除**不由执行层报告:每次 LegTracker 变化后,以及任何路径(链接 `Broken`、`HEDGE_LIMIT`、执行层 `BrokenReq`、`resolve_broken`)对某 group 设 `Broken` 之后,在同一 Input 内对该 group 调一次 `risk.on_legs(a, g, &LegSummary)`(同一 Input 内同一 group 的摘要未变时不重复调用,摘要变了照常再调)(从 OMS `LegView` 投影:`naked_ratio`、`naked_notional`、`naked_filled_notional = |Σ leg.net| × mark`(取价同 hft-exec 尘埃判定)、`quiescent` = 无 Pending*/Unknown 槽且各腿 `inflight_immediate = 0`);返回 `AutoClear{Broken}` 时交 `apply`(`HedgeFailure` 不动)。规则归 [hft-risk 断腿](../hft-risk/README.md#断腿broken自动有界平仓l3-与-flatten)。
8. **二次投递**:第 6 步产生的执行层通知(`EventRef::Exec`)与意图结果在同一 Input 内投递给策略;这一轮 `Ctx` 的订单意图照常进管线,目标变更交给执行层,其切片推进留给下一个相关 Input(执行层为其登记立即到期的 `Timer{Exec, Wake}`)。每个 Input 至多两轮策略投递。
9. `decided_tsc = drv.measure()`,写入本 Input 的每条 `OrderDecision.decided_tsc`、出环 `GwCmd` 与 OMS `SlotStamps`(新单、撤单与改单相同)——测量字段,不参与任何决策。
10. 输出按产生顺序 `append`:**记录头 `tsc` = 触发它的 Input 的 `tsc`**,体内带 `trigger_seq`。
11. `journal.end_event()`:在本 Input 的最后一条**非 Meta** 记录上置 `EVENT_END`。
12. **任何出环提交之前先 `journal.publish()`**,保证已发出的订单其 `OrderDecision` 已对读者可见;同一 Input 的多条出环消息:先全部追加并 `end_event`、publish 一次、再依次推环(模拟账户的推 `out-sim`)。

余额初始化与校验共用 quiet:该账户该 venue 全部品种的 open_buy/open_sell 为零且最近成交已过静默窗口,不以只覆盖增仓的名义金额代替订单量。保证金 mark 新鲜度包含永续在途品种;最后持仓与在途均消失时立即重投影,报告自身的过期检查仍生效。

| Input | 引擎处理 |
|---|---|
| `EngineClock`(每次进程启动一条,先于其他 Input) | 设本次运行的引擎时钟基准(见[引擎时间](#引擎时间engine_ns))→ 清空连接活性(节 26、各 inst 的成员位与 `RESYNC`)与 Viewer 预览表 → `ledger.on_run_start(base)`、`risk.set_run_start(base)` → 同一事件内写 `Note{Sequencer, EpochStart}`(其 seq 即 `Now.boot_seq`) |
| `Bbo` | 簿顶更新(`Top::apply`)→ 连接活性(见上节)→ 报价被应用时 `ledger.on_ref_px(…, now.engine_ns)` → 未实现因此变化的每个账户(ledger `take_upnl_changed`,账户号升序)调 `risk.on_pnl`(当日 PnL 含未实现,[hft-risk 当日亏损](../hft-risk/README.md#当日亏损锁存))并执行其 `ModeOutcome` → `exec.touch_inst` → 订阅者入队(`EventRef::Bbo` 带按值的 `BboView`) |
| `L2Batch` | `hft-book` **整批应用**;整批完成后才更新 mid、调 `ledger.on_ref_px`(及随后的 `risk.on_pnl`,同 `Bbo` 行)、`exec.touch_inst`、订阅者入队——策略永远看不到半条消息的簿 |
| `L2Snapshot{origin = TopN}`(分档快照流) | hft-book 按整替换应用(`apply_snapshot`,无序号链、无 REST 重同步);之后与 `L2Batch` 相同:更新 mid、`ledger.on_ref_px`、`exec.touch_inst`、订阅者入队 |
| `L2Snapshot{origin = Exchange}`(REST 快照分片,只在启用增量深度时出现) | 按 hft-book 的快照规则应用该片(重同步状态机归 [hft-venue](../hft-venue/README.md),其后的 `ConnState{Up, inst}` 使簿回到 Live) |
| `Trade`/`Funding`/`IndexPrice`/`Liquidation` | `MarketLast` 更新(`Funding` 带实际结算周期,`interval_s ≠ 0` 时同时调 `ledger.on_funding_interval(inst, interval_s)`,与当前值不同时先把旧值记入 `prev_interval_s`;`Funding` 另记 `rx_tsc` 与 seq 供 `FundingView.upd_tsc`;把 `next_funding_ns` 推过未结算的结算时刻时先做[资金费合成](#资金费合成))→ 订阅者入队 |
| `MarkPrice` | `MarketLast` 更新(`mark_px`、`mark_ts_ns`、`mark_ens = now.engine_ns`)→ `ledger.on_mark(…, now.engine_ns, &mut touched)` → 对受影响的 (account, venue) 把 ledger `MarginState` 投影为 `MarginSample`(两种强平距离、两种比率、`report_ens`、`mark_ens = mark_oldest_ens`)调 `risk.on_margin`(执行 `MarginOut`)→ 未实现变化的账户调 `risk.on_pnl`(同 `Bbo` 行)→ `exec.touch_inst` → 订阅者入队 |
| `OrderEvent`/`SendReport` | `oms.on_*` → 按第 3 步排空 `OmsOut`(对冲最前;`EXEC` 槽更新交 `exec.on_order_update`,链接变化交 `exec.on_link`)→ `risk.on_pnl`/`on_legs` → 执行 `ModeOutcome`。有槽更新的 group 即使无成交也刷新敞口,包括 Expired 移除在途量;链接 Broken 按第 7 步处理 |
| `OrderEvent{FeeOnly}` | OMS 不迁移;`OmsOut.fees` 转 `FeeDelta` 交 `ledger.apply_fee` → 返回 `NetAdjust` 时同事件调 `oms.net_adjust` 与 `exec.on_leg_adjust`,排空输出后刷新该 group 的 `risk.on_legs` 与 CellFlat,再投递策略 |
| `ExternalFill`(另有两个来源按同一处理:OMS 防御路径的 `ForeignFill`:直接以其成交字段构造 `ExternalFill{cause = Unknown}`,显式结算额取 `ForeignFill.cash` 的累计差额;同事件的 `Note{OmsForeignFill}` 供冷读者核对原始 Input 并独立计算结算增量(其余入账字段与该 Note 一致,未列出的字段为 0),去重键取 `ForeignFill.trade_id`,为 0 时 `exch_order_id` + `fill_seq` = `ForeignFill.cum_filled` 的原始值;对账 `AUTO_EXTERNAL` 条目,见 `ReconcileReport` 行) | `PERP_CASH` 的原始事件经 `ledger.apply_settled_external` 校验完整现金并去重入账,错误返回 `Fatal(Settlement)`;OMS 防御路径传递完整累计结算的差额;原始 ExternalFill 在记账前核验 OMS 来源和容量,成功后保留来源,跨来源重叠报 `Fatal(OmsCash)`。AUTO_EXTERNAL 有原始凭据时经 `ReconcileBatch` 整组预检并调用 `apply_reconciled_external`,缺少所需现金事实则返回 `Fatal(UnsupportedExternalCash)`;其他 venue 调 `ledger.apply_external(e, auto_recon, t)`(去重、`auto_held` 抵扣、归属 group 与腿(以该腿减到 0 为限)、入账,由 ledger 写注记;`auto_recon` 只对 `AUTO_EXTERNAL` 合成的本体为真;规则归 [hft-ledger](../hft-ledger/README.md#外部成交externalfill));返回 `None`(重复,或已被 `auto_held` 全部抵扣)→ 本条到此为止(不调 OMS、执行层与风控,不入队);否则 `ExternalApplied` 的归属部分(`net_qty ≠ 0` 时;`group` 可为 `GROUP_NONE`)交 `oms.external_adjust`(LegTracker;`cause = Unknown` 时记下以便与之后的槽成交去重;OMS 自己的 `ForeignFill` 传 `exch_order_id = 0` 不记——其累计量已在 OMS 的 `foreign` 表,记下会在同一单的下一次报告里再扣一次)→ `exec.on_external(a, &view)`(视图的 `group`、`qty` 取归属部分;返回截止时刻,语义归 hft-exec);`none_qty ≠ 0`(超出该腿、记 `GROUP_NONE` 的部分)另以 `group = GROUP_NONE` 的视图同样调 `exec.on_external`;量为 0 的部分不调用、不入队 → `risk.on_pnl`/`on_legs` → 截止写进 `ExternalFillView.deadline_tsc` 后向该账户绑定的实例入队 `EventRef::External`(两部分都非 0 时两条)。OMS 防御路径的 `ForeignFill` 与 `AUTO_EXTERNAL` 一样在处理该 Input 时确定地构造、不另写 Input;带足入账字段的 Output 类注记由 hft-oms 写出(字段归 hft-oms),冷读者经 hft-ledger 的同一消费路径入账 |
| `MarginReport` | `ledger.on_margin(m, now.engine_ns)` → `risk.on_margin`(`truncated` = `ITEMS_TRUNCATED`)→ 执行 `MarginOut`:`mode` → `risk.mode.apply`;`events` → 该账户绑定的实例入队 `EventRef::Risk`,并调 `exec.on_risk(a, &e, now)`;`force` → `exec.force_deleverage(a, venue, frac, now, &mut out)`;`timers`(engine_ns 截止)→ 按 `now.tsc_of` 换成本次运行的 TSC 进堆;`notes` |
| `AccountConfigReport` | `risk.on_account_cfg(hdr, &lev_items)`(实际杠杆:检查链 #7b 与永续杠杆上限)→ `risk.mode.apply(Gate(AccountCfg{hdr, keys_required}))`(`keys_required` 见[模式与 cap 的驱动](#模式与-cap-的驱动))(闸门 `ACCOUNT_CFG`/`IDENTITY`;放行后不符 → cap;UID 不符 → HARD + 内部 Drain) |
| `FundingSettle`(推送) | 按 `VenueCaps.funding_settle`:`PerSymbolPush` → `ledger.apply_funding`(重复 → 无动作);`RateOnly` 认证账单实额 → `ledger.apply_funding`(差分完成估算,规则归 [ledger](../hft-ledger/README.md#资金费));`BalanceOnlyPush` → `ledger.apply_funding_push`(钱包入账与去重;`Mismatch` → `Note{FundingMismatch}`)→ `risk.on_pnl`。结算时刻一律经 hft-types [`funding_settle_norm_ns`](../hft-types/README.md#资金费结算时刻归一化) 归一化后比较与配对(ledger 入口内完成,周期与锚的取法归 [hft-ledger](../hft-ledger/README.md#资金费时刻归一化)) |
| `BalanceReport` | `ledger.on_balance(b, quiet, now.engine_ns)`(`quiet` 由 OMS 在途状态判定)→ 写账本注记,其中实际执行校验的 `BalanceCheck` 是冷读者重算漂移标志的依据([ledger 冷消费者](../hft-ledger/README.md#冷消费者journalledger)) |
| `PositionReport` | 只比较,差异写 `Note` |
| `ConnState` | BBO 连接(`role = Md`、`conn < MAX_BBO_CONN_SLOTS`):更新[连接活性](#连接活性),`Alive` 只做这一步、不入队给策略。深度/成交连接的 `Alive`:无动作。其余:深度连接的簿失效/重同步、`oms.on_conn_state`、策略入队 `EventRef::Conn`、启动闸门(gw 的主下单连接与用户流、热备分别汇总为 `Gate(OrderConns{…})`,见 [hft-risk 启动闸门](../hft-risk/README.md#启动闸门)) |
| `ReconcileReport` | **`Funding` 条目**(含 `inst = INST_NONE` 的钱包条目)到达即处理,不暂存:投影为 `FundingReconItem` 交 `ledger.reconcile_funding(it, now.utc_ns, now.engine_ns)`,它写的 `Note{FundingCorrected}` 追加在本 Input 内(冷读者按该注记入账),返回 `Corrected` 的 `PnlDelta` 交 `risk.on_pnl`([hft-ledger 资金费对账校正](../hft-ledger/README.md#资金费对账校正))。**其余条目**按 `report_id` 暂存(节 19);同组末条到达时:① 处理带 `AUTO_EXTERNAL` 的仓位条目(见下);② 整组交 `oms.on_reconcile`、`risk.on_reconcile`(它跳过 `Funding` 条目),`ReconVerdicts.gate` 为 `Some` 时交 `mode.apply(Gate(Recon))`(不含 `Summary` 的组为 `None`,不产生闸门输入、不计作一轮,[hft-risk 对账分级](../hft-risk/README.md#对账分级全系统唯一一次)第 8 条);③ 对该 (account, venue) 调一次 `ledger.funding_window_sweep`(注记同样追加在本 Input 内),`Corrected` 的 `PnlDelta` 交 `risk.on_pnl`。余额条目的资金费修正已由 reconciler 完成,这里不再做。**`AUTO_EXTERNAL`**(位归 hft-event,置位条件归 [reconciler](../../apps/reconciler/README.md)):sequencer 先复核——用户流前置条件 `risk.auto_external_ok(a, v, summary.sflags)` 为真(本组与上一组报告都无 `USERSTREAM_DOWN`,且该 (account, venue) 的用户流自最近一次 Up 起已稳定 ≥ 1 个对账周期;规则归 [hft-risk 对账分级](../hft-risk/README.md#对账分级全系统唯一一次))、该 (account, venue) 无 Unknown 槽、该 inst 无非终态槽、该 inst 自 `view_seq` 以来无成交(与 `AdoptExchange` 的 `StaleReport` 同一判定)、引擎永续仓位等于条目的 `engine_qty` 且 `exch_qty` 与之不反号(同号,或为 0 = 全量 ADL)、绝对值更小(永续"同号缩小",与 reconciler 置位条件同一口径);通过则调 hft-ledger 的合成函数 `auto_external_fill(a, v, inst, engine_qty − exch_qty, mark_px, report_id)`(`mark_px` = 该 inst 的 `MarketLast.mark_px`;字段与去重键的唯一定义在 [hft-ledger 外部成交](../hft-ledger/README.md#外部成交externalfill)),按 `ExternalFill` 行的同一处理入账(`auto_recon = true`;P0;ledger 注记带 `AUTO_RECON`),再以 `Gate(Disposed{…, AutoExternal})` 报告处置;复核不通过 → 不入账,条目照常分级、等人工处置。**这笔成交在 `ReconcileReport` 这条 Input 内确定地合成,不另写记录**(另写会在回放时被重复应用):回放与验证同样重现;冷读者(reconciler 的引擎视图、hft-py)经 hft-ledger 的 journal 消费路径调用同一函数得到它 |
| `ControlCmd`/`ConfigBlob` | 命令路径 → `CmdResult` |
| `Timer` | 见上表;`owner = Oms`:`Sweep` → `oms.on_sweep`、`HedgeRetry{link}` → `oms.on_hedge_retry`、`UnknownQuery` → `oms.on_unknown_query`;`owner = Exec` → `exec.on_timer`;`owner = Risk` → `risk.on_timer(t, &now, &mut out)`(执行 `MarginOut`;engine_ns 截止未到的早到定时器由 risk 重新登记) |
| `Advice` | [Advice 路由](#advice-路由) |
| `TxLifecycle`/`PoolQuote`/`ChainHead`(optional) | `oms.on_tx_lifecycle` → `ledger.apply_chain`(feature `chain`)→ 同成交 |

### 意图管线

`oms.place` 返回 `NoFreeSlot` 或 `LinkTableFull` 时,同一 Input 写 `OrderDecision{reject = CAPACITY, slot = 0, gen = 0}`;`NoTable` 写 `INVALID`。保留原意图的身份、价量、`intent_no` 与触发 seq,不出环、不扣令牌。OMS 内部对冲失败的计次与重试不变。令牌在 place 成功后、排空 OMS 输出前提交。

Simultaneous 第一腿在第 5 步前预计算第二腿的实际数量、限价、分类与模式门,检查两腿槽和链接容量;链接可负担性使用含累计 lot 残余的实际第二腿数量,名义不低于实际订单。`risk.evaluate_pair` 全部通过才提交两份 admission 并连续建槽。两份 OrderDecision 属于同一 Input,EVENT_END/publish 之后以 `OutRecord::Pair` 出环。任一腿模式、风控或槽/队列容量失败,两腿均不提交;成功预检后建槽异常为 Fatal(PairAdmission),本事件不发布。两腿均用准入时限价,不等待成交后再次定价。

所有来源的意图(策略 `Ctx`、执行层 `ExecOut.orders`、OMS 的 `Second`/`SecondRetry`/`Reverse`、命令)走同一条管线:

0. **链接结构**(只对策略 `place` 的带 `LegLink` 意图):`hft_oms::link_valid(&link)` 为假 → 只写 `OrderDecision{reject = INVALID}`,结果 `Rejected(INVALID)`(有效模式 Shadow 时记录另带 SHADOW,结果 Shadowed),不经后续各步(规则归 [hft-oms LegLink 与 HedgePlan](../hft-oms/README.md#leglink-与-hedgeplan))。

1. **对冲定价**(OMS 的对冲请求):`oms::price_hedge(req, plan, bbo, now, &risk.hedge_px_limits(account, venue, inst), …)`(签名以 hft-oms 为准,含该 inst 的 tick)——`HedgePxLimits`(hft-types)的连接健康阈值 `conn_liveness_ms` 与越价上限 `rr_max_cross`(该 inst 所属价格带类别)只在 `RiskParams` 里配置(与检查链 #5 同值同源;报价可用 = `BboView::quote_live`,盘口不变不算过期);限价按 hft-types `cap_cross_px` 取对手价越价与 `rr_max_cross` 上限中更近的一个(仍是 IOC)。`Err(StaleQuote | BeyondCross)` 是门控:调 `oms.hedge_gated(link_id, HedgeGate::Px(err), …)`(登记 HedgeRetry、不计次、截止照算),本条不出环。
2. **模式门**:只对策略 `place` 的意图与执行层 `origin = Target` 的增仓订单,读 `risk.mode.effective(cell)`;Shadow → 标记只评估,继续完整风控检查链;Live 但 `open_allowed = false` 的 Open → `MODE_SHADOW`;行为表归 [hft-strategy-sdk](../hft-strategy-sdk/README.md)。OMS 的对冲单、执行层的 Engine 订单与减仓订单不经模式门,只受 HARD 约束;**命令产生的冒烟单(`SmokeOrder`)不经模式门**,走完整检查链(检查链对它的口径归 [hft-risk](../hft-risk/README.md))。
3. `risk.classify(…, &VenueCaps)` → `Classification`。
4. 投影 `IntentView`:`tif`、`flags`(`OrderFlags`,含 `SMOKE`)、`px` = 意图限价、`notional` = `Risk::notional(qty, px, quote_rate)`(名义的唯一实现,[hft-risk](../hft-risk/README.md));带 LegLink 的第一腿 Open 填 `link_venue/link_account/link_inst/link_side/link_qty/link_notional`(取自 `HedgePlan`,`link_qty` = qty × ratio;`link_notional` = `link_qty` × `mid × (1 + rr_max_cross)`(`mid` 与 `rr_max_cross` 取第二腿 inst 及其价格带类别,两个方向相同):这是第二腿限价的上界——买单限价不高于 `mid × (1 + rr_max_cross)`(OMS 超滑点时就按它定价),卖单限价不高于对手价、因而不高于 `mid`——所以按同一 `mid` 定价的第二腿一定在 `max_hedge_notional` 之内;风控据此做第二腿报价年龄 `LINK_STALE` 与可负担性 `LINK_FUNDS` 检查);Simultaneous 第二腿填 `HedgeRequest.peer_venue/peer_account`;Hedge 类填产生方给出的 `hedge_cap_qty`(`HedgeRequest.hedge_cap_qty`、`ExecOrder.hedge_cap_qty`,策略无链接 Hedge 则取 `PosView::hedge_cap_qty(side)`,规则归 hft-risk 分类)。
5. 先从当前 OMS 腿摘要刷新本次意图(含整对准入的另一账户)的风控缓存;这里只刷新读数,cap 副作用仍在回报排空之后处理。`risk.evaluate(&view, &RiskViewAdapter, now)`(只读)。
6. Shadow 或被拒:只写 `OrderDecision`(Shadow 同时保留 `SHADOW` 与 evaluate 得到的 `reject`,通过为 0),**不扣令牌、不计在途、不分配槽**。`HEDGE_LIMIT` 是非 Shadow 单的终局拒绝:OMS 来源调 `oms.hedge_vetoed(link_id)`(链接 `Broken{HedgeLimit}`)、执行层来源经 `on_place_result(Rejected(HEDGE_LIMIT))` 回填,两者都 `apply(Auto(Broken{account, group}))` 并写 P0 注记(执行层来源先回填、后 `apply`:执行层据此进入 `Broken(HedgeVeto)` 并写 P0 `ExecBroken`;同一 Input 内对同一 (account, group) 的 `apply(Auto(Broken))` 与随后的 `on_legs` 只做一次,执行层回填产生的 `BrokenReq` 不重复);OMS 对冲请求被 `HEDGE_LIMIT` 以外的位拒绝 → `oms.hedge_gated(link_id, HedgeGate::Risk(bits), retry_at, …)`(`retry_at` = 拒绝位为 `RATE_BACKOFF` 时 `now.tsc_of(risk.backoff_until(该腿 venue, account))`,其余为 0)(门控:不计次、截止照算),执行层订单的其他拒绝经 `on_place_result` 回填(Flatten/ForceStep 被 #5/#6 拒绝的处理归 hft-exec)。
7. 放行:`oms.place`(`ApprovedIntent` 带本 Input 的 `TrigStamps`;带 `LegLink{role = First}` 的意图填 `hedge_ref_px`(取法归 [hft-oms](../hft-oms/README.md#输出-omsout容量固定由-sequencer-预分配并在每个输入事件后清空)),OMS 对冲请求填第 1 步的 `price_hedge` 结果 `hedge_px`,其余为 0;永续 Open 带 `lev_x100` = `risk.leverage(…)` 换成百分之一倍、向下取整(与 `Views::leverage` 同一换算),未知为 0,供 `InflightBook.open_margin` 增量维护;冒烟单以 `origin = Command` 与 `OrderFlags::SMOKE` 建槽,规则归 hft-oms)→ 分配成功后 `risk.commit` → `OrderDecision` → 出环(`AccountMeta.kind = Sim` 的账户推 `out-sim`,否则 `out-<venue>`)。
8. 结果回填到发起方(策略的 `IntentHandle`:`Placed | Rejected(bits) | Shadowed`;执行层的 `PlaceResult`;冒烟单 → 该命令的 `CmdResult`)。

### 目标意图与执行层

`Ctx::target(TargetIntent)` 替换该 (account, group) 的当前目标,`clear_target` 清除;sequencer 以发出它的实例为 `owner` 调 `exec.set_target`/`exec.clear_target`,校验(实例绑定的账户、group 归属、腿与算法)由执行层完成;执行层看不到 `open_allowed`,所以 cell 有效模式为 Live、该 group `open_allowed = false` 且目标使某腿离开当前净量(增仓或变号)时,sequencer 给该次 `TargetSet`/`TargetReplaced` 记录补 SHADOW 标志、结果回填 `TargetShadowed`;处理带 `reduce_only` 的 `StrategyCmd` 那次 `on_event` 产生的目标以 `ro = true` 调 `set_target`(只减仓判定归 [hft-exec 目标的接受与替换](../hft-exec/README.md#目标的接受与替换));拒绝时执行层写 `ExecDecision{TargetRejected}` 并通知策略。目标的推进、切片、腿顺序、限价与单笔名义、对冲量上限、补腿、外来成交默认处理、人工保持、L3 默认减仓(含另一腿不可用时的串行退路与 `GROUP_NONE` 永续持仓)全部归 [hft-exec](../hft-exec/README.md);执行层的每次决定写 Output `ExecDecision`。执行层状态是 sequencer 状态的一部分(进快照)。有效模式变化调 `exec.on_mode`;`HedgeFix` 命令调 `exec.hedge_fix_cmd`;`Kill{Account, flatten}` 调 `exec.engine_flatten`(执行层默认减仓路径:引擎目标 0、`reason = Hard`、`PerpFirst`);它与保证金 L3 的 `exec.force_deleverage` 一样**覆盖处于 `Broken` 的 group**(先对永续腿 reduce-only 减仓,再处理现货腿)。设 `reason ∈ {MarginStress, Hard}` 的引擎目标时,执行层经 `ExecOut.cancel_resting` 请求撤该 inst 上本账户全部非 EXEC 挂单,sequencer 调 `oms.cancel_resting_on`(见 `process` 第 6 步;撤单确认前的截量规则归 hft-exec)。`Broken` 持续 `broken_auto_fix_ms` 且无人工 `HedgeFix` 时,执行层用自己的 `Timer{Exec, AutoFix}` 发起自动有界平仓(HARD 下不执行);sequencer 只按上面的 `ExecOut` 规则处理其订单与完成报告。cap 侧规则见 [hft-risk 断腿](../hft-risk/README.md#断腿broken自动有界平仓l3-与-flatten)。`Draining` 期间执行层不发增仓,只完成减仓与进行中的链接。

### Advice 路由

advice 环元素 = `AdviceHdr { strategy, account, local_kind, group, len, basis_seq, basis_tsc }` + `payload[len]`(布局与字段顺序归 [hft-event](../hft-event/README.md))。作为 Input 原样写进 journal(变长)后:

1. 实例 = `instances[hdr.strategy]`;不存在、`len` 越界、`hdr.group = GROUP_ALL`、`hdr.group ≠ GROUP_NONE` 且其高 8 位 ≠ `hdr.strategy`(按 `GroupId` 构造规则不可能属于该实例)、或 `hdr.account ≠ 0` 且 `AccountMeta(hdr.account).bound_instance ≠ hdr.strategy` → 丢弃并写 `Note{AdviceMisrouted}`。
2. `gix`:`hdr.group = GROUP_NONE`(实例级载荷)→ `GIX_NONE`;否则该实例 `group → gix` 现查,查不到(group 已被移除,或尚未生效)→ 丢弃并写 `Note{AdviceStale}`。
3. **group 规格变化**:每个 group 记 `spec_seq` = 使其当前 `GroupSpec::fingerprint()`(SDK;资产与各腿)生效的那条 `SetParams` 记录的 seq(新 group、或指纹与上一参数版本不同的 group 在应用时更新;指纹不变不更新;进快照节 5)。`hdr.basis_seq < spec_seq(实例, group)` → 丢弃并写 `Note{AdviceStale}`:该建议依据的是旧资产或旧腿。`GROUP_NONE` 不做此项。
4. 只投递给该实例:`EventRef::Advice{gix, local_kind, payload, basis_seq, basis_tsc}`。**引擎不解释载荷,也不按时长判过期**:策略用 `now().tsc − basis_tsc` 判断建议是否过期(阈值是策略参数);回放中两者都来自 journal,结果确定。**TSC 从不跨启动比较**:`basis_seq < now().boot_seq` 的 Advice(本次启动之前的状态算出的建议,如检查点续算的追赶期)由策略判为过期,直到 Model 产出启动后的建议(规则归 [hft-strategy-sdk](../hft-strategy-sdk/README.md));sequencer 只做上面的路由检查,不给 Advice 加任何过期标志。

### advisor 与 Model

1. **镜像**:`Advisor` 启动时 `mirror` 从 journal 中的完整 `StateSnapshot`(引导块)起步,`MirrorDriver` 按 seq 顺序把 Input 记录喂给同一份 `Sequencer` 代码(Output 不比对、不出环;Meta 跳过),于是镜像在每个 seq 处的状态与 sequencer 处理完该 Input 时相同。镜像的 `SeqViews` 实现 `ModelViews`。追赶跨越进程重启或 build_git 变化时,上一批止于新引导块前,随后从新引导快照和其记录配置重建引擎,保留身份匹配的 Model;参数引用变化时重新应用该处参数。普通滚段继续当前镜像。重建失败存入 `Advisor.failure`,后续 poll 不再推进,调用方必须按失败处理。
2. **Model 输入**:每条被镜像应用后的记录,按 SDK 的投递表(Model 收哪些变体以 [hft-strategy-sdk EventRef](../hft-strategy-sdk/README.md#eventref) 为唯一定义;`model_tick` 刻度来自 journal 中的 `Timer{Sys, AdvisorTick}`,被 Applied 的 `InjectWarmup` 见下)构造 `EventRef`,调 `S::model_on_event(&mut m, &ev, &mirror_views, &mut out)`。`now()` = 该记录;Model 不读系统时钟、不做 I/O、不依赖 `HashMap` 迭代——同一检查点 + 同一段 journal 产出同一序列的 Advice 载荷。
3. **参数**:镜像应用 `SetParams`/`ReplaceRefdata` 后调 `S::model_on_params`;返回 `ModelParamEffect::ResetGroups(…)`/`ResetAll` 时,advisor 对每个被重置的 group 写一条 Meta `Note{source = Advisor, code = ModelGroupReset}`(`v[0]` 原因码:`ResetAll` 为 4,`ResetGroups` 的原因随 SDK 返回值给出;`v[1]` = 触发它的参数 version;字段以 hft-event 为准;pipeline 据此对它们自动预热)。重启追赶时镜像重放同一条 `SetParams` 会再写一次相同的注记,消费方按 (strategy, group, `v[1]`) 去重;新实例 `S::create_model`。
4. **预热注入**:镜像应用一条 Applied 的 `ControlCmd{InjectWarmup{instance, local_kind, chunk_no, last, payload}}` 后,向该实例的 Model 投递 `EventRef::Warmup{local_kind, chunk_no, last, payload}`;载荷含义、分块约定与"预热完成"的判断归策略(SDK)。它是 journal 的 Input,所以重放与检查点续算时同样被投递,结果确定。
5. **输出与背压**:`AdviceOut` 的每条填 `AdviceHdr.basis_seq` = Model 已消费的最后一条 journal 记录的 seq、`basis_tsc` = 该记录的记录头 tsc(replayer 据 `basis_seq` 复核 Model 确定性,策略据 `basis_tsc` 判过期),进 `outbox`;outbox 按 (strategy, account, group, local_kind) 只留最新一条(后到覆盖先到,覆盖次数计入 `advice_coalesced`,以 Meta `Note{AdvisorStats}` 周期导出)。advice 环是变长记录的字节环:环剩余空间放得下下一条时才推入,放不下的留在 outbox 等下一次 `poll_once`;从不阻塞,也不丢弃环内已入队的记录。
6. **时间片**:每次 `poll_once` 至多处理一批记录(连同其 Model 调用;单次 Model 调用的时间上限归 SDK),或写检查点的一块,总计 ≤ 1 ms。
7. **内存与检查点上界**:各实例 Model 的内存与检查点上界是 SDK `Model::mem_bytes(params, n_groups)` 与 `Model::snapshot_bytes(params, n_groups)`;本 hft-core 实例全部策略实例的内存之和 ≤ `advisor_mem_budget_mib`、检查点两代合计 2 × Σ ≤ `advisor_ckpt_max_mib`(ECF1 段 5);装配时校验归 apps/hft-core,运行中由 `SetParams` 复核(见[命令路径](#命令路径))。
8. **advisor 落后量**:`AdvisorLag` 的口径(按 `engine_ns` 计的滞后;超过 `advisor_max_lag_ms` 期间每个实例每 10 s 至多一条 Meta `Note{source = Advisor, code = AdvisorLag}`)唯一定义在 [hft-event](../hft-event/README.md#控制与系统0x04xx0x05xx);本 crate 的 advisor 按该定义,用已提交末条记录与该实例 Model 已消费末条记录的 `engine_ns`(镜像持有同一基准)计算并经 advice 环写出,告警归 [watchtower](../../apps/watchtower/README.md)。写检查点期间的停顿计入落后量。
9. **模型检查点**(只是预热优化,不是正确性条件——Advice 是 Input,回放与验证从不需要 Model 状态):见下节。
10. advisor 的输出是 journal 记录的 Input,回放从日志读取,advisor 自身的时序不影响确定性([13 R10](../../design/13-设计评审与优化.md))。

Model 从检查点恢复、镜像尚未追到其 `last_seq` 时,落后量以该检查点已经消费的位置计算;该处 `engine_ns` 从对应 journal 记录及其运行的 `EngineClock` 取得,不采信检查点存储适配器的时间提示。镜像追到后再使用实时消费位置,各实例分别计算。

### 模型检查点

检查点键的组成与失效规则归 [hft-strategy-sdk](../hft-strategy-sdk/README.md)(含 `LAYOUT_VERSION` 与路径状态节 `Model::PATH_VERSION` 的纪律;参数版本、refdata 版本、二进制 SHA **不**使检查点失效)。文件 `<strategy_instance>.ckpt` + 上一代 `.prev`、文件头 `CkptHdr` 的格式、目录、周期 `advisor.model_checkpoint_ms`、块大小 `advisor.checkpoint_chunk_kib` 与磁盘上限归 [apps/hft-core](../../apps/hft-core/README.md)。本 crate 的 advisor 实现读写流程:

版本键经 `S::model_layout_version(kind_id)` 读取实际 `Model::LAYOUT_VERSION`;布局描述的内容或调试文本不充当版本键。`advisor::checkpoint::{encode, decode}` 是该文件格式的共用冷路径编解码器,不做 I/O;`decode` 验证完整文件后借用路径节与样本节,大小上限由调用方传入。`Decoded::header(last_ens)` 转为 advisor 的运行期元数据,其中 `last_ens` 必须取自 journal 的 `last_seq`,不占用文件保留字段。路径节首块按文件格式写出版本和长度;文件发现、读取前的大小检查、fsync、rename 与两代保留由 app 负责。

1. **写(分块)**:按事件时间每 `advisor.model_checkpoint_ms` 与 Drain 时,对每个有 Model 的实例依次:`ckpt.begin(hdr)`(`last_seq` = 该实例 Model 已消费的最后一条记录)→ 每次 `poll_once` 调一次 `S::model_snapshot_chunk(&m, from_gix, &mut w, max_bytes)` 并 `write_chunk` 一块(`max_bytes` = `advisor.checkpoint_chunk_kib` × 1024;路径状态节作为第一块写出,分节规则归 SDK)→ `SnapshotProgress::More(next_gix)` 时下次从 `next_gix` 接着写,`Done` 时 `commit`。**写一份检查点的全部块期间,整个 advisor(镜像与它承载的全部 Model)停在 `last_seq`**:不读 journal、不投递任何事件——镜像是共享的,只停一个 Model 而让镜像前进会使它错过事件、续算不再确定;停顿时长 = 块数 × ≤ 1 ms,计入 `AdvisorLag`,写完后照常追赶。写失败(含超出磁盘上限、两节合计(路径状态节前缀的 8 B 不计)超过 `snapshot_bytes`)→ 本代作废、上一代保留,写 Meta `Note{ModelCkptSkipped}`(P1,`v[0]` = strategy、`v[1]` = 已写字节、`v[2]` = 声明的 `snapshot_bytes`),下周期重试。
2. **读**:advisor 启动时对每个实例 `ckpt.load`;键中 `last_seq` 以外的各项与当前实例一致、`last_seq` 在可读的 journal(热段或冷副本)内、校验和正确 → `S::model_restore`(路径状态节版本不符时只丢该节并写 `Note{ModelPathReset}`,规则归 SDK),然后按 SDK 规定以 `last_seq` 处生效的参数调一次 `S::model_on_params`,镜像从不晚于 `last_seq` 的快照起步、只重放到 `last_seq` 而不投递给该 Model,此后照常投递(含 `SetParams` → `model_on_params`、`InjectWarmup` → `EventRef::Warmup`)。
3. **丢弃**:任一条件不满足 → `S::create_model` 空状态起步,写 Meta `Note{source = Advisor, code = ModelCheckpointDiscarded}`(`v[0]` = strategy,`v[1]` = 原因码,码表在 hft-event);随后靠预热注入或实时样本就绪。
4. **模型 tick 的续算**:从检查点续算时,镜像在投递 `last_seq` 之后的第一条 `Timer{Sys, AdvisorTick}` 之前,先取 journal 中 `last_seq` 及以前最近一条 `AdvisorTick` 的 UTC 作为"前一条"(SDK `model_tick` 的跨界判定),所以续算与连续运行在同一条记录上投递;`last_seq` 以前没有 `AdvisorTick` 时按 SDK 规则不投递第一条。冷启动(无检查点)从镜像起点的快照起同样按此取前一条。
5. journal 热段保留时长应覆盖 `advisor.model_checkpoint_ms`(冷段也可读,只是更慢);追赶期间产出的 Advice 的 `basis_seq` 早于本次启动的 `EpochStart`,策略按 `basis_seq < boot_seq` 判为过期([Advice 路由](#advice-路由)),不会被当成新的。
6. **参数边界**:有待恢复参数的 Model 时,镜像每批至多消费到其中最早的 `last_seq`,在该边界应用参数后才允许下一条 Input。不同实例按各自边界解除等待;边界恰在起点快照时先应用再处理首条后续输入,边界落在 Output/Meta 时以该前缀的 Input 全部处理完为准。批大小不改变 Model 的调用顺序或 Advice。

### 资金费合成

`VenueCaps.funding_settle ∈ {BalanceOnlyPush, RateOnly}` 的 venue 由 sequencer 合成每 inst 的资金费(纯协议的 `venue-*` 不合成):

1. `view().next_funding_settle()` = 这些 venue 上有持仓、尚未结算的 inst 的 `MarketLast.next_funding_ns` 的最小值及其 venue;实盘驱动在 UTC 到达该值时写 `Timer{Sys, FundingSettle{venue}}`。
2. 触发(两者先到者,每个结算时刻只结算一次):`Timer{Sys, FundingSettle{venue}}`;或某 inst 的 `Funding` 事件把 `next_funding_ns` 推过了一个尚未结算的结算时刻。
3. 对该 venue 每个 `next_funding_ns ≤ 当前结算时刻` 且未结算的 (account, inst):`mark` = `MarketLast.mark_px`,`rate` = 该期费率(`MarketLast.funding_rate`),`settle_ts_ns` = 该结算时刻(`synth_funding` 入口内经 hft-types [`funding_settle_norm_ns`](../hft-types/README.md#资金费结算时刻归一化) 按 [hft-ledger `funding_grid`](../hft-ledger/README.md#资金费时刻归一化) 归一化,Output 里是归一化后的值),`ledger.synth_funding(a, inst, mark, rate, settle_ts_ns)` → 追加 Output `FundingSettle{synth = 1}` → `ledger.apply_funding` → `risk.on_pnl`;记 `settled_cut_ns[inst]`。
合成的 `FundingSettle{synth = 1}` Output 记录体的 `rx_tsc` 写 0(合成不是接收,写真实 TSC 会使验证回放比对不确定;字段归 hft-event)。
4. `BalanceOnlyPush` 所的推送(`inst = INST_NONE`)按推送实额入钱包并去重,交易所流水只做校验,规则在 [hft-ledger 资金费](../hft-ledger/README.md#资金费);逐笔与钱包差额由对账报告的 `Funding` 条目经 `ledger.reconcile_funding` 自动校正(不进启动闸门)。

### 模式与 cap 的驱动

模式状态的唯一归属是 [hft-risk `mode`](../hft-risk/README.md#mode-模块唯一的模式状态);语义与优先级表在 [control-plane](../../docs/subsystems/control-plane.md#模式与上限的统一优先级)。sequencer 只把 Input 翻成 `ModeInput` 调用 `risk.mode.apply(inp, seq, &view, &mut outcome)`(`outcome` 在启动时按 `max_cells_total`、refdata 账户数与 (account, venue) 数预分配一份,各向量容量取构造上界、不会溢出,[hft-risk mode](../hft-risk/README.md#mode-模块唯一的模式状态)),再执行 `ModeOutcome`:

| Input | `ModeInput` |
|---|---|
| `SetMode` / `Kill` / `Resume` 命令 | `SetMode` / `Kill` / `Resume` |
| `on_pnl` 越线、`on_reconcile` 判定、OMS 输出中 P0 级的 `Note{OmsUnknown}`(→ `UnknownTimeout{account, venue}`,同一 Input 内)、链接 `Broken`(含续接超时)、`HEDGE_LIMIT` 拒绝、执行层 `BrokenReq`、`on_legs` 的 `AutoClear{Broken}`、`Ctx::request_halt`、冷启动、`Drain`/`ReplaceRefdata`、`on_margin` 档位 | `Auto(CapRecord)` / `AutoClear` |
| `Timer{ClockOk\|ClockBad}` | `Clock{ok}` |
| `Timer{PersistLag\|DiskLow\|StorageOk}` | `Storage{kind, level}` |
| `VenueNotice` 命令 | `Notice` |
| `ConnState`、簿 Live、Unknown 计数、对账组、`AdoptExchange`/`SetBalanceBaseline` 与 `AUTO_EXTERNAL` 的处置、`AccountConfigReport`、`Timer{KillSelfTestOk}` | `Gate(…)` |
| 恢复结束时的账户标志 | `RecoveryFlags` |
| 该 cell 的成交、撤单终态、LegTracker 更新后 flat 变化 | `CellFlat` |
| `SetParams` 应用后的 group 增删 | `GroupsChanged` |

执行 `ModeOutcome`(同一 Input 内它总在全部 `exec.on_order_update` 与 `exec.on_link` 之后,见 `process` 第 3 步):① `cells` 中每个变化向该 cell 的策略入队 `EventRef::Mode{old, new}`(实盘实例先),进入或离开 Shadow 的 cell 另调 `ledger.set_cell_shadow`(外来成交归属只在非 Shadow cell 中找);变成 `CloseOnly{deferred_shadow: true}`(推迟落 Shadow)或由它落到 `Shadow` 的变化写 `Note{ShadowDeferred}`(`v[2]` = 0 / 1);② `cancels` 中每项 `{scope, open_only}`:cell 范围调 `oms.cancel_open(cell)`,其余调 `oms.cancel_all(scope, open_only)`(`open_only` 由 hft-risk 给出:非 HARD 的 Kill 为 true,HARD 的 Kill 为 false);③ `kill_changed` → `LiveDriver` 把 `risk.mode.kill_bits()` 写进各 gw 的 `KillWord`(`Release`;模拟账户不经 gw,kill 对它们与实盘一样经检查链生效);④ `resolve_broken`(hft-risk 只在删除带 HARD 的 `Kill` 记录时产出)→ 按游标调 `oms.naked_groups(account, &mut buf, &mut cursor)`(每次 ≤ 64 个 group;本 Input 内取一批,未取完的游标经[续推](#通知队列与续推)在下一轮接着取),对每个 group 调 `exec.resolve_broken(a, g, &views)`(补腿口径:无活动链接(`Open`/`Hedging`)、已成交裸量超过补腿阈值且高于尘埃线、跳过 `manual_hold` 的 cell;判定归 [hft-exec](../hft-exec/README.md)),判定为真的 group 再 `apply(Auto(Broken))` 并按第 7 步调 `on_legs`,**不产生任何自动 HedgeFix**(有活动链接的由 OMS 按 `hedge_deadline_ms` 判定);非 HARD 的 `Resume{Kill}`、其他锁存 cap 的解除与 Recovery 的解除都不产出 `resolve_broken`;⑤ `hedge_gate_open` 中每个 (account, venue) → `oms.on_hedge_gate_open(a, v, now, seq, &mut out)`(续接单(第二腿或 `Reverse` 的第一腿)在该 venue、处于 `RESUME_HOLD` 的链接续接一次,产出的 `HedgeRequest` 照常进意图管线;规则与 `resume_hold_max_ms` 超时归 [hft-oms](../hft-oms/README.md));`venue_gate_open` 中每个 (account, venue) → `exec.on_venue_gate_open(a, v, now)`(多出那一腿在 `v` 上的 cell 据此开始补腿、残余修剪、外来成交默认处理与自动有界平仓,并重设其截止;条件 `venue_gate_ok` 归 [hft-risk 启动闸门](../hft-risk/README.md#启动闸门),执行规则归 hft-exec);`released`(启动闸门刚放行的账户)→ `exec.on_gate_open(a, now)`(规则归 hft-exec)——冷启动不立刻判 `Broken`;⑥ `risk_events` → 该账户绑定的实例入队 `EventRef::Risk`,并调 `exec.on_risk`;有效模式变化另调 `exec.on_mode`;⑦ `drain` → 发起内部 Drain(`StorageCritical` 见[存储降级](#存储降级),是否推迟已由 hft-risk 判定;`IdentityMismatch{account, venue}` = `AccountConfigReport` 报 UID 不符(闸门放行前或放行后),hft-risk 已设该账户 HARD,sequencer 写 P0 `Note{IdentityMismatch}`);⑧ `gate_changes` 每项写 `Note{StartupGate}`(`v[0]` = account,`v[1]` = 旧 gate_bits,`v[2]` = 新 gate_bits,`v[3]` = released);`notes`(`CapSet`/`CapCleared`/`RecoveryHeldSim`)原样追加。`VenueNotice` 另把 `EventRef::Notice` 投递给订阅该 inst 的策略。

**与 hft-risk 的接口**(各方法与字段的语义归 [hft-risk](../hft-risk/README.md)):构造与 `restore` 时传 `ModeCfg{auto_resume_after_clean_drain, allow_missing_standby}`(前者取 ECF1 段 5,后者取 hft-core 配置);`mode.apply` 的视图是本 crate 的 `ModeView` 适配器——`unknown_count` 读 OMS、`engine_reduce_pending` 转调 `exec.engine_reduce_pending()`、`lev_bad` 读 hft-risk 自己的 `on_groups_changed` 结果,其余按 hft-risk 的方法表从 ledger、OMS、执行层取值;`ModeOutcome.daily_loss_resumed` 中每个账户在同一 Input 内调 `risk.on_daily_loss_resumed(a, ledger 当前 day_pnl)`;`AccountConfigReport` 的 `Gate(AccountCfg{hdr, keys_required})` 中 `keys_required` = 装配时 hft-core 载入的该 (account, venue) 交易 key 集合;交易所限速拒单(`ConnState` 带 `RATE_LIMIT` 提示,产出方归 [hft-venue](../hft-venue/README.md))时调 `risk.on_rate_limit_hit(v, a, &now)` 并把返回的 `Note{RateLimitHit}` 追加到本 Input 的输出。

**撤单持续义务**:`Kill{cancel_resting}` 或 `cancel_resting = true` 的 `Draining` 生效期间(`Draining` cap 的 `params[0]` 记录该 Drain 的 `cancel_resting`,登记在 [hft-risk cap 表](../hft-risk/README.md#cap-记录),随节 4 进快照;为 false 时只按 Drain 第 2 步撤带 LegLink 的挂单第一腿),每次 `ConnState{Up}` 或某槽 Unknown 收敛为 Live,sequencer 对受影响范围调 `oms.reissue_cancels(scope, open_only)`(`open_only` 与最初的撤单相同:非 HARD 的 Kill 为 true,HARD 的 Kill 与 `Draining` 为 false)。

### 模拟账户

`AccountMeta.kind = Sim(SimAccountCfg)` 的账户以正常 `RunMode` 运行,与实盘走同一条管线;差别只在出入环与优先级:订单类 `GwCmd` 推 `out-sim`,sim-gw(warm 线程,[hft-sim](../hft-sim/README.md))撮合后把回报写入 `rep-sim`,sequencer 照常记为 Input——回放与验证不区分模拟与实盘。**模拟让位于实盘**:rep-sim 是优先级最低的输入环、每轮 ≤ `sim_max_per_round`(默认 32);同一事件派发时实盘实例先于模拟实例,执行层推进也先实盘 cell。模拟账户只在 `sim` 实例里运行(同一二进制、只含 Sim 账户、自己的行情连接与 journal;统一升级后与 `prod` 同机,见 [apps/hft-core](../../apps/hft-core/README.md));`sim` 实例里没有实盘输入,让位规则不起作用,保留为同进程混跑(测试)时的防御。模拟账户不占 kill 字位,kill 与 HARD 对它们和实盘一样经检查链 #1/#3 生效(没有另一种写法)。模拟账户不登记 `DeadMan`、`ConnRotate`、`SessionKeepalive` 定时器,不参与对账;启动闸门的 `RECONCILED`、`ACCOUNT_CFG`、`IDENTITY` 对模拟账户视为已满足(没有交易所),其余位照常([hft-risk 启动闸门](../hft-risk/README.md#启动闸门))。

### 输出扇出

一个 Input 产生的出环消息最多发 `max_out_per_event`(K)条;超出部分留在 OMS 的待发队列(状态,进快照),`view().wants_continue()` 为真,实盘驱动下一轮最先写 `Timer{OmsContinue}`,每个 `OmsContinue` 再发至多 K 条。Kill 与 `Drain` 优先用 venue 级 `GwCmd{CancelAll}`(每 (venue, account) 一条)。扇出队列按 (venue, account) 分配额、`RISK_REDUCING` 优先、账户之间轮转(规则在 [hft-oms 输出扇出](../hft-oms/README.md))。

### 通知队列与续推

固定容量的结构从不丢弃数据:

1. **`ModeOutcome`**:各向量容量取构造上界(`cells` = `max_cells_total`),一次 `apply` 不可能溢出(hft-risk 保证)。
2. **`NotifyQueue`**(每个 Input 128 项,分区见 `process` 第 3 步):某分区满时,该 Input 其后产生的通知按产生顺序进 `NotifyBacklog`(容量 `4 × U`,`U = max_cells_total + 128` 是单个 Input 的通知上界,启动时分配);模式分区溢出后,随后产生的账户级 cap 通知也进入积压。积压非空 → `view().wants_continue()` 为真,实盘驱动下一轮最先写 `Timer{Sys, NotifyContinue}`;每个 `NotifyContinue` 先按产生顺序取至多 `notify_per_continue` 条,再按实例调度顺序投递(实盘实例先、同实例内顺序不变),其 `Ctx` 照常处理。
3. **背压**:轮首积压余量(`view().notify_headroom()`)< 2U 时,驱动这一轮先写 `NotifyContinue`,随后**只取实盘回报**,且每取一条之前要求当时的余量 ≥ U(单条回报最坏也放得下);ctl、bbo、advice、md、rep-sim 这一轮不取。余量恰好 2U 不进入背压轮,恰好 U 允许取实盘回报;下一轮重新判断。对冲腿回报因此不被续推饿住;只在连续的大面积模式变化时出现,至多数轮。
4. **`resolve_broken` 扫描**:`oms.naked_groups` 每次至多 64 个 group,游标未完时同样经 `NotifyContinue` 续取。
5. 积压与游标是状态,但 `Sys{Snapshot}`/`Sys{Roll}` 只在 `wants_continue()` 为假时写、Drain 等它们为空才写快照,所以不进快照。积压清空只解除续推约束,周期快照与软滚段仍须满足各自的空闲条件;Drain 在最后一批通知投递并满足退出条件的 Input 内完成。OMS 的待发队列另由 `OmsContinue` 续发并进快照([输出扇出](#输出扇出))。

### journal 满

`hft-journal` 为每段预留 **2 × 引导块** 的空间;段用量超过其硬上限时实盘驱动在下一个事件边界写 `Sys{Roll}`(软阈值只在空闲轮滚动,见[环境决定写成 Input](#环境决定写成-input))。若仍出现 `AppendErr::Full`,`step` 返回 `Fatal(JournalFull)`,hft-core abort——不在没有记录的情况下继续交易;挂单由交易所死手与 kill-switch 处理,重启后按崩溃恢复。存储降级的早期信号走 `Storage` cap(见下),不等到写满。

### 出环满

`LiveIo::out_free` 返回单生产者下的保守空闲槽数;后续相应数量的 push 必须成功。`OutRecord::Pair` 同时检查两个环(同一 sim 环需两槽);任一不足时两腿都不推,各产生一条 RingFull Input。此容量检查位于 journal 发布后,不从未记录的环状态生成交易决策。

`drv.emit` 失败时,驱动构造一条**合成 Input** `SendReport{status = RingFull}` 作为下一条交出(像其他 Input 一样被记录),OMS 据此把槽迁到 `Rejected(LOCAL_SEND_FAIL)`。该路径只应在 gw 线程停摆时出现,并伴随 P0 告警。`feedctl-*` 环满时不合成任何 Input(`FeedCmd` 不是 journal 记录,不影响状态):`LiveDriver` 把推不进的 `FeedCmd` 按 (venue, inst) 合并暂存(同一 inst 只留最新一条,容量 = `max_insts`),每轮开头先按原顺序续推,所以启动时每个订阅 inst 各一条 `Subscribe` 的突发不受 `feedctl` 环容量限制。

### inst → 策略的稠密索引

订阅 inst 在首次订阅时得到稠密下标 `sub_ix`(`InstId → sub_ix` 映射按 refdata 宇宙预分配,u16;下标进程生命周期内不回收)。CSR:`sub_off: [u32; max_insts + 1]`、`sub_ent: [(instance_ix: u16, gix: u16); max_insts × max_subs_per_inst]`;行情事件经 `sub_ix` 直接下标,行内顺序按 `(dispatch_order 中的位置, gix)` 升序——先实盘实例、后模拟实例(确定)。由各实例 `subscriptions()` 在启动与 `SetParams` 改变订阅时原地重建(命令路径,O(总订阅数),不分配)。**行情订阅随之下发**:重建时对每个 inst 求全部实例(快速部分与 Model)订阅的 SDK `EvMask` 之并,再并上**引擎自身的订阅**(持仓驱动):① 任一已绑定 GroupSpec 的交易腿含 `BBO`,永续交易腿另含 `MARK` 与 `FUNDING`;② 本实例账户中 `InstPos.qty ≠ 0` 的永续 inst 含 `BBO`、`MARK`、`FUNDING`,有现货库存(现货 `InstPos.qty ≠ 0`)的 inst 含 `BBO`——含核销、外来成交与人工交易留下的账户级无 group 持仓;③ EngineConfig 段 2 的每个 `ref_inst` 含 `BBO`。ledger 的保证金、未实现与换算率、资金费合成、MarginStress 与 L3 依赖它们;只为引擎订阅、策略未订阅的事件不投递给策略。并集在启动与 `SetParams` 改变订阅时重算,ledger 使某 (account, inst) 的 `InstPos.qty` 在 0 与非 0 之间变化时在同一 Input 内重算该 inst;只由 ②③ 引入的 inst 同样占 `sub_ix`(CSR 行为空),`max_insts` 用尽时该 inst 不订阅(永续持仓因此没有标记价,hft-risk 按标记价断流至少 L1 并告警)。与上次下发的值不同时出环一条 `FeedCmd::Subscribe{inst, streams = 新并集}`(首次订阅与并集变化都重发;并集变为空 → `Unsubscribe{inst}`;字段归 [hft-event](../hft-event/README.md#环元素类型),按位分发到 feed-bbo/feed-md 的规则归 [hft-venue](../hft-venue/README.md));上次下发的并集不进快照(只在本次运行有效),启动时每个并集非空的 inst 各发一次。group 级事件用 `group → (instance_ix, gix)` 的同构索引。

### 定时器

策略定时器的投递键为 `(strategy, account, GroupId, tag)`(不用 `gix`);投递时现查 `gix`。每次登记有独立的 SDK `TimerId`,同 tag 的登记各占一格;取消只移除指定 ID 对应截止的一条,到期只移除该截止的一条登记。被 `SetParams` 移除的 group 的定时器在应用参数时撤销并写 `Note`。

**调用时预留**:每轮按既定实例与回调顺序共享一份剩余策略配额,行情、通知和直接投递都借入同一计数。成功的 `timer_at` 立即预留;有效取消按调用顺序释放一次。实例的多个回调共用本轮 Ctx 缓冲,不重复初始化配额。所有回调结束后仍按原顺序应用 Ctx;成功预留却无法入堆属于 `TimerOverflow`,不能静默丢弃。事件缓冲和配额的满计数都在每实例排空时汇总一次。登记表按 `max_timers` 在每实例创建时预分配,保持 TimerId 升序,保存本次运行的截止;跨重启时截止与堆使用同一公式平移。验收见 [SEQ-350](TESTS.md#seq-350-策略定时器超出共享配额时-timer_at-返回-ctxfull不进堆) 与 [SEQ-407](TESTS.md#seq-407-同-tag-的策略定时器按登记身份取消并按调用顺序释放配额)。

**身份持久化**:节 27 按 StrategyId 升序保存每个绑定实例的 `next_id` 与按 TimerId 升序的登记行,包括没有参数或当前没有定时器的实例。ID 从 1 开始,成功分配后递增;取消和到期不回退计数器,`u64::MAX` 表示耗尽。恢复时登记行的 `(deadline_tsc, owner, tag, group)` 多重集合必须与节 8 的全部策略定时器完全相同,相同截止/tag 的两次登记保留两个 ID。实例集合、排序、填充、版本、计数器或集合对应关系错误均拒绝快照。节 27 的截止与节 8 同属已登记 TSC,恢复结束时一起平移,ID 原样保留。

**跨重启(停机时间不计)**:堆以本次运行的 TSC 为键(`Timer` 体的 `deadline_tsc` 同口径),随快照节 8 保存(同一快照的节 25 给出该运行的时钟基准)。[恢复结束](#冷启动恢复recoverdriver)时,堆内每个条目与 OMS、执行层的已登记 TSC 字段按**同一平移公式**处理——时长按两次运行的 `tsc_hz_milli` 换算,过去的时刻落到 `now` 之前(恢复后立即到期)、将来的截止保持剩余时长、0 保持 0;公式的唯一定义在 [hft-exec 快照与恢复](../hft-exec/README.md#快照与恢复),OMS 字段的平移归 [hft-oms](../hft-oms/README.md)。执行层的定时器另经 `exec.recovery_timers()` 按平移后的时刻重新登记(与堆中同键 (owner, timer_id, group) 的条目合并、保留较早者;早到的由执行层忽略)。周期性系统定时器(OmsSweep、DeadMan、ConnRotate、SessionKeepalive)每次启动都重新登记,替换快照堆中同键 (owner, timer_id, group) 的旧条目(每键至多一条);例外:快照堆中平移后的 `OmsSweep` 早于本次登记时刻 + 1 000 ms 时保留它、不替换(重启不推迟 sweep,连续重启下终态槽照常回收)。

**按 engine_ns 记截止的登记方**(hft-risk 的保证金截止与下一步、策略经 SDK `Now::tsc_of` 换算的定时器):登记时 engine_ns → 本次运行 TSC 向上取整,同一运行内不会早到;跨重启平移的取整差可能使其早到极小量,登记方到期时按 `now.engine_ns` 复核,未到则重新登记。令牌桶与退避在 hft-risk 里按 engine_ns 存,重启不需任何处理。

`TimerHeap`:预分配二叉堆(容量 `max_timers`),键 `(deadline_tsc, owner, timer_id, group)`,整序确定;堆内容是状态(进快照),"是否到期"是环境判断,由实盘驱动比较 TSC 后写一条 `Timer` Input(每轮开头,以及环输入之间的中途检查,见[主循环](#主循环step))。周期性系统定时器(`OmsSweep` 1 s;`DeadMan(venue, account)` 周期 = 该 venue `DeadManSpec.refresh_ms`,每次发 `GwCmd{DeadManRefresh, arg: timeout_ms}`(`timeout_ms ≥ 2 × refresh_ms`,取值归 [hft-venue-proto](../hft-venue-proto/README.md));`ConnRotate(venue)` 周期按 hft-venue ECF1 段 6 的计划切换时刻(`ws_max_conn_age_s − conn.rotate_margin_s`,[hft-venue](../hft-venue/README.md)),仅在无 Pending 槽时发 `GwCmd{Rotate}`)以 `Deadline` 形式进堆;`DeadMan` 由 sequencer 驱动,sequencer 卡死时刷新自然停止,交易所侧撤单生效,无接口的所由 kill-switch 兜底。

**容量**:引擎登记方(执行层、OMS、风控、系统周期定时器)在堆中的条目数有构造上界 `engine_timer_demand` = 执行层 `3 × max_cells_total + 1`(每 cell 同时至多 3 种定时器,另加全局唯一的 Continue,[hft-exec](../hft-exec/README.md))+ OMS `Σ links_per_account + Σ slots + 1`(每链接一个 HedgeRetry、每槽一个 UnknownQuery、一个 Sweep,[hft-oms](../hft-oms/README.md))+ 风控 `3 × (account, 永续 venue) 数`(MarginStale、MarginDeadline、MarginForce)+ 系统周期定时器数(上段的登记,按装配出的 venue 与账户计)。`boot` 按装配出的配置计算它并要求 `max_timers > engine_timer_demand`,否则 `BootError::TimerCapacity{demand}`(退出码归 apps/hft-core);策略定时器的配额 = `max_timers − engine_timer_demand` 由全部策略共享,`Ctx::timer_at` 登记会超出它时返回 `CtxFull`(计数,不进堆)。因此运行中堆满只可能是引擎缺陷:`push` 失败 → `StepOutcome::Fatal(TimerOverflow)`,hft-core abort 并进入崩溃恢复。

### 快照

- **段滚动**(`Timer{Roll}`,两个事件之间):旧段写 `SegmentRoll{End}`(Meta)并封存 → `journal.roll()` 切到已预创建的新段 → 新段首写**引导块**,顺序严格按 [journal-format 段滚动与引导块](../../docs/subsystems/journal-format.md#段滚动与引导块):`SegmentRoll{Begin}` → `HostProfile` → `ClockStatus` → `ConfigBlob{boot=1}` × k(refdata、`Engine`(EngineConfig)、每个实例的 `StrategyParams`、`RiskParams`,`sim` 实例另有 `SimConfig`)→ `StateSnapshot{purpose=Boot}` → 每个 Live 簿一条 `L2Snapshot{origin=Keyframe}` → `SegmentRoll{BootDone}` → `publish()`。引导块全部从本地状态导出,不发 REST、不重连,一次写完。`journal.roll()` 返回 `NextNotReady` → 这条 `Timer{Sys, Roll}` 不写任何记录、不改状态(继续用当前段),滚动条件仍成立,实盘驱动在下一个事件边界再写一条 `Timer{Sys, Roll}`;验证与恢复驱动按日志复现:该 `Timer` 事件之后紧跟 `SegmentRoll{End}` 时滚动并重新生成引导块,否则同样不写。
- **关键帧**(`StateSnapshot{purpose=Keyframe}`)**一次写完**:实盘驱动按[环境决定表](#环境决定写成-input)的 `Sys{Snapshot}` 行(时间或事件数到期、空闲轮、最大推迟 2 个周期)交出 `Timer{Sys, Snapshot}`;sequencer 处理它时在同一事件内写完整个快照,随后 `end_event`、`publish`。停顿 ≤ 300 µs,是 [latency-budget §3.1](../../docs/latency-budget.md#31-p999-记录项) 的 p99.9 记录项;写出期间到达的输入在环中等待。两种触发与最大推迟一起给出恢复重放尾部的上界(数值写在 [apps/hft-core](../../apps/hft-core/README.md) 启动步骤)。**关键帧不计算 `state_hash`**:头部该字段写 0(四种驱动相同,逐字节比对不受影响),需要它的读者从节内容按[状态哈希](#状态哈希)的规则计算。恢复、验证与镜像驱动读到 `Timer{Sys, Snapshot}` 时同样一次写完(验证驱动逐字节比对)。
- **L2 关键帧**:每个 Live 簿的 `L2Snapshot{origin=Keyframe}` 由 `Timer{Keyframe, inst}` 在每个 `keyframe_period_ms` 窗口内均摊写出(每簿每窗口一条;引导块另有一条),不在同一时刻导出全部簿。
- **按需**(`SnapshotNow`)、**Drain**、**Recovery** 与引导块的快照同样一次写完。
- **写出**:sequencer 按固定顺序调用各组件的 `snapshot(&self, w)`,每个组件一次写完本 crate 的全部节:[hft-ledger](../hft-ledger/README.md)(节 1、13、22)→ [hft-oms](../hft-oms/README.md)(节 2、3、12)→ [hft-risk](../hft-risk/README.md)(节 4、14、17、18、23)→ 本 crate(节 5、8、9、10、15、16、19、20、24、25、26、27,有成交记录时另有节 28,有保留的对账视图时另有节 29)→ [hft-book](../hft-book/README.md) `BookSet::snapshot`(节 6、7)→ [hft-exec](../hft-exec/README.md)(节 21)。快照体内各节按这个写出顺序排列(节号不要求递增;读者按 `SectionHdr.section` 定位);`state_hash` 按写出顺序对各节内容累积(节 16、28、29 除外;关键帧不算、写 0),写完全部节后与 `n_sections` 一起回填 `SnapshotHdr`(此时尚未 publish,读者不可见)。写完任一快照后 `rx_from_seq` = 该快照的 `at_seq`(见[只读视图](#只读视图seqviews实现-sdk-views))。
- **内容**:分节表与 `SnapshotHdr`/`SectionHdr` 归 [journal-format 快照记录结构](../../docs/subsystems/journal-format.md#快照记录结构);行格式由节的归属 crate 定义。本 crate 负责节 5、8–10、15、16、19、20、24、25、26、27、28、29(全部 `#[repr(C)]`、显式填充、按所列键升序;节 20 与节 25 的行格式在 journal-format;节 5 的实例状态字节由策略经 SDK 的 `SnapshotWriter` 写出)。
- **快照中的 TSC 字段**(本 crate 的已登记行;其余持久时刻一律 engine_ns):节 8 `TimerRow.deadline_tsc` 与节 27 `StrategyTimerRow.deadline_tsc`(恢复结束时按[定时器](#定时器)的公式平移);节 26 `ConnLiveRow.live_tsc`(本次运行的值,处理 `EngineClock` 时清零,不平移);节 16 `ClockCalRow.tsc`(不计入 `state_hash`,只供冷读者换算 UTC)。
- **从快照起步**(恢复、验证、镜像同一流程):各组件 `restore` 之后、处理任何 Input 之前,以节 25 的 `base_engine_ns` 恢复 `risk.set_run_start`(保证金数据的新旧运行判定沿用该快照的运行起点),`rx_from_seq` = 该快照的 `at_seq`,对每个实例按生效的 `StrategyParams` 调 `ledger.on_groups_changed`、`risk.on_groups_changed`,并重算 `model_mem`、`model_ckpt`;对每个 group 以恢复出的 LegTracker 摘要调一次 `risk.on_legs`(只重建 hft-risk 的 `on_legs` 缓存;返回的 `AutoClear` 不执行、不写记录——快照之前的 Input 已对同一状态判定过);引导块与之后读到的 `L2Snapshot{origin = Keyframe}` 只写档位、不改 `restore` 读回的簿元数据([hft-book](../hft-book/README.md));再以节 10 每 inst 的 `(mark_px, mark_ens)` 调一次 `ledger.restore_marks`、以 `interval_s ≠ 0` 的行逐 inst 调 `ledger.on_funding_interval`——这些只重建不进快照的派生索引与缓存,不产生任何记录。

#### 节行格式(草案,实现时以代码与编译期断言为准)

当前行版本:节 5 v2、节 8 v2、节 9 v1、节 10 v6、节 15 v2、节 16 v1、节 19 v2、节 24 v1、节 26 v2、节 27 v1、节 28 v1、节 29 v1(节 20、25 的行格式与版本在 journal-format)。

**节 28 `FillSeq`**(最近成交 seq):`CmdState.fill_seq` 是按 (account, inst) 的定长稠密表(启动时按 refdata 的账户数 × 品种数分配,成交路径只写不分配),`AdoptExchange` 的 `StaleReport` 守卫与 `AUTO_EXTERNAL` 前提读它。快照只写非零行,按 (account, inst) 升序;没有任何成交时整节不写,所以无成交的快照字节不变。恢复按行写回;节缺失(含早于本节的旧快照)= 空表(与旧版本恢复行为相同);行超出 refdata 维度、版本不符、`_pad` 非零、seq 为 0、inst 为 0 或乱序 → `SnapshotUndecodable{section: 28}`。本节是成交事件的派生索引,不计入 `state_hash` 与 DST `HashTrail`(与节 16 一样),但快照记录仍逐字节参与验证回放,离线快照自检(`replay_snapshot`)单独核对它的往返字节;因此从快照起步与连续回放做出相同的守卫判定。

**节 29 `ReconView`**(保留的对账视图):`CmdState.recon_views` 为 `AdoptExchange` 保留最近 `RECON_VIEW_REPORTS` = 16 个 report 的 `view_seq` 与 Position 条目差量(交易所 − 引擎),差量行合计上限 `RECON_VIEW_DIFFS` = 4096;两者启动时预分配,运行期不扩容。报告的任一条目(普通 ReconcileReport 或批次)到达即登记该 report 的视图(Position 条目设定 `view_seq`,后到者覆盖;其他条目只在缺失时以其 `view_seq` 建立),表满时按登记先后整份淘汰最旧的其他报告;单个报告的差量独占全部容量仍放不下时,该报告标记 OVERFLOW、丢弃其差量,写一条 P0 `ReconcileRejected{reason = 5, detail = 4096}`。`AdoptExchange` 引用的报告不在表中(被淘汰、OVERFLOW、或从不含本节的旧快照恢复)→ `PreconditionFailed{StaleReport}`,不再以零差量入账。快照按淘汰顺序(最旧在前)写每个报告一行 `ReconViewRow` 及其 `n_diffs` 条 `ReconDiffRow`;表为空时整节不写;缺失 = 空表;版本不符、报告数或差量数超上限、report_id 或 (account, inst) 重复、OVERFLOW 行带差量、`_pad` 非零 → `SnapshotUndecodable{section: 29}`。与节 28 相同,本节不计入 `state_hash` 与 HashTrail,快照字节仍逐字节验证,离线快照自检核对其往返。

```rust
#[repr(C)] pub struct StrategyRowHdr {                                                                                  // 节 5,24 B
    pub strategy: StrategyId, pub account: AccountId, pub kind_id: u16, pub schema_ver: u16,
    pub desc_version: u16,           // SDK StrategyDesc.version(快速部分快照布局与语义版本);restore 接受规则归 SDK,不接受 → BootError::SnapshotUndecodable
    pub params_version: u64,
    pub len: u32,                    // 其后先是 n_groups 条 GroupSeqRow,再是 len 字节 = 实例 snapshot(),补零到 8 B
    pub n_groups: u16, pub _pad: u16,
}
#[repr(C)] pub struct GroupSeqRow { pub group: GroupId, pub _pad: u32, pub spec_seq: u64 }                              // 节 5,16 B;每 group 一条,按 GroupId 升序
#[repr(C)] pub struct TimerRow  { pub deadline_tsc: u64, pub timer_id: u32, pub owner: u16, pub _pad: u16, pub group: GroupId, pub _pad2: u32 } // 节 8,24 B;deadline_tsc = 本快照所在运行的 TSC(节 25 给出该运行的基准)
#[repr(C)] pub struct StrategyTimersHdr { pub strategy: StrategyId, pub _pad: [u8; 3], pub n_timers: u32, pub next_id: u64 } // 节 27,16 B;count 为实例数,每行头后 n_timers 条登记
#[repr(C)] pub struct StrategyTimerRow { pub id: u64, pub deadline_tsc: u64, pub tag: u32, pub group: GroupId } // 节 27 登记,24 B;deadline_tsc 与节 8 同一运行、同一平移
#[repr(C)] pub struct ActiveConfigRow { pub blob: BlobRef, pub scope: CmdScope }                                       // 节 9,64 B
#[repr(C)] pub struct FillSeqRow { pub account: AccountId, pub _pad: [u8; 3], pub inst: InstId, pub seq: u64 }      // 节 28,16 B;键 (account, inst),只写 seq ≠ 0
#[repr(C)] pub struct ReconViewRow { pub report_id: u64, pub view_seq: u64, pub n_diffs: u32, pub flags: u32 /* OVERFLOW=1 */ } // 节 29,24 B;其后 n_diffs 条 ReconDiffRow
#[repr(C)] pub struct ReconDiffRow { pub account: AccountId, pub _pad: [u8; 3], pub inst: InstId, pub diff: Qty }   // 节 29,16 B;登记顺序

#[repr(C)] pub struct MarketLastRow {                                                                                    // 节 10,112 B;键 inst
    pub inst: InstId, pub interval_s: u32, pub trade_px: Px, pub trade_qty: Qty, pub trade_ts_ns: i64,
    pub funding_rate: Rate, pub next_funding_ns: i64, pub mark_px: Px, pub index_px: Px, pub mark_ts_ns: i64,
    pub settled_cut_ns: i64,
    pub bbo_member_mask: u8,         // 该 inst 的 BBO 成员连接槽位图(bit k = 槽 k);本次运行的值,处理 EngineClock 时清零
    pub bbo_flags: u8,               // RESYNC=1:该 inst 的 BBO 在补帧(逐 inst ConnState{Resync, BBO_REFRESH}),下一条被应用的 Bbo 清除,EngineClock 清零;
                                     // SW_TS=2:最近一条被应用的 Bbo 的记录头带 SW_TS(bbo_hw_rx_ns 是软件戳),随 bbo_hw_rx_ns 一起更新
    pub _rsv: [u8; 6],
    pub bbo_hw_rx_ns: i64,           // 最近一条被应用的 Bbo 的 MarketHdr.hw_rx_ns(PHC 域,口径归 event-model;0 = 无)
    pub mark_ens: i64,               // 最近一次 MarkPrice 被应用时的 engine_ns(ledger restore_marks 用)
    pub prev_interval_s: u32,        // v6:上一次不同的结算周期(SDK FundingView.prev_interval_s;未变化过 = interval_s);v5 行读出为 interval_s
    pub _pad2: u32,
}
#[repr(C)] pub struct ReconPendingHdr { pub report_id: u64, pub account: AccountId, pub venue: VenueId,
                                        pub n_stored: u16, pub count: u16, pub received: u16 }                          // 节 19,16 B;count = 组内条目总数,
                                        // received = 已到达条数(含到达即处理的 Funding 条目);其后 n_stored 条非 Funding 的 ReconcileReport 体原样
#[repr(C)] pub struct CmdDedupHdr { pub n_dedup: u32, pub n_preview: u32 }                                              // 节 15,8 B:其后 n_dedup 条 CmdDedupRow,再 n_preview 条 ViewerPreviewRow
#[repr(C)] pub struct CmdDedupRow { pub cmd_id: [u8; 16], pub cmd_seq: u64, pub status: u8 /* CmdResult 结果码,码表见 control-plane */, pub _pad: [u8; 7] } // 节 15,32 B
#[repr(C)] pub struct ViewerPreviewRow { pub cmd_hash: [u8; 32], pub issuer: [u8; 24], pub preview_engine_ns: i64 }  // 节 15,64 B;Viewer Kill 的预览表,≤ 16 条
#[repr(C)] pub struct ClockCalRow { pub tsc: Tsc, pub realtime_ns: i64, pub phc_ns: i64, pub tsc_hz_milli: u64, pub from_seq: u64 } // 节 16,40 B
#[repr(C)] pub struct VenueNoticeRow { pub venue: VenueId, pub kind: u8 /* Delisting=1 FundingInterval=2 Maintenance=3 */, pub _pad: u16,
                                     pub inst: InstId /* 0 = 整个 venue */, pub a: i64, pub b: i64, pub cmd_seq: u64 }      // 节 24 VenueNotices,32 B;键 (venue, inst, kind)
// 节 20 SeqDay:last_day_rolled 的唯一归属(SeqDayRow 行格式在 journal-format)。
// 节 25 EngineClock:当前运行的引擎时钟基准(EngineClockRow 行格式在 journal-format;见"引擎时间")。
#[repr(C)] pub struct ConnLiveRow { pub venue: VenueId, pub slot: u8, pub flags: u8 /* UP=1 RESYNC=2 */, pub _pad: [u8; 5], pub live_tsc: Tsc } // 节 26 ConnLive,16 B;键 (venue, slot);只含本次运行用过的槽
// 每个结构旁有 const _: () = assert!(size_of::<T>() == N) 与关键字段 offset_of 断言。
```

- **兼容规则**:节行只在尾部追加字段时,旧读者按前缀读取;语义不兼容的变化换新节号。读者不认识的节按 `SectionHdr.len` 跳过——这保证回滚到上一版本时最新快照通常仍可读(见[版本变化与回滚](#版本变化与回滚))。

SEQ-253 覆盖节 10 v5 的周期默认值、追加行尾、未知节和节 8 行数/长度冲突。兼容恢复会重新写出当前行格式;旧字节不能原样再生时,离线快照自检仍返回 Size/RoundTrip,不借兼容读取豁免。恢复后新写出的 Recovery 快照及其后续输入可独立验证。

### 冷启动恢复(RecoverDriver)

1. hft-journal `open` 做截断与补复制;本 crate 再截断到**最后一个带 `EVENT_END` 的记录**,之后的半个事件丢弃,其 Input 视为未发生;半个事件的 Input 哈希(`InputHash = xxh3_64(kind ‖ body)`)报告给 hft-core(毒丸判定)。
2. 按 `RecoverPlan.start` 选快照:`Latest`(默认)、`DrainTail`(版本变化的干净升级)、`AtSeq`/`NewestReadable`(`--recover-from-snapshot`)。各组件 `restore`;节 5 的 `desc_version` 交策略 `restore`(读取方式归 SDK `SnapshotReader`),不被接受 → `BootError::SnapshotUndecodable`(`NewestReadable` 时改试更早的快照);恢复后的占用超过当前容量 → `BootError::CapacityShrink`。随后按"从快照起步"重建派生缓存。
3. `replay_tail = true`:从快照起照常应用其后的 Input(各组件 `restore` 只装载、不改任何 TSC 量,尾部重放用旧运行的事件 tsc 原样比较);重新生成的 Output 与日志比较,**不一致**按下节"恢复时决策不一致"处理,不 Fatal;不出环。`replay_tail = false`:不读尾部,全部账户置 `TAIL_LOST`。
4. **恢复结束**(尾部重放之后、写 `purpose = Recovery` 快照之前;结果进入该快照,新段的验证回放从平移后的状态起步):`stop_tsc` = 旧 journal 最后一条已应用记录的记录头 tsc(`replay_tail = false` 时取所选快照所在记录的 tsc);**`now = drv.run_tsc()`——本次运行的 TscClock 读数,不是日志值**(整机重启后 TSC 从小值重新计数,用日志值会把定时器平移到极远的将来),它同时写进该快照的 `SnapshotHdr.snap_tsc`;`hz_prev` = 上次运行的 `tsc_hz_milli`(节 25),`hz_cur` = 本次运行的(新段段头,与随后的 `EngineClock` 同值)。调 `oms.recovery_end(now, stop_tsc, hz_prev, hz_cur, …)`(全部非终态槽置 `Unknown` 并登记查询;已登记 TSC 字段平移;`Open`/`Hedging` 链接置 `RESUME_HOLD`)与 `exec.recovery_end(now, stop_tsc, hz_prev, hz_cur)`,再把 `exec.recovery_timers()` 逐条登记进堆;本 crate 的堆内定时器按[定时器](#定时器)的同一公式平移(令牌桶在 hft-risk 里按 engine_ns 存,不需处理)。`stop_engine_ns` = 上次运行最后一条已应用记录的 engine_ns(`replay_tail = false` 时取所选快照的 `SnapshotHdr.engine_ns`),经 `view().stop_engine_ns()` 交实盘驱动写进新运行的 `EngineClock.base_engine_ns`。随后有"恢复时决策不一致"的 (venue, account) 调 `oms.force_unknown`;每个账户 `apply(Auto(Recovery{account}))` 与 `apply(RecoveryFlags{account, flags})`(`flags` 在 `RecoverPlan.flags` 与本次恢复置的 `RECOVER_MISMATCH` 之外,hft-risk 干净重启条件 ①–③ 对整个实例成立时全部账户另置 `CLEAN_DRAIN`;本步判定一次,条件 ③ 中"无未到达的引擎目标"读 `exec.engine_reduce_pending()` 为假,条件定义归 [hft-risk](../hft-risk/README.md#干净-drain-重启自动解除-recovery))——**不写任何 cell**,期望模式保持快照值;是否自动解除 Recovery 由 [hft-risk](../hft-risk/README.md#干净-drain-重启自动解除-recovery) 判定(含只含 Sim 账户的实例的例外)。
5. **epoch**:取值公式与持久化归 [apps/hft-core 启动顺序](../../apps/hft-core/README.md#启动顺序)步 5;`RecoverPlan.epoch` 是 hft-core `DurableMeta` 中的持久值,本 crate 按该公式与快照节 3 的值得到新值,写进 OMS 计数(节 3)并经 `view().engine_epoch()` 交 hft-core 在 `EngineClock` 之前用 [hft-journal `DurableMeta::write_sync`](../hft-journal/README.md) 持久化。新段以 `RollReason::Recovery` 打开;带 `FROM_SNAPSHOT` 时用 `RecoverFromSnapshot`,两者均写完整引导块(快照 `purpose = Recovery`,含第 4 步的结果);随后实盘驱动写本次运行的第一条 Input `EngineClock`,sequencer 处理它(设时钟基准、清空连接活性与 Viewer 预览表、`ledger.on_run_start`/`risk.set_run_start`)并在同一事件内写 `Note{EpochStart}`(其 seq = `Now.boot_seq`,见[引擎时间](#引擎时间engine_ns)),然后经 `GwCmd{SetEpoch{epoch, accept_prev: true, prev_epoch}}` 通知各 gw(`prev_epoch` = 所选快照节 3 的 epoch,不按 `epoch − 1` 推算;编码归 hft-event `GwCmd`)。
6. 对每个 (venue, account) 经 `oms.mirror_restore` 分批下发 `GwCmd{MirrorRestore}`;无法编码查询的槽保持 Unknown 并转交 reconciler。
7. **不在启动时判 `Broken`**:`RESUME_HOLD` 的链接不判 `hedge_deadline_ms`;续接单所在 venue 的对冲闸门子集满足时(`ModeOutcome.hedge_gate_open`)sequencer 调 `oms.on_hedge_gate_open(a, v, …)`,OMS 对每个这样的链接续接一次(`SecondRetry` 或 `Reverse`),之后按正常截止判断;超过 `resume_hold_max_ms` 仍未续接的处理归 [hft-oms](../hft-oms/README.md)。执行层的恢复期自动处置按多出那一腿所在 venue 开始(`venue_gate_open` → `exec.on_venue_gate_open`),启动闸门放行时(`released`)调 `exec.on_gate_open(a, now)`(见[模式与 cap 的驱动](#模式与-cap-的驱动) ⑤)。

`FROM_SNAPSHOT` 必须配合 `replay_tail = false`,矛盾配置返回 `BootError::Config`。hft-core 恢复入口经 `boot(Journal(plan))` 在引导块 ClockStatus 后写一条 `Note{Boot, RecoverFromSnapshot}`;`v[0]` 是所选快照的首片 seq,`v[1..3]` 是快照末片之后至最后完整提交事件末片的物理闭区间,空区间写 0/0,未提交的半事件不计入。`v[3]` 使用 TailSkipped 的 Operator 原因码 3,`v[4]` 是启动入口 `cli:systemd` 的前 8 字节。它属于 Meta,不计入输出哈希;临时范围不进快照。文件读取保留末片位置,不能用逻辑记录首 seq 代替末片 seq。

验证回放(`VerifyDriver`,逐字节、不一致即失败)只是离线夜跑与 CI 工具([apps/replayer](../../apps/replayer/README.md)),不在启动路径上。

### 恢复时决策不一致

尾部重放产生的 Output 与日志不一致,说明重算状态与真实发出的订单可能不同(真实订单的回报会落成 StaleGen 或 ForeignOrder)。处理:

1. 确定受影响范围:不一致的记录带 (venue, account)(`OrderDecision`、`ExecDecision`、`FundingSettle`)→ 该 (venue, account);不带账户的记录(`CmdResult`、无账户的 `Note`、快照哈希)→ 全部 (venue, account)。
2. 恢复结束时对每个受影响的 (venue, account):`oms.force_unknown(venue, account)` 把**全部**非终态槽(含 Live;范围归 [hft-oms](../hft-oms/README.md))置 `Unknown` 并查询;该 (venue, account) 的上一 epoch 回报一律按成交证据应用(迟到成交按墓碑入账并报 P0)。
3. 受影响账户置 `RECOVER_MISMATCH`:禁用自动解除 Recovery;`RECONCILED` 需要该账户每个 venue 一组 `EpochStart` 之后的报告满足严格条件(未知挂单由对账以孤儿单报出,须先撤单或处置)。
4. 每个受影响的 (venue, account) 恰好写一条 `Note{RecoverMismatch, seq, venue, account}`(`seq` = 使它受影响的第一条不一致记录的 seq),级别 **P0**;这些 Note 在 Recovery 引导块之后、`EngineClock` 之前写成一个事件,追加成功后按顺序计入 `out_hash` 的下一区间(引导块的快照已含恢复结束的结果,新段的验证回放从该快照起步)。

### 毒丸输入隔离

确定性的坏输入可能让进程在同一处反复崩溃。两层分工,计数都在 apps/hft-core 的 `DurableMeta`(键与流程归 [apps/hft-core](../../apps/hft-core/README.md),持久化归 [hft-journal](../hft-journal/README.md)),阈值都是 `startup.poison_crash_limit`(默认 2):

| 层 | 管什么 | 判据与计数 | 达到阈值后 |
|---|---|---|---|
| **恢复期**:hft-core `boot_guard` | 恢复重放尾部时崩溃 | `boot_guard = {snapshot_seq, tail_end_seq, attempts}`:同一重放区间再次开始重放 → `attempts += 1`,成功结束清零 | `RecoverPlan{start: Latest, replay_tail: false, flags: POISON \| TAIL_LOST}`:从最近快照起步、不重放尾部、严格闸门、无自动解除;`Note{TailSkipped{Poison}}` P0 |
| **运行期**:本 crate `InputHash` | 实盘处理某条 Input 时崩溃——它属于被截掉的半个事件,不会被重放,但同样内容可能再次到达(重新生成的对账报告、重发的 Advice、同一行情帧) | 每次启动本 crate 报告被截掉的半个事件的 `InputHash = xxh3_64(kind ‖ body)`;hft-core 记 `DurableMeta.poison_input = {hash, count}`:与上次相同 → `count += 1`,否则置 1 | `RecoverPlan.quarantine = [hash]`,`flags` 加 `POISON`(尾部照常重放,致崩输入不在其中);`LiveDriver` 取到哈希命中隔离集合的输入时不交 `process`、不写成 Input,只写一条 Meta `Note{InputQuarantined}`(字段以 hft-event 为准)并 P0;该输入从未发生,回放与验证一致;隔离集合在本进程生命周期内有效 |

两层都置 `POISON`,因而禁用自动解除 Recovery、`RECONCILED` 按严格条件。运维也可显式要求 `--recover-from-snapshot [seq]`(hft-core 参数):`RecoverPlan{start: AtSeq | Latest, replay_tail: false, flags: FROM_SNAPSHOT | TAIL_LOST}`,随后严格对账。

### 版本变化与回滚

| 情形 | 启动方式 |
|---|---|
| 二进制与 schema 版本与 journal 相同 | 正常恢复(重放尾部) |
| 版本不同,journal 末尾是 `Drain` 写出的快照(其后无 Input) | `start = DrainTail`,不重放(干净升级);策略状态按节 5 的 `desc_version` 由策略 `restore`(接受当前与上一版本,规则归 SDK) |
| 版本不同,末尾不是 Drain 快照(典型:新版本崩溃后回滚旧版本) | 默认 `BootError::VersionNeedsDrain`(hft-core 退出 4 并提示);带 `--recover-from-snapshot` 时 `start = NewestReadable`(本二进制能解析的最新快照,含策略 `desc_version` 可接受)、`replay_tail = false`、`FROM_SNAPSHOT | TAIL_LOST`,随后严格对账,交易所真值经 `AdoptExchange` 收敛 |

回滚因此总有路径:停新版本 → 以 `--recover-from-snapshot` 启动旧版本 → 闸门严格放行 → 人工 `Resume{Recovery}`。步骤写在 [runbooks](../../docs/runbooks/README.md)。

### 启动闸门

闸门状态与规则归 [hft-risk `mode`](../hft-risk/README.md#启动闸门);sequencer 只把 `ConnState`(含 gw 主下单连接、用户流与热备的汇总)、簿 Live、Unknown 计数、对账组与处置、时钟判定、`AccountConfigReport`、`Timer{KillSelfTestOk}` 作为 `Gate(…)` 输入交给 `apply`;闸门位变化由 mode 在 `ModeOutcome.gate_changes` 报告,sequencer 写 `Note{StartupGate}`;对冲闸门子集与 `hedge_gate_open` 的含义同样归 hft-risk。

### 存储降级

`Timer{PersistLag{level}}` / `Timer{DiskLow{level}}` / `Timer{StorageOk}`(阈值 `storage.*` 归 apps/hft-core)→ `risk.mode.apply(Storage{kind, level})`:`level ≥ 1` → `Storage` cap(全局禁开),`StorageOk` → 删除;`level = 2` → `ModeOutcome.drain = Some(StorageCritical)`,sequencer 以内部命令走与 `Drain{cancel_resting: true}` 相同的流程,结束时 `StepOutcome::Done{restart: false, cause: StorageCritical}`(hft-core 的退出码与是否重启归 [apps/hft-core](../../apps/hft-core/README.md))。存在未到达的 `reason ∈ {MarginStress, Hard}` 引擎目标时(与 `exec.drain_ready()` 同口径;不按清仓锁是否仍在判定)hft-risk 推迟产出 `drain`:sequencer 的 `ModeView` 适配器以 `exec.engine_reduce_pending()` 作为 `engine_reduce_pending()` 的读数;`risk.mode.drain_deferred()` 为真期间,每条 Input 在执行层推进(`exec.advance`)之后调一次 `apply(StorageRecheck)` 并执行其 `ModeOutcome`(推迟条件与产出点归 [hft-risk 存储降级](../hft-risk/README.md#存储降级storage));推迟期间的硬阈值是 journal 本身,写满仍按 [journal 满](#journal-满) abort。

### 命令路径

语义、类别、前置条件与结果码在 [control-plane](../../docs/subsystems/control-plane.md);gateway 只做传输、鉴权、角色上限与参数解码,**不复制 sequencer 状态**。sequencer 侧:过期检查 → `cmd_id` 去重(最近 `recent_cmd_ids` 个,进快照节 15)→ `CmdHdr.role` 复核(进程主体 `role = 0`,按 issuer 只接受其允许的命令,允许表归 [hft-ctl-proto](../hft-ctl-proto/README.md))→ 账户语义复核 → **确认与版本复核** → 应用 → `CmdResult`。

- **PREVIEW**:带 PREVIEW 标志的命令不改任何模式、风险或业务状态,回 `CmdResult{preview}`(`status` = 此刻经确认执行会得到的码:前置条件不满足时为相应拒绝码,否则 `Applied`,不出现 `NeedsConfirm`/`ConfirmInvalid`,[control-plane 版本与确认](../../docs/subsystems/control-plane.md#版本与确认));唯一的写入是 Viewer `Kill` 的预览在预览表登记一条(见下)。
- **确认**:按当前状态判定是否为升级(`mode.is_upgrade`、`risk.loosens`);`SetParams`:**pipeline 主体发出的默认中性、不需确认,Live 账户与 Sim 账户相同**——包括新增 group 与把 `open_allowed` 由 0 变 1(作用于 Live 实例的人工参数编辑在 ops_bot 侧确认后由 pipeline 下发,覆盖项按实例分开,规则归 control-plane 与 pipeline);人工主体(`cli:<login>` 等)发出且某个 group 的 `open_allowed` 由 0 变 1(或新增 `open_allowed = 1` 的 group)时为升级、需确认;**作用于 Live 实例(定义的唯一归属是 [control-plane 类别](../../docs/subsystems/control-plane.md#类别))且 `structural_changed` 为真时,不论主体一律升级、需确认**(预览内容归 control-plane;pipeline 主体不发确认,因此这类改动只能由人工发出);其余参数变化是中性——策略参数的上界由 `RiskParams` 约束(主体类型的判定以 [control-plane](../../docs/subsystems/control-plane.md) 为准);需要确认而 `confirmed = 0` → `NeedsConfirm`;一般命令 `expect_state_version ≠ ctl_state_version` → `ConfirmInvalid{StateChanged}`(Viewer 紧急停止的绑定方式不同,见下);`confirmed = 1` 而 `expect_state_version = 0`(绕过 gateway 直接写环的确认)对需要确认的命令同样 → `ConfirmInvalid{StateChanged}`,不按"不检查"处理,与 gateway 的 `check_confirm_fields` 一致;`Kill` 例外(Viewer 的 `Kill` 按预览绑定,其余主体的 `Kill` 不比对版本)。
- 每个命令先复核账户语义:`CmdHdr.account == CmdScope.account`(不带 scope 的命令比对载荷中的 `account`);`Draining` cap 存在时升级命令 → `PreconditionFailed{Draining}`。**例外**:`Kill{Account, flatten}` 在 `Draining` 与该账户启动闸门未放行期间同样接受(仍需确认;control-plane 归为升级类的例外),它只减仓,恢复期与停机前正需要它。
- `SetParams`(blob = `StrategyParamsHdr` + `[GroupSpec; n]` + body,容器布局归 hft-event,body 编码归 SDK):gateway 通用解析头与 `GroupSpec`,用装配处的 `S::decode_params` + `S::validate_params` 解码并校验 body、算出 `model_mem`、`model_ckpt` 与 `structural_changed`,解析结果经 Mailbox 交来。sequencer 依次复核:
  1. refdata 的 `check_binding`(`AccountMeta(account).bound_instance == strategy`,否则 `InvalidArgs{AccountBound}`;该实例在 refdata `InstanceMeta` 中的 `kind_id` 等于 `hdr.kind_id`,否则 `KindMismatch`)、该种类的 `StrategyDesc.non_prod_only` 为真而账户 `is_prod` → `InvalidArgs{ParamInvalid}`(kind `NonProdOnly`),与 `check_group`(各腿 inst、venue 在账户内、同一基础资产与报价资产;**同一 GroupSpec 的任意两条腿不得在同一交易所**(`VenueMeta.exchange` 相同)→ `InvalidArgs{SameExchangeLegs}`,gateway 先查、这里复核;错误名以 [hft-refdata](../hft-refdata/README.md) 为准);
  2. `version` 递增(`StaleVersion`)、`base_version`;
  3. `GroupId` 不是 `GROUP_NONE`/`GROUP_ALL`、高 8 位等于该实例的 `StrategyId`(构造规则 `(StrategyId << 24) | 实例内计数器` 归 [hft-types](../hft-types/README.md);否则 `BadGroup`),且全局唯一(`GroupIdDup`,防御:构造已保证);
  4. 同一实例内任一 (account, inst) 至多被一个非 Shadow cell 引用(`InstOwned`);
  5. 被移除的 group 与指纹(`GroupSpec::fingerprint()`)变化的 group(视同"移除 + 新增")无持仓、无库存、无在途、无活动目标(`PreconditionFailed{GroupHasPosition}`);
  6. 容量:应用后全进程 cell 数 ≤ `max_cells_total`、每 inst 引用数 ≤ `max_subs_per_inst`、订阅 inst 数 ≤ `max_insts`(`CapacityExceeded`)。这是引擎容量层;单个实例的 group 数上限 `max_groups_per_instance` 由 SDK 在 gateway 解码时检查(`TooManyGroups` → `InvalidArgs{ParamInvalid}`,kind `Strategy`),不在这一层;
  7. **两腿 lot 对齐**(`InvalidArgs{LotMismatch}`):**只检查新增的 group 与指纹变化的 group**(存量 group 不因价格变化把整条命令拒掉):对每个这样含 ≥ 2 条交易所腿的 `GroupSpec`,任意两条交易所腿按对冲比 1/1 取 `L = pair_lot(lot_a, lot_b)`(hft-types 纯函数);不能整除且 `L × 参考价 >` 该账户 `ExecConfig.force_slice.max_chunk_notional` → 整条命令拒绝。阈值与取整规则的唯一归属是 [hft-exec 两腿 lot 对齐](../hft-exec/README.md#两腿-lot-对齐)(GroupSpec 不带切片参数,所以 `SetParams` 用引擎切片的阈值;减仓与风险目标在执行层按较粗 lot 取整而不拒)。参考价 = 两腿 inst 的 `MarketLast.mark_px`(先有者),都没有则簿 mid;仍没有 → 同样拒绝(只发生在不能整除的 lot 组合上,运维可在行情可用后重发)。**尘埃线与最小名义**(`InvalidArgs{DustBelowMinNotional}`):同一范围的每个 group,其交易腿 inst 的 `InstMeta.min_notional` 最大值 > 该账户 `ExecConfig.dust_notional` → 整条命令拒绝(与 hft-core 装配期的 `DustBelowMinNotional` 同一判定函数,归 [apps/hft-core](../../apps/hft-core/README.md);运行中 refdata 不变,只有新增或改腿的 group 会引入新的交易腿);
  8. **Model 内存与检查点**:把本实例的 `model_mem`、`model_ckpt` 换成新值后,本 hft-core 实例全部策略实例的 Σ `model_mem` ≤ `advisor_mem_budget_mib` 且 2 × Σ `model_ckpt` ≤ `advisor_ckpt_max_mib`,否则 `InvalidArgs{ParamInvalid}`(kind `ModelMemBudget`),旧参数继续生效(所以运行中通过的参数在下次启动时不会被 hft-core 的同一预算拒绝)。

  应用:新实例 `S::create`(Err → `InvalidArgs{ParamInvalid}`,kind `Strategy`,不建实例、参数不生效),已有实例 `on_params`(Err → `InvalidArgs{ParamInvalid}`,kind `Strategy`,旧参数继续生效);重建订阅;同步 OMS/ledger/执行层的 group 表;调 `risk.on_groups_changed`(**先于** `apply(GroupsChanged)`:杠杆超限的禁开由 `ModeView::lev_bad` 读它的结果);`apply(GroupsChanged)`;按 `GroupSpec::fingerprint()` 更新各 group 的 `spec_seq`(见 [Advice 路由](#advice-路由));撤销被移除 group 的定时器与目标(写 Note);新 group 取 `default_mode`。
- `SetRisk`:按账户作用域替换(`risk.set_params(p, scope)`);`RiskParamError::Invalid` → `InvalidArgs{ParamInvalid}`;`ExecRiskMismatch`(执行配置的 `force_slice` 越出新风控上限,规则归 [hft-risk](../hft-risk/README.md))→ `InvalidArgs{ExecRiskMismatch}`;行数超出预分配 → `CapacityExceeded`;两者旧参数都继续生效。
- `Kill` / `Resume` / `SetMode`:翻成 `ModeInput` 调 `risk.mode.apply`(`hard` 允许与 Global 或 Account 组合;`hard` 与 `flatten` 不能同时置位,HARD 拦全部引擎单,清仓无法执行);`ModeErr` 的映射见下表;目标 Shadow 且 cell 非 flat → `Applied{detail = ShadowDeferred}`;`Kill{Account, flatten}` 另调 `exec.engine_flatten`(含 `Broken` 的 group)。模拟账户的 `Kill` 走同一路径。

  | `ModeErr` | `CmdResult`(detail 码归 control-plane) |
  |---|---|
  | `GateNotReleased`、`Draining`、`NakedNotCleared`、`ReconNotClean`、`ResumeLimit`、`NoMatchingCap` | `PreconditionFailed{同名 detail}` |
  | `BrokenRemaining`(`Resume{HedgeFailure}` 时该账户仍有 `Broken`) | `Rejected{BrokenRemaining}` |
  | `NotResumable` | `InvalidArgs{NotResumable}` |
  | `UnknownPending`(`Resume{UnknownTimeout}` 时该 (account, venue) 仍有 Unknown 槽) | `PreconditionFailed{UnknownPresent}` |
  | `InvalidScope`(`hard` 与 Venue、`flatten` 与 Global、`flatten` 与 `hard` 等) | `InvalidArgs{BadCombination}` |
- **停机标记产生的 `Kill`**:hft-core 启动时读到 kill-switch 写下的停机标记,由实盘驱动在 `EngineClock` 之后、任何其他 Input 之前写一条主体为 `cli:systemd`(进程主体)、非 HARD、不带 `flatten` 的 `ControlCmd{Kill{scope}}`(流程、标记文件与改名归 [apps/hft-core](../../apps/hft-core/README.md));它是 journal 的 Input,回放照读。sequencer 按普通 `Kill` 处理(降级,不需确认,闸门未放行时同样生效);cap 与 Trader 发出的 Kill 相同,只由 `Resume{Kill}` 解除——干净 Drain 重启自动解除的只是 `Recovery`,不碰它,所以停机之后的重启不会自动恢复开仓。
- `AdoptExchange{account, venue, inst, report_id, item: Position | Balance, asset, group, entry_px}`:取该 `report_id` 保留的对账视图(节 29;未保留 → `PreconditionFailed{StaleReport}`);该 inst/资产自 `view_seq` 以来有成交 → `PreconditionFailed{StaleReport}`;否则构造 `AdjustDelta`(仓位或余额)交 `ledger.apply_adjust`,目标 group ≠ `GROUP_NONE` 时以同量调 `oms.adopt_adjust` 调整 LegTracker,再以 `Gate(Disposed{…, Adopted})` 报告处置;不建 OMS 槽。
- `SetBalanceBaseline{account, venue}`:`ledger.set_balance_baseline` → 对返回的资产逐个 `Gate(Disposed{…, Baselined})`。
- `VenueNotice{venue, inst | *, kind}`:记入生效公告表(节 24;下架时刻、结算周期覆盖、维护窗口;inst 退出 refdata 或维护窗口结束后在下一次快照时清除)→ `apply(Notice)`(Delisting → `Delisting` cap)→ 向订阅该 inst 的实例投递 `EventRef::Notice`。
- `SimReset{account}` / `SimTransfer{…}`:只对 `AccountMeta.kind = Sim`(否则 `InvalidArgs{NotSim}`)。`SimReset` 的前置条件(含无非终态 IOC 槽)以 [control-plane](../../docs/subsystems/control-plane.md) 为准,不满足 → `PreconditionFailed{SimNotIdle}`;随后经 `out-sim` 交 sim-gw(它撤销该账户挂单并回报 `Cancelled`、恢复初始余额并发真值报告,见 [hft-sim](../hft-sim/README.md)),同一事件内 `ledger.sim_reset`、`oms.sim_reset(account)` 清零该账户 LegTracker、`exec.clear_account(account)` 清除该账户目标,再对该账户每个 group 调 `risk.on_legs`(裸量为零:`Broken` 随之解除;`HedgeFailure` 仍需 `Resume{HedgeFailure}`)。`SimTransfer`:按 ledger 可用余额校验(不足 → `PreconditionFailed{InsufficientBalance}`)→ `ledger.apply_transfer(kind = Sim)` → 交 sim-gw。
- `SimFault{account, venue, fault, delay_ms}`(Admin;参数与校验归 control-plane,sim-gw 侧语义与状态归 [hft-sim](../hft-sim/README.md)):只对 `AccountMeta.kind = Sim`(否则 `InvalidArgs{NotSim}`);本 hft-core 实例的角色为 `prod`(实例角色归 [apps/hft-core](../../apps/hft-core/README.md),装配时交给本 crate)时 → `Rejected{Forbidden}`(prod 实例不接受故障注入);通过后同一 Input 内经 `out-sim` 交 sim-gw 一条 `GwCmd::sim_fault(account, venue, fault, delay_ms)`(编码归 hft-event),`CmdResult = Applied`(`Off` 而当前无故障时同样 `Applied`,sim-gw 无动作)。
- `RecordTransfer`:`ledger.apply_transfer(kind = Record)`。
- `CancelAll{account | *, venue | *, inst?}`:按参数构造 hft-oms 的 `CancelScope`,调 `oms.cancel_all(scope, open_only = false)`(可撤状态与批量撤单规则归 [hft-oms](../hft-oms/README.md));不改模式与 cap。
- `Annotate`:不改任何状态,回 `Applied`(记录本身就是审计)。
- `HedgeFix{scope}`:`exec.hedge_fix_cmd(account, group)`,按 LegTracker 把多出的一腿平掉(`Broken`/`Reorg` 的人工处理;`Broken` 另有执行层的自动有界平仓;`hedge_cap_qty` 由执行层给出、风控 #6 检查),订单经意图管线;已成交裸量降到尘埃线以下且 group 静止后由 hft-risk 自动解除 `Broken`。`Err(ExecCmdErr)` → `CmdResult`:`UnknownGroup` → `InvalidArgs{UnknownGroup}`,`Hard`、`NotQuiescent`、`NothingToFix`、`QuoteUnavailable`、`ChainLeg` → `PreconditionFailed{同名 detail}`(detail 码登记归 control-plane);出错时执行层不改状态、不出单。
- `WriteOffResidual{account, group}`(Admin,升级类,需确认;语义、前置条件与结果码归 control-plane):sequencer 复核该 group 属于该账户绑定实例的当前参数(否则 `InvalidArgs{UnknownGroup}`);随后调 `oms.write_off_residual(a, g)`(适用于**静止的** group,`Broken` 或非 `Broken` 均可,静止的定义与判定归 OMS,不满足 → `PreconditionFailed{NotQuiescent}`;把多出那一腿的已成交裸量移出该 group,并清零该 group 全部腿的 `residual`;入口与规则归 [hft-oms](../hft-oms/README.md#残余核销)),把返回的单条 `WriteOff{venue, inst, leg_ix, qty}`(无裸量时 `qty = 0`)构造成 `AdjustDelta{kind: WriteOff}` 交 `ledger.apply_adjust`(该量从该 group 移到 unassigned;`qty = 0` 时账不变、只写注记;核销的记账归 [hft-ledger](../hft-ledger/README.md#人工干预的采纳adoptexchange)),再调 `exec.on_leg_adjust(a, g)` 与 `risk.on_legs(a, g, …)`——处于 `Broken` 的 group 裸量降到尘埃线以下即 `AutoClear{Broken}`;`HedgeFailure` 仍需人工 `Resume{HedgeFailure}`。不产生任何订单。
- `SmokeOrder{account, venue, inst, side, ttl_ms}`(Admin,升级类,需确认;语义、`ttl_ms` 范围与结果码归 control-plane):只对 Live 账户(否则 `InvalidArgs{NotLive}`);`CmdHdr.role` 不是 Admin → `RoleInsufficient`。sequencer 调 `oms.smoke_intent(…)`(按该 inst 的按值 `BboView` 与 `conn_liveness_ms` 生成 PostOnly 意图:离对手价 ≥ `smoke_far_bp`、最小名义 × 1.1 向上取整到 lot;规则归 [hft-oms](../hft-oms/README.md)),该 (account, venue) 已有一张 → `PreconditionFailed{SmokeActive}`,取不到合格价格 → `PreconditionFailed{SmokeNoPrice}`(`SmokeErr` 其余变体如 `BadInst` 的结果码归 control-plane);否则以 `Origin::Command` 与 `OrderFlags::SMOKE` 进意图管线:**不经模式门**(Shadow/CloseOnly 下照发),走完整检查链(冒烟单在各检查上的口径归 hft-risk,HARD 下被 #1 拒绝)。放行 → `CmdResult{Applied}`(同一事件内有该单的 `OrderDecision`,`trigger_seq` 指向本命令),`ttl_ms` 到期由 OMS 自动撤单;被拒 → `CmdResult{Rejected, detail = SmokeRiskReject}`。冒烟单的 `strategy` 字段是该账户绑定的实例(只为检查链 #0 的绑定检查,hft-oms),它不计入任何策略、不属于任何 group,不改任何目标;其回报不投递给任何策略(见第 3 步),万一成交照常入账(`GROUP_NONE`)。
- `StrategyCmd{instance, group, payload ≤ 256 B}`:sequencer 复核实例存在、账户绑定(`AccountBound`)、`group` 为 `GROUP_NONE` 或属于该 (实例, 账户) 的当前参数(`UnknownGroup`),然后在同一 Input 内向该实例的快速部分投递 `EventRef::Manual`(载荷不透明,字段以 [hft-strategy-sdk](../hft-strategy-sdk/README.md) 为准);策略据此给出的意图照常经模式门与检查链。类别按 [control-plane](../../docs/subsystems/control-plane.md):默认升级(需确认、闸门未放行或 Draining 时拒绝);**带 `reduce_only` 标志时为中性**(Draining、CloseOnly、闸门未放行时也可用),此时该次投递的 `Ctx` 标为只减仓:其中经 classify 为 Open 的订单意图一律拒绝(`CLOSE_ONLY`,不出环),目标意图以 `ro = true` 交执行层(会增加任一腿 |仓位| 的整条拒绝,`TargetRejected(ReduceOnlyCmd)`,判定归 [hft-exec](../hft-exec/README.md#目标的接受与替换)),这些意图的 `OrderDecision`/`ExecDecision` 以 `trigger_seq` 指向该命令以便审计。引擎不解释载荷。
- `InjectWarmup{instance, local_kind: u16, chunk_no: u32, last: bool, payload ≤ 4096 B}`(主体 Trader 或 pipeline;实例级命令,`account` 必须为 `*`;中性,闸门未放行与 Draining 期间同样可用):sequencer 复核实例存在与长度后回 `Applied`;该命令记录本身就是 journal 的 Input。策略声明接收(`Part::Both`)时同一 Input 内向快速部分投递 `EventRef::Warmup{local_kind, chunk_no, last, payload}`;Model 侧的投递由 advisor 镜像在看到同一事件的 `CmdResult{Applied}` 后完成(见 [advisor 与 Model](#advisor-与-model))。载荷由策略定义,生产者是 pipeline 的策略插件;回放确定。每轮 ctl 载荷字节受 `ctl_bytes_per_round` 限制,大量预热块不会在一轮内连续占用 sequencer。
- **`CmdHdr.role = Viewer`**(紧急停止 `/stop`):sequencer 复核只接受非 HARD、不带 `flatten`、`cancel_resting = true` 的 `Kill{Global}` 或 `Kill{Account(a)}`(其他命令或参数组合 → `RoleInsufficient`);`Resume` 一律 `RoleInsufficient`。确认**绑定预览的 `cmd_hash`,不比对 `ctl_state_version`**(协议归 [control-plane 版本与确认](../../docs/subsystems/control-plane.md#版本与确认)):① 预览照常不改模式与 cap,只在预览表(≤ 16 条,满时淘汰最旧;快照节 15;处理 `EngineClock` 时清空)登记 `{cmd_hash, issuer, preview_engine_ns = now.engine_ns}`;② 不带 `confirmed` → `NeedsConfirm`;③ 带 `confirmed = 1`(`expect_state_version` 忽略)时,表中须有 `cmd_hash` 与 `issuer` 都相同且 `now.engine_ns − preview_engine_ns ≤ VIEWER_CONFIRM_TTL_MS`(60 000)的记录 → 应用并删除该条;否则 `ConfirmInvalid{NoValidPreview}`。`cmd_hash` 覆盖账户与作用域,事故中 cap 频繁变化不会使紧急停止失效;Trader/Admin 的命令(含它们的 Kill)仍按 `expect_state_version` 比对。
- `ReplaceRefdata`:ctl-gateway 先做装配校验,不通过直接回 `Rejected{RefdataAssembly}`,命令不到达本 crate(规则归 [hft-ctl](../hft-ctl/README.md));sequencer 先查 `version`(≤ 当前 → `StaleVersion`,先于其余复核);通过后用 `RefData::load(新 blob, 当前)` 解析并做跨版本兼容校验(冷路径;回放驱动对日志里的 `ConfigBlob` 重做同一纯函数),结果随 `ParsedConfig::Refdata` 交来。sequencer 复核:载入失败,或 `bound_instance` 变化的账户仍有仓位、挂单或 Unknown(`BindingChangedWithPosition`,规则归 [hft-refdata](../hft-refdata/README.md)),或新 refdata 的订阅需求超出当前容量 → `CmdResult{Rejected, detail = RefdataRecheck}`(arg 的含义归 control-plane;本 crate 填:载入失败为 `LoadErrCode` 的数值码,绑定变更为 `BindingChangedWithPosition` 的数值码,订阅容量不足为 0;多项同时不满足时按这三项的列出顺序取第一项),blob 不生效、不 Drain、旧 refdata 继续生效、进程不退出。通过 → 把该 `ConfigBlob` 记为待生效引用(本次运行仍用旧 refdata;Drain 快照节 9 指向它,hft-core 重启时加载)、回 `Applied`,随即进入 Drain(`cancel_resting = false`);Drain 末的二次复核与撤销见 [Drain](#drain) 第 4 步,通过时 `StepOutcome::Done{restart: true, cause: ReplaceRefdata}`。
- `GetSchema` 是只读查询,由 ctl-gateway 从装配处的 `StrategySet` 直接回答,不经 sequencer。
- 每个 `CmdResult` 带当前 `ctl_state_version`。

### Drain

`Drain{cancel_resting}`(命令、SIGTERM、`ReplaceRefdata`、存储危急、身份不符五处同一语义;后两者是内部 Drain,`cancel_resting = true`,结束于 `Done{cause: StorageCritical | IdentityMismatch, restart: false}`):

1. 设不可清除的 `Draining` cap(禁开、升级命令 `PreconditionFailed{Draining}`,`Kill{Account, flatten}` 例外);执行层停止**增仓**切片(减仓、进行中的配对步骤、自动有界平仓与引擎减仓照常,规则归 hft-exec)。
2. 撤单:`cancel_resting = true` → venue 级 `CancelAll`;为 false 时保留普通挂单,但**带 LegLink 的挂单第一腿一律撤**。
3. 等待,直到每个账户同时满足:`oms.quiescent(a)` 不含 `LINKS_ACTIVE`(全部 LegLink 处于 `Done` 或 `Broken`)、`IOC_NONTERMINAL`(全部 IOC/FOK 槽终态)、`HEDGE_RETRY_PENDING`(无待触发的 `HedgeRetry`),且无 Pending* 槽;执行层 `exec.drain_ready()` 为真(每个账户 `drain_ready_acct(a)`:无在途切片与待发的补腿、默认处理与自动平仓,且没有未到达的 `reason ∈ {MarginStress, Hard}` 引擎目标——Drain 中的 L3 与 `Kill{Account, flatten}` 清仓做完才结束;定义归 hft-exec);本 crate 的通知积压与 `naked_groups` 游标为空。Unknown 槽不等(写进快照)。以上都受第 6 步的 `drain_timeout_ms` 约束。
4. **`ReplaceRefdata` 的 Drain 末二次复核**(只在本次 Drain 由 `ReplaceRefdata` 发起时;复核的执行与退出码归 [apps/hft-core](../../apps/hft-core/README.md#refdata-变更drain--重启)):第 3 步满足(或第 6 步超时)后不写快照,`view().refdata_recheck_due()` 置真;实盘驱动写一条 `Timer{Sys, RefdataRecheck}`(装配校验结果,见[环境决定写成 Input](#环境决定写成-input))。sequencer 处理它时再跑一遍命令路径对 `ReplaceRefdata` 的复核(`RefData::load(待生效, 当前)`、`BindingChangedWithPosition`、订阅容量;纯函数,回放确定):
   - 两者都通过 → 第 5 步,`Done{restart: true, cause: ReplaceRefdata}`;
   - 任一不通过 → **撤销待生效引用**:清除命令 `Applied` 时记下的待生效 refdata 引用,快照节 9 仍指当前生效的旧 refdata(新 `ConfigBlob` 留在 journal 里、保持惰性,重启不加载);写 `Note{Sequencer, RefdataRecheckFailed}`(P1;`v = [stage, arg]`:stage 1 = 装配校验(arg = `RefdataAssembly` 失败项)、2 = `RefData::load`(arg = `LoadErrCode` 的数值码,`#[repr(u16)]` 码表归 [hft-refdata](../hft-refdata/README.md))、3 = 绑定变更(arg = 账户)、4 = 订阅容量(arg = 0));再第 5 步,`Done{restart: false, cause: Drain}`(退出 0、不重启)。
   恢复与验证驱动照读日志里的该 `Timer`,不重跑装配校验。
5. 快照(`purpose = Drain`,一次写完)→ `StepOutcome::Done`。
6. hft-core 的 `shutdown.drain_timeout_ms` 先到 → sequencer 写 `Note{DrainTimeout}`(有未结束的 `Open`/`Hedging` 链接或未到达的引擎减仓目标 → P0,否则 P1;`v[0..4]` = 未满足第 3 步的账户位图(bit = `AccountId`)、`v[4]` = 未结束的链接数、`v[5]` = 未对冲名义、`v[6]` = 未到达的引擎减仓目标数)并照常写快照(`ReplaceRefdata` 发起的先做第 4 步),`Done.cause` 保持发起 Drain 的原因(第 4 步复核不通过时为 `Drain`);未结束的链接留在快照里,下次启动不满足干净重启条件。

### 确定性规则

CI 用 `clippy::disallowed_methods`/`disallowed_types` 强制([08 §5](../../design/08-数据面与回放.md)):

1. 时间只来自记录头 `tsc` 与事件体字段;TSC 只在 `Driver::take`(实盘的记录头与定时器中途检查)、`run_tsc`、`measure` 与实盘驱动的环境决定器里读。
2. 禁止 `HashMap`/`HashSet` 迭代;遍历顺序由 id 决定(实例按 `StrategyId`、目标按 (account, group))。
3. 随机数只能来自日志中的种子;禁止全局可变状态、`thread_local`。
4. 决策路径无 f64(Model 在 advisor 线程可用 f64,其输出以 Advice 载荷进入 journal)。
5. 输出结构体填充字节为零;含枚举字段的体用 `TryFromBytes` 读。
6. 容量与每轮上限是配置;环境条件(环满、kill 拦截、时钟、空闲、积压、存储、md 时间片、让位、定时器中途到期、关键帧时机)只能以 Input 记录或取出顺序影响状态,取出顺序记进 journal。
7. 同一 Input 的处理不依赖处理耗时。
8. Meta 记录不得影响状态。

### 状态哈希

- `out_hash`:按 journal 顺序对前一 `StateSnapshot` 与本快照之间的 Output 做 xxh3-128 流式累积,不含两端快照;字节流为 `kind:u16 LE ‖ flags:u16 LE ‖ logical_body`,flags 屏蔽 `EVENT_END/CRC/PART`,体按测量字段偏移置零。实际追加快照时填入 `SnapshotHdr.out_hash` 并清空累计器,同一事件内快照前的 Output 属于当前区间、快照后的属于下一区间。段首 `ConfigBlob` 计入前面的区间,快照后的 L2 关键帧计入后面的区间;从快照起播时,先把该快照之后、下一个 Input 之前的 Output 加入空累计器。
- `state_hash`:各节内容按写出顺序(见[快照](#快照))的 xxh3-128,节 16 不计入;`purpose = Keyframe` 的快照头写 0(实盘不在关键帧停顿内计算,四种驱动写同一值),replayer 与测试需要时从该快照的节内容按同一规则计算;快照头的主机字段(`snap_tsc`、`boot_id`、`snap_utc_ns`)在 `MEASURE_MASK` 中屏蔽,换机验证不因它们不一致。
- 验证回放逐条比较 Output(快照记录本身也逐字节比较),并在每个快照处比较 `out_hash` 与非关键帧的 `state_hash`;恢复驱动比较后按"恢复时决策不一致"处理;镜像驱动不比较。

### 规划源文件

| 文件 | 职责 |
|---|---|
| `src/lib.rs` | `Sequencer<D, S>`、`step`、`process` |
| `src/driver/{mod,live,recover,verify,mirror}.rs` | 四种驱动;`live/sched.rs` 取环顺序、让位、定时器中途检查与每轮上限,`live/env.rs` 环境决定器(含关键帧的时间与事件数触发、最大推迟),`live/feedctl.rs` `FeedCmd` 环满时的暂存续推,`live/kill.rs` 写 KillWord,`live/quarantine.rs` 毒丸隔离 |
| `src/conflate.rs` | BBO 合并 |
| `src/dispatch.rs` | kind 分派表、CSR 索引与 `sub_ix`(含按 `EvMask` 与引擎持仓驱动订阅的并集下发 `FeedCmd::Subscribe`)、`L2Batch` 整批应用、OMS 输出的排空顺序、`NotifyQueue` 与 `NotifyBacklog`(续推、背压) |
| `src/clock.rs` | 引擎时钟基准、`engine_ns` 换算(乘数与移位、±1 校正、`ClockRange`)、`stop_engine_ns`、`EngineClock` 处理 |
| `src/liveness.rs` | 按 (venue, 槽) 的连接活性、成员位图、`BboView` 按值投影 |
| `src/strategies.rs` | 实例表、`StrategySet` 调用、Ctx 应用、`IntentHandle` 回填、`spec_seq` |
| `src/views.rs` | `SeqViews`(SDK `Views`/`ModelViews` 实现)、`RiskView` 适配器、`LegSummary` 投影 |
| `src/intents.rs` | 意图管线:模式门 → classify → evaluate → commit → OMS |
| `src/exec_glue.rs` | 目标转交、执行层推进与 `ExecDecision`、`cancel_resting` 转交 |
| `src/advice.rs` | Advice 路由 |
| `src/funding.rs` | 资金费合成 |
| `src/recon.rs` | `ReconcileReport` 暂存、`Funding` 条目到达即处理、整组处理、`AUTO_EXTERNAL` 复核(合成调 hft-ledger `auto_external_fill`) |
| `src/fanout.rs` | 输出扇出与 `OmsContinue` |
| `src/timers.rs` | `TimerHeap` 与跨重启平移 |
| `src/snapshot.rs` | 段滚动、关键帧、各组件 `snapshot` 的写出顺序、哈希与头部回填、从快照起步的派生重建 |
| `src/recover.rs` | `EVENT_END` 截断、快照选择、恢复结束、不一致处理、版本判断 |
| `src/commands.rs` | 命令应用与 `CmdResult`、`ModeErr` 映射、Viewer 预览表、`SetParams` 通用校验(含 `LotMismatch`、`DustBelowMinNotional`、`NonProdOnly`、`ModelMemBudget`、`GroupId` 高位)、`ReplaceRefdata` 复核(含 Drain 末二次复核与撤销待生效引用)、`WriteOffResidual`、`SmokeOrder`、停机标记的 `Kill` |
| `src/drain.rs` | Drain 状态机 |
| `src/advisor.rs` | `Advisor`(镜像、Model、outbox、分块模型检查点) |
| `src/advisor/checkpoint.rs` | Model 检查点文件编解码、CRC32C、键与分节校验;无 I/O |
| `benches/seq.rs`、`benches/l2burst.rs`、`tests/{recover,determinism,advisor,sim_route,drain,poison,clock,liveness,notify_backlog,viewer_confirm,keyframe,preempt,recon,commands}.rs` | 见测试 |

</details>

`ReconcileBatch` 作为一次 Input 处理,先验证完整体,不与已暂存的同 report_id 普通分组混用。**批次来自 warm 平面(reconciler 重发、一张 REST 凭据与 WS 推送不一致都可能发生),其内容错误永不使引擎停机**:体不可解码 → 写 P0 `Note{ReconcileRejected, reason = 1}`(report_id/venue/account 取自可读的头,否则 0),不写 ReconcileCash,不做任何处理;同 (report_id, account, venue) 的普通分组仍在途 → `ReconcileRejected{reason = 2}` + `ReconcileCash{applied = 0}`,不处理本批任何条目(不并入在途分组的计数);ledger 或 OMS 整组预检失败 → `ReconcileRejected{reason = 3 | 4, detail = 错误判别值}` + `ReconcileCash{applied = 0}`,其余按"不适用"处理(不补账,仍完成对账分级)。字段归 [hft-event](../hft-event/README.md#变长记录前缀)。`Fatal` 只留给引擎内部不变量被破坏的情形。全组 AUTO_EXTERNAL 的用户流稳定、无 Unknown/在途、view_seq 未被新成交越过、当前仓位等于 engine_qty 等条件全部成立,且 Summary 不含 DOWN/TRUNCATED/HISTORY_UNCHECKED 时才应用。处理任何资金费、OMS 或账本变动前,先完成 ledger 与 OMS 的整组预检。先写 ReconcileCash(应用或跳过),再按 Position 报告顺序、同品种原始成交顺序调 apply_reconciled_external,每笔仍走原有 OMS、执行层和策略通知路径。任一适用性条件不成立,整组不补账,仍完成对账分级。原始金额只来自输入凭据;旧 ReconcileReport 不给严格现金模式补造现金。

严格结算原始输入先经 OMS 来源预检。已保留的同键同金融字段原件直接结束,不调用账本或重复通知;它不能作为 AUTO_EXTERNAL 的新缺口凭据。首次确实完整入账后才允许 OMS 保存原件,普通重复与确认不提供新证明。跨来源累计重复的范围归 [OMS 回报匹配](../hft-oms/README.md#回报匹配与成交增量),其余重叠仍失败。

待确认凭据恢复归 hft-ledger,原始来源与单笔证明恢复归 hft-oms;批次无跨 Input 暂存状态。

## 延迟与资源契约

分段名以 [latency-budget](../../docs/latency-budget.md) 为准:本 crate 负责 `ingest` 中的环取出部分、`decide` 全段以及 `leg_gap`/`hedge_host`/`rep_to_order` 的 sequencer 内部分;`decide`、`tick_to_trade` 与 `rep_to_order` 的分配与验收数值只在 latency-budget §3 与 [testing ⑩](../../docs/testing.md#8-上线门槛),本页只给组件预算。

| 入口 / 路径 | p50 | p99 | 基准名 |
|---|---|---|---|
| 每条 Input 的固定开销(append + engine_ns 换算 + 分派 + end_event,不含组件) | ≤ 150 ns | ≤ 400 ns | `seq/overhead_per_event` |
| 让位用的环头检查,按环数 R 计(每环一次 Acquire 读;生产者刚写过的环头缓存未命中),加一次定时器到期比较 | ≤ 5 ns × R | ≤ 30 ns × R(待实测:参考机 R = 13,其中 3 个环的生产者在另一核持续写入) | `seq/head_check` |
| BBO 在 md 突发下的等待:每个 `Bbo` 样本 `bbo_wait` ≤ 同次运行单条 md 消息应用时间的最大值 + 环头检查 + 环跳 | — | 断言 | `seq/l2burst`(单进程,不依赖 mock;驱动与断言见 [benchmarks](../../benchmarks/README.md)) |
| `ConnState{Alive}`(只更新连接活性) | ≤ 80 ns | ≤ 250 ns | `seq/conn_alive` |
| `ConnState{Down, inst = 0}`(连接退役,清该 venue 1 024 个 inst 的成员位) | ≤ 3 µs | ≤ 8 µs | `seq/conn_down` |
| `Views::bbo(i)` 按值投影(2 个成员) | ≤ 15 ns | ≤ 40 ns | `seq/bbo_view` |
| `Timer{Sys, NotifyContinue}`(投递 64 条积压通知,每条 1 次空策略调用) | ≤ 10 µs | ≤ 30 µs | `seq/notify_continue` |
| `Bbo` → 1 个策略未命中 | ≤ 250 ns | ≤ 600 ns | `seq/bbo_miss` |
| `Bbo` → 策略未命中 + 执行层推进 1 个目标(带内,不下单) | ≤ 350 ns | ≤ 800 ns | `seq/bbo_target_idle` |
| `Bbo` → 策略命中 → evaluate/commit → OMS → 出环(`decide`) | ≤ 2 µs | ≤ 5 µs | `seq/bbo_fire` |
| `Bbo` → 执行层切片 → evaluate/commit → OMS → 出环 | ≤ 2 µs | ≤ 5 µs | `seq/bbo_exec_fire` |
| `L2Batch`(20 档)整批应用 + 1 次策略调用 | ≤ 1 µs | ≤ 3 µs | `seq/l2batch_20` |
| 成交回报 → 第二腿出环(`leg_gap` 与 `rep_to_order` 的 sequencer 部分,含 ledger 与通知) | ≤ 1.5 µs | ≤ 4 µs | `seq/fill_to_hedge` |
| `Advice` 路由 + 1 次策略调用(载荷 512 B) | ≤ 300 ns | ≤ 800 ns | `seq/advice_route` |
| `MarginReport` 处理(不在 tick-to-trade 路径) | ≤ 2 µs | ≤ 5 µs | `seq/margin_report` |
| `ExternalFill` 处理(不在 tick-to-trade 路径) | ≤ 2 µs | ≤ 5 µs | `seq/external_fill` |
| `ReconcileReport`:一条 `Funding` 条目到达即处理 / 同组末条的整组处理(256 个非 Funding 条目,含 oms/risk 分级与 `funding_window_sweep`;ctl 优先级,条目数上限归 reconciler) | ≤ 600 ns / ≤ 15 µs | ≤ 1.5 µs / ≤ 40 µs | `seq/recon_funding_item`、`seq/recon_group` |
| `InjectWarmup`(4 KiB 载荷:append + `CmdResult`,不投递快速部分)/ `StrategyCmd` 路由 + 1 次策略调用(命令路径,不在 tick-to-trade) | ≤ 2 µs / ≤ 500 ns | ≤ 5 µs / ≤ 1.2 µs | `seq/inject_warmup`、`seq/strategy_cmd` |
| 关键帧一次写完(≤ 2 MiB,不算 `state_hash`;按周期或事件数到期、在空闲轮之后,最大推迟后在事件边界强制;引导块与 Drain 快照同量级,另算 `state_hash`) | ≤ 150 µs | ≤ 300 µs(p99.9 记录项,[latency-budget §3.1](../../docs/latency-budget.md#31-p999-记录项)) | `seq/snapshot_keyframe` |
| 恢复/验证回放吞吐 | 单核 ≥ 20 M 事件/分钟 | — | `seq/replay_throughput`([08 §5](../../design/08-数据面与回放.md)) |
| advisor 镜像吞吐(不含 Model) | 单核 ≥ 20 M 事件/分钟 | — | `seq/mirror_throughput` |

- 运行期零分配、零释放(旧参数对象经 `retire` 交还 warm 线程;实例创建在命令路径、使用启动期预分配的槽);写 RAM 段是内存写,无系统调用;策略经枚举静态分派,无 `dyn`;sequencer 不向任何 Park 线程发唤醒。
- 共享缓存行:心跳、kill 字(sequencer 写、gw 读,每个 `KillWord` 独占缓存行)、各环头尾索引(BBO、advice 与 md 每条之后读 ctl/rep(md 另读 bbo)的生产者索引)。
- 周期性停顿:关键帧每个周期或每 `keyframe_max_events` 条 Input 一次、一次写完(上表),段滚动的引导块每段一次(整点滚动为主,段用量触发的滚动另计)、一次写完,`NotifyContinue` 背压时实盘回报仍被取;这些只影响 p99.9(只记录,[latency-budget](../../docs/latency-budget.md))。
- advisor 镜像持有一份与 sequencer 同规模的状态(内存计入 hft-core 的主机类预算,见 [apps/hft-core 内存与 journal 段](../../apps/hft-core/README.md#内存与-journal-段按主机类唯一归属))。
- 检查点文件编解码只在冷路径调用;`decode` 零分配、O(文件长),`encode` 分配一份文件大小的缓冲。两者均受调用方的体长上限约束,不计入 sequencer 热路径或 advisor 分块写入的预算。

## 测试与验收

用例与验收见 [TESTS.md](TESTS.md)。

定时器的 GroupId 到当前 gix 映射、执行层恢复合并(含 Broken 的闸门滞留告警)及跨频率周期重新登记由真实引擎和独立回放验证,见 [SEQ-234](TESTS.md#seq-234-策略定时器以-groupid-为键投递时现查-gix)、[SEQ-238](TESTS.md#seq-238-执行层的恢复定时器与堆中同键条目合并保留较早者) 与 [SEQ-239](TESTS.md#seq-239-周期性系统定时器每次启动重新登记替换旧条目更早的-omssweep-保留)。`hft-exec/testing` 仅在 dev 依赖中用于准备冲突截止的快照夹具。

独立定时器回放夹具经 `B0::boot_self_contained` 写入完整 ECF1/RKP1,含用例显式设置的执行层参数,回放端从 journal 装配配置。精简 B0 的注入解码器用于其余单元用例。

advisor 的定时投递、重启注记去重、落后量、两种环容量的独立合并模型,以及三种检查点块大小的完整载荷续算见 [SEQ-036–046](TESTS.md#seq-036-model-的时间刻度只来自-journal-中的-timersys-advisortick)。这些验收在测试进程中生成 journal,不替代固定回放样本与参考主机延迟门槛。

## 依赖

- 内部:`hft-types`、`hft-refdata`、`hft-event`、`hft-book`、`hft-oms`、`hft-risk`、`hft-ledger`、`hft-exec`、`hft-strategy-sdk`(`StrategySet`、`Views`、`EventRef`、`Ctx`、意图类型)、`hft-journal`(读者;写者在 feature `live` 下,只有 apps/hft-core 与 apps/replayer 打开)、`hft-rt`(环、`ByteSpsc`、`Mailbox`、`KillWord`、`TscClock`、`Heartbeat`)。
- 外部:`xxhash-rust`(xxh3,无分配流式接口)、`sha2`(0.10,冷启动核对 journal 配置内容,不启用 asm)、`zerocopy`(快照与配置行布局)、`crc32c`(冷路径检查点文件校验)。
- 禁止:任何 `strategies/*`(策略只在装配处经 `strategy_set!` 进入)、`hft-strat-lib`、`hft-sim`(sim-gw 由 hft-core 装配)、`hft-net`、`hft-venue`、`venue-*`、`tokio`、任何 `cold` crate。

## 延伸阅读

- [快照校准元数据与回放比对](../../.agents/notes/implemented/bug-fix/2026-10-12-snapshot-clock-calibration.md)
- [策略定时器身份持久化](../../.agents/notes/implemented/bug-fix/2026-10-12-strategy-timer-identity.md)
- [策略定时器的共享配额与取消身份](../../.agents/notes/implemented/bug-fix/2026-10-12-strategy-timer-admission.md)

- [离线 Model 起点与配对决定](../../.agents/notes/implemented/architecture/2026-10-12-offline-model-origin.md)、[跨重启 Model 来源连续性](../../.agents/notes/implemented/bug-fix/2026-10-12-model-restart-continuity.md)

- [round8 裁决](../../design/_work/round8-decisions.md)(R8-2 延迟优先、R8-3 解耦、R8-5 模拟账户、R8-8 模拟让位于实盘、预热注入、断腿自动处置、R8-10、R8-11);评审证据 [review-2026-10-02](../../design/_work/review-2026-10-02-主策略适配与执行正确性.md) §4,[review-2026-10-03](../../design/_work/review-2026-10-03-M0独立评审.md),[M0-b 评审](../../design/_work/review-2026-10-03-M0b独立评审.md),[M0-c 五路汇总](../../design/_work/review-2026-10-03-M0c五路汇总.md)(L-P0-2 md 让位、L-P1-2 BBO 成员聚合、M-P1-1/3/5、S-P1-7、V5 P2-1/2/6),[M0-d 五路汇总](../../design/_work/review-2026-10-03-M0d五路汇总.md)(A-P0-1 关键帧一次写完、L-3 BBO/advice 让位、M-P2、W3 P1-12/16/18/21/22),[M0-e 五路汇总](../../design/_work/review-2026-10-03-M0e五路汇总.md)(X1 P1-A…E、X3 P1-6/15、X5 L-P2-5/6)
- Agent Note:[引擎时间 engine_ns](../../.agents/notes/proposed/architecture/2026-10-03-engine-time.md)、[报价新鲜度 = 连接健康](../../.agents/notes/proposed/architecture/2026-10-03-quote-freshness-conn-health.md)、[sequencer md 让位](../../.agents/notes/proposed/architecture/2026-10-03-sequencer-md-preempt.md)、[resolve_broken 只在 HARD 解除时](../../.agents/notes/proposed/architecture/2026-10-03-resolve-broken-hard-only.md)、[AUTO_EXTERNAL 前置条件](../../.agents/notes/proposed/architecture/2026-10-03-auto-external-guard.md)、[持仓驱动订阅](../../.agents/notes/proposed/architecture/2026-10-03-position-driven-subscription.md)
- [01 §3.3、§5](../../design/01-总体架构.md)、[08 §2、§5、§6](../../design/08-数据面与回放.md)、[09 §4.2](../../design/09-可观测性、风控与控制面.md)、[13 R6、R7、R10、O9](../../design/13-设计评审与优化.md)
- [event-model](../../docs/subsystems/event-model.md)、[journal-format](../../docs/subsystems/journal-format.md)、[control-plane](../../docs/subsystems/control-plane.md)、[hot-path-rules](../../docs/hot-path-rules.md)、[latency-budget](../../docs/latency-budget.md)、[testing](../../docs/testing.md#8-上线门槛)
- [hft-strategy-sdk](../hft-strategy-sdk/README.md)、[hft-exec](../hft-exec/README.md)、[hft-sim](../hft-sim/README.md)、[apps/hft-core](../../apps/hft-core/README.md)、[apps/replayer](../../apps/replayer/README.md)

- [指定快照恢复审计范围](../../.agents/notes/implemented/bug-fix/2026-10-11-snapshot-recovery-audit-range.md)
- [恢复快照的引擎时间](../../.agents/notes/implemented/bug-fix/2026-10-11-recovery-snapshot-engine-time.md)

### 非正式账户 bootstrap

`Sequencer::boot_accounts` 只给新 journal 的非正式 Live 账户接受余额/持仓基线；绑定错误、重复资产/持仓、正式或 Sim 账户、无效持仓拒绝启动。策略初始化前建立账户 unassigned 状态并写第一份完整快照。RateOnly 真实结算输入先记录再检查差异，不自动纠正现金。

## 已知限制与后续工作

- 含绑定实例的旧开发快照若缺节 27,恢复返回 `SnapshotUndecodable{section: 27}`;即使堆为空也不能推断 ID 计数器为 1。旧版本会跳过未知节,但不具备稳定 TimerId 的恢复契约,不作为使用该能力的回滚版本。固定样本须由支持节 27 的版本生成;上一已验证版本的兼容升级与回滚仍须负责人评审。

- `Log::from_reader` 当前在显式上限内保留整份逻辑日志;长时段文件的分段流式验证仍待接入。独立恢复运行支持连续、封闭文件;RecoveryGap 输入仍不接受。[replayer 限制](../../apps/replayer/README.md#已知限制与后续工作)列出完整 CLI 验收缺口。

- **关键帧一次写完**:每个周期或每 `keyframe_max_events` 条 Input 一次 ≤ 300 µs 的停顿,在空闲轮之后发生(最大推迟后在事件边界强制),期间到达的输入等待;段滚动的引导块(每段一次)与 Drain/Recovery 快照同样一次写完。可选扩展:由 advisor 镜像在旁路生成关键帧(镜像在每个 seq 的状态与 sequencer 相同,sequencer 只写滚动时的引导块;触发条件:实测关键帧停顿超出 p99.9 记录目标)。
- **快照之后的 `BboView.rx_tsc`**:hft-book 的快照行不含 `Top.rx_tsc`(恢复后为 0),为了实盘与从快照起步的回放一致,早于最近快照的接收时刻一律给 0(关键帧见[快照](#快照),引导块每段一次);`FundingView.upd_tsc` 同样。用 `rx_tsc` 做信号的策略在安静 inst 上会周期性看到"未知";`hw_rx_ns`(在节 10)不受影响。可选扩展:把 `Top.rx_tsc` 作为登记的 TSC 行进快照(触发条件:某策略的信号因此失效)。
- **md 让位的粒度是一条消息**:一条超大的 `L2Batch`(交易所一次推送数千档)仍整条应用;REST 快照已按 `feed.snapshot_part_levels` 分片。可选扩展:按档切分超大增量(触发条件:实测单条增量档数使 `bbo_wait` p99 超预算)。
- **下单路径预热**:不做任何"每 tick 空跑下单路径"。可选扩展:无副作用的预热路径(触发条件:模拟账户长跑中 `seq/bbo_fire` 冷/热差使 `decide` p99 超预算)。
- **refdata 不热替换**:`ReplaceRefdata` = 复核 + Drain + 重启。可选扩展:允许热替换的变更集(触发条件:Drain + 重启的停机窗口在运营中不可接受)。
- **advisor 镜像的成本**:镜像重放全部 Input,CPU 与内存约等于 sequencer 本身;在 2 核主机类上与其他 warm 任务共享一个核。写模型检查点期间整个 advisor 停顿(块数 × ≤ 1 ms)。可选扩展:按 Model 订阅裁剪镜像(只应用行情与该实例相关的执行事件)(触发条件:镜像落后量持续超过 `advisor_max_lag_ms`)。
- 二次投递中的目标变更推迟到下一相关 Input 才推进(至多一个行情间隔)。
- **同进程混跑模拟**:实例划分保证 `prod` 不含 Sim 账户,混跑只出现在测试中;若混跑,同一 Input 的出环在 `end_event` + `publish` 之后统一推环(崩溃截断的正确性要求),模拟实例对同一条行情的处理时间计入该 Input 实盘订单的出环时刻,模拟实例的行情处理量也不受 `sim_max_per_round` 限制。
- **跨重启的 TSC 平移**:定时器堆与 OMS/exec 的 TSC 量在恢复结束时平移,`EngineClock` 在引导块之后才写,两者之间的启动耗时(毫秒级)不计入任何截止;按 engine_ns 记截止的登记方因此可能收到早到的定时器,由其复核后重新登记。
- **BBO 连接退役的成员清理**是 O(该 venue 订阅的 inst 数),只在连接退役时发生(每条连接约 24 h 一次)。
- **旧能力 `AUTO_EXTERNAL` 合成的成交价**取处理时的标记价,不是交易所的 ADL 成交价;差额留在已实现 PnL 里,由之后的对账与人工 `AdoptExchange` 校正;交易所之后补推的同向真实强平/ADL 由 hft-ledger 的 `auto_held` 抵扣,不重复入账(规则与残留情形归 [hft-ledger](../hft-ledger/README.md#外部成交externalfill))。
- 多 sequencer(按资产分片)不在本规格。

### 开发备忘

<details><summary>给实现者</summary>

- **植入缺陷(feature `planted`,只在测试中)**:`src/planted.rs` 按线程打开的开关:对冲定价以盘口最后变化时刻代替连接活性(`hedge-px-book-age`);`planted::DEFECTS` 为全部缺陷的名字表,`planted::set`/`restore` 按名字切换本线程的各 crate 开关。只由 hft-testkit 的 `planted` feature 打开,生产清单、`seq` 与 `dst-bin` 都不含它([TK-178](../hft-testkit/TESTS.md#tk-178-植入缺陷不进入任何生产构建));真实引擎 DST 发现各缺陷的验收归 [TK-179](../hft-testkit/TESTS.md#tk-179-真实引擎的植入缺陷在有界种子内被不变量发现) 与 TK-053.21–TK-053.23。
- **待核实**:单条交易所深度增量消息的档数分布(决定 `bbo_wait` 的实际上界),方法:用现有采集的 raw 数据统计各所 `L2Batch` 档数的 p99/max(M3 前);满容量关键帧的字节数与一次写完的耗时,方法:`seq/snapshot_keyframe` 在参考机上按满容量配置测量;关键帧与引导块的 2 MiB 顺序写是否冲刷本核 L2、改用非临时存储(STNP)能否消除:待实测(方法:M3 末探针测关键帧后 1 ms 内 `decide` 的 p99,普通存储与 STNP 两种写法 A/B;写入路径在 hft-journal 的 append);环头检查的每环成本,方法:`seq/head_check` 在参考机上按 R = 13、3 个活跃生产者测量。
- 实现顺序:`RecoverDriver` + `EVENT_END` 截断 → 引擎时间(`EngineClock`,乘数-移位换算与 `ClockRange`)→ `LiveDriver` 取环顺序、让位、定时器中途检查、环境决定器 → 连接活性(槽、成员位图、按值投影)→ 实例表与 `StrategySet` 胶水 → 意图管线 → 定时器 → 快照(各组件写出顺序与哈希回填)→ 扇出 → 执行层胶水 → 资金费合成 → 对账报告(含 `AUTO_EXTERNAL`)→ 合并 → Drain → advisor(镜像 + Model + 分块检查点)→ 毒丸隔离 → `VerifyDriver`(离线)→ `seq/l2burst` 单进程基准(M2)。
</details>
