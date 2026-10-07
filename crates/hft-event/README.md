---
description: "事件体、策略载荷容器(Advice、StrategyParams)、控制命令与 ConfigBlob 的唯一 Rust 定义与编解码;journal 写者/读者、环、冷进程、PyO3 与 gen-schema 生成器都从这里取布局"
kind: crate
tier: pure
scope: core
status: spec
---

# hft-event

## 概要

`hft-event` 是记录头、`EventKind` 注册表、全部事件体、策略载荷的**通用容器**(变长不透明的 `Advice`、`StrategyParams` 的 `StrategyParamsHdr` + `GroupSpec` + schema 序编码的 body)、`ControlCmd`/`CmdResult`/`ConfigBlob` 线格式与变长记录前缀的**唯一 Rust 来源**。它提供零拷贝(`zerocopy`)的编码/解码、kind 元数据(域、角色、定长/变长、`schema_version`、字段描述),并向 `cargo xtask gen-schema` 暴露字段表以生成 Arrow schema、Python dtype 与 `docs/generated/event-catalog.md`。**新增或修改策略不需要改本 crate**:策略的慢速输出是不透明载荷,参数 body 按策略自带的 schema 解码([hft-strategy-sdk](../hft-strategy-sdk/README.md)),本 crate 只定义容器。它不拥有记录头与 kind 编号段的**规则**([event-model](../../docs/subsystems/event-model.md)),不拥有 journal 文件结构与多段机制([journal-format](../../docs/subsystems/journal-format.md)),不拥有命令的语义与权限([control-plane](../../docs/subsystems/control-plane.md))。热路径上,进程内环直接搬运类型化的体(不经字节解析);字节编解码发生在 journal 追加、journal 读取与跨进程/字节环边界。

## 目录

