---
description: "chain-core 的业务逻辑(cold tier):free profile(免费公共 RPC、同一签名交易并发提交、≥ 2 独立运营商回执、nonce 槽候选集合、SwapGuard 链上过期、gas 上限)的 TxState 生命周期、门控与 chain journal;builder profile 为可选扩展"
kind: crate
tier: cold
scope: optional
status: frozen
---

# hft-chain-evm

## 概要

> **冻结**:不随 core 规格同步;解冻(触发条件同原文)时先按 core 规格复核。已知待复核:无(`architecture-v2/_work/review-2026-10-03-M0c-V3一致性.md` §2 中没有只涉及本组件的行)。

> **可选扩展:CEX-DEX 策略从旧框架迁移时启用。** 新框架首要服务 CEX-CEX 策略,链上策略继续在旧框架运行、迁移不急;本规格保留,启用前按届时的实际情况复核。下文中的"core""迁移范围内"指该迁移启用之后的范围;外部 venue 协议与链上事件编号段在架构上保留。

`hft-chain-evm` 是 [`chain-core`](../../apps/chain-core/README.md) 的全部业务逻辑:把 hft-core 发来的 swap 意图变成一笔**带链上过期**的交易送上链,再用多个**独立运营商**的证据驱动 `TxState` 生命周期直到终局,把 `TxLifecycle`(及 `ChainHead`)经[外部 venue 协议](../../docs/subsystems/external-venue-protocol.md)交回 hft-core。它是链上策略免费版在新框架里的链上腿:只用免费公共 RPC 与免费 WebSocket,不需要自建节点、块流或 builder。

按 **cold tier** 实现:可以用 `tokio`、成熟的 RPC 客户端(`alloy`)与 hft-journal 的离线写者;它不在 hft-core 的 tick-to-trade 路径上。发送方式抽象为 **profile**:**free**(迁移范围内,core)与 **builder**(可选扩展:bundle 直投 builder、块流、自建节点;本规格只保留 profile 抽象,不展开实现)。两者共用生命周期状态机、nonce 槽、`SwapGuard`([contracts/](../../contracts/README.md))、门控与 chain journal。本 crate 拥有在途意图与交易、nonce 槽、执行钱包状态、端点健康度、cap 集合与 chain journal 的写入;不拥有策略(链上策略包,规则见 [strategies/AGENTS.md](../../strategies/AGENTS.md))、仓位与 PnL([hft-ledger](../hft-ledger/README.md))、路由的生成(`python/pipeline` → refdata)。

贯穿全 crate 的规则:**传输错误只能把交易推入 `Unknown`,只有链上证据能改变状态**([defensive-patterns](../../docs/defensive-patterns.md),9/24 事故);**每笔交易都有链上过期**(`SwapGuard.maxBlock`),迟到上链只会 revert、只花 gas。它不改变包含速度与被抢跑(公共 mempool),只消除误判失败、重复成交、旧价迟到成交、单源回执这几类正确性事故。

## 目录

