---
component: hft-refdata
prefix: REF
spec: README.md
---

# hft-refdata 测试规格

格式与规则见 [docs/tests](../../docs/tests/README.md)。

## 覆盖

| README 小节 | 用例 |
|---|---|
| 概要 | 不适用:非规范 |
| 目录 | 不适用:非规范 |
| 使用本组件 | 不适用:见子节 |
| 生命周期(变更 = 重启) | REF-015, REF-039–REF-044, REF-062, REF-071, REF-079 |
| 公开 API | REF-003, REF-011, REF-032, REF-045–REF-061, REF-064, REF-065, REF-067, REF-068, REF-072, REF-080 |
| LoadErrCode 码表 | REF-005, REF-008, REF-009, REF-032, REF-037, REF-039, REF-042, REF-062, REF-065, REF-076, REF-077 |
| 数据结构 | REF-009, REF-016, REF-019, REF-026, REF-027, REF-031, REF-033, REF-034, REF-037, REF-038, REF-050–REF-053, REF-056, REF-060, REF-074, REF-075, REF-078, REF-090–REF-092 |
| 交易所能力表(VenueCaps) | REF-009, REF-020–REF-024, REF-051, REF-062, REF-075, REF-077, REF-089, REF-093、REF-094 |
| 配置项 | REF-012, REF-063 |
| 实现说明 | REF-001–REF-048, REF-059–REF-071, REF-074, REF-077–REF-079, REF-090–REF-093 |
| 延迟与资源契约 | REF-071–REF-073, REF-081–REF-087, REF-089 |
| 测试与验收 | 不适用:非规范 |
| 依赖 | REF-088 |
| 延伸阅读 | 不适用:非规范 |
| 已知限制与后续工作 | REF-027, REF-028 |
| 开发备忘 | 不适用:非规范 |

## 用例

**基准 fixture `F0`**(`crates/hft-refdata/tests/common/f0.rs::f0()`,只经 `RdbBuilder` 构造;用例写"F0 上…"时只改所列字段,其余取下表的值;`RefData::load(b, None)` 简写为 `load(b)`;段号与行下标:Assets 1、Venues 2、Insts 3、Accounts 4、Fees 5、FeeOverrides 6、RateLimits 7、Endpoints 8、Maint 9、Tokens 10、Aliases 11、Strings 12、VenueCaps 13、AccountVenues 14、Instances 15、SimAccounts 16、SimBalances 17、MmrTiers 18、StrRefs 19,行下标从 0 起,Insts 第 k 行是 `InstId(k + 1)`):

| 表 | F0 的取值 |
|---|---|
| 头 | `version = 10`、`base_version = 0`、`created_utc_ns = 1 759 276 800 000 000 000` |
| Assets | 1 `BTC` Crypto `(px_exp, qty_exp) = (−8, −8)`、`px_max` 10 000 000(raw 10¹⁵,−8)、`qty_max` 1 000 000(raw 10¹⁴);2 `USDT` Stable (−8, −8)、`px_max` 10、`qty_max` 10¹⁰;3 `ETH` Crypto (−8, −8)、`px_max` 10⁶、`qty_max` 10⁷(F0 中无 inst 引用);4 `USDC` Stable (−8, −8)、10、10¹⁰;5 `PEPE` Crypto (−12, 0)、`px_max` 1(raw 10¹²)、`qty_max` 10¹⁵ |
| Venues | `BINANCE_USDM`(1,Binance,Perp,`stale_guard = RecvWindow`)、`BINANCE_SPOT`(2,Binance,Spot,RecvWindow)、`OKX_SWAP`(5,Okx,Perp,ExpireAt)、`OKX_SPOT`(6,Okx,Spot,ExpireAt);`ws_max_conn_age_s = 86 400`、`client_id_max_len` 36/36/32/32、`ts_unit = Ms` |
| VenueCaps | 永续行(1、5):`market = LinearPerp`、`fee_asset_rule = Quote`、`reduce_only = true`、`OneWay`、`Cross`、`required_multi_asset = Some(false)`、`required_spot_borrow = None`、`PerSymbolPush`、`margin_data = Push`、`ClientIdPrefix` + `"autoclose-"`、`ignorable_user_events = ["TRADE_LITE"]`、`flags = AMEND\|POST_ONLY\|IOC`(7)、`dms = None` 且四个时长 0、`testnet = Testnet`;现货行(2、6):`market = Spot`、`ReceivedAsset`、`reduce_only = false`、两个 mode `NotApplicable`、`required_multi_asset = None`、`required_spot_borrow` 行 2 为 `None`、行 6 为 `Some(false)`、`funding_settle = NotApplicable`、`margin_data = NotApplicable`、`NotIdentifiable` + `""`、`[]`、`flags = POST_ONLY\|IOC`(6)、`dms = None`、`Testnet` |
| Insts | 1 `BTCUSDT`@1 Perp BTC/USDT 平移 (0, 0)、`tick_str "0.10"` → `tick` 0.1(raw 10 000 000,−8)、`lot_str "0.001"` → 100 000、`min_qty_str "0.001"` → 100 000、`max_qty` 1 000、`min_notional` 5 USDT、`book_window` 4 096、`venue_num_id` 0、`funding_interval_s` 28 800、`max_leverage` 125;2 `BTCUSDT`@2 Spot BTC/USDT、`"0.01"` → 1 000 000、`"0.00001"` → 1 000、`"0.00001"`;3 `BTC-USDT-SWAP`@5 Perp、`qty_unit_shift = −2`(`qty_wire_exp = −6`)、`"0.1"` → 10 000 000、`lot_str "0.01"` → 10 000、`"0.01"`、`venue_num_id` 10 459;4 `BTC-USDT`@6 Spot、`"0.1"`、`"0.00000001"` → 1、`"0.00001"` → 1 000、`venue_num_id` 20 001;5 `1000PEPEUSDT`@1 Perp PEPE/USDT、平移 (3, 3)(`px_wire_exp = −9`、`qty_wire_exp = −3`)、`"0.0000001"` → 100、`"1"` → 1 000、`"1"`;全部 `Trading`、`book_window` 4 096;`max_qty` 除 inst 5 为 10⁹ PEPE 外都是 1 000 BTC |
| Aliases | `(venue 2, "BTC_USDT") → inst 2` |
| Instances | 1 `"pc-main"` kind 0xFF00;2 `"pc-sim-a"` kind 0xFF00;3 `"probe-a"` kind 0xFF01(未绑定) |
| Accounts | 1 `"acct-a"` Live `is_prod = true`、`bound_instance = 1`、`venues = {1, 6}`、`uid_policy = Verified`;2 `"sim-a"` Sim `is_prod = false`、`bound_instance = 2`、`venues = {1, 6}` |
| AccountVenues | `(1, 1, "1001")`、`(1, 6, "2001")` |
| Fees | `(1, 0)` maker 0.0002 / taker 0.0005;`(1, 1)` 0.0001 / 0.0004;`(2, 0)`、`(5, 0)` 0.0002 / 0.0005;`(6, 0)` 0.0008 / 0.001;`(6, 1)` 0.0006 / 0.0008;FeeOverrides 无 |
| Sim | SimAccounts `(2, fee_tier 1, latency_model 0, margin_model 0)`;SimBalances `(2, 1, USDT, 10 000)`、`(2, 6, USDT, 10 000)` |
| 新字段 | 全部资产 `dust` raw 1 000;Insts 的 `mmr_tiers` 为空(无段 18)、`price_band = {0, 0}`;VenueCaps 各行 `amend_qty_down_only = false` |
| 其余 | RateLimits `(1, Account, Orders, 300, 10 000 ms, 1)`、`(1, Ip, Weight, 2 400, 60 000 ms, 1)`;Endpoints venue 1:`OrderWs`/Prod、`MdPublic`/Prod、`MdPublic`/Testnet 各一行;Maint、Tokens 无 |

**`E0`**(`tests/common/e0.rs::e0()`,经 `EcfBuilder`):`magic = HFTECF01`、`version = 3`、`host_class = c8`(3),段 1–8 各一段(`row_ver = 1`,行内容为各段的测试字节,本 crate 不解析)。**`P0`**:与 F0 四个 venue 一致的 `ProtocolFacts`(`market`、`caps` 等于 F0 的 VenueCaps 行、`stale_guard` 同 F0);**`A0`** = `AssemblyFacts { host: C8, max_universe_insts: 8192, protocols: P0, kinds: [0xFF00, 0xFF01] }`。

### REF-001 RdbHeader 是 64 B 且字段偏移固定

