---
description: "Gate 现货与 USDT 永续的纯协议实现(optional,现货优先):book_ticker/obu/order_book_update/SBE 解码,WS API 下单帧(action_mode=ACK)与回报解码;需要 Gate 腿时接入"
kind: crate
tier: pure
scope: optional
status: frozen
---

# venue-gate

## 概要

> **冻结**:不随 core 规格同步;解冻(触发条件同原文)时先按 core 规格复核。已知待复核(V3 = `architecture-v2/_work/review-2026-10-03-M0c-V3一致性.md` §2):#53 能力表缺 `fok` 行;#56 Gate 字符串错误码到 `reject_code` 的映射;#59 期货合约乘数在运行时乘除(以 hft-refdata 的指数平移为准);#60 现货能力表缺 `required_spot_borrow`(应为 `Some(false)`);#64 `stale_guard()` 在核实前已声明 `ExpireAt`(核实前应为 `None`);#101 `EncodeErr` 变体写法与 hft-venue-proto 不一致;#106 金样目录 `fixtures/gate/` 与 `NAME` 常量不一致;#107 `text` 长度上限是否含 `t-` 前缀的表述不一。

> **可选扩展**。何时接入:策略需要 Gate 的现货腿(某些币只在 Gate 有足够现货深度或更低费率),或需要第二个现货所分散对手方风险。接入顺序**现货优先**(`GateSpot` 先于 `GateFutures`);接入即把本 README 的"接入时补齐"各节按 [hft-venue-proto 必备节](../hft-venue-proto/README.md#每个-venue-readme-的必备节) 写全。

`venue-gate` 实现两个 [`VenueProtocol`](../hft-venue-proto/README.md#venueprotocol静态分派):`GateSpot` 与 `GateFutures`(USDT 结算永续)。它拥有 Gate 的协议知识:`book_ticker` 与深度频道(`spot.obu`、`order_book_update`)的 `U/u` 链续、spot SBE 流、WS API 的 `login`(HMAC-SHA512)与 `order_place/amend/cancel` 帧(`action_mode=ACK`)、请求头 `x-gate-exptime` 过期保护、响应里的 `x_in_time/x_out_time` µs 网关戳、`t-` 前缀 client id、期货"张数"换算、官方限速默认值。Gate 在 AWS ap-northeast-1 且 LB 覆盖三个 AZ,是协议最完整的一家(WS 下单 + µs 网关戳 + exptime + SBE),其网关戳适合做交易所内部延迟的基线([research/02 §6](../../design/research/02-exchange-connectivity.md))。纯函数,运行时见 [`hft-venue`](../hft-venue/README.md)。

## 目录

- [使用本组件](#使用本组件)
- [实现说明](#实现说明)
- [延迟与资源契约](#延迟与资源契约)
- [测试与验收](#测试与验收)
- [依赖](#依赖)
- [延伸阅读](#延伸阅读)
- [已知限制与后续工作](#已知限制与后续工作)

## 使用本组件

```rust
pub struct GateSpot;      // NAME = "gate-spot"
pub struct GateFutures;   // NAME = "gate-futures"(settle = usdt)

// client id(`text`):`render_cid(&ClientOrderId, &mut [u8; 48])` / `parse_cid(&[u8]) -> Option<ClientOrderId>`(VenueProtocol 方法)。
// 编码是 hft-types 的 ClientOrderId(96 bit,含 epoch/venue/account/slot/gen/校验,规范 24 位十六进制),本 crate 只加外形:t-<24hex>,26 字节(去前缀 ≤ 28)。
```

### 端点

| 用途 | 端点 | 说明 |
|---|---|---|
| 现货行情 + 下单 + 私有频道 | `wss://api.gateio.ws/ws/v4/` | 同一端点承载订阅与 WS API(`event: "api"`);ELB 直连 |
| 现货 SBE 行情 | `wss://api.gateio.ws/ws/v4/ws/spot/sbe` | JSON 订阅 + 首响应,之后 SBE 二进制帧(opcode 2) |
| 期货行情 + 下单 + 私有频道 | `wss://fx-ws.gateio.ws/v4/ws/usdt` | EC2 直连 |
| REST | `https://api.gateio.ws/api/v4` | 快照、查询、兜底 |

LB 节点分布在三个 AZ;部署期逐 A 记录测 RTT,把同 AZ 节点钉进 `endpoints[].ips`(同 AZ 0.38 ms vs 跨 AZ 0.83–1.54 ms,[research/02 §1.2](../../design/research/02-exchange-connectivity.md))。连接数 ≤ 300 / IP。

### 行情频道 → 事件

| 频道 | 节拍 | 规则 | 事件 | 时间戳 |
|---|---|---|---|---|
| `spot.book_ticker`(独立 BBO 连接) | 10 ms | `Monotonic` on `u` | `Bbo`(bbo 环) | `ex_ts_ns ← result.t`(ms),`ex_send_ns ← time_ms` |
| `futures.book_ticker`(独立 BBO 连接,双路) | 实时 | `Monotonic` on `u` | `Bbo`(bbo 环) | 同上 |
| `spot.obu`(50 档) | 20 ms | 首帧 `full=true` 为快照;之后 `Chain`(`prev_id = U − 1`) | `L2Snapshot` / 一条 `L2Batch` | 同上 |
| `futures.order_book_update`(20 档) | 20 ms | `Chain`(`prev_id = U − 1`);快照走 REST(`with_id=true`) | 一条 `L2Batch` | 同上 |
| `spot.trades` / `futures.trades` | 实时 | `Monotonic` on `id` | `Trade` | `ex_ts_ns ← create_time_ms` |
| SBE `bbo`(模板 1) | 实时 | `Monotonic` | `Bbo` | 以 schema 字段为准 |
| SBE `obu`(模板 3)/ `orderBookUpdate`(模板 5) | 20 ms | `Chain`(`prev_id = U − 1`) | `L2Batch` | 同上 |

- `futures.book_ticker` 的 `B`/`A` 是 JSON **数字**(张数),现货是字符串(现有 cex-arb 的坑);两者都走定点解析,期货再乘合约乘数。
- 期货数量以"张"计:`qty_base = size × quanto_multiplier`,乘数来自 refdata `InstMeta`;`futures.trades.size` 带符号(负数 = 卖方主动)。

### 下单:WS API v4

| 操作 | 现货 channel | 期货 channel | 关键参数 |
|---|---|---|---|
| 登录 | `spot.login` | `futures.login` | `payload{api_key, signature, timestamp, req_id}`;签名见下 |
| 下单 | `spot.order_place` | `futures.order_place` | 现货 `req_param{text, currency_pair, type:"limit", account:"spot", side, amount, price, time_in_force ∈ {gtc, ioc, poc, fok}, action_mode:"ACK"}`;期货 `req_param{text, contract, size(带符号整数张), price, tif, reduce_only}` |
| 批量 | `spot.order_place`(载荷为数组) | `futures.order_batch_place` | 现货 ≤ 4 币对 × 10 单;期货 ≤ 10 单 |
| 改单 | `spot.order_amend` | `futures.order_amend` | `order_id` + 新 `price`/`amount`(`size`) |
| 撤单 | `spot.order_cancel` / `order_cancel_ids` / `order_cancel_cp` | `futures.order_cancel` / `order_cancel_cp` | 按 id / 批量 id / 按币对全撤 |
| 查询 | `spot.order_status` | `futures.order_status` | `order_id` 或 `text` |
| 过期保护 | 请求头 `x-gate-exptime`(ms 绝对时间,超过即拒) | 同左 | `now + gw.stale_guard_ms` |

- **签名**:`HexEncode(HMAC_SHA512(secret, "api\n<channel>\n<req_param 原文>\n<ts 秒>"))`;登录时 `req_param` 为空串,即 `"api\nspot.login\n\n<ts>"`。窗口 60 s。登录后会话内请求**不再签名**。
- **`action_mode=ACK`**:异步精简回执,最快;成交与终态以私有频道 `spot.orders`/`futures.orders` 的 update 为准(累计量来源);`spot.usertrades`/`futures.usertrades` 只取费用。私有频道在同一下单连接上订阅(需 `auth` 签名,warm 线程完成)。
- 响应头带 `x_in_time`/`x_out_time`(µs,网关收到/发出)、`conn_id`、`trace_id`、`x_gate_ratelimit_*`。

### 限速默认值(`rate_rules()`)

| 范围 | 现货 | 期货 |
|---|---|---|
| 下单 + 改单 / UID(+市场) | 10 r/s | 100 r/s(VIP14+ 按 7 日成交率放宽到 400) |
| 撤单 / UID | 200 r/s | 200 r/s |
| 公共 REST | 200 r / 10 s / 端点 / IP | 同左 |
| 低成交率账户(< 0.1) | 下单 + 改单 10 / 10 s(2026-01-26 起) | — |
| WS 连接 | ≤ 300 / IP | ≤ 300 / IP |

来源 [research/02 §6](../../design/research/02-exchange-connectivity.md)。响应头 `x_gate_ratelimit_requests_remain / limit / reset_timestamp` → `RateObs`。

### L2 序号归一化(`L2Batch.prev_id/last_id`)

| 流 | `prev_id` | `last_id` | 快照 `snap_id` |
|---|---|---|---|
| `spot.obu` 增量 / SBE obu、orderBookUpdate | `U − 1` | `u` | 首帧 `full=true` 的 `u` |
| `futures.order_book_update` | `U − 1` | `u` | REST `order_book?with_id=true` 的 `id` |
| 快照后第一条 | `snap_id`(`RxMeta.prev_override`) | `u` | — |

契约见 [hft-venue-proto](../hft-venue-proto/README.md#实现方必须遵守的契约)。

### 累计量与费用来源

| 流 | `cum_filled` | `avg_px` | 费用 |
|---|---|---|---|
| `futures.orders` update | `(size − left) × quanto_multiplier`(张数带符号取绝对值) | `fill_price` | 不含;来自 `futures.usertrades.fee`(按 `(inst, trade id)` 去重、`order_id` 关联;[契约 22](../hft-venue-proto/README.md#实现方必须遵守的契约)) |
| `spot.orders` update | `amount − left` | `avg_deal_price` | 不含;来自 `spot.usertrades.fee` + `fee_currency` |

### VenueCaps 默认值

`default_caps()`([hft-venue-proto 契约 15](../hft-venue-proto/README.md#实现方必须遵守的契约));本所为 optional,除协议字段外大多待核实,接入时逐项核实并改标来源。

| 字段 | `GateSpot` | `GateFutures` | 来源 |
|---|---|---|---|
| `fee_asset_rule` | `ReceivedAssetUnlessDiscount{GT}`(现货手续费扣收到的币;开启 GT 抵扣且 GT 余额足 → 扣 GT) | `Quote`(USDT) | 待核实(评审 P0-4 的前提;用 `spot.usertrades.fee_currency` 实测) |
| `reduce_only` | 否 | 是(`reduce_only`) | 官方 |
| `required_position_mode` | 不适用 | 单向持仓(双仓模式关闭) | 待核实(字段) |
| `required_margin_mode` | 不适用 | 全仓 | 待核实(Gate 期货以杠杆值表达全仓/逐仓,写法待核实) |
| `funding_settle` | 不适用 | `BalanceOnlyPush`(`futures.balances` 的资金费类变动是否带合约名待核实;带则 `PerSymbolPush`) | 待核实 |
| `margin_data` | 不适用 | `Poll{interval_ms: 5000}`(若核实 `futures.positions`/`futures.balances` 推送足够,改 `Push`) | 待核实 |
| `own_liquidation_marker` | 无 | `futures.orders` 的 `finish_as ∈ {liquidated, auto_deleveraged}` 或 `text` 为系统强平/减仓标记 | 待核实 |
| `ignorable_user_events` | 空(接入时补) | 空 | 待核实 |
| `amend` | 是(`spot.order_amend`) | 是(`futures.order_amend`) | 官方 |
| `post_only` | 是(`poc`) | 是(`poc`) | 官方 |
| `ioc` | 是 | 是 | 官方 |
| `batch_cancel` | 是(`order_cancel_ids`、`order_cancel_cp`) | 是(`order_cancel_cp`) | 官方 |
| `dms` | 待核实(`countdown_cancel_all` 候选;核实前无) | 同左 | 待核实 |
| `testnet` | `Testnet{…}`(官方提到测试网,含 SBE;端点待核实) | 同左 | 待核实 |

`stale_guard()` = `ExpireAt{default_ms: 1500}`(`x-gate-exptime`);`dead_man()` 核实前 `None`。

### 接入时补齐

以下各节在接入时按 [hft-venue-proto 必备节](../hft-venue-proto/README.md#每个-venue-readme-的必备节) 写全,这里只记录已知线索:

- **账户配置核验**:现货——账户 UID 与 key 权限(提现关闭)、GT 抵扣开关与 `fee_asset_rule` 是否一致、统一账户是否开启(本 crate 只支持现货现金账户)、现货借币 / 杠杆是否关闭(开启 → `AUTO_BORROW`);期货——双仓模式、全仓设置、每合约杠杆(`LeverageItem`)。会话 key 与 REST key 是否同一把(决定 `session_identity_request`)。端点待核实。
- **保证金数据换算**:期货账户(总权益、维持保证金、可用)与持仓(强平价 `liq_price`、ADL 档,字段待核实)。现货不适用。
- **外部成交识别**:期货见上表 `own_liquidation_marker`;`text` 不以 `t-` 开头的成交为非本系统订单(网页单的 `text` 取值待核实,用于区分 `Manual`)。
- **用户流事件表**:`spot.orders`、`spot.usertrades`、`spot.balances`、`futures.orders`、`futures.usertrades`、`futures.positions`、`futures.balances` 各自的产出;其他频道事件按可忽略/未知处理。
- **结果未知码表**:见"回报"一节的两张 `label` 表。
- **测试网/模拟盘**:端点与限制待核实。

### `mock` 特性

与 [venue-binance](../venue-binance/README.md#mock-特性给-mock-exchange-的反向编解码) 相同的四个反向编解码函数(`encode_market`、`decode_order_frame`、`encode_order_event`、`encode_rest_response`),接入时一并交付,供 [mock-exchange](../../apps/mock-exchange/README.md) 使用。

## 实现说明

<details><summary>实现要点</summary>

### 1. 分派

推送帧形如 `{"time":…,"time_ms":…,"channel":"spot.book_ticker","event":"update","result":{…}}`;`classify` 在前 64 B 内用 `memchr` 找 `"channel":"` 后的频道名,按前 8 字节 + 长度区分。`event` 为 `subscribe` 的是订阅回执(`result.status`),`api` 的是 WS API 响应;带 `"ack":true` 的是网关确认帧。SBE 连接:8 B 小端头 `blockLength, templateId, schemaId, version`;订阅时选 `schemaId=1`(定点),解码器拒绝其他 schema。

### 2. 深度同步

- `spot.obu`:`ResyncMethod::Resubscribe`——断裂时对该币对退订再订阅,新首帧 `full=true` 即快照(→ `L2Snapshot`),无需 REST。
- `futures.order_book_update`:`ResyncMethod::RestSnapshot`,`GET /futures/usdt/order_book?contract=&limit=20&with_id=true` 得 `id`。`align_first(L=id)`:`u < L + 1` → `Drop`;`U ≤ L + 1 ≤ u` → `Apply`;`U > L + 1` → `Gap`。此后按 `Chain`(`prev_id = U − 1`)检查。
- 期货档位是对象 `{"p":"价","s":张数}`,现货是 `["价","量"]` 数组;`qty == 0` 删档。

### 3. 订单帧模板

```text
{"time":1790000000,"channel":"futures.order_place","event":"api",
 "payload":{"req_id":"p0000002a00000003","req_header":{"x-gate-exptime":"1790000001500"},
  "req_param":{"text":"t-1a2b3c4d5e6f708192a3b4c5","contract":"BTC_USDT","size":-4,"price":"65000.1","tif":"ioc","reduce_only":false}}}
```

`reduce_only` 只在 `GwCmd.oflags` 带 `REDUCE_ONLY` 时为 true(只有期货模板有该字段;现货命令带 `REDUCE_ONLY` → `EncodeErr::Unsupported`)。

- `time`(秒)与 `x-gate-exptime`(ms)由 `WallClock` 填写;`req_id` 编码 `(op, slot, gen)`,响应据此还原 `ReqRef`。
- 现货 `amount` 为基础币数量;市价买单的 `amount` 是计价币金额(现有 cex-arb 已踩过),我们只发限价单(IOC 用 `time_in_force=ioc`,post-only 用 `poc`)。
- 期货 `size` = `qty / quanto_multiplier`,必须整除(refdata 的 lot 保证);卖出取负。
- 帧长 ≤ 600 B;`ORDER_FRAME_CAP = 2048`。

### 4. 回报

| 输入 | `OrderEvent.what` | 字段 |
|---|---|---|
| `order_place` 响应 `status=200` | `Ack` | `exch_order_id ← result.id`;`ex_ts_ns ← x_out_time`(µs) |
| 响应错误(`label`、`message`) | 登记为确定拒绝的 `label` → `Reject`;其他 → `outcome_unknown` | 确定拒绝表:`BALANCE_NOT_ENOUGH`、`INVALID_PARAM_VALUE`、`ORDER_POC_IMMEDIATE`、`TOO_MANY_REQUESTS` 等;**结果未知表**:`SERVER_ERROR`、`TOO_BUSY`、网关超时类(确切 label 待核实)与任何未登记 label(契约 8) |
| `x-gate-exptime` 已过 | `Reject`(过期标签,待核实确切 label) | — |
| `spot.orders` `event=put` | `Ack`(幂等) | — |
| `spot.orders` / `futures.orders` `event=update`(`left` 减少) | `PartialFill` / `Fill`(**成交源**) | `cum_filled ← size − left`(期货张数 × 乘数;现货 `amount − left`),`avg_px ← fill_price`(期货)/ `avg_deal_price`(现货),`ex_ts_ns ← update_time_ms`;本笔量 = 新 `cum` − 上次 `cum`(`GwCtx` 按 slot 记上次 cum) |
| `spot.usertrades` / `futures.usertrades` | 不产出成交;只补费用 | `fee`、`fee_currency` 按 `(inst, trade id)` 去重,经 `order_id` 关联到该槽,在下一条(或延迟 ≤ 1 条)成交类 `OrderEvent` 里带出;订单已终态(或之后再无成交类事件)时到达的费用单独发一条 `OrderEvent{what: FeeOnly, qty: 0, fee, fee_asset, exch_order_id}`([hft-venue-proto 契约 12](../hft-venue-proto/README.md#实现方必须遵守的契约));不带 `text` 时由 gw 的 `IdTable` 终态保留条目解析到原订单([hft-venue §6](../hft-venue/README.md#6-exch-id--slot-私有表idtable)) |
| `orders` `event=finish`,`finish_as ∈ {cancelled, ioc, poc, stp}` | `CancelAck` / `Expired` | `left` 校验剩余 |
| `finish_as=filled` | `Fill` 终态(`cum = size`) | — |
| `order_amend` 响应 | `AmendAck` / `AmendReject` | — |

网关停留 `x_out_time − x_in_time` 放进 `ReportOutcome`,由 gw 作为延迟样本上报(直接测量交易所内部延迟,[05 §7](../../design/05-CEX执行层.md))。`futures.positions`、`spot.balances` → `PositionReport`/`BalanceReport`(只校验)。

### 5. 心跳与会话

- 应用层 `{"time":…,"channel":"spot.ping"}`(期货 `futures.ping`)每 15 s,回 `*.pong`;另有 WS Ping 用于 RTT。服务端超时阈值待核实,先按 30 s 无入站判死。
- 登录在 warm 线程完成;私有频道订阅 `auth{method:"api_key", KEY, SIGN}` 也在 warm 线程签好随登录一起发出,再交接给 gw。

### 6. 死手

`dead_man()` **待核实**:候选为 REST `POST /spot/countdown_cancel_all` 与 `POST /futures/usdt/countdown_cancel_all`(`timeout` 秒,可重复调用续期,0 取消)。核实前按 `DeadManSpec::None` 处理,由 [kill-switch](../../apps/kill-switch/README.md) 兜底;核实后改为 `CountdownAccount{via: Rest}`,并满足 `timeout_ms ≥ 2 × refresh_ms`([hft-venue-proto](../hft-venue-proto/README.md#辅助类型))。

### 7. 源文件规划

| 文件 | 职责 |
|---|---|
| `src/lib.rs` | `GateSpot`、`GateFutures` |
| `src/cid.rs` | `t-` client id、`req_id` |
| `src/sign.rs` | HMAC-SHA512 登录与私有订阅签名 |
| `src/labels.rs` | 错误 `label` 两张表:确定拒绝 / 结果未知(未登记默认结果未知) |
| `src/md_json.rs` | book_ticker、obu、order_book_update、trades |
| `src/md_sbe.rs` | SBE 头与模板 1/2/3/5 |
| `src/snapshot.rs` | 期货 REST 快照与 `align_first` |
| `src/orders.rs` | 模板与 `encode`(现货/期货) |
| `src/reports.rs` | WS API 响应、orders/usertrades/positions/balances |

</details>

## 延迟与资源契约

纯函数,热路径零分配零系统调用。

| 入口 | p50 / p99 预算 | 基准名 |
|---|---|---|
| `classify` | ≤ 30 ns / ≤ 60 ns | `bench_classify_gate` |
| `decode_md` book_ticker → `Bbo` | ≤ 0.4 µs / ≤ 0.8 µs | `bench_decode_gate_bt` |
| `decode_md` obu 50 档增量 | ≤ 2 µs / ≤ 4 µs | `bench_decode_gate_obu` |
| `decode_md` SBE bbo | ≤ 60 ns / ≤ 120 ns | `bench_decode_gate_sbe_bbo` |
| `encode` futures.order_place | ≤ 0.3 µs / ≤ 0.6 µs | `bench_encode_gate_place` |
| `decode_report` orders update | ≤ 1 µs / ≤ 2 µs | `bench_decode_gate_orders` |

## 测试与验收

**金样**(`fixtures/gate/`):spot/futures `book_ticker`(含期货数字型 `B/A`)、`spot.obu`(`full=true` 首帧、增量、删档)、`futures.order_book_update` 与 REST 快照(对齐四种情况)、`trades`(期货负 size)、SBE bbo/obu/orderBookUpdate(schemaId 1)、login 成功/失败、`order_place` ack 帧 + 结果帧、错误 label(余额不足、POC 会吃单、限速、过期)、amend、cancel、`order_cancel_cp`、`spot.orders` put/update/finish 各 `finish_as`、`usertrades`、`futures.positions`。

**属性**:对任意 `ClientOrderId`,`render_cid` 输出 `t-` + 24 位小写十六进制(26 字节)且 `parse_cid` 往返;缺 `t-` 前缀或长度不符 → `None`;期货张数换算与 refdata 乘数一致(不整除返回 `EncodeErr::FieldOverflow`,不静默取整);`encode` 输出可被 `sonic-rs` 解析且与输入 `GwCmd` 一致。

**fuzz**:`fuzz_gate_md_json`、`fuzz_gate_sbe`、`fuzz_gate_report`、`fuzz_gate_snapshot`、`fuzz_gate_encode_roundtrip`。

**验收**:金样全绿;fuzz 每个目标累计 ≥ 2 h 零 panic(跨多夜累计,任一失败清零,[testing §8](../../docs/testing.md#8-上线门槛) 门槛 ③);上表预算;testkit 假交易所 + 测试网连续 3 天冒烟([testing §8](../../docs/testing.md#8-上线门槛));`x_out_time − x_in_time` 分布进入看板作为交易所内部延迟基线。

## 依赖

| 依赖 | 理由 |
|---|---|
| `hft-types`、`hft-refdata`、`hft-event` | 定点、合约乘数、事件体 |
| `hft-venue-proto` | `VenueProtocol` 与辅助类型(测试与 fuzz 用其 `testkit` feature) |
| `sonic-rs 0.5` | JSON |
| `hmac 0.12` + `sha2 0.10` | HMAC-SHA512(warm 线程) |
| `hex` / `memchr` | 签名编码、分派 |

**禁止**:`hft-net`、`hft-venue`、`tokio`、`serde_json`(dev-only)。

## 延伸阅读

- [research/02 §6 Gate](../../design/research/02-exchange-connectivity.md)、[§10 对比表](../../design/research/02-exchange-connectivity.md)
- [04 §5](../../design/04-行情接入层.md)、[05 §7](../../design/05-CEX执行层.md)
- [hft-venue](../hft-venue/README.md)、[venue-binance](../venue-binance/README.md)(core,结构对照)、[cookbook/adding-a-venue.md](../../docs/cookbook/adding-a-venue.md)
- 现有知识来源:旧套利引擎 Rust 交易模块 `cex-arb/src/exchange/gate_spot.rs`(位于 `E:\trading\Trading\` 下;登录签名串、ack 帧)、`feed.rs`(book_ticker 字段)

## 已知限制与后续工作

- **期货 SBE**:官方未见文档,期货行情先用 JSON;核实后加模板。
- **STP**:`stp_act` 需先加入 STP 组,否则拒单;我们不设置该字段。
- **统一账户 / 其他结算币**:只做 USDT 永续与现货现金账户;BTC 结算、交割合约不在范围。
- **REST 下单兜底**:只在 WS 连接全部不可用时由 gw 走 REST(签名 `HMAC_SHA512(METHOD\nURL\nquery\nSHA512(body)\nts)`),不作为常规路径。

### 开发备忘

<details><summary>给实现者</summary>

**待核实**(方法:测试网实测,样本存 fixtures;只有实盘才会出现的样本在该所上线后从 raw journal 补核)

1. `x-gate-exptime` 在 WS API 中的位置(`req_header` 对象内?)与过期拒单的确切 `label`。
2. `text` 长度上限是否含 `t-` 前缀(官方表述为"去掉 `t-` 后 ≤ 28 字节");我们固定 26 字节(`t-` + 24 位十六进制),两种解释下都 ≤ 28,都合法。
3. 死手:`countdown_cancel_all` 现货/期货端点是否存在、最小 `timeout`、是否有 WS API 等价方法。
4. 服务端心跳超时阈值与 `*.ping` 是否必需。
5. `action_mode=ACK` 下是否仍先发 `"ack":true` 网关帧再发结果帧;两帧的 `x_in_time/x_out_time` 分别代表什么。
6. futures SBE 是否存在;spot SBE schemaId 2 的"统一簿字段"是否对我们更省解析。
7. `futures.orders` 的 `left` 符号(是否随 `size` 带符号)、`fill_price` 是否为累计均价;`spot.orders` 的 `avg_deal_price` 在部分成交时是否已更新;usertrades 与 orders update 的到达先后分布(决定费用关联的等待策略)。方法:测试网小单 + raw 样本。

**实现顺序(现货优先)**:现货 `spot.book_ticker`/`spot.obu` → 现货下单(`spot.login`、`spot.order_place`)与回报(`spot.orders` + `spot.usertrades` 费用关联)→ 现货账户配置核验与余额 → 期货 book_ticker/order_book_update + 快照 → 期货下单与回报 → 期货保证金与外部成交 → 现货 SBE(可选)。

</details>
