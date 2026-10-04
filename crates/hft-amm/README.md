---
description: "PancakeSwap V2/V3/Infinity 的纯定价数学、池日志解码、多跳路由与价格换算;chain-core 的 L0 报价、hft-sim 的链上腿、Python 研究共用同一份代码"
kind: crate
tier: pure
scope: optional
status: frozen
---

# hft-amm

## 概要

> **冻结**:不随 core 规格同步;解冻(触发条件同原文)时先按 core 规格复核。已知待复核:无(`architecture-v2/_work/review-2026-10-03-M0c-V3一致性.md` §2 中没有只涉及本组件的行)。

> **可选扩展:CEX-DEX 策略从旧框架迁移时启用。** 新框架首要服务 CEX-CEX 策略,链上策略继续在旧框架运行、迁移不急;本规格保留,启用前按届时的实际情况复核。下文中的"core""迁移范围内"指该迁移启用之后的范围;外部 venue 协议与链上事件编号段在架构上保留。

`hft-amm` 拥有 BSC 上我们交易的三类池子的**闭式定价数学**(PancakeSwap V2 常数积、V3 集中流动性、Infinity CL 无 hook 池)、池子日志到状态的纯函数、多跳路由的报价,以及从链上数量到 [`hft-types`](../hft-types/README.md) 定点**价格**的换算(数量本身的 U256 ↔ `Qty` 换算归 hft-types)。它不做 I/O、不持有池子集合、不知道区块从哪来:池子镜像的维护、drift 检测与重建属于 [`hft-chain-evm`](../hft-chain-evm/README.md) 的 `PoolMirror`,calldata 编码(Universal Router 命令、自有合约)也在那里。调用方是 chain-core 的 `chain-seq` 线程(L0 报价,[06 §2.3](../../design/06-链上执行层.md))、[`hft-sim`](../hft-sim/README.md) 的链上腿模拟、[`hft-py`](../hft-py/README.md) 暴露给研究与 `python/pipeline`。它不在 hft-core 的 tick-to-trade 路径上,但在 chain-core 的 `chain_decide` 分段里,每个公开入口都有预算。

**可选扩展**:迁移范围内的链上腿是 chain-core 的 free profile,定价与预检都用链上 `eth_call`(Quoter / 路由模拟),不需要本 crate;`python/pipeline` 的池子核验(pool_finder、深度与冲击探测、桥接费率档选择)同样用链上 Quoter 的 `eth_call` 完成,也不依赖本 crate。值得做的触发条件:① 公共 RPC 的报价请求量成为限流瓶颈(需要本地闭式报价减少 `eth_call`);② 免费 WS 能可靠提供目标池 `logs`、要开 L0 `PoolMirror`;③ 需要完整 [hft-sim](../hft-sim/README.md) 的链上腿模拟。