- 规则:[实现说明](README.md#实现说明)
- 级别:table
- 里程碑:M1
- 前置:编译期断言与 `offset_of!`;对 ECF1 头同样成立(同一结构)
- 实现:`crates/hft-refdata/tests/layout.rs::ref_001_rdb_header_layout`

| 行 | 断言 | 期望 |
|---|---|---|
| REF-001.1 | `size_of::<RdbHeader>()`、`align_of` | 64、8 |
| REF-001.2 | `magic`、`format`、`n_sections`、`_pad` | 0、8、10、12 |
| REF-001.3 | `version`、`base_version`、`created_utc_ns`、`total_len` | 16、24、32、40 |
| REF-001.4 | `_rsv`(16 B;ECF1 的 `host_class` 是 `_rsv[0]`) | 48 |

### REF-002 SectionEntry 是 24 B 且字段偏移固定

- 规则:[实现说明](README.md#实现说明)
- 级别:table
- 里程碑:M1
- 前置:编译期断言
- 实现:`crates/hft-refdata/tests/layout.rs::ref_002_section_entry_layout`

| 行 | 断言 | 期望 |
|---|---|---|
| REF-002.1 | `size_of::<SectionEntry>()` | 24 |
| REF-002.2 | `id`、`row_ver`、`row_size`、`offset`、`count`、`flags` | 0、2、4、8、16、20 |
| REF-002.3 | `StrRef { off: u32, len: u32 }` 大小 | 8 |

### REF-003 F0 加载成功,身份是整块字节的 SHA-256

- 规则:[公开 API](README.md#公开-api)、[实现说明](README.md#实现说明)
- 级别:unit
- 里程碑:M1
- 前置:`b = f0()`
- 步骤:
  1. `rd = load(&b).unwrap()`
- 期望:
  - `rd.version() == 10`;`rd.sha256() == sha2::Sha256::digest(&b)`
  - `rd.warnings()` 为空;`rd.symbols(1).get(b"BTCUSDT") == Some(InstId(1))`
- 实现:`crates/hft-refdata/tests/load.rs::ref_003_f0_loads`

### REF-004 同一输入两次构造的 blob 逐字节相同

- 规则:[实现说明](README.md#实现说明)
- 级别:unit
- 里程碑:M1
- 前置:F0 的全部输入(含 `created_utc_ns`)
- 步骤:
  1. 两个独立的 `RdbBuilder` 以相同顺序与相同输入构造并 `finish()`
- 期望:两份字节完全相同;SHA-256 相同
- 实现:`crates/hft-refdata/tests/builder.rs::ref_004_builder_deterministic`

### REF-005 头部字段非法时以 Structure 拒绝

- 规则:[LoadErrCode 码表](README.md#loaderrcode-码表)、[实现说明](README.md#实现说明)
- 级别:table
- 里程碑:M1
- 前置:F0 字节上只改所列头部字段,其余字节不变;期望写 `errors`(`LoadReport` 的全部条目)
- 实现:`crates/hft-refdata/tests/structure.rs::ref_005_header_structure`

| 行 | 改动 | 期望 |
|---|---|---|
| REF-005.1 | `magic = *b"HFTRDB02"` | `[{Structure, 0, 0}]` |
| REF-005.2 | 把 `E0` 交给 `RefData::load`(magic `HFTECF01`) | `[{Structure, 0, 0}]` |
| REF-005.3 | `format = 0` | `[{Structure, 0, 0}]` |
| REF-005.4 | `format = 2` | `[{Structure, 0, 0}]` |
| REF-005.5 | `total_len = blob.len() + 1` | `[{Structure, 0, 0}]` |
| REF-005.6 | `total_len = blob.len() − 1` | `[{Structure, 0, 0}]` |
| REF-005.7 | blob 只取前 63 字节 | `[{Structure, 0, 0}]` |
| REF-005.8 | `base_version = 1` | `[{Structure, 0, 0}]` |
| REF-005.9 | `n_sections` 改为 200(目录超出 blob) | `[{Structure, 0, 0}]` |
| REF-005.10 | 未改(对照) | `Ok` |
| REF-005.11 | `version = 0` | `[{Structure, 0, 0}]` |

### REF-006 段越界、重叠、未对齐与重复段 id 以 Structure 拒绝

- 规则:[实现说明](README.md#实现说明)
- 级别:table
- 里程碑:M1
- 前置:F0 经测试钩子 `RdbBuilder::finish_raw()` 得到 `RawBlob`,把 Insts 段排到最后再编码,然后就地改目录项
- 实现:`crates/hft-refdata/tests/structure.rs::ref_006_section_bounds`

| 行 | 目录改动 | 期望 |
|---|---|---|
| REF-006.1 | Insts 段 `offset + row_size × count == total_len` | `Ok` |
| REF-006.2 | 同上再 `count + 1`(越界 1 行) | `[{Structure, 3, 0}]` |
| REF-006.3 | Venues 段起点前移 8 B,与 Assets 段末 8 B 重叠 | `Err`,`errors` 恰一条且 `code == Structure` |
| REF-006.4 | Venues 段紧接 Assets 段末尾(相邻不重叠) | `Ok` |
| REF-006.5 | Venues 段 `offset % 8 == 4` | `[{Structure, 2, 0}]` |
| REF-006.6 | 目录中 Assets 段出现两次(内容相同) | `Err`,`errors` 恰一条且 `code == Structure` |

### REF-007 row_size 小于已知大小拒绝,大于时按前缀读取

- 规则:[实现说明](README.md#实现说明)
- 级别:table
- 里程碑:M1
- 前置:F0 的 Insts 段;已知行大小记为 `K`;"追加 8 B"指每行末尾追加 8 个 0xAB 字节
- 实现:`crates/hft-refdata/tests/structure.rs::ref_007_row_size_prefix`

| 行 | 改动 | 期望 |
|---|---|---|
| REF-007.1 | `row_size = K − 8`(行截短) | `[{Structure, 3, 0}]` |
| REF-007.2 | `row_size = K` | `Ok` |
| REF-007.3 | `row_size = K + 8`,每行追加 8 B | `Ok`;5 个 inst 的 `InstHot` 与 `InstMeta` 全部字段等于 F0 加载的结果 |

### REF-008 未知段带 OPTIONAL 跳过、不带拒绝,退役段一律拒绝

- 规则:[实现说明](README.md#实现说明)、[LoadErrCode 码表](README.md#loaderrcode-码表)
- 级别:table
- 里程碑:M1
- 前置:F0 追加一个 16 B 的段,`id` 与 `flags` 如下
- 实现:`crates/hft-refdata/tests/structure.rs::ref_008_unknown_and_retired_sections`

| 行 | 段 id | flags | 期望 |
|---|---|---|---|
| REF-008.1 | 20 | OPTIONAL | `Ok`;加载结果与 F0 相同(`sha256` 除外) |
| REF-008.2 | 20 | 0 | `[{Structure, 20, 0}]` |
| REF-008.3 | 31 | 0 | `[{Structure, 31, 0}]` |
| REF-008.4 | 32 | OPTIONAL | `[{Structure, 32, 0}]`(退役) |
| REF-008.5 | 32 | 0 | `[{Structure, 32, 0}]` |
| REF-008.6 | 33 | 0 | `[{Structure, 33, 0}]`(退役) |
| REF-008.7 | 37 | OPTIONAL | `[{Structure, 37, 0}]`(退役) |
| REF-008.8 | 38 | OPTIONAL | `Ok` |
| REF-008.9 | 38 | 0 | `[{Structure, 38, 0}]` |

### REF-009 行内枚举与 Option 线编码的非法判别值以 Structure 拒绝

- 规则:[LoadErrCode 码表](README.md#loaderrcode-码表)、[数据结构](README.md#数据结构)、[交易所能力表(VenueCaps)](README.md#交易所能力表venuecaps)
- 级别:table
- 里程碑:M1
- 前置:F0 字节上就地改一个字段的编码(其他不变)
- 实现:`crates/hft-refdata/tests/structure.rs::ref_009_bad_discriminants`

| 行 | 字段(段, 行) | 写入值 | 期望 |
|---|---|---|---|
| REF-009.1 | Insts 0 `status` | 7 | `[{Structure, 3, 0}]` |
| REF-009.2 | Insts 0 `kind` | 0 | `[{Structure, 3, 0}]` |
| REF-009.3 | Insts 0 `kind` | 4 | `[{Structure, 3, 0}]` |
| REF-009.4 | Assets 0 `class` | 5 | `[{Structure, 1, 0}]` |
| REF-009.5 | Venues 0 `exchange` | 9 | `[{Structure, 2, 0}]` |
| REF-009.6 | Venues 0 `stale_guard` | 0 | `[{Structure, 2, 0}]` |
| REF-009.7 | Venues 0 `ts_unit` | 4 | `[{Structure, 2, 0}]` |
| REF-009.8 | Accounts 0 `kind` | 3 | `[{Structure, 4, 0}]` |
| REF-009.9 | Accounts 0 `uid_policy` | 0 | `[{Structure, 4, 0}]` |
| REF-009.10 | Accounts 0 `uid_policy` | 3 | `[{Structure, 4, 0}]` |
| REF-009.11 | VenueCaps 0 `funding_settle` | 5 | `[{Structure, 13, 0}]` |
| REF-009.12 | VenueCaps 0 `margin_data` 判别 | 4 | `[{Structure, 13, 0}]` |
| REF-009.13 | VenueCaps 0 `required_position_mode` | 4 | `[{Structure, 13, 0}]` |
| REF-009.14 | VenueCaps 0 `required_margin_mode` | 5 | `[{Structure, 13, 0}]` |
| REF-009.15 | VenueCaps 0 `required_multi_asset` | 0 | `[{Structure, 13, 0}]` |
| REF-009.16 | VenueCaps 0 `required_multi_asset` | 4 | `[{Structure, 13, 0}]` |
| REF-009.17 | VenueCaps 0 `required_spot_borrow` | 4 | `[{Structure, 13, 0}]` |
| REF-009.18 | VenueCaps 0 `own_liquidation_marker` | 5 | `[{Structure, 13, 0}]` |
| REF-009.19 | VenueCaps 0 `dms` | 6 | `[{Structure, 13, 0}]` |
| REF-009.20 | VenueCaps 0 `testnet` | 4 | `[{Structure, 13, 0}]` |
| REF-009.21 | VenueCaps 0 `fee_asset_rule` 判别 | 5 | `[{Structure, 13, 0}]` |
| REF-009.22 | RateLimits 0 `scope` | 6 | `[{Structure, 7, 0}]` |
| REF-009.23 | RateLimits 0 `bucket` | 6 | `[{Structure, 7, 0}]` |
| REF-009.24 | Endpoints 0 `role` | 7 | `[{Structure, 8, 0}]` |
| REF-009.25 | Endpoints 0 `env` | 4 | `[{Structure, 8, 0}]` |
| REF-009.26 | F0 + Maint 行 `kind` | 4 | `[{Structure, 9, 0}]` |

### REF-010 某一步有错误时不进入下一步

- 规则:[实现说明](README.md#实现说明)
- 级别:unit
- 里程碑:M1
- 前置:F0 上同时:`format = 2`(第 1 步)、账户 2 的 `label = "acct-a"`(第 2 步)、Assets id 改为 `[1, 2, 4, 5, 6]`(第 3 步)
- 步骤:
  1. `r = load(&b).unwrap_err()`
- 期望:`r.errors == [{Structure, 0, 0}]`;`r.code() == Structure`(1)
- 实现:`crates/hft-refdata/tests/report.rs::ref_010_stop_after_failing_step`

### REF-011 同一步内收集全部错误,按规则顺序与 (段, 行) 升序

- 规则:[实现说明](README.md#实现说明)、[公开 API](README.md#公开-api)
- 级别:unit
- 里程碑:M1
- 前置:F0 上 Instances id 改为 `[1, 2, 4]`(缺 3)且 Assets id 改为 `[1, 2, 3, 5, 6]`(缺 4);两处都只触发第 3 步
- 步骤:
  1. `r = load(&b).unwrap_err()`
- 期望:`r.errors == [{Ids, 1, 3}, {Ids, 15, 2}]`(先 Assets 段、后 Instances 段);`r.code() == Ids`(3)
- 实现:`crates/hft-refdata/tests/report.rs::ref_011_collect_within_step`

### REF-012 字符串引用与符号字符规则

- 规则:[实现说明](README.md#实现说明)、[配置项](README.md#配置项)
- 级别:table
- 里程碑:M1
- 前置:F0;`refdata.max_symbol_len` 为常量 27
- 实现:`crates/hft-refdata/tests/strings.rs::ref_012_strings`

| 行 | 改动 | 期望 |
|---|---|---|
| REF-012.1 | inst 1 的符号 `StrRef` 使 `off + len == Strings 段长` | `Ok` |
| REF-012.2 | 同上 `len + 1` | `[{Strings, 3, 0}]` |
| REF-012.3 | inst 1 的符号字节为 `[0xFF, 0xFE]` | `[{Strings, 3, 0}]` |
| REF-012.4 | inst 1 的符号为空串 | `[{Strings, 3, 0}]` |
| REF-012.5 | inst 1 的符号为 `"A"`(1 字节) | `Ok`;`get(b"A") == Some(InstId(1))` |
| REF-012.6 | inst 1 的符号为 27 个 `'A'` | `Ok` |
| REF-012.7 | inst 1 的符号为 28 个 `'A'` | `[{Strings, 3, 0}]` |
| REF-012.8 | inst 1 的符号为 `"BTC USDT"` | `[{Strings, 3, 0}]` |
| REF-012.9 | inst 1 的符号为 `"\tBTCUSDT"` | `[{Strings, 3, 0}]` |
| REF-012.10 | inst 1 的符号含 0x7F | `[{Strings, 3, 0}]` |
| REF-012.11 | 别名为 28 个 `'B'` | `[{Strings, 11, 0}]` |
| REF-012.12 | VenueCaps 行 0 的 `ignorable_user_events` 的 `{off, count}` 超出 StrRef 数组 1 项 | `[{Strings, 13, 0}]` |
| REF-012.13 | 实例 2 的名字改为 `"pc-main"` | `[{Strings, 15, 1}]` |
| REF-012.14 | 账户 2 的 `label` 改为 `"acct-a"` | `[{Strings, 4, 1}]` |

### REF-013 稠密 id 表必须从 1 起连续、唯一、升序

- 规则:[实现说明](README.md#实现说明)
- 级别:table
- 里程碑:M1
- 前置:F0;只改所列表的 id 序列(引用随之改,使第 4 步之后不受影响)
- 实现:`crates/hft-refdata/tests/ids.rs::ref_013_dense_ids`

| 行 | 改动 | 期望 |
|---|---|---|
| REF-013.1 | Assets `[1, 2, 3, 4, 5]`(F0) | `Ok` |
| REF-013.2 | Assets [1, 2, 4, 5, 6] | `[{Ids, 1, 2}]` |
| REF-013.3 | Assets [0, 1, 2, 3, 4] | `[{Ids, 1, 0}]` |
| REF-013.4 | Assets [2, 1, 3, 4, 5] | `[{Ids, 1, 0}]` |
| REF-013.5 | Assets [1, 1, 2, 3, 4] | `[{Ids, 1, 1}]` |
| REF-013.6 | Insts [1, 2, 3, 5, 6] | `[{Ids, 3, 3}]` |
| REF-013.7 | Accounts [1, 3] | `[{Ids, 4, 1}]` |
| REF-013.8 | Instances [1, 2, 4] | `[{Ids, 15, 2}]` |

### REF-014 以 VenueId 为键的表不要求稠密,但要求唯一升序

- 规则:[实现说明](README.md#实现说明)
- 级别:table
- 里程碑:M1
- 前置:F0
- 实现:`crates/hft-refdata/tests/ids.rs::ref_014_venue_keyed_tables`

| 行 | 改动 | 期望 |
|---|---|---|
| REF-014.1 | Venues `[1, 2, 5, 6]`(F0,不稠密) | `Ok` |
| REF-014.2 | Venues [2, 1, 5, 6] | `[{Ids, 2, 0}]` |
| REF-014.3 | Venues [1, 2, 5, 5] | `[{Ids, 2, 3}]` |
| REF-014.4 | VenueCaps 行 `[1, 1, 2, 5, 6]`(venue 1 两行) | `[{Ids, 13, 1}]` |
| REF-014.5 | VenueCaps 行 [2, 1, 5, 6] | `[{Ids, 13, 0}]` |

### REF-015 VenueId 不在 hft-types 常量表内报 UnknownVenue

- 规则:[实现说明](README.md#实现说明)、[生命周期(变更 = 重启)](README.md#生命周期变更--重启)
- 级别:table
- 里程碑:M1
- 前置:F0 的 Venues 段按升序插入一行(无 inst、无 VenueCaps 行)
- 实现:`crates/hft-refdata/tests/ids.rs::ref_015_unknown_venue`

| 行 | 追加的 VenueId | 期望 |
|---|---|---|
| REF-015.1 | 0 | `[{UnknownVenue, 2, 0}]` |
| REF-015.2 | 7(保留的 `BINANCE_COINM`) | `[{UnknownVenue, 2, 4}]` |
| REF-015.3 | 16 | `[{UnknownVenue, 2, 4}]` |
| REF-015.4 | 34 | `[{UnknownVenue, 2, 4}]` |
| REF-015.5 | 3(`GATE_USDT_FUT`,optional) | `Ok` |
| REF-015.6 | 33(`CHAIN_SOLANA`,optional) | `Ok` |

### REF-016 实例 id 上限 254

- 规则:[实现说明](README.md#实现说明)、[数据结构](README.md#数据结构)
- 级别:table
- 里程碑:M1
- 前置:F0 的 Instances 扩成 id 1..=N(名字 `"i<N>"`,kind 0xFF00,只有 1、2 被绑定)
- 实现:`crates/hft-refdata/tests/ids.rs::ref_016_instance_id_range`

| 行 | N | 期望 |
|---|---|---|
| REF-016.1 | 253 | `Ok` |
| REF-016.2 | 254 | `Ok`;`instance(StrategyInstanceId(254))` 为 `Some` |
| REF-016.3 | 255 | `[{InstanceIdRange, 15, 254}]` |

### REF-017 悬空引用以 DanglingRef 拒绝

- 规则:[实现说明](README.md#实现说明)
- 级别:table
- 里程碑:M1
- 前置:F0;"9"表示不存在的 `AssetId(9)`/`AccountId(9)`/`StrategyInstanceId(9)`,`InstId(99)` 不存在,`VenueId 3` 合法但不在 Venues 中;追加的行排在该段末尾
- 实现:`crates/hft-refdata/tests/refs.rs::ref_017_dangling_refs`

| 行 | 改动 | 期望 |
|---|---|---|
| REF-017.1 | inst 1 的 venue = 3 | `[{DanglingRef, 3, 0}]` |
| REF-017.2 | inst 1 的 asset = 9 | `[{DanglingRef, 3, 0}]` |
| REF-017.3 | inst 1 的 quote = 9 | `[{DanglingRef, 3, 0}]` |
| REF-017.4 | 追加 FeeOverrides `(account 9, venue 1)` | `[{DanglingRef, 6, 0}]` |
| REF-017.5 | 追加 FeeOverrides `(account 1, venue 3)` | `[{DanglingRef, 6, 0}]` |
| REF-017.6 | 追加 Maint `(venue 6, inst 99)` | `[{DanglingRef, 9, 0}]` |
| REF-017.7 | 追加 Maint `(venue 3, inst 0)` | `[{DanglingRef, 9, 0}]` |
| REF-017.8 | 追加 Endpoints `(venue 3, …)` | `[{DanglingRef, 8, 3}]` |
| REF-017.9 | 追加 RateLimits `(venue 3, …)` | `[{DanglingRef, 7, 2}]` |
| REF-017.10 | Venues 加 `CHAIN_BSC`(32),追加 Tokens `(chain 32, asset 9)` | `[{DanglingRef, 10, 0}]` |
| REF-017.11 | 追加 Tokens `(chain 2, asset USDT)`(chain 不是链上 venue) | `[{DanglingRef, 10, 0}]` |
| REF-017.12 | Venues 加 `CHAIN_BSC`,追加 Tokens `(chain 32, asset USDT, decimals 18)` | `Ok` |
| REF-017.13 | 别名改为 `(venue 2, "BTC_USDT") → inst 99` | `[{DanglingRef, 11, 0}]` |
| REF-017.14 | 别名改为 `(venue 2, "BTC_USDT") → inst 1`(inst 1 在 venue 1) | `[{DanglingRef, 11, 0}]` |
| REF-017.15 | 追加 venue 10(`BITGET_UTA_USDT_FUT`)的 VenueCaps 行,Venues 中没有 venue 10 | `[{DanglingRef, 13, 4}]` |
| REF-017.16 | VenueCaps 行 0 的 `fee_asset_rule = Configured(9)` | `[{DanglingRef, 13, 0}]` |
| REF-017.17 | VenueCaps 行 1 的 `fee_asset_rule = ReceivedAssetUnlessDiscount{token: 9}` | `[{DanglingRef, 13, 1}]` |
| REF-017.18 | 账户 1 的 `bound_instance = 9` | `[{DanglingRef, 4, 0}]` |
| REF-017.19 | 账户 1 的 `bound_instance = 0` | `Ok`;`account_of(1) == None` |
| REF-017.20 | 追加 AccountVenues `(account 9, venue 1, "x")` | `[{DanglingRef, 14, 2}]` |
| REF-017.21 | 追加 AccountVenues `(account 1, venue 3, "x")`(按键排在第 1 行) | `[{DanglingRef, 14, 1}]` |
| REF-017.22 | SimAccounts 行的 account = 9 | `[{DanglingRef, 16, 0}]` |
| REF-017.23 | SimBalances 行 0 的 asset = 9 | `[{DanglingRef, 17, 0}]` |
| REF-017.24 | 账户 1 的 `venues` 加 bit 3(venue 3 不在 Venues) | `[{DanglingRef, 4, 0}]` |

### REF-018 报价资产必须是稳定币

- 规则:[实现说明](README.md#实现说明)
- 级别:table
- 里程碑:M1
- 前置:F0
- 实现:`crates/hft-refdata/tests/refs.rs::ref_018_quote_stable`

| 行 | 改动 | 期望 |
|---|---|---|
| REF-018.1 | inst 2 的 quote = BTC(`Crypto`) | `[{QuoteNotStable, 3, 1}]` |
| REF-018.2 | inst 2 的 quote = USDC(`Stable`) | `Ok` |
| REF-018.3 | USDT 的 `class = Equity` | `[{QuoteNotStable, 3, 0}, {QuoteNotStable, 3, 1}, {QuoteNotStable, 3, 2}, {QuoteNotStable, 3, 3}, {QuoteNotStable, 3, 4}]` |

### REF-019 一个实例可被多个账户绑定(账户集合)

- 规则:[实现说明](README.md#实现说明)、[数据结构](README.md#数据结构)
- 级别:table
- 里程碑:M1
- 前置:F0
- 实现:`crates/hft-refdata/tests/refs.rs::ref_019_instance_account_set`

| 行 | 改动 | 期望 |
|---|---|---|
| REF-019.1 | 账户 2 的 `bound_instance = 1`(与账户 1 相同) | `Ok`;`accounts_of(1) = [1, 2]`,`account_of(1) = 1`,账户 2 的 `bound_instance` 仍为 1 |
| REF-019.2 | 账户 1、2 的 `bound_instance` 都为 0 | `Ok` |

### REF-020 有 inst 的 venue 缺 VenueCaps 行报 MissingCaps,无 inst 的 venue 不要求

- 规则:[实现说明](README.md#实现说明)、[交易所能力表(VenueCaps)](README.md#交易所能力表venuecaps)
- 级别:table
- 里程碑:M1
- 前置:F0
- 实现:`crates/hft-refdata/tests/caps.rs::ref_020_missing_caps`

| 行 | 改动 | 期望 |
|---|---|---|
| REF-020.1 | 删除 venue 5 的 VenueCaps 行(inst 3 在 venue 5) | `Err`,`errors` 恰一条且 `code == MissingCaps` |
| REF-020.2 | inst 3 改到 venue 1(符号与 `venue_num_id` 不变),删除 venue 5 的 VenueCaps 行(venue 5 仍在 Venues) | `Ok`;`caps(5)` 为 NONE 行(`flags == 0`) |

### REF-021 能力表的市场段不一致以 CapsMarketMismatch 拒绝

- 规则:[实现说明](README.md#实现说明)、[交易所能力表(VenueCaps)](README.md#交易所能力表venuecaps)
- 级别:table
- 里程碑:M1
- 前置:F0;VenueCaps 行下标:venue 1 → 0,2 → 1,5 → 2,6 → 3
- 实现:`crates/hft-refdata/tests/caps.rs::ref_021_caps_market_mismatch`

| 行 | 改动 | 期望 |
|---|---|---|
| REF-021.1 | 行 1(`BINANCE_SPOT`)`market = LinearPerp` | `[{CapsMarketMismatch, 13, 1}]` |
| REF-021.2 | 行 0(`BINANCE_USDM`)`market = Spot` | `[{CapsMarketMismatch, 13, 0}]` |
| REF-021.3 | Venues 中 `BINANCE_SPOT` 的 `product = Perp`(caps 行仍为 `Spot`) | `[{CapsMarketMismatch, 13, 1}, {CapsMarketMismatch, 3, 1}]`(caps 行与 `VenueId::market()` 不符在先,inst 2 的 Spot 与 product 不符在后) |
| REF-021.4 | 行 1 `required_position_mode = OneWay` | `[{CapsMarketMismatch, 13, 1}]` |
| REF-021.5 | 行 1 `required_margin_mode = Cross` | `[{CapsMarketMismatch, 13, 1}]` |
| REF-021.6 | 行 1 `funding_settle = PerSymbolPush` | `[{CapsMarketMismatch, 13, 1}]` |
| REF-021.7 | 行 1 `margin_data = Push` | `[{CapsMarketMismatch, 13, 1}]` |
| REF-021.8 | 行 1 `margin_data = Poll{interval_ms: 60 000}` | `[{CapsMarketMismatch, 13, 1}]` |
| REF-021.9 | 行 1 `required_multi_asset = Some(false)` | `[{CapsMarketMismatch, 13, 1}]` |
| REF-021.10 | 行 0 `required_spot_borrow = Some(false)` | `[{CapsMarketMismatch, 13, 0}]` |
| REF-021.11 | 行 0 `funding_settle = NotApplicable` | `[{CapsMarketMismatch, 13, 0}]` |
| REF-021.12 | 行 0 `required_margin_mode = Mixed` | `[{CapsMarketMismatch, 13, 0}]` |
| REF-021.13 | 行 0 `required_margin_mode = Isolated` | `Ok` |
| REF-021.14 | 行 0 `required_position_mode = NotApplicable`(永续不检查持仓模式) | `Ok` |
| REF-021.15 | inst 2(`BINANCE_SPOT`)的 `kind = Perp` | `[{CapsMarketMismatch, 3, 1}]` |

### REF-022 现货行要求借币以 SpotBorrowRequired 拒绝

- 规则:[实现说明](README.md#实现说明)、[交易所能力表(VenueCaps)](README.md#交易所能力表venuecaps)
- 级别:table
- 里程碑:M1
- 前置:F0 的 VenueCaps 行 3(`OKX_SPOT`)
- 实现:`crates/hft-refdata/tests/caps.rs::ref_022_spot_borrow`

| 行 | `required_spot_borrow` | 期望 |
|---|---|---|
| REF-022.1 | `Some(true)` | `[{SpotBorrowRequired, 13, 3}]` |
| REF-022.2 | `Some(false)` | `Ok` |
| REF-022.3 | `None` | `Ok` |

### REF-023 Poll 的轮询周期边界

- 规则:[实现说明](README.md#实现说明)
- 级别:table
- 里程碑:M1
- 前置:F0 的 VenueCaps 行 0 `margin_data = Poll{interval_ms}`
- 实现:`crates/hft-refdata/tests/caps.rs::ref_023_poll_interval`

| 行 | `interval_ms` | 期望 |
|---|---|---|
| REF-023.1 | 0 | `[{CapsValue, 13, 0}]` |
| REF-023.2 | 999 | `[{CapsValue, 13, 0}]` |
| REF-023.3 | 1 000 | `Ok`;`caps(1).margin_data == Poll{interval_ms: 1000}` |
| REF-023.4 | 600 000 | `Ok` |
| REF-023.5 | 600 001 | `[{CapsValue, 13, 0}]` |

### REF-024 死手时长的约束

- 规则:[实现说明](README.md#实现说明)、[交易所能力表(VenueCaps)](README.md#交易所能力表venuecaps)
- 级别:table
- 里程碑:M1
- 前置:F0 的 VenueCaps 行 2(`OKX_SWAP`);列为 `(dms, dms_min_ms, dms_max_ms, dms_timeout_ms, dms_refresh_ms)`
- 实现:`crates/hft-refdata/tests/caps.rs::ref_024_dead_man_range`

| 行 | 取值 | 期望 |
|---|---|---|
| REF-024.1 | `(CountdownAccount, 10 000, 120 000, 20 000, 10 000)`(超时恰为 2 × 刷新) | `Ok` |
| REF-024.2 | `(CountdownAccount, 10 000, 120 000, 20 000, 10 001)` | `[{DeadManRange, 13, 2}]` |
| REF-024.3 | `(CountdownAccount, 10 000, 120 000, 10 000, 5 000)`(超时恰为下限) | `Ok` |
| REF-024.4 | `(CountdownAccount, 10 000, 120 000, 9 999, 4 000)` | `[{DeadManRange, 13, 2}]` |
| REF-024.5 | `(CountdownAccount, 10 000, 120 000, 120 000, 10 000)`(恰为上限) | `Ok` |
| REF-024.6 | `(CountdownAccount, 10 000, 120 000, 120 001, 10 000)` | `[{DeadManRange, 13, 2}]` |
| REF-024.7 | `(CountdownPerInst, 0, 0, 3 600 000, 1 000)`(`max = 0` 无上限) | `Ok` |
| REF-024.8 | `(OnDisconnect, 0, 0, 10 000, 0)`(刷新 0) | `[{DeadManRange, 13, 2}]` |
| REF-024.9 | `(None, 0, 0, 0, 0)` | `Ok` |
| REF-024.10 | `(None, 1, 0, 0, 0)` | `[{DeadManRange, 13, 2}]` |
| REF-024.11 | `(None, 0, 1, 0, 0)` | `[{DeadManRange, 13, 2}]` |
| REF-024.12 | `(None, 0, 0, 1, 0)` | `[{DeadManRange, 13, 2}]` |
| REF-024.13 | `(None, 0, 0, 0, 1)` | `[{DeadManRange, 13, 2}]` |

### REF-025 资产指数范围与名义指数约束

- 规则:[实现说明](README.md#实现说明)、[数据结构](README.md#数据结构)
- 级别:table
- 里程碑:M1
- 前置:F0 的 ETH(资产 3,无 inst 引用);每行 ETH 的 `px_max`、`qty_max` 的 raw 都设为 1 000(使第 8 步不受影响)
- 实现:`crates/hft-refdata/tests/exps.rs::ref_025_asset_exp_range`

| 行 | `(px_exp, qty_exp)` | 期望 |
|---|---|---|
| REF-025.1 | (−18, −8) | `Ok` |
| REF-025.2 | (−19, −8) | `[{ExpRange, 1, 2}]` |
| REF-025.3 | (6, −12) | `Ok` |
| REF-025.4 | (7, −13) | `[{ExpRange, 1, 2}]` |
| REF-025.5 | (−8, −18) | `Ok` |
| REF-025.6 | (−8, −19) | `[{ExpRange, 1, 2}]` |
| REF-025.7 | (−12, 6) | `Ok`(和为 −6) |
| REF-025.8 | (−13, 7) | `[{ExpRange, 1, 2}]` |
| REF-025.9 | (−2, −4) | `Ok`(和为 −6) |
| REF-025.10 | (−1, −4) | `[{NotionalExp, 1, 2}]`(和为 −5) |
| REF-025.11 | (−8, 6) | `[{NotionalExp, 1, 2}]`(和为 −2) |

### REF-026 单位平移范围与线格式指数由加载器计算且必须落在 [−18, 6]

- 规则:[实现说明](README.md#实现说明)、[数据结构](README.md#数据结构)
- 级别:table
- 里程碑:M1
- 前置:F0 加资产 6 `TST` Crypto `(−4, −8)`、`px_max`/`qty_max` raw 10¹²,与 inst 6 `TSTUSDT`@`BINANCE_SPOT`(Spot,TST/USDT,`book_window` 64,`max_qty` raw 1 000,`min_qty_str = lot_str`、`min_qty = lot`);每行给出 `(px_unit_shift, qty_unit_shift, tick_str, lot_str)`
- 实现:`crates/hft-refdata/tests/exps.rs::ref_026_unit_shift_and_wire_exp`

| 行 | 取值 | 期望 |
|---|---|---|
| REF-026.1 | (0, 0, `"0.0001"`, `"0.00000001"`) | `Ok`;`inst(6)`:`px_wire_exp = −4`、`qty_wire_exp = −8`、`tick = 1`、`lot = 1` |
| REF-026.2 | (−12, 0, `"0.0000000000000001"`, `"0.00000001"`) | `Ok`;`px_wire_exp = −16`、`tick = 1` |
| REF-026.3 | (−13, 0, …) | `[{ExpRange, 3, 5}]` |
| REF-026.4 | (10, 0, `"1000000"`, `"0.00000001"`) | `Ok`;`px_wire_exp = 6`、`tick = 1` |
| REF-026.5 | (11, 0, `"10000000"`, `"0.00000001"`)(`px_wire_exp = 7`) | `[{ExpRange, 3, 5}]` |
| REF-026.6 | (0, 10, `"0.0001"`, `"0.000000000000000001"`) | `Ok`;`qty_wire_exp = −18`、`lot = 1` |
| REF-026.7 | (0, 11, `"0.0001"`, …)(`qty_wire_exp = −19`) | `[{ExpRange, 3, 5}]` |
| REF-026.8 | (0, −12, `"0.0001"`, `"10000"`) | `Ok`;`qty_wire_exp = 4`、`lot = 1` |
| REF-026.9 | (0, −13, …) | `[{ExpRange, 3, 5}]` |
| REF-026.10 | (0, 12, …) | `[{ExpRange, 3, 5}]`(`qty_wire_exp = −20`) |
| REF-026.11 | F0 inst 5(`1000PEPEUSDT`,(3, 3)) | `Ok`;`px_wire_exp = −9`、`qty_wire_exp = −3`、`tick = 100`、`lot = 1 000` |

### REF-027 交割合约以 UnsupportedKind 拒绝,InstMeta::market 映射

- 规则:[实现说明](README.md#实现说明)、[数据结构](README.md#数据结构)
- 级别:table
- 里程碑:M1
- 前置:F0
- 实现:`crates/hft-refdata/tests/exps.rs::ref_027_unsupported_kind_and_market`

| 行 | 输入 | 期望 |
|---|---|---|
| REF-027.1 | inst 1 的 `kind = Delivery`(venue 1 仍为 Perp) | `[{UnsupportedKind, 3, 0}]` |
| REF-027.2 | F0 `inst_meta(1).market()` | `LinearPerp` |
| REF-027.3 | F0 `inst_meta(2).market()` | `Spot` |

### REF-028 步长按交易所原文复核

- 规则:[实现说明](README.md#实现说明)、[已知限制与后续工作](README.md#已知限制与后续工作)
- 级别:table
- 里程碑:M1
- 前置:F0 的 inst 1(`px_wire_exp = qty_wire_exp = −8`);行内值的单位是 raw
- 实现:`crates/hft-refdata/tests/steps.rs::ref_028_tick_lot_recheck`

| 行 | 改动 | 期望 |
|---|---|---|
| REF-028.1 | 无(`"0.10"` ↔ 10 000 000) | `Ok` |
| REF-028.2 | `tick = 9 999 999` | `[{TickMismatch, 3, 0}]` |
| REF-028.3 | `tick_str = "0.000000001"`(`tick` 不变) | `[{AssetExpTooCoarse, 3, 0}]` |
| REF-028.4 | `tick_str = "0"`、`tick = 0` | `[{TickMismatch, 3, 0}]` |
| REF-028.5 | `tick_str = "-0.10"`、`tick = −10 000 000` | `[{TickMismatch, 3, 0}]` |
| REF-028.6 | `tick_str = "abc"` | `[{TickMismatch, 3, 0}]` |
| REF-028.7 | `lot = 100 001` | `[{TickMismatch, 3, 0}, {TickMismatch, 3, 0}]`(lot 复核失败;min_qty 100 000 不是 100 001 的倍数) |
| REF-028.8 | `lot_str = "0.0000000001"` | `[{AssetExpTooCoarse, 3, 0}]` |
| REF-028.9 | `lot_str = "0"`、`lot = 0` | `Err`;`code() == TickMismatch`,`errors` 的每一条都是 `{TickMismatch, 3, 0}` |
| REF-028.10 | `min_qty_str = "0.003"`、`min_qty = 300 000`(3 × lot) | `Ok` |
| REF-028.11 | `min_qty_str = "0.0035"`、`min_qty = 350 000`(不是 lot 整数倍) | `[{TickMismatch, 3, 0}]` |
| REF-028.12 | `min_qty_str = "0.003"`、`min_qty = 300 001` | `[{TickMismatch, 3, 0}, {TickMismatch, 3, 0}]`(min_qty 复核失败;也不是 lot 的倍数) |
| REF-028.13 | `min_qty_str = "0"`、`min_qty = 0` | `[{TickMismatch, 3, 0}]` |
| REF-028.14 | `min_qty_str = "0.0000000001"` | `[{AssetExpTooCoarse, 3, 0}]` |
| REF-028.15 | 面值不是 10 的幂(1 张 = 0.5 BTC)而 hft-cfg 写成平移 −1:inst 3 `qty_unit_shift = −1`、`lot_str = "1"`、`lot` = 0.5 BTC(raw 50 000 000)、`min_qty_str = "1"`、`min_qty` raw 50 000 000 | `[{TickMismatch, 3, 2}, {TickMismatch, 3, 2}]`(lot 复核在先、min_qty 复核在后) |

### REF-029 价格与数量上限、簿窗口的范围

- 规则:[实现说明](README.md#实现说明)
- 级别:table
- 里程碑:M1
- 前置:F0;`MAX_RAW / 4 = 2⁶⁰ − 1 = 1 152 921 504 606 846 975`(`MAX_RAW = 2⁶² − 1`,整数除法)
- 实现:`crates/hft-refdata/tests/ranges.rs::ref_029_value_range`

| 行 | 改动 | 期望 |
|---|---|---|
| REF-029.1 | BTC `px_max` raw = 2⁶⁰ − 1 | `Ok` |
| REF-029.2 | BTC `px_max` raw = 2⁶⁰ | `[{ValueRange, 1, 0}]` |
| REF-029.3 | BTC `qty_max` raw = 2⁶⁰ − 1 | `Ok` |
| REF-029.4 | BTC `qty_max` raw = 2⁶⁰ | `[{ValueRange, 1, 0}]` |
| REF-029.5 | inst 1 `max_qty` = BTC `qty_max`(raw 10¹⁴) | `Ok` |
| REF-029.6 | inst 1 `max_qty` raw = 10¹⁴ + 1 | `[{ValueRange, 3, 0}]` |
| REF-029.7 | inst 1 `book_window = 63` | `[{ValueRange, 3, 0}]` |
| REF-029.8 | inst 1 `book_window = 64` | `Ok` |
| REF-029.9 | inst 1 `book_window = 65 536` | `Ok` |
| REF-029.10 | inst 1 `book_window = 65 537` | `[{ValueRange, 3, 0}]` |
| REF-029.11 | BTC `px_max` raw = 4 096 × 10 000 000 = 40 960 000 000(恰为 `book_window × tick`) | `Ok` |
| REF-029.12 | BTC `px_max` raw = 40 959 999 999 | `[{ValueRange, 3, 0}, {ValueRange, 3, 2}, {ValueRange, 3, 3}]`(inst 1、3、4 的 tick 都是 10 000 000) |
| REF-029.13 | BTC `px_max` raw = 2⁶⁰ − 2 | `Ok` |

### REF-030 同 venue 内符号、数值 id 与别名唯一

- 规则:[实现说明](README.md#实现说明)
- 级别:table
- 里程碑:M1
- 前置:F0(inst 1 与 inst 2 同名 `BTCUSDT` 但在不同 venue)
- 实现:`crates/hft-refdata/tests/ranges.rs::ref_030_duplicate_symbol`

| 行 | 改动 | 期望 |
|---|---|---|
| REF-030.1 | 无 | `Ok`;`symbols(1).get(b"BTCUSDT") == Some(1)`、`symbols(2).get(b"BTCUSDT") == Some(2)` |
| REF-030.2 | inst 5 的符号改为 `"BTCUSDT"`(与 inst 1 同在 venue 1) | `[{DuplicateSymbol, 3, 4}]` |
| REF-030.3 | inst 1 与 inst 5 的 `venue_num_id` 都为 777 | `[{DuplicateSymbol, 3, 4}]` |
| REF-030.4 | inst 3(venue 5)与 inst 4(venue 6)的 `venue_num_id` 都为 10 459 | `Ok` |
| REF-030.5 | inst 1 与 inst 5 的 `venue_num_id` 都为 0 | `Ok`(0 不进表) |
| REF-030.6 | 别名改为 `(venue 2, "BTCUSDT") → inst 2`(与符号相同) | `[{DuplicateSymbol, 11, 0}]` |
| REF-030.7 | 再追加别名 `(venue 2, "BTC_USDT") → inst 2`(两条相同) | `[{DuplicateSymbol, 11, 1}]` |
| REF-030.8 | 追加别名 `(venue 6, "BTCUSDT") → inst 4`(venue 6 上无此符号) | `Ok`;`symbols(6).get(b"BTCUSDT") == Some(4)` |

### REF-031 一个账户在同一交易所只用一个市场

- 规则:[实现说明](README.md#实现说明)、[数据结构](README.md#数据结构)
- 级别:table
- 里程碑:M1
- 前置:F0;Live 账户新增的 venue 同时补 `AccountVenues` 行(UID 互不相同)
- 实现:`crates/hft-refdata/tests/integrity.rs::ref_031_same_exchange_markets`

| 行 | 改动 | 期望 |
|---|---|---|
| REF-031.1 | 账户 1 `venues = {5, 6}`(`OKX_SWAP` + `OKX_SPOT`) | `[{SameExchangeMarkets, 4, 0}]` |
| REF-031.2 | 账户 2(Sim)`venues = {1, 2}`(`BINANCE_USDM` + `BINANCE_SPOT`),SimBalances 改到 venue 1,补 Fees `(2, 1)` | `[{SameExchangeMarkets, 4, 1}]` |
| REF-031.3 | 账户 1 `venues = {1, 5}`(两个交易所各一个) | `Ok` |
| REF-031.4 | 账户 1 `venues = {2, 6}` | `Ok` |

### REF-032 同一交易所 UID 重复:涉及正式账户时拒绝,两个非正式账户之间只告警(同一 VenueId 上仍拒绝)

- 规则:[实现说明](README.md#实现说明)、[LoadErrCode 码表](README.md#loaderrcode-码表)、[公开 API](README.md#公开-api)
- 级别:table
- 里程碑:M1
- 前置:F0 追加 Live 账户 3 `"acct-b"` 与 4 `"acct-c"`(`bound_instance = 0`),各自 `venues` 与 AccountVenues 行如下;AccountVenues 行按 (account, venue) 升序,行下标随之确定
- 实现:`crates/hft-refdata/tests/integrity.rs::ref_032_duplicate_venue_uid`

| 行 | 账户 3 | 账户 4 | 期望 |
|---|---|---|---|
| REF-032.1 | `is_prod = true`,venue 6 UID `"2001"`(与账户 1 相同) | 无 | `[{DuplicateVenueUid, 14, 2}]` |
| REF-032.2 | `is_prod = false`,venue 6 UID `"2001"`(账户 1 为正式) | 无 | `[{DuplicateVenueUid, 14, 2}]` |
| REF-032.3 | `is_prod = false`,venue 2 UID `"9001"` | `is_prod = false`,venue 1 UID `"9001"`(同所另一产品线) | `Ok`;`warnings() == [{DuplicateVenueUid, 14, 3}]` |
| REF-032.4 | `is_prod = false`,venue 2 UID `"9001"` | `is_prod = false`,venue 2 UID `"9001"`(同一 VenueId) | `[{DuplicateVenueUid, 14, 3}]` |
| REF-032.5 | `is_prod = true`,venue 5 UID `"1001"`(与账户 1 的 Binance UID 同串,不同交易所) | 无 | `Ok`;`warnings()` 为空 |
| REF-032.6 | `is_prod = false`、`uid_policy = Unverifiable`,venue 2 UID `""` | 同左,venue 1 UID `""` | `Ok`;`warnings()` 为空(空串不比较) |

### REF-033 Live 账户每个 venue 都要有 UID 行,空串只允许在 Unverifiable 账户

- 规则:[实现说明](README.md#实现说明)、[数据结构](README.md#数据结构)
- 级别:table
- 里程碑:M1
- 前置:F0 的账户 1
- 实现:`crates/hft-refdata/tests/integrity.rs::ref_033_missing_venue_uid`

| 行 | 改动 | 期望 |
|---|---|---|
| REF-033.1 | 删除 AccountVenues `(1, 6, "2001")` | `Err`,`errors` 恰一条且 `code == MissingVenueUid` |
| REF-033.2 | `(1, 6)` 的 UID 改为 `""`,`uid_policy = Verified` | `Err`,`errors` 恰一条且 `code == MissingVenueUid` |
| REF-033.3 | `(1, 6)` 的 UID 改为 `""`,`is_prod = false`、`uid_policy = Unverifiable` | `Ok`;`venue_uid(1, 6) == None`、`venue_uid(1, 1) == Some("1001")` |

### REF-034 uid_policy = Unverifiable 只允许非正式的 Live 账户

- 规则:[数据结构](README.md#数据结构)、[实现说明](README.md#实现说明)
- 级别:table
- 里程碑:M1
- 前置:F0
- 实现:`crates/hft-refdata/tests/integrity.rs::ref_034_uid_policy_prod`

| 行 | 改动 | 期望 |
|---|---|---|
| REF-034.1 | 账户 1(`is_prod = true`)`uid_policy = Unverifiable` | `[{UidPolicyProd, 4, 0}]` |
| REF-034.2 | 账户 2(Sim)`uid_policy = Unverifiable` | `[{UidPolicyProd, 4, 1}]` |
| REF-034.3 | 账户 1 `is_prod = false`、`uid_policy = Unverifiable`(UID 保持非空) | `Ok`;`account(1).uid_policy == Unverifiable` |
| REF-034.4 | F0 不写 `uid_policy`(构造器缺省) | `Ok`;`account(1).uid_policy == Verified`(1) |

### REF-035 每个可交易 inst 对其 venue 上启用的每个账户都有费率

- 规则:[实现说明](README.md#实现说明)、[公开 API](README.md#公开-api)
- 级别:table
- 里程碑:M1
- 前置:F0(账户 1 在 venue 6 用档 0,Sim 账户 2 在 venue 6 用档 1;venue 6 上只有 inst 4)
- 实现:`crates/hft-refdata/tests/integrity.rs::ref_035_missing_fee`

| 行 | 改动 | 期望 |
|---|---|---|
| REF-035.1 | 删除 Fees `(6, 0)` | `Err`,`errors` 恰一条且 `code == MissingFee` |
| REF-035.2 | 删除 Fees `(6, 0)`,追加 FeeOverrides `(1, 6, 全部 inst)` | `Ok` |
| REF-035.3 | 删除 Fees `(6, 0)`,追加 FeeOverrides `(1, 6, inst 4)` | `Ok` |
| REF-035.4 | 删除 Fees `(6, 0)`,inst 4 `status = Halted` | `Ok` |
| REF-035.5 | 删除 Fees `(6, 0)`,inst 4 `status = ReduceOnly` | `Err`,`code == MissingFee` |
| REF-035.6 | 删除 Fees `(6, 1)`(Sim 账户的档) | `Err`,`code == MissingFee` |
| REF-035.7 | 账户 2 的 `fee_tier = 2`(无该档的任何行) | `Err`,`code == MissingFee` |

### REF-036 每个 venue 的启用 Live 账户不超过 12

- 规则:[实现说明](README.md#实现说明)
- 级别:table
- 里程碑:M1
- 前置:F0 去掉账户 1 的 venue 6,改为在 venue 6 上追加 N 个 Live 账户(`label` 为 `"l<k>"`、UID 为 `"u<k>"`、`bound_instance = 0`、`is_prod = true`)
- 实现:`crates/hft-refdata/tests/integrity.rs::ref_036_accounts_per_venue`

| 行 | 取值 | 期望 |
|---|---|---|
| REF-036.1 | N = 11 | `Ok` |
| REF-036.2 | N = 12 | `Ok`;`venue_account_ix(6, 最大 AccountId) == Some(11)` |
| REF-036.3 | N = 13 | `Err`,`code == TooManyAccountsPerVenue` |
| REF-036.4 | N = 12,另追加 Sim 账户 `"sim-b"`(venue 6,带 SimAccounts 与 SimBalances) | `Ok`(Sim 不计;F0 的 Sim 账户 2 在各行都位于 venue 6) |

### REF-037 Sim 账户的完整性

- 规则:[实现说明](README.md#实现说明)、[数据结构](README.md#数据结构)、[LoadErrCode 码表](README.md#loaderrcode-码表)
- 级别:table
- 里程碑:M1
- 前置:F0(账户 2 为 Sim)
- 实现:`crates/hft-refdata/tests/integrity.rs::ref_037_sim_account_rules`

| 行 | 改动 | 期望 |
|---|---|---|
| REF-037.1 | 账户 2 `is_prod = true` | `[{SimIsProd, 4, 1}]` |
| REF-037.2 | 追加 AccountVenues `(2, 1, "5001")` | `[{SimHasUid, 14, 2}]` |
| REF-037.3 | 删除 SimAccounts 行 | `Err`,`errors` 恰一条且 `code == SimCfg` |
| REF-037.4 | SimAccounts 账户 2 两行 | `[{SimCfg, 16, 1}]` |
| REF-037.5 | 删除全部 SimBalances 行 | `Err`,`errors` 恰一条且 `code == SimCfg` |
| REF-037.6 | SimBalances 行 0 的 venue = 5(不在账户 2 的 `venues`) | `[{SimCfg, 17, 0}]` |
| REF-037.7 | 追加 SimAccounts `(1, …)`(Live 账户) | `[{SimCfg, 16, 0}]` |
| REF-037.8 | 追加 SimBalances `(1, 1, USDT, 1)`(Live 账户) | `[{SimCfg, 17, 0}]` |

### REF-038 Sim 账户的模型下标不由本 crate 校验

- 规则:[数据结构](README.md#数据结构)
- 级别:unit
- 里程碑:M1
- 前置:F0 的 SimAccounts 行 `latency_model = 65 535`、`margin_model = 65 535`
- 步骤:
  1. `rd = load(&b).unwrap()`
- 期望:`account(2).kind == Sim(SimAccountCfg{ fee_tier: 1, latency_model: 65535, margin_model: 65535, initial_balances: [(1, USDT, 10 000), (6, USDT, 10 000)] })`(范围由 hft-core 启动时校验)
- 实现:`crates/hft-refdata/tests/integrity.rs::ref_038_sim_model_index_unchecked`

### REF-039 与当前版本 SHA-256 相同时只返回 AlreadyActive

- 规则:[实现说明](README.md#实现说明)、[LoadErrCode 码表](README.md#loaderrcode-码表)
- 级别:unit
- 里程碑:M1
- 前置:`cur = load(&f0())`
- 步骤:
  1. `r = RefData::load(&f0(), Some(&cur)).unwrap_err()`
- 期望:`r.errors == [{AlreadyActive, 0, 0}]`;`r.code() == AlreadyActive`(30)
- 实现:`crates/hft-refdata/tests/version.rs::ref_039_already_active`

### REF-040 跨版本:版本号必须递增,current = None 跳过本步

- 规则:[实现说明](README.md#实现说明)
- 级别:table
- 里程碑:M1
- 前置:`cur` = F0(v10);新 blob = F0 改 inst 1 的 `max_leverage = 100` 再改版本号
- 实现:`crates/hft-refdata/tests/version.rs::ref_040_version_increasing`

| 行 | 新版本 | `current` | 期望 |
|---|---|---|---|
| REF-040.1 | 10 | `cur` | `[{VersionNotIncreasing, 0, 0}]` |
| REF-040.2 | 9 | `cur` | `[{VersionNotIncreasing, 0, 0}]` |
| REF-040.3 | 11 | `cur` | `Ok` |
| REF-040.4 | 1 | `None` | `Ok` |

### REF-041 跨版本:inst 只能追加或改状态,不能删行或换身份

- 规则:[实现说明](README.md#实现说明)
- 级别:table
- 里程碑:M1
- 前置:`cur` = F0(v10);新 blob v11 = F0 加下列改动
- 实现:`crates/hft-refdata/tests/version.rs::ref_041_inst_cross_version`

| 行 | 改动 | 期望 |
|---|---|---|
| REF-041.1 | 删除 inst 5 及其引用 | `[{InstRemoved, 3, 4}]` |
| REF-041.2 | inst 5 的符号改为 `"1000PEPEUSDC"` | `[{InstRemoved, 3, 4}]` |
| REF-041.3 | inst 5 的 venue 改为 5(`OKX_SWAP`) | `[{InstRemoved, 3, 4}]` |
| REF-041.4 | inst 2 的 asset 改为 ETH | `[{InstRemoved, 3, 1}]` |
| REF-041.5 | inst 5 `status = Delisted` | `Ok` |
| REF-041.6 | 追加 inst 6 `ETHUSDT`@1 | `Ok` |

### REF-042 跨版本:资产名、指数与合约乘数不变

- 规则:[实现说明](README.md#实现说明)、[LoadErrCode 码表](README.md#loaderrcode-码表)
- 级别:table
- 里程碑:M1
- 前置:`cur` = F0(v10);新 blob v11
- 实现:`crates/hft-refdata/tests/version.rs::ref_042_asset_and_multiplier`

| 行 | 改动 | 期望 |
|---|---|---|
| REF-042.1 | BTC 的名字改为 `"XBT"` | `[{AssetChanged, 1, 0}]` |
| REF-042.2 | ETH `px_exp` −8 → −9 | `[{AssetChanged, 1, 2}]` |
| REF-042.3 | 删除资产 5 与 inst 5 | `[{InstRemoved, 3, 4}, {AssetChanged, 1, 4}]`(inst 规则在资产规则之前) |
| REF-042.4 | inst 5 `qty_unit_shift` 3 → 4,`lot_str = min_qty_str = "0.1"` | `[{MultiplierChanged, 3, 4}]` |
| REF-042.5 | inst 5 `px_unit_shift` 3 → 4,`tick_str = "0.000001"` | `[{MultiplierChanged, 3, 4}]` |

### REF-043 跨版本:账户身份与 UID 不变,新增 venue 不算变化

- 规则:[实现说明](README.md#实现说明)
- 级别:table
- 里程碑:M1
- 前置:`cur` = F0(v10);新 blob v11
- 实现:`crates/hft-refdata/tests/version.rs::ref_043_account_cross_version`

| 行 | 改动 | 期望 |
|---|---|---|
| REF-043.1 | 账户 1 `is_prod` true → false | `[{AccountKindChanged, 4, 0}]` |
| REF-043.2 | 账户 2 Sim → Live(`is_prod = false`,补 AccountVenues,删 SimAccounts/SimBalances) | `[{AccountKindChanged, 4, 1}]` |
| REF-043.3 | 删除账户 2 及其 SimAccounts、SimBalances 行(实例 2 改为未绑定) | `[{AccountKindChanged, 4, 1}]` |
| REF-043.4 | `(1, 6)` 的 UID `"2001"` → `"2002"` | `[{UidChanged, 14, 1}]` |
| REF-043.5 | 账户 1 `venues` {1, 6} → {1, 5},AccountVenues `(1, 5, "3001")` 替换 `(1, 6, …)` | `Ok` |
| REF-043.6 | 账户 1 `bound_instance` 1 → 3 | `Ok`(由 `check_rebind` 另行约束) |

### REF-044 跨版本:实例 id 永久绑定 (kind_id, name)

- 规则:[实现说明](README.md#实现说明)、[数据结构](README.md#数据结构)
- 级别:table
- 里程碑:M1
- 前置:`cur` = F0(v10);新 blob v11
- 实现:`crates/hft-refdata/tests/version.rs::ref_044_instance_cross_version`

| 行 | 改动 | 期望 |
|---|---|---|
| REF-044.1 | 实例 1 的 `kind_id` 0xFF00 → 0xFF01 | `[{InstanceChanged, 15, 0}]` |
| REF-044.2 | 实例 1 的名字 `"pc-main"` → `"pc-main-2"` | `[{InstanceChanged, 15, 0}]` |
| REF-044.3 | 删除实例 3(未绑定) | `[{InstanceChanged, 15, 2}]` |
| REF-044.4 | 追加实例 4 `"probe-b"` | `Ok` |

### REF-045 符号表按原样字节查找,大小写敏感,别名映射到同一 inst

- 规则:[公开 API](README.md#公开-api)、[实现说明](README.md#实现说明)
- 级别:table
- 里程碑:M1
- 前置:`rd = load(&f0())`
- 实现:`crates/hft-refdata/tests/symbols.rs::ref_045_symbol_get`

| 行 | 调用 | 期望 |
|---|---|---|
| REF-045.1 | `symbols(1).get(b"BTCUSDT")` | `Some(InstId(1))` |
| REF-045.2 | `symbols(1).get(b"1000PEPEUSDT")` | `Some(InstId(5))` |
| REF-045.3 | `symbols(1).get(b"btcusdt")` | `None` |
| REF-045.4 | `symbols(1).get(b"BTCUSD")`(前缀) | `None` |
| REF-045.5 | `symbols(1).get(b"BTCUSDTX")` | `None` |
| REF-045.6 | `symbols(1).get(b"")` | `None` |
| REF-045.7 | `symbols(2).get(b"BTC_USDT")`(别名) | `Some(InstId(2))` |
| REF-045.8 | `symbols(1).get(b"BTC_USDT")`(别名属于 venue 2) | `None` |
| REF-045.9 | `symbols(3).get(b"BTCUSDT")`(未登记的 venue) | `None` |
| REF-045.10 | `symbols(1).get(&[b'A'; 40])`(超过 27 字节) | `None` |

### REF-046 数值 id 查找,0 恒不命中

- 规则:[公开 API](README.md#公开-api)、[实现说明](README.md#实现说明)
- 级别:table
- 里程碑:M1
- 前置:`rd = load(&f0())`
- 实现:`crates/hft-refdata/tests/symbols.rs::ref_046_symbol_get_num`

| 行 | 调用 | 期望 |
|---|---|---|
| REF-046.1 | `symbols(5).get_num(10459)` | `Some(InstId(3))` |
| REF-046.2 | `symbols(6).get_num(20001)` | `Some(InstId(4))` |
| REF-046.3 | `symbols(6).get_num(10459)` | `None` |
| REF-046.4 | `symbols(1).get_num(0)`(inst 1、5 的 `venue_num_id` 都是 0) | `None` |
| REF-046.5 | `symbols(5).get_num(u64::MAX)` | `None` |

### REF-047 符号表在 8 192 条以内的任意集合上查全、查准,两条路径结果相同

- 规则:[实现说明](README.md#实现说明)
- 级别:prop
- 里程碑:M1
- 性质:对随机生成的符号集合 S(同一 venue),`get(s) == Some(id(s))` 对每个 s ∈ S 成立;对随机非成员 t,`get(t) == None`;有序数组二分路径与 FxHash 前置表路径对每次查询返回相同结果;每次二分的有序比较次数 ≤ max(1, ⌈log₂ |S|⌉)(8 192 条时 ≤ 13;测试构建的比较计数器)
- 生成:|S| ∈ 1..=8 192;符号长度 1..=27,字节取自 `[A-Z0-9_\-.]`;非成员 1 000 个,其中一半是成员改一字节、一半随机
- 参考:`std::collections::BTreeMap<Vec<u8>, InstId>`
- 例数:PR 256;每夜 4 096
- 实现:`crates/hft-refdata/tests/prop_symbols.rs::ref_047_symbol_table_matches_btreemap`

### REF-048 FxHash 前置表容量为 next_pow2(2n) 且构建确定

- 规则:[实现说明](README.md#实现说明)
- 级别:table
- 里程碑:M1
- 前置:单个 venue 上 n 个符号(`"S<k>"`);构建两次
- 实现:`crates/hft-refdata/tests/symbols.rs::ref_048_front_table_capacity`

| 行 | n | 期望 |
|---|---|---|
| REF-048.1 | 1 | 容量 2;两次构建的前置表字节相同 |
| REF-048.2 | 3 | 容量 8 |
| REF-048.3 | 4 096 | 容量 8 192 |
| REF-048.4 | 5 000 | 容量 16 384 |

### REF-049 prewarm 读全部条目并返回确定的校验和

- 规则:[公开 API](README.md#公开-api)
- 级别:unit
- 里程碑:M1
- 前置:`rd1 = load(&f0())`、`rd2 = load(&f0())`
- 步骤:
  1. `a = rd1.symbols(1).prewarm()`;`b = rd1.symbols(1).prewarm()`;`c = rd2.symbols(1).prewarm()`
  2. `d = rd1.symbols(6).prewarm()`
- 期望:`a == b == c`;`a ≠ d`(两个 venue 的条目不同);调用不分配(计数分配器为 0)
- 实现:`crates/hft-refdata/tests/symbols.rs::ref_049_prewarm`

### REF-050 inst 越界或为 0 时返回 NONE 行

- 规则:[公开 API](README.md#公开-api)、[数据结构](README.md#数据结构)
- 级别:table
- 里程碑:M1
- 前置:`rd = load(&f0())`
- 实现:`crates/hft-refdata/tests/query.rs::ref_050_inst_lookup`

| 行 | 调用 | 期望 |
|---|---|---|
| REF-050.1 | `inst(InstId(1))` | `venue = 1`、`status = Trading`、`kind = Perp`、`flags = 0`、`(px_exp, qty_exp, px_wire_exp, qty_wire_exp) = (−8, −8, −8, −8)`、`asset = 1`、`quote = 2`、`book_window = 4096`、`tick = 10 000 000`、`lot = 100 000`、`min_qty = 100 000`、`max_qty` raw 10¹¹ |
| REF-050.2 | `inst(InstId(0))` | `status = Invalid`(0) |
| REF-050.3 | `inst(InstId(6))`(最大 id + 1) | `status = Invalid` |
| REF-050.4 | `inst(InstId(u32::MAX))` | `status = Invalid` |
| REF-050.5 | `inst_meta(InstId(5))` | `symbol = "1000PEPEUSDT"`、`px_unit_shift = 3`、`qty_unit_shift = 3`、`tick_str = "0.0000001"`、`lot_str = "1"`、`min_qty_str = "1"`、`funding_interval_s = 28 800`、`funding_anchor_s = 0`、`max_leverage = 125`、`delivery_utc_ns = 0` |

### REF-051 能力表、账户与资产、venue 的查询

- 规则:[公开 API](README.md#公开-api)、[交易所能力表(VenueCaps)](README.md#交易所能力表venuecaps)、[数据结构](README.md#数据结构)
- 级别:table
- 里程碑:M1
- 前置:`rd = load(&f0())`
- 实现:`crates/hft-refdata/tests/query.rs::ref_051_caps_account_asset_venue`

| 行 | 调用 | 期望 |
|---|---|---|
| REF-051.1 | `caps(1)` | 等于 F0 的永续行(`flags == 7`、`funding_settle = PerSymbolPush`、`ignorable_user_events == ["TRADE_LITE"]`、`liq_marker_arg == "autoclose-"`) |
| REF-051.2 | `caps(3)`(未登记) | NONE 行:`flags == 0`、`dms = None`、四个时长 0 |
| REF-051.3 | `caps(VenueId(0))` | NONE 行 |
| REF-051.4 | `account(1)` | `kind = Live`、`bound_instance = 1`、`is_prod = true`、`venues` 位图恰含 bit 1 与 bit 6、`label = "acct-a"`、`uid_policy = Verified` |
| REF-051.5 | `account(AccountId(0))` | NONE 行:`bound_instance = 0`、`venues = 0`、`is_prod = false` |
| REF-051.6 | `account(AccountId(3))`(越界) | NONE 行 |
| REF-051.7 | `asset(5)` | `name = "PEPE"`、`class = Crypto`、`exp = (−12, 0)`、`px_max` raw 10¹² |
| REF-051.8 | `venue(6)` | `exchange = Okx`、`product = Spot`、`stale_guard = ExpireAt`、`client_id_max_len = 32`、`ws_max_conn_age_s = 86 400`、`ts_unit = Ms` |

### REF-052 费率解析:按 inst 覆盖 > 按 venue 覆盖 > (venue, 档)

- 规则:[公开 API](README.md#公开-api)、[数据结构](README.md#数据结构)
- 级别:table
- 里程碑:M1
- 前置:F0 加 inst 6 `ETH-USDT`@6(Spot ETH/USDT,步长同 inst 4);各行再加所列 FeeOverrides
- 实现:`crates/hft-refdata/tests/fees.rs::ref_052_fee_precedence`

| 行 | FeeOverrides | 调用 | 期望 |
|---|---|---|---|
| REF-052.1 | 无 | `fees(1, 4)` | `{maker: 0.0008, taker: 0.001}`(Fees `(6, 0)`) |
| REF-052.2 | 无 | `fees(2, 4)`(Sim,档 1) | `{0.0006, 0.0008}` |
| REF-052.3 | `(1, 6, 全部) = {0.0003, 0.0007}` | `fees(1, 4)` | `{0.0003, 0.0007}` |
| REF-052.4 | `(1, 6, 全部) = {0.0003, 0.0007}` 与 `(1, 6, inst 4) = {0.0004, 0.0009}` | `fees(1, 4)` | `{0.0004, 0.0009}` |
| REF-052.5 | 同上 | `fees(1, 6)` | `{0.0003, 0.0007}` |
| REF-052.6 | `(1, 6, inst 4) = {−0.00005, 0.0005}` | `fees(1, 4)` | `maker = −0.00005`(返佣原样保留) |
| REF-052.7 | `(1, 6, 全部) = {0.0003, 0.0007}` | `fees(2, 4)`(覆盖属于账户 1) | `{0.0006, 0.0008}` |

### REF-053 UID、实例与绑定的查询

- 规则:[公开 API](README.md#公开-api)、[数据结构](README.md#数据结构)
- 级别:table
- 里程碑:M1
- 前置:`rd = load(&f0())`
- 实现:`crates/hft-refdata/tests/query.rs::ref_053_uid_instance_binding`

| 行 | 调用 | 期望 |
|---|---|---|
| REF-053.1 | `venue_uid(1, 1)` | `Some("1001")` |
| REF-053.2 | `venue_uid(1, 6)` | `Some("2001")` |
| REF-053.3 | `venue_uid(1, 5)`(未启用) | `None` |
| REF-053.4 | `venue_uid(2, 1)`(Sim) | `None` |
| REF-053.5 | `instance(1)` | `Some({kind_id: 0xFF00, name: "pc-main"})` |
| REF-053.6 | `instance(9)` | `None` |
| REF-053.7 | `account_of(1)`、`account_of(2)` | `Some(1)`、`Some(2)` |
| REF-053.8 | `account_of(3)`(未绑定) | `None` |

### REF-054 按资产、按 venue 的 inst 列表按 InstId 升序;限速与端点按 venue 与环境

- 规则:[公开 API](README.md#公开-api)
- 级别:table
- 里程碑:M1
- 前置:`rd = load(&f0())`
- 实现:`crates/hft-refdata/tests/query.rs::ref_054_lists`

| 行 | 调用 | 期望 |
|---|---|---|
| REF-054.1 | `insts_of_asset(BTC)` | `[1, 2, 3, 4]` |
| REF-054.2 | `insts_of_asset(ETH)` | `[]` |
| REF-054.3 | `insts_of_venue(1)` | `[1, 5]` |
| REF-054.4 | `insts_of_venue(3)` | `[]` |
| REF-054.5 | `rate_limits(1)` | 恰 2 行:`{Account, Orders, 300, 10 000, 1}` 与 `{Ip, Weight, 2 400, 60 000, 1}` |
| REF-054.6 | `rate_limits(2)` | `[]` |
| REF-054.7 | `endpoints(1, Prod)` | 恰 2 行(`OrderWs`、`MdPublic`) |
| REF-054.8 | `endpoints(1, Testnet)`、`endpoints(1, Demo)` | 1 行(`MdPublic`)、`[]` |

### REF-055 维护窗口为左闭右开,inst 窗口优先于全 venue 窗口

- 规则:[公开 API](README.md#公开-api)
- 级别:table
- 里程碑:M1
- 前置:F0 加 Maint:`W1 = (venue 6, inst 0, [1 000, 2 000), VenueMaintenance)`、`W2 = (venue 6, inst 4, [1 500, 2 500), Delisting)`、`W3 = (venue 6, inst 4, [1 600, 3 000), MarketClosed)`;时刻单位 ns
- 实现:`crates/hft-refdata/tests/query.rs::ref_055_maintenance`

| 行 | 调用 | 期望 |
|---|---|---|
| REF-055.1 | `maintenance(4, 999)` | `None` |
| REF-055.2 | `maintenance(4, 1 000)` | `Some(W1)` |
| REF-055.3 | `maintenance(4, 1 499)` | `Some(W1)` |
| REF-055.4 | `maintenance(4, 1 500)` | `Some(W2)` |
| REF-055.5 | `maintenance(4, 1 700)`(W2、W3 同为 inst 级) | `Some(W2)`(start 较小) |
| REF-055.6 | `maintenance(4, 2 500)` | `Some(W3)` |
| REF-055.7 | `maintenance(4, 2 999)` | `Some(W3)` |
| REF-055.8 | `maintenance(4, 3 000)` | `None` |
| REF-055.9 | `maintenance(1, 1 500)`(venue 1 无窗口) | `None` |

### REF-056 链上代币查表

- 规则:[公开 API](README.md#公开-api)、[数据结构](README.md#数据结构)
- 级别:table
- 里程碑:M1
- 前置:F0 加 Venues `CHAIN_BSC`(32)与 Tokens `(chain 32, USDT, address = [0x55; 20] 右补零到 32 B, decimals 18)`
- 实现:`crates/hft-refdata/tests/query.rs::ref_056_token`

| 行 | 调用 | 期望 |
|---|---|---|
| REF-056.1 | `token(32, USDT)` | `Some({chain: 32, asset: USDT, decimals: 18, address 与输入逐字节相同})` |
| REF-056.2 | `token(32, BTC)` | `None` |
| REF-056.3 | `token(2, USDT)` | `None` |

### REF-057 venue 账户稠密序号按 AccountId 升序,只给 Live 账户

- 规则:[公开 API](README.md#公开-api)
- 级别:table
- 里程碑:M1
- 前置:F0 加 Live 账户 3(venue 6,UID `"2003"`)、Sim 账户 4(venue 6,带 SimAccounts 与 SimBalances)、Live 账户 5(venue 6,UID `"2005"`),都未绑定
- 实现:`crates/hft-refdata/tests/query.rs::ref_057_venue_account_ix`

| 行 | 调用 | 期望 |
|---|---|---|
| REF-057.1 | `venue_account_ix(6, 1)` | `Some(0)` |
| REF-057.2 | `venue_account_ix(6, 3)` | `Some(1)` |
| REF-057.3 | `venue_account_ix(6, 5)` | `Some(2)` |
| REF-057.4 | `venue_account_ix(6, 4)`、`(6, 2)`(Sim) | `None`、`None` |
| REF-057.5 | `venue_account_ix(1, 3)`(账户 3 未启用 venue 1) | `None` |
| REF-057.6 | `venue_account_ix(1, 1)` | `Some(0)` |

### REF-058 check_binding 的三种结果

- 规则:[公开 API](README.md#公开-api)、[实现说明](README.md#实现说明)
- 级别:table
- 里程碑:M1
- 前置:`rd = load(&f0())`;另一份 `rd0` = F0 改账户 1 `bound_instance = 0`
- 实现:`crates/hft-refdata/tests/params.rs::ref_058_check_binding`

| 行 | 调用 | 期望 |
|---|---|---|
| REF-058.1 | `rd.check_binding(1, 1, 0xFF00)` | `Ok(())` |
| REF-058.2 | `rd.check_binding(1, 3, 0xFF01)`(账户 1 绑定的是实例 1) | `Err(code = AccountBound)` |
| REF-058.3 | `rd0.check_binding(1, 1, 0xFF00)`(账户未绑定) | `Err(code = AccountBound)` |
| REF-058.4 | `rd.check_binding(1, 1, 0xFF01)` | `Err(code = KindMismatch)` |
| REF-058.5 | `rd.check_binding(9, 1, 0xFF00)`(不存在的账户) | `Err(code = AccountBound)` |

### REF-059 check_group 通过时返回报价资产

- 规则:[公开 API](README.md#公开-api)、[实现说明](README.md#实现说明)
- 级别:unit
- 里程碑:M1
- 前置:`rd = load(&f0())`;`g = GroupSpecView{asset: BTC, legs: [{inst 1, venue 1}, {inst 4, venue 6}]}`;`g1` 只含第一条腿
- 步骤:
  1. `rd.check_group(1, &g)`
  2. `rd.check_group(1, &g1)`
- 期望:两次都 `Ok(AssetId(2))`(USDT)
- 实现:`crates/hft-refdata/tests/params.rs::ref_059_check_group_ok`

### REF-060 check_group 的每个拒绝原因

- 规则:[公开 API](README.md#公开-api)、[实现说明](README.md#实现说明)、[数据结构](README.md#数据结构)
- 级别:table
- 里程碑:M1
- 前置:F0(必要时改 inst 4 的状态);另加 inst 6 `BTC-USDC`@6(Spot BTC/USDC,步长同 inst 4,Fees 已覆盖);`L(i, v)` 表示 `{inst i, venue v}`;账户 1
- 实现:`crates/hft-refdata/tests/params.rs::ref_060_check_group_rejects`

| 行 | `GroupSpecView` | inst 4 状态 | 期望 |
|---|---|---|---|
| REF-060.1 | BTC,[L(1, 1), L(99, 6)] | Trading | `Err(LegNotTradable)` |
| REF-060.2 | BTC,[L(1, 1), L(4, 6)] | Halted | `Err(LegNotTradable)` |
| REF-060.3 | 同上 | Delisted | `Err(LegNotTradable)` |
| REF-060.4 | 同上 | PreTrading | `Err(LegNotTradable)` |
| REF-060.5 | 同上 | Maintenance | `Err(LegNotTradable)` |
| REF-060.6 | 同上 | ReduceOnly | `Ok(USDT)` |
| REF-060.7 | BTC,`[L(1, 1), L(4, 5)]`(腿的 venue 与 inst 不符) | Trading | `Err(LegVenueNotInAccount)` |
| REF-060.8 | BTC,`[L(1, 1), L(3, 5)]`(venue 5 不在账户 1) | — | `Err(LegVenueNotInAccount)` |
| REF-060.9 | ETH,[L(1, 1), L(4, 6)] | Trading | `Err(LegAssetMismatch)` |
| REF-060.10 | BTC,`[L(1, 1), L(6, 6)]`(USDT 与 USDC) | — | `Err(QuoteMismatch)` |
| REF-060.11 | BTC,`[L(1, 1), L(1, 1)]`(两腿同在 Binance) | — | `Err(SameExchangeLegs)` |
| REF-060.12 | BTC,`[L(99, 1), L(4, 5)]`(两条腿各有错) | Trading | `Err(LegNotTradable)`(按腿顺序与规则顺序取第一个) |
| REF-060.13 | ETH,`[L(1, 1), L(1, 1)]`(资产不符与同所并存) | — | `Err(LegAssetMismatch)`(逐腿检查在两腿之间的检查之前) |

### REF-061 check_assembly 全部通过的基线

- 规则:[实现说明](README.md#实现说明)、[公开 API](README.md#公开-api)
- 级别:unit
- 里程碑:M1
- 前置:`rd = load(&f0())`;`ec = EngineConfig::load(&e0(), None)`;`A0`
- 步骤:
  1. `check_assembly(&rd, &ec, &A0)`
- 期望:`Ok(())`
- 实现:`crates/hft-refdata/tests/assembly.rs::ref_061_assembly_ok`

### REF-062 check_assembly 的每个拒绝原因与收集顺序

- 规则:[实现说明](README.md#实现说明)、[交易所能力表(VenueCaps)](README.md#交易所能力表venuecaps)、[LoadErrCode 码表](README.md#loaderrcode-码表)、[生命周期(变更 = 重启)](README.md#生命周期变更--重启)
- 级别:table
- 里程碑:M1
- 前置:F0、E0、A0;每行只改所列项
- 实现:`crates/hft-refdata/tests/assembly.rs::ref_062_assembly_rejects`

| 行 | 改动 | 期望 |
|---|---|---|
| REF-062.1 | `A0.host = C16`(E0 为 c8) | `[{HostClassMismatch, 0, 0}]` |
| REF-062.2 | `A0.max_universe_insts = 4`(F0 有 5 个 inst) | `Err`,`errors` 恰一条且 `code == ValueRange` |
| REF-062.3 | `A0.max_universe_insts = 5` | `Ok` |
| REF-062.4 | `A0.protocols` 去掉 venue 6 | `[{UnknownVenue, 2, 3}]` |
| REF-062.5 | F0 的 Venues 加 `GATE_USDT_FUT`(3,无 inst),`A0` 不变 | `Ok`(无 inst 的 venue 不要求协议) |
| REF-062.6 | F0 加 inst 6 `BTC_USDT`@`GATE_SPOT`(4,含 Venues、VenueCaps、Fees 行),`A0` 不变 | `[{UnknownVenue, 2, 2}]` |
| REF-062.7 | venue 1 协议的 `market = Spot` | `[{CapsMarketMismatch, 13, 0}]` |
| REF-062.8 | venue 1 协议的 `caps.flags = POST_ONLY\|IOC`(6),F0 行为 7 | `[{CapsExceedProtocol, 13, 0}]` |
| REF-062.9 | venue 2 协议的 `caps.flags = AMEND\|POST_ONLY\|IOC\|FOK`(15),F0 行为 6 | `Ok`(子集) |
| REF-062.10 | venue 5 协议的 `caps.dms = CountdownAccount`,F0 行为 `None` | `[{CapsExceedProtocol, 13, 2}]` |
| REF-062.11 | venue 1 协议的 `stale_guard = ExpireAt`,F0 为 RecvWindow | `[{StaleGuardMismatch, 2, 0}]` |
| REF-062.12 | `A0.kinds = [0xFF00]`(实例 3 的 kind 0xFF01 未编译进来) | `[{UnknownStrategyKind, 15, 2}]` |
| REF-062.13 | 同时 `host = C16` 与 `kinds = [0xFF00]` | `[{HostClassMismatch, 0, 0}, {UnknownStrategyKind, 15, 2}]` |

### REF-063 max_universe_insts 的缺省值与边界

- 规则:[配置项](README.md#配置项)
- 级别:table
- 里程碑:M1
- 前置:生成器 `universe(n)`:F0 的 venue 1 上补足到 n 个 inst(符号 `"T<k>USDT"`,步长同 inst 1)
- 实现:`crates/hft-refdata/tests/assembly.rs::ref_063_max_universe_default`

| 行 | 输入 | 期望 |
|---|---|---|
| REF-063.1 | `DEFAULT_MAX_UNIVERSE_INSTS` | 8 192 |
| REF-063.2 | `universe(8192)`,`max_universe_insts = DEFAULT_MAX_UNIVERSE_INSTS` | `check_assembly` → `Ok` |
| REF-063.3 | `universe(8193)`,同上 | `Err`,`errors` 恰一条且 `code == ValueRange` |
| REF-063.4 | `universe(8191)`,同上 | `Ok` |

### REF-064 绑定变更时账户必须空闲

- 规则:[实现说明](README.md#实现说明)、[公开 API](README.md#公开-api)
- 级别:table
- 里程碑:M1
- 前置:`prev = load(F0)`;`new` = F0 v11 改所列绑定;`busy(a)` 由用例给出
- 实现:`crates/hft-refdata/tests/rebind.rs::ref_064_check_rebind`

| 行 | 新绑定 | `busy` | 期望 |
|---|---|---|---|
| REF-064.1 | 账户 1:1 → 3 | `busy(1) = true` | `[{BindingChangedWithPosition, 4, 1}]`(row = AccountId) |
| REF-064.2 | 账户 1:1 → 3 | 全部 false | `Ok` |
| REF-064.3 | 不变 | 全部 true | `Ok` |
| REF-064.4 | 账户 1:1 → 0 | `busy(1) = true` | `[{BindingChangedWithPosition, 4, 1}]` |
| REF-064.5 | prev 中账户 1 未绑定,new 中 0 → 1 | `busy(1) = true` | `[{BindingChangedWithPosition, 4, 1}]` |
| REF-064.6 | 账户 1:1 → 3,账户 2:2 → 1 | 全部 true | `[{BindingChangedWithPosition, 4, 1}, {BindingChangedWithPosition, 4, 2}]` |

### REF-065 ref_inst 必须存在且可交易

- 规则:[实现说明](README.md#实现说明)、[LoadErrCode 码表](README.md#loaderrcode-码表)
- 级别:table
- 里程碑:M1
- 前置:F0;inst 2 的状态按行设定
- 实现:`crates/hft-refdata/tests/rebind.rs::ref_065_check_ref_inst`

| 行 | 输入 | 期望 |
|---|---|---|
| REF-065.1 | `check_ref_inst(2)`,Trading | `Ok` |
| REF-065.2 | 同上,ReduceOnly | `Ok` |
| REF-065.3 | 同上,Halted | `Err(RefInstNotSubscribed)` |
| REF-065.4 | 同上,Delisted | `Err(RefInstNotSubscribed)` |
| REF-065.5 | 同上,PreTrading | `Err(RefInstNotSubscribed)` |
| REF-065.6 | 同上,Maintenance | `Err(RefInstNotSubscribed)` |
| REF-065.7 | `check_ref_inst(99)` | `Err(RefInstNotSubscribed)` |
| REF-065.8 | `check_ref_inst(0)` | `Err(RefInstNotSubscribed)` |

### REF-066 EngineConfig 容器与主机类编码

- 规则:[实现说明](README.md#实现说明)、[公开 API](README.md#公开-api)
- 级别:table
- 里程碑:M1
- 前置:E0 字节上改 `_rsv[0]`(`host_class`)或头部
- 实现:`crates/hft-refdata/tests/engine.rs::ref_066_ecf1_container`

| 行 | 改动 | 期望 |
|---|---|---|
| REF-066.1 | `host_class` = 1、2、3、4、5、6 | 各 `Ok`,`host_class()` 依次为 dev2、c4、c8、metal、c16、c32 |
| REF-066.2 | `host_class = 0` | `[{Structure, 0, 0}]` |
| REF-066.3 | `host_class = 7` | `[{Structure, 0, 0}]` |
| REF-066.4 | 把 F0(magic `HFTRDB01`)交给 `EngineConfig::load` | `[{Structure, 0, 0}]` |
| REF-066.5 | `format = 2` | `[{Structure, 0, 0}]` |
| REF-066.6 | 追加段 9 不带 OPTIONAL | `[{Structure, 9, 0}]` |
| REF-066.7 | 追加段 9 带 OPTIONAL | `Ok`;`EngineSection` 无段 9,段 1–8 的视图与 E0 相同 |

### REF-067 段视图原样给出行版本、行大小、行数与字节

- 规则:[公开 API](README.md#公开-api)、[实现说明](README.md#实现说明)
- 级别:table
- 里程碑:M1
- 前置:E0;段 1 写 3 行、`row_ver = 2`、`row_size = 40`,字节为 0..120 递增;E0 去掉段 6
- 实现:`crates/hft-refdata/tests/engine.rs::ref_067_section_view`

| 行 | 调用 | 期望 |
|---|---|---|
| REF-067.1 | `section(Oms)` | `Some({row_ver: 2, row_size: 40, count: 3, bytes: [0, 1, …, 119]})` |
| REF-067.2 | `section(Conn)` | `None` |
| REF-067.3 | `EngineSection` 码值 | Oms 1、Ledger 2、Capacities 3、Rings 4、Sequencer 5、Conn 6、Exec 7、Assembly 8 |

### REF-068 validate 对每个存在的段调用同段的验证器

- 规则:[实现说明](README.md#实现说明)、[公开 API](README.md#公开-api)
- 级别:table
- 里程碑:M1
- 前置:`rd = load(F0)`;`ec = load(E0)`;记录型验证器把被调用的段号追加进日志,返回值按行设定
- 实现:`crates/hft-refdata/tests/engine.rs::ref_068_validate`

| 行 | 验证器 | 期望 |
|---|---|---|
| REF-068.1 | 段 1–8 各一个,全部 `Ok` | `Ok`;日志 `[1, 2, 3, 4, 5, 6, 7, 8]`(升序) |
| REF-068.2 | 缺段 5 的验证器 | `[{NoValidator, 5, 0}]`;其余 7 个照常调用 |
| REF-068.3 | E0 去掉段 6,不提供段 6 验证器 | `Ok` |
| REF-068.4 | 段 2 的验证器返回 `{RefInstNotSubscribed, 2, 3}` | `[{RefInstNotSubscribed, 2, 3}]` |
| REF-068.5 | 缺段 5 且段 2 返回错误 | `[{RefInstNotSubscribed, 2, 3}, {NoValidator, 5, 0}]`(按段 id 升序) |

### REF-069 EngineConfig 的版本规则,容量缩小不在加载时拒绝

- 规则:[实现说明](README.md#实现说明)
- 级别:table
- 里程碑:M1
- 前置:`cur = load(E0)`(v3);新 blob = E0 改段 3 的字节(表示更小的容量)再改版本
- 实现:`crates/hft-refdata/tests/engine.rs::ref_069_ecf1_versions`

| 行 | 输入 | 期望 |
|---|---|---|
| REF-069.1 | E0 原样,`current = cur` | `[{AlreadyActive, 0, 0}]` |
| REF-069.2 | v3、内容不同 | `[{VersionNotIncreasing, 0, 0}]` |
| REF-069.3 | v2 | `[{VersionNotIncreasing, 0, 0}]` |
| REF-069.4 | v4(容量更小) | `Ok`(恢复时由归属 crate 判 `CapacityShrink`) |
| REF-069.5 | v1,`current = None` | `Ok` |

### REF-070 EcfBuilder 往返与确定性

- 规则:[实现说明](README.md#实现说明)
- 级别:prop
- 里程碑:M1
- 性质:任意段集合经 `EcfBuilder` 编码后 `EngineConfig::load` 得到的每个段视图(`row_ver`、`row_size`、`count`、`bytes`)与输入逐字节相同;同一输入编码两次字节相同;编码结果 `total_len == len`、各段 8 B 对齐
- 生成:段 id ⊆ {1..7};每段 `row_ver` 1..=4、`row_size` 8..=256 且为 8 的倍数、`count` 0..=64、字节随机;`host_class` 1..=6
- 参考:输入本身
- 例数:PR 512;每夜 8 192
- 实现:`crates/hft-refdata/tests/prop_engine.rs::ref_070_ecf_roundtrip`

### REF-071 同一 blob 两次加载的全部内部表逐字节相同

- 规则:[延迟与资源契约](README.md#延迟与资源契约)、[生命周期(变更 = 重启)](README.md#生命周期变更--重启)
- 级别:unit
- 里程碑:M1
- 前置:F0;`RefData::debug_tables()`(`#[doc(hidden)]`,导出 blob 副本与哈希以外的全部内部表字节)
- 步骤:
  1. 在两个线程中各 `load(&f0())` 一次
  2. 第三次在同一线程先分配并释放 1 MiB 后再加载(改变堆布局)
- 期望:三次的 `debug_tables()` 逐字节相同(含 FxHash 前置表);三次的 `sha256()` 相同
- 实现:`crates/hft-refdata/src/lib.rs::ref_071_load_deterministic`

### REF-072 load 与查询的分配规则

- 规则:[延迟与资源契约](README.md#延迟与资源契约)
- 级别:unit
- 里程碑:M1
- 前置:计数分配器;`rd = load(&f0())` 之后清零计数
- 步骤:
  1. 各调用 10 000 次:`inst`、`caps`、`account`、`fees`、`symbols(1).get`、`get_num`、`inst_meta`、`asset`、`venue`、`venue_uid`、`instance`、`account_of`、`insts_of_asset`、`insts_of_venue`、`rate_limits`、`endpoints`、`maintenance`、`venue_account_ix`、`warnings`
  2. 成功路径的 `check_binding`、`check_group`、`check_ref_inst` 各 10 000 次
- 期望:两步后分配次数都为 0
- 实现:`crates/hft-refdata/tests/alloc.rs::ref_072_queries_do_not_allocate`

### REF-073 5 000 个 inst 的内存占用

- 规则:[延迟与资源契约](README.md#延迟与资源契约)、[实现说明](README.md#实现说明)
- 级别:unit
- 里程碑:M1
- 前置:`universe6(5000, 20)`(5 000 个 inst 均匀分布在 6 个 core venue,20 个 Live 账户);计数分配器记录 `load` 期间的分配字节
- 步骤:
  1. `rd = load(&b)`
- 期望:`InstHot` 表 5 001 行 × 64 B = 320 064 B;`load` 返回时仍持有的分配字节(分配减释放)除 blob 副本与 `InstHot` 表外合计 < 4 000 000 B
- 实现:`crates/hft-refdata/tests/alloc.rs::ref_073_memory_5000`

### REF-074 InstHot、SymEntry 与热字段的布局

- 规则:[数据结构](README.md#数据结构)、[实现说明](README.md#实现说明)
- 级别:table
- 里程碑:M1
- 前置:编译期断言与 `offset_of!`
- 实现:`crates/hft-refdata/tests/layout.rs::ref_074_hot_layouts`

| 行 | 断言 | 期望 |
|---|---|---|
| REF-074.1 | `size_of::<InstHot>()`、`align_of` | 64、64 |
| REF-074.2 | `venue`、`status`、`kind`、`flags` | 0、1、2、3 |
| REF-074.3 | `px_exp`、`qty_exp`、`px_wire_exp`、`qty_wire_exp` | 4、5、6、7 |
| REF-074.4 | `asset`、`quote`、`book_window` | 8、10、12 |
| REF-074.5 | `tick`、`lot`、`min_qty`、`max_qty`、`min_notional` | 16、24、32、40、48 |
| REF-074.6 | `l2_depth`、`crossed_tolerance`、`_pad` | 56、58、60 |
| REF-074.7 | `SymEntry`:大小,`key`、`len`、`inst` | 32;0、27、28 |
| REF-074.8 | `size_of::<VenueCaps>()`;`flags` 与 `dms` 的偏移 | ≤ 128;都 < 64 |
| REF-074.9 | `AccountMeta` 的 `kind` 判别值、`bound_instance`、`venues` 的偏移 | 都 < 64 |
| REF-074.10 | `InstHot` 表首地址 | 64 的整数倍 |

### REF-075 每张码表的值与往返

- 规则:[数据结构](README.md#数据结构)、[交易所能力表(VenueCaps)](README.md#交易所能力表venuecaps)
- 级别:table
- 里程碑:M1
- 前置:每个值经 `RdbBuilder` 写入一行再 `load`,读出值与写入相同;每张表越界值见 REF-009
- 实现:`crates/hft-refdata/tests/codes.rs::ref_075_code_tables`

| 行 | 码表 | 期望值 |
|---|---|---|
| REF-075.1 | `InstStatus`(Invalid 0 只出现在 NONE 行,不写入) | Trading 1、ReduceOnly 2、Halted 3、Maintenance 4、PreTrading 5、Delisted 6 |
| REF-075.2 | `InstKind` | Spot 1、Perp 2、Delivery 3 |
| REF-075.3 | `MaintKind` | VenueMaintenance 1、MarketClosed 2、Delisting 3 |
| REF-075.4 | `AssetClass` | Crypto 1、Stable 2、Equity 3、Commodity 4 |
| REF-075.5 | `LimitScope` | Ip 1、Uid 2、Account 3、Conn 4、Inst 5 |
| REF-075.6 | `LimitBucket` | Orders 1、Weight 2、WsInMsgs 3、NewConns 4、Amends 5 |
| REF-075.7 | `EndpointRole` | MdPublic 1、MdMarket 2、UserData 3、OrderWs 4、Rest 5、RestBackup 6 |
| REF-075.8 | `EndpointEnv`、`TsUnit` | Prod 1、Testnet 2、Demo 3;Ms 1、Us 2、Ns 3 |
| REF-075.9 | `StaleGuardKind` | None 1、RecvWindow 2、ExpireAt 3 |
| REF-075.10 | `Exchange` | Binance 1、Okx 2、Bitget 3、Gate 4、Bybit 5、Kucoin 6、Hyperliquid 7、Polymarket 8、Chain 32 |
| REF-075.11 | `UidPolicy`、`AccountKind` 行内编码 | Verified 1、Unverifiable 2;Live 1、Sim 2 |
| REF-075.12 | `FundingSettleKind` | PerSymbolPush 1、BalanceOnlyPush 2、RateOnly 3、NotApplicable 4 |
| REF-075.13 | `MarginData` 判别 | Push 1、Poll 2(带 `interval_ms`)、NotApplicable 3 |
| REF-075.14 | `PositionMode`、`MarginMode`、`MultiAsset` | NotApplicable 1、OneWay 2、Hedge 3;NotApplicable 1、Cross 2、Isolated 3、Mixed 4;NotApplicable 1、Off 2、On 3 |
| REF-075.15 | `LiqMarker` | NotIdentifiable 1、ClientIdPrefix 2、OrderTypeField 3、ExecTypeField 4 |
| REF-075.16 | `DeadManCap` | None 1、CountdownPerInst 2、CountdownAccount 3、OnDisconnect 4、Schedule 5 |
| REF-075.17 | `TestnetKind` | None 1、Testnet 2、Demo 3 |
| REF-075.18 | `Option<bool>` 线编码(`required_multi_asset`、`required_spot_borrow`) | None 1、Some(false) 2、Some(true) 3 |
| REF-075.19 | `CapFlags` 位 | AMEND 1、POST_ONLY 2、IOC 4、FOK 8、BATCH_CANCEL 16、PERP_CASH 32、CANCEL_NEEDS_ACK 256,两两不重叠 |
| REF-075.20 | `InstHot.flags` 位 | NO_AMEND 1 |
| REF-075.21 | `FeeAssetRule` 四个变体 | 各写一行(`ReceivedAssetUnlessDiscount{token: BTC}`、`Configured(USDT)`)读回相等 |

### REF-076 LoadErrCode 的码值逐一固定

- 规则:[LoadErrCode 码表](README.md#loaderrcode-码表)
- 级别:table
- 里程碑:M1
- 前置:`LoadErrCode as u16`;`size_of::<LoadErrCode>() == 2`
- 实现:`crates/hft-refdata/tests/codes.rs::ref_076_load_err_codes`

| 行 | 变体 | 期望值 |
|---|---|---|
| REF-076.1 | Structure、Strings、Ids、UnknownVenue、InstanceIdRange、DanglingRef、QuoteNotStable、InstanceMultiBound | 1、2、3、4、5、6、7、8 |
| REF-076.2 | MissingCaps、CapsMarketMismatch、SpotBorrowRequired、DeadManRange、CapsValue | 9、10、11、12、13 |
| REF-076.3 | ExpRange、NotionalExp、UnsupportedKind、TickMismatch、AssetExpTooCoarse、ValueRange、DuplicateSymbol | 14、15、16、17、18、19、20 |
| REF-076.4 | SameExchangeMarkets、DuplicateVenueUid、MissingFee、MissingVenueUid、TooManyAccountsPerVenue、SimIsProd、SimHasUid、SimCfg、UidPolicyProd | 21–29 依次 |
| REF-076.5 | AlreadyActive、VersionNotIncreasing、InstRemoved、AssetChanged、MultiplierChanged、AccountKindChanged、UidChanged、InstanceChanged | 30–37 依次 |
| REF-076.6 | AccountBound、KindMismatch、LegNotTradable、LegVenueNotInAccount、LegAssetMismatch、QuoteMismatch、SameExchangeLegs | 38–44 依次 |
| REF-076.7 | NoValidator、HostClassMismatch、RefInstNotSubscribed、UnknownStrategyKind、CapsExceedProtocol、StaleGuardMismatch、BindingChangedWithPosition | 45–51 依次 |
| REF-076.8 | `LoadErrCode::try_from(0u16)`、`try_from(52u16)` | 都返回 `Err`(0 不用;52 未分配) |

### REF-077 schema 导出与码表、行布局一致

- 规则:[LoadErrCode 码表](README.md#loaderrcode-码表)、[实现说明](README.md#实现说明)、[交易所能力表(VenueCaps)](README.md#交易所能力表venuecaps)
- 级别:unit
- 里程碑:M1
- 前置:`src/schema.rs` 导出的 RDB1 行描述与 LoadErrCode 码表(gen-schema 读取的同一份数据)
- 步骤:
  1. 取导出的 LoadErrCode 列表
  2. 取导出的每个 RDB1 行描述(字段名、偏移、类型)
  3. 收集全部行描述中字段名含 `leverage` 的字段
- 期望:
  - 步骤 1 恰 51 项,(名字, 值) 与 REF-076 逐项相同
  - 步骤 2 中每个字段的偏移等于该行结构的 `offset_of!`,行大小等于 `size_of`
  - 步骤 3 的结果恰为 `["InstMeta.max_leverage"]`(refdata 没有默认杠杆常量)
- 实现:`crates/hft-refdata/tests/schema.rs::ref_077_schema_export`

### REF-078 可读转储与两版本差异确定且使用小写枚举名

- 规则:[实现说明](README.md#实现说明)、[数据结构](README.md#数据结构)
- 级别:unit
- 里程碑:M1
- 前置:feature `tools`;`a = load(F0)`、`b = load(F0 改账户 1 is_prod = false、uid_policy = Unverifiable,v11)`
- 步骤:
  1. `t1 = dump(&a)`、`t2 = dump(&a)`
  2. `d = diff(&a, &b)`
- 期望:
  - `t1 == t2`;`t1` 中账户 1 一节含 `uid_policy = "verified"`
  - `d` 恰含三项差异 (位置, 旧值, 新值):(`version`, 10, 11)、(账户 `acct-a` 的 `is_prod`, true, false)、(账户 `acct-a` 的 `uid_policy`, `"verified"`, `"unverifiable"`)
- 实现:`crates/hft-refdata/tests/dump.rs::ref_078_dump_and_diff`

### REF-079 hft-cfg 的 TOML 经构造器、加载与转储往返一致

- 规则:[生命周期(变更 = 重启)](README.md#生命周期变更--重启)、[实现说明](README.md#实现说明)
- 级别:integ
- 里程碑:M1
- 前置:`fixtures/refdata/f0.toml`(与 F0 同内容);CI 每次提交运行
- 步骤:
  1. `hft-cfg build --kind refdata f0.toml` 两次,得 `b1`、`b2`
  2. `load(b1)` 后 `dump`,把转储的 TOML 再经 `hft-cfg build` 得 `b3`
- 期望:`b1 == b2 == b3`(逐字节);`b1 == f0()`(与测试构造器结果相同)
- 实现:`tools/hft-cfg/tests/refdata_roundtrip.rs::ref_079_toml_roundtrip`

### REF-080 加载器对任意字节不 panic

- 规则:[公开 API](README.md#公开-api)、[实现说明](README.md#实现说明)
- 级别:fuzz
- 里程碑:M1
- 目标:`crates/hft-refdata/fuzz/load`:同一输入依次调用 `RefData::load(data, None)`、`RefData::load(data, Some(&f0_loaded))` 与 `EngineConfig::load(data, None)`
- 语料:F0、E0、REF-005 到 REF-009 的全部失败输入
- 判定:不 panic、不越界、单输入不超时(口径见 [testing §3.3](../../docs/testing.md#33-fuzz));成功时返回值满足 `version() > 0`
- 实现:`crates/hft-refdata/fuzz/fuzz_targets/load.rs::ref_080_fuzz_load`

### REF-081 符号查找基准

- 规则:[延迟与资源契约](README.md#延迟与资源契约)
- 级别:bench
- 里程碑:M1
- 基准名:`refdata_symbol_get`
- 前置:≤ 16 字节符号,表已预热,命中
- 实现:`crates/hft-refdata/benches/symbols.rs::ref_081_refdata_symbol_get`

### REF-082 符号查找未命中基准

- 规则:[延迟与资源契约](README.md#延迟与资源契约)
- 级别:bench
- 里程碑:M1
- 基准名:`refdata_symbol_get_miss`
- 实现:`crates/hft-refdata/benches/symbols.rs::ref_082_refdata_symbol_get_miss`

### REF-083 数值 id 查找基准

- 规则:[延迟与资源契约](README.md#延迟与资源契约)
- 级别:bench
- 里程碑:M1
- 基准名:`refdata_num_get`
- 实现:`crates/hft-refdata/benches/symbols.rs::ref_083_refdata_num_get`

### REF-084 inst 行查找基准

- 规则:[延迟与资源契约](README.md#延迟与资源契约)
- 级别:bench
- 里程碑:M1
- 基准名:`refdata_inst`
- 实现:`crates/hft-refdata/benches/query.rs::ref_084_refdata_inst`

### REF-085 能力表查找基准

- 规则:[延迟与资源契约](README.md#延迟与资源契约)
- 级别:bench
- 里程碑:M1
- 基准名:`refdata_caps`
- 实现:`crates/hft-refdata/benches/query.rs::ref_085_refdata_caps`

### REF-086 四腿 check_group 基准

- 规则:[延迟与资源契约](README.md#延迟与资源契约)
- 级别:bench
- 里程碑:M1
- 基准名:`refdata_check_group`
- 实现:`crates/hft-refdata/benches/params.rs::ref_086_refdata_check_group`

### REF-087 5 000 个 inst 的加载基准

- 规则:[延迟与资源契约](README.md#延迟与资源契约)
- 级别:bench
- 里程碑:M1
- 基准名:`refdata_load_5000`
- 前置:`universe6(5000, 20)`(同 REF-073)
- 实现:`crates/hft-refdata/benches/load.rs::ref_087_refdata_load_5000`

### REF-088 依赖闭包与纯 crate 约束

- 规则:[依赖](README.md#依赖)
- 级别:unit
- 里程碑:M1
- 前置:`cargo metadata` 的输出;`src/` 的源文件文本
- 步骤:
  1. 取 hft-refdata 的普通依赖(不含 feature `tools`)
  2. 扫描 `src/**/*.rs`(`#[cfg(test)] mod tests` 测试模块除外)
- 期望:
  - 步骤 1 的集合 ⊆ {`hft-types`、`zerocopy`、`sha2`、`rustc-hash`};不含任何 `venue-*`、`hft-strategy-sdk`、`serde_json`、`tokio`
  - 步骤 2 中没有 `std::fs`、`std::net`、`std::thread`、`std::time`、`HashMap`、`AtomicU`、`rand` 的出现
- 实现:`crates/hft-refdata/tests/hygiene.rs::ref_088_pure_dependencies`

### REF-089 真实 core venue refdata 的加载与能力表出处

- 规则:[延迟与资源契约](README.md#延迟与资源契约)、[交易所能力表(VenueCaps)](README.md#交易所能力表venuecaps)
- 级别:host
- 里程碑:M3
- 环境:按需租用的参考机(`c8g.4xlarge`);`hft-cfg refdata fetch` 从三家 6 个产品线拉取并经人工审阅后构建的 blob
- 授权:无
- 步骤:
  1. `cargo bench --bench load -- refdata_real` 以该 blob 运行 100 次
  2. 核对 `venue-binance`、`venue-okx`、`venue-bitget` README 的 `VenueCaps` 默认值表
- 通过标准:加载 p99 < 30 ms;全部 inst 加载无错误;三家 6 个 `VenueId` 的每个 `VenueCaps` 字段在对应 README 中有出处或写明核实方法
- 证据:`ops-log/refdata/REF-089-<日期>.md`(基准输出与核对清单)
- 实现:`crates/hft-refdata/tests/host/REF-089.sh`

### REF-090 资产尘埃线的范围与查询

- 规则:[数据结构](README.md#数据结构)、[实现说明](README.md#实现说明)
- 级别:table
- 里程碑:M1
- 前置:F0(BTC `qty_max` raw 10¹⁴,`qty_exp = −8`);每行只改 BTC(资产 1)的 `dust`
- 实现:`crates/hft-refdata/tests/ranges.rs::ref_090_asset_dust`

| 行 | BTC `dust`(raw) | 期望 |
|---|---|---|
| REF-090.1 | 1 000 | `Ok`;`asset(AssetId(1)).dust == Qty(1 000)`(0.00001 BTC) |
| REF-090.2 | 1 | `Ok` |
| REF-090.3 | 0 | `[{ValueRange, 1, 0}]` |
| REF-090.4 | −1 | `[{ValueRange, 1, 0}]` |
| REF-090.5 | 10¹⁴(= `qty_max`) | `Ok` |
| REF-090.6 | 10¹⁴ − 1 | `Ok` |
| REF-090.7 | 10¹⁴ + 1 | `[{ValueRange, 1, 0}]` |

### REF-091 分档维持保证金率段的加载、查询与校验

- 规则:[数据结构](README.md#数据结构)、[实现说明](README.md#实现说明)
- 级别:table
- 里程碑:M1
- 前置:F0 追加段 18 `MmrTiers`,行按所列;`T(i, n, m)` = `{inst i, notional_max n USDT, mmr m}`,mmr 写百分数(1% = `Rate(10 000 000)`)
- 实现:`crates/hft-refdata/tests/ranges.rs::ref_091_mmr_tiers`

| 行 | 段 18 的行 | 期望 |
|---|---|---|
| REF-091.1 | 无段 18 | `Ok`;`inst_meta(InstId(1)).mmr_tiers` 为空 |
| REF-091.2 | `T(1, 50 000, 0.4%)`、`T(1, 600 000, 0.5%)` | `Ok`;inst 1 的 `mmr_tiers` 依次为这两档;inst 3 为空 |
| REF-091.3 | `T(1, 50 000, 0.4%)`、`T(1, 50 000, 0.5%)`(上限不严格递增) | `[{ValueRange, 18, 1}]` |
| REF-091.4 | `T(1, 50 000, 0.5%)`、`T(1, 600 000, 0.4%)`(mmr 递减) | `[{ValueRange, 18, 1}]` |
| REF-091.5 | `T(1, 50 000, 0.4%)`、`T(1, 600 000, 0.4%)`(mmr 相等) | `Ok` |
| REF-091.6 | `T(1, 50 000, 0)` | `[{ValueRange, 18, 0}]` |
| REF-091.7 | `T(1, 50 000, 100%)`(`Rate(1 000 000 000)`) | `[{ValueRange, 18, 0}]` |
| REF-091.8 | `T(1, 50 000, Rate(999 999 999))` | `Ok` |
| REF-091.9 | `T(2, 50 000, 0.4%)`(inst 2 是现货) | `[{ValueRange, 18, 0}]` |
| REF-091.10 | `T(99, 50 000, 0.4%)`(inst 不存在) | `[{DanglingRef, 18, 0}]` |

### REF-092 价格限制带的范围与查询

- 规则:[数据结构](README.md#数据结构)、[实现说明](README.md#实现说明)
- 级别:table
- 里程碑:M1
- 前置:F0;每行只改 inst 1 的 `price_band`(`Rate`,1% = `Rate(10 000 000)`)
- 实现:`crates/hft-refdata/tests/ranges.rs::ref_092_price_band`

| 行 | `price_band` | 期望 |
|---|---|---|
| REF-092.1 | `{0, 0}` | `Ok`;`inst_meta(InstId(1)).price_band == {Rate(0), Rate(0)}`(未登记) |
| REF-092.2 | `{5%, 5%}` | `Ok`;查询返回同值 |
| REF-092.3 | `{Rate(999 999 999), Rate(999 999 999)}` | `Ok` |
| REF-092.4 | `{Rate(1 000 000 000), 5%}` | `[{ValueRange, 3, 0}]` |
| REF-092.5 | `{5%, Rate(1 000 000 000)}` | `[{ValueRange, 3, 0}]` |
| REF-092.6 | `{Rate(−1), 5%}` | `[{ValueRange, 3, 0}]` |

### REF-093 amend_qty_down_only 只能收紧,协议为真时 refdata 行必须为真

- 规则:[交易所能力表(VenueCaps)](README.md#交易所能力表venuecaps)、[实现说明](README.md#实现说明)
- 级别:table
- 里程碑:M1
- 前置:F0、E0、A0;每行给出 VenueCaps 行 2(`BINANCE_SPOT`)与 `P0` 中 venue 2 的 `caps.amend_qty_down_only`,调用 `check_assembly`
- 实现:`crates/hft-refdata/tests/assembly.rs::ref_093_amend_qty_down_only`

| 行 | refdata 行 | 协议 | 期望 |
|---|---|---|---|
| REF-093.1 | true | true | `Ok`;`caps(VenueId(2)).amend_qty_down_only == true` |
| REF-093.2 | true | false | `Ok`(收紧) |
| REF-093.3 | false | false | `Ok` |
| REF-093.4 | false | true | `[{CapsExceedProtocol, 13, 1}]` |

### REF-094 永续结算能力必须明确且受协议支持

- 规则:[交易所能力表(VenueCaps)](README.md#交易所能力表venuecaps)
- 级别:table
- 里程碑:M2
- 前置:F0、E0、A0,只改变所列能力位
- 实现:`crates/hft-refdata/tests/assembly.rs::ref_094_perp_cash_capability_is_explicit`

| 行 | 输入 | 期望 |
|---|---|---|
| REF-094.1 | venue 1 与协议均支持 PERP_CASH | 加载往返保留该位,装配成功 |
| REF-094.2 | refdata 支持,协议不支持 | CapsExceedProtocol |
| REF-094.3 | 现货 venue 2 声明 PERP_CASH | CapsMarketMismatch |

### REF-095 观察 group 免查腿 venue 在账户内,其余检查不变

- 规则:[实现说明](README.md#实现说明)(跨 blob 第 2 条)
- 级别:table
- 里程碑:M2
- 前置:F0(账户 1 的 venues = {1, 6},inst 3 `BTC-USDT-SWAP`@5 不在账户内);`L(i, v)` 同 REF-060;`open` 为 `GroupSpecView.open_allowed`
- 实现:`crates/hft-refdata/tests/params.rs::ref_095_observe_group_venue`

| 行 | `GroupSpecView` | `open` | 期望 |
|---|---|---|---|
| REF-095.1 | BTC,`[L(3, 5)]`(参考行情:唯一一腿不在账户内) | false | `Ok(USDT)` |
| REF-095.2 | 同上 | true | `Err(LegVenueNotInAccount)`(可开仓的 group 不放宽) |
| REF-095.3 | BTC,`[L(1, 1), L(3, 5)]`(一腿在账户内) | false | `Err(LegVenueNotInAccount)` |
| REF-095.4 | BTC,`[L(4, 5)]`(腿的 venue 与 inst 不符) | false | `Err(LegVenueNotInAccount)` |
| REF-095.5 | BTC,`[L(99, 5)]`(inst 不存在) | false | `Err(LegNotTradable)` |
| REF-095.6 | ETH,`[L(3, 5)]` | false | `Err(LegAssetMismatch)` |

## 验收

- 里程碑 M1:本文件中里程碑为 M1 的用例全部有同编号的实现且通过(`cargo xtask test-trace` 与 [docs/tests §6](../../docs/tests/README.md#6-与代码的对应));表驱动用例每一行都是独立断言。
- 属性测试的大例数运行(每夜):REF-047 4 096 例,REF-070 8 192 例;PR 例数见各用例。
- fuzz:REF-080 在 PR 冒烟、CI 每夜与服务器累计中零失败(不计入上线门槛 ③,口径见 [testing §3.3](../../docs/testing.md#33-fuzz))。
- 基准:REF-081–REF-087 从 M1.5 起每个里程碑末在参考机上满足 [延迟与资源契约](README.md#延迟与资源契约) 的预算与 [benchmarks](../../benchmarks/README.md) 的单 crate 上限。
- 里程碑 M3:REF-089 的证据归档(真实 core venue refdata 的加载时间与 `VenueCaps` 出处核对)。

POSITION_CLOSE 仅限 reduce-only 永续;POSITION_CLOSE_ALL 必须包含 POSITION_CLOSE,否则 CapsMarketMismatch。