- [使用本组件](#使用本组件)
- [实现说明](#实现说明)
- [延迟与资源契约](#延迟与资源契约)
- [测试与验收](#测试与验收)
- [依赖](#依赖)
- [延伸阅读](#延伸阅读)
- [已知限制与后续工作](#已知限制与后续工作)

## 使用本组件

### 相对旧框架链上引擎的改进点

依据现有代码(服务器 9/26–9/30 版本,快照在 `E:\trading\analysis\util0929\src` 与 `E:\trading\analysis\strat0926\orig5`:`traders/bsc_trader.py`、`traders/bsc_route.py`、`core/open_position_bot.py`、`core/close_position_bot.py`、`monitoring/spread_engine.py`)。现有系统迁移前照常运行、不做改动(第七轮裁决 R7-1);下表是新系统要从结构上消除的问题,也是迁移验收的对照项。

| 规则 | 旧框架的做法 | 问题 | 本 crate 的规则 |
|---|---|---|---|
| 传输错误只进 `Unknown` | `_send_raw` 依次试节点,`nonce too low` 在前面没有超时时直接抛错;`_wait_receipt` 90 s 超时 → `RECEIPT_UNKNOWN`;机器人 `_verify_by_balance` 余额 20 s 内没变 → **"按没成交处理"**,随后换新 nonce 重试 | 余额没变不是没成交的证据;没有链上过期的旧交易可能晚到,重试后两笔都成交(重复买卖) | 状态只由证据改变;`Unknown` 时该 group 不开新仓、不反向,等收敛(实现说明 §3) |
| 本地 nonce 分配器 | `_nonce()` = 公共节点 `get_transaction_count(addr, "pending")`;价差引擎每币一个执行线程,多币可同时发同一钱包 | 私有/滞留交易对 pending 不可见;两线程读到同一 nonce | nonce 槽 → 候选交易集合;每钱包 `Open` 槽 ≤ 1;复用 nonce 必须提价 |
| 每笔交易有链上过期 | 路由器直接调用(`recipient = self.address`、UR `MSG_SENDER`、聚合器 `swap_data['to']`);`_ensure_approval` 在买卖路径上临时授权 | 滞留在公共 mempool 的旧交易可在价格回去后成交;临时授权多等一个块(9/25 复盘) | 每笔 swap 经 `SwapGuard`(`maxBlock = 发送块号 + N`、`minOut`);授权在上币时完成;聚合器与只报价的 UniV4 路由不支持 |
| ≥ 2 个独立运营商回执 | `_wait_receipt` 任一节点返回回执即采用 | 单源回执,落后或出错的节点给出错误结论 | 运营商按封禁/限流域划分,≥ 3 个回执来源,≥ 2 个一致才算 |
| 证据检测 | 0.5 s 轮询回执(前 3 s 0.15 s),中位 1.3 s 才看到成交 | 慢,且把公共节点问到 403 | WS `newHeads` 每块向两个域拉回执;每端点令牌桶 |
| 对冲 | Python 机器人收到成交后下币安单,中位 146 ms、p90 30 s | 慢且受线程调度影响 | `TxLifecycle{IncludedOk}` 进 seq journal,OMS `LegLink` 在同一事件内出环(`leg_gap`) |
| 可追溯 | `print` 文本 + 交易 CSV | 无法按交易重建生命周期 | chain journal(每条输入先记录再处理),可回放 |

### profile

`send.profile = "free"`(迁移范围内只有 free)。下表之外的模块两者共用(`IntentTable`、`TxTracker`、`NonceAllocator`、`SwapGuard` 编码、`Routes`、`Onboarding`、门控、`ChainCtl`、chain journal)。

| 模块 | free(core) | builder(可选扩展) |
|---|---|---|
| 预检 | 公共 RPC `eth_call` + `eth_estimateGas`(`latest`,两个独立运营商) | 自建节点 `eth_call` / revm |
| 签名 | 每意图一笔交易;gasPrice = clamp(端点中位价 × 倍数, 下限, 上限) | 每家 builder 一个变体,共享 nonce |
| `SwapGuard.maxBlock` | 发送时块号 + `free.max_blocks`(默认 3) | bundle `maxBlockNumber` |
| 出价 | 无(`bribeTo = 0`) | 合约内转给 builder |
| `Sent` 语义 | 同一字节并发 `eth_sendRawTransaction` 到 K 个独立运营商;**至少一个端点接受**(返回哈希或 `already known`) | 至少一家 builder 确认收到 |
| gas 记账 | 失败交易会上链:`IncludedReverted` 计 gas | bundle 失败不上链 |
| 块头 / 证据来源 | 免费 WS `newHeads`(无 WS 时轮询);≥ 3 个独立运营商的公共 RPC | 自建节点、块流、builder 状态 |
| 报价(`PoolQuote`) | 每块一次 Multicall3 批量 `eth_call`(Quoter / 路由模拟) | L0 池镜像([hft-amm](../hft-amm/README.md))或 revm |

builder profile 值得做的触发条件:free profile 的包含延迟与被抢跑率(每日报表)成为 D/E 策略的主要损失来源,且付费通道的预期收益覆盖成本;立项时再评估是否热化 chain-core 与是否需要 [chain-grpc-relay](../../apps/chain-grpc-relay/README.md)、[hft-amm](../hft-amm/README.md) 的 L0 镜像。

### 与 hft-core 的接口

- 输入 `SwapIntentMsg`(协议 §3.1;字段与 profile 无关),输出 `TxLifecycle`、`IntentAck`、`IntentState`、`ChainHead`、`PoolQuote`([external-venue-protocol](../../docs/subsystems/external-venue-protocol.md))。free profile 的 `PoolQuote` 由**每块一次的 Multicall3 批量 `eth_call`**(各路由两个评估规模的 Quoter / 路由模拟,沿用现有价差引擎的做法)生成,`level` 记为 L1(链上模拟);批量请求受端点令牌桶约束,路由多时按优先级(有持仓、有在途、名单靠前)分批轮转,未刷新的路由报价带 `STALE`。链上策略包只看这些 `PoolQuote`。
- **成交数量一律是基础资产(代币)数量**(第六轮裁决 B-7):Buy 取 `TxLifecycle.amount_out`,Sell 取 `TxLifecycle.amount_in`;计价资产那一侧只用于名义与 ledger。数量来自 `SwapGuard.Swapped` 事件,经证据规则校验。
- `RISK_REDUCING` 只看 `SwapIntentMsg.sflags`(由 hft-core 的 `hft-risk::classify` 判定),本 crate 不按方向或路由推断。
- 路由来自 refdata `routes` 段([hft-refdata](../hft-refdata/README.md)),由 hft-core 在会话建立时经协议转发 `ConfigBlob`(协议 §6.1);chain-core 不另有路由文件。

### 核心类型

```rust
pub struct ChainSeq { /* 单写者:全部业务状态;每条输入先写 chain journal 再处理 */ }
impl ChainSeq {
    pub fn new(cfg: &ChainConfig, j: JournalWriter) -> Result<Self, ConfigErr>;
    pub fn recover(&mut self, r: &mut JournalReader) -> Result<RecoverReport, RecoverErr>;   // 恢复后对全部非终结槽重新取证
    pub fn on_input(&mut self, inp: ChainIn) -> Outputs;                                     // 确定性:同一输入序列 → 同一输出
}
pub enum ChainIn { Ctl(ChainCtlCmd), Link(ExtFrame), Evidence(EvidenceMsg), PreSend(PreSendMsg),
                   Head(HeadMsg), Rpc(RpcMsg), Quote(QuoteBatchMsg), Timer(ChainTimer) }

pub trait Profile {                  // free 一个实现(builder 可选);enum 分派
    fn presend(&mut self, it: &Intent) -> PreSendJob;
    fn sign(&self, it: &Intent, grant: &NonceGrant, signer: &Signer) -> Candidates;
    fn submit(&mut self, c: &Candidates) -> SubmitTicket;
    fn evidence_sources(&self) -> &SourceSet;
}

#[repr(transparent)] pub struct WalletId(pub u8);
#[repr(transparent)] pub struct DomainId(pub u8);          // 运营商 = 封禁/限流域
pub enum IntentKind { Swap, Approve, Transfer, Cancel }

pub struct Intent {
    pub intent_id: u64,               // 唯一定义见 hft-event(SwapDecision.intent_id)
    pub body_hash: u64,               // 去重键第二半:xxh3-64(SwapIntentMsg 全部字节)
    pub kind: IntentKind, pub account: AccountId, pub wallet: WalletId, pub route: u32,
    pub side: Side, pub exact: SwapExact,
    pub amount_wei: U256, pub limit_wei: U256,   // limit = min_out / max_in,来自信号价,永不放松
    pub max_block: u64,               // 写进 SwapGuard 调用
    pub risk_reducing: bool,          // = sflags & RISK_REDUCING
    pub state: IntentState,
}

#[repr(u16)] pub enum RejectReason {  // 判别值 = 协议 §3.3 码表
    SimRevert = 1, MinOutUnmet, NonceBusy, Stale, BadRoute, Paused, WalletTainted, NodeLag, RateStale, Duplicate,
    HeadChurn, BribeBelowMin, InsufficientBalance, DeadlinePassed, Reserved15, IdCollision, NotOnboarded,
    DestNotWhitelisted, GasBudget,
}
```

`TxState` 的线上编码在 [hft-event](../hft-event/README.md);**迁移规则与证据要求归本 crate**(实现说明 §3)。

### TxTracker:nonce 槽 → 候选交易集合

```rust
pub struct TxTracker { slots: FixedMap<(WalletId, u64 /*nonce*/), NonceSlot> }   // 容量 MAX_SLOTS = 64
pub struct NonceSlot {
    pub wallet: WalletId, pub nonce: u64,
    pub candidates: ArrayVec<Candidate, 8>,   // 原交易、同 nonce 提价替换、Cancel(builder 档:各变体)
    pub max_price_wei: u64,                   // 本槽已广播的最高 gasPrice(替换下限的依据)
    pub status: SlotStatus,                   // Open | Consumed{by: CandIdx, block, hash} | Released
}
pub struct Candidate { pub intent_id: u64, pub kind: IntentKind, pub tx_hash: B256, pub gas_price_wei: u64, pub state: TxState }
```

- 一个 nonce 槽从第一笔广播起存在,到"某个候选被终局包含"(`Consumed`)或"终局块号 > 全部候选的 `max_block` 且该 nonce 未被消耗"(`Released`,下一次 grant 复用该 nonce)为止。
- 槽内至多一个候选能上链;一个候选被包含后,其余候选一律 `Dropped(NONCE_SUPERSEDED)`(可立即判定)。

### 门控(cap)

与 hft-core 相同的 cap 模型([control-plane 优先级表](../../docs/subsystems/control-plane.md)):每个门控是独立的 cap 记录 `(source, scope, set_seq, params)`,`Resume{source, scope}` 只删完全匹配的那一条。

| source | scope | 设置 / 解除 | 强度 |
|---|---|---|---|
| `HftKill` | 账户 | hft-core 心跳的每账户 KillBits 位图;变化先写 chain journal Input 再生效(编码归 hft-event);只随心跳变化 | Kill 位 → 软;`HARD` → 硬 |
| `Ctl` | `all` \| 账户 \| 钱包 | `Pause{scope, hard}` / `Resume{Ctl, scope}` | 由 `hard` 决定 |
| `GasBudget` | 钱包 \| 路由 | 日 gas 上限、路由连续 revert 上限 / `Resume{GasBudget, scope}` 或日切 | 软 |
| `SourcesLow` | 全部 | 可用独立来源 < 2 持续 `sources_low_blocks` 块(B-27)/ 来源恢复后人工 `Resume` | 软 + P0 |
| `LinkDown` | hft-core 意图 | 会话 `Down`(条件型) | 软 |
| `Draining` | 进程 | `Drain`、SIGTERM;不可清除 | 软 |

放行规则:scope 内有任一硬 cap → 拒绝;否则有任一软 cap → 只放行 `risk_reducing` 的意图;拒绝码取挡住它的 cap(`GasBudget` → `GasBudget`,其余 `Paused`)。**`risk_reducing` 意图另有 gas 硬上限**(B-27:单笔 gasPrice、gas 与每日合计),会被超过时不发送、P0、转人工——始终生效。

### 控制命令(`ChainCtlCmd`)

角色与权限只在 [hft-ctl 的 chain-core 会话表](../hft-ctl/README.md#chain-core-的-ctl-会话)。每条命令先写 chain journal 再生效。

| 命令 | 语义 |
|---|---|
| `Pause{scope, hard}` | 设 `Ctl` cap |
| `Resume{source, scope}` | 只删完全匹配的 cap;`HftKill`、`Draining` 不可清除 |
| `ClearTaint{wallet}` | 人工核对后解除 `Tainted`;先重新校准 nonce |
| `SetEndpoint{name, enabled}` | 端点启停(免费端点改规则或下线时) |
| `SetBidCap{max_bribe}` | **仅可选的 builder profile**:改出价上限;free profile 收到 → `PreconditionFailed`(无出价) |
| `Drain` | 设 `Draining`,停收非 `risk_reducing` 意图,等在途槽有结论或超时,写快照后退出 |

路由不经 ctl 更改:路由表是 refdata `routes` 段,经 `ReplaceRefdata`(Drain + 重启)更换。ops_bot `/kill chain` = `Pause{all, hard = false}`;`/kill all` = hft-core `Kill{Global}` + 本进程 `Pause{all}`(第七轮裁决 R7-4),回执 `CmdResult.effect_count` = 在途意图数。

### 配置(`ChainConfig`,文件与解析归 apps/chain-core)

| 键 | 含义 | 初值 |
|---|---|---|
| `send.profile` | `free`(builder 为可选扩展) | `free` |
| `endpoints[]` | `{name, domain, http, ws?, roles, rps, burst, cooldown_s, cooldown_max_s, ban_threshold, ban_s, weight}` | 必填;**`receipt` 角色至少 3 个不同 `domain`**(启动校验) |
| `free.send_fanout` / `free.max_blocks` / `free.receipt_poll_ms` / `free.head_poll_ms` | 并发投递域数;链上过期块数;无 WS 时的轮询 | 3 / 3 / 150 / 200 |
| `evidence.domains_min` / `evidence.sources_low_blocks` | `Included*` 需要一致的独立域数;可用独立来源 < 2 持续多少块 → P0 | 2 / 20 |
| `gas.price_mult_bp` / `gas.price_floor_wei` / `gas.price_cap_wei` / `gas.limit_mult_bp` | gasPrice 与 gasLimit | 12,000 / 0.1 gwei / 5 gwei / 12,500 |
| `gas.daily_cap_wei` / `gas.revert_streak_max` | 普通意图的 gas 上限 | 必填 / 3 |
| `gas.rr_max_price_wei` / `gas.rr_max_gas` / `gas.rr_daily_cap_wei` | `risk_reducing` 意图的 gas 硬上限 | 必填(建议 1 gwei / 900,000 / 0.005 BNB,由老板定) |
| `nonce.replace_bump_bp` / `nonce.cancel_stuck_after_s` | 同 nonce 替换提价;卡单清理 | 1,250 / 60 |
| `wallets[]` | 执行钱包与其 `SwapGuard` | 必填 |

## 实现说明

<details><summary>实现要点</summary>

### 1. 意图流程

1. **去重**:键 `(intent_id, body_hash)`;同键 → `Duplicate`,回当前状态;同 id 不同体 → `Rejected(IdCollision)` + P0,永不回放旧状态。写 `IntentRx`。
2. **校验**:门控 → `Paused`/`GasBudget`;路由在 refdata `routes` 段 → `BadRoute`(聚合器与只报价的 UniV4 路由一律 `BadRoute`);onboarding 完成且 Permit2 未过期 → `NotOnboarded`(交易路径上不临时授权);`risk_reducing` 的 gas 硬上限预估;卖出数量 vs 余额。
3. **nonce**:`grant` → 新槽或复用已 `Released` 的 nonce;复用曾广播过的 nonce 时 gasPrice ≥ `max_price_wei` × (1 + bump)。
4. **预检**:两个独立运营商的 `eth_call` + `eth_estimateGas`;不满足 `limit` → `MinOutUnmet`;revert → `SimRevert`;两来源结论不一致 → 取第三个,仍不一致按 `SimRevert`。只复核,不放宽界限;重试是 hft-core 的新意图(AIN、NAORIS、JCT 复盘)。
5. **签名与投递**:写 `BundleBuilt`(free `n_variants = 1`);同一字节并发投 `send_fanout` 个独立运营商。
6. **`Sent`** = 至少一个端点接受;全部投递都是传输错误或限速 → `Unknown`。`nonce too low` 只是查证线索(现有 `_send_raw` 的"前一节点超时、下一节点报 nonce too low"情形在这里由证据规则收敛)。
7. **重播**:`max_block` 之前对查不到本交易的端点重播同一字节;过了 `max_block` 不重播、不为同一意图提价加速——交给 `SwapGuard` 过期。

### 2. SwapGuard 外壳与授权

外层 `execute(Guard{maxBlock, tokenIn, amountIn, tokenOut, minOut, bribeTo = 0, bribe = 0, intentId}, router, inner)`,`msg.value = 0`;内层为路由器 calldata,收款人 = guard:UR 编码规则翻译自现有 `bsc_route.py` 的 `ur_buy_plan`/`ur_sell_plan`(V3 路径、Infinity `SETTLE`/`CL_SWAP_EXACT_IN_SINGLE`/`TAKE`、原生 BNB 包装、`CONTRACT_BALANCE`/`OPEN_DELTA` 常量,动态结构体前缀 32 B offset),V3 `exactInputSingle`/`exactInput` 与 V2 `swapExactTokensForTokens` 把收款人改为 guard;路由器路径另设 `deadline`(`maxBlock` 的预计时间戳 + 2 s);`gas` = 预检 × 1.25(不再用现有的固定 300k/500k/700k)。ExactOut 只对单池与纯 V3 多跳路由开放。授权在上币时完成:钱包 `approve(guard, cap)`、guard `approveRouter(token, router, amount)`、guard `approvePermit2(token, amount, expiration)`(到期前刷新);未授权的路由 `NotOnboarded`。

### 3. 证据规则(TxTracker)

证据来源按**运营商 = 封禁/限流域**计(B-27):同一家的多个 URL(如 `bsc-dataseed1..4.binance.org`)、同一限流账号或同一 CDN 后的端点只算一个域。

1. **提交响应不是链上证据**:HTTP 错误、超时、403/429/5xx、断连、`nonce too low` 只更新端点健康度或作为查证线索。
2. **`IncludedOk`**:≥ 2 个独立域的回执 `status == 1`、块号与块哈希一致;且 `Swapped.intentId` 匹配、`amountOut ≥ min_out`(ExactIn)或 = 目标量(ExactOut)、`amountIn ≤ max_in`、`Transfer` 与事件一致。日志不符 → `Unknown(LOG_MISMATCH)` + P1,不触发对冲。
3. **回执不一致**:一个域有、另一个查无(落后)→ 继续等;块哈希或 status 不同 → `Unknown`,向更多域取证,以终局块为准;矛盾计入两端点健康度。
4. **`IncludedReverted`**:≥ 2 个独立域 `status == 0` 且块哈希一致(失败交易上链,只花 gas)。
5. **`Dropped`**:终局块号 > `max_block`,≥ 2 个独立域查无本候选回执,且终局块上 `getTransactionCount ≤ nonce`;nonce 已被**本槽另一候选**消耗 → `Dropped(NONCE_SUPERSEDED)`;被**未知交易**消耗 → 钱包 `Tainted`。
6. **迟到上链**:`Dropped` 之后同一候选上链 → `SwapGuard` 令其 revert → `IncludedReverted{LATE_LANDING}`;绝不出现 `Dropped → IncludedOk`;重组回灌同理。
7. **有界 `Unknown`**:`max_block` 终局后规则 2–5 必有一条成立;例外是可用独立来源 < 2:持续 `sources_low_blocks` 块 → `SourcesLow` cap + P0 + 转人工,不推断任何交易的结果。
8. **`Finalized`**:终局 ≥ 块号、终局哈希一致;余额核对只看目标代币与预期方向(外部转入容忍);方向相反或短缺 → `BAL_MISMATCH` + 钱包 `Tainted` + P1。
9. **重组**:`IncludedOk → Unknown(REORG)` 后若收敛为 `Dropped`/`IncludedReverted`,hft-core 按 [hft-ledger 终局前重组](../hft-ledger/README.md#链上腿optionalfeature-chain) 回收 CEX 对冲。

### 4. nonce 与钱包

专用执行钱包,只由 chain-core 发交易(迁移时接管旧框架的执行钱包,私钥从旧系统配置中移除,见 [migration runbook](../../docs/runbooks/migration.md));每钱包 `Open` 槽 ≤ 1(多币触发时串行,吞吐约每数秒一笔)。启动校准:chain journal 恢复槽表,再向 ≥ 2 个独立域读 `latest` 计数;计数 > 预期且无已知候选消耗 → `Tainted`。`pending` 计数不参与。卡单:槽已 `Dropped` 但仍可从某端点查到且 `cancel_stuck_after_s` 内无新意图 → 发 `Cancel`(0 值自转账,按提价规则)。`Tainted` 只由"nonce 被未知交易消耗"或"终局余额方向相反/短缺"触发;外部转入不触发。资金进出只经 `Transfer`(目的地址白名单)。

### 5. gas 记账

每笔上链交易的 `gas_used × effectiveGasPrice` 写进 `TxLifecycle.gas_cost`;`IncludedReverted`(含 `LATE_LANDING`)、`Cancel`、授权、划转的 gas 计入发起它的 group 或账户(hft-ledger 记账),不计入仓位。`GasBook` 维护每钱包日合计、每路由连续 revert 与 `risk_reducing` 日合计。

### 6. chain journal

与 seq journal 同格式;写者 `ChainSeq`;每条输入先记录再处理,预检结果作为输入记录,回放不需要端点。cold tier 用 [hft-journal](../hft-journal/README.md) 的离线写者(普通文件 + 周期 `fdatasync`)。记录 kind 与布局归 [event-model](../../docs/subsystems/event-model.md#kind-编号段) 与 hft-event:`IntentRx`/`IntentVerdict`、`SimResult`、`BundleBuilt`/`BuilderSubmit`(每端点一条提交结果)、`Evidence`(含端点健康度)、`NonceEvent`/`WalletCalib`、`TxLifecycle`、`ChainHead`、`ControlCmd`、`ConfigBlob`、KillBits 位图变化、`BuilderStat`(按端点)。签名后的原始交易字节不入 journal。快照 `ChainState`:槽表(含候选与 `max_price_wei`)、`IntentTable` 非终结项与去重键、`Onboarding`、`Routes` 版本、`GasBook`、cap 集合、端点健康度。

### 7. 源文件

| 文件 | 职责 |
|---|---|
| `src/seq.rs`、`src/intent.rs`、`src/gate.rs`、`src/ctl.rs` | 主状态机、去重与校验、cap 与放行、控制命令 |
| `src/tracker.rs`、`src/slot.rs`、`src/nonce.rs`、`src/wallet.rs`、`src/gas.rs` | 证据规则、nonce 槽与候选、分配与校准、Taint、gas |
| `src/profile/{mod.rs, free.rs}` | profile 抽象与 free 实现 |
| `src/endpoints.rs` | 端点表、域、令牌桶、冷却、降权 |
| `src/calldata/{guard.rs, ur.rs, v3router.rs, v2router.rs}`、`src/routes.rs`、`src/onboarding.rs` | 编码、路由、授权 |
| `src/quote.rs` | 每块 Multicall3 报价批次与 `PoolQuote` 生成 |
| `src/journal.rs` | chain journal 记录 |

### 8. 不变量

- 同一 `(intent_id, body_hash)` 至多一个候选组;同 id 不同体永不执行。
- 每钱包 `Open` 槽 ≤ 1;同一槽至多一个候选被包含;复用 nonce 的候选 gasPrice ≥ 槽内最高 × (1 + bump)。
- `limit_wei` 与 `max_block` 在意图生命周期内不变,原样写进 `SwapGuard` 调用。
- `Sent` 只在至少一个端点接受后出现;传输错误不产生除 `Unknown` 以外的迁移;`Included*` 需要 ≥ 2 个独立域一致。
- 交易路径上不发起授权;不存在不经 `SwapGuard` 的 swap;`risk_reducing` 的 gas 硬上限始终生效。

</details>

## 延迟与资源契约

cold tier,无主机内延迟门槛;分段与报告口径见 [latency-budget §4](../../docs/latency-budget.md)。`ChainSeq::on_input` 是同步纯状态机(便于回放与属性测试),端点 I/O 在 tokio 任务里。报告项:预检往返、`chain_decide`、`chain_include_blocks`、`chain_include_seen`(现状:0.5 s 轮询、中位 1.3 s 看到成交)、证据 → `TxLifecycle` 出、revert 率与 gas 合计、端点健康度;包含速度与被抢跑只报告,不设目标。资源预算在 [apps/chain-core](../../apps/chain-core/README.md)。

## 测试与验收

| 层 | 方法 | 内容 |
|---|---|---|
| 属性(生命周期) | `proptest-state-machine` | 端点限速/封禁/超时、`nonce too low`、各域回执(一致/查无/矛盾/重复/乱序)、重组、终局停摆、公共 mempool 滞留后迟到上链、同 nonce 替换竞争、可用独立来源 < 2;断言实现说明 §3 全部规则与 §8 不变量 |
| 属性(nonce 槽) | 同上 | 槽内至多一个候选被包含;其余 `NONCE_SUPERSEDED`;`Released` 后复用;外部消耗 → `Tainted`;尘埃转入 → 无标记 |
| 门控 | 单元 | KillBits 位图变化先入 journal;软 cap 只放行 `risk_reducing`;硬 cap 全拒;`risk_reducing` gas 硬上限 → 不发送 + P0 |
| 故障注入 | 本地 JSON-RPC 代理夹在端点前 | 403/429/超时/断连、某域落后、两域给出不同块哈希、投递"假成功"、WS 断开;每种都断言:状态只由证据改变、无重复 swap、无反向动作 |
| 本地分叉 | `anvil --fork-url <公共 RPC> --fork-block-number <N>` + 部署 `SwapGuard` | 每种路由(V2、V3、routed、Infinity 原生 BNB)真实兑换;**迟到上链**:关闭自动出块,持有已签名交易到 `maxBlock` 之后再打包 → revert;同 nonce 替换竞争;重组(`anvil_reorg`)回灌后迟到上链 |
| 测试网 | BSC testnet(chainId 97) | 端到端 `Intent → IncludedOk → Finalized`;独立来源配置 |
| 事故回放 | 单元 | 9/24:发送端点 403 + 回执稍后在另一域出现 → 最终 `IncludedOk`;"前一节点超时、下一节点报 nonce too low" → 查证后 `IncludedOk`,不重发;ARC 型 → `LOG_MISMATCH`;AIN/NAORIS/JCT 型 → 不放宽、不重试;"余额没变按没成交"形态 → 保持 `Unknown` 直到 `max_block` 终局,不重试 |
| DST | [hft-testkit](../hft-testkit/README.md) | 链上故障集 × `ChainSeq` |

**迁移验收**(与 [apps/chain-core](../../apps/chain-core/README.md#测试与验收) 一起):属性测试、故障注入、本地分叉、测试网全绿;Shadow 运行期间(只预检不发送)链上意图判定与同期旧引擎日志离线对照;迁移切换后的正式实盘中传输错误被当作失败 0 次、重复 swap 0 次、过期后成交 0 次、单源回执当成交 0 次、未解释的 `Unknown`/`Tainted` 0 次。

## 依赖

| 依赖 | 用途 |
|---|---|
| [hft-event](../hft-event/README.md)、[hft-types](../hft-types/README.md)、[hft-refdata](../hft-refdata/README.md)、[hft-journal](../hft-journal/README.md)(离线写者)、[hft-ext-proto](../hft-ext-proto/README.md) | 事件体、定点、`routes` 段、chain journal、协议帧 |
| `tokio`、`alloy`(provider、sol-types、rlp、signer)、`secp256k1` | cold tier 的 I/O 与编码 |

禁止:`hft-sequencer`、任何 `strategies/*`、`hft-oms`;被任何 hot crate 依赖。[hft-amm](../hft-amm/README.md) 与 `revm` 只在可选扩展(L0 镜像、builder profile)中引入。

## 延伸阅读

- [06 链上执行层](../../design/06-链上执行层.md)、[research/03](../../design/research/03-onchain-bsc-solana.md) §1.5–1.7、§5
- [external-venue-protocol](../../docs/subsystems/external-venue-protocol.md)、[contracts/](../../contracts/README.md)、[apps/chain-core](../../apps/chain-core/README.md)
- [Agent Note:交易链上过期](../../.agents/notes/proposed/architecture/2026-10-01-tx-onchain-expiry.md)、[Agent Note:chain-core free 档](../../.agents/notes/proposed/architecture/2026-10-01-chain-core-free-profile.md)
- [hft-ledger 终局前重组](../hft-ledger/README.md#链上腿optionalfeature-chain)
- 事故复盘:`E:\trading\analysis\DEX加速与AIN插针复盘.md`,以及同目录下 2026-09-25、09-26、09-29 的旧框架正式账户复盘、策略优化与资金利用率复盘

## 已知限制与后续工作

- 不改变包含速度与被抢跑;`SwapGuard` 的 `minOut` 只把被抢跑后的坏成交变成 revert(花 gas)。builder profile 为可选扩展(触发条件见上)。
- 每账户 1 个钱包,吞吐受串行限制;多钱包轮转是可选扩展(触发条件:钱包串行排队造成可观的错过机会,需要 treasury 的再平衡支持)。
- 免费端点随时改限额或下线(`rpc.ankr.com/bsc` 已改为需要 key);端点表靠配置维护,失效端点自动降权。
- 可选扩展:L0 `PoolMirror` 与本地报价([hft-amm](../hft-amm/README.md))——触发条件:免费 WS 能可靠提供目标池 `logs`,或公共 RPC 报价请求成为限流瓶颈。

### 开发备忘

<details><summary>给实现者</summary>

**链上事实口径**:已核实——BSC 出块 0.45 s、快速终局 ≈ 1.125 s(Fermi,2026-01-14)、最低 gasPrice 0.05 gwei。待核实:免费端点的限额、域划分与封禁规则(只读探测脚本按配置速率跑 24 h);免费 WS(`wss://bsc-rpc.publicnode.com`,现有引擎已用其 `newHeads`)的稳定性与是否提供 `logs`;`finalized` 头在各公共端点上的查询方式;BSC testnet 上 PancakeSwap V3/Infinity 部署是否齐全(不齐全的路由只在本地分叉测)。

**初始端点(来自旧框架的链上引擎配置)**:`bsc-dataseed1/2.binance.org`(同一域)、`bsc-dataseed1.defibit.io`、`bsc-dataseed1.ninicoin.io`、NodeReal 免费档、publicnode WS;`rpc.ankr.com/bsc` 现需 key,不列入。

**已知地址初值**(写进 refdata 路由,接入前用链上 `code` 核对):V2 Router `0x10ED43C718714eb63d5aA57B78B54704E256024E`、V3 SwapRouter `0x1b81D678ffb9C0263b24A97847620C99d213eB14`、Infinity Universal Router `0xd9C500DfF816a1Da21A48A732d3498Bf09dc9AEB`、Permit2 `0x31c2F6fcFf4F8759b3Bd5Bf0e1084A055615c768`、WBNB `0xbb4CdB9CBd36B01bD1cBaEBF2De08d9173bc095c`、USDT `0x55d398326f99059fF775485246999027B3197955`。

**实现顺序**(按依赖):`ChainSeq` 纯状态机 + 属性测试 → 端点集合与块头 → `SwapGuard` calldata + 预检 → free profile 投递与回执 → 门控、ctl、gas → 与 hft-core 会话 → 故障注入、分叉、测试网 → Shadow → 迁移切换(正式实盘)。

</details>
