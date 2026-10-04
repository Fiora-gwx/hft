---
description: "OKX USDT 永续与现货的纯协议实现(core):bbo-tbt/books 的 seqId 链续、登录后免签的 WS 下单(instIdCode 整数、expTime)与回报解码、账户配置核验、推送型保证金数据、外部成交识别与 VenueCaps 默认值;SBE 行情为 optional"
kind: crate
tier: pure
scope: core
status: spec
---

# venue-okx

## 概要

`venue-okx` 实现两个 [`VenueProtocol`](../hft-venue-proto/README.md#venueprotocol静态分派):`OkxSwap`(USDT 本位永续,`MarketKind::LinearPerp`)与 `OkxSpot`(现货,`MarketKind::Spot`),两者都是 core。它拥有 OKX 的协议知识:`bbo-tbt`、分档快照 `books5`(core 深度)与增量 `books*` 频道的 `seqId/prevSeqId` 链续(可选扩展;checksum 已固定为 0)、`login` 签名、`order/batch-orders/amend-order/cancel-order/batch-cancel-orders` 帧(**只能用整数 `instIdCode`**)、请求级 `expTime` 过期保护、响应 `inTime/outTime` µs 网关戳、`cancel-all-after` 死手、`account/config` 账户配置核验、`account`/`positions` 推送的保证金与 ADL 数据、`orders` 推送 `category` 的强平/ADL 识别、`clOrdId` 格式、限速与 `VenueCaps` 默认值。OKX 2026-07 末迁到东京,全域 Cloudflare 前置(REST 热连接 14–16 ms vs WS 6.4 ms),所以下单与订单查询**只走 WS**,REST 只做账户查询、快照与兜底;没有 colo/专线可申请([research/02 §3](../../design/research/02-exchange-connectivity.md))。SBE 行情是 optional(需登录、无初始快照;触发条件:对照测试证明解析收益)。**一个账户在 OKX 只启用 `OkxSwap` 或 `OkxSpot` 之一**(refdata `SameExchangeMarkets`,[hft-refdata](../hft-refdata/README.md)):OKX 交易账户里现货与永续共用同一钱包,本系统不在同一账户里同时交易两者,所以 `account` 推送的余额与保证金只归属该账户启用的那一个 venue,不存在两个 venue 重复记同一笔 USDT 的情形。标"官方"的事实来自 OKX v5 文档全文的本地缓存(`architecture-v2/research/_src/okx_docs.txt`,2026-09-29 抓取),标"待核实"的写了核实方法。纯函数,运行时见 [`hft-venue`](../hft-venue/README.md)。

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
pub struct OkxSwap;   // NAME = "okx-swap", VENUE = OKX_SWAP, MARKET = LinearPerp
pub struct OkxSpot;   // NAME = "okx-spot", VENUE = OKX_SPOT, MARKET = Spot

// client id(`clOrdId`):`render_cid(&ClientOrderId, &mut [u8; 48])` / `parse_cid(&[u8]) -> Option<ClientOrderId>`(VenueProtocol 方法)。
// 编码是 hft-types 的 ClientOrderId(规范 24 位十六进制),本 crate 只加外形:<24hex>,24 字节(≤ 32,仅字母数字)。
// parse_cid:恰为 24 个小写十六进制字符且 ClientOrderId::decode 成功 → Some;其余(空串、长度不符、含 `-` 等非字母数字、大写、校验位错)→ None。

pub enum MdClass {     // 两个产品线共用;增量 books*、SBE 的类随可选扩展另加
    BboTbt, Books5, Trades, MarkPrice, FundingRate, IndexTickers,
    Control,           // {"event":"subscribe"|"unsubscribe"|"error"|"notice"} → control() 给 SubAck{ok, code} 或 ServerShutdown(64008)
    Pong,              // 文本 `pong` → ControlHint::Pong
    Unknown,
}
pub enum ReportClass {
    OrderResp,         // {"id":"<p|c|m>…":单单 order / cancel-order / amend-order 的响应
    BatchResp,         // {"id":"<b|x|n>…":batch-orders / batch-cancel-orders / batch-amend-orders 的响应
    RestResp,          // {"code":…:REST 响应(查单、orders-pending、cancel-all-after)
    Orders, Positions, Account, BalanceAndPosition, LiquidationWarning,
    Control,           // {"event":"login"|"subscribe"|"unsubscribe"|"error"|"notice"}
    Pong, Ignorable, Unknown,
}
```

请求 `id` = `<op 字符><slot 8 位小写十六进制><gen 8 位小写十六进制>`(17 字符,只含字母数字);op 字符:`p` = `order`(`Place`)、`c` = `cancel-order`(`Cancel`)、`m` = `amend-order`(`Amend`)、`b` = `batch-orders`、`x` = `batch-cancel-orders`、`n` = `batch-amend-orders`(批量帧的 id 取第一单的 slot/gen)。响应的 `id` 不符合此格式 → `ReportOutcome.req = None`。

### 端点与路由

| 用途 | 端点 | 说明 |
|---|---|---|
| 公共行情(JSON) | `wss://ws.okx.com:8443/ws/v5/public` | BBO 路由 `public-bbo`(`bbo-tbt`,`race=true`);分档快照路由 `public-topn`(`books5`,`RouteKind::DepthTopN`)、成交/标记价路由各 1 条;增量深度(可选扩展)另 1 条 |
| 公共行情(SBE,optional) | `wss://ws.okx.com:8443/ws/v5/public-sbe` | 需登录;同连接 JSON(opcode 1)与 SBE(opcode 2)混发 |
| 私有:下单 + 订单/持仓/账户推送 | `wss://ws.okx.com:8443/ws/v5/private` | 登录后免签;primary 是私有频道归属连接,standby 只登录不订阅([hft-venue §4b](../hft-venue/README.md#4b-私有频道归属与热备连接)) |
| 业务频道 | `wss://ws.okx.com:8443/ws/v5/business` | `trades-all` 等,不用 |
| REST | `https://openapi.okx.com` | 2026-05-20 起推荐域;账户查询、SBE 初始快照与 BBO 补帧(snap-fetch)、撤全部兜底的挂单列表(`GET /api/v5/trade/orders-pending`)、CAA 死手。REST 端点表只有一项(下标 0),两个产品线相同 |

`wsaws.okx.com`、`aws.okx.com` 已停用。模拟盘端点见"测试网/模拟盘"。

**`routes()`**(两个产品线;`OkxSpot` 没有 `public-mark`):`public-bbo`(`RouteKind::Bbo`,`race = true`)、`public-topn`(`DepthTopN`,`topn_levels = [5]`)、`public-trades`(`Md`,`trades`)、`public-mark`(`Md`,只 `OkxSwap`:`mark-price`、`funding-rate`、`index-tickers`);每条 `max_streams = 100`、`max_in_msgs_per_s = 4`(OKX 未公布每连接订阅数与每秒入向上限,取保守值,待核实开发备忘第 5 条;订阅/退订/登录另受 `rate_rules()` 的 480 次/h 约束)。**`endpoint`**:`Md` → `/ws/v5/public`,`Order` → `/ws/v5/private`(私有频道也在这条连接上);`UserStream` → `Err(ProtoErr::Unsupported)`(没有独立用户流连接);主机取自 refdata 端点表。`bootstrap` 对所有角色返回 `None`;`login(Md)` → `LoginStep::NotNeeded`,`login(Order)` → `LoginStep::Frame`。

### 行情频道 → 事件

| 频道 | 产品线 | 节拍 / 门槛 | 规则 | 事件 | 时间戳 |
|---|---|---|---|---|---|
| `bbo-tbt`(独立 BBO 连接,双路;feed-bbo 任务) | 两者 | 10 ms 有变化才推;所有等级 | `Monotonic` on `seqId`(待核实,见开发备忘) | `Bbo`(bbo 环) | `ex_ts_ns ← ts`(ms) |
| `books5`(分档快照,core) | 两者 | 5 档;节拍见"分档快照流" | `Snapshot` on `seqId` 或 `ByTime`(见"分档快照流") | `L2Snapshot{origin = TopN}`(前 5 档整替换) | `ts` |
| `books-l2-tbt`(增量深度,可选扩展) | 两者 | 400 档,10 ms;**VIP4+** | `Chain`(`prev_id = prevSeqId`,`last_id = seqId`) | 首帧 `L2Snapshot{origin = Exchange}`,之后每条消息一条 `L2Batch` | `ts` |
| `books50-l2-tbt`(增量深度,可选扩展) | 两者 | 50 档,10 ms;VIP4+ | 同上 | 同上 | `ts` |
| `books`(增量深度,可选扩展) | 两者 | 400 档,100 ms;VIP4 以下的回退 | 同上 | 同上 | `ts` |
| `trades` | 两者 | 按 taker 单聚合 | `Monotonic` on `tradeId` | `Trade` | `ts` |
| `mark-price` | 永续 | — | `ByTime` | `MarkPrice` | `ts` |
| `funding-rate` | 永续 | — | `ByTime` | `Funding{rate ← fundingRate, next ← fundingTime}`;实际结算周期 = `nextFundingTime − fundingTime`(待核实字段语义,开发备忘第 8 条) | `ts` |
| `index-tickers` | 永续 | — | `ByTime` | `IndexPrice` | `ts` |
| SBE `bbo-tbt` / `books-l2-tbt` / `trades`(optional) | 两者 | 同 JSON | 同上 | 同上 | `tsUs`(µs) |

- 字段映射:`bbo-tbt` → `Bbo{bid ← bids[0], ask ← asks[0], update_id ← seqId}`(`bids` 或 `asks` 为空数组 → 该侧 `px = 0`、`qty = 0`);`books5` → `L2Snapshot{origin = TopN, snap_id ← seqId}`(`qty = "0"` 的档不输出);`trades` → `Trade{px ← px, qty ← sz, trade_id ← tradeId, taker_side ← side}`;`mark-price` → `MarkPrice{px ← markPx}`;`funding-rate` → `Funding{rate ← fundingRate, next_funding_ns ← fundingTime, interval_s ← (nextFundingTime − fundingTime) / 1000}`(`nextFundingTime` 为空 → `interval_s = 0`);`index-tickers` → `IndexPrice{px ← idxPx}`,其 `instId` 是指数名(如 `BTC-USDT`),映射到 refdata 中符号为 `<instId>-SWAP` 的 `OkxSwap` inst。全部 `ex_ts_ns ← ts`(ms × 10⁶),`ex_send_ns = 0`。数量字段(`sz`、档位量)一律按 `qty_wire_exp` 解析。
- 价格/数量字符串按 refdata 线格式指数经 hft-types `parse_exact` 解析;`FxErr::OVERFLOW` → `DecodeErr::Overflow`,其余解析错误 → `DecodeErr::Number`;必需字段缺失或类型不符 → `DecodeErr::Shape`,报文在必需字段前结束 → `DecodeErr::Truncated`。
- 非 `Chain` 类的 `peek`:`ids.prev_id = 0`,`ids.last_id` = 去重序号(`bbo-tbt`、`books5` 的 `seqId`,`trades` 的 `tradeId`;`ByTime` 类为 `ts` 换算的 ns);`instId` 不在 refdata 本 venue 的符号表 → `inst = 0`。
- 同一连接推送顺序固定:`bbo-tbt → books-l2-tbt → books50-l2-tbt → books → books-rpi → books5`。
- 深度档位 `[px, qty, "0", numOrders]`;`qty == "0"` 删档。长时间无变化时服务端发空 `asks/bids`、`seqId == prevSeqId` 的保活更新,按重复丢弃。
- 增量深度(可选扩展)的重同步:JSON 深度用 `ResyncMethod::Resubscribe`(退订再订阅,首帧即快照,在 feed-md 解码);SBE 无初始快照,用 REST `GET /api/v5/market/books-sbe`(400 档,约 500 ms 一版,10 req / 10 s / IP+instIdCode),由 snap-fetch 请求与解析([hft-venue §3b](../hft-venue/README.md#3b-快照获取snap-fetch))。

### 下单:WS `/private`

| 操作 | op | 要点 |
|---|---|---|
| 登录 | `login` | `args[{apiKey, passphrase, timestamp, sign}]`,`sign = Base64(HMAC-SHA256(secret, ts + "GET" + "/users/self/verify"))`,`ts` 为 Unix 秒;30 s 窗口(现有采集系统私有流已在生产使用) |
| 下单 | `order` | `args[{instIdCode, tdMode, side, ordType ∈ {limit, post_only, ioc, fok}, sz, px, clOrdId, reduceOnly}]`;`tdMode`:永续 `cross`、现货 `cash`;`reduceOnly` 只用于永续;请求顶层 `expTime`([官方]) |
| 批量 | `batch-orders` | ≤ 20 单(`encode_batch`) |
| 改单 | `amend-order` / `batch-amend-orders` | 每单在途 amend ≤ 3 |
| 撤单 | `cancel-order` / `batch-cancel-orders` | `mass-cancel` 只适用期权,不用;`CancelAll` = 运行时把该账户全部活动订单展开为 `Cancel` 按 ≤ 20 个一批发 `batch-cancel-orders`,再按 `CANCEL_ALL_SWEEP = true` 查一次 `orders-pending` 兜底 |
| 查单 | WS 无;REST `GET /api/v5/trade/order?instId=&clOrdId=` | `order_channel(Query) = Rest`,`encode_rest` 生成请求,走 gw 的 REST 池;响应经 `decode_rest`:`accFillSz`、`avgPx` → `QueryResult` 的 `cum_filled`、`avg_px` |
| 死手 | REST `POST /api/v5/trade/cancel-all-after` | `timeOut` 秒,取值 0 或 [10, 120],0 关闭;账户级;限速 1 次/s(UID)([官方]) |

- **2026-03-26 起 WS 下单必须用整数 `instIdCode`**(`instId` 被移除);码值来自 refdata(hft-cfg 从 instruments 接口同步);**同一 `instId` 的码值在实盘与模拟盘可能不同**([官方]),两套 refdata 分别生成。
- `expTime` = `now_ms + gw.stale_guard_ms`(默认 1500):服务器时间超过即不处理([官方];[05 §7](../../design/05-CEX执行层.md))。它在网关还是撮合入口生效待核实(开发备忘第 16 条),所以核实前只作为减少过期执行的保护,不作为查询"不存在"定终态的依据(`stale_guard() = None`,见"VenueCaps 默认值")。
- **数量只经 refdata 线格式指数换算**:永续 `sz` 以张计,`ctVal` 是 10 的幂,hft-cfg 把它写成 `InstMeta.qty_unit_shift = log10(ctVal)`(如 BTC-USDT-SWAP `ctVal = 0.01` → −2;1000PEPE 类大面值合约为正),加载器算出 `qty_wire_exp = qty_exp − qty_unit_shift`([hft-refdata](../hft-refdata/README.md));`encode` 用 `qty_wire_exp` 把资产单位的 `Qty` 直接格式化成张数,`decode_*` 用同一指数把 `sz`/`fillSz`/`accFillSz` 直接解析成资产单位——本 crate 运行时**不出现任何乘除 `ctVal`**。非 10 的幂的 `ctVal` 在 refdata 生成时被排除。现货 `qty_unit_shift = 0`,`sz` 即基础币数量。
- 热路径必须 `rpiTakerAccess=false`(否则所有单被加 speed bump);`stpMode` 保持默认 `cancel_maker`。
- 响应 `{"id","op","code","msg","data":[{clOrdId, ordId, ts, sCode, sMsg}],"inTime","outTime"}`:`inTime/outTime` 为 µs 网关收发时刻。
- 帧内容:`order`/`batch-orders` 见实现说明第 3 点(帧中不写 `rpiTakerAccess`,缺省即 false;`Tif` → `ordType`:`Gtc` → `limit`、`PostOnly` → `post_only`、`Ioc` → `ioc`、`Fok` → `fok`);`amend-order` 的 `args[{instIdCode, clOrdId, newSz ← c.qty, newPx ← c.px}]`;`cancel-order` 的 `args[{instIdCode, clOrdId}]`(一律按 `clOrdId`,不用 `ordId`)。`expTime` 只写在 `order`、`batch-orders`、`amend-order`、`batch-amend-orders` 上(撤单帧没有该参数)。
- **`CancelAll`**:运行时把槽镜像中该账户的活动订单作为 `Cancel` 命令按 ≤ 20 个一批交给 `encode_batch`(→ `batch-cancel-orders`);`encode` 对 `CancelAll`(`inst = 0` 与按 inst 都)返回 `EncodeErr::Unsupported(CancelAll)`([hft-venue 出环](../hft-venue/README.md#出环outmsg--gwcmd) 的展开第 ③ 级);其后运行时以 `ro_request(OpenOrders)`(即 `orders-pending`)查一次挂单列表,经 `decode_rest(Sweep)` 产出仍挂着的本系统订单的 `QueryResult`。`encode_batch`:`cs.len() > 20` → `EncodeErr::FieldOverflow`;`cs` 的 `op` 不全相同或不属于 `Place`/`Cancel`/`Amend` → `EncodeErr::Unsupported(<首个不符的 op>)`。
- **REST 签名**(查单、`orders-pending`、CAA、账户查询):头 `OK-ACCESS-KEY`、`OK-ACCESS-SIGN = Base64(HMAC-SHA256(secret, ts + METHOD + requestPath(含 query) + body))`、`OK-ACCESS-TIMESTAMP = ts`、`OK-ACCESS-PASSPHRASE`,`ts` = `WallClock.unix_ms` 的 UTC ISO 8601 毫秒形式(如 `2026-09-21T14:13:20.000Z`)([官方]);模拟盘另加 `x-simulated-trading: 1`;`RestReqBuf.redacted = true`。
- **REST 响应**(`decode_rest`,槽与操作取自 `RestOrigin`):查单(`RestWhat::Order`)与 `orders-pending`(`RestWhat::Sweep`;只取 `clOrdId` 可解析为本账户的订单)的每个订单对象产出一条 `OrderEvent{what: QueryResult}`(`cum_filled ← accFillSz`、`avg_px ← avgPx`、`exch_order_id ← ordId`;订单状态写进 `detail`([契约 28](../hft-venue-proto/README.md#实现方必须遵守的契约)):`live` → New(1)、`partially_filled` → PartiallyFilled(2)、`filled` → Filled(3)、`canceled`/`mmp_canceled` → Cancelled(4));查单返回 `51603`(订单不存在,响应不带 `clOrdId`)→ 以 `RestOrigin.req` 的槽产出 `QueryResult` 的 NotFound 形态(`detail = 7`),不定终态(契约 10)。CAA 响应(`RestWhat::DeadMan`)`code = "0"` → 零事件;`code ≠ "0"` → 零事件、`ReportOutcome.raw_code` 按契约 8 编码。

### 限速默认值

| 范围 | 值 |
|---|---|
| 下单/改单/撤单 | 60 req / 2 s / UID+instId(与 REST 共享) |
| 批量 | 300 单 / 2 s |
| 子账户合计新单 + 改单 | 1000 / 2 s;VIP5+ 按 7 日 fill ratio 放宽到 10 000 / 2 s |
| WS 建连 | 3 次 / s / IP |
| 订阅/退订/登录 | 480 次 / h / 连接 |
| 私有频道连接 | 每子账户每频道 ≤ 30 |
| 空闲 | 30 s 无数据断开 |
| `cancel-all-after` | 1 次 / s / UID([官方]) |
| `account/config`、`account/balance`、`account/positions` | 待核实(官方页有,未摘录;开发备忘第 5 条) |

来源 [research/02 §3](../../design/research/02-exchange-connectivity.md);限速被拒返回 `50011`([官方])。

`rate_rules()` 按上表依次给出 6 条:`(Inst, Orders, 2000 ms, 60)`、`(Inst, Orders, 2000 ms, 300)`(批量按单计)、`(Account, Orders, 2000 ms, 1000)`、`(Ip, NewConns, 1000 ms, 3)`、`(Conn, WsInMsgs, 3 600 000 ms, 480)`、`(Uid, Weight, 1000 ms, 1)`(`cancel-all-after`);私有频道连接数与空闲断开不是速率,不进表;账户查询类端点的限速在开发备忘第 5 条有结论后追加。响应不带限速余量:`ReportOutcome.rate = None`,只有 `50011` 时为 `Some(RateObs{bucket: Orders, used = limit, limit, window_ms})`(取生效 `RateLimitTable` 中第一条 `Orders` 行)。

### 分档快照流

core 的深度来源([hft-venue-proto 契约 26](../hft-venue-proto/README.md#实现方必须遵守的契约));`RouteSpec.topn_levels = [5]`,`feed.topn_levels` 大于 5 时也只有 5 档。

| 产品线 | 频道 | 档位 | 节拍 | 序号 / 时间 | 路由与端点 | 来源 |
|---|---|---|---|---|---|---|
| 两者 | `books5` | 5 | 约 100 ms 一次、有变化才推(待核实,开发备忘第 18 条) | `ts`(ms);是否带单调的 `seqId` 待核实(有则 `Snapshot`,否则 `ByTime`;开发备忘第 18 条) | `public-topn`,`/public`;订阅不带 `id`(实现说明第 2 点) | 频道名与档位:官方(推送顺序表);其余待核实 |

档位 `[px, qty, "0", numOrders]`,按 refdata 线格式指数解析。订阅后首帧即当前快照(待核实,开发备忘第 18 条)。

### L2 序号归一化(`L2Batch.prev_id/last_id`)

增量深度(可选扩展)用;规格保留,`feed.depth = incremental` 时才生效。

| 流 | `prev_id` | `last_id` | 快照 `snap_id` |
|---|---|---|---|
| `books*` `action=update` | `prevSeqId` | `seqId` | `action=snapshot` 帧的 `seqId` |
| SBE `books-l2-tbt`(optional) | `prevSeqId` | `seqId` | REST `books-sbe` 的序号 |
| 快照后第一条 | `snap_id`(`RxMeta.prev_override`;流内快照时恰好等于 `prevSeqId`) | `seqId` | — |

`seqId == prevSeqId` 的保活帧 `last_id ≤ 已转发` 被当作重复丢弃。契约见 [hft-venue-proto](../hft-venue-proto/README.md#实现方必须遵守的契约)。

### 累计量与费用来源

| 流 | `cum_filled` | `avg_px` | 费用 |
|---|---|---|---|
| `orders` 推送(两产品线) | `accFillSz`(按 `qty_wire_exp` 解析,直接得资产单位) | `avgPx` | `fillFee`(本笔增量;**负数 = 扣费**,本层取反为正)+ `fillFeeCcy`;不产出 `FeeOnly` |
| REST 查单 | `accFillSz`(同上) | `avgPx` | `fee`(累计,只作校验) |

成交去重键 `(inst, tradeId)`(`tradeId` 按 instId 编号,跨品种可能撞号);`tradeId` 为空、0 或负时改用 `(inst, ordId, accFillSz)`(`GwCtx` 每账户 `TradeIdRing<1024>`,[契约 22](../hft-venue-proto/README.md#实现方必须遵守的契约));资金费推送的键见"用户流事件表"。`orders` 推送直接给累计量 `accFillSz`,不需要合成。

### 结果未知码表

`src/codes.rs` 两张表([契约 8](../hft-venue-proto/README.md#实现方必须遵守的契约))。码的形态:纯数字字符串(`"50004"`),`reject_code`/`raw_code` 按契约 8 解析为十进制整数。WS 响应顶层 `code ∈ {"0", "1", "2"}`(成功 / 失败 / 批量部分成功),逐单结果看 `data[].sCode`;二者都按下表判定。

| 表 | 码 | 来源 |
|---|---|---|
| 结果未知 | `50001`(503 服务暂不可用)、`50004`(端点超时,"不代表成功或失败")、`50013`(系统繁忙)、`50026`(500 系统错误),以及**任何未登记码** | 官方 |
| 确定拒绝 | `50011`(限速,请求未处理)、`50036`(`expTime` 早于系统时间)、`50037`(订单已过期)、`50017`–`50022`(仓位/币种/账户因 ADL 或强平被冻结;另给 `hint = MarginNow`)、`50027`/`50029`(账户受限)、`51006`(价格超出限价带)、`51008`(余额或保证金不足)、`51400`(撤单失败:订单已成交、已撤或不存在)、其余 `51xxx` 参数/余额/价格带类(逐码在金样中登记) | 官方(`51006`、`51008`、`51400` 已登记;其余 `51xxx` 逐码待登记) |

- 主码与子码:`sCode` 带子码(官方错误码表的 `"51008_1000"` 形态)或响应另带 `subCode` 时,按主码(`_` 之前)查表并以主码编码 `reject_code`/`raw_code`(`"51008_1000"` → 51008);子码只在 raw journal。
- 按操作产出:`order` 的 `sCode = "0"` → `Ack`;`cancel-order` 与 `amend-order` 的 `sCode = "0"` → 零事件(`CancelAck`/`AmendAck` 来自 `orders` 推送);确定拒绝码 → 对应操作的 `Reject` / `CancelReject` / `AmendReject`(`51400` 只出现在撤单上,属 `GONE` 类:`CancelReject{detail = GONE}`;其余确定拒绝码在撤改上 `detail = 0`);结果未知码 → `outcome_unknown`。
- 批量响应(`code = "1"` 全部失败、`"2"` 部分成功):逐条 `data[]` 按上一条判定,`slot/gen/epoch` 取自该条的 `clOrdId`;结果未知的条目由 `decode_report` 直接产出 `OrderEvent{what: OutcomeUnknown, reject_code = raw}`([契约 8](../hft-venue-proto/README.md#实现方必须遵守的契约) 的批量规则)。
- 控制提示([契约 30](../hft-venue-proto/README.md#实现方必须遵守的契约)):`50011` → `ctl = RateLimited`(照常 `Reject`);私有连接上的 `64008` → `ctl = ServerShutdown`;时间戳越界类码未登记(`expTime` 过期的 `50036` 只作确定拒绝),`ClockSkew` 不产出;OKX 没有用户流终止事件。顶层 `code` 属结果未知表且 `data` 为空 → `outcome_unknown` = 请求 id 中的第一单。

### VenueCaps 默认值

| 字段 | `OkxSwap` | `OkxSpot` | 来源 |
|---|---|---|---|
| `fee_asset_rule` | `Quote`(USDT 结算币) | `ReceivedAsset`(账户 `feeType = 0`,默认);refdata 可覆盖为 `Quote` 并要求 `feeType = 1`(只对现货生效) | 官方(`feeType` 语义与设置接口) |
| `reduce_only` | 是 | 否 | 官方 |
| `required_position_mode` | 净持仓(`posMode = net_mode`) | 不适用 | 官方 |
| `required_margin_mode` | 全仓(下单 `tdMode = cross`;已有持仓 `mgnMode` 须为 `cross`);多资产关闭(`acctLv = 2` 合约模式) | 不适用(下单 `tdMode = cash`) | 官方 |
| `required_spot_borrow` | `None`(永续) | `Some(false)`:`enableSpotBorrow = false`;`acctLv = 3` 时另要求 `autoLoan = false` | 官方(字段);`acctLv = 2` 下的语义待核实(开发备忘第 14 条) |
| `funding_settle` | `BalanceOnlyPush`(`balance_and_position` 的 `eventType = funding_fee`;官方说明"只有余额变化时只推 `balData`",故预期不带 `instId`,待核实) | 不适用 | 官方 + 待核实(开发备忘第 1 条) |
| `margin_data` | `Push`(`account` + `positions` 频道) | 不适用 | 官方 |
| `own_liquidation_marker` | `orders` 推送 `category ∈ {full_liquidation, partial_liquidation, adl, delivery}` | 无(`acctLv` 1/2 下现货为现金,无强平) | 官方(字段与枚举) |
| `ignorable_user_events` | `channel-conn-count`、`channel-conn-count-error` | 同左 | 待核实(开发备忘第 9 条) |
| `amend` | 是 | 是 | 官方 |
| `amend_qty_down_only` | 否 | 否 | 官方(`amend-order` 可改价与量) |
| `post_only` | 是(`ordType = post_only`) | 是 | 官方 |
| `ioc` | 是 | 是 | 官方 |
| `batch_cancel` | 是(≤ 20) | 是(≤ 20) | 官方 |
| `dms` | `CountdownAccount`(CAA,REST,账户级;时长取下方 `dead_man()`) | 同左(CAA 对账户内全部订单簿类产品生效) | 官方 |
| `fok` | 是(`ordType = fok`) | 是 | 官方 |
| `testnet` | `Demo`(端点与请求头见"测试网/模拟盘") | 同左 | 官方 |

`default_caps()` 的编码:`own_liquidation_marker`:`OkxSwap` = `LiqMarker::OrderTypeField`、`liq_marker_arg = "full_liquidation|partial_liquidation|adl|delivery"`,`OkxSpot` = `LiqMarker::NotIdentifiable`、`""`;`required_multi_asset`:`OkxSwap` = `Some(false)`、`OkxSpot` = `None`;`flags = AMEND | POST_ONLY | IOC | FOK | BATCH_CANCEL`(31,两者);`dms = CountdownAccount`、`dms_min_ms = 10000`、`dms_max_ms = 120000`、`dms_timeout_ms = 30000`、`dms_refresh_ms = 5000`(两者);`testnet = Demo`;现货 `funding_settle = NotApplicable`、`margin_data = NotApplicable`。

`stale_guard()` = `None`(核实前;[契约 10](../hft-venue-proto/README.md#实现方必须遵守的契约):只有 `expTime` 被核实为在撮合入口之前拒绝过期请求,才改为 `ExpireAt{default_ms: 1500}`,届时非对冲、非链接槽的"不存在"可在 grace 后定终态;核实方法见开发备忘第 16 条);`dead_man()` = `CountdownAccount{min_ms: 10000, max_ms: 120000, timeout_ms: 30000, refresh_ms: 5000, via: Rest}`(`timeout_ms ≥ 2 × refresh_ms`;账户级计时器,与有无挂单无关,每次都刷新;一个账户只启用一个产品线,由该产品线的 gw 刷新);线上 `timeOut` 以秒填写 = `timeout_ms / 1000`(默认值都是整秒)。`dead_man_refresh` 每次都产出一个 `POST /api/v5/trade/cancel-all-after` 请求(`insts` 不参与);`timeout_ms` 不是 1000 的整数倍或不在 [`min_ms`, `max_ms`] 内 → `EncodeErr::FieldOverflow`,不产出请求(`timeOut = 0` 即关闭死手,不由刷新表达)。

### 账户配置核验

`acct_queries(Config)`([契约 16](../hft-venue-proto/README.md#实现方必须遵守的契约)):

| 步 | 请求 | 字段 → 判定 | 来源 |
|---|---|---|---|
| 0 | `GET /api/v5/account/config` | `uid` ≠ `AccountMeta.venue_uid` → `UID`;`perm` 含 `withdraw` 或不含 `trade` → `KEY_PERM`;`acctLv`:`OkxSwap` 账户要求多资产关闭时须为 `2`、开启时须为 `3`,`1`(现货模式,不能交易永续)/`4`(组合保证金)→ `ACCT_TYPE`;`OkxSpot` 账户须为 `1` 或 `2`(现货为现金),`3`/`4`(现货余额参与保证金)→ `ACCT_TYPE`;`posMode ≠ net_mode` → `POS_MODE`(只对 `OkxSwap`);`feeType` 与现货 `fee_asset_rule` 不符 → `FEE_RULE`;`acctLv = 3` 且 `autoLoan = true` → `AUTO_BORROW`;`OkxSpot` 账户 `enableSpotBorrow ≠ false`(现货模式借币开关)→ `AUTO_BORROW`,字段缺失 → `UNREADABLE`(该字段在 `acctLv = 2` 下的含义待核实) | 官方(`enableSpotBorrow` 在 `acctLv = 2` 的语义待核实,开发备忘第 14 条) |
| 1(`OkxSwap`) | `GET /api/v5/account/positions?instType=SWAP` | 任一 refdata 可交易 inst 的持仓 `mgnMode = isolated` → `MARGIN_MODE`(`margin_mode` 填 `MarginMode::Mixed`) | 官方(字段);权重待核实(开发备忘第 5 条) |
| 2(`OkxSwap`) | `GET /api/v5/account/leverage-info?instId=<≤ 20 个,逗号分隔>&mgnMode=cross`,按 refdata 可交易永续分批 | 每个 inst 的 `lever` → 一条 `LeverageItem`;某批失败 → 该批 `leverage = 0` 并置 `UNREADABLE` | 官方(端点);批量上限与权重待核实(开发备忘第 14 条) |

**UID**:登录 WS 与 REST 签名用同一把 API key,`session_identity_request` 返回 `Ok(false)`(`decode_identity` 不被调用,调用即 `Err(DecodeErr::UnknownClass)`),步 0 的 `uid` 同时覆盖两种用途(`uid_keys = SESSION | REST`,[契约 23](../hft-venue-proto/README.md#实现方必须遵守的契约))。`uid_policy = "unverifiable"` 的账户(只在模拟盘,规则归 [hft-refdata](../hft-refdata/README.md#数据结构))不核对 UID:`uid` 缺失或为空时 `UID`、`UNREADABLE` 都不因它置位,`uid_keys = 0`,`venue_uid` 填返回值(缺失为空)。

`position_mode ← posMode`,`multi_asset ← (acctLv ∈ {3, 4})`,`margin_mode`:永续由下单 `tdMode = cross` 保证,只在发现逐仓持仓时为 `MarginMode::Mixed`;现货三项都填 `NotApplicable`。`OkxSwap` 的 `acctLv` 为 2/3 但与多资产要求相反(要求关闭而为 3、要求开启而为 2)→ `MULTI_ASSET`(不是 `ACCT_TYPE`);`feeType` 只对 `OkxSpot` 核对(`ReceivedAsset` 要求 0、`Quote` 要求 1)。各请求的名义权重 `AcctQuery.weight = 1`(OKX 按请求计数)。OKX 没有账户配置变化的推送:运行中依赖 `gw.acct_cfg_recheck_s` 周期重查;下单被拒码表明持仓模式不符(待登记)时给 `hint = ConfigRecheck`;杠杆在交易所被人工改动后,最迟一个重查周期反映到杠杆条目。

### 保证金数据换算

`margin_data = Push`([契约 17](../hft-venue-proto/README.md#实现方必须遵守的契约)):`account` 频道(按事件推送,约 50 ms 聚合一次,另有定期推送[官方])与 `positions` 频道(`OkxSwap`,`instType = SWAP`)在归属连接上订阅;`GwCtx` 缓存最近的账户级字段与每 inst 条目,任一推送更新即产出一份完整 `MarginReport`(`source = Push`;尚未收到 `account` 推送时账户级字段为 0、`margin_ratio = Rate::MAX`)。推送超过 `gw.margin_push_stale_ms` 未到 → 兜底请求 `GET /api/v5/account/balance` + `GET /api/v5/account/positions?instType=SWAP`(`acct_queries(Margin)` 的步 0、步 1,都是 `push_fallback = true`、`every = 1`、`weight = 1`;`OkxSpot` 为空表);兜底结果是一份独立完整的 `MarginReport{source = Poll}`,不写入推送缓存(契约 27)。`margin_ratio` 一律按契约 17 由本表的 `maint_margin / margin_balance` 算出(i128 计算 `maint × 10⁹ / balance`,向零取整;`margin_balance ≤ 0` → `Rate::MAX`);交易所的 `mgnRatio`(OKX 口径 = 权益 / 维持保证金)只作校验对照,不直接采用。

| `MarginReport` 字段 | `acctLv = 2`(合约模式,默认) | `acctLv = 3`(多币种保证金,覆盖时) | 来源 |
|---|---|---|---|
| `margin_balance` | `details[ccy=USDT].eq`(币种权益,含未实现盈亏) | `adjEq`(折算后有效权益,USD) | 官方 |
| `maint_margin` | `details[ccy=USDT].mmr` | `mmr` | 官方 |
| `avail_margin` | `details[ccy=USDT].availEq`(为空时取 `availBal`) | `availEq` | 官方 |
| `margin_ratio` | `details[ccy=USDT].mmr / details[ccy=USDT].eq`(`mmr` 为空即无全仓持仓 → 0);对照值 `1 / details[ccy=USDT].mgnRatio` | `mmr / adjEq`;对照值 `1 / mgnRatio`(`mgnRatio = adjEq / mmr`) | 官方(字段);计算按契约 17 |
| 条目 `liq_px` | `positions[].liqPx`(空 → 0) | 同左 | 官方 |
| 条目 `adl_quantile` | `positions[].adl`(0..5,数字越大 ADL 强度越高)× 20 → 0..=100;空 → 255(未知);> 5 → `DecodeErr::Shape` | 同左 | 官方 |

口径按推送本身判定(`GwCtx` 不读账户配置,契约 27):`account` 推送顶层 `adjEq` 非空 → 多币种保证金口径,为空 → 合约模式口径。

`liquidation-warning` 频道(`instType = SWAP`;现有采集系统已在生产订阅)推送 → `hint = MarginNow`(立即执行一次兜底查询)。`balance_and_position` 的 `eventType ∈ {liquidation, adl}` 同样给 `MarginNow`。

### 外部成交识别

`own_liquidation_marker` 的实现([契约 18](../hft-venue-proto/README.md#实现方必须遵守的契约))。成交类消息 = `orders` 推送且 `fillSz > 0`。我方订单都带 `clOrdId` 且 `orders` 推送回显该字段(待核实回显在全部状态下都存在,开发备忘第 3 条),所以 `clOrdId` 为空或非我们格式即外部成交。

| 判定(按顺序) | `cause` | 来源 |
|---|---|---|
| `category ∈ {full_liquidation, partial_liquidation}` | `Liquidation` | 官方 |
| `category = adl` | `Adl` | 官方 |
| `category = delivery` | `Delivery` | 官方 |
| `parse_cid(clOrdId)` 成功 | 本系统订单 → `OrderEvent` | — |
| `clOrdId` 为空或非我们格式 | `Unknown`(网页/App 单是否有可识别标记待核实;有则改 `Manual`) | 待核实(开发备忘第 3 条) |

`ExternalFill` 字段:`side ← side`、`qty ← fillSz`(按 `qty_wire_exp` 解析)、`px ← fillPx`、`fee ← −fillFee`、`fee_asset ← fillFeeCcy`、`exch_order_id ← ordId`、`trade_id ← tradeId`(空、0 或负填 0)、`ex_ts_ns ← fillTime`;去重键同上(强平/ADL 成交的 `tradeId` 是否为空待核实,开发备忘第 15 条;为空即走 `(inst, ordId, accFillSz)`)。强平罚金按持仓累计字段 `liqPenalty` 只作校验(不进 `ExternalFill`)。`clOrdId` 不可解析且 `fillSz = 0` 的推送(外来单的挂单、撤单等状态变化)不产出事件。

### 用户流事件表

私有频道(归属连接上订阅,按该账户启用的产品线):`orders`(`OkxSwap` 账户 `instType = SWAP`,`OkxSpot` 账户 `SPOT`)、`account`、`balance_and_position`;只对 `OkxSwap`:`positions`(`SWAP`)、`liquidation-warning`(`SWAP`)。

| 消息 | 产出 | 提示 |
|---|---|---|
| `orders` 推送 | `OrderEvent` 或 `ExternalFill` | — |
| `positions` 推送 | 每 inst 一条 `PositionReport`(只校验);更新 `GwCtx` 的保证金条目并产出 `MarginReport` | — |
| `account` 推送 | `BalanceReport`(归属该账户启用的 venue;`details[].cashBal`、`frozenBal`;只校验);`OkxSwap` 账户另更新账户级字段并产出 `MarginReport` | — |
| `balance_and_position`,`eventType = funding_fee` | `FundingSettle{inst: INST_NONE, amount ← balData[ccy = USDT] 的余额变化, settle_ts_ns ← pTime, synth: 0}`(若核实带 `posData.instId`,改为带 inst);去重键 `TradeKey::Funding{settle_ts_ns ← pTime, disc ← balData[ccy = USDT].cashBal 的 raw 定点}`([契约 22](../hft-venue-proto/README.md#实现方必须遵守的契约);余额变化的取法、`pTime` 是否等于结算时刻待核实,开发备忘第 1 条) | — |
| `balance_and_position`,`eventType ∈ {liquidation, adl}` | 无(成交以 `orders` 为准) | `MarginNow` |
| `balance_and_position`,其他 `eventType` | 无 | — |
| `liquidation-warning` 推送 | 无 | `MarginNow` |
| `{"event":"login"|"subscribe"|"unsubscribe"|"error"}` | 无(控制回执) | — |
| `{"event":"notice","code":"64008"}` | 无 | 行情连接:`ControlHint::ServerShutdown`;私有连接:`ctl = ServerShutdown`(预告后计划切换) |
| `channel-conn-count`、`channel-conn-count-error` | 无(`skip = Listed`) | — |
| 其他 | 无(`skip = Unknown`) | — |

- 字段映射:`positions` → `PositionReport{inst ← instId, qty ← pos(按 qty_wire_exp,净持仓带符号), entry_px ← avgPx, upnl ← upl, liq_px ← liqPx(空 → 0), ex_ts_ns ← uTime}`;`account` → 每个 refdata 中存在的 `details[].ccy` 一条 `BalanceReport{asset ← ccy, free ← cashBal − frozenBal, locked ← frozenBal, ex_ts_ns ← uTime}`;不在 refdata 的 `instId`/`ccy` 跳过。
- 资金费余额变化:`GwCtx` 记住最近一次 `account` 或 `balance_and_position` 推送里 USDT 的 `cashBal`(基线);`funding_fee` 推送的 `amount` = 本条 `cashBal` − 基线(换算到 `Notional`),随后基线更新为本条值;进程内尚无基线 → 不产出 `FundingSettle`(该时刻由 sequencer 照常合成,reconciler 校正)。

资金费按币归属由 sequencer 合成;reconciler 用账单 `GET /api/v5/account/bills`(`subType` 173 资金费支出 / 174 资金费收入,带 `instId`[官方])校正。

### 测试网/模拟盘

| 项 | 内容 | 来源 |
|---|---|---|
| 端点 | WS `wss://wspap.okx.com:8443/ws/v5/{public,private,business}`;REST 同生产域名,请求头 `x-simulated-trading: 1` | 官方 |
| key | 模拟盘单独创建 API key(不过期) | 官方 |
| refdata | `instIdCode` 可能与实盘不同,模拟盘用单独生成的 refdata | 官方 |
| 限制 | 部分功能不可用(错误码 `50038`);CAA、`account/config` 是否可用待核实;`account/config` 读不到 `uid` 时该模拟盘账户按 `uid_policy = "unverifiable"` 配置(规则归 [hft-refdata](../hft-refdata/README.md#数据结构)) | 官方 / 待核实(开发备忘第 5 条) |
| 行情与符号 | 模拟盘行情走 `wspap` 的 `/public`(与模拟盘撮合同源;与主网盘口的差异待核实,开发备忘第 17 条);`instId` 与实盘同名、可交易品种集合与 `instIdCode` 可能不同,模拟盘 refdata 由 hft-cfg 按模拟盘 instruments 接口单独生成 | 官方(`instIdCode`)/ 待核实(品种集合、行情差异) |

上线门槛第 ⑥ 项([testing §8](../../docs/testing.md#8-上线门槛))在模拟盘连续 3 天冒烟,行情与下单都用模拟盘(`feed_env = "testnet"` + 模拟盘 refdata)。

### 心跳与 BBO 探测

按连接种类分别写([契约 24](../hft-venue-proto/README.md#实现方必须遵守的契约)):

| 连接 | 服务端行为 | 我们的动作 | 来源 |
|---|---|---|---|
| JSON `/public`、`/private`、`/business` | 30 s 无数据断开;服务端不主动 ping | 25 s 无入站发文本 `ping`,10 s 内须回 `pong`(`HeartbeatSpec`,实现说明 §5) | 官方 + 现有采集系统生产 |
| SBE `/public-sbe`(optional) | 每 20 s 发 WS Ping,60 s 无 Pong 断开 | 回 Pong | 官方 |
| BBO 连接(`bbo-tbt`,双路) | 同 JSON `/public` | 另每 `feed.bbo_ping_ms` 一次探测:`bbo_probe()` 默认 WS Ping;若核实公共端点不回 WS Pong,改为文本 `ping`(回 `pong`)。探测不属于订阅/登录类请求,不占 480 次/h 的额度 | 待核实(开发备忘第 15 条) |

BBO 补帧:`bbo-tbt` 订阅后是否立即推当前值待核实(开发备忘第 15 条);核实前 `bbo_snapshot_request` 返回 `Ok(true)`,每 inst 一个 `GET /api/v5/market/books?instId=&sz=1`(`seqId` 作 `update_id`;限速待核实);`BBO_SNAPSHOT_BATCH = 1`,`insts.len() > 1` → `Err(ProtoErr::BufFull)`。`decode_bbo_snapshot` 产出一条 `Bbo{h.inst ← insts[0](响应不带 instId), bid ← bids[0], ask ← asks[0], update_id ← seqId, ex_ts_ns ← ts}`,返回 `Ok(1)`;响应 `code ≠ "0"` 或 `data` 为空 → `Ok(0)`。

`heartbeat(role)` 对 `Md`、`Order` 两种 WS 角色都返回实现说明 §5 的 `HeartbeatSpec`;`app_ping` 写 4 字节 `ping` 并返回 4;`bbo_probe()` = `PingKind::WsPing`(核实前)。

### 只读查询

`ro_request`/`ro_decode` 的实现([契约 31](../hft-venue-proto/README.md#实现方必须遵守的契约));reconciler 以只读 key 构建 `Templates` 调用;gw 只用 `OpenOrders` 做 `CancelAll` 兜底(`decode_rest(Sweep)`)。签名同"下单"一节的 REST 签名;分页与字段待核实(开发备忘第 20 条)。

| `RoKind` | 端点(两个产品线,`instType` 按产品线取 `SWAP`/`SPOT`) | 字段映射 |
|---|---|---|
| `OpenOrders` | `GET /api/v5/trade/orders-pending?instType=` | 每单 `open_order`:`clOrdId` 经 `parse_cid` 定槽,`accFillSz` → `cum_filled`,`state` → `detail` |
| `Fills` | `GET /api/v5/trade/fills`(近 3 天)/ `fills-history`(更早),游标 = 最后一条 `billId` | 每笔 `fill`:`tradeId`、`ordId`、`fillSz`(按 `qty_wire_exp`)、`fillPx`、`−fee`/`feeCcy`、`ts`;`clOrdId` 可解析时 `cid = Some` |
| `FundingFlow` | `GET /api/v5/account/bills?type=8`(资金费;`subType` 173/174),游标 = 最后一条 `billId`;`OkxSpot` → `Unsupported` | 每笔 `funding`:`instId` → inst,`balChg` → amount,`ts` → `settle_ts_ns` |
| `Balance` | `GET /api/v5/account/balance` | `details[]` 每币 `balance`(`cashBal − frozenBal` / `frozenBal`) |
| `Positions` | `GET /api/v5/account/positions?instType=SWAP`;`OkxSpot` → `Unsupported` | 每 inst `position`,映射同"用户流事件表"的 `positions` |

### `mock` 特性

与 [venue-binance](../venue-binance/README.md#mock-特性给-mock-exchange-的反向编解码) 相同的四个反向编解码函数(`encode_market`、`decode_order_frame`、`encode_order_event`、`encode_rest_response`),供 optional 的 [mock-exchange](../../apps/mock-exchange/README.md) 使用。

## 实现说明

<details><summary>实现要点</summary>

### 1. 分派

JSON 推送 `{"arg":{"channel":"bbo-tbt","instId":…},"data":[…]}`,频道名在前 32 B 内;带 `"action":"snapshot"|"update"` 的是深度。`{"event":"subscribe"|"error"|"login"` 为控制回执;`{"event":"notice","code":"64008"` 是服务升级断线预告。文本 `pong` 是应用心跳回应。私有连接:`{"id":` 开头为下单响应,`{"arg":{"channel":"<名>"` 为私有推送,按"用户流事件表"分类;`{"event":"<名>"` 中不认得的事件名先查生效 `VenueCaps.ignorable_user_events`,否则 `Unknown`。SBE:按 OKX 发布的 schema 读消息头模板号,`instIdCode` 直接映射 `InstId`(refdata 预建数组表,不走字符串哈希)。

### 2. 订阅约束

订阅请求**不带顶层 `id`**(带了会被 `60033 Parameter id error` 拒绝,现有采集系统已在生产验证);回执按回显的 `arg` 关联。每帧 ≤ 50 个 `args`。core 深度订阅 `books5`(所有等级可用待核实,开发备忘第 18 条);增量深度(可选扩展)按等级选择:VIP4+ 用 `books-l2-tbt`,否则 `books`;被拒(权限不足)时退到下一档并上报 `ControlHint::SubAck{ok:false}`。私有频道订阅由 `private_subscribe` 生成,只发给归属连接。

### 3. 订单帧模板

```text
{"id":"p0000002a00000003","op":"order","expTime":"1790000001500",
 "args":[{"instIdCode":10459,"tdMode":"cross","side":"sell","ordType":"ioc","sz":"4","px":"65000.1",
          "clOrdId":"1a2b3c4d5e6f708192a3b4c5","reduceOnly":false}]}
```

`reduceOnly` 只在 `GwCmd.oflags` 带 `REDUCE_ONLY` 时为 true,且只出现在 `OkxSwap` 模板;`OkxSpot` 模板的 `tdMode` 固定 `cash`,带 `REDUCE_ONLY` 的命令 → `EncodeErr::Unsupported`。请求 `id` 只允许字母数字(≤ 32),编码 `(op, slot, gen)`;`clOrdId` 同样只允许字母数字,不能用 `-`。帧长 ≤ 400 B;批量帧 ≤ 20 单,`ORDER_FRAME_CAP = 8192`。

### 4. 回报

| 输入 | `OrderEvent.what` |
|---|---|
| 响应 `code="0"`,`sCode="0"` | `Ack`(`exch_order_id ← ordId`,`ex_ts_ns ← outTime`) |
| `sCode ≠ "0"` 且登记为确定拒绝 | `Reject{reject_code ← sCode}` |
| `code`/`sCode` ∈ 结果未知表或任何未登记码 | 不产出 Reject,`outcome_unknown` |
| `orders` 推送 `state=live` | `Ack`(幂等;post_only 的 `live` 在上簿后约 1 ms 才推,2026-08-20 起) |
| `state=partially_filled / filled`,`fillSz>0` | 先过"外部成交识别";本系统订单 → `PartialFill` / `Fill`(`qty ← fillSz`、`cum_filled ← accFillSz`,两者按 `qty_wire_exp` 解析;`px ← fillPx`,`avg_px ← avgPx`,`fee ← −fillFee`,`fee_asset ← fillFeeCcy`,`ex_ts_ns ← fillTime`) |
| `state=canceled / mmp_canceled` | `CancelAck`;IOC/FOK 剩余撤销 → `Expired`(按 `ordType ∈ {ioc, fok}` 判定) |
| `amendResult` 非空(改单结果推送) | `"0"` → `AmendAck`;`"-1"`、`"1"` → `AmendReject`(`"1"` 的同一推送 `state = canceled` 另产出 `CancelAck`);该推送的 `state = live` 不再产出 `Ack` |

同一推送 `fillSz > 0` 而 `state ∈ {canceled, mmp_canceled}` 时先产出 `PartialFill`,再按 `state` 产出 `CancelAck`/`Expired`。`classify_report` 只认编译进来的默认可忽略名单(`default_caps()`);refdata 追加的名字由 `decode_report` 按生效 `ignorable_user_events` 判为 `skip = Listed`。

`inTime/outTime` 的差值随 `ReportOutcome` 交给 gw 作为交易所内部延迟样本。`fillFeeCcy` 不在 `fee_asset_rule` 允许集合内(如现货 `feeType` 被人工改为 1)→ `fee_rule_deviation = true`。

### 5. 心跳与会话

`HeartbeatSpec{ping: AppText("ping"), interval_ms: 25000, unconditional: false, pong_timeout_ms: 10000, idle_timeout_ms: 30000}`:25 s 无入站则发 `ping`,10 s 内须回 `pong`(与现有采集系统一致);SBE 连接由服务端 20 s ping、60 s 无 pong 断。死手:`cancel-all-after` 只有 REST 形式([官方]),每 `refresh_ms` 由 sequencer 的 `DeadMan` 定时器驱动刷新一次(官方建议约每秒心跳;我们以 30 s 超时 + 5 s 刷新为默认,超时取值以 refdata 为准)。

### 6. 源文件规划

| 文件 | 职责 |
|---|---|
| `src/lib.rs` | `OkxSwap`、`OkxSpot` |
| `src/caps.rs` | 两个产品线的 `default_caps()` |
| `src/cid.rs`、`src/sign.rs` | clOrdId/请求 id;登录与 REST 签名 |
| `src/md_json.rs` | bbo-tbt、books5、trades、mark/funding/index;增量 books*(可选扩展) |
| `src/md_sbe.rs` | SBE 头与三个模板;`books-sbe` 快照(optional) |
| `src/orders.rs` | 模板与 `encode`(instIdCode、expTime、张数、tdMode) |
| `src/reports.rs` | 响应、`orders`、`positions`、`account`、`balance_and_position`、`liquidation-warning`、用户流事件表 |
| `src/margin.rs` | 推送缓存与 `MarginReport` 组装(两种 `acctLv` 口径) |
| `src/acct.rs` | Config 序列与 Margin 兜底请求 |
| `src/external.rs` | `category` 判定与外部成交 |
| `src/codes.rs` | 两张表:确定拒绝 / 结果未知(未登记默认结果未知) |

</details>

## 延迟与资源契约

| 入口 | p50 / p99 预算 | 基准名 |
|---|---|---|
| `classify` | ≤ 30 ns / ≤ 60 ns | `bench_classify_okx` |
| `decode_md` bbo-tbt(≈ 184 B)→ `Bbo` | ≤ 0.4 µs / ≤ 0.8 µs | `bench_decode_okx_bbo` |
| `decode_md` SBE bbo-tbt(optional) | ≤ 80 ns / ≤ 150 ns | `bench_decode_okx_sbe_bbo` |
| `decode_md` books5 → `L2Snapshot{TopN}` | ≤ 0.8 µs / ≤ 1.5 µs | `bench_decode_okx_books5` |
| `decode_md` books-l2-tbt 增量 1 KiB(增量深度,可选扩展) | ≤ 1.5 µs / ≤ 3 µs | `bench_decode_okx_l2` |
| `encode` order | ≤ 0.3 µs / ≤ 0.6 µs | `bench_encode_okx_order` |
| `decode_report` orders 推送(含外部成交判定与去重) | ≤ 1.2 µs / ≤ 2.5 µs | `bench_decode_okx_orders` |
| `decode_report` account 推送 → `MarginReport`(20 币种) | ≤ 8 µs / ≤ 15 µs | `bench_decode_okx_account` |
| `decode_report` positions 推送(10 个持仓) | ≤ 6 µs / ≤ 12 µs | `bench_decode_okx_positions` |

纯函数,热路径零分配零系统调用。`account`/`positions` 推送是本所最大的推送类型,与订单推送同在 `/private` 连接上、在 gw 线程上按到达顺序解码;`bench_decode_okx_account` 与 `bench_decode_okx_positions` 的 p99 不得超过 [hft-venue](../hft-venue/README.md#延迟与资源契约) `venue_gw_margin_push`。实际推送体积待核实(开发备忘第 19 条);超过时把这两个频道移到 standby 之外的第三条只订阅不下单的连接(可选扩展)。

## 测试与验收

用例与验收见 [TESTS.md](TESTS.md)。

## 依赖

`hft-types`、`hft-refdata`(`instIdCode`、线格式指数 `px_wire_exp`/`qty_wire_exp`(含 `ctVal` 平移)、`VenueCaps`)、`hft-event`、`hft-venue-proto`、`sonic-rs 0.5`、`hmac 0.12` + `sha2 0.10`、`base64 0.22`。**禁止**:`hft-net`、`hft-venue`、`tokio`、`serde_json`(dev-only)。

## 延伸阅读

- [research/02 §3 OKX](../../design/research/02-exchange-connectivity.md)、[§11.2](../../design/research/02-exchange-connectivity.md);官方文档本地缓存 `architecture-v2/research/_src/okx_docs.txt`
- [04 §5](../../design/04-行情接入层.md)、[05 §7](../../design/05-CEX执行层.md)
- [hft-venue-proto 契约](../hft-venue-proto/README.md#实现方必须遵守的契约)、[hft-venue](../hft-venue/README.md)、[hft-refdata](../hft-refdata/README.md)、[cookbook/adding-a-venue.md](../../docs/cookbook/adding-a-venue.md)
- 范围:[round8-decisions](../../design/_work/round8-decisions.md)
- 现有知识来源:`E:\trading\Trading\crates\okx`(seqId 规则、订阅不带 id、按 inst 重订阅、ping)、`E:\trading\Trading\apps\collector\src\private.rs`(登录签名、`orders` 与 `liquidation-warning` 私有订阅)

## 已知限制与后续工作

- **core 深度只有 5 档**(`books5`);更深的需求触发增量深度(可选扩展),其中 L2 逐笔需要 VIP4,未达标时只有 `books` 100 ms。
- **SBE 需登录**且无初始快照;先上 JSON,SBE 在对照测试证明解析收益后启用。
- **`fills` 频道**(更早的成交推送)有等级门槛,未纳入;成交以 `orders` 推送为准。
- **Cloudflare 尖峰**:所有连接经 Cloudflare,无法直连源站;p99 基线单独建立,不与 Binance 混算。
- **`acctLv` 范围**:`OkxSwap` 账户只支持 2 与 3(现货模式不能交易永续,组合保证金(4)的保证金口径不同);`OkxSpot` 账户只支持 1 与 2(现货余额参与保证金的 3/4 不覆盖)。
- **CAA 是账户级**:计时器覆盖子账户内全部订单簿类产品。一个账户只启用一个产品线,由该产品线的 gw 刷新;一个 OKX 子账户只归一个 `AccountId`([hft-core 多账户](../../apps/hft-core/README.md#多账户)),所以 CAA 与 `cancel-all` 不会跨账户生效。

### 开发备忘

<details><summary>给实现者</summary>

**待核实**(本机连不上 OKX。方法 A:在 `okx_docs.txt` 缓存里查原文并摘录;方法 B:经东京服务器抓取最新官方页;方法 C:模拟盘实测抓 raw;方法 D:正式实盘开始后从 raw journal 补核——只用于上线后才可能出现的样本,上线前必须确定的项不得依赖它。)

1. `balance_and_position` 的 `funding_fee` 事件是否带 `posData.instId`;余额变化从哪个字段取(`balData` 只给 `cashBal` 时按与上一份的差);`pTime` 是否等于结算时刻,同一结算时刻是否按 inst 分多条推送、`cashBal` 能否作 `disc`;`positions` 推送在结算后 `fundingFee` 累计值是否更新(若是,可按差分产出带 inst 的 `FundingSettle`,`funding_settle` 改为 `PerSymbolPush`):C(模拟盘持仓跨过一次结算);模拟盘不结算时按偏安全的 `BalanceOnlyPush` 上线,D 补核。
2. `category` 为 `full_liquidation`/`partial_liquidation`/`adl` 的 `orders` 推送原文:模拟盘高杠杆小仓制造强平(C);ADL 样本等实盘出现或按文档构造并标"合成"。
3. `orders` 推送在全部状态下都回显 `clOrdId`;网页/App 下单的 `clOrdId` 是否为空、`source` 等字段能否识别人工来源:C;取不到时按 `Unknown` 上线,D 补核。
4. `account` 频道定期推送的周期(决定 `gw.margin_push_stale_ms` 的取值);`liquidation-warning` 的触发阈值:A/C。
5. `account/config`、`account/positions`、`account/balance` 的限速;每条 WS 连接可订阅的频道数与入向消息每秒上限(决定 `routes()` 的 `max_streams`、`max_in_msgs_per_s`,核实前取 100 与 4);模拟盘是否支持 `account/config` 与 CAA:A/C。
6. `bbo-tbt` 是否带 `seqId`(否则 BBO 去重改用 `ts` + 价量比较):A/C。
7. 序号重置时 `seqId < prevSeqId` 的确切表现;按断裂处理进 Resync:A。
8. `funding-rate` 频道的 `fundingTime`/`nextFundingTime` 语义与周期变化的表现:A。
9. 可忽略事件名全集(`channel-conn-count` 等):A;上线后由"未知事件"计数补全。
10. 51xxx 逐码分类(确定拒绝)与"持仓模式不符"的码:A,随金样登记。
11. SBE schema 版本与模板号;`books-sbe` REST 的权重(optional):A。
12. **私有频道双订阅(M3 之前必须核实)**:同一子账户两条 `/private` 连接同时订阅 `orders`(及 `account`、`positions`、`balance_and_position`)是否都收到推送(决定先建后断):C。不允许时切换退化为"先断后建 + VERIFY"并写进本 README。
13. (启用 L2 双路合并时)两条同订阅连接的 `books` 增量是否逐条相同。
14. `enableSpotBorrow` 在 `acctLv = 1`/`2` 下的语义与默认值;`leverage-info` 的批量上限与权重:A/C。
15. 公共行情连接是否回应 WS 协议 Ping(决定 `bbo_probe()`);`bbo-tbt` 订阅后是否立即推当前值(决定 `bbo_snapshot_request`);强平/ADL 成交的 `tradeId` 是否为空:A/C。
16. `expTime` 的生效位置(网关拒绝还是撮合入口拒绝;请求排队期间过期是否仍可能成交):A/C。核实为"撮合入口之前拒绝"后 `stale_guard()` 改为 `ExpireAt`。
17. 模拟盘的可交易品种集合与 `instIdCode` 映射、模拟盘行情是否覆盖 `bbo-tbt`/`books`(门槛 ⑥ 用模拟盘行情):A/C。
18. **`books5`**:推送节拍(固定 100 ms 还是有变化才推)、是否带单调的 `seqId`、订阅后首帧是否立即推当前快照、模拟盘是否提供:A/C。结论写回"分档快照流"表。
19. **保证金推送的体积**:`account`(多币种)与 `positions`(多持仓)推送的字节数分布与频率,决定 gw 上内联解码的成本是否在 hft-venue `venue_gw_margin_push` 之内:C(模拟盘持 10 个以上永续仓位,抓 1 小时 raw 统计)。
20. **只读查询**("只读查询"表):`fills`/`fills-history`、`bills` 的分页参数与每页上限、资金费账单的 `type`/`subType` 取值与 `balChg` 符号、`orders-pending` 的分页:A/C(模拟盘只读 key 逐个调用并抓响应进 fixtures)。

**实现顺序**:`md_json.rs`(bbo-tbt、books5、trades、mark/funding/index)+ 金样(复用现有采集系统 raw)→ `orders.rs`/`reports.rs`/`external.rs` → `margin.rs` → `acct.rs` → 模拟盘冒烟 →(可选扩展)增量 books → SBE(optional)。

**迁移注意**:现有采集系统用 `instId` 字符串;新栈下单必须 `instIdCode`,行情解码可继续用 `instId` 查表或(SBE)用码值。

</details>
