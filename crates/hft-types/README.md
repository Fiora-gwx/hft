---
description: "所有 crate 在表示价格、数量、名义、ID 与交易枚举时使用的唯一定点数与 newtype 定义;feed 解码、OMS 填帧、风控、记账、策略 SDK、回放、PyO3 共用"
kind: crate
tier: pure
scope: core
status: spec
---

# hft-types

## 概要

`hft-types` 拥有全系统的数值与标识符词汇:按资产指数的定点数 `Px`/`Qty`、跨资产名义 `Notional`、比率 `Rate`、逐字节十进制解析与格式化、向 tick/lot 取整、链上 U256 ↔ `Qty` 换算(optional 链上使用)、ID newtype(`InstId`、`VenueId`、`AccountId`、`GroupId`(含其构造规则)、`StrategyId`/`StrategyInstanceId`、`SlotId`…)、协议常量 `MAX_BBO_CONN_SLOTS` 与通用交易枚举(`Side`、`Tif`、`MarketKind`)与主机类 `HostClass`,以及跨 crate 共用的线上编码:`RunMode`/`KillLevel`/`CapSource` 的 `u8` 值、`KillBits`(32 位 kill 字的唯一布局)、`RejectBits`、`ClientOrderId` 编解码;另有 L1 之间共用的纯工具:两腿 lot 对齐的 `pair_lot`、对冲定价限值 `HedgePxLimits`、引擎单限价上限函数 `cap_cross_px` 与资金费结算时刻归一化 `funding_settle_norm_ns`。它不拥有任何表(指数与 tick/lot 的**取值**在 [hft-refdata](../hft-refdata/README.md)),不做 I/O,不含线程。它处在依赖图最底层,被每个 crate(含 [hft-strategy-sdk](../hft-strategy-sdk/README.md) 与 `strategies/*`)使用;热路径上的调用点是 feed 线程的字符串→定点、sequencer 内的乘除与取整、gw 线程的定点→字符串写帧与 kill 字判定。

## 目录

