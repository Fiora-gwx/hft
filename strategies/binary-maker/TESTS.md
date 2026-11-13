---
component: binary-maker
prefix: BMK
spec: README.md
---

# binary-maker 测试规格

格式与规则见 [docs/tests](../../docs/tests/README.md)。

本文件的记号:夹具 `tests/common`:二元 venue `VB` 的六个槽位 inst(`U0, D0, U1, D1, U2, D2`;px_exp −3、qty_exp −6、计价 USDC)与参考 venue `VR` 的现货 inst `R`(px_exp −2);第二参考 venue `VR2`(OKX 现货,代替原生 Coinbase)的 inst `R2`;Sim 账户 `A1`;group 表 = 六个槽位按序加参考(`specs2` 再加 `R2`)。参数:默认值且 `enabled = 1`、`vol_min_samples = 60`。`warm(secs, end_s, pair)`:参考价在 `end_s` 之前 `secs` 秒里每秒一个报价,中间价在 100 000.00 与 100 010.00 之间交替(σ ≈ 1 bp/s),随后该对的两个槽位簿为买一 0.450、卖一 0.470,再在 `end_s + 0.5 s` 送一条参考报价。测试里 UTC = 引擎时间。

## 覆盖

| README 小节 | 用例 |
|---|---|
| 概要 | BMK-003 |
| 目录 | 不适用:非规范 |
| 使用本组件 | 不适用:见子节 |
| group 形态 | BMK-001, BMK-049, BMK-051 |
| 参数 | BMK-003, BMK-004, BMK-008, BMK-016, BMK-022, BMK-024, BMK-033 |
| 订阅 | BMK-003、BMK-009、BMK-026、BMK-051 |
| 波动率 | BMK-002 |
| 特征 | BMK-034、BMK-035 |
| 规则 | BMK-052、BMK-053、BMK-054、BMK-055、BMK-057、BMK-058、BMK-059、BMK-003–BMK-005, BMK-007, BMK-008, BMK-010–BMK-021, BMK-023, BMK-025–BMK-034, BMK-036–BMK-038 |
| Note 码表与报表字段 | BMK-003 |
| 实现说明 | BMK-006, BMK-017, BMK-025, BMK-030, BMK-034 |
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
  - 步骤 2:撤槽 12;槽 11 的撤单(399.9 s)超过 `stuck_reset_ms` 没有终态回报,再撤一次(规则 8)
- 实现:`strategies/binary-maker/tests/scenario.rs::bmk_007_requote_and_pull`

### BMK-008 库存上限

