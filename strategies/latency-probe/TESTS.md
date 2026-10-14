---
component: latency-probe
prefix: LPR
spec: README.md
---

# latency-probe 测试规格

格式与规则见 [docs/tests](../../docs/tests/README.md)。

## 覆盖

| README 小节 | 用例 |
|---|---|
| 概要 | LPR-001、LPR-064 |
| 目录 | 不适用:非规范 |
| 使用本组件 | 不适用:见子节 |
| 用途与运行要求 | LPR-065、LPR-070、LPR-078、LPR-079、LPR-081、REG-013 |
| 装配与策略接口 | LPR-001 |
| 组 | LPR-006–LPR-008、LPR-033、LPR-034 |
| 参数 schema(`ProbeParams`) | LPR-004–LPR-007、LPR-014–LPR-016、LPR-019、LPR-023、LPR-024、LPR-034、LPR-052、LPR-061 |
| 事件处理 | LPR-009、LPR-015–LPR-022、LPR-036–LPR-039、LPR-049–LPR-053、LPR-057、LPR-071 |
| 意图 | LPR-010、LPR-020、LPR-021、LPR-033、LPR-035、LPR-040、LPR-041、LPR-060 |
| 产生的延迟样本 | LPR-030、LPR-065–LPR-068 |
| Note 码表与报表字段 | LPR-002、LPR-003、LPR-013、LPR-020、LPR-042、LPR-045、LPR-052、LPR-056、LPR-057 |
| 运行模式 | LPR-015、LPR-018、LPR-049 |
| 实现说明 | 不适用:见子节 |
| 状态(`ProbeGroup`,≤ 256 B) | LPR-025–LPR-027、LPR-054、LPR-055、LPR-063 |
| quote 探测 | LPR-009–LPR-032、LPR-052、LPR-053、LPR-058、LPR-059、LPR-065、LPR-066、LPR-071 |
| pair 探测 | LPR-033–LPR-040、LPR-043、LPR-048、LPR-067、LPR-069、LPR-071 |
| target 探测 | LPR-041–LPR-048、LPR-062、LPR-068、LPR-071 |
| 规划源文件 | LPR-064 |
| 延迟与资源契约 | LPR-063、LPR-072–LPR-077、LPR-080、LPR-082、LPR-083 |
| 测试与验收 | 不适用:非规范(用例与验收在本文件) |
| 依赖 | LPR-064 |
| 延伸阅读 | 不适用:非规范 |
| 已知限制与后续工作 | 不适用:非规范 |
| 开发备忘 | LPR-081 |

## 用例

**共用夹具**(用例的 `前置` 引用这里的名字;用例依赖的参数值仍在 `前置` 里写出):

- **`RL1`**(SDK `testing::RefFixture`):账户 `T1`(Live,`is_prod = false`,venues {V1, V2, V3},三者属于不同交易所);资产 BTC(`px_exp = −2`、`qty_exp = −8`);`I1`(V1 现货,tick 1、lot 1_000、`min_qty` 1_000)、`I2`(V2 USDT 永续,tick 10、lot 100_000、`min_qty` 100_000)、`I3`(V3 USDT 永续,同 I2);三者 `min_notional` = 5_000_000(5 USDT)。
- **`PP0`**(全部 17 个字段显式取值):`enabled = true`、`mode = quote`、`probe_period_ms = 1_000`、`far_bp = 30_000_000`(300 bp)、`quote_side = buy`、`quote_rest_ms = 2_000`、`quote_xgw = false`、`burst_period_ms = 0`、`pair_period_ms = 10_000`、`target_period_ms = 60_000`、`pair_flat_ms = 5_000`、`pair_cross = 5_000_000`(50 bp)、`pair_side = auto`、`size_margin = 100_000_000`(10%)、`max_spread = 10_000_000`(100 bp)、`max_inv_notional = 100_000_000`(100 USDT)、`max_consec_fail = 5`。用例写"`PP0` 改 x = y"表示只改该字段。
- **group**(GroupTable 只含用例所列的 group;同一 inst 只在一个 group):`GQ`(quote,legs [I2]);`GX`(quote,`quote_xgw = true`,legs [I2, I3]);`GX3`(quote,`quote_xgw = true`,legs [I2, I3, I1]);`GPS`(pair,A = I1 现货、B = I2 永续);`GSP`(pair,A = I2 永续、B = I1 现货);`GPP`(pair,A = I2、B = I3);`GTS`(target,A = I1、B = I2)。mode 字段按 group 覆盖。
- **`MV0`**(SDK `testing::MockViews`):`now.engine_ns` 起点 `E0` = 1_000_000_000_000,`+x ms` 表示 `E0 + x × 10^6`;`tsc_per_ms = 3_000_000`;全部 cell 有效模式 `Live`、`open_allowed = true`;`risk(T1)`:`margin_level = None`、`conn_liveness_ms = 2_000`;BBO(bid / ask):I1 6_000_000 / 6_000_001、I2 6_000_000 / 6_000_010、I3 6_000_100 / 6_000_110,数量各 1_000_000_000,连接健康(`quote_ok` 为真);各腿净量 0;无生效目标。
- **`CH`** = SDK `testing::CtxHarness::new(&LatencyProbe::DESC)`;"上一事件的结果"用 `CH.set_outcomes` 设定;未写明时订单意图的结果为 `Placed(r)`(r 依次为 r1、r2 …)。
- 场景驱动:单元用例经 `LatencyProbe::on_event` 直接喂 SDK `testing::ev` 构造的事件;引擎级用例在 `strategies/registry/tests/` 以 registry 的 `Strategies` 实例化 hft-testkit 的场景与 DST 驱动(撮合器即 hft-sim 的 `SimExchange`)。

### LPR-001 描述符常量

