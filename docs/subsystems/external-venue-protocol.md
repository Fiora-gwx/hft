# 外部 venue 协议(hft-core ↔ chain-core,optional)

> 冻结:本页随 hft-ext-proto、hft-chain-evm、apps/chain-core 一起冻结,不随 core 规格同步;解冻(链上策略迁移到新框架)时先按 core 规格复核。已知待复核:原因码 `Paused`(V3 一致性评审 #110 的退役名称)。

本页是 **hft-core 与外部执行场所(chain-core)之间的消息目录、协议自有消息的布局、帧格式、载体、会话、断连与恢复语义、原因码表与版本规则的唯一定义**。**scope: optional**——协议在架构上保留(消息目录、编号段与语义不删),链上(CEX-DEX)策略迁移到新框架时启用;首次上线(ccex)不运行 chain-core,也不实现本协议。启用后 chain-core 只经它与 hft-core 交互,没有任何临时或旁路接口。**协议字段与 chain-core 的 profile(free / builder)无关**:同一消息在两种 profile 下语义相同,profile 只影响 chain-core 内部如何取得证据。跨进程与 seq journal 共用的事件体(`PoolQuote`、`TxLifecycle`、`ChainHead`、`SwapDecision`)的字节布局归 [`hft-event`](../../crates/hft-event/README.md#链上0x03xx),记录头与演进规则见 [event-model.md](event-model.md);协议自有消息的 Rust 类型实现在 [`hft-ext-proto`](../../crates/hft-ext-proto/README.md)(hft-event 只预留链上 kind 编号段),布局以本页为准。chain-core 内部如何产生这些消息见 [`hft-chain-evm`](../../crates/hft-chain-evm/README.md);两端线程与核在 [`apps/chain-core`](../../apps/chain-core/README.md) 与 [`apps/hft-core`](../../apps/hft-core/README.md)。设计依据:[06 §2.9](../../design/06-链上执行层.md)、[13 O1 与 §4](../../design/13-设计评审与优化.md)、[11 §3/§5/§7](../../design/11-实施路线图.md)。

## 1. 角色与总览

```
hft-core                                                   chain-core@bsc(默认:与 hft-core 同机,共享内存环;可选:tokyo-chain,跨机 UDP)
  sequencer ─出环─▶ ext-bsc ══ to_chain ══▶ chain-seq(同机直接轮询环)/ chain-link(跨机 UDP)
            ◀─回报环── ext-bsc ◀═ to_core ═══ chain-seq / chain-link
                 载体:同机 hft-rt::ShmSpsc ×2 / 跨机 UDP(同 CPG)
```

- **hft-core 侧 `ext-<chain>` 线程**(如 `ext-bsc`;线程与核见 [hft-core 线程计划](../../apps/hft-core/README.md#线程计划),`metal` 计划中为核 22、`Spin`):成帧/解帧、会话、可靠通道、去重、断连合成;入站消息作为输入事件推进 sequencer 的回报环(与 gw 回报同优先级),先入 seq journal 再影响决策。接线在 `apps/hft-core/src/chain_link.rs`。
- **chain-core 侧**:同机时 `chain-seq` 直接轮询两条 `ShmSpsc`,会话帧也由它处理;跨机时由 `chain-link` 线程承载 UDP([apps/chain-core 的"与 hft-core 的会话"](../../apps/chain-core/README.md#与-hft-core-的会话))。
- 帧头、协议自有消息、会话状态机的纯逻辑在 `hft-ext-proto`,两端共用一份代码。

对 sequencer 而言 chain-core 就是一个 venue:`PoolQuote` 与 `Bbo` 同级,`TxLifecycle` 与 `OrderEvent` 同级;chain-first 的对冲由 OMS 的 `LegLink` 在处理 `TxLifecycle{IncludedOk}` 的同一事件内出环([hft-oms](../../crates/hft-oms/README.md))。

## 2. 消息目录

| kind | 名称 | 方向 | 通道 | 频率 | 进 seq journal |
|---|---|---|---|---|---|
| `0x01` | `Hello` | 双向 | 控制 | 建链/重连 | 结果以 `ConnState` 记录 |
| `0x02` | `HelloAck` | 双向 | 控制 | 同上 | 同上 |
| `0x03` | `Heartbeat` | 双向 | 控制 | 每 100 ms | 否(超时以 `ConnState` 记录) |
| `0x04` | `Ack` / `Nack` | 双向 | 控制 | 有未确认帧时每 1 ms;发现缺口立即 `Nack` | 否 |
| `0x10` | `BboSnapshot` | core → chain | 最新值 | 汇率 inst 与观察名单 inst 每次变化,每 inst 合并间隔 ≥ `bbo_min_interval_us` | 否(来源 `Bbo` 已在 seq journal) |
| `0x11` | `SwapIntent`(`SwapIntentMsg`) | core → chain | 可靠 | 稀疏 | 以 `SwapDecision`(0x0303)记录,在发出之前 |
| `0x12` | `IntentQuery` | core → chain | 可靠 | 恢复时 | 否 |
| `0x13` | `ConfigChunk` | core → chain | 可靠 | refdata 变更时(§6.1) | 否(`ConfigBlob` 已在 seq journal) |
| `0x20` | `IntentAck` | chain → core | 可靠 | 每个 `SwapIntent` 一次 | 否(传输层确认;拒绝另以 `TxLifecycle{Dropped}` 报告) |
| `0x21` | `TxLifecycle` | chain → core | 可靠 | 每次状态迁移 | 是(0x0304) |
| `0x22` | `PoolQuote` | chain → core | 最新值 | 每 (路由, 评估规模) 每块 ≤ 1 条 | 是(0x0302) |
| `0x23` | `ChainHead` | chain → core | 最新值 | 每个新的 `node_head` | 是(0x0301) |
| `0x24` | `IntentState` | chain → core | 可靠 | `IntentQuery` 的逐条回答 | 转成 `TxLifecycle` 记录(§6) |
| `0x25` | `SyncDone` | chain → core | 可靠 | 恢复同步结束 | 否 |
| `0x26` | `ConfigApplied` | chain → core | 可靠 | 新 refdata 在块边界生效后 | 否(随后重新 `Hello`) |

未知 kind:计数后丢弃,不断开(支持 minor 版本新增消息)。

## 3. 帧格式与协议自有消息

```rust
#[repr(C, align(8))]
pub struct ExtFrameHdr {          // 32 B
    pub magic: u16,               // 0xE7C1
    pub major: u8,                // 协议主版本,当前 1
    pub kind: u8,                 // §2
    pub len: u16,                 // 整帧字节数(头 + 体 + 可选 auth 尾),8 B 对齐,≤ 256(shm 槽长)
    pub flags: u16,               // RELIABLE | RETX | AUTH | SYNTH
    pub session: u32,             // 发送方会话号:进程启动时随机生成,重启即变
    pub chan: u16,                // 逻辑通道:每条链一个(bsc = 1,solana = 2)
    pub _pad: u16,
    pub seq: u64,                 // 可靠通道本方向序号(link_seq),从 1 起;最新值/控制帧为 0
    pub ack: u64,                 // 捎带累计确认:已按序收到的对端最大 seq
}
// 体紧随其后;AUTH 置位时尾部 16 B = HMAC-SHA256(key, 头‖体) 截断(仅 UDP 载体)
```

### 3.1 core → chain

```rust
#[repr(C)] pub struct BboSnapshot {     // 56 B
    pub inst: InstId, pub venue: VenueId, pub _pad: [u8; 3],
    pub bid_px: Px, pub bid_qty: Qty, pub ask_px: Px, pub ask_qty: Qty,
    pub ex_ts_ns: i64,                  // 来源 Bbo 的 ex_ts_ns
    pub src_seq: u64,                   // 来源 Bbo 在 seq journal 的 seq
}
```

用途:chain-core 的汇率(BNB、ETH、BTC、USDC、USD1、FDUSD 对 USDT,用于出价与 gas 折算)与观察名单的机会预筛;它**不驱动发送**,发送只由 `SwapIntent` 触发。

```rust
#[repr(C)] pub struct SwapIntentMsg {   // 104 B;放行的 SwapDecision 的执行投影,同名字段语义与 hft-event SwapDecision、hft-strategy-sdk `chain` 模块的 SwapIntent 相同
    pub intent_id: u64,                 //   0 = SwapDecision.intent_id(编码的唯一定义在 hft-event)
    pub quote_id: u64,                  //   8 = SwapDecision.quote_id(所依据的 PoolQuote)
    pub basis_block: u64,               //  16 该 PoolQuote 的块号
    pub amount_in: Qty,                 //  24 ExactIn:输入量(输入资产 qty_exp);ExactOut:0
    pub min_out: Qty,                   //  32 ExactIn:输出下限;ExactOut:0
    pub amount_out: Qty,                //  40 ExactOut:目标输出量;ExactIn:0
    pub max_in: Qty,                    //  48 ExactOut:输入上限;ExactIn:0 —— min_out/max_in 由信号价决定,chain-core 只能收紧不能放松(NAORIS)
    pub limit_px: Px,                   //  56 信号价,仅诊断
    pub edge: Notional,                 //  64 出价前预期净利(USDT 1e-6),chain-core 按比例出价
    pub max_bribe: Notional,            //  72 出价上限
    pub decided_ns: i64,                //  80 decided_tsc 的 UTC(latency-budget §2.2 的 signal 点)
    pub group: GroupId,                 //  88
    pub route: u32,                     //  92 = SwapDecision.route(路由表下标;路由表的归属与下发见 hft-chain-evm,两端同一版本)
    pub account: AccountId, pub chain: VenueId, pub side: Side /* Buy = 计价资产→代币 */, pub exact: u8, // 96  0 ExactIn 1 ExactOut
    pub valid_blocks: u8, pub sflags: u8 /* RISK_REDUCING = 1,= SwapDecision.sflags */, pub _pad: [u8; 2], // 100 默认 2
}

#[repr(C)] pub struct IntentQuery { pub n: u8, pub _pad: [u8; 7], pub ids: [u64; 16] }   // 136 B

#[repr(C)] pub struct ConfigChunk { pub blob_hash: [u8; 32], pub idx: u16, pub n: u16, pub len: u16, pub _pad: u16, pub bytes: [u8; 176] }  // 216 B
```

**幂等键**:`intent_id` 的编码唯一定义在 [hft-event](../../crates/hft-event/README.md#链上0x03xx)(`SwapDecision.intent_id`),本协议不复述。chain-core 的去重键是 `(intent_id, body_hash)`,`body_hash` = xxh3-64(`SwapIntentMsg` 全部字节):重传必然同体 → `IntentAck{Duplicate}` + 当前状态;**同 id 不同体 → `Rejected(IdCollision)` + P0,chain-core 永不对它回放旧状态**。

**kill 与 `RISK_REDUCING`**:链上意图在 kill 下**只看 `sflags & RISK_REDUCING`**——该位由 hft-core 的 `hft-risk::classify` 判定、OMS 拷入([hft-risk](../../crates/hft-risk/README.md)),chain-core 不按方向、路由或数量另行推断。hft-core 在心跳里携带**每个账户的 KillBits 位图**(§5);chain-core 收到位图变化时先把它写成 chain journal 的 Input 记录(编码归 [hft-event](../../crates/hft-event/README.md),位语义归 [hft-risk](../../crates/hft-risk/README.md))再生效:全局 `HARD` 或该账户的账户 HARD 位置位(位定义见 [hft-types](../../crates/hft-types/README.md#kill-字位))→ 拒绝全部新意图;任一其他 kill 位(Global、Account、本链 Venue)置位 → 只放行带 `RISK_REDUCING` 的意图;拒绝码 `Paused`。chain-core 自己的 `Pause{scope, hard}` 是独立的 cap,规则相同;两者互不覆盖,chain-core 的 `Resume` 清不掉 hft-core 的 kill 位([hft-chain-evm 门控](../../crates/hft-chain-evm/README.md))。

### 3.2 chain → core

`PoolQuote`、`TxLifecycle`、`ChainHead` 的体**与 seq journal 中同名事件体逐字节相同**(布局见 [hft-event](../../crates/hft-event/README.md#链上0x03xx) 与 [event-model](event-model.md#关键事件体));`ext-bsc` 只填 hft-core 侧的接收字段 `rx_tsc` 后原样入环。chain-core 的填充语义(字段名以 hft-event 为准):

| 事件 | 字段 | chain-core 如何填 |
|---|---|---|
| `PoolQuote` | `quote_id`、`block` | chain-core 内单调的报价序号;报价所用状态的块号(本机节点已导入的 `node_head`) |
| | `group`、`route`、`chain` | 路由所属 group;路由表下标;链的 `VenueId` |
| | `level`、`size` | L0(镜像)或 L1(revm);评估规模——路由表给每条路由两个规模(小额探测、Sizer 目标),每个规模每块至多一条 `PoolQuote` |
| | `buy_px`、`sell_px` | 以 `size` 买入/卖出的均价,USDT/代币、资产 `px_exp`,含池费,不含 gas 与出价;买价向上、卖价向下取整 |
| | `depth`、`gas_cost`、`fee` | 深度估计(USDT);一次 swap 的 gas 折算(USDT,不含出价);路由总费率 |
| | `qflags` | `STALE`(池重建/降级中,策略不得据此开仓)、`HOOK_POOL`、`MULTIHOP`、`INDICATIVE`(路由含 `SimOnly` hook 池,价格来自 L1 或中间价,不得用于定界限)、`NO_LIQ`(规模内流动性不足) |
| | `computed_ts_ns` | chain-core 算出该报价的 UTC |
| `ChainHead` | `block`、`finalized`、`hflags` | `block` = 新的 `node_head`(状态可读的块);终局高度;`FINALITY_STALLED`、`NODE_BEHIND`(块流领先本机节点) |
| | `source`、`observed_ts_ns`、`hash`、`block_ts_ns` | 该块首次被任一来源看到的来源与 UTC;块哈希与块时间戳。策略 `ConfirmWindow` 的"跨 ≥ 2 块"用 `block` |
| `TxLifecycle` | `state` | 迁移规则与证据要求见 [hft-chain-evm](../../crates/hft-chain-evm/README.md) 实现说明 §3 |
| | `builder`、`block`、`tx_hash`、`sig_hi` | 包含它的 builder(未知 0xFF);包含块号(未包含为 0);被包含的变体哈希(未包含时为首个变体);Solana 签名的后 32 B(BSC 为 0) |
| | `amount_in`、`amount_out` | 实际输入/输出,各自资产的 `qty_exp`。**hft-core 的链上槽数量一律是基础资产(代币)数量**:Buy 取 `amount_out`,Sell 取 `amount_in`;计价资产那一侧只用于名义与 ledger |
| | `gas_used`、`gas_cost`、`bribe` | gas 用量;gas 费(BNB `qty_exp`,不含出价);付给 builder 的出价(BNB) |
| | `observed_ts_ns` | 触发该迁移的证据首达 UTC(latency-budget 的 `include_seen`) |
| | `reason`、`tflags`、`src_mask`、`wallet` | 见 §3.3 |

`IntentAck`:`{intent_id: u64, status: u8 /* Accepted=1 | Rejected=2 | Duplicate=3 */, _pad: u8, reason: u16, _pad2: u32, chain_seq: u64}`(24 B)。`Rejected` 之后 chain-core 另发 `TxLifecycle{Dropped, reason}`,hft-core 以后者为准。

`IntentState`:`{intent_id: u64, found: u8, _pad: [u8; 7], last: TxLifecycle}`;`found = 0` 表示 chain-core 从未收到该意图,并已为它登记**墓碑**——此后到达的同 id 意图一律拒绝。

### 3.3 码表(`TxLifecycle.reason`、`IntentAck.reason`、`tflags`、`src_mask`)

| `reason` | 名称 | 含义 |
|---|---|---|
| 0 | — | 无 |
| 1 | `SimRevert` | 最新块 L1 模拟 revert,未发送 |
| 2 | `MinOutUnmet` | 最新块模拟结果不满足 `limit`,未发送 |
| 3 | `NonceBusy` | 该账户无空闲钱包(每钱包在途 ≤ 1) |
| 4 | `Stale` | `basis_block` 距最新块过远 |
| 5 | `BadRoute` | 路由不存在、池不在白名单或 hook 池被禁 |
| 6 | `Paused` | kill、运维暂停或会话 `Down` |
| 7 | `WalletTainted` | 钱包检测到外部使用 |
| 8 | `NodeLag` | 块流领先节点超过等待上限 |
| 9 | `RateStale` | 汇率过期 |
| 10 | `Duplicate` | 重复 `intent_id` |
| 11 | `HeadChurn` | 发送前新块过多,重模拟超限 |
| 12 | `BribeBelowMin` | 出价低于所有 builder 下限 |
| 13 | `InsufficientBalance` | 钱包余额不足 |
| 14 | `DeadlinePassed` | 截止时间已过 |
| 15 | (保留) | 编号保留、永不复用 |
| 16 | `IdCollision` | 同一 `intent_id` 的消息体与已记录的不同(P0) |
| 17 | `NotOnboarded` | 该钱包/代币/路由器的授权未完成或 Permit2 已过期(交易路径不临时授权) |
| 18 | `DestNotWhitelisted` | `submit_transfer` 的目的地址不在白名单 |
| 19 | `GasBudget` | 钱包当日 gas 达上限,或该路由连续 revert 达上限 |
| 32 | `LINK_DOWN` | hft-core 合成:会话断开时未确认的意图 |
| 33 | `NOT_RECEIVED` | chain-core 确认从未收到(已登记墓碑) |
| 34 | `REORG` | 所在块在终局前被重组 |
| 35 | `LOG_MISMATCH` | 回执成功但 Swap/Transfer 日志不符 |
| 36 | `EARLY_CONTRADICTED` | 早证据未被回执印证 |
| 37 | `LATE_LANDING` | 已判 `Dropped` 的交易迟到上链,被 `SwapGuard` 的 `maxBlock` 检查 revert(只计 gas) |
| 38 | `NONCE_SUPERSEDED` | 该 nonce 已被本钱包另一笔已知交易(同 nonce 替换或 `Cancel`)在终局块上消耗,本交易永远不会上链 |

1–19 与 `hft-chain-evm::RejectReason` 的判别值一致;码表只追加。`tflags`:`EARLY = 1`(块流早证据触发)、`BAL_MISMATCH = 2`(终局余额差不符)、`SYNTH = 4`(hft-core `ext` 线程合成)。`src_mask`:`NODE = 1`、`RPC1 = 2`、`RPC2 = 4`、`B48 = 8`(48Club bundle 状态)、`STREAM = 16`(块流)、`SUBMIT = 32`(提交响应)、`RPC3 = 64`、`RPC4 = 128`。`RPC1..RPC4` 依次是给出证据的前四个**独立运营商**(运营商 = 封禁/限流域;域到位的映射写在 chain journal 的端点表 `ConfigBlob` 里);没有本机节点时 `NODE` 位恒为 0。

Solana 签名为 64 B:前 32 B 在 `tx_hash`、后 32 B 在 `sig_hi`(布局见 [event-model](event-model.md#关键事件体))。

## 4. 载体

| | 同机 | 跨机(同 CPG) |
|---|---|---|
| 实现 | 两条 [`hft-rt::ShmSpsc`](../../crates/hft-rt/README.md):`/dev/hugepages2M/hft/rings/chain-link/{to_chain,to_core}.ring`(2 MiB 页 hugetlbfs 上的具名文件,[deploy](../../deploy/README.md#hugepages-与-hugetlbfs) 创建挂载) | UDP,每方向一个 socket;一个数据报装 1..N 帧,≤ 1,400 B |
| 槽 / 容量 | 槽 256 B(`hft_event::ext::ExtSlot`);`to_chain` 8,192 槽、`to_core` 16,384 槽 | 接收窗口 1,024 帧 |
| 可靠通道 | 环本身有序无损;seq/ack 仍然工作(为跨重启的会话恢复) | 累计 `Ack`(捎带或独立)+ 缺口即 `Nack`(快速重传)+ 超时重传 `max(2 ms, 4 × srtt)`、上限 50 ms、每帧最多 20 次;乱序缓存,按 seq 去重 |
| 最新值通道 | 不重传,接收方按路由 inst / inst 只保留最新 | 同左 |
| 满/拥塞 | 生产者不阻塞:最新值帧丢弃并计数;可靠帧进本地重传缓冲(1,024 项);缓冲满 → 会话 `Degraded`,hft-core 风控停新意图 | 同左 |
| 鉴权 | 文件权限(同一服务组) | `AUTH`(HMAC 截断 16 B,两端从 Secrets Manager 载入密钥);安全组只放行对端 |
| 预算 | 单跳 p99 ≤ 1 µs(`ShmSpsc` 基准);`TxLifecycle` 从证据到 hft-core 入环 < 20 µs | 同 CPG 单程分布实测后回填 [latency-budget](../latency-budget.md) |

环文件由先启动的一方创建并写环头(魔数、版本、槽长、容量),后启动的一方附着并校验,不一致以退出码 3 拒绝启动。消费者(重)启动时把读位置设为当前写位置(丢弃积压;可靠消息由 §6 找回)。

## 5. 会话与心跳

```
  Down ──(发 Hello)──▶ Handshaking ──(HelloAck{accepted})──▶ Up ──(3 个心跳间隔未收到任何帧 / 载体错误)──▶ Down
                           └──(版本/refdata/角色不符)──▶ Refused(记录原因,每 5 s 重试)
  Up ──(重传缓冲满)──▶ Degraded ──(缓冲回落 < 50%)──▶ Up
```

- `Hello{side: Core|Chain, major, minor, schema_hash: [u8; 16], refdata_hash: [u8; 32], session, peer_last_rx_seq, chan}`。
- 接受条件:`major` 相同;`chan` 相同;**`refdata_hash` 相同**(路由、group、资产指数两端必须同一版本,否则拒绝并告警 P1);chain-core 同一时刻只接受**一个** hft-core 会话(同一账户只由一个系统交易,没有第二实例)。`minor` 不同可以连接,能力取较低者;`schema_hash` 不同只告警(消息体只追加,读者按 `len` 截断或补零)。
- 心跳每 100 ms 一条(带本端 seq/ack、chain-core 的 `node_head`;hft-core 方向另带**每账户 KillBits 位图**,布局归 hft-event);3 个间隔无任何入站帧 → `Down`(300 ms,= hft-core `chain.session_timeout_ms`)。位图只在变化时被 chain-core 记录(见 §3.1 "kill 与 `RISK_REDUCING`");会话 `Down` 期间 chain-core 不接受新意图,恢复后以新会话第一条心跳的位图为准。
- 每次会话状态变化,`ext-bsc` 向 sequencer 输入 `ConnState{venue = 链, state = Up|Down|Degraded, reason}`。

## 6. 断连与恢复语义

**hft-core 侧(`ext-bsc` 在 `Down` 时)**

1. 输入 `ConnState{Down}`;风控把该链 venue 置为不可下单;此后策略评估把全部 `PoolQuote` 视为过期。
2. 对每个**已发出、未收到 `IntentAck`** 的意图,合成 `TxLifecycle{state = Unknown, reason = LINK_DOWN, tflags = SYNTH}`:未确认的 `SwapIntent` 视为 `Unknown`,chain-core 可能已收到并执行。
3. 已确认、未终结的意图保持最后已知状态(`Sent` 等);链路断开本身不是关于它们的证据。风控对这些意图按"可能已成交"预留敞口。
4. 不重发任何旧意图。**`Unknown` 期间不产生任何反向动作**([defensive-patterns](../defensive-patterns.md))。

合成事件只能把状态推向 `Unknown`,与"传输错误 ≠ 业务失败"一致;它们与其他输入一样先入 seq journal。

**chain-core 侧**

1. 继续把在途意图推进到终态(已投出的 bundle 无法撤回;有效期保证 2–3 块内有结论),证据照常写 chain journal。
2. 待发的 `TxLifecycle` 留在可靠通道缓冲;不接受新意图。
3. 汇率(`BboSnapshot`)超过 `rate_max_age_ms` 后,新意图一律 `Rejected(RateStale)`。

**恢复(`Up` 之后)**

1. 对端 `session` 未变(链路抖动):可靠通道按 `peer_last_rx_seq` 续传 `last_acked` 之后的全部 `TxLifecycle`,无需同步。
2. 任一端 `session` 变了:chain-core 在 `HelloAck` 之后**主动推送**上一会话全部非终结意图与 24 h 内终结意图的 `IntentState`;hft-core 另发 `IntentQuery` 列出本端全部非终结意图(含第 2 步合成的 `Unknown`)中未被推送覆盖的部分,chain-core 逐条回答,最后 `SyncDone`。
3. `ext-bsc` 把回答变成输入:`found = 1` 且与本端最后状态不同 → 输入 `TxLifecycle(last)`;`found = 0` → 输入 `TxLifecycle{Dropped, reason = NOT_RECEIVED}`(chain-core 已登记墓碑,该意图不会再被执行;这是执行方给出的证据,等价于 CEX 的"查无此单")。
4. 同一 `(intent_id, state, block)` 只输入一次(`ext-bsc` 维护 256 项在途意图表)。
5. 推送里出现 hft-core 不认识的 `intent_id`(OMS 无对应槽)→ 照常输入,OMS 判 `NakedExposure` P0([incident-response](../runbooks/incident-response.md)):那是 chain-core 执行了、hft-core 却没有记录的交易。

chain-core 的幂等保证:`intent_id` 在任何动作之前写入 chain journal;重复到达只回 `IntentAck{Duplicate}` 与当前 `TxLifecycle`;墓碑与已执行 id 从 chain journal 恢复,保留 24 h。

### 6.1 refdata 切换

1. hft-core 启动或应用新的 refdata(refdata 变更 = hft-core 重启,见 [hft-refdata](../../crates/hft-refdata/README.md))后,把同一 `ConfigBlob` 以 `ConfigChunk` 分片发给 chain-core(可靠通道)。
2. chain-core 收齐后全量校验 `Tokens` 段与路由表;失败 → 保持旧版本并 P1 告警,会话照常。
3. 通过后在**下一个块边界**原子切换(在途意图继续用其创建时的路由快照),写 `ConfigBlob` 进 chain journal,回 `ConfigApplied{blob_hash}`。
4. 双方以新的 `refdata_hash` 重新 `Hello`;切换期间到达的、`refdata_hash` 与 chain-core 当前版本不符的 `SwapIntent` → `Rejected(Stale)`。

路由表的来源与格式以 [hft-chain-evm](../../crates/hft-chain-evm/README.md) 为准(refdata 的原 `routes` 段已退役,见 [hft-refdata](../../crates/hft-refdata/README.md));链上策略接入立项时定稿。

## 7. 版本与演进

- `major` 变更 = 不兼容,两端必须同一发布一起升级;`minor` 用于新增消息 kind 与体尾追加字段。
- 消息体遵循 [event-model 的演进规则](event-model.md#演进规则):只追加、改语义用新 kind、编号永不复用;§3.3 码表只追加。
- `schema_hash` 由 `cargo xtask gen-schema` 对 `hft-ext-proto` 全部布局计算,写进两端二进制与 `Hello`。

## 8. 验收

| 测试 | 内容 |
|---|---|
| 编解码属性 | 任意体 → 帧 → 体往返相等;新旧版本体的截断/补零读取 |
| 可靠性模拟 | UDP 载体在丢包 0–30%、乱序、重复、延迟下:可靠消息按序恰好一次交付;最新值只保留最新 |
| 会话 | 版本不符、`refdata_hash` 不符、第二个 hft-core 会话被拒;任一端重启后 `IntentQuery`/`IntentState` 收敛;心跳丢失只合成 `Unknown`,不产生其他状态 |
| DST | [`hft-testkit`](../../crates/hft-testkit/README.md) 的链路故障(断开、单向丢失、chain-core 重启、墓碑)× sequencer/OMS:断连期间零反向动作;恢复后仓位只由证据改变([testing.md §4](../testing.md)) |
| fuzz | `fuzz/ext_frame`:帧头与各体解码不 panic |
| 基准 | shm 单跳 p99 ≤ 1 µs;UDP 同 CPG 单程分布进每日报表 |

## 延伸阅读

- 范围的决策出处(链上转 optional):[round8-decisions](../../design/_work/round8-decisions.md)
- [hft-ext-proto](../../crates/hft-ext-proto/README.md)、[hft-chain-evm](../../crates/hft-chain-evm/README.md)、[apps/chain-core](../../apps/chain-core/README.md)、[strategies/cdex](../../strategies/cdex/README.md)
