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
  - [多连接去重](#多连接去重)
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

协议默认每条连接 2 个流(`max_streams = 2`,即一个市场的 Up/Down 两个代币;实际值取 refdata `ws_max_streams_per_conn`):服务端在某条连接的发送缓冲写满时以 WS 1013 关闭它(2026-10-19 实测,16 个代币挤在一条连接上 13 分钟断开 3 次,每条连接只放一个市场的录制器约每小时 5 次,只发生在最活跃的 BTC),每次重连后丢失约 0.2–0.3 s。

### 槽位绑定

refdata 为每个资产预置固定数量的槽位 inst(符号如 `btc-up-0`)。槽位 ↔ 代币 id 的对应由 `FeedCtx::bind` 写入,规则:

- 代币 id 只能是 1–80 位 ASCII 数字(`BindErr::Token`);inst 0 拒绝(`Inst`)。
- 一个代币同一时刻只绑定一个 inst(`TokenInUse`);容量用尽拒绝(`Full`)。
- 重新绑定一个 inst 换掉它的代币,保留它的合成序号计数;`unbind` 只清代币。
- 查找按代币文本的 FNV-1a 开放寻址,不分配。

绑定由 [`BindSlot`](../../docs/subsystems/control-plane.md#命令集) 命令写入:sequencer 记录并把 32 字节交易所 id 经 `FeedCmd::BindWire` 交给 feed 任务,feed 任务调用 `bind_wire`(uint256 大端 → 十进制代币文本)。改绑时旧代币保留到下一个退订帧(`unsub_token_of`),订阅帧跳过尚未绑定的 inst。

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
| 空数组 `[]` | 无 | 部分订阅帧之后服务端推送(2026-10-19 实测);`peek` → `UnknownClass`,不算解码错误 |

- 时间:`timestamp`(毫秒字符串)→ `MarketHdr.ex_ts_ns`;交易所不给发送时间,`ex_send_ns = 0`。
- 价格、数量按 inst 的 `px_wire_exp`/`qty_wire_exp` 精确解析,多余小数位 → `DecodeErr::Number`。一条消息先校验全部条目再写事件:任一条出错时不写任何事件。
- 过期增量:(重)订阅后交易所先推当前整簿,之后仍可能送达时间戳早于该整簿的 `price_change`(2026-10-19 实测重连后出现),它们已包含在整簿里,再应用会把簿退回旧状态。`FeedCtx` 记录每个 inst 最近一次 `book` 的交易所时间,时间戳早于它的 `price_change` 条目丢弃(同一毫秒的照常应用);改绑新代币时清零。
- 未绑定的代币静默跳过(不是错误)。`peek` 返回首个**已绑定**代币的 inst(`price_change` 的各条变化按出现顺序取前 4 个不同代币,列表帧看前 8 个元素;都未绑定时返回首个代币,即 inst 0),`stream = DepthTopN`(簿)或 `Trade`。运行时按 `peek` 的 inst 决定整帧是否交付:一个市场的两个代币分别绑定时,首个代币尚未绑定的 `price_change` 不能因此丢掉另一个代币的变化(2026-10-20 hft-core 验收中出现)。
- `seq_rule = None`,`resync_method(Push) = Resubscribe`:重订阅后交易所先推整簿。

### 合成序号

交易所的簿流没有序号,而 hft-book 要求 `L2Batch.prev_id` 等于上一条的 `last_id`。`FeedCtx` 为每个 inst 维护单调计数:`book` 取下一个值作 `snap_id`;`price_change` 给出 `(prev_id, last_id) = (当前值, 当前值 + 1)`;成交另有一个计数作 `trade_id`。计数跨重连、跨重新绑定只增不减,所以新窗口的整簿不会和旧 id 冲突。

### 多连接去重

交易所以 WS 1013(`slow consumer: send buffer full`)单独断开某条连接:断开前约 0.3–0.4 s 服务端先停止推送(接收端延迟从约 10 ms 升到 300–400 ms,接收端并无积压),随后关闭;BTC 在活跃时段每小时数次,其余资产少见,有时同一市场的两条连接在同一秒被断开(2026-10-20 实测,eu-west-1)。重连约 1–3 s,其间该连接上的增量丢失。因此 hft-core 为本协议的深度组建两条同订阅的连接(hft-venue `feed.depth_conns`),同一条消息会从两条连接各到一次,由 `FeedCtx` 在解码时去重(同一 feed 任务的全部连接共用):

- **键**:`book` = (代币, 时间戳, `hash`);`price_change` = (时间戳, 各条变化的代币、价格、数量、方向与 `hash`);`last_trade_price` = (代币, 时间戳, `transaction_hash`, 价格, 数量, 方向)。`price_change` 条目的 `hash` 不是每条变化唯一:同一连接上约 3.5% 的消息与另一条内容不同的消息有相同的时间戳与 `hash`(2026-10-20 实测 3 个窗口),所以键必须包含全部字段。
- **次数**:表里记每条连接送达每个键的次数。某连接第 n 次送达某键,只有其他连接都还没送达到 n 次时才算新消息;否则整条消息丢弃、不产出事件(`decode_md` 返回 `Ok(events 0)`),计入 `FeedCtx::dups()`。所以同一连接上两笔完全相同的成交仍是两条,同一笔成交从两条连接各到一次只算一条。
- **表**:两张各 32 768 槽的开放寻址表(当前与上一代),当前表写满 16 384 个键后换代,清空最旧的一张;一个键至少在其后 16 384 个键内被记住(按 2026-10-20 的消息率,所有资产合计约 5 s 以上)。换代清一张 512 KiB 的表,约每数秒一次。
- **迟到**:落后的连接送来、而表已不再记得的消息不能再应用,否则簿会退回旧状态。每个 inst 记已应用的最新交易所时间:早于它的 `book` 丢弃(连接重订阅后推送的整簿,若另一条连接已送达更新的数据,只会把簿退回去);早于它超过 2 000 ms 的 `price_change` 条目与成交丢弃;两者都计入 `FeedCtx::lates()`。改绑新代币时清零。
- 单连接时没有重复。迟到只见于整簿:实测单连接每个 BTC 窗口有 0–14 条 `book` 比已应用的增量早(至多 62 ms),应用它会把这些增量退掉,所以丢弃;增量与成交在单连接上没有早于已应用时间 2 s 以上的(2026-10-19/20 的 4 个窗口,约 120 万条变化)。

### 心跳

`heartbeat`:应用层文本 `PING`,每 10 s 无条件发送,`PONG` 超时 10 s;不设空闲超时(`idle_timeout_ms = 0`):一个连接只放一个市场时,未开盘或已收盘的市场可以几分钟没有数据,安静不等于断开,存活以 `PONG` 为准(2026-10-19 实测 30 s 空闲超时在 13 分钟内误断 35 次)。

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

无基准门槛(optional,尚未进入上线门槛)。解码不分配;去重表在 `FeedCtx::with_capacity` 时分配两张 512 KiB 的表,换代时清一张;`book` 解码用两段 1000 档的 `Level` 栈数组(各 16 KiB)。2026-10-19 录制的 400 帧样本全部解码通过,重建簿的最优价与交易所给出的 `best_bid`/`best_ask` 一致([PMK-011](TESTS.md#pmk-011-实采窗口重建的最优价与交易所一致))。

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
- `tick_size_change` 不解码:每个 5 分钟市场在价格接近 0 或 1 时(约开盘 4 分钟后)把 tick 从 0.01 改为 0.001,之后出现三位小数价格。解码按 px_exp −3 照常解析;hft-core 的 Polymarket 槽位 inst 以 tick 0.001 建 refdata(`pm_sim_config`),所以三位小数的档位照常应用。
- 服务端 1013 断开见[多连接去重](#多连接去重)。两条连接在同一秒都被断开时仍有约 1–2 s 的缺口(该组连接都断开后推 `Resync{CONN_LOST}`,重连后的整簿恢复)。另外,市场结算后约 147 s 交易所以 1000 关闭订阅了该市场代币的连接;hft-net 把它与 1013 记为同一个 `WS_CLOSE` 位,早先"探针每 13 分钟断 3–4 次"的统计把这类正常关闭也算了进去(2026-10-20 重新比对:同机同期同 6 个 BTC 市场,探针异常断开 1 次,Python 录制器 3 次)。

### 开发备忘

<details><summary>给实现者</summary>

已实测(2026-10-19,eu-west-1):订阅帧无应答;同一连接可追加订阅与退订;`PING` → `PONG`;每次成交后推一次整簿。下单实现前需核实:签名域与订单结构以官方 SDK 生成的金样为准;下单与撤单限速;用户频道消息全集。

</details>
