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
| 参数 | BMK-003, BMK-004, BMK-008, BMK-016, BMK-022, BMK-024, BMK-033 |
| 订阅 | BMK-003、BMK-009、BMK-026 |
| 波动率 | BMK-002 |
| 特征 | BMK-034、BMK-035 |
| 规则 | BMK-003–BMK-005, BMK-007, BMK-008, BMK-010–BMK-021, BMK-023, BMK-025–BMK-034 |
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
| BMK-024.1 | schema;`DESC_VERSION` | 版本 6;`DESC_VERSION` 7;`n_fields_at(5)` = 32,`n_fields_at(6)` = 56 |
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
