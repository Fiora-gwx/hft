---
description: "Polymarket 实时价格流(PolyBolt)的只读行情协议 polymarket-price:Chainlink 价格 → IndexPrice、60 s TWAP → MarkPrice,行情连接用 Polymarket API 凭据登录,订阅后的历史帧补齐断线期间的秒;无下单、无账户"
kind: crate
tier: pure
scope: optional
status: impl
---

# venue-pmprice

## 概要

`venue-pmprice` 实现 [`VenueProtocol`](../hft-venue-proto/README.md#venueprotocol静态分派) `PolymarketPrice`(`NAME = "polymarket-price"`,`VenueId::POLYMARKET_PRICE` = 17,`Exchange::Polymarket`,`MarketKind::Spot`):Polymarket 实时数据服务(内部名 PolyBolt)的价格频道。Polymarket 的 BTC 5 分钟盘按 Chainlink BTC/USD 的 60 s TWAP 结算(窗口末 TWAP ≥ 窗口初 TWAP → Up),本协议把 Chainlink 价格样本与 TWAP 样本原生地送进引擎,取代实盘机上以 `InjectWarmup` 推样本的 Python 旁路([提案](../../.agents/notes/proposed/architecture/2026-11-09-pm-md-perp-chainlink-model.md))。只读:没有下单、回报与账户;唯一的会话动作是行情连接的登录。纯函数:不碰 socket、不创建线程、不写 journal。

## 目录

- [使用本组件](#使用本组件)
  - [端点与路由](#端点与路由)
  - [登录](#登录)
  - [订阅](#订阅)
  - [行情频道 → 事件](#行情频道--事件)
  - [去重](#去重)
  - [价格精度](#价格精度)
  - [心跳](#心跳)
  - [限速默认值](#限速默认值)
  - [VenueCaps 默认值](#venuecaps-默认值)
  - [账户](#账户)
- [实现说明](#实现说明)
- [延迟与资源契约](#延迟与资源契约)
- [测试与验收](#测试与验收)
- [依赖](#依赖)
- [延伸阅读](#延伸阅读)
- [已知限制与后续工作](#已知限制与后续工作)

## 使用本组件

```rust
pub struct PolymarketPrice;   // impl VenueProtocol —— NAME = "polymarket-price", VENUE = POLYMARKET_PRICE, MARKET = Spot
pub enum MdClass { Price, Twap, Subscribed, Error, Pong, Authed, Unknown }
pub struct FeedCtx { /* 每个 (inst, 流) 已转发的最新样本时间 */ }
impl FeedCtx { pub fn advance(&mut self, inst: InstId, stream: StreamKind, ts_ms: i64) -> bool; }
pub const ROUTE_PRICE: RouteId;          // 唯一路由
pub const HISTORY_MAX: usize = 1024;     // 一条历史帧至多的样本数
pub const FEED_INSTS: usize = 8;         // 有自己水位的 inst 数
pub fn api_secret(cred: &Credential) -> &[u8];          // secret 中第一个 `|` 之前
pub fn round_dec(s: &[u8], exp: i8) -> Result<i64, DecodeErr>;   // 十进制按 10^exp 四舍五入
```

refdata:inst 的符号是小写 `<asset>usd`(如 `btcusd`),资产为该币、计价为 USD;价格按该资产的 `px_exp`(BTC 为 −4)。

### 端点与路由

| 用途 | 端点 | 说明 |
|---|---|---|
| 价格 | `wss://ws-live-v2.polymarket.com/ws` | 路由 `price`(`ROUTE_PRICE`,`RouteKind::Md`,不赛马,至多 16 个订阅);`stream_route`:`Index`、`Mark` → `price`,其余 `None`;`endpoint(Md, price)` 的路径为 `/ws`,其他角色或路由 → `Unsupported` |

旧的 `wss://ws-live-data.polymarket.com`(RTDS)现在只在连接时推一次最近 60 s,不再推实时样本(公司收数机 2026-11 的观察),不用。

### 登录

未认证的订阅被拒(`{"op":"error","code":"auth_required"}`,2026-11-09 实测)。连接建立后、订阅之前发一帧(`login(Md, …, step 0)` → `LoginStep::Frame`):

```json
{"op":"auth","rid":"a1","auth":{"apiKey":"<key>","secret":"<api secret>","passphrase":"<passphrase>"}}
```

- 凭据是 Polymarket CLOB 的 L2 API 凭据(任一账户)。`secret` 只取 `Credential.secret` 中第一个 `|` 之前的部分(`api_secret`):Polymarket 账户的 secret 文件是 `<api secret>|<钱包私钥>|<存款钱包>`,后两段绝不进帧。三者都必须非空、只含可打印 ASCII 且不含 `"` 与 `\`(帧不转义),否则 `ProtoErr::Cfg`;`step ≠ 0` → `Cfg`;非 `Md` 角色 → `Unsupported`。
- `login_result`:`{"op":"authed"…}` → `Ok`;`{"op":"error"…}` → `Rejected(1)`;其余 `NotMine`。
- 登录帧由会话驱动直接写出,不进 journal(hft-venue 行情凭据,[hft-venue](../hft-venue/README.md))。

### 订阅

每个订阅规格一帧(服务器对一帧里的每个订阅分别应答;一帧一个使应答与帧一一对应,`SubFrames` 记 `subs = 1`):

| 流 | 帧 |
|---|---|
| `Index` | `{"op":"subscribe","rid":"s","subscriptions":[{"channel":"price.crypto","filter":{"symbol":"<sym>"}}]}` |
| `Mark` | `{"op":"subscribe","rid":"s","subscriptions":[{"channel":"price.crypto.twap","filter":{"symbol":"<sym>","window_seconds":60}}]}` |

退订把 `"op"` 换成 `"unsubscribe"`、`rid` 为 `"u"`(服务器是否支持见[开发备忘](#开发备忘);引擎只在停机时退订)。inst 不属于本 venue 或符号不是 `[a-z0-9]+` → `Cfg`;其他流 → `Unsupported`。应答:`{"op":"subscribed",…}` → `ControlHint::SubAck{ok: true}`;`{"op":"error",…}`(如 `bad_channel`)→ `SubAck{ok: false, code: 1}`。

### 行情频道 → 事件

| 帧 | 分类 | 事件 | 时间戳 |
|---|---|---|---|
| `{"v":1,"channel":"price.crypto",…,"payload":{"full_accuracy_value":"…","symbol":…,"timestamp":…}}` | `Price` | 一条 `IndexPrice{px}` | `ex_ts_ns ← payload.timestamp`(样本秒,ms)、`ex_send_ns ← ts` |
| 同上,`"channel":"price.crypto.twap"`(`window_seconds` 60) | `Twap` | 一条 `MarkPrice{px}`(结算标记价 = 60 s TWAP) | 同上 |
| 订阅后的历史帧(`"snapshot":true`,`payload.data` = 最近约 120 个逐秒样本,`payload.symbol` 在数组之后) | `Price`/`Twap` | 每个未转发过的样本一条事件,按数组顺序 | 每个样本自己的 `timestamp`;`ex_send_ns ← ts` |

`px` 取 `full_accuracy_value`(字符串),不取浮点 `value`。接收元数据(`conn`、`rx_tsc`、`sw_rx_ns`、`hw_rx_ns`、`rx_empty_gap_ns`)原样复制(契约 3)。refdata 没有该符号 → 校验形状后不产出事件(`events = 0`)。分类只看前 64 B:`{"op":"<name>"` → 按名;以 `{"v":` 开头且含 `"channel":"price.crypto.twap"` → `Twap`、含 `"channel":"price.crypto"` → `Price`;二进制帧与其他 → `Unknown`。

### 去重

`seq_rule`:`Price`、`Twap` → `Monotonic`,`peek` 的 `last_id` = 帧中最新的样本时间(ms;历史帧取数组最大值)。`FeedCtx` 另按 (inst, 流) 记已转发的最新样本时间:历史帧只转发比它新的样本(断线重连后补上断线期间的秒,不重发已转发的),价格与 TWAP 各自独立。历史帧先逐项校验(数值、至多 `HISTORY_MAX` 项),任一项不合法则整帧拒绝、不产出事件。服务器的 `seq` 按连接计数,不用。

### 价格精度

`full_accuracy_value` 有至多 18 位小数;按 inst 的 `px_wire_exp` 四舍五入(`round_dec`,半数进位)。只接受 `[0-9]+(\.[0-9]+)?`;负号、指数记法、空小数 → `DecodeErr::Number`;超出 i64 → `Overflow`;价格 ≤ 0 或样本时间 ≤ 0 → `Shape`。

### 心跳

应用层 `{"op":"ping","rid":"p"}`(`PingKind::AppJson`)每 5 s 无条件发出,`{"op":"pong",…}` → `ControlHint::Pong`,5 s 内未答即断;样本每秒一条,10 s 没有任何帧视为死连接(`idle_timeout_ms` 10 000)。

### 限速默认值

官方未公布。每连接入站 10 条/s、每 IP 新建连接 1 条/s(只防重连风暴)。

### VenueCaps 默认值

`VenueCaps::none` 加 `market = Spot`、`fee_asset_rule = Quote`、`reduce_only = false`、`own_liquidation_marker = NotIdentifiable`、`testnet = None`、`dms = None`;`stale_guard` 与 `dead_man` 为 `None`。

### 账户

没有账户:hft-core 拒绝本 venue 的 `accounts`(`READ_ONLY_PROTOCOLS`,`ReadOnlyVenue`)。凭据只经 hft-core `[venues.<v>.md_login]` 进入行情连接的登录([hft-core](../../apps/hft-core/README.md))。所有下单、回报、账户、身份方法返回 `Unsupported` / `UnknownClass`。

## 实现说明

<details><summary>实现要点</summary>

- `src/session.rs`:路由、端点、登录帧与订阅帧(定长缓冲,不分配)、心跳;`src/md.rs`:分类、`peek`、控制应答、`round_dec`、解码(sonic-rs 借用解析,历史帧数组以不分配的元素迭代器遍历);`src/caps.rs`:默认 caps 与限速;`src/lib.rs`:trait 实现与 `FeedCtx`。
- 金样 `fixtures/polymarket-price/capture/`:2026-11-09 实盘机 40 s 抓包(不含登录帧),期望由 Python `Decimal` 独立计算。

</details>

## 延迟与资源契约

每秒每符号两条消息,解码不在任何决策路径的时间预算内;未建基准。

## 测试与验收

用例与验收见 [TESTS.md](TESTS.md)。

## 依赖

允许:`hft-types`、`hft-refdata`、`hft-event`、`hft-venue-proto`;外部 `sonic-rs`、`serde`。`[dev-dependencies]`:`hft-venue-proto`(`testkit`)、`serde_json`(金样比较)。

## 延伸阅读

- [提案:Polymarket 行情层升级](../../.agents/notes/proposed/architecture/2026-11-09-pm-md-perp-chainlink-model.md)
- [venue-polymarket](../venue-polymarket/README.md)(同一账户的 CLOB 凭据)

## 已知限制与后续工作

- 只有一条连接;断线期间的样本靠重连后的历史帧补(约 120 s 以内)。
- 退订帧未在服务器上核实。
- 没有 fuzz 目标(输入面小;后续按 venue-coinbase 的形式补)。

### 开发备忘

- 帧格式、认证与历史帧来自 2026-11-09 实盘机实测(`fixtures/polymarket-price/capture/`)与公司收数机 `pmrec/record.py`;服务器没有公开文档。退订帧是否被接受未核实(方法:连一条认证连接,订阅后发 `unsubscribe`,看是否仍收到样本)。
