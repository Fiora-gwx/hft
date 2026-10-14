---
component: hft-strat-lib
prefix: SLB
spec: README.md
---

# hft-strat-lib 测试规格

实时 FundingStats 子集由 `tests/funding.rs` 验证结算、周期变化、回退、缺口和日化边界；同一 Funding 投递的 SDK 视图集成见 [C15.1](../../docs/tests/scenarios.md#c15-venuenoticedelisting-设该-inst-的-delisting-cap-并把公告投递给策略)。预热与快照用例仍待实现。

格式与规则见 [docs/tests](../../docs/tests/README.md)。

## 覆盖

| README 小节 | 用例 |
|---|---|
| 概要 | SLB-132、SLB-133、SLB-134 |
| 目录 | 不适用:非规范 |
| 使用本组件 | 不适用:见子节 |
| 单位约定 | SLB-001、SLB-022、SLB-030、SLB-036、SLB-038、SLB-052、SLB-055、SLB-056、SLB-066 |
| 构件总表 | SLB-132、SLB-136–SLB-152 |
| 快速部分可用的构件 | SLB-153、SLB-001–SLB-019、SLB-031、SLB-033、SLB-035、SLB-039、SLB-043、SLB-050、SLB-053、SLB-057–SLB-059、SLB-062–SLB-066、SLB-068、SLB-073、SLB-075、SLB-078、SLB-079、SLB-084、SLB-086–SLB-088、SLB-128、SLB-130–SLB-132 |
| 只给 Model 用的构件 | SLB-092、SLB-093、SLB-098–SLB-101、SLB-106–SLB-110、SLB-112、SLB-114、SLB-116、SLB-119、SLB-121、SLB-126、SLB-129、SLB-131、SLB-134 |
| 输出与 Advice 载荷 | SLB-018、SLB-019、SLB-031、SLB-032、SLB-053–SLB-056、SLB-065、SLB-067、SLB-088、SLB-089、SLB-105–SLB-108、SLB-113、SLB-114、SLB-123、SLB-125、SLB-129、SLB-131、SLB-134、SLB-135 |
| 参数 | SLB-004、SLB-020、SLB-030、SLB-034、SLB-046、SLB-058、SLB-092 |
| 实现说明 | 不适用:见子节 |
| SpreadCalc 与 CostLine | SLB-001–SLB-010、SLB-016 |
| TriggerPx(预计算触发价) | SLB-012–SLB-018、SLB-055 |
| ConfirmWindow | SLB-020–SLB-031、SLB-055 |
| DirtyFilter | SLB-033–SLB-037 |
| Throttle | SLB-039–SLB-051、SLB-055 |
| ImpactModel::vwap | SLB-058、SLB-060、SLB-061 |
| FundingStats | SLB-068–SLB-091 |
| PercentileStats | SLB-093–SLB-100、SLB-102–SLB-105、SLB-109–SLB-113、SLB-115 |
| LeadLag | SLB-116–SLB-125、SLB-127、SLB-135 |
| 规划源文件 | SLB-134 |
| 延迟与资源契约 | SLB-057、SLB-103、SLB-104、SLB-114、SLB-126、SLB-128、SLB-132、SLB-136–SLB-152 |
| 测试与验收 | 不适用:非规范(用例与验收在本文件) |
| 依赖 | SLB-133、SLB-134 |
| 延伸阅读 | 不适用:非规范 |
| 已知限制与后续工作 | 不适用:非规范 |
| 开发备忘 | SLB-091 |

## 用例

**共用夹具**(用例的 `前置` 引用这里的名字;用例依赖的参数值仍在 `前置` 里写出):

- 定点写法:`Px` 指数 −2(1 raw = 0.01 USDT)、`Qty` 指数 −8、`Rate` 1e-9(1 bp = 100_000,1% = 10_000_000)。`MAX_RAW` = 4_611_686_018_427_387_903(hft-types)。
- 时间:engine_ns 与 UTC ns 都写整数 ns;`H` = 3_600_000_000_000 ns;`T0` = 1_759_708_800_000_000_000(2025-10-06 00:00:00 UTC)。
- **`RF1`**(SDK `testing::RefFixture`):账户 `A1`(venues {V1, V2}),inst `I1`(V1 现货,tick 1、lot 1_000)、`I2`(V2 USDT 永续,tick 10、lot 100_000);费率档:I1 maker 0 / taker 1_000_000(10 bp),I2 maker −50_000(−0.5 bp)/ taker 500_000(5 bp);另有 inst `I3`(V3 永续,V3 不在 A1 的 venues 内)与账户 `A2`(不是 `rd.account()`);inst 999 不在 refdata。
- **`BK1`**(实现 `BookRead` 的测试簿,`status = Live`):卖盘 (6_001_000, 50_000_000)、(6_001_100, 100_000_000)、(6_001_500, 200_000_000);买盘 (6_000_000, 50_000_000)、(5_999_900, 100_000_000)、(5_999_500, 200_000_000);`vwap` 的 side 是吃单方向:`Buy` 吃卖盘、`Sell` 吃买盘。测试簿记录 `best`/`levels` 的调用次数与 `out.len()`。
- **`C1`**(`PctConfig`):`sample_ms = 1_000`、`windows_ms = [10_000, 60_000, 0, 0]`、`n_windows = 2`、`lo = −1_000_000`、`hi = 1_000_000`、`bin = 100_000`(`n_bins` = 21,箱中心 −1_000_000、−900_000 … 1_000_000)、`fill_max_ms = 3_000`、`sides = 2`;L = 60。秒数 `s` 写作 `s × 10^9` ns。
- **`LL1`**(`LeadLagCfg`):`bucket_ns = 50_000`、`max_lag_ns = 1_000_000`(J = 20,L = 41)、`window_ms = 60_000`、`period_ms = 1_000`、`min_samples = 10`、`max_changes = 1_000`;`TL` = 1_000_000_000_000(50_000 的整数倍)。
- **`LLX`**(领先-滞后合成序列,leg 0 = a、leg 1 = b):两腿先在 `TL − 1_000_000` 各观测一次 mid 6_000_000(只记 mid);a 的第 i 个变动在 `TL + i × 1_000_000`(i = 0..19),mid 依次 6_000_010、6_000_000、6_000_010 …(符号 +、−、+ … 交替);b 的第 i 个变动在 a 的第 i 个变动之后 200_000 ns(4 个 bucket),mid 与符号同 a。
- `Now` 由 SDK `testing::MockViews` 给出(`tsc_per_ms = 3_000_000`;本次启动锚点 tsc 0 ↔ engine_ns 5_000_000_000,故 `ens_of(t) = 5_000_000_000 + t / 3`)。

### SLB-001 `SpreadCalc::buy` 以慢侧价为分母、HalfEven、分母非正为 0、溢出饱和

- 规则:[快速部分可用的构件](README.md#快速部分可用的构件)、[SpreadCalc 与 CostLine](README.md#spreadcalc-与-costline)、[单位约定](README.md#单位约定)
- 级别:table
- 里程碑:M2
- 前置:无
- 实现:`crates/hft-strat-lib/tests/spread.rs::slb_001_buy_spread`

| 行 | 输入 `buy(bid_fast, buy_px_slow)` | 期望 |
|---|---|---|
| SLB-001.1 | `(6_003_000, 6_000_000)`(60 030.00 / 60 000.00) | `Rate(500_000)`(5 bp) |
| SLB-001.2 | `(6_000_001, 6_000_000)` | `Rate(167)`(166.67 → 167) |
| SLB-001.3 | `(2_000_000_001, 2_000_000_000)` | `Rate(0)`(0.5 → 偶数 0) |
| SLB-001.4 | `(2_000_000_003, 2_000_000_000)` | `Rate(2)`(1.5 → 偶数 2) |
| SLB-001.5 | `(5_997_000, 6_000_000)` | `Rate(−500_000)` |
| SLB-001.6 | `(6_000_000, 0)` | `Rate(0)` |
| SLB-001.7 | `(6_000_000, −1)` | `Rate(0)` |
| SLB-001.8 | `(MAX_RAW, 1)` | `Rate(i64::MAX)`(饱和) |
| SLB-001.9 | `(−MAX_RAW, 1)` | `Rate(−i64::MAX)`(饱和) |

### SLB-002 `SpreadCalc::sell` 以慢侧卖价为分母

- 规则:[快速部分可用的构件](README.md#快速部分可用的构件)、[SpreadCalc 与 CostLine](README.md#spreadcalc-与-costline)
- 级别:table
- 里程碑:M2
- 前置:无
- 实现:`crates/hft-strat-lib/tests/spread.rs::slb_002_sell_spread`

| 行 | 输入 `sell(ask_fast, sell_px_slow)` | 期望 |
|---|---|---|
| SLB-002.1 | `(5_998_800, 6_000_000)` | `Rate(−200_000)` |
| SLB-002.2 | `(6_000_000, 6_000_000)` | `Rate(0)` |
| SLB-002.3 | `(6_001_000, 6_000_000)` | `Rate(166_667)`(166 666.67 → 166 667) |
| SLB-002.4 | `(6_000_000, 0)` | `Rate(0)` |

### SLB-003 `SpreadCalc::net` 是饱和减

- 规则:[快速部分可用的构件](README.md#快速部分可用的构件)
- 级别:table
- 里程碑:M2
- 前置:无
- 实现:`crates/hft-strat-lib/tests/spread.rs::slb_003_net_saturating`

| 行 | 输入 `net(spread, cost)` | 期望 |
|---|---|---|
| SLB-003.1 | `(500_000, 300_000)` | `Rate(200_000)` |
| SLB-003.2 | `(300_000, 500_000)` | `Rate(−200_000)` |
| SLB-003.3 | `(i64::MIN + 1, 2)` | `Rate(i64::MIN)` |
| SLB-003.4 | `(i64::MAX, −1)` | `Rate(i64::MAX)` |
| SLB-003.5 | `(0, i64::MIN)` | `Rate(i64::MAX)` |

### SLB-004 `CostLine::set` 按用途位预算三个合计

- 规则:[快速部分可用的构件](README.md#快速部分可用的构件)、[SpreadCalc 与 CostLine](README.md#spreadcalc-与-costline)、[参数](README.md#参数)
- 级别:unit
- 里程碑:M2
- 前置:items = [{rate 3_000_000, label 1, flags `ROUND_TRIP | OUT_OF_QUOTE`}, {rate 500_000, label 2, flags `FIXED`}, {rate 200_000, label 3, flags `OUT_OF_QUOTE`}, {rate 1_000_000, label 4, flags `ROUND_TRIP`}, {rate 9_000_000, label 5, flags 0}];`rt_floor = 2_000_000`
- 步骤:
  1. `set(&items, rt_floor)`
  2. 读 `round_trip()`、`open_floor()`、`out_of_quote()`、`n`、`items[0..5]`
- 期望:
  - 步骤 1 返回 `Ok(())`
  - 步骤 2:`round_trip = 4_000_000`、`open_floor = 4_500_000`、`out_of_quote = 3_200_000`;`n = 5`;`items[0..5]` 与输入逐字段相同;flags 为 0 的第 5 项不进任何合计;`label` 不影响合计
- 实现:`crates/hft-strat-lib/tests/cost.rs::slb_004_set_sums_by_flag`

### SLB-005 `round_trip` 取 `rt_floor` 与往返项之和的较大者

- 规则:[快速部分可用的构件](README.md#快速部分可用的构件)
- 级别:table
- 里程碑:M2
- 前置:items 同 SLB-004(Σ `ROUND_TRIP` = 4_000_000,Σ `FIXED` = 500_000);每行只改 `rt_floor`
- 实现:`crates/hft-strat-lib/tests/cost.rs::slb_005_rt_floor_boundary`

| 行 | `rt_floor` | 期望 `round_trip` / `open_floor` |
|---|---|---|
| SLB-005.1 | 3_999_999 | 4_000_000 / 4_500_000 |
| SLB-005.2 | 4_000_000 | 4_000_000 / 4_500_000 |
| SLB-005.3 | 4_000_001 | 4_000_001 / 4_500_001 |
| SLB-005.4 | −1 | 4_000_000 / 4_500_000 |

### SLB-006 `CostLine::set` 的错误分支

- 规则:[快速部分可用的构件](README.md#快速部分可用的构件)、[SpreadCalc 与 CostLine](README.md#spreadcalc-与-costline)
- 级别:table
- 里程碑:M2
- 前置:每行在新建的 `CostLine` 上调用一次 `set`;`rt_floor = 0`;未写明的项 `flags = ROUND_TRIP`、`rate = 100_000`
- 实现:`crates/hft-strat-lib/tests/cost.rs::slb_006_set_errors`

| 行 | items | 期望 |
|---|---|---|
| SLB-006.1 | 8 项 | `Ok(())`,`round_trip = 800_000` |
| SLB-006.2 | 9 项 | `Err(CostErr::TooMany)` |
| SLB-006.3 | 0 项 | `Ok(())`,三个合计都为 0 |
| SLB-006.4 | 1 项,`flags = 8` | `Err(CostErr::BadFlags)` |
| SLB-006.5 | 1 项,`flags = ROUND_TRIP \| FIXED`(3) | `Err(CostErr::BadFlags)` |
| SLB-006.6 | 1 项,`flags = FIXED \| OUT_OF_QUOTE`(6) | `Ok(())`,`open_floor = 100_000`,`out_of_quote = 100_000` |
| SLB-006.7 | 2 项,`rate = i64::MAX / 2 + 1`,都为 `ROUND_TRIP` | `Err(CostErr::Overflow)` |
| SLB-006.8 | 2 项,`rate = i64::MAX / 2`,都为 `ROUND_TRIP` | `Ok(())`,`round_trip = i64::MAX − 1` |
| SLB-006.9 | `round_trip` 合计 = `i64::MAX`(1 项),另 1 项 `FIXED` rate 1 | `Err(CostErr::Overflow)`(`open_floor` 超出 i64) |

### SLB-007 `CostLine::set` 失败时保留原合计

- 规则:[快速部分可用的构件](README.md#快速部分可用的构件)
- 级别:unit
- 里程碑:M2
- 前置:`CostLine` 已以 SLB-004 的 items 与 `rt_floor = 2_000_000` 成功 `set`
- 步骤:
  1. 以 9 项调用 `set`
  2. 以含 `flags = 3` 的 1 项调用 `set`
- 期望:
  - 两步都返回 `Err`;之后 `round_trip = 4_000_000`、`open_floor = 4_500_000`、`out_of_quote = 3_200_000`、`n = 5`、`rt_floor = 2_000_000`,与失败前逐字段相同
- 实现:`crates/hft-strat-lib/tests/cost.rs::slb_007_set_error_keeps_state`

### SLB-008 `fee_round_trip` 取两腿费率档、两倍求和

- 规则:[快速部分可用的构件](README.md#快速部分可用的构件)、[SpreadCalc 与 CostLine](README.md#spreadcalc-与-costline)
- 级别:table
- 里程碑:M2
- 前置:`RF1`;`rd = RefView`(本实例账户 A1)
- 实现:`crates/hft-strat-lib/tests/cost.rs::slb_008_fee_round_trip`

| 行 | legs | 期望 |
|---|---|---|
| SLB-008.1 | [(I1, A1, Taker), (I2, A1, Taker)] | `Ok(Rate(3_000_000))` |
| SLB-008.2 | [(I1, A1, Taker), (I2, A1, Maker)] | `Ok(Rate(1_900_000))`(maker 返佣为负) |
| SLB-008.3 | [(I1, A1, Maker)] | `Ok(Rate(0))` |
| SLB-008.4 | [] | `Ok(Rate(0))` |

### SLB-009 `fee_round_trip` 的错误按第一个出错的腿返回

- 规则:[快速部分可用的构件](README.md#快速部分可用的构件)
- 级别:table
- 里程碑:M2
- 前置:`RF1`;`rd = RefView`(本实例账户 A1)
- 实现:`crates/hft-strat-lib/tests/cost.rs::slb_009_fee_errors`

| 行 | legs | 期望 |
|---|---|---|
| SLB-009.1 | [(999, A1, Taker)] | `Err(FeeErr::UnknownInst)` |
| SLB-009.2 | [(I1, A2, Taker)] | `Err(FeeErr::NoFeeTier)` |
| SLB-009.3 | [(I3, A1, Taker)](V3 不在 A1 的 venues 内) | `Err(FeeErr::NoFeeTier)` |
| SLB-009.4 | [(I1, A2, Taker), (999, A1, Taker)] | `Err(FeeErr::NoFeeTier)` |
| SLB-009.5 | [(999, A1, Taker), (I1, A2, Taker)] | `Err(FeeErr::UnknownInst)` |
| SLB-009.6 | [(I1, A1, Taker), (999, A1, Taker)] | `Err(FeeErr::UnknownInst)` |

### SLB-010 `fee_round_trip` 不读 `VenueCaps.fee_asset_rule`

- 规则:[SpreadCalc 与 CostLine](README.md#spreadcalc-与-costline)
- 级别:unit
- 里程碑:M2
- 前置:`RF1` 与 `RF1'`(只把 V1 的 `fee_asset_rule` 改为以基础币扣费,其余逐字节相同)
- 步骤:
  1. 在两份 refdata 上各算 [(I1, A1, Taker), (I2, A1, Taker)]
- 期望:
  - 两次都 `Ok(Rate(3_000_000))`
- 实现:`crates/hft-strat-lib/tests/cost.rs::slb_010_fee_asset_rule_ignored`

### SLB-011 `fee_round_trip` 合计在 i128 中求和、超出 i64 饱和

- 规则:[快速部分可用的构件](README.md#快速部分可用的构件)
- 级别:prop
- 里程碑:M2
- 性质:对任意费率档,`fee_round_trip` 不 panic;结果 = clamp(2 × Σ fee,`−i64::MAX`,`i64::MAX`)(Σ 与乘 2 在 i128 中计算)
- 生成:1–4 条腿;每腿 maker、taker 从 {i64::MIN + 1, −1, 0, 1, 1_000_000, i64::MAX} ∪ 任意 i64 中抽取(测试 refdata 构造器不做范围校验)
- 参考:i128 参考实现
- 例数:PR 10 000;每夜 1 000 000
- 实现:`crates/hft-strat-lib/tests/props.rs::slb_011_fee_sum_saturates`

### SLB-012 `TriggerPx::update` 按更难触发的方向取整到 fast 的 tick

- 规则:[快速部分可用的构件](README.md#快速部分可用的构件)、[TriggerPx(预计算触发价)](README.md#triggerpx预计算触发价)
- 级别:table
- 里程碑:M2
- 前置:`now_ens = 7_000_000_000`;每行一次 `update(buy_px_slow, sell_px_slow, θ_open, θ_close, tick_fast, now_ens)`
- 实现:`crates/hft-strat-lib/tests/trigger.rs::slb_012_update_rounding`

| 行 | `(buy_slow, sell_slow, θ_open, θ_close, tick)` | 期望 `buy_bid_fast` / `sell_ask_fast` |
|---|---|---|
| SLB-012.1 | (6_000_000, 6_000_000, 500_000, −200_000, 10) | 6_003_000 / 5_998_800 |
| SLB-012.2 | (6_000_001, 6_000_001, 500_000, 500_000, 1) | 6_003_002 / 6_003_001(积 6_003_001.5000005) |
| SLB-012.3 | (6_000_000, 6_000_000, 505_000, 505_000, 100) | 6_003_100 / 6_003_000(积 6_003_030) |
| SLB-012.4 | (6_000_005, 6_000_005, 0, 0, 10) | 6_000_010 / 6_000_000 |
| SLB-012.5 | (MAX_RAW, MAX_RAW, 1_000_000_000, 0, 10) | 4_611_686_018_427_387_900 / 4_611_686_018_427_387_900(超出 MAX_RAW 时取不超过它的最近 tick 整数倍) |

每行之后 `valid = true`、`slow_ens = 7_000_000_000`。

### SLB-013 `hit_buy`/`hit_sell` 的等号边界

- 规则:[快速部分可用的构件](README.md#快速部分可用的构件)、[TriggerPx(预计算触发价)](README.md#triggerpx预计算触发价)
- 级别:table
- 里程碑:M2
- 前置:SLB-012.1 之后的 `TriggerPx`(`buy_bid_fast = 6_003_000`、`sell_ask_fast = 5_998_800`、tick 10)
- 实现:`crates/hft-strat-lib/tests/trigger.rs::slb_013_hit_boundaries`

| 行 | 调用 | 期望 |
|---|---|---|
| SLB-013.1 | `hit_buy(6_002_990)`(−1 tick) | `false` |
| SLB-013.2 | `hit_buy(6_002_999)`(−1 raw) | `false` |
| SLB-013.3 | `hit_buy(6_003_000)` | `true` |
| SLB-013.4 | `hit_buy(6_003_010)`(+1 tick) | `true` |
| SLB-013.5 | `hit_sell(5_998_810)`(+1 tick) | `false` |
| SLB-013.6 | `hit_sell(5_998_800)` | `true` |
| SLB-013.7 | `hit_sell(5_998_790)`(−1 tick) | `true` |

### SLB-014 慢侧价或 tick 非正时 `update` 使触发价失效

- 规则:[快速部分可用的构件](README.md#快速部分可用的构件)
- 级别:table
- 里程碑:M2
- 前置:每行先以 SLB-012.1 的输入 `update` 成功(`valid = true`),再以该行输入 `update`
- 实现:`crates/hft-strat-lib/tests/trigger.rs::slb_014_update_invalid_inputs`

| 行 | 第二次 `update` 的 `(buy_slow, sell_slow, tick)` | 期望 |
|---|---|---|
| SLB-014.1 | (0, 6_000_000, 10) | `valid = false`;`hit_buy(MAX_RAW) = false`、`hit_sell(−MAX_RAW) = false` |
| SLB-014.2 | (6_000_000, −1, 10) | 同上 |
| SLB-014.3 | (6_000_000, 6_000_000, 0) | 同上 |
| SLB-014.4 | (6_000_000, 6_000_000, −10) | 同上 |
| SLB-014.5 | (1, 1, 1) | `valid = true`(1 raw 是合法的正价) |

### SLB-015 `invalidate` 之后 `hit_*` 恒为假,下一次慢侧更新恢复

- 规则:[TriggerPx(预计算触发价)](README.md#triggerpx预计算触发价)
- 级别:unit
- 里程碑:M2
- 前置:SLB-012.1 之后的 `TriggerPx`
- 步骤:
  1. `invalidate()`;调 `hit_buy(MAX_RAW)`、`hit_sell(−MAX_RAW)`
  2. `update(6_000_000, 6_000_000, 500_000, −200_000, 10, 8_000_000_000)`;调 `hit_buy(6_003_000)`
- 期望:
  - 步骤 1:两次都 `false`;`valid = false`
  - 步骤 2:`true`;`slow_ens = 8_000_000_000`
- 实现:`crates/hft-strat-lib/tests/trigger.rs::slb_015_invalidate_then_update`

### SLB-016 `hit_buy` 命中蕴含开向价差达阈值,差异至多一个 tick

- 规则:[TriggerPx(预计算触发价)](README.md#triggerpx预计算触发价)、[SpreadCalc 与 CostLine](README.md#spreadcalc-与-costline)
- 级别:prop
- 里程碑:M2
- 性质:`update` 之后对任意 tick 对齐的 `b`:`hit_buy(b) ⇒ SpreadCalc::buy(b, slow) ≥ θ_open`;`SpreadCalc::buy(b, slow) ≥ θ_open ∧ ¬hit_buy(b) ⇒ b = buy_bid_fast − tick`
- 生成:`tick` ∈ {1, 10, 100, 1_000};`slow` ∈ [1, 10^9 × tick](HalfEven 的半个 1e-9 不超过半个 tick);`θ_open` ∈ [−10^8, 10^8];`b` = `buy_bid_fast + k × tick`,k ∈ [−3, 3]
- 参考:上面两条蕴含式
- 例数:PR 10 000;每夜 1 000 000
- 实现:`crates/hft-strat-lib/tests/props.rs::slb_016_hit_buy_vs_spread`

### SLB-017 `hit_sell` 命中蕴含平向价差不高于阈值,差异至多一个 tick

- 规则:[TriggerPx(预计算触发价)](README.md#triggerpx预计算触发价)
- 级别:prop
- 里程碑:M2
- 性质:`hit_sell(a) ⇒ SpreadCalc::sell(a, slow) ≤ θ_close`;`SpreadCalc::sell(a, slow) ≤ θ_close ∧ ¬hit_sell(a) ⇒ a = sell_ask_fast + tick`
- 生成:同 SLB-016(`θ_close` 代替 `θ_open`,`a = sell_ask_fast + k × tick`)
- 参考:上面两条蕴含式
- 例数:PR 10 000;每夜 1 000 000
- 实现:`crates/hft-strat-lib/tests/props.rs::slb_017_hit_sell_vs_spread`

### SLB-018 `TriggerPx` 快照为 25 B,恢复后保持失效直到慢侧更新

- 规则:[快速部分可用的构件](README.md#快速部分可用的构件)、[TriggerPx(预计算触发价)](README.md#triggerpx预计算触发价)、[输出与 Advice 载荷](README.md#输出与-advice-载荷)
- 级别:unit
- 里程碑:M2
- 前置:SLB-012.1 之后的 `TriggerPx`(`slow_ens = 7_000_000_000`)
- 步骤:
  1. `snapshot` 到容量 64 B 的 `SnapshotWriter`
  2. 在新的 `TriggerPx` 上 `restore`;调 `hit_buy(6_003_000)`
  3. `update(6_000_000, 6_000_000, 500_000, −200_000, 10, 9_000_000_000)`;再调 `hit_buy(6_003_000)`
- 期望:
  - 步骤 1 写出 25 B:`buy_bid_fast` 6_003_000(i64 小端)、`sell_ask_fast` 5_998_800、`valid` 01、`slow_ens` 7_000_000_000
  - 步骤 2:`restore` 返回 `Ok`;`buy_bid_fast`、`sell_ask_fast`、`slow_ens` 与写出时相同,`valid = false`;`hit_buy` 返回 `false`
  - 步骤 3:`hit_buy` 返回 `true`
- 实现:`crates/hft-strat-lib/tests/trigger.rs::slb_018_snapshot_restore_invalid`

### SLB-019 `TriggerPx::restore` 遇截断字节返回错误且不改状态

- 规则:[输出与 Advice 载荷](README.md#输出与-advice-载荷)
- 级别:unit
- 里程碑:M2
- 前置:SLB-018 步骤 1 写出的 25 B;目标 `TriggerPx` 为 SLB-012.3 之后的状态(`valid = true`)
- 步骤:
  1. 用前 24 B 构造 `SnapshotReader` 并 `restore`
- 期望:
  - 返回 `Err(RestoreError)`;目标的 4 个字段与调用前逐字段相同(`valid = true`、`buy_bid_fast = 6_003_100`)
- 实现:`crates/hft-strat-lib/tests/trigger.rs::slb_019_restore_truncated`

### SLB-020 全 0 的确认参数在第一次条件成立时即确认

- 规则:[ConfirmWindow](README.md#confirmwindow)、[参数](README.md#参数)
- 级别:unit
- 里程碑:M2
- 前置:`ConfirmParams{min_ms: 0, min_updates: 0, min_slow_updates: 0, min_slow_span: 0}.ns()`;新建窗口
- 步骤:
  1. `observe(true, false, 0, 1_000, p)`
- 期望:
  - 返回 `Confirmed`
- 实现:`crates/hft-strat-lib/tests/confirm.rs::slb_020_all_zero_confirms_first`

### SLB-021 `min_updates` 的等号边界

- 规则:[ConfirmWindow](README.md#confirmwindow)
- 级别:table
- 里程碑:M2
- 前置:`min_updates = 3`,其余参数 0;从新建窗口起,`cond = true`、`from_slow = false`、`slow_seq = 0`,第 k 次 `observe` 的 `now_ens = k × 1_000`
- 实现:`crates/hft-strat-lib/tests/confirm.rs::slb_021_min_updates_boundary`

| 行 | 第几次 `observe` | 期望 |
|---|---|---|
| SLB-021.1 | 1 | `Pending` |
| SLB-021.2 | 2(边界 − 1) | `Pending` |
| SLB-021.3 | 3(边界) | `Confirmed` |
| SLB-021.4 | 4(边界 + 1) | `Confirmed` |

### SLB-022 `min_ms` 按 engine_ns 计、含等号

- 规则:[ConfirmWindow](README.md#confirmwindow)、[单位约定](README.md#单位约定)
- 级别:table
- 里程碑:M2
- 前置:`min_ms = 100`(`min_ns = 100_000_000`),其余参数 0;每行在新建窗口上先 `observe(true, false, 0, 1_000_000_000, p)`(返回 `Pending`),再在该行时刻 `observe(true, false, 0, t, p)`
- 实现:`crates/hft-strat-lib/tests/confirm.rs::slb_022_min_ms_boundary`

| 行 | 第二次的 `t` | 期望 |
|---|---|---|
| SLB-022.1 | 1_099_999_999 | `Pending` |
| SLB-022.2 | 1_100_000_000 | `Confirmed` |
| SLB-022.3 | 1_100_000_001 | `Confirmed` |

### SLB-023 `min_slow_updates` 只计慢侧触发的评估

- 规则:[ConfirmWindow](README.md#confirmwindow)
- 级别:table
- 里程碑:M2
- 前置:`min_slow_updates = 2`,其余参数 0;新建窗口;`cond = true`、`slow_seq = 0`,第 k 次 `now_ens = k`
- 实现:`crates/hft-strat-lib/tests/confirm.rs::slb_023_min_slow_updates`

| 行 | `from_slow` 序列 | 期望(最后一次的返回) |
|---|---|---|
| SLB-023.1 | false, false, false | `Pending` |
| SLB-023.2 | true | `Pending` |
| SLB-023.3 | true, false, true | `Confirmed`(第 3 次) |
| SLB-023.4 | true, true | `Confirmed`(第 2 次) |

### SLB-024 `min_slow_span` 按首个序号与最大序号计

- 规则:[ConfirmWindow](README.md#confirmwindow)
- 级别:table
- 里程碑:M2
- 前置:`min_slow_span = 3`,其余参数 0;新建窗口;`cond = true`、`from_slow = true`,第 k 次 `now_ens = k`
- 实现:`crates/hft-strat-lib/tests/confirm.rs::slb_024_min_slow_span`

| 行 | `slow_seq` 序列 | 期望(最后一次的返回) |
|---|---|---|
| SLB-024.1 | 100, 101 | `Pending`(跨度 2) |
| SLB-024.2 | 100, 101, 102 | `Confirmed`(跨度 3) |
| SLB-024.3 | 100, 102 | `Confirmed`(跨度 3) |
| SLB-024.4 | 100, 101, 99 | `Pending`(99 不降低最大值,跨度仍 2) |
| SLB-024.5 | 100, 99, 98 | `Pending`(首个序号固定为 100,跨度 1) |
| SLB-024.6 | 0, 0, 0(CEX 慢侧无序号) | `Pending`(跨度恒为 1) |

### SLB-025 条件不成立时任何状态都回到 `Idle`

- 规则:[ConfirmWindow](README.md#confirmwindow)
- 级别:table
- 里程碑:M2
- 前置:`min_updates = 2`,其余参数 0
- 实现:`crates/hft-strat-lib/tests/confirm.rs::slb_025_false_resets`

| 行 | 序列(`cond`) | 期望 |
|---|---|---|
| SLB-025.1 | true, false | 第 2 次返回 `Idle` |
| SLB-025.2 | true, true, false | 第 3 次返回 `Idle`(从 `Confirmed` 回到 `Idle`) |
| SLB-025.3 | true, false, true | 第 3 次返回 `Pending`(重新起算,`n_updates = 1`) |
| SLB-025.4 | false | `Idle` |

### SLB-026 已确认的窗口在条件持续成立时保持确认并继续累加

- 规则:[ConfirmWindow](README.md#confirmwindow)
- 级别:unit
- 里程碑:M2
- 前置:`min_updates = 2`,其余参数 0;新建窗口
- 步骤:
  1. `observe(true, true, 10, 1, p)`、`observe(true, false, 11, 2, p)`
  2. `observe(true, true, 12, 3, p)`;`snapshot`
- 期望:
  - 步骤 1 第 2 次返回 `Confirmed`
  - 步骤 2 返回 `Confirmed`;快照中 `n_updates = 3`、`n_slow = 2`、`first_slow_seq = 10`、`last_slow_seq = 12`、`since_ens = 1`、`state` = `Confirmed`
- 实现:`crates/hft-strat-lib/tests/confirm.rs::slb_026_confirmed_stays`

### SLB-027 计数饱和于 `u16::MAX` 不回绕

- 规则:[ConfirmWindow](README.md#confirmwindow)
- 级别:unit
- 里程碑:M2
- 前置:`min_updates = 65_535`、`min_slow_updates = 65_535`,其余 0;新建窗口
- 步骤:
  1. 连续 70_000 次 `observe(true, true, 0, k, p)`(k = 1..70_000)
  2. `snapshot`
- 期望:
  - 第 65_534 次返回 `Pending`,第 65_535 次起返回 `Confirmed`,第 70_000 次仍为 `Confirmed`
  - 快照中 `n_updates = 65_535`、`n_slow = 65_535`
- 实现:`crates/hft-strat-lib/tests/confirm.rs::slb_027_counters_saturate`

### SLB-028 `reset` 后重新起算

- 规则:[ConfirmWindow](README.md#confirmwindow)
- 级别:unit
- 里程碑:M2
- 前置:`min_ms = 100`,其余 0;`observe(true, false, 0, 1_000_000_000, p)` 之后
- 步骤:
  1. `reset()`
  2. `observe(true, false, 0, 1_150_000_000, p)`
  3. `observe(true, false, 0, 1_249_999_999, p)`、`observe(true, false, 0, 1_250_000_000, p)`
- 期望:
  - 步骤 2 返回 `Pending`(`since_ens` = 1_150_000_000,不是 1_000_000_000)
  - 步骤 3 依次返回 `Pending`、`Confirmed`
- 实现:`crates/hft-strat-lib/tests/confirm.rs::slb_028_reset_restarts`

### SLB-029 含一次 `false` 的序列不会在该 `false` 之前的起点上确认

- 规则:[ConfirmWindow](README.md#confirmwindow)
- 级别:prop
- 里程碑:M2
- 性质:任意 `(cond, from_slow, slow_seq, now_ens)` 序列与参数下,若第 i 次返回 `Confirmed`,则存在 j ≤ i 使第 j..i 次的 `cond` 全为真、第 j − 1 次(若有)为假或为 `reset`,且按第 j..i 次的观测单独计算的四项条件全部满足
- 生成:序列长 1–200;`cond` 以 90% 为真;`now_ens` 单调非降,步长 0–50_000_000;`slow_seq` 0–1_000;参数各项 0–10(`min_ms` 0–200),每 50 步以 5% 概率插入 `reset`
- 参考:按 README 第 1–5 步写的朴素参考实现(保存全部观测)
- 例数:PR 10 000;每夜 1 000 000
- 实现:`crates/hft-strat-lib/tests/props.rs::slb_029_confirm_no_false_inside`

### SLB-030 `ConfirmParams::ns` 只换算毫秒字段

- 规则:[单位约定](README.md#单位约定)、[参数](README.md#参数)
- 级别:table
- 里程碑:M2
- 前置:无
- 实现:`crates/hft-strat-lib/tests/confirm.rs::slb_030_params_ns`

| 行 | 输入 | 期望 |
|---|---|---|
| SLB-030.1 | `{min_ms: 250, min_updates: 3, min_slow_updates: 2, min_slow_span: 5}` | `min_ns = 250_000_000`,其余三项原样 |
| SLB-030.2 | `min_ms = u32::MAX` | `min_ns = 4_294_967_295_000_000`(不溢出) |
| SLB-030.3 | `min_ms = 0` | `min_ns = 0` |

### SLB-031 `ConfirmWindow` 快照为 29 B,恢复后与连续运行同步确认

- 规则:[快速部分可用的构件](README.md#快速部分可用的构件)、[ConfirmWindow](README.md#confirmwindow)、[输出与 Advice 载荷](README.md#输出与-advice-载荷)
- 级别:unit
- 里程碑:M2
- 前置:`min_updates = 4`,其余 0;窗口 W 已 `observe(true, true, 7, 1_000, p)`、`observe(true, false, 8, 2_000, p)`(`Pending`)
- 步骤:
  1. `snapshot`
  2. 新窗口 W' `restore`;对 W 与 W' 各再 `observe(true, false, 9, 3_000, p)`、`observe(true, true, 9, 4_000, p)`
- 期望:
  - 步骤 1 写出 29 B:`since_ens` 1_000、`n_updates` 2、`n_slow` 1、`first_slow_seq` 7、`last_slow_seq` 8、`state` = `Pending`
  - 步骤 2:W 与 W' 两次返回都依次为 `Pending`、`Confirmed`;之后两者快照逐字节相同
- 实现:`crates/hft-strat-lib/tests/confirm.rs::slb_031_snapshot_restore`

### SLB-032 `ConfirmWindow::restore` 遇截断字节返回错误且不改状态

- 规则:[输出与 Advice 载荷](README.md#输出与-advice-载荷)
- 级别:unit
- 里程碑:M2
- 前置:SLB-031 步骤 1 的 29 B;目标窗口为 `Idle`
- 步骤:
  1. 用前 28 B `restore`
- 期望:
  - 返回 `Err(RestoreError)`;目标窗口快照仍是 `Idle` 的 29 B(全部计数 0)
- 实现:`crates/hft-strat-lib/tests/confirm.rs::slb_032_restore_truncated`

### SLB-033 `DirtyFilter::check` 的原因、等号边界与检查顺序

- 规则:[快速部分可用的构件](README.md#快速部分可用的构件)、[DirtyFilter](README.md#dirtyfilter)
- 级别:table
- 里程碑:M2
- 前置:`DirtyParams{stale_fast_ms: 2_000, stale_slow_ms: 1_000, max_abs_spread: 10_000_000, junk_after: 3}.tsc(3_000_000)`(两腿上限 6_000_000_000 / 3_000_000_000 tsc);`now = 100_000_000_000`;基准输入:两腿 `LegFresh{live_tsc: now, conn_ok: true, resync: false}`、`spread = 0`、`no_quote_streak = 0`;每行只做所列改动
- 实现:`crates/hft-strat-lib/tests/dirty.rs::slb_033_check_reasons`

| 行 | 改动 | 期望 |
|---|---|---|
| SLB-033.1 | 不改 | `Ok(())` |
| SLB-033.2 | `fast.conn_ok = false` | `Err(Stale)` |
| SLB-033.3 | `fast.live_tsc = now − 6_000_000_000` | `Ok(())` |
| SLB-033.4 | `fast.live_tsc = now − 6_000_000_001` | `Err(Stale)` |
| SLB-033.5 | `fast.live_tsc = now − 5_999_999_999` | `Ok(())` |
| SLB-033.6 | `slow.live_tsc = now − 3_000_000_001` | `Err(Stale)` |
| SLB-033.7 | `slow.live_tsc = now − 3_000_000_000` | `Ok(())` |
| SLB-033.8 | `slow.conn_ok = false` | `Err(Stale)` |
| SLB-033.9 | `fast.resync = true` | `Err(Resync)` |
| SLB-033.10 | `slow.resync = true` | `Err(Resync)` |
| SLB-033.11 | `spread = 10_000_000` | `Ok(())` |
| SLB-033.12 | `spread = 10_000_001` | `Err(AbsSpread)` |
| SLB-033.13 | `spread = −10_000_001` | `Err(AbsSpread)` |
| SLB-033.14 | `spread = 9_999_999` | `Ok(())` |
| SLB-033.15 | `no_quote_streak = 2` | `Ok(())` |
| SLB-033.16 | `no_quote_streak = 3` | `Err(Junk)` |
| SLB-033.17 | `no_quote_streak = 4` | `Err(Junk)` |
| SLB-033.18 | `fast.conn_ok = false`、`slow.resync = true`、`spread = 10_000_001`、`no_quote_streak = 3` | `Err(Stale)` |
| SLB-033.19 | `fast.resync = true`、`spread = 10_000_001`、`no_quote_streak = 3` | `Err(Resync)` |
| SLB-033.20 | `spread = 10_000_001`、`no_quote_streak = 3` | `Err(AbsSpread)` |
| SLB-033.21 | `fast.live_tsc = now + 1_000`(晚于 now) | `Ok(())`(年龄按 0) |

### SLB-034 参数为 0 只关闭该项比较

- 规则:[DirtyFilter](README.md#dirtyfilter)、[参数](README.md#参数)
- 级别:table
- 里程碑:M2
- 前置:同 SLB-033 的基准,`now = 100_000_000_000`;每行的参数在 SLB-033 的参数上只改所列一项,输入只做所列改动
- 实现:`crates/hft-strat-lib/tests/dirty.rs::slb_034_zero_disables`

| 行 | 参数与输入 | 期望 |
|---|---|---|
| SLB-034.1 | `stale_fast_ms = 0`;`fast.live_tsc = 0` | `Ok(())` |
| SLB-034.2 | `stale_fast_ms = 0`;`fast.conn_ok = false` | `Err(Stale)` |
| SLB-034.3 | `stale_slow_ms = 0`;`slow.live_tsc = 0` | `Ok(())` |
| SLB-034.4 | `max_abs_spread = 0`;`spread = i64::MAX` | `Ok(())` |
| SLB-034.5 | `junk_after = 0`;`no_quote_streak = 65_535` | `Ok(())` |
| SLB-034.6 | `junk_after = 1`;`no_quote_streak = 1` | `Err(Junk)` |

### SLB-035 `LegFresh` 的两种构造只取连接健康字段

- 规则:[快速部分可用的构件](README.md#快速部分可用的构件)、[DirtyFilter](README.md#dirtyfilter)
- 级别:unit
- 里程碑:M2
- 前置:`BboView` b1:`conn_live_tsc = 5_000`、`conn_up = true`、`resync = false`、`rx_tsc = 1`(报价很久未变);b2 与 b1 只差 `rx_tsc = 4_999`
- 步骤:
  1. `LegFresh::from_bbo(&b1)`、`LegFresh::from_bbo(&b2)`
  2. `LegFresh::from_quote(7_000, false, true)`
- 期望:
  - 步骤 1 两次都得到 `{live_tsc: 5_000, conn_ok: true, resync: false}`(`rx_tsc` 不影响结果)
  - 步骤 2 得到 `{live_tsc: 7_000, conn_ok: false, resync: true}`
- 实现:`crates/hft-strat-lib/tests/dirty.rs::slb_035_legfresh_ctors`

### SLB-036 CEX 快腿的放行与 SDK `quote_live` 逐例一致

- 规则:[DirtyFilter](README.md#dirtyfilter)、[单位约定](README.md#单位约定)
- 级别:prop
- 里程碑:M2
- 性质:慢腿健康、`spread = 0`、`junk_after = 0` 时,`DirtyFilter::check(..).is_ok() == b.quote_live(now, stale_fast_ms × tsc_per_ms)`,其中快腿 `LegFresh = from_bbo(&b)`、`stale_fast_ms ≥ 1`
- 生成:`stale_fast_ms` 1–10_000;`tsc_per_ms` 1_000_000–4_000_000;`now` 10^12;`conn_live_tsc` ∈ [0, now + 10^6] 并含 `now − 上限` 的 ±1;`conn_up`、`resync`、`rx_tsc` 随机
- 参考:SDK `BboView::quote_live`
- 例数:PR 10 000;每夜 1 000 000
- 实现:`crates/hft-strat-lib/tests/props.rs::slb_036_dirty_matches_quote_live`

### SLB-037 盘口不变而连接活性持续时一直放行,连接静默、断开或重同步时拒绝

- 规则:[DirtyFilter](README.md#dirtyfilter)
- 级别:prop
- 里程碑:M2
- 性质:快腿 `BboView` 的报价在 10 s 内不变(`rx_tsc` 固定)而 `conn_live_tsc` 每 1 s 前进一次:每 100 ms 一次 `check` 全部 `Ok`;在某时刻起 `conn_live_tsc` 停止前进后,`now − conn_live_tsc > 上限` 的每次 `check` 都是 `Err(Stale)`;任一时刻改 `conn_up = false` → `Err(Stale)`;改 `resync = true`(连接仍健康)→ `Err(Resync)`
- 生成:`stale_fast_ms` 1_500–5_000;`tsc_per_ms` 3_000_000;停止前进的时刻、断开或重同步的时刻在 [0, 10 s] 内随机
- 参考:上述逐步断言
- 例数:PR 10 000;每夜 1 000 000
- 实现:`crates/hft-strat-lib/tests/props.rs::slb_037_quiet_book_live`

### SLB-038 `DirtyParams::tsc` 按每毫秒 tsc 数换算两个时长

- 规则:[单位约定](README.md#单位约定)
- 级别:table
- 里程碑:M2
- 前置:无
- 实现:`crates/hft-strat-lib/tests/dirty.rs::slb_038_params_tsc`

| 行 | 输入 | 期望 |
|---|---|---|
| SLB-038.1 | `{2_000, 1_000, 10_000_000, 3}.tsc(3_000_000)` | 快腿 6_000_000_000、慢腿 3_000_000_000;`max_abs_spread`、`junk_after` 原样 |
| SLB-038.2 | `stale_fast_ms = u32::MAX`,`tsc(4_000_000)` | 17_179_869_180_000_000(不溢出) |
| SLB-038.3 | `stale_fast_ms = 0` | 0 |

### SLB-039 `can_fire` 的放行表

- 规则:[快速部分可用的构件](README.md#快速部分可用的构件)、[Throttle](README.md#throttle)
- 级别:table
- 里程碑:M2
- 前置:`TP0` = `ThrottleParams{cooldown_ms: 500, max_consec_fail: 3, fallback_ms: 60_000, hard_rearm_ms: 1_000}.ns()`;每行从 `Throttle::default()` 构造所列状态(时刻为 engine_ns),在"查询时刻"对四类动作各调一次 `can_fire`
- 实现:`crates/hft-strat-lib/tests/throttle.rs::slb_039_can_fire_table`

| 行 | 状态(构造)与查询时刻 | Open | Close | Flatten | Hard |
|---|---|---|---|---|---|
| SLB-039.1 | 不改;10_000_000_000 | Ok | Ok | Ok | Ok |
| SLB-039.2 | 只有 in_flight:`on_fire(Open, 10_000_000_000)`;10_600_000_000 | `InFlight` | `InFlight` | Ok | Ok |
| SLB-039.3 | 只有冷却:`on_fire(Open, 10_000_000_000)`、`on_outcome(NoFill, 10_100_000_000)`;10_200_000_000 | `Cooldown` | `Cooldown` | Ok | Ok |
| SLB-039.4 | 只有 halted:`set_halted(true)`;10_000_000_000 | `Halted` | Ok | Ok | Ok |
| SLB-039.5 | 只有退避:`set_backoff(11_000_000_000)`;10_000_000_000 | `Backoff` | `Backoff` | Ok | Ok |
| SLB-039.6 | 只有 hard_rearm:`on_fire(Flatten, 10_000_000_000)`;10_500_000_000 | Ok | Ok | `HardRearm` | `HardRearm` |

### SLB-040 多个原因同时成立时按列序返回第一个

- 规则:[Throttle](README.md#throttle)
- 级别:table
- 里程碑:M2
- 前置:`TP0`;查询时刻 10_200_000_000
- 实现:`crates/hft-strat-lib/tests/throttle.rs::slb_040_reason_order`

| 行 | 状态(构造) | 调用 | 期望 |
|---|---|---|---|
| SLB-040.1 | `on_fire(Open, 10_000_000_000)`、`set_halted(true)`、`set_backoff(20_000_000_000)` | `can_fire(Open)` / `can_fire(Close)` | `InFlight` / `InFlight` |
| SLB-040.2 | 同 .1 再 `on_outcome(NoFill, 10_100_000_000)` | `can_fire(Open)` / `can_fire(Close)` | `Cooldown` / `Cooldown` |
| SLB-040.3 | `set_halted(true)`、`set_backoff(20_000_000_000)` | `can_fire(Open)` / `can_fire(Close)` | `Halted` / `Backoff` |
| SLB-040.4 | `on_fire(Open, 10_000_000_000)`、`on_fire(Flatten, 10_000_000_000)`、`set_halted(true)`、`set_backoff(20_000_000_000)` | `can_fire(Flatten)` | `HardRearm` |

### SLB-041 冷却在截止时刻放行

- 规则:[Throttle](README.md#throttle)
- 级别:table
- 里程碑:M2
- 前置:`TP0`;`on_fire(Open, 10_000_000_000)`(`cooldown_until_ens = 10_500_000_000`)、`on_outcome(NoFill, 10_100_000_000)`
- 实现:`crates/hft-strat-lib/tests/throttle.rs::slb_041_cooldown_boundary`

| 行 | 调用 | 期望 |
|---|---|---|
| SLB-041.1 | `can_fire(Open, 10_499_999_999)` | `Err(Cooldown)` |
| SLB-041.2 | `can_fire(Open, 10_500_000_000)` | `Ok(())` |
| SLB-041.3 | `can_fire(Open, 10_500_000_001)` | `Ok(())` |
| SLB-041.4 | `can_fire(Close, 10_499_999_999)` | `Err(Cooldown)` |

### SLB-042 `hard_rearm` 只作用于 Flatten/Hard

- 规则:[Throttle](README.md#throttle)
- 级别:table
- 里程碑:M2
- 前置:`TP0`;`on_fire(Flatten, 10_000_000_000)`(`hard_rearm_until_ens = 11_000_000_000`)
- 实现:`crates/hft-strat-lib/tests/throttle.rs::slb_042_hard_rearm`

| 行 | 调用 | 期望 |
|---|---|---|
| SLB-042.1 | `can_fire(Flatten, 10_999_999_999)` | `Err(HardRearm)` |
| SLB-042.2 | `can_fire(Flatten, 11_000_000_000)` | `Ok(())` |
| SLB-042.3 | `can_fire(Hard, 10_500_000_000)` | `Err(HardRearm)` |
| SLB-042.4 | `can_fire(Open, 10_000_000_000)` | `Ok(())`(Flatten 不设 in_flight 与冷却) |
| SLB-042.5 | `can_fire(Close, 10_000_000_000)` | `Ok(())` |

### SLB-043 退避镜像覆盖为最新的截止时刻

- 规则:[快速部分可用的构件](README.md#快速部分可用的构件)、[Throttle](README.md#throttle)
- 级别:table
- 里程碑:M2
- 前置:`TP0`;`set_backoff(12_000_000_000)`;行 .5 之前再 `set_backoff(11_000_000_000)`
- 实现:`crates/hft-strat-lib/tests/throttle.rs::slb_043_backoff`

| 行 | 调用 | 期望 |
|---|---|---|
| SLB-043.1 | `can_fire(Open, 11_999_999_999)` | `Err(Backoff)` |
| SLB-043.2 | `can_fire(Open, 12_000_000_000)` | `Ok(())` |
| SLB-043.3 | `can_fire(Close, 11_900_000_000)` | `Err(Backoff)` |
| SLB-043.4 | `can_fire(Flatten, 11_900_000_000)` | `Ok(())` |
| SLB-043.5 | (`set_backoff(11_000_000_000)` 之后)`can_fire(Open, 11_500_000_000)` | `Ok(())` |

### SLB-044 `on_fire` 对开平与对 Flatten/Hard 设的状态不同

- 规则:[Throttle](README.md#throttle)
- 级别:unit
- 里程碑:M2
- 前置:`TP0`;两个 `Throttle::default()`:X、Y
- 步骤:
  1. `X.on_fire(Close, 10_000_000_000)`;`snapshot`
  2. `Y.on_fire(Hard, 10_000_000_000)`;`snapshot`
- 期望:
  - 步骤 1:`in_flight = 1`、`cooldown_until_ens = 10_500_000_000`,其余字段 0
  - 步骤 2:`hard_rearm_until_ens = 11_000_000_000`,其余字段 0(`in_flight = 0`、冷却 0)
- 实现:`crates/hft-strat-lib/tests/throttle.rs::slb_044_on_fire_effects`

### SLB-045 `on_outcome` 的连续失败计数

- 规则:[Throttle](README.md#throttle)
- 级别:table
- 里程碑:M2
- 前置:`TP0`(`max_consec_fail = 3`);每行从 `Throttle::default()` 起,每次 `on_outcome` 之前 `on_fire(Open, t)`,t 每次加 1_000_000_000
- 实现:`crates/hft-strat-lib/tests/throttle.rs::slb_045_consec_fail`

| 行 | outcome 序列 | 期望(各次返回) |
|---|---|---|
| SLB-045.1 | Failed | None |
| SLB-045.2 | Failed, Failed | None, None |
| SLB-045.3 | Failed ×3 | None, None, RequestHalt |
| SLB-045.4 | Failed ×4 | None, None, RequestHalt, RequestHalt |
| SLB-045.5 | Failed, Failed, Filled, Failed | None, None, None, None |
| SLB-045.6 | Failed, Failed, NoFill, Failed, Failed | None ×5 |
| SLB-045.7 | Failed, Failed, Gated, Failed | None, None, None, RequestHalt |
| SLB-045.8 | Gated ×10 | None ×10 |
| SLB-045.9 | Failed, Failed, Rollback, Failed | None, None, EnterFallback, RequestHalt |

### SLB-046 `max_consec_fail` 为 0 不熔断,计数饱和于 255

- 规则:[Throttle](README.md#throttle)、[参数](README.md#参数)
- 级别:table
- 里程碑:M2
- 前置:每行只改 `max_consec_fail`,其余同 `TP0`;outcome 全为 Failed
- 实现:`crates/hft-strat-lib/tests/throttle.rs::slb_046_max_consec_fail_edges`

| 行 | `max_consec_fail` / 次数 | 期望 |
|---|---|---|
| SLB-046.1 | 0 / 300 | 全部 None;快照 `consec_fail = 255` |
| SLB-046.2 | 1 / 1 | RequestHalt |
| SLB-046.3 | 2 / 2 | None, RequestHalt |
| SLB-046.4 | 255 / 254 | 全部 None |
| SLB-046.5 | 255 / 300 | 第 255 次起 RequestHalt;快照 `consec_fail = 255` |

### SLB-047 `Rollback` 进入回退期,`can_fire` 不看回退期

- 规则:[Throttle](README.md#throttle)
- 级别:unit
- 里程碑:M2
- 前置:`TP0`;`on_fire(Open, 10_000_000_000)`
- 步骤:
  1. `on_outcome(Rollback, 10_200_000_000)`
  2. `in_fallback(70_199_999_999)`、`in_fallback(70_200_000_000)`
  3. `can_fire(Open, 10_600_000_000)`
- 期望:
  - 步骤 1 返回 `EnterFallback`
  - 步骤 2 依次 `true`、`false`
  - 步骤 3 `Ok(())`
- 实现:`crates/hft-strat-lib/tests/throttle.rs::slb_047_rollback_fallback`

### SLB-048 每种结果都清 `in_flight`

- 规则:[Throttle](README.md#throttle)
- 级别:table
- 里程碑:M2
- 前置:`TP0`;每行从 `Throttle::default()` 起 `on_fire(Open, 10_000_000_000)`,再 `on_outcome(o, 10_100_000_000)`,然后 `can_fire(Open, 10_500_000_000)`
- 实现:`crates/hft-strat-lib/tests/throttle.rs::slb_048_outcome_clears_inflight`

| 行 | o | 期望(返回 / `can_fire`) |
|---|---|---|
| SLB-048.1 | Filled | None / Ok |
| SLB-048.2 | NoFill | None / Ok |
| SLB-048.3 | Failed | None / Ok |
| SLB-048.4 | Rollback | EnterFallback / Ok |
| SLB-048.5 | Gated | None / Ok |

### SLB-049 `set_halted` 只拦 Open

- 规则:[Throttle](README.md#throttle)
- 级别:unit
- 里程碑:M2
- 前置:`TP0`;`Throttle::default()`
- 步骤:
  1. `set_halted(true)`;对四类动作 `can_fire(k, 10_000_000_000)`
  2. `set_halted(false)`;`can_fire(Open, 10_000_000_000)`
- 期望:
  - 步骤 1:Open → `Err(Halted)`;Close、Flatten、Hard → `Ok(())`
  - 步骤 2:`Ok(())`
- 实现:`crates/hft-strat-lib/tests/throttle.rs::slb_049_halted_mirror`

### SLB-050 `reset_open_state` 只清冷却与回退期

- 规则:[快速部分可用的构件](README.md#快速部分可用的构件)
- 级别:unit
- 里程碑:M2
- 前置:`TP0`;依次 `on_fire(Open, 10_000_000_000)`、`on_outcome(Failed, 10_050_000_000)`、`on_outcome(Rollback, 10_100_000_000)`、`on_fire(Open, 10_200_000_000)`、`on_fire(Flatten, 10_200_000_000)`、`set_halted(true)`、`set_backoff(12_000_000_000)`
- 步骤:
  1. `reset_open_state()`;`snapshot`
  2. `can_fire(Open, 10_300_000_000)`、`in_fallback(10_300_000_000)`
- 期望:
  - 步骤 1:`cooldown_until_ens = 0`、`in_flight = 1`、`consec_fail = 1`、`fallback_until_ens = 0`、`backoff_until_ens = 12_000_000_000`、`halted = 1`、`hard_rearm_until_ens = 11_200_000_000`
  - 步骤 2:`Err(InFlight)`、`false`
- 实现:`crates/hft-strat-lib/tests/throttle.rs::slb_050_reset_open_state`

### SLB-051 Flatten/Hard 永不因在途、冷却、halted 或退避被拒

- 规则:[Throttle](README.md#throttle)
- 级别:prop
- 里程碑:M2
- 性质:任意 `on_fire`/`on_outcome`/`set_halted`/`set_backoff`/`reset_open_state` 序列之后:`can_fire(Flatten|Hard, t)` 只可能返回 `Ok` 或 `HardRearm`;`halted` 为真时 `can_fire(Open, t)` 恒为 `Err`;`can_fire(Close, t)` 永不返回 `Halted`;`on_outcome(Gated)` 前后 `consec_fail` 相等
- 生成:序列长 1–300;时刻单调非降,步长 0–2_000_000_000;参数各项 0–10_000 ms、`max_consec_fail` 0–10
- 参考:上述断言,另与按 README 放行表写的朴素参考实现逐次比较返回值
- 例数:PR 10 000;每夜 1 000 000
- 实现:`crates/hft-strat-lib/tests/props.rs::slb_051_throttle_invariants`

### SLB-052 `ThrottleParams::ns` 换算三个毫秒字段

- 规则:[单位约定](README.md#单位约定)
- 级别:table
- 里程碑:M2
- 前置:无
- 实现:`crates/hft-strat-lib/tests/throttle.rs::slb_052_params_ns`

| 行 | 输入 | 期望 |
|---|---|---|
| SLB-052.1 | `TP0` 的 `ThrottleParams` | 500_000_000、60_000_000_000、1_000_000_000;`max_consec_fail = 3` |
| SLB-052.2 | 三个毫秒字段都为 `u32::MAX` | 各为 4_294_967_295_000_000 |

### SLB-053 `Throttle` 快照为 35 B、按字段声明顺序

- 规则:[快速部分可用的构件](README.md#快速部分可用的构件)、[输出与 Advice 载荷](README.md#输出与-advice-载荷)
- 级别:unit
- 里程碑:M2
- 前置:SLB-050 前置的序列(不调 `reset_open_state`)之后的 `Throttle` X
- 步骤:
  1. `X.snapshot`
  2. 新 `Throttle::default()` Y `restore`;对 Y 与 X 在时刻 10_300_000_000、10_800_000_000、12_000_000_000 各调四类 `can_fire`
- 期望:
  - 步骤 1 写出 35 B:`cooldown_until_ens` 10_700_000_000、`in_flight` 01、`consec_fail` 01、`fallback_until_ens` 70_100_000_000、`backoff_until_ens` 12_000_000_000、`halted` 01、`hard_rearm_until_ens` 11_200_000_000(i64 小端)
  - 步骤 2:`restore` 返回 `Ok`;12 次 `can_fire` 的结果 X 与 Y 逐项相同;Y 的快照与 X 的逐字节相同
- 实现:`crates/hft-strat-lib/tests/throttle.rs::slb_053_snapshot_layout`

### SLB-054 `Throttle::restore` 遇截断字节返回错误且不改状态

- 规则:[输出与 Advice 载荷](README.md#输出与-advice-载荷)
- 级别:unit
- 里程碑:M2
- 前置:SLB-053 步骤 1 的 35 B;目标为 `Throttle::default()` 后 `set_halted(true)`
- 步骤:
  1. 用前 34 B `restore`
- 期望:
  - 返回 `Err(RestoreError)`;目标快照仍为:`halted = 1`,其余字段 0
- 实现:`crates/hft-strat-lib/tests/throttle.rs::slb_054_restore_truncated`

### SLB-055 跨重启后放行与拒绝序列与不停机的参考运行相同

- 规则:[单位约定](README.md#单位约定)、[输出与 Advice 载荷](README.md#输出与-advice-载荷)、[Throttle](README.md#throttle)、[ConfirmWindow](README.md#confirmwindow)、[TriggerPx(预计算触发价)](README.md#triggerpx预计算触发价)
- 级别:prop
- 里程碑:M2
- 性质:对 `Throttle`、`ConfirmWindow`、`TriggerPx`、`Expiring<i64>` 各跑一条随机脚本两次:运行 R 在第 i 步后 `snapshot` → 模拟重启(TSC 从新原点以另一频率计数、`boot_seq` 前进、engine_ns 连续,停机不计)→ 新实例 `restore` → 继续其余步骤;参考 R0 不停机,只在同一位置让 `boot_seq` 前进并对 `TriggerPx` 调 `invalidate()`。两次运行的 `can_fire`、`observe`、`hit_*`、`get` 返回序列逐项相同;R 中对 `basis_seq < boot_seq` 的 `Expiring` 值 `get` 一律为 `Stale`
- 生成:脚本 1–200 步;i 均匀取;新 TSC 原点 ∈ [0, 10^15]、`tsc_per_ms` ∈ {1_000_000, 3_000_000};engine_ns 单调非降
- 参考:R0
- 例数:PR 5 000;每夜 1 000 000
- 实现:`crates/hft-strat-lib/tests/props.rs::slb_055_restart_equivalence`

### SLB-056 快照字节不含 TSC:engine_ns 相同、TSC 不同的两次运行快照逐字节相同

- 规则:[单位约定](README.md#单位约定)、[输出与 Advice 载荷](README.md#输出与-advice-载荷)
- 级别:unit
- 里程碑:M2
- 前置:两套 `Now`:N1(TSC 原点 0、`tsc_per_ms = 3_000_000`)与 N2(TSC 原点 10^15、`tsc_per_ms = 1_000_000`),二者的 engine_ns 序列相同;`Expiring::put_advice` 的 `basis_tsc` 在两边分别取使 `ens_of(basis_tsc)` 相同的值
- 步骤:
  1. 在 N1、N2 下对 `Throttle`、`ConfirmWindow`、`TriggerPx`、`Expiring<i64>` 跑同一组 20 步调用(时刻取 `now.engine_ns`)
  2. 各自 `snapshot`
- 期望:
  - 4 个构件在 N1 与 N2 下的快照逐字节相同
- 实现:`crates/hft-strat-lib/tests/restart.rs::slb_056_snapshot_has_no_tsc`

### SLB-057 快速部分构件对任意整数输入不 panic

- 规则:[快速部分可用的构件](README.md#快速部分可用的构件)、[延迟与资源契约](README.md#延迟与资源契约)
- 级别:prop
- 里程碑:M2
- 性质:`SpreadCalc` 三个函数、`CostLine::set`、`TriggerPx::update`/`hit_*`、`ConfirmWindow::observe`、`DirtyFilter::check`、`Throttle` 全部方法、`ImpactModel::vwap`、`Expiring` 全部方法对任意 i64/u64/u32 输入(含 `i64::MIN`、`i64::MAX`、时刻倒退)不 panic;`on_fire(Open, i64::MAX)` 之后快照中 `cooldown_until_ens = i64::MAX`
- 生成:各参数从 {i64::MIN, −1, 0, 1, i64::MAX} ∪ 任意值中抽取
- 参考:不 panic(`catch_unwind` 断言)与上面的饱和值
- 例数:PR 10 000;每夜 1 000 000
- 实现:`crates/hft-strat-lib/tests/props.rs::slb_057_no_panic`

### SLB-058 `ImpactModel::vwap` 在 `BK1` 上的结果

- 规则:[快速部分可用的构件](README.md#快速部分可用的构件)、[ImpactModel::vwap](README.md#impactmodelvwap)、[参数](README.md#参数)
- 级别:table
- 里程碑:M2
- 前置:`BK1`;未写明时 `book_frac = 500_000_000`(50%)、`max_levels = 20`
- 实现:`crates/hft-strat-lib/tests/impact.rs::slb_058_vwap_bk1`

| 行 | `(side, qty)` 与参数改动 | 期望 |
|---|---|---|
| SLB-058.1 | (Buy, 20_000_000) | `Top(6_001_000)` |
| SLB-058.2 | (Buy, 25_000_000)(边界) | `Top(6_001_000)` |
| SLB-058.3 | (Buy, 25_000_001) | `Walked{vwap: 6_001_000, levels: 1, impact: 0}` |
| SLB-058.4 | (Buy, 100_000_000) | `Walked{vwap: 6_001_050, levels: 2, impact: 8_332}` |
| SLB-058.5 | (Buy, 70_000_000) | `Walked{vwap: 6_001_029, levels: 2, impact: 4_833}`(6_001_028.57 向上) |
| SLB-058.6 | (Buy, 350_000_000) | `Walked{vwap: 6_001_315, levels: 3, impact: 52_491}` |
| SLB-058.7 | (Buy, 350_000_001) | `Insufficient{avail: 350_000_000}` |
| SLB-058.8 | (Buy, 200_000_000),`max_levels = 2` | `Insufficient{avail: 150_000_000}` |
| SLB-058.9 | (Buy, 150_000_000),`max_levels = 2` | `Walked{vwap: 6_001_067, levels: 2, impact: 11_165}` |
| SLB-058.10 | (Buy, 20_000_000),`max_levels = 0` | `Top(6_001_000)` |
| SLB-058.11 | (Buy, 30_000_000),`max_levels = 0` | `Insufficient{avail: 0}` |
| SLB-058.12 | (Buy, 350_000_000),`max_levels = 200` | `Walked{vwap: 6_001_315, levels: 3, impact: 52_491}`(按 64 处理) |
| SLB-058.13 | (Buy, 20_000_000),`book_frac = 0` | `Walked{vwap: 6_001_000, levels: 1, impact: 0}` |
| SLB-058.14 | (Buy, 20_000_000),`book_frac = −1` | `Walked{vwap: 6_001_000, levels: 1, impact: 0}` |
| SLB-058.15 | (Sell, 70_000_000) | `Walked{vwap: 5_999_971, levels: 2, impact: 4_833}`(5_999_971.43 向下) |
| SLB-058.16 | (Sell, 20_000_000) | `Top(6_000_000)` |
| SLB-058.17 | (Buy, 50_000_000),`book_frac = 1_000_000_000` | `Top(6_001_000)` |
| SLB-058.18 | (Buy, 50_000_001),`book_frac = 1_000_000_000` | `Walked{vwap: 6_001_001, levels: 2, impact: 167}` |
| SLB-058.19 | (Buy, 0) | `Top(6_001_000)` |

### SLB-059 簿不可用或该侧无档时为不足、可用量 0

- 规则:[快速部分可用的构件](README.md#快速部分可用的构件)
- 级别:table
- 里程碑:M2
- 前置:`book_frac = 500_000_000`、`max_levels = 20`;每行一个测试簿
- 实现:`crates/hft-strat-lib/tests/impact.rs::slb_059_vwap_unusable_book`

| 行 | 簿 | 调用 | 期望 |
|---|---|---|---|
| SLB-059.1 | `BK1` 的档位,`status = Uninit`(只订阅 BBO 的 inst) | (Buy, 20_000_000) | `Insufficient{avail: 0}` |
| SLB-059.2 | 同上,`status = Stale` | (Buy, 20_000_000) | `Insufficient{avail: 0}` |
| SLB-059.3 | 同上,`status = Resyncing` | (Sell, 20_000_000) | `Insufficient{avail: 0}` |
| SLB-059.4 | `Live`,只有买盘(卖盘为空) | (Buy, 20_000_000) | `Insufficient{avail: 0}` |
| SLB-059.5 | `Live`,只有买盘 | (Sell, 20_000_000) | `Top(6_000_000)` |
| SLB-059.6 | `Live`,只有 5 档(分档快照流 N = 5),每档 10_000_000 | (Buy, 60_000_000),`max_levels = 20` | `Insufficient{avail: 50_000_000}` |

### SLB-060 `vwap` 只按需读取簿:Top 不读逐档,逐档至多读 `max_levels` 档

- 规则:[ImpactModel::vwap](README.md#impactmodelvwap)
- 级别:table
- 里程碑:M2
- 前置:`BK1` 的计数测试簿;`book_frac = 500_000_000`
- 实现:`crates/hft-strat-lib/tests/impact.rs::slb_060_vwap_reads`

| 行 | 调用 | 期望 |
|---|---|---|
| SLB-060.1 | (Buy, 20_000_000),`max_levels = 20` | `levels` 调用 0 次 |
| SLB-060.2 | (Buy, 70_000_000),`max_levels = 20` | `levels` 调用 1 次,`out.len() = 20` |
| SLB-060.3 | (Buy, 70_000_000),`max_levels = 200` | `levels` 调用 1 次,`out.len() = 64` |

### SLB-061 `Walked` 的均价落在顶档与最后一档之间,数量守恒

- 规则:[ImpactModel::vwap](README.md#impactmodelvwap)
- 级别:prop
- 里程碑:M2
- 性质:任意 Live 簿与请求下:`Walked{vwap, levels, impact}` → `vwap` 介于顶档价与第 `levels` 档价之间(含端点),各档消耗量之和 = qty,`impact = |vwap.ratio(top)|`;`Insufficient{avail}` → `avail` = 前 min(max_levels, 64, 档数) 档量之和且 < qty;`Top(p)` → `p` = 顶档价且 `top_qty × book_frac ≥ qty × 10^9`
- 生成:1–80 档,价按方向单调,每档量 1–10^10;qty 0–10^11;`book_frac` −1–2 × 10^9;`max_levels` 0–255;两侧随机
- 参考:i128 朴素参考实现(逐档列表)
- 例数:PR 10 000;每夜 1 000 000
- 实现:`crates/hft-strat-lib/tests/props.rs::slb_061_vwap_bounds`

### SLB-062 `Expiring::get` 的三种结果与边界

- 规则:[快速部分可用的构件](README.md#快速部分可用的构件)、[输出与 Advice 载荷](README.md#输出与-advice-载荷)
- 级别:table
- 里程碑:M2
- 前置:`e = Expiring::new(0i64)`;除 .1 外先 `put(7, 1_000_000_000_000, 50)`;每行一个 `Now{engine_ns, boot_seq}`
- 实现:`crates/hft-strat-lib/tests/expiring.rs::slb_062_get`

| 行 | `Now` | 期望 |
|---|---|---|
| SLB-062.1 | 未 put;(999_999_999_999, 40) | `Missing` |
| SLB-062.2 | (999_999_999_999, 40) | `Fresh(&7)` |
| SLB-062.3 | (1_000_000_000_000, 40) | `Fresh(&7)` |
| SLB-062.4 | (1_000_000_000_001, 40) | `Stale(&7)` |
| SLB-062.5 | (999_999_999_999, 51) | `Stale(&7)`(`basis_seq` 50 < `boot_seq` 51) |
| SLB-062.6 | (999_999_999_999, 50) | `Fresh(&7)` |

### SLB-063 `put_advice` 的有效期从依据时刻起算

- 规则:[快速部分可用的构件](README.md#快速部分可用的构件)
- 级别:unit
- 里程碑:M2
- 前置:公共 `Now` 锚点(`ens_of(30_000_000_000) = 15_000_000_000`);`e = Expiring::new(0i64)`
- 步骤:
  1. `put_advice(9, 90, 30_000_000_000, 5, now{engine_ns: 15_002_000_000, boot_seq: 80})`
  2. `get(now{engine_ns: 15_005_000_000, boot_seq: 80})`、`get(now{engine_ns: 15_005_000_001, boot_seq: 80})`
- 期望:
  - 步骤 2 依次 `Fresh(&9)`、`Stale(&9)`(`valid_until_ens = 15_005_000_000`)
- 实现:`crates/hft-strat-lib/tests/expiring.rs::slb_063_put_advice_basis_time`

### SLB-064 到达时刻不同、依据时刻相同的两条 Advice 有效期相同

- 规则:[快速部分可用的构件](README.md#快速部分可用的构件)
- 级别:unit
- 里程碑:M2
- 前置:同 SLB-063;两个实例 e1、e2
- 步骤:
  1. `e1.put_advice(9, 90, 30_000_000_000, 5, now{engine_ns: 15_001_000_000, ..})`;`e2.put_advice(9, 90, 30_000_000_000, 5, now{engine_ns: 15_004_000_000, ..})`
  2. 两者各 `snapshot`(`put` 写 i64)
- 期望:
  - 两份快照逐字节相同,其中 `valid_until_ens = 15_005_000_000`
- 实现:`crates/hft-strat-lib/tests/expiring.rs::slb_064_arrival_time_irrelevant`

### SLB-065 `Expiring` 快照:17 B 头 + 值,未设置时不写值

- 规则:[快速部分可用的构件](README.md#快速部分可用的构件)、[输出与 Advice 载荷](README.md#输出与-advice-载荷)
- 级别:unit
- 里程碑:M2
- 前置:`put = |v, w| w.put_i64(*v)`、`get = |r| r.get_i64()`;e1 = `Expiring::new(0)` 后 `put(7, 1_000_000_000_000, 50)`;e2 = `Expiring::new(0)`(未设置)
- 步骤:
  1. `e1.snapshot(w, put)`、`e2.snapshot(w', put)`
  2. 两份字节各 `restore` 进新实例,`get(now{engine_ns: 999_999_999_999, boot_seq: 40})`
- 期望:
  - 步骤 1:e1 写 25 B(`set` 01、`valid_until_ens` 1_000_000_000_000、`basis_seq` 50、值 7);e2 写 17 B(`set` 00,后两项 0)
  - 步骤 2:前者 `Fresh(&7)`,后者 `Missing`
- 实现:`crates/hft-strat-lib/tests/expiring.rs::slb_065_snapshot_layout`

### SLB-066 重启前的依据在重启后一律过期

- 规则:[快速部分可用的构件](README.md#快速部分可用的构件)、[单位约定](README.md#单位约定)
- 级别:unit
- 里程碑:M2
- 前置:e = `Expiring::new(0)` 后 `put(7, i64::MAX, 50)`(`boot_seq` 40 的启动内)
- 步骤:
  1. `snapshot`;新实例 `restore`;`get(now{engine_ns: 2_000_000_000_000, boot_seq: 100})`
  2. `put(8, i64::MAX, 120)`;`get(now{engine_ns: 2_000_000_000_001, boot_seq: 100})`
- 期望:
  - 步骤 1:`Stale(&7)`(有效期未到,但依据早于本次启动)
  - 步骤 2:`Fresh(&8)`
- 实现:`crates/hft-strat-lib/tests/expiring.rs::slb_066_prior_boot_stale`

### SLB-067 `Expiring::restore` 在值读取失败时返回错误且不改状态

- 规则:[输出与 Advice 载荷](README.md#输出与-advice-载荷)
- 级别:unit
- 里程碑:M2
- 前置:SLB-065 中 e1 的 25 B 截去最后 1 B(24 B);目标 = `Expiring::new(0)` 后 `put(3, 500, 1)`
- 步骤:
  1. 以 SLB-065 的 `get` `restore`
- 期望:
  - 返回 `Err(RestoreError)`;目标 `get(now{engine_ns: 400, boot_seq: 1})` 仍为 `Fresh(&3)`
- 实现:`crates/hft-strat-lib/tests/expiring.rs::slb_067_restore_error_keeps_state`

### SLB-068 `next_settle_ns` 跳变时以跳变前最后一次费率记一次结算

- 规则:[快速部分可用的构件](README.md#快速部分可用的构件)、[FundingStats](README.md#fundingstats)
- 级别:unit
- 里程碑:M4
- 前置:`fs = FundingStats::new()`;`FundingView` 写作 `{rate, next_settle_ns, interval_s}`(`prev_interval_s`、`upd_tsc` 取任意值,不影响结果)
- 步骤:
  1. `observe({100_000, T0, 28_800}, T0 − H)`;`summary(24 × H, T0 − H, H)`
  2. `observe({120_000, T0, 28_800}, T0 − H / 2)`
  3. `observe({50_000, T0 + 8 × H, 28_800}, T0 + 1_000_000_000)`
  4. `summary(24 × H, T0 + 2_000_000_000, H)`
- 期望:
  - 步骤 1 返回 `Normal`(第一次看到周期不算变化);summary:`n_settles = 0`、`interval_s = 28_800`、`interval_changed_at_ns = 0`、`flags` 不含 `INTERVAL_CHANGED`、`NO_DATA`、`STALE`
  - 步骤 2 返回 `Normal`
  - 步骤 3 返回 `Settled(SettleRec{at_ns: T0, rate: 120_000, flags: 0, _pad: 0})`
  - 步骤 4:`n_settles = 1`、`realized_sum = 120_000`、`predicted = 50_000`、`predicted_daily = 150_000`、`next_settle_ns = T0 + 8 × H`、`interval_s = 28_800`、`flags = 0`
- 实现:`crates/hft-strat-lib/tests/funding.rs::slb_068_settle_detect`

### SLB-069 跳变 ≥ 2 个周期只记一条并置 GAP

- 规则:[FundingStats](README.md#fundingstats)
- 级别:table
- 里程碑:M4
- 前置:每行在新的 `fs` 上 `observe({120_000, T0, 28_800}, T0 − H)`,再 `observe({50_000, T0 + J, 28_800}, T0 + 1_000_000_000)`
- 实现:`crates/hft-strat-lib/tests/funding.rs::slb_069_gap_boundary`

| 行 | J | 期望(第二次的返回) |
|---|---|---|
| SLB-069.1 | 16 × H − 1 | `Settled{at_ns: T0, rate: 120_000, flags: 0}` |
| SLB-069.2 | 16 × H | `Settled{at_ns: T0, rate: 120_000, flags: GAP(4)}` |
| SLB-069.3 | 16 × H + 1 | `Settled{.., flags: GAP}` |
| SLB-069.4 | 24 × H | `Settled{.., flags: GAP}`;之后 `summary` 的 `n_settles = 1`、`flags` 含 `GAP` |

### SLB-070 不推周期时按观测值定周期、不判 GAP

- 规则:[FundingStats](README.md#fundingstats)
- 级别:unit
- 里程碑:M4
- 前置:`fs = FundingStats::new()`
- 步骤:
  1. `observe({1, T0, 0}, T0 − H)`
  2. `observe({2, T0 + 8 × H, 0}, T0 + 1_000_000_000)`;`summary(100 × H, T0 + 2_000_000_000, 100 × H)`
  3. `observe({3, T0 + 32 × H, 0}, T0 + 8 × H + 1_000_000_000)`
- 期望:
  - 步骤 1 `Normal`
  - 步骤 2 返回 `Settled(SettleRec{at_ns: T0, rate: 1, flags: 0})`(观测周期 28_800 是第一次看到,不算变化);summary `interval_s = 28_800`
  - 步骤 3 返回 `SettledChanged{rec: SettleRec{at_ns: T0 + 8 × H, rate: 2, flags: 0}, old_s: 28_800, new_s: 86_400}`(跳变 3 个旧周期,不置 GAP)
- 实现:`crates/hft-strat-lib/tests/funding.rs::slb_070_observed_interval_no_gap`

### SLB-071 观测周期按 60 s 的整数倍 HalfEven 取整

- 规则:[FundingStats](README.md#fundingstats)
- 级别:table
- 里程碑:M4
- 前置:每行在新的 `fs` 上 `observe({1, T0, 0}, T0 − H)`,再 `observe({2, T0 + J, 0}, T0 + J)`,读 `summary(100 × H, T0 + J, 100 × H).interval_s`
- 实现:`crates/hft-strat-lib/tests/funding.rs::slb_071_interval_rounding`

| 行 | J(秒 × 10^9) | 期望 `interval_s` |
|---|---|---|
| SLB-071.1 | 28_829 s | 28_800 |
| SLB-071.2 | 28_830 s | 28_800(480.5 → 480) |
| SLB-071.3 | 28_831 s | 28_860 |
| SLB-071.4 | 28_890 s | 28_920(481.5 → 482) |
| SLB-071.5 | 30 s | 0(0.5 → 0) |
| SLB-071.6 | 90 s | 120(1.5 → 2) |

### SLB-072 推送的周期变化返回 `IntervalChanged` 并在窗口内置标志

- 规则:[FundingStats](README.md#fundingstats)
- 级别:unit
- 里程碑:M4
- 前置:`fs = FundingStats::new()`;`observe({1, T0, 28_800}, T0 − 2 × H)`
- 步骤:
  1. `observe({1, T0, 14_400}, T0 − H)`
  2. `summary(8 × H, T0 − H + 1, 100 × H)`
  3. `summary(8 × H, T0 + 10 × H, 100 × H)`
- 期望:
  - 步骤 1 返回 `IntervalChanged{old_s: 28_800, new_s: 14_400}`
  - 步骤 2:`interval_s = 14_400`、`interval_changed_at_ns = T0 − H`、`flags` 含 `INTERVAL_CHANGED`
  - 步骤 3:`interval_changed_at_ns = T0 − H`,`flags` 不含 `INTERVAL_CHANGED`(变化早于窗口起点 T0 + 2 × H)
- 实现:`crates/hft-strat-lib/tests/funding.rs::slb_072_interval_changed`

### SLB-073 同一事件既结算又改周期返回 `SettledChanged`

- 规则:[快速部分可用的构件](README.md#快速部分可用的构件)、[FundingStats](README.md#fundingstats)
- 级别:unit
- 里程碑:M4
- 前置:`fs = FundingStats::new()`;`observe({1, T0, 28_800}, T0 − H)`
- 步骤:
  1. `observe({5, T0 + 4 × H, 14_400}, T0 + 1_000_000_000)`
- 期望:
  - 返回 `SettledChanged{rec: SettleRec{at_ns: T0, rate: 1, flags: 0}, old_s: 28_800, new_s: 14_400}`(跳变 = 1 个新周期,不置 GAP);`interval_changed_at_ns = T0 + 1_000_000_000`
- 实现:`crates/hft-strat-lib/tests/funding.rs::slb_073_settled_changed`

### SLB-074 `next_settle_ns` 回退的事件只更新最近事件时刻

- 规则:[FundingStats](README.md#fundingstats)
- 级别:unit
- 里程碑:M4
- 前置:SLB-068 步骤 3 之后的 `fs`(`last_next_ns = T0 + 8 × H`、`last_rate = 50_000`)
- 步骤:
  1. `observe({999, T0, 28_800}, T0 + 2_000_000_000)`
  2. `summary(24 × H, T0 + 2_000_000_000 + H, H)`、`summary(24 × H, T0 + 2_000_000_001 + H, H)`
- 期望:
  - 步骤 1 返回 `Normal`
  - 步骤 2 两次:`predicted = 50_000`、`next_settle_ns = T0 + 8 × H`、`n_settles = 1`;前者 `flags` 不含 `STALE`(距最近事件恰为 H),后者含 `STALE`
- 实现:`crates/hft-strat-lib/tests/funding.rs::slb_074_regression_ignored`

### SLB-075 `summary` 的聚合项

- 规则:[快速部分可用的构件](README.md#快速部分可用的构件)、[FundingStats](README.md#fundingstats)
- 级别:unit
- 里程碑:M4
- 前置:`fs = FundingStats::new()`;`prefill_settled` 5 条:`at_ns = T0 + k × 8 × H`(k = 0..4),费率依次 100_000、−50_000、0、−20_000、−10_000
- 步骤:
  1. `summary(40 × H, T0 + 32 × H, 1_000 × H)`
- 期望:
  - `n_settles = 5`、`n_positive = 1`、`n_negative = 3`、`neg_streak = 2`、`realized_sum = 20_000`、`min_rate = −50_000`、`max_rate = 100_000`、`realized_daily = 12_000`;`predicted = 0`、`predicted_daily = 0`(无实时事件,`interval_s = 0`)、`next_settle_ns = 0`;`flags = STALE`(有记录、无实时事件)
- 实现:`crates/hft-strat-lib/tests/funding.rs::slb_075_summary_aggregates`

### SLB-076 汇总窗口含起点

- 规则:[FundingStats](README.md#fundingstats)
- 级别:table
- 里程碑:M4
- 前置:新的 `fs`;`prefill_settled` 1 条 `{at_ns: T0, rate: −7}`;`now_ns = T0 + 40 × H`;`stale_ns = 1_000 × H`
- 实现:`crates/hft-strat-lib/tests/funding.rs::slb_076_window_boundary`

| 行 | `window_ns` | 期望 |
|---|---|---|
| SLB-076.1 | 40 × H | `n_settles = 1`、`neg_streak = 1`、`min_rate = max_rate = −7` |
| SLB-076.2 | 40 × H − 1 | `n_settles = 0`、`min_rate = 0`、`max_rate = 0`、`neg_streak = 0`、`realized_sum = 0` |
| SLB-076.3 | 40 × H + 1 | `n_settles = 1` |

### SLB-077 `NO_DATA` 与 `STALE` 的判定

- 规则:[FundingStats](README.md#fundingstats)
- 级别:table
- 里程碑:M4
- 前置:每行新的 `fs`;`window_ns = 100 × H`
- 实现:`crates/hft-strat-lib/tests/funding.rs::slb_077_no_data_stale`

| 行 | 状态与 `summary(.., now_ns, stale_ns)` | 期望 `flags` |
|---|---|---|
| SLB-077.1 | `new()`;now T0,stale H | `NO_DATA`(不含 `STALE`) |
| SLB-077.2 | 只 `prefill_settled` 1 条(at T0 − H);now T0,stale H | `STALE`(不含 `NO_DATA`) |
| SLB-077.3 | `observe({1, T0 + H, 28_800}, T0)`;now T0 + H,stale H | 不含 `STALE` |
| SLB-077.4 | 同 .3;now T0 + H + 1 | `STALE` |
| SLB-077.5 | 同 .3;now T0,stale 0 | 不含 `STALE` |
| SLB-077.6 | 同 .3;now T0 + 1,stale 0 | `STALE` |

### SLB-078 环满且最旧记录晚于窗口起点时置 `TRUNCATED`

- 规则:[快速部分可用的构件](README.md#快速部分可用的构件)、[FundingStats](README.md#fundingstats)
- 级别:table
- 里程碑:M4
- 前置:新的 `fs`;`prefill_settled` 300 条 1 h 周期记录 `at_ns = T0 + k × H`(k = 0..299,费率 1)——环保留 k = 44..299;`now_ns = T0 + 299 × H + 1_000_000_000`;`stale_ns = 1_000 × H`
- 实现:`crates/hft-strat-lib/tests/funding.rs::slb_078_truncated`

| 行 | `window_ns` 与状态 | 期望 |
|---|---|---|
| SLB-078.1 | 240 × H(10 天) | 不含 `TRUNCATED`;`n_settles = 240` |
| SLB-078.2 | 264 × H(11 天) | 含 `TRUNCATED`;`n_settles = 256` |
| SLB-078.3 | 255 × H + 1_000_000_000(起点恰为最旧记录) | 不含 `TRUNCATED`;`n_settles = 256` |
| SLB-078.4 | 255 × H + 1_000_000_001 | 含 `TRUNCATED` |
| SLB-078.5 | 只 prefill 255 条(k = 45..299);窗口 1_000 × H | 不含 `TRUNCATED`(环未满);`n_settles = 255` |

### SLB-079 `daily` 的折日化、HalfEven、0 周期与饱和

- 规则:[快速部分可用的构件](README.md#快速部分可用的构件)
- 级别:table
- 里程碑:M4
- 前置:无
- 实现:`crates/hft-strat-lib/tests/funding.rs::slb_079_daily`

| 行 | `daily(rate, interval_s)` | 期望 |
|---|---|---|
| SLB-079.1 | (100_000, 28_800) | 300_000 |
| SLB-079.2 | (100_000, 14_400) | 600_000 |
| SLB-079.3 | (100_000, 3_600) | 2_400_000 |
| SLB-079.4 | (1, 172_800) | 0(0.5 → 0) |
| SLB-079.5 | (3, 172_800) | 2(1.5 → 2) |
| SLB-079.6 | (−3, 172_800) | −2 |
| SLB-079.7 | (−1, 172_800) | 0 |
| SLB-079.8 | (1, 50_000) | 2(1.728) |
| SLB-079.9 | (100_000, 0) | 0 |
| SLB-079.10 | (i64::MAX, 1) | i64::MAX |
| SLB-079.11 | (−i64::MAX, 1) | −i64::MAX |

### SLB-080 `realized_daily` 按窗口纳秒数折日化

- 规则:[FundingStats](README.md#fundingstats)
- 级别:table
- 里程碑:M4
- 前置:每行新的 `fs`,按所列 `prefill_settled`;`now_ns = T0 + H`;`stale_ns = 1_000 × H`
- 实现:`crates/hft-strat-lib/tests/funding.rs::slb_080_realized_daily`

| 行 | 记录与 `window_ns` | 期望 `realized_daily` |
|---|---|---|
| SLB-080.1 | `{T0, 1}`;48 × H | 0(0.5 → 0) |
| SLB-080.2 | `{T0, 3}`;48 × H | 2(1.5 → 2) |
| SLB-080.3 | `{T0, −3}`;48 × H | −2 |
| SLB-080.4 | `{T0, 20_000}`;24 × H | 20_000 |
| SLB-080.5 | `{T0, 5}`;0 | 0(`n_settles = 0`) |
| SLB-080.6 | `{T0, 5}`;−1 | 0 |

### SLB-081 汇总的 `GAP` 只看窗口内的记录

- 规则:[FundingStats](README.md#fundingstats)
- 级别:unit
- 里程碑:M4
- 前置:新的 `fs`;`prefill_settled` `{T0, 1, flags: GAP}` 与 `{T0 + 8 × H, 2, flags: 0}`;`stale_ns = 1_000 × H`
- 步骤:
  1. `summary(9 × H, T0 + 8 × H, ..)`
  2. `summary(8 × H − 1, T0 + 8 × H, ..)`
- 期望:
  - 步骤 1:`flags` 含 `GAP`
  - 步骤 2:`flags` 不含 `GAP`(GAP 记录在窗口外)
- 实现:`crates/hft-strat-lib/tests/funding.rs::slb_081_gap_flag_in_window`

### SLB-082 结算次数等于 `next_settle_ns` 严格增加的次数

- 规则:[FundingStats](README.md#fundingstats)
- 级别:prop
- 里程碑:M4
- 性质:任意 `Funding` 事件序列:`Settled` 与 `SettledChanged` 的返回次数之和 = 相邻两次(按处理顺序、忽略回退事件)`next_settle_ns` 严格增加且前值 ≠ 0 的次数;`IntervalChanged` 与 `SettledChanged` 之和 = 周期值变化次数(第一次从 0 变为非 0 不计);环内 `at_ns` 严格递增;`observe`/`summary` 不 panic
- 生成:1–2_000 个事件;`next_settle_ns` 以 70% 不变、25% 前进 1–3 个周期、5% 回退;`interval_s` 从 {0, 3_600, 14_400, 28_800} 抽取并以 2% 概率变化;费率 ±10^7
- 参考:按 README 第 1–3 步写的朴素参考实现
- 例数:PR 5 000;每夜 1 000 000
- 实现:`crates/hft-strat-lib/tests/funding_seq.rs::slb_082_settle_count`

### SLB-083 周期 8 h → 4 h → 1 h 每次变化恰好一次周期变化结果

- 规则:[FundingStats](README.md#fundingstats)
- 级别:unit
- 里程碑:M4
- 前置:新的 `fs`;事件序列:每期结算前推送 2 次、结算后 1 次;前 3 期周期 28_800,第 4 期起 14_400,第 7 期起 3_600(改周期的推送同时带新的 `next_settle_ns`)
- 步骤:
  1. 依次 `observe` 全部事件,统计返回值
- 期望:
  - `IntervalChanged` 0 次、`SettledChanged` 2 次(28_800 → 14_400、14_400 → 3_600),`Settled` 与 `SettledChanged` 合计 = 结算次数;结束时 `summary().interval_s = 3_600`
- 实现:`crates/hft-strat-lib/tests/funding_seq.rs::slb_083_interval_sequence`

### SLB-084 `prefill_settled` 有序插入、同 `at_ns` 忽略、返回新插入条数

- 规则:[快速部分可用的构件](README.md#快速部分可用的构件)、[FundingStats](README.md#fundingstats)
- 级别:unit
- 里程碑:M4
- 前置:新的 `fs`;`stale_ns = H`
- 步骤:
  1. `prefill_settled(&[{T0 + 8 × H, 2}, {T0, 1}])`
  2. 再次 `prefill_settled(&[{T0 + 8 × H, 2}, {T0, 1}])`
  3. `prefill_settled(&[{T0, 99}])`
  4. `summary(100 × H, T0 + 9 × H, H)`
- 期望:
  - 步骤 1 返回 2;步骤 2 返回 0;步骤 3 返回 0
  - 步骤 4:`n_settles = 2`、`realized_sum = 3`(T0 的费率仍为 1)、`flags` 含 `STALE`、不含 `NO_DATA`
- 实现:`crates/hft-strat-lib/tests/funding.rs::slb_084_prefill_basic`

### SLB-085 实时检测的结算替换同一时刻的预热记录,与先后顺序无关

- 规则:[FundingStats](README.md#fundingstats)
- 级别:unit
- 里程碑:M4
- 前置:两个新的 `fs`:X、Y
- 步骤:
  1. X:`prefill_settled(&[{T0, 999}])`,然后 `observe({5, T0, 28_800}, T0 − H)`、`observe({6, T0 + 8 × H, 28_800}, T0 + 1_000_000_000)`
  2. Y:先做 X 的两次 `observe`,再 `prefill_settled(&[{T0, 999}])`
  3. 两者 `summary(24 × H, T0 + 2_000_000_000, H)`;两者 `snapshot`
- 期望:
  - 步骤 1 第二次 `observe` 返回 `Settled(SettleRec{at_ns: T0, rate: 5, flags: 0})`;步骤 2 的 `prefill_settled` 返回 0
  - 步骤 3:两者 `n_settles = 1`、`realized_sum = 5`;两份快照逐字节相同
- 实现:`crates/hft-strat-lib/tests/funding.rs::slb_085_realtime_wins`

### SLB-086 环满时保留最新的 256 条,返回仍在环中的新插入条数

- 规则:[快速部分可用的构件](README.md#快速部分可用的构件)
- 级别:table
- 里程碑:M4
- 前置:每行新的 `fs`;"k" 指 `at_ns = T0 + k × H` 的记录(费率 1)
- 实现:`crates/hft-strat-lib/tests/funding.rs::slb_086_prefill_ring_full`

| 行 | 先有记录(经 `prefill_settled`) | 本次 `prefill_settled` | 期望返回 / 本次之后最旧的 k |
|---|---|---|---|
| SLB-086.1 | 无 | k = 0..299 | 256 / 44 |
| SLB-086.2 | k = 6..255(250 条) | k = 0..9 | 6 / 0 |
| SLB-086.3 | k = 0..255(满) | k = −10..−1 | 0 / 0(环不变) |
| SLB-086.4 | k = 1..255(255 条) | k = −1、0 | 1 / 0 |
| SLB-086.5 | k = 0..255(满) | k = 256 | 1 / 1 |

### SLB-087 预热记录任意切块、重复、与实时检测交错,环内容与按时间顺序处理的参考相同

- 规则:[快速部分可用的构件](README.md#快速部分可用的构件)、[FundingStats](README.md#fundingstats)
- 级别:prop
- 里程碑:M4
- 性质:同一组历史结算记录与同一组实时 `Funding` 事件,任意切块、任意重复调用 `prefill_settled`、任意与 `observe` 交错之后,环内容(全部 `SettleRec`)、`summary` 与快照字节都与参考结果相同
- 生成:0–400 条历史记录(at 为 1 h 网格上的随机子集);0–300 个实时事件;切块 1–50 条;重复次数 1–3
- 参考:先按时间顺序处理实时事件、再一次性 `prefill_settled` 全部历史记录的结果
- 例数:PR 2 000;每夜 1 000 000
- 实现:`crates/hft-strat-lib/tests/funding_seq.rs::slb_087_prefill_order_free`

### SLB-088 `FundingStats` 快照恢复后继续处理与连续运行逐位相同

- 规则:[快速部分可用的构件](README.md#快速部分可用的构件)、[输出与 Advice 载荷](README.md#输出与-advice-载荷)
- 级别:unit
- 里程碑:M4
- 前置:脚本:10 个事件(含 3 次结算、1 次 GAP、1 次周期变化),之后再 5 个事件(含 1 次结算)
- 步骤:
  1. X 处理前 10 个事件后 `snapshot`;新 `FundingStats` Y `restore`
  2. X、Y 各处理后 5 个事件,每步后各 `summary(100 × H, now, H)`
- 期望:
  - 步骤 1 `restore` 返回 `Ok`;此时两者 `summary` 逐字段相同
  - 步骤 2:5 次返回值与 5 次 `summary` 逐字段相同;最终两份快照逐字节相同
- 实现:`crates/hft-strat-lib/tests/funding.rs::slb_088_snapshot_continue`

### SLB-089 `FundingStats::restore` 遇截断字节返回错误且不改状态

- 规则:[输出与 Advice 载荷](README.md#输出与-advice-载荷)
- 级别:unit
- 里程碑:M4
- 前置:SLB-088 步骤 1 的快照字节去掉最后 1 B;目标为只处理过 `observe({1, T0, 28_800}, T0 − H)` 的 `fs`
- 步骤:
  1. `restore`
- 期望:
  - 返回 `Err(RestoreError)`;目标的快照与调用前逐字节相同
- 实现:`crates/hft-strat-lib/tests/funding.rs::slb_089_restore_truncated`

### SLB-090 资金费合成序列金样

- 规则:[FundingStats](README.md#fundingstats)
- 级别:golden
- 里程碑:M4
- 输入:`fixtures/stratlib/funding/{interval_change,gap,regression,prefill_mix}.in.jsonl`(每行一个 `FundingView` 与 `now_ns`,或一条 `prefill_settled` 调用)
- 期望文件:`fixtures/stratlib/funding/{interval_change,gap,regression,prefill_mix}.expected.jsonl`(每步的 `FundingObs` 与 `summary(…)` 逐字段)
- 实现:`crates/hft-strat-lib/tests/golden.rs::slb_090_funding_golden`

### SLB-091 结算推送的费率与交易所资金费历史逐期核对

- 规则:[FundingStats](README.md#fundingstats)、[开发备忘](README.md#开发备忘)
- 级别:host
- 里程碑:M3
- 环境:dev2,hft-core 主网行情实例(每个 core venue 的 USDT 永续至少 3 个 inst 订阅 `FUNDING`),seq journal 本地保留
- 授权:无
- 步骤:
  1. 录一段跨越两个资金费结算整点的 seq journal
  2. 用 hft-py 读出其中的 `Funding` 记录,按记录顺序喂 `FundingStats::observe`,导出每次 `Settled`/`SettledChanged`
  3. 用各所资金费历史 REST 接口取同一时段的结算费率,逐期比对;记录推送是否带结算周期字段
- 通过标准:每个 (venue, inst, 结算时刻) 的 `SettleRec.rate` 与历史结算费率相等;不相等的 venue 的差异与原因写进该 `venue-*` README 与本组件"已结算费率 = 跳变前最后预测值"一条的出处
- 证据:`ops-log/tests/SLB-091-<UTC 日期>.md` 与比对表 `SLB-091-<UTC 日期>.csv`
- 实现:`crates/hft-strat-lib/tests/host/SLB-091.sh`

### SLB-092 `PercentileStats::new` 的配置校验

- 规则:[只给 Model 用的构件](README.md#只给-model-用的构件)、[参数](README.md#参数)
- 级别:table
- 里程碑:M4
- 前置:每行在 `C1` 上只改所列字段
- 实现:`crates/hft-strat-lib/tests/pct.rs::slb_092_new_validation`

| 行 | 改动 | 期望 |
|---|---|---|
| SLB-092.1 | 不改 | `Ok` |
| SLB-092.2 | `sample_ms = 0` | `Err(PctErr::Sample)` |
| SLB-092.3 | `n_windows = 0` | `Err(PctErr::Windows)` |
| SLB-092.4 | `n_windows = 5` | `Err(PctErr::Windows)` |
| SLB-092.5 | `windows_ms = [60_000, 10_000, 0, 0]` | `Err(PctErr::Windows)`(非升序) |
| SLB-092.6 | `windows_ms = [10_000, 10_000, 0, 0]` | `Err(PctErr::Windows)`(非严格升序) |
| SLB-092.7 | `windows_ms = [10_500, 60_000, 0, 0]` | `Err(PctErr::Windows)`(不是 1_000 的整数倍) |
| SLB-092.8 | `windows_ms = [0, 60_000, 0, 0]` | `Err(PctErr::Windows)` |
| SLB-092.9 | `n_windows = 1`,`windows_ms = [60_000, 5, 7, 9]` | `Ok`(只校验前 n_windows 项) |
| SLB-092.10 | `lo = hi = 0` | `Err(PctErr::Range)` |
| SLB-092.11 | `lo = 1_000_000`、`hi = −1_000_000` | `Err(PctErr::Range)` |
| SLB-092.12 | `bin = 0` | `Err(PctErr::Range)` |
| SLB-092.13 | `bin = −100_000` | `Err(PctErr::Range)` |
| SLB-092.14 | `bin = 300_000` | `Err(PctErr::Range)`(2_000_000 不是 300_000 的整数倍) |
| SLB-092.15 | `lo = 0`、`hi = 2_147_483_647`、`bin = 1` | `Err(PctErr::Range)`(`n_bins` = 2^31 > `i32::MAX`) |
| SLB-092.16 | `sides = 0` | `Err(PctErr::Sides)` |
| SLB-092.17 | `sides = 3` | `Err(PctErr::Sides)` |
| SLB-092.18 | `sides = 1` | `Ok` |

### SLB-093 样本夹到范围内并按 HalfEven 定箱号

- 规则:[PercentileStats](README.md#percentilestats)、[只给 Model 用的构件](README.md#只给-model-用的构件)
- 级别:table
- 里程碑:M4
- 前置:每行 `PercentileStats::new(&C1)`;`observe(1_500_000_000, x, 0)`、`observe(2_500_000_000, 0, 0)`(提交采样点 1);读 `value(Buy, 0, 500_000)` 与 `coverage(0).clamped`
- 实现:`crates/hft-strat-lib/tests/pct.rs::slb_093_bin_index`

| 行 | x | 期望 `value` / `clamped` |
|---|---|---|
| SLB-093.1 | 0 | 0 / 0 |
| SLB-093.2 | 149_999 | 100_000 / 0 |
| SLB-093.3 | 150_000 | 200_000 / 0(11.5 → 12) |
| SLB-093.4 | 250_000 | 200_000 / 0(12.5 → 12) |
| SLB-093.5 | −50_000 | 0 / 0(9.5 → 10) |
| SLB-093.6 | −150_000 | −200_000 / 0(8.5 → 8) |
| SLB-093.7 | 1_000_000 | 1_000_000 / 1(端箱) |
| SLB-093.8 | 1_000_001 | 1_000_000 / 1 |
| SLB-093.9 | −1_000_001 | −1_000_000 / 1 |
| SLB-093.10 | i64::MAX | 1_000_000 / 1 |

### SLB-094 采样点取区间内最后一次观测,跨入新区间时提交

- 规则:[PercentileStats](README.md#percentilestats)
- 级别:unit
- 里程碑:M4
- 前置:`PercentileStats::new(&C1)`
- 步骤:
  1. `observe(1_200_000_000, 100_000, −100_000)`、`observe(1_700_000_000, 300_000, −300_000)`;读 `coverage(0)`
  2. `observe(2_100_000_000, 0, 0)`;读 `value(Buy, 0, 500_000)`、`value(Sell, 0, 500_000)`、`coverage(0)`
- 期望:
  - 步骤 1:`coverage(0) = {samples: 0, expected: 10, gaps: 10, clamped: 0}`(采样点 1 尚未提交)
  - 步骤 2:`Some(300_000)`、`Some(−300_000)`;`coverage(0) = {samples: 1, expected: 10, gaps: 9, clamped: 0}`
- 实现:`crates/hft-strat-lib/tests/pct.rs::slb_094_last_in_interval`

### SLB-095 相邻观测间隔不超过 `fill_max_ms` 时补齐空采样点,否则记缺口

- 规则:[PercentileStats](README.md#percentilestats)
- 级别:table
- 里程碑:M4
- 前置:每行 `PercentileStats::new(&C1')`(`C1` 只改所列 `fill_max_ms`);`observe(1_500_000_000, 100_000, 100_000)`,再在 `t_B` `observe(t_B, 0, 0)`;读 `coverage(0).samples`
- 实现:`crates/hft-strat-lib/tests/pct.rs::slb_095_fill_vs_gap`

| 行 | `fill_max_ms` / `t_B` | 期望 `samples` |
|---|---|---|
| SLB-095.1 | 3_000 / 4_499_999_999 | 3(点 1 提交,点 2、3 补齐) |
| SLB-095.2 | 3_000 / 4_500_000_000(间隔恰为 3_000 ms) | 3 |
| SLB-095.3 | 3_000 / 4_500_000_001 | 1(点 2、3 记 MISSING) |
| SLB-095.4 | 0 / 2_500_000_000(相邻区间,无空点) | 1 |
| SLB-095.5 | 0 / 3_500_000_000 | 1(点 2 记 MISSING) |

### SLB-096 非正时刻与早于末次观测的观测被忽略,同一时刻替换

- 规则:[PercentileStats](README.md#percentilestats)
- 级别:unit
- 里程碑:M4
- 前置:`PercentileStats::new(&C1)`;另一个 `new(&C1)` 的快照字节 S0
- 步骤:
  1. `observe(0, 100_000, 0)`、`observe(−5, 100_000, 0)`;`snapshot`
  2. `observe(2_500_000_000, 100_000, 0)`、`observe(2_400_000_000, 300_000, 0)`、`observe(3_100_000_000, 0, 0)`;读 `value(Buy, 0, 1_000_000)`
  3. `observe(5_500_000_000, 100_000, 0)`、`observe(5_500_000_000, 400_000, 0)`、`observe(6_100_000_000, 0, 0)`;读 `value(Buy, 0, 1_000_000)` 与 `coverage(0).samples`
- 期望:
  - 步骤 1:快照与 S0 逐字节相同
  - 步骤 2:`Some(100_000)`(点 2 的值是 2.5 s 的观测)
  - 步骤 3:`Some(400_000)`;`samples = 4`(点 2 = 100_000、点 3 = 0、点 4 补齐为 0、点 5 = 400_000)
- 实现:`crates/hft-strat-lib/tests/pct.rs::slb_096_ignore_and_replace`

### SLB-097 各窗口只含最近的 W 个采样点

- 规则:[PercentileStats](README.md#percentilestats)
- 级别:unit
- 里程碑:M4
- 前置:`PercentileStats::new(&C1)`;在 `t = k × 10^9 + 500_000_000`(k = 1..71)各 `observe(t, b_k, 0)`,`b_k = 100_000`(k ≤ 60)、`b_k = −100_000`(k ≥ 61)
- 步骤:
  1. 读 `coverage(0)`、`coverage(1)`
  2. 读 `value(Buy, 0, 1_000_000)`、`value(Buy, 1, 166_666)`、`value(Buy, 1, 166_667)`
- 期望:
  - 步骤 1:`{samples: 10, expected: 10, gaps: 0, clamped: 0}` 与 `{samples: 60, expected: 60, gaps: 0, clamped: 0}`(已提交点 1..70,窗口 0 为 61..70、窗口 1 为 11..70)
  - 步骤 2:`Some(−100_000)`;`Some(−100_000)`(r = 10);`Some(100_000)`(r = 11)
- 实现:`crates/hft-strat-lib/tests/pct.rs::slb_097_window_slide`

### SLB-098 `value` 的目标秩、分位夹取与 `None`

- 规则:[只给 Model 用的构件](README.md#只给-model-用的构件)、[PercentileStats](README.md#percentilestats)
- 级别:table
- 里程碑:M4
- 前置:`PercentileStats::new(&C1)`;在 `t = k × 10^9 + 500_000_000`(k = 1..11)各 `observe(t, (k − 6) × 100_000, −(k − 6) × 100_000)`——窗口 0 的 Buy 侧为 −500_000 … 400_000 各一个,Sell 侧为 500_000 … −400_000
- 实现:`crates/hft-strat-lib/tests/pct.rs::slb_098_value_queries`

| 行 | 调用 | 期望 |
|---|---|---|
| SLB-098.1 | `value(Buy, 0, 0)` | `Some(−500_000)`(r = 1) |
| SLB-098.2 | `value(Buy, 0, 100_000)` | `Some(−500_000)`(r = 1) |
| SLB-098.3 | `value(Buy, 0, 100_001)` | `Some(−400_000)`(r = 2) |
| SLB-098.4 | `value(Buy, 0, 500_000)` | `Some(−100_000)` |
| SLB-098.5 | `value(Buy, 0, 550_000)` | `Some(0)` |
| SLB-098.6 | `value(Buy, 0, 1_000_000)` | `Some(400_000)` |
| SLB-098.7 | `value(Buy, 0, 1_500_000)` | `Some(400_000)`(按 10^6) |
| SLB-098.8 | `value(Sell, 0, 1_000_000)` | `Some(500_000)` |
| SLB-098.9 | `value(Buy, 2, 500_000)` | `None`(`window_ix ≥ n_windows`) |
| SLB-098.10 | 新建实例上 `value(Buy, 0, 500_000)` | `None`(无样本) |

### SLB-099 `rank` 按箱中心计数、ppm、HalfEven

- 规则:[只给 Model 用的构件](README.md#只给-model-用的构件)、[PercentileStats](README.md#percentilestats)
- 级别:table
- 里程碑:M4
- 前置:行 .1–.7 用 SLB-098 的数据;行 .8–.9 用 `C2` = `C1` 但 `windows_ms = [128_000, 0, 0, 0]`、`n_windows = 1`,提交 128 个点:点 1 的 Buy = −1_000_000,点 2、3 的 Buy = −900_000,其余 0
- 实现:`crates/hft-strat-lib/tests/pct.rs::slb_099_rank_queries`

| 行 | 调用 | 期望 |
|---|---|---|
| SLB-099.1 | `rank(Buy, 0, −500_000)` | `Some(100_000)` |
| SLB-099.2 | `rank(Buy, 0, −500_001)` | `Some(0)` |
| SLB-099.3 | `rank(Buy, 0, 400_000)` | `Some(1_000_000)` |
| SLB-099.4 | `rank(Buy, 0, −50_000)` | `Some(500_000)`(箱中心 ≤ −50_000 的有 5 个) |
| SLB-099.5 | `rank(Buy, 0, i64::MAX)` | `Some(1_000_000)` |
| SLB-099.6 | `1_000_000 − rank(Buy, 0, −1)` | 500_000(≥ 0 的有 5 个) |
| SLB-099.7 | `rank(Buy, 1, 0)`(`C1` 有 2 个窗口,窗口 1 内同样 10 个样本) | `Some(600_000)`(箱中心 ≤ 0 的有 6 个) |
| SLB-099.8 | `C2`:`rank(Buy, 0, −1_000_000)` | `Some(7_812)`(7 812.5 → 偶数) |
| SLB-099.9 | `C2`:`rank(Buy, 0, −900_000)` | `Some(23_438)`(23 437.5 → 偶数) |

### SLB-100 `coverage` 的四项由环算出

- 规则:[只给 Model 用的构件](README.md#只给-model-用的构件)、[PercentileStats](README.md#percentilestats)
- 级别:unit
- 里程碑:M4
- 前置:`PercentileStats::new(&C1)`;依次 `observe(1_500_000_000, 1_000_000, 0)`、`observe(2_500_000_000, 0, −1_000_000)`、`observe(3_500_000_000, 0, 0)`、`observe(4_500_000_000, 0, 0)`
- 步骤:
  1. 读 `coverage(0)`、`coverage(1)`、`coverage(2)`
- 期望:
  - `{samples: 3, expected: 10, gaps: 7, clamped: 2}`、`{samples: 3, expected: 60, gaps: 57, clamped: 2}`、`{0, 0, 0, 0}`
- 实现:`crates/hft-strat-lib/tests/pct.rs::slb_100_coverage`

### SLB-101 `sides = 1` 只建 Buy 侧

- 规则:[只给 Model 用的构件](README.md#只给-model-用的构件)
- 级别:unit
- 里程碑:M4
- 前置:`C1s` = `C1` 但 `sides = 1`
- 步骤:
  1. `new(&C1s)`;`observe(1_500_000_000, 100_000, 300_000)`、`observe(2_500_000_000, 0, 0)`
  2. 读 `value(Buy, 0, 500_000)`、`value(Sell, 0, 500_000)`、`rank(Sell, 0, 0)`;`snapshot`
  3. 比较 `mem_bytes(&C1s)` 与 `mem_bytes(&C1)`
- 期望:
  - 步骤 2:`Some(100_000)`、`None`、`None`;快照 210 B(66 + 24 + 60 × 1 × 2)
  - 步骤 3:`mem_bytes(&C1s) < mem_bytes(&C1)`
- 实现:`crates/hft-strat-lib/tests/pct.rs::slb_101_one_side`

### SLB-102 与"保留全部样本、排序求分位"的参考相比误差不超过半个箱宽

- 规则:[PercentileStats](README.md#percentilestats)
- 级别:prop
- 里程碑:M4
- 性质:任意观测序列之后,对每个窗口、每侧、q ∈ {0, 1, 1_000, 250_000, 500_000, 990_000, 1_000_000}:`value` 与参考值之差 ≤ `bin / 2`(参考值所在的样本未被夹取时);`coverage` 与参考逐项相等;`rank(x)` 与参考在箱中心网格上的 x 相等
- 生成:`C1` 与随机合法配置(1–4 个窗口、`sample_ms` 100–5_000、箱数 3–5_000、`fill_max_ms` 0–10_000、`sides` 1–2);观测 1–3_000 次,时刻步长 0–20 s,值在 [lo − 3 × bin, hi + 3 × bin]
- 参考:保留每个采样点原始值(按 README 第 1 步采样与补齐)、排序求秩的实现
- 例数:PR 1 000;每夜 1 000 000
- 实现:`crates/hft-strat-lib/tests/pct_ref.rs::slb_102_matches_sorted_reference`

### SLB-103 快照大小与字段顺序

- 规则:[PercentileStats](README.md#percentilestats)、[延迟与资源契约](README.md#延迟与资源契约)
- 级别:unit
- 里程碑:M4
- 前置:SLB-094 步骤 2 之后的实例
- 步骤:
  1. `snapshot`
- 期望:
  - 写出 330 B(66 + 24 + 60 × 2 × 2):偏移 0 `sample_ms` = 1_000(u32)、4 起 4 个 u64 窗口 10_000、60_000、0、0、36 `n_windows` = 2、37 `lo`、45 `hi`、53 `bin`、61 `fill_max_ms` = 3_000、65 `sides` = 2;66 起末次观测 `(2_100_000_000, 0, 0)`;90 起 60 个采样点 × 2 侧的 u16 箱号,其中恰有一个点为 (Buy 13, Sell 7),其余 59 个点两侧都是 `u16::MAX`
- 实现:`crates/hft-strat-lib/tests/pct.rs::slb_103_snapshot_layout`

### SLB-104 箱数 65 534 用 u16 编码,65 535 起用 i32

- 规则:[PercentileStats](README.md#percentilestats)、[延迟与资源契约](README.md#延迟与资源契约)
- 级别:table
- 里程碑:M4
- 前置:配置:`sample_ms = 1_000`、`windows_ms = [60_000, 0, 0, 0]`、`n_windows = 1`、`lo = 0`、`bin = 1`、`fill_max_ms = 0`、`sides = 2`;每行只改 `hi`;观测 2 次后 `snapshot`
- 实现:`crates/hft-strat-lib/tests/pct.rs::slb_104_encoding_boundary`

| 行 | `hi`(`n_bins`) | 期望快照大小 |
|---|---|---|
| SLB-104.1 | 65_532(65 533) | 330 B |
| SLB-104.2 | 65_533(65 534) | 330 B |
| SLB-104.3 | 65_534(65 535) | 570 B(60 × 2 × 4) |

### SLB-105 任意位置快照恢复后,查询与后续状态与连续运行逐位相同

- 规则:[PercentileStats](README.md#percentilestats)、[输出与 Advice 载荷](README.md#输出与-advice-载荷)
- 级别:prop
- 里程碑:M4
- 性质:观测脚本在任意位置(含采样区间中途)`snapshot` → 新实例 `restore` → 两者继续同一脚本:每步之后 `value`、`rank`、`coverage` 逐位相同,最终两份快照逐字节相同(含跨区间补齐与缺口);u16 与 i32 两种编码都覆盖
- 生成:同 SLB-102 的配置生成(另含 `n_bins` ∈ {65_533, 65_535} 的配置);脚本 1–2_000 次观测;快照位置均匀
- 参考:不快照的连续运行
- 例数:PR 1 000;每夜 1 000 000
- 实现:`crates/hft-strat-lib/tests/pct_ref.rs::slb_105_snapshot_continue`

### SLB-106 `restore` 以快照中的配置为准

- 规则:[只给 Model 用的构件](README.md#只给-model-用的构件)、[输出与 Advice 载荷](README.md#输出与-advice-载荷)
- 级别:unit
- 里程碑:M4
- 前置:X = SLB-098 的实例(`C1`);Y = `new(&C1b)`,`C1b` = `C1` 但 `bin = 50_000`
- 步骤:
  1. X `snapshot`;Y `restore`
  2. 读 `Y.config()`、`Y.config().layout_eq(&C1b)`;对 SLB-098 的 10 行在 Y 上查询
- 期望:
  - 步骤 1 返回 `Ok`
  - 步骤 2:`Y.config() == C1`;`false`;10 行结果与 SLB-098 相同
- 实现:`crates/hft-strat-lib/tests/pct.rs::slb_106_restore_takes_snapshot_cfg`

### SLB-107 `layout_eq` 只比存储解释字段与前 n_windows 个窗口

- 规则:[只给 Model 用的构件](README.md#只给-model-用的构件)、[输出与 Advice 载荷](README.md#输出与-advice-载荷)
- 级别:table
- 里程碑:M4
- 前置:基准 `C1`;每行只改所列字段后调用 `C1.layout_eq(&改后)`
- 实现:`crates/hft-strat-lib/tests/pct.rs::slb_107_layout_eq`

| 行 | 改动 | 期望 |
|---|---|---|
| SLB-107.1 | 不改 | `true` |
| SLB-107.2 | `fill_max_ms = 5_000` | `true` |
| SLB-107.3 | `windows_ms[2] = 5`(未用项) | `true` |
| SLB-107.4 | `sample_ms = 2_000` | `false` |
| SLB-107.5 | `windows_ms[0] = 20_000` | `false` |
| SLB-107.6 | `n_windows = 1` | `false` |
| SLB-107.7 | `lo = −2_000_000` | `false` |
| SLB-107.8 | `hi = 2_000_000` | `false` |
| SLB-107.9 | `bin = 50_000` | `false` |
| SLB-107.10 | `sides = 1` | `false` |

### SLB-108 `set_fill_max_ms` 原地更新,只影响此后的补齐

- 规则:[只给 Model 用的构件](README.md#只给-model-用的构件)、[输出与 Advice 载荷](README.md#输出与-advice-载荷)
- 级别:unit
- 里程碑:M4
- 前置:X、Y 都是 `new(&C1)`,各 `observe(1_500_000_000, 100_000, 0)`
- 步骤:
  1. X `snapshot` 得 S1;X `set_fill_max_ms(0)`;X `snapshot` 得 S2
  2. X、Y 各 `observe(3_500_000_000, 0, 0)`;读两者 `coverage(0).samples`
- 期望:
  - 步骤 1:S1 与 S2 只有偏移 61..65(`fill_max_ms`)不同,S2 中为 0;`X.config()` 其余字段不变
  - 步骤 2:X 为 1(点 2 记 MISSING),Y 为 2(点 2 补齐)
- 实现:`crates/hft-strat-lib/tests/pct.rs::slb_108_set_fill_max`

### SLB-109 `prefill` 只填仍为 MISSING 的点

- 规则:[只给 Model 用的构件](README.md#只给-model-用的构件)、[PercentileStats](README.md#percentilestats)
- 级别:unit
- 里程碑:M4
- 前置:`new(&C1)`;在 95.5 s、96.5 s … 100.5 s 各 `observe(t, 0, 0)`(提交点 95..99)
- 步骤:
  1. `prefill` 样本 `(k × 10^9, 100_000, 100_000)`(k = 90..99)加一个 `(0, 100_000, 100_000)`,`fill_ms = 0`,`now_ns = 100_500_000_000`
  2. 读 `coverage(0)`、`value(Buy, 0, 1_000_000)`、`value(Buy, 0, 0)`
  3. 以相同参数再 `prefill` 一次
- 期望:
  - 步骤 1 返回 `Ok(5)`(点 90..94;ts = 0 的样本跳过)
  - 步骤 2:`{samples: 9, expected: 10, gaps: 1, clamped: 0}`(窗口 0 为点 91..100,点 100 尚无值);`Some(100_000)`;`Some(0)`(实时点保持 0)
  - 步骤 3 返回 `Ok(0)`,快照与步骤 2 时逐字节相同
- 实现:`crates/hft-strat-lib/tests/pct.rs::slb_109_prefill_only_missing`

### SLB-110 `prefill` 每个样本覆盖的点数与窗口范围

- 规则:[只给 Model 用的构件](README.md#只给-model-用的构件)、[PercentileStats](README.md#percentilestats)
- 级别:table
- 里程碑:M4
- 前置:每行 `new(&C1)`,`now_ns = 100_000_000_000`(E = 100,窗口为点 41..100);一个样本 `(ts, 100_000, 100_000)`
- 实现:`crates/hft-strat-lib/tests/pct.rs::slb_110_prefill_span`

| 行 | ts / `fill_ms` | 期望返回 |
|---|---|---|
| SLB-110.1 | 50 s / 0 | `Ok(1)` |
| SLB-110.2 | 50 s / 1_000 | `Ok(1)` |
| SLB-110.3 | 50 s / 2_500 | `Ok(2)` |
| SLB-110.4 | 50 s / 3_000 | `Ok(3)`;之后 `coverage(1) = {samples: 3, expected: 60, gaps: 57, clamped: 0}` |
| SLB-110.5 | 50.5 s / 3_000 | `Ok(3)`(点 50、51、52) |
| SLB-110.6 | 40 s / 0 | `Ok(0)`(点 40 在窗口外) |
| SLB-110.7 | 41 s / 0 | `Ok(1)` |
| SLB-110.8 | 99 s / 3_000 | `Ok(2)`(点 99、100;点 101 晚于 E) |
| SLB-110.9 | 0 / 0 | `Ok(0)` |
| SLB-110.10 | −1 s / 0 | `Ok(0)` |

### SLB-111 实时提交覆盖预热值,缺口不覆盖预热值

- 规则:[PercentileStats](README.md#percentilestats)
- 级别:unit
- 里程碑:M4
- 前置:`new(&C1)`
- 步骤:
  1. `prefill([(100 × 10^9, 100_000, 100_000)], 0, 100_500_000_000)`
  2. `observe(100_200_000_000, −100_000, −100_000)`、`observe(101_200_000_000, 0, 0)`;读 `coverage(0).samples`、`value(Buy, 0, 1_000_000)`
  3. `prefill([(103 × 10^9, 200_000, 200_000), (104 × 10^9, 200_000, 200_000)], 0, 105_000_000_000)`
  4. `observe(105_500_000_000, 0, 0)`;读 `coverage(0)`、`value(Buy, 0, 1_000_000)`
- 期望:
  - 步骤 1 返回 `Ok(1)`
  - 步骤 2:`samples = 1`;`Some(−100_000)`(点 100 的预热值被实时值替换)
  - 步骤 3 返回 `Ok(2)`(E = 105)
  - 步骤 4:`{samples: 4, expected: 10, gaps: 6, clamped: 0}`(点 100 = −100_000、101 = 0、102 MISSING、103 与 104 保留预热值 200_000);`Some(200_000)`
- 实现:`crates/hft-strat-lib/tests/pct.rs::slb_111_realtime_vs_prefill`

### SLB-112 预热与实时样本任意顺序、切块、重复,最终状态相同

- 规则:[只给 Model 用的构件](README.md#只给-model-用的构件)、[PercentileStats](README.md#percentilestats)
- 级别:prop
- 里程碑:M4
- 性质:同一组预热样本与实时观测,以任意交错顺序、任意切块、任意重复调用 `prefill` 之后,环、各窗口直方图(经全部 `value`/`rank` 查询比较)、`coverage` 与快照字节都与参考相同
- 生成:`C1` 与 SLB-102 的随机配置;预热样本 0–500 个(`fill_ms` 0–60_000);实时观测 0–500 次;切块 1–100;重复 1–3 次;每次 `prefill` 的 `now_ns` 取当时最新观测时刻或更晚
- 参考:先全部 `observe`、再一次性 `prefill` 全部样本(`now_ns` 取全序列最大值)的结果
- 例数:PR 1 000;每夜 1 000 000
- 实现:`crates/hft-strat-lib/tests/pct_ref.rs::slb_112_prefill_order_free`

### SLB-113 `PercentileStats::restore` 的错误分支不改状态

- 规则:[PercentileStats](README.md#percentilestats)、[输出与 Advice 载荷](README.md#输出与-advice-载荷)
- 级别:table
- 里程碑:M4
- 前置:源快照 = SLB-103 的 330 B;目标 = SLB-098 的实例,记其快照为 S_t;每行改造源快照后 `restore` 进目标
- 实现:`crates/hft-strat-lib/tests/pct.rs::slb_113_restore_errors`

| 行 | 改造 | 期望 |
|---|---|---|
| SLB-113.1 | 截去最后 1 B | `Err(RestoreError)`;目标快照 = S_t |
| SLB-113.2 | `bin` 字段改为 0 | `Err(RestoreError)`;目标快照 = S_t |
| SLB-113.3 | 点 1 的 Buy 箱号改为 21(= `n_bins`) | `Err(RestoreError)`;目标快照 = S_t |
| SLB-113.4 | 点 1 的 Buy 箱号改为 20 | `Ok`;`value(Buy, 0, 500_000) = Some(1_000_000)` |

### SLB-114 内存与快照上界及 `LAYOUT_ELEMS`

- 规则:[只给 Model 用的构件](README.md#只给-model-用的构件)、[输出与 Advice 载荷](README.md#输出与-advice-载荷)、[延迟与资源契约](README.md#延迟与资源契约)
- 级别:prop
- 里程碑:M4
- 性质:任意合法 `PctConfig`:`new` 与满窗口观测、预热之后的实际分配(计数分配器)≤ `mem_bytes(cfg)`;快照字节数 ≤ `snapshot_bytes(cfg)`;SDK `testing::check_layout` 的记录型 writer 记下的元素序列与 `LAYOUT_ELEMS` 一致
- 生成:1–4 个窗口、`sample_ms` 100–10_000、L ≤ 200_000、箱数 3–70_000、`sides` 1–2
- 参考:上述不等式与元素序列比较
- 例数:PR 200;每夜 20 000
- 实现:`crates/hft-strat-lib/tests/pct_ref.rs::slb_114_bounds_and_layout`

### SLB-115 分位越界合成序列金样

- 规则:[PercentileStats](README.md#percentilestats)
- 级别:golden
- 里程碑:M4
- 输入:`fixtures/stratlib/pct/clamp.in.jsonl`(`C1`;含越界、恰在 lo/hi、缺口与补齐的观测,以及两次 `prefill`)
- 期望文件:`fixtures/stratlib/pct/clamp.expected.jsonl`(每步之后两个窗口、两侧在 q = 0、250_000、500_000、750_000、1_000_000 的 `value` 与 `coverage`)
- 实现:`crates/hft-strat-lib/tests/golden.rs::slb_115_pct_clamp_golden`

### SLB-116 领先-滞后的峰值、显著度与可用窗口(手算样例)

- 规则:[只给 Model 用的构件](README.md#只给-model-用的构件)、[LeadLag](README.md#leadlag)
- 级别:unit
- 里程碑:M4
- 前置:`LeadLag::new(&LL1)`;按 `LLX` 依时间顺序 `observe`(两腿交错)
- 步骤:
  1. `estimate(TL + 20_000_000)`
- 期望:
  - `Some(LeadLagEst{lead_leg: 0, n_samples: 20, lag_ns: 200_000, window_ns: 50_000, corr_peak_ppm: 1_000_000, confidence_ppm: 6_902_366, computed_at_ns: TL + 20_000_000})`(acc[4] = 20、acc[−16] = −19,其余 0;μ = −0.02375、σ = √0.0219984375)
- 实现:`crates/hft-strat-lib/tests/leadlag.rs::slb_116_worked_example`

### SLB-117 领先方向与峰值位置

- 规则:[LeadLag](README.md#leadlag)
- 级别:table
- 里程碑:M4
- 前置:`LL1`;每行一个合成序列(初始观测同 `LLX`),`estimate(T_end + 20_000_000)`,`T_end` = 最后一个变动的时刻
- 实现:`crates/hft-strat-lib/tests/leadlag.rs::slb_117_lead_direction`

| 行 | 序列 | 期望 |
|---|---|---|
| SLB-117.1 | `LLX` 两腿互换(b 先变,a 晚 200_000 ns 跟随) | `lead_leg = 1`、`lag_ns = −200_000`、`corr_peak_ppm = 1_000_000`、`confidence_ppm = 6_902_366`、`window_ns = 50_000` |
| SLB-117.2 | b 的变动与 a 同一时刻、同号 | `lead_leg = 0xFF`、`lag_ns = 0`、`corr_peak_ppm = 1_000_000` |
| SLB-117.3 | b 的 20 个变动在 `TL + 1_000_000_000 + i × 1_000_000`(与 a 相距 > `max_lag_ns`) | `lead_leg = 0xFF`、`lag_ns = 0`、`corr_peak_ppm = 0`、`confidence_ppm = 0`、`window_ns = 2_050_000`、`n_samples = 20` |

### SLB-118 被忽略的观测不改上一个 mid

- 规则:[LeadLag](README.md#leadlag)
- 级别:table
- 里程碑:M4
- 前置:`LL1` 但 `min_samples = 1`;b 先在 `TL − 1_000_000` 观测一次,再在 `TL` 起每 1 ms 一个变动共 30 个(n_b = 30);a 按所列序列在 `TL + 1_000_000` 起每步 +1 ms `observe`;`estimate(TL + 40_000_000)` 的 `n_samples` 即 n_a
- 实现:`crates/hft-strat-lib/tests/leadlag.rs::slb_118_ignored_observations`

| 行 | a 的 `observe` 序列(mid;或说明) | 期望 |
|---|---|---|
| SLB-118.1 | 100、101 | `n_samples = 1` |
| SLB-118.2 | 100、100 | `None`(相等不记,n_a = 0) |
| SLB-118.3 | 100、0、100 | `None`(0 被忽略,100 与上一个有效 mid 相等) |
| SLB-118.4 | 100、−5、101 | `n_samples = 1` |
| SLB-118.5 | 100、101、(时刻早 1 ns 的)100、102 | `n_samples = 2`(早于最后变动的观测被忽略,102 相对 101 为 +) |
| SLB-118.6 | 以 `leg = 2` 观测 5 次 | `None`(n_a = 0) |

### SLB-119 `min_samples` 的边界

- 规则:[只给 Model 用的构件](README.md#只给-model-用的构件)、[LeadLag](README.md#leadlag)
- 级别:table
- 里程碑:M4
- 前置:`LL1`(`min_samples = 10`,行 .3 改为 0);`LLX` 只取前 n 个变动(两腿各 n 个)
- 实现:`crates/hft-strat-lib/tests/leadlag.rs::slb_119_min_samples`

| 行 | n / `min_samples` | 期望 `estimate(TL + 20_000_000)` |
|---|---|---|
| SLB-119.1 | 9 / 10 | `None` |
| SLB-119.2 | 10 / 10 | `Some`,`n_samples = 10` |
| SLB-119.3 | 0 / 0 | `None`(n = 0) |

### SLB-120 窗口滑出的变动不再计入

- 规则:[LeadLag](README.md#leadlag)
- 级别:unit
- 里程碑:M4
- 前置:`LeadLag::new(&LL1)`;`LLX` 全部观测
- 步骤:
  1. `estimate(TL + 60_010_000_000)`
  2. `estimate(TL + 60_010_000_001)`
- 期望:
  - 步骤 1:`Some`,`n_samples = 10`(a、b 的第 0..9 个变动早于窗口起点 `TL + 10_000_000` 被删去,第 10 个恰在起点保留)、`lag_ns = 200_000`、`lead_leg = 0`、`corr_peak_ppm = 1_000_000`
  - 步骤 2:`None`(a 的第 10 个变动也被删去,n_a = 9 < 10)
- 实现:`crates/hft-strat-lib/tests/leadlag.rs::slb_120_window_slide`

### SLB-121 超过 `max_changes` 时丢最旧的变动并计数

- 规则:[只给 Model 用的构件](README.md#只给-model-用的构件)、[LeadLag](README.md#leadlag)
- 级别:unit
- 里程碑:M4
- 前置:`LL1` 但 `max_changes = 5`、`min_samples = 1`;`LLX` 全部观测
- 步骤:
  1. 读 `dropped()`;`estimate(TL + 20_000_000)`
- 期望:
  - `dropped() = 30`(每腿 15);`Some`,`n_samples = 5`、`lag_ns = 200_000`、`corr_peak_ppm = 1_000_000`(acc[4] = 5、acc[−16] = −4,与只用 a、b 各最后 5 个变动的全量重算相同)
- 实现:`crates/hft-strat-lib/tests/leadlag.rs::slb_121_max_changes`

### SLB-122 增量累加器与窗口内全量重算逐值相等

- 规则:[LeadLag](README.md#leadlag)
- 级别:prop
- 里程碑:M4
- 性质:任意两腿观测序列与 `estimate` 时刻序列下,每次 `estimate` 的输出与"按当时窗口内保留的变动全量重算 acc 再按 README 归一化"的参考逐字段相等
- 生成:`bucket_ns` 1_000–100_000、`max_lag_ns` 0–2_000_000、`window_ms` 10–5_000、`max_changes` 1–500;每腿 0–1_000 次观测,时刻步长 0–3 ms(含倒退),mid 随机游走(含 0 与负值);`estimate` 每 1–50 次观测一次
- 参考:全量重算实现(f64 运算顺序与 README 相同)
- 例数:PR 1 000;每夜 1 000 000
- 实现:`crates/hft-strat-lib/tests/leadlag.rs::slb_122_incremental_matches_full`

### SLB-123 快照恢复后累加器与后续输出与连续运行逐值相等

- 规则:[LeadLag](README.md#leadlag)、[输出与 Advice 载荷](README.md#输出与-advice-载荷)
- 级别:prop
- 里程碑:M4
- 性质:任意位置 `snapshot` → 新实例 `restore` 后,`estimate` 与连续运行逐字段相等;继续 `observe` 同一序列(恢复后的第一个观测的 mid 与快照前最后一个 mid 相同、不同各一半)之后仍逐字段相等;`dropped()` 在恢复后从 0 计
- 生成:同 SLB-122
- 参考:不快照的连续运行
- 例数:PR 1 000;每夜 1 000 000
- 实现:`crates/hft-strat-lib/tests/leadlag.rs::slb_123_snapshot_continue`

### SLB-124 注入固定滞后的合成序列,估计的滞后落在 d ± 1 个 bucket

- 规则:[LeadLag](README.md#leadlag)
- 级别:prop
- 里程碑:M4
- 性质:a 的 mid 为随机游走,b 在每个 a 变动之后 d(± 抖动 ≤ bucket_ns / 4)ns 以同号变动,另加 10% 独立噪声变动:`estimate` 返回 `Some` 且 `|lag_ns − d| ≤ bucket_ns`;d > bucket_ns 时 `lead_leg = 0`
- 生成:`LL1`;d ∈ [−900_000, 900_000];每腿 50–2_000 个变动,相邻 a 变动间隔 ≥ 2 × `max_lag_ns`
- 参考:上述断言
- 例数:PR 500;每夜 100_000
- 实现:`crates/hft-strat-lib/tests/leadlag.rs::slb_124_recovers_lag`

### SLB-125 `LeadLag::restore` 截断时返回错误且不改状态,`dropped` 不进快照

- 规则:[LeadLag](README.md#leadlag)、[输出与 Advice 载荷](README.md#输出与-advice-载荷)
- 级别:unit
- 里程碑:M4
- 前置:X = SLB-121 步骤之后的实例(`dropped() = 30`);Y = `LeadLag::new(&LL1)` 只观测过 `LLX` 的前 3 个变动
- 步骤:
  1. X `snapshot`;新实例 Z(同配置)`restore`;读 `Z.dropped()`,`Z.estimate(TL + 20_000_000)`
  2. 用 X 快照去掉最后 1 B 的字节 `restore` 进 Y
- 期望:
  - 步骤 1:`Ok`;`0`;与 X 的 `estimate` 结果逐字段相同
  - 步骤 2:`Err(RestoreError)`;Y 的快照与调用前逐字节相同
- 实现:`crates/hft-strat-lib/tests/leadlag.rs::slb_125_restore_truncated_and_dropped`

### SLB-126 `LeadLag` 的内存与快照上界及 `LAYOUT_ELEMS`

- 规则:[只给 Model 用的构件](README.md#只给-model-用的构件)、[延迟与资源契约](README.md#延迟与资源契约)
- 级别:prop
- 里程碑:M4
- 性质:任意合法 `LeadLagCfg`:每腿观测到 `max_changes` 个变动之后实际分配 ≤ `mem_bytes(cfg)`、快照字节 ≤ `snapshot_bytes(cfg)`;`check_layout` 记下的元素序列与 `LAYOUT_ELEMS` 一致
- 生成:`bucket_ns` 1_000–100_000、`max_lag_ns` 0–10_000_000、`max_changes` 1–100_000
- 参考:上述不等式与元素序列比较
- 例数:PR 200;每夜 20 000
- 实现:`crates/hft-strat-lib/tests/leadlag.rs::slb_126_bounds_and_layout`

### SLB-127 `bucket_ns = 0` 按 1 处理、不 panic

- 规则:[LeadLag](README.md#leadlag)
- 级别:unit
- 里程碑:M4
- 前置:`LL1` 但 `bucket_ns = 0`、`max_lag_ns = 3`
- 步骤:
  1. `new`;两腿各 `observe` 初始 mid 与 10 个交替变动(a 的第 i 个在 `t_i = TL + i × 1_000_000`,b 的第 i 个在 `t_i + 2`);`estimate(TL + 20_000_000)`
- 期望:
  - 不 panic;返回 `Some`,`lag_ns = 2`、`lead_leg = 0`(按 `bucket_ns = 1`,L = 7)
- 实现:`crates/hft-strat-lib/tests/leadlag.rs::slb_127_zero_bucket`

### SLB-128 `CostItem` 与 `CostLine` 的布局

- 规则:[快速部分可用的构件](README.md#快速部分可用的构件)、[延迟与资源契约](README.md#延迟与资源契约)
- 级别:unit
- 里程碑:M2
- 前置:无
- 步骤:
  1. 取 `size_of`、`align_of` 与公开字段的 `offset_of`
- 期望:
  - `CostItem`:16 B、对齐 8;`rate` 0、`label` 8、`flags` 10、`_pad` 11
  - `CostLine`:168 B、对齐 8;`items` 0、`n` 128、`_pad` 129、`rt_floor` 136
- 实现:`crates/hft-strat-lib/tests/layout.rs::slb_128_cost_layout`

### SLB-129 `SettleRec`、`FundingSummary`、`LeadLagEst` 的布局

- 规则:[输出与 Advice 载荷](README.md#输出与-advice-载荷)、[只给 Model 用的构件](README.md#只给-model-用的构件)
- 级别:unit
- 里程碑:M4
- 前置:无
- 步骤:
  1. 取三个结构的 `size_of`、`align_of` 与各字段 `offset_of`
- 期望:
  - `SettleRec`:24 B、对齐 8;`at_ns` 0、`rate` 8、`flags` 16、`_pad` 20
  - `FundingSummary`:80 B、对齐 8;`predicted` 0、`predicted_daily` 8、`realized_sum` 16、`realized_daily` 24、`min_rate` 32、`max_rate` 40、`n_settles` 48、`n_positive` 50、`n_negative` 52、`neg_streak` 54、`interval_s` 56、`flags` 60、`next_settle_ns` 64、`interval_changed_at_ns` 72
  - `LeadLagEst`:40 B、对齐 8;`lead_leg` 0、`_pad` 1、`n_samples` 4、`lag_ns` 8、`window_ns` 16、`corr_peak_ppm` 24、`confidence_ppm` 28、`computed_at_ns` 32
- 实现:`crates/hft-strat-lib/tests/layout.rs::slb_129_output_layouts`

### SLB-130 成本位与动作类别的码值

- 规则:[快速部分可用的构件](README.md#快速部分可用的构件)
- 级别:table
- 里程碑:M2
- 前置:无
- 实现:`crates/hft-strat-lib/tests/layout.rs::slb_130_fast_codes`

| 行 | 名字 | 期望值 |
|---|---|---|
| SLB-130.1 | `cost_flags::ROUND_TRIP` / `FIXED` / `OUT_OF_QUOTE` | 1 / 2 / 4 |
| SLB-130.2 | `FireKind::Open` / `Close` / `Flatten` / `Hard` as u8 | 0 / 1 / 2 / 3 |

### SLB-131 资金费标志、分位侧与领先腿的码值

- 规则:[快速部分可用的构件](README.md#快速部分可用的构件)、[只给 Model 用的构件](README.md#只给-model-用的构件)、[输出与 Advice 载荷](README.md#输出与-advice-载荷)
- 级别:table
- 里程碑:M4
- 前置:无
- 实现:`crates/hft-strat-lib/tests/layout.rs::slb_131_model_codes`

| 行 | 名字 | 期望值 |
|---|---|---|
| SLB-131.1 | `funding_flags::INTERVAL_CHANGED` / `STALE` / `GAP` / `NO_DATA` / `TRUNCATED` | 1 / 2 / 4 / 8 / 16 |
| SLB-131.2 | SLB-069.2 返回的 `SettleRec.flags` | 4(与 `FundingSummary` 的 `GAP` 同一位) |
| SLB-131.3 | `PctSide::Buy` / `Sell` as u8 | 0 / 1 |
| SLB-131.4 | `LeadLagEst.lead_leg`:a 领先 / b 领先 / 无显著领先 | 0 / 1 / 0xFF(取自 SLB-116、SLB-117.1、SLB-117.3) |

### SLB-132 快速部分构件零分配、结构不超过 128 B

- 规则:[快速部分可用的构件](README.md#快速部分可用的构件)、[构件总表](README.md#构件总表)、[延迟与资源契约](README.md#延迟与资源契约)
- 级别:unit
- 里程碑:M2
- 前置:测试二进制装计数分配器
- 步骤:
  1. 对 `SpreadCalc` 三个函数、`CostLine` 三个读函数、`TriggerPx::update`/`hit_*`、`ConfirmWindow::observe`、`DirtyFilter::check`、`Throttle` 全部方法、`ImpactModel::vwap`(20 档)、`Expiring::get`/`put`/`put_advice` 各调用 10_000 次,读分配计数
  2. 取 `size_of`
- 期望:
  - 步骤 1:分配次数 0
  - 步骤 2:`TriggerPx`、`ConfirmWindow`、`Throttle`、`LegFresh`、`Freshness`、`DirtyParamsTsc`、`ThrottleParamsNs`、`ConfirmParamsNs`、`VwapParams` 各 ≤ 128;`Expiring<i64>` ≤ 136;`CostLine` = 168
- 实现:`crates/hft-strat-lib/tests/alloc.rs::slb_132_zero_alloc_and_sizes`

### SLB-133 依赖方向:本库只依赖 L0 与 SDK,引擎 crate 不依赖本库

- 规则:[概要](README.md#概要)、[依赖](README.md#依赖)
- 级别:table
- 里程碑:M2
- 前置:`tools/xtask/tests/fixtures/` 下的小型假仓库,`deps.toml` 与主仓库相同;每行在干净副本上只做该行的一处改动,运行 `cargo xtask deps`
- 实现:`tools/xtask/tests/deps_stratlib.rs::slb_133_dependency_direction`

| 行 | 改动 | 期望 |
|---|---|---|
| SLB-133.1 | 不改 | 退出 0 |
| SLB-133.2 | `hft-strat-lib` 的 `[dependencies]` 加 `hft-oms` | 退出 1;报告边 `hft-strat-lib → hft-oms` |
| SLB-133.3 | 加 `hft-sequencer` | 退出 1 |
| SLB-133.4 | 加 `hft-journal` | 退出 1 |
| SLB-133.5 | 加 `hft-rt` | 退出 1 |
| SLB-133.6 | 加 `strategies/x` | 退出 1 |
| SLB-133.7 | `[dev-dependencies]` 加 `hft-book` | 退出 1(含 dev 依赖) |
| SLB-133.8 | 加 `tokio` | 退出 1(pure crate 闭包) |
| SLB-133.9 | `crates/hft-sequencer` 依赖 `hft-strat-lib` | 退出 1(引擎 crate 不依赖本库) |
| SLB-133.10 | `strategies/x` 依赖 `hft-strat-lib` 与 `hft-strategy-sdk` | 退出 0 |

### SLB-134 f64 只在 `src/model/`,本库无 I/O、系统时钟、`HashMap` 迭代与 `dyn`

- 规则:[依赖](README.md#依赖)、[规划源文件](README.md#规划源文件)、[输出与 Advice 载荷](README.md#输出与-advice-载荷)、[只给 Model 用的构件](README.md#只给-model-用的构件)
- 级别:table
- 里程碑:M2
- 前置:同 SLB-133 的假仓库;每行在 `crates/hft-strat-lib` 中加一处代码,运行所列检查
- 实现:`tools/xtask/tests/hot_check_stratlib.rs::slb_134_forbidden_constructs`

| 行 | 加入的代码 | 检查 | 期望 |
|---|---|---|---|
| SLB-134.1 | `src/spread.rs` 中 `let x: f64 = 0.0;` | `cargo xtask hot-check` | 退出 1,报 N3 与文件行号 |
| SLB-134.2 | `src/model/leadlag.rs` 中 `let x: f64 = 0.0;` | `cargo xtask hot-check` | 退出 0 |
| SLB-134.3 | `src/throttle.rs` 中 `std::time::Instant::now()` | `cargo xtask hot-check` | 退出 1,报 T4 与文件行号 |
| SLB-134.4 | `src/model/pct.rs` 中对 `std::collections::HashMap` 的 `iter()` | `cargo xtask ci`(clippy) | 失败 |
| SLB-134.5 | `src/impact.rs` 中参数类型 `&dyn BookRead` | `cargo xtask hot-check` | 退出 1,报 N6 |
| SLB-134.6 | `src/funding.rs` 中 `std::fs::read("x")` | `cargo xtask hot-check` | 退出 1,报 `pure` 与文件行号 |

### SLB-135 同一输入在 x86_64 与 aarch64 上输出逐位相同

- 规则:[输出与 Advice 载荷](README.md#输出与-advice-载荷)、[LeadLag](README.md#leadlag)
- 级别:golden
- 里程碑:M4
- 输入:`fixtures/stratlib/determinism/corpus.in.jsonl`(覆盖全部构件的调用脚本,含 `LeadLag::estimate` 与 `PercentileStats` 的预热)
- 期望文件:`fixtures/stratlib/determinism/corpus.expected.jsonl`(每次调用的返回值与最终快照字节的十六进制);CI 在 x86_64 与 aarch64 各跑一次,两边都与该文件逐字节相同
- 实现:`crates/hft-strat-lib/tests/golden.rs::slb_135_cross_arch_determinism`

### SLB-136 基准:`SpreadCalc::buy`

- 规则:[延迟与资源契约](README.md#延迟与资源契约)
- 级别:bench
- 里程碑:M2
- 基准名:`stratlib/spread`
- 实现:`crates/hft-strat-lib/benches/fast.rs::slb_136_spread`

### SLB-137 基准:`CostLine::open_floor`

- 规则:[延迟与资源契约](README.md#延迟与资源契约)
- 级别:bench
- 里程碑:M2
- 基准名:`stratlib/cost_read`
- 实现:`crates/hft-strat-lib/benches/fast.rs::slb_137_cost_read`

### SLB-138 基准:`TriggerPx::update`

- 规则:[延迟与资源契约](README.md#延迟与资源契约)
- 级别:bench
- 里程碑:M2
- 基准名:`stratlib/trigger_update`
- 实现:`crates/hft-strat-lib/benches/fast.rs::slb_138_trigger_update`

### SLB-139 基准:`TriggerPx::hit_*`

- 规则:[延迟与资源契约](README.md#延迟与资源契约)
- 级别:bench
- 里程碑:M2
- 基准名:`stratlib/trigger_hit`
- 实现:`crates/hft-strat-lib/benches/fast.rs::slb_139_trigger_hit`

### SLB-140 基准:`ConfirmWindow::observe`

- 规则:[延迟与资源契约](README.md#延迟与资源契约)
- 级别:bench
- 里程碑:M2
- 基准名:`stratlib/confirm`
- 实现:`crates/hft-strat-lib/benches/fast.rs::slb_140_confirm`

### SLB-141 基准:`DirtyFilter::check`

- 规则:[延迟与资源契约](README.md#延迟与资源契约)
- 级别:bench
- 里程碑:M2
- 基准名:`stratlib/dirty`
- 实现:`crates/hft-strat-lib/benches/fast.rs::slb_141_dirty`

### SLB-142 基准:`Throttle::can_fire`

- 规则:[延迟与资源契约](README.md#延迟与资源契约)
- 级别:bench
- 里程碑:M2
- 基准名:`stratlib/throttle`
- 实现:`crates/hft-strat-lib/benches/fast.rs::slb_142_throttle`

### SLB-143 基准:`ImpactModel::vwap`(20 档)

- 规则:[延迟与资源契约](README.md#延迟与资源契约)
- 级别:bench
- 里程碑:M2
- 基准名:`stratlib/vwap20`
- 实现:`crates/hft-strat-lib/benches/fast.rs::slb_143_vwap20`

### SLB-144 基准:`FundingStats::observe`

- 规则:[延迟与资源契约](README.md#延迟与资源契约)
- 级别:bench
- 里程碑:M4
- 基准名:`stratlib/funding_observe`
- 实现:`crates/hft-strat-lib/benches/funding.rs::slb_144_funding_observe`

### SLB-145 基准:`FundingStats::summary`(256 期)

- 规则:[延迟与资源契约](README.md#延迟与资源契约)
- 级别:bench
- 里程碑:M4
- 基准名:`stratlib/funding_summary`
- 实现:`crates/hft-strat-lib/benches/funding.rs::slb_145_funding_summary`

### SLB-146 基准:`Expiring::get`

- 规则:[延迟与资源契约](README.md#延迟与资源契约)
- 级别:bench
- 里程碑:M2
- 基准名:`stratlib/expiring`
- 实现:`crates/hft-strat-lib/benches/fast.rs::slb_146_expiring`

### SLB-147 基准:`PercentileStats::observe`(4 窗口)

- 规则:[延迟与资源契约](README.md#延迟与资源契约)
- 级别:bench
- 里程碑:M4
- 基准名:`stratlib/pct_observe`
- 实现:`crates/hft-strat-lib/benches/model.rs::slb_147_pct_observe`

### SLB-148 基准:`PercentileStats::prefill`(一块约 500 个采样点)

- 规则:[延迟与资源契约](README.md#延迟与资源契约)
- 级别:bench
- 里程碑:M4
- 基准名:`stratlib/pct_prefill_chunk`
- 实现:`crates/hft-strat-lib/benches/model.rs::slb_148_pct_prefill_chunk`

### SLB-149 基准:`PercentileStats::value`/`rank`(60,001 箱)

- 规则:[延迟与资源契约](README.md#延迟与资源契约)
- 级别:bench
- 里程碑:M4
- 基准名:`stratlib/pct_query`
- 实现:`crates/hft-strat-lib/benches/model.rs::slb_149_pct_query`

### SLB-150 基准:`PercentileStats::value`/`rank`(6,001 箱)

- 规则:[延迟与资源契约](README.md#延迟与资源契约)
- 级别:bench
- 里程碑:M4
- 基准名:`stratlib/pct_query_small`
- 实现:`crates/hft-strat-lib/benches/model.rs::slb_150_pct_query_small`

### SLB-151 基准:`LeadLag::observe`

- 规则:[延迟与资源契约](README.md#延迟与资源契约)
- 级别:bench
- 里程碑:M4
- 基准名:`stratlib/leadlag_observe`
- 实现:`crates/hft-strat-lib/benches/model.rs::slb_151_leadlag_observe`

### SLB-152 基准:`LeadLag::estimate`(L = 201)

- 规则:[延迟与资源契约](README.md#延迟与资源契约)
- 级别:bench
- 里程碑:M4
- 基准名:`stratlib/leadlag_estimate`
- 实现:`crates/hft-strat-lib/benches/model.rs::slb_152_leadlag_estimate`

### SLB-153 按 SDK 的 rate_backoff 布尔读数镜像退避:真时一直退避,假时立即解除

- 规则:[快速部分可用的构件](README.md#快速部分可用的构件)、[Throttle](README.md#throttle)
- 级别:table
- 里程碑:M2
- 前置:`TP0`;每行先按"读数"调用 `set_backoff`(true → `set_backoff(i64::MAX)`,false → `set_backoff(0)`),再调用所列 `can_fire`
- 实现:`crates/hft-strat-lib/tests/throttle.rs::slb_153_backoff_mirror_bool`

| 行 | 读数序列 | 调用 | 期望 |
|---|---|---|---|
| SLB-153.1 | true | `can_fire(Open, 10_000_000_000)` | `Err(Backoff)` |
| SLB-153.2 | true | `can_fire(Close, 9_000_000_000_000_000_000)` | `Err(Backoff)`(截止 `i64::MAX` 不会自行到期) |
| SLB-153.3 | true,然后 false | `can_fire(Open, 10_000_000_000)` | `Ok(())` |
| SLB-153.4 | true | `can_fire(Flatten, 10_000_000_000)` | `Ok(())`(Flatten 不受退避约束) |

## 验收

- 里程碑已到的用例全部有同编号的实现且通过(`cargo xtask test-trace`,规则见 [docs/tests §6](../../docs/tests/README.md#6-与代码的对应));快速部分构件(M2)的用例在 M2 完成,`FundingStats`、`PercentileStats`、`LeadLag` 与输出结构(M4)的用例在 M4 完成。
- 属性测试:每个属性用例在 CI 每夜按"例数"字段中的每夜例数运行,除 SLB-114、SLB-124、SLB-126(分配与合成序列,每夜 20 000–100 000 例)外每个 ≥ 10^6 例(运行矩阵归 [testing §6](../../docs/testing.md#6-ci-运行矩阵))。
- 确定性:SLB-135 在 x86_64 与 aarch64 上都与金样逐字节相同(CI 双架构比对)。
- 金样:本库自带合成序列金样(SLB-090 资金费周期变化、缺口与回退,SLB-115 分位越界);使用方策略的真实行情金样(真实行情序列 + 期望结果)由使用方策略 crate 的 `TESTS.md` 规定,本库提供 fixture 读取工具供其以 dev 依赖使用。
- 基准:SLB-136–SLB-152 全部登记在 `benches/budgets.toml`;数值上限只在 [benchmarks](../../benchmarks/README.md) 与 README"延迟与资源契约"登记,CI 与 dev2 只报告,参考机上由 `cargo xtask bench-gate --reference` 判定,超预算即失败。
- SLB-091(资金费结算费率与交易所历史的逐期核对)在 M3 交易所接入后执行一次,结论写入对应 `venue-*` README。
