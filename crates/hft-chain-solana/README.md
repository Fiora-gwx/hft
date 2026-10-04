---
description: "chain-core 的 Solana 适配:LaserStream 账户流、本地报价器(Raydium CLMM/CPMM、Orca Whirlpool、Meteora DLMM、PumpSwap)、Helius Sender + Jito 双路发送、确认状态机;可选的法兰克福中继"
kind: crate
tier: hot
scope: optional
status: frozen
---

# hft-chain-solana

> **可选扩展,冻结为草稿**:只在 CEX-DEX 策略从旧框架迁移之后才可能启用。值得做的触发条件:Solana 腿的机会与 PnL 证明值得付费数据服务(Helius LaserStream 等)。开工前按届时的 hft-chain-evm 实际形态、Alpenglow 状态与各待核实项重新评审,再改为正式规格。

## 概要

> **冻结**:不随 core 规格同步;解冻(触发条件同原文)时先按 core 规格复核。已知待复核:无(`architecture-v2/_work/review-2026-10-03-M0c-V3一致性.md` §2 中没有只涉及本组件的行)。

`hft-chain-solana` 让 chain-core 以同一个外部 venue 身份执行 Solana 腿(CEX-DEX 的 D/E 类,[06 §5](../../design/06-链上执行层.md)):它订阅 Helius LaserStream 上持仓与名单币的池子账户,在本地用各 AMM 的精确数学报价(替代 Jupiter 轮询),构造并签名交易,经 Helius Sender(东京,SWQoS)与 Jito 东京 block engine 双路发送,再按 Solana 的确认语义驱动与 BSC 相同的 `TxState` 生命周期。它拥有 Solana 侧的池账户镜像、blockhash 与有效期、在途签名表、发送路径统计;它不拥有策略(在 hft-core)、仓位(在 hft-ledger)、协议与会话([external-venue-protocol](../../docs/subsystems/external-venue-protocol.md))。结构与 [`hft-chain-evm`](../hft-chain-evm/README.md) 对称(单写者 `sol-seq` + I/O 线程 + chain journal),本页只写 Solana 特有的部分,相同的机制链接过去。东京部署为基准形态;法兰克福中继是另一个可选扩展(见实现说明 §7)。

## 目录

