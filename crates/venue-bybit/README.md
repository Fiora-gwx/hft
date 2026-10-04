---
description: "Bybit linear 与 spot 的纯协议实现:orderbook.1/50 与 publicTrade 解码(cts 撮合时间)、/v5/trade 登录后免签下单帧、order/execution 回报解码;2026-11-12 迁 apne1-az4 后启用"
kind: crate
tier: pure
scope: optional
status: frozen
---

# venue-bybit

## 概要

> **冻结**:不随 core 规格同步;解冻(触发条件同原文)时先按 core 规格复核。已知待复核(V3 = `architecture-v2/_work/review-2026-10-03-M0c-V3一致性.md` §2):#53 能力表缺 `fok` 行;#62 自建成交合成/暂存缓冲(应改用 hft-venue 运行时的统一暂存);#105 协议常量(`HeartbeatSpec` 等)的来源;#106 金样目录 `fixtures/bybit/` 与 `NAME` 常量不一致。

> **可选扩展**。何时值得接入:2026-11-12 Bybit 迁到 AWS 东京 apne1-az4 之后,实测 `orderbook.1` 单向延迟降到 ≤ 3 ms,且离线研究显示有价差或资金费机会。

`venue-bybit` 实现 `BybitLinear`(USDT 永续)与 `BybitSpot` 两个 [`VenueProtocol`](../hft-venue-proto/README.md#venueprotocol静态分派)。它拥有 Bybit v5 的协议知识:`orderbook.{1,50,200}` 的快照/增量与 `u` 连续规则、`ts`(系统时间)与 `cts`(撮合引擎时间)、`publicTrade`、`/v5/trade` 的 `auth` 与 `order.create/amend/cancel` 及批量帧、`X-BAPI-TIMESTAMP`/`X-BAPI-RECV-WINDOW` 头、ack 响应头里的限速余量、`order`/`execution`/`execution.fast` 私有流、`orderLinkId` 格式与限速默认值。**时间窗口**:Bybit 撮合在 AWS 新加坡,**2026-11-12 08:00 UTC 迁到 AWS 东京 apne1-az4**(端点与字段不变);迁移前是 ≈ 70 ms 单程的慢腿,只做"先成交快腿再补 Bybit"或暂不上线,迁移后重测 AZ 再定位([research/02 §4](../../design/research/02-exchange-connectivity.md))。纯函数,运行时见 [`hft-venue`](../hft-venue/README.md)。

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
pub struct BybitLinear;   // NAME = "bybit-linear"
pub struct BybitSpot;     // NAME = "bybit-spot"

// client id(`orderLinkId`):`render_cid(&ClientOrderId, &mut [u8; 48])` / `parse_cid(&[u8]) -> Option<ClientOrderId>`(VenueProtocol 方法)。
// 编码是 hft-types 的 ClientOrderId(96 bit,含 epoch/venue/account/slot/gen/校验,规范 24 位十六进制),本 crate 只加外形:<24hex>,24 字节(≤ 36)。
```

### 端点

| 用途 | 端点 | 说明 |
|---|---|---|
| 公共行情 | `wss://stream.bybit.com/v5/public/linear`、`/v5/public/spot` | CloudFront 前置;`race=true` |
| 下单 | `wss://stream.bybit.com/v5/trade` | `auth` 后免签;未登录不回 ping |
| 私有推送 | `wss://stream.bybit.com/v5/private` | `order`、`execution`、`execution.fast`、`position`、`wallet` |
| REST | `https://api.bybit.com`(备 `api.bytick.com`) | 冷查询、兜底 |

机构通道 MMWS/MMGW(私有主机名、≤ 10 白名单 IP、承载 JSON 与 SBE)官方声明"不保证更低绝对延迟",不接;SBE 行情与 SBE 下单只在 MMWS 上提供。

### 行情频道 → 事件

| 频道 | 节拍 | 规则 | 事件 | 时间戳 |
|---|---|---|---|---|
| `orderbook.1.<sym>`(独立 BBO 连接,双路) | 10 ms(linear/spot) | `Snapshot`(每条都是 1 档快照;3 s 无变化重发)`Monotonic` on `u` | `Bbo` | `ex_ts_ns ← cts`,`ex_send_ns ← ts` |
| `orderbook.50.<sym>` | 20 ms | 首帧 `type=snapshot`,之后 `delta` 按 `Chain`(`prev_id = u − 1`,待核实) | `L2Snapshot` / 每条消息一条 `L2Batch` | 同上 |
| `orderbook.200.<sym>` | linear 100 ms / spot 100 ms | 同上 | 同上 | 同上 |
| `publicTrade.<sym>` | 实时,≤ 1024 笔/帧 | 见开发备忘(去重键) | `Trade{taker_side ← S}` | `ex_ts_ns ← T` |
| `tickers.<sym>` | 100 ms(衍生品) | `ByTime` | `MarkPrice`、`IndexPrice`、`Funding` | `ts` |

- 收到新的 `snapshot`(含 `u == 1`,服务重启)一律重置该 inst 的簿:运行时把它当作 `L2Snapshot` 处理,序号从新快照继续。
- 重同步:`ResyncMethod::Resubscribe`。
- RPI 单不进 orderbook;"看得见的最优价未必可成交"。

### 下单:`/v5/trade`

| 操作 | op | 要点 |
|---|---|---|
| 登录 | `auth` | `args[apiKey, expires, signature]`,`signature = hex(HMAC-SHA256(secret, "GET/realtime" + expires))`;也支持 RSA-SHA256(自生成 key) |
| 下单 | `order.create` | `args[{category, symbol, side, orderType:"Limit", qty, price, timeInForce ∈ {GTC, IOC, FOK, PostOnly}, orderLinkId, reduceOnly}]` |
| 批量 | `order.create-batch` / `amend-batch` / `cancel-batch` | linear 20、spot 10 |
| 改单 | `order.amend` | — |
| 撤单 | `order.cancel` | — |
| 请求头 | `header{"X-BAPI-TIMESTAMP", "X-BAPI-RECV-WINDOW"}` | 服务端规则 `server − recv_window ≤ ts < server + 1000`;`recv_window = gw.stale_guard_ms` |
| 响应 | `{"reqId","retCode","retMsg","op","data":{orderId, orderLinkId},"header":{X-Bapi-Limit, X-Bapi-Limit-Status, X-Bapi-Limit-Reset-Timestamp, Timenow, Traceid}}` | **ack ≠ 成交**,状态以私有流为准 |

热路径不设 `rpiTakerAccess`(IOC/FOK 2026-09-23 起受其影响);`smpType` 保持 None。

### 限速默认值

| 范围 | 值 |
|---|---|
| 下单/撤单/批量 / UID(滚动 1 s) | linear 10/s,spot 20/s;amend 10/s;VIP3–Supreme 至 60/60/40 |
| PRO 框架 | PRO1 200/s … PRO6 1200/s;MM1–3 600/800/1000/s |
| HTTP | 600 req / 5 s / IP(`403` = 封 10 min) |
| WS | 500 新连接 / 5 min / 域名;≤ 1000 行情连接 / IP;spot 每次订阅 ≤ 10 args;IP 3000 req/s(`10403`) |

WS 与 HTTP 共享账户限速;`X-Bapi-Limit-Status` → `RateObs`。

### L2 序号归一化(`L2Batch.prev_id/last_id`)

| 流 | `prev_id` | `last_id` | 快照 `snap_id` |
|---|---|---|---|
| `orderbook.50/200` `type=delta` | `u − 1`(前提:`u` 逐条 +1,待核实) | `u` | `type=snapshot` 帧的 `u` |
| 快照后第一条 | `snap_id`(`RxMeta.prev_override`) | `u` | — |

**以官方文档核实结果为准**:官方文档对 `u` 只写"update id,`u = 1` 表示服务重启后的快照",未明示 delta 逐条 +1(本地缓存文档未含该页,待核实)。核实方法:官方 orderbook 页原文 + 1 h 双 inst 捕获统计相邻 delta 的 `u` 差值分布。结论写回本节:① 逐条 +1 → 上表生效;② 否则 `prev_id = 上一条 delta 的 u`(同一连接内按到达取),不做跳号检测,只靠 `type=snapshot`(含 `u = 1`)重置与定期重订阅校验簿,并在 hft-book 的 Bybit 条目里注明"无法检测断裂"。契约见 [hft-venue-proto](../hft-venue-proto/README.md#实现方必须遵守的契约)。

### 累计量与费用来源

`execution.fast` 是最快的成交回报,但只有单笔 `execQty`/`execPrice`,没有累计量与费用。`GwCtx` 维护每槽一条合成记录(按 slot 下标的预分配数组,`{cum_qty, cum_notional, fee, last_exec_ids: [u64; 8] 环}`):

1. 收到 `execution.fast`:`execId`(字符串,xxh3 成 u64)在该槽最近 8 个里出现过 → 丢弃(重复);否则 `cum_qty += execQty`、`cum_notional += execQty × execPrice`,产出 `OrderEvent{cum_filled = cum_qty, avg_px = cum_notional / cum_qty, fee = 0}`。
2. 收到 `execution`(完整成交,带 `execFee`/`feeCurrency`/`cumExecQty` 的订单级累计,待核实):按 `execId` 找到已计过的那笔 → 只补费用,产出一条 `OrderEvent{what: FeeOnly, qty: 0, fee, fee_asset}`([hft-venue-proto 契约 12](../hft-venue-proto/README.md#实现方必须遵守的契约));若它先于 `execution.fast` 到达 → 按第 1 步计量并带费用,之后到达的 fast 按重复丢弃。
3. `order` 推送的 `cumExecQty`/`avgPrice` 作为校验:与合成值不符 → 以 `order` 推送为准修正并计数告警(不按解码失败处理;`REPORT_DECODE` 只用于真正解析不了的帧)。

**maker 成交的槽解析**:`execution.fast` 对 maker 成交可能不带 `orderLinkId`(空串,待核实)。此时按 `orderId` 查 gw 的 `IdTable`——`order.create` 的 ack 同时返回 `orderId` 与 `orderLinkId`,gw 在 ack 时插入 `orderId → (slot, gen)`;ack 尚未到达而成交先到 → 该事件先以 `SlotId::UNRESOLVED` 暂存在 `GwCtx` 的小缓冲(≤ 64 条),ack 到达后补发;超时 1 s 仍未解析 → 以 `SlotId::UNRESOLVED` 发出(sequencer 走对账)。

| 流 | `cum_filled` | `avg_px` | 费用 |
|---|---|---|---|
| `execution.fast` | `GwCtx` 按 `execId` 去重累加 | 合成 `cum_notional / cum_qty` | 无 |
| `execution` | 同上(与 fast 按 `execId` 合并) | 同上 | `execFee` + `feeCurrency` |
| `order` | `cumExecQty`(只作校验) | `avgPrice`(只作校验) | — |

### VenueCaps 默认值

`default_caps()`([hft-venue-proto 契约 15](../hft-venue-proto/README.md#实现方必须遵守的契约));optional,除协议字段外待核实,接入时按 [必备节](../hft-venue-proto/README.md#每个-venue-readme-的必备节) 写全账户配置核验、保证金数据换算、外部成交识别、用户流事件表与测试网各节。

| 字段 | `BybitLinear` | `BybitSpot` | 来源 |
|---|---|---|---|
| `fee_asset_rule` | `Quote`(USDT) | `ReceivedAsset`(现货手续费扣收到的币) | 待核实 |
| `reduce_only` | 是 | 否 | 官方 |
| `required_position_mode` / `required_margin_mode` | 单向持仓 / 全仓(统一账户),多资产关闭 | 不适用 | 待核实(`account/info` 一类端点) |
| `required_spot_borrow` | `None`(永续) | `Some(false)`:统一账户的现货杠杆(spot margin trading)必须关闭,开启 → `AUTO_BORROW`(端点与字段待核实) | 待核实 |
| `funding_settle` | `PerSymbolPush`(`execution` 流的资金费类成交带 symbol,待核实;否则 `BalanceOnlyPush`) | 不适用 | 待核实 |
| `margin_data` | `Push`(`wallet` 推送账户维持保证金率,`position` 推送强平价与 ADL 档) | 不适用 | 待核实 |
| `own_liquidation_marker` | `execution` 的成交类型为强平成交 / ADL 成交 / 交割(`execType` 取值待核实) | 无 | 待核实 |
| `ignorable_user_events` | 空(接入时补) | 空 | — |
| `amend` / `post_only` / `ioc` / `batch_cancel` | 是 / 是(`PostOnly`)/ 是 / 是(`cancel-batch`) | 同左 | 官方 |
| `dms` | DCP(仅机构账户;否则无,由 kill-switch 兜底) | 同左 | 官方 |
| `testnet` | `Testnet{…}`(测试网与模拟盘端点待核实) | 同左 | 待核实 |

### `mock` 特性

与 [venue-binance](../venue-binance/README.md#mock-特性给-mock-exchange-的反向编解码) 相同的四个反向编解码函数(`encode_market`、`decode_order_frame`、`encode_order_event`、`encode_rest_response`),接入时一并交付,供 [mock-exchange](../../apps/mock-exchange/README.md) 使用。

## 实现说明

<details><summary>实现要点</summary>

1. **分派**:公共推送以 `{"topic":"orderbook.1.` / `"publicTrade.` / `"tickers.` 开头;`{"success":` 为订阅/心跳回执;`{"op":"pong"` 为心跳。下单连接以 `{"reqId":` 开头;私有连接以 `{"topic":"order"` / `"execution"` / `"execution.fast"` 开头。
2. **模板**:

   ```text
   {"reqId":"p0000002a00000003","header":{"X-BAPI-TIMESTAMP":"1790000000000","X-BAPI-RECV-WINDOW":"1500"},
    "op":"order.create","args":[{"category":"linear","symbol":"BTCUSDT","side":"Sell","orderType":"Limit",
    "qty":"0.004","price":"65000.1","timeInForce":"IOC","orderLinkId":"1a2b3c4d5e6f708192a3b4c5","reduceOnly":false}]}
   ```

   `reduceOnly` 只在 `GwCmd.oflags` 带 `REDUCE_ONLY` 时为 true。

3. **回报映射**:`retCode=0` → `Ack`(`exch_order_id` = `orderId` 的 xxh3,UUID 全文留在 gw 的 id 表);`retCode≠0`:登记为确定拒绝(`10001` 参数、`10002` 时间窗、`10006`/`10403` 限速、`110007` 余额不足…)→ `Reject`;**结果未知**(`10000` 服务端超时、`10016` 服务端错误)与任何未登记码 → `outcome_unknown`(契约 8);两张表在 `codes.rs`。`execution.fast`(精简低延迟成交:`execId, execPrice, execQty, orderId, orderLinkId, side, isMaker, execTime, seq`)→ `PartialFill/Fill`,`ex_ts_ns ← execTime`;**它不带累计量与费用,由 `GwCtx` 合成**(见"累计量与费用来源");`execution` 只补费用(`execFee`、`feeCurrency`),按 `execId` 关联,同一 `execId` 只计一次成交;`order` 推送 `orderStatus ∈ {New, PartiallyFilled, Filled, Cancelled, Rejected, PartiallyFilledCanceled, Deactivated}` → `Ack/CancelAck/Expired`(IOC 剩余);`position`、`wallet` → `PositionReport`/`BalanceReport`。
4. **心跳**:`{"op":"ping"}` 每 20 s(`HeartbeatSpec{ping: AppJson, interval_ms: 20000, unconditional: true}`)。
5. **死手**:DCP(Disconnection Protect)**仅机构账户可开**(默认 10 s,窗口 3–300 s,需订阅 `dcp` 主题)。我们不满足资格时 `DeadManSpec::None` → [kill-switch](../../apps/kill-switch/README.md);获批后 `OnDisconnect{window_ms}`。
6. **源文件**:`lib.rs`(两个类型)、`cid.rs`、`sign.rs`(HMAC/RSA)、`md.rs`、`orders.rs`、`reports.rs`(响应、order、execution、execution.fast、position、wallet)、`codes.rs`(确定拒绝 / 结果未知两张表)。

</details>

## 延迟与资源契约

| 入口 | p50 / p99 预算 | 基准名 |
|---|---|---|
| `classify` | ≤ 30 ns / ≤ 60 ns | `bench_classify_bybit` |
| `decode_md` orderbook.1 → `Bbo` | ≤ 0.4 µs / ≤ 0.8 µs | `bench_decode_bybit_ob1` |
| `decode_md` orderbook.50 delta 1 KiB | ≤ 1.5 µs / ≤ 3 µs | `bench_decode_bybit_ob50` |
| `encode` order.create | ≤ 0.3 µs / ≤ 0.6 µs | `bench_encode_bybit_create` |
| `decode_report` execution.fast | ≤ 0.8 µs / ≤ 1.5 µs | `bench_decode_bybit_execfast` |

纯函数,热路径零分配零系统调用。

## 测试与验收

**金样**(`fixtures/bybit/`):orderbook.1(含 3 s 重发)、orderbook.50 snapshot/delta/`u==1` 重置、publicTrade(多笔帧)、tickers、auth 成功/失败、order.create/批量/amend/cancel 成功与主要 `retCode`、`recv_window` 超时拒单、execution.fast、execution、order 全状态、position、wallet。迁移前后各采一套(字段应不变,时间戳分布会变)。

**属性**:`orderLinkId` 对任意 `ClientOrderId` 往返(24 字节);`encode` 输出可被 `sonic-rs` 解析且与输入 `GwCmd` 一致;同一 `execId` 在 `execution.fast` 与 `execution` 中只产生一次成交;去重键带 inst(`(inst, execId)`,[契约 22](../hft-venue-proto/README.md#实现方必须遵守的契约))。

**fuzz**:`fuzz_bybit_md`、`fuzz_bybit_report`、`fuzz_bybit_encode_roundtrip`。

**验收**:金样全绿;fuzz 每个目标累计 ≥ 2 h 零 panic(跨多夜累计,任一失败清零,[testing §8](../../docs/testing.md#8-上线门槛) 门槛 ③);11-12 迁移后重测 AZ 与单向延迟(期望 `orderbook.1` 从 36 ms 降到 1–3 ms)再在测试网连续 3 天冒烟([testing §8](../../docs/testing.md#8-上线门槛))。

## 依赖

`hft-types`、`hft-refdata`、`hft-event`、`hft-venue-proto`、`sonic-rs 0.5`、`hmac 0.12` + `sha2 0.10`、`hex`;RSA 登录需要时加 `rsa 0.9`(warm 线程)。**禁止**:`hft-net`、`hft-venue`、`tokio`、`serde_json`(dev-only)。

## 延伸阅读

- [research/02 §4 Bybit](../../design/research/02-exchange-connectivity.md)、[04 §5](../../design/04-行情接入层.md)、[05 §7](../../design/05-CEX执行层.md)
- [research/06 §6](../../design/research/06-data-plane-replay-observability.md)(DCP 资格)
- [hft-venue](../hft-venue/README.md)、[cookbook/adding-a-venue.md](../../docs/cookbook/adding-a-venue.md)

## 已知限制与后续工作

- **11-12 前是慢腿**:不适合作需要快速对冲的腿;领先-滞后研究在迁移后重做。
- **无 SBE/MMWS**:机构关系落地后评估(先测再谈,官方不承诺延迟)。
- **spot FIX 4.4**(`fix-oe.bybit.com:9000`,RSA)不做。
- **DCP**:非机构账户无交易所侧死手,依赖 kill-switch。

### 开发备忘

<details><summary>给实现者</summary>

**待核实**

1. `publicTrade` 是否带单调的 `seq`(trade id `i` 是 UUID,不能做单调去重);若无,trade 走 `SeqRule::None` 且只从 primary 连接取。
2. 私有/交易连接 `max_active_time` 参数的默认行为(调研记为 30 s–10 min);若默认会到时断开,轮换窗口相应缩短。
3. 迁移后 `stream.bybit.com` 与 `stream.bytick.com` 的前置与 AZ;是否仍经 CloudFront。
4. `execution.fast` 是否包含 `orderLinkId`(用于直接解 slot)。
5. 两条同订阅连接的 orderbook.50 增量是否逐条相同。

</details>
