---
description: "策略快速部分的事件驱动特征节点:处理器看到所声明种类的每个事件,求值按需;定点、无分配、不读时钟;导出定点列供研究与回放;不含任何策略的规则数字"
kind: crate
tier: pure
scope: core
status: impl
---

# hft-feature

## 概要

`hft-feature` 是策略 crate 可选依赖的**特征节点库**。一个特征 = **处理器**(handler:所声明种类的每个事件都要经过它,O(1) 或有界 O(k) 地更新状态)+ **求值**(eval:规则或导出需要时才算)。节点是定点整数(`Rate` = 1e-9,无 f64)、只在 `new` 分配、不读时钟(时间由调用方从 `Views::now()` 与事件传入)、不含阈值(阈值都是调用参数)。策略把节点组合成自己的特征集,并按事件种类写出固定的执行计划(哪些处理器、什么顺序),规则只读特征;同一份特征代码在实盘与回放中运行。它只依赖 [hft-strategy-sdk](../hft-strategy-sdk/README.md);**引擎 crate 不依赖它**。它不拥有策略状态的快照(特征状态是否进快照由使用方决定),也不拥有任何策略专属的组合(那些留在策略包里)。

## 目录

- [使用本组件](#使用本组件)
  - [定点约定](#定点约定)
  - [Feature 契约](#feature-契约)
  - [节点库](#节点库)
  - [组合与执行计划](#组合与执行计划)
  - [导出列](#导出列)
- [实现说明](#实现说明)
- [延迟与资源契约](#延迟与资源契约)
- [测试与验收](#测试与验收)
- [依赖](#依赖)
- [延伸阅读](#延伸阅读)
- [已知限制与后续工作](#已知限制与后续工作)

## 使用本组件

### 定点约定

- 比例、收益、失衡、概率与标准差一律 `Rate`(i64,1e-9,`ONE` = 1e9);中间乘积用 i128,平方和用 u128。
- 价格与数量是调用方 inst 的原始整数(同一资产各 venue 同一指数);"2 × 中间价"(买一 + 卖一)不除以 2,避免取整。
- 每个公式都按文档写明的截断方式计算(Rust 整数除法向零截断,`div_euclid` 处另注),离线移植可逐位复现。

### Feature 契约

```rust
pub trait Feature {
    type Input<'a>;                        // 处理器的输入,由调用方从事件与视图取出
    const EVENTS: Events;                  // 必须看到的事件种类(BBO | BOOK | TRADE | ORDER | TIMER;ANY = 每个事件)
    const COLUMNS: &'static [ColumnDesc];  // 导出列(名、单位、十进制指数)
    fn on_event(&mut self, x: Self::Input<'_>);
    fn export(&self, row: &mut FeatureRow<'_>);   // 按 COLUMNS 顺序写出求值结果
}
pub struct Events(pub u16);               // NONE, BBO, BOOK, TRADE, ORDER, TIMER, ANY
pub enum Unit { Ratio, Px, Qty, Ns, Count, Minute }
pub struct ColumnDesc { pub name: &'static str, pub unit: Unit, pub exp: i8 }   // 值 = 原始整数 × 10^exp 该单位
pub const NA: i64 = i64::MIN;            // 缺值
pub struct FeatureRow<'a> { /* 调用方的 &mut [i64] 与写出计数 */ }
impl FeatureRow<'_> { pub fn new(vals: &mut [i64]) -> FeatureRow<'_>; pub fn put(&mut self, v: i64); pub fn put_opt(&mut self, v: Option<i64>); pub fn len(&self) -> usize; }
```

- 处理器不做截断(cutoff):输入相同即更新,不因"值没变"跳过下游。
- 带参数的求值(如 `recent(k)`、`at(t)`、`net(x, o, from)`)是节点的固有方法;`export` 只写不带参数的求值。
- `FeatureRow` 超出缓冲的值不写,但计入 `len`(调用方据此发现缓冲不足)。
- `Unit`、`ColumnDesc`、`NA`、`FeatureRow` 定义在 [hft-strategy-sdk 特征导出](../hft-strategy-sdk/README.md#特征导出),本 crate 原样再导出:策略的 `Strategy::FEATURES` / `export_features` 与节点的 `COLUMNS` / `export` 是同一组类型。

### 节点库

| 节点 | 处理器(`EVENTS`) | 求值 | 导出列 |
|---|---|---|---|
| `RealizedVol` | `on_mid(mid_x2, utc_sec)`(BBO):UTC 秒内第一条报价记一个收益 r = (m − m₀)·1e9 / m₀(截到 ±1e9),长度 `cap` 的环与 Σr² | `sigma(min_samples)` = ⌊√(Σr² / n)⌋(n < `min_samples` 或 0 → `None`);`recent(k)` = 最近 k 个收益之和与绝对值之和;`samples()` | `sigma`(Ratio, −9;至少 1 个样本)、`samples`(Count) |
| `BboTop` | `on_quote(bid_px, bid_qty, ask_px, ask_qty)`(BBO):两侧有价时 `mid_x2` = 买一 + 卖一;数量都 ≥ 0 且和 > 0 时 `imb` = (bq − aq)·1e9 / (bq + aq),否则保留上一值;返回是否两侧有价 | 字段 `mid_x2`、`imb` | `mid_x2`(Px, 0)、`imb`(Ratio, −9) |
| `SecHist<N>` | `put(utc_sec, v)`(ANY):每秒一个值,同一秒后写覆盖;`N` 秒的环 | `at(t)` = 第 t 秒的值,否则 t − 1,否则 t − 2,否则 `None` | 无 |
| `SignedFlow<N>` | `push(utc_ns, inst, signed_qty)`(TRADE):最近 `N` 笔(调用方决定哪些成交计入) | `net(x, o, from_ns)` = 时刻 ≥ `from_ns` 的 x 的买与 o 的卖为正、x 的卖与 o 的买为负之和(i128) | 无 |
| `OwnExcludedSide<L>` | 无状态:`read(book, side, own)` 从最优价起复制至多 `L` 档 | `size_at(px)` = 该价数量 − 自己在该价的剩余量(≥ 0);`best()` = 离最优价最近的扣除后仍有量的价;`top_size()`;`depth(bucket, n)` = 从 `best` 所在的 `bucket` 整数倍价起、背离最优价方向(买往下、卖往上)n 个整数倍价的扣除后数量之和 | 无 |
| `LagQuantiles` | `push(rx_ns, lag_ns)`(样本按接收秒归组,早于当前秒的计 `late`、超过 1 024 个的计 `overflow`);`tick(utc_ns)`(ANY):UTC 进入新秒即结算上一秒(排序后取 90 分位与中位数,×10),进入新分钟即结算上一分钟(各秒值的中位数,×100) | `gates(utc_ns, load_ns, slow_ns)` = (上一 UTC 分钟每秒 90 分位的中位数 ≥ `load_ns`,每秒中位数的中位数 > `slow_ns`);阈值 0 不判;该分钟不是上一分钟 → 都为假;字段 `done_min`、`done_p90`、`done_p50` | `lag_minute`(Minute)、`lag_p90`、`lag_p50`(Ns, −2);未结算任何分钟 → `NA` |
| `zsum` | 函数:z = ⌊Σᵢ ⌊signᵢ·(xᵢ − muᵢ)·1e9 / sdᵢ⌋ · 1e9 / `sqrt_n_nano`⌋;缺失或 sd ≤ 0 的特征加 0;`sqrt_n_nano` ≤ 0 → 0 | — | — |
| 定点函数 | `isqrt`(牛顿法,向下取整)、`atanh2(u)` = 2·Σ_{j=0}^{11} u^(2j+1)/(2j+1)(u² 与每次幂乘都 ⌊·/1e9⌋)、`ln_ratio(a, b)` = `atanh2(⌊(a − b)·1e9 / (a + b)⌋)`(a 或 b ≤ 0 → 0)、`ln_rate(x)`(x ≥ 1e9:减半 k 次到 < 2e9,k·ln2 + `ln_ratio`;否则 0)、`imbalance(a, b)`、`quantile_x10(v, q10)`(pandas 线性插值 ×10)、`cdf`/`inv_cdf`(Φ(k/64) 表 k = 0..=320 线性插值、对称;逆函数二分,p 截到 [0.001, 0.999]) | — | — |

### 组合与执行计划

使用方在自己的特征集结构里持有节点(内联、无 `Box<dyn>`),并写出每种事件的处理器序列:被读的先更新,无依赖的按声明顺序;序列在编译期固定,运行时只做事件种类的分派。按参数开关整组跳过处理器是使用方的规则(写进其 README),不是本库的截断。策略的组合示例:[binary-maker 特征](../../strategies/binary-maker/README.md#特征)。

### 导出列

导出一行 = 依次调用各节点的 `export`;列名、单位与指数由各节点的 `COLUMNS` 拼成使用方的列表。研究侧按 `exp` 把定点值换成浮点(`NA` 为缺值)。

## 实现说明

<details><summary>实现要点</summary>

| 文件 | 职责 |
|---|---|
| `src/lib.rs` | `Feature`、`Events`;`Unit`、`ColumnDesc`、`NA`、`FeatureRow` 再导出自 SDK |
| `src/fixed.rs` | `ONE`、`LN2_NANO`、`isqrt`、`atanh2`、`ln_ratio`、`ln_rate`、`imbalance`、`quantile_x10` |
| `src/norm.rs` | Φ 表与 `cdf`、`inv_cdf` |
| `src/vol.rs`、`src/top.rs`、`src/hist.rs`、`src/flow.rs`、`src/depth.rs`、`src/lag.rs`、`src/zscore.rs` | 节点(见节点库) |
| `tests/nodes.rs` | 用例(编号见 [TESTS.md](TESTS.md)) |

- 分配只在 `RealizedVol::new`(环 `cap` × 8 B)与 `LagQuantiles::new`(1 024 × 8 B 的秒缓冲);其余节点是定长数组。
- 越界一律用 `get`/`get_mut` 处理,不 panic。

</details>

## 延迟与资源契约

处理器与不带参数的求值 O(1);`recent(k)` O(k);`net` O(N);`OwnExcludedSide::read` O(L),`size_at`/`best`/`depth` O(L)/O(L²)/O(n·L);`LagQuantiles` 每秒一次至多 1 024 个样本的排序、每分钟一次 60 个值的排序。零分配(`new` 之后)、无系统调用、无共享可变状态。开销计入使用方策略的 `decided_tsc − proc_tsc` 份额;单节点不设独立基准,使用方的 `on_event` 基准覆盖(例:binary-maker `tick_comp`)。

## 测试与验收

用例见 [TESTS.md](TESTS.md)。

## 依赖

- 内部:[`hft-strategy-sdk`](../hft-strategy-sdk/README.md)(`BookRead`、`Level`、`Side`、`InstId`、`Px`、`Qty`);dev 依赖 `hft-strategy-sdk`(`testing`)、`hft-refdata`。
- 外部:无。
- 禁止:引擎 crate、`hft-rt`、任何 `strategies/*`;I/O、`std::time`、`HashMap`、`dyn Trait`、f64(`#![deny(clippy::float_arithmetic)]`)。

## 延伸阅读

- [hft-strat-lib](../hft-strat-lib/README.md)(另一类通用构件:价差、触发价、确认窗口、节流)
- [hot-path-rules](../../docs/hot-path-rules.md)、[strategies/AGENTS.md](../../strategies/AGENTS.md)

## 已知限制与后续工作

- 节点没有快照读写;要进策略快照时,在节点上加 `snapshot`/`restore` 并由使用方升级快照布局。
- 执行计划由使用方手写;按依赖自动排序的声明宏(`feature_dag!`)未实现。
- 导出列只定义了结构;写进 journal(Meta 记录)或回放 Parquet 的导出由回放工具实现。

### 开发备忘

<details><summary>给实现者</summary>

- 新节点:处理器只吃调用方取出的原始整数(不直接读 `Views`),这样回放与研究可以在没有视图的地方复用同一处理器;在本表、`TESTS.md` 与 `tests/nodes.rs` 同一改动补齐。

</details>
