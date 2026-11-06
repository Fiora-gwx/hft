---
description: "Coinbase Exchange 现货的只读行情协议(optional):ticker → Bbo(双路赛马)、matches → 成交、heartbeat 存活、订阅帧、VenueCaps 与限速默认值;无下单、无 key、无账户"
kind: crate
tier: pure
scope: optional
status: impl
---

# venue-coinbase

## 概要

本 crate 实现 Coinbase Exchange 现货的 `VenueProtocol`(`coinbase-spot`,`VenueId::COINBASE_SPOT` = 16,`Exchange::Coinbase` = 9),**只读行情**:`ticker` 频道产出 `Bbo`(独立的 `RouteKind::Bbo` 路由,两条连接赛马、按 `sequence` 去重),`matches` 频道产出 `Trade`(`RouteKind::Md` 路由),`heartbeat` 频道是应用层存活信号。没有下单、回报、账户、key;这些方法一律返回 trait 允许的 `Unsupported`/`UnknownClass` 形式,hft-core 拒绝给本协议的 venue 配 `accounts`(`ReadOnlyVenue`)。它拥有分派、`peek`、解码、订阅帧、心跳与能力默认值,不拥有连接、线程与 I/O(hft-venue 运行时)。触发条件:Polymarket 做市需要第二个参考行情——替代外部 sidecar `cb_feed.py`(经 `InjectWarmup local_kind 3` 推 Coinbase BBO 样本);行情进 `raw-feed-<v>` 与 seq journal,研究回放无需改动即可重放。

## 目录

