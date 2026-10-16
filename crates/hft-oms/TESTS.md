---
component: hft-oms
prefix: OMS
spec: README.md
---

# hft-oms 测试规格

格式与规则见 [docs/tests](../../docs/tests/README.md)。

## 覆盖

| README 小节 | 用例 |
|---|---|
| 概要 | 不适用:非规范 |
| 目录 | 不适用:非规范 |
| 使用本组件 | 不适用:见子节 |
| 与 hft-exec 的边界 | OMS-205, OMS-124, OMS-236, OMS-250, OMS-248 |
| 入口(全部在 sequencer 线程调用,`&mut self`,无分配) | OMS-005–OMS-007, OMS-252, OMS-040–OMS-050, OMS-088, OMS-092, OMS-133, OMS-135, OMS-152, OMS-189, OMS-200, OMS-240, OMS-248, OMS-285–OMS-287, OMS-310, OMS-315, OMS-342–OMS-344, OMS-360, OMS-390, OMS-410 |
| 输出 `OmsOut`(容量固定,由 sequencer 预分配并在每个输入事件后清空) | OMS-020, OMS-024, OMS-109, OMS-202, OMS-330 |
| 消费与产生的事件 | OMS-040, OMS-055–OMS-068, OMS-070–OMS-074, OMS-080, OMS-130, OMS-142, OMS-250 |
| 配置项(`OmsConfig`) | OMS-001–OMS-004, OMS-024, OMS-091, OMS-110, OMS-111, OMS-132, OMS-137, OMS-138, OMS-140, OMS-150, OMS-161, OMS-164, OMS-170, OMS-189, OMS-194, OMS-195, OMS-204、OMS-459, OMS-227, OMS-263, OMS-320, OMS-334、OMS-465、OMS-466、OMS-484 |
| 实现说明 | 不适用:见子节 |
| 数据布局(草案,实现时以代码与编译期断言为准) | OMS-003, OMS-005, OMS-010–OMS-017, OMS-021, OMS-022, OMS-034, OMS-051, OMS-381, OMS-421 |
| ClientOrderId | OMS-030–OMS-035 |
| 状态机 | OMS-456, OMS-018, OMS-019, OMS-023, OMS-040, OMS-045–OMS-052, OMS-380 |
| 回报处理表 | OMS-051, OMS-055–OMS-069, OMS-143, OMS-380 |
| SendReport 处理 | OMS-070–OMS-077, OMS-380 |
| 延迟样本 | OMS-012, OMS-071, OMS-080–OMS-089 |
| REPORT_DECODE | OMS-090, OMS-091 |
| 恢复后的 gw 映射 | OMS-092, OMS-093 |
| 回报匹配与成交增量 | OMS-031, OMS-035, OMS-057, OMS-069, OMS-100–OMS-115, OMS-154, OMS-253, OMS-460、OMS-467、OMS-468、OMS-470–OMS-475、OMS-476–OMS-480、OMS-481–OMS-483 |
| 成交净量(B4) | OMS-067, OMS-120–OMS-124, OMS-184, OMS-404 |
| Unknown 收敛流程 | OMS-032, OMS-065, OMS-066, OMS-068, OMS-070, OMS-073, OMS-089, OMS-130–OMS-154, OMS-400, OMS-402, OMS-420 |
| 墓碑与迟到成交(C1) | OMS-013, OMS-148, OMS-160–OMS-164, OMS-287, OMS-322, OMS-462、OMS-469、OMS-485 |
| StaleOrderSweep(`Timer{Oms, Sweep}`,1 s) | OMS-132, OMS-161, OMS-164, OMS-170–OMS-172, OMS-207, OMS-271 |
| LegLink 与 HedgePlan | OMS-463, OMS-457, OMS-014, OMS-043, OMS-153, OMS-180, OMS-181, OMS-185, OMS-191, OMS-194, OMS-201, OMS-207, OMS-363 |
| OnFill | OMS-458, OMS-044, OMS-075–OMS-077, OMS-104, OMS-151, OMS-181–OMS-207, OMS-229, OMS-382–OMS-385, OMS-387, OMS-400、TK-106 |
| Simultaneous | OMS-199, OMS-210–OMS-214, OMS-386 |
| 对冲定价 | OMS-193, OMS-220–OMS-230, OMS-401, OMS-405 |
| 数量对齐 | OMS-182, OMS-235, OMS-236, OMS-382 |
| 风控所需的链接信息(B1) | OMS-202, OMS-240 |
| RISK_REDUCING 判定 | OMS-458, OMS-021, OMS-022, OMS-077, OMS-192, OMS-245, OMS-248, OMS-340, OMS-385 |
| 外来成交(B3) | OMS-107, OMS-108, OMS-110, OMS-111, OMS-113, OMS-250–OMS-253 |
| 冒烟单(SmokeOrder) | OMS-260–OMS-275 |
| 残余核销 | OMS-280–OMS-284 |
| LegTracker | OMS-007, OMS-016, OMS-046, OMS-130, OMS-186, OMS-251, OMS-280, OMS-290–OMS-295, OMS-365, OMS-381, OMS-383 |
| InflightBook | OMS-290, OMS-291, OMS-300–OMS-303, OMS-364, OMS-381 |
| Drain 静止判定(支持 C11) | OMS-305 |
| 恢复:停机时间不计与闸门续接 | OMS-464, OMS-206, OMS-272, OMS-310–OMS-323, OMS-332, OMS-403, OMS-410 |
| 输出扇出 | OMS-015, OMS-047, OMS-074, OMS-268, OMS-273, OMS-330–OMS-335, OMS-340–OMS-344, OMS-388 |
| 链上槽(optional) | 不适用:里程碑外 |
| 每次迁移产生的输出 | OMS-040, OMS-055, OMS-057, OMS-059, OMS-062, OMS-105, OMS-113, OMS-130, OMS-160, OMS-162, OMS-193, OMS-265, OMS-281, OMS-315, OMS-320, OMS-343 |
| Note 码表 | OMS-023, OMS-025, OMS-026, OMS-114, OMS-145, OMS-152, OMS-222, OMS-230, OMS-267, OMS-481 |
| 快照行格式(节 2、3、12;草案,实现时以代码与编译期断言为准) | OMS-017, OMS-086, OMS-274, OMS-333, OMS-360–OMS-366, OMS-389, OMS-390, OMS-461、OMS-469、OMS-470 |
| 规划源文件 | 不适用:非规范 |
| 延迟与资源契约 | OMS-006, OMS-440–OMS-455 |
| 测试与验收 | 不适用:非规范 |
| 依赖 | 不适用:非规范 |
| 延伸阅读 | 不适用:非规范 |
| 已知限制与后续工作 | 不适用:非规范 |
| 开发备忘 | 不适用:非规范 |

## 用例

公共夹具(用例在 `前置` 里按名字引用,并显式写出所依赖的参数;夹具值不代表各所的实际登记):

- **时间**:TSC 频率 1 GHz(`tsc_hz_milli = 1 000 000 000 000`,1 ms = 1 000 000 tick);用例里的时刻写成毫秒,`t = 0` 为该用例的起点。
- **定点**:BTC `AssetExp{px_exp: −2, qty_exp: −8}`(`Qty` raw = 值 × 10⁸,`Px` raw = 值 × 100);`Rate` 指数 −9(1 bp = 100 000);`Notional` 指数 −6。资产 id:BTC = 1、USDT = 2、BNB = 3。
- **venue**:`VS` = `BINANCE_SPOT`(2;`stale_guard = RecvWindow`,过期窗口 5 000 ms;`flags = POST_ONLY|IOC|BATCH_CANCEL`;`dms = None`)、`VX` = `OKX_SWAP`(5;`stale_guard = ExpireAt`,窗口 3 000 ms;`flags = AMEND|POST_ONLY|IOC|FOK|BATCH_CANCEL`;`dms = CountdownAccount`)、`VG` = `BITGET_UTA_USDT_FUT`(10;`stale_guard = None`;`flags = AMEND|POST_ONLY|IOC|FOK`;`dms = None`)。
- **账户**:`a1`(AccountId 1,Live,在 `VS`、`VX`、`VG` 各一张表)、`a2`(2,Live,只在 `VS`)、`s1`(3,Sim,只在 `VS`);venue 稠密序号 VS = 0、VX = 1、VG = 2,账户序号 a1 = 0、a2 = 1、s1 = 2,`n_accounts = 3`,所以 `table_ix(VS, a1) = 0`、`table_ix(VX, a1) = 3`、`table_ix(VG, a1) = 6`。策略实例 1 绑定 `a1`,实例 2 绑定 `a2`。
- **inst**:`X` = BTCUSDT@VS(InstId 11,现货;`lot = 0.00001`(raw 1 000)、`min_qty = 0.00001`、`min_notional = 5 USDT`、`tick = 0.01`(raw 1))、`X2` = ETHUSDT@VS(12)、`Y` = BTC-USDT-SWAP@VX(21,永续;`lot = 0.001`(raw 100 000)、`min_qty = 0.001`、`min_notional = 5`、`tick = 0.1`(raw 10))、`Z` = BTCUSDT@VG(31,永续;`lot = 0.001`、`tick = 0.1`)。
- **group**:`g1 = GroupId::compose(1, 1)`(0x0100_0001),腿 0 = (a1, X)、腿 1 = (a1, Y);`g2 = compose(1, 2)`,腿 0 = (a1, X)、腿 1 = (a1, Z)。
- **报价**:`X` bid 60000.00 / ask 60000.01;`Y` bid 59990.0 / ask 60010.0(mid 60000.0);连接健康(`conn_up = true`、`resync = false`、`conn_live_tsc = now`)。
- **lim**:`HedgePxLimits{conn_liveness_ms = 2000, rr_max_cross = 0.005}`(raw 5 000 000)。
- **链接 L1**:`g1` 上 OnFill 链接;第一腿买 X(tif 按用例);`HedgePlan{kind = OnFill, venue = VX, account = a1, inst = Y, side = Sell, ratio_num = 1, ratio_den = 1, px_mode = Cross, px_param = 0.001, max_slip = 0.001, on_fail = Retry, tif = Ioc, reduce_only = 0, limit_px = 0, residual = Defer}`;`hedge_ref_px = 60010.0`;`LegLink.tolerance = 0.02`。
- **槽 s**:a1@VS 上买 X 的槽,`qty = 0.01`(raw 1 000 000)、`px = 60000.00`(raw 6 000 000)、`filled = 0`、GTC、`origin = Strategy`、拥有者实例 1、`group = g1`;`engine_epoch = 7`;回报的 `(epoch, slot, gen)` 与槽相符。
- **配置 P0**(显式取值,与默认值无关):`slots = 64`(每表)、`links_per_account = 16`、`ack_timeout_ms = 2000`、`cancel_timeout_ms = 2000`、`ioc_timeout_ms = 1000`、`unknown_query_interval_ms = 1000`、`unknown_max_queries = 5`、`unknown_alarm_ms = 30000`、`out_quota = 64`、`not_found_grace_ms = {VS: 5500, VX: 3500, VG: 2000}`、`smoke_far_bp = 0.03`、`reuse_quarantine_ms = 5000`、`tombstone_gens = 2`、`tombstone_keep_ms = 3600000`、`sweep_batch = 256`、`max_out_per_event = 16`、`hedge_retry_max = 3`、`hedge_retry_min_ms = 100`、`hedge_deadline_ms = 3000`、`resume_hold_max_ms = 30000`、`foreign_fill_track = 64`、`ext_held_track = 64`、`decode_fail_down_n = 3`。用例的 `前置` 写"P0"再列出它依赖的那几项(覆盖 P0 的值以用例为准)。
- **下单连接**:`ConnState{VS/VX/VG, Order, Up}` 已到达(`conn_up = true`),除非另写。

### OMS-001 OmsConfig 各参数的默认值

