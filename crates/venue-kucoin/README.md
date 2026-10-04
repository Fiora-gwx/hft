---
description: "KuCoin 现货与永续的纯协议实现:bullet token 引导、level1/level2 解码与 sequence 链续、18 s ping、Pro WS 下单(挑战式登录、allowMaxTimeWindow)与回报解码"
kind: crate
tier: pure
scope: optional
status: frozen
---

# venue-kucoin

## 概要

> **冻结**:不随 core 规格同步;解冻(触发条件同原文)时先按 core 规格复核。已知待复核(V3 = `architecture-v2/_work/review-2026-10-03-M0c-V3一致性.md` §2):#53 能力表缺 `fok` 行;#59 期货合约乘数在运行时乘除(以 hft-refdata 的指数平移为准);#63 未登记错误码映射为 `Reject`(契约 8 要求按结果未知);#105 协议常量(`HeartbeatSpec` 等)的来源;#106 金样目录 `fixtures/kucoin/` 与 `NAME` 常量不一致。

> **可选扩展**。何时值得接入:离线研究显示 KuCoin 腿有可用机会,或拿到 MM 计划的 DMA/co-location(30 日成交 ≥ 3000 万 USDT)。

`venue-kucoin` 实现 `KucoinSpot` 与 `KucoinFutures` 两个 [`VenueProtocol`](../hft-venue-proto/README.md#venueprotocol静态分派)。它拥有 KuCoin 的协议知识:**bullet token 引导**(连接行情 WS 前先 REST 取 token 与服务器列表)、服务器下发的 `pingInterval`(18 s)/`pingTimeout`(10 s)、`level1`/`level2` 频道与 `sequence` 链续、Pro WS 下单连接的 URL 签名与**挑战式会话握手**、`spot.order`/`futures.order` 帧、`allowMaxTimeWindow + clientTimestamp` 过期保护、响应 `inTime/outTime`(Colo UTA 用户为 ns)、`userRateLimit`、`clientOid` 格式与限速默认值。KuCoin 的 MM 计划(30 日成交 ≥ 3000 万 USDT)明示 **DMA + co-location**,达标后是真正的直连;在此之前 WS 经 CloudFront、REST 经 Cloudflare([research/02 §7](../../design/research/02-exchange-connectivity.md))。纯函数,运行时见 [`hft-venue`](../hft-venue/README.md)。

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
pub struct KucoinSpot;      // NAME = "kucoin-spot"(hf 高频账户)
pub struct KucoinFutures;   // NAME = "kucoin-futures"

// client id(`clientOid`):`render_cid(&ClientOrderId, &mut [u8; 48])` / `parse_cid(&[u8]) -> Option<ClientOrderId>`(VenueProtocol 方法)。
// 编码是 hft-types 的 ClientOrderId(96 bit,含 epoch/venue/account/slot/gen/校验,规范 24 位十六进制),本 crate 只加外形:<24hex>,24 字节(上限 40 待核实)。
```

### 端点与引导

| 用途 | 端点 | 说明 |
|---|---|---|
| 行情引导 | `POST https://api.kucoin.com/api/v1/bullet-public` | 返回 `token` 与 `instanceServers[{endpoint, pingInterval, pingTimeout}]`;token 用于建连,有效 24 h |
| 行情 WS | `{endpoint}?token=<t>&connectId=<唯一>` | 5 s 内须收到 `welcome`;CloudFront |
| 私有推送引导 | `POST /api/v1/bullet-private`(签名) | 同上 |
| 下单 WS(Pro) | `wss://wsapi.kucoin.com/v1/private?apikey=&timestamp=&sign=&passphrase=&enable_ns=true` | URL 参数全部 percent-encode |
| REST | `https://api.kucoin.com`(现货)、`https://api-futures.kucoin.com`(期货) | Cloudflare;快照、查询、DCP |

`VenueProtocol::bootstrap` 返回 bullet 请求;warm 线程在每次建连前取新 token(不缓存跨连接)。

### 行情频道 → 事件

| 频道 | 节拍 | 规则 | 事件 | 时间戳 |
|---|---|---|---|---|
| 现货 `/spotMarket/level1:{sym}`(独立 BBO 连接,双路) | 10 ms | `Monotonic`(`sequence`,待核实) | `Bbo` | 以帧内时间为准(待核实) |
| 现货 `/market/level2:{sym}` | 实时增量 | `Chain`(`prev_id = sequenceStart − 1`,`last_id = sequenceEnd`) | 每条消息一条 `L2Batch` | `time` |
| 现货 `/spotMarket/level2Depth50:{sym}` | 100 ms 快照 | `Snapshot` | 只作校验 | — |
| 现货 `/market/match:{sym}` | 逐笔 | `Monotonic` on `sequence` | `Trade` | `time`(ns) |
| 期货 `/contractMarket/level2:{sym}` | 实时增量 | `Chain`(`prev_id = sequence − 1`) | `L2Batch` | `timestamp` |
| 期货 `/contractMarket/tickerV2:{sym}` | 实时 | `Monotonic` on `sequence` | `Bbo` | `ts`(ns) |
| 期货 `/contractMarket/execution:{sym}` | 实时 | `Monotonic` on `sequence` | `Trade` | `ts` |

- 每个 topic ≤ 100 个 symbol(逗号拼接)。
- **ticker 的 `time` 是最后成交时间,不是推送时间**(现有 cex-arb 的坑),用它做 `ex_ts` 会把延迟算错;BBO 用 `level1`/`tickerV2`。
- level2 快照:现货 `GET /api/v3/market/orderbook/level2?symbol=`(需签名)、期货 `GET /api/v1/level2/snapshot?symbol=`,取 `sequence` 作 `snap_id`。`align_first`:`sequenceEnd ≤ L` → `Drop`;`sequenceStart ≤ L + 1 ≤ sequenceEnd` → `Apply`(变更逐档再按档内 `sequence ≤ L` 跳过);否则 `Gap`。

### 下单:Pro WS

| 步骤 / 操作 | 内容 |
|---|---|
| URL 签名 | `sign = Base64(HMAC-SHA256(secret, apikey + timestamp))`;`passphrase = Base64(HMAC-SHA256(secret, passphrase))` |
| 会话握手 | 服务端先发会话消息 → 客户端回 `Base64(HMAC-SHA256(secret, 原始会话消息))`(**纯文本帧,无 JSON 包裹**)→ 期待 `data == "welcome"`(`LoginStep::Handshake`,`LoginResult::Continue`) |
| 下单 | `{"id":"<req>","op":"spot.order","args":{clientOid, symbol, side, type:"limit", price, size, timeInForce ∈ {GTC, IOC, FOK}, postOnly, allowMaxTimeWindow, clientTimestamp}}`;期货 `futures.order`(`size` 为张数) |
| 同步下单 | `spot.sync_order`:等撮合结果再回(现有 cex-arb 市价单在用);热路径用异步 `spot.order` |
| 撤单 / 改单 | `spot.cancel`、`futures.cancel`;改单(`modify`)**待核实** |
| 过期保护 | `allowMaxTimeWindow`(ms)+ `clientTimestamp`:网关收到时已超窗即拒。**是否对 Pro WS 下单生效待核实**;核实前按"无过期保护"处理:查询"订单不存在"不定终态,只由对账收敛 |
| 响应 | `{"id","op","code":"200000","data":{orderId, clientOid…},"inTime","outTime","userRateLimit":{limit, remaining, reset}}`;`inTime/outTime` µs(`enable_ns=true` 且 Colo UTA 用户为 ns) |

### 限速默认值

| 范围 | 值 |
|---|---|
| UTA 交易(含 WS 下单) | VIP0 300/s … VIP12 4000/s;WS 与 REST 共享 |
| Classic 权重池 / 30 s | VIP0 Spot 4000、Futures 2000 … VIP12 40 000 / 20 000 |
| WS 连接 | 新连接 ≤ 300 / 5 min / IP;公共 ≤ 500(现货)/ 800(期货)/ IP;私有 ≤ 1024 |
| WS 入向消息 | 公共 300 / 私有 100 条 / 10 s / 连接 |
| topic | 公共 ≤ 600 / 私有 ≤ 400 / 连接;每 topic ≤ 100 symbol |

### L2 序号归一化(`L2Batch.prev_id/last_id`)

| 流 | `prev_id` | `last_id` | 快照 `snap_id` |
|---|---|---|---|
| 现货 `/market/level2` | `sequenceStart − 1` | `sequenceEnd` | REST 快照 `sequence` |
| 期货 `/contractMarket/level2` | `sequence − 1` | `sequence` | REST 快照 `sequence` |
| 快照后第一条 | `snap_id`(`RxMeta.prev_override`);档内 `sequence ≤ snap_id` 的变更跳过 | 同上 | — |

契约见 [hft-venue-proto](../hft-venue-proto/README.md#实现方必须遵守的契约)。

### 累计量与费用来源

| 流 | `cum_filled` | `avg_px` | 费用 |
|---|---|---|---|
| 现货 `/spotMarket/tradeOrdersV2` `type=match` | `filledSize`(订单级累计) | 累计成交额 / `filledSize`(字段待核实;无则 `GwCtx` 按 `(inst, tradeId)` 去重合成,[契约 22](../hft-venue-proto/README.md#实现方必须遵守的契约)) | 推送不含费用时由对账补(待核实) |
| 期货 `/contractMarket/tradeOrders` `type=match` | `filledSize × multiplier` | 同上 | 同上 |

错误码两张表在 `codes.rs`:`200000` 成功;登记的参数/余额类 → `Reject`;系统繁忙/超时类(确切码待核实)与任何未登记码 → `outcome_unknown`(契约 8)。

### VenueCaps 默认值

`default_caps()`([hft-venue-proto 契约 15](../hft-venue-proto/README.md#实现方必须遵守的契约));optional,除协议字段外待核实,接入时按 [必备节](../hft-venue-proto/README.md#每个-venue-readme-的必备节) 写全其余各节。

| 字段 | `KucoinFutures` | `KucoinSpot` | 来源 |
|---|---|---|---|
| `fee_asset_rule` | `Quote`(USDT) | 待核实(计价币或收到的币;KCS 抵扣时 `ReceivedAssetUnlessDiscount{KCS}` 一类) | 待核实 |
| `reduce_only` | 是 | 否 | 待核实 |
| `required_position_mode` / `required_margin_mode` | 单向 / 全仓(字段待核实) | 不适用 | 待核实 |
| `required_spot_borrow` | `None`(永续) | `None`:现货下单走现货(HF)账户,该账户不能借币;核验项 = 账户类型(杠杆账户 → `ACCT_TYPE`);现货账户是否存在自动借币类开关待核实,存在则改 `Some(false)` | 待核实 |
| `funding_settle` | `PerSymbolPush`(私有持仓频道的资金费结算推送带 symbol,待核实) | 不适用 | 待核实 |
| `margin_data` | `Poll{interval_ms: 5000}`(若持仓/账户推送足够则改 `Push`) | 不适用 | 待核实 |
| `own_liquidation_marker` | 成交推送的成交类型为强平 / ADL / 结算(字段与取值待核实) | 无 | 待核实 |
| `ignorable_user_events` | 空(接入时补) | 空 | — |
| `amend` / `post_only` / `ioc` / `batch_cancel` | 待核实(先撤后下)/ 是 / 是 / 待核实 | 同左 | 待核实 |
| `dms` | DCP(REST,待核实;核实前无) | 同左 | 待核实 |
| `testnet` | 待核实(`None` 直到核实) | 同左 | 待核实 |

### `mock` 特性

与 [venue-binance](../venue-binance/README.md#mock-特性给-mock-exchange-的反向编解码) 相同的四个反向编解码函数(`encode_market`、`decode_order_frame`、`encode_order_event`、`encode_rest_response`),接入时一并交付,供 [mock-exchange](../../apps/mock-exchange/README.md) 使用。

## 实现说明

<details><summary>实现要点</summary>

1. **心跳**:`HeartbeatSpec` 由 welcome/bullet 响应的 `pingInterval=18000`、`pingTimeout=10000` 决定(协议常量取服务端下发值,缺省 18 s/10 s);行情连接发 `{"id":"<n>","type":"ping"}`,下单连接发 `{"id":"<n>","op":"ping","timestamp":"<ms>"}`,回 `pong`。现有 cex-arb 固定 15 s 且不读服务端值,新实现以服务端值为准。
2. **分派**:行情 `{"type":"message","topic":"/market/level2:` …;`welcome`、`ack`、`pong`、`error` 为控制类。下单连接以 `{"id":` 开头为响应。
3. **账户**:现货下单要求 hf 高频账户(需 API 划转到 `trade_hf` 激活,社区实测往返显著更低);refdata 标注账户类型,未激活拒绝启动该 venue。
4. **回报**:响应 `code="200000"` → `Ack`;其他 → `Reject`。私有推送(`/spotMarket/tradeOrdersV2`、期货 `/contractMarket/tradeOrders`)的 `type ∈ {open, match, filled, canceled, update}` → `Ack/PartialFill/Fill/CancelAck/AmendAck`;`userRateLimit` → `RateObs`。
5. **死手**:REST hf 的 DCP(dead-man switch,`timeout` 秒 + 可选 symbol 列表)→ `DeadManSpec::CountdownAccount{via: Rest}`;端点与参数待核实,核实前按 `None` + kill-switch。
6. **源文件**:`lib.rs`、`bullet.rs`(引导与服务器选择)、`cid.rs`、`sign.rs`(URL 签名、会话挑战)、`md.rs`、`snapshot.rs`、`orders.rs`、`reports.rs`、`codes.rs`。

</details>

## 延迟与资源契约

| 入口 | p50 / p99 预算 | 基准名 |
|---|---|---|
| `classify` | ≤ 30 ns / ≤ 60 ns | `bench_classify_kucoin` |
| `decode_md` level1 → `Bbo` | ≤ 0.4 µs / ≤ 0.8 µs | `bench_decode_kucoin_l1` |
| `decode_md` level2 增量 | ≤ 1 µs / ≤ 2 µs | `bench_decode_kucoin_l2` |
| `encode` spot.order | ≤ 0.3 µs / ≤ 0.6 µs | `bench_encode_kucoin_order` |
| `decode_report` 订单推送 | ≤ 1 µs / ≤ 2 µs | `bench_decode_kucoin_order` |

纯函数,热路径零分配零系统调用。`bootstrap`、`login`(含挑战应答)在 warm 线程。

## 测试与验收

**金样**(`fixtures/kucoin/`):bullet-public 响应、welcome、level1、level2 增量与快照(对齐四种情况)、match、期货 level2/tickerV2/execution、Pro WS 会话握手全过程(会话消息 → 应答 → welcome)、spot.order/futures.order 成功与错误、`allowMaxTimeWindow` 超窗拒单、订单推送全状态。

**属性**:clientOid 往返;level2 档内 `sequence ≤ L` 的跳过逻辑与参考实现一致;握手应答签名与现有 cex-arb 实现逐字节一致。

**fuzz**:`fuzz_kucoin_md`、`fuzz_kucoin_report`、`fuzz_kucoin_encode_roundtrip`。

**验收**:金样全绿;fuzz 每个目标累计 ≥ 2 h 零 panic(跨多夜累计,任一失败清零,[testing §8](../../docs/testing.md#8-上线门槛) 门槛 ③);testkit 假交易所 + 测试网连续 3 天冒烟([testing §8](../../docs/testing.md#8-上线门槛))。

## 依赖

`hft-types`、`hft-refdata`、`hft-event`、`hft-venue-proto`、`sonic-rs 0.5`、`hmac 0.12` + `sha2 0.10`、`base64 0.22`、`percent-encoding 2`(warm 线程)。**禁止**:`hft-net`、`hft-venue`、`tokio`、`serde_json`(dev-only)。

## 延伸阅读

- [research/02 §7 KuCoin](../../design/research/02-exchange-connectivity.md)、[04 §5](../../design/04-行情接入层.md)、[05 §7](../../design/05-CEX执行层.md)
- [hft-venue](../hft-venue/README.md)(`bootstrap`、`LoginStep::Handshake`)、[cookbook/adding-a-venue.md](../../docs/cookbook/adding-a-venue.md)
- 现有知识来源:旧套利引擎 Rust 交易模块 `cex-arb/src/exchange/kc_ws_trader.rs`(位于 `E:\trading\Trading\` 下;URL 签名与会话握手)、`feed.rs`(bullet 流程、ticker `time` 的坑)

## 已知限制与后续工作

- **UTA 接口**(`uta.order`、`x-push-*` 主机)未纳入;账户升级 UTA 后新增类型。
- **DMA/co-location**:MM 计划达标后申请,获批后只改端点配置与重测延迟。
- **改单**:未确认 WS 改单能力,先用撤 + 新下代替。

### 开发备忘

<details><summary>给实现者</summary>

**待核实**

1. `level1` 是否带单调序号字段与推送时间字段;否则 BBO 去重只能按价量变化。
2. `clientOid` 长度上限与字符集。
3. KuCoin 签名时间窗口(历史 5 s,调研未找到明示)。
4. DCP 端点、参数与最小 timeout;是否有 WS 形式。
5. Pro WS 下单连接的 ping 规则是否与行情相同(18 s)。
6. `x-push-spot/futures` 与 `wsapi` 主机的前置(CloudFront/直连)。

</details>
