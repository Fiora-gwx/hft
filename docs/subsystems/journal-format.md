# journal 格式

本页是 journal **文件格式**的唯一定义:目录与命名、段文件布局、`SegmentHeader`、提交游标、记录对齐与 CRC、变长与多段记录、段滚动与引导块、快照记录结构、冷副本、校验与缺段检测。记录头与事件体在 [event-model.md](event-model.md);写者/读者/persist/恢复的实现与 API 在 [hft-journal](../../crates/hft-journal/README.md);Rust 类型在 [hft-event](../../crates/hft-event/README.md)。设计依据见 [08 §2–6](../../design/08-数据面与回放.md)、[13 R2/R6](../../design/13-设计评审与优化.md)、[research/06 §1.2、§2.5–2.6](../../design/research/06-data-plane-replay-observability.md)。

## 三类 journal

| journal 名 | scope | 写者(唯一) | 记录头 `seq` | 记录头 `tsc` | 引导块 | CRC |
|---|---|---|---|---|---|---|
| `seq` | core | sequencer | sequencer 全局序号 | `proc_tsc` | 有 | 按 kind([hft-event](../../crates/hft-event/README.md) `KindInfo.crc`) |
| `raw-feed-<venue>`;BBO 拆成独立任务时另有 `raw-feed-<venue>-bbo`([hft-venue §1](../../crates/hft-venue/README.md)) | core | 行情任务(各自 raw journal 的唯一写者;按 venue `off`/`sampled`/`on`,见 [hft-journal](../../crates/hft-journal/README.md)) | 本 journal 序号 | `rx_tsc` | 无(只有 `Begin`) | 全部记录 |
| `raw-gw-<venue>` | core | gw 线程 | 本 journal 序号 | `rx_tsc`/`sent_tsc` | 无 | 全部记录 |
| `raw-chain-<chain>-<source>` | optional(链上策略接入时启用) | chain-core 的 ingest 线程(每个 `<source>` 一个 journal) | 本 journal 序号 | `rx_tsc` | 无(只有 `Begin`) | 全部记录 |
| `chain-<chain>` | optional(同上) | chain-core 主循环 | chain-core 序号 | chain-core `proc_tsc` | 有 | 按 kind |

模拟账户没有单独的 journal 种类:sim-gw 产出的回报与实盘回报一样经回报环进入 `sim` 实例的 `seq` journal(Sim 账户只在 `sim` 实例内运行,`prod` 实例不含 Sim 账户,规则归 [apps/hft-core](../../apps/hft-core/README.md));sim-gw 不写 raw journal。

**段大小、热段保留份数与热段页(1 GiB 大页、2 MiB 大页或 tmpfs 4 KiB 页)按主机类与实例定义,唯一归属是 [apps/hft-core](../../apps/hft-core/README.md)**(连同各主机类的内存预算与 `dev2` 护栏);本页与 [hft-journal](../../crates/hft-journal/README.md) 只给公式与目录,不写数字。tmpfs 热段预触摸 + `mlock`;段格式与本页其余规则不随页的种类变化。chain journal 的存储方式随 chain-core 立项确定,格式与 `seq` 相同。

