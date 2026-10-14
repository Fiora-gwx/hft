---
description: "Binance USDT-M 永续与现货的纯协议实现(core:USDT-M 与现货 JSON;现货 SBE 行情为 optional):行情解码、快照对齐、ws-fapi/ws-api 下单帧与回报解码、账户配置核验、保证金/ADL 轮询、外部成交识别与 VenueCaps 默认值"
kind: crate
tier: pure
scope: core
status: impl
---

# venue-binance

## 概要

`venue-binance` 实现两个 [`VenueProtocol`](../hft-venue-proto/README.md#venueprotocol静态分派):`Usdm`(USDT 本位永续,`MarketKind::LinearPerp`)与 `Spot`(现货,`MarketKind::Spot`)。**core 范围** = `Usdm` 全部 + `Spot` 的 JSON 行情与 ws-api 下单/用户流;**现货 SBE 行情是 optional**(feature `spot-sbe`,触发条件:需要现货 µs 级交易所时间戳或 20 ms 深度,且对照测试证明解析收益)。它拥有 Binance 的全部协议知识——行情频道与路由、`E`/`T` 时间戳语义、分档快照流(core 深度)、增量 depth 的 `pu` 链续与快照对齐(可选扩展)、`session.logon` 登录帧、`order.place/modify/cancel/cancelReplace/amend.keepPriority` 模板、回报与用户数据流解码(含可忽略事件)、账户配置与保证金/ADL 查询的请求与解码、自身强平/ADL 成交的识别、`rateLimits` 闭环字段、`countdownCancelAll` 死手、client id 外形、官方限速默认值与 `VenueCaps` 默认值。它是纯函数:不碰 socket、不创建线程、不写 journal;实盘由 [`hft-venue`](../hft-venue/README.md) 的 feed/gw 线程调用,同一份代码给 mock-exchange、raw 重放、金样测试与 fuzz 使用。一个账户在 Binance 只启用 `Usdm` 或 `Spot` 之一(refdata `SameExchangeMarkets`,[hft-refdata](../hft-refdata/README.md));两者在交易所侧本来就是独立钱包,`Usdm` 账户的身份与 key 权限查询仍走现货 REST 主机(见"账户配置核验")。标"待核实"的交易所行为在本机无法查官方文档,核实方法见开发备忘。

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
pub struct Usdm;   // impl VenueProtocol for Usdm  —— NAME = "binance-usdm", VENUE = BINANCE_USDM, MARKET = LinearPerp
pub struct Spot;   // impl VenueProtocol for Spot  —— NAME = "binance-spot", VENUE = BINANCE_SPOT, MARKET = Spot

pub mod usdm { pub enum MdClass { BookTicker, DepthTopN /* 分档快照 */, DepthUpdate /* 增量,可选扩展 */, AggTrade, MarkPrice, ForceOrder,
                                  SubAck, ServerShutdown, Unknown }
               pub enum ReportClass { WsResp, OrderTradeUpdate, AccountUpdate, MarginCall, AccountConfigUpdate, ListenKeyExpired,
                                      Control /* logon、订阅类响应 */, Ignorable, Unknown } }
pub mod spot { pub enum MdClass { JsonBookTicker, JsonDepthTopN /* combined 流分档快照 */, JsonDepth /* 增量,可选扩展 */, JsonTrade, SubAck, Unknown,
                                  #[cfg(feature = "spot-sbe")] SbeBestBidAsk, #[cfg(feature = "spot-sbe")] SbeTrade,
                                  #[cfg(feature = "spot-sbe")] SbeDepthDiff, #[cfg(feature = "spot-sbe")] SbeDepth20 }
               pub enum ReportClass { WsResp, ExecutionReport, OutboundAccountPosition, EventStreamTerminated,
                                      Control, Ignorable, Unknown } }

// client id(`newClientOrderId`):`render_cid(&ClientOrderId, &mut [u8; 48])` / `parse_cid(&[u8]) -> Option<ClientOrderId>`(VenueProtocol 方法)。
// 编码是 hft-types 的 ClientOrderId(规范 24 位十六进制),本 crate 只加外形:<24hex>,24 字节(≤ 36,字符集 [.A-Za-z:/0-9_-] 满足)。
```

### 端点与路由

| 用途 | 端点 | 说明 |
|---|---|---|
| USDT-M BBO | `wss://fstream.binance.com/public/ws` | 只订 `@bookTicker`;路由 `public-bbo`(`RouteKind::Bbo`,`race=true`:两条同订阅,按 `u` 去重,事件进 bbo 环;由 `feed-bbo` 任务承载,其余路由由 `feed-md` 承载,[hft-venue §1](../hft-venue/README.md#1-线程与环由-hft-core-装配)) |
| USDT-M 分档快照 | `wss://fstream.binance.com/public/ws`(另一条连接;是否在 `/public` 待核实,开发备忘第 20 条) | `@depth<N>@100ms`(N ∈ 5/10/20);路由 `public-topn`(`RouteKind::DepthTopN`,单路;见"分档快照流") |
| USDT-M 增量深度(可选扩展) | 同上 | `@depth@100ms`;路由 `public-depth`(`RouteKind::Md`,单路);只在 `feed.depth = incremental` 时订阅 |
| USDT-M 行情(其余) | `wss://fstream.binance.com/market/ws` | `@aggTrade`、`@markPrice@1s`、`@forceOrder`;**只在 `/market` 推送**(无路由的旧 URL 只收 `/public` 内容,现有采集系统 v0.3.7 事故已验证);路由 `market`,`race=false` |
| USDT-M 用户数据流 | `wss://fstream.binance.com/private/ws/<listenKey>` | listenKey 由 REST 取得,每 30 min 由维护类 `GwOp::SessionKeepalive` 续期;**成交回报唯一来源**;它是该账户私有频道的归属连接([hft-venue §4b](../hft-venue/README.md#4b-私有频道归属与热备连接)) |
| USDT-M 下单 | `wss://ws-fapi.binance.com/ws-fapi/v1` | WS API;`session.logon` 后免签;主备两条,热备不收用户流 |
| USDT-M REST | `https://fapi.binance.com` | BBO 补帧、(增量深度时的)快照、listenKey、`countdownCancelAll`、查单、保证金与配置查询;签名用**独立的 HMAC key**,Ed25519 key 只给 ws-fapi 会话 |
| 现货 JSON 行情(core) | `wss://stream.binance.com:9443/ws?timeUnit=MICROSECOND` | `@bookTicker`(路由 `spot-bbo`,`race=true`)、`@trade`(路由 `spot-md`,`RouteKind::Md`,`race=false`);增量深度 `@depth@100ms` 只在 `feed.depth = incremental` 时订阅(同在 `spot-md`) |
| 现货分档快照(core) | `wss://stream.binance.com:9443/stream?timeUnit=MICROSECOND`(combined 流:推送体不带 symbol,靠外层 `stream` 名取 inst;待核实,开发备忘第 20 条) | `@depth<N>@100ms`(N ∈ 5/10/20);路由 `spot-topn`(`RouteKind::DepthTopN`) |
| 现货 SBE 行情(optional) | `wss://stream-sbe.binance.com:9443/ws` | Upgrade 头 `X-MBX-APIKEY: <Ed25519 key>`(无签名) |
| 现货下单 + 用户流 | `wss://ws-api.binance.com:443/ws-api/v3?timeUnit=MICROSECOND` | `session.logon` 后免签;用户数据流在**归属连接**(primary)上以 `userDataStream.subscribe` 订阅([官方,本地缓存]);没有独立的用户流连接,`endpoint(Role::UserStream, …)` → `ProtoErr::Unsupported` |
| 现货 REST | `https://api1.binance.com` … `api4` | **不用 `api.binance.com`(CloudFront)**([research/02 §0.7](../../design/research/02-exchange-connectivity.md));快照、查单、账户配置查询;`Usdm` 的 Config 序列也用它查 UID 与 key 权限(`RestReqBuf.endpoint` 指向此端点) |

`FAPI-MM`/`FSTREAM-MM` 主机只对 Futures Liquidity Provider 计划成员开放;获批后只改 `endpoints[]` 配置,协议不变。

**REST 端点表**(下标 = `RestReqBuf.endpoint`,规则见 [hft-venue §4a](../hft-venue/README.md#4a-账户查询保证金轮询与配置核验)):

| 产品线 | 下标 | 用途 | 默认主机 |
|---|---|---|---|
| `Usdm` | 0 | 主端点:BBO 补帧、(增量深度时的)快照、listenKey、`countdownCancelAll`、查单、保证金与配置查询 | `https://fapi.binance.com` |
| `Usdm` | 1 | 身份与 key 权限(`/api/v3/account`、`/sapi/v1/account/apiRestrictions`) | `https://api1.binance.com` |
| `Spot` | 0 | 主端点:快照、BBO 补帧、查单、账户配置查询 | `https://api1.binance.com` |

### 行情频道 → 事件

| 频道(`Usdm`) | 规则 | 事件 | 时间戳 |
|---|---|---|---|
| `<s>@bookTicker` | `Monotonic` on `u` | `Bbo{update_id=u}` | `ex_ts_ns ← T`,`ex_send_ns ← E` |
| `<s>@depth<N>@100ms`(分档快照) | `Snapshot` on `u` | `L2Snapshot{origin = TopN, snap_id = u}`(前 N 档整替换) | `ex_ts_ns ← T`,`ex_send_ns ← E` |
| `<s>@depth@100ms`(增量深度,可选扩展) | `Chain` | 一条 `L2Batch{prev_id = pu, last_id = u}`(快照后首条 `prev_id = snap_id`) | 同上 |
| REST `GET /fapi/v1/depth?symbol=&limit=1000`(增量深度,可选扩展) | 快照 | `L2Snapshot{origin = Exchange, snap_id = lastUpdateId}` | `ex_ts_ns ← T`(响应体),`ex_send_ns ← E` |
| `<s>@aggTrade` | `Monotonic` on `a` | `Trade{trade_id=a, taker_side = m ? Sell : Buy}` | `ex_ts_ns ← T`,`ex_send_ns ← E` |
| `<s>@markPrice@1s` | `ByTime` on `E` | `MarkPrice{p}`、`IndexPrice{i}`、`Funding{r, next=T}`(实际结算周期取相邻 `T` 之差,周期变化即反映在 `Funding` 里;`FeedCtx` 按 inst 记上一个不同的 `T` 与最近一次的差:该 inst 还没见过 `T` 变化时 `interval_s = 0`,之后为最近一次变化的差(秒)) | `ex_send_ns ← E`(`ex_ts_ns = 0`) |
| `<s>@forceOrder` | `None` | `Liquidation{px ← o.ap, qty ← o.z, side ← o.S}`(公共强平流,与本账户无关) | `ex_ts_ns ← o.T`,`ex_send_ns ← E` |

| 频道(`Spot`) | 规则 | 事件 | 时间戳 |
|---|---|---|---|
| JSON `<s>@bookTicker`(core) | `Monotonic` on `u` | `Bbo` | **无时间戳字段**,`ex_ts_ns = 0` |
| JSON `<s>@depth<N>@100ms`(core,分档快照) | `Snapshot` on `lastUpdateId` | `L2Snapshot{origin = TopN, snap_id = lastUpdateId}` | **无时间戳字段**,`ex_ts_ns = 0` |
| JSON `<s>@trade`(core) | `Monotonic` on `t` | `Trade{trade_id ← t, taker_side = m ? Sell : Buy}` | `ex_ts_ns ← T`,`ex_send_ns ← E`(µs) |
| JSON `<s>@depth@100ms`(增量深度,可选扩展) | `Chain` | 一条 `L2Batch{prev_id = U − 1, last_id = u}` | `E`(µs) |
| REST `GET /api/v3/depth?limit=1000`(增量深度,可选扩展;档数见开发备忘第 21 条) | 快照 | `L2Snapshot{origin = Exchange}` | 无交易所时间 |
| SBE `<s>@bestBidAsk`(optional) | `Monotonic` on `bookUpdateId` | `Bbo` | `eventTime`(µs)→ `ex_ts_ns` |
| SBE `<s>@trade`(optional) | `Monotonic` on `tradeId` | `Trade` | `eventTime`/`transactTime`(µs) |
| SBE `<s>@depth`(20 ms diff,optional) | `Chain` | 一条 `L2Batch{prev_id = U − 1, last_id = u}` | µs |

`E` 是网关推送时间,`T` 是撮合时间;单向延迟以 `T` 为锚([04 §5](../../design/04-行情接入层.md))。映射遵循 [event-model 时间戳语义](../../docs/subsystems/event-model.md#时间戳语义)。

`peek` 的 `ids`:`Chain` 类见"L2 序号归一化";其余类 `prev_id = 0`,`last_id` = 上表"规则"列的序号(`u`、`a`、`t`、`lastUpdateId`);`ByTime` 类取 `E` 换算的 ns;`None` 类为 0。

### 下单:WS API

| 操作 | `Usdm`(ws-fapi) | `Spot`(ws-api) |
|---|---|---|
| 登录 | `session.logon`:`params{apiKey, timestamp, signature}`,签名 = Ed25519(`apiKey=<k>&timestamp=<ms>`),base64 | 同左 |
| 下单 | `order.place`:`symbol, side, type=LIMIT, timeInForce ∈ {GTC, IOC, FOK, GTX}, price, quantity, newClientOrderId, reduceOnly, newOrderRespType=ACK, timestamp, recvWindow` | `order.place`:同上字段(无 `reduceOnly`)+ `selfTradePreventionMode=EXPIRE_MAKER`;`timeInForce` 无 GTX,post-only 用 `type=LIMIT_MAKER`(不带 `timeInForce`) |
| 改单 | `order.modify`(仅 LIMIT;**改单会重新排队**) | `order.amend.keepPriority`(**只能减量**,保留队列位置;`newQty ← GwCmd.qty`,帧中不含价格) |
| 撤改原子 | — | `order.cancelReplace`(`cancelReplaceMode=STOP_ON_FAILURE`):可选扩展,core 中不由任何 `GwOp` 产生(OMS 未建模"撤成功新单失败",[hft-oms 已知限制](../hft-oms/README.md#已知限制与后续工作)) |
| 撤单 | `order.cancel`(一律用 `origClientOrderId`,取自 `TargetRef.id`;不用 `orderId`) | 同左 |
| 全撤(`CancelAll`) | REST `DELETE /fapi/v1/allOpenOrders?symbol=`(`order_channel(CancelAll) = Rest`,`encode_rest`;`inst = 0` → `Unsupported(CancelAll)`,运行时按有活动单的 symbol 逐个调用) | `openOrders.cancelAll`(按 symbol;`inst = 0` → `Unsupported(CancelAll)`) |
| 查询 | `order.status`(`origClientOrderId`;响应 `executedQty` → `cum_filled`,`avgPrice` → `avg_px`) | `order.status`(`executedQty`、`cummulativeQuoteQty / executedQty` → `avg_px`) |
| 死手 | REST `POST /fapi/v1/countdownCancelAll{symbol, countdownTime}` | **无** → [kill-switch](../../apps/kill-switch/README.md) |
| 回报 | 下单响应(Ack/Reject)+ 用户数据流 `ORDER_TRADE_UPDATE` | 下单响应 + 归属连接上的 `executionReport` |

登录后请求**不带** `apiKey`/`signature`,但仍带 `timestamp`;`recvWindow` 由 `gw.stale_guard_ms` 收紧到 1500 ms([05 §7](../../design/05-CEX执行层.md))。请求 `id` 编码为 `"<op><slot 8 hex><gen 8 hex>"`(17 字符,十六进制小写),响应据此还原 `ReqRef`;op 字符:`p` = Place、`c` = Cancel、`m` = Amend、`x` = CancelAll、`q` = Query(还原为 `ReqRef`),`l` = `session.logon`、`u` = `userDataStream.subscribe/unsubscribe`(只作 `Control` 类,不还原);Cancel/Amend/Query 的 slot/gen 取目标槽,CancelAll 为 0。其他首字符、长度不是 17 或含非小写十六进制 → 不是我们的请求(`ReportClass::Unknown`)。现货命令带 `REDUCE_ONLY` → `EncodeErr::Unsupported`([hft-venue-proto 契约 11](../hft-venue-proto/README.md#实现方必须遵守的契约))。

其余订单类 op 的 `order_channel` 为 `Ws`;两者 `CANCEL_ALL_SWEEP = false`。`encode_rest` 的签名同"账户配置核验"的签名 REST 规则;`DELETE` 响应经 `decode_rest`:`code = 200` 或 HTTP 2xx → 零事件(撤单回报来自用户流),错误码 → 零事件、`raw_code` 按契约 8、`ctl` 按"结果未知码表"。

`order.status` 的 `status` → `QueryResult` 的订单状态,写进 `OrderEvent.detail`([契约 28](../hft-venue-proto/README.md#实现方必须遵守的契约);码同 `ReconcileReport.order_status`):`NEW` → New(1)、`PARTIALLY_FILLED` → PartiallyFilled(2)、`FILLED` → Filled(3)、`CANCELED` → Cancelled(4)、`REJECTED` → Rejected(5)、`EXPIRED`/`EXPIRED_IN_MATCH` → Expired(6);错误 `-2013` → NotFound(7)。

### 限速默认值(`rate_rules()`,生效值以 refdata 为准)

| 范围 | `Usdm` | `Spot` |
|---|---|---|
| 账户 ORDERS | 300 / 10 s,1200 / min | 100 / 10 s,200 000 / 日 |
| IP REQUEST_WEIGHT | 2400 / min | 6000 / min |
| 行情连接入向消息 | 10 条 / s / 连接 | 5 条 / s / 连接 |
| 每连接流数 | 1024 | 1024 |
| 新连接 | 核实前按现货口径 300 次 / 5 min / IP(合约值待核实,开发备忘第 11 条) | 300 次 / 5 min / IP |
| 强制断开 | 24 h | 24 h |

来源 [research/02 §2.1](../../design/research/02-exchange-connectivity.md)。`rate_rules()` 按上表逐行给出(`LimitScope` 账户 ORDERS → `Account`,IP 类 → `Ip`,每连接 → `Conn`;"每连接流数"与"强制断开"不是限速规则,分别在 `RouteSpec.max_streams` 与 refdata `ws_max_conn_age_s`)。WS API 每个响应带 `rateLimits[]`(`rateLimitType, interval, intervalNum, limit, count`),`decode_report` 把它变成 `RateObs` 交给 gw 校正发送侧计数:`ORDERS` → `Orders`、`REQUEST_WEIGHT` → `Weight`、`RAW_REQUESTS` 不映射;`window_ms` = `interval`(`SECOND`/`MINUTE`/`DAY`)× `intervalNum`;多条时取 `count / limit` 最大的一条(相同取窗口最短者)。保证金与配置查询的 REST 权重见"保证金数据换算"与"账户配置核验"。

### 分档快照流

core 的深度来源([hft-venue-proto 契约 26](../hft-venue-proto/README.md#实现方必须遵守的契约));`RouteSpec.topn_levels = [5, 10, 20]`。

| 产品线 | 频道 | 档位 | 节拍 | 序号 / 时间 | 路由与端点 | 来源 |
|---|---|---|---|---|---|---|
| `Usdm` | `<s>@depth<N>@100ms`(Partial Book Depth) | 5 / 10 / 20 | 100 ms(另有 250 ms、500 ms 档,不用) | `u` 单调(`Snapshot`);`T` 撮合时间、`E` 推送时间 | `public-topn`,`/public` | 待核实(开发备忘第 20 条) |
| `Spot` | `<s>@depth<N>@100ms`(Partial Book Depth) | 5 / 10 / 20 | 100 ms | `lastUpdateId` 单调(`Snapshot`);无时间字段 | `spot-topn`,combined 流 `/stream`(推送体不带 symbol) | 待核实(开发备忘第 20 条) |

推送体的 `bids`/`asks` 是前 N 档(价、量字符串),按 refdata 线格式指数解析;空档位(缺失的档、`qty` 为 0 的档)不输出,`n_bids`/`n_asks` 是实际输出的档数。订阅后首帧即当前快照,不需要 REST(待核实,开发备忘第 20 条)。

### L2 序号归一化(`L2Batch.prev_id/last_id`)

增量深度(可选扩展)用;规格保留,`feed.depth = incremental` 时才生效。按 [hft-venue-proto 契约 1](../hft-venue-proto/README.md#实现方必须遵守的契约) 与 [hft-book](../hft-book/README.md),本 crate 在 `peek`/`decode_md` 里把交易所序号换算成"`prev_id` = 本消息作用于其上的簿状态 id":

| 流 | `prev_id` | `last_id` | 快照 `snap_id` |
|---|---|---|---|
| `Usdm` `@depth@100ms` | `pu` | `u` | REST `lastUpdateId` |
| `Spot` JSON `@depth@100ms` / SBE depth diff | `U − 1` | `u` | REST `lastUpdateId` |
| 快照后第一条(两者) | `snap_id`(运行时经 `RxMeta.prev_override` 传入) | `u` | — |

现货的"`U ≤ L+1 ≤ u` 重叠"只在 `align_first` 里判定;一旦 `Apply`,首条 `prev_id = snap_id`,之后 `U − 1` 恰好等于上一条 `u`,簿只做 `prev_id == last_id`。

### 累计量与费用来源

| 流 | `cum_filled` | `avg_px` | 费用 |
|---|---|---|---|
| `Usdm` `ORDER_TRADE_UPDATE` | `z`(累计成交量) | `ap` | `n`(本笔,增量)+ `N` 资产;不产出 `FeeOnly` |
| `Spot` `executionReport` | `z` | `Z / z`(累计成交额 / 累计量;`Px::from_notional`,`RoundMode::HalfEven`;`z = 0` → 0) | `n` + `N`;不产出 `FeeOnly` |
| 查询响应 `order.status` | `executedQty` | `Usdm` `avgPrice`;`Spot` `cummulativeQuoteQty / executedQty` | 不含 |
| 下单响应(`newOrderRespType=ACK`) | 不含成交 | — | — |

成交去重键:`(inst, t)`(`Usdm` 与 `Spot` 的 `t` 都是按交易对编号的 trade id,不同 symbol 可能撞号);`t` 缺失或 ≤ 0 时改用 `(inst, i, z)`(订单 id + 本笔之后的累计成交量);`GwCtx` 每账户一个 `TradeIdRing<1024>`([契约 22](../hft-venue-proto/README.md#实现方必须遵守的契约));资金费推送的键见"用户流事件表"。`Usdm` 与 `Spot` 的推送都直接给累计量 `z`,不需要合成。

### 结果未知码表

`src/errors.rs` 两张表([契约 8](../hft-venue-proto/README.md#实现方必须遵守的契约))。码的形态:数值码(多为负数),`reject_code`/`raw_code` 按契约 8 以 i32 补码存;只有 HTTP 状态时按契约 8 的状态码编码。

| 表 | 码 | 来源 |
|---|---|---|
| 确定拒绝 | `-1013`(过滤器)、`-1021`(时间戳超出 recvWindow)、`-1111`(精度)、`-2010`(现货新单被拒,含余额不足)、`-2019`(保证金不足)、`-2022`(reduceOnly 被拒)、`-4164`(低于最小名义)、`-5022`(GTX 会吃单)、`-4061`(持仓方向与模式不符)、`-1003`/`-1015`(限速,请求未处理)、HTTP 429/418 | research/02 与现有系统实测;逐码待核实(方法 A + B:官方错误码页原文与演示环境样本,随金样登记) |
| 结果未知 | `-1007`(超时,状态未知)、`-1001`(内部错误/连接断开)、`-1008`(服务过载)、HTTP/WS `status ≥ 500`,以及**任何未登记码** | 现货 ws-api 文档明示"5XX 不代表失败,状态未知"[官方,本地缓存];期货待核实(方法 A) |

撤单响应 `-2011`(未知订单,常因已成交)→ `CancelReject{detail = GONE}`(属 `GONE` 类,契约 28;sequencer 等成交回报,不重试);其余确定拒绝码在撤改上 `detail = 0`。查询 `order.status` 返回 `-2013`(订单不存在)→ `QueryResult` 的 NotFound 形态,**不定终态**:`stale_guard()` 是 `RecvWindow`,按[契约 10](../hft-venue-proto/README.md#实现方必须遵守的契约) 由 OMS 保持 `Unknown` 并经成交历史(`Usdm` `GET /fapi/v1/userTrades`,`Spot` `myTrades`,按 symbol + 时间窗)确认收敛。**成交历史的记录只带 `orderId`、不带 client id**(待核实,开发备忘第 19 条),所以"这笔从未成交"不能按 client id 在历史里直接查到;判据归 [reconciler](../../apps/reconciler/README.md):成交历史窗口内出现既不属于已知槽、也不属于已登记外来单的 `orderId` → `history_checked = 0` 并报 P0;grace 后按 client id 查询(`order.status`,或 REST `GET /fapi/v1/order?origClientOrderId=`)连续两次仍 `-2013`、且窗口内没有未归属的成交,才给 `history_checked = 1`。控制提示([契约 30](../hft-venue-proto/README.md#实现方必须遵守的契约)):`-1021` → `ctl = ClockSkew`(运行时推 `ConnState{Degraded, reason=CLOCK_SKEW}`);`-1003`、`-1015`、HTTP 429/418 → `ctl = RateLimited`,`RateObs` 置满;`listenKey` 续期返回 `-1125`(listenKey 不存在)→ `ctl = UserStreamEnded`。

### VenueCaps 默认值

`default_caps()` 返回下表([hft-refdata](../hft-refdata/README.md) 的 `VenueCaps`;[契约 15](../hft-venue-proto/README.md#实现方必须遵守的契约))。来源等级:官方 = 官方文档(含本地缓存);自测 = 现有系统生产或我们的实测;待核实 = 未经官方确认,方法写在开发备忘对应条目。

| 字段 | `Usdm` | `Spot` | 来源 |
|---|---|---|---|
| `fee_asset_rule` | `Quote`(USDT 保证金资产;要求 BNB 抵扣合约手续费关闭,否则 `FEE_RULE`) | `ReceivedAssetUnlessDiscount{BNB}`(BNB 抵扣开启且 BNB 足 → 扣 BNB;否则扣成交所得资产) | 待核实(抵扣开关的端点与回退行为,开发备忘第 5、7 条) |
| `reduce_only` | 是 | 否 | 官方 |
| `required_position_mode` | 单向持仓(`dualSidePosition = false`) | 不适用 | 待核实(端点,开发备忘第 5 条) |
| `required_margin_mode` | 全仓(`marginType = CROSSED`,所有可交易 symbol);多资产模式关闭 | 不适用(现金账户) | 待核实(端点,开发备忘第 5 条) |
| `required_multi_asset` | `Some(false)`(多资产模式关闭) | `None` | 同上 |
| `required_spot_borrow` | `None`(永续) | `None`:现货钱包不能借币,由"账户类型 = `SPOT`"核验(否则 `ACCT_TYPE`,见"账户配置核验") | 官方(`accountType` 字段);现货钱包无自动借币开关待核实(开发备忘第 5 条) |
| `funding_settle` | `RateOnly`:行情估算 + 认证 income 历史实额完成 | 不适用 | [资金费实额](#资金费实额) |
| `margin_data` | `Poll{interval_ms: 5000}` | 不适用 | — |
| `own_liquidation_marker` | `LiqMarker::ClientIdPrefix`,`liq_marker_arg = "autoclose-\|adl_autoclose\|settlement_autoclose-"`:client id 前缀 `autoclose-`(强平)、`adl_autoclose`(ADL)、`settlement_autoclose-`(下架/交割结算);辅证 `o.o = LIQUIDATION`、`x = CALCULATED` | `LiqMarker::NotIdentifiable`,`""`(现金现货无强平) | 待核实(开发备忘第 2 条) |
| `ignorable_user_events` | `TRADE_LITE`、`STRATEGY_UPDATE`、`GRID_UPDATE`、`CONDITIONAL_ORDER_TRIGGER_REJECT` | `balanceUpdate`、`listStatus`、`externalLockUpdate` | 待核实(事件名全集,开发备忘第 4 条) |
| `amend` | 是(`order.modify`,重新排队) | 是(`order.amend.keepPriority`,只减量) | 官方 |
| `amend_qty_down_only` | 否 | 是(改单只能减量、不能改价;OMS 拒绝改价或增量的 Amend,规则归 [hft-oms](../hft-oms/README.md)) | 官方 |
| `post_only` | 是(`GTX`) | 是(`LIMIT_MAKER`) | 官方 |
| `ioc` | 是 | 是 | 官方 |
| `fok` | 是(`timeInForce = FOK`) | 是(`timeInForce = FOK`) | 官方 |
| `batch_cancel` | 是(按 symbol 全撤;批量撤单 REST ≤ 10 单待核实) | 是(`openOrders.cancelAll` 按 symbol) | 官方 / 待核实(方法 A) |
| `dms` | `CountdownPerInst`(`countdownCancelAll`,REST,按 symbol;时长取下方 `dead_man()`,`dms_max_ms = 0`) | `None` → kill-switch 兜底 | 待核实(端点名来自 research/02;语义、最小倒计时与权重见开发备忘第 10 条)/ — |
| `testnet` | `Demo`(Demo Trading,端点见"测试网/模拟盘") | `Demo`(同左) | 官方文档列出两者的 Demo 端点(原文待缓存);可用范围待核实(开发备忘第 14 条) |

"不适用"的字段取 hft-refdata 的 `NotApplicable`/`None` 变体(`funding_settle = NotApplicable`、`margin_data = NotApplicable`、`required_position_mode`/`required_margin_mode = NotApplicable`);能力位 `flags` 两者都是 `AMEND | POST_ONLY | IOC | FOK | BATCH_CANCEL`(31),不声明 `PERP_CASH`;结算来源与查询覆盖尚未接通,不能因通用能力全集扩展而自动启用。`fee_asset_rule` 里的平台币以资产名 `BNB` 交给 `default_caps` 的资产查找参数,解析为该版本 refdata 的 `AssetId`(查不到写 `ASSET_NONE`,[契约 15](../hft-venue-proto/README.md#实现方必须遵守的契约))。

`stale_guard()` 两者均为 `RecvWindow{default_ms: 5000}`(运行时以 `gw.stale_guard_ms` 收紧;查询"不存在"不定终态,见上节);`dead_man()`:`Usdm` = `CountdownPerInst{min_ms: 5000, timeout_ms: 30000, refresh_ms: 10000, via: Rest}`(`timeout_ms ≥ 2 × refresh_ms`,丢一次刷新不触发全撤),`Spot` = `None`。

### 账户配置核验

`acct_queries(Config)` 的请求序列与解码([契约 16](../hft-venue-proto/README.md#实现方必须遵守的契约))。读不出任何一项(HTTP 非 2xx、字段缺失或无法解析、refdata 中某个可交易 symbol 不在 `symbolConfig` 响应里)→ `UNREADABLE`;表中权重是 `AcctQuery.weight` 的取值(待核实的按开发备忘第 5 条核实后改)。签名 REST 请求的查询串 = 业务参数 + `timestamp`(`WallClock.unix_ms`)+ `recvWindow`(`gw.stale_guard_ms`),末尾追加 `signature`(HMAC-SHA256 小写十六进制;会话 key 的 Ed25519 签名为 base64 后 URL 编码),请求头 `X-MBX-APIKEY`,`redacted = true`。

| 产品线 | 步 | 请求(REST,HMAC 签名) | 字段 → 判定 | 来源 |
|---|---|---|---|---|
| `Usdm` | 0 | `GET /fapi/v1/positionSide/dual`(权重 30,待核实) | `dualSidePosition` → `position_mode`;≠ 要求 → `POS_MODE` | 待核实(开发备忘第 5 条) |
| `Usdm` | 1 | `GET /fapi/v1/multiAssetsMargin`(权重 30,待核实) | `multiAssetsMargin` → `multi_asset`;≠ 要求 → `MULTI_ASSET` | 待核实(开发备忘第 5 条) |
| `Usdm` | 2 | `GET /fapi/v1/symbolConfig`(权重 5,待核实) | 每个 refdata 可交易 symbol 的 `marginType`;任一 ≠ `CROSSED` → `MARGIN_MODE`;`margin_mode` 填全部 `CROSSED` → `Cross`、全部 `ISOLATED` → `Isolated`、两者都有 → `MarginMode::Mixed`;同一响应的 `leverage` → 每 symbol 一条 `LeverageItem` | 待核实(端点与 `leverage` 字段;备选 `positionRisk` 的 `marginType`/`leverage`;开发备忘第 5 条) |
| `Usdm` | 3 | `GET /fapi/v1/feeBurn`(权重 30,待核实) | `feeBurn = true` 而规则为 `Quote` → `FEE_RULE` | 待核实(开发备忘第 5、7 条) |
| `Usdm` | 4 | `GET /fapi/v3/account`(fapi,权重 5,待核实) | 请求被拒且错误码表示统一账户(Portfolio Margin)→ `ACCT_TYPE`;`canTrade = false` → `KEY_PERM` | 待核实(开发备忘第 5 条) |
| `Usdm`、`Spot` | 5 / 0 | `GET /api/v3/account`(现货端点,weight 20,HMAC REST key 签名) | `uid` ≠ `AccountMeta.venue_uid` → `UID`(`uid_keys |= REST`);`canTrade = false` → `KEY_PERM`;`Spot`:`accountType ≠ SPOT` → `ACCT_TYPE` | 官方(本地缓存:响应含 `uid`、`canTrade`、`permissions`、`accountType`) |
| `Usdm`、`Spot` | 6 / 1 | `GET /sapi/v1/account/apiRestrictions`(权重 1,待核实) | `enableWithdrawals = true` → `KEY_PERM`;`Usdm` 还要求 `enableFutures = true` | 待核实(开发备忘第 5 条) |
| `Spot` | 2 | `GET /sapi/v1/bnbBurn`(权重 1,待核实) | `spotBNBBurn` 与 `fee_asset_rule` 矛盾(规则为 `Quote` 类而开启,或规则要求抵扣而关闭)→ `FEE_RULE` | 待核实(开发备忘第 5、7 条) |

**会话 key 的 UID**([契约 23](../hft-venue-proto/README.md#实现方必须遵守的契约)):Ed25519 会话 key 与 HMAC REST key 是两把 key,`session_identity_request` 返回 `Ok(true)`:warm 线程登录 ws-fapi/ws-api 后,用 Ed25519 key 签名请求现货 REST `GET /api/v3/account`(现货 REST 接受 Ed25519 签名,官方),`decode_identity` 取 `uid`,`uid_keys |= SESSION`。只开合约权限的 Ed25519 key 能否读现货 `/api/v3/account` 待核实;不能时改用 ws-api `account.status`(响应是否带 `uid` 待核实)。两项见开发备忘第 16 条。

**现货借币**:`Spot` 账户下单走现货钱包,现货钱包不能借币(杠杆是独立的杠杆账户,本系统不在其上下单),所以 `AUTO_BORROW` 由"账户类型 = `SPOT`"这一项保证(上表步 0);统一账户(Portfolio Margin)→ `ACCT_TYPE`。现货钱包是否存在自动借币类设置待核实(方法 A)。

运行中触发重查(`ReportOutcome.hint = ConfigRecheck`):`ACCOUNT_CONFIG_UPDATE` 含 `ai`(多资产模式变化)或 `ac`(杠杆变化,重读杠杆条目)、`ACCOUNT_UPDATE m = MARGIN_TYPE_CHANGE`;这些推送本身不产出事件。

### 保证金数据换算

只对 `Usdm`([契约 17](../hft-venue-proto/README.md#实现方必须遵守的契约));`Spot` 的 Margin 序列为空。

| 步 | 请求 | 频率 | 权重 | 产出 |
|---|---|---|---|---|
| 0 | `GET /fapi/v3/account` | 每次 | 5(待核实,开发备忘第 6 条) | 账户级字段 |
| 1 | `GET /fapi/v3/positionRisk` | 每次 | 5(待核实,开发备忘第 6 条) | 每 inst 条目 + `PositionReport`(只校验);只保留 refdata 中的 symbol |
| 2 | `GET /fapi/v1/adlQuantile` | 每 6 次(`every = 6`,约 30 s;交易所约 30 s 更新一次,待核实,开发备忘第 6 条) | 5(待核实) | ADL 档 |

合计约 (5 + 5) × 12 + 5 × 2 = 130 weight/min,占 IP 配额 2400/min 的 5.4%。

| `MarginReport` 字段 | 来源字段 | 换算 |
|---|---|---|
| `margin_balance` | `totalMarginBalance` | USDT(多资产模式下为交易所折算的 USD 值) |
| `maint_margin` | `totalMaintMargin` | — |
| `avail_margin` | `availableBalance` | — |
| `margin_ratio` | `totalMaintMargin / totalMarginBalance` | `Rate`(i128 计算 `maint × 10⁹ / balance`,向零取整);分母 ≤ 0 → `Rate::MAX`;交易所给出的比率字段不采用 |
| 条目 `liq_px` | `positionRisk[].liquidationPrice` | `"0"` → 0(无强平价) |
| 条目 `adl_quantile` | `adlQuantile[].adlQuantile.BOTH`(单向持仓;0..4) | × 25 → 0..=100;未取到的轮次沿用上一次的值(存在 `AcctCtx`,[契约 27](../hft-venue-proto/README.md#实现方必须遵守的契约));该 inst 从未取到过 → 255 |

每 inst 一条 `PositionReport{qty ← positionAmt, entry_px ← entryPrice, upnl ← unRealizedProfit, liq_px ← liquidationPrice}`(只校验)。用户流 `MARGIN_CALL` → `hint = MarginNow`(立即轮询一次,不直接产出事件)。若核实 `positionRisk` v3 已带 ADL 档字段,删除第 2 步。

### 外部成交识别

`own_liquidation_marker` 的实现([契约 18](../hft-venue-proto/README.md#实现方必须遵守的契约))。成交类消息:`Usdm` `ORDER_TRADE_UPDATE` 且 `x ∈ {TRADE, CALCULATED}`;`Spot` `executionReport` 且 `x = TRADE`。我方订单的成交推送**必带** client id(`c` 字段恒在,待核实,方法 A + B),所以 client id 不是我们的格式即可判为外部成交。

| 判定(按顺序) | `cause` | 来源 |
|---|---|---|
| `Usdm`:`o.c` 以 `autoclose-` 开头,或 `o.o = LIQUIDATION` 且不满足下两行 | `Liquidation` | 待核实(开发备忘第 2 条) |
| `Usdm`:`o.c` 以 `adl_autoclose` 开头 | `Adl` | 待核实(开发备忘第 2 条) |
| `Usdm`:`o.c` 以 `settlement_autoclose-` 开头 | `Delivery`(下架/交割结算) | 待核实(开发备忘第 2 条) |
| `parse_cid(c)` 成功 | 本系统订单 → `OrderEvent` | — |
| `c` 以 `web_`、`ios_`、`and_` 开头 | `Manual` | 待核实(开发备忘第 3 条) |
| 其余 | `Unknown` | — |

`ExternalFill` 字段:`side ← S`、`qty ← l`、`px ← L`、`fee ← n`、`fee_asset ← N`、`exch_order_id ← i`、`trade_id ← t`(`t ≤ 0` 填 0)、`ex_ts_ns ← T`;去重键 `(inst, t)`,`t ≤ 0` 时 `(inst, i, z)`。强平单的清算费若以独立的余额变动出现(`ACCOUNT_UPDATE m = INSURANCE_CLEAR` 等),只作余额校验,不并入 `ExternalFill`(待核实其字段后再定,开发备忘第 2 条)。

### 用户流事件表

| 事件(`Usdm`) | 产出 | 提示 |
|---|---|---|
| `ORDER_TRADE_UPDATE` | `OrderEvent`(见"回报解码")或 `ExternalFill` | — |
| `ACCOUNT_UPDATE`,`m ≠ FUNDING_FEE` | 每个 refdata 中的 `P[]` 一条 `PositionReport{inst ← s, qty ← pa, entry_px ← ep, upnl ← up, liq_px = 0, ex_ts_ns ← T}`,每个 `B[]` 一条 `BalanceReport{asset ← a, free ← wb, locked = 0, ex_ts_ns ← T}`(只校验,不驱动仓位;refdata 之外的 symbol/资产跳过) | `m = MARGIN_TYPE_CHANGE` → `ConfigRecheck`;`m ∈ {INSURANCE_CLEAR, ADMIN_DEPOSIT, ADMIN_WITHDRAW}` → `MarginNow` |
| `ACCOUNT_UPDATE`,`m = FUNDING_FEE` | 可选 BalanceOnlyPush 配置(默认 RateOnly 不由此记账)。全仓:`FundingSettle{inst: INST_NONE, amount ← B[a = USDT].bc, settle_ts_ns ← T, synth: 0}`(`B[]` 里出现 USDT 以外的资产 → 计数告警,不入 `amount`);若带 `P`(逐仓)则 `FundingSettle{inst}`。去重键([契约 22](../hft-venue-proto/README.md#实现方必须遵守的契约)):`TradeKey::Funding{settle_ts_ns ← T, disc ← B[a = USDT].wb 的 raw 定点}`(`T` 是否等于结算时刻、同一结算时刻是否按 symbol 分多条推送,待核实,开发备忘第 1 条) | — |
| `MARGIN_CALL` | 无 | `MarginNow` |
| `ACCOUNT_CONFIG_UPDATE` | 无 | 含 `ai` 或 `ac` → `ConfigRecheck` |
| `listenKeyExpired` | 无;`ctl = UserStreamEnded`(运行时推 `ConnState{Down, role=UserStream}`) | 运行时重建用户流连接(`bootstrap` 取新 key) |
| `TRADE_LITE`、`STRATEGY_UPDATE`、`GRID_UPDATE`、`CONDITIONAL_ORDER_TRIGGER_REJECT` | 无(`skip = Listed`) | — |
| 其他 | 无(`skip = Unknown`) | — |

| 事件(`Spot`) | 产出 | 提示 |
|---|---|---|
| `executionReport` | `OrderEvent` 或 `ExternalFill` | — |
| `outboundAccountPosition` | 每个 refdata 资产一条 `BalanceReport`(`free ← f`,`locked ← l`,`ex_ts_ns ← u`) | — |
| `eventStreamTerminated` | 无;`ctl = UserStreamEnded`(运行时推 `ConnState{Down, role=UserStream}`) | 运行时在归属连接上重新 `userDataStream.subscribe` |
| `balanceUpdate`、`listStatus`、`externalLockUpdate` | 无(`skip = Listed`;`balanceUpdate` 之后的 `outboundAccountPosition` 给出绝对余额) | — |
| 其他 | 无(`skip = Unknown`) | — |

资金费由行情估算与[认证历史实额](#资金费实额)完成入账。默认 `RateOnly` 不把 `ACCOUNT_UPDATE m=FUNDING_FEE` 的账户总额再次记入钱包;用户流资金费解码属于可选 `BalanceOnlyPush` 配置,不得与历史实额通道同时启用。

### 资金费实额

`Usdm` 实现 `FundingHistoryProtocol`:HMAC-SHA256 签名的 `GET /fapi/v1/income?incomeType=FUNDING_FEE`,明确 `startTime/endTime/page/limit=100/timestamp`;仅接受 `FUNDING_FEE` + `USDT`,按 `symbol` 找 inst,`tranId` 为账单键,`time` 为毫秒,`income` 为有符号实额。满页继续请求下一页,不足一页才结束。未知合约、数值错误或非 200 状态拒绝整页。请求与 headers 标记为敏感,不可写原文日志。接口参数来源为 [官方连接器](https://github.com/binance/binance-futures-connector-python/blob/main/binance/um_futures/account.py)。

`markPriceUpdate` 解码出 MarkPrice、IndexPrice、Funding;费率与下次结算时间供策略只读视图使用,不代表资金费已到账。周期由同合约相邻不同 `T` 的差得出,首次与倒退时未知(0),表容量 256,超容量保持未知。轮询的原始账单由 [FundingPoll](../hft-venue/README.md#资金费账单轮询) 完成去重、汇总和 journal 入口。

### 测试网/模拟盘

Binance 的测试环境用 **Demo Trading**(演示环境);旧测试网(`testnet.binance.vision`、`testnet.binancefuture.com`)的鉴权据第三方报告已不可用,不用。下表端点取自官方文档(Spot "Demo Mode" 与 USDⓈ-M "General Info"),原文待缓存,可用范围待核实(开发备忘第 14 条)。

| 产品线 | 端点 | 用途与限制 |
|---|---|---|
| `Usdm` | REST `https://demo-fapi.binance.com`;行情与用户流 `wss://demo-fstream.binance.com`(`/public`、`/market`、`/private` 路由拆分在演示环境是否存在,待核实);WS API(下单,`ws-fapi` 的演示端点)待核实,**M3 之前必须核实**(开发备忘第 14 条) | 协议冒烟(登录、下单、撤单、用户流、配置查询);演示环境行情与主网盘口的关系待核实,不用于模拟账户;强平/ADL 难以在演示环境制造 |
| `Spot` | REST `https://demo-api.binance.com`;WS API `wss://demo-ws-api.binance.com/ws-api/v3`;行情 `wss://demo-stream.binance.com/ws` 与 combined `/stream`;SBE(optional)`wss://demo-stream-sbe.binance.com/ws` | 同上;`sapi` 类端点(bnbBurn、apiRestrictions)在演示环境是否存在待核实(开发备忘第 14 条),不存在时冒烟按"不可用"记录 |

**身份**:演示环境的 API key 在 Demo Trading 的 API 管理页创建;据第三方实测同一把 key 同时可用于现货与 USDⓈ-M,即两条产品线是同一个演示账户(待核实,开发备忘第 14 条)。`GET /api/v3/account` 在演示环境是否返回 `uid` 待核实;读不到 UID 的演示账户配置为 `uid_policy = "unverifiable"`(只允许 `is_prod = false`,适用条件、签字要求与 `IDENTITY` 的放行方式归 [hft-refdata](../hft-refdata/README.md#数据结构)),本 crate 的 Config 序列不因此改变,只是不核对 UID:该账户 `uid` 缺失或为空时 `UID`、`UNREADABLE` 都不因它置位,`uid_keys` 不含 `REST`,`venue_uid` 填返回值(缺失为全 0)(`ACCOUNT_CFG` 把 `UNREADABLE` 算作不符,否则演示账户永远过不了闸门)。`uid_policy = "verified"` 的账户缺 `uid` 照常置 `UNREADABLE`。演示环境的现货与 USDⓈ-M 是两个 `AccountId`(`SameExchangeMarkets`),两者都 `is_prod = false`、在同一交易所的不同 `VenueId`(`BINANCE_SPOT`、`BINANCE_USDM`)上登记同一个 UID:按 hft-refdata 的 `DuplicateVenueUid` 规则只记一条告警(经 `RefData::warnings()`,hft-core 启动时写 P1 `Note{RefdataWarning}`),不是加载错误;同一 `VenueId` 上重复仍是加载错误。

**演示环境 refdata**:symbol 名与主网相同,但可交易品种集合、tick/lot 与过滤器可能不同(待核实,开发备忘第 14 条);由 hft-cfg 按演示环境的 `exchangeInfo` 单独生成,不复用主网 refdata。

上线门槛第 ⑥ 项([testing §8](../../docs/testing.md#8-上线门槛))在演示环境连续 3 天冒烟,行情与下单都用演示环境(`feed_env = "testnet"` + 演示环境 refdata),避免主网价格与演示环境盘口错配。

### 心跳与 BBO 探测

按产品线与连接种类分别写([契约 24](../hft-venue-proto/README.md#实现方必须遵守的契约));各行数值来自不同文档,未逐条核实的标待核实(开发备忘第 18 条)。

| 连接 | 服务端 Ping | 无 Pong 断开 | 我们的动作 | 来源 |
|---|---|---|---|---|
| `Usdm` 行情 `fstream`(`/public`、`/market`)与用户流 `/private` | 每 3 min | 10 min | `hft-net` 自动回 Pong;BBO 连接另每 `feed.bbo_ping_ms` 发一次 WS Ping(`bbo_probe() = WsPing`);fstream 的 Pong 被批处理到约 50 ms,只用于存活,不进 RTT 直方图 | USDⓈ-M 文档口径([research/02 §1.4](../../design/research/02-exchange-connectivity.md)),待核实(方法 A) |
| `Usdm` 下单 `ws-fapi` | 待核实 | 待核实 | 回 Pong;往返用 `{"method":"ping"}` 测(p50 1.95 ms,自测) | 待核实(开发备忘第 18 条) |
| `Spot` 行情 `stream.binance.com` | 每 20 s | 1 min | 回 Pong;BBO 连接每 `feed.bbo_ping_ms` 发 WS Ping | 现货文档口径,当前值待核实(开发备忘第 18 条) |
| `Spot` 下单 + 用户流 `ws-api` | 每 20 s | 1 min | 回 Pong | 现货 WS API 文档口径,待核实(开发备忘第 18 条) |

- `heartbeat(role)`:四种连接都是 `HeartbeatSpec{ping: WsPing, interval_ms: 0(不发应用层 ping), unconditional: false, pong_timeout_ms: 10000, idle_timeout_ms: 服务端 Ping 间隔 × 2}`——`Usdm` 行情与用户流 360000;`Spot` 行情与 ws-api 40000;`Usdm` ws-fapi 核实前取 40000(下单连接每 5 s 有 RTT 探测的 Pong,不会因安静而超时);`app_ping` 写 0 字节(用 WS Ping)。
- 每秒一次的 Ping 与回帧计入行情连接的入向消息限额(`Usdm` 10 条/s、`Spot` 5 条/s;Ping/Pong 帧是否计数待核实(开发备忘第 18 条),核实前按计数处理):订阅帧按限额的一半再减 1 条/s 发送。
- BBO 补帧:`@bookTicker` 订阅后是否立即推当前 BBO 待核实(方法 B);核实前 `bbo_snapshot_request` 返回 `Ok(true)`:`Usdm` `GET /fapi/v1/ticker/bookTicker?symbol=`(权重 2;`BBO_SNAPSHOT_BATCH = 1`,`insts.len() > 1` → `ProtoErr::BufFull`;响应的 `lastUpdateId` 作 `update_id`(缺失为 0,字段待核实,开发备忘第 17 条),`time` → `ex_ts_ns`)、`Spot` `GET /api/v3/ticker/bookTicker?symbols=[…]`(`BBO_SNAPSHOT_BATCH = u16::MAX`;JSON 数组 URL 编码,如 `symbols=%5B%22BTCUSDT%22%2C%22BNBUSDT%22%5D`;放不进 `path_query` → `ProtoErr::BufFull`;无序号与时间,`update_id = 0`、`ex_ts_ns = 0`)。`decode_bbo_snapshot` 返回产出的 `Bbo` 条数;响应里不在传入 `insts` 中的 symbol 跳过;响应是错误体(`{"code":…}`)→ `DecodeErr::Shape`。

### 只读查询

Warm 入口支持 OpenOrders、Fills、Balance 与永续 Positions，整页校验后才调用 RoSink；错误页没有部分结果，外来 client id 保持 UNRESOLVED/None，不借用本账户槽。FundingFlow 暂由 `FundingHistoryProtocol` 的固定窗口接口承载，`ro_request(FundingFlow)` 返回 Unsupported。合成响应仅证明本地映射，真实接口权限与分页仍须测试网验证。


`ro_request`/`ro_decode` 的实现([契约 31](../hft-venue-proto/README.md#实现方必须遵守的契约));reconciler 以只读 HMAC key 构建 `Templates` 调用,gw 不用(`CANCEL_ALL_SWEEP = false`)。签名同"账户配置核验";成交分页按 [官方 USD-M 文档](https://developers.binance.com/docs/derivatives/usds-margined-futures/trade/rest-api/Account-Trade-List) 与 [Spot 文档](https://developers.binance.com/docs/binance-spot-api-docs/rest-api/account-endpoints) 生成请求。第一页用 startTime，后续只用 fromId；满页游标取最大 id + 1，空页终止。

| `RoKind` | `Usdm` | `Spot` | 字段映射 |
|---|---|---|---|
| `OpenOrders` | `GET /fapi/v1/openOrders`(不带 symbol = 全部) | `GET /api/v3/openOrders` | 每单 `open_order`:`clientOrderId` 经 `parse_cid` 定槽,`origQty`/`price`、`executedQty` → `cum_filled`,`status` → `detail` |
| `Fills` | `GET /fapi/v1/userTrades?symbol=&fromId=`(按 symbol;游标 = 最后一条 `id` + 1) | `GET /api/v3/myTrades?symbol=&fromId=` | 每笔 `fill`:`id` → `trade_id`,`orderId`,`qty`、`price`、`commission`/`commissionAsset`,`time`;**不带 client id**(`cid = None`);强平类 `cause` 无法从本接口判定(`None`,开发备忘第 2 条) |
| `FundingFlow` | `GET /fapi/v1/income?incomeType=FUNDING_FEE&startTime=`(游标 = 最后一条 `time` + 1) | `Unsupported` | 每笔 `funding`:`symbol` → inst,`income` → amount,`time` → `settle_ts_ns` |
| `Balance` | `GET /fapi/v3/account` | `GET /api/v3/account` | 每资产 `balance`(`free`/`locked`) |
| `Positions` | `GET /fapi/v3/positionRisk` | `Unsupported` | 每 inst `position`,映射同"保证金数据换算" |

### `mock` 特性(给 mock-exchange 的反向编解码)

[mock-exchange](../../apps/mock-exchange/README.md)(optional)不另写协议知识,而是调用本 crate 在 `features = ["mock"]` 下提供的纯函数(与正向解码共享字段映射;生产构建不启用):

```rust
#[cfg(feature = "mock")]
pub mod mock {
    pub fn encode_market(v: MockVenueKind, ev: &MockMarket<'_>, out: &mut Vec<u8>) -> MsgOp;         // Bbo/L2/Trade/Mark → 交易所报文
    pub fn decode_order_frame(v: MockVenueKind, payload: &[u8]) -> Result<MockOrderReq, DecodeErr>;  // 我们发出的订单帧 → 请求
    pub fn encode_order_event(v: MockVenueKind, ev: &MockOrderResp, out: &mut Vec<u8>) -> MsgOp;     // 响应、用户流推送、错误
    pub fn encode_rest_response(v: MockVenueKind, req: &MockRestReq<'_>, out: &mut Vec<u8>) -> u16;   // 快照、查单、撤单、账户查询;返回 HTTP 状态
}
```

## 实现说明

<details><summary>实现要点</summary>

### 1. 分派(`classify` / `classify_report`,只看前 64 B)

- `Usdm` 行情(非 combined 模式,订阅前不发 `SET_PROPERTY combined`):payload 以 `{"e":"` 开头,事件名紧随其后:`bookTicker`、`depthUpdate`、`aggTrade`、`markPriceUpdate`、`forceOrder`;`{"result":null,"id":` 为订阅回执,`{"error":` 为订阅被拒(都是 `SubAck`,`control` 给 `SubAck{ok, code}`);`{"e":"serverShutdown"`(若出现)→ `ServerShutdown` 类,`control` 给 `ControlHint::ServerShutdown`。按第 7 个字节起的前 4 字节做 `u32` 比较即可区分。分档快照与增量深度的推送同名(`depthUpdate`),按 `classify` 的 `route` 区分:`public-topn` → `DepthTopN`,`public-depth` → `DepthUpdate`,其他路由 → `Unknown`。二进制帧、前 64 B 内认不出的 → `Unknown`。
- `Spot` JSON:`{"u":` 开头(bookTicker 推送没有事件名)→ `JsonBookTicker`;`{"e":"trade"` → `JsonTrade`;`{"e":"depthUpdate"` → `JsonDepth`;订阅回执同 `Usdm`。`Spot` 分档快照走 combined 流:payload 以 `{"stream":"<s>@depth<N>@100ms","data":` 开头 → `JsonDepthTopN`,inst 取自 `stream` 名(小写 symbol 转大写后查表;推送体本身没有 symbol)。
- `Spot` SBE(optional):二进制帧前 8 B 为小端 `blockLength:u16, templateId:u16, schemaId:u16, version:u16`;`templateId` → 类别;`schemaId/version` 与编译进来的 schema 不符 → `Unknown` 并告警(协议变更)。文本帧是订阅回执。
- 下单连接与用户流:`{"id":"` 后第一个字符是我们的 op 码 → 响应;`{"e":"<名>"`(`Usdm` 用户流)或 `{"subscriptionId":…,"event":{"e":"<名>"`(`Spot` ws-api 用户流)→ 取事件名,按"用户流事件表"分类;不在表中的名字:`classify_report` 只认 `default_caps()` 的可忽略名单(命中 → `Ignorable`,否则 `Unknown`),`decode_report` 对 `Unknown` 类再查生效 `VenueCaps.ignorable_user_events`(refdata 追加的名字命中 → `skip = Listed`,否则 `skip = Unknown`)([契约 20](../hft-venue-proto/README.md#实现方必须遵守的契约))。

### 2. 解码

- `sonic-rs` 专用结构体,字段借用 `&str`,价格/数量保持字符串,用 hft-types 定点解析按资产指数转换(≈ 20–40 ns/字段);禁止 `serde_json::Value`。解析错误的映射:`FxErr::OVERFLOW` → `DecodeErr::Overflow`,其余(`SYNTAX`、`EMPTY`、`PRECISION`、`EXP_RANGE`)→ `DecodeErr::Number`;必需字段缺失或类型不符 → `Shape`;报文在必需字段之前结束 → `Truncated`。
- depth 的 `b`/`a` 数组经 `l2_begin → l2_level × N → l2_end` 写成一条 `L2Batch`,`qty == 0` 为删档。
- `timeUnit=MICROSECOND` 下 `E`/`T` 为 µs;`FeedCtx.time_unit` 由连接 URL 决定,解码不按数值大小猜。若合约流不支持该参数(待核实,开发备忘第 8 条),保持 ms,×10⁶ 转 ns。
- 账户查询响应(`acct_decode`):`positionRisk`、`symbolConfig` 可能列出全部 symbol,解码时按 `SymbolIndex` 只保留 refdata 中该账户可交易的 inst 写入 `AcctScratch`(定长 `[Item; MAX_INSTS_PER_VENUE]`);响应超过 50 KiB 时优先改用按 symbol 过滤的请求(需核实是否支持多 symbol 参数)。
- SBE(optional):按 Binance 发布的 schema XML 手写解码器(不引入 SBE 代码生成工具链,[13 O5](../../design/13-设计评审与优化.md));mantissa + 指数转为本资产定点,溢出 → `DecodeErr::Overflow`。schema 文件随 crate 放在 `schemas/`,版本号写进 `FeedCtx`。

### 3. 快照对齐(`align_first`)

`Usdm`,`L = lastUpdateId`:

1. 丢弃 `u < L` 的缓冲增量(`Align::Drop`)。
2. 第一条被应用的增量须满足 `U ≤ L ≤ u`;**或** `pu == L`(TradFi 合约会给 `U > L` 但 `pu == L` 的首条,现有采集系统已验证)→ `Apply`。
3. 否则 `Gap`(快照太旧或缓冲缺失),重取快照。
4. `Apply` 的那一条以 `prev_id = snap_id` 输出;此后 `prev_id = pu`,连续性由运行时按 `Chain`(`prev_id == 上一条 last_id`)与簿的同一比较检查。

`Spot`:丢弃 `u ≤ L`;第一条须 `U ≤ L + 1 ≤ u`(以 `prev_id = snap_id` 输出);此后 `prev_id = U − 1`,连续 ⇔ `U − 1 == 上一条 u`(`Chain`)。SBE diff depth 的首末 id 字段同理(字段名以 schema 为准)。

本节只在增量深度(可选扩展)时生效。快照 REST 权重:合约 `limit=1000` 为 20(`500` → 10、`100` → 5);现货 `limit=1000` 为 50(待核实,开发备忘第 21 条)。快照由 warm 任务 snap-fetch 请求与解析([hft-venue §3b](../hft-venue/README.md#3b-快照获取snap-fetch)),重同步风暴时由 `snap.parallel` 与 REST 权重计数共同约束(现有采集系统用"同时 ≤ 4 个快照"避免 418/429)。

### 4. 订单帧模板

`build_templates` 在 warm 线程为每个 (inst, 操作) 生成一段固定 JSON,运行时 `encode` 只写可变字段:

```text
{"id":"p0000002a00000003","method":"order.place","params":{"symbol":"BTCUSDT","side":"BUY","type":"LIMIT",
 "timeInForce":"IOC","price":"65000.1","quantity":"0.004","newClientOrderId":"1a2b3c4d5e6f708192a3b4c5",
 "newOrderRespType":"ACK","timestamp":1790000000000,"recvWindow":1500}}
```

- `side`、`timeInForce` 按出环 `GwCmd` 的 `side`/`tif` 选预编码片段;`reduceOnly=true` **只**在 `oflags` 带 `REDUCE_ONLY` 时写入(`Usdm`;写成 `"reduceOnly":"true"`,紧接在 `quantity` 之后);价格/数量按 inst 的 tick/lot 格式化为最短十进制(不补零,不经 f64)。
- `timestamp` 由 `WallClock.unix_ms` 填写(gw 从 TSC 校准换算);`recvWindow` 取 `stale_guard`。
- `newOrderRespType=ACK`:最快回执;成交以用户数据流为准。
- 帧长 ≤ 512 B,`ORDER_FRAME_CAP = 2048`。

### 5. 回报解码

| 输入 | `OrderEvent.what` |
|---|---|
| 下单响应 `status=200` | `Ack`(`exch_order_id = result.orderId`) |
| 下单响应 `status≥400`,`error.code` | 确定拒绝表 → `Reject{reject_code}`;结果未知表与未登记码 → `outcome_unknown` |
| 撤单响应(`order.cancel`)`status=200` | `CancelAck`(被撤订单的 client id:`Usdm` 取 `result.clientOrderId`,`Spot` 取 `result.origClientOrderId`——`Spot` 的 `clientOrderId` 是撤单请求自己的 id;`exch_order_id = result.orderId`);与用户流 `x=CANCELED` 重复时 OMS 幂等处理 |
| 改单响应(`order.modify`、`order.amend.keepPriority`)`status=200` | `AmendAck`(`Spot` 的被改订单在 `result.amendedOrder`,client id 取其 `origClientOrderId`) |
| `openOrders.cancelAll` 响应 `status=200`(`result` 为数组) | 零事件(撤单回报来自用户流) |
| 撤单、改单、查询、全撤响应 `status≥400` | 撤单:`-2011` → `CancelReject{detail = GONE}`,其余确定拒绝码 → `CancelReject{detail = 0}`;改单:确定拒绝码 → `AmendReject`;查询:`-2013` → `QueryResult` 的 NotFound 形态(见"结果未知码表"),其余确定拒绝码零事件;全撤:零事件;结果未知码(撤单、改单)→ `outcome_unknown`,查询与全撤只记 `raw_code`。错误响应不带 client id:`slot`/`gen` 取请求 id,`epoch` 写 0(见"已知限制与后续工作") |
| `ORDER_TRADE_UPDATE` `x=NEW` | `Ack`(若响应已给过 Ack,sequencer 幂等忽略) |
| `x=TRADE`,`X=PARTIALLY_FILLED/FILLED` | `PartialFill` / `Fill`(`qty ← l`,`px ← L`,`cum_filled ← z`,`avg_px ← ap`,`fee ← n`,`fee_asset ← N`,`ex_ts_ns ← T`);先过"外部成交识别" |
| `x=CALCULATED` | 只经"外部成交识别"产出 `ExternalFill`(本系统订单不会出现此类型;若 client id 是我们的且不命中强平类前缀 → `DecodeErr::Shape`,运行时按 `REPORT_DECODE` 告警) |
| `x=CANCELED` | `CancelAck` |
| `x=EXPIRED`(IOC 剩余、GTX 会吃单) | `Expired` |
| `x=AMENDMENT` | `AmendAck` |
| `executionReport`(Spot) | 同上映射(改单回报 `x = REPLACED` → `AmendAck`,待核实,方法 A),`c`/`C` 为 client id / 原 client id(撤单回报的 `c` 是撤单请求自己的 id,取 `C`),`Z / z` 为累计均价 |

`fee_asset` 与生效 `fee_asset_rule` 不符(如 `Usdm` 出现 `N = BNB` 而规则为 `Quote`,或 `Spot` 的 BNB 用尽后改扣成交币)→ `fee_rule_deviation = true`(契约 19)。

### 6. 死手与会话维护

- `dead_man() = CountdownPerInst{ min_ms: 5000, timeout_ms: 30000, refresh_ms: 10000, via: Rest }`(`Usdm`):sequencer 的 `DeadMan` 定时器发 `GwCmd{op: DeadManRefresh, arg: timeout_ms}`,gw 对槽镜像中**有挂单**的 inst 逐个发签名 REST(`countdownTime = timeout_ms`)。每次权重 10(待核实,开发备忘第 10 条),只刷有挂单的 inst;Admin 冒烟单(`OrderFlags::SMOKE`)占用槽镜像,所在 symbol 同样被刷新。
- listenKey:REST `POST /fapi/v1/listenKey` 取得(由 `bootstrap(Role::UserStream, cfg, Some(REST 凭证))` 给出该请求,头 `X-MBX-APIKEY`,无签名;其余角色与 `Spot` 返回 `None`;`endpoint` 从 `boot` 响应体取 `listenKey` 拼出 `/private/ws/<listenKey>`);sequencer 的系统定时器每 30 min 发 `SessionKeepalive`,`session_keepalive_request` 给出 `PUT /fapi/v1/listenKey`(`Usdm` 返回 `Ok(true)`,`Spot` 返回 `Ok(false)`);续期失败 → `ConnState{Degraded, role=UserStream}`;`listenKeyExpired` 或续期返回 `-1125` → `ctl = UserStreamEnded`,运行时推 `ConnState{Down, role=UserStream}`(OMS 置 VERIFY 并查询)并重建用户流连接(重新 `bootstrap` 取新 key)。listenKey 不进快照;每次进程启动 gw 重新取 key,sequencer 重新登记 30 min 周期的 `SessionKeepalive` 定时器;gw 收到 `SessionKeepalive` 时若尚无 key → 忽略并计数。
- 现货用户流:归属连接登录后发 `userDataStream.subscribe`;计划切换时新 primary 先订阅、收到回执后旧连接 `userDataStream.unsubscribe`([hft-venue §4b](../hft-venue/README.md#4b-私有频道归属与热备连接))。
- 24 h 强断:refdata `VenueMeta.ws_max_conn_age_s = 86400`,运行时在 24 h − `conn.rotate_margin_s` 处计划轮换([hft-venue 配置项](../hft-venue/README.md#配置项));收到 `serverShutdown` → `ServerShutdown` 提示,运行时提前轮换。
- 心跳与 BBO 探测:按产品线见[心跳与 BBO 探测](#心跳与-bbo-探测)。

### 7. 订阅节奏

每条行情连接入向 ≤ 10 条/s(合约)/ 5 条/s(现货):`subscribe` 把多个流装进一个 `SUBSCRIBE` 帧的 `params`(每帧 ≤ 200 个流,帧 `id` 从 1 起逐帧加 1),运行时按 `RouteSpec.max_in_msgs_per_s` 的一半限速发送。流名(symbol 小写):`Bbo` → `<s>@bookTicker`;`DepthTopN` → `<s>@depth<N>@100ms`(`N` 不在 `topn_levels` → `ProtoErr::Unsupported`);`Trade` → `Usdm` `<s>@aggTrade`、`Spot` `<s>@trade`;`Mark`/`Index`/`Funding` → `<s>@markPrice@1s`(同一 inst 只出一个);`Liquidation` → `<s>@forceOrder`;`Spot` 的 `Mark`/`Index`/`Funding`/`Liquidation` → `ProtoErr::Unsupported`。`private_subscribe`:`Usdm` 用户流由 listenKey 路径确定,不出帧;`Spot` 出一帧 `userDataStream.subscribe`(`unsub = true` 为 `userDataStream.unsubscribe`)。每连接流数 ≤ 1024;官方建议单连接高频流不宜过多,`public` 路由按 inst 数拆成多组连接由 refdata 配置。

### 8. 源文件规划

| 文件 | 职责 |
|---|---|
| `src/lib.rs` | `Usdm`、`Spot`(同一份实现,按 `Line` 区分产品线)与 trait 实现入口 |
| `src/line.rs` | `Line`:两个产品线的差异 |
| `src/json.rs` | sonic-rs 专用结构体(字段借用 `&str`)、`rateLimits` 选择、解码错误映射 |
| `src/wr.rs` | 定长帧写入器(写满即失败,不分配) |
| `src/caps.rs` | 两个产品线的 `default_caps()`、官方默认限速、`stale_guard`/`dead_man`、心跳规格 |
| `src/cid.rs` | client id 与请求 id 渲染/解析 |
| `src/sign.rs` | Ed25519 logon 签名;REST 一律 HMAC-SHA256(独立 key) |
| `src/errors.rs` | 错误码两张表:确定拒绝 / 结果未知(未登记码默认结果未知) |
| `src/external.rs` | 外部成交识别(两个产品线共用的前缀表与判定顺序) |
| `src/md.rs` | 行情分派、`peek`、序号规则、控制提示与行情解码(bookTicker、分档快照、aggTrade/trade、markPrice、forceOrder;增量 depth 为可选扩展) |
| `src/snapshot.rs` | REST 快照解析与 `align_first`(可选扩展) |
| `src/session.rs` | 路由、端点、listenKey bootstrap、logon、订阅与私有频道订阅、`countdownCancelAll` |
| `src/orders.rs` | 模板与 `encode`(两个产品线的 ws-fapi/ws-api 帧) |
| `src/reports.rs` | 回报分派、WS API 响应、`ORDER_TRADE_UPDATE`/`executionReport` 与用户流事件表 |
| `src/acct.rs` | Config 与 Margin 序列的请求与解码 |
| `src/spot/sbe/*.rs` | SBE 头、bestBidAsk、trade、depth diff、depth20(feature `spot-sbe`) |
| `schemas/` | SBE schema XML(随版本锁定;feature `spot-sbe`) |
| `fuzz/` | fuzz 目标(见 [TESTS.md](TESTS.md)) |

</details>

## 延迟与资源契约

纯函数,无系统调用、无分配(`login`/`build_templates`/`subscribe`/`private_subscribe` 在 warm 线程,可分配)。

| 入口 | p50 / p99 预算 | 基准名 |
|---|---|---|
| `classify` | ≤ 20 ns / ≤ 50 ns | `bench_classify_usdm` |
| `peek` bookTicker(符号查表 + `u`) | ≤ 80 ns / ≤ 150 ns | `bench_peek_bookticker` |
| `decode_md` bookTicker 300 B → `Bbo` | ≤ 0.4 µs / ≤ 0.8 µs | `bench_decode_bookticker` |
| `decode_md` 分档快照 20 档 → `L2Snapshot{TopN}` | ≤ 1.2 µs / ≤ 2 µs | `bench_decode_depth20` |
| `decode_md` depthUpdate 2 KiB → 一条 `L2Batch`(增量深度,可选扩展) | ≤ 2.5 µs / ≤ 5 µs | `bench_decode_depth_2k` |
| `decode_md` 现货 JSON bookTicker | ≤ 0.4 µs / ≤ 0.8 µs | `bench_decode_spot_bookticker` |
| `decode_md` SBE bestBidAsk(optional) | ≤ 60 ns / ≤ 120 ns | `bench_decode_sbe_bba` |
| `encode` order.place | ≤ 0.3 µs / ≤ 0.6 µs | `bench_encode_place` |
| `decode_report` ORDER_TRADE_UPDATE(含外部成交判定与去重) | ≤ 1 µs / ≤ 2 µs | `bench_decode_otu` |
| `decode_report` 可忽略/未知事件 | ≤ 0.15 µs / ≤ 0.3 µs | `bench_skip_event_usdm` |
| `acct_decode` positionRisk 50 KiB | ≤ 60 µs / ≤ 100 µs | `bench_acct_decode_position_risk` |
| `decode_snapshot` 1000 档(snap-fetch,warm;增量深度,可选扩展) | ≤ 80 µs / ≤ 100 µs | `bench_decode_snapshot_1000` |

验收门槛"解码 < 1 µs/条"指 bookTicker/aggTrade/回报类单条消息。

## 测试与验收

用例与验收见 [TESTS.md](TESTS.md)。

## 依赖

| 依赖 | 理由 |
|---|---|
| `hft-types`、`hft-refdata`、`hft-event` | 定点、ID、`SymbolIndex`、`VenueCaps`、事件体、出环元素 `GwCmd` |
| `hft-venue-proto` | `VenueProtocol` 与辅助类型(测试与 fuzz 用其 `testkit` feature) |
| `sonic-rs 0.5` | JSON 专用结构体解码 |
| `serde 1`(`derive`) | sonic-rs 专用结构体的派生(不用 serde_json) |
| `ed25519-dalek 2` | logon 与 SBE key 签名(warm 线程) |
| `hmac 0.12` + `sha2 0.10` | REST HMAC(独立 key) |
| `base64 0.22` | 签名编码 |

**禁止**:`hft-net`、`hft-venue`、`tokio`、`serde_json`(仅 `[dev-dependencies]` 做金样对照)、任何 I/O。

## 延伸阅读

- [research/02 §2 Binance](../../design/research/02-exchange-connectivity.md)、[§11.2 通道矩阵](../../design/research/02-exchange-connectivity.md)
- [04 §5](../../design/04-行情接入层.md)、[05 §7](../../design/05-CEX执行层.md)、[13 R8](../../design/13-设计评审与优化.md)
- [research/01 §4.3 Binance Spot SBE/FIX](../../design/research/01-network-kernel-bypass-tls.md)
- [hft-venue-proto 契约](../hft-venue-proto/README.md#实现方必须遵守的契约)、[hft-venue](../hft-venue/README.md)、[hft-refdata](../hft-refdata/README.md)(`VenueCaps`)、[event-model](../../docs/subsystems/event-model.md)、[cookbook/adding-a-venue.md](../../docs/cookbook/adding-a-venue.md)
- 评审证据(资金费按币归属、强平识别、账户模式核验):[review-2026-10-02](../../design/_work/review-2026-10-02-主策略适配与执行正确性.md);范围:[round8-decisions](../../design/_work/round8-decisions.md)
- 现有知识来源:`E:\trading\Trading\crates\binance`(depth 同步、`/market` 路由)、`E:\trading\Trading\apps\collector\src\private.rs`(listenKey、私有对账端点)、旧套利引擎 Rust 交易模块 `cex-arb/src/exchange/bn_futures.rs`(位于 `E:\trading\Trading\` 下;logon 签名串、`positionRisk`)

### 测试环境 warm 账户入口

USDM 的 `encode_rest(CancelAll)` 按合约生成签名 DELETE，全账户 inst=0 返回 Unsupported，供运行时逐合约展开。REST code=200 只确认请求；不生成逐单取消回报。403/429/5xx 与业务错误保留结果未知，限速/时钟偏差另发连接提示。`dead_man_refresh` 按 inst 生成签名 POST；确认的 countdownTime 接受官方字符串及数字形式，值与请求必须相等。

AccountProtocol 核对 exchangeInfo 的 tick/lot/minQty/minNotional；USDM accountConfig、symbolConfig 核验单向、单资产、全仓与杠杆，v3/account 和 v3/positionRisk 提供基线。遗留订单逐笔取消后查到空。REST 支持 HMAC/Ed25519，listenKey 由 REST 创建续期。accountConfig 不返回 UID，Demo 的非正式 Unverifiable 账户须使用同一 REST/WS key 并经人工身份确认；正式账户与完整跨主机权限/UID 查询不由此入口放行。

私有流支持 ORDER_TRADE_UPDATE、ACCOUNT_UPDATE、现货包装 executionReport/outboundAccountPosition；成交按有限窗口去重，非正 trade id 按订单累计量去重；资金费推送按时刻与钱包余额去重。流结束触发运行时重连认证。

## 已知限制与后续工作

- **COIN-M 不做**:需要时新增 `Coinm` 类型复用 `usdm` 模块。
- **统一账户(Portfolio Margin)不支持**:PM 账户走另一套 API;核验发现即 `ACCT_TYPE`。
- **FIX OE / FIX-SBE 与 SBE 下单响应**(`responseFormat=sbe`):可选扩展:与 JSON ws-api 对比证明更快时启用;需 `hft-net` 支持 FIX 分帧。
- **RPI 深度**(`@rpiDepth@500ms`)不订阅:bookTicker/depth 不含 RPI 单,"看得见的最优价未必可成交"由策略侧参数处理。
- **批量下单**:合约 REST `batchOrders` ≤ 5 单且并发处理不保证顺序,WS API 无对应方法;不做。
- **`TRADE_LITE` 不用**:它可能比 `ORDER_TRADE_UPDATE` 更早到达;可选扩展(触发条件:对冲第二腿的回报延迟成为主要项)时改为成交提示,并与 `ORDER_TRADE_UPDATE` 按 `t` 去重。
- **保证金轮询走 REST**:5 s 间隔下逼空时的保证金率最多滞后 5 s + 一次往返;更快的来源(ws-fapi 账户方法)待核实后再评估(开发备忘第 10 条)。
- **现货用户流**依赖 ws-api 归属连接上的订阅;归属切换期间的缺口由 `Unknown` + 查询收敛。
- **当前实现**:core 行情含 bookTicker、TopN、逐笔、标记/指数/资金费与公开强平;私有回报与账户启动查询已接入。公共 REST BBO 支持 USDT-M 单品种与 Spot symbols 批量,整批校验后输出。签名 REST 网关方法、统一只读查询族、完整金样与增量深度仍未完成。`fuzz/` 含两条产品线的行情、BBO、回报与账户解码目标;累计 fuzz 门槛仍待运行。
- **错误响应的 epoch**:WS API 错误响应不带 client id,请求 id 也不含 epoch,`decode_report` 以请求 id 的 `slot`/`gen` 与 `epoch = 0` 产出事件;gw 的 epoch 不为 0 时这些事件按外来处理(hft-venue 把 `slot` 置为 `UNRESOLVED`)。M3 之前在 hft-venue gw 侧按槽镜像补 epoch(或把 epoch 编进请求 id),届时同步改 [hft-venue-proto 契约 13](../hft-venue-proto/README.md#实现方必须遵守的契约) 与"5. 回报解码"表。

### 开发备忘

<details><summary>给实现者</summary>

**待核实**(本机连不上 developers.binance.com。方法 A:经东京服务器抓官方文档原文(`research/_src/rgrab.py`),摘录写进本 README 并把原文缓存到 `research/_src/`;方法 B:Demo Trading 演示环境实测并把 raw 样本放进 fixtures;方法 C:正式实盘开始后从 raw journal 补核——只用于上线后才可能出现的样本(真实强平、ADL),上线前必须确定的项不得依赖它。每项注明用哪种。)

1. **全仓资金费推送的形态**(决定 `funding_settle = BalanceOnlyPush` 与去重键):是否不带持仓 `P`;同一结算时刻是否按 symbol 分多条推送;`T` 是否等于结算时刻;`B[].wb` 在同一时刻的多条推送间是否各不相同(作 `disc`)。方法:A + B(演示环境持仓两个 symbol 跨过一次结算,抓 `ACCOUNT_UPDATE m=FUNDING_FEE` 原文);当前默认保持 `RateOnly` 与认证 income 历史实额。未核实上述字段前不得启用可选推送记账;切换来源须同时停用历史实额,并另行评审去重与归属。
2. **强平/ADL/结算单的 client id 前缀、`o.o = LIQUIDATION`、`x = CALCULATED` 的语义**:A;再在演示环境用高杠杆小仓制造一次强平取样(B)。ADL 样本只能等实盘出现或向官方索取样本;取得前 ADL 行的金样用文档原文构造并标"合成"。
3. **网页/App 单的 client id 前缀**(`web_`、`ios_`、`and_`):B(演示环境网页端下一笔单,若演示环境有网页端);否则按 `Unknown` 上线(偏安全),C 补核。
4. **可忽略事件名全集**(`TRADE_LITE`、`STRATEGY_UPDATE`、`GRID_UPDATE`、`CONDITIONAL_ORDER_TRIGGER_REJECT`、现货 `externalLockUpdate`):A;上线后由"未知事件"计数补全(refdata 追加,不需发版)。
5. **账户配置端点**:`positionSide/dual`、`multiAssetsMargin`、`symbolConfig`、`feeBurn`、`sapi/v1/bnbBurn`、`sapi/v1/account/apiRestrictions` 的路径、权重与字段;统一账户在 fapi 上的错误码:A + B。
6. **保证金端点**:`fapi/v3/account`、`fapi/v3/positionRisk` 的字段与权重,v3 是否只返回有持仓/挂单的 symbol、是否带 ADL 档;`adlQuantile` 的更新周期与 0..4 语义:A + B。
7. **BNB 抵扣**:合约 `feeBurn` 开启时 `N` 是否为 BNB;现货 BNB 不足时是否回退为扣成交所得资产:B(演示环境分别开关各成交一笔)。
8. `fstream` 是否接受 `timeUnit=MICROSECOND`;不接受则合约保持 ms:B。
9. `@depth@0ms` 是否仍可订阅(文档已无):直接订阅,看是否报错及实际节拍。
10. **`countdownCancelAll`**:端点、语义(倒计时到期撤该 symbol 全部挂单)、实际权重与最小倒计时;`ws-fapi` 是否提供等价方法与账户/持仓查询方法;合约 REST 是否接受 Ed25519 签名:A。核实前 `dms` 一行的来源列写待核实并指向本条(方法 A),[kill-switch](../../apps/kill-switch/README.md) 引用本条的结论。
11. `fstream` `/public`、`/market` 的新连接限速与 `serverShutdown` 事件的确切格式与提前量:A。
12. **私有频道双订阅(M3 之前必须核实)**:同一 listenKey 两条连接是否都收到推送(决定 `Usdm` 用户流轮换能否先建后断);同一账户两条 ws-api 连接同时 `userDataStream.subscribe` 是否都收到(`Spot`):B。不允许时该产品线的切换退化为"先断后建 + VERIFY"并写进本 README。
13. ws-fapi 下单连接是否也受"10 条入向/s"约束(若是,批量撤单与高频改单要在风控令牌桶里体现):A。
14. **Demo Trading 演示环境(M3 之前必须核实)**:按官方 Spot "Demo Mode"(developers.binance.com/docs/binance-spot-api-docs/demo-mode/general-info)与 USDⓈ-M "General Info"(developers.binance.com/docs/derivatives/usds-margined-futures/general-info)缓存原文,核对"测试网/模拟盘"表的每个端点;USDⓈ-M 演示环境是否提供 WS API 下单(`ws-fapi` 的演示端点)——决定门槛 ⑥ 的下单通道与 egress 形态;`demo-fstream` 是否有 `/public`、`/market`、`/private` 路由拆分;同一把演示 key 是否同时用于现货与 USDⓈ-M、`/api/v3/account` 是否返回 `uid`、`sapi` 类端点是否存在(决定 `uid_policy` 与冒烟时记为"不可用"的项);演示环境 symbol 与主网的映射(名称、品种集合、tick/lot/过滤器)与行情来源:A + B。
15. (启用 L2 双路合并时)同一 inst 两条连接的 depth 增量批是否逐条相同。
16. **会话 key 的 UID 查询**:只开合约权限的 Ed25519 key 能否签名读取现货 `GET /api/v3/account`;不能时 ws-api `account.status` 是否返回 `uid`:A + B。
17. **`@bookTicker` 订阅后是否立即推当前 BBO**(决定 `bbo_snapshot_request`)、`/fapi/v1/ticker/bookTicker` 的 `lastUpdateId` 字段:B。
18. **心跳口径按产品线**:fstream、ws-fapi、现货 stream、ws-api 各自的服务端 Ping 间隔与无 Pong 断开时限;客户端 Ping/Pong 帧是否计入"入向消息/s"限额:A。
19. **成交历史不带 client id**:`GET /fapi/v1/userTrades` 与 `myTrades` 的记录是否只有 `orderId`(无 `clientOrderId`);`GET /fapi/v1/order?origClientOrderId=` 对已过期或从未被接受的 client id 返回的码(`-2013`)与可见性延迟:A + B(演示环境下单后立即按 client id 查询)。结论决定 reconciler 的 `history_checked` 判据能否简化。
20. **分档快照流**:`Usdm` `<s>@depth<N>@100ms` 在 `/public` 还是 `/market` 路由、推送体是否与增量 depth 同名(`depthUpdate`)、`u` 是否单调、订阅后首帧是否立即推当前快照;`Spot` 部分深度推送体是否不带 symbol(决定用 combined 流)、`lastUpdateId` 是否单调、是否接受 `timeUnit=MICROSECOND`;两者在突发时的实际节拍与单条字节数:A + B。核实前按本 README "分档快照流" 表实现,结论写回该表。
21. **现货 REST 深度快照的大小**(只在增量深度启用时):`/api/v3/depth?limit=1000` 与 `limit=5000` 对主流合约的响应体字节数与权重;决定 snap 环容量(单条记录 ≤ 容量 / 4,容量归 hft-core 环容量表,dev2 较小)与可用档数。核实前用 `limit=1000`:B(演示环境与主网各取 20 个合约)。
22. **只读查询端点**("只读查询"表):各端点的路径、版本、分页上限、权重与字段(含 `userTrades` 是否可不带 symbol、`income` 每页上限):A + B(演示环境只读 key 逐个调用并抓响应进 fixtures)。

**实现顺序**:`usdm/md.rs`(bookTicker、分档快照、aggTrade、markPrice)+ 金样 → `orders.rs`/`reports.rs`(含 `external.rs` 与用户流事件表)→ `usdm/acct.rs` → `session.rs` → fuzz → `spot/json.rs` → `spot/orders.rs`/`reports.rs`/`acct.rs` →(可选扩展)增量深度与 `snapshot.rs` 对齐属性测试 → (optional)SBE。

</details>
