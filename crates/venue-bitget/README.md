---
description: "Bitget UTA(统一账户)USDT 永续与现货的纯协议实现(core):books1/books5/books50 与增量 books 的 seq/pseq 链续、ticker 资金费、每 25 s 无条件应用层 ping、UTA V3 WS 下单帧与回报解码、外部成交识别与 VenueCaps 默认值;无交易所侧死手"
kind: crate
tier: pure
scope: core
status: impl
---

# venue-bitget

## 概要

`venue-bitget` 实现两个 [`VenueProtocol`](../hft-venue-proto/README.md#venueprotocol静态分派):`BitgetUsdtFutures`(USDT 本位永续,`MarketKind::LinearPerp`)与 `BitgetSpot`(现货,`MarketKind::Spot`),两者都是 core,以 **UTA(统一账户)V3** 接口为主(Classic V2 只作核对与行情对照)。它拥有 Bitget 的协议知识:`books1`(1 ms JSON 快照,BBO)、分档快照 `books5`/`books50`(core 深度)、增量 `books` 的 `seq/pseq` 严格链续(可选扩展;UTA 无 checksum)、永续 `ticker` 的标记价/指数价/资金费、**每 25 s 无条件发应用层 `ping`**(research/02 的口径是客户端每 30 s 发一次;服务端 120 s 没收到 ping 就断,即使数据在流,实测约 150 s;取 25 s 留余量)、`login` 签名、UTA V3 WS `place-order`/`modify-order`/`cancel-order` 帧、账户模式核验、保证金与强平价轮询、强平/ADL 成交识别、`clientOid` 格式、限速与 `VenueCaps` 默认值。Bitget **没有交易所侧死手**,由 [kill-switch](../../apps/kill-switch/README.md) 兜底。

**线上事实的来源**:UTA V3 的公共/私有频道、订阅写法、WS 下单帧与响应、REST 签名与订单详情、错误码与枚举(开平类型、撤单原因)取自官方 UTA 文档原文与 2026-10-15 demo 脱敏采样。对应金样及未覆盖范围归 [TESTS.md](TESTS.md);采集客户端的开平不代表 hft-core 联调通过。标"待核实"的是仍需模拟盘确认的项(开发备忘);**官方资料中未找到 Bitget SBE 与 "LOLA" 线路**,本规格不依赖它们。纯函数,运行时见 [`hft-venue`](../hft-venue/README.md)。

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
pub struct BitgetUsdtFutures;   // NAME = "bitget-usdt-futures", VENUE = BITGET_UTA_USDT_FUT, MARKET = LinearPerp
pub struct BitgetSpot;          // NAME = "bitget-spot",         VENUE = BITGET_UTA_SPOT,     MARKET = Spot

// client id(`clientOid`):`render_cid(&ClientOrderId, &mut [u8; 48])` / `parse_cid(&[u8]) -> Option<ClientOrderId>`(VenueProtocol 方法)。
// 编码是 hft-types 的 ClientOrderId(规范 24 位十六进制),本 crate 只加外形:<24hex>,24 字节(官方:`^[0-9A-Za-z_:#\-+\s]{1,32}$`)。
// 请求 id(WS 下单帧的 `id`,响应原样回显):"<op><slot 8 hex><gen 8 hex>"(17 字符,小写十六进制);op:p = Place、m = Amend、c = Cancel(`CancelAll`、`Query` 不编码成帧)。

pub enum MdClass { Books1, BooksTopN /* books5、books50 */, BooksIncr /* 增量 books,可选扩展,路由 public-books */, Trade, Ticker,
                   SubAck /* event = subscribe / unsubscribe / error */, Pong, Unknown }
pub enum ReportClass { WsResp /* event = trade,或 event = error 且带本系统请求 id */, Orders, Fills /* fill / fast-fill,不订阅 */, Positions, Account,
                       Push /* 私有推送但频道名不在前 64 B 内:decode_report 解析 arg 后再按上列分 */,
                       Control /* event = login / subscribe / unsubscribe / error */, Pong, Ignorable, Unknown }

pub struct FeedCtx;   // FeedCtx::new():每 inst 的资金费周期表(256 项定长)
pub struct GwCtx;     // GwCtx::new():启动时分配的订单累计表(4096 项,按 orderId),热路径不分配
```

### 账户形态(官方,本地缓存的 UTA 概览页)

UTA 有三种账户模式:**逐仓模式**(每个仓位独立风险,只支持逐仓)、**基础模式**(只用稳定币作保证金,USDT 本位与 USDC 本位合约共享保证金池,支持全仓/逐仓,现货不作抵押)、**高级模式**(多资产按折算率作保证金)。全仓账户的保证金率 =(维持保证金 + 部分强平手续费)÷ 账户权益——方向与我们的统一口径一致(越大越危险)。默认要求 **基础模式 + 全仓 + 单向持仓**(见"VenueCaps 默认值");Classic 账户不支持。

**一个账户在 Bitget 只启用 `BitgetUsdtFutures` 或 `BitgetSpot` 之一**(refdata `SameExchangeMarkets`,[hft-refdata](../hft-refdata/README.md))。UTA 下现货与合约在同一个统一账户里,本系统不在同一账户里同时交易两者,所以不建模两条产品线之间的资金占用:账户资产查询与账户频道产出的 `BalanceReport`/`MarginReport` 只归属该账户启用的那一个 venue;私有频道只订阅该产品线。一个 Bitget 子账户只归一个 `AccountId`([hft-core 多账户](../../apps/hft-core/README.md#多账户))。

### 端点

| 用途 | 端点 | 说明 |
|---|---|---|
| 公共行情(UTA V3) | `wss://ws.bitget.com/v3/ws/public`(官方;模拟盘 `wss://wspap.bitget.com/v3/ws/public`) | CloudFront;BBO 路由 `books1`(`race=true`);分档快照路由 `public-topn`(`books5`/`books50`);成交、`ticker` 与增量 `books` 路由各 1 条 |
| 私有 + 下单(UTA V3) | `wss://ws.bitget.com/v3/ws/private`(官方;模拟盘 `wss://wspap.bitget.com/v3/ws/private`) | 登录后免签;primary 是私有频道归属连接,standby 只登录不订阅 |
| Classic V2(对照) | `wss://ws.bitget.com/v2/ws/public` / `private` | 现有采集系统在生产使用(公共行情 + 私有 `orders`) |
| REST | `https://api.bitget.com` | Cloudflare(GSLB);账户查询、查单、兜底;成功码 `"00000"`;模拟盘加请求头 `paptrading: 1`(`VenueCfg.env = Testnet`)。REST 端点表只有一项(下标 0),两个产品线相同 |

`routes()`:`books1`(`RouteId(0)`,`RouteKind::Bbo`,`race = true`)、`public-topn`(1,`RouteKind::DepthTopN`,`topn_levels = [5, 50]`)、`public-trade`(2,`RouteKind::Md`)、只在永续上的 `public-ticker`(3,`RouteKind::Md`)与增量深度 `public-books`(4,`RouteKind::Md`,可选扩展);每条 `max_streams = 1000`、`max_in_msgs_per_s = 10`,除 `books1` 外 `race = false`。私有推送在下单 primary 连接上,不单独建 `Role::UserStream` 连接(`endpoint(Role::UserStream, …)` → `ProtoErr::Unsupported`)。

### 行情频道 → 事件

| 频道(UTA) | 产品线 | 节拍 | 规则 | 事件 | 时间戳 |
|---|---|---|---|---|---|
| `books1`(独立 BBO 连接,双路;feed-bbo 任务) | 两者 | **1 ms**(2026-04-28 起;V2 为 10 ms) | `Snapshot`,`Monotonic` on `seq` | `Bbo` | `ex_ts_ns ← data.ts`(ms),`ex_send_ns ← 顶层 ts` |
| `books5` / `books50`(分档快照,core) | 两者 | 见"分档快照流" | `Snapshot` on `seq` | `L2Snapshot{origin = TopN}`(前 5/50 档整替换) | 同上 |
| `books`(增量深度,可选扩展) | 两者 | 50 ms,首帧快照(≤ 500 档/边)后增量 | `Chain`(`prev_id = pseq`,`last_id = seq`;重置后 `pseq = 0` 视为新快照) | `L2Snapshot{origin = Exchange}` / 每条消息一条 `L2Batch` | 同上 |
| `publicTrade`(New-Trades) | 两者 | 实时 | `Monotonic` on 成交 id(`peek` 取一帧内最大的 `i`) | `Trade`(`i` → `trade_id`、`p`、`v`、`S` → `taker_side`;V2 名 `tradeId`/`price`/`size`/`side` 同样接受) | `T`(成交时间) |
| `ticker` | 永续 | 300–400 ms | `ByTime` | `MarkPrice ← markPrice`、`IndexPrice ← indexPrice`、`Funding{rate ← fundingRate, next ← nextFundingTime}`(官方 UTA 字段;V2 同名,现有采集系统已验证) | `data.ts`;UTA `ticker` 的 `data` 没有 `ts` 时取顶层 `ts` |

- 订阅后的第一条 `trade` 推送是 `action:"snapshot"` 的历史批,丢弃,不产生 `Trade`;之后每条 `update` 的 `data[]` 每笔一条 `Trade`(`trade_id` ← 成交 id 按十进制解析为 u64,非数字 → `DecodeErr::Number`;`taker_side` ← `S`)。BGT-305 的真实 demo 采样确认 snapshot 后为 update;正式环境同字段仍需核验。
- `books1` 某一侧为空数组 → 产出 `Bbo`,该侧 `px = 0`、`qty = 0`(下游按"缺一侧"处理,与其他所一致);`books5`/`books15` 中数量为 0 的档不输出。
- `ticker` 的 `Funding.interval_s` = 相邻两次不同 `nextFundingTime` 之差(秒);本 `FeedCtx` 首次见到该 inst、`nextFundingTime` 尚未变过、或差值 ≤ 0 时为 0(未知)。
- `ticker` 的 mark/index 是估算参考价,可超过交易价格小数位,按资产价格指数作一次 HalfEven 舍入;盘口与实际成交价格仍要求精确表示。真实 V3 指数价的 15 位小数样本归 BGT-307。
- 非 `Chain` 类的 `peek`:`ids.last_id` = 去重键(`books1`、`books5`/`books15` 取 `seq`,成交取成交 id,`ticker` 取 `ts` 换算的 ns),`ids.prev_id = 0`。
- 增量深度(可选扩展,路由 `public-books`):`action = snapshot` 或 `pseq = 0` 的推送解码为 `L2Snapshot{origin = Exchange, snap_id = seq}`(零量档不输出),其余每条一条 `L2Batch{prev_id = pseq, last_id = seq}`(量 0 = 删档);`peek` 给 `ids = {pseq, seq}`;`align_first`:快照 `seq` 落在首条增量的 `[pseq, seq]` 内 → `Apply`,`seq ≤ snap_id` → `Drop`,否则 `Gap`;每边至多 1000 档(`maxDepth` 上限),超出 → `DecodeErr::Shape`;重同步 `ResyncMethod::Resubscribe`(重新订阅即得新快照)。
- 公共强平流:V2 没有;UTA 有 Liquidation 频道(格式待核实,开发备忘第 1 条),不订阅(与本账户无关)。
- 旧 V2 流实测 p50 6.5 ms、p90 48 ms、p99 140 ms,接收 → 建簿 p99 0.2 ms,重尾在交易所侧;**`books1` 1 ms 能否消掉重尾是接入后的第一个实验**([04 §5](../../design/04-行情接入层.md))。

### 下单:UTA V3 WS

| 操作 | 帧(官方 UTA Place/Cancel/Modify-Order-Channel) | 对应 REST |
|---|---|---|
| 登录 | `{"op":"login","args":[{apiKey, passphrase, timestamp, sign}]}`,`sign = Base64(HMAC-SHA256(secret, ts + "GET" + "/user/verify"))`,`ts` 发秒;BGT-310 真实 demo 秒/毫秒均登录成功,不据此要求毫秒;回执 `{"event":"login","code":"0"}`,失败 `{"event":"error","code":…}` | — |
| 下单 | `{"op":"trade","id":<请求 id>,"category":"usdt-futures"\|"spot","topic":"place-order","args":[{symbol, side, orderType:"limit", price, qty, timeInForce, clientOid[, reduceOnly:"YES"]}]}` | `/api/v3/trade/place-order` |
| 改单 | `topic = "modify-order"`,`args = [{symbol, clientOid, price, qty, autoCancel:"no"}]` | `/api/v3/trade/modify-order` |
| 撤单 | `topic = "cancel-order"`,`args = [{clientOid}]` | `/api/v3/trade/cancel-order` |
| 全撤(`CancelAll`) | WS 无按品种全撤;运行时展开为逐单 `cancel-order` | `/api/v3/trade/cancel-symbol-order`(按 `category` + `symbol`,本 crate 未用) |
| 查单 | REST(`order_channel(Query) = Rest`,`encode_rest`):`GET /api/v3/trade/order-info?clientOid=<cid>` | — |

- 响应:`{"event":"trade","id":…,"topic":…,"args":[{orderId, clientOid[, symbol, cTime]}],"code":"0","msg":"success","ts":…}`;失败为 `{"event":"error","id":…,"code":"<码>","msg":…}`(`event = trade` 而 `code ≠ 0` 同样按失败处理)。**官方:下单、改单、撤单的成功响应只表示交易所收到请求**,订单状态以私有 `order` 频道为准;下单在入撮合前被拒也只体现为响应码。因此:下单成功响应 → `Ack`(`exch_order_id ← orderId`,`exch_id` 登记);**撤单与改单的成功响应不产生事件**(只结束在途请求),`CancelAck` 由订单频道的 `cancelled` 推送产出;改单是异步的、失败不另行通知,所以 `VenueCaps.flags` 不含 `AMEND`(帧仍可编码,运行时不会发)。
- 价/量按 inst 的 tick/lot 格式化为最短十进制(不补零、不经 f64);`Tif::Gtc/Ioc/Fok/PostOnly` → `gtc`/`ioc`/`fok`/`post_only`;永续 `oflags` 带 `REDUCE_ONLY` 时写 `"reduceOnly":"YES"`,否则不写(缺省为否);现货带 `REDUCE_ONLY` → `EncodeErr::Unsupported`。UTA 下单没有 `pxAmendType` 参数:超出价格带的单以 `25205`/`25206` 拒绝(登记为确定拒绝),不夹到边界。`ORDER_FRAME_CAP = 2048`;`encode_batch` 保持默认 `Unsupported`(官方批量上限 20,[hft-oms](../hft-oms/README.md) 每个意图一个槽一个帧)。
- `CancelAll`:`encode` 对 `CancelAll` 返回 `EncodeErr::Unsupported(CancelAll)`;运行时把槽镜像中该账户的活动订单展开为逐条 `Cancel`([hft-venue 出环](../hft-venue/README.md#出环outmsg--gwcmd) 的第 ③ 级),回报按 `Cancel` 解码;`CANCEL_ALL_SWEEP = false`。
- `Query`:签名同实现说明第 8 点(`ACCESS-TIMESTAMP` 为 ms,另带 `locale: en-US`,模拟盘 `paptrading: 1`);响应经 `decode_rest`(槽取自 `RestOrigin`,`clientOid` 必须与请求的 venue/account/槽/代一致后取其 epoch;非空品种必须匹配原请求):`orderStatus` → `OrderEvent.detail`(`live`/`new` → New、`partially_filled`、`filled`、`cancelled`,契约 28)、`cumExecQty`/`avgPrice` → `cum_filled`/`avg_px`(契约 9);码 `25204`(订单不存在)→ `detail = NotFound(7)`,不定终态;其余失败 → 结果未知(HTTP 429 置 `RateLimited`)。
- 每频道限速、STP、过期保护字段:官方未写,按 10 单/s 与"无过期字段"处理(`stale_guard() = None`,见实现说明第 7 点);响应无网关时间戳,`ReportOutcome.gw_dwell_us = 0`。

### 限速默认值

| 范围 | 值 |
|---|---|
| 下单 / UID | 默认 10/s;VIP1 60/s;VIP2 80/s;VIP3–7 100/s |
| 机构(MM/PRO) | 单账户 Tier1 600/s;母子合计 Tier1 120 000/s;WS 与 REST 共享 |
| WS 建连 | 300 次 / 5 min / IP;≤ 100 连接 / IP |
| 订阅 | 240 次 / h / 连接;≤ 1000 频道 / 连接(建议 < 50) |
| 客户端入向 | 10 条 / s / 连接(超出被断) |
| 账户/持仓查询 REST | 待核实(开发备忘第 5 条) |

来源 [research/02 §5](../../design/research/02-exchange-connectivity.md)。`rate_rules()` 返回三条:`(Uid, Orders, 1 000 ms, 10)`、`(Ip, NewConns, 300 000 ms, 300)`、`(Conn, WsInMsgs, 1 000 ms, 10)`;VIP 与机构档由 refdata 覆盖。订阅次数(240 次/h/连接)与每 IP 连接数(≤ 100)没有对应的 `LimitBucket`,不进 `rate_rules()`,分别由订阅帧合并(实现说明第 4 点)与部署的连接数保证。

### 分档快照流

core 的深度来源([hft-venue-proto 契约 26](../hft-venue-proto/README.md#实现方必须遵守的契约));`RouteSpec.topn_levels = [5, 15]`。

| 产品线 | 频道 | 档位 | 节拍 | 序号 / 时间 | 路由与端点 | 来源 |
|---|---|---|---|---|---|---|
| 两者 | `books5`、`books50` | 5 / 50 | 10 ms / 20 ms(官方) | `seq` 单调(`Snapshot`);`data.ts`(ms)、顶层 `ts` | 路由 `public-topn`(`RouteKind::DepthTopN`),UTA V3 `/v3/ws/public` | 官方 UTA Order-Book-Channel(UTA 没有 `books15`) |

每条推送是完整的前 N 档;盘口不变时不推(官方);订阅后首帧是否立即推当前快照待核实(开发备忘第 17 条)。`seq`/`ts` 的数字与字符串两种形式都接受(实现说明第 3 点)。

### L2 序号归一化(`L2Batch.prev_id/last_id`)

增量深度(可选扩展)用;规格保留,`feed.depth = incremental` 时才生效。

| 流 | `prev_id` | `last_id` | 快照 `snap_id` |
|---|---|---|---|
| `books` 增量 | `pseq` | `seq` | 快照帧(或 `pseq = 0` 的重置帧)的 `seq` |
| 快照后第一条 | `snap_id`(`RxMeta.prev_override`) | `seq` | — |

契约见 [hft-venue-proto](../hft-venue-proto/README.md#实现方必须遵守的契约)。

### 累计量与费用来源

`PartialFill`/`Fill.detail` 保留 wire `side` 的 Buy=1/Sell=2,供 OMS 对迟到或无归属成交确定净量方向;其他回报继续使用各自状态/拒绝分类。带非零累计成交量却缺失方向的行拒绝,不猜测买卖。

UTA 订单回报的累计量比已保存量小时不产出事件,也不回退累计金额、费用或终态基线;累计量未增长时保留已有金额基线。`avgPrice` 按资产价格指数作 HalfEven 舍入,允许成交均价含更多小数位。手续费按扣费资产数量指数作一次 HalfEven 舍入后取绝对值,允许真实订单频道比资产指数多出小数位。负累计量、金额、均价以及超过累计量的单笔数量在更新上下文之前拒绝。

| 流 | `cum_filled` | `avg_px` | 费用 |
|---|---|---|---|
| UTA 私有 `order` 频道 | `cumExecQty`(订单累计;V2 `accBaseVolume`) | `avgPrice`(V2 `priceAvg`) | `feeDetail[0].fee`(订单累计,取绝对值)+ `feeCoin`;本笔费用 = 与 `GwCtx` 中上次累计的差分(V2 本笔 `fillFee`/`fillFeeCoin` 存在时直接用) |
| UTA `fill` / `fast-fill` 频道 | 不订阅 | — | — |
| REST 查单 | `cumExecQty` | `avgPrice` | — |

UTA 订单推送不带本笔成交:`GwCtx` 按 `orderId` 记上次的累计量、累计成交额(`cumExecValue`)与累计费用;累计量前进时产出成交,本笔量 = 累计量差,本笔价 = 成交额差 / 量差(无成交额时取均价),`trade_id = 0`(去重键为契约 22 的 `(inst, 订单 id, 累计成交量)`:累计量不前进 = 重复,0 个事件)。`GwCtx` 定长 4096 项,探测窗口满时复用终态项、否则最旧项;被复用的活动订单从零重算,重复的累计量由 OMS 按契约 9 忽略。订单推送缺 `cumExecQty`(非挂单/撤单状态)→ `DecodeErr::Shape`。`funding_settle = RateOnly`,不产出资金费推送,没有资金费去重键。

### 结果未知码表

`src/codes.rs` 两张表([契约 8](../hft-venue-proto/README.md#实现方必须遵守的契约)),码来自官方 UTA REST 错误码页。码的形态:WS 为字符串(`"25204"`),订阅拒绝在 V2 为数字(`30001`);两种形态都按契约 8 编码为 `reject_code`/`raw_code`(非数字码取 FNV-1a 哈希段)。

| 表 | 码 | 来源 |
|---|---|---|
| 确定拒绝 | `25004`(过于频繁,同时 `ctl = RateLimited`)、`25100`/`25101`/`25102`/`25104`/`25105`(品种不存在、未开放、维护、下架、不可开仓)、`25200`(校验错误)、`25202`/`25203`(余额/保证金不足)、`25205`/`25206`(超出价格带)、`25207`/`25208`(数量上下限)、`25211`(挂单数上限)、`25230`/`25231`/`25232`/`25574`(超最大可开、平仓超持仓、reduceOnly 冲突/保护)、`25244`(价格非 tick 倍数)、`25610`/`25611`(精度、最小下单量)、`40017`(参数校验)、`40762`(下单页示例:超最大可开) | 官方 UTA 错误码页 |
| 结果未知 | `25000`(系统错误)、`25001`(超时)、`25003`(并发)、`40010`(请求超时)、`40725`、`45001`、HTTP 403/408/418/429/5xx,以及**任何未登记码**(含 `25212` 重复 `clientOid`:前一笔可能仍挂着) | 官方 + 契约默认 |

传输失败状态优先于业务码,即使响应含 `25204` 也保持 Unknown,不生成 NotFound。

`GONE`(契约 28):撤单、改单失败码 `25204`(订单不存在)→ `CancelReject`/`AmendReject{detail = 1}`;查单 `25204` → `NotFound`。`40008`(时间戳过期)→ `ctl = ClockSkew`;HTTP 429 → `RateLimited`。

### VenueCaps 默认值

| 字段 | `BitgetUsdtFutures` | `BitgetSpot` | 来源 |
|---|---|---|---|
| `fee_asset_rule` | `Quote`(USDT) | `ReceivedAssetUnlessDiscount{BGB}`(BGB 抵扣开启且余额足 → BGB;否则成交所得资产) | 待核实(开发备忘第 8 条) |
| `reduce_only` | 是(`reduceOnly: "YES"`) | 否 | 官方 |
| `required_position_mode` | 单向持仓 | 不适用 | 待核实(字段,开发备忘第 5 条) |
| `required_margin_mode` | 全仓;多资产关闭(UTA 基础模式) | 不适用 | 模式:官方;字段:待核实(开发备忘第 5 条) |
| `required_spot_borrow` | `None`(永续) | `Some(false)`:UTA 现货杠杆 / 借币关闭 | 待核实(端点与字段,开发备忘第 15 条) |
| `funding_settle` | `RateOnly`(未找到资金费实收推送;找到后改 `PerSymbolPush`/`BalanceOnlyPush`) | 不适用 | 待核实(开发备忘第 7 条) |
| `margin_data` | `Poll{interval_ms: 5000}`(若核实账户频道推送保证金率,改 `Push`) | 不适用 | 待核实(开发备忘第 5 条) |
| `own_liquidation_marker` | 订单推送的 `tradeSide`(另认 `delegateType = liquidation / delivery_close_*`)。双向:`burst_close_*`、`reduce_close_*`(强平/部分强平)、`dte_sys_adl_close_*`(ADL)、`delivery_close_*`(交割);单向:`burst_buy_single`/`burst_sell_single`、`reduce_buy_single`/`reduce_sell_single`、`dte_sys_adl_buy_in_single_side_mode`/`dte_sys_adl_sell_in_single_side_mode`、`delivery_buy_single`/`delivery_sell_single` | 无(基础模式现货无强平) | 官方 UTA 枚举页;模拟盘样本待采(开发备忘第 6、14 条) |
| `ignorable_user_events` | 空(核实私有频道全集后补) | 空 | 待核实(开发备忘第 1 条) |
| `amend` | **否**(`modify-order` 的成功响应只是回执,结果异步且失败不通知) | 否 | 官方 Modify-Order-Channel |
| `amend_qty_down_only` | 否 | 否 | 待核实(开发备忘第 2 条:`modify-order` 能否改价与增量) |
| `post_only` | 是(`post_only`) | 是 | 官方 |
| `ioc` | 是 | 是 | 官方 |
| `fok` | 是 | 是 | 官方 |
| `batch_cancel` | 否(逐个撤;官方有 `batch-cancel`,未用) | 否 | 官方 |
| `dms` | `None`(`DeadManSpec::None`)→ kill-switch 兜底 | `None` | 官方有 REST `countdown-cancel-all`(5–60 s,只对 UTA,需向商务申请开通),未接入(已知限制) |
| `testnet` | `Demo`(UTA 文档目录有"Demo Trading"页;端点、请求头与模拟盘符号见"测试网/模拟盘",待核实) | 同左 | 官方目录 / 待核实(开发备忘第 9 条) |

`stale_guard()` = `None`(核实前);`dead_man()` = `None`;`dead_man_refresh` 被调用 → `EncodeErr::Unsupported(GwOp::DeadManRefresh)`。

`default_caps()` 中上表未写出码值的字段:`own_liquidation_marker`:永续 `LiqMarker::OrderTypeField`,`liq_marker_arg` = 上表 16 个取值以 `|` 连接,现货 `NotIdentifiable`、`""`;`required_multi_asset`:永续 `Some(false)`,现货 `None`;`flags` = `POST_ONLY | IOC | FOK`(= 14,两者);`dms` = `DeadManCap::None`,四个 `dms_*_ms` 为 0;`testnet` = `TestnetKind::Demo`;`fee_asset_rule` 里的 `BGB` 以资产名给出,由 hft-cfg 按 refdata 资产表解析为 `AssetId`。

### 账户配置核验

UTA 设置接受 `accountMode = unified | hybrid` 且 `accountLevel = basic`;`classic`、`upgrading`、`switching` 与未知值拒绝。字段含义归 [官方账户设置](https://www.bitget.com/docs/catalog/account/account-settings)。demo 实测 basic/单向账户返回 `hybrid` 与 `assetMode=multi_assets`;保证金资格按账户级别、零债务和逐品种 crossed/杠杆核验,不把该 assetMode 字面值当成 advanced。demo `account/info` 的 `permType=unknown`、空 permissions 仍拒绝;Unverifiable 只放宽缺失身份,不豁免权限。

`acct_queries(Config)`([契约 16](../hft-venue-proto/README.md#实现方必须遵守的契约)),端点与字段全部待核实(开发备忘第 5、15 条),核实前按下表的"要读到什么"实现,读不到即 `UNREADABLE`(fail closed,启动闸门不放行):

| 步 | 请求(候选,待核实:开发备忘第 5 条) | 要读到什么 → 判定 |
|---|---|---|
| 0 | UTA 账户设置(`GET /api/v3/account/settings` 一类) | 账户模式:非 UTA(Classic)→ `ACCT_TYPE`;逐仓模式 → `MARGIN_MODE`;永续:高级模式而要求多资产关闭 → `MULTI_ASSET`;现货:高级(多资产)模式 → `ACCT_TYPE`(现货余额参与保证金,同 OKX 现货 `acctLv` 3/4);持仓模式 ≠ 单向 → `POS_MODE`(只对永续) |
| 1 | 每个 refdata 可交易永续的保证金模式(按品种配置或持仓列表) | 任一为逐仓 → `MARGIN_MODE`(全仓与逐仓并存时 `margin_mode` 填 `MarginMode::Mixed`,全为逐仓填 `Isolated`) |
| 2 | 账户信息(含 UID 与 API key 权限,V2 有 `userId`/`authorities` 一类字段) | UID ≠ `AccountMeta.venue_uid` → `UID`;带提现权限或缺交易权限 → `KEY_PERM`;登录 WS 与 REST 签名是同一把 key,`session_identity_request` 返回 `Ok(false)`,本步同时覆盖两种用途(`uid_keys = SESSION \| REST`);`uid_policy = "unverifiable"` 的账户不核对 UID:UID 缺失或为空时 `UID`、`UNREADABLE` 都不因它置位,`uid_keys = 0`,`venue_uid` 填返回值(缺失为全 0) |
| 3 | BGB 抵扣开关(现货) | 与 `fee_asset_rule` 矛盾(规则为 `ReceivedAssetUnlessDiscount{BGB}` 而抵扣关闭,或规则为其他变体而抵扣开启)→ `FEE_RULE` |
| 4(`BitgetSpot`) | UTA 现货杠杆 / 借币开关(端点与字段待核实,开发备忘第 15 条) | **现货必须关闭借币**:开启 → `AUTO_BORROW`;读不到 → `UNREADABLE`(核实前现货账户过不了 `ACCOUNT_CFG`) |
| 5(`BitgetUsdtFutures`) | 每个可交易永续的杠杆(按品种配置或持仓列表,端点待核实,开发备忘第 15 条) | 每 inst 一条 `LeverageItem`;读不到写 0 |

序列:永续 = 步 0、1、2、5,现货 = 步 0、2、3、4;每步 `every = 1`、`weight = 1`(Bitget 按请求次数限速,不按权重;各端点限速待核实,开发备忘第 5 条)。某步 HTTP 非 2xx、响应体无法解析或该步要读的字段缺失 → `UNREADABLE`,该步的其余判定不做。

运行中:若核实存在账户配置变化推送,给 `hint = ConfigRecheck`;否则依赖 `gw.acct_cfg_recheck_s` 周期重查。

### 保证金数据换算

当前持仓响应的 `data.list = null` 与空数组都表示零持仓;缺失 `list` 或错误类型拒绝整页,保留上次真值。此语义覆盖 warm 启动与只读对账复用的账户解析器。

`margin_data = Poll{interval_ms: 5000}`([契约 17](../hft-venue-proto/README.md#实现方必须遵守的契约)),只对 `BitgetUsdtFutures`:

| 步 | 请求(候选,待核实:开发备忘第 5 条) | 产出 |
|---|---|---|
| 0 | UTA 账户资产(账户权益、维持保证金、可用、保证金率) | 账户级字段 |
| 1 | UTA 当前持仓(强平价、ADL 档若有) | 每 inst 条目 + `PositionReport` |

两步都是 `every = 1`、`weight = 1`;现货的 Margin 序列为空。`PositionReport{qty ← 持仓量(多为正、空为负), entry_px ← 开仓均价, upnl ← 未实现盈亏, liq_px ← 强平价}`,只保留 refdata 中的品种(字段名待核实,开发备忘第 5 条)。任一步失败(HTTP 非 2xx、响应无法解析、必需的账户级字段缺失)→ `acct_finish` 返回 `DecodeErr::Shape`,不产出报告(数据过期由 hft-risk 的 `margin_stale_ms` 处理)。

| `MarginReport` 字段 | 换算 |
|---|---|
| `margin_balance` | 账户权益(USDT;基础模式下稳定币权益) |
| `maint_margin` | 维持保证金 |
| `avail_margin` | 可用保证金 |
| `margin_ratio` | 一律按统一口径 `maint_margin / margin_balance` 计算(i128 整数除法,结果超出 `Rate` 范围饱和到 `Rate::MAX`)([hft-event `MarginReportHdr`](../hft-event/README.md));交易所给出的保证金率(官方定义 =(维持保证金 + 部分强平手续费)÷ 账户权益)只作校验对照,不直接采用 |
| 条目 `liq_px` | 持仓强平价(空/0 → 0) |
| 条目 `adl_quantile` | 若 API 提供 ADL 档则归一化到 0..=100;否则 255(未知) |

### 外部成交识别

`own_liquidation_marker` 的实现([契约 18](../hft-venue-proto/README.md#实现方必须遵守的契约)):

| 判定(按顺序) | `cause` | 来源 |
|---|---|---|
| `tradeSide` ∈ {`burst_close_*`, `reduce_close_*`, `burst_buy_single`, `burst_sell_single`, `reduce_buy_single`, `reduce_sell_single`},或 `delegateType = liquidation` | `Liquidation` | 官方枚举页 |
| `tradeSide` ∈ {`dte_sys_adl_close_*`, `dte_sys_adl_buy_in_single_side_mode`, `dte_sys_adl_sell_in_single_side_mode`} | `Adl` | 官方枚举页 |
| `tradeSide` ∈ {`delivery_close_*`, `delivery_buy_single`, `delivery_sell_single`},或 `delegateType = delivery_close_*` | `Delivery` | 官方枚举页 |
| 无上述标记且 `clientOid` 非空、非我们的格式 | `Unknown`(偏安全:仍作外部成交入账并 P0) | — |
| `parse_cid(clientOid)` 成功 | 本系统订单 → `OrderEvent` | — |
| `clientOid` 为空 | 成交交运行时按 `IdTable` 解析(`exch_id(orderId, None)`;`Resolved` 时直接用其槽,否则 `slot = UNRESOLVED`),非成交事件丢弃 | 契约 18 |

"我方订单的成交推送必带 `clientOid`"待核实(开发备忘第 3 条);核实前按"不保证"处理:`clientOid` 缺失的成交交给运行时按 `IdTable` 解析,超时转 `ExternalFill{Unknown}`([hft-venue §4](../hft-venue/README.md#4-gwruntimepoll_once-的循环结构))。

### 用户流事件表

| 消息(UTA 私有频道,两个产品线相同;`position` 只永续) | 产出 | 提示 |
|---|---|---|
| `order`(`{"instType":"UTA","topic":"order"}`;只保留 `category` = 本产品线) | 先过"外部成交识别";`live`/`new` → `Ack`;累计量前进 → `PartialFill`/`Fill`;`cancelled` → `CancelAck`,`timeInForce ∈ {ioc, fok}` 或 `cancelReason ∈ {ioc_not_full_cancel, fok_not_full_cancel, post_only_fill_cancel}` 时 `Expired`(都带当时的累计量与本笔增量) | — |
| `fill` / `fast-fill` | 不订阅;收到时 `skip = Listed` | — |
| `position`(只永续订阅;现货不产出) | `PositionReport{qty ← size(posSide = short 取负), entry_px ← avgPrice, upnl ← unrealisedPnl, liq_px ← liqPrice(空 → 0)}`(只校验;refdata 外品种跳过) | — |
| `account` | `data[0].coin[]` 每币种一条 `BalanceReport`:永续 `free ← balance, locked ← 0`;现货 `free ← balance`(有符号),`locked ← locked`(必须非负)(只校验;refdata 外币种跳过);`margin_data = Poll` 期间不产出 `MarginReport`;若核实带保证金率则 `MarginReport`(并把 `margin_data` 改为 `Push`;推送在 gw 线程上解码,预算上限见 [hft-venue](../hft-venue/README.md#延迟与资源契约) `venue_gw_margin_push`) | — |
| `{"event":"login"|"subscribe"|"error"}` | 无(控制回执;订阅被拒 `code` 为数字,如 `30001`) | — |
| `pong` | 无(心跳回应) | — |
| 其他 | 无(`skip = Unknown`;核实后把确认无用的名字加入 `ignorable_user_events`) | — |

### 资金费实额

`BitgetUsdtFutures` 实现 `FundingHistoryProtocol`,HMAC 签名查询 `GET /api/v3/account/financial-records?category=USDT-FUTURES&coin=USDT`,明确起止毫秒、`limit=100` 和返回的 cursor。这不是资金账户的 `funding-financial-records`。只接受 `CONTRACT_MAIN_SETTLE_FEE_USER_IN/OUT` 的交易账单,`id` 去重,`symbol` 定位合约,`ts` 为毫秒,`amount` 为现金金额,由 IN/OUT 规范正负。交易费、转账与 RWA 分红类型不当作普通资金费。分页错误、未知合约、错误币种或非成功响应拒绝整页。来源为 [官方财务记录](https://www.bitget.com/docs/catalog/account/assets-balance) 与 [官方枚举](https://www.bitget.com/docs/uta/enum)。

`amount` 在 Notional 的 -6 指数边界作一次 HalfEven 舍入,再按 IN/OUT 规范正负;原始账单金额保留在金样中。BGT-336 的真实八位小数资金费不能要求六位精确表示,每笔表示误差至多 0.5 微 USDT,不宣称原始现金逐位无损。

`ticker` 的费率/下次结算时间经 Funding 进入策略;account 推送的余额不能推导逐合约收付。`RateOnly` 保留行情估算,认证账单经 [FundingPoll](../hft-venue/README.md#资金费账单轮询) 和 [ledger 实额完成](../hft-ledger/README.md#资金费) 修正金额,不等待 M9 的资金费自动对账。

### 测试网/模拟盘

真实 demo 采样使用 `wspap.bitget.com/v3/ws/{public,private}`、REST `api.bitget.com/api/v3` 与 `paptrading: 1`,REST 签名时间戳为毫秒;ETHUSDT/BTCUSDT 不加模拟币前缀。`account/info` 返回 unknown/空权限仍拒绝配置闸门,不能由成功下单推断无提现权限;`deduct-info` 在本轮 demo 返回 404,覆盖范围仍待核实。模拟盘读不到 UID 时该账户按 `uid_policy = "unverifiable"` 配置(规则归 [hft-refdata](../hft-refdata/README.md#数据结构)),该策略不豁免权限核验。模拟盘 refdata 由 hft-cfg 按模拟盘的品种接口单独生成。上线门槛第 ⑥ 项([testing §8](../../docs/testing.md#8-上线门槛))要求 Bitget 模拟盘联调,行情与下单都用模拟盘(`feed_env = "testnet"` + 模拟盘 refdata);模拟盘不覆盖的接口逐项记录,处理方式归 testing §8,不以实盘小单替代。

### 心跳与 BBO 探测

按连接种类分别写([契约 24](../hft-venue-proto/README.md#实现方必须遵守的契约)):

| 连接 | 服务端行为 | 我们的动作 | 来源 |
|---|---|---|---|
| 公共 / 私有(UTA V3、Classic V2) | 120 s 没收到客户端 `ping` 即断,即使数据在流(实测约 150 s) | 每 25 s **无条件**发文本 `ping`,10 s 内须回 `pong`(实现说明第 1 点) | 现有采集系统 2026-09-04 事故 + research/02 |
| BBO 连接(`books1`,双路) | 同上 | `bbo_probe()` = 文本 `ping`(服务端是否回 WS 协议 Pong 待核实),每 `feed.bbo_ping_ms` 一次,同时满足上一行的 25 s 要求;`pong` 计入存活 | 待核实(开发备忘第 16 条) |

- 入向上限 10 条/s/连接(超出被断):BBO 连接的探测占 1 条/s,订阅帧按 4 条/s 发送。
- BBO 补帧:`GET /api/v3/market/orderbook?category=…&symbol=…&limit=1`,公共接口不签名,`BBO_SNAPSHOT_BATCH = 1`。按[官方市场数据](https://www.bitget.com/docs/catalog/market/market-data)读取 `data.a/b/ts`;价量的 JSON 数字保留原始文本做定点解析。REST 不提供序号,`update_id = 0`;缺任一侧或无效数值拒绝整条响应。

### 只读查询

真实 demo 查单与未成交订单页的覆盖范围归 BGT-333/334;外来 client ID 不取得自有槽身份,分页 cursor 按交易所返回值继续查询。

Warm 入口支持 OpenOrders、Fills、Balance 与永续 Positions，整页校验后才调用 RoSink；错误页没有部分结果，外来 client id 保持 UNRESOLVED/None，不借用本账户槽。FundingFlow 暂由 `FundingHistoryProtocol` 的固定窗口接口承载，`ro_request(FundingFlow)` 返回 Unsupported。合成响应仅证明本地映射，真实接口权限与分页仍须测试网验证。

UTA V3 分页取响应 cursor，空/null 游标表示末页，显式 list=null 表示空列表；缺 list、空列表带续页游标、重复或非数值游标返回错误，不伪造末页。查询按 category 请求，inst 过滤在完整响应中完成，避免缩小页面后丢失下一页。Fills 的 feeDetail 必须可归到同一已登记资产；混合手续费币种拒绝该页。字段来源为 [官方订单查询文档](https://www.bitget.com/docs/catalog/trading/order-management)。


`ro_request_signed`/`ro_decode` 的实现([契约 31](../hft-venue-proto/README.md#实现方必须遵守的契约));reconciler 以只读 key 构建 `Templates` 调用,gw 不用(`CANCEL_ALL_SWEEP = false`)。UTA 端点、分页与字段**全部待核实**(开发备忘第 18 条),核实前按下表"要读到什么"实现,读不到即 `DecodeErr::Shape`。

| `RoKind` | 请求(候选,两个产品线) | 要读到什么 |
|---|---|---|
| `OpenOrders` | UTA 未成交订单查询 | 每单 `clientOid`、订单 id、品种、方向、价、量、累计成交量、状态 → `open_order` |
| `Fills` | UTA 成交查询(按时间与游标翻页) | 成交 id、订单 id、`clientOid`(若带)、量、价、费用与币种、时间 → `fill` |
| `FundingFlow` | UTA 账单(资金费类型);现货 → `Unsupported` | 品种、金额、结算时刻 → `funding` |
| `Balance` | UTA 资产 | 每币可用与冻结 → `balance` |
| `Positions` | UTA 当前仓位;现货 → `Unsupported` | 每品种持仓、开仓均价、未实现盈亏、强平价 → `position` |

### `mock` 特性

`features = ["mock"]` 导出 `mock::{encode_market, decode_order_frame, encode_order_event, encode_rest_response}` 与输入类型,供 optional 的 [mock-exchange](../../apps/mock-exchange/README.md) 离线使用。支持 BBO/深度/成交/标记资金费、订单请求/应答/推送、REST BBO 与查单;生产默认关闭。账户 REST 模拟响应由测试夹具提供。

## 实现说明

<details><summary>实现要点</summary>

1. **心跳**:`HeartbeatSpec{ping: AppText("ping"), interval_ms: 25000, unconditional: true, pong_timeout_ms: 10000, idle_timeout_ms: 30000}`——**无论是否有数据都按周期发**(≤ 30 s)。现有采集系统在 2026-09-04 只在空闲时 ping,所有连接在建立约 150 s 后被服务端 reset;这条规则是协议常量,不是可调参数。
2. **分派**:推送 `{"arg":{"instType":…,"channel":"books1","instId":…},"action":…,"data":[…],"ts":…}`(UTA 的 `arg` 字段名待核实,开发备忘第 1 条:`topic`/`symbol` 或 `channel`/`instId`;两种写法都接受)。V2 推送以 `{"action":"snapshot",` 开头时频道名从第 65 字节起,不在 `classify` 的 64 B 内,所以**行情按 `route` 分类**:以 `{"action":` 或 `{"arg":` 开头的数据推送 → 该路由的类(`books1` → `Books1`,`public-topn` → `BooksTopN`,`public-trade` → `Trade`,`public-ticker` → `Ticker`);`{"event":"subscribe"`/`"unsubscribe"`/`"error"` → `SubAck`(`{"event":"error","code":30001` 为订阅被拒,`code` 是数字);文本 `pong` → `Pong`;其余 → `Unknown`。私有连接:`{"id":"` 后第一个字符是 op 码 → `WsResp`;`{"event":` → `Control`;`pong` → `Pong`;推送的频道名完整落在前 64 B 内则按"用户流事件表"分类,否则 `Push`(由 `decode_report` 解析 `arg` 后分类,结果相同);`classify_report` 不认得的频道名为 `Unknown`,`decode_report` 再查生效 `ignorable_user_events`(命中 → `skip = Listed`,否则 `skip = Unknown`,契约 20)。
3. **数字宽容**:`seq` 为数字,`ts` 为字符串;解析器同时接受数字与字符串形式(现有采集系统的做法)。价/量按 hft-types `parse_exact` 解析(科学计数法照收);`FxErr::OVERFLOW` → `DecodeErr::Overflow`,其余解析错误(含超精度非零位)→ `DecodeErr::Number`。
4. **订阅**:每帧 ≤ 50 个 `args`,不带 `id`;`StreamKind` → 频道:`Bbo` → `books1`,`DepthTopN` → `books5`/`books15`(`levels` 为 5 或 15,其他 → `ProtoErr::Unsupported`),`Trade` → `publicTrade`,`Mark`/`Index`/`Funding` 同一 inst 合并为一个 `ticker`(只永续),`Liquidation` 与现货的 `Mark`/`Index`/`Funding` → `ProtoErr::Unsupported`;按 10 条/s 入向上限的一半限速。V2 私有 `orders` 用 `instId: "default"` 订阅全账户(生产验证);UTA 写法待核实(开发备忘第 1 条)。
5. **回报**:见"下单:UTA V3 WS"(响应)、"累计量与费用来源"与"用户流事件表"(订单频道)。一条推送的全部订单先全部校验,任一失败 → `DecodeErr`,不写事件、不改 `GwCtx`。字段(UTA 名,V2 名作别名):`orderId` → `exch_order_id`、`updatedTime` → `ex_ts_ns`、`feeDetail[].feeCoin` → `fee_asset`(按资产名在两个 Bitget 产品线的 inst 资产与抵扣币中查找)。扣费币种与 `fee_asset_rule` 不符 → `fee_rule_deviation`。
6. **死手**:`DeadManSpec::None`。kill-switch 进程持有撤单权限的 REST 凭证,在 hft-core 心跳消失时撤该账户全部挂单([09 §4.2](../../design/09-可观测性、风控与控制面.md))。
7. **无过期保护**:核实前 `stale_guard() = StaleGuard::None`。查询得到"订单不存在"不能按超时定终态(迟到的订单可能仍会被撮合),OMS 只能等对账收敛([契约 10](../hft-venue-proto/README.md#实现方必须遵守的契约))。
8. **账户查询**:REST 签名 `Base64(HMAC-SHA256(secret, ts + METHOD + path + query + body))`,头 `ACCESS-KEY/ACCESS-SIGN/ACCESS-TIMESTAMP/ACCESS-PASSPHRASE`(research/02);只保留 refdata 中的品种写入 `AcctScratch`。
9. **源文件**:`lib.rs`(两个类型、`FeedCtx`、`GwCtx`)、`line.rs`、`json.rs`(数字/字符串两用的整数与码、原始档位文本的解析、定长数组)、`caps.rs`、`cid.rs`、`sign.rs`、`session.rs`(路由、端点、登录、订阅)、`md.rs`(books1、books5/books50、增量 books、publicTrade、ticker)、`orders.rs`(WS 帧、REST 查单)、`reports.rs`(响应、订单/持仓/账户频道、外部成交、查单响应)、`codes.rs`;规划中的 `acct.rs`(Config 与 Margin 序列)未建。

</details>

盘口与序号预读单遍投影 `arg` 与 `data[0]`，价量数组借用原始字节；未知值不建立 token 表。私有频道分派只投影 `arg`。其他形态回退严格解码器，先校验 UTF-8，非法文本返回 Shape。

## 延迟与资源契约

| 入口 | p50 / p99 预算 | 基准名 |
|---|---|---|
| `classify` | ≤ 30 ns / ≤ 60 ns | `bench_classify_bitget` |
| `decode_md` books1 → `Bbo`(永续 / 现货) | ≤ 0.4 µs / ≤ 0.8 µs | `bench_decode_bitget_books1`、`bench_decode_bitget_books1/spot` |
| `decode_md` books15 → `L2Snapshot{TopN}` | ≤ 1 µs / ≤ 2 µs | `bench_decode_bitget_books15` |
| `decode_md` books 增量 1 KiB(增量深度,可选扩展) | ≤ 1.5 µs / ≤ 3 µs | `bench_decode_bitget_books` |
| `encode` place-order | ≤ 0.3 µs / ≤ 0.6 µs | `bench_encode_bitget_place` |
| `decode_report` 订单推送(含外部成交判定与去重) | ≤ 1 µs / ≤ 2 µs | `bench_decode_bitget_order` |
| `acct_decode` 持仓列表 50 KiB | ≤ 60 µs / ≤ 100 µs | `bench_acct_decode_bitget` |

`books1` 1 ms 节拍下每个 inst 最多 1000 条/s,feed 核余量按 [01 §3.2](../../design/01-总体架构.md) 的"每条 2–5 µs"估算仍 ≥ 90%。纯函数,热路径零分配零系统调用。

`benches/md.rs` 使用 BGT-500/501 的合成 UTA 报文,计时范围为生产 `classify` 或 `decode_md` 调用;报文构造、refdata 加载和接收缓冲预分配不计时。Criterion 结果只作相对比较,参考机分位数门槛保持上表。

## 测试与验收

用例与验收见 [TESTS.md](TESTS.md)。

## 依赖

`hft-types`、`hft-refdata`、`hft-event`、`hft-venue-proto`、`sonic-rs 0.5`、`hmac 0.12` + `sha2 0.10`、`base64 0.22`。**禁止**:`hft-net`、`hft-venue`、`tokio`、`serde_json`(dev-only)。

## 延伸阅读

- [research/02 §5 Bitget](../../design/research/02-exchange-connectivity.md);UTA 概览页本地缓存 `architecture-v2/research/_src/bitget_ws_intro.txt`
- [04 §5](../../design/04-行情接入层.md)、[05 §7](../../design/05-CEX执行层.md)
- [hft-venue-proto 契约](../hft-venue-proto/README.md#实现方必须遵守的契约)、[hft-venue](../hft-venue/README.md)、[kill-switch](../../apps/kill-switch/README.md)、[cookbook/adding-a-venue.md](../../docs/cookbook/adding-a-venue.md)
- 范围:[round8-decisions](../../design/_work/round8-decisions.md)
- 现有知识来源:`E:\trading\Trading\crates\bitget`(seq/pseq、无条件 ping、登录签名与测试向量)、`E:\trading\Trading\apps\collector\src\private.rs`(V2 私有登录与 `orders` 订阅)
- 官方 UTA 文档原文的镜像(2026-10-14 取得,本地缓存 `E:\trading\analysis\bitget-src\`):GitHub `metafrogmeme-droid/001` 的 `docs/bitget-uta/*.md`(由 www.bitget.com/api-doc/uta 抓取,含 WebSocket 全部频道、Trade、Market、枚举与错误码页)、官方 SDK `Bitget-AI/agent-sdk` 的 `openapi.yaml`(UTA REST 请求结构)

UTA REST 与私有推送的 `balance` 已排除现货冻结金额,`BalanceReport.free` 直接取该值,现货 `locked` 单独取原字段;永续 `locked=0`。`available` 是可用保证金/购买力,不能替代现金余额。字段依据 [官方账户频道](https://www.bitget.com/docs/uta/websocket/private/Account-Channel),冻结前后现金守恒由 BGT-324 的 demo REST/WS 金样核实。持仓 size 必须非负,posSide 只接受 long/short,均价与强平估算价按资产价格指数 HalfEven 舍入。订单推送的已解析 clientOid 必须属于当前产品线与账户,否则作为外来成交;WS 下单响应中的有效 clientOid 若与请求槽/gen 或当前账户不符则拒绝。

累计成交量推进而金额倒退、非零手续费币种未知/跨累计基线变化、多个非空 feeDetail 币种行均拒绝整帧,不改 GwCtx、不产出部分事件;同帧先后记录也经过该检查。未带新费用的重复推送保留旧费用币种,绝对值溢出拒绝。

### 测试环境 warm 账户入口

账户页面先完整解码与校验再替换 `State` 中对应观察值;坏行、重复资产/订单、负冻结余额、负价格、无效或重复游标拒绝整页,保留原观察值。撤单回执的 orderId 必须匹配请求。空资金费页若仍带非空 cursor 则拒绝,不把未完成的账单窗口标为结束。

`Templates` 仅保存账户与非敏感帧配置。warm REST/只读请求使用 `*_signed` 入口,每次从调用方借用凭证;旧无凭证入口不发认证请求。账户运行时持有 REST key;warm task 与网关 REST 池均按请求借用,全撤沿用各自通道。

当前执行接口是 `AccountProtocol::{account_request, account_decode}` 与独立 `State`,允许 warm 分配;冻结的 `acct_queries/acct_decode/acct_finish` 分步接口仍待统一,不把其无分配、完整配置序列或 ADL 缓存用例视为完成。50 KiB 持仓基准测量当前 warm 解码(包括分配),不作为旧无分配入口的达标证据。

AccountProtocol 使用 UTA V3 instruments/settings/info/assets/current-position/unfilled-orders。basic unified 账户要求单向/全仓、杠杆不超配置、无债务；key 要有 uta_trade 且无 withdraw，REST/WS 分别读取 UID 并匹配。现货订单显式 autoBorrow=no，demo REST 带 paptrading: 1。

永续资金费读 [account/financial-records](https://www.bitget.com/docs/catalog/account/assets-balance)，过滤 CONTRACT_MAIN_SETTLE_FEE_USER_IN/OUT，按 id 去重并完成分页；funding-financial-records 是资金账户流水。显式 `list=null` 且无 cursor 是合法空末页；缺 list、空页带 cursor 均拒绝。RateOnly 真实 FundingSettle 入 journal 后仅检查差异，不重复记现金。真实 demo 入账样本及未覆盖范围归 BGT-336；捕获客户端不证明引擎记账通过。

账户 UID 缺失仅在 refdata 明确 Unverifiable 时允许，报告 uid_keys=0、venue_uid 全零；Verified 缺失或不匹配仍失败。两把凭证都返回 UID 时必须一致。持仓总量必须非负，再按 posSide 赋号；本轮 MarginItem 只含非零仓位，liq_px 随当前持仓更新，ADL 缺失为 255。

私有订阅确认由 `private_ack` 按 order/position/account 映射为位 0/1/2；现货只等待 order/account。重复回执不增加完成数，unsubscribe 回执不确认 subscribe，error 回执使该频道失败。

## 已知限制与后续工作

- **无死手**:进程崩溃到 kill-switch 撤单之间有窗口(kill-switch 心跳检测周期决定)。
- **验收缺口**:账户配置与保证金轮询已有 warm `AccountProtocol` 接口及应用接线,与规格中的 `acct_*` 分步接口尚未统一;只读查询族、账户字段实测核验、完整金样(BGT-300–338)与基准仍未完成。公共 BBO REST、mock 与两条产品线 fuzz 入口已实现;固定输入按 meta 区分真实 demo 与 synthetic,人工复核和累计 fuzz 门槛仍须完成。
- **交易所侧死手未接**:官方 `POST /api/v3/trade/countdown-cancel-all`(5–60 s,需向商务申请开通)可作为 `CountdownAccount` 死手,开通后再接;当前仍靠 kill-switch。
- **资金费实收**:`account` 频道在资金费结算时推送,但不带品种与金额明细,`funding_settle = RateOnly`;实际结算经[资金费实额](#资金费实额)查询。真实账户的账单延迟与 timestamp 相对结算网格的偏差待核实(方法:持仓跨结算采集)。
- **保证金数据靠轮询**:5 s 周期;若核实账户频道推送保证金率,改为 `Push`。
- **SBE / LOLA**:官方未见;若机构团队确认存在,另开 Agent Note 评估,不在本规格内。
- **只用 UTA**:Classic 账户模式不支持;账户须已升级 UTA 且为基础模式(或经 refdata 覆盖为高级模式)。

### 开发备忘

<details><summary>给实现者</summary>

**实现前置**(未完成不得开始 `orders.rs`/`reports.rs`/`acct.rs`):方法 A = 经东京服务器抓取官方 UTA 文档原文并缓存到 `research/_src/`;方法 B = 模拟盘实测抓 raw;方法 C = 正式实盘开始后从 raw journal 补核(只用于上线后才可能出现的样本,实现前置项不得依赖它);方法 D = 向 Bitget 机构团队书面确认。

1. 私有频道全集与公共强平流仍待核实:B。普通成交标签与单向 posSide 的已采样范围归 BGT-305/317/322。
2. 每频道硬限速、错误帧是否总带 `id`、FOK/改单/STP/过期保护仍待核实:B。demo 的 place/cancel 各报告 limit=20,无效请求短突发未触发限流,不据此放宽默认限速;WS reduceOnly 的 YES 与 yes 均完成实际平仓。
3. 订单推送的累计量、均价、手续费字段(本笔还是累计)与扣费币种;成交频道是否存在、费用是否晚于订单终态:B。
4. ~~错误码全表~~(官方 UTA 错误码页已登记,见"结果未知码表");模拟盘逐码确认确定拒绝表:B。
5. 账户设置、账户信息(UID、key 权限)、账户资产、当前持仓的端点、字段与限速;是否有账户配置变化推送:A + B。
6. 开平类型字段在 UTA 中的名字与强平/ADL/交割取值:A;模拟盘高杠杆小仓制造强平(B);ADL 取不到样本时按文档构造并标"合成"。
7. 资金费实收是否有私有推送、是否带品种(决定 `funding_settle`);资金费账单的 REST 端点(reconciler 校正用):A + B(模拟盘持仓跨过一次结算);模拟盘不结算时按偏安全的 `RateOnly` 上线,C 补核。
8. 现货 BGB 抵扣的开关端点与余额不足时的回退:A + B。
9. 模拟盘尚未覆盖接口与正式环境字段差异:A + B;已观察的连接/头/符号规则归"测试网/模拟盘"。
10. 正式 V3 登录时间戳容忍范围:B;demo 秒/毫秒接受情况归 BGT-310。
11. "LOLA" 线路与 SBE 是否存在:D。
12. 私有频道双订阅的正式环境与长时间重连验证:B。本轮 demo 两条连接均收到 order/account/fill 推送;BGT-318 覆盖重复累计量只记一次,尚不代表引擎连接切换验收。
13. (启用 L2 双路合并时)`books` 双连接增量批是否逐条相同。
14. **单向持仓模式下强平/ADL/交割成交的开平类型取值**,以及单向模式普通开平的取值全集(决定"外部成交识别"表):A + B(模拟盘单向持仓高杠杆小仓制造强平)。
15. UTA 现货杠杆/借币开关的端点与字段;每品种杠杆的读取端点:A + B。
16. BBO 连接每秒一次应用层 `ping` 是否被允许(入向 10 条/s 内);`books1` 订阅后是否立即推当前快照:A + B。
17. ~~**`books5`/`books15`**~~(官方:UTA 为 `books5` 10 ms、`books50` 20 ms):剩订阅后首帧是否立即推当前快照、模拟盘是否提供。原文:UTA V3 的频道名、推送节拍、`seq` 是否单调、订阅后首帧是否立即推当前快照、模拟盘是否提供:A + B。结论写回"分档快照流"表。
18. **只读查询端点**("只读查询"表):UTA 未成交订单、成交、账单(资金费类型码)、资产、仓位的路径、分页、限速与字段:A + B(模拟盘只读 key 逐个调用并抓响应进 fixtures)。

**实现顺序**:`md.rs`(books1、books5/books15、trade、ticker)+ 金样(行情不依赖下单与账户类的核实项,可先做)→ 完成实现前置 1–6 → `orders.rs`/`reports.rs`/`external.rs` → `acct.rs` → 模拟盘冒烟。

</details>
