---
component: hft-feature
prefix: FEA
spec: README.md
---

# hft-feature 测试规格

格式与规则见 [docs/tests](../../docs/tests/README.md)。节点与 binary-maker 原内联实现的逐位一致由 [BMK-034](../../strategies/binary-maker/TESTS.md#bmk-034-生成事件流上的决定轨迹固定) 在策略层验证。

## 覆盖

| README 小节 | 用例 |
|---|---|
| 概要 | FEA-010 |
| 目录 | 不适用:非规范 |
| 使用本组件 | 不适用:见子节 |
| 定点约定 | FEA-001、FEA-009 |
| Feature 契约 | FEA-010 |
| 节点库 | FEA-001–FEA-009 |
| 组合与执行计划 | 不适用:由 BMK-034 覆盖 |
| 导出列 | FEA-003、FEA-008、FEA-010 |
| 实现说明 | 不适用:非规范 |
| 延迟与资源契约 | 不适用:由使用方基准覆盖 |
| 测试与验收 | 不适用:非规范 |
| 依赖 | 不适用:由 `cargo xtask deps` 覆盖 |
| 延伸阅读 | 不适用:非规范 |
| 已知限制与后续工作 | 不适用:非规范 |
| 开发备忘 | 不适用:非规范 |

## 用例

### FEA-001 定点函数

- 规则:[节点库](README.md#节点库)(定点函数)
- 级别:table
- 里程碑:M3
- 前置:无状态函数
- 实现:`crates/hft-feature/tests/nodes.rs::fea_001_fixed_point_math`

| 行 | 输入 | 期望 |
|---|---|---|
| FEA-001.1 | `isqrt` 0、1、99、100、2¹⁰⁰ | 0、1、9、10、2⁵⁰ |
| FEA-001.2 | `ln_rate` 1e9、2e9、1e10、3.1e10、999 999 999;`ln_ratio(10 001, 10 000)` 与反向、`(0, 10 000)`;`atanh2(−3e8)` | 0、693 147 181、2 302 585 091、3 433 987 198、0;99 994、−99 994、0;−619 039 204 |
| FEA-001.3 | `imbalance` (3, 1)、(1, 2)、(0, 0) | 5e8、−333 333 333、0 |
| FEA-001.4 | `quantile_x10` 1..=10 的 0.9、1..=4 的 0.5、[7] 的 0.9、空 | 91、25、70、`None` |

### FEA-002 标准正态分布函数

- 规则:[节点库](README.md#节点库)(`cdf`/`inv_cdf`)
- 级别:table
- 里程碑:M3
- 前置:Φ 表(k/64,k = 0..=320)
- 实现:`crates/hft-feature/tests/nodes.rs::fea_002_normal_cdf`

| 行 | 输入 | 期望 |
|---|---|---|
| FEA-002.1 | Φ(0)、Φ(±1)、Φ(1.96)、Φ(9);Φ(1/64) | 与真值差 ≤ 2e-5;表值 506 233 219 |
| FEA-002.2 | Φ(Φ⁻¹(p)),p = 0.01…0.99;Φ⁻¹(0) 与 Φ⁻¹(0.001);Φ⁻¹(−5e-9) 与 Φ⁻¹(1 + 5e-9) | 差 ≤ 1e-3;两者相等(截断);互为相反数 |

### FEA-003 实现波动率

- 规则:[节点库](README.md#节点库)(`RealizedVol`)
- 级别:table
- 里程碑:M3
- 前置:`RealizedVol::new(4)`(FEA-003.3 为 `new(2)`)
- 实现:`crates/hft-feature/tests/nodes.rs::fea_003_realized_vol`

| 行 | 输入 | 期望 |
|---|---|---|
| FEA-003.1 | 环长 4;第 0..=4 秒 2 × 中间价 2 000 000、2 000 200 交替 | 4 个样本;`sigma(4)` = 99 995、`sigma(5)` = `None`;`recent(2)` = (10, 199 990) |
| FEA-003.2 | 同一秒再来一条;第 6 秒一条 | 样本数不变;最旧的出环,`recent(1)` = (100 000, 100 000) |
| FEA-003.3 | 中间价 0;10 → 1 000 | 不记样本;收益截到 1e9 |
| FEA-003.4 | `export` | (`sigma(1)`, 4);列名 `sigma`、`samples` |

### FEA-004 最优报价

- 规则:[节点库](README.md#节点库)(`BboTop`)
- 级别:table
- 里程碑:M3
- 前置:`BboTop::default()`
- 实现:`crates/hft-feature/tests/nodes.rs::fea_004_bbo_top`

| 行 | 输入 | 期望 |
|---|---|---|
| FEA-004.1 | 买一或卖一为 0 | 返回假,状态不变 |
| FEA-004.2 | 99 × 30 / 101 × 10 | `mid_x2` 200、`imb` 5e8 |
| FEA-004.3 | 100 × 0 / 104 × 0 | `mid_x2` 204,`imb` 保留 5e8 |

### FEA-005 逐秒历史

- 规则:[节点库](README.md#节点库)(`SecHist`)
- 级别:table
- 里程碑:M3
- 前置:`SecHist::<8>::default()`
- 实现:`crates/hft-feature/tests/nodes.rs::fea_005_second_history`

| 行 | 输入 | 期望 |
|---|---|---|
| FEA-005.1 | 环长 8;第 100 秒写 1 再写 2,第 101 秒写 3 | `at(100)` = 2、`at(101)` = 3、`at(103)` = 3、`at(104)` 与 `at(99)` = `None` |
| FEA-005.2 | 第 108 秒写 9 | 覆盖第 100 秒的槽:`at(100)` = `None`、`at(108)` = 9 |

### FEA-006 带符号成交流

- 规则:[节点库](README.md#节点库)(`SignedFlow`)
- 级别:table
- 里程碑:M3
- 前置:`SignedFlow::<4>::default()`;x、o 为一对 inst
- 实现:`crates/hft-feature/tests/nodes.rs::fea_006_signed_flow`

| 行 | 输入 | 期望 |
|---|---|---|
| FEA-006.1 | 环长 4:x +50、o +80、x −60、其他 inst +1 000 | 朝 x 的净量 −90;自时刻 12 起 −60;朝 o 为 +90 |
| FEA-006.2 | 第 5 笔 x +7 | 覆盖最旧一笔:−133 |

### FEA-007 扣除自有挂单的盘口一侧

- 规则:[节点库](README.md#节点库)(`OwnExcludedSide`)
- 级别:table
- 里程碑:M3
- 前置:测试簿(买盘、卖盘各若干档,`Live`);`OwnExcludedSide::<64>`(FEA-007.4 为 `<2>`)
- 实现:`crates/hft-feature/tests/nodes.rs::fea_007_own_excluded_side`

| 行 | 输入 | 期望 |
|---|---|---|
| FEA-007.1 | 买盘 453×30、450×120、447×40、440×60、420×200;自有 453×30 | `best` 450、`top_size` 120、`depth(10, 5)` 380 |
| FEA-007.2 | 自有 450×20;无自有 | `best` 453、`top_size` 30、`size_at(450)` 100;`depth(10, 5)` 380(只计整数倍价) |
| FEA-007.3 | 卖盘 470×100、475×20、491×5 | `best` 470;`depth(10, 3)` 100;`depth(5, 2)` 120 |
| FEA-007.4 | 只读 2 档 | `depth(10, 5)` 120 |

### FEA-008 行情延迟分位

- 规则:[节点库](README.md#节点库)(`LagQuantiles`)
- 级别:table
- 里程碑:M3
- 前置:`LagQuantiles::new()`
- 实现:`crates/hft-feature/tests/nodes.rs::fea_008_lag_quantiles`

| 行 | 输入 | 期望 |
|---|---|---|
| FEA-008.1 | 未结算任何分钟时导出 | 三列 `NA` |
| FEA-008.2 | 第 60 秒 10..=19 ms、第 61 秒 30 ms、第 62 秒 3、1 ms;再来一条第 61 秒的样本 | `late` = 1 |
| FEA-008.3 | UTC 进入第 2 分钟 | `done_min` 1、`done_p90` 1 810 000 000、`done_p50` 1 450 000 000(×100 ns) |
| FEA-008.4 | `gates` 于 130 s:负载阈值 18.1 ms、18.100001 ms;慢阈值 14.499999 ms;于 180 s | (真, 假)、(假, 假)、(假, 真);(假, 假) |
| FEA-008.5 | 导出 | (1, 1 810 000 000, 1 450 000 000) |

### FEA-009 复合 z 分数

- 规则:[节点库](README.md#节点库)(`zsum`)
- 级别:table
- 里程碑:M3
- 前置:三个特征
- 实现:`crates/hft-feature/tests/nodes.rs::fea_009_zscore_sum`

| 行 | 输入 | 期望 |
|---|---|---|
| FEA-009.1 | x = (3, 缺, −1)、mu = (1, 0, 0)、sd = (1, 1e-9, 2)、sign = (1, 1, −1)、√N = 2 | 1.25e9 |
| FEA-009.2 | 第一个 sd 为 0;√N = 0 | 2.5e8;0 |

### FEA-010 导出行与事件种类

- 规则:[Feature 契约](README.md#feature-契约)
- 级别:table
- 里程碑:M3
- 前置:2 格的导出缓冲
- 实现:`crates/hft-feature/tests/nodes.rs::fea_010_feature_row_and_events`

| 行 | 输入 | 期望 |
|---|---|---|
| FEA-010.1 | 2 格缓冲写 5、`None`、9 | 缓冲 (5, `NA`),写出计数 3 |
| FEA-010.2 | 事件位的并与包含;`SecHist`、`SignedFlow`、`BboTop` 的 `EVENTS` | 按位;ANY、TRADE、BBO |
