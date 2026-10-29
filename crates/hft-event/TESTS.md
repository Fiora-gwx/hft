---
component: hft-event
prefix: EVT
spec: README.md
---

# hft-event 测试规格

格式与规则见 [docs/tests](../../docs/tests/README.md)。本文件同时覆盖本 crate 实现的 [event-model](../../docs/subsystems/event-model.md) 布局与演进规则(记录头、kind 编号与分类、关键事件体、`MarketHdr`)以及 [journal-format](../../docs/subsystems/journal-format.md) 中由本 crate 实现的类型(`PartHdr`、`SnapshotHdr`、`SectionHdr`、CRC 位置)。`gen-schema` 的导出与 `--check` 规则归本 crate([实现说明](README.md#实现说明)),其用例在本文件,实现落在 `tools/xtask/tests/`。

## 覆盖

| README 小节 | 用例 |
|---|---|
| 概要 | 不适用:非规范 |
| 目录 | 不适用:非规范 |
| 使用本组件 | 不适用:见子节 |
| 记录头、标志与 kind | EVT-001–EVT-008 |
| 体 trait 与编解码 | EVT-010–EVT-028、EVT-200 |
| 所有权与线程 | EVT-030 |
| 事件体布局 | EVT-040、EVT-050、EVT-060、EVT-070、EVT-074 |
| 行情(0x01xx) | EVT-040–EVT-041 |
| 执行(0x02xx) | EVT-050–EVT-055、EVT-205 |
| 链上(0x03xx) | EVT-060–EVT-061、EVT-178 |
| 控制与系统(0x04xx、0x05xx) | EVT-070–EVT-084 |
| cap 记录 | EVT-090–EVT-091 |
| 测量字段(验证回放屏蔽) | EVT-100–EVT-102 |
| 定时器编码 | EVT-110–EVT-113 |
| 策略载荷容器 | EVT-123 |
| Advice(0x0503,Var,Input) | EVT-120–EVT-122 |
| StrategyParams(SPB1) | EVT-130–EVT-133、EVT-203 |
| 环元素类型 | EVT-124, EVT-140–EVT-144 |
| 控制命令编码 | EVT-150–EVT-158、EVT-201 |
| 变长记录前缀 | EVT-160–EVT-165、EVT-202、EVT-204 |
| 实现说明 | EVT-170–EVT-180 |
| 延迟与资源契约 | EVT-190–EVT-196 |
| 测试与验收 | 不适用:非规范 |
| 依赖 | EVT-123 |
| 延伸阅读 | 不适用:非规范 |
| 已知限制与后续工作 | 不适用:非规范 |
| 开发备忘 | 不适用:非规范 |

## 用例

约定(全文适用,不再在每条用例重复):"布局"类表驱动用例对每行断言 `size_of`、`align_of` 与列出的每个 `offset_of`(编译期断言之外再由 `tests/layout.rs` 运行期断言一次,失败信息带行号);"往返"指 `encode_*` → `decode_record` → `read`/`view`/`var` 后逐字段相等且记录头 `kind`、`flags`、`seq`、`tsc` 原样;定点值写 raw 值。测试只经公开 API。

### EVT-001 记录框架结构的大小、对齐与字段偏移与规格一致

- 规则:[记录头、标志与 kind](README.md#记录头标志与-kind)、[event-model 记录头](../../docs/subsystems/event-model.md#记录头24-b全系统唯一)、[journal-format 多段记录](../../docs/subsystems/journal-format.md#变长与多段记录)、[快照记录结构](../../docs/subsystems/journal-format.md#快照记录结构)
- 级别:table
- 里程碑:M1
- 前置:无参数;目标 aarch64 与 x86_64 各编译一次(布局与目标无关)
- 实现:`crates/hft-event/tests/layout.rs::evt_001_frame_layouts`

| 行 | 结构 | 期望 |
|---|---|---|
| EVT-001.1 | `RecordHeader` | size 24、align 8;`len`@0、`kind`@4、`flags`@6、`seq`@8、`tsc`@16 |
| EVT-001.2 | `PartHdr` | size 16、align 4;`total_len`@0、`chunk_len`@4、`part`@8、`parts`@10、`_rsv`@12 |
| EVT-001.3 | `CtlRecordHdr` | size 8、align 4;`kind`@0、`flags`@2、`len`@4 |
| EVT-001.4 | `SnapshotHdr` | size 96、align 8;`purpose`@0、`n_sections`@4、`at_seq`@8、`state_hash`@16、`n_books`@32、`out_hash`@40、`snap_tsc`@56、`boot_id`@64、`snap_utc_ns`@80、`engine_ns`@88 |
| EVT-001.5 | `SectionHdr` | size 16、align 4;`section`@0、`version`@2、`count`@4、`len`@8、`_rsv`@12 |

### EVT-002 RecordFlags 七个位值两两不重叠且与规格一致

- 规则:[记录头、标志与 kind](README.md#记录头标志与-kind)
- 级别:table
- 里程碑:M1
- 前置:无参数
- 实现:`crates/hft-event/tests/kinds.rs::evt_002_record_flag_bits`

| 行 | 常量 | 期望 |
|---|---|---|
| EVT-002.1 | `CONFLATED` | `0x0001` |
| EVT-002.2 | `SHADOW` | `0x0002` |
| EVT-002.3 | `HW_TS` | `0x0004` |
| EVT-002.4 | `SW_TS` | `0x0008` |
| EVT-002.5 | `PART` | `0x0010` |
| EVT-002.6 | `CRC` | `0x0020` |
| EVT-002.7 | `EVENT_END` | `0x0040` |
| EVT-002.8 | 七个常量两两按位与 | 全部为 0;七个之或 = `0x007F`;`RecordFlags` 没有第八个具名常量(对模拟账户不设标志位) |

### EVT-003 未定义的标志位不影响解码且原样保留

- 规则:[event-model 记录头](../../docs/subsystems/event-model.md#记录头24-b全系统唯一)
- 级别:unit
- 里程碑:M1
- 前置:一条 `Bbo` 记录,`seq = 7`、`tsc = 1000`,`flags = EVENT_END | 0xFF80`(bit 7..15 全置位)
- 步骤:
  1. `encode_fixed::<Bbo>` 写入 128 B 缓冲
  2. `decode_record` 后 `read::<Bbo>()`
- 期望:
  - 步骤 2 返回 `Ok`;`hdr.flags == 0xFFC0`;`hdr.len == 120`;读出的 `Bbo` 与写入逐字段相等
- 实现:`crates/hft-event/tests/codec.rs::evt_003_unknown_flag_bits_ignored`

### EVT-004 KINDS 注册表逐 kind 与 event-model 编号表一致

- 规则:[记录头、标志与 kind](README.md#记录头标志与-kind)、[event-model kind 编号段](../../docs/subsystems/event-model.md#kind-编号段)
- 级别:table
- 里程碑:M1
- 前置:默认特性构建(不含 dev 测试 kind);每行断言 `kind_info(k)` 的 `name`、`domain`、`layout`、`role`、`schema_version`、`journals`、`crc`、`optional`;另断言 `KINDS.len() == 41`(下表即全集,没有为模拟账户另设的 kind,没有 `0x7Fxx`)
- 实现:`crates/hft-event/tests/kinds.rs::evt_004_kinds_registry`

| 行 | kind | 期望(name / domain / layout / role / version / journals / crc / optional) |
|---|---|---|
| EVT-004.1 | `0x0101` | `Bbo` / Market / Fixed 96 / Input / 1 / SEQ / Never / false |
| EVT-004.2 | `0x0102` | `L2Delta` / Market / Fixed 96 / Input / 1 / SEQ / Never / false |
| EVT-004.3 | `0x0103` | `L2Snapshot` / Market / Multi prefix 72 / ByBody / 1 / SEQ / Never / false |
| EVT-004.4 | `0x0104` | `Trade` / Market / Fixed 88 / Input / 1 / SEQ / Never / false |
| EVT-004.5 | `0x0105` | `Funding` / Market / Fixed 80 / Input / 1 / SEQ / Never / false |
| EVT-004.6 | `0x0106` | `MarkPrice` / Market / Fixed 64 / Input / 1 / SEQ / Never / false |
| EVT-004.7 | `0x0107` | `IndexPrice` / Market / Fixed 64 / Input / 1 / SEQ / Never / false |
| EVT-004.8 | `0x0108` | `Liquidation` / Market / Fixed 80 / Input / 1 / SEQ / Never / false |
| EVT-004.9 | `0x0109` | `L2Batch` / Market / Var prefix 80 max 64 000 / Input / 1 / SEQ / Never / false |
| EVT-004.10 | `0x0201` | `OrderDecision` / Exec / Fixed 88 / Output / 1 / SEQ / Never / false |
| EVT-004.11 | `0x0202` | `SendReport` / Exec / Fixed 48 / Input / 1 / SEQ / Never / false |
| EVT-004.12 | `0x0203` | `OrderEvent` / Exec / Fixed 136 / Input / 4 / SEQ / Never / false |
| EVT-004.13 | `0x0204` | `PositionReport` / Exec / Fixed 56 / Input / 1 / SEQ / Never / false |
| EVT-004.14 | `0x0205` | `BalanceReport` / Exec / Fixed 40 / Input / 1 / SEQ / Never / false |
| EVT-004.15 | `0x0206` | `FundingSettle` / Exec / Fixed 56 / ByBody / 2 / SEQ / Never / false |
| EVT-004.16 | `0x0207` | `ExternalFill` / Exec / Fixed 96 / Input / 3 / SEQ / Never / false |
| EVT-004.17 | `0x0208` | `MarginReport` / Exec / Var prefix 64 max 16 384 / Input / 1 / SEQ / Never / false |
| EVT-004.18 | `0x0209` | `AccountConfigReport` / Exec / Var prefix 64 max 16 384 / Input / 1 / SEQ / Never / false |
| EVT-004.19 | `0x020A` | `ExecDecision` / Exec / Fixed 104 / Output / 1 / SEQ / Never / false |
| EVT-004.20 | `0x0301` | `ChainHead` / Chain / Fixed 80 / Input / 1 / SEQ / Never / true |
| EVT-004.21 | `0x0302` | `PoolQuote` / Chain / Fixed 96 / Input / 1 / SEQ / Never / true |
| EVT-004.22 | `0x0303` | `SwapDecision` / Chain / Fixed 128 / Output / 1 / SEQ / Never / true |
| EVT-004.23 | `0x0304` | `TxLifecycle` / Chain / Fixed 152 / Input / 1 / SEQ / Never / true |
| EVT-004.24 | `0x0401` | `ControlCmd` / Control / Multi prefix 128 / Input / 1 / SEQ / Always / false |
| EVT-004.25 | `0x0402` | `CmdResult` / Control / Fixed 144 / Output / 3 / SEQ / Never / false |
| EVT-004.26 | `0x0403` | `ConfigBlob` / Control / Multi prefix 72 / ByBody / 1 / SEQ \| CHAIN / Always / false |
| EVT-004.40 | `0x0405` | `ReconcileBatch` / Control / Var 16 + ≤51200 / Input / 1 / SEQ / Never / false |
| EVT-004.27 | `0x0404` | `ReconcileReport` / Control / Fixed 104 / Input / 3 / SEQ / Never / false |
| EVT-004.28 | `0x0501` | `Timer` / System / Fixed 24 / Input / 1 / SEQ / Never / false |
| EVT-004.29 | `0x0502` | `ConnState` / System / Fixed 24 / Input / 2 / SEQ / Never / false |
| EVT-004.30 | `0x0503` | `Advice` / System / Var prefix 32 max 4 096 / Input / 1 / SEQ / Never / false |
| EVT-004.31 | `0x0504` | `StateSnapshot` / System / Multi prefix 96 / Output / 1 / SEQ \| CHAIN / Always / false |
| EVT-004.32 | `0x0505` | `LatencySample` / System / Fixed 136 / Meta / 3 / SEQ / Never / false |
| EVT-004.33 | `0x0506` | `Note` / System / Fixed 80 / ByBody / 1 / SEQ / Never / false |
| EVT-004.34 | `0x0507` | `HostProfile` / System / Fixed 176 / Meta / 1 / SEQ \| CHAIN / Never / false |
| EVT-004.35 | `0x0508` | `ClockStatus` / System / Fixed 56 / Meta / 1 / SEQ \| CHAIN / Never / false |
| EVT-004.36 | `0x0509` | `SegmentRoll` / System / Fixed 48 / Meta / 1 / SEQ \| RAW \| CHAIN / Never / false |
| EVT-004.37 | `0x050A` | `EngineClock` / System / Fixed 32 / Input / 1 / SEQ / Never / false |
| EVT-004.38 | `0x0601` | `RawRx` / Raw / Multi prefix 24 / Meta / 1 / RAW / Always / false |
| EVT-004.39 | `0x0602` | `RawTx` / Raw / Multi prefix 24 / Meta / 1 / RAW / Always / false |
| EVT-004.41 | `0x050B` | `UtcAnchor` / System / Fixed 32 / Input / 1 / SEQ / Never / false |

### EVT-005 KINDS 与 Body 实现一一对应、编号唯一、按编号段归域

- 规则:[记录头、标志与 kind](README.md#记录头标志与-kind)、[实现说明](README.md#实现说明)、[event-model kind 编号段](../../docs/subsystems/event-model.md#kind-编号段)
- 级别:unit
- 里程碑:M1
- 前置:`KINDS` 与 `tests/bodies.rs` 中列出的全部 `Body` 实现(定长体与 Var 前缀)
- 步骤:
  1. 收集每个 `Body` 的 `(KIND, SCHEMA_VERSION, size_of)`
  2. 遍历 `KINDS`
- 期望:
  - 每个 `Layout::Fixed` 的 kind 恰有一个 `Body` 且 `size` 等于 `size_of`、`schema_version` 等于 `SCHEMA_VERSION`;每个 `Var` 的 kind 恰有一个前缀 `Body` 且 `prefix` 等于其 `size_of`;没有 `KIND` 未登记的 `Body`
  - `KINDS` 内编号两两不同;编号高字节 → 域:`0x01` Market、`0x02` Exec、`0x03` Chain、`0x04` Control、`0x05` System、`0x06` Raw;没有编号落在 `0x0310..=0x033F`(该段归 hft-ext-proto)
  - 每个生产 kind 的 `fields` 非空(测试 kind 的体不透明,不参与本条),`fields` 的末项偏移 + 宽度 = `size`(定长)或 `prefix`(Var/Multi)
- 实现:`crates/hft-event/tests/kinds.rs::evt_005_kinds_match_bodies`

### EVT-006 kind_info 对已登记编号返回条目、对其余编号返回 None

- 规则:[记录头、标志与 kind](README.md#记录头标志与-kind)
- 级别:table
- 里程碑:M1
- 前置:默认特性构建
- 实现:`crates/hft-event/tests/kinds.rs::evt_006_kind_info_lookup`

| 行 | 输入 | 期望 |
|---|---|---|
| EVT-006.1 | `0x0101` | `Some`,`name == "Bbo"`,`kind == 0x0101` |
| EVT-006.2 | `0x050A` | `Some`,`name == "EngineClock"` |
| EVT-006.3 | `0x0000` | `None` |
| EVT-006.4 | `0x010A`(行情段未分配) | `None` |
| EVT-006.5 | `0x020B`(执行段未分配) | `None` |
| EVT-006.6 | `0x0310`(hft-ext-proto 段) | `None` |
| EVT-006.7 | `0x050C`(系统段未分配) | `None` |
| EVT-006.8 | `0x7F01` | `None`(生产构建无测试 kind) |
| EVT-006.9 | `0xFFFF` | `None` |

### EVT-007 classify 按 kind 与判定字段给出记录的实际类别

- 规则:[记录头、标志与 kind](README.md#记录头标志与-kind)、[Note 来源与码登记表](README.md#note-来源与码登记表)
- 级别:table
- 里程碑:M1
- 前置:每行构造对应 kind 的合法逻辑体,只改"输入"列给出的判定字段
- 实现:`crates/hft-event/tests/kinds.rs::evt_007_classify`

| 行 | 输入 | 期望 |
|---|---|---|
| EVT-007.1 | `Bbo` | `Ok(Input)` |
| EVT-007.2 | `OrderDecision` | `Ok(Output)` |
| EVT-007.3 | `ExecDecision` | `Ok(Output)` |
| EVT-007.4 | `LatencySample` | `Ok(Meta)` |
| EVT-007.5 | `SegmentRoll` | `Ok(Meta)` |
| EVT-007.6 | `RawRx` | `Ok(Meta)` |
| EVT-007.7 | `EngineClock` | `Ok(Input)` |
| EVT-007.8 | `L2Snapshot`,`origin = 1`(Exchange) | `Ok(Input)` |
| EVT-007.9 | `L2Snapshot`,`origin = 2`(Keyframe) | `Ok(Output)` |
| EVT-007.10 | `L2Snapshot`,`origin = 3`(TopN) | `Ok(Input)` |
| EVT-007.11 | `L2Snapshot`,`origin = 0` | `Err(InvalidEnum)` |
| EVT-007.12 | `L2Snapshot`,`origin = 4` | `Err(InvalidEnum)` |
| EVT-007.13 | `ConfigBlob`,`boot = 0` | `Ok(Input)` |
| EVT-007.14 | `ConfigBlob`,`boot = 1` | `Ok(Output)` |
| EVT-007.15 | `ConfigBlob`,`boot = 2` | `Err(InvalidEnum)` |
| EVT-007.16 | `FundingSettle`,`synth = 0` | `Ok(Input)` |
| EVT-007.17 | `FundingSettle`,`synth = 1` | `Ok(Output)` |
| EVT-007.18 | `FundingSettle`,`synth = 2` | `Err(InvalidEnum)` |
| EVT-007.19 | `Note`,`source` = 1、2、3、4、9、10(Sequencer、Strategy、Risk、Oms、Ledger、Exec) | 各为 `Ok(Output)` |
| EVT-007.20 | `Note`,`source` = 5、6、8、12(Venue、CtlAudit、Housekeeping、Advisor) | 各为 `Ok(Meta)` |
| EVT-007.21 | `Note`,`source` = 7、11(Measure、Boot) | 各为 `Ok(Meta)` |
| EVT-007.22 | `Note`,`source = 0` | `Err(InvalidEnum)` |
| EVT-007.23 | `Note`,`source = 13` | `Err(InvalidEnum)` |
| EVT-007.24 | 未登记 kind `0x010A`,任意 16 B 体 | `Err(UnknownKind)` |
| EVT-007.25 | `FundingSettle` v1 体(48 B,无 `synth` 字节) | `Ok(Input)`(缺失字节按 0) |

### EVT-008 启用 dev-kinds 时登记六个测试 kind

- 规则:[记录头、标志与 kind](README.md#记录头标志与-kind)
- 级别:table
- 里程碑:M1
- 前置:以 `--features dev-kinds` 构建测试;不启用时的结果由 EVT-006.8 覆盖
- 实现:`crates/hft-event/tests/dev_kinds.rs::evt_008_dev_kinds_registered`

| 行 | 输入 | 期望 |
|---|---|---|
| EVT-008.1 | `kind_info(0x7F01)` | `name == "TIn"`、`Fixed{size: 80}`、`role == Input`、`crc == Never`、`domain == Test`、`journals == SEQ \| RAW \| CHAIN` |
| EVT-008.2 | `kind_info(0x7F02)` | `TOut`、`Fixed{size: 56}`、`Output`、`Never` |
| EVT-008.3 | `kind_info(0x7F03)` | `TMeta`、`Fixed{size: 32}`、`Meta`、`Never` |
| EVT-008.4 | `kind_info(0x7F04)` | `TCrc`、`Fixed{size: 40}`、`Input`、`Always`;`record_len(40, true) == 72` |
| EVT-008.5 | `kind_info(0x7F05)` | `TMulti`、`Multi{prefix: 0}`、`Input`、`Never` |
| EVT-008.6 | `kind_info(0x7F06)` | `TVar`、`Var{prefix: 16, max: 65 000}`、`Input`、`Never` |
| EVT-008.7 | `kind_info(0x7F07)`、`kind_info(0x7F00)` | `None` |
| EVT-008.8 | `gen-schema` 在启用 dev-kinds 的构建上导出 | `schemas/event-layout.lock` 与生成物中没有 `0x7Fxx` |

### EVT-010 record_len 按 24 + 体 (+4) 向上取 8

- 规则:[体 trait 与编解码](README.md#体-trait-与编解码)、[journal-format 记录对齐与 CRC](../../docs/subsystems/journal-format.md#记录对齐与-crc)
- 级别:table
- 里程碑:M1
- 前置:无参数
- 实现:`crates/hft-event/tests/codec.rs::evt_010_record_len`

| 行 | 输入 `(body_len, crc)` | 期望 |
|---|---|---|
| EVT-010.1 | `(0, false)` | 24 |
| EVT-010.2 | `(1, false)` | 32 |
| EVT-010.3 | `(8, false)` | 32 |
| EVT-010.4 | `(80, false)` | 104 |
| EVT-010.5 | `(80, true)` | 112 |
| EVT-010.6 | `(84, true)` | 112 |
| EVT-010.7 | `(85, true)` | 120 |
| EVT-010.8 | `(0, true)` | 32 |
| EVT-010.9 | `(65 512, false)` | 65 536 |
| EVT-010.10 | `(65 508, true)` | 65 536 |

### EVT-011 任意合法定长体编码后解码逐字节相等

- 规则:[体 trait 与编解码](README.md#体-trait-与编解码)
- 级别:prop
- 里程碑:M1
- 性质:对 `KINDS` 中每个 `Layout::Fixed` 的 kind,任意合法体 b、任意 `seq`、`tsc`、`flags ⊆ {CONFLATED, SHADOW, HW_TS, SW_TS, CRC, EVENT_END}`:`n = encode_fixed(out, seq, tsc, flags, &b)` 后 `n == record_len(size_of, flags 含 CRC)`;`decode_record(&out[..n])` 成功;`read::<B>() == b`;`view::<B>()` 指向的值 `== b`;记录头 `kind == B::KIND`、`seq`、`tsc`、`flags` 原样
- 生成:每个 kind 一个生成器:整数字段取全域,枚举字段只取合法判别值,`_pad*`/`_rsv*` 恒 0,`Px`/`Qty` 取 `i64` 全域;按 kind 均匀抽取
- 参考:写入值本身;CRC 记录另以 `crc32c` crate 独立计算校验和比对末 4 字节
- 例数:PR 每 kind 256(合计约 9 000)
- 实现:`crates/hft-event/tests/prop_codec.rs::evt_011_fixed_roundtrip`

### EVT-012 encode_fixed 写出 B::KIND 与长度且不写出界

- 规则:[体 trait 与编解码](README.md#体-trait-与编解码)
- 级别:unit
- 里程碑:M1
- 前置:256 B 缓冲全部预填 `0xAA`;一条 `OrderDecision`(`px = 100`、`qty = 3`、`action = New`、`side = Buy`、`tif = Gtc`,其余 0),`seq = 42`、`tsc = 9`、`flags = 0`
- 步骤:
  1. `n = encode_fixed(&mut buf, 42, 9, RecordFlags(0), &od)`
- 期望:
  - `n == 112`;`buf[0..4]` 小端 = 112;`buf[4..6]` = `0x0201`;`buf[6..8]` = 0;`buf[8..16]` = 42;`buf[16..24]` = 9
  - `buf[112..256]` 全部仍为 `0xAA`
- 实现:`crates/hft-event/tests/codec.rs::evt_012_encode_fixed_header_and_bounds`

### EVT-013 CRC 位置位时校验和在末 4 字节且体末与 CRC 之间为零

- 规则:[体 trait 与编解码](README.md#体-trait-与编解码)、[journal-format 记录对齐与 CRC](../../docs/subsystems/journal-format.md#记录对齐与-crc)
- 级别:unit
- 里程碑:M1
- 前置:256 B 缓冲预填 `0xAA`;一条 `Bbo`,`flags = CRC | EVENT_END`
- 步骤:
  1. `n = encode_fixed(...)`
  2. 独立计算 `crc32c(&buf[0..n − 4])`
- 期望:
  - `n == 128`;`buf[0..4]` = 128;`buf[120..124]` 全 0;`buf[124..128]` 小端 = 步骤 2 的值;`buf[128..]` 仍为 `0xAA`
  - `decode_record(&buf[..128])` 成功,`body.len() == 100`(体 96 + 4 B 零填充)
- 实现:`crates/hft-event/tests/codec.rs::evt_013_crc_placement`

### EVT-014 encode_fixed 的输出缓冲不足时 debug 构建断言失败

- 规则:[体 trait 与编解码](README.md#体-trait-与编解码)
- 级别:unit
- 里程碑:M1
- 前置:`debug_assertions` 开启;一条 `Bbo`;缓冲长度 119(需要 120)
- 步骤:
  1. `encode_fixed(&mut buf[..119], 1, 1, RecordFlags(0), &bbo)`
- 期望:
  - 线程以 debug_assert 失败结束(`#[should_panic]`),缓冲 `[0..119)` 之外无写入
- 实现:`crates/hft-event/tests/codec.rs::evt_014_encode_fixed_short_buffer_asserts`

### EVT-015 decode_record 拒绝长度与对齐不合法的记录

- 规则:[体 trait 与编解码](README.md#体-trait-与编解码)、[journal-format 记录对齐与 CRC](../../docs/subsystems/journal-format.md#记录对齐与-crc)
- 级别:table
- 里程碑:M1
- 前置:每行从一条合法 `Timer` 记录(`len = 48`,无 CRC)出发,只改"输入"列所述字节或缓冲长度
- 实现:`crates/hft-event/tests/codec.rs::evt_015_decode_record_bounds`

| 行 | 输入 | 期望 |
|---|---|---|
| EVT-015.1 | 原记录,缓冲 48 B | `Ok`,`body.len() == 24` |
| EVT-015.2 | 缓冲截为 23 B | `Err(Short)` |
| EVT-015.3 | `len = 0` | `Err(Short)` |
| EVT-015.4 | `len = 16` | `Err(Short)` |
| EVT-015.5 | `len = 56`,缓冲仍 48 B | `Err(Short)` |
| EVT-015.6 | `len = 44` | `Err(Misaligned)` |
| EVT-015.7 | `len = 65 544`,缓冲 65 544 B | `Err(TooLong)` |
| EVT-015.8 | `len = 65 536`,缓冲 65 536 B(未登记 kind `0x010A`,体全 0) | `Ok`,`body.len() == 65 512` |
| EVT-015.9 | `len = 24`,`flags = CRC` | `Err(Short)` |
| EVT-015.10 | `len = 24`,无 CRC | `Ok`,`body.len() == 0` |

### EVT-016 CRC 不符的记录被拒绝,无 CRC 位的记录不做校验

- 规则:[体 trait 与编解码](README.md#体-trait-与编解码)
- 级别:table
- 里程碑:M1
- 前置:一条带 `CRC` 的 `Bbo` 记录 R(128 B,EVT-013 的产物)
- 实现:`crates/hft-event/tests/codec.rs::evt_016_crc_check`

| 行 | 输入 | 期望 |
|---|---|---|
| EVT-016.1 | R 原样 | `Ok` |
| EVT-016.2 | R 体内偏移 30 的一位翻转 | `Err(BadCrc)` |
| EVT-016.3 | R 记录头 `seq` 的最低位翻转 | `Err(BadCrc)`(校验覆盖记录头) |
| EVT-016.4 | R 的 CRC 字段本身一位翻转 | `Err(BadCrc)` |
| EVT-016.5 | R 体末与 CRC 之间的 4 B 填充改为 1 | `Err(BadCrc)` |
| EVT-016.6 | 无 `CRC` 位的 `Bbo` 记录,末 4 字节任意改动 | `Ok`(不校验) |

### EVT-017 未登记 kind 的记录可按 len 跳过而不影响下一条

- 规则:[体 trait 与编解码](README.md#体-trait-与编解码)、[记录头、标志与 kind](README.md#记录头标志与-kind)
- 级别:unit
- 里程碑:M1
- 前置:缓冲依次放三条记录:`Timer` 记录(`len = 48`)、未登记 kind `0x010A` 的记录(`len = 40`,体 16 B 任意)、`Bbo` 记录(`len = 120`)
- 步骤:
  1. 从偏移 0 起循环:`decode_record`,`kind_info(hdr.kind)` 为 `None` 时按 `hdr.len` 前进,否则 `read` 对应体再前进
- 期望:
  - 第 2 条 `decode_record` 返回 `Ok`、`kind_info` 为 `None`;第 3 条在偏移 88 处解码为 `Bbo` 且与写入值相等;共前进 208 B
- 实现:`crates/hft-event/tests/codec.rs::evt_017_unknown_kind_skippable`

### EVT-018 read 与 view 拒绝 kind 不符的记录

- 规则:[体 trait 与编解码](README.md#体-trait-与编解码)
- 级别:unit
- 里程碑:M1
- 前置:一条 `Trade` 记录(体 88 B)与一条 `Bbo` 记录
- 步骤:
  1. 对 `Trade` 记录调 `read::<Bbo>()`、`view::<Bbo>()`
  2. 对 `Bbo` 记录调 `var::<AdviceHdr>()`
- 期望:
  - 步骤 1 两次都返回 `Err(KindMismatch)`
  - 步骤 2 返回 `Err(KindMismatch)`
- 实现:`crates/hft-event/tests/codec.rs::evt_018_kind_mismatch`

### EVT-019 read 对旧写者的短体补零、对新写者的长体截断

- 规则:[体 trait 与编解码](README.md#体-trait-与编解码)、[event-model 演进规则](../../docs/subsystems/event-model.md#演进规则)
- 级别:table
- 里程碑:M1
- 前置:每行手工构造记录头 `kind` 正确、`len = 24 + 体长`的记录;体的已给出部分取一个非零样本
- 实现:`crates/hft-event/tests/codec.rs::evt_019_read_short_long_bodies`

| 行 | 输入 | 期望 |
|---|---|---|
| EVT-019.1 | `Bbo` 体 88 B(缺 `update_id`) | `read` 为 `Ok`;前 88 B 字段与样本相同,`update_id == 0` |
| EVT-019.2 | `Bbo` 体 104 B(末 8 B 为 `0xFF`) | `read` 为 `Ok`,与前 96 B 的样本相同;`view` 为 `Err(TooLong)` |
| EVT-019.3 | `Bbo` 体 0 B | `read` 为 `Ok`,全部字段为 0 |
| EVT-019.4 | `OrderDecision` 体 56 B(到 `strategy` 之前) | `read` 为 `Err(InvalidEnum)`(补零后 `action = 0`) |
| EVT-019.5 | `Timer` 体 16 B | `read` 为 `Ok`,`deadline_tsc == 0` |

### EVT-020 view 要求记录长度恰好等于当前版本的记录长

- 规则:[体 trait 与编解码](README.md#体-trait-与编解码)
- 级别:table
- 里程碑:M1
- 前置:8 B 对齐的缓冲
- 实现:`crates/hft-event/tests/codec.rs::evt_020_view_exact_len`

| 行 | 输入 | 期望 |
|---|---|---|
| EVT-020.1 | `Bbo`,`len = 120` | `Ok`,`&Bbo` 指向缓冲偏移 24 |
| EVT-020.2 | `Bbo`,`len = 112`(体 88) | `Err(Short)` |
| EVT-020.3 | `Bbo`,`len = 128`,无 CRC(体 104) | `Err(TooLong)` |
| EVT-020.4 | `Bbo`,`len = 128`,带 CRC | `Ok` |
| EVT-020.5 | `ExternalFill`,`len = 96`(v1 体 72) | `Err(Short)`;同一记录 `read` 为 `Ok`,`fill_seq == 0` |

### EVT-021 view 在体地址未 8 B 对齐时拒绝而 read 照常

- 规则:[体 trait 与编解码](README.md#体-trait-与编解码)
- 级别:unit
- 里程碑:M1
- 前置:一条 `Bbo` 记录拷贝到 `Vec<u64>` 背书的缓冲的字节偏移 4 处
- 步骤:
  1. 对偏移 4 起的切片 `decode_record` 后调 `view::<Bbo>()`
  2. 同一 `RecordView` 调 `read::<Bbo>()`
- 期望:
  - 步骤 1 的 `decode_record` 为 `Ok`,`view` 为 `Err(Misaligned)`
  - 步骤 2 为 `Ok` 且与写入值相等
- 实现:`crates/hft-event/tests/codec.rs::evt_021_view_misaligned`

### EVT-022 枚举字段为 0 或越界时 read 与 view 返回 InvalidEnum

- 规则:[体 trait 与编解码](README.md#体-trait-与编解码)、[执行(0x02xx)](README.md#执行0x02xx)、[控制与系统](README.md#控制与系统0x04xx0x05xx)、[event-model 关键事件体](../../docs/subsystems/event-model.md#关键事件体)
- 级别:table
- 里程碑:M1
- 前置:每行从该 kind 的一条合法记录出发,只改所列字段的字节;每行对边界内的最大合法值另断言 `Ok`
- 实现:`crates/hft-event/tests/codec.rs::evt_022_invalid_enums`

| 行 | 输入(字段 = 值) | 期望 |
|---|---|---|
| EVT-022.1 | `OrderDecision.action` = 0 / 6;= 5 | `InvalidEnum` / `InvalidEnum`;5 → `Ok` |
| EVT-022.2 | `OrderDecision.side` = 0 / 3;= 2 | `InvalidEnum` / `InvalidEnum`;`Ok` |
| EVT-022.3 | `OrderDecision.tif` = 0 / 5;= 4 | `InvalidEnum` / `InvalidEnum`;`Ok` |
| EVT-022.4 | `SendReport.status` = 0 / 9;= 8 | `InvalidEnum` / `InvalidEnum`;`Ok` |
| EVT-022.5 | `SendReport.op` = 0 / 19;= 18 | `InvalidEnum` / `InvalidEnum`;`Ok` |
| EVT-022.6 | `OrderEvent.what` = 0 / 13;= 12 | `InvalidEnum` / `InvalidEnum`;`Ok` |
| EVT-022.7 | `ExternalFill.side` = 0;`cause` = 0 / 6;`cause` = 5 | `InvalidEnum`;`InvalidEnum` / `InvalidEnum`;`Ok` |
| EVT-022.8 | `L2Delta.side` = 0;`Trade.taker_side` = 3;`Liquidation.side` = 0 | 各为 `InvalidEnum` |
| EVT-022.9 | `Note.level` = 0 / 5;= 4 | `InvalidEnum` / `InvalidEnum`;`Ok` |
| EVT-022.10 | `Note.source` = 0 / 13;= 12 | `InvalidEnum` / `InvalidEnum`;`Ok` |
| EVT-022.11 | `ConnState.state` = 0 / 7;= 6 | `InvalidEnum` / `InvalidEnum`;`Ok` |
| EVT-022.12 | `ConnState.role` = 0 / 5;= 4 | `InvalidEnum` / `InvalidEnum`;`Ok` |
| EVT-022.13 | `TxLifecycle.state` = 0 / 7;= 6 | `InvalidEnum` / `InvalidEnum`;`Ok` |
| EVT-022.14 | `SwapDecision.side` = 0 | `InvalidEnum` |

### EVT-023 var 按前缀计数校验尾部长度并返回恰为 L 的尾部

- 规则:[体 trait 与编解码](README.md#体-trait-与编解码)
- 级别:table
- 里程碑:M1
- 前置:每行手工构造 Var 记录,记录头 `len = 24 + 体长`(体长按行给出,8 的倍数)
- 实现:`crates/hft-event/tests/codec.rs::evt_023_var_tail_length`

| 行 | 输入 | 期望 |
|---|---|---|
| EVT-023.1 | `Advice`,`len = 0`,体 32 | `Ok`,尾部长 0 |
| EVT-023.2 | `Advice`,`len = 5`,体 40(5 B 载荷 + 3 B 填充为 `0x77`) | `Ok`,尾部长 5、等于载荷;填充不检查 |
| EVT-023.3 | `Advice`,`len = 5`,体 32 | `Err(Short)` |
| EVT-023.4 | `Advice`,`len = 5`,体 48 | `Err(TooLong)` |
| EVT-023.5 | `MarginReport`,`n = 3`,体 112 | `Ok`,尾部 48 B = 3 条 `MarginItem` |
| EVT-023.6 | `MarginReport`,`n = 3`,体 96 | `Err(Short)` |
| EVT-023.7 | `AccountConfigReport`,`n_lev = 0`,体 64 | `Ok`,尾部长 0 |
| EVT-023.8 | `AccountConfigReport`,`n_lev = 2`,体 104 | `Err(TooLong)` |
| EVT-023.9 | `L2Batch`,`n_bids = 2`、`n_asks = 3`,体 160 | `Ok`,尾部 80 B |
| EVT-023.10 | `L2Batch`,`n_bids = 2`、`n_asks = 3`,体 144 | `Err(Short)` |
| EVT-023.11 | `MarginReport` 体 56(短于前缀) | `Err(Short)` |

### EVT-024 encode_var 拒绝计数不符、超上限与缓冲不足

- 规则:[体 trait 与编解码](README.md#体-trait-与编解码)
- 级别:table
- 里程碑:M1
- 前置:输出缓冲 70 000 B(第 4 行除外)
- 实现:`crates/hft-event/tests/codec.rs::evt_024_encode_var_errors`

| 行 | 输入 | 期望 |
|---|---|---|
| EVT-024.1 | `AdviceHdr.len = 10`,`tail.len() = 10` | `Ok(72)`(24 + 32 + 16) |
| EVT-024.2 | `AdviceHdr.len = 10`,`tail.len() = 11` | `Err(Count)` |
| EVT-024.3 | `AdviceHdr.len = 4 097`,`tail.len() = 4 097` | `Err(TooLong)` |
| EVT-024.4 | `AdviceHdr.len = 10`,缓冲 71 B | `Err(BufTooSmall)` |
| EVT-024.5 | `MarginReportHdr.n = 2`,`tail.len() = 16` | `Err(Count)` |
| EVT-024.6 | `L2BatchHdr.n_bids + n_asks = 4 000`,`tail.len() = 64 000` | `Ok(64 104)`(24 + 80 + 64 000) |

### EVT-025 任意合法 Var 记录编码后解码相等

- 规则:[体 trait 与编解码](README.md#体-trait-与编解码)
- 级别:prop
- 里程碑:M1
- 性质:`encode_var` 后 `decode_record` → `var::<P>()` 得到的前缀与尾部逐字节等于输入;填充字节为 0;`len == record_len(size_of::<P>() + L, crc)`
- 生成:`Advice` 载荷长 0..=4096(随机字节);`MarginReport` `n` 0..=1024;`AccountConfigReport` `n_lev` 0..=1024;`L2Batch` `n_bids + n_asks` 0..=4000;各前缀字段取合法值;`flags` 随机含或不含 `CRC`
- 参考:输入本身
- 例数:PR 每 kind 256
- 实现:`crates/hft-event/tests/prop_codec.rs::evt_025_var_roundtrip`

### EVT-026 Var 尾部上限取边界值时接受、超一档时拒绝

- 规则:[体 trait 与编解码](README.md#体-trait-与编解码)、[实现说明](README.md#实现说明)
- 级别:table
- 里程碑:M1
- 前置:输出缓冲 70 000 B
- 实现:`crates/hft-event/tests/codec.rs::evt_026_var_max_bounds`

| 行 | 输入 | 期望 |
|---|---|---|
| EVT-026.1 | `Advice` 载荷 4 095 / 4 096 / 4 097 B | `Ok` / `Ok` / `Err(TooLong)` |
| EVT-026.2 | `MarginReport` `n` = 1 023 / 1 024 / 1 025 | `Ok` / `Ok` / `Err(TooLong)` |
| EVT-026.3 | `AccountConfigReport` `n_lev` = 1 023 / 1 024 / 1 025 | `Ok` / `Ok` / `Err(TooLong)` |
| EVT-026.4 | `L2Batch` `n_bids + n_asks` = 3 999 / 4 000 / 4 001 | `Ok` / `Ok` / `Err(TooLong)` |

### EVT-027 大端目标上编译 hft-event 失败

- 规则:[体 trait 与编解码](README.md#体-trait-与编解码)
- 级别:proc
- 里程碑:M1
- 前置:CI 的 `ubuntu-latest`;`rustup target add powerpc64-unknown-linux-gnu`
- 步骤:
  1. `cargo check -p hft-event --target powerpc64-unknown-linux-gnu`
- 期望:
  - 退出码 ≠ 0;stderr 的第一条 `error` 指向 `crates/hft-event/src/lib.rs` 中以 `cfg(target_endian = "big")` 守卫的 `compile_error!`;同一命令对 `aarch64-unknown-linux-gnu` 退出码为 0
- 实现:`crates/hft-event/tests/endian.rs::evt_027_big_endian_build_fails`

### EVT-028 离线精确体读取保留业务尾零并拒绝错误填充

- 规则:[体 trait 与编解码](README.md#体-trait-与编解码)
- 级别:table
- 里程碑:M2
- 前置:当前事件 schema,分别带/不带 CRC 的单条记录
- 输入:OrderDecision 定长体;Advice 的 5 B 载荷 `[7, 0, 2, 0, 0]`;分别带/不带 CRC
- 步骤:经真实编解码调用 exact_body;副本修改对齐填充、记录长与 kind
- 期望:定长体逐字节相同,Advice 逻辑体恰为 37 B 且尾部两个零保留;非零填充为 BadPadding,短/长记录为 Short/TooLong;Multi 为 BadPart,未知 kind 为 UnknownKind
- 实现:`crates/hft-event/tests/codec.rs::evt_028_exact_body_preserves_payload_zeros`

| 行 | 记录或改动 | 期望 |
|---|---|---|
| EVT-028.1 | 定长 OrderDecision | 精确体与原始体相等 |
| EVT-028.2 | 5 B Advice 载荷 | 37 B 逻辑体,两个尾零保留 |
| EVT-028.3 | 非零传输填充 | BadPadding |
| EVT-028.4 | 记录长加 8 B | TooLong |
| EVT-028.5 | 记录长减 8 B | Short |
| EVT-028.6 | Multi kind | BadPart,要求先重组 |
| EVT-028.7 | 未知 kind | UnknownKind |

### EVT-030 编解码无内部状态:多线程并发编码与单线程逐字节相同

- 规则:[所有权与线程](README.md#所有权与线程)
- 级别:unit
- 里程碑:M1
- 前置:1 000 条固定样本(每个定长 kind 若干条,`seq` 递增)
- 步骤:
  1. 单线程依次编码,得字节串 S
  2. 4 个线程各自独立编码全部样本 10 次
- 期望:
  - 每个线程每一次的结果都与 S 逐字节相同;4 个线程读到的 `KINDS.as_ptr()` 相同
- 实现:`crates/hft-event/tests/codec.rs::evt_030_stateless_concurrent_encode`

### EVT-040 行情体与 MarketHdr 的大小、对齐与字段偏移与规格一致

- 规则:[行情(0x01xx)](README.md#行情0x01xx)、[事件体布局](README.md#事件体布局)、[event-model 关键事件体](../../docs/subsystems/event-model.md#关键事件体)
- 级别:table
- 里程碑:M1
- 前置:无参数;`MarketHdr` 56 B 是设计基线 v1 的起点布局
- 实现:`crates/hft-event/tests/layout.rs::evt_040_market_layouts`

| 行 | 结构 | 期望 |
|---|---|---|
| EVT-040.1 | `MarketHdr` | size 56、align 8;`venue`@0、`conn`@1、`_pad`@2、`inst`@4、`ex_ts_ns`@8、`ex_send_ns`@16、`hw_rx_ns`@24、`rx_tsc`@32、`sw_rx_ns`@40、`rx_empty_gap_ns`@48、`_rsv`@52 |
| EVT-040.2 | `Bbo` | size 96、align 8;`h`@0、`bid_px`@56、`bid_qty`@64、`ask_px`@72、`ask_qty`@80、`update_id`@88 |
| EVT-040.3 | `L2Delta` | size 96;`prev_id`@56、`last_id`@64、`px`@72、`qty`@80、`side`@88、`dflags`@89、`idx`@90、`_pad`@92 |
| EVT-040.4 | `Trade` | size 88;`px`@56、`qty`@64、`trade_id`@72、`taker_side`@80、`_pad`@81 |
| EVT-040.5 | `Funding` | size 80;`rate`@56、`next_funding_ns`@64、`interval_s`@72、`_pad`@76 |
| EVT-040.6 | `MarkPrice` | size 64;`px`@56 |
| EVT-040.7 | `IndexPrice` | size 64;`px`@56 |
| EVT-040.8 | `Liquidation` | size 80;`px`@56、`qty`@64、`side`@72、`_pad`@73 |

### EVT-041 MarketHdr 的接收戳字段取边界值时原样往返

- 规则:[行情(0x01xx)](README.md#行情0x01xx)、[event-model 时间戳语义](../../docs/subsystems/event-model.md#时间戳语义)
- 级别:table
- 里程碑:M1
- 前置:一条 `Bbo`,其余字段取非零样本;每行只改 `MarketHdr` 的所列字段;编码后检查体内偏移 40..56 的原始字节并 `read` 回来
- 实现:`crates/hft-event/tests/codec.rs::evt_041_market_hdr_rx_fields`

| 行 | 输入 | 期望 |
|---|---|---|
| EVT-041.1 | `rx_empty_gap_ns = 0`(尚无确认) | 体偏移 48..52 = `00 00 00 00`;读回 0 |
| EVT-041.2 | `rx_empty_gap_ns = 1` | 体偏移 48..52 = `01 00 00 00`;读回 1 |
| EVT-041.3 | `rx_empty_gap_ns = u32::MAX`(饱和值 4 294 967 295) | 体偏移 48..52 = `FF FF FF FF`;读回 `u32::MAX` |
| EVT-041.4 | `sw_rx_ns = 0`(不可用) | 体偏移 40..48 全 0;读回 0 |
| EVT-041.5 | `sw_rx_ns = 1 759 000 000 123 456 789` | 读回同值 |
| EVT-041.6 | 同一组 `MarketHdr` 值分别写入 7 个定长行情体与 `L2SnapHdr`、`L2BatchHdr` | 每个体的前 56 字节逐字节相同;`_rsv`(偏移 52..56)为 0 |

### EVT-050 执行体的大小、对齐与字段偏移与规格一致

- 规则:[执行(0x02xx)](README.md#执行0x02xx)、[事件体布局](README.md#事件体布局)、[event-model 关键事件体](../../docs/subsystems/event-model.md#关键事件体)
- 级别:table
- 里程碑:M1
- 前置:无参数;全部 align 8
- 实现:`crates/hft-event/tests/layout.rs::evt_050_exec_layouts`

| 行 | 结构 | 期望 |
|---|---|---|
| EVT-050.1 | `OrderDecision` | size 88;`trigger_seq`@0、`decided_tsc`@8、`px`@16、`qty`@24、`group`@32、`inst`@36、`slot`@40、`gen`@44、`target_slot`@48、`reject`@52、`strategy`@56、`venue`@57、`account`@58、`action`@59、`side`@60、`tif`@61、`oflags`@62、`reason`@64、`intent_no`@68、`req_px`@72、`link_role`@80、`_pad`@81 |
| EVT-050.2 | `SendReport` | size 48;`decision_seq`@0、`sent_tsc`@8、`slot`@16、`gen`@20、`target_slot`@24、`target_gen`@28、`venue`@32、`account`@33、`conn`@34、`status`@35、`op`@36、`_pad`@37、`os_errno`@40、`_pad2`@44 |
| EVT-050.3 | `OrderEvent` | size 136;`venue`@0、`account`@1、`what`@2、`detail`@3、`slot`@4、`gen`@8、`inst`@12、`px`@16、`qty`@24、`cum_filled`@32、`fee`@40、`fee_asset`@48、`epoch`@50、`reject_code`@52、`exch_order_id`@56、`ex_ts_ns`@64、`hw_rx_ns`@72、`rx_tsc`@80、`avg_px`@88、`cash_qty`@96、`cash_sum`@104、`trade_id`@112、`fill_cash_qty`@120、`fill_cash_amount`@128 |
| EVT-050.4 | `PositionReport` | size 56;`venue`@0、`account`@1、`_pad`@2、`inst`@4、`qty`@8、`entry_px`@16、`upnl`@24、`liq_px`@32、`ex_ts_ns`@40、`rx_tsc`@48 |
| EVT-050.5 | `BalanceReport` | size 40;`venue`@0、`account`@1、`asset`@2、`_pad`@4、`free`@8、`locked`@16、`ex_ts_ns`@24、`rx_tsc`@32 |
| EVT-050.6 | `FundingSettle` | size 56;`venue`@0、`account`@1、`_pad`@2、`inst`@4、`amount`@8、`rate`@16、`pos_qty`@24、`settle_ts_ns`@32、`rx_tsc`@40、`synth`@48、`_pad2`@49 |
| EVT-050.7 | `ExternalFill` | size 96;`venue`@0、`account`@1、`side`@2、`cause`@3、`inst`@4、`qty`@8、`px`@16、`fee`@24、`fee_asset`@32、`_pad`@34、`exch_order_id`@40、`trade_id`@48、`ex_ts_ns`@56、`rx_tsc`@64、`fill_seq`@72、`cash_qty`@80、`cash_amount`@88 |
| EVT-050.8 | `MarginReportHdr` | size 64;`venue`@0、`account`@1、`source`@2、`mflags`@3、`n`@4、`_pad`@6、`margin_balance`@8、`maint_margin`@16、`avail_margin`@24、`margin_ratio`@32、`ex_ts_ns`@40、`rx_tsc`@48、`_rsv`@56 |
| EVT-050.9 | `MarginItem` | size 16;`inst`@0、`adl_quantile`@4、`_pad`@5、`liq_px`@8 |
| EVT-050.10 | `AccountConfigReportHdr` | size 64;`venue`@0、`account`@1、`position_mode`@2、`margin_mode`@3、`multi_asset`@4、`source`@5、`n_lev`@6、`mismatches`@8、`rflags`@12、`uid_keys`@14、`_pad`@15、`venue_uid`@16(24 B)、`ex_ts_ns`@40、`rx_tsc`@48、`_rsv`@56 |
| EVT-050.11 | `LeverageItem` | size 16;`inst`@0、`_pad`@4、`leverage`@8 |
| EVT-050.12 | `ExecDecision` | size 104;`trigger_seq`@0、`set_seq`@8、`group`@16、`tag`@20、`owner`@24、`account`@25、`action`@26、`algo`@27、`origin`@28、`reason`@29、`urgency`@30、`state_from`@31、`state_to`@32、`leg_ix`@33、`first_leg_ix`@34、`side`@35、`eflags`@36、`_pad`@37、`detail`@40、`gate`@44、`exec_order_id`@48、`_pad2`@52、`qty`@56、`px`@64、`target_qty`@72、`pos_qty`@80、`inflight_qty`@88、`depth_qty`@96 |

### EVT-051 OrderFlags 的位值、互斥与线上掩码

- 规则:[OrderFlags](README.md#orderflags)
- 级别:table
- 里程碑:M1
- 前置:无参数
- 实现:`crates/hft-event/tests/flags.rs::evt_051_order_flags`

| 行 | 常量 | 期望 |
|---|---|---|
| EVT-051.1 | `REDUCE_ONLY` | `0x0001` |
| EVT-051.2 | `POST_ONLY` | `0x0002` |
| EVT-051.3 | `LINKED` | `0x0004` |
| EVT-051.4 | `HEDGE_FIX` | `0x0008` |
| EVT-051.5 | `RISK_REDUCING` | `0x0010` |
| EVT-051.6 | `SMOKE` | `0x0020` |
| EVT-051.7 | `WIRE_MASK` | `0x00FF` |
| EVT-051.8 | 六个标志两两按位与;各自与 `!WIRE_MASK` 按位与 | 全部为 0 |

### EVT-052 OrderDecision 的 oflags 含 bit 8..15 时 debug 构建断言失败

- 规则:[OrderFlags](README.md#orderflags)
- 级别:table
- 里程碑:M1
- 前置:`debug_assertions` 开启;一条合法 `OrderDecision`,每行只改 `oflags`;输出缓冲 112 B
- 实现:`crates/hft-event/tests/flags.rs::evt_052_oflags_reserved_bits_assert`

| 行 | 输入 `oflags` | 期望 |
|---|---|---|
| EVT-052.1 | `0x0100` | `encode_fixed` debug_assert 失败(panic) |
| EVT-052.2 | `0x8000` | debug_assert 失败 |
| EVT-052.3 | `0x00FF`(含保留的 bit 6、7) | 不 panic,返回 112,体偏移 62 = `FF 00` |
| EVT-052.4 | `0x0030`(`RISK_REDUCING \| SMOKE`) | 不 panic;读回 `0x0030` |

### EVT-053 release 构建不检查 oflags 保留位且字节原样写出

- 规则:[OrderFlags](README.md#orderflags)
- 级别:unit
- 里程碑:M1
- 前置:`debug_assertions` 关闭(`cargo test --release`;`#[cfg(not(debug_assertions))]`);`OrderDecision.oflags = 0x0100`
- 步骤:
  1. `encode_fixed` 后 `decode_record`、`read::<OrderDecision>()`
- 期望:
  - 不 panic;读回 `oflags == 0x0100`(保留位不上线的保证来自 debug 构建的测试与 DST,见 EVT-052)
- 实现:`crates/hft-event/tests/flags.rs::evt_053_oflags_release_no_check`

### EVT-054 AccountConfigReport 的模式码与 hft-refdata 枚举逐值相同

- 规则:[执行(0x02xx)](README.md#执行0x02xx)
- 级别:table
- 里程碑:M1
- 前置:hft-event 的 `EnumDesc`(`AccountConfigReportHdr.position_mode`/`margin_mode`/`multi_asset`、`CmdPreview.target_kind`)与 `hft_refdata` 的同名枚举
- 实现:`crates/hft-event/tests/enums.rs::evt_054_refdata_mode_codes`

| 行 | 输入 | 期望 |
|---|---|---|
| EVT-054.1 | `PositionMode` | `NotApplicable = 1`、`OneWay = 2`、`Hedge = 3`,与 `position_mode` 码表逐项同名同值,码表无其他值 |
| EVT-054.2 | `MarginMode` | `NotApplicable = 1`、`Cross = 2`、`Isolated = 3`、`Mixed = 4`,与 `margin_mode` 码表逐项相同 |
| EVT-054.3 | `MultiAsset` | `NotApplicable = 1`、`Off = 2`、`On = 3`,与 `multi_asset` 码表逐项相同 |
| EVT-054.4 | `AccountKind` 行内编码 | `Live = 1`、`Sim = 2`,与 `CmdPreview.target_kind` 码表逐项相同 |

### EVT-055 TargetOrigin 线上码只接受 1..=6

- 规则:[执行(0x02xx)](README.md#执行0x02xx)
- 级别:table
- 里程碑:M1
- 前置:`TargetOriginWire::try_from(u8)`
- 实现:`crates/hft-event/tests/enums.rs::evt_055_target_origin_wire`

| 行 | 输入 | 期望 |
|---|---|---|
| EVT-055.1 | 1 | `Ok(Strategy)` |
| EVT-055.2 | 2 | `Ok(EngineMarginStress)` |
| EVT-055.3 | 3 | `Ok(EngineExternal)` |
| EVT-055.4 | 4 | `Ok(EngineCommand)` |
| EVT-055.5 | 5 | `Ok(EngineKillFlatten)` |
| EVT-055.6 | 6 | `Ok(EngineAutoFix)` |
| EVT-055.7 | 0 | `Err(InvalidEnum)` |
| EVT-055.8 | 7 | `Err(InvalidEnum)` |
| EVT-055.9 | 255 | `Err(InvalidEnum)` |

### EVT-060 链上体的大小、对齐与字段偏移与规格一致

- 规则:[链上(0x03xx)](README.md#链上0x03xx)、[event-model 关键事件体](../../docs/subsystems/event-model.md#关键事件体)
- 级别:table
- 里程碑:M1
- 前置:无参数;四个 kind 在 `KINDS` 中 `optional = true`(EVT-004.20–EVT-004.23),布局照常断言
- 实现:`crates/hft-event/tests/layout.rs::evt_060_chain_layouts`

| 行 | 结构 | 期望 |
|---|---|---|
| EVT-060.1 | `ChainHead` | size 80;`chain`@0、`source`@1、`hflags`@2、`_pad`@3、`block`@8、`block_ts_ns`@16、`observed_ts_ns`@24、`rx_tsc`@32、`hash`@40(32 B)、`finalized`@72 |
| EVT-060.2 | `PoolQuote` | size 96;`quote_id`@0、`block`@8、`group`@16、`route`@20、`chain`@24、`level`@25、`qflags`@26、`_pad`@27、`size`@32、`buy_px`@40、`sell_px`@48、`depth`@56、`gas_cost`@64、`fee`@72、`computed_ts_ns`@80、`rx_tsc`@88 |
| EVT-060.3 | `SwapDecision` | size 128;`intent_id`@0、`quote_id`@8、`trigger_seq`@16、`decided_tsc`@24、`amount_in`@32、`min_out`@40、`amount_out`@48、`max_in`@56、`limit_px`@64、`edge`@72、`max_bribe`@80、`group`@88、`route`@92、`reject`@96、`hedge_inst`@100、`strategy`@104、`account`@105、`chain`@106、`side`@107、`valid_blocks`@108、`exact`@109、`hedge_side`@110、`hedge_px_mode`@111、`hedge_limit`@112、`sflags`@120 |
| EVT-060.4 | `TxLifecycle` | size 152;`intent_id`@0、`state`@8、`builder`@9、`account`@10、`_pad`@11、`block`@16、`amount_in`@24、`amount_out`@32、`gas_used`@40、`gas_cost`@48、`tx_hash`@56、`observed_ts_ns`@88、`reason`@96、`tflags`@98、`src_mask`@100、`wallet`@101、`_pad2`@102、`bribe`@104、`rx_tsc`@112、`sig_hi`@120 |

### EVT-061 TxState 码值与 SwapDecision 无对冲腿时的 hedge_side

- 规则:[链上(0x03xx)](README.md#链上0x03xx)
- 级别:table
- 里程碑:M1
- 前置:合法的 `TxLifecycle` 与 `SwapDecision` 记录,每行只改所列字段
- 实现:`crates/hft-event/tests/enums.rs::evt_061_chain_codes`

| 行 | 输入 | 期望 |
|---|---|---|
| EVT-061.1 | `TxLifecycle.state` = 1..=6 | 依次读出 `Sent`、`IncludedOk`、`IncludedReverted`、`Dropped`、`Unknown`、`Finalized` |
| EVT-061.2 | `TxLifecycle.state` = 0 | `Err(InvalidEnum)` |
| EVT-061.3 | `TxLifecycle.state` = 7 | `Err(InvalidEnum)` |
| EVT-061.4 | `SwapDecision.hedge_inst = 0`、`hedge_side = Buy` | `Ok` |
| EVT-061.5 | `SwapDecision.hedge_inst = 0`、`hedge_side` 字节 = 0 | `Err(InvalidEnum)`(写者必须写 Buy) |

### EVT-070 控制与系统体的大小、对齐与字段偏移与规格一致

- 规则:[控制与系统](README.md#控制与系统0x04xx0x05xx)、[EngineClock](README.md#engineclock0x050ainput)、[事件体布局](README.md#事件体布局)、[event-model 关键事件体](../../docs/subsystems/event-model.md#关键事件体)
- 级别:table
- 里程碑:M1
- 前置:无参数
- 实现:`crates/hft-event/tests/layout.rs::evt_070_system_layouts`

| 行 | 结构 | 期望 |
|---|---|---|
| EVT-070.1 | `CmdResult` | size 144、align 8;`cmd_id`@0、`cmd_seq`@16、`cmd`@24、`status`@26、`_pad`@27、`detail`@28、`engine_epoch`@32、`effect_count`@36、`refdata_version`@40、`risk_version`@48、`params_version`@56、`ctl_state_version`@64、`rflags`@72、`_pad2`@76、`preview`@80 |
| EVT-070.2 | `CmdPreview` | size 64;`target_account`@0、`other_account`@1、`target_mode`@2、`other_mode`@3、`target_kind`@4、`_pad`@5、`affected_cells`@8、`open_orders`@12、`active_caps`@16、`cap_sources`@20、`reset_groups`@24、`no_open_ms`@28、`wo_venue`@32、`wo_leg_ix`@33、`_pad2`@34、`wo_inst`@36、`wo_qty`@40、`_rsv`@48 |
| EVT-070.3 | `ReconcileReport` | size 104;`report_id`@0、`venue`@8、`account`@9、`item`@10、`severity`@11、`inst`@12、`asset`@16、`idx`@18、`count`@20、`_pad`@22、`engine_qty`@24、`exch_qty`@32、`diff`@40、`exch_ts_ns`@48、`slot`@56、`order_status`@60、`history_checked`@61、`_pad2`@62、`view_seq`@64、`gen`@72、`epoch`@76、`_pad3`@78、`exch_amount`@80、`engine_amount`@88、`sflags`@96、`skipped_inflight`@98、`_pad4`@100 |
| EVT-070.4 | `LatencySample` | size 136;`trigger_seq`@0、`ack_seq`@8、`slot`@16、`gen`@20、`venue`@24、`account`@25、`strategy`@26、`_pad`@27、`src`@28、`ex_ts_ns`@32、`ex_send_ns`@40、`hw_rx_ns`@48、`rx_tsc`@56、`proc_tsc`@64、`decided_tsc`@72、`sent_tsc`@80、`ack_hw_rx_ns`@88、`ack_rx_tsc`@96、`ack_ex_ts_ns`@104、`sw_rx_ns`@112、`link_role`@120、`action`@121、`_pad2`@122、`trigger_kind`@128、`_pad3`@130 |
| EVT-070.5 | `Note` | size 80;`code`@0、`level`@4、`source`@5、`strategy`@6、`_pad`@7、`group`@8、`inst`@12、`v`@16(64 B) |
| EVT-070.6 | `HostProfile` | size 176;`boot_id`@0、`build_git`@16、`_pad`@36、`profile_sha256`@40、`release`@72、`kernel`@88、`tsc_hz_milli`@120、`host_checks`@128、`hugepages_1g_total`@136、`hugepages_1g_free`@140、`isolated_cpus`@144 |
| EVT-070.7 | `ClockStatus` | size 56;`tsc`@0、`realtime_ns`@8、`phc_ns`@16、`tsc_hz_milli`@24、`chrony_offset_ns`@32、`max_error_ns`@40、`rms_ns`@44、`calib_err_ns`@48、`source`@52、`state`@53、`_pad`@54 |
| EVT-070.8 | `SegmentRoll` | size 48;`segment_no`@0、`other_segment_no`@8、`seq_mark`@16、`utc_ns`@24、`mark`@32、`reason`@33、`journal_kind`@34、`_pad`@35、`boot_records`@36、`_rsv`@40 |
| EVT-070.9 | `EngineClock` | size 32;`base_engine_ns`@0、`tsc_hz_milli`@8、`prev_last_seq`@16、`source`@24、`_pad`@25 |
| EVT-070.10 | `Timer` | size 24;`timer_id`@0、`owner`@4、`_pad`@6、`group`@8、`_pad2`@12、`deadline_tsc`@16 |
| EVT-070.11 | `ConnState` | size 24;`venue`@0、`conn`@1、`state`@2、`role`@3、`inst`@4、`reason`@8、`account`@12、`_pad2`@13、`rx_tsc`@16 |
| EVT-070.12 | `UtcAnchor` | size 32;`tsc`@0、`realtime_ns`@8、`hz_milli`@16、`err_ns`@24、`_pad`@28 |

### EVT-071 旧版本写出的记录读出时新增字段为 0

- 规则:[控制与系统](README.md#控制与系统0x04xx0x05xx)、[执行(0x02xx)](README.md#执行0x02xx)、[实现说明](README.md#实现说明)、[event-model 演进规则](../../docs/subsystems/event-model.md#演进规则)
- 级别:table
- 里程碑:M1
- 前置:fixture `fixtures/hft-event/EVT-071/` 中每行一条按旧版本长度写出的记录(旧字段取非零样本,记录头 `len = 24 + 旧体长`),段头 schema 表登记该旧版本;读者为当前版本
- 实现:`crates/hft-event/tests/version.rs::evt_071_old_versions_zero_fill`

| 行 | 输入 | 期望 |
|---|---|---|
| EVT-071.1 | `ExternalFill` v1(72 B) | `read` 为 `Ok`;前 72 B 字段同样本;`fill_seq == 0` |
| EVT-071.2 | `ReconcileReport` v1(80 B) | `exch_amount == 0`、`engine_amount == 0`、`sflags == 0`、`skipped_inflight == 0` |
| EVT-071.3 | `ReconcileReport` v2(96 B) | `exch_amount`/`engine_amount` 同样本;`sflags == 0`、`skipped_inflight == 0` |
| EVT-071.4 | `LatencySample` v1(112 B) | `sw_rx_ns == 0`、`link_role == 0`、`action == 0`、`trigger_kind == 0` |
| EVT-071.5 | `LatencySample` v2(128 B,偏移 121 为 0) | `trigger_kind == 0`;`action == 0`(New);`sw_rx_ns`、`link_role` 同样本 |
| EVT-071.6 | `LatencySample` v3(136 B,偏移 121 = 2) | `action == 2`(Amend) |
| EVT-071.7 | `CmdResult` v2(144 B,体偏移 104..112 为 0) | `preview.reset_groups == 0`、`preview.no_open_ms == 0`,其余字段同样本 |
| EVT-071.8 | `ConnState` v1(16 B) | `rx_tsc == 0`(未提供) |
| EVT-071.9 | `FundingSettle` v1(48 B) | `synth == 0`;`classify` 为 `Input` |
| EVT-071.10 | `OrderEvent` v1(96 B) | 原字段逐字节保留;`cash_qty == cash_sum == trade_id == 0`,表示缺少结算与正成交 ID 事实 |
| EVT-071.11 | `ExternalFill` v2(80 B) | 原字段逐字节保留;`cash_qty == cash_amount == 0`,表示缺少结算事实 |
| EVT-071.12 | `OrderEvent` v2(112 B) | 原字段含非零结算事实逐字节保留;`trade_id == 0`,表示没有正成交 ID 事实 |

### EVT-072 LatencySample.action 只允许 0..=2

- 规则:[控制与系统](README.md#控制与系统0x04xx0x05xx)
- 级别:table
- 里程碑:M1
- 前置:`debug_assertions` 开启;一条合法 `LatencySample`,每行只改 `action`
- 实现:`crates/hft-event/tests/codec.rs::evt_072_latency_action_range`

| 行 | 输入 `action` | 期望 |
|---|---|---|
| EVT-072.1 | 0 | 编码成功,读回 0 |
| EVT-072.2 | 1 | 编码成功,读回 1 |
| EVT-072.3 | 2 | 编码成功,读回 2 |
| EVT-072.4 | 3 | `encode_fixed` debug_assert 失败 |
| EVT-072.5 | 255 | debug_assert 失败 |

### EVT-073 各位集合字段的位两两不重叠

- 规则:[控制与系统](README.md#控制与系统0x04xx0x05xx)、[执行(0x02xx)](README.md#执行0x02xx)、[cap 记录](README.md#cap-记录)
- 级别:table
- 里程碑:M1
- 前置:各字段的位常量(`EnumDesc` 的 `Bits` 形式)
- 实现:`crates/hft-event/tests/enums.rs::evt_073_bitsets_disjoint`

| 行 | 输入 | 期望 |
|---|---|---|
| EVT-073.1 | `ReconcileReport.sflags`:`USERSTREAM_DOWN`、`TRUNCATED`、`HISTORY_UNCHECKED`、`AUTO_EXTERNAL` | 值 1、2、4、8;两两按位与为 0 |
| EVT-073.2 | `CmdResult.rflags`:`PREVIEW`、`NEEDS_CONFIRM`、`PREVIEW_TTL` | 1、2、4;两两不重叠 |
| EVT-073.3 | `AccountConfigReportHdr.mismatches` 九位 | 1、2、4、8、16、32、64、128、256;两两不重叠 |
| EVT-073.4 | `AccountConfigReportHdr.rflags`:`PARTIAL`、`LEV_TRUNCATED`;`uid_keys`:`SESSION`、`REST` | 1、2;1、2 |
| EVT-073.5 | `MarginReportHdr.mflags`:`MARGIN_CALL`、`ITEMS_TRUNCATED` | 1、2 |
| EVT-073.6 | `ExecDecision.eflags`:`SHADOW`、`CLEARED_BY_ENGINE` | 1、2 |
| EVT-073.7 | `CapRecord.cflags`:`HARD`、`CONDITIONAL`、`NON_CLEARABLE` | 1、2、4 |
| EVT-073.8 | `RawHdr.rflags`:`REDACTED`、`SAMPLED`;`L2BatchHdr.bflags`:`SNAPSHOT_BRIDGE`;`L2Delta.dflags`:`LAST` | 1、2;1;1 |

### EVT-074 每张线上码表的代码形式与规格逐值相同

- 规则:[事件体布局](README.md#事件体布局)、[执行(0x02xx)](README.md#执行0x02xx)、[链上(0x03xx)](README.md#链上0x03xx)、[控制与系统](README.md#控制与系统0x04xx0x05xx)、[变长记录前缀](README.md#变长记录前缀)、[控制命令编码](README.md#控制命令编码)、[event-model 关键事件体](../../docs/subsystems/event-model.md#关键事件体)
- 级别:table
- 里程碑:M1
- 前置:读取每个字段 `FieldDesc.ty` 的 `EnumDesc`(或带 `#[repr]` 的 Rust 枚举的判别值);每行断言码表恰为所列项(无多余项),列出的"退役"值不在码表中
- 实现:`crates/hft-event/tests/enums.rs::evt_074_code_tables`

| 行 | 字段 | 期望码表 |
|---|---|---|
| EVT-074.1 | `OrderDecision.action`(`OrderAction`) | `New=1 Cancel=2 Amend=3 CancelAll=4 Query=5` |
| EVT-074.2 | `OrderDecision.link_role`、`LatencySample.link_role` | `0` 无、`First=1 Second=2 SecondRetry=3 HedgeFix=4 Reverse=5` |
| EVT-074.3 | `SendReport.status`(`SendStatus`) | `Written=1 KillBlocked=2 RateLimited=3 WriteError=4 NoConn=5 EncodeError=6 RingFull=7 QueryUnsupported=8` |
| EVT-074.4 | `SendReport.op`、`GwCmd.op`(`GwOp`) | `Place=1 Cancel=2 Amend=3 CancelAll=4 Query=5 DeadManRefresh=6 Rotate=7 ResetRate=8 Resubscribe=9 SetEpoch=10 SessionKeepalive=11 MirrorRestore=12 SimReset=13 SimTransfer=14 SimFault=15 SimSettle=16 BindWire=17 Deliver=18`;1..=5 与 `OrderAction` 同值 |
| EVT-074.5 | `OrderEvent.what`(`OrderEventKind`) | `Ack=1 Reject=2 PartialFill=3 Fill=4 CancelAck=5 CancelReject=6 AmendAck=7 AmendReject=8 Expired=9 QueryResult=10 OutcomeUnknown=11 FeeOnly=12` |
| EVT-074.6 | `ExternalFill.cause`(`ExternalCause`) | `Liquidation=1 Adl=2 Manual=3 Delivery=4 Unknown=5` |
| EVT-074.7 | `MarginReportHdr.source` | `Push=1 Poll=2` |
| EVT-074.8 | `AccountConfigReportHdr.source` | `Startup=1 Push=2 Poll=3` |
| EVT-074.9 | `ExecDecision.action` | `TargetSet=1 TargetReplaced=2 TargetCleared=3 TargetRejected=4 Slice=5 State=6 HedgeFix=7 ResidualTrim=8 ExternalSeen=9 ExternalDefault=10 ForceStep=11 CmdHedgeFix=12 ChainLegExcess=13 AutoFix=14` |
| EVT-074.10 | `ExecDecision.algo` | `0` 无、`TakerSlice=1 PairOnFill=2 Simultaneous=3 MakerFirst=4 Engine=5` |
| EVT-074.11 | `ExecDecision.reason` | `Rebalance=1 Exit=2 MarginStress=3 Hard=4 Manual=5` |
| EVT-074.12 | `ExecDecision.urgency` | `Passive=1 Normal=2 Urgent=3` |
| EVT-074.13 | `ExecDecision.state_from`、`state_to`(`TargetState`) | `Idle=1 Working=2 Stuck=3 Broken=4` |
| EVT-074.14 | `ExecDecision.origin` | 与 `TargetOriginWire` 相同(EVT-055) |
| EVT-074.15 | `ReconcileReport.item` | `Position=1 Balance=2 OpenOrder=3 UnknownResolved=4 Summary=5 Funding=6` |
| EVT-074.16 | `ReconcileReport.severity` | `Ignore=1 Warn=2`;3 退役(不在表中) |
| EVT-074.17 | `ReconcileReport.order_status` | `0` 其他项、`New=1 PartiallyFilled=2 Filled=3 Cancelled=4 Rejected=5 Expired=6 NotFound=7` |
| EVT-074.18 | `LatencySample.src` 每点 2 位 | `0` 缺失、`HW=1 SW=2 TSC=3` |
| EVT-074.19 | `LatencySample.action` | `New=0 Cancel=1 Amend=2` |
| EVT-074.20 | `Note.level`(`NoteLevel`) | `P0=1 P1=2 P2=3 Info=4` |
| EVT-074.21 | `Note.source`(`NoteSource`) | `Sequencer=1 Strategy=2 Risk=3 Oms=4 Venue=5 CtlAudit=6 Measure=7 Housekeeping=8 Ledger=9 Exec=10 Boot=11 Advisor=12` |
| EVT-074.22 | `ConnState.state`(`ConnStateKind`) | `Up=1 Down=2 Resync=3 Rotated=4 Degraded=5 Alive=6` |
| EVT-074.23 | `ConnState.role`(`ConnRole`) | `Md=1 Order=2 UserStream=3 Rest=4` |
| EVT-074.24 | `ClockStatus.source`;`ClockStatus.state` | `Phc=1 Ntp=2 None=3`;`Synced=1 Unsynced=2 Stepped=3` |
| EVT-074.25 | `SegmentRoll.mark`;`reason`;`journal_kind` | `End=1 Begin=2 BootDone=3`;`Hourly=1 Size=2 Startup=3 Recovery=4 Manual=5 RecoverFromSnapshot=6`;`Raw=1 Seq=2 Chain=3` |
| EVT-074.26 | `EngineClock.source` | `Fresh=1 Tail=2 Snapshot=3` |
| EVT-074.27 | `L2SnapHdr.origin` | `Exchange=1 Keyframe=2 TopN=3` |
| EVT-074.28 | `BlobHdr.blob.kind`(`BlobKind`) | `Refdata=1 StrategyParams=2 RiskParams=3 HostManifest=4 Engine=5 SimConfig=6` |
| EVT-074.29 | `RawHdr.what` | `WsText=1 WsBinary=2 RestResponse=3 Ping=4 Pong=5 Connect=6 Disconnect=7 SubAck=8 WsClose=9` |
| EVT-074.30 | `TxLifecycle.state`(`TxState`) | `Sent=1 IncludedOk=2 IncludedReverted=3 Dropped=4 Unknown=5 Finalized=6` |
| EVT-074.31 | `ChainHead.source`;`PoolQuote.level`;`SwapDecision.exact`;`SwapDecision.hedge_px_mode` | `Node=1 BlockStream=2 Rpc=3`;`L0=1 L1=2`;`ExactIn=0 ExactOut=1`;`Cross=1 Mid=2 LimitBp=3` |
| EVT-074.32 | `StrategyParamsHdr.source`(`ParamSource`);`LegDef.kind` | `Pipeline=1 Manual=2`;`Cex=1 Chain=2` |
| EVT-074.33 | `CmdHdr.role`;`CmdHdr.hflags` | `0` 进程主体、`Viewer=1 Trader=2 Admin=4`(3 退役);`PREVIEW=1` |
| EVT-074.34 | `Annotate.kind`;`VenueNotice.kind`;`AdoptExchange.item` | `OpsNote=1 LocalWrite=2 Incident=3`;`Delisting=1 FundingInterval=2 Maintenance=3`;`Position=0 Balance=1` |
| EVT-074.35 | `OrderEvent.detail`(按 `what`) | `QueryResult`:`New=1 PartiallyFilled=2 Filled=3 Cancelled=4 Rejected=5 Expired=6 NotFound=7`(与 EVT-074.17 逐值相同,0 不在表中);`CancelReject`/`AmendReject`:`OTHER=0 GONE=1 EARLY=2`;其余 `what` 只有 0 |

### EVT-075 Note 码名登记表的代码形式逐行与 README 一致

- 规则:[Note 来源与码登记表](README.md#note-来源与码登记表)
- 级别:table
- 里程碑:M1
- 前置:`NOTE_NAMES` 与 `note_name`;"期望"写 `source / level / v`,`level` 为 `None` 表示"按条件";`v` 写成 `(bit, bits)` 序列,只比较位置与宽度,不比较 `VField.name`;`[]` 表示本表不登记 `v[]` 字段;另断言 `NOTE_NAMES.len() == 95` 且顺序与 README 登记表行序一致(EVT-075.85–EVT-075.96 位于各自 source 组末尾)
- 实现:`crates/hft-event/tests/notes.rs::evt_075_note_registry_rows`

| 行 | 码名 | 期望 |
|---|---|---|
| EVT-075.1 | `EpochStart` | Sequencer / Info / `(0,64) (64,64) (128,64)` |
| EVT-075.2 | `Conflated` | Sequencer / Info / `[]` |
| EVT-075.3 | `DrainTimeout` | Sequencer / None / `[]` |
| EVT-075.4 | `note_name("ConfigDiffers")` | `None`(已从登记表删除:无产生规则) |
| EVT-075.5 | `RecoverMismatch` | Sequencer / P0 / `(0,8) (8,8) (64,64)` |
| EVT-075.6 | `AdviceMisrouted` | Sequencer / P2 / `[]` |
| EVT-075.7 | `AdviceStale` | Sequencer / P2 / `[]` |
| EVT-075.8 | `StartupGate` | Sequencer / Info / `(0,64) (64,64) (128,64) (192,64)` |
| EVT-075.9 | `ShadowDeferred` | Sequencer / Info / `(0,8) (8,8) (64,64) (128,64)` |
| EVT-075.10 | `IdentityMismatch` | Sequencer / P0 / `(0,8) (8,8)` |
| EVT-075.11 | `RefdataRecheckFailed` | Sequencer / P1 / `[]` |
| EVT-075.89 | `TimersCancelled` | Sequencer / Info / `[]` |
| EVT-075.90 | `TargetCleared` | Sequencer / Info / `[]` |
| EVT-075.91 | `PositionDiff` | Sequencer / P1 / `(0,8) (8,8) (64,64) (128,64)` |
| EVT-075.12 | `RateLimitHit` | Risk / P2 / `[]` |
| EVT-075.13 | `CapSet` | Risk / Info / `(0,384)` |
| EVT-075.14 | `CapCleared` | Risk / Info / `(0,384)` |
| EVT-075.15 | `MarginLevel` | Risk / None / `(0,8) (8,8) (64,64) (128,64) (192,64) (256,64)` |
| EVT-075.16 | `RecoveryHeldSim` | Risk / P1 / `[]` |
| EVT-075.17 | `IdentityUnverified` | Risk / P1 / `(0,8) (8,8)` |
| EVT-075.18 | `OmsTransition` | Oms / Info / `[]` |
| EVT-075.19 | `OmsUnknown` | Oms / None / `[]` |
| EVT-075.20 | `OmsUnknownStuck` | Oms / P0 / `[]` |
| EVT-075.21 | `OmsVenueReady` | Oms / Info / `[]` |
| EVT-075.22 | `OmsStaleGenFill` | Oms / P0 / `[]` |
| EVT-075.23 | `OmsLateFill` | Oms / P0 / `[]` |
| EVT-075.24 | `OmsOverfill` | Oms / P0 / `[]` |
| EVT-075.25 | `OmsForeignOrder` | Oms / P1 / `[]` |
| EVT-075.26 | `OmsForeignFillViaOrderEvent` | Oms / P1 / `[]` |
| EVT-075.27 | `OmsExtDedup` | Oms / P1 / `[]` |
| EVT-075.28 | `OmsForeignFill` | Oms / Info / `[]` |
| EVT-075.29 | `OmsHedgeBroken` | Oms / P0 / `[]` |
| EVT-075.30 | `OmsHedgePxStale` | Oms / P1 / `[]` |
| EVT-075.31 | `OmsHedgePxGated` | Oms / P1 / `[]` |
| EVT-075.32 | `OmsHedgeSlipCapped` | Oms / P1 / `[]` |
| EVT-075.33 | `OmsHedgeNakedCapped` | Oms / P1 / `[]` |
| EVT-075.34 | `OmsResumeRetry` | Oms / P1 / `[]` |
| EVT-075.35 | `OmsResumeTimeout` | Oms / None / `[]` |
| EVT-075.36 | `OmsFillPxEstimated` | Oms / P1 / `[]` |
| EVT-075.37 | `OmsFeeUnattributed` | Oms / P1 / `[]` |
| EVT-075.38 | `OmsCancelNotSent` | Oms / P2 / `[]` |
| EVT-075.39 | `OmsNeedsRestQuery` | Oms / Info / `[]` |
| EVT-075.40 | `OmsAnomaly` | Oms / None / `[]` |
| EVT-075.96 | `OmsHedgeResidual` | Oms / P1 / `[]` |
| EVT-075.97 | `OmsLinkCapacity` | Oms / P1 / `[]` |
| EVT-075.41 | `RoundClosed` | Ledger / Info / `[]` |
| EVT-075.42 | `Adjusted` | Ledger / Info / `(0,8) (8,8) (64,64) (128,64) (192,64) (256,64) (320,64)` |
| EVT-075.43 | `BaselineSet` | Ledger / Info / `[]` |
| EVT-075.44 | `BalanceInit` | Ledger / Info / `[]` |
| EVT-075.45 | `SimReset` | Ledger / Info / `[]` |
| EVT-075.46 | `ResidualWrittenOff` | Ledger / P1 / `[]` |
| EVT-075.47 | `ExternalFill` | Ledger / None / `[]` |
| EVT-075.48 | `LedgerAutoHeldOffset` | Ledger / P0 / `[]` |
| EVT-075.49 | `BalanceDrift` | Ledger / P1 / `[]` |
| EVT-075.50 | `InvShortfall` | Ledger / P1 / `[]` |
| EVT-075.51 | `NetQtyMismatch` | Ledger / P1 / `[]` |
| EVT-075.52 | `FundingMismatch` | Ledger / P1 / `[]` |
| EVT-075.53 | `FundingCorrected` | Ledger / P1 / `[]` |
| EVT-075.54 | `ChainReorg` | Ledger / P1 / `[]` |
| EVT-075.55 | `ChainReorgNaked` | Ledger / P0 / `[]` |
| EVT-075.92 | `BalanceCheck` | Ledger / Info / `(0,8) (8,8) (64,64) (128,64) (192,64) (256,64)` |
| EVT-075.56 | `ExecBroken` | Exec / P0 / `[]` |
| EVT-075.57 | `ExecExternalDefault` | Exec / None / `[]` |
| EVT-075.58 | `ExecAutoFixFailed` | Exec / P0 / `[]` |
| EVT-075.59 | `ExecStuck` | Exec / P1 / `[]` |
| EVT-075.60 | `ExecForceStep` | Exec / P1 / `[]` |
| EVT-075.61 | `ExecAutoFixDone` | Exec / P1 / `[]` |
| EVT-075.62 | `ExecSerialFallback` | Exec / P0 / `[]` |
| EVT-075.63 | `ExecEngineOrderRejected` | Exec / P0 / `[]` |
| EVT-075.64 | `ExecManualExternal` | Exec / None / `[]` |
| EVT-075.65 | `ExecManualHold` | Exec / None / `[]` |
| EVT-075.66 | `ExecUngroupedReduce` | Exec / P0 / `[]` |
| EVT-075.67 | `ExecGateHeldNaked` | Exec / P0 / `[]` |
| EVT-075.68 | `ExecChainLegExcess` | Exec / P0 / `[]` |
| EVT-075.69 | `CtlAudit` | CtlAudit / Info / `(0,128) (128,16) (144,8) (152,8) (160,8) (168,8) (176,8) (192,64) (256,192) (448,32)` |
| EVT-075.70 | `PartialEventDropped` | Boot / P1 / `[]` |
| EVT-075.71 | `TailSkipped` | Boot / P0 / `(0,64) (64,64) (128,64)` |
| EVT-075.72 | `RecoverFromSnapshot` | Boot / P1 / `(0,64) (64,64) (128,64) (192,64) (256,64)` |
| EVT-075.73 | `KillSelfTest` | Boot / Info / `(0,64) (64,64)` |
| EVT-075.74 | `InputQuarantined` | Boot / P0 / `(0,64) (64,64)` |
| EVT-075.75 | `StartupStep` | Boot / Info / `[]` |
| EVT-075.76 | `HostDegraded` | Boot / P1 / `[]` |
| EVT-075.77 | `JournalTailLost` | Boot / P1 / `[]` |
| EVT-075.78 | `ModelCheckpointDiscarded` | Advisor / P1 / `(0,64) (64,64)` |
| EVT-075.79 | `ModelPathReset` | Advisor / P2 / `(0,64) (64,64) (128,64) (192,64) (256,64)` |
| EVT-075.80 | `AdvisorLag` | Advisor / P1 / `(0,64) (64,64) (128,64) (192,64)` |
| EVT-075.81 | `ModelGroupReset` | Advisor / P2 / `(0,64) (64,64)` |
| EVT-075.82 | `AdvisorStats` | Advisor / Info / `[]` |
| EVT-075.83 | `note_name("NoSuchNote")` | `None` |
| EVT-075.84 | `note_name("VenueReconnects")`(假想的 `Venue` 源计数码) | `None`(该类码只在 source crate 的 const 表) |
| EVT-075.85 | `RefdataWarning` | Boot / P1 / `(0,64) (64,64) (128,64)` |
| EVT-075.86 | `StopMarkerCorrupt` | Boot / P0 / `[]` |
| EVT-075.87 | `EndpointOverride` | Boot / P1 / `(0,8)` |
| EVT-075.88 | `ModelCkptSkipped` | Advisor / P1 / `[]` |
| EVT-075.93 | `ReconcileCash` | Sequencer / Info / `(0,64) (64,8) (72,8) (128,64) (192,64) (256,64)` |
| EVT-075.94 | `ReconcileRejected` | Sequencer / P0 / `(0,64) (64,8) (72,8) (128,64) (192,64)` |
| EVT-075.95 | `ReceiptMismatch` | Ledger / P0 / `(0,8) (8,8) (64,64) (128,64) (192,64) (256,64) (320,64)` |
| EVT-075.96 | `DayPnlCarried` | Ledger / Info / `(0,8) (64,64)` |

### EVT-076 Note 码名登记表满足命名与字段不变量

- 规则:[Note 来源与码登记表](README.md#note-来源与码登记表)
- 级别:unit
- 里程碑:M1
- 前置:`NOTE_NAMES`
- 步骤:
  1. 遍历全部项
- 期望:
  - 名字两两不同;`source = Oms` 的名字都以 `Oms` 开头,`source = Exec` 的都以 `Exec` 开头;没有 `source ∈ {Strategy, Venue, Measure, Housekeeping}` 的项
  - 每项的 `v` 字段两两不重叠;每个字段 `bit + bits ≤ 512`;`bits ≤ 64` 的字段满足 `bit / 64 == (bit + bits − 1) / 64`(不跨 `v[]` 元素);`bits > 64` 的字段 `bit % 64 == 0` 且 `bits % 64 == 0`
- 实现:`crates/hft-event/tests/notes.rs::evt_076_note_registry_invariants`

### EVT-077 note_code 把 source 放在高 8 位、n 放在低 24 位

- 规则:[控制与系统](README.md#控制与系统0x04xx0x05xx)、[Note 来源与码登记表](README.md#note-来源与码登记表)
- 级别:table
- 里程碑:M1
- 前置:`debug_assertions` 开启
- 实现:`crates/hft-event/tests/notes.rs::evt_077_note_code_packing`

| 行 | 输入 `(source, n)` | 期望 |
|---|---|---|
| EVT-077.1 | `(Sequencer, 1)` | `0x0100_0001` |
| EVT-077.2 | `(Oms, 5)` | `0x0400_0005` |
| EVT-077.3 | `(Advisor, 0xFF_FFFF)` | `0x0CFF_FFFF` |
| EVT-077.4 | `(Exec, 0)` | debug_assert 失败(n = 0 保留) |
| EVT-077.5 | `(Risk, 0x100_0000)` | debug_assert 失败 |
| EVT-077.6 | `(Boot, 1)` | `0x0B00_0001` |

### EVT-078 VField 的 get 与 put 只读写本字段的位

- 规则:[Note 来源与码登记表](README.md#note-来源与码登记表)
- 级别:table
- 里程碑:M1
- 前置:`v = [0x1111_1111_1111_1111; 8]` 作为每行的初值
- 实现:`crates/hft-event/tests/notes.rs::evt_078_vfield_get_put`

| 行 | 输入 | 期望 |
|---|---|---|
| EVT-078.1 | `VField{bit: 8, bits: 8}.put(v, 0xAB)` | `Ok`;`v[0] == 0x1111_1111_1111_AB11`;`v[1..8]` 不变;`get == 0xAB` |
| EVT-078.2 | `VField{bit: 8, bits: 8}.put(v, 0xFF)` | `Ok`;`get == 0xFF` |
| EVT-078.3 | `VField{bit: 8, bits: 8}.put(v, 0x100)` | `Err(TooLong)`;`v` 全部不变 |
| EVT-078.4 | `VField{bit: 128, bits: 16}.put(v, 0xFFFF)` | `Ok`;`v[2] == 0x1111_1111_1111_FFFF` |
| EVT-078.5 | `VField{bit: 64, bits: 64}.put(v, u64::MAX)` | `Ok`;`v[1] == u64::MAX`,其余元素不变 |
| EVT-078.6 | `VField{bit: 176, bits: 8}.put(v, 0x07)` 后 `get` | `0x07`;`v[2]` 只有位 48..56 改变 |

### EVT-080 AdvisorLag 只在落后量超过阈值时写出

- 规则:[控制与系统](README.md#控制与系统0x04xx0x05xx)
- 级别:table
- 里程碑:M2
- 前置:`advisor_max_lag_ms = 5000`;一个策略实例 `strategy = 3`;已提交末条记录 seq = 1 000 500、已消费末条 seq = 1 000 000;两条记录同属本次运行;评估一次(该实例此前没有写过本码)
- 实现:`crates/hft-sequencer/tests/advisor_lag.rs::evt_080_threshold_boundary`

| 行 | 输入 `lag_ns` | 期望 |
|---|---|---|
| EVT-080.1 | 4 999 999 999 | 不写 `AdvisorLag` |
| EVT-080.2 | 5 000 000 000 | 不写(阈值是严格大于) |
| EVT-080.3 | 5 000 000 001 | 写一条 `Note{source = Advisor, code = AdvisorLag, level = P1}`,`v[0] = 3`、`v[1] = 5000`、`v[2] = 500`、`v[3] = 1 000 000`,`Note.strategy = 3` |
| EVT-080.4 | 12 345 678 901 | 写一条,`v[1] = 12345`(ms 向下取整) |

### EVT-081 每个实例每 10 s 引擎时间至多写一条 AdvisorLag

- 规则:[控制与系统](README.md#控制与系统0x04xx0x05xx)
- 级别:unit
- 里程碑:M2
- 前置:`advisor_max_lag_ms = 5000`;实例 3 与实例 4 在已提交末条记录的 `engine_ns` 从 T0 到 T0 + 25 s 期间落后量恒为 8 s;advisor 每 100 ms(按已提交末条记录的 `engine_ns`)评估一次
- 步骤:
  1. 驱动 251 次评估(T0 到 T0 + 25 s,含两端)
- 期望:
  - 实例 3 恰好 3 条,`engine_ns` 依次为 T0、T0 + 10 000 ms、T0 + 20 000 ms;实例 4 同样恰好 3 条,与实例 3 互不影响
  - T0 + 9 900 ms 的评估不写;T0 + 10 000 ms 写
- 实现:`crates/hft-sequencer/tests/advisor_lag.rs::evt_081_rate_limit_per_instance`

### EVT-082 落后量回落到阈值以下后不再写 AdvisorLag

- 规则:[控制与系统](README.md#控制与系统0x04xx0x05xx)
- 级别:unit
- 里程碑:M2
- 前置:`advisor_max_lag_ms = 5000`;实例 3;落后量:T0 为 8 s,T0 + 1 s 起为 4 s 并保持到 T0 + 30 s,T0 + 31 s 起为 9 s;每 100 ms 评估一次直到 T0 + 32 s
- 步骤:
  1. 驱动全部评估
- 期望:
  - 恰好 2 条:T0 一条;T0 + 31 000 ms 一条(距上一条 > 10 s);T0 + 1 s 到 T0 + 30.9 s 之间没有
- 实现:`crates/hft-sequencer/tests/advisor_lag.rs::evt_082_no_note_below_threshold`

### EVT-083 AdvisorLag 按 engine_ns 计,停机时长不计入

- 规则:[控制与系统](README.md#控制与系统0x04xx0x05xx)、[event-model 时间戳语义](../../docs/subsystems/event-model.md#时间戳语义)
- 级别:unit
- 里程碑:M2
- 前置:`advisor_max_lag_ms = 5000`;上次运行最后消费的记录 `engine_ns` = 100 000 000 000;停机 1 h 后启动,`EngineClock.base_engine_ns` = 100 000 000 000;本次运行已提交末条记录的 `engine_ns` = 106 000 000 000(记录头 tsc 与上次运行不可比)
- 步骤:
  1. 评估实例 3
- 期望:
  - 写一条 `AdvisorLag`,`v[1] = 6000`(不是 3 606 000)
- 实现:`crates/hft-sequencer/tests/advisor_lag.rs::evt_083_engine_ns_across_restart`

### EVT-084 sequencer 从不写 AdvisorLag

- 规则:[控制与系统](README.md#控制与系统0x04xx0x05xx)
- 级别:unit
- 里程碑:M2
- 前置:`fixtures/replay/` 的一个样本 journal,其中 advisor 落后超过 `advisor_max_lag_ms = 5000` 的区间至少 30 s
- 步骤:
  1. 以验证回放驱动 sequencer 处理全部 Input
- 期望:
  - sequencer 产出的记录中没有 `code` 为 `AdvisorLag` 的 `Note`;journal 中已有的 `AdvisorLag` 的 `source` 都是 `Advisor`
- 实现:`crates/hft-sequencer/tests/advisor_lag.rs::evt_084_sequencer_never_writes_advisor_lag`

### EVT-090 CapRecord 与 CmdScope 的布局

- 规则:[cap 记录](README.md#cap-记录)、[控制命令编码](README.md#控制命令编码)
- 级别:table
- 里程碑:M1
- 前置:无参数
- 实现:`crates/hft-event/tests/layout.rs::evt_090_cap_layouts`

| 行 | 结构 | 期望 |
|---|---|---|
| EVT-090.1 | `CapRecord` | size 48、align 8;`source`@0、`level`@1、`cflags`@2、`_pad`@3、`scope`@8、`set_seq`@24、`params`@32 |
| EVT-090.2 | `CmdScope` | size 16、align 4;`strategy`@0、`venue`@1、`account`@2、`_pad`@3、`group`@4、`inst`@8、`_pad2`@12 |

### EVT-091 CapRecord 的 48 字节经 Note{CapSet} 的 v[0..6] 原样往返

- 规则:[cap 记录](README.md#cap-记录)、[Note 来源与码登记表](README.md#note-来源与码登记表)
- 级别:unit
- 里程碑:M1
- 前置:`CapRecord{source = Kill(1), level = 3(Account), cflags = HARD, scope = {strategy 0, venue 0, account 5, group GROUP_ALL, inst 0}, set_seq = 77, params = [3, 0]}`
- 步骤:
  1. 把其 48 字节按小端填入 `Note.v[0..6]`,`v[6] = v[7] = 0`,`source = Risk`、`level = Info`,编码为 `Note` 记录
  2. 解码,取 `v[0..6]` 的 48 字节按 `CapRecord` 读回
- 期望:
  - 读回的 `CapRecord` 与原值逐字段相等;`scope.group == u32::MAX`;`v[6] == v[7] == 0`
- 实现:`crates/hft-event/tests/notes.rs::evt_091_cap_record_in_note`

### EVT-100 MEASURE_MASK 恰好登记五段体内字节

- 规则:[测量字段(验证回放屏蔽)](README.md#测量字段验证回放屏蔽)
- 级别:table
- 里程碑:M1
- 前置:`MEASURE_MASK`;断言条目数为 5 且无其他条目(记录头 `tsc` 不在表中)
- 实现:`crates/hft-event/tests/measure.rs::evt_100_measure_mask_entries`

| 行 | 输入 | 期望 |
|---|---|---|
| EVT-100.1 | `OrderDecision` | `(0x0201, 8, 8)` |
| EVT-100.2 | `SwapDecision` | `(0x0303, 24, 8)` |
| EVT-100.3 | `StateSnapshot` `snap_tsc` | `(0x0504, 56, 8)` |
| EVT-100.4 | `StateSnapshot` `boot_id` | `(0x0504, 64, 16)` |
| EVT-100.5 | `StateSnapshot` `snap_utc_ns` | `(0x0504, 80, 8)` |
| EVT-100.6 | `ExecDecision`、`CmdResult`、`L2Snapshot`、`ConfigBlob`、`FundingSettle`、`Note` | 无条目 |

### EVT-101 Output kind 的 FieldDesc.measure 与 MEASURE_MASK 一致

- 规则:[测量字段(验证回放屏蔽)](README.md#测量字段验证回放屏蔽)、[记录头、标志与 kind](README.md#记录头标志与-kind)
- 级别:unit
- 里程碑:M1
- 前置:`KINDS` 中 `role ∈ {Output, ByBody}` 的 kind 与 `StateSnapshot` 的 `SnapshotHdr` 字段表
- 步骤:
  1. 对每个这样的 kind,收集 `measure = true` 的字段 `(offset, width)`
- 期望:
  - 收集结果与 `MEASURE_MASK` 中同 kind 的条目集合相等:`OrderDecision` 只有 `decided_tsc`;`SwapDecision` 只有 `decided_tsc`;`StateSnapshot` 只有 `snap_tsc`、`boot_id`、`snap_utc_ns`;其余为空
  - `FundingSettle` 的 `rx_tsc` `measure = false`(合成记录写 0,参与比对)
- 实现:`crates/hft-event/tests/measure.rs::evt_101_measure_flags_consistent`

### EVT-102 按测量掩码比较时只忽略登记的字节

- 规则:[测量字段(验证回放屏蔽)](README.md#测量字段验证回放屏蔽)
- 级别:table
- 里程碑:M1
- 前置:比较方法:把记录头 `tsc`(偏移 16..24)与 `MEASURE_MASK` 中该 kind 的体内字节(`Multi` 按逻辑体)置零后逐字节比较;每行两条记录只在"输入"所列字段不同
- 实现:`crates/hft-event/tests/measure.rs::evt_102_masked_compare`

| 行 | 输入 | 期望 |
|---|---|---|
| EVT-102.1 | 两条 `OrderDecision`,记录头 `tsc` 与 `decided_tsc` 不同 | 相等 |
| EVT-102.2 | 两条 `OrderDecision`,`px` 相差 1 | 不等 |
| EVT-102.3 | 两个 `StateSnapshot` 逻辑体,`snap_tsc`、`boot_id`、`snap_utc_ns` 不同 | 相等 |
| EVT-102.4 | 两个 `StateSnapshot` 逻辑体,`at_seq` 不同 | 不等 |
| EVT-102.5 | 两个 `StateSnapshot` 逻辑体,`engine_ns` 不同 | 不等 |
| EVT-102.6 | 两条 `ExecDecision`,`depth_qty` 相差 1 | 不等 |
| EVT-102.7 | 两条 `SwapDecision`,`decided_tsc` 不同 | 相等 |
| EVT-102.8 | 两条 `FundingSettle{synth = 1}`,`rx_tsc` 分别为 0 与 5 | 不等 |

### EVT-110 每个定时器类别按编码表打包

- 规则:[定时器编码](README.md#定时器编码)
- 级别:table
- 里程碑:M1
- 前置:`G = GroupId(0x0300_0001)`;每行断言 `TimerCode::encode` 的 `(owner, timer_id, group)`
- 实现:`crates/hft-event/tests/timer.rs::evt_110_timer_encode_table`

| 行 | 输入 | 期望 |
|---|---|---|
| EVT-110.1 | `Sys(Snapshot)` | `(1, 0x0000_0001, 0)` |
| EVT-110.2 | `Sys(Roll)` | `(1, 0x0000_0002, 0)` |
| EVT-110.3 | `Sys(DayRoll{day: 20 000})` | `(1, 0x004E_2003, 0)` |
| EVT-110.4 | `Sys(DeadMan{venue: 1, account: 3})` | `(1, 0x0003_0104, 0)` |
| EVT-110.5 | `Sys(ConnRotate{venue: 5, account: 0})` | `(1, 0x0000_0505, 0)` |
| EVT-110.6 | `Sys(AdvisorTick)` | `(1, 0x0000_0006, 0)` |
| EVT-110.7 | `Sys(SnapshotNow)` | `(1, 0x0000_0007, 0)` |
| EVT-110.8 | `Sys(ClockOk)` / `Sys(ClockBad)` | `(1, 8, 0)` / `(1, 9, 0)` |
| EVT-110.9 | `Sys(ConflateReport)` | `(1, 0x0000_000A, 0)` |
| EVT-110.10 | `Sys(SessionKeepalive{venue: 2, account: 4})` | `(1, 0x0004_020B, 0)` |
| EVT-110.11 | `Sys(PersistLag{level: 1})` | `(1, 0x0000_010C, 0)` |
| EVT-110.12 | `Sys(DiskLow{level: 2})` | `(1, 0x0000_020D, 0)` |
| EVT-110.13 | `Sys(StorageOk)` | `(1, 0x0000_000E, 0)` |
| EVT-110.14 | `Sys(FundingSettle{venue: 10})` | `(1, 0x0000_0A0F, 0)` |
| EVT-110.15 | `Sys(KillSelfTestOk{account: 7})` | `(1, 0x0007_0010, 0)` |
| EVT-110.16 | `Sys(NotifyContinue)` | `(1, 0x0000_0011, 0)` |
| EVT-110.17 | `Sys(RefdataRecheck{arg: 0})` / `{arg: 5}` | `(1, 0x0000_0012, 0)` / `(1, 0x0000_0512, 0)` |
| EVT-110.18 | `Keyframe{inst: 123 456}` | `(2, 123 456, 0)` |
| EVT-110.19 | `Oms(Continue)` / `Oms(Sweep)` | `(3, 1, 0)` / `(3, 2, 0)` |
| EVT-110.20 | `Oms(UnknownQuery{table_ix: 7, slot: 1 000})` | `(3, 0x03E8_0703, 0)` |
| EVT-110.21 | `Oms(HedgeRetry{link_id: 0x12_3456})` | `(3, 0x1234_5604, 0)` |
| EVT-110.22 | `Oms(SmokeTtl{account: 2, venue: 6})` | `(3, 0x0006_0205, 0)` |
| EVT-110.23 | `Risk{kind: 1 / 2 / 3, account: 3, venue: 1}` | `(4, 0x0001_0301 / 0x0001_0302 / 0x0001_0303, 0)` |
| EVT-110.24 | `Exec{kind: 1..=6, account: 2, group: G}` | `(5, 0x0201..=0x0206, 0x0300_0001)` |
| EVT-110.25 | `Exec{kind: 1, account: 2, group: GROUP_NONE}`(无组 cell) | `(5, 0x0201, 0)` |
| EVT-110.26 | `Strategy{strategy: 3, tag: 0xDEAD_BEEF, group: 0x0300_0005}` | `(19, 0xDEAD_BEEF, 0x0300_0005)` |
| EVT-110.27 | `Strategy{strategy: 1, tag: 0, group: GROUP_NONE}` / `{strategy: 254, ...}` | owner 17 / owner 270 |

### EVT-111 任意合法定时器码打包后解包得到原值

- 规则:[定时器编码](README.md#定时器编码)
- 级别:prop
- 里程碑:M1
- 性质:对任意合法 `TimerCode` c:`encode(c)` 为 `Ok((o, t, g))` 且 `decode(o, t, g) == Ok(c)`;`Exec`/`Strategy` 以外 `g == GROUP_NONE`
- 生成:均匀选类别;`Sys` 均匀选 18 个 id,参数取合法域(`day` 0..2^24、`level` ∈ {1, 2}、`arg` 0..=5、venue/account 0..=255);`Keyframe.inst` 全域;`UnknownQuery` `table_ix` 0..256、`slot` 0..65 536;`HedgeRetry.link_id` 0..2^24;`Risk.kind` 1..=3;`Exec.kind` 1..=6;`Strategy.strategy` 1..=254、`tag` 全域;`group` 取 `GROUP_NONE` 或合法 `GroupId`
- 参考:输入本身
- 例数:PR 5 000
- 实现:`crates/hft-event/tests/timer.rs::evt_111_timer_roundtrip`

### EVT-112 定时器打包对越界参数与非法 group 返回错误

- 规则:[定时器编码](README.md#定时器编码)
- 级别:table
- 里程碑:M1
- 前置:无参数
- 实现:`crates/hft-event/tests/timer.rs::evt_112_timer_encode_errors`

| 行 | 输入 | 期望 |
|---|---|---|
| EVT-112.1 | `Strategy{strategy: 0, ..}` | `Err(Owner)` |
| EVT-112.2 | `Strategy{strategy: 255, ..}` | `Err(Owner)` |
| EVT-112.3 | `Risk{kind: 0, ..}` / `Risk{kind: 4, ..}` | `Err(Id)` / `Err(Id)` |
| EVT-112.4 | `Exec{kind: 0, ..}` / `Exec{kind: 7, ..}` | `Err(Id)` / `Err(Id)` |
| EVT-112.5 | `Sys(DayRoll{day: 0xFF_FFFF})` | `Ok((1, 0xFFFF_FF03, 0))` |
| EVT-112.6 | `Sys(DayRoll{day: 0x100_0000})` | `Err(Range)` |
| EVT-112.7 | `Oms(UnknownQuery{table_ix: 255, slot: 65 535})` | `Ok((3, 0xFFFF_FF03, 0))` |
| EVT-112.8 | `Oms(UnknownQuery{table_ix: 256, slot: 0})` | `Err(Range)` |
| EVT-112.9 | `Oms(UnknownQuery{table_ix: 0, slot: 65 536})` | `Err(Range)` |
| EVT-112.10 | `Oms(HedgeRetry{link_id: 0xFF_FFFF})` | `Ok((3, 0xFFFF_FF04, 0))` |
| EVT-112.11 | `Oms(HedgeRetry{link_id: 0x100_0000})` | `Err(Range)` |
| EVT-112.12 | `Sys(PersistLag{level: 0})` / `{level: 3}` | `Err(Range)` / `Err(Range)` |
| EVT-112.13 | `Sys(DiskLow{level: 0})` / `{level: 3}` | `Err(Range)` / `Err(Range)` |
| EVT-112.14 | `Sys(RefdataRecheck{arg: 6})` | `Err(Range)` |
| EVT-112.15 | `Exec{kind: 1, account: 2, group: GROUP_ALL}` | `Err(Group)` |
| EVT-112.16 | `Strategy{strategy: 3, tag: 1, group: GROUP_ALL}` | `Err(Group)` |

### EVT-113 定时器解包对未分配 owner、未登记 id、越界参数与错用 group 返回错误

- 规则:[定时器编码](README.md#定时器编码)
- 级别:table
- 里程碑:M1
- 前置:输入写作 `(owner, timer_id, group)`
- 实现:`crates/hft-event/tests/timer.rs::evt_113_timer_decode_errors`

| 行 | 输入 | 期望 |
|---|---|---|
| EVT-113.1 | `(0, 1, 0)` | `Err(Owner)` |
| EVT-113.2 | `(6, 1, 0)` | `Err(Owner)` |
| EVT-113.3 | `(16, 1, 0)` | `Err(Owner)` |
| EVT-113.4 | `(271, 1, 0)` | `Err(Owner)` |
| EVT-113.5 | `(0xFFFF, 1, 0)` | `Err(Owner)` |
| EVT-113.6 | `(17, 0, 0)` | `Ok(Strategy{strategy: 1, tag: 0, group: GROUP_NONE})` |
| EVT-113.7 | `(270, 9, 0x0300_0001)` | `Ok(Strategy{strategy: 254, tag: 9, group: 0x0300_0001})` |
| EVT-113.8 | `(1, 0, 0)` / `(1, 19, 0)` | `Err(Id)` / `Err(Id)` |
| EVT-113.9 | `(1, 0x0000_0101, 0)`(Snapshot 带参数位) | `Err(Id)` |
| EVT-113.10 | `(1, 0x0007_0110, 0)`(KillSelfTestOk 的位 8..15 非 0) | `Err(Id)` |
| EVT-113.11 | `(1, 0x0100_0505, 0)`(ConnRotate 的位 24..31 非 0) | `Err(Id)` |
| EVT-113.12 | `(1, 0x0000_030C, 0)` / `(1, 0x0000_000C, 0)`(PersistLag level 3 / 0) | `Err(Range)` / `Err(Range)` |
| EVT-113.13 | `(1, 0x0000_0612, 0)`(RefdataRecheck arg 6) | `Err(Range)` |
| EVT-113.14 | `(3, 0, 0)` / `(3, 6, 0)` | `Err(Id)` / `Err(Id)` |
| EVT-113.15 | `(3, 0x0100_0205, 0)`(SmokeTtl 的位 24..31 非 0) | `Err(Id)` |
| EVT-113.16 | `(4, 4, 0)` / `(4, 0x0100_0301, 0)` | `Err(Id)` / `Err(Id)` |
| EVT-113.17 | `(5, 7, 0)` / `(5, 0x0001_0201, 0)` | `Err(Id)` / `Err(Id)` |
| EVT-113.18 | `(1, 1, 5)`(Sys 带 group) | `Err(Group)` |
| EVT-113.19 | `(5, 0x0201, 0xFFFF_FFFF)` / `(19, 1, 0xFFFF_FFFF)` | `Err(Group)` / `Err(Group)` |
| EVT-113.20 | `(0, 19, 5)` | `Err(Owner)`(先查 owner) |
| EVT-113.21 | `(1, 19, 5)` | `Err(Id)`(id 先于 group) |
| EVT-113.22 | `(1, 0x0000_030C, 5)` | `Err(Range)`(参数先于 group) |
| EVT-113.23 | `(2, 0, 7)`(Keyframe 带 group) | `Err(Group)` |

### EVT-120 AdviceHdr 的布局与载荷上限常量

- 规则:[Advice(0x0503,Var,Input)](README.md#advice0x0503varinput)
- 级别:unit
- 里程碑:M1
- 前置:无参数
- 步骤:
  1. 读取 `size_of`、`align_of`、各 `offset_of` 与 `ADVICE_MAX_PAYLOAD`
- 期望:
  - `AdviceHdr` size 32、align 8;`strategy`@0、`account`@1、`local_kind`@2、`group`@4、`len`@8、`_pad`@10、`basis_seq`@16、`basis_tsc`@24
  - `ADVICE_MAX_PAYLOAD == 4096`;`KINDS` 中 `Advice` 的 `Var.max == 4096`(与常量同值)
- 实现:`crates/hft-event/tests/layout.rs::evt_120_advice_hdr_layout`

### EVT-121 AdviceRecord 编码后读回相同且按 8 B 补零

- 规则:[Advice(0x0503,Var,Input)](README.md#advice0x0503varinput)、[环元素类型](README.md#环元素类型)
- 级别:table
- 里程碑:M1
- 前置:`AdviceHdr{strategy: 3, account: 0, local_kind: 2, group: GROUP_NONE, basis_seq: 900, basis_tsc: 12 345}`,`len` = 载荷长;载荷为递增字节;输出缓冲 8 KiB 预填 `0xAA`
- 实现:`crates/hft-event/tests/ring.rs::evt_121_advice_record_roundtrip`

| 行 | 输入载荷长 | 期望 |
|---|---|---|
| EVT-121.1 | 0 | `encoded_len == 32`,`encode_into` 返回 32;`read` 得同一头、空载荷 |
| EVT-121.2 | 1 | `encoded_len == 40`;偏移 33..40 为 0;`read` 载荷长 1 |
| EVT-121.3 | 7 | 40;偏移 39 为 0 |
| EVT-121.4 | 8 | 40;无填充 |
| EVT-121.5 | 9 | 48;偏移 41..48 为 0 |
| EVT-121.6 | 4 096 | 4 128;`read` 载荷与输入逐字节相同;偏移 4 128 起仍为 `0xAA` |

### EVT-122 AdviceRecord 拒绝超长、长度不符、缓冲不足与截断

- 规则:[Advice(0x0503,Var,Input)](README.md#advice0x0503varinput)、[环元素类型](README.md#环元素类型)
- 级别:table
- 里程碑:M1
- 前置:同 EVT-121 的头,每行改所列字段
- 实现:`crates/hft-event/tests/ring.rs::evt_122_advice_record_errors`

| 行 | 输入 | 期望 |
|---|---|---|
| EVT-122.1 | `encode_into`,`len = 4 097`,载荷 4 097 B | `Err(TooLong)` |
| EVT-122.2 | `encode_into`,`hdr.len = 10`,载荷 9 B | `Err(Count)` |
| EVT-122.3 | `encode_into`,载荷 1 B,输出 39 B | `Err(BufTooSmall)` |
| EVT-122.4 | `read`,输入 31 B | `Err(Short)` |
| EVT-122.5 | `read`,`hdr.len = 100`,输入 128 B | `Err(Short)` |
| EVT-122.6 | `read`,`hdr.len = 4 097`,输入 4 136 B | `Err(TooLong)` |

### EVT-123 hft-event 的依赖闭包不含策略、SDK、序列化框架与 L1 以上 crate

- 规则:[策略载荷容器](README.md#策略载荷容器)、[依赖](README.md#依赖)
- 级别:unit
- 里程碑:M1
- 前置:`cargo metadata --format-version 1` 的输出;工作区 crate 的检查含 dev 与 build 依赖,外部 crate(serde、SBE)的检查只看 normal 与 build 闭包(criterion、trybuild 等 dev 工具自带 serde,不进生产闭包)
- 步骤:
  1. 求 `hft-event` 的完整依赖闭包
- 期望:
  - 闭包内的工作区 crate 只有 `hft-types`、`hft-refdata`
  - 闭包内没有 `hft-strategy-sdk`、`hft-strategy-registry`、`strategies/*` 下的任何 crate、`serde`、名字含 `sbe` 的 crate、`hft-book`/`hft-oms`/`hft-risk`/`hft-ledger`/`hft-exec`/`hft-sequencer`/`hft-rt`/`hft-journal`/`hft-net`/`hft-venue`/`hft-venue-proto`
  - 新增或修改策略不需要改本 crate 的机检由 `cargo xtask scaffold-gate` 承担(上线门槛 ⑧),本用例只断言依赖方向
- 实现:`crates/hft-event/tests/deps.rs::evt_123_dependency_closure`

### EVT-124 advice 环帧承载 Advice 与 Advisor 源的 Note,拒绝其他 kind 与其他 source

- 规则:[环元素类型](README.md#环元素类型)、[Note 来源与码登记表](README.md#note-来源与码登记表)
- 级别:table
- 里程碑:M1
- 前置:EVT-121 的头;`N` = `Note{code = note_code(Advisor, n_AdvisorLag), level = P1, source = Advisor, ..}`;输出缓冲 8 KiB 预填 `0xAA`
- 实现:`crates/hft-event/tests/ring.rs::evt_124_advice_msg_frames`

| 行 | 输入 | 期望 |
|---|---|---|
| EVT-124.1 | `encode_into(AdviceMsg::Advice(载荷 9 B))` | 返回 56;字节 0..2 = `0x0503`、4..8 = 48;`read` 得 `Advice`,头与载荷相同 |
| EVT-124.2 | `encode_into(AdviceMsg::Note(N))` | 返回 88;字节 0..2 = `0x0506`、4..8 = 80;`read` 得 `Note(N)` |
| EVT-124.3 | `encode_into(Note{source = Housekeeping, ..})` | `Err(Count)` |
| EVT-124.4 | `read`,帧头 `kind = 0x0506`,体为 `source = Boot` 的 Note | `Err(InvalidEnum)` |
| EVT-124.5 | `read`,帧头 `kind = 0x0401` | `Err(KindMismatch)` |
| EVT-124.6 | `read`,帧头 `kind = 0x0506`、`len = 72` | `Err(Short)` |
| EVT-124.7 | `read`,帧头 `kind = 0x0506`、`len = 88` | `Err(TooLong)` |
| EVT-124.8 | `read`,输入 7 B | `Err(Short)` |
| EVT-124.9 | `encode_into(Note(N))`,输出 87 B | `Err(BufTooSmall)` |

### EVT-130 SPB1 容器结构的布局

- 规则:[StrategyParams(SPB1)](README.md#strategyparamsspb1)
- 级别:table
- 里程碑:M1
- 前置:无参数
- 实现:`crates/hft-event/tests/layout.rs::evt_130_spb1_layouts`

| 行 | 结构 | 期望 |
|---|---|---|
| EVT-130.1 | `StrategyParamsHdr` | size 48、align 8;`strategy`@0、`account`@1、`kind_id`@2、`schema_id`@4、`schema_ver`@8、`source`@10、`_pad`@11、`n_groups`@12、`_pad2`@14、`version`@16、`generated_at_ns`@24、`body_len`@32、`group_ctr_hwm`@36、`schema_digest`@40 |
| EVT-130.2 | `GroupSpec` | size 80、align 8;`id`@0、`asset`@4、`n_legs`@6、`open_allowed`@7、`legs`@8(4 × 16 B)、`_rsv`@72 |
| EVT-130.3 | `LegDef` | size 16、align 4;`inst`@0、`venue`@4、`kind`@5、`_pad`@6、`route`@8、`_pad2`@12 |

### EVT-131 合法的 SPB1 blob 解析为头、group 切片与原样 body

- 规则:[StrategyParams(SPB1)](README.md#strategyparamsspb1)
- 级别:unit
- 里程碑:M1
- 前置:blob B(224 B):头 `strategy = 3`、`account = 2`、`source = Pipeline`、`n_groups = 2`、`body_len = 16`、`group_ctr_hwm = 2`;group 0 `id = 0x0300_0001`、`n_legs = 2`(两条 Cex 腿,`inst` 10 与 20,`route = 0`)、`open_allowed = 1`;group 1 `id = 0x0300_0002`、`n_legs = 1`、`open_allowed = 0`;body 16 B 为 `0x00..=0x0F`
- 步骤:
  1. `parse_spb1(&B)`
- 期望:
  - `Ok(view)`;`view.hdr.strategy == 3`;`view.groups.len() == 2`,`view.groups[1].id == 0x0300_0002`
  - `view.body.as_ptr() == B[208..].as_ptr()`,`view.body.len() == 16`,内容为 `0x00..=0x0F`
  - `view.groups[0].view(..)`:2 条腿、第 2 条 `inst == 20`、`open_allowed == true`;`view.groups[1].view(..).open_allowed == false`
- 实现:`crates/hft-event/tests/spb1.rs::evt_131_parse_ok`

### EVT-132 SPB1 容器级校验对每类错误返回对应码

- 规则:[StrategyParams(SPB1)](README.md#strategyparamsspb1)
- 级别:table
- 里程碑:M1
- 前置:从 EVT-131 的 blob B 出发,每行只改所列内容(改长度时同步截断或追加)
- 实现:`crates/hft-event/tests/spb1.rs::evt_132_parse_errors`

| 行 | 输入 | 期望 |
|---|---|---|
| EVT-132.1 | 只取前 47 B | `Err(Truncated)` |
| EVT-132.2 | `n_groups = 3`,长度仍 224 | `Err(Truncated)`(48 + 240 > 224) |
| EVT-132.3 | `body_len = 12`,总长 220 | `Err(BadLen)` |
| EVT-132.4 | 末尾多 8 B(总长 232) | `Err(BadLen)` |
| EVT-132.5 | `source = 0` / `source = 3` | `Err(BadHdr)` / `Err(BadHdr)` |
| EVT-132.6 | 头 `_pad = 1` / `_pad2 = 1` | `Err(Padding)` / `Err(Padding)` |
| EVT-132.7 | group 1 未用的 `legs[1].inst = 5` | `Err(Padding)` |
| EVT-132.8 | group 0 `_rsv = 1` | `Err(Padding)` |
| EVT-132.9 | group 0 有效腿 `legs[0]._pad2 = 1` | `Err(Padding)` |
| EVT-132.10 | group 0 `id = 0x0400_0001`(高 8 位 ≠ 3) | `Err(BadGroup)` |
| EVT-132.11 | group 0 `id = 0`(`GROUP_NONE`) / `id = 0xFFFF_FFFF`(`GROUP_ALL`) | `Err(BadGroup)` / `Err(BadGroup)` |
| EVT-132.12 | group 0 `id = 0x0300_0000`(计数器 0) | `Err(BadGroup)` |
| EVT-132.13 | group 0 `id = 0x03FF_FFFF`(计数器 = `N_MAX` + 1) | `Err(BadGroup)` |
| EVT-132.14 | group 0 `id = 0x03FF_FFFE`(计数器 = `N_MAX`),`group_ctr_hwm = N_MAX` | `Ok` |
| EVT-132.15 | group 0 `n_legs = 0` / `n_legs = 5` | `Err(BadGroup)` / `Err(BadGroup)` |
| EVT-132.16 | group 0 `n_legs = 4`,四条腿都合法 | `Ok` |
| EVT-132.17 | group 0 `open_allowed = 2` | `Err(BadGroup)` |
| EVT-132.18 | group 1 `id = 0x0300_0001`(与 group 0 相同) | `Err(DupGroup)` |
| EVT-132.19 | group 0 `legs[0].inst = 0` | `Err(BadLeg)` |
| EVT-132.20 | group 0 `legs[0].kind = 0` / `kind = 3` | `Err(BadLeg)` / `Err(BadLeg)` |
| EVT-132.21 | group 0 Cex 腿 `route = 1` | `Err(BadLeg)` |
| EVT-132.22 | group 0 `legs[0].kind = 2`(Chain)、`route = 7` | `Ok` |
| EVT-132.23 | `n_groups = 0`、`body_len = 0`,总长 48 | `Ok`,`groups` 与 `body` 都为空 |
| EVT-132.24 | `group_ctr_hwm = 1`(group 1 的计数器为 2) | `Err(BadGroup)` |
| EVT-132.25 | `group_ctr_hwm = 3` | `Ok` |
| EVT-132.26 | `group_ctr_hwm = N_MAX` | `Ok` |
| EVT-132.27 | `group_ctr_hwm = N_MAX + 1` | `Err(BadHdr)` |
| EVT-132.28 | `n_groups = 0`、`body_len = 0`、`group_ctr_hwm = 7`(总长 48) | `Ok`(移除全部 group 后 hwm 保留) |

### EVT-133 任意合法 SPB1 blob 解析后原样拼回,body 不被检查或改动

- 规则:[StrategyParams(SPB1)](README.md#strategyparamsspb1)
- 级别:prop
- 里程碑:M1
- 性质:`parse_spb1(blob)` 为 `Ok(v)`;`v.hdr`、`v.groups`、`v.body` 的字节依次拼接等于 `blob`;`v.body` 指向 blob 内同一段内存;body 的内容不影响结果
- 生成:`strategy` 1..=254;`n_groups` 1..=500,计数器在 `1..=N_MAX` 中随机且互不相同;`group_ctr_hwm` 在 [最大计数器, `N_MAX`] 中均匀;每 group `n_legs` 1..=4、腿 `inst` 1..=u32::MAX、`kind = Cex`、`route = 0`、`open_allowed` ∈ {0, 1};body 长 0..=65 536 且为 8 的倍数,内容随机字节(含全 0xFF)
- 参考:输入 blob 本身
- 例数:PR 500
- 实现:`crates/hft-event/tests/prop_spb1.rs::evt_133_spb1_roundtrip_opaque_body`

### EVT-140 环元素类型的大小上限与 Copy

- 规则:[环元素类型](README.md#环元素类型)
- 级别:table
- 里程碑:M1
- 前置:编译期断言之外,运行期再断言一次;`fn assert_copy<T: Copy>()` 对每个类型实例化
- 实现:`crates/hft-event/tests/ring.rs::evt_140_ring_element_sizes`

| 行 | 类型 | 期望 |
|---|---|---|
| EVT-140.1 | 含来源标志的 `FeedMsg` | `size_of ≤ 128`;`Copy` |
| EVT-140.2 | 含来源标志的 `RepMsg` | `size_of ≤ 144`;`Copy` |
| EVT-140.3 | `HkMsg` | `size_of ≤ 128`;`Copy` |
| EVT-140.4 | `FeedCmd` | `size_of ≤ 16`;`Copy` |
| EVT-140.5 | `GwCmd` | `size_of == 64`、`align_of == 8`;`Copy` |
| EVT-140.6 | `OutMsg` | 与 `GwCmd` 是同一类型(`TypeId` 相等) |

### EVT-141 GwCmd 的字段偏移与规格一致

- 规则:[环元素类型](README.md#环元素类型)
- 级别:unit
- 里程碑:M1
- 前置:无参数
- 步骤:
  1. 读取各 `offset_of`
- 期望:
  - `decision_seq`@0、`decided_tsc`@8、`px`@16、`qty`@24、`inst`@32、`slot`@36、`gen`@40、`target_slot`@44、`op`@48、`account`@49、`side`@50、`tif`@51、`oflags`@52、`_pad`@54、`arg`@56、`arg2`@60
- 实现:`crates/hft-event/tests/ring.rs::evt_141_gwcmd_layout`

### EVT-142 GwCmd 构造器与访问器按规定的字段装取

- 规则:[环元素类型](README.md#环元素类型)
- 级别:table
- 里程碑:M1
- 前置:无参数
- 实现:`crates/hft-event/tests/ring.rs::evt_142_gwcmd_ctors`
- 备注:"其余字段 0"不含 `side`/`tif`:维护类与模拟类构造器写占位值 `Buy`/`Gtc`(枚举无 0 值;gw 与 sim-gw 不读)

| 行 | 输入 | 期望 |
|---|---|---|
| EVT-142.1 | `mirror_restore(account 2, inst 77, slot 9, gen 3, epoch 0x7FFF, exch_order_id u64::MAX)` | `op == MirrorRestore`(12)、`account == 2`、`inst == 77`、`slot == 9`、`gen == 3`、`qty == 0`、`arg == 0x0000_7FFF`、`decision_seq == 0`、`oflags == 0`;`exch_order_id() == u64::MAX`;`epoch() == 0x7FFF` |
| EVT-142.2 | `mirror_restore(..., exch_order_id 0x8000_0000_0000_0001)` | `exch_order_id()` 返回同值 |
| EVT-142.3 | `mirror_restore(..., epoch 0, exch_order_id 0)` | `arg == 0`;`epoch() == 0`;`exch_order_id() == 0` |
| EVT-142.4 | `set_epoch(0x1234, true, 0x1200)` | `op == SetEpoch`(10)、`arg == 0x0000_1234`、`arg2 == 0x1200_0001`,其余字段 0;`epoch() == 0x1234`、`accept_prev() == true`、`prev_epoch() == 0x1200` |
| EVT-142.5 | `sim_reset(account 9)` | `op == SimReset`(13)、`account == 9`,其余字段全 0 |
| EVT-142.6 | `sim_transfer(account 9, from 1, to 2, asset 0xFFFF, amount 5 000 000)` | `op == SimTransfer`(14)、`qty == 5 000 000`、`arg == 0x0201_FFFF`、`px == 0` |
| EVT-142.7 | `sim_transfer(account 9, from 255, to 255, asset 0, amount 1)` | `arg == 0xFFFF_0000` |
| EVT-142.8 | `set_epoch(70, false, 5)` | `arg2 == 0x0005_0000`;`accept_prev() == false`、`prev_epoch() == 5` |
| EVT-142.9 | `set_epoch(u16::MAX, true, u16::MAX)` | `arg == 0x0000_FFFF`、`arg2 == 0xFFFF_0001` |
| EVT-142.10 | `sim_fault(account 9, venue 3, fault 4(DelayReports), delay_ms 3 000)` | `op == SimFault`(15)、`account == 9`、`arg == 0x0000_0403`、`arg2 == 3 000`,其余字段 0 |
| EVT-142.11 | `bind_wire(inst 77, part 1, 字节 0xF0..=0xFF)` | `op == BindWire`(17)、`account == 0`、`inst == 77`、`arg == 1`;`px` = 前 8 字节大端、`qty` = 后 8 字节大端,其余字段 0;`wire_chunk()` 返回原 16 字节 |
| EVT-142.12 | `bind_wire(inst 1, part 0, 全零)` | `wire_chunk()` 全零 |
| EVT-142.13 | `deliver(account 9, venue 15, inst 4, payout 1000, end_s 1300, qty 5 000 000)` | `op == Deliver`(18)、`px == 1000`、`qty == 5 000 000`、`arg == 15`、`arg2 == 1300`,其余字段 0 |

### EVT-143 CtlRecord 帧编码后读回相同并拒绝截断与不允许的 kind

- 规则:[环元素类型](README.md#环元素类型)
- 级别:table
- 里程碑:M1
- 前置:输出缓冲 1 KiB 预填 `0xAA`
- 实现:`crates/hft-event/tests/ring.rs::evt_143_ctl_record`

| 行 | 输入 | 期望 |
|---|---|---|
| EVT-143.1 | `encode_into(CtlRecord{kind: ControlCmd, flags: 0, body: 130 B})` | `encoded_len == 144`,返回 144;字节 0..2 = `0x0401`、2..4 = 0、4..8 = 130;138..144 为 0;`read` 得同一 kind、flags 与体 |
| EVT-143.2 | 体 0 B | `encoded_len == 8` |
| EVT-143.3 | `flags = PART` | `read` 得 `flags == PART` |
| EVT-143.4 | 体 130 B,输出 143 B | `Err(BufTooSmall)` |
| EVT-143.5 | `read` 输入 7 B | `Err(Short)` |
| EVT-143.6 | `read`,头 `len = 200`,输入 144 B | `Err(Short)` |
| EVT-143.7 | `read`,头 `kind = 0x0101`(Bbo) | `Err(KindMismatch)` |
| EVT-143.8 | `read`,头 `kind` = `0x0403`、`0x0404`、`0x0506` | 各为 `Ok` |

### EVT-144 环拓扑登记与 hft-sequencer 环表一致

- 规则:[环元素类型](README.md#环元素类型)
- 级别:table
- 里程碑:M1
- 前置:`cargo xtask doc-check` 读取 hft-event"环拓扑"段与 [hft-sequencer](../hft-sequencer/README.md) 环表;optional 的 `chain-in`/`chain-out` 不参与比对;每行断言该环在两处都出现且元素类型如列
- 实现:`tools/xtask/tests/doc_check.rs::evt_144_ring_registry_consistent`

| 行 | 环 | 期望元素类型 |
|---|---|---|
| EVT-144.1 | `ctl` | `CtlRecord` 帧(`ByteSpsc`) |
| EVT-144.2 | `hk` | `HkMsg` |
| EVT-144.3 | `rep-<venue>` | `RepMsg`;配套 `rep-<venue>-bulk` 字节环 |
| EVT-144.4 | `rep-sim` | `RepMsg` |
| EVT-144.5 | `bbo-<venue>` | `FeedMsg`(只有 `Bbo` 与 `ConnState`) |
| EVT-144.6 | `md-<venue>` | `FeedMsg`;配套 `md-<venue>-bulk` 字节环 |
| EVT-144.7 | `advice` | `AdviceMsg` 帧(`ByteSpsc`;`Advice` 与 `source = Advisor` 的 `Note`) |
| EVT-144.8 | `out-<venue>` | `GwCmd`(`OutMsg`) |
| EVT-144.9 | `out-sim` | `GwCmd` |
| EVT-144.10 | `feedctl-<venue>`、`feedctl-<venue>-bbo` | `FeedCmd` |
| EVT-144.11 | 在测试副本的 hft-event 环拓扑中加入 `foo-<venue>` | doc-check 退出 1,输出含 `foo-<venue>` |
| EVT-144.12 | 在测试副本的 hft-sequencer 环表中删除 `hk` 行 | doc-check 退出 1,输出含 `hk` |

### EVT-150 命令信封结构的布局

- 规则:[控制命令编码](README.md#控制命令编码)
- 级别:table
- 里程碑:M1
- 前置:无参数
- 实现:`crates/hft-event/tests/layout.rs::evt_150_cmd_envelope_layouts`

| 行 | 结构 | 期望 |
|---|---|---|
| EVT-150.1 | `CmdHdr` | size 128、align 8;`cmd_id`@0、`issuer`@16、`issued_at_ns`@40、`expires_at_ns`@48、`expires_tsc`@56、`peer_uid`@64、`cmd`@68、`cmd_ver`@70、`role`@72、`account`@73、`confirmed`@74、`hflags`@75、`payload_len`@76、`cmd_hash`@80、`base_version`@112、`expect_state_version`@120 |
| EVT-150.2 | `BlobRef` | size 48、align 8;`kind`@0、`encoding`@2、`len`@4、`version`@8、`sha256`@16 |
| EVT-150.3 | 常量 | `STRATEGY_CMD_MAX_PAYLOAD == 256`;`WARMUP_MAX_PAYLOAD == 4096` |

### EVT-151 CmdKind 编号表与保留、退役编号

- 规则:[控制命令编码](README.md#控制命令编码)
- 级别:table
- 里程碑:M1
- 前置:对每个已分配编号断言 `CmdKind` 的名字与 `ControlCmd::kind()`;对保留与退役编号构造 `CmdHdr.cmd` = 该值、`payload_len = 0` 的逻辑体调用 `decode`
- 实现:`crates/hft-event/tests/cmd.rs::evt_151_cmd_kind_numbers`

| 行 | 输入 | 期望 |
|---|---|---|
| EVT-151.1 | `0x0001`–`0x0004` | `SetMode`、`SetParams`、`SetRisk`、`Kill` |
| EVT-151.2 | `0x0006`–`0x000A` | `RotateConn`、`Resubscribe`、`SnapshotNow`、`ReplaceRefdata`、`Resume` |
| EVT-151.3 | `0x000E`–`0x0013` | `HedgeFix`、`CancelAll`、`Drain`、`RecordTransfer`、`Annotate`、`AdoptExchange` |
| EVT-151.4 | `0x0014`–`0x001B` | `SimReset`、`SimTransfer`、`VenueNotice`、`SetBalanceBaseline`、`StrategyCmd`、`InjectWarmup`、`SmokeOrder`、`WriteOffResidual` |
| EVT-151.5 | `0x0005`(保留) | `decode` 为 `Err(UnknownKind)` |
| EVT-151.6 | `0x000B`、`0x000C`、`0x000D`(退役) | 各为 `Err(UnknownKind)` |
| EVT-151.7 | `0x0000` | `Err(UnknownKind)` |
| EVT-151.8 | `0x001F`(未分配) | `Err(UnknownKind)` |
| EVT-151.9 | `0x001C` | `SimFault` |
| EVT-151.10 | `0x001D` | `BindSlot` |
| EVT-151.11 | `0x001E` | `Settle` |

### EVT-152 每个命令载荷的大小与关键偏移

- 规则:[控制命令编码](README.md#控制命令编码)
- 级别:table
- 里程碑:M1
- 前置:当前 `cmd_ver`(`Kill` 2、`AdoptExchange` 2,其余 1);每行编码一个字段取非零样本的命令,断言 `CmdHdr.payload_len` 与逻辑体中载荷各字段的偏移(相对载荷起点)
- 实现:`crates/hft-event/tests/cmd.rs::evt_152_payload_sizes`

| 行 | 命令 | 期望(载荷字节数;偏移) |
|---|---|---|
| EVT-152.1 | `SetMode` | 24;`scope`@0、`mode`@16 |
| EVT-152.2 | `SetParams` | 64;`scope`@0、`blob`@16 |
| EVT-152.3 | `SetRisk`、`ReplaceRefdata` | 48;`blob`@0 |
| EVT-152.4 | `Kill` v2 | 24;`scope`@0、`level`@16、`flatten`@17、`cancel_resting`@18、`hard`@19 |
| EVT-152.5 | `RotateConn` | 8;`venue`@0、`account`@1 |
| EVT-152.6 | `Resubscribe` | 8;`venue`@0、`inst`@4 |
| EVT-152.7 | `SnapshotNow` | 0 |
| EVT-152.8 | `Resume` | 24;`scope`@0、`source`@16 |
| EVT-152.9 | `HedgeFix`、`CancelAll` | 16;`scope`@0 |
| EVT-152.10 | `Drain` | 8;`cancel_resting`@0 |
| EVT-152.11 | `RecordTransfer` | 80;`account`@0、`from`@1、`to`@2、`asset`@4、`amount`@8、`reference`@16 |
| EVT-152.12 | `Annotate` | 296;`kind`@0、`text_len`@2、`digest`@4、`text`@36 |
| EVT-152.13 | `AdoptExchange` v2 | 32;`account`@0、`venue`@1、`inst`@4、`report_id`@8、`item`@16、`asset`@18、`group`@20、`entry_px`@24 |
| EVT-152.14 | `SimReset` | 8;`account`@0 |
| EVT-152.15 | `SimTransfer` | 16;`account`@0、`from`@1、`to`@2、`asset`@4、`amount`@8 |
| EVT-152.16 | `VenueNotice` | 24;`venue`@0、`kind`@1、`inst`@4、`a`@8、`b`@16 |
| EVT-152.17 | `SetBalanceBaseline` | 8;`account`@0、`venue`@1 |
| EVT-152.18 | `StrategyCmd` | 264;`instance`@0、`reduce_only`@1、`len`@2、`group`@4、`payload`@8 |
| EVT-152.19 | `InjectWarmup`,`len = 10` | 32(16 + 16);`instance`@0、`last`@1、`local_kind`@2、`chunk_no`@4、`len`@8、`payload`@16;偏移 26..32 为 0 |
| EVT-152.20 | `SmokeOrder` | 16;`account`@0、`venue`@1、`inst`@4、`side`@8、`ttl_ms`@12 |
| EVT-152.21 | `WriteOffResidual` | 8;`account`@0、`group`@4 |
| EVT-152.22 | `SimFault` | 8;`account`@0、`venue`@1、`fault`@2、`delay_ms`@4 |

### EVT-153 任意合法命令编码后解码得到同一命令

- 规则:[控制命令编码](README.md#控制命令编码)
- 级别:prop
- 里程碑:M1
- 性质:对任意合法 `ControlCmd` c 与任意 `CmdHdr` h:`n = c.encode(&h, out)` 后 `ControlCmd::decode(&out[..n])` 返回 `(h', c)`,`h'` 除 `cmd`、`cmd_ver`、`payload_len` 外与 h 逐字段相等;`n == 128 + payload_len`
- 生成:均匀选 24 个变体;枚举字段只取合法值,`CapSource` 1..=15,bool 取 0/1;`StrategyCmd.len` 0..=256(`len` 之后的载荷为 0);`InjectWarmup.len` 0..=4096;`CmdScope.group` 取 `GROUP_ALL` 或合法 `GroupId`;h 的其余字段全域
- 参考:输入本身
- 例数:PR 2 000
- 实现:`crates/hft-event/tests/prop_cmd.rs::evt_153_cmd_roundtrip`

### EVT-154 ControlCmd::decode 对截断、未知命令、长度不符与越界字段返回错误

- 规则:[控制命令编码](README.md#控制命令编码)
- 级别:table
- 里程碑:M1
- 前置:每行从一条合法命令的逻辑体出发,只改所列内容;"逻辑体长"= 128 + 实际载荷字节数
- 实现:`crates/hft-event/tests/cmd.rs::evt_154_decode_errors`

| 行 | 输入 | 期望 |
|---|---|---|
| EVT-154.1 | 逻辑体 127 B | `Err(Short)` |
| EVT-154.2 | `SetMode`,`cmd_ver = 2` | `Err(UnknownKind)` |
| EVT-154.3 | `Kill`,`cmd_ver = 3` | `Err(UnknownKind)` |
| EVT-154.4 | `SetMode`,`payload_len = 24`,逻辑体 160 B | `Err(TooLong)` |
| EVT-154.5 | `SetMode`,`payload_len = 16`,逻辑体 144 B | `Err(Short)` |
| EVT-154.6 | `SetMode`,`payload_len = 32`,逻辑体 160 B | `Err(TooLong)` |
| EVT-154.7 | `SetMode.mode` = 2(退役)/ 0 / 5 | 各为 `Err(InvalidEnum)` |
| EVT-154.8 | `Kill.level` = 1(退役)/ 5 | 各为 `Err(InvalidEnum)` |
| EVT-154.9 | `Kill.flatten` 字节 = 2 | `Err(InvalidEnum)` |
| EVT-154.10 | `Resume.source` = 0 / 16 | 各为 `Err(InvalidEnum)` |
| EVT-154.11 | `Resume.source` = 9(不可由命令清除) | `Ok`(可否清除由 sequencer 判定) |
| EVT-154.12 | `StrategyCmd.len = 257` | `Err(TooLong)` |
| EVT-154.13 | `StrategyCmd.len = 3`,`payload[3] = 1` | `Err(TooLong)` |
| EVT-154.14 | `StrategyCmd.len = 3`,`payload[3..]` 全 0 | `Ok` |
| EVT-154.15 | `StrategyCmd.reduce_only = 2` | `Err(InvalidEnum)` |
| EVT-154.16 | `InjectWarmup.len = 4 097`,`payload_len = 4 120` | `Err(TooLong)` |
| EVT-154.17 | `InjectWarmup.len = 10`,`payload_len = 32` | `Ok` |
| EVT-154.18 | `InjectWarmup.len = 10`,`payload_len = 24` | `Err(Short)` |
| EVT-154.19 | `InjectWarmup.len = 10`,`payload_len = 40` | `Err(TooLong)` |
| EVT-154.20 | `InjectWarmup.last = 2` | `Err(InvalidEnum)` |
| EVT-154.21 | `SmokeOrder.side = 0` | `Err(InvalidEnum)` |
| EVT-154.22 | `Drain.cancel_resting = 2` | `Err(InvalidEnum)` |
| EVT-154.23 | `SetParams.blob.kind = 0` / `7` | 各为 `Err(InvalidEnum)` |
| EVT-154.24 | `SetMode` 载荷偏移 17(隐式填充)= 1;`InjectWarmup` 载荷尾部填充(偏移 16 + len)= 1 | 各为 `Err(InvalidEnum)` |

### EVT-155 旧 cmd_ver 的载荷按旧布局读出、新字段取缺省

- 规则:[控制命令编码](README.md#控制命令编码)
- 级别:table
- 里程碑:M1
- 前置:按旧版本手工构造的逻辑体
- 实现:`crates/hft-event/tests/cmd.rs::evt_155_old_cmd_versions`

| 行 | 输入 | 期望 |
|---|---|---|
| EVT-155.1 | `Kill` v1(24 B,偏移 19 = 0) | `Ok`,`hard == false`,其余字段同样本 |
| EVT-155.2 | `Kill` v1,偏移 19 = 1(v1 的填充字节) | `Ok`,`hard == false` |
| EVT-155.3 | `Kill` v2,偏移 19 = 1 | `Ok`,`hard == true` |
| EVT-155.4 | `AdoptExchange` v1(16 B) | `Ok`,`item == 0`(Position)、`asset == 0`、`group == GROUP_NONE`、`entry_px == 0`;`report_id` 同样本 |
| EVT-155.5 | `AdoptExchange` v2,`item = 1` | `Ok`,`item == 1`(Balance) |
| EVT-155.6 | `AdoptExchange` v1,`payload_len = 32` | `Err(TooLong)` |

### EVT-156 encode 按命令填写 cmd、cmd_ver 与 payload_len

- 规则:[控制命令编码](README.md#控制命令编码)
- 级别:unit
- 里程碑:M1
- 前置:`CmdHdr` 样本 h 的 `cmd = 0x0001`、`cmd_ver = 9`、`payload_len = 999`,其余字段非零
- 步骤:
  1. 用 h 分别编码 `Kill`、`AdoptExchange`、`SetMode`、`InjectWarmup{len: 10}`
- 期望:
  - `Kill`:逻辑体中 `cmd = 0x0004`、`cmd_ver = 2`、`payload_len = 24`
  - `AdoptExchange`:`0x0013`、2、32;`SetMode`:`0x0001`、1、24;`InjectWarmup`:`0x0019`、1、32
  - 四次编码中 h 的其余字段(`cmd_id`、`issuer`、`account`、`cmd_hash`、`expect_state_version` 等)逐字节原样
- 实现:`crates/hft-event/tests/cmd.rs::evt_156_encode_fills_header`

### EVT-157 encode 拒绝缓冲不足与超长载荷

- 规则:[控制命令编码](README.md#控制命令编码)
- 级别:table
- 里程碑:M1
- 前置:无参数
- 实现:`crates/hft-event/tests/cmd.rs::evt_157_encode_errors`

| 行 | 输入 | 期望 |
|---|---|---|
| EVT-157.1 | `SetMode`,输出 151 B(需要 152) | `Err(BufTooSmall)` |
| EVT-157.2 | `SetMode`,输出 152 B | `Ok(152)` |
| EVT-157.3 | `StrategyCmd{len: 257, ..}` | `Err(TooLong)` |
| EVT-157.4 | `InjectWarmup{len: 4 097}`,载荷 4 097 B | `Err(TooLong)` |
| EVT-157.5 | `InjectWarmup{len: 11}`,载荷 10 B | `Err(TooLong)` |

### EVT-158 最大的 InjectWarmup 命令总是单段且整条不超过 4 288 B

- 规则:[控制命令编码](README.md#控制命令编码)、[实现说明](README.md#实现说明)
- 级别:unit
- 里程碑:M1
- 前置:`InjectWarmup{len: 4096}`;`ControlCmd` 的 CRC 策略为 Always(EVT-004.24)
- 步骤:
  1. 编码逻辑体,交给 `PartIter::new`
  2. 计算 `record_len(16 + chunk_len, true)`
- 期望:
  - 逻辑体 4 240 B;`PartIter` 恰产出 1 段,`parts == 1`,无 `PART` 位
  - 整条记录长 4 288 B
- 实现:`crates/hft-event/tests/cmd.rs::evt_158_max_warmup_single_part`

### EVT-160 变长记录前缀的布局

- 规则:[变长记录前缀](README.md#变长记录前缀)
- 级别:table
- 里程碑:M1
- 前置:无参数
- 实现:`crates/hft-event/tests/layout.rs::evt_160_var_prefix_layouts`

| 行 | 结构 | 期望 |
|---|---|---|
| EVT-160.1 | `L2SnapHdr` | size 72、align 8;`h`@0、`snap_id`@56、`origin`@64、`_pad`@65、`n_bids`@68、`n_asks`@70 |
| EVT-160.2 | `Level` | size 16;`px`@0、`qty`@8 |
| EVT-160.3 | `L2BatchHdr` | size 80;`h`@0、`prev_id`@56、`last_id`@64、`n_bids`@72、`n_asks`@74、`bflags`@76、`_pad`@78 |
| EVT-160.4 | `BlobHdr` | size 72;`blob`@0、`scope`@48、`boot`@64、`_pad`@65 |
| EVT-160.5 | `RawHdr` | size 24;`venue`@0、`conn`@1、`what`@2、`rflags`@3、`stream`@4、`hw_rx_ns`@8、`payload_len`@16、`_pad`@20 |

### EVT-161 multi_view 校验前缀判定字段与尾部长度

- 规则:[变长记录前缀](README.md#变长记录前缀)
- 级别:table
- 里程碑:M1
- 前置:每行构造一个 Multi 逻辑体
- 实现:`crates/hft-event/tests/var.rs::evt_161_multi_view`

| 行 | 输入 | 期望 |
|---|---|---|
| EVT-161.1 | `L2Snapshot`,`origin = 3`,`n_bids = 2`、`n_asks = 1`,尾部 48 B | `Ok`,尾部 48 B |
| EVT-161.2 | 同上,`origin = 0` / `origin = 4` | `Err(InvalidEnum)` / `Err(InvalidEnum)` |
| EVT-161.3 | 同上,尾部 32 B / 64 B | `Err(Short)` / `Err(TooLong)` |
| EVT-161.4 | `L2Snapshot` 逻辑体 71 B | `Err(Short)` |
| EVT-161.5 | `ConfigBlob`,`blob.len = 100`,尾部 100 B,`boot = 1` | `Ok` |
| EVT-161.6 | 同上,尾部 99 B | `Err(Short)` |
| EVT-161.7 | 同上,`boot = 2` | `Err(InvalidEnum)` |
| EVT-161.8 | 同上,`blob.kind = 0` / `7` | `Err(InvalidEnum)` / `Err(InvalidEnum)` |
| EVT-161.9 | `RawRx`,`payload_len = 0`,尾部 0 | `Ok` |
| EVT-161.10 | `RawRx`,`payload_len = 10`,尾部 11 B | `Err(TooLong)` |

### EVT-162 PartIter 在单段上限与段数上限处的切分

- 规则:[变长记录前缀](README.md#变长记录前缀)、[journal-format 多段记录](../../docs/subsystems/journal-format.md#变长与多段记录)
- 级别:table
- 里程碑:M1
- 前置:大逻辑体用 `vec![0u8; n]`(按需清零的页,测试不触碰内容);只检查 `PartHdr`、chunk 长度与 flags
- 实现:`crates/hft-event/tests/var.rs::evt_162_part_iter_bounds`

| 行 | 输入逻辑体长 | 期望 |
|---|---|---|
| EVT-162.1 | 0 | 1 段:`{total_len 0, chunk_len 0, part 0, parts 1}`,无 `PART` |
| EVT-162.2 | 65 488 | 1 段,`chunk_len = 65 488`,无 `PART` |
| EVT-162.3 | 65 489 | 2 段:`chunk_len` 65 488 与 1;第 0 段有 `PART`、第 1 段无;两段 `parts = 2`、`total_len = 65 489` |
| EVT-162.4 | 130 976 | 2 段,各 65 488 |
| EVT-162.5 | 65 535 × 65 488 = 4 291 756 080 | `Ok`,65 535 段,末段 `chunk_len = 65 488` |
| EVT-162.6 | 4 291 756 081 | `Err(TooLong)` |
| EVT-162.7 | 每段带 CRC 的记录长 `record_len(16 + 65 488, true)` | 65 536(不超记录上限) |

### EVT-163 任意逻辑体切分后重组逐字节相同

- 规则:[变长记录前缀](README.md#变长记录前缀)
- 级别:prop
- 里程碑:M1
- 性质:逻辑体 L 经 `PartIter` 切分、每段写成一条记录(同 kind、同 `tsc`、`seq` 连续)后依次 `push`:前 n − 1 次返回 `Ok(None)`,末次返回 `Ok(Some(x))` 且 `x == L`
- 生成:L 长 0..=300 KiB,随机字节;kind ∈ {`ConfigBlob`, `StateSnapshot`, `ControlCmd`};`flags` 含或不含 `CRC`;重组缓冲 512 KiB;同一 `Reassembler` 连续处理 1..=5 组
- 参考:输入 L
- 例数:PR 300
- 实现:`crates/hft-event/tests/prop_var.rs::evt_163_reassemble_roundtrip`

### EVT-164 Reassembler 对组内不一致与超限返回错误

- 规则:[变长记录前缀](README.md#变长记录前缀)、[journal-format 多段记录](../../docs/subsystems/journal-format.md#变长与多段记录)
- 级别:table
- 里程碑:M1
- 前置:`ConfigBlob` 逻辑体 150 000 B 切成 3 段(`seq` 100、101、102,同一 `tsc`);重组缓冲 256 KiB;每行只改所列内容后从组首依次 push
- 实现:`crates/hft-event/tests/var.rs::evt_164_reassemble_errors`

| 行 | 输入 | 期望 |
|---|---|---|
| EVT-164.1 | 先 push 第 1 段 | `Err(BadPart)` |
| EVT-164.2 | 第 1 段的 kind 改为 `StateSnapshot` | 第 1 段 push 返回 `Err(BadPart)` |
| EVT-164.3 | 第 1 段的 `tsc` 加 1 | `Err(BadPart)` |
| EVT-164.4 | 第 1 段的 `seq` 改为 102 | `Err(BadPart)` |
| EVT-164.5 | 第 2 段(末段)带 `PART` | `Err(BadPart)` |
| EVT-164.6 | 第 0 段不带 `PART` | `Err(BadPart)` |
| EVT-164.7 | 第 1 段 `parts = 4` | `Err(BadPart)` |
| EVT-164.8 | 第 1 段 `total_len = 150 001` | `Err(BadPart)` |
| EVT-164.9 | 末段 `chunk_len` 少 8 且体同步截短(Σ ≠ `total_len`) | `Err(BadPart)` |
| EVT-164.10 | 第 0 段 `chunk_len` 大于该段体减 16 | `Err(BadPart)` |
| EVT-164.11 | `total_len = 300 000`(超过 256 KiB 缓冲) | 第 0 段 push 返回 `Err(TooLong)` |
| EVT-164.12 | 单段记录(`parts = 1`,无 `PART`) | 第一次 push 即返回 `Ok(Some(..))` |

### EVT-165 出错或 reset 后丢弃未完成的组并能接收下一组

- 规则:[变长记录前缀](README.md#变长记录前缀)
- 级别:unit
- 里程碑:M1
- 前置:重组缓冲 256 KiB;组 A 为 EVT-164 的 3 段组;组 B 为 2 段、80 000 B 的 `ConfigBlob`(`seq` 200、201)
- 步骤:
  1. push A 第 0 段,再 push 带 `PART` 的 A 末段(得 `BadPart`),然后依次 push B 的两段
  2. 新建 Reassembler,push A 第 0 段,`reset()`,再 push 一个单段组 C
- 期望:
  - 步骤 1:B 的第 2 次 push 返回 `Ok(Some(B))` 且与 B 逐字节相同
  - 步骤 2:push C 返回 `Ok(Some(C))`;A 的残留不出现在结果中
- 实现:`crates/hft-event/tests/var.rs::evt_165_reset_and_recover`

### EVT-170 event_body! 的编译期断言拒绝不合规的体

- 规则:[实现说明](README.md#实现说明)、[体 trait 与编解码](README.md#体-trait-与编解码)
- 级别:table
- 里程碑:M1
- 前置:`trybuild` 编译 `tests/compile_fail/` 下的最小 crate,每行一个文件;"编译成功"的行放 `tests/compile_pass/`
- 实现:`crates/hft-event/tests/compile.rs::evt_170_event_body_static_asserts`

| 行 | 输入 | 期望 |
|---|---|---|
| EVT-170.1 | 体字段合计 84 B(不是 8 的倍数) | 编译失败 |
| EVT-170.2 | `u8` 后紧跟 `u64` 而未写 `_pad`(隐式填充) | 编译失败 |
| EVT-170.3 | `KINDS` 登记 `Fixed{size: 96}`,体实际 88 B | 编译失败 |
| EVT-170.4 | 定长体 65 504 B / 65 512 B | 编译成功 / 编译失败(上限 65 536 − 24 − 8) |
| EVT-170.5 | Var 前缀 80 B,`max` = 65 424 / 65 425 | 编译成功 / 编译失败(`prefix + max ≤ 65 504`) |
| EVT-170.6 | 含 `Side` 字段的体只派生 `FromBytes` | 编译失败(枚举体必须用 `TryFromBytes`) |
| EVT-170.7 | 体含 `Vec<u8>` 或引用字段 | 编译失败 |

### EVT-171 每个体的 FieldDesc 与实际布局一致且覆盖全部字节

- 规则:[实现说明](README.md#实现说明)、[记录头、标志与 kind](README.md#记录头标志与-kind)
- 级别:unit
- 里程碑:M1
- 前置:`KINDS` 每项的 `fields`(嵌套 `Nested` 展开)
- 步骤:
  1. 对每个 kind 把字段按偏移排序,与 `offset_of!` 结果比较
- 期望:
  - 每个 `FieldDesc.offset` 等于对应 `offset_of!`;相邻字段首尾相接(`offset + 宽度 == 下一个 offset`),`_pad*`/`_rsv*` 以 `Pad(n)` 出现;末字段结束于 `size`(定长)或 `prefix`
  - `Px`/`Qty`/`Notional`/`Rate` 字段的 `ty` 分别为 `FieldTy::Px`/`Qty`/`Notional`/`Rate`(不是 `I64`)
- 实现:`crates/hft-event/tests/kinds.rs::evt_171_field_desc_matches_layout`

### EVT-172 gen-schema 产出全部生成物且 Python dtype 与 Rust 布局一致

- 规则:[实现说明](README.md#实现说明)
- 级别:unit
- 里程碑:M1
- 前置:测试直接调用 xtask 的生成函数(`gen_schema::run_model`),输出到临时目录 `<tmp>`;模型由链接的 hft-event/hft-refdata 导出构建,策略 schema 部分以一个替身策略代替,`Oms`、`Ledger` 源的 const 表以替身注入(登记表中该源的全部名字);系统 Python 3 带 numpy
- 步骤:
  1. 生成
  2. 用 Python 导入 `<tmp>/python/_generated/events.py`,对每个 kind 读出 dtype
- 期望:
  - `docs/generated/event-catalog.md` 含 41 个 kind 小节,每节列出编号、域、角色、布局、`schema_version` 与字段表;Note 码表含 `NOTE_NAMES` 的全部 95 个名字,包含 Ledger BalanceCheck、ReceiptMismatch、DayPnlCarried 与 Sequencer ReconcileCash、ReconcileRejected
  - `schemas/arrow/` 下 41 个 `<kind>.json`;`Px`/`Qty` 列为 `Int64` 且带元数据 `hft.type = px`/`qty`
  - 每个 kind 的 dtype `itemsize` 等于 `size`(Var/Multi 等于 `prefix`),`names` 与 `offsets` 与 `FieldDesc` 逐项相同;`StrategyParamsHdr`、`GroupSpec`、`LegDef` 的 dtype itemsize 为 48、80、16
  - `refdata.py` 含 `LoadErrCode` 每个变体一项,数值与 hft-refdata 相同
  - `event-layout.lock` 含 41 个 kind、全部已分配的 `CmdKind`(25 个,含 `BindSlot`)与 `LoadErrCode`
  - `crates/hft-telemetry/src/notes_catalog.rs` 对每个 source crate `const` 表中的每个 `(source, n)` 恰有一项,名字与默认级别同 `NOTE_NAMES`
- 实现:`tools/xtask/tests/gen_schema.rs::evt_172_outputs_and_python_dtype`

### EVT-173 每个 kind 的规范样例编码与金样逐字节相同

- 规则:[实现说明](README.md#实现说明)、[体 trait 与编解码](README.md#体-trait-与编解码)
- 级别:golden
- 里程碑:M1
- 输入:`crates/hft-event/tests/common/canonical.rs` 中每个 kind 一条规范样例(各字段取非零且按字段序递增的值,枚举取最大合法值;`seq = 1`、`tsc = 2`、`flags = EVENT_END`,`CRC` 按该 kind 的策略),Multi kind 为单段记录
- 期望文件:`fixtures/hft-event/EVT-173/<kind>.bin`(41 个)
- 实现:`crates/hft-event/tests/golden.rs::evt_173_canonical_records`

### EVT-174 gen-schema --check 在生成物与仓库一致时通过、被手改时失败

- 规则:[实现说明](README.md#实现说明)
- 级别:table
- 里程碑:M1
- 前置:临时 git 仓库,内容为当前构建的 xtask(链接 `crates/hft-event`、`crates/hft-refdata`)生成的全部生成物并已提交;每行在干净副本上做所列改动后运行 `cargo xtask --root <副本> gen-schema --check`
- 实现:`tools/xtask/tests/gen_schema.rs::evt_174_check_detects_drift`

| 行 | 输入 | 期望 |
|---|---|---|
| EVT-174.1 | 不改动 | 退出 0,输出无差异 |
| EVT-174.2 | 手改 `python/_generated/events.py` 中 `Bbo` 的一个 offset | 退出 1,输出含 `python/_generated/events.py` |
| EVT-174.3 | 删除 `schemas/arrow/bbo.json` | 退出 1,输出含该路径 |
| EVT-174.4 | 手改 `docs/generated/event-catalog.md` 一行 | 退出 1 |
| EVT-174.5 | 副本中有 `apps/hft-core/Cargo.toml`,已构建的 `hft-core` 二进制不存在(无 `HFT_CORE_BIN`) | 退出 2 |

### EVT-175 只追加核对拒绝改布局而放行合规的追加与占用保留字节

- 规则:[实现说明](README.md#实现说明)、[event-model 演进规则](../../docs/subsystems/event-model.md#演进规则)
- 级别:table
- 里程碑:M1
- 前置:基线 `schemas/event-layout.lock` 为当前布局;每行在干净副本上做所列源码改动后以写入模式运行 gen-schema——改动施加在 xtask 由链接的 hft-event 导出构建的模型(`gen_schema::model::Model`)上,与改源码后重新构建 xtask 等价,测试不重新编译 hft-event
- 实现:`tools/xtask/tests/gen_schema.rs::evt_175_append_only_rules`

| 行 | 输入 | 期望 |
|---|---|---|
| EVT-175.1 | `ExternalFill` 体尾追加 `foo: u64`(大小 104),`schema_version` 3 → 4 | 退出 0;lock 中 `ExternalFill` 为 v4、104 B,原 17 个字段的名、偏移、类型不变 |
| EVT-175.2 | 同上但 `schema_version` 不变 | 退出 1,输出含 `ExternalFill` 与 `schema_version` |
| EVT-175.3 | 交换 `OrderDecision.px` 与 `qty` 的顺序(大小不变) | 退出 1(偏移改变) |
| EVT-175.4 | 把 `OrderDecision.reason` 改名为 `reason_code` | 退出 1(改名) |
| EVT-175.5 | 把 `ExternalFill.trade_id` 由 `u64` 改为 `i64` | 退出 1(改型) |
| EVT-175.6 | 删除 `Trade.trade_id`(`schema_version` 加 1) | 退出 1(删字段) |
| EVT-175.7 | 在 `Bbo.update_id` 之前插入 `foo: u64`(`schema_version` 加 1) | 退出 1(中部插入) |
| EVT-175.8 | 把 `LatencySample._pad2` 的第 1 字节改为具名字段 `foo: u8`,大小不变、版本不变 | 退出 0(占用保留字节) |
| EVT-175.9 | 同上但新字段类型为 `Side`(0 非法的枚举) | 退出 1 |
| EVT-175.10 | `ExternalFill` 体尾追加 `foo_side: Side`(加 `_pad`,版本加 1) | 退出 1(新字段是 0 非法的枚举) |
| EVT-175.11 | `MarketHdr` 尾部追加 `foo: u64`(56 → 64),各行情 kind 版本加 1 | 退出 1(各行情体字段偏移改变) |
| EVT-175.12 | `MarketHdr._rsv` 改为具名字段 `foo: u32` | 退出 0 |
| EVT-175.13 | 基线中没有 `event-layout.lock` | 退出 0,生成 lock(首次生成不核对) |
| EVT-175.14 | 把 `0x0102 L2Delta` 标为 `#[deprecated]`(仍在 `KINDS`) | 退出 0;lock 中该行带 `deprecated` |

### EVT-176 kind、CmdKind 与 LoadErrCode 的编号消失或被复用时失败(含引擎 Note 码与策略的 kind_id、Note 码、op、检查点节 tag)

- 规则:[实现说明](README.md#实现说明)、[控制命令编码](README.md#控制命令编码)
- 级别:table
- 里程碑:M1
- 前置:同 EVT-175
- 实现:`tools/xtask/tests/gen_schema.rs::evt_176_numbers_never_vanish_or_reuse`

| 行 | 输入 | 期望 |
|---|---|---|
| EVT-176.1 | 从 `KINDS` 删除 `0x0108 Liquidation` | 退出 1,输出含 `0x0108` |
| EVT-176.2 | `0x0108` 改登记为新 kind `Foo` | 退出 1(复用) |
| EVT-176.3 | 删除 `CmdKind 0x0016 VenueNotice` | 退出 1 |
| EVT-176.4 | 把退役的 `0x000B` 分配给新命令 | 退出 1(复用) |
| EVT-176.5 | 把 `0x0017` 改名为 `SetBaseline` | 退出 1(同一编号换名字) |
| EVT-176.6 | 删除 `LoadErrCode::DanglingRef`(6) | 退出 1 |
| EVT-176.7 | `LoadErrCode` 6 改为新名字 | 退出 1 |
| EVT-176.8 | 追加新 kind `0x010A`(v1) | 退出 0;lock 多出该 kind 的块 |
| EVT-176.9 | 追加 `CmdKind 0x001D` | 退出 0 |
| EVT-176.10 | 在 `LoadErrCode` 末尾追加一个新编号 | 退出 0 |
| EVT-176.11 | 引擎 Note 码 `(Oms, n)` 中 `OmsAnomaly` 的 `n` 改给新码 `OmsFoo`,`OmsAnomaly` 换新 `n` | 退出 1(复用) |
| EVT-176.12 | 测试策略的 `kind_id` 0xFF01 从 `hft-core schemas` 导出中消失 | 退出 1,输出含 `0xFF01` |
| EVT-176.13 | 测试策略 `NoteDesc.code = 3` 改名 | 退出 1 |
| EVT-176.14 | 测试策略删除 `CmdDesc{op = 2}` | 退出 1 |
| EVT-176.15 | 测试策略检查点 `SectionDesc{tag = 1}` 改名 | 退出 1 |
| EVT-176.16 | 测试策略追加 `NoteDesc.code = 9`、`CmdDesc.op = 4` 与检查点 `tag = 3` | 退出 0 |
| EVT-176.17 | 另一策略种类使用与测试策略相同的 Note 码、`op` 与 `tag` | 退出 0(按 `kind_id` 分别核对) |

### EVT-177 Note 码表与各 source crate 的 const 表不一致时失败

- 规则:[实现说明](README.md#实现说明)、[Note 来源与码登记表](README.md#note-来源与码登记表)
- 级别:table
- 里程碑:M1
- 前置:gen-schema 从各 source crate 的 `NOTE_CODES` 表读取 `(source, n, name)`(测试以替身 const 表注入:`Oms`、`Ledger`、`Exec` 源各列出登记表中该源的全部名字,`n` 自 1 递增);基线一致
- 实现:`tools/xtask/tests/gen_schema.rs::evt_177_note_const_tables`

| 行 | 输入 | 期望 |
|---|---|---|
| EVT-177.1 | 不改动 | 退出 0 |
| EVT-177.2 | hft-oms 的 const 表删去 `OmsAnomaly` | 退出 1,输出含 `OmsAnomaly` |
| EVT-177.3 | hft-oms 的 const 表多出 `OmsNewThing`(不在 `NOTE_NAMES`) | 退出 1 |
| EVT-177.4 | hft-ledger 的 const 表把 `ExternalFill` 登记在 `Risk` 源 | 退出 1(source 不同) |
| EVT-177.5 | hft-exec 的 const 表两项 `n` 相同 | 退出 1 |
| EVT-177.6 | hft-venue 的 const 表有 `ReconnectCount`(Venue 源,无前缀) | 退出 1 |
| EVT-177.7 | hft-venue 的 const 表有 `VenueReconnects` | 退出 0(不要求在 `NOTE_NAMES`) |
| EVT-177.8 | apps/hft-core 的 const 表有 `HkDiskSample`(Housekeeping 源) | 退出 0 |
| EVT-177.9 | 某策略 `StrategyDesc.notes` 的码与 `OmsAnomaly` 的 `n` 相同 | 退出 0(策略码不参与) |
| EVT-177.10 | hft-ledger 的 const 表有一项 `n = 0` | 退出 1(n = 0 保留) |
| EVT-177.11 | 测试策略 `NoteDesc.code = 0` | 退出 1 |

### EVT-178 hft-ext-proto 的 kind 与 KINDS 编号重叠或越段时失败

- 规则:[实现说明](README.md#实现说明)、[链上(0x03xx)](README.md#链上0x03xx)
- 级别:table
- 里程碑:M1
- 前置:以替身 `KINDS_EXT` 注入
- 实现:`tools/xtask/tests/gen_schema.rs::evt_178_ext_proto_merge`

| 行 | 输入 | 期望 |
|---|---|---|
| EVT-178.1 | `KINDS_EXT` 只含 `0x0310..=0x033F` 内互不相同的编号 | 退出 0;目录中这些 kind 标为 hft-ext-proto |
| EVT-178.2 | `KINDS_EXT` 含 `0x0301` | 退出 1(与 `ChainHead` 重叠) |
| EVT-178.3 | `KINDS_EXT` 含 `0x0340` | 退出 1(越段) |
| EVT-178.4 | `KINDS` 新增 `0x0311` | 退出 1 |
| EVT-178.5 | 未启用 hft-ext-proto | 退出 0 |

### EVT-179 gen-schema 核对失败时不改写任何生成物

- 规则:[实现说明](README.md#实现说明)
- 级别:unit
- 里程碑:M1
- 前置:EVT-175.2 的改动(追加字段未升版本);记录全部生成物的 SHA-256
- 步骤:
  1. 运行 `cargo xtask gen-schema`(写入模式)
- 期望:
  - 退出 1;全部生成物(含 `schemas/event-layout.lock`)的 SHA-256 与运行前相同;没有新增文件
- 实现:`tools/xtask/tests/gen_schema.rs::evt_179_no_write_on_violation`

### EVT-180 CI 以目标分支合并基点上的 lock 为基线,同一提交改基线无效

- 规则:[实现说明](README.md#实现说明)
- 级别:proc
- 里程碑:M1
- 前置:临时 git 仓库,`main` 上有当前代码的生成物与 lock;分支 `pr` 的一个提交同时删除 `0x0108 Liquidation`(模型替身,同 EVT-175)并重新生成全部生成物(lock 中不再有该块);步骤 1 的基线按 GitHub PR 事件选取(`GITHUB_EVENT_NAME=pull_request`、`GITHUB_BASE_REF=main`,测试直接传入同一选择)
- 步骤:
  1. 在 `pr` 上运行 `cargo xtask gen-schema --check`
  2. 不设上述环境变量再运行一次
- 期望:
  - 步骤 1 退出 1,输出含 `0x0108`
  - 步骤 2(本地,基线为工作区已提交的 lock)退出 0
- 实现:`tools/xtask/tests/gen_schema.rs::evt_180_ci_baseline_merge_base`

### EVT-190 encode_fixed 基准

- 规则:[延迟与资源契约](README.md#延迟与资源契约)
- 级别:bench
- 里程碑:M1
- 基准名:`event_encode_80b`
- 前置:80 B 体(`Note`),写入已预触摸的缓冲;数值上限在 README 与 [benchmarks](../../benchmarks/README.md) 登记
- 实现:`crates/hft-event/benches/codec.rs::evt_190_encode_fixed_80b`

### EVT-191 encode_var 基准(Advice 1 KiB)

- 规则:[延迟与资源契约](README.md#延迟与资源契约)
- 级别:bench
- 里程碑:M1
- 基准名:`event_encode_advice_1k`
- 实现:`crates/hft-event/benches/codec.rs::evt_191_encode_advice_1k`

### EVT-192 decode_record + view 基准(无 CRC)

- 规则:[延迟与资源契约](README.md#延迟与资源契约)
- 级别:bench
- 里程碑:M1
- 基准名:`event_decode_view`
- 实现:`crates/hft-event/benches/codec.rs::evt_192_decode_view`

### EVT-193 CRC32C 64 KiB 基准

- 规则:[延迟与资源契约](README.md#延迟与资源契约)
- 级别:bench
- 里程碑:M1
- 基准名:`event_crc_64k`
- 实现:`crates/hft-event/benches/codec.rs::evt_193_crc_64k`

### EVT-194 ControlCmd::decode 基准

- 规则:[延迟与资源契约](README.md#延迟与资源契约)
- 级别:bench
- 里程碑:M1
- 基准名:`event_cmd_decode`
- 实现:`crates/hft-event/benches/cmd.rs::evt_194_cmd_decode`

### EVT-195 parse_spb1(500 group)基准

- 规则:[延迟与资源契约](README.md#延迟与资源契约)
- 级别:bench
- 里程碑:M1
- 基准名:`event_spb1_parse_500`
- 实现:`crates/hft-event/benches/spb1.rs::evt_195_spb1_parse_500`

### EVT-196 编解码入口零分配、零系统调用

- 规则:[延迟与资源契约](README.md#延迟与资源契约)、[所有权与线程](README.md#所有权与线程)
- 级别:unit
- 里程碑:M1
- 前置:计数型全局分配器,启用开关和分配/释放次数为无分配初始化的线程局部量;热身后清零计数;调用方提供全部缓冲(含 `Reassembler` 的 1 MiB 缓冲)。先验证同线程一次真实分配及释放恰计 1/1,再以原子握手让另一线程的真实分配与本线程测量区间重叠,本线程计数须为 0/0
- 步骤:
  1. 各调用 1 000 次:`record_len`、`encode_fixed`、`encode_var`、`decode_record`、`read`、`view`、`var`、`classify`、`kind_info`、`TimerCode::encode/decode`、`CtlRecord::encode_into/read`、`AdviceRecord::encode_into/read`、`ControlCmd::encode/decode`、`parse_spb1`、`multi_view`、`PartIter`、`Reassembler::push`
  2. 在 Linux 上以 `seccomp` 过滤器(只允许 `exit_group`)包住步骤 1 的子进程再跑一次
- 期望:
  - 步骤 1 分配次数与释放次数都为 0
  - 步骤 2 子进程以退出码 0 结束(未因被拦截的系统调用收到 `SIGSYS`)
- 实现:`crates/hft-event/tests/alloc.rs::evt_196_zero_alloc_zero_syscall`

### EVT-200 fuzz:decode_record 与按 kind 读取

- 规则:[体 trait 与编解码](README.md#体-trait-与编解码)
- 级别:fuzz
- 里程碑:M1
- 目标:`fuzz/decode_record`:任意字节 → `decode_record`;成功时对 `kind_info` 已登记的 kind 调 `classify`(逻辑体取 `body`)与对应的 `read`/`view`/`var`/`multi_view`
- 语料:EVT-173 的全部金样记录 + EVT-071 的旧版本记录 + 变异
- 判定:零 panic、零越界、零单输入超时(口径见 [testing §3.3](../../docs/testing.md#33-fuzz));不计入门槛 ③
- 实现:`crates/hft-event/fuzz/fuzz_targets/decode_record.rs::evt_200_fuzz_decode_record`

### EVT-201 fuzz:ControlCmd::decode

- 规则:[控制命令编码](README.md#控制命令编码)
- 级别:fuzz
- 里程碑:M1
- 目标:`fuzz/decode_cmd`:任意字节作为逻辑体 → `ControlCmd::decode`;成功时再 `encode` 并断言与输入的前 `128 + payload_len` 字节相同(`cmd_ver` 为当前版本时)
- 语料:EVT-153 生成的每个变体各一条 + EVT-155 的旧版本载荷
- 判定:零 panic、零单输入超时(口径见 [testing §3.3](../../docs/testing.md#33-fuzz));往返断言失败按崩溃处理
- 实现:`crates/hft-event/fuzz/fuzz_targets/decode_cmd.rs::evt_201_fuzz_decode_cmd`

### EVT-202 fuzz:多段重组

- 规则:[变长记录前缀](README.md#变长记录前缀)
- 级别:fuzz
- 里程碑:M1
- 目标:`fuzz/reassemble`:任意字节切成记录序列,依次 `decode_record` 后交给 `Reassembler::push`(缓冲 1 MiB);完成的逻辑体再交 `multi_view` 与 `ControlCmd::decode`
- 语料:EVT-163 生成的切分记录序列
- 判定:零 panic、零越界写(ASan)、零单输入超时(口径见 [testing §3.3](../../docs/testing.md#33-fuzz))
- 实现:`crates/hft-event/fuzz/fuzz_targets/reassemble.rs::evt_202_fuzz_reassemble`

### EVT-203 fuzz:parse_spb1

- 规则:[StrategyParams(SPB1)](README.md#strategyparamsspb1)
- 级别:fuzz
- 里程碑:M1
- 目标:`fuzz/spb1`:任意字节 → `parse_spb1`;成功时断言 `48 + 80 × n_groups + body_len == len`
- 语料:EVT-131、EVT-132 的 blob + 变异
- 判定:零 panic、零单输入超时(口径见 [testing §3.3](../../docs/testing.md#33-fuzz))
- 实现:`crates/hft-event/fuzz/fuzz_targets/spb1.rs::evt_203_fuzz_spb1`

### EVT-204 原始结算批次的布局与完整性

- 规则:[实现规格](README.md#事件体布局)
- 级别:unit
- 里程碑:M2
- 前置:两项同账户报告和一笔原始成交
- 步骤:读取合法体,逐字节截断,并分别破坏计数、保留位、索引、账户和枚举;执行 CRC 编解码与控制环帧往返
- 期望:合法体按原顺序读取,CRC 编解码一致;坏体明确失败
- 实现:`crates/hft-event/tests/reconcile.rs::evt_204_reconciliation_batch_is_bounded_and_complete`

### EVT-205 订单单笔成交 ID 完整保存并受 CRC 保护

- 规则:[执行](README.md#执行0x02xx)
- 级别:unit
- 里程碑:M2
- 前置:所有原字段非零的 Fill;成交 ID 分别为 0、1、2^63、u64 最大值
- 步骤:分别编码带 CRC 与不带 CRC 的记录,解码完整体;带 CRC 时破坏 ID 的一位
- 期望:ID 逐位保留,前 112 B 与原字段相同,分类仍为 Input;ID 被破坏时报 BadCrc,不能把它当作不参与验证的测量字段
- 实现:`crates/hft-event/tests/version.rs::evt_205_order_execution_id_survives_record_and_crc`

## 验收

- **M1 完成条件**:本文件里程碑为 M1 的用例全部有同编号的实现且通过(`cargo xtask test-trace`,[docs/tests §6](../../docs/tests/README.md#6-与代码的对应));`gen-schema --check` 在 CI 通过。里程碑为 M2 的 EVT-080–EVT-084 随 hft-sequencer 的 advisor 一起完成。
- **属性测试**:PR 例数见各用例;CI 每夜以 `PROPTEST_CASES` = PR 例数 × 50 运行 EVT-011、EVT-025、EVT-111、EVT-133、EVT-153、EVT-163,失败的种子进 `proptest-regressions/` 并入库。
- **fuzz**:EVT-200–EVT-203 进 `tools/xtask/fuzz-targets.toml` 的每夜轮转;任何失败按 [testing §3.3](../../docs/testing.md#33-fuzz) 最小化后进回归语料并修复;这四个目标不计入上线门槛 ③。
- **基准**:EVT-190–EVT-195 在 CI 只做冒烟;数值上限以本组件 README [延迟与资源契约](README.md#延迟与资源契约) 为准(`benches/budgets.toml` 不得更宽),判定在参考机上按 [benchmarks](../../benchmarks/README.md) 执行。
- **金样**:`fixtures/hft-event/` 的文件只由 EVT-173、EVT-071 的生成器重写;重写须与布局变更同一提交且 `gen-schema` 只追加核对通过。

OrderFlags bit 6 = POSITION_CLOSE,不改变布局;生成 schema 与 OrderDecision/GwCmd 保留该位,端到端由 SEQ-185 覆盖。

EVT-205 补充覆盖逐笔现金的尾部编码、CRC 与旧 120 B 前缀补零;连续前缀转换拒绝缺口、混合累计/逐笔事实及金额溢出。
