---
component: hft-venue-proto
prefix: VPR
spec: README.md
---

# hft-venue-proto 测试规格

格式与规则见 [docs/tests](../../docs/tests/README.md)。本 crate 只有类型、trait、去重与码编码;"实现方必须遵守的契约"由 feature `testkit` 的检查函数在每个 venue 上执行。本文件的契约用例在内置假协议上证明每个检查函数对遵守契约的实现通过(正例)、对违反契约的变体检出(反例);各 venue 对真实报文调用这些检查的用例在该 venue 的 `TESTS.md`。

MarginItem 由 warm State 传至账户入口，CORE-169 验证变长编码；只读查询页行为由各 venue 的 BIN-175/BGT-175 验证。

CORE-120 非正式账户启动链路由 `apps/hft-core/tests/account_process.rs::core_120_mock_accounts_bootstrap_reauth_and_drift` 以独立子进程、两家真实协议和脚本 REST 验证单位倍率规则归一化、撤空、基线、重新核验、超时/错误模式失败与余额差异不覆盖账本；不替代真实 TLS、交易所或正式恢复验收。资金费账单复用 FundingPoll，沿用现有实际结算记账与去重。

## 覆盖

| README 小节 | 用例 |
|---|---|
| Warm 账户扩展 | VPR-015、VPR-126 |
| 认证资金费历史扩展 | VPR-100 |
| 概要 | VPR-122、VPR-150 |
| 目录 | 不适用:非规范 |
| 使用本组件 | 不适用:见子节 |
| `VenueProtocol`(静态分派) | VPR-010–VPR-016、VPR-126、VPR-048、VPR-063、VPR-073、VPR-080、VPR-083、VPR-087、VPR-097、VPR-099、VPR-112、VPR-114、VPR-122 |
| 辅助类型 | VPR-001–VPR-007、VPR-128、VPR-133、VPR-020、VPR-030–VPR-035、VPR-066、VPR-073、VPR-080、VPR-082、VPR-083、VPR-089、VPR-090、VPR-093、VPR-097、VPR-110、VPR-118、VPR-119 |
| 解码输出 sink | VPR-040–VPR-042、VPR-122 |
| 实现方必须遵守的契约 | VPR-012、VPR-013、VPR-020–VPR-022、VPR-030、VPR-045、VPR-049、VPR-050、VPR-060–VPR-129、VPR-130、VPR-133–VPR-139 |
| 每个 venue README 的必备节 | VPR-130 |
| 实现说明 | VPR-010、VPR-022、VPR-040、VPR-042–VPR-050、VPR-060、VPR-072、VPR-074、VPR-151 |
| 延迟与资源契约 | VPR-001–VPR-003、VPR-131、VPR-132、VPR-140、VPR-141 |
| 测试与验收 | 不适用:非规范 |
| 依赖 | VPR-150 |
| 延伸阅读 | 不适用:非规范 |
| 已知限制与后续工作 | 不适用:非规范 |
| 开发备忘 | VPR-021 |

## 用例

夹具(`crates/hft-venue-proto/tests/`):

- `fake_venue.rs`:内置假协议 `FakeSpot`(`MARKET = Spot`)与 `FakePerp`(`MARKET = LinearPerp`),报文为简单 JSON 文本,实现 trait 的全部方法,并且遵守全部契约;`FakePerp` 另实现增量深度(`SeqRule::Chain`)、BBO 补帧(`bbo_snapshot_request` 返回 `Ok(true)`,`BBO_SNAPSHOT_BATCH = 2`)、REST 查单(`order_channel(Query) = Rest`)、会话维护请求与 `CANCEL_ALL_SWEEP = true`;两者都实现只读查询族(`FakeSpot` 的 `Positions`、`FundingFlow` 返回 `Unsupported`)。
- 变体 `Fake<P, Mut::X>`:与 P 相同,只在一处违反某条契约(变体名写在各用例的前置里);用于证明对应的检查函数能发现违反。
- 样本集:`fake_venue.rs` 为每个检查函数提供的 payload 与期望字段;refdata `fake_rd()`:`FAKE-SPOT-1`(inst 101)、`FAKE-PERP-1`(inst 201)、`FAKE-PERP-2`(inst 202),价格指数 −8、数量指数 −8;账户 3(Live)。
- 检查函数的签名形如 `check_x::<P>(samples, …) -> Result<(), ContractFail>`;"通过"= 返回 `Ok(())`;"检出"= 返回 `Err(ContractFail{contract: N, …})`,`N` 为用例写明的契约编号。

### VPR-001 `RxMeta` 为 48 B,字段偏移固定

