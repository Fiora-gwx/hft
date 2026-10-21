---
component: binary-maker
prefix: BMK
spec: README.md
---

# binary-maker 测试规格

格式与规则见 [docs/tests](../../docs/tests/README.md)。

本文件的记号:夹具 `tests/common`:二元 venue `VB` 的六个槽位 inst(`U0, D0, U1, D1, U2, D2`;px_exp −3、qty_exp −6、计价 USDC)与参考 venue `VR` 的现货 inst `R`(px_exp −2);Sim 账户 `A1`;group 表 = 六个槽位按序加参考。参数:默认值且 `enabled = 1`、`vol_min_samples = 60`。`warm(secs, end_s, pair)`:参考价在 `end_s` 之前 `secs` 秒里每秒一个报价,中间价在 100 000.00 与 100 010.00 之间交替(σ ≈ 1 bp/s),随后该对的两个槽位簿为买一 0.450、卖一 0.470,再在 `end_s + 0.5 s` 送一条参考报价。测试里 UTC = 引擎时间。

## 覆盖

| README 小节 | 用例 |
|---|---|
| 概要 | BMK-003 |
| 目录 | 不适用:非规范 |
| 使用本组件 | 不适用:见子节 |
| group 形态 | BMK-001 |
| 参数 | BMK-003, BMK-004, BMK-008 |
| 订阅 | BMK-003、BMK-009 |
| 波动率 | BMK-002 |
| 规则 | BMK-003–BMK-005, BMK-007, BMK-008, BMK-010–BMK-012 |
| Note 码表与报表字段 | BMK-003 |
| 实现说明 | BMK-006 |
| 延迟与资源契约 | BMK-003 |
| 测试与验收 | 不适用:非规范 |
| 依赖 | 不适用:非规范 |
| 延伸阅读 | 不适用:非规范 |
| 已知限制与后续工作 | 不适用:非规范 |
| 开发备忘 | 不适用:非规范 |

## 用例

### BMK-001 group 形态

