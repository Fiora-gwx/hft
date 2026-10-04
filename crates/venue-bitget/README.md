---
description: "Bitget UTA(统一账户)USDT 永续与现货的纯协议实现(core):books1/books 的 seq/pseq 链续、每 25 s 无条件应用层 ping、UTA V3 WS 下单帧与回报解码、账户配置核验、保证金轮询、外部成交识别与 VenueCaps 默认值;无交易所侧死手"
kind: crate
tier: pure
scope: core
status: spec
---

# venue-bitget

## 概要

`venue-bitget` 实现两个 [`VenueProtocol`](../hft-venue-proto/README.md#venueprotocol静态分派):`BitgetUsdtFutures`(USDT 本位永续,`MarketKind::LinearPerp`)与 `BitgetSpot`(现货,`MarketKind::Spot`),两者都是 core,以 **UTA(统一账户)V3** 接口为主(Classic V2 只作核对与行情对照)。它拥有 Bitget 的协议知识:`books1`(1 ms JSON 快照,BBO)、分档快照 `books5`/`books15`(core 深度)、增量 `books` 的 `seq/pseq` 严格链续(可选扩展;UTA 无 checksum)、**每 25 s 无条件发应用层 `ping`**(research/02 的口径是客户端每 30 s 发一次;服务端 120 s 没收到 ping 就断,即使数据在流,实测约 150 s;取 25 s 留余量)、`login` 签名、UTA V3 WS `place-order/batch-place/modify-order/cancel-order` 帧、`pxAmendType` 显式设置、账户模式核验、保证金与强平价轮询、强平/ADL 成交识别、`clientOid` 格式、限速与 `VenueCaps` 默认值。Bitget **没有交易所侧死手**,由 [kill-switch](../../apps/kill-switch/README.md) 兜底。

**本 crate 的待核实项多于其他 core 交易所**:UTA V3 的私有频道名、下单字段、错误码、账户与保证金端点都没有取得官方原文(本机连不上 Bitget 文档站,本地只缓存了 UTA 概览页)。凡标"待核实"的都不能当作事实实现,必须先按开发备忘的方法核实并把结论写回本 README;**官方资料中未找到 Bitget SBE 与 "LOLA" 线路**,本规格不依赖它们。纯函数,运行时见 [`hft-venue`](../hft-venue/README.md)。

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
// 编码是 hft-types 的 ClientOrderId(规范 24 位十六进制),本 crate 只加外形:<24hex>,24 字节(上限 40 与字符集待核实,开发备忘第 2 条)。
// 请求 id(WS 下单帧):"<op><slot 8 hex><gen 8 hex>"(17 字符,小写十六进制);op:p = Place、m = Amend、c = Cancel(`CancelAll` 不编码成帧,见"下单:UTA V3 WS";字段名与长度上限待核实,开发备忘第 2 条)。

pub enum MdClass { Books1, BooksTopN /* books5、books15 */, BooksIncr /* 增量 books,可选扩展 */, Trade, Ticker,
                   SubAck /* event = subscribe / unsubscribe / error */, Pong, Unknown }
pub enum ReportClass { WsResp /* 下单、改单、撤单响应 */, Orders, Fills /* 成交频道(若存在) */, Positions, Account,
                       Push /* 私有推送但频道名不在前 64 B 内:decode_report 解析 arg 后再按上列分 */,
                       Control /* event = login / subscribe / error */, Pong, Ignorable, Unknown }
```

### 账户形态(官方,本地缓存的 UTA 概览页)

UTA 有三种账户模式:**逐仓模式**(每个仓位独立风险,只支持逐仓)、**基础模式**(只用稳定币作保证金,USDT 本位与 USDC 本位合约共享保证金池,支持全仓/逐仓,现货不作抵押)、**高级模式**(多资产按折算率作保证金)。全仓账户的保证金率 =(维持保证金 + 部分强平手续费)÷ 账户权益——方向与我们的统一口径一致(越大越危险)。默认要求 **基础模式 + 全仓 + 单向持仓**(见"VenueCaps 默认值");Classic 账户不支持。

**一个账户在 Bitget 只启用 `BitgetUsdtFutures` 或 `BitgetSpot` 之一**(refdata `SameExchangeMarkets`,[hft-refdata](../hft-refdata/README.md))。UTA 下现货与合约在同一个统一账户里,本系统不在同一账户里同时交易两者,所以不建模两条产品线之间的资金占用:账户资产查询与账户频道产出的 `BalanceReport`/`MarginReport` 只归属该账户启用的那一个 venue;私有频道只订阅该产品线。一个 Bitget 子账户只归一个 `AccountId`([hft-core 多账户](../../apps/hft-core/README.md#多账户))。

### 端点

| 用途 | 端点 | 说明 |
|---|---|---|
| 公共行情(UTA V3) | `wss://ws.bitget.com/v3/ws/public`(路径待核实,开发备忘第 1 条) | CloudFront;BBO 路由 `books1`(`race=true`);分档快照路由 `public-topn`(`books5`/`books15`);成交与 `ticker` 路由各 1 条 |
| 私有 + 下单(UTA V3) | `wss://ws.bitget.com/v3/ws/private`(路径待核实,开发备忘第 1 条) | 登录后免签;primary 是私有频道归属连接,standby 只登录不订阅 |
| Classic V2(对照) | `wss://ws.bitget.com/v2/ws/public` / `private` | 现有采集系统在生产使用(公共行情 + 私有 `orders`) |
| REST | `https://api.bitget.com` | Cloudflare(GSLB);账户查询、查单、BBO 补帧(snap-fetch)、兜底;成功码 `"00000"`。REST 端点表只有一项(下标 0),两个产品线相同 |

`routes()`:`books1`(`RouteKind::Bbo`,`race = true`)、`public-topn`(`RouteKind::DepthTopN`,`topn_levels = [5, 15]`)、`public-trade`(`RouteKind::Md`)与只在永续上的 `public-ticker`(`RouteKind::Md`);每条 `max_streams = 1000`、`max_in_msgs_per_s = 10`,除 `books1` 外 `race = false`。私有推送在下单 primary 连接上,不单独建 `Role::UserStream` 连接(`endpoint(Role::UserStream, …)` → `ProtoErr::Unsupported`)。

### 行情频道 → 事件

| 频道(UTA) | 产品线 | 节拍 | 规则 | 事件 | 时间戳 |
|---|---|---|---|---|---|
| `books1`(独立 BBO 连接,双路;feed-bbo 任务) | 两者 | **1 ms**(2026-04-28 起;V2 为 10 ms) | `Snapshot`,`Monotonic` on `seq` | `Bbo` | `ex_ts_ns ← data.ts`(ms),`ex_send_ns ← 顶层 ts` |
| `books5` / `books15`(分档快照,core) | 两者 | 见"分档快照流" | `Snapshot` on `seq` | `L2Snapshot{origin = TopN}`(前 5/15 档整替换) | 同上 |
| `books`(增量深度,可选扩展) | 两者 | 50 ms,首帧快照(≤ 500 档/边)后增量 | `Chain`(`prev_id = pseq`,`last_id = seq`;重置后 `pseq = 0` 视为新快照) | `L2Snapshot{origin = Exchange}` / 每条消息一条 `L2Batch` | 同上 |
| `publicTrade`(New-Trades) | 两者 | 实时 | `Monotonic` on 成交 id | `Trade` | 成交时间 |
| `ticker` | 永续 | — | `ByTime` | `MarkPrice`、`IndexPrice`、`Funding{rate, next ← nextFundingTime}`(V2 永续 `ticker` 带资金费与持仓量,现有采集系统已验证;UTA 字段名待核实,开发备忘第 1 条) | `ts` |

- 订阅后的第一条 `trade` 推送是 `action:"snapshot"` 的历史批,丢弃,不产生 `Trade`;之后每条推送的 `data[]` 每笔一条 `Trade`(`trade_id` ← 成交 id 按十进制解析为 u64,非数字 → `DecodeErr::Number`;`taker_side` ← `side`,V2 为 taker 方向,UTA 待核实,开发备忘第 1 条)。
- `books1` 某一侧为空数组 → 产出 `Bbo`,该侧 `px = 0`、`qty = 0`(下游按"缺一侧"处理,与其他所一致);`books5`/`books15` 中数量为 0 的档不输出。
- `ticker` 的 `Funding.interval_s` = 相邻两次不同 `nextFundingTime` 之差(秒);本 `FeedCtx` 首次见到该 inst、`nextFundingTime` 尚未变过、或差值 ≤ 0 时为 0(未知)。
- 非 `Chain` 类的 `peek`:`ids.last_id` = 去重键(`books1`、`books5`/`books15` 取 `seq`,成交取成交 id,`ticker` 取 `ts` 换算的 ns),`ids.prev_id = 0`。
- 增量深度(可选扩展)的重同步:`ResyncMethod::Resubscribe`(重新订阅即得新快照)。
- 公共强平流:V2 没有;UTA 有 Liquidation 频道(格式待核实,开发备忘第 1 条),不订阅(与本账户无关)。
- 旧 V2 流实测 p50 6.5 ms、p90 48 ms、p99 140 ms,接收 → 建簿 p99 0.2 ms,重尾在交易所侧;**`books1` 1 ms 能否消掉重尾是接入后的第一个实验**([04 §5](../../design/04-行情接入层.md))。

### 下单:UTA V3 WS

| 操作 | 频道(2026-09-17 changelog;频道是否存在与字段 **M3 之前必须核实**,开发备忘第 2 条) | 对应 REST |
|---|---|---|
| 登录 | `login`:`args[{apiKey, passphrase, timestamp, sign}]`,`sign = Base64(HMAC-SHA256(secret, ts + "GET" + "/user/verify"))`,`ts` 为秒,30 s 窗口(V2 已在生产验证;V3 待核实,开发备忘第 10 条) | — |
| 下单 / 批量 | `place-order` / `batch-place` | `/api/v3/trade/place-order`、`/place-batch` |
| 改单 / 批量 | `modify-order` / `batch-modify` | `/api/v3/trade/modify-order`、`/batch-modify-order` |
| 撤单 | `cancel-order` | `/api/v3/trade/cancel-order` |
| 全撤(`CancelAll`) | 待核实(有按品种全撤则用;否则对槽镜像中该账户每个活动订单逐个 `cancel-order`;开发备忘第 2 条) | 待核实 |
| 查单 | REST 订单详情(`order_channel(Query) = Rest`,`encode_rest`;端点待核实,开发备忘第 5 条) | — |

- 每单显式设置 `pxAmendType`(超出价格带时拒单,不夹到边界),避免"成交价被静默改写"。
- 永续每单带 `reduceOnly`(字段名待核实,开发备忘第 2 条)只在 `oflags` 带 `REDUCE_ONLY` 时为真;现货带 `REDUCE_ONLY` → `EncodeErr::Unsupported`。
- 每频道限速、批量上限、响应时间戳字段、过期保护字段**待核实**(开发备忘第 2 条);核实前按 10 单/s 与"无过期字段"处理(`stale_guard() = None`,见实现说明第 7 点);响应无网关时间戳时 `ReportOutcome.gw_dwell_us = 0`。
- `encode`:`Tif::Gtc`/`Ioc`/`PostOnly` 写成 UTA 对应写法(待核实,开发备忘第 2 条);`Tif::Fok` → `EncodeErr::Unsupported(Place)`(`flags` 无 `FOK`);价/量按 inst 的 tick/lot 格式化为最短十进制(不补零、不经 f64);`Amend` → `modify-order`;`ORDER_FRAME_CAP = 2048`。`encode_batch` 保持默认 `Unsupported`(批量上限待核实,开发备忘第 2 条;[hft-oms](../hft-oms/README.md) 每个意图一个槽一个帧)。
- `CancelAll`:按品种全撤核实前,`encode` 对 `CancelAll`(`inst = 0` 与按 inst 都)返回 `EncodeErr::Unsupported(CancelAll)`;运行时把槽镜像中该账户的活动订单展开为逐条 `Cancel`([hft-venue 出环](../hft-venue/README.md#出环outmsg--gwcmd) 的第 ③ 级;`encode_batch` 不支持,逐条 `encode`,请求 id = `c` + 各订单的 slot、gen),回报按 `Cancel` 解码;`CANCEL_ALL_SWEEP = false`。
- `Query` 走 REST 订单详情(端点待核实,开发备忘第 5 条),由 `encode_rest` 按 `clientOid` 生成,签名同实现说明第 8 点;响应经 `decode_rest`(槽取自 `RestOrigin`):订单状态 → `OrderEvent.detail`(契约 28)、累计成交量与均价 → `QueryResult`(`cum_filled`、`avg_px` 必填,契约 9);"订单不存在" → NotFound 形态(`detail = 7`),不定终态。

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
| 两者 | `books5`、`books15` | 5 / 15 | 待核实(V2 口径 `books5` 10 ms;开发备忘第 17 条) | `seq` 单调(`Snapshot`);`data.ts`(ms)、顶层 `ts` | 路由 `public-topn`(`RouteKind::DepthTopN`),UTA V3 `/v3/ws/public` | 频道名:V2 自测;UTA 写法与字段待核实(开发备忘第 1、17 条) |

每条推送是完整的前 N 档;订阅后首帧即当前快照(待核实,开发备忘第 17 条)。`seq`/`ts` 的数字与字符串两种形式都接受(实现说明第 3 点)。

### L2 序号归一化(`L2Batch.prev_id/last_id`)

增量深度(可选扩展)用;规格保留,`feed.depth = incremental` 时才生效。

| 流 | `prev_id` | `last_id` | 快照 `snap_id` |
|---|---|---|---|
| `books` 增量 | `pseq` | `seq` | 快照帧(或 `pseq = 0` 的重置帧)的 `seq` |
| 快照后第一条 | `snap_id`(`RxMeta.prev_override`) | `seq` | — |

契约见 [hft-venue-proto](../hft-venue-proto/README.md#实现方必须遵守的契约)。

### 累计量与费用来源

| 流 | `cum_filled` | `avg_px` | 费用 |
|---|---|---|---|
| UTA 私有订单频道 | 订单级累计成交量字段(V2 为 `accBaseVolume`,UTA 字段名待核实,开发备忘第 3 条) | 均价字段(V2 `priceAvg`) | 手续费字段(V2 `fillFee`/`feeDetail[]`,含 `feeCoin`;本笔或累计待核实;若为累计,`GwCtx` 取差分)+ 扣费币种 |
| UTA 私有成交频道(若存在,待核实,开发备忘第 3 条) | — | — | 若费用只在成交频道出现且晚于订单终态 → `FeeOnly` |
| REST 查单 | 累计成交量 | 均价 | — |

若 UTA 推送不带累计量,则在 `GwCtx` 按成交 id 去重累加合成(契约 9;重启后合成表从空开始,这种情形下的收敛办法须在核实时一并写明,见 [hft-venue 已知限制](../hft-venue/README.md#已知限制与后续工作));去重键 = `(inst, 成交 id)`,成交 id 缺失、0 或负时 `(inst, 订单 id, 累计成交量)`(`TradeIdRing<1024>`,[契约 22](../hft-venue-proto/README.md#实现方必须遵守的契约))。`funding_settle = RateOnly`,不产出资金费推送,没有资金费去重键。

### 结果未知码表

`src/codes.rs` 两张表([契约 8](../hft-venue-proto/README.md#实现方必须遵守的契约))。码的形态:字符串(V2 为纯数字字符串,自测;UTA 待核实,开发备忘第 4 条),按契约 8 编码为 `reject_code`/`raw_code`。**核实前确定拒绝表只登记在模拟盘(或正式实盘开始后的 raw journal)中观察到、且有官方错误码页原文佐证的参数类与余额类错误**;系统错误、超时、限流类及一切未登记码一律 `outcome_unknown`(偏安全:多一次查询,不会把已成交的单当成拒绝)。

| 表 | 码 | 来源 |
|---|---|---|
| 确定拒绝 | 核实前为空;核实后登记参数错误、余额不足、价格超出价格带(`pxAmendType` 生效时)、reduceOnly 被拒、限速类 | 待核实(开发备忘第 4 条) |
| 结果未知 | 系统繁忙/超时/内部错误类(确切码待核实,开发备忘第 4 条),HTTP 5xx,以及**任何未登记码** | 契约默认 |

`GONE` 类(撤改失败因订单已成交、已撤或不存在,契约 28)与置控制提示的码(契约 30:时间戳越界 → `ClockSkew`,限速 → `RateLimited`)随确定拒绝表一起核实登记(开发备忘第 4 条);核实前撤改的错误一律结果未知、`detail = 0`,只有 HTTP 429 置 `ctl = RateLimited`。

### VenueCaps 默认值

| 字段 | `BitgetUsdtFutures` | `BitgetSpot` | 来源 |
|---|---|---|---|
| `fee_asset_rule` | `Quote`(USDT) | `ReceivedAssetUnlessDiscount{BGB}`(BGB 抵扣开启且余额足 → BGB;否则成交所得资产) | 待核实(开发备忘第 8 条) |
| `reduce_only` | 是 | 否 | 待核实(UTA 字段,开发备忘第 2 条) |
| `required_position_mode` | 单向持仓 | 不适用 | 待核实(字段,开发备忘第 5 条) |
| `required_margin_mode` | 全仓;多资产关闭(UTA 基础模式) | 不适用 | 模式:官方;字段:待核实(开发备忘第 5 条) |
| `required_spot_borrow` | `None`(永续) | `Some(false)`:UTA 现货杠杆 / 借币关闭 | 待核实(端点与字段,开发备忘第 15 条) |
| `funding_settle` | `RateOnly`(未找到资金费实收推送;找到后改 `PerSymbolPush`/`BalanceOnlyPush`) | 不适用 | 待核实(开发备忘第 7 条) |
| `margin_data` | `Poll{interval_ms: 5000}`(若核实账户频道推送保证金率,改 `Push`) | 不适用 | 待核实(开发备忘第 5 条) |
| `own_liquidation_marker` | 订单/成交的开平类型字段。双向持仓取值(Classic V2 `tradeSide`):`burst_close_long`/`burst_close_short`(强平)、`dte_sys_adl_close_long`/`dte_sys_adl_close_short`(ADL)、`delivery_close_long`/`delivery_close_short`(交割)。**本系统要求单向持仓,单向模式下的取值待核实**(候选:`burst_buy_single`/`burst_sell_single`、`dte_sys_adl_buy_in_single_side_mode`/`dte_sys_adl_sell_in_single_side_mode`、`delivery_buy_single`/`delivery_sell_single`,来自记忆中的 V2 枚举,未见原文);UTA 字段名待核实 | 无(基础模式现货无强平) | 待核实(开发备忘第 6、14 条) |
| `ignorable_user_events` | 空(核实私有频道全集后补) | 空 | 待核实(开发备忘第 1 条) |
| `amend` | 是(`modify-order`) | 是 | changelog / 待核实(开发备忘第 2 条) |
| `amend_qty_down_only` | 否 | 否 | 待核实(开发备忘第 2 条:`modify-order` 能否改价与增量) |
| `post_only` | 是(字段待核实) | 是 | 待核实(开发备忘第 2 条) |
| `ioc` | 是 | 是 | 待核实(开发备忘第 2 条) |
| `fok` | 待核实(默认否,核实前不下 FOK) | 待核实(默认否) | 待核实(开发备忘第 2 条) |
| `batch_cancel` | 待核实(默认否:逐个撤) | 待核实 | 待核实(开发备忘第 2 条) |
| `dms` | `None`(`DeadManSpec::None`)→ kill-switch 兜底 | `None` | research/02(未见官方死手) |
| `testnet` | `Demo`(UTA 文档目录有"Demo Trading"页;端点、请求头与模拟盘符号见"测试网/模拟盘",待核实) | 同左 | 官方目录 / 待核实(开发备忘第 9 条) |

`stale_guard()` = `None`(核实前);`dead_man()` = `None`;`dead_man_refresh` 被调用 → `EncodeErr::Unsupported(GwOp::DeadManRefresh)`。

`default_caps()` 中上表未写出码值的字段:`own_liquidation_marker`:永续 `LiqMarker::OrderTypeField`,`liq_marker_arg = "burst_close_long|burst_close_short|dte_sys_adl_close_long|dte_sys_adl_close_short|delivery_close_long|delivery_close_short"`(单向模式取值核实后追加),现货 `NotIdentifiable`、`""`;`required_multi_asset`:永续 `Some(false)`,现货 `None`;`flags` = `AMEND | POST_ONLY | IOC`(= 7,两者);`dms` = `DeadManCap::None`,四个 `dms_*_ms` 为 0;`testnet` = `TestnetKind::Demo`;`fee_asset_rule` 里的 `BGB` 以资产名给出,由 hft-cfg 按 refdata 资产表解析为 `AssetId`。

### 账户配置核验

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
| 开平类型 ∈ {`burst_close_long`, `burst_close_short`} 或单向模式的强平取值(待核实) | `Liquidation` | 待核实(V2 双向取值;单向取值未见原文;开发备忘第 6、14 条) |
| 开平类型 ∈ {`dte_sys_adl_close_long`, `dte_sys_adl_close_short`} 或单向模式的 ADL 取值(待核实) | `Adl` | 待核实(开发备忘第 6、14 条) |
| 开平类型 ∈ {`delivery_close_long`, `delivery_close_short`} 或单向模式的交割取值(待核实) | `Delivery` | 待核实(开发备忘第 6、14 条) |
| 开平类型不在"本系统可能产生的取值"集合内(单向模式的普通开平,核实后列出)且 `clientOid` 非我们的格式 | `Unknown`(偏安全:仍作外部成交入账并 P0) | — |
| `parse_cid(clientOid)` 成功 | 本系统订单 → `OrderEvent` | — |
| `clientOid` 为空或非我们格式 | `Unknown`(人工单的来源标记待核实,有则 `Manual`) | 待核实(开发备忘第 3 条) |

"我方订单的成交推送必带 `clientOid`"待核实(开发备忘第 3 条);核实前按"不保证"处理:`clientOid` 缺失的成交交给运行时按 `IdTable` 解析,超时转 `ExternalFill{Unknown}`([hft-venue §4](../hft-venue/README.md#4-gwruntimepoll_once-的循环结构))。

### 用户流事件表

| 消息(UTA 私有频道,名称待核实:开发备忘第 1 条) | 产出 | 提示 |
|---|---|---|
| 订单频道 | `OrderEvent` 或 `ExternalFill` | — |
| 成交频道(若存在) | 费用补齐(`FeeOnly`)或外部成交 | — |
| 持仓频道 | `PositionReport`(只校验) | — |
| 账户频道 | 每币种一条 `BalanceReport{free ← 可用, locked ← 冻结}`(V2 `available`/`frozen`;只校验);`margin_data = Poll` 期间不产出 `MarginReport`;若核实带保证金率则 `MarginReport`(并把 `margin_data` 改为 `Push`;推送在 gw 线程上解码,预算上限见 [hft-venue](../hft-venue/README.md#延迟与资源契约) `venue_gw_margin_push`) | — |
| `{"event":"login"|"subscribe"|"error"}` | 无(控制回执;订阅被拒 `code` 为数字,如 `30001`) | — |
| `pong` | 无(心跳回应) | — |
| 其他 | 无(`skip = Unknown`;核实后把确认无用的名字加入 `ignorable_user_events`) | — |

### 测试网/模拟盘

UTA 文档目录有"Demo Trading"一节(官方,本地缓存的目录页),端点(行情与下单)、请求头、模拟盘符号与主网符号的映射(Classic V2 的模拟盘曾用带前缀的符号与产品类型,UTA 是否沿用待核实)、API key 申请方式与 UTA V3 接口的覆盖范围待核实(开发备忘第 9 条);模拟盘读不到 UID 时该账户按 `uid_policy = "unverifiable"` 配置(规则归 [hft-refdata](../hft-refdata/README.md#数据结构))。模拟盘 refdata 由 hft-cfg 按模拟盘的品种接口单独生成。上线门槛第 ⑥ 项([testing §8](../../docs/testing.md#8-上线门槛))要求 Bitget 模拟盘联调,行情与下单都用模拟盘(`feed_env = "testnet"` + 模拟盘 refdata);模拟盘不覆盖的接口逐项记录,处理方式归 testing §8,不以实盘小单替代。

### 心跳与 BBO 探测

按连接种类分别写([契约 24](../hft-venue-proto/README.md#实现方必须遵守的契约)):

| 连接 | 服务端行为 | 我们的动作 | 来源 |
|---|---|---|---|
| 公共 / 私有(UTA V3、Classic V2) | 120 s 没收到客户端 `ping` 即断,即使数据在流(实测约 150 s) | 每 25 s **无条件**发文本 `ping`,10 s 内须回 `pong`(实现说明第 1 点) | 现有采集系统 2026-09-04 事故 + research/02 |
| BBO 连接(`books1`,双路) | 同上 | `bbo_probe()` = 文本 `ping`(服务端是否回 WS 协议 Pong 待核实),每 `feed.bbo_ping_ms` 一次,同时满足上一行的 25 s 要求;`pong` 计入存活 | 待核实(开发备忘第 16 条) |

- 入向上限 10 条/s/连接(超出被断):BBO 连接的探测占 1 条/s,订阅帧按 4 条/s 发送。
- BBO 补帧:`books1` 每条都是完整一档快照,订阅后是否立即推送待核实;核实前 `bbo_snapshot_request` 返回 `Ok(true)`,用 UTA 行情 REST 的一档深度接口(路径待核实;公共接口,不签名),`BBO_SNAPSHOT_BATCH = 1`(`insts.len() > 1` → `ProtoErr::BufFull`);`decode_bbo_snapshot` 的 inst 取自传入的 `insts`,`update_id` 取响应里的序号,无则 0。两项见开发备忘第 16 条。

### 只读查询

`ro_request`/`ro_decode` 的实现([契约 31](../hft-venue-proto/README.md#实现方必须遵守的契约));reconciler 以只读 key 构建 `Templates` 调用,gw 不用(`CANCEL_ALL_SWEEP = false`)。UTA 端点、分页与字段**全部待核实**(开发备忘第 18 条),核实前按下表"要读到什么"实现,读不到即 `DecodeErr::Shape`。

| `RoKind` | 请求(候选,两个产品线) | 要读到什么 |
|---|---|---|
| `OpenOrders` | UTA 未成交订单查询 | 每单 `clientOid`、订单 id、品种、方向、价、量、累计成交量、状态 → `open_order` |
| `Fills` | UTA 成交查询(按时间与游标翻页) | 成交 id、订单 id、`clientOid`(若带)、量、价、费用与币种、时间 → `fill` |
| `FundingFlow` | UTA 账单(资金费类型);现货 → `Unsupported` | 品种、金额、结算时刻 → `funding` |
| `Balance` | UTA 资产 | 每币可用与冻结 → `balance` |
| `Positions` | UTA 当前仓位;现货 → `Unsupported` | 每品种持仓、开仓均价、未实现盈亏、强平价 → `position` |

### `mock` 特性

与 [venue-binance](../venue-binance/README.md#mock-特性给-mock-exchange-的反向编解码) 相同的四个反向编解码函数,供 optional 的 [mock-exchange](../../apps/mock-exchange/README.md) 使用。

## 实现说明

<details><summary>实现要点</summary>

1. **心跳**:`HeartbeatSpec{ping: AppText("ping"), interval_ms: 25000, unconditional: true, pong_timeout_ms: 10000, idle_timeout_ms: 30000}`——**无论是否有数据都按周期发**(≤ 30 s)。现有采集系统在 2026-09-04 只在空闲时 ping,所有连接在建立约 150 s 后被服务端 reset;这条规则是协议常量,不是可调参数。
2. **分派**:推送 `{"arg":{"instType":…,"channel":"books1","instId":…},"action":…,"data":[…],"ts":…}`(UTA 的 `arg` 字段名待核实,开发备忘第 1 条:`topic`/`symbol` 或 `channel`/`instId`;两种写法都接受)。V2 推送以 `{"action":"snapshot",` 开头时频道名从第 65 字节起,不在 `classify` 的 64 B 内,所以**行情按 `route` 分类**:以 `{"action":` 或 `{"arg":` 开头的数据推送 → 该路由的类(`books1` → `Books1`,`public-topn` → `BooksTopN`,`public-trade` → `Trade`,`public-ticker` → `Ticker`);`{"event":"subscribe"`/`"unsubscribe"`/`"error"` → `SubAck`(`{"event":"error","code":30001` 为订阅被拒,`code` 是数字);文本 `pong` → `Pong`;其余 → `Unknown`。私有连接:`{"id":"` 后第一个字符是 op 码 → `WsResp`;`{"event":` → `Control`;`pong` → `Pong`;推送的频道名完整落在前 64 B 内则按"用户流事件表"分类,否则 `Push`(由 `decode_report` 解析 `arg` 后分类,结果相同);`classify_report` 不认得的频道名为 `Unknown`,`decode_report` 再查生效 `ignorable_user_events`(命中 → `skip = Listed`,否则 `skip = Unknown`,契约 20)。
3. **数字宽容**:`seq` 为数字,`ts` 为字符串;解析器同时接受数字与字符串形式(现有采集系统的做法)。价/量按 hft-types `parse_exact` 解析(科学计数法照收);`FxErr::OVERFLOW` → `DecodeErr::Overflow`,其余解析错误(含超精度非零位)→ `DecodeErr::Number`。
4. **订阅**:每帧 ≤ 50 个 `args`,不带 `id`;`StreamKind` → 频道:`Bbo` → `books1`,`DepthTopN` → `books5`/`books15`(`levels` 为 5 或 15,其他 → `ProtoErr::Unsupported`),`Trade` → `publicTrade`,`Mark`/`Index`/`Funding` 同一 inst 合并为一个 `ticker`(只永续),`Liquidation` 与现货的 `Mark`/`Index`/`Funding` → `ProtoErr::Unsupported`;按 10 条/s 入向上限的一半限速。V2 私有 `orders` 用 `instId: "default"` 订阅全账户(生产验证);UTA 写法待核实(开发备忘第 1 条)。
5. **回报**:下单响应 → `Ack`(`exch_order_id ← orderId`),改单响应 → `AmendAck`,撤单响应 → `CancelAck`;登记为确定拒绝的错误码 → `Reject`(撤单为 `CancelReject`、改单为 `AmendReject`);其余 → `outcome_unknown`(契约 8);私有订单频道 → 先过"外部成交识别",再按订单状态:`live` → `Ack`,有本笔成交的 `partially_filled`/`filled` → `PartialFill`/`Fill`,撤销 → `CancelAck`,但订单 tif 为 IOC 的撤销 → `Expired`(V2 `status`/`force` 取值;UTA 待核实,开发备忘第 3 条);持仓、账户频道 → `PositionReport`/`BalanceReport`。成交字段(V2 名):本笔量 `baseVolume` → `qty`、`fillPrice` → `px`、`accBaseVolume` → `cum_filled`、`priceAvg` → `avg_px`、`fillFee`/`fillFeeCoin` → `fee`/`fee_asset`(V2 以负数表示扣费,本层取反为正,待核实第 3 条)、`orderId` → `exch_order_id`、`tradeId` → 去重键、`fillTime` → `ex_ts_ns`。字段名以 UTA 金样为准。扣费币种与 `fee_asset_rule` 不符 → `fee_rule_deviation`。
6. **死手**:`DeadManSpec::None`。kill-switch 进程持有撤单权限的 REST 凭证,在 hft-core 心跳消失时撤该账户全部挂单([09 §4.2](../../design/09-可观测性、风控与控制面.md))。
7. **无过期保护**:核实前 `stale_guard() = StaleGuard::None`。查询得到"订单不存在"不能按超时定终态(迟到的订单可能仍会被撮合),OMS 只能等对账收敛([契约 10](../hft-venue-proto/README.md#实现方必须遵守的契约))。
8. **账户查询**:REST 签名 `Base64(HMAC-SHA256(secret, ts + METHOD + path + query + body))`,头 `ACCESS-KEY/ACCESS-SIGN/ACCESS-TIMESTAMP/ACCESS-PASSPHRASE`(research/02);只保留 refdata 中的品种写入 `AcctScratch`。
9. **源文件**:`lib.rs`、`caps.rs`、`cid.rs`、`sign.rs`、`md.rs`(books1、books5/books15、trade、ticker;增量 books 为可选扩展)、`orders.rs`、`reports.rs`(含用户流事件表)、`external.rs`、`acct.rs`(Config 与 Margin 序列)、`codes.rs`(确定拒绝 / 结果未知两张表)。

</details>

## 延迟与资源契约

| 入口 | p50 / p99 预算 | 基准名 |
|---|---|---|
| `classify` | ≤ 30 ns / ≤ 60 ns | `bench_classify_bitget` |
| `decode_md` books1 → `Bbo` | ≤ 0.4 µs / ≤ 0.8 µs | `bench_decode_bitget_books1` |
| `decode_md` books15 → `L2Snapshot{TopN}` | ≤ 1 µs / ≤ 2 µs | `bench_decode_bitget_books15` |
| `decode_md` books 增量 1 KiB(增量深度,可选扩展) | ≤ 1.5 µs / ≤ 3 µs | `bench_decode_bitget_books` |
| `encode` place-order | ≤ 0.3 µs / ≤ 0.6 µs | `bench_encode_bitget_place` |
| `decode_report` 订单推送(含外部成交判定与去重) | ≤ 1 µs / ≤ 2 µs | `bench_decode_bitget_order` |
| `acct_decode` 持仓列表 50 KiB | ≤ 60 µs / ≤ 100 µs | `bench_acct_decode_bitget` |

`books1` 1 ms 节拍下每个 inst 最多 1000 条/s,feed 核余量按 [01 §3.2](../../design/01-总体架构.md) 的"每条 2–5 µs"估算仍 ≥ 90%。纯函数,热路径零分配零系统调用。

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

## 已知限制与后续工作

- **无死手**:进程崩溃到 kill-switch 撤单之间有窗口(kill-switch 心跳检测周期决定)。
- **待核实项多**(开发备忘第 1–9 条):UTA V3 私有频道、下单字段、错误码、账户/保证金端点、扣费规则、资金费推送;核实完成前本 crate 不能通过上线门槛。
- **保证金数据靠轮询**:5 s 周期;若核实账户频道推送保证金率,改为 `Push`。
- **SBE / LOLA**:官方未见;若机构团队确认存在,另开 Agent Note 评估,不在本规格内。
- **只用 UTA**:Classic 账户模式不支持;账户须已升级 UTA 且为基础模式(或经 refdata 覆盖为高级模式)。

### 开发备忘

<details><summary>给实现者</summary>

**实现前置**(未完成不得开始 `orders.rs`/`reports.rs`/`acct.rs`):方法 A = 经东京服务器抓取官方 UTA 文档原文并缓存到 `research/_src/`;方法 B = 模拟盘实测抓 raw;方法 C = 正式实盘开始后从 raw journal 补核(只用于上线后才可能出现的样本,实现前置项不得依赖它);方法 D = 向 Bitget 机构团队书面确认。

1. UTA V3 WS 公共/私有路径、订阅 `args` 字段名、私有频道名全集(订单、成交、持仓、账户):A + B。
2. **UTA V3 WS 下单(M3 之前必须核实,决定 egress 的形态)**:`place-order` 等频道是否在 WS 上可用(否则下单只能走 REST,下单路径需重新设计)、字段(`clientOid` 长度与字符集、`reduceOnly`、post-only、IOC、FOK 写法、`pxAmendType`)、每频道限速、批量上限、响应字段(是否有网关时间戳)、STP、过期保护:A + B。
3. 订单推送的累计量、均价、手续费字段(本笔还是累计)与扣费币种;成交频道是否存在、费用是否晚于订单终态:B。
4. 错误码全表,逐码分为确定拒绝 / 结果未知:A + D。
5. 账户设置、账户信息(UID、key 权限)、账户资产、当前持仓的端点、字段与限速;是否有账户配置变化推送:A + B。
6. 开平类型字段在 UTA 中的名字与强平/ADL/交割取值:A;模拟盘高杠杆小仓制造强平(B);ADL 取不到样本时按文档构造并标"合成"。
7. 资金费实收是否有私有推送、是否带品种(决定 `funding_settle`);资金费账单的 REST 端点(reconciler 校正用):A + B(模拟盘持仓跨过一次结算);模拟盘不结算时按偏安全的 `RateOnly` 上线,C 补核。
8. 现货 BGB 抵扣的开关端点与余额不足时的回退:A + B。
9. 模拟盘的行情与下单端点、请求头与符号规则(与主网符号的映射,门槛 ⑥ 用模拟盘行情与模拟盘 refdata):A + B。
10. 登录 `timestamp` 在 V3 是否改为 ms:B。
11. "LOLA" 线路与 SBE 是否存在:D。
12. **私有频道双订阅(M3 之前必须核实)**:同一账户两条私有连接同时订阅订单频道是否都收到推送(决定先建后断):B。不允许时切换退化为"先断后建 + VERIFY"并写进本 README。
13. (启用 L2 双路合并时)`books` 双连接增量批是否逐条相同。
14. **单向持仓模式下强平/ADL/交割成交的开平类型取值**,以及单向模式普通开平的取值全集(决定"外部成交识别"表):A + B(模拟盘单向持仓高杠杆小仓制造强平)。
15. UTA 现货杠杆/借币开关的端点与字段;每品种杠杆的读取端点:A + B。
16. BBO 连接每秒一次应用层 `ping` 是否被允许(入向 10 条/s 内);`books1` 订阅后是否立即推当前快照:A + B。
17. **`books5`/`books15`**:UTA V3 的频道名、推送节拍、`seq` 是否单调、订阅后首帧是否立即推当前快照、模拟盘是否提供:A + B。结论写回"分档快照流"表。
18. **只读查询端点**("只读查询"表):UTA 未成交订单、成交、账单(资金费类型码)、资产、仓位的路径、分页、限速与字段:A + B(模拟盘只读 key 逐个调用并抓响应进 fixtures)。

**实现顺序**:`md.rs`(books1、books5/books15、trade、ticker)+ 金样(行情不依赖下单与账户类的核实项,可先做)→ 完成实现前置 1–6 → `orders.rs`/`reports.rs`/`external.rs` → `acct.rs` → 模拟盘冒烟。

</details>