- [使用本组件](#使用本组件)
- [实现说明](#实现说明)
- [延迟与资源契约](#延迟与资源契约)
- [测试与验收](#测试与验收)
- [依赖](#依赖)
- [延伸阅读](#延伸阅读)
- [已知限制与后续工作](#已知限制与后续工作)

## 使用本组件

### 线程模型(由 [apps/chain-core](../../apps/chain-core/README.md) 的 `chain-core@solana` 实例装配;线程名以该 README 为准)

| 线程 | tier | 做什么 | 独占状态 |
|---|---|---|---|
| `sol-seq` | hot(隔离核,忙等) | 取输入环(link > 证据 > 账户更新 > slot > 定时器);写 chain journal;维护 `AccountMirror`、`SigTracker`、`BlockhashBook`;报价、校验意图、构造交易、签名(ed25519 ≈ 20 µs,不另开线程) | 上述全部;chain journal 唯一写者 |
| `sol-ingest` | warm | 读 [`chain-grpc-relay`](../../apps/chain-grpc-relay/README.md) 转发的 LaserStream 帧(账户更新、slot、本钱包相关交易;relay 断流时收到 `SourceDown`)→ 输入环 | relay 环 |
| `sol-send` | hot(忙等) | Helius Sender 与 Jito 各 2–4 条预热的 HTTP/1.1 keep-alive(TLS)连接(JSON-RPC,不需 tokio);发送与在有效期内重播同一签名交易 | 连接 |
| `sol-receipt` | warm | 两家 RPC 的 `getSignatureStatuses`/`getTransaction`、`getLatestBlockhash`、Jito `tip_floor` → 输入环 | 连接 |

LaserStream 是 Yellowstone gRPC;chain-core 依赖闭包里不得有 tokio([apps/AGENTS.md](../../apps/AGENTS.md)),所以 gRPC 由 [`chain-grpc-relay`](../../apps/chain-grpc-relay/README.md)(唯一允许 tokio 的链上侧进程)承接,经 `hft-rt::ShmSpsc`(同机)或 UDP 交给 `sol-ingest`。LaserStream 断流时 `sol-ingest` 收到 `SourceDown`:受影响池子的报价置 `STALE`,在途签名的确认改由两家 RPC 的 `getSignatureStatuses` 继续(证据规则不变)。

### 公开类型

```rust
#[repr(transparent)] pub struct SolPoolIdx(pub u16);
pub enum SolPoolKind { RaydiumClmm, RaydiumCpmm, OrcaWhirlpool, MeteoraDlmm, PumpSwap }

pub struct AccountMirror { /* 池账户、tick/bin 数组、金库余额、配置账户;按 Pubkey 索引,容量 MAX_ACCOUNTS = 4096 */ }
impl AccountMirror {
    pub fn on_account(&mut self, u: &AccountUpdate) -> Result<AffectedRoutes, DecodeErr>;   // 按 (slot, write_version) 单调
    pub fn quote(&self, route: RouteId, exact: SwapExact, amount: u64, now_unix: i64) -> Result<SolQuote, QuoteErr>;
}

pub mod quote {                                  // 纯函数,无 I/O;hft-sim/hft-py 以 default-features = false 使用
    pub fn clmm_swap(s: &ClmmState, arrays: &TickArrays, req: &SwapReq) -> Result<SwapRes, QuoteErr>;     // Raydium CLMM
    pub fn cpmm_swap(s: &CpmmState, req: &SwapReq) -> Result<SwapRes, QuoteErr>;                          // Raydium CPMM
    pub fn whirlpool_swap(s: &WhirlpoolState, arrays: &TickArrays, req: &SwapReq) -> Result<SwapRes, QuoteErr>;
    pub fn dlmm_swap(s: &LbPairState, bins: &BinArrays, req: &SwapReq, now_unix: i64) -> Result<SwapRes, QuoteErr>;  // 可变费率依赖时间
    pub fn pumpswap_swap(s: &PumpState, req: &SwapReq) -> Result<SwapRes, QuoteErr>;
}

pub struct SigTracker { /* 在途签名表,容量 64;状态迁移规则见实现说明 §5 */ }
pub struct BlockhashBook { /* 最近 300 个 (slot, blockhash, last_valid_block_height) */ }
#[repr(u8)] pub enum SendPath { HeliusSender = 0, Jito = 1, Both = 2 }
```

数量是 SPL 的 `u64` 最小单位;数量换算用 [`hft-types`](../hft-types/README.md) 的 `qty_from_u256`/`qty_to_u256`,价格换算用 [`hft-amm`](../hft-amm/README.md) 的 `px_from_amounts`(`U256::from(u64)`),口径与 BSC 相同。

### 事件

与 BSC 相同:向 hft-core 输出 `ChainHead`(`block` = slot)、`PoolQuote`(`block` = 状态所属 slot)、`TxLifecycle`;接收 `SwapIntent`(`SwapIntentMsg`)、`BboSnapshot`(SOL/USDC/USDT 汇率与 tip 换算)。Solana 签名 64 B:前 32 B 在 `TxLifecycle.tx_hash`、后 32 B 在 `TxLifecycle.sig_hi`([event-model](../../docs/subsystems/event-model.md#关键事件体));完整签名另在 chain journal。

### 配置(`/etc/hft/chain-core-sol.toml`;资金相关无默认)

| 名称 | 类型 | 含义 | 来源 |
|---|---|---|---|
| `relay.ring` | path | LaserStream relay 环 | 部署 |
| `rpc[2]` | url | 状态与回执源(Helius + Triton/QuickNode) | research/03 §2.3 |
| `sender.helius_url` / `sender.jito_url` | url | 东京端点 | research/03 §2.4 |
| `send.path_default` | `SendPath` | 默认 `Both`(同一签名双路) | 06 §5.1 |
| `tip.jito_pctl_lo` / `tip.jito_pctl_hi` | u8 | tip 取 `tip_floor` 该分位区间内随机 | 06 §5.1(75–95) |
| `tip.sender_min_lamports` / `tip.max_lamports` | u64 | Sender 最低 tip(SWQoS-only 5,000;Max 1,000,000)与上限 | research/03 §2.4 |
| `tx.cu_limit[route]` | u32 | 每路由计算单元上限(历史 p99 × 1.2,pipeline 写入) | — |
| `tx.cu_price_micro` | u64 | 优先费(micro-lamports/CU);SWQoS 下影响可忽略,取小值 | research/03 §2.4 |
| `tx.blockhash_min_remaining` / `tx.blockhash_max_remaining` | u16 | 选用的 blockhash 剩余有效块数区间,决定 UNKNOWN 上界 | 实现说明 §4;初值 40 / 90 |
| `tx.rebroadcast_ms` | u16 | 有效期内同一签名重播间隔 | 初值 200 |
| `confirm.sources_min` | u8 | 触发 `IncludedOk` 所需一致来源数 | **默认 2**;A/B 数据后可降为 1(需 Agent Note) |
| `wallets[]` | {wallet, account, pubkey, key_ref} | 执行钱包与 ATA | 部署 |

## 实现说明

<details><summary>实现要点</summary>

### 1. 账户流与镜像

- LaserStream 订阅:持仓/名单路由涉及的池账户、tick array/bin array(当前价两侧各 3 个)、金库 token 账户、AMM 配置账户;另订阅 slot 与本钱包签名的交易(`processed` 承诺级)。
- `AccountMirror` 按 `(slot, write_version)` 单调应用;解码器按账户 owner program + discriminator 分派,**布局版本写进 refdata**(research/03 §2.6:Raydium CLMM 2026-08 布局在第 390 字节新增 `fee_on`);未知 discriminator 或长度 → `DecodeErr`,该池降级,不猜。
- 价格移出已订阅的 tick/bin 数组 → 动态增订并在补齐前置 `STALE`。
- 没有 Geyser 流时忙池状态滞后 1–2 s 是报价误差主因(research/03 §2.6);本设计只接受流驱动的状态,不以轮询补洞。

### 2. 报价器

| AMM | 状态 | 数学 | 要点 |
|---|---|---|---|
| Raydium CLMM | `PoolState`(`sqrt_price_x64`、`tick_current`、`liquidity`、`tick_spacing`)、`AmmConfig`(`trade_fee_rate`/1e6)、`TickArrayState`(每数组 60 tick)、bitmap 与扩展账户 | Q64.64 集中流动性,逐 tick 步进(与 hft-amm 的 V3 结构同构,位宽为 u128) | `fee_on` 字段决定费用从输入还是输出扣(待核实) |
| Raydium CPMM | `PoolState` + 两个金库余额 − 累计协议/基金费 | 常数积,费率 `trade_fee_rate`/1e6 作用于输入 | 有效储备 = 金库余额 − 未提取费用 |
| Orca Whirlpool | `Whirlpool`(`sqrt_price` Q64.64、`tick_current_index`、`liquidity`、`fee_rate`/1e6)、`TickArray`(每数组 88 tick) | 同 CLMM | 是否启用自适应费率(待核实) |
| Meteora DLMM | `LbPair`(`active_id`、`bin_step`、基础费参数、`volatility_accumulator/reference`、过滤/衰减周期)、`BinArray`(每数组 70 bin,各含 x/y 数量) | bin 内恒和(x+y 按 bin 价),逐 bin 走;价格 = (1 + bin_step/1e4)^id(Q64.64) | 可变费率随波动累加器与**当前时间**变化:`now_unix` 取最新 slot 的区块时间,由事件提供,保证可回放 |
| PumpSwap | 池账户 + 金库余额、`GlobalConfig` 费率(LP、协议、创建者) | 常数积 | 费率结构待核实 |

每个报价器必须与链上 Swap 结果逐笔一致(金样);research/03 §2.6 提到一个 DLMM 实现 131/131 笔与链上一致,作为验收门槛的量级参考。

### 3. 意图 → 交易

1. 去重(按 `intent_id`)、校验(路由、池健康、`basis_block` 新鲜度按 slot、汇率新鲜)、换算界限(`min_out` 向上、`max_in` 向下取整)。
2. 本地报价复核:在最新镜像上报价不满足界限 → `Rejected(MinOutUnmet)`,不发送、不放宽(与 BSC 规则相同,[hft-chain-evm §1](../hft-chain-evm/README.md))。Solana 热路径不做远端 `simulateTransaction`(一次往返数十 ms);新路由上线前在冷路径模拟一次以确定 `cu_limit`。
3. 构造 v0 消息:`SetComputeUnitLimit`、`SetComputeUnitPrice`、1–2 个 swap 指令(直接调用 AMM 程序的精确输入/输出指令,内置 `min_out`/`max_in` 检查)、tip 转账;ATA 在钱包初始化时预建,热路径不创建账户。
4. blockhash 选取见 §4;签名一次(ed25519)。
5. 交给 `sol-send`:按 `send.path_default` 把**同一笔已签名交易**发往 Helius Sender 与 Jito(`sendBundle` 单笔或 `sendTransaction`,按核实结果)。**同一意图永远只有一个签名**:两条路径若要求互不兼容的 tip 指令,则该意图只走一条路径(按路径统计选),绝不签两笔不同的交易——那会导致同一 swap 执行两次。不使用 durable nonce(它会取消 blockhash 有效期,使 UNKNOWN 无界)。
6. 在有效期内每 `rebroadcast_ms` 重播同一签名(网络保证同一签名至多执行一次,research/03 §2.7),直到出现包含证据或过期。

### 4. 有效期与 UNKNOWN 上界

- 交易的有效期由所用 blockhash 的 `lastValidBlockHeight` 决定(最新 blockhash 约 150 块 ≈ 60 s)。为缩短"不知道"的时长,`BlockhashBook` 选用**剩余有效块数在 `[blockhash_min_remaining, blockhash_max_remaining]` 内的较旧 blockhash**(初值 40–90 块 ≈ 16–36 s);太短会在落地前过期,由路径统计的"过期未落地"比例调参。
- `DROPPED` 的唯一判据:**按 `confirmed` 承诺读到的块高**(`getBlockHeight(commitment=confirmed)`,终局判定时用 `finalized`)> `lastValidBlockHeight`,且签名在 ≥ 2 个来源(`searchTransactionHistory: true`)都查无状态。不得用 `processed` 块高判过期:processed 分叉可能超前,过早判 `DROPPED` 会让仍可能落地的交易被当成零成本失败。

### 5. 确认状态机(`SigTracker`)

```
 SENT ──(processed 且 meta.err == null,余额变化符合预期,≥ sources_min 个来源一致)──▶ INCLUDED_OK   → 触发对冲
   │ ──(processed/confirmed 且 meta.err != null)────────────────────────────────────▶ INCLUDED_REVERTED(只付费)
   │ ──(confirmed 块高 > lastValidBlockHeight 且 ≥ 2 源查无)────────────────────────────────▶ DROPPED
   └──(RPC 错误/429/超时,或 getSignatureStatuses 返回 null)──────────────────────────▶ UNKNOWN
 UNKNOWN ──(以上任一证据规则成立)──▶ 对应状态
 INCLUDED_OK ──(confirmed 复核通过)──▶ 保持;──(finalized)──▶ FINALIZED
 INCLUDED_OK ──(confirmed 前签名从各来源消失:processed 所在分叉被弃)──▶ UNKNOWN(REORG)
```

- 来源:LaserStream 本钱包交易更新(最快,带 `meta.err` 与前后 token 余额)、RPC1、RPC2 的 `getSignatureStatuses`/`getTransaction`。
- **`confirmationStatus` 与 `err` 是独立字段**:任何承诺级别下 `err != null` 都是失败交易;`null` 状态是"不知道/不在缓存",**永不当失败**(research/03 §2.7;ARC 幻影成交事故)。
- 成功还要核对余额:本钱包输出代币 ATA 的 `post − pre` ≥ `min_out`(ExactIn)或 = 目标量(ExactOut),输入 ATA 的减少 ≤ `max_in`;不符 → `UNKNOWN(LOG_MISMATCH)` 并告警。
- 库或 RPC 客户端抛出的异常只是传输错误(web3.js `confirmTransaction` 在 429/503 时未处理 rejection 的历史 bug,research/03 §2.7)。
- `gas_cost` = 基础费 + 优先费(SOL 资产);tip 记为出价(待 `TxLifecycle` 追加 `bribe` 字段)。

### 6. 并发与钱包

Solana 没有 nonce;并发上限由账户写锁与资金决定。每钱包在途上限 `max_inflight_per_wallet`(初值 2),同一池同方向同时只允许 1 笔在途(避免自己跟自己竞争写锁)。卖出只从持有该代币的钱包发起。

### 7. 可选扩展:法兰克福中继([11](../../design/11-实施路线图.md))

- 触发:Solana 腿 PnL 证明值得;东京发送的落地率不可接受(12 A9)。
- 形态:法兰克福裸金属 RPC 节点(24–32 核、768 GB–1 TB、双 NVMe、10 Gbps)+ DoubleZero Edge shred 订阅 + BAM preconfirmation(经 Helius/Triton)+ 直连 leader TPU(Yellowstone Jet,QUIC + SWQoS 通道)+ Jito Frankfurt/Amsterdam([06 §5.2](../../design/06-链上执行层.md))。
- 本 crate 以同一代码运行在法兰克福的 `chain-core@sol-fra` 实例:`AccountMirror` 由本地节点/shred 驱动(更新鲜);hft-core(东京)经外部 venue 协议的 UDP 载体跨 WAN 连接(WireGuard 隧道 + 帧 HMAC;重传超时下限调到 250 ms)。
- 法兰克福实例在**最新本地状态**上复核意图(只收紧不放宽),再发送;包含证据回传东京触发对冲(单程 ≈ 120 ms,06 §5.2 判为可接受)。`PoolQuote` 由法兰克福实例产生并发回东京。
- 直连 TPU 的 QUIC 客户端(Yellowstone Jet)依赖 tokio,作为 [`chain-grpc-relay`](../../apps/chain-grpc-relay/README.md) 的出站通道运行(可选),不进 chain-core。

### 8. 源文件

| 文件 | 职责 |
|---|---|
| `src/lib.rs`、`src/types.rs` | 公开 API 与类型 |
| `src/seq.rs` | `sol-seq` 主循环 |
| `src/mirror.rs`、`src/decode/{raydium_clmm.rs, raydium_cpmm.rs, whirlpool.rs, dlmm.rs, pumpswap.rs}` | 账户镜像与布局解码 |
| `src/quote/{clmm.rs, cpmm.rs, whirlpool.rs, dlmm.rs, pumpswap.rs, q64.rs}` | 纯报价数学 |
| `src/tx/{build.rs, blockhash.rs, tip.rs, sign.rs}` | 交易构造、有效期、tip、签名 |
| `src/tracker.rs` | `SigTracker` |
| `src/io/{ingest.rs, send.rs, receipt.rs}` | I/O 线程循环体(无 tokio) |
| `src/journal.rs`、`src/stats.rs` | chain journal 记录、路径统计 |

</details>

## 延迟与资源契约

Solana 分段在 [latency-budget §4](../../docs/latency-budget.md) 明确 Solana 分段不设门槛;下列为本 crate 的内部预算:

| 入口 / 分段 | p50 | p99 | 基准 / 测量 |
|---|---|---|---|
| `AccountMirror::on_account`(解码 + 应用) | ≤ 1 µs | ≤ 3 µs | `chain_sol/on_account` |
| 单路由报价(CLMM/Whirlpool 跨 ≤ 4 tick;DLMM 跨 ≤ 8 bin) | ≤ 5 µs | ≤ 15 µs | `chain_sol/quote_*` |
| 意图接收 → 两路写出(构造 + 签名 + 写) | ≤ 150 µs | ≤ 400 µs | 直方图 `sol_decide` |
| 证据到达 → `TxLifecycle` 出链接 | ≤ 10 µs | ≤ 30 µs | 直方图 |
| 触发对冲的证据时刻 − 发送 | ≈ 400 ms(processed) | — | 06 §5.1;每日报表 |

`sol-seq` 运行期零分配;账户缓冲预分配(4,096 × 最大账户长度)。

## 测试与验收

| 层 | 内容 |
|---|---|
| 金样 | 每种 AMM ≥ 20 个真实池 × 多个 slot:账户原始字节 + 同 slot 链上 Swap 的实际输入输出 → 报价逐单位相等;含布局版本切换前后的样本 |
| 属性 | `SigTracker`:任意来源交错(processed/confirmed/finalized、err/null、429、超时、分叉消失、过期)下:`null` 与传输错误只产生 `UNKNOWN`;`err != null` 永不 `INCLUDED_OK`;按 `confirmed` 块高未过期永不 `DROPPED`(processed 块高超前时也不);同一意图只有一个签名 |
| 事故回放 | ARC 型:`confirmed` 但 `err != null` → `INCLUDED_REVERTED`,不对冲;`getSignatureStatuses` 返回 null 且 RPC 429 → `UNKNOWN`,无反向动作 |
| 集成 | 测试钱包小额真实往返(东京):双路发送、重播、过期判定、余额核对 |
| DST | [`hft-testkit`](../hft-testkit/README.md) 的链上故障集(Solana 变体) |
| 验收(启用时) | 影子 → 正式实盘;报价与链上成交逐笔一致率 100%(金样)/ 实盘偏差中位 < 1 bp;传输错误只进 UNKNOWN;落地率与路径统计进每日报表(12 A9) |

## 依赖

| 依赖 | 用途 |
|---|---|
| [`hft-amm`](../hft-amm/README.md)(换算)、[`hft-rt`](../hft-rt/README.md)、[`hft-journal`](../hft-journal/README.md)、[`hft-event`](../hft-event/README.md)、[`hft-refdata`](../hft-refdata/README.md)、[`hft-net`](../hft-net/README.md) | 同 hft-chain-evm |
| `ed25519-dalek`、`solana` 消息编码的最小子集(`solana-program`/`solana-sdk` 中不依赖 tokio 的 crate,或自写 v0 消息编码) | 签名与交易格式 |
| `borsh`/`bytemuck` | 账户布局解码 |

禁止:`tokio`、`solana-client`、`yellowstone-grpc-client`、`tonic`(依赖 tokio,只能在 chain-grpc-relay 里)。

## 延伸阅读

- [06 §5 Solana](../../design/06-链上执行层.md)、[research/03 §2、§4.3、§5](../../design/research/03-onchain-bsc-solana.md)
- [12 A9 与事故对照](../../design/12-风险与未决问题.md)、[11 §5–6](../../design/11-实施路线图.md)
- [hft-chain-evm](../hft-chain-evm/README.md)(相同的单写者、journal、证据原则)、[external-venue-protocol](../../docs/subsystems/external-venue-protocol.md)

## 已知限制与后续工作

- **Alpenglow**(finality ≈ 150 ms)上主网后,"finalized 后对冲"可能可行;需重新评估 `confirm.sources_min` 与 `FINALIZED` 的用途(research/03 §2.1:主网激活窗口暂定 11/9,Anza 未承诺)。
- Raydium AMM v4(老 AMM)、Meteora DAMM、Pump.fun 曲线未支持;按名单需求逐个加报价器与金样。
- 多跳路由(USDC → SOL → token)限 2 跳、同一交易内完成。
- Jupiter 自托管只用于离线路由发现与校验(research/03 §2.6),不在本 crate。

### 开发备忘

<details><summary>给实现者</summary>

**待核实**(接入前用官方文档与测试钱包小额实测核对):

1. Helius Sender 的 tip 要求:tip 账户是 Jito 的 8 个还是 Helius 自有账户;`swqos_only=true` 的最低 tip;同一签名交易能否同时满足 Sender 与 Jito 的 tip 规则(决定 `SendPath::Both` 是否可行)。
2. Jito `sendBundle` 单笔 bundle 与 `sendTransaction?bundleOnly=true` 在 revert 保护上的差别;默认 5 RPS 限额是否需要申请提升。
3. LaserStream 与 Triton 客户端的 proto 兼容性(research/03 §2.3 标注一份评测称需 Helius SDK)。
4. Raydium CLMM `fee_on`(第 390 字节)的语义;Orca Whirlpool 是否启用自适应费率;PumpSwap 的费率组成与创建者费。
5. 选用较旧 blockhash 是否会被 Sender/Jito 拒绝或降低落地率。

**实现顺序建议**:报价器 + 金样(只需历史账户数据即可开始)→ 镜像与 chain-grpc-relay 接入 → `SigTracker` 属性测试 → 发送与重播 → 影子 → 正式实盘。

**开放问题**:同一 hft-core 同时连 BSC 与 Solana 两个 chain-core 实例时,外部 venue 协议用两个 `chan`(每链一个),`ext-<chain>` 线程各一个;是否合并成一个线程以省核,待 hft-core 线程计划定(可作为同一线程的两个 `ext:` 任务)。

</details>
