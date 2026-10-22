---
description: "Polymarket CLOB 二元结果代币的纯协议实现(optional):market 频道解码与合成序号、运行期 token 绑定表、文本 PING 心跳、VenueCaps 默认值;REST 下单(存款钱包签名、post-only、GTD)、撤单与查单,用户频道回报,账户启动核验"
kind: crate
tier: pure
scope: optional
status: impl
---

# venue-polymarket

## 概要

本 crate 实现 Polymarket CLOB 的 `VenueProtocol`(`polymarket-clob`,`VenueId::POLYMARKET_CLOB` = 15)。每个结果代币(Up/Down)按一个现货 inst 处理,计价资产为 USDC。它拥有 market 频道的解码、订阅帧、心跳与能力默认值,不拥有连接、线程与 I/O(hft-venue 运行时)。代币 id 是 77–78 位十进制数,超过 refdata 符号表的 27 字节上限,也随每个 5 分钟窗口变化,所以 refdata 只放固定的槽位 inst,槽位 ↔ 代币 id 的对应放在 feed 任务自己的绑定表(`FeedCtx`)里。触发条件:负责人决定交易 Polymarket 5 分钟涨跌盘([提案](../../.agents/notes/proposed/architecture/2026-10-19-polymarket-venue.md))。下单走 REST(每单 EIP-712 存款钱包签名),回报走带鉴权的用户频道;订单没有 client id,交易所订单哈希经 REST 回执绑定到槽位([下单](#下单))。尚未实现的部分见[已知限制与后续工作](#已知限制与后续工作)。

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
  - [签名](#签名)
  - [凭证](#凭证)
  - [下单](#下单)
  - [用户流事件表](#用户流事件表)
  - [累计量与费用来源](#累计量与费用来源)
  - [结果未知码表](#结果未知码表)
  - [账户配置核验](#账户配置核验)
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
pub enum ReportClass { Push, Ignorable, Unknown }
pub struct Templates { pub account: AccountId, pub eoa: [u8; 20], pub wallet: [u8; 20], pub domain: [u8; 32], pub gtd_s: u32 }
pub const GTD_S: u32 = 300;
pub struct GwCtx;                          // 每账户:token 绑定、本进程订单的哈希与累计成交
impl GwCtx {
    pub fn token_of(&self, inst: InstId) -> Option<&[u8]>;
    pub fn order_hash(&self, exch_order_id: u64) -> Option<[u8; 32]>;
}
```

### 端点

| 用途 | 地址 |
|---|---|
| market 频道 | `wss://ws-subscriptions-clob.polymarket.com/ws/market`(refdata `MdPublic`/`MdMarket` 端点给主机,`endpoint()` 给路径 `/ws/market`) |
| 用户频道(下单连接) | 同一主机,路径 `/ws/user`(refdata `OrderWs` 端点给主机,`endpoint(Order)` 给路径) |
| REST(下单、撤单、查单、账户) | `https://clob.polymarket.com`(refdata `Rest`) |

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
| `flags` | `POST_ONLY`、`IOC`、`FOK`(交易所支持;本实现只编码 `PostOnly` 与 `Gtc`,IOC/FOK → `Unsupported`) |
| `fee_asset_rule` | `Quote` |
| `required_spot_borrow` | `Some(false)` |
| `own_liquidation_marker` | `NotIdentifiable` |
| `testnet` | `None`(交易所没有测试网) |
| `dms` | `None` |

限速默认值:`Ip`/`NewConns` 300 次每 5 分钟,`Conn`/`WsInMsgs` 每秒 10 条,`Account`/`Orders` 每秒 20 单(交易所按 key 每秒 40 单、突发 60,撤单 80、突发 120,2026-10-22 官方文档;默认取下单的一半)。死手 `None`:交易所的订单心跳(`POST /v1/heartbeats`,链式 `heartbeat_id`,10 s 无心跳撤全部挂单)尚未接入,见[已知限制](#已知限制与后续工作)。

### 签名

CLOB V2(2026-04-28 起生产只接受 V2)的签名,`src/sign.rs`,纯函数、不分配。下单网关(提案 P2)用它们,本节只规定算法。

- **钱包**:2026-05-04 以后交易所只接受存款钱包(Deposit Wallet,签名类型 3)做 maker,EOA 直接做 maker 被拒(`maker address not allowed, please use the deposit wallet flow`,2026-10-22 实测)。存款钱包是由 EOA 控制的智能合约钱包:`maker = signer = 存款钱包地址`,EOA 私钥签名(见下方"存款钱包签名")。签名类型 0(`maker = signer = EOA`)保留给测试。私钥 32 字节十六进制(可带 `0x`);地址 = keccak-256(未压缩公钥去掉首字节)的后 20 字节,对外写成 EIP-55 大小写混合形式。
- **订单签名**:EIP-712。域 `EIP712Domain(string name,string version,uint256 chainId,address verifyingContract)`,`name = "Polymarket CTF Exchange"`、`version = "2"`、`chainId = 137`、`verifyingContract` = CTF Exchange V2 `0xE111180000d2663C0091e4f400237545B87B996B`(本 venue 的 5 分钟盘不是 neg-risk)。类型 `Order(uint256 salt,address maker,address signer,uint256 tokenId,uint256 makerAmount,uint256 takerAmount,uint8 side,uint8 signatureType,uint256 timestamp,bytes32 metadata,bytes32 builder)`;`side` 0 = 买、1 = 卖;金额以 1e-6 为单位(买单 `makerAmount` = 价格 × 份数的 pUSD,`takerAmount` = 份数);`timestamp` 为下单时刻(毫秒),V2 用它代替 nonce 保证唯一;`metadata`、`builder` 为 0。摘要 = keccak(`0x19 0x01` ‖ 域分隔符 ‖ 结构哈希)。`expiration`(GTD)不在签名结构里,只在请求体。
- **存款钱包签名**(类型 3,ERC-1271 经 ERC-7739 嵌套类型数据):内层结构 `TypedDataSign(Order contents,string name,string version,uint256 chainId,address verifyingContract,bytes32 salt)` 后接 `Order` 类型串;`contents` = 订单结构哈希,`name = "DepositWallet"`、`version = "1"`、`verifyingContract` = 订单的 `signer`(存款钱包)、`salt` = 0;EOA 对 keccak(`0x19 0x01` ‖ 交易所域分隔符 ‖ 该结构哈希)签名。发出的签名 = `r‖s‖v` ‖ 交易所域分隔符 ‖ 订单结构哈希 ‖ `Order` 类型串 ‖ 类型串长度(u16 大端),共 65 + 32 + 32 + 类型串长 + 2 字节。
- **L1 认证**(派生 API 凭证):域 `EIP712Domain(string name,string version,uint256 chainId)`,`name = "ClobAuthDomain"`、`version = "1"`;类型 `ClobAuth(address address,string timestamp,uint256 nonce,string message)`,`timestamp` 为十进制秒的字符串,`message` 固定为 `This message attests that I control the given wallet`。
- **ECDSA**:secp256k1,RFC 6979 确定性随机数,低 s;输出 `r ‖ s ‖ v`(65 字节,v = 27 + 恢复位),写成 `0x` 加小写十六进制。
- **L2 请求签名**:HMAC-SHA256,密钥为 API secret 的 URL-safe base64 解码,消息为 `timestamp ‖ method ‖ path ‖ body`(秒级时间戳文本、大写方法、带查询串的路径、原样请求体;无请求体时为空),结果为带填充的 URL-safe base64。

三者与官方 `py-clob-client-v2` 在固定测试私钥、salt 与时间戳下的输出逐字节一致(金样 `tests/sign_v2_golden.json`,测试私钥;签名类字段不放 `fixtures/`,`cargo xtask fixtures check` 会拦)。

### 凭证

一个账户一把 CLOB API key(L2:`api_key`、`passphrase`),`Credential.secret` 是三段 `|` 分隔的文本:`<API secret>|<EOA 私钥(0x + 64 位十六进制)>|<存款钱包地址(0x + 40 位十六进制)>`。`alg` 为 `Secp256k1`(也接受 `HmacSha256`)。EOA 由私钥算出,是 `POLY_ADDRESS` 与订单签名人;存款钱包是 `maker = signer`。`build_templates` 只保存两个地址与交易所域分隔符(契约 7),私钥按请求借用。三段缺一、私钥或地址格式不对 → `ProtoErr::Cfg`。

### 下单

全部订单类 op 走 REST(`order_channel = Rest`),由 `encode_rest_ctx` 编码(它读 `GwCtx` 里运行期绑定的 token id 与订单哈希;不带 `GwCtx` 的 `encode_rest_signed` 一律 `Unsupported`)。每个请求带 L2 头:`POLY_ADDRESS`(EOA,EIP-55)、`POLY_SIGNATURE`(L2 HMAC,见[签名](#签名))、`POLY_TIMESTAMP`(秒)、`POLY_API_KEY`、`POLY_PASSPHRASE`,有请求体时另加 `Content-Type: application/json`;`redacted = true`。

| op | 请求 | 说明 |
|---|---|---|
| `Place` | `POST /order`,体 `{"order":{"salt":<数字>,"maker","signer","tokenId","makerAmount","takerAmount","side":"BUY"/"SELL","expiration","signatureType":3,"timestamp","metadata":0…,"builder":0…,"signature"},"owner":<API key>,"orderType":"GTD","deferExec":false,"postOnly":<bool>}` | `tokenId` = 该 inst 的绑定(`BindWire`);未绑定 → `UnknownInst`。买单 `makerAmount` = 价 × 量、`takerAmount` = 量(卖单相反),以 1e-6 为单位、必须整除,否则 `FieldOverflow`。`Tif::PostOnly` → `postOnly: true`(会成交即被交易所拒绝),`Tif::Gtc` → `false`;IOC/FOK、reduce-only、平仓标记 → `Unsupported`。`expiration` = 当前秒 + `GTD_S`(300 s;交易所要求晚于当前 60 s 以上),进程失联时挂单至多再留 5 分钟。`salt` = (毫秒时间戳低 33 位) << 20 \| 槽号低 20 位(小于 2⁵³,体里写成数字),`timestamp` = 毫秒时间戳。签名 = 存款钱包签名(类型 3) |
| `Cancel` | 订单哈希已知:`DELETE /order {"orderID":"0x…"}`;未知(REST 回执未到、或重启前的订单):`DELETE /cancel-market-orders {"asset_id":<token>}` | 后者撤该账户在该代币上的全部挂单;本 venue 的策略每个代币至多一张挂单 |
| `CancelAll` | `inst ≠ 0`:`DELETE /cancel-market-orders {"asset_id"}`;`inst = 0`:`DELETE /cancel-all` | |
| `Query` | `GET /data/order/<哈希>` | 哈希未知 → `UnknownTarget` |

**订单 id**:交易所订单 id 是 32 字节哈希,没有 client id。`exch_order_id` = 哈希前 8 字节(大端,0 记为 1),运行时 `IdTable` 的键是它的十进制文本(`exch_id_value` 取回同一值)。`REST_PLACE_BINDS_ID = true`:`decode_rest` 对成功的 `POST /order` 产出 `Ack{slot, gen, inst, exch_order_id}` 并以该键调 `exch_id(key, None)`,运行时把它绑到发起请求的 `(slot, gen, epoch)`([hft-venue-proto 契约 29](../hft-venue-proto/README.md#实现方必须遵守的契约));`GwCtx` 记下完整哈希(撤单、查单用),每账户 512 条,最旧的先覆盖。

**REST 应答**:

| 请求 | 应答 | 产出 |
|---|---|---|
| `POST /order` | 200 `{"success":true,"orderID":"0x…","status":"live"|"matched"|…}` | `Ack`(成交由用户频道报) |
| | 200 `{"success":false,"errorMsg":…}`;400/401/403/404 `{"error":…}`;425、429 | `Reject`,`reject_code` = 消息文本的码(无消息时为 HTTP 码);425/429 另置 `ctl = RateLimited` |
| | 5xx、其他、无应答 | `outcome_unknown` |
| `DELETE /order`、`/cancel-market-orders`、`/cancel-all` | 200 `{"canceled":[哈希…],"not_canceled":{哈希: 原因}}` | `canceled` 里每个能定槽的订单一条 `CancelAck`(每单至多一次,与用户频道的 `CANCELLATION` 去重);单笔 `Cancel` 的 `not_canceled` 非空 → `CancelReject{detail = GONE}` |
| | 非 2xx | 单笔 `Cancel`:`outcome_unknown`(拒绝类 HTTP 码除外);`CancelAll`:只记 `raw_code` |
| `GET /data/order/<哈希>` | 200 订单对象 | `QueryResult`:`LIVE`/`DELAYED` → New(`size_matched = 0`)或 PartiallyFilled,`MATCHED` → Filled,`CANCELED`、`CANCELED_MARKET_RESOLVED`、`UNMATCHED`、`INVALID` → Cancelled;`cum_filled` = `size_matched`,`avg_px` = 订单价 |
| | 200 `null`/空体、404 | `QueryResult{NotFound}`(`stale_guard = None`,不定终态,契约 10) |

### 用户流事件表

下单连接就是用户频道:`login` 发 `{"auth":{"apiKey","secret","passphrase"},"type":"user"}`(订阅账户全部市场,交易所不回复 → `LoginStep::Sent`;登录帧由运行时标 `REDACTED`),`private_subscribe` 不再发帧。帧是一个 JSON 对象或对象数组(每帧至多 16 个),`event_type` 在帧尾,所以 `classify_report` 只区分 `Push`(`{`/`[` 开头)、`Ignorable`(`PONG`)与 `Unknown`;`is_private_report(Push) = true`(计划切换的两条连接上只收归属连接的回报)。

| 事件 | 产出 |
|---|---|
| `order`,`type = PLACEMENT` | 无(回执来自 REST);记下哈希 |
| `order`,`type = UPDATE` | `size_matched` 比 `GwCtx` 里该单的累计量大时一条 `PartialFill`(`= original_size` 时 `Fill`):`qty` = 增量,`px` = `avg_px` = 订单价(挂单按自己的价格成交),`detail` = 方向,`ex_ts_ns` = `timestamp`(毫秒) |
| `order`,`type = CANCELLATION` | 先按 `size_matched` 补成交(同上),再一条 `CancelAck`(未终态且能定槽时) |
| `trade` | 无(成交以订单的累计量为准;`MATCHED → MINED → CONFIRMED` 的链上状态不进事件) |
| 其他 `event_type` | `skip = Unknown`,事件名供 Note |
| `PONG` | `skip = Listed` |

定槽:按 `exch_id(哈希键, None)`;未知的成交以 `UNRESOLVED` 交运行时暂存(REST 回执稍后绑定),未知的撤单丢弃。`asset_id` 不在绑定表里的订单(本引擎之外、或已解绑窗口的订单)不产出事件。

### 累计量与费用来源

| 字段 | 来源 |
|---|---|
| `cum_filled` | 用户频道 `order` 的 `size_matched`(累计,按 inst 的 qty 精度) |
| 本笔量 | `size_matched` − `GwCtx` 中该单上次的累计量;重复推送(两条连接、重放)不再产出 |
| 价格、均价 | 订单的 `price`(挂单不会以别的价格成交) |
| 手续费 | 0:挂单方不付费(吃单费 0.07·p·(1 − p) 本实现不产生,post-only 单不会吃单);返佣按日另发,不进订单事件 |
| `trade_id` | 0;去重键按契约 22 的 `(inst, exch_order_id, fill_seq = cum_filled)` |

### 结果未知码表

交易所的错误是文本(`error` / `errorMsg`),码 = `raw_code_str(文本)`;没有文本时为 HTTP 码 `0x2000_0000 | status`。

| 类 | 码 |
|---|---|
| 确定拒绝(订单没有进入撮合) | HTTP 400(含 `invalid post-only order: order crosses book`、`not enough balance / allowance`、订单格式错误)、401、403、404;HTTP 425(撮合引擎重启中)、429(限速),二者另置 `ctl = RateLimited`;200 且 `success: false` |
| 结果未知 | 5xx、其他未登记状态、无应答 |
| `GONE`(撤单) | 单笔撤单 `not_canceled` 里的任何原因(已成交、已撤或不存在) |

不产出 `ClockSkew`(请求时间戳不做窗口校验)、`UserStreamEnded`、`ServerShutdown`。

### 账户配置核验

启动基线(hft-venue `account::Client`)按序:

| 查询 | 请求 | 判定 |
|---|---|---|
| `Instruments` | `GET /time`(公开) | 2xx 且为正整数秒;槽位 inst 运行期绑定,没有要核对的合约表 |
| `Settings`(REST key 与会话 key 各一次,同一把) | `GET /auth/api-keys` | 返回该钱包的 API key 列表。**venue UID = API key 的前 24 个字符**:refdata 登记的 UID 必须是列表中某个 key 的前缀,否则失败(`Unverifiable` 账户置空);`position_mode = margin_mode = multi_asset = 1`,无杠杆 |
| `Permissions` | `GET /auth/ban-status/closed-only` | `closed_only = true`(只能平仓)→ 失败 |
| `OpenOrders` | `GET /data/orders?next_cursor=<游标>`(首页 `MA==`,末页 `LTE=`) | 启动时有挂单 → 逐个 `Cancel`(`DELETE /order`,应答的 `canceled` 必须含它)后重查 |
| `Balances` | `GET /balance-allowance?asset_type=COLLATERAL&signature_type=3` | `balance`(pUSD,1e-6)→ 计价资产的 `BalanceReport{free}`;结果代币按窗口变化,不在此查询 |

任何非 2xx 或形状不符都失败(启动不放行)。现货借币:不适用(`required_spot_borrow = Some(false)`)。

## 实现说明

<details><summary>实现要点</summary>

| 文件 | 职责 |
|---|---|
| `src/lib.rs` | `VenueProtocol`、`AccountProtocol`、`FundingHistoryProtocol` 实现(资金费入口 `Unsupported`) |
| `src/orders.rs` | 凭证拆分、`Templates`、L2 头、`POST /order`/撤单/查单编码([下单](#下单)) |
| `src/reports.rs` | `GwCtx`、REST 应答与用户频道解码([用户流事件表](#用户流事件表)) |
| `src/account.rs` | 账户启动核验与余额轮询([账户配置核验](#账户配置核验)) |
| `src/bind.rs` | 绑定表与合成序号(固定容量,构造时分配一次) |
| `src/md.rs` | 分类、`peek`、三种消息的解码 |
| `src/session.rs` | 路由、端点、订阅帧、心跳、用户频道登录帧 |
| `src/caps.rs` | `VenueCaps`、限速、过期与死手默认值 |
| `src/sign.rs` | EIP-712 订单与 L1 认证摘要、secp256k1 签名、L2 HMAC、地址([签名](#签名)) |

- 解析:消息头字段用 sonic-rs 借用字符串;`bids`/`asks`/`price_changes` 保持原始文本(`LazyValue`),逐个对象投影(`hft_venue_proto::json::project`,失败回落到校验解码),不在栈上为每档预留空间。
- 列表帧按顶层对象切分(跟踪字符串与嵌套),每帧最多 16 个对象;`price_changes` 每条消息最多 512 条、8 个不同代币,超出 → `Shape`。
- 不缓存 refdata(契约 25):每条消息按 inst 读一次精度。

</details>

## 延迟与资源契约

无基准门槛(optional,尚未进入上线门槛)。解码不分配;去重表在 `FeedCtx::with_capacity` 时分配两张 512 KiB 的表,换代时清一张;`book` 解码用两段 1000 档的 `Level` 栈数组(各 16 KiB)。2026-10-19 录制的 400 帧样本全部解码通过,重建簿的最优价与交易所给出的 `best_bid`/`best_ask` 一致([PMK-011](TESTS.md#pmk-011-实采窗口重建的最优价与交易所一致))。

## 测试与验收

用例与验收见 [TESTS.md](TESTS.md)。

## 依赖

允许:`hft-types`、`hft-refdata`、`hft-event`、`hft-venue-proto`;外部 `sonic-rs` 0.5、`serde`(与其他 venue crate 同版本);签名用 `k256` 0.13(`ecdsa`)、`sha3` 0.10(keccak-256)、`hmac` 0.12、`sha2` 0.10、`base64` 0.22(后三者与 venue-bitget 同版本)。dev:`hft-venue-proto`(`testkit`)、`serde_json`(仅金样比对)。禁止依赖运行时、I/O 与任何 `strategies/*`。

## 延伸阅读

- [接入提案](../../.agents/notes/proposed/architecture/2026-10-19-polymarket-venue.md)
- [hft-venue-proto 契约](../hft-venue-proto/README.md)
- [行情接入验收探针](../../tools/pm-md-probe/README.md)

## 已知限制与后续工作

- 订单心跳(死手):`POST /v1/heartbeats` 是链式的(每次带上一次返回的 `heartbeat_id`),而 `dead_man_refresh` 拿不到 `GwCtx`;接入前靠 GTD(300 s)、窗口结束前策略撤单与进程退出时的全撤兜底。
- 吃单手续费 `FeeModel`(0.07·p·(1 − p)):post-only 下单不产生;以后做吃单再补。
- 重启后的订单哈希:`GwCtx` 不进快照,重启前的挂单只能按代币撤(`/cancel-market-orders`),查单 → `UnknownTarget`(交 reconciler)。
- `tick_size_change` 不解码:每个 5 分钟市场在价格接近 0 或 1 时(约开盘 4 分钟后)把 tick 从 0.01 改为 0.001,之后出现三位小数价格。解码按 px_exp −3 照常解析;hft-core 的 Polymarket 槽位 inst 以 tick 0.001 建 refdata(`pm_sim_config`),所以三位小数的档位照常应用。
- 服务端 1013 断开见[多连接去重](#多连接去重)。两条连接在同一秒都被断开时仍有约 1–2 s 的缺口(该组连接都断开后推 `Resync{CONN_LOST}`,重连后的整簿恢复)。另外,市场结算后约 147 s 交易所以 1000 关闭订阅了该市场代币的连接;hft-net 把它与 1013 记为同一个 `WS_CLOSE` 位,早先"探针每 13 分钟断 3–4 次"的统计把这类正常关闭也算了进去(2026-10-20 重新比对:同机同期同 6 个 BTC 市场,探针异常断开 1 次,Python 录制器 3 次)。

### 开发备忘

<details><summary>给实现者</summary>

已实测(2026-10-19,eu-west-1):订阅帧无应答;同一连接可追加订阅与退订;`PING` → `PONG`;每次成交后推一次整簿。2026-10-22 实测(不可成交的 0.02 post-only 买单):`POST /order` 回 `orderID` 与 `status: live`;用户频道不带市场过滤订阅即收到全账户的 `PLACEMENT`/`CANCELLATION`,与 REST 回执相差 ±50 ms,先后不定;`DELETE /order`、`/cancel-market-orders`、`/cancel-all` 的应答格式同[下单](#下单)。成交(`UPDATE`、`trade`)样本按官方 asyncapi 文档,首个实盘成交后补实采样本。

</details>