- 规则:[规则](README.md#规则)、[参数](README.md#参数)
- 级别:table
- 里程碑:M3
- 前置:`U1` 所在 group 持有的份数见下表(`inv_max_shares = 100`,`clip_shares = 10`)
- 步骤:依次执行下表各行,每行 `warm(400, 399, 1)`
- 实现:`strategies/binary-maker/tests/scenario.rs::bmk_008_inventory_cap`

| 行 | `U1` 持有 | 期望(最后一个事件的 `place`) |
|---|---|---|
| BMK-008.1 | 100 份 | 只有 `D1` |
| BMK-008.2 | 90 份(90 + 10 = 100) | `U1` 与 `D1` |
| BMK-008.3 | 90.01 份(成交尾数;90.01 + 10 > 100) | 只有 `D1` |

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
| BMK-011.4 | `pair_margin` = −0.05;成本 0.600;`warm(400, 399, 1)` | `U1` @ 0.450(上限 1 − 0.600 + 0.05 = 0.450,对价 1.050);`D1` @ 0.450 |
| BMK-011.5 | `pair_margin` = −0.05;成本 0.640;`warm(400, 399, 1)` | `U1` @ 0.410(上限 1 − 0.640 + 0.05);`D1` @ 0.450 |
| BMK-011.6 | schema 中 `pair_margin` 的范围 | min = −0.10(−100 000 000),max = 1 |

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

### BMK-013 不追价

- 规则:[规则](README.md#规则)
- 级别:table
- 里程碑:M3
- 前置:同 BMK-005;每行从前置重新开始
- 步骤:399.6 s `U1` 簿买一 0.460(被抢先);399.7 s `U1` 的 `Cancelled`;之后按表中时刻送 `U1` 的簿事件(买一 0.460)
- 实现:`strategies/binary-maker/tests/scenario.rs::bmk_013_nochase_after_undercut`

| 行 | 参数 | 期望 |
|---|---|---|
| BMK-013.1 | 默认(`nochase_ms` 0) | 399.6 s 撤 `U1`;399.9 s 在 0.460 重挂 |
| BMK-013.2 | `nochase_ms` 1000 | 399.6 s 撤 `U1`;399.9 s、400.5 s 不挂;400.7 s 在 0.460 重挂 |

### BMK-014 连续成交暂停

- 规则:[规则](README.md#规则)
- 级别:table
- 里程碑:M3
- 前置:同 BMK-005;`U1` 在 399.6 s 部分成交 5 份、399.8 s 成交完 5 份
- 步骤:依次执行下表各行(每行从前置重新开始),之后在表中时刻送 `U1` 的簿事件(买一 0.450)
- 实现:`strategies/binary-maker/tests/scenario.rs::bmk_014_streak_pause`

| 行 | 参数 / 输入 | 期望 |
|---|---|---|
| BMK-014.1 | 默认(`streak_k` 0) | 400.1 s 重挂 |
| BMK-014.2 | `streak_k` 2、`streak_window_ms` 10 000、`streak_pause_ms` 10 000 | 400.1 s、409.5 s 不挂;409.7 s 重挂(倒数第 2 笔 399.6 s 已在 10 s 之外) |
| BMK-014.3 | 同 .2,但第一笔在 387.0 s | 400.1 s 重挂(两笔相隔 12.8 s,不算连续) |

### BMK-015 每窗成本上限

- 规则:[规则](README.md#规则)
- 级别:table
- 里程碑:M3
- 前置:`max_window_cost` 20 USDC
- 步骤:依次执行下表各行
- 实现:`strategies/binary-maker/tests/scenario.rs::bmk_015_window_cost_cap`

| 行 | 输入 | 期望 |
|---|---|---|
| BMK-015.1 | 持 50 份 `U1` @0.450(22.50 USDC);`warm(400, 399, 1)` | 两腿都不挂 |
| BMK-015.2 | 持 30 份 @0.450(13.50 USDC);`warm(400, 399, 1)` | 两腿各一条 `place` |
| BMK-015.3 | 两张挂单存活后加上 50 份 @0.450 的持仓;399.6 s `U1` 簿事件 | 不撤单(上限只挡新挂单) |


### BMK-016 撤单没有终态回报时复位

- 规则:[规则](README.md#规则) 8、[参数](README.md#参数)
- 级别:table
- 里程碑:M3
- 前置:同 BMK-005;399.6 s `U1` 簿买一 0.460(被抢先,撤槽 11),之后没有槽 11 的回报
- 步骤:依次执行下表各行
- 实现:`strategies/binary-maker/tests/scenario.rs::bmk_016_stuck_cancel_reset`

| 行 | 输入 | 期望 |
|---|---|---|
| BMK-016.1 | 默认 `stuck_reset_ms` 5 000;404.5 s、404.7 s 各一条 `U1` 簿事件(买一 0.460) | 404.5 s 无动作;404.7 s 再撤槽 11,并在 0.460 挂 `U1` |
| BMK-016.2 | 接 .1,`U1` 新挂单为槽 21 在 0.460 存活;404.8 s 槽 11 的 `Cancelled` | 无动作;`U1` 仍是槽 21 的 Live |
| BMK-016.3 | `stuck_reset_ms` 10 000;404.7 s `U1` 簿事件 | 无动作 |
| BMK-016.4 | schema | 下标 22(第 23 个)字段 `stuck_reset_ms`,`since` 4,默认 5 000;`n_fields_at(3)` = 22,`n_fields_at(4)` = 23 |

### BMK-017 槽位对复用时清除上一窗口的状态

- 规则:[规则](README.md#规则) 8、[实现说明](README.md#实现说明)
- 级别:unit
- 里程碑:M3
- 前置:`warm(400, 1199, 0)`;`U1` 为窗口 300 的 Live 挂单(槽 11),`D1` 为窗口 300 的撤单在途(槽 12,1 298.6 s 撤单);`U1`、`D1` 簿买一 0.450、卖一 0.470
- 步骤:
  1. 1 299.6 s 参考报价(窗口 1200–1500 为第 1 对)
  2. `U1` 新挂单为槽 21(窗口 1200)存活;1 299.7 s 槽 11 的 `Filled{done}`
- 期望:
  - 步骤 1:撤槽 11、12 各一次;`U1`、`D1` 各一条 `place`
  - 步骤 2:无撤单、无 `U1` 的 `place`;`U1` 仍是槽 21 的 Live
- 实现:`strategies/binary-maker/tests/scenario.rs::bmk_017_window_change_clears_stale_quotes`

### BMK-018 锁住或交叉的簿不算被抢先

- 规则:[规则](README.md#规则) 6
- 级别:table
- 里程碑:M3
- 前置:同 BMK-005
- 步骤:依次执行下表各行
- 实现:`strategies/binary-maker/tests/scenario.rs::bmk_018_locked_book_is_not_an_undercut`

| 行 | 输入 | 期望 |
|---|---|---|
| BMK-018.1 | 399.6 s `U1` 簿买一 0.460、卖一 0.460 | 无动作 |
| BMK-018.2 | 399.7 s 买一 0.470、卖一 0.460 | 无动作 |
| BMK-018.3 | 399.8 s 买一 0.460、卖一 0.470 | 撤 `U1`(槽 11) |

### BMK-019 `Aged` 时撤单

- 规则:[规则](README.md#规则) 8
- 级别:unit
- 里程碑:M3
- 前置:同 BMK-005
- 步骤:
  1. 399.6 s 槽 11 的 `Aged`
- 期望:撤槽 11;无 `U1` 的 `place`;`U1` 为撤单在途
- 实现:`strategies/binary-maker/tests/scenario.rs::bmk_019_aged_quote_is_cancelled`

### BMK-020 公允价上限

- 规则:[规则](README.md#规则) 4、6、9
- 级别:table
- 里程碑:M3
- 前置:`fv_limit` 1、`inv_skew` 0、`pred_k` 0,其余同 BMK-003;`warm(400, 399, 1)`(Up 中间价 0.460)
- 步骤:依次执行下表各行
- 实现:`strategies/binary-maker/tests/scenario.rs::bmk_020_fair_value_limit`

| 行 | 输入 | 期望 |
|---|---|---|
| BMK-020.1 | 预热 | `U1`、`D1` 各在 0.450 挂单(上限约 0.46 / 0.54) |
| BMK-020.2 | `inv_skew` 0.195,持有 `U1` 50 份(成本 0.450),预热 | `U1` 在 0.360(0.46 − 0.0975 向下取整)、`D1` 在 0.450 |
| BMK-020.3 | `fv_cancel` 0;两腿在 0.450 存活(槽 11、12);399.6 s 参考中间价升 5 bp(100 055.00) | 只撤 `D1`(槽 12):Down 上限约 0.44 < 0.450 − 0.005;无 `place` |
| BMK-020.4 | 接 .3,399.9 s 槽 12 的 `Cancelled` | `D1` 在 0.450 挂单(锚定已吸收约 30%,Down 上限约 0.461) |
| BMK-020.5 | 同 .3、.4,`fv_anchor_ms` 600 000 | 399.6 s 撤槽 12;399.9 s `D1` 在 0.420 挂单(上限约 0.427) |
| BMK-020.6 | `fv_limit` 0、`fv_cancel` 0;同 .3 | 无撤单 |

### BMK-021 方向预测

- 规则:[规则](README.md#规则) 9
- 级别:table
- 里程碑:M3
- 前置:`warm(400, 399, 1)`(最近 5 个 1 秒收益 +1、−1、+1、−1、+1 bp)
- 实现:`strategies/binary-maker/tests/scenario.rs::bmk_021_forecast`

| 行 | 输入 | 期望 |
|---|---|---|
| BMK-021.1 | `pred_w_ret` = `pred_w_ser` = 0;399.6 s 参考报价买一数量 3、卖一数量 1 | 失衡 0.5;预测 = 1.37249 × 0.5 = 0.686245 |
| BMK-021.2 | `pred_w_imb` = `pred_w_ser` = 0,`pred_w_ret` 1 | `recent(5)` = (100 020, 499 980);预测 ≈ 0.4473(±0.001) |
| BMK-021.3 | 只有 `pred_w_ser` 1 | 预测 ≈ 0.0667(最近 15 个:8 涨 7 跌) |
| BMK-021.4 | `pred_w_imb` 10、失衡 1 | 截到 6 |

### BMK-022 schema v5

- 规则:[参数](README.md#参数)
- 级别:table
- 里程碑:M3
- 前置:无
- 实现:`strategies/binary-maker/tests/scenario.rs::bmk_022_schema_v5`

| 行 | 输入 | 期望 |
|---|---|---|
| BMK-022.1 | schema | 版本 ≥ 5;`n_fields_at(5)` = 32 |
| BMK-022.2 | 下标 23–31 | `fv_limit` 0、`fv_anchor_ms` 1 000、`fv_edge` 0、`fv_hyst` 5 000 000、`inv_skew` 30 000 000、`pred_k` 200 000 000、`pred_w_imb` 1 372 490 000、`pred_w_ret` 121 290 000、`pred_w_ser` 187 200 000,`since` 均为 5 |

### BMK-023 预测漂移

- 规则:[规则](README.md#规则) 9
- 级别:table
- 里程碑:M3
- 前置:`fv_limit` 1、`inv_skew` 0;参考报价 0–399 s 同 `warm`,`U1`、`D1` 簿买一 0.450、卖一 0.470,399.5 s 参考报价买一数量 9、卖一数量 1(失衡 0.8)
- 实现:`strategies/binary-maker/tests/scenario.rs::bmk_023_forecast_drift`

| 行 | 输入 | 期望 |
|---|---|---|
| BMK-023.1 | `pred_k` 2 | `U1` 在 0.450;`D1` 在 0.200–0.300(公允 Up 概率约 0.73) |
| BMK-023.2 | `pred_k` 0 | `U1`、`D1` 各在 0.450 |

### BMK-024 schema v6

- 规则:[参数](README.md#参数)
- 级别:table
- 里程碑:M3
- 前置:无
- 实现:`strategies/binary-maker/tests/scenario.rs::bmk_024_schema_v6`

| 行 | 输入 | 期望 |
|---|---|---|
| BMK-024.1 | schema;`DESC_VERSION` | 版本 48;`DESC_VERSION` 7;`n_fields_at(5)` = 32,`n_fields_at(6)` = 56 |
| BMK-024.3 | 下标 56 | `dir_pair_free` 0,`since` 7;版本 7,`n_fields_at(7)` = 57 |
| BMK-024.34 | schema v38 的最后一个字段 | `ref_cb_w` 0,`since` 38;`n_fields_at(38)` = v37 字段数 + 1 |
| BMK-024.35 | schema v41 的字段(v39/v40 属于另一条研究线,本线没有) | `n_fields_at(40)` = `n_fields_at(38)`;v41 依次为 `ref_aux`、`fc_on`、`fc_s`、`fc_lam`、`fc_c0`、`fc_c1`、`fc_w`、`fc_m_post`、`fc_m_cancel`、`fc_m_pair`、`fc_iv_n`、`fc_ver`,全部 `since` 41;`ref_aux` 0、`fc_on` 0、`fc_m_pair` −1(不配对) |
| BMK-024.36 | schema v42 的字段 | 依次 `lead_w_ms` 0、`lead_x_bp` 0、`lead_life_ms` 3 000、`lead_cool_ms` 2 000、`lead_shares` 5、`lead_pc_w_ms` 0、`lead_pc_x_bp` 0、`lead_pc_block_ms` 1 000、`lead_pc_cool_ms` 1 000、`lead_only` 0,`since` 42 |
| BMK-024.37 | schema v43 的字段 | 依次 `lead_src` 1、`lead_price` 0、`lead_m` 0.5 c、`lead_maxtick` 3、`lead_k` 1,`since` 43;`n_fields_at(42)` = 总数 − 5 |
| BMK-024.38 | schema v44 的字段 | `lead_p_min` 0,`since` 44;`n_fields_at(43)` = 总数 − 14 |
| BMK-024.39 | schema v45 的字段与放宽的上限 | 依次 `lead_cf_ms` 0、`lead_cf_x_bp` 0.5 bp,`since` 45;`n_fields_at(44)` = 总数 − 13;`ref_aux` 上限 7,`lead_src` 上限 15 |
| BMK-024.40 | schema v46 的字段 | 依次 `fvl_w_ms` 0、`fvl_x_bp` 0、`fvl_gap` 0.06、`fvl_cool_ms` 2 000,`since` 46;`n_fields_at(45)` = 总数 − 11 |
| BMK-024.41 | schema v47 的字段 | 依次 `take_on` 0、`take_p_min` 0.55、`take_p_max` 0.85、`take_slip_c` 1、`take_max_win` 1、`take_max_hour` 0,`since` 47;`n_fields_at(46)` = 总数 − 7 |
| BMK-024.42 | schema v48 的字段 | `take_one_side` 0,`since` 48;`n_fields_at(47)` = 总数 − 1 |
| BMK-024.4 | 下标 57–60 | `tilt_z`、`tilt_clip_shares`、`late_hedge_ms`、`late_hedge_z` 均为 0,`since` 8;版本 8,`n_fields_at(8)` = 61 |
| BMK-024.2 | 下标 32–55 | `comp_skip` 0、`comp_dir` 0、`dir_cap_shares` 20、`comp_every_ms` 250、`comp_flow_min_shares` 50、`comp_excl_own` 1、八个特征的 `_mu`/`_sd`(README"参数"所列值)、`load_gate_ns` 0、`mdslow_gate_ns` 0,`since` 均为 6 |

### BMK-025 定点对数

- 规则:[规则](README.md#规则) 10(对数)
- 级别:table
- 里程碑:M3
- 前置:无
- 实现:`strategies/binary-maker/tests/scenario.rs::bmk_025_integer_logarithm`

| 行 | 输入 | 期望 |
|---|---|---|
| BMK-025.1 | ln(x / 1e9),x = 1e9、2e9、10e9、31e9;x = 1e9 − 1 | 0、693 147 181、2 302 585 091、3 433 987 198(逐位);0 |
| BMK-025.2 | ln(10 001 / 10 000)、ln(10 000 / 10 001)、ln(0 / 10 000) | 99 994、−99 994、0 |
| BMK-025.3 | atanh2(−300 000 000) | −619 039 204 |
| BMK-025.4 | ln(1 + n),n = 1、7、50、999、123 456、10^7 份 | 与真值相差 ≤ 2e-8 |

### BMK-026 方向分数的特征与分数

- 规则:[规则](README.md#规则) 10
- 级别:table
- 里程碑:M3
- 前置:X = `U1` 买盘 0.453 × 30、0.450 × 120、0.447 × 40、0.440 × 60、0.420 × 200 份,其中 0.453 的 30 份是本实例的挂单;O = `D1` 买盘 0.540 × 80、0.530 × 50、0.500 × 100;UTC 1 000.4 s;X 的 M[1000] = 920、M[969] = 900、M[938] = 880,B[1000] = 453、B[998] = 460;R[1000] = 20 010 000、R[985] = 20 000 000、R[940] = 19 990 000;隐含 σ 0.000139;大单:969 s X 买 500、975 s X 买 100、990 s O 买 60、995 s X 卖 70、996 s `U0` 买 1 000 份;默认均值与标准差
- 实现:`strategies/binary-maker/tests/scenario.rs::bmk_026_composite_score`

| 行 | 输入 | 期望 |
|---|---|---|
| BMK-026.1 | 买盘读数 | X:买一 0.450(扣除自己后)、120 份、5 美分深度 380 份(0.453、0.447 不在整美分上);O:0.540、80、230 |
| BMK-026.2 | 特征 | imb1 200 000 000、imb5 245 901 639、mom30 10 000 000、mom60 20 000 000、bret15 928 539 016、bret60 928 772 947、flow30 −3 433 987 198(v = −30 份)、bdrop1 −700 000 000 |
| BMK-026.3 | 分数 | 1 192 797 279 |
| BMK-026.4 | 无逐秒记录 | mom、bret 缺失,bdrop1 0;分数 525 800 236 |
| BMK-026.5 | 规则 10 开启时 `U1` 依次成交 49 份买、50 份买,`D1` 80 份买,`U1` 60 份卖(10–13 s);关闭时同样 | 开启:`U1` 对 `D1` 的净流向 −90 份(49 份未记),13 s 起 −60 份;关闭:不记 |

### BMK-027 方向分数跳过不利的代币

- 规则:[规则](README.md#规则) 4、6、10
- 级别:table
- 里程碑:M3
- 前置:`comp_skip` = `comp_dir` = 0.5、`comp_imb1_sd` 0.1(买一失衡起决定作用);`warm(400, 399, 1)` 的参考报价,`U1`、`D1` 的买一 0.450 的数量按行给出
- 实现:`strategies/binary-maker/tests/scenario.rs::bmk_027_score_skips_the_token_it_is_against`

| 行 | 输入 | 期望 |
|---|---|---|
| BMK-027.1 | `U1` 10 份、`D1` 190 份 | 分数缓存有效(窗口 300),Up < −0.5、Down > 0.5;只有 `D1` 在 0.450 挂单 |
| BMK-027.2 | 两边各 100 份 | `U1`、`D1` 各在 0.450 挂单 |
| BMK-027.3 | 接 .2,两张挂单存活(槽 11、12),399.6 s 簿变为 10 / 190 份并来一条 `U1` 簿事件 | 无撤单、无挂单(分数仍是 399.5 s 的缓存) |
| BMK-027.4 | 399.8 s 再一条 `U1` 簿事件 | 分数重算;撤 `U1`(槽 11),不挂单 |
| BMK-027.5 | `comp_skip` 0;10 / 190 份 | 两腿都挂 |
| BMK-027.6 | `comp_every_ms` 0;同 .3 | 399.6 s 即撤槽 11 |
| BMK-027.7 | `comp_skip` −0.5(方向 maker);两边各 100 份 | 两个分数都在 ±0.5 内;不挂单 |
| BMK-027.8 | `comp_skip` −0.5;10 / 190 份 | 只有 `D1` 在 0.450 挂单 |
| BMK-027.9 | 接 .8,两腿各有一张存活挂单(槽 11、12),399.8 s 一条 `U1` 簿事件 | 分数重算;撤 `U1`(槽 11),`D1` 保留,不挂单 |

### BMK-028 方向分数放宽有利一侧的库存上限

- 规则:[规则](README.md#规则) 4、10
- 级别:table
- 里程碑:M3
- 前置:同 BMK-027,`clip_shares` 5、`inv_max_shares` 10、`dir_cap_shares` 20;`U1` 10 份、`D1` 190 份(分数有利 Down);持有 `D1` 的份数按行给出
- 实现:`strategies/binary-maker/tests/scenario.rs::bmk_028_direction_cap`

| 行 | 输入 | 期望 |
|---|---|---|
| BMK-028.1 | 持有 10 份 | `D1` 在 0.450 挂单(10 + 5 ≤ 20);`U1` 被跳过 |
| BMK-028.2 | 持有 10 份,`comp_dir` 0 | 不挂单(上限 10) |
| BMK-028.3 | 持有 16 份 | 不挂单(16 + 5 > 20) |
| BMK-028.4 | 持有 15 份 | `D1` 在 0.450 挂单 |

### BMK-029 失衡特征扣除自己的挂单

- 规则:[规则](README.md#规则) 10(特征)
- 级别:table
- 里程碑:M3
- 前置:同 BMK-027,`U1`、`D1` 买一 0.450 各 20 份;`U1` 的报价为当前窗口的 Live(槽 11);`Views::order(槽 11)` 按行给出(委托 10 份,0.450);399.8 s 一条 `U1` 簿事件后读 Up 的分数
- 实现:`strategies/binary-maker/tests/scenario.rs::bmk_029_own_bid_left_out_of_the_imbalance`

| 行 | 输入 | 期望 |
|---|---|---|
| BMK-029.1 | `Live`、未成交 | 分数比没有订单视图时低 1 以上(扣除 10 份:10 对 20) |
| BMK-029.2 | `Live`、已成交 4 份 | 介于 .1 与没有订单视图之间(扣除 6 份) |
| BMK-029.3 | `PendingNew`;或 `comp_excl_own` 0 | 与没有订单视图时相同 |

### BMK-030 行情延迟的分位与闸门判定

- 规则:[规则](README.md#规则) 11
- 级别:table
- 里程碑:M3
- 前置:`LagGate::new()`
- 实现:`strategies/binary-maker/tests/scenario.rs::bmk_030_lag_quantiles`

| 行 | 输入 | 期望 |
|---|---|---|
| BMK-030.1 | `quantile_x10`:1..10 的 90 分位;[1, 2, 3, 4] 的中位数;[7] 的 90 分位;空 | 91;25;70;`None` |
| BMK-030.2 | 60 s 的 10 个样本 10–19 ms,61 s 一个 30 ms,62 s 两个 3、1 ms;再一个 61 s 的样本 | 最后一个丢弃(`late` = 1) |
| BMK-030.3 | UTC 进入 120 s | 第 1 分钟结束:90 分位的中位数 18.1 ms、中位数的中位数 14.5 ms(× 100 ns:1 810 000 000、1 450 000 000) |
| BMK-030.4 | 130 s 判定:`load_gate_ns` 18 100 000 / 18 100 001;`mdslow_gate_ns` 14 499 999 / 14 500 000 | 关 / 开;关 / 开(中位数须大于阈值) |
| BMK-030.5 | 180 s 判定(上一分钟没有样本) | 都开 |

### BMK-031 行情延迟闸门停止挂单

- 规则:[规则](README.md#规则) 2、11
- 级别:table
- 里程碑:M3
- 前置:同 `warm(400, 399, 1)`,但参考报价在每秒的 0.9 s、最后一条在 399.95 s;`U1` 在按行给出的秒内每秒 3 条行情戳(接收于 0.0、0.2、0.4 s,延迟按行给出);399.95 s 的判定读第 5 分钟(300–359 s)
- 实现:`strategies/binary-maker/tests/scenario.rs::bmk_031_venue_load_gates`

| 行 | 输入 | 期望 |
|---|---|---|
| BMK-031.1 | `load_gate_ns` 12 688 500;300–359 s 延迟 13 ms | 无挂单 |
| BMK-031.2 | 同上,延迟 12 ms | `U1`、`D1` 各在 0.450 挂单 |
| BMK-031.3 | 同上,每秒延迟 5、5、20 ms(90 分位 17 ms,中位数 5 ms) | 无挂单 |
| BMK-031.4 | 同上,延迟 13 ms 只在 240–299 s | 两腿都挂 |
| BMK-031.5 | `mdslow_gate_ns` 40 000 000;延迟 41 ms / 40 ms | 无挂单 / 两腿都挂 |
| BMK-031.6 | 两个闸门都为 0;延迟 100 ms | 两腿都挂 |

### BMK-032 延迟闸门关闭时撤掉存活的挂单

- 规则:[规则](README.md#规则) 6、11
- 级别:unit
- 里程碑:M3
- 前置:`load_gate_ns` 12 688 500;0–420 s 每秒 0.9 s 一条参考报价(399 s 起中间价不变);`U1` 每秒 3 条行情戳,延迟在 359 s 及以前 12 ms、360 s 起 13 ms;399.9 s 起 `U1`、`D1` 簿买一 0.450、卖一 0.470
- 步骤:400.9 s 的判定挂出两腿,随后视为存活(槽 11、12);继续到 420.9 s
- 期望:401.9–419.9 s 不撤单;420.9 s(第 7 分钟读第 6 分钟的 13 ms)撤槽 11、12
- 实现:`strategies/binary-maker/tests/scenario.rs::bmk_032_gate_cancels_resting_quotes`

### BMK-033 v7 的规则关闭时决定与 v6 相同

- 规则:[参数](README.md#参数)(schema v6 字段的默认值)、[规则](README.md#规则) 10、11
- 级别:unit
- 里程碑:M3
- 前置:两组参数:默认;默认但 `dir_cap_shares` 1、`comp_every_ms` 0、`comp_flow_min_shares` 0、`comp_excl_own` 0、`comp_imb1_mu` 0.9、`comp_imb1_sd` 与 `comp_bdrop1_sd` 1e-9(`comp_skip`、`comp_dir`、`load_gate_ns`、`mdslow_gate_ns` 仍为 0)
- 步骤:同一事件序列:`U1` 10 份、`D1` 190 份的预热,随后 40 个事件(每 150 ms 一个,轮流为簿变化、参考报价、`U1` 80 份的成交、`D1` 的行情戳与簿事件、槽 12 的 `Cancelled`)
- 期望:两组参数每个事件的意图逐项相同,且序列中有挂单
- 实现:`strategies/binary-maker/tests/scenario.rs::bmk_033_v7_off_is_v6`

### BMK-034 生成事件流上的决定轨迹固定

- 规则:[规则](README.md#规则) 1–11、[实现说明](README.md#实现说明)
- 级别:unit
- 里程碑:M3
- 前置:四组参数(默认;`fv_limit` + `nochase_ms` 500 + `streak_k` 2 + `max_window_cost` 30 USDC;加方向分数;加两道延迟闸门),每组在 1 100 s 换成下一组;固定种子的 xorshift64* 事件流:2 400 s 内每 20–300 ms 一个事件(参考报价、槽位簿(含锁住/交叉)、成交、行情戳(300–900 s 为高延迟段)、存活挂单的成交/撤单/超时/确认、定时器),多数挂单被确认为存活订单,成交累计到 group 持仓,每个窗口清零
- 步骤:逐事件对捕获的意图、各槽报价状态、分数缓存与公允价锚做 FNV-1a,最后加快照字节
- 期望:四组的 (哈希, 下单数, 撤单数, 分数有效的事件数, 成交数) 等于用 v7 内联实现记录的值;特征实现迁移(决定不变的修复)后不变
- 实现:`strategies/binary-maker/tests/scenario.rs::bmk_034_decision_trace_is_stable`

### BMK-035 特征导出一行定点列

- 规则:[特征](README.md#特征)
- 级别:unit
- 里程碑:M3
- 前置:默认参数;`warm(400, 399, 1)`(400 个参考报价样本,最后一条参考报价中间价 100 005.00)
- 步骤:`MakerFeatures::export` 写入 7 列的缓冲;另以 9 列的缓冲与 3 列的缓冲各写一次
- 期望:列名与 `features::COLUMNS` 一致;值依次为 `ref_mid_x2` 20 001 000、`ref_imb` 0、`ref_sigma` = `RealizedVol::sigma(1)`、`ref_samples` 399、三个延迟列 `NA`(延迟闸门关闭);较长的缓冲多出的两格不被写,较短的缓冲只写前三列且写出计数仍为 7;`Strategy::FEATURES` 等于 `COLUMNS`,`Strategy::export_features` 写出的 9 格缓冲与 `MakerFeatures::export` 的相同
- 实现:`strategies/binary-maker/tests/scenario.rs::bmk_035_feature_export_row`

### BMK-036 被看好的一侧凑对不受对价上限

- 规则:[规则](README.md#规则) 3、10;参数 `dir_pair_free`
- 级别:table
- 里程碑:M3
- 前置:`comp_skip` = `comp_dir` = 0.5、`comp_imb1_sd` 0.1、`clip_shares` 5、`inv_max_shares` 10;持有 10 份 Down(均价 0.600);`warm(400, 399, 1)`,`U1` 买一 190 份、`D1` 10 份(分数看好 Up,Down 被跳过)
- 实现:`strategies/binary-maker/tests/scenario.rs::bmk_036_favoured_side_pairs_without_the_limit`

| 行 | 输入 | 期望 |
|---|---|---|
| BMK-036.1 | `dir_pair_free` 0 | `U1` 在对价上限 0.390(1 − 0.600 − 0.01)挂单 |
| BMK-036.2 | `dir_pair_free` 1 | `U1` 在买一 0.450 挂单 |
| BMK-036.3 | `dir_pair_free` 1、`comp_dir` 0 | 没有被看好的代币:`U1` 在 0.390 |

### BMK-037 倾斜:分数偏向的一侧挂大单

- 规则:参数 `tilt_z`、`tilt_clip_shares`
- 级别:table
- 里程碑:M3
- 前置:`comp_skip` = `comp_dir` = 5(不跳过、不加方向上限)、`comp_imb1_sd` 0.1、`clip_shares` 5、`inv_max_shares` 20;`U1` 买一 140 份、`D1` 60 份
- 实现:`strategies/binary-maker/tests/scenario.rs::bmk_037_tilt_quotes_the_leaning_side_larger`

| 行 | 输入 | 期望 |
|---|---|---|
| BMK-037.1 | 关 | 两腿各 5 份 |
| BMK-037.2 | `tilt_z` 0.1、`tilt_clip_shares` 8 | `U1` 8 份、`D1` 5 份 |
| BMK-037.3 | `tilt_clip_shares` 0 | 两腿各 5 份 |

### BMK-038 最后一分钟按信号止损凑对

- 规则:参数 `late_hedge_ms`、`late_hedge_z`
- 级别:table
- 里程碑:M3
- 前置:`comp_params`、`clip_shares` 5、`inv_max_shares` 10、`stop_before_ms` 250 000;持有 10 份 Down(0.600);`U1` 买一 190 份、`D1` 10 份(分数看好 Up)
- 实现:`strategies/binary-maker/tests/scenario.rs::bmk_038_last_minute_stop_loss_pairs_on_the_signal`

| 行 | 输入 | 期望 |
|---|---|---|
| BMK-038.1 | `late_hedge_ms` 0 | 不挂单 |
| BMK-038.2 | `late_hedge_ms` 10 000、`late_hedge_z` 0.5 | `U1` 在买一 0.450 挂单(无对价上限 0.390) |
| BMK-038.3 | `late_hedge_z` 9 | 不挂单 |
| BMK-038.4 | `late_hedge_ms` 250 000 | 不挂单(已进入最后 `late_hedge_ms`) |

### BMK-045 止损配对(规则 13)

- 规则:[规则](README.md#规则) 13;参数 `rescue_*`(schema v32)
- 级别:table
- 里程碑:M3
- 前置:`comp_params`、`clip_shares` 5、`inv_max_shares` 10、`sigma_min` 1.0(主规则关闭);持有 10 份 Down(0.600);两腿买一 0.450 / 卖一 0.470,`U1` 买一 190 份、`D1` 10 份(分数看好 Up);399.5 s(窗口 300,年龄 99.5 s)
- 实现:`strategies/binary-maker/tests/scenario.rs::bmk_045_stop_loss_pairing`

| 行 | 输入 | 期望 |
|---|---|---|
| BMK-045.1 | `rescue_ms` 0 | 不挂单,记号未设 |
| BMK-045.2 | `rescue_ms` 90 000、`rescue_z` 0.5、`rescue_px` 0.50 | `U1` 在买一 0.450 挂单(无对价上限 0.390),记号 = 300 |
| BMK-045.3 | `rescue_px` 0.40;`rescue_ms` 120 000;`rescue_z` 9 | 都不挂单 |
| BMK-045.4 | 加 `rescue_cap` 0.02 | `U1` 挂 0.420 |

### BMK-046 规则 12 的 Chainlink 价源

- 规则:[规则](README.md#规则) 12(Chainlink);参数 `settle_src`、`settle_lead_usd`、`settle_edge`(schema v32)
- 级别:table
- 里程碑:M3
- 前置:同 BMK-044;Chainlink 样本(local_kind 1)在每秒参考报价之前注入
- 实现:`strategies/binary-maker/tests/scenario.rs::bmk_046_settlement_on_chainlink`

| 行 | 输入 | 期望 |
|---|---|---|
| BMK-046.1 | `settle_src` 1,Chainlink = 参考中间价 | (p_up, μ, n_lock) 与参考版相同;545 s 挂 0.950 |
| BMK-046.2 | 无样本;样本只到 541 s;`settle_src` 0 无样本 | 前两者不挂;参考版照常挂 |
| BMK-046.3 | Chainlink 第 540 s 起停在行权价 | 不挂 |
| BMK-046.4 | `settle_lead_usd` 90 / 100(领先 95 USD),两种价源 | 90 挂,100 不挂 |
| BMK-046.5 | `settle_edge` = p − 0.95 ∓ 0.001 | 前者挂,后者不挂 |
| BMK-046.6 | 载荷:正常、更早样本、版本 2、16 字节、负价 | 计 2 个样本,最新 (100, 7);98 秒有值、101 秒无;103 s 为当前价,104 s 不是 |

### BMK-047 震荡开关预热

- 规则:[规则](README.md#规则) 13 之后"震荡开关预热";`InjectWarmup` local_kind 2
- 级别:table
- 里程碑:M3
- 前置:载荷 v1(版本 1、条数、条数 × (窗口起点, 效率比));12 条窗口 1 000 + 300k、效率比 0.10 + 0.01k
- 实现:`strategies/binary-maker/tests/scenario.rs::bmk_047_chop_seed`

| 行 | 输入 | 期望 |
|---|---|---|
| BMK-047.1 | 空环收 12 条 | len 12、pos 12;近 12 均值 0.155,近 6 均值 0.185 |
| BMK-047.2 | 已有记录再收;版本 2 | 都不变 |
| BMK-047.3 | 正在累计窗口 2 200;条目超范围、乱序、≥ 2 200 | 只留 0.30、0.20 两条 |
| BMK-047.4 | 30 条 | 留最后 24 条,最新在最后 |
| BMK-047.5 | 经策略注入 local_kind 2 | `chop.mean(12)` = 0.155 |

### BMK-048 压力挡开仓

- 规则:[规则](README.md#规则) 14;参数 `ofi_win_ms`、`ofi_obi_max`(schema v33)
- 级别:table
- 里程碑:M3
- 前置:.4–.6 参考预热 392 s(槽位簿为空),392..398 s 每秒设对 1 的簿(Up 买一 0.450 按行、卖一 0.470 × 100;Down 0.450 × 100 / 0.470 × 100)并发 Up 的 `Book`,399.5 s 一条参考报价;只看 398 s 起的挂单
- 实现:`strategies/binary-maker/tests/scenario.rs::bmk_048_pressure_block`

| 行 | 输入 | 期望 |
|---|---|---|
| BMK-048.1 | 买一 100 → 150(同价);卖一 0.470 × 100 → 0.460 × 80 | 累计 +50,再 −30 |
| BMK-048.2 | 1.3 s 时过去 1 s / 0.5 s / 5 s;1.25 s 时 0 ms | −80、−80、`None`、0 |
| BMK-048.3 | 换合约 | 环重来(cum 0、首桶 20) |
| BMK-048.4 | Up 买一 10(失衡 −0.82)、订单流不变;`ofi_win_ms` 5 000、`ofi_obi_max` −0.3;关闭 | 开:Up 不挂、Down 挂;关:两条都挂 |
| BMK-048.5 | 397 s 起 Up 买一 30(订单流 +20,失衡 −0.54) | Up 挂 |
| BMK-048.6 | Up 买一 100(失衡 0)、订单流不变 | Up 挂 |

### BMK-044 结算窗口流动性(规则 12)

- 规则:[规则](README.md#规则) 12;参数 `settle_*`(schema v25)
- 级别:table
- 里程碑:M3
- 前置:`settle_ms` 60 000、`sigma_min` 1.0(1e9,主规则关闭),其余 `settle_*` 默认;参考每秒一条(第 100 s 起),中间价在 100 000.00 / 100 010.00 间交替,第 540 s 起 100 100.00;`U1` 买一 / 卖一按行,`D1` 为其互补
- 实现:`strategies/binary-maker/tests/scenario.rs::bmk_044_settlement_window_liquidity`

| 行 | 输入 | 期望 |
|---|---|---|
| BMK-044.1 | 555.1 s(n_lock 15、n_fut 45) | n_lock 15;μ 与浮点公式(`oct_lock.p_up_bn`,同一 σ)差 < 2e-9;p_up 差 < 1e-5;导出第 30–32 列为 (p_up, μ, n_lock);最后一分钟之前为 `NA` |
| BMK-044.2 | 买一 0.950 / 卖一 0.960;0.980;0.850;`settle_improve` 1 且 0.940 / 0.970;`settle_ms` 0 | 545 s 在 `U1` 0.950 挂 20 份、tag `settle_tag_of(2)`,之前不挂;价带外不挂;抬价挂 0.950;关闭时不挂 |
| BMK-044.3 | 挂单存活,550 s 参考回到 100 000.00 | p_up < 0.90,撤单 |
| BMK-044.4 | 买一降到 0.940;再升到 0.955 | 降:留守;升:撤单,`Cancelled` 后 `settle_requote_ms` 内不挂,之后挂 0.955 |
| BMK-044.5 | 成交 20 份,`settle_max_shares` 20 / 60 | 20:不再挂;60:再挂一张 |
| BMK-044.6 | `sigma_min` 默认、`stop_before_ms` 30 000(主规则在挂单段但无目标) | 模块到 570 s(最后 30 s)才挂 |

### BMK-049 第二参考组(Coinbase 快参考)

- 规则:[group 形态](README.md#group-形态)
- 级别:table
- 里程碑:M3
- 前置:默认参数
- 实现:`strategies/binary-maker/tests/scenario.rs::bmk_049_second_reference_group`

| 行 | 输入 | 期望 |
|---|---|---|
| BMK-049.1 | 六个槽位 + `R` + `R2` | `Ok` |
| BMK-049.2 | 去掉第一个槽位(五个槽位);四个槽位 + 三个参考 | 都是 `GroupShape{gix 0, code 1}` |
| BMK-049.3 | 八个 group 的表 | `reference` = `R`,`reference2` = `R2` |
| BMK-049.4 | 1 s 时 `R2` 的 BBO 99 999.99 / 100 000.01 | Coinbase 样本 1 个,1 s 时中间价 × 2 = 20 000 000 |
| BMK-049.5 | 再来一条 local_kind 3 的旁路样本 | 被忽略(样本仍 1 个) |
| BMK-049.6 | 夹具七个 group 的表,同一旁路样本 | `reference2` 未设,样本 1 个 |

### BMK-050 公允价参考融合(schema v38 `ref_cb_w`)

- 规则:[group 形态](README.md#group-形态)
- 级别:table
- 里程碑:M3
- 前置:默认参数;.5 用 `specs2`(第二参考 `R2`)
- 实现:`strategies/binary-maker/tests/scenario.rs::bmk_050_reference_fusion`

| 行 | 输入 | 期望 |
|---|---|---|
| BMK-050.1 | 没有第二参考报价时的第一参考报价;`w` = 0 | 未就绪(0):公允价读第一参考自己的中间价 |
| BMK-050.2 | 第二参考 99 900.00 后第一参考 100 000.00,`w` = 0.5 | 比值取 1.001,融合中间价 = 第一参考的中间价(±1) |
| BMK-050.3 | 第二参考上涨 10 bp | 融合中间价上涨 5 bp(第一参考未动) |
| BMK-050.4 | 第二参考超过 5 s 未更新 | 未就绪(0) |
| BMK-050.5 | 策略内:`ref_cb_w` 0.5 / 0,第二参考报价在第一参考之后上涨 | 0.5:`ref_mid_x2` 偏离 Binance 中间价;0:等于 Binance 中间价 |


### BMK-051 辅助组(schema v41 `ref_aux`):永续参考与原生 Chainlink

- 规则:[group 形态](README.md#group-形态)、[订阅](README.md#订阅)、[规则](README.md#规则)(Chainlink)
- 级别:table
- 里程碑:M3
- 前置:`specs3` = `specs2` + 永续组(`RP`,`BINANCE_USDM`,`LinearPerp`)+ Chainlink 组(`RC`,`POLYMARKET_PRICE`,BTC px_exp −2)
- 实现:`strategies/binary-maker/tests/scenario.rs::bmk_051_auxiliary_groups`

| 行 | 输入 | 期望 |
|---|---|---|
| BMK-051.1 | `ref_aux` 3 + `specs3`;`ref_aux` 0 + `specs3`;`ref_aux` 3 + 最后两组对调 | `Ok`;`GroupShape` 码 1(参考组多于 2);码 5(位 0 的组不是永续) |
| BMK-051.2 | `subscriptions` | 槽位 `BOOK | TRADE`、`R`/`R2` `BBO`、`RP` `BBO | BOOK | TRADE`、`RC` `INDEX | MARK`;`reference`/`reference2`/`perp`/`clref` = `R`/`R2`/`RP`/`RC`;`cl_mul` = 10^6 |
| BMK-051.3 | BMK-034 的第 1、4 对参数加 `ref_aux` 3,`specs3`,另以独立随机流穿插永续 BBO/簿/成交与 Chainlink 价格/TWAP 事件 | 辅助事件无输出;轨迹哈希与计数等于不带辅助组的同一轨迹 |
| BMK-051.4 | 同一组 Chainlink 样本:旁路(`InjectWarmup` local_kind 1)/ 原生 `Index`;原生表上再发旁路样本;原生 `Mark` | 两者的逐秒价格、最新秒与价格、样本数相同;旁路样本被忽略;`twap_at` 取到该 TWAP,之前的秒为 `None` |

### BMK-052 规则 14 与 Python 参照逐秒平价

- 规则:[规则](README.md#规则)(规则 14)
- 级别:table
- 里程碑:M3
- 前置:`fixtures/binary-maker-fcev/parity_20261109.jsonl`(公司收数机 2026-11-09 的 4 个 BTC 窗口,1 195 个判定秒;由仓外 `parity_dump.py` 以 `sim2.py` 计算);模型 s 1.25、λ 0.35、c0 1.3、c1 0.5、w 0.5
- 实现:`strategies/binary-maker/tests/scenario.rs::bmk_052_fcev_parity_with_the_python_reference`

| 行 | 检查 | 期望 |
|---|---|---|
| BMK-052.1 | `time_factors` 的 sq 与 remf | 与 Python 差 ≤ 1e-6 |
| BMK-052.2 | `dist_up` | 差 ≤ 2e-4 bp |
| BMK-052.3 | 隐含 σ 样本数与中位数(Python 已有 ≥ 5 个样本的行) | 样本数不同的行 ≤ 1 %;σ 相对差 ≤ 5e-4 |
| BMK-052.4 | 两个代币在买一的分数(σ 不足 5 个样本时用 Python 的回退 σ) | 差 ≤ 0.002 美分 |

### BMK-053 规则 14 的挂单、撤单与配对

- 规则:[规则](README.md#规则)(规则 14)
- 级别:table
- 里程碑:M3
- 前置:`specs3`,`ref_aux` 3、`fc_on` 1,其余缺省;窗口 600 s(第 2 对槽位):Up 0.450/0.470、Down 0.530/0.550,参考与 Chainlink 100 100.00,起点 TWAP 100 000.00
- 实现:`strategies/binary-maker/tests/scenario.rs::bmk_053_rule14_decisions`

| 行 | 输入 | 期望 |
|---|---|---|
| BMK-053.1 | 没有起点 TWAP | 不挂 |
| BMK-053.2 | 有 TWAP,641 s 的簿事件 | 只在 Up 的买一 0.450 挂单 |
| BMK-053.3 | `fc_m_post` 40 美分 | 不挂 |
| BMK-053.4 | Up 挂在 0.450:Chainlink 跌到 −10 bp;或 Up 买一升到 0.460 | 撤单 |
| BMK-053.5 | `fc_m_pair` 0.5 美分,Up 持 10 份均价 0.450 | Down 只挂配对单:0.530(min(54 美分, 买一 53 美分)),10 份 |
| BMK-053.6 | `fc_on` 0 | 不挂;隐含 σ 环从未取样 |

### BMK-054 规则 15 领先挂单

- 规则:[规则](README.md#规则)(规则 15)
- 级别:table
- 里程碑:M3
- 前置:`specs3`,`ref_aux` 3、`lead_w_ms` 300、`lead_x_bp` 1 bp、`lead_only` 1、`inv_max_shares` 20;窗口 600 s(第 2 对槽位):Up 0.450/0.470、Down 0.530/0.550;永续中间价 100 000.00
- 实现:`strategies/binary-maker/tests/scenario.rs::bmk_054_rule15_lead_quote`

| 行 | 输入 | 期望 |
|---|---|---|
| BMK-054.1 | 400 ms 内 +1.5 bp | 第一条永续报价不挂;之后 Up 挂 5 份 @ 0.460 |
| BMK-054.2 | 400 ms 内 −1.5 bp | Down 挂 5 份 @ 0.540 |
| BMK-054.3 | +0.9 bp;Up 价差 1 美分;Up 已持 20 份;窗口第 2 s;结束前 9 s | 都不挂 |
| BMK-054.4 | 第一次触发后 1 s 再涨(该腿已空)/ 2.1 s 后再涨 | 冷却内不挂 / 再挂一张 |
| BMK-054.5 | 挂着的领先单 2.9 s / 3.0 s;`lead_only` 下 Down 上挂着一张非领先单 | 不撤 / 撤;非领先单立即撤 |
| BMK-054.6 | 永续撤单 1 000 ms / 0.5 bp,1 s 内 −0.6 bp | Up 的领先单撤掉 |
| BMK-054.7 | `lead_only` 下的簿事件;规则 15 关(同一 group 表)时永续 +5 bp | 都不挂 |

### BMK-055 规则 15 的现货触发源与模型价(schema v43)

- 规则:[规则](README.md#规则)(规则 15)
- 级别:table
- 里程碑:M3
- 前置:同 BMK-054;.2 用 `fixtures/binary-maker-fcev/lead_px_20261108.jsonl`(公司收数机 2026-11-08 的 29 个窗口里 `PS1_mod05_pc_life1` 挂出的 116 张领先单,由仓外 `lead_parity_dump.py` 以 `sim2.py` 计算)
- 实现:`strategies/binary-maker/tests/scenario.rs::bmk_055_rule15_sources_and_model_price`

| 行 | 输入 | 期望 |
|---|---|---|
| BMK-055.1 | `model_px`:Up 0.450/0.500,E 3 c / 0.3 c / 0;0.450/0.470;m 10 c | 0.480(maxtick)/ 0.470 / 0.460(卖一 − 1)/ 0.460(不低于买一 + 1) |
| BMK-055.2 | 116 张领先单的 sim2 特征、ret、买一卖一 | 挂单价与 sim2 逐笔相同 |
| BMK-055.3 | `lead_src` 2 的现货 +1.5 bp;`lead_src` 1 的现货 +1.5 bp;`lead_src` 3 永续触发后 0.5 s 现货同向 +1.5 bp | Up 0.460;不挂;现货仍触发(冷却按源独立) |
| BMK-055.4 | `lead_price` 1,没有 Chainlink 起点 TWAP | 买一 + 1 美分(0.460) |
| BMK-055.5 | `lead_p_min` 0.50 / 0.46,模型价 0.46 | 0.50:不挂;0.46:挂 0.46 |

### BMK-057 规则 15 的 Coinbase 触发源与确认(schema v45)

- 规则:[规则](README.md#规则)(规则 15:Coinbase 触发源与确认)、[group 形态](README.md#group-形态)
- 级别:table
- 里程碑:M3
- 前置:`specs4`(`specs3` 加 Coinbase 永续参考组 `RCP`,venue 18),`ref_aux` 7、`lead_w_ms` 300、`lead_x_bp` 1 bp、`lead_life_ms` 750、`lead_only` 1、`inv_max_shares` 20;窗口 600 s(第 2 对槽位):Up 0.450/0.470、Down 0.530/0.550;各参考中间价 100 000.00;.5–.7 另设 `lead_src` 15、`lead_cf_ms` 200、`lead_cf_x_bp` 0.5 bp,挂出后把该腿置为已挂
- 实现:`strategies/binary-maker/tests/scenario.rs::bmk_057_rule15_coinbase_sources_and_confirmation`

| 行 | 输入 | 期望 |
|---|---|---|
| BMK-057.1 | `ref_aux` 7 + `specs4`;`ref_aux` 3 + `specs4`;最后两组对调;`ref_aux` 4 + `specs2` 加 `RCP` 组 | `Ok`;`GroupShape` 码 1;码 5(Coinbase 永续组不在永续 venue 上);`Ok` |
| BMK-057.2 | 订阅与 inst | 参考与辅助组依次 `R` BBO、`R2` BBO、`RP` BBO + 簿 + 成交、`RC` 指数 + 标记、`RCP` BBO;`cbperp` = `RCP`;`ref_aux` 4 时 `perp`、`clref` 为空 |
| BMK-057.3 | `lead_src` 4:`R2` 报价 400 ms 内 +1.5 bp;`lead_src` 3 同样的报价 | Up 0.460,该腿记录的源 = 4;不挂 |
| BMK-057.4 | `lead_src` 15:`RCP` 报价 −1.5 bp;0.5 s 后 `R2` 同向 −1.5 bp(该腿已空);`lead_src` 3 的 `RCP` 报价 | Down 0.540,源 = 8;`R2` 仍触发(冷却按源独立);不挂 |
| BMK-057.5 | `R2` 触发挂出后永续只 +0.02 bp:挂出后 150 ms、200 ms 的永续报价 | 150 ms 不撤;200 ms 撤 |
| BMK-057.6 | 同上但 100 ms 时永续 +0.6 bp;300 ms 时永续回到原价;750 ms;另:永续自己触发的单,300 ms 时 `R2` 报价 | 记为已确认、不撤;不撤;到寿命撤;不撤(币安源不需确认) |
| BMK-057.7 | `RCP` 触发的 Down 单,200 ms 时永续 +0.6 bp;`R2` 触发的 Up 单,现货参考 +0.6 bp | 撤(方向相反不算确认);不撤(现货参考也能确认) |

### BMK-058 规则 17:公允价差领先单(schema v46)

- 规则:[规则](README.md#规则)(规则 17)
- 级别:table
- 里程碑:M3
- 前置:`specs3`,`ref_aux` 3、`lead_src` 0、`lead_only` 1、`inv_max_shares` 20、`fvl_w_ms` 1 000、`fvl_x_bp` 0.3 bp(`fvl_gap` 缺省 6 美分、`fvl_cool_ms` 缺省 2 s);窗口 600 s(第 2 对槽位):Up 0.450/0.470、Down 0.530/0.550;起点 TWAP 100 000.00,参考与 Chainlink 100 100.00(+10 bp:Up 的公允价远高于中间价 0.46,Down 的远低于 0.54);永续报价 641 s 起,第一条 100 000.00
- 实现:`strategies/binary-maker/tests/scenario.rs::bmk_058_rule17_fair_gap_lead`

| 行 | 输入 | 期望 |
|---|---|---|
| BMK-058.1 | 1.1 s 后永续 +0.5 bp | Up 挂 0.460;该腿记录的源 = 16(`LEAD_SRC_FV`) |
| BMK-058.2 | 该腿置空;挂出后 600 ms 永续再涨;挂出后 2.1 s(Chainlink 样本更新)永续 +0.5 bp | 冷却内不挂;冷却后再挂 0.460 |
| BMK-058.3 | 永续 +0.2 bp | 不挂 |
| BMK-058.4 | 永续 −0.5 bp(有利于被高估的 Down) | 不挂 |
| BMK-058.5 | 参考与 Chainlink 在起点价(公允价 0.50,差 4 美分),永续 +0.5 bp;同上但 `fvl_gap` 3 美分 | 不挂;挂 0.460 |
| BMK-058.6 | 没有起点 TWAP;Up 盘口 0.460/0.470(价差 1 美分) | 都不挂 |
| BMK-058.7 | 另开规则 15(`lead_src` 1、300 ms、1 bp),永续 1.1 s 后 +1.5 bp | 只挂一张,该腿记录的源 = 1(规则 15 先占用该腿) |
| BMK-058.8 | `fvl_w_ms` 0(缺省)、`fvl_x_bp` 0.3 bp,同 .1 的报价 | 不挂;`lead_on` 为假 |

### BMK-059 规则 18:领先吃单(schema v47)

- 规则:[规则](README.md#规则)(规则 18)
- 级别:table
- 里程碑:M3
- 前置:`specs3`,`ref_aux` 3、`lead_w_ms` 300、`lead_x_bp` 1.5 bp、`lead_only` 1、`inv_max_shares` 20、`take_on` 1(其余缺省:卖一带 [0.55, 0.85)、`take_slip_c` 1、`take_max_win` 1、`take_max_hour` 0、`lead_shares` 5);窗口 600 s(第 2 对槽位):Up、Down 盘口都是 0.600/0.620;永续 640 s 起 100 000.00,400 ms 后 +1.6 bp
- 实现:`strategies/binary-maker/tests/scenario.rs::bmk_059_rule18_lead_take`

| 行 | 输入 | 期望 |
|---|---|---|
| BMK-059.1 | 触发 | 只有一张单:Up、买、`Ioc`、最差价 0.630、5 份、`SPEND`、标签 `take_tag_of(4)`;该腿记录的源 = 32;没有领先挂单 |
| BMK-059.2 | 该腿置空,2.6 s 后同向再触发;同上但 `take_max_win` 2 | 不发;发第二张 0.630 |
| BMK-059.3 | Up 盘口 0.450/0.470;0.830/0.850;0.530/0.550 | 不发;不发(上界不含);发 |
| BMK-059.4 | `take_max_hour` 1:Up 吃单后 1 s 永续反向 −1.7 bp(Down 触发);`take_max_hour` 0 | 不发;发 |
| BMK-059.5 | 另设 `lead_life_ms` 750、永续撤单 1 000 ms / 0.5 bp;吃单置为已挂,900 ms 后永续回落 1.2 bp | 不撤 |
| BMK-059.6 | `take_on` 0(缺省),同样的触发 | 一张 `PostOnly` 领先挂单 0.610 |
| BMK-059.7 | `take_one_side` 1:持有 5 份 Up,永续 −1.7 bp(Down 触发);同上但 `take_one_side` 0;持有 5 份 Down,Up 触发;无持仓,Down 触发 | 不发;发;不发;发 |
