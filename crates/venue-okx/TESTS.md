---
component: venue-okx
prefix: OKX
spec: README.md
---

# venue-okx 测试规格

格式与规则见 [docs/tests](../../docs/tests/README.md)。本文件的记号(各用例在 `前置` 中引用,数值即定义,不依赖默认值):

- `RD-OKX` = 测试 refdata `fixtures/refdata/okx-test-1.blob`(由同目录 `okx-test-1.toml` 经 hft-cfg 生成)。资产:`USDT` = AssetId 1(`qty_exp = −8`)、`BTC` = 2(`px_exp = −2`,`qty_exp = −8`)、`ETH` = 3(`px_exp = −2`,`qty_exp = −8`)、`PEPE` = 5(`px_exp = −10`,`qty_exp = 0`)。`OKX_SWAP`(5):`BTC-USDT-SWAP` = InstId 1501(`instIdCode = 10459`,ctVal 0.01 → `qty_unit_shift = −2`,`qty_wire_exp = −6`)、`ETH-USDT-SWAP` = 1502(`instIdCode = 10460`,ctVal 0.1 → `qty_unit_shift = −1`,`qty_wire_exp = −7`)、`PEPE-USDT-SWAP` = 1503(`instIdCode = 10461`,ctVal 10⁷ → `qty_unit_shift = 7`,`qty_wire_exp = −7`);三者都 `Trading`。`OKX_SPOT`(6):`BTC-USDT` = 1601(`instIdCode = 10001`,`qty_wire_exp = −8`)、`ETH-USDT` = 1602(`instIdCode = 10002`)。账户 `AccountId 2`,`is_prod = false`,`venue_uid = "acct-0001"`,`uid_policy = "verified"`;生效 `VenueCaps` = 各自的 `default_caps()`(用例另行覆盖的除外)。定点值写成"十进制(raw)"一次,之后只写十进制。
- `RD-OKX-25` = `RD-OKX` 另加 22 个可交易 `*-USDT-SWAP`(InstId 1504–1525),共 25 个可交易永续。
- `RD-OKX-DEMO` = 模拟盘 refdata `fixtures/refdata/okx-demo-1.blob`:同 `RD-OKX`,但 `BTC-USDT-SWAP` 的 `instIdCode = 99999`,端点环境 `EndpointEnv::Demo`。
- `RX` = `RxMeta{rx_tsc: 1000, hw_rx_ns: 1790000000126000000, sw_rx_ns: 1790000000126500000, prev_override: 0, conn: 3, account: 0, ts_flags: 0, rx_empty_gap_ns: 2500}`。
- `CID(e, s, g)` = `ClientOrderId{epoch: e, venue: <本产品线>, account: 2, slot: s, gen: g}` 的规范 24 位文本:`OKX_SWAP` 的 `CID(3, 42, 7) = 1000305020002a000000077d`、`CID(3, 43, 1) = 1000305020002b000000017a`、`CID(2, 42, 7) = 1000205020002a000000076d`;`OKX_SPOT` 的 `CID(3, 42, 7) = 1000306020002a000000074d`、`CID(3, 43, 1) = 1000306020002b000000014a`。`CID✗` = `CID(3, 42, 7)` 末字节异或 1(`1000305020002a000000077c`,校验位错)。
- `WC` = `WallClock{unix_ms: 1790000000000, unix_us: 1790000000000000}`(ISO 形式 `2026-09-21T14:13:20.000Z`);`gw.stale_guard_ms = 1500`。
- `KEY` = 测试凭证 `apiKey = "k-test"`、`passphrase = "p-test"`、`secret = "22582BD0CFF14C41EDBF1AB98506286D"`(官方文档示例 secret,不是真实凭证);除注明外 `Templates` 以 `build_templates(cfg, rd, 2, &KEY)` 构建。
- `VecSink` / `GoldenDriver` 来自 hft-venue-proto feature `testkit`;"事件"指 sink 收到的调用。

## 覆盖

| README 小节 | 用例 |
|---|---|
| 概要 | 不适用:非规范 |
| 目录 | 不适用:非规范 |
| 使用本组件 | OKX-001, OKX-010, OKX-011, OKX-083–OKX-086, OKX-095 |
| 端点与路由 | OKX-002–OKX-004, OKX-063 |
| 行情频道 → 事件 | OKX-020–OKX-027, OKX-029–OKX-031, OKX-033, OKX-035–OKX-039, OKX-128, OKX-200, OKX-300, OKX-302–OKX-305 |
| 下单:WS `/private` | OKX-060–OKX-075, OKX-078, OKX-080–OKX-082, OKX-087, OKX-097–OKX-099, OKX-107, OKX-142, OKX-210, OKX-211, OKX-307, OKX-317, OKX-322, OKX-335 |
| 限速默认值 | OKX-091, OKX-170 |
| 分档快照流 | OKX-002, OKX-020, OKX-029, OKX-030, OKX-216, OKX-301 |
| L2 序号归一化(`L2Batch.prev_id/last_id`) | 不适用:里程碑外 |
| 累计量与费用来源 | OKX-097, OKX-101–OKX-105, OKX-107, OKX-129, OKX-316 |
| 结果未知码表 | OKX-091–OKX-094, OKX-311–OKX-315 |
| VenueCaps 默认值 | OKX-099, OKX-171–OKX-176, OKX-318, OKX-329 |
| 账户配置核验 | OKX-140, OKX-142–OKX-147, OKX-213, OKX-331–OKX-333 |
| 保证金数据换算 | OKX-113–OKX-121, OKX-125, OKX-126, OKX-141, OKX-148, OKX-149, OKX-323–OKX-325, OKX-332, OKX-334 |
| 外部成交识别 | OKX-108–OKX-111, OKX-320, OKX-321 |
| 用户流事件表 | OKX-011, OKX-040, OKX-062, OKX-100, OKX-113, OKX-114, OKX-121–OKX-127, OKX-203, OKX-204, OKX-215, OKX-309, OKX-326–OKX-330 |
| 测试网/模拟盘 | OKX-210, OKX-211, OKX-213–OKX-216 |
| 心跳与 BBO 探测 | OKX-050–OKX-054, OKX-190, OKX-300, OKX-306 |
| 只读查询 | OKX-178, OKX-336–OKX-340 |
| `mock` 特性 | 不适用:里程碑外 |
| 实现说明 | 不适用:见子节 |
| 1. 分派 | OKX-010–OKX-012, OKX-040, OKX-041, OKX-127, OKX-309, OKX-400, OKX-401 |
| 2. 订阅约束 | OKX-040, OKX-200–OKX-204, OKX-308 |
| 3. 订单帧模板 | OKX-001, OKX-065–OKX-069, OKX-072, OKX-076, OKX-077, OKX-083, OKX-084, OKX-335, OKX-402 |
| 4. 回报 | OKX-090, OKX-092, OKX-095, OKX-096, OKX-100, OKX-101, OKX-106, OKX-130, OKX-311, OKX-319 |
| 5. 心跳与会话 | OKX-060, OKX-064, OKX-173, OKX-190, OKX-310 |
| 6. 源文件规划 | OKX-071, OKX-177 |
| 延迟与资源契约 | OKX-042, OKX-500–OKX-506 |
| 测试与验收 | 不适用:非规范 |
| 依赖 | OKX-212 |
| 延伸阅读 | 不适用:非规范 |
| 已知限制与后续工作 | 不适用:非规范 |
| 开发备忘 | 不适用:非规范 |

## 用例

### OKX-001 两个产品线的身份常量与帧容量