- 规则:[配置项](README.md#配置项omsconfig)
- 级别:table
- 里程碑:M2
- 前置:`EngineConfig` blob 的 `oms` 段为空表;refdata 为公共夹具(`VS` 过期窗口 5 000 ms、`VX` 3 000 ms、`VG` 无过期保护)
- 实现:`crates/hft-oms/tests/config.rs::oms_001_defaults`

| 行 | 参数 | 期望默认值 |
|---|---|---|
| OMS-001.1 | `slots[(VS, a1)]` | 4096 |
| OMS-001.2 | `links_per_account` | 4096 |
| OMS-001.3 | `ack_timeout_ms[VS]` | 2000 |
| OMS-001.4 | `cancel_timeout_ms[VS]` | 2000 |
| OMS-001.5 | `ioc_timeout_ms[VS]` | 1000 |
| OMS-001.6 | `unknown_query_interval_ms` | 1000 |
| OMS-001.7 | `unknown_max_queries` | 5 |
| OMS-001.8 | `unknown_alarm_ms` | 30000 |
| OMS-001.9 | `out_quota[(VS, a1)]` | 64(= `max_out_per_event` 16 × 4) |
| OMS-001.10 | `not_found_grace_ms[VS]` | 5500(5 000 + 500) |
| OMS-001.11 | `not_found_grace_ms[VX]` | 3500(3 000 + 500) |
| OMS-001.12 | `not_found_grace_ms[VG]` | 2000(无过期保护) |
| OMS-001.13 | `smoke_far_bp` | `Rate(30 000 000)`(0.03) |
| OMS-001.14 | `reuse_quarantine_ms` | 5000 |
| OMS-001.15 | `tombstone_gens` | 2 |
| OMS-001.16 | `tombstone_keep_ms` | 3600000 |
| OMS-001.17 | `sweep_batch` | 256 |
| OMS-001.18 | `max_out_per_event` | 16 |
| OMS-001.19 | `hedge_retry_max` | 3 |
| OMS-001.20 | `hedge_retry_min_ms` | 100 |
| OMS-001.21 | `hedge_deadline_ms` | 3000 |
| OMS-001.22 | `resume_hold_max_ms` | 30000 |
| OMS-001.23 | `foreign_fill_track[(VS, a1)]` | 64 |
| OMS-001.24 | `ext_held_track[(VS, a1)]` | 64 |
| OMS-001.25 | `decode_fail_down_n` | 3 |

### OMS-002 每表槽数只接受 1..=65 535

- 规则:[配置项](README.md#配置项omsconfig)
- 级别:table
- 里程碑:M2
- 前置:P0;只改 `slots[(VS, a1)]`;调用 `Oms::new(&cfg, &rd)`
- 实现:`crates/hft-oms/tests/config.rs::oms_002_slots_range`

| 行 | `slots[(VS, a1)]` | 期望 |
|---|---|---|
| OMS-002.1 | 0 | `Err(OmsConfigError::SlotsOutOfRange{venue: VS, account: a1})` |
| OMS-002.2 | 1 | `Ok`;第一次 `place` 得 `slot_id = 1`,第二次 `Err(PlaceError::NoFreeSlot)` |
| OMS-002.3 | 65 534 | `Ok` |
| OMS-002.4 | 65 535 | `Ok`;依次 `place` 65 535 次,最后一槽 `slot_id = 65 535` |
| OMS-002.5 | 65 536 | `Err(OmsConfigError::SlotsOutOfRange{venue: VS, account: a1})` |

### OMS-003 (venue, account) 表数上限 256

- 规则:[配置项](README.md#配置项omsconfig)、[数据布局](README.md#数据布局草案实现时以代码与编译期断言为准)
- 级别:table
- 里程碑:M2
- 前置:P0;refdata 只改 venue 数 × 账户数(稠密网格,`table_ix = venue_ix × n_accounts + account_ix`)
- 实现:`crates/hft-oms/tests/config.rs::oms_003_table_count`
- 备注:257 是质数,无法由 ≤ 255 个账户的网格构成,+1 一侧取最小可构成的 258;hft-types 只定义 15 个 VenueId,网格用少 venue × 多账户

| 行 | 网格 | 期望 |
|---|---|---|
| OMS-003.1 | 3 × 85 = 255 | `Ok` |
| OMS-003.2 | 2 × 128 = 256 | `Ok`;最大 `table_ix = 255` |
| OMS-003.3 | 2 × 129 = 258 | `Err(OmsConfigError::TooManyTables)` |

### OMS-004 墓碑代数只接受 1..=4

- 规则:[配置项](README.md#配置项omsconfig)
- 级别:table
- 里程碑:M2
- 前置:P0;只改 `tombstone_gens`
- 实现:`crates/hft-oms/tests/config.rs::oms_004_tombstone_gens_range`

| 行 | `tombstone_gens` | 期望 |
|---|---|---|
| OMS-004.1 | 0 | `Err(OmsConfigError::TombGens)` |
| OMS-004.2 | 1 | `Ok` |
| OMS-004.3 | 4 | `Ok` |
| OMS-004.4 | 5 | `Err(OmsConfigError::TombGens)` |

### OMS-005 槽按 FIFO 空闲表分配,回收的槽排到队尾

- 规则:[数据布局](README.md#数据布局草案实现时以代码与编译期断言为准)
- 级别:unit
- 里程碑:M2
- 前置:P0;`slots[(VS, a1)] = 8`;`reuse_quarantine_ms = 5000`
- 步骤:
  1. 连续 `place` 8 张买 X 的 GTC 单(t = 0)
  2. 第 9 次 `place`
  3. 槽 3 与槽 5 依次收到 `CancelAck`(t = 10);t = 5011 调 `on_sweep`
  4. 再 `place` 两张
- 期望:
  - 步骤 1 的 `slot_id` 依次为 1..=8,`gen` 均为 1;任何一次都不是 0
  - 步骤 2 返回 `Err(PlaceError::NoFreeSlot)`,输出为空
  - 步骤 4 依次得到 `slot_id = 3, gen = 2` 与 `slot_id = 5, gen = 2`
- 实现:`crates/hft-oms/tests/table.rs::oms_005_fifo_free_list`

### OMS-006 构造之后运行期零堆分配

- 规则:[入口](README.md#入口全部在-sequencer-线程调用mut-self无分配)、[延迟与资源契约](README.md#延迟与资源契约)
- 级别:unit
- 里程碑:M2
- 前置:P0;计数型全局分配器;`Oms::new` 与 `on_groups_changed({g1, g2})` 之后把计数清零
- 步骤:
  1. 按固定种子跑 10 000 次调用,覆盖 `place`、`cancel`、`amend`、`on_order_event`、`on_send_report`、`on_conn_state`、`on_reconcile`、`on_sweep`、`on_unknown_query`、`on_hedge_retry`、`hedge_gated`、`on_continue`、`price_hedge`、`snapshot`(写入预分配缓冲)
- 期望:
  - 分配计数为 0,释放计数为 0
- 实现:`crates/hft-oms/tests/alloc.rs::oms_006_zero_alloc_runtime`

### OMS-007 SetParams 之后按新 GroupSpec 重建腿归属

- 规则:[入口](README.md#入口全部在-sequencer-线程调用mut-self无分配)、[LegTracker](README.md#legtracker)
- 级别:unit
- 里程碑:M2
- 前置:P0;初始 `on_groups_changed({g1})`
- 步骤:
  1. `on_groups_changed({g1, g2})`
  2. `place` 买 Z(a1@VG,`group = g2`,qty 0.002,IOC)并收到 `Fill{cum = 0.002}`(手续费 USDT)
- 期望:
  - 该槽 `SlotExt.leg_ix = 1`
  - `legs(a1, g2).leg[1].net = 0.002`(raw 200 000);`legs(a1, g1)` 各腿 `net` 均为 0
  - 步骤 1 期间分配计数为 0(容量预分配,只重写内容)
- 实现:`crates/hft-oms/tests/legs.rs::oms_007_groups_changed_rebuilds_map`

### OMS-010 OrderSlot 大小 128 B、对齐 64 B 与关键偏移

- 规则:[数据布局](README.md#数据布局草案实现时以代码与编译期断言为准)
- 级别:unit
- 里程碑:M2
- 前置:无运行时状态(编译期与 `offset_of!` 断言)
- 步骤:
  1. 读取 `size_of`、`align_of` 与各字段 `offset_of!`
- 期望:
  - `size_of::<OrderSlot>() = 128`,`align_of = 64`
  - 偏移:`state` 0、`prev_state` 3、`oflags` 4、`gen` 8、`slot_id` 12、`inst` 16、`query_count` 21、`epoch` 22、`group` 24、`link_id` 28、`qty` 32、`filled` 40、`px` 48、`exch_order_id` 56、`filled_notional` 64、`pending_px` 80、`fee_accum` 96、`fee_asset` 104、`sflags` 106、`reject_code` 108、`t_state` 112、`seq_last` 120
- 实现:`crates/hft-oms/tests/layout.rs::oms_010_order_slot_layout`

### OMS-011 SlotExt 大小 64 B 与关键偏移

- 规则:[数据布局](README.md#数据布局草案实现时以代码与编译期断言为准)
- 级别:unit
- 里程碑:M2
- 前置:无运行时状态
- 步骤:
  1. 读取 `size_of::<SlotExt>()`、`align_of` 与字段偏移
- 期望:
  - 大小 64、对齐 32;`net_filled` 0、`tag` 8、`leg_ix` 12、`role` 13、`origin` 14、`class` 15、`lev_x100` 16、`max_rest_ms` 20、`fee_cum` 32、`cash_qty` 40、`cash_sum` 48
- 实现:`crates/hft-oms/tests/layout.rs::oms_011_slot_ext_layout`

### OMS-012 SlotStamps 大小 80 B 与关键偏移

- 规则:[数据布局](README.md#数据布局草案实现时以代码与编译期断言为准)、[延迟样本](README.md#延迟样本)
- 级别:unit
- 里程碑:M2
- 前置:无运行时状态
- 步骤:
  1. 读取 `size_of::<SlotStamps>()` 与字段偏移
- 期望:
  - 大小 80;`trig_ex_ts_ns` 0、`trig_sw_rx_ns` 24、`trig_rx_tsc` 32、`decided_tsc` 48、`sent_tsc` 56、`trigger_seq` 64、`trig_kind` 72、`op` 74
- 实现:`crates/hft-oms/tests/layout.rs::oms_012_slot_stamps_layout`

### OMS-013 Tomb 大小 80 B 与关键偏移

- 规则:[数据布局](README.md#数据布局草案实现时以代码与编译期断言为准)、[墓碑与迟到成交](README.md#墓碑与迟到成交c1)
- 级别:unit
- 里程碑:M2
- 前置:无运行时状态
- 步骤:
  1. 读取 `size_of::<Tomb>()` 与字段偏移
- 期望:
  - 大小 80;`gen` 0、`epoch` 4、`oflags` 6、`inst` 8、`group` 12、`side` 16、`leg_ix` 17、`strategy` 18、`role` 19、`link_id` 20、`filled` 24、`net_filled` 32、`exch_order_id` 40、`freed_tsc` 48、`cash_qty` 56、`cash_sum` 64、`fee_cum` 72
- 实现:`crates/hft-oms/tests/layout.rs::oms_013_tomb_layout`

### OMS-014 LinkEntry 大小 80 B 与关键偏移

- 规则:[LegLink 与 HedgePlan](README.md#leglink-与-hedgeplan)
- 级别:unit
- 里程碑:M2
- 前置:无运行时状态
- 步骤:
  1. 读取 `size_of::<LinkEntry>()`、`align_of` 与字段偏移
- 期望:
  - 大小 80、对齐 8;`link_id` 0、`group` 4、`account` 8、`state` 9、`retries` 10、`flags` 11、`plan` 12、`first` 16、`held_ms` 28、`first_net` 32、`hedge_req` 40、`hedge_filled` 48、`hedge_open` 56、`first_fill_tsc` 64、`ref_px` 72
- 实现:`crates/hft-oms/tests/layout.rs::oms_014_link_entry_layout`

### OMS-015 OutItem 大小 32 B 与关键偏移

- 规则:[输出扇出](README.md#输出扇出)
- 级别:unit
- 里程碑:M2
- 前置:无运行时状态
- 步骤:
  1. 读取 `size_of::<OutItem>()` 与字段偏移
- 期望:
  - 大小 32;`kind` 0、`action` 1、`from` 2、`cause` 3、`link_id` 4、`cause_seq` 8、`slot` 16、`_pad` 28
- 实现:`crates/hft-oms/tests/layout.rs::oms_015_out_item_layout`

### OMS-016 LegState 大小 192 B 与关键偏移

- 规则:[LegTracker](README.md#legtracker)
- 级别:unit
- 里程碑:M2
- 前置:无运行时状态
- 步骤:
  1. 读取 `size_of::<LegState>()` 与字段偏移
- 期望:
  - 大小 192;`view` 0(`size_of::<LegView>() = 160`)、`tolerance` 160、`last_fill_tsc` 168、`last_link` 176、`broken_links` 180、`pending_slots` 182、`unknown_slots` 184
- 实现:`crates/hft-oms/tests/layout.rs::oms_016_leg_state_layout`

### OMS-017 快照行的大小断言

- 规则:[快照行格式](README.md#快照行格式节-2312草案实现时以代码与编译期断言为准)
- 级别:table
- 前置:无运行时状态;"行长"取 `snapshot()` 实际写出的字节数(对一张含 1 个非空槽、1 条链接、1 个 group、1 个墓碑、1 个 `OutItem` 的表)
- 里程碑:M2
- 实现:`crates/hft-oms/tests/layout.rs::oms_017_snapshot_row_sizes`

| 行 | 对象 | 期望 |
|---|---|---|
| OMS-017.1 | `TableRow` | `size_of = 32`,`smoke_slot` 偏移 28 |
| OMS-017.2 | `LinkRowHdr` | `size_of = 8` |
| OMS-017.3 | `OmsCountersHdr` | `size_of = 16`,`resume_hold_until` 偏移 8 |
| OMS-017.4 | 节 2 行 | `SLOT_ROW_LEN = 192`,写出 192 字节(128 + 64);`SectionHdr.version = 3` |
| OMS-017.5 | 节 12 `Link` 行体 | 80 字节 |
| OMS-017.6 | 节 12 `Plan` 行体 | 64 字节 |
| OMS-017.7 | 节 12 `Leg` 行体 | 200 字节 |
| OMS-017.8 | 节 12 `Tomb` 行体 | 80 字节 |
| OMS-017.9 | 节 12 `Out` 行体 | 32 字节 |

### OMS-018 OrderState 码值往返

- 规则:[状态机](README.md#状态机)
- 级别:table
- 里程碑:M2
- 前置:`OrderState::try_from(u8)` 与 `as u8`
- 实现:`crates/hft-oms/tests/codes.rs::oms_018_order_state_codes`

| 行 | 输入 | 期望 |
|---|---|---|
| OMS-018.1 | 0..=8 逐个 | 依次 `Free PendingNew Live PendingCancel PendingAmend Filled Cancelled Rejected Unknown`,再转回得到原值 |
| OMS-018.2 | 9 | `Err`(非法判别值) |
| OMS-018.3 | 255 | `Err` |

### OMS-019 TransitionCause 码值往返

- 规则:[状态机](README.md#状态机)
- 级别:table
- 里程碑:M2
- 前置:`TransitionCause::try_from(u8)` 与 `as u8`
- 实现:`crates/hft-oms/tests/codes.rs::oms_019_transition_cause_codes`

| 行 | 输入 | 期望 |
|---|---|---|
| OMS-019.1 | 1..=26 逐个 | 依次 `Place CancelReq AmendReq Ack Reject Fill CancelAck Expired CancelReject AmendAck AmendReject QueryResult Reconcile OutcomeUnknown ConnDown Timeout LocalSendFail WriteError ReportDecode RecoveryEnd ForceUnknown NotFoundExpire Recycle SimReset SmokeTtl FeeOnly`,转回原值 |
| OMS-019.2 | 0 | `Err` |
| OMS-019.3 | 27 | `Err` |

### OMS-020 LinkBroken 码值往返

- 规则:[输出 OmsOut](README.md#输出-omsout容量固定由-sequencer-预分配并在每个输入事件后清空)
- 级别:table
- 里程碑:M2
- 前置:`LinkBroken::try_from(u8)`
- 实现:`crates/hft-oms/tests/codes.rs::oms_020_link_broken_codes`

| 行 | 输入 | 期望 |
|---|---|---|
| OMS-020.1 | 0..=6 逐个 | 依次 `None RetryExhausted Deadline HedgeLimit Overfill ReverseFailed ResumeTimeout`,转回原值 |
| OMS-020.2 | 7 | `Err` |

### OMS-021 私有 SlotFlags 从不出现在 OrderDecision 与 GwCmd 上

- 规则:[RISK_REDUCING 判定](README.md#risk_reducing-判定)、[数据布局](README.md#数据布局草案实现时以代码与编译期断言为准)
- 级别:unit
- 里程碑:M2
- 前置:P0;槽 s 为 `Live`,`oflags = POST_ONLY`(0x0002);测试钩子把 `sflags` 置为 0x07FF 去掉 `CANCEL_SENT`、`AMEND_SENT`(其余全部已定义位;这两位置上时撤改单不再发出)
- 步骤:
  1. `cancel(s)`;再对 `VX` 上一张 `Live` 槽 `amend`;再 `place` 一张新单
- 期望:
  - 三条 `OrderDecision` 的 `oflags & 0xFF00 = 0`,且与各自槽的 `oflags` 逐位相等(撤改单 = 目标槽的 `oflags`)
  - `SlotFlags` 各常量值为 `CANCEL_SENT = 1`、`AMEND_SENT = 2`、`FILL_BEFORE_ACK = 4`、`VERIFY = 8`、`QUEUED = 16`、`EXEC = 32`、`SMOKE_DUE = 64`、`ACKED = 128`、`HIST_WAIT = 256`、`ALARMED = 512`、`AGED = 1024`,两两不重叠
- 实现:`crates/hft-oms/tests/flags.rs::oms_021_slot_flags_never_on_wire`

### OMS-022 槽与决策的线上标志位值取自 hft-event OrderFlags

- 规则:[RISK_REDUCING 判定](README.md#risk_reducing-判定)、[数据布局](README.md#数据布局草案实现时以代码与编译期断言为准)
- 级别:unit
- 里程碑:M2
- 前置:P0;`ApprovedIntent{tif = PostOnly, risk_reducing = true, reduce_only = true}`(永续 Z,a1@VG)
- 步骤:
  1. `place`
- 期望:
  - 槽与 `OrderDecision` 的 `oflags = hft_event::OrderFlags::{REDUCE_ONLY | POST_ONLY | RISK_REDUCING}.0 = 0x0013`,两者逐位相等
  - 本 crate 的公开项中没有名为 `REDUCE_ONLY`、`POST_ONLY`、`LINKED`、`HEDGE_FIX`、`RISK_REDUCING`、`SMOKE` 的常量(由 `cargo xtask doc-check` 的符号扫描与本测试的 `oflags` 常量来源断言共同检查:`OrderFlags` 类型路径为 `hft_event::OrderFlags`)
- 实现:`crates/hft-oms/tests/flags.rs::oms_022_oflags_from_hft_event`

### OMS-023 OmsTransition 的 v[] 打包与码表一致

- 规则:[Note 码表](README.md#note-码表)、[状态机](README.md#状态机)
- 级别:unit
- 里程碑:M2
- 前置:P0;槽 s(`slot_id = 1`、`gen = 1`)处于 `PendingNew`,`exch_order_id = 0`
- 步骤:
  1. 投递 `OrderEvent{what = Ack, exch_order_id = 777}`,`hdr.seq = 900`
- 期望:
  - 恰好一条 `Note{OmsTransition}`,`level = Info`,`inst = 11`,`group = g1`
  - `v[0] = 2 | 1 << 8 | 1 << 16 | 2 << 24 | 4 << 32 = 17 213 489 410`(venue 2、account 1、from PendingNew 1、to Live 2、cause Ack 4)
  - `v[1..8] = [1, 1, 900, 0, 1 000 000, 6 000 000, 777]`
- 实现:`crates/hft-oms/tests/notes.rs::oms_023_transition_note_packing`

### OMS-024 OmsTimerReq.id 的编码在两端取值处往返

- 规则:[输出 OmsOut](README.md#输出-omsout容量固定由-sequencer-预分配并在每个输入事件后清空)、[配置项](README.md#配置项omsconfig)
- 级别:table
- 里程碑:M2
- 前置:P0;编码函数与 `on_unknown_query`/`on_hedge_retry`/`on_smoke_ttl` 的解码
- 实现:`crates/hft-oms/tests/timers.rs::oms_024_timer_id_roundtrip`

| 行 | 输入 | 期望 |
|---|---|---|
| OMS-024.1 | `UnknownQuery{table_ix = 0, slot_id = 1}` | `id = 256`;解回 (0, 1) |
| OMS-024.2 | `UnknownQuery{table_ix = 255, slot_id = 65 535}` | `id = 16 777 215`;解回 (255, 65 535) |
| OMS-024.3 | `UnknownQuery{table_ix = 3, slot_id = 2}` | `id = 515`;`on_unknown_query(515)` 作用于 (VX, a1) 表的槽 2 |
| OMS-024.4 | `HedgeRetry{link_id = 9}` | `id = 9` |
| OMS-024.5 | `SmokeTtl{account = 1, venue = 5}` | `id = 1 \| 5 << 8 = 1281`;解回 (a1, VX) |

### OMS-025 Oms 源 Note 码表与 hft-event 登记表一一对应

- 规则:[Note 码表](README.md#note-码表)
- 级别:table
- 里程碑:M2
- 前置:本 crate 的 `const` 码表;hft-event 登记表由 `gen-schema` 导出的 JSON
- 实现:`crates/hft-oms/tests/notes.rs::oms_025_note_codes_registered`

| 行 | 检查 | 期望 |
|---|---|---|
| OMS-025.1 | 码名集合 | 恰为 23 个:`OmsTransition OmsUnknown OmsUnknownStuck OmsVenueReady OmsNeedsRestQuery OmsStaleGenFill OmsLateFill OmsOverfill OmsForeignOrder OmsForeignFillViaOrderEvent OmsForeignFill OmsExtDedup OmsFillPxEstimated OmsFeeUnattributed OmsCancelNotSent OmsHedgeBroken OmsHedgePxStale OmsHedgePxGated OmsHedgeSlipCapped OmsHedgeNakedCapped OmsResumeRetry OmsResumeTimeout OmsAnomaly`;与登记表中 `source = Oms` 的名字集合相等 |
| OMS-025.2 | `n` 唯一 | 23 个 `n` 两两不同;`Note.code = 4 << 24 \| n` |
| OMS-025.3 | 默认级别 | P0:`OmsUnknownStuck OmsStaleGenFill OmsLateFill OmsOverfill OmsHedgeBroken OmsResumeTimeout`;P1:`OmsUnknown OmsForeignOrder OmsForeignFillViaOrderEvent OmsExtDedup OmsFillPxEstimated OmsFeeUnattributed OmsHedgePxStale OmsHedgePxGated OmsHedgeSlipCapped OmsHedgeNakedCapped OmsResumeRetry OmsAnomaly`;P2:`OmsCancelNotSent`;Info:`OmsTransition OmsVenueReady OmsNeedsRestQuery OmsForeignFill` |

### OMS-026 OmsAnomaly 的种类码

- 规则:[Note 码表](README.md#note-码表)
- 级别:table
- 里程碑:M2
- 前置:P0;各行的触发见"期望"中的用例编号
- 实现:`crates/hft-oms/tests/notes.rs::oms_026_anomaly_kinds`

| 行 | 触发 | 期望 |
|---|---|---|
| OMS-026.1 | 回报处理表中 Reject 与已有成交矛盾(OMS-056.2) | `Note{OmsAnomaly}`,`level = P1`,`v[0] = 1`(Table) |
| OMS-026.2 | `foreign` 表 FIFO 淘汰(OMS-110.2) | `level = P1`,`v[0] = 2`(ForeignEvict) |
| OMS-026.3 | 冒烟单成交(OMS-267) | `level = P0`,`v[0] = 3`(SmokeFilled) |
| OMS-026.4 | `ReconcileReport` 条目被忽略(OMS-145) | `level = P1`,`v[0] = 4`(ReconIgnored) |

### OMS-030 槽的 epoch 取 engine_epoch 的低 15 位,保留位恒 0

- 规则:[ClientOrderId](README.md#clientorderid)
- 级别:table
- 里程碑:M2
- 前置:P0;按行设定 `engine_epoch` 后 `Oms::new`,再 `place` 一张买 X
- 实现:`crates/hft-oms/tests/cid.rs::oms_030_epoch_low15`

| 行 | `engine_epoch` | 期望 |
|---|---|---|
| OMS-030.1 | 7 | `slot.epoch = 7`;`ClientOrderId{epoch: 7, venue: 2, account: 1, slot: 1, gen: 1}` 经 hft-types `encode` 后 `decode` 往返相等 |
| OMS-030.2 | 0x7FFF | `slot.epoch = 0x7FFF` |
| OMS-030.3 | 0x8000 | `slot.epoch = 0`(bit 15 恒 0) |
| OMS-030.4 | 0x8005 | `slot.epoch = 5` |

### OMS-031 回报按 (epoch, slot, gen) 匹配,上一 epoch 的槽只被该 epoch 的回报命中

- 规则:[ClientOrderId](README.md#clientorderid)、[回报匹配与成交增量](README.md#回报匹配与成交增量)
- 级别:table
- 里程碑:M2
- 前置:P0;从快照恢复,快照节 3 `engine_epoch = 4`,本次运行 `engine_epoch = 9`;恢复的槽 r(`slot_id = 2`、`gen = 3`、`epoch = 4`)处于 `Unknown`;新下的槽 n(`slot_id = 5`、`gen = 1`、`epoch = 9`)处于 `PendingNew`;两槽都无墓碑
- 实现:`crates/hft-oms/tests/cid.rs::oms_031_match_by_epoch`

| 行 | 回报 | 期望 |
|---|---|---|
| OMS-031.1 | `QueryResult{epoch 4, slot 2, gen 3, detail New}` | r → `Live` |
| OMS-031.2 | `Ack{epoch 9, slot 2, gen 3}` | 不匹配 r;无墓碑 → `StaleGen`(无成交):r 不变,`OmsStats.stale_gen` +1,无 Note |
| OMS-031.3 | `Ack{epoch 9, slot 5, gen 1}` | n → `Live` |
| OMS-031.4 | `Ack{epoch 4, slot 5, gen 1}` | 不匹配 n;`StaleGen` 计数,n 保持 `PendingNew` |

### OMS-032 Unknown 的槽从不重发 New,只出现在 Query 与撤单里

- 规则:[ClientOrderId](README.md#clientorderid)、[Unknown 收敛流程](README.md#unknown-收敛流程)
- 级别:unit
- 里程碑:M2
- 前置:P0;槽 s 因 `SendReport{Place, WriteError}` 进入 `Unknown`(t = 0);`unknown_query_interval_ms = 1000`
- 步骤:
  1. 依次调用 `on_unknown_query`(t = 1000、2000、3000)、`on_conn_state(Order Down)`、`on_conn_state(Order Up)`、`on_sweep`(t = 3500)
  2. 投递 `QueryResult{detail = NotFound}`(t = 6000,超过 grace 5500),再投递 `ReconcileReport{UnknownResolved, NotFound, history_checked = 1}`
  3. 拥有者再 `place` 一张同价同量的买 X
- 期望:
  - 步骤 1、2 的全部输出中 `OrderDecision.action ∈ {Query}`,`slot`/`target_slot` 指向 s 的没有一条是 `New`
  - 步骤 2 后 s 为 `Rejected`、`reject_code = 0`
  - 步骤 3 的 `OrderDecision{New}` 的 `(slot, gen)` 与 s 的 `(slot, gen)` 不同
- 实现:`crates/hft-oms/tests/unknown.rs::oms_032_unknown_never_renews`

### OMS-033 规范 client id 满足各所字段长度与字符集

- 规则:[ClientOrderId](README.md#clientorderid)
- 级别:table
- 里程碑:M2
- 前置:`ClientOrderId{epoch: 0x7FFF, venue: 14, account: 255, slot: 65 535, gen: u32::MAX}` 经 hft-types `encode` 得 24 字符;各行按表中"我们发出的形式"加前缀
- 实现:`crates/hft-oms/tests/cid.rs::oms_033_cid_fits_venue_limits`

| 行 | venue 字段 | 期望 |
|---|---|---|
| OMS-033.1 | Binance `newClientOrderId` | 长度 24 ≤ 36;字符全在 `[0-9a-f]` |
| OMS-033.2 | OKX `clOrdId` | 长度 24 ≤ 32;全为字母数字 |
| OMS-033.3 | Bitget `clientOid` | 长度 24 ≤ 40 |
| OMS-033.4 | Bybit `orderLinkId` | 长度 24 ≤ 36 |
| OMS-033.5 | Gate `text` | `t-` + 24 = 26;去前缀 24 ≤ 28 |
| OMS-033.6 | KuCoin `clientOid` | 长度 24 ≤ 40 |
| OMS-033.7 | Hyperliquid `cloid` | `0x` + `00000001` + 24 = 34;去 `0x` 后 32 个十六进制字符(128 位) |

### OMS-034 槽每次分配代数加 1

- 规则:[数据布局](README.md#数据布局草案实现时以代码与编译期断言为准)、[ClientOrderId](README.md#clientorderid)
- 级别:unit
- 里程碑:M2
- 前置:P0;`slots[(VS, a1)] = 1`;`reuse_quarantine_ms = 5000`
- 步骤:
  1. `place`(得槽 1);`CancelAck`(t = 0);`on_sweep`(t = 5001);再 `place`;重复三轮
- 期望:
  - 四次 `place` 的 `gen` 依次为 1、2、3、4,`slot_id` 均为 1;每轮回收后 `Tomb[gen % 2]` 的 `gen` 为上一代
- 实现:`crates/hft-oms/tests/table.rs::oms_034_gen_increments`

### OMS-035 无成交的外来单回报只计数并写 OmsForeignOrder

- 规则:[ClientOrderId](README.md#clientorderid)、[回报匹配与成交增量](README.md#回报匹配与成交增量)
- 级别:unit
- 里程碑:M2
- 前置:P0;a1@VS 表有槽 s(`Live`)
- 步骤:
  1. 投递 `OrderEvent{slot = 0 (SLOT_UNRESOLVED), what = Ack, exch_order_id = 555, cum_filled = 0}`
- 期望:
  - 恰好一条 `Note{OmsForeignOrder}`,`level = P1`;无 `FillOut`、`ForeignFill`、`OrderDecision`
  - s 与所有槽状态不变;外来单计数 +1
- 实现:`crates/hft-oms/tests/matching.rs::oms_035_foreign_order_no_fill`

### OMS-040 place 分配槽并产出 New 决策与在途

- 规则:[入口](README.md#入口全部在-sequencer-线程调用mut-self无分配)、[状态机](README.md#状态机)、[每次迁移产生的输出](README.md#每次迁移产生的输出)
- 级别:unit
- 里程碑:M2
- 前置:P0;`ApprovedIntent{origin = Strategy, 买 X, qty 0.01, px 60000.00, GTC, group g1, class Open, risk_reducing = false, tag = 42, trig.kind = 0x0101}`,`now = 1000`,`cause_seq = 500`
- 步骤:
  1. `place`
- 期望:
  - 返回 `Ok(SlotRef{venue 2, account 1, slot 1, gen 1})`
  - `OrderDecision{action = New, slot 1, gen 1, px 6 000 000, qty 1 000 000, group g1, inst 11, target_slot 0, reject 0, reason 0, link_role 0, oflags = 0, trigger_seq = 500}` 一条
  - `Note{OmsTransition}` 一条:from Free(0)、to PendingNew(1)、cause Place(1)
  - 槽 `state = PendingNew`、`t_state = 1000 ms`、`SlotExt{tag 42, origin 1, class 1, leg_ix 0}`、`SlotStamps.op = 0`(New)
  - InflightBook:`open_buy(a1, X)` 与 `imm_buy(a1, X)` 各 +0.01;`buy_notional(a1, VS)` +600.00 USDT;g1 腿 0 `inflight_immediate` +0.01
- 实现:`crates/hft-oms/tests/fsm.rs::oms_040_place_allocates`

### OMS-041 空闲表为空时 place 返回 NoFreeSlot 且不改任何状态

- 规则:[入口](README.md#入口全部在-sequencer-线程调用mut-self无分配)
- 级别:unit
- 里程碑:M2
- 前置:P0;`slots[(VS, a1)] = 2`,两槽都 `Live`
- 步骤:
  1. `place` 第三张
- 期望:
  - `Err(PlaceError::NoFreeSlot)`;`OmsOut` 各向量为空;InflightBook、LegTracker、空闲表长度与调用前逐字节相同
- 实现:`crates/hft-oms/tests/fsm.rs::oms_041_place_no_free_slot`

### OMS-042 refdata 中没有的 (venue, account) 返回 NoTable

- 规则:[入口](README.md#入口全部在-sequencer-线程调用mut-self无分配)
- 级别:unit
- 里程碑:M2
- 前置:P0;`a2` 只在 `VS` 有表
- 步骤:
  1. `place` 一张 `a2@VG` 的单
- 期望:
  - `Err(PlaceError::NoTable)`;输出为空
- 实现:`crates/hft-oms/tests/fsm.rs::oms_042_place_no_table`

### OMS-043 LinkTable 满时带链接的第一腿返回 LinkTableFull

- 规则:[LegLink 与 HedgePlan](README.md#leglink-与-hedgeplan)、[入口](README.md#入口全部在-sequencer-线程调用mut-self无分配)
- 级别:unit
- 里程碑:M2
- 前置:P0;`links_per_account = 1`;已有一条 `Open` 的链接
- 步骤:
  1. `place` 一张带 L1 计划的第一腿
  2. `place` 一张不带链接的单
- 期望:
  - 步骤 1 `Err(PlaceError::LinkTableFull)`,空闲表长度不变、输出为空
  - 步骤 2 `Ok`
- 实现:`crates/hft-oms/tests/link.rs::oms_043_link_table_full`

### OMS-044 本 crate 对冲请求 place 失败时链接记一次执行失败

- 规则:[入口](README.md#入口全部在-sequencer-线程调用mut-self无分配)、[OnFill](README.md#onfill)
- 级别:unit
- 里程碑:M2
- 前置:P0;L1 处于 `Hedging`,`retries = 0`,`owed = 0.012`;a1@VX 表空闲表为空;`hedge_retry_min_ms = 100`;`now = 1000`
- 步骤:
  1. `place(ApprovedIntent{origin = Oms, link L1, role Second, 卖 Y 0.012})`
- 期望:
  - `Err(PlaceError::NoFreeSlot)`;无 `OrderDecision`
  - L1 `retries = 1`;`timers` 恰好一条 `OmsTimerReq{HedgeRetry, id = L1, deadline = 1100 ms, cancel = false}`
- 实现:`crates/hft-oms/tests/link.rs::oms_044_hedge_place_err_counts`

### OMS-045 拥有者撤 Live 单进入 PendingCancel

- 规则:[状态机](README.md#状态机)
- 级别:unit
- 里程碑:M2
- 前置:P0;槽 s `Live`(ACKED,`exch_order_id = 777`)
- 步骤:
  1. `cancel(s, trig, now = 2000, cause_seq = 600)`
- 期望:
  - `Ok`;s `PendingCancel`,置 `CANCEL_SENT`
  - `OrderDecision{action = Cancel, target_slot = s.slot, gen = s.gen}` 一条;`OmsTransition{from 2, to 3, cause CancelReq(2)}` 一条
  - InflightBook 与 LegTracker 不变(挂单在途仍计)
- 实现:`crates/hft-oms/tests/fsm.rs::oms_045_cancel_live`

### OMS-046 撤 PendingNew 的单进入 PendingCancel 并仍按即时在途计

- 规则:[状态机](README.md#状态机)、[LegTracker](README.md#legtracker)
- 级别:unit
- 里程碑:M2
- 前置:P0;槽 s `PendingNew`(未置 ACKED)
- 步骤:
  1. `cancel(s)`
  2. 投递 `Ack{exch_order_id = 777}`
  3. 投递 `CancelAck`
- 期望:
  - 步骤 1 后 s `PendingCancel`;g1 腿 0 `inflight_immediate = 0.01`、`inflight_resting = 0`
  - 步骤 2 后仍 `PendingCancel`,`exch_order_id = 777`,置 ACKED;`inflight_immediate = 0`、`inflight_resting = 0.01`
  - 步骤 3 后 `Cancelled`,两类在途均为 0
- 实现:`crates/hft-oms/tests/fsm.rs::oms_046_cancel_pending_new`

### OMS-047 撤 PendingAmend 的单进入 PendingCancel

- 规则:[状态机](README.md#状态机)、[输出扇出](README.md#输出扇出)
- 级别:unit
- 里程碑:M2
- 前置:P0;a1@VX 的槽 t(卖 Y 0.01,GTC)`PendingAmend`(`AMEND_SENT`,`pending_px = 60100.0`)
- 步骤:
  1. `cancel(t)`
- 期望:
  - `Ok`;t `PendingCancel`,`CANCEL_SENT` 与 `AMEND_SENT` 都置位;`OrderDecision{Cancel}` 一条
- 实现:`crates/hft-oms/tests/fsm.rs::oms_047_cancel_pending_amend`

### OMS-048 不可撤的状态与过期句柄被拒且无输出

- 规则:[状态机](README.md#状态机)
- 级别:table
- 里程碑:M2
- 前置:P0;槽 s 按行置于给定状态;每行调用 `cancel`
- 实现:`crates/hft-oms/tests/fsm.rs::oms_048_cancel_rejected_states`

| 行 | 槽状态 / 句柄 | 期望 |
|---|---|---|
| OMS-048.1 | `PendingCancel` | `Err(OpError::NotAllowed)`;输出为空 |
| OMS-048.2 | `Unknown` | `Err(OpError::NotAllowed)` |
| OMS-048.3 | `Filled` | `Err(OpError::NotAllowed)` |
| OMS-048.4 | `Cancelled` | `Err(OpError::NotAllowed)` |
| OMS-048.5 | `Rejected` | `Err(OpError::NotAllowed)` |
| OMS-048.6 | `Live`,句柄 `gen = s.gen − 1` | `Err(OpError::StaleRef)` |
| OMS-048.7 | 句柄指向 `Free` 槽 | `Err(OpError::StaleRef)` |

### OMS-049 改单 Live 单进入 PendingAmend

- 规则:[状态机](README.md#状态机)
- 级别:unit
- 里程碑:M2
- 前置:P0;a1@VX 槽 t(卖 Y 0.01 @ 60100.0,GTC)`Live`;`VX.flags` 含 `AMEND`
- 步骤:
  1. `amend(t, px = 60200.0, qty = 0.008)`
- 期望:
  - `Ok`;t `PendingAmend`,`pending_px = 60200.0`、`pending_qty = 0.008`,置 `AMEND_SENT`;`px`/`qty` 仍为 60100.0/0.01
  - `OrderDecision{action = Amend, px 6 020 000, qty 800 000, target_slot = t.slot}` 一条;`OmsTransition{2 → 4, AmendReq(3)}`
- 实现:`crates/hft-oms/tests/fsm.rs::oms_049_amend_live`

### OMS-050 不可改单的情形被拒且无输出

- 规则:[状态机](README.md#状态机)
- 级别:table
- 里程碑:M2
- 前置:P0;每行一个槽,调用 `amend(px = 60200.00, qty = 0.008)`
- 实现:`crates/hft-oms/tests/fsm.rs::oms_050_amend_rejected_states`

| 行 | 槽 | 期望 |
|---|---|---|
| OMS-050.1 | s(VS,`flags` 无 `AMEND`)`Live` | `Err(OpError::NotAllowed)`;输出为空 |
| OMS-050.2 | t(VX)`PendingNew` | `Err(OpError::NotAllowed)` |
| OMS-050.3 | t `PendingCancel` | `Err(OpError::NotAllowed)` |
| OMS-050.4 | t `PendingAmend` | `Err(OpError::NotAllowed)` |
| OMS-050.5 | t `Unknown` | `Err(OpError::NotAllowed)` |
| OMS-050.6 | t `Filled` | `Err(OpError::NotAllowed)` |
| OMS-050.7 | t `Live`,句柄代数过期 | `Err(OpError::StaleRef)` |

### OMS-051 改单减量到已成交量以下时 qty 取已成交量并迁 Filled

- 规则:[数据布局](README.md#数据布局草案实现时以代码与编译期断言为准)、[回报处理表](README.md#回报处理表)
- 级别:unit
- 里程碑:M2
- 前置:P0;t(VX,卖 Y,`qty = 0.01`)`PendingAmend`,`filled = 0.009`,`pending_qty = 0.008`
- 步骤:
  1. 投递 `AmendAck{px 60100.0, qty 0.008, cum_filled 0.009}`
- 期望:
  - t `qty = 0.009`、`state = Filled`;`OmsTransition{4 → 5, AmendAck}`;InflightBook 中 t 的剩余量为 0
- 实现:`crates/hft-oms/tests/fsm.rs::oms_051_amend_below_filled`

### OMS-052 部分成交仍是 Live,不另设状态

- 规则:[状态机](README.md#状态机)
- 级别:unit
- 里程碑:M2
- 前置:P0;槽 s `Live`
- 步骤:
  1. 投递 `PartialFill{cum 0.004, avg_px 60000.00}`
- 期望:
  - s `state = Live`(码 2)、`filled = 0.004`;`OrderStateView` 投影为 `Live`
- 实现:`crates/hft-oms/tests/fsm.rs::oms_052_partial_is_live`

### OMS-055 回报处理表:Ack 行

- 规则:[回报处理表](README.md#回报处理表)
- 级别:table
- 里程碑:M2
- 前置:P0;槽 s(`qty 0.01`、GTC、未链接);每行用测试钩子把 s 置于"状态"列,事件 `OrderEvent{what = Ack, exch_order_id = 777}`
- 实现:`crates/hft-oms/tests/fsm_table.rs::oms_055_ack_row`

| 行 | 状态 | 期望 |
|---|---|---|
| OMS-055.1 | `PendingNew`(`exch_order_id = 0`) | → `Live`;`exch_order_id = 777`;置 ACKED;`OmsTransition{1 → 2, Ack(4)}`;`OrderUpdate{Acked}`;`imm_buy` −0.01、`open_buy` 不变;腿 0 即时 −0.01、挂单 +0.01 |
| OMS-055.2 | `Live`(ACKED) | 输出为空;`OmsStats.dup` +1 |
| OMS-055.3 | `PendingCancel`(由 PendingNew 撤单,未置 ACKED) | 保持 `PendingCancel`;`exch_order_id = 777`;置 ACKED;无 `OmsTransition`;腿 0 即时 −0.01、挂单 +0.01 |
| OMS-055.4 | `PendingAmend`(ACKED,`exch_order_id = 777`) | 保持;输出为空 |
| OMS-055.5 | `Unknown`(`prev_state = PendingNew`) | → `Live`;`unknown_count` −1;`OmsTransition{8 → 2, Ack}`;`OrderUpdate{Acked}`;腿 0 即时 −0.01、挂单 +0.01 |
| OMS-055.6 | `Filled` | 输出为空;`OmsStats.dup` +1 |
| OMS-055.7 | `Cancelled` | 输出为空;`OmsStats.dup` +1 |
| OMS-055.8 | `Rejected` | 输出为空;`OmsStats.dup` +1 |

### OMS-056 回报处理表:Reject 行

- 规则:[回报处理表](README.md#回报处理表)
- 级别:table
- 里程碑:M2
- 前置:P0;同 OMS-055;事件 `OrderEvent{what = Reject, reject_code = 2010}`
- 实现:`crates/hft-oms/tests/fsm_table.rs::oms_056_reject_row`

| 行 | 状态 | 期望 |
|---|---|---|
| OMS-056.1 | `PendingNew` | → `Rejected`;`reject_code = 2010`;`OmsTransition{1 → 7, Reject(5)}`;`OrderUpdate::Rejected{bits: 0, venue_code: 2010}`;`open_buy`、`imm_buy` 各 −0.01;腿 0 即时 −0.01 |
| OMS-056.2 | `Live` | 零成交 → `Rejected`,释放在途、通知拥有者;有成交证据时保留成交及剩余在途并告警,新累计增量只记一次 |
| OMS-056.3 | `PendingCancel` | 零成交 → `Rejected`;有成交同 OMS-056.2 |
| OMS-056.4 | `PendingAmend` | 零成交 → `Rejected`;有成交同 OMS-056.2 |
| OMS-056.5 | `Unknown`(prev `PendingNew`) | → `Rejected`;`unknown_count` −1;`OrderUpdate::Rejected{bits: 0, venue_code: 2010}` |
| OMS-056.6 | `Filled` | 输出为空;`OmsStats.dup` +1 |
| OMS-056.7 | `Cancelled` | 输出为空;`OmsStats.dup` +1 |
| OMS-056.8 | `Rejected` | 输出为空;`OmsStats.dup` +1 |

### OMS-057 回报处理表:PartialFill 行

- 规则:[回报处理表](README.md#回报处理表)、[回报匹配与成交增量](README.md#回报匹配与成交增量)
- 级别:table
- 里程碑:M2
- 前置:P0;同 OMS-055;事件 `OrderEvent{what = PartialFill, cum_filled 0.004, avg_px 60000.00, fee 0.24, fee_asset USDT}`;除 .11 外槽 `filled = 0`
- 实现:`crates/hft-oms/tests/fsm_table.rs::oms_057_partial_fill_row`

| 行 | 状态 | 期望 |
|---|---|---|
| OMS-057.1 | `PendingNew` | → `Live`;置 `FILL_BEFORE_ACK` 与 ACKED;`FillOut{qty 0.004, net_qty 0.004, px 60000.00}`;`OrderUpdate{Acked}` 后接 `OrderUpdate::Filled{d 0.004, cum 0.004, done false}`;`OmsTransition{1 → 2, Fill(6)}` |
| OMS-057.2 | `Live` | `filled = 0.004`,保持 `Live`;`FillOut` 一条;`OmsTransition{2 → 2, Fill}`(`v[4]` = 400 000) |
| OMS-057.3 | `PendingCancel` | 应用增量,保持 `PendingCancel`;`FillOut` 一条 |
| OMS-057.4 | `PendingAmend` | 应用增量,保持 `PendingAmend`;`FillOut` 一条 |
| OMS-057.5 | `Unknown`(prev `PendingNew`) | → `Live`;`unknown_count` −1;`FillOut` 一条 |
| OMS-057.6 | `Unknown`(prev `Live`) | → `Live` |
| OMS-057.7 | `Unknown`(prev `PendingCancel`) | → `PendingCancel`;`unknown_count` −1 |
| OMS-057.8 | `Unknown`(prev `PendingAmend`) | → `PendingAmend` |
| OMS-057.9 | `Cancelled` | `filled = 0.004`,保持 `Cancelled`;`FillOut` 一条;`OmsAnomaly{Table}` 一条;`OmsNeedsRestQuery`(`trades = 1`,指向 s)一条 |
| OMS-057.10 | `Rejected` | 同 .9,保持 `Rejected` |
| OMS-057.11 | `Filled`(`filled = 0.01`) | `d ≤ 0`:输出为空;`OmsStats.dup` +1 |

### OMS-058 回报处理表:Fill(cum == qty)行

- 规则:[回报处理表](README.md#回报处理表)
- 级别:table
- 里程碑:M2
- 前置:P0;同 OMS-055;事件 `OrderEvent{what = Fill, cum_filled 0.01, avg_px 60000.00}`;"状态"列注明槽的 `filled`
- 实现:`crates/hft-oms/tests/fsm_table.rs::oms_058_fill_row`

| 行 | 状态 | 期望 |
|---|---|---|
| OMS-058.1 | `PendingNew`,0 | → `Filled`;置 `FILL_BEFORE_ACK`;`FillOut{qty 0.01}`;`OrderUpdate::Filled{done true}`;`open_buy` −0.01 |
| OMS-058.2 | `Live`,0.004 | → `Filled`;`FillOut{qty 0.006}` |
| OMS-058.3 | `PendingCancel`,0.004 | → `Filled`;`FillOut{qty 0.006}`;`CANCEL_SENT` 清除 |
| OMS-058.4 | `PendingAmend`,0.004 | → `Filled`;`AMEND_SENT` 清除 |
| OMS-058.5 | `Unknown`,0 | → `Filled`;`unknown_count` −1 |
| OMS-058.6 | `Cancelled`,0.004 | → `Filled`;`FillOut{qty 0.006}`;`OmsAnomaly{Table}`;`OmsNeedsRestQuery{trades = 1}` |
| OMS-058.7 | `Rejected`,0 | → `Filled`;`FillOut{qty 0.01}`;`OmsAnomaly{Table}`;`OmsNeedsRestQuery{trades = 1}` |
| OMS-058.8 | `Filled`,0.01 | 输出为空;`OmsStats.dup` +1 |

### OMS-059 回报处理表:CancelAck 行

- 规则:[回报处理表](README.md#回报处理表)
- 级别:table
- 里程碑:M2
- 前置:P0;同 OMS-055;事件 `OrderEvent{what = CancelAck, cum_filled = 槽的 filled}`
- 实现:`crates/hft-oms/tests/fsm_table.rs::oms_059_cancel_ack_row`

| 行 | 状态 | 期望 |
|---|---|---|
| OMS-059.1 | `PendingNew` | → `Cancelled`;`OmsTransition{1 → 6, CancelAck(7)}`;`OrderUpdate{Cancelled}`;`open_buy`、`imm_buy` 各 −0.01 |
| OMS-059.2 | `Live`(filled 0.004) | → `Cancelled`;在途 −0.006 |
| OMS-059.3 | `PendingCancel` | → `Cancelled` |
| OMS-059.4 | `PendingAmend` | → `Cancelled` |
| OMS-059.5 | `Unknown` | → `Cancelled`;`unknown_count` −1 |
| OMS-059.6 | `Filled` | 输出为空;`OmsStats.dup` +1 |
| OMS-059.7 | `Cancelled` | 输出为空;`OmsStats.dup` +1 |
| OMS-059.8 | `Rejected` | 输出为空;`OmsStats.dup` +1 |

### OMS-060 回报处理表:Expired 行

- 规则:[回报处理表](README.md#回报处理表)
- 级别:table
- 里程碑:M2
- 前置:P0;同 OMS-055;事件 `OrderEvent{what = Expired}`;.2 的槽为 IOC
- 实现:`crates/hft-oms/tests/fsm_table.rs::oms_060_expired_row`

| 行 | 状态 | 期望 |
|---|---|---|
| OMS-060.1 | `PendingNew` | → `Cancelled`;`OmsTransition` 的 cause = `Expired(8)` |
| OMS-060.2 | IOC `Live`(filled 0.004,交易所撤余量) | → `Cancelled`;即时在途 −0.006 |
| OMS-060.3 | `PendingCancel` | → `Cancelled` |
| OMS-060.4 | `PendingAmend` | → `Cancelled` |
| OMS-060.5 | `Unknown` | → `Cancelled`;`unknown_count` −1 |
| OMS-060.6 | `Cancelled` | 输出为空;`OmsStats.dup` +1 |

### OMS-061 回报处理表:CancelReject(已成交/不存在)行

- 规则:[回报处理表](README.md#回报处理表)
- 级别:table
- 里程碑:M2
- 前置:P0;同 OMS-055;事件 `OrderEvent{what = CancelReject, detail = GONE}`;下单连接 Up;"状态"列注明 `filled`
- 实现:`crates/hft-oms/tests/fsm_table.rs::oms_061_cancel_reject_gone_row`

| 行 | 状态 | 期望 |
|---|---|---|
| OMS-061.1 | `PendingNew`,0 | 保持;`OmsAnomaly{Table}` 一条(`—` 格) |
| OMS-061.2 | `Live`,0 | 保持;`OmsAnomaly{Table}` 一条 |
| OMS-061.3 | `PendingCancel`,0.01 | → `Filled`;`CANCEL_SENT` 清除 |
| OMS-061.4 | `PendingCancel`,0.004 | → `Unknown`(`prev_state = PendingCancel`);`unknown_count` +1;`OmsUnknown`(P1)与 `OrderUpdate{Unknown}` 各一条;`OrderDecision{Query, target_slot = s}` 一条;`UnknownQuery` 定时器 = now + 1000 ms;无 `OrderDecision{Cancel}` |
| OMS-061.5 | `PendingAmend`,0.01 | → `Filled` |
| OMS-061.6 | `PendingAmend`,0.004 | → `Unknown` + `Query`(同 .4,`prev_state = PendingAmend`) |
| OMS-061.7 | `Unknown`,0 | 无增量时保持;有新增累计成交则只入账增量,部分成交保持 Unknown、全量成交收敛 Filled;重复回报不重复入账;AmendReject 同规则 |
| OMS-061.8 | `Cancelled` | 输出为空;`OmsStats.dup` +1 |

### OMS-062 回报处理表:CancelReject(其他原因)行

- 规则:[回报处理表](README.md#回报处理表)
- 级别:table
- 里程碑:M2
- 前置:P0;同 OMS-055;事件 `OrderEvent{what = CancelReject, detail = 0, reject_code = 1015}`
- 实现:`crates/hft-oms/tests/fsm_table.rs::oms_062_cancel_reject_other_row`

| 行 | 状态 | 期望 |
|---|---|---|
| OMS-062.1 | `PendingNew` | 保持;`OmsAnomaly{Table}` 一条 |
| OMS-062.2 | `Live` | 保持;`OmsAnomaly{Table}` 一条 |
| OMS-062.3 | `PendingCancel`(ACKED) | → `Live`;`CANCEL_SENT` 清除;`OmsTransition{3 → 2, CancelReject(9)}`;无 `OrderUpdate`;无新的 `OrderDecision{Cancel}` |
| OMS-062.4 | `PendingCancel`(由 PendingNew 撤单,未置 ACKED) | → `Live`;置 ACKED;腿 0 即时 −0.01、挂单 +0.01 |
| OMS-062.5 | `PendingAmend` | 保持;`OmsAnomaly{Table}` 一条(`—` 格) |
| OMS-062.6 | `Unknown` | 保持;输出为空 |
| OMS-062.7 | `Filled` | 输出为空;`OmsStats.dup` +1 |

### OMS-063 回报处理表:AmendAck 行

- 规则:[回报处理表](README.md#回报处理表)
- 级别:table
- 里程碑:M2
- 前置:P0;a1@VX 槽 t(卖 Y,`qty 0.01`、`px 60100.0`,GTC);`pending_px = 60200.0`、`pending_qty = 0.008`(需要时);事件 `OrderEvent{what = AmendAck, px 60200.0, qty 0.008, cum_filled = 槽的 filled}`
- 实现:`crates/hft-oms/tests/fsm_table.rs::oms_063_amend_ack_row`

| 行 | 状态 | 期望 |
|---|---|---|
| OMS-063.1 | `PendingNew` | 保持;`OmsAnomaly{Table}` 一条 |
| OMS-063.2 | `Live` | 保持,`px/qty` 仍为 60100.0/0.01;`OmsAnomaly{Table}` 一条 |
| OMS-063.3 | `PendingCancel` 且置 `AMEND_SENT` | 保持 `PendingCancel`;`px = 60200.0`、`qty = 0.008`;`AMEND_SENT` 清除;无 `OmsTransition`;挂单在途按 0.008 计 |
| OMS-063.4 | `PendingCancel`,未置 `AMEND_SENT` | 保持;`OmsAnomaly{Table}` 一条;`px/qty` 不变 |
| OMS-063.5 | `PendingAmend` | → `Live`;`px = 60200.0`、`qty = 0.008`;`AMEND_SENT` 清除;`OmsTransition{4 → 2, AmendAck(10)}`;无 `OrderUpdate`;`open_sell(a1, Y)` 由 0.01 变为 0.008 |
| OMS-063.6 | `Unknown`(prev `PendingAmend`) | → `Live`;`px/qty` 取事件内 60200.0/0.008;`unknown_count` −1 |
| OMS-063.7 | `Filled` | 输出为空;`OmsStats.dup` +1 |

### OMS-064 回报处理表:AmendReject 行

- 规则:[回报处理表](README.md#回报处理表)
- 级别:table
- 里程碑:M2
- 前置:P0;同 OMS-063;事件 `OrderEvent{what = AmendReject}`,`detail` 按行;下单连接 Up
- 实现:`crates/hft-oms/tests/fsm_table.rs::oms_064_amend_reject_row`

| 行 | 状态 | 期望 |
|---|---|---|
| OMS-064.1 | `PendingNew` | 保持;`OmsAnomaly{Table}` 一条 |
| OMS-064.2 | `Live` | 保持;`OmsAnomaly{Table}` 一条 |
| OMS-064.3 | `PendingCancel` 且置 `AMEND_SENT`,`detail = 0` | 保持 `PendingCancel`;`px/qty` 仍为 60100.0/0.01;`AMEND_SENT` 清除 |
| OMS-064.4 | `PendingAmend`,`detail = 0` | → `Live`;`px/qty` = 60100.0/0.01;`AMEND_SENT` 清除;`OmsTransition{4 → 2, AmendReject(11)}` |
| OMS-064.5 | `PendingAmend`,`detail = GONE`,`filled = 0.01` | → `Filled` |
| OMS-064.6 | `PendingAmend`,`detail = GONE`,`filled = 0.004` | → `Unknown`(`prev_state = PendingAmend`)+ `OrderDecision{Query}` |
| OMS-064.7 | `Unknown` | 保持;输出为空 |
| OMS-064.8 | `Cancelled` | 输出为空;`OmsStats.dup` +1 |

### OMS-065 回报处理表:QueryResult 在各状态按结果收敛

- 规则:[回报处理表](README.md#回报处理表)、[Unknown 收敛流程](README.md#unknown-收敛流程)
- 级别:table
- 里程碑:M2
- 前置:P0;同 OMS-055;事件 `OrderEvent{what = QueryResult, detail = 订单状态码, cum_filled, avg_px, px 60000.00, qty 0.01}`
- 实现:`crates/hft-oms/tests/fsm_table.rs::oms_065_query_result_row`

| 行 | 状态 | 事件 | 期望 |
|---|---|---|---|
| OMS-065.1 | `PendingNew` | `detail = New(1)`,cum 0 | → `Live`;置 ACKED;`OrderUpdate{Acked}`;`OmsTransition` cause = `QueryResult(12)` |
| OMS-065.2 | `Live` + `VERIFY` | `PartiallyFilled(2)`,cum 0.004,avg 60000.00 | 保持 `Live`;`FillOut{qty 0.004}`;`VERIFY` 清除 |
| OMS-065.3 | `PendingCancel` | `Cancelled(4)`,cum 0 | → `Cancelled` |
| OMS-065.4 | `PendingAmend` | `New(1)`,cum 0,`px 60050.00`、`qty 0.01` | → `Live`;`px = 60050.00`;无 `OrderUpdate` |
| OMS-065.5 | `Unknown` | `Filled(3)`,cum 0.01,avg 60000.00 | → `Filled`;`unknown_count` −1;`FillOut{qty 0.01}` |
| OMS-065.6 | `Filled`(filled 0.01) | `Filled(3)`,cum 0.01 | 输出为空 |
| OMS-065.7 | `Cancelled`(filled 0.004) | `Cancelled(4)`,cum 0.006,avg 60000.00 | 保持 `Cancelled`;`FillOut{qty 0.002}`;无 `OmsAnomaly` |

### OMS-066 QueryResult 的订单状态映射

- 规则:[Unknown 收敛流程](README.md#unknown-收敛流程)
- 级别:table
- 里程碑:M2
- 前置:P0;槽 s `Unknown`(`prev_state = PendingNew`);事件 `QueryResult{detail = 按行}`
- 实现:`crates/hft-oms/tests/unknown.rs::oms_066_query_status_mapping`

| 行 | `detail` / cum | 期望 |
|---|---|---|
| OMS-066.1 | `New(1)` / 0 | → `Live` |
| OMS-066.2 | `PartiallyFilled(2)` / 0.004 | → `Live`;`FillOut{qty 0.004}` |
| OMS-066.3 | `Filled(3)` / 0.01 | → `Filled` |
| OMS-066.4 | `Cancelled(4)` / 0 | → `Cancelled` |
| OMS-066.5 | `Expired(6)` / 0 | → `Cancelled` |
| OMS-066.6 | `Rejected(5)` / 0,`reject_code = 2010` | → `Rejected`;`reject_code = 2010` |
| OMS-066.7 | `NotFound(7)` / 0 | 保持 `Unknown`(后续按 OMS-139–OMS-149) |

### OMS-067 回报处理表:FeeOnly 在任何状态都不迁移

- 规则:[回报处理表](README.md#回报处理表)、[成交净量](README.md#成交净量b4)
- 级别:table
- 里程碑:M2
- 前置:P0;同 OMS-055;事件 `OrderEvent{what = FeeOnly, fee 0.00001, fee_asset BTC}`
- 实现:`crates/hft-oms/tests/fsm_table.rs::oms_067_fee_only_row`

| 行 | 状态 | 期望 |
|---|---|---|
| OMS-067.1 | `PendingNew` | 状态不变;`FeeOut{account a1, venue VS, inst X, group g1, side Buy, fee 0.00001, fee_asset BTC}` 一条;无 `OmsTransition`、无 `OmsNeedsRestQuery` |
| OMS-067.2 | `Live` | 同 .1 |
| OMS-067.3 | `PendingCancel` | 同 .1 |
| OMS-067.4 | `PendingAmend` | 同 .1 |
| OMS-067.5 | `Unknown` | 同 .1;`unknown_count` 不变 |
| OMS-067.6 | `Filled` | 同 .1 |
| OMS-067.7 | `Cancelled` | 同 .1 |
| OMS-067.8 | 槽已回收(代数不符,墓碑在) | `FeeOut` 的 `group`、`inst`、`side` 取自墓碑;当前槽不变 |

### OMS-068 回报处理表:OutcomeUnknown 行

- 规则:[回报处理表](README.md#回报处理表)、[Unknown 收敛流程](README.md#unknown-收敛流程)
- 级别:table
- 里程碑:M2
- 前置:P0;同 OMS-055;事件 `OrderEvent{what = OutcomeUnknown, reject_code = 1007}`(未登记码或"结果未知"码,由 venue 协议映射);下单连接 Up
- 实现:`crates/hft-oms/tests/fsm_table.rs::oms_068_outcome_unknown_row`

| 行 | 状态 | 期望 |
|---|---|---|
| OMS-068.1 | `PendingNew` | → `Unknown`(cause `OutcomeUnknown(14)`);`OrderDecision{Query}` 一条;不是 `Rejected`,在途不释放 |
| OMS-068.2 | `Live` | 保持 `Live`;置 `VERIFY`;`OrderDecision{Query}` 一条 |
| OMS-068.3 | `PendingCancel` | → `Unknown` + `Query` |
| OMS-068.4 | `PendingAmend` | → `Unknown` + `Query` |
| OMS-068.5 | `Unknown` | 保持;本调用无 `OrderDecision` |
| OMS-068.6 | `Cancelled` | 输出为空;`OmsStats.dup` +1 |

### OMS-069 代数或 epoch 不符的回报不查表

- 规则:[回报处理表](README.md#回报处理表)、[回报匹配与成交增量](README.md#回报匹配与成交增量)
- 级别:unit
- 里程碑:M2
- 前置:P0;槽 s `Live`、`gen = 2`;`Tomb[1]` 存 gen 1(`Filled`,`filled = 0.01`)
- 步骤:
  1. 投递 `Reject{slot = s.slot, gen = 3}`
  2. 投递 `CancelAck{slot = s.slot, gen = 1, cum_filled = 0.01}`
- 期望:
  - 两步都不改 s(`Live`,`filled` 不变),无 `OmsAnomaly`、无 `OmsTransition`
  - 步骤 1 `OmsStats.stale_gen` +1;步骤 2 命中墓碑、`d = 0`,无 `FillOut`、无 Note
- 实现:`crates/hft-oms/tests/matching.rs::oms_069_mismatch_skips_table`

### OMS-070 SendReport 处理表:Place 行

- 规则:[SendReport 处理](README.md#sendreport-处理)、[Unknown 收敛流程](README.md#unknown-收敛流程)
- 级别:table
- 里程碑:M2
- 前置:P0;槽 s 刚 `place`(`PendingNew`,`SlotStamps.op = New`),未链接;下单连接 Up;事件 `SendReport{op = Place, slot = s.slot, gen = s.gen, status = 按行, sent_tsc = 1 ms}`
- 实现:`crates/hft-oms/tests/send.rs::oms_070_send_report_place`

| 行 | `status` | 期望 |
|---|---|---|
| OMS-070.1 | `Written(1)` | `SlotStamps.sent_tsc = 1 000 000`;状态不变;无 Note |
| OMS-070.2 | `KillBlocked(2)` | → `Rejected`,`reject_code = 2`;`OmsTransition{1 → 7, LocalSendFail(17)}`;`OrderUpdate::Rejected{bits: LOCAL_SEND_FAIL (1 << 25), venue_code: 2}`;`open_buy`、`imm_buy` 各 −0.01;腿 0 即时 −0.01;`SlotStamps.op = 0xFF` |
| OMS-070.3 | `RateLimited(3)` | 同 .2,`reject_code = 3`、`venue_code = 3` |
| OMS-070.4 | `NoConn(5)` | 同 .2,`reject_code = 5` |
| OMS-070.5 | `EncodeError(6)` | 同 .2,`reject_code = 6` |
| OMS-070.6 | `RingFull(7)` | 同 .2,`reject_code = 7` |
| OMS-070.7 | `QueryUnsupported(8)` | 同 .2,`reject_code = 8` |
| OMS-070.8 | `WriteError(4)` | → `Unknown`(cause `WriteError(18)`),不是 `Rejected`/`Cancelled`;在途不释放;`OrderDecision{Query}` 一条 |

### OMS-071 SendReport 处理表:Cancel 行

- 规则:[SendReport 处理](README.md#sendreport-处理)、[延迟样本](README.md#延迟样本)
- 级别:table
- 里程碑:M2
- 前置:P0;槽 s `PendingCancel`(拥有者撤单,`CANCEL_SENT`,`SlotStamps.op = Cancel`,除 .9 外);事件 `SendReport{op = Cancel, target_slot = s.slot, target_gen = s.gen, sent_tsc = 5 ms}`
- 实现:`crates/hft-oms/tests/send.rs::oms_071_send_report_cancel`

| 行 | `status` | 期望 |
|---|---|---|
| OMS-071.1 | `Written` | `SlotStamps.sent_tsc = 5 000 000`;状态不变 |
| OMS-071.2 | `KillBlocked` | 状态仍 `PendingCancel`;`CANCEL_SENT` 清除;`Note{OmsCancelNotSent}`(P2)一条;`SlotStamps.op = 0xFF` |
| OMS-071.3 | `RateLimited` | 同 .2 |
| OMS-071.4 | `NoConn` | 同 .2 |
| OMS-071.5 | `EncodeError` | 同 .2 |
| OMS-071.6 | `RingFull` | 同 .2 |
| OMS-071.7 | `QueryUnsupported` | 同 .2 |
| OMS-071.8 | `WriteError` | 状态仍 `PendingCancel`;置 `VERIFY`;`OrderDecision{Query}` 一条 |
| OMS-071.9 | `Written`,但 `SlotStamps.op = New`(新单样本未合成,撤单未记戳) | `sent_tsc` 保持新单的写出时刻 |

### OMS-072 SendReport 处理表:Amend 行

- 规则:[SendReport 处理](README.md#sendreport-处理)
- 级别:table
- 里程碑:M2
- 前置:P0;a1@VX 槽 t `PendingAmend`(`AMEND_SENT`,`SlotStamps.op = Amend`);事件 `SendReport{op = Amend, target_slot = t.slot, sent_tsc = 5 ms}`
- 实现:`crates/hft-oms/tests/send.rs::oms_072_send_report_amend`

| 行 | `status` | 期望 |
|---|---|---|
| OMS-072.1 | `Written` | `sent_tsc = 5 000 000`;状态不变 |
| OMS-072.2 | `KillBlocked` | 状态仍 `PendingAmend`;`AMEND_SENT` 清除;`OmsCancelNotSent` 一条;`op = 0xFF` |
| OMS-072.3 | `RateLimited` | 同 .2 |
| OMS-072.4 | `NoConn` | 同 .2 |
| OMS-072.5 | `EncodeError` | 同 .2 |
| OMS-072.6 | `RingFull` | 同 .2 |
| OMS-072.7 | `WriteError` | 置 `VERIFY`;`OrderDecision{Query}` 一条 |

### OMS-073 SendReport 处理表:Query 行

- 规则:[SendReport 处理](README.md#sendreport-处理)、[Unknown 收敛流程](README.md#unknown-收敛流程)
- 级别:table
- 里程碑:M2
- 前置:P0;槽 s `Unknown`,`query_count = 2`;事件 `SendReport{op = Query, target_slot = s.slot}`
- 实现:`crates/hft-oms/tests/send.rs::oms_073_send_report_query`

| 行 | `status` | 期望 |
|---|---|---|
| OMS-073.1 | `Written` | `query_count = 3` |
| OMS-073.2 | `KillBlocked` | 保持 `Unknown`;`query_count = 2`;无 Note |
| OMS-073.3 | `RateLimited` | 同 .2 |
| OMS-073.4 | `NoConn` | 同 .2 |
| OMS-073.5 | `EncodeError` | 同 .2 |
| OMS-073.6 | `RingFull` | 同 .2 |
| OMS-073.7 | `QueryUnsupported` | 保持 `Unknown`;`query_count = 2`;`Note{OmsNeedsRestQuery}` 一条(`trades = 0`,`v[1]` = s.slot) |
| OMS-073.8 | `WriteError` | 保持 `Unknown`;`query_count = 2` |

### OMS-074 SendReport 处理表:CancelAll 行

- 规则:[SendReport 处理](README.md#sendreport-处理)、[输出扇出](README.md#输出扇出)
- 级别:table
- 里程碑:M2
- 前置:P0;a1@VX 三个 `Live` 槽经 `cancel_all(open_only = false)` 发出一条 `CancelAll`,三槽置 `CANCEL_SENT` 并处于 `PendingCancel`;事件 `SendReport{op = CancelAll}`;下单连接 Up
- 实现:`crates/hft-oms/tests/send.rs::oms_074_send_report_cancel_all`

| 行 | `status` | 期望 |
|---|---|---|
| OMS-074.1 | `Written` | 三槽状态与标志不变;输出为空 |
| OMS-074.2 | `KillBlocked` | 三槽状态不变,`CANCEL_SENT` 均清除;`Note{OmsCancelNotSent}` 恰好一条 |
| OMS-074.3 | `RateLimited` | 同 .2 |
| OMS-074.4 | `NoConn` | 同 .2 |
| OMS-074.5 | `EncodeError` | 同 .2 |
| OMS-074.6 | `RingFull` | 同 .2 |
| OMS-074.7 | `WriteError` | 三槽置 `VERIFY`;`OrderDecision{Query}` 三条 |

### OMS-075 对冲腿本地发送失败的计次分类

- 规则:[SendReport 处理](README.md#sendreport-处理)、[OnFill](README.md#onfill)
- 级别:table
- 里程碑:M2
- 前置:P0;L1 `Hedging`,第一腿已成交 0.012(`first_fill_tsc = 0`),第二腿槽 h(卖 Y 0.012,IOC,`role = Second`)刚 `place`;`retries = 0`;`hedge_retry_min_ms = 100`;`now = 500 ms`;事件 `SendReport{op = Place, slot = h, status = 按行}`
- 实现:`crates/hft-oms/tests/send.rs::oms_075_local_fail_classes`

| 行 | `status` | 期望 |
|---|---|---|
| OMS-075.1 | `KillBlocked` | h → `Rejected(LOCAL_SEND_FAIL)`;L1 `retries = 0`;`OmsTimerReq{HedgeRetry, L1, deadline = 600 ms}` 一条 |
| OMS-075.2 | `RateLimited` | 同 .1 |
| OMS-075.3 | `NoConn` | 同 .1 |
| OMS-075.4 | `RingFull` | 同 .1 |
| OMS-075.5 | `EncodeError` | h → `Rejected(LOCAL_SEND_FAIL)`;L1 `retries = 1`;HedgeRetry 定时器 600 ms 一条 |
| OMS-075.6 | `WriteError` | h → `Unknown`;L1 `retries = 0`;无 HedgeRetry 定时器;`hedge_open` 仍为 0.012 |

### OMS-076 编码错误计次耗尽后链接 Broken

- 规则:[SendReport 处理](README.md#sendreport-处理)、[OnFill](README.md#onfill)
- 级别:unit
- 里程碑:M2
- 前置:P0;`hedge_retry_max = 3`、`hedge_deadline_ms = 3000`;L1 第一腿在 t = 0 成交 0.012
- 步骤:
  1. 第二腿 `SendReport{EncodeError}`(t = 10);之后每次 `on_hedge_retry` 产出的 `SecondRetry` 都收到 `SendReport{EncodeError}`(t = 120、230、340)
- 期望:
  - 共产出 1 条 `Second` 与 3 条 `SecondRetry` 的 `HedgeRequest`;第 4 次失败(t = 340)后 L1 `state = Broken`、`broken_reason = RetryExhausted(1)`
  - 同一调用内 `LinkUpdate{state Broken, broken_reason 1}` 与 `Note{OmsHedgeBroken}`(P0)各一条;之后 `on_hedge_retry(L1)` 无输出
- 实现:`crates/hft-oms/tests/link.rs::oms_076_encode_error_exhausts`

### OMS-077 门控拒绝不计次但截止照算

- 规则:[SendReport 处理](README.md#sendreport-处理)、[RISK_REDUCING 判定](README.md#risk_reducing-判定)、[OnFill](README.md#onfill)
- 级别:unit
- 里程碑:M2
- 前置:P0;`hedge_deadline_ms = 3000`、`hedge_retry_min_ms = 100`、`hedge_retry_max = 3`;HARD kill 生效(gw 对一切单回 `KillBlocked`);L1 第一腿 t = 0 成交 0.012
- 步骤:
  1. 第二腿与每次重试都收到 `SendReport{KillBlocked}`;按定时器推进到 t = 3100
- 期望:
  - t ≤ 3000 的各次失败后 L1 `retries = 0`、状态 `Hedging`,每次登记一条 HedgeRetry
  - t = 3000 之后的第一次失败处理(`now − first_fill_tsc` = 3001 ms 或更大)把 L1 置 `Broken{Deadline(2)}`;`OmsHedgeBroken` 一条
- 实现:`crates/hft-oms/tests/link.rs::oms_077_kill_blocked_deadline`

### OMS-080 新单在首个交易所响应时合成一条 action = New 的样本

- 规则:[延迟样本](README.md#延迟样本)
- 级别:unit
- 里程碑:M2
- 前置:P0;`TrigStamps{ex_ts_ns 100, ex_send_ns 110, hw_rx_ns 120, sw_rx_ns 125, rx_tsc 1000, proc_tsc 1010, trigger_seq 77, kind 0x0101}`
- 步骤:
  1. `place`;sequencer 填 `decided_tsc = 1020`;`SendReport{Written, sent_tsc 1030}`
  2. 投递 `Ack`(`hdr.seq = 80`,`rx_tsc 2000`,`hw_rx_ns 1990`,`ex_ts_ns 1980`)
  3. 再投递 `PartialFill`
- 期望:
  - 步骤 2 输出恰好一条 `LatencySample{trigger_seq 77, ack_seq 80, slot, gen, venue 2, account 1, strategy 1, ex_ts_ns 100, ex_send_ns 110, hw_rx_ns 120, rx_tsc 1000, proc_tsc 1010, decided_tsc 1020, sent_tsc 1030, ack_hw_rx_ns 1990, ack_rx_tsc 2000, ack_ex_ts_ns 1980, sw_rx_ns 125, link_role 0, action 0, trigger_kind 0x0101}`;之后 `SlotStamps.op = 0xFF`
  - 步骤 3 无 `LatencySample`
  - 触发与响应的 HW/SW/Missing 组合分别传入 `src` 的接收点；有效 `sent_tsc` 为 TSC，不把未知来源标为 HW。
- 实现:`crates/hft-oms/tests/latency.rs::oms_080_new_sample_on_first_response`

### OMS-081 Ack 后同一事件撤单:新单与撤单各一条样本

- 规则:[延迟样本](README.md#延迟样本)
- 级别:unit
- 里程碑:M2
- 前置:P0;同 OMS-080 步骤 1
- 步骤:
  1. 投递 `Ack`(`hdr.seq = 80`,`hdr.kind = OrderEvent 0x0203`);同一事件内拥有者 `cancel(s, trig = {trigger_seq 80, kind 0x0203, rx_tsc 2000, sw_rx_ns 0})`;sequencer 填 `decided_tsc = 2010`;`SendReport{Cancel, Written, sent_tsc 2020}`
  2. 投递 `CancelAck`(`hdr.seq = 90`,`rx_tsc 3000`)
- 期望:
  - 步骤 1 输出 `LatencySample{action 0, trigger_seq 77}` 一条
  - 步骤 2 输出 `LatencySample{action 1, trigger_seq 80, trigger_kind 0x0203, decided_tsc 2010, sent_tsc 2020, ack_seq 90, ack_rx_tsc 3000}` 一条
- 实现:`crates/hft-oms/tests/latency.rs::oms_081_quote_style_cancel_sample`

### OMS-082 改单在 AmendAck 或 AmendReject 时合成 action = Amend

- 规则:[延迟样本](README.md#延迟样本)
- 级别:table
- 里程碑:M2
- 前置:P0;a1@VX 槽 t `Live`,新单样本已合成(`op = 0xFF`);`amend(t, trig = {trigger_seq 300, kind 0x0101})`、`SendReport{Amend, Written, sent_tsc 4000}`
- 实现:`crates/hft-oms/tests/latency.rs::oms_082_amend_sample`

| 行 | 首个响应 | 期望 |
|---|---|---|
| OMS-082.1 | `AmendAck` | `LatencySample{action 2, trigger_seq 300, sent_tsc 4000}` 一条;`op = 0xFF` |
| OMS-082.2 | `AmendReject` | 同 .1 |
| OMS-082.3 | `PartialFill`(改单期间的成交) | 无样本;`op` 仍为 Amend;随后的 `AmendAck` 合成一条 |

### OMS-083 未合成的新单样本不被撤单覆盖

- 规则:[延迟样本](README.md#延迟样本)
- 级别:unit
- 里程碑:M2
- 前置:P0;`place`(trig `trigger_seq 77`),`SendReport{Written}`,尚无交易所响应(`op = New`)
- 步骤:
  1. 拥有者 `cancel(s, trig = {trigger_seq 99})`;`SendReport{Cancel, Written}`
  2. 投递 `Ack`;再投递 `CancelAck`
- 期望:
  - 步骤 1 后 `SlotStamps.trigger_seq = 77`、`op = New`
  - 步骤 2:`Ack` 时一条 `LatencySample{action 0, trigger_seq 77}`;`CancelAck` 时无样本
- 实现:`crates/hft-oms/tests/latency.rs::oms_083_pending_new_cancel_not_stamped`

### OMS-084 撤单确定未写出则不产生撤单样本

- 规则:[延迟样本](README.md#延迟样本)
- 级别:unit
- 里程碑:M2
- 前置:P0;s `Live`,`op = 0xFF`;`cancel(s, trig = {trigger_seq 99})`
- 步骤:
  1. `SendReport{Cancel, RateLimited}`
  2. 投递 `CancelAck`
- 期望:
  - 步骤 1 后 `op = 0xFF`
  - 步骤 2 无 `LatencySample`
- 实现:`crates/hft-oms/tests/latency.rs::oms_084_unsent_cancel_no_sample`

### OMS-085 批量撤单与冒烟单到期撤单不记戳

- 规则:[延迟样本](README.md#延迟样本)
- 级别:table
- 里程碑:M2
- 前置:P0;每行一个 `Live` 槽,`op = 0xFF`;按行调用批量入口,随后投递 `CancelAck`
- 实现:`crates/hft-oms/tests/latency.rs::oms_085_bulk_cancels_unstamped`

| 行 | 入口 | 期望 |
|---|---|---|
| OMS-085.1 | `cancel_all(Account(a1), open_only = false)`(VG,无批量撤单,逐槽) | 撤单后 `op = 0xFF`;`CancelAck` 时无样本 |
| OMS-085.2 | `cancel_open(cell (a1, g1))` | 同 .1 |
| OMS-085.3 | `cancel_resting_on(a1, X)` | 同 .1 |
| OMS-085.4 | `reissue_cancels(Account(a1), false)` | 同 .1 |
| OMS-085.5 | `on_smoke_ttl(a1, VS, keep = false)`(冒烟单槽) | 同 .1 |

### OMS-086 恢复后的在途槽不产生样本

- 规则:[延迟样本](README.md#延迟样本)、[快照行格式](README.md#快照行格式节-2312草案实现时以代码与编译期断言为准)
- 级别:unit
- 里程碑:M2
- 前置:P0;快照前 s `PendingNew`,`op = New`、`trigger_seq = 77`;`snapshot` 后 `restore`
- 步骤:
  1. `recovery_end`;下单连接 Up;投递 `QueryResult{New}`;再投递 `PartialFill`
- 期望:
  - `restore` 后 s 的 `SlotStamps` 除 `op = 0xFF` 外各字段为 0
  - 全过程无 `LatencySample`
- 实现:`crates/hft-oms/tests/latency.rs::oms_086_restored_slot_no_sample`

### OMS-087 槽回收时清除待合成样本

- 规则:[延迟样本](README.md#延迟样本)
- 级别:unit
- 里程碑:M2
- 前置:P0;`reuse_quarantine_ms = 5000`;s 经 `SendReport{WriteError}` 进 `Unknown`(`op = New`),再经 `QueryResult{Filled}` 收敛(`QueryResult` 不是首个响应)
- 步骤:
  1. t = 5001 后 `on_sweep` 回收 s;再 `place` 得到同一槽的下一代 s'
- 期望:
  - 收敛时无 `LatencySample`;回收后该槽 `op = 0xFF`;s' 的 `SlotStamps` 只含新单的 trig 值
- 实现:`crates/hft-oms/tests/latency.rs::oms_087_recycle_clears_stamp`

### OMS-088 触发事件的 sw_rx 与 kind 原样进样本

- 规则:[延迟样本](README.md#延迟样本)、[入口](README.md#入口全部在-sequencer-线程调用mut-self无分配)
- 级别:table
- 里程碑:M2
- 前置:P0;`place` 的 `TrigStamps` 按行;`SendReport{Written}` 后投递 `Ack`
- 实现:`crates/hft-oms/tests/latency.rs::oms_088_trig_passthrough`

| 行 | `TrigStamps` | 期望 |
|---|---|---|
| OMS-088.1 | `sw_rx_ns = 1 234 567, kind = 0x0101`(Bbo) | 样本 `sw_rx_ns = 1 234 567`、`trigger_kind = 0x0101` |
| OMS-088.2 | `sw_rx_ns = 0, kind = 0x0203`(OrderEvent) | 样本 `sw_rx_ns = 0`、`trigger_kind = 0x0203` |
| OMS-088.3 | 触发为第一腿成交的第二腿(`link_role = Second`) | 样本 `link_role = 2` |

### OMS-089 撤单写出时刻成为 NotFound grace 的起点

- 规则:[延迟样本](README.md#延迟样本)、[Unknown 收敛流程](README.md#unknown-收敛流程)
- 级别:unit
- 里程碑:M2
- 前置:P0;`not_found_grace_ms[VS] = 5500`;s 在 t = 0 写出(`sent_tsc = 0 ms`),新单样本已合成;t = 4000 拥有者撤单,`SendReport{Cancel, Written, sent_tsc = 4000 ms}`;t = 4100 `CancelReject{GONE}` 使 s 进 `Unknown`
- 步骤:
  1. t = 9000 投递 `QueryResult{NotFound}`
  2. t = 9501 投递 `QueryResult{NotFound}`
- 期望:
  - 步骤 1:`9000 − 4000 = 5000 ≤ 5500`,保持 `Unknown`、不置 `HIST_WAIT`、无 `OmsNeedsRestQuery`
  - 步骤 2:`5501 > 5500`,置 `HIST_WAIT`,`OmsNeedsRestQuery{trades = 1}` 一条
- 实现:`crates/hft-oms/tests/unknown.rs::oms_089_grace_from_cancel_sent`

### OMS-090 一次 REPORT_DECODE 把非终态槽置 VERIFY 并查询

- 规则:[REPORT_DECODE](README.md#report_decode)
- 级别:unit
- 里程碑:M2
- 前置:P0;a1@VS 有 `PendingNew` 槽 p、`Live` 槽 l、`Filled` 槽 f;`decode_fail_down_n = 3`
- 步骤:
  1. 投递 `ConnState{venue VS, account a1, role UserStream, state Degraded, reason REPORT_DECODE}`
- 期望:
  - p 与 l 置 `VERIFY`,状态不变;f 不变;`OrderDecision{Query}` 两条;`unknown_count = 0`;`decode_fails = 1`
- 实现:`crates/hft-oms/tests/send.rs::oms_090_report_decode_verify`

### OMS-091 连续 decode_fail_down_n 次 REPORT_DECODE 按下单连接 Down 处理

- 规则:[REPORT_DECODE](README.md#report_decode)
- 级别:table
- 里程碑:M2
- 前置:P0;`decode_fail_down_n = 3`;a1@VS 有 `PendingNew` 槽 p、`Live` 槽 l;事件序列按行
- 实现:`crates/hft-oms/tests/send.rs::oms_091_decode_fail_threshold`

| 行 | 序列 | 期望 |
|---|---|---|
| OMS-091.1 | `Degraded{REPORT_DECODE}` × 2 | p 仍 `PendingNew`(带 `VERIFY`);`decode_fails = 2` |
| OMS-091.2 | × 3 | 第 3 次:p → `Unknown`(cause `ReportDecode(19)`);l 带 `VERIFY`;`conn_up` 仍为 true;`decode_fails = 0` |
| OMS-091.3 | × 2,`ConnState{UserStream, Up}`,× 1 | p 仍 `PendingNew`;`decode_fails = 1` |
| OMS-091.4 | `decode_fail_down_n = 1`;× 1 | p → `Unknown` |

### OMS-092 mirror_restore 为每个非终态槽给出一条映射并按游标分批

- 规则:[恢复后的 gw 映射](README.md#恢复后的-gw-映射)、[入口](README.md#入口全部在-sequencer-线程调用mut-self无分配)
- 级别:unit
- 里程碑:M2
- 前置:P0;`slots[(VS, a1)] = 400`;恢复后 a1@VS 有 300 个非终态槽(其中 `Unknown` 120 个、带 `VERIFY` 的 `Live` 30 个),另有 50 个终态槽
- 步骤:
  1. `mirror_restore(VS, a1, &mut buf, &mut cursor = 0)`(`buf` 容量 256)
  2. 清空 `buf` 后以同一游标再调一次
- 期望:
  - 步骤 1 返回 true,`buf` 256 条;步骤 2 返回 false,`buf` 44 条
  - 两批合计 300 条,按 `slot_id` 升序、不重不漏;每条 `MirrorRestore{slot, gen, epoch, inst, exch_order_id}` 与槽字段相等;终态槽不出现
- 实现:`crates/hft-oms/tests/recover.rs::oms_092_mirror_restore_batches`

### OMS-093 gw 无法为其编码查询的槽保持 Unknown 并转交 reconciler

- 规则:[恢复后的 gw 映射](README.md#恢复后的-gw-映射)
- 级别:unit
- 里程碑:M2
- 前置:P0;恢复后槽 r `Unknown`,`exch_order_id = 0`;`mirror_restore` 已发出;下单连接 Up
- 步骤:
  1. `on_conn_state(Order Up)` 产出 `Query`;gw 回 `SendReport{op = Query, target_slot = r, status = QueryUnsupported}`
- 期望:
  - r 保持 `Unknown`,`query_count = 0`;`Note{OmsNeedsRestQuery}`(`trades = 0`、`v[1] = r.slot`、`v[2] = r.gen`)一条
- 实现:`crates/hft-oms/tests/recover.rs::oms_093_query_unsupported_after_mirror`

### OMS-100 成交增量的名义由累计均价推出

- 规则:[回报匹配与成交增量](README.md#回报匹配与成交增量)
- 级别:unit
- 里程碑:M2
- 前置:P0;s `Live`,`filled = 0`
- 步骤:
  1. `PartialFill{cum 0.004, avg_px 60000.00}`
  2. `Fill{cum 0.01, avg_px 60001.00}`
- 期望:
  - 步骤 1:`FillOut{qty 0.004, px 60000.00}`;`filled_notional` = 240.00 USDT
  - 步骤 2:`filled_notional` = 600.01 USDT;本次增量名义 360.01;`FillOut{qty 0.006, px 60001.67}`(360.01 / 0.006 = 60001.666…,`HalfEven` 取到 0.01)
- 实现:`crates/hft-oms/tests/matching.rs::oms_100_increment_notional`

### OMS-101 累计量不增的回报不产生成交输出

- 规则:[回报匹配与成交增量](README.md#回报匹配与成交增量)
- 级别:table
- 里程碑:M2
- 前置:P0;s `Live`,`filled = 0.004`
- 实现:`crates/hft-oms/tests/matching.rs::oms_101_non_increasing_cum`

| 行 | 事件 | 期望 |
|---|---|---|
| OMS-101.1 | `PartialFill{cum 0.004}`(重复) | 无 `FillOut`;`filled = 0.004`;`OmsStats.dup` +1 |
| OMS-101.2 | `PartialFill{cum 0.003}`(回退) | 无 `FillOut`;`filled = 0.004` |
| OMS-101.3 | `PartialFill{cum 0.005}` | `FillOut{qty 0.001}` |

### OMS-102 OrderEvent 成交回报缺均价时按本笔成交价计名义

- 规则:[回报匹配与成交增量](README.md#回报匹配与成交增量)
- 级别:unit
- 里程碑:M2
- 前置:P0;s `Live`,`filled = 0.004`、`filled_notional = 240.00`
- 步骤:
  1. `PartialFill{cum 0.006, avg_px 0, px 60010.00}`
- 期望:
  - 增量名义 = 60010.00 × 0.002 = 120.02;`FillOut{qty 0.002, px 60010.00}`,`flags` 不含 `PX_EST`;无 `OmsFillPxEstimated`
- 实现:`crates/hft-oms/tests/matching.rs::oms_102_order_event_zero_avg_px`

### OMS-103 QueryResult 缺均价时以委托价估计并请求成交历史

- 规则:[回报匹配与成交增量](README.md#回报匹配与成交增量)
- 级别:unit
- 里程碑:M2
- 前置:P0;s `Unknown`,`filled = 0`、`px = 60000.00`
- 步骤:
  1. `QueryResult{detail Filled, cum 0.01, avg_px 0, px 60000.00}`
- 期望:
  - `FillOut{qty 0.01, px 60000.00, flags ⊇ PX_EST}`;`Note{OmsFillPxEstimated}`(P1)一条;`Note{OmsNeedsRestQuery}`(`trades = 1`)一条;s → `Filled`
- 实现:`crates/hft-oms/tests/matching.rs::oms_103_query_zero_avg_px`

### OMS-104 超量成交照常入账、不进对冲基数并使链接 Broken

- 规则:[回报匹配与成交增量](README.md#回报匹配与成交增量)、[OnFill](README.md#onfill)
- 级别:unit
- 里程碑:M2
- 前置:P0;L1 第一腿槽 f(买 X,`qty 0.01`,IOC)`Live`,`filled = 0`;手续费 USDT
- 步骤:
  1. `Fill{cum 0.012}`
- 期望:
  - `FillOut{qty 0.012, flags ⊇ OVERFILL}`;腿 0 `net = 0.012`
  - L1 `first_net = 0.01`(超出部分不计);`HedgeRequest` 至多一条且 `basis ≤ 0.01`
  - L1 `Broken{Overfill(4)}`;`Note{OmsOverfill}`(P0)与 `Note{OmsHedgeBroken}`(P0)各一条;`LinkUpdate{Broken, 4}` 一条
- 实现:`crates/hft-oms/tests/matching.rs::oms_104_overfill`

### OMS-105 槽与墓碑都不匹配且带成交:按外来成交入账并请求对账

- 规则:[回报匹配与成交增量](README.md#回报匹配与成交增量)
- 级别:unit
- 里程碑:M2
- 前置:P0;a1@VS 槽 3 当前 `gen = 5`,墓碑存 gen 3、4;`foreign` 与 `ext_held` 为空
- 步骤:
  1. `PartialFill{slot 3, gen 1, epoch 7, exch_order_id 4444, cum 0.004, avg_px 60000.00, fee 0.24 USDT, side Buy}`
- 期望:
  - 槽 3 不变;`ForeignFill{venue 2, account 1, inst 11, side Buy, qty 0.004, px 60000.00, exch_order_id 4444, trade_id 0, cum_filled 0.004}` 一条
  - `Note{OmsStaleGenFill}`(P0,`v = [258, 3, 1, 1, 7, 4444, 400 000]`)、`Note{OmsForeignFill}`(`path = 2`)与 `Note{OmsNeedsRestQuery}`(`trades = 1`)各一条
- 实现:`crates/hft-oms/tests/matching.rs::oms_105_stale_gen_fill_booked`

### OMS-106 StaleGen 带成交但无 exch_order_id 时不入账

- 规则:[回报匹配与成交增量](README.md#回报匹配与成交增量)
- 级别:unit
- 里程碑:M2
- 前置:P0;同 OMS-105
- 步骤:
  1. 同 OMS-105,但 `exch_order_id = 0`
- 期望:
  - 无 `ForeignFill`、无 `Note{OmsForeignFill}`;`Note{OmsStaleGenFill}`(P0,`v[2] = 0`)一条;`OmsNeedsRestQuery{trades = 1}` 一条
- 实现:`crates/hft-oms/tests/matching.rs::oms_106_stale_gen_no_eid`

### OMS-107 StaleGen 的成交先扣除已按外来成交入账的部分

- 规则:[回报匹配与成交增量](README.md#回报匹配与成交增量)、[外来成交](README.md#外来成交b3)
- 级别:table
- 里程碑:M2
- 前置:P0;同 OMS-105;`ext_held[4444]` 按行;事件 `PartialFill{gen 1, exch_order_id 4444, cum 0.01}`
- 实现:`crates/hft-oms/tests/matching.rs::oms_107_stale_gen_ext_held`

| 行 | `ext_held[4444]` | 期望 |
|---|---|---|
| OMS-107.1 | 0.004 | `ForeignFill{qty 0.006, cum_filled 0.01}`;`ext_held[4444] = 0`;`OmsExtDedup` 一条;`OmsStaleGenFill.v[2] = 1` |
| OMS-107.2 | 0.01 | 无 `ForeignFill`;`ext_held[4444] = 0`;`OmsExtDedup` 一条;`OmsStaleGenFill.v[2] = 0` |
| OMS-107.3 | 0.012 | 无 `ForeignFill`;`ext_held[4444] = 0.002`;`OmsExtDedup` 一条 |
| OMS-107.4 | 无条目 | `ForeignFill{qty 0.01}`;无 `OmsExtDedup` |

### OMS-108 防御路径按 foreign 表的累计量去重

- 规则:[回报匹配与成交增量](README.md#回报匹配与成交增量)、[外来成交](README.md#外来成交b3)
- 级别:unit
- 里程碑:M2
- 前置:P0;`foreign_fill_track[(VS, a1)] = 64`;`foreign` 为空
- 步骤:
  1. `OrderEvent{slot 0, exch_order_id 555, cum 0.004, px 60000.00, side Sell}`
  2. 同单 `cum 0.01`
  3. 同单 `cum 0.01`(重复)
- 期望:
  - 步骤 1:`ForeignFill{qty 0.004, cum_filled 0.004, side Sell}`;`Note{OmsForeignFillViaOrderEvent}`(P1)与 `Note{OmsForeignFill}`(`path = 1`)各一条;`foreign[555] = 0.004`
  - 步骤 2:`ForeignFill{qty 0.006, cum_filled 0.01}`;`foreign[555] = 0.01`
  - 步骤 3:无 `ForeignFill`、无 Note
- 实现:`crates/hft-oms/tests/matching.rs::oms_108_defensive_path_dedup`

### OMS-109 trade_id = 0 的同单两笔增量构造出不同的 fill_seq,ledger 两笔都入账

- 规则:[输出 OmsOut](README.md#输出-omsout容量固定由-sequencer-预分配并在每个输入事件后清空)、[回报匹配与成交增量](README.md#回报匹配与成交增量)
- 级别:integ
- 里程碑:M2
- 前置:P0;testkit 进程内 sequencer + hft-ledger;a1@VS 外来卖单 555 经防御路径到达两次(`side Sell`,`cum 0.004`、`cum 0.01`,`trade_id = 0`)
- 步骤:
  1. 依次投递两条 `OrderEvent{slot 0, exch_order_id 555}`
- 期望:
  - sequencer 构造的两条 `ExternalFill{cause Unknown}` 的 `fill_seq` 分别为 400 000 与 1 000 000;`ledger.apply_external` 两次都返回 `Some`;账户 X 持仓变化合计 −0.01
- 实现:`crates/hft-sequencer/tests/oms_integ.rs::oms_109_foreign_fill_seq_distinct`

### OMS-110 外来单跟踪表 FIFO 淘汰时写 Note

- 规则:[回报匹配与成交增量](README.md#回报匹配与成交增量)、[外来成交](README.md#外来成交b3)、[配置项](README.md#配置项omsconfig)
- 级别:table
- 里程碑:M2
- 前置:P0;`foreign_fill_track[(VS, a1)] = 2`;依次到达外来单 501、502、503(各 `cum 0.001`,防御路径)
- 实现:`crates/hft-oms/tests/matching.rs::oms_110_foreign_table_evict`

| 行 | 到达序列 | 期望 |
|---|---|---|
| OMS-110.1 | 501、502 | 两条 `ForeignFill`;无 `OmsAnomaly`;`n_foreign = 2` |
| OMS-110.2 | 501、502、503 | 第三条到达时淘汰 501:`OmsAnomaly{ForeignEvict(2)}` 一条;`foreign` = {502, 503} |
| OMS-110.3 | 501、502、503,再 501 `cum 0.002` | 501 已被淘汰,按 0 起算:`ForeignFill{qty 0.002}` |

### OMS-111 ext_held 表 FIFO 淘汰时写 Note

- 规则:[外来成交](README.md#外来成交b3)、[配置项](README.md#配置项omsconfig)
- 级别:table
- 里程碑:M2
- 前置:P0;`ext_held_track[(VS, a1)] = 2`;`external_adjust(cause Unknown)` 依次带 eid 601、602、603,各 `net 0.001`
- 实现:`crates/hft-oms/tests/external.rs::oms_111_ext_held_evict`

| 行 | 序列 | 期望 |
|---|---|---|
| OMS-111.1 | 601、602 | `ext_held` = {601: 0.001, 602: 0.001};无 Note |
| OMS-111.2 | 601、602、603 | 淘汰 601;`OmsAnomaly{ForeignEvict(2)}` 一条 |

### OMS-113 已按外来成交入账的量在槽成交时扣除

- 规则:[回报匹配与成交增量](README.md#回报匹配与成交增量)、[外来成交](README.md#外来成交b3)
- 级别:unit
- 里程碑:M2
- 前置:P0;L1 第一腿槽 f(买 X 0.01,IOC)`Unknown`,`exch_order_id = 4444`;`ext_held[4444] = 0.004`(先前 `external_adjust(cause Unknown, net +0.004)` 已把腿 0 `net` 加到 0.004);第二腿腿 1 `net = 0`;手续费 USDT
- 步骤:
  1. `QueryResult{detail Filled, cum 0.01, avg_px 60000.00}`
- 期望:
  - f `filled = 0.01`、`Filled`;L1 `first_net = 0.01`(按全量推进)
  - `FillOut{qty 0.006, net_qty 0.006, flags ⊇ EXT_DEDUP}`;腿 0 `net = 0.01`;`ext_held[4444] = 0`;`Note{OmsExtDedup}`(P1)一条
  - `HedgeRequest{Second}` 数量 = min(0.01, `floor_to_lot(naked_dir)` = 0.01) = 0.01(腿 1 未被平掉)
- 实现:`crates/hft-oms/tests/external.rs::oms_113_ext_held_dedup_on_slot`

### OMS-114 OmsForeignFill 带足入账字段

- 规则:[回报匹配与成交增量](README.md#回报匹配与成交增量)、[Note 码表](README.md#note-码表)
- 级别:unit
- 里程碑:M2
- 前置:P0;外来单经防御路径到达:`OrderEvent{slot 0, venue VS, account a1, inst X, side Buy, exch_order_id 555, cum 0.004, px 60000.00, fee 0.000004, fee_asset BTC}`
- 步骤:
  1. 投递该事件
- 期望:
  - `Note{OmsForeignFill}` 恰好一条:`level = Info`、`inst = 11`、`group = 0`
  - `v[0] = 2 | 1 << 8 | 1 << 16 | 1 << 24 | 1 << 32 = 4 311 810 306`(venue 2、account 1、side Buy、path 1、fee_asset BTC)
  - `v[1..8] = [400 000, 6 000 000, 400, 555, 0, 400 000, 400 000]`(qty、px、fee、exch_order_id、trade_id、fill_seq、cum_filled 的 raw 值)
- 实现:`crates/hft-oms/tests/matching.rs::oms_114_foreign_fill_note_fields`

### OMS-115 sequencer 构造的 ExternalFill 与按 OmsForeignFill 字段构造的逐字段相同

- 规则:[回报匹配与成交增量](README.md#回报匹配与成交增量)
- 级别:integ
- 里程碑:M9
- 前置:testkit 进程内 sequencer;一条防御路径外来成交(OMS-114 的事件)与一条 `StaleGen` 成交(OMS-105 的事件)
- 步骤:
  1. 处理两条 Input;从 journal 读出同一事件内的 `Note{OmsForeignFill}`,按 README 第 8 条的字段构造 `ExternalFill{cause = Unknown}`(其余字段 0)
- 期望:
  - 两条构造结果分别与 sequencer 交给 `ledger.apply_external` 的 `ExternalFill` 逐字段相等(含 `fill_seq`、`trade_id`、`fee`、`fee_asset`)
  - journal 中这两个事件都没有 `ExternalFill` 记录(不另写 Input)
- 实现:`crates/hft-testkit/src/scenarios/oms.rs::oms_115_foreign_fill_note_matches_synth`

### OMS-120 成交净量按回报里的 fee_asset 计算

- 规则:[成交净量](README.md#成交净量b4)
- 级别:table
- 里程碑:M2
- 前置:P0;各行一张成交 0.01 的单;`FillOut.net_qty` 带符号(买为正)
- 实现:`crates/hft-oms/tests/net.rs::oms_120_net_qty_rules`

| 行 | 单 | 回报的费用 | 期望 `FillOut.net_qty` / `SlotExt.net_filled` |
|---|---|---|---|
| OMS-120.1 | 现货买 X | 0.00001 BTC | +0.00999 / 0.00999 |
| OMS-120.2 | 现货卖 X | 0.00001 BTC | −0.01001 / 0.01001 |
| OMS-120.3 | 现货买 X | 0.6 USDT | +0.01 / 0.01 |
| OMS-120.4 | 现货买 X | 0.0012 BNB | +0.01 / 0.01 |
| OMS-120.5 | 永续卖 Y | 0.00001 BTC | −0.01 / 0.01 |
| OMS-120.6 | 现货买 X | `fee_asset = ASSET_NONE` | +0.01 / 0.01 |

### OMS-121 费用晚到:先按毛量对冲,FeeOnly 后经 net_adjust 校正腿与余数

- 规则:[成交净量](README.md#成交净量b4)、[OnFill](README.md#onfill)
- 级别:unit
- 里程碑:M2
- 前置:P0;L1(未 Broken)第一腿 f 买 X 0.012(IOC),`last_link(a1, g1) = L1`
- 步骤:
  1. `Fill{cum 0.012, fee_asset ASSET_NONE}`
  2. `FeeOnly{fee 0.000012, fee_asset BTC}`;sequencer 把 ledger 返回的 `NetAdjust{delta −0.000012}` 转交 `net_adjust(a1, g1, X, −0.000012)`
- 期望:
  - 步骤 1:`HedgeRequest{qty 0.012}`(按毛量);`residual[g1][1] = 0`
  - 步骤 2:`FeeOut{fee 0.000012, fee_asset BTC, group g1}` 一条;无迁移;`net_adjust` 后腿 0 `net = 0.011988`、f `net_filled = 0.011988`;`residual[g1][1] = −0.000012`
- 实现:`crates/hft-oms/tests/net.rs::oms_121_late_fee_net_adjust`

### OMS-122 最近链接已 Broken 时 net_adjust 不动余数

- 规则:[成交净量](README.md#成交净量b4)
- 级别:unit
- 里程碑:M2
- 前置:P0;同 OMS-121 步骤 1 之后,L1 被置 `Broken`
- 步骤:
  1. `net_adjust(a1, g1, X, −0.000012)`
- 期望:
  - 腿 0 `net` 减 0.000012;`residual[g1][1]` 保持 0
- 实现:`crates/hft-oms/tests/net.rs::oms_122_net_adjust_broken_link`

### OMS-123 找不到槽与墓碑的 FeeOnly 记账户级

- 规则:[成交净量](README.md#成交净量b4)
- 级别:unit
- 里程碑:M2
- 前置:P0;a1@VS 无匹配 `(epoch 7, slot 9, gen 4)` 的槽与墓碑
- 步骤:
  1. `FeeOnly{slot 9, gen 4, inst X, fee 0.3, fee_asset USDT}`
- 期望:
  - `FeeOut{group GROUP_NONE, inst 11, side 0, fee 0.3, fee_asset USDT}` 一条;`Note{OmsFeeUnattributed}`(P1)一条;任何槽与 LegTracker 不变
- 实现:`crates/hft-oms/tests/net.rs::oms_123_fee_unattributed`

### OMS-124 本 crate 不检查现货卖出的库存与余额

- 规则:[成交净量](README.md#成交净量b4)
- 级别:unit
- 里程碑:M2
- 前置:P0;a1 的 X 库存为 0(LegTracker 与任何视图);`ApprovedIntent{卖 X 0.5}`
- 步骤:
  1. `place`
- 期望:
  - `Ok`;`OrderDecision{New, qty 0.5}` 一条
- 实现:`crates/hft-oms/tests/net.rs::oms_124_no_balance_check`

### OMS-130 下单连接断开:Pending* 进 Unknown、Live 置 VERIFY

- 规则:[Unknown 收敛流程](README.md#unknown-收敛流程)、[LegTracker](README.md#legtracker)
- 级别:unit
- 里程碑:M2
- 前置:P0;a1@VX 表:p `PendingNew`(GTC)、c `PendingCancel`(GTC,ACKED)、m `PendingAmend`(GTC,ACKED)、l `Live`(GTC,ACKED)、f `Filled`;各槽 `group = g1`、`qty 0.01`;`unknown_query_interval_ms = 1000`;`now = 100 ms`
- 步骤:
  1. 投递 `ConnState{venue VX, account a1, role Order, state Down}`
- 期望:
  - p、c、m → `Unknown`,`prev_state` 分别为 `PendingNew`、`PendingCancel`、`PendingAmend`;`OmsTransition` cause = `ConnDown(15)`;`Note{OmsUnknown}`(P1)与 `OrderUpdate{Unknown}` 各三条
  - l 保持 `Live` 并置 `VERIFY`;f 不变
  - `unknown_count = 3`;`venue_ready(VX, a1) = false`;`conn_up = false`;`legs(a1, g1).has_unknown = true`
  - 无 `OrderDecision{Query}`(连接 Down);`UnknownQuery` 定时器四条,`deadline = 1100 ms`
  - 腿 1 的 c、m 剩余量从挂单在途移到即时在途
- 实现:`crates/hft-oms/tests/unknown.rs::oms_130_order_conn_down`

### OMS-131 用户流断开:非终态槽置 VERIFY 并查询,不进 Unknown

- 规则:[Unknown 收敛流程](README.md#unknown-收敛流程)
- 级别:unit
- 里程碑:M2
- 前置:P0;同 OMS-130 的表;下单连接 Up
- 步骤:
  1. 投递 `ConnState{VX, a1, role UserStream, state Down}`
- 期望:
  - p、c、m、l 置 `VERIFY`,状态不变;`unknown_count = 0`;`OrderDecision{Query}` 四条;f 不变
- 实现:`crates/hft-oms/tests/unknown.rs::oms_131_user_stream_down`

### OMS-132 sweep 的 Pending 超时与 IOC 超时边界

- 规则:[Unknown 收敛流程](README.md#unknown-收敛流程)、[StaleOrderSweep](README.md#staleordersweeptimeroms-sweep1-s)、[配置项](README.md#配置项omsconfig)
- 级别:table
- 里程碑:M2
- 前置:P0;`ack_timeout_ms = 2000`、`cancel_timeout_ms = 2000`、`ioc_timeout_ms = 1000`;槽的 `t_state = 0`;在 `now = 列值` 调 `on_sweep`
- 实现:`crates/hft-oms/tests/sweep.rs::oms_132_pending_timeouts`

| 行 | 槽 | `now`(ms) | 期望 |
|---|---|---|---|
| OMS-132.1 | `PendingNew` | 1999 | 保持 |
| OMS-132.2 | `PendingNew` | 2000 | 保持 |
| OMS-132.3 | `PendingNew` | 2001 | → `Unknown`,cause `Timeout(16)` |
| OMS-132.4 | `PendingCancel` | 2000 | 保持 |
| OMS-132.5 | `PendingCancel` | 2001 | → `Unknown` |
| OMS-132.6 | `PendingAmend` | 2001 | → `Unknown` |
| OMS-132.7 | IOC `Live` | 1000 | 保持 |
| OMS-132.8 | IOC `Live` | 1001 | → `Unknown` |
| OMS-132.9 | GTC `Live` | 10 000 000 | 保持(无超时) |
| OMS-132.10 | `ack_timeout_ms = 500`,`PendingNew` | 501 | → `Unknown` |

### OMS-133 force_unknown 把非终态槽置 Unknown 并返回受影响数

- 规则:[入口](README.md#入口全部在-sequencer-线程调用mut-self无分配)
- 级别:unit
- 里程碑:M2
- 前置:P0;a1@VS:`Live` 2 个、`PendingNew` 1 个、`Filled` 1 个(未回收)、`Unknown` 1 个;下单连接 Up
- 步骤:
  1. `force_unknown(VS, a1, now, cause_seq)`
- 期望:
  - 返回 3;三个槽 → `Unknown`(cause `ForceUnknown(21)`);已在 `Unknown` 的与 `Filled` 的不变;`unknown_count = 4`;`OrderDecision{Query}` 三条
- 实现:`crates/hft-oms/tests/unknown.rs::oms_133_force_unknown`

### OMS-135 进入 Unknown 时连接 Up 即查询并登记定时器,到期再查

- 规则:[Unknown 收敛流程](README.md#unknown-收敛流程)、[入口](README.md#入口全部在-sequencer-线程调用mut-self无分配)
- 级别:unit
- 里程碑:M2
- 前置:P0;`unknown_query_interval_ms = 1000`;下单连接 Up;s 在 t = 0 经 `OutcomeUnknown` 进 `Unknown`
- 步骤:
  1. 观察 t = 0 的输出
  2. t = 1000 `on_unknown_query(id(s))`
  3. s 收敛为 `Live`(`QueryResult{New}`,t = 1500);t = 2000 `on_unknown_query(id(s))`
- 期望:
  - 步骤 1:`OrderDecision{Query, target_slot = s}` 一条;`OmsTimerReq{UnknownQuery, id = 0 | s.slot << 8, deadline = 1000 ms}` 一条
  - 步骤 2:`Query` 一条;新定时器 `deadline = 2000 ms`
  - 步骤 3:无输出,不再登记
- 实现:`crates/hft-oms/tests/unknown.rs::oms_135_query_on_enter_and_timer`

### OMS-136 连接断开期间不查询,恢复 Up 时对全部 Unknown 与 VERIFY 槽各查一次

- 规则:[Unknown 收敛流程](README.md#unknown-收敛流程)
- 级别:unit
- 里程碑:M2
- 前置:P0;同 OMS-130 的状态(下单连接 Down,3 个 `Unknown`、1 个 `VERIFY`)
- 步骤:
  1. t = 1100 依次 `on_unknown_query` 四个槽
  2. t = 1500 `ConnState{VX, a1, Order, Up}`
- 期望:
  - 步骤 1:无 `OrderDecision`;四个定时器各重新登记(`deadline = 2100 ms`)
  - 步骤 2:`OrderDecision{Query}` 恰好四条;`conn_up = true`;`venue_ready` 仍为 false(`unknown_count = 3`)
- 实现:`crates/hft-oms/tests/unknown.rs::oms_136_no_query_while_down`

### OMS-137 查询重试间隔取 unknown_query_interval_ms

- 规则:[Unknown 收敛流程](README.md#unknown-收敛流程)、[配置项](README.md#配置项omsconfig)
- 级别:table
- 里程碑:M2
- 前置:P0;s 在 `now = 0` 进 `Unknown`,下单连接 Up;只改 `unknown_query_interval_ms`
- 实现:`crates/hft-oms/tests/unknown.rs::oms_137_query_interval`

| 行 | `unknown_query_interval_ms` | 期望 |
|---|---|---|
| OMS-137.1 | 1000 | 定时器 `deadline_tsc = 1 000 000 000` |
| OMS-137.2 | 1 | `deadline_tsc = 1 000 000` |
| OMS-137.3 | 250 | `deadline_tsc = 250 000 000`;在 250 ms 到期后的再登记为 500 000 000 |

### OMS-138 写出的查询超过 unknown_max_queries 时写一次 OmsUnknownStuck

超过阈值后 `REST_QUERY` 保存到槽/快照，后续定时器与断连重连均只请求 reconciler；`QueryUnsupported` 同样停止 gateway 查询。回归保持 Unknown，匹配对账证据确认 Cancelled 后清位并停止轮询。0/5 的既有“超过”阈值不变，255 饱和停止。

- 规则:[Unknown 收敛流程](README.md#unknown-收敛流程)、[配置项](README.md#配置项omsconfig)
- 级别:table
- 里程碑:M2
- 前置:P0;s `Unknown`(非 `HIST_WAIT`);每次查询都收到 `SendReport{Query, Written}` 且无结果
- 实现:`crates/hft-oms/tests/unknown.rs::oms_138_unknown_stuck`

| 行 | `unknown_max_queries` / 写出次数 | 期望 |
|---|---|---|
| OMS-138.1 | 5 / 5 | 无 `OmsUnknownStuck`;`query_count = 5` |
| OMS-138.2 | 5 / 6 | 第 6 次写出时 `Note{OmsUnknownStuck}`(P0)一条 |
| OMS-138.3 | 5 / 7 | 仍只有一条 |
| OMS-138.4 | 0 / 1 | 第 1 次写出即一条 |
| OMS-138.5 | 5 / 6,但第 6 个 `SendReport` 为 `RateLimited` | 无 `OmsUnknownStuck`;`query_count = 5` |

### OMS-139 grace 内的 NotFound 只保持 Unknown 并继续按 client id 查询

- 规则:[Unknown 收敛流程](README.md#unknown-收敛流程)
- 级别:unit
- 里程碑:M2
- 前置:P0;`not_found_grace_ms[VS] = 5500`;s(VS,非链接)`sent_tsc = 0`,t = 100 进 `Unknown`
- 步骤:
  1. t = 1000 `QueryResult{NotFound}`
  2. t = 1100 `on_unknown_query(id(s))`
- 期望:
  - 步骤 1:s 保持 `Unknown`;不置 `HIST_WAIT`;无 `OmsNeedsRestQuery`
  - 步骤 2:`OrderDecision{Query}` 一条
- 实现:`crates/hft-oms/tests/unknown.rs::oms_139_notfound_within_grace`

### OMS-140 RecvWindow 与无过期保护 venue 的 NotFound 超过 grace 才请求成交历史

- 规则:[Unknown 收敛流程](README.md#unknown-收敛流程)、[配置项](README.md#配置项omsconfig)
- 级别:table
- 里程碑:M2
- 前置:P0;非链接槽,`sent_tsc = 0`,处于 `Unknown`;在 `now` 投递 `QueryResult{NotFound}`
- 实现:`crates/hft-oms/tests/unknown.rs::oms_140_grace_boundary`

| 行 | venue / grace | `now`(ms) | 期望 |
|---|---|---|---|
| OMS-140.1 | VS / 5500 | 5499 | 保持 `Unknown`,无 `HIST_WAIT` |
| OMS-140.2 | VS / 5500 | 5500 | 同 .1 |
| OMS-140.3 | VS / 5500 | 5501 | 置 `HIST_WAIT`;`Note{OmsNeedsRestQuery}`(`trades = 1`,`v[0] = 2 \| 1 << 8 \| 1 << 16`)一条;仍为 `Unknown` |
| OMS-140.4 | VG(`stale_guard = None`)/ 2000 | 2001 | 同 .3(`v[0] = 10 \| 1 << 8 \| 1 << 16`) |
| OMS-140.5 | VG / 2000 | 20 000 | 仍为 `Unknown`(不按超时定终态);`HIST_WAIT` |

### OMS-141 等待成交历史期间的重请求不发 Query、不计入 query_count

- 规则:[Unknown 收敛流程](README.md#unknown-收敛流程)
- 级别:unit
- 里程碑:M2
- 前置:P0;`unknown_max_queries = 5`;s 带 `HIST_WAIT`,`query_count = 2`;下单连接 Up
- 步骤:
  1. 连续 20 次 `on_unknown_query(id(s))`(每次间隔 1000 ms)
- 期望:
  - 每次输出 `Note{OmsNeedsRestQuery}`(`trades = 1`)一条、无 `OrderDecision`;`query_count` 保持 2;全程无 `OmsUnknownStuck`
- 实现:`crates/hft-oms/tests/unknown.rs::oms_141_hist_wait_rerequest`

### OMS-142 history_checked = 0 的确认结果只记录、不收敛

- 规则:[Unknown 收敛流程](README.md#unknown-收敛流程)
- 级别:unit
- 里程碑:M2
- 前置:P0;s 带 `HIST_WAIT`,`query_count = 2`
- 步骤:
  1. 投递 `ReconcileReport{item UnknownResolved, slot s, gen, epoch 7, order_status NotFound(7), history_checked 0}`
- 期望:
  - s 保持 `Unknown` 与 `HIST_WAIT`;无 `OmsTransition`、无 `OrderUpdate`;`query_count = 2`
- 实现:`crates/hft-oms/tests/unknown.rs::oms_142_history_unchecked_recorded`

### OMS-143 history_checked = 1 且无成交:定 Rejected(NOT_FOUND)

- 规则:[Unknown 收敛流程](README.md#unknown-收敛流程)、[回报处理表](README.md#回报处理表)
- 级别:unit
- 里程碑:M2
- 前置:P0;s(买 X 0.01,GTC)带 `HIST_WAIT`、`filled = 0`;`unknown_count = 1`
- 步骤:
  1. 投递 `ReconcileReport{UnknownResolved, (epoch 7, slot s, gen), NotFound, history_checked 1, exch_qty 0}`
- 期望:
  - s → `Rejected`,`reject_code = 0`;`OmsTransition{8 → 7, Reconcile(13)}`;`OrderUpdate::Rejected{bits: 0, venue_code: 0}`;`unknown_count = 0`;`HIST_WAIT` 清除
  - `open_buy`、`imm_buy` 各 −0.01;`venue_ready(VS, a1) = true` 且 `OmsVenueReady` 一条
- 实现:`crates/hft-oms/tests/unknown.rs::oms_143_history_confirmed_not_found`

### OMS-144 history_checked = 1 且有成交:按 exch_qty 应用增量后按终态迁移

- 规则:[Unknown 收敛流程](README.md#unknown-收敛流程)
- 级别:table
- 里程碑:M2
- 前置:P0;s(买 X 0.01,`px 60000.00`)带 `HIST_WAIT`、`filled = 0`;条目 `history_checked = 1`
- 实现:`crates/hft-oms/tests/unknown.rs::oms_144_history_confirmed_with_fills`

| 行 | 条目 | 期望 |
|---|---|---|
| OMS-144.1 | `order_status Filled(3)`,`exch_qty 0.01` | `FillOut{qty 0.01, px 60000.00, flags ⊇ PX_EST}`;`OmsFillPxEstimated` 与 `OmsNeedsRestQuery{trades = 1}` 各一条;s → `Filled` |
| OMS-144.2 | `order_status Cancelled(4)`,`exch_qty 0.004` | `FillOut{qty 0.004, PX_EST}`;s → `Cancelled` |

### OMS-145 不匹配或槽已不是 Unknown/VERIFY 的对账条目被忽略

- 规则:[Unknown 收敛流程](README.md#unknown-收敛流程)、[Note 码表](README.md#note-码表)
- 级别:table
- 里程碑:M2
- 前置:P0;s 当前 `gen = 2`、`epoch = 7`
- 实现:`crates/hft-oms/tests/unknown.rs::oms_145_recon_entry_ignored`

| 行 | s 的状态 | 条目 | 期望 |
|---|---|---|---|
| OMS-145.1 | `Unknown` | `UnknownResolved{gen 1, Filled}` | s 不变;`Note{OmsAnomaly}`(`v[0] = 4`,P1)一条 |
| OMS-145.2 | `Unknown` | `UnknownResolved{epoch 6, gen 2}` | 同 .1 |
| OMS-145.3 | `Live`,无 `VERIFY` | `OpenOrder{gen 2}` | 同 .1 |
| OMS-145.4 | `Live` + `VERIFY` | `OpenOrder{epoch 7, gen 2}` | `VERIFY` 清除;无 `OmsAnomaly` |
| OMS-145.5 | `Unknown`(prev `PendingNew`) | `OpenOrder{epoch 7, gen 2}` | → `Live`;`OrderUpdate{Acked}` |

### OMS-146 ExpireAt venue 上的链接腿 NotFound 仍走成交历史确认

- 规则:[Unknown 收敛流程](README.md#unknown-收敛流程)
- 级别:unit
- 里程碑:M2
- 前置:P0;`not_found_grace_ms[VX] = 3500`;L1 第二腿槽 h(VX,`role = Second`)`Unknown`,`sent_tsc = 0`
- 步骤:
  1. t = 3501 `QueryResult{NotFound}`;t = 10 000 再一次
- 期望:
  - 两次之后 h 仍为 `Unknown`,带 `HIST_WAIT`;`OmsNeedsRestQuery{trades = 1}` 至少一条;无 `NotFoundExpire` 迁移
- 实现:`crates/hft-oms/tests/unknown.rs::oms_146_expire_at_linked_needs_history`

### OMS-147 ExpireAt venue 上的非链接槽在 grace 后定 Rejected(NOT_FOUND)

- 规则:[Unknown 收敛流程](README.md#unknown-收敛流程)
- 级别:table
- 里程碑:M2
- 前置:P0;`not_found_grace_ms[VX] = 3500`;a1@VX 非链接槽 t `Unknown`,`sent_tsc = 0`;在 `now` 投递 `QueryResult{NotFound}`
- 实现:`crates/hft-oms/tests/unknown.rs::oms_147_expire_at_terminal`

| 行 | `now`(ms) | 期望 |
|---|---|---|
| OMS-147.1 | 3499 | 保持 `Unknown` |
| OMS-147.2 | 3500 | 保持 `Unknown` |
| OMS-147.3 | 3501 | → `Rejected`,`reject_code = 0`,cause `NotFoundExpire(22)`;无 `OmsNeedsRestQuery` |

### OMS-148 定终态之后到达的成交照样入账

- 规则:[Unknown 收敛流程](README.md#unknown-收敛流程)、[墓碑与迟到成交](README.md#墓碑与迟到成交c1)
- 级别:table
- 里程碑:M2
- 前置:P0;同 OMS-147.3 之后(t 为 `Rejected`,`filled = 0`);`reuse_quarantine_ms = 5000`
- 实现:`crates/hft-oms/tests/unknown.rs::oms_148_fill_after_not_found`

| 行 | 到达时机 | 期望 |
|---|---|---|
| OMS-148.1 | 回收前:`Fill{cum 0.01}` | → `Filled`;`FillOut{qty 0.01}`;`OmsAnomaly{Table}` 与 `OmsNeedsRestQuery{trades = 1}` 各一条 |
| OMS-148.2 | 回收后(墓碑在):`PartialFill{cum 0.004}` | `FillOut{qty 0.004, flags ⊇ LATE}`;`Note{OmsLateFill}`(P0)一条;当前槽不变 |

### OMS-150 Unknown 持续超过 unknown_alarm_ms 时升 P0 一次

- 规则:[Unknown 收敛流程](README.md#unknown-收敛流程)、[配置项](README.md#配置项omsconfig)
- 级别:table
- 里程碑:M2
- 前置:P0;`unknown_alarm_ms = 30000`;s 在 `t_state = 0` 进 `Unknown`;在 `now` 调 `on_sweep`
- 实现:`crates/hft-oms/tests/sweep.rs::oms_150_unknown_alarm`

| 行 | 序列 | 期望 |
|---|---|---|
| OMS-150.1 | `now = 30 000` | 无 Note;未置 `ALARMED` |
| OMS-150.2 | `now = 30 001` | `Note{OmsUnknown}` 一条,`level = P0`,`v[1] = s.slot`;置 `ALARMED` |
| OMS-150.3 | `now = 30 001`,再 `now = 31 001` | 共一条 P0 `OmsUnknown` |
| OMS-150.4 | 升级后 s 收敛为 `Live`,`t = 40 000` 再进 `Unknown`,`now = 70 001` | `ALARMED` 已在收敛时清除;第二次升级再写一条 P0 |

### OMS-151 有 Unknown 第二腿槽的链接不重试也不反向

- 规则:[Unknown 收敛流程](README.md#unknown-收敛流程)、[OnFill](README.md#onfill)
- 级别:unit
- 里程碑:M2
- 前置:P0;L1(`on_fail = Retry`)第二腿 h `Unknown`,`owed = 0.012`;HedgeRetry 定时器已登记;另一例同条件 `on_fail = Reverse`
- 步骤:
  1. `on_hedge_retry(L1)`(两例各一次)
- 期望:
  - 两例都无 `HedgeRequest`(既无 `SecondRetry` 也无 `Reverse`)、无 `OrderDecision{New}`
- 实现:`crates/hft-oms/tests/link.rs::oms_151_unknown_leg_blocks_retry`

### OMS-152 venue_ready 由假变真时恰好一条 OmsVenueReady

- 规则:[Unknown 收敛流程](README.md#unknown-收敛流程)、[入口](README.md#入口全部在-sequencer-线程调用mut-self无分配)
- 级别:table
- 里程碑:M2
- 前置:P0;a1@VS 表
- 实现:`crates/hft-oms/tests/unknown.rs::oms_152_venue_ready_note`

| 行 | 序列 | 期望 |
|---|---|---|
| OMS-152.1 | 连接 Up,唯一的 `Unknown` 槽收敛 | `venue_ready = true`;`Note{OmsVenueReady}`(`v[0] = 258`)一条 |
| OMS-152.2 | 无 `Unknown` 槽:`Order Down` 后 `Order Up` | Down 时 `venue_ready = false`、无 Note;Up 时一条 |
| OMS-152.3 | 连接 Down 期间唯一的 `Unknown` 槽收敛,之后 `Order Up` | 收敛时无 Note;Up 时一条 |
| OMS-152.4 | 两个 `Unknown`,只收敛一个 | `venue_ready = false`;无 Note |

### OMS-153 对冲腿成交历史确认晚于截止:链接 Deadline Broken,确认后只更新 LegTracker

- 规则:[Unknown 收敛流程](README.md#unknown-收敛流程)、[LegLink 与 HedgePlan](README.md#leglink-与-hedgeplan)
- 级别:unit
- 里程碑:M2
- 前置:P0;`hedge_deadline_ms = 3000`、`not_found_grace_ms[VX] = 1000`;L1 第一腿 t = 0 成交 0.012;第二腿 h(卖 Y 0.012)`sent_tsc = 10 ms`,t = 50 进 `Unknown`,t = 2000 收到 `QueryResult{NotFound}`(已过 grace)并置 `HIST_WAIT`
- 步骤:
  1. t = 3000 `on_unknown_query(id(h))`
  2. t = 3001 `on_unknown_query(id(h))`
  3. t = 8000 `ReconcileReport{UnknownResolved, h, Filled, history_checked 1, exch_qty 0.012}`
- 期望:
  - 步骤 1:L1 仍 `Hedging`
  - 步骤 2:L1 `Broken{Deadline(2)}`;`LinkUpdate{Broken, 2}` 与 `OmsHedgeBroken`(P0)各一条
  - 步骤 3:h → `Filled`;腿 1 `net = −0.012`;L1 `hedge_filled = 0.012` 但保持 `Broken`;无 `HedgeRequest`、无 `OrderDecision{New}`
- 实现:`crates/hft-oms/tests/unknown.rs::oms_153_linked_notfound_deadline`

### OMS-154 查询结果与迟到成交竞争时成交量单调

- 规则:[Unknown 收敛流程](README.md#unknown-收敛流程)、[回报匹配与成交增量](README.md#回报匹配与成交增量)
- 级别:table
- 里程碑:M2
- 前置:P0;s `Unknown`(prev `PendingNew`),`filled = 0`
- 实现:`crates/hft-oms/tests/unknown.rs::oms_154_query_vs_late_fill`

| 行 | 到达顺序 | 期望 |
|---|---|---|
| OMS-154.1 | `QueryResult{PartiallyFilled, cum 0.004}`,然后 `Fill{cum 0.01}` | `FillOut` 两条(0.004、0.006);终态 `Filled`,`filled = 0.01` |
| OMS-154.2 | `Fill{cum 0.01}`,然后 `QueryResult{PartiallyFilled, cum 0.004}` | `FillOut` 一条(0.01);终态 `Filled`;第二条无 `FillOut`、`filled` 不回退 |

### OMS-160 回收时写墓碑、槽排到空闲表队尾、不写 Note

- 规则:[墓碑与迟到成交](README.md#墓碑与迟到成交c1)、[每次迁移产生的输出](README.md#每次迁移产生的输出)
- 级别:unit
- 里程碑:M2
- 前置:P0;`reuse_quarantine_ms = 5000`、`tombstone_gens = 2`;槽 3(`gen = 2`)`Filled`,`t_state = 0`,`filled = 0.01`、`net_filled = 0.00999`、`exch_order_id = 777`、`group g1`、`leg_ix 0`、`link_id = L1`、`role First`、`side Buy`、`oflags = LINKED`
- 步骤:
  1. t = 5001 `on_sweep`
- 期望:
  - 槽 3 → `Free`;`tombs[3 × 2 + 0]` = `{gen 2, epoch 7, oflags LINKED, inst 11, group g1, side Buy, leg_ix 0, strategy 1, role 1, link_id L1, filled 0.01, net_filled 0.00999, exch_order_id 777, freed_tsc 5001 ms}`
  - 空闲表队尾为 3;输出中无 Note、无 `OrderUpdate`;`OmsStats.recycled` +1;该槽 `SlotStamps.op = 0xFF`
- 实现:`crates/hft-oms/tests/tomb.rs::oms_160_recycle_writes_tomb`

### OMS-161 终态超过 reuse_quarantine_ms 才回收

- 规则:[墓碑与迟到成交](README.md#墓碑与迟到成交c1)、[StaleOrderSweep](README.md#staleordersweeptimeroms-sweep1-s)、[配置项](README.md#配置项omsconfig)
- 级别:table
- 里程碑:M2
- 前置:P0;`reuse_quarantine_ms = 5000`;终态槽 `t_state = 0`;在 `now` 调 `on_sweep`
- 实现:`crates/hft-oms/tests/tomb.rs::oms_161_reuse_quarantine`

| 行 | `now`(ms) | 期望 |
|---|---|---|
| OMS-161.1 | 4999 | 不回收 |
| OMS-161.2 | 5000 | 不回收 |
| OMS-161.3 | 5001 | 回收、写墓碑 |
| OMS-161.4 | `reuse_quarantine_ms = 0`,`now = 1` | 回收 |

### OMS-162 墓碑命中的迟到成交照常入账且不重开链接

- 规则:[墓碑与迟到成交](README.md#墓碑与迟到成交c1)
- 级别:unit
- 里程碑:M2
- 前置:P0;同 OMS-160 之后;槽 3 已被重新分配为 `gen = 3`(`Live`,另一张单);L1 `Done`
- 步骤:
  1. 投递 `PartialFill{slot 3, gen 2, epoch 7, cum 0.012, avg_px 60000.00, fee 0.72 USDT}`
- 期望:
  - `FillOut{qty 0.002, net_qty 0.002, flags ⊇ LATE, group g1, leg_ix 0}` 一条;`legs(a1, g1).leg[0].net` +0.002;`tomb.filled = 0.012`、`tomb.net_filled = 0.01199`
  - `Note{OmsLateFill}`(P0)一条;槽 3(gen 3)不变;L1 仍 `Done`;无 `HedgeRequest`、无 `OrderDecision`、无 `LinkUpdate`
- 实现:`crates/hft-oms/tests/tomb.rs::oms_162_late_fill_hits_tomb`

### OMS-163 超过 tombstone_gens 代的回报按 StaleGen 处理

- 规则:[墓碑与迟到成交](README.md#墓碑与迟到成交c1)
- 级别:unit
- 里程碑:M2
- 前置:P0;`tombstone_gens = 2`;槽 3 已回收过 gen 1、2、3(墓碑存 gen 2、3;gen 1 被覆盖)
- 步骤:
  1. 投递 `PartialFill{slot 3, gen 1, exch_order_id 8888, cum 0.001}`
- 期望:
  - 不命中墓碑:`ForeignFill{qty 0.001, exch_order_id 8888}` 一条;`OmsStaleGenFill`(P0)一条;无 `OmsLateFill`
- 实现:`crates/hft-oms/tests/tomb.rs::oms_163_tomb_overwritten`

### OMS-164 墓碑超过 tombstone_keep_ms 由 sweep 清零

- 规则:[墓碑与迟到成交](README.md#墓碑与迟到成交c1)、[StaleOrderSweep](README.md#staleordersweeptimeroms-sweep1-s)、[配置项](README.md#配置项omsconfig)
- 级别:table
- 里程碑:M2
- 前置:P0;`tombstone_keep_ms = 3600000`;墓碑 `freed_tsc = 0`;在 `now` 调 `on_sweep`(游标覆盖该槽),随后投递命中该墓碑代数的 `PartialFill{exch_order_id 777}`
- 实现:`crates/hft-oms/tests/tomb.rs::oms_164_tomb_keep`

| 行 | `now`(ms) | 期望 |
|---|---|---|
| OMS-164.1 | 3 599 999 | 墓碑保留;成交按 `OmsLateFill` 入账 |
| OMS-164.2 | 3 600 000 | 墓碑保留 |
| OMS-164.3 | 3 600 001 | 墓碑清零;成交按 `StaleGen` 处理(`OmsStaleGenFill`) |

### OMS-170 sweep 每次最多检查 sweep_batch 个非空闲槽,游标跨次推进

- 规则:[StaleOrderSweep](README.md#staleordersweeptimeroms-sweep1-s)、[配置项](README.md#配置项omsconfig)
- 级别:table
- 里程碑:M2
- 前置:P0;`slots[(VS, a1)] = 1000`;600 个非空闲 `PendingNew` 槽,`t_state = 0`;`ack_timeout_ms = 2000`;从 `now = 3000` 起每次 `on_sweep`
- 实现:`crates/hft-oms/tests/sweep.rs::oms_170_sweep_cursor`

| 行 | `sweep_batch` / 调用次数 | 期望 |
|---|---|---|
| OMS-170.1 | 256 / 1 | 恰好 256 个槽 → `Unknown`(槽号最小的 256 个非空闲槽) |
| OMS-170.2 | 256 / 2 | 512 个 |
| OMS-170.3 | 256 / 3 | 600 个全部;第 3 次只检查 88 个后游标回到起点 |
| OMS-170.4 | 1 / 1 | 1 个 |

### OMS-171 挂单年龄超过 max_rest_ms 时给拥有者一次 Aged,不替其撤单

- 规则:[StaleOrderSweep](README.md#staleordersweeptimeroms-sweep1-s)
- 级别:table
- 里程碑:M2
- 前置:P0;s(GTC)`Live`,`t_state = 0`,`SlotExt.max_rest_ms` 按行;在 `now` 调 `on_sweep`
- 实现:`crates/hft-oms/tests/sweep.rs::oms_171_aged_once`

| 行 | `max_rest_ms` / 序列 | 期望 |
|---|---|---|
| OMS-171.1 | 10 000 / `now = 10 000` | 无输出 |
| OMS-171.2 | 10 000 / `now = 10 001` | `OrderUpdate{Aged}` 一条;置 `AGED`;无 `OrderDecision` |
| OMS-171.3 | 10 000 / 10 001、11 001 | 共一条 `Aged` |
| OMS-171.4 | 10 000 / Aged 后改单回到 `Live`(`t_state = 12 000`),`now = 22 001` | 第二条 `Aged` |
| OMS-171.5 | 0 / `now = 10 000 000` | 无 `Aged` |
| OMS-171.6 | 10 000 / IOC `Live`,`now = 10 001` | 无 `Aged`(只对 GTC/PostOnly) |

### OMS-172 sweep 不发 Query

- 规则:[StaleOrderSweep](README.md#staleordersweeptimeroms-sweep1-s)
- 级别:unit
- 里程碑:M2
- 前置:P0;10 个 `Unknown` 槽,下单连接 Up,`UnknownQuery` 定时器已登记
- 步骤:
  1. `on_sweep`(`now` 晚于全部定时器的到期时刻)
- 期望:
  - 输出中无 `OrderDecision{Query}`;查询只在 `on_unknown_query` 中发出
- 实现:`crates/hft-oms/tests/sweep.rs::oms_172_sweep_no_query`

### OMS-180 带 LegLink 的第一腿建链接

- 规则:[LegLink 与 HedgePlan](README.md#leglink-与-hedgeplan)
- 级别:unit
- 里程碑:M2
- 前置:P0;`ApprovedIntent{买 X 0.012, IOC, group g1, link = L1 计划, hedge_ref_px = 60010.0}`
- 步骤:
  1. `place`
- 期望:
  - L1 `LinkEntry{state Open, first = 该槽, ref_px 60010.0, first_net 0, hedge_req 0, hedge_filled 0, hedge_open 0, first_fill_tsc 0, retries 0, flags 0}`
  - `LinkUpdate{state Open, first, tolerance 0.02, residual_policy Defer, flags 0}` 一条
  - 槽 `oflags ⊇ LINKED`、`SlotExt.role = First(1)`、`link_id = L1`;`OrderDecision.link_role = 1`;`legs(a1, g1).open_links = 1`
- 实现:`crates/hft-oms/tests/link.rs::oms_180_link_created`

### OMS-181 第一腿成交在同一事件内按净量产生第二腿

- 规则:[OnFill](README.md#onfill)、[LegLink 与 HedgePlan](README.md#leglink-与-hedgeplan)
- 级别:unit
- 里程碑:M2
- 前置:P0;OMS-180 之后;手续费 USDT;`now = 100 ms`、`hdr.seq = 1000`
- 步骤:
  1. 第一腿 `Fill{cum 0.012, avg_px 60000.01}`
- 期望:
  - 同一调用内:腿 0 `net = 0.012`;L1 `first_net = 0.012`、`first_fill_tsc = 100 ms`、`hedge_req = 0.012`、`state = Hedging`
  - `HedgeRequest{link L1, role Second, venue VX, account a1, side Sell, reduce_only false, inst Y, group g1, strategy 1, qty 0.012, basis 0.012, hedge_cap_qty 0.013, ref_px 60010.0, peer_venue VS, peer_account a1, trigger_seq 1000}` 恰好一条
  - `LinkUpdate{state Hedging, first_net 0.012}` 一条
- 实现:`crates/hft-oms/tests/link.rs::oms_181_onfill_basic`

### OMS-182 第二腿数量向零取整,零头按 group 累计到下一笔

- 规则:[OnFill](README.md#onfill)、[数量对齐](README.md#数量对齐)
- 级别:unit
- 里程碑:M2
- 前置:P0;L1 第一腿为 GTC 买 X 0.02;手续费 USDT
- 步骤:
  1. `PartialFill{cum 0.0123}`
  2. `PartialFill{cum 0.0131}`
- 期望:
  - 步骤 1:`HedgeRequest{qty 0.012, basis 0.0123, hedge_cap_qty 0.0133}`;`residual[g1][1] = 0.0003`
  - 步骤 2:`raw = 0.0008 + 0.0003 = 0.0011`;`HedgeRequest{qty 0.001, basis 0.0011}`;`residual = 0.0001`
- 实现:`crates/hft-oms/tests/link.rs::oms_182_residual_carry`

### OMS-183 低于最小下单量或最小名义的第二腿不发,全部进余数

- 规则:[OnFill](README.md#onfill)
- 级别:table
- 里程碑:M2
- 前置:P0;L1;`residual[g1][1] = 0`;`ref_px = 60010.0`;每行一次第一腿成交增量(手续费 USDT),Y 的 `min_qty`/`min_notional` 按行
- 实现:`crates/hft-oms/tests/link.rs::oms_183_min_qty_notional`

| 行 | 增量 / Y 的下限 | 期望 |
|---|---|---|
| OMS-183.1 | 0.0009 / `min_qty 0.001` | 不发;`residual = 0.0009` |
| OMS-183.2 | 0.001 / `min_qty 0.001` | `HedgeRequest{qty 0.001}`;`residual = 0` |
| OMS-183.3 | 0.012 / `min_notional 720.12` | 0.012 × 60010.0 = 720.12,不低于下限:发出 0.012 |
| OMS-183.4 | 0.012 / `min_notional 720.13` | 不发;`residual = 0.012` |
| OMS-183.5 | 0.001 / `min_qty 0.002`(lot 仍 0.001) | 不发;`residual = 0.001` |

### OMS-184 基础资产扣费的第一腿按净量对冲

- 规则:[OnFill](README.md#onfill)、[成交净量](README.md#成交净量b4)
- 级别:unit
- 里程碑:M2
- 前置:P0;L1 第一腿 IOC 买 X 0.0123
- 步骤:
  1. `Fill{cum 0.0123, fee 0.0000123, fee_asset BTC}`
- 期望:
  - `d_net = 0.0122877`;`HedgeRequest{qty 0.012, basis 0.0122877}`;`residual[g1][1] = 0.0002877`;L1 `first_net = 0.0122877`
- 实现:`crates/hft-oms/tests/link.rs::oms_184_base_fee_net_hedge`

### OMS-185 对冲比不是 1/1 时按 num/den 换算

- 规则:[OnFill](README.md#onfill)、[LegLink 与 HedgePlan](README.md#leglink-与-hedgeplan)
- 级别:table
- 里程碑:M2
- 前置:P0;L1 改 `ratio_num/ratio_den` 按行;手续费 USDT
- 实现:`crates/hft-oms/tests/link.rs::oms_185_ratio`

| 行 | ratio / 增量 | 期望 |
|---|---|---|
| OMS-185.1 | 2/1 / 0.005 | `HedgeRequest{qty 0.010, basis 0.010}` |
| OMS-185.2 | 1/3 / 0.006 | `qty 0.002` |
| OMS-185.3 | 1/3 / 0.0061 | `raw = 0.00203333…`(i128 向零取到 raw 1 = 0.00000001,即 0.00203333);`qty 0.002`;`residual = 0.00003333` |

### OMS-186 第二腿数量不超过该方向裸量,差额不进余数

- 规则:[OnFill](README.md#onfill)、[LegTracker](README.md#legtracker)
- 级别:unit
- 里程碑:M2
- 前置:P0;L1;腿 1 `net = −0.005`(执行层先前已按外来成交卖出 Y);手续费 USDT
- 步骤:
  1. 第一腿 `Fill{cum 0.0123}`
- 期望:
  - `q_fill = 0.012`、`naked_dir = 0.0123 − 0.005 = 0.0073`、`q = 0.007`;`HedgeRequest{qty 0.007, basis 0.0123}`
  - `Note{OmsHedgeNakedCapped}`(P1,`v = [L1, 1 200 000, 700 000, 730 000]`)一条
  - L1 `hedge_req = 0.007`;`residual[g1][1] = 0.0003`(只有取整零头)
- 实现:`crates/hft-oms/tests/link.rs::oms_186_naked_cap`

### OMS-187 裸量上限截到最小下单量以下时本次不发

- 规则:[OnFill](README.md#onfill)
- 级别:unit
- 里程碑:M2
- 前置:P0;L1;腿 1 `net = −0.0115`
- 步骤:
  1. 第一腿 `Fill{cum 0.012}`
- 期望:
  - `naked_dir = 0.0005`,`floor_to_lot = 0`:无 `HedgeRequest`;`OmsHedgeNakedCapped` 一条;`hedge_req = 0`;`residual = 0`
- 实现:`crates/hft-oms/tests/link.rs::oms_187_naked_cap_below_min`

### OMS-188 第二腿被交易所拒绝后按定时器以 SecondRetry 重发欠对冲量

- 规则:[OnFill](README.md#onfill)
- 级别:unit
- 里程碑:M2
- 前置:P0;`hedge_retry_min_ms = 100`;OMS-181 之后,第二腿 h 已 `place`(`hedge_open = 0.012`)
- 步骤:
  1. t = 200 h 收到 `Reject`
  2. t = 300 `on_hedge_retry(L1)`
- 期望:
  - 步骤 1:L1 `retries = 1`、`owed = 0.012`、`hedge_open = 0`;`OmsTimerReq{HedgeRetry, L1, deadline 300 ms}` 一条
  - 步骤 2:`HedgeRequest{role SecondRetry, qty 0.012, basis 0.012, hedge_cap_qty 0.013, reduce_only false}` 一条;`hedge_req` 仍为 0.012
- 实现:`crates/hft-oms/tests/link.rs::oms_188_second_retry`

### OMS-189 重试到期时刻取 hedge_retry_min_ms 与退避截止中较晚者

- 规则:[OnFill](README.md#onfill)、[入口](README.md#入口全部在-sequencer-线程调用mut-self无分配)
- 级别:table
- 里程碑:M2
- 前置:P0;L1 `Hedging`,`owed = 0.012`;失败发生在 `now = 200 ms`
- 实现:`crates/hft-oms/tests/link.rs::oms_189_retry_interval`

| 行 | 失败方式 / 参数 | 期望 HedgeRetry 到期 |
|---|---|---|
| OMS-189.1 | 交易所拒;`hedge_retry_min_ms = 100` | 300 ms |
| OMS-189.2 | `hedge_gated(Risk(RATE_BACKOFF), retry_at = 450 ms)`;100 | 450 ms |
| OMS-189.3 | `hedge_gated(Risk(RATE_BACKOFF), retry_at = 250 ms)`;100;另测 `RATE` 在 201 ms 可用 | 250 ms;另例 201 ms,不额外等待 100 ms |
| OMS-189.4 | 交易所拒;`hedge_retry_min_ms = 1` | 201 ms |
| OMS-189.5 | `hedge_gated(Px(StaleQuote), retry_at = 0)`;100 | 300 ms |

### OMS-190 重试被裸量上限截量时从 hedge_req 核销

- 规则:[OnFill](README.md#onfill)
- 级别:unit
- 里程碑:M2
- 前置:P0;L1 `owed = 0.012`(`hedge_req 0.012`);执行层随后把腿 1 卖出 0.007(腿 1 `net = −0.007`,`naked_dir = 0.005`)
- 步骤:
  1. `on_hedge_retry(L1)`
- 期望:
  - `HedgeRequest{SecondRetry, qty 0.005, basis 0.012}`;`hedge_req = 0.005`;`OmsHedgeNakedCapped`(`v = [L1, 1 200 000, 500 000, 500 000]`)一条
- 实现:`crates/hft-oms/tests/link.rs::oms_190_retry_naked_writeoff`

### OMS-191 重试量被截为 0 且无在途时链接 Done

第一腿槽与墓碑过期后,仍按 group 的第一腿品种与当前净量截量;无法确定第一腿时不发送重试。

- 规则:[OnFill](README.md#onfill)、[LegLink 与 HedgePlan](README.md#leglink-与-hedgeplan)
- 级别:unit
- 里程碑:M2
- 前置:P0;L1 第一腿已 `Filled` 0.012,`owed = 0.012`,`hedge_open = 0`;腿 1 `net = −0.012`(已由别的途径平掉)
- 步骤:
  1. `on_hedge_retry(L1)`
- 期望:
  - 无 `HedgeRequest`;`hedge_req = 0`;L1 `Done`;`LinkUpdate{Done}` 与 `OmsHedgeNakedCapped` 各一条;`legs(a1, g1).open_links = 0`
- 实现:`crates/hft-oms/tests/link.rs::oms_191_retry_capped_to_zero_done`

### OMS-192 第二腿与重试继承计划的 reduce_only,反向单恒为 reduce-only

- 规则:[OnFill](README.md#onfill)、[RISK_REDUCING 判定](README.md#risk_reducing-判定)
- 级别:table
- 里程碑:M2
- 前置:P0;L1 的 `HedgePlan.reduce_only` 按行;第二腿失败后重试或反向
- 实现:`crates/hft-oms/tests/link.rs::oms_192_reduce_only_inherit`

| 行 | `reduce_only` / 请求 | 期望 `HedgeRequest.reduce_only` |
|---|---|---|
| OMS-192.1 | 0 / Second | false |
| OMS-192.2 | 0 / SecondRetry | false |
| OMS-192.3 | 1 / Second | true |
| OMS-192.4 | 1 / SecondRetry | true |
| OMS-192.5 | 0 / Reverse(`on_fail = Reverse`) | true |

### OMS-193 门控拒绝不计次、登记重试

- 规则:[OnFill](README.md#onfill)、[对冲定价](README.md#对冲定价)
- 级别:table
- 里程碑:M2
- 前置:P0;L1 `Hedging`,`retries = 0`;`now = 500 ms`
- 实现:`crates/hft-oms/tests/link.rs::oms_193_gated_not_counted`

| 行 | 调用 | 期望 |
|---|---|---|
| OMS-193.1 | `hedge_gated(L1, Px(StaleQuote), 0)` | `retries = 0`;HedgeRetry 到期 600 ms;`OmsHedgePxStale`(P1)一条 |
| OMS-193.2 | `hedge_gated(L1, Px(BeyondCross), 0)` | `retries = 0`;`OmsHedgePxGated`(P1)一条 |
| OMS-193.3 | `hedge_gated(L1, Risk(KILLED), 0)` | `retries = 0`;无定价 Note |
| OMS-193.4 | `hedge_gated(L1, Risk(STALE_MID), 0)` | `retries = 0` |

### OMS-194 hedge_deadline_ms 的判定边界

- 规则:[OnFill](README.md#onfill)、[LegLink 与 HedgePlan](README.md#leglink-与-hedgeplan)、[配置项](README.md#配置项omsconfig)
- 级别:table
- 里程碑:M2
- 前置:P0;`hedge_deadline_ms = 3000`、`hedge_retry_max = 3`;L1 `first_fill_tsc = 0`;第二腿在 `now` 被交易所拒(`retries` 由 0 变 1)
- 实现:`crates/hft-oms/tests/link.rs::oms_194_deadline_boundary`

| 行 | `now`(ms) | 期望 |
|---|---|---|
| OMS-194.1 | 2999 | 登记重试;L1 `Hedging` |
| OMS-194.2 | 3000 | 登记重试;L1 `Hedging` |
| OMS-194.3 | 3001 | L1 `Broken{Deadline(2)}`;无重试定时器 |
| OMS-194.4 | `hedge_deadline_ms = 500`,`now = 501` | `Broken{Deadline}` |

### OMS-195 执行失败计次上限

- 规则:[OnFill](README.md#onfill)、[配置项](README.md#配置项omsconfig)
- 级别:table
- 里程碑:M2
- 前置:P0;`hedge_deadline_ms = 60000`;L1 第二腿与每次重试都被交易所拒
- 实现:`crates/hft-oms/tests/link.rs::oms_195_retry_cap`

| 行 | `hedge_retry_max` | 期望 |
|---|---|---|
| OMS-195.1 | 3 | 发出 3 条 `SecondRetry`;第 4 次失败后 `Broken{RetryExhausted(1)}` |
| OMS-195.2 | 1 | 1 条 `SecondRetry`;第 2 次失败后 `Broken` |
| OMS-195.3 | 0 | 无 `SecondRetry`;第 1 次失败即 `Broken` |

### OMS-196 IOC 部分成交后按余量重试

- 规则:[OnFill](README.md#onfill)
- 级别:unit
- 里程碑:M2
- 前置:P0;L1 第二腿 h(卖 Y 0.012,IOC)
- 步骤:
  1. h `PartialFill{cum 0.005}`,随后 `Expired`(t = 200)
  2. t = 300 `on_hedge_retry(L1)`
- 期望:
  - 步骤 1:`hedge_filled = 0.005`、`hedge_open = 0`、`owed = 0.007`;`retries = 1`(IOC 未全成计次)
  - 步骤 2:`HedgeRequest{SecondRetry, qty 0.007, basis 0.007, hedge_cap_qty 0.008}`
- 实现:`crates/hft-oms/tests/link.rs::oms_196_ioc_partial_retry`

### OMS-197 on_fail = Hold:失败后不发单,链接 Done

- 规则:[OnFill](README.md#onfill)
- 级别:unit
- 里程碑:M2
- 前置:P0;L1 改 `on_fail = Hold`;第一腿已 `Filled` 0.012
- 步骤:
  1. 第二腿被交易所拒
- 期望:
  - 无 `HedgeRequest`、无 HedgeRetry 定时器;L1 `Done`;`LinkUpdate{Done, owed 0.012}` 一条;腿 0 `net = 0.012` 保持(敞口交执行层)
- 实现:`crates/hft-oms/tests/link.rs::oms_197_on_fail_hold`

### OMS-198 on_fail = Reverse:对第一腿未对冲的净量发 reduce-only 反向单

- 规则:[OnFill](README.md#onfill)、[对冲定价](README.md#对冲定价)
- 级别:unit
- 里程碑:M2
- 前置:P0;L1 改 `on_fail = Reverse`;第一腿已 `Filled` 0.012;第二腿零成交
- 步骤:
  1. 第二腿被交易所拒
  2. 反向单 `Fill{cum 0.012}`
- 期望:
  - 步骤 1:`HedgeRequest{role Reverse, venue VS, account a1, inst X, side Sell, reduce_only true, qty 0.012, basis 0.012, hedge_cap_qty 0.01201, ref_px 0}` 一条;不受 `naked_dir` 截量;同一调用内 L1 `hedge_req = 0`(= `hedge_filled + hedge_open`),`owed = 0`;L1 仍为 `Hedging`(有在途 `Reverse`)
  - 步骤 2:L1 `first_net = 0`;L1 `Done`;`LinkUpdate{Done}` 一条
- 实现:`crates/hft-oms/tests/link.rs::oms_198_reverse`

### OMS-199 反向单失败按同一计次重发,耗尽后 ReverseFailed

- 规则:[OnFill](README.md#onfill)、[Simultaneous](README.md#simultaneous)
- 级别:unit
- 里程碑:M2
- 前置:P0;`hedge_retry_max = 3`、`hedge_deadline_ms = 60000`;同 OMS-198 步骤 1 之后(第二腿被拒已计 `retries = 1`)
- 步骤:
  1. 反向单 `PartialFill{cum 0.005}` 后 `Expired`;`on_hedge_retry(L1)`
  2. 之后每个反向单都被交易所拒,每次到期调 `on_hedge_retry(L1)`
- 期望:
  - 步骤 1:`retries = 2`;`first_net = 0.007`;重发 `HedgeRequest{Reverse, qty 0.007}`
  - 步骤 2:第一次被拒后 `retries = 3`,再重发一次;第二次被拒后 `retries = 4 > 3`:L1 `Broken{ReverseFailed(5)}`、`OmsHedgeBroken` 一条;全程共发出 3 条 `Reverse`(1 条初始、2 条重发)
- 实现:`crates/hft-oms/tests/link.rs::oms_199_reverse_retry_exhausted`

### OMS-200 HEDGE_LIMIT 否决后链接 Broken 且不再产生任何订单

- 规则:[OnFill](README.md#onfill)、[入口](README.md#入口全部在-sequencer-线程调用mut-self无分配)
- 级别:unit
- 里程碑:M2
- 前置:P0;L1 第一腿 GTC 0.02 已部分成交 0.012
- 步骤:
  1. `hedge_vetoed(L1)`
  2. 第一腿再 `PartialFill{cum 0.015}`;`on_hedge_retry(L1)`;`on_sweep`
- 期望:
  - 步骤 1:L1 `Broken{HedgeLimit(3)}`;`LinkUpdate` 与 `OmsHedgeBroken`(P0)各一条;无 HedgeRetry 定时器
  - 步骤 2:腿 0 `net = 0.015`;无 `HedgeRequest`、无 `OrderDecision{New}`
- 实现:`crates/hft-oms/tests/link.rs::oms_200_hedge_vetoed`

### OMS-201 链接状态迁到 Done 的条件

- 规则:[LegLink 与 HedgePlan](README.md#leglink-与-hedgeplan)
- 级别:table
- 里程碑:M2
- 前置:P0;L1(OnFill,`on_fail = Retry`)
- 实现:`crates/hft-oms/tests/link.rs::oms_201_done_conditions`

| 行 | 序列 | 期望 |
|---|---|---|
| OMS-201.1 | 第一腿 `Filled` 0.012;第二腿 `Filled` 0.012 | 第二腿成交的那次调用内 L1 → `Done`;`LinkUpdate{Done}` 一条 |
| OMS-201.2 | 第一腿零成交 `Cancelled` | L1 `Open` → `Done` |
| OMS-201.3 | 第一腿成交 0.004 后 `Cancelled`;第二腿成交 0.004 | `Done` |
| OMS-201.4 | 第一腿 `Filled` 0.012;第二腿被拒、HedgeRetry 已登记 | 保持 `Hedging` |
| OMS-201.5 | 第一腿仍 `Live`(GTC,已成交 0.004);第二腿成交 0.004 | 保持 `Hedging`(第一腿未终态) |

### OMS-202 Broken 时 LinkUpdate 带足投影字段

- 规则:[输出 OmsOut](README.md#输出-omsout容量固定由-sequencer-预分配并在每个输入事件后清空)、[风控所需的链接信息](README.md#风控所需的链接信息b1)
- 级别:unit
- 里程碑:M2
- 前置:P0;同 OMS-194.3
- 步骤:
  1. 触发 `Broken{Deadline}`
- 期望:
  - `LinkUpdate{link_id L1, account a1, group g1, first = 第一腿 SlotRef, state Broken, broken_reason 2, flags 0, first_net 0.012, hedge_filled 0, owed 0.012, tolerance 0.02, residual_policy Defer}` 恰好一条;`Note{OmsHedgeBroken}` `level = P0`、`group = g1`
- 实现:`crates/hft-oms/tests/link.rs::oms_202_broken_link_update`

### OMS-203 同 group 后一条链接吸收前一条的余数

- 规则:[OnFill](README.md#onfill)
- 级别:unit
- 里程碑:M2
- 前置:P0;L1 已 `Done`,`residual[g1][1] = 0.0003`;新链接 L2(同 g1、同计划)
- 步骤:
  1. L2 第一腿成交 0.0007
- 期望:
  - `raw = 0.001`;`HedgeRequest{link L2, qty 0.001}`;`residual[g1][1] = 0`
- 实现:`crates/hft-oms/tests/link.rs::oms_203_residual_across_links`

### OMS-204 截止从第一腿首次成交起算

已删除:截止起点改为最早一笔尚未对冲完的成交,由 OMS-459 替代。

### OMS-205 本 crate 不做补腿,Broken 之后不为该 group 产生订单

- 规则:[与 hft-exec 的边界](README.md#与-hft-exec-的边界)、[OnFill](README.md#onfill)
- 级别:unit
- 里程碑:M2
- 前置:P0;L1 `Broken{RetryExhausted}`,腿 0 `net = 0.012`、腿 1 `net = 0`
- 步骤:
  1. 依次 `on_sweep`、`external_adjust(a1, VX, g1, Y, −0.004, 0, Adl)`、`net_adjust(a1, g1, X, −0.00001)`、`on_hedge_retry(L1)`
- 期望:
  - 全部输出中无 `HedgeRequest`、无 `OrderDecision{New}`;`HedgeRequest.role` 在本 crate 的任何输出里都不取 `HedgeFix(4)`
- 实现:`crates/hft-oms/tests/link.rs::oms_205_no_auto_fix`

### OMS-206 过期的 HedgeRetry 定时器无害

- 规则:[OnFill](README.md#onfill)、[恢复:停机时间不计与闸门续接](README.md#恢复停机时间不计与闸门续接)
- 级别:table
- 里程碑:M2
- 前置:P0;L1 状态按行
- 实现:`crates/hft-oms/tests/link.rs::oms_206_stale_retry_timer`

| 行 | L1 状态 | 期望 `on_hedge_retry(L1)` |
|---|---|---|
| OMS-206.1 | `Done` | 无输出 |
| OMS-206.2 | `Broken` | 无输出 |
| OMS-206.3 | `Hedging`,`owed = 0`、`hedge_open = 0.012`(重试单在途) | 无输出 |
| OMS-206.4 | 链接表中不存在的 `link_id` | 无输出,不 panic |

### OMS-207 sweep 对链接的 Unknown 槽判截止

- 规则:[LegLink 与 HedgePlan](README.md#leglink-与-hedgeplan)、[StaleOrderSweep](README.md#staleordersweeptimeroms-sweep1-s)
- 级别:unit
- 里程碑:M2
- 前置:P0;`hedge_deadline_ms = 3000`;L1 第一腿 t = 0 成交 0.012,第二腿 h `Unknown`(`hedge_open = 0.012`)
- 步骤:
  1. t = 3000 `on_sweep`;t = 3001 `on_sweep`
- 期望:
  - t = 3000 无变化;t = 3001 L1 `Broken{Deadline}`,`OmsHedgeBroken` 一条
- 实现:`crates/hft-oms/tests/sweep.rs::oms_207_sweep_link_deadline`

### OMS-210 Simultaneous 第一腿 place 的同一事件产出第二腿

- 规则:[Simultaneous](README.md#simultaneous)
- 级别:unit
- 里程碑:M2
- 前置:P0;链接 L2:L1 计划改 `kind = Simultaneous`、`on_fail = Reverse`;`residual[g1][1] = 0.0004`;腿 1 `net = −0.02`(该方向裸量为 0)
- 步骤:
  1. `place` 第一腿(IOC 买 X 0.012)
- 期望:
  - 同一调用内 `HedgeRequest{role Second, qty 0.012, basis 0.0124, peer_venue VS, peer_account a1}` 一条(不受裸量上限约束);`residual[g1][1] = 0.0004`;L2 `Hedging`
- 实现:`crates/hft-oms/tests/simul.rs::oms_210_simultaneous_place`

### OMS-211 Simultaneous 第二腿少于第一腿时按 on_fail 处理

- 规则:[Simultaneous](README.md#simultaneous)
- 级别:table
- 里程碑:M2
- 前置:P0;同 OMS-210 之后;第一腿 `Filled` 0.012;第二腿 `PartialFill 0.005` 后 `Expired`
- 实现:`crates/hft-oms/tests/simul.rs::oms_211_second_short`

| 行 | `on_fail` | 期望 |
|---|---|---|
| OMS-211.1 | `Reverse` | 两腿终态的那次调用内 `HedgeRequest{Reverse, venue VS, inst X, side Sell, reduce_only true, qty 0.007}` 一条 |
| OMS-211.2 | `Hold` | 无请求;L2 `Done`;腿 0 `net = 0.012`、腿 1 `net = −0.025` |

### OMS-212 Simultaneous 第一腿少于第二腿时在第二腿上反向

- 规则:[Simultaneous](README.md#simultaneous)、[OnFill](README.md#onfill)
- 级别:unit
- 里程碑:M2
- 前置:P0;L2(`on_fail = Reverse`,腿 1 初始 `net = 0`);第一腿被交易所拒(0 成交);第二腿 `Filled` 0.012
- 步骤:
  1. 两腿都终态
  2. 反向单 `Fill{cum 0.012}`
- 期望:
  - 步骤 1:`HedgeRequest{Reverse, venue VX, inst Y, side Buy, reduce_only true, qty 0.012}` 一条
  - 步骤 2:L2 `hedge_filled = 0`;L2 `Done`
- 实现:`crates/hft-oms/tests/simul.rs::oms_212_first_short_reverse_second`

### OMS-213 Simultaneous 反向失败耗尽后 ReverseFailed

- 规则:[Simultaneous](README.md#simultaneous)
- 级别:unit
- 里程碑:M2
- 前置:P0;`hedge_retry_max = 3`、`hedge_deadline_ms = 60000`;同 OMS-212 步骤 1 之后
- 步骤:
  1. 每个反向单都被交易所拒
- 期望:
  - 共 1 条初始 `Reverse` 与 3 条重发;第 4 次失败后 `Broken{ReverseFailed(5)}`
- 实现:`crates/hft-oms/tests/simul.rs::oms_213_simul_reverse_failed`

### OMS-214 Simultaneous 两腿成交相等时直接 Done

- 规则:[Simultaneous](README.md#simultaneous)
- 级别:unit
- 里程碑:M2
- 前置:P0;L2;第一腿 `Filled` 0.012;第二腿 `Filled` 0.012
- 步骤:
  1. 两腿都终态
- 期望:
  - 无 `Reverse`;L2 `Done`
- 实现:`crates/hft-oms/tests/simul.rs::oms_214_simul_equal_done`

### OMS-220 三种定价模式在 dev ≤ max_slip 时的限价

- 规则:[对冲定价](README.md#对冲定价)
- 级别:table
- 里程碑:M2
- 前置:Y 报价 bid 59990.0 / ask 60010.0(mid 60000.0),连接健康;`lim = {2000, 0.005}`;`tick = 10`(0.1);`max_slip = 0.001`;`ref_px = 60010.0`;对每行同时断言结果与 hft-types `cap_cross_px` 的返回逐值相等(Cross/Mid)
- 实现:`crates/hft-oms/tests/price.rs::oms_220_modes`

| 行 | 方向 / 模式 / 参数 | 期望 `HedgePx` |
|---|---|---|
| OMS-220.1 | 卖 / Cross / `px_param 0.001` | `{px 59930.1, capped false, slipped false}`(max(59990 × 0.999, 59700) 向上取到 0.1) |
| OMS-220.2 | 卖 / Mid / 0.001 | `{px 59940.0, capped false}` |
| OMS-220.3 | 卖 / Limit / `limit_px 59950.0` | `{px 59950.0, capped false}` |
| OMS-220.4 | 卖 / Limit / `limit_px 59600.0` | `{px 59700.0, capped true}` |
| OMS-220.5 | 买 / Cross / 0.001(`ref_px 59990.0`) | `{px 60070.0, capped false}`(min(60010 × 1.001, 60300) 向下取到 0.1) |
| OMS-220.6 | 买 / Mid / 0.001 | `{px 60060.0, capped false}` |
| OMS-220.7 | 买 / Limit / `limit_px 60500.0` | `{px 60300.0, capped true}` |
| OMS-220.8 | 买 / Cross / `px_param 0.01` | `{px 60300.0, capped true}`(结果等于 cap) |

### OMS-221 超滑点时 Cross/Mid 按越价上限定价、Limit 保持策略价

- 规则:[对冲定价](README.md#对冲定价)
- 级别:table
- 里程碑:M2
- 前置:L1 卖 Y;`ref_px = 60010.0`、`max_slip = 0.001`;Y 报价移到 bid 59900.0 / ask 59920.0(mid 59910.0,卖 cap = 59610.5);连接健康
- 实现:`crates/hft-oms/tests/price.rs::oms_221_slip_capped`

| 行 | 模式 | 期望 |
|---|---|---|
| OMS-221.1 | Cross | `{px 59610.5, capped true, slipped true, touch 59900.0, cap 59610.5}`(dev = 110 / 60010 ≈ 0.00183 > 0.001) |
| OMS-221.2 | Mid | 同 .1 |
| OMS-221.3 | Limit,`limit_px 59950.0` | `{px 59950.0, capped false, slipped false}` |
| OMS-221.4 | Cross,报价 bid 59950.0 / ask 59970.0(dev = 60 / 60010 ≈ 0.001 以内) | `slipped false`;`px` = `cap_cross_px` 结果 |

### OMS-222 超滑点 Note 由对冲请求的 place 写,每链接每次进入一条

- 规则:[对冲定价](README.md#对冲定价)、[Note 码表](README.md#note-码表)
- 级别:unit
- 里程碑:M2
- 前置:P0;L1;sequencer 把 OMS-221.1 的 `HedgePx` 放进 `ApprovedIntent.hedge_px`
- 步骤:
  1. `place` 第二腿(slipped)
  2. 重试再 `place`(仍 slipped)
  3. 再 `place` 一条 `slipped = false` 的;再 `place` 一条 slipped 的
- 期望:
  - 步骤 1:`Note{OmsHedgeSlipCapped}`(P1,`v = [L1, 6 001 000, 5 990 000, 5 961 050]`)一条;置 `SLIP_NOTED`
  - 步骤 2:无该 Note
  - 步骤 3:第一条清 `SLIP_NOTED`;第二条再写一条
- 实现:`crates/hft-oms/tests/price.rs::oms_222_slip_note_once`

### OMS-223 对手价越过越价上限返回 BeyondCross

- 规则:[对冲定价](README.md#对冲定价)
- 级别:table
- 里程碑:M2
- 前置:Y 报价 bid 59600.0 / ask 60400.0(mid 60000.0;买 cap 60300.0、卖 cap 59700.0);连接健康;`lim = {2000, 0.005}`
- 实现:`crates/hft-oms/tests/price.rs::oms_223_beyond_cross`

| 行 | 方向 | 期望 |
|---|---|---|
| OMS-223.1 | 买(ask 60400.0 > 60300.0) | `Err(HedgePxErr::BeyondCross)` |
| OMS-223.2 | 卖(bid 59600.0 < 59700.0) | `Err(HedgePxErr::BeyondCross)` |
| OMS-223.3 | 买,ask 改为 60300.0 | `Ok`,`px = 60300.0` |

### OMS-224 盘口 10 s 未变而连接健康时照常定价

- 规则:[对冲定价](README.md#对冲定价)
- 级别:unit
- 里程碑:M2
- 前置:Y 的 BBO 在 `now − 10 000 ms` 之后没有变化(`BboView.rx_tsc = now − 10 000 ms`),连接每 500 ms 回 pong(`conn_live_tsc = now − 400 ms`),`conn_up = true`、`resync = false`;`lim = {2000, 0.005}`
- 步骤:
  1. `price_hedge`(L1 卖,Cross,0.001)
- 期望:
  - `Ok{px 59930.1}`,与 OMS-220.1 相同;不返回 `StaleQuote`
- 实现:`crates/hft-oms/tests/price.rs::oms_224_quiet_book_prices`

### OMS-225 报价连接不健康时有保护价用保护价,否则 StaleQuote

- 规则:[对冲定价](README.md#对冲定价)
- 级别:table
- 里程碑:M2
- 前置:L1 卖 Y;`lim.conn_liveness_ms = 2000`;每行两支:`limit_px = 0` 与 `limit_px = 59950.0`(Cross 模式下作保护价)
- 实现:`crates/hft-oms/tests/price.rs::oms_225_stale_quote`

| 行 | 报价状态 | `limit_px = 0` | `limit_px = 59950.0` |
|---|---|---|---|
| OMS-225.1 | `conn_up = false` | `Err(StaleQuote)` | `Ok{px 59950.0, capped false}` |
| OMS-225.2 | `resync = true` | `Err(StaleQuote)` | `Ok{px 59950.0}` |
| OMS-225.3 | `now − conn_live_tsc = 1999 ms` | `Ok{px 59930.1}` | `Ok{px 59930.1}` |
| OMS-225.4 | `now − conn_live_tsc = 2000 ms` | `Ok{px 59930.1}` | `Ok{px 59930.1}` |
| OMS-225.5 | `now − conn_live_tsc = 2001 ms` | `Err(StaleQuote)` | `Ok{px 59950.0}` |
| OMS-225.6 | `bid_px = 0` | `Err(StaleQuote)` | `Ok{px 59950.0}` |
| OMS-225.7 | `ask_px = 0` | `Err(StaleQuote)` | `Ok{px 59950.0}` |

### OMS-226 限价按对冲腿 inst 的 tick 取整

- 规则:[对冲定价](README.md#对冲定价)
- 级别:table
- 里程碑:M2
- 前置:两个 inst 用同一组报价 bid 59990.00 / ask 60010.00(mid 60000.00);买,Cross,`px_param 0.001`;`lim = {2000, 0.005}`
- 实现:`crates/hft-oms/tests/price.rs::oms_226_tick_from_inst`

| 行 | inst / `tick` | 期望 `px` |
|---|---|---|
| OMS-226.1 | X / 1(0.01) | 60070.01 |
| OMS-226.2 | Y / 10(0.1) | 60070.0 |

### OMS-227 HedgePxLimits 取对冲单所在账户的值,与风控 #5 同值

- 规则:[对冲定价](README.md#对冲定价)、[配置项](README.md#配置项omsconfig)
- 级别:integ
- 里程碑:M2
- 前置:testkit 进程内 sequencer + hft-risk;`RiskParams`:Y 所在价格带 `rr_max_cross = 0.005`(价格带按 inst 归类,其余 inst 另一档);L1(a1)第二腿报价 bid 59710.0 / ask 60290.0(中价 60000.0)下的买单
- 步骤:
  1. 第一腿成交,第二腿进对冲定价
- 期望:
  - `price_hedge` 收到的 `lim = {2000, 0.005}`,与 `risk.hedge_px_limits(a1, VX, Y)` 逐字段相等;定出的买价 60300.0(= 越价上限,`capped = true`)经风控 #5 放行
- 实现:`crates/hft-sequencer/tests/oms_integ.rs::oms_227_limits_same_source`

### OMS-228 ref_px 为 0 时不判超滑点

- 规则:[对冲定价](README.md#对冲定价)
- 级别:unit
- 里程碑:M2
- 前置:L1 但 `ref_px = 0`(第一腿 `place` 时第二腿报价不可用);报价同 OMS-221
- 步骤:
  1. `price_hedge`(Cross,0.001)
- 期望:
  - `slipped = false`;`px` = `cap_cross_px(Sell, 59900.0, 59910.0, 0.001, lim, 10)` 的结果 59840.1
- 实现:`crates/hft-oms/tests/price.rs::oms_228_zero_ref_px`

### OMS-229 反向单按 Cross 在被反向腿的报价上定价,不用 limit_px

- 规则:[对冲定价](README.md#对冲定价)、[OnFill](README.md#onfill)
- 级别:unit
- 里程碑:M2
- 前置:L1 改 `px_mode = Limit`、`limit_px = 59950.0`、`px_param = 0.001`;反向单卖 X(`ref_px = 0`);X 报价 bid 60000.00 / ask 60000.01,`tick = 1`
- 步骤:
  1. `price_hedge(req = Reverse, …)`
- 期望:
  - `Ok{px 59940.00, capped false, slipped false}`(max(60000.00 × 0.999, mid 60000.00 × 0.995))
- 实现:`crates/hft-oms/tests/price.rs::oms_229_reverse_pricing`

### OMS-230 定价门控 Note 每链接每次进入一条

- 规则:[对冲定价](README.md#对冲定价)、[Note 码表](README.md#note-码表)
- 级别:table
- 里程碑:M2
- 前置:P0;L1 `Hedging`
- 实现:`crates/hft-oms/tests/price.rs::oms_230_gate_note_once`

| 行 | 序列 | 期望 |
|---|---|---|
| OMS-230.1 | `hedge_gated(Px(BeyondCross))` × 2 | `OmsHedgePxGated` 一条 |
| OMS-230.2 | BeyondCross,随后一次对冲 `place`,再 BeyondCross | `OmsHedgePxGated` 两条 |
| OMS-230.3 | `hedge_gated(Px(StaleQuote))` × 3 | `OmsHedgePxStale` 一条 |
| OMS-230.4 | StaleQuote 后 BeyondCross | `OmsHedgePxStale`、`OmsHedgePxGated` 各一条 |

### OMS-235 第一腿按 pair_lot 取整后全部成交,第二腿为其 lot 的整数倍

- 规则:[数量对齐](README.md#数量对齐)
- 级别:unit
- 里程碑:M2
- 前置:P0;L1;`lot_X = 0.00001`、`lot_Y = 0.001`,`pair_lot = 0.001`;第一腿 IOC 买 X 0.012(0.001 的整数倍);手续费 USDT
- 步骤:
  1. 第一腿 `Fill{cum 0.012}`
- 期望:
  - `HedgeRequest{qty 0.012}`;`residual[g1][1] = 0`
- 实现:`crates/hft-oms/tests/link.rs::oms_235_pair_lot_full_fill`

### OMS-236 不按 pair_lot 对齐的第一腿照收、不重新取整

- 规则:[数量对齐](README.md#数量对齐)
- 级别:unit
- 里程碑:M2
- 前置:P0;L1;第一腿意图 `qty = 0.01234`
- 步骤:
  1. `place`;`Fill{cum 0.01234}`
- 期望:
  - `place` 返回 `Ok`,`OrderDecision.qty = 0.01234`;第二腿 `qty 0.012`、`residual = 0.00034`
- 实现:`crates/hft-oms/tests/link.rs::oms_236_unaligned_first_leg_accepted`

### OMS-240 链接两侧的 venue 与就绪状态可查

- 规则:[风控所需的链接信息](README.md#风控所需的链接信息b1)、[入口](README.md#入口全部在-sequencer-线程调用mut-self无分配)
- 级别:unit
- 里程碑:M2
- 前置:P0;L1 已建;a1@VX 有一个 `Unknown` 槽
- 步骤:
  1. 调 `link_peer(L1)`、`link_peer(999)`、`venue_ready(VS, a1)`、`venue_ready(VX, a1)`
- 期望:
  - `Some((VX, a1))`、`None`、`true`、`false`
- 实现:`crates/hft-oms/tests/link.rs::oms_240_link_peer_and_ready`

### OMS-245 槽与决策的 oflags 由 classify 结果与来源填写

- 规则:[RISK_REDUCING 判定](README.md#risk_reducing-判定)
- 级别:table
- 里程碑:M2
- 前置:P0;各行一个 `ApprovedIntent`(`risk_reducing`、`reduce_only` 为 hft-risk `classify()` 的结果)
- 实现:`crates/hft-oms/tests/flags.rs::oms_245_oflags_from_classify`

| 行 | 意图 | 期望 `oflags` |
|---|---|---|
| OMS-245.1 | 策略 Open,GTC,无链接 | 0 |
| OMS-245.2 | 策略 Open,PostOnly | `POST_ONLY` = 0x02 |
| OMS-245.3 | 策略第一腿(带 LegLink) | `LINKED` = 0x04 |
| OMS-245.4 | OMS `Second`,`risk_reducing = 1`、`reduce_only = 0` | `LINKED \| RISK_REDUCING` = 0x14 |
| OMS-245.5 | OMS `SecondRetry`,`risk_reducing = 1`、`reduce_only = 1` | `REDUCE_ONLY \| LINKED \| RISK_REDUCING` = 0x15 |
| OMS-245.6 | OMS `Reverse` | 0x15 |
| OMS-245.7 | 执行层 `HedgeFix` 单(`LegLink.role = HedgeFix`,`risk_reducing = 1`) | `HEDGE_FIX \| RISK_REDUCING` = 0x18;槽置 `EXEC` |
| OMS-245.8 | 冒烟单 | `SMOKE \| POST_ONLY` = 0x22 |

### OMS-248 naked_groups 只列出候选、按 group 升序分批,不产生订单

- 规则:[入口](README.md#入口全部在-sequencer-线程调用mut-self无分配)、[RISK_REDUCING 判定](README.md#risk_reducing-判定)
- 级别:unit
- 里程碑:M2
- 前置:P0;a1 有 100 个 group,其中 70 个已成交裸量 `|Σ net| ≠ 0`,另有 5 个只有即时在途、已成交裸量为 0
- 步骤:
  1. `naked_groups(a1, &mut buf, &mut cursor = 0)`(容量 64);清空后同游标再调
- 期望:
  - 第一次返回 true、64 个,第二次返回 false、6 个;合计 70 个、按 `GroupId` 升序、不重不漏;只有即时在途的 5 个不出现
  - 两次调用都不改任何状态(`&self`),无输出
- 实现:`crates/hft-oms/tests/legs.rs::oms_248_naked_groups_batches`

### OMS-250 external_adjust 按带符号净量更新腿,不动槽与链接

- 规则:[外来成交](README.md#外来成交b3)
- 级别:table
- 里程碑:M2
- 前置:P0;L1 `Hedging`(`first_net 0.012`);腿 0 `net = 0.012`、腿 1 `net = −0.012`
- 实现:`crates/hft-oms/tests/external.rs::oms_250_external_adjust`

| 行 | 调用 | 期望 |
|---|---|---|
| OMS-250.1 | `(a1, VX, g1, Y, +0.004, eid 0, Liquidation)` | 腿 1 `net = −0.008`;`naked_qty` 重算为 0.004;L1 字段不变;无 `OrderDecision`、无 `HedgeRequest`;`ext_held` 不变 |
| OMS-250.2 | `(…, Adl)` | 同 .1 |
| OMS-250.3 | `(…, Manual)` | 同 .1 |
| OMS-250.4 | `(a1, VX, g1, Y, +0.004, eid 9001, Unknown)` | 同 .1;另 `ext_held[9001] = 0.004` |
| OMS-250.5 | `(a1, VX, g1, Y, −0.004, eid 9002, Unknown)` | 腿 1 `net = −0.016`;`ext_held[9002] = 0.004`(取绝对值) |
| OMS-250.6 | `(a1, VX, g1, Y, +0.004, eid 0, Unknown)` | 腿 1 更新;不记 `ext_held` |

### OMS-251 GROUP_NONE 的外来成交不改 LegTracker

- 规则:[外来成交](README.md#外来成交b3)、[LegTracker](README.md#legtracker)
- 级别:unit
- 里程碑:M2
- 前置:P0;同 OMS-250
- 步骤:
  1. `external_adjust(a1, VX, GROUP_NONE, Y, +0.004, 9003, Unknown)`
- 期望:
  - 所有 `LegState` 不变;`legs(a1, GROUP_NONE)` 各字段为 0;`ext_held[9003] = 0.004`
- 实现:`crates/hft-oms/tests/external.rs::oms_251_group_none_external`

### OMS-252 AdoptExchange 按采纳差额同量调整 LegTracker、不建槽

- 规则:[入口](README.md#入口全部在-sequencer-线程调用mut-self无分配)、[LegTracker](README.md#legtracker)
- 级别:table
- 里程碑:M2
- 前置:P0;g1 腿 0 `net = 0.012`、腿 1 `net = −0.012`;a1@VX 空闲表长度 64
- 实现:`crates/hft-oms/tests/legs.rs::oms_252_adopt_adjust`

| 行 | 调用 | 期望 |
|---|---|---|
| OMS-252.1 | `adopt_adjust(a1, g1, Y, +0.003)` | 腿 1 `net = −0.009`;`naked_qty = 0.003`;空闲表长度仍 64;无任何输出 |
| OMS-252.2 | `adopt_adjust(a1, g1, X, −0.012)` | 腿 0 `net = 0`;其余不变 |
| OMS-252.3 | `adopt_adjust(a1, GROUP_NONE, Y, +0.003)` | 所有 `LegState` 不变 |

### OMS-253 外来成交已被平掉时,匹配到槽后的第二腿被截为 0

- 规则:[外来成交](README.md#外来成交b3)、[回报匹配与成交增量](README.md#回报匹配与成交增量)
- 级别:unit
- 里程碑:M2
- 前置:P0;L1 第一腿 f(买 X 0.012)`Unknown`,`exch_order_id = 4444`;`ext_held[4444] = 0.012`,腿 0 `net = 0.012`;执行层已按外来成交默认处理卖出 Y 0.012(腿 1 `net = −0.012`)
- 步骤:
  1. `QueryResult{f, Filled, cum 0.012, avg_px 60000.00}`
- 期望:
  - f → `Filled`;L1 `first_net = 0.012`;无 `FillOut`(扣除后为 0);腿 0 `net = 0.012` 不变;`OmsExtDedup` 一条
  - `naked_dir = 0`:无 `HedgeRequest`;`OmsHedgeNakedCapped` 一条;`hedge_req = 0`
- 实现:`crates/hft-oms/tests/external.rs::oms_253_ext_flattened_no_double_hedge`

### OMS-260 买向冒烟单:价格离对手价至少 smoke_far_bp、数量为最小名义 × 1.1 向上取整

- 规则:[冒烟单](README.md#冒烟单smokeorder)
- 级别:unit
- 里程碑:M2
- 前置:P0;`smoke_far_bp = 0.03`;X 报价 bid 60000.00 / ask 60000.01,连接健康;`conn_liveness_ms = 2000`;a1@VS `smoke_slot = 0`
- 步骤:
  1. `smoke_intent(SmokeReq{account a1, venue VS, inst X, side Buy, ttl_ms 60000}, bbo, hot, 2000, now)`
- 期望:
  - `Ok(OrderIntent{venue VS, account a1, side Buy, tif PostOnly, inst X, group GROUP_NONE, px 58200.00, qty 0.0001, link.some 0, max_rest_ms 0})`(60000.01 × 0.97 = 58200.0097 向下取到 0.01;5 × 1.1 / 58200.00 = 0.0000945… 向上取到 lot 0.00001 = 0.0001)
  - 调用前后 OMS 状态逐字节相同(只读)
- 实现:`crates/hft-oms/tests/smoke.rs::oms_260_smoke_buy_intent`

### OMS-261 卖向冒烟单向上取到 tick

- 规则:[冒烟单](README.md#冒烟单smokeorder)
- 级别:unit
- 里程碑:M2
- 前置:同 OMS-260
- 步骤:
  1. `smoke_intent(side = Sell)`
- 期望:
  - `px = 61800.00`(60000.00 × 1.03);`qty = 0.00009`(5.5 / 61800.00 = 0.0000890… 向上取到 0.00001)
- 实现:`crates/hft-oms/tests/smoke.rs::oms_261_smoke_sell_intent`

### OMS-262 冒烟单数量取最小下单量与最小名义 × 1.1 中的较大者并向上取整

- 规则:[冒烟单](README.md#冒烟单smokeorder)
- 级别:table
- 里程碑:M2
- 前置:同 OMS-260(买,`px = 58200.00`);X 的 `min_qty`、`min_notional` 按行
- 实现:`crates/hft-oms/tests/smoke.rs::oms_262_smoke_qty`

| 行 | `min_qty` / `min_notional` | 期望 `qty` |
|---|---|---|
| OMS-262.1 | 0.00001 / 5 | 0.0001 |
| OMS-262.2 | 0.001 / 5 | 0.001(最小下单量更大) |
| OMS-262.3 | 0.00001 / 100 | 0.0019(110 / 58200.00 = 0.00189003… 向上) |
| OMS-262.4 | 0.00001 / 5.82 | 0.00011(6.402 / 58200.00 恰为 0.00011,不再进位) |

### OMS-263 冒烟单价格距离边界

- 规则:[冒烟单](README.md#冒烟单smokeorder)
- 级别:table
- 里程碑:M2
- 前置:同 OMS-260(买);`smoke_far_bp` 与 ask 按行
- 实现:`crates/hft-oms/tests/smoke.rs::oms_263_smoke_distance`

| 行 | `smoke_far_bp` / ask | 期望 `px` 与距离 |
|---|---|---|
| OMS-263.1 | 0.03 / 60000.00 | 58200.00;(ask − px) / ask = 0.03 |
| OMS-263.2 | 0.03 / 60000.01 | 58200.00;距离 0.0300001 ≥ 0.03 |
| OMS-263.3 | 0.0001 / 60000.00 | 59994.00 |

### OMS-264 冒烟单的拒绝条件

- 规则:[冒烟单](README.md#冒烟单smokeorder)
- 级别:table
- 里程碑:M2
- 前置:同 OMS-260;各行改一项
- 实现:`crates/hft-oms/tests/smoke.rs::oms_264_smoke_errors`

| 行 | 条件 | 期望 |
|---|---|---|
| OMS-264.1 | a1@VS `smoke_slot = 4`(已有非终态冒烟单) | `Err(SmokeErr::Busy)` |
| OMS-264.2 | `inst = Y`(属于 VX,不属于 VS) | `Err(SmokeErr::BadInst)` |
| OMS-264.3 | `conn_up = false` | `Err(SmokeErr::NoQuote)` |
| OMS-264.4 | `resync = true` | `Err(SmokeErr::NoQuote)` |
| OMS-264.5 | `ask_px = 0`(买向缺对手侧) | `Err(SmokeErr::NoQuote)` |
| OMS-264.6 | `bid_px = 0`(两侧都须有价) | `Err(SmokeErr::NoQuote)` |
| OMS-264.7 | `now − conn_live_tsc = 2000 ms` | `Ok` |
| OMS-264.8 | `now − conn_live_tsc = 2001 ms` | `Err(SmokeErr::NoQuote)` |

### OMS-265 冒烟单建槽:SMOKE 标志、无链接、不进 LegTracker、登记到期定时器

- 规则:[冒烟单](README.md#冒烟单smokeorder)
- 级别:unit
- 里程碑:M2
- 前置:P0;OMS-260 的意图作为 `ApprovedIntent{origin = Command, smoke_ttl_ms = 60000}`;`now = 1000 ms`
- 步骤:
  1. `place`
- 期望:
  - 槽 `oflags = SMOKE | POST_ONLY`(0x22)、`SlotExt.origin = 4`、`group = 0`、`strategy = 1`、`link_id = 0`;a1@VS `smoke_slot` = 该 `slot_id`
  - `OmsTimerReq{SmokeTtl, id = 1 \| 2 << 8 = 513, deadline 61 000 ms, cancel false}` 一条
  - 所有 `LegState` 不变;`open_buy(a1, X)` +0.0001、`buy_notional(a1, VS)` +5.82
- 实现:`crates/hft-oms/tests/smoke.rs::oms_265_smoke_place`

### OMS-266 冒烟单的回报不投递给任何策略

- 规则:[冒烟单](README.md#冒烟单smokeorder)
- 级别:unit
- 里程碑:M2
- 前置:P0;OMS-265 之后
- 步骤:
  1. 投递 `Ack`;再投递 `CancelAck`
- 期望:
  - 两步各一条 `OmsTransition`;`updates` 为空(无 `OrderUpdate`)
- 实现:`crates/hft-oms/tests/smoke.rs::oms_266_smoke_no_owner_update`

### OMS-267 冒烟单成交按未归属入账并报 P0

- 规则:[冒烟单](README.md#冒烟单smokeorder)、[Note 码表](README.md#note-码表)
- 级别:unit
- 里程碑:M2
- 前置:P0;冒烟单 `Live`
- 步骤:
  1. 投递 `PartialFill{cum 0.00005}`
- 期望:
  - `FillOut{group GROUP_NONE, qty 0.00005}` 一条;`Note{OmsAnomaly}`(`v[0] = 3`,`level = P0`)一条;所有 `LegState` 不变;无 `OrderUpdate`
- 实现:`crates/hft-oms/tests/smoke.rs::oms_267_smoke_filled`

### OMS-268 冒烟单到期撤单按槽状态处理

- 规则:[冒烟单](README.md#冒烟单smokeorder)、[输出扇出](README.md#输出扇出)
- 级别:table
- 里程碑:M2
- 前置:P0;a1@VS 冒烟单槽按行;调用 `on_smoke_ttl(a1, VS, keep = false)`
- 实现:`crates/hft-oms/tests/smoke.rs::oms_268_smoke_ttl_by_state`

| 行 | 槽状态 | 期望 |
|---|---|---|
| OMS-268.1 | `Live` | `OrderDecision{Cancel}` 一条;`OmsTransition` cause `SmokeTtl(25)`;→ `PendingCancel` |
| OMS-268.2 | `PendingNew` | 同 .1 |
| OMS-268.3 | `Unknown` | 置 `SMOKE_DUE`;无 `OrderDecision`;随后 `QueryResult{New}` 使其收敛为 `Live` 的同一调用内出 `OrderDecision{Cancel}` |
| OMS-268.4 | `Filled`(未回收) | 无输出 |
| OMS-268.5 | `smoke_slot = 0` | 无输出 |

### OMS-269 冒烟单终态时清 smoke_slot 并撤销到期定时器

- 规则:[冒烟单](README.md#冒烟单smokeorder)
- 级别:unit
- 里程碑:M2
- 前置:P0;冒烟单 `Live`(定时器 id 513)
- 步骤:
  1. 交易所 `CancelAck`(人工撤单)
  2. `smoke_intent` 再下一张
- 期望:
  - 步骤 1:`smoke_slot = 0`;`OmsTimerReq{SmokeTtl, id 513, cancel true}` 一条
  - 步骤 2:`Ok`
- 实现:`crates/hft-oms/tests/smoke.rs::oms_269_smoke_terminal_clears`

### OMS-270 不撤挂单的 Drain 期间到期:有交易所死手的 venue 不撤

- 规则:[冒烟单](README.md#冒烟单smokeorder)
- 级别:table
- 里程碑:M2
- 前置:P0;冒烟单 `Live`,所在 venue 按行
- 实现:`crates/hft-oms/tests/smoke.rs::oms_270_smoke_keep`

| 行 | venue(`dms`)/ `keep` | 期望 |
|---|---|---|
| OMS-270.1 | VX(`CountdownAccount`)/ true | 无 `OrderDecision`;不置 `SMOKE_DUE`;`smoke_slot` 不变 |
| OMS-270.2 | VS(`None`)/ true | `OrderDecision{Cancel}` 一条 |
| OMS-270.3 | VX / false | `OrderDecision{Cancel}` 一条 |

### OMS-271 sweep 补撤到期未撤的冒烟单

- 规则:[冒烟单](README.md#冒烟单smokeorder)、[StaleOrderSweep](README.md#staleordersweeptimeroms-sweep1-s)
- 级别:table
- 里程碑:M2
- 前置:P0;冒烟单槽置 `SMOKE_DUE`;调用 `on_sweep`
- 实现:`crates/hft-oms/tests/smoke.rs::oms_271_sweep_recancels_smoke`

| 行 | 槽 | 期望 |
|---|---|---|
| OMS-271.1 | `Live`,未置 `CANCEL_SENT` | `OrderDecision{Cancel}` 一条 |
| OMS-271.2 | `PendingCancel`(已置 `CANCEL_SENT`) | 无输出 |
| OMS-271.3 | `Unknown` | 无输出 |

### OMS-272 冒烟单不跨运行保留,重启后只撤一次

- 规则:[冒烟单](README.md#冒烟单smokeorder)、[恢复:停机时间不计与闸门续接](README.md#恢复停机时间不计与闸门续接)
- 级别:unit
- 里程碑:M2
- 前置:P0;快照时冒烟单 `Live`(定时器尚未到期);重启:`restore` → `recovery_end`
- 步骤:
  1. `recovery_end`;`ConnState{Order, Up}`;`QueryResult{New}`
  2. 旧 `SmokeTtl` 定时器早到:`on_smoke_ttl(a1, VS, false)`
  3. `CancelAck`;旧定时器晚到:再 `on_smoke_ttl(a1, VS, false)`
- 期望:
  - 步骤 1:`recovery_end` 置 `SMOKE_DUE`;收敛为 `Live` 的同一调用内 `OrderDecision{Cancel}` 一条
  - 步骤 2:槽已 `PendingCancel`、`CANCEL_SENT`,无新 `Cancel`
  - 步骤 3:`smoke_slot = 0` 后到期无输出;全程 `Cancel` 恰好一条
- 实现:`crates/hft-oms/tests/smoke.rs::oms_272_smoke_restart_cancel_once`

### OMS-273 Kill、Drain、CancelAll 与 cancel_resting_on 照常撤冒烟单

- 规则:[冒烟单](README.md#冒烟单smokeorder)、[输出扇出](README.md#输出扇出)
- 级别:table
- 里程碑:M2
- 前置:P0;冒烟单 `Live`(VS,无批量撤单之外的其他单)
- 实现:`crates/hft-oms/tests/smoke.rs::oms_273_bulk_cancels_include_smoke`

| 行 | 入口 | 期望 |
|---|---|---|
| OMS-273.1 | `cancel_all(Account(a1), open_only = true)`(非 HARD Kill) | 冒烟单被撤(非 `RISK_REDUCING`) |
| OMS-273.2 | `cancel_all(Account(a1), open_only = false)`(Drain 撤挂单 / CancelAll) | 被撤 |
| OMS-273.3 | `cancel_resting_on(a1, X)` | 被撤 |

### OMS-274 smoke_slot 与 SMOKE_DUE 随快照往返

- 规则:[冒烟单](README.md#冒烟单smokeorder)、[快照行格式](README.md#快照行格式节-2312草案实现时以代码与编译期断言为准)
- 级别:unit
- 里程碑:M2
- 前置:P0;冒烟单槽 `slot_id = 7`、`Unknown` 且置 `SMOKE_DUE`
- 步骤:
  1. `snapshot` 后 `restore`
- 期望:
  - `TableRow.smoke_slot = 7`;恢复后 `smoke_slot = 7`、该槽 `sflags` 含 `SMOKE_DUE`
- 实现:`crates/hft-oms/tests/smoke.rs::oms_274_smoke_snapshot`

### OMS-275 冒烟单被交易所拒绝时如实回报、不重试

- 规则:[冒烟单](README.md#冒烟单smokeorder)
- 级别:unit
- 里程碑:M2
- 前置:P0;冒烟单 `PendingNew`
- 步骤:
  1. `Reject{reject_code = 170193}`(价格保护带)
- 期望:
  - → `Rejected`;`smoke_slot = 0`;无新 `OrderDecision`、无 `OrderUpdate`
- 实现:`crates/hft-oms/tests/smoke.rs::oms_275_smoke_rejected`

### OMS-280 残余核销要求 group 静止

- 规则:[残余核销](README.md#残余核销)、[LegTracker](README.md#legtracker)
- 级别:table
- 里程碑:M2
- 前置:P0;g1 腿 0 `net = 0.0004`、腿 1 `net = −0.0001`;每行另加一项条件;调用 `write_off_residual(a1, g1)`(.5 行为 `g9`)
- 实现:`crates/hft-oms/tests/writeoff.rs::oms_280_not_quiescent`

| 行 | 附加条件 | 期望 |
|---|---|---|
| OMS-280.1 | 有 `Hedging` 链接(`open_links = 1`) | `Err(OpError::NotQuiescent)`;LegTracker 不变 |
| OMS-280.2 | 有 `Unknown` 槽 | `Err(NotQuiescent)` |
| OMS-280.3 | 只有一个 `PendingNew` GTC 槽 | `Err(NotQuiescent)`(`has_pending`) |
| OMS-280.4 | 一个 IOC `Live` 槽(`inflight_immediate ≠ 0`) | `Err(NotQuiescent)` |
| OMS-280.5 | 不存在的 group `g9` | `Err(NotQuiescent)` |
| OMS-280.6 | 只有一个已 Ack 的 GTC 挂单 | `Ok` |

### OMS-281 核销移出多出那一腿的已成交裸量并清零全部腿的余数

- 规则:[残余核销](README.md#残余核销)
- 级别:table
- 里程碑:M2
- 前置:P0;g1 腿 0 `net = 0.0004`、腿 1 `net = −0.0001`(`naked_sum = 0.0003`);`residual` 腿 0 = 0.00002、腿 1 = 0.0006;g1 静止;Broken 与否按行
- 实现:`crates/hft-oms/tests/writeoff.rs::oms_281_write_off`

| 行 | g1 | 期望 |
|---|---|---|
| OMS-281.1 | 有 `Broken` 链接 | `Ok(WriteOff{venue VS, inst X, leg_ix 0, qty +0.0003})`;腿 0 `net = 0.0001`、腿 1 `net = −0.0001`;两腿 `residual = 0`;槽与链接不变;无 Note、无订单 |
| OMS-281.2 | 非 Broken | 同 .1 |

### OMS-282 没有多出的一腿时只清余数并返回 0

- 规则:[残余核销](README.md#残余核销)
- 级别:unit
- 里程碑:M2
- 前置:P0;g1 腿 0 `net = 0.003`、腿 1 `net = −0.003`;腿 1 `residual = 0.0006`;静止
- 步骤:
  1. `write_off_residual(a1, g1)`
- 期望:
  - `Ok(WriteOff{qty 0})`;两腿 `net` 不变;两腿 `residual = 0`
- 实现:`crates/hft-oms/tests/writeoff.rs::oms_282_zero_naked_sum`

### OMS-283 核销后的新链接不把旧余数补成对冲

- 规则:[残余核销](README.md#残余核销)、[OnFill](README.md#onfill)
- 级别:unit
- 里程碑:M2
- 前置:P0;同 OMS-281 核销之后(腿 1 `residual = 0`;核销前为 0.0006)
- 步骤:
  1. g1 上新链接 L2 第一腿成交 0.0005(手续费 USDT)
- 期望:
  - `raw = 0.0005` < lot 0.001:无 `HedgeRequest`;`residual[g1][1] = 0.0005`(若未清零,`raw` 将为 0.0011 并发出 0.001)
- 实现:`crates/hft-oms/tests/writeoff.rs::oms_283_no_stale_residual_hedge`

### OMS-284 核销后 LegTracker 与 ledger 的 group 仓位相等

- 规则:[残余核销](README.md#残余核销)
- 级别:integ
- 里程碑:M9
- 前置:testkit 进程内 sequencer + hft-ledger;g1 状态同 OMS-281.1(ledger 中 g1 的 X 持仓 0.0004、Y −0.0001)
- 步骤:
  1. `ControlCmd{WriteOffResidual{a1, g1}}`
- 期望:
  - 同一 Input 内 ledger 收到 `AdjustDelta::WriteOff`(X 0.0003 从 g1 移到 unassigned);之后 ledger g1 的 X 持仓 = `legs(a1, g1).leg[0].net` = 0.0001,Y 持仓 = 腿 1 `net` = −0.0001
- 实现:`crates/hft-testkit/src/scenarios/oms.rs::oms_284_write_off_matches_ledger`

### OMS-285 SimReset 要求账户空闲

- 规则:[入口](README.md#入口全部在-sequencer-线程调用mut-self无分配)
- 级别:table
- 里程碑:M2
- 前置:P0;s1@VS 槽按行;调用 `sim_reset(s1)`
- 实现:`crates/hft-oms/tests/sim_reset.rs::oms_285_sim_reset_preconditions`

| 行 | 槽 | 期望 |
|---|---|---|
| OMS-285.1 | 一个 `Unknown` 槽 | `Err(OpError::NotIdle)`;状态不变 |
| OMS-285.2 | 一个 IOC `Live` 槽 | `Err(OpError::NotIdle)` |
| OMS-285.3 | 一个 FOK `PendingNew` 槽 | `Err(OpError::NotIdle)` |
| OMS-285.4 | 只有 GTC `Live` 槽 | `Ok` |

### OMS-286 SimReset 把挂单撤销终结进墓碑并清零账本侧状态

- 规则:[入口](README.md#入口全部在-sequencer-线程调用mut-self无分配)
- 级别:unit
- 里程碑:M2
- 前置:P0;s1@VS:旧墓碑 2 个;GTC `Live` 槽 q(买 X 0.01,`gen 3`);冒烟单槽 k(`Live`);g3(s1 的 group)腿 0 `net = 0.02`、`residual = 0.0001`;InflightBook 有 q、k 的在途
- 步骤:
  1. `sim_reset(s1)`
  2. sim-gw 回 q 的 `CancelAck{gen 3}`
- 期望:
  - 步骤 1:旧墓碑清零;q、k 以 `Cancelled`(cause `SimReset(24)`)终结、写进各自墓碑、回到空闲表;`smoke_slot = 0`;g3 的 `LegState` 全零;s1 的 InflightBook 各项为 0
  - 步骤 2:命中 q 的墓碑、无成交:输出为空
- 实现:`crates/hft-oms/tests/sim_reset.rs::oms_286_sim_reset`

### OMS-287 SimReset 之后旧单的成交仍按墓碑入账并报 P0

- 规则:[入口](README.md#入口全部在-sequencer-线程调用mut-self无分配)、[墓碑与迟到成交](README.md#墓碑与迟到成交c1)
- 级别:unit
- 里程碑:M2
- 前置:P0;OMS-286 步骤 1 之后
- 步骤:
  1. 投递 q 的 `PartialFill{gen 3, cum 0.003}`
- 期望:
  - `FillOut{flags ⊇ LATE, qty 0.003}`;`Note{OmsLateFill}`(P0)一条
- 实现:`crates/hft-oms/tests/sim_reset.rs::oms_287_fill_after_sim_reset`

### OMS-290 在途两类的划分表每一行

- 规则:[LegTracker](README.md#legtracker)、[InflightBook](README.md#inflightbook)
- 级别:table
- 里程碑:M2
- 前置:P0;g1 腿 0 上一个买 X 0.01 的槽,其余为空;读取 `legs(a1, g1).leg[0]` 与 `imm_buy`/`open_buy(a1, X)`
- 实现:`crates/hft-oms/tests/legs.rs::oms_290_inflight_classes`

| 行 | tif / 状态 / ACKED | 期望 `inflight_immediate` / `inflight_resting` / `imm_buy` / `open_buy` |
|---|---|---|
| OMS-290.1 | IOC / `PendingNew` / 否 | 0.01 / 0 / 0.01 / 0.01 |
| OMS-290.2 | IOC / `Live` / 是 | 0.01 / 0 / 0.01 / 0.01 |
| OMS-290.3 | FOK / `Live` / 是 | 0.01 / 0 / 0.01 / 0.01 |
| OMS-290.4 | GTC / `PendingNew` / 否 | 0.01 / 0 / 0.01 / 0.01 |
| OMS-290.5 | PostOnly / `PendingNew` / 否 | 0.01 / 0 / 0.01 / 0.01 |
| OMS-290.6 | GTC / `PendingCancel` / 否 | 0.01 / 0 / 0.01 / 0.01 |
| OMS-290.7 | GTC / `Live` / 是 | 0 / 0.01 / 0 / 0.01 |
| OMS-290.8 | GTC / `PendingCancel` / 是 | 0 / 0.01 / 0 / 0.01 |
| OMS-290.9 | PostOnly / `PendingAmend` / 是 | 0 / 0.01 / 0 / 0.01 |
| OMS-290.10 | GTC / `Unknown` / 是 | 0.01 / 0 / 0.01 / 0.01 |
| OMS-290.11 | GTC / `Live` / 是,`filled = 0.004` | 0 / 0.006 / 0 / 0.006 |

### OMS-291 迁移使剩余量在两类之间移动

- 规则:[LegTracker](README.md#legtracker)、[InflightBook](README.md#inflightbook)
- 级别:unit
- 里程碑:M2
- 前置:P0;GTC 买 X 0.01(g1 腿 0)
- 步骤:
  1. `place` → `Ack` → `cancel` → `ConnState{Order, Down}` → `ConnState{Order, Up}` 与 `QueryResult{New}` → `CancelAck`,每步后读腿 0 与 `imm_buy`/`open_buy`
- 期望:
  - (即时, 挂单, imm_buy, open_buy)依次为 (0.01, 0, 0.01, 0.01) → (0, 0.01, 0, 0.01) → (0, 0.01, 0, 0.01)(已 Ack 的 `PendingCancel`)→ (0.01, 0, 0.01, 0.01)(进 `Unknown`)→ (0, 0.01, 0, 0.01)(收敛为 `Live`)→ (0, 0, 0, 0)
- 实现:`crates/hft-oms/tests/legs.rs::oms_291_class_moves`

### OMS-292 对冲腿上的挂单不算对冲覆盖

- 规则:[LegTracker](README.md#legtracker)、[OnFill](README.md#onfill)
- 级别:unit
- 里程碑:M2
- 前置:P0;L1;腿 1(Y)上有策略挂的卖 1.0 GTC(已 Ack);第一腿 IOC 买 X 1.0;手续费 USDT
- 步骤:
  1. 第一腿 `Fill{cum 1.0}`
- 期望:
  - `naked_dir = 1.0`;`HedgeRequest{qty 1.0}`;`legs(a1, g1).naked_qty = 1.0`
- 实现:`crates/hft-oms/tests/legs.rs::oms_292_resting_not_cover`

### OMS-293 该方向裸量 naked_dir 的计算

- 规则:[LegTracker](README.md#legtracker)
- 级别:table
- 里程碑:M2
- 前置:P0;第一腿 f、对冲腿 h;按行设定 `net` 与在途(即时在途带符号,买为正);ratio 1/1,除 .9 外
- 实现:`crates/hft-oms/tests/legs.rs::oms_293_naked_dir`

| 行 | 对冲方向 / `net_f`、`net_h` / 在途 | 期望 `naked_dir` |
|---|---|---|
| OMS-293.1 | 卖 / 0.012、0 / 无 | 0.012 |
| OMS-293.2 | 卖 / 0.012、0 / h 即时 −0.004(在途对冲) | 0.008 |
| OMS-293.3 | 卖 / 0.012、0 / f 即时 +0.005(第一腿同向未成交) | 0.012 |
| OMS-293.4 | 卖 / 0.012、0 / f 即时 −0.003(第一腿上反向在途) | 0.009 |
| OMS-293.5 | 卖 / 0.012、0 / h 挂单 −0.012 | 0.012 |
| OMS-293.6 | 卖 / 0.012、−0.02 / 无 | 0(E 为负) |
| OMS-293.7 | 买 / −0.012、0 / 无 | 0.012 |
| OMS-293.8 | 买 / −0.012、0 / h 即时 +0.004 | 0.008 |
| OMS-293.9 | 卖,ratio 2/1 / 0.005、−0.004 / 无 | 0.006(E = 0.005 × 2 − 0.004) |

### OMS-294 naked_qty、naked_ratio 与 naked_notional

- 规则:[LegTracker](README.md#legtracker)
- 级别:unit
- 里程碑:M2
- 前置:P0;g1 腿 0 `net = 0.012`、腿 1 `net = −0.009`、腿 1 即时在途 −0.0006;g1 最近成交价 60000.00
- 步骤:
  1. 读取 `legs(a1, g1)`
- 期望:
  - `naked_qty = |0.012 − 0.009 − 0.0006| = 0.0024`;`naked_ratio = 0.0024 / 0.012 = 0.2`(`Rate(200 000 000)`);`naked_notional = 144.00 USDT`
- 实现:`crates/hft-oms/tests/legs.rs::oms_294_naked_metrics`

### OMS-295 订单侧抑制条件的计数

- 规则:[LegTracker](README.md#legtracker)
- 级别:table
- 里程碑:M2
- 前置:P0;g1 初始无槽无链接;按行加入对象后读取 `legs(a1, g1)`
- 实现:`crates/hft-oms/tests/legs.rs::oms_295_suppress_counters`

| 行 | 加入 | 期望 |
|---|---|---|
| OMS-295.1 | 一条 `Open` 链接 | `open_links = 1` |
| OMS-295.2 | 一条 `Hedging` 与一条 `Done` 链接 | `open_links = 1` |
| OMS-295.3 | 一个 `Unknown` 槽 | `has_unknown = true` |
| OMS-295.4 | 一个 `PendingCancel` 槽 | `has_pending = true` |
| OMS-295.5 | 一个已 Ack 的 GTC `Live` 槽 | `has_pending = false`、`has_unknown = false`、即时在途 0 |

### OMS-300 InflightBook 的两类读数随状态变化

- 规则:[InflightBook](README.md#inflightbook)
- 级别:unit
- 里程碑:M2
- 前置:P0;同 OMS-291
- 步骤:
  1. 同 OMS-291,并读取 `open_sell`、`imm_sell` 与名义项
- 期望:
  - 全过程 `open_sell = imm_sell = 0`;`CancelAck` 之前每一步 `buy_notional(a1, VS) = 600.00`、Open 类在途名义 `(a1, g1)` 与 `(a1, VS)` 均为 600.00;`CancelAck` 之后三者均为 0
- 实现:`crates/hft-oms/tests/inflight.rs::oms_300_inflight_lifecycle`

### OMS-301 buy_notional 只计现货买单的剩余量 × 委托价

- 规则:[InflightBook](README.md#inflightbook)
- 级别:table
- 里程碑:M2
- 前置:P0;a1@VS 与 a1@VX 上按行下单
- 实现:`crates/hft-oms/tests/inflight.rs::oms_301_buy_notional`

| 行 | 订单 | 期望 `buy_notional` |
|---|---|---|
| OMS-301.1 | VS 买 X 0.01 @ 60000.00 | (a1, VS) = 600.00 |
| OMS-301.2 | 同上,部分成交 0.004 | 360.00 |
| OMS-301.3 | VS 卖 X 0.01 | (a1, VS) = 0 |
| OMS-301.4 | VX 买 Y 0.01(永续) | (a1, VX) = 0 |

### OMS-302 open_margin 按下单时的杠杆计入,之后不重算

- 规则:[InflightBook](README.md#inflightbook)
- 级别:table
- 里程碑:M2
- 前置:P0;a1@VX 永续 Open 买 Y 0.01 @ 60000.0;`ApprovedIntent.lev_x100` 按行
- 实现:`crates/hft-oms/tests/inflight.rs::oms_302_open_margin`

| 行 | `lev_x100` / 之后 | 期望 `open_margin(a1, VX)` |
|---|---|---|
| OMS-302.1 | 2000(20 倍) | 30.00(600.00 × 100 / 2000) |
| OMS-302.2 | 0 | 600.00(按 1 倍) |
| OMS-302.3 | 2000;风控杠杆随后改为 10 倍 | 仍 30.00 |
| OMS-302.4 | 2000;部分成交 0.004 | 18.00 |
| OMS-302.5 | Reduce 类(`class = Reduce`,`lev_x100 = 0`) | 0 |

### OMS-303 在途名义只计 Open 类

- 规则:[InflightBook](README.md#inflightbook)
- 级别:table
- 里程碑:M2
- 前置:P0;g1 上按行下单(a1@VS)
- 实现:`crates/hft-oms/tests/inflight.rs::oms_303_open_notional`

| 行 | 订单 | 期望 (a1, g1) / (a1, VS) 在途名义 |
|---|---|---|
| OMS-303.1 | Open 买 X 0.01 @ 60000.00 | 600.00 / 600.00 |
| OMS-303.2 | Reduce 卖 X 0.01 | 0 / 0 |
| OMS-303.3 | Hedge(OMS 第二腿)卖 X 0.01 | 0 / 0 |

### OMS-305 Drain 静止原因位

- 规则:[Drain 静止判定](README.md#drain-静止判定支持-c11)
- 级别:table
- 里程碑:M2
- 前置:P0;a1 按行设定;读取 `quiescent(a1)`
- 实现:`crates/hft-oms/tests/quiesce.rs::oms_305_quiescent_bits`

| 行 | 状态 | 期望 |
|---|---|---|
| OMS-305.1 | 只有已 Ack 的 GTC 挂单 | 空位集 |
| OMS-305.2 | 一条 `Hedging` 链接 | `LINKS_ACTIVE` |
| OMS-305.3 | 一个已 Ack 的 IOC `Live` 槽 | `IOC_NONTERMINAL` |
| OMS-305.4 | 一个 FOK `PendingNew` 槽 | `IOC_NONTERMINAL` |
| OMS-305.5 | 已登记未到期的 HedgeRetry,链接 `Hedging` | `LINKS_ACTIVE \| HEDGE_RETRY_PENDING` |
| OMS-305.6 | 一个 `Unknown` 槽 | `UNKNOWN` |
| OMS-305.7 | a2 有 `Unknown` 槽 | `quiescent(a1)` 为空位集 |

### OMS-310 restore 只装载,不改任何 TSC 量

- 规则:[恢复:停机时间不计与闸门续接](README.md#恢复停机时间不计与闸门续接)、[入口](README.md#入口全部在-sequencer-线程调用mut-self无分配)
- 级别:unit
- 里程碑:M2
- 前置:P0;运行 1 中槽 p `PendingNew`(`t_state = 100 ms`)、L1 `first_fill_tsc = 50 ms`、墓碑 `freed_tsc = 80 ms`、g1 `last_fill_tsc = 50 ms`;写快照;`ack_timeout_ms = 2000`
- 步骤:
  1. `restore`
  2. 按运行 1 的时间轴重放尾部:`on_sweep(now = 2101 ms)`
- 期望:
  - 步骤 1 后四个 TSC 字段与快照前逐值相同;`OmsCountersHdr.resume_hold_until` 不变
  - 步骤 2:p → `Unknown`(cause `Timeout`),与运行 1 journal 中该事件的输出逐字节相同
- 实现:`crates/hft-oms/tests/recover.rs::oms_310_restore_loads_only`

### OMS-311 recovery_end 把非终态槽置 Unknown、清连接与闸门状态、重给续接窗口

- 规则:[恢复:停机时间不计与闸门续接](README.md#恢复停机时间不计与闸门续接)
- 级别:unit
- 里程碑:M2
- 前置:P0;`resume_hold_max_ms = 30000`;恢复后 a1@VX:`Live` 槽 2 个、`PendingNew` 1 个、`Filled` 1 个;L1 `Hedging`;冒烟单槽 `Live`;`hz_prev = hz_cur = 1 GHz`;`now = 10 000 ms`、`stop_tsc = 500 000 ms`
- 步骤:
  1. `recovery_end(now, stop_tsc, hz_prev, hz_cur, cause_seq, out)`
- 期望:
  - 4 个非终态槽(含冒烟单)→ `Unknown`(cause `RecoveryEnd(20)`),冒烟单另置 `SMOKE_DUE`;`Filled` 槽不变;`UnknownQuery` 定时器 4 条,`deadline = 11 000 ms`;无 `OrderDecision`
  - 各表 `conn_up = false`、`hedge_gate = false`、`decode_fails = 0`;`resume_hold_until = 40 000 ms`(4 × 10¹⁰ tick)
  - L1 置 `RESUME_HOLD`;输出中没有 `LinkUpdate`
- 实现:`crates/hft-oms/tests/recover.rs::oms_311_recovery_end_state`

### OMS-312 已登记 TSC 字段按停机不计平移

- 规则:[恢复:停机时间不计与闸门续接](README.md#恢复停机时间不计与闸门续接)
- 级别:table
- 里程碑:M2
- 前置:P0;`stop_tsc` 与 `now` 按行(ms,按各自频率换成 tick);`conv(Δ) = Δ × hz_cur / hz_prev`
- 实现:`crates/hft-oms/tests/recover.rs::oms_312_tsc_shift`

| 行 | 字段 = 原值 / 环境 | 期望平移后 |
|---|---|---|
| OMS-312.1 | `OrderSlot.t_state = 499 000 ms` / 跨开机,1 GHz → 1 GHz,`stop 500 000`、`now 10 000` | 9 000 ms |
| OMS-312.2 | `LinkEntry.first_fill_tsc = 499 000 ms` / 同 .1 | 9 000 ms |
| OMS-312.3 | `Tomb.freed_tsc = 499 000 ms` / 同 .1 | 9 000 ms |
| OMS-312.4 | `LegState.last_fill_tsc = 499 000 ms` / 同 .1 | 9 000 ms |
| OMS-312.5 | `t_state` 原值 4.99 × 10¹¹ tick / 1 GHz → 2 GHz,`stop` 5 × 10¹¹ tick,`now` 2 × 10¹⁰ tick | 1.8 × 10¹⁰ tick(Δ 10⁹ 换成 2 × 10⁹) |
| OMS-312.6 | `t_state = 499 000 ms` / `now = 500 ms` | 1 tick(饱和,不为 0) |
| OMS-312.7 | `first_fill_tsc = 0` | 0 |
| OMS-312.8 | `t_state = 499 000 ms` / 同开机,`now = 600 000 ms` | 599 000 ms |
| OMS-312.9 | 值 500 100 ms(晚于 `stop_tsc`)/ 同 .1 | 10 100 ms |

### OMS-313 held_ms 用平移前的时刻按上次运行频率计算并截到 hedge_deadline_ms

- 规则:[恢复:停机时间不计与闸门续接](README.md#恢复停机时间不计与闸门续接)
- 级别:table
- 里程碑:M2
- 前置:P0;`hedge_deadline_ms = 3000`;L1 `Hedging`;`stop_tsc = 500 000 ms`(按 `hz_prev`);调用 `recovery_end`
- 实现:`crates/hft-oms/tests/recover.rs::oms_313_held_ms`

| 行 | L1 恢复前 | 期望 `held_ms` |
|---|---|---|
| OMS-313.1 | `first_fill_tsc = 498 800 ms`,1 GHz → 1 GHz | 1200 |
| OMS-313.2 | `first_fill_tsc = 495 000 ms` | 3000(截断) |
| OMS-313.3 | `first_fill_tsc = 0` | 0 |
| OMS-313.4 | `hz_prev = 2 GHz`、`hz_cur = 1 GHz`,`stop_tsc − first_fill_tsc = 2.4 × 10⁹ tick` | 1200 |
| OMS-313.5 | 已带 `RESUME_HOLD`、`held_ms = 700`,`first_fill_tsc = 400 000 ms` | 700(不重复累计) |

### OMS-314 RESUME_HOLD 期间不判截止、忽略重试定时器、OnFill 照常

- 规则:[恢复:停机时间不计与闸门续接](README.md#恢复停机时间不计与闸门续接)
- 级别:table
- 里程碑:M2
- 前置:P0;OMS-311 之后,L1 `RESUME_HOLD`、`held_ms = 1200`;`hedge_deadline_ms = 3000`
- 实现:`crates/hft-oms/tests/recover.rs::oms_314_resume_hold_behaviour`

| 行 | 事件(`now`) | 期望 |
|---|---|---|
| OMS-314.1 | 旧 `on_hedge_retry(L1)`(15 000) | 无输出 |
| OMS-314.2 | `on_sweep`(20 000;按平移后的 `first_fill_tsc` 已过 11 000 ms) | L1 不 Broken |
| OMS-314.3 | 第一腿迟到 `PartialFill{+0.003}`(20 000) | 同一调用内 `HedgeRequest{Second, qty 0.003}` |
| OMS-314.4 | .3 的第二腿被交易所拒(20 100) | 登记 HedgeRetry;L1 不 Broken;`retries` +1 |

### OMS-315 对冲闸门子集满足时续接一次 SecondRetry

- 规则:[恢复:停机时间不计与闸门续接](README.md#恢复停机时间不计与闸门续接)、[入口](README.md#入口全部在-sequencer-线程调用mut-self无分配)
- 级别:unit
- 里程碑:M2
- 前置:P0;L1 `RESUME_HOLD`、`held_ms = 1200`、`owed = 0.012`、无 `Unknown` 槽、无在途;腿 1 `net = 0`;`now = 30 000 ms`
- 步骤:
  1. `on_hedge_gate_open(a1, VX, now)`
- 期望:
  - L1 清 `RESUME_HOLD`、置 `RESUME_RETRY`;`first_fill_tsc = 28 800 ms`;a1@VX `hedge_gate = true`
  - `HedgeRequest{SecondRetry, qty 0.012, basis 0.012}` 一条;`Note{OmsResumeRetry}`(P1,`v = [L1, 1200, 1 200 000, 1 200 000]`)一条;`LinkUpdate{flags 0}` 一条
- 实现:`crates/hft-oms/tests/recover.rs::oms_315_resume_second_retry`

### OMS-316 续接只由续接单所在 venue 的闸门触发

- 规则:[恢复:停机时间不计与闸门续接](README.md#恢复停机时间不计与闸门续接)
- 级别:table
- 里程碑:M2
- 前置:P0;L1(第一腿 VS、第二腿 VX)`RESUME_HOLD`、`owed = 0.012`;`on_fail` 按行
- 实现:`crates/hft-oms/tests/recover.rs::oms_316_resume_venue`

| 行 | `on_fail` / 调用 | 期望 |
|---|---|---|
| OMS-316.1 | `Retry` / `on_hedge_gate_open(a1, VX)` | `SecondRetry` 一条 |
| OMS-316.2 | `Retry` / `on_hedge_gate_open(a1, VS)` | 无输出;L1 仍 `RESUME_HOLD` |
| OMS-316.3 | `Reverse` / `(a1, VS)` | `HedgeRequest{Reverse, venue VS, qty 0.012}` 一条;置 `RESUME_RETRY` |
| OMS-316.4 | `Reverse` / `(a1, VX)` | 无输出 |
| OMS-316.5 | `Hold` / `(a1, VX)` | 清 `RESUME_HOLD`;无请求;不置 `RESUME_RETRY`;`OmsResumeRetry`(`v[3] = 0`)与 `LinkUpdate` 各一条 |
| OMS-316.6 | `Retry` / `(a1, VX)` 连续两次 | 第二次无输出 |

### OMS-317 链接有 Unknown 槽时续接推迟到收敛的那个事件

- 补充验收:`tests/recover.rs::check_simultaneous_recovery_waits_for_both_terminal` 覆盖 Simultaneous 快照恢复、两腿 QueryResult(Live) 不提前反向、第二腿多成时在该腿反向、Reverse 再次恢复不重复出单,以及 Hold 留给 LegTracker。

- 规则:[恢复:停机时间不计与闸门续接](README.md#恢复停机时间不计与闸门续接)
- 级别:unit
- 里程碑:M2
- 前置:P0;L1 `RESUME_HOLD`,第二腿 h `Unknown`(`hedge_open = 0.012`)
- 步骤:
  1. `on_hedge_gate_open(a1, VX)`(t = 30 000)
  2. t = 31 000 h 收到 `QueryResult{Cancelled, cum 0}`
- 期望:
  - 步骤 1:无请求;L1 仍 `RESUME_HOLD`;`hedge_gate = true`
  - 步骤 2:同一调用内 h → `Cancelled`、`owed = 0.012`,随即 `HedgeRequest{SecondRetry, qty 0.012}`、`OmsResumeRetry` 一条;`first_fill_tsc = 31 000 − held_ms`
- 实现:`crates/hft-oms/tests/recover.rs::oms_317_resume_waits_unknown`

### OMS-318 续接量被裸量上限截为 0 时不置 RESUME_RETRY

- 规则:[恢复:停机时间不计与闸门续接](README.md#恢复停机时间不计与闸门续接)
- 级别:table
- 里程碑:M2
- 前置:P0;L1 `RESUME_HOLD`、`owed = 0.012`;腿 1 `net = −0.012`(已由别的途径平掉)
- 实现:`crates/hft-oms/tests/recover.rs::oms_318_resume_capped_zero`

| 行 | 第一腿 | 期望 `on_hedge_gate_open(a1, VX)` 之后 |
|---|---|---|
| OMS-318.1 | 已 `Filled`,无在途 | 无请求;`hedge_req` 核销为 0;L1 `Done`;不置 `RESUME_RETRY` |
| OMS-318.2 | 仍 `Live`(GTC) | 无请求;`hedge_req = 0`;L1 `Hedging`、无 `RESUME_*` 位 |

### OMS-319 续接单终态后清 RESUME_RETRY 并按已用运行时间判截止

- 规则:[恢复:停机时间不计与闸门续接](README.md#恢复停机时间不计与闸门续接)
- 级别:table
- 里程碑:M2
- 前置:P0;OMS-315 之后(`first_fill_tsc = 28 800 ms`、`RESUME_RETRY`);`hedge_deadline_ms = 3000`
- 实现:`crates/hft-oms/tests/recover.rs::oms_319_resume_retry_terminal`

| 行 | 续接单结果 | 期望 |
|---|---|---|
| OMS-319.1 | t = 30 200 全部成交 | 清 `RESUME_RETRY`;L1 `Done` |
| OMS-319.2 | t = 30 500 被交易所拒 | 清 `RESUME_RETRY`;`now − first_fill_tsc = 1700 ≤ 3000`:登记重试,`retries` +1 |
| OMS-319.3 | t = 31 801 被交易所拒 | `3001 > 3000`:`Broken{Deadline}` |

### OMS-320 续接超时:有敞口的链接 Broken(P0),零敞口的只清标志(P1)

- 规则:[恢复:停机时间不计与闸门续接](README.md#恢复停机时间不计与闸门续接)、[配置项](README.md#配置项omsconfig)
- 级别:table
- 里程碑:M2
- 前置:P0;`resume_hold_until = 40 000 ms`;L1(a1,第二腿 VX)`RESUME_HOLD`、`held_ms 1200`、`owed 0.012`、`first_net 0.012`,VX 闸门未放行;L4(a1,第二腿 VX)`RESUME_HOLD`、`first_net 0`、第一腿 `Unknown`,VX 闸门未放行;在 `now` 调 `on_sweep`
- 实现:`crates/hft-oms/tests/recover.rs::oms_320_resume_timeout`

| 行 | `now` | 期望 |
|---|---|---|
| OMS-320.1 | 40 000 ms − 1 tick | 无输出 |
| OMS-320.2 | 40 000 ms | L1 `Broken{ResumeTimeout(6)}`;`Note{OmsResumeTimeout}` `level = P0`、`v = [L1, 1281, 1200, 1 200 000, 1, 1 200 000]`;`LinkUpdate{Broken, 6}`。L4 不 Broken:清 `RESUME_HOLD`;`OmsResumeTimeout` `level = P1`、`v[4] = 1`;`LinkUpdate{flags 0}` |
| OMS-320.3 | 40 000 ms,但 VX 闸门已放行、L1 的第二腿 `Unknown` | L1 `Broken{ResumeTimeout}`,`v[4] = 2`(UNKNOWN) |

### OMS-321 恢复期间再次崩溃:held_ms 不重复累计、窗口重新给满

- 规则:[恢复:停机时间不计与闸门续接](README.md#恢复停机时间不计与闸门续接)
- 级别:unit
- 里程碑:M2
- 前置:P0;OMS-311 之后(L1 `RESUME_HOLD`、`held_ms 1200`)写 Recovery 快照;再次崩溃,`stop_tsc` = 15 000 ms(运行 2),新运行 `now = 5 000 ms`
- 步骤:
  1. `restore` → `recovery_end(now 5 000, stop 15 000, …)`
- 期望:
  - L1 `held_ms = 1200`;`resume_hold_until = 35 000 ms`
- 实现:`crates/hft-oms/tests/recover.rs::oms_321_crash_during_recovery`

### OMS-322 跨开机恢复后墓碑按剩余保留时长过期

- 规则:[恢复:停机时间不计与闸门续接](README.md#恢复停机时间不计与闸门续接)、[墓碑与迟到成交](README.md#墓碑与迟到成交c1)
- 级别:table
- 里程碑:M2
- 前置:P0;`tombstone_keep_ms = 3600000`;墓碑 `freed_tsc = 499 000 ms`;跨开机 `stop 500 000`、`now 10 000`(平移后 9 000);随后在 `now` 调 `on_sweep`
- 实现:`crates/hft-oms/tests/recover.rs::oms_322_tomb_expiry_after_reboot`

| 行 | `now`(ms) | 期望 |
|---|---|---|
| OMS-322.1 | 3 609 000 | 墓碑保留 |
| OMS-322.2 | 3 609 001 | 墓碑清零 |

### OMS-323 零敞口链接续接超时之后:第一腿确认无成交则 Done,有成交则照常发第二腿

- 规则:[恢复:停机时间不计与闸门续接](README.md#恢复停机时间不计与闸门续接)
- 级别:table
- 里程碑:M2
- 前置:P0;OMS-320.2 之后的 L4(第一腿 f `Unknown` 带 `HIST_WAIT`,`first_fill_tsc = 0`);`hedge_deadline_ms = 3000`
- 实现:`crates/hft-oms/tests/recover.rs::oms_323_zero_exposure_followup`

| 行 | 之后的事件 | 期望 |
|---|---|---|
| OMS-323.1 | t = 45 000 `UnknownResolved{f, NotFound, history_checked 1}` | f → `Rejected`;L4 `Done` |
| OMS-323.2 | t = 45 000 `UnknownResolved{f, Filled, history_checked 1, exch_qty 0.012}` | 同一调用内 `HedgeRequest{Second, qty 0.012}`;`first_fill_tsc = 45 000 ms` |
| OMS-323.3 | .2 之后第二腿 t = 48 001 被拒 | `Broken{Deadline}` |

### OMS-330 4096 个槽同时进 Unknown:状态立即完成,输出按 FIFO 分批交出

- 规则:[输出扇出](README.md#输出扇出)
- 级别:unit
- 里程碑:M2
- 前置:P0;`slots[(VX, a1)] = 4096`,全部 `PendingNew`;`max_out_per_event = 16`;参考实现 = 容量无限的同一逻辑
- 步骤:
  1. `ConnState{VX, a1, Order, Down}`(`hdr.seq = 5000`)
  2. 只要 `wants_continue()` 为真就调 `on_continue`
- 期望:
  - 步骤 1 返回时:4096 个槽均为 `Unknown`、`unknown_count = 4096`、`venue_ready = false`、即时在途与 InflightBook 已按 `Unknown` 计;`wants_continue() = true`
  - 步骤 1 与全部 `on_continue` 的输出按顺序拼接后,与参考实现逐条相等(4096 条 `OmsTransition`、4096 条 `OmsUnknown`、4096 条 `OrderUpdate{Unknown}`);每条 Note 的 `cause_seq` 为 5000;任何一次调用的各输出向量都不超过其容量
- 实现:`crates/hft-oms/tests/out.rs::oms_330_mass_unknown_fanout`

### OMS-331 一次撤 4096 个槽:每轮至多 K 条出环决策

- 规则:[输出扇出](README.md#输出扇出)
- 级别:table
- 里程碑:M2
- 前置:P0;a1 在某 venue 上 4096 个 `Live` 非 `RISK_REDUCING` 槽;`max_out_per_event = 16`;`out_quota = 64`;调用 `cancel_all(Account(a1), open_only = false)`,随后反复 `on_continue`
- 实现:`crates/hft-oms/tests/out.rs::oms_331_mass_cancel`

| 行 | venue | 期望 |
|---|---|---|
| OMS-331.1 | VG(无 `BATCH_CANCEL`) | 首次调用后 4096 个槽都 `PendingCancel`;交出 16 条 `OrderDecision{Cancel}`,其余 4080 个槽带 `QUEUED`;此后每次 `on_continue` 交出 16 条,共 256 轮;交出顺序按槽号,交出时清 `QUEUED` |
| OMS-331.2 | VX(有 `BATCH_CANCEL`) | 恰好一条 `OrderDecision{CancelAll, venue VX, account a1}`;4096 个槽置 `CANCEL_SENT` |

### OMS-332 64 条链接续接与 64 条续接超时分批交出

- 规则:[输出扇出](README.md#输出扇出)、[恢复:停机时间不计与闸门续接](README.md#恢复停机时间不计与闸门续接)
- 级别:table
- 里程碑:M2
- 前置:P0;a1 有 64 条 `RESUME_HOLD` 链接(第二腿都在 VX);`OmsOut.hedges` 容量 8、`links` 容量 8
- 实现:`crates/hft-oms/tests/out.rs::oms_332_mass_resume`

| 行 | 调用 | 期望 |
|---|---|---|
| OMS-332.1 | `on_hedge_gate_open(a1, VX)` | 返回时 64 条链接都已清 `RESUME_HOLD`;本次交出 8 条 `HedgeRequest`,其余以 `OutItem{kind 3}` 排队;全部交出后共 64 条 `HedgeRequest`、64 条 `LinkUpdate{flags 0}`,每条链接恰好一条 |
| OMS-332.2 | 同 .1,但第 9 条链接的腿 1 在第二次 `on_continue` 之前被外来成交平掉 | 该链接的 `HedgeRequest` 在交出时按当时的 `naked_dir` 计算:数量 0 → 不交出请求,`hedge_req` 核销 |
| OMS-332.3 | 64 条都有敞口,`on_sweep` 时 `now ≥ resume_hold_until` | 返回时 64 条均 `Broken{ResumeTimeout}`;`LinkUpdate{Broken}` 共 64 条分批交出(`OutItem{kind 4}`),内容按交出时链接状态填写 |

### OMS-333 pending_out 跨快照后续发顺序不变

- 规则:[输出扇出](README.md#输出扇出)、[快照行格式](README.md#快照行格式节-2312草案实现时以代码与编译期断言为准)
- 级别:unit
- 里程碑:M2
- 前置:P0;同 OMS-331.1 首次调用之后(4080 项待发)
- 步骤:
  1. `snapshot` → `restore`;两侧各自反复 `on_continue` 直到 `wants_continue()` 为假
- 期望:
  - 原实例与恢复实例交出的决策序列逐条相等;节 12 的 `Out` 行中 `Decision` 项 4080 行(`Notify` 等项同在 `pending_out`,另计),按 FIFO 顺序
- 实现:`crates/hft-oms/tests/out.rs::oms_333_pending_out_snapshot`

### OMS-334 出环配额:账户之间轮转、同一队列内 RISK_REDUCING 先出

- 规则:[输出扇出](README.md#输出扇出)、[配置项](README.md#配置项omsconfig)
- 级别:table
- 里程碑:M2
- 前置:P0;`max_out_per_event = 16`;队列 (VS, a1) 依次产生 100 条 Open 决策、随后 2 条 `RISK_REDUCING`(第二腿);队列 (VS, a2) 3 条 Open;一次交出
- 实现:`crates/hft-oms/tests/out.rs::oms_334_quota_round_robin`

| 行 | `out_quota` | 期望本次交出 |
|---|---|---|
| OMS-334.1 | 64 | a1 13 条(前 2 条是 `RISK_REDUCING`)、a2 3 条;顺序 a1、a2 交替直到 a2 取空 |
| OMS-334.2 | 8 | a1 8 条、a2 3 条,共 11 条 |
| OMS-334.3 | 1 | a1 1 条(`RISK_REDUCING`)、a2 1 条 |

### OMS-335 交出前已终态的槽的排队撤单被丢弃

- 规则:[输出扇出](README.md#输出扇出)
- 级别:unit
- 里程碑:M2
- 前置:P0;OMS-331.1 首次调用之后,槽 100(排队中)收到 `Fill` 成 `Filled`
- 步骤:
  1. 反复 `on_continue`
- 期望:
  - 交出的 `OrderDecision{Cancel}` 中没有槽 100;其余 4079 条照常交出
- 实现:`crates/hft-oms/tests/out.rs::oms_335_drop_stale_queued_cancel`

### OMS-340 非 HARD Kill 只撤非 RISK_REDUCING 的可撤槽

- 规则:[输出扇出](README.md#输出扇出)、[RISK_REDUCING 判定](README.md#risk_reducing-判定)
- 级别:table
- 里程碑:M2
- 前置:P0;a1@VX(有 `BATCH_CANCEL`):s1 `Live` Open、s2 `PendingNew` Open、s3 `PendingAmend` Open、s4 `PendingCancel`、s5 `Unknown`、s6 `Live` `RISK_REDUCING`(挂着的减仓单);调用 `cancel_all(Account(a1), open_only = true)`
- 实现:`crates/hft-oms/tests/out.rs::oms_340_non_hard_kill_scope`

| 行 | 场景 | 期望 |
|---|---|---|
| OMS-340.1 | 如前置(有非终态 `RISK_REDUCING` 槽 s6) | 逐槽撤单:s1、s2、s3 各一条 `OrderDecision{Cancel}`;s4、s5、s6 无撤单;无 `CancelAll` |
| OMS-340.2 | 去掉 s6 | 一条 `OrderDecision{CancelAll}`;s1、s2、s3 → `PendingCancel` 并置 `CANCEL_SENT` |
| OMS-340.3 | 同 .2 但 venue 为 VG(无 `BATCH_CANCEL`) | 逐槽撤 s1、s2、s3 |

### OMS-341 HARD Kill、撤挂单的 Drain 与 CancelAll 命令撤全部可撤槽

- 规则:[输出扇出](README.md#输出扇出)
- 级别:table
- 里程碑:M2
- 前置:P0;同 OMS-340 前置(含 s6);调用 `cancel_all(Account(a1), open_only = false)`
- 实现:`crates/hft-oms/tests/out.rs::oms_341_full_cancel_scope`

| 行 | venue | 期望 |
|---|---|---|
| OMS-341.1 | VX(有 `BATCH_CANCEL`) | 一条 `CancelAll`;s1、s2、s3、s6 置 `CANCEL_SENT`;s4、s5 不变 |
| OMS-341.2 | VG | 逐槽撤 s1、s2、s3、s6 |

### OMS-342 撤单持续义务:连接恢复或 Unknown 收敛为 Live 时补撤

- 规则:[输出扇出](README.md#输出扇出)、[入口](README.md#入口全部在-sequencer-线程调用mut-self无分配)
- 级别:table
- 里程碑:M2
- 前置:P0;a1@VG(无批量撤单);`reissue_cancels(Account(a1), open_only)` 由测试按行调用
- 实现:`crates/hft-oms/tests/out.rs::oms_342_reissue_cancels`

| 行 | 场景 | 期望 |
|---|---|---|
| OMS-342.1 | 两个从未撤过的 `Live` Open 槽;`open_only = true` | 两条 `OrderDecision{Cancel}` |
| OMS-342.2 | 一个 `Live` `RISK_REDUCING` 槽;`open_only = true` | 无撤单 |
| OMS-342.3 | 同 .2;`open_only = false` | 一条撤单 |
| OMS-342.4 | 一个 `Unknown` 槽收敛为 `Live` 后调用;`open_only = false` | 一条撤单 |
| OMS-342.5 | 槽 `PendingCancel` 且置 `CANCEL_SENT` | 无撤单 |
| OMS-342.6 | 槽 `PendingCancel`,撤单曾 `SendReport{NoConn}`(`CANCEL_SENT` 已清) | 无撤单(`PendingCancel` 已在撤;由 `cancel_timeout_ms` 转 `Unknown` 后经查询收敛) |

### OMS-343 cancel_resting_on 只撤该账户该 inst 上无 EXEC 位的挂单

- 规则:[入口](README.md#入口全部在-sequencer-线程调用mut-self无分配)、[输出扇出](README.md#输出扇出)
- 级别:table
- 里程碑:M2
- 前置:P0;a1@VX 有下列槽;调用 `cancel_resting_on(a1, Y)`
- 实现:`crates/hft-oms/tests/out.rs::oms_343_cancel_resting_on`

| 行 | 槽 | 期望 |
|---|---|---|
| OMS-343.1 | 策略 GTC `Live`(Y,Open) | 撤 |
| OMS-343.2 | 策略 reduce-only GTC `Live`(Y,`RISK_REDUCING`) | 撤 |
| OMS-343.3 | 冒烟单 PostOnly `Live`(Y) | 撤 |
| OMS-343.4 | 策略 GTC `PendingNew`(Y) | 撤 |
| OMS-343.5 | 执行层 GTC `Live`(Y,`EXEC`) | 不撤 |
| OMS-343.6 | 策略 IOC `Live`(Y) | 不撤 |
| OMS-343.7 | 策略 GTC `Live`(Z@VG) | 不撤 |
| OMS-343.8 | a2 的 GTC(另一账户) | 不撤 |
| OMS-343.9 | 全部撤单 | 逐槽 `OrderDecision{Cancel}` + `OmsTransition{CancelReq}`;无 `CancelAll`;不记戳 |

### OMS-344 cancel_open 只撤该 cell 的 Open 类挂单

- 规则:[入口](README.md#入口全部在-sequencer-线程调用mut-self无分配)
- 级别:table
- 里程碑:M2
- 前置:P0;cell (a1, g1) 有下列槽;调用 `cancel_open((a1, g1))`
- 实现:`crates/hft-oms/tests/out.rs::oms_344_cancel_open`

| 行 | 槽 | 期望 |
|---|---|---|
| OMS-344.1 | `class = Open` GTC `Live` | 撤 |
| OMS-344.2 | `class = Open` PostOnly `PendingNew` | 撤 |
| OMS-344.3 | `class = Reduce` GTC `Live`,`RISK_REDUCING` | 不撤 |
| OMS-344.4 | `class = Open` IOC `Live` | 不撤(不是挂单) |
| OMS-344.5 | g2 上的 Open GTC | 不撤 |

### OMS-360 快照往返后状态与再次写出的字节都相同

- 规则:[快照行格式](README.md#快照行格式节-2312草案实现时以代码与编译期断言为准)、[入口](README.md#入口全部在-sequencer-线程调用mut-self无分配)
- 级别:unit
- 里程碑:M2
- 前置:P0;混合状态:三张表共 40 个非空闲槽(各状态都有,含 `Unknown`、`VERIFY`、`SMOKE_DUE`)、5 条非 `Done` 链接(含 `RESUME_HOLD`)、3 个非零 `LegState`、6 个未过期墓碑、`foreign`/`ext_held` 各 2 项、`pending_out` 20 项、`resume_hold_until ≠ 0`、`decode_fails = 2`
- 步骤:
  1. `snapshot(w1)` → `restore(r1)` → `snapshot(w2)`
- 期望:
  - `w1` 与 `w2` 逐字节相同;恢复实例的全部只读视图(`slot`、`inflight`、`legs`、`quiescent`、`venue_ready`、`link_peer`)与原实例逐值相等
- 实现:`crates/hft-oms/tests/snapshot.rs::oms_360_snapshot_roundtrip`

### OMS-361 快照各节的行内容与顺序

- 规则:[快照行格式](README.md#快照行格式节-2312草案实现时以代码与编译期断言为准)
- 级别:table
- 里程碑:M2
- 前置:P0;同 OMS-360,另加一条 `Done` 链接、一个全零的 `LegState`、一个过期墓碑、两个 `Free` 槽
- 实现:`crates/hft-oms/tests/snapshot.rs::oms_361_snapshot_rows`

| 行 | 节 | 期望 |
|---|---|---|
| OMS-361.1 | 2 | `count = 40`;只含非 `Free` 槽;按 `(venue_ix, account_ix, slot_id)` 升序;每行 192 字节 = `OrderSlot` 128 ‖ `SlotExt` 64 |
| OMS-361.2 | 3 | 首行 `OmsCountersHdr{engine_epoch 7, n_tables 5(夹具的 (venue, account) 表数), resume_hold_until}`;每表一行 `TableRow`(键升序),其后 `free`(FIFO,队头在前)、`gens`(含 `Free` 槽)、`foreign`、`ext_held`(FIFO)、`decode_fails` 与 7 字节零,补零到 8 字节 |
| OMS-361.3 | 12 | 行顺序 `Link*` → `Plan*` → `Leg*` → `Tomb*` → `Out*`;`Done` 链接不写;`Plan` 只写被活动链接引用的;全零 `LegState` 不写;过期墓碑不写;`Out` 行按 FIFO 顺序、不排序 |
| OMS-361.4 | 12 | 每行 `LinkRowHdr{kind, len}`:`len` 依次为 80、64、200、88、32(夹具含费用水位低于 `filled` 的墓碑,节 12 为 v5) |

### OMS-362 行版本:节 2 写 3、节 3 写 1、节 12 写 5,读到更高版本拒读

- 规则:[快照行格式](README.md#快照行格式节-2312草案实现时以代码与编译期断言为准)
- 级别:table
- 里程碑:M2
- 前置:P0;OMS-360 的快照字节,按行改 `SectionHdr.version`
- 实现:`crates/hft-oms/tests/snapshot.rs::oms_362_row_versions`

| 行 | 修改 | 期望 |
|---|---|---|
| OMS-362.1 | 不改 | 节 2 的 `version` 为 3,节 3 为 1、节 12 为 5(夹具墓碑有未记费的数量);`restore` 返回 `Ok` |
| OMS-362.2 | 节 2 `version = 4` | `Err(RestoreError)` |
| OMS-362.3 | 节 3 `version = 2` | `Err(RestoreError)` |
| OMS-362.4 | 节 12 `version = 6` | `Err(RestoreError)` |

### OMS-363 非法判别值从快照读回时报 RestoreError

- 规则:[快照行格式](README.md#快照行格式节-2312草案实现时以代码与编译期断言为准)、[LegLink 与 HedgePlan](README.md#leglink-与-hedgeplan)
- 级别:table
- 里程碑:M2
- 前置:P0;OMS-360 的快照字节,按行篡改一个字节
- 实现:`crates/hft-oms/tests/snapshot.rs::oms_363_bad_discriminants`

| 行 | 篡改 | 期望 |
|---|---|---|
| OMS-363.1 | 节 2 某行 `OrderSlot.state = 9` | `Err(RestoreError)` |
| OMS-363.2 | 节 2 某行 `OrderSlot.side = 0` | `Err(RestoreError)` |
| OMS-363.3 | 节 2 某行 `SlotExt.role = 6` | `Err(RestoreError)` |
| OMS-363.4 | 节 12 `Link` 行 `state` 为非法值 | `Err(RestoreError)` |
| OMS-363.5 | 节 12 `Tomb` 行 `side = 3` | `Err(RestoreError)` |
| OMS-363.6 | 节 12 `Plan` 行 `HedgePlan.px_mode = 4` | `Err(RestoreError)` |
| OMS-363.7 | 节 12 `LinkRowHdr.kind = 6` | `Err(RestoreError)` |

### OMS-364 恢复时由槽重算在途并与快照中的 LegState 校验

- 规则:[快照行格式](README.md#快照行格式节-2312草案实现时以代码与编译期断言为准)、[InflightBook](README.md#inflightbook)
- 级别:table
- 里程碑:M2
- 前置:P0;OMS-360 的快照字节
- 实现:`crates/hft-oms/tests/snapshot.rs::oms_364_inflight_recomputed`

| 行 | 修改 | 期望 |
|---|---|---|
| OMS-364.1 | 不改 | `restore` 后 InflightBook 各项等于按节 2 逐槽求和;各腿两类在途与快照中的 `LegState` 相等 |
| OMS-364.2 | 把某 `Leg` 行的 `inflight_immediate` 改大 1 raw | `Err(RestoreError)` |

### OMS-365 (account, inst) 归属表不进快照,由恢复时的 GroupSpec 重建

- 规则:[快照行格式](README.md#快照行格式节-2312草案实现时以代码与编译期断言为准)、[LegTracker](README.md#legtracker)
- 级别:unit
- 里程碑:M2
- 前置:P0;快照时 groups = {g1, g2}
- 步骤:
  1. `restore`;`on_groups_changed({g1, g2})`;`place` 买 Z(g2)
- 期望:
  - 快照字节中不含归属表;新槽 `SlotExt.leg_ix = 1`;恢复前后 `legs(a1, g2)` 相同
- 实现:`crates/hft-oms/tests/snapshot.rs::oms_365_group_map_rebuilt`

### OMS-366 快照节字节的金样

- 规则:[快照行格式](README.md#快照行格式节-2312草案实现时以代码与编译期断言为准)
- 级别:golden
- 里程碑:M2
- 输入:固定脚本 `fixtures/oms/OMS-366/script.txt`(P0;10 个事件:下单、Ack、部分成交、链接、外来成交、撤单待发)驱动后调用 `snapshot`
- 固定脚本无墓碑且未启用显式结算;节 2 v3 与节 12 v2 的行体须与旧格式逐字节相同,仅节头版本变化。迟到回报用例在回收或 SimReset 前保存原订单的完整身份,不能从复用后的槽拼接。
- 期望文件:`fixtures/oms/OMS-366/sections.bin`(节 2、3、12 的完整字节)与 `fixtures/oms/OMS-366/sections.txt`(逐字段的可读转储)
- 实现:`crates/hft-oms/tests/snapshot.rs::oms_366_snapshot_golden`

### OMS-380 任意回报交错下终态与成交量等于参考模型

- 规则:[状态机](README.md#状态机)、[回报处理表](README.md#回报处理表)、[SendReport 处理](README.md#sendreport-处理)
- 级别:prop
- 里程碑:M2
- 性质:任意 place/cancel/amend 与交易所回报、`SendReport`、连接事件的交错、重复、乱序与丢失执行完毕后(最后补一轮 `QueryResult` 给出交易所真值),每个槽的 `filled` 与终态等于参考模型;必含路径:fill-before-ack、因已成交被拒的撤单、Ack 在 Fill 之后、重复 Fill、`cum_filled` 回退、槽复用后迟到的上一代回报(墓碑内入账、墓碑外 `StaleGen`)
- 生成:1–64 张订单(tif ∈ {Gtc, Ioc, PostOnly, Fok},venue ∈ {VS, VX, VG});每张 0–8 条回报,从 {Ack, PartialFill, Fill, CancelAck, Expired, CancelReject(GONE/其他), AmendAck, AmendReject, Reject, OutcomeUnknown, FeeOnly, 重复上一条} 抽取;`SendReport` 状态从 8 种中抽取;随机置换顺序,每条以 5% 概率丢弃;`reuse_quarantine_ms` ∈ {0, 5000}
- 参考:`crates/hft-oms/tests/model.rs` 的 `Vec<Order>` 语义实现(按 README 回报处理表逐格编码,不共享被测代码)
- 例数:PR 2 000
- 实现:`crates/hft-oms/tests/prop_fsm.rs::oms_380_fsm_matches_model`

### OMS-381 数据布局一节的不变量每步成立

- 规则:[数据布局](README.md#数据布局草案实现时以代码与编译期断言为准)、[LegTracker](README.md#legtracker)、[InflightBook](README.md#inflightbook)
- 级别:prop
- 里程碑:M2
- 性质:OMS-380 的每一步之后:`filled ≤ qty`(超量成交除外,带 `OVERFILL`);`filled` 单调不减;`net_filled` 等于逐笔按净量规则调整后的累计;`state == Free ⇔ 槽在空闲表中`;`unknown_count == 本表 Unknown 槽数`;InflightBook 各项等于对非终态槽求和;各腿 `inflight_immediate + inflight_resting` 等于该腿非终态槽带符号剩余量之和,且每个槽只计入一类
- 生成:同 OMS-380,另加随机 `on_sweep` 与 `on_unknown_query`
- 参考:逐项重算的断言函数 `check_invariants(&Oms)`
- 例数:PR 2 000
- 实现:`crates/hft-oms/tests/prop_fsm.rs::oms_381_layout_invariants`

### OMS-382 第二腿请求量与余数之和守恒,且从不向上取整

- 规则:[OnFill](README.md#onfill)、[数量对齐](README.md#数量对齐)
- 级别:prop
- 性质:无外来成交、无裸量截量时,任意成交拆分与费用序列下 `Σ 第二腿请求量 + residual[g][h] == Σ 第一腿成交净量 × num/den`(误差 ≤ 晚到 `FeeOnly` 调整条数 raw;无调整时精确相等);每条第二腿请求量 ≤ 对应 `raw` 且为 lot 的整数倍
- 生成:ratio ∈ {1/1, 2/1, 1/3};第一腿 1–20 笔增量(raw 1..=10⁷),费用资产 ∈ {BTC, USDT, ASSET_NONE};`FeeOnly` 晚到 0–5 条
- 参考:i128 精确算术的逐笔求和
- 例数:PR 2 000
- 里程碑:M2
- 实现:`crates/hft-oms/tests/link_props.rs::oms_382_hedge_sum_conserved`

### OMS-383 第二腿与重试数量从不超过发出时的该方向裸量

- 规则:[OnFill](README.md#onfill)、[LegTracker](README.md#legtracker)
- 级别:prop
- 里程碑:M2
- 性质:任意序列(含外来成交、执行层平仓、费用晚到、重复回报)下,每条 `Second`/`SecondRetry` 请求的 `qty ≤ floor_to_lot(naked_dir)`(按产出时刻的 LegTracker 计算);`HedgeRequest.role ∈ {Second, SecondRetry, Reverse}`;`hedge_cap_qty = basis + 1 lot`
- 生成:OMS-382 的生成器 + 随机 `external_adjust`(四种 cause,eid 0 或随机)+ 随机 `ext_held` 命中
- 参考:独立计算的 `naked_dir`
- 例数:PR 2 000
- 实现:`crates/hft-oms/tests/link_props.rs::oms_383_hedge_le_naked`

### OMS-384 同 group 连续多条链接的余数被后续链接吸收

- 规则:[OnFill](README.md#onfill)
- 级别:prop
- 里程碑:M2
- 性质:同一 group 先后 1–10 条链接,全部结束后 `Σ 各链接第二腿请求量 + 最终 residual == Σ 全部第一腿净量 × ratio`;任一时刻 `|residual| <` lot + 累计晚到费用 × ratio
- 生成:每条链接 1–5 笔第一腿增量
- 参考:同 OMS-382
- 例数:PR 1 000
- 实现:`crates/hft-oms/tests/link_props.rs::oms_384_residual_across_links`

### OMS-385 重试计次、reduce_only 继承与门控不计次

- 规则:[OnFill](README.md#onfill)、[RISK_REDUCING 判定](README.md#risk_reducing-判定)
- 级别:prop
- 里程碑:M2
- 性质:任意执行失败与门控交错下,每条链接紧随执行失败产生的 `SecondRetry` 条数 ≤ `hedge_retry_max`(门控后的到期重试不计入);`retries` 只在执行失败时增加;每条 `SecondRetry` 的 `reduce_only == HedgePlan.reduce_only`;门控(`StaleQuote`、`BeyondCross`、风控位、不计次类本地发送失败)不改变 `retries`
- 生成:`hedge_retry_max` ∈ 0..=5;失败序列从 {交易所拒, IOC 部分成交, IOC 零成交, EncodeError, KillBlocked, RateLimited, NoConn, RingFull, StaleQuote, BeyondCross, Risk(KILLED)} 抽取 1–12 次
- 参考:计次规则的直接编码
- 例数:PR 2 000
- 实现:`crates/hft-oms/tests/link_props.rs::oms_385_retry_counting`

### OMS-386 Simultaneous 的反向量等于未对冲量

- 规则:[Simultaneous](README.md#simultaneous)
- 级别:prop
- 里程碑:M2
- 性质:两腿任意成交量(含 0)终态后,`on_fail = Reverse` 时反向单的数量 = `floor_to_lot(|first_net × num/den − hedge_filled|)`(按被反向那一腿换算),方向使两腿差缩小;相等时不出反向单
- 生成:第一腿成交 0..=qty、第二腿成交 0..=qty × ratio,raw 随机
- 参考:直接计算
- 例数:PR 1 000
- 实现:`crates/hft-oms/tests/link_props.rs::oms_386_simul_reverse_qty`

### OMS-387 HEDGE_LIMIT 否决之后该链接不再产生任何订单

- 规则:[OnFill](README.md#onfill)
- 级别:prop
- 里程碑:M2
- 性质:任意时刻 `hedge_vetoed(L)` 之后,后续任意事件序列中没有 `link_id = L` 的 `HedgeRequest` 或 `OrderDecision{New}`
- 生成:OMS-383 的生成器,随机位置插入 `hedge_vetoed`
- 参考:输出过滤断言
- 例数:PR 1 000
- 实现:`crates/hft-oms/tests/link_props.rs::oms_387_vetoed_no_orders`

### OMS-388 任意容量下输出与容量无限的参考实现逐条相同

- 规则:[输出扇出](README.md#输出扇出)
- 级别:prop
- 里程碑:M2
- 性质:批量入口与随机事件交错、`max_out_per_event` ∈ 1..=32、`out_quota` ∈ 1..=64 时,把每次调用与全部 `on_continue` 的输出拼接,得到的序列与容量无限的参考实现(同一事件序列)逐条相同;状态在每次调用返回时即与参考实现相同
- 生成:2–4 个 (venue, account) 队列,每队 0–600 个槽;批量入口从 {`cancel_all`, `cancel_open`, `cancel_resting_on`, `reissue_cancels`, `on_conn_state`, `force_unknown`, `on_hedge_gate_open`, `on_sweep`} 抽取
- 参考:`OmsOut` 容量设为 usize::MAX 的同一实现
- 例数:PR 500
- 实现:`crates/hft-oms/tests/out_props.rs::oms_388_fanout_matches_unbounded`

### OMS-389 任意字节的快照节只会恢复成功或报 RestoreError

- 规则:[快照行格式](README.md#快照行格式节-2312草案实现时以代码与编译期断言为准)
- 级别:prop
- 里程碑:M2
- 性质:对 OMS-360 快照的任意字节翻转、截断与追加,`restore` 返回 `Ok` 或 `Err(RestoreError)`,不 panic、不越界读
- 生成:翻转 1–16 个随机字节;截断到随机长度;追加 0–64 个随机字节
- 参考:无 panic 断言(`catch_unwind`)
- 例数:PR 2 000
- 实现:`crates/hft-oms/tests/snapshot_props.rs::oms_389_restore_never_panics`

### OMS-390 同一输入序列两次运行的输出字节相同

- 规则:[入口](README.md#入口全部在-sequencer-线程调用mut-self无分配)、[快照行格式](README.md#快照行格式节-2312草案实现时以代码与编译期断言为准)
- 级别:prop
- 里程碑:M2
- 性质:同一事件序列喂给两个新建实例,全部输出(决策、Note、更新、对冲请求、定时器请求)与最终 `snapshot` 字节逐字节相同
- 生成:OMS-380 与 OMS-383 的生成器
- 参考:两次运行逐字节比较
- 例数:PR 500
- 实现:`crates/hft-oms/tests/prop_fsm.rs::oms_390_deterministic`

### OMS-400 故障注入下引擎持仓与模拟交易所一致、无槽泄漏、无重复对冲

- 规则:[状态机](README.md#状态机)、[OnFill](README.md#onfill)、[Unknown 收敛流程](README.md#unknown-收敛流程)
- 级别:dst
- 里程碑:M2
- 前置:testkit 模拟交易所(hft-sim 撮合器)× VS、VX;L1 类 OnFill 链接的配对开仓策略 + 单腿 GTC 挂单策略;P0
- 故障:丢包 1%、重复回报 2%、乱序窗口 50 ms、断连(每 10 min 一次,持续 1–5 s)、限速拒绝 1%、部分成交、费用晚到(10% 的成交只带 `ASSET_NONE`,0.5–5 s 后补 `FeeOnly`)、迟到成交(槽回收后 0–10 s)、第二腿被拒 5%
- 种子:PR 200(提交派生)+ 回归集;每夜日期派生
- 断言:场景结束(补一轮对账)时引擎每 (account, inst) 持仓 == 模拟交易所持仓;占用槽数 == 仍在途订单数;每笔第一腿成交至多被对冲一次(同一成交的第二腿请求量合计 ≤ 其净量 × ratio + 1 lot);INV-2、INV-7、INV-19 每步成立
- 实现:`crates/hft-testkit/src/scenarios/oms.rs::oms_400_faults_reconcile_zero`

### OMS-401 第二腿盘口 10 s 不变而连接照常回 pong 时对冲不被卡住

- 规则:[对冲定价](README.md#对冲定价)
- 级别:dst
- 里程碑:M2
- 门槛:④
- 前置:P0;两腿 VS(现货)、VX(永续);`conn_liveness_ms = 2000`
- 故障:VX 的 BBO 自 t = −10 s 起不变,该连接每 500 ms 回 pong;t = 0 第一腿成交 0.012;另一支:VX 连接自 t = 0 起不回任何帧 5 s 后恢复
- 种子:PR 200 + 回归集;每夜日期派生
- 断言:前一支第二腿 `OrderDecision{New}` 在第一腿成交的同一 Input 内发出,全程无 `HedgeGate::Px(StaleQuote)`、无 `OmsHedgePxStale`;后一支 t ∈ (2 s, 5 s] 内只出现门控与 HedgeRetry,连接恢复后的第一次重试发出,`retries = 0`,若 `now − first_fill_tsc > hedge_deadline_ms` 才 `Broken`
- 实现:`crates/hft-testkit/src/scenarios/oms.rs::oms_401_quiet_book_hedges`

### OMS-402 RecvWindow 类 venue 查询暂时返回不存在后成交

- 规则:[Unknown 收敛流程](README.md#unknown-收敛流程)
- 级别:dst
- 里程碑:M2
- 前置:P0;VS(`RecvWindow`)上的对冲腿与非链接单;`not_found_grace_ms[VS] = 5500`
- 故障:模拟交易所对已接受的订单在 0–8 s 内按 client id 查询返回 NotFound,之后成交;对账成交历史按 reconciler 规则在两个不同对账轮次都确认之后给 `history_checked = 1`
- 种子:PR 200 + 回归集;每夜日期派生
- 断言:成交到达前这些槽从未成为 `Rejected`/`Cancelled`;没有对这些槽的重发 `New`、没有 `SecondRetry`;成交到达后 LegTracker 与模拟交易所一致;对冲腿确认晚于 `hedge_deadline_ms` 的链接为 `Broken{Deadline}` 且无重复对冲
- 实现:`crates/hft-testkit/src/scenarios/oms.rs::oms_402_notfound_then_fill`

### OMS-403 崩溃与停机后恢复:链接续接、不误判 Broken

- 规则:[恢复:停机时间不计与闸门续接](README.md#恢复停机时间不计与闸门续接)
- 级别:dst
- 里程碑:M5
- 前置:P0;`resume_hold_max_ms = 30000`;配对开仓进行中
- 故障:在随机事件点崩溃(含第一腿已成交、第二腿在途),停机 10 s–30 min;重启后第二腿 venue 的对冲闸门子集在 0–40 s 后满足,另一 venue 的对账延迟;10% 的种子在恢复期内再次崩溃
- 种子:PR 200 + 回归集;每夜日期派生
- 断言:`recovery_end` 之前与 `RESUME_HOLD` 期间无 `Broken{Deadline}`;每条续接链接恰好一次续接请求;闸门 30 s 内未满足且有敞口的链接 `Broken{ResumeTimeout}`;恢复后持仓与模拟交易所一致;`replay verify` 对崩溃前后的 journal 一致
- 实现:`crates/hft-testkit/src/scenarios/oms.rs::oms_403_crash_resume`

### OMS-404 费用晚到时按净量补差,group 余数被吸收

- 规则:[成交净量](README.md#成交净量b4)
- 级别:dst
- 里程碑:M2
- 前置:P0;现货腿 VS 基础资产扣费
- 故障:50% 的现货成交只带 `ASSET_NONE`,0.1–3 s 后补 `FeeOnly`;5% 的 `FeeOnly` 在槽回收之后到达
- 种子:PR 200 + 回归集;每夜日期派生
- 断言:场景结束时每腿 `net` 等于模拟交易所的库存变化;`|residual|` < 1 lot + 未吸收的费用 × ratio;回收后到达的 `FeeOnly` 按墓碑归属,无 `OmsFeeUnattributed`
- 实现:`crates/hft-testkit/src/scenarios/oms.rs::oms_404_late_fees`

### OMS-405 对手价跳变超滑点时第二腿按越价上限发出

- 规则:[对冲定价](README.md#对冲定价)
- 级别:dst
- 里程碑:M2
- 前置:P0;`max_slip = 0.001`、`rr_max_cross = 0.005`
- 故障:第一腿成交前后第二腿对手价跳变 0.05%–1%
- 种子:PR 200 + 回归集;每夜日期派生
- 断言:跳变 ≤ `rr_max_cross` 时第二腿在同一 Input 内以 `capped = true` 的价格发出,不计入 `retries`;超过时只门控(`BeyondCross`),价格回落后发出;无风控 #5 拒绝
- 实现:`crates/hft-testkit/src/scenarios/oms.rs::oms_405_slip_jump`

### OMS-410 含 OMS 的 journal 回放逐字节一致

- 规则:[入口](README.md#入口全部在-sequencer-线程调用mut-self无分配)、[恢复:停机时间不计与闸门续接](README.md#恢复停机时间不计与闸门续接)
- 级别:replay
- 里程碑:M2
- 输入:`fixtures/replay/oms-basic/`(下单、成交、链接、Unknown 收敛、外来成交、冒烟单)与 `fixtures/replay/oms-recover/`(含一次 `recovery_end` 与之后的新段)
- 期望:`replayer verify` 对两个样本的意图流与状态哈希序列逐字节一致;第二个样本从 `purpose = Recovery` 快照起步的新段同样一致
- 实现:`apps/replayer/tests/tail.rs::oms_410_replay_verify`

### OMS-420 三所测试网与模拟盘联调中 Unknown 收敛与槽泄漏

- 规则:[Unknown 收敛流程](README.md#unknown-收敛流程)
- 级别:testnet
- 里程碑:M6
- 环境:dev2 联调窗口;Binance 测试网/演示环境、OKX 模拟盘、Bitget 模拟盘;`prod` 角色实例 `feed_env = "testnet"`
- 授权:无(测试网与模拟盘账户)
- 步骤:
  1. 按 [testing §8](../../docs/testing.md#8-上线门槛) 门槛 ⑥ 的联调计划运行;期间人为断开下单连接与用户流各 3 次
  2. 窗口结束时导出 `Note{OmsUnknown}` 与收敛迁移的时刻
- 通过标准:Unknown 进入到收敛的 p99 < 3 s(需要成交历史确认的 NotFound 单列统计,不计入该 p99);窗口结束且全部订单终态后占用槽数为 0
- 证据:`ops-log/testnet/OMS-420-<日期>.md`(统计表与原始 journal 指针)
- 实现:`tests/host/OMS-420.sh`

### OMS-421 模拟账户长跑零槽泄漏

- 规则:[数据布局](README.md#数据布局草案实现时以代码与编译期断言为准)
- 级别:host
- 里程碑:M6
- 环境:dev2 的 `sim` 实例,模拟账户连续运行(门槛 ⑦ 的同一运行)
- 授权:无
- 步骤:
  1. 每日从 `replayer` 报告读取各 (venue, account) 表的占用槽数与非终态订单数
- 通过标准:每日报告中占用槽数 == 非终态订单数;无 `OmsUnknownStuck`;Unknown p99 收敛时间 < 3 s(口径同 OMS-420)
- 证据:`ops-log/sim/OMS-421-<日期>.md`
- 实现:`tests/host/OMS-421.sh`

### OMS-440 place 的延迟

- 规则:[延迟与资源契约](README.md#延迟与资源契约)
- 级别:bench
- 里程碑:M2
- 基准名:`oms/place`
- 实现:`crates/hft-oms/benches/oms.rs::oms_440_place`

### OMS-441 Ack 回报处理的延迟

- 规则:[延迟与资源契约](README.md#延迟与资源契约)
- 级别:bench
- 里程碑:M2
- 基准名:`oms/on_event_ack`
- 实现:`crates/hft-oms/benches/oms.rs::oms_441_on_event_ack`

### OMS-442 成交回报处理(不含第二腿)的延迟

- 规则:[延迟与资源契约](README.md#延迟与资源契约)
- 级别:bench
- 里程碑:M2
- 基准名:`oms/on_event_fill`
- 实现:`crates/hft-oms/benches/oms.rs::oms_442_on_event_fill`

### OMS-443 成交到第二腿 HedgeRequest 的延迟

- 规则:[延迟与资源契约](README.md#延迟与资源契约)
- 级别:bench
- 里程碑:M2
- 基准名:`oms/leglink_onfill`
- 实现:`crates/hft-oms/benches/oms.rs::oms_443_leglink_onfill`

### OMS-444 price_hedge 的延迟

- 规则:[延迟与资源契约](README.md#延迟与资源契约)
- 级别:bench
- 里程碑:M2
- 基准名:`oms/price_hedge`
- 实现:`crates/hft-oms/benches/oms.rs::oms_444_price_hedge`

### OMS-445 hedge_gated / hedge_vetoed 的延迟

- 规则:[延迟与资源契约](README.md#延迟与资源契约)
- 级别:bench
- 里程碑:M2
- 基准名:`oms/hedge_gated`
- 实现:`crates/hft-oms/benches/oms.rs::oms_445_hedge_gated`

### OMS-446 on_unknown_query 的延迟

- 规则:[延迟与资源契约](README.md#延迟与资源契约)
- 级别:bench
- 里程碑:M2
- 基准名:`oms/unknown_query`
- 实现:`crates/hft-oms/benches/oms.rs::oms_446_unknown_query`

### OMS-447 墓碑命中迟到成交的延迟

- 规则:[延迟与资源契约](README.md#延迟与资源契约)
- 级别:bench
- 里程碑:M2
- 基准名:`oms/tomb_late_fill`
- 实现:`crates/hft-oms/benches/oms.rs::oms_447_tomb_late_fill`

### OMS-448 external_adjust / net_adjust 的延迟

- 规则:[延迟与资源契约](README.md#延迟与资源契约)
- 级别:bench
- 里程碑:M2
- 基准名:`oms/leg_adjust`
- 实现:`crates/hft-oms/benches/oms.rs::oms_448_leg_adjust`

### OMS-449 on_sweep(256 槽)的延迟

- 规则:[延迟与资源契约](README.md#延迟与资源契约)
- 级别:bench
- 里程碑:M2
- 基准名:`oms/sweep_256`
- 实现:`crates/hft-oms/benches/oms.rs::oms_449_sweep_256`

### OMS-450 on_hedge_gate_open(64 条链接)的延迟

- 规则:[延迟与资源契约](README.md#延迟与资源契约)
- 级别:bench
- 里程碑:M2
- 基准名:`oms/hedge_gate_open_64`
- 实现:`crates/hft-oms/benches/oms.rs::oms_450_hedge_gate_open_64`

### OMS-451 on_continue(交出 16 项)的延迟

- 规则:[延迟与资源契约](README.md#延迟与资源契约)
- 级别:bench
- 里程碑:M2
- 基准名:`oms/continue_16`
- 实现:`crates/hft-oms/benches/oms.rs::oms_451_continue_16`

### OMS-452 cancel_resting_on(8 个槽)的延迟

- 规则:[延迟与资源契约](README.md#延迟与资源契约)
- 级别:bench
- 里程碑:M2
- 基准名:`oms/cancel_resting_8`
- 实现:`crates/hft-oms/benches/oms.rs::oms_452_cancel_resting_8`

### OMS-453 冒烟单生成、建槽与到期的延迟

- 规则:[延迟与资源契约](README.md#延迟与资源契约)
- 级别:bench
- 里程碑:M2
- 基准名:`oms/smoke`
- 实现:`crates/hft-oms/benches/oms.rs::oms_453_smoke`

### OMS-454 write_off_residual 的延迟

- 规则:[延迟与资源契约](README.md#延迟与资源契约)
- 级别:bench
- 里程碑:M2
- 基准名:`oms/write_off`
- 实现:`crates/hft-oms/benches/oms.rs::oms_454_write_off`

### OMS-455 recovery_end(4096 槽 + 4096 链接)的延迟

- 规则:[延迟与资源契约](README.md#延迟与资源契约)
- 级别:bench
- 里程碑:M2
- 基准名:`oms/recovery_end_4096`
- 实现:`crates/hft-oms/benches/oms.rs::oms_455_recovery_end_4096`

### OMS-456 只能减量的 venue 上改价或增量的改单被拒且无输出

- 规则:[状态机](README.md#状态机)
- 级别:table
- 里程碑:M2
- 前置:P0;a1@VX 槽 t(卖 Y 0.01 @ 60100.0,GTC)`Live`;`VX.flags` 含 `AMEND`;`VX.amend_qty_down_only` 如列;每行调用一次 `amend(t, px, qty)`
- 实现:`crates/hft-oms/tests/fsm.rs::oms_456_amend_qty_down_only`

| 行 | `amend_qty_down_only` | `px` / `qty` | 期望 |
|---|---|---|---|
| OMS-456.1 | true | 60100.0 / 0.008 | `Ok`;t `PendingAmend`,`pending_qty = 0.008`;`OrderDecision{Amend}` 一条 |
| OMS-456.2 | true | 60100.0 / 0.01(同量) | `Ok`(新数量 ≤ 当前数量) |
| OMS-456.3 | true | 60100.0 / 0.011 | `Err(OpError::AmendUnsupported)`;t 仍 `Live`、未置 `AMEND_SENT`;输出为空 |
| OMS-456.4 | true | 60200.0 / 0.008 | `Err(OpError::AmendUnsupported)`;输出为空 |
| OMS-456.5 | true | 60099.9 / 0.01 | `Err(OpError::AmendUnsupported)` |
| OMS-456.6 | false | 60200.0 / 0.011 | `Ok`;`PendingAmend` |
| OMS-456.7 | true;t 为 `PendingCancel` | 60100.0 / 0.008 | `Err(OpError::NotAllowed)`(状态先于该项) |

### OMS-457 link_valid 逐条判定策略链接的结构

- 规则:[LegLink 与 HedgePlan](README.md#leglink-与-hedgeplan)
- 级别:table
- 里程碑:M2
- 前置:P0;以 L1 的 `LegLink`(`role = First`、OnFill、`on_fail = Retry`、`tif = Ioc`、ratio 1/1)为基准,每行只改所列字段,调用 `link_valid`
- 实现:`crates/hft-oms/tests/link.rs::oms_457_link_valid`

| 行 | 改动 | 期望 |
|---|---|---|
| OMS-457.1 | 无 | true |
| OMS-457.2 | `role = Second` | false |
| OMS-457.3 | `role = SecondRetry` / `Reverse` / `HedgeFix`(三个子例) | false |
| OMS-457.4 | `hedge.kind = None` | false |
| OMS-457.5 | `on_fail = Reverse` / `Hold` | true |
| OMS-457.6 | `tif = Fok` | false(OnFill 只取 Ioc) |
| OMS-457.7 | `tif = Gtc` | false |
| OMS-457.8 | `kind = Simultaneous`,`on_fail = Retry` | false |
| OMS-457.9 | `kind = Simultaneous`,`on_fail = Reverse`,`tif = Fok` | true |
| OMS-457.10 | `kind = Simultaneous`,`on_fail = Hold`,`tif = Ioc` | true |
| OMS-457.11 | `ratio_num = 0` | false |
| OMS-457.12 | `ratio_den = 0` | false |

### OMS-458 HARD 下第二腿在风控 #1 被拒后按门控交回,不计次、截止照算

- 规则:[RISK_REDUCING 判定](README.md#risk_reducing-判定)、[OnFill](README.md#onfill)
- 级别:unit
- 里程碑:M2
- 前置:P0;`hedge_deadline_ms = 3000`、`hedge_retry_min_ms = 100`;L1 第一腿 t = 0 成交 0.012,`HedgeRequest{Second, 0.012}` 已产出;a1 上 `Kill{Account, hard}` 生效
- 步骤:
  1. t = 1 sequencer 以 `hedge_gated(L1, HedgeGate::Risk(HARD), retry_at = 0, now)` 交回(风控 #1 拒,未建槽)
  2. 之后每次 HedgeRetry 都同样被 #1 拒,直到 t = 3001
- 期望:
  - 步骤 1:`retries = 0`;登记 `Timer{Oms, HedgeRetry{L1}}` 于 t = 101;无新槽、无 `OrderDecision{New}` 由 OMS 产出
  - 步骤 2:`retries` 始终为 0;t = 3001 的那次交回把 L1 置 `Broken{Deadline}`、`OmsHedgeBroken`(P0)
- 实现:`crates/hft-oms/tests/link.rs::oms_458_hard_second_leg_gated`

### OMS-459 截止从最早一笔尚未对冲完的第一腿成交起算

- 规则:[OnFill](README.md#onfill)、[配置项](README.md#配置项omsconfig)
- 级别:table
- 里程碑:M2
- 前置:P0;`hedge_deadline_ms = 3000`、`hedge_retry_max = 10`、`hedge_retry_min_ms = 100`;L1 第一腿 GTC 买 X 0.02;t = 0 成交 0.005,对应第二腿 t = 100 全部成交(.5、.6 另列起始序列)
- 实现:`crates/hft-oms/tests/link.rs::oms_459_deadline_from_first_fill`

| 行 | 之后的序列 | 期望 |
|---|---|---|
| OMS-459.1 | t = 5000 第一腿再成交 0.005,第二腿 t = 5100 全部成交 | 无 `Broken`;L1 `Hedging`(第一腿未终态) |
| OMS-459.2 | t = 5000 第一腿再成交 0.005,第二腿 t = 5100 被拒 | t = 100 第二腿全部成交时 `first_fill_tsc = 0`;t = 5000 成交后 `first_fill_tsc = 5000 ms`;失败处理时 `now − first_fill_tsc = 100 ≤ 3000`:不 Broken,登记 HedgeRetry,`retries = 1` |
| OMS-459.4 | 同 .2,重试单持续被交易所拒,失败处理分别在 t = 7999、8000、8001 | t = 7999、8000:不 Broken(`now − first_fill_tsc` = 2999、3000);t = 8001:`Broken{Deadline}` + `OmsHedgeBroken`(P0) |
| OMS-459.5 | 起始序列改为:t = 0 成交 0.005,第二腿 t = 100 被拒;t = 2000 第一腿再成交 0.005;重试单持续被拒,失败处理在 t = 3001 | `first_fill_tsc` 保持 0 ms(t = 2000 的成交不改起点);t = 3001 `Broken{Deadline}` |
| OMS-459.6 | 同 .5,但 t = 2500 第二腿重试单成交 0.010(未对冲量回到 0) | `first_fill_tsc = 0`;t = 3001 无 `Broken` |
| OMS-459.3 | 无新成交,t = 10 000 `on_sweep` | 无 `Broken`(`owed = 0` 且 `hedge_open = 0`) |

### OMS-460 数量已由非成交回报带来的成交回报只入账费用,重复回报不入账

- 规则:[回报匹配与成交增量](README.md#回报匹配与成交增量)
- 级别:unit
- 里程碑:M2
- 前置:P0;a1@VS 买 X 0.01 @ 60 000(GTC)已 Ack;发出撤单(`PendingCancel`)后收到 `CancelReject{detail = GONE, cum_filled = 0.01, avg_px = 60 000, fee = 0}`:槽 → `Filled`,`FillOut` 一条、`fee = 0`
- 步骤:
  1. 投递 `Fill{cum_filled = 0.01, fee = 1000, fee_asset = USDT}`
  2. 再投递同一条 `Fill`
  3. 另一张同样的单走到同一状态后,依次投递 `PartialFill{cum_filled = 0.004, fee = 400}`、同一条 `PartialFill`、`Fill{cum_filled = 0.01, fee = 600}`
- 期望:
  - 步骤 1:无 `FillOut`;`FeeOut` 一条(inst X、`fee = 1000`、`fee_asset = USDT`);`SlotExt.fee_cum = 0.01`;`fee_accum = 1000`
  - 步骤 2:无 `FillOut`、无 `FeeOut`
  - 步骤 3:第一条 `PartialFill` 产出 `FeeOut{fee = 400}`;重复的那条不产出;`Fill` 产出 `FeeOut{fee = 600}`;`fee_accum = 1000`
- 实现:`crates/hft-oms/tests/matching.rs::oms_460_fee_of_fill_after_non_fill_report`

### OMS-461 行版本 1 的节 2 行恢复时 fee_cum 取 filled

- 规则:[快照行格式](README.md#快照行格式节-2312草案实现时以代码与编译期断言为准)
- 级别:unit
- 前置:P0;a1@VS 买 X 0.01 已 Ack、成交 0.004(`fee_cum = 0.004`);快照后把节 2 改写为行版本 1(每行只留前 160 字节,`SectionHdr.version = 1`、`len` 按 160 B 行)
- 里程碑:M2
- 步骤:
  1. 以改写后的快照 `restore`
  2. 以原快照(行版本 2)`restore`
- 期望:
  - 步骤 1 成功;该槽 `filled = 0.004`、`SlotExt.fee_cum = 0.004`,其余 `SlotExt` 字段与写出时相同
  - 步骤 2 成功;`fee_cum = 0.004`
- 实现:`crates/hft-oms/tests/snapshot.rs::oms_461_restore_slot_rows_v1`

### OMS-462 墓碑上的重复回报计数,FeeOnly 仍独立入账

- 规则:[墓碑与迟到成交(C1)](README.md#墓碑与迟到成交c1)
- 级别:table
- 里程碑:M2
- 前置:P0;a1@VS 一张 X 的 Buy 单已成交 0.01,均价 60000;超过隔离期后 sweep 回收为 Free,墓碑仍保留
- 步骤:投递表中回报,均带 fee 1000、fee_asset USDT
- 期望:槽保持 Free,墓碑 filled 保持 0.01;无 FillOut、ForeignFill、下单或 Note;除 FeeOnly 外无 FeeOut
- 实现:`crates/hft-oms/tests/tomb.rs::oms_462_tomb_duplicate_count_and_fee_only`

| 行 | 回报 | cum_filled | 额外期望 |
|---|---|---|---|
| OMS-462.1 | Ack | 0 | dup +1 |
| OMS-462.2 | CancelAck | 0 | dup +1 |
| OMS-462.3 | QueryResult | 0.01 | dup +1 |
| OMS-462.4 | Fill | 0.01 | dup +1 |
| OMS-462.5 | PartialFill | 0.004 | dup +1 |
| OMS-462.6 | FeeOnly | 0.01 | dup 不变,一条归属 a1@VS/X 的 FeeOut,fee = 1000 |

### OMS-463 同一 Input 内完成并复用链接的诊断量

- 规则:[OnFill](README.md#onfill)
- 级别:unit
- 里程碑:M2
- 前置:`testkit` feature;每账户一个链接;第一腿成交且第二腿在途
- 步骤:一次诊断事件内完成第二腿并立即发下一张第一腿,复用链接槽
- 期望:退休记录保存完成后的第一腿净量、第二腿请求与成交累计量,订单 epoch/slot/gen 与创建序号保持旧身份;当前行拥有新身份和零累计量;下一事件清空退休缓冲而保留当前身份
- 实现:`crates/hft-oms/tests/audit.rs::oms_463_completed_link_reused_in_one_input`

### OMS-464 恢复后迟到的第一腿成交不产生重复续接

- 规则:[恢复:停机时间不计与闸门续接](README.md#恢复停机时间不计与闸门续接)
- 级别:unit
- 里程碑:M2
- 前置:IOC 第一腿尚无回报,快照后恢复为 Unknown,第二腿 venue 对冲闸门已满足
- 步骤:同一 Input 确认第一腿全成并解除 Unknown,随后第二腿成交
- 期望:恰一条 Second,无重复 SecondRetry;RESUME_HOLD 与 RESUME_RETRY 清除,第二腿完成后链接 Done
- 实现:`crates/hft-oms/tests/recover.rs::oms_464_late_first_fill_does_not_double_resume`

### OMS-465 ECF1 OMS 配置完整往返

- 规则:[配置项](README.md#配置项omsconfig)
- 级别:unit
- 里程碑:M2
- 前置:所有标量均非默认值,venue 与表覆盖含 Some/None
- 步骤:经 to_rows/from_rows 与 EcfBuilder/from_engine 两条路径往返;同键再写一条覆盖
- 期望:所有字段保持;可选 None 不被转为 0;重复覆盖编码成最后有效值
- 实现:`crates/hft-oms/tests/config_blob.rs::oms_465_config_section_roundtrip`

### OMS-466 OMS 配置不接受缺默认行或歧义覆盖

- 规则:[配置项](README.md#配置项omsconfig)
- 级别:unit
- 里程碑:M2
- 前置:完整 OMS 配置行
- 步骤:逐个删除 25 个默认行;分别注入重复键、填充、错误作用域、越界值、未知字段及未知 venue
- 期望:全部返回错误或 None,不以 OmsConfig::new 的默认值补洞
- 实现:`crates/hft-oms/tests/config_blob.rs::oms_466_config_section_rejects_incomplete`

### OMS-467 累计结算不受查询及同单交付排列影响

- 规则:[回报匹配与成交增量](README.md#回报匹配与成交增量)
- 级别:unit
- 里程碑:M2
- 前置:P0,VX 声明 PERP_CASH,一张卖 Y 0.03 的订单
- 步骤:三条累计量 0.01/0.02/0.03、累计现金 0/−11700/−11800 的 PartialFill/QueryResult/Fill 按全部六种排列交付;重复终态回报后再输入费用 123 的 FeeOnly
- 期望:每种排列成交输出总量 0.03、现金总额 −11800;查询跨越的成交量只记一次;明确零额保持 Some(0);重复无现金输出,晚到费用不改变水位
- 实现:`crates/hft-oms/tests/settlement.rs::oms_467_cash_cumulative_queries_and_permutations`

### OMS-468 无效结算事实不改变订单状态

- 规则:[回报匹配与成交增量](README.md#回报匹配与成交增量)
- 级别:unit
- 里程碑:M2
- 前置:严格结算订单已成交 0.01,累计结算 −11700
- 步骤:逐一注入缺失、部分覆盖、过量覆盖、负覆盖、零覆盖非零金额、同水位冲突、金额差溢出、错误 inst 和错误 epoch;冲突行同时携带非零费用
- 期望:返回对应 CashError;槽与快照逐字节不变,无成交/费用/对冲输出;错误使 OmsOut 非空,clear 后才清除
- 实现:`crates/hft-oms/tests/settlement.rs::oms_468_cash_bad_facts_leave_slot_unchanged`

### OMS-469 已知结算跨部分成交恢复、墓碑和槽复用

- 规则:[回报匹配与成交增量](README.md#回报匹配与成交增量)、[快照行格式](README.md#快照行格式节-2312草案实现时以代码与编译期断言为准)
- 级别:unit
- 里程碑:M2
- 前置:VX 一槽,卖 Y 0.02,已成交 0.01、结算 −11700
- 步骤:部分成交快照恢复并重送回报,撤单后回收并复用槽;复用后连续与恢复两条路径输入旧代累计 0.02、结算 −11800;再次恢复后重复与篡改该回报
- 期望:旧代只补 −100;新代数量和现金水位仍为零;不重开链接或产生订单;连续与恢复快照一致;重复不记账、同水位篡改明确失败
- 实现:`crates/hft-oms/tests/settlement.rs::oms_469_cash_tomb_reuse_and_restore`

### OMS-470 未知结算状态与未具备事实的收敛路径明确失败

- 规则:[回报匹配与成交增量](README.md#回报匹配与成交增量)、[快照行格式](README.md#快照行格式节-2312草案实现时以代码与编译期断言为准)
- 级别:unit
- 里程碑:M2
- 前置:严格模式的已成交槽或墓碑,累计结算明确为零
- 步骤:改写为节 2 v1/v2、节 12 v1 的旧格式,分别用严格与旧配置恢复;另对 Unknown 槽输入无现金事实的对账补量、无法匹配的外来回报和已有 external held 的重叠回报
- 期望:旧配置可恢复旧格式,严格配置拒绝丢失结算水位的状态;对账补量 Missing、未知身份 Untracked、缺少现金重叠水位 ExternalOverlap;不产生猜测的结算
- 实现:`crates/hft-oms/tests/settlement.rs::oms_470_cash_legacy_snapshots_and_unproven_paths`

### OMS-471 外来累计现金跨乱序和恢复去重

- 规则:[回报匹配与成交增量](README.md#回报匹配与成交增量)
- 级别:unit
- 里程碑:M2
- 前置:PERP_CASH,三个完整累计回报,现金 0/−11700/−11800
- 步骤:防御路径与 StaleGen 各三种排列,每条后快照恢复,最后重发全部回报
- 期望:毛量共 3000000、现金共 −11800;恢复字节相同;重复无入账,对应路径告警各一条且 v[2] 为零
- 实现:`crates/hft-oms/tests/settlement.rs::oms_471_foreign_cash_cursors_survive_restore_and_reordered_reports`

### OMS-472 错误外来现金与跨路径身份不改状态

- 规则:[回报匹配与成交增量](README.md#回报匹配与成交增量)
- 级别:unit
- 里程碑:M2
- 前置:PERP_CASH,已记录第一笔完整结算并恢复
- 输入:缺覆盖、超覆盖、同水位异额、溢出、错方向、零订单号、零价格、不支持的查询、已记外来后匹配普通槽、原始成交后转外来或普通槽、未知 FeeOnly 十二行
- 步骤:先记录一条外来结算并恢复,分别施加错误
- 期望:对应 Missing/Coverage/Conflict/Overflow/Identity/Untracked/ExternalOverlap;快照字节不变,无成交或费用输出
- 实现:`crates/hft-oms/tests/settlement.rs::oms_472_foreign_cash_bad_facts_and_cross_path_aliases_are_atomic`

### OMS-473 外来现金表满不淘汰水位

- 规则:[回报匹配与成交增量](README.md#回报匹配与成交增量)
- 级别:unit
- 里程碑:M2
- 前置:结算来源容量 1,已有明确零结算的订单
- 步骤:第二订单进入,再重发第一单;原始 ExternalFill 查询同号与新号
- 期望:第二单 Capacity 且快照不变;第一单不重复入账;原始来源分别报 ExternalOverlap/Capacity
- 实现:`crates/hft-oms/tests/settlement.rs::oms_473_foreign_cash_capacity_never_evicts_a_booked_identity`

### OMS-474 64 行结算来源查询预算

- 规则:[延迟与资源契约](README.md#延迟与资源契约)
- 级别:bench
- 里程碑:M2
- 基准名:`oms/foreign_cash_seen`、`oms/external_cash_guard`、`oms/remember_external_cash`
- 步骤:填满 64 行,查询最后一行的来源、核验与重复登记
- 期望:三项基准满足归属预算、运行期零分配;CI smoke 只报告
- 实现:`crates/hft-oms/benches/oms.rs::oms_474_foreign_cash_provenance_64`

### OMS-475 模拟重置清空本账户结算来源

- 规则:[使用本组件](README.md#使用本组件)
- 级别:unit
- 里程碑:M2
- 前置:实盘 A1 和模拟 S1 均启用 VX 的严格永续结算,各有同订单号的外来累计结算,以及另一订单的单笔原件证明
- 步骤:重置模拟账户,再向两账户交付相同回报
- 期望:模拟账户按新资金周期重新记账;另一账户保留水位且不重复入账;单笔证明只随所属模拟账户清除
- 实现:`crates/hft-oms/tests/settlement.rs::oms_475_sim_reset_clears_only_that_accounts_foreign_cash`

### OMS-476 对账整组来源容量预检

- 规则:[实现规格](README.md#回报匹配与成交增量)
- 级别:unit
- 里程碑:M2
- 前置:来源容量为 2,已有 1 行
- 步骤:预检两笔同订单、两个不同新订单、现有订单与新订单三组凭据;比较每组前后的快照
- 期望:同一订单只占一行,两个新订单报 Capacity;全部预检不改变快照
- 实现:`crates/hft-oms/tests/settlement.rs::oms_476_reconcile_cash_capacity_counts_distinct_order_identities`

### OMS-477 单笔原件在恢复后证明累计重复

- 规则:[回报匹配与成交增量](README.md#回报匹配与成交增量)
- 级别:unit
- 里程碑:M2
- 前置:严格结算、已实际完整入账的 Unknown 原件与 ext_held
- 步骤:防御/StaleGen 两路径 × 正/零 ID × 正/零/负现金 × 正/零/负费用共 36 组,每组保存并两次恢复后重放同笔累计回报
- 期望:节 12 v4 保留证明;无成交或费用输出、快照字节不变;每条累计重复仍有一条对应路径告警且 v[2] 为零,无入账或 ExtDedup 注记;原件预检证明已入账
- 实现:`crates/hft-oms/tests/settlement.rs::oms_477_original_receipt_proves_only_the_same_first_execution_after_restore`

### OMS-478 不完整或错误原件不能放行跨来源重叠

- 规则:[回报匹配与成交增量](README.md#回报匹配与成交增量)
- 级别:unit
- 里程碑:M2
- 前置:单笔原件,每个反例独立恢复
- 步骤:改变成交 ID、委托量、累计量、现金覆盖/金额、费用/资产、方向、价格、报告类型、代次;另设未实际入账与不同后续成交两行;原件同键异额另行预检
- 期望:13 行全部明确报错且无账务输出、快照不变;后续成交清除证明后,恢复和重复原件均不能重新建立证明
- 实现:`crates/hft-oms/tests/settlement.rs::oms_478_original_receipt_never_authorizes_wrong_or_missing_facts`

### OMS-479 原件快照校验与旧格式恢复

- 规则:[快照行格式](README.md#快照行格式节-2312草案实现时以代码与编译期断言为准)
- 级别:unit
- 里程碑:M2
- 前置:含单笔证明的 v4 快照
- 步骤:13 行篡改方向、身份、数量、价格、规范键、填充、资产、重复行、来源或版本;另删去证明生成合法 v3 快照
- 期望:损坏快照全部拒绝;v3 可读但不能证明跨来源重复,后续未入账的重复原件不能补造证明
- 实现:`crates/hft-oms/tests/settlement.rs::oms_479_receipt_restore_rejects_corruption_and_legacy_has_no_proof`

### OMS-480 成交输出保留价格取整前的名义

- 规则:[回报匹配与成交增量](README.md#回报匹配与成交增量)
- 级别:unit
- 里程碑:M2
- 前置:Live 普通槽,尚未成交且未受外来成交抵扣
- 步骤:先累计 0.004 BTC、均价 60000,再累计 0.01 BTC、均价 60001;独立新槽再交付 avg_px 为零的成交
- 期望:第二笔保留原始增量名义 360.01 USDT,与已取整 px × qty 不同;无均价时保留 ev.px × d;数量与价格既有语义不变
- 实现:`crates/hft-oms/tests/fsm_table.rs::oms_480_fill_preserves_original_notional_before_increment_price_rounding`

### OMS-481 回报 inst 为 0 视为未提供,非严格现金路径的品种不符只告警

- 规则:[回报匹配与成交增量](README.md#回报匹配与成交增量)、[Note 码表](README.md#note-码表)
- 级别:table
- 里程碑:M2
- 前置:普通现货槽在 VS(inst X,非 PERP_CASH);另一夹具令 VX 声明 PERP_CASH,永续槽 inst Y;墓碑夹具:VS 槽 3 的第 2 代已全部成交 0.01 后回收
- 实现:`crates/hft-oms/tests/tomb.rs::oms_481_report_inst_none_is_unsupplied_and_mismatch_is_not_fatal_off_cash`

| 行 | 回报 | 期望 |
|---|---|---|
| OMS-481.1 | 普通槽 PendingNew,`Reject{inst = 0}` | 无 `cash_error`;槽 Rejected |
| OMS-481.2 | 普通槽 PendingCancel,`CancelReject{inst = 0}` | 无 `cash_error`;槽 Live |
| OMS-481.3 | 普通槽 PendingNew,`Reject{inst = Y}` | 无 `cash_error`;槽仍 PendingNew;恰好一条 `OmsAnomaly`,级别 P0,`Note.inst = X`,`v[0] = 5`,`v[1] = Y` |
| OMS-481.4 | 严格现金槽 PendingNew,`Reject{inst = 0}` | 无 `cash_error`;槽 Rejected |
| OMS-481.5 | 严格现金槽 PendingNew,`Reject{inst = X}` | `cash_error = Identity`;槽仍 PendingNew |
| OMS-481.6 | 墓碑,迟到 `Fill{inst = 0, cum = 0.011}` | 无 `cash_error`;一条 `FillOut{inst = X, qty = 0.001}` |
| OMS-481.7 | 墓碑,迟到 `Fill{inst = Y, cum = 0.011}` | 无 `cash_error`;无 `FillOut`;一条 `OmsAnomaly`;墓碑 `filled` 仍 0.01 |

### OMS-482 非成交回报带来的费用只入账一次

- 规则:[回报匹配与成交增量](README.md#回报匹配与成交增量)
- 级别:table
- 前置:VS 现货槽 0.01 BTC 买单已 Ack,已发撤单(PendingCancel)
- 里程碑:M2
- 实现:`crates/hft-oms/tests/fsm.rs::oms_482_fee_carried_by_non_fill_report_is_booked_once`

| 行 | 步骤 | 期望 |
|---|---|---|
| OMS-482.1 | `CancelAck{cum = 0.004, fee = 1000}`,再两次 `PartialFill{cum = 0.004, fee = 1000}` | CancelAck 产出一条 `FillOut{fee = 1000}`,`fee_cum = 0.004`;两条 PartialFill 都无 `FillOut`、无 `FeeOut` |
| OMS-482.2 | `CancelAck{cum = 0.004, fee = 0}`,再两次 `PartialFill{cum = 0.004, fee = 1000}` | CancelAck 的 `FillOut.fee = 0`,`fee_cum` 仍 0;第一条 PartialFill 恰好一条 `FeeOut{fee = 1000}`,`fee_cum = 0.004`;第二条无输出 |

### OMS-483 外来现金来源只接受节 12 恢复能接受的行

- 规则:[回报匹配与成交增量](README.md#回报匹配与成交增量)
- 级别:unit
- 里程碑:M2
- 前置:VX 声明 PERP_CASH,原始 `ExternalFill{venue = VX, inst = Y, exch_order_id = 9901}`
- 步骤:
  1. 对原件调用 `external_cash_guard`
  2. 把 inst 改为 VS 的现货 X,分别调用 `external_cash_guard` 与 `remember_external_cash(booked = true)`
  3. 把 venue 改为 VS(inst 仍为 Y),同第 2 步
  4. 保留原件来源后快照并恢复
- 期望:
  - 第 1 步 `Ok(false)`
  - 第 2、3 步两次调用都返回 `Err(Identity)`;快照字节不变且可恢复
  - 第 4 步快照与恢复后重写的字节相同
- 实现:`crates/hft-oms/tests/settlement.rs::oms_483_external_cash_guard_rejects_rows_the_restore_would_refuse`

### OMS-484 配置段的行宽必须是整行

- 规则:[配置项(`OmsConfig`)](README.md#配置项omsconfig)
- 级别:table
- 里程碑:M2
- 前置:`OmsConfig::new` 的默认行编码为 N 行 × 16 B
- 实现:`crates/hft-oms/tests/config_blob.rs::oms_484_config_section_stride_must_be_whole_rows`

| 行 | 段的行宽 × 行数 | 期望 |
|---|---|---|
| OMS-484.1 | 16 × N | 加载成功 |
| OMS-484.2 | 8 × 2N(同一字节) | 拒绝(容器或 `from_engine`) |
| OMS-484.3 | 32 × ⌊N/2⌋ | 拒绝 |

## 验收

`testkit` 对冲请求原始事实与实际决策的跨组件观察验收归 [TK-107](../hft-testkit/TESTS.md#tk-107-真实引擎逐事件观察与反例)。

- **完成标准**:M2 里程碑的全部 `unit`/`table`/`prop`/`golden`/`integ`/`replay`/`bench`/`dst` 用例有同编号的实现并通过(`cargo xtask test-trace` 无未实现、多余或不一致项,规则见 [docs/tests §6](../../docs/tests/README.md#6-与代码的对应));`testnet`/`host` 用例(OMS-420、OMS-421)在 M6 按 `证据` 字段归档。
- **属性测试**:各 `prop` 用例 PR 例数见其 `例数` 字段;每夜每个 `prop` 用例 100 000 例,发布前累计 1 000 000 例无失败;失败的最小化种子按 [testing 金字塔](../../docs/testing.md#2-金字塔) 的规则写进本 crate 的 `tests/regressions/` 永久保留。
- **DST**:OMS-400–OMS-405 按 [testing §8 上线门槛](../../docs/testing.md#8-上线门槛) ④ 的种子累计零违反,OMS-401 为必过项。
- **基准**:OMS-440–OMS-455 的上限只在 [benchmarks](../../benchmarks/README.md) 登记(与 README"延迟与资源契约"表的 p50/p99 一致),参考机判定。
- **联调与长跑**:Unknown p99 收敛时间与零槽泄漏的数值门槛写在 OMS-420、OMS-421 的 `通过标准`;正式实盘后同一指标由 watchtower 持续监控。

### OMS-485 槽回收后到达的成交回报仍补记不带费用的回报所欠的手续费

- 规则:[回报匹配与成交增量](README.md#回报匹配与成交增量)、[墓碑与迟到成交](README.md#墓碑与迟到成交c1)
- 级别:unit
- 里程碑:M2
- 前置:P0;VS 现货买单 0.01 已 Ack,`QueryResult{Filled, cum = 0.01, fee = 0}` 使其 Filled(`fee_cum = 0`),sweep 回收为墓碑
- 步骤:
  1. 快照并恢复
  2. 投递原始 `Fill{cum = 0.01, fee = 600 000, fee_asset = USDT}`,再投递一次
- 期望:
  - 墓碑 `filled = 0.01`、`fee_cum = 0`;快照节 12 为 v5,恢复后 `fee_cum` 仍为 0
  - 第一条 Fill:无 `FillOut`,恰好一条 `FeeOut{fee = 600 000, USDT}`,墓碑 `fee_cum = 0.01`;第二条无输出
- 实现:`crates/hft-oms/tests/tomb.rs::oms_485_late_fill_fee_after_feeless_query_on_tomb`