- 规则:[辅助类型](README.md#辅助类型)、[延迟与资源契约](README.md#延迟与资源契约)
- 级别:table
- 里程碑:M1.5
- 前置:无参数
- 实现:`crates/hft-venue-proto/tests/layout.rs::vpr_001_rx_meta_layout`

| 行 | 断言 | 期望 |
|---|---|---|
| VPR-001.1 | `size_of::<RxMeta>()`;`align_of` | 48;8 |
| VPR-001.2 | `rx_tsc`/`hw_rx_ns`/`sw_rx_ns`/`prev_override` 偏移 | 0 / 8 / 16 / 24 |
| VPR-001.3 | `conn`/`account`/`_pad` 偏移 | 32 / 33 / 34 |
| VPR-001.4 | `ts_flags`/`rx_empty_gap_ns`/`_pad2` 偏移 | 36 / 40 / 44 |

### VPR-002 `SeqIds` 16 B、`RawSeq` 32 B,偏移固定

- 规则:[辅助类型](README.md#辅助类型)、[延迟与资源契约](README.md#延迟与资源契约)
- 级别:table
- 里程碑:M3
- 前置:无参数
- 实现:`crates/hft-venue-proto/tests/layout.rs::vpr_002_seq_layouts`

| 行 | 断言 | 期望 |
|---|---|---|
| VPR-002.1 | `size_of::<SeqIds>()`;`prev_id`/`last_id` 偏移 | 16;0 / 8 |
| VPR-002.2 | `size_of::<RawSeq>()` | 32 |
| VPR-002.3 | `first`/`last`/`prev`/`has_prev`/`_pad` 偏移 | 0 / 8 / 16 / 24 / 25 |

### VPR-003 `TargetRef` 与 `AcctQuery` 不超过 64 B

- 规则:[延迟与资源契约](README.md#延迟与资源契约)
- 级别:table
- 里程碑:M3
- 前置:无参数
- 实现:`crates/hft-venue-proto/tests/layout.rs::vpr_003_small_types`

| 行 | 断言 | 期望 |
|---|---|---|
| VPR-003.1 | `size_of::<TargetRef>()` | ≤ 64 |
| VPR-003.2 | `size_of::<AcctQuery>()` | ≤ 64 |
| VPR-003.3 | `TargetRef`、`AcctQuery` 实现 `Copy` | 编译通过 |

### VPR-004 `StreamKind` 码值 1–8

- 规则:[辅助类型](README.md#辅助类型)
- 级别:table
- 里程碑:M3
- 前置:无参数
- 实现:`crates/hft-venue-proto/tests/layout.rs::vpr_004_stream_kind_codes`

| 行 | 变体 | `as u8` |
|---|---|---|
| VPR-004.1 | `Bbo` / `DepthTopN` / `DepthIncr` | 1 / 2 / 3 |
| VPR-004.2 | `Trade` / `Mark` / `Index` | 4 / 5 / 6 |
| VPR-004.3 | `Funding` / `Liquidation` | 7 / 8 |

### VPR-005 `AcctQueryKind`、`AcctHint`、`Skip` 码值

- 规则:[辅助类型](README.md#辅助类型)
- 级别:table
- 里程碑:M3
- 前置:无参数
- 实现:`crates/hft-venue-proto/tests/layout.rs::vpr_005_small_enum_codes`

| 行 | 变体 | `as u8` |
|---|---|---|
| VPR-005.1 | `AcctQueryKind::Config` / `Margin` | 1 / 2 |
| VPR-005.2 | `AcctHint::None` / `MarginNow` / `ConfigRecheck` | 0 / 1 / 2 |
| VPR-005.3 | `Skip::No` / `Listed` / `Unknown` | 0 / 1 / 2 |

### VPR-006 `Role` 就是 hft-event 的 `ConnRole`

- 规则:[辅助类型](README.md#辅助类型)
- 级别:table
- 里程碑:M3
- 前置:无参数
- 实现:`crates/hft-venue-proto/tests/layout.rs::vpr_006_role_is_conn_role`

| 行 | 断言 | 期望 |
|---|---|---|
| VPR-006.1 | `TypeId::of::<Role>() == TypeId::of::<hft_event::ConnRole>()` | true |
| VPR-006.2 | `Role::Md` / `Order` / `UserStream` / `Rest` 的码值 | 1 / 2 / 3 / 4 |
| VPR-006.3 | `RouteId(0)` | 默认路由;`FakePerp::routes()` 中 id 0 的路由存在 |

### VPR-007 `ReportCtl` 与 `RoKind` 码值

- 规则:[辅助类型](README.md#辅助类型)
- 级别:table
- 里程碑:M3
- 前置:无参数
- 实现:`crates/hft-venue-proto/tests/layout.rs::vpr_007_ctl_and_ro_codes`

| 行 | 变体 | `as u8` |
|---|---|---|
| VPR-007.1 | `ReportCtl::None` / `ClockSkew` / `RateLimited` / `UserStreamEnded` / `ServerShutdown` | 0 / 1 / 2 / 3 / 4 |
| VPR-007.2 | `RoKind::OpenOrders` / `Fills` / `FundingFlow` / `Balance` / `Positions` | 1 / 2 / 3 / 4 / 5 |
| VPR-007.3 | `ReportOutcome::default().ctl` | `ReportCtl::None` |

### VPR-010 encode_batch 默认返回 Unsupported(Place)

- 规则:[`VenueProtocol`(静态分派)](README.md#venueprotocol静态分派)、[实现说明](README.md#实现说明)
- 级别:unit
- 里程碑:M3
- 前置:只实现必需方法的最小假协议 `Minimal`
- 步骤:
  1. `Minimal::encode_batch(rd, t, &[c1, c2], &[tg1, tg2], &clk, &mut body)`
- 期望:
  - 返回 `Err(EncodeErr::Unsupported(GwOp::Place))`;`body` 全部字节不变
- 实现:`crates/hft-venue-proto/tests/defaults.rs::vpr_010_encode_batch_default`

### VPR-011 needs_per_order_signing 默认 false

- 规则:[`VenueProtocol`(静态分派)](README.md#venueprotocol静态分派)
- 级别:unit
- 里程碑:M3
- 前置:`Minimal`
- 步骤:
  1. `Minimal::needs_per_order_signing()`
- 期望:
  - `false`
- 实现:`crates/hft-venue-proto/tests/defaults.rs::vpr_011_per_order_signing_default`

### VPR-012 bbo_probe 默认 WsPing

- 规则:[`VenueProtocol`(静态分派)](README.md#venueprotocol静态分派)、[实现方必须遵守的契约](README.md#实现方必须遵守的契约)
- 级别:unit
- 里程碑:M3
- 前置:`Minimal`
- 步骤:
  1. `Minimal::bbo_probe()`
- 期望:
  - `PingKind::WsPing`
- 实现:`crates/hft-venue-proto/tests/defaults.rs::vpr_012_bbo_probe_default`

### VPR-013 bbo_snapshot_request 默认 Ok(false) 且不写请求缓冲

- 规则:[`VenueProtocol`(静态分派)](README.md#venueprotocol静态分派)、[实现方必须遵守的契约](README.md#实现方必须遵守的契约)
- 级别:unit
- 里程碑:M3
- 前置:`Minimal`;`RestReqBuf` 预置 `path_query = b"/keep"`
- 步骤:
  1. `Minimal::bbo_snapshot_request(rd, &ctx, &[101], &mut out)`
- 期望:
  - `Ok(false)`;`out.path_query == b"/keep"`
- 实现:`crates/hft-venue-proto/tests/defaults.rs::vpr_013_bbo_snapshot_request_default`

### VPR-014 decode_bbo_snapshot 默认 Err(UnknownClass) 且不产出事件

- 规则:[`VenueProtocol`(静态分派)](README.md#venueprotocol静态分派)
- 级别:unit
- 里程碑:M3
- 前置:`Minimal`;`VecSink` 为空
- 步骤:
  1. `Minimal::decode_bbo_snapshot(rd, &mut ctx, &[101], b"{}", &rx, &mut sink)`
- 期望:
  - `Err(DecodeErr::UnknownClass)`;`sink` 记录数 0
- 实现:`crates/hft-venue-proto/tests/defaults.rs::vpr_014_decode_bbo_snapshot_default`

### VPR-015 REST 出口与会话维护的默认实现

- 规则:[`VenueProtocol`(静态分派)](README.md#venueprotocol静态分派)、[实现方必须遵守的契约](README.md#实现方必须遵守的契约)
- 级别:table
- 里程碑:M3
- 前置:`Minimal`;`RestReqBuf` 预置 `path_query = b"/keep"`;`GwCmd{op: Query, target_slot: 7}`
- 实现:`crates/hft-venue-proto/tests/defaults.rs::vpr_015_rest_defaults`

| 行 | 调用 | 期望 |
|---|---|---|
| VPR-015.1 | `order_channel(op)`,`op` ∈ {Place, Cancel, Amend, CancelAll, Query} | 都是 `OrderChannel::Ws` |
| VPR-015.2 | `encode_rest(…, &c, …)` | `Err(EncodeErr::Unsupported(GwOp::Query))`;`path_query == b"/keep"` |
| VPR-015.3 | `session_keepalive_request(…)` | `Ok(false)`;`path_query == b"/keep"` |
| VPR-015.4 | `decode_rest(…, &RestOrigin{what: Order, …}, 200, b"{}", …)` | `Err(DecodeErr::UnknownClass)`;sink 记录数 0 |
| VPR-015.5 | `CANCEL_ALL_SWEEP`;`BBO_SNAPSHOT_BATCH` | `false`;1 |

### VPR-016 default_caps 经注入的资产表解析平台币

- 规则:[`VenueProtocol`(静态分派)](README.md#venueprotocol静态分派)、[实现方必须遵守的契约](README.md#实现方必须遵守的契约)
- 级别:table
- 里程碑:M3
- 前置:`FakeSpot` 的默认 `fee_asset_rule = ReceivedAssetUnlessDiscount{token: "FKT"}`(资产名);按行传入的查找函数
- 实现:`crates/hft-venue-proto/tests/defaults.rs::vpr_016_default_caps_asset_injection`

| 行 | 查找函数 | 期望 `default_caps(f).fee_asset_rule` |
|---|---|---|
| VPR-016.1 | `"FKT"` → `AssetId(9)` | `ReceivedAssetUnlessDiscount{token: 9}` |
| VPR-016.2 | 一律 `None` | `ReceivedAssetUnlessDiscount{token: ASSET_NONE}`;其余字段与 VPR-016.1 逐字段相同 |
| VPR-016.3 | `FakePerp`(规则 `Quote`,不引用资产名),查找函数被调用次数 | 0 |

### VPR-020 原始码编码

- 规则:[实现方必须遵守的契约](README.md#实现方必须遵守的契约)、[辅助类型](README.md#辅助类型)
- 级别:table
- 里程碑:M3
- 前置:`h(s) = xxh3_64(s)`(xxhash-rust)
- 实现:`crates/hft-venue-proto/tests/helpers.rs::vpr_020_raw_code_encoding`

| 行 | 调用 | 期望 |
|---|---|---|
| VPR-020.1 | `raw_code_num(-2010)` | `0xFFFF_F826`;`as i32` 读回 −2010 |
| VPR-020.2 | `raw_code_num(-1007)` | `0xFFFF_FC11` |
| VPR-020.3 | `raw_code_str(b"51008")` | 51008 |
| VPR-020.4 | `raw_code_str(b"-1007")` | `0xFFFF_FC11` |
| VPR-020.5 | `raw_code_str(b"007")` | 7 |
| VPR-020.6 | `raw_code_str(b"2147483647")` / `b"-2147483648"` | `0x7FFF_FFFF` / `0x8000_0000` |
| VPR-020.7 | `raw_code_str(b"2147483648")` | `0x4000_0000 \| (h(b"2147483648") & 0x00FF_FFFF)` |
| VPR-020.8 | `raw_code_str(b"abc")` | `0x4000_0000 \| (h(b"abc") & 0x00FF_FFFF)` |
| VPR-020.9 | `raw_code_str(b"+5")` / `b""` / `b"-"` | 各为 `0x4000_0000 \| (h(该串) & 0x00FF_FFFF)` |
| VPR-020.10 | `raw_code_http(503)` | `0x2000_01F7` |
| VPR-020.11 | `RAW_CODE_NONE` | 0 |

### VPR-021 三种编码的取值段互不重叠

- 规则:[实现方必须遵守的契约](README.md#实现方必须遵守的契约)、[开发备忘](README.md#开发备忘)
- 级别:prop
- 里程碑:M3
- 性质:非数字串的结果 ∈ `[0x4000_0000, 0x40FF_FFFF]`;`raw_code_http(s)` ∈ `[0x2000_0000, 0x2000_FFFF]`;`|c| < 2²⁹` 的数值码结果不落在这两段,且 ≠ 0(c ≠ 0 时)
- 生成:随机非数字字节串 0–32 B;`s ∈ u16`;`c ∈ (−2²⁹, 2²⁹)`
- 参考:区间断言
- 例数:PR 100 000
- 实现:`crates/hft-venue-proto/tests/code.rs::vpr_021_ranges_disjoint`

### VPR-022 码编码、dedup_key 与 TradeIdRing 不分配

- 规则:[实现说明](README.md#实现说明)、[实现方必须遵守的契约](README.md#实现方必须遵守的契约)
- 级别:unit
- 里程碑:M3
- 前置:测试二进制安装计数全局分配器;`TradeIdRing<1024>` 已构造
- 步骤:
  1. 10⁵ 次:`raw_code_str`(随机 1–16 B)、`raw_code_num`、`raw_code_http`、`dedup_key`、`seen_or_insert`
- 期望:
  - 分配次数 0
- 实现:`crates/hft-venue/tests/alloc.rs::vpr_022_helpers_zero_alloc`

### VPR-030 dedup_key 区分三种键,键总是带 inst

- 规则:[辅助类型](README.md#辅助类型)、[实现方必须遵守的契约](README.md#实现方必须遵守的契约)
- 级别:table
- 里程碑:M3
- 前置:无参数
- 实现:`crates/hft-venue-proto/tests/helpers.rs::vpr_030_dedup_key_distinct`

| 行 | 比较 | 期望 |
|---|---|---|
| VPR-030.1 | `dedup_key(201, Id(5))` 与 `dedup_key(202, Id(5))` | 不等 |
| VPR-030.2 | `dedup_key(201, Id(5))` 与 `dedup_key(201, OrderSeq{exch_order_id: 5, fill_seq: 0})` | 不等 |
| VPR-030.3 | `dedup_key(201, OrderSeq{5, 0})` 与 `dedup_key(201, Funding{settle_ts_ns: 5, disc: 0})` | 不等 |
| VPR-030.4 | `dedup_key(201, OrderSeq{7, 100})` 与 `dedup_key(201, OrderSeq{7, 200})` | 不等 |
| VPR-030.5 | `dedup_key(0, Funding{1_700_000_000_000_000_000, 1})` 与 `dedup_key(0, Funding{…, 2})` | 不等 |
| VPR-030.6 | 同一输入调用两次 | 相等 |
| VPR-030.7 | 上面出现的每个结果 | ≠ 0 |

### VPR-031 dedup_key 确定、非零,随机不同输入无碰撞

- 规则:[辅助类型](README.md#辅助类型)
- 级别:prop
- 里程碑:M3
- 性质:同一 `(inst, TradeKey)` 结果恒等;结果 ≠ 0;10⁶ 个两两不同的输入的结果两两不同
- 生成:inst ∈ u32;三种 `TradeKey` 各占 1/3,字段 ∈ u64 / i64
- 参考:`HashSet<u64>` 计数
- 例数:PR 1(每例 10⁶ 个输入)
- 实现:`crates/hft-venue-proto/tests/dedup.rs::vpr_031_dedup_key_no_collision`

### VPR-032 TradeIdRing 首次插入返回 false,再次返回 true

- 规则:[辅助类型](README.md#辅助类型)
- 级别:unit
- 里程碑:M3
- 前置:新 `TradeIdRing<1024>`
- 步骤:
  1. `seen_or_insert(1)`;`seen_or_insert(1)`;`seen_or_insert(2)`
- 期望:
  - 依次 `false`、`true`、`false`
- 实现:`crates/hft-venue-proto/tests/helpers.rs::vpr_032_ring_seen`

### VPR-033 TradeIdRing 满后挤出最老的键

- 规则:[辅助类型](README.md#辅助类型)
- 级别:table
- 里程碑:M3
- 前置:新 `TradeIdRing<1024>`;按行插入键 1..=k(两两不同)后查询键 1
- 实现:`crates/hft-venue-proto/tests/helpers.rs::vpr_033_ring_eviction_boundary`

| 行 | 已插入 k | 期望 `seen_or_insert(1)` |
|---|---|---|
| VPR-033.1 | 1023 | `true` |
| VPR-033.2 | 1024 | `true` |
| VPR-033.3 | 1025 | `false`(键 1 已被挤出,此次重新插入) |

### VPR-034 TradeIdRing 收到键 0 触发 debug_assert

- 规则:[辅助类型](README.md#辅助类型)
- 级别:unit
- 里程碑:M3
- 前置:debug 构建;新 `TradeIdRing<1024>`
- 步骤:
  1. `seen_or_insert(0)`
- 期望:
  - panic(debug_assert)
- 实现:`crates/hft-venue-proto/tests/dedup.rs::vpr_034_ring_zero_key_asserts`

### VPR-035 TradeIdRing 与"集合 + 先进先出"参考模型一致

- 规则:[辅助类型](README.md#辅助类型)
- 级别:prop
- 里程碑:M3
- 性质:任意非零键序列上,每次 `seen_or_insert` 的返回值等于参考模型
- 生成:10⁶ 次操作;键从 1..=4096 均匀抽取(制造重复与挤出)
- 参考:`HashSet<u64>` + `VecDeque<u64>`(容量 1024,满时弹出最老)
- 例数:PR 10(每例 10⁶ 次操作)
- 实现:`crates/hft-venue-proto/tests/helpers.rs::vpr_035_ring_matches_model`

### VPR-040 VecSink 按调用顺序记录 MdSink 的每个方法

- 规则:[解码输出 sink](README.md#解码输出-sink)、[实现说明](README.md#实现说明)
- 级别:table
- 里程碑:M3
- 前置:新 `VecSink`;按行调用一次
- 实现:`crates/hft-venue-proto/tests/testkit.rs::vpr_040_vec_sink_md`

| 行 | 调用 | 期望记录 |
|---|---|---|
| VPR-040.1 | `bbo(&b)` | `[Md::Bbo(b)]` |
| VPR-040.2 | `l2_begin(&h)`、`l2_level(Bid, 1, 2)`、`l2_level(Ask, 3, 0)`、`l2_end()` | `[L2Begin(h), L2Level(Bid,1,2), L2Level(Ask,3,0), L2End]` |
| VPR-040.3 | `l2_snapshot(&part)` | `[L2Snapshot(part 的拷贝)]` |
| VPR-040.4 | `trade`、`mark`、`index`、`funding`、`liquidation` 各一次 | 依次五条,类型与内容逐字节同输入 |

### VPR-041 VecSink 按调用顺序记录 ReportSink 的每个方法

- 规则:[解码输出 sink](README.md#解码输出-sink)
- 级别:table
- 里程碑:M3
- 前置:新 `VecSink`
- 实现:`crates/hft-venue-proto/tests/helpers.rs::vpr_041_vec_sink_report`

| 行 | 调用 | 期望记录 |
|---|---|---|
| VPR-041.1 | `order(&e)` | `[Report::Order(e)]` |
| VPR-041.2 | `position`、`balance`、`funding_settle`、`external_fill` 各一次 | 依次四条,内容同输入 |
| VPR-041.3 | `margin(&hdr, &[i1, i2])` | `[Margin(hdr, [i1, i2])]` |
| VPR-041.4 | `account_config(&hdr, &[])` | `[AccountConfig(hdr, [])]` |

### VPR-042 VecSink 的 exch_id:记录、命中、未知

- 规则:[解码输出 sink](README.md#解码输出-sink)、[实现说明](README.md#实现说明)
- 级别:table
- 里程碑:M3
- 前置:新 `VecSink`;`id = ClientOrderId{epoch: 4, venue, account: 3, slot: 77, gen: 9}`;按行顺序执行
- 实现:`crates/hft-venue-proto/tests/helpers.rs::vpr_042_vec_sink_exch_id`

| 行 | 调用 | 期望 |
|---|---|---|
| VPR-042.1 | `exch_id(b"X1", None)` | `Unknown` |
| VPR-042.2 | `exch_id(b"X1", Some(&id))` | `Recorded` |
| VPR-042.3 | `exch_id(b"X1", None)` | `Resolved{epoch: 4, slot: 77, gen: 9}` |
| VPR-042.4 | `exch_id(b"X2", None)` | `Unknown` |

### VPR-043 GoldenDriver 跑通假协议的分档快照序列

- 规则:[实现说明](README.md#实现说明)
- 级别:unit
- 里程碑:M3
- 前置:`FakePerp`;样本 `topn_seq`:3 条 20 档快照 payload 与期望 `L2Snapshot{origin = TopN}` 体
- 步骤:
  1. `GoldenDriver::<FakePerp>::run_md(topn_seq)`
- 期望:
  - 返回 `Ok`;比对了 3 条记录
- 实现:`crates/hft-venue-proto/tests/testkit.rs::vpr_043_golden_topn`

### VPR-044 GoldenDriver 跑通正常增量与断裂两种序列

- 规则:[实现说明](README.md#实现说明)
- 级别:table
- 里程碑:M3
- 前置:`FakePerp`(增量深度)
- 实现:`crates/hft-venue-proto/tests/testkit.rs::vpr_044_golden_incremental`

| 行 | 样本 | 期望 |
|---|---|---|
| VPR-044.1 | 5 条连续增量(`prev_id` = 上一条 `last_id`) | `Ok`;5 条 `L2Batch` 逐字节相同 |
| VPR-044.2 | 第 3 条 `prev_id` 不等于第 2 条 `last_id` | `Ok`;期望文件里第 3 条的 `prev_id` 照原值,比对通过(断裂判定不在驱动里) |

### VPR-045 GoldenDriver 在指定位置注入 prev_override

- 规则:[实现说明](README.md#实现说明)、[实现方必须遵守的契约](README.md#实现方必须遵守的契约)
- 级别:unit
- 里程碑:M3
- 前置:`FakePerp`;3 条增量;在第 2 条注入 `prev_override = 9_000`
- 步骤:
  1. `run_md` 并取出输出
- 期望:
  - 第 2 条 `L2Batch.prev_id == 9_000`;第 1、3 条的 `prev_id` 为报文原值
- 实现:`crates/hft-venue-proto/tests/testkit.rs::vpr_045_golden_prev_override`

### VPR-046 GoldenDriver 报告第一个不同字节的位置

- 规则:[实现说明](README.md#实现说明)
- 级别:unit
- 里程碑:M3
- 前置:`FakePerp`;期望文件第 2 条记录的第 17 字节被改动
- 步骤:
  1. `run_md`
- 期望:
  - 返回 `Err`,其中样本名为该样本、记录序号 = 1(从 0 计)、字节偏移 = 16
- 实现:`crates/hft-venue-proto/tests/helpers.rs::vpr_046_golden_first_difference`

### VPR-047 GoldenDriver 跑通账户查询响应序列

- 规则:[实现说明](README.md#实现说明)
- 级别:unit
- 里程碑:M3
- 前置:`FakePerp`;Config 序列 3 步的响应与期望 `AccountConfigReport`;Margin 序列 2 步的响应与期望 `MarginReport` + 2 条 `PositionReport`
- 步骤:
  1. `GoldenDriver::<FakePerp>::run_acct(Config, …)`;`run_acct(Margin, …)`
- 期望:
  - 两次都 `Ok`;`acct_request` 产出的每个 `RestReqBuf` 与期望逐字节相同
- 实现:`crates/hft-venue-proto/tests/testkit.rs::vpr_047_golden_acct`

### VPR-048 GoldenDriver 逐字节比对编码类方法

- 规则:[实现说明](README.md#实现说明)、[`VenueProtocol`(静态分派)](README.md#venueprotocol静态分派)
- 级别:table
- 里程碑:M3
- 前置:`FakePerp`;每行一组输入与期望输出(`tests/data/fake/encode/`)
- 实现:`crates/hft-venue-proto/tests/testkit.rs::vpr_048_golden_encoders`

| 行 | 方法 | 期望 |
|---|---|---|
| VPR-048.1 | `encode`(Place、Cancel、Amend、CancelAll、Query 各一) | 五个帧与期望逐字节相同 |
| VPR-048.2 | `encode_batch`(`FakePerp` 支持,2 条 Place) | 帧逐字节相同;`Minimal` 同输入返回 `Unsupported(Place)` |
| VPR-048.3 | `render_cid` | 写入字节数与内容同期望,规范 24 位文本位于末尾 |
| VPR-048.4 | `login`、`subscribe`、`private_subscribe` | 输出缓冲与期望逐字节相同 |
| VPR-048.5 | `endpoint` | `EndpointSpec` 的 path、query、额外头同期望 |
| VPR-048.6 | `snapshot_request`、`bbo_snapshot_request`、`acct_request`、`dead_man_refresh`、`session_identity_request` | 每个 `RestReqBuf` 逐字节相同 |
| VPR-048.7 | `encode_rest`(Query)、`session_keepalive_request`、`ro_request`(五种 `RoKind`) | 每个 `RestReqBuf` 逐字节相同 |

### VPR-049 FuzzHarness 对假协议跑 10⁵ 个随机输入无 panic

- 规则:[实现说明](README.md#实现说明)、[实现方必须遵守的契约](README.md#实现方必须遵守的契约)
- 级别:unit
- 里程碑:M3
- 前置:`FakeSpot`、`FakePerp`;固定种子 7 的随机字节(0–4 096 B)
- 步骤:
  1. 每个输入依次喂 `classify → peek → decode_md`、`decode_snapshot`、`decode_bbo_snapshot`、`classify_report → decode_report`、`decode_rest`、`acct_decode`、`ro_decode`
- 期望:
  - 无 panic;每次调用返回 `Ok` 或 `Err(DecodeErr::*)`
- 实现:`crates/hft-venue-proto/tests/testkit.rs::vpr_049_fuzz_harness_fake`

### VPR-050 FuzzHarness 让会 panic 的实现失败并给出输入

- 规则:[实现说明](README.md#实现说明)、[实现方必须遵守的契约](README.md#实现方必须遵守的契约)
- 级别:unit
- 里程碑:M3
- 前置:变体 `Fake<FakePerp, Mut::PanicOnShort>`(payload < 8 B 时 `decode_md` 索引越界)
- 步骤:
  1. `catch_unwind` 中对输入 `b"{}"` 运行 FuzzHarness 的单输入入口
- 期望:
  - `catch_unwind` 返回 `Err`;harness 记录的最后输入为 `b"{}"`
- 实现:`crates/hft-venue-proto/tests/testkit.rs::vpr_050_fuzz_harness_detects_panic`

### VPR-060 check_chain:增量序号归一化与 prev_override(契约 1)

- 规则:[实现方必须遵守的契约](README.md#实现方必须遵守的契约)、[实现说明](README.md#实现说明)
- 级别:unit
- 里程碑:M3
- 前置:`FakePerp`;样本:快照 `snap_id = 1_000`,其后增量 A(原始序号覆盖 990–1_005)、B、C
- 步骤:
  1. `check_chain::<FakePerp>(samples)`
- 期望:
  - 通过。检查内容:`align_first(1_000, A.raw) == Apply`;以 `prev_override = 1_000` 解码 A 时 `L2Batch.prev_id == 1_000`;B 的 `prev_id` = A 的 `last_id`,C 的 `prev_id` = B 的 `last_id`;`seq_rule == Chain`;对早于快照的增量 `align_first == Drop`,晚于快照且不衔接的 `== Gap`
- 实现:`crates/hft-venue-proto/tests/contract_md.rs::vpr_060_check_chain_pass`

### VPR-061 check_chain 检出忽略 prev_override 的实现

- 规则:[实现方必须遵守的契约](README.md#实现方必须遵守的契约)
- 级别:unit
- 里程碑:M3
- 前置:变体 `Fake<FakePerp, Mut::IgnorePrevOverride>`;样本同 VPR-060
- 步骤:
  1. `check_chain`
- 期望:
  - `Err(ContractFail{contract: 1})`,`sample` 指向增量 A
- 实现:`crates/hft-venue-proto/tests/contract_md.rs::vpr_061_check_chain_prev_override`

### VPR-062 check_chain 检出前后不衔接的归一化

- 规则:[实现方必须遵守的契约](README.md#实现方必须遵守的契约)
- 级别:unit
- 里程碑:M3
- 前置:变体 `Fake<FakePerp, Mut::LastIdOffByOne>`(`last_id` 写成交易所原值 − 1)
- 步骤:
  1. `check_chain`
- 期望:
  - `Err(ContractFail{contract: 1})`,`sample` 指向增量 B
- 实现:`crates/hft-venue-proto/tests/contract_md.rs::vpr_062_check_chain_continuity`

### VPR-063 check_md_basics:peek 与 decode_md 一致、peek 不解析价格(契约 2)

- 规则:[实现方必须遵守的契约](README.md#实现方必须遵守的契约)、[`VenueProtocol`(静态分派)](README.md#venueprotocol静态分派)
- 级别:unit
- 里程碑:M3
- 前置:`FakeSpot`、`FakePerp`;每类行情样本各 ≥ 3 条
- 步骤:
  1. `check_md_basics::<FakePerp>(samples, fake_rd())`;同样对 `FakeSpot`
- 期望:
  - 两者通过。检查内容:每条样本 `peek` 的 `inst` = `decode_md` 的 `MdOutcome.inst`;有 `L2Batch` 时 `peek.ids` = 批头 `prev_id/last_id`;`MdOutcome.events` = sink 收到的事件数;把价格与数量字段的数字替换为 `"x"` 后 `peek` 结果不变、`decode_md` 返回 `Err(Number)`;未订阅符号的 `peek.inst == 0`
- 实现:`crates/hft-venue-proto/tests/contract_md.rs::vpr_063_peek_without_price`

### VPR-064 check_md_basics 检出 peek 序号与批头不一致

- 规则:[实现方必须遵守的契约](README.md#实现方必须遵守的契约)
- 级别:unit
- 里程碑:M3
- 前置:变体 `Fake<FakePerp, Mut::PeekIdsDiffer>`(`peek.ids.last_id` 比批头大 1)
- 步骤:
  1. `check_md_basics`
- 期望:
  - `Err(ContractFail{contract: 2})`
- 实现:`crates/hft-venue-proto/tests/contract_md.rs::vpr_064_peek_ids_mismatch`

### VPR-065 check_md_basics 检出在 peek 里解析价格的实现

- 规则:[实现方必须遵守的契约](README.md#实现方必须遵守的契约)
- 级别:unit
- 里程碑:M3
- 前置:变体 `Fake<FakePerp, Mut::PeekParsesPrice>`(价格非法时 `peek` 返回 `Err(Number)`)
- 步骤:
  1. `check_md_basics`
- 期望:
  - `Err(ContractFail{contract: 2})`
- 实现:`crates/hft-venue-proto/tests/contract_md.rs::vpr_065_peek_parses_price`

### VPR-066 check_md_basics:RxMeta 的接收时间点与 conn 原样写进 MarketHdr(契约 3)

- 规则:[实现方必须遵守的契约](README.md#实现方必须遵守的契约)、[辅助类型](README.md#辅助类型)
- 级别:unit
- 里程碑:M1.5
- 前置:`FakePerp`;`RxMeta{rx_tsc: 0x1111, hw_rx_ns: 0x2222, sw_rx_ns: 0x3333, prev_override: 0, conn: 7, account: 0, ts_flags: 2, rx_empty_gap_ns: 0x4444}`;另一组 `hw_rx_ns = 0`、`ts_flags = 0`
- 步骤:
  1. 解码 BBO、成交、标记价、20 档快照样本各一条
- 期望:
  - 每条事件的 `MarketHdr`:`rx_tsc == 0x1111`、`hw_rx_ns == 0x2222`、`sw_rx_ns == 0x3333`、`rx_empty_gap_ns == 0x4444`、`conn == 7`
  - 第二组:`hw_rx_ns == 0`(实现方不以 `sw_rx_ns` 代替)
- 实现:`crates/hft-venue-proto/tests/contract_md.rs::vpr_066_rx_meta_copied`

### VPR-067 check_md_basics 检出自行把 SW 戳填进 hw_rx_ns 的实现

- 规则:[实现方必须遵守的契约](README.md#实现方必须遵守的契约)
- 级别:unit
- 里程碑:M3
- 前置:变体 `Fake<FakePerp, Mut::SubstituteSwTs>`(`hw_rx_ns == 0` 时写 `sw_rx_ns`)
- 步骤:
  1. `check_md_basics`
- 期望:
  - `Err(ContractFail{contract: 3})`
- 实现:`crates/hft-venue-proto/tests/contract_md.rs::vpr_067_sw_substitution_detected`

### VPR-068 check_md_basics 检出丢掉 rx_empty_gap_ns 或 conn 的实现

- 规则:[实现方必须遵守的契约](README.md#实现方必须遵守的契约)
- 级别:table
- 里程碑:M3
- 前置:按行的变体
- 实现:`crates/hft-venue-proto/tests/contract_md.rs::vpr_068_dropped_fields_detected`

| 行 | 变体 | 期望 |
|---|---|---|
| VPR-068.1 | `Mut::ZeroGap`(`rx_empty_gap_ns` 写 0) | `Err(ContractFail{contract: 3})` |
| VPR-068.2 | `Mut::ZeroConn`(`conn` 写 0) | `Err(ContractFail{contract: 3})` |

### VPR-069 check_md_basics:价格数量按资产指数精确解析,溢出与非法数字的错误类别(契约 4)

- 规则:[实现方必须遵守的契约](README.md#实现方必须遵守的契约)
- 级别:table
- 里程碑:M3
- 前置:`FakePerp`;inst 201 价格指数 −8;BBO 样本的 `bid_px` 字段按行取值
- 实现:`crates/hft-venue-proto/tests/contract_md.rs::vpr_069_decimal_errors`

| 行 | `bid_px` 文本 | 期望 |
|---|---|---|
| VPR-069.1 | `"1.10000000"` | `Ok`;`Bbo.bid_px == Px(110_000_000)` |
| VPR-069.2 | `"92233720368.54775807"` | `Ok`;`Bbo.bid_px == Px(i64::MAX)` |
| VPR-069.3 | `"92233720368.54775808"` | `Err(DecodeErr::Overflow)` |
| VPR-069.4 | `"0.000000001"` | `Err(DecodeErr::Number)` |
| VPR-069.5 | `""` | `Err(DecodeErr::Number)` |
| VPR-069.6 | `"1.2.3"` | `Err(DecodeErr::Number)` |

### VPR-070 check_md_basics 检出经 f64 的解析

- 规则:[实现方必须遵守的契约](README.md#实现方必须遵守的契约)
- 级别:unit
- 里程碑:M3
- 前置:变体 `Fake<FakePerp, Mut::ViaF64>`(先 `parse::<f64>` 再乘 1e8 取整)
- 步骤:
  1. `check_md_basics`(样本含 VPR-069.2 那一条)
- 期望:
  - `Err(ContractFail{contract: 4})`
- 实现:`crates/hft-venue-proto/tests/contract_md.rs::vpr_070_f64_detected`

### VPR-071 check_md_basics 检出把溢出报成 Number 的实现

- 规则:[实现方必须遵守的契约](README.md#实现方必须遵守的契约)
- 级别:unit
- 里程碑:M3
- 前置:变体 `Fake<FakePerp, Mut::OverflowAsNumber>`
- 步骤:
  1. `check_md_basics`(样本含 VPR-069.3 那一条)
- 期望:
  - `Err(ContractFail{contract: 4})`
- 实现:`crates/hft-venue-proto/tests/contract_md.rs::vpr_071_overflow_kind_detected`

### VPR-072 check_md_basics 检出在 FeedCtx 里缓存 refdata 的实现

- 规则:[实现说明](README.md#实现说明)
- 级别:unit
- 里程碑:M3
- 前置:变体 `Fake<FakePerp, Mut::CacheRd>`(首次调用时把符号表拷进 `FeedCtx`);两份 refdata:`rd_a` 把 `FAKE-PERP-1` 映射为 201,`rd_b` 映射为 301
- 步骤:
  1. `check_md_basics` 对同一个 `FeedCtx` 先以 `rd_a`、再以 `rd_b` 解码同一条 BBO
- 期望:
  - `FakePerp`:两次 `Bbo.h.inst` 依次 201、301,通过
  - 变体:`Err(ContractFail{contract: 2})`,`detail` 指出第二次的 inst 为 201
- 实现:`crates/hft-venue-proto/tests/contract_md.rs::vpr_072_rd_not_cached`

### VPR-073 decode_md 以控制类或未知类调用时返回 UnknownClass

- 规则:[辅助类型](README.md#辅助类型)、[`VenueProtocol`(静态分派)](README.md#venueprotocol静态分派)
- 级别:unit
- 里程碑:M3
- 前置:`FakePerp`;订阅回执 payload(`classify` 得 `Control`)
- 步骤:
  1. `decode_md(rd, ctx, Control, payload, rx, sink)`;`control(Control, payload)`
- 期望:
  - `decode_md` 返回 `Err(UnknownClass)`,sink 记录数 0;`control` 返回 `Some(ControlHint::SubAck{ok: true, code: 0})`
- 实现:`crates/hft-venue-proto/tests/contract_md.rs::vpr_073_non_data_class`

### VPR-074 check_hot_alloc:热路径方法与 acct_decode 不分配(契约 5)

- 规则:[实现方必须遵守的契约](README.md#实现方必须遵守的契约)、[实现说明](README.md#实现说明)
- 级别:unit
- 里程碑:M3
- 前置:测试二进制安装计数全局分配器;`FakePerp`;全部行情、回报、编码样本与一个 50 KiB 的持仓响应(含 200 个 refdata 之外的符号)
- 步骤:
  1. `check_hot_alloc::<FakePerp>(samples, alloc_count)`
- 期望:
  - 通过。检查内容:预热一轮后,`classify`、`peek`、`decode_md`、`encode`、`classify_report`、`decode_report`、`acct_decode` 的分配次数为 0;响应中 refdata 之外的 200 个符号不出现在 `acct_finish` 的报告里
- 实现:`crates/hft-venue-proto/tests/contract_report.rs::vpr_074_check_hot_alloc_pass`

### VPR-075 check_hot_alloc 检出在 decode_md 里分配的实现

- 规则:[实现方必须遵守的契约](README.md#实现方必须遵守的契约)
- 级别:unit
- 里程碑:M3
- 前置:变体 `Fake<FakePerp, Mut::AllocInDecode>`(每次 `decode_md` 建一个 `Vec`)
- 步骤:
  1. `check_hot_alloc`
- 期望:
  - `Err(ContractFail{contract: 5})`,`detail` 含 `decode_md`
- 实现:`crates/hft-venue-proto/tests/contract_report.rs::vpr_075_alloc_detected`

### VPR-076 check_unknown_codes:结果未知码与未登记码不产出 Reject(契约 6、8)

- 规则:[实现方必须遵守的契约](README.md#实现方必须遵守的契约)
- 级别:table
- 里程碑:M3
- 前置:`FakePerp`;`codes.rs` 确定拒绝表 {`-2010`, `"51008"`},结果未知表 {`-1007`, `"50004"`};样本构造函数按码生成下单响应
- 实现:`crates/hft-venue-proto/tests/contract_report.rs::vpr_076_check_unknown_codes_pass`

| 行 | 响应码 | 期望 `decode_report` |
|---|---|---|
| VPR-076.1 | `-2010` | 产出 `Reject`,`raw_code == 0xFFFF_F826`;`outcome_unknown == None` |
| VPR-076.2 | `"51008"` | 产出 `Reject`,`raw_code == 51008` |
| VPR-076.3 | `-1007` | 无 `Reject`;`outcome_unknown == Some(ReqRef{…})`;`raw_code == 0xFFFF_FC11` |
| VPR-076.4 | `"50004"` | 无 `Reject`;`outcome_unknown` 为 `Some`;`raw_code == 50004` |
| VPR-076.5 | 未登记 `-9999` | 无 `Reject`;`outcome_unknown` 为 `Some`;`raw_code == raw_code_num(-9999)` |
| VPR-076.6 | 未登记 `"ERR_X"` | 无 `Reject`;`raw_code == raw_code_str(b"ERR_X")` |
| VPR-076.7 | 只有 HTTP 503 | 无 `Reject`;`raw_code == 0x2000_01F7` |
| VPR-076.8 | 整张表交给 `check_unknown_codes` | 通过 |

### VPR-077 check_unknown_codes 检出把未登记码当作 Reject 的实现

- 规则:[实现方必须遵守的契约](README.md#实现方必须遵守的契约)
- 级别:unit
- 里程碑:M3
- 前置:变体 `Fake<FakePerp, Mut::UnknownAsReject>`
- 步骤:
  1. `check_unknown_codes`
- 期望:
  - `Err(ContractFail{contract: 8})`
- 实现:`crates/hft-venue-proto/tests/contract_report.rs::vpr_077_unregistered_reject_detected`

### VPR-078 check_unknown_codes 检出把传输超时类码当作 Reject 的实现

- 规则:[实现方必须遵守的契约](README.md#实现方必须遵守的契约)
- 级别:unit
- 里程碑:M3
- 前置:变体 `Fake<FakePerp, Mut::TimeoutAsReject>`(`-1007` 产出 `Reject`)
- 步骤:
  1. `check_unknown_codes`
- 期望:
  - `Err(ContractFail{contract: 6})`
- 实现:`crates/hft-venue-proto/tests/contract_report.rs::vpr_078_timeout_reject_detected`

### VPR-079 check_unknown_codes 检出原始码编码错误

- 规则:[实现方必须遵守的契约](README.md#实现方必须遵守的契约)
- 级别:unit
- 里程碑:M3
- 前置:变体 `Fake<FakePerp, Mut::HashNumericString>`(`"51008"` 走哈希段)
- 步骤:
  1. `check_unknown_codes`
- 期望:
  - `Err(ContractFail{contract: 8})`,`detail` 含 `51008`
- 实现:`crates/hft-venue-proto/tests/contract_report.rs::vpr_079_raw_code_encoding_detected`

### VPR-080 check_session:签名输出标脱敏、不含密钥,心跳与探测方式自洽(契约 7、24)

- 规则:[实现方必须遵守的契约](README.md#实现方必须遵守的契约)、[`VenueProtocol`(静态分派)](README.md#venueprotocol静态分派)、[辅助类型](README.md#辅助类型)
- 级别:unit
- 里程碑:M3
- 前置:`FakePerp`;`Credential` 的私钥材料为 32 B 哨兵 `0xA5…A5`
- 步骤:
  1. `check_session::<FakePerp>(cfg, cred)`
- 期望:
  - 通过。检查内容:`login`、`private_subscribe`、`endpoint`、`bootstrap`、`acct_request`、`dead_man_refresh`、`session_identity_request`、`encode`、`encode_rest`、`session_keepalive_request`、`ro_request` 的输出都不含哨兵字节串;含签名的 `RestReqBuf.redacted == true`,`snapshot_request`/`bbo_snapshot_request` 的 `redacted == false`;每个 `Role`:`heartbeat(role).ping == WsPing` ⇔ `app_ping(role, …) == 0`;`routes()` 的每条路由 `endpoint(Md, route, …)` 返回 `Ok`;`bootstrap(Md, cfg, None)` 为 `None` 时 `endpoint` 的 `boot` 参数传 `None` 也成功
- 实现:`crates/hft-venue-proto/tests/contract_session.rs::vpr_080_check_session_pass`

### VPR-081 check_session 检出把私钥材料写进输出的实现

- 规则:[实现方必须遵守的契约](README.md#实现方必须遵守的契约)
- 级别:unit
- 里程碑:M3
- 前置:变体 `Fake<FakePerp, Mut::SecretInHeader>`(`acct_request` 把私钥写进一个头)
- 步骤:
  1. `check_session`
- 期望:
  - `Err(ContractFail{contract: 7})`,`detail` 含 `acct_request`
- 实现:`crates/hft-venue-proto/tests/contract_session.rs::vpr_081_secret_leak_detected`

### VPR-082 check_session 检出含签名却未标 redacted 的请求

- 规则:[实现方必须遵守的契约](README.md#实现方必须遵守的契约)、[辅助类型](README.md#辅助类型)
- 级别:unit
- 里程碑:M3
- 前置:变体 `Fake<FakePerp, Mut::SignedNotRedacted>`(`dead_man_refresh` 的 REST 请求 `redacted = false`)
- 步骤:
  1. `check_session`
- 期望:
  - `Err(ContractFail{contract: 7})`
- 实现:`crates/hft-venue-proto/tests/contract_session.rs::vpr_082_redacted_flag_detected`

### VPR-083 check_session 检出心跳方式与 app_ping 不一致

- 规则:[`VenueProtocol`(静态分派)](README.md#venueprotocol静态分派)、[辅助类型](README.md#辅助类型)
- 级别:unit
- 里程碑:M3
- 前置:变体 `Fake<FakePerp, Mut::AppPingMissing>`(`heartbeat(Order).ping == AppText` 而 `app_ping(Order)` 返回 0)
- 步骤:
  1. `check_session`
- 期望:
  - `Err(ContractFail{contract: 24})`
- 实现:`crates/hft-venue-proto/tests/contract_session.rs::vpr_083_heartbeat_inconsistent`

### VPR-084 check_cum:累计量单调且等于去重后之和,查询带均价(契约 9)

- 规则:[实现方必须遵守的契约](README.md#实现方必须遵守的契约)
- 级别:unit
- 里程碑:M3
- 前置:`FakePerp`;一张单的成交推送 t1(0.1)、t2(0.2)、重复 t2、t3(0.3)乱序到达(t3 先于 t2);查询响应 `filled = 0.6`、`avgPx = 100.5`
- 步骤:
  1. `check_cum::<FakePerp>(samples)`
- 期望:
  - 通过。检查内容:`OrderEvent.cum_filled` 依次 0.1、0.4、0.6(重复的 t2 不产出成交增量),单调不减,最终 = 0.6;每条成交类事件 `avg_px > 0`;`QueryResult{cum_filled: 0.6, avg_px: 100.5}`
- 实现:`crates/hft-venue-proto/tests/contract_report.rs::vpr_084_check_cum_pass`

### VPR-085 check_cum 检出重复成交被累计两次

- 规则:[实现方必须遵守的契约](README.md#实现方必须遵守的契约)
- 级别:unit
- 里程碑:M3
- 前置:变体 `Fake<FakePerp, Mut::CumNoDedup>`
- 步骤:
  1. `check_cum`
- 期望:
  - `Err(ContractFail{contract: 9})`
- 实现:`crates/hft-venue-proto/tests/contract_report.rs::vpr_085_cum_dup_detected`

### VPR-086 check_cum 检出 cum_filled > 0 而 avg_px = 0 时仍返回 Ok 的实现

- 规则:[实现方必须遵守的契约](README.md#实现方必须遵守的契约)
- 级别:table
- 里程碑:M3
- 前置:查询响应 `filled = 0.6`、缺 `avgPx` 字段
- 实现:`crates/hft-venue-proto/tests/contract_report.rs::vpr_086_avg_px_zero`

| 行 | 实现 | 期望 |
|---|---|---|
| VPR-086.1 | `FakePerp` | `decode_report` 返回 `Err(DecodeErr::Shape)`;`check_cum` 通过 |
| VPR-086.2 | 变体 `Mut::ZeroAvgOk`(返回 `QueryResult{avg_px: 0}`) | `check_cum` 返回 `Err(ContractFail{contract: 9})` |

### VPR-087 check_default_caps:能力表与 stale_guard、dead_man、MARKET 一致(契约 10、11、15)

- 规则:[实现方必须遵守的契约](README.md#实现方必须遵守的契约)、[辅助类型](README.md#辅助类型)、[`VenueProtocol`(静态分派)](README.md#venueprotocol静态分派)
- 级别:unit
- 里程碑:M3
- 前置:`FakeSpot`(`stale_guard = None`、`dead_man = None`、`reduce_only = false`);`FakePerp`(`stale_guard = RecvWindow{default_ms: 5000}`、`dead_man = CountdownAccount{min_ms: 10_000, max_ms: 120_000, timeout_ms: 60_000, refresh_ms: 15_000, via: Ws}`)
- 步骤:
  1. `check_default_caps::<FakeSpot>()`;`check_default_caps::<FakePerp>()`
- 期望:
  - 两者通过。检查内容:`default_caps()` 的 stale_guard 与 `stale_guard()` 同变体同 `default_ms`;`dms` 与 `dead_man()` 同名变体,`dms_min_ms/max_ms/timeout_ms/refresh_ms` = 10_000/120_000/60_000/15_000;`default_caps()` 的市场 = `MARKET`;`FakeSpot` 编码带 `REDUCE_ONLY` 的 Place 返回 `Err(Unsupported(Place))`;`FakeSpot::dead_man_refresh` 返回 `Err(Unsupported(GwOp::DeadManRefresh))`;`rate_rules()` 非空且每条 `window_ms > 0`、`limit > 0`
- 实现:`crates/hft-venue-proto/tests/contract_caps.rs::vpr_087_check_default_caps_pass`

### VPR-088 check_default_caps 检出能力表与 stale_guard 不一致

- 规则:[实现方必须遵守的契约](README.md#实现方必须遵守的契约)
- 级别:unit
- 里程碑:M3
- 前置:变体 `Fake<FakePerp, Mut::StaleGuardMismatch>`(`default_caps` 写 `ExpireAt`)
- 步骤:
  1. `check_default_caps`
- 期望:
  - `Err(ContractFail{contract: 15})`
- 实现:`crates/hft-venue-proto/tests/contract_caps.rs::vpr_088_stale_guard_mismatch`

### VPR-089 倒计时死手 timeout_ms ≥ max(min_ms, 2 × refresh_ms) 的边界

- 规则:[辅助类型](README.md#辅助类型)、[实现方必须遵守的契约](README.md#实现方必须遵守的契约)
- 级别:table
- 里程碑:M3
- 前置:变体 `Mut::DeadMan(spec)`,`spec = CountdownPerInst{min_ms: 5_000, timeout_ms, refresh_ms, via: Ws}`;能力表与之同步
- 实现:`crates/hft-venue-proto/tests/helpers.rs::vpr_089_dead_man_boundary`

| 行 | `timeout_ms` / `refresh_ms` | 期望 `check_default_caps` |
|---|---|---|
| VPR-089.1 | 20_000 / 10_000 | 通过 |
| VPR-089.2 | 19_999 / 10_000 | `Err(ContractFail{contract: 15})` |
| VPR-089.3 | 20_001 / 10_000 | 通过 |
| VPR-089.4 | 5_000 / 2_000 | 通过(= min_ms) |
| VPR-089.5 | 4_999 / 2_000 | `Err(ContractFail{contract: 15})` |

### VPR-090 DeadManSpec 各变体与能力表 dms 字段的对应

- 规则:[辅助类型](README.md#辅助类型)
- 级别:table
- 里程碑:M3
- 前置:变体 `Mut::DeadMan(spec)`,能力表按 README 的对应写入;每行再构造一个故意错一个字段的能力表作反例
- 实现:`crates/hft-venue-proto/tests/helpers.rs::vpr_090_dead_man_caps`

| 行 | `dead_man()` | 期望能力表(min / max / timeout / refresh);反例 |
|---|---|---|
| VPR-090.1 | `None` | `DeadManCap::None`,四项 0;反例 `CountdownAccount` → `contract: 15` |
| VPR-090.2 | `CountdownPerInst{5_000, 20_000, 10_000}` | 5_000 / 0 / 20_000 / 10_000;反例 max = 1 → `contract: 15` |
| VPR-090.3 | `CountdownAccount{10_000, 120_000, 60_000, 15_000}` | 10_000 / 120_000 / 60_000 / 15_000;反例 refresh = 0 → `contract: 15` |
| VPR-090.4 | `OnDisconnect{window_ms: 30_000}` | 0 / 0 / 30_000 / 0;反例 timeout = 29_999 → `contract: 15` |
| VPR-090.5 | `Schedule{min_ahead_ms: 5_000, max_triggers_per_day: 10, timeout_ms: 60_000, refresh_ms: 20_000}` | 5_000 / 0 / 60_000 / 20_000;反例 min = 0 → `contract: 15` |

### VPR-091 check_default_caps 检出静默丢掉 REDUCE_ONLY 的现货实现

- 规则:[实现方必须遵守的契约](README.md#实现方必须遵守的契约)
- 级别:unit
- 里程碑:M3
- 前置:变体 `Fake<FakeSpot, Mut::DropReduceOnly>`(带 `REDUCE_ONLY` 的 Place 照常编码,不带该字段)
- 步骤:
  1. `check_default_caps`
- 期望:
  - `Err(ContractFail{contract: 11})`
- 实现:`crates/hft-venue-proto/tests/contract_caps.rs::vpr_091_spot_reduce_only_dropped`

### VPR-092 永续只在 oflags 带 REDUCE_ONLY 时填 reduce-only 字段

- 规则:[实现方必须遵守的契约](README.md#实现方必须遵守的契约)
- 级别:table
- 里程碑:M3
- 前置:`FakePerp`;Place 命令,其余字段固定
- 实现:`crates/hft-venue-proto/tests/contract_caps.rs::vpr_092_perp_reduce_only_field`

| 行 | `oflags` | 期望帧 |
|---|---|---|
| VPR-092.1 | `REDUCE_ONLY` | 含 `"ro":true` |
| VPR-092.2 | 0 | 不含 `"ro"` 键 |
| VPR-092.3 | `RISK_REDUCING`(不带 `REDUCE_ONLY`) | 不含 `"ro"` 键 |

### VPR-093 check_default_caps 检出 window_ms 为 0 的限速规则

- 规则:[辅助类型](README.md#辅助类型)
- 级别:unit
- 里程碑:M3
- 前置:变体 `Fake<FakePerp, Mut::ZeroWindowRule>`
- 步骤:
  1. `check_default_caps`
- 期望:
  - `Err(ContractFail{contract: 15})`,`detail` 含 `rate_rules`
- 实现:`crates/hft-venue-proto/tests/contract_caps.rs::vpr_093_zero_window_rule`

### VPR-094 check_fee_asset:实际费用与扣费资产,晚到费用走 FeeOnly(契约 12、19)

- 规则:[实现方必须遵守的契约](README.md#实现方必须遵守的契约)
- 级别:unit
- 里程碑:M3
- 前置:`FakePerp`(`fee_asset_rule = Quote`);样本:带费用的成交(USDT 0.05)、不带费用的成交、订单已 `Filled` 后到达的费用推送(USDT 0.02)、扣费资产为 BNB 的成交
- 步骤:
  1. `check_fee_asset::<FakePerp>(samples)`
- 期望:
  - 通过。检查内容:带费用成交的 `fee == 0.05`、`fee_asset == USDT`;不带费用的 `fee == 0`、`fee_asset == ASSET_NONE`;晚到费用产出 `OrderEvent{what: FeeOnly, qty: 0, fee: 0.02}`;全部样本的输出里没有 `qty == 0` 的 `Fill`/`PartialFill`;BNB 成交 `fee_rule_deviation == true` 且事件 `fee_asset == BNB`
- 实现:`crates/hft-venue-proto/tests/contract_report.rs::vpr_094_check_fee_asset_pass`

### VPR-095 check_fee_asset 检出用 qty = 0 的 Fill 表达晚到费用

- 规则:[实现方必须遵守的契约](README.md#实现方必须遵守的契约)
- 级别:unit
- 里程碑:M3
- 前置:变体 `Fake<FakePerp, Mut::FeeAsZeroFill>`
- 步骤:
  1. `check_fee_asset`
- 期望:
  - `Err(ContractFail{contract: 12})`
- 实现:`crates/hft-venue-proto/tests/contract_report.rs::vpr_095_zero_fill_detected`

### VPR-096 check_fee_asset 检出按规则猜费用或不报偏离的实现

- 规则:[实现方必须遵守的契约](README.md#实现方必须遵守的契约)
- 级别:table
- 里程碑:M3
- 前置:按行的变体
- 实现:`crates/hft-venue-proto/tests/contract_report.rs::vpr_096_fee_rule_violations`

| 行 | 变体 | 期望 |
|---|---|---|
| VPR-096.1 | `Mut::GuessFee`(无费用的成交按费率猜一个非零费用) | `Err(ContractFail{contract: 19})` |
| VPR-096.2 | `Mut::NoDeviationFlag`(BNB 成交不置 `fee_rule_deviation`) | `Err(ContractFail{contract: 19})` |
| VPR-096.3 | `Mut::RewriteFeeAsset`(把 BNB 改写成 USDT) | `Err(ContractFail{contract: 19})` |

### VPR-097 check_cid:client id 外形往返、epoch 来自 client id、无 id 时交运行时(契约 13)

- 规则:[实现方必须遵守的契约](README.md#实现方必须遵守的契约)、[`VenueProtocol`(静态分派)](README.md#venueprotocol静态分派)、[辅助类型](README.md#辅助类型)
- 级别:unit
- 里程碑:M3
- 前置:`FakePerp`(外形 = 前缀 `"hf"` + 规范 24 位);`VecSink`;两个 id:epoch 4 与 epoch 5
- 步骤:
  1. `check_cid::<FakePerp>(samples)`
- 期望:
  - 通过。检查内容:`render_cid` 写 26 B,末 24 B = `ClientOrderId::encode` 的文本;`parse_cid(render)` 往返相等;`parse_cid(b"web_123")` 与 `parse_cid` 校验位错的文本都为 `None`;带 epoch 4、5 的回报分别产出 `OrderEvent{epoch: 4}`、`{epoch: 5}`;不带 client id 的回报 `slot == SlotId::UNRESOLVED` 且 sink 收到 `exch_id(key, None)`;Ack 回报的 `ReportOutcome.req == Some(ReqRef{slot, gen, op: Place})`;同一 `GwCmd` 以 `WallClock{unix_ms: 0, …}` 与 `{unix_ms: 1, …}` 编码,只有时间字段不同;inst 不属于本 venue → `Err(UnknownInst)`;输出缓冲 8 B → `Err(FieldOverflow)`;需要 exch id 的 Amend 且 `exch_order_id = 0` → `Err(UnknownTarget)`
- 实现:`crates/hft-venue-proto/tests/contract_report.rs::vpr_097_check_cid_pass`

### VPR-098 check_cid 检出用当前 epoch 代替 client id 里 epoch 的实现

- 规则:[实现方必须遵守的契约](README.md#实现方必须遵守的契约)
- 级别:unit
- 里程碑:M3
- 前置:变体 `Fake<FakePerp, Mut::CurrentEpoch>`(`OrderEvent.epoch` 恒填 5)
- 步骤:
  1. `check_cid`
- 期望:
  - `Err(ContractFail{contract: 13})`,`sample` 指向 epoch 4 的回报
- 实现:`crates/hft-venue-proto/tests/contract_report.rs::vpr_098_current_epoch_detected`

### VPR-099 check_cid 检出外形里规范文本不在末尾或接受外来格式的实现

- 规则:[`VenueProtocol`(静态分派)](README.md#venueprotocol静态分派)、[实现方必须遵守的契约](README.md#实现方必须遵守的契约)
- 级别:table
- 里程碑:M3
- 前置:按行的变体
- 实现:`crates/hft-venue-proto/tests/contract_report.rs::vpr_099_cid_shape_violations`

| 行 | 变体 | 期望 |
|---|---|---|
| VPR-099.1 | `Mut::CidSuffix`(规范文本后加 `"x"`) | `Err(ContractFail{contract: 13})` |
| VPR-099.2 | `Mut::ParseAnything`(`parse_cid(b"web_123")` 返回 `Some`) | `Err(ContractFail{contract: 13})` |
| VPR-099.3 | `Mut::SystemClock`(编码时读系统时钟) | `Err(ContractFail{contract: 13})`,`detail` 含 `WallClock` |

### VPR-100 check_funding:按 funding_settle 能力产出,venue 从不合成(契约 14)

- 规则:[实现方必须遵守的契约](README.md#实现方必须遵守的契约)
- 级别:table
- 里程碑:M3
- 前置:`FakePerp` 以能力表的 `funding_settle` 取值分三种配置;样本:带 symbol 的实收付推送(inst 201,−1.25 USDT,结算时刻 T)、只有余额变化的资金费推送(−1.25 USDT,时刻 T);`T = 1_700_006_400_000_000_000`
- 实现:`crates/hft-venue-proto/tests/contract_report.rs::vpr_100_check_funding_pass`

| 行 | `funding_settle` | 期望(`check_funding` 通过,且以下输出) |
|---|---|---|
| VPR-100.1 | `PerSymbolPush` | `FundingSettle{inst: 201, amount: −1.25, settle_ts_ns: T, synth: 0}` |
| VPR-100.2 | `BalanceOnlyPush` | `FundingSettle{inst: INST_NONE, amount: −1.25, settle_ts_ns: T, synth: 0}` |
| VPR-100.3 | `RateOnly` | 私有推送零条 `FundingSettle`;费率推送仍经 `MdSink::funding` 产出 `Funding`;独立 warm `FundingHistoryProtocol` 查询返回账单,验收见各 venue 的资金费历史用例 |

### VPR-101 check_funding 检出 venue 自行合成或在 RateOnly 下产出的实现

- 规则:[实现方必须遵守的契约](README.md#实现方必须遵守的契约)
- 级别:table
- 里程碑:M3
- 前置:按行的变体
- 实现:`crates/hft-venue-proto/tests/contract_report.rs::vpr_101_funding_violations`

| 行 | 变体 | 期望 |
|---|---|---|
| VPR-101.1 | `Mut::SynthFlag`(推送型写 `synth: 1`) | `Err(ContractFail{contract: 14})` |
| VPR-101.2 | `Mut::RateOnlyEmits`(`RateOnly` 下仍产出 `FundingSettle`) | `Err(ContractFail{contract: 14})` |
| VPR-101.3 | `Mut::WrongSettleTs`(写本机时间而非交易所结算时刻) | `Err(ContractFail{contract: 14})` |

### VPR-102 check_account_config:每个 mismatch 位都能被触发(契约 16)

- 规则:[实现方必须遵守的契约](README.md#实现方必须遵守的契约)
- 级别:table
- 里程碑:M3
- 前置:`FakeSpot`、`FakePerp`;`AccountMeta.venue_uid = "U1"`;能力表要求单向持仓、全仓、多资产关、`fee_asset_rule = Quote`;每行一组 Config 响应(其余项满足要求)
- 实现:`crates/hft-venue-proto/tests/contract_acct.rs::vpr_102_check_account_config_bits`

| 行 | 响应差异 | 期望 `AccountConfigReport` |
|---|---|---|
| VPR-102.1 | 全部满足(永续,2 个可交易 inst) | `mismatches == 0`;`n_lev == 2`,杠杆条目 inst 201、202 |
| VPR-102.2 | 双向持仓 | `POS_MODE` |
| VPR-102.3 | inst 201 全仓、202 逐仓 | `MARGIN_MODE`;`margin_mode == Mixed` |
| VPR-102.4 | 多资产开 | `MULTI_ASSET` |
| VPR-102.5 | 平台币抵扣开 | `FEE_RULE` |
| VPR-102.6 | 组合保证金账户 | `ACCT_TYPE` |
| VPR-102.7 | 现货:自动借币开 | `AUTO_BORROW`;`n_lev == 0`;`margin_mode == NotApplicable` |
| VPR-102.8 | 现货:借币开关字段缺失 | `UNREADABLE` |
| VPR-102.9 | key 带提现权限 | `KEY_PERM` |
| VPR-102.10 | UID = `"U2"` | `UID`;`venue_uid == "U2"` |
| VPR-102.11 | 杠杆字段读不出 | 该条目 `leverage == 0`;`mismatches` 不因杠杆置位 |
| VPR-102.12 | 整组样本交给 `check_account_config` | 通过 |

### VPR-103 check_account_config 检出读不出却放行或漏判 Mixed 的实现

- 规则:[实现方必须遵守的契约](README.md#实现方必须遵守的契约)
- 级别:table
- 里程碑:M3
- 前置:按行的变体
- 实现:`crates/hft-venue-proto/tests/contract_acct.rs::vpr_103_account_config_violations`

| 行 | 变体 | 期望 |
|---|---|---|
| VPR-103.1 | `Mut::UnreadableAsDefault`(缺字段按默认值放行) | `Err(ContractFail{contract: 16})` |
| VPR-103.2 | `Mut::NoMixed`(部分逐仓时报 `Cross`) | `Err(ContractFail{contract: 16})` |
| VPR-103.3 | `Mut::SkipLeverageItem`(少报一个可交易 inst 的杠杆条目) | `Err(ContractFail{contract: 16})` |
| VPR-103.4 | `Mut::NoConfigRecheck`(配置变化推送的 `hint` 为 `None`) | `Err(ContractFail{contract: 16})` |

### VPR-104 check_margin:保证金率由两个字段算出并向上取整,ADL 档归一化(契约 17)

- 规则:[实现方必须遵守的契约](README.md#实现方必须遵守的契约)
- 级别:table
- 里程碑:M3
- 前置:`FakePerp` 的 Margin 序列(`margin_data = Poll`)与推送(`margin_data = Push`)各一套样本;`Rate` 指数 −9
- 实现:`crates/hft-venue-proto/tests/contract_acct.rs::vpr_104_check_margin_pass`

| 行 | 输入 | 期望 |
|---|---|---|
| VPR-104.1 | `margin_balance = 3`、`maint_margin = 1`、交易所比率字段 `0.5` | `margin_ratio == Rate(333_333_334)` |
| VPR-104.2 | `margin_balance = 4`、`maint_margin = 1` | `Rate(250_000_000)` |
| VPR-104.3 | `margin_balance = 0` | `Rate::MAX` |
| VPR-104.4 | `margin_balance = -5` | `Rate::MAX` |
| VPR-104.5 | ADL 第 2 档 / 最高 5 档;第 2 档 / 最高 3 档;字段缺失 | `adl_quantile` 40;66;255 |
| VPR-104.6 | 无强平价 | `liq_px == 0` |
| VPR-104.7 | Poll:`acct_finish(Margin)` | 1 条 `MarginReport` + 每个持仓 inst 1 条 `PositionReport` |
| VPR-104.8 | Push:先账户推送、再持仓推送 | 第二次产出的 `MarginReport` 同时含账户级字段与持仓条目 |
| VPR-104.9 | `FakeSpot::acct_queries(Margin)` | 空;任何样本不产出 `MarginReport` |
| VPR-104.10 | 保证金预警推送 | `ReportOutcome.hint == MarginNow` |
| VPR-104.11 | 整组样本交给 `check_margin` | 通过 |

### VPR-105 check_margin 检出采用交易所比率或不按规则取整的实现

- 规则:[实现方必须遵守的契约](README.md#实现方必须遵守的契约)
- 级别:table
- 里程碑:M3
- 前置:按行的变体
- 实现:`crates/hft-venue-proto/tests/contract_acct.rs::vpr_105_margin_violations`

| 行 | 变体 | 期望 |
|---|---|---|
| VPR-105.1 | `Mut::ExchangeRatio`(直接用交易所比率字段) | `Err(ContractFail{contract: 17})` |
| VPR-105.2 | `Mut::RatioRoundDown` | `Err(ContractFail{contract: 17})`(VPR-104.1 得 333_333_333) |
| VPR-105.3 | `Mut::ZeroBalanceZeroRatio`(余额 0 时写 0) | `Err(ContractFail{contract: 17})` |
| VPR-105.4 | `Mut::AdlRound`(四舍五入,2/3 得 67) | `Err(ContractFail{contract: 17})` |

### VPR-106 check_external_fill:强平、ADL、人工与未知来源按顺序判定(契约 18)

- 规则:[实现方必须遵守的契约](README.md#实现方必须遵守的契约)
- 级别:table
- 里程碑:M3
- 前置:`FakePerp`(`own_liquidation_marker` = client id 前缀 `"liq_"`;声明"我方订单的成交推送必带 client id");每行一条成交推送
- 实现:`crates/hft-venue-proto/tests/contract_report.rs::vpr_106_check_external_fill_pass`

| 行 | 成交推送 | 期望 |
|---|---|---|
| VPR-106.1 | client id `"liq_123"`,类别强平 | `ExternalFill{cause: Liquidation}`;无 `OrderEvent` |
| VPR-106.2 | 类别 ADL,无 client id | `ExternalFill{cause: Adl}` |
| VPR-106.3 | 带 marker 且 client id 恰好可被 `parse_cid` 解析 | `ExternalFill{cause: Liquidation}`(marker 优先) |
| VPR-106.4 | client id 为本系统格式 | `OrderEvent`,无 `ExternalFill` |
| VPR-106.5 | client id `"web_9"`(网页来源) | `ExternalFill{cause: Manual}` |
| VPR-106.6 | client id `"zz"` | `ExternalFill{cause: Unknown}` |
| VPR-106.7 | 无 client id(本所声明必带) | `ExternalFill{cause: Unknown}` |
| VPR-106.8 | 成交 id 为 −1 的强平 | `ExternalFill.trade_id == 0`;方向、数量、价格、费用、`exch_order_id`、`ex_ts_ns` 与报文一致 |
| VPR-106.9 | 整组样本交给 `check_external_fill` | 通过 |

### VPR-107 check_external_fill 检出先解析 client id 后看 marker 的实现

- 规则:[实现方必须遵守的契约](README.md#实现方必须遵守的契约)
- 级别:unit
- 里程碑:M3
- 前置:变体 `Fake<FakePerp, Mut::MarkerAfterCid>`
- 步骤:
  1. `check_external_fill`(样本含 VPR-106.3)
- 期望:
  - `Err(ContractFail{contract: 18})`
- 实现:`crates/hft-venue-proto/tests/contract_report.rs::vpr_107_marker_order_detected`

### VPR-108 check_unknown_events:可忽略与未知事件返回 Ok + skip(契约 20)

- 规则:[实现方必须遵守的契约](README.md#实现方必须遵守的契约)
- 级别:table
- 里程碑:M3
- 前置:`FakePerp`;`ignorable_user_events = ["listStatus"]`;按行一条用户流消息
- 实现:`crates/hft-venue-proto/tests/contract_report.rs::vpr_108_check_unknown_events_pass`

| 行 | 事件 | 期望 |
|---|---|---|
| VPR-108.1 | `"listStatus"` | `classify_report` = `Ignorable`;`Ok(ReportOutcome{skip: Listed, events: 0})`;sink 记录数 0 |
| VPR-108.2 | `"brandNewEvent"` | `classify_report` = `Unknown`;`Ok(ReportOutcome{skip: Unknown, events: 0})` |
| VPR-108.3 | 已知订单事件,数量字段为 `"x"` | `Err(DecodeErr::Number)` |
| VPR-108.4 | refdata 追加 `"brandNewEvent"` 到可忽略名单后再喂 VPR-108.2 | `classify_report` 仍为 `Unknown`;`decode_report` 返回 `skip: Listed` |
| VPR-108.5 | 整组样本交给 `check_unknown_events` | 通过 |

### VPR-109 check_unknown_events 检出把未知事件当解码失败的实现

- 规则:[实现方必须遵守的契约](README.md#实现方必须遵守的契约)
- 级别:unit
- 里程碑:M3
- 前置:变体 `Fake<FakePerp, Mut::UnknownIsError>`
- 步骤:
  1. `check_unknown_events`
- 期望:
  - `Err(ContractFail{contract: 20})`
- 实现:`crates/hft-venue-proto/tests/contract_report.rs::vpr_109_unknown_error_detected`

### VPR-110 check_dedup:同一成交与资金费推送至多产出一次(契约 22)

- 规则:[实现方必须遵守的契约](README.md#实现方必须遵守的契约)、[辅助类型](README.md#辅助类型)
- 级别:table
- 里程碑:M3
- 前置:`FakePerp`;每行一个新 `GwCtx`
- 实现:`crates/hft-venue-proto/tests/contract_report.rs::vpr_110_check_dedup_pass`

| 行 | 输入序列 | 期望 |
|---|---|---|
| VPR-110.1 | 同一成交帧(inst 201,id 77)两次 | 1 条成交增量 |
| VPR-110.2 | inst 201 与 202 的成交都带 id 77 | 2 条 |
| VPR-110.3 | 成交 id 0 的强平(累计 0.5)两次 | 1 条 `ExternalFill` |
| VPR-110.4 | 同一订单成交 id 为 −1 的两笔,累计 0.5 与 0.8 | 2 条 |
| VPR-110.5 | 同一晚到费用推送两次 | 1 条 `FeeOnly` |
| VPR-110.6 | 同一资金费推送两次 | 1 条 `FundingSettle` |
| VPR-110.7 | 同一结算时刻、`disc` 不同的两条资金费推送 | 2 条 |
| VPR-110.8 | 余额推送两次 | 2 条 `BalanceReport`(只做校验,不去重) |
| VPR-110.9 | 整组样本交给 `check_dedup` | 通过 |

### VPR-111 check_dedup 检出去重键不带 inst、以非正 id 入环或忽略 disc 的实现

- 规则:[实现方必须遵守的契约](README.md#实现方必须遵守的契约)
- 级别:table
- 里程碑:M3
- 前置:按行的变体
- 实现:`crates/hft-venue-proto/tests/contract_report.rs::vpr_111_dedup_violations`

| 行 | 变体 | 期望 |
|---|---|---|
| VPR-111.1 | `Mut::KeyWithoutInst`(VPR-110.2 只产出 1 条) | `Err(ContractFail{contract: 22})` |
| VPR-111.2 | `Mut::NonPositiveIdKey`(VPR-110.4 只产出 1 条) | `Err(ContractFail{contract: 22})` |
| VPR-111.3 | `Mut::IgnoreDisc`(VPR-110.7 只产出 1 条) | `Err(ContractFail{contract: 22})` |
| VPR-111.4 | `Mut::NoDedup`(VPR-110.1 产出 2 条) | `Err(ContractFail{contract: 22})` |

### VPR-112 check_identity:每把交易 key 的 UID 都被核对(契约 23)

- 规则:[实现方必须遵守的契约](README.md#实现方必须遵守的契约)、[`VenueProtocol`(静态分派)](README.md#venueprotocol静态分派)
- 级别:table
- 里程碑:M3
- 前置:`FakePerp`(会话凭证与 REST 凭证不同:`session_identity_request` 返回 `Ok(true)`);`FakeSpot`(同一把:返回 `Ok(false)`);UID 样本 `"U1"`
- 实现:`crates/hft-venue-proto/tests/contract_acct.rs::vpr_112_check_identity_pass`

| 行 | 产品线 | 期望 |
|---|---|---|
| VPR-112.1 | `FakePerp` | `session_identity_request` 返回 `Ok(true)` 且 `out.redacted == true`;`decode_identity(200, body)` 写出 `"U1"` 右补 0 的 24 B,与 Config 序列解出的 UID 相同 |
| VPR-112.2 | `FakeSpot` | 返回 `Ok(false)`;Config 报告的 `uid_keys` 同时含 `SESSION` 与 `REST` |
| VPR-112.3 | `FakePerp`,`decode_identity(401, body)` | `Err(DecodeErr::Shape)` |
| VPR-112.4 | 两者交给 `check_identity` | 通过 |

### VPR-113 check_identity 检出会话 UID 解码与 Config 不一致的实现

- 规则:[实现方必须遵守的契约](README.md#实现方必须遵守的契约)
- 级别:unit
- 里程碑:M3
- 前置:变体 `Fake<FakePerp, Mut::TruncateUid>`(`decode_identity` 只写前 1 B)
- 步骤:
  1. `check_identity`
- 期望:
  - `Err(ContractFail{contract: 23})`
- 实现:`crates/hft-venue-proto/tests/contract_acct.rs::vpr_113_identity_mismatch`

### VPR-114 check_bbo_refresh:BBO 探测方式与补帧实现自洽(契约 24)

- 规则:[实现方必须遵守的契约](README.md#实现方必须遵守的契约)、[`VenueProtocol`(静态分派)](README.md#venueprotocol静态分派)
- 级别:table
- 里程碑:M3
- 前置:`FakePerp`(`bbo_probe = AppText`,`bbo_snapshot_request` 返回 `Ok(true)`);`FakeSpot`(默认实现)
- 实现:`crates/hft-venue-proto/tests/contract_md.rs::vpr_114_check_bbo_refresh_pass`

| 行 | 产品线 | 期望 |
|---|---|---|
| VPR-114.1 | `FakePerp`(`BBO_SNAPSHOT_BATCH = 2`) | `app_ping(Md, …)` 返回 > 0;`bbo_snapshot_request(&[201, 202])` 返回 `Ok(true)` 且请求非空;对响应样本 `decode_bbo_snapshot(&[201, 202], …)` 返回 `Ok(2)`,产出 2 条 `Bbo`,`update_id` 取响应里的序号 |
| VPR-114.5 | `FakePerp`,`bbo_snapshot_request(&[201, 202, 203])`(3 > 2) | `Err(ProtoErr::BufFull)`,`out` 不变 |
| VPR-114.6 | `FakePerp`,响应样本不带 symbol,`decode_bbo_snapshot(&[202], …)` | 1 条 `Bbo`,`h.inst == 202` |
| VPR-114.2 | `FakePerp`,响应样本无序号字段 | `Bbo.update_id == 0` |
| VPR-114.3 | `FakeSpot` | `bbo_probe == WsPing`;`bbo_snapshot_request` 返回 `Ok(false)` |
| VPR-114.4 | 两者交给 `check_bbo_refresh` | 通过 |

### VPR-115 check_bbo_refresh 检出声明补帧却没有解码实现、或应用层探测没有帧的实现

- 规则:[实现方必须遵守的契约](README.md#实现方必须遵守的契约)
- 级别:table
- 里程碑:M3
- 前置:按行的变体
- 实现:`crates/hft-venue-proto/tests/contract_md.rs::vpr_115_bbo_refresh_violations`

| 行 | 变体 | 期望 |
|---|---|---|
| VPR-115.1 | `Mut::RefreshNoDecode`(`Ok(true)` 而 `decode_bbo_snapshot` 用默认) | `Err(ContractFail{contract: 24})` |
| VPR-115.2 | `Mut::AppProbeNoFrame`(`bbo_probe = AppText` 而 `app_ping(Md)` 返回 0) | `Err(ContractFail{contract: 24})` |
| VPR-115.3 | `Mut::BatchOverAccepted`(输入超过 `BBO_SNAPSHOT_BATCH` 仍返回 `Ok(true)`) | `Err(ContractFail{contract: 24})` |
| VPR-115.4 | `Mut::InstFromBody`(不按传入的 `insts` 定 inst,无 symbol 的响应产出 `inst = 0`) | `Err(ContractFail{contract: 24})` |

### VPR-116 check_snapshot_isolation:快照解码不依赖行情解码的状态(契约 25)

- 规则:[实现方必须遵守的契约](README.md#实现方必须遵守的契约)
- 级别:unit
- 里程碑:M3
- 前置:`FakePerp`;BBO 补帧响应与 1000 档深度快照响应;10⁴ 条行情样本
- 步骤:
  1. `check_snapshot_isolation::<FakePerp>(samples)`
- 期望:
  - 通过。检查内容:同一快照响应在全新 `FeedCtx` 与"已解码 10⁴ 条行情的 `FeedCtx`"上的输出逐字节相同;在一个 `FeedCtx` 上穿插快照解码前后,`decode_md` 对同一行情序列的输出逐字节相同;`decode_snapshot` 返回的 `SnapshotInfo{inst: 201, snap_id: 快照序号, levels: 1000}`
- 实现:`crates/hft-venue-proto/tests/contract_md.rs::vpr_116_check_snapshot_isolation_pass`

### VPR-117 check_snapshot_isolation 检出共享状态的实现

- 规则:[实现方必须遵守的契约](README.md#实现方必须遵守的契约)
- 级别:table
- 里程碑:M3
- 前置:按行的变体
- 实现:`crates/hft-venue-proto/tests/contract_md.rs::vpr_117_snapshot_isolation_violations`

| 行 | 变体 | 期望 |
|---|---|---|
| VPR-117.1 | `Mut::SnapshotReadsMdState`(快照解码读 `decode_md` 写的最近价) | `Err(ContractFail{contract: 25})` |
| VPR-117.2 | `Mut::SnapshotWritesMdState`(快照解码改写 `decode_md` 读的状态) | `Err(ContractFail{contract: 25})` |

### VPR-118 check_topn:分档快照路由与输出(契约 26)

- 规则:[实现方必须遵守的契约](README.md#实现方必须遵守的契约)、[辅助类型](README.md#辅助类型)
- 级别:unit
- 里程碑:M3
- 前置:`FakePerp`(`routes()`:id 0 `Bbo`(`race = true`)、id 1 `DepthTopN`(`topn_levels = [5, 20]`)、id 2 `Md`)
- 步骤:
  1. `check_topn::<FakePerp>(samples)`
- 期望:
  - 通过。检查内容:恰有一条 `DepthTopN` 路由,`topn_levels` 非空且严格升序;非 `DepthTopN` 路由的 `topn_levels` 为空;只有 `Bbo` 路由 `race == true`;分档快照样本只产出 `L2Snapshot{origin = TopN}`、零条 `L2Batch`;`resync_method == None`;`seq_rule ∈ {Snapshot, ByTime}`;同一样本喂两次,第二次按 `seq_rule` 判为重复(`Snapshot`:`last_id` 不大于已转发)
- 实现:`crates/hft-venue-proto/tests/contract_md.rs::vpr_118_check_topn_pass`

### VPR-119 check_topn 检出路由表或输出不合规的实现

- 规则:[实现方必须遵守的契约](README.md#实现方必须遵守的契约)、[辅助类型](README.md#辅助类型)
- 级别:table
- 里程碑:M3
- 前置:按行的变体
- 实现:`crates/hft-venue-proto/tests/contract_md.rs::vpr_119_topn_violations`

| 行 | 变体 | 期望 |
|---|---|---|
| VPR-119.1 | 两条 `DepthTopN` 路由 | `Err(ContractFail{contract: 26})` |
| VPR-119.2 | `topn_levels = [20, 5]` | `Err(ContractFail{contract: 26})` |
| VPR-119.3 | 分档快照产出 `L2Batch` | `Err(ContractFail{contract: 26})` |
| VPR-119.4 | `resync_method = RestSnapshot` | `Err(ContractFail{contract: 26})` |
| VPR-119.5 | `Md` 路由 `race = true` | `Err(ContractFail{contract: 26})` |
| VPR-119.6 | `Md` 路由 `topn_levels = [5]` | `Err(ContractFail{contract: 26})` |

### VPR-120 check_acct_isolation:账户查询与回报解码互不依赖(契约 27)

- 规则:[实现方必须遵守的契约](README.md#实现方必须遵守的契约)
- 级别:unit
- 里程碑:M3
- 前置:`FakePerp`(`margin_data = Push`,另有 `push_fallback` 请求);账户查询响应序列;10⁴ 条回报样本
- 步骤:
  1. `check_acct_isolation::<FakePerp>(samples)`
- 期望:
  - 通过。检查内容:同一组响应在全新 `AcctCtx` 与"已运行 10³ 个序列的 `AcctCtx`"上产出的报告逐字节相同(该所声明跨序列沿用的字段除外);穿插 10⁴ 条 `decode_report` 前后,账户查询输出不变;`push_fallback` 序列产出一份独立完整的 `MarginReport`,之后下一条推送产出的报告与未执行 fallback 时逐字节相同;`AcctCtx::default()` 构造的上下文可直接使用
- 实现:`crates/hft-venue-proto/tests/contract_acct.rs::vpr_120_check_acct_isolation_pass`

### VPR-121 check_acct_isolation 检出共享状态的实现

- 规则:[实现方必须遵守的契约](README.md#实现方必须遵守的契约)
- 级别:table
- 里程碑:M3
- 前置:按行的变体
- 实现:`crates/hft-venue-proto/tests/contract_acct.rs::vpr_121_acct_isolation_violations`

| 行 | 变体 | 期望 |
|---|---|---|
| VPR-121.1 | `Mut::AcctUsesGwState`(`acct_finish` 读 `decode_report` 留下的推送缓存) | `Err(ContractFail{contract: 27})` |
| VPR-121.2 | `Mut::FallbackMergesPush`(fallback 结果写进推送缓存) | `Err(ContractFail{contract: 27})` |
| VPR-121.3 | `Mut::AcctCarriesState`(未声明的跨序列状态) | `Err(ContractFail{contract: 27})` |

### VPR-122 假协议实现 trait 的每个方法,GoldenDriver 对每个方法有金样

- 规则:[`VenueProtocol`(静态分派)](README.md#venueprotocol静态分派)、[解码输出 sink](README.md#解码输出-sink)
- 级别:table
- 里程碑:M3
- 前置:`FakePerp`;期望文件 `crates/hft-venue-proto/tests/data/fake/<方法>.expected`
- 实现:`crates/hft-venue-proto/tests/trait_methods.rs::vpr_122_every_method_golden`

| 行 | 方法或常量 | 期望 |
|---|---|---|
| VPR-122.1 | `NAME`、`VENUE`、`MARKET`、`ORDER_FRAME_CAP`、`CANCEL_ALL_SWEEP`、`BBO_SNAPSHOT_BATCH` | `"fake-perp"`、测试 venue 号、`LinearPerp`、8192、true、2 |
| VPR-122.2 | `default_caps`、`rate_rules`、`stale_guard`、`dead_man` | 与期望文件逐字段相同 |
| VPR-122.3 | `bootstrap`、`routes`、`endpoint`、`heartbeat`、`app_ping`、`bbo_probe` | 同上 |
| VPR-122.4 | `login`、`login_result`(`Ok`、`Rejected(1001)`、`Continue`、`NotMine` 各一) | 同上 |
| VPR-122.5 | `subscribe`(3 个 `SubSpec` 分 2 批)、`private_subscribe`(订阅与退订) | 同上 |
| VPR-122.6 | `session_identity_request`、`decode_identity` | 同上 |
| VPR-122.7 | `classify`、`peek`、`seq_rule`、`decode_md`、`control`、`resync_method`、`align_first` | 同上 |
| VPR-122.8 | `snapshot_request`、`decode_snapshot`、`bbo_snapshot_request`、`decode_bbo_snapshot` | 同上 |
| VPR-122.9 | `build_templates`、`order_channel`、`encode`、`encode_batch`、`encode_rest`、`session_keepalive_request`、`decode_rest`、`render_cid`、`parse_cid` | 同上 |
| VPR-122.10 | `classify_report`、`decode_report` | 同上 |
| VPR-122.11 | `acct_queries(Config)`、`acct_queries(Margin)`、`acct_request`、`acct_decode`、`acct_finish` | 同上;`acct_queries` 中 `every` ≥ 1、`step` 在序列内两两不同 |
| VPR-122.12 | `dead_man_refresh`、`needs_per_order_signing` | 同上 |
| VPR-122.13 | `ro_request`、`ro_decode`(五种 `RoKind`) | 同上 |

### VPR-123 查询"订单不存在"产出 QueryResult 的 NotFound 形态,不产出 Reject(契约 10)

- 规则:[实现方必须遵守的契约](README.md#实现方必须遵守的契约)
- 级别:table
- 里程碑:M3
- 前置:`FakePerp`(`stale_guard = RecvWindow{default_ms: 5000}`);查询响应"订单不存在"
- 实现:`crates/hft-venue-proto/tests/contract_report.rs::vpr_123_query_not_found_shape`

| 行 | 实现 | 期望 |
|---|---|---|
| VPR-123.1 | `FakePerp` | 产出 `OrderEvent{what: QueryResult}` 的 NotFound 形态,`cum_filled == 0`;无 `Reject`;`check_cum` 通过 |
| VPR-123.2 | 变体 `Mut::NotFoundAsReject` | `check_cum` 返回 `Err(ContractFail{contract: 10})` |

### VPR-124 check_detail:QueryResult 带订单状态码,已成交/不存在类撤改拒绝带 GONE(契约 28)

- 规则:[实现方必须遵守的契约](README.md#实现方必须遵守的契约)
- 级别:table
- 里程碑:M3
- 前置:`FakePerp`;`codes.rs` 把 `-2011` 登记为确定拒绝且属 `GONE` 类,`-2010` 为确定拒绝、非 `GONE`;每行一条回报
- 实现:`crates/hft-venue-proto/tests/contract_report.rs::vpr_124_check_detail_pass`

| 行 | 回报 | 期望 `OrderEvent` |
|---|---|---|
| VPR-124.1 | 查询响应,状态 `PARTIALLY_FILLED` | `what = QueryResult`,`detail = 2` |
| VPR-124.2 | 查询响应"订单不存在" | `QueryResult`,`detail = 7` |
| VPR-124.3 | 撤单响应 `-2011` | `CancelReject`,`detail = GONE` |
| VPR-124.4 | 改单响应 `-2011` | `AmendReject`,`detail = GONE` |
| VPR-124.5 | 撤单响应 `-2010` | `CancelReject`,`detail = 0` |
| VPR-124.6 | 成交推送 | `Fill`,`detail = 0` |
| VPR-124.7 | 整组样本交给 `check_detail` | 通过 |

### VPR-125 check_detail 检出漏填或错填 detail 的实现

- 规则:[实现方必须遵守的契约](README.md#实现方必须遵守的契约)
- 级别:table
- 里程碑:M3
- 前置:按行的变体
- 实现:`crates/hft-venue-proto/tests/contract_report.rs::vpr_125_detail_violations`

| 行 | 变体 | 期望 |
|---|---|---|
| VPR-125.1 | `Mut::NoQueryStatus`(`QueryResult.detail` 恒 0) | `Err(ContractFail{contract: 28})` |
| VPR-125.2 | `Mut::GoneOnAll`(`-2010` 也填 `GONE`) | `Err(ContractFail{contract: 28})` |
| VPR-125.3 | `Mut::NoGone`(`-2011` 填 0) | `Err(ContractFail{contract: 28})` |

### VPR-126 check_rest:REST 通道的 op 只经 encode_rest 编码,REST 响应按 RestOrigin 定槽(契约 29)

- 规则:[实现方必须遵守的契约](README.md#实现方必须遵守的契约)、[`VenueProtocol`(静态分派)](README.md#venueprotocol静态分派)
- 级别:table
- 里程碑:M3
- 前置:`FakePerp`(`order_channel(Query) = Rest`,其余 Ws);`RestOrigin{what: Order, req: Some(ReqRef{slot: 42, gen: 7, op: Query}), inst: 201}`;查单响应样本不带 client id
- 实现:`crates/hft-venue-proto/tests/contract_rest.rs::vpr_126_check_rest_pass`

| 行 | 调用 | 期望 |
|---|---|---|
| VPR-126.1 | `encode_rest(Query{target_slot: 42})` | `Ok`;`method = GET`;`redacted = true` |
| VPR-126.2 | `encode(Query{target_slot: 42})` | `Err(EncodeErr::Unsupported(GwOp::Query))`(同一 op 只走一种通道) |
| VPR-126.3 | `encode_rest(Place)` | `Err(EncodeErr::Unsupported(GwOp::Place))` |
| VPR-126.4 | `decode_rest(origin, 200, 查单响应)` | 一条 `OrderEvent{what: QueryResult, slot: 42, gen: 7}`;`ReportOutcome.req == Some(ReqRef{42, 7, Query})` |
| VPR-126.5 | `decode_rest(RestOrigin{what: Sweep, …}, 200, 挂单列表:本账户 2 单 + 外来 1 单)` | 2 条 `QueryResult`(槽取自各自 client id);外来单不产出 |
| VPR-126.6 | `decode_rest(RestOrigin{what: Keepalive, …}, 200, …)`;`decode_rest(DeadMan, 200, …)` | 两者事件数 0,`ctl = None` |
| VPR-126.7 | `session_keepalive_request(…)` | `Ok(true)`;请求带签名或 API key 头,`redacted = true` |
| VPR-126.8 | 整组样本交给 `check_rest` | 通过 |

### VPR-127 check_rest 检出通道不一致或按响应内容猜槽的实现

- 规则:[实现方必须遵守的契约](README.md#实现方必须遵守的契约)
- 级别:table
- 里程碑:M3
- 前置:按行的变体
- 实现:`crates/hft-venue-proto/tests/contract_rest.rs::vpr_127_rest_violations`

| 行 | 变体 | 期望 |
|---|---|---|
| VPR-127.1 | `Mut::BothChannels`(`order_channel(Query) = Rest` 而 `encode(Query)` 也返回 `Ok`) | `Err(ContractFail{contract: 29})` |
| VPR-127.2 | `Mut::IgnoreOrigin`(查单响应产出 `slot = UNRESOLVED`) | `Err(ContractFail{contract: 29})` |
| VPR-127.3 | `Mut::SweepForeign`(挂单列表中的外来单也产出 `QueryResult`) | `Err(ContractFail{contract: 29})` |
| VPR-127.4 | `Mut::RestNotRedacted`(`encode_rest` 的签名请求 `redacted = false`) | `Err(ContractFail{contract: 7})` |

### VPR-128 check_ctl:时钟越界、限速、用户流终止与维护预告置对应提示(契约 30)

- 规则:[实现方必须遵守的契约](README.md#实现方必须遵守的契约)、[辅助类型](README.md#辅助类型)
- 级别:table
- 里程碑:M3
- 前置:`FakePerp`;`codes.rs`:`-1021` 确定拒绝 + `ClockSkew`,`-1015` 确定拒绝 + `RateLimited`;每行一条回报或 REST 响应
- 实现:`crates/hft-venue-proto/tests/contract_report.rs::vpr_128_check_ctl_pass`

| 行 | 输入 | 期望 |
|---|---|---|
| VPR-128.1 | 下单响应 `-1021` | 一条 `Reject{reject_code = 0xFFFF_FC03}`;`ctl = ClockSkew` |
| VPR-128.2 | 下单响应 `-1015` | 一条 `Reject`;`ctl = RateLimited` |
| VPR-128.3 | REST 查单响应 HTTP 429 | 无事件;`ctl = RateLimited` |
| VPR-128.4 | 用户流事件"流已终止" | 无事件;`ctl = UserStreamEnded` |
| VPR-128.5 | 会话维护响应"会话不存在" | 无事件;`ctl = UserStreamEnded` |
| VPR-128.6 | 私有连接上的维护预告 | 无事件;`ctl = ServerShutdown` |
| VPR-128.7 | 普通成交推送 | `ctl = None` |
| VPR-128.8 | 整组样本交给 `check_ctl` | 通过;sink 未收到任何 `ConnState` |

### VPR-129 check_ctl 检出漏置提示或以提示代替拒单的实现

- 规则:[实现方必须遵守的契约](README.md#实现方必须遵守的契约)
- 级别:table
- 里程碑:M3
- 前置:按行的变体
- 实现:`crates/hft-venue-proto/tests/contract_report.rs::vpr_129_ctl_violations`

| 行 | 变体 | 期望 |
|---|---|---|
| VPR-129.1 | `Mut::NoClockSkew`(`-1021` 的 `ctl = None`) | `Err(ContractFail{contract: 30})` |
| VPR-129.2 | `Mut::CtlInsteadOfReject`(`-1015` 只置 `RateLimited`、不产出 `Reject`) | `Err(ContractFail{contract: 30})` |
| VPR-129.3 | `Mut::StreamEndAsDecodeErr`(流终止事件返回 `DecodeErr`) | `Err(ContractFail{contract: 30})` |

### VPR-130 doc-check 要求 core venue README 含全部必备节

- 规则:[每个 venue README 的必备节](README.md#每个-venue-readme-的必备节)、[实现方必须遵守的契约](README.md#实现方必须遵守的契约)
- 级别:table
- 里程碑:M3
- 前置:`doc-check` 的 venue 规则对夹具 README 运行;基线夹具 = `scope: core`、含全部 12 个必备节、每节对现货与永续都有内容;每行在基线上做一处改动
- 实现:`tools/xtask/tests/doc_check_venue.rs::vpr_130_required_sections`

| 行 | 改动 | 期望 |
|---|---|---|
| VPR-130.1 | 无 | 通过 |
| VPR-130.2 | 删"分档快照流"一节 | 报错,指出缺"分档快照流" |
| VPR-130.3 | 删"L2 序号归一化"一节 | 报错 |
| VPR-130.4 | 删"累计量与费用来源"一节 | 报错 |
| VPR-130.5 | 删"结果未知码表"一节 | 报错 |
| VPR-130.6 | 删"VenueCaps 默认值"一节 | 报错 |
| VPR-130.7 | 删"账户配置核验"一节 | 报错 |
| VPR-130.8 | 删"保证金数据换算"一节 | 报错 |
| VPR-130.9 | 删"外部成交识别"一节 | 报错 |
| VPR-130.10 | 删"用户流事件表"一节 | 报错 |
| VPR-130.11 | 删"测试网/模拟盘"一节 | 报错 |
| VPR-130.12 | 删"心跳与 BBO 探测"一节 | 报错 |
| VPR-130.13 | "账户配置核验"表只有永续列 | 报错,指出缺现货(契约 21) |
| VPR-130.14 | `scope: optional`,只有"VenueCaps 默认值",其余各节写"接入时补齐" | 通过 |
| VPR-130.15 | `scope: optional`,缺"VenueCaps 默认值" | 报错 |
| VPR-130.16 | 删"只读查询"一节 | 报错 |

### VPR-131 doc-check 拒绝宽于统一门槛的 venue 预算

- 规则:[延迟与资源契约](README.md#延迟与资源契约)
- 级别:table
- 里程碑:M3
- 前置:`doc-check` 读取本 README 的门槛表与夹具 venue 的预算表、`benches/budgets.toml`;每行改夹具中的一项
- 实现:`tools/xtask/tests/doc_check_venue.rs::vpr_131_venue_budget_not_wider`

| 行 | 夹具预算 | 期望 |
|---|---|---|
| VPR-131.1 | `classify` 30 ns / 60 ns | 通过 |
| VPR-131.2 | `classify` 30 ns / 61 ns | 报错,指出 p99 宽于 60 ns |
| VPR-131.3 | BBO `decode_md` 0.41 µs / 0.8 µs | 报错,指出 p50 宽于 0.4 µs |
| VPR-131.4 | BBO `decode_md` 0.4 µs / 0.8 µs | 通过 |
| VPR-131.5 | 20 档 `decode_md` 1.2 µs / 2.001 µs | 报错 |
| VPR-131.6 | `encode` 0.3 µs / 0.6 µs | 通过 |
| VPR-131.7 | 成交类 `decode_report` 1.2 µs / 2.6 µs | 报错 |
| VPR-131.8 | 可忽略事件 0.16 µs / 0.3 µs | 报错 |
| VPR-131.9 | `acct_decode` 50 KiB 60 µs / 101 µs | 报错 |
| VPR-131.10 | `decode_snapshot` 1000 档 81 µs / 100 µs | 报错 |
| VPR-131.11 | 只写一个数、未写分位数 | 报错,指出缺分位数 |

### VPR-132 doc-check 核对门槛与 ingest 分配

- 规则:[延迟与资源契约](README.md#延迟与资源契约)
- 级别:table
- 里程碑:M3
- 前置:`doc-check` 读取本 README 门槛表、hft-net 预算表(`net_rx_300b_mem`、`net_rx_2k_mem`)与 latency-budget §3 `ingest` 行;每行改一个输入
- 实现:`tools/xtask/tests/doc_check_venue.rs::vpr_132_ingest_consistency`

| 行 | 输入 | 期望 |
|---|---|---|
| VPR-132.1 | 现行三份文档 | 通过 |
| VPR-132.2 | BBO `decode_md` 门槛 p99 改为 1.3 µs(三项 p99 之和 2.16 µs > 10 µs / 5) | 报错,指出 BBO 路径 p99 |
| VPR-132.3 | 20 档 `decode_md` 门槛 p50 改为 1.4 µs(三项 p50 之和 2.13 µs > 4 µs / 2) | 报错,指出深度路径 p50 |

### VPR-133 check_ro:只读查询族生成只读请求并逐行产出(契约 31)

- 规则:[实现方必须遵守的契约](README.md#实现方必须遵守的契约)、[辅助类型](README.md#辅助类型)
- 级别:table
- 里程碑:M5
- 前置:`FakePerp`、`FakeSpot`;以只读凭证构建的 `Templates`;每行一种 `RoKind` 的请求与响应样本(响应含 1 个 refdata 之外的 symbol)
- 实现:`crates/hft-venue-proto/tests/contract_ro.rs::vpr_133_check_ro_pass`

| 行 | 输入 | 期望 |
|---|---|---|
| VPR-133.1 | 五种 `RoKind` 的 `ro_request`(`FakePerp`) | 都 `Ok`;`method = GET`;`redacted = true` |
| VPR-133.2 | `OpenOrders`:本账户 1 单 + 外来 1 单 | 2 次 `open_order`:前者 `QueryResult{slot, gen, epoch}` 取自 client id、`detail = 1`;后者 `slot = UNRESOLVED` |
| VPR-133.3 | `Fills`:2 条成交,其一为强平 | 2 次 `fill`;强平那条 `cause = Some(Liquidation)`;`trade_id`、`exch_order_id`、`ex_ts_ns` 与报文一致 |
| VPR-133.4 | `FundingFlow`:inst 201 两笔 | 2 次 `funding`,`inst = 201`、`synth = 0`、`settle_ts_ns` 为报文原值 |
| VPR-133.5 | `Balance`、`Positions` | 各自 `balance`/`position`;refdata 之外的 symbol 不产出 |
| VPR-133.6 | `Fills` 第一页响应带下一页游标 `"c2"` | `RoPage{rows: 2, next_cursor ≠ 0}`;以该游标再调 `ro_request` 的请求含 `"c2"` |
| VPR-133.7 | `FakeSpot` 的 `Positions`、`FundingFlow` | `Err(EncodeErr::Unsupported(GwOp::Query))` |
| VPR-133.8 | 整组样本交给 `check_ro` | 通过;同一输入两次 `ro_decode` 的输出逐字节相同 |

### VPR-134 check_ro 检出写操作请求或不确定的解码

- 规则:[实现方必须遵守的契约](README.md#实现方必须遵守的契约)
- 级别:table
- 里程碑:M5
- 前置:按行的变体
- 实现:`crates/hft-venue-proto/tests/contract_ro.rs::vpr_134_ro_violations`

| 行 | 变体 | 期望 |
|---|---|---|
| VPR-134.1 | `Mut::RoPost`(`OpenOrders` 用 `DELETE`) | `Err(ContractFail{contract: 31})` |
| VPR-134.2 | `Mut::RoKeepsForeignSymbol`(refdata 之外的 symbol 也产出) | `Err(ContractFail{contract: 31})` |
| VPR-134.3 | `Mut::RoFundingNoInst`(资金费流水 `inst = 0`) | `Err(ContractFail{contract: 31})` |
| VPR-134.4 | `Mut::SpotPositionsOk`(现货 `Positions` 返回 `Ok`) | `Err(ContractFail{contract: 31})` |

### VPR-135 批量响应中结果未知的条目逐条产出 OutcomeUnknown(契约 8)

- 规则:[实现方必须遵守的契约](README.md#实现方必须遵守的契约)
- 级别:table
- 里程碑:M3
- 前置:`FakePerp`(支持 `encode_batch`);批量下单响应 3 条:槽 40 成功、槽 41 `"50004"`(结果未知)、槽 42 `"51008"`(确定拒绝)
- 实现:`crates/hft-venue-proto/tests/contract_report.rs::vpr_135_batch_outcome_unknown`

| 行 | 输入 | 期望 |
|---|---|---|
| VPR-135.1 | 上述响应 | `Ack{slot: 40}`、`OrderEvent{what: OutcomeUnknown, slot: 41, reject_code: 50004}`、`Reject{slot: 42, reject_code: 51008}`;`ReportOutcome.outcome_unknown == None` |
| VPR-135.2 | 整帧顶层码 `"50004"` 且无逐条数据 | 无事件;`outcome_unknown == Some(ReqRef{slot: 40, …})`(请求 id 的第一单) |
| VPR-135.3 | 整组样本交给 `check_unknown_codes` | 通过 |

### VPR-136 check_unknown_codes 检出批量响应里只报第一条结果未知的实现

- 规则:[实现方必须遵守的契约](README.md#实现方必须遵守的契约)
- 级别:unit
- 里程碑:M3
- 前置:变体 `Fake<FakePerp, Mut::BatchFirstUnknownOnly>`(批量响应中第二条起的结果未知条目不产出事件)
- 步骤:
  1. `check_unknown_codes`(样本含 VPR-135.1)
- 期望:
  - `Err(ContractFail{contract: 8})`,`sample` 指向 VPR-135.1
- 实现:`crates/hft-venue-proto/tests/contract_report.rs::vpr_136_batch_unknown_detected`

### VPR-137 `Unverifiable` 账户缺 UID 不置位(契约 16、23)

- 规则:[实现方必须遵守的契约](README.md#实现方必须遵守的契约)
- 级别:table
- 里程碑:M3
- 前置:`FakeSpot`(同一把凭证);Config 响应的 UID 字段按行;其余项满足要求
- 实现:`crates/hft-venue-proto/tests/contract_acct.rs::vpr_137_unverifiable_uid`

| 行 | `uid_policy` / 响应 UID | 期望 `AccountConfigReport` |
|---|---|---|
| VPR-137.1 | `Unverifiable` / 缺失 | `mismatches == 0`;`uid_keys == 0`;`venue_uid` 全 0 |
| VPR-137.2 | `Unverifiable` / `""` | 同上 |
| VPR-137.3 | `Verified` / 缺失 | `UNREADABLE`;`uid_keys == 0` |
| VPR-137.4 | 整组样本交给 `check_account_config` | 通过 |

### VPR-138 check_account_config 检出 Unverifiable 账户仍因缺 UID 置位的实现

- 规则:[实现方必须遵守的契约](README.md#实现方必须遵守的契约)
- 级别:table
- 里程碑:M3
- 前置:按行的变体
- 实现:`crates/hft-venue-proto/tests/contract_acct.rs::vpr_138_unverifiable_violations`

| 行 | 变体 | 期望 |
|---|---|---|
| VPR-138.1 | `Mut::UnverifiableUnreadable`(VPR-137.1 置 `UNREADABLE`) | `Err(ContractFail{contract: 16})` |
| VPR-138.2 | `Mut::VerifiedMissingOk`(VPR-137.3 不置位) | `Err(ContractFail{contract: 16})` |

### VPR-139 现货钱包不能借币的产品线以账户类型样本代替借币样本

- 规则:[实现方必须遵守的契约](README.md#实现方必须遵守的契约)、[实现说明](README.md#实现说明)
- 级别:table
- 里程碑:M3
- 前置:变体 `Fake<FakeSpot, Mut::NoSpotBorrowSwitch>`(`required_spot_borrow = None`,Config 序列不读借币开关,读账户类型)
- 实现:`crates/hft-venue-proto/tests/contract_acct.rs::vpr_139_spot_no_borrow_switch`

| 行 | 样本 | 期望 |
|---|---|---|
| VPR-139.1 | 账户类型 = 现货现金账户 | `mismatches == 0` |
| VPR-139.2 | 账户类型 = 杠杆账户 | `ACCT_TYPE` |
| VPR-139.3 | 不含借币开关样本、含 VPR-139.2,交给 `check_account_config` | 通过 |
| VPR-139.4 | 两类样本都不含,交给 `check_account_config` | `Err(ContractFail{contract: 16})`,`detail` 指出缺借币或账户类型样本 |

### VPR-140 基准:TradeIdRing::seen_or_insert

- 规则:[延迟与资源契约](README.md#延迟与资源契约)
- 级别:bench
- 里程碑:M3
- 基准名:`bench_trade_id_ring`
- 实现:`crates/hft-venue-proto/benches/dedup.rs::vpr_140_trade_id_ring`

### VPR-141 基准:dedup_key

- 规则:[延迟与资源契约](README.md#延迟与资源契约)
- 级别:bench
- 里程碑:M3
- 基准名:`bench_dedup_key`
- 实现:`crates/hft-venue-proto/benches/dedup.rs::vpr_141_dedup_key`

### VPR-150 normal 依赖闭包只含四个 crate

- 规则:[依赖](README.md#依赖)
- 级别:table
- 里程碑:M3
- 前置:`cargo metadata` 取 `hft-venue-proto` 不开 feature 的 normal 依赖的直接内部依赖与外部依赖;另一份副本清单加入 `hft-net`
- 实现:`crates/hft-venue-proto/tests/deps.rs::vpr_150_deps_closure`

| 行 | 检查 | 期望 |
|---|---|---|
| VPR-150.1 | 直接依赖集合 | 恰为 {`hft-types`, `hft-refdata`, `hft-event`, `xxhash-rust`} |
| VPR-150.2 | 闭包中的 `hft-rt`、`hft-net`、`hft-journal`、`hft-venue`、`venue-*` | 不存在 |
| VPR-150.3 | 闭包中的 `serde_json`、`tokio` | 不存在 |
| VPR-150.4 | 副本清单交给同一检查函数 | 报告 1 处违例:`hft-net` |

### VPR-151 venue crate 只经 testkit feature 使用本 crate 的测试设施

- 规则:[实现说明](README.md#实现说明)
- 级别:table
- 里程碑:M3
- 前置:`cargo metadata` 读 `venue-binance`、`venue-okx`、`venue-bitget` 的 dev 依赖
- 实现:`crates/hft-venue-proto/tests/deps.rs::vpr_151_venue_dev_deps_use_testkit`

| 行 | 检查 | 期望 |
|---|---|---|
| VPR-151.1 | 三者对 `hft-venue-proto` 的 dev 依赖 | 都带 `features = ["testkit"]` |
| VPR-151.2 | 三者的 dev 依赖中的 `hft-testkit` | 不存在 |
| VPR-151.3 | 三者的 `tests/contract.rs` 引用的 `check_*` 集合 | 等于 testkit 导出的全部 `check_*`(`check_chain` 只在启用增量深度的 venue 中要求) |

## 验收

- **完成标准**:里程碑已到的全部用例有同编号的实现且通过(`cargo xtask test-trace`,[docs/tests §6](../../docs/tests/README.md#6-与代码的对应));VPR-001、VPR-066 在 M1.5 纵切片之前通过,VPR-133、VPR-134 随 M5(只读查询族的实现随 reconciler),其余随 M3。
- **契约检查覆盖全部契约**:契约 1–31 每条至少有一个正例与一个反例(本文件 VPR-060–VPR-129、VPR-133–VPR-139;契约 21 的文档要求由 VPR-130 承担);每个 core venue 的 `tests/contract.rs` 对两个产品线调用全部 `check_*` 且通过(VPR-151 核对调用集合;通过与否是该 venue `TESTS.md` 的用例)。
- **属性测试的大例数运行**(每夜):VPR-021 10⁶ 例;VPR-031 10 例(每例 10⁶ 个输入);VPR-035 100 例。
- **门槛**:VPR-131、VPR-132 在 `doc-check` 中通过(每个 venue 的 p50、p99 不宽于统一门槛;门槛与 [latency-budget §3](../../docs/latency-budget.md#3-cex-路径预算) 的 `ingest` 分配一致);VPR-140、VPR-141 在参考机上满足本 README 的门槛(判定规则归 [benchmarks](../../benchmarks/README.md))。
- **依赖**:VPR-150 通过(normal 依赖只含 `hft-types`、`hft-refdata`、`hft-event` 与 `xxhash-rust`)。
