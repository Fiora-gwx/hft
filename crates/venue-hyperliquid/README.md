---
description: "Hyperliquid 永续的纯协议实现:WS bbo/l2Book/trades 解码、每个 action 一次 EIP-712 secp256k1 签名的下单帧(ALO/IOC、cloid、expiresAfter、nonce)与回报解码;可选的第二对冲场所"
kind: crate
tier: pure
scope: optional
status: frozen
---

# venue-hyperliquid

## 概要

> **冻结**:不随 core 规格同步;解冻(触发条件同原文)时先按 core 规格复核。已知待复核(V3 = `architecture-v2/_work/review-2026-10-03-M0c-V3一致性.md` §2):#52 死手 `Schedule` 不在 `DeadManCap` 的名字与单位映射里;#53 能力表缺 `fok` 行。

> **可选扩展**。何时值得接入:需要第二对冲场所或做"HL 上 ALO 挂单 + CEX 对冲",且东京自建非验证节点可行;接入前须复核本 README 标出的修正(userFills 快照去重、`noop` nonce、`scheduleCancel` 语义)。

`venue-hyperliquid` 实现 `HlPerp` 一个 [`VenueProtocol`](../hft-venue-proto/README.md#venueprotocol静态分派)。它拥有 Hyperliquid 的协议知识:公开 WS 的 `bbo`/`l2Book`/`trades` 与用户流、`post` 请求承载的 `order/cancel/cancelByCloid/modify/batchModify/scheduleCancel/noop` action、**每个 action(一批订单)一次 EIP-712 secp256k1 签名**(msgpack → keccak → phantom agent → EIP-712)、nonce 窗口规则、`expiresAfter` 过期保护、ALO(post-only)与 IOC 分批、128-bit `cloid`、价格有效数字规则、IP 与**地址级**限速。它是唯一 `needs_per_order_signing() = true` 的 venue:gw 线程持有 API 钱包私钥并在 `encode` 里签名(≈ 50–150 µs)。Hyperliquid 是"区块链节拍"的场所(≈ 70 ms 一块,官方同城端到端中位 0.2 s / p99 0.9 s),典型形态是"HL 上 ALO 挂单 + CEX 对冲",µs 级优化在这里没有意义([research/02 §8](../../design/research/02-exchange-connectivity.md))。纯函数,运行时见 [`hft-venue`](../hft-venue/README.md)。

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
pub struct HlPerp;   // NAME = "hyperliquid-perp";needs_per_order_signing() = true

// client id(`cloid`):`render_cid(&ClientOrderId, &mut [u8; 48])` / `parse_cid(&[u8]) -> Option<ClientOrderId>`(VenueProtocol 方法)。
// 编码是 hft-types 的 ClientOrderId(96 bit,含 epoch/venue/account/slot/gen/校验,规范 24 位十六进制),本 crate 只加外形:0x + 00000001 + <24hex>,34 字节(128 位十六进制)。

/// 签名器:gw 线程持有;密钥启动期从 KMS/Secrets Manager 载入,不入 journal。
pub struct ActionSigner { /* API 钱包私钥、预计算的 EIP-712 域哈希、上次 nonce */ }
impl ActionSigner {
    pub fn next_nonce(&mut self, unix_ms: i64) -> u64;                  // max(unix_ms, last + 1)
    pub fn sign(&mut self, action_msgpack: &[u8], nonce: u64, expires_after: Option<u64>) -> Sig65;
}
```

### 端点

| 用途 | 端点 | 说明 |
|---|---|---|
| 行情 + 用户流 + 下单 | `wss://api.hyperliquid.xyz/ws` | CloudFront;下单用 `{"method":"post","id":N,"request":{"type":"action","payload":…}}`,响应在 `channel:"post"` |
| REST | `https://api.hyperliquid.xyz/exchange`、`/info` | 兜底与冷查询;`/info` 取 meta(资产下标、`szDecimals`) |
| 自建非验证节点(后续) | 本机文件输出 | 见已知限制 |

WS 限制:≤ 10 连接 / IP、30 次新建 / min、1000 订阅、10 个用户、2000 条入向 / min、100 个在途 `post`;60 s 无消息断开 → 每 30 s `{"method":"ping"}`。

### 行情订阅 → 事件

| 订阅 | 观测节拍(自测) | 规则 | 事件 | 时间戳 |
|---|---|---|---|---|
| `{"type":"bbo","coin":…}` | 只在某块 BBO 变化时推;间隔 p50 108 ms | `ByTime` on `time` | `Bbo` | `ex_ts_ns ← time`(ms,块时间) |
| `{"type":"l2Book","coin":…}` | 默认 20 档约 5 s 一帧;`fast` 5 档约 0.5 s | `Snapshot` | 只作校验 | `time` |
| `{"type":"trades","coin":…}` | 逐块 | `Monotonic` on `tid` | `Trade` | `time` |
| `userFills` / `orderUpdates` / `userEvents` | — | — | `OrderEvent` 等(gw 线程) | `time` |

公开 `l2Book` 被严重限流,块级深度只能靠自建节点或第三方;HL 腿只用 `bbo` 触发、Binance 深度估算冲击。

### 下单:action

| action | 结构 | 说明 |
|---|---|---|
| `order` | `{"type":"order","orders":[{a, b, p, s, r, t:{limit:{tif: "Alo"\|"Ioc"\|"Gtc"}}, c}],"grouping":"na"}` | `a` 资产下标(refdata);`p`/`s` 为字符串;`c` = cloid |
| `cancel` / `cancelByCloid` | `{"type":"cancel","cancels":[{a, o}]}` / `{"type":"cancelByCloid","cancels":[{asset, cloid}]}` | — |
| `modify` / `batchModify` | 按 oid 或 cloid 改价量 | — |
| `scheduleCancel` | `{"type":"scheduleCancel","time":<ms>}` | 设定未来某时刻撤全部挂单;`time` ≥ now + 5 s;不带 `time` 即取消已设定的计划。**只有真正触发(到点撤单)才计次,每日 ≤ 10 次触发**(UTC 0 点重置);反复把 `time` 往后推不计次(以官方文档原文复核) |
| `noop` | 用**被撤那条在途 action 的 nonce** 签一个 noop 发出 | 两者只能有一个被接受:noop 先到,原 action 因 nonce 已用而失效("撤在途单更有效")。用新 nonce 的 noop 没有撤单作用。gw 在槽镜像里保存每条在途 action 的 nonce(64 条环),`Cancel` 命中"仍在途、尚无 oid"的槽时发 noop 而非 `cancelByCloid` |
| 请求外层 | `{"action":…, "nonce":<u64>, "signature":{r, s, v}, "vaultAddress":null, "expiresAfter":<ms>}` | `expiresAfter = now_ms + gw.stale_guard_ms` |

响应 `statuses:[{resting:{oid}} | {filled:{totalSz, avgPx, oid}} | {error:"…"}]`,与请求中订单一一对应。

### 限速与额度

| 范围 | 值 |
|---|---|
| IP | 1200 weight / min;exchange 请求权重 `1 + floor(batch_len / 40)` |
| 地址 | 累计每成交 1 USDC 换 1 个 action,初始缓冲 10 000;超限后 1 请求 / 10 s;批量 n 单计 n;撤单另有额度 `min(limit + 100000, 2 × limit)` |
| 陈旧拒单 | `expiresAfter` 过期被拒的请求按 **5×** 计入地址额度 |
| 挂单上限 | 1000(每 500 万 USDC 成交 +1,上限 5000) |
| 购买额度 | `reserveRequestWeight`,0.0005 USDC / 个 |

### L2 序号归一化

无增量频道:`l2Book` 是整簿快照,只作校验,不产生 `L2Delta`;BBO 与成交按 `time`/`tid` 去重。

### 累计量与费用来源

| 流 | `cum_filled` | `avg_px` | 费用 |
|---|---|---|---|
| `userFills`(非快照) | `GwCtx` 按 `tid` 去重累加 `sz` | 合成 `Σ(px·sz)/Σsz` | `fee`(本笔)+ `feeToken` |
| `post` 响应 `filled{totalSz, avgPx}` | `totalSz`(只作校验) | `avgPx`(只作校验) | — |

### VenueCaps 默认值

`default_caps()`([hft-venue-proto 契约 15](../hft-venue-proto/README.md#实现方必须遵守的契约));只有永续一个产品线(`MarketKind::LinearPerp`,USDC 结算),接入时按 [必备节](../hft-venue-proto/README.md#每个-venue-readme-的必备节) 写全其余各节。

| 字段 | `HlPerp` | 来源 |
|---|---|---|
| `fee_asset_rule` | `Configured(USDC)` | 官方(费用以 USDC 计,`userFills.feeToken` 实测确认) |
| `reduce_only` | 是(`r`) | 官方 |
| `required_position_mode` / `required_margin_mode` | 单向(交易所只有净持仓)/ 全仓(按资产 `updateLeverage` 的 cross 设置) | 待核实 |
| `funding_settle` | `PerSymbolPush`(用户资金费事件带 coin,待核实) | 待核实 |
| `margin_data` | `Poll{interval_ms: 5000}`(`info` 的账户状态:账户价值、全仓维持保证金、持仓强平价) | 待核实 |
| `own_liquidation_marker` | `userFills` 中带强平信息的成交 / 用户事件中的强平事件 | 待核实 |
| `ignorable_user_events` | 空(接入时补) | — |
| `amend` / `post_only` / `ioc` / `batch_cancel` | 是(`modify`)/ 是(`Alo`)/ 是 / 是(一批一签) | 官方 |
| `dms` | `Schedule`(`scheduleCancel`) | 官方 |
| `testnet` | `Testnet{…}`(官方测试网端点待核实) | 待核实 |

ADL 档:未见 HL 提供 ADL 排名(待核实),核实前 `adl_quantile` 填 255(未知)。

### `mock` 特性

与 [venue-binance](../venue-binance/README.md#mock-特性给-mock-exchange-的反向编解码) 相同的四个反向编解码函数(`encode_market`、`decode_order_frame`、`encode_order_event`、`encode_rest_response`),接入时一并交付,供 [mock-exchange](../../apps/mock-exchange/README.md) 使用。

## 实现说明

<details><summary>实现要点</summary>

### 1. 签名流程(`encode` 内,gw 线程)

1. 按出环 `GwCmd` 生成 action 的 **msgpack**:键顺序与字段类型必须与官方 Python SDK 完全一致(否则签名不同、被拒),用手写编码器而非通用序列化库;`p`/`s` 字符串按价格规则格式化。
2. `nonce = signer.next_nonce(wall_clock.unix_ms)`:严格递增的 ms 时间戳。服务端对每个签名者保留最高的 100 个 nonce,新 nonce 须大于其中最小者且未用过,且落在 `(T − 2 天, T + 1 天)`。
3. `connection_id = keccak256(msgpack(action) ‖ nonce(u64 大端) ‖ vault 标志/地址 ‖ [expiresAfter 标志 + u64])`。
4. phantom agent `{source: "a"(主网), connectionId}` → EIP-712(域 `name="Exchange"`、`version="1"`、`chainId=1337`、`verifyingContract=0x0`,类型 `Agent(string source, bytes32 connectionId)`)→ secp256k1 签名 `(r, s, v)`。域分隔哈希与类型哈希启动期预计算。
5. 帧:`{"method":"post","id":<req>,"request":{"type":"action","payload":{…}}}`,`req` 编码 `(op, slot, gen)`。
6. 一批订单一次签名:ALO 与 IOC **分开成批**("验证人优先处理纯 ALO 批";同块内 cancel 与 ALO 先于 IOC/GTC 执行),建议每 0.1 s 一批——批次聚合由 sequencer/OMS 决定,gw 对出环里连续取到的同账户、同类(ALO / IOC)`GwCmd` 合成一个 action 一次签名(每轮 ≤ `cmds_per_loop` 条,不跨轮等待)。

### 2. 价格与数量格式

价格最多 5 位有效数字,且小数位 ≤ `6 − szDecimals`(永续);整数价格总是允许。数量按 `szDecimals` 截断。refdata 的 tick 对 HL 不是常数:`encode` 按上述规则从定点值生成最短十进制串,不满足规则时返回 `EncodeErr::FieldOverflow`,由风控在前面用同一规则拒绝。

### 3. 回报映射

| 输入 | `OrderEvent.what` |
|---|---|
| `statuses[i].resting{oid}` | `Ack`(`exch_order_id ← oid`) |
| `statuses[i].filled{totalSz, avgPx, oid}` | `Fill`(IOC 立即成交;精确成交与费用以 `userFills` 为准) |
| `statuses[i].error` | `Reject`(ALO 会吃单、保证金不足、价格规则、过期等 → `reject_code`) |
| `userFills` | `PartialFill` / `Fill`(`px`、`sz`、`fee`、`tid`、`time`;`cloid` 解 slot)。**订阅后首条消息带 `isSnapshot: true`,是历史成交回放,整条丢弃,不入账**;之后的增量按 `tid` 去重(重连重订阅后的快照同样丢弃,缺口由查询/对账补)。`cum_filled` 由 `GwCtx` 按 `tid` 去重累加合成(契约 9) |
| `orderUpdates` `status ∈ {open, filled, canceled, rejected, marginCanceled…}` | `Ack` / `CancelAck` / `Expired` |

`post` 请求超时或连接断开时结果未知 → slot 进 `Unknown`,用 `orderStatus`(`/info` 或 WS `info` post)按 cloid 查询收敛。

### 4. 死手与陈旧保护

`DeadManSpec::Schedule{min_ahead_ms: 5000, max_triggers_per_day: 10, timeout_ms: 15000, refresh_ms: 5000}`(`timeout_ms ≥ 2 × refresh_ms`):sequencer 的 `DeadMan` 定时器每 `refresh_ms` 发 `DeadManRefresh`,gw 发 `scheduleCancel{time: now + timeout_ms}`把撤单时刻往后推——这就是心跳式死手:hft-core 停摆后,时刻到达即撤全部挂单并计 1 次触发。每日 10 次触发的上限只在反复真实触发时才会碰到;碰到上限后当日交易所侧死手失效,由 `expiresAfter`(每单)+ [kill-switch](../../apps/kill-switch/README.md) 兜底并 P1 告警。计次规则与"推后不计次"以官方文档复核为准。

### 5. 源文件规划

| 文件 | 职责 |
|---|---|
| `src/lib.rs` | `HlPerp` |
| `src/msgpack.rs` | 固定键序的 action 编码器 |
| `src/eip712.rs` | phantom agent、域/类型哈希、`ActionSigner` |
| `src/price.rs` | 有效数字与小数位规则 |
| `src/md.rs` | bbo、l2Book、trades |
| `src/orders.rs` | action 构造、批次、`encode` |
| `src/reports.rs` | `post` 响应、userFills、orderUpdates |
| `src/cid.rs` | cloid 与请求 id |

</details>

## 延迟与资源契约

| 入口 | p50 / p99 预算 | 基准名 |
|---|---|---|
| `decode_md` bbo | ≤ 0.5 µs / ≤ 1 µs | `bench_decode_hl_bbo` |
| msgpack 编码(单单) | ≤ 0.5 µs / ≤ 1 µs | `bench_hl_msgpack_order` |
| `ActionSigner::sign` | ≤ 100 µs / ≤ 150 µs | `bench_hl_sign` |
| `encode` 单单(含签名) | ≤ 110 µs / ≤ 170 µs | `bench_encode_hl_order` |
| `decode_report` userFills | ≤ 1.5 µs / ≤ 3 µs | `bench_decode_hl_fills` |

签名占 gw 线程 ~0.1 ms/批:HL 的 gw 线程不与其他 venue 共线程;与块节拍(70 ms)和端到端 0.2 s 相比可忽略。热路径零分配(msgpack 写入预分配缓冲)。

## 测试与验收

**金样**:用官方 Python SDK 生成的签名测试向量(固定私钥、action、nonce、`expiresAfter`)→ 我们的 msgpack 字节与签名逐字节一致;bbo、l2Book、trades、`post` 响应(resting/filled/error 混合)、userFills、orderUpdates 各状态。

**属性**:nonce 严格递增且在窗口内(含时钟回拨场景);价格格式化满足有效数字规则且与参考实现一致;cloid 往返。

**fuzz**:`fuzz_hl_md`、`fuzz_hl_report`、`fuzz_hl_msgpack_roundtrip`。

**失败路径**:订阅/重订阅后的 `userFills` 快照(`isSnapshot: true`)不产生任何成交事件;同一 `tid` 两次到达只计一次;对在途 action 撤单时 noop 使用原 nonce,原 action 随后被拒;`scheduleCancel` 刷新只推后时刻、不触发撤单。

**验收**:签名向量全过;测试网往返(ALO 挂单、IOC、撤单、`noop`)通过;测试网连续 3 天冒烟([testing §8](../../docs/testing.md#8-上线门槛));地址额度消耗与预测一致。

## 依赖

| 依赖 | 理由 |
|---|---|
| `hft-types`、`hft-refdata`、`hft-event`、`hft-venue-proto` | 同其他 venue |
| `sonic-rs 0.5` | JSON |
| `k256 0.13`(或 `secp256k1 0.29`,以基准为准) | ECDSA 签名 |
| `tiny-keccak 2`(keccak256) | connectionId、EIP-712 哈希 |

**禁止**:`ethers`/`alloy` 全家桶(体量大、分配多;只需两个哈希与一次签名)、`rmp-serde`(键序不可控)、`hft-net`、`tokio`、`serde_json`(dev-only)。

## 延伸阅读

- [research/02 §8 Hyperliquid](../../design/research/02-exchange-connectivity.md)、[§11.2](../../design/research/02-exchange-connectivity.md)
- [04 §5](../../design/04-行情接入层.md)、[05 §7](../../design/05-CEX执行层.md)、[11 实施路线图](../../design/11-实施路线图.md)
- [hft-venue-proto](../hft-venue-proto/README.md)(`needs_per_order_signing`)、[hft-venue](../hft-venue/README.md)、[cookbook/adding-a-venue.md](../../docs/cookbook/adding-a-venue.md)

## 已知限制与后续工作

- **自建非验证节点**(东京,16 vCPU/128 GB;`--write-fills --write-order-statuses --write-raw-book-diffs --disable-output-file-buffering`,`split_client_blocks` 提前 70–150 ms 看到输入)输出的是本地文件而非 WS,不适合 `VenueProtocol` 的帧模型;需要一个文件尾随的 feed 变体,另开 Agent Note。
- **优先费**(读侧 gossip 拍卖、写侧下单优先费)不做,等 HL 腿 PnL 证明值得。
- **vault / 子账户**:`vaultAddress` 固定为 null。
- **现货**:不做。

### 开发备忘

<details><summary>给实现者</summary>

**待核实**

1. msgpack 键序与 `expiresAfter` 参与 connectionId 哈希的确切字节布局——以官方 Python SDK 签名代码与测试为准,先做向量再写实现。
2. 下单优先费字段的格式与计费。
3. `bbo` 是否带单调序号(当前按 `time` 去重)。
4. k256 与 libsecp256k1 绑定在目标机上的签名耗时,选快者。
5. API 钱包有效期(≤ 180 天)的续期流程,纳入 runbook。

</details>