`<venue>` 是 [hft-types](../../crates/hft-types/README.md#id-与枚举) `VenueId` 常量名的小写短横线形式(`binance-usdm`)。optional 的 `raw-chain-<chain>-<source>` 与 `raw-feed-*` 同格式(`journal_kind = Raw`,段头 `venue` = 链的 `VenueId`,记录为 `RawRx`,`RawHdr.stream` 区分订阅)。raw journal 段**不要求自足**:从 raw 重跑解码器时,用同一 UTC 小时 `seq` 段的引导块作种子。

**段大小公式**(apps/hft-core 按它给出各主机类的取值;输入用 S3 上现有采集数据按目标订阅实算,[08 §3](../../design/08-数据面与回放.md) 的"seq 5–20 GB/天"低估了约一个数量级,不能作为依据):

1. 对每个目标订阅(venue × 频道 × inst)统计 24 h 的消息数分布(均值、p99 小时、最忙分钟)。
2. 每条消息折算成记录字节:`Bbo` 120 B(24 头 + 96 体)、`Trade` 112 B、`L2Batch` 24 + 80 + 16 × 档数(对齐到 8)、`Funding`/`MarkPrice` 88–104 B;回报、决策、`ExecDecision` 与 `Advice` 按预期速率另加。raw 记录 = 24 + 16(`PartHdr`)+ 24(`RawHdr`)+ 原文长度,对齐到 8。
3. 小时量 `H` = Σ 消息率 × 记录字节 × 3600,取最忙小时;`seg_ideal = ⌈1.5 × H⌉`(取整到页大小)。
4. 热段内存 `M = seg × (2 + hot_keep)`(1 活动 + 1 预创建 + 保留份数)。约束:`Σ_journal M ≤` 该主机类分给 journal 的内存上限(apps/hft-core);不满足时 `seg = ⌊上限 / Σ(2 + hot_keep)⌋` 取整到页,一小时内由大小安全阀(见下)多次滚动——段数变多,格式与正确性不变。

示例(说明量级):400 个 inst 的 bookTicker 平均 20 条/s → 8 000 条/s × 120 B ≈ 0.96 MB/s ≈ 3.5 GB/h。安全阀滚动保证段写满时不会停写。

## 目录与命名

```
热段    /dev/hugepages/hft/<instance>/<journal>/<segment_no:010>.jrn        用 1 GiB 页的 journal(hugetlbfs,pagesize=1G)
        /dev/hugepages2M/hft/<instance>/<journal>/<segment_no:010>.jrn     用 2 MiB 页的 journal(hugetlbfs,pagesize=2M)
        /dev/shm/hft/<instance>/<journal>/<segment_no:010>.jrn             用 tmpfs 的 journal(4 KiB 页)
冷副本  <cold_root>/<instance>/<journal>/<yyyymmdd>/<segment_no:010>-<first_seq:020>.jrn
封存    <cold_root>/<instance>/<journal>/<yyyymmdd>/<segment_no:010>-<first_seq:020>.seal
上传    同目录 .uploaded(journal-archiver 在 S3 清单确认后写)
```

- 两个 hugetlbfs 挂载点由 [deploy](../../deploy/README.md) 的调优清单创建:`/dev/hugepages`(`pagesize=1G`)与 `/dev/hugepages2M`(`pagesize=2M`)。热段一律是 hugetlbfs 或 tmpfs 上的**具名文件**:进程崩溃后文件仍在,恢复程序据此补复制;**不用匿名 `memfd_create`**(随进程释放)。写者与预创建者用 `fstatfs` 校验魔数(`HUGETLBFS_MAGIC`/`TMPFS_MAGIC`),否则拒绝。唯一例外是 replayer `--emit-journal` 的离线写者([hft-journal `open_offline`](../../crates/hft-journal/README.md#cargo-feature)):它只写普通文件系统目录,并拒绝 hugetlbfs/tmpfs 与生产 journal 根。
- `<instance>`:hft-core 实例名,只允许 `prod` | `sim`([hft-journal](../../crates/hft-journal/README.md) `InstanceName::{Prod, Sim}`;`sim` = 只含模拟账户、不加载交易所 key 的独立实例,见 [apps/hft-core](../../apps/hft-core/README.md));其他值在类型层面不可表达。两个实例各有自己的目录与 journal,永不写同一目录;测试、DST 与 replayer 用各自的临时根目录(实例名仍是这两者之一)。段头 `instance` 字段写同一名字的 ASCII。
- 热段按 `segment_no`(每 journal 单调,从 1 起)命名,因为下一段在 `first_seq` 确定之前就要预创建;冷副本在复制时 `first_seq` 已知,带进文件名便于按序号定位。
- 预创建临时名为同目录 `<segment_no:010>.jrn.preparing`,不属于读者或 persist 的段列表;正式名只在预创建完成后原子发布。发布与恢复清理规则归 [hft-journal 实现说明](../../crates/hft-journal/README.md#实现说明)。
- `<yyyymmdd>` 是该段 `created_utc_ns` 的 UTC 日期;整点滚动使一个 `seq` 段 ≈ 一个 Parquet 小时分区。

## 段文件布局

```
偏移 0        SegmentHeader(4096 B):不可变区 0..3072 | 写者行 3072..3136 | persist 行 3136..3200 | 保留至 4096
偏移 4096     记录 0 | 记录 1 | …          每条从 8 B 对齐偏移开始,首尾相接
committed_len 之后   全零(热段由内核在缺页时清零;冷副本在封存时截断到 committed_len)
```

## SegmentHeader

> 草案:本页全部字节布局(`SegmentHeader`、`PartHdr`、`SnapshotHdr`、`SectionHdr`、各节行格式与行版本、`SealRecord`)实现时以代码与编译期断言为准;码值与字段名全仓一致。

```rust
#[repr(C)] pub struct SegmentHeader {       // 4096 B
    // —— 不可变区:预创建时由 housekeeping 填 [P],滚动时由写者填 [W],之后不再改;crc 覆盖 0..3068 ——
    pub magic: [u8; 8],                     //    0 [P] *b"HFTJRNL1"
    pub format_version: u16,                //    8 [P] 1
    pub journal_kind: u8,                   //   10 [P] Raw=1 Seq=2 Chain=3
    pub venue: VenueId,                     //   11 [P] raw 专用,否则 0
    pub header_len: u32,                    //   12 [P] 4096
    pub journal_name: [u8; 32],             //   16 [P] ASCII 右补 0
    pub instance: [u8; 16],                 //   48 [P]
    pub segment_no: u64,                    //   64 [P]
    pub first_seq: u64,                     //   72 [W] 本段第一条记录的 seq
    pub prev_last_seq: u64,                 //   80 [W] 上一段最后一条记录的 seq;首段 0
    pub created_utc_ns: i64,                //   88 [W]
    pub created_tsc: u64,                   //   96 [W]
    pub segment_bytes: u64,                 //  104 [P] 文件容量
    pub writer_epoch: u32,                  //  112 [P] 写者进程启动计数
    pub roll_reason: u8,                    //  116 [W] 同 SegmentRoll.reason
    pub flags: u8,                          //  117 [W] RECOVERY_GAP=1(上一段尾部可能因掉电丢失)
    pub _pad0: u16,                         //  118
    pub tsc_hz_milli: u64,                  //  120 [W] TSC 频率 × 1000
    pub calib_tsc: u64,                     //  128 [W] 校准对:TSC
    pub calib_realtime_ns: i64,             //  136 [W] 校准对:CLOCK_REALTIME
    pub calib_phc_ns: i64,                  //  144 [W] 校准对:PHC(无则 0)
    pub calib_err_ns: u32,                  //  152 [W] 校准对误差界(hft-rt `CalibPair.err_ns`)
    pub clock_max_error_ns: u32,            //  156 [W] chrony 误差界
    pub prev_header_sha256: [u8; 32],       //  160 [P] 上一段 0..3072 的 SHA-256;首段全 0
    pub host_profile_sha256: [u8; 32],      //  192 [P]
    pub build_git: [u8; 20],                //  224 [P]
    pub _pad1: [u8; 4],                     //  244
    pub n_schema: u16, pub n_config: u16,   //  248 [P]/[W]
    pub _pad2: u32,                         //  252
    pub schema: [SchemaEntry; 128],         //  256 [P] { kind: u16, version: u16, size: u32 } 写者二进制已知的全部 kind
    pub config_digest: [u8; 32],            // 1280 [W] 引导块全部 ConfigBlob 的 BlobRef 按出现顺序拼接后的 SHA-256;条数 = n_config(u16,不设 16 的上限),内容与索引都在引导块里
    pub _rsv: [u8; 1756],                   // 1312
    pub header_crc32c: u32,                 // 3068 [W]
    // —— 写者行(64 B,写者 Release 写,读者 Acquire 读)——
    pub committed_len: AtomicU64,           // 3072 已提交字节(从文件头算起);读者上界
    pub last_seq: AtomicU64,                // 3080
    pub state: AtomicU32,                   // 3088 Preparing=0 Open=1 Sealed=2
    pub _w: [u8; 44],
    // —— persist 行(64 B,persist 线程写)——
    pub persisted_len: AtomicU64,           // 3136 已复制到冷副本
    pub synced_len: AtomicU64,              // 3144 已 fdatasync
    pub retired: AtomicU32,                 // 3152 1 = 热段即将删除,读者须在 1 s 内改读冷副本
    pub _p: [u8; 44],
    pub _tail: [u8; 896],                   // 3200..4096
}
```

## 提交游标

每条记录自带提交点,`committed_len` 只是读者与 persist 的上界提示:

1. 写者按"体 → 头部 `kind/flags/seq/tsc` → 最后 Release 写 `len`"的顺序写一条记录。`len == 0` 的位置即数据末尾。热段内存由内核清零,预创建时已缺页完成,运行期零缺页。
2. 写者在每轮主循环末、以及**把任何依赖该轮记录的数据提交到出环之前**,Release 写 `committed_len` 与 `last_seq`(`publish`)。`publish` 只发生在事件边界(该事件的 `EVENT_END` 已置位之后),所以 `committed_len` 之内的记录头都是终值。因此任何已发出的订单,其 `OrderDecision` 都已提交且对读者可见。
3. 读者 Acquire 读 `committed_len`,读到它为止;不越过它读取(热段)。
4. 进程崩溃后,恢复程序从 `committed_len` 向后扫描 `len != 0` 且结构合法、`seq` 连续的完整记录,直到第一个不合法处——这些记录都已完整写入(第 1 步的顺序保证)。
5. **事件边界**:sequencer 处理一个输入事件时写下的全部记录(输入本身 + 它产生的全部输出)中,最后一条置记录头 `EVENT_END` 位;事件之外独立写入的 Meta 记录与引导块末条(`BootDone`)也各自置位,**事件之内产生的 Meta(如处理回报时写的 `LatencySample`)不置位**;多段记录只在末段置位。恢复把第 4 步得到的末尾**再截断到最后一条带 `EVENT_END` 的记录之后**(不低于 `committed_len`:已发布的部分总在事件边界上;其后的半个事件整体丢弃、字节清零),因此 journal 里永远没有"输入已记录、输出只写了一半"的事件。raw journal 的每条记录都置位。

写者行只在批次边界写一次;冷读者轮询间隔 ≥ 50 µs、同进程的 advisor ≥ 10 µs,使该行在写者核上的 RFO(每次读者轮询后至多一次,≈ 100 ns)摊还到可忽略([research/06 §1.2](../../design/research/06-data-plane-replay-observability.md))。

## 记录对齐与 CRC

- 记录 = `RecordHeader`(24 B)+ 体 +(可选)CRC + 零填充,`len` 含全部,是 8 的倍数,≤ 65 536。
- `CRC` 标志置位时,记录**最后 4 字节**是 CRC32C,覆盖 `[0, len − 4)`(含记录头,`len` 按最终值计算);体末与 CRC 之间填零。
- 同一 journal 内 `seq` 逐条 +1(含 `SegmentRoll` 与多段的每一段);跨段 `first_seq == prev_last_seq + 1`,除非 `RECOVERY_GAP`。
- 记录的 kind 必须允许出现在该类 journal(`KindInfo.journals`);`0x7Fxx` 只在 dev 构建中出现,生产读者拒绝。

## 变长与多段记录

变长 kind 分两种布局([hft-event `KindInfo.layout`](../../crates/hft-event/README.md#记录头标志与-kind)):

- **`Var`**(`L2Batch`、`Advice`、`MarginReport`、`AccountConfigReport`):单条记录,体 = 定长前缀 + 变长尾部,**没有 `PartHdr`**;前缀里的计数决定尾部长度,整条 ≤ 64 KiB,不拆段。
- **`Multi`**(`L2Snapshot`、`ControlCmd`、`ConfigBlob`、`StateSnapshot`、`RawRx`、`RawTx`):每条记录体以 `PartHdr` 开头,按下述规则可拆成多段:

```rust
#[repr(C)] pub struct PartHdr {   // 16 B
    pub total_len: u32,           // 逻辑体总字节数(所有段的 chunk 之和)
    pub chunk_len: u32,           // 本段携带的逻辑体字节数
    pub part: u16,                // 0..parts-1
    pub parts: u16,               // ≥ 1
    pub _rsv: u32,
}
```

1. 逻辑体 ≤ 65 488 B(65 536 − 24 − 16 − 8)时写一段:`parts = 1`,不置 `PART`。
2. 否则按 ≤ 65 488 B 切块,`part = 0..n−1`;除末段外都置 `PART`;所有段 `kind` 相同、`tsc` 相同、`seq` 连续、**在 journal 中连续**(中间没有其他记录),且**不跨段文件**——写者在开始前检查剩余空间,不足则先滚动。
3. 逻辑事件的 `seq` = 第 0 段的 `seq`;事件在末段(未置 `PART`)提交后才算完整。读者缓冲各段、按 `part` 顺序拼接、校验 `Σ chunk_len == total_len` 后交付。上限:`parts ≤ 65 535`,`total_len < 4 GiB`;读者的重组缓冲上限可配置(默认 256 MiB),超出报错而不是分配。
4. 恢复时,末尾未完成的多段组(最后一段仍置 `PART`)整体丢弃:`committed_len` 回退到组首,组内字节清零。
5. CRC 策略按 kind 作用于每一段。
6. 逻辑体开头的定长前缀(`L2SnapHdr`、`CmdHdr`、`BlobHdr`、`RawHdr`,以及 `Var` 的 `L2BatchHdr`、`AdviceHdr`、`MarginReportHdr`、`AccountConfigReportHdr`)见 [hft-event 变长记录前缀](../../crates/hft-event/README.md#变长记录前缀);`StateSnapshot` 的逻辑体结构见[下文](#快照记录结构)。

## 段滚动与引导块

滚动是写者线程在**两个事件之间**完成的一次切换;`seq`/`chain` journal 的新段以一个**引导块**开头,使任何读者从段首即可自举——这是现有 WAL 滚动无快照、84% 段被隔离的直接修正([08 §9](../../design/08-数据面与回放.md))。

触发:

- **整点**:live driver 在 UTC 整点后**第一个空闲轮**(所有输入环为空)写一条 Input 记录 `Timer{Sys, Roll}`,sequencer 处理它时滚动;若整点落在任一订阅 venue 的资金费结算时刻前后 ±2 min 内,推迟到该窗口结束后的第一个空闲轮。回放读这条记录,因此滚动位置确定。
- **大小安全阀**(两级,阈值归 [hft-journal](../../crates/hft-journal/README.md)):`committed_len + 预留(2 × 上次引导块大小)` 超过第一级阈值后,live driver 在**第一个空闲轮**写 `Timer{Sys, Roll}`;超过硬上限仍未滚动时,在**下一个事件边界**写(不等空闲轮)。两种都是记录下来的 Input。预留区只供滚动使用;普通记录写不下时写者返回 `Full`,sequencer `Fatal(JournalFull)` 并 abort([hft-journal 调用约束](../../crates/hft-journal/README.md#写者))。
- **引导块写到一半崩溃**:该段没有已提交的 `BootDone`,恢复时按 `Preparing` 整段删除(热段与已复制的冷副本),从上一段的 `End` 之后同号重开;读者在 `BootDone` 提交前不交付该段记录。
- 启动、崩溃恢复、人工滚动。`--recover-from-snapshot` 启动时新段的 `SegmentRoll{Begin}` 与段头 `roll_reason` 为 `RecoverFromSnapshot`,引导块里另有 `Note{Boot, RecoverFromSnapshot}` 记录所用快照与跳过的 seq 区间;验证回放在这样的引导块处重新起步(规则归 [replayer](../../apps/replayer/README.md))。raw journal 由各自线程在 UTC 整点或安全阀滚动,不写引导块。

```
段 N   … 最后一个普通事件 (seq s)
       SegmentRoll { mark: End, segment_no: N, other: N+1, seq_mark: s }      seq s+1
       → state = Sealed,committed_len 为终值
段 N+1 SegmentRoll { mark: Begin, segment_no: N+1, other: N, seq_mark: s+1 }  seq s+2   ← 引导块开始
       HostProfile(当前)
       ClockStatus(当前校准)
       ConfigBlob { boot: 1 } × k      当前生效的 refdata、EngineConfig(Engine)、sim 实例的 SimConfig、每个 StrategyParams、RiskParams
                                       (k = 段头 n_config,摘要 = config_digest;读者从段首自举所需的全部配置都在这里)
       StateSnapshot { purpose: Boot }  (多段)
       L2Snapshot { origin: Keyframe } × 每个状态为 Live 的 L2 簿
       SegmentRoll { mark: BootDone, boot_records: 引导块记录数(含 Begin 与本条) }   ← 引导块结束
       … 普通事件
```

- 引导块全部由写者线程从本地状态导出:**不发 REST、不重连**。
- **验证回放跳过引导块**(不作为输入喂给 sequencer),并在同一位置重新生成、逐字节比对——比对屏蔽 Meta 记录(`HostProfile`、`ClockStatus`、`SegmentRoll`);这同时是对簿、仓位、策略状态确定性的检查。
- 周期关键帧:`StateSnapshot { purpose: Keyframe }` 在 Input 记录 `Timer{Sys, Snapshot}` 处**一次写完**(单条多段记录,写出耗时记入 [latency-budget §3.1](../latency-budget.md#31-p999-记录项));触发条件(周期 `keyframe_period_ms`、事件数 `keyframe_max_events`、到期后的最大推迟与之后在事件边界强制写)与写法归 [hft-sequencer](../../crates/hft-sequencer/README.md)(参数在 ECF1 段 5)。每个 Live 簿在每个周期内恰好一条 `L2Snapshot { origin: Keyframe }`,由 live driver 写成 Input 记录 `Timer{Keyframe, inst}` 在周期内均摊触发(不在同一时刻导出全部簿;回放照读这些 Timer,位置确定)。读者从任意时刻起步:找到目标时刻前第二个 `Keyframe` 快照 `K₀`,从 `K₀` 起读,每个簿在见到自己的关键帧后开始应用后续行情,读到下一个 `Keyframe` 快照 `K₁` 时全部簿已就绪,非簿状态取 `K₁`。
- `StateSnapshot { purpose: OnDemand }` 由 `SnapshotNow` 命令触发,写在段中间,不滚动。
- `StateSnapshot { purpose: Drain }` 在 Drain 完成、干净退出之前写出(段中间,不滚动);冷启动据"journal 末尾是 Drain 快照且其后无 Input"判定干净重启(条件归 [hft-sequencer](../../crates/hft-sequencer/README.md) 与 [hft-risk](../../crates/hft-risk/README.md))。
- `StateSnapshot { purpose: Recovery }` 在崩溃恢复(含 `--recover-from-snapshot`)的新段引导块里写出,反映 `recovery_end` 平移之后的状态;`Boot` 是其余滚动的引导块快照。
- **进程启动**:启动(含崩溃恢复)打开的新段在引导块(`BootDone`)之后,第一条普通记录是 live driver 写的 Input [`EngineClock`](../../crates/hft-event/README.md#engineclock0x050ainput),同一事件内紧跟 sequencer 的 `Note{Sequencer, EpochStart}`;此后才有其他 Input。运行中的整点/安全阀滚动不写 `EngineClock`。引擎时间的算法见 [event-model 时间戳语义](event-model.md#时间戳语义)。

```
段 M(进程启动)  SegmentRoll{Begin} … 引导块 … SegmentRoll{BootDone}
                  EngineClock { base_engine_ns, tsc_hz_milli, prev_last_seq, source }     ← Input
                  Note{Sequencer, EpochStart}                                             ← Output,同一事件,EVENT_END
                  … 普通事件
```

## 快照记录结构

> 草案:`SnapshotHdr`、`SectionHdr` 与各节的行格式、行版本实现时以代码与编译期断言为准;节号、节名、码值与字段名全仓一致。

`StateSnapshot`(0x0504)的逻辑体(多段重组后):

```rust
#[repr(C)] pub struct SnapshotHdr {   // 96 B
    pub purpose: u8,                  //  0 SnapshotPurpose:Boot=1 Keyframe=2 OnDemand=3 Recovery=4 Drain=5(只追加)
    pub _pad: [u8; 3],                //  1
    pub n_sections: u32,              //  4
    pub at_seq: u64,                  //  8 快照反映的是处理完该 seq 之后的状态
    pub state_hash: [u8; 16],         // 16 各节内容按写出顺序(= hft-sequencer 快照节写出顺序,节 16 除外)的 xxh3-128;purpose = Keyframe 时恒为 0(见下)
    pub n_books: u32,                 // 32 随后应出现的 L2 关键帧数(Boot、Recovery);其余 purpose 为 0
    pub _pad2: u32,                   // 36
    pub out_hash: [u8; 16],           // 40 两快照之间(不含边界)的 Output 滚动 xxh3-128,区间与字节流规则归 hft-sequencer "状态哈希"
    pub snap_tsc: u64,                // 56 测量字段:写快照时本次运行的 TSC;purpose = Recovery 时等于 recovery_end 所用的 now(本次运行 TscClock,不取日志值)
    pub boot_id: [u8; 16],            // 64 测量字段:写快照时主机的 /proc/sys/kernel/random/boot_id(只供诊断,不参与任何恢复计算)
    pub snap_utc_ns: i64,             // 80 测量字段:写快照时的 UTC(TSC 校准换算;只供人读与冷读者定位)
    pub engine_ns: i64,               // 88 at_seq 那条记录的引擎时间(event-model 时间戳语义);--recover-from-snapshot 时作为 EngineClock.base_engine_ns;0 = 未提供
}
const _: () = assert!(size_of::<SnapshotHdr>() == 96);
// snap_tsc、boot_id、snap_utc_ns 在 hft-event MEASURE_MASK 中屏蔽:换机或重跑验证回放时它们必然不同,不参与 state_hash/out_hash 与逐字节比对。
// 跨重启保存的时刻(策略、ledger、risk 状态里的冷却与截止)一律存 engine_ns 或 UTC 日历量,恢复后原样有效,不需平移。
// 唯一例外:OMS、hft-exec 与定时器堆及身份登记(节 2、12、21、8、27)的已登记 TSC 行;恢复结束时由 recovery_end 把它们按"停机时间不计"平移到本次运行
// (t → now + (t − stop_tsc),跨开机、换机同一规则;行清单与平移规则归 hft-sequencer、hft-oms、hft-exec)。
// 节 3 的 resume_hold_until 是登记的 TSC 字段,但不平移:recovery_end 重新设定它(规则归 hft-oms)。
// 节 21 Gate 行的 gate_closed_tsc 同样是登记的 TSC 字段、不平移:recovery_end 把它重设为 now(规则归 hft-exec)。
// 其余 TSC 不跨重启使用:节 26 ConnLive.live_tsc 在处理 EngineClock 时清空;节 7 不含 rx_tsc,恢复后 Top.rx_tsc 为 0(规则归 hft-book);节 16 ClockCal 只供冷读者换算、不计入 state_hash;其他节里的 *_tsc 只是测量值。
#[repr(C)] pub struct SectionHdr {    // 16 B,每节前缀;节体 8 B 对齐
    pub section: u16, pub version: u16, pub count: u32, pub len: u32, pub _rsv: u32,
}
```

**`state_hash` 只在读者侧算关键帧的值**:`purpose = Keyframe` 的快照头 `state_hash` 恒写 0——实盘写关键帧时不计算,验证回放重新生成关键帧时同样写 0,逐字节比对照常成立;需要关键帧哈希的读者(replayer、冷读者)按上面的定义对各节内容自行计算。其余 purpose 的 `state_hash` 由写者计算。`out_hash` 不受影响。

| 节 | 名 | 行格式归属 | 内容 |
|---|---|---|---|
| 1 | `Positions` | [hft-ledger](../../crates/hft-ledger/README.md) | 每 (account, inst) 仓位、成本(均价)、已实现 PnL、费用(资金费累计在节 13 的 `PnlBreakdown.funding`) |
| 2 | `OrderSlots` | [hft-oms](../../crates/hft-oms/README.md) | 所有非 `Free` 槽(含 `Unknown`;槽的私有状态位与线上 `OrderFlags` 分开存)(行格式与行版本见归属 crate) |
| 3 | `OmsCounters` | [hft-oms](../../crates/hft-oms/README.md) | engine_epoch、各 (venue, account) 的 gen 与空闲表游标(行版本见归属 crate) |
| 4 | `RiskState` | 行格式 = [hft-event `CapRecord`](../../crates/hft-event/README.md#cap-记录)(48 B);语义归 [hft-risk](../../crates/hft-risk/README.md) | 全部生效 cap,每条 `{source, scope, set_seq, params}`(含不可清除的 Draining 与条件型 ClockBad);kill 字由它重算,不另存 |
| 5 | `StrategyState` | 行头 `StrategyRowHdr`(含 `kind_id`、`schema_ver`、`params_version`、策略状态布局版本 `desc_version`、`len`)与每 group 的 `GroupSeqRow`(`spec_seq`)归 [hft-sequencer](../../crates/hft-sequencer/README.md);`bytes` 由策略经 [hft-strategy-sdk](../../crates/hft-strategy-sdk/README.md) 的 `SnapshotWriter` 写出,`restore` 接受的版本规则归 SDK | 每策略实例一行:行头 + `GroupSeqRow` × n_groups + 快速部分的状态字节(慢速部分 `Model` 的状态由 advisor 从 journal 与模型检查点重建,不进本节) |
| 6 | `BookMeta` | [hft-book](../../crates/hft-book/README.md) | 每 inst 簿状态、`last_id`、窗口基准、越界/交叉计数 |
| 7 | `Tops` | hft-book | 每 inst 的 BBO 缓存(`Bbo` 流);行不含 `rx_tsc`,恢复后 `Top.rx_tsc` 为 0(规则归 hft-book) |
| 8 | `Timers` | [hft-sequencer](../../crates/hft-sequencer/README.md) | 待触发定时器 |
| 9 | `ActiveConfig` | [hft-sequencer](../../crates/hft-sequencer/README.md) | 生效配置的 `BlobRef` 列表 |
| 10 | `MarketLast` | [hft-sequencer](../../crates/hft-sequencer/README.md) | 每 inst 最新成交、资金费(含实际结算周期)、标记价、指数价、BBO 连接成员位图 `bbo_member_mask` 与逐 inst 的重同步标志 |
| 11 | `ChainState` | [hft-chain-evm](../../crates/hft-chain-evm/README.md)(optional,仅 chain journal) | nonce 表、在途交易生命周期、池镜像版本 |
| 12 | `OmsLinks` | [hft-oms](../../crates/hft-oms/README.md) | 活动 `LegLink`/`HedgePlan`、对冲余数累计、`LegTracker` 状态、未过期的墓碑(`Tomb` 行,迟到成交入账用)、外来累计结算、原始成交来源及可验证的单笔原件、尚未交出的输出队列 `pending_out`(`Out` 行:行头 kind = 5,每条 `OutItem` 32 B,按 FIFO 顺序、队头在前——顺序本身是状态;固定容量的输出从不丢弃,溢出部分经续推定时器在后续轮次交出)(行格式与行版本见归属 crate) |
| 13 | `LedgerGroups` | [hft-ledger](../../crates/hft-ledger/README.md) | 账户账(各所现金、保证金余额、当日 PnL)、换算率表、每 (account, group) 的 PnL 分解、轮次、资金费对账校正与待确认原始成交凭据;optional 链上的 `chain_pending`(行格式见归属 crate) |
| 14 | `ModeTable` | [hft-risk](../../crates/hft-risk/README.md)(`hft-risk::mode`);行格式在本页 | 只存期望模式:`ModeRow { strategy: StrategyId, account: AccountId, mode: RunMode, flags: u8 /* DEFAULT=1:该行是 (strategy, account) 的 default_mode,此时 group = GROUP_NONE(0);LANDED=2:期望的 Shadow 已生效 */, group: GroupId }`(8 B,行版本 2) |
| 15 | `CmdDedup` | [hft-sequencer](../../crates/hft-sequencer/README.md) | 最近的 `cmd_id` 及其结果码(条数 = ECF1 段 3 的 `recent_cmd_ids`,容量归 [hft-sequencer](../../crates/hft-sequencer/README.md))([control-plane 幂等](control-plane.md#幂等));Viewer `Kill` 的预览记录(≤ 16 条 `{cmd_hash, issuer, preview_engine_ns}`,[control-plane 版本与确认](control-plane.md#版本与确认);处理 `EngineClock` 时清空) |
| 16 | `ClockCal` | [hft-sequencer](../../crates/hft-sequencer/README.md) | 最近一条 `ClockStatus` 中换算所需的字段子集(行格式归 hft-sequencer),只供冷读者从快照起步时换算 UTC;**不计入 `state_hash`**,sequencer 不读(时钟决定都是 `Timer` 输入) |
| 17 | `RiskCounters` | [hft-risk](../../crates/hft-risk/README.md) | 每账户当日 PnL 基线、日亏触发线与额度;每 (venue, account[, inst]) 速率令牌与退避;对账连续轮数(`ReconRow`)。不含 `last_day_rolled`(节 20)与 `ctl_state_version`(节 18) |
| 18 | `Gates` | [hft-risk](../../crates/hft-risk/README.md)(`hft-risk::mode`);行格式在本页 | `GateRow { account: AccountId, released: u8, clean_restart: u8, recovery_flags: u8 /* RecoveryFlags 位(TAIL_LOST、RECOVER_MISMATCH、POISON、FROM_SNAPSHOT、CLEAN_DRAIN;码值与含义归 hft-risk mode 模块) */, gate_bits: u32 /* 含 ACCOUNT_CFG、IDENTITY */, reconciled_venues: u64 /* VenueId 位图 */, cfg_ok_venues: u64, identity_ok_venues: u64, conns_ok_venues: u64 /* 主下单连接与用户流都 Up 的 venue 位图,对冲闸门子集用(判定归 [hft-risk](../../crates/hft-risk/README.md)) */, ctl_state_version: u64 /* 全局值,每行相同 */, standby_ok_venues: u64 /* 热备下单连接 Up 的 venue 位图,ORDER_CONNS 用(判定归 hft-risk) */, gflags: u64 /* KILL_SELFTEST=1:本次运行该账户的 kill 自检已通过;STORAGE_DRAIN_SENT=2:当前 Storage cap 已产生 StorageCritical(全局值,每行相同) */ }`(64 B,行版本 5),键 account |
| 19 | `ReconPending` | [hft-sequencer](../../crates/hft-sequencer/README.md) | 尚未收齐的 `ReconcileReport` 分组(已到达的条目原样保存,恢复后继续收齐) |
| 20 | `SeqDay` | [hft-sequencer](../../crates/hft-sequencer/README.md)(`last_day_rolled` 的唯一归属);行格式在本页 | `SeqDayRow { last_day_rolled: u32 /* 自 1970-01-01 的 UTC 日序号 */, _pad: u32 }`(8 B,一行,行版本 1):时钟可信且 UTC 日 > 该值时 live driver 写 `Timer{Sys, DayRoll, day}`;`day ≤ last_day_rolled` 的 DayRoll 忽略 |
| 21 | `ExecTargets` | [hft-exec](../../crates/hft-exec/README.md) | 每 (account, group) 的当前目标(按字段展开的显式行,不原样嵌入 SDK 类型)、进度、切片冷却截止、待默认处理的外部成交截止、自动有界平仓状态;每账户的清仓锁 `flatten_lock`;每账户一条闸门行(`gate_open`、`venue_open`、`gate_closed_tsc`)(行顺序、行格式与行版本见归属 crate) |
| 22 | `LedgerInventory` | [hft-ledger](../../crates/hft-ledger/README.md) | 每 (account, venue, asset) 的现货库存(按 group 或 unassigned)与余额基线(`SetBalanceBaseline`) |
| 23 | `MarginState` | [hft-risk](../../crates/hft-risk/README.md) | 每 (account, venue) 最近一条 `MarginReport` 的摘要与 seq、`MarginStress` 档位与减仓截止 |
| 24 | `VenueNotices` | [hft-sequencer](../../crates/hft-sequencer/README.md) | 生效的 `VenueNotice`(下架时刻、结算周期覆盖、维护窗口) |
| 25 | `EngineClock` | [hft-sequencer](../../crates/hft-sequencer/README.md);行格式在本页 | 当前运行的引擎时钟基准:`EngineClockRow { base_engine_ns: i64, base_tsc: u64 /* 该 EngineClock 记录的记录头 tsc */, tsc_hz_milli: u64, clock_seq: u64 /* 该 EngineClock 记录的 seq */ }`(32 B,一行,行版本 1);计入 `state_hash`(全部来自 Input,回放确定);从中途段起步的读者据此换算 `engine_ns` |
| 26 | `ConnLive` | [hft-sequencer](../../crates/hft-sequencer/README.md) | 每 (venue, BBO 连接槽) 的 `ConnLive{up, resync, live_tsc}`(槽数 `MAX_BBO_CONN_SLOTS`);只含本次运行的值,处理 `EngineClock` 时清空;从关键帧起步的读者据此延续 `BboView` 的连接健康 |

| 27 | `StrategyTimers` | [hft-sequencer](../../crates/hft-sequencer/README.md) | 每绑定实例的 SDK TimerId 计数器及存活登记;恢复与节 8 逐项核对,截止一起平移 |

各节的行格式与行版本只在归属 crate 登记(本页只登记节号、节名与归属;本页给出行格式的节 14、18、20、25 除外)。模拟账户的状态与实盘账户在同一组节里(按 `account` 区分);sim-gw 自身的撮合状态不进本快照,启动时由 [hft-sim](../../crates/hft-sim/README.md) 从这些节重建。未知节号的读者按 `len` 跳过;节内行格式的演进规则同事件体(只追加、零值表示未提供,`version` 递增)。行只由显式字段与 `_pad` 组成(无隐式填充,未用字节补零);不原样嵌入数据枚举(`#[repr(C, u8)]`)或标明"不以原始字节写进 journal"的类型,这类内容按字段展开(判别值 + 显式填充 + 定长载荷)。L2 档位不进 `StateSnapshot`,由紧随其后的 `L2Snapshot` 关键帧承载。

## 冷副本与热段

1. persist 线程按顺序把热段 `[persisted_len, committed_len)` 原样 `pwrite` 到冷副本同偏移,按阈值 `fdatasync`,再更新 `persisted_len`/`synced_len`(热段 persist 行),并把 `synced_len` 写入冷副本头部的 persist 行。
2. 冷副本创建时 `fallocate` 到段容量;封存时(热段 `Sealed` 且全部同步)截断到 `committed_len`、`fsync` 文件与目录,再写 `.seal`。
3. 冷副本与热段在 `[0, committed_len)` 上除写者行与 persist 行(3072..3200)外逐字节相同;两者的内容哈希都按"该 128 B 置零"计算。
4. 热段在"已封存 + 已同步 + 比最新段旧 `hot_keep` 段"后置 `retired = 1`,宽限 2 s 后删除;读者看到 `retired` 必须切到冷副本同偏移继续。
5. 冷副本本地保留:`seq`/`chain` 7 天、raw 3 天;删除前提是存在 `.uploaded`([08 §3](../../design/08-数据面与回放.md))。

```rust
#[repr(C)] pub struct SealRecord {   // .seal 文件,128 B
    pub magic: [u8; 8],              // *b"HFTSEAL1"
    pub segment_no: u64, pub first_seq: u64, pub last_seq: u64, pub len: u64,
    pub sealed_utc_ns: i64,
    pub sha256: [u8; 32],            // 冷副本 [0, len) 的 SHA-256(3072..3200 置零)
    pub reason: u8 /* 同 SegmentRoll.reason */, pub flags: u8 /* 同 SegmentHeader.flags */, pub _pad: [u8; 2],
    pub _rsv: [u8; 40],
    pub crc32c: u32,                 // 覆盖前 124 B
}
```

## 校验与缺段检测

`cargo xtask journal-verify <dir>`(检查逻辑在 [hft-journal](../../crates/hft-journal/README.md) 的读者,命令规格在 [tools](../../tools/README.md#其他-xtask-子命令))逐项检查,任一失败即报告段号与偏移:

1. 段头:magic、`format_version`、`header_crc32c`。
2. 链:`segment_no` 连续;`prev_header_sha256 == SHA-256(上一段头 0..3072)`;`first_seq == prev_last_seq + 1 == 上一段 last_seq + 1`,除非本段 `RECOVERY_GAP`(报告为"可能丢失的尾部",不算损坏)。
3. 记录:结构合法(`len` 界限与对齐、kind 已注册且允许、`seq` 逐条 +1)、CRC、多段组完整。
4. 引导块:`seq`/`chain` 段的第一条是 `Begin`,存在 `BootDone`,其 `boot_records` 与实际一致,`n_books` 与关键帧数一致。
5. 封存:`.seal` 的 CRC、`sha256` 与文件内容一致,`last_seq`/`len` 与段内一致;最后一条是 `End`(`Recovery` 滚动时亦然)。
6. 汇总:缺失的 `segment_no` 区间、`RECOVERY_GAP` 位置、未封存段。

## 读者 API 概要

读者只读映射(`PROT_READ`),可在任意进程;从 `Oldest`、`Seq(n)`、`SegmentBoot(n)`、`LatestBoot`、`LatestKeyframe`、`Tail` 起步;逐条交付记录或重组后的逻辑事件;报告自己是否处于引导块;热段退役或删除时透明切到冷副本;遇到缺段按 `OnGap::{Error, SkipToNextBoot}` 处理。签名与语义见 [hft-journal](../../crates/hft-journal/README.md#读者);Python 访问经 [hft-py](../../crates/hft-py/README.md)。

离线不可变文件由 `JournalReader::open_files` 按段头排序并预检整组后交付;返回 `FileOpenError` 或带 `FileSegment` 清单的读者。`open_files_recoverable` 显式保留已标记恢复缺口;封闭边界、seal、原路径与输入不可变契约归 [hft-journal 读者](../../crates/hft-journal/README.md#读者),不改变段头、记录或快照格式。

## 格式演进

- `format_version` 只在段头或记录框架(提交规则、多段、CRC 位置)变化时递增;事件体变化走 [event-model 演进规则](event-model.md#演进规则),由段头 `schema[]` 表描述。
- 读者必须支持所有已发布的 `format_version`;新版本写者不改变旧字段的偏移与含义,只使用 `_rsv` 区或追加。
