---
description: "raw/seq/chain 三类 journal 的写者、读者、段预创建、滚动与引导块、persist 复制、崩溃恢复、保留与校验;sequencer/feed/gw/chain-core 写,冷进程与回放读"
kind: crate
tier: hot
scope: core
status: spec
---

# hft-journal

## 概要

`hft-journal` 实现 [journal-format](../../docs/subsystems/journal-format.md) 规定的文件格式:热路径上的 `JournalWriter`(RAM 热段追加、提交、滚动、引导块框架),persist 线程上的 `SegmentPreparer`(预创建与预触摸下一段)、`Persister`(复制到冷副本 + `fdatasync` + 封存)与 `HotRetirer`(热段退役),任意进程里的 `JournalReader`(尾随、多段重组、热/冷切换、缺段处理),以及启动期的恢复与离线校验。它不决定写**什么**(事件内容由 [hft-event](../hft-event/README.md) 定义、由 sequencer/venue 运行时产生)、不决定**何时**滚动与出快照(live driver 写成 `Timer` 输入记录,[hft-sequencer](../hft-sequencer/README.md))、不导出状态快照内容(sequencer 汇集各 crate 的节并一次写完,写出时机归 hft-sequencer;本 crate 不计算状态哈希,也不提供回退已追加记录的接口)、不创建线程(装配在 [apps/hft-core](../../apps/hft-core/README.md),以及 optional 的 [apps/chain-core](../../apps/chain-core/README.md))。热路径上它是每个事件的第二步:sequencer 取出事件 → `append` → 簿/策略。

## 目录

