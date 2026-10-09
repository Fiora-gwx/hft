---
description: "延迟探针策略 LatencyProbe:只在非正式账户(交易所测试网/模拟盘)上运行;quote 模式按 BBO 事件限速挂远离对手价的最小名义 PostOnly、Ack 后同一事件撤单,在主网行情负载下产生 tick_to_trade 与 rep_to_order 样本;pair 模式(两所最小名义 IOC + LegLink 第二腿)与 target 模式(TargetIntent → hft-exec 切片)在测试网行情时段做小样本确认;为 M1.5 延迟探针、M3 末满订阅探针与上线门槛 ⑩ 产生主机内延迟样本"
kind: crate
tier: hot
scope: core
status: spec
---

# latency-probe

## 概要

`LatencyProbe` 是一个**只产生测量样本**的策略:它不追求收益,只让真实的下单路径(BBO 触发 → 决策 → 出环 → 交易所回报;回报 → 由它触发的下一单出环;第一腿成交 → 第二腿出环;目标意图 → 执行层切片出环)按固定节奏跑起来,使 `tick_to_trade`、`rep_to_order`、`egress`、`order_rtt`、`hedge_host`、`leg_gap` 等分段([latency-budget](../../docs/latency-budget.md))每天积累到足够做 p99 的样本量。它服务三件事:M1.5 延迟探针、M3 末满订阅探针(里程碑归 [architecture-v2/11](../../design/11-实施路线图.md))与[上线门槛](../../docs/testing.md#8-上线门槛) ⑩ 的样本。三种模式:`quote`(每个探测点在 `Bbo` 触发时至多每 `probe_period_ms` 挂一张离对手价 `far_bp` 的最小名义 PostOnly(突发层可另按 `burst_period_ms` 提高配额),Ack 后在同一事件内撤单;撤单即一个 `rep_to_order` 样本(`LatencySample.action = Cancel`);`quote_xgw` 组另由一腿的 Ack 撤另一 venue 的在挂探测单,产生跨 gw 的样本)、`pair`(每 `pair_period_ms` 在一所以最小名义 IOC 穿价、经 LegLink 在另一所下第二腿,成交后 `pair_flat_ms` 内两边平掉)与 `target`(每 `target_period_ms` 设一个两腿最小量的目标意图,由 [hft-exec](../../crates/hft-exec/README.md) 切片与对冲,`pair_flat_ms` 后设 0/0 退出)。`StrategyDesc.non_prod_only = true`:引擎拒绝把它绑定到任何 `is_prod = true` 的账户([SDK](../../crates/hft-strategy-sdk/README.md#strategy-与-model))。没有慢速部分(`Model = NoModel`)。它不拥有订单状态机、第二腿、执行算法、风控与账本;所有下单照常经模式门、风控检查链与测试网限速。参数 `enabled` 默认 false。本 README 不出现交易所名字,交易所差异只经 `VenueCaps` 与 refdata。

## 目录

- [使用本组件](#使用本组件)
  - [用途与运行要求](#用途与运行要求)
  - [装配与策略接口](#装配与策略接口)
  - [组](#组)
  - [参数 schema(ProbeParams)](#参数-schemaprobeparams)
  - [事件处理](#事件处理)
  - [意图](#意图)
  - [产生的延迟样本](#产生的延迟样本)
  - [Note 码表与报表字段](#note-码表与报表字段)
  - [运行模式](#运行模式)
- [实现说明](#实现说明)
- [延迟与资源契约](#延迟与资源契约)
- [测试与验收](#测试与验收)
- [依赖](#依赖)
- [延伸阅读](#延伸阅读)
- [已知限制与后续工作](#已知限制与后续工作)

## 使用本组件

### 用途与运行要求

| 场合 | 实例与账户 | 模式与用途 |
|---|---|---|
| M1.5 延迟探针(按需租用的实例,只放测试网 key) | `prod` 角色实例,`order_env = "testnet"`,测试网账户(`is_prod = false`) | `quote`:首批 Graviton 上的 `kernel_rx`/`ingest`/`decide`/`egress`/`tick_to_trade`/`rep_to_order` 直方图(HW/SW 分开;至少一个 `quote_xgw` 组给出跨 gw 的 `rep_to_order`) |
| M3 末满订阅探针(按需租用;范围与线程计划归 [architecture-v2/11](../../design/11-实施路线图.md)) | 同上 | `quote`:满订阅负载下的下单路径样本(该探针用来选定主机类与是否启用 IRQ 拆分) |
| dev2 测试网联调窗口(M6) | 同上 | 功能冒烟、样本管线验证(dev2 不做延迟验收);`pair`/`target` 的小样本确认(只记录) |
| 统一升级主机,M7 延迟验收 | 同上,与 `sim` 实例同机 | `quote`:门槛 ⑩ 的订单类分段样本(主网行情负载下);`pair`/`target`:任意 ≥ 1 天的测试网行情时段的小样本确认(只记录) |
| 正式实盘(M8 起) | — | **不能运行**:正式账户 `is_prod = true`,装配退出 2(`ProbeOnProdAccount`),`SetParams` → `ParamInvalid{NonProdOnly}` |

- ⑩ 用哪些样本、样本量与判定口径归 [testing §8](../../docs/testing.md#8-上线门槛);行情类分段(`kernel_rx`、`ingest`、`feed_host`)按事件加权的离线统计取自 seq journal,不靠本策略。本策略只产生订单类分段的样本。
- Sim 账户(`is_prod = false`)引擎允许绑定,但 sim-gw 的出环与回报是模拟的,样本只进模拟数据集、不计入延迟统计([latency-budget §5](../../docs/latency-budget.md#5-分组与不同数据集不混算));不建议这样用。
- 测试网上的 `order_rtt`、`tick_to_ack` 只代表测试网,不代表生产;主机内分段(`kernel_rx`、`ingest`、`decide`、`egress`、`tick_to_trade`、`rep_to_order`、`hedge_host`、`leg_gap`)才是本策略要的证据。
- **风控参数要求**(测试网账户的 `RiskParams`,由运营设定,[hft-risk](../../crates/hft-risk/README.md)):探测单是 Open 类,检查链 #5 的价格带 `max_px_dev` 须 ≥ `far_bp`,否则每张都被 `PX_BAND` 门控拒绝、只剩 `decide` 样本;`pair_cross` 须 ≤ 两腿 inst 的 `ioc_max_cross`(`target` 模式的切片越价同样按它截取);单笔名义上限须 ≥ 各 inst 的最小名义 × (1 + `size_margin`)。
- **行情环境**:`quote` 模式在"主网行情 + 测试网下单"下运行(`far_bp` 远大于两边价差时 PostOnly 不会穿价),这正是 ⑩ 需要的主网负载;`pair` 与 `target` 模式要求本实例 `feed_env = "testnet"`([apps/hft-core](../../apps/hft-core/README.md)),否则按主网价格穿价的 IOC 在测试网上多数不成交(只记 `PairNoFill`)。

### 装配与策略接口

```rust
// strategies/registry 的 strategy_set! 列表中的一行(core,默认构建即包含)
latency_probe::LatencyProbe,

pub struct LatencyProbe { inst: StrategyInstance, groups: Box<[ProbeGroup]>, p: ProbeParamsNs }
impl Strategy for LatencyProbe {
    const DESC: StrategyDesc = StrategyDesc { name: "latency-probe", kind_id: 5, version: 1, non_prod_only: true,
                                              schema: &ProbeParams::SCHEMA, notes: &NOTES, report: &REPORT, cmds: &[] };
    type Params = ProbeParams;
    type Model = NoModel<ProbeParams>;
    /* new / subscriptions / on_event / on_params / snapshot / restore:签名见 SDK */
}
```

`kind_id = 5`、`schema.id = 5`,登记于 [strategies/registry](../registry/README.md);`ProbeParams` 用 SDK 的 `define_params!` 声明。不接受策略命令:启停用参数 `enabled`(人工经 `hft-cfg` 发 `SetParams`),残余仓位用引擎的 `Kill{flatten}` 或本策略的自动平仓处理。

### 组

| 模式 | 腿 | 说明 |
|---|---|---|
| `quote` | `leg 0..n`(1 ≤ n_legs ≤ 4) | 每条腿是一个探测点 (venue, inst);同一 inst 只出现在一个 group(引擎的 inst 引用规则);`quote_xgw = true` 时要求 n_legs ≥ 2 且 leg 0 与 leg 1 的 venue 不同(不同 gw),否则 `check` 拒绝(`GroupShape`) |
| `pair` | `leg 0` = A(第一腿,IOC 穿价)、`leg 1` = B(第二腿,LegLink `OnFill`) | 同一资产、不同交易所(引擎强制);至少一条永续腿;两腿都是现货 → `check` 拒绝(`GroupShape`) |
| `target` | 同 `pair`(A = 先手腿) | 同 `pair`;两腿目标经 `ExecSpec::PairOnFill` 由执行层切片与对冲 |

`pair`/`target` 开仓方向(`pair_side = auto`):有现货腿时现货腿开仓买入(A 为现货 → 买 A、卖 B;B 为现货 → 卖 A、买 B),平仓卖出,不做现货借币;两腿都是永续时每次开仓轮流换向,使累计方向中性。订阅:每条腿 `SubsBuilder::market(inst, gix, EvMask::BBO, Part::Fast)`。

### 参数 schema(`ProbeParams`)

| 字段 | 单位 | 默认 | 组 | 属性 | 含义 |
|---|---|---|---|---|---|
| `enabled` | Bool | false | | `manual_only`、`relax = Up` | false 时不发任何新探测单与新开仓目标(已挂的探测单照常撤,残余照常平) |
| `mode` | Enum[quote, pair, target] | quote | ✓ | `manual_only` | 该 group 的模式 |
| `probe_period_ms` | DurationMs | 1 000(范围 100–600 000) | ✓ | `relax = Down` | quote:同一探测点两次挂单的最小间隔(engine_ns) |
| `far_bp` | Rate | 300 bp(范围 50 bp–10%) | ✓ | `relax = Down` | quote:PostOnly 价格离对手价的距离 |
| `quote_side` | Enum[buy, sell, alternate] | buy | ✓ | | quote 的挂单方向;`alternate` 每次挂单换向;现货腿一律只挂买单(sell 需要库存) |
| `quote_rest_ms` | DurationMs | 2 000(范围 100–600 000) | ✓ | | 探测单的 `max_rest_ms`(撤单丢失时由 OMS 发 `Aged` 兜底) |
| `quote_xgw` | Bool | false | ✓ | | quote:跨 gw 变体(见 [quote 探测](#quote-探测)第 5 步);非 quote 组忽略 |
| `burst_period_ms` | DurationMs | 0(范围 0–600 000) | ✓ | | quote:突发层触发的最小间隔(分层触发配额,见 [quote 探测](#quote-探测)第 1 步);0 = 不另设 |
| `pair_period_ms` | DurationMs | 10 000(范围 10 000–3 600 000) | ✓ | `relax = Down` | pair:两次开仓的最小间隔(默认 ≈ 8,640 次/天) |
| `target_period_ms` | DurationMs | 60 000(范围 10 000–3 600 000;暂定) | ✓ | `relax = Down` | target:两次设开仓目标的最小间隔 |
| `pair_flat_ms` | DurationMs | 5 000(范围 1 000–600 000) | ✓ | | pair/target:第一腿成交(target:开仓目标 `Done`)后多久两边平掉;也是平仓复查间隔 |
| `pair_cross` | Rate | 50 bp(范围 1 bp–5%;暂定) | ✓ | `relax = Up` | pair:第一腿与平仓 IOC 的越价;target:切片 `max_cross` 与对冲 `hedge_cross`(都须 ≤ `RiskParams` 的 `ioc_max_cross`);残余平仓 IOC 的越价 |
| `pair_side` | Enum[auto, buy_a, sell_a] | auto | ✓ | | pair/target:开仓方向(auto 见"组";`buy_a` = 买 A 卖 B,`sell_a` = 卖 A 买 B) |
| `size_margin` | Rate | 10%(范围 0–100%) | | | 单笔量的名义余量(见"意图") |
| `max_spread` | Rate | 100 bp(范围 1 bp–10%) | ✓ | `relax = Up` | 本腿 (ask − bid) / mid 超过它时本次不探测(测试网盘口异常) |
| `max_inv_notional` | Notional | 100 USDT(范围 1–10 000 USDT;暂定) | ✓ | `relax = Up` | 该 group 残余仓位名义上限;超过 → 停探测、平仓并 `request_halt`(见 [quote 探测](#quote-探测)第 6 步) |
| `max_consec_fail` | Count | 5(范围 1–100) | | `relax = Up` | 连续执行失败(交易所拒单,不含门控)达到 → `request_halt` |

表中顺序即 schema 字段下标 `ix`(`enabled` = 0 … `max_consec_fail` = 16)。**`check` 的错误**(按 gix 升序逐组检查,每组先查腿形、再查字段,返回第一个错误):`GroupShape{gix, code}`——1 = 腿数与模式不符(quote 1–4 腿,pair/target 恰 2 腿)、2 = pair/target 两腿都是现货、3 = `quote_xgw` 组腿数 < 2 或 leg 0 与 leg 1 同 venue;`Invalid{field_ix, code: 1}`——`field_ix = 4`:quote 组 `quote_side = sell` 且含现货腿;`field_ix = 12`:`pair_side` 使现货腿开仓卖出(A 为现货且 `sell_a`,或 B 为现货且 `buy_a`);`field_ix = 7`:quote 组 `burst_period_ms ≠ 0` 且大于该组 `probe_period_ms`。

本策略不走门槛 ⑦(不绑定正式账户),全部字段的 `minor_change` 取缺省 `None`。

### 事件处理

| `EventRef` | 处理 |
|---|---|
| `Bbo`(quote 组的某条腿) | [quote 探测](#quote-探测)第 1 步的条件全部满足 → 挂单;到期且该腿空闲、但报价不健康或价差过宽 → 计 `Skipped` |
| `Bbo`(pair/target 组的 A 腿) | 阶段为 `Due` → [pair 探测](#pair-探测)第 2 步,通过即发第一腿(target 组:设开仓目标) |
| 定时器 `PAIR_TICK` / `FLAT`(tag 1 / 2) | 按 [pair 探测](#pair-探测)的阶段迁移表 |
| `Order` | quote 探测单:`Acked` → 同一事件 `cancel`(该撤单就是由 Ack 回报触发的下一单,产生 `action = Cancel` 的 `rep_to_order` 样本;`quote_xgw` 组的 leg 0 例外,见 [quote 探测](#quote-探测)第 5 步);`Cancelled`/`Aged` → 清在途,`Throttle` 记 `NoFill`;`Rejected{bits, venue_code}` → 清在途,`bits = 0`(交易所拒单)记 `Failed` 并计 `ProbeRejected`,`bits ≠ 0`(门控位,含 `LOCAL_SEND_FAIL`)记 `Gated` 并计 `ProbeGated`;`Unknown` → 保留在途、暂停该腿,直到同一订单的终态回报再按上述处理;`Filled`(意外成交)→ 写 `ProbeFilled`、记 `Filled`、对该腿发残余平仓(`done` 时清在途)。pair 第一腿与平仓腿的终态按[阶段迁移表](#pair-探测);残余平仓单的回报只用于残余规则 |
| `Exec` | target 组按 [target 探测](#target-探测);pair 组只在其 cell Broken 与解除时收到 `Broken`、`Done{cleared_by_engine}`,按阶段迁移表;quote 组忽略 |
| `External` | 该腿仓位被外部改变 → 对每条净量非 0 的腿发残余平仓;quote 组各腿置外部暂停位、pair/target 组进 `Blocked`,都到净量为 0 为止(`cause = Manual` 时执行层的 `manual_hold` 是否随之解除按 [hft-exec 人工保持与恢复期抑制](../../crates/hft-exec/README.md#人工保持与恢复期抑制)) |
| `Risk` | `MarginStress` 不另设状态:探测与开仓条件要求 `v.risk(a).margin_level` 为 None(平仓照常);`HedgeBroken{group}` → 该 group 进 `Blocked`(阶段迁移表);本 cell 的 `CapSet/CapCleared{Halted}` → `Throttle::set_halted(true/false)` |
| `Mode` | 降到 `CloseOnly`/`Shadow` → 停探测(在挂的 Open 探测单由 sequencer 撤):`Due` → `Idle`,target 组 `Opening` → `clear_target` 后进 `Holding`(登记 `FLAT`),`Throttle::reset_open_state()`;平仓照常。回到 `Live` → quote 各腿 `last_probe_ens = 0`,`Idle` 的 pair/target 组 `next_pair_ens = now`、本事件登记 `PAIR_TICK` |
| `Notice` | `Delisting` → 该腿(`inst = INST_ALL` 时该 venue 的全部腿)永久暂停;`Maintenance{from_ns, to_ns}` → `now.utc_ns` 落在 [from_ns, to_ns] 内时该腿暂停;暂停的腿不探测,含暂停腿的 pair/target 组不开仓 |
| `Conn` | 不单独处理(每次探测前查 `v.quote_ok`) |
| `Advice` / `Manual` / `Warmup` | 不会收到(无 Model、`cmds` 为空) |

**上一事件意图的结果**(每次 `on_event` 先读 `Ctx::outcome`):订单意图 `Placed(r)` → 该腿在途为 r;`Rejected(bits)` → 清在途、`Throttle` 记 `Gated`、计 `ProbeGated`;`Shadowed` → 清在途、记 `Gated`;pair 第一腿的 `Rejected`/`Shadowed` 使 `Opening` 回 `Idle`(不计 `PairNoFill`)。目标意图的结果见 [target 探测](#target-探测)。

### 意图

| 动作 | 意图 | 类别 |
|---|---|---|
| quote 探测 | 该腿 `PostOnly`:买 `px = floor_to_tick(ask × (1 − far_bp))`、卖 `px = ceil_to_tick(bid × (1 + far_bp))`(`mul_rate` 与取整到 tick 同向:买 Floor、卖 Ceil);`qty` 见下;`max_rest_ms = quote_rest_ms`;无 `link` | Open |
| quote 撤单 | `Ctx::cancel(rest)`,与 `Acked` 同一事件 | — |
| pair 开仓 | A 腿 `Ioc`,`px` = 对手价 × (1 ± `pair_cross`)(买按 ask × (1 + pair_cross)、卖按 bid × (1 − pair_cross),按 `Side::price_round` 取整到 tick),`qty` 见下;`link = LegLink{role: First, tolerance: 0(执行层缺省阈值), hedge: HedgePlan{kind: OnFill, px_mode: Cross, px_param: pair_cross, max_slip: pair_cross, on_fail: Retry, tif: Ioc, venue/account/inst = B, side = A 的反向, reduce_only = 0, ratio 1/1, residual: TrimExcess}}`;第二腿由 OMS 产出 | Open(第二腿由风控按 Hedge 分类) |
| pair 平仓 | 按 `v.legs()` 的实际净量:A 腿反向 `Ioc`,数量 = A 腿净量的绝对值向零取整到 A 腿 lot,价格按开仓的越价规则(永续带 `REDUCE_ONLY`);`link` 同上但 `HedgePlan.side` = 本单方向的反向、`reduce_only` = B 为永续时 1 | Reduce |
| target 开仓 | `Ctx::target(TargetIntent{reason: Rebalance, urgency: Normal, band: ⌈(pair_lot / 2) × 10^9 / q⌉, legs: [(0, ±q), (1, ∓q)], exec: PairOnFill{slice: {depth_frac: 100%, depth_levels: 1, max_cross: pair_cross, max_chunk_notional: q 按 A 腿开仓方向对手价的名义 × 2(一笔切完), min_chunk_notional: 0, cooldown_ms: 0}, first_leg: Fixed, first_leg_ix: 0, hedge_px_mode: Cross, hedge_cross: pair_cross, hedge_max_slip: pair_cross, ratio 1/1, fix_tolerance: 0, residual: TrimExcess}, tag})`;`q` 同 pair 的最小量;`band` 为半个 `pair_lot` 的相对量(`Rate`,向上取整):差不足半个 lot(现货腿以基础币扣手续费、净量略小于目标)判到达,差一个 lot 仍会交易 | 增仓部分按 Open(切片与第二腿由执行层与 OMS 产出) |
| target 平仓 | 同一 `exec` 的 `TargetIntent{reason: Exit, legs: [(0, 0), (1, 0)]}`;重发时 `tag` 低位放重发次数 | 减仓部分按 Reduce |
| 残余平仓 | 单腿 `Ioc` 反向,数量 = 该腿净量的绝对值(向零取整到 lot),价格按 pair 开仓的越价规则(该组的 `pair_cross`),永续腿带 `REDUCE_ONLY` | Flatten |

**单笔量 q**:满足 `q × px ≥ min_notional × (1 + size_margin)`、`q ≥ min_qty`、q 为 lot 整数倍的最小值(i128 计算;`min_notional`、`min_qty`、lot 取 refdata `InstHot`);quote 探测的 px 取该单限价;pair/target 的 q 须同时满足两腿(各取该腿当时的对手价:买入腿 ask、卖出腿 bid)且为两腿 `pair_lot` 的整数倍。

两腿残余与第二腿失败的处理归执行层与 OMS(补腿、Broken、自动有界平仓,[hft-exec](../../crates/hft-exec/README.md));本策略不自行补腿。

### 产生的延迟样本

样本由引擎产生(OMS 对新单、撤单与改单各合成一条 `LatencySample`,`action` = New / Cancel / Amend,字段归 [hft-event](../../crates/hft-event/README.md),合成归 [hft-oms](../../crates/hft-oms/README.md));`order_rtt` 只统计 `action = New`,`rep_to_order` 含三种 action,按触发事件的 kind 区分 `tick_to_trade` 与 `rep_to_order`(分段计算与分组归 [latency-budget](../../docs/latency-budget.md)),本策略只负责让它们按节奏出现。下表的数量是默认参数下的产生速率;⑩ 用哪些、要多少归 [testing §8](../../docs/testing.md#8-上线门槛)。

| 模式 | 触发 | 分段 | 每天数量(默认参数) |
|---|---|---|---|
| quote | `Bbo` → PostOnly 出环 → Ack;Ack 回报 → 同一事件的撤单出环 | 挂单(`action = New`):`kernel_rx`、`ingest`、`decide`、`egress`、`tick_to_trade`(HW/SW 时间戳分开)、`order_rtt`、`tick_to_ack`;撤单(`action = Cancel`):`rep_to_order`(回报帧 → 由它触发的下一单,走与对冲相同的 gw 解码 → rep 环 → sequencer → out 环 → gw 发送路径;不计 `order_rtt`) | 每个探测点挂单 ≈ 86,400、`rep_to_order` 同量(每秒 1 张;盘口一秒内无 `Bbo` 时顺延到下一条) |
| quote(`quote_xgw`) | leg 1 的 Ack 回报 → 同一事件撤 leg 1 自己与 leg 0 的在挂探测单 | 跨 gw 的 `rep_to_order`(回报由 leg 1 的 gw 读到,撤单经 leg 0 的 gw 发出)与同 gw 的 `rep_to_order` 各一条 | 每个 `quote_xgw` 组各 ≈ 86,400(leg 0 不再产生同 gw 的撤单样本) |
| pair | A 腿 `Bbo` → 第一腿 IOC;第一腿成交帧 → 第二腿出环 | 第一腿同上;第二腿 `hedge_host`(`rep_to_order` 中 `link_role = Second` 的子集)、`leg_gap` | 每个 group ≈ 8,640(开仓链接);平仓链接另有同量 |
| target | A 腿 `Bbo` → 目标意图 → 执行层切片出环;切片成交帧 → 第二腿出环 | 切片按触发事件分组的 `tick_to_trade`;第二腿 `hedge_host`、`leg_gap` | 每个 group ≈ 1,440(开仓);平仓另有同量 |

### Note 码表与报表字段

即 `DESC.notes` / `DESC.report`([SDK 策略描述符](../../crates/hft-strategy-sdk/README.md#策略描述符note报表策略命令));计数类 Note(2–5)每 group 每 10 min 至多一条(计数非零时),不按探测单逐条写:处理该 group 的事件时先检查,若 `now.engine_ns ≥ note_due_ens`,对每个非零计数写一条并清零,`note_due_ens = now.engine_ns + 600 000 ms`(group 起步时同样设为起步时刻 + 600 000 ms),然后才处理该事件本身。其余 Note 在产生时写;`v[]` 中未列出的项为 0。

| code | name | 级别 | v[] | 产生时机 |
|---|---|---|---|---|
| 1 | `ProbeFilled` | P2 | v0 = leg_ix,v1 = 成交量,v2 = 成交价 | quote 探测单意外成交 |
| 2 | `ProbeRejected` | Info | v0 = 次数,v1 = 最近一次 `RejectBits`,v2 = 最近一次 `venue_code` | 交易所拒单(非门控)计数 |
| 3 | `ProbeGated` | Info | v0 = 次数,v1 = 最近一次 `RejectBits` | 门控拒绝计数(价格带、限速、kill 等) |
| 4 | `PairNoFill` | Info | v0 = 次数 | pair 第一腿 IOC 无成交计数 |
| 5 | `Skipped` | Info | v0 = 价差过宽次数,v1 = 报价不健康次数 | 到期但未探测的计数 |
| 6 | `FlattenIncomplete` | P1 | v0 = 残余名义,v1 = leg_ix | pair/target 两次平仓后仍未归零 |
| 7 | `ProbeHalted` | P1 | v0 = 原因(1 连续失败、2 残余超 `max_inv_notional`),v1 = 计数或名义 | `request_halt` |
| 8 | `TargetStuck` | P2 | v0 = `StuckReason` 码(hft-exec 登记),v1 = 1 开仓目标 / 2 平仓目标 | target 组收到 `Exec{Stuck}` |
| 9 | `TargetRejected` | P2 | v0 = `TargetReject` 码(hft-exec 登记) | target 组的目标被拒(`Ctx::outcome` 返回 `TargetRejected`) |

| 报表字段 | 单位 | 聚合 | 范围 | 来源 |
|---|---|---|---|---|
| `probe_fills` | Count | Count | Group | `ProbeFilled` |
| `rejects` / `gated` / `pair_nofill` | Count | Sum | Group | `ProbeRejected` v0 / `ProbeGated` v0 / `PairNoFill` v0 |
| `skipped_spread` | Count | Sum | Group | `Skipped` v0 |
| `flatten_incomplete` | Count | Count | Group | `FlattenIncomplete` |
| `target_stuck` / `target_rejected` | Count | Count | Group | `TargetStuck` / `TargetRejected` |

延迟直方图本身由 watchtower 从 `LatencySample` 汇总,不经本策略的报表。

### 运行模式

本策略只在有效模式 `Live` 时发新探测单与开仓目标(`Shadow`、`CloseOnly` 下不探测,平仓照常,见[事件处理](#事件处理)的 `Mode` 行);意图的处理以 [SDK 运行模式行为表](../../crates/hft-strategy-sdk/README.md#运行模式下意图的行为表)为准:`Live` 且 `open_allowed` 为真才出环,`open_allowed` 为假时探测单按 `MODE_SHADOW` 只记录(只有 `decide` 样本,结果 `Shadowed`)、目标的第一笔切片记为 `SHADOW` 决策。启用:人工 `SetParams{enabled = true}`(`manual_only`)→ `SetMode{Live}`。

## 实现说明

<details><summary>实现要点</summary>

### 状态(`ProbeGroup`,≤ 256 B)

每条腿:`last_probe_ens: i64`、`rest: Option<OrderRef>`、`cancel_sent: bool`、`next_side: Side`、`paused: u8`(Unknown、Delisting、外部成交位);每 group 一个维护窗口 `maint: (i64, i64)`(UTC ns;无则 (0, 0))及其适用腿的位图(同一窗口的通知并入位图,不同窗口的通知替换前一个);pair/target:`phase: PairPhase { Idle, Due, Opening, Holding, Flattening(attempt), Blocked }`、`next_pair_ens`、`flat_at_ens`、`next_dir: i8`、`retry_tag: u8`(target 平仓目标的重发次数,放进 `tag` 低位);`thr: Throttle`;各 Note 的计数与 `note_due_ens`;`fp: u64`(GroupSpec 指纹)。实例级另有 `last_bbo_rx_tsc`(本实例上一条 `Bbo` 的 `rx_tsc`,只用于突发判别;不进快照,`restore` 后为 0)。跨事件时刻全部是 engine_ns,快照里没有 TSC。`restore` 后 `rest` 视为待核:下一事件用 `v.order(r)` 查,`None` 或已终态则清在途,仍在挂(`PendingNew`/`Live`)则撤;快照前最后一个事件挂出、`Ctx::outcome` 尚未读到 `OrderRef` 的探测单保持在途,等它的回报带来 `OrderRef`;在途而始终没有 `OrderRef` 的腿在挂单时刻 + 2 × `quote_rest_ms` 之后视为失效(被接受的单最迟以 `Aged` 回报)。回到 `Live` 时在途的腿不清 `last_probe_ens`。target 开仓目标被拒 `LotMismatch` 进入的 `Blocked` 只由参数变化(`on_params`)解除,净量为 0 也不回 `Idle`。订单意图的 `tag` = 种类 << 8 | 腿号(1 探测单、2 pair 第一腿、3 平仓第一腿、4 残余平仓),回报按它归类;pair/target 组在 `restore` 之后的第一个事件:target 组先 `clear_target`;`Blocked` 保持;其余阶段按 `v.legs()` 实际净量——为 0 → `Idle`,否则 → `Holding`(登记 `FLAT = now + pair_flat_ms`)。`on_params` 中指纹变化或新 GroupId 的 group 状态从空起步;`mode` 变化等同于该 group 重新开始(在下一个事件中先按实际净量发残余平仓,再从 `Idle` 起步)。`Throttle` 参数:`cooldown_ms = 0`、`fallback_ms = 0`、`hard_rearm_ms = 0`、`max_consec_fail` = 同名参数;quote 组不调用 `on_fire`(每腿在途由 `rest` 管理,`can_fire(Open)` 只受 halted 约束),pair/target 组在发第一腿或设开仓目标时 `on_fire(Open)`。

### quote 探测

1. 条件(依次,任一不满足本次不探测):`enabled`;有效模式 `Live`;到期——`now.engine_ns ≥ last_probe_ens + probe_period`,或**突发触发**:`burst_period_ms ≠ 0`、本 `Bbo` 的 `rx_tsc ≠ 0` 且等于 `last_bbo_rx_tsc`(两条记录由同一次读取凑齐,属 [testing §8.2](../../docs/testing.md#82-延迟验收的测法) 第 9 条的突发层)、且 `now.engine_ns ≥ last_probe_ens + burst_period`;该腿无 `rest`、未暂停;`v.risk(a).margin_level` 为 None;`v.quote_ok(a, inst)`(否则计 `Skipped` v1);两侧报价都 > 0 且价差 ≤ `max_spread`(价差 = (ask − bid) × 10^9 / mid,mid = `Px::mid_floor(bid, ask)`,i128,HalfEven;否则计 `Skipped` v0);`Throttle::can_fire(Open)`;该 group 残余名义 ≤ `max_inv_notional`。每条 `Bbo` 处理完后 `last_bbo_rx_tsc = b.rx_tsc`。突发判别只决定配额;样本属于哪一层仍由 testing §8.2 第 9 条的离线统计判定(本策略看不到同一读取里的其他记录,判别是突发层的子集)。
2. 价格与数量按"意图"表;`last_probe_ens` 在决定时即更新(无论之后被拒与否),所以单腿下单速率恒 ≤ 1 / min(`probe_period_ms`, 非 0 的 `burst_period_ms`)。`alternate` 的方向在每次挂单后翻转(现货腿恒为买)。
3. 回报按[事件处理](#事件处理)表的 `Order` 行:`Acked` → 同一事件 `cancel`、`cancel_sent = true`(撤单在处理 Ack 的同一事件内出环,`rep_to_order` 因此量的是回报路径而不含策略等待);`Cancelled`/`Aged`/`Rejected` → 清 `rest`;`Unknown` → 保留 `rest` 并暂停该腿,终态回报到达后按终态处理。每条腿同一时刻至多一张探测单(连同它的撤单)在途。
4. `Throttle::on_outcome`:交易所拒单 → `Failed`;返回 `RequestHalt` → `request_halt(g, 1)`、写 `ProbeHalted{v0 = 1, v1 = 连续失败次数}`、`Throttle::set_halted(true)`;门控拒绝 → `Gated`(计 `ProbeGated`,不计失败);`Cancelled`/`Aged` → `NoFill`(清零连续失败);意外成交 → `Filled`。
5. **跨 gw 变体**(`quote_xgw = true`):leg 0 是锚,它的探测单 Ack 后不撤、保持在挂(仍受 `quote_rest_ms` 的 `Aged` 兜底,该轮无跨 gw 样本);leg 1 只在 leg 0 有已 Ack 的在挂单时探测(第 1 步的其余条件照旧);leg 1 的 `Acked` → 同一事件先撤 leg 1 自己的单、再撤 leg 0 的在挂单(leg 0 已无在挂单时只撤 leg 1 的)。后一张撤单由 leg 1 所在 gw 读到的回报触发、经 leg 0 所在 gw 发出,即跨 gw 的 `rep_to_order` 样本(触发回报的 venue ≠ 该单 venue);不同 venue 的下单由不同 gw 线程承担(每个 `gw:<venue>` 独占一个线程,[hft-core 线程计划](../../apps/hft-core/README.md#线程计划)第 4 条),Sim 账户的下单都经 sim-gw,不产生跨 gw 样本。leg 2 及以后按普通 quote 腿处理。
6. **残余上限**(quote、pair、target 组都适用):处理该 group 的 `Order{Filled}`、`External` 与 `FLAT` 时按当时的视图计算残余名义——quote 组 = Σ 各腿 |净量| × 该腿 mid,pair/target 组 = |Σ 两腿(净量 + 即时在途)| × A 腿 mid(同一资产,两腿相抵;在途的第二腿计入,第一腿刚成交时不误判),mid = `Px::mid_floor(bid, ask)`,名义向上取整;> `max_inv_notional` 且 `Throttle` 未 halted → 本事件不执行阶段迁移表的动作(pair/target 组进 `Blocked`;`ProbeFilled` 照写),只对每条净量非 0 的腿发残余平仓、`request_halt(g, 2)`、写 `ProbeHalted{v0 = 2, v1 = 名义}`、`Throttle::set_halted(true)`;已 halted 时只发残余平仓。同一事件里每条腿至多一张残余平仓单(与 `Filled` 行的残余平仓合并)。

### pair 探测

1. 定时器 `PAIR_TICK`(tag 1)与 `FLAT`(tag 2)以 GroupId 登记,engine_ns 截止经 `Now::tsc_of` 换算。处理该 group 的事件末尾,若阶段为 `Idle` 且没有已登记的 `PAIR_TICK`,按 `max(next_pair_ens, now)` 登记(含进入 `Idle` 的那个事件;新 group 的 `next_pair_ens` = 它的第一个事件的 `now.engine_ns`)。
2. `Due` 时 A 腿的下一条 `Bbo`:`enabled`、有效模式 `Live`、`v.risk(a).margin_level` 为 None、两腿都未暂停、两腿 `quote_ok`、两腿价差 ≤ `max_spread`、净量为 0(两腿净量的绝对值都 < 该腿 1 lot)、`Throttle::can_fire(Open)` → 发第一腿(意图表)、`on_fire(Open)`、`next_pair_ens = now + pair_period`;条件不满足保持 `Due`(报价或价差不满足计 `Skipped`)。触发事件因此总是 `Bbo`,`tick_to_trade` 按 `Bbo` 触发分组。
3. **阶段迁移表**(pair 与 target 共用;"净量为 0"同第 2 步,按 `v.legs()`):

| 阶段 | 事件 | 动作 | 之后 |
|---|---|---|---|
| `Idle` | `PAIR_TICK` 到期 | — | `Due` |
| `Due` | A 腿 `Bbo`,第 2 步条件满足 | pair 发第一腿;target 设开仓目标 | `Opening` |
| `Opening` | pair 第一腿终态、成交量 0 | 计 `PairNoFill`;`on_outcome(NoFill)` | `Idle` |
| `Opening` | pair 第一腿终态有成交;target 开仓目标 `Exec{Done}` | 登记 `FLAT`(`flat_at_ens = now + pair_flat_ms`);`on_outcome(Filled)` | `Holding` |
| `Holding` | `FLAT` 到期,净量为 0 | — | `Idle` |
| `Holding` | `FLAT` 到期,pair 两腿都有净量;target 任一腿有净量 | pair 发平仓链接;target 设 0/0 目标 | `Flattening(1)` |
| `Holding` | `FLAT` 到期,pair 只有 A 腿有净量(第二腿失败、cell 可能 Broken) | 不动作(等执行层补腿或 Broken 处置),`pair_flat_ms` 后再登记 `FLAT` | `Holding` |
| `Flattening(n)` | pair 平仓第一腿终态 | 登记 `FLAT`(`now + pair_flat_ms`,等第二腿) | `Flattening(n)` |
| `Flattening(n)` | `FLAT` 到期且净量为 0;target 平仓目标 `Exec{Done}` | target 另 `clear_target` | `Idle` |
| `Flattening(1)` | pair:`FLAT` 到期而净量不为 0 | 按当时净量再发一次平仓链接 | `Flattening(2)` |
| `Flattening(2)` | pair:`FLAT` 到期而净量不为 0 | 写 `FlattenIncomplete`(v0 = Σ 各腿 \|净量\| × 该腿 mid 的名义,v1 = 其中名义最大的腿号) | `Blocked` |
| 任意 | `Exec{Broken}` 或 `Risk{HedgeBroken{group}}` | 不开仓、不设新目标、不平仓(本策略不自行补腿) | `Blocked` |
| `Blocked` | `Exec{Done{cleared_by_engine: true}}` 且净量不为 0 | pair 发平仓链接;target 设 0/0 目标 | `Flattening(1)` |
| `Blocked` | 处理该 group 的任一事件时净量为 0 | — | `Idle` |

表外的 (阶段, 事件) 组合不改阶段、不产生意图(例如 `Idle` 时的 A 腿 `Bbo`、`Holding` 时的 `PAIR_TICK`)。target 组的 `Stuck` 与 `TargetRejected` 见 [target 探测](#target-探测)。

4. 开仓方向见"组";`pair_side ≠ auto` 时按参数,但现货腿不卖出(参数与腿型冲突 → `check` 拒绝)。两腿都是永续时 `auto` 的方向随每次开仓翻转(`next_dir`),第一次为买 A 卖 B。

### target 探测

1. `PAIR_TICK` 按 `target_period_ms` 推进(`next_pair_ens = now + target_period`),`Idle` → `Due` 另要求该 group 无生效目标(`v.target(a, g)` 为 `None`;有生效目标时保持 `Idle`,`next_pair_ens = now + target_period`);其余条件与阶段迁移同 [pair 探测](#pair-探测)。
2. 设开仓目标后下一事件的 `Ctx::outcome`:`TargetAccepted`/`TargetShadowed` → 保持 `Opening`;`TargetRejected(r)` → 写 `TargetRejected{v0 = r}`、`on_outcome(Gated)`、回 `Idle`,`r = LotMismatch` 时进 `Blocked`(要改参数)。
3. 目标的 `tag` = 重发次数(`retry_tag`,低 8 位;开仓目标与首次平仓目标为 0,其余位 0)。
4. `Exec{Stuck(r)}`:`Opening` → 写 `TargetStuck{v0 = r, v1 = 1}`,改设 0/0 平仓目标 → `Flattening(1)`;`Flattening(n)` 且 `r ∈ {Shadow, Gated(_)}` → 只写 `TargetStuck{v1 = 2}`(计入 `target_stuck`),不重发、阶段不变;`Flattening(n)` 的其他原因 → 写 `TargetStuck{v1 = 2}`;n = 1 时登记 `FLAT`(`now + pair_flat_ms`),到期以 `tag` + 1 重发同一 0/0 目标 → `Flattening(2)`(逐字段相同的目标不会被执行层当作新目标,所以必须改 `tag`);n = 2 时 `clear_target`、写 `FlattenIncomplete`、进 `Blocked`。
5. `Exec{Broken}` 与 `Risk{HedgeBroken}` 按阶段迁移表;本策略不自行补腿。

### 规划源文件

| 文件 | 职责 |
|---|---|
| `src/lib.rs` | `LatencyProbe`、`DESC` |
| `src/params.rs` | `define_params!` 声明 `ProbeParams`;`check`(腿数与模式、两现货腿、`pair_side` 与腿型、`quote_side = sell` 与现货腿、`quote_xgw` 的腿数与 venue) |
| `src/desc.rs` | Note 码表、报表字段 |
| `src/fast/{quote,pair,target,flatten}.rs` | quote 探测、pair 阶段机、target 阶段机、残余平仓(`#![deny(clippy::float_arithmetic)]`) |
| `tests/*.rs`、`tests/scenarios/*.rs` | 单元、属性与 SDK 场景测试(用例编号与函数名见 [TESTS.md](TESTS.md)) |

</details>

## 延迟与资源契约

快速部分计入分段 `decided_tsc − proc_tsc` 内的策略份额;本策略产生的样本本身就测这一段,所以它的开销必须远小于 `decide` 预算。

| 入口 | p50 | p99 | 基准名 |
|---|---|---|---|
| `on_event` 快侧未命中(`Bbo` 未到期) | ≤ 15 ns | ≤ 40 ns | `strategies/latency-probe/tick_miss` |
| `on_event` 命中 → 一条订单意图(quote 或 pair 第一腿) | ≤ 150 ns | ≤ 500 ns | `strategies/latency-probe/tick_fire` |
| `on_event(Order{Acked})` → 撤单(本策略额外登记;计入 `rep_to_order` 的 sequencer 部分) | ≤ 50 ns | ≤ 150 ns | `strategies/latency-probe/ack_cancel` |
| `on_event` 慢侧更新(无慢侧:pair 组 B 腿的 `Bbo`,不重算触发价) | ≤ 60 ns | ≤ 150 ns | `strategies/latency-probe/slow_update` |
| `on_event(Advice)`(无 Model:不会收到,忽略) | ≤ 100 ns | ≤ 300 ns | `strategies/latency-probe/advice` |
| `on_event` → 构造 `TargetIntent`(target 组) | ≤ 250 ns | ≤ 800 ns | `strategies/latency-probe/target_set` |
| `on_event(Exec)`(target 组) | ≤ 80 ns | ≤ 250 ns | `strategies/latency-probe/exec_update` |
| `on_event(Risk)`,每个受影响 group | ≤ 150 ns | ≤ 400 ns | `strategies/latency-probe/risk_event` |
| `Model::on_event` 采样 / 模型 tick | 不适用(`NoModel`) | — | — |

- 零分配、无系统调用、无 `dyn`、无 f64;每 group 状态 ≤ 256 B。
- 下单速率:quote 每个探测点每秒 1 次挂单 + 1 次撤单(`burst_period_ms` 非 0 时突发层另按它提高);pair 每 group 每 `pair_period_ms`(默认 10 s)2 张第一腿(开、平)与对应第二腿;target 每 group 每 `target_period_ms` 两个目标,各产生每腿至少一笔切片或对冲单。全部受 hft-risk 令牌桶(refdata `RateLimitTable`)约束,超限即门控拒绝、计 `ProbeGated`。
- journal 量:每张 quote 探测单约 10 条记录(挂单与撤单各一组 `OrderDecision`、`SendReport`、OMS 迁移记录、回报与 `LatencySample`)≈ 1.2 KB,即每个探测点约 100 MB/天的 seq journal(待实测,方法:M1.5 第一天运行 1 h 后按 seq journal 字节数 ÷ 探测单数核算);dev2 上只在联调窗口启用、探测点按存储预算选择(容量归 [apps/hft-core](../../apps/hft-core/README.md))。

## 测试与验收

用例与验收见 [TESTS.md](TESTS.md)。

## 依赖

- 内部:[`hft-strategy-sdk`](../../crates/hft-strategy-sdk/README.md)、[`hft-strat-lib`](../../crates/hft-strat-lib/README.md)(`Throttle`)。
- 外部:无;dev 依赖 `proptest`、`criterion`。
- 禁止:任何引擎 crate、其他 `strategies/*`;I/O、`std::time`、`HashMap` 迭代、f64、`dyn Trait`。

## 延伸阅读

- [latency-budget](../../docs/latency-budget.md)(分段名、数据集分组、验收方法)、[testing §8](../../docs/testing.md#8-上线门槛)(门槛 ⑩)、[benchmarks](../../benchmarks/README.md)
- [11 实施路线图](../../design/11-实施路线图.md)(M1.5 延迟探针、M3 末满订阅探针)、[V5 延迟评审](../../design/_work/review-2026-10-03-M0c-V5延迟与整体.md)、[W5 延迟与整体评审](../../design/_work/review-2026-10-03-M0d-W5延迟与整体.md)
- Agent Note:[M1.5 延迟探针](../../.agents/notes/proposed/architecture/2026-10-03-latency-probe-milestone.md)、[延迟验收的证据](../../.agents/notes/proposed/process/2026-10-03-latency-acceptance-evidence.md)
- [hft-strategy-sdk](../../crates/hft-strategy-sdk/README.md)(`non_prod_only`、LegLink 与目标意图类型)、[hft-oms](../../crates/hft-oms/README.md)(第二腿)、[hft-exec](../../crates/hft-exec/README.md)(目标切片)、[strategies/AGENTS.md](../AGENTS.md)

## 已知限制与后续工作

- 测试网的 `order_rtt`、`tick_to_ack` 与撮合行为不代表生产;生产端点上的往返只能在正式实盘后由真实策略的样本观察(本策略不能绑定正式账户)。
- quote 模式的撤单样本(`action = Cancel`)只进 `rep_to_order`,不计 `order_rtt`,即不测撤单往返;需要撤单往返分段时另加(可选扩展,触发条件:做市类策略启用)。
- quote 模式按时间节奏挂单(每探测点每秒至多一张),突发时段的权重低于按事件加权;行情类分段的按事件加权统计归 hft-telemetry 的离线模块,不由本策略弥补;M1.5 与 M3 末探针的样本按"突发 / 平静"分层抽样与报告,规则归 [testing §8.2](../../docs/testing.md#82-延迟验收的测法)。
- pair 与 target 模式依赖测试网两所都有可成交的盘口;测试网盘口稀薄时 `PairNoFill` 与开仓目标 `Stuck` 偏多,`hedge_host` 样本按实际成交计,只作小样本确认。

### 开发备忘

<details><summary>给实现者</summary>

- 各 core venue 测试网的 PostOnly 穿价拒单码、下单/撤单限速、价格带(离标记价的最大偏离)是否允许 300 bp 外的挂单、最小名义:待核实(方法:M1.5 第一天在测试网逐所下单实测,结论写进对应 `venue-*` README 与 `VenueCaps`,必要时调整本策略默认值)。
- 测试网账户的 `RiskParams`(`max_px_dev`、`ioc_max_cross`、单笔名义上限、令牌桶)按"用途与运行要求"一节设定,写进联调窗口的配置样例。
- 实现顺序:params/check → quote(挂、撤、回报)→ 场景测试 → pair 阶段机 → target 阶段机 → 平仓与残余 → DST 目标 → 基准。

</details>