- [使用本组件](#使用本组件)
- [实现说明](#实现说明)
- [延迟与资源契约](#延迟与资源契约)
- [测试与验收](#测试与验收)
- [依赖](#依赖)
- [延伸阅读](#延伸阅读)
- [已知限制与后续工作](#已知限制与后续工作)

## 使用本组件

> 草案:本节的 `#[repr(C)]` 布局、位值与码值实现时以代码与编译期断言为准;码值与字段名全仓一致([布局规则](#布局规则全仓-reprc-结构适用))。

### 定点数:按资产指数

一个数值 = `raw × 10^exp`。**指数属于资产,不属于 venue**:同一资产(BTC、PEPE…)在所有 venue 用同一 `AssetExp`,跨所比较与相减零换算([13 R4](../../design/13-设计评审与优化.md))。`Px` 与 `Qty` 本身不携带指数(热路径不为此付字节与比较),指数由调用方从 refdata 取得并作为参数传入;把两个不同资产的 `Px` 相减是逻辑错误,由 group 内同资产约束避免。

```rust
#[repr(C)] #[derive(Copy, Clone, Eq, PartialEq, Debug, zerocopy::*)]
pub struct AssetExp { pub px_exp: i8, pub qty_exp: i8 }   // 2 B;取值范围 [-18, +6],由 hft-refdata 加载时校验

#[repr(transparent)] pub struct Px(pub i64);        // 报价币种/1 个资产单位,指数 px_exp;可为负(价差、中间量)
#[repr(transparent)] pub struct Qty(pub i64);       // 资产单位(不是合约张数、不是 1000PEPE 单位),指数 qty_exp;有符号(仓位)
#[repr(transparent)] pub struct Notional(pub i64);  // 固定指数 -6,单位 = 该 inst 报价资产(USDT/USDC…);范围 ±9.22e12
#[repr(transparent)] pub struct Rate(pub i64);      // 无量纲比率,固定指数 -9:1 bp = 100_000;费率、资金费率、偏离带、保证金率
impl Rate { pub const MAX: Rate = Rate(i64::MAX); }   // 哨兵:分母 ≤ 0 的比率(如保证金余额 ≤ 0 时的保证金率)
pub const MAX_RAW: i64 = (1 << 62) - 1;              // 4 611 686 018 427 387 903 ≈ 4.61e18;所有解析/构造结果的绝对值上限(含)
```

不变量(构造入口负责保证,运算依赖它):

1. `|Px.0|, |Qty.0| ≤ MAX_RAW`。因此 `Px ± Px`、`Qty ± Qty` 在 i64 内永不溢出(`2 × MAX_RAW < i64::MAX`);release 构建关闭 overflow-checks 也安全。
2. 任何 `Px × Qty` 在 i128 内做,`|积| < 2^124`,永不溢出;结果换算到 `Notional` 时按显式 `RoundMode` 取整,`|结果| > i64::MAX`(含恰为 `i64::MIN`)时**饱和**到 `±i64::MAX`(同号)并返回 `FxErr::OVERFLOW` 位(饱和值必然超过任何风控限额,风控据此拒绝)。
3. 定点值不经过 `f64`。本 crate 不提供 `to_f64`;显示用 `write_fixed`,研究侧的浮点列由 [hft-py](../hft-py/README.md) 在 Arrow 导出时生成([hot-path-rules](../../docs/hot-path-rules.md) 禁止 f64 出现在下单路径;策略的慢速部分 `Model` 允许 f64,见 [hft-strategy-sdk](../hft-strategy-sdk/README.md))。
4. **返回 `Px`/`Qty` 而没有错误通道的运算**(`mul_rate`、`round_to_tick`、`round_to_lot`、`cap_cross_px`)结果超出 `±MAX_RAW` 时饱和:`mul_rate` 取 `±MAX_RAW`,取到步长的运算取同号、绝对值不超过 `MAX_RAW` 的最近步长整数倍(此时取整方向让位于不变量 1)。`Notional::mul_rate` 与 `Px::ratio` 超出 i64 时饱和到 `±i64::MAX`。所有运算对任意 i64 输入(含违反不变量 1 的值)不 panic。
5. 指数参数的合法范围:资产指数(`AssetExp` 的两个字段)`[-18, 6]`;线格式指数(`parse_*`、`write_fixed` 的 `exp`)`[-30, 18]`(资产指数加 refdata 的合约平移 `[-12, 12]`)。越界 → `FxErr::EXP_RANGE`(`mul_qty` 返回 `(Notional(0), EXP_RANGE)`,`write_fixed` 返回 0 且不写)。

`Notional` 的币种是 inst 的报价资产;换算到 USDT 由 [hft-ledger](../hft-ledger/README.md) 用行情事件中的汇率完成。只允许 USD 稳定币报价(USDT/USDC/FDUSD/USD),其他报价资产在 refdata 加载时拒绝(见[已知限制](#已知限制与后续工作))。

### 运算

```rust
#[repr(u8)] pub enum RoundMode { Floor = 1, Ceil = 2, TowardZero = 3, HalfEven = 4 }

impl Px {
    pub fn mul_qty(self, q: Qty, e: AssetExp, m: RoundMode) -> (Notional, FxErr); // i128 积 → 指数 -6
    pub fn from_notional(n: Notional, q: Qty, e: AssetExp, m: RoundMode) -> Result<Px, FxErr>; // 均价 n / q;q=0 → DIV_ZERO;|结果| > MAX_RAW → OVERFLOW
    pub fn mul_rate(self, r: Rate, m: RoundMode) -> Px;          // px × (1e9 + r) / 1e9,i128 中间量,按 m 取整一次
    pub fn round_to_tick(self, tick: i64, m: RoundMode) -> Px;   // tick 为资产指数下的整数步长;tick ≤ 0 → Px(0)
    pub fn mid_floor(a: Px, b: Px) -> Px;                         // (a+b)/2 向 −∞ 取整;不溢出(不变量 1)
    pub fn ratio(self, base: Px) -> Rate;                         // (self-base)/base,HalfEven;base ≤ 0 → Rate(0)(无错误通道,调用方先保证 base > 0)
}
impl Qty {
    pub fn round_to_lot(self, lot: i64, m: RoundMode) -> Qty;    // lot ≤ 0 → Qty(0)
    pub fn split_lot(self, lot: i64) -> (Qty, Qty);               // (可下单部分 TowardZero, 余数),两者之和 = self、余数与 self 同号;lot ≤ 0 → (Qty(0), self)
}
/// 两腿 lot 对齐的第一腿取整单位(数量取整模块;引擎与策略共用这一份实现,规则归 hft-exec"两腿 lot 对齐"):
/// 第一腿量为 lot_f 的整数倍、且第二腿量 q × num / den 恰为 lot_h 整数倍的最小 q = lcm(lot_f, M / gcd(M, num)),M = den × lot_h。
/// 对冲比 1/1 时:一方整除另一方 → 较粗者,否则 lcm(lot_f, lot_h)。两个 lot 同一资产、同一 qty_exp;i128 中间量。
/// 任一参数 ≤ 0(num/den 为 0)或结果超过 MAX_RAW → None。
pub fn pair_lot(lot_f: i64, lot_h: i64, num: u32, den: u32) -> Option<Qty>;
impl Notional { pub fn mul_rate(self, r: Rate, m: RoundMode) -> Notional; }   // 费用 = 名义 × 费率
impl Side {
    pub const fn price_round(self) -> RoundMode;  // Buy → Floor,Sell → Ceil:限价向"不更差"方向取整
    pub const fn sign(self) -> i64;               // Buy +1,Sell -1
    pub const fn opposite(self) -> Side;
}
```

取整规则(调用方必须遵守,风控复核):

| 量 | 规则 | 理由 |
|---|---|---|
| 限价 | `side.price_round()`:买向下、卖向上取到 tick | 取整后价格永不比策略给出的更差 |
| 下单数量 | `TowardZero` 取到 lot;余数留给调用方 | 不超过策略/风控批准的量;对冲余数由 [hft-oms](../hft-oms/README.md) / [hft-exec](../hft-exec/README.md) 累计到下一笔 |
| 取整后名义 < `min_notional` | 调用方丢弃该意图并计数;**本 crate 不提供"向上凑到 lot/最小名义"的入口** | 交易所会拒;向上凑会超出批准量。需要凑整的执行算法(如对冲余数)由执行层显式累计后再下 |
| 两腿链接的第一腿数量 | `TowardZero` 取到 `pair_lot(lot_f, lot_h, num, den)` 的整数倍 | 第一腿全部成交时第二腿数量恰为其 lot 的整数倍,不留不可对冲的余数;切片规则与 `min_qty` 约束归 [hft-exec](../hft-exec/README.md#两腿-lot-对齐),`pair_lot` 的名义超过 `max_chunk_notional` 且两 lot 不能整除时 `SetParams` 被拒(`InvalidArgs{LotMismatch}`,[control-plane](../../docs/subsystems/control-plane.md#结果码)) |
| 费用、资金费 | `Notional::mul_rate`,支出用 `Ceil`(按绝对值)、收入用 `Floor` | 记账保守 |
| 均价 | `HalfEven` | 无偏;只用于显示与 PnL 分解 |

### 字符串 ↔ 定点(逐字节)

```rust
pub fn parse_exact(s: &[u8], exp: i8) -> Result<i64, FxErr>;              // px/qty:超精度非零位 → PRECISION
pub fn parse_round(s: &[u8], exp: i8, m: RoundMode) -> Result<i64, FxErr>; // 费率、资金费率、派生值
pub fn write_fixed(raw: i64, exp: i8, out: &mut [u8; 40]) -> usize;        // gw 写帧;不分配;返回写入字节数,exp 越界 → 0
```

`exp` 是**线格式指数**,不一定等于资产指数:venue 的价格若按 10^k 个资产单位报价(Binance `1000PEPEUSDT`)、数量按合约张数(OKX `ctVal`),调用方传入 `InstMeta::px_wire_exp()` / `qty_wire_exp()`(见 [hft-refdata](../hft-refdata/README.md#使用本组件)),解析结果直接就是资产单位的 raw,换算只是一次指数平移,精确无误差。

语法(ABNF 的等价描述;不在语法内的一律 `SYNTAX`):

```
number   = ["-"] 1*DIGIT ["." 1*DIGIT] [("e" / "E") ["+" / "-"] 1*3DIGIT]
```

1. 空串 → `EMPTY`(与 `SYNTAX` 区分,解码器把 OKX 等的 `""` 映射为"字段缺省")。`"+1"`、`".5"`、`"5."`、`"NaN"`、`"Infinity"`、`"null"`、空白、千分位 → `SYNTAX`。`"-0"`、`"-0.000"` → `0`。
2. 科学计数法**接受**(部分 venue 的 Java 端会输出 `1.0E-4`、`0E-8`):尾数按普通数解析,指数 ≤ 3 位,合并为单一十进制指数后再按下述规则落到 `exp`。
3. **有效数字** = 去掉前导零与末尾零之后的数字串(整数部末尾的零与小数尾零都只并入十进制指数,不累加进尾数);有效数字超过 19 位 → `parse_exact` 返回 `OVERFLOW`,`parse_round` 第 20 位起只参与取整。
4. **超精度**:比 `exp` 更细的小数位若全为 `0` 则接受(`"0.10000000"` 在 `exp=-4` 下合法);若有非零位,`parse_exact` 返回 `PRECISION`,**不截断**。理由:资产指数由 refdata 保证细于所有 inst 的 tick/lot,出现非零超精度位意味着 refdata 错配或交易所改了 tick,必须暴露(解码器置错误位、计数、丢弃该事件)。`parse_round` 的结果 = 该十进制值按 `m` 取到 `10^exp` 的整数倍(与任意精度参考逐值相同,含指数部很大或很小的输入)。
5. 结果 `|raw| > MAX_RAW` → `OVERFLOW`;尾数为 0 时任何指数部都得 0。
6. 不做本地化、不接受十六进制;JSON 数字与 JSON 字符串形式的数字都以原始字节传入(解码器用 `sonic-rs` 的原始数字/借用 `&str`,**禁止先转 f64**)。
7. 错误只报一个位,按以下先后判定:`exp` 越界 → `EXP_RANGE`;空串 → `EMPTY`;语法(含输入超过 40 字节)→ `SYNTAX`;第 3 条 → `OVERFLOW`;第 4 条 → `PRECISION`;第 5 条 → `OVERFLOW`。

真实 demo 字符串的固定期望与采样范围归 [TYP-049](TESTS.md#typ-049-core-venue-的真实价格与数量字符串解析结果固定);采样子集通过不代表六产品的完整覆盖。

`write_fixed` 输出最短十进制表示:无指数、去尾零、`|x|<1` 时带前导 `"0."`、负数带 `"-"`;raw 是 tick 整数倍时结果满足交易所小数位要求。接受任意 i64 的 `raw`;`exp ∈ [-30, 18]` 时最长 38 字节(符号 + 19 位 + 18 个尾零),40 字节缓冲总够。

### 链上 U256 ↔ Qty(optional:链上组件使用)

链上金额在进入 sequencer 前换算**一次**(chain-core 生成 `TxLifecycle`/`PoolQuote` 时),不依赖 `alloy`/`ruint`:入参是小端 64 位肢。只有 optional 的链上组件(链上策略接入时启用)调用;函数无依赖、留在本 crate。

```rust
pub type U256Limbs = [u64; 4];   // alloy_primitives::U256::as_limbs() 的布局
pub fn qty_from_u256(v: &U256Limbs, token_decimals: u8, qty_exp: i8, m: RoundMode) -> Result<Qty, FxErr>;
pub fn qty_to_u256(q: Qty, token_decimals: u8, qty_exp: i8) -> Result<U256Limbs, FxErr>; // q<0 → NEGATIVE
```

- 令 `d = token_decimals + qty_exp`,合法范围 `[-18, 36]`(`qty_exp` 须在 `[-18, 6]`),越界 → `EXP_RANGE`。`d ≥ 0`:`raw = v / 10^d`,余数按 `m` 处理;`d < 0`:`raw = v × 10^(-d)`(检查溢出)。结果 > `MAX_RAW` → `OVERFLOW`。`qty_to_u256` 在 `d < 0` 时 `q` 须被 `10^(-d)` 整除,否则 `PRECISION`(不取整)。
- 方向约定:**收到的量**(`amount_out`、余额)用 `Floor`——永不高估所得;**付出的量**(`amount_in`、gas 以原生币计)用 `Ceil`——永不低估所花。`min_out` 从 `Qty` 转回 U256 精确相乘,无取整。

### ID 与枚举

所有 ID 是 `#[repr(transparent)]` newtype,**0 保留为"无/全部"**,有效值从 1 开始;稠密 ID 直接作数组下标(下标 0 空置一格)。**`GroupId` 例外**:0 只表示"无",命令作用域的"全部"另用 `u32::MAX`;有效值只能按下表的构造规则得到(因此落在 `0x0100_0001..=0xFEFF_FFFE` 内)。具名常量:

```rust
pub const INST_NONE: InstId = InstId(0);              // 无 inst(如仅余额推送的 FundingSettle、未订阅的符号)
pub const INST_ALL: InstId = InstId(0);               // 作用域与公告里的"全部 inst"(CmdScope.inst、VenueNotice.inst、SDK Notice);与 INST_NONE 同值,只用于作用域字段
pub const ASSET_NONE: AssetId = AssetId(0);           // 无资产(如回报未带费用资产时的 fee_asset)
pub const GROUP_NONE: GroupId = GroupId(0);           // 无 group:不针对 group 的载荷、unassigned 库存/外来成交、Timer.group 不适用
pub const GROUP_ALL: GroupId = GroupId(u32::MAX);     // 命令与 cap 作用域里的 group "*"(CmdScope.group、CapRecord.scope.group);不出现在其他字段
pub const ACCOUNT_ALL: AccountId = AccountId(0);      // 命令作用域"*"
```

ID 常量只有本节列出的这些;不另设 `InstId::NONE`、`AssetId::NONE` 之类的关联常量。

| 类型 | 底层 | 分配者 | 稳定性 |
|---|---|---|---|
| `InstId` | u32 | refdata(`hft-cfg`),稠密递增 | 一经分配永久绑定 `(venue, 线格式符号)`,不复用;下架置状态不删号 |
| `AssetId` | u16 | refdata | 永久绑定资产名与 `AssetExp` |
| `VenueId` | u8 | 本 crate 常量表(下) | 编号永不复用;新增 venue 追加 |
| `AccountId` | u8 | refdata `AccountMeta` | 永久;一个账户是一个逻辑账户,可跨交易所,但**在每个交易所只用一个市场**(现货或永续,refdata 加载校验 `SameExchangeMarkets`);实盘或模拟由 `AccountMeta.kind` 区分 |
| `StrategyId` = `StrategyInstanceId` | u8 | refdata `Instances` 段(`hft-cfg`) | **策略实例** id:永久绑定 `(kind_id, 实例名)`,不复用;journal、快照、定时器 owner、`AccountMeta.bound_instance` 都用它 |
| `GroupId` | u32 | `StrategyParams` 的 `GroupSpec`;**构造** `GroupId = (StrategyId << 24) \| n`,`n` 是该策略实例内的计数器(`1..=0xFF_FFFE`);分配者是 pipeline 与 `hft-cfg`(人工 TOML),二者用同一续号规则:新 group 的计数器高于本实例分配过的最大计数器,该最大值随 blob 头 `StrategyParamsHdr.group_ctr_hwm` 逐版本携带、只增不减(字段归 [hft-event SPB1](../hft-event/README.md#strategyparamsspb1);续号与 hwm 的规则归 [tools hft-cfg](../../tools/README.md#hft-cfg),pipeline 链接它) | 全局唯一**由构造保证**(高 8 位是实例 id,实例 id 永不复用;同一实例内 `n` 单调分配、永不复用,已移除的最大 `n` 同样不再分配):一个 id 一经分配,永久对应同一个 `GroupSpec::fingerprint()`(基础资产 + 各腿);group 被移除后该 id 不再分配给任何 group;构造保证它既不是 0(`GROUP_NONE`)也不是 `u32::MAX`(`GROUP_ALL`)。`GroupSpec.id` 的高 8 位必须等于该 blob 的 `StrategyParamsHdr.strategy`(hft-event `parse_spb1` 校验,`BadGroup`)。误复用的兜底:SDK `on_params` 按指纹不一致重置该 group 的状态,sequencer 丢弃指纹变化前产生的 Advice(规则归 [hft-strategy-sdk](../hft-strategy-sdk/README.md) 与 [hft-sequencer](../hft-sequencer/README.md)) |
| `SlotId` | u32 | [hft-oms](../hft-oms/README.md),每 (venue, account) 槽数组下标 | 复用,配合 OMS 的 `gen` 区分;`SlotId::UNRESOLVED = SlotId(0)`:回报无法解析到槽(gw 写入 `OrderEvent`,OMS 走对账) |
| `ConnId` | u8 | venue 运行时,每 venue 内编号;BBO 连接的 id 即其槽号(`< MAX_BBO_CONN_SLOTS`,常量在本节,SDK 重导出;槽的分配与退役后复用归 [hft-venue](../hft-venue/README.md)) | 进程生命周期内 |
| `Seq` | u64 | journal 写者 | 每 journal 单调 +1 |
| `Tsc` | u64 | [hft-rt](../hft-rt/README.md) 读数 | 同一主机、同一次启动内可比 |

```rust
#[repr(transparent)] pub struct StrategyId(pub u8);   // 有效值 1..=254(refdata Instances 加载校验;255 不分配,GroupId 构造依赖它)
pub type StrategyInstanceId = StrategyId;   // 同一类型的两个名字:事件字段沿用 StrategyId,账户绑定与 SDK 的 StrategyInstance 用 StrategyInstanceId

#[repr(transparent)] pub struct GroupId(pub u32);
impl GroupId {
    pub const N_MAX: u32 = 0xFF_FFFE;                              // 实例内计数器上限
    pub const fn compose(s: StrategyId, n: u32) -> Option<GroupId>; // s ∈ 1..=254 且 n ∈ 1..=N_MAX → Some((s << 24) | n),否则 None
    pub const fn strategy(self) -> StrategyId;                       // 高 8 位
    pub const fn counter(self) -> u32;                               // 低 24 位
}

/// 每个 venue 的 BBO 连接槽数(连接 id 即槽号,0..MAX_BBO_CONN_SLOTS);sequencer 的成员位图 `bbo_member_mask: u8` 与快照节 26 的行数依赖它。
/// hft-venue、hft-sequencer、hft-strategy-sdk(重导出)共用这一个常量。
pub const MAX_BBO_CONN_SLOTS: u8 = 8;
```

策略**种类**(`kind_id: u16`,由各策略 crate 的 `StrategyDesc` 给出,只追加)不是本 crate 的 ID 类型,规则见 [hft-strategy-sdk](../hft-strategy-sdk/README.md);一个种类可以有多个实例(不同账户、不同参数),一个账户只绑定一个实例([hft-refdata `AccountMeta`](../hft-refdata/README.md#数据结构))。

`VenueId` 常量(`VenueId` 的关联常量,如 `VenueId::BINANCE_USDM`;协议级,与编译进二进制的 `VenueProtocol` 一一对应;refdata 引用未知编号即加载失败)。**`VenueId` 按产品划分,因此每个 `VenueId` 恰有一个 `MarketKind`**(refdata 加载时校验 `VenueCaps` 行与此表一致):

| 值 | 常量 | `MarketKind` | scope |
|---|---|---|---|
| 1 | `BINANCE_USDM` | `LinearPerp` | core |
| 2 | `BINANCE_SPOT` | `Spot` | core |
| 3 | `GATE_USDT_FUT` | `LinearPerp` | optional |
| 4 | `GATE_SPOT` | `Spot` | optional |
| 5 | `OKX_SWAP` | `LinearPerp`(USDT 本位) | core |
| 6 | `OKX_SPOT` | `Spot` | core |
| 7 | 保留(`BINANCE_COINM`) | — | — |
| 8 | `BYBIT_LINEAR` | `LinearPerp` | optional |
| 9 | `BYBIT_SPOT` | `Spot` | optional |
| 10 | `BITGET_UTA_USDT_FUT` | `LinearPerp` | core |
| 11 | `BITGET_UTA_SPOT` | `Spot` | core |
| 12 | `KUCOIN_FUT` | `LinearPerp` | optional |
| 13 | `KUCOIN_SPOT` | `Spot` | optional |
| 14 | `HYPERLIQUID_PERP` | `LinearPerp`(USDC 结算) | optional |
| 15 | `POLYMARKET_CLOB` | `Spot`(二元结果代币,USDC 计价) | optional |
| 32 | `CHAIN_BSC`(外部 venue) | — | optional |
| 33 | `CHAIN_SOLANA`(外部 venue) | — | optional |

`scope` 列与各 `venue-*` README frontmatter 一致(以 frontmatter 为准,`doc-check` 比对)。

线格式枚举全部 `#[repr(u8)]`,0 不是合法值(读到 0 即损坏),派生 `zerocopy::{TryFromBytes, IntoBytes, KnownLayout, Immutable}`:

```rust
pub enum Side       { Buy = 1, Sell = 2 }
pub enum Tif        { Gtc = 1, Ioc = 2, Fok = 3, PostOnly = 4 }
impl Tif { pub const FAK: Tif = Tif::Ioc; }   // fill-and-kill 与 Ioc 同一订单语义;原生类型名为 FAK 的 venue(Polymarket)把 Ioc 映射过去
pub enum MarketKind { Spot = 1, LinearPerp = 2 }   // 市场段;反向(币本位)永续与交割不在 core,不分配值
pub enum HostClass  { Dev2 = 1, C4 = 2, C8 = 3, Metal = 4, C16 = 5, C32 = 6 }   // 主机类:配置 host.class 与 ECF1 头 host_class 的编码,只追加
impl VenueId {
    pub const fn market(self) -> Option<MarketKind>;    // 上表的代码形式;链上 venue、保留与未分配编号 → None
    pub const fn name(self) -> Option<&'static str>;    // 上表"常量"列的名字(如 "BINANCE_USDM");保留与未分配编号 → None
}
```

`MarketKind` 放在本 crate 而不是 refdata:它是 [hft-strategy-sdk](../hft-strategy-sdk/README.md) 视图、[hft-refdata `VenueCaps`](../hft-refdata/README.md#交易所能力表venuecaps)(以 `VenueId` 为键,行内 `market` 冗余)与 venue 协议(`VenueProtocol::MARKET`)共用的交易词汇,和 `Side`/`Tif` 同类。refdata 的 `InstKind`(Spot/Perp/Delivery)描述合约本身,`MarketKind` 描述能力表的市场段,映射为 `Spot → Spot`、`Perp(USDT/USDC 线性) → LinearPerp`。

`HostClass` 放在本 crate 同理:[hft-refdata](../hft-refdata/README.md)(ECF1 `host_class()`、`AssemblyFacts.host`)、[hft-rt](../hft-rt/README.md)(`HostNeeds.class` 与各类的检查下限、hard 位)与 apps/hft-core 共用这一个类型(`pub use`);各类的含义、取值与选定归 [apps/hft-core](../../apps/hft-core/README.md)。

### kill 字位

**本节是 kill 字位布局的唯一归属**。gw 线程发送前读取的 kill 字位常量放在这里(gw 所在的 [hft-venue](../hft-venue/README.md) 不依赖 hft-risk),位的语义与写入规则归属 [hft-risk](../hft-risk/README.md)。

**kill 字全系统只有一种宽度:32 位**。每个 gw 一个 `KillWord` = 缓存行对齐的 `AtomicU32`(类型在 [hft-rt](../hft-rt/README.md),本 crate 不含原子);写者是 sequencer(hft-risk 投影),读者是该 gw 线程;gw 预先算好的掩码 `kill_mask` 同为 `u32`(`KillBits::gw_mask(ix)`)。任何地方出现 8 位或 16 位的 kill 字、kill 掩码或账户位,都是规格错误。

| 位 | 常量 | 含义 | 对 `RISK_REDUCING` |
|---|---|---|---|
| 0 | `GLOBAL` | 全局 kill | 放行 |
| 1 | `VENUE` | 本 gw 所在 venue 的全部账户 kill | 放行 |
| 2 | `HARD` | 全局 HARD:拒绝全部 Place/Amend | 拒绝 |
| 3..7 | — | 保留(恒 0) | — |
| **8..19** | `account(ix)` = `1 << (8 + ix)`,ix = 0..11 | 该 gw 上账户稠密序号 ix 的**账户 kill** | 放行 |
| **20..31** | `account_hard(ix)` = `1 << (20 + ix)`,ix = 0..11 | 同一序号的**账户 HARD**:拒绝该账户全部 Place/Amend | 拒绝 |

```rust
#[repr(transparent)] pub struct KillBits(pub u32);   // KillWord(AtomicU32,hft-rt)里存的值;size_of = 4(编译期断言)
impl KillBits {
    pub const GLOBAL: Self = 1 << 0;
    pub const VENUE: Self  = 1 << 1;
    pub const HARD: Self   = 1 << 2;
    pub const ACCOUNT0: Self = 1 << 8;                  // 账户 kill 位段起点
    pub const ACCOUNT_HARD0: Self = 1 << 20;            // 账户 HARD 位段起点
    pub const ACCOUNT_MASK: Self = 0x000F_FF00;         // bit 8..19
    pub const ACCOUNT_HARD_MASK: Self = 0xFFF0_0000;    // bit 20..31
    pub const MAX_ACCOUNTS_PER_GW: u8 = 12;             // refdata 加载校验:每个 venue 上启用的实盘账户 ≤ 12
    pub const fn account(ix: u8) -> Self;               // 1 << (8 + ix);ix ≥ 12 → debug_assert,release 返回 KillBits(0)(不越入其他位段;account_hard 同)
    pub const fn account_hard(ix: u8) -> Self;          // 1 << (20 + ix)
    pub const fn gw_mask(ix: u8) -> Self;               // GLOBAL | VENUE | HARD | account(ix) | account_hard(ix):gw 对该账户的单只看这些位
    pub const fn hard_mask(ix: u8) -> Self;             // HARD | account_hard(ix)
}
```

gw 判定(实现在 hft-venue,规则归 hft-risk):`op ∈ {Place, Amend}` 时 `k = kill.load(Acquire) & gw_mask(ix)`;`k & hard_mask(ix) ≠ 0` → 拒(含 `RISK_REDUCING`);`k ≠ 0` 且 `oflags` 无 `RISK_REDUCING` → 拒;撤单、查询、维护类永远放行。`RISK_REDUCING` 的位值归 [hft-event `OrderFlags`](../hft-event/README.md#orderflags)。`ix` 是 [hft-refdata `venue_account_ix`](../hft-refdata/README.md#公开-api) 给出的稠密序号;模拟账户的单不经 gw(由 sim-gw 处理,见 [hft-sim](../hft-sim/README.md)),不占序号,kill 对它们只经检查链生效。

**启动自检**:hft-core 在启动闸门 `IDENTITY` 之前,对每个 gw、每个启用的账户序号 ix,经**真实的 `KillWord`**(同一个 `AtomicU32`,`store(Release)` 后在 gw 线程 `load(Acquire)`)分别验证 `account(ix)` 与 `account_hard(ix)` 两位的判定结果,通过后写 Input `Timer{Sys, KillSelfTestOk}`(流程归 [apps/hft-core](../../apps/hft-core/README.md);编码见 [hft-event 定时器编码](../hft-event/README.md#定时器编码))。不允许只对常量做纯函数测试来代替。

### 模式、kill 级别、cap 来源与风控拒绝位

**枚举与编码在本 crate,语义在 [hft-risk](../hft-risk/README.md)**(`hft-risk::mode` 是模式/cap 的唯一归属)与 [hft-strategy-sdk](../hft-strategy-sdk/README.md)(运行模式下策略意图的行为表);hft-risk、hft-strategy-sdk、hft-oms、hft-exec 都 `pub use` 这里的类型:

```rust
#[repr(u8)] pub enum RunMode { Shadow = 1, CloseOnly = 3, Live = 4 }  // 2 退役、永不复用;线上其他值 → InvalidArgs
#[repr(u8)] pub enum KillLevel { Venue = 2, Account = 3, Global = 4 } // 1 退役、永不复用(停一个策略实例用 SetMode)
// HARD 不是级别,是 Kill 命令的 `hard` 位:允许与 Global(全局 HARD)或 Account(按账户 HARD)组合;与 Venue 组合 → InvalidArgs。
// 模拟不是 RunMode,是账户类型(hft-refdata AccountKind::Sim)。

#[repr(transparent)] pub struct CapSource(pub u8);   // cap(模式上限)来源;作用域、设置者、解除条件归 hft-risk,优先级与解除命令列在 control-plane
impl CapSource {
    KILL = 1; DAILY_LOSS = 2; RECON = 3; UNKNOWN_TIMEOUT = 4; BROKEN = 5; REORG = 6 /* optional 链上腿 */; RECOVERY = 7; HALTED = 8;
    DRAINING = 9 /* 不可清除 */; CLOCK_BAD = 10 /* 条件型,自动解除 */;
    HEDGE_FAILURE = 11 /* 账户:任一 group Broken */; MARGIN_STRESS = 12 /* 条件型:保证金分档 */;
    STORAGE = 13 /* 条件型:persist/磁盘退化 */; DELISTING = 14 /* (venue, inst):VenueNotice{Delisting} */;
    ACCOUNT_CFG = 15 /* 条件型:(account, venue) 的账户配置与 VenueCaps 要求不一致 */;
    pub const fn resumable(self) -> bool;   // Resume{source} 只接受 1..=8、11、14;9、10、12、13、15 不能被命令清除 → InvalidArgs
}

#[repr(transparent)] pub struct RejectBits(pub u32);   // OrderDecision/SwapDecision.reject;位号只追加;各位的判定规则归 hft-risk
impl RejectBits {
    INVALID = 1<<0; NOT_TRADABLE = 1<<1; VENUE_NOT_READY = 1<<2; KILLED = 1<<3; CLOSE_ONLY = 1<<4; HARD = 1<<5;
    UNKNOWN_PENDING = 1<<6; NO_MID = 1<<7; STALE_MID = 1<<8 /* 报价不可用:该 inst 的 BBO 连接不健康或在重同步(口径 = 连接健康,不按报价变化时刻) */; PX_BAND = 1<<9;
    NOTIONAL_ORDER = 1<<10; NOTIONAL_GROUP = 1<<11; NOTIONAL_ACCOUNT = 1<<12; NOTIONAL_VENUE = 1<<13;
    POSITION = 1<<14; REDUCE_INVALID = 1<<15; RATE = 1<<16; RATE_BACKOFF = 1<<17;
    DAILY_LOSS = 1<<18; EXPOSURE_GATE = 1<<19; RECON_DEVIATION = 1<<20; /* 1<<21 退役,位号不复用 */ CLOCK = 1<<22; DRAINING = 1<<23;
    MODE_SHADOW = 1<<24 /* 非拒绝:有效模式使意图只记录 */; LOCAL_SEND_FAIL = 1<<25 /* SendReport 确定未写出 */;
    HEDGE_LIMIT = 1<<26 /* 对冲/补腿数量或名义超过上限 */; NOT_BOUND = 1<<27 /* 意图的账户未绑定到发出它的策略实例 */;
    LINK_STALE = 1<<28 /* 链接开仓:第二腿 inst 的 BBO 连接不健康(非 Live、重同步中或连接活性超时;判定归 hft-risk #5) */;
    LINK_FUNDS = 1<<29 /* 链接开仓:第二腿的可用计价资产或可用保证金不足以负担该链接 */;
    CAPACITY = 1<<30 /* OMS 订单槽或链接表容量耗尽,未建单未发送 */;
}
```

`RejectBits` 放在这里是为了让 hft-oms、hft-exec、hft-event 的导出器与 hft-risk 同用一份位号,而不让 OMS/执行层依赖 hft-risk。`CAPACITY` 由 [sequencer 意图管线](../hft-sequencer/README.md#意图管线)置位;其余各位何时置位(检查链顺序、哪些类别适用)只在 [hft-risk](../hft-risk/README.md) 定义。

### 对冲定价限值

风控与 OMS 共用的值类型(L1 互不依赖,所以放在 L0;**本节是它的唯一定义**):sequencer 按对冲单的账户与 inst 经 hft-risk `hedge_px_limits()` 填写,传给 [hft-oms `price_hedge`](../hft-oms/README.md);hft-risk 检查链 #5(含链接腿 `LINK_STALE`)用同一组值,保证"OMS 认为可定价"与"风控认为报价可用"同值同源。字段的含义、默认值与校验归 [hft-risk](../hft-risk/README.md)。

```rust
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub struct HedgePxLimits {
    pub conn_liveness_ms: u32,   // 报价连接健康阈值:now − BboView.conn_live_tsc ≤ 它(判定的唯一定义在 hft-strategy-sdk BboView)
    pub rr_max_cross: Rate,      // 风险递减单相对 mid 的越价上限(= 该 inst 所属价格带类别的 PxBand.rr_max_cross;风控检查链 #5 的同一口径)
}

/// 引擎单与对冲单限价的唯一实现:OMS `price_hedge` 与 hft-exec 的全部 Engine 订单(HedgeFix、AutoFix、ForceStep、Flatten、补腿)共用。
/// 买:min(touch × (1 + max_cross), mid × (1 + lim.rr_max_cross)),向下取到 tick;
/// 卖:max(touch × (1 − max_cross), mid × (1 − lim.rr_max_cross)),向上取到 tick(取整方向 = side.price_round())。
/// touch = 对手价(买取 ask、卖取 bid);max_cross ≥ 0、lim.rr_max_cross ≥ 0;touch ≤ 0、mid ≤ 0、max_cross < 0、
/// lim.rr_max_cross < 0、tick ≤ 0 或取整后结果 ≤ 0 → Px(0)(调用方按报价不可用处理)。
/// 两个限值按精确值(i128)比较,结果只取整一次(等价于各中间量按同一方向取整);超出 MAX_RAW 按不变量 4 饱和。
/// 结果可能比 touch 更不利于成交(买价 < ask):是否仍下单(如 `BeyondCross`)由调用方判定。
pub fn cap_cross_px(side: Side, touch: Px, mid: Px, max_cross: Rate, lim: &HedgePxLimits, tick: i64) -> Px;
```

`cap_cross_px` 只算价格;数量按该限价折算名义(`IntentView.notional = qty × 限价`,口径归 [hft-risk](../hft-risk/README.md))与切片规则归 [hft-exec](../hft-exec/README.md)。

### 资金费结算时刻归一化

同一笔资金费结算的时刻有三个来源:交易所推送(`FundingSettle.settle_ts_ns`,由 `venue-*` 解码)、sequencer 合成(结算边界)、reconciler 读到的交易所流水。三方按"归一化后的结算时刻"配对,归一化**只有这一个实现**(hft-ledger、hft-sequencer、reconciler 共用;`venue-*` 只给原始时刻、不调用它;配对规则归 [hft-ledger](../hft-ledger/README.md)):

```rust
/// 把 ts_ns 取到最近的结算边界 anchor_s + k × interval_s(UTC 秒,k 为整数);到两侧边界等距时取较早者。
/// interval_s 取该 inst 的实际周期(Funding.interval_s;0 = 未知时用 refdata InstMeta.funding_interval_s);interval_s = 0 → 原样返回 ts_ns。
/// i128 中间量,不溢出;纯函数。最近边界超出 i64 范围时取另一侧(范围内)的相邻边界。
pub fn funding_settle_norm_ns(ts_ns: i64, interval_s: u32, anchor_s: u32) -> i64;
```

只属于某一事件的枚举(`OrderEventKind`、`TxState`、`ConnStateKind`、`OrderDecision.link_role` 的编码…)在 [hft-event](../hft-event/README.md)。

### ClientOrderId 编解码

字段含义、宽度、分配与唯一性、上一 epoch 的接受窗口归属 [hft-oms](../hft-oms/README.md);本 crate 只提供逻辑值与**规范文本**(24 个小写十六进制字符)的编解码,供 gw 线程里的 `venue-*` 渲染与解析(各所外形——前缀、填充——由 `VenueProtocol::render_cid/parse_cid` 包装)。

```rust
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub struct ClientOrderId { pub epoch: u16 /* 最高位 = 保留位,恒 0 */, pub venue: VenueId, pub account: AccountId, pub slot: u32 /* 低 20 位 */, pub r#gen: u32 /* gen 是 2024 版保留字 */ }
// 96 位 = ver:4 | epoch:16 | venue:8 | account:8 | slot:20 | gen:32 | check:8,从最高位起按此顺序排列;ver = 1
// 文本 = 这 96 位按大端(最高位在前)的 24 个小写十六进制字符;check = 前 11 个字节(前 88 位)按字节异或
// epoch 的最高位(bit 15)是恒 0 的保留位;低 15 位是 engine_epoch(取值与分配规则归 hft-oms)
impl ClientOrderId {
    pub const RESERVED_BIT: u16 = 1 << 15;                    // 编码恒写 0(输入的该位被忽略);解码读到 1 → CidErr::Reserved
    pub fn encode(&self, out: &mut [u8; 24]);                  // slot ≥ 2^20 → 调用方错误,debug_assert;release 只编码低 20 位,不 panic
    pub fn decode(text: &[u8]) -> Result<ClientOrderId, CidErr>; // 多个错误时按 Length → Charset → Version → Check → Reserved 报第一个
}
#[repr(u8)] pub enum CidErr { Length = 1, Charset = 2 /* 非小写十六进制 */, Version = 3, Check = 4, Reserved = 5 /* 保留位非 0 */ }
```

bit 15 只是布局中的保留位,没有任何逻辑。一个交易所账户只由一个系统、一个策略实例交易,因此不需要"是不是本系统的单"判定;解码不判断 epoch 是否当前、venue/account 是否与连接相符——那是调用方(gw、OMS)的规则。交易所侧的强平/ADL/人工单解码失败后的处理(`ExternalFill`)归 venue 协议与 [hft-oms](../hft-oms/README.md)。

### 布局规则(全仓 `#[repr(C)]` 结构适用)

- 每个 `#[repr(C)]` 结构写出**全部**显式 `_pad` 字段(无隐式填充),并紧跟编译期断言:`const _: () = assert!(size_of::<T>() == N); const _: () = assert!(offset_of!(T, f) == K);`——对每个在 README 注释中写了偏移的字段都断言。
- 含枚举字段的结构只派生 `TryFromBytes`(0 与越界值非法);结构尾部**追加**或**占用保留字节**的字段不得是枚举,或者该枚举必须把 0 定义为合法的"未提供"值(旧记录补零读出时仍合法;事件体的两种演进方式见 [event-model 演进规则](../../docs/subsystems/event-model.md#演进规则))。
- 比率类型全仓只有 `Rate`(1e-9);不出现 bp 整数、`f64` 比率或其他精度的 ppm 字段。
- 规格中写出的字节布局(字段顺序、偏移、`_pad`、行版本号)是**草案,实现时以代码与编译期断言为准**;码值与字段名全仓必须一致,改动走同一改动内的 README 更新。

### 错误

```rust
#[repr(transparent)] pub struct FxErr(pub u8);   // 位标志,可 OR 聚合后一次判断
impl FxErr { OK = 0; SYNTAX = 1; EMPTY = 2; PRECISION = 4; OVERFLOW = 8; NEGATIVE = 16; DIV_ZERO = 32; EXP_RANGE = 64 }
// 实现 BitOr / BitOrAssign(聚合);OK 表示无错误
```

没有 `panic`、没有 `Result<_, String>`;热路径把多个字段的 `FxErr` OR 在一起,一次分支。

## 实现说明

<details><summary>实现要点</summary>

**解析算法**(`parse_exact`,输入长度 n ≤ 40 字节,超长 → `SYNTAX`):

1. 读可选 `-`。
2. 整数部:跳过前导 `0`;用 SWAR 一次处理 8 位(`u64::from_le_bytes` 减 `0x30…`、校验每字节 < 10、两两合并),剩余逐字节;累加到 `mant: u64`,记有效位数 `nd`。
3. 可选 `.` 后小数部:同上累加,记小数位数 `nf`;整个数字串(整数部与小数部连起来)末尾的零单独计数 `tz`,不累加进 `mant`(有效位规则见语法第 3 条)。
4. 可选 `e/E`:读符号与 1–3 位十进制 `x`。
5. 合并十进制指数 `E = x + tz - nf`;目标位移 `k = E - exp`。`mant = 0` → 0;`k ≥ 0`:`mant × 10^k`(`k ≥ 20` 或 `checked_mul` 溢出 → `OVERFLOW`,查 `POW10: [u64; 20]`);`k < 0`:`mant` 必须被 `10^-k` 整除,否则 `PRECISION`(exact)/按 `RoundMode` 调整(round,余数与半值比较,HalfEven 看商的奇偶)。
6. 检查 `≤ MAX_RAW`,施加符号。

**乘法到名义**:`(px as i128) * (qty as i128)`,指数 `px_exp + qty_exp`;目标指数 −6:`s = (px_exp + qty_exp) + 6`。`s ≥ 0` 乘 `10^s`(i128),`s < 0` 除 `10^-s` 并取整。`POW10_I128: [i128; 39]` 常量表。取整除法统一实现 `div_round(n: i128, d: i128, m) -> i128`,负数语义:Floor 向 −∞、Ceil 向 +∞、TowardZero 截断、HalfEven 半值取偶。

**U256**:`d ≥ 0` 时做 256÷64 的长除(除数 `10^d` 在 d ≤ 19 时为单肢;d 最大 18+18=36 时分两次除以 `10^18` 与 `10^(d-18)`,余数合并判断取整);结果须落入单肢且 ≤ `MAX_RAW`。

**格式化**:`write_fixed` 先写整数部(`itoa` 风格两位查表),再写小数部并去尾零;40 B 输出缓冲足够(`exp ∈ [-30, 18]`:最长为符号 + 19 位数字 + 18 个尾零 = 38,或符号 + `"0."` + 30 位小数 = 33)。

**kill 字**:`KillBits` 是 u32,每 gw 一个缓存行对齐的 `AtomicU32`(`KillWord` 在 hft-rt);账户位按 `venue_account_ix` 的稠密序号,序号 0..11 同时决定 `account(ix)`(bit 8..19)与 `account_hard(ix)`(bit 20..31)。`gw_mask`/`hard_mask` 是 `const fn`,gw 在装配时按它服务的账户算好掩码,热路径只有一次 `load` + 两次与运算。

**`pair_lot`**:全部在 i128 内计算:`M = den × lot_h`,`step_h = M / gcd(M, num)`,结果 `lcm(lot_f, step_h) = lot_f / gcd(lot_f, step_h) × step_h`;结果 > `MAX_RAW` → `None`。gcd 用二进制算法。冷路径:`SetParams` 校验与执行层 `set_target` 各算一次,结果缓存在 cell 中。

**范围核验(core 品种;指数的取值规则归 [hft-refdata 指数规则](../hft-refdata/README.md#实现说明),由 `hft-cfg` 生成 refdata 时给出,最终以 refdata 为准)**:

| 资产 | `px_exp` | `qty_exp` | 典型价 → raw | 价上限 → raw | 数量上限 → raw | 名义例:i128 积 → `Notional` |
|---|---|---|---|---|---|---|
| BTC(现货 tick 0.01、lot 1e-5) | −4 | −8 | 1e5 → 1e9 | 1e7 → 1e11 | 2.1e7 枚(总量)→ 2.1e15 | 50 BTC:1e9 × 5e9 = 5e18,指数 −12 → 5e6 USDT → raw 5e12 |
| PEPE(`1000PEPEUSDT` tick 1e-7/千枚 = 1e-10/枚) | −12 | −3 | 1e-5 → 1e7 | 1e-2 → 1e10 | 4.2e14 枚(总量)→ 4.2e17 | 1e11 枚:1e7 × 1e14 = 1e21,指数 −15 → 1e6 USDT → raw 1e12 |
| SHIB(同量级) | −12 | −3 | 1e-5 → 1e7 | 1e-2 → 1e10 | 5.9e14 枚 → 5.9e17 | 同上 |
| 链上低价代币(价 1e-10,optional) | −18 | 0 | 1e-10 → 1e8(8 位有效) | 1e-4 → 1e14 | 1e18 枚 → 1e18 | 1e13 枚:1e8 × 1e13 = 1e21,指数 −18 → 1e3 USDT → raw 1e9 |
| 供应量 > 4e18 的代币 | −18 | +3(千枚为单位) | — | — | 1e21 枚 → 1e18 | 正指数合法;链上金额按 `Floor/Ceil` 取到千枚,单位价值 ≪ 1e-6 USDT |

所有行 `|raw| ≤ 4.61e18`(`MAX_RAW`),所有 i128 积 ≤ 1e21 ≪ 1.7e38,名义结果 ≤ 9.22e18。**约束 `px_exp + qty_exp ≤ −6`**(由 [hft-refdata](../hft-refdata/README.md) 加载校验,上表各行分别为 −12、−15、−15、−18、−15):它保证换算到 `Notional` 的那一步永远是除法(`s ≤ 0`),不会出现乘 `10^s` 的溢出路径;`mul_qty` 仍对 `s > 0` 做 checked 饱和作为防御。`1000PEPEUSDT` 的换算:价格线格式指数 = `px_exp + 3 = −9`(`"0.0123456"` → 12 345 600 = 1.23456e-5 USDT/枚),数量线格式指数 = `qty_exp − 3 = −6`(`"5"` → 5e6 = 5000 枚)。

**规划源文件**

| 文件 | 职责 |
|---|---|
| `src/lib.rs` | 模块入口、重导出 |
| `src/types.rs` | `Px`/`Qty`/`Notional`/`Rate`/`AssetExp`/ID newtype/枚举(含 `MarketKind`、`HostClass`、`StrategyInstanceId` 别名、`GroupId::compose`、`INST_NONE` 等常量、`MAX_BBO_CONN_SLOTS`)、`HedgePxLimits`,只有类型与 `const fn` |
| `src/fixed.rs` | 乘除、取整、`div_round`、`POW10` 表;数量取整模块(`round_to_lot`、`split_lot`、`pair_lot`);`cap_cross_px`;`funding_settle_norm_ns` |
| `src/parse.rs` | `parse_exact`/`parse_round`(SWAR) |
| `src/format.rs` | `write_fixed` |
| `src/u256.rs` | `qty_from_u256`/`qty_to_u256` |
| `src/venue_ids.rs` | `VenueId` 常量表、`name()`、`market()` |
| `src/wire.rs` | `KillBits`、`RunMode`、`KillLevel`、`CapSource`、`RejectBits` |
| `src/clid.rs` | `ClientOrderId` 编解码 |
| `src/err.rs` | `FxErr` |

</details>

## 延迟与资源契约

| 入口 | 预算(参考机,单核,Criterion 中位 / p99) | 基准 |
|---|---|---|
| `parse_exact`(≤ 16 字节典型价格/数量) | ≤ 20 ns / ≤ 40 ns([04 §2](../../design/04-行情接入层.md) 给 20–40 ns/字段) | `types_parse_px`、`types_parse_qty`、`types_parse_sci` |
| `write_fixed` | ≤ 15 ns / ≤ 30 ns | `types_write_fixed` |
| `Px::mul_qty`(i128 + 除 10^s) | ≤ 5 ns / ≤ 10 ns | `types_mul_qty` |
| `round_to_tick`/`round_to_lot` | ≤ 3 ns | `types_round` |
| `qty_from_u256`(optional 链上) | ≤ 40 ns(非热路径,chain-core 每事件一次) | `types_u256` |
| `ClientOrderId::encode` / `decode`(SIMD 十六进制) | p50 ≤ 20 / 15 ns,p99 ≤ 40 / 30 ns(预算归 [hft-oms](../hft-oms/README.md)) | `types/clid_encode`、`types/clid_decode` |
| `cap_cross_px` | ≤ 10 ns / ≤ 20 ns(两次 `mul_rate` + 比较 + 取整) | `types_cap_cross_px` |
| `funding_settle_norm_ns` | 冷路径(资金费结算每 inst 每周期一次),无预算 | — |

零分配、零系统调用、无共享可变状态;所有函数是 `#[inline]` 候选,可被 PyO3 与回放直接调用。基准只在按需租用的参考机上按回归阈值阻断(每个里程碑结束时运行 `bench-gate --reference`,[benchmarks](../../benchmarks/README.md#参考机与-ci));dev2 与 CI 只报告。

## 测试与验收

用例与验收见 [TESTS.md](TESTS.md)。

## 依赖

- 内部:无(最底层)。
- 外部:`zerocopy`(0.8,派生布局 trait,与 [hft-event](../hft-event/README.md) 同版本);可选 `pyo3` 仅在 `hft-py` 的 feature 下为类型实现转换(不在本 crate 默认依赖)。
- 禁止:`rust_decimal`、`serde_json`、任何浮点解析库、`alloy`/`ruint`(U256 用肢数组交换)、`std::time`。
- 版本约束理由:`zerocopy` 锁 0.8.x 小版本,因为派生出的布局检查是事件线格式的一部分,升级走 `gen-schema` 的布局锁比对。

## 延伸阅读

- [13 R4 按资产定指数](../../design/13-设计评审与优化.md)、[10 §3–4 定点与编码规则](../../design/10-语言分工与代码组织.md)、[07 策略层](../../design/07-策略层.md)
- [research/04 §2.8 定点库对比与 f64 双重舍入陷阱](../../design/research/04-oss-frameworks-and-patterns.md)
- 取值与校验:[hft-refdata](../hft-refdata/README.md);线格式使用:[event-model](../../docs/subsystems/event-model.md);cap 与 kill 的语义:[hft-risk](../hft-risk/README.md);优先级与解除命令:[control-plane](../../docs/subsystems/control-plane.md#模式与上限的统一优先级)
- 范围与账户规则的决策出处:[round8-decisions](../../design/_work/round8-decisions.md);决策记录:[Agent Notes](../../.agents/notes/README.md) 中的 `per-asset-fixed-point`、`one-account-per-strategy-instance`

## 已知限制与后续工作

- **报价资产限于 USD 稳定币**:BTC/BNB 计价的 CEX 交易对需要 `(base, quote)` 维度的指数,当前不支持。可选扩展:新增 `PairExp` 并在 refdata 以 `(asset, quote)` 为键(触发条件:要交易非稳定币报价的 CEX 交易对)。
- **反向(币本位)永续与交割合约**不在 `MarketKind` 里。可选扩展:追加 `InversePerp`、`Delivery`(触发条件:策略需要这些市场)。
- **每个 venue 至多 12 个实盘账户**(`KillBits` 的账户位宽度:bit 8..19 与 bit 20..31 各 12 位)。超出时把 `KillWord` 扩为 `AtomicU64`(账户位与 HARD 位各 28 个),是一次全系统布局变更,本节、hft-rt、hft-venue 与 hft-risk 同一改动(触发条件:单个 venue 上的实盘子账户数超过 12)。
- **价格相关 tick**(Hyperliquid 5 位有效数字规则)不是单一整数步长;`round_to_tick` 只处理固定步长。可选扩展:refdata 增加 `TickRule::SigFig`、本 crate 增加 `round_sigfig`(触发条件:接入 Hyperliquid)。
- `Notional` 固定 1e-6:单笔费用在 1e-6 报价币以下的部分被取整;对 PnL 汇总的累计误差 < 1e-6 × 成交笔数,可接受。

### 开发备忘

<details><summary>给实现者</summary>

- **实现顺序**:`types.rs` → `fixed.rs`(含参考实现与属性测试;`cap_cross_px` 随之)→ `parse.rs` 标量版 → SWAR 版(属性测试对拍标量版)→ `format.rs` → `wire.rs` → `clid.rs` → `u256.rs`(optional 链上使用,可最后)。
- **待核实**:哪些 core venue 会输出科学计数法字符串(方法:在 Binance、OKX、Bitget 金样与 24 h raw 采集中 grep `[eE][-+]?[0-9]`);OKX 的 PEPE/SHIB 最细 tick 与 `ctVal`、OKX 是否存在非 10 的幂的 `ctVal`(方法:各所 instruments 接口,由 `hft-cfg refdata fetch` 生成待审 TOML 时打印;非 10 的幂的 inst 被 refdata 加载拒绝,见 hft-refdata)。
- 本 crate 开 `#![deny(clippy::arithmetic_side_effects)]`([hot-path-rules N2](../../docs/hot-path-rules.md)):加减也写成显式 `checked_*`,溢出置 `FxErr::OVERFLOW` 并饱和;不变量 1 保证它在合法输入下不触发。
- `Px`/`Qty` 不实现 `Add<Px> for Qty` 之类跨类型运算;不实现 `Mul` 运算符(乘法必须显式给指数与取整)。`Ord`、`Add`、`Sub`、`Neg` 只在同类型间实现;`Add`/`Sub`/`Neg` 在 i64 边界饱和(合法输入下等于精确结果,违反不变量 1 的输入也不 panic)。
- `Rate` 选 1e-9 是为了资金费率(交易所给 8 位小数)与 0.01 bp 级费率都精确;若将来遇到更细的输入,`parse_round(HalfEven)` 处理。
- `StrategyInstanceId` 是 `StrategyId` 的类型别名而不是新 newtype:两者是同一身份(策略实例),分成两个类型会让每个事件字段与绑定表之间多一次无意义的转换。

</details>