- 规则:[使用本组件](README.md#使用本组件)、[3. 订单帧模板](README.md#3-订单帧模板)
- 级别:table
- 里程碑:M3
- 前置:无参数;逐行断言关联常量
- 实现:`crates/venue-okx/tests/consts.rs::okx_001_identity_consts`

| 行 | 产品线 | 期望 |
|---|---|---|
| OKX-001.1 | `OkxSwap` | `NAME == "okx-swap"`,`VENUE == OKX_SWAP`(5),`MARKET == MarketKind::LinearPerp`,`ORDER_FRAME_CAP == 8192` |
| OKX-001.2 | `OkxSpot` | `NAME == "okx-spot"`,`VENUE == OKX_SPOT`(6),`MARKET == MarketKind::Spot`,`ORDER_FRAME_CAP == 8192` |
| OKX-001.3 | 两者 | `NAME` 等于金样目录名:`fixtures/okx-swap/`、`fixtures/okx-spot/` 存在 |

### OKX-002 routes() 给出 BBO、分档快照、成交与标记价路由

- 规则:[端点与路由](README.md#端点与路由)、[分档快照流](README.md#分档快照流)
- 级别:table
- 里程碑:M3
- 前置:无参数;断言 `routes()` 的全部条目(顺序即下表)
- 实现:`crates/venue-okx/tests/consts.rs::okx_002_routes`

| 行 | 产品线 / 下标 | 期望 `RouteSpec` |
|---|---|---|
| OKX-002.1 | `OkxSwap` 0 | `name = "public-bbo"`,`kind = Bbo`,`race = true`,`max_streams = 100`,`max_in_msgs_per_s = 4`,`topn_levels = []` |
| OKX-002.2 | `OkxSwap` 1 | `public-topn`,`DepthTopN`,`race = false`,100,4,`topn_levels = [5]` |
| OKX-002.3 | `OkxSwap` 2 | `public-trades`,`Md`,`race = false`,100,4,`[]` |
| OKX-002.4 | `OkxSwap` 3 | `public-mark`,`Md`,`race = false`,100,4,`[]`;`routes().len() == 4` |
| OKX-002.5 | `OkxSpot` 0–2 | 同 OKX-002.1–OKX-002.3;`routes().len() == 3`(无 `public-mark`) |
| OKX-002.6 | 两者 | 恰有一条 `DepthTopN` 路由;只有 `kind = Bbo` 的路由 `race = true` |

### OKX-003 endpoint 按角色给出路径,UserStream 不受支持

- 规则:[端点与路由](README.md#端点与路由)
- 级别:table
- 里程碑:M3
- 前置:`RD-OKX`;`KEY`;`now_ms = 1790000000000`;两个产品线逐行各跑一次
- 实现:`crates/venue-okx/tests/session.rs::okx_003_endpoint_paths`

| 行 | 调用 | 期望 |
|---|---|---|
| OKX-003.1 | `endpoint(Md, public-bbo, …)` | `Ok`,`out.path = "/ws/v5/public"`,无额外 Upgrade 头 |
| OKX-003.2 | `endpoint(Md, public-topn, …)` / `public-trades` | `Ok`,`/ws/v5/public` |
| OKX-003.3 | `endpoint(Order, 0, …)` | `Ok`,`/ws/v5/private` |
| OKX-003.4 | `endpoint(UserStream, 0, …)` | `Err(ProtoErr::Unsupported)` |
| OKX-003.5 | `bootstrap(Md, cfg, None)`、`bootstrap(Order, cfg, Some(&KEY))`、`bootstrap(UserStream, cfg, Some(&KEY))` | 都是 `None` |

### OKX-004 全部 REST 请求的端点下标都是 0

- 规则:[端点与路由](README.md#端点与路由)
- 级别:unit
- 里程碑:M3
- 前置:`RD-OKX`;`KEY`;`WC`;`OkxSwap`
- 步骤:
  1. 生成 BBO 补帧请求(inst 1501)、Config 序列步 0–2 的请求、Margin 兜底步 0–1 的请求、`dead_man_refresh(timeout_ms = 30000)` 的请求、`GwCmd{op: Query, target_slot: 42}` 的 REST 查单请求
- 期望:
  - 每个 `RestReqBuf.endpoint == 0`
- 实现:`crates/venue-okx/tests/rest.rs::okx_004_rest_endpoint_index_zero`

### OKX-010 行情消息按前 64 字节分派到类

- 规则:[1. 分派](README.md#1-分派)、[使用本组件](README.md#使用本组件)
- 级别:table
- 里程碑:M3
- 前置:`route = 0`;`MsgOp::Text`,除注明外;头为报文前缀(其后内容任意)
- 实现:`crates/venue-okx/tests/classify.rs::okx_010_classify_md`

| 行 | 头 | 期望 `MdClass` |
|---|---|---|
| OKX-010.1 | `{"arg":{"channel":"bbo-tbt","instId":"BTC-USDT-SWAP"},"data":[` | `BboTbt` |
| OKX-010.2 | `{"arg":{"channel":"books5","instId":"BTC-USDT"},"data":[` | `Books5` |
| OKX-010.3 | `{"arg":{"channel":"trades","instId":"BTC-USDT-SWAP"},"data":[` | `Trades` |
| OKX-010.4 | `{"arg":{"channel":"mark-price","instId":"BTC-USDT-SWAP"},` | `MarkPrice` |
| OKX-010.5 | `{"arg":{"channel":"funding-rate","instId":"BTC-USDT-SWAP"},` | `FundingRate` |
| OKX-010.6 | `{"arg":{"channel":"index-tickers","instId":"BTC-USDT"},` | `IndexTickers` |
| OKX-010.7 | `{"event":"subscribe","arg":{"channel":"bbo-tbt"` | `Control` |
| OKX-010.8 | `{"event":"error","code":"60033","msg":"Parameter id error"` | `Control` |
| OKX-010.9 | `{"event":"notice","code":"64008","msg":"The connection will soon be closed` | `Control` |
| OKX-010.10 | `pong`(4 字节) | `Pong` |
| OKX-010.11 | `{"arg":{"channel":"books","instId":"BTC-USDT"},"action":"snapshot"` | `Unknown`(增量深度未启用) |
| OKX-010.12 | `{"arg":{"channel":"tickers","instId":"BTC-USDT"}` | `Unknown` |
| OKX-010.13 | 任意字节,`MsgOp::Binary` | `Unknown` |
| OKX-010.14 | 空串 | `Unknown` |
| OKX-010.15 | `{"arg":{"chan`(13 字节,截断) | `Unknown` |
| OKX-010.16 | `ping` | `Unknown` |
| OKX-010.17 | 64 个空格后接 `{"arg":{"channel":"bbo-tbt"` | `Unknown`(频道名不在前 64 B) |

### OKX-011 私有连接与 REST 响应按前 64 字节分派到回报类

- 规则:[1. 分派](README.md#1-分派)、[使用本组件](README.md#使用本组件)、[用户流事件表](README.md#用户流事件表)
- 级别:table
- 里程碑:M3
- 前置:`MsgOp::Text`;编译进来的默认可忽略名单 = `default_caps().ignorable_user_events`
- 实现:`crates/venue-okx/tests/classify.rs::okx_011_classify_report`

| 行 | 头 | 期望 `ReportClass` |
|---|---|---|
| OKX-011.1 | `{"id":"p0000002a00000007","op":"order",` | `OrderResp` |
| OKX-011.2 | `{"id":"c0000002a00000007","op":"cancel-order",` | `OrderResp` |
| OKX-011.3 | `{"id":"m0000002a00000007","op":"amend-order",` | `OrderResp` |
| OKX-011.4 | `{"id":"b0000002a00000007","op":"batch-orders",` | `BatchResp` |
| OKX-011.5 | `{"id":"x0000002a00000007","op":"batch-cancel-orders",` | `BatchResp` |
| OKX-011.6 | `{"id":"n0000002a00000007","op":"batch-amend-orders",` | `BatchResp` |
| OKX-011.7 | `{"id":"z0000002a00000007",` | `Unknown` |
| OKX-011.8 | `{"code":"0","msg":"","data":[{` | `RestResp` |
| OKX-011.9 | `{"arg":{"channel":"orders","instType":"SWAP"` | `Orders` |
| OKX-011.10 | `{"arg":{"channel":"positions","instType":"SWAP"` | `Positions` |
| OKX-011.11 | `{"arg":{"channel":"account"` | `Account` |
| OKX-011.12 | `{"arg":{"channel":"balance_and_position"` | `BalanceAndPosition` |
| OKX-011.13 | `{"arg":{"channel":"liquidation-warning","instType":"SWAP"` | `LiquidationWarning` |
| OKX-011.14 | `{"event":"login","code":"0"` | `Control` |
| OKX-011.15 | `{"event":"notice","code":"64008"` | `Control` |
| OKX-011.16 | `{"event":"channel-conn-count","channel":"orders"` | `Ignorable` |
| OKX-011.17 | `{"event":"channel-conn-count-error","channel":"orders"` | `Ignorable` |
| OKX-011.18 | `{"event":"brand-new-event"` | `Unknown` |
| OKX-011.19 | `{"arg":{"channel":"fills"` | `Unknown` |
| OKX-011.20 | `pong` | `Pong` |
| OKX-011.21 | `MsgOp::Binary` 任意字节 | `Unknown` |

### OKX-012 分派结果只取决于前 64 字节

- 规则:[1. 分派](README.md#1-分派)
- 级别:prop
- 里程碑:M3
- 性质:对任意两条前 64 字节相同的报文,`classify` 结果相同,`classify_report` 结果相同;对长度 < 64 的报文不越界读
- 生成:从 OKX-010、OKX-011 的头与全部金样输入中取前缀 p(长度 0–64),后接 0–4096 个随机字节,成对生成两条前 64 字节相同、之后不同的报文
- 参考:断言 `classify(r, op, a) == classify(r, op, b)` 与 `classify_report(op, a) == classify_report(op, b)`;ASan 下零越界
- 例数:PR 10 000
- 实现:`crates/venue-okx/tests/classify.rs::okx_012_classify_prefix_only`

### OKX-020 每个行情类的序号规则与重同步方式

- 规则:[行情频道 → 事件](README.md#行情频道--事件)、[分档快照流](README.md#分档快照流)
- 级别:table
- 里程碑:M3
- 前置:无参数
- 实现:`crates/venue-okx/tests/md.rs::okx_020_seq_rule_and_resync`
- 备注:依赖待核实:开发备忘第 6 条(`bbo-tbt` 是否带 `seqId`)、第 18 条(`books5` 是否带单调 `seqId`);核实结论改变规则时同一改动修改本表

| 行 | `MdClass` | 期望 `seq_rule` / `resync_method` |
|---|---|---|
| OKX-020.1 | `BboTbt` | `Monotonic` / `None` |
| OKX-020.2 | `Books5` | `Snapshot` / `None` |
| OKX-020.3 | `Trades` | `Monotonic` / `None` |
| OKX-020.4 | `MarkPrice` | `ByTime` / `None` |
| OKX-020.5 | `FundingRate` | `ByTime` / `None` |
| OKX-020.6 | `IndexTickers` | `ByTime` / `None` |
| OKX-020.7 | `Control`、`Pong`、`Unknown` | `None` / `None` |

### OKX-021 peek 取出 inst 与 seqId,不填 prev_id

- 规则:[行情频道 → 事件](README.md#行情频道--事件)、[hft-venue-proto 契约 2](../hft-venue-proto/README.md#实现方必须遵守的契约)
- 级别:unit
- 里程碑:M3
- 前置:`RD-OKX`;`OkxSwap`;报文 `B1` = `{"arg":{"channel":"bbo-tbt","instId":"BTC-USDT-SWAP"},"data":[{"asks":[["65000.2","12","0","3"]],"bids":[["65000.1","150","0","5"]],"ts":"1790000000123","seqId":4000123}]}`
- 步骤:
  1. `peek(rd, ctx, BboTbt, B1)`
- 期望:
  - `Ok(Peek{inst: 1501, ids: SeqIds{prev_id: 0, last_id: 4000123}})`
- 实现:`crates/venue-okx/tests/md.rs::okx_021_peek_bbo`

### OKX-022 peek 对 refdata 中没有的 instId 返回 inst = 0

- 规则:[行情频道 → 事件](README.md#行情频道--事件)
- 级别:unit
- 里程碑:M3
- 前置:`RD-OKX`;`OkxSwap`;报文同 OKX-021 的 `B1`,但 `instId = "DOGE-USDT-SWAP"`;另一条 `instId = "BTC-USDT"`(现货符号)
- 步骤:
  1. 对两条报文各 `peek`
- 期望:
  - 两次都是 `Ok(Peek{inst: 0, …})`
- 实现:`crates/venue-okx/tests/md.rs::okx_022_peek_unknown_symbol`

### OKX-023 ByTime 类的 peek 以 ts 换算的 ns 作 last_id

- 规则:[行情频道 → 事件](README.md#行情频道--事件)
- 级别:unit
- 里程碑:M3
- 前置:`RD-OKX`;`OkxSwap`;报文 `{"arg":{"channel":"mark-price","instId":"BTC-USDT-SWAP"},"data":[{"instType":"SWAP","instId":"BTC-USDT-SWAP","markPx":"65001.3","ts":"1790000000500"}]}`
- 步骤:
  1. `peek(rd, ctx, MarkPrice, …)`
- 期望:
  - `Ok(Peek{inst: 1501, ids: SeqIds{prev_id: 0, last_id: 1790000000500000000}})`
- 实现:`crates/venue-okx/tests/md.rs::okx_023_peek_bytime`

### OKX-024 peek 不解析价格与数量

- 规则:[hft-venue-proto 契约 2](../hft-venue-proto/README.md#实现方必须遵守的契约)、[行情频道 → 事件](README.md#行情频道--事件)
- 级别:unit
- 里程碑:M3
- 前置:`RD-OKX`;`OkxSwap`;报文同 OKX-021 的 `B1`,但 `bids[0][0] = "abc"`
- 步骤:
  1. `peek`
  2. `decode_md(rd, ctx, BboTbt, payload, &RX, &mut sink)`
- 期望:
  - 步骤 1 `Ok(Peek{inst: 1501, ids.last_id: 4000123})`
  - 步骤 2 `Err(DecodeErr::Number)`,sink 无事件
- 实现:`crates/venue-okx/tests/md.rs::okx_024_peek_ignores_prices`

### OKX-025 永续 bbo-tbt 解码为 Bbo,数量经 qty_wire_exp 换算成资产单位

- 规则:[行情频道 → 事件](README.md#行情频道--事件)、[下单:WS `/private`](README.md#下单ws-private)
- 级别:unit
- 里程碑:M3
- 前置:`RD-OKX`;`OkxSwap`;报文 OKX-021 的 `B1`;`RX`
- 步骤:
  1. `decode_md(rd, ctx, BboTbt, B1, &RX, &mut sink)`
- 期望:
  - 返回 `Ok(MdOutcome{events: 1, inst: 1501})`
  - sink 恰好一条 `Bbo{h: {venue: 5, conn: 3, inst: 1501, ex_ts_ns: 1790000000123000000, ex_send_ns: 0, hw_rx_ns: 1790000000126000000, rx_tsc: 1000, sw_rx_ns: 1790000000126500000, rx_empty_gap_ns: 2500}, bid_px: 65000.1(6500010), bid_qty: 1.5(150000000), ask_px: 65000.2(6500020), ask_qty: 0.12(12000000), update_id: 4000123}`(150 张 × 0.01 = 1.5 BTC;12 张 = 0.12 BTC)
- 实现:`crates/venue-okx/tests/md.rs::okx_025_bbo_swap`

### OKX-026 现货 bbo-tbt 的数量即基础币数量

- 规则:[行情频道 → 事件](README.md#行情频道--事件)、[下单:WS `/private`](README.md#下单ws-private)
- 级别:unit
- 里程碑:M3
- 前置:`RD-OKX`;`OkxSpot`;报文 `{"arg":{"channel":"bbo-tbt","instId":"BTC-USDT"},"data":[{"asks":[["65000.2","0.3","0","2"]],"bids":[["65000.1","0.002","0","1"]],"ts":"1790000000123","seqId":88}]}`
- 步骤:
  1. `decode_md`
- 期望:
  - 一条 `Bbo{h.venue: 6, h.inst: 1601, bid_qty: 0.002(200000), ask_qty: 0.3(30000000), update_id: 88}`
- 实现:`crates/venue-okx/tests/md.rs::okx_026_bbo_spot`

### OKX-027 bbo-tbt 解码的失败与边界输入

- 规则:[行情频道 → 事件](README.md#行情频道--事件)、[hft-venue-proto 契约 4、5](../hft-venue-proto/README.md#实现方必须遵守的契约)
- 级别:table
- 里程碑:M3
- 前置:`RD-OKX`;`OkxSwap`;基准报文 OKX-021 的 `B1`,每行只改所列字段;每行断言返回值与 sink 事件数,无 panic
- 实现:`crates/venue-okx/tests/md.rs::okx_027_bbo_decode_failures`

| 行 | 改动 | 期望 |
|---|---|---|
| OKX-027.1 | 删除 `bids` 键 | `Err(DecodeErr::Shape)`,0 事件 |
| OKX-027.2 | 报文在 `"seqId":` 处截断 | `Err(DecodeErr::Truncated)`,0 事件 |
| OKX-027.3 | `bids[0][0] = ""` | `Err(DecodeErr::Number)` |
| OKX-027.4 | `bids[0][0] = "65000.123"`(比 `px_exp = −2` 多一位非零) | `Err(DecodeErr::Number)` |
| OKX-027.5 | `bids[0][0] = "65000.100"`(多出的位为 0) | `Ok`,`bid_px = 6500010` |
| OKX-027.6 | `bids[0][0] = "6.50001E4"` | `Ok`,`bid_px = 6500010` |
| OKX-027.7 | `bids[0][1] = "100000000000000"`(10¹⁴ 张 → 10¹² BTC,raw 超出 i64) | `Err(DecodeErr::Overflow)` |
| OKX-027.8 | `"bids":[]` | `Ok`,一条 `Bbo`,`bid_px = 0`、`bid_qty = 0`,ask 侧不变 |
| OKX-027.9 | `ts = "1790000000123456"`(µs 量级) | `Err(DecodeErr::Overflow)`(按 ms × 10⁶ 换算溢出,不按数值大小改判单位) |
| OKX-027.10 | `"ts":1790000000123`(数字而非字符串) | `Err(DecodeErr::Shape)` |
| OKX-027.11 | `seqId` 键缺失 | `Err(DecodeErr::Shape)` |

### OKX-029 books5 解码为一条 L2Snapshot{origin = TopN},不产出 L2Batch

- 规则:[分档快照流](README.md#分档快照流)、[行情频道 → 事件](README.md#行情频道--事件)、[hft-venue-proto 契约 26](../hft-venue-proto/README.md#实现方必须遵守的契约)
- 级别:unit
- 里程碑:M3
- 前置:`RD-OKX`;`OkxSwap`;`books5` 报文:`instId = "BTC-USDT-SWAP"`,`asks` = `[65000.2, 10]、[65000.3, 20]、[65000.4, 30]、[65000.5, 40]、[65000.6, 50]`(张),`bids` = `[65000.1, 15]、[65000.0, 25]、[64999.9, 35]、[64999.8, 45]、[64999.7, 55]`,`ts = "1790000000200"`,`seqId = 5001`
- 步骤:
  1. `decode_md(rd, ctx, Books5, …, &RX, &mut sink)`
- 期望:
  - 返回 `Ok(MdOutcome{events: 1, inst: 1501})`
  - sink 恰好一次 `l2_snapshot`:`hdr.origin = 3`(TopN),`hdr.snap_id = 5001`,`hdr.n_bids = 5`,`hdr.n_asks = 5`,`hdr.h.ex_ts_ns = 1790000000200000000`;`bids[0] = {px: 6500010, qty: 15000000}`(0.15 BTC)… `bids[4] = {6499970, 55000000}`(价降序);`asks[0] = {6500020, 10000000}` … `asks[4] = {6500060, 50000000}`(价升序)
  - `l2_begin`/`l2_level`/`l2_end` 调用次数 0
- 实现:`crates/venue-okx/tests/md.rs::okx_029_books5_topn`

### OKX-030 books5 档数不足 5 时只输出存在的档

- 规则:[分档快照流](README.md#分档快照流)
- 级别:unit
- 里程碑:M3
- 前置:同 OKX-029,但 `bids` 只有 3 档、`asks` 5 档
- 步骤:
  1. `decode_md`
- 期望:
  - 一次 `l2_snapshot`,`n_bids = 3`,`n_asks = 5`,`bids.len() == 3`
- 实现:`crates/venue-okx/tests/md.rs::okx_030_books5_short`

### OKX-031 books5 中数量为 0 的档不输出

- 规则:[行情频道 → 事件](README.md#行情频道--事件)
- 级别:unit
- 里程碑:M3
- 前置:同 OKX-029,但 `bids[2] = ["64999.9","0","0","0"]`
- 步骤:
  1. `decode_md`
- 期望:
  - `n_bids = 4`;输出的买档价依次为 6500010、6500000、6499980、6499970(64999.9 不出现)
- 实现:`crates/venue-okx/tests/md.rs::okx_031_books5_zero_qty_skipped`

### OKX-033 trades 解码为 Trade,taker 方向取 side

- 规则:[行情频道 → 事件](README.md#行情频道--事件)
- 级别:table
- 里程碑:M3
- 前置:`RD-OKX`;`RX`;报文 `{"arg":{"channel":"trades","instId":<sym>},"data":[{"instId":<sym>,"tradeId":<id>,"px":<px>,"sz":<sz>,"side":<side>,"ts":"1790000000300","count":"1"}]}`
- 实现:`crates/venue-okx/tests/md.rs::okx_033_trades`

| 行 | 产品线、sym、id、px、sz、side | 期望 `Trade` |
|---|---|---|
| OKX-033.1 | `OkxSwap`,`BTC-USDT-SWAP`,`"130639474"`,`"65000.1"`,`"3"`,`"buy"` | `inst 1501`,`px 6500010`,`qty 0.03(3000000)`,`trade_id 130639474`,`taker_side Buy`,`ex_ts_ns 1790000000300000000` |
| OKX-033.2 | 同上,`side = "sell"` | `taker_side Sell` |
| OKX-033.3 | `OkxSpot`,`BTC-USDT`,`"77"`,`"65000.1"`,`"0.002"`,`"buy"` | `inst 1601`,`qty 0.002(200000)`,`trade_id 77` |
| OKX-033.4 | `OkxSwap`,`PEPE-USDT-SWAP`,`"9"`,`"0.0000123456"`,`"2"`,`"buy"` | `inst 1503`,`px 123456`(`px_exp = −10`),`qty 20000000`(2 张 × 10⁷ PEPE) |
| OKX-033.5 | `tradeId = ""` | `Err(DecodeErr::Shape)`,0 事件 |
| OKX-033.6 | `side = "both"` | `Err(DecodeErr::Shape)` |

### OKX-035 mark-price 解码为 MarkPrice

- 规则:[行情频道 → 事件](README.md#行情频道--事件)
- 级别:unit
- 里程碑:M3
- 前置:`RD-OKX`;`OkxSwap`;OKX-023 的报文;`RX`
- 步骤:
  1. `decode_md(rd, ctx, MarkPrice, …)`
- 期望:
  - 恰好一条 `MarkPrice{h.inst: 1501, h.ex_ts_ns: 1790000000500000000, h.ex_send_ns: 0, px: 65001.3(6500130)}`;`MdOutcome.events = 1`
- 实现:`crates/venue-okx/tests/md.rs::okx_035_mark_price`

### OKX-036 funding-rate 解码为 Funding,结算周期取两个时刻之差

- 规则:[行情频道 → 事件](README.md#行情频道--事件)
- 级别:table
- 里程碑:M3
- 前置:`RD-OKX`;`OkxSwap`;报文 `{"arg":{"channel":"funding-rate","instId":"BTC-USDT-SWAP"},"data":[{"instId":"BTC-USDT-SWAP","fundingRate":<r>,"fundingTime":<ft>,"nextFundingTime":<nft>,"ts":"1790000000000"}]}`
- 实现:`crates/venue-okx/tests/md.rs::okx_036_funding_rate`
- 备注:依赖待核实:开发备忘第 8 条(`fundingTime`/`nextFundingTime` 语义)

| 行 | r、ft、nft | 期望 `Funding` |
|---|---|---|
| OKX-036.1 | `"0.00010000"`,`"1790006400000"`,`"1790035200000"` | `rate 0.0001(100000)`,`next_funding_ns 1790006400000000000`,`interval_s 28800` |
| OKX-036.2 | `"0.00010000"`,`"1790006400000"`,`"1790020800000"` | `interval_s 14400`(周期 8 h → 4 h 变化) |
| OKX-036.3 | `"-0.00005"`,`"1790006400000"`,`""` | `rate −50000`,`interval_s 0` |
| OKX-036.4 | `"abc"`,…,… | `Err(DecodeErr::Number)`,0 事件 |
| OKX-036.5 | 缺 `fundingTime` | `Err(DecodeErr::Shape)` |

### OKX-037 index-tickers 的指数名映射到对应永续 inst

- 规则:[行情频道 → 事件](README.md#行情频道--事件)
- 级别:unit
- 里程碑:M3
- 前置:`RD-OKX`;`OkxSwap`;报文 `{"arg":{"channel":"index-tickers","instId":"BTC-USDT"},"data":[{"instId":"BTC-USDT","idxPx":"64998.7","ts":"1790000000700"}]}`;第二条同形但 `instId = "DOGE-USDT"`
- 步骤:
  1. 两条各 `peek` 与 `decode_md`
- 期望:
  - 第一条:`peek.inst = 1501`;一条 `IndexPrice{h.inst: 1501, h.ex_ts_ns: 1790000000700000000, px: 6499870}`
  - 第二条:`peek.inst = 0`(refdata 无 `DOGE-USDT-SWAP`)
- 实现:`crates/venue-okx/tests/md.rs::okx_037_index_tickers_map_to_swap`

### OKX-038 现货产品线不认永续的标记价、资金费与指数报文

- 规则:[行情频道 → 事件](README.md#行情频道--事件)
- 级别:unit
- 里程碑:M3
- 前置:`RD-OKX`;`OkxSpot`;OKX-023 的 `mark-price` 报文(`BTC-USDT-SWAP`)
- 步骤:
  1. `peek(rd, ctx, MarkPrice, …)`
- 期望:
  - `Ok(Peek{inst: 0, …})`(`OKX_SPOT` 符号表无该符号)
- 实现:`crates/venue-okx/tests/md.rs::okx_038_spot_ignores_swap_channels`

### OKX-039 每种行情事件原样带出 RxMeta 的接收时间点

- 规则:[hft-venue-proto 契约 3](../hft-venue-proto/README.md#实现方必须遵守的契约)、[行情频道 → 事件](README.md#行情频道--事件)
- 级别:table
- 里程碑:M3
- 前置:`RD-OKX`;`OkxSwap`;`RX`;每行取 OKX-025、OKX-029、OKX-033.1、OKX-035、OKX-036.1、OKX-037 的报文
- 实现:`crates/venue-okx/tests/md.rs::okx_039_rxmeta_passthrough`

| 行 | 事件 | 期望 `MarketHdr` |
|---|---|---|
| OKX-039.1 | `Bbo` | `venue 5`,`conn 3`,`rx_tsc 1000`,`hw_rx_ns 1790000000126000000`,`sw_rx_ns 1790000000126500000`,`rx_empty_gap_ns 2500`,`_rsv 0` |
| OKX-039.2 | `L2Snapshot` 头 | 同上 |
| OKX-039.3 | `Trade` | 同上 |
| OKX-039.4 | `MarkPrice` | 同上 |
| OKX-039.5 | `Funding` | 同上 |
| OKX-039.6 | `IndexPrice` | 同上 |

### OKX-040 控制回执与心跳回应给出 ControlHint

- 规则:[1. 分派](README.md#1-分派)、[2. 订阅约束](README.md#2-订阅约束)、[用户流事件表](README.md#用户流事件表)
- 级别:table
- 里程碑:M3
- 前置:无参数;对报文先 `classify` 再 `control(c, payload)`
- 实现:`crates/venue-okx/tests/md.rs::okx_040_control_hints`

| 行 | 报文 | 期望 |
|---|---|---|
| OKX-040.1 | `{"event":"subscribe","arg":{"channel":"bbo-tbt","instId":"BTC-USDT-SWAP"},"connId":"a4d3ae55"}` | `Some(SubAck{ok: true, code: 0})` |
| OKX-040.2 | `{"event":"error","code":"60033","msg":"Parameter id error","connId":"a4d3ae55"}` | `Some(SubAck{ok: false, code: 60033})` |
| OKX-040.3 | `{"event":"error","code":"64003","msg":"…fee tier…"}` | `Some(SubAck{ok: false, code: 64003})` |
| OKX-040.4 | `{"event":"notice","code":"64008","msg":"The connection will soon be closed for a service upgrade."}` | `Some(ServerShutdown)` |
| OKX-040.5 | `pong` | `Some(Pong)` |
| OKX-040.6 | `{"event":"unsubscribe","arg":{"channel":"books5"}}` | `Some(SubAck{ok: true, code: 0})` |
| OKX-040.7 | OKX-021 的 `B1`(行情类) | `None` |

### OKX-041 以不处理的类调用 decode_md 返回 UnknownClass

- 规则:[1. 分派](README.md#1-分派)
- 级别:unit
- 里程碑:M3
- 前置:`RD-OKX`;OKX-040.1 的报文
- 步骤:
  1. `decode_md(rd, ctx, Control, …)`
  2. `decode_md(rd, ctx, Unknown, …)`
- 期望:
  - 两次都是 `Err(DecodeErr::UnknownClass)`,0 事件
- 实现:`crates/venue-okx/tests/md.rs::okx_041_decode_md_wrong_class`

### OKX-042 热路径方法零分配

- 规则:[hft-venue-proto 契约 5](../hft-venue-proto/README.md#实现方必须遵守的契约)、[延迟与资源契约](README.md#延迟与资源契约)
- 级别:unit
- 里程碑:M3
- 前置:`RD-OKX`;测试进程装计数分配器;`build_templates` 与 `GwCtx`/`FeedCtx` 构造在计数开始前完成
- 步骤:
  1. 依次各 10⁴ 次:`classify` + `peek` + `decode_md`(OKX-025、OKX-029、OKX-033.1、OKX-036.1 的报文)、`encode`(OKX-065 的 Place)、`classify_report` + `decode_report`(OKX-090 的响应、OKX-101 的成交推送、OKX-114 的 `account` 推送、OKX-113 的 `positions` 推送)
- 期望:
  - 计数期间分配次数 = 0,释放次数 = 0;无 panic
- 实现:`crates/venue-okx/tests/alloc.rs::okx_042_hot_path_zero_alloc`

### OKX-050 BBO 补帧请求每次只带一个 inst

- 规则:[心跳与 BBO 探测](README.md#心跳与-bbo-探测)、[hft-venue-proto 契约 24](../hft-venue-proto/README.md#实现方必须遵守的契约)
- 级别:unit
- 里程碑:M3
- 前置:`RD-OKX`;`OkxSwap`
- 步骤:
  1. `bbo_snapshot_request(rd, ctx, &[1501], &mut out)`
- 期望:
  - `Ok(true)`;`out.endpoint = 0`,`out.method = GET`,`out.path_query = "/api/v5/market/books?instId=BTC-USDT-SWAP&sz=1"`,`out.headers` 不含 `OK-ACCESS-*`,`out.redacted = false`
- 实现:`crates/venue-okx/tests/snap.rs::okx_050_bbo_snapshot_request`
- 备注:依赖待核实:开发备忘第 15 条(订阅即推首帧时改为返回 `Ok(false)`,本用例随之修改)

### OKX-051 BBO 补帧请求的 inst 数不为 1 时不受支持

- 规则:[心跳与 BBO 探测](README.md#心跳与-bbo-探测)
- 级别:table
- 里程碑:M3
- 前置:`RD-OKX`;`OkxSwap`;每行调用前 `out` 清空
- 实现:`crates/venue-okx/tests/snap.rs::okx_051_bbo_snapshot_request_arity`

| 行 | `insts` | 期望 |
|---|---|---|
| OKX-051.1 | `[]` | `Err(ProtoErr::Unsupported)`,`out.path_query` 为空 |
| OKX-051.2 | `[1501, 1502]`(超过 `BBO_SNAPSHOT_BATCH = 1`) | `Err(ProtoErr::BufFull)`,`out.path_query` 为空 |
| OKX-051.3 | `[1601]`(不属于 `OKX_SWAP`) | `Err(ProtoErr::Unsupported)` |

### OKX-052 BBO 补帧响应解码为一条 Bbo

- 规则:[心跳与 BBO 探测](README.md#心跳与-bbo-探测)
- 级别:unit
- 里程碑:M3
- 前置:`RD-OKX`;`OkxSwap`;snap-fetch 自己的 `FeedCtx`;请求的 inst = 1501;响应体 `{"code":"0","msg":"","data":[{"asks":[["65000.2","12","0","3"]],"bids":[["65000.1","150","0","5"]],"ts":"1790000000900","seqId":4000200}]}`;`RX`
- 步骤:
  1. `decode_bbo_snapshot(rd, &mut ctx, &[1501], body, &RX, &mut sink)`
- 期望:
  - `Ok(1)`;一条 `Bbo{h.inst: 1501, h.ex_ts_ns: 1790000000900000000, bid_px: 6500010, bid_qty: 150000000, ask_px: 6500020, ask_qty: 12000000, update_id: 4000200}`
- 实现:`crates/venue-okx/tests/snap.rs::okx_052_bbo_snapshot_decode`
- 备注:响应体不带 `instId`,inst 取自传入的 `insts`

### OKX-053 BBO 补帧响应的失败形态

- 规则:[心跳与 BBO 探测](README.md#心跳与-bbo-探测)
- 级别:table
- 里程碑:M3
- 前置:同 OKX-052;每行替换响应体
- 实现:`crates/venue-okx/tests/snap.rs::okx_053_bbo_snapshot_failures`

| 行 | 响应体 | 期望 |
|---|---|---|
| OKX-053.1 | `{"code":"51001","msg":"Instrument ID does not exist","data":[]}` | `Ok(0)`,0 事件 |
| OKX-053.2 | `{"code":"0","msg":"","data":[]}` | `Ok(0)`,0 事件 |
| OKX-053.3 | OKX-052 的响应体在 `"seqId"` 前截断 | `Err(DecodeErr::Truncated)` |
| OKX-053.4 | `bids[0][0] = "x"` | `Err(DecodeErr::Number)` |
| OKX-053.5 | 非 JSON 字节 `<html>` | `Err(DecodeErr::Shape)` |

### OKX-054 快照解码不依赖也不改变行情解码的上下文

- 规则:[hft-venue-proto 契约 25](../hft-venue-proto/README.md#实现方必须遵守的契约)、[心跳与 BBO 探测](README.md#心跳与-bbo-探测)
- 级别:unit
- 里程碑:M3
- 前置:`RD-OKX`;`OkxSwap`;`ctx_a` = 新建 `FeedCtx`,`ctx_b` = 已对 OKX-025/OKX-029/OKX-036.1 报文各解码 10⁴ 次的 `FeedCtx`;OKX-052 的响应体
- 步骤:
  1. 分别在 `ctx_a`、`ctx_b` 上 `decode_bbo_snapshot`
  2. 在 `ctx_b` 上再解码一条 `funding-rate`(OKX-036.2 的报文)
- 期望:
  - 步骤 1 两次的 sink 输出逐字节相同
  - 步骤 2 的 `Funding.interval_s = 14400`(与未做快照解码时相同)
- 实现:`crates/venue-okx/tests/snap.rs::okx_054_snapshot_isolation`

### OKX-060 login 帧按秒级时间戳签名

- 规则:[下单:WS `/private`](README.md#下单ws-private)、[5. 心跳与会话](README.md#5-心跳与会话)
- 级别:unit
- 里程碑:M3
- 前置:`KEY`;`now_ms = 1538054050000`;`OkxSwap`
- 步骤:
  1. `login(Order, cfg, cred, now_ms, &mut out)`
- 期望:
  - 返回 `Ok(LoginStep::Frame)`
  - `out` 逐字节等于 `{"op":"login","args":[{"apiKey":"k-test","passphrase":"p-test","timestamp":"1538054050","sign":"+LdIr8lkkvhr5hoA3g9TMC0+uQJ849ftAcocA/ouu4M="}]}`(`sign = Base64(HMAC-SHA256(secret, "1538054050GET/users/self/verify"))`);帧中没有 `"id"` 键
- 实现:`crates/venue-okx/tests/session.rs::okx_060_login_frame`

### OKX-061 login 的秒级时间戳向下取整

- 规则:[下单:WS `/private`](README.md#下单ws-private)
- 级别:unit
- 里程碑:M3
- 前置:同 OKX-060,但 `now_ms = 1538054050999`
- 步骤:
  1. `login(Order, …)`
- 期望:
  - `"timestamp":"1538054050"`,`sign` 与 OKX-060 相同
- 实现:`crates/venue-okx/tests/session.rs::okx_061_login_ts_floor`

### OKX-062 login_result 区分成功、拒绝与非登录回执

- 规则:[下单:WS `/private`](README.md#下单ws-private)、[用户流事件表](README.md#用户流事件表)
- 级别:table
- 里程碑:M3
- 前置:无参数
- 实现:`crates/venue-okx/tests/session.rs::okx_062_login_result`

| 行 | 报文 | 期望 |
|---|---|---|
| OKX-062.1 | `{"event":"login","code":"0","msg":"","connId":"a4d3ae55"}` | `LoginResult::Ok` |
| OKX-062.2 | `{"event":"error","code":"60009","msg":"Login failed.","connId":"a4d3ae55"}` | `Rejected(60009)` |
| OKX-062.3 | `{"event":"error","code":"60004","msg":"Invalid timestamp"}` | `Rejected(60004)` |
| OKX-062.4 | `{"event":"error","code":"60024","msg":"Wrong passphrase"}` | `Rejected(60024)` |
| OKX-062.5 | `{"event":"subscribe","arg":{"channel":"orders"}}` | `NotMine` |
| OKX-062.6 | `{"id":"p0000002a00000007","op":"order","code":"0"` … | `NotMine` |
| OKX-062.7 | `pong` | `NotMine` |
| OKX-062.8 | 非 JSON 字节 `\x00\x01` | `NotMine` |

### OKX-063 公共行情连接不登录

- 规则:[端点与路由](README.md#端点与路由)
- 级别:unit
- 里程碑:M3
- 前置:`KEY`;`now_ms = 1790000000000`
- 步骤:
  1. `login(Md, cfg, cred, now_ms, &mut out)`
- 期望:
  - `Ok(LoginStep::NotNeeded)`;`out.len() == 0`
- 实现:`crates/venue-okx/tests/session.rs::okx_063_md_no_login`

### OKX-064 实现方不在任何上下文中保留密钥材料

- 规则:[hft-venue-proto 契约 7](../hft-venue-proto/README.md#实现方必须遵守的契约)
- 级别:unit
- 里程碑:M3
- 前置:`RD-OKX`;`KEY`;两个产品线
- 步骤:
  1. `login(Order, …)`;`build_templates(cfg, rd, 2)`;构造 `GwCtx`、`FeedCtx`、`AcctCtx`;对每个 Config 步调用 `acct_request`
  2. 扫描 `Templates`、`GwCtx`、`FeedCtx`、`AcctCtx`、`AcctScratch` 的全部字节
- 期望:
  - 扫描结果中不出现 `secret`、`passphrase` 的字节串,也不出现 OKX-060 的 `sign` 文本
- 实现:`crates/venue-okx/tests/session.rs::okx_064_no_secret_retained`

### OKX-065 永续 Place 编码为固定模板的 order 帧

- 规则:[3. 订单帧模板](README.md#3-订单帧模板)、[下单:WS `/private`](README.md#下单ws-private)
- 级别:unit
- 里程碑:M3
- 前置:`RD-OKX`;`OkxSwap`;`gw.stale_guard_ms = 1500`;`build_templates(cfg, rd, 2)`;`WC`;`c = GwCmd{op: Place, inst: 1501, side: Sell, tif: Ioc, px: 6500010, qty: 0.04(4000000), slot: 42, gen: 7, account: 2, oflags: 0}`;client id = `CID(3, 42, 7)`(gw 渲染后经模板写入)
- 步骤:
  1. `encode(rd, &t, &c, &tgt, &WC, &mut body)`(`body` 2048 B)
- 期望:
  - 返回 `Ok(n)`,`body[..n]` 逐字节等于 `{"id":"p0000002a00000007","op":"order","expTime":"1790000001500","args":[{"instIdCode":10459,"tdMode":"cross","side":"sell","ordType":"ioc","sz":"4","px":"65000.1","clOrdId":"1000305020002a000000077d","reduceOnly":false}]}`
  - 帧中没有 `"instId"` 键、没有 `rpiTakerAccess` 键
- 实现:`crates/venue-okx/tests/encode.rs::okx_065_place_swap_frame`

### OKX-066 Place 的方向、TIF 与 reduceOnly 映射

- 规则:[下单:WS `/private`](README.md#下单ws-private)、[3. 订单帧模板](README.md#3-订单帧模板)、[hft-venue-proto 契约 11](../hft-venue-proto/README.md#实现方必须遵守的契约)
- 级别:table
- 里程碑:M3
- 前置:同 OKX-065,每行只改所列字段;断言帧中对应键的值
- 实现:`crates/venue-okx/tests/encode.rs::okx_066_place_field_mapping`

| 行 | 改动 | 期望帧片段 |
|---|---|---|
| OKX-066.1 | `tif = Gtc` | `"ordType":"limit"` |
| OKX-066.2 | `tif = PostOnly` | `"ordType":"post_only"` |
| OKX-066.3 | `tif = Fok` | `"ordType":"fok"` |
| OKX-066.4 | `side = Buy` | `"side":"buy"` |
| OKX-066.5 | `oflags = REDUCE_ONLY` | `"reduceOnly":true` |
| OKX-066.6 | `oflags = RISK_REDUCING`(不带 `REDUCE_ONLY`) | `"reduceOnly":false`(venue 不推断) |
| OKX-066.7 | `oflags = SMOKE` | 帧与 OKX-065 逐字节相同 |
| OKX-066.8 | `oflags = POST_ONLY`,`tif = Ioc` | `"ordType":"ioc"`(以 `tif` 为准) |

### OKX-067 现货 Place 用 cash 模式且不写 reduceOnly

- 规则:[3. 订单帧模板](README.md#3-订单帧模板)、[下单:WS `/private`](README.md#下单ws-private)
- 级别:unit
- 里程碑:M3
- 前置:`RD-OKX`;`OkxSpot`;`gw.stale_guard_ms = 1500`;`WC`;`c = GwCmd{op: Place, inst: 1601, side: Buy, tif: Gtc, px: 6500010, qty: 0.002(200000), slot: 42, gen: 7, oflags: 0}`;client id = `OKX_SPOT` 的 `CID(3, 42, 7)`
- 步骤:
  1. `encode`
- 期望:
  - `body[..n]` = `{"id":"p0000002a00000007","op":"order","expTime":"1790000001500","args":[{"instIdCode":10001,"tdMode":"cash","side":"buy","ordType":"limit","sz":"0.002","px":"65000.1","clOrdId":"1000306020002a000000074d"}]}`
- 实现:`crates/venue-okx/tests/encode.rs::okx_067_place_spot_frame`

### OKX-068 现货命令带 REDUCE_ONLY 返回 Unsupported,不丢弃该标志

- 规则:[3. 订单帧模板](README.md#3-订单帧模板)、[hft-venue-proto 契约 11](../hft-venue-proto/README.md#实现方必须遵守的契约)
- 级别:unit
- 里程碑:M3
- 前置:同 OKX-067,`oflags = REDUCE_ONLY`
- 步骤:
  1. `encode`
- 期望:
  - `Err(EncodeErr::Unsupported(GwOp::Place))`
- 实现:`crates/venue-okx/tests/encode.rs::okx_068_spot_reduce_only_unsupported`

### OKX-069 价格与张数按线格式指数格式化为最短十进制

- 规则:[下单:WS `/private`](README.md#下单ws-private)、[3. 订单帧模板](README.md#3-订单帧模板)
- 级别:table
- 里程碑:M3
- 前置:`RD-OKX`;每行一条 Place,断言帧中 `sz`/`px` 的值;不经 f64
- 实现:`crates/venue-okx/tests/encode.rs::okx_069_wire_formatting`

| 行 | inst、`Qty` / `Px` raw | 期望 |
|---|---|---|
| OKX-069.1 | 1501,`qty = 4000000`(0.04 BTC) | `"sz":"4"` |
| OKX-069.2 | 1501,`qty = 150000000`(1.5 BTC) | `"sz":"150"` |
| OKX-069.3 | 1501,`qty = 1000000`(0.01 BTC) | `"sz":"1"` |
| OKX-069.4 | 1502,`qty = 10000000`(0.1 ETH,ctVal 0.1) | `"sz":"1"` |
| OKX-069.5 | 1502,`qty = 235000000`(2.35 ETH) | `"sz":"23.5"` |
| OKX-069.6 | 1503,`qty = 10000000`(10⁷ PEPE) | `"sz":"1"` |
| OKX-069.7 | 1503,`qty = 25000000` | `"sz":"2.5"` |
| OKX-069.8 | 1601,`qty = 200000` | `"sz":"0.002"` |
| OKX-069.9 | 1501,`px = 6500000` | `"px":"65000"` |
| OKX-069.10 | 1501,`px = 1` | `"px":"0.01"` |
| OKX-069.11 | 1503,`px = 123456`(`px_exp = −10`) | `"px":"0.0000123456"` |

### OKX-070 任意 lot 对齐数量经线格式指数编码再解析逐位相等

- 规则:[下单:WS `/private`](README.md#下单ws-private)
- 级别:prop
- 里程碑:M3
- 性质:对任意 `Qty`(lot 整数倍,raw ∈ [1, 10¹⁵])与 `ctVal ∈ {1, 0.1, 0.01, 0.001, 10, 1000}`(`qty_unit_shift ∈ {0, −1, −2, −3, 1, 3}`),`encode` 写出的 `sz` 经 `parse_exact(sz, qty_wire_exp)` 读回等于原 `Qty`;`sz` 的十进制值等于 i128 参考 `qty / ctVal`(精确,无舍入)
- 生成:qty_exp ∈ {−8, −6, 0};lot ∈ {1, 10, 1000} raw;数量 = lot × k,k ∈ [1, 10⁹];shift 从上列均匀抽取
- 参考:i128 十进制参考实现 `qty × 10^(qty_exp) / ctVal`,渲染为最短十进制
- 例数:PR 10 000
- 实现:`crates/venue-okx/tests/prop_qty.rs::okx_070_qty_wire_roundtrip`

### OKX-071 运行时代码不出现 ctVal 乘除

- 规则:[下单:WS `/private`](README.md#下单ws-private)
- 级别:unit
- 里程碑:M3
- 前置:源码树 `crates/venue-okx/src/**/*.rs`
- 步骤:
  1. 去掉注释后扫描标识符 `ct_val`、`ctVal`、`ct_mult`;扫描数量路径的格式化与解析调用
- 期望:
  - 三个标识符出现 0 次;数量的格式化只调用 `write_fixed(…, qty_wire_exp)`、解析只调用 `parse_exact(…, qty_wire_exp)`(静态检查清单逐项列出调用点,任一调用点用 `qty_exp` 即失败)
- 实现:`crates/venue-okx/tests/static_checks.rs::okx_071_no_ctval_arithmetic`

### OKX-072 Cancel 编码为按 clOrdId 的 cancel-order 帧,不带 expTime

- 规则:[下单:WS `/private`](README.md#下单ws-private)
- 级别:unit
- 里程碑:M3
- 前置:`RD-OKX`;`OkxSwap`;`WC`;`c = GwCmd{op: Cancel, inst: 1501, slot: 42, gen: 7, target_slot: 42}`;`tgt = TargetRef{id: CID(3, 42, 7), exch_order_id: 0}`
- 步骤:
  1. `encode`
- 期望:
  - `body[..n]` = `{"id":"c0000002a00000007","op":"cancel-order","args":[{"instIdCode":10459,"clOrdId":"1000305020002a000000077d"}]}`
  - `tgt.exch_order_id = 0` 不导致 `UnknownTarget`;帧中没有 `ordId`、`expTime`
- 实现:`crates/venue-okx/tests/encode.rs::okx_072_cancel_frame`

### OKX-073 Amend 编码为带新价新量与 expTime 的 amend-order 帧

- 规则:[下单:WS `/private`](README.md#下单ws-private)
- 级别:unit
- 里程碑:M3
- 前置:`RD-OKX`;`OkxSwap`;`gw.stale_guard_ms = 1500`;`WC`;`c = GwCmd{op: Amend, inst: 1501, px: 6500020, qty: 5000000, slot: 42, gen: 7, target_slot: 42}`;`tgt.id = CID(3, 42, 7)`
- 步骤:
  1. `encode`
- 期望:
  - `body[..n]` = `{"id":"m0000002a00000007","op":"amend-order","expTime":"1790000001500","args":[{"instIdCode":10459,"clOrdId":"1000305020002a000000077d","newSz":"5","newPx":"65000.2"}]}`
- 实现:`crates/venue-okx/tests/encode.rs::okx_073_amend_frame`

### OKX-074 单条 CancelAll 不由 encode 编码

- 规则:[下单:WS `/private`](README.md#下单ws-private)
- 级别:unit
- 里程碑:M3
- 前置:`RD-OKX`;两个产品线;`c = GwCmd{op: CancelAll, account: 2, inst: 0}`
- 步骤:
  1. `encode`
- 期望:
  - `Err(EncodeErr::Unsupported(GwOp::CancelAll))`
- 实现:`crates/venue-okx/tests/encode.rs::okx_074_cancel_all_not_single_frame`

### OKX-075 Query 生成按 clOrdId 的签名 REST 查单请求

- 规则:[下单:WS `/private`](README.md#下单ws-private)
- 级别:unit
- 里程碑:M3
- 前置:`RD-OKX`;`OkxSwap`;`KEY`;`WC`;`c = GwCmd{op: Query, inst: 1501, slot: 42, gen: 7, target_slot: 42}`;`tgt.id = CID(3, 42, 7)`
- 步骤:
  1. `order_channel(Query)`;`encode_rest(rd, &t, &c, &tgt, &WC, &mut out)`
  2. 对同一 `c` 调用 `encode`
- 期望:
  - 步骤 1:`order_channel` 为 `Rest`;`RestReqBuf{endpoint: 0, method: GET, path_query: "/api/v5/trade/order?instId=BTC-USDT-SWAP&clOrdId=1000305020002a000000077d", redacted: true}`;头 `OK-ACCESS-KEY: k-test`、`OK-ACCESS-TIMESTAMP: 2026-09-21T14:13:20.000Z`、`OK-ACCESS-PASSPHRASE: p-test`、`OK-ACCESS-SIGN: JUbEhizBypVPPDag3ME22/h/FGo9tnk0/sTy95IeLAk=`;无 `x-simulated-trading`
  - 步骤 2:`Err(EncodeErr::Unsupported(GwOp::Query))`(WS 没有查单)
- 实现:`crates/venue-okx/tests/rest.rs::okx_075_query_rest_request`

### OKX-076 编码的失败路径

- 规则:[3. 订单帧模板](README.md#3-订单帧模板)、[hft-venue-proto 契约 5](../hft-venue-proto/README.md#实现方必须遵守的契约)
- 级别:table
- 里程碑:M3
- 前置:`RD-OKX`;`OkxSwap`;基准为 OKX-065 的 `c`;每行只改所列项
- 实现:`crates/venue-okx/tests/encode.rs::okx_076_encode_failures`

| 行 | 改动 | 期望 |
|---|---|---|
| OKX-076.1 | `inst = 1601`(`OKX_SPOT` 的 inst) | `Err(EncodeErr::UnknownInst)` |
| OKX-076.2 | `inst = 9999`(refdata 无) | `Err(EncodeErr::UnknownInst)` |
| OKX-076.3 | `body` 长 64 B | `Err(EncodeErr::FieldOverflow)` |
| OKX-076.4 | `body` 长 = OKX-065 帧长 − 1 | `Err(EncodeErr::FieldOverflow)` |
| OKX-076.5 | `body` 长 = OKX-065 帧长 | `Ok(帧长)` |
| OKX-076.6 | `op = DeadManRefresh`(维护类) | `Err(EncodeErr::Unsupported(GwOp::DeadManRefresh))` |

### OKX-077 最长输入的单单帧不超过 400 字节

- 规则:[3. 订单帧模板](README.md#3-订单帧模板)
- 级别:unit
- 里程碑:M3
- 前置:`RD-OKX`;`OkxSwap`;`c` 取最长输入:`px = 9223372036854775807`(raw 上限)、`qty` 为 lot 对齐的最大值、`slot = 0xFFFFF`、`gen = 0xFFFFFFFF`、`oflags = REDUCE_ONLY`、`tif = PostOnly`;Place 与 Amend 各一次
- 步骤:
  1. `encode`
- 期望:
  - 两次都 `Ok(n)` 且 `n ≤ 400`
- 实现:`crates/venue-okx/tests/encode.rs::okx_077_frame_len_bound`

### OKX-078 expTime 等于当前毫秒加 stale_guard_ms

- 规则:[下单:WS `/private`](README.md#下单ws-private)
- 级别:table
- 里程碑:M3
- 前置:`RD-OKX`;`OkxSwap`;`WC`;每行以所列 `gw.stale_guard_ms` 调用 `build_templates` 后编码 OKX-065 的 `c`
- 实现:`crates/venue-okx/tests/encode.rs::okx_078_exptime`

| 行 | `gw.stale_guard_ms` | 期望 |
|---|---|---|
| OKX-078.1 | 1500 | `"expTime":"1790000001500"` |
| OKX-078.2 | 1000 | `"expTime":"1790000001000"` |
| OKX-078.3 | 1500,`WC.unix_ms = 1790000000999` | `"expTime":"1790000002499"` |

### OKX-080 encode_batch 把 20 条 Place 合成一帧 batch-orders

- 规则:[下单:WS `/private`](README.md#下单ws-private)
- 级别:unit
- 里程碑:M3
- 前置:`RD-OKX`;`OkxSwap`;`WC`;`cs` = 20 条 Place(slot 42–61,gen 7,inst 1501,其余同 OKX-065)
- 步骤:
  1. `encode_batch(rd, &t, &cs, &tgts, &WC, &mut body)`(`body` 8192 B)
- 期望:
  - `Ok(n)`,`n ≤ 8192`;帧以 `{"id":"b0000002a00000007","op":"batch-orders","expTime":"1790000001500","args":[` 开头;`args` 恰 20 项,第 k 项的 `clOrdId` = `CID(3, 42 + k, 7)`,各项字段与 OKX-065 单单帧的 `args[0]` 相同
- 实现:`crates/venue-okx/tests/encode.rs::okx_080_batch_place`

### OKX-081 encode_batch 把撤单合成 batch-cancel-orders,不带 expTime

- 规则:[下单:WS `/private`](README.md#下单ws-private)
- 级别:unit
- 里程碑:M3
- 前置:`RD-OKX`;`OkxSwap`;`cs` = 20 条 Cancel(目标 slot 42–61),`tgts[k].id = CID(3, 42 + k, 7)`
- 步骤:
  1. `encode_batch`
- 期望:
  - 帧 = `{"id":"x0000002a00000007","op":"batch-cancel-orders","args":[{"instIdCode":10459,"clOrdId":"…"} × 20]}`;不含 `expTime`
- 实现:`crates/venue-okx/tests/encode.rs::okx_081_batch_cancel`

### OKX-082 encode_batch 的上限与混合命令

- 规则:[下单:WS `/private`](README.md#下单ws-private)
- 级别:table
- 里程碑:M3
- 前置:`RD-OKX`;`OkxSwap`;`WC`
- 实现:`crates/venue-okx/tests/encode.rs::okx_082_batch_limits`

| 行 | `cs` | 期望 |
|---|---|---|
| OKX-082.1 | 19 条 Place | `Ok`,`args` 19 项 |
| OKX-082.2 | 20 条 Place | `Ok`,`args` 20 项 |
| OKX-082.3 | 21 条 Place | `Err(EncodeErr::FieldOverflow)` |
| OKX-082.4 | 21 条 Cancel | `Err(EncodeErr::FieldOverflow)` |
| OKX-082.5 | Place, Place, Cancel | `Err(EncodeErr::Unsupported(GwOp::Cancel))` |
| OKX-082.6 | Cancel, CancelAll | `Err(EncodeErr::Unsupported(GwOp::CancelAll))` |
| OKX-082.7 | 3 条 Amend | `Ok`,`"op":"batch-amend-orders"`,id 以 `n` 开头,带 `expTime` |

### OKX-083 请求 id 的编码与解码

- 规则:[使用本组件](README.md#使用本组件)、[3. 订单帧模板](README.md#3-订单帧模板)
- 级别:table
- 里程碑:M3
- 前置:无参数;"解码"= 从响应 `id` 还原 `ReportOutcome.req`(经 OKX-090 的路径)
- 实现:`crates/venue-okx/tests/cid.rs::okx_083_request_id`

| 行 | 输入 | 期望 |
|---|---|---|
| OKX-083.1 | 编码 `(Place, slot 42, gen 7)` | `"p0000002a00000007"`(17 字符) |
| OKX-083.2 | 编码 `(Cancel, 0xFFFFF, 0xFFFFFFFF)` | `"c000fffffffffffff"` |
| OKX-083.3 | 解码 `"m0000002a00000007"` | `ReqRef{slot: 42, gen: 7, op: Amend}` |
| OKX-083.4 | 解码 `"p0000002A00000007"`(大写) | `None` |
| OKX-083.5 | 解码 `"p0000002a0000000"`(16 字符) | `None` |
| OKX-083.6 | 解码 `"p0000002a000000077"`(18 字符) | `None` |
| OKX-083.7 | 解码 `"z0000002a00000007"` | `None` |
| OKX-083.8 | 解码 `"1512"` | `None` |
| OKX-083.9 | 解码 `"b0000002a00000007"` | `ReqRef{slot: 42, gen: 7, op: Place}`(批量帧取第一单) |
| OKX-083.10 | 解码 `"x0000002a00000007"` | `ReqRef{slot: 42, gen: 7, op: Cancel}` |

### OKX-084 clOrdId 与请求 id 只含字母数字、不超过 32 字符且往返一致

- 规则:[使用本组件](README.md#使用本组件)、[3. 订单帧模板](README.md#3-订单帧模板)
- 级别:prop
- 里程碑:M3
- 性质:对任意 `ClientOrderId`(epoch ∈ [0, 2¹⁵),venue ∈ {5, 6},account ∈ [0, 255],slot ∈ [0, 2²⁰),gen ∈ u32)与任意 `(op, slot, gen)`:`render_cid` 输出匹配 `^[0-9a-f]{24}$` 且 `parse_cid(render) == Some(id)`;请求 id 匹配 `^[pcmbxn][0-9a-f]{16}$`,长度 17 ≤ 32,解码回原值
- 生成:上述取值域均匀抽取
- 参考:hft-types `ClientOrderId::encode/decode`
- 例数:PR 10 000
- 实现:`crates/venue-okx/tests/cid.rs::okx_084_ids_alnum_roundtrip`

### OKX-085 parse_cid 只接受规范的 24 位小写十六进制

- 规则:[使用本组件](README.md#使用本组件)
- 级别:table
- 里程碑:M3
- 前置:`OkxSwap`
- 实现:`crates/venue-okx/tests/cid.rs::okx_085_parse_cid`

| 行 | 输入 | 期望 |
|---|---|---|
| OKX-085.1 | `1000305020002a000000077d` | `Some(ClientOrderId{epoch: 3, venue: 5, account: 2, slot: 42, gen: 7})` |
| OKX-085.2 | `1000305020002a000000077c`(`CID✗`) | `None` |
| OKX-085.3 | `1000305020002A000000077D` | `None` |
| OKX-085.4 | `1000305020002a000000077d0`(25 字符) | `None` |
| OKX-085.5 | `1000305020002a000000077`(23 字符) | `None` |
| OKX-085.6 | `1000305-20002a000000077d` | `None` |
| OKX-085.7 | 空串 | `None` |
| OKX-085.8 | `web1690000000` | `None` |
| OKX-085.9 | `1800305020002a0000000775`(保留位 = 1) | `None` |

### OKX-086 render_cid 原样写出规范文本

- 规则:[使用本组件](README.md#使用本组件)
- 级别:unit
- 里程碑:M3
- 前置:`OkxSwap`;`id = ClientOrderId{epoch: 3, venue: 5, account: 2, slot: 42, gen: 7}`;`out = [0xAA; 48]`
- 步骤:
  1. `render_cid(&id, &mut out)`
- 期望:
  - 返回 24;`out[..24] == b"1000305020002a000000077d"`;`out[24..]` 仍为 `0xAA`
- 实现:`crates/venue-okx/tests/cid.rs::okx_086_render_cid`

### OKX-087 不需要逐单签名

- 规则:[下单:WS `/private`](README.md#下单ws-private)
- 级别:unit
- 里程碑:M3
- 前置:无参数
- 步骤:
  1. 两个产品线调用 `needs_per_order_signing()`
- 期望:
  - 都返回 `false`
- 实现:`crates/venue-okx/tests/consts.rs::okx_087_no_per_order_signing`

### OKX-090 order 成功响应产出 Ack 并登记交易所订单 id

- 规则:[4. 回报](README.md#4-回报)、[hft-venue-proto 契约 13](../hft-venue-proto/README.md#实现方必须遵守的契约)
- 级别:unit
- 里程碑:M3
- 前置:`RD-OKX`;`OkxSwap`;`RX`;响应 `R0` = `{"id":"p0000002a00000007","op":"order","code":"0","msg":"","data":[{"clOrdId":"1000305020002a000000077d","ordId":"312269865356374016","tag":"","ts":"1790000000150","sCode":"0","sMsg":"Order placed"}],"inTime":"1790000000140100","outTime":"1790000000145300"}`
- 步骤:
  1. `classify_report(Text, R0)` 后 `decode_report(rd, &mut gw, OrderResp, R0, &RX, &mut sink)`
- 期望:
  - 恰好一条 `OrderEvent{what: Ack, venue: 5, account: 2, inst: 1501, epoch: 3, slot: 42, gen: 7, exch_order_id: 312269865356374016, ex_ts_ns: 1790000000145300000, rx_tsc: 1000, reject_code: 0}`
  - `sink.exch_id(key = "312269865356374016", owner = Some(CID(3, 42, 7)))` 被调用一次
  - `ReportOutcome{req: Some(ReqRef{slot: 42, gen: 7, op: Place}), rate: None, gw_dwell_us: 5200, events: 1, outcome_unknown: None, raw_code: 0, skip: No, hint: None, fee_rule_deviation: false}`
- 实现:`crates/venue-okx/tests/reports.rs::okx_090_order_ack`

### OKX-091 单单响应的错误码按两张表映射

- 规则:[结果未知码表](README.md#结果未知码表)、[hft-venue-proto 契约 6、8](../hft-venue-proto/README.md#实现方必须遵守的契约)
- 级别:table
- 里程碑:M3
- 前置:`RD-OKX`;`OkxSwap`;生效 `RateLimitTable` 第一条 `Orders` 行 = `(Inst, Orders, 2000 ms, 60)`;响应形如 OKX-090 的 `R0`,但顶层 `code = "1"`、`data[0].sCode` 与 `sMsg` 按行替换(行注明"顶层"时改顶层 `code` 且 `data = []`);"Reject" 指一条 `OrderEvent{what: Reject, slot: 42, gen: 7, reject_code = raw}`,"未知" 指 0 条 `OrderEvent` 且 `outcome_unknown = Some(ReqRef{42, 7, Place})`;每行另断言 `ReportOutcome.raw_code = raw`
- 实现:`crates/venue-okx/tests/codes.rs::okx_091_order_code_table`

| 行 | 码 | 期望(raw;其余输出) |
|---|---|---|
| OKX-091.1 | `50001` | 未知(50001) |
| OKX-091.2 | `50004` | 未知(50004) |
| OKX-091.3 | `50013` | 未知(50013) |
| OKX-091.4 | `50026` | 未知(50026) |
| OKX-091.5 | `59999`(未登记) | 未知(59999) |
| OKX-091.6 | `50038`(未登记;模拟盘功能不可用) | 未知(50038) |
| OKX-091.7 | `"abc"`(非数字) | 未知(`raw_code_str(b"abc")`,落在 `0x4000_0000..=0x40FF_FFFF`) |
| OKX-091.8 | `50011` | Reject(50011);`ctl = RateLimited`;`rate = Some(RateObs{bucket: Orders, used: 60, limit: 60, window_ms: 2000})` |
| OKX-091.9 | `50036` | Reject(50036) |
| OKX-091.10 | `50037` | Reject(50037) |
| OKX-091.11 | `50017` | Reject(50017);`hint = MarginNow` |
| OKX-091.12 | `50018` | Reject(50018);`hint = MarginNow` |
| OKX-091.13 | `50019` | Reject(50019);`hint = MarginNow` |
| OKX-091.14 | `50020` | Reject(50020);`hint = MarginNow` |
| OKX-091.15 | `50021` | Reject(50021);`hint = MarginNow` |
| OKX-091.16 | `50022` | Reject(50022);`hint = MarginNow` |
| OKX-091.17 | `50027` | Reject(50027);`hint = None` |
| OKX-091.18 | `50029` | Reject(50029) |
| OKX-091.19 | `51006` | Reject(51006) |
| OKX-091.20 | `51008` | Reject(51008) |
| OKX-091.21 | `51008_1000` | Reject(51008)(子码不进 raw) |
| OKX-091.22 | `sCode = 51008`,另带 `"subCode":"1001"` | Reject(51008) |
| OKX-091.23 | `51400`(出现在 `order` 上) | Reject(51400) |
| OKX-091.24 | 顶层 `code = "50001"`,`data = []` | 未知(50001) |
| OKX-091.25 | 顶层 `code = "50011"`,`data = []` | Reject(50011),`slot/gen` 取自请求 id |
| OKX-091.26 | 顶层 `code = "0"`,`sCode = "0"` | `Ack`(同 OKX-090),`raw_code = 0` |

### OKX-092 撤单与改单响应的产出

- 规则:[结果未知码表](README.md#结果未知码表)、[4. 回报](README.md#4-回报)
- 级别:table
- 里程碑:M3
- 前置:`RD-OKX`;`OkxSwap`;响应 `{"id":<id>,"op":<op>,"code":<c>,"msg":"","data":[{"clOrdId":"1000305020002a000000077d","ordId":"312269865356374016","sCode":<s>,"sMsg":""}],"inTime":"1790000000140100","outTime":"1790000000145300"}`
- 实现:`crates/venue-okx/tests/codes.rs::okx_092_cancel_amend_responses`

| 行 | op、id、c、s | 期望 |
|---|---|---|
| OKX-092.1 | `cancel-order`,`c0000002a00000007`,`"0"`,`"0"` | 0 事件;`req = Some(ReqRef{42, 7, Cancel})` |
| OKX-092.2 | `cancel-order`,…,`"1"`,`"51400"` | 一条 `OrderEvent{what: CancelReject, slot: 42, gen: 7, reject_code: 51400, detail: GONE}` |
| OKX-092.3 | `cancel-order`,…,`"1"`,`"50004"` | 0 条 `OrderEvent`;`outcome_unknown = Some(ReqRef{42, 7, Cancel})`,`raw_code = 50004` |
| OKX-092.4 | `amend-order`,`m0000002a00000007`,`"0"`,`"0"` | 0 事件;`req = Some(ReqRef{42, 7, Amend})` |
| OKX-092.5 | `amend-order`,…,`"1"`,`"51008"` | 一条 `OrderEvent{what: AmendReject, reject_code: 51008}` |
| OKX-092.6 | `amend-order`,…,`"1"`,`"50001"` | 0 条 `OrderEvent`;`outcome_unknown = Some(ReqRef{42, 7, Amend})` |

### OKX-093 批量部分成功逐条产出,结果未知的条目直接产出 OutcomeUnknown

- 规则:[结果未知码表](README.md#结果未知码表)
- 级别:unit
- 里程碑:M3
- 前置:`RD-OKX`;`OkxSwap`;响应 `{"id":"b0000002a00000007","op":"batch-orders","code":"2","msg":"","data":[{"clOrdId":"1000305020002a000000077d","ordId":"11","sCode":"0","sMsg":""},{"clOrdId":"1000305020002b000000017a","ordId":"","sCode":"51008","sMsg":""},{"clOrdId":"1000305020002c000000027e","ordId":"","sCode":"50004","sMsg":""}],"inTime":"1790000000140100","outTime":"1790000000145300"}`
- 步骤:
  1. `decode_report(rd, &mut gw, BatchResp, …)`
- 期望:
  - 依次三条 `OrderEvent`:`{Ack, slot 42, gen 7, exch_order_id 11}`、`{Reject, slot 43, gen 1, reject_code 51008}`、`{OutcomeUnknown, slot 44, gen 2, reject_code 50004, qty 0, cum_filled 0}`
  - `ReportOutcome{events: 3, outcome_unknown: None, req: Some(ReqRef{42, 7, Place})}`
- 实现:`crates/venue-okx/tests/codes.rs::okx_093_batch_partial`

### OKX-094 批量请求整体结果未知时只能交出第一单

- 规则:[结果未知码表](README.md#结果未知码表)
- 级别:unit
- 里程碑:M3
- 前置:`RD-OKX`;`OkxSwap`;响应 `{"id":"x0000002a00000007","op":"batch-cancel-orders","code":"50013","msg":"Systems are busy","data":[],"inTime":"","outTime":""}`
- 步骤:
  1. `decode_report(…, BatchResp, …)`
- 期望:
  - 0 条 `OrderEvent`;`outcome_unknown = Some(ReqRef{slot: 42, gen: 7, op: Cancel})`;`raw_code = 50013`;`gw_dwell_us = 0`
- 实现:`crates/venue-okx/tests/codes.rs::okx_094_batch_whole_unknown`

### OKX-095 响应 id 不是本系统格式时仍按 clOrdId 产出事件

- 规则:[使用本组件](README.md#使用本组件)、[4. 回报](README.md#4-回报)
- 级别:unit
- 里程碑:M3
- 前置:同 OKX-090,但 `"id":"1512"`
- 步骤:
  1. `decode_report`
- 期望:
  - 一条 `OrderEvent{what: Ack, slot: 42, gen: 7, epoch: 3}`;`ReportOutcome.req = None`
- 实现:`crates/venue-okx/tests/reports.rs::okx_095_foreign_request_id`

### OKX-096 inTime/outTime 缺失时网关驻留时间为 0

- 规则:[4. 回报](README.md#4-回报)
- 级别:table
- 里程碑:M3
- 前置:同 OKX-090,每行改 `inTime`/`outTime`
- 实现:`crates/venue-okx/tests/reports.rs::okx_096_gw_dwell`

| 行 | `inTime`、`outTime` | 期望 |
|---|---|---|
| OKX-096.1 | `"1790000000140100"`、`"1790000000145300"` | `gw_dwell_us = 5200`;`Ack.ex_ts_ns = 1790000000145300000` |
| OKX-096.2 | `""`、`"1790000000145300"` | `gw_dwell_us = 0`;`ex_ts_ns = 1790000000145300000` |
| OKX-096.3 | 两者都缺 | `gw_dwell_us = 0`;`Ack.ex_ts_ns = 0` |

### OKX-097 REST 查单响应产出 QueryResult,订单不存在不定终态

- 规则:[下单:WS `/private`](README.md#下单ws-private)、[累计量与费用来源](README.md#累计量与费用来源)、[hft-venue-proto 契约 9、10](../hft-venue-proto/README.md#实现方必须遵守的契约)
- 级别:table
- 里程碑:M3
- 前置:`RD-OKX`;`OkxSwap`;响应 `{"code":<c>,"msg":"","data":[{"instId":"BTC-USDT-SWAP","ordId":"312269865356374016","clOrdId":"1000305020002a000000077d","state":<st>,"accFillSz":<acc>,"avgPx":<avg>,"fee":"-0.13","feeCcy":"USDT","sz":"5","px":"65000.1","uTime":"1790000001000"}]}`
- 实现:`crates/venue-okx/tests/reports.rs::okx_097_query_result`
- 前置补充:以 `decode_rest(rd, ctx, &RestOrigin{what: Order, req: Some(ReqRef{slot: 42, gen: 7, op: Query}), inst: 1501}, 200, body, &RX, &mut sink)` 解码;"状态"即 `OrderEvent.detail`

| 行 | c、st、acc、avg | 期望 |
|---|---|---|
| OKX-097.1 | `"0"`,`partially_filled`,`"3"`,`"65000.05"` | 一条 `OrderEvent{what: QueryResult, slot 42, gen 7, epoch 3, cum_filled 3000000, avg_px 6500005, exch_order_id 312269865356374016, fee 0, detail 2}`(PartiallyFilled) |
| OKX-097.2 | `"0"`,`filled`,`"5"`,`"65000.1"` | 状态 Filled(3),`cum_filled 5000000` |
| OKX-097.3 | `"0"`,`live`,`"0"`,`""` | 状态 New(1),`cum_filled 0`,`avg_px 0` |
| OKX-097.4 | `"0"`,`canceled`,`"0"`,`""` | 状态 Cancelled(4) |
| OKX-097.5 | `"0"`,`mmp_canceled`,`"2"`,`"65000"` | 状态 Cancelled(4),`cum_filled 2000000` |
| OKX-097.6 | `"51603"`,`data = []` | 一条 `QueryResult`,状态 NotFound(7),`slot/gen` 取自该查单请求(`slot 42, gen 7`),`cum_filled 0` |
| OKX-097.7 | `"0"`,`partially_filled`,`"3"`,`""` | `Err(DecodeErr::Shape)`(`cum_filled > 0` 而无均价) |
| OKX-097.8 | `"50001"`,`data = []` | 0 事件,`raw_code = 50001` |

### OKX-098 orders-pending 响应对本系统的每个挂单产出 QueryResult

- 规则:[下单:WS `/private`](README.md#下单ws-private)
- 级别:unit
- 里程碑:M3
- 前置:`RD-OKX`;`OkxSwap`;响应 `{"code":"0","msg":"","data":[<订单 A:clOrdId = CID(3, 42, 7),state live,accFillSz "0">,<订单 B:clOrdId = CID(3, 43, 1),state partially_filled,accFillSz "1",avgPx "65000">,<订单 C:clOrdId = "",state live>]}`
- 步骤:
  1. `decode_rest(…, &RestOrigin{what: Sweep, req: None, inst: 0}, 200, body, …)`
- 期望:
  - 两条 `QueryResult`:`{slot 42, gen 7, 状态 New, cum_filled 0}`、`{slot 43, gen 1, 状态 PartiallyFilled, cum_filled 1000000, avg_px 6500000}`;订单 C 不产出事件
- 实现:`crates/venue-okx/tests/reports.rs::okx_098_orders_pending`

### OKX-099 cancel-all-after 响应只回报码

- 规则:[下单:WS `/private`](README.md#下单ws-private)、[VenueCaps 默认值](README.md#venuecaps-默认值)
- 级别:table
- 里程碑:M3
- 前置:`RD-OKX`;`OkxSwap`
- 实现:`crates/venue-okx/tests/reports.rs::okx_099_caa_response`

| 行 | 响应 | 期望 |
|---|---|---|
| OKX-099.1 | `{"code":"0","msg":"","data":[{"triggerTime":"1790000030000","tag":"","ts":"1790000000000"}]}` | 0 事件,`raw_code = 0` |
| OKX-099.2 | `{"code":"50038","msg":"This feature is unavailable in demo trading","data":[]}` | 0 事件,`raw_code = 50038` |
| OKX-099.3 | `{"code":"50011","msg":"Rate limit reached","data":[]}` | 0 事件,`raw_code = 50011` |

### OKX-100 orders 推送的状态映射到 OrderEvent

- 规则:[4. 回报](README.md#4-回报)、[用户流事件表](README.md#用户流事件表)
- 级别:table
- 里程碑:M3
- 前置:`RD-OKX`;`OkxSwap`;`RX`;基准推送 `ORD` = `{"arg":{"channel":"orders","instType":"SWAP","uid":"acct-0001"},"data":[{"instType":"SWAP","instId":"BTC-USDT-SWAP","ordId":"312269865356374016","clOrdId":"1000305020002a000000077d","ordType":"limit","side":"buy","sz":"5","px":"65000.1","state":"partially_filled","fillSz":"3","fillPx":"65000.1","tradeId":"242589207","fillFee":"-0.0975","fillFeeCcy":"USDT","fillTime":"1790000000400","accFillSz":"3","avgPx":"65000.1","category":"normal","amendResult":"","reqId":"","uTime":"1790000000401"}]}`;每行只改所列字段,断言产出的 `OrderEvent.what` 序列(全部 `slot 42, gen 7, epoch 3`)
- 实现:`crates/venue-okx/tests/reports.rs::okx_100_orders_state_mapping`

| 行 | 改动 | 期望 `what` 序列 |
|---|---|---|
| OKX-100.1 | `state = live`,`fillSz = "0"`,`tradeId = ""`,`accFillSz = "0"`,`avgPx = ""` | `[Ack]` |
| OKX-100.2 | 不改 | `[PartialFill]` |
| OKX-100.3 | `state = filled`,`accFillSz = "5"` | `[Fill]` |
| OKX-100.4 | `state = canceled`,`fillSz = "0"` | `[CancelAck]` |
| OKX-100.5 | `state = canceled`,`fillSz = "0"`,`ordType = ioc` | `[Expired]` |
| OKX-100.6 | `state = canceled`,`fillSz = "0"`,`ordType = fok` | `[Expired]` |
| OKX-100.7 | `state = mmp_canceled`,`fillSz = "0"` | `[CancelAck]` |
| OKX-100.8 | `state = live`,`fillSz = "0"`,`amendResult = "0"`,`reqId = "r1"` | `[AmendAck]`(无 `Ack`) |
| OKX-100.9 | `state = live`,`fillSz = "0"`,`amendResult = "-1"` | `[AmendReject]` |
| OKX-100.10 | `state = canceled`,`fillSz = "0"`,`amendResult = "1"` | `[AmendReject, CancelAck]` |
| OKX-100.11 | `state = canceled`,`ordType = ioc`(`fillSz = "3"`) | `[PartialFill, Expired]` |
| OKX-100.12 | `state = partially_filled`,`fillSz = "0"`,`amendResult = ""` | `[]`(0 事件) |
| OKX-100.13 | `state = "unknown_state"`,`fillSz = "0"` | `Err(DecodeErr::Shape)` |

### OKX-101 本系统订单的成交字段逐项映射

- 规则:[4. 回报](README.md#4-回报)、[累计量与费用来源](README.md#累计量与费用来源)、[hft-venue-proto 契约 9、13、19](../hft-venue-proto/README.md#实现方必须遵守的契约)
- 级别:unit
- 里程碑:M3
- 前置:`RD-OKX`;`OkxSwap`;`RX`;OKX-100 的 `ORD`
- 步骤:
  1. `decode_report(rd, &mut gw, Orders, ORD, &RX, &mut sink)`
- 期望:
  - 恰好一条 `OrderEvent{what: PartialFill, venue: 5, account: 2, inst: 1501, epoch: 3, slot: 42, gen: 7, px: 65000.1(6500010), qty: 0.03(3000000), cum_filled: 0.03(3000000), avg_px: 6500010, fee: 0.0975(9750000), fee_asset: 1(USDT), exch_order_id: 312269865356374016, ex_ts_ns: 1790000000400000000, rx_tsc: 1000, reject_code: 0}`
  - 0 条 `ExternalFill`;`ReportOutcome{events: 1, fee_rule_deviation: false, skip: No}`
- 实现:`crates/venue-okx/tests/reports.rs::okx_101_fill_fields`

### OKX-102 fillFee 为正(返佣)时 fee 为负

- 规则:[累计量与费用来源](README.md#累计量与费用来源)、[hft-venue-proto 契约 19](../hft-venue-proto/README.md#实现方必须遵守的契约)
- 级别:unit
- 里程碑:M3
- 前置:OKX-100 的 `ORD`,`fillFee = "0.0130"`
- 步骤:
  1. `decode_report`
- 期望:
  - `PartialFill.fee = −0.013(−1300000)`,`fee_asset = 1`
- 实现:`crates/venue-okx/tests/reports.rs::okx_102_rebate_negative_fee`

### OKX-103 同一笔成交重复送达只产出一次

- 规则:[累计量与费用来源](README.md#累计量与费用来源)、[hft-venue-proto 契约 22](../hft-venue-proto/README.md#实现方必须遵守的契约)
- 级别:unit
- 里程碑:M3
- 前置:`RD-OKX`;`OkxSwap`;同一 `GwCtx`;OKX-100 的 `ORD`(`tradeId = "242589207"`)
- 步骤:
  1. `decode_report(ORD)`
  2. 再次 `decode_report(ORD)`(模拟两条连接重叠或重订阅回放)
- 期望:
  - 步骤 1:一条 `PartialFill`
  - 步骤 2:0 条事件,`ReportOutcome.events = 0`,返回 `Ok`
- 实现:`crates/venue-okx/tests/dedup.rs::okx_103_fill_dedup`

### OKX-104 两个 inst 的相同 tradeId 互不误杀

- 规则:[累计量与费用来源](README.md#累计量与费用来源)
- 级别:unit
- 里程碑:M3
- 前置:`RD-OKX`;`OkxSwap`;同一 `GwCtx`;推送 a = OKX-100 的 `ORD`;推送 b = 同 `tradeId = "242589207"` 但 `instId = "ETH-USDT-SWAP"`、`clOrdId = CID(3, 43, 1)`、`ordId = "7"`
- 步骤:
  1. `decode_report(a)`;`decode_report(b)`
- 期望:
  - 两条 `PartialFill`:`inst 1501` 与 `inst 1502`
- 实现:`crates/venue-okx/tests/dedup.rs::okx_104_same_tradeid_two_insts`

### OKX-105 tradeId 为空、0 或负时按订单累计量去重

- 规则:[累计量与费用来源](README.md#累计量与费用来源)、[hft-venue-proto 契约 22](../hft-venue-proto/README.md#实现方必须遵守的契约)
- 级别:table
- 里程碑:M3
- 前置:`RD-OKX`;`OkxSwap`;每行新建 `GwCtx`;推送为 OKX-100 的 `ORD` 的变体,按序喂入
- 实现:`crates/venue-okx/tests/dedup.rs::okx_105_dedup_without_tradeid`

| 行 | 喂入序列 | 期望 |
|---|---|---|
| OKX-105.1 | `tradeId = ""`、`accFillSz = "3"` 喂两次 | 1 条 `PartialFill` |
| OKX-105.2 | `tradeId = "0"`、`accFillSz = "3"` 喂两次 | 1 条 |
| OKX-105.3 | `tradeId = "-5"`、`accFillSz = "3"` 喂两次 | 1 条 |
| OKX-105.4 | `tradeId = ""` 先 `accFillSz = "3"`、再 `accFillSz = "4"`(`fillSz = "1"`) | 2 条,`cum_filled` 3000000 与 4000000 |
| OKX-105.5 | `tradeId = ""`、`accFillSz = "3"`,两条 `ordId` 不同(`"1"`、`"2"`) | 2 条 |

### OKX-106 扣费资产与 fee_asset_rule 不符时置偏差位且不改事件

- 规则:[4. 回报](README.md#4-回报)、[hft-venue-proto 契约 19](../hft-venue-proto/README.md#实现方必须遵守的契约)
- 级别:table
- 里程碑:M3
- 前置:`RD-OKX`;推送为 OKX-100 的 `ORD`(永续)或其现货变体(`instType = SPOT`,`instId = BTC-USDT`,`clOrdId` = `OKX_SPOT` 的 `CID(3, 42, 7)`,`fillSz = "0.002"`);每行断言 `ReportOutcome.fee_rule_deviation` 与事件的 `fee_asset`
- 实现:`crates/venue-okx/tests/reports.rs::okx_106_fee_rule_deviation`

| 行 | 产品线、生效 `fee_asset_rule`、side、`fillFeeCcy` | 期望 |
|---|---|---|
| OKX-106.1 | 永续,`Quote`,buy,`USDT` | `false`,`fee_asset 1` |
| OKX-106.2 | 永续,`Quote`,buy,`BTC` | `true`,`fee_asset 2`,`fee` 与报文一致 |
| OKX-106.3 | 现货,`ReceivedAsset`,buy,`BTC` | `false` |
| OKX-106.4 | 现货,`ReceivedAsset`,buy,`USDT` | `true` |
| OKX-106.5 | 现货,`ReceivedAsset`,sell,`USDT` | `false` |
| OKX-106.6 | 现货,refdata 覆盖为 `Quote`,buy,`USDT` | `false` |
| OKX-106.7 | 现货,refdata 覆盖为 `Quote`,buy,`BTC` | `true` |
| OKX-106.8 | 现货,`ReceivedAsset`,buy,`fillFeeCcy = "OKB"`(refdata 无此资产) | `Err(DecodeErr::Shape)` |

### OKX-107 现货成交数量即基础币数量

- 规则:[累计量与费用来源](README.md#累计量与费用来源)、[下单:WS `/private`](README.md#下单ws-private)
- 级别:unit
- 里程碑:M3
- 前置:`RD-OKX`;`OkxSpot`;OKX-106 的现货变体,`fillSz = "0.002"`,`accFillSz = "0.005"`,`fillFee = "-0.000002"`,`fillFeeCcy = "BTC"`
- 步骤:
  1. `decode_report`
- 期望:
  - `PartialFill{inst: 1601, qty: 200000, cum_filled: 500000, fee: 200, fee_asset: 2}`
- 实现:`crates/venue-okx/tests/reports.rs::okx_107_spot_fill_units`

### OKX-108 成交消息按外部成交识别顺序只走一条路径

- 规则:[外部成交识别](README.md#外部成交识别)、[hft-venue-proto 契约 18](../hft-venue-proto/README.md#实现方必须遵守的契约)
- 级别:table
- 里程碑:M3
- 前置:`RD-OKX`;`OkxSwap`;每行新建 `GwCtx`;推送为 OKX-100 的 `ORD`,只改 `category` 与 `clOrdId`
- 实现:`crates/venue-okx/tests/external.rs::okx_108_external_fill_order`
- 备注:依赖待核实:开发备忘第 2 条(`category` 推送原文)、第 3 条(网页/App 单的 `clOrdId`);核实前 ADL 与交割样本为按文档构造的合成样本

| 行 | `category`、`clOrdId` | 期望 |
|---|---|---|
| OKX-108.1 | `full_liquidation`,`""` | 1 条 `ExternalFill{cause: Liquidation}`,0 条 `OrderEvent` |
| OKX-108.2 | `partial_liquidation`,`""` | `ExternalFill{cause: Liquidation}` |
| OKX-108.3 | `adl`,`""` | `ExternalFill{cause: Adl}` |
| OKX-108.4 | `delivery`,`""` | `ExternalFill{cause: Delivery}` |
| OKX-108.5 | `adl`,`CID(3, 42, 7)` | `ExternalFill{cause: Adl}`,0 条 `OrderEvent`(标记优先于 client id) |
| OKX-108.6 | `normal`,`CID(3, 42, 7)` | 1 条 `OrderEvent{PartialFill}`,0 条 `ExternalFill` |
| OKX-108.7 | `normal`,`""` | `ExternalFill{cause: Unknown}` |
| OKX-108.8 | `normal`,`"web1690000000"` | `ExternalFill{cause: Unknown}` |
| OKX-108.9 | `normal`,`CID✗` | `ExternalFill{cause: Unknown}` |
| OKX-108.10 | `auto_conversion`,`""` | `ExternalFill{cause: Unknown}` |
| OKX-108.11 | `normal`,删除 `clOrdId` 键 | `ExternalFill{cause: Unknown}` |
| OKX-108.12 | `full_liquidation`,`""`,`fillSz = "0"`,`state = live` | 0 事件(不是成交消息) |
| OKX-108.13 | `normal`,`""`,`fillSz = "0"`,`state = canceled` | 0 事件(外来单的状态变化不产出) |

### OKX-109 ExternalFill 的字段与去重键字段

- 规则:[外部成交识别](README.md#外部成交识别)、[hft-venue-proto 契约 22](../hft-venue-proto/README.md#实现方必须遵守的契约)
- 级别:table
- 里程碑:M3
- 前置:`RD-OKX`;`OkxSwap`;OKX-100 的 `ORD`,`category = full_liquidation`,`clOrdId = ""`,`side = sell`,`fillSz = "2"`,`fillPx = "52000.5"`,`fillFee = "-1.04"`,`accFillSz = "2"`
- 实现:`crates/venue-okx/tests/external.rs::okx_109_external_fill_fields`

| 行 | `tradeId` | 期望 `ExternalFill` |
|---|---|---|
| OKX-109.1 | `"242589300"` | `{venue: 5, account: 2, side: Sell, cause: Liquidation, inst: 1501, qty: 0.02(2000000), px: 5200050, fee: 1.04(104000000), fee_asset: 1, exch_order_id: 312269865356374016, trade_id: 242589300, ex_ts_ns: 1790000000400000000, fill_seq: 0}` |
| OKX-109.2 | `""` | 同上但 `trade_id: 0`,`fill_seq: 2000000`(`accFillSz` 的 raw) |
| OKX-109.3 | `"-1"` | `trade_id: 0`,`fill_seq: 2000000` |

### OKX-110 外部成交重复送达只产出一次

- 规则:[外部成交识别](README.md#外部成交识别)、[hft-venue-proto 契约 22](../hft-venue-proto/README.md#实现方必须遵守的契约)
- 级别:unit
- 里程碑:M3
- 前置:同一 `GwCtx`;OKX-109.2 的推送(`tradeId = ""`)
- 步骤:
  1. 喂两次
- 期望:
  - 共 1 条 `ExternalFill`;第二次 `events = 0`
- 实现:`crates/venue-okx/tests/external.rs::okx_110_external_fill_dedup`

### OKX-111 强平罚金不并入 ExternalFill 的 fee

- 规则:[外部成交识别](README.md#外部成交识别)
- 级别:unit
- 里程碑:M3
- 前置:OKX-109.1 的推送,另在同一推送的 `data[0]` 加 `"liqPenalty":"-3.5"`
- 步骤:
  1. `decode_report`
- 期望:
  - `ExternalFill.fee = 104000000`(只取 `−fillFee`)
- 实现:`crates/venue-okx/tests/external.rs::okx_111_liq_penalty_excluded`

### OKX-113 positions 推送产出 PositionReport 并更新保证金条目

- 规则:[用户流事件表](README.md#用户流事件表)、[保证金数据换算](README.md#保证金数据换算)、[hft-venue-proto 契约 17](../hft-venue-proto/README.md#实现方必须遵守的契约)
- 级别:unit
- 里程碑:M3
- 前置:`RD-OKX`;`OkxSwap`;同一 `GwCtx` 已解码 OKX-114 的 `account` 推送;`positions` 推送 `data` = `[{"instType":"SWAP","instId":"BTC-USDT-SWAP","mgnMode":"cross","posSide":"net","pos":"-150","avgPx":"64000.5","upl":"12.5","liqPx":"71000.5","adl":"3","uTime":"1790000000600"},{"instType":"SWAP","instId":"ETH-USDT-SWAP","mgnMode":"cross","posSide":"net","pos":"20","avgPx":"3000.1","upl":"-1.2","liqPx":"","adl":"0","uTime":"1790000000600"}]`
- 步骤:
  1. `decode_report(…, Positions, …)`
- 期望:
  - 两条 `PositionReport`:`{venue 5, account 2, inst 1501, qty −1.5(−150000000), entry_px 6400050, upnl 12.5(12500000), liq_px 7100050, ex_ts_ns 1790000000600000000}`、`{inst 1502, qty 2(200000000), entry_px 300010, upnl −1200000, liq_px 0}`
  - 一份 `MarginReport`:`hdr{source: 1(Push), n: 2, margin_balance/maint_margin/avail_margin` 取 OKX-114 缓存的值`}`,条目 `{1501, adl_quantile 60, liq_px 7100050}`、`{1502, adl_quantile 0, liq_px 0}`
- 实现:`crates/venue-okx/tests/margin.rs::okx_113_positions_push`

### OKX-114 account 推送(合约模式)产出余额与完整保证金报告

- 规则:[用户流事件表](README.md#用户流事件表)、[保证金数据换算](README.md#保证金数据换算)
- 级别:unit
- 里程碑:M3
- 前置:`RD-OKX`;`OkxSwap`;新建 `GwCtx`;`account` 推送 `data[0]` = `{"adjEq":"","mmr":"","uTime":"1790000000500","details":[{"ccy":"USDT","eq":"10000","cashBal":"10050","frozenBal":"50","availEq":"8000.5","availBal":"8000","mmr":"250","mgnRatio":"40"},{"ccy":"BTC","eq":"0.01","cashBal":"0.01","frozenBal":"0","availEq":"0.01","mmr":"0","mgnRatio":""},{"ccy":"XYZ","eq":"5","cashBal":"5","frozenBal":"0"}]}`
- 步骤:
  1. `decode_report(…, Account, …)`
- 期望:
  - 两条 `BalanceReport`:`{asset 1, free 10000(1000000000000), locked 50(5000000000), ex_ts_ns 1790000000500000000}`、`{asset 2, free 0.01(1000000), locked 0}`;`XYZ` 无事件
  - 一份 `MarginReport{source: 1, n: 0, margin_balance: 10000(10000000000), maint_margin: 250(250000000), avail_margin: 8000.5(8000500000), margin_ratio: 0.025(25000000)}`
- 实现:`crates/venue-okx/tests/margin.rs::okx_114_account_push_contract_mode`

### OKX-115 account 推送(多币种保证金口径)按 adjEq 计

- 规则:[保证金数据换算](README.md#保证金数据换算)
- 级别:unit
- 里程碑:M3
- 前置:`RD-OKX`;`OkxSwap`;新建 `GwCtx`;`account` 推送 `data[0]` = `{"adjEq":"20000","mmr":"1000","availEq":"15000","mgnRatio":"20","uTime":"1790000000500","details":[{"ccy":"USDT","eq":"10000","cashBal":"10000","frozenBal":"0","availEq":"9000","mmr":"250"}]}`
- 步骤:
  1. `decode_report(…, Account, …)`
- 期望:
  - `MarginReport{margin_balance: 20000000000, maint_margin: 1000000000, avail_margin: 15000000000, margin_ratio: 0.05(50000000)}`(不取 `details[USDT]` 的值)
- 实现:`crates/venue-okx/tests/margin.rs::okx_115_account_push_multi_ccy`

### OKX-116 margin_ratio 由维持保证金与权益算出的边界

- 规则:[保证金数据换算](README.md#保证金数据换算)、[hft-venue-proto 契约 17](../hft-venue-proto/README.md#实现方必须遵守的契约)
- 级别:table
- 里程碑:M3
- 前置:OKX-114 的推送,每行只改 `details[USDT]` 的 `eq`、`mmr`、`mgnRatio`
- 实现:`crates/venue-okx/tests/margin.rs::okx_116_margin_ratio_edges`

| 行 | `eq`、`mmr`、`mgnRatio` | 期望 `margin_ratio` |
|---|---|---|
| OKX-116.1 | `"10000"`、`"250"`、`"40"` | 25000000 |
| OKX-116.2 | `"10000"`、`""`、`""` | 0 |
| OKX-116.3 | `"0"`、`"250"`、`"0"` | `Rate::MAX` |
| OKX-116.4 | `"-5"`、`"250"`、`""` | `Rate::MAX` |
| OKX-116.5 | `"500"`、`"1000"`、`"0.5"` | 2000000000(2.0,已过强平线) |
| OKX-116.6 | `"10000"`、`"250"`、`"10"`(与 eq/mmr 不一致的交易所比率) | 25000000(不采用 `mgnRatio`) |

### OKX-117 margin_ratio 与 i128 参考实现一致

- 规则:[保证金数据换算](README.md#保证金数据换算)
- 级别:prop
- 里程碑:M3
- 性质:对任意 `eq`、`mmr` 十进制串(合约模式)与 `adjEq`、`mmr`(多币种口径),`MarginReport.margin_ratio` 等于 i128 参考 `floor(mmr × 10⁹ / eq)`(`eq ≤ 0` → `Rate::MAX`;`mmr` 为空 → 0);不 panic
- 生成:`eq` ∈ [−10⁶, 10¹²],`mmr` ∈ [0, 10¹²],各 0–6 位小数;5% 概率为空串
- 参考:i128 十进制参考实现
- 例数:PR 10 000
- 实现:`crates/venue-okx/tests/prop_margin.rs::okx_117_margin_ratio_matches_ref`

### OKX-118 尚未收到 account 推送时保证金报告的账户级字段为 0

- 规则:[保证金数据换算](README.md#保证金数据换算)
- 级别:unit
- 里程碑:M3
- 前置:`RD-OKX`;`OkxSwap`;新建 `GwCtx`;OKX-113 的 `positions` 推送
- 步骤:
  1. `decode_report(…, Positions, …)`
- 期望:
  - `MarginReport{margin_balance: 0, maint_margin: 0, avail_margin: 0, margin_ratio: Rate::MAX, n: 2}`
- 实现:`crates/venue-okx/tests/margin.rs::okx_118_margin_before_account_push`

### OKX-119 可用保证金在 availEq 为空时取 availBal

- 规则:[保证金数据换算](README.md#保证金数据换算)
- 级别:unit
- 里程碑:M3
- 前置:OKX-114 的推送,`details[USDT].availEq = ""`,`availBal = "7999"`
- 步骤:
  1. `decode_report`
- 期望:
  - `MarginReport.avail_margin = 7999000000`
- 实现:`crates/venue-okx/tests/margin.rs::okx_119_avail_fallback`

### OKX-120 ADL 档按 20 倍归一化

- 规则:[保证金数据换算](README.md#保证金数据换算)
- 级别:table
- 里程碑:M3
- 前置:OKX-113 的 `positions` 推送,只改第一条的 `adl`
- 实现:`crates/venue-okx/tests/margin.rs::okx_120_adl_quantile`

| 行 | `adl` | 期望 |
|---|---|---|
| OKX-120.1 | `"0"` | `adl_quantile 0` |
| OKX-120.2 | `"1"` | 20 |
| OKX-120.3 | `"5"` | 100 |
| OKX-120.4 | `""` | 255 |
| OKX-120.5 | `"6"` | `Err(DecodeErr::Shape)`,0 事件 |

### OKX-121 现货账户的 account 推送只产出余额

- 规则:[用户流事件表](README.md#用户流事件表)、[保证金数据换算](README.md#保证金数据换算)
- 级别:unit
- 里程碑:M3
- 前置:`RD-OKX`;`OkxSpot`;OKX-114 的 `account` 推送
- 步骤:
  1. `decode_report(…, Account, …)`
- 期望:
  - 两条 `BalanceReport`(`venue 6`,同 OKX-114 的值);0 份 `MarginReport`
- 实现:`crates/venue-okx/tests/margin.rs::okx_121_spot_account_balance_only`

### OKX-122 资金费推送按 USDT 余额变化产出账户级 FundingSettle

- 规则:[用户流事件表](README.md#用户流事件表)、[hft-venue-proto 契约 14、22](../hft-venue-proto/README.md#实现方必须遵守的契约)
- 级别:unit
- 里程碑:M3
- 前置:`RD-OKX`;`OkxSwap`;同一 `GwCtx` 已解码 OKX-114 的 `account` 推送(USDT `cashBal = 10050`);资金费推送 `F1` = `{"arg":{"channel":"balance_and_position"},"data":[{"pTime":"1790006400050","eventType":"funding_fee","balData":[{"ccy":"USDT","cashBal":"10049.65","uTime":"1790006400050"}],"posData":[]}]}`
- 步骤:
  1. `decode_report(…, BalanceAndPosition, F1, …)`
- 期望:
  - 恰好一条 `FundingSettle{venue: 5, account: 2, inst: 0(INST_NONE), amount: −0.35(−350000), rate: 0, pos_qty: 0, settle_ts_ns: 1790006400050000000, synth: 0}`
- 实现:`crates/venue-okx/tests/funding.rs::okx_122_funding_fee_push`
- 备注:依赖待核实:开发备忘第 1 条(余额变化的取法、`pTime` 是否等于结算时刻)

### OKX-123 没有余额基线时资金费推送不产出 FundingSettle

- 规则:[用户流事件表](README.md#用户流事件表)
- 级别:unit
- 里程碑:M3
- 前置:新建 `GwCtx`(未收到任何 `account`/`balance_and_position` 推送);OKX-122 的 `F1`
- 步骤:
  1. `decode_report(F1)`
  2. 再喂 `F2` = `F1` 但 `pTime = "1790035200050"`、`cashBal = "10049.30"`
- 期望:
  - 步骤 1:0 事件,返回 `Ok`
  - 步骤 2:一条 `FundingSettle{amount: −350000, settle_ts_ns: 1790035200050000000}`(基线取自步骤 1)
- 实现:`crates/venue-okx/tests/funding.rs::okx_123_funding_without_baseline`

### OKX-124 资金费推送按 (pTime, cashBal) 去重

- 规则:[用户流事件表](README.md#用户流事件表)、[hft-venue-proto 契约 22](../hft-venue-proto/README.md#实现方必须遵守的契约)
- 级别:table
- 里程碑:M3
- 前置:同 OKX-122 的初始状态;每行新建该状态后按序喂入
- 实现:`crates/venue-okx/tests/funding.rs::okx_124_funding_dedup`

| 行 | 喂入 | 期望 |
|---|---|---|
| OKX-124.1 | `F1`、`F1` | 1 条 `FundingSettle` |
| OKX-124.2 | `F1`、`F1'`(同 `pTime`,`cashBal = "10049.40"`) | 2 条:`amount −350000`、`amount −250000` |
| OKX-124.3 | `F1`、再喂 OKX-114 的 `account` 推送、再 `F1` | 1 条(第二次 `F1` 被去重,不按新基线重算) |

### OKX-125 balance_and_position 的其他事件类型

- 规则:[用户流事件表](README.md#用户流事件表)、[保证金数据换算](README.md#保证金数据换算)
- 级别:table
- 里程碑:M3
- 前置:`RD-OKX`;`OkxSwap`;`F1` 只改 `eventType`
- 实现:`crates/venue-okx/tests/funding.rs::okx_125_balance_and_position_events`

| 行 | `eventType` | 期望 |
|---|---|---|
| OKX-125.1 | `liquidation` | 0 事件;`hint = MarginNow` |
| OKX-125.2 | `adl` | 0 事件;`hint = MarginNow` |
| OKX-125.3 | `filled` | 0 事件;`hint = None`;USDT 基线更新为 10049.65 |
| OKX-125.4 | `snapshot` | 0 事件;`hint = None` |
| OKX-125.5 | `"brand_new_type"` | 0 事件;`hint = None`;`skip = No` |

### OKX-126 liquidation-warning 推送只给立即查询提示

- 规则:[用户流事件表](README.md#用户流事件表)、[保证金数据换算](README.md#保证金数据换算)
- 级别:unit
- 里程碑:M3
- 前置:`RD-OKX`;`OkxSwap`;推送 `{"arg":{"channel":"liquidation-warning","instType":"SWAP"},"data":[{"instId":"BTC-USDT-SWAP","mgnMode":"cross","pos":"-150","liqPx":"71000.5","mgnRatio":"1.2"}]}`
- 步骤:
  1. `decode_report(…, LiquidationWarning, …)`
- 期望:
  - 0 事件;`ReportOutcome.hint = MarginNow`
- 实现:`crates/venue-okx/tests/margin.rs::okx_126_liquidation_warning`

### OKX-127 可忽略与未知的私有消息只计数,不是解码失败

- 规则:[用户流事件表](README.md#用户流事件表)、[1. 分派](README.md#1-分派)、[hft-venue-proto 契约 20](../hft-venue-proto/README.md#实现方必须遵守的契约)
- 级别:table
- 里程碑:M3
- 前置:`RD-OKX`;`OkxSwap`;行 4 的 refdata 在 `ignorable_user_events` 追加 `"foo-notice"`;每行先 `classify_report` 再 `decode_report`
- 实现:`crates/venue-okx/tests/reports.rs::okx_127_ignorable_and_unknown`

| 行 | 报文 | 期望 |
|---|---|---|
| OKX-127.1 | `{"event":"channel-conn-count","channel":"orders","connCount":"2","connId":"a1"}` | `Ok(ReportOutcome{skip: Listed, events: 0})` |
| OKX-127.2 | `{"event":"channel-conn-count-error","channel":"orders","connCount":"31","connId":"a1"}` | `skip: Listed`,0 事件 |
| OKX-127.3 | `{"event":"brand-new-event","connId":"a1"}` | `skip: Unknown`,0 事件 |
| OKX-127.4 | `{"event":"foo-notice","connId":"a1"}`(refdata 追加) | `classify_report = Unknown`;`decode_report` 得 `skip: Listed`,0 事件 |
| OKX-127.5 | `{"arg":{"channel":"fills"},"data":[{}]}` | `skip: Unknown`,0 事件 |
| OKX-127.6 | `{"event":"login","code":"0","msg":"","connId":"a1"}` | `skip: No`,0 事件 |
| OKX-127.7 | `{"event":"subscribe","arg":{"channel":"orders","instType":"SWAP"},"connId":"a1"}` | `skip: No`,0 事件 |
| OKX-127.8 | `{"event":"notice","code":"64008","msg":"…"}` | `skip: No`,0 事件,`ctl = ServerShutdown` |

### OKX-128 已知类别的推送内容解析失败返回 DecodeErr

- 规则:[hft-venue-proto 契约 20](../hft-venue-proto/README.md#实现方必须遵守的契约)、[行情频道 → 事件](README.md#行情频道--事件)
- 级别:table
- 里程碑:M3
- 前置:`RD-OKX`;`OkxSwap`;OKX-100 的 `ORD` 的变体
- 实现:`crates/venue-okx/tests/reports.rs::okx_128_known_class_bad_body`

| 行 | 改动 | 期望 |
|---|---|---|
| OKX-128.1 | `accFillSz = "abc"` | `Err(DecodeErr::Number)`,0 事件 |
| OKX-128.2 | 在 `"fillFee"` 处截断 | `Err(DecodeErr::Truncated)` |
| OKX-128.3 | `fillSz = "100000000000000"`(张数溢出) | `Err(DecodeErr::Overflow)` |
| OKX-128.4 | 删除 `ordId` | `Err(DecodeErr::Shape)` |
| OKX-128.5 | OKX-113 的推送 `pos = "x"` | `Err(DecodeErr::Number)` |

### OKX-129 累计成交量直接取 accFillSz,不按 fillSz 累加

- 规则:[累计量与费用来源](README.md#累计量与费用来源)、[hft-venue-proto 契约 9](../hft-venue-proto/README.md#实现方必须遵守的契约)
- 级别:unit
- 里程碑:M3
- 前置:新建 `GwCtx`;OKX-100 的 `ORD`,`fillSz = "1"`,`accFillSz = "4"`(之前的成交推送未送达)
- 步骤:
  1. `decode_report`
- 期望:
  - `PartialFill{qty: 1000000, cum_filled: 4000000}`
- 实现:`crates/venue-okx/tests/reports.rs::okx_129_cum_from_accfillsz`

### OKX-130 回报的 epoch 取自 clOrdId,venue 不判断是否当前

- 规则:[hft-venue-proto 契约 13](../hft-venue-proto/README.md#实现方必须遵守的契约)
- 级别:unit
- 里程碑:M3
- 前置:OKX-100 的 `ORD`,`clOrdId = CID(2, 42, 7)`
- 步骤:
  1. `decode_report`
- 期望:
  - `PartialFill{epoch: 2, slot: 42, gen: 7}`
- 实现:`crates/venue-okx/tests/reports.rs::okx_130_epoch_from_cid`

### OKX-140 Config 序列的请求表

- 规则:[账户配置核验](README.md#账户配置核验)、[hft-venue-proto 契约 16](../hft-venue-proto/README.md#实现方必须遵守的契约)
- 级别:table
- 里程碑:M3
- 前置:无参数;断言 `acct_queries(Config)` 的条目(顺序即下表)
- 实现:`crates/venue-okx/tests/acct.rs::okx_140_config_queries`

| 行 | 产品线 / 条目 | 期望 `AcctQuery` 与对应请求路径 |
|---|---|---|
| OKX-140.1 | `OkxSwap` 0 | `{kind: Config, step: 0, every: 1, push_fallback: false, weight: 1}`,`GET /api/v5/account/config` |
| OKX-140.2 | `OkxSwap` 1 | `{Config, 1, 1, false, 1}`,`GET /api/v5/account/positions?instType=SWAP` |
| OKX-140.3 | `OkxSwap` 2 | `{Config, 2, 1, false, 1}`,`GET /api/v5/account/leverage-info?instId=…&mgnMode=cross` |
| OKX-140.4 | `OkxSpot` | 恰一条:`{Config, 0, 1, false, 1}`,`GET /api/v5/account/config` |

### OKX-141 Margin 序列只有推送过期时的兜底请求

- 规则:[保证金数据换算](README.md#保证金数据换算)、[hft-venue-proto 契约 17](../hft-venue-proto/README.md#实现方必须遵守的契约)
- 级别:table
- 里程碑:M3
- 前置:无参数
- 实现:`crates/venue-okx/tests/acct.rs::okx_141_margin_queries`

| 行 | 产品线 / 条目 | 期望 |
|---|---|---|
| OKX-141.1 | `OkxSwap` 0 | `{kind: Margin, step: 0, every: 1, push_fallback: true, weight: 1}`,`GET /api/v5/account/balance` |
| OKX-141.2 | `OkxSwap` 1 | `{Margin, 1, 1, true, 1}`,`GET /api/v5/account/positions?instType=SWAP` |
| OKX-141.3 | `OkxSpot` | 空表 |

### OKX-142 账户查询请求按 OKX REST 规则签名

- 规则:[下单:WS `/private`](README.md#下单ws-private)、[账户配置核验](README.md#账户配置核验)
- 级别:unit
- 里程碑:M3
- 前置:`RD-OKX`;`OkxSwap`;`KEY`;`WC`
- 步骤:
  1. `acct_request(rd, &t, 2, Config 步 0, &WC, &mut out)`
- 期望:
  - `out{endpoint: 0, method: GET, path_query: "/api/v5/account/config", body: 空, redacted: true}`;头 `OK-ACCESS-KEY: k-test`、`OK-ACCESS-SIGN: EEfkmLggmlOw2pXCrkIBoSGYXbMIfDnbqnxGg7RGff4=`、`OK-ACCESS-TIMESTAMP: 2026-09-21T14:13:20.000Z`、`OK-ACCESS-PASSPHRASE: p-test`;无 `x-simulated-trading`
- 实现:`crates/venue-okx/tests/acct.rs::okx_142_rest_signing`

### OKX-143 杠杆查询按 ≤ 20 个 inst 一批

- 规则:[账户配置核验](README.md#账户配置核验)
- 级别:table
- 里程碑:M3
- 前置:`OkxSwap`;`KEY`;`WC`;经 `GoldenDriver` 跑完整 Config 序列,记录产出的全部请求
- 实现:`crates/venue-okx/tests/acct.rs::okx_143_leverage_batches`
- 备注:依赖待核实:开发备忘第 14 条(批量上限);静态请求表展开为多批的机制归 hft-venue-proto,本用例只断言请求内容

| 行 | refdata | 期望杠杆请求 |
|---|---|---|
| OKX-143.1 | `RD-OKX`(3 个可交易永续) | 恰 1 个:`/api/v5/account/leverage-info?instId=BTC-USDT-SWAP,ETH-USDT-SWAP,PEPE-USDT-SWAP&mgnMode=cross` |
| OKX-143.2 | `RD-OKX-25` | 恰 2 个:第一批按 InstId 升序 20 个(1501–1520),第二批 5 个(1521–1525) |
| OKX-143.3 | `RD-OKX` 但三个永续都为 `Delisted` | 0 个杠杆请求;报告 `n_lev = 0` |

### OKX-144 永续账户 Config 报告的不符位

- 规则:[账户配置核验](README.md#账户配置核验)、[VenueCaps 默认值](README.md#venuecaps-默认值)、[hft-venue-proto 契约 16、23](../hft-venue-proto/README.md#实现方必须遵守的契约)
- 级别:table
- 里程碑:M3
- 前置:`RD-OKX`;`OkxSwap`;经 `GoldenDriver` 喂 Config 序列响应后 `acct_finish`;基准响应:`account/config` 的 `data[0]` = `{"uid":"acct-0001","acctLv":"2","posMode":"net_mode","perm":"read_only,trade","feeType":"0","autoLoan":false,"enableSpotBorrow":false}`;`positions` 全部 `mgnMode = cross`;`leverage-info` 三个 inst 的 `lever` = 5、10、3;基准期望 `AccountConfigReport{position_mode: 2(OneWay), margin_mode: 2(Cross), multi_asset: 2(Off), mismatches: 0, uid_keys: 3(SESSION|REST), venue_uid: "acct-0001", n_lev: 3}`;每行只改所列项,写出与基准不同的字段
- 实现:`crates/venue-okx/tests/acct.rs::okx_144_swap_config_bits`
- 备注:依赖待核实:开发备忘第 5 条(端点字段与模拟盘可用性)

| 行 | 改动 | 期望 |
|---|---|---|
| OKX-144.1 | 无 | 基准期望 |
| OKX-144.2 | `acctLv = "1"` | `mismatches = ACCT_TYPE(16)` |
| OKX-144.3 | `acctLv = "4"` | `ACCT_TYPE`;`multi_asset = 3(On)` |
| OKX-144.4 | `acctLv = "3"` | `MULTI_ASSET(4)`;`multi_asset = 3` |
| OKX-144.5 | `acctLv = "3"`,refdata 覆盖 `required_multi_asset = Some(true)` | `mismatches = 0`;`multi_asset = 3` |
| OKX-144.6 | `acctLv = "2"`,refdata 覆盖 `required_multi_asset = Some(true)` | `MULTI_ASSET` |
| OKX-144.7 | `posMode = "long_short_mode"` | `POS_MODE(1)`;`position_mode = 3(Hedge)` |
| OKX-144.8 | `perm = "read_only,trade,withdraw"` | `KEY_PERM(64)` |
| OKX-144.9 | `perm = "read_only"` | `KEY_PERM` |
| OKX-144.10 | `uid = "acct-0002"` | `UID(128)`;`venue_uid = "acct-0002"` |
| OKX-144.11 | `acctLv = "3"`,`autoLoan = true`,覆盖 `required_multi_asset = Some(true)` | `AUTO_BORROW(32)` |
| OKX-144.12 | `acctLv = "2"`,`autoLoan = true` | `mismatches = 0`(只在 `acctLv = 3` 核对) |
| OKX-144.13 | `positions` 中 1502 的 `mgnMode = isolated` | `MARGIN_MODE(2)`;`margin_mode = 4(Mixed)` |
| OKX-144.14 | `positions` 中一个 refdata 外的 inst 为逐仓 | `mismatches = 0` |
| OKX-144.15 | `feeType = "1"` | `mismatches = 0`(永续不核对 `feeType`) |
| OKX-144.16 | `account/config` 响应 `{"code":"50001","data":[]}` | `UNREADABLE(256)` |
| OKX-144.17 | 删除 `acctLv` | `UNREADABLE` |
| OKX-144.18 | `positions` 响应 HTTP 500 | `UNREADABLE` |
| OKX-144.19 | `perm = "read_only,trade,withdraw"` 且 `posMode = "long_short_mode"` | `KEY_PERM | POS_MODE`(65) |
| OKX-144.20 | `uid` 缺失,账户 `uid_policy = "unverifiable"`、`venue_uid = ""` | `mismatches = 0`;`uid_keys = 0`;`venue_uid` 全 0 |
| OKX-144.21 | `uid` 缺失,`uid_policy = "verified"` | `UNREADABLE` |

### OKX-145 现货账户 Config 报告的不符位

- 规则:[账户配置核验](README.md#账户配置核验)、[VenueCaps 默认值](README.md#venuecaps-默认值)、[hft-venue-proto 契约 16](../hft-venue-proto/README.md#实现方必须遵守的契约)
- 级别:table
- 里程碑:M3
- 前置:`RD-OKX`;`OkxSpot`;基准响应同 OKX-144 但 `acctLv = "1"`;基准期望 `{position_mode: 1, margin_mode: 1, multi_asset: 1(都是 NotApplicable), mismatches: 0, uid_keys: 3, n_lev: 0}`
- 实现:`crates/venue-okx/tests/acct.rs::okx_145_spot_config_bits`
- 备注:依赖待核实:开发备忘第 14 条(`enableSpotBorrow` 在 `acctLv = 2` 下的语义)

| 行 | 改动 | 期望 |
|---|---|---|
| OKX-145.1 | 无 | 基准期望 |
| OKX-145.2 | `acctLv = "2"` | `mismatches = 0` |
| OKX-145.3 | `acctLv = "3"` | `ACCT_TYPE` |
| OKX-145.4 | `acctLv = "4"` | `ACCT_TYPE` |
| OKX-145.5 | `enableSpotBorrow = true` | `AUTO_BORROW` |
| OKX-145.6 | 删除 `enableSpotBorrow` | `UNREADABLE` |
| OKX-145.7 | `feeType = "1"`(规则 `ReceivedAsset`) | `FEE_RULE(8)` |
| OKX-145.8 | refdata 覆盖 `fee_asset_rule = Quote`,`feeType = "1"` | `mismatches = 0` |
| OKX-145.9 | refdata 覆盖 `fee_asset_rule = Quote`,`feeType = "0"` | `FEE_RULE` |
| OKX-145.10 | `posMode = "long_short_mode"` | `mismatches = 0`(现货不核对) |
| OKX-145.11 | `perm = "read_only,trade,withdraw"` | `KEY_PERM` |
| OKX-145.12 | `uid = "acct-0002"` | `UID` |
| OKX-145.13 | `acctLv = "2"`,`autoLoan = true` | `mismatches = 0` |

### OKX-146 杠杆条目逐 inst 给出,读不到的批写 0 并置 UNREADABLE

- 规则:[账户配置核验](README.md#账户配置核验)、[hft-venue-proto 契约 16](../hft-venue-proto/README.md#实现方必须遵守的契约)
- 级别:table
- 里程碑:M3
- 前置:`OkxSwap`;Config 序列其余响应同 OKX-144 基准
- 实现:`crates/venue-okx/tests/acct.rs::okx_146_leverage_items`

| 行 | refdata 与杠杆响应 | 期望 |
|---|---|---|
| OKX-146.1 | `RD-OKX`;`lever` 5、10、3 | `n_lev = 3`;`LeverageItem` 按 InstId 升序:`{1501, 5000000000}`、`{1502, 10000000000}`、`{1503, 3000000000}`;`mismatches = 0` |
| OKX-146.2 | `RD-OKX`;响应另含 refdata 外的 `DOGE-USDT-SWAP` | 同上(多出的跳过) |
| OKX-146.3 | `RD-OKX`;响应缺 1503 | 1503 的 `leverage = 0`;`UNREADABLE` |
| OKX-146.4 | `RD-OKX-25`;第二批响应 `{"code":"50001","data":[]}` | 1521–1525 共 5 条 `leverage = 0`;1501–1520 的值照常;`UNREADABLE`;`n_lev = 25` |

### OKX-147 同一把 key:不另查会话 key 的 UID

- 规则:[账户配置核验](README.md#账户配置核验)、[hft-venue-proto 契约 23](../hft-venue-proto/README.md#实现方必须遵守的契约)
- 级别:unit
- 里程碑:M3
- 前置:`RD-OKX`;`KEY`;两个产品线
- 步骤:
  1. `session_identity_request(cfg, cred, 1790000000000, &mut out)`
  2. `decode_identity(200, b"{}", &mut uid)`
- 期望:
  - 步骤 1 `Ok(false)`,`out` 未写入(`path_query` 为空)
  - 步骤 2 `Err(DecodeErr::UnknownClass)`
- 实现:`crates/venue-okx/tests/acct.rs::okx_147_single_key_identity`

### OKX-148 推送过期兜底的结果是独立完整报告,不并入推送缓存

- 规则:[保证金数据换算](README.md#保证金数据换算)、[hft-venue-proto 契约 27](../hft-venue-proto/README.md#实现方必须遵守的契约)
- 级别:unit
- 里程碑:M3
- 前置:`RD-OKX`;`OkxSwap`;`GwCtx` 已解码 OKX-114 的 `account` 推送(USDT `eq = 10000`);新建 `AcctCtx`;兜底响应:`account/balance` 的 `data[0]` 与 OKX-114 同形但 `details[USDT].eq = "9000"`、`mmr = "300"`;`account/positions` 与 OKX-113 的 `data` 相同
- 步骤:
  1. 对 Margin 步 0、1 依次 `acct_decode`,再 `acct_finish(Margin)`
  2. 再用同一 `GwCtx` 解码 OKX-113 的 `positions` 推送
- 期望:
  - 步骤 1:一份 `MarginReport{source: 2(Poll), margin_balance: 9000000000, maint_margin: 300000000, margin_ratio: 33333333, n: 2}` 与两条 `PositionReport`
  - 步骤 2:`MarginReport.margin_balance = 10000000000`(推送缓存未被兜底结果改写)
- 实现:`crates/venue-okx/tests/margin.rs::okx_148_push_fallback_isolated`

### OKX-149 现货的 Margin 序列不产出保证金报告

- 规则:[保证金数据换算](README.md#保证金数据换算)
- 级别:unit
- 里程碑:M3
- 前置:`RD-OKX`;`OkxSpot`;新建 `AcctCtx`、空 `AcctScratch`
- 步骤:
  1. `acct_finish(rd, &mut ctx, 2, Margin, &s, &RX, &mut sink)`
- 期望:
  - `Ok(())`;0 份 `MarginReport`、0 条 `PositionReport`
- 实现:`crates/venue-okx/tests/margin.rs::okx_149_spot_no_margin`

### OKX-170 rate_rules() 的官方默认限速

- 规则:[限速默认值](README.md#限速默认值)
- 级别:table
- 里程碑:M3
- 前置:无参数;两个产品线的 `rate_rules()` 相同
- 实现:`crates/venue-okx/tests/consts.rs::okx_170_rate_rules`

| 行 | 下标 | 期望 `RateRule{scope, bucket, window_ms, limit}` |
|---|---|---|
| OKX-170.1 | 0 | `{Inst, Orders, 2000, 60}` |
| OKX-170.2 | 1 | `{Inst, Orders, 2000, 300}` |
| OKX-170.3 | 2 | `{Account, Orders, 2000, 1000}` |
| OKX-170.4 | 3 | `{Ip, NewConns, 1000, 3}` |
| OKX-170.5 | 4 | `{Conn, WsInMsgs, 3600000, 480}` |
| OKX-170.6 | 5 | `{Uid, Weight, 1000, 1}`;`rate_rules().len() == 6` |

### OKX-171 核实前没有过期保护的定终态依据

- 规则:[VenueCaps 默认值](README.md#venuecaps-默认值)、[hft-venue-proto 契约 10](../hft-venue-proto/README.md#实现方必须遵守的契约)
- 级别:unit
- 里程碑:M3
- 前置:无参数
- 步骤:
  1. 两个产品线调用 `stale_guard()`
- 期望:
  - 都是 `StaleGuard::None`(请求照样带 `expTime`,见 OKX-078)
- 实现:`crates/venue-okx/tests/consts.rs::okx_171_stale_guard_none`
- 备注:依赖待核实:开发备忘第 16 条(核实为撮合入口前拒绝后改为 `ExpireAt{default_ms: 1500}`,本用例随之修改)

### OKX-172 dead_man() 是账户级倒计时且丢一次刷新不触发

- 规则:[VenueCaps 默认值](README.md#venuecaps-默认值)
- 级别:table
- 里程碑:M3
- 前置:无参数
- 实现:`crates/venue-okx/tests/deadman.rs::okx_172_dead_man_spec`

| 行 | 产品线 | 期望 |
|---|---|---|
| OKX-172.1 | `OkxSwap` | `CountdownAccount{min_ms: 10000, max_ms: 120000, timeout_ms: 30000, refresh_ms: 5000, via: Rest}` |
| OKX-172.2 | `OkxSpot` | 同上 |
| OKX-172.3 | 两者 | `timeout_ms ≥ max(min_ms, 2 × refresh_ms)`:30000 ≥ 10000;`timeout_ms ≤ max_ms` |

### OKX-173 dead_man_refresh 每次产出一个与挂单无关的 CAA 请求

- 规则:[VenueCaps 默认值](README.md#venuecaps-默认值)、[5. 心跳与会话](README.md#5-心跳与会话)
- 级别:unit
- 里程碑:M3
- 前置:`RD-OKX`;`OkxSwap`;`KEY`;`WC`
- 步骤:
  1. `dead_man_refresh(rd, &t, 30000, &[], &WC, &mut out)`
  2. `dead_man_refresh(rd, &t, 30000, &[1501, 1502], &WC, &mut out2)`
- 期望:
  - 步骤 1:`Ok`;`out` 恰一个 REST 请求 `{endpoint: 0, method: POST, path_query: "/api/v5/trade/cancel-all-after", body: {"timeOut":"30"}, redacted: true}`,头 `OK-ACCESS-SIGN: +L7u1XSaq+fsBSnTTT3Mf2JBf0N46qJJjO8+tCLoezo=`、`OK-ACCESS-TIMESTAMP: 2026-09-21T14:13:20.000Z`
  - 步骤 2:与步骤 1 逐字节相同(`insts` 不参与)
- 实现:`crates/venue-okx/tests/deadman.rs::okx_173_caa_request`

### OKX-174 CAA 的 timeOut 边界

- 规则:[VenueCaps 默认值](README.md#venuecaps-默认值)
- 级别:table
- 里程碑:M3
- 前置:同 OKX-173;每行以所列 `timeout_ms` 调用
- 实现:`crates/venue-okx/tests/deadman.rs::okx_174_timeout_bounds`

| 行 | `timeout_ms` | 期望 |
|---|---|---|
| OKX-174.1 | 9000 | `Err(EncodeErr::FieldOverflow)`,0 个请求 |
| OKX-174.2 | 10000 | `Ok`,`{"timeOut":"10"}` |
| OKX-174.3 | 11000 | `Ok`,`{"timeOut":"11"}` |
| OKX-174.4 | 119000 | `Ok`,`{"timeOut":"119"}` |
| OKX-174.5 | 120000 | `Ok`,`{"timeOut":"120"}` |
| OKX-174.6 | 121000 | `Err(EncodeErr::FieldOverflow)` |
| OKX-174.7 | 10500 | `Err(EncodeErr::FieldOverflow)`(不是整秒) |
| OKX-174.8 | 0 | `Err(EncodeErr::FieldOverflow)`(关闭死手不由刷新表达) |

### OKX-175 OkxSwap 的默认能力表

- 规则:[VenueCaps 默认值](README.md#venuecaps-默认值)、[hft-venue-proto 契约 15](../hft-venue-proto/README.md#实现方必须遵守的契约)
- 级别:table
- 里程碑:M3
- 前置:无参数;断言 `OkxSwap::default_caps()` 的每个字段
- 实现:`crates/venue-okx/tests/caps.rs::okx_175_default_caps_swap`

| 行 | 字段 | 期望 |
|---|---|---|
| OKX-175.1 | `venue`、`market` | `OKX_SWAP`、`LinearPerp` |
| OKX-175.2 | `fee_asset_rule` | `Quote` |
| OKX-175.3 | `reduce_only` | `true` |
| OKX-175.4 | `required_position_mode` | `OneWay` |
| OKX-175.5 | `required_margin_mode` | `Cross` |
| OKX-175.6 | `required_multi_asset` | `Some(false)` |
| OKX-175.7 | `required_spot_borrow` | `None` |
| OKX-175.8 | `funding_settle` | `BalanceOnlyPush` |
| OKX-175.9 | `margin_data` | `Push` |
| OKX-175.10 | `own_liquidation_marker`、`liq_marker_arg` | `OrderTypeField`、`"full_liquidation|partial_liquidation|adl|delivery"` |
| OKX-175.11 | `ignorable_user_events` | `["channel-conn-count", "channel-conn-count-error"]` |
| OKX-175.12 | `flags` | `AMEND | POST_ONLY | IOC | FOK | BATCH_CANCEL`(31) |
| OKX-175.13 | `dms`、`dms_min_ms`、`dms_max_ms`、`dms_timeout_ms`、`dms_refresh_ms` | `CountdownAccount`、10000、120000、30000、5000 |
| OKX-175.14 | `testnet` | `Demo` |

### OKX-176 OkxSpot 的默认能力表

- 规则:[VenueCaps 默认值](README.md#venuecaps-默认值)、[hft-venue-proto 契约 15、21](../hft-venue-proto/README.md#实现方必须遵守的契约)
- 级别:table
- 里程碑:M3
- 前置:无参数;断言 `OkxSpot::default_caps()` 的每个字段
- 实现:`crates/venue-okx/tests/caps.rs::okx_176_default_caps_spot`

| 行 | 字段 | 期望 |
|---|---|---|
| OKX-176.1 | `venue`、`market` | `OKX_SPOT`、`Spot` |
| OKX-176.2 | `fee_asset_rule` | `ReceivedAsset` |
| OKX-176.3 | `reduce_only` | `false` |
| OKX-176.4 | `required_position_mode`、`required_margin_mode` | `NotApplicable`、`NotApplicable` |
| OKX-176.5 | `required_multi_asset` | `None` |
| OKX-176.6 | `required_spot_borrow` | `Some(false)` |
| OKX-176.7 | `funding_settle`、`margin_data` | `NotApplicable`、`NotApplicable` |
| OKX-176.8 | `own_liquidation_marker`、`liq_marker_arg` | `NotIdentifiable`、`""` |
| OKX-176.9 | `ignorable_user_events` | `["channel-conn-count", "channel-conn-count-error"]` |
| OKX-176.10 | `flags` | 31 |
| OKX-176.11 | `dms` 及四个时长 | `CountdownAccount`、10000、120000、30000、5000 |
| OKX-176.12 | `testnet` | `Demo` |
| OKX-176.13 | `amend_qty_down_only`(两个产品线) | false |

### OKX-177 hft-venue-proto 的全部契约检查对两个产品线通过

- 规则:[hft-venue-proto 契约](../hft-venue-proto/README.md#实现方必须遵守的契约)、[6. 源文件规划](README.md#6-源文件规划)
- 级别:table
- 里程碑:M3
- 前置:`RD-OKX`;`tests/contract.rs` 以 testkit 的检查函数分别实例化 `OkxSwap`、`OkxSpot`;样本取自 `fixtures/okx-swap/`、`fixtures/okx-spot/`;每行对两个产品线各跑一次
- 实现:`crates/venue-okx/tests/contract.rs::okx_177_contract_checks`

| 行 | 检查 | 期望 |
|---|---|---|
| OKX-177.1 | `check_unknown_codes` | 通过(合成未登记码全部 `outcome_unknown`,`raw_code` 按契约 8) |
| OKX-177.2 | `check_cum` | 通过 |
| OKX-177.3 | `check_unknown_events` | 通过 |
| OKX-177.4 | `check_dedup` | 通过(含 `Funding{pTime, cashBal}` 两条推送;`OkxSpot` 跳过资金费子项) |
| OKX-177.5 | `check_external_fill` | 通过(`OkxSwap` 用 OKX-108.1–OKX-108.4 的样本;`OkxSpot` 用 `clOrdId` 为空的成交样本) |
| OKX-177.6 | `check_fee_asset` | 通过 |
| OKX-177.7 | `check_account_config` | 通过(每个位至少一个触发样本,来源 OKX-144、OKX-145) |
| OKX-177.8 | `check_identity` | 通过(`Ok(false)` 分支:`uid_keys` 覆盖 SESSION 与 REST) |
| OKX-177.9 | `check_default_caps` | 通过(含 `OkxSpot` 带 `REDUCE_ONLY` → `Unsupported`) |
| OKX-177.10 | `check_snapshot_isolation` | 通过(BBO 补帧响应) |
| OKX-177.11 | `check_topn` | 通过(`books5` 样本喂两次,第二次按 `Snapshot` 判重复) |
| OKX-177.12 | `check_acct_isolation` | 通过 |
| OKX-177.13 | `check_md_basics` | 通过 |
| OKX-177.14 | `check_hot_alloc` | 通过 |
| OKX-177.15 | `check_session` | 通过(哨兵密钥为 `KEY` 的替身) |
| OKX-177.16 | `check_cid` | 通过 |
| OKX-177.17 | `check_funding` | `OkxSwap` 按 `BalanceOnlyPush` 通过;`OkxSpot` 0 条 `FundingSettle` |
| OKX-177.18 | `check_margin` | `OkxSwap` 按 `Push`(含兜底请求)通过;`OkxSpot` 的 Margin 序列为空 |
| OKX-177.19 | `check_bbo_refresh` | 通过(批量 1,inst 取自传入列表) |
| OKX-177.20 | `check_detail` | 通过(`51400` 为 `GONE` 类) |
| OKX-177.21 | `check_rest` | 通过(`Query` 为 REST;`CANCEL_ALL_SWEEP = true`;会话维护 `Ok(false)`) |
| OKX-177.22 | `check_ctl` | 通过(`50011` → `RateLimited`,私有 `64008` → `ServerShutdown`) |

### OKX-178 只读查询的请求与解码通过 check_ro

- 规则:[只读查询](README.md#只读查询)、[hft-venue-proto 契约 31](../hft-venue-proto/README.md#实现方必须遵守的契约)
- 级别:table
- 里程碑:M5
- 前置:`RD-OKX`;以只读测试凭证(同 `KEY` 向量)构建的 `Templates`;`WC`;样本取 OKX-336–OKX-340
- 实现:`crates/venue-okx/tests/ro.rs::okx_178_read_only_queries`
- 备注:依赖待核实:开发备忘第 20 条

| 行 | 调用 | 期望 |
|---|---|---|
| OKX-178.1 | `OkxSwap::ro_request(OpenOrders)` | `GET /api/v5/trade/orders-pending?instType=SWAP`;带 `OK-ACCESS-*` 头;`redacted = true` |
| OKX-178.2 | `OkxSwap::ro_request(FundingFlow, since_ms = 1790000000000)` | `GET /api/v5/account/bills?type=8&…` |
| OKX-178.3 | `OkxSpot::ro_request(Positions)`、`OkxSpot::ro_request(FundingFlow)` | 都 `Err(EncodeErr::Unsupported(GwOp::Query))` |
| OKX-178.4 | `ro_decode(Fills)` 的成交行(`clOrdId` 为本系统格式) | `FillRow.cid = Some(CID(3, 42, 7))`;`fee` 取 `−fee` |
| OKX-178.5 | 两个产品线交给 `check_ro` | 通过 |

### OKX-190 心跳规格:25 s 无入站发文本 ping

- 规则:[5. 心跳与会话](README.md#5-心跳与会话)、[心跳与 BBO 探测](README.md#心跳与-bbo-探测)
- 级别:table
- 里程碑:M3
- 前置:无参数;两个产品线
- 实现:`crates/venue-okx/tests/session.rs::okx_190_heartbeat_spec`

| 行 | 调用 | 期望 |
|---|---|---|
| OKX-190.1 | `heartbeat(Md)` | `HeartbeatSpec{ping: AppText, interval_ms: 25000, unconditional: false, pong_timeout_ms: 10000, idle_timeout_ms: 30000}` |
| OKX-190.2 | `heartbeat(Order)` | 同上 |
| OKX-190.3 | `app_ping(Md, 1790000000000, &mut [0u8; 16])` | 返回 4,前 4 字节 `ping` |
| OKX-190.4 | `bbo_probe()` | `PingKind::WsPing` |

### OKX-200 订阅帧不带顶层 id,按路由给出频道

- 规则:[2. 订阅约束](README.md#2-订阅约束)、[行情频道 → 事件](README.md#行情频道--事件)
- 级别:table
- 里程碑:M3
- 前置:`RD-OKX`;每行一组 `SubSpec`,调用 `subscribe(subs, cfg, &mut out)`
- 实现:`crates/venue-okx/tests/subscribe.rs::okx_200_subscribe_frames`

| 行 | 产品线、`SubSpec` | 期望 `out` 的帧 |
|---|---|---|
| OKX-200.1 | Swap,`{1501, Bbo, 0, public-bbo}` | `[{"op":"subscribe","args":[{"channel":"bbo-tbt","instId":"BTC-USDT-SWAP"}]}]` |
| OKX-200.2 | Swap,`{1501, DepthTopN, 5, public-topn}` | `…{"channel":"books5","instId":"BTC-USDT-SWAP"}…` |
| OKX-200.3 | Swap,`{1501, DepthTopN, 20, public-topn}` | 同 OKX-200.2(只有 5 档) |
| OKX-200.4 | Swap,`{1501, Trade, 0, public-trades}` | `{"channel":"trades","instId":"BTC-USDT-SWAP"}` |
| OKX-200.5 | Swap,`{1501, Mark}`、`{1501, Funding}`、`{1501, Index}`(`public-mark`) | 一帧,`args` = `[{"channel":"mark-price","instId":"BTC-USDT-SWAP"},{"channel":"funding-rate","instId":"BTC-USDT-SWAP"},{"channel":"index-tickers","instId":"BTC-USDT"}]` |
| OKX-200.6 | Spot,`{1601, Bbo}` | `{"channel":"bbo-tbt","instId":"BTC-USDT"}` |
| OKX-200.7 | Swap,`{1501, Liquidation}` | `Err(ProtoErr::Unsupported)` |
| OKX-200.8 | Swap,`{1501, DepthIncr}` | `Err(ProtoErr::Unsupported)`(增量深度未启用) |
| OKX-200.9 | Spot,`{1601, Mark}` | `Err(ProtoErr::Unsupported)` |
| OKX-200.10 | Spot,`{1601, Funding}`、`{1601, Index}` | 各 `Err(ProtoErr::Unsupported)` |

### OKX-201 每帧至多 50 个 args

- 规则:[2. 订阅约束](README.md#2-订阅约束)
- 级别:table
- 里程碑:M3
- 前置:合成 refdata `RD-OKX-120`(`RD-OKX` 另加到共 120 个可交易永续);`OkxSwap`;`Bbo` 流
- 实现:`crates/venue-okx/tests/subscribe.rs::okx_201_subscribe_batching`

| 行 | `SubSpec` 数 | 期望帧数与每帧 args 数 |
|---|---|---|
| OKX-201.1 | 49 | 1 帧:49 |
| OKX-201.2 | 50 | 1 帧:50 |
| OKX-201.3 | 51 | 2 帧:50、1 |
| OKX-201.4 | 120 | 3 帧:50、50、20 |

### OKX-202 任意订阅集合的帧都不含 id 键

- 规则:[2. 订阅约束](README.md#2-订阅约束)
- 级别:prop
- 里程碑:M3
- 性质:对任意 `SubSpec` 集合(0–300 条)与 `unsub ∈ {false, true}`,`subscribe` 与 `private_subscribe` 写出的每一帧能被 `sonic-rs` 解析、顶层键集合 = `{op, args}`、`args.len() ≤ 50`
- 生成:inst 从 `RD-OKX` 两产品线的 inst 抽取;流从各产品线支持的 `StreamKind` 抽取
- 参考:断言;另把全部帧的 `args` 拼接后与输入集合一一对应
- 例数:PR 2 000
- 实现:`crates/venue-okx/tests/subscribe.rs::okx_202_no_id_in_sub_frames`

### OKX-203 永续账户的私有频道订阅

- 规则:[用户流事件表](README.md#用户流事件表)、[2. 订阅约束](README.md#2-订阅约束)
- 级别:table
- 里程碑:M3
- 前置:`RD-OKX`;`OkxSwap`;`acct = 2`
- 实现:`crates/venue-okx/tests/subscribe.rs::okx_203_private_subscribe_swap`

| 行 | `unsub` | 期望 `out` |
|---|---|---|
| OKX-203.1 | `false` | 一帧:`{"op":"subscribe","args":[{"channel":"orders","instType":"SWAP"},{"channel":"account"},{"channel":"balance_and_position"},{"channel":"positions","instType":"SWAP"},{"channel":"liquidation-warning","instType":"SWAP"}]}` |
| OKX-203.2 | `true` | 同上但 `"op":"unsubscribe"` |

### OKX-204 现货账户不订阅持仓与强平预警频道

- 规则:[用户流事件表](README.md#用户流事件表)
- 级别:unit
- 里程碑:M3
- 前置:`RD-OKX`;`OkxSpot`;`acct = 2`
- 步骤:
  1. `private_subscribe(cfg, 2, false, &mut out)`
- 期望:
  - 一帧:`{"op":"subscribe","args":[{"channel":"orders","instType":"SPOT"},{"channel":"account"},{"channel":"balance_and_position"}]}`;帧中不出现 `positions`、`liquidation-warning`
- 实现:`crates/venue-okx/tests/subscribe.rs::okx_204_private_subscribe_spot`

### OKX-210 模拟盘环境的 REST 请求带模拟盘请求头

- 规则:[测试网/模拟盘](README.md#测试网模拟盘)、[下单:WS `/private`](README.md#下单ws-private)
- 级别:table
- 里程碑:M3
- 前置:`RD-OKX-DEMO`(端点环境 `Demo`);`KEY`;`WC`;`OkxSwap`
- 实现:`crates/venue-okx/tests/demo.rs::okx_210_demo_header`

| 行 | 请求 | 期望 |
|---|---|---|
| OKX-210.1 | Config 步 0 | 头含 `x-simulated-trading: 1`;签名与 OKX-142 相同(该头不参与签名) |
| OKX-210.2 | CAA(`timeout_ms = 30000`) | 头含 `x-simulated-trading: 1` |
| OKX-210.3 | BBO 补帧(inst 1501) | 头含 `x-simulated-trading: 1` |
| OKX-210.4 | 同三项,但 refdata 为 `RD-OKX`(实盘) | 三个请求都不含 `x-simulated-trading` |

### OKX-211 instIdCode 取自 refdata,模拟盘用单独的码值

- 规则:[测试网/模拟盘](README.md#测试网模拟盘)、[下单:WS `/private`](README.md#下单ws-private)
- 级别:unit
- 里程碑:M3
- 前置:`RD-OKX-DEMO`;OKX-065 的 `c`
- 步骤:
  1. `build_templates` 后 `encode`
- 期望:
  - 帧中 `"instIdCode":99999`;不出现 10459
- 实现:`crates/venue-okx/tests/demo.rs::okx_211_demo_instidcode`

### OKX-212 crate 依赖不含 I/O 与运行时

- 规则:[依赖](README.md#依赖)
- 级别:unit
- 里程碑:M3
- 前置:CI 文档作业;`cargo tree -p venue-okx -e normal` 与 `-e dev`
- 步骤:
  1. 取 normal 依赖闭包与 dev 依赖列表
- 期望:
  - normal 闭包不含 `hft-net`、`hft-venue`、`tokio`、`serde_json`;`serde_json` 只出现在 dev 依赖;`cargo xtask deps` 对本 crate 无违规
- 实现:`crates/venue-okx/tests/deps.rs::okx_212_dependency_closure`

### OKX-213 模拟盘账户 UID 读不到时按 unverifiable 配置可通过加载

- 规则:[测试网/模拟盘](README.md#测试网模拟盘)、[账户配置核验](README.md#账户配置核验)
- 级别:unit
- 里程碑:M3
- 前置:模拟盘 refdata(已由 hft-cfg 按 TOML `uid_policy = "unverifiable"`、`uid_policy_signoff` 生成并加载):账户 2 `is_prod = false`、`UidPolicy::Unverifiable`、`venue_uid = ""`;`account/config` 响应无 `uid`,其余同 OKX-144 基准
- 步骤:
  1. 经 `GoldenDriver` 跑 `OkxSwap` 的 Config 序列
  2. 同一响应,但账户为 `UidPolicy::Verified`、`venue_uid = "acct-0001"`
- 期望:
  - 步骤 1:报告 `mismatches = 0`、`uid_keys = 0`、`venue_uid` 全 0
  - 步骤 2:`mismatches = UNREADABLE(256)`、`uid_keys = 3`
- 实现:`crates/venue-okx/tests/demo.rs::okx_213_demo_uid_policy`

### OKX-214 模拟盘连续 3 天冒烟(门槛 ⑥)

- 规则:[测试网/模拟盘](README.md#测试网模拟盘)
- 级别:testnet
- 里程碑:M6
- 门槛:⑥
- 环境:dev2 联调窗口内的 `prod` 角色实例,`feed_env = "testnet"`、`order_env = "testnet"`;refdata = OKX 模拟盘 refdata(hft-cfg 按模拟盘 instruments 生成);两个模拟盘账户(一个 `OkxSwap`、一个 `OkxSpot`,`is_prod = false`)
- 授权:`[需授权]`(模拟盘 API key 由负责人创建并放入 dev2 凭据目录)
- 步骤:
  1. 按 [testing §8 门槛 ⑥](../../docs/testing.md#8-上线门槛) 连续运行 3 天:每个产品线下单、撤单、改单、成交、断连重连、BBO 与 `books5` 订阅、Config 核验、CAA 刷新、kill-switch 撤单、reconciler 对账
  2. 每日汇总 raw journal 中的解码错误、`skip = Unknown` 计数与 `ConnState{Degraded}`
- 通过标准:3 天内解码错误 = 0;每个 `skip = Unknown` 的事件名都有结论(加入可忽略名单或开 issue);无未解释的 `Degraded`/`Down`;模拟盘不支持的接口(如 `50038`)逐项记入门槛记录
- 证据:`ops-log/<UTC 日期>-OKX-214.md`
- 实现:`tests/host/OKX-214.sh`

### OKX-215 私有频道双订阅是否都收到推送(M3 之前核实)

- 规则:[测试网/模拟盘](README.md#测试网模拟盘)、[用户流事件表](README.md#用户流事件表)
- 级别:testnet
- 里程碑:M3
- 环境:dev2;OKX 模拟盘账户一个;两条登录后的 `/private` 连接
- 授权:`[需授权]`(模拟盘 API key)
- 步骤:
  1. 两条连接同时 `private_subscribe`;在模拟盘下一笔 `BTC-USDT-SWAP` 小单并撤单
  2. 比对两路 raw 中 `orders`、`account`、`positions`、`balance_and_position` 的推送
- 通过标准:得到"两路都收到 / 只有一路收到"的明确结论,并写回 README 开发备忘第 12 条与 [hft-venue §4b](../hft-venue/README.md#4b-私有频道归属与热备连接) 的切换方式
- 证据:`ops-log/<UTC 日期>-OKX-215.md`(附两路 raw journal 片段的指针与结论)
- 实现:`tests/host/OKX-215.sh`

### OKX-216 模拟盘对 core 频道与账户接口的覆盖

- 规则:[测试网/模拟盘](README.md#测试网模拟盘)、[分档快照流](README.md#分档快照流)
- 级别:testnet
- 里程碑:M3
- 环境:dev2;OKX 模拟盘 `wspap` 端点与 `x-simulated-trading: 1` 的 REST
- 授权:`[需授权]`(模拟盘 API key)
- 步骤:
  1. 订阅 `bbo-tbt`、`books5`、`trades`、`mark-price`、`funding-rate`、`index-tickers` 各 10 min,记录 `books5` 节拍、`seqId` 是否单调、订阅后首帧时延
  2. 调用 `account/config`、`account/leverage-info`、`account/positions`、`account/balance`、`cancel-all-after`,记录可用性与 `uid` 是否返回
- 通过标准:开发备忘第 5、15、17、18 条各有结论并写回 README 对应表;`books5` 的 `seq_rule` 与 OKX-020 一致或同一改动修改 OKX-020
- 证据:`ops-log/<UTC 日期>-OKX-216.md`
- 实现:`tests/host/OKX-216.sh`

### OKX-300 金样:bbo-tbt

- 规则:[行情频道 → 事件](README.md#行情频道--事件)、[心跳与 BBO 探测](README.md#心跳与-bbo-探测)
- 级别:golden
- 里程碑:M3
- 输入:`fixtures/okx-swap/bbo-tbt/*.in` 与 `fixtures/okx-spot/bbo-tbt/*.in`;case:`normal`、`large_face_value_pepe`(`PEPE-USDT-SWAP`,`qty_unit_shift = 7`)、`one_side_empty`、`same_seqid_twice`、`price_scientific_notation`(合成)、`missing_seqId`(合成);来源:现有采集系统 raw WAL(`cargo xtask fixtures extract --wal … --channel bbo-tbt`),合成项 `source = "synthetic"`
- 期望文件:同目录 `<case>.expected.jsonl`(`Bbo` 逐字段;`missing_seqId` 期望 `{"error":"Shape"}`)
- 夹具:`fixtures/refdata/okx-test-1.blob`
- 实现:`crates/venue-okx/tests/golden.rs::okx_300_golden_bbo_tbt`

### OKX-301 金样:books5

- 规则:[分档快照流](README.md#分档快照流)、[行情频道 → 事件](README.md#行情频道--事件)
- 级别:golden
- 里程碑:M3
- 输入:`fixtures/okx-swap/books5/*.in`、`fixtures/okx-spot/books5/*.in`;case:`normal`、`duplicate_seqId`、`duplicate_ts`、`fewer_than_5_levels`、`zero_qty_level`、`ctval_0_01`;来源:现有采集系统 raw WAL;`duplicate_*` 为连续两条原文
- 期望文件:同目录 `<case>.expected.jsonl`(每条一条 `L2Snapshot{origin = TopN}`,按 `feed.snapshot_part_levels` 切片后的各片)
- 夹具:`fixtures/refdata/okx-test-1.blob`
- 实现:`crates/venue-okx/tests/golden.rs::okx_301_golden_books5`
- 备注:依赖待核实:开发备忘第 18 条(`seqId` 是否单调;核实为无 `seqId` 时期望改为 `snap_id = 0` 并改 OKX-020)

### OKX-302 金样:trades

- 规则:[行情频道 → 事件](README.md#行情频道--事件)
- 级别:golden
- 里程碑:M3
- 输入:`fixtures/okx-swap/trades/*.in`、`fixtures/okx-spot/trades/*.in`;case:`taker_buy`、`taker_sell`、`ctval_0_1_eth`、`pepe_large_face_value`、`empty_tradeId`(合成);来源:现有采集系统 raw WAL
- 期望文件:同目录 `<case>.expected.jsonl`
- 实现:`crates/venue-okx/tests/golden.rs::okx_302_golden_trades`

### OKX-303 金样:mark-price

- 规则:[行情频道 → 事件](README.md#行情频道--事件)
- 级别:golden
- 里程碑:M3
- 输入:`fixtures/okx-swap/mark-price/*.in`;case:`normal`、`bad_markPx`(合成);来源:现有采集系统 raw WAL
- 期望文件:同目录 `<case>.expected.jsonl`
- 实现:`crates/venue-okx/tests/golden.rs::okx_303_golden_mark_price`

### OKX-304 金样:funding-rate

- 规则:[行情频道 → 事件](README.md#行情频道--事件)
- 级别:golden
- 里程碑:M3
- 输入:`fixtures/okx-swap/funding-rate/*.in`;case:`interval_8h`、`interval_change_8h_to_4h`、`next_time_empty`;来源:现有采集系统 raw WAL;`interval_change_*` 待采集(方法 A:在 `okx_docs.txt` 与采集系统历史中找周期变化的合约,取变化前后各一条)
- 期望文件:同目录 `<case>.expected.jsonl`(`Funding.interval_s` 逐条)
- 实现:`crates/venue-okx/tests/golden.rs::okx_304_golden_funding_rate`
- 备注:依赖待核实:开发备忘第 8 条

### OKX-305 金样:index-tickers

- 规则:[行情频道 → 事件](README.md#行情频道--事件)
- 级别:golden
- 里程碑:M3
- 输入:`fixtures/okx-swap/index-tickers/*.in`;case:`normal`、`index_without_swap_in_refdata`;来源:现有采集系统 raw WAL
- 期望文件:同目录 `<case>.expected.jsonl`(第二个 case 期望为空)
- 实现:`crates/venue-okx/tests/golden.rs::okx_305_golden_index_tickers`

### OKX-306 金样:BBO 补帧 REST(books sz=1)

- 规则:[心跳与 BBO 探测](README.md#心跳与-bbo-探测)
- 级别:golden
- 里程碑:M3
- 输入:`fixtures/okx-swap/rest.books/*.in`、`fixtures/okx-spot/rest.books/*.in`;case:`normal`、`unknown_instrument_51001`、`empty_data`;来源:待采集(方法 C:模拟盘与实盘公共 REST 各取一条,公共接口无需 key)
- 期望文件:同目录 `<case>.expected.jsonl`
- 实现:`crates/venue-okx/tests/golden.rs::okx_306_golden_rest_books`

### OKX-307 金样:login 回执

- 规则:[下单:WS `/private`](README.md#下单ws-private)
- 级别:golden
- 里程碑:M3
- 输入:`fixtures/okx-swap/login/*.in`;case:`ok`、`failed_60009`、`bad_timestamp_60004`;只存服务端回执(登录帧含签名,永不入库);来源:现有采集系统私有流 raw(已清洗 `connId`)
- 期望文件:同目录 `<case>.expected.jsonl`(`login_result` 的渲染:`{"login":"Ok"}`、`{"login":"Rejected","code":60009}`)
- 实现:`crates/venue-okx/tests/golden.rs::okx_307_golden_login_result`

### OKX-308 金样:订阅回执与订阅被拒

- 规则:[2. 订阅约束](README.md#2-订阅约束)
- 级别:golden
- 里程碑:M3
- 输入:`fixtures/okx-swap/subscribe/*.in`;case:`ack`、`unsubscribe_ack`、`error_60033_id_param`、`error_64003_fee_tier`;来源:现有采集系统 raw(`60033` 已在生产出现),`64003` 待采集(方法 C:模拟盘低等级账户订阅 `books-l2-tbt`)
- 期望文件:同目录 `<case>.expected.jsonl`(`ControlHint::SubAck{ok, code}`)
- 实现:`crates/venue-okx/tests/golden.rs::okx_308_golden_subscribe_ack`

### OKX-309 金样:64008 服务升级预告

- 规则:[用户流事件表](README.md#用户流事件表)、[1. 分派](README.md#1-分派)
- 级别:golden
- 里程碑:M3
- 输入:`fixtures/okx-swap/notice/64008.in`(公共与私有连接各一条);来源:`exchange-doc`(官方文档示例),线上第一次出现时补真实样本
- 期望文件:`fixtures/okx-swap/notice/64008.expected.jsonl`(公共连接 `ServerShutdown`;私有连接 0 事件)
- 实现:`crates/venue-okx/tests/golden.rs::okx_309_golden_notice_64008`

### OKX-310 金样:pong

- 规则:[5. 心跳与会话](README.md#5-心跳与会话)
- 级别:golden
- 里程碑:M3
- 输入:`fixtures/okx-swap/pong/pong.in`;来源:现有采集系统 raw
- 期望文件:`fixtures/okx-swap/pong/pong.expected.jsonl`(`ControlHint::Pong`)
- 实现:`crates/venue-okx/tests/golden.rs::okx_310_golden_pong`

### OKX-311 金样:order 响应(成功与每个登记码)

- 规则:[结果未知码表](README.md#结果未知码表)、[4. 回报](README.md#4-回报)
- 级别:golden
- 里程碑:M3
- 输入:`fixtures/okx-swap/order/*.in`、`fixtures/okx-spot/order/*.in`;case:`ok`、`50001`、`50004`、`50013`、`50026`、`50011`、`50036`、`50037`、`50017`–`50022`、`50027`、`50029`、`51006`、`51008`、`51008_subcode`、`unregistered_59999`、`spot_cash_ok`;来源:模拟盘 raw(方法 C);模拟盘无法制造的码(`50001`、`50004`、`50013`、`50026`、`50017`–`50022`)用 `exchange-doc`
- 期望文件:同目录 `<case>.expected.jsonl`(结果未知与未登记码期望 `OutcomeUnknown`,不是 `Reject`)
- 实现:`crates/venue-okx/tests/golden.rs::okx_311_golden_order_resp`
- 备注:依赖待核实:开发备忘第 10 条(其余 `51xxx` 逐码登记时追加 case)

### OKX-312 金样:batch-orders 响应

- 规则:[结果未知码表](README.md#结果未知码表)
- 级别:golden
- 里程碑:M3
- 输入:`fixtures/okx-swap/batch-orders/*.in`;case:`all_ok`、`code_1_all_failed`、`code_2_partial`、`whole_unknown_50013`;来源:模拟盘 raw(方法 C),`whole_unknown_50013` 为 `synthetic`
- 期望文件:同目录 `<case>.expected.jsonl`
- 实现:`crates/venue-okx/tests/golden.rs::okx_312_golden_batch_orders`

### OKX-313 金样:amend-order 与 batch-amend-orders 响应

- 规则:[结果未知码表](README.md#结果未知码表)
- 级别:golden
- 里程碑:M3
- 输入:`fixtures/okx-swap/amend-order/*.in`;case:`ok`、`reject_51008`、`unknown_50001`、`batch_partial`;来源:模拟盘 raw(方法 C)
- 期望文件:同目录 `<case>.expected.jsonl`
- 实现:`crates/venue-okx/tests/golden.rs::okx_313_golden_amend_resp`

### OKX-314 金样:cancel-order 响应

- 规则:[结果未知码表](README.md#结果未知码表)
- 级别:golden
- 里程碑:M3
- 输入:`fixtures/okx-swap/cancel-order/*.in`、`fixtures/okx-spot/cancel-order/*.in`;case:`ok`、`already_filled_51400`、`unknown_50004`;来源:模拟盘 raw(方法 C:对已成交的单撤单得 `51400`)
- 期望文件:同目录 `<case>.expected.jsonl`
- 实现:`crates/venue-okx/tests/golden.rs::okx_314_golden_cancel_resp`

### OKX-315 金样:batch-cancel-orders 响应

- 规则:[结果未知码表](README.md#结果未知码表)、[下单:WS `/private`](README.md#下单ws-private)
- 级别:golden
- 里程碑:M3
- 输入:`fixtures/okx-swap/batch-cancel-orders/*.in`;case:`all_ok`、`partial_51400`;来源:模拟盘 raw(方法 C)
- 期望文件:同目录 `<case>.expected.jsonl`
- 实现:`crates/venue-okx/tests/golden.rs::okx_315_golden_batch_cancel`

### OKX-316 金样:REST 查单

- 规则:[下单:WS `/private`](README.md#下单ws-private)、[累计量与费用来源](README.md#累计量与费用来源)
- 级别:golden
- 里程碑:M3
- 输入:`fixtures/okx-swap/rest.trade_order/*.in`、`fixtures/okx-spot/rest.trade_order/*.in`;case:`live`、`partially_filled`、`filled`、`canceled`、`not_found_51603`;来源:模拟盘 raw(方法 C)
- 期望文件:同目录 `<case>.expected.jsonl`(`QueryResult` 带 `cum_filled` 与 `avg_px`;`51603` 期望 NotFound)
- 实现:`crates/venue-okx/tests/golden.rs::okx_316_golden_rest_query`

### OKX-317 金样:orders-pending

- 规则:[下单:WS `/private`](README.md#下单ws-private)
- 级别:golden
- 里程碑:M3
- 输入:`fixtures/okx-swap/rest.orders_pending/*.in`;case:`mixed_own_and_foreign`、`empty`;来源:模拟盘 raw(方法 C:挂两笔本系统单与一笔网页单)
- 期望文件:同目录 `<case>.expected.jsonl`
- 实现:`crates/venue-okx/tests/golden.rs::okx_317_golden_orders_pending`

### OKX-318 金样:cancel-all-after 响应

- 规则:[VenueCaps 默认值](README.md#venuecaps-默认值)
- 级别:golden
- 里程碑:M3
- 输入:`fixtures/okx-swap/rest.cancel_all_after/*.in`;case:`ok`、`demo_unavailable_50038`;来源:待采集(方法 C:模拟盘调用一次;开发备忘第 5 条)
- 期望文件:同目录 `<case>.expected.jsonl`(0 事件;`raw_code`)
- 实现:`crates/venue-okx/tests/golden.rs::okx_318_golden_caa`

### OKX-319 金样:orders 推送全部状态

- 规则:[4. 回报](README.md#4-回报)、[累计量与费用来源](README.md#累计量与费用来源)
- 级别:golden
- 里程碑:M3
- 输入:`fixtures/okx-swap/orders/*.in`、`fixtures/okx-spot/orders/*.in`;case:`live`、`post_only_live`、`partially_filled`、`filled`、`canceled`、`mmp_canceled`、`ioc_expired`、`fok_expired`、`amend_ok`、`amend_failed`、`amend_auto_cancel`、`rebate_positive_fillFee`、`spot_fee_in_base`、`spot_fee_feeType_1`;来源:模拟盘 raw(方法 C)
- 期望文件:同目录 `<case>.expected.jsonl`
- 实现:`crates/venue-okx/tests/golden.rs::okx_319_golden_orders_states`

### OKX-320 金样:orders 推送的强平、ADL 与交割

- 规则:[外部成交识别](README.md#外部成交识别)
- 级别:golden
- 里程碑:M3
- 输入:`fixtures/okx-swap/orders/*.in`;case:`full_liquidation`、`partial_liquidation`、`adl`、`delivery`、`liquidation_empty_tradeId`、`same_liquidation_twice`;来源:强平待采集(方法 C:模拟盘高杠杆小仓制造强平),ADL 与交割按官方文档构造(`source = "synthetic"`,`meta.toml` 标"合成",正式实盘出现后补真实样本)
- 期望文件:同目录 `<case>.expected.jsonl`(`ExternalFill` 带 `cause`、`trade_id`/`fill_seq`)
- 实现:`crates/venue-okx/tests/golden.rs::okx_320_golden_liquidation_adl`
- 备注:依赖待核实:开发备忘第 2、15 条

### OKX-321 金样:网页单与未知来源成交

- 规则:[外部成交识别](README.md#外部成交识别)
- 级别:golden
- 里程碑:M3
- 输入:`fixtures/okx-swap/orders/web_order_fill.in`、`fixtures/okx-spot/orders/web_order_fill.in`;来源:待采集(方法 C:模拟盘网页端下单成交)
- 期望文件:同目录 `web_order_fill.expected.jsonl`(`ExternalFill{cause: Unknown}`)
- 实现:`crates/venue-okx/tests/golden.rs::okx_321_golden_web_order`
- 备注:依赖待核实:开发备忘第 3 条(若能识别人工来源改为 `Manual` 并新建 case)

### OKX-322 金样:数量换算(ctVal 平移)

- 规则:[下单:WS `/private`](README.md#下单ws-private)、[累计量与费用来源](README.md#累计量与费用来源)
- 级别:golden
- 里程碑:M3
- 输入:`fixtures/okx-swap/orders/qty_*.in` 与 `fixtures/okx-swap/encode/qty_*.in`;case:`qty_btc_ctval_0_01`、`qty_eth_ctval_0_1`、`qty_pepe_ctval_1e7`;每个 case 一条下单 `GwCmd`(编码)与一条成交推送(解码);来源:模拟盘 raw(方法 C)
- 期望文件:同目录 `<case>.expected.jsonl`(下单帧 `sz` 为张数;成交事件 `qty`/`cum_filled` 为资产单位)
- 实现:`crates/venue-okx/tests/golden.rs::okx_322_golden_qty_conversion`

### OKX-323 金样:positions 推送

- 规则:[保证金数据换算](README.md#保证金数据换算)、[用户流事件表](README.md#用户流事件表)
- 级别:golden
- 里程碑:M3
- 输入:`fixtures/okx-swap/positions/*.in`;case:`two_positions`、`liqPx_empty`、`adl_levels_0_to_5`、`isolated_position`;来源:模拟盘 raw(方法 C;开发备忘第 19 条同时统计体积)
- 期望文件:同目录 `<case>.expected.jsonl`(`PositionReport` + `MarginReport`)
- 实现:`crates/venue-okx/tests/golden.rs::okx_323_golden_positions`

### OKX-324 金样:account 推送(合约模式)

- 规则:[保证金数据换算](README.md#保证金数据换算)
- 级别:golden
- 里程碑:M3
- 输入:`fixtures/okx-swap/account/acctlv2_*.in`、`fixtures/okx-spot/account/*.in`;case:`acctlv2_normal`、`acctlv2_no_cross_position`、`acctlv2_20_currencies`;来源:模拟盘 raw(方法 C)
- 期望文件:同目录 `<case>.expected.jsonl`
- 实现:`crates/venue-okx/tests/golden.rs::okx_324_golden_account_lv2`

### OKX-325 金样:account 推送(多币种保证金)

- 规则:[保证金数据换算](README.md#保证金数据换算)
- 级别:golden
- 里程碑:M3
- 输入:`fixtures/okx-swap/account/acctlv3_*.in`;case:`acctlv3_normal`;来源:待采集(方法 C:模拟盘切换为多币种保证金模式后抓一条)
- 期望文件:同目录 `<case>.expected.jsonl`
- 实现:`crates/venue-okx/tests/golden.rs::okx_325_golden_account_lv3`

### OKX-326 金样:balance_and_position 资金费

- 规则:[用户流事件表](README.md#用户流事件表)
- 级别:golden
- 里程碑:M3
- 输入:`fixtures/okx-swap/balance_and_position/funding_*.in`;case:`funding_fee_after_account`、`funding_fee_same_push_twice`、`funding_fee_two_disc`、`funding_fee_no_baseline`;来源:待采集(方法 C:模拟盘持仓跨过一次结算;模拟盘不结算时用 `exchange-doc` 构造并在 `meta.toml` 标"合成",方法 D 补核)
- 期望文件:同目录 `<case>.expected.jsonl`(`FundingSettle{inst: 0}` 与去重结果)
- 实现:`crates/venue-okx/tests/golden.rs::okx_326_golden_funding_fee`
- 备注:依赖待核实:开发备忘第 1 条

### OKX-327 金样:balance_and_position 的强平、ADL 与其他事件

- 规则:[用户流事件表](README.md#用户流事件表)
- 级别:golden
- 里程碑:M3
- 输入:`fixtures/okx-swap/balance_and_position/*.in`;case:`liquidation`、`adl`、`filled`、`snapshot`;来源:`filled`、`snapshot` 取模拟盘 raw(方法 C);`liquidation`、`adl` 与 OKX-320 同法取得
- 期望文件:同目录 `<case>.expected.jsonl`(0 事件,`hint` 渲染为 `{"hint":"MarginNow"}`)
- 实现:`crates/venue-okx/tests/golden.rs::okx_327_golden_balance_and_position_other`

### OKX-328 金样:liquidation-warning

- 规则:[用户流事件表](README.md#用户流事件表)
- 级别:golden
- 里程碑:M3
- 输入:`fixtures/okx-swap/liquidation-warning/*.in`;case:`warning`;来源:现有采集系统私有流 raw(该频道已在生产订阅)
- 期望文件:同目录 `<case>.expected.jsonl`(`{"hint":"MarginNow"}`)
- 实现:`crates/venue-okx/tests/golden.rs::okx_328_golden_liquidation_warning`

### OKX-329 金样:可忽略事件

- 规则:[用户流事件表](README.md#用户流事件表)、[VenueCaps 默认值](README.md#venuecaps-默认值)
- 级别:golden
- 里程碑:M3
- 输入:`fixtures/okx-swap/channel-conn-count/*.in`、`fixtures/okx-spot/channel-conn-count/*.in`;case:`count`、`count_error`;来源:待采集(方法 C:同一账户对 `orders` 开多条连接)
- 期望文件:同目录 `<case>.expected.jsonl`(`{"ignored":"channel-conn-count"}` 等)
- 实现:`crates/venue-okx/tests/golden.rs::okx_329_golden_ignorable`
- 备注:依赖待核实:开发备忘第 9 条

### OKX-330 金样:未知事件与未知私有频道

- 规则:[用户流事件表](README.md#用户流事件表)
- 级别:golden
- 里程碑:M3
- 输入:`fixtures/okx-swap/unknown/*.in`;case:`unknown_event_name`、`unknown_private_channel`;来源:`synthetic`
- 期望文件:同目录 `<case>.expected.jsonl`(计数,不是错误)
- 实现:`crates/venue-okx/tests/golden.rs::okx_330_golden_unknown_event`

### OKX-331 金样:account/config

- 规则:[账户配置核验](README.md#账户配置核验)
- 级别:golden
- 里程碑:M3
- 输入:`fixtures/okx-swap/rest.account_config/*.in`、`fixtures/okx-spot/rest.account_config/*.in`;case:`acctlv_1`、`acctlv_2`、`acctlv_3`、`acctlv_4`、`posmode_net`、`posmode_long_short`、`feetype_0`、`feetype_1`、`perm_with_withdraw`、`perm_without_trade`、`autoloan_true`、`spot_borrow_on`、`spot_borrow_off`、`spot_borrow_missing`、`uid_missing_demo`;来源:模拟盘 raw(方法 C),无法在模拟盘切换的组合用 `synthetic`(由真实响应改字段,`meta.toml` 写明)
- 期望文件:同目录 `<case>.expected.jsonl`(完整 Config 序列跑完后的 `AccountConfigReport`)
- 实现:`crates/venue-okx/tests/golden.rs::okx_331_golden_account_config`
- 备注:依赖待核实:开发备忘第 5、14 条

### OKX-332 金样:account/positions(Config 步 1 与兜底)

- 规则:[账户配置核验](README.md#账户配置核验)、[保证金数据换算](README.md#保证金数据换算)
- 级别:golden
- 里程碑:M3
- 输入:`fixtures/okx-swap/rest.account_positions/*.in`;case:`all_cross`、`one_isolated`、`foreign_isolated`;来源:模拟盘 raw(方法 C)
- 期望文件:同目录 `<case>.expected.jsonl`
- 实现:`crates/venue-okx/tests/golden.rs::okx_332_golden_rest_positions`

### OKX-333 金样:account/leverage-info

- 规则:[账户配置核验](README.md#账户配置核验)
- 级别:golden
- 里程碑:M3
- 输入:`fixtures/okx-swap/rest.leverage_info/*.in`;case:`three_insts`、`two_batches_second_failed`;来源:模拟盘 raw(方法 C),失败批为 `synthetic`
- 期望文件:同目录 `<case>.expected.jsonl`(`LeverageItem` 列表与 `UNREADABLE`)
- 实现:`crates/venue-okx/tests/golden.rs::okx_333_golden_leverage_info`

### OKX-334 金样:account/balance(推送过期兜底)

- 规则:[保证金数据换算](README.md#保证金数据换算)
- 级别:golden
- 里程碑:M3
- 输入:`fixtures/okx-swap/rest.account_balance/*.in`;case:`acctlv2`、`acctlv3`;来源:模拟盘 raw(方法 C)
- 期望文件:同目录 `<case>.expected.jsonl`(`MarginReport{source = Poll}`)
- 实现:`crates/venue-okx/tests/golden.rs::okx_334_golden_rest_balance`

### OKX-335 金样:下单、改单、撤单与批量帧的编码

- 规则:[3. 订单帧模板](README.md#3-订单帧模板)、[下单:WS `/private`](README.md#下单ws-private)
- 级别:golden
- 里程碑:M3
- 输入:`fixtures/okx-swap/encode/*.in`、`fixtures/okx-spot/encode/*.in`(每个 case 是一条或多条 `GwCmd` + `TargetRef` + `WallClock` 的规范 JSON);case:`place_limit`、`place_ioc_reduce_only`、`place_post_only`、`place_fok`、`amend`、`cancel`、`batch_place_20`、`batch_cancel_20`;来源:`synthetic`(帧由人按官方文档逐字段核对)
- 期望文件:同目录 `<case>.expected.jsonl`(每条命令一行 `{"frame":"<帧文本>"}`,与 `encode`/`encode_batch` 的输出逐字节相等)
- 实现:`crates/venue-okx/tests/golden.rs::okx_335_golden_encode`

### OKX-336 金样:orders-pending(只读查询)

- 规则:[只读查询](README.md#只读查询)
- 级别:golden
- 里程碑:M5
- 输入:`fixtures/okx-swap/rest.orders_pending/` 与 `fixtures/okx-spot/rest.orders_pending/` 的 `two_orders_one_foreign.in`、`empty.in`;来源:模拟盘 raw(方法 C)
- 期望文件:同目录 `<case>.ro.expected.jsonl`(`open_order` 行;外来单 `slot = UNRESOLVED`)
- 实现:`crates/venue-okx/tests/golden.rs::okx_336_ro_open_orders`

### OKX-337 金样:fills 与 fills-history(只读查询)

- 规则:[只读查询](README.md#只读查询)
- 级别:golden
- 里程碑:M5
- 输入:`fixtures/okx-swap/rest.fills/`、`fixtures/okx-swap/rest.fills_history/`、`fixtures/okx-spot/rest.fills/` 的 `page.in`、`last_page.in`;来源:模拟盘 raw(方法 C)
- 期望文件:同目录 `<case>.expected.jsonl`(`fill` 行与 `RoPage`)
- 实现:`crates/venue-okx/tests/golden.rs::okx_337_ro_fills`

### OKX-338 金样:资金费账单(只读查询)

- 规则:[只读查询](README.md#只读查询)
- 级别:golden
- 里程碑:M5
- 输入:`fixtures/okx-swap/rest.bills/funding_two_insts.in`;来源:模拟盘 raw(方法 C;模拟盘不结算时用文档示例,`source = "exchange-doc"`)
- 期望文件:同目录 `<case>.expected.jsonl`(`funding` 行)
- 实现:`crates/venue-okx/tests/golden.rs::okx_338_ro_funding`

### OKX-339 金样:account/balance(只读查询)

- 规则:[只读查询](README.md#只读查询)
- 级别:golden
- 里程碑:M5
- 输入:`fixtures/okx-swap/rest.balance/ro.in`、`fixtures/okx-spot/rest.balance/ro.in`;来源:模拟盘 raw(方法 C)
- 期望文件:同目录 `<case>.expected.jsonl`(`balance` 行)
- 实现:`crates/venue-okx/tests/golden.rs::okx_339_ro_balance`

### OKX-340 金样:account/positions(只读查询)

- 规则:[只读查询](README.md#只读查询)
- 级别:golden
- 里程碑:M5
- 输入:`fixtures/okx-swap/rest.positions/ro.in`;来源:模拟盘 raw(方法 C)
- 期望文件:同目录 `<case>.expected.jsonl`(`position` 行)
- 实现:`crates/venue-okx/tests/golden.rs::okx_340_ro_positions`

### OKX-400 fuzz:okx-swap(计入门槛 ③)

- 规则:[hft-venue-proto 契约 5](../hft-venue-proto/README.md#实现方必须遵守的契约)、[1. 分派](README.md#1-分派)
- 级别:fuzz
- 里程碑:M3
- 门槛:③
- 目标:`okx-swap`:按输入首字节分派到 `classify → peek → decode_md`、`decode_bbo_snapshot`、`classify_report → decode_report`(下单响应、私有推送、REST 响应)、`acct_decode`(Config 与 Margin 各步),均以 `OkxSwap` 与 `RD-OKX` 调用
- 语料:`cargo xtask fixtures corpus okx-swap` 由 `fixtures/okx-swap/**/*.in` 生成 + 变异;`fixtures/okx-swap/regressions/` 全部
- 判定:零 panic、零越界、零单输入超时(口径见 [testing §3.3](../../docs/testing.md#33-fuzz))
- 实现:`crates/venue-okx/fuzz/fuzz_targets/okx_swap.rs::okx_400_fuzz_okx_swap`

### OKX-401 fuzz:okx-spot(计入门槛 ③)

- 规则:[hft-venue-proto 契约 5](../hft-venue-proto/README.md#实现方必须遵守的契约)、[1. 分派](README.md#1-分派)
- 级别:fuzz
- 里程碑:M3
- 门槛:③
- 目标:`okx-spot`:同 OKX-400,以 `OkxSpot` 调用
- 语料:`fixtures/okx-spot/**/*.in` 生成 + 变异;`fixtures/okx-spot/regressions/` 全部
- 判定:零 panic、零越界、零单输入超时(口径见 [testing §3.3](../../docs/testing.md#33-fuzz))
- 实现:`crates/venue-okx/fuzz/fuzz_targets/okx_spot.rs::okx_401_fuzz_okx_spot`

### OKX-402 fuzz:编码往返(不计入门槛 ③)

- 规则:[3. 订单帧模板](README.md#3-订单帧模板)
- 级别:fuzz
- 里程碑:M3
- 目标:`fuzz_okx_encode_roundtrip`:任意字节构造 `GwCmd`(inst 取 `RD-OKX` 中的 inst,价量取任意 raw),`encode`/`encode_batch` 成功时用 `sonic-rs` 解析帧,`sz`、`px`、`clOrdId`、`instIdCode`、`expTime` 解析回的值与输入一致;失败时只能是 `EncodeErr`
- 语料:OKX-335 的输入 + 变异
- 判定:零 panic、零单输入超时(口径见 [testing §3.3](../../docs/testing.md#33-fuzz));往返不一致按崩溃处理
- 实现:`crates/venue-okx/fuzz/fuzz_targets/encode_roundtrip.rs::okx_402_fuzz_encode_roundtrip`

### OKX-500 基准:classify

- 规则:[延迟与资源契约](README.md#延迟与资源契约)
- 级别:bench
- 里程碑:M3
- 基准名:`bench_classify_okx`
- 实现:`crates/venue-okx/benches/decode.rs::okx_500_classify`

### OKX-501 基准:bbo-tbt 解码

- 规则:[延迟与资源契约](README.md#延迟与资源契约)
- 级别:bench
- 里程碑:M3
- 基准名:`bench_decode_okx_bbo`
- 实现:`crates/venue-okx/benches/decode.rs::okx_501_decode_bbo`

### OKX-502 基准:books5 解码

- 规则:[延迟与资源契约](README.md#延迟与资源契约)
- 级别:bench
- 里程碑:M3
- 基准名:`bench_decode_okx_books5`
- 实现:`crates/venue-okx/benches/decode.rs::okx_502_decode_books5`

### OKX-503 基准:order 帧编码

- 规则:[延迟与资源契约](README.md#延迟与资源契约)
- 级别:bench
- 里程碑:M3
- 基准名:`bench_encode_okx_order`
- 实现:`crates/venue-okx/benches/encode.rs::okx_503_encode_order`

### OKX-504 基准:orders 推送解码(含外部成交判定与去重)

- 规则:[延迟与资源契约](README.md#延迟与资源契约)
- 级别:bench
- 里程碑:M3
- 基准名:`bench_decode_okx_orders`
- 实现:`crates/venue-okx/benches/reports.rs::okx_504_decode_orders`

### OKX-505 基准:account 推送解码为 MarginReport

- 规则:[延迟与资源契约](README.md#延迟与资源契约)
- 级别:bench
- 里程碑:M3
- 基准名:`bench_decode_okx_account`
- 实现:`crates/venue-okx/benches/reports.rs::okx_505_decode_account`

### OKX-506 基准:positions 推送解码

- 规则:[延迟与资源契约](README.md#延迟与资源契约)
- 级别:bench
- 里程碑:M3
- 基准名:`bench_decode_okx_positions`
- 实现:`crates/venue-okx/benches/reports.rs::okx_506_decode_positions`

## 验收

- 本文件中里程碑 ≤ 当前里程碑的用例全部有同编号实现且通过(`cargo xtask test-trace`,[docs/tests §6](../../docs/tests/README.md#6-与代码的对应));`OKX-214`(门槛 ⑥)、`OKX-215`、`OKX-216` 的证据按各自 `证据` 字段归档。
- 金样(OKX-300–OKX-335)全绿,覆盖 [fixtures 测试与验收](../../fixtures/README.md#测试与验收) 第 1–10 条对 `okx-swap`、`okx-spot` 的要求(门槛 ②);新增 `51xxx` 登记码与新频道同一改动补 case。
- 属性测试(OKX-012、OKX-070、OKX-084、OKX-117、OKX-202)PR 例数见各用例;每夜各 100 000 例。
- fuzz:`okx-swap`、`okx-spot` 满足 [testing §8](../../docs/testing.md#8-上线门槛) 门槛 ③(累计口径归 testing §3.3、§6.1);`fuzz_okx_encode_roundtrip` 零失败。
- 基准(OKX-500–OKX-506)在参考机上不超过 README "延迟与资源契约"表与 [benchmarks](../../benchmarks/README.md) 的上限;`bench_decode_okx_account`、`bench_decode_okx_positions` 的 p99 不超过 [hft-venue](../hft-venue/README.md#延迟与资源契约) `venue_gw_margin_push`。
- 模拟盘冒烟(OKX-214)通过;`inTime→outTime` 与 `bbo-tbt` 单向延迟进入看板基线(迁东京后 p50 3.6 ms 作对照,只记录)。
- 可选扩展(增量深度 `books`/`books-l2-tbt`/`books50-l2-tbt` 与其重同步、SBE 行情与 `books-sbe` 快照、`mock` 反向编解码)本文件不含用例:启用时同一改动补写用例并通过(含 README 的 `bench_decode_okx_sbe_bbo`、`bench_decode_okx_l2` 两个基准);核实开发备忘第 10 条后登记的"持仓模式不符"拒绝码同一改动补 `hint = ConfigRecheck` 的用例。