- [使用本组件](#使用本组件)
- [事件体布局](#事件体布局)
- [策略载荷容器](#策略载荷容器)
- [环元素类型](#环元素类型)
- [控制命令编码](#控制命令编码)
- [变长记录前缀](#变长记录前缀)
- [实现说明](#实现说明)
- [延迟与资源契约](#延迟与资源契约)
- [测试与验收](#测试与验收)
- [依赖](#依赖)
- [延伸阅读](#延伸阅读)
- [已知限制与后续工作](#已知限制与后续工作)

## 使用本组件

### 记录头、标志与 kind

`RecordHeader`(24 B)、kind 编号段与每个 kind 的分类(Input/Output/Meta)列在 [event-model](../../docs/subsystems/event-model.md#记录头24-b全系统唯一);本 crate 给出位值与注册表:

```rust
#[repr(transparent)] pub struct RecordFlags(pub u16);
impl RecordFlags { CONFLATED = 1 << 0; SHADOW = 1 << 1; HW_TS = 1 << 2; SW_TS = 1 << 3; PART = 1 << 4; CRC = 1 << 5;
                   EVENT_END = 1 << 6; }  // 一个输入事件的全部记录(输入 + 其全部输出)的最后一条置位;语义见 journal-format 提交游标

#[repr(transparent)] pub struct EventKind(pub u16);   // 常量:EventKind::BBO = 0x0101 … 与 event-model 编号表一致

pub struct KindInfo {
    pub kind: EventKind, pub name: &'static str, pub domain: Domain,   // Market|Exec|Chain|Control|System|Raw|Test
    pub layout: Layout,          // Fixed { size: u16 }
                                 // | Var { prefix: u16, max: u32 }   单条记录、无 PartHdr:体 = 定长前缀 + 变长尾部(L2Batch、Advice、MarginReport、AccountConfigReport)
                                 // | Multi { prefix: u16 }           以 PartHdr 开头、可拆成多段(L2Snapshot、ControlCmd、ConfigBlob、StateSnapshot、RawRx、RawTx)
    pub schema_version: u16,     // 从 1 起;布局变化(只追加)时 +1
    pub role: Role,              // Input | Output | Meta | ByBody(见下);RawRx/RawTx = Meta(只在 raw journal,回放不读)
    pub journals: JournalMask,   // SEQ | RAW | CHAIN:允许出现在哪类 journal。RawRx/RawTx = RAW;SegmentRoll = SEQ | RAW | CHAIN;
                                 // HostProfile、ClockStatus、ConfigBlob、StateSnapshot = SEQ | CHAIN(chain journal 的引导块,optional);其余 = SEQ
    pub crc: CrcPolicy,          // Never | Always:该 kind 在 seq journal 中是否带 CRC。Always = ControlCmd、ConfigBlob、StateSnapshot、RawRx、RawTx;
                                 // 其余 Never(raw/chain journal 的全部记录由写者按 journal 类别加 CRC,见 journal-format 三类 journal)
    pub optional: bool,          // 属于 optional 组件:0x0301–0x0304 为 true,其余 false;生产者只在对应组件启用时存在;读者照常解码
    pub fields: &'static [FieldDesc],
}
pub struct FieldDesc { pub name: &'static str, pub offset: u16, pub ty: FieldTy, pub measure: bool, pub doc: &'static str }
// measure = 测量字段(如 OrderDecision.decided_tsc):验证回放逐字节比对 Output 记录时屏蔽它们与记录头 tsc
pub enum FieldTy { U8, U16, U32, U64, I8, I16, I32, I64, Px, Qty, Notional, Rate, Tsc, UtcNs, Id(IdKind),
                   Enum(&'static EnumDesc), Bits(&'static EnumDesc), Bytes(u16), Pad(u16), Nested(&'static [FieldDesc]) }
pub struct EnumDesc { pub name: &'static str, pub width: u8 /* 字段字节宽 */, pub items: &'static [EnumItem] }   // 码表(Enum)或具名位(Bits)
pub struct EnumItem { pub name: &'static str, pub value: u32 }
// 整数编码的码表与位集合(ExecDecision.action、ReconcileReport.sflags 等)以 `codes` 模块的 EnumDesc 登记,FieldDesc.ty 引用它们
pub static KINDS: &[KindInfo];
pub fn kind_info(k: EventKind) -> Option<&'static KindInfo>;   // 未知 kind → None,读者跳过该记录(按 len)
```

**测试 kind**(cargo feature `dev-kinds`,默认关闭,只允许出现在 `[dev-dependencies]`,`cargo xtask deps` 检查):启用后 `KINDS` 另登记 `0x7F01`–`0x7F06`(`domain = Test`,`journals = SEQ | RAW | CHAIN`,`schema_version = 1`,`optional = false`),供 hft-journal 等下游 crate 的测试使用;不启用时这些编号 `kind_info` 返回 `None`,生产读者按 [event-model](../../docs/subsystems/event-model.md#kind-编号段) 拒绝。

| kind | 名 | 布局 | role | crc |
|---|---|---|---|---|
| `0x7F01` | `TIn` | Fixed 80 B | Input | Never |
| `0x7F02` | `TOut` | Fixed 56 B | Output | Never |
| `0x7F03` | `TMeta` | Fixed 32 B | Meta | Never |
| `0x7F04` | `TCrc` | Fixed 40 B | Input | Always |
| `0x7F05` | `TMulti` | Multi(前缀 0 B,逻辑体为不透明字节) | Input | Never |
| `0x7F06` | `TVar` | Var(前缀 16 B,首 4 字节 u32 = 尾部长度 L;`max` = 65 000) | Input | Never |

测试 kind 的体是不透明字节(定长体全部字段为 `Bytes`),不进 `gen-schema` 生成物与 `schemas/event-layout.lock`。

**记录分类**(每个 kind 恰属一类;回放驱动与验证比较器只看这里;每个 kind 的归类列在 [event-model kind 编号段](../../docs/subsystems/event-model.md#kind-编号段)):

| 类 | 含义 | 回放 | 验证比对 |
|---|---|---|---|
| `Input` | 能影响 sequencer 状态的一切输入,包括**依赖运行环境的决定**——何时快照、何时滚动、何时出关键帧、何时继续扇出、存储是否退化,由 live driver 当场写成 `Timer{…}` | 喂给 sequencer | — |
| `Output` | sequencer 由输入确定性地产生 | 重新生成 | 逐字节(屏蔽测量字段) |
| `Meta` | 测量与结构记录:占 seq,但**不参与任何状态、不参与验证比对**(引导块比对同样屏蔽) | 跳过 | 不比 |

`Role::ByBody` 的 kind 按体内字段决定类别(`classify` 的唯一规则):`L2Snapshot`(`origin = Keyframe` 为 Output,`Exchange`/`TopN` 为 Input)、`ConfigBlob`(`boot = 1` 为 Output,`boot = 0` 为 Input)、`Note`(按 `source`,见 [Note 来源与码登记表](#note-来源与码登记表);回放角色写"同 `Venue`"的源按 Meta 返回)、`FundingSettle`(`synth = 1` 为 Output,`synth = 0` 为 Input);判定字段取其他值 → `InvalidEnum`。`SegmentRoll` 是 **Meta**(结构记录;滚动决定本身是 Input `Timer{Roll}`)。sequencer 不得读取 Meta 记录的内容做任何决定(例如 UTC 定时器不能用 `ClockStatus` 换算;整点滚动由 live driver 写 `Timer{Roll}`)。

**模拟账户**(`AccountMeta.kind = Sim`,见 [hft-refdata](../hft-refdata/README.md#数据结构))的回报与实盘回报是**同一组事件**(`OrderEvent`、`SendReport`、`BalanceReport`、`PositionReport`、`FundingSettle`、`ExternalFill`、`MarginReport`…),由 sim-gw 产出、经回报环进入、记为 Input;不另设 kind 或标志位,读者按 `account` 查 `AccountMeta.kind` 区分。

### 体 trait 与编解码

```rust
pub trait Body: zerocopy::TryFromBytes + zerocopy::IntoBytes + zerocopy::KnownLayout + zerocopy::Immutable + Copy {
    const KIND: EventKind;
    const SCHEMA_VERSION: u16;
}

pub fn record_len(body_len: usize, crc: bool) -> u32;                  // 24 + body (+4 crc) 向上取 8
pub fn encode_fixed<B: Body>(out: &mut [u8], seq: u64, tsc: u64, flags: RecordFlags, b: &B) -> u32;
    // 写 kind = B::KIND;返回写入长度 = record_len(size_of::<B>(), flags 含 CRC);out 短于它 → debug_assert(写者按 record_len 预留)
pub fn encode_var<P: Body>(out: &mut [u8], seq: u64, tsc: u64, flags: RecordFlags, prefix: &P, tail: &[u8]) -> Result<u32, EncodeErr>;
    // Var 布局;tail.len() ≠ 前缀计数推出的长度 L(见下)→ Count;L 超过 KindInfo.max → TooLong;out 不足 → BufTooSmall
#[repr(u8)] pub enum EncodeErr { TooLong = 1, BufTooSmall = 2, Count = 3 }
pub struct RecordView<'a> { pub hdr: RecordHeader, pub body: &'a [u8] }   // hdr 按值(缓冲不要求 8 B 对齐);body = 记录 [24, len)(CRC 位置位时 [24, len − 4)),含体末零填充
pub fn encode_part(out: &mut [u8], seq: u64, tsc: u64, flags: RecordFlags, kind: EventKind, ph: &PartHdr, chunk: &[u8]) -> Result<u32, EncodeErr>;
    // Multi 记录的一段(体 = PartHdr + chunk,PartIter 产出);chunk.len() ≠ ph.chunk_len → Count;记录 > 65 536 → TooLong;out 不足 → BufTooSmall
pub fn decode_record(buf: &[u8]) -> Result<RecordView<'_>, DecodeErr>;
    // buf < 24、len < 24、len > buf.len() 或 CRC 位置位而 len < 32 → Short;len > 65 536 → TooLong;len 不是 8 的倍数 → Misaligned;
    // CRC 位置位且末 4 字节 ≠ crc32c([0, len − 4)) → BadCrc;不检查 kind 是否登记(未登记的由调用方经 kind_info 判定后按 len 跳过)
impl<'a> RecordView<'a> {
    pub fn read<B: Body>(&self) -> Result<B, DecodeErr>;   // hdr.kind ≠ B::KIND → KindMismatch;按值拷贝 min(body.len(), size_of::<B>()) 字节:
                                                           // 体比 B 短(旧写者)→ 缺失字段补零;更长(新写者)→ 截断;枚举字段为 0 或越界 → InvalidEnum
    pub fn view<B: Body>(&self) -> Result<&'a B, DecodeErr>; // 零拷贝:hdr.len 小于/大于 record_len(size_of::<B>(), CRC 位)→ Short/TooLong;
                                                           // body 地址不是 8 B 对齐 → Misaligned;kind 与枚举检查同 read
    pub fn var<P: Body>(&self) -> Result<(&'a P, &'a [u8]), DecodeErr>; // Var 布局:返回前缀与长度恰为 L 的尾部;
                                                           // body.len() 小于/大于 size_of::<P>() + L 向上取 8 → Short/TooLong;填充字节不检查
}
#[repr(u8)] pub enum DecodeErr { Short = 1, TooLong = 2, Misaligned = 3, BadCrc = 4, KindMismatch = 5, InvalidEnum = 6, UnknownKind = 7,
                                 BadPart = 8 /* 多段组不一致,见变长记录前缀 Reassembler */ }
pub fn classify(kind: EventKind, logical_body: &[u8]) -> Result<Role, DecodeErr>;
    // 记录的实际类别(Input/Output/Meta,从不返回 ByBody);未登记 kind → UnknownKind;ByBody 的判定字段越界 → InvalidEnum(见下)
```

Var 尾部长度 L 由前缀计数推出:`Advice` = `AdviceHdr.len`;`MarginReport` = 16 × `n`;`AccountConfigReport` = 16 × `n_lev`;`L2Batch` = 16 × (`n_bids` + `n_asks`)。

- 所有体 `#[repr(C)]`、无隐式填充(`IntoBytes` 派生强制)、大小是 8 的倍数、无指针/`String`/`Vec`;每个体写出全部 `_pad`,并对大小与本 README 注释中的每个偏移写 `size_of`/`offset_of` 编译期断言([hft-types 布局规则](../hft-types/README.md#布局规则全仓-reprc-结构适用))。含枚举字段的体只能派生 `TryFromBytes`(枚举 0 值与越界值非法),`read`/`view` 顺带完成枚举合法性检查——跨进程与 journal 读取的损坏数据在这里被拒绝。
- 线格式只支持小端;大端目标编译失败。
- 新字段只追加在体尾,或占用已有的保留字节(两种做法的条件与版本规则归 [event-model 演进规则](../../docs/subsystems/event-model.md#演进规则));**全零必须表示"未提供"或该字段的缺省含义**(旧写者的记录被补零读出时语义正确);新字段不得是 0 非法的枚举。

### 所有权与线程

本 crate 无状态;`KINDS` 是 `'static` 常量表。任何线程都可调用编解码;热路径只调用 `encode_fixed`/`encode_var`(journal 追加)与 `record_len`。

## 事件体布局

> 草案:本页与 event-model 的全部字节布局(字段顺序、偏移、`_pad`、大小)实现时以代码与编译期断言为准;码值与字段名全仓一致,布局一旦改动同一改动内更新本页。

`Bbo`、`L2Delta`、`Trade`、`OrderEvent`、`TxLifecycle`、`Timer`、`ConnState` 与公共前缀 `MarketHdr` 的布局在 [event-model](../../docs/subsystems/event-model.md#关键事件体);以下是其余体。偏移写在注释里,`gen-schema` 以代码为准导出。类型来自 [hft-types](../hft-types/README.md)(`Px`/`Qty` 为资产指数,`Notional` 为报价币 1e-6,`Rate` 为 1e-9)。

### 行情(0x01xx)

```rust
pub struct Funding     { pub h: MarketHdr, pub rate: Rate, pub next_funding_ns: i64, pub interval_s: u32, pub _pad: u32 } // 80 B
pub struct MarkPrice   { pub h: MarketHdr, pub px: Px }                                                               // 64 B
pub struct IndexPrice  { pub h: MarketHdr, pub px: Px }                                                               // 64 B
pub struct Liquidation { pub h: MarketHdr, pub px: Px, pub qty: Qty, pub side: Side, pub _pad: [u8; 7] }              // 80 B(公开市场强平流,不是本账户的强平)
```

`MarketHdr` 是 56 B(布局在 [event-model](../../docs/subsystems/event-model.md#关键事件体),含常态记录的 `sw_rx_ns` 与 `rx_empty_gap_ns`);以上大小都按它计算。

`Funding.interval_s` 是该 inst **实际**的结算周期:交易所报文直接给出时原样填;只给下次结算时刻时,由 venue 协议用相邻两次 `next_funding_ns` 之差推出(方法写在各 `venue-*` README);推不出时为 0(= 未知,读者退回 refdata `InstMeta.funding_interval_s` 默认值)。结算周期变化由 [hft-strat-lib](../hft-strat-lib/README.md) 的 FundingStats 检测;人工提前登记的变化走 `VenueNotice{FundingInterval}` 命令。

深度有两种产出(路由与启用条件归 [hft-venue](../hft-venue/README.md),路由枚举 `RouteKind` 归 [hft-venue-proto](../hft-venue-proto/README.md)):

- **分档快照流**(`RouteKind::DepthTopN`):每条交易所消息一条 `L2Snapshot{origin = TopN}`(Input,前缀 `L2SnapHdr` 见[变长记录前缀](#变长记录前缀)),簿按整替换应用,无序号链、无 REST 重同步。
- **增量深度**(可选扩展):`L2Batch`(0x0109,`Var`),**一条交易所增量消息一条记录**,前缀 `L2BatchHdr`;sequencer 一次应用整条消息,整批应用后才更新 mid 并调用策略。`L2Delta`(逐档)保留给需要逐档记录的读者与工具,feed 不产出。

### 执行(0x02xx)

```rust
pub struct OrderDecision {          // 0x0201,88 B,Output。每个意图一条,无论放行、被拒还是 Shadow 扣留(SHADOW 标志)
    pub trigger_seq: u64,           //  0 触发该意图的事件 seq
    pub decided_tsc: Tsc,           //  8 风控 + OMS 完成(decided_tsc,测量字段)
    pub px: Px,                     // 16 取整后的限价
    pub qty: Qty,                   // 24 取整后的数量(资产单位)
    pub group: GroupId,             // 32
    pub inst: InstId,               // 36
    pub slot: SlotId,               // 40 分配的槽;被拒/扣留 = 0
    pub gen: u32,                   // 44 槽代数
    pub target_slot: SlotId,        // 48 Cancel/Amend 的目标;LegLink 第二腿 = 第一腿的槽;否则 0
    pub reject: u32,                // 52 RejectBits(位号在 hft-types,判定规则在 hft-risk);0 = 放行
    pub strategy: StrategyId,       // 56 发出意图的策略实例;引擎自动产生的单(第二腿、默认减仓)= 该 group 所属实例
    pub venue: VenueId,             // 57
    pub account: AccountId,         // 58
    pub action: OrderAction,        // 59 New=1 Cancel=2 Amend=3 CancelAll=4 Query=5(Unknown 收敛)
    pub side: Side,                 // 60
    pub tif: Tif,                   // 61
    pub oflags: u16,                // 62 OrderFlags(位定义见下,唯一归属本 crate)
    pub reason: u32,                // 64 理由码:执行层订单 = hft-exec 登记的 ExecOrderKind 码;策略意图、OMS 产生的对冲单与冒烟单 = 0(见"已知限制")
    pub intent_no: u32,             // 68 该策略实例内的意图序号,从 0 递增(确定性)
    pub req_px: Px,                 // 72 取整前给出的价格(审计取整方向)
    pub link_role: u8,              // 80 LinkRole 线上码(类型归 hft-strategy-sdk,语义归 hft-oms):0 无 / 1 First / 2 Second / 3 SecondRetry / 4 HedgeFix / 5 Reverse
    pub _pad: [u8; 7],              // 81
}
```

#### OrderFlags

**`OrderDecision.oflags` 与 `GwCmd.oflags` 的位值只在这里定义**;hft-oms、hft-risk、hft-venue、venue-* 与 hft-sim 都链接本节,不另写位值。置位规则:`REDUCE_ONLY`/`RISK_REDUCING` 由 [hft-risk `classify()`](../hft-risk/README.md) 判定、OMS 拷入;其余由 [hft-oms](../hft-oms/README.md) 按意图来源填写。

```rust
#[repr(transparent)] pub struct OrderFlags(pub u16);
impl OrderFlags {
    pub const REDUCE_ONLY: Self   = 1 << 0;  // 填交易所 reduce-only 字段;只在 VenueCaps.reduce_only = true 的产品线出现(否则 venue 编码返回 Unsupported)
    pub const POST_ONLY: Self     = 1 << 1;
    pub const LINKED: Self        = 1 << 2;  // 属于一个 LegLink(第一腿或第二腿)
    pub const HEDGE_FIX: Self     = 1 << 3;  // "平掉多出的一腿"(人工 HedgeFix、自动有界平仓 AutoFix)
    pub const RISK_REDUCING: Self = 1 << 4;  // 风险递减:非 HARD 的 kill 下放行(gw 只看此位,判定见 hft-types kill 字位)
    pub const SMOKE: Self         = 1 << 5;  // Admin 冒烟单(SmokeOrder 命令建的槽;OMS 按 origin = Command 填写,语义与自动撤单归 hft-oms);gw 照常编码,不另做处理
    // bit 6..7:保留给线上标志,只追加
    // bit 8..15:保留不用,恒 0,永不出现在 OrderDecision/GwCmd 上(OMS 的槽内部状态位是独立的 SlotFlags,见 hft-oms)
    pub const WIRE_MASK: Self     = 0x00FF;
}
```

`encode_fixed::<OrderDecision>` 遇到 `oflags & !WIRE_MASK ≠ 0` → debug_assert(release 构建不检查;保留位不得上线);`GwCmd.oflags` 原样复制自已编码的 `OrderDecision`,因此同受此约束。optional 链上的 `SwapDecision.sflags` 是另一个字段(`RISK_REDUCING = 1`,u8),不与 `OrderFlags` 共用位值。

`link_role = SecondRetry`(3):开仓链接第二腿失败后的重试单;风控分类与 `Second` 相同(`RISK_REDUCING`,`reduce_only` 继承 `HedgePlan.reduce_only`),只用于重试计数与审计;`HedgeFix`(4)只用于"平掉多出的一腿"。

```rust
pub struct SendReport {             // 0x0202,48 B,Input(gw/sim-gw → sequencer;RingFull 由 sequencer 合成)
    pub decision_seq: u64,          //  0
    pub sent_tsc: Tsc,              //  8 测量字段
    pub slot: SlotId, pub gen: u32, // 16 本操作的槽(Place:新槽;其余:与 target 相同)
    pub target_slot: SlotId,        // 24 Cancel/Amend/Query 的目标槽;Place/CancelAll 为 0
    pub target_gen: u32,            // 28 目标槽的代数
    pub venue: VenueId, pub account: AccountId, pub conn: ConnId,   // sim-gw 写 conn = 0
    pub status: SendStatus,         // 35 Written=1 KillBlocked=2 RateLimited=3 WriteError=4 NoConn=5 EncodeError=6 RingFull=7 QueryUnsupported=8(op = Query 无法编码:槽镜像缺失且无 exch id,OMS 保持 Unknown 并转交 reconciler)
    pub op: GwOp,                   // 36 = GwCmd.op
    pub _pad: [u8; 3],              // 37
    pub os_errno: i32,              // 40
    pub _pad2: u32,                 // 44
}
// 非 Written 的 SendReport 只在 op = Place 时把槽迁到 Rejected;Cancel/Amend/Query/CancelAll 只清目标槽的 CANCEL_SENT/AMEND_SENT 并写 Note,目标槽状态不变(语义归 hft-oms)
pub struct PositionReport {         // 0x0204,56 B,Input;交易所推送,只用于校验
    pub venue: VenueId, pub account: AccountId, pub _pad: u16, pub inst: InstId,
    pub qty: Qty, pub entry_px: Px, pub upnl: Notional, pub liq_px: Px, pub ex_ts_ns: i64, pub rx_tsc: Tsc,
}
pub struct BalanceReport {          // 0x0205,40 B,Input
    pub venue: VenueId, pub account: AccountId, pub asset: AssetId, pub _pad: u32,
    pub free: Qty, pub locked: Qty, pub ex_ts_ns: i64, pub rx_tsc: Tsc,   // Qty 按该资产的 qty_exp
}
pub struct FundingSettle {          // 0x0206,56 B,Input(交易所推送)或 Output(synth = 1,sequencer 合成);账户收付的资金费
    pub venue: VenueId, pub account: AccountId, pub _pad: u16,
    pub inst: InstId,               //  4 INST_NONE(0)= 交易所只推了账户级合计(VenueCaps.funding_settle = BalanceOnlyPush)
    pub amount: Notional,           //  8 收入为正
    pub rate: Rate, pub pos_qty: Qty, pub settle_ts_ns: i64, pub rx_tsc: Tsc,   // inst = INST_NONE 时 rate、pos_qty 为 0
    pub synth: u8,                  // 48 1 = sequencer 按"结算时刻持仓 × mark × rate"合成(每 inst 一条;Output,rx_tsc 写 0);0 = 交易所推送
    pub _pad2: [u8; 7],
}                                   // 56 B(schema_version 2 追加 synth)
```

资金费的来源按 [hft-refdata `VenueCaps.funding_settle`](../hft-refdata/README.md#交易所能力表venuecaps):`PerSymbolPush` → 交易所推送带 inst 的实收,直接入账;`BalanceOnlyPush` → 交易所推送 `inst = INST_NONE` 的合计(Input,只用于校验),sequencer 合成每 inst 的 `synth = 1` 记录入账;`RateOnly` → 无推送,只合成。合成规则与校验差异的处理归 [hft-sequencer](../hft-sequencer/README.md) 与 [hft-ledger](../hft-ledger/README.md)。

`OrderEventKind`(event-model 列出)编码:`Ack=1 Reject=2 PartialFill=3 Fill=4 CancelAck=5 CancelReject=6 AmendAck=7 AmendReject=8 Expired=9 QueryResult=10 OutcomeUnknown=11 FeeOnly=12`。`FeeOnly`:只携带费用(延迟到达的手续费回报);OMS 在任何状态下只把 `fee` 交给 ledger,不迁移状态、不请求对账;`gen` 不符时按 `(venue, account, exch_order_id)` 入账;ledger 与 LegTracker 据此补净量差(手续费资产 = 基础资产时)。`OutcomeUnknown`:gw 收到 venue 协议判为"结果未知"的响应码(含未登记码,[hft-venue-proto](../hft-venue-proto/README.md))时产出,`slot/gen` 取自 `ReportOutcome.outcome_unknown`,`reject_code` = 原始码,价量与成交字段为 0;OMS 据此把该槽置 `Unknown` 并查询。`QueryResult` 必须带 `cum_filled` 与 `avg_px`(交易所给不出均价时由 venue 协议按成交明细合成,不得填 0 而 `cum_filled > 0`)。`OrderEvent.detail`(偏移 3,u8,由 venue 协议按其 `codes.rs` 填写)按 `what` 取值:`QueryResult` = 订单状态码,与 `ReconcileReport.order_status` 同码(`New=1 PartiallyFilled=2 Filled=3 Cancelled=4 Rejected=5 Expired=6 NotFound=7`);`CancelReject`/`AmendReject` = 拒绝分类 `OTHER=0 GONE=1`(GONE:订单已成交或不存在);其余 `what` 写 0。各分类如何处理归 [hft-oms](../hft-oms/README.md)。

```rust
pub struct ExternalFill {           // 0x0207,80 B(schema_version 2),Input;本账户上**不是本系统下的单**产生的成交:自身强平、ADL、网页/人工下单、交割
    pub venue: VenueId,             //  0
    pub account: AccountId,         //  1
    pub side: Side,                 //  2 本账户方向
    pub cause: ExternalCause,       //  3 Liquidation=1 Adl=2 Manual=3 Delivery=4 Unknown=5
    pub inst: InstId,               //  4
    pub qty: Qty,                   //  8 本次成交量(资产单位,> 0)
    pub px: Px,                     // 16 成交价
    pub fee: Qty,                   // 24 按 fee_asset 的 qty_exp,正 = 支出
    pub fee_asset: AssetId,         // 32
    pub _pad: [u8; 6],              // 34
    pub exch_order_id: u64,         // 40
    pub trade_id: u64,              // 48 交易所成交 id;缺失、为 0 或为负(强平/ADL 等)→ 0
    pub ex_ts_ns: i64,              // 56
    pub rx_tsc: Tsc,                // 64
    pub fill_seq: u64,              // 72 v2:trade_id = 0 时 = 该订单本笔之后的累计成交量 raw(hft-venue-proto 契约 22 的 fill_seq);trade_id > 0 时 0
}                                   // 80 B
```

去重键的唯一定义在 [hft-venue-proto 契约 22](../hft-venue-proto/README.md):`trade_id > 0` 时 `(inst, trade_id)`,否则 `(inst, exch_order_id, fill_seq)`;同一 (venue, account) 内去重。本体带齐这两种键的全部字段,下游(OMS、ledger)按同一键去重;读 v1 记录(72 B)时 `fill_seq` 补零。

`ExternalFill` 由 venue 协议按 `VenueCaps.own_liquidation_marker` 识别;无法识别 cause 的外来成交,以及无法匹配到本系统订单的成交在暂存超时后,记 `cause = Unknown`(暂存与超时规则归 [hft-venue](../hft-venue/README.md)/[hft-oms](../hft-oms/README.md));对账报告中带 `AUTO_EXTERNAL` 的仓位条目在该 `ReconcileReport` Input 内确定地合成一笔 `cause = Unknown` 的外来成交,**不另写 `ExternalFill` 记录**(合成函数归 [hft-ledger](../hft-ledger/README.md),冷读者经同一消费路径得到同一笔;调用点与去重键取值归 [hft-sequencer](../hft-sequencer/README.md))。ledger 入账(净量按 `VenueCaps.fee_asset_rule`)、LegTracker 更新、按 cause 告警(`Note{ExternalFill}` 的级别见登记表)、投递给该账户绑定的策略实例(`EventRef::External`);策略在 `external_deadline_ms` 内未处理时由 [hft-exec](../hft-exec/README.md) 按"平掉多出的一腿"默认处理——`cause = Manual`(人工在网页操作)除外:引擎不做默认处理,该 cell 置 `manual_hold`(抑制与解除规则归 hft-exec,写 `Note{ExecManualHold}`),由策略按自己的规则处理。各环节的语义归对应 crate,本 crate 只定义体。

```rust
pub struct MarginReportHdr {        // 0x0208 前缀(Var),64 B;其后 [MarginItem; n];Input
    pub venue: VenueId,             //  0
    pub account: AccountId,         //  1
    pub source: u8,                 //  2 Push=1 Poll=2(VenueCaps.margin_data)
    pub mflags: u8,                 //  3 MARGIN_CALL=1(交易所的追保推送)ITEMS_TRUNCATED=2(条目超上限被截断)
    pub n: u16,                     //  4 条目数,≤ 1024
    pub _pad: u16,                  //  6
    pub margin_balance: Notional,   //  8 保证金余额 = wallet + Σ未实现(USDT 1e-6;联合保证金下为交易所给出的 USD 等值)
    pub maint_margin: Notional,     // 16 维持保证金
    pub avail_margin: Notional,     // 24 可用保证金
    pub margin_ratio: Rate,         // 32 统一口径 = maint_margin / margin_balance;margin_balance ≤ 0 → Rate::MAX;各所换算方法写在 venue-*
    pub ex_ts_ns: i64,              // 40
    pub rx_tsc: Tsc,                // 48
    pub _rsv: u64,                  // 56
}
pub struct MarginItem {             // 16 B,每个有仓位的永续 inst 一条
    pub inst: InstId,               //  0
    pub adl_quantile: u8,           //  4 ADL 排位归一化到 0..=100(越大越先被减;各所档位的换算写在 venue-*);255 = 未知
    pub _pad: [u8; 3],              //  5
    pub liq_px: Px,                 //  8 强平价;0 = 未提供
}
```

保证金数据的消费(`MarginStress` 分档、数据过期 `margin_stale_ms` 视为 L1)归 [hft-risk](../hft-risk/README.md);权益视图里的保证金字段归 [hft-ledger](../hft-ledger/README.md)。

```rust
pub struct AccountConfigReportHdr { // 0x0209 前缀(Var),64 B;其后 [LeverageItem; n_lev];Input;gw 登录后查询一次,其后按推送/轮询更新
    pub venue: VenueId,             //  0
    pub account: AccountId,         //  1
    pub position_mode: u8,          //  2 = hft-refdata PositionMode 编码:NotApplicable=1 OneWay=2 Hedge=3
    pub margin_mode: u8,            //  3 = hft-refdata MarginMode 编码:NotApplicable=1 Cross=2 Isolated=3 Mixed=4(各 inst 模式不一致)
    pub multi_asset: u8,            //  4 = hft-refdata MultiAsset 编码:NotApplicable=1 Off=2 On=3
    pub source: u8,                 //  5 Startup=1 Push=2 Poll=3
    pub n_lev: u16,                 //  6 杠杆条目数,≤ 1024;现货 venue 恒 0
    pub mismatches: u32,            //  8 CfgMismatch 位:POS_MODE=1 MARGIN_MODE=2 MULTI_ASSET=4 FEE_RULE=8 ACCT_TYPE=16
                                    //    AUTO_BORROW=32(现货借币/自动借币开启,而 VenueCaps.required_spot_borrow = Some(false))
                                    //    KEY_PERM=64 UID=128 UNREADABLE=256;0 = 与要求一致;各位的判定方法写在 venue-*
    pub rflags: u16,                // 12 PARTIAL=1(只含变化的 inst,未列出的保持上一份报告的值)LEV_TRUNCATED=2(条目超上限被截断)
    pub uid_keys: u8,               // 14 本报告核对过 UID 的交易 key:SESSION=1(Ed25519 下单会话 key)REST=2(HMAC REST key);UID 位 = 其中任一把 ≠ venue_uid
    pub _pad: u8,                   // 15
    pub venue_uid: [u8; 24],        // 16 交易所按 key 返回的账户 UID(ASCII,右补 0;多把 key 一致时的值,不一致时为第一把不一致者);启动闸门 IDENTITY 用它比对
    pub ex_ts_ns: i64,              // 40
    pub rx_tsc: Tsc,                // 48
    pub _rsv: u64,                  // 56
}                                   // 64 B
pub struct LeverageItem {           // 16 B,每个永续 inst 一条
    pub inst: InstId,               //  0
    pub _pad: u32,                  //  4
    pub leverage: Rate,             //  8 交易所账户上该 inst 当前生效的杠杆倍数(Rate 1e-9 定点:20 倍 = 20_000_000_000);0 = 读不到
}
```

- `mismatches` 由 gw 侧的 venue 协议按 [hft-refdata](../hft-refdata/README.md) 的 `VenueCaps`(`required_*`、`fee_asset_rule` 等)与 `AccountMeta` 登记的 `venue_uid` 计算,是纯函数(同样的报文 + refdata 得到同样的位);`UNREADABLE` = 查询失败或字段读不出,按不一致处理。启动闸门位 `ACCOUNT_CFG`、`IDENTITY` 与运行中变化设的 `AccountCfg` cap 归 [hft-risk](../hft-risk/README.md);`IDENTITY` 要求该 venue 用到的每把交易 key 都已核对(`uid_keys` 覆盖全部),哪些 key 存在、谁去查归 [apps/hft-core](../../apps/hft-core/README.md) 与各 `venue-*`。
- **杠杆条目**:永续 venue 的报告对 refdata 中该 venue 每个 `Trading`/`ReduceOnly` 的永续 inst 给出一条实际杠杆(交易所按账户统一设置时每条同值;查询方法与分批归各 `venue-*`);`source = Push` 的杠杆变更推送可只带变化的 inst(`PARTIAL`)。杠杆是**观测值**,不是要求:refdata 不登记杠杆,运营在交易所设置(步骤见 [runbooks/launch](../../docs/runbooks/launch.md)),hft-risk 的 `LINK_FUNDS` 等按这里读到的值计算,`leverage = 0`(读不到)的处理归 [hft-risk](../hft-risk/README.md)。

```rust
pub struct ExecDecision {           // 0x020A,104 B,Output;hft-exec 有决定时写一条(无动作的推进不写)
    pub trigger_seq: u64,           //   0 触发该决定的事件 seq;Slice 之后同一事件内的 OrderDecision 与它同 trigger_seq
    pub set_seq: u64,               //   8 设置当前目标的那条 ExecDecision{TargetSet|TargetReplaced} 的 seq;无目标 0
    pub group: GroupId,             //  16
    pub tag: u32,                   //  20 = TargetIntent.tag
    pub owner: StrategyId,          //  24 cell 的拥有者(该账户绑定的实例)
    pub account: AccountId,         //  25
    pub action: u8,                 //  26 TargetSet=1 TargetReplaced=2 TargetCleared=3 TargetRejected=4 Slice=5 State=6 HedgeFix=7
                                    //     ResidualTrim=8 ExternalSeen=9 ExternalDefault=10 ForceStep=11 CmdHedgeFix=12 ChainLegExcess=13
                                    //     AutoFix=14(Broken 持续 broken_auto_fix_ms 后的一次有界平仓)
    pub algo: u8,                   //  27 TakerSlice=1 PairOnFill=2 Simultaneous=3 MakerFirst=4 Engine=5;无 = 0
    pub origin: u8,                 //  28 TargetOrigin(码表见下)
    pub reason: u8,                 //  29 TargetReason:Rebalance=1 Exit=2 MarginStress=3 Hard=4 Manual=5
    pub urgency: u8,                //  30 Passive=1 Normal=2 Urgent=3
    pub state_from: u8,             //  31 TargetState:Idle=1 Working=2 Stuck=3 Broken=4
    pub state_to: u8,               //  32
    pub leg_ix: u8,                 //  33 本笔切片/动作的腿;无 = 0xFF
    pub first_leg_ix: u8,           //  34 PairOnFill 选定的先手腿;无 = 0xFF
    pub side: u8,                   //  35 Side 编码;无 = 0
    pub eflags: u8,                 //  36 SHADOW=1(目标在有效模式 Shadow 下被接受)CLEARED_BY_ENGINE=2
    pub _pad: [u8; 3],              //  37
    pub detail: u32,                //  40 按 action 解释的子码(TargetReject、Stuck、Broken、AutoFix、ForceStep 各子码,含 SerialFallback、RiskReject);
                                    //     全部数值与 u32 打包规则的唯一登记处是 hft-exec
    pub gate: u32,                  //  44 门控拒绝时的 RejectBits;否则 0
    pub exec_order_id: u32,         //  48 ExecOrderId { cix: u16, n: u16 }
    pub _pad2: u32,                 //  52
    pub qty: Qty,                   //  56 本笔切片量(资产单位,正);无 0
    pub px: Px,                     //  64 本笔限价;无 0
    pub target_qty: Qty,            //  72 该腿目标净量(带符号)
    pub pos_qty: Qty,               //  80 该腿已成净量
    pub inflight_qty: Qty,          //  88 该腿即时在途量(口径归 hft-oms)
    pub depth_qty: Qty,             //  96 决策时弱侧可见深度
}                                   // 104 B
```

字段的取值口径与各码的语义(状态机、Stuck/Broken 的判定、腿顺序规则)归 [hft-exec](../hft-exec/README.md);`action`、`algo`、`reason`、`urgency`、`TargetState` 与下表 `TargetOrigin` 的线上码在本 crate 登记(只追加;`TargetState`/`TargetOrigin` 是 [hft-strategy-sdk](../hft-strategy-sdk/README.md) 的视图枚举),`detail` 子码(`StuckReason`、`BrokenReason`、`TargetReject` 等)的数值与打包只在 hft-exec 登记。

**`TargetOrigin` 线上码**(`ExecDecision.origin`;SDK 视图形状 `Strategy | Engine(EngineOrigin)`,一一对应;只追加):

| 码 | SDK 值 | 含义(语义归 hft-exec) |
|---|---|---|
| 1 | `Strategy` | 策略输出的目标 |
| 2 | `Engine(MarginStress)` | 保证金 L3 强制减仓的引擎目标 |
| 3 | `Engine(External)` | 外来成交未被策略处理时的默认处理 |
| 4 | `Engine(Command)` | 人工命令(`HedgeFix` 等)产生的引擎目标 |
| 5 | `Engine(KillFlatten)` | `Kill{flatten}` 的清仓 |
| 6 | `Engine(AutoFix)` | `Broken` 持续 `broken_auto_fix_ms` 后的一次自动有界平仓 |

线上码的代码形式:`#[repr(u8)] pub enum TargetOriginWire { Strategy = 1, EngineMarginStress = 2, EngineExternal = 3, EngineCommand = 4, EngineKillFlatten = 5, EngineAutoFix = 6 }`,`TryFrom<u8>` 对 0 与越界值返回 `DecodeErr::InvalidEnum`;与 SDK 视图的互转在 SDK 一侧。

### 链上(0x03xx)

**optional**:本段编号与布局**保留**(外部 venue 协议在架构上保留,链上策略接入时启用);ccex 部署中没有生产者,读者照常解码。

```rust
pub struct ChainHead {              // 0x0301,80 B,Input;每个新的 node_head 一条;chain-core 如何填写见 external-venue-protocol §3.2
    pub chain: VenueId, pub source: u8 /* 首达来源:Node=1 BlockStream=2 Rpc=3 */,
    pub hflags: u8 /* FINALITY_STALLED=1 NODE_BEHIND=2 */, pub _pad: [u8; 5],
    pub block: u64 /* = node_head */, pub block_ts_ns: i64, pub observed_ts_ns: i64 /* 首达 UTC */, pub rx_tsc: Tsc, pub hash: [u8; 32],
    pub finalized: u64,             // 72 当前终局高度
}
pub struct PoolQuote {              // 0x0302,96 B,Input;每 (路由, 评估规模) 每块至多一条
    pub quote_id: u64,              //  0 chain-core 内单调
    pub block: u64,                 //  8 报价所用状态的块号(node_head)
    pub group: GroupId,             // 16
    pub route: u32,                 // 20 chain-core 路由表下标(路由表归 hft-chain-evm)
    pub chain: VenueId,             // 24
    pub level: u8,                  // 25 L0=1 L1=2
    pub qflags: u8,                 // 26 STALE=1 HOOK_POOL=2 MULTIHOP=4 INDICATIVE=8 NO_LIQ=16
    pub _pad: [u8; 5],              // 27
    pub size: Qty,                  // 32 评估规模(代币资产单位)
    pub buy_px: Px,                 // 40 以 size 买入的均价,USDT/代币、资产 px_exp;含池费,向上取整
    pub sell_px: Px,                // 48 以 size 卖出的均价,向下取整
    pub depth: Notional,            // 56 深度估计
    pub gas_cost: Notional,         // 64 一次 swap 的 gas 折算(USDT,不含出价)
    pub fee: Rate,                  // 72 路由总费率
    pub computed_ts_ns: i64,        // 80 chain-core UTC
    pub rx_tsc: Tsc,                // 88 hft-core ext 线程收到时填
}
pub struct SwapDecision {           // 0x0303,128 B,Output;放行的经 chain-out 环交给 ext 线程,投影为 external-venue 的 SwapIntentMsg
    pub intent_id: u64,             //   0 (engine_epoch << 32) | n,n = gen × N_chain + slot(hft-oms);TxLifecycle.intent_id 引用它
    pub quote_id: u64,              //   8 所依据的 PoolQuote
    pub trigger_seq: u64,           //  16
    pub decided_tsc: Tsc,           //  24 测量字段
    pub amount_in: Qty,             //  32 ExactIn:输入量(输入资产 qty_exp);ExactOut:0
    pub min_out: Qty,               //  40 ExactIn:由信号价反推的硬下限;chain-core 只能收紧
    pub amount_out: Qty,            //  48 ExactOut:目标输出量;ExactIn:0
    pub max_in: Qty,                //  56 ExactOut:输入上限;ExactIn:0
    pub limit_px: Px,               //  64 同一限价的价格形式(风控价格带、诊断)
    pub edge: Notional,             //  72 出价前预期净利(USDT 1e-6),chain-core 按比例出价
    pub max_bribe: Notional,        //  80 出价上限
    pub group: GroupId,             //  88
    pub route: u32,                 //  92 = PoolQuote.route
    pub reject: u32,                //  96 RejectBits;0 = 放行
    pub hedge_inst: InstId,         // 100 chain-first 对冲腿;0 = 无
    pub strategy: StrategyId, pub account: AccountId, pub chain: VenueId, pub side: Side /* Buy = 计价资产→代币 */, // 104
    pub valid_blocks: u8, pub exact: u8 /* 0 ExactIn 1 ExactOut */, pub hedge_side: Side /* hedge_inst = 0 时写 Buy,读者忽略 */, pub hedge_px_mode: u8 /* Cross=1 Mid=2 LimitBp=3 */, // 108
    pub hedge_limit: Rate,          // 112 LimitBp 模式下允许的偏离
    pub sflags: u8, pub _pad: [u8; 7], // 120 RISK_REDUCING=1
}
```

`TxState` 编码(`TxLifecycle.state`):`Sent=1 IncludedOk=2 IncludedReverted=3 Dropped=4 Unknown=5 Finalized=6`。状态机行为在 [hft-chain-evm](../hft-chain-evm/README.md);线格式枚举在本 crate,因为 L0 不能依赖链上 crate。`SwapDecision` 的字段与 [hft-strategy-sdk](../hft-strategy-sdk/README.md) `chain` 模块的 `SwapIntent` 一一对应(`route_id` → `route`、`signal_seq` → `trigger_seq`,其余同名),放行后投影为 external-venue 的 `SwapIntentMsg`。

**chain journal 专用 kind(`0x0310–0x033F`)、`ext`(外部 venue 协议帧)与 `relay`(chain-grpc-relay 帧)**在 [hft-ext-proto](../hft-ext-proto/README.md) 定义(optional)。本 crate 的 `KINDS` 注册表登记该编号段归 hft-ext-proto;`gen-schema` 同时读取 hft-ext-proto 的注册表(该 crate 存在时),编号冲突即失败。

### 控制与系统(0x04xx、0x05xx)

```rust
pub struct CmdResult {              // 0x0402,144 B(schema_version 3),Output;只有到达 sequencer 的命令才有(gateway 拒绝的命令只进审计 Note)
    pub cmd_id: [u8; 16],           //   0 = CmdHdr.cmd_id(UUID)
    pub cmd_seq: u64,               //  16 对应 ControlCmd 记录的 seq
    pub cmd: CmdKind, pub status: u8, pub _pad: u8, // 24 status = control-plane 结果码(码表见 control-plane 结果码:0 Applied、1 NoOp … 40 Internal)
    pub detail: u32,                //  28 = detail 码 | arg << 16;码表与 arg 的含义归 control-plane(结果码一节的 detail 码表)
    pub engine_epoch: u32, pub effect_count: u32, // 32 effect_count:如撤单数
    pub refdata_version: u64,       //  40 执行后的当前版本(control-plane:每个结果回带各配置版本)
    pub risk_version: u64,          //  48
    pub params_version: u64,        //  56 命令目标 (strategy, account) 的参数版本;无目标为 0
    pub ctl_state_version: u64,     //  64 执行后的控制状态版本:每次 SetMode/Kill/Resume/cap 变化 +1;确认请求以 expect_state_version 绑定它
    pub rflags: u32,                //  72 PREVIEW=1(本结果是预览)NEEDS_CONFIRM=2(该命令需二次确认)PREVIEW_TTL=4(确认绑定本预览的 cmd_hash,
                                    //     须由同一 issuer 在 60 s 内完成,不比对 ctl_state_version:Viewer 的 Kill)
    pub _pad2: u32,                 //  76
    pub preview: CmdPreview,        //  80 只在 rflags.PREVIEW 时非零(全零 = 未提供)
}                                   // 144 B
pub struct CmdPreview {             // 64 B;sequencer(hft-risk::mode::preview)填写,ops_bot/hft-cfg 渲染
    pub target_account: AccountId, pub other_account: AccountId,   // other_account = 请求附带的对照账户(ops_bot 当前视图账户);0 = 无
    pub target_mode: u8, pub other_mode: u8,   // 各账户受影响 cell 的有效模式最严值(RunMode 编码;0 = 无 cell)
    pub target_kind: u8,                       // 目标账户类型:Live=1 Sim=2(hft-refdata AccountKind 编码)
    pub _pad: [u8; 3],
    pub affected_cells: u32, pub open_orders: u32, // 目标作用域内受影响的 cell 数与挂单数
    pub active_caps: u32,           // 目标作用域内生效 cap 条数
    pub cap_sources: u32,           // 生效 cap 的来源位图(bit = CapSource 编号)
    pub reset_groups: u32,          // v3:SetParams 将重置状态的 group 数(结构字段变化或指纹变化;判定归 hft-strategy-sdk);其他命令 0
    pub no_open_ms: u32,            // v3:SetParams 之后预计不能开仓的时长(ms,估算归 hft-strategy-sdk);0 = 无影响或不适用
    pub wo_venue: VenueId,          // 32 占用保留字节:WriteOffResidual 预览中"多出那一腿"的 venue;其他命令与无多出腿时 0
    pub wo_leg_ix: u8,              // 33 该腿在 GroupSpec 中的下标
    pub _pad2: u16,                 // 34
    pub wo_inst: InstId,            // 36 该腿的 inst
    pub wo_qty: Qty,                // 40 将核销的已成交裸量(带符号,该 inst 基础资产的 qty_exp;判定归 hft-oms 残余核销)
    pub _rsv: [u64; 2],             // 48
}                                   // 64 B
pub struct ReconcileReport {        // 0x0404,104 B(schema_version 3),Input;reconciler 经 ctl-gateway → ctl 环直接到达(不是 ControlCmd 变体);同一 report_id 的 count 条连续,末条到达时整组生效
    pub report_id: u64,             //   0
    pub venue: VenueId, pub account: AccountId, // 8
    pub item: u8,                   //  10 Position=1 Balance=2 OpenOrder=3 UnknownResolved=4 Summary=5 Funding=6(交易所逐笔资金费,见下)
    pub severity: u8,               //  11 Ignore=1 Warn=2(3 退役,永不复用);reconciler 只报差异不分级,分级归 hft-risk
    pub inst: InstId,               //  12
    pub asset: AssetId, pub idx: u16, pub count: u16, pub _pad: u16,   // 16 一次对账 count 条,idx 从 0
    pub engine_qty: Qty,            //  24
    pub exch_qty: Qty,              //  32
    pub diff: Notional,             //  40 Funding:= exch_amount − engine_amount
    pub exch_ts_ns: i64,            //  48 Funding:该笔资金费的结算时刻 settle_ts_ns
    pub slot: SlotId,               //  56 OpenOrder/UnknownResolved
    pub order_status: u8,           //  60 UnknownResolved:New=1 PartiallyFilled=2 Filled=3 Cancelled=4 Rejected=5 Expired=6 NotFound=7;其他项 0
    pub history_checked: u8,        //  61 UnknownResolved{NotFound}:1 = 已查过成交历史且无该订单、窗口内也无未归属的成交;0 = 未查、查询失败或有未归属成交。
                                    //     NotFound 能否不经成交历史直接定终态由 hft-oms 按 venue 的 stale_guard 与槽是否在链接内判定
    pub _pad2: [u8; 2],             //  62
    pub view_seq: u64,              //  64 reconciler 比对所依据的引擎视图的 seq(S0);AdoptExchange 用它判断报告是否过期
    pub gen: u32,                   //  72 OpenOrder/UnknownResolved:槽代数
    pub epoch: u16,                 //  76 OpenOrder/UnknownResolved:engine_epoch;OMS 仅在 (epoch, slot, gen) 完全匹配且槽仍为 Unknown/VERIFY 时应用
    pub _pad3: u16,                 //  78
    pub exch_amount: Notional,      //  80 v2:Funding = 交易所资金费流水中该 (account, venue, inst, settle_ts) 的金额(收入为正);其他项 0
    pub engine_amount: Notional,    //  88 v2:Funding = 引擎在 view_seq 时对同一笔已入账的金额(含合成记录);其他项 0
    pub sflags: u16,                //  96 v3,按 item 解释(各位的填法归 reconciler):
                                    //     Summary:USERSTREAM_DOWN=1(该轮对账窗口内该 (venue, account) 的 UserStream 断开过)TRUNCATED=2(条目超上限被截断)
                                    //       HISTORY_UNCHECKED=4(该轮在某个 Unknown 槽的成交历史窗口内见到未归属成交);
                                    //     Position:AUTO_EXTERNAL=8(置位条件归 reconciler;处理见下文);
                                    //     其他项 0
    pub skipped_inflight: u16,      //  98 v3,只用于 Summary:该轮因在途(下单或成交未落定)而跳过比对的条目数;其他项 0
    pub _pad4: u32,                 // 100
}                                   // 104 B
pub struct LatencySample {          // 0x0505,136 B(schema_version 3),Meta;八个点与 sw_rx 的名称与语义以 latency-budget §1 为准
    pub trigger_seq: u64,           //   0 触发事件的 seq(其 kind 见 trigger_kind)
    pub ack_seq: u64,               //   8 第一条交易所回报的 seq
    pub slot: SlotId, pub gen: u32, //  16
    pub venue: VenueId, pub account: AccountId, pub strategy: StrategyId, pub _pad: u8, // 24
    pub src: u32,                   //  28 每点 2 bit(点 0..7):0 缺失 1 HW 2 SW 3 TSC
    pub ex_ts_ns: i64,              //  32 点 0 ex_ts
    pub ex_send_ns: i64,            //  40 点 1 ex_send
    pub hw_rx_ns: i64,              //  48 点 2 hw_rx
    pub rx_tsc: Tsc,                //  56 点 3
    pub proc_tsc: Tsc,              //  64 点 4
    pub decided_tsc: Tsc,           //  72 点 5
    pub sent_tsc: Tsc,              //  80 点 6
    pub ack_hw_rx_ns: i64,          //  88 点 7 ack_hw_rx
    pub ack_rx_tsc: Tsc,            //  96 附加:回报的 rx_tsc(纯 TSC 的往返)
    pub ack_ex_ts_ns: i64,          // 104 附加:交易所回报时间
    pub sw_rx_ns: i64,              // 112 v2:触发帧的内核软件接收戳 sw_rx(行情触发取 MarketHdr.sw_rx_ns);触发事件不带时 0
    pub link_role: u8,              // 120 v2:= 该单 OrderDecision.link_role(0 无 1 First 2 Second 3 SecondRetry 4 HedgeFix 5 Reverse)
    pub action: u8,                 // 121 占用保留字节:样本对应的订单动作 0 = New 1 = Cancel 2 = Amend(旧记录读出 0 = New);样本合成归 hft-oms;
                                    //     encode_fixed 遇到 action > 2 → debug_assert
    pub _pad2: [u8; 6],             // 122
    pub trigger_kind: u16,          // 128 v3:触发事件的 EventKind 编号(event-model kind 编号表;如 Bbo = 0x0101、OrderEvent = 0x0203);0 = 未提供(v2 记录)
    pub _pad3: [u8; 6],             // 130
}                                   // 136 B
#[repr(u8)] pub enum NoteLevel  { P0 = 1, P1 = 2, P2 = 3, Info = 4 }   // 告警级别的唯一编码;SDK、ops_bot、hft-telemetry、watchtower 用同名同值
#[repr(u8)] pub enum NoteSource { Sequencer = 1, Strategy = 2, Risk = 3, Oms = 4, Venue = 5, CtlAudit = 6, Measure = 7,
                                  Housekeeping = 8, Ledger = 9, Exec = 10, Boot = 11, Advisor = 12 }   // 只追加
pub struct Note {                   // 0x0506,80 B;热路径唯一的"日志",角色按 source 决定(见下)
    pub code: u32,                  //  0 = source << 24 | n:n 由 source 所在 crate 的 const 表给出(名字以下方登记表为准);Strategy 源由 StrategyDesc.notes 给出
    pub level: NoteLevel,           //  4 本条的级别(默认值见登记表;按档位变化的码在登记表注明)
    pub source: NoteSource,         //  5
    pub strategy: StrategyId,       //  6 Strategy 源 = 发出的实例;其他源 = 相关实例或 0
    pub _pad: u8,                   //  7
    pub group: GroupId,             //  8 相关 group;无 = GROUP_NONE
    pub inst: InstId,               // 12 相关 inst;无 = INST_NONE
    pub v: [u64; 8],                // 16 含义见登记表或各 source 的码表
}                                   // 80 B
```

**`ReconcileReport{item = Funding}`**:reconciler 对最近 48 h 内每一笔与引擎入账不一致的资金费结算发一条(交易所有而引擎没有时 `engine_amount = 0`,反之 `exch_amount = 0`),字段为 `(account, venue, inst, exch_ts_ns = settle_ts, exch_amount, engine_amount, diff)`。这类条目**不参与**启动闸门 `RECONCILED`、不触发 `Recon` cap、不参与 HARD 解除条件;它们只用于告警(watchtower 规则)与账本自动校正(hft-ledger 对结算时刻早于 `funding_recon_lag_ms` 的差额写 `AdjustDelta::Funding` 与 `Note{FundingCorrected}`)。闸门与 cap 的规则归 [hft-risk](../hft-risk/README.md),校正规则归 [hft-ledger](../hft-ledger/README.md),比对方法归 [reconciler](../../apps/reconciler/README.md)。

**`ReconcileReport{item = Summary}`**:每 (venue, account) 每轮一条;`sflags` 与 `skipped_inflight` 的填法归 reconciler,`RECONCILED` 等闸门如何使用它们归 hft-risk。读旧记录:v1(80 B)两个金额补零;v2(96 B)`sflags`、`skipped_inflight` 补零(= 无标志、无跳过)。**`AUTO_EXTERNAL`**(`item = Position` 的 `sflags` 位):置位条件归 [reconciler](../../apps/reconciler/README.md);sequencer 收齐该组、复核前置条件后,在这条 Input 内对带此位的条目合成外来成交(不另写记录,见 [`ExternalFill`](#执行0x02xx))并报 P0;现货差异与反向差异从不带此位。

模拟账户的 `LatencySample` 照常产生(点 1–2、7 缺失,`src` 对应位为 0),watchtower 按 `AccountMeta.kind` 分开统计,不与实盘直方图混算。样本按**触发事件的 kind**(`trigger_kind`)分两类,点 2/3(`hw_rx_ns`/`rx_tsc`)都取触发帧,点 6 都是该单(撤单与改单样本为该次操作)的 `sent_tsc`:

- 行情触发(`Bbo`、`L2Snapshot`、`L2Batch`、`Trade` 等):`tick_to_trade` 等分段的样本。
- **回报触发**(`trigger_kind = OrderEvent`:由某条 Ack/Fill/Cancel 等回报在同一事件内引出的下一笔订单、撤单或改单):`rep_to_order` 的样本;其中 `link_role = Second`(由第一腿成交引出的第二腿,不含 `SecondRetry`)的子集是 `hedge_host` 的样本。

`action` 区分新单(0)、撤单(1)与改单(2):`order_rtt` 只统计 `action = New`,`rep_to_order` 的样本包含三种 action。分段公式与分组唯一归属 [latency-budget](../../docs/latency-budget.md),样本的合成与写入时机归 [hft-oms](../hft-oms/README.md)。读 v2(128 B)记录:`trigger_kind` 补零,读者按 `trigger_seq` 指向的记录头取 kind。

#### Note 来源与码登记表

| `source` | 值 | 产生者 / 到达路径 | 回放角色 |
|---|---|---|---|
| `Sequencer` | 1 | sequencer 自身 | Output,比对 |
| `Strategy` | 2 | 策略经 `ctx.note` | Output,比对 |
| `Risk` | 3 | hft-risk | Output,比对 |
| `Oms` | 4 | hft-oms | Output,比对 |
| `Venue` | 5 | feed/gw 经 feed/rep 环 | 记录,不驱动状态;回放不喂不比 |
| `CtlAudit` | 6 | ctl-gateway 写进 ctl 环(与 `ControlCmd` 同一条环、同一 `ByteSpsc` 记录格式);sequencer 赋 seq 原样记录 | 同 `Venue` |
| `Measure` | 7 | sequencer 的测量输出 | Meta:不驱动状态、不比对 |
| `Housekeeping` | 8 | housekeeping 经 hk 环 | 同 `Venue` |
| `Ledger` | 9 | hft-ledger(sequencer 线程内,随 `PnlDelta` 输出) | Output,比对 |
| `Exec` | 10 | hft-exec(sequencer 线程内) | Output,比对 |
| `Boot` | 11 | hft-core 启动与恢复路径:启动步 1–5 经 hk 环(只在 housekeeping 线程启动之前写,之后 hk 环的唯一写者是 housekeeping 线程)、步 6–8 由 live driver 直接写(不经环;步骤归 [apps/hft-core](../../apps/hft-core/README.md)) | Meta:不驱动状态、不比对 |
| `Advisor` | 12 | advisor 线程经 advice 环(`AdviceMsg::Note`,见[环元素类型](#环元素类型)) | 同 `Venue` |

**引擎 Note 码登记表**(名字的唯一登记处;正文里出现的 `Note{X}` 必须是本表的名字或某个策略 `StrategyDesc.notes` 里的名字)。码名按来源加前缀:`Oms` 源以 `Oms` 开头、`Exec` 源以 `Exec` 开头;`Venue`、`Measure`、`Housekeeping` 源的计数类码分别以 `Venue`、`Measure`、`Hk` 开头,由 hft-venue、hft-sequencer、apps/hft-core 在各自的 `const` 表登记(不在正文里按名引用,不进本表;gen-schema 只核对其前缀)。**分工**:本表登记码名、source 与默认级别;数值 `n` 与 `v[]` 的含义只在 source 所在 crate 的 `const` 表(本表的 `v[]` 列只写跨 crate 读者需要的那几项);`gen-schema` 检查 `const` 表与本表的名字一一对应(`Venue`、`Measure`、`Housekeeping` 源除外,见上)、同一 source 内 `n` 唯一。级别是默认值,`Note.level` 是实际值;"按条件"的码由产出方按注明的条件取级别。**每个 source 的 `n = 0` 保留、不分配**(`code` 因此从不为 0);gen-schema 拒绝 `n = 0` 的 const 表项,策略 `NoteDesc.code = 0` 由 SDK 校验拒绝。

| 码名 | source | 级别 | 含义与 `v[]`(语义归) |
|---|---|---|---|
| `EpochStart` | Sequencer | Info | 每次进程启动恰好一条,是处理 [`EngineClock`](#engineclock0x050ainput) 的同一事件的输出;v[0] = engine_epoch,v[1] = `EngineClock.base_engine_ns`,v[2] = 该 `EngineClock` 记录的 seq;SDK `Now.boot_seq` = 本条的 seq;reconciler 据此立即对账 |
| `Conflated` | Sequencer | Info | BBO 积压合并计数(hft-sequencer) |
| `DrainTimeout` | Sequencer | 按条件:有未结束的链接或未到达的引擎减仓目标 P0,否则 P1 | Drain 超时,`v[]` 列出未完成的账户(等待条件、定级与 `v[]` 归 hft-sequencer;退出码归 apps/hft-core) |
| `RecoverMismatch` | Sequencer | P0 | 恢复时决策不一致:v[0] = venue \| account << 8,v[1] = 不一致记录的 seq |
| `AdviceMisrouted` | Sequencer | P2 | Advice 的实例未知、`len` 越界、`group = GROUP_ALL`、group 的高 8 位与实例不符,或账户未绑定到该实例(判定归 [hft-sequencer Advice 路由](../hft-sequencer/README.md#advice-路由)) |
| `AdviceStale` | Sequencer | P2 | Advice 的 group 已被移除,或其 `GroupSpec::fingerprint()` 在 `basis_seq` 之后变化过(`basis_seq < spec_seq`),被丢弃(hft-sequencer) |
| `StartupGate` | Sequencer | Info | 启动闸门位变化(hft-risk `ModeOutcome.gate_changes` 给出,sequencer 写):v[0] = account,v[1] = 旧 gate_bits,v[2] = 新 gate_bits,v[3] = released |
| `ShadowDeferred` | Sequencer | Info | v[0] = strategy \| account << 8,v[1] = group;v[2] = 0 推迟 / 1 已落到 Shadow(hft-risk `ModeOutcome` 给出,sequencer 写) |
| `IdentityMismatch` | Sequencer | P0 | UID 核验不符:v[0] = venue \| account << 8;hft-risk 置账户级 HARD 并发起 Drain(sequencer 按 `ModeOutcome` 写本条),hft-core 以退出码 2 结束 |
| `RefdataRecheckFailed` | Sequencer | P1 | `ReplaceRefdata` 的 Drain 末二次复核不通过(复核由 apps/hft-core 执行、结果作为 Input 交 sequencer):sequencer 撤销该 blob 的待生效引用并写本条,旧 refdata 继续生效,进程以退出码 0 结束(`v[]` 归 hft-sequencer) |
| `RateLimitHit` | Risk | P2 | 限速命中(`v[]` 归 [hft-risk](../hft-risk/README.md)) |
| `CapSet` / `CapCleared` | Risk | Info | v[0..6] = 该条 [`CapRecord`](#cap-记录) 的 48 字节;告警级别由 [watchtower](../../apps/watchtower/README.md) 按 cap 来源定 |
| `MarginLevel` | Risk | 按档位:L2 P1、L3 P0 | v = [account \| venue << 8, level, liq_dist, ratio, deadline_ens];档位语义归 hft-risk |
| `RecoveryHeldSim` | Risk | P1 | 只含 Sim 账户的实例非干净重启后保留 `Recovery` cap(有 `RecoverMismatch` 或毒丸;解除条件与 `v[]` 归 hft-risk,实例判定归 apps/hft-core) |
| `IdentityUnverified` | Risk | P1 | `uid_policy = Unverifiable` 的账户在该 venue 未核验 UID 而放行 `IDENTITY`:v[0] = venue \| account << 8(放行条件归 hft-risk;`uid_policy` 归 [hft-refdata](../hft-refdata/README.md#数据结构)) |
| `OmsTransition` | Oms | Info | 每次槽迁移一条(`v[]` 归 hft-oms) |
| `OmsUnknown` / `OmsUnknownStuck` | Oms | P1(超时升 P0)/ P0 | 槽进入 Unknown / 查询次数耗尽 |
| `OmsVenueReady` | Oms | Info | 最后一个 Unknown 收敛且连接 Up |
| `OmsStaleGenFill`、`OmsLateFill`、`OmsOverfill` | Oms | P0 | 代数不符的成交、墓碑命中的迟到成交、超量成交 |
| `OmsForeignOrder`、`OmsForeignFillViaOrderEvent` | Oms | P1 | 外来单回报;本应以 `ExternalFill` 到达的外来成交 |
| `OmsExtDedup` | Oms | P1 | 槽成交命中已按外来成交入账的部分 |
| `OmsForeignFill` | Oms | Info | 防御路径或 `StaleGen` 合成的外来成交(每条 `ForeignFill` 一条,Output):冷读者在同一事件内据此构造 `ExternalFill{cause = Unknown}`,经 [hft-ledger 外部成交](../hft-ledger/README.md#外部成交externalfill) 的消费路径入账;`v[]` 归 [hft-oms 回报匹配与成交增量](../hft-oms/README.md#回报匹配与成交增量) |
| `OmsHedgeBroken` | Oms | P0 | 链接 Broken |
| `OmsHedgePxStale`、`OmsHedgePxGated`、`OmsHedgeSlipCapped`、`OmsHedgeNakedCapped` | Oms | P1 | 对冲定价门控与截量 |
| `OmsResumeRetry` | Oms | P1 | 闸门放行后对 `RESUME_HOLD` 链接续接一次 |
| `OmsResumeTimeout` | Oms | 按条件:有敞口的链接 P0,零敞口的链接 P1 | `RESUME_HOLD` 超过 `resume_hold_max_ms` 未续接:有敞口的链接 Broken,零敞口的只清 `RESUME_HOLD`(hft-oms、hft-risk) |
| `OmsFillPxEstimated`、`OmsFeeUnattributed` | Oms | P1 | 均价按委托价估计;费用无法归属到槽 |
| `OmsCancelNotSent` | Oms | P2 | 撤单/改单确定未写出(撤单义务仍在时由 OMS 重发) |
| `OmsNeedsRestQuery` | Oms | Info | 请 reconciler 经 REST 查询(含成交历史) |
| `OmsAnomaly` | Oms | 按条件:冒烟单成交(`SmokeFilled`)P0,其余 P1 | 其余 OMS 异常(种类与 `v[]` 归 hft-oms) |
| `RoundClosed`、`Adjusted`、`BaselineSet`、`BalanceInit`、`SimReset` | Ledger | Info | `RoundClosed`:带结束轮次的 group(`Note.group`),不带策略 `tag`(内容归 hft-ledger);`Adjusted`:`AdoptExchange` 采纳的调整,v[0] = account \| venue << 8,v[1] = inst 或 asset,v[2] = report_id,v[3] = qty,v[4] = px,v[5] = 调整计入的已实现 Notional;其余 `v[]` 归 hft-ledger |
| `ResidualWrittenOff` | Ledger | P1 | `WriteOffResidual` 把某 group 多出的那一腿(单腿)的已成交裸量移到 unassigned(`AdjustDelta::WriteOff`;`v[]` 归 hft-ledger) |
| `ExternalFill` | Ledger | 按 `cause`:`Manual` P1,其余 P0 | 外来成交入账(hft-ledger) |
| `LedgerAutoHeldOffset` | Ledger | P0 | 同 inst、同方向的 `ExternalFill{Adl \| Liquidation \| Unknown}` 先抵扣对账自动入账留下的 `auto_held`,只入账超出部分(抵扣规则与 `v[]` 归 hft-ledger) |
| `BalanceDrift`、`InvShortfall`、`NetQtyMismatch`、`FundingMismatch` | Ledger | P1 | 账本与推送/库存/净量/资金费不一致(hft-ledger) |
| `FundingCorrected` | Ledger | P1 | 按 `ReconcileReport{Funding}` 自动校正一笔资金费,入账为 `AdjustDelta::Funding` |
| `ChainReorg` / `ChainReorgNaked` | Ledger | P1 / P0 | optional 链上腿的重组 |
| `ExecBroken` | Exec | P0 | cell 进入 Broken |
| `ExecExternalDefault` | Exec | 按条件:发单 P0,裸量在阈值内不发单 P1 | 外来成交的默认处理(条件归 [hft-exec](../hft-exec/README.md)) |
| `ExecAutoFixFailed` | Exec | P0 | 自动有界平仓失败 |
| `ExecStuck`、`ExecForceStep`、`ExecAutoFixDone` | Exec | P1 | 目标卡住;L3/flatten 的一步;自动有界平仓完成 |
| `ExecSerialFallback` | Exec | P0 | `reason ∈ {MarginStress, Hard}` 的 cell 退回永续先的串行路径(原因码见 hft-exec `ExecDecision.detail` 表) |
| `ExecEngineOrderRejected` | Exec | P0 | Flatten/ForceStep 等引擎单被风控 #5/#6 拒绝,按执行失败处理(hft-exec) |
| `ExecManualExternal` | Exec | 按条件:P1;同一调用刚写了置位的 `ExecManualHold` 时 Info | `ExternalFill{Manual}`:引擎不做默认处理,交策略处理(hft-exec) |
| `ExecManualHold` | Exec | 按条件:置位 P1,解除 Info | cell 的 `manual_hold` 置位或解除(置位条件、被抑制的动作、解除方式与 `v[]` 归 hft-exec) |
| `ExecUngroupedReduce` | Exec | P0 | L3/flatten 减该账户不属于任何 cell 的永续持仓(`GROUP_NONE`,`v[]` 归 hft-exec) |
| `ExecGateHeldNaked` | Exec | P0 | 自动处置被挡住超过 `gate_naked_alarm_ms`:账户启动闸门未放行而存在 Broken 或超过补腿阈值的裸量,或自动有界平仓到期后因等待条件迟迟不能开始(场合、参数与 `v[]` 归 hft-exec) |
| `ExecChainLegExcess` | Exec | P0 | optional 链上腿的多余成交 |
| `CtlAudit` | CtlAudit | Info | 每个请求一条(含被 gateway 拒绝的、只读的 `GetSchema`),非 gateway 写操作的 `Annotate` 回执。v[0..2] = cmd_id,v[2] = cmd_kind \| result_code << 16 \| role << 24 \| view_account << 32 \| chat_type << 40(Private=1 Group=2 Cli=3 Pipeline=4)\| target_account << 48,v[3] = issuer 前 8 字节,v[4..7] = cmd_hash 前 24 字节,v[7] = 对端 uid(`CmdHdr.peer_uid`,u32;非 socket 来源 0) |
| `PartialEventDropped` | Boot | P1 | 恢复时截掉的半个事件(apps/hft-core) |
| `TailSkipped` | Boot | P0 | v[0..2] = 跳过的 seq 区间,v[2] = 原因 Poison=1 Version=2 Operator=3 |
| `RecoverFromSnapshot` | Boot | P1 | v[0] = snapshot_seq,v[1..3] = 跳过区间,v[3] = 原因,v[4] = issuer 前 8 字节 |
| `KillSelfTest` | Boot | Info | v[0] = account,v[1] = 结果位;供人读,闸门读的是 Input `Timer{KillSelfTestOk}` |
| `InputQuarantined` | Boot | P0 | 毒丸隔离:v[0] = seq,v[1] = 崩溃次数 |
| `StartupStep` | Boot | Info | 启动每步开始与结束各一条(步骤与 `v[]` 归 apps/hft-core) |
| `HostDegraded` | Boot | P1 | 主机剖面不符而按 `degrade` 继续启动(apps/hft-core) |
| `JournalTailLost` | Boot | P1 | 恢复时 RAM 热段缺失、以冷副本为准,尾部可能丢失(apps/hft-core) |
| `RefdataWarning` | Boot | P1 | 启动时 `RefData::warnings()` 的每一条一条:v[0] = `LoadErrCode` 数值,v[1] = section,v[2] = row([hft-refdata](../hft-refdata/README.md#公开-api);写出时机归 apps/hft-core) |
| `StopMarkerCorrupt` | Boot | P0 | 停机标记损坏,退化为全局 Kill(规则与 `v[]` 归 apps/hft-core) |
| `EndpointOverride` | Boot | P1 | 以含 `endpoint_override` 的配置(测试网配置或 `sim` 实例)启动,每个生效的 venue 一条:v[0] = venue(条件归 apps/hft-core) |
| `ModelCheckpointDiscarded` | Advisor | P1 | v[0] = strategy,v[1] = 原因:Missing=1 Corrupt=2 KeyMismatch=3(`kind_id`、`Model::LAYOUT_VERSION`、`schema.id` 或 strategy_instance 不符)SeqNotInJournal=4 RestoreErr=5;参数版本、refdata 版本、二进制 SHA、`Model::PATH_VERSION` 不是丢弃原因;检查点键与格式见 [hft-strategy-sdk](../hft-strategy-sdk/README.md) |
| `ModelPathReset` | Advisor | P2 | 检查点的路径状态节因 `Model::PATH_VERSION` 不符被丢弃(原始样本节照常恢复,派生状态按原始样本重算):`Note.strategy` 指明实例,v[0] = 原因 VersionMismatch=1 RestoreErr=2(`restore_path` 出错),v[1] = 检查点的 `last_seq`,v[2] = `path_settle_ms`(该实例参数下 SDK 给出的路径状态收敛时长),v[3] = 检查点里的 `PATH_VERSION`,v[4] = 二进制的 `PATH_VERSION`;replayer 的 advice recompute 据 v[1]、v[2] 推迟比较起点(规则归 hft-strategy-sdk 与 [replayer](../../apps/replayer/README.md)) |
| `AdvisorLag` | Advisor | P1 | 唯一定义见下 |
| `ModelGroupReset` | Advisor | P2 | Model 重置了某 group 的状态(`ModelParamEffect::ResetGroups/ResetAll`、指纹变化或分位配置不一致);pipeline 据此对该 group 自动预热。`Note.strategy`、`Note.group` 指明对象;v[0] = 原因 Structural=1 Fingerprint=2 PctConfig=3 ResetAll=4,v[1] = 触发它的参数 version(规则归 hft-strategy-sdk) |
| `AdvisorStats` | Advisor | Info | 周期计数(`advice_coalesced` 等,`v[]` 归 hft-sequencer advisor) |
| `ModelCkptSkipped` | Advisor | P1 | 模型检查点超过盘上限被跳过(规则与 `v[]` 归 apps/hft-core) |

登记表的代码形式(`src/system.rs`;gen-schema 据它导出 Note 码表并与各 source crate 的 `const` 表核对):

```rust
pub struct NoteName {
    pub name: &'static str, pub source: NoteSource,
    pub level: Option<NoteLevel>,    // 上表"级别"列的固定值;"按条件 / 按档位 / 按 cause"的码为 None(实际级别由产出方按该列的条件取)
    pub v: &'static [VField],        // 上表 v[] 列写出的字段;未写出的归 source 所在 crate 的 const 表
}
pub struct VField { pub name: &'static str, pub bit: u16, pub bits: u16 }
    // 字段占 v[] 的位 [bit, bit + bits),位号 = 下标 × 64 + 位移:"v[i] = x" 为 bit = 64·i、bits = 64;"a | x << k" 中的子字段 bit = 64·i + k、bits = 该字段类型的位宽(u8 为 8,u16 为 16);
    // "v[i..j] = 字节串" 为 bit = 64·i、bits = 64·(j − i)。同一 NoteName 的字段两两不重叠;bits ≤ 64 的字段不跨 v[] 元素
impl VField {
    pub const fn get(&self, v: &[u64; 8]) -> u64;                         // 只用于 bits ≤ 64
    pub fn put(&self, v: &mut [u64; 8], x: u64) -> Result<(), EncodeErr>;  // bits < 64 且 x ≥ 2^bits → TooLong;只改本字段的位
}
pub static NOTE_NAMES: &[NoteName];   // 按上表行序;一行列出的多个码名各占一项
pub fn note_name(name: &str) -> Option<&'static NoteName>;
pub const fn note_code(source: NoteSource, n: u32) -> u32;   // (source as u32) << 24 | n;n = 0(保留)或 n ≥ 2^24 → debug_assert
```

`Strategy` 源的码、名、级别与 `v[]` 只在各策略的 SDK `StrategyDesc.notes`(`NoteDesc`)里,随策略 schema JSON 导出,watchtower 与 journal-archiver 按该描述符通用渲染与汇总;本 crate 与 gen-schema 不登记策略码,策略 Note 不用 `P0`(SDK 约束)。新增策略不改本表与生成物。

**`AdvisorLag`(唯一定义)**:某策略实例的 Model 落后于 journal 的量,**按引擎时间计**:`lag_ns = engine_ns(advisor 所见的 seq journal 已提交末条记录) − engine_ns(该实例 Model 已消费的末条记录)`(`engine_ns` 的算法见 [event-model 时间戳语义](../../docs/subsystems/event-model.md#时间戳语义);两条记录同属本次运行时等于记录头 tsc 之差的换算,跨重启追赶时仍连续)。advisor 线程在 `lag_ns > advisor_max_lag_ms × 1_000_000`(`advisor_max_lag_ms` 默认 5 000,[apps/hft-core](../../apps/hft-core/README.md) `[advisor]` 配置)期间,**每个实例每 10 s(按已提交末条记录的 `engine_ns`)至多写一条** `Note{source = Advisor, code = AdvisorLag}`(P1),`v[0]` = strategy,`v[1]` = `lag_ns / 1_000_000`(ms),`v[2]` = 落后的记录条数(已提交末条 seq − 已消费末条 seq),`v[3]` = 已消费末条 seq;回落到阈值以下后不写。watchtower 见到即报 P1,不另设阈值;telemetry 可按同一公式导出连续的 gauge,但不另起名字或口径。sequencer 不写本码。

```rust
pub struct HostProfile {            // 0x0507,176 B,Meta;启动时第一条,其后每段 boot 重述
    pub boot_id: [u8; 16], pub build_git: [u8; 20], pub _pad: [u8; 4],
    pub profile_sha256: [u8; 32],   // 随发布归档的调优清单哈希(deploy/)
    pub release: [u8; 16], pub kernel: [u8; 32],             // ASCII,右补 0
    pub tsc_hz_milli: u64, pub host_checks: u64 /* hft-rt HostCheck 位 */,
    pub hugepages_1g_total: u32, pub hugepages_1g_free: u32, pub isolated_cpus: [u64; 4],
}
pub struct ClockStatus {            // 0x0508,56 B,Meta;housekeeping 每 1 000 ms 一条(另在每段引导块重述),sequencer 原样记录、不据此做任何决定;
                                    // 冷读者用相邻两条的 (tsc, realtime_ns, phc_ns) 做跨时钟域换算(插值与误差界规则归 latency-budget §2.1)
    pub tsc: Tsc, pub realtime_ns: i64, pub phc_ns: i64, pub tsc_hz_milli: u64, pub chrony_offset_ns: i64,
    pub max_error_ns: u32, pub rms_ns: u32, pub calib_err_ns: u32 /* 校准对误差界(PTP_SYS_OFFSET_EXTENDED) */,
    pub source: u8 /* Phc=1 Ntp=2 None=3 */, pub state: u8 /* Synced=1 Unsynced=2 Stepped=3 */, pub _pad: u16,
}
pub struct SegmentRoll {            // 0x0509,48 B,Meta(结构记录;滚动决定本身是 Input `Timer{Roll}`);语义见 journal-format 的段滚动
    pub segment_no: u64, pub other_segment_no: u64, pub seq_mark: u64, pub utc_ns: i64,
    pub mark: u8 /* End=1 Begin=2 BootDone=3 */, pub reason: u8 /* Hourly=1 Size=2 Startup=3 Recovery=4 Manual=5 RecoverFromSnapshot=6 */,
    pub journal_kind: u8 /* Raw=1 Seq=2 Chain=3 */, pub _pad: u8, pub boot_records: u32, pub _rsv: u64,
}
```

#### EngineClock(0x050A,Input)

引擎时间 `engine_ns`(跨进程重启连续、停机时间不计)的**运行区间基准**。live driver([apps/hft-core](../../apps/hft-core/README.md))在每次进程启动、恢复与首段引导块完成之后、**任何其他 Input 之前**写恰好一条;sequencer 处理它时设置本次运行的引擎时钟基准,并在同一事件内写 `Note{Sequencer, EpochStart}`。回放照读这条记录,因此 `engine_ns` 确定。算法与语义的唯一定义在 [event-model 时间戳语义](../../docs/subsystems/event-model.md#时间戳语义)。

```rust
pub struct EngineClock {            // 0x050A,32 B,Input
    pub base_engine_ns: i64,        //  0(与 SDK `Now.engine_ns` 同为 i64) 本条记录(记录头 tsc)对应的引擎时间 = 上次运行最后一条已提交(EVENT_END)记录的 engine_ns;无历史 = 0
    pub tsc_hz_milli: u64,          //  8 本次运行换算 engine_ns 的 TSC 频率 × 1000(启动时校准一次,本次运行内不变)
    pub prev_last_seq: u64,         // 16 base_engine_ns 所依据的记录 seq(上次运行最后一条已提交记录;Snapshot 来源时 = SnapshotHdr.at_seq);Fresh 为 0
    pub source: u8,                 // 24 Fresh=1(空 journal)Tail=2(由上次运行的尾部记录算出)Snapshot=3(--recover-from-snapshot,取 SnapshotHdr.engine_ns)
    pub _pad: [u8; 7],              // 25
}                                   // 32 B
```

- 上次运行的尾部若丢失(崩溃后只恢复到更早的已提交记录),`base_engine_ns` 取恢复到的最后一条记录的值:丢失的那段运行时间不计入引擎时间,所有按 `engine_ns` 计的截止与冷却只会更晚到期(保守)。
- 运行中的段滚动不写本记录;读者从中途某段引导块起步时,从该段 `StateSnapshot` 的 `EngineClock` 节([journal-format 快照记录结构](../../docs/subsystems/journal-format.md#快照记录结构))取得当前基准。

### cap 记录

模式上限(cap)的编码,快照 `RiskState` 节的行、`Note{CapSet/CapCleared}` 的 `v[]` 都用它;来源、作用域、恢复条件归 [hft-risk](../hft-risk/README.md),优先级与解除命令列在 [control-plane](../../docs/subsystems/control-plane.md#模式与上限的统一优先级)。

```rust
#[repr(C)] pub struct CapRecord {   // 48 B
    pub source: CapSource,          //  0 hft-types:Kill=1 … Halted=8、Draining=9、ClockBad=10、HedgeFailure=11 … AccountCfg=15
    pub level: u8,                  //  1 source = Kill 时为 KillLevel;MarginStress 时为档位 1..3;否则 0
    pub cflags: u8,                 //  2 HARD=1 CONDITIONAL=2 NON_CLEARABLE=4
    pub _pad: [u8; 5],              //  3
    pub scope: CmdScope,            //  8 作用域(account、venue、strategy、inst:0 = 全部;group:GROUP_ALL = 全部,见 CmdScope)
    pub set_seq: u64,               // 24 设置它的记录 seq;Resume 只删 (source, scope) 完全匹配者
    pub params: [u64; 2],           // 32 来源相关;各来源(含 Kill 的位值)的含义只在 hft-risk 的 cap 表登记(本 crate 只保证 16 B)
}
const _: () = assert!(size_of::<CapRecord>() == 48);
```

`params` 的按来源取值见 [hft-risk cap 记录](../hft-risk/README.md#cap-记录)。`Kill` 带 `cflags.HARD` 时,`scope.account = 0` 为全局 HARD、非 0 为该账户的 HARD;kill 字位见 [hft-types](../hft-types/README.md#kill-字位)。

### 测量字段(验证回放屏蔽)

Meta 记录整条不参与比较;Output 记录逐字节比较时屏蔽下列字节;表以 `pub static MEASURE_MASK: &[(EventKind, u16 /* 偏移 */, u16 /* 长度 */)]` 导出,只列体内字节(`Multi` kind 的偏移按重组后的逻辑体计);记录头 `tsc` 对全部 Output 记录一律屏蔽,不逐 kind 列出。Output kind 的 `FieldDesc.measure` 与之一致(Input/Meta kind 上的 `measure` 只作说明)。

| 记录 | 屏蔽 | 偏移 / 长度 |
|---|---|---|
| 全部 Output 记录 | 记录头 `tsc` | 16 / 8 |
| `OrderDecision` | `decided_tsc` | 体 8 / 8 |
| `SwapDecision`(optional) | `decided_tsc` | 体 24 / 8 |
| `StateSnapshot` | [`SnapshotHdr`](../../docs/subsystems/journal-format.md#快照记录结构) 的 `snap_tsc`、`boot_id`、`snap_utc_ns`(写快照的主机与时刻,换机或重跑必然不同) | 逻辑体 56 / 8、64 / 16、80 / 8 |
| `ExecDecision`、`L2Snapshot{Keyframe}` | 无 | — |

`SnapshotHdr.state_hash` 与 `out_hash` 不覆盖上述字段,因此它们不影响验证回放的哈希比对。

### 定时器编码

`Timer`(event-model)的 `owner`/`timer_id`:

| `owner` | 值 | `timer_id` | 由谁写 |
|---|---|---|---|
| `Sys` | 1 | `Snapshot=1`(关键帧 `StateSnapshot`,一次写完;触发条件——周期、事件数与最大推迟——与写法归 [hft-sequencer](../hft-sequencer/README.md))、`Roll=2`(段滚动:整点后第一个空闲轮,避开资金费结算前后 ±2 min;大小安全阀分两级,见 [journal-format 段滚动](../../docs/subsystems/journal-format.md#段滚动与引导块))、`DayRoll=3`(`timer_id = 3 \| day << 8`,`day` = UTC 日序号)、`DeadMan=4`、`ConnRotate=5`、`AdvisorTick=6`、`SnapshotNow=7`、`ClockOk=8` / `ClockBad=9`、`ConflateReport=10`、`SessionKeepalive=11`、`PersistLag=12` / `DiskLow=13`(`timer_id = id \| level << 8`;level 1 = 禁开阈值,2 = Drain 阈值;housekeeping 观测、live driver 写入)、`StorageOk=14`(两项都回到阈值以下)、`FundingSettle=15`(`timer_id = 15 \| venue << 8`:该 venue 的资金费结算时刻已过,触发合成)、`KillSelfTestOk=16`(`timer_id = 16 \| account << 16`:启动时该账户的 kill 位自检通过,供启动闸门 `IDENTITY`)、`NotifyContinue=17`(策略通知队列溢出后的续推)、`RefdataRecheck=18`(`timer_id = 18 \| arg << 8`:Drain 末 `ReplaceRefdata` 装配复核的结果,arg = 0 通过,否则为 `RefdataAssembly` 的失败项 1–5;规则归 [hft-sequencer](../hft-sequencer/README.md) Drain);按 venue/账户的定时器(`DeadMan`、`ConnRotate`、`SessionKeepalive`)`timer_id = id \| venue << 8 \| account << 16` | live driver 在当时写成 Input 记录;回放照读 |
| `Keyframe` | 2 | `InstId`(该簿的 L2 关键帧) | live driver 在每个 `keyframe_period_ms` 周期内均摊 |
| `Oms` | 3 | `OmsContinue=1`(扇出上限 K 之后的续发)、`Sweep=2`、`UnknownQuery=3`(`timer_id = 3 \| table_ix << 8 \| slot << 16`:`table_ix` = hft-oms 按 venue × account 分设的槽表的稠密序号,8 位,< 256;`slot` = 表内槽号,16 位,< 65 536;两个位宽由 hft-oms 在加载 `OmsConfig` 时校验)、`HedgeRetry=4`(`timer_id = 4 \| link_id << 8`)、`SmokeTtl=5`(冒烟单 `ttl_ms` 到期自动撤单;`timer_id = 5 \| account << 8 \| venue << 16`,语义归 hft-oms) | sequencer 登记、live driver 到期写入 |
| `Risk` | 4 | 由 hft-risk 登记(如 MarginStress 减仓截止) | 同上;已登记 `MarginStale=1`、`MarginDeadline=2`、`MarginForce=3`(`timer_id = kind \| account << 8 \| venue << 16`) |
| `Exec` | 5 | `timer_id = kind \| account << 8`,`group` 字段 = GroupId;`kind`:`Wake=1`、`Fix=2`、`External=3`、`Continue=4`、`AutoFix=5`(Broken 持续 `broken_auto_fix_ms` 的截止)、`Alarm=6`(闸门滞留告警的复核时刻)(语义归 hft-exec) | 同上 |
| `Strategy` | 16 + `StrategyId` | `timer_id` = 策略经 `ctx.timer_at` 给的 `tag`,`group` 字段 = 该定时器所属 GroupId(实例级定时器为 `GROUP_NONE`) | 同上 |

`Timer.group`(u32)只由 `Exec` 与 `Strategy` 使用(可为 `GROUP_NONE`,不得为 `GROUP_ALL`),其余 owner 填 `GROUP_NONE`(0)(`GroupId` 是 u32,放不进 32 位的 `timer_id`)。各定时器的业务语义归登记它的 crate;本表只分配编码(只追加)。`ConnStateKind` 的编码表随 `gen-schema` 导出。

上表的代码形式是打包与解包的唯一实现(登记方 crate、live driver、replayer 共用;`Timer` 体的 `(owner, timer_id, group)` 由它产生):

```rust
pub const TIMER_OWNER_STRATEGY_BASE: u16 = 16;     // Strategy owner = 16 + StrategyId(17..=270);0 与 6..=16 未分配
pub enum TimerCode {
    Sys(SysTimer), Keyframe { inst: InstId }, Oms(OmsTimer),
    Risk { kind: u8 /* 1..=3 */, account: AccountId, venue: VenueId },
    Exec { kind: u8 /* 1..=6 */, account: AccountId, group: GroupId },
    Strategy { strategy: StrategyId, tag: u32, group: GroupId },
}
pub enum SysTimer { Snapshot, Roll, DayRoll { day: u32 }, DeadMan { venue: VenueId, account: AccountId }, ConnRotate { venue: VenueId, account: AccountId },
                    AdvisorTick, SnapshotNow, ClockOk, ClockBad, ConflateReport, SessionKeepalive { venue: VenueId, account: AccountId },
                    PersistLag { level: u8 }, DiskLow { level: u8 }, StorageOk, FundingSettle { venue: VenueId },
                    KillSelfTestOk { account: AccountId }, NotifyContinue, RefdataRecheck { arg: u8 } }
pub enum OmsTimer { Continue, Sweep, UnknownQuery { table_ix: u16, slot: u32 }, HedgeRetry { link_id: u32 },
                    SmokeTtl { account: AccountId, venue: VenueId } }
impl TimerCode {
    pub fn encode(&self) -> Result<(u16 /* owner */, u32 /* timer_id */, GroupId), TimerErr>;
    pub fn decode(owner: u16, timer_id: u32, group: GroupId) -> Result<TimerCode, TimerErr>;
}
#[repr(u8)] pub enum TimerErr { Owner = 1, Id = 2, Range = 3, Group = 4 }
```

- 打包按上表(低 8 位 = id;`Risk`/`Exec` 的 kind 即 id)。`encode`:`Strategy.strategy ∉ 1..=254` → `Owner`;`Risk.kind ∉ 1..=3`、`Exec.kind ∉ 1..=6` → `Id`;`DayRoll.day ≥ 2^24`、`UnknownQuery.table_ix ≥ 256` 或 `slot ≥ 65 536`、`HedgeRetry.link_id ≥ 2^24`、`PersistLag`/`DiskLow` 的 `level ∉ {1, 2}`、`RefdataRecheck.arg > 5` → `Range`;`Exec`/`Strategy` 的 `group = GROUP_ALL` → `Group`。
- `decode`:owner 未分配(0、6..=16、≥ 271)→ `Owner`;id 未登记,或该 id 不使用的位非 0 → `Id`;参数越界(同上)→ `Range`;`Exec`/`Strategy` 以外 `group ≠ GROUP_NONE`,或 `group = GROUP_ALL` → `Group`。检查顺序 Owner → Id → Range → Group;`decode(encode(c)) == c`。

## 策略载荷容器

策略的慢速输出(`Advice`)与参数(`StrategyParams`)只有容器在本 crate;载荷内容由策略 crate 自己定义与解码([hft-strategy-sdk](../hft-strategy-sdk/README.md))。**新增策略、修改策略的 advice 或参数字段都不改本 crate**;没有全局 `AdviceKind` 登记,也没有按策略的参数枚举。

### Advice(0x0503,Var,Input)

```rust
#[repr(C)] pub struct AdviceHdr {   // 32 B;逻辑体 = AdviceHdr + payload[len](记录按 8 B 对齐补零);字段与顺序是 SDK AdviceHdr 描述的唯一来源
    pub strategy: StrategyId,       //  0 产出它的策略实例;sequencer 只投递给该实例
    pub account: AccountId,         //  1 0 = 与账户无关
    pub local_kind: u16,            //  2 策略自定义的载荷类型(同一策略内唯一,含拆分续条的约定)
    pub group: GroupId,             //  4 GROUP_NONE(0)= 不针对某个 group(实例级或账户级载荷);其他值 = 该 group;GROUP_ALL 非法
    pub len: u16,                   //  8 payload 字节数,≤ ADVICE_MAX_PAYLOAD
    pub _pad: [u8; 6],              // 10
    pub basis_seq: u64,             // 16 模型产出它时已消费到的最后一条 seq journal 记录(模型只用 ≤ 该 seq 的事件)
    pub basis_tsc: Tsc,             // 24 basis_seq 那条记录的记录头 tsc(模型所见最新事件的处理时刻)
}
const _: () = assert!(size_of::<AdviceHdr>() == 32);
pub const ADVICE_MAX_PAYLOAD: usize = 4096;   // 更大的内容拆成多条,local_kind 与载荷内的序号由策略自行约定
```

- 引擎**不解释载荷**:sequencer 检查 `strategy` 存在、`len` 合法、`group` 为 `GROUP_NONE` 或属于该实例当前参数,把 `EventRef::Advice{gix, local_kind, payload, basis_seq, basis_tsc}` 投递给该实例(`gix` 由 `group` 现查;`GROUP_NONE` 的投递形式归 SDK)。未知 `strategy`、`len` 越界、`group = GROUP_ALL`、`group ≠ GROUP_NONE` 且其高 8 位 ≠ `strategy`、或 `account ≠ 0` 且未绑定到该实例 → 丢弃并写 `Note{Sequencer, AdviceMisrouted}`;group 已被移除或尚未生效,或该 group 的 `GroupSpec::fingerprint()` 在 `basis_seq` 之后变化过(`basis_seq < spec_seq`,sequencer 按 group 记录 `spec_seq`)→ 丢弃并写 `Note{Sequencer, AdviceStale}`(规则与检查顺序归 [hft-sequencer Advice 路由](../hft-sequencer/README.md#advice-路由))。
- **新鲜度按 `basis_tsc` 判定**,不按到达时刻:策略用 `Views::now()` 的 tsc 与 `basis_tsc` 之差判断建议是否过期(阈值是策略参数);回放中两者都取自 journal,结果确定。advisor 落后时 `basis_tsc` 随之变旧,建议自然被判过期。**跨启动**:TSC 不跨进程启动比较,`basis_seq < Now.boot_seq` 的 Advice 由策略判为过期([hft-strategy-sdk](../hft-strategy-sdk/README.md));sequencer 只做上一条的路由与指纹检查,不给 Advice 加任何过期标志。落后量的唯一口径是 [`AdvisorLag`](#控制与系统0x04xx0x05xx)(按 `engine_ns`)。
- `Advice` 是 Input:回放从 journal 读,不重算模型。`basis_seq` 让 replayer 可选地重跑模型并在同一位置比对载荷(模型确定性检查,见 [replayer](../../apps/replayer/README.md))。
- advisor → sequencer 经 advice 字节环([环元素类型](#环元素类型)),sequencer 写 journal 后才投递(决策可见 ⟺ 已记录)。

### StrategyParams(SPB1)

以 `ConfigBlob{kind = StrategyParams, encoding = 1}` 承载。每 (strategy, account) 一个 blob,内含该实例在该账户的全部 group;经 `SetParams` 整体替换。字节布局(小端,各段 8 B 对齐):

```
StrategyParamsHdr (48 B) | GroupSpec × n_groups (80 B each) | body (body_len B,本 crate 视为不透明字节)
```

```rust
#[repr(C)] pub struct StrategyParamsHdr {   // 48 B
    pub strategy: StrategyId,       //  0 策略实例
    pub account: AccountId,         //  1 必须已绑定到该实例(AccountMeta.bound_instance),否则 InvalidArgs{AccountBound}
    pub kind_id: u16,               //  2 = 该实例在 refdata Instances 中登记的种类 = StrategyDesc.kind_id
    pub schema_id: u32,             //  4 = ParamSchemaDesc.id
    pub schema_ver: u16,            //  8 = ParamSchemaDesc.version;body 按此版本解码
    pub source: ParamSource,        // 10 Pipeline=1 Manual=2
    pub _pad: u8,                   // 11
    pub n_groups: u16,              // 12 ≤ max_groups_per_instance(SDK);全部 cell 合计 ≤ max_cells_total(EngineConfig capacities;取值归 apps/hft-core)
    pub _pad2: u16,                 // 14
    pub version: u64,               // 16 每 (strategy, account) 单调递增
    pub generated_at_ns: i64,       // 24 UTC
    pub body_len: u32,              // 32 body 字节数,8 的倍数
    pub group_ctr_hwm: u32,         // 36 本实例分配过的最大 group 计数器(跨版本只增不减;续号与写入规则归 tools hft-cfg);≥ 本 blob 内每个 id.counter(),≤ GroupId::N_MAX
    pub schema_digest: u64,         // 40 规范 schema JSON 的摘要(由 hft-strategy-sdk `params` 计算):编码器所用 schema 与二进制内 schema 一致的证据
}
#[repr(C)] pub struct GroupSpec {   // 80 B;引擎理解的部分:订阅、账户绑定、LegTracker、ledger 归属
    pub id: GroupId,                //  0 = (strategy << 24) | n(构造归 hft-types):高 8 位 = StrategyParamsHdr.strategy,n ∈ 1..=GroupId::N_MAX;全局唯一由构造保证
    pub asset: AssetId,             //  4 各腿共同的基础资产(LegTracker 的净量单位)
    pub n_legs: u8,                 //  6 1..=4
    pub open_allowed: u8,           //  7 0/1;Live 模式下只有 1 才放行 Open
    pub legs: [LegDef; 4],          //  8 前 n_legs 项有效,其余全零
    pub _rsv: u64,                  // 72
}
#[repr(C)] pub struct LegDef {      // 16 B
    pub inst: InstId,               //  0 refdata 中的 inst;其 venue 必须在该账户的 venues 内
    pub venue: VenueId,             //  4 = inst 的 venue(冗余,加载时校验)
    pub kind: u8,                   //  5 Cex=1 Chain=2(Chain 为 optional)
    pub _pad: u16,                  //  6
    pub route: u32,                 //  8 Chain 腿的路由下标(optional);Cex 腿 0
    pub _pad2: u32,                 // 12
}
pub struct SpbView<'a> { pub hdr: &'a StrategyParamsHdr, pub groups: &'a [GroupSpec], pub body: &'a [u8] }
impl GroupSpec { pub fn view<'a>(&self, legs: &'a mut [hft_refdata::LegView; 4]) -> hft_refdata::GroupSpecView<'a>; }
    // 供 check_group:asset 与前 n_legs 条腿的 (inst, venue),腿视图写在调用方给的存储里(GroupSpecView 借用 LegView 切片);refdata 不依赖本 crate,转换在这里
pub fn parse_spb1(blob: &[u8]) -> Result<SpbView<'_>, ParamsErr>;     // 只做容器级校验(下列 1–3,按序,返回第一个错误);body 原样返回,不检查其内部结构
#[repr(u8)] pub enum ParamsErr { Truncated = 1, BadLen = 2, BadGroup = 3, BadLeg = 4, Padding = 5, DupGroup = 6, BadHdr = 7,
                                 Misaligned = 8 /* blob 不是 8 B 对齐,无法零拷贝视图 */ }
```

容器级校验(`parse_spb1`):

1. 总长 = 48 + 80 × `n_groups` + `body_len`,`body_len` 是 8 的倍数:blob 短于 48 + 80 × `n_groups`(含短于 48)→ `Truncated`;`body_len` 不是 8 的倍数或总长不等于该式 → `BadLen`;`source ∉ {Pipeline, Manual}` 或 `group_ctr_hwm > GroupId::N_MAX` → `BadHdr`;填充与未用腿全零(`Padding`)。
2. 每个 `GroupSpec`:`id.strategy() == hdr.strategy` 且 `id.counter() ∈ 1..=GroupId::N_MAX`(因此 `id ∉ {GROUP_NONE, GROUP_ALL}`)、`n_legs ∈ 1..=4`、`open_allowed ∈ {0, 1}`、`id.counter() ≤ hdr.group_ctr_hwm`(`BadGroup`);`id` 在 blob 内唯一(`DupGroup`,ctl 映射为 `GroupIdDup`)。
3. 每个有效 `LegDef`:`inst ≠ 0`、`kind ∈ {Cex, Chain}`、Cex 腿 `route = 0`(`BadLeg`);inst 与 venue、账户、资产的一致性以及"两腿不在同一交易所"(`SameExchangeLegs`)由 [hft-refdata `check_group`](../hft-refdata/README.md#公开-api) 检查。

**body 是不透明字节**:它的 `#[repr(C)]` 布局(`ParamsBodyHdr`、`ParamOverride`、容器视图 `body_view`)、规范编码("schema 序编码":按 `FieldDesc` 顺序的默认值 + 按 group 的覆盖表)与编解码器只在 [hft-strategy-sdk 的 `params` 模块](../hft-strategy-sdk/README.md)定义;`hft-cfg` 与 `hft-py` 包装同一个编解码器,Rust 与 Python 产出逐字节相同的 body。本 crate 不解析 body、不知道任何策略的字段。ctl-gateway 用 `parse_spb1` 做通用解析与 `GroupSpec` 校验,再调装配处 `StrategySet::decode_params(kind_id, body, schema_ver, …)` 解码 body(流程见 [hft-ctl](../hft-ctl/README.md));schema JSON 的格式与摘要也由 SDK `params` 产生,随发布制品由 `hft-core schemas --out <dir>` 导出(见 [tools hft-cfg](../../tools/README.md#hft-cfg))。

## 环元素类型

hft-core 进程内各 SPSC 环的元素类型定义在这里,因为生产者与消费者分属不同 crate(feed/gw 在 [hft-venue](../hft-venue/README.md),sim-gw 在 [hft-sim](../hft-sim/README.md),advisor 在 apps/hft-core,sequencer 在 [hft-sequencer](../hft-sequencer/README.md)),而它们都只能向下依赖。它们只在进程内流动、不进 journal(sequencer 记录的是其中的事件体),因此不要求字节布局稳定,只要求 `Copy` 与大小上限。

```rust
pub enum FeedMsg {                   // bbo-<venue> 环(只有 Bbo 与 BBO 连接的 ConnState)与 md-<venue> 环(其余行情与 md 连接的 ConnState)的元素;
                                     // size_of ≤ 128 B(编译期断言)
    Bbo(Bbo), L2Delta(L2Delta), Trade(Trade), Funding(Funding), MarkPrice(MarkPrice),
    IndexPrice(IndexPrice), Liquidation(Liquidation), ConnState(ConnState),
    Bulk { kind: EventKind, len: u32 }, // 只在 md-<venue> 环:下一条变长逻辑体(L2Batch、L2Snapshot)在配套的 md-<venue>-bulk ByteSpsc 里,sequencer 按此处的顺序取
}
pub enum RepMsg {                    // rep-<venue> 环与 sim-gw 的 rep-sim 环(同一类型);size_of ≤ 128 B
    OrderEvent(OrderEvent), SendReport(SendReport), PositionReport(PositionReport),
    BalanceReport(BalanceReport), FundingSettle(FundingSettle), ConnState(ConnState),
    ExternalFill(ExternalFill),
    AccountConfigReport(AccountConfigReportHdr), // 其后 hdr.n_lev 条 LeverageItem 在配套的 rep-<venue>-bulk ByteSpsc 里
    MarginReport(MarginReportHdr),   // 其后 hdr.n 条 MarginItem 在配套的 rep-<venue>-bulk ByteSpsc 里,sequencer 按此处的顺序取
}
pub enum HkMsg {                     // hk 环元素(→ sequencer;唯一写者是 housekeeping 线程,启动路径在 housekeeping 线程启动之前写完;方向归 hft-sequencer 环表);size_of ≤ 128 B(编译期断言)
    ClockStatus(ClockStatus),        // Meta,原样记录
    Note(Note),                      // Meta;source ∈ {Housekeeping, Boot}(advisor 的 Note 走 advice 环)
    Sys { timer_id: u32 },           // 环境观测(存储档位、时钟判定、kill 自检等,按定时器编码打包好的 Sys timer_id):live driver 当场写成 Input Timer{Sys}
}
pub type OutMsg = GwCmd;            // out 环元素的唯一类型(没有 enum 包装);hft-venue/hft-sequencer 沿用 OutMsg 这个名字
#[repr(C)]
pub struct GwCmd {                   // out-<venue> 环与 out-sim 环元素,64 B;gw 用 venue 模板把它编码成帧,sim-gw 直接撮合
    pub decision_seq: u64,           //  0 对应 OrderDecision 的 seq;非订单操作 0
    pub decided_tsc: Tsc,            //  8 测量字段
    pub px: Px,                      // 16
    pub qty: Qty,                    // 24
    pub inst: InstId,                // 32
    pub slot: SlotId,                // 36
    pub gen: u32,                    // 40
    pub target_slot: SlotId,         // 44 Cancel/Amend/Query 的目标;gw 从私有槽镜像取其 gen 与交易所 id
    pub op: GwOp,                    // 48 订单类与 OrderAction 同值:Place=1(= New) Cancel=2 Amend=3 CancelAll=4 Query=5;
                                     //    维护类:DeadManRefresh=6 Rotate=7 ResetRate=8 Resubscribe=9 SetEpoch=10 SessionKeepalive=11 MirrorRestore=12;
                                     //    模拟账户专用(只出现在 out-sim 环):SimReset=13 SimTransfer=14 SimFault=15
    pub account: AccountId,          // 49
    pub side: Side, pub tif: Tif,    // 50 仅 Place/Amend 有意义
    pub oflags: u16,                 // 52 = OrderDecision.oflags 原样复制;维护类 0
    pub _pad: [u8; 2],               // 54
    pub arg: u32,                    // 56 DeadManRefresh:timeout_ms;SetEpoch 与 MirrorRestore:epoch(u16,高 16 位为 0);SimTransfer:asset | from_venue << 16 | to_venue << 24;SimFault:venue | fault << 8;其余 0
    pub arg2: u32,                   // 60 SetEpoch:accept_prev(位 0)| prev_epoch << 16(位 1..16 为 0);SimFault:delay_ms;其余 0
}
const _: () = assert!(size_of::<GwCmd>() == 64);
impl GwCmd {
    // 维护类与模拟类构造器的 side/tif 写占位值 Buy/Gtc(gw 与 sim-gw 不读),其余未列字段为 0
    pub fn mirror_restore(account: AccountId, inst: InstId, slot: SlotId, gen: u32, epoch: u16, exch_order_id: u64) -> GwCmd;
    pub fn exch_order_id(&self) -> u64;   // 只对 MirrorRestore 有意义:从 px 字段的 64 位取回
    pub fn epoch(&self) -> u16;           // 只对 SetEpoch、MirrorRestore 有意义:arg 的低 16 位
    pub fn set_epoch(epoch: u16, accept_prev: bool, prev_epoch: u16) -> GwCmd; // op = SetEpoch,其余字段 0;prev_epoch = 恢复所依据快照节 3 的 engine_epoch(不按 epoch − 1 推算)
    pub fn accept_prev(&self) -> bool;    // 只对 SetEpoch 有意义:arg2 位 0
    pub fn prev_epoch(&self) -> u16;      // 只对 SetEpoch 有意义:arg2 的高 16 位
    pub fn sim_reset(account: AccountId) -> GwCmd;
    pub fn sim_transfer(account: AccountId, from: VenueId, to: VenueId, asset: AssetId, amount: Qty) -> GwCmd; // amount 装在 qty
    pub fn sim_fault(account: AccountId, venue: VenueId, fault: u8, delay_ms: u32) -> GwCmd; // fault 同 ControlCmd::SimFault
}
// MirrorRestore:冷启动恢复并 SetEpoch 之后,sequencer 为每个非终态槽发一条,让 gw 重建私有槽镜像(slot → gen、epoch、inst、交易所 id);
//   exch_order_id 装在 px 的 64 位(qty = 0)、槽的 epoch 装在 arg,只经构造器与访问器读写。无法编码查询的槽保持 Unknown,交 reconciler。
// SimReset/SimTransfer:sequencer 执行同名命令后投影给 sim-gw,让模拟交易所的余额与仓位与 ledger 同步变化(语义归 hft-sim)。
// SimFault:sequencer 执行同名命令后原样投影给 sim-gw(受控故障注入,语义归 hft-sim)。
pub enum FeedCmd {                   // sequencer → feed 的 feedctl-<venue> 环;非 repr(C) 的进程内枚举,size_of ≤ 16 B(编译期断言)
    Resync { inst: InstId }, Rotate { conn: ConnId },
    Subscribe { inst: InstId, streams: u8 }, // streams:该 inst 需要的流集合,位值与 SDK EvMask 相同
    Unsubscribe { inst: InstId },
}
// Subscribe.streams:位定义归 hft-strategy-sdk EvMask(本 crate 不依赖 SDK,故用 u8);值 = 该 inst 全部订阅的 EvMask 之并,
//   sequencer 在并集变化时重发 Subscribe;按位分发到 feed-bbo/feed-md 与各路由的规则归 hft-venue §1。
```

**环拓扑**(本表登记 sequencer 两侧每条环的元素类型;环名、方向、优先级与每轮上限的唯一归属是 [hft-sequencer 环表](../hft-sequencer/README.md),两表的环名与元素类型由 [`cargo xtask doc-check`](../../tools/README.md#cargo-xtask-doc-check) 核对一一对应,optional 的 `chain-in`/`chain-out` 不列;线程落点归 apps/hft-core):

| 环 | 写者 | 元素类型 |
|---|---|---|
| `ctl` | ctl-gateway | `CtlRecord` 帧(`ByteSpsc`,见下) |
| `hk` | housekeeping 线程(启动路径在 housekeeping 线程启动前写完) | `HkMsg` |
| `rep-<venue>` | gw(账户查询的解码结果 `AccountConfigReport`、`MarginReport` 等也由 gw 推入) | `RepMsg`;配套 `rep-<venue>-bulk` 字节环 |
| `rep-sim` | 模拟账户的 sim-gw | `RepMsg` |
| `bbo-<venue>` | 该 venue 的 `feed-bbo:<venue>` 任务 | `FeedMsg`(只有 `Bbo` 与 BBO 连接的 `ConnState`) |
| `md-<venue>` | 该 venue 的 `feed-md:<venue>` 任务(`routes = All` 时与 `bbo-<venue>` 由同一任务写) | `FeedMsg`;配套 `md-<venue>-bulk` 字节环 |
| `advice` | advisor | `AdviceMsg` 帧(`ByteSpsc`,见下) |
| `out-<venue>` | sequencer | `GwCmd`(`OutMsg`) |
| `out-sim` | sequencer | `GwCmd` |
| `feedctl-<venue>`、`feedctl-<venue>-bbo`(拆分时) | sequencer | `FeedCmd` |

以下环在 hft-venue 内部、不进 sequencer,环名与元素类型归 [hft-venue](../hft-venue/README.md):行情任务与 snap-fetch 之间的快照请求/回送环(BBO 补帧与可选的深度快照各一组);`acctreq-<venue>`(gw → acct-fetch,账户查询请求)与 `acct-<venue>`(acct-fetch → gw,hft-venue 内部字节环,携带原始响应与切好的报告,由 gw 写 raw 后推入 `rep-<venue>`)。

**路由**:sequencer 按 `AccountMeta.kind` 把 `GwCmd` 推入 `out-<venue>`(实盘账户)或 `out-sim`(模拟账户);gw 永远收不到模拟账户的单,sim-gw 永远收不到实盘账户的单(装配时断言,见 [apps/hft-core](../../apps/hft-core/README.md))。

**`OrderDecision` 与 `GwCmd` 的分工**:`OrderDecision` 只写 seq journal(意图 + 风控结果 + OMS 分配,含被拒与 Shadow 的);放行且非 `SHADOW` 的决策由 sequencer 投影成一条 `GwCmd`(`op = action`,`oflags` 原样,`decision_seq` = 该决策的 seq)推入 out 环,gw/sim-gw 只从 `GwCmd` 取下单字段。维护类与模拟类 `op` 由 sequencer 的系统定时器与命令产生。gw/sim-gw 对每条订单类 `GwCmd` 恰好回一条 `SendReport`;维护类与模拟类不回 `SendReport`,失败以 `ConnState{Degraded}` 报告。

**ctl 环**:ctl-gateway → sequencer 的 ctl 环是 [hft-rt](../hft-rt/README.md) `ByteSpsc`(容量按字节,≥ 256 KiB,单条 ≤ 容量/4),每条是一个无记录头的帧:

```rust
#[repr(C)] pub struct CtlRecordHdr { pub kind: EventKind, pub flags: u16 /* PART 等,同 RecordFlags */, pub len: u32 } // 8 B,其后 len 字节体
pub struct CtlRecord<'a> { pub kind: EventKind, pub flags: RecordFlags, pub body: &'a [u8] }  // kind ∈ {ControlCmd, ConfigBlob, ReconcileReport, Note{CtlAudit}}
impl<'a> CtlRecord<'a> {
    pub fn encoded_len(&self) -> usize;                                    // 8 + body.len(),向上取 8
    pub fn encode_into(&self, out: &mut [u8]) -> Result<usize, EncodeErr>; // 调用方从 ByteProducer::reserve 拿到 out 后 commit;本 crate 不依赖 hft-rt;
                                                                           // out 短于 encoded_len → BufTooSmall;填充写 0
    pub fn read(b: &'a [u8]) -> Result<Self, DecodeErr>;                   // b < 8 或 b < 8 + len → Short;kind 不是上述四种 → KindMismatch
}
```

sequencer 取出后原样加上 `RecordHeader`(赋 seq、`tsc`)写进 journal;大于单条上限的 `ConfigBlob` 由 gateway 按多段规则拆成多条 `PART` 帧。已解析好的大对象(`StrategyParams` 的解码结果、`RiskParams`)经 hft-rt `Mailbox` 交接,不经 ctl 环。

**advice 环**:advisor → sequencer 也是 `ByteSpsc`,advisor 线程是唯一写者;每条 = 与 ctl 环相同的 8 B `CtlRecordHdr`(`kind` = `Advice` 或 `Note`,`flags = 0`)+ 体,体为 `AdviceRecord`(`AdviceHdr` + payload,按 8 B 对齐)或一条 80 B 的 `Note`(只允许 `source = Advisor`):

```rust
pub struct AdviceRecord<'a> { pub hdr: AdviceHdr, pub payload: &'a [u8] }
impl<'a> AdviceRecord<'a> {
    pub fn encoded_len(&self) -> usize;                                    // 32 + payload.len(),向上取 8
    pub fn encode_into(&self, out: &mut [u8]) -> Result<usize, EncodeErr>; // len > ADVICE_MAX_PAYLOAD → TooLong;hdr.len ≠ payload.len() → Count;
                                                                           // out 短于 encoded_len → BufTooSmall;填充写 0
    pub fn read(b: &'a [u8]) -> Result<Self, DecodeErr>;                   // b < 32 或 b < 32 + len → Short;len > ADVICE_MAX_PAYLOAD → TooLong
}
pub enum AdviceMsg<'a> { Advice(AdviceRecord<'a>), Note(Note) }          // advice 环元素
impl<'a> AdviceMsg<'a> {
    pub fn encoded_len(&self) -> usize;                                    // 8 + 体长(Advice:AdviceRecord::encoded_len;Note:80)
    pub fn encode_into(&self, out: &mut [u8]) -> Result<usize, EncodeErr>; // Advice 体的错误同 AdviceRecord;Note.source ≠ Advisor → Count;out 不足 → BufTooSmall
    pub fn read(b: &'a [u8]) -> Result<Self, DecodeErr>;                   // b < 8 或 b < 8 + len → Short;kind 不是 Advice/Note → KindMismatch;
                                                                           // Note 体 len ≠ 80 → 小于为 Short、大于为 TooLong;source ≠ Advisor → InvalidEnum;Advice 体按 AdviceRecord::read
}
```

gw 发送前的 kill 检查只看 `op` 与 `oflags & RISK_REDUCING`([OrderFlags](#orderflags)),规则归 [hft-risk](../hft-risk/README.md),kill 字位(`AtomicU32`)在 [hft-types](../hft-types/README.md#kill-字位)。

## 控制命令编码

`ControlCmd`(0x0401,Multi,Input)= [`PartHdr`](../../docs/subsystems/journal-format.md#变长与多段记录) + `CmdHdr` + 按 `cmd` 的载荷(除 `InjectWarmup` 外都是定长;`InjectWarmup` = 16 B 前缀 + `len` 字节,`CmdHdr.payload_len` 给出总长)。本节只定义编码;每个命令的语义、类别、幂等与错误码在 [control-plane](../../docs/subsystems/control-plane.md),权限在 [hft-ctl-proto](../hft-ctl-proto/README.md#权限表)。

```rust
pub struct CmdHdr {                  // 128 B;字段对应 control-plane 的"信封"
    pub cmd_id: [u8; 16],            //   0 UUID,客户端生成,重试复用;(cmd_id) 重复 → Duplicate
    pub issuer: [u8; 24],            //  16 ASCII,如 "tg:123456789"、"pipeline"、"reconciler"、"cli:<login>",右补 0
    pub issued_at_ns: i64,           //  40 UTC
    pub expires_at_ns: i64,          //  48 UTC
    pub expires_tsc: Tsc,            //  56 gateway 换算;sequencer 用记录头 tsc 与它比较
    pub peer_uid: u32,               //  64 Unix socket 对端 uid
    pub cmd: CmdKind,                //  68 u16,见下表
    pub cmd_ver: u16,                //  70 该命令载荷版本
    pub role: u8,                    //  72 gateway 判定的有效角色:Viewer=1 Trader=2 Admin=4;3 退役、永不复用;进程主体 0
    pub account: AccountId,          //  73 0 = "*"
    pub confirmed: u8,               //  74 1 = 二次确认的第二步(先 PREVIEW,再带 confirmed 重发;绑定方式见 control-plane 版本与确认)
    pub hflags: u8,                  //  75 PREVIEW=1:只预览,sequencer 不改风险与模式状态,回 CmdResult{preview}(Viewer 的 Kill 另登记一条预览记录)
    pub payload_len: u32,            //  76
    pub cmd_hash: [u8; 32],          //  80 规范化命令的 SHA-256(覆盖 account、完整 CmdScope 与载荷)
    pub base_version: u64,           // 112 比较并设置;0 = 不检查
    pub expect_state_version: u64,   // 120 确认步骤:预览时得到的 ctl_state_version;sequencer 当前值不同 → ConfirmInvalid{StateChanged};0 = 不检查。
                                     //     role = Viewer 的 Kill 不用本字段(写 0):其确认绑定同一 issuer 的预览 cmd_hash + 60 s TTL
}
pub struct CmdScope { pub strategy: StrategyId, pub venue: VenueId, pub account: AccountId, pub _pad: u8,
                      pub group: GroupId, pub inst: InstId, pub _pad2: u32 }          // 16 B
// strategy、venue、account、inst:0 = 全部("*");group:GROUP_ALL(u32::MAX)= 全部("*")或该命令不按 group 限定,
// 具体 GroupId = 该 group;GROUP_NONE(0)在 CmdScope 中非法(→ InvalidArgs)。CapRecord.scope 同此编码。
pub struct BlobRef  { pub kind: BlobKind /* u16 */, pub encoding: u16, pub len: u32, pub version: u64, pub sha256: [u8; 32] } // 48 B

pub enum ControlCmd<'a> {            // Rust 侧类型化视图(InjectWarmup 的 payload 借用逻辑体);载荷字节布局 = 字段按序排列 + 显式填充到 8 的倍数;名字以 control-plane 为准
    SetMode { scope: CmdScope, mode: RunMode },                     // 0x0001  24 B
    SetParams { scope: CmdScope, blob: BlobRef },                   // 0x0002  64 B;scope.strategy/account 必须等于 blob 的 StrategyParamsHdr
    SetRisk { blob: BlobRef },                                      // 0x0003  48 B
    Kill { scope: CmdScope, level: KillLevel, flatten: bool,        // 0x0004  v2 24 B(scope 16 + level/flatten/cancel_resting/hard 4 + 4 填充;
           cancel_resting: bool, hard: bool },                      //         v1 同为 24 B,hard 占 v1 的第一个填充字节,读 v1 为 0);hard 允许与 Global 或 Account 组合;
                                                                    //         与 Venue → InvalidArgs;flatten 只与 Account 组合且不与 hard 同用(BadCombination)
    // 0x0005 保留,永不复用
    RotateConn { venue: VenueId, account: AccountId },              // 0x0006   8 B(optional)
    Resubscribe { venue: VenueId, inst: InstId /* 0 = 全部 */ },    // 0x0007   8 B(optional)
    SnapshotNow,                                                    // 0x0008   0 B(optional)
    ReplaceRefdata { blob: BlobRef },                               // 0x0009  48 B
    Resume { scope: CmdScope, source: CapSource },                  // 0x000A  24 B;source 必须 CapSource::resumable();只删 (source, scope) 完全匹配的一条 cap
    // 0x000B 退役,永不复用
    // 0x000C、0x000D 退役,永不复用
    HedgeFix { scope: CmdScope },                                   // 0x000E  16 B;Broken/Reorg 的人工处理(自动有界平仓之外)
    CancelAll { scope: CmdScope },                                  // 0x000F  16 B(account、venue、inst;0 = 全部)
    Drain { cancel_resting: bool },                                 // 0x0010   8 B(1 + 7 填充)
    RecordTransfer { account: AccountId, from: VenueId, to: VenueId, asset: AssetId,
                     amount: Qty, reference: [u8; 64] /* 交易所划转 id(ASCII)或链上 tx 哈希原始字节,右补 0 */ }, // 0x0011 80 B
    Annotate { kind: u16 /* OpsNote=1 LocalWrite=2 Incident=3 */, text_len: u16,
               digest: [u8; 32], text: [u8; 256] /* UTF-8,右补 0 */ },          // 0x0012 296 B;无状态效果,只进 journal
    AdoptExchange { account: AccountId, venue: VenueId, inst: InstId, report_id: u64,   // 0x0013 v1 16 B(1+1+2 填充+4+8)
                    item: u8 /* 0 = Position(v1 读出)1 = Balance */, _pad: u8, asset: AssetId, // v2 追加 16 B → 32 B
                    group: GroupId /* Balance:归属的 group;GROUP_NONE(0)= unassigned */, entry_px: Px /* 0 = 按对账时 mark/bid 计价 */ },
    SimReset { account: AccountId },                                // 0x0014   8 B;只对 Sim 账户
    SimTransfer { account: AccountId, from: VenueId, to: VenueId, asset: AssetId, amount: Qty }, // 0x0015 16 B(1+1+1+1 填充+2+2 填充+8);只对 Sim 账户
    VenueNotice { venue: VenueId, kind: u8 /* Delisting=1 FundingInterval=2 Maintenance=3 */, _pad: u16,
                  inst: InstId /* 0 = 该 venue 全部 inst */, a: i64, b: i64 },  // 0x0016 24 B;Delisting:a = at_ns;FundingInterval:a = new_s;Maintenance:a = from_ns,b = to_ns
    SetBalanceBaseline { account: AccountId, venue: VenueId },      // 0x0017   8 B
    StrategyCmd { instance: StrategyId, reduce_only: u8 /* 0/1:发送方声明只减仓 */, len: u16, group: GroupId, // 0x0018 264 B(1+1+2+4+256);载荷含义由该实例的策略定义,引擎不解释
                  payload: [u8; 256] /* 前 len 字节有效,其余 0;len ≤ 256 */ },  //   group:GROUP_NONE = 实例级;GROUP_ALL 非法;账户 = CmdHdr.account
    InjectWarmup { instance: StrategyId, last: u8 /* 0/1:本批预热数据的末块 */, local_kind: u16, // 0x0019 变长:16 B 前缀 + len B 载荷,按 8 B 补零
                   chunk_no: u32 /* 同一 (instance, local_kind) 批内从 0 递增 */, len: u16, _pad: [u8; 6],
                   payload: &'a [u8] /* len 字节,≤ WARMUP_MAX_PAYLOAD,格式由策略定义 */ },   //   实例级命令:CmdHdr.account 必须为 0
    SmokeOrder { account: AccountId, venue: VenueId, _pad: u16, inst: InstId,     // 0x001A  16 B(1+1+2 填充+4+1+3 填充+4);Admin 冒烟单,只对 Live 账户
                 side: Side, _pad2: [u8; 3], ttl_ms: u32 /* 范围与缺省值见 control-plane */ },
    WriteOffResidual { account: AccountId, _pad: [u8; 3], group: GroupId },        // 0x001B   8 B;残余核销(语义见 control-plane)
    SimFault { account: AccountId, venue: VenueId, fault: u8 /* Off=1 RejectNew=2 Disconnect=3 DelayReports=4 */, _pad: u8, // 0x001C 8 B;只对 Sim 账户
               delay_ms: u32 /* 只用于 DelayReports(1..=60 000),其余 0 */ },   //   参数与校验见 control-plane,sim-gw 语义见 hft-sim
}
pub const STRATEGY_CMD_MAX_PAYLOAD: usize = 256;
pub const WARMUP_MAX_PAYLOAD: usize = 4096;   // 整条 ControlCmd 记录 ≤ 24 + 16 + 128 + 16 + 4096 + 4(CRC)向上取 8 = 4 288 B,单段
impl ControlCmd {
    pub fn kind(&self) -> CmdKind;
    pub fn encode(&self, hdr: &CmdHdr, out: &mut [u8]) -> Result<usize, EncodeErr>;
        // 写 CmdHdr + 载荷(逻辑体);CmdHdr.cmd、cmd_ver、payload_len 按 self 与当前版本填写(忽略 hdr 中这三项);out 不足 → BufTooSmall;
        // StrategyCmd.len > 256、InjectWarmup.len > 4096 或 payload.len() ≠ len → TooLong
    pub fn decode(logical_body: &'a [u8]) -> Result<(CmdHdr, ControlCmd<'a>), DecodeErr>;
        // logical_body < 128 → Short;未知 cmd 或 cmd_ver → UnknownKind;payload_len 与 logical_body.len() − 128 不等,或与该 (cmd, cmd_ver)
        // 的载荷大小(InjectWarmup:16 + len 向上取 8)不等 → 小于为 Short、大于为 TooLong;StrategyCmd.len > 256、InjectWarmup.len > 4096、
        // StrategyCmd.payload 在 len 之后有非零字节、Annotate.text_len > 256 → TooLong;枚举、码字段(Annotate.kind、VenueNotice.kind、SimFault.fault、
        // AdoptExchange.item 越出码表)、CapSource(0 或 > 15)与 bool 字段(非 0/1)越界 → InvalidEnum;载荷中未作为变体字段列出的填充字节非 0 → InvalidEnum
        // (Kill v1 的偏移 19 除外:v1 填充,读为 hard = false)。因此当前 cmd_ver 的命令 decode 后再 encode 逐字节相同
}
```

`RunMode`、`KillLevel`、`CapSource`、`RejectBits` 的枚举与编码在 [hft-types](../hft-types/README.md#模式kill-级别cap-来源与风控拒绝位),语义在 [hft-risk](../hft-risk/README.md);`decode` 的 `UnknownKind`/`InvalidEnum` 由 gateway 与 sequencer 回 `InvalidArgs{UnknownDiscriminant}`(control-plane 结果码)。`ReconcileReport` 不是命令变体,见[事件体](#控制与系统0x04xx0x05xx)。`GetSchema` 是 gateway 本地应答的只读请求,不进 ctl 环、没有 `CmdKind`(见 [hft-ctl](../hft-ctl/README.md))。

**CmdKind 编号**(只追加、永不复用):`0x0001 SetMode`、`0x0002 SetParams`、`0x0003 SetRisk`、`0x0004 Kill`、`0x0005` 保留、`0x0006 RotateConn`、`0x0007 Resubscribe`、`0x0008 SnapshotNow`、`0x0009 ReplaceRefdata`、`0x000A Resume`、`0x000B` 退役、`0x000C` 退役、`0x000D` 退役、`0x000E HedgeFix`、`0x000F CancelAll`、`0x0010 Drain`、`0x0011 RecordTransfer`、`0x0012 Annotate`、`0x0013 AdoptExchange`、`0x0014 SimReset`、`0x0015 SimTransfer`、`0x0016 VenueNotice`、`0x0017 SetBalanceBaseline`、`0x0018 StrategyCmd`、`0x0019 InjectWarmup`、`0x001A SmokeOrder`、`0x001B WriteOffResidual`、`0x001C SimFault`。各命令是 core 还是 optional 以 [control-plane](../../docs/subsystems/control-plane.md#命令集) 为准;编码全部在此定义。新增命令按 [cookbook](../../docs/cookbook/adding-a-control-command.md) 走;**新增策略不需要新命令**:策略专属的人工操作用 `StrategyCmd`,冷启动预热数据用 `InjectWarmup`,两者的载荷都由策略定义。

**两个策略通道命令的投递**(语义、类别与权限在 control-plane;SDK 侧的 `EventRef` 形状以 [hft-strategy-sdk](../hft-strategy-sdk/README.md) 为准):

- `StrategyCmd`:sequencer 校验后把 `payload[..len]` 与 `reduce_only` 作为 `EventRef::Manual` 投递给该实例的快速部分(`group ≠ GROUP_NONE` 时带该 group 的 `gix`);策略因此产生的意图照常经模式门与检查链。`reduce_only = 1` 时,策略处理该命令期间产生的意图带 `MANUAL_RO` 标记,其中的 Open 类一律被拒(`CLOSE_ONLY`)。
- `InjectWarmup`:每块是生产者(策略的 pipeline 插件)产出的**自身完整**的块(≤ `WARMUP_MAX_PAYLOAD`;块边界与块内格式归策略,[hft-strategy-sdk](../hft-strategy-sdk/README.md)),客户端按块列表逐块发送、递增 `chunk_no`,不切字节([hft-ctl 客户端](../hft-ctl/README.md#客户端))。作为 Input 记进 seq journal;sequencer 只校验实例存在与 `len`,写 `CmdResult`。advisor 尾随 journal,读到同一事件内 `CmdResult{Applied}` 后把它作为 `EventRef::Warmup{local_kind, chunk_no, last, payload}` 交给该实例的 `Model`;策略在 SDK 中声明接收时,sequencer 也把同一事件投递给快速部分。回放从 journal 读同样的块,结果确定。

**`SmokeOrder` 与 `WriteOffResidual`**:语义、类别、前置条件与结果码在 [control-plane 命令集](../../docs/subsystems/control-plane.md#命令集);冒烟单建槽(`OrderFlags::SMOKE`)、定价与 `Timer{Oms, SmokeTtl}` 自动撤单归 [hft-oms](../hft-oms/README.md);核销的记账(`AdjustDelta::WriteOff`)归 [hft-ledger](../hft-ledger/README.md)。

**内容与引用分离**:`SetParams`/`SetRisk`/`ReplaceRefdata` 的命令只携带 `BlobRef`;内容(`ConfigBlob` 分段)**先于**该命令写入 journal,本身是惰性的,命令 `Applied` 才生效。回放读到命令时,按 `sha256` 在已读过的 `ConfigBlob` 中取。

**账户编码一致性**:命令只有一个目标账户来源——`CmdHdr.account`。带 `CmdScope` 的命令要求 `CmdScope.account == CmdHdr.account`,不带 scope 但有 `account` 字段的命令(`AdoptExchange`、`SimReset`、`SimTransfer`、`SetBalanceBaseline`、`RecordTransfer`、`SmokeOrder`、`WriteOffResidual`、`SimFault`)要求该字段 `== CmdHdr.account`,否则 gateway 返回 `InvalidArgs`(sequencer 也复核);不带账户的命令(`SetRisk`、`ReplaceRefdata`、`VenueNotice`、`StrategyCmd`…)只看 `CmdHdr.account`。`account = 0` 编码 `*`;每个命令允许的账户取值(必须具体、只能 `*`、两者均可)只在 [control-plane 账户语义](../../docs/subsystems/control-plane.md#账户语义)第 9 条登记,本 crate 只定义编码。`cmd_hash` 覆盖 `CmdHdr.account` 与完整载荷。

## 变长记录前缀

`Multi` kind 的逻辑体 = 多段拼接后的字节(拆分、`PART` 标志、重组规则见 [journal-format](../../docs/subsystems/journal-format.md#变长与多段记录));`Var` kind 是单条记录,体 = 前缀 + 尾部。各前缀:

```rust
pub struct L2SnapHdr {              // L2Snapshot(0x0103,Multi)前缀,72 B;其后 [Level; n_bids + n_asks]
    pub h: MarketHdr, pub snap_id: u64 /* 快照对应的簿状态 id(Binance lastUpdateId 等);TopN:交易所给出的消息序号,无则 0(不做序号链校验) */,
    pub origin: u8 /* Exchange=1(增量深度的 REST/WS 对齐快照,Input) Keyframe=2(sequencer 本地导出,Output)
                      TopN=3(分档快照流的一条消息,Input;簿整替换) */, pub _pad: [u8; 3],
    pub n_bids: u16, pub n_asks: u16,
}
pub struct Level { pub px: Px, pub qty: Qty }       // 16 B;买档价降序在前,卖档价升序在后

pub struct L2BatchHdr {             // L2Batch(0x0109,Var)前缀,80 B;其后 [Level; n_bids + n_asks](买在前、卖在后,qty = 0 删档)
    pub h: MarketHdr,               //  0
    pub prev_id: u64,               // 56 本消息作用于其上的簿状态 id(venue 协议归一化,同 L2Delta)
    pub last_id: u64,               // 64 应用后的状态 id
    pub n_bids: u16, pub n_asks: u16, // 72
    pub bflags: u16,                // 76 SNAPSHOT_BRIDGE=1(快照后首条)
    pub _pad: u16,                  // 78
}
const _: () = assert!(size_of::<L2BatchHdr>() == 80);

// venue 协议解码时写给运行时 sink 的借用视图(不进 journal;运行时据此写 L2Snapshot/L2Batch 记录)
pub struct L2SnapshotPart<'a> { pub hdr: L2SnapHdr, pub bids: &'a [Level], pub asks: &'a [Level], pub part: u16, pub parts: u16 }
pub struct L2BatchView<'a>    { pub hdr: L2BatchHdr, pub levels: &'a [Level] }

pub struct BlobHdr {                // ConfigBlob(0x0403,Multi)前缀,72 B;其后 len 字节内容,SHA-256 = blob.sha256
    pub blob: BlobRef, pub scope: CmdScope,     // StrategyParams:scope.strategy/account = 该 blob 的 (strategy, account)
    pub boot: u8 /* 1 = 段首重述当前生效配置 */, pub _pad: [u8; 7],
}
#[repr(u16)] pub enum BlobKind { Refdata = 1, StrategyParams = 2, RiskParams = 3, HostManifest = 4, Engine = 5 /* EngineConfig(ECF1) */, SimConfig = 6 /* hft-core `[sim]` 表(延迟/保证金模型),只在 sim 实例;replayer sim 读取 */ }
// Engine 与 SimConfig 同 refdata、StrategyParams、RiskParams 一样在每段引导块以 boot = 1 重述(journal-format 段滚动与引导块)
// encoding:Refdata → 1 = RDB1(hft-refdata);StrategyParams → 1 = SPB1(本 crate,见策略载荷容器);RiskParams 的编码号由 hft-risk 登记;Engine → 1 = ECF1(hft-refdata)

pub struct RawHdr {                 // RawRx(0x0601)/RawTx(0x0602)前缀(Multi),24 B;其后 payload_len 字节原文
    pub venue: VenueId, pub conn: ConnId,
    pub what: u8 /* WsText=1 WsBinary=2 RestResponse=3 Ping=4 Pong=5 Connect=6 Disconnect=7 SubAck=8 WsClose=9 */,
    pub rflags: u8 /* REDACTED=1:含签名帧,只记元数据不记原文;SAMPLED=2:raw-feed sampled 模式下被抽中的数据帧 */, pub stream: u32 /* 订阅内部 id */,
    pub hw_rx_ns: i64 /* RawTx 为 0 */, pub payload_len: u32, pub _pad: u32,
}

pub fn multi_view<P: Body>(logical: &[u8]) -> Result<(P, &[u8]), DecodeErr>;
    // Multi 逻辑体 = 前缀 P + 尾部:按值读前缀(短于前缀 → Short;枚举检查同 read;L2SnapHdr.origin ∉ 1..=3、BlobHdr.boot ∉ {0, 1} → InvalidEnum);
    // 尾部长度与前缀计数(L2Snapshot:16 × (n_bids + n_asks);ConfigBlob:blob.len;RawRx/RawTx:payload_len)不等 → 短为 Short、长为 TooLong
pub const PART_MAX_CHUNK: usize = 65_488;   // = 65 536 − 24 − 16 − 8(journal-format 多段规则 1)
pub struct PartIter<'a> { /* … */ }          // Item = (PartHdr, &'a [u8] /* chunk */, RecordFlags /* 非末段含 PART */)
impl<'a> PartIter<'a> { pub fn new(logical: &'a [u8]) -> Result<Self, EncodeErr>; }   // 按 PART_MAX_CHUNK 切块;段数 > 65 535 → TooLong
pub struct Reassembler<'b> { /* … */ }
impl<'b> Reassembler<'b> {
    pub fn new(buf: &'b mut [u8]) -> Self;                                   // 重组缓冲由调用方提供,上限 = buf.len()
    pub fn push(&mut self, rec: &RecordView<'_>) -> Result<Option<&[u8]>, DecodeErr>;
        // rec.body = PartHdr + chunk;末段返回完整逻辑体。total_len > buf.len() → TooLong;part 不是期望的下一号、kind/tsc/parts/total_len 与组首不同、
        // seq 不是上一段 + 1、PART 位与 part < parts − 1 不符、chunk_len 超出该段体、末段 Σchunk_len ≠ total_len → BadPart(组被丢弃)
    pub fn reset(&mut self);                                                  // 丢弃未完成的组
}
```

`Advice`(0x0503,Var)的前缀 `AdviceHdr` 见[策略载荷容器](#advice0x0503varinput);`MarginReport`(0x0208,Var)的前缀 `MarginReportHdr` 与 `AccountConfigReport`(0x0209,Var)的前缀 `AccountConfigReportHdr` 见[执行](#执行0x02xx)。`StateSnapshot`(0x0504,Multi)的逻辑体结构(`SnapshotHdr` + 分节)在 [journal-format](../../docs/subsystems/journal-format.md#快照记录结构) 定义,本 crate 实现其类型。raw journal 记录头的 `tsc` 是 `rx_tsc`(RawRx)或 `sent_tsc`(RawTx)。

## 实现说明

<details><summary>实现要点</summary>

**定义宏**:每个定长体与 Var 前缀用一个 `macro_rules! event_body!` 声明(不引入 proc-macro crate),宏展开出:结构体与 `zerocopy` 派生、`Body` 实现、`FieldDesc` 表(偏移用 `core::mem::offset_of!`)、编译期断言(`size_of % 8 == 0`、`size_of ≤ 65536 − 24 − 8`、与 `KINDS` 中登记的 `size`/`prefix` 相等)。`KINDS` 是手写的 `static` 数组,单元测试断言它与全部 `Body` 实现一一对应、编号唯一、落在 event-model 规定的编号段内。Var kind 的 `max` 编译期检查 `prefix + max ≤ 65536 − 24 − 8`(`Advice`:32 + 4096;`MarginReport`:64 + 16 × 1024;`AccountConfigReport`:64 + 16 × 1024;`L2Batch`:80 + 16 × 4000)。`ControlCmd` 是 Multi,但 `InjectWarmup` 的最大逻辑体(16 + 128 + 16 + 4096)远小于单段上限,编码器断言它总是单段。

**schema 生成**(`cargo xtask gen-schema`,调用 `hft_event::schema::catalog()` 与 `hft_refdata::schema::{RDB1_ROWS, load_err_codes}`;嵌套结构(如 `MarketHdr`)在 lock、Arrow 与目录中展开为 `外层.字段`,在 Python 中为嵌套 dtype):

| 输出 | 内容 |
|---|---|
| `docs/generated/event-catalog.md` | 每 kind:编号、域、角色、布局、`schema_version`、字段表(偏移、类型、单位、说明)、枚举码表;Note 码表、`ConnState.reason` 码表、`ExecDecision.detail` 子码表与 `OrderDecision.reason` 码表(各 crate 与策略 crate 经 SDK 提供) |
| `python/_generated/events.py` | 每 kind 的 `numpy.dtype({names, formats, offsets, itemsize})`(`names`/`offsets` 与 `FieldDesc` 逐项相同,Var/Multi 的 `itemsize` 为前缀大小)、kind 常量、枚举字典;`Px/Qty` 为 `int64` 并带 `hft.type` 注解;SPB1 容器头(`StrategyParamsHdr`、`GroupSpec`、`LegDef`)的 dtype(body 由 hft-py 包装的 SDK 编码器产生) |
| `schemas/arrow/<kind>.json` | 每 kind 一个文件,文件名为 kind 名的 snake_case(`Bbo` → `bbo.json`,`L2Delta` → `l2_delta.json`);JSON `{fields: [{name, type, nullable, metadata}], metadata}`,`type` 为 Arrow 类型名(`UInt8`…`Int64`、`FixedSizeBinary(n)`),保留字节不导出;`Px/Qty` 为 `Int64` + 字段元数据 `hft.type=px/qty`(十进制换算由 journal-archiver 按 inst 的 `AssetExp` 完成),ID、码表与 TSC 字段同样以 `hft.type` 标注 |
| `python/_generated/refdata.py` | RDB1 行 dtype(只读,解析 journal 里的 refdata blob;编码器只在 Rust);hft-refdata 的 `LoadErrCode` 码表(`event-catalog.md` 同时列出) |
| `schemas/event-layout.lock` | 只追加核对的基线(逐行文本:`kind <编号> <名> v<版本> <fixed\|var\|multi> <大小> [deprecated]` 后接缩进的 `<偏移> <字段> <类型>` 行,`cmd`、`load-err`、`note <source> <n> <名>`、`struct`、`strategy <kind_id> <名>` 后接 `note`/`op`/`tag` 行):每 kind 的编号、名、`schema_version`、大小、字段表(名、偏移、类型)与 `deprecated` 标记(`kinds.rs` 中该 kind 常量带 `#[deprecated]`);`CmdKind`、hft-refdata `LoadErrCode` 的编号与名;引擎 Note 码 `(source, n) → 码名`;各策略种类 `kind_id → 名` 及其 Note 码(`NoteDesc.code → 名`)、命令 `CmdDesc.op → 名` 与检查点节 `SectionDesc.tag → 名`(从 `hft-core schemas` 导出读取);SPB1 容器结构(`StrategyParamsHdr`、`GroupSpec`、`LegDef` 的字段表) |
| `schemas/conn-reason.json` | `ConnState.reason` 的 `Resync`/`Degraded` 码表(读 [hft-venue](../hft-venue/README.md#产生与消费的事件) `src/codes.rs` 的 `CONN_REASONS` const 表):`{reasons: [{code, name, state}]}`,按码升序;`event-catalog.md` 同时列出 |
| `crates/hft-telemetry/src/notes_catalog.rs` | 引擎 Note 码表:每个 `(source, n)` 的码名、默认级别与 `NOTE_NAMES` 中的 `v[]` 字段(hft-telemetry 的翻译表;策略码不进此表) |

`gen-schema` 每次运行都对照基线 `schemas/event-layout.lock` 做只追加核对(CI 取 PR 目标分支合并基点上的该文件,取不到合并基点退出 2;本地取工作区 HEAD 已提交的版本;基线中还没有该文件时只生成、不核对),违反时不改写任何生成物、退出 1;`--check` 另把生成物与仓库逐字节比较。失败条件:布局变化而 `schema_version` 未加一(占用保留字节且大小不变的情形除外,条件见 [event-model 演进规则](../../docs/subsystems/event-model.md#演进规则));新布局不是旧布局的前缀扩展(改名、改型、改偏移、删字段;把 `_pad*`/`_rsv*` 中的字节改为具名字段不算改名);kind、`CmdKind`、hft-refdata `LoadErrCode`、引擎 Note 码 `(source, n)`、策略 `kind_id`、策略 Note 码、`CmdDesc.op` 或检查点节 `tag` 的编号消失或被复用(同一编号换了名字;策略的后四项按 `kind_id` 分别核对;kind(按编号段)、`CmdKind` 与 `LoadErrCode` 的新编号小于基线中同一编号空间的最大值而基线没有它,视为复用退役或保留的编号);const 表或 `NoteDesc` 中出现 `n = 0`;追加或占用保留字节的新字段是 0 非法的枚举;Note 登记表(`NOTE_NAMES`)与各 source crate 的 `const` 表名字不一一对应、`source` 不同或同一 source 内 `n` 重复,`Venue`/`Measure`/`Housekeeping` 源的码名不以 `Venue`/`Measure`/`Hk` 开头(仓库里还没有任何 const 表项的 source——其 crate 尚未实现——不做一一对应核对);hft-ext-proto 的 `KINDS_EXT` 与 `KINDS` 编号重叠或越段;生成物与仓库不一致(仅 `--check`;含已不再生成的 `schemas/arrow/*.json`,写入模式删除它们)。策略参数 schema 的导出(每策略一个 JSON)不在本 crate,见 [tools hft-cfg](../../tools/README.md#hft-cfg)。

**const 表的形式**(gen-schema 以文本扫描读取,不链接 source crate):每个 source crate 在 `src/` 下声明 `pub const NOTE_CODES: &[(NoteSource, u32, &str)] = &[(NoteSource::Oms, 1, "OmsTransition"), …];`(`n` 写十进制或十六进制字面量);扫描范围是 `crates/*/src` 与 `apps/*/src`。**策略四项的来源**:`apps/hft-core` 在 workspace 中时运行已构建的 `hft-core schemas`(找不到二进制退出 2,规则归 [tools gen-schema](../../tools/README.md#cargo-xtask-gen-schema)),读 `index.json` 与各 schema JSON 的 `notes[].code`、`cmds[].op`、`sections[].tag`;`apps/hft-core` 尚不在 workspace 时跳过这一部分(运行输出注明),lock 中的策略行原样沿用基线。hft-ext-proto 的 `KINDS_EXT` 随该可选扩展启用时链接;扩展的 crate 已在仓库而 xtask 尚未链接它 → 退出 2。

**版本读取**:段头记录写者的 `(kind, schema_version, size)` 表([journal-format](../../docs/subsystems/journal-format.md#segmentheader));读者以段头表为准判断旧/新记录,`RecordView::read` 按 `min(len, size_of::<B>())` 拷贝并补零。

**规划源文件**

| 文件 | 职责 |
|---|---|
| `src/lib.rs` | 重导出、`Body` trait |
| `src/header.rs` | `RecordHeader`、`RecordFlags`、`record_len` |
| `src/kinds.rs` | `EventKind` 常量、`KINDS` 注册表、`KindInfo` |
| `src/market.rs` / `exec.rs` / `chain.rs` / `control.rs` / `system.rs` / `raw.rs` | 各域体定义(`event_body!`);`exec.rs` 含 `OrderFlags`、`ExternalFill`、`MarginReportHdr`、`AccountConfigReportHdr`/`LeverageItem`、`ExecDecision` 与 `TargetOrigin` 线上码;`system.rs` 含 `EngineClock`、`NoteLevel`、`NoteSource` 与 Note 码名登记表(`NoteName`、`VField`、`NOTE_NAMES`) |
| `src/timer.rs` | `TimerCode` 的打包与解包(定时器编码表的唯一实现) |
| `src/strategy.rs` | `AdviceHdr`、`AdviceRecord`、SPB1 容器(`StrategyParamsHdr`、`GroupSpec`、`LegDef`、`parse_spb1`;body 不解析) |
| `src/cmd.rs` | `CmdHdr`、`CmdKind`、`ControlCmd` 编解码 |
| `src/var.rs` | `PartHdr`、`L2SnapHdr`、`BlobHdr`、`RawHdr`、`SnapshotHdr`/`SectionHdr`;`multi_view`、`PartIter`(拆分)与 `Reassembler`(重组,调用方提供缓冲) |
| `src/codec.rs` | `encode_fixed`、`encode_var`、`decode_record`、`classify`、CRC32C(`crc32c` crate,ARMv8 CRC 指令) |
| `src/ring.rs` | `FeedMsg`、`RepMsg`、`HkMsg`、`OutMsg`、`GwCmd`、`FeedCmd`、`CtlRecord`、`AdviceMsg`(进程内环元素与 ctl、advice 帧) |
| `src/schema.rs` | `FieldDesc`/`EnumDesc`、`catalog()`(返回 `Catalog`:39 个生产 kind、`CmdKind`、`NOTE_NAMES`、SPB1 与分帧结构、全部码表、`MEASURE_MASK`;测试 kind 不导出) |
| `src/codes.rs` | 整数编码字段的码表与位集合(`EnumDesc`) |

</details>

## 延迟与资源契约

| 入口 | 预算 | 基准 |
|---|---|---|
| `encode_fixed`(≤ 128 B 体,写入已预触摸的缓冲) | p50 ≤ 10 ns,p99 ≤ 25 ns | `event_encode_80b` |
| `encode_var`(Advice,1 KiB 载荷) | p50 ≤ 40 ns,p99 ≤ 100 ns | `event_encode_advice_1k` |
| `decode_record` + `view`(无 CRC) | p50 ≤ 5 ns | `event_decode_view` |
| CRC32C | ≥ 10 GB/s(硬件指令);64 KiB 记录 ≤ 8 µs | `event_crc_64k` |
| `ControlCmd::decode` | 冷路径,≤ 1 µs | `event_cmd_decode` |
| `parse_spb1`(500 group) | 冷路径(ctl-gateway),≤ 50 µs | `event_spb1_parse_500` |

零分配(`Reassembler` 使用调用方缓冲);零系统调用;`KINDS` 查表为数组下标 + 编号段偏移。

## 测试与验收

用例与验收见 [TESTS.md](TESTS.md)。

## 依赖

- 内部:[hft-types](../hft-types/README.md)、[hft-refdata](../hft-refdata/README.md)(仅 `BlobKind::Refdata`/`Engine` 的编码号与 `schema.rs` 的 refdata 行导出;事件体用 u8 携带 refdata 枚举的编码,不引用 refdata 类型)。
- 外部:`zerocopy` 0.8(与 hft-types 同版本)、`crc32c`。
- 禁止:`serde`(线格式不用序列化框架)、SBE 工具链(内部编码只用本 crate,SBE 只用于解码交易所流,[13 O5](../../design/13-设计评审与优化.md))、任何 L1 及以上 crate(`RejectBits` 判定、`RunMode` 行为、`TxState` 状态机都在上层;本 crate 只存编码)、[hft-strategy-sdk](../hft-strategy-sdk/README.md) 与任何 `strategies/*`(SDK 依赖本 crate,反之不行)。

## 延伸阅读

- [event-model](../../docs/subsystems/event-model.md)(记录头、编号段与分类、关键体、演进规则)、[journal-format](../../docs/subsystems/journal-format.md)(多段、快照、段头 schema 表)、[control-plane](../../docs/subsystems/control-plane.md)(命令语义与权限)、[external-venue-protocol](../../docs/subsystems/external-venue-protocol.md)(optional:`PoolQuote`/`TxLifecycle`/`SwapDecision` 的跨进程承载)
- [hft-strategy-sdk](../hft-strategy-sdk/README.md)(`Model`、`ParamSet`、`ParamSchemaDesc`、`EventRef`)、[hft-exec](../hft-exec/README.md)(`ExecDecision` 的语义)、[hft-sim](../hft-sim/README.md)(模拟账户回报)
- [13 R5/R6/O5](../../design/13-设计评审与优化.md)、[08 §2](../../design/08-数据面与回放.md)、[09 §1、§5](../../design/09-可观测性、风控与控制面.md)、[research/06 §2](../../design/research/06-data-plane-replay-observability.md)(DBN 记录布局、SBE 演进原则);决策出处:[round8-decisions](../../design/_work/round8-decisions.md)
- 新增 kind:[cookbook/adding-an-event-kind.md](../../docs/cookbook/adding-an-event-kind.md);决策记录 `single-event-header`、`rust-first-event-schema`、`self-contained-journal`、`strategy-sdk-decoupling`([Agent Notes](../../.agents/notes/README.md))

## 已知限制与后续工作

- `Advice` 单条载荷 ≤ 4096 B;更大的模型输出(如全币分配表)由策略拆成多条并自行约定续条。若某策略经常需要 > 64 KiB 的一次性输出,可选扩展:把 `Advice` 改为 `Multi` 布局的新 kind(触发条件:拆分续条在策略侧造成可测的复杂度或延迟)。
- SPB1 头只描述 group 结构;参数字段的类型能力(当前只有标量与按 group 的标量覆盖)由 [hft-strategy-sdk `params`](../hft-strategy-sdk/README.md) 决定。body 编码升级时由 SDK 定义新编码,本 crate 只在需要区分时追加 `encoding` 号。
- `OrderDecision.reason` 只对执行层订单有值(`ExecOrderKind`,码表在 [hft-exec](../hft-exec/README.md));策略意图没有理由码(SDK `OrderIntent` 无此字段),写 0。可选扩展:SDK `OrderIntent` 追加理由码字段、各策略经 SDK 导出码表给 gen-schema(触发条件:运维需要按理由筛选或统计策略订单)。
- `ReconcileReport` 每差异项一条,单次对账上限 65 535 项(`count: u16`);资金费条目只发有差异的结算(48 h 窗口),正常情况下为 0 条。
- `InjectWarmup` 单块 ≤ 4096 B,每块一条 `ControlCmd` + `CmdResult` + 审计 `Note`(约 4.4 KiB journal);预热数据以 MiB 计时按块限速注入(限速在 [hft-ctl](../hft-ctl/README.md))。若某策略的预热数据常态超过数十 MiB,可选扩展:改为先写 `ConfigBlob` 再由命令引用(触发条件:注入耗时或 journal 占用成为冷启动瓶颈)。
- `MarginReport` 单条至多 1024 个条目;超出时置 `ITEMS_TRUNCATED`,hft-risk 按"数据不完整"处理(视为 L1)。`AccountConfigReport` 同样至多 1024 条杠杆条目,超出置 `LEV_TRUNCATED`,未列出的 inst 杠杆按"读不到"处理(规则归 hft-risk);某 venue 的永续宇宙超过 1024 个 inst 时,可选扩展:按 inst 分多条 `PARTIAL` 报告(触发条件:refdata 中单个永续 venue 的可交易 inst 超过 1024)。
- `EngineClock` 每次进程启动一条;引擎时间的精度受启动时 TSC 频率校准误差限制(≤ 1e-6 相对误差时,一天的运行累计偏差 ≤ 87 ms),它只用于跨重启的冷却与截止,不用于延迟测量。
- Arrow 导出不做十进制类型:`Px/Qty` 以 `Int64` + 元数据导出,十进制列在 journal-archiver 按分区生成。
- 金样用的规范 JSON 渲染 `to_canonical_json`([fixtures 金样文件格式](../../fixtures/README.md#金样文件格式))还不是 gen-schema 的生成物:它需要 refdata 的 venue 名与资产指数,随第一个 venue 解码金样驱动一起加入生成物清单;在此之前金样期望以手写结构体比较(fixtures 已注明)。

### 开发备忘

<details><summary>给实现者</summary>

- 实现顺序:`header.rs` → `kinds.rs` + `event_body!` → 各域体 → `codec.rs` → `var.rs` → `strategy.rs`(SPB1 容器先写失败用例)→ `cmd.rs` → `schema.rs` → xtask 生成器。
- 进程内环搬运的是类型化值(`FeedMsg`、`RepMsg` 等 `enum` 与定长 `GwCmd`,见[环元素类型](#环元素类型)),不要在进程内为了"统一"而先编码成字节再解码;Advice 与 ctl 是字节环,因为体是变长的。
- `Note.code` 的数值、`ExecDecision.detail` 与 `OrderDecision.reason`(`ExecOrderKind`)的码表由上层 crate(含策略 crate,经 SDK)以 `const` 表提供给 xtask;本 crate 只登记引擎 Note 的名字、来源与默认级别(上文登记表),不硬编码数值与 `v[]` 以外的含义。
- `AccountConfigReport` 的三个模式字段与 hft-refdata 的 `PositionMode`/`MarginMode`/`MultiAsset` 同码:单元测试逐值断言,防止两处编码漂移。
- `LeverageItem.leverage` 用 `Rate`(1e-9)而不是整数倍数:部分交易所允许小数杠杆(待核实:OKX `lever` 是否接受小数、Bitget 的步长;方法:各所设置杠杆接口文档与模拟盘实测),且全仓只有 `Rate` 一种比率类型。
- `ConnState{Alive}` 的产出频率上限由 feed 线程的规则保证(见 [event-model](../../docs/subsystems/event-model.md#关键事件体)),本 crate 不做节流。
- 开放问题:`HostProfile` 是否需要变长扩展(完整内核参数清单)——当前只存清单哈希,清单内容以 `ConfigBlob{HostManifest}` 写入,确认 watchtower 需要哪些字段后再定。

</details>
