---
description: "Polymarket CLOB 二元结果代币的纯协议实现(optional):market 频道的 book/price_change/last_trade_price 解码、按 inst 合成的连续序号、运行期 token 绑定表、文本 PING 心跳与 VenueCaps 默认值;下单与用户频道尚未实现"
kind: crate
tier: pure
scope: optional
status: impl
---

# venue-polymarket

## 概要

本 crate 实现 Polymarket CLOB 的 `VenueProtocol`(`polymarket-clob`,`VenueId::POLYMARKET_CLOB` = 15)。每个结果代币(Up/Down)按一个现货 inst 处理,计价资产为 USDC。它拥有 market 频道的解码、订阅帧、心跳与能力默认值,不拥有连接、线程与 I/O(hft-venue 运行时)。代币 id 是 77–78 位十进制数,超过 refdata 符号表的 27 字节上限,也随每个 5 分钟窗口变化,所以 refdata 只放固定的槽位 inst,槽位 ↔ 代币 id 的对应放在 feed 任务自己的绑定表(`FeedCtx`)里。触发条件:负责人决定交易 Polymarket 5 分钟涨跌盘([提案](../../.agents/notes/proposed/architecture/2026-10-19-polymarket-venue.md))。当前只实现行情;下单(EIP-712 签名的 REST 订单)、用户频道与账户查询见[已知限制与后续工作](#已知限制与后续工作)。

## 目录

- [使用本组件](#使用本组件)
  - [端点](#端点)
  - [槽位绑定](#槽位绑定)
  - [订阅](#订阅)
  - [行情频道 → 事件](#行情频道--事件)
  - [合成序号](#合成序号)
  - [心跳](#心跳)
  - [VenueCaps 默认值](#venuecaps-默认值)
- [实现说明](#实现说明)
- [延迟与资源契约](#延迟与资源契约)
- [测试与验收](#测试与验收)
- [依赖](#依赖)
- [延伸阅读](#延伸阅读)
- [已知限制与后续工作](#已知限制与后续工作)

## 使用本组件

```rust
pub struct PolymarketClob;                 // NAME = "polymarket-clob", MARKET = Spot, SUB_ACKS = false
pub enum MdClass { Push, Pong, Unknown }
pub struct FeedCtx;                        // 绑定表 + 每 inst 的合成序号
impl FeedCtx {
    pub fn with_capacity(insts: usize) -> FeedCtx;
    pub fn bind(&mut self, inst: InstId, token: &[u8]) -> Result<(), BindErr>;
    pub fn unbind(&mut self, inst: InstId);
    pub fn inst_of(&self, token: &[u8]) -> InstId;      // 0 = 未绑定
    pub fn token_of(&self, inst: InstId) -> Option<&[u8]>;
}
pub enum BindErr { Token, Inst, TokenInUse, Full }
pub const TOKEN_MAX: usize = 80;
```

### 端点

| 用途 | 地址 |
|---|---|
| market 频道 | `wss://ws-subscriptions-clob.polymarket.com/ws/market`(refdata `MdPublic`/`MdMarket` 端点给主机,`endpoint()` 给路径 `/ws/market`) |

只有一条路由 `market`(`RouteId(0)`),声明为 `DepthTopN`、档位 `[100]`:价格网格每边最多 99 档,"前 100 档" 就是全簿。同一连接还推送成交,所以 `Trade` 没有独立路由(`stream_route(Trade) = None`),订阅了成交的 inst 在这条连接上直接收到成交。

### 槽位绑定

refdata 为每个资产预置固定数量的槽位 inst(符号如 `btc-up-0`)。槽位 ↔ 代币 id 的对应由 `FeedCtx::bind` 写入,规则:

- 代币 id 只能是 1–80 位 ASCII 数字(`BindErr::Token`);inst 0 拒绝(`Inst`)。
- 一个代币同一时刻只绑定一个 inst(`TokenInUse`);容量用尽拒绝(`Full`)。
- 重新绑定一个 inst 换掉它的代币,保留它的合成序号计数;`unbind` 只清代币。
- 查找按代币文本的 FNV-1a 开放寻址,不分配。

绑定由谁、在什么时候写入不归本 crate:当前由装配方在启动时按配置写入;运行期绑定命令见[已知限制与后续工作](#已知限制与后续工作)。

### 订阅

`subscribe` 永远返回 `ProtoErr::Cfg`(代币不在 refdata 里);运行时调用 `subscribe_ctx`,从绑定表取代币:

```text
{"type":"market","operation":"subscribe","assets_ids":["<token>",…]}      // 退订: "operation":"unsubscribe"
```

- 一帧最多 100 个代币;同一 inst 的簿与成交流只写一次代币。
- 接受 `DepthTopN`、`DepthIncr`、`Trade`;其他流 → `ProtoErr::Unsupported`;任一 inst 未绑定 → `ProtoErr::Cfg`,不写任何帧。
- 交易所不回复订阅帧(2026-10-19 实测),`SUB_ACKS = false`:运行时发出即视为确认。同一连接上可以随时追加订阅或退订(实测有效)。

### 行情频道 → 事件

消息是 JSON 对象,或对象数组(连上后的首批 `book`)。`event_type` 在对象末尾,前 64 字节看不到,所以 `classify` 只分 `Push`(以 `{`/`[` 开头)、`Pong`(文本 `PONG`)与 `Unknown`,类型由 `peek`/`decode_md` 读出。

| `event_type` | 事件 | 说明 |
|---|---|---|
| `book` | `L2Snapshot`(`origin = EXCHANGE`) | `bids`/`asks` 为 `{"price","size"}` 数组,数量为 0 的档丢弃;整簿替换 |
| `price_change` | 每个已绑定代币一个 `L2Batch` | 每条为一档的新总量(`"0"` = 删档),`BUY` → 买边,`SELL` → 卖边;一条消息通常同时含 Up/Down 两个代币 |
| `last_trade_price` | `Trade` | `side` 为吃单方向;`trade_id` 为合成序号 |
| `tick_size_change` 及其他 | 无 | `peek` → `UnknownClass`(运行时计数并记原始帧) |

- 时间:`timestamp`(毫秒字符串)→ `MarketHdr.ex_ts_ns`;交易所不给发送时间,`ex_send_ns = 0`。
- 价格、数量按 inst 的 `px_wire_exp`/`qty_wire_exp` 精确解析,多余小数位 → `DecodeErr::Number`。一条消息先校验全部条目再写事件:任一条出错时不写任何事件。
- 未绑定的代币静默跳过(不是错误)。`peek` 返回首个代币的 inst,`stream = DepthTopN`(簿)或 `Trade`。
- `seq_rule = None`,`resync_method(Push) = Resubscribe`:重订阅后交易所先推整簿。

### 合成序号

交易所的簿流没有序号,而 hft-book 要求 `L2Batch.prev_id` 等于上一条的 `last_id`。`FeedCtx` 为每个 inst 维护单调计数:`book` 取下一个值作 `snap_id`;`price_change` 给出 `(prev_id, last_id) = (当前值, 当前值 + 1)`;成交另有一个计数作 `trade_id`。计数跨重连、跨重新绑定只增不减,所以新窗口的整簿不会和旧 id 冲突。

### 心跳

`heartbeat`:应用层文本 `PING`,每 10 s 无条件发送,`PONG` 超时 10 s,空闲 30 s 判断开(服务端会断开沉默的连接)。

### VenueCaps 默认值

| 字段 | 值 |
|---|---|
| `market` | `Spot` |
| `flags` | `POST_ONLY`、`IOC`、`FOK`(下单实现前不使用) |
| `fee_asset_rule` | `Quote` |
| `required_spot_borrow` | `Some(false)` |
| `own_liquidation_marker` | `NotIdentifiable` |
| `testnet` | `None`(交易所没有测试网) |
| `dms` | `None` |

限速默认值只有连接与入站消息两条(`Ip`/`NewConns` 300 次每 5 分钟,`Conn`/`WsInMsgs` 每秒 10 条);下单限速随下单实现补齐。

## 实现说明

<details><summary>实现要点</summary>

| 文件 | 职责 |
|---|---|
| `src/lib.rs` | `VenueProtocol`、`AccountProtocol`、`FundingHistoryProtocol` 实现;下单、账户与资金费入口返回 `Unsupported`/`UnknownClass` |
| `src/bind.rs` | 绑定表与合成序号(固定容量,构造时分配一次) |
| `src/md.rs` | 分类、`peek`、三种消息的解码 |
| `src/session.rs` | 路由、端点、订阅帧、心跳 |
| `src/caps.rs` | `VenueCaps`、限速、过期与死手默认值 |

- 解析:消息头字段用 sonic-rs 借用字符串;`bids`/`asks`/`price_changes` 保持原始文本(`LazyValue`),逐个对象投影(`hft_venue_proto::json::project`,失败回落到校验解码),不在栈上为每档预留空间。
- 列表帧按顶层对象切分(跟踪字符串与嵌套),每帧最多 16 个对象;`price_changes` 每条消息最多 512 条、8 个不同代币,超出 → `Shape`。
- 不缓存 refdata(契约 25):每条消息按 inst 读一次精度。

</details>

## 延迟与资源契约

无基准门槛(optional,尚未进入上线门槛)。解码不分配;`book` 解码用两段 1000 档的 `Level` 栈数组(各 16 KiB)。2026-10-19 录制的 400 帧样本全部解码通过,重建簿的最优价与交易所给出的 `best_bid`/`best_ask` 一致([PMK-011](TESTS.md#pmk-011-实采窗口重建的最优价与交易所一致))。

## 测试与验收

用例与验收见 [TESTS.md](TESTS.md)。

## 依赖

允许:`hft-types`、`hft-refdata`、`hft-event`、`hft-venue-proto`;外部 `sonic-rs` 0.5、`serde`(与其他 venue crate 同版本)。dev:`hft-venue-proto`(`testkit`)、`serde_json`(仅金样比对)。禁止依赖运行时、I/O 与任何 `strategies/*`。

## 延伸阅读

- [接入提案](../../.agents/notes/proposed/architecture/2026-10-19-polymarket-venue.md)
- [hft-venue-proto 契约](../hft-venue-proto/README.md)
- [行情接入验收探针](../../tools/pm-md-probe/README.md)

## 已知限制与后续工作

- 下单:EIP-712(secp256k1)逐单签名的 REST `POST /order`、撤单、GTD 到期时间、用户频道回报、`FeeModel` 非线性手续费,按提案 P2 实现。
- 运行期绑定:当前绑定只在装配时写入;提案 P1b 的绑定命令(先写 journal 再生效)与市场发现进程尚未实现。
- `tick_size_change` 不解码:tick 变化(0.01 → 0.001)需要按新精度重建 inst,随 P1b 处理。

### 开发备忘

<details><summary>给实现者</summary>

已实测(2026-10-19,eu-west-1):订阅帧无应答;同一连接可追加订阅与退订;`PING` → `PONG`;每次成交后推一次整簿。下单实现前需核实:签名域与订单结构以官方 SDK 生成的金样为准;下单与撤单限速;用户频道消息全集。

</details>