- 规则:[group 形态](README.md#group-形态)
- 级别:table
- 里程碑:M3
- 前置:默认参数
- 实现:`strategies/binary-maker/tests/scenario.rs::bmk_001_group_shape`

| 行 | group 表 | 期望 |
|---|---|---|
| BMK-001.1 | 夹具的七个 group | `Ok` |
| BMK-001.2 | 去掉第一个(六个) | `GroupShape{gix 0, code 1}` |
| BMK-001.3 | 第 0 个改为两腿 | `GroupShape{gix 0, code 2}` |
| BMK-001.4 | 参考 group 的腿改到槽位 venue | `GroupShape{gix 6, code 3}` |

### BMK-002 1 秒收益的实现波动率

- 规则:[波动率](README.md#波动率)
- 级别:table
- 里程碑:M3
- 前置:`Vol::new(4)`
- 实现:`strategies/binary-maker/tests/scenario.rs::bmk_002_volatility`

| 行 | 输入 | 期望 |
|---|---|---|
| BMK-002.1 | 第 0–4 秒中间价 ×2 依次为 2 000 000、2 000 200、2 000 000、2 000 200、2 000 000 | 4 个样本;σ ≈ 1 bp(99 000–100 100) |
| BMK-002.2 | `sigma(5)` | `None` |
| BMK-002.3 | 同一秒再一条报价;下一秒一条 | 前者不增加样本;后者使环满时样本数仍为 4 |
| BMK-002.4 | 整数平方根 0、1、99、100、2¹⁰⁰ | 0、1、9、10、2⁵⁰ |

### BMK-003 当前窗口两个代币各挂一张买一价的 post-only 买单

- 规则:[规则](README.md#规则)、[订阅](README.md#订阅)
- 级别:unit
- 里程碑:M3
- 前置:默认夹具
- 步骤:
  1. `warm(400, 399, 1)`(窗口 300–600 为第 1 对,剩余 201 s)
- 期望:最后一个事件恰好两条 `place`:`U1`、`D1` 各一条 `{Buy, PostOnly, px 0.450, qty 10 份, venue VB}`;不分配(计数分配器)
- 实现:`strategies/binary-maker/tests/scenario.rs::bmk_003_quotes_both_tokens_of_the_current_window`

### BMK-004 不满足条件时不挂单

- 规则:[规则](README.md#规则)、[参数](README.md#参数)
- 级别:table
- 里程碑:M3
- 前置:默认夹具;各行按"条件"列改参数或行情
- 实现:`strategies/binary-maker/tests/scenario.rs::bmk_004_no_quotes_when_gated`

| 行 | 条件 | 期望 |
|---|---|---|
| BMK-004.1 | 参考价 400 秒不变(σ = 0),再送 `U1` 簿事件 | 无 `place` |
| BMK-004.2 | `warm(400, 549, 1)`(剩余 51 s < 60 s) | 无 `place` |
| BMK-004.3 | `enabled = 0` | 无 `place` |
| BMK-004.4 | `p_min = 0.5`(买一 0.450 低于它) | 无 `place` |

### BMK-005 公允价撤单与冷却

- 规则:[规则](README.md#规则)
- 级别:table
- 里程碑:M3
- 前置:`warm(400, 399, 1)`,两张挂单(`U1` 槽 11、`D1` 槽 12)在 0.450 存活;挂单时参考中间价 100 005.00、Up 中间价 0.460、σ ≈ 1 bp/s、剩余 200.5 s(0.5 美分的公允价变化约等于参考价 0.2 bp)
- 步骤:依次执行下表各行(每行从前置重新开始)
- 实现:`strategies/binary-maker/tests/scenario.rs::bmk_005_fair_value_cancel`

| 行 | 输入(399.6 s 起) | 期望 |
|---|---|---|
| BMK-005.1 | 参考中间价 100 004.00(−0.1 bp) | 不撤单 |
| BMK-005.2 | 100 000.00(−0.5 bp) | `dfv(U1)` ≤ −0.005;只撤 `U1`(槽 11) |
| BMK-005.3 | 100 010.00(+0.5 bp) | 只撤 `D1`(槽 12) |
| BMK-005.4 | 同 .2,399.7 s `U1` 的 `Cancelled`;400.6 s、401.7 s 各一条参考报价 | 399.7 s 与 400.6 s 无 `place`(冷却 2 s);401.7 s 一条 `U1` 的 `place` |

### BMK-006 快照往返

- 规则:[实现说明](README.md#实现说明)
- 级别:unit
- 里程碑:M3
- 前置:默认夹具
- 步骤:
  1. `warm(120, 399, 1)`
  2. snapshot → restore → snapshot
- 期望:两次快照字节相同
- 实现:`strategies/binary-maker/tests/scenario.rs::bmk_006_snapshot_roundtrip`

### BMK-007 改价与撤单

- 规则:[规则](README.md#规则)
- 级别:unit
- 里程碑:M3
- 前置:`warm(400, 399, 1)`;`U1`、`D1` 的挂单为 Live(槽 11、12),价 0.450
- 步骤:
  1. 399.9 s `U1` 买一变为 0.460,送 `U1` 簿事件
  2. 541 s(剩余 59 s)送参考报价
- 期望:
  - 步骤 1:只撤槽 11;无 `place`
  - 步骤 2:撤槽 12
- 实现:`strategies/binary-maker/tests/scenario.rs::bmk_007_requote_and_pull`

### BMK-008 库存上限

- 规则:[规则](README.md#规则)、[参数](README.md#参数)
- 级别:unit
- 里程碑:M3
- 前置:`U1` 所在 group 持有 100 份(`inv_max_shares = 100`)
- 步骤:
  1. `warm(400, 399, 1)`
- 期望:最后一个事件只有 `D1` 的 `place`
- 实现:`strategies/binary-maker/tests/scenario.rs::bmk_008_inventory_cap`

## 验收

- 本组件完成的标准:里程碑已到的用例全部有同编号的实现且通过。
- 上线前在 `sim` 实例连续运行两天(规则归 [testing ⑦](../../docs/testing.md#8-上线门槛)与模拟盘提案 `.agents/notes/proposed/architecture/2026-10-20-polymarket-sim-maker.md`)。

### BMK-009 订阅与成交事件

- 规则:[订阅](README.md#订阅)
- 级别:table
- 里程碑:M3
- 前置:默认夹具;`SubsBuilder` 的 feeds 含 `VB`、`VR`
- 步骤:依次执行下表各行
- 实现:`strategies/binary-maker/tests/scenario.rs::bmk_009_subscriptions_and_trades`

| 行 | 输入 | 期望 |
|---|---|---|
| BMK-009.1 | `subscriptions` | 6 个槽位腿 `BOOK | TRADE`、参考腿 `R` `BBO`,全部 `Part::Fast`,按 gix 顺序 |
| BMK-009.2 | `warm(400, 399, 1)` 后 `U1` 一条成交 | 不产生任何意图;不分配 |

### BMK-010 本价位吃空时留守

- 规则:[规则](README.md#规则)
- 级别:unit
- 里程碑:M3
- 前置:同 BMK-005
- 步骤:
  1. 399.6 s `U1` 簿买一 0.440、卖一 0.470
  2. 399.7 s `U1` 簿买一 0.460
- 期望:
  - 步骤 1:无任何动作(挂单价 0.450 高于买一,留守)
  - 步骤 2:撤 `U1`(槽 11)
- 实现:`strategies/binary-maker/tests/scenario.rs::bmk_010_stay_when_the_level_clears`

### BMK-011 对价上限

- 规则:[规则](README.md#规则)
- 级别:table
- 里程碑:M3
- 前置:默认夹具;`group_pos(A1, gid(3))` = Down `D1` 持 50 份、平均成本见下表
- 步骤:依次执行下表各行
- 实现:`strategies/binary-maker/tests/scenario.rs::bmk_011_pair_cost_limit`

| 行 | 输入 | 期望 |
|---|---|---|
| BMK-011.1 | 成本 0.560;`warm(400, 399, 1)` | `U1` @ 0.430(1 − 0.560 − 0.01,低于买一 0.450);`D1` @ 0.450 |
| BMK-011.2 | 两张挂单在 0.450 存活后加上成本 0.560 的 Down 仓位;399.6 s `U1` 的簿事件 | 撤 `U1`(高于上限 0.430) |
| BMK-011.3 | 成本 0.950(上限 0.040 < p_min);`warm(400, 399, 1)` | 只有 `D1` 的 `place` |

### BMK-012 整数正态分布函数

- 规则:[规则](README.md#规则)
- 级别:table
- 里程碑:M3
- 前置:无
- 步骤:依次执行下表各行
- 实现:`strategies/binary-maker/tests/scenario.rs::bmk_012_normal_cdf`

| 行 | 输入 | 期望 |
|---|---|---|
| BMK-012.1 | Φ(0)、Φ(±1)、Φ(1.96)、Φ(9) | 0.5、0.841345、0.158655、0.975002、0.9999997,误差 ≤ 2e-5 |
| BMK-012.2 | p = 0.01, 0.02, …, 0.99:Φ(Φ⁻¹(p));Φ⁻¹(0);Φ⁻¹(0.5) | 与 p 相差 ≤ 1e-3;Φ⁻¹(0) = Φ⁻¹(0.001);Φ⁻¹(0.5) ≈ 0 |