- [使用本组件](#使用本组件)
- [实现说明](#实现说明)
- [延迟与资源契约](#延迟与资源契约)
- [测试与验收](#测试与验收)
- [依赖](#依赖)
- [延伸阅读](#延伸阅读)
- [已知限制与后续工作](#已知限制与后续工作)

## 使用本组件

### 写者

```rust
pub struct JournalConfig {
    pub name: JournalName,           // "seq" | "raw-feed-binance-usdm" | "raw-feed-binance-usdm-bbo"(feed-bbo 任务,命名归 hft-venue)| "raw-gw-okx-swap" | (optional 链上)"raw-chain-bsc-node" | "chain-bsc"
    pub kind: JournalKind,           // Raw | Seq | Chain
    pub venue: VenueId,              // raw 专用
    pub instance: InstanceName,      // 目录名,只允许 "prod" | "sim"(见下);测试、DST 与 replayer 用各自的临时根目录
    pub hot_dir: PathBuf,            // hugetlbfs 或 tmpfs(/dev/shm/hft)上的目录;各 journal 用哪个挂载点按主机类与实例归 apps/hft-core 内存表
    pub cold_root: PathBuf,          // NVMe/EBS 挂载点
    pub segment_bytes: u64,          // 取值按主机类由 apps/hft-core 给出(唯一归属);本 crate 只规定约束,见"段大小与热段内存";须是 page 的整数倍且 > 4096
    pub page: PageSize,              // Huge1G | Huge2M | Normal(4 KiB,tmpfs,见"小段 + 4K 页模式");各 journal 的页大小同样归 hft-core 内存表
    pub size_idle_pct: u8,           // 默认 75(常量 DEFAULT_SIZE_IDLE_PCT):安全阀软阈值,到达后在空闲轮滚动(调用约束第 5 条)
    pub size_valve_pct: u8,          // 默认 90(常量 DEFAULT_SIZE_VALVE_PCT):安全阀硬上限,到达后在下一个事件边界滚动;须 0 < size_idle_pct < size_valve_pct < 100
    pub hot_keep: u8,                // 同上:按主机类由 apps/hft-core 给出
}
pub struct JournalName(pub String);  // ASCII [a-z0-9-],1–32 字节(open 校验,否则 BadConfig);段头 journal_name 右补 0
pub struct WriterEnv {               // 装配时给出,open 按值取走;预创建段经 prepared 交给写者,旧段经 retired 交还 persist 线程(见旧段回收)
    pub writer_epoch: u32, pub build_git: [u8; 20], pub host_profile_sha256: [u8; 32],
    pub prepared: Consumer<PreparedSegment>, pub retired: Producer<RetiredSegment>,
}
#[repr(u8)] pub enum OpenErr {
    DiskBacked = 1,      // 热目录 fstatfs 不是 HUGETLBFS_MAGIC/TMPFS_MAGIC
    OfflineTarget = 2,   // open_offline 的目标是 hugetlbfs/tmpfs 或位于生产 journal 根之下
    BadConfig = 3,       // JournalConfig 违反上面的约束、实例名或 journal 名非法
    Io = 4,              // 打开、映射、读取、复制失败
    ChainBroken = 5,     // 热目录或冷目录的段号不连续、段头链(prev_header_sha256、first_seq)不连续且无 RECOVERY_GAP
    ColdHotConflict = 6, // 同一段的冷副本与热段在 [0, synced_len) 上(除写者行与 persist 行)不同,或 synced_len > 0 而冷副本缺失或更短(恢复第 1 步)
    SeqUnavailable = 7,  // 读者 StartAt::Seq(n) 的 n 早于最早可读记录或晚于 last_seq + 1
    NotFound = 8,        // 读者:该 journal 没有任何段
}

/// hft-core 实例名:`Prod` = "prod"(交易所账户,不含模拟账户);`Sim` = "sim"(只含模拟账户的实例;dev2 与统一升级主机都有,升级主机上与 prod 同机,见 apps/hft-core)。
/// 两个实例的热段、冷副本与 meta 目录按名字分开,永不写同一目录;没有其他取值(配置里出现其他名字 → 加载失败)。
#[repr(u8)] pub enum InstanceName { Prod = 1, Sim = 2 }   // FromStr:只接受 "prod" | "sim",其他名字失败

pub struct JournalWriter { /* Send、!Sync;每 journal 恰好一个 */ }
impl JournalWriter {
    pub fn open(cfg: &JournalConfig, env: WriterEnv) -> Result<(JournalWriter, RecoveryReport), OpenErr>; // 启动期,含恢复;env 的两个环端被写者取走
    pub fn open_offline(cfg: &JournalConfig, out_dir: &Path) -> Result<JournalWriter, OpenErr>;              // 只给 replayer:普通目录,见 cargo feature
    // —— 热路径 ——
    pub fn append<B: Body>(&mut self, flags: RecordFlags, tsc: Tsc, body: &B) -> Result<Seq, AppendErr>;
    pub fn append_var(&mut self, kind: EventKind, flags: RecordFlags, tsc: Tsc, pieces: &[&[u8]]) -> Result<Seq, AppendErr>;
    pub fn reserve_var(&mut self, kind: EventKind, flags: RecordFlags, tsc: Tsc, len: u32) -> Result<VarSlot<'_>, AppendErr>;
    pub fn publish(&mut self);                           // Release 写 committed_len、last_seq;只推进到最近一次事件边界(最近一次 end_event,或其后事件之外的 Meta 末尾),事件中途调用不暴露未结束事件的记录
    pub fn end_event(&mut self);                         // 在自上次 end_event 以来最近一条已追加的非 Meta 记录上置 EVENT_END;事件内的 Meta 不置位(journal-format 提交游标第 5 条);期间没有非 Meta 记录 → 不改任何记录
    pub fn append_meta<B: Body>(&mut self, tsc: Tsc, body: &B) -> Result<Seq, AppendErr>; // Meta 记录;事件之外写入时自带 EVENT_END,事件之内不置位
    pub fn next_seq(&self) -> Seq;
    pub fn needs_roll(&self, now: Tsc) -> Option<(RollReason, RollAt)>; // 两级安全阀(调用约束第 5 条);raw 另含整点(set_roll_deadline)
    pub fn set_roll_deadline(&mut self, at: Tsc);        // raw 写者:下一个 UTC 整点对应的 TSC
    // —— 滚动(两个事件之间调用)——
    pub fn roll(&mut self, reason: RollReason, info: &RollInfo) -> Result<BootWriter<'_>, RollErr>;
    pub fn seal_and_close(self) -> Result<(), RollErr>;  // 干净退出:写 SegmentRoll{End, other_segment_no = 0, reason = Manual}、publish、Sealed
}
impl VarSlot<'_> { pub fn buf(&mut self) -> &mut [u8]; pub fn commit(self) -> Seq; } // 就地写入(raw payload 直接从 TLS 缓冲拷入);未 commit 就 drop → 不追加、next_seq 不变、已写字节清零;
                                                                                   // 多段组(> 65 488 B)经写者自有的暂存缓冲,commit 时按多段规则写出
pub struct RollInfo<'a> { pub utc_ns: i64, pub calib: CalibPair, pub tsc_hz_milli: u64, pub configs: &'a [BlobRef] } // 条数不设上限;段头只存条数与摘要
pub struct BootWriter<'a> { /* 借用 writer,只在引导块期间存在 */ }
impl BootWriter<'_> {
    pub fn append<B: Body>(&mut self, tsc: Tsc, body: &B) -> Result<Seq, AppendErr>;
    pub fn append_var(&mut self, kind: EventKind, tsc: Tsc, pieces: &[&[u8]]) -> Result<Seq, AppendErr>;
    pub fn finish(self) -> Result<Seq, AppendErr>;        // seq/chain:写 BootDone{boot_records};raw:不写任何记录,只结束借用
}
#[repr(u8)] pub enum AppendErr {
    Full = 1,      // 当前段预留区之外放不下整条(或整组;BootWriter 内可动用预留区,预留区也不够才 Full);不写任何字节;sequencer → Fatal(JournalFull) + abort
    TooLarge = 2,  // 单条 > 65 536 B、Var 记录超过 KindInfo.max、多段组 parts > 65 535 或 total_len ≥ 4 GiB,或整组超过一个空段的普通空间(segment_bytes − 4096 − 预留)
    Sealed = 3,    // 没有可写段:open 之后、首次 roll 之前
    InBoot = 4,    // 上一个 BootWriter 未 finish 就被 drop(引导块未完成)
}
#[repr(u8)] pub enum RollErr {
    NextNotReady = 1, // 预创建通道为空;不写任何记录,继续用当前段
    Io = 2,           // 取到的预创建段不可用(段头 [P] 与本 journal 不符、segment_no ≠ 当前 + 1 或 state ≠ Preparing);丢弃它,不写任何记录,继续用当前段
    InBoot = 3,       // 同 AppendErr::InBoot
}
#[repr(u8)] pub enum RollAt    { IdleRound = 1 /* 软阈值:调用方在第一个空闲轮滚动 */, EventBoundary = 2 /* 硬上限或 raw 整点:下一个事件边界 */ }
```

调用约束:

1. **一个 journal 一个写线程**;`seq` journal 的 `seq` 就是 sequencer 的全局序号——**`append` 返回的 `Seq` 是 seq 的唯一来源**,sequencer 与任何调用方都不另行计数。已追加的记录只能随事件提交或随恢复截断消失,写者不提供回退接口。
   - **事件边界**:sequencer 写完一个输入事件的全部记录后调用 `end_event()`,写者给该事件最后一条非 Meta 记录的头部置 `EVENT_END`。`publish()` 只在 `end_event()` 之后调用(从不在事件中途发布;中途调用也只推进到上一个事件边界),因此 `committed_len` 之内的记录 flags 都已是终值——热读者与 persist 只读到 `committed_len`,永远看不到"尚未置位"的中间态。Meta 记录用 `append_meta`:写者若处于事件之内(自上次 `end_event()` 以来已追加过非 Meta 记录),该 Meta 不置位,事件的 `EVENT_END` 仍落在 `end_event()` 时的最后一条记录上;事件之外的独立 Meta 自带 `EVENT_END`。
   - **raw 与引导块**:`Raw` 写者给每条记录(多段组的末段)自动置 `EVENT_END`,`end_event()` 对它是空操作;`seq`/`chain` 引导块内只有 `BootDone` 置 `EVENT_END`(`Begin` 与其余引导块记录不置位,引导块整体是一个单元)。
2. **先记录、后生效**:sequencer 在处理事件前 `append`;在把任何依赖这些记录的帧提交到出环**之前**调用 `publish()`(见 [journal-format 提交游标](../../docs/subsystems/journal-format.md#提交游标))。
3. `roll` 只在两个事件之间调用;返回的 `BootWriter` 期间不得追加普通事件;`seq`/`chain` 的引导块内容与顺序见 [journal-format 段滚动与引导块](../../docs/subsystems/journal-format.md#段滚动与引导块)。`open` 完成恢复后写者没有可写段(恢复已把最后一段封存):调用方的第一个操作是 `roll(report.boot_reason 或 RecoverFromSnapshot, …)`,它直接打开下一段、不再写 `End`;此前 `append*` 返回 `Sealed`。
4. `roll` 返回 `NextNotReady`(下一段尚未预创建)时,写者继续使用当前段,调用方下一个事件边界重试,persist 线程计数并告警。
5. **段满(两级安全阀)**:写者为滚动永久预留 `reserve = 2 × 当前段引导块的字节数`(`seq`/`chain`:从 `Begin` 到 `BootDone` 含两端;raw:`Begin` 一条),记 `used = committed_len + reserve`,容量 = `segment_bytes`,阈值按整数比较 `used × 100 ≥ pct × segment_bytes`。到 `size_idle_pct` 时 `needs_roll` 返回 `(Size, IdleRound)`:调用方在**第一个空闲轮**滚动(seq/chain 由 live driver 写 `Timer{Sys, Roll}`,写法归 [hft-sequencer](../hft-sequencer/README.md));一直没有空闲轮而到 `size_valve_pct` 时返回 `(Size, EventBoundary)`:调用方在**下一个事件边界**滚动(不等空闲轮)。这样多数大小滚动(及其引导块停顿)落在空闲时段,硬上限保证不停写。raw 写者没有引导块,只用硬上限(对 raw 不返回 `IdleRound`),在下一个帧边界滚动;`set_roll_deadline(at)` 之后 `now ≥ at` 时返回 `(Hourly, EventBoundary)`。多个条件同时成立时按 `(Size, EventBoundary)` > `(Hourly, EventBoundary)` > `(Size, IdleRound)` 取第一个;都不成立 → `None`。两个阈值的数值只在本 crate([配置项](#配置项))。普通 `append` 不得动用预留区;`append` 返回 `AppendErr::Full`(预留区之外已无空间且无法滚动)时,sequencer 进入 `Fatal(JournalFull)` 并 `abort`——决策可见 ⟺ 已记录,不能继续交易,撤单交给交易所死手与 kill-switch([hft-sequencer](../hft-sequencer/README.md));raw 写者收到 `Full` 的处置归 [hft-venue](../hft-venue/README.md)。
6. **CRC 由写者决定**:`seq`/`chain` 写者按 [hft-event `KindInfo.crc`](../hft-event/README.md#记录头标志与-kind) 给 `Always` 的 kind 置 `CRC` 并写校验和,`raw` 写者给全部记录加 CRC([journal-format 三类 journal](../../docs/subsystems/journal-format.md#三类-journal));调用方 `flags` 中的 `CRC`、`PART`、`EVENT_END` 位被忽略(后两者按调用约束第 1 条与多段规则由写者置)。
7. **快照记录**:`StateSnapshot` 经 `append_var`/`reserve_var` 作为普通多段记录写入,本 crate 不解析其内容;实盘写关键帧时 `SnapshotHdr.state_hash` 填 0,哈希由读者侧(验证回放、replayer)计算,哪些 purpose 适用归 [journal-format 快照记录结构](../../docs/subsystems/journal-format.md#快照记录结构)。

### 读者

```rust
pub struct ReaderConfig {
    pub hot_dir: PathBuf, pub cold_root: PathBuf, pub instance: InstanceName, pub name: JournalName,
    pub start: StartAt, pub end: EndAt, pub on_gap: OnGap,
    pub reassembly_cap: usize,       // 默认 256 MiB;0 = 不重组,多段记录逐段以 Next::Record 交付
    pub tail_poll_us: u32,           // 默认 50;读到 Pending 后两次重读 committed_len 的最小间隔
    pub same_process: bool,          // 默认 false;同进程尾随读者:复用本进程已有的热段映射(固定槽位,见下),不 mmap/munmap
}
impl ReaderConfig { pub fn new(hot_dir: PathBuf, cold_root: PathBuf, instance: InstanceName, name: JournalName) -> Self; } // start = Oldest、end = Follow、on_gap = Error,其余取默认
pub enum StartAt {
    Oldest,                          // 最早仍可读的段(热或冷)的第一条
    Seq(u64),                        // seq = n 的记录(落在多段组内时从组首起);n = last_seq + 1 等同 Tail;更早或更晚 → OpenErr::SeqUnavailable
    SegmentBoot(u64),                // 段 n 的 Begin
    LatestBoot,                      // 引导块已完整(BootDone 已提交)的最新段的 Begin
    LatestKeyframe,                  // 最新的 StateSnapshot{purpose = Keyframe};最新完整引导块之后没有时同 LatestBoot
    Tail,                            // open 时的 committed_len 之后(只交付新记录)
}
pub enum EndAt   { Follow /* 尾随 */, LastCompleteEvent /* 只交付到恢复截断点(见恢复第 2 步),之后返回 End */ }
pub enum OnGap  { Error /* 返回 Gap 后停住,其后每次 poll 返回同一个 Gap */, SkipToNextBoot /* 返回一次 Gap,下次 poll 从下一个可用段的 Begin 继续 */ }
pub struct Position { pub segment_no: u64, pub offset: u64, pub seq: u64 }
pub struct GapInfo { pub at: Position /* 最后交付位置 */, pub expected_seq: u64, pub resume_segment_no: u64 /* 0 = 无可续段 */, pub reason: GapReason }
#[repr(u8)] pub enum GapReason {
    MissingSegment = 1, // 下一段从未出现过(读者打开时的段列表里没有它)而更新的段存在
    RecoveryGap = 2, ChainBroken = 3,
    Deleted = 4,        // 读者见过(打开时列出或正在映射)的段,其热段与冷副本都已删除
}
pub struct LogicalEvent<'a> { pub seq: u64 /* 第 0 段 */, pub kind: EventKind, pub tsc: u64, pub flags: RecordFlags /* 末段的 */, pub parts: u16, pub body: &'a [u8] /* 拼接后的逻辑体,不含 PartHdr */ }
#[repr(u8)] pub enum ReadErr {
    Corrupt = 1,         // len 越界或未对齐、seq 不逐条 +1、kind 不允许出现在该类 journal、多段组不连续或 Σ chunk_len ≠ total_len、段头 magic 或 `format_version` 不认识、段头 CRC 错或映射不足 4096 B
    BadCrc = 2,          // 记录 CRC 不符
    ReassemblyCap = 3,   // 多段组 total_len > reassembly_cap(首段即判定,不分配)
    DevKind = 4,         // 生产构建读到 0x7Fxx
}
pub struct JournalReader { /* 只读映射;任意进程 */ }
impl JournalReader {
    pub fn open(cfg: &ReaderConfig) -> Result<JournalReader, OpenErr>;
    pub fn poll(&mut self) -> Next<'_>;
    pub fn position(&self) -> Position;                 // { segment_no, offset, seq }
    pub fn in_boot_block(&self) -> bool;
    pub fn last_complete_event(&self) -> Option<Position>;  // 最后一条 EVENT_END 记录之后的位置;恢复与 RecoverDriver 用它截断
}
pub enum Next<'a> {
    Record(RecordView<'a>),       // 定长记录,或变长记录的单段(调用方自己重组时)
    Event(LogicalEvent<'a>),      // 多段重组完成的变长逻辑事件(seq = 第 0 段)
    Pending,                      // 暂无新数据(尾随)
    Gap(GapInfo),                 // 缺段/RECOVERY_GAP;按 OnGap 已处理或需要调用方决定
    End,                          // 离线读到最后一个已封存段末尾
    Err(ReadErr),                 // 结构损坏、CRC 错、重组超限
}
```

- 读者不做背压、不写任何共享行;慢读者不影响写者。`Pending` 时如何等待由调用方决定(冷进程可以 sleep;warm 线程用 `hft-rt::Backoff` 或 park)。`seq`/`chain` 段的 `BootDone` 提交之前,读者不交付该段的任何记录(返回 `Pending`)。
- **尾随粒度**(避免写者 `publish` 的缓存行被频繁抢走):读者在已知可读区间消费完之前不重读 `committed_len`;读到 `Pending` 之后,两次重读的间隔 ≥ `journal.reader.tail_poll_us`(同进程 warm 读者,默认 50 µs)。persist 线程按块粒度读(每次最多 `chunk_bytes`,`sync_interval_ms` 节拍)。因此每个读者对写者行的 RFO 至多每 `tail_poll_us` 一次。
- 读者最多同时映射 2 个热段;看到 `retired` 或热段文件消失时,在同一偏移切到冷副本;冷副本也已被保留策略删除时报 `Gap{Deleted}`。
- **段文件被删除后解除映射**:读者每次重读 `committed_len` 时检查该段 `retired`,并至少每 1 s 对每个已映射的段文件(热段与冷副本)`fstat` 一次,`st_nlink = 0` 视为已删除;看到 `retired = 1` 或已删除后,在同一次 `poll` 内 `munmap` 并关闭该文件,之后不再访问它(否则已删除的 hugetlbfs/tmpfs 页与冷盘空间被读者占住)。同进程尾随读者的固定段槽位(下一条)只切换指向,不 `munmap`,映射由写者进程的 `HotRetirer` 统一回收。
- **kind 的处理**:本二进制不认识的 kind 照常以 `Next::Record` 交付(调用方按 `kind_info = None` 跳过);认识但不允许出现在该类 journal 的 kind → `Err(Corrupt)`;生产构建读到 `0x7Fxx` → `Err(DevKind)`。
- **同进程尾随读者**(advisor、sim-gw、ctl-gateway;`ReaderConfig.same_process = true`)使用**预映射的固定段槽位**:复用本进程中预创建者/写者已建立的热段映射(进程内映射表,`hot_keep + 2` 个段),滚动时只切换指向,不反复 `mmap/munmap`(避免 TLB shootdown 打到热核);本进程没有该段映射时退回普通映射;冷进程读者照常映射。
- **段头读取**:`SegmentHeader` 含原子字段(`committed_len`、`state`、persist 行),不能派生 `FromBytes`/`Immutable`;读者对映射首页做对齐与长度检查后以 `unsafe { &*(ptr as *const SegmentHeader) }` 取引用(`// SAFETY:` 写明:页对齐、4096 B 可读、原子字段 8 B 对齐、映射在引用存活期内不解除),不可变区 0..3072 另以 `zerocopy` 的 `SegmentHeaderFixed` 视图校验 CRC。该 `unsafe` 只在本 crate 的 `layout.rs`。
- 验证回放驱动用 `in_boot_block()` 跳过引导块输入并比对重新生成的引导块;冷读者从 `LatestBoot`/`LatestKeyframe` 自举。

### persist 与后台维护(persist 线程调用)

`Persister::poll`、`SegmentPreparer::poll` 与 `HotRetirer::poll` 都由独立的 **persist 线程**循环调用(所有主机类上独占一个线程;升级主机类上独占一个非隔离核,不在核 0/1,不与 advisor 同核,见 [hft-core 线程计划](../../apps/hft-core/README.md#线程计划))——`fdatasync` 可能阻塞数毫秒、预创建的 `MAP_POPULATE` 可能阻塞 50–100 ms,两者都不能拖住 housekeeping 的 `sd_notify`。每轮先保证每个 journal 有一个已准备好的下一段,再复制与同步,最后退役;预创建期间复制暂停,persist 滞后相应上升。

**旧段回收**(写者线程不做任何 `munmap`/`unlink`):写者滚动后把旧段的映射句柄(`RetiredSegment { base, len, segment_no }`)经一条 `Spsc<RetiredSegment>` 交给 persist 线程(写者在 `Spin` 线程上时该环 `wake = None`,persist 靠 park 超时取走,[hot-path-rules T7](../../docs/hot-path-rules.md#线程t));`HotRetirer` 等段 n 满足"已封存、`synced_len == committed_len`、`.seal` 已写、`n + hot_keep < 当前活动段号`",置 `retired = 1`,宽限期 2 s(常量 `RETIRE_GRACE_MS = 2000`,按 `poll` 传入的 `now_utc_ns` 计,`now ≥ 置位时刻 + 2 s` 才执行)后在 persist 线程 `munmap` + `unlink`。进程内 `munmap` 使本进程在全部 CPU 上的该段 TLB 条目失效:aarch64 上是广播 TLBI(不发 IPI、不打断热核,热核只承担 DVM 同步),可选 x86 剖面上是 IPI shootdown(打断热核);频率为每段退役一次,计入 [latency-budget §3.1](../../docs/latency-budget.md#31-p999-记录项) 的 p99.9 记录项,对热核的实际影响待 M3 末满订阅探针实测(开 raw journal 与 persist、有段退役;M1.5 纵切片没有段退役)。启动自检(`HotRetirer::audit`)统计"已 unlink 仍被映射"的 hugetlbfs 页,计入大页短缺并告警。进程的 mimalloc 配置 `purge_delay = -1`(不把空闲内存还给内核,避免运行期 `madvise` 触发 TLB shootdown)。

```rust
pub struct PrepEnv { pub writer_epoch: u32, pub build_git: [u8; 20], pub host_profile_sha256: [u8; 32], pub to_writer: Producer<PreparedSegment> }
pub struct SegmentPreparer { /* 每 journal 一个 */ }
impl SegmentPreparer {
    pub fn new(cfg: &JournalConfig, env: PrepEnv) -> SegmentPreparer;
    pub fn poll(&mut self) -> PrepState;   // persist 线程:确保下一段存在、已缺页清零、段头 [P] 字段已填;须在写者 open(恢复)之后开始调用
}
#[repr(u8)] pub enum PrepState { Ready = 1 /* 已有一个准备好的下一段,这次 poll 不动 */, Prepared = 2 /* 这次 poll 准备了一段 */, Failed = 3 /* 这次 poll 失败(空间不足、文件已存在、映射失败),不留半成品文件,下次 poll 重试 */ }

pub struct PersistConfig { pub chunk_bytes: u32 /* 4 MiB */, pub sync_bytes: u32 /* 1 MiB */, pub sync_interval_ms: u32 /* 20 */ }   // Default 即括号内的值
pub struct Persister { /* 管理若干 journal */ }
pub struct PersistProgress { pub bytes_copied: u64, pub syncs: u32, pub sealed: u32, pub errors: u32 /* 这次 poll 失败的复制/同步/封存(如 ENOSPC),下次 poll 重试 */ }
pub struct JournalLag { pub name: JournalName, pub bytes: u64, pub ms: u64 }
impl Persister {
    pub fn new(cfg: PersistConfig) -> Persister;
    pub fn add(&mut self, cfg: &JournalConfig);      // 装配时登记每个 journal
    pub fn poll(&mut self, mono_ns: u64, utc_ns: i64) -> PersistProgress;  // 每 journal 最多复制 chunk_bytes;满足阈值即 fdatasync(间隔按 mono_ns);封存段写 .seal(sealed_utc_ns = utc_ns)
    pub fn lag(&self) -> Vec<JournalLag>;            // 每 journal 一项(登记顺序):未同步字节与估计毫秒;watchtower 指标 journal_persist_lag_*(warm,允许分配)
}
pub struct HotRetirer { /* 每 journal 一个 */ }
pub struct HugeAudit { pub pages_2m: u64, pub pages_1g: u64 }
impl HotRetirer {
    pub fn new(cfg: &JournalConfig, from_writer: Consumer<RetiredSegment>) -> HotRetirer; // cfg 给出热/冷目录与 hot_keep;当前活动段号 = 收到的最新旧段号 + 1
    pub fn poll(&mut self, now_utc_ns: i64) -> usize;        // persist 线程:置 retired,宽限后 munmap + unlink;返回本次删除的段数
    pub fn audit(&self) -> HugeAudit;                          // 已 unlink 仍映射的页数(启动自检与每分钟):本机各页大小的已用大页 − hugetlbfs 挂载上文件占用的页
}
pub struct RetiredSegment { pub segment_no: u64, /* 映射句柄:base()、len() */ }
pub struct RetireReport { pub segments: u32, pub bytes: u64 }
pub struct ColdRetention { pub keep_days: u16, pub require_uploaded: bool }
impl ColdRetention { pub fn for_kind(k: JournalKind) -> ColdRetention; }   // 缺省:Seq/Chain 7 天、Raw 3 天,require_uploaded = true
pub fn cold_retire(root: &Path, r: &ColdRetention, now_utc_ns: i64) -> Result<RetireReport, io::Error>; // journal-archiver 调用;root = <cold_root>/<instance>/<journal>
// 删除条件:已封存(有 .seal)且 now_utc_ns − SealRecord.sealed_utc_ns > keep_days × 86 400 s,require_uploaded 时另需 .uploaded;
// .jrn、.seal、.uploaded 一起删除,日期目录空了随之删除;未封存的冷副本从不删除
```

**冷副本目录**:本 crate 在 `<cold_root>/<instance>/` 下建立的每一级子目录(`<journal>/`、`<yyyymmdd>/`、`meta/`)以 `mkdir(…, 0770)` 创建,最终模式恰为 `0770`,父目录带 setgid 时为 `2770`(属组与 setgid 位经父目录继承;umask 使组位不足 `rwx` 时创建后 `chmod`,保留继承的 setgid 位);父目录带默认 ACL 时,子目录的 ACL mask 因此为 `rwx`,deploy 给 `hft-archive` 的具名用户条目(写 `.uploaded`、删除已提交的段)不被收窄,`hft-journal-ro` 组的有效权限仍由默认 ACL 的组条目决定(路径、属主与 ACL 归 [deploy 路径表](../../deploy/README.md#身份路径与-s3-前缀))。

### 恢复与校验

```rust
pub struct RecoveryReport {
    pub last_seq: Seq, pub repaired_segments: u32, pub bytes_copied: u64, pub bytes_discarded: u64,
    pub gap_possible: bool,                 // 掉电:冷副本尾部可能丢失
    pub rebuild_from: Position,             // 最近一个引导块或 Keyframe 快照,sequencer 从这里重放重建状态
    pub boot_reason: RollReason,            // 首次 roll 的原因:空 journal 或最后一段由 seal_and_close 封存 → Startup;其余(恢复第 2、4、5 步动过数据或补了 End)→ Recovery
}
pub fn verify(dir: &Path, opts: &VerifyOpts) -> VerifyReport;   // cargo xtask journal-verify;规则见 journal-format 校验与缺段检测;dir = 冷目录 <cold_root>/<instance>/<journal>(日期子目录)或热段目录
pub struct VerifyOpts { pub production: bool /* 拒绝 0x7Fxx,即使本二进制认识 */ }
pub struct VerifyReport { pub issues: Vec<VerifyIssue>, pub missing: Vec<(u64, u64)> /* 缺失的 segment_no 区间 */, pub recovery_gaps: Vec<u64>, pub unsealed: Vec<u64> }
pub struct VerifyIssue { pub segment_no: u64, pub offset: u64, pub item: u8 /* journal-format 校验与缺段检测的第 1–5 项 */ }
```

### 持久元数据(engine_epoch 等)

```rust
pub struct DurableMeta { /* <cold_root>/<instance>/meta/ 目录 */ }
impl DurableMeta {
    pub fn open(cold_root: &Path, instance: &InstanceName) -> Result<DurableMeta, io::Error>;
    pub fn read(&self, key: &str) -> Result<Option<u64>, io::Error>;
    pub fn write_sync(&self, key: &str, v: u64) -> Result<(), io::Error>; // 写临时文件 → fsync → rename → fsync 目录;返回即已落盘
}
```

冷路径接口(启动期调用,允许阻塞)。键名 `[a-z0-9_]{1,64}`(否则 `ErrorKind::InvalidInput`);每个键一个文件 `meta/<key>`,内容两行:十进制 `u64`(无前导零,0 写作 `0`)与 `crc32c:` + 第一行字节的 CRC32C(8 位小写十六进制);临时文件 `meta/<key>.tmp`,读取忽略它;不存在 → `Ok(None)`。本 crate 只提供按实例分目录的键值存储,键的语义归各自所有者:毒丸的两层计数(hft-core `boot_guard` 的恢复期崩溃计数、sequencer 按 InputHash 的运行期崩溃计数)也存在这里。hft-core 在**发出第一笔订单之前**以 `write_sync("engine_epoch", e)` 持久化 engine_epoch(`e` 的取值规则归 [hft-core 启动顺序](../../apps/hft-core/README.md#启动顺序)步 5);因此即使 RAM 热段与冷副本尾部都丢失,也不会复用 epoch(进而不会复用 client order id)。该目录的异机备份与换 AZ 恢复归 [journal-archiver](../../apps/journal-archiver/README.md)。值是 `u64` 文本 + CRC 行,读到损坏文件返回错误(拒绝启动,不猜)。不属于 `writer` feature(默认可用):journal-archiver 的 `restore` 用 `write_sync` 写回取回的键。

### cargo feature

| feature | 默认 | 提供 | 谁启用 |
|---|---|---|---|
| (无) | 是 | `JournalReader`、`verify`、`cold_retire`、`DurableMeta`、布局类型 | 冷进程(journal-archiver、watchtower、reconciler、replayer)、hft-py、advisor |
| `writer` | 否 | `JournalWriter`、`BootWriter`、`SegmentPreparer`、`Persister`、`HotRetirer`、恢复 | 只有 apps/hft-core、apps/replayer 与 optional 的 apps/chain-core(这些 app 的依赖闭包之外启用即 `cargo xtask deps` 失败) |
| `mem-backend` | 否 | 堆内存段后端与测试钩子模块 `testing`(见实现说明"段内存后端") | 只有本 crate 的测试、基准与 fuzz 目标(`required-features`) |

除 replayer 外的冷进程的 `Cargo.toml` 不得启用 `writer`(`cargo xtask deps` 检查);replayer 只用 `JournalWriter::open_offline`(`--emit-journal`):写 `--out` 指定的**普通文件系统目录**(`fstatfs` 为 hugetlbfs/tmpfs、或路径位于任何生产 journal 根之下——`cfg.hot_dir`、`cfg.cold_root`、`/dev/hugepages`、`/dev/hugepages2M`、`/dev/shm/hft` → `OpenErr::OfflineTarget`),段文件普通 `pwrite` + 关闭时 `fsync`,不预创建、不 persist、不退役。读者映射一律 `PROT_READ`,因此冷进程在类型与权限两层都不能写热段。kill-switch 不依赖本 crate,按 [journal-format](../../docs/subsystems/journal-format.md) 的已发布布局只读段头。

**测试写者** `examples/testwriter.rs`(`required-features = ["writer"]`,不进任何发布制品,只在测试中以子进程运行):供只用读者的组件(hft-py、treasury 等)生成 journal 夹具。在 `--hot <dir>` 与 `--cold <dir>` 下以 `--instance prod|sim` 写一个 `seq` journal,每段引导块为 `Begin`、`--refdata <blob>` 给出时的 `ConfigBlob{boot = 1, kind = Refdata}`、`BootDone`;源二选一:`--from <journal 目录>`(逐事件复制其引导块之后的非 Meta 记录,seq 由写者重新分配)或 `--synth-bbo <n> --inst <id> --bid <raw> --ask <raw> --qty <raw> --t0-ns <utc> --step-ns <ns>`(第 i 条 `Bbo` 的时刻 t = t0 + i × step,`ex_ts_ns = t − 2 ms`,`hw_rx_ns` 在 i 为偶数时 = t − 1 ms、奇数时 = 0);每个源事件写成一个事件(`end_event`),每 `--publish-every`(默认 1)个事件 `publish` 一次;`--segment-bytes`(默认 8 MiB)、`--sync-interval-ms`(默认 50)按名配置;结束时 persist 刷到 `committed_len`,给出 `--seal` 时 `seal_and_close` 并写 `.seal`,否则最后一段保持 `Open`(模拟运行中的实例)。另有脚本模式 `--script <文件|->`(源不再是 `--from`/`--synth-bbo`):逐行读命令——`boot <kind> <hex>`/`boot-clear`(此后每个引导块写入的记录)、`rec <kind> <flags> <tsc> <hex>`、`bulk <文件>`(二进制批量:记录、`end`、`publish`、`roll`、`ClockStatus`)、`synth <n> <inst> <bid> <ask> <qty> <t0_ns> <step_ns> [<每秒条数>]`、`end`、`publish`、`roll`、`persist on|off`、`flush`、`sleep <ms>`、`mark <标记>`(在 stdout 打印标记与 `CLOCK_MONOTONIC` ns)、`seal`、`abort`(模拟崩溃)、`exit`;`--tsc-hz-milli`/`--calib-utc-ns` 给段头校准对,滚动发生在最后一条记录的 TSC 处(段头校准对、引导块记录用它),`--clock-every-ns` 让 `synth` 按整周期插入 `ClockStatus`;persist 每 256 次 `publish` 推进一轮(`flush` 与滚动立即推进)。退出码:0 成功,1 参数或源错误,2 写者错误(`OpenErr`/`AppendErr`/`RollErr`)。

### raw journal 模式

`raw-feed-<venue>`(及拆分后的 `raw-feed-<venue>-bbo`)写哪些帧(`off` 只写非数据帧、`sampled` 采样写数据帧、`on` 全写)由 feed 运行时决定:配置键与含义归 [hft-venue 配置项](../hft-venue/README.md)(`feed.raw.bbo`/`feed.raw.md`),取值按主机类与实例归 [hft-core 内存表](../../apps/hft-core/README.md#内存与-journal-段按主机类唯一归属)。本 crate 只提供写者,不判断写不写。`raw-gw-*` 始终全写(取证所需,量小)。非全写模式下解码器金样与重跑解码只能覆盖被记录的帧;模式写进 `HostProfile` 便于读者识别。

### 小段 + 4K 页模式(tmpfs 热段)

`dev2` 的两个实例与统一升级主机上的 `sim` 实例不用大页(按 [hft-core 内存表](../../apps/hft-core/README.md#内存与-journal-段按主机类唯一归属)):热段放 tmpfs(`/dev/shm/hft/<instance>/<journal>/` 具名文件),`page = Normal`,段小。写者、读者、提交游标、滚动、引导块、persist、恢复与段格式**完全相同**,只换配置:

| 项 | tmpfs 热段的约束 | 说明 |
|---|---|---|
| `hot_dir` | `/dev/shm/hft` | `fstatfs` 必须是 `TMPFS_MAGIC`;启动检查 `/dev/shm` 可用空间 ≥ 全部热段需求 − 本实例已有段的占用(hft-rt `SHM_SHORT`,见下节第 5 条) |
| `segment_bytes`、`hot_keep` | 取值由 [hft-core](../../apps/hft-core/README.md#内存与-journal-段按主机类唯一归属) 按主机类与实例给出 | 受该实例 tmpfs 挂载大小约束,并计入 hft-core 的内存上限(见下节公式) |
| 预创建 | `ftruncate` + `mmap(MAP_SHARED\|MAP_POPULATE)` + `mlock` | 预触摸后锁定,运行期零缺页;tmpfs 页在内存里,没有回写,因此不触发 R2 的写保护问题(R2 针对磁盘文件 mmap 的回写),代价只是 4 KiB 页的 TLB 缺失多于大页 |
| 滚动 | 整点或两级安全阀(调用约束第 5 条);段小,滚动更频繁 | 引导块每段一份,验收"每个段都有完整引导块"不变 |

小段模式不改变任何语义,只影响:TLB 开销(`journal_append_*` 基准在 tmpfs 热段上只记录、不判门槛)、滚动频率(引导块开销按段摊销)、掉电后丢失窗口(同 persist 滞后)。

### 段大小与热段内存

段大小与 `hot_keep` 的**取值**唯一归属 [apps/hft-core](../../apps/hft-core/README.md#内存与-journal-段按主机类唯一归属)(按主机类);本 crate 只规定公式与约束,hft-core 启动时按它校验,不满足即拒绝启动:

1. 每个 journal 的热段常驻 = (1 活动 + 1 预创建 + `hot_keep`)× `segment_bytes`;全部启用 journal 之和记为 `H`。
2. hugetlbfs 主机类:按页大小分别求和(各 journal 用哪种页归 hft-core 内存表),各自 ≤ 该主机类预留的大页数 × 页大小;`H` + 环、簿、槽与堆必须小于物理内存,并给冷进程与页缓存留余量——段大小 × 保留段数不能超过物理内存。
3. tmpfs 热段:`H` ≤ 该实例 tmpfs 上限,并计入 hft-core 的内存上限。
4. 下限按 [hft-core 段大小规则](../../apps/hft-core/README.md#内存与-journal-段按主机类唯一归属)(≥ 16 × 引导块大小,且容得下滚动预留区 2 × 引导块,见调用约束第 5 条);最忙小时量超过段大小时由安全阀提前滚动、不停写,不要求一个段装下一个整点周期。各类 journal 的流量估算方法见 [journal-format 三类 journal](../../docs/subsystems/journal-format.md)。
5. **启动空间检查扣除本实例已有的段**:崩溃或 Drain 后,本实例 `hot_dir/<instance>/` 下的段文件仍占着 tmpfs 或大页,它们会被恢复复用、persist 补齐后退役,不是新的需求。hft-core 启动时以 `held_by_instance` 统计本实例已有段的占用,检查条件是"可用 ≥ 需求 − 已占用"(hft-rt `HostNeeds.held`);只扣本实例目录,同机另一实例(`prod`/`sim`)的段不扣除。否则崩溃后的第一次重启会因空间不足起不来。

```rust
pub fn held_by_instance(hot_dirs: &[&Path], instance: InstanceName) -> Result<hft_rt::HeldBySelf, io::Error>; // 冷路径;只统计 <hot_dir>/<instance>/<journal>/ 下文件名为 <10 位数字>.jrn 且前 8 字节为 HFTJRNL1 的段文件(含 Preparing),按所在挂载的页大小分别计(文件大小向上取整到页)
```

### 配置项

| 名称 | 类型 | 默认 | 来源 |
|---|---|---|---|
| `journal.<name>.segment_bytes` | u64 | 无默认;按主机类由 hft-core 给出 | apps 配置(取值归 [hft-core](../../apps/hft-core/README.md#内存与-journal-段按主机类唯一归属)) |
| `journal.<name>.hot_keep` | u8 | 无默认;按主机类由 hft-core 给出 | 同上 |
| `journal.page` | `huge` \| `normal` | 无默认;按 hft-core 内存表:tmpfs 热段(`dev2` 的两个实例、升级主机的 `sim` 实例)为 `normal`,其余为 `huge`;与表不符即退出 2 | 同上 |
| `journal.size_idle_pct` | u8 | 75(须 `0 < size_idle_pct < size_valve_pct < 100`,否则 `OpenErr::BadConfig`,hft-core 退出 2) | 同上 |
| `journal.size_valve_pct` | u8 | 90 | 同上 |
| `journal.persist.chunk_bytes` / `sync_bytes` / `sync_interval_ms` | u32 | 4 MiB / 1 MiB / 20 ms | 同上;依据 [research/06 §2.6](../../design/research/06-data-plane-replay-observability.md) |
| `journal.cold.keep_days` | u16 | seq/chain 7,raw 3 | [08 §3](../../design/08-数据面与回放.md) |
| `journal.reader.reassembly_cap` | usize | 256 MiB | 读者进程 |
| `journal.reader.tail_poll_us` | u32 | 50 | 同进程 warm 读者读到 `Pending` 后重读 `committed_len` 的最小间隔 |

## 实现说明

<details><summary>实现要点</summary>

**写者内部状态**(全部启动期分配):当前段映射 `base: *mut u8`、`cap`、`off`(下一条记录偏移)、`next_seq`、`hdr: &SegmentHeader`、下一段句柄(由 `SegmentPreparer` 经一个 `Spsc<PreparedSegment>` 交给写者;写者只取,不打开文件)。

**`append` 步骤**:

1. `len = record_len(size_of::<B>(), crc)`;`off + len > cap` → `Full`(安全阀应早已触发)。
2. 拷贝体到 `base + off + 24`;写 `kind/flags/seq/tsc`;若 CRC,计算并写入末 4 字节(此时 `len` 字段按最终值参与计算,写入顺序见下);
3. `atomic::fence(Release)` 语义下写 `len`(以 `AtomicU32::store(Release)` 写记录头首字);`off += len`;`next_seq += 1`;返回 seq。
4. 不写 `committed_len`——由 `publish()` 批量写。

**`append_var`**:按 [多段规则](../../docs/subsystems/journal-format.md#变长与多段记录) 计算段数;先检查 `TooLarge` 条件,再检查预留区之外的剩余空间容纳整组(不足 → `Full`,不写任何字节;安全阀保证正常流量下在此之前已滚动;`BootWriter` 内可动用预留区);逐段写 `PartHdr` + chunk(`pieces` 为分散输入,避免调用方先拼接)。

**`roll` 步骤**(写者线程,两个事件之间):

1. 从预创建通道取下一段;没有 → `NextNotReady`。
2. 当前段追加 `SegmentRoll{End}`;`publish()`;Release 写 `state = Sealed`。
3. 填新段头 [W] 字段(`first_seq`、`prev_last_seq`、时间与校准对、`roll_reason`、`flags`、`n_config` 与 `config_digest`),计算 `header_crc32c`,Release 写 `state = Open`。
   把旧段句柄推入回收环(见旧段回收)。
4. 交换当前段指针;追加 `SegmentRoll{Begin}`;返回 `BootWriter`。
5. raw journal 调用方随即 `finish()`(不写 `BootDone`,只保留 `Begin`);`seq`/`chain` 调用方写完引导块后 `finish()` 写 `BootDone`。

**预创建**(`SegmentPreparer::poll`,warm):先 `fstatfs` 热目录,魔数必须是 `HUGETLBFS_MAGIC` 或 `TMPFS_MAGIC`,否则 `OpenErr::DiskBacked`(`JournalWriter::open` 同样检查);在 hugetlbfs 下 `open(O_CREAT|O_EXCL)` + `ftruncate(segment_bytes)` + `mmap(MAP_SHARED|MAP_POPULATE)`(内核在此缺页并清零,耗时落在 persist 线程);tmpfs 小段模式另加 `mlock(段)`→ 填段头 [P] 字段(含 `prev_header_sha256` = 当前活动段头 0..3072 的 SHA-256,此时它已不可变)→ `state = Preparing` → 经通道交给写者。每 journal 始终保持 1 个已准备好的下一段。

**persist**(`Persister::poll`,warm;写冷副本出错时关闭该文件句柄,下一轮重新打开;冷副本已不存在则重建并从字节 0 重新复制):对每个 journal,读热段 `committed_len`(Acquire),把 `[persisted_len, committed_len)` 中最多 `chunk_bytes` 以 `pwrite` 写到冷副本同偏移(冷副本在该段第一次复制时创建:`open` + `fallocate(segment_bytes)`,文件名含 `first_seq`);累计未同步 ≥ `sync_bytes` 或距上次同步 ≥ `sync_interval_ms` → `fdatasync` → 更新热段 `synced_len` 并把它 `pwrite` 进冷副本头部 persist 行。热段 `Sealed` 且 `synced_len == committed_len` → 冷副本 `ftruncate(committed_len)`、`fsync`、目录 `fsync`、写 `.seal`(复制过程中以增量 SHA-256 累积,3072..3200 以零参与)。

**恢复**(`JournalWriter::open`):

1. 枚举热目录中本 journal 的段(按 `segment_no`);校验段头 CRC,`Preparing` 状态的段直接删除(由 `SegmentPreparer` 重新准备)。段号不连续、段头链不连续(且无 `RECOVERY_GAP`)→ `OpenErr::ChainBroken`;同一段的热段与冷副本在 `[0, synced_len)` 上(除 3072..3200)不同,或热段 `synced_len > 0` 而冷副本缺失或短于 `synced_len`(冷副本根与热段不属于同一次运行,如测试网实例遗留的热段)→ `OpenErr::ColdHotConflict`(hft-core 以退出码 4 退出);`synced_len = 0` 且无冷副本是正常情形(persist 尚未复制)。**`seq`/`chain` 段若引导块未写完(没有已提交的 `BootDone`),按 `Preparing` 处理:热段与已复制的冷副本(及其 `.seal`/同步元数据)整段删除**,恢复从上一段(已封存,末条为 `SegmentRoll{End}`)继续,新段沿用同一 `segment_no` 重新打开。读者在一个段的 `BootDone` 提交前不交付该段任何记录,所以整段删除对读者不可见。
2. 对最后一个 `Open` 段:从 `committed_len` 起扫描完整记录(`len != 0`、`len` 合法、kind 已注册、`seq` 连续、CRC 若有则正确),再**截断到扫描区间内最后一条 `EVENT_END` 记录之后**,区间内没有则截断到 `committed_len`(已发布的部分总在事件边界上,截断点不低于它;半个事件——输入已记、输出未写完——整体丢弃;未完成的多段组随之丢弃);得到 `end`,将 `[end, 下一个全零页)` 清零,`committed_len = end`。这个 `end` 就是"恢复截断点"(`last_complete_event()` 与 `EndAt::LastCompleteEvent` 用同一算法)。
3. 对所有 `synced_len < committed_len` 的段:补复制并同步(**先补复制,再做任何新写入**;冷副本不存在时从字节 0 复制)。
4. 最后一个 `Open` 段:追加 `SegmentRoll{End, reason: Recovery}` 并封存;`boot_reason = Recovery`,调用方的首次 `roll` 以它打开新段(`roll_reason = Recovery`)。
5. 热目录为空(整机重启、RAM 丢失,或干净停机后热段已被释放——dev2 的 `hft-shm-release`,[apps/hft-core](../../apps/hft-core/README.md)):取冷目录最后一段;有 `.seal` → 冷副本完整,新段不置 `RECOVERY_GAP`、`gap_possible = false`(是否干净停机由 sequencer 按末尾是否 Drain 快照判定,不判 `tail_lost`);若无 `.seal`,从 `synced_len` 起按第 2 步的规则扫描并截断(截断点不低于 `synced_len`)、追加 `End{Recovery}`、封存;新段置 `RECOVERY_GAP`;`gap_possible = true`。
6. 定位 `rebuild_from`:最后一个完整引导块或 `Keyframe` 快照的位置。

状态重建不在本 crate:hft-sequencer 的 `RecoverDriver` 从 `rebuild_from` 起**照常应用输入**,重新生成的输出与日志不一致只告警、不 Fatal,新输出写入以 `Recovery` 打开的新段;**二进制版本与日志不同时从最近快照起步、不重放尾部**;随后与交易所真值对账,通过后才放行交易。逐字节的验证回放只作为离线夜跑与 CI 工具([hft-sequencer](../hft-sequencer/README.md))。

**读者**:`poll` 取当前段 `committed_len`(热段)或文件长度/`synced_len`(冷副本)为上界,逐条 `decode_record`;多段记录进入 hft-event 的 `Reassembler`(读者自有缓冲,上限 `reassembly_cap`);读到 `SegmentRoll{End}` 后按 `other_segment_no` 打开下一段(热目录优先,找不到则在冷目录按 `segment_no` 前缀查找);段头链与 `first_seq` 不连续 → `Gap`。

**三类 journal 的差异**:见 [journal-format 三类 journal](../../docs/subsystems/journal-format.md#三类-journal);同一 `JournalWriter` 实现,差异只在配置(页大小、CRC 策略、有无引导块、整点滚动由谁触发)。

**段内存后端**:写者、读者、滚动与恢复扫描的核心逻辑对段内存泛型(基址、长度、Release/Acquire 原子字段);生产后端是 hugetlbfs/tmpfs 具名文件的共享映射,单元测试另有堆内存后端(feature `mem-backend`,任意平台可跑:`testing::mem_root(dir)` 把该目录及其下的热段放进进程内的堆"文件",冷副本仍是普通文件;`testing` 另提供每线程操作日志与段文件字节读写,供测试观察预创建/复制/同步/退役与映射调用)。`fstatfs`、`mmap`/`munmap`/`unlink`、`mlock`、跨进程映射与崩溃存活只由 Linux 上的真实文件后端覆盖;非 Linux 平台上真实文件后端返回 `Unsupported`(`open` 表现为 `DiskBacked`/`Io`)。

**规划源文件**

| 文件 | 职责 |
|---|---|
| `src/writer.rs` | `JournalWriter`、`append*`、`publish`、`needs_roll` |
| `src/roll.rs` | `roll`、`BootWriter`、段头 [W] 填写与 CRC |
| `src/prepare.rs` | `SegmentPreparer` |
| `src/persist.rs` | `Persister`、`.seal`、增量 SHA-256 |
| `src/retire.rs` | `HotRetirer`、`cold_retire` |
| `src/recover.rs` | 恢复步骤 1–6、`RecoveryReport` |
| `src/meta.rs` | `DurableMeta` |
| `src/reader.rs` | `JournalReader`、热/冷切换、重组 |
| `src/verify.rs` | 离线校验(xtask 与 journal-archiver 共用) |
| `src/layout.rs` | `SegmentHeader`(含原子字段)、`SegmentHeaderFixed`(zerocopy 视图)、`SealRecord`;段内存句柄(写者行/persist 行原子访问、`mmap`/`munmap`、堆后端)与 `statfs`/`fallocate`——本 crate 唯一含 `unsafe` 的文件 |
| `src/fs.rs` | 热段与冷副本的文件操作、路径与命名、`0770` 目录、进程内映射表 |
| `src/scan.rs` | 记录合法性检查、尾部扫描、段头哈希、`.seal` 哈希(恢复、读者、校验共用) |
| `src/config.rs` | 名字、配置、码表与错误类型 |
| `src/held.rs` | `held_by_instance` |
| `src/testing.rs`、`src/oplog.rs` | `mem-backend` 的测试钩子与操作日志 |

</details>

## 延迟与资源契约

| 入口 | 预算(统一升级主机类与 `metal`,热段已预触摸;`dev2` 与 tmpfs 热段只记录不判门槛) | 基准 |
|---|---|---|
| `append`(≤ 128 B 定长体,无 CRC) | p50 ≤ 40 ns,p99 ≤ 100 ns | `journal_append_80b` |
| `append_var` / `reserve_var`(512 B raw,含 CRC) | p50 ≤ 100 ns,p99 ≤ 250 ns([08 §2](../../design/08-数据面与回放.md) 给 50–200 ns/条) | `journal_append_raw_512b` |
| `publish` | 无读者争用 ≤ 5 ns;有读者按尾随粒度轮询时 p99 ≤ 150 ns(每读者至多每 `tail_poll_us` 一次 RFO) | `journal_publish` |
| `roll`(切换本身,不含引导块内容) | p99 ≤ 5 µs | `journal_roll_switch` |
| 引导块写入(400 簿 × 200 档 + 快照 ≈ 1.5 MB,sequencer 的预算项) | ≤ 1 ms,每次滚动一次(滚动频率待实测) | `journal_boot_400books` |
| `end_event` | ≤ 3 ns | `journal_end_event` |
| persist 滞后(`committed_len − synced_len` 的时间) | io2:p99 ≤ 50 ms;gp3:p99 ≤ 250 ms(预创建期间的暂停计入);只影响掉电丢失窗口 | 指标 `journal_persist_lag_ms` |
| 热段退役 `munmap`(persist 线程) | 不在热线程上执行;对热核的影响计入 p99.9 记录项,待 M3 末满订阅探针实测 | `journal_retire_tlb`(热线程环跳分布在退役前后的对比) |
| 读者顺序扫描 | ≥ 1 GB/s(页缓存命中) | `journal_read_scan`;回放目标 10 GB < 10 min([08 §5](../../design/08-数据面与回放.md)) |
| 恢复(1 GiB 未复制) | ≤ 5 s;冷启动总目标 < 30 s([08 §6](../../design/08-数据面与回放.md)) | `journal_recover_1g` |

热路径入口零分配、零系统调用、零缺页;JNL-105 的缺页计数只取执行 append 的线程。写者与读者共享的只有写者行(批次边界写一次)与 persist 行(warm 写)。热段内存:见 [journal-format 三类 journal](../../docs/subsystems/journal-format.md#三类-journal) 的预算。

## 测试与验收

用例与验收见 [TESTS.md](TESTS.md)。

进程崩溃用例的随机写者在事件开始前按未发布写游标检查整事件容量,父进程必须确认到达预定杀点且退出信号为 `SIGKILL`;提前 EOF、panic 或普通退出均失败。段容量、种子数与恢复断言归 [JNL-072](TESTS.md#jnl-072-任意时刻被杀后保留的恰是全部完整事件)。

## 依赖

- 内部:[hft-types](../hft-types/README.md)、[hft-event](../hft-event/README.md)(记录编解码、多段、kind 注册表)、[hft-rt](../hft-rt/README.md)(`HugeRegion`、`Spsc`、`CalibPair`)。
- 外部:`libc`、`zerocopy`、`sha2`(不启用 `asm`,理由同 [hft-refdata](../hft-refdata/README.md);同时链接启用 `asm` 的进程经特性合并得到 asm 版,摘要相同)、`crc32c`(与 hft-event 同一 crate,本 crate 直接依赖以计算记录 CRC);dev:`criterion`、`proptest`、`xxhash-rust`、hft-event 的 `dev-kinds`。
- 禁止:`tokio`、任何磁盘文件系统上的文件型 `mmap` 作为**写者**的写入目标(热写者只写 hugetlbfs 或 tmpfs 上的**命名文件**,不用匿名 `memfd_create`——它随进程释放,崩溃不丢不成立;[13 R2](../../design/13-设计评审与优化.md)、[hot-path-rules M3](../../docs/hot-path-rules.md))、`O_DIRECT`、写者线程中的 `fsync`/`fdatasync`/`msync`。

## 延伸阅读

- [journal-format](../../docs/subsystems/journal-format.md)(格式唯一定义)、[event-model](../../docs/subsystems/event-model.md)
- [08 §1–3、§6](../../design/08-数据面与回放.md)、[13 R2/R6](../../design/13-设计评审与优化.md)、[04 §7 raw journal](../../design/04-行情接入层.md)
- [research/06 §1.2、§2.5、§2.6](../../design/research/06-data-plane-replay-observability.md)(一热写多冷读、snapshot-at-roll、fsync 策略)
- 现有教训来源:`E:\trading\Trading\crates\wal`(500 MiB 滚动无快照 → 84% 段隔离;`recovery.rs` 的尾部截断思路)
- 决策记录 `hot-journal-in-ram`、`self-contained-journal`([Agent Notes](../../.agents/notes/README.md))

## 已知限制与后续工作

- 整机掉电丢失 `synced_len` 之后的尾部(典型 ≤ 数十 ms);订单与仓位由交易所真值 + [reconciler](../../apps/reconciler/README.md) 收敛,行情由重连快照恢复。可选扩展:跨机复制(Aeron Archive 式;触发条件:掉电丢失窗口不可接受或多机部署)。
- raw journal 段不自足,重跑解码器依赖同一小时的 `seq` 引导块;若将来需要独立的 raw 自举,在 raw 段首写入该 venue 的 REST 快照(代价是每小时每 inst 一次 REST 权重)。
- 读者按 `segment_no` 在冷目录查找需要列目录;可选扩展:由 journal-archiver 维护一个段索引文件(触发条件:冷读者数量多、列目录成为瓶颈)。
- 引导块在每次滚动时一次性导出全部簿(≈ 1 ms);若簿数量增长到使其超过 2 ms,改为引导块只含非簿状态、簿关键帧在其后若干轮内补齐,并在 `SnapshotHdr.n_books` 中声明。

### 开发备忘

<details><summary>给实现者</summary>

- **待核实**:目标实例上 hugetlbfs 1 GiB 页的 `ftruncate` + `MAP_POPULATE` 耗时(预期 50–100 ms,落在 persist 线程,期间复制暂停);io2 与 gp3 上 `fdatasync` 的实测分布(方法:`journal_persist` 基准在目标卷上跑 24 h,结果写进 deploy 的调优记录)。
- 实现顺序:`layout.rs` → `writer.rs`(先在普通内存上跑基准)→ `reader.rs` → `prepare.rs` → `roll.rs` → `persist.rs` → `recover.rs`(崩溃测试先写)→ `retire.rs` → `verify.rs`。
- 现有 `Trading/crates/wal/src/recovery.rs` 的"扫描到最后一条完整记录再截断 + 追加 gap 记录"可作结构参考,但格式不兼容,不复用代码。
- 开放问题:`chain` journal 的写者线程是否也需要 `publish` 先于外发(bundle 投递)——规则应与 sequencer 相同,待 [hft-chain-evm](../hft-chain-evm/README.md) 规格确认。
- 开放问题:关键帧与引导块的大块追加(≤ 2 MiB 顺序写)是否冲刷热核的 L2、之后若干事件的 `decide` 是否变慢;若是,大块拷贝改用非临时存储(aarch64 `STNP`)。待实测(方法:M3 末满订阅探针比较关键帧后 1 ms 内与平时的 `decide` p99)。
- 开放问题:若 M3 末满订阅探针实测进程内 `munmap` 对热核的影响超出 p99 预算,是否改为退役段长期映射、只 `unlink` 不 `munmap`(页在进程退出前不归还,大页预留与 hft-core 内存表要按进程寿命内的退役段数重算);热段文件仍只新建与删除、永不原地复用。

</details>