本 crate 是现有 Python `traders/bsc_route.py`、`utils/bsc_price.py`、`utils/dex_price.py`、`monitoring/pancake_infinity.py` 中**定价部分**的 Rust 翻译,并把那里修过的坑(V4 指数、双重换算、默认费率、默认精度、报不出就退回中间价)做成类型或测试上不可能再犯的形式,见 [实现说明 §6](#实现说明)。

## 目录

- [使用本组件](#使用本组件)
- [实现说明](#实现说明)
- [延迟与资源契约](#延迟与资源契约)
- [测试与验收](#测试与验收)
- [依赖](#依赖)
- [延伸阅读](#延伸阅读)
- [已知限制与后续工作](#已知限制与后续工作)

## 使用本组件

### 基本类型

```rust
pub use alloy_primitives::{Address, U256, B256};
pub type U512 = ruint::Uint<512, 8>;          // 只用于 mul_div 中间积

#[repr(transparent)] pub struct TokenIdx(pub u16);   // 本 crate 调用方维护的代币表下标
#[repr(transparent)] pub struct PoolIdx(pub u16);    // 池子表下标(PoolMirror 分配)
#[repr(transparent)] pub struct RouteId(pub u32);    // refdata `routes` 段下标(= PoolQuote.route = SwapDecision.route;路由表由 pipeline 生成)

pub struct TokenMeta {
    pub addr: Address,
    pub decimals: u8,          // 来自链上 decimals(),由 pipeline 写入 refdata;缺失 = 配置加载失败,永不默认 18
    pub asset: u32,            // hft-refdata AssetMeta 下标;决定 px_exp/qty_exp
    pub flags: TokenFlags,     // NATIVE(Infinity 零地址 BNB)、FEE_ON_TRANSFER(禁止交易)、REBASING(禁止交易)
}

pub enum PoolKind { V2, V3, InfinityCl }        // InfinityBin 未支持,见"已知限制"

pub enum PoolClass {                             // 由 classify() 给出,决定能否闭式定价
    Closed,                                      // V2/V3;或 Infinity 无 hook 且静态费率
    ClosedWhitelisted,                           // Infinity 有 hook,但未注册任何 swap 路径回调、静态费率,且在白名单
    SimOnly,                                     // Infinity 注册了 beforeSwap/afterSwap/*ReturnsDelta 或动态费率,且在白名单:L0 只给指示性中间价,成交价只认 L1
    Forbidden,                                   // 有 hook 且不在白名单:不报价、不交易
}
```

### 池子状态

```rust
pub struct V2State {
    pub reserve0: u128,        // uint112 实际范围
    pub reserve1: u128,
    pub fee_ppm: u32,          // PancakeSwap V2 = 2_500(amountIn × 9975/10000);来自池子注册表,无默认
}

pub struct ClState {           // V3 与 Infinity CL 共用
    pub sqrt_price_x96: U256,  // uint160
    pub tick: i32,
    pub liquidity: u128,
    pub fee_ppm: u32,          // V3:池子 fee;Infinity:有效费率 = protocolFee ⊕ lpFee(见实现说明 §3)
    pub tick_spacing: i32,
    pub ticks: TickTable,      // 窗口内已初始化 tick(liquidityNet),升序
    pub window: TickWindow,    // 窗口 [lo_word, hi_word],按 bitmap 字(256 × tick_spacing)对齐
}

pub struct TickEntry { pub tick: i32, pub liquidity_net: i128 }
pub struct TickTable { /* 固定容量升序数组;容量构造时给定,插入超容量返回 AmmErr::TickTableFull */ }
pub struct TickWindow { pub lo_word: i16, pub hi_word: i16 }

pub struct InfinityPoolKey {   // 与链上 PoolKey 字段顺序一致
    pub currency0: Address, pub currency1: Address, pub hooks: Address, pub pool_manager: Address,
    pub fee: u32,              // uint24;0x800000 位 = 动态费率(待核实,见开发备忘)
    pub parameters: B256,      // 低 16 位 hook 注册位图;CL 池 16..40 位为 tickSpacing(待核实)
}
```

### 报价

```rust
pub enum SwapKind { ExactIn, ExactOut }

pub struct SwapReq {
    pub zero_for_one: bool,
    pub kind: SwapKind,
    pub amount: U256,                 // ExactIn: 输入量;ExactOut: 期望输出量
    pub sqrt_price_limit: Option<U256>, // None = MIN_SQRT_RATIO+1 / MAX_SQRT_RATIO-1
}

pub struct SwapRes {
    pub amount_in: U256, pub amount_out: U256, pub fee_paid: U256,
    pub sqrt_price_after: U256, pub tick_after: i32, pub liquidity_after: u128,
    pub ticks_crossed: u16,
}

#[repr(u8)] pub enum AmmErr {
    ZeroAmount, InsufficientLiquidity, // 流动性耗尽或触到价格上下限仍未成交完:不返回部分结果当作报价
    OutOfWindow,                       // 需要跨出已镜像的 tick 窗口
    TooManyTicks,                      // 超过调用方给的 max_ticks
    Overflow, BadState, NotClosedForm, // NotClosedForm:SimOnly/Forbidden 池
    TickTableFull, BadRoute,
}

pub fn v2_swap(s: &V2State, req: &SwapReq) -> Result<SwapRes, AmmErr>;
pub fn cl_swap(s: &ClState, req: &SwapReq, max_ticks: u16) -> Result<SwapRes, AmmErr>;
pub fn classify(kind: PoolKind, key: Option<&InfinityPoolKey>, wl: &HookWhitelist) -> PoolClass;
pub fn infinity_pool_id(key: &InfinityPoolKey) -> B256;   // keccak256(abi.encode(key)),6 个 32 B 字
```

`cl_swap`/`v2_swap` 是纯函数,**不修改** `s`;要推进状态用下面的 `apply_*`。

### 日志 → 状态

```rust
pub enum PoolEvent {
    V2Sync { reserve0: u128, reserve1: u128 },
    ClSwap { sqrt_price_x96: U256, liquidity: u128, tick: i32 },
    ClModify { tick_lower: i32, tick_upper: i32, liquidity_delta: i128 },   // V3 Mint/Burn 与 Infinity ModifyLiquidity 归一
    InfinityFee { lp_fee_ppm: u32 },                                        // Infinity Swap 事件自带当次 fee,静态池用于校验
}
pub struct LogRef<'a> { pub address: Address, pub topics: &'a [B256], pub data: &'a [u8] }

pub fn decode_log(kind: PoolKind, log: &LogRef, want_pool_id: Option<&B256>) -> Option<PoolEvent>;
pub fn apply(s: &mut PoolState, ev: &PoolEvent) -> Result<(), AmmErr>;   // PoolState = enum { V2(V2State), Cl(ClState) }
```

事件签名常量(topic0)在 `events.rs` 以编译期常量给出,值在实现时由 keccak 计算并用金样核对,见开发备忘。

### 路由

```rust
pub const MAX_HOPS: usize = 3;

pub struct Hop { pub pool: PoolIdx, pub zero_for_one: bool }
pub enum WrapStep { None, UnwrapBeforeHop(u8), WrapAfterHop(u8) }  // 原生 BNB 计价的 Infinity 池:路由里 WBNB⇄BNB
pub struct Route {
    pub id: RouteId,
    pub hops: [Hop; MAX_HOPS], pub n_hops: u8,
    pub token_in: TokenIdx, pub token_out: TokenIdx,
    pub via: Option<TokenIdx>,         // 中转币:WBNB / ETH / BTCB(BNB 计价池的中转币也是 WBNB)
    pub wrap: WrapStep,
    pub quote_asset: TokenIdx,         // 该路由报价的计价端;路由起点是 USDT 时 = USDT,换算表不参与
}

pub trait PoolView {                   // PoolMirror 与 hft-sim 各自实现
    fn state(&self, p: PoolIdx) -> Option<&PoolState>;
    fn class(&self, p: PoolIdx) -> PoolClass;
}

pub struct RouteQuote {
    pub amount_in: U256, pub amount_out: U256,
    pub per_hop: [SwapRes; MAX_HOPS], pub n_hops: u8,
    pub closed_form: bool,             // 任一跳 SimOnly → false(只可作指示,不可定 min_out)
}

pub fn quote_route<V: PoolView>(r: &Route, v: &V, kind: SwapKind, amount: U256, max_ticks: u16)
    -> Result<RouteQuote, AmmErr>;
```

ExactIn 按跳正向链式计算(上一跳 `amount_out` 是下一跳 `amount`),ExactOut 按跳反向;**不**用各跳中间价相乘。

### 与定点的换算

数量的 U256 ↔ `Qty` 换算归 [`hft-types`](../hft-types/README.md)(`qty_from_u256`、`qty_to_u256`、`RoundMode`),本 crate 直接调用,只提供 `alloy_primitives::U256` ↔ `hft_types::U256Limbs` 的零成本转换。本 crate 拥有的是**价格**换算——它们依赖池子数学与两种代币的精度:

```rust
use hft_types::{Px, Qty, RoundMode};

/// 每枚 base 值多少 quote:(quote_wei / 10^qd) ÷ (base_wei / 10^bd),按 px_exp 取整;U512 中间积
pub fn px_from_amounts(quote_wei: U256, quote_dec: u8, base_wei: U256, base_dec: u8, px_exp: i8, m: RoundMode)
    -> Result<Px, ConvErr>;
/// CL 池中间价(只作指示):token 是 currency0 还是 currency1 各一条整数公式,见实现说明 §6
pub fn mid_px_from_sqrt(sqrt_price_x96: U256, token_is_c0: bool, token_dec: u8, quote_dec: u8, px_exp: i8)
    -> Result<Px, ConvErr>;
/// 非 USDT 计价 → USDT:只在 Route.quote_asset != USDT 时由调用方调用一次;rate = quote 资产的 USDT 价
pub fn convert_px(px_in_quote: Px, rate: Px, px_exp: i8, rate_exp: i8, m: RoundMode) -> Result<Px, ConvErr>;

#[repr(u8)] pub enum ConvErr { Overflow, Zero, BadExp }
```

`Px`、`Qty` 的含义(值 = raw × 10^exp,exp 属于资产)以 hft-types 为准。取整规则是契约的一部分:

| 用途 | 函数 | 取整 |
|---|---|---|
| 报价得到的输出量 → Qty | `hft_types::qty_from_u256` | `Floor`(不高估拿到的) |
| 报价需要的输入量 → Qty | `hft_types::qty_from_u256` | `Ceil`(不低估要付的) |
| 买价(每枚 USDT) | `px_from_amounts` | `Ceil` |
| 卖价 | `px_from_amounts` | `Floor` |
| `min_out` / `max_in` Qty → wei | `hft_types::qty_to_u256` | 精确(refdata 保证链上资产 `qty_exp ≥ −decimals`,否则加载失败),界限不会被取整放松 |

### 配置(调用方提供,本 crate 不读文件)

| 名称 | 类型 | 含义 | 来源 |
|---|---|---|---|
| `fee_ppm` | u32 | 每池费率;V2 = 2500,V3 = 池子 fee 档(100/500/2500/10000) | `python/pipeline` 写入 refdata 路由表,链上核对后才落盘 |
| `tick_spacing` | i32 | V3 由 fee 档决定(1/10/50/200);Infinity 从 `parameters` 解出 | 同上 |
| `HookWhitelist` | `[(Address, HookClass); ≤ 64]` | 允许的 hook 与其分级 | pipeline 生成,人工审批;经 `ConfigBlob` 进 chain journal |
| `max_ticks` | u16 | 单次报价最多跨越的已初始化 tick 数;默认值由 chain-core 配置给出 | chain-core `[amm]` |
| `window_words` | i16 | 每个 V3/Infinity 池镜像的 bitmap 字半径 | chain-core `[amm]` |

## 实现说明

<details><summary>实现要点</summary>

### 1. PancakeSwap V2

1. `amount_in_with_fee = amount_in × (1_000_000 − fee_ppm)`;`out = amount_in_with_fee × reserve_out / (reserve_in × 1_000_000 + amount_in_with_fee)`,全部 U256,整除向下。与 Pancake V2 Router `getAmountOut`(9975/10000)对 fee_ppm = 2500 逐位一致。
2. ExactOut:`in = reserve_in × out × 1_000_000 / ((reserve_out − out) × (1_000_000 − fee_ppm)) + 1`;`out ≥ reserve_out` → `InsufficientLiquidity`。
3. 状态只由 `Sync(reserve0, reserve1)` 推进(Swap/Mint/Burn 都会紧跟 `Sync`),不从 Swap 数量推算。

### 2. PancakeSwap V3(Uniswap V3 数学的逐位移植)

模块与 Uniswap v3-core 同名同语义,整数宽度与取整方向逐函数照搬:

| 模块 | 内容 |
|---|---|
| `full_math` | `mul_div`、`mul_div_rounding_up`(U512 中间积) |
| `tick_math` | `get_sqrt_ratio_at_tick`、`get_tick_at_sqrt_ratio`、`MIN_TICK/MAX_TICK`、`MIN_SQRT_RATIO/MAX_SQRT_RATIO` |
| `sqrt_price_math` | `get_next_sqrt_price_from_input/output`、`get_amount0_delta`、`get_amount1_delta`(有/无符号两版) |
| `swap_math` | `compute_swap_step(sqrt_cur, sqrt_target, liquidity, amount_remaining, fee_pips)` |
| `liquidity_math` | `add_delta` |
| `tick_walk` | `next_initialized_tick_within_one_word` 的**语义等价实现**(见下) |

swap 循环(`cl_swap`):

1. 校验 `amount != 0`、价格上限方向合法;`amount_specified` 按 ExactIn 为正、ExactOut 为负(与合约同号约定)。
2. 循环直到剩余量为 0 或价格到达上限:
   1. `(tick_next, initialized) = tick_walk::next(state.tick, tick_spacing, zero_for_one, &ticks, window)`;tick_next 所在字超出 `window` → `OutOfWindow`。
   2. `tick_next` 夹到 `[MIN_TICK, MAX_TICK]`;`sqrt_next = get_sqrt_ratio_at_tick(tick_next)`;目标价 = 向上限方向不越过 `sqrt_price_limit`。
   3. `compute_swap_step` 得到本步 `amount_in/amount_out/fee`;按 ExactIn/ExactOut 扣减剩余量(与合约同样的 `amountSpecifiedRemaining` 更新)。
   4. 到达 `sqrt_next` 且 `initialized`:`liquidity = add_delta(liquidity, ±liquidity_net)`,`ticks_crossed += 1`,超过 `max_ticks` → `TooManyTicks`;更新 `tick`(zero_for_one 时 `tick_next − 1`)。
   5. 未到达:`tick = get_tick_at_sqrt_ratio(sqrt_price)`。
3. 剩余量 ≠ 0 且价格已到上下限 → `InsufficientLiquidity`(**不**返回部分量冒充报价;现有代码 9/24 ROBO 事故:报不出时退回中间价,把"卖不出"伪装成正常卖价)。

`tick_walk` 不存 bitmap 本身,只存窗口内已初始化 tick 的升序数组;对给定 `(tick, spacing, lte)` 先算压缩 tick 与所在 256 位字的边界,再在数组里二分找同一字内最近的已初始化 tick,找不到则返回字边界且 `initialized=false`。这样**保留了合约按字边界分步的行为**——跨空字时 `compute_swap_step` 在字边界多切一步,取整会与一步到位不同,必须与链上一致。

Pancake V3 与 Uniswap V3 在交易者支付的费率与价格数学上相同;差异在协议费分配与 `Swap` 事件多两个 `protocolFeesToken0/1` 字段(topic0 不同,见开发备忘),这只影响 `decode_log`。

### 3. Infinity CL(无 hook 与白名单 hook)

- 价格数学与 V3 相同(单例 `CLPoolManager` 内的 `Pool.swap`,同一套 tick 数学);状态按 `PoolId` 存在管理合约里,日志 `Swap(id, sender, amount0, amount1, sqrtPriceX96, liquidity, tick, fee, protocolFee)` 与 `ModifyLiquidity(id, sender, tickLower, tickUpper, liquidityDelta, salt)` 按 `topics[1] == pool_id` 过滤。
- 有效费率:`fee_ppm = protocol_fee + lp_fee − protocol_fee × lp_fee / 1_000_000`(Uniswap v4 `ProtocolFeeLibrary.calculateSwapFee` 的公式;Pancake 是否一致待核实)。`protocol_fee` 按方向分 0→1 / 1→0 两个 12 位值,状态里存两份。
- `classify` 规则(按顺序,第一条命中即返回):
  1. 非 Infinity → `Closed`。
  2. `hooks == 0x0` 且 `fee` 无动态位 → `Closed`。
  3. hook 不在白名单 → `Forbidden`。
  4. `fee` 带动态位,或 `parameters` 的 hook 位图含 `beforeSwap`、`afterSwap`、`beforeSwapReturnsDelta`、`afterSwapReturnsDelta` 任一 → `SimOnly`(动态费由 `beforeSwap` 决定且可能与 sender 相关,不可闭式,[research/03 §1.7](../../design/research/03-onchain-bsc-solana.md))。
  5. 否则 → `ClosedWhitelisted`。
- `SimOnly` 池:`cl_swap` 返回 `NotClosedForm`;调用方可用 `mid_px_from_sqrt` 得指示价(写进 `PoolQuote` 时置"指示"标志,见 [external-venue-protocol](../../docs/subsystems/external-venue-protocol.md)),`min_out` 与成交判断只认 L1。
- `infinity_pool_id`:`keccak256(currency0 ‖ currency1 ‖ hooks ‖ pool_manager ‖ uint256(fee) ‖ parameters)`,每项左补零到 32 B(与现有 `compute_v4_pool_id` 一致)。

### 4. 多跳路由(翻译 `traders/bsc_route.py`)

现有规则,原样保留为路由表的约束(由 pipeline 生成、chain-core 加载时校验,违反即拒绝加载):

| 规则 | 来源 |
|---|---|
| 钱包只持 USDT;非 USDT 计价池经一跳中转,买 = USDT → 中转 → 目标,卖反向,一笔交易完成,不持有中转币 | `bsc_route.py` 头注释 |
| 中转币只允许 WBNB、BNB(原生,中转币仍是 WBNB,路由里 wrap/unwrap)、ETH、BTCB;要求有深 USDT V3 池(2026-09-24 实测 WBNB fee100 $9.7M、ETH fee500 $1.5M、BTCB fee500 $11.5M) | `ROUTE_VIA` |
| USDT⇄中转那一跳的费率档从 {100, 500, 2500} 中选,以 $30 真实报价拿到最多中转币者为准 | `best_route_fee` |
| V3 目标池:一条 V3 path(USDT‖fee_in‖via‖fee_pool‖token) | `encode_path` |
| Infinity 目标池:V3 一跳 + Infinity 一跳(UR 一笔 execute;编码在 hft-chain-evm) | `ur_buy_plan`/`ur_sell_plan` |
| 老的 V2 `use_wbnb_routing`:USDT → WBNB → token 的 V2 路径,结果已是 USDT | `bsc_trader.get_quote` |

- 本 crate 提供 `select_bridge_fee(pools, via, probe_usdt) -> Option<u32>`(给 pipeline 经 hft-py 调用)。**路由上的费率档是路由表的固定字段**,报价与执行走同一条路由;chain-core 不在热路径上重新选档(现有 10 分钟缓存 + 实时选档会让报价路径与成交路径不一致)。
- 往返额外成本线(现 `route_cost_pct = fee/1e4 × 2 + 0.1%`)不在本 crate:手续费已精确含在逐跳报价里,`+0.1%` 余量是策略参数,归链上策略包的成本线。

### 5. 预分配与容量

| 结构 | 容量 | 说明 |
|---|---|---|
| `TickTable` | 构造时给定,chain-core 默认按 `window_words` 估算(每池 ≤ 512 个已初始化 tick) | 插入超容量 → `TickTableFull`,由 `PoolMirror` 标记该池降级并告警 |
| `RouteQuote.per_hop` | `[SwapRes; 3]` 定长 | 无分配 |
| `HookWhitelist` | ≤ 64 项,线性查找 | 构造于冷路径 |

报价路径零分配、无 `panic`(溢出一律返回 `Overflow`),`Result` 用 `#[repr(u8)]` 错误枚举。

### 6. 已修正的坑 → 本 crate 的结构性防护

| 现有事故/修复 | 在 Rust 中如何做到不可再犯 |
|---|---|
| **V4 指数**(9/25):`get_bsc_v4_price` 在 token 为 currency1 时写成 `1/(ratio × 10^(td−bd))`,8 位精度的 KGEN 差 `10^(2(td−bd))` 倍 | `mid_px_from_sqrt` 两种情形各有一条整数公式:token 为 c0 时 `px_raw = sqrtP² × 10^(td−qd−px_exp) / 2^192`;token 为 c1 时 `px_raw = 2^192 × 10^(td−qd−px_exp) / sqrtP²`(指数为负时移到另一侧乘);不经浮点。属性测试:对 (td, qd) ∈ {6,8,9,18}² 两种朝向,中间价与 1e-6 级小额 ExactIn 报价的均价偏差 < 1 bp;金样含 KGEN |
| **双重换算**(9/25,`instant_trade` 与路由报价):路由报价已是 USDT,又乘了一次 BNB 汇率 | `Route.quote_asset` 在加载时确定;`convert_px` 只在 `quote_asset != USDT` 时由调用方调用一次,且接受的是 `(Px, quote 资产)` 而不是"已换算与否"的布尔。属性测试:起点为 USDT 的路由,换算表任意取值,输出不变 |
| **默认费率**:监控列表缺费率档时按 V2/默认 fee 问价,AIN 等 V3 币差几百倍 | `fee_ppm` 无默认;路由表加载时缺字段即失败([AGENTS 错误配置在加载时失败](../../AGENTS.md)) |
| **默认精度 18** | `TokenMeta.decimals` 必填;pipeline 从合约读取后写入 |
| **报不出就退回中间价**(9/24 ROBO:流动性全在现价上方,卖 100 个 quoter 失败) | `InsufficientLiquidity`/`OutOfWindow`/`NotClosedForm` 都是错误,不存在"退回 mid"的 API;`mid_px_from_sqrt` 的结果类型与成交报价不同(调用方只能用于指示字段) |
| **稳定币/BNB 汇率兜底**:取不到汇率时稳定币按 1.0、BNB 返回 None | 汇率来源与陈旧判定属于 chain-core(汇率来自 hft-core 的 `BboSnapshot`);本 crate 只做确定性换算,不做兜底 |
| USDC 计价 V3 池被 pipeline 拒绝(9/25 已知缺口) | 路由模型支持任意 `quote_asset`;是否启用由 pipeline 决定 |

### 7. 源文件

| 文件 | 职责 |
|---|---|
| `src/lib.rs` | 公开 API 重导出 |
| `src/types.rs` | `TokenMeta`、`PoolKind`、`PoolClass`、`V2State`、`ClState`、`TickTable`、`InfinityPoolKey`、`SwapReq/SwapRes`、`AmmErr`、`ConvErr` |
| `src/full_math.rs`、`tick_math.rs`、`sqrt_price_math.rs`、`swap_math.rs`、`liquidity_math.rs` | V3 数学逐位移植 |
| `src/tick_walk.rs` | 窗口内已初始化 tick 的按字行走 |
| `src/v2.rs`、`src/cl.rs` | `v2_swap`、`cl_swap` |
| `src/infinity.rs` | `classify`、`infinity_pool_id`、参数位解析、有效费率 |
| `src/events.rs` | topic0 常量、`decode_log`、`apply` |
| `src/route.rs` | `Route`、`PoolView`、`quote_route`、`select_bridge_fee` |
| `src/conv.rs` | `px_from_amounts`、`mid_px_from_sqrt`、`convert_px`、`U256` ↔ `U256Limbs` |

### 8. 不变量

- 同一 `(state, req)` 输出逐位确定(无浮点、无全局状态)。
- ExactIn 的 `amount_out` ≤ 链上实际;ExactOut 的 `amount_in` ≥ 链上实际(取整方向与合约一致,因此相等)。
- `apply` 后的 `ClState.tick` 与 `get_tick_at_sqrt_ratio(sqrt_price_x96)` 一致(V3 合约不变量),否则返回 `BadState`。

</details>

## 延迟与资源契约

纯函数,零分配、零系统调用。预算在参考机(c7i/i7ie 同代 Intel,Criterion 中位数与 p99)上测,是 chain-core L0 报价 < 50 µs 的组成部分([06 §2.3](../../design/06-链上执行层.md)、[latency-budget §4](../../docs/latency-budget.md))。

| 入口 | p50 | p99 | 基准名 |
|---|---|---|---|
| `v2_swap` | ≤ 150 ns | ≤ 300 ns | `amm/v2_exact_in` |
| `cl_swap`,不跨 tick | ≤ 500 ns | ≤ 1 µs | `amm/cl_exact_in_0x` |
| `cl_swap`,跨 8 个已初始化 tick | ≤ 4 µs | ≤ 8 µs | `amm/cl_exact_in_8x` |
| `quote_route`,2 跳 V3+V3 或 V3+Infinity,各 ≤ 2 次跨 tick | ≤ 3 µs | ≤ 8 µs | `amm/route_2hop` |
| 一条路由的完整 L0 报价(买/卖 × 2 个规模 = 4 次 `quote_route`) | ≤ 12 µs | ≤ 30 µs | `amm/route_l0_full` |
| `mid_px_from_sqrt` / `px_from_amounts` | ≤ 200 ns | ≤ 400 ns | `amm/conv_px` |
| `apply(ClModify)`(插入/删除 tick) | ≤ 300 ns | ≤ 1 µs | `amm/apply_modify` |

内存:每个 CL 池 `ClState` ≈ 200 B + `TickTable` 容量 × 24 B(512 → 12 KiB);1,024 个池 < 13 MiB。

## 测试与验收

| 层 | 内容 | 门槛 |
|---|---|---|
| 单元 | `tick_math` 两端与 ±1、每个 fee 档的 tick_spacing、`compute_swap_step` 的四种组合(方向 × Exact 类型)、V2 公式与 Router `getAmountOut` 常数对照 | 每次提交 |
| 差分(属性) | proptest 生成随机 `ClState`(liquidity、tick、窗口内 0–64 个已初始化 tick)与随机 `SwapReq`,与 Uniswap v3-core 参考合约在 revm 中执行结果比对(revm 只在 `[dev-dependencies]`) | **逐位相等**;每次提交 5,000 例,每夜 200,000 例 |
| 金样 | `fixtures/amm/bsc/<pool>/<block>.json`:从本机节点在块 N 抓取的完整池状态 + 同块 V3 `QuoterV2` / Infinity `CLQuoter` / V2 `getAmountsOut` 的结果;覆盖 V2、V3 四个费率档、Infinity CL 无 hook、token 为 c0 与 c1、decimals ∈ {6, 8, 9, 18}(含 KGEN 8 位)、WBNB/BNB/ETH/BTCB 中转路由各一 | ≥ 50 个池;报价逐位相等;中间价与小额报价偏差 < 1 bp(对应 [11 §3](../../design/11-实施路线图.md) 的"与 Quoter 偏差 < 1 bp",本 crate 的门槛更严) |
| 失败路径 | 流动性耗尽(ROBO 形态:流动性全在现价上方)→ `InsufficientLiquidity`;跨出窗口 → `OutOfWindow`;SimOnly/Forbidden 池 → `NotClosedForm`;fee-on-transfer 代币进路由 → 加载失败;费率档/精度缺失 → 加载失败;`TickTable` 满 → `TickTableFull` | 每次提交 |
| 回归(已修的坑) | V4 指数两种朝向 × 四种精度;路由报价 × 任意汇率表不变(双重换算);取整方向表逐行断言(含对 hft-types 数量换算的调用) | 每次提交 |
| fuzz | `fuzz/amm_cl_swap`(任意字节 → 状态 + 请求,断言不 panic 且不变量成立)、`fuzz/amm_decode_log` | 每夜 1 h;任何 panic 阻断 |
| 基准 | 上表全部基准,回归阈值 10%(`cargo xtask bench-gate`) | 每次提交 |

## 依赖

| 依赖 | 用途 | 约束 |
|---|---|---|
| [`hft-types`](../hft-types/README.md) | `Px`、`Qty`、`RoundMode`、`qty_from_u256`/`qty_to_u256` | — |
| `alloy-primitives` | `U256`、`Address`、`B256`、`keccak256` | 只用无 I/O 部分;版本与 hft-chain-evm 的 alloy 对齐 |
| `ruint` | `U512` 中间积 | alloy 已间接依赖,锁同一版本 |
| dev:`revm`、`proptest`、`criterion` | 差分测试与基准 | 只在 `[dev-dependencies]` |

禁止:任何 I/O、线程、`std::time`、`f64` 参与报价与换算(只允许在 `hft-py` 暴露的展示函数里把结果转成 float);禁止依赖 `hft-chain-evm`、`hft-refdata`(代币与路由元数据由调用方传入,避免 L1 横向依赖;字段语义以 hft-refdata 为准)。

## 延伸阅读

- [06 §2.3 L0 池状态](../../design/06-链上执行层.md)、[research/03 §1.7 本地定价与 Infinity](../../design/research/03-onchain-bsc-solana.md)
- [13 R4 按资产定点](../../design/13-设计评审与优化.md)、[hft-types](../hft-types/README.md)
- [hft-chain-evm](../hft-chain-evm/README.md)(`PoolMirror`、calldata、L1)、[hft-sim](../hft-sim/README.md)、[hft-py](../hft-py/README.md)
- 现有实现:`E:\trading\analysis\strat0926\orig5\traders\bsc_route.py`、`utils\bsc_price.py`、`utils\dex_price.py`、`monitoring\pancake_infinity.py`;核对脚本 `E:\trading\analysis\verify_bsc_prices.py`

## 已知限制与后续工作

- **Infinity Bin(LBAMM)池不支持**:现有交易配置只用 CL;需要时新增 `PoolKind::InfinityBin` 与 bin walk 数学(由 pipeline 发现 Bin 池的交易需求触发)。
- **Uniswap V4(BSC)与 1inch/OpenOcean 聚合器路径不支持**:现有配置里 `use_univ4_swap` 只报价不成交、聚合器路径无法本地定价;这些币在新栈里不进 L0,只能由 pipeline 迁到 Pancake 池或放弃。
- fee-on-transfer / rebasing 代币直接拒绝;若出现值得做的此类币,需要在 L1 层按余额差定价。
- 多于 3 跳的路由不支持(现有逻辑最多 2 跳)。
- Solana 各 AMM 的数学不在本 crate,见 [hft-chain-solana](../hft-chain-solana/README.md)。

### 开发备忘

<details><summary>给实现者</summary>

**待核实**(核实方法:读对应合约源码 + 用本机节点在真实池子上做金样):

1. Pancake V3 `Swap` 事件签名是否为 `Swap(address,address,int256,int256,uint160,uint128,int24,uint128,uint128)`(比 Uniswap 多两个协议费字段)——对照 `pancakeswap/pancake-v3-contracts` 的 `IPancakeV3PoolEvents`,并用一笔真实 Swap 日志的 topic0 核对。
2. Infinity `parameters` 的位布局(低 16 位 hook 注册位图、CL 的 tickSpacing 在 16..40 位)与 hook 位序号(beforeSwap/afterSwap/*ReturnsDelta 各是第几位)——读 `pancakeswap/infinity-core` 的 `CLPoolParametersHelper`、`Hooks.sol`。
3. Infinity 动态费率标志是否为 `fee & 0x800000`、协议费合成公式是否与 Uniswap v4 `calculateSwapFee` 一致——读 `LPFeeLibrary`、`ProtocolFeeLibrary`。
4. Infinity `Swap`、`ModifyLiquidity` 事件的字段顺序与 indexed 位——读 `ICLPoolManager`,用真实日志核对。
5. 现有配置里 `hook_data = bytes(20)`(`_build_v4_swap_payload`)的来由:某些池的 hook 需要 20 字节 hookData;确认白名单 hook 各自的 hookData 要求,写进 `HookWhitelist` 的每项(属于 calldata 侧,记录在 hft-chain-evm)。

**实现顺序建议**:`full_math`/`tick_math` + 差分测试骨架 → V2 → `cl_swap`(先 V3)→ 金样抓取工具(放 `tools/`,从本机节点导出池状态与 Quoter 结果)→ Infinity → 路由 → 换算与回归测试。

**开放问题**:`window_words` 默认值取决于 06 §9.5 的实测(每块能覆盖多少池);先按 ±8 字、每池 512 tick 实现,chain-core 上线后按 `OutOfWindow` 计数调整。

</details>