- 规则:[装配与策略接口](README.md#装配与策略接口)、[概要](README.md#概要)
- 级别:unit
- 里程碑:M2
- 前置:无
- 步骤:
  1. 读 `LatencyProbe::DESC` 与 `<LatencyProbe as Strategy>::Model` 的常量
- 期望:
  - `name = "latency-probe"`、`kind_id = 5`、`version = 1`、`non_prod_only = true`、`schema.id = 5`、`schema.version = 1`、`cmds` 为空、`notes` 9 项、`report` 8 项
  - Model 为 `NoModel<ProbeParams>`:`LAYOUT_VERSION = 0`、`PATH_VERSION = 0`、`WARMUP = false`,`mem_bytes`/`snapshot_bytes` 为 0
- 实现:`strategies/latency-probe/tests/desc.rs::lpr_001_desc_constants`

### LPR-002 Note 码表

- 规则:[Note 码表与报表字段](README.md#note-码表与报表字段)
- 级别:table
- 里程碑:M2
- 前置:`LatencyProbe::DESC.notes`
- 实现:`strategies/latency-probe/tests/desc.rs::lpr_002_note_table`

| 行 | code | 期望 name / level / v 项数 |
|---|---|---|
| LPR-002.1 | 1 | `ProbeFilled` / P2 / 3 |
| LPR-002.2 | 2 | `ProbeRejected` / Info / 3 |
| LPR-002.3 | 3 | `ProbeGated` / Info / 2 |
| LPR-002.4 | 4 | `PairNoFill` / Info / 1 |
| LPR-002.5 | 5 | `Skipped` / Info / 2 |
| LPR-002.6 | 6 | `FlattenIncomplete` / P1 / 2 |
| LPR-002.7 | 7 | `ProbeHalted` / P1 / 2 |
| LPR-002.8 | 8 | `TargetStuck` / P2 / 2 |
| LPR-002.9 | 9 | `TargetRejected` / P2 / 1 |
| LPR-002.10 | 10 | 不存在(码表只有 1–9) |

### LPR-003 报表字段

- 规则:[Note 码表与报表字段](README.md#note-码表与报表字段)
- 级别:table
- 里程碑:M2
- 前置:`LatencyProbe::DESC.report`
- 实现:`strategies/latency-probe/tests/desc.rs::lpr_003_report_fields`

| 行 | name | 期望 unit / agg / scope / note / v_ix |
|---|---|---|
| LPR-003.1 | `probe_fills` | Count / Count / Group / 1 / 0 |
| LPR-003.2 | `rejects` | Count / Sum / Group / 2 / 0 |
| LPR-003.3 | `gated` | Count / Sum / Group / 3 / 0 |
| LPR-003.4 | `pair_nofill` | Count / Sum / Group / 4 / 0 |
| LPR-003.5 | `skipped_spread` | Count / Sum / Group / 5 / 0 |
| LPR-003.6 | `flatten_incomplete` | Count / Count / Group / 6 / 0 |
| LPR-003.7 | `target_stuck` | Count / Count / Group / 8 / 0 |
| LPR-003.8 | `target_rejected` | Count / Count / Group / 9 / 0 |

### LPR-004 参数 schema 的下标、默认值与属性

- 规则:[参数 schema(`ProbeParams`)](README.md#参数-schemaprobeparams)
- 级别:table
- 里程碑:M2
- 前置:`ProbeParams::SCHEMA`(`schema_id = 5`、`version = 1`,17 个字段,`since` 都为 1)
- 实现:`strategies/latency-probe/tests/params.rs::lpr_004_schema_defaults`

| 行 | ix | 期望 字段 / 单位 / 默认(原始值) / per_group / 属性 |
|---|---|---|
| LPR-004.1 | 0 | `enabled` / Bool / 0 / 否 / `manual_only`、`relax = Up` |
| LPR-004.2 | 1 | `mode` / Enum[quote, pair, target] / 0(quote)/ 是 / `manual_only` |
| LPR-004.3 | 2 | `probe_period_ms` / DurationMs / 1_000 / 是 / `relax = Down` |
| LPR-004.4 | 3 | `far_bp` / Rate / 30_000_000 / 是 / `relax = Down` |
| LPR-004.5 | 4 | `quote_side` / Enum[buy, sell, alternate] / 0(buy)/ 是 / — |
| LPR-004.6 | 5 | `quote_rest_ms` / DurationMs / 2_000 / 是 / — |
| LPR-004.7 | 6 | `quote_xgw` / Bool / 0 / 是 / — |
| LPR-004.8 | 7 | `burst_period_ms` / DurationMs / 0 / 是 / — |
| LPR-004.9 | 8 | `pair_period_ms` / DurationMs / 10_000 / 是 / `relax = Down` |
| LPR-004.10 | 9 | `target_period_ms` / DurationMs / 60_000 / 是 / `relax = Down` |
| LPR-004.11 | 10 | `pair_flat_ms` / DurationMs / 5_000 / 是 / — |
| LPR-004.12 | 11 | `pair_cross` / Rate / 5_000_000 / 是 / `relax = Up` |
| LPR-004.13 | 12 | `pair_side` / Enum[auto, buy_a, sell_a] / 0(auto)/ 是 / — |
| LPR-004.14 | 13 | `size_margin` / Rate / 100_000_000 / 否 / — |
| LPR-004.15 | 14 | `max_spread` / Rate / 10_000_000 / 是 / `relax = Up` |
| LPR-004.16 | 15 | `max_inv_notional` / Notional / 100_000_000 / 是 / `relax = Up` |
| LPR-004.17 | 16 | `max_consec_fail` / Count / 5 / 否 / `relax = Up` |
| LPR-004.18 | 全部 | `structural = false`、`minor_change = None`(schema JSON 中为 `"none"`) |

### LPR-005 参数范围的边界

- 规则:[参数 schema(`ProbeParams`)](README.md#参数-schemaprobeparams)
- 级别:table
- 里程碑:M2
- 前置:`PP0` 上只改所列字段的默认值,读 SDK `params::encode_body` 的结果;"下限 − 1 / 下限 / 上限 / 上限 + 1"依次取所列四个原始值;期望列的 `OutOfRange` 指 `Err(OutOfRange{field_ix: 该字段的 ix, group_ix: 0xFFFF})`,`BadEnum` 指 `Err(BadEnum{field_ix: 1})`
- 实现:`strategies/latency-probe/tests/params.rs::lpr_005_ranges`

| 行 | 字段:四个原始值 | 期望 |
|---|---|---|
| LPR-005.1 | `probe_period_ms`:99、100、600_000、600_001 | `OutOfRange`、Ok、Ok、`OutOfRange` |
| LPR-005.2 | `far_bp`:4_999_999、5_000_000、100_000_000、100_000_001 | `OutOfRange`、Ok、Ok、`OutOfRange` |
| LPR-005.3 | `quote_rest_ms`:99、100、600_000、600_001 | `OutOfRange`、Ok、Ok、`OutOfRange` |
| LPR-005.4 | `burst_period_ms`:−1、0、600_000、600_001 | `OutOfRange`、Ok、Ok、`OutOfRange` |
| LPR-005.5 | `pair_period_ms`:9_999、10_000、3_600_000、3_600_001 | `OutOfRange`、Ok、Ok、`OutOfRange` |
| LPR-005.6 | `target_period_ms`:9_999、10_000、3_600_000、3_600_001 | `OutOfRange`、Ok、Ok、`OutOfRange` |
| LPR-005.7 | `pair_flat_ms`:999、1_000、600_000、600_001 | `OutOfRange`、Ok、Ok、`OutOfRange` |
| LPR-005.8 | `pair_cross`:99_999、100_000、50_000_000、50_000_001 | `OutOfRange`、Ok、Ok、`OutOfRange` |
| LPR-005.9 | `size_margin`:−1、0、1_000_000_000、1_000_000_001 | `OutOfRange`、Ok、Ok、`OutOfRange` |
| LPR-005.10 | `max_spread`:99_999、100_000、100_000_000、100_000_001 | `OutOfRange`、Ok、Ok、`OutOfRange` |
| LPR-005.11 | `max_inv_notional`:999_999、1_000_000、10_000_000_000、10_000_000_001 | `OutOfRange`、Ok、Ok、`OutOfRange` |
| LPR-005.12 | `max_consec_fail`:0、1、100、101 | `OutOfRange`、Ok、Ok、`OutOfRange` |
| LPR-005.13 | `mode`:枚举下标 3 | `BadEnum` |

### LPR-006 `check` 拒绝不合法的腿形

- 规则:[组](README.md#组)、[参数 schema(`ProbeParams`)](README.md#参数-schemaprobeparams)
- 级别:table
- 里程碑:M2
- 前置:`RL1`、`PP0`;每行一张 GroupTable,经 `Strategies`/`ProbeParams` 的 `validate` 调 `check`;另有 inst `I4`(V1 现货,资产 ETH)与 `I5`(V2 永续,资产 ETH)
- 实现:`strategies/latency-probe/tests/params.rs::lpr_006_check_group_shape`

| 行 | GroupTable(gix 0 起) | 期望 |
|---|---|---|
| LPR-006.1 | [`GQ`] | `Ok(())` |
| LPR-006.2 | [quote,legs = 0 条] | `Err(GroupShape{gix: 0, code: 1})` |
| LPR-006.3 | [quote,legs = 5 条] | `Err(GroupShape{gix: 0, code: 1})` |
| LPR-006.4 | [quote,legs = 4 条(I1、I2、I3、I5)] | `Ok(())` |
| LPR-006.5 | [pair,legs [I2]] | `Err(GroupShape{gix: 0, code: 1})` |
| LPR-006.6 | [target,legs [I1, I2, I3]] | `Err(GroupShape{gix: 0, code: 1})` |
| LPR-006.7 | [pair,A = I1 现货、B = I4 现货] | `Err(GroupShape{gix: 0, code: 2})` |
| LPR-006.8 | [target,A = I4、B = I1(都是现货)] | `Err(GroupShape{gix: 0, code: 2})` |
| LPR-006.9 | [quote,`quote_xgw = true`,legs [I2]] | `Err(GroupShape{gix: 0, code: 3})` |
| LPR-006.10 | [quote,`quote_xgw = true`,legs [I2, I5](同 venue V2)] | `Err(GroupShape{gix: 0, code: 3})` |
| LPR-006.11 | [`GX`] | `Ok(())` |
| LPR-006.12 | [pair `GPS`,`quote_xgw = true`] | `Ok(())`(非 quote 组忽略) |
| LPR-006.13 | [`GQ`, quote 0 条腿] | `Err(GroupShape{gix: 1, code: 1})` |

### LPR-007 `check` 拒绝与腿型冲突的字段

- 规则:[组](README.md#组)、[参数 schema(`ProbeParams`)](README.md#参数-schemaprobeparams)、[pair 探测](README.md#pair-探测)
- 级别:table
- 里程碑:M2
- 前置:`RL1`、`PP0`;每行一张 GroupTable 与所列覆盖
- 实现:`strategies/latency-probe/tests/params.rs::lpr_007_check_fields`

| 行 | GroupTable 与覆盖 | 期望 |
|---|---|---|
| LPR-007.1 | [quote,legs [I1]],`quote_side = sell` | `Err(Invalid{field_ix: 4, code: 1})` |
| LPR-007.2 | [quote,legs [I1]],`quote_side = alternate` | `Ok(())`(现货腿只挂买单) |
| LPR-007.3 | [quote,legs [I2]],`quote_side = sell` | `Ok(())` |
| LPR-007.4 | [`GPS`],`pair_side = sell_a` | `Err(Invalid{field_ix: 12, code: 1})` |
| LPR-007.5 | [`GPS`],`pair_side = buy_a` | `Ok(())` |
| LPR-007.6 | [`GSP`],`pair_side = buy_a` | `Err(Invalid{field_ix: 12, code: 1})`(B 现货会被卖出) |
| LPR-007.7 | [`GSP`],`pair_side = sell_a` | `Ok(())` |
| LPR-007.8 | [`GPP`],`pair_side = sell_a` | `Ok(())` |
| LPR-007.9 | [`GQ`],`burst_period_ms = 1_001`、`probe_period_ms = 1_000` | `Err(Invalid{field_ix: 7, code: 1})` |
| LPR-007.10 | [`GQ`],`burst_period_ms = 1_000`、`probe_period_ms = 1_000` | `Ok(())` |
| LPR-007.11 | [`GQ`],`burst_period_ms = 999` | `Ok(())` |
| LPR-007.12 | [quote,legs = 0 条],`quote_side = sell` | `Err(GroupShape{gix: 0, code: 1})`(先查腿形) |

### LPR-008 订阅:每条腿 BBO、只给快速部分

- 规则:[组](README.md#组)
- 级别:unit
- 里程碑:M2
- 前置:`RL1`、`PP0`;GroupTable [`GQ`, pair 组(A = I1、B = I3)](gix 0、1)
- 步骤:
  1. `subscriptions` 写入记录型 `SubsBuilder`
- 期望:
  - 恰好 3 次 `market`,依次 (I2, 0, `BBO`, `Fast`)、(I1, 1, `BBO`, `Fast`)、(I3, 1, `BBO`, `Fast`);没有 `model_tick`、`model_own`、`warmup` 调用
- 实现:`strategies/latency-probe/tests/scenario_quote.rs::lpr_008_subscriptions`

### LPR-009 quote:BBO 每 100 ms 一条时每个探测周期恰好一张 PostOnly

- 规则:[quote 探测](README.md#quote-探测)、[事件处理](README.md#事件处理)
- 级别:unit
- 里程碑:M2
- 前置:`RL1`、`MV0`、`CH`;`PP0`;GroupTable [`GQ`]
- 步骤:
  1. 在 +0、+100 … +2_900 ms 各投递一条 I2 的 `Bbo`(30 条);每张新单在其后 +50 ms 投递 `Order{Acked}`、+80 ms 投递 `Order{Cancelled}`(按时间顺序与 Bbo 交错)
- 期望:
  - 共 3 次 `place`,分别在 +0、+1_000、+2_000 ms 的 `Bbo` 上;每次:`venue = V2`、`side = Buy`、`tif = PostOnly`、`class = Open`、`flags = 0`、`inst = I2`、`px = 5_820_000`、`qty = 100_000`、`max_rest_ms = 2_000`、`link.some = 0`
  - 共 3 次 `cancel`,各在对应 `Acked` 的同一事件内、目标为该单的 `OrderRef`
  - 其余 27 条 `Bbo` 不产生任何意图与 Note
- 实现:`strategies/latency-probe/tests/scenario_quote.rs::lpr_009_one_probe_per_period`

### LPR-010 quote 探测单的价格与数量

- 规则:[意图](README.md#意图)、[quote 探测](README.md#quote-探测)
- 级别:table
- 里程碑:M2
- 前置:`RL1`、`MV0`、`CH`;`PP0` 加所列改动;GroupTable 为单腿 quote 组(腿如所列);在 +0 投递该腿的 `Bbo`
- 实现:`strategies/latency-probe/tests/scenario_quote.rs::lpr_010_price_and_qty`

| 行 | 腿、方向与参数改动 | 期望 `px` / `qty` |
|---|---|---|
| LPR-010.1 | I2,buy | 5_820_000 / 100_000(6_000_010 × 0.97 = 5 820 009.7 → 向下到 tick) |
| LPR-010.2 | I2,`quote_side = sell` | 6_180_000 / 100_000 |
| LPR-010.3 | I3,buy | 5_820_100 / 100_000(5 820 106.7 → 5 820 100) |
| LPR-010.4 | I3,`quote_side = sell` | 6_180_110 / 100_000(6 180 103 → 向上到 tick) |
| LPR-010.5 | I1(现货),buy | 5_820_000 / 10_000(q × px ≥ 5.5 USDT 的最小 lot 倍数) |
| LPR-010.6 | I1,buy,`size_margin = 0` | 5_820_000 / 9_000(9_000 → 5.238 USDT ≥ 5;8_000 → 4.656 < 5) |
| LPR-010.7 | I1,buy,`size_margin = 1_000_000_000` | 5_820_000 / 18_000(≥ 10 USDT:18_000 → 10.476,17_000 → 9.894) |
| LPR-010.8 | I2,buy,`size_margin = 0` | 5_820_000 / 100_000(`min_qty` 下限) |
| LPR-010.9 | I2,buy,`far_bp = 5_000_000` | 5_970_000 / 100_000 |

### LPR-011 quote 探测周期的等号边界

- 规则:[quote 探测](README.md#quote-探测)
- 级别:table
- 里程碑:M2
- 前置:`RL1`、`MV0`、`CH`;`PP0`;[`GQ`];+0 的 `Bbo` 已挂单 r1,+50 `Acked`、+80 `Cancelled` 已处理;每行再投递一条 I2 的 `Bbo`
- 实现:`strategies/latency-probe/tests/scenario_quote.rs::lpr_011_period_boundary`

| 行 | `Bbo` 的 `now.engine_ns` | 期望 |
|---|---|---|
| LPR-011.1 | E0 + 999_999_999 | 无 `place` |
| LPR-011.2 | E0 + 1_000_000_000 | 1 次 `place` |
| LPR-011.3 | E0 + 1_000_000_001 | 1 次 `place` |

### LPR-012 有在途探测单时到期也不再挂

- 规则:[quote 探测](README.md#quote-探测)
- 级别:unit
- 里程碑:M2
- 前置:`RL1`、`MV0`、`CH`;`PP0`;[`GQ`];+0 挂单 r1(`Placed(r1)` 已处理),未收到 Ack
- 步骤:
  1. +1_500 ms 投递 `Bbo`
  2. +1_600 ms 投递 `Order{r1, Acked}`、+1_700 ms `Order{r1, Cancelled}`
  3. +1_800 ms 投递 `Bbo`
- 期望:
  - 步骤 1 无 `place`、无 `Skipped` 计数(该腿不空闲)
  - 步骤 2 只有一次 `cancel(r1)`
  - 步骤 3 一次 `place`
- 实现:`strategies/latency-probe/tests/scenario_quote.rs::lpr_012_no_probe_while_inflight`

### LPR-013 到期且空闲时报价不健康或价差过宽计 `Skipped`

- 规则:[quote 探测](README.md#quote-探测)、[Note 码表与报表字段](README.md#note-码表与报表字段)
- 级别:table
- 里程碑:M2
- 前置:`RL1`、`MV0`、`CH`;`PP0`;[`GQ`];group 起步于 +0(`note_due_ens` = +600_000 ms);每行在 +1_000 ms 投递一条按所列状态的 I2 `Bbo`,再在 +600_000 ms 投递一条健康的 `Bbo` 读出 Note
- 实现:`strategies/latency-probe/tests/scenario_quote.rs::lpr_013_skipped_counts`

| 行 | +1_000 ms 时的状态 | 期望(+1_000 ms 的意图;+600_000 ms 的 Note) |
|---|---|---|
| LPR-013.1 | 连接静默:`conn_live_tsc` 早于 now 7 s | 无 `place`;`Note{Skipped, v0 = 0, v1 = 1}` |
| LPR-013.2 | bid 6_000_000、ask 6_100_000(价差 16 528 926 > 10^7) | 无 `place`;`Note{Skipped, v0 = 1, v1 = 0}` |
| LPR-013.3 | bid 0、ask 6_000_010 | 无 `place`;`Note{Skipped, v0 = 1, v1 = 0}` |
| LPR-013.4 | 连接静默,但 +500 ms 刚挂过单并已撤完(未到期) | 无 `place`;+600_000 ms 无 `Skipped` |
| LPR-013.5 | 连接静默,该腿有在途单 | 无 `place`;+600_000 ms 无 `Skipped` |
| LPR-013.6 | 健康(对照) | 1 次 `place`;+600_000 ms 无 `Skipped` |

### LPR-014 价差按 mid 计算、含等号

- 规则:[quote 探测](README.md#quote-探测)、[参数 schema(`ProbeParams`)](README.md#参数-schemaprobeparams)
- 级别:table
- 里程碑:M2
- 前置:`RL1`、`MV0`、`CH`;`PP0`(`max_spread = 10_000_000`);[`GQ`];每行 +0 投递一条 I2 `Bbo`
- 实现:`strategies/latency-probe/tests/scenario_quote.rs::lpr_014_spread_boundary`

| 行 | bid / ask | 期望 |
|---|---|---|
| LPR-014.1 | 9_950_000 / 10_050_000(mid 10_000_000,价差 10_000_000) | 1 次 `place` |
| LPR-014.2 | 9_950_000 / 10_050_010(价差 10_000_995) | 无 `place`,`Skipped` v0 + 1 |
| LPR-014.3 | 9_950_000 / 10_049_990(价差 9_999_005) | 1 次 `place` |

### LPR-015 只在有效模式 Live、`enabled`、无保证金压力时探测

- 规则:[quote 探测](README.md#quote-探测)、[运行模式](README.md#运行模式)、[事件处理](README.md#事件处理)
- 级别:table
- 里程碑:M2
- 前置:`RL1`、`MV0`、`CH`;`PP0` 加所列改动;[`GQ`];+0 投递一条健康的 I2 `Bbo`
- 实现:`strategies/latency-probe/tests/scenario_quote.rs::lpr_015_gates`

| 行 | 改动 | 期望 |
|---|---|---|
| LPR-015.1 | 无 | 1 次 `place` |
| LPR-015.2 | `enabled = false` | 无意图 |
| LPR-015.3 | 该 cell 有效模式 `CloseOnly` | 无意图 |
| LPR-015.4 | 该 cell 有效模式 `Shadow` | 无意图 |
| LPR-015.5 | `risk(T1).margin_level = L1` | 无意图 |
| LPR-015.6 | `Throttle` 已 halted(先投递本 cell 的 `Risk{CapSet{Halted}}`) | 无意图 |
| LPR-015.7 | 同 .6,再投递 `Risk{CapCleared{Halted}}`,之后的 `Bbo` | 1 次 `place` |
| LPR-015.8 | 另一 cell 的 `Risk{CapSet{Halted}}` | 1 次 `place` |

### LPR-016 `enabled = false` 后已挂的探测单照常撤,残余照常平

- 规则:[参数 schema(`ProbeParams`)](README.md#参数-schemaprobeparams)、[事件处理](README.md#事件处理)
- 级别:unit
- 里程碑:M2
- 前置:`RL1`、`MV0`、`CH`;`PP0`;[`GQ`];+0 挂单 r1
- 步骤:
  1. `on_params` 改 `enabled = false`
  2. +50 ms `Order{r1, Acked}`
  3. +60 ms `Order{r1, Filled{d: 100_000, px: 5_820_000, done: true}}`(I2 净量 +100_000)
  4. +2_000 ms `Bbo`
- 期望:
  - 步骤 2:`cancel(r1)`
  - 步骤 3:写 `ProbeFilled{v0 = 0, v1 = 100_000, v2 = 5_820_000}`;一次 `place`:I2 `Sell`、`Ioc`、`class = Flatten`、`flags = REDUCE_ONLY`、`qty = 100_000`、`px = 5_970_000`
  - 步骤 4:无 `place`
- 实现:`strategies/latency-probe/tests/scenario_quote.rs::lpr_016_disabled_still_cleans_up`

### LPR-017 订单回报对在途与失败计数的作用

- 规则:[事件处理](README.md#事件处理)、[quote 探测](README.md#quote-探测)
- 级别:table
- 里程碑:M2
- 前置:`RL1`、`MV0`、`CH`;`PP0`;[`GQ`];+0 挂单 r1;每行在 +50 ms 投递所列回报,再在 +1_000 ms 投递 `Bbo`,+600_000 ms 投递 `Bbo` 读出计数 Note
- 实现:`strategies/latency-probe/tests/scenario_quote.rs::lpr_017_order_outcomes`

| 行 | +50 ms 的回报 | 期望 |
|---|---|---|
| LPR-017.1 | `Cancelled` | +1_000 ms 1 次 `place`;无计数 Note |
| LPR-017.2 | `Aged` | +1_000 ms 1 次 `place`;无计数 Note |
| LPR-017.3 | `Rejected{bits: 0, venue_code: 1013}` | +1_000 ms 1 次 `place`;`Note{ProbeRejected, v0 = 1, v1 = 0, v2 = 1013}` |
| LPR-017.4 | `Rejected{bits: LOCAL_SEND_FAIL, venue_code: 3}` | +1_000 ms 1 次 `place`;`Note{ProbeGated, v0 = 1, v1 = LOCAL_SEND_FAIL}`;无 `ProbeRejected` |
| LPR-017.5 | `Unknown` | +1_000 ms 无 `place`(暂停) |

### LPR-018 上一事件意图的结果:`Placed`、门控 `Rejected`、`Shadowed`

- 规则:[事件处理](README.md#事件处理)、[运行模式](README.md#运行模式)
- 级别:table
- 里程碑:M2
- 前置:`RL1`、`MV0`、`CH`;`PP0`;[`GQ`];+0 挂单;每行用 `CH.set_outcomes` 设该意图的结果,在 +10 ms 投递 `Bbo`(使策略读到结果),+1_000 ms 再投递 `Bbo`,+600_000 ms 读出计数 Note
- 实现:`strategies/latency-probe/tests/scenario_quote.rs::lpr_018_intent_outcomes`

| 行 | 结果 | 期望 |
|---|---|---|
| LPR-018.1 | `Placed(r1)` | +1_000 ms 无 `place`(r1 在途);之后 `Order{r1, Acked}` 撤的是 r1 |
| LPR-018.2 | `Rejected(PX_BAND)` | +1_000 ms 1 次 `place`;`Note{ProbeGated, v0 = 1, v1 = PX_BAND}`;连续失败数不变 |
| LPR-018.3 | `Shadowed` | +1_000 ms 1 次 `place`;无 `ProbeGated`、无 `ProbeRejected` |

### LPR-019 连续 `max_consec_fail` 次交易所拒单请求 Halted

- 规则:[quote 探测](README.md#quote-探测)、[参数 schema(`ProbeParams`)](README.md#参数-schemaprobeparams)
- 级别:table
- 里程碑:M2
- 前置:`RL1`、`MV0`、`CH`;`PP0` 改 `max_consec_fail` 如所列;[`GQ`];每秒一个探测周期,每张单的回报按序列给出(R = `Rejected{bits: 0}`,G = `Rejected{bits: RATE}`,C = `Cancelled`)
- 实现:`strategies/latency-probe/tests/scenario_quote.rs::lpr_019_consec_fail_halt`

| 行 | `max_consec_fail` / 回报序列 | 期望 |
|---|---|---|
| LPR-019.1 | 5 / R R R R | 无 `request_halt` |
| LPR-019.2 | 5 / R R R R R | 第 5 个回报的同一事件:`request_halt(GQ, 1)`、`Note{ProbeHalted, v0 = 1, v1 = 5}`;之后的到期 `Bbo` 无 `place` |
| LPR-019.3 | 5 / R R G R R R | 第 6 个回报时 `request_halt`(门控不计数) |
| LPR-019.4 | 5 / R R C R R R | 无 `request_halt`(撤单成功清零) |
| LPR-019.5 | 1 / R | 第 1 个回报即 `request_halt(GQ, 1)`、`ProbeHalted{v1 = 1}` |

### LPR-020 意外成交写 `ProbeFilled` 并平掉该腿

- 规则:[事件处理](README.md#事件处理)、[意图](README.md#意图)、[Note 码表与报表字段](README.md#note-码表与报表字段)
- 级别:unit
- 里程碑:M2
- 前置:`RL1`、`MV0`、`CH`;`PP0`;[`GQ`];+0 挂单 r1,+50 ms `Acked`(已发 `cancel(r1)`)
- 步骤:
  1. +60 ms `Order{r1, Filled{d: 40_000, px: 5_820_000, done: false}}`;视图 I2 净量 +40_000
  2. +70 ms `Order{r1, Cancelled}`;+1_000 ms `Bbo`
- 期望:
  - 步骤 1:`Note{ProbeFilled, v0 = 0, v1 = 40_000, v2 = 5_820_000}`(P2);没有 `place`(残余 40_000 向零取整到 lot 100_000 为 0)
  - 步骤 2:`Cancelled` 清在途;+1_000 ms 一次 `place`
- 实现:`strategies/latency-probe/tests/scenario_quote.rs::lpr_020_unexpected_fill_below_lot`

### LPR-021 意外成交满一个 lot 时发残余平仓 IOC

- 规则:[事件处理](README.md#事件处理)、[意图](README.md#意图)
- 级别:unit
- 里程碑:M2
- 前置:同 LPR-020
- 步骤:
  1. +60 ms `Order{r1, Filled{d: 100_000, px: 5_820_000, done: true}}`;视图 I2 净量 +100_000
- 期望:
  - `Note{ProbeFilled, v0 = 0, v1 = 100_000, v2 = 5_820_000}`;一次 `place`:I2、`Sell`、`Ioc`、`class = Flatten`、`flags = REDUCE_ONLY`、`qty = 100_000`、`px = 5_970_000`(bid 6_000_000 × (1 − 0.005),向上到 tick);在途清除
- 实现:`strategies/latency-probe/tests/scenario_quote.rs::lpr_021_unexpected_fill_flatten`

### LPR-022 `Unknown` 暂停该腿直到同一订单的终态

- 规则:[事件处理](README.md#事件处理)、[quote 探测](README.md#quote-探测)
- 级别:unit
- 里程碑:M2
- 前置:`RL1`、`MV0`、`CH`;`PP0`;[`GQ`];+0 挂单 r1
- 步骤:
  1. +50 ms `Order{r1, Unknown}`;+1_000、+2_000 ms 各一条健康 `Bbo`
  2. +2_100 ms `Order{r1, Cancelled}`;+2_200 ms `Bbo`
- 期望:
  - 步骤 1:两条 `Bbo` 都无 `place`;+600_000 ms 无 `Skipped`(暂停的腿不计)
  - 步骤 2:+2_200 ms 一次 `place`
- 实现:`strategies/latency-probe/tests/scenario_quote.rs::lpr_022_unknown_pauses_leg`

### LPR-023 `alternate` 在永续腿上换向、在现货腿上恒为买

- 规则:[参数 schema(`ProbeParams`)](README.md#参数-schemaprobeparams)、[quote 探测](README.md#quote-探测)
- 级别:table
- 里程碑:M2
- 前置:`RL1`、`MV0`、`CH`;`PP0` 改 `quote_side = alternate`;每行一个单腿 quote 组;连续 4 个探测周期(每张单都正常撤)
- 实现:`strategies/latency-probe/tests/scenario_quote.rs::lpr_023_alternate`

| 行 | 腿 | 期望 4 张单的方向 |
|---|---|---|
| LPR-023.1 | I2(永续) | Buy、Sell、Buy、Sell |
| LPR-023.2 | I1(现货) | Buy、Buy、Buy、Buy |

### LPR-024 突发触发:同一读取凑齐的 BBO 在 `burst_period_ms` 后即可探测

- 规则:[quote 探测](README.md#quote-探测)、[参数 schema(`ProbeParams`)](README.md#参数-schemaprobeparams)
- 级别:table
- 里程碑:M2
- 前置:`RL1`、`MV0`、`CH`;`PP0` 改 `burst_period_ms` 如所列;[`GQ`];+0 的 `Bbo`(`rx_tsc = 1_000`)挂单并在 +80 ms 撤完;每行在所列时刻连续投递两条 I2 `Bbo`(不同 `update_id`),`rx_tsc` 如所列
- 实现:`strategies/latency-probe/tests/scenario_quote.rs::lpr_024_burst_trigger`

| 行 | `burst_period_ms` / 时刻 / 两条的 `rx_tsc` | 期望(第一条无 `place`;第二条上) |
|---|---|---|
| LPR-024.1 | 200 / +300 ms / 相同(9_000_000) | 1 次 `place` |
| LPR-024.2 | 200 / +300 ms / 不同 | 无 `place` |
| LPR-024.3 | 200 / E0 + 199_999_999 / 相同 | 无 `place` |
| LPR-024.4 | 200 / E0 + 200_000_000 / 相同 | 1 次 `place` |
| LPR-024.5 | 0 / +300 ms / 相同 | 无 `place` |
| LPR-024.6 | 200 / +300 ms / 都为 0(未知) | 无 `place` |

### LPR-025 突发判别的上一条 `rx_tsc` 不进快照

- 规则:[状态(`ProbeGroup`,≤ 256 B)](README.md#状态probegroup-256-b)、[quote 探测](README.md#quote-探测)
- 级别:unit
- 里程碑:M2
- 前置:同 LPR-024.1 的前置(`burst_period_ms = 200`),+250 ms 已投递一条 I2 `Bbo`(`rx_tsc = 9_000_000`,未到期、无 `place`)
- 步骤:
  1. `snapshot`;`MockViews::restart`(engine_ns 连续);新实例 `restore`
  2. +300 ms 投递 I2 `Bbo`(`rx_tsc = 9_000_000`)
- 期望:
  - 步骤 2 无 `place`(恢复后 `last_bbo_rx_tsc = 0`,且普通周期未到)
- 实现:`strategies/latency-probe/tests/scenario_quote.rs::lpr_025_burst_state_not_persisted`

### LPR-026 重启后探测间隔按 engine_ns 继续,快照不含 TSC

- 规则:[状态(`ProbeGroup`,≤ 256 B)](README.md#状态probegroup-256-b)、[quote 探测](README.md#quote-探测)
- 级别:unit
- 里程碑:M2
- 前置:`RL1`、`MV0`、`CH`;`PP0`;[`GQ`];+0 挂单 r1 并在 +80 ms 撤完
- 步骤:
  1. +500 ms `snapshot` 得 S1;另一套 `MockViews`(TSC 原点 10^15、`tsc_per_ms = 1_000_000`,engine_ns 相同)上跑同一脚本得 S2
  2. `MockViews::restart(new_tsc = 7, new_boot_seq = 2)`(停机不计,engine_ns 从 +500 ms 继续);新实例 `restore(S1)`
  3. 投递 `Bbo` 于 engine_ns E0 + 999_999_999 与 E0 + 1_000_000_000
- 期望:
  - 步骤 1:S1 与 S2 逐字节相同
  - 步骤 3:前者无 `place`,后者 1 次 `place`
- 实现:`strategies/latency-probe/tests/scenario_quote.rs::lpr_026_restart_continues`

### LPR-027 恢复时在挂的探测单被撤,已终结的清除

- 规则:[状态(`ProbeGroup`,≤ 256 B)](README.md#状态probegroup-256-b)
- 级别:table
- 里程碑:M2
- 前置:`RL1`、`MV0`、`CH`;`PP0`;[`GQ`];+0 挂单 r1(`Placed(r1)` 已处理)后 `snapshot`、重启、`restore`;每行设 `v.order(r1)` 的结果后投递 +1_500 ms 的 `Bbo`
- 实现:`strategies/latency-probe/tests/scenario_quote.rs::lpr_027_restore_pending_rest`

| 行 | `v.order(r1)` | 期望 |
|---|---|---|
| LPR-027.1 | `Some(state = Live)` | `cancel(r1)`;无 `place` |
| LPR-027.2 | `Some(state = PendingNew)` | `cancel(r1)`;无 `place` |
| LPR-027.3 | `None`(代数不符) | 无 `cancel`;1 次 `place` |
| LPR-027.4 | `Some(state = Cancelled)` | 无 `cancel`;1 次 `place` |

### LPR-028 跨 gw 变体:leg 0 的 Ack 不撤

- 规则:[quote 探测](README.md#quote-探测)
- 级别:unit
- 里程碑:M2
- 前置:`RL1`、`MV0`、`CH`;`PP0`;[`GX`](leg 0 = I2、leg 1 = I3)
- 步骤:
  1. +0 投递 I2 `Bbo`(挂 r0);+50 ms `Order{r0, Acked}`
- 期望:
  - 步骤 1:一次 `place`(I2);`Acked` 事件无 `cancel`
- 实现:`strategies/latency-probe/tests/scenario_xgw.rs::lpr_028_leg0_ack_kept`

### LPR-029 跨 gw 变体:leg 0 没有已 Ack 的在挂单时 leg 1 不探测

- 规则:[quote 探测](README.md#quote-探测)
- 级别:table
- 里程碑:M2
- 前置:`RL1`、`MV0`、`CH`;`PP0`;[`GX`];每行先构造 leg 0 的状态,再在 +100 ms 投递 I3 `Bbo`
- 实现:`strategies/latency-probe/tests/scenario_xgw.rs::lpr_029_leg1_needs_acked_leg0`

| 行 | leg 0 状态 | 期望(I3 `Bbo` 上) |
|---|---|---|
| LPR-029.1 | 无在途 | 无 `place` |
| LPR-029.2 | 在途 r0,尚未 Ack | 无 `place` |
| LPR-029.3 | r0 已 Ack、在挂 | 1 次 `place`(I3) |

### LPR-030 跨 gw 变体:leg 1 的 Ack 在同一事件先撤 leg 1、再撤 leg 0

- 规则:[quote 探测](README.md#quote-探测)、[产生的延迟样本](README.md#产生的延迟样本)
- 级别:table
- 里程碑:M2
- 前置:`RL1`、`MV0`、`CH`;`PP0`;[`GX`];leg 0 的 r0 已 Ack 在挂,leg 1 已挂 r1
- 实现:`strategies/latency-probe/tests/scenario_xgw.rs::lpr_030_leg1_ack_cancels_both`

| 行 | 先决事件 | `Order{r1, Acked}` 事件的期望 |
|---|---|---|
| LPR-030.1 | 无 | 恰好 2 次 `cancel`,顺序为 r1、r0 |
| LPR-030.2 | r0 已 `Aged`(在途清除) | 恰好 1 次 `cancel(r1)` |

### LPR-031 跨 gw 变体:leg 0 的单 `Aged` 后下一轮照常

- 规则:[quote 探测](README.md#quote-探测)
- 级别:unit
- 里程碑:M2
- 前置:`RL1`、`MV0`、`CH`;`PP0`;[`GX`];+0 leg 0 挂 r0,+50 ms Ack
- 步骤:
  1. +2_050 ms `Order{r0, Aged}`
  2. +2_100 ms 投递 I2 `Bbo`
- 期望:
  - 步骤 2:一次 `place`(I2,新的 leg 0 探测单)
- 实现:`strategies/latency-probe/tests/scenario_xgw.rs::lpr_031_leg0_aged_next_round`

### LPR-032 跨 gw 组的 leg 2 按普通 quote 腿处理

- 规则:[quote 探测](README.md#quote-探测)
- 级别:unit
- 里程碑:M2
- 前置:`RL1`、`MV0`、`CH`;`PP0`;[`GX3`](leg 2 = I1);leg 0 无在途
- 步骤:
  1. +0 投递 I1 `Bbo`(挂 r2);+50 ms `Order{r2, Acked}`
- 期望:
  - I1 `Bbo` 上一次 `place`(不要求 leg 0 在挂);`Acked` 事件恰好一次 `cancel(r2)`
- 实现:`strategies/latency-probe/tests/scenario_xgw.rs::lpr_032_leg2_plain`

### LPR-033 pair:登记 `PAIR_TICK`、到期进 `Due`、A 腿 BBO 触发第一腿

- 规则:[pair 探测](README.md#pair-探测)、[意图](README.md#意图)、[组](README.md#组)
- 级别:unit
- 里程碑:M2
- 前置:`RL1`、`MV0`、`CH`;`PP0` 改 `mode = pair`;[`GPS`](A = I1 现货、B = I2 永续)
- 步骤:
  1. +0 投递 I2 `Bbo`(该 group 的第一个事件)
  2. +1 ms 投递 `Timer{tag: 1, group: GPS}`
  3. +2 ms 投递 I1 `Bbo`
  4. +3 ms `Order{r1, Cancelled}`(IOC 无成交,视图净量仍为 0)
- 期望:
  - 步骤 1:无 `place`;一次 `timer_at(tsc_of(E0), 1, GPS)`
  - 步骤 2:无意图(阶段 `Due`)
  - 步骤 3:一次 `place`:`venue = V1`、`side = Buy`、`tif = Ioc`、`class = Open`、`flags = 0`、`inst = I1`、`group = GPS`、`px = 6_030_001`(6_000_001 × 1.005 向下到 tick)、`qty = 100_000`(两腿 `pair_lot` = 100_000)、`max_rest_ms = 0`;`link.some = 1`,`LegLink{group: GPS, role: First, tolerance: 0, hedge: HedgePlan{kind: OnFill, px_mode: Cross, on_fail: Retry, tif: Ioc, venue: V2, account: T1, side: Sell, reduce_only: 0, inst: I2, ratio_num: 1, ratio_den: 1, residual: TrimExcess, px_param: 5_000_000, max_slip: 5_000_000, limit_px: 0}}`
  - 步骤 4:一次 `timer_at(tsc_of(E0 + 10_002_000_000), 1, GPS)`;`PairNoFill` 计数 + 1(在下一次计数 Note 中 `v0 = 1`)
- 实现:`strategies/latency-probe/tests/scenario_pair.rs::lpr_033_tick_due_first_leg`

### LPR-034 pair 与 target 的开仓方向

- 规则:[组](README.md#组)、[pair 探测](README.md#pair-探测)、[参数 schema(`ProbeParams`)](README.md#参数-schemaprobeparams)
- 级别:table
- 里程碑:M2
- 前置:`RL1`、`MV0`、`CH`;`PP0` 改 `mode = pair` 与所列 `pair_side`;每个周期都在 `Due` 时触发第一腿,成交并平仓完成后回 `Idle`
- 实现:`strategies/latency-probe/tests/scenario_pair.rs::lpr_034_open_direction`

| 行 | group / `pair_side` | 期望(依次各次开仓的第一腿方向与价格;第二腿方向) |
|---|---|---|
| LPR-034.1 | `GPS` / auto | Buy I1 @ 6_030_001;B 卖 |
| LPR-034.2 | `GSP` / auto | Sell I2 @ 5_970_000;B(I1 现货)买 |
| LPR-034.3 | `GPP` / auto,连续 3 次开仓 | Buy I2 @ 6_030_010、Sell I2 @ 5_970_000、Buy I2 @ 6_030_010 |
| LPR-034.4 | `GPP` / buy_a,连续 2 次 | Buy、Buy |
| LPR-034.5 | `GPP` / sell_a,连续 2 次 | Sell、Sell |
| LPR-034.6 | `GPS` / buy_a | Buy I1 @ 6_030_001 |

### LPR-035 pair 单笔量须同时满足两腿最小名义并按 `pair_lot` 对齐

- 规则:[意图](README.md#意图)
- 级别:table
- 里程碑:M2
- 前置:`RL1`(只改所列 `min_notional`)、`MV0`、`CH`;`PP0` 改 `mode = pair`;[`GPS`];`Due` 时投递 I1 `Bbo`
- 实现:`strategies/latency-probe/tests/scenario_pair.rs::lpr_035_pair_qty`

| 行 | I2 的 `min_notional` | 期望第一腿 `qty` |
|---|---|---|
| LPR-035.1 | 5_000_000(5 USDT) | 100_000 |
| LPR-035.2 | 54_545_454 | 100_000(× 1.1 = 59.9999994 USDT ≤ 60 USDT) |
| LPR-035.3 | 60_000_000 | 200_000(需 ≥ 66 USDT,100_000 只有 60) |

### LPR-036 `Due` 时条件不满足则保持 `Due`、不开仓

- 规则:[pair 探测](README.md#pair-探测)
- 级别:table
- 里程碑:M2
- 前置:`RL1`、`MV0`、`CH`;`PP0` 改 `mode = pair`;[`GPS`] 已处于 `Due`;每行只做所列改动后投递 I1 `Bbo`
- 实现:`strategies/latency-probe/tests/scenario_pair.rs::lpr_036_due_conditions`

| 行 | 改动 | 期望 |
|---|---|---|
| LPR-036.1 | 无 | 1 次 `place`;阶段 `Opening` |
| LPR-036.2 | I2 连接静默 7 s | 无意图;`Due`;`Skipped` v1 + 1 |
| LPR-036.3 | I1 bid 6_000_000、ask 6_100_000 | 无意图;`Due`;`Skipped` v0 + 1 |
| LPR-036.4 | 视图 I1 净量 +1_000(1 lot) | 无意图;`Due` |
| LPR-036.5 | 视图 I1 净量 +999、I2 净量 −99_999(都 < 1 lot) | 1 次 `place` |
| LPR-036.6 | 本 cell `Throttle` halted | 无意图;`Due` |
| LPR-036.7 | `margin_level = L1` | 无意图;`Due` |
| LPR-036.8 | I2 已收到 `Notice{Delisting}` | 无意图;`Due` |
| LPR-036.9 | `enabled = false` | 无意图;`Due` |
| LPR-036.10 | 有效模式 `CloseOnly` | 无意图 |

### LPR-037 pair 阶段迁移表的每一行

- 规则:[pair 探测](README.md#pair-探测)、[事件处理](README.md#事件处理)
- 级别:table
- 里程碑:M2
- 前置:`RL1`、`MV0`、`CH`;`PP0` 改 `mode = pair`;[`GPS`];每行先驱动到所列阶段与视图,再投递所列事件(时刻记为 t);第一腿记 r1、平仓单记 r2
- 实现:`strategies/latency-probe/tests/scenario_pair.rs::lpr_037_phase_table`

| 行 | 阶段与视图 | 事件 | 期望 |
|---|---|---|---|
| LPR-037.1 | `Idle`(已登记 `PAIR_TICK`) | `Timer{1}` | `Due`;无意图 |
| LPR-037.2 | `Due` | I1 `Bbo`(条件满足) | 1 次 `place`(LPR-033 步骤 3 的字段);`Opening` |
| LPR-037.3 | `Opening` | `Order{r1, Cancelled}`,视图净量 0 | `Idle`;`PairNoFill` + 1;`PAIR_TICK` 登记于 `next_pair_ens` |
| LPR-037.4 | `Opening` | `Order{r1, Filled{d: 100_000, done: true}}` | `Holding`;`timer_at(tsc_of(t + 5_000 ms), 2, GPS)` |
| LPR-037.5 | `Holding`;A +100_000、B −100_000 | `Timer{2}` | 1 次 `place`(平仓链接,见 LPR-040);`Flattening(1)` |
| LPR-037.6 | `Holding`;净量 0 | `Timer{2}` | `Idle`;无 `place` |
| LPR-037.7 | `Holding`;A +100_000、B 0 | `Timer{2}` | 无 `place`;`timer_at(tsc_of(t + 5_000 ms), 2, GPS)`;`Holding` |
| LPR-037.8 | `Flattening(1)` | `Order{r2, Filled{done: true}}` | `timer_at(tsc_of(t + 5_000 ms), 2, GPS)`;`Flattening(1)` |
| LPR-037.9 | `Flattening(1)`;净量 0 | `Timer{2}` | `Idle` |
| LPR-037.10 | `Flattening(1)`;A +100_000、B −100_000 | `Timer{2}` | 1 次 `place`(平仓链接,`qty = 100_000`);`Flattening(2)` |
| LPR-037.11 | `Flattening(2)`;A +100_000、B −100_000 | `Timer{2}` | `Note{FlattenIncomplete, v0 = 120_000_050, v1 = 1}`(P1);无 `place`;`Blocked` |
| LPR-037.12 | `Blocked`;净量 0 | I2 `Bbo` | `Idle`;`PAIR_TICK` 登记 |
| LPR-037.13 | `Holding` | `Risk{HedgeBroken{GPS}}` | `Blocked`;无意图 |
| LPR-037.14 | `Opening` | `Exec{Broken(LinkBroken)}` | `Blocked`;无意图 |
| LPR-037.15 | `Blocked`;A +100_000、B 0 | `Exec{Done{cleared_by_engine: true}}` | 1 次 `place`(平仓链接,`qty = 100_000`);`Flattening(1)` |
| LPR-037.16 | `Blocked`;A +100_000、B 0 | `Timer{1}` | 无意图;`Blocked` |

### LPR-038 表外的(阶段, 事件)组合不改阶段、不产生意图

- 规则:[pair 探测](README.md#pair-探测)
- 级别:table
- 里程碑:M2
- 前置:同 LPR-037
- 实现:`strategies/latency-probe/tests/scenario_pair.rs::lpr_038_ignored_combinations`

| 行 | 阶段 | 事件 | 期望 |
|---|---|---|---|
| LPR-038.1 | `Idle`(已登记 `PAIR_TICK`) | I1 `Bbo` | 无意图;`Idle` |
| LPR-038.2 | `Holding` | `Timer{1}` | 无意图;`Holding` |
| LPR-038.3 | `Idle` | `Timer{2}` | 无意图;`Idle` |
| LPR-038.4 | `Due` | `Timer{2}` | 无意图;`Due` |
| LPR-038.5 | `Opening` | `Timer{1}` | 无意图;`Opening` |
| LPR-038.6 | `Holding` | I1 `Bbo` | 无意图;`Holding` |
| LPR-038.7 | `Flattening(1)` | `Timer{1}` | 无意图;`Flattening(1)` |

### LPR-039 第一腿被门控拒绝或只记录时回 `Idle`、不计 `PairNoFill`

- 规则:[事件处理](README.md#事件处理)、[pair 探测](README.md#pair-探测)
- 级别:table
- 里程碑:M2
- 前置:`RL1`、`MV0`、`CH`;`PP0` 改 `mode = pair`;[`GPS`];`Due` 时第一腿已发出,下一事件读到所列结果
- 实现:`strategies/latency-probe/tests/scenario_pair.rs::lpr_039_first_leg_gated`

| 行 | `Ctx::outcome` | 期望 |
|---|---|---|
| LPR-039.1 | `Rejected(LINK_STALE)` | `Idle`;`ProbeGated` + 1;`PairNoFill` 不变 |
| LPR-039.2 | `Shadowed` | `Idle`;`PairNoFill` 不变 |

### LPR-040 pair 平仓链接的字段

- 规则:[意图](README.md#意图)
- 级别:table
- 里程碑:M2
- 前置:`RL1`、`MV0`、`CH`;`PP0` 改 `mode = pair`;每行一个 group 处于 `Holding`,视图净量如所列,投递 `Timer{2}`
- 实现:`strategies/latency-probe/tests/scenario_pair.rs::lpr_040_close_link`

| 行 | group / 净量 | 期望平仓单 |
|---|---|---|
| LPR-040.1 | `GPS` / A(I1)+100_000、B(I2)−100_000 | I1 `Sell`、`Ioc`、`class = Reduce`、`flags = 0`(现货)、`px = 5_970_000`、`qty = 100_000`;`HedgePlan{side: Buy, reduce_only: 1, inst: I2, kind: OnFill, px_param: 5_000_000, max_slip: 5_000_000}` |
| LPR-040.2 | `GPP` / A(I2)+100_000、B(I3)−100_000 | I2 `Sell`、`Ioc`、`class = Reduce`、`flags = REDUCE_ONLY`、`px = 5_970_000`、`qty = 100_000`;`HedgePlan{side: Buy, reduce_only: 1, inst: I3}` |
| LPR-040.3 | `GSP` / A(I2)−100_000、B(I1)+100_000 | I2 `Buy`、`flags = REDUCE_ONLY`、`px = 6_030_010`、`qty = 100_000`;`HedgePlan{side: Sell, reduce_only: 0, inst: I1}`(B 是现货) |
| LPR-040.4 | `GPS` / A +99_900(基础币扣费后)、B −100_000 | I1 `Sell`、`qty = 99_000`(向零取整到 I1 的 lot) |

### LPR-041 target:到期设开仓目标,字段按意图表

- 规则:[target 探测](README.md#target-探测)、[意图](README.md#意图)
- 级别:unit
- 里程碑:M2
- 前置:`RL1`、`MV0`、`CH`;`PP0` 改 `mode = target`;[`GTS`](A = I1、B = I2);阶段 `Due`
- 步骤:
  1. +0 投递 I1 `Bbo`
- 期望:
  - 一次 `target`:`TargetIntent{account: T1, group: GTS, n_legs: 2, legs: [(0, 100_000), (1, −100_000)], reason: Rebalance, urgency: Normal, band: 500_000_000, exec: PairOnFill{slice: {depth_frac: 1_000_000_000, depth_levels: 1, max_cross: 5_000_000, max_chunk_notional: 120_000_020, min_chunk_notional: 0, cooldown_ms: 0}, first_leg: Fixed, first_leg_ix: 0, hedge_px_mode: Cross, hedge_cross: 5_000_000, hedge_max_slip: 5_000_000, ratio_num: 1, ratio_den: 1, fix_tolerance: 0, residual: TrimExcess}, tag: 0}`;无 `place`;阶段 `Opening`;`next_pair_ens = E0 + 60_000_000_000`
- 实现:`strategies/latency-probe/tests/scenario_target.rs::lpr_041_open_target_fields`

### LPR-042 target:设目标后的结果

- 规则:[target 探测](README.md#target-探测)、[Note 码表与报表字段](README.md#note-码表与报表字段)
- 级别:table
- 里程碑:M2
- 前置:同 LPR-041 步骤 1 之后(`Opening`);下一事件(+10 ms 的 I2 `Bbo`)读到所列结果
- 实现:`strategies/latency-probe/tests/scenario_target.rs::lpr_042_target_outcomes`

| 行 | `Ctx::outcome` | 期望 |
|---|---|---|
| LPR-042.1 | `TargetAccepted` | `Opening`;无 Note |
| LPR-042.2 | `TargetShadowed` | `Opening` |
| LPR-042.3 | `TargetRejected(BadLegs)` | `Note{TargetRejected, v0 = 2}`(P2);`Idle`;`PAIR_TICK` 登记 |
| LPR-042.4 | `TargetRejected(LotMismatch)` | `Note{TargetRejected, v0 = 10}`;`Blocked` |

### LPR-043 target:开仓到达 → 持有 → 0/0 退出 → 清除目标

- 规则:[target 探测](README.md#target-探测)、[pair 探测](README.md#pair-探测)
- 级别:unit
- 里程碑:M2
- 前置:LPR-042.1 之后
- 步骤:
  1. +1_000 ms `Exec{Done{cleared_by_engine: false}}`;视图 A +100_000、B −100_000
  2. +6_000 ms `Timer{2}`
  3. +7_000 ms `Exec{Done{cleared_by_engine: false}}`;视图净量 0
- 期望:
  - 步骤 1:`Holding`;`timer_at(tsc_of(E0 + 6_000_000_000), 2, GTS)`
  - 步骤 2:一次 `target`:`reason = Exit`、`legs = [(0, 0), (1, 0)]`、`exec` 与开仓目标逐字段相同、`tag = 0`;`Flattening(1)`
  - 步骤 3:一次 `clear_target(T1, GTS)`;`Idle`;`PAIR_TICK` 登记于 `E0 + 60_000_000_000`
- 实现:`strategies/latency-probe/tests/scenario_target.rs::lpr_043_target_cycle`

### LPR-044 target:开仓阶段 Stuck 改设 0/0 目标

- 规则:[target 探测](README.md#target-探测)
- 级别:unit
- 里程碑:M2
- 前置:LPR-042.1 之后
- 步骤:
  1. `Exec{Stuck(NoLiquidity)}`
- 期望:
  - `Note{TargetStuck, v0 = 3, v1 = 1}`(P2);一次 `target`(`reason = Exit`、0/0、`tag = 0`);`Flattening(1)`
- 实现:`strategies/latency-probe/tests/scenario_target.rs::lpr_044_stuck_opening`

### LPR-045 target:平仓阶段 Stuck 改 `tag` 重发,两次后 `Blocked`

- 规则:[target 探测](README.md#target-探测)、[Note 码表与报表字段](README.md#note-码表与报表字段)
- 级别:unit
- 里程碑:M2
- 前置:LPR-043 步骤 2 之后(`Flattening(1)`,0/0 目标 `tag = 0` 生效);视图 A +100_000、B −100_000
- 步骤:
  1. t1 投递 `Exec{Stuck(SliceFails)}`
  2. t1 + 5_000 ms 投递 `Timer{2}`
  3. 投递 `Exec{Stuck(SliceFails)}`
- 期望:
  - 步骤 1:`Note{TargetStuck, v0 = 5, v1 = 2}`;`timer_at(tsc_of(t1 + 5_000 ms), 2, GTS)`;无 `target`
  - 步骤 2:一次 `target`(0/0,`tag = 1`,其余字段与上一目标相同);`Flattening(2)`
  - 步骤 3:一次 `clear_target(T1, GTS)`;`Note{FlattenIncomplete, v0 = 120_000_050, v1 = 1}`;`Blocked`
- 实现:`strategies/latency-probe/tests/scenario_target.rs::lpr_045_stuck_closing_retry`

### LPR-046 target:平仓阶段 `Stuck(Shadow)` 与 `Stuck(Gated(_))` 只计数

- 规则:[target 探测](README.md#target-探测)
- 级别:table
- 里程碑:M2
- 前置:同 LPR-045 的前置
- 实现:`strategies/latency-probe/tests/scenario_target.rs::lpr_046_stuck_shadow_gated`

| 行 | 事件 | 期望 |
|---|---|---|
| LPR-046.1 | `Exec{Stuck(Shadow)}` | `Note{TargetStuck, v0 = 2, v1 = 2}`;无 `target`、无 `FLAT` 登记;仍 `Flattening(1)` |
| LPR-046.2 | `Exec{Stuck(Gated(CLOSE_ONLY))}` | `Note{TargetStuck, v0 = 1, v1 = 2}`;无 `target`、无 `FLAT` 登记;仍 `Flattening(1)` |

### LPR-047 target:有生效目标时 `PAIR_TICK` 不进 `Due`

- 规则:[target 探测](README.md#target-探测)
- 级别:unit
- 里程碑:M2
- 前置:`RL1`、`MV0`、`CH`;`PP0` 改 `mode = target`;[`GTS`] 处于 `Idle`;视图 `target(T1, GTS)` 为 `Some`(引擎目标)
- 步骤:
  1. +0 投递 `Timer{1}`;+1 ms 投递 I1 `Bbo`
- 期望:
  - 无 `target`;仍 `Idle`;事件末尾登记的 `PAIR_TICK` 在 `tsc_of(E0 + 60_000_000_000)`
- 实现:`strategies/latency-probe/tests/scenario_target.rs::lpr_047_active_target_blocks_due`

### LPR-048 target:Broken 时停组,解除后按实际净量退出

- 规则:[target 探测](README.md#target-探测)、[pair 探测](README.md#pair-探测)
- 级别:table
- 里程碑:M2
- 前置:LPR-042.1 之后;投递 `Exec{Broken(ReverseFailed)}`(进 `Blocked`)
- 实现:`strategies/latency-probe/tests/scenario_target.rs::lpr_048_broken_then_cleared`

| 行 | 之后的事件与视图 | 期望 |
|---|---|---|
| LPR-048.1 | `Timer{1}` | 无意图;`Blocked` |
| LPR-048.2 | `Exec{Done{cleared_by_engine: true}}`;A +100_000、B 0 | 一次 `target`(0/0,`reason = Exit`);`Flattening(1)` |
| LPR-048.3 | `Exec{Done{cleared_by_engine: true}}`;净量 0 | 无意图;`Idle` |

### LPR-049 模式降级与回到 Live

- 规则:[事件处理](README.md#事件处理)、[运行模式](README.md#运行模式)
- 级别:table
- 里程碑:M2
- 前置:`RL1`、`MV0`、`CH`;`PP0`(各行 group 的 `mode` 如所列)
- 实现:`strategies/latency-probe/tests/scenario_mode.rs::lpr_049_mode_events`

| 行 | 状态与事件 | 期望 |
|---|---|---|
| LPR-049.1 | [`GPS`] `Due`;`Mode{Live → CloseOnly}` | `Idle`;无意图 |
| LPR-049.2 | [`GTS`] `Opening`;`Mode{Live → Shadow}`,时刻 t | 一次 `clear_target(T1, GTS)`;`Holding`;`timer_at(tsc_of(t + 5_000 ms), 2, GTS)` |
| LPR-049.3 | [`GQ`] +0 已探测并撤完;+300 ms `Mode{CloseOnly → Live}`;+301 ms I2 `Bbo` | +301 ms 一次 `place`(`last_probe_ens` 已清零) |
| LPR-049.4 | [`GPS`] `Idle`、`next_pair_ens = E0 + 10_000 ms`、无 `PAIR_TICK`;+400 ms `Mode{Shadow → Live}` | 同一事件 `timer_at(tsc_of(E0 + 400_000_000), 1, GPS)` |
| LPR-049.5 | [`GPS`] `Holding`;`Mode{Live → CloseOnly}` | 无意图;`Holding`(平仓照常,`FLAT` 到期仍发平仓链接) |

### LPR-050 外部成交:平掉非零腿并停探测到净量归零

- 规则:[事件处理](README.md#事件处理)、[意图](README.md#意图)
- 级别:table
- 里程碑:M2
- 前置:`RL1`、`MV0`、`CH`;`PP0`(group 的 `mode` 如所列)
- 实现:`strategies/latency-probe/tests/scenario_external.rs::lpr_050_external`

| 行 | 状态、事件与视图 | 期望 |
|---|---|---|
| LPR-050.1 | [`GQ`];`External{I2, cause: Liquidation}`,视图 I2 +100_000;之后 +2_000 ms 健康 `Bbo` | 一次 `place`:I2 `Sell`、`Ioc`、`class = Flatten`、`flags = REDUCE_ONLY`、`qty = 100_000`;+2_000 ms 无 `place` |
| LPR-050.2 | 同 .1,再把视图 I2 净量置 0;+3_000 ms 健康 `Bbo` | +3_000 ms 一次 `place`(探测单) |
| LPR-050.3 | [`GPS`] `Holding`;`External{I2, cause: Adl}`,视图 A +100_000、B 0 | 一次 `place`:I1 `Sell`、`Ioc`、`class = Flatten`、`flags = 0`、`qty = 100_000`;`Blocked` |
| LPR-050.4 | 同 .3 但 `cause: Manual` | 意图与 .3 相同 |

### LPR-051 交易所通知暂停探测

- 规则:[事件处理](README.md#事件处理)
- 级别:table
- 里程碑:M2
- 前置:`RL1`、`MV0`、`CH`;`PP0`;U = 1_759_708_800_000_000_000(UTC ns),`Maintenance{from_ns: U + 1_000_000_000, to_ns: U + 2_000_000_000}`
- 实现:`strategies/latency-probe/tests/scenario_external.rs::lpr_051_notices`

| 行 | 状态与事件 | 期望 |
|---|---|---|
| LPR-051.1 | [`GQ`];`Notice{I2, Delisting}`;之后每秒一条健康 `Bbo` 共 10 条 | 全部无 `place` |
| LPR-051.2 | [quote 组 legs [I2, I3]];`Notice{V2, INST_ALL, Delisting}`;两腿各一条到期 `Bbo` | I2 无 `place`;I3 一次 `place` |
| LPR-051.3 | [`GQ`];收到上述 `Maintenance`;到期 `Bbo` 时 `now.utc_ns` = U + 999_999_999 | 一次 `place` |
| LPR-051.4 | 同上,`now.utc_ns` = U + 1_000_000_000 | 无 `place` |
| LPR-051.5 | 同上,`now.utc_ns` = U + 2_000_000_000 | 无 `place` |
| LPR-051.6 | 同上,`now.utc_ns` = U + 2_000_000_001 | 一次 `place` |
| LPR-051.7 | [`GPS`] `Due`;I2 处于维护窗口内;I1 `Bbo` | 无意图;`Due` |

### LPR-052 残余名义超过上限时平仓并请求 Halted

- 规则:[quote 探测](README.md#quote-探测)、[参数 schema(`ProbeParams`)](README.md#参数-schemaprobeparams)、[Note 码表与报表字段](README.md#note-码表与报表字段)
- 级别:table
- 里程碑:M2
- 前置:`RL1`、`MV0`、`CH`;`PP0` 改所列 `max_inv_notional`(与 `mode`)
- 实现:`strategies/latency-probe/tests/scenario_external.rs::lpr_052_residual_cap`

| 行 | `max_inv_notional` / 状态与事件 | 期望 |
|---|---|---|
| LPR-052.1 | 60_000_050 / [`GQ`] 探测单意外成交 100_000(视图 I2 +100_000,mid 6_000_005) | `ProbeFilled` 与一次残余平仓 `place`;无 `request_halt` |
| LPR-052.2 | 60_000_049 / 同上 | 恰好一次残余平仓 `place`(I2 `Sell` `qty = 100_000`);`request_halt(GQ, 2)`;`Note{ProbeHalted, v0 = 2, v1 = 60_000_050}` |
| LPR-052.3 | 1_000_000 / [`GPS`] `Holding`,A +100_000、B −100_000,`Timer{2}` | 平仓链接 `place`;无 `request_halt`(两腿相抵,残余 0) |
| LPR-052.4 | 5_999_999 / [`GPS`] `Holding`,A +100_000、B −90_000(无在途),`Timer{2}` | `request_halt(GPS, 2)`;`Note{ProbeHalted, v0 = 2, v1 = 6_000_000}`;恰好一次 `place`:I1 `Sell`、`class = Flatten`、`qty = 100_000`(B 的 90_000 向零取整到 lot 为 0,不发;本事件不发平仓链接);`Blocked` |
| LPR-052.5 | 50_000_000 / [`GPS`] `Opening`,第一腿 `Filled{d: 100_000, done: true}`;视图 A +100_000、B 净量 0 且即时在途 −100_000(OMS 第二腿) | 无 `request_halt`(残余 0);`Holding` |

### LPR-053 残余超限的 Halted 请求不重复,解除后可再触发

- 规则:[quote 探测](README.md#quote-探测)、[事件处理](README.md#事件处理)
- 级别:unit
- 里程碑:M2
- 前置:LPR-052.2 之后(`Throttle` 已 halted)
- 步骤:
  1. 再投递一次意外成交(视图 I2 +200_000)
  2. 投递本 cell 的 `Risk{CapCleared{Halted}}`,再投递一次意外成交(视图 I2 +300_000)
- 期望:
  - 步骤 1:恰好一次残余平仓 `place`(`qty = 200_000`);无 `request_halt`、无 `ProbeHalted`
  - 步骤 2:恰好一次残余平仓 `place`(`qty = 300_000`);一次 `request_halt(GQ, 2)` 与 `ProbeHalted{v0 = 2, v1 = 180_000_150}`
- 实现:`strategies/latency-probe/tests/scenario_external.rs::lpr_053_halt_not_repeated`

### LPR-054 参数变化:新 GroupId 从空起步,`mode` 变化先平残余

- 规则:[状态(`ProbeGroup`,≤ 256 B)](README.md#状态probegroup-256-b)
- 级别:table
- 里程碑:M2
- 前置:`RL1`、`MV0`、`CH`;`PP0`
- 实现:`strategies/latency-probe/tests/scenario_params.rs::lpr_054_on_params_reset`

| 行 | 状态与 `on_params` | 之后的事件 | 期望 |
|---|---|---|---|
| LPR-054.1 | [`GQ`] +0 已探测并撤完;同一 GroupId、同一指纹的新参数 | +500 ms `Bbo` | 无 `place`(间隔保留) |
| LPR-054.2 | 同上,但换成新 GroupId(腿相同) | +500 ms `Bbo` | 一次 `place`(从空状态起步) |
| LPR-054.3 | [`GPS`] `Holding`,A +100_000、B −100_000;`mode` 改为 target | +600 ms I2 `Bbo` | 两次 `place`:I1 `Sell` `qty = 100_000`(Flatten)、I2 `Buy` `qty = 100_000`(Flatten,`REDUCE_ONLY`);阶段 `Idle`;`PAIR_TICK` 登记 |

### LPR-055 恢复后 pair/target 组的第一个事件

- 规则:[状态(`ProbeGroup`,≤ 256 B)](README.md#状态probegroup-256-b)
- 级别:table
- 里程碑:M2
- 前置:`RL1`、`MV0`、`CH`;`PP0`;每行在所列阶段 `snapshot` → `MockViews::restart` → `restore`,视图净量如所列,再投递一条 I2 `Bbo`(时刻 t)
- 实现:`strategies/latency-probe/tests/scenario_params.rs::lpr_055_restore_pair_target`

| 行 | group / 快照时阶段 / 净量 | 期望 |
|---|---|---|
| LPR-055.1 | `GPS` / `Due` / 0 | `Idle`;`PAIR_TICK` 登记 |
| LPR-055.2 | `GPS` / `Opening` / 0 | `Idle` |
| LPR-055.3 | `GPS` / `Holding` / A +100_000、B −100_000 | `Holding`;`timer_at(tsc_of(t + 5_000 ms), 2, GPS)` |
| LPR-055.4 | `GPS` / `Blocked` / A +100_000、B 0 | `Blocked`;无意图 |
| LPR-055.5 | `GTS` / `Flattening(1)`(0/0 目标生效)/ A +100_000、B −100_000 | 一次 `clear_target(T1, GTS)`;`Holding`;`FLAT` 登记 |
| LPR-055.6 | `GTS` / `Opening` / 0 | 一次 `clear_target(T1, GTS)`;`Idle` |

### LPR-056 计数类 Note 每 group 每 10 min 至多一条

- 规则:[Note 码表与报表字段](README.md#note-码表与报表字段)
- 级别:unit
- 里程碑:M2
- 前置:`RL1`、`MV0`、`CH`;`PP0`;[`GQ`];group 的第一个事件在 +0(`note_due_ens` = +600_000 ms)
- 步骤:
  1. +1 s 至 +100 s 内:3 次到期时报价不健康、2 次门控拒绝(`Rejected(RATE)`)、1 次交易所拒单(`Rejected{bits: 0, venue_code: 2010}`)
  2. +599_999 ms 投递一条 `Bbo`
  3. +600_000 ms 投递一条 `Bbo`
  4. +1_200_000 ms 投递一条 `Bbo`(期间无新计数)
- 期望:
  - 步骤 2:没有计数类 Note
  - 步骤 3:恰好 3 条 Note:`Skipped{v0 = 0, v1 = 3}`、`ProbeGated{v0 = 2, v1 = RATE}`、`ProbeRejected{v0 = 1, v1 = 0, v2 = 2010}`,各自 `group = GQ`、级别 Info;`PairNoFill` 不写(计数为 0)
  - 步骤 4:没有计数类 Note
- 实现:`strategies/latency-probe/tests/scenario_notes.rs::lpr_056_count_note_cadence`

### LPR-057 每个登记的 Note 码至少触发一次,且都已登记

- 规则:[Note 码表与报表字段](README.md#note-码表与报表字段)、[事件处理](README.md#事件处理)
- 级别:unit
- 里程碑:M2
- 前置:`RL1`、`MV0`;每一步在新实例与新的 `CH`(记录 Note 时断言码与级别已在 `DESC.notes` 登记)上,按所引用用例的前置重放
- 步骤:
  1. 依次重放 LPR-016 步骤 3(码 1)、LPR-056 步骤 1–3(码 2、3、5)、LPR-033 之后推进到 +600_000 ms 的一条事件(码 4)、LPR-037.11(码 6)、LPR-019.2(码 7)、LPR-044(码 8)、LPR-042.3(码 9)
- 期望:
  - 全部重放合计,码 1–9 各至少出现一次,级别分别为 P2、Info、Info、Info、Info、P1、P1、P2、P2;各 `CH` 都没有断言失败;没有码 0 或 ≥ 10 的 Note
- 实现:`strategies/latency-probe/tests/scenario_notes.rs::lpr_057_all_codes_registered`

### LPR-058 任意事件序列下每条腿同时至多一张在途探测单

- 规则:[quote 探测](README.md#quote-探测)
- 级别:prop
- 里程碑:M2
- 性质:quote 组(含 `quote_xgw`)在任意事件序列下,每条腿已发出而未收到终态(`Cancelled`、`Aged`、`Rejected`、`done` 的 `Filled`、门控结果)的探测单 ≤ 1
- 生成:1–4 条腿;事件 0–2_000 个,从 {到期/未到期 `Bbo`、`Acked`、`Cancelled`、`Aged`、`Rejected`(两类)、`Unknown`、`Filled`(部分/全部)、重复上一条回报、门控结果、`Mode`、`restart`} 抽取,回报随机延迟与乱序;参数在合法范围内随机
- 参考:按 `OrderRef` 跟踪的在途表
- 例数:PR 2 000;每夜 1 000 000
- 实现:`strategies/latency-probe/tests/props.rs::lpr_058_one_inflight_per_leg`

### LPR-059 任意时长 T 内单腿挂单数不超过 ⌈T / 周期⌉ + 1

- 规则:[quote 探测](README.md#quote-探测)
- 级别:prop
- 里程碑:M2
- 性质:对任意事件序列与任意长度 T 的时间窗,单腿 PostOnly 数 ≤ ⌈T / P⌉ + 1,P = min(`probe_period_ms`, 非 0 的 `burst_period_ms`);拒单(含交易所拒单)之后同一腿下一次挂单与上一次的间隔仍 ≥ P
- 生成:同 LPR-058 但不含 `Mode`(回到 `Live` 按事件处理表清零 `last_probe_ens`,不受本界约束);`Bbo` 间隔 0–500 ms,其中 30% 与上一条同 `rx_tsc`;`burst_period_ms` ∈ {0} ∪ [1, `probe_period_ms`]
- 参考:滑窗计数
- 例数:PR 2 000;每夜 1 000 000
- 实现:`strategies/latency-probe/tests/props.rs::lpr_059_rate_bound`

### LPR-060 quote 价格永不越过对手价

- 规则:[意图](README.md#意图)
- 级别:prop
- 里程碑:M2
- 性质:每张 quote 探测单在发出时:买单 `px < ask`、卖单 `px > bid`,且 `px` 为 tick 整数倍
- 生成:bid、ask ∈ [1, 10^10](ask ≥ bid,价差 ≤ `max_spread`);tick ∈ {1, 10, 100};`far_bp` ∈ [5_000_000, 100_000_000]
- 参考:上述不等式
- 例数:PR 10 000;每夜 1 000 000
- 实现:`strategies/latency-probe/tests/props.rs::lpr_060_never_crosses`

### LPR-061 `enabled = false` 后不再出现 Open 意图与增仓目标

- 规则:[参数 schema(`ProbeParams`)](README.md#参数-schemaprobeparams)
- 级别:prop
- 里程碑:M2
- 性质:任意事件序列中某一步 `on_params` 改 `enabled = false` 之后,`place` 的 `class` 只有 `Reduce`/`Flatten`,`target` 只有 `reason = Exit` 或各腿 |目标| 不大于当时实际净量
- 生成:三种模式的 group 各 0–2 个;事件 0–2_000 个;关闭时刻随机
- 参考:上述断言
- 例数:PR 2 000;每夜 1 000 000
- 实现:`strategies/latency-probe/tests/props.rs::lpr_061_disabled_no_open`

### LPR-062 target 组只在没有生效目标时设开仓目标

- 规则:[target 探测](README.md#target-探测)
- 级别:prop
- 里程碑:M2
- 性质:任意事件序列下,每次 `reason = Rebalance` 的 `target` 发出时 `v.target(a, g)` 为 `None`;同一 group 在一次 `on_event` 内至多发一个 `target`
- 生成:target 组 1–2 个;事件从 {`Timer{1}`、`Timer{2}`、A 腿 `Bbo`、`Exec` 各变体、目标结果、`Risk{HedgeBroken}`、`Mode`、`restart`} 抽取 0–1_000 个;视图的生效目标按发出的 `target`/`clear_target` 与引擎清除随机维护
- 参考:上述断言
- 例数:PR 2 000;每夜 1 000 000
- 实现:`strategies/latency-probe/tests/props.rs::lpr_062_single_active_target`

### LPR-063 快速部分零分配,每 group 状态不超过 256 B

- 规则:[延迟与资源契约](README.md#延迟与资源契约)、[状态(`ProbeGroup`,≤ 256 B)](README.md#状态probegroup-256-b)
- 级别:unit
- 里程碑:M2
- 前置:测试二进制装计数分配器;三个实例分别以 [`GQ`]、[`GPP`](mode pair)、[`GTS`](mode target)创建,`new` 完成后清零计数
- 步骤:
  1. 对每种 `EventRef` 变体各投递 1_000 次(含触发挂单、撤单、目标与 Note 的情形)
  2. 取 `size_of::<ProbeGroup>()`
- 期望:
  - 步骤 1 分配次数 0
  - 步骤 2 ≤ 256
- 实现:`strategies/latency-probe/tests/alloc.rs::lpr_063_zero_alloc_state_size`

### LPR-064 依赖、名字与浮点的机检

- 规则:[依赖](README.md#依赖)、[规划源文件](README.md#规划源文件)、[概要](README.md#概要)
- 级别:table
- 里程碑:M2
- 前置:`tools/xtask/tests/fixtures/` 下的小型假仓库;每行在干净副本上只做一处改动,运行所列检查
- 实现:`tools/xtask/tests/deps_latency_probe.rs::lpr_064_machine_checks`

| 行 | 改动 | 检查 | 期望 |
|---|---|---|---|
| LPR-064.1 | 不改 | `cargo xtask deps`、`doc-check`、`hot-check` | 都退出 0 |
| LPR-064.2 | `strategies/latency-probe` 依赖 `hft-oms` | `cargo xtask deps` | 退出 1 |
| LPR-064.3 | 依赖另一个策略 crate | `cargo xtask deps` | 退出 1 |
| LPR-064.4 | 依赖 `hft-types`(L0 应经 SDK) | `cargo xtask deps` | 退出 1 |
| LPR-064.5 | README 正文加入一个交易所名 | `cargo xtask doc-check` | 退出 1,报"策略 README 出现交易所名" |
| LPR-064.6 | `src/fast/quote.rs` 加 `let x: f64 = 0.0;` | `cargo xtask hot-check` | 退出 1(S1) |
| LPR-064.7 | `src/fast/pair.rs` 调用 `std::time::Instant::now()` | `cargo xtask ci`(clippy) | 失败 |

### LPR-065 引擎级:1 h quote 运行的样本数与撤单

- 规则:[产生的延迟样本](README.md#产生的延迟样本)、[quote 探测](README.md#quote-探测)、[用途与运行要求](README.md#用途与运行要求)
- 级别:integ
- 里程碑:M2
- 前置:`strategies/registry/tests/latency_probe_scenarios.rs`;registry 的 `Strategies` 实例化 hft-testkit 场景;`RL1` 的 T1 为 Live 测试网账户、V2 由 `MockVenue`(hft-sim `SimExchange`)模拟,PostOnly 远离对手价不成交;`RiskParams`:I2 的 `max_px_dev = 50_000_000`(≥ `far_bp`)、令牌桶足够;`PP0`;[`GQ`];脚本行情:I2 的 `Bbo` 在 t = 0、100、200 … ms 各一条,共 1 h;另有 5 段各 3 s 无 `Bbo`,每段从某次探测之后 50 ms 开始、段间相隔 ≥ 10 min;`MockVenue` 的 Ack 与撤单回报延迟 5 ms
- 步骤:
  1. 运行 1 h 脚本
- 期望:
  - 设本策略 `place` 的探测单数为 N:`LatencySample{action = New}` 中 `venue = V2` 的条数 = N,`trigger_kind` 都是 Bbo(0x0101);`LatencySample{action = Cancel}` 条数 = N,每条的 `trigger_kind` 都是 OrderEvent(0x0203)且触发记录是同一订单的 Ack;成交 0 笔;每张探测单都以 `Cancelled` 终结(无 `Aged`)
  - N = 3_590(每段空白使之后的探测顺延 100 ms、少 2 张)
- 实现:`strategies/registry/tests/latency_probe_scenarios.rs::lpr_065_quote_hour`

### LPR-066 引擎级:跨 gw 变体的样本

- 规则:[产生的延迟样本](README.md#产生的延迟样本)、[quote 探测](README.md#quote-探测)
- 级别:integ
- 里程碑:M2
- 前置:同 LPR-065,但 [`GX`](leg 0 = I2 @ V2、leg 1 = I3 @ V3),V2 与 V3 各一个 `MockVenue`;两腿 `Bbo` 都每 100 ms 一条,10 min,无空白
- 步骤:
  1. 运行 10 min
- 期望:
  - 每条 leg 1 的 `Acked` 所在 Input 中有 2 条撤单的 `OrderDecision`(先 V3、后 V2)
  - 跨 gw 样本数(`action = Cancel`、`venue = V2`、触发记录为 V3 的回报)= 被撤的 leg 0 探测单数 = leg 0 探测单总数;leg 0 没有同 gw 的撤单样本
- 实现:`strategies/registry/tests/latency_probe_scenarios.rs::lpr_066_xgw_samples`

### LPR-067 引擎级:pair 开仓 → 第二腿 → 平仓

- 规则:[pair 探测](README.md#pair-探测)、[产生的延迟样本](README.md#产生的延迟样本)
- 级别:integ
- 里程碑:M2
- 前置:同 LPR-065 的装配;[`GPP`](mode pair);两个 `MockVenue` 的盘口可成交;`RiskParams` 的 `ioc_max_cross ≥ pair_cross`;运行 5 个 `pair_period_ms`
- 步骤:
  1. 运行脚本
- 期望:
  - 每个成交的第一腿都有一条 `LatencySample{link_role = 2(Second)}`(第二腿,`hedge_host` 的数据);每个开仓之后 `pair_flat_ms` 内发出平仓链接;运行结束时两腿净量都为 0;无 `Broken`
- 实现:`strategies/registry/tests/latency_probe_scenarios.rs::lpr_067_pair_cycle`

### LPR-068 引擎级:target 开仓经执行层切片与对冲,0/0 退出

- 规则:[target 探测](README.md#target-探测)、[产生的延迟样本](README.md#产生的延迟样本)
- 级别:integ
- 里程碑:M2
- 前置:同 LPR-067,[`GTS`](mode target,A = I1、B = I3)
- 步骤:
  1. 运行 3 个 `target_period_ms`
- 期望:
  - 每个开仓目标都有执行层切片 `ExecDecision` 与第二腿 `LatencySample{link_role = 2}`;每个开仓之后有 `reason = Exit` 的 0/0 目标并到达(`Exec{Done}`);结束时两腿净量为 0,无生效目标
- 实现:`strategies/registry/tests/latency_probe_scenarios.rs::lpr_068_target_cycle`

### LPR-069 引擎级:第二腿被拒走执行层 Broken 路径,本策略停开仓

- 规则:[pair 探测](README.md#pair-探测)、[事件处理](README.md#事件处理)
- 级别:integ
- 里程碑:M2
- 前置:同 LPR-067;B 所在 `MockVenue` 拒绝全部新单
- 步骤:
  1. 运行到第一腿成交后 3 个 `pair_period_ms`
- 期望:
  - 该 cell 进入 Broken(`Exec{Broken}` 投递给本策略);之后本策略没有新的第一腿与平仓链接(`Blocked`);执行层的自动有界平仓照常(`origin = Engine(AutoFix)`)
- 实现:`strategies/registry/tests/latency_probe_scenarios.rs::lpr_069_second_leg_rejected`

### LPR-070 引擎级:价格带小于 `far_bp` 时每张探测单都被门控拒绝

- 规则:[用途与运行要求](README.md#用途与运行要求)
- 级别:integ
- 里程碑:M2
- 前置:同 LPR-065(无空白),但 I2 的 `max_px_dev = 20_000_000`(< `far_bp` 30_000_000);运行到 t = 600_000 ms 那条 `Bbo`(含)
- 步骤:
  1. 运行脚本
- 期望:
  - t = 0 … 599 s 各一次探测意图(共 600 次),每次的 `OrderDecision.reject` 含 `PX_BAND`、不出环;没有 `LatencySample`;t = 600 s 的事件先写 `ProbeGated{v0 = 600, v1 = PX_BAND}`,再发第 601 次意图
- 实现:`strategies/registry/tests/latency_probe_scenarios.rs::lpr_070_px_band_gated`

### LPR-071 DST:故障注入下在途、残余与开仓约束成立

- 规则:[quote 探测](README.md#quote-探测)、[pair 探测](README.md#pair-探测)、[target 探测](README.md#target-探测)、[事件处理](README.md#事件处理)
- 级别:dst
- 里程碑:M2
- 前置:`strategies/registry/tests/latency_probe_dst.rs`;`RL1` 另加 ETH 的 `I5`(V2 永续)、`I6`(V3 永续)与 SOL 的 `I7`(V2 永续)、`I8`(V3 永续);GroupTable [`GQ`(I2), quote_xgw 组 (I5, I6), pair 组 (I7, I8), target 组 (I1, I3)];`PP0`
- 故障:Ack 丢失与 `Unknown`、撤单被拒(已成交)、行情与下单连接断开重连、限速 429、部分成交、第二腿被拒、切片 `Stuck`、任意时刻崩溃后重启(hft-testkit 故障目录对应各项)
- 种子:PR 200(提交派生)+ 回归集;每夜日期派生(`cargo xtask dst --strategy latency-probe`)
- 断言:每腿在途探测单 ≤ 1;每个 group 的残余名义(quote 第 6 步口径)≤ `max_inv_notional` + 1 lot 名义;`enabled = false` 与有效模式 `CloseOnly` 期间无 Open 意图与增仓目标;交易所拒单后同一腿下一次挂单的间隔 ≥ `probe_period_ms`;INV 由 hft-testkit 的不变量集每步检查
- 实现:`strategies/registry/tests/latency_probe_dst.rs::lpr_071_dst_suite`

### LPR-072 基准:`on_event` 快侧未命中

- 规则:[延迟与资源契约](README.md#延迟与资源契约)
- 级别:bench
- 里程碑:M2
- 基准名:`strategies/latency-probe/tick_miss`
- 实现:`strategies/latency-probe/benches/on_event.rs::lpr_072_tick_miss`

### LPR-073 基准:`on_event` 命中产生一条订单意图

- 规则:[延迟与资源契约](README.md#延迟与资源契约)
- 级别:bench
- 里程碑:M2
- 基准名:`strategies/latency-probe/tick_fire`
- 实现:`strategies/latency-probe/benches/on_event.rs::lpr_073_tick_fire`

### LPR-074 基准:`Order{Acked}` → 撤单

- 规则:[延迟与资源契约](README.md#延迟与资源契约)
- 级别:bench
- 里程碑:M2
- 基准名:`strategies/latency-probe/ack_cancel`
- 实现:`strategies/latency-probe/benches/on_event.rs::lpr_074_ack_cancel`

### LPR-075 基准:构造 `TargetIntent`

- 规则:[延迟与资源契约](README.md#延迟与资源契约)
- 级别:bench
- 里程碑:M2
- 基准名:`strategies/latency-probe/target_set`
- 实现:`strategies/latency-probe/benches/on_event.rs::lpr_075_target_set`

### LPR-076 基准:`on_event(Exec)`

- 规则:[延迟与资源契约](README.md#延迟与资源契约)
- 级别:bench
- 里程碑:M2
- 基准名:`strategies/latency-probe/exec_update`
- 实现:`strategies/latency-probe/benches/on_event.rs::lpr_076_exec_update`

### LPR-077 基准:`on_event(Risk)`

- 规则:[延迟与资源契约](README.md#延迟与资源契约)
- 级别:bench
- 里程碑:M2
- 基准名:`strategies/latency-probe/risk_event`
- 实现:`strategies/latency-probe/benches/on_event.rs::lpr_077_risk_event`

### LPR-078 测试网:quote 模式每个 core venue 连续 24 h

- 规则:[用途与运行要求](README.md#用途与运行要求)、[产生的延迟样本](README.md#产生的延迟样本)
- 级别:testnet
- 里程碑:M3
- 环境:按需租用的 `c8g.4xlarge`(或 dev2 联调窗口),`prod` 角色实例 `order_env = "testnet"`、`feed_env` 为主网;每个 core venue 至少一个 quote 探测点(测试网账户,`is_prod = false`),另有一个 `quote_xgw` 组
- 授权:`[需授权]`(租用实例与测试网 key 白名单由负责人执行)
- 步骤:
  1. `SetParams{enabled = true}` → `SetMode{Live}`,连续运行 24 h
  2. 用 `replayer latency` 统计各探测点的样本
- 通过标准:每个探测点 `tick_to_trade` 样本 ≥ 80,000(HW 与 SW 数据集分开计数);`LatencySample{action = Cancel}` 条数 = 被撤探测单数;≥ 99% 的探测单在 Ack 后被撤,其余逐条可解释;成交为 0 或逐条可解释;`quote_xgw` 组有跨 gw 的 `rep_to_order` 样本
- 证据:`ops-log/tests/LPR-078-<UTC 日期>.md`(含每探测点计数表与 `replayer latency` 输出)
- 实现:`strategies/latency-probe/tests/host/LPR-078.sh`

### LPR-079 测试网:pair 与 target 模式各一组跨所腿运行 ≥ 1 天

- 规则:[用途与运行要求](README.md#用途与运行要求)、[pair 探测](README.md#pair-探测)、[target 探测](README.md#target-探测)
- 级别:testnet
- 里程碑:M6
- 环境:dev2 测试网联调窗口(`hft-window start`),`prod` 角色实例 `feed_env = "testnet"`、`order_env = "testnet"`
- 授权:无
- 步骤:
  1. 一组 pair、一组 target(跨所两腿)运行 ≥ 1 天
- 通过标准:每个成交的第一腿都有 `hedge_host` 样本(`LatencySample{link_role = 2}`);日终两腿净量为 0;`FlattenIncomplete` 为 0 或逐条可解释
- 证据:`ops-log/tests/LPR-079-<UTC 日期>.md`
- 实现:`strategies/latency-probe/tests/host/LPR-079.sh`

### LPR-080 每张 quote 探测单的 journal 量实测

- 规则:[延迟与资源契约](README.md#延迟与资源契约)
- 级别:host
- 里程碑:M3
- 环境:LPR-078 的实例,第一天
- 授权:无
- 步骤:
  1. 运行 1 h 后,取该时段 seq journal 字节数与探测单数
- 通过标准:得到"每张探测单的 journal 字节数"与"每个探测点每天的 seq journal 量"两个实测值,写回 README"延迟与资源契约"替换待实测的估算(≈ 1.2 KB、≈ 100 MB/天)
- 证据:`ops-log/tests/LPR-080-<UTC 日期>.md`
- 实现:`strategies/latency-probe/tests/host/LPR-080.sh`

### LPR-081 测试网交易所事实核实:PostOnly 拒单码、限速、价格带与最小名义

- 规则:[开发备忘](README.md#开发备忘)、[用途与运行要求](README.md#用途与运行要求)
- 级别:testnet
- 里程碑:M3
- 环境:LPR-078 的实例
- 授权:`[需授权]`(测试网 key)
- 步骤:
  1. 在每个 core venue 测试网分别下:穿价 PostOnly、离对手价 300 bp 的 PostOnly、低于最小名义的单、按 1/s 与 10/s 的挂撤
- 通过标准:每所的 PostOnly 穿价拒单码、下单/撤单限速、是否允许 300 bp 外挂单、最小名义都有实测结论,写入对应 `venue-*` README 与 `VenueCaps`;必要时调整本策略默认值并记 `docs/changes.md`
- 证据:`ops-log/tests/LPR-081-<UTC 日期>.md`
- 实现:`strategies/latency-probe/tests/host/LPR-081.sh`

### LPR-082 基准:`on_event` 慢侧更新(pair 组 B 腿 `Bbo`)

- 规则:[延迟与资源契约](README.md#延迟与资源契约)
- 级别:bench
- 里程碑:M2
- 基准名:`strategies/latency-probe/slow_update`
- 实现:`strategies/latency-probe/benches/on_event.rs::lpr_082_slow_update`

### LPR-083 基准:`on_event(Advice)`(忽略)

- 规则:[延迟与资源契约](README.md#延迟与资源契约)
- 级别:bench
- 里程碑:M2
- 基准名:`strategies/latency-probe/advice`
- 实现:`strategies/latency-probe/benches/on_event.rs::lpr_083_advice`

## 验收

- 里程碑已到的用例全部有同编号的实现且通过(`cargo xtask test-trace`,规则见 [docs/tests §6](../../docs/tests/README.md#6-与代码的对应));单元、属性、引擎级与 DST 用例在 M2 完成。
- 属性测试:LPR-058–LPR-062 在 CI 每夜各跑 1 000 000 例(运行矩阵归 [testing §6](../../docs/testing.md#6-ci-运行矩阵))。
- DST:LPR-071 的种子数与回归集按 [testing §3.4](../../docs/testing.md#34-dst) 运行;以生产策略运行的 DST 不计入门槛 ④ 的累计(hft-testkit)。
- 基准:LPR-072–LPR-077、LPR-082、LPR-083 登记在 `benches/budgets.toml`;数值上限只在 README"延迟与资源契约"与 [benchmarks](../../benchmarks/README.md) 登记,参考机上由 `cargo xtask bench-gate --reference` 判定。
- 功能验收:LPR-078 在 M3(M3 末满订阅探针之前)通过;LPR-079 在 M6 联调窗口通过;⑩ 的判定用哪些样本、要多少归 [testing §8](../../docs/testing.md#8-上线门槛)。
- 绑定约束:`non_prod_only` 的绑定拒绝由 [registry](../registry/TESTS.md) 的 REG-012、REG-013 与 hft-core 的装配用例(退出 2 `ProbeOnProdAccount`)覆盖。