- [使用本组件](#使用本组件)
  - [端点与路由](#端点与路由)
  - [订阅](#订阅)
  - [行情频道 → 事件](#行情频道--事件)
  - [多连接去重](#多连接去重)
  - [时间戳](#时间戳)
  - [心跳与 BBO 探测](#心跳与-bbo-探测)
  - [限速默认值](#限速默认值)
  - [VenueCaps 默认值](#venuecaps-默认值)
  - [账户](#账户)
  - [接入清单中不适用的步骤](#接入清单中不适用的步骤)
- [实现说明](#实现说明)
- [延迟与资源契约](#延迟与资源契约)
- [测试与验收](#测试与验收)
- [依赖](#依赖)
- [延伸阅读](#延伸阅读)
- [已知限制与后续工作](#已知限制与后续工作)

## 使用本组件

```rust
pub struct CoinbaseSpot;                   // NAME = "coinbase-spot", VENUE = COINBASE_SPOT, MARKET = Spot, SUB_ACKS = true
pub enum MdClass { Ticker, Match, LastMatch, Heartbeat, Subscriptions, Error, Unknown }
pub enum ReportClass { Ignorable, Unknown }  // 无私有流,只为满足契约 20
pub struct FeedCtx;                        // 无状态
pub const ROUTE_BBO: RouteId;              // RouteId(1),"ticker"
pub const ROUTE_MD: RouteId;               // RouteId(2),"matches"
pub fn iso8601_ns(s: &[u8]) -> Result<i64, DecodeErr>;   // 交易所时间 → Unix ns
// GwCtx = (),Templates = ();AccountProtocol / FundingHistoryProtocol 全部 Unsupported
```

hft-core 装配:`venues.<v>.protocol = "coinbase-spot"`,与 `binance-spot` 参考行情同样(`--dry-run` 下 `live_feed = true`,网络模式下无账户的行情 venue);`feed.bbo_conns` 缺省 2(BBO 双路赛马)。配置由 `tools/hft-pmcfg` 写入(`[venues.cb]`,BTC-USD = InstId 8,`--no-coinbase` 不写)。

### 端点与路由

| 用途 | 端点 | 来源 |
|---|---|---|
| 行情 WS(两条路由同一主机,路径 `/`) | `wss://ws-feed.exchange.coinbase.com` | 官方 Exchange WebSocket Overview |
| REST(hft-core 装配 snap-fetch 要求有一个 `Rest` 端点;本协议不发 REST 请求) | `https://api.exchange.coinbase.com` | 官方 |

用 **Exchange** 公共行情,不用 Advanced Trade(`advanced-trade-ws.coinbase.com`):2026-11-06 在 eu-west-1 对同一批成交实测 Advanced Trade 晚约 60 ms 到达(自测)。到达本机延迟 Coinbase p50 约 49–55 ms,Binance 约 102 ms(自测,同日)。白名单 `hft-core venues::public_market_endpoint`:`ws-feed.exchange.coinbase.com`(MdPublic/MdMarket)、`api.exchange.coinbase.com`(Rest)。

| 路由 | `RouteId` | 种类 | 频道 | 赛马 | 承载流 |
|---|---|---|---|---|---|
| `ticker` | 1 | `Bbo` | `ticker` + `heartbeat` | 是(`feed.bbo_conns`,缺省 2) | `StreamKind::Bbo` |
| `matches` | 2 | `Md` | `matches` + `heartbeat` | 否(hft-venue 的 Md 路由每组 1 条) | `StreamKind::Trade` |

其余流(`DepthTopN`、标记价、资金费、强平)没有路由(`stream_route` → `None`)。同一主机的两条连接:一条在 96.5% 的成交上先到,平均早 9 ms(p99 49 ms,自测 2026-11-06);hft-net 按连接轮转主机解析出的 IPv4 地址,两条连接落在不同服务器地址上。

### 订阅

- 帧:`{"type":"subscribe","product_ids":["BTC-USD",…],"channels":["ticker","heartbeat"]}`(Md 路由为 `"matches"`)。一帧至多 64 个产品,只覆盖同一种流的前导条目;产品名取 refdata 符号,只允许 `[A-Z0-9-]`(否则 `ProtoErr::Cfg`)。退订帧只列数据频道(`{"type":"unsubscribe",…,"channels":["ticker"]}`),`heartbeat` 保留,所以每个成功应答的 `channels` 都非空。
- 应答(自测 2026-11-06,官方 Overview):每个订阅帧恰好一条 `subscriptions`,列出该连接当前的全部订阅 → `ControlHint::SubAck{ok: true}`;被拒的帧先回 `{"type":"error","message":"Failed to subscribe","reason":…}` → `SubAck{ok: false, code: 1}`,随后一条 `channels` 为空的 `subscriptions` → 不算应答(`control` 返回 `None`,计入 unknown,写 raw)。一帧里部分产品无效时可能出现 `error` 与非空 `subscriptions` 两条应答(多出一条 stray ack);产品来自 refdata,实际不出现。
- 连接后 5 s 内不订阅即被断开(官方)。
- 订阅 `ticker` 后交易所立即推当前 ticker(即最近一笔成交时的盘口,实测)→ `bbo_snapshot_request` 取默认 `Ok(false)`,不做 REST 补帧(REST `/products/<id>/ticker` 只有价没有量,也没有 `sequence`)。
- 订阅 `matches` 后交易所先推一条 `last_match`(订阅前的最近成交,实测):校验后不产出事件(`decode_md` 返回 0 个事件),但它的 `trade_id` 经 `Monotonic` 成为成交水位。

### 行情频道 → 事件

| 消息(`type`) | 类 | 事件 | 字段映射 | 序号(`peek`) |
|---|---|---|---|---|
| `ticker` | `Ticker` | `Bbo` | `best_bid`/`best_bid_size`/`best_ask`/`best_ask_size` → 买卖价量;`sequence` → `update_id`;`time` → `ex_ts_ns` | `sequence`,`Monotonic` |
| `match` | `Match` | `Trade` | `price`、`size`、`trade_id`;`side` 是 **maker** 方向:`sell` → `taker_side = Buy`,`buy` → `Sell`(官方 Channels;实测 `last_match` 的 `side` 与同一成交 `ticker` 的 `side` 相反);`time` → `ex_ts_ns` | `trade_id`,`Monotonic` |
| `last_match` | `LastMatch` | 无 | 同 `match`,只校验 | `trade_id`,`Monotonic` |
| `heartbeat` | `Heartbeat` | 无(`ControlHint::Pong`) | — | — |
| `subscriptions` / `error` | 见[订阅](#订阅) | 无 | — | — |
| 其他 | `Unknown` | 无(计数,写 raw) | — | — |

- `ticker` 只在成交时推(批量撮合时合并);2026-11-06 的 120 s 样本里 1 319 条 ticker 对 1 318 条 match + 1 条 `last_match`,`sequence` 与同一笔 match 的相同。**盘口只随成交更新**:两笔成交之间的挂单变化看不到,消费者应看 `ex_ts_ns`。
- 价格与数量按 refdata 线格式指数精确解析(`parse_exact`,多出小数位 → `Number`);未知产品 → 0 个事件(`inst = 0`)。
- `ex_send_ns` = 0(报文没有推送时间)。

### 多连接去重

两条 `ticker` 连接送来同一组消息(实测 120 s:两条连接各 1 319 条、`sequence` 集合完全相同、各自严格递增)。去重由 hft-venue 的 `SeqRule::Monotonic` 在 `InstSeq.last_fwd` 上完成(`sequence ≤` 已转发 → 丢弃并计 `dup`),本 crate 不保存状态(`FeedCtx` 无状态)。重连后交易所重推的当前 ticker 与已转发的 `sequence` 相同 → 丢弃;BBO 补帧期间按 `≥` 转发。`sequence` 跳号不是断裂(ticker 不是每个 `sequence` 都有),不检测缺口。`matches` 只有一条连接(hft-venue 的 Md 路由每组 1 条);`trade_id` 每产品严格递增(实测 1 318 条无缺口)。

### 时间戳

`time` 是 ISO-8601 UTC(`2026-11-06T15:58:52.472727Z`,微秒;REST 为纳秒),`iso8601_ns` 接受 `YYYY-MM-DDTHH:MM:SS[.f{1,9}]Z`(年份 1970–2261),其余形态 → `Number`,不猜单位。`ticker` 的 `time` 是触发它的成交时间,写入 `ex_ts_ns`。

### 心跳与 BBO 探测

| 连接 | 服务端 | 我们 |
|---|---|---|
| BBO(`ticker`) | 不主动发 WS Ping(未观察到,待核实);`heartbeat` 每产品每秒一条 | hft-venue §3a:每 `feed.bbo_ping_ms`(1 000)一次 `bbo_probe = WsPing`(实测有 Pong);`heartbeat` → `ControlHint::Pong` 也产出 `ConnState{Alive}`;`feed.bbo_dead_ms`(3 000)无任何入向帧 → 退役 |
| Md(`matches`) | 同上 | `HeartbeatSpec{WsPing, interval_ms 5 000, unconditional false, pong_timeout_ms 5 000, idle_timeout_ms 0}`:5 s 无数据发 WS Ping,5 s 内 Pong 或任一 `heartbeat` 视为存活;不设空闲超时(安静的市场不是断线) |

### 限速默认值

官方 "Exchange WebSocket Rate Limits"(页面 2026-11-04):每 IP 每秒 8 个请求(突发 20),客户端每 IP 每秒 100 条消息,入向消息 10 RPS。`rate_rules()`:`Ip/WsInMsgs 1 000 ms 8`、`Conn/WsInMsgs 1 000 ms 10`、`Ip/NewConns 1 000 ms 8`(把"请求"同时按订阅帧与建连计,偏安全;建连是否计入待核实)。refdata `ws_max_in_msgs_per_s` = 8(订阅帧节奏 = 8/2 − 1 次探测 = 3 帧/s),`ws_max_streams_per_conn` = 64(官方未给单连接订阅数上限,待核实)。

### VenueCaps 默认值

| 字段 | 值 | 来源 |
|---|---|---|
| `market` | `Spot` | — |
| `flags` | 无(不下单) | — |
| `fee_asset_rule` | `Quote`(不用) | — |
| `reduce_only` | `false` | — |
| `required_spot_borrow` | `None`(无账户) | — |
| `own_liquidation_marker` | `NotIdentifiable` | — |
| `testnet` | `None`(交易所有 sandbox,本 crate 不用) | 官方 |
| `dms` | `None` | — |

`stale_guard()` = `None`(refdata `StaleGuardKind::None`),`dead_man()` = `None`。

### 账户

无。`build_templates`、`encode`、`private_subscribe`、`session_identity_request`、`account_request` → `Unsupported`;`decode_report`、`decode_identity`、`account_decode`、`funding_decode`、`decode_snapshot` → `UnknownClass`;`login` 对 `Md` 返回 `NotNeeded`,其余角色 `Unsupported`;`render_cid` 返回 0、`parse_cid` 返回 `None`。hft-core 对带 `accounts` 的本协议 venue 退出 2(`ReadOnlyVenue`)。

### 接入清单中不适用的步骤

[接入清单](../../docs/cookbook/adding-a-venue.md)按步骤:

| 步骤 | 状态 |
|---|---|
| 0 事实表 | 只做行情部分(本 README);下单、账户、资金与风险、用户流、测试网:**不适用**(只读) |
| 1 建 crate | 完成;`cid.rs`、`sign.rs`、`orders.rs`、`reports.rs`、`external.rs`、`acct.rs`、`codes.rs`、`mock.rs`:**不适用**(无下单与账户);`benches/`:未建(见[已知限制](#已知限制与后续工作)) |
| 2 `VenueProtocol` | 行情部分完成;第 4–8 小步(下单、回报差异、账户查询、过期与死手、逐单签名):**不适用**,方法返回 Unsupported 形式 |
| 3 `VenueCaps` | 完成(上表);hft-cfg `default_caps` 已登记 |
| 4 金样 | 行情样本完成(`fixtures/coinbase-spot/`,2026-11-06 实盘录制);下单、回报、外部成交、账户查询样本:**不适用** |
| 5 fuzz | `fuzz/fuzz_targets/coinbase_spot.rs`(目标名 `coinbase-spot`) |
| 6 假交易所与测试网 | 行情回放由金样与赛马回放覆盖(CBX-014);进程内夹具场景与 wire 级 mock:未做;下单往返、测试网:**不适用** |
| 7 refdata | `tools/hft-pmcfg` 写入 venue、端点、caps、限速与 BTC-USD inst;账户登记:**不适用** |
| 8 hft-core 装配 | 泛型实例化、端点白名单、协议登记完成;kill 字、死手、凭据、gw 线程:**不适用**(无账户) |
| 9 运维 | reconciler、kill-switch:**不适用**;watchtower 看板:未做 |
| 10 上线 | 冒烟与回放一致性待部署后做(不在本次范围) |

## 实现说明

<details><summary>实现要点</summary>

- 分派只看前 64 B 的 `{"type":"<name>"`;`peek` 先用 `hft_venue_proto::json::project` 取 `product_id` 与序号,失败再走 sonic-rs 结构体;`decode_md` 先按 2026-11-06 抓到的字段顺序走 `json::ordered`,顺序或字段有任何变化走 sonic-rs 结构体(同一错误分类)。无 `serde_json::Value`、无浮点、无分配。
- 解码失败不产出部分事件:事件在全部字段解析成功后才写入 sink。

| 文件 | 职责 |
|---|---|
| `src/lib.rs` | `CoinbaseSpot` 的 `VenueProtocol`、`AccountProtocol`、`FundingHistoryProtocol` |
| `src/md.rs` | 分派、`peek`、序号规则、控制提示、`ticker`/`match` 解码 |
| `src/session.rs` | 路由、端点路径、订阅帧 |
| `src/caps.rs` | `VenueCaps`、限速、心跳 |
| `src/time.rs` | ISO-8601 → ns |

</details>

## 延迟与资源契约

热入口 `classify`、`peek`、`decode_md`:无分配、无系统调用,工作量与报文长度成线性(`ticker` 约 450 B)。未建基准(见[已知限制](#已知限制与后续工作));预算沿用 [hft-venue-proto 统一门槛表](../hft-venue-proto/README.md#延迟与资源契约)。

## 测试与验收

用例与验收见 [TESTS.md](TESTS.md)。

## 依赖

`hft-types`、`hft-refdata`、`hft-event`、`hft-venue-proto`、`sonic-rs` 与 `serde`(`derive`);`serde_json` 只在 `[dev-dependencies]`。不依赖 `hft-venue`、`hft-net`、`hft-journal`、`hft-rt`、`tokio`。

## 延伸阅读

- [接入一个新交易所](../../docs/cookbook/adding-a-venue.md)
- [hft-venue §3 去重](../hft-venue/README.md#3-去重断裂与重同步)、[§3a BBO 连接健康](../hft-venue/README.md#3a-bbo-连接健康)
- 官方:Exchange WebSocket Overview / Channels / Rate Limits(docs.cdp.coinbase.com/exchange/websocket-feed)

## 已知限制与后续工作

- **盘口只随成交更新**:`ticker` 是成交触发的;两笔成交之间的挂单变化看不到。`level2_batch`(实测订阅名 `level2_50`,无需鉴权)给全簿快照与 50 ms 批量增量,但消息**没有序号**,无法双路去重与断裂检测;`level2`/`full` 需要鉴权(实测 `error`:"level2, level3, and full channels now require authentication")。`ticker_batch`(`ticker_1000`)每 5 s 一条,无用。都不接。
- `matches` 只有一条连接(hft-venue 的 `Md` 路由每组 1 条,`feed.depth_conns` 只作用于 `DepthTopN`);要双连接需要改 hft-venue(接入清单的改动边界之外),或改用 `ticker` 里的成交字段。
- 未建 `benches/`,未在 hft-core 进程内夹具加场景,未做 wire 级 mock。
- 待核实:服务端是否发 WS Ping 及无 Pong 断开时限;单连接订阅数上限;"每 IP 每秒 8 个请求"是否含建连;交易所是否强制断开长连接(refdata `ws_max_conn_age_s` 暂取 86 400)。核实方法:在 eu-west-1 盒子上用只读脚本长连 24 h 记录控制帧与断开原因。

### 开发备忘

<details><summary>给实现者</summary>

- 金样由 `fixtures/coinbase-spot/*/*.meta.toml` 记录来源;期望值由独立脚本(Python `Decimal`)从线上值算出,不由解码器生成。
- 赛马回放样本 `fixtures/coinbase-spot/race/ticker_two_conns.jsonl` 是两条同主机 `ticker` 连接按接收顺序的前 400 帧。

</details>
