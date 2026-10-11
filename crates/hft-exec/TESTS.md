---
component: hft-exec
prefix: EXE
spec: README.md
---

# hft-exec 测试规格

格式与规则见 [docs/tests](../../docs/tests/README.md)。

## 覆盖

| README 小节 | 用例 |
|---|---|
| 概要 | 不适用:非规范 |
| 目录 | 不适用:非规范 |
| 使用本组件 | 不适用:见子节 |
| 入口(全部在 sequencer 线程调用,无分配) | EXE-506、EXE-507、EXE-002、EXE-003、EXE-010、EXE-011、EXE-012、EXE-013、EXE-014、EXE-015、EXE-016、EXE-017、EXE-231、EXE-317、EXE-318、EXE-396、EXE-405、EXE-475 |
| 输出 `ExecOut`(容量固定,sequencer 预分配;**每次调用本 crate 之后排空**) | EXE-020、EXE-021、EXE-022、EXE-023、EXE-024、EXE-316、EXE-459 |
| 执行规格参数(类型定义在 SDK,本节定义语义) | EXE-040、EXE-041、EXE-042 |
| 配置项(`ExecConfig`,`EngineConfig` blob 的 `exec` 段;每账户一行 + 全局默认行;变更 = Drain + 重启) | EXE-001、EXE-003、EXE-004、EXE-514、EXE-515 |
| 消费与产生的事件 | EXE-014、EXE-068、EXE-205、EXE-220、EXE-283、EXE-290、EXE-316、EXE-340、EXE-416、EXE-434、EXE-440 |
| 调用顺序(sequencer 同一输入事件内;流程归属 [hft-sequencer](../hft-sequencer/README.md)) | EXE-025、EXE-026、EXE-027 |
| 实现说明 | 不适用:见子节 |
| 目标单元(`TargetCell`) | EXE-002、EXE-010、EXE-014、EXE-050、EXE-051、EXE-052 |
| 状态机 | EXE-060、EXE-061、EXE-063、EXE-064、EXE-065、EXE-066、EXE-067、EXE-068、EXE-069、EXE-070、EXE-071、EXE-072、EXE-073、EXE-074、EXE-075、EXE-076、EXE-262、EXE-369、EXE-382 |
| 推进(`advance` 中每个 cell,O(n_legs) + 每腿 ≤ `depth_levels` 档) | EXE-061、EXE-062、EXE-080、EXE-081、EXE-082、EXE-083、EXE-084、EXE-085、EXE-086、EXE-087、EXE-088、EXE-089、EXE-090、EXE-091、EXE-100、EXE-501 |
| 不交易带与紧急度 | EXE-508、EXE-082、EXE-100、EXE-101、EXE-102、EXE-103、EXE-104、EXE-105、EXE-510、EXE-106 |
| 增仓与减仓的分类 | EXE-110、EXE-111、EXE-112、EXE-368 |
| 报价可用(连接健康) | EXE-115、EXE-116、EXE-117、EXE-118、EXE-119、EXE-120、EXE-168 |
| 限价与单笔名义 | EXE-125、EXE-126、EXE-127、EXE-128、EXE-129、EXE-130、EXE-131、EXE-132、EXE-133、EXE-242、EXE-492、EXE-503 |
| TakerSlice | EXE-111、EXE-118、EXE-145、EXE-146、EXE-147、EXE-148 |
| PairOnFill | EXE-041、EXE-068、EXE-089、EXE-112、EXE-155、EXE-156、EXE-157、EXE-158、EXE-159、EXE-160、EXE-161、EXE-511、EXE-162、EXE-163、EXE-164、EXE-165、EXE-166、EXE-167、EXE-168、EXE-169、EXE-500 |
| 两腿 lot 对齐 | EXE-180、EXE-181、EXE-182、EXE-183、EXE-184、EXE-185、EXE-186、EXE-187、EXE-188、EXE-494、EXE-500 |
| Simultaneous | EXE-195、EXE-502 |
| MakerFirst(optional) | EXE-196 |
| 对冲量上限(B6) | EXE-070、EXE-161、EXE-511、EXE-200、EXE-201、EXE-254 |
| 补腿(`HedgeFix`,C4) | EXE-119、EXE-169、EXE-205、EXE-206、EXE-207、EXE-208、EXE-209、EXE-210、EXE-211、EXE-212、EXE-213、EXE-214、EXE-215、EXE-216、EXE-220、EXE-221、EXE-222、EXE-296、EXE-505 |
| HARD 解除时的 Broken 判定(`resolve_broken`) | EXE-208、EXE-230、EXE-231、EXE-232 |
| Broken 处置 | EXE-025、EXE-069、EXE-222、EXE-237、EXE-238、EXE-239、EXE-240、EXE-241、EXE-242、EXE-243、EXE-244、EXE-245、EXE-246、EXE-247、EXE-248、EXE-249、EXE-250、EXE-251、EXE-252、EXE-253、EXE-254、EXE-255、EXE-256、EXE-257、EXE-258、EXE-259、EXE-260、EXE-261、EXE-262、EXE-326、EXE-358、EXE-433、EXE-463、EXE-505 |
| 残余与尘埃 | EXE-083、EXE-185、EXE-211、EXE-280、EXE-281、EXE-282、EXE-283 |
| 外来成交的默认处理(B3) | EXE-290、EXE-291、EXE-292、EXE-293、EXE-294、EXE-295、EXE-512、EXE-296、EXE-297、EXE-298、EXE-299、EXE-300、EXE-419 |
| 人工保持与恢复期抑制 | EXE-220、EXE-247、EXE-248、EXE-249、EXE-291、EXE-297、EXE-310、EXE-311、EXE-312、EXE-313、EXE-314、EXE-315、EXE-316、EXE-317、EXE-318、EXE-319、EXE-325、EXE-326、EXE-327、EXE-328、EXE-329、EXE-344、EXE-405 |
| 保证金危急的默认减仓(A5)与 `engine_flatten` | EXE-023、EXE-052、EXE-072、EXE-260、EXE-292、EXE-300、EXE-340、EXE-341、EXE-342、EXE-343、EXE-344、EXE-345、EXE-346、EXE-347、EXE-348、EXE-349、EXE-350、EXE-351、EXE-352、EXE-353、EXE-354、EXE-355、EXE-356、EXE-357、EXE-358、EXE-359、EXE-360、EXE-361、EXE-362、EXE-363、EXE-364、EXE-365、EXE-366、EXE-367、EXE-368、EXE-369、EXE-420、EXE-503 |
| 门控拒绝与退避 | EXE-064、EXE-132、EXE-166、EXE-212、EXE-256、EXE-380、EXE-381、EXE-382、EXE-383、EXE-384、EXE-385、EXE-504 |
| 运行模式、Drain 与 Unknown | EXE-506、EXE-085、EXE-232、EXE-390、EXE-391、EXE-392、EXE-393、EXE-394、EXE-395、EXE-396、EXE-397、EXE-398、EXE-504 |
| 目标的接受与替换 | EXE-507、EXE-013、EXE-042、EXE-063、EXE-067、EXE-073、EXE-155、EXE-183、EXE-196、EXE-359、EXE-364、EXE-365、EXE-400、EXE-401、EXE-402、EXE-403、EXE-404 |
| `ExecDecision` 与进度通知 | EXE-015、EXE-354、EXE-415、EXE-416、EXE-417、EXE-418、EXE-419、EXE-420、EXE-462、EXE-464 |
| `ExecDecision.detail` 打包与子码 | EXE-430、EXE-431、EXE-432、EXE-433 |
| 定时器 | EXE-027、EXE-328、EXE-440、EXE-441、EXE-442、EXE-443、EXE-444、EXE-458 |
| 快照与恢复 | EXE-509、EXE-164、EXE-186、EXE-251、EXE-314、EXE-319、EXE-329、EXE-450、EXE-451、EXE-452、EXE-453、EXE-454、EXE-455、EXE-456、EXE-457、EXE-458、EXE-459、EXE-460、EXE-461、EXE-462、EXE-463、EXE-464、EXE-493 |
| 规划源文件 | EXE-133、EXE-184、EXE-187、EXE-434 |
| 延迟与资源契约 | EXE-051、EXE-475、EXE-477、EXE-480、EXE-481、EXE-482、EXE-483、EXE-484、EXE-485、EXE-486、EXE-487、EXE-488、EXE-489、EXE-490、EXE-491、EXE-492、EXE-493、EXE-494 |
| 测试与验收 | 不适用:非规范(README 中只保留指向本文件的链接) |
| 依赖 | EXE-476、EXE-477 |
| 延伸阅读 | 不适用:非规范 |
| 已知限制与后续工作 | 不适用:非规范 |
| 开发备忘 | 不适用:非规范 |

## 用例

**基准夹具**(`crates/hft-exec/tests/common/mod.rs`;mock 的 `Views`、OMS 回报与风控结果由测试直接构造,不链接 hft-oms/hft-risk/hft-ledger)。用例写"夹具 + 差异";用例依赖的参数在 `前置` 里再写一次具体值。

- **时钟**:TSC 1 GHz(1 tick = 1 ns,`tsc_hz_milli = 1 000 000 000 000`);`t = x ms` 表示 `now = x × 10⁶` tick。
- **账户与实例**:a1 = `AccountId(1)`(Live)、a2 = `AccountId(2)`(Live)、s1 = `AccountId(3)`(Sim);实例 i1 = `StrategyInstanceId(1)` 绑定 a1,i2 绑定 a2,i3 绑定 s1。
- **两腿 group g7** = `GroupId(0x0100_0007)`(i1,a1):腿 0 = 现货 S(venue A = `VenueId(1)`,`InstId(11)`),腿 1 = 永续 P(venue B = `VenueId(2)`,`InstId(21)`);同一基础资产,`qty_exp = −8`。
- **S**:lot 0.01、`min_qty` 0.01、`min_notional` 5 USDT、tick 0.01;BBO 99.90 / 100.10(各 40);L2 `Live`,每侧 5 档、档距 0.10、每档 40;该 venue 不支持 reduceOnly。
- **P**:lot 0.1、`min_qty` 0.1、`min_notional` 5 USDT、tick 0.01;BBO 99.80 / 100.20(各 50);L2 `Live`,每侧 5 档、档距 0.10、每档 50;`VenueCaps.reduce_only = true`。
- **价格**:两腿 mark 100.00;计价 USDT,`quote_rate = 1`;两腿 `conn_up`、`resync = false`、`conn_live_tsc = now`;`venue_ready = true`。
- **风控视图 R0**:`hard = false`、MarginStress L0、无 cap、`conn_liveness_ms = 2000`、`max_order_notional = max_reduce_notional = max_hedge_notional = 2000 USDT`;`risk_for`:`rr_max_cross = 0.01`、`ioc_max_cross = 0.005`、`l3_max_cross = 0`、`rate_backoff = false`。模式 Live、`open_allowed = true`;a1 已 `on_gate_open`(全部 venue 放行)。
- **参数集 C0**(测试里逐字段写出,不调 `ExecConfig::default()`):`band_floor_notional = 20 USDT`、`dust_notional = 5 USDT`、`fix_tolerance_default = 0.02`、`fix_max = 3`、`fix_cooldown_ms = 1000`、`fix_deadline_ms = 10 000`、`broken_auto_fix_ms = 120 000`、`external_deadline_ms = 5000`、`gate_naked_alarm_ms = 300 000`、`stuck_after_ms = 2000`、`gated_backoff_ms = 500`、`max_slice_fails = 3`、`max_nofill = 5`、`urgent_cooldown_ms = 100`、`passive_cooldown_mult = 4`、`force_slice = {depth_frac 0.3, depth_levels 5, max_cross 0.005, max_chunk_notional 1000 USDT, min_chunk_notional 0, cooldown_ms 200}`。
- **目标规格 T0**(g7):`PairOnFill{slice = {depth_frac 0.5, depth_levels 5, max_cross 0.005, max_chunk_notional 1000 USDT, min_chunk_notional 10 USDT, cooldown_ms 300}, first_leg = ByLiquidity, ratio 1/1, fix_tolerance 0.02, residual = TrimExcess, hedge_px_mode/hedge_cross/hedge_max_slip = Cross/0.003/0.004}`、`reason = Rebalance`、`urgency = Normal`、`band = 0.01`、`tag = 0x10`。g7 的 `pair_lot` = 0.1、`pair_min` = 0.1。
- **单腿 group g8** = `GroupId(0x0100_0008)`(i1,a1):腿 0 = P;目标规格 T1 = `TakerSlice`(参数同 T0 的 `slice`)。
- **记法**:`ExecDecision{动作, 字段 = 值}` 只列被断言的字段;"无 X"指该步骤调用返回后 `ExecOut` 中没有 X;"Note{码}" 断言 `source = Exec`、码名与实际级别;`v[]` 只在列出时断言。

### EXE-001 ExecConfig 各字段的默认值等于配置表

- 规则:[配置项](README.md#配置项execconfigengineconfig-blob-的-exec-段每账户一行--全局默认行变更--drain--重启)
- 级别:table
- 里程碑:M2
- 前置:`ExecConfig::default()` 的全局默认行(本文件唯一读取默认值的用例)
- 实现:`crates/hft-exec/tests/config.rs::exe_001_defaults`

| 行 | 字段 | 期望 |
|---|---|---|
| EXE-001.1 | `band_floor_notional` | 20 USDT(`Notional(20_000_000)`) |
| EXE-001.2 | `dust_notional` | 5 USDT(`Notional(5_000_000)`) |
| EXE-001.3 | `fix_tolerance_default` | 0.02(`Rate(20_000_000)`) |
| EXE-001.4 | `fix_max` | 3 |
| EXE-001.5 | `fix_cooldown_ms` | 1000 |
| EXE-001.6 | `fix_deadline_ms` | 10 000 |
| EXE-001.7 | `broken_auto_fix_ms` | 120 000 |
| EXE-001.8 | `external_deadline_ms` | 5000 |
| EXE-001.9 | `gate_naked_alarm_ms` | 300 000 |
| EXE-001.10 | `stuck_after_ms` | 2000 |
| EXE-001.11 | `gated_backoff_ms` | 500 |
| EXE-001.12 | `max_slice_fails` | 3 |
| EXE-001.13 | `max_nofill` | 5 |
| EXE-001.14 | `urgent_cooldown_ms` | 100 |
| EXE-001.15 | `passive_cooldown_mult` | 4 |
| EXE-001.16 | `force_slice` | `depth_frac = Rate(300_000_000)`、`depth_levels = 5`、`max_cross = Rate(5_000_000)`、`max_chunk_notional = Notional(1_000_000_000)`、`min_chunk_notional = Notional(0)`、`cooldown_ms = 200` |

### EXE-002 new 预分配全部 cell,全部账户闸门关闭

- 规则:[入口](README.md#入口全部在-sequencer-线程调用无分配)、[目标单元](README.md#目标单元targetcell)
- 级别:unit
- 里程碑:M2
- 前置:C0;`Capacities.max_cells_total = 256`;账户 a1、a2、s1;计数分配器
- 步骤:
  1. `Exec::new(&cfg, &cap)`
  2. `snapshot(&mut w)`
- 期望:
  - 步骤 1 返回 `Ok`;cell 表长度 = 259(256 + 3 个无组 cell);`has_pending() = false`;`drain_ready() = true`
  - 步骤 2 的节 21 恰好 3 条 Gate 行(a1、a2、s1 升序),每行 `gate_open = 0`、`venue_open = 0`、`gate_closed_tsc = 0`;无 Cell 行、无 Acct 行
  - 步骤 1 之后的任何入口调用(EXE-475 的序列)分配计数为 0
- 实现:`crates/hft-exec/tests/config.rs::exe_002_new_preallocates`

### EXE-003 new 拒绝越界的 ExecConfig

- 规则:[入口](README.md#入口全部在-sequencer-线程调用无分配)、[配置项](README.md#配置项execconfigengineconfig-blob-的-exec-段每账户一行--全局默认行变更--drain--重启)
- 级别:table
- 里程碑:M2
- 前置:C0,逐行只改所列字段;`max_cells_total = 256`、3 个账户(除 .12–.13)
- 实现:`crates/hft-exec/tests/config.rs::exe_003_new_rejects_out_of_range`

| 行 | 改动 | 期望 |
|---|---|---|
| EXE-003.1 | `force_slice.depth_levels = 0` | `Err(ExecConfigError::ForceSlice)` |
| EXE-003.2 | `force_slice.depth_levels = 1` | `Ok` |
| EXE-003.3 | `force_slice.depth_levels = 10` | `Ok` |
| EXE-003.4 | `force_slice.depth_levels = 11` | `Err(ForceSlice)` |
| EXE-003.5 | `force_slice.depth_frac = Rate(0)` | `Err(ForceSlice)` |
| EXE-003.6 | `force_slice.depth_frac = Rate(1_000_000_000)` | `Ok` |
| EXE-003.7 | `force_slice.depth_frac = Rate(1_000_000_001)` | `Err(ForceSlice)` |
| EXE-003.8 | `fix_max = 0` | `Err(ZeroLimit)` |
| EXE-003.9 | `max_slice_fails = 0` | `Err(ZeroLimit)` |
| EXE-003.10 | `max_nofill = 0` | `Err(ZeroLimit)` |
| EXE-003.11 | `passive_cooldown_mult = 0` | `Err(ZeroLimit)` |
| EXE-003.12 | `max_cells_total = 65 532`,3 个账户(合计 65 535) | `Ok` |
| EXE-003.13 | `max_cells_total = 65 533`,3 个账户(合计 65 536) | `Err(Capacity)` |

### EXE-004 账户行覆盖全局默认行,无行的账户用默认行

- 规则:[配置项](README.md#配置项execconfigengineconfig-blob-的-exec-段每账户一行--全局默认行变更--drain--重启)
- 级别:unit
- 里程碑:M2
- 前置:C0 作全局默认行(`dust_notional = 5 USDT`);a2 另有一行 `dust_notional = 10 USDT`,a1 无行;a2 有与 g7 同构的 g9 = `GroupId(0x0200_0009)`(i2);g7 与 g9 都无目标、无链接、静止,腿净量 S +1.08、P −1.00(裸量 0.08,名义 8 USDT,裸量比 0.074 > `tol` 0.02)
- 步骤:
  1. `on_leg_adjust(a1, g7)`、`on_leg_adjust(a2, g9)`;`advance`
- 期望:
  - g7 恰好一条 `ExecOrder{kind = HedgeFix, side = Sell, inst = S, qty = 0.08}`
  - g9 无订单、无 `ExecDecision`
- 实现:`crates/hft-exec/tests/config.rs::exe_004_account_row_overrides_default`

### EXE-010 on_groups_changed 按 (account, group) 升序分配 cix,无组 cell 排在其后

- 规则:[入口](README.md#入口全部在-sequencer-线程调用无分配)、[目标单元](README.md#目标单元targetcell)
- 级别:unit
- 里程碑:M2
- 前置:C0;GroupTable 依次给出 (a2, g9)、(a1, g8)、(a1, g7);账户 a1、a2、s1
- 步骤:
  1. `on_groups_changed`;对三个 group 各设一个带外的 `TakerSlice` 目标并 `advance`
  2. `force_deleverage(a1, B, 0.2)`,a1 另有无组永续持仓 P −3.0;`advance`
- 期望:
  - 步骤 1 各订单的 `ExecOrder.id.cix`:(a1, g7) = 0、(a1, g8) = 1、(a2, g9) = 2;`ExecDecision` 的写出顺序也是 g7、g8、g9
  - 步骤 2 无组 cell 的订单 `id.cix = 3`(a1 的无组 cell),`ExecDecision.group = GROUP_NONE`
- 实现:`crates/hft-exec/tests/entry.rs::exe_010_cix_order`

### EXE-011 on_groups_changed 保留同 (account, group) 的状态

- 规则:[入口](README.md#入口全部在-sequencer-线程调用无分配)
- 级别:unit
- 里程碑:M2
- 前置:C0、T0;g7 目标 S +10 / P −10 进行中:`filled = [+2.0, −2.0]`、`fix.count = 1`、`fail_streak = 1`
- 步骤:
  1. 快照 → 字节串 X
  2. `on_groups_changed`(新表在 g7 之前插入 (a1, `GroupId(0x0100_0006)`),g7 腿定义不变)
  3. 快照 → 字节串 Y
- 期望:
  - 步骤 2 后 `ExecOut` 为空;`view(a1, g7)` 的 `intent`、`filled`、`state` 与步骤 1 前相同
  - Y 中 g7 的 Cell 行与 X 中逐字节相同
- 实现:`crates/hft-exec/tests/entry.rs::exe_011_groups_changed_keeps_state`

### EXE-012 腿定义变化的 cell 清除目标并写 TargetCleared

- 规则:[入口](README.md#入口全部在-sequencer-线程调用无分配)
- 级别:unit
- 里程碑:M2
- 前置:C0、T0;g7 目标进行中、无在途
- 步骤:
  1. `on_groups_changed`(g7 的腿 1 改为 `InstId(22)`)
  2. `advance`
- 期望:
  - 步骤 1 后 `ExecOut` 全空,`has_pending() = true`
  - 步骤 2 写 `ExecDecision{TargetCleared, group = g7}` 一条;`view(a1, g7).state = Idle`、无目标;无订单
- 实现:`crates/hft-exec/tests/entry.rs::exe_012_leg_change_clears_target`

### EXE-013 被移除的 group 不再有 cell,对它设目标按 NotOwner 拒绝

- 规则:[入口](README.md#入口全部在-sequencer-线程调用无分配)、[目标的接受与替换](README.md#目标的接受与替换)
- 级别:unit
- 里程碑:M2
- 前置:C0;g8 有目标
- 步骤:
  1. `on_groups_changed`(新表不含 g8)
  2. i1 `set_target`(g8 的 T1 目标)
- 期望:
  - 步骤 1 后 `view(a1, g8) = None`
  - 步骤 2 返回 `Err(TargetReject::NotOwner)`,写 `ExecDecision{TargetRejected, detail = 1 | 1 << 8}`
- 实现:`crates/hft-exec/tests/entry.rs::exe_013_removed_group_not_owner`

### EXE-014 touch_inst 只标记活动 cell

- 规则:[入口](README.md#入口全部在-sequencer-线程调用无分配)、[目标单元](README.md#目标单元targetcell)
- 级别:unit
- 里程碑:M2
- 前置:C0;g7 Idle、无目标、`FixState`/`ext_deadline`/`auto_fix` 全零;g8 目标 P −10 进行中(Working)
- 步骤:
  1. `touch_inst(S)`(只有 g7 引用 S)
  2. `touch_inst(P)`
  3. `advance`
- 期望:
  - 步骤 1 后 `has_pending() = false`
  - 步骤 2 后 `has_pending() = true`
  - 步骤 3 的 `ExecDecision` 全部 `group = g8`
- 实现:`crates/hft-exec/tests/entry.rs::exe_014_touch_inst_live_only`

### EXE-015 view 返回目标单元的投影

- 规则:[入口](README.md#入口全部在-sequencer-线程调用无分配)、[`ExecDecision` 与进度通知](README.md#execdecision-与进度通知)
- 级别:unit
- 里程碑:M2
- 前置:C0、T1;g8 目标 P −10.0 在 seq 100 被接受;第一片 −10.0 成交 −4.0 后终态
- 步骤:
  1. `view(a1, g8)`;`view(a1, GroupId(0x0100_0099))`
- 期望:
  - 第一次返回 `Some`:`intent` 与设定的目标逐字段相同、`origin = Strategy`、`state = Working`、`set_seq = 100`、`n_slices = 1`、`filled[0] = −4.0`
  - 第二次返回 `None`
- 实现:`crates/hft-exec/tests/entry.rs::exe_015_view_projection`

### EXE-016 has_pending 在 advance 处理完全部待推进 cell 后为假

- 规则:[入口](README.md#入口全部在-sequencer-线程调用无分配)
- 级别:unit
- 里程碑:M2
- 前置:C0;g7、g8 各有带外目标
- 步骤:
  1. `set_target`(g7)、`set_target`(g8)
  2. `advance`(输出容量充足)
- 期望:
  - 步骤 1 后 `has_pending() = true`;步骤 2 后 `has_pending() = false`
- 实现:`crates/hft-exec/tests/entry.rs::exe_016_has_pending_cleared`

### EXE-017 advance 先推进实盘账户的 cell,再推进 Sim 账户的 cell

- 规则:[入口](README.md#入口全部在-sequencer-线程调用无分配)
- 级别:unit
- 里程碑:M2
- 前置:C0;本用例账户 `AccountId(1)` 为 Sim、`AccountId(2)` 为 Live,各有一个带外的 `TakerSlice` 目标
- 步骤:
  1. 两个 `set_target`;`advance`
- 期望:
  - `ExecDecision` 与 `ExecOut.orders` 中 `AccountId(2)` 的条目全部排在 `AccountId(1)` 之前
- 实现:`crates/hft-exec/tests/entry.rs::exe_017_live_before_sim`

### EXE-020 单 cell 入口的单次产出不超过上限

- 规则:[输出 `ExecOut`](README.md#输出-execout容量固定sequencer-预分配每次调用本-crate-之后排空)
- 级别:prop
- 里程碑:M2
- 性质:从任意可达状态调用一次单 cell 入口(`set_target`、`clear_target`、`on_external`、`hedge_fix_cmd`、`on_place_result`、`on_order_update`、`on_link`、`on_timer`、`on_mode`、`note_strategy_action`)后,`ExecOut` 各向量长度 ≤ decisions 2、notify 1、orders 1、cancels 4、cancel_resting 0、broken 1、timers 2、notes 2
- 生成:1–8 个 cell;每 cell 0–2 条腿在途;状态从 {Idle, Working, Stuck(每种原因), Broken(每个 `AutoFixPhase`)} 抽取;`hold` 位、`serial`、`force_active`/`flatten_lock` 随机;入口与参数随机
- 参考:断言上述上限
- 例数:PR 2 000
- 实现:`crates/hft-exec/tests/props.rs::exe_020_single_cell_entry_caps`

### EXE-021 多 cell 登记入口不直接写输出

- 规则:[输出 `ExecOut`](README.md#输出-execout容量固定sequencer-预分配每次调用本-crate-之后排空)
- 级别:table
- 里程碑:M2
- 前置:C0;a1 有 64 个两腿 cell(同 g7 结构,各自目标进行中、永续腿有仓位),其中 2 个 Broken;a1 已 `recovery_end`(未放行)用于 .5–.6
- 实现:`crates/hft-exec/tests/output.rs::exe_021_multi_cell_entries_write_nothing`

| 行 | 调用 | 期望 |
|---|---|---|
| EXE-021.1 | `engine_flatten(a1)` | 调用后 `ExecOut` 8 个向量长度全为 0;`has_pending() = true` |
| EXE-021.2 | `force_deleverage(a1, B, 0.2)` | 同上 |
| EXE-021.3 | `clear_account(a1)` | 同上 |
| EXE-021.4 | `on_risk(a1, CapSet{Hard})` | 同上 |
| EXE-021.5 | `on_gate_open(a1)` | 同上 |
| EXE-021.6 | `on_venue_gate_open(a1, B)` | 同上 |
| EXE-021.7 | `on_groups_changed`(64 个 cell 中 10 个腿定义改变) | 同上 |
| EXE-021.8 | 每行之后用容量无限的 `ExecOut` 调 `advance` | 写出该入口对应的全部 `ExecDecision`、通知与定时器(.1:64 条 `ForceStep{Engine/Step}`;.7:10 条 `TargetCleared`) |

### EXE-022 advance 剩余容量不足单 cell 上限时停止并登记 Continue

- 规则:[输出 `ExecOut`](README.md#输出-execout容量固定sequencer-预分配每次调用本-crate-之后排空)
- 级别:unit
- 里程碑:M2
- 前置:C0;a1 有 64 个两腿 cell,永续腿各 −5.0、现货腿各 +5.00;`ExecOut` 用 README 给定容量(orders 8、decisions 16、cancel_resting 8、notes 8)
- 步骤:
  1. `engine_flatten(a1)`;`advance`
  2. 以 `Timer{Exec, Continue}` 到期调 `on_timer`,再 `advance`;重复直到 `has_pending() = false`
- 期望:
  - 步骤 1:各向量长度不超过容量;被处理的 cell 是 cix 升序的前缀;`ExecOut.timers` 恰有一条 `Timer{Exec, Continue}`(`timer_id = 4`,`group = GROUP_NONE`);`has_pending() = true`
  - 每次停止前最后处理的 cell 之后,剩余容量至少有一项低于单 cell 上限(orders 4、cancels 4、cancel_resting 4、decisions 6、notify 2、broken 1、timers 3、notes 3)
  - 步骤 2 结束后,全部批次的 `ExecDecision` 拼接与容量无限的一次 `advance` 逐条相同(64 条 `ForceStep{Engine/Step}`,无重复、无缺失)
- 实现:`crates/hft-exec/tests/output.rs::exe_022_advance_continue`

### EXE-023 同一 advance 内同一 (account, inst) 的 cancel_resting 只写一条

- 规则:[输出 `ExecOut`](README.md#输出-execout容量固定sequencer-预分配每次调用本-crate-之后排空)、[保证金危急的默认减仓](README.md#保证金危急的默认减仓a5与-engine_flatten)
- 级别:unit
- 里程碑:M2
- 前置:C0;a1 的 g7 与 g10 = `GroupId(0x0100_000A)` 都由 S、P 两腿组成,永续腿各 −5.0;a1 MarginStress L3
- 步骤:
  1. `force_deleverage(a1, B, 0.2)`;`advance`
- 期望:
  - `ExecOut.cancel_resting` 恰为 `{a1, S}`、`{a1, P}` 各一条
  - g7、g10 各一条 `ExecDecision{ForceStep, detail = 5 | 5 << 8 | 0b11 << 16}`(`Engine/CancelResting`,腿位图 0b11)
- 实现:`crates/hft-exec/tests/output.rs::exe_023_cancel_resting_dedup`

### EXE-024 任意容量组合下 ExecDecision 与通知不丢失

- 规则:[输出 `ExecOut`](README.md#输出-execout容量固定sequencer-预分配每次调用本-crate-之后排空)
- 级别:prop
- 里程碑:M2
- 性质:同一输入序列,在每个向量容量取 README 值的实现上(按 `Continue` 续推)与容量无限的参考实现上运行,每个 cell 的 `ExecDecision` 序列、通知序列、订单序列逐条相同,只是分批(意图管线的结果在每批之后回填,不同 cell 的记录可按批交错)
- 生成:1–64 个 cell;事件序列 1–200 条,从 {set_target, clear_target, 成交回报, LinkNotice, on_risk(CapSet/CapCleared/MarginStress), force_deleverage, engine_flatten, clear_account, on_venue_gate_open, 定时器到期} 抽取
- 参考:`tests/model.rs` 的无限容量运行
- 例数:PR 1 000
- 实现:`crates/hft-exec/tests/props.rs::exe_024_no_output_lost_under_caps`

### EXE-025 最后一片自动平仓的回报先于 CapCleared{Broken}:Done 在 on_order_update 中写出

- 规则:[调用顺序](README.md#调用顺序sequencer-同一输入事件内流程归属-hft-sequencer)、[Broken 处置](README.md#broken-处置)
- 级别:unit
- 里程碑:M2
- 前置:C0;g7 Broken,`auto_fix.phase = Running`、`leg_ix = 0`、开始时裸量 0.50、`budget = 0.50`、`done_qty = 0.40`、`n_slices = 4`;在途 `AutoFix` 片卖 S 0.10;腿净量 S +1.10、P −1.00
- 步骤:
  1. `on_order_update`(该片全部成交 0.10,终态)
  2. `on_risk(a1, CapCleared{Broken, g7})`
  3. `advance`
- 期望:
  - 步骤 1 的 `ExecOut`:`ExecDecision{AutoFix, detail = 4 | 3 << 8}`(`AutoFix/Done`)与 `Note{ExecAutoFixDone}`(P1,`v = [0x01_0000_0701, 0, 0.50, 0.50, 0, 5]`)各一条
  - 步骤 2 后 `auto_fix.phase = None`
  - 步骤 3 写 `ExecDecision{State, state_from = Broken, state_to = Idle}`,通知 `Done{cleared_by_engine: true}`;全过程只有一条 `ExecAutoFixDone`
- 实现:`crates/hft-exec/tests/output.rs::exe_025_last_autofix_slice_before_capcleared`

### EXE-026 set_target 接受后登记立即到期的 Timer{Exec, Wake},不在入口内出单

- 规则:[调用顺序](README.md#调用顺序sequencer-同一输入事件内流程归属-hft-sequencer)
- 级别:unit
- 里程碑:M2
- 前置:C0、T1;g8 无目标;`now = t 1000`
- 步骤:
  1. i1 `set_target`(g8:P −10.0)
- 期望:
  - 返回 `Ok`;`ExecOut.orders` 为空;`ExecOut.timers` 含 `Timer{Exec, Wake}`(`timer_id = 1 | 1 << 8`,`group = g8`,到期 = t 1000)一条;写 `ExecDecision{TargetSet}` 一条;`has_pending() = true`
- 实现:`crates/hft-exec/tests/output.rs::exe_026_set_target_registers_wake`

### EXE-027 早到的旧 Wake 定时器只重新登记

- 规则:[定时器](README.md#定时器)、[调用顺序](README.md#调用顺序sequencer-同一输入事件内流程归属-hft-sequencer)
- 级别:unit
- 里程碑:M2
- 前置:C0、T1;g8 在 t 1000 出第一片后 `wake_at = t 1300`(冷却 300 ms)
- 步骤:
  1. `on_timer(1 | 1 << 8 | g8 << 32)`,`now = t 1001`
  2. 同一定时器在 `now = t 1300` 到期
- 期望:
  - 步骤 1:`ExecOut.timers` 恰一条 `Timer{Exec, Wake}` 到期 t 1300;无订单、无 `ExecDecision`;`has_pending() = false`
  - 步骤 2 之后 `has_pending() = true`,`advance` 按 EXE-145 出第二片
- 实现:`crates/hft-exec/tests/output.rs::exe_027_early_wake_reregisters`

### EXE-040 tag 原样带进订单、ExecDecision 与 ExecUpdate

- 规则:[执行规格参数](README.md#执行规格参数类型定义在-sdk本节定义语义)
- 级别:unit
- 里程碑:M2
- 前置:C0、T1;g8 目标 P −10.0,`tag = 0xDEAD_BEEF`
- 步骤:
  1. `set_target`;`advance`
  2. 第一片成交 −4.0、终态
- 期望:
  - 步骤 1:`ExecDecision{TargetSet}` 与 `ExecDecision{Slice}` 的 `tag = 0xDEAD_BEEF`;`ExecOrder.intent.tag = 0xDEAD_BEEF`
  - 步骤 2:通知 `ExecUpdate::Progress{leg_ix = 0, d = −4.0}`,其 `tag = 0xDEAD_BEEF`
- 实现:`crates/hft-exec/tests/params.rs::exe_040_tag_propagates`

### EXE-041 PairParams 写进第一腿的 LegLink 与 HedgePlan

- 规则:[执行规格参数](README.md#执行规格参数类型定义在-sdk本节定义语义)、[PairOnFill](README.md#paironfill)
- 级别:unit
- 里程碑:M2
- 前置:C0;T0 但 `ratio 2/1`、`fix_tolerance = 0.03`、`residual = Defer`、`hedge_px_mode = Cross`、`hedge_cross = 0.003`、`hedge_max_slip = 0.004`;目标 S +10.00 / P −20.0
- 步骤:
  1. `set_target`;`advance`
- 期望:
  - 第一腿 `ExecOrder.intent` 带 `LegLink{group = g7, role = First, tolerance = 0.03, hedge = HedgePlan{kind = OnFill, venue = B, account = a1, inst = P, side = Sell, px_mode = Cross, px_param = 0.003, max_slip = 0.004, reduce_only = 0, ratio = 2/1, residual = Defer, on_fail = Retry, tif = Ioc}}`;`ExecOrder.kind = PairFirst`(2)
- 实现:`crates/hft-exec/tests/params.rs::exe_041_pair_params_into_leglink`

### EXE-042 目标包含不合法的规格参数时按 SpecInvalid 拒绝

- 规则:[执行规格参数](README.md#执行规格参数类型定义在-sdk本节定义语义)、[目标的接受与替换](README.md#目标的接受与替换)
- 级别:table
- 里程碑:M2
- 前置:C0;g7 无目标;每行在 T0 上只改所列字段,目标 S +10 / P −10
- 实现:`crates/hft-exec/tests/accept.rs::exe_042_spec_param_ranges`

| 行 | 改动 | 期望 |
|---|---|---|
| EXE-042.1 | `slice.depth_levels = 0` | `Err(SpecInvalid)` |
| EXE-042.2 | `slice.depth_levels = 1` | `Ok` |
| EXE-042.3 | `slice.depth_levels = 10` | `Ok` |
| EXE-042.4 | `slice.depth_levels = 11` | `Err(SpecInvalid)` |
| EXE-042.5 | `slice.depth_frac = Rate(0)` | `Err(SpecInvalid)` |
| EXE-042.6 | `slice.depth_frac = Rate(1_000_000_000)` | `Ok` |
| EXE-042.7 | `slice.depth_frac = Rate(1_000_000_001)` | `Err(SpecInvalid)` |
| EXE-042.8 | `ratio_num = 0` | `Err(SpecInvalid)` |
| EXE-042.9 | `ratio_den = 0` | `Err(SpecInvalid)` |
| EXE-042.10 | `first_leg = Fixed, first_leg_ix = 1` | `Ok`(先手腿 1) |
| EXE-042.11 | `first_leg = Fixed, first_leg_ix = 2` | `Err(SpecInvalid)` |

### EXE-050 目标单元不变量在任意事件序列下逐步成立

- 规则:[目标单元](README.md#目标单元targetcell)
- 级别:prop
- 里程碑:M2
- 性质:每次入口调用之后:新发的 `origin = Target` 订单只出现在 `state ∈ {Working, Stuck}` 的 cell 上,`state = Broken` 的 cell 只发引擎目标的订单、补腿类订单与 `CmdHedgeFix`(进入 Idle/Broken 时已在途的 IOC 不撤);`state = Idle ⇒ !has_target ∨ 全部腿在带内`;`state = Broken ⇒` 无 `origin = Target` 订单;`hold & MANUAL_HOLD ⇒` 无 `HedgeFix`/`ResidualTrim`/`ExternalDefault`/`AutoFix` 订单;这四类订单不落在 `venue_open` 无该位的 venue;Engine 订单不增加任何腿的 |仓位|;无订单穿越零(Unknown 切片按全部成交计);`flatten_lock` 账户上被接受的目标每腿 |目标| ≤ 接受时的参照量;配对第一腿订单量是 `pair_lot` 的整数倍(按较粗 lot 的减仓与风险目标除外);`serial ≠ 0 ⇒` 无带配对链接(`role = First`)的订单;每笔订单限价 = `cap_cross_px` 的结果且 `⌈qty × px × quote_rate⌉ ≤ cap_n`;无组 cell 只产生 reduce-only 永续 `Flatten` 单
- 生成:1–8 个 cell(1–4 腿,现货/永续混合,lot 从 {0.001, 0.003, 0.01, 0.1} 抽取);事件 1–300 条:BBO/深度变化、成交拆分(部分、零、全部、Unknown 后迟到)、门控拒绝位、LinkNotice、外来成交(各 cause)、on_risk(MarginStress 各档、CapSet/CapCleared Broken/Reorg/Kill/Hard)、`force_deleverage`、`engine_flatten`、`on_venue_gate_open`、定时器到期、`set_target`/`clear_target`/`note_strategy_action`/`hedge_fix_cmd`
- 参考:逐条断言上列性质(mock 视图按成交更新 `legs` 与 `portfolio`)
- 例数:PR 2 000
- 实现:`crates/hft-exec/tests/props.rs::exe_050_cell_invariants`
- 回归:固定序列见 EXE-513。

### EXE-051 TargetCell 不超过 640 B,AutoFixState 为 56 B 且字段偏移固定

- 规则:[目标单元](README.md#目标单元targetcell)、[延迟与资源契约](README.md#延迟与资源契约)
- 级别:table
- 里程碑:M2
- 前置:编译期常量断言,另以 `memoffset`/`offset_of!` 运行期复核
- 实现:`crates/hft-exec/tests/layout.rs::exe_051_cell_layout`

| 行 | 断言 | 期望 |
|---|---|---|
| EXE-051.1 | `size_of::<TargetCell>()` | ≤ 640 |
| EXE-051.2 | `size_of::<AutoFixState>()` | 56 |
| EXE-051.3 | `offset_of!(AutoFixState, exec_ms)` | 8 |
| EXE-051.4 | `offset_of!(AutoFixState, due_tsc)` | 16 |
| EXE-051.5 | `offset_of!(AutoFixState, started_tsc)` | 24 |
| EXE-051.6 | `offset_of!(AutoFixState, run_mark_tsc)` | 32 |
| EXE-051.7 | `offset_of!(AutoFixState, budget)` | 40 |
| EXE-051.8 | `offset_of!(AutoFixState, done_qty)` | 48 |
| EXE-051.9 | `AutoFixPhase` 判别值 | `None=0 Primed=1 WaitHard=2 Running=3 Done=4 Failed=5 Cancelled=6 WaitHold=7` |

### EXE-052 无组 cell 的在途取自本 cell 的 own_infl,含 Unknown 槽

- 规则:[目标单元](README.md#目标单元targetcell)、[保证金危急的默认减仓](README.md#保证金危急的默认减仓a5与-engine_flatten)
- 级别:unit
- 里程碑:M2
- 前置:C0;a1 无组永续持仓 P −3.0(`portfolio.perp(B, P) = −3.0`,无 cell 引用 P 的持仓);a1 MarginStress L3
- 步骤:
  1. `force_deleverage(a1, B, 0.5)`;`advance`
  2. `on_order_update`(该片 → `Unknown`,无成交)
  3. 冷却 200 ms 到期后 `advance`
- 期望:
  - 步骤 1:无组 cell 出 `ExecOrder{kind = ForceStep, side = Buy, inst = P, qty = 1.5}`,`reduce_only` 置位(目标 −1.5)
  - 步骤 2 后快照中无组 cell 行 `own_infl[0] = +1.5`、`filled` 全 0
  - 步骤 3:`eff = −3.0 + 1.5 = −1.5 = 目标` → 无订单;全过程无 `ExecUpdate`
- 实现:`crates/hft-exec/tests/cell.rs::exe_052_ungrouped_own_infl`

### EXE-060 Idle 的 cell 收到带外目标进入 Working

- 规则:[状态机](README.md#状态机)
- 级别:unit
- 里程碑:M2
- 前置:C0、T1;g8 Idle,P 净量 0
- 步骤:
  1. `set_target`(P −10.0)
- 期望:
  - `ExecDecision{TargetSet, state_from = Idle, state_to = Working}`;`view(a1, g8).state = Working`
- 实现:`crates/hft-exec/tests/fsm.rs::exe_060_idle_to_working`

### EXE-061 全部腿在带内且无在途时回到 Idle 并通知 Done

- 规则:[状态机](README.md#状态机)、[推进](README.md#推进advance-中每个-cellon_legs--每腿--depth_levels-档)
- 级别:unit
- 里程碑:M2
- 前置:C0、T1(`band = 0.01`);g8 目标 P −10.0,Working,`filled[0] = −9.9`,P 净量 −9.9,无在途
- 步骤:
  1. `touch_inst(P)`;`advance`
- 期望:
  - `ExecDecision{State, state_from = Working, state_to = Idle, detail = 0}` 一条;通知 `ExecUpdate::Done{cleared_by_engine: false}` 一条;无订单
- 实现:`crates/hft-exec/tests/fsm.rs::exe_061_working_to_idle_done`

### EXE-062 带内但有即时在途时不判 Done

- 规则:[推进](README.md#推进advance-中每个-cellon_legs--每腿--depth_levels-档)
- 级别:unit
- 里程碑:M2
- 前置:同 EXE-061,但 `legs.leg[0].inflight_immediate = −0.1`
- 步骤:
  1. `touch_inst(P)`;`advance`
- 期望:
  - 无 `ExecDecision`、无通知、无订单;`state = Working`
- 实现:`crates/hft-exec/tests/fsm.rs::exe_062_inflight_blocks_done`

### EXE-063 clear_target 使 Working 的 cell 回到 Idle,不撤在途 IOC

- 规则:[状态机](README.md#状态机)、[目标的接受与替换](README.md#目标的接受与替换)
- 级别:unit
- 里程碑:M2
- 前置:C0、T1;g8 Working,在途 IOC 一片
- 步骤:
  1. `clear_target(i1, a1, g8)`
- 期望:
  - `ExecDecision{TargetCleared, state_from = Working, state_to = Idle}`;`ExecOut.cancels` 为空;`view(a1, g8)` 无目标
- 实现:`crates/hft-exec/tests/fsm.rs::exe_063_clear_target_to_idle`

### EXE-064 门控持续时长到 stuck_after_ms 才进入 Stuck{Gated}

- 规则:[状态机](README.md#状态机)、[门控拒绝与退避](README.md#门控拒绝与退避)
- 级别:table
- 里程碑:M2
- 前置:C0(`stuck_after_ms = 2000`、`gated_backoff_ms = 500`)、T1;g8 目标 P −10.0;自 t 0 起每片都被风控以 `RejectBits::RATE_BACKOFF` 拒绝(`on_place_result(Rejected(RATE_BACKOFF))`),`gated_since = t 0`;每行在所列时刻推进一次
- 实现:`crates/hft-exec/tests/fsm.rs::exe_064_gated_to_stuck_boundary`

| 行 | 推进时刻 | 期望 |
|---|---|---|
| EXE-064.1 | t 1999 | `state = Working`;无 `State` 决定 |
| EXE-064.2 | t 2000 | `ExecDecision{State, state_to = Stuck, detail = 2 \| 1 << 8, gate = RATE_BACKOFF}`;通知 `Stuck(Gated(RATE_BACKOFF))` |
| EXE-064.3 | t 2001(未在 .2 推进) | 同 .2 |

### EXE-065 即时原因直接进入 Stuck,不等 stuck_after_ms

- 规则:[状态机](README.md#状态机)
- 级别:table
- 里程碑:M2
- 前置:C0、T1;g8 Working,`gated_since = 0`
- 实现:`crates/hft-exec/tests/fsm.rs::exe_065_immediate_stuck_reasons`

| 行 | 输入 | 期望 `ExecDecision{State}` 的 `detail` |
|---|---|---|
| EXE-065.1 | 第一片 `on_place_result(Shadowed)` | `2 \| 2 << 8`(`Stuck/Shadow`) |
| EXE-065.2 | `on_external`(P,`cause = Liquidation`,`origin = Strategy` 的目标) | `2 \| 6 << 8`(`Stuck/External`) |
| EXE-065.3 | a1 L3 `force_deleverage` 替换该策略目标后 `advance` | `2 \| 7 << 8`(`Stuck/EngineOverride`) |
| EXE-065.4 | 连续 3 次交易所拒单(`max_slice_fails = 3`) | `2 \| 5 << 8`(`Stuck/SliceFails`) |

### EXE-066 Stuck 的原因消失后下一次推进回到 Working

- 规则:[状态机](README.md#状态机)
- 级别:unit
- 里程碑:M2
- 前置:C0、T1;g8 `Stuck{StaleQuote}`(P 的 `conn_live_tsc` 落后 2001 ms)
- 步骤:
  1. P 的 `conn_live_tsc = now`;`touch_inst(P)`;`advance`
- 期望:
  - `ExecDecision{State, state_from = Stuck, state_to = Working, detail = 0}` 与 `ExecDecision{Slice}` 各一条
- 实现:`crates/hft-exec/tests/fsm.rs::exe_066_stuck_back_to_working`

### EXE-067 Stuck{SliceFails} 不因原样重发目标解除,改 tag 或清除后解除

- 规则:[状态机](README.md#状态机)、[目标的接受与替换](README.md#目标的接受与替换)
- 级别:unit
- 里程碑:M2
- 前置:C0(`max_slice_fails = 3`)、T1;g8 `Stuck{SliceFails}`,目标 tag 0x10
- 步骤:
  1. i1 `set_target`(与当前目标逐字段相同)
  2. i1 `set_target`(只把 `tag` 改为 0x11)
- 期望:
  - 步骤 1 返回 `Ok`;无 `ExecDecision`;`state = Stuck`、`fail_streak = 3`
  - 步骤 2 写 `ExecDecision{TargetReplaced}`;`fail_streak = 0`、`state = Working`
- 实现:`crates/hft-exec/tests/fsm.rs::exe_067_slice_fails_needs_replace`

### EXE-068 链接 Broken 使 cell 进入 Broken{LinkBroken} 并带链接原因

- 规则:[状态机](README.md#状态机)、[PairOnFill](README.md#paironfill)
- 级别:table
- 里程碑:M2
- 前置:C0、T0;g7 Working,`active` 为链接 L1
- 实现:`crates/hft-exec/tests/fsm.rs::exe_068_link_broken_causes`

| 行 | `LinkNotice{state = Broken, broken = …}` | 期望 |
|---|---|---|
| EXE-068.1 | `RetryExhausted` | `ExecDecision{State, state_to = Broken, detail = 3 \| 1 << 8 \| 1 << 16}`;`ExecOut.broken` 一条 `BrokenReq{a1, g7}`;`Note{ExecBroken}` P0;通知 `Broken(LinkBroken)` |
| EXE-068.2 | `Deadline` | `detail = 3 \| 1 << 8 \| 2 << 16` |
| EXE-068.3 | `Overfill` | `detail = 3 \| 1 << 8 \| 4 << 16` |
| EXE-068.4 | `ResumeTimeout` | `detail = 3 \| 1 << 8 \| 6 << 16` |
| EXE-068.5 | `ReverseFailed` | `detail = 3 \| 2 << 8`(`Broken/ReverseFailed`,arg 0) |
| EXE-068.6 | `HedgeLimit` | `detail = 3 \| 4 << 8`(`Broken/HedgeVeto`,arg 0) |

### EXE-069 on_risk 收到 CapSet{Broken} 使任一状态的 cell 进入 Broken,重复设置幂等

- 规则:[状态机](README.md#状态机)、[Broken 处置](README.md#broken-处置)
- 级别:table
- 里程碑:M2
- 前置:C0;`now = t 5000`;每行 g7 的初始状态不同
- 实现:`crates/hft-exec/tests/fsm.rs::exe_069_capset_broken_from_any_state`

| 行 | 初始状态 | 期望(`on_risk(a1, CapSet{Broken, g7})` + `advance`) |
|---|---|---|
| EXE-069.1 | Idle(无目标,裸量 0.5) | `state = Broken`;`broken_since = t 5000`;`auto_fix.phase = Primed`、`due_tsc = t 125 000`;`Timer{Exec, AutoFix}` 一条 |
| EXE-069.2 | Working | 同 .1;策略目标保留(`view.intent` 不变) |
| EXE-069.3 | Stuck{Gated} | 同 .1 |
| EXE-069.4 | 已 Broken(`broken_since = t 1000`,`Primed`) | `broken_since`、`due_tsc` 不变;无新的 `State` 决定;无新定时器 |

### EXE-070 HEDGE_LIMIT 拒绝使 cell 进入 Broken{HedgeVeto}

- 规则:[对冲量上限](README.md#对冲量上限b6)、[状态机](README.md#状态机)
- 级别:unit
- 里程碑:M2
- 前置:C0;g7 非 Broken,一笔 `HedgeFix` 单已交出
- 步骤:
  1. `on_place_result(id, Rejected(HEDGE_LIMIT))`
- 期望:
  - `ExecDecision{State, state_to = Broken, detail = 3 \| 4 << 8}`;`auto_fix.phase = Primed`;`fix.count` 不增加;无 `Note{ExecEngineOrderRejected}`
- 实现:`crates/hft-exec/tests/fsm.rs::exe_070_hedge_limit_to_broken`

### EXE-071 CapCleared{Broken} 且无引擎目标:只清策略目标,回到 Idle

- 规则:[状态机](README.md#状态机)
- 级别:unit
- 里程碑:M2
- 前置:C0、T0;g7 Broken,策略目标(冻结)存在,无引擎目标,`auto_fix.phase = Primed`
- 步骤:
  1. `on_risk(a1, CapCleared{Broken, g7})`;`advance`
- 期望:
  - `ExecDecision{State, state_from = Broken, state_to = Idle}` 与 `ExecDecision{TargetCleared, eflags = CLEARED_BY_ENGINE}`;通知 `Done{cleared_by_engine: true}` 一条;`auto_fix` 全零;`Timer{Exec, AutoFix}` 被撤(`ExecOut.timers` 中一条撤销)
- 实现:`crates/hft-exec/tests/fsm.rs::exe_071_capcleared_to_idle`

### EXE-072 CapCleared{Broken} 且有引擎目标:保留引擎目标,回到 Working

- 规则:[状态机](README.md#状态机)、[保证金危急的默认减仓](README.md#保证金危急的默认减仓a5与-engine_flatten)
- 级别:unit
- 里程碑:M2
- 前置:C0;g7 Broken,`engine_flatten(a1)` 设下的引擎目标 0/0(`reason = Hard`)进行中,P 净量 −0.3、S +0.30
- 步骤:
  1. `on_risk(a1, CapCleared{Broken, g7})`;`advance`
- 期望:
  - `ExecDecision{State, state_from = Broken, state_to = Working}`;`view.origin = Engine(KillFlatten)`,引擎目标仍为 0/0;同一 `advance` 内有 `ExecOrder{kind = ForceStep}`
- 实现:`crates/hft-exec/tests/fsm.rs::exe_072_capcleared_keeps_engine_target`

### EXE-073 Broken 的 cell 上 clear_target 只清目标,状态保持 Broken

- 规则:[状态机](README.md#状态机)、[目标的接受与替换](README.md#目标的接受与替换)
- 级别:unit
- 里程碑:M2
- 前置:C0、T0;g7 Broken,策略目标冻结
- 步骤:
  1. `clear_target(i1, a1, g7)`
- 期望:
  - `ExecDecision{TargetCleared, state_from = Broken, state_to = Broken}`;`view.state = Broken`;`auto_fix` 不变
- 实现:`crates/hft-exec/tests/fsm.rs::exe_073_clear_target_keeps_broken`

### EXE-074 对非 Broken 的 cell 投递 CapCleared{Broken} 不改变状态

- 规则:[状态机](README.md#状态机)
- 级别:unit
- 里程碑:M2
- 前置:C0、T1;g8 Working
- 步骤:
  1. `on_risk(a1, CapCleared{Broken, g8})`;`advance`
- 期望:
  - 无 `State` 决定、无通知;`view.state = Working`;目标保留
- 实现:`crates/hft-exec/tests/fsm.rs::exe_074_capcleared_non_broken_noop`

### EXE-075 Broken 期间不产生 Target 订单、不做自动补腿与残余修剪

- 规则:[状态机](README.md#状态机)
- 级别:unit
- 里程碑:M2
- 前置:C0、T0;g7 Broken(t 0 起),策略目标 S +10 / P −10 冻结,腿净量 S +2.00、P −1.0,`auto_fix.phase = Primed`、`due_tsc = t 120 000`
- 步骤:
  1. 在 t 1000、t 60 000 各 `touch_inst(S)`、`touch_inst(P)`;`advance`
- 期望:
  - 两次 `advance` 都无订单、无 `ExecDecision`(Broken 且无引擎目标、未到期 → 跳过)
- 实现:`crates/hft-exec/tests/fsm.rs::exe_075_broken_no_target_orders`

### EXE-076 Stuck 的 cell 收到链接 Broken 进入 Broken

- 规则:[状态机](README.md#状态机)
- 级别:unit
- 里程碑:M2
- 前置:C0、T0;g7 `Stuck{Gated(LINK_STALE)}`,在途链接 L1(由更早一片产生)
- 步骤:
  1. `on_link(LinkNotice{L1, state = Broken, broken = Deadline})`
- 期望:
  - `ExecDecision{State, state_from = Stuck, state_to = Broken, detail = 3 \| 1 << 8 \| 2 << 16}`
- 实现:`crates/hft-exec/tests/fsm.rs::exe_076_stuck_to_broken`

### EXE-080 推进第 0 步:有 force_req 时先设定引擎目标再推进

- 规则:[推进](README.md#推进advance-中每个-cellon_legs--每腿--depth_levels-档)
- 级别:unit
- 里程碑:M2
- 前置:C0;g8 策略目标 P −10.0,P 净量 −10.0(已到达,Idle);a1 L3
- 步骤:
  1. `force_deleverage(a1, B, 0.2)`;`advance`
- 期望:
  - 同一 `advance` 内依次写 `ExecDecision{ForceStep, detail = 5 \| 1 << 8}`(目标 P −8.0)、`ExecDecision{Slice}`(买 P 2.0,`origin = Engine`、`reduce_only` 置位);快照 Acct 行 `force_kind = 0`(`force_req` 已清)
- 实现:`crates/hft-exec/tests/advance.rs::exe_080_force_req_first`

### EXE-081 推进第 1 步:未被事件标记且未到 wake_at 的 cell 跳过

- 规则:[推进](README.md#推进advance-中每个-cellon_legs--每腿--depth_levels-档)
- 级别:table
- 里程碑:M2
- 前置:C0、T1(`cooldown_ms = 300`);g8 在 t 1000 出第一片并已终态(部分成交),`wake_at = t 1300`;每行由 `touch_inst(P)`(行情标记)或定时器标记后推进
- 实现:`crates/hft-exec/tests/advance.rs::exe_081_wake_at_boundary`

| 行 | 标记方式与时刻 | 期望 |
|---|---|---|
| EXE-081.1 | `touch_inst(P)` 于 t 1299 | 无订单 |
| EXE-081.2 | `Timer{Exec, Wake}` 于 t 1300 | 出第二片 |
| EXE-081.3 | `touch_inst(P)` 于 t 1301 | 出第二片 |
| EXE-081.4 | 成交回报(`on_order_update`)于 t 1100 标记 | 不受 `wake_at` 限制进入推进;切片间隔仍按冷却,无订单 |

### EXE-082 目标为 0 的腿按尘埃线判定到达

- 规则:[推进](README.md#推进advance-中每个-cellon_legs--每腿--depth_levels-档)、[不交易带与紧急度](README.md#不交易带与紧急度)
- 级别:table
- 里程碑:M2
- 前置:C0(`dust_notional = 5 USDT`)、T1(`band = 0.01`);g8 目标 P 0;mark 100.00;无在途
- 实现:`crates/hft-exec/tests/advance.rs::exe_082_zero_target_dust_reach`

| 行 | P 净量 | 期望 |
|---|---|---|
| EXE-082.1 | −0.05(名义 5 USDT) | 到达:`State → Idle`、`Done` |
| EXE-082.2 | −0.05000001(名义 5.000001 USDT) | 未到达;出一片(或按最小下单量交残余规则) |
| EXE-082.3 | −0.04999999 | 到达 |

### EXE-083 到达后按残余规则检查一次

- 规则:[推进](README.md#推进advance-中每个-cellon_legs--每腿--depth_levels-档)、[残余与尘埃](README.md#残余与尘埃)
- 级别:unit
- 里程碑:M2
- 前置:C0(`dust_notional = 5 USDT`)、T0(`band = 0.01`、`residual = TrimExcess`、`fix_tolerance = 0.02`);g7 目标 S +10.00 / P −10.0;腿净量 S +10.06、P −10.0,无在途、无链接(两腿在带内;裸量 0.06,名义 6 USDT,裸量比 0.006 ≤ `tol`;0.06 < P 的 `min_qty` 0.1,属残余)
- 步骤:
  1. `touch_inst(S)`;`advance`
- 期望:
  - 依次写 `ExecDecision{State, state_to = Idle}`、`ExecDecision{ResidualTrim}`;通知 `Done`;`ExecOrder{kind = ResidualTrim, side = Sell, inst = S, qty = 0.06, hedge_cap_qty = 0.07, origin = Engine}`;无 `HedgeFix` 单
- 实现:`crates/hft-exec/tests/advance.rs::exe_083_residual_check_after_done`

### EXE-084 全部腿在带内但有在途,或配对算法已有在途切片时等待

- 规则:[推进](README.md#推进advance-中每个-cellon_legs--每腿--depth_levels-档)
- 级别:table
- 里程碑:M2
- 前置:C0、T0;g7 目标 S +10 / P −10
- 实现:`crates/hft-exec/tests/advance.rs::exe_084_wait_on_inflight`

| 行 | 状态 | 期望(`advance`) |
|---|---|---|
| EXE-084.1 | 两腿在带内,S `inflight_immediate = +0.05` | 无订单、无决定 |
| EXE-084.2 | S 差 5.00,`active.kind = Pair`(链接 L1 未结束) | 无订单 |
| EXE-084.3 | g8(`TakerSlice` 单腿)`active.kind = Slice` | 无订单 |
| EXE-084.4 | 4 腿 `TakerSlice` 目标,腿 0 有在途、腿 1 无在途且在带外 | 只出腿 1 一片 |

### EXE-085 风险目标的永续 reduce-only 腿在途切片 Unknown 时扣量继续,不穿越零

- 规则:[推进](README.md#推进advance-中每个-cellon_legs--每腿--depth_levels-档)、[运行模式、Drain 与 Unknown](README.md#运行模式drain-与-unknown)
- 级别:unit
- 里程碑:M2
- 前置:C0;g8 P 净量 −6.0;`engine_flatten(a1)` 设引擎目标 0;第一片买 P 6.0 发出后 `on_order_update` → `Unknown`(无成交),`legs.leg[0].inflight_immediate = +6.0`;P 的 `VenueCaps.reduce_only = true`
- 步骤:
  1. 冷却 200 ms 后 `advance`
  2. 改为第一片 `Unknown` 前已部分成交 2.0(P 净量 −4.0,在途 +4.0),`advance`
- 期望:
  - 步骤 1:`active` 已释放;下一片数量上限 = |−6.0| − 6.0 = 0 → 无订单
  - 步骤 2:无订单(|−4.0| − 4.0 = 0);在任何回报组合下 P 净量不越过 0
- 实现:`crates/hft-exec/tests/advance.rs::exe_085_unknown_reduce_only_continue`

### EXE-086 Unknown 扣量继续的反例:现货腿、不支持 reduceOnly 的腿、带链接的配对第一腿都等待

- 规则:[推进](README.md#推进advance-中每个-cellon_legs--每腿--depth_levels-档)
- 级别:table
- 里程碑:M2
- 前置:C0;`engine_flatten(a1)` 已设引擎目标 0;每行一条在途切片进入 `Unknown`(剩余 1.0);冷却已过
- 实现:`crates/hft-exec/tests/advance.rs::exe_086_unknown_wait_cases`

| 行 | 场景 | 期望(`advance`) |
|---|---|---|
| EXE-086.1 | 串行腿的现货 S(净量 +3.00) | 无 S 订单;`active` 不释放 |
| EXE-086.2 | P 改为 `VenueCaps.reduce_only = false`(净量 −3.0) | 无 P 订单 |
| EXE-086.3 | 非 Broken 的 g7 配对第一腿 P 带 LegLink、`Unknown` | 不在本行释放 `active`;转 `SerialFallback`(原因 6,见 EXE-347) |
| EXE-086.4 | 策略目标(`reason = Rebalance`)的单腿 P 切片 `Unknown` | 无订单(例外只用于 `MarginStress`/`Hard` 引擎目标) |

### EXE-087 预检:增仓腿的模式与风控条件不满足时不出单并记 gated_since

- 规则:[推进](README.md#推进advance-中每个-cellon_legs--每腿--depth_levels-档)
- 级别:table
- 里程碑:M2
- 前置:C0、T1;g8 目标 P −10.0(增仓);`now = t 1000`
- 实现:`crates/hft-exec/tests/advance.rs::exe_087_precheck_open`

| 行 | 条件 | 期望(`advance`) |
|---|---|---|
| EXE-087.1 | `v.mode = Shadow`,cell 尚未 `Stuck{Shadow}` | 预检放行,算出第一片交出(之后的处理见 EXE-391) |
| EXE-087.2 | `v.mode = CloseOnly` | 无订单;`gated_since = t 1000`;`gated_bits ⊇ CLOSE_ONLY` |
| EXE-087.3 | `open_allowed = false`(Live),cell 已 `Stuck{Shadow}` | 无订单;不再出片(与 Shadow 同,见 EXE-392) |
| EXE-087.4 | `v.risk.hard = true` | 无订单;`gated_since = t 1000` |
| EXE-087.5 | `v.risk.open_blocked = true`(Kill cap) | 无订单;`gated_since = t 1000` |
| EXE-087.6 | MarginStress L1 | 无订单;`gated_since = t 1000` |
| EXE-087.7 | 全部条件满足 | 出一片;`gated_since = 0` |

### EXE-088 预检:减仓腿只看 HARD

- 规则:[推进](README.md#推进advance-中每个-cellon_legs--每腿--depth_levels-档)
- 级别:table
- 里程碑:M2
- 前置:C0、T1;g8 P 净量 −10.0,目标 −5.0(减仓)
- 实现:`crates/hft-exec/tests/advance.rs::exe_088_precheck_reduce`

| 行 | 条件 | 期望 |
|---|---|---|
| EXE-088.1 | `v.mode = CloseOnly` | 出一片买 P(`class = Reduce`) |
| EXE-088.2 | `open_blocked = true` | 出一片 |
| EXE-088.3 | MarginStress L2 | 出一片 |
| EXE-088.4 | `hard = true` | 无订单;`gated_since` 置位 |

### EXE-089 预检:配对的第二腿报价不可用时第一腿不发

- 规则:[推进](README.md#推进advance-中每个-cellon_legs--每腿--depth_levels-档)、[PairOnFill](README.md#paironfill)
- 级别:unit
- 里程碑:M2
- 前置:C0、T0(`reason = Rebalance`);g7 目标 S +10 / P −10;P 的 `conn_live_tsc = now − 2001 ms`
- 步骤:
  1. `advance`
- 期望:
  - 无订单;`gated_since = now`;持续 2000 ms 后 `ExecDecision{State, state_to = Stuck, detail = 2 \| 4 << 8}`(`Stuck/StaleQuote`,不是 `Gated`)
- 实现:`crates/hft-exec/tests/advance.rs::exe_089_second_leg_quote_blocks_pair`

### EXE-090 穿越零:先只减到 0,开向另一侧在下一笔

- 规则:[推进](README.md#推进advance-中每个-cellon_legs--每腿--depth_levels-档)
- 级别:unit
- 里程碑:M2
- 前置:C0、T1(`cooldown_ms = 300`);g8 P 净量 +3.0,目标 −2.0
- 步骤:
  1. `set_target`;`advance`
  2. 第一片全部成交;t + 300 ms `advance`
- 期望:
  - 步骤 1:`ExecOrder{side = Sell, qty = 3.0}`(`class = Reduce`,`reduce_only` 置位)
  - 步骤 2:`ExecOrder{side = Sell, qty = 2.0}`(`class = Open`)
  - 全程无一笔订单使净量从 > 0 跨到 < 0
- 实现:`crates/hft-exec/tests/advance.rs::exe_090_cross_zero_split`

### EXE-091 出片写 Slice 决定、active、wake_at 与定时器

- 规则:[推进](README.md#推进advance-中每个-cellon_legs--每腿--depth_levels-档)
- 级别:unit
- 里程碑:M2
- 前置:C0、T1(`cooldown_ms = 300`);g8 目标 P −10.0;`now = t 1000`;`set_seq = 77`
- 步骤:
  1. `advance`
- 期望:
  - `ExecDecision{Slice, algo = TakerSlice, origin = Strategy, reason = Rebalance, urgency = Normal, leg_ix = 0, first_leg_ix = 0xFF, side = Sell, qty = 10.0, px = 99.31, target_qty = −10.0, pos_qty = 0, inflight_qty = 0, depth_qty = 250.0, set_seq = 77, exec_order_id = {cix 1, n 1}}`
  - `ExecOrder{id = {1, 1}, kind = Slice, origin = Target, reason = Rebalance, hedge_cap_qty = 0}`;`ExecOut.timers` 含 `Timer{Exec, Wake}` 到期 t 1300
- 实现:`crates/hft-exec/tests/advance.rs::exe_091_slice_records`

### EXE-100 不交易带的边界:恰在带上不动,超出 1 raw 即动

- 规则:[不交易带与紧急度](README.md#不交易带与紧急度)、[推进](README.md#推进advance-中每个-cellon_legs--每腿--depth_levels-档)
- 级别:table
- 里程碑:M2
- 前置:C0(`band_floor_notional = 20 USDT`);g7 改用 `TakerSlice`(T0 的 slice),目标 S +10.00、P 不设目标腿;`band = 0.01`(带宽 0.1);mark 100.00;无在途
- 实现:`crates/hft-exec/tests/band.rs::exe_100_band_boundary`

| 行 | S 净量 | 期望(`advance`) |
|---|---|---|
| EXE-100.1 | 9.90 | 在带内:`State → Idle`、无订单 |
| EXE-100.2 | 9.89999999(差 0.10000001) | 出一片买 S |
| EXE-100.3 | 10.10 | 在带内 |
| EXE-100.4 | 10.10000001 | 出一片卖 S |

### EXE-101 带的分母下限 floor = band_floor_notional / mark,按 lot 取整

- 规则:[不交易带与紧急度](README.md#不交易带与紧急度)
- 级别:table
- 里程碑:M2
- 前置:C0(`band_floor_notional = 20 USDT`);g7 `TakerSlice`,目标 S +0.10,`band = 0.1`;无在途;S 的 `min_notional = 1 USDT`(夹具默认 5 USDT 时 .2/.4 的 0.03/0.02 S 低于交易所最小名义)
- 实现:`crates/hft-exec/tests/band.rs::exe_101_band_floor`

| 行 | mark | S 净量 | 期望 |
|---|---|---|---|
| EXE-101.1 | 100.00(floor 0.20,带宽 0.02) | 0.08 | 在带内,无订单 |
| EXE-101.2 | 100.00 | 0.07 | 出一片买 S 0.03 |
| EXE-101.3 | 95.00(20 / 95 = 0.2105,向零取 lot 得 0.21,带宽 0.021) | 0.079(差 0.021) | 在带内,无订单 |
| EXE-101.4 | 95.00 | 0.07896(差 0.02104) | 出一片买 S 0.02(未取整的带宽 0.02105 会判在带内) |

### EXE-102 切片间隔按紧急度:Passive ×4、Normal、Urgent 取较小者

- 规则:[不交易带与紧急度](README.md#不交易带与紧急度)
- 级别:table
- 里程碑:M2
- 前置:C0(`passive_cooldown_mult = 4`、`urgent_cooldown_ms = 100`)、T1(`cooldown_ms = 300`);g8 目标 P −100.0;第一片在 t 0 发出并在 t 1 终态(部分成交);每行在所列时刻以定时器标记推进
- 实现:`crates/hft-exec/tests/band.rs::exe_102_cooldown_by_urgency`

| 行 | `urgency` | 推进时刻 | 期望 |
|---|---|---|---|
| EXE-102.1 | Passive | t 1199 | 无订单 |
| EXE-102.2 | Passive | t 1200 | 出第二片 |
| EXE-102.3 | Normal | t 299 | 无订单 |
| EXE-102.4 | Normal | t 300 | 出第二片 |
| EXE-102.5 | Urgent | t 99 | 无订单 |
| EXE-102.6 | Urgent | t 100 | 出第二片 |
| EXE-102.7 | Urgent,`cooldown_ms = 50` | t 50 | 出第二片(取 min(50, 100)) |

### EXE-103 Stuck 判定时长按紧急度:Passive ×4、Normal、Urgent 减半

- 规则:[不交易带与紧急度](README.md#不交易带与紧急度)
- 级别:table
- 里程碑:M2
- 前置:C0(`stuck_after_ms = 2000`、`passive_cooldown_mult = 4`)、T1;g8 目标 P −10.0;自 t 0 起 P 报价不可用(`conn_live_tsc` 停在 t −3000)
- 实现:`crates/hft-exec/tests/band.rs::exe_103_stuck_after_by_urgency`

| 行 | `urgency` | 推进时刻 | 期望 |
|---|---|---|---|
| EXE-103.1 | Passive | t 7999 | `Working` |
| EXE-103.2 | Passive | t 8000 | `Stuck{StaleQuote}` |
| EXE-103.3 | Normal | t 1999 | `Working` |
| EXE-103.4 | Normal | t 2000 | `Stuck{StaleQuote}` |
| EXE-103.5 | Urgent | t 999 | `Working` |
| EXE-103.6 | Urgent | t 1000 | `Stuck{StaleQuote}` |

### EXE-104 Passive 只用对手价一档的深度

- 规则:[不交易带与紧急度](README.md#不交易带与紧急度)
- 级别:table
- 里程碑:M2
- 前置:C0;R0 但 `max_order_notional = 100 000 USDT`;T1 但 `depth_frac = 0.2`、`depth_levels = 5`、`max_chunk_notional = 100 000 USDT`;g8 目标 P −100.0
- 实现:`crates/hft-exec/tests/band.rs::exe_104_passive_top_level_only`

| 行 | `urgency` | 期望第一片 |
|---|---|---|
| EXE-104.1 | Normal | 卖 P 50.0(5 档 250 × 0.2),`depth_qty = 250.0` |
| EXE-104.2 | Passive | 卖 P 10.0(顶档 50 × 0.2),`depth_qty = 50.0` |

### EXE-105 Urgent 时目标为 0 的腿不用带,只看尘埃线

已删除:规则改为目标为 0 的腿在任何紧急度下都不用带,由 EXE-510 替代。

### EXE-106 MarginStress/Hard 原因一律按 Urgent

- 规则:[不交易带与紧急度](README.md#不交易带与紧急度)
- 级别:unit
- 里程碑:M2
- 前置:C0(`urgent_cooldown_ms = 100`)、T1(`cooldown_ms = 300`);g8 P 净量 −10.0;目标 P 0,`reason = Hard`,`urgency = Passive`
- 步骤:
  1. `set_target`;`advance`(第一片 t 0 发出、t 1 终态)
  2. t 100 定时器标记推进
- 期望:
  - 步骤 2 出第二片(间隔取 min(300, 100) = 100 ms,而非 Passive 的 1200 ms)
- 实现:`crates/hft-exec/tests/band.rs::exe_106_risk_reason_urgent`

### EXE-110 分类提示、交易所侧 REDUCE_ONLY 与 origin 按切片作用

- 规则:[增仓与减仓的分类](README.md#增仓与减仓的分类)
- 级别:table
- 里程碑:M2
- 前置:C0;R0;S 不支持 reduceOnly、P 支持
- 实现:`crates/hft-exec/tests/class.rs::exe_110_class_hint_table`

| 行 | 场景 | 期望 `ExecOrder` |
|---|---|---|
| EXE-110.1 | g8 策略目标 P 0 → −10(`Rebalance`) | `intent.class = Open`、`reduce_only = 0`、`origin = Target` |
| EXE-110.2 | g8 P −10 → −5(`Rebalance`) | `class = Reduce`、`reduce_only = 1`、`origin = Target` |
| EXE-110.3 | g7 `TakerSlice` S +5.00 → 0(`Exit`) | `class = Reduce`、`reduce_only = 0`、`origin = Target` |
| EXE-110.4 | g8 P −10 → −5(`Manual`) | `class = Reduce`、`reduce_only = 1` |
| EXE-110.5 | g8 L3 引擎目标 P −10 → −8 | `class = Flatten`、`reduce_only = 1`、`origin = Engine`、`reason = MarginStress` |
| EXE-110.6 | g8 `engine_flatten` P −10 → 0 | `class = Flatten`、`reason = Hard`、`origin = Engine` |
| EXE-110.7 | g7 补腿,多出 P −0.5 | `class = Hedge`、`intent.link.role = HedgeFix`、`reduce_only = 1`、`hedge_cap_qty = 0.6` |
| EXE-110.8 | g7 残余修剪,多出 S +0.06 | `class = Hedge`、`reduce_only = 0`、`hedge_cap_qty = 0.07` |
| EXE-110.9 | a1 `force_active`,g8 策略 `Exit` 目标 P −8 → −4(替换了引擎目标) | `class = Flatten`、`origin = Engine` |
| EXE-110.10 | a1 离开 `force_active` 后,g8 新策略目标 P −4 → −2 | `class = Reduce`、`origin = Target` |

### EXE-111 现货减仓卖出量不超过 min(group 库存, 可用余额)

- 规则:[增仓与减仓的分类](README.md#增仓与减仓的分类)、[TakerSlice](README.md#takerslice)
- 级别:table
- 里程碑:M2
- 前置:C0;g7 `TakerSlice`(T0 的 slice,`depth_frac = 1.0`),S 净量 +10.00,目标 S 0(`Exit`);S 计价 USDT,`max_reduce_notional = 2000 USDT`
- 实现:`crates/hft-exec/tests/class.rs::exe_111_spot_sell_capped`

| 行 | group 库存 | 可用余额 | 期望第一片 |
|---|---|---|---|
| EXE-111.1 | 10.00 | 6.00 | 卖 S 6.00 |
| EXE-111.2 | 4.00 | 6.00 | 卖 S 4.00 |
| EXE-111.3 | 10.00 | 0.005(低于 lot) | 无订单 |

### EXE-112 配对第二腿的 HedgePlan.reduce_only 按第二腿是否减仓与是否支持

- 规则:[增仓与减仓的分类](README.md#增仓与减仓的分类)、[PairOnFill](README.md#paironfill)
- 级别:table
- 里程碑:M2
- 前置:C0、T0;各行先手腿按 `first_leg = Fixed` 指定
- 实现:`crates/hft-exec/tests/class.rs::exe_112_second_leg_reduce_only`

| 行 | 目标与先手 | 期望 `HedgePlan.reduce_only` |
|---|---|---|
| EXE-112.1 | 开仓 S 0 → +10、P 0 → −10,先手 S | 0(第二腿 P 增仓) |
| EXE-112.2 | 平仓 S +10 → 0、P −10 → 0,先手 S | 1(第二腿 P 减仓且支持) |
| EXE-112.3 | 平仓,先手 P | 0(第二腿 S 不支持 reduceOnly) |

### EXE-115 报价可用按连接健康判定,阈值处 2000 ms 放行、2001 ms 不放行

- 规则:[报价可用](README.md#报价可用连接健康)
- 级别:table
- 里程碑:M2
- 前置:C0;R0(`conn_liveness_ms = 2000`);g8 目标 P −10.0;`now = t 10 000`
- 实现:`crates/hft-exec/tests/quote.rs::exe_115_quote_ok_boundary`

| 行 | P 的连接与报价 | 期望(`advance`) |
|---|---|---|
| EXE-115.1 | `conn_live_tsc = t 8001`(差 1999 ms) | 出一片 |
| EXE-115.2 | `conn_live_tsc = t 8000`(差 2000 ms) | 出一片 |
| EXE-115.3 | `conn_live_tsc = t 7999`(差 2001 ms) | 无订单;`gated_since = t 10 000` |
| EXE-115.4 | `resync = true` | 无订单 |
| EXE-115.5 | `conn_up = false` | 无订单 |
| EXE-115.6 | bid 为空(只有 ask) | 无订单 |
| EXE-115.7 | ask 为空 | 无订单 |

### EXE-116 盘口 10 s 不变而连接健康时各类订单照常定价

- 规则:[报价可用](README.md#报价可用连接健康)
- 级别:table
- 里程碑:M2
- 前置:C0;P 的 BBO 99.80 / 100.20 最后一次变化在 t 0,`conn_live_tsc` 由 pong 刷新为 `now`;`now = t 10 000`
- 实现:`crates/hft-exec/tests/quote.rs::exe_116_quiet_book_prices`

| 行 | 订单 | 期望 |
|---|---|---|
| EXE-116.1 | g8 策略切片卖 P | 发出,`px = 99.31`;无 `StaleQuote` |
| EXE-116.2 | g7 补腿(多出 P −0.5) | 发出买 P 0.5,`px = 100.70` |
| EXE-116.3 | g7 自动有界平仓(Broken 到期,多出 P −0.5) | `AutoFix/Start` 与一片 |
| EXE-116.4 | g8 L3 引擎目标一步 | 发出 `ForceStep` |

### EXE-117 报价阈值取自 v.risk(a).conn_liveness_ms

- 规则:[报价可用](README.md#报价可用连接健康)
- 级别:table
- 里程碑:M2
- 前置:C0;R0 但 `conn_liveness_ms = 500`;g8 目标 P −10.0;`now = t 10 000`
- 实现:`crates/hft-exec/tests/quote.rs::exe_117_threshold_from_risk_view`

| 行 | `conn_live_tsc` | 期望 |
|---|---|---|
| EXE-117.1 | t 9500 | 出一片 |
| EXE-117.2 | t 9499 | 无订单 |

### EXE-118 簿非 Live 时深度只用 BBO 顶档

- 规则:[报价可用](README.md#报价可用连接健康)、[TakerSlice](README.md#takerslice)
- 级别:table
- 里程碑:M2
- 前置:C0;R0 但 `max_order_notional = 100 000 USDT`;T1 但 `depth_frac = 0.1`、`max_chunk_notional = 100 000 USDT`;g8 目标 P −100.0
- 实现:`crates/hft-exec/tests/quote.rs::exe_118_non_live_book_top_only`

| 行 | `v.book(P).status()` | 期望第一片 |
|---|---|---|
| EXE-118.1 | `Live` | 卖 P 25.0,`depth_qty = 250.0` |
| EXE-118.2 | 重同步中(非 `Live`) | 卖 P 5.0,`depth_qty = 50.0` |
| EXE-118.3 | 无 L2 簿 | 卖 P 5.0 |

### EXE-119 引擎单遇报价不可用:门控等待、不计次,补腿截止照算

- 规则:[报价可用](README.md#报价可用连接健康)、[补腿](README.md#补腿hedgefixc4)
- 级别:unit
- 里程碑:M2
- 前置:C0(`fix_deadline_ms = 10 000`、`fix_max = 3`);g7 链接已结束、静止,腿净量 S +1.00、P −1.5(多出 P,裸量 0.5,名义 50 USDT,裸量比 0.33);自 t 0 起 P 报价不可用
- 步骤:
  1. t 0 `on_leg_adjust(a1, g7)`;`advance`
  2. 每 500 ms 推进一次直到 t 10 001
- 期望:
  - t 0 记 `fix.first_tsc = t 0`;t 0–t 10 000 无订单、`fix.count = 0`
  - t 10 001 写 `ExecDecision{State, state_to = Broken, detail = 3 \| 3 << 8}`(`FixExhausted`)与 `Note{ExecBroken}` P0
- 实现:`crates/hft-exec/tests/quote.rs::exe_119_engine_quote_wait_deadline_runs`

### EXE-120 L3 与清仓的引擎目标遇报价不可用:只门控等待,不设截止

- 规则:[报价可用](README.md#报价可用连接健康)
- 级别:unit
- 里程碑:M2
- 前置:C0;g8 P 净量 −10.0;`engine_flatten(a1)`;自 t 0 起 P 报价不可用
- 步骤:
  1. 每 500 ms 推进,直到 t 60 000
- 期望:
  - 无订单;cell 不进入 Broken;`ExecDecision{State, state_to = Stuck, detail = 2 \| 4 << 8}` 在 t 1000(Urgent:2000 / 2)恰好一条,此后不重复
- 实现:`crates/hft-exec/tests/quote.rs::exe_120_force_quote_wait_no_deadline`

### EXE-125 越价参数按订单类别截取

- 规则:[限价与单笔名义](README.md#限价与单笔名义)
- 级别:table
- 里程碑:M2
- 前置:C0(`force_slice.max_cross = 0.005`);R0(`rr_max_cross = 0.01`、`ioc_max_cross = 0.005`);P 99.80 / 100.20、S 99.90 / 100.10,mid 100.00
- 实现:`crates/hft-exec/tests/price.rs::exe_125_max_cross_eff`

| 行 | 订单 | `slice.max_cross` | 期望限价 |
|---|---|---|---|
| EXE-125.1 | 策略 Open 买 P | 0.003 | 100.50(min(100.20 × 1.003, 101.00) = 100.5006 向下取 tick) |
| EXE-125.2 | 策略 Open 买 P | 0.008 | 100.70(截取到 `ioc_max_cross` 0.005) |
| EXE-125.3 | 策略 Reduce 卖 P | 0.008 | 99.01(max(99.80 × 0.992, 99.00) = 99.0016 向上取 tick) |
| EXE-125.4 | 策略 Reduce 卖 P | 0.02 | 99.00(截取到 `rr_max_cross` 0.01) |
| EXE-125.5 | 补腿卖 S(引擎单) | — | 99.41(`force_slice.max_cross` 0.005:99.4005 向上取 tick) |

### EXE-126 越价上限 rr_eff:L3/清仓/对齐/无组目标可取 l3_max_cross

- 规则:[限价与单笔名义](README.md#限价与单笔名义)
- 级别:table
- 里程碑:M2
- 前置:C0(`force_slice.max_cross = 0.005`);P 报价改为 98.50 / 101.50(mid 100.00);R0(`rr_max_cross = 0.01`)
- 实现:`crates/hft-exec/tests/price.rs::exe_126_rr_eff_l3`

| 行 | 订单 | `l3_max_cross` | 期望限价 |
|---|---|---|---|
| EXE-126.1 | g8 L3 引擎单卖 P | 0 | 99.00(max(98.0075, 100 × 0.99)) |
| EXE-126.2 | g8 L3 引擎单卖 P | 0.02 | 98.01(max(98.0075, 98.00) 向上取 tick) |
| EXE-126.3 | g8 L3 引擎单卖 P | 0.005 | 99.00(取 max(0.01, 0.005)) |
| EXE-126.4 | g7 补腿卖 P(非 MarginStress/Hard) | 0.02 | 99.00 |
| EXE-126.5 | 无组 cell 清仓单卖 P | 0.02 | 98.01 |

### EXE-127 单笔名义上限按订单类别

- 规则:[限价与单笔名义](README.md#限价与单笔名义)
- 级别:table
- 里程碑:M2
- 前置:C0 但 `force_slice.max_chunk_notional = 2000 USDT`;R0 但 `max_order_notional = 2000`、`max_reduce_notional = 1500`、`max_hedge_notional = 1600 USDT`;策略 `slice.max_chunk_notional = 1800 USDT`、`depth_frac = 0.5`;各行数量差均为 100(深度不约束)
- 实现:`crates/hft-exec/tests/price.rs::exe_127_cap_n_by_class`

| 行 | 订单 | 期望 `cap_n` | 期望数量 |
|---|---|---|---|
| EXE-127.1 | 策略 Open 买 P @ 100.70 | 1800 | 17.8(17.8 × 100.70 = 1792.46) |
| EXE-127.2 | 策略 Reduce 卖 P @ 99.31 | 1500 | 15.1(1499.581) |
| EXE-127.3 | L3 Flatten 买 P @ 100.70 | 1500 | 14.8(1490.36) |
| EXE-127.4 | 补腿卖 S @ 99.41(裸量 30.00) | 1600 | 16.09(1599.5069) |
| EXE-127.5 | 自动有界平仓卖 S @ 99.41 | 1500(min(1500, 1600)) | 15.08(1499.1028) |
| EXE-127.6 | 策略 Open,`slice.max_chunk_notional = 2500` | 2000(截取,不拒目标) | 19.8(1993.86) |

### EXE-128 非 USDT 计价按 quote_rate 折算并验证向上取整后不超 cap_n

- 规则:[限价与单笔名义](README.md#限价与单笔名义)
- 级别:table
- 里程碑:M2
- 前置:C0;P2(`InstId(23)`,USDC 计价,lot 0.1、tick 0.01,BBO 99.80 / 100.20)`quote_rate = 1.001`;策略 Open 买 P2 @ 100.70,`cap_n = 1800 USDT`
- 实现:`crates/hft-exec/tests/price.rs::exe_128_quote_rate`

| 行 | `quote_rate` | 期望数量 |
|---|---|---|
| EXE-128.1 | 1.0(USDT) | 17.8 |
| EXE-128.2 | 1.001 | 17.8(⌈17.8 × 100.70 × 1.001⌉ = 1794.252460 ≤ 1800) |
| EXE-128.3 | 1.02 | 17.5(1800 / 102.714 = 17.524;17.5 × 102.714 = 1797.495) |

### EXE-129 每笔订单的数量是满足名义上限的最大 lot 倍数

- 规则:[限价与单笔名义](README.md#限价与单笔名义)
- 级别:prop
- 里程碑:M2
- 性质:对任意 `px`、`quote_rate`、`cap_n`、lot,本节第 4 条算出的 `qty` 满足 `⌈qty × px × q⌉ ≤ cap_n`(USDT 1e-6 向上取整),且 `qty + lot` 不满足(除非被深度、差额或库存约束)
- 生成:`px` 0.0001–1e6(tick 1e-8–1),`q` 0.5–2.0(Rate 精度),`cap_n` 1–1e6 USDT,lot 1e-8–100
- 参考:i128 精确有理数计算的参考实现
- 例数:PR 10 000
- 实现:`crates/hft-exec/tests/props.rs::exe_129_qty_max_under_cap`

### EXE-130 越价上限按 inst 的价格带类别取值

- 规则:[限价与单笔名义](README.md#限价与单笔名义)
- 级别:table
- 里程碑:M2
- 前置:C0;P(类别 A:`rr_max_cross = 0.01`、`ioc_max_cross = 0.005`)与 P3(`InstId(24)`,类别 B:`rr_max_cross = 0.03`、`ioc_max_cross = 0.015`,BBO 99.80 / 100.20、tick 0.01);策略 Open 买,`slice.max_cross = 0.02`
- 实现:`crates/hft-exec/tests/price.rs::exe_130_per_inst_band_class`

| 行 | inst | 期望限价 |
|---|---|---|
| EXE-130.1 | P | 100.70(0.005) |
| EXE-130.2 | P3 | 101.70(min(100.20 × 1.015 = 101.703, 103.00) 向下取 tick) |

### EXE-131 引擎单被 PX_BAND 或 NOTIONAL_* 拒绝按执行失败处理并报 P0

- 规则:[限价与单笔名义](README.md#限价与单笔名义)
- 级别:table
- 里程碑:M2
- 前置:C0;g7 补腿单买 P 0.5 @ 100.70(多出 P −0.5,`cap_n = 1000`)已交出;mid 100.00;`fix.count = 0`、`fail_streak = 0`
- 实现:`crates/hft-exec/tests/price.rs::exe_131_engine_risk_reject`

| 行 | `on_place_result(Rejected(bits))` | 期望 |
|---|---|---|
| EXE-131.1 | `PX_BAND` | `ExecDecision{HedgeFix, detail = 5 \| 3 << 8, gate = PX_BAND}`;`Note{ExecEngineOrderRejected}` P0,`v = [0x01_0000_0701, 1, 5, 1 << 9, 0.5, 100.70, 100.00, 1000]`;`fix.count = 1`、`fail_streak = 1` |
| EXE-131.2 | `NOTIONAL_ORDER` | 同上,`gate = NOTIONAL_ORDER`,`v[3] = 1 << 10` |
| EXE-131.3 | `NOTIONAL_ACCOUNT` | 同上,`v[3] = 1 << 12` |
| EXE-131.4 | g8 L3 引擎单(`ForceStep`)被 `PX_BAND` 拒 | `ExecDecision{ForceStep, detail = 5 \| 3 << 8}`;`v[2] = 8` |
| EXE-131.5 | 补腿单被 `RATE_BACKOFF` 拒 | 门控:无 `ExecEngineOrderRejected`;`fix.count = 0`;`wake_at = now + 500 ms` |
| EXE-131.6 | 补腿单被 `HEDGE_LIMIT` 拒 | 不计 `RiskReject`:进入 Broken{HedgeVeto}(见 EXE-070) |

### EXE-132 策略切片被 PX_BAND 拒绝按门控处理

- 规则:[限价与单笔名义](README.md#限价与单笔名义)、[门控拒绝与退避](README.md#门控拒绝与退避)
- 级别:unit
- 里程碑:M2
- 前置:C0(`stuck_after_ms = 2000`、`gated_backoff_ms = 500`)、T1;g8 目标 P −10.0;每片都被 `PX_BAND` 拒
- 步骤:
  1. t 0 起每 500 ms 推进,直到 t 2000
- 期望:
  - 无 `Note{ExecEngineOrderRejected}`;`fail_streak = 0`;t 2000 `ExecDecision{State, state_to = Stuck, detail = 2 \| 1 << 8, gate = PX_BAND}`
- 实现:`crates/hft-exec/tests/price.rs::exe_132_strategy_px_band_is_gate`

### EXE-133 同一价格函数:每笔订单限价等于 cap_cross_px 的结果

- 规则:[限价与单笔名义](README.md#限价与单笔名义)、[规划源文件](README.md#规划源文件)
- 级别:prop
- 里程碑:M2
- 性质:任意 BBO、类别与参数下,每笔 `ExecOrder` 的限价等于以本节第 1 条的 `max_cross_eff`、`rr_eff` 调用 hft-types `cap_cross_px` 的结果;mock 风控按 #5/#6 的同一口径判定时,引擎单从不被 `PX_BAND`/`NOTIONAL_*` 拒
- 生成:BBO 价差 0–5%,mid 偏离 0–3%,`rr_max_cross` 0.001–0.05,`l3_max_cross` ∈ {0, 0.005–0.1},订单类别全部 9 类
- 参考:直接调用 `hft_types::cap_cross_px` 与 mock #5/#6
- 例数:PR 5 000
- 实现:`crates/hft-exec/tests/props.rs::exe_133_price_is_cap_cross_px`

### EXE-145 TakerSlice 切片量 = min(差额, 深度 × depth_frac, 名义上限折算量),按 lot 向零取整

- 规则:[TakerSlice](README.md#takerslice)
- 级别:table
- 里程碑:M2
- 前置:C0;R0 但 `max_order_notional = 2000`;g8 目标为负(卖 P,限价 99.31,`cap_n = 2000` → 20.1);除各行所改外 T1 但 `max_chunk_notional = 2000`、`depth_frac = 0.5`、`depth_levels = 5`
- 实现:`crates/hft-exec/tests/taker.rs::exe_145_chunk_formula`

| 行 | 差额 | 改动 | 期望第一片 |
|---|---|---|---|
| EXE-145.1 | 10.0 | — | 10.0 |
| EXE-145.2 | 100.0 | `depth_frac = 0.05` | 12.5 |
| EXE-145.3 | 100.0 | — | 20.1(名义上限) |
| EXE-145.4 | 3.37 | — | 3.3(向零取到 lot 0.1) |
| EXE-145.5 | 100.0 | `depth_levels = 2`,两个上限改为 100 000 | 50.0 |
| EXE-145.6 | 100.0 | `max_cross = 0.001`(价格范围 ≥ 99.7002,只计 99.80 一档),两个上限改为 100 000 | 25.0,`px = 99.71` |

### EXE-146 低于 min_chunk_notional 的切片:最后一笔照发,不足最小下单量交残余,其余等待

- 规则:[TakerSlice](README.md#takerslice)
- 级别:table
- 里程碑:M2
- 前置:C0(`stuck_after_ms = 2000`);T1 但 `min_chunk_notional = 50 USDT`、`band = 0`;g8 目标为负
- 实现:`crates/hft-exec/tests/taker.rs::exe_146_min_chunk_rules`

| 行 | 场景 | 期望 |
|---|---|---|
| EXE-146.1 | 差额 0.4(39.72 USDT < 50,≥ 交易所 5 USDT) | 发卖 P 0.4(本目标最后一笔) |
| EXE-146.2 | 差额 0.05(< `min_qty` 0.1) | 无订单 |
| EXE-146.3 | 差额 10.0,`depth_frac = 0.001`(切片 0.2,19.86 USDT < 50) | 无订单;t + 2000 `Stuck{NoLiquidity}`(`detail = 2 \| 3 << 8`) |
| EXE-146.4 | 差额 0.1,P 的 `min_notional = 20 USDT`(9.93 < 20) | 无订单 |

### EXE-147 TakerSlice 订单为 IOC,限价即算出的 px

- 规则:[TakerSlice](README.md#takerslice)
- 级别:unit
- 里程碑:M2
- 前置:C0、T1;g8 目标 P −10.0
- 步骤:
  1. `advance`
- 期望:
  - `ExecOrder.intent{tif = Ioc, px = 99.31, qty = 10.0, side = Sell, inst = P}`;`intent.link = None`
- 实现:`crates/hft-exec/tests/taker.rs::exe_147_ioc_limit`

### EXE-148 TakerSlice 各腿独立推进、各按自己的 lot,不做配对取整

- 规则:[TakerSlice](README.md#takerslice)
- 级别:unit
- 里程碑:M2
- 前置:C0;g7 改用 `TakerSlice`(T0 的 slice),目标 S +3.37、P −3.37;无在途
- 步骤:
  1. `set_target`;`advance`
- 期望:
  - 同一 `advance` 两条订单:买 S 3.37(lot 0.01)与卖 P 3.3(lot 0.1);两条都无 `LegLink`
- 实现:`crates/hft-exec/tests/taker.rs::exe_148_legs_independent`

### EXE-155 配对算法的腿数与算法匹配

- 规则:[PairOnFill](README.md#paironfill)、[目标的接受与替换](README.md#目标的接受与替换)
- 级别:table
- 里程碑:M2
- 前置:C0;g12 = `GroupId(0x0100_000C)` 三腿(S、P、Q = 永续 `InstId(31)` 于 venue C = `VenueId(3)`);g8 单腿
- 实现:`crates/hft-exec/tests/pair.rs::exe_155_leg_count`

| 行 | 目标 | 期望 |
|---|---|---|
| EXE-155.1 | g12,`PairOnFill`,3 条腿目标 | `Err(TooManyLegs)`,`detail = 1 \| 8 << 8` |
| EXE-155.2 | g12,`Simultaneous`,3 条腿 | `Err(TooManyLegs)` |
| EXE-155.3 | g8,`PairOnFill`,1 条腿 | `Err(SpecInvalid)`,`detail = 1 \| 3 << 8` |
| EXE-155.4 | g12,`TakerSlice`,3 条腿 | `Ok` |

### EXE-156 腿顺序按 reason × first_leg × 流动性

- 规则:[PairOnFill](README.md#paironfill)
- 级别:table
- 里程碑:M2
- 前置:C0、T0;g7 开仓目标 S +10 / P −10(或各行写明的平仓目标);对手侧可见深度名义:S 买方向 5 档 × 40 ≈ 20 060 USDT,P 卖方向 5 档 × 50 ≈ 24 900 USDT(S 流动性差)
- 实现:`crates/hft-exec/tests/pair.rs::exe_156_leg_order_table`

| 行 | `reason` | `first_leg` | 其他 | 期望 `first_leg_ix` |
|---|---|---|---|---|
| EXE-156.1 | Rebalance | ByLiquidity | — | 0(S) |
| EXE-156.2 | Rebalance | ByLiquidity | S 每档改为 80(S ≈ 40 120 > P) | 1(P) |
| EXE-156.3 | Rebalance | PerpFirst | — | 1 |
| EXE-156.4 | Rebalance | SpotFirst | — | 0 |
| EXE-156.5 | Rebalance | Fixed,`first_leg_ix = 1` | — | 1 |
| EXE-156.6 | Exit(平仓 S +10 → 0、P −10 → 0) | ByLiquidity | 平仓方向:S 卖 5 档 × 40,P 买 5 档 × 50 | 0 |
| EXE-156.7 | Manual | SpotFirst | — | 0 |
| EXE-156.8 | MarginStress(平仓) | SpotFirst | — | 1(强制 PerpFirst) |
| EXE-156.9 | Hard(平仓) | Fixed,`first_leg_ix = 0` | — | 1 |
| EXE-156.10 | Exit(平仓) | SpotFirst | a1 `force_active`(替换了引擎目标) | 1 |

### EXE-157 两腿都是永续时风险目标的先手腿

- 规则:[PairOnFill](README.md#paironfill)
- 级别:table
- 里程碑:M2
- 前置:C0;g11 = `GroupId(0x0100_000B)`:腿 0 = P(venue B,净量 −5.0),腿 1 = Q(venue C,净量 +5.0);两腿 BBO 与深度相同
- 实现:`crates/hft-exec/tests/pair.rs::exe_157_both_perp_first_leg`

| 行 | 场景 | 期望 `first_leg_ix` |
|---|---|---|
| EXE-157.1 | `force_deleverage(a1, C, 0.2)` 设下的引擎目标 | 1(Q,指定 venue) |
| EXE-157.2 | `engine_flatten(a1)`;`PortfolioView` 保证金率 B 0.50、C 0.70 | 1(C 较高) |
| EXE-157.3 | `engine_flatten(a1)`;保证金率相同,P 净量 −8.0、Q +5.0 | 0(名义较大) |

### EXE-158 风险目标没有永续腿或参数找不到对应市场类型时退回 ByLiquidity

- 规则:[PairOnFill](README.md#paironfill)
- 级别:table
- 里程碑:M2
- 前置:C0;g13 = `GroupId(0x0100_000D)` 两条现货腿 S(深度名义 ≈ 20 000)与 S2(`InstId(12)`,现货 venue D,深度名义 ≈ 30 000);g11 两条永续腿 P(≈ 24 900)与 Q(≈ 30 000)
- 实现:`crates/hft-exec/tests/pair.rs::exe_158_fallback_by_liquidity`

| 行 | 场景 | 期望 `first_leg_ix` |
|---|---|---|
| EXE-158.1 | g13,`reason = Hard` 平仓 | 0(S 流动性差) |
| EXE-158.2 | g13,`Rebalance`,`PerpFirst` | 0 |
| EXE-158.3 | g11,`Rebalance`,`SpotFirst` | 0(P 流动性差) |

### EXE-159 ByLiquidity 深度名义相同时取 leg_ix 小者

- 规则:[PairOnFill](README.md#paironfill)
- 级别:unit
- 里程碑:M2
- 前置:C0、T0;S 与 P 的对手侧 5 档深度名义调成相同(各 25 000.00 USDT)
- 步骤:
  1. `set_target`(S +10 / P −10);`advance`
- 期望:
  - `ExecDecision{Slice, first_leg_ix = 0}`
- 实现:`crates/hft-exec/tests/pair.rs::exe_159_by_liquidity_tie`

### EXE-160 第一腿量取两腿中较弱一侧的深度并折算对冲比

- 规则:[PairOnFill](README.md#paironfill)
- 级别:table
- 里程碑:M2
- 前置:C0;R0 但三个名义上限均为 100 000 USDT;T0 但 `max_chunk_notional = 100 000 USDT`、`depth_frac = 0.5`;先手 S(深度 200),第二腿 P(深度 250)
- 实现:`crates/hft-exec/tests/pair.rs::exe_160_first_qty_weak_side`

| 行 | 对冲比 | 目标 | 期望第一腿量 |
|---|---|---|---|
| EXE-160.1 | 1/1 | S +100 / P −100 | 100.0(min(200, 250) × 0.5),`depth_qty = 200.0` |
| EXE-160.2 | 2/1(`pair_lot` 0.05) | S +100 / P −200 | 62.50(min(200, 250 × 1/2) × 0.5),`depth_qty = 125.0` |

### EXE-161 第一腿量受第二腿最不利价名义 ≤ max_hedge_notional 约束

已删除:第二腿名义上界改为两个方向都按 mid × (1 + rr_max_cross),由 EXE-511 替代。

### EXE-162 第二腿方向取第二腿差额的符号;第二腿在带内时退化为无链接单腿切片

- 规则:[PairOnFill](README.md#paironfill)
- 级别:table
- 里程碑:M2
- 前置:C0、T0(`band = 0.01`);先手 S
- 实现:`crates/hft-exec/tests/pair.rs::exe_162_second_leg_side_and_degenerate`

| 行 | 净量 / 目标 | 期望 |
|---|---|---|
| EXE-162.1 | S 0 → +10、P 0 → −10 | 第一腿带 LegLink,`HedgePlan.side = Sell`,`kind = PairFirst` |
| EXE-162.2 | S +10 → 0、P −10 → 0 | `HedgePlan.side = Buy` |
| EXE-162.3 | S 0 → +10、P 已为 −10.0(在带内) | 买 S,`intent.link = None`,`ExecOrder.kind = Slice` |

### EXE-163 LinkNotice{Done} 清 active,下一笔按冷却排程

- 规则:[PairOnFill](README.md#paironfill)
- 级别:table
- 里程碑:M2
- 前置:C0、T0(`cooldown_ms = 300`);第一片在 t 0 发出,链接 L1;t 100 收到 `LinkNotice{L1, Done}`,目标未到达
- 实现:`crates/hft-exec/tests/pair.rs::exe_163_link_done_cooldown`

| 行 | 推进时刻 | 期望 |
|---|---|---|
| EXE-163.1 | t 100(随 LinkNotice) | `active.kind = None`;无订单 |
| EXE-163.2 | t 299 | 无订单 |
| EXE-163.3 | t 300 | 出第二片(新链接) |

### EXE-164 LINK_HOLD 由不带 RESUME_HOLD 的 LinkNotice 或链接终态清除

- 规则:[PairOnFill](README.md#paironfill)、[快照与恢复](README.md#快照与恢复)
- 级别:table
- 里程碑:M2
- 前置:C0、T0;g7 `active.link_id = 9`;`recovery_end` 后 cell 行 `flags & LINK_HOLD(8) ≠ 0`
- 实现:`crates/hft-exec/tests/pair.rs::exe_164_link_hold_clear`

| 行 | 投递 | 期望 `flags & 8` |
|---|---|---|
| EXE-164.1 | `LinkNotice{9, Hedging, flags = RESUME_HOLD}` | 仍为 8 |
| EXE-164.2 | `LinkNotice{9, Hedging, flags = 0}` | 0 |
| EXE-164.3 | `LinkNotice{9, Done, flags = RESUME_HOLD}` | 0(终态) |
| EXE-164.4 | `LinkNotice{9, Broken, flags = RESUME_HOLD}` | 0;cell Broken |

### EXE-165 第一腿已到带内而第二腿仍在带外:第二腿单腿尾笔

- 规则:[PairOnFill](README.md#paironfill)
- 级别:table
- 里程碑:M2
- 前置:C0、T0(`band = 0.01`);目标 S +10.00 / P −10.0;S 净量 +10.00;无链接、无在途
- 实现:`crates/hft-exec/tests/pair.rs::exe_165_second_leg_tail`

| 行 | P 净量 | 期望 |
|---|---|---|
| EXE-165.1 | −9.0(不足,增仓) | 卖 P 1.0,`kind = Slice`,`class = Open`,无 LegLink |
| EXE-165.2 | −10.5(多出,减仓) | 买 P 0.5,`kind = ResidualTrim`,`class = Hedge`,`hedge_cap_qty = 0.6` |

### EXE-166 第一腿被 LINK_STALE 拒绝按门控退避,不计执行失败

- 规则:[PairOnFill](README.md#paironfill)、[门控拒绝与退避](README.md#门控拒绝与退避)
- 级别:table
- 里程碑:M2
- 前置:C0(`gated_backoff_ms = 500`)、T0;第一腿在 t 0 发出并被 `Rejected(LINK_STALE)`
- 实现:`crates/hft-exec/tests/pair.rs::exe_166_link_stale_backoff`

| 行 | 推进时刻 | 期望 |
|---|---|---|
| EXE-166.1 | t 499 | 无订单 |
| EXE-166.2 | t 500 | 再发第一腿,数量不变(`fund_shrink = 0`) |
| EXE-166.3 | 任一时刻 | `fail_streak = 0`;`gated_bits = LINK_STALE` |

### EXE-167 LINK_FUNDS 逐次把第一腿量减半,上限 4 次,不低于 pair_min,被接受后清零

- 规则:[PairOnFill](README.md#paironfill)
- 级别:table
- 里程碑:M2
- 前置:C0、T0;未缩量时第一腿量 9.94 取整为 9.9;`pair_lot = pair_min = 0.1`;每行之前的各次第一腿都被 `Rejected(LINK_FUNDS)`
- 实现:`crates/hft-exec/tests/pair.rs::exe_167_link_funds_shrink`

| 行 | 已被拒次数 | 期望下一笔第一腿量 |
|---|---|---|
| EXE-167.1 | 0 | 9.9 |
| EXE-167.2 | 1 | 4.9(9.94 右移 1 位 = 4.97) |
| EXE-167.3 | 2 | 2.4(2.485) |
| EXE-167.4 | 3 | 1.2(1.2425) |
| EXE-167.5 | 4 | 0.6(0.62125) |
| EXE-167.6 | 5 | 0.6(`fund_shrink` 停在 4) |
| EXE-167.7 | 第 5 笔被接受后的下一笔 | 9.9(`fund_shrink = 0`) |
| EXE-167.8 | 2,未缩量的量为 0.3(深度受限) | 0.1(0.075 低于 `pair_min`,取 `pair_min`) |

### EXE-168 配对开仓的先手腿报价不可用时不发

- 规则:[PairOnFill](README.md#paironfill)、[报价可用](README.md#报价可用连接健康)
- 级别:unit
- 里程碑:M2
- 前置:C0、T0;S 的 `resync = true`,P 健康
- 步骤:
  1. `advance`
- 期望:
  - 无订单;`gated_since = now`
- 实现:`crates/hft-exec/tests/pair.rs::exe_168_first_leg_quote_blocks`

### EXE-169 链接的 tolerance 与 residual 更新 cell 的补腿阈值与残余策略

- 规则:[PairOnFill](README.md#paironfill)、[补腿](README.md#补腿hedgefixc4)
- 级别:unit
- 里程碑:M2
- 前置:C0(`fix_tolerance_default = 0.02`、`dust_notional = 5 USDT`)、T0(`fix_tolerance = 0.02`);链接结束后腿净量 S +1.08、P −1.0(裸量 0.08,名义 8 USDT,裸量比 0.074);静止
- 步骤:
  1. 投递 `LinkNotice{Done, tolerance = 0.10, residual = Hold}`;`advance`
  2. 另一实例:投递 `LinkNotice{Done, tolerance = 0.05, residual = Hold}`;`advance`
- 期望:
  - 步骤 1:无 `HedgeFix`(0.074 ≤ 0.10)
  - 步骤 2:`HedgeFix` 卖 S 0.08(0.074 > 0.05)
- 实现:`crates/hft-exec/tests/pair.rs::exe_169_link_tolerance_updates`

### EXE-180 配对第一腿量向零取整到 pair_lot 的整数倍

- 规则:[两腿 lot 对齐](README.md#两腿-lot-对齐)
- 级别:table
- 里程碑:M2
- 前置:C0;各行两腿 lot 与对冲比如列;第一腿在取整前的量(深度与名义约束后)为 c;`pair_min` 不约束
- 实现:`crates/hft-exec/tests/lots.rs::exe_180_first_leg_pair_lot`

| 行 | `lot_f` / `lot_h` | 对冲比 | c | 期望 `pair_lot` | 期望第一腿量 |
|---|---|---|---|---|---|
| EXE-180.1 | 0.001 / 0.01 | 1/1 | 0.0377 | 0.01 | 0.03 |
| EXE-180.2 | 0.01 / 0.001 | 1/1 | 0.0377 | 0.01 | 0.03 |
| EXE-180.3 | 0.003 / 0.002 | 1/1 | 0.0377 | 0.006 | 0.036 |
| EXE-180.4 | 0.01 / 0.1 | 2/1 | 0.377 | 0.05 | 0.35 |
| EXE-180.5 | 0.01 / 0.1 | 1/3 | 0.977 | 0.3 | 0.9 |
| EXE-180.6 | 0.001 / 0.001 | 1/1 | 0.0377 | 0.001 | 0.037 |

### EXE-181 第一腿量低于 pair_min 不发

- 规则:[两腿 lot 对齐](README.md#两腿-lot-对齐)
- 级别:table
- 里程碑:M2
- 前置:C0(`stuck_after_ms = 2000`);两腿 lot 0.003 / 0.002(`pair_lot` 0.006),第一腿 `min_qty` 0.003、第二腿 `min_qty` 0.01,两腿 `min_notional` 5 USDT,对手价 100.00 → `pair_min` = 0.054(0.006 的倍数中同时满足第二腿 ≥ 0.01 与名义 ≥ 5 USDT 的最小者)
- 实现:`crates/hft-exec/tests/lots.rs::exe_181_pair_min`

| 行 | 取整前第一腿量 | 期望 |
|---|---|---|
| EXE-181.1 | 0.053 | 无订单;持续 2000 ms 后 `Stuck{NoLiquidity}` |
| EXE-181.2 | 0.054 | 第一腿 0.054 |
| EXE-181.3 | 0.0599 | 第一腿 0.054 |
| EXE-181.4 | 0.060 | 第一腿 0.060 |

### EXE-182 价格上涨使一个 pair_lot 的名义超过切片上限时仍取一个 pair_lot,受风控单笔上限约束

- 规则:[两腿 lot 对齐](README.md#两腿-lot-对齐)
- 级别:table
- 里程碑:M2
- 前置:C0;两腿 lot 0.003 / 0.002(`pair_lot` 0.006),对手价 200 000(一个 `pair_lot` = 1200 USDT);`slice.max_chunk_notional = 1000`;目标属已接受的增仓目标(接受时价格 100 000)
- 实现:`crates/hft-exec/tests/lots.rs::exe_182_one_pair_lot_floor`

| 行 | `max_order_notional` | 期望 |
|---|---|---|
| EXE-182.1 | 2000 | 第一腿 0.006 |
| EXE-182.2 | 1100 | 无订单(1200 > 风控上限) |

### EXE-183 set_target 对配对增仓目标做 LotMismatch 校验,减仓与风险目标不做

- 规则:[两腿 lot 对齐](README.md#两腿-lot-对齐)、[目标的接受与替换](README.md#目标的接受与替换)
- 级别:table
- 里程碑:M2
- 前置:C0;g14 = `GroupId(0x0100_000E)`:腿 0 lot 0.003、腿 1 lot 0.002(不能整除,lcm 0.006),对冲比 1/1;`slice.max_chunk_notional = 600 USDT`、`max_order_notional = 2000`;第一腿 mark 100 000.00(lcm 名义 600.00)
- 实现:`crates/hft-exec/tests/lots.rs::exe_183_lot_mismatch_table`

| 行 | 目标与条件 | 期望 |
|---|---|---|
| EXE-183.1 | `Rebalance` 增仓 | `Ok`(600.00 不大于 600) |
| EXE-183.2 | `Rebalance` 增仓,mark 100 000.01(600.00006) | `Err(LotMismatch)`,`detail = 1 \| 10 << 8` |
| EXE-183.3 | `Manual` 增仓,mark 100 000.01 | `Err(LotMismatch)` |
| EXE-183.4 | `Exit`,mark 100 000.01 | `Ok` |
| EXE-183.5 | `Rebalance`,两腿都减仓,mark 100 000.01 | `Ok` |
| EXE-183.6 | `MarginStress`,mark 100 000.01 | `Ok` |
| EXE-183.7 | `Hard`,mark 100 000.01 | `Ok` |
| EXE-183.8 | 无 mark,BBO 中价 100 000.01 | `Err(LotMismatch)` |
| EXE-183.9 | 无 mark、无 BBO | `Ok`(跳过此项) |
| EXE-183.10 | `slice.max_chunk_notional = 2500`、`max_order_notional = 2000`,mark 333 333.34(lcm 名义 2000.00004) | `Err(LotMismatch)`(阈值取截取后的 2000) |
| EXE-183.11 | 腿 lot 0.001 / 0.01(能整除),mark 10 000 000 | `Ok` |

### EXE-184 pair_lot 溢出时增仓目标 LotMismatch

- 规则:[两腿 lot 对齐](README.md#两腿-lot-对齐)、[规划源文件](README.md#规划源文件)
- 级别:unit
- 里程碑:M2
- 前置:C0;两腿 lot 的 raw 值为 4 611 686 018 427 387 903 与 4 611 686 018 427 387 902(互素,`pair_lot` 超过 `MAX_RAW` → `None`)
- 步骤:
  1. `Rebalance` 增仓目标 `set_target`;`Exit` 目标 `set_target`
- 期望:
  - 前者 `Err(LotMismatch)`;后者 `Ok`
- 实现:`crates/hft-exec/tests/lots.rs::exe_184_pair_lot_overflow`

### EXE-185 减仓与风险目标在 lcm 名义过大时按较粗 lot 取整第一腿

- 规则:[两腿 lot 对齐](README.md#两腿-lot-对齐)、[残余与尘埃](README.md#残余与尘埃)
- 级别:table
- 里程碑:M2
- 前置:C0;g14(lot 0.003 / 0.002,lcm 0.006);`slice.max_chunk_notional = 500`;第一腿 mark 100 000(lcm 名义 600 > 500);取整前第一腿量 0.0167
- 实现:`crates/hft-exec/tests/lots.rs::exe_185_coarse_lot_for_reduce`

| 行 | 目标 | 期望第一腿量 |
|---|---|---|
| EXE-185.1 | `Exit` 平仓 | 0.015(按较粗 lot 0.003 向零取整) |
| EXE-185.2 | `Hard` 平仓 | 0.015 |
| EXE-185.3 | `Exit`,`slice.max_chunk_notional = 700`(lcm 名义 ≤ 阈值) | 0.012(按 `pair_lot` 0.006) |

### EXE-186 restore 后重算的 pair_lot 与快照前相同

- 规则:[两腿 lot 对齐](README.md#两腿-lot-对齐)、[快照与恢复](README.md#快照与恢复)
- 级别:unit
- 里程碑:M2
- 前置:C0;g14 增仓目标已接受(`pair_lot` 0.006)
- 步骤:
  1. `snapshot`;`restore`(同一 refdata 与 GroupTable)
  2. 两个实例以相同视图各 `advance` 一次
- 期望:
  - 两个实例的第一腿订单量相同,均为 0.006 的整数倍
- 实现:`crates/hft-exec/tests/lots.rs::exe_186_pair_lot_after_restore`

### EXE-187 执行层与 hft-types 共用同一 pair_lot 实现

- 规则:[两腿 lot 对齐](README.md#两腿-lot-对齐)、[规划源文件](README.md#规划源文件)
- 级别:prop
- 里程碑:M2
- 性质:任意两腿 lot 与对冲比,cell 中存入的 `pair_lot` 等于 `hft_types::pair_lot(lot_f, lot_h, num, den)`;为 `None` 时增仓目标 `LotMismatch`
- 生成:lot raw 1–1e12,`num`/`den` 1–100
- 参考:直接调用 hft-types 函数
- 例数:PR 5 000
- 实现:`crates/hft-exec/tests/props.rs::exe_187_pair_lot_shared_impl`

### EXE-188 配对第一腿全部成交时第二腿量是其 lot 的整数倍

- 规则:[两腿 lot 对齐](README.md#两腿-lot-对齐)
- 级别:prop
- 里程碑:M2
- 性质:任意深度、价格、成交拆分与扣费序列下,配对增仓目标的第一腿订单量恒为 `pair_lot` 的整数倍,且 `订单量 × num / den` 恰为 `lot_h` 的整数倍、≥ 第二腿 `min_qty`
- 生成:lot 组合从 {0.001, 0.002, 0.003, 0.01, 0.1} 两两抽取;对冲比 {1/1, 2/1, 1/3};深度 1–1000;价格 1–200 000
- 参考:断言整除关系
- 例数:PR 5 000
- 实现:`crates/hft-exec/tests/props.rs::exe_188_second_leg_lot_multiple`

### EXE-195 Simultaneous 第一腿带 HedgePlan{Simultaneous, on_fail}

- 规则:[Simultaneous](README.md#simultaneous)
- 级别:table
- 里程碑:M2
- 前置:C0;g7 目标规格 `Simultaneous{slice = T0.slice, on_fail, fix_tolerance 0.02, residual = TrimExcess}`,目标 S +10 / P −10
- 实现:`crates/hft-exec/tests/simul.rs::exe_195_simul_first_leg`

| 行 | `on_fail` | 期望第一腿 |
|---|---|---|
| EXE-195.1 | Reverse | `kind = SimulFirst`(3);`qty = 9.9`(同 PairOnFill 第 2 步);`LegLink.hedge{kind = Simultaneous, on_fail = Reverse}` |
| EXE-195.2 | Hold | 同上,`on_fail = Hold` |

### EXE-196 未启用 feature 时 MakerFirst 目标 AlgoUnsupported

- 规则:[MakerFirst(optional)](README.md#makerfirstoptional)、[目标的接受与替换](README.md#目标的接受与替换)
- 级别:unit
- 里程碑:M2
- 前置:C0;未启用 `maker` feature;g7 目标规格 `MakerFirst{…}`
- 步骤:
  1. `set_target`
- 期望:
  - `Err(AlgoUnsupported)`;`ExecDecision{TargetRejected, detail = 1 \| 4 << 8}`
- 实现:`crates/hft-exec/tests/simul.rs::exe_196_maker_first_unsupported`

### EXE-200 Hedge 类订单的 hedge_cap_qty = 已成交裸量 + 1 lot,其余订单为 0

- 规则:[对冲量上限](README.md#对冲量上限b6)
- 级别:table
- 里程碑:M2
- 前置:C0;g7 静止;多出的一腿与裸量如列
- 实现:`crates/hft-exec/tests/cap.rs::exe_200_hedge_cap_qty`

| 行 | 订单类型 | 多出腿 / 裸量 | 期望 `hedge_cap_qty` |
|---|---|---|---|
| EXE-200.1 | `HedgeFix` | P / 0.5 | 0.6 |
| EXE-200.2 | `ResidualTrim` | S / 0.06 | 0.07 |
| EXE-200.3 | `ExternalDefault` | P / 0.5 | 0.6 |
| EXE-200.4 | `CmdHedgeFix` | S / 0.30 | 0.31 |
| EXE-200.5 | `AutoFix` | P / 0.5 | 0.6 |
| EXE-200.6 | 策略 `Slice` / `PairFirst` | — | 0 |
| EXE-200.7 | `ForceStep`(L3、清仓、无组 cell) | — | 0 |

### EXE-201 Hedge 类订单数量从不超过 hedge_cap_qty

- 规则:[对冲量上限](README.md#对冲量上限b6)
- 级别:prop
- 里程碑:M2
- 性质:任意序列中每条 Hedge 类订单 `qty ≤ hedge_cap_qty`,且 `hedge_cap_qty` 只由已成交净量算出(不含在途)
- 生成:同 EXE-050
- 参考:断言
- 例数:PR 2 000
- 实现:`crates/hft-exec/tests/props.rs::exe_201_hedge_qty_le_cap`
### EXE-205 链接结束后裸量超过阈值:对多出的一腿发 HedgeFix

- 规则:[补腿](README.md#补腿hedgefixc4)
- 级别:unit
- 里程碑:M2
- 前置:C0(`dust_notional = 5 USDT`、`fix_cooldown_ms = 1000`);T0(`fix_tolerance = 0.02`);g7 链接 L1 刚结束,腿净量 S +1.00、P −1.5,无在途、无 Unknown;`now = t 1000`
- 步骤:
  1. `on_link(LinkNotice{L1, Done, tolerance = 0.02})`;`advance`
- 期望:
  - `ExecOrder{kind = HedgeFix(5), origin = Engine, side = Buy, inst = P, qty = 0.5, intent.tif = Ioc, intent.px = 100.70, intent.link.role = HedgeFix, hedge_cap_qty = 0.6, reason = Rebalance}`
  - `ExecDecision{HedgeFix, leg_ix = 1, qty = 0.5}`;`fix.first_tsc = t 1000`;`ExecOut.timers` 含 `Timer{Exec, Fix}` 到期 t 2000
- 实现:`crates/hft-exec/tests/fix.rs::exe_205_hedge_fix_after_link`

### EXE-206 补腿触发的两个阈值:尘埃线与裸量比,边界上不触发

- 规则:[补腿](README.md#补腿hedgefixc4)
- 级别:table
- 里程碑:M2
- 前置:C0(`dust_notional = 5 USDT`、`fix_tolerance_default = 0.05`);mark 100.00;g7 静止;cell `tolerance` 如列;S 的 `min_notional = 4 USDT`(.2 的 0.05 S 按封顶卖价 99.41 折 4.97 USDT)
- 实现:`crates/hft-exec/tests/fix.rs::exe_206_trigger_thresholds`

| 行 | 净量 S / P | cell `tolerance` | 期望 |
|---|---|---|---|
| EXE-206.1 | +1.05 / −1.0(裸量 0.05,5 USDT) | 0.02 | 不触发 |
| EXE-206.2 | +1.05000001 / −1.0 | 0.02 | 卖 S 0.05 |
| EXE-206.3 | +10.00 / −9.8(裸量比 0.02) | 0.02 | 不触发 |
| EXE-206.4 | +10.00 / −9.79999999(裸量比 0.020000001) | 0.02 | 卖 S 0.20 |
| EXE-206.5 | +10.00 / −9.6(裸量比 0.04) | 0(用 `fix_tolerance_default` 0.05) | 不触发 |
| EXE-206.6 | +10.00 / −9.4(裸量比 0.06) | 0 | 卖 S 0.60 |
| EXE-206.7 | +10.00 / −9.5(裸量比 = 0.05) | 0 | 不触发 |

### EXE-207 补腿的每个抑制条件:不发、不计次

- 规则:[补腿](README.md#补腿hedgefixc4)
- 级别:table
- 里程碑:M2
- 前置:C0;g7 腿净量 S +1.00、P −1.5(满足触发);`fix.count = 0`、`fix.first_tsc = 0`;每行只让所列一项成立
- 实现:`crates/hft-exec/tests/fix.rs::exe_207_suppressions`

| 行 | 成立的条件 | 期望 |
|---|---|---|
| EXE-207.1 | `legs.open_links = 1` | 无订单;`count = 0`;`first_tsc = 0` |
| EXE-207.2 | `legs.has_unknown = true` | 同 .1 |
| EXE-207.3 | S `inflight_immediate = +0.1` | 同 .1 |
| EXE-207.4 | 只有 S `inflight_resting = +0.5`(已 Ack 挂单) | 不抑制:买 P 0.5 |
| EXE-207.5 | `cap_block = BROKEN` | 无 `HedgeFix`(交 Broken 处置) |
| EXE-207.6 | `cap_block = REORG` | 无订单;`count = 0` |
| EXE-207.7 | `v.risk.hard = true` | 无订单;`FixState` 全零 |
| EXE-207.8 | `active.kind = Slice`(本 cell 有在途切片) | 无订单;`first_tsc = 0` |
| EXE-207.9 | `serial = SERIAL_FALLBACK` | 无订单 |
| EXE-207.10 | `ext_deadline` 未到期 | 无订单;`first_tsc = 0` |
| EXE-207.11 | `hold = MANUAL_HOLD` | 无订单;`first_tsc = 0` |
| EXE-207.12 | a1 的 `venue_open` 无 B(多出腿 P 在 B) | 无订单;`first_tsc = now`(只作告警起点) |
| EXE-207.13 | a1 的 `venue_open` 无 A、有 B(多出腿 P 在已放行的 B) | 买 P 0.5 |

### EXE-208 HARD 期间清空 FixState;HARD 解除本身不产生订单

- 规则:[补腿](README.md#补腿hedgefixc4)、[HARD 解除时的 Broken 判定](README.md#hard-解除时的-broken-判定resolve_broken)
- 级别:unit
- 里程碑:M2
- 前置:C0;g7 腿净量 S +1.00、P −1.5,`fix.count = 2`、`fix.first_tsc = t 0`;`now = t 3000`
- 步骤:
  1. `on_risk(a1, CapSet{Kill, hard})`;`advance`
  2. t 9000:`on_risk(a1, CapCleared{Kill, hard})`、`on_risk(a1, CapSet{Broken, g7})`(sequencer 按 `resolve_broken` 为真所设);`advance`
- 期望:
  - 步骤 1:无订单;`fix.count = 0`、`fix.first_tsc = 0`
  - 步骤 2:无 `HedgeFix` 单;cell Broken,`broken_since = t 9000`、`auto_fix.due_tsc = t 129 000`
- 实现:`crates/hft-exec/tests/fix.rs::exe_208_hard_clears_fixstate`

### EXE-209 多出的一腿:与裸量同号且 |净量| 最大,并列时取支持 reduceOnly 的腿

- 规则:[补腿](README.md#补腿hedgefixc4)
- 级别:table
- 里程碑:M2
- 前置:C0;各 group 静止;Q = 永续 `InstId(31)`(venue C,lot 0.1,支持 reduceOnly)
- 实现:`crates/hft-exec/tests/fix.rs::exe_209_excess_leg_choice`

| 行 | 腿净量 | 期望 |
|---|---|---|
| EXE-209.1 | S +1.50、P −1.0 | 卖 S 0.50 |
| EXE-209.2 | S +1.00、P −1.5 | 买 P 0.5 |
| EXE-209.3 | P −2.0、Q +2.5 | 卖 Q 0.5(reduce-only) |
| EXE-209.4 | 三腿 P +1.0、S +1.00、Q −1.5 | 卖 P 0.5(P 与 S 并列,P 支持 reduceOnly) |

### EXE-210 补腿数量不超过该腿 |净量|,现货另不超过 min(group 库存, 可用余额)

- 规则:[补腿](README.md#补腿hedgefixc4)
- 级别:table
- 里程碑:M2
- 前置:C0;各 group 静止
- 实现:`crates/hft-exec/tests/fix.rs::exe_210_fix_qty_caps`

| 行 | 场景 | 期望 |
|---|---|---|
| EXE-210.1 | 三腿 S +0.30、S2 +0.29、P −0.1(裸量 0.49,多出腿 S) | 卖 S 0.30 |
| EXE-210.2 | S +1.50、P −1.0,S 库存 1.50、可用 0.20 | 卖 S 0.20 |
| EXE-210.3 | S +1.50、P −1.0,S 库存 0.10、可用 0.20 | 卖 S 0.10 |

### EXE-211 补腿量低于该腿最小下单量:不发单,交残余规则

- 规则:[补腿](README.md#补腿hedgefixc4)、[残余与尘埃](README.md#残余与尘埃)
- 级别:unit
- 里程碑:M2
- 前置:C0;本用例 S 的 `min_qty = 0.1`;T0(`residual = Hold`);g7 腿净量 S +1.08、P −1.0(裸量 0.08,8 USDT,裸量比 0.074)
- 步骤:
  1. `on_leg_adjust(a1, g7)`;`advance`
- 期望:
  - 无订单;`fix.count = 0`;无 `Note{ExecBroken}`
- 实现:`crates/hft-exec/tests/fix.rs::exe_211_below_min_order_no_fix`

### EXE-212 补腿计次:执行结果类失败计次,门控拒绝不计次

- 规则:[补腿](README.md#补腿hedgefixc4)、[门控拒绝与退避](README.md#门控拒绝与退避)
- 级别:table
- 里程碑:M2
- 前置:C0;g7 补腿单买 P 0.5 已交出,`fix.count = 0`
- 实现:`crates/hft-exec/tests/fix.rs::exe_212_fix_count_by_outcome`

| 行 | 结果 | 期望 |
|---|---|---|
| EXE-212.1 | 交易所 `Reject`(`on_order_update`) | `count = 1`、`fail_streak = 1` |
| EXE-212.2 | IOC 终态零成交 | `count = 1`、`nofill_streak = 1` |
| EXE-212.3 | 风控 `PX_BAND`(引擎单 `RiskReject`) | `count = 1` |
| EXE-212.4 | OMS 本地发送失败 `EncodeError`(槽 `Rejected`) | `count = 1` |
| EXE-212.5 | OMS 本地发送失败 `KillBlocked` / `RateLimited` / `NoConn` / `RingFull`(各一例) | `count = 0`;`wake_at = now + 500 ms` |
| EXE-212.6 | 风控 `RATE_BACKOFF` | `count = 0` |
| EXE-212.7 | 部分成交 0.2 后终态 | `count = 0`;`fail_streak = 0`、`nofill_streak = 0` |

### EXE-213 两次自动补腿的间隔不小于 fix_cooldown_ms

- 规则:[补腿](README.md#补腿hedgefixc4)
- 级别:table
- 里程碑:M2
- 前置:C0(`fix_cooldown_ms = 1000`);g7 第一次补腿在 t 0 发出、t 5 零成交终态(`count = 1`),裸量仍超阈值
- 实现:`crates/hft-exec/tests/fix.rs::exe_213_fix_cooldown`

| 行 | 推进时刻 | 期望 |
|---|---|---|
| EXE-213.1 | t 999 | 无订单 |
| EXE-213.2 | t 1000(`Timer{Exec, Fix}`) | 第二次补腿 |

### EXE-214 补腿截止 fix_deadline_ms:超过才判 Broken{FixExhausted}

- 规则:[补腿](README.md#补腿hedgefixc4)
- 级别:table
- 里程碑:M2
- 前置:C0(`fix_deadline_ms = 10 000`、`fix_max = 3`);g7 自 t 0 满足触发(`first_tsc = t 0`),每次补腿都被 `RATE_BACKOFF` 门控拒绝(不计次)
- 实现:`crates/hft-exec/tests/fix.rs::exe_214_fix_deadline`

| 行 | 推进时刻 | 期望 |
|---|---|---|
| EXE-214.1 | t 9999 | 不 Broken |
| EXE-214.2 | t 10 000 | 不 Broken |
| EXE-214.3 | t 10 001 | `ExecDecision{State, state_to = Broken, detail = 3 \| 3 << 8}`;`Note{ExecBroken}` P0;`ExecOut.broken` 一条 |

### EXE-215 补腿计次达到 fix_max 判 Broken{FixExhausted}

- 规则:[补腿](README.md#补腿hedgefixc4)
- 级别:table
- 里程碑:M2
- 前置:C0(`fix_max = 3`、`fix_cooldown_ms = 1000`);g7 每次补腿都被交易所拒绝
- 实现:`crates/hft-exec/tests/fix.rs::exe_215_fix_max`

| 行 | 第 n 次拒绝之后 | 期望 |
|---|---|---|
| EXE-215.1 | 2 | `count = 2`;非 Broken;t + 1000 再发 |
| EXE-215.2 | 3 | Broken{FixExhausted};`Note{ExecBroken}` P0;不再发补腿 |

### EXE-216 裸量回到阈值内时 FixState 清零

- 规则:[补腿](README.md#补腿hedgefixc4)
- 级别:unit
- 里程碑:M2
- 前置:C0;g7 `fix.count = 2`、`first_tsc = t 0`;腿净量 S +1.00、P −1.5
- 步骤:
  1. 外部入账使 P −1.0(`on_leg_adjust`);`advance`
- 期望:
  - `fix.count = 0`、`first_tsc = 0`、`next_tsc = 0`;无订单
- 实现:`crates/hft-exec/tests/fix.rs::exe_216_fixstate_reset`

### EXE-220 hedge_fix_cmd 立即出单,不受计次、冷却、Broken、manual_hold 与恢复期限制

- 规则:[补腿](README.md#补腿hedgefixc4)、[人工保持与恢复期抑制](README.md#人工保持与恢复期抑制)
- 级别:table
- 里程碑:M2
- 前置:C0;g7 静止,腿净量 S +1.00、P −1.5;各行另加所列状态
- 实现:`crates/hft-exec/tests/fix.rs::exe_220_cmd_bypasses_suppression`

| 行 | 状态 | 期望(`hedge_fix_cmd(a1, g7)`) |
|---|---|---|
| EXE-220.1 | 上次自动补腿在 100 ms 前(冷却中),`count = 2` | `Ok`;`ExecOrder{kind = CmdHedgeFix(9), side = Buy, inst = P, qty = 0.5, hedge_cap_qty = 0.6}`;`ExecDecision{CmdHedgeFix, origin = Engine(Command)}` |
| EXE-220.2 | Broken,`auto_fix.phase = Primed` | `Ok`;同上;`auto_fix.phase = Cancelled` |
| EXE-220.3 | `hold = MANUAL_HOLD` | `Ok`;同上;`hold & MANUAL_HOLD = 0`;`Note{ExecManualHold}` 级别 Info,`v = [0x01_0000_0701, 0, 2]` |
| EXE-220.4 | a1 `venue_open` 无 B | `Ok`;同上 |
| EXE-220.5 | `cap_block = REORG` | `Ok`;同上 |
| EXE-220.6 | S +10.00、P −9.9(裸量 0.1,名义 10 USDT > 尘埃,裸量比 0.01 ≤ `tol`) | `Ok`;卖 S 0.10(命令不看 `tol`) |

### EXE-221 hedge_fix_cmd 的拒绝原因

- 规则:[补腿](README.md#补腿hedgefixc4)
- 级别:table
- 里程碑:M2
- 前置:C0;g7 腿净量 S +1.00、P −1.5,除各行所改
- 实现:`crates/hft-exec/tests/fix.rs::exe_221_cmd_errors`

| 行 | 条件 | 期望 |
|---|---|---|
| EXE-221.1 | `v.risk.hard = true` | `Err(ExecCmdErr::Hard)`;无订单、无 `ExecDecision` |
| EXE-221.2 | `hedge_fix_cmd(a1, GroupId(0x0100_0099))` | `Err(UnknownGroup)` |
| EXE-221.3 | 净量 S +1.05、P −1.0(5 USDT = 尘埃) | `Err(NothingToFix)` |
| EXE-221.4 | P 的 `min_qty = 1.0`,裸量 0.5 | `Err(NothingToFix)` |
| EXE-221.5 | `open_links = 1` | `Err(NotQuiescent)` |
| EXE-221.6 | `has_unknown = true` | `Err(NotQuiescent)` |
| EXE-221.7 | P `inflight_immediate = +0.1` | `Err(NotQuiescent)` |
| EXE-221.8 | P 报价不可用 | `Err(QuoteUnavailable)` |
| EXE-221.9 | 拒绝时 cell 有 `MANUAL_HOLD` | `hold` 不变(只有成功的命令解除) |

### EXE-222 命令取消本 Broken 期的自动有界平仓,再次进入 Broken 才重新计时

- 规则:[补腿](README.md#补腿hedgefixc4)、[Broken 处置](README.md#broken-处置)
- 级别:unit
- 里程碑:M2
- 前置:C0(`broken_auto_fix_ms = 120 000`);g7 在 t 0 Broken,`Primed`
- 步骤:
  1. t 10 000 `hedge_fix_cmd`,成交后裸量仍 > 尘埃(部分成交)
  2. t 130 000 推进
  3. t 140 000 `CapCleared{Broken}`;t 150 000 `CapSet{Broken}`
- 期望:
  - 步骤 2:无 `AutoFix` 决定、无订单(`phase = Cancelled`)
  - 步骤 3:`phase = Primed`、`due_tsc = t 270 000`
- 实现:`crates/hft-exec/tests/fix.rs::exe_222_cmd_cancels_autofix`

### EXE-230 resolve_broken 的判定

- 规则:[HARD 解除时的 Broken 判定](README.md#hard-解除时的-broken-判定resolve_broken)
- 级别:table
- 里程碑:M2
- 前置:C0(`dust_notional = 5 USDT`);T0(`fix_tolerance = 0.02`,cell `tolerance = 0.02`);mark 100.00
- 实现:`crates/hft-exec/tests/resolve.rs::exe_230_resolve_broken_table`

| 行 | 状态 | 期望 `resolve_broken(a1, g7, v)` |
|---|---|---|
| EXE-230.1 | 净量 S +1.00、P −1.5,`open_links = 0`,无保持 | true |
| EXE-230.2 | 同 .1,`open_links = 1` | false |
| EXE-230.3 | 同 .1,`hold = MANUAL_HOLD` | false |
| EXE-230.4 | S +10.00、P −9.8(裸量比 = 0.02) | false |
| EXE-230.5 | S +10.00、P −9.79999999 | true |
| EXE-230.6 | S +1.05、P −1.0(名义 = 尘埃) | false |
| EXE-230.7 | S +1.05000001、P −1.0 | true |
| EXE-230.8 | 有 `has_unknown`、无活动链接、裸量超阈值 | true |

### EXE-231 resolve_broken 只读:不改状态、不写输出

- 规则:[HARD 解除时的 Broken 判定](README.md#hard-解除时的-broken-判定resolve_broken)、[入口](README.md#入口全部在-sequencer-线程调用无分配)
- 级别:unit
- 里程碑:M2
- 前置:C0;g7 同 EXE-230.1
- 步骤:
  1. 快照 X;调 `resolve_broken` 1000 次;快照 Y
- 期望:
  - X 与 Y 逐字节相同;`has_pending()` 不变
- 实现:`crates/hft-exec/tests/resolve.rs::exe_231_read_only`

### EXE-232 非 HARD 的 Kill 解除不判 Broken,补腿照常

- 规则:[HARD 解除时的 Broken 判定](README.md#hard-解除时的-broken-判定resolve_broken)、[运行模式、Drain 与 Unknown](README.md#运行模式drain-与-unknown)
- 级别:unit
- 里程碑:M2
- 前置:C0;a1 非 HARD `Kill{Account}` 生效(`open_blocked = true`);g7 静止,腿净量 S +1.00、P −1.5
- 步骤:
  1. `advance`
  2. `on_risk(a1, CapCleared{Kill})`;`advance`
- 期望:
  - 步骤 1:发 `HedgeFix` 买 P 0.5(Kill 期间补腿照常)
  - 步骤 2:无 `State → Broken`;cell 不进入 Broken
- 实现:`crates/hft-exec/tests/resolve.rs::exe_232_soft_kill_release_no_broken`

### EXE-237 进入 Broken 的四个来源都开始计时,已有 manual_hold 时转 WaitHold

- 规则:[Broken 处置](README.md#broken-处置)
- 级别:table
- 里程碑:M2
- 前置:C0(`broken_auto_fix_ms = 120 000`);g7 非 Broken;`now = t 1000`;多 cell 入口(`on_risk`)之后调 `advance` 写出记录
- 实现:`crates/hft-exec/tests/broken.rs::exe_237_broken_entry_timing`

| 行 | 来源 | 期望 |
|---|---|---|
| EXE-237.1 | 补腿计次耗尽(本 crate 的 `BrokenReq`) | `broken_since = t 1000`、`phase = Primed`、`due_tsc = t 121 000`;`ExecOut.timers` 含 `Timer{Exec, AutoFix}`(`timer_id = 5 \| 1 << 8`)到期 t 121 000 |
| EXE-237.2 | `LinkNotice{Broken}` | 同 .1 |
| EXE-237.3 | `on_place_result(Rejected(HEDGE_LIMIT))` | 同 .1 |
| EXE-237.4 | `on_risk(CapSet{Broken, g7})` | 同 .1 |
| EXE-237.5 | 同 .4,cell 已有 `MANUAL_HOLD` | `phase = WaitHold`;`broken_since = t 1000` |

### EXE-238 到期边界:due_tsc − 1 ms 不开始,到期即开始

- 规则:[Broken 处置](README.md#broken-处置)
- 级别:table
- 里程碑:M2
- 前置:C0;g7 t 0 进入 Broken,`due_tsc = t 120 000`;腿净量 S +1.00、P −1.5,静止,无等待条件
- 实现:`crates/hft-exec/tests/broken.rs::exe_238_due_boundary`

| 行 | 推进时刻 | 期望 |
|---|---|---|
| EXE-238.1 | t 119 999 | 无 `AutoFix` 决定、无订单 |
| EXE-238.2 | t 120 000(`Timer{Exec, AutoFix}`) | `AutoFix/Start` 与第一片 |
| EXE-238.3 | t 120 001(未在 .2 推进) | 同 .2 |

### EXE-239 开始时写 Start 决定并锁定 budget、方向与可执行时长起点

- 规则:[Broken 处置](README.md#broken-处置)
- 级别:unit
- 里程碑:M2
- 前置:C0;g7 Broken,`due_tsc = t 120 000`;腿净量 S +1.00、P −1.55(裸量 0.55)
- 步骤:
  1. t 120 000 推进
- 期望:
  - `phase = Running`、`started_tsc = run_mark_tsc = t 120 000`、`exec_ms = 0`、`leg_ix = 1`、`start_sign = −1`、`budget = 0.5`(0.55 向零取 lot 0.1)
  - `ExecDecision{AutoFix, origin = Engine(AutoFix), detail = 4 \| 1 << 8, leg_ix = 1, target_qty = 0.5}`
- 实现:`crates/hft-exec/tests/broken.rs::exe_239_start_records`

### EXE-240 budget 只取开始时的已成交裸量,不受当时可用余额限制

- 规则:[Broken 处置](README.md#broken-处置)
- 级别:table
- 里程碑:M2
- 前置:C0;g7 Broken 已到期,静止
- 实现:`crates/hft-exec/tests/broken.rs::exe_240_budget`

| 行 | 状态 | 期望 `budget` |
|---|---|---|
| EXE-240.1 | S +1.00、P −1.55 | 0.5 |
| EXE-240.2 | S +1.555、P −1.0;S 库存 1.555、可用 1.555 | 0.55 |
| EXE-240.3 | 同 .2,S 库存 0.40 | 0.40 |
| EXE-240.4 | 同 .2,S 库存 1.555、可用 0.10 | 0.55(第一片 ≤ 0.10) |
| EXE-240.5 | 开始后外来成交使 S 净量 +2.00(裸量 1.00) | 仍为 0.55(此后不增加) |

### EXE-241 开始前的每个等待条件:不开始、不算失败,条件消失的那次推进立即开始

- 规则:[Broken 处置](README.md#broken-处置)
- 级别:table
- 里程碑:M2
- 前置:C0;g7 Broken,`due_tsc = t 120 000`;腿净量 S +1.00、P −1.5(多出 P);自 t 100 000 起只让所列一项成立,t 130 000 解除
- 实现:`crates/hft-exec/tests/broken.rs::exe_241_start_wait_conditions`

| 行 | 等待条件 | 期望 |
|---|---|---|
| EXE-241.1 | `legs.has_pending = true` | t 120 000–t 129 999 每次推进 `phase = Primed`、无决定;t 130 000 `AutoFix/Start` |
| EXE-241.2 | `legs.has_unknown = true` | 同 .1 |
| EXE-241.3 | S `inflight_immediate = +0.1` | 同 .1 |
| EXE-241.4 | `open_links = 1` | 同 .1 |
| EXE-241.5 | a1 `venue_open` 无 B | 不开始;放行在 t 130 000 → `due_tsc = t 250 000`(见 EXE-248) |
| EXE-241.6 | cell 有 L3 引擎目标 | 同 .1(引擎目标在 t 130 000 清除) |
| EXE-241.7 | `cap_block = REORG` | 同 .1 |
| EXE-241.8 | P 报价不可用 | 同 .1 |
| EXE-241.9 | `risk_for(a1, P).rate_backoff = true` | 同 .1 |
| EXE-241.10 | P ask 101.10 > mid × 1.01 = 101.00(越过上限) | 同 .1 |
| EXE-241.11 | 多出腿为 S(净量 S +1.50、P −1.0),S 可用余额 0.005、库存 1.50 | 同 .1;不判 `MinOrder` |

### EXE-242 对手价越价判定的边界

- 规则:[Broken 处置](README.md#broken-处置)、[限价与单笔名义](README.md#限价与单笔名义)
- 级别:table
- 里程碑:M2
- 前置:C0;R0(`rr_max_cross = 0.01`、`l3_max_cross = 0`);g7 Broken 已到期,静止;mid 100.00
- 实现:`crates/hft-exec/tests/broken.rs::exe_242_beyond_cross_boundary`

| 行 | 多出腿与报价 | 期望 |
|---|---|---|
| EXE-242.1 | 多出 P(买),ask 101.00 | 开始;限价 101.00 |
| EXE-242.2 | 多出 P,ask 101.01 | 不开始 |
| EXE-242.3 | 多出 S(卖),bid 99.00 | 开始;限价 99.00 |
| EXE-242.4 | 多出 S,bid 98.99 | 不开始 |

### EXE-243 其余等待条件不影响 due_tsc,到期后条件一消失立即开始

- 规则:[Broken 处置](README.md#broken-处置)
- 级别:unit
- 里程碑:M2
- 前置:C0;g7 t 0 Broken(`due_tsc = t 120 000`);P(多出腿)报价自 t 50 000 至 t 150 000 不可用
- 步骤:
  1. t 50 000–t 150 000 每 10 000 ms 推进;t 150 000 报价恢复后推进
- 期望:
  - t 150 000 之前无 `AutoFix` 决定;`due_tsc` 始终为 t 120 000;t 150 000 写 `AutoFix/Start`
- 实现:`crates/hft-exec/tests/broken.rs::exe_243_other_waits_keep_due`

### EXE-244 开始时已到尘埃线以下直接按成功结束

- 规则:[Broken 处置](README.md#broken-处置)
- 级别:unit
- 里程碑:M2
- 前置:C0(`dust_notional = 5 USDT`);g7 Broken 已到期,静止;腿净量 S +1.04、P −1.0(4 USDT)
- 步骤:
  1. 推进
- 期望:
  - `phase = Done`;`ExecDecision{AutoFix, detail = 4 \| 3 << 8}`;`Note{ExecAutoFixDone}` P1,`v = [0x01_0000_0701, 0, 0, 0.04, 0.04, 0]`;无订单
- 实现:`crates/hft-exec/tests/broken.rs::exe_244_start_below_dust_done`

### EXE-245 开始时 budget 低于最小下单量按 MinOrder 失败

- 规则:[Broken 处置](README.md#broken-处置)
- 级别:unit
- 里程碑:M2
- 前置:C0;本用例 P 的 `min_qty = 1.0`;g7 Broken 已到期,静止;腿净量 S +1.00、P −1.5
- 步骤:
  1. 推进
- 期望:
  - `phase = Failed`、`fail = 2`;`ExecDecision{AutoFix, detail = 4 \| 4 << 8 \| 2 << 16}`;`Note{ExecAutoFixFailed}` P0,`v = [0x01_0000_0701, 1, 2, 0, 0, 0.5, 0.5]`;无订单;cell 保持 Broken
- 实现:`crates/hft-exec/tests/broken.rs::exe_245_start_min_order_fail`

### EXE-246 HARD 生效时暂停,解除后重给完整窗口

- 规则:[Broken 处置](README.md#broken-处置)
- 级别:table
- 里程碑:M2
- 前置:C0(`broken_auto_fix_ms = 120 000`);g7 t 0 Broken
- 实现:`crates/hft-exec/tests/broken.rs::exe_246_hard_pause`

| 行 | 序列 | 期望 |
|---|---|---|
| EXE-246.1 | t 60 000 `CapSet{Kill, hard}`(Primed) | `phase = WaitHard`;t 120 000 推进无动作 |
| EXE-246.2 | 接 .1,t 200 000 `CapCleared{Kill, hard}` | `phase = Primed`、`due_tsc = t 320 000` |
| EXE-246.3 | t 130 000 Running 中、一片在途时 `CapSet{Kill, hard}` | `phase = WaitHard`;在途片终态后不发新片 |
| EXE-246.4 | 接 .3,t 140 000 解除 | `phase = Primed`、`due_tsc = t 260 000`;t 260 000 重新 `Start` 时 `exec_ms = 0` |
| EXE-246.5 | 全局 HARD(`Kill{Global, hard}`) | 同 .1 |

### EXE-247 manual_hold 置位时暂停,解除后重给完整窗口

- 规则:[Broken 处置](README.md#broken-处置)、[人工保持与恢复期抑制](README.md#人工保持与恢复期抑制)
- 级别:table
- 里程碑:M2
- 前置:C0;g7 t 0 Broken
- 实现:`crates/hft-exec/tests/broken.rs::exe_247_hold_pause`

| 行 | 序列 | 期望 |
|---|---|---|
| EXE-247.1 | t 60 000 `on_external(cause = Manual)`(Primed) | `phase = WaitHold` |
| EXE-247.2 | 接 .1,t 300 000 策略 `note_strategy_action` | `phase = Primed`、`due_tsc = t 420 000` |
| EXE-247.3 | Running 中 Manual 成交 | `phase = WaitHold`;在途片终态后不发新片 |

### EXE-248 恢复期:多出腿所在 venue 放行前不开始,放行时重给完整窗口

- 规则:[Broken 处置](README.md#broken-处置)、[人工保持与恢复期抑制](README.md#人工保持与恢复期抑制)
- 级别:table
- 里程碑:M2
- 前置:C0;崩溃前 g7 已 Broken 100 000 ms(恢复平移后 `due_tsc` 落在 `now + 20 000 ms`);`recovery_end` 于 t 0(`venue_open = 0`);多出腿 P 在 B
- 实现:`crates/hft-exec/tests/broken.rs::exe_248_recovery_full_window`

| 行 | 序列 | 期望 |
|---|---|---|
| EXE-248.1 | t 30 000 推进(已过平移后的到期) | 不开始 |
| EXE-248.2 | t 50 000 `on_venue_gate_open(a1, B)` | `due_tsc = t 170 000` |
| EXE-248.3 | t 169 999 推进 | 不开始 |
| EXE-248.4 | t 170 000 推进 | `AutoFix/Start` |

### EXE-249 多腿 cell 的各 venue 先后放行时以最后一次为准

- 规则:[Broken 处置](README.md#broken-处置)、[人工保持与恢复期抑制](README.md#人工保持与恢复期抑制)
- 级别:unit
- 里程碑:M2
- 前置:C0;`recovery_end` 于 t 0;g7 Broken、`Primed`,多出腿 P 在 B
- 步骤:
  1. t 1000 `on_venue_gate_open(a1, B)`
  2. t 50 000 `on_venue_gate_open(a1, A)`
- 期望:
  - 步骤 1 后 `due_tsc = t 121 000`;步骤 2 后 `due_tsc = t 170 000`;t 121 000 推进不开始
- 实现:`crates/hft-exec/tests/broken.rs::exe_249_last_venue_open_wins`

### EXE-250 可执行时长只在可执行状态累计,超过 fix_deadline_ms 才失败

- 规则:[Broken 处置](README.md#broken-处置)
- 级别:table
- 里程碑:M2
- 前置:C0(`fix_deadline_ms = 10 000`、`force_slice.cooldown_ms = 200`);g7 自动有界平仓 t 0 开始(`Running`);每片 IOC 零成交(`max_nofill = 1000`,避免触发零成交等待)
- 实现:`crates/hft-exec/tests/broken.rs::exe_250_exec_time`

| 行 | 序列 | 期望 |
|---|---|---|
| EXE-250.1 | 无等待;t 10 000 推进 | `phase = Running`(`exec_ms` 合计 10 000) |
| EXE-250.2 | 无等待;t 10 001 推进 | `phase = Failed`、`fail = 7`(Deadline) |
| EXE-250.3 | t 1000–t 31 000 P 报价断开(等待);t 39 999 推进 | `Running`(1000 + 8999 = 9999) |
| EXE-250.4 | 同 .3;t 40 000 推进 | `Running`(10 000) |
| EXE-250.5 | 同 .3;t 40 001 推进 | `Failed`、`fail = 7` |
| EXE-250.6 | 每片被 `RATE_BACKOFF` 门控拒绝(退避等待照常计时);t 10 001 推进 | `Failed`、`fail = 7` |

### EXE-251 恢复结束平移 run_mark_tsc,exec_ms 不平移

- 规则:[Broken 处置](README.md#broken-处置)、[快照与恢复](README.md#快照与恢复)
- 级别:unit
- 里程碑:M2
- 前置:C0;g7 `Running`,`exec_ms = 4000`、`run_mark_tsc = stop_tsc − 1000 ms`;停机 10 min;`recovery_end(now = N, stop_tsc, hz, hz)`;多出腿 venue 在 N 放行
- 步骤:
  1. `recovery_end`;`on_venue_gate_open(a1, B)`;N + 4999 ms 推进;N + 5001 ms 推进
- 期望:
  - `recovery_end` 后 `run_mark_tsc = N − 1000 ms`、`exec_ms = 4000`
  - N + 4999 ms:`Running`(4000 + 1000 + 4999 = 9999);N + 5001 ms:`Failed`、`fail = 7`(10 001)
- 实现:`crates/hft-exec/tests/broken.rs::exe_251_exec_time_across_restart`

### EXE-252 每一片前复核:到尘埃线即成功,方向变号即 SignFlip 失败且不向另一侧下单

- 规则:[Broken 处置](README.md#broken-处置)
- 级别:table
- 里程碑:M2
- 前置:C0;g7 `Running`,`start_sign = −1`(多出 P)、`budget = 0.5`、`done_qty = 0.2`;上一片已终态,静止
- 实现:`crates/hft-exec/tests/broken.rs::exe_252_per_slice_recheck`

| 行 | 当前净量 | 期望 |
|---|---|---|
| EXE-252.1 | S +1.00、P −1.3 | 出一片买 P 0.3 |
| EXE-252.2 | S +1.00、P −1.04(4 USDT) | `phase = Done`;`ExecAutoFixDone`;无订单 |
| EXE-252.3 | S +1.40、P −1.3(外来成交改了 S,裸量 +0.1) | `phase = Failed`、`fail = 8`(SignFlip);无订单(不卖 S) |

### EXE-253 每片数量取 budget 余量、裸量、名义上限与 |净量| 的最小者

- 规则:[Broken 处置](README.md#broken-处置)
- 级别:table
- 里程碑:M2
- 前置:C0;R0(`max_reduce_notional = max_hedge_notional = 2000`);`force_slice.max_chunk_notional = 1000`
- 实现:`crates/hft-exec/tests/broken.rs::exe_253_slice_qty`

| 行 | 状态 | 期望本片 |
|---|---|---|
| EXE-253.1 | 多出 P,`budget = 0.5`、`done_qty = 0.2`,裸量 0.3 | 买 P 0.3 |
| EXE-253.2 | 多出 P,`budget = 20.0`、`done_qty = 0`,裸量 20.0 | 买 P 9.9(1000 / 100.70 = 9.93) |
| EXE-253.3 | 多出 P,`budget = 0.5`、`done_qty = 0.4`,裸量 0.5 | 买 P 0.1(budget 余量) |
| EXE-253.4 | 多出 S,`budget = 0.55`,裸量 0.55,S 库存 0.55、可用 0.30 | 卖 S 0.30 |
| EXE-253.5 | 多出 S,可用余额 0.005(< lot),库存足够 | 无订单;暂停可执行时长(`run_mark_tsc = 0`);不失败 |
| EXE-253.6 | 多出 P,本行 P 的 `min_qty = 0.2`;`budget = 0.5`、`done_qty = 0.4`,裸量 0.1(10 USDT) | `Failed`、`fail = 2`(MinOrder) |

### EXE-254 自动有界平仓订单的字段

- 规则:[Broken 处置](README.md#broken-处置)、[对冲量上限](README.md#对冲量上限b6)
- 级别:unit
- 里程碑:M2
- 前置:C0;g7 `Running`,多出 P,裸量 0.5
- 步骤:
  1. 推进出一片
- 期望:
  - `ExecOrder{kind = AutoFix(10), origin = Engine, side = Buy, inst = P, qty = 0.5, hedge_cap_qty = 0.6, intent.tif = Ioc, intent.link.role = HedgeFix}`,`reduce_only` 置位
  - `ExecDecision{AutoFix, detail = 4 \| 2 << 8, origin = Engine(AutoFix), qty = 0.5}`
- 实现:`crates/hft-exec/tests/broken.rs::exe_254_autofix_order_fields`

### EXE-255 片间隔 ≥ force_slice.cooldown_ms,门控拒绝后 ≥ gated_backoff_ms

- 规则:[Broken 处置](README.md#broken-处置)
- 级别:table
- 里程碑:M2
- 前置:C0(`force_slice.cooldown_ms = 200`、`gated_backoff_ms = 500`);g7 `Running`;上一片在 t 0 发出
- 实现:`crates/hft-exec/tests/broken.rs::exe_255_slice_spacing`

| 行 | 上一片结果与推进时刻 | 期望 |
|---|---|---|
| EXE-255.1 | t 1 部分成交终态;t 199 | 无订单 |
| EXE-255.2 | 同上;t 200 | 下一片 |
| EXE-255.3 | t 0 被 `RATE_BACKOFF` 拒;t 499 | 无订单 |
| EXE-255.4 | 同上;t 500 | 下一片;`ExecDecision{AutoFix, detail = 4 \| 2 << 8, gate = RATE_BACKOFF}` 写在被拒那次 |

### EXE-256 开始后的门控拒绝与零成交只重试,不结束、不报 P0

- 规则:[Broken 处置](README.md#broken-处置)、[门控拒绝与退避](README.md#门控拒绝与退避)
- 级别:table
- 里程碑:M2
- 前置:C0(`max_nofill = 5`、`stuck_after_ms = 2000`);g7 `Running`
- 实现:`crates/hft-exec/tests/broken.rs::exe_256_retry_not_fail`

| 行 | 结果 | 期望 |
|---|---|---|
| EXE-256.1 | 风控 `RATE_BACKOFF` | `phase = Running`;无 `ExecAutoFixFailed`;`gated_bits = RATE_BACKOFF` |
| EXE-256.2 | OMS 本地 `NoConn` | 同 .1 |
| EXE-256.3 | 模式门拒绝 | 同 .1 |
| EXE-256.4 | 连续 4 次零成交 | 第 5 片照常按冷却发出 |
| EXE-256.5 | 连续 5 次零成交 | 下一片在最后一次零成交后 2000 ms 才发出;cell 状态保持 Broken(不进 Stuck) |
| EXE-256.6 | 4 次零成交后一次部分成交 | `nofill_streak = 0` |

### EXE-257 结束表:失败原因与码值

- 规则:[Broken 处置](README.md#broken-处置)
- 级别:table
- 里程碑:M2
- 前置:C0;g7 `Running`,多出 P,`budget = 0.5`
- 实现:`crates/hft-exec/tests/broken.rs::exe_257_end_table`

| 行 | 事件 | 期望 `phase` / `fail` / `detail` 的 arg |
|---|---|---|
| EXE-257.1 | 交易所拒单 | Failed / 4(Rejected) / 4 |
| EXE-257.2 | OMS 本地发送失败 `EncodeError` | Failed / 4 / 4 |
| EXE-257.3 | `on_place_result(Rejected(HEDGE_LIMIT))` | Failed / 9(RiskReject) / 9;`v[3] = 1 << 26` |
| EXE-257.4 | `Rejected(PX_BAND)` | Failed / 9 / 9;`v[3] = 1 << 9` |
| EXE-257.5 | 待发量低于最小下单量(非余额原因) | Failed / 2(MinOrder) / 2 |
| EXE-257.6 | `done_qty = 0.5 = budget`,裸量仍 0.1(10 USDT) | Failed / 6(Residual) / 6 |
| EXE-257.7 | 可执行时长 10 001 ms | Failed / 7(Deadline) / 7 |
| EXE-257.8 | 复核时裸量变号 | Failed / 8(SignFlip) / 8 |

### EXE-258 失败写 P0 Note 与 Failed 决定,保持 Broken,本 Broken 期不再自动尝试

- 规则:[Broken 处置](README.md#broken-处置)
- 级别:unit
- 里程碑:M2
- 前置:C0;g7 `Running`,多出 P,开始时裸量 0.5、`done_qty = 0.2`;净量 S +1.00、P −1.3
- 步骤:
  1. 下一片被交易所拒(`reject_bits` 0)
  2. 此后 24 h 内每 10 s 推进一次,期间报价、余额、链接全部满足开始条件
- 期望:
  - 步骤 1:`Note{ExecAutoFixFailed}` P0,`v = [0x01_0000_0701, 1, 4, 0, 0.2, 0.5, 0.3]`;`ExecDecision{AutoFix, detail = 4 \| 4 << 8 \| 4 << 16}`;`view.state = Broken`
  - 步骤 2:无 `AutoFix` 决定、无订单
- 实现:`crates/hft-exec/tests/broken.rs::exe_258_failure_final`

### EXE-259 已发出未终态的平仓片不算已减:终态前不判成功

- 规则:[Broken 处置](README.md#broken-处置)
- 级别:unit
- 里程碑:M2
- 前置:C0;g7 `Running`,最后一片买 P 0.5 已发出(P 净量 −1.5,`inflight_immediate = +0.5`)
- 步骤:
  1. 推进(片未终态)
  2. 该片零成交终态,推进
- 期望:
  - 步骤 1:无 `AutoFix/Done`、无订单
  - 步骤 2:无 `AutoFix/Done`;裸量仍 0.5;按冷却再发一片
- 实现:`crates/hft-exec/tests/broken.rs::exe_259_inflight_not_counted`

### EXE-260 引擎目标在 Running 期间被设下:本片终态后停止,回 Primed;清除后按新裸量重新开始

- 规则:[Broken 处置](README.md#broken-处置)、[保证金危急的默认减仓](README.md#保证金危急的默认减仓a5与-engine_flatten)
- 级别:unit
- 里程碑:M2
- 前置:C0;g7 `Running`,`exec_ms = 3000`、`budget = 0.5`、`done_qty = 0.2`;一片在途
- 步骤:
  1. `force_deleverage(a1, B, 0.2)`;`advance`
  2. 在途片终态;推进
  3. L3 回落(`MarginStress` 降到 L2),引擎目标清除,裸量此时 0.4;推进
- 期望:
  - 步骤 2 后 `phase = Primed`;无 `AutoFix` 片(引擎减仓推进)
  - 步骤 3:`AutoFix/Start`,`budget = 0.4`、`exec_ms = 0`、`done_qty = 0`
- 实现:`crates/hft-exec/tests/broken.rs::exe_260_engine_target_takeover`

### EXE-261 每个 Broken 期至多一次终态,成交总量不超过 budget

- 规则:[Broken 处置](README.md#broken-处置)
- 级别:prop
- 里程碑:M2
- 性质:任意事件序列中,每个 Broken 期(进入到 `CapCleared{Broken}`)`AutoFix/Done` 与 `AutoFix/Failed` 合计至多一条;同一期内 `AutoFix` 订单成交总量 ≤ 该次 `Start` 的 `budget`(引擎目标接手后重新开始的那次各自计);`Cancelled` 之后本期无 `AutoFix` 订单;无 `AutoFix` 订单落在 `venue_open` 无该位的 venue
- 生成:同 EXE-050,另含 Broken 进出、HARD 与 `manual_hold` 交错、报价断开 0–60 s、限速退避
- 参考:断言
- 例数:PR 2 000
- 实现:`crates/hft-exec/tests/props.rs::exe_261_one_attempt_per_broken`

### EXE-262 Done 只写记录,cell 离开 Broken 等 CapCleared

- 规则:[Broken 处置](README.md#broken-处置)、[状态机](README.md#状态机)
- 级别:unit
- 里程碑:M2
- 前置:C0;g7 `Running`;最后一片成交使裸量 0.03(3 USDT)
- 步骤:
  1. `on_order_update`(终态);`advance`
- 期望:
  - `phase = Done`;`view.state = Broken`(未收到 `CapCleared`);无订单;再推进不重复写 `ExecAutoFixDone`
- 实现:`crates/hft-exec/tests/broken.rs::exe_262_done_waits_capcleared`

### EXE-280 尘埃:不动作、不阻塞 Done、不触发补腿

- 规则:[残余与尘埃](README.md#残余与尘埃)
- 级别:unit
- 里程碑:M2
- 前置:C0(`dust_notional = 5 USDT`)、T0(`band = 0.01`、`residual = TrimExcess`);g7 目标 S +10.00 / P −10.0;净量 S +10.05、P −10.0(裸量 0.05,5 USDT);静止
- 步骤:
  1. `touch_inst(S)`;`advance`;此后 60 s 每 1 s 推进
- 期望:
  - 第一次推进写 `State → Idle` 与 `Done`;全程无订单、无 `HedgeFix`/`ResidualTrim` 决定;`fix.first_tsc = 0`
- 实现:`crates/hft-exec/tests/residual.rs::exe_280_dust_no_action`

### EXE-281 残余策略:Defer、TrimExcess、Hold

- 规则:[残余与尘埃](README.md#残余与尘埃)
- 级别:table
- 里程碑:M2
- 前置:C0(`dust_notional = 5 USDT`);g7 目标 S +10.00 / P −10.0;净量 S +10.06、P −10.0(裸量 0.06,6 USDT,裸量比 0.006 ≤ `tol` 0.02;0.06 < P 的 `min_qty` 0.1);最近链接的 `HedgePlan.residual` 如列;静止
- 实现:`crates/hft-exec/tests/residual.rs::exe_281_residual_policies`

| 行 | 残余策略与条件 | 期望 |
|---|---|---|
| EXE-281.1 | `Defer`,目标未到达(S 目标 +20.00,Working) | 无修剪单(留给下一笔链接) |
| EXE-281.2 | `Defer`,目标已到达(Idle) | 无订单(等同 Hold) |
| EXE-281.3 | `TrimExcess` | `ExecOrder{kind = ResidualTrim(6), side = Sell, inst = S, qty = 0.06}`;`ExecDecision{ResidualTrim}` |
| EXE-281.4 | `TrimExcess`,本行 S 的 `min_qty = 0.1` | 无订单(转 Hold) |
| EXE-281.5 | `Hold` | 无订单 |
| EXE-281.6 | `Hold`,净量 S +1.08、P −1.0(裸量比 0.074 > `tol`) | 补腿卖 S 0.08(`kind = HedgeFix`) |
| EXE-281.7 | `TrimExcess`,`hold = MANUAL_HOLD` | 无订单 |
| EXE-281.8 | `TrimExcess`,a1 `venue_open` 无 A | 无订单 |
| EXE-281.9 | `TrimExcess`,S 报价不可用 | 无订单;不计次 |
| EXE-281.10 | 无链接,目标 `PairParams.residual = TrimExcess` | 同 .3(取目标的策略) |

### EXE-282 目标为 0 时最后剩下的低于最小下单量的一腿

- 规则:[残余与尘埃](README.md#残余与尘埃)
- 级别:table
- 里程碑:M2
- 前置:C0(`dust_notional = 5 USDT`);本用例 P 的 `min_qty = 0.1`;g8 目标 P 0(`Exit`,`urgency = Urgent`);无在途
- 实现:`crates/hft-exec/tests/residual.rs::exe_282_zero_target_tail`

| 行 | P 净量 | 期望 |
|---|---|---|
| EXE-282.1 | −0.08(8 USDT) | `ExecDecision{State, state_to = Stuck, detail = 2 \| 8 << 8}`(`MinOrder`);`Note{ExecStuck}` P1;无订单 |
| EXE-282.2 | −0.05(5 USDT) | `State → Idle`;`Done` |
| EXE-282.3 | −0.04 | `State → Idle` |

### EXE-283 核销之后:不因核销产生订单,有目标时按新净量推进

- 规则:[残余与尘埃](README.md#残余与尘埃)
- 级别:table
- 里程碑:M2
- 前置:C0;各行在 `WriteOffResidual` 之后由 sequencer 调 `on_leg_adjust(a1, g7)`
- 实现:`crates/hft-exec/tests/residual.rs::exe_283_after_write_off`

| 行 | 核销前 → 核销后 | 期望 |
|---|---|---|
| EXE-283.1 | g7 Broken,`auto_fix.phase = Failed`(MinOrder),净量 S +1.06 → +1.00、P −1.0 | `advance` 无订单;随后 `on_risk(CapCleared{Broken})` + `advance` → `State → Idle`、`auto_fix` 全零 |
| EXE-283.2 | g7 目标 S +10.00 / P −10.0,净量 S +10.16 → +10.00 | `advance` 写 `State → Idle` 与 `Done`;无订单 |
| EXE-283.3 | g7 无目标、非 Broken,净量 S +1.06 → +1.00 | `advance` 无订单、无决定 |

### EXE-290 on_external 按条件表自上而下取第一条成立的处理

- 规则:[外来成交的默认处理](README.md#外来成交的默认处理b3)
- 级别:table
- 里程碑:M2
- 前置:C0(`external_deadline_ms = 5000`);`now = t 1000`;外来成交落在 g7 的 P 腿(`f.group = g7`、`leg_ix = 1`)
- 实现:`crates/hft-exec/tests/external.rs::exe_290_condition_table`

| 行 | cell 状态 / `f.cause` | 期望返回 | 期望输出与状态 |
|---|---|---|---|
| EXE-290.1 | Broken(`Primed`,`due_tsc = t 100 000`)/ Liquidation | 0 | `ExecDecision{ExternalSeen, detail = 6 \| 1 << 8 \| 4 << 16}`;仍 Broken;`due_tsc` 不变;`ext_deadline = 0` |
| EXE-290.2 | L3 引擎目标(`origin = Engine(MarginStress)`)/ Adl | 0 | `detail = 6 \| 2 << 8 \| 3 << 16`;引擎目标不变 |
| EXE-290.3 | 无目标,a1 `force_active` / Liquidation | 0 | `detail = 6 \| 1 << 8 \| 3 << 16` |
| EXE-290.4 | 策略目标,a1 `flatten_lock` / Liquidation | 0 | `arg = 3`(EngineOwned) |
| EXE-290.5 | 策略目标 / Manual | 0 | `detail = 6 \| 3 << 8 \| 2 << 16`;`Stuck{External}`;`hold = MANUAL_HOLD` |
| EXE-290.6 | 策略目标 / Liquidation | t 6000 | `detail = 6 \| 1 << 8 \| 1 << 16`;`ExecDecision{State, state_to = Stuck, detail = 2 \| 6 << 8}`;`Timer{Exec, External}` 到期 t 6000 |
| EXE-290.7 | 无目标 / Adl | t 6000 | `arg = 1`;状态保持 Idle(无 Stuck 决定) |
| EXE-290.8 | 策略目标 / Unknown | t 6000 | 同 .6,`code = 5` |
| EXE-290.9 | 策略目标 / Delivery | t 6000 | 同 .6,`code = 4` |

### EXE-291 Manual 成交在任一行都写 ExecManualExternal,级别随是否刚置位 manual_hold

- 规则:[外来成交的默认处理](README.md#外来成交的默认处理b3)、[人工保持与恢复期抑制](README.md#人工保持与恢复期抑制)
- 级别:table
- 里程碑:M2
- 前置:C0;外来成交 `cause = Manual`,P 腿,`side = Buy`、`qty = 0.5`、`px = 100.10`
- 实现:`crates/hft-exec/tests/external.rs::exe_291_manual_notes`

| 行 | cell 状态 | 期望 Note |
|---|---|---|
| EXE-291.1 | 策略目标,无保持 | `ExecManualHold` P1(置位);`ExecManualExternal` 级别 Info,`v = [1, 1, 21, Buy, 0.5, 100.10]` |
| EXE-291.2 | 已 `MANUAL_HOLD` | 无 `ExecManualHold`;`ExecManualExternal` P1 |
| EXE-291.3 | Broken,无保持 | `ExecManualHold` P1;`ExecManualExternal` Info;`auto_fix.phase = WaitHold` |
| EXE-291.4 | L3 引擎目标 | 无 `ExecManualHold`;`hold = MANUAL_HOLD_DEFERRED`;`ExecManualExternal` P1 |

### EXE-292 GROUP_NONE 的外来成交返回 0,清仓锁下登记无组重扫

- 规则:[外来成交的默认处理](README.md#外来成交的默认处理b3)、[保证金危急的默认减仓](README.md#保证金危急的默认减仓a5与-engine_flatten)
- 级别:table
- 里程碑:M2
- 前置:C0;外来成交 `f.group = GROUP_NONE`,P,`cause = Unknown`,入账后 a1 账户 P 持仓出现不属于任何 cell 的 −0.8
- 实现:`crates/hft-exec/tests/external.rs::exe_292_group_none`

| 行 | 账户状态 | 期望 |
|---|---|---|
| EXE-292.1 | 无锁、非 L3 | 返回 0;`ExecOut` 全空;`has_pending() = false` |
| EXE-292.2 | `flatten_lock` | 返回 0;`ExecOut` 全空;同一 Input 的 `advance` 写 `ExecDecision{ForceStep, group = GROUP_NONE, detail = 5 \| 4 << 8}`、`Note{ExecUngroupedReduce}` P0,无组 cell 买 P 0.8(reduce-only) |
| EXE-292.3 | L3(`force_active`),无锁 | 返回 0;本 Input 无无组单;下一次 `force_deleverage` 时纳入 |

### EXE-293 策略处理外来成交:清截止与暂停;逐字段相同的目标不算处理

- 规则:[外来成交的默认处理](README.md#外来成交的默认处理b3)
- 级别:table
- 里程碑:M2
- 前置:C0;g7 策略目标被 Liquidation 暂停为 `Stuck{External}`,`ext_deadline = t 6000`;`now = t 3000`
- 实现:`crates/hft-exec/tests/external.rs::exe_293_handled`

| 行 | 策略动作 | 期望 |
|---|---|---|
| EXE-293.1 | `set_target`(不同的目标) | `ext_deadline = 0`;撤 `Timer{Exec, External}`;`ExecDecision{TargetReplaced}` 与 `ExecDecision{ExternalSeen, detail = 6 \| 0 << 8 \| 5 << 16}`(Cleared);state `Working` |
| EXE-293.2 | `clear_target` | `ext_deadline = 0`;`TargetCleared` 与 Cleared 决定;state `Idle` |
| EXE-293.3 | `note_strategy_action(a1, g7)` | `ext_deadline = 0`;Cleared 决定;`ExecDecision{State, state_from = Stuck, state_to = Working}` |
| EXE-293.4 | `set_target`(与当前目标逐字段相同) | `ext_deadline = t 6000` 不变;无决定;t 6000 截止照常执行(EXE-294) |

### EXE-294 截止到达未处理:清暂停的策略目标并对多出的一腿发 ExternalDefault

- 规则:[外来成交的默认处理](README.md#外来成交的默认处理b3)
- 级别:table
- 里程碑:M2
- 前置:C0(`external_deadline_ms = 5000`);g7 策略目标被 t 1000 的 Liquidation(P 空头被平 0.5)暂停,`ext_deadline = t 6000`;净量 S +1.00、P −0.5(多出 S,裸量 0.5);静止
- 实现:`crates/hft-exec/tests/external.rs::exe_294_deadline_default`

| 行 | 推进时刻 | 期望 |
|---|---|---|
| EXE-294.1 | t 5999 | 无动作 |
| EXE-294.2 | t 6000(`Timer{Exec, External}`) | `ExecDecision{TargetCleared, eflags = CLEARED_BY_ENGINE}`、通知 `Done{cleared_by_engine: true}`;`ExecOrder{kind = ExternalDefault(7), side = Sell, inst = S, qty = 0.50, hedge_cap_qty = 0.51}`;`ExecDecision{ExternalDefault}`;`Note{ExecExternalDefault}` P0;`ext_deadline = 0` |

### EXE-295 截止到达而裸量在阈值内:不发单,仍写决定与 P0

已删除:不发单时 ExecExternalDefault 改为 P1,由 EXE-512 替代。

### EXE-296 ExternalDefault 与补腿同一抑制、计次与截止

- 规则:[外来成交的默认处理](README.md#外来成交的默认处理b3)、[补腿](README.md#补腿hedgefixc4)
- 级别:table
- 里程碑:M2
- 前置:C0;同 EXE-294,截止 t 6000 到达时所列条件成立
- 实现:`crates/hft-exec/tests/external.rs::exe_296_default_suppression`

| 行 | 条件 | 期望 |
|---|---|---|
| EXE-296.1 | `has_unknown = true` 至 t 8000 | t 6000 无订单;t 8000 收敛后发 `ExternalDefault` 单 |
| EXE-296.2 | S 报价不可用至 t 20 000 | t 6000–t 16 000 无订单;t 16 001 `Broken{FixExhausted}`(截止自 t 6000 起照算) |
| EXE-296.3 | `ExternalDefault` 单连续 3 次被交易所拒 | 第 3 次后 `Broken{FixExhausted}` |

### EXE-297 恢复期内截止到达不执行;venue 放行时截止重设

- 规则:[外来成交的默认处理](README.md#外来成交的默认处理b3)、[人工保持与恢复期抑制](README.md#人工保持与恢复期抑制)
- 级别:unit
- 里程碑:M2
- 前置:C0;`recovery_end` 于 t 0(`venue_open = 0`);g7 `ext_deadline` 平移后为 t 2000;多出腿 S 在 A
- 步骤:
  1. t 3000 推进
  2. t 10 000 `on_venue_gate_open(a1, A)`;`advance`
  3. t 14 999 推进;t 15 000 推进
- 期望:
  - 步骤 1:无 `ExternalDefault`
  - 步骤 2:`ext_deadline = t 15 000`;`Timer{Exec, External}` 重登到期 t 15 000
  - 步骤 3:t 14 999 无动作;t 15 000 执行默认处理
- 实现:`crates/hft-exec/tests/external.rs::exe_297_recovery_deadline_reset`

### EXE-298 恢复期内 on_external 仍返回名义截止

- 规则:[外来成交的默认处理](README.md#外来成交的默认处理b3)
- 级别:unit
- 里程碑:M2
- 前置:C0(`external_deadline_ms = 5000`);a1 `venue_open = 0`;`now = t 1000`
- 步骤:
  1. `on_external`(g7,Liquidation,策略目标)
- 期望:
  - 返回 t 6000;`ext_deadline = t 6000`
- 实现:`crates/hft-exec/tests/external.rs::exe_298_nominal_deadline`

### EXE-299 Manual 成交不计时,截止永不到来

- 规则:[外来成交的默认处理](README.md#外来成交的默认处理b3)
- 级别:unit
- 里程碑:M2
- 前置:C0;g7 策略目标;t 0 `on_external(cause = Manual)`,多出 S 0.5
- 步骤:
  1. 此后 1 h 内每 1 s 推进
- 期望:
  - 无 `ExternalDefault`、无 `HedgeFix`/`ResidualTrim` 决定与订单;`ext_deadline = 0`;`view.state = Stuck`(`External`)
- 实现:`crates/hft-exec/tests/external.rs::exe_299_manual_no_timer`

### EXE-300 L3 期间 ADL:引擎目标不暂停、按成交后仓位继续,无 ExternalDefault

- 规则:[外来成交的默认处理](README.md#外来成交的默认处理b3)、[保证金危急的默认减仓](README.md#保证金危急的默认减仓a5与-engine_flatten)
- 级别:unit
- 里程碑:M2
- 前置:C0;a1 L3,g7 引擎目标 S +8.00 / P −8.0(当前 S +10.00、P −10.0)
- 步骤:
  1. `on_external`(P,Adl,P 空头被减 1.5 → P −8.5);`advance`
- 期望:
  - 返回 0;引擎目标仍 S +8.00 / P −8.0;本次推进的 P 单为买 0.5(按 −8.5),无 `ExternalDefault`;后续引擎目标到达后剩余的多出一腿按补腿处理
- 实现:`crates/hft-exec/tests/external.rs::exe_300_adl_during_l3`

### EXE-310 manual_hold 置位:清补腿与外来计时、暂停策略目标、自动平仓转 WaitHold,写 P1

- 规则:[人工保持与恢复期抑制](README.md#人工保持与恢复期抑制)
- 级别:unit
- 里程碑:M2
- 前置:C0;g7 策略目标 Working,净量 S +1.00、P −1.0;`fix.count = 1`、`first_tsc = t 0`(此前的补腿条件);`ext_deadline = t 6000`(此前一笔 Liquidation 已入账并计时)
- 步骤:
  1. t 2000 `on_external`(P,`cause = Manual`,`side = Buy`、`qty = 0.5`;入账后 P −0.5,裸量 0.5 多出 S)
- 期望:
  - `hold = MANUAL_HOLD`;`FixState` 全零;`ext_deadline = 0`;`ExecOut.timers` 撤销 `Timer{Exec, Fix}` 与 `Timer{Exec, External}` 各一条
  - `Note{ExecManualHold}` P1,`v = [0x01_0000_0701, 1, 1, 21, Buy, 0.5, 0.5]`
  - `ExecDecision{ExternalSeen, detail = 6 \| 3 << 8 \| 2 << 16}` 与 `ExecDecision{ExternalSeen, detail = 6 \| 0 << 8 \| 5 << 16}`(先前计时被清)
- 实现:`crates/hft-exec/tests/hold.rs::exe_310_manual_hold_set`

### EXE-311 引擎目标期间的 Manual 成交只置延迟位,引擎目标清除时转为置位

- 规则:[人工保持与恢复期抑制](README.md#人工保持与恢复期抑制)
- 级别:table
- 里程碑:M2
- 前置:C0;g7 处于所列引擎目标期间,收到 `ExternalFill{Manual}`
- 实现:`crates/hft-exec/tests/hold.rs::exe_311_deferred_hold`

| 行 | 引擎目标与清除方式 | 期望 |
|---|---|---|
| EXE-311.1 | L3 引擎目标;随后 `MarginStress` 降到 L2(引擎目标清除) | 收到时 `hold = MANUAL_HOLD_DEFERRED`、无 `ExecManualHold`;清除时 `hold = MANUAL_HOLD`、`Note{ExecManualHold}` P1 |
| EXE-311.2 | `HardAlign` 对齐目标;对齐目标到达后清除 | 同 .1 |
| EXE-311.3 | 清仓目标 0;清仓目标到达后清除 | 同 .1 |
| EXE-311.4 | L3 引擎目标期间策略 `note_strategy_action` | `MANUAL_HOLD_DEFERRED` 被清;引擎目标清除时不置位 |

### EXE-312 manual_hold 只由所列动作解除;每种解除写 Info 级 Note 并重给自动平仓窗口

- 规则:[人工保持与恢复期抑制](README.md#人工保持与恢复期抑制)
- 级别:table
- 里程碑:M2
- 前置:C0(`broken_auto_fix_ms = 120 000`);g7 `hold = MANUAL_HOLD`,策略目标 tag 0x10;`now = t 50 000`
- 实现:`crates/hft-exec/tests/hold.rs::exe_312_release_actions`

| 行 | 动作 | 期望 |
|---|---|---|
| EXE-312.1 | `set_target`(tag 0x11,替换) | `hold = 0`;`Note{ExecManualHold}` Info,`v = [0x01_0000_0701, 0, 1]`;`fix.first_tsc = 0` |
| EXE-312.2 | `note_strategy_action` | 同 .1(原因 1) |
| EXE-312.3 | `hedge_fix_cmd` 成功 | 原因 2 |
| EXE-312.4 | L3 一步设下引擎目标 | 原因 3 |
| EXE-312.5 | `clear_account(a1)` | 原因 4 |
| EXE-312.6 | `clear_target` | 不解除;无 Note |
| EXE-312.7 | `set_target`(逐字段相同) | 不解除;无 Note |
| EXE-312.8 | `set_target` 被拒(`SpotShort`) | 不解除 |
| EXE-312.9 | g7 Broken、`phase = WaitHold`,`note_strategy_action` | 解除;`phase = Primed`、`due_tsc = t 170 000` |

### EXE-313 manual_hold 期间:自动处置不发、不计时;引擎减仓与命令不受影响

- 规则:[人工保持与恢复期抑制](README.md#人工保持与恢复期抑制)
- 级别:table
- 里程碑:M2
- 前置:C0;g7 `hold = MANUAL_HOLD`,净量 S +1.00、P −1.5,静止
- 实现:`crates/hft-exec/tests/hold.rs::exe_313_hold_suppression`

| 行 | 场景 | 期望 |
|---|---|---|
| EXE-313.1 | 推进 1 h | 无 `HedgeFix`;`first_tsc = 0`;无 `FixExhausted` |
| EXE-313.2 | `residual = TrimExcess` 的残余 | 无 `ResidualTrim` |
| EXE-313.3 | `force_deleverage(a1, B, 0.2)` | 出 `ForceStep` 单;`hold` 解除(原因 3) |
| EXE-313.4 | `engine_flatten(a1)` | 出 `ForceStep` 单 |
| EXE-313.5 | `hedge_fix_cmd` | 出 `CmdHedgeFix` 单 |

### EXE-314 两个保持位进快照,恢复后保持

- 规则:[人工保持与恢复期抑制](README.md#人工保持与恢复期抑制)、[快照与恢复](README.md#快照与恢复)
- 级别:table
- 里程碑:M2
- 前置:C0;g7 与 g10 分别 `MANUAL_HOLD` 与 `MANUAL_HOLD_DEFERRED`
- 实现:`crates/hft-exec/tests/hold.rs::exe_314_hold_snapshot`

| 行 | cell | 期望 Cell 行 `flags` 与恢复后 |
|---|---|---|
| EXE-314.1 | g7 | `flags & 4 = 4`;`restore` 后补腿仍被抑制 |
| EXE-314.2 | g10 | `flags & 16 = 16`;`restore` 后引擎目标清除时转为置位 |

### EXE-315 恢复期按多出腿所在 venue 抑制

- 规则:[人工保持与恢复期抑制](README.md#人工保持与恢复期抑制)
- 级别:table
- 里程碑:M2
- 前置:C0(`fix_deadline_ms = 10 000`);`recovery_end` 于 t 0;t 100 `on_venue_gate_open(a1, A)`(B 未放行);各行的多出腿如列
- 实现:`crates/hft-exec/tests/recovery_gate.rs::exe_315_per_venue_suppression`

| 行 | 场景 | 期望 |
|---|---|---|
| EXE-315.1 | g7 多出 S(A) | t 100 之后补腿卖 S |
| EXE-315.2 | g7 多出 P(B) | 无补腿;`first_tsc = t 100`;t 10 101 无 `FixExhausted` |
| EXE-315.3 | g7 多出 P(B),`TrimExcess` 残余 | 无 `ResidualTrim` |
| EXE-315.4 | g7 多出 P(B),外来成交截止到达 | 不执行 |
| EXE-315.5 | g7 Broken,多出 P(B),已到期 | 不开始自动平仓 |
| EXE-315.6 | g8(只有 P)L3 引擎目标 | 照常出 `ForceStep` 单 |
| EXE-315.7 | g7 多出 S(A),Broken 已到期 | 开始自动平仓 |

### EXE-316 on_venue_gate_open 重设该 venue 上各 cell 的截止,不直接写输出

- 规则:[人工保持与恢复期抑制](README.md#人工保持与恢复期抑制)、[输出 `ExecOut`](README.md#输出-execout容量固定sequencer-预分配每次调用本-crate-之后排空)
- 级别:unit
- 里程碑:M2
- 前置:C0;`recovery_end` 于 t 0;g7 `fix.count = 1`、`first_tsc = t 0`、`ext_deadline = t 3000`、Broken `Primed`(`due_tsc` 平移后 t 20 000);g15 = `GroupId(0x0100_000F)` 只有 S 腿(A),`ext_deadline = t 3000`
- 步骤:
  1. t 40 000 `on_venue_gate_open(a1, B)`
  2. `advance`
- 期望:
  - 步骤 1 后 `ExecOut` 全空;g7 `fix.count = 0`、`first_tsc = 0`、`ext_deadline = t 45 000`、`due_tsc = t 160 000`;g15 不变;Gate 行 `venue_open` 含 B 的位
  - 步骤 2:g7 的 `Timer{Exec, External}` 与 `Timer{Exec, AutoFix}` 按新时刻重登
- 实现:`crates/hft-exec/tests/recovery_gate.rs::exe_316_venue_open_resets`

### EXE-317 on_venue_gate_open 已置位时无动作

- 规则:[人工保持与恢复期抑制](README.md#人工保持与恢复期抑制)、[入口](README.md#入口全部在-sequencer-线程调用无分配)
- 级别:unit
- 里程碑:M2
- 前置:C0;t 1000 已 `on_venue_gate_open(a1, B)`;g7 此后 `ext_deadline = t 7000`
- 步骤:
  1. t 5000 再次 `on_venue_gate_open(a1, B)`
- 期望:
  - `ext_deadline = t 7000` 不变;`has_pending()` 不因此变为真;快照逐字节不变
- 实现:`crates/hft-exec/tests/recovery_gate.rs::exe_317_venue_open_idempotent`

### EXE-318 on_gate_open 置 gate_open 并补做尚未放行的 venue

- 规则:[人工保持与恢复期抑制](README.md#人工保持与恢复期抑制)、[入口](README.md#入口全部在-sequencer-线程调用无分配)
- 级别:unit
- 里程碑:M2
- 前置:C0;`recovery_end` 于 t 0;t 1000 `on_venue_gate_open(a1, B)`;g7(腿在 A、B)与 g8(只在 B)各有 `ext_deadline`
- 步骤:
  1. t 9000 `on_gate_open(a1)`;`advance`
- 期望:
  - Gate 行 `gate_open = 1`、`venue_open` 含 A 与 B;g7 `ext_deadline = t 14 000`(A 在此放行);g8 的 `ext_deadline` 不变(B 已在 t 1000 放行)
- 实现:`crates/hft-exec/tests/recovery_gate.rs::exe_318_gate_open_fills_venues`

### EXE-319 recovery_end 关闭全部账户闸门

- 规则:[人工保持与恢复期抑制](README.md#人工保持与恢复期抑制)、[快照与恢复](README.md#快照与恢复)
- 级别:unit
- 里程碑:M2
- 前置:C0;快照中 a1 `gate_open = 1`、`venue_open = 0b110`、`gate_closed_tsc = 5`
- 步骤:
  1. `restore`;`recovery_end(now = N, …)`;`snapshot`
- 期望:
  - a1、a2、s1 的 Gate 行均为 `gate_open = 0`、`venue_open = 0`、`gate_closed_tsc = N`
- 实现:`crates/hft-exec/tests/recovery_gate.rs::exe_319_recovery_end_closes_gate`

### EXE-325 闸门滞留告警场合 1:账户闸门未放行时的 Broken 或超阈值裸量

- 规则:[人工保持与恢复期抑制](README.md#人工保持与恢复期抑制)
- 级别:table
- 里程碑:M2
- 前置:C0(`gate_naked_alarm_ms = 300 000`);`recovery_end` 于 t 0(`gate_closed_tsc = t 0`);a1 未 `on_gate_open`、无 venue 放行;mark 100.00
- 实现:`crates/hft-exec/tests/recovery_gate.rs::exe_325_alarm_case1`

| 行 | cell 状态 | 推进时刻 | 期望 |
|---|---|---|---|
| EXE-325.1 | g7 崩溃前已 Broken(平移后 `broken_since` 早于 `gate_closed_tsc`),多出 P 0.5 | t 299 999 | 无 `ExecGateHeldNaked` |
| EXE-325.2 | 同 .1 | t 300 000(`Timer{Exec, Alarm}`) | `Note{ExecGateHeldNaked}` P0,`v = [0x01_0000_0701, 1, 1, 300 000, 1, 0.5, 50]`(等待原因 Venue) |
| EXE-325.3 | g7 非 Broken,`first_tsc = t 20 000`(补腿被恢复期抑制),多出 P 0.5 | t 320 000 | 一条,`v[1] = 1`、`v[2] = 0`、`v[3] = 300 000` |
| EXE-325.4 | 同 .1,t 200 000 `on_gate_open(a1)` | t 300 000 | 无 |
| EXE-325.5 | 同 .1,`hold = MANUAL_HOLD` | t 300 000 | 无 |

### EXE-326 闸门滞留告警场合 2:自动平仓到期后因等待条件迟迟不能开始

- 规则:[人工保持与恢复期抑制](README.md#人工保持与恢复期抑制)、[Broken 处置](README.md#broken-处置)
- 级别:table
- 里程碑:M2
- 前置:C0;a1 闸门已放行;g7 t 0 Broken(`due_tsc = t 120 000`),多出 P 0.5;所列等待条件自 t 100 000 起持续
- 实现:`crates/hft-exec/tests/recovery_gate.rs::exe_326_alarm_case2`

| 行 | 等待条件 | 推进时刻 | 期望 |
|---|---|---|---|
| EXE-326.1 | P 报价不可用 | t 419 999 | 无 |
| EXE-326.2 | P 报价不可用 | t 420 000 | 一条,`v[1] = 2`、`v[2] = 4`(Quote)、`v[3] = 300 000` |
| EXE-326.3 | `has_unknown` | t 420 000 | `v[2] = 2`(NotQuiescent) |
| EXE-326.4 | `cap_block = REORG` | t 420 000 | `v[2] = 3` |
| EXE-326.5 | `rate_backoff` | t 420 000 | `v[2] = 5` |
| EXE-326.6 | 对手价越过上限 | t 420 000 | `v[2] = 6` |
| EXE-326.7 | 多出 S,可用余额不足 | t 420 000 | `v[2] = 7` |
| EXE-326.8 | 引擎目标 | t 420 000 | 无(引擎目标除外) |
| EXE-326.9 | P 报价不可用,t 400 000 恢复 | t 420 000 | 无(复核不成立) |

### EXE-327 两种场合同时成立只写场合 1;同一起点只写一条

- 规则:[人工保持与恢复期抑制](README.md#人工保持与恢复期抑制)
- 级别:unit
- 里程碑:M2
- 前置:C0;`recovery_end` 于 t 0,a1 闸门未放行;多出 P 的 venue B 在 t 1000 放行,`due_tsc = t 121 000`;P 报价自 t 0 不可用(场合 1 告警时刻 t 300 000,场合 2 告警时刻 t 421 000)
- 步骤:
  1. 推进到 t 1 000 000,每 1000 ms 一次
- 期望:
  - 恰好一条 `ExecGateHeldNaked`,在 t 300 000,`v[1] = 1`;t 421 000 无第二条
- 实现:`crates/hft-exec/tests/recovery_gate.rs::exe_327_alarm_case1_wins`

### EXE-328 告警定时器只在告警时刻之前登记,起点变化时改期

- 规则:[人工保持与恢复期抑制](README.md#人工保持与恢复期抑制)、[定时器](README.md#定时器)
- 级别:table
- 里程碑:M2
- 前置:C0;a1 闸门已放行;g7 Broken,`due_tsc = t 120 000`,P 报价不可用
- 实现:`crates/hft-exec/tests/recovery_gate.rs::exe_328_alarm_timer`

| 行 | 动作 | 期望 `Timer{Exec, Alarm}`(`timer_id = 6 \| 1 << 8`) |
|---|---|---|
| EXE-328.1 | t 120 000 推进 | 登记到期 t 420 000 |
| EXE-328.2 | t 200 000 `manual_hold` 置位后 t 250 000 解除(`due_tsc = t 370 000`) | 先撤后登,到期 t 670 000 |
| EXE-328.3 | t 430 000(已过告警时刻)再推进 | 不再登记 |
| EXE-328.4 | `on_timer` 早到(t 300 000,保存的告警时刻 t 420 000) | 只重新登记,无 Note |

### EXE-329 恢复后按恢复后的状态重新登记告警,新的闸门关闭期从 gate_closed_tsc 起算

- 规则:[人工保持与恢复期抑制](README.md#人工保持与恢复期抑制)、[快照与恢复](README.md#快照与恢复)
- 级别:unit
- 里程碑:M2
- 前置:C0;崩溃前 g7 Broken 且场合 1 告警已写过;`recovery_end(now = N)`
- 步骤:
  1. 收集 `recovery_timers()`;推进到 N + 300 000 ms
- 期望:
  - `recovery_timers()` 含 g7 的 `Timer{Exec, Alarm}` 到期 N + 300 000 ms;到期写一条新的场合 1 告警
- 实现:`crates/hft-exec/tests/recovery_gate.rs::exe_329_alarm_after_restore`

### EXE-340 force_deleverage 只登记请求;advance 设定按比例、向零取 lot 的引擎目标

- 规则:[保证金危急的默认减仓](README.md#保证金危急的默认减仓a5与-engine_flatten)
- 级别:table
- 里程碑:M2
- 前置:C0;a1 L3;g8(只有 P)无策略目标;每行 P 净量与 `frac` 如列
- 实现:`crates/hft-exec/tests/force.rs::exe_340_proportional_target`

| 行 | P 净量 / `frac` | 期望 |
|---|---|---|
| EXE-340.1 | −10.0 / 0.2 | 调用后 `ExecOut` 全空;Acct 行 `force_active = 1`、`force_kind = 1`、`force_venues` 含 B、`force_frac = 0.2`;`advance` 后引擎目标 −8.0,买 P 2.0 |
| EXE-340.2 | −7.3 / 0.25 | 目标 −5.4(−5.475 向零取 lot) |
| EXE-340.3 | +3.3 / 0.5 | 目标 +1.6 |
| EXE-340.4 | −10.0 / 1.0 | 目标 0 |
| EXE-340.5 | −10.0 / 0.2,`advance` 之后 | Acct 行 `force_kind = 0`(`force_req` 已清),`force_active = 1` |

### EXE-341 步与步按比例复合,成交慢时不重复减同一段

- 规则:[保证金危急的默认减仓](README.md#保证金危急的默认减仓a5与-engine_flatten)
- 级别:table
- 里程碑:M2
- 前置:C0;a1 L3;g8 P −10.0;第一步 `frac = 0.2` 设引擎目标 −8.0
- 实现:`crates/hft-exec/tests/force.rs::exe_341_compounding`

| 行 | 第二步之前的 P 净量 | 期望第二步(`frac = 0.2`)目标 |
|---|---|---|
| EXE-341.1 | −9.5(第一步未完成) | −6.4(以 −8.0 为基准) |
| EXE-341.2 | −8.0(已到达) | −6.4 |

### EXE-342 每腿取比例目标与当前目标中更接近 0 者,反号取 0

- 规则:[保证金危急的默认减仓](README.md#保证金危急的默认减仓a5与-engine_flatten)
- 级别:table
- 里程碑:M2
- 前置:C0;a1 L3;g8 P −10.0;`frac = 0.2`(比例目标 −8.0)
- 实现:`crates/hft-exec/tests/force.rs::exe_342_closer_to_zero`

| 行 | g8 当前目标 | 期望引擎目标 |
|---|---|---|
| EXE-342.1 | 策略 `Exit` 0 | 0 |
| EXE-342.2 | 策略 −9.0 | −8.0 |
| EXE-342.3 | 策略 −5.0 | −5.0 |
| EXE-342.4 | 策略 +2.0(与 −8.0 反号) | 0 |
| EXE-342.5 | 无目标 | −8.0 |

### EXE-343 L3 只作用于在该 venue 有永续腿仓位的 cell,同 group 其他交易所腿同比例

- 规则:[保证金危急的默认减仓](README.md#保证金危急的默认减仓a5与-engine_flatten)
- 级别:unit
- 里程碑:M2
- 前置:C0;a1 L3;g7 S +10.00、P −10.0;g11(P −2.0 在 B、Q +2.0 在 C);g17 = `GroupId(0x0100_0011)` 只有 Q(C)+1.0;g15 只有 S
- 步骤:
  1. `force_deleverage(a1, B, 0.2)`;`advance`
- 期望:
  - g7 引擎目标 S +8.00、P −8.0;g11 引擎目标 P −1.6、Q +1.6;g17、g15 无引擎目标、无决定
  - 每条 `ExecDecision{ForceStep}` 的 `reason = MarginStress`、`urgency = Urgent`、`origin = Engine(MarginStress)`;目标 `band = 0`
- 实现:`crates/hft-exec/tests/force.rs::exe_343_venue_scope`

### EXE-344 引擎目标替换策略目标:写 ForceStep、通知 EngineOverride、解除 manual_hold

- 规则:[保证金危急的默认减仓](README.md#保证金危急的默认减仓a5与-engine_flatten)、[人工保持与恢复期抑制](README.md#人工保持与恢复期抑制)
- 级别:unit
- 里程碑:M2
- 前置:C0;a1 L3;g8 策略目标 P −10.0(Working),`hold = MANUAL_HOLD`
- 步骤:
  1. `force_deleverage(a1, B, 0.2)`;`advance`
- 期望:
  - `ExecDecision{ForceStep, detail = 5 \| 1 << 8, target_qty = −8.0}`;`ExecDecision{State, state_to = Stuck, detail = 2 \| 7 << 8}`;通知 `Stuck(EngineOverride)`
  - `hold = 0`;`Note{ExecManualHold}` Info,`v = [0x01_0000_0801, 0, 3]`
- 实现:`crates/hft-exec/tests/force.rs::exe_344_override_records`

### EXE-345 设 MarginStress/Hard 引擎目标时每条交易所腿请求撤策略挂单,每步重发

- 规则:[保证金危急的默认减仓](README.md#保证金危急的默认减仓a5与-engine_flatten)
- 级别:table
- 里程碑:M2
- 前置:C0;a1 L3;g7 S +10.00、P −10.0
- 实现:`crates/hft-exec/tests/force.rs::exe_345_cancel_resting`

| 行 | 动作 | 期望 |
|---|---|---|
| EXE-345.1 | 第一步 `force_deleverage(a1, B, 0.2)` + `advance` | `cancel_resting` = {a1, S}、{a1, P};`ExecDecision{ForceStep, detail = 5 \| 5 << 8 \| 0b11 << 16}` |
| EXE-345.2 | 第二步(`force_step_interval_ms` 后) | 再写同样两条 `cancel_resting` 与一条 `CancelResting` 决定 |
| EXE-345.3 | `engine_flatten(a1)` + `advance` | 同 .1 |
| EXE-345.4 | 无组 cell 装入 P(无组 −3.0) | `cancel_resting` 含 {a1, P}(与 g7 的同一条去重) |
| EXE-345.5 | 普通补腿单(非 MarginStress/Hard) | 无 `cancel_resting` |

### EXE-346 撤单确认前每片数量 ≤ |仓位| − 同向即时在途

- 规则:[保证金危急的默认减仓](README.md#保证金危急的默认减仓a5与-engine_flatten)
- 级别:table
- 里程碑:M2
- 前置:C0;R0 但 `max_reduce_notional = 5000`;`force_slice.max_chunk_notional = 5000`、`depth_frac = 1.0`;g8 P −10.0;`engine_flatten(a1)`(目标 0)
- 实现:`crates/hft-exec/tests/force.rs::exe_346_slice_minus_inflight`

| 行 | P 在途 | 期望第一片 |
|---|---|---|
| EXE-346.1 | 无 | 买 P 10.0 |
| EXE-346.2 | 策略 reduce-only GTC 买 10.0 已 Ack(`inflight_resting = +10.0`) | 买 P 10.0(挂单在途不扣) |
| EXE-346.3 | 策略 IOC 买 9.0 在途(`inflight_immediate = +9.0`) | 买 P 1.0 |
| EXE-346.4 | `inflight_immediate = +10.0` | 无订单 |

### EXE-347 SerialFallback 的四种触发,各写一条 ForceStep 决定与 P0 Note

- 规则:[保证金危急的默认减仓](README.md#保证金危急的默认减仓a5与-engine_flatten)
- 级别:table
- 里程碑:M2
- 前置:C0;a1 L3;g7 非 Broken,S +10.00、P −10.0;`force_deleverage(a1, B, 0.2)` 后 `advance`;P 报价健康
- 实现:`crates/hft-exec/tests/force.rs::exe_347_serial_fallback_reasons`

| 行 | 条件 | 期望 |
|---|---|---|
| EXE-347.1 | S 报价不可用 | `serial = 1`;`ExecDecision{ForceStep, detail = 5 \| 2 << 8 \| 1 << 16}`;`Note{ExecSerialFallback}` P0,`v = [1, 0, 1, −10.0, 10.00, 3]`;买 P 2.0 无 LegLink、reduce-only |
| EXE-347.2 | `venue_ready(a1, A) = false` | 同上,原因 4 |
| EXE-347.3 | 在途链接处于 `LINK_HOLD` | 同上,原因 5;`active` 释放该链接 |
| EXE-347.4 | 在途链接的第一腿槽 `Unknown` | 同上,原因 6 |
| EXE-347.5 | 两腿都可用 | 无 SerialFallback;第一腿买 P 带 LegLink(`HedgePlan.residual = TrimExcess`),`first_leg_ix = 1` |
| EXE-347.6 | S 报价不可用且 `venue_ready(a1, A) = false`,同时在途链接 `LINK_HOLD` | 一条 `Note{ExecSerialFallback}`,`v[2] = 1`(三项都成立时取码值最小的 StaleQuote);`ExecDecision` 的 `arg = 1` |
| EXE-347.7 | `venue_ready(a1, A) = false` 且在途链接第一腿槽 `Unknown` | `v[2] = 4`、`arg = 4` |

### EXE-348 进入 SerialFallback 后不切回配对,期间无自动补腿,到达后 serial 清零

- 规则:[保证金危急的默认减仓](README.md#保证金危急的默认减仓a5与-engine_flatten)
- 级别:unit
- 里程碑:M2
- 前置:C0;同 EXE-347.1 进入串行;引擎目标 S +8.00 / P −8.0
- 步骤:
  1. P 减到 −8.0;S 报价在 t 5000 恢复;推进
  2. S 卖到 +8.00;推进
- 期望:
  - 步骤 1:S 报价恢复前无 S 订单、无 `HedgeFix`;恢复后卖 S 2.00 无 LegLink(不切回 PairOnFill)
  - 步骤 2:`State → Idle`;`serial = 0`
- 实现:`crates/hft-exec/tests/force.rs::exe_348_serial_sticky`

### EXE-349 永续腿报价不可用时门控等待,不进入 SerialFallback

- 规则:[保证金危急的默认减仓](README.md#保证金危急的默认减仓a5与-engine_flatten)
- 级别:unit
- 里程碑:M2
- 前置:C0;a1 L3;g7 S +10.00、P −10.0;P 报价不可用、S 健康
- 步骤:
  1. `force_deleverage(a1, B, 0.2)`;`advance`
- 期望:
  - 无订单;`serial = 0`;无 `ExecSerialFallback`;`gated_since = now`
- 实现:`crates/hft-exec/tests/force.rs::exe_349_perp_quote_gates`

### EXE-350 非 Broken 两腿 cell 用 PairOnFill(PerpFirst, force_slice, TrimExcess),单腿用 TakerSlice(force_slice)

- 规则:[保证金危急的默认减仓](README.md#保证金危急的默认减仓a5与-engine_flatten)
- 级别:table
- 里程碑:M2
- 前置:C0(`force_slice = {0.3, 5, 0.005, 1000 USDT, 0, 200 ms}`);a1 L3;`frac = 0.5`
- 实现:`crates/hft-exec/tests/force.rs::exe_350_algo`

| 行 | cell | 期望第一片 |
|---|---|---|
| EXE-350.1 | g7 S +10.00、P −10.0 | 买 P 5.0,带 LegLink(第二腿卖 S、`HedgePlan.residual = TrimExcess`),`ExecDecision.algo = PairOnFill`、`first_leg_ix = 1` |
| EXE-350.2 | g8 P −10.0 | 买 P 5.0,`algo = TakerSlice` |

### EXE-351 Broken 的 cell 按腿串行:永续 reduce-only 先,现货后,现货 ≤ min(库存, 可用)

- 规则:[保证金危急的默认减仓](README.md#保证金危急的默认减仓a5与-engine_flatten)
- 级别:unit
- 里程碑:M2
- 前置:C0;a1 L3;g7 Broken,S +10.00、P −6.0;S 库存 10.00、可用 3.00
- 步骤:
  1. `force_deleverage(a1, B, 0.5)`;`advance`(引擎目标 S +5.00、P −3.0)
  2. P 到达 −3.0 之前每次推进
  3. P 到达 −3.0 后推进
- 期望:
  - 步骤 1–2:只有买 P 的 `ForceStep` 单(reduce-only、无 LegLink、`ExecDecision.algo = Engine`);无 S 单;`view.state = Broken`;无 `origin = Target` 订单
  - 步骤 3:卖 S 3.00(受可用余额)
- 实现:`crates/hft-exec/tests/force.rs::exe_351_broken_serial`

### EXE-352 无组永续持仓 u_i 的计算与装入条件

- 规则:[保证金危急的默认减仓](README.md#保证金危急的默认减仓a5与-engine_flatten)
- 级别:table
- 里程碑:M2
- 前置:C0(`dust_notional = 5 USDT`);a1 L3;`force_deleverage(a1, B, 0.5)`;mark 100.00
- 实现:`crates/hft-exec/tests/force.rs::exe_352_ungrouped_u`

| 行 | 账户持仓 `perp(B, P)` / 各 cell 的 `group_pos` | 期望无组 cell |
|---|---|---|
| EXE-352.1 | −5.0 / g8 −3.0 | 装入 P,`u = −2.0`,目标 −1.0 |
| EXE-352.2 | P4(`InstId(25)`)−4.0 / 无 cell 引用 | 装入 P4,`u = −4.0`,目标 −2.0 |
| EXE-352.3 | −5.0 / Shadow 的 g16 引用 P,`group_pos = 0` | 装入 P,`u = −5.0` |
| EXE-352.4 | −1.0 / g8 −3.0(`u = +2.0`,与账户持仓反号) | 不装入 |
| EXE-352.5 | −3.05 / g8 −3.0(`u = −0.05`,5 USDT) | 不装入 |
| EXE-352.6 | −1.0 / g8 +1.0(`u = −2.0`,同号) | 装入,数量取 |账户持仓| 1.0,目标 −0.5 |

### EXE-353 无组 cell 至多装 4 条腿,按名义从大到小,到达后换入下一条

- 规则:[保证金危急的默认减仓](README.md#保证金危急的默认减仓a5与-engine_flatten)
- 级别:unit
- 里程碑:M2
- 前置:C0;a1 L3;venue B 上 6 个无组永续持仓,名义 600、500、400、300、200、100 USDT(各 inst 不同)
- 步骤:
  1. `force_deleverage(a1, B, 1.0)`;`advance`
  2. 名义 600 的那条到达目标 0 后,下一次 `force_deleverage(a1, B, 1.0)`;`advance`
- 期望:
  - 步骤 1:装入 600、500、400、300 四条;`Note{ExecUngroupedReduce}` 的 `v[4] = 4`
  - 步骤 2:装入 500、400、300、200
- 实现:`crates/hft-exec/tests/force.rs::exe_353_ungrouped_top4`

### EXE-354 无组 cell 的记录:只写四种决定、P0 Note、不通知策略,无组现货不动

- 规则:[保证金危急的默认减仓](README.md#保证金危急的默认减仓a5与-engine_flatten)、[`ExecDecision` 与进度通知](README.md#execdecision-与进度通知)
- 级别:table
- 里程碑:M2
- 前置:C0;a1 无组持仓:P −3.0(300 USDT)、P4 −2.0(200 USDT),另有无组现货 S +5.00(`unassigned`)
- 实现:`crates/hft-exec/tests/force.rs::exe_354_ungrouped_records`

| 行 | 动作 | 期望 |
|---|---|---|
| EXE-354.1 | `force_deleverage(a1, B, 0.5)` + `advance` | `ExecDecision{ForceStep, group = GROUP_NONE, detail = 5 \| 4 << 8}`;`Note{ExecUngroupedReduce}` P0,`v = [1, 2, 2, 500, 2, 3]`;`ExecOut.notify` 无该 cell 的条目;无 S 订单 |
| EXE-354.2 | `engine_flatten(a1)` + `advance` | `v = [1, 0xFF, 2, 500, 2, 4]` |
| EXE-354.3 | 全过程 | 无组 cell 的 `ExecDecision.action` 只出现 `ForceStep`、`Slice`、`State`、`TargetCleared`;`owner = i1` |

### EXE-355 L3 回落:非串行 cell 与无组 cell 清除引擎目标、通知 Done,不恢复策略目标

- 规则:[保证金危急的默认减仓](README.md#保证金危急的默认减仓a5与-engine_flatten)
- 级别:unit
- 里程碑:M2
- 前置:C0;a1 L3,g8 引擎目标 −8.0(替换了策略目标 −10.0),一片在途;无组 cell 装入 P4
- 步骤:
  1. `on_risk(a1, MarginStress{venue = B, level = L2})`;`advance`
  2. 在途片终态;`advance`
- 期望:
  - 步骤 1 后 Acct 行 `force_active = 0`
  - 步骤 2:g8 `ExecDecision{TargetCleared, eflags = CLEARED_BY_ENGINE}`,通知 `Done{cleared_by_engine: true}`;`view(a1, g8)` 无目标(不恢复 −10.0);无组 cell `TargetCleared`;此后无新单
- 实现:`crates/hft-exec/tests/force.rs::exe_355_l3_fall_clears`

### EXE-356 L3 回落时串行 cell 改设 Hard 对齐目标

- 规则:[保证金危急的默认减仓](README.md#保证金危急的默认减仓a5与-engine_flatten)
- 级别:unit
- 里程碑:M2
- 前置:C0;a1 L3,g7 `serial = 1`(S 报价断),当前 S +10.00、P −6.0
- 步骤:
  1. `on_risk(a1, MarginStress{B, L2})`;`advance`
  2. S 报价恢复;推进直到到达
- 期望:
  - 步骤 1:`ExecDecision{ForceStep, detail = 5 \| 6 << 8}`;引擎目标 P −6.0、S +6.00,`reason = Hard`、`urgency = Urgent`、`origin = Engine(MarginStress)`;`serial = 1`
  - S 报价恢复前无订单、无 `FixExhausted`、不 Broken
  - 步骤 2:卖 S 4.00(无 LegLink);到达时 `ExecDecision{State, state_to = Idle}` 一条、通知 `Done{cleared_by_engine: true}`、`serial = 0`,对齐目标随之清除(`view` 无目标,不另写 `TargetCleared`)
- 实现:`crates/hft-exec/tests/force.rs::exe_356_hard_align`

### EXE-357 对齐目标被替换:新的引擎目标沿用 serial,策略目标清除 serial

- 规则:[保证金危急的默认减仓](README.md#保证金危急的默认减仓a5与-engine_flatten)
- 级别:table
- 里程碑:M2
- 前置:C0;g7 处于 `HardAlign` 对齐目标,`serial = 1`;a1 已不在 `force_active`
- 实现:`crates/hft-exec/tests/force.rs::exe_357_hardalign_replaced`

| 行 | 替换者 | 期望 |
|---|---|---|
| EXE-357.1 | 再次 L3 `force_deleverage(a1, B, 0.2)` | 新引擎目标;`serial = 1`;下一片无 LegLink |
| EXE-357.2 | 策略 `set_target`(`Rebalance` 减仓目标) | 接受;`serial = 0`;S 报价仍不可用时按预检门控等待;剩余的多出一腿交补腿 |

### EXE-358 L3 回落后 Broken 的 cell 仍为 Broken,已到期的自动平仓立即检查

- 规则:[保证金危急的默认减仓](README.md#保证金危急的默认减仓a5与-engine_flatten)、[Broken 处置](README.md#broken-处置)
- 级别:unit
- 里程碑:M2
- 前置:C0;a1 L3;g7 Broken(t 0,`due_tsc = t 120 000`),引擎目标进行中;静止
- 步骤:
  1. t 200 000 `on_risk(a1, MarginStress{B, L2})`;`advance`
- 期望:
  - `view.state = Broken`;同一 `advance` 内 `AutoFix/Start`
- 实现:`crates/hft-exec/tests/force.rs::exe_358_l3_fall_broken_autofix`

### EXE-359 默认减仓期间的 set_target:只接受每腿减得更多且不变号的目标

- 规则:[保证金危急的默认减仓](README.md#保证金危急的默认减仓a5与-engine_flatten)、[目标的接受与替换](README.md#目标的接受与替换)
- 级别:table
- 里程碑:M2
- 前置:C0;a1 `force_active`;g7 引擎目标 S +8.00 / P −8.0,`serial = 1`
- 实现:`crates/hft-exec/tests/force.rs::exe_359_set_target_during_force`

| 行 | 策略目标 | 期望 |
|---|---|---|
| EXE-359.1 | S +6.00 / P −6.0,`Rebalance` | `Ok`;替换引擎目标;后续片 `first_leg_ix = 1`、`urgency = Urgent`、`origin = Engine`;`serial = 1` |
| EXE-359.2 | S 0 / P 0,`Exit` | `Ok` |
| EXE-359.3 | S +8.00 / P −8.0(相等) | `Ok` |
| EXE-359.4 | S +9.00 / P −8.0 | `Err(ForcedReduceActive)`,`detail = 1 \| 7 << 8` |
| EXE-359.5 | S +6.00 / P +1.0(P 变号) | `Err(ForcedReduceActive)` |
| EXE-359.6 | g7 Broken 时任一目标 | `Err(CellBroken)`,`detail = 1 \| 6 << 8` |

### EXE-360 每一步写 ForceStep 决定与 P1 Note

- 规则:[保证金危急的默认减仓](README.md#保证金危急的默认减仓a5与-engine_flatten)
- 级别:unit
- 里程碑:M2
- 前置:C0;a1 L3;g8 P −10.0
- 步骤:
  1. 连续三步 `force_deleverage(a1, B, 0.2)`,每步后 `advance`
- 期望:
  - 每步恰好一条 `Note{ExecForceStep}` P1 与一条 `ExecDecision{ForceStep, detail = 5 \| 1 << 8}`;快照 cell 行 `force_steps = 3`
- 实现:`crates/hft-exec/tests/force.rs::exe_360_step_records`

### EXE-361 engine_flatten:全部 cell 与全部 venue 的无组永续持仓设目标 0,置清仓锁,幂等

- 规则:[保证金危急的默认减仓](README.md#保证金危急的默认减仓a5与-engine_flatten)
- 级别:unit
- 里程碑:M2
- 前置:C0;a1:g7(S +10.00、P −10.0)、g8(P 0,无目标)、g11 Broken(P −2.0、Q +2.5);无组持仓 P4 −1.0(B)(账户 Q 持仓即 g11 的 +2.5,无组 Q 为 0);a2 不受影响
- 步骤:
  1. `engine_flatten(a1)`;`advance`
  2. 再次 `engine_flatten(a1)`;`advance`
- 期望:
  - 步骤 1:g7、g8、g11 与无组 cell 各有 `ExecDecision{ForceStep, detail = 5 \| 1 << 8, reason = Hard, origin = Engine(KillFlatten)}`(无组 cell 为 `Engine/Ungrouped`),引擎目标各腿 0;Acct 行 `flatten_lock = 1`、`force_kind = 0`;a2 无决定
  - 步骤 2:与步骤 1 后相比,引擎目标与锁不变;不重复下已在途的量
- 实现:`crates/hft-exec/tests/flatten.rs::exe_361_engine_flatten_all`

### EXE-362 清仓的算法:非 Broken 配对永续先、第二腿不可用退回串行;Broken 串行;CapCleared{Broken} 不清目标

- 规则:[保证金危急的默认减仓](README.md#保证金危急的默认减仓a5与-engine_flatten)
- 级别:table
- 里程碑:M2
- 前置:C0;`engine_flatten(a1)` 后
- 实现:`crates/hft-exec/tests/flatten.rs::exe_362_flatten_algo`

| 行 | cell | 期望 |
|---|---|---|
| EXE-362.1 | g7 非 Broken,两腿可用 | 第一腿买 P 带 LegLink,`first_leg_ix = 1` |
| EXE-362.2 | g7 非 Broken,在途链接 `LINK_HOLD` | `SerialFallback` 原因 5 |
| EXE-362.3 | g7 Broken | 只出买 P reduce-only(无 LegLink),P 到 0 后才卖 S |
| EXE-362.4 | g7 Broken,裸量降到尘埃线下后收到 `CapCleared{Broken}` | 引擎目标仍为 0;`State → Working`;继续推进到 0 |

### EXE-363 HARD 生效时清仓单全部停止,目标保留并 Stuck{Gated(HARD)}

- 规则:[保证金危急的默认减仓](README.md#保证金危急的默认减仓a5与-engine_flatten)
- 级别:unit
- 里程碑:M2
- 前置:C0;`engine_flatten(a1)` 进行中,g8 P −10.0;t 0 全局 HARD 生效(`v.risk.hard = true`)
- 步骤:
  1. t 0–t 5000 每 500 ms 推进
- 期望:
  - 无订单;引擎目标仍为 0;t 1000 `ExecDecision{State, state_to = Stuck, detail = 2 \| 1 << 8, gate = HARD}` 与 `Note{ExecStuck}` P1 各一条,此后不重复
- 实现:`crates/hft-exec/tests/flatten.rs::exe_363_hard_stops_flatten`

### EXE-364 清仓锁期间 set_target 只接受比参照量更接近 0 的目标

- 规则:[保证金危急的默认减仓](README.md#保证金危急的默认减仓a5与-engine_flatten)、[目标的接受与替换](README.md#目标的接受与替换)
- 级别:table
- 里程碑:M2
- 前置:C0;a1 `flatten_lock = 1`
- 实现:`crates/hft-exec/tests/flatten.rs::exe_364_lock_accept`

| 行 | 状态 / 目标 | 期望 |
|---|---|---|
| EXE-364.1 | g8 清仓进行中(引擎目标 0,P −5.0)/ P −3.0 | `Err(FlattenLocked)`,`detail = 1 \| 9 << 8` |
| EXE-364.2 | 同上 / P 0,`Exit` | `Ok`;后续片 `urgency = Urgent`、`origin = Engine` |
| EXE-364.3 | 清仓已完成、引擎目标已清除,P 0 / P −1.0 | `Err(FlattenLocked)` |
| EXE-364.4 | g8 后来出现仓位 P −2.0(无引擎目标)/ P −1.5 | `Ok` |
| EXE-364.5 | 同 .4 / P +1.0(变号) | `Err(FlattenLocked)` |
| EXE-364.6 | 同 .4 / P −2.5(远离 0) | `Err(FlattenLocked)` |
| EXE-364.7 | g7 清仓进行中 / S 0、P 0 的 `Exit`,`first_leg = SpotFirst` | `Ok`;下一片 `first_leg_ix = 1`(仍 PerpFirst) |

### EXE-365 清仓锁与默认减仓期间 clear_target 一律忽略

- 规则:[目标的接受与替换](README.md#目标的接受与替换)、[保证金危急的默认减仓](README.md#保证金危急的默认减仓a5与-engine_flatten)
- 级别:table
- 里程碑:M2
- 前置:C0;g8 P −5.0
- 实现:`crates/hft-exec/tests/flatten.rs::exe_365_clear_ignored`

| 行 | 状态与序列 | 期望 |
|---|---|---|
| EXE-365.1 | `flatten_lock`,清仓进行中;`clear_target` | `ExecDecision{TargetRejected, detail = 1 \| 9 << 8}`;引擎目标 0 仍在,下一次推进照常出单 |
| EXE-365.2 | `flatten_lock`;先 `set_target(P 0, Exit)` 被接受,再 `clear_target` | 第二步被忽略(`detail = 1 \| 9 << 8`);目标 0 仍在 |
| EXE-365.3 | `force_active`;`clear_target` | `ExecDecision{TargetRejected, detail = 1 \| 7 << 8}`;引擎目标仍在 |

### EXE-366 清仓锁只由该账户 Kill cap 的 CapCleared 清除

- 规则:[保证金危急的默认减仓](README.md#保证金危急的默认减仓a5与-engine_flatten)
- 级别:table
- 里程碑:M2
- 前置:C0;a1 `flatten_lock = 1`,清仓已完成
- 实现:`crates/hft-exec/tests/flatten.rs::exe_366_lock_release`

| 行 | 事件 | 期望 Acct 行 `flatten_lock` |
|---|---|---|
| EXE-366.1 | `on_risk(a1, CapCleared{Kill})` | 0;之后增仓目标被接受 |
| EXE-366.2 | `on_risk(a1, CapCleared{Broken, g7})` | 1 |
| EXE-366.3 | `on_risk(a1, CapCleared{HedgeFailure})` | 1 |
| EXE-366.4 | `on_risk(a2, CapCleared{Kill})` | 1(另一账户) |
| EXE-366.5 | `clear_account(a1)` | 1 |
| EXE-366.6 | `snapshot` + `restore` + `recovery_end` | 1 |

### EXE-367 清仓锁期间的 L3 引擎目标仍为 0

- 规则:[保证金危急的默认减仓](README.md#保证金危急的默认减仓a5与-engine_flatten)
- 级别:unit
- 里程碑:M2
- 前置:C0;`engine_flatten(a1)` 进行中,g8 P −5.0,引擎目标 0;a1 进入 L3
- 步骤:
  1. `force_deleverage(a1, B, 0.2)`;`advance`
- 期望:
  - g8 引擎目标仍为 0(不变成 −4.0);`ExecDecision{ForceStep}` 的 `target_qty = 0`
- 实现:`crates/hft-exec/tests/flatten.rs::exe_367_l3_under_lock_zero`

### EXE-368 清仓锁账户上没有任何增加 |仓位| 的订单;默认减仓期间该账户订单都是 origin = Engine

- 规则:[保证金危急的默认减仓](README.md#保证金危急的默认减仓a5与-engine_flatten)、[增仓与减仓的分类](README.md#增仓与减仓的分类)
- 级别:prop
- 里程碑:M2
- 性质:`flatten_lock` 账户上任何来源(引擎或目标)的订单都不增加任何腿的 |仓位|;`force_active` 或 `flatten_lock` 期间该账户的每条订单 `origin = Engine`
- 生成:同 EXE-050,另含 `engine_flatten`、`force_deleverage`、策略每步给出随机方向的目标与 `clear_target`
- 参考:断言
- 例数:PR 2 000
- 实现:`crates/hft-exec/tests/props.rs::exe_368_lock_no_increase`

### EXE-369 设下引擎目标时的 cell 状态与到达记录

- 规则:[保证金危急的默认减仓](README.md#保证金危急的默认减仓a5与-engine_flatten)、[状态机](README.md#状态机)
- 级别:table
- 里程碑:M2
- 前置:C0;a1 L3,`force_deleverage(a1, B, 0.2)`
- 实现:`crates/hft-exec/tests/force.rs::exe_369_engine_target_state`

| 行 | 设定前 | 期望 |
|---|---|---|
| EXE-369.1 | g8 策略目标 Working | `State → Stuck{EngineOverride}`;到达引擎目标时 `ExecDecision{State, state_from = Stuck, state_to = Idle}` |
| EXE-369.2 | g8 无目标,P −10.0 | `ExecDecision{State, state_from = Idle, state_to = Working}`;到达时 `State → Idle` |
| EXE-369.3 | g8 无目标,`engine_flatten`,P 0 | 同一 `advance` 内 `State → Working` 与 `State → Idle` 各一条;无订单 |
### EXE-380 门控拒绝:记 gated_bits,gated_backoff_ms 之后再试

- 规则:[门控拒绝与退避](README.md#门控拒绝与退避)
- 级别:table
- 里程碑:M2
- 前置:C0(`gated_backoff_ms = 500`)、T1;g8 目标 P −10.0;第一片在 t 0 被 `Rejected(RATE)` 拒
- 实现:`crates/hft-exec/tests/gate.rs::exe_380_gate_backoff`

| 行 | 推进时刻 | 期望 |
|---|---|---|
| EXE-380.1 | t 0(回填之后) | `gated_bits = RATE`;`gated_since = t 0`;`wake_at = t 500`;`fail_streak = 0` |
| EXE-380.2 | t 499 | 无订单 |
| EXE-380.3 | t 500 | 再发一片 |

### EXE-381 门控期间每个 cell 每 gated_backoff_ms 至多一条被拒意图

- 规则:[门控拒绝与退避](README.md#门控拒绝与退避)
- 级别:unit
- 里程碑:M2
- 前置:C0(`gated_backoff_ms = 500`)、T1;g8 目标 P −10.0;t 0–t 10 000 每片都被 `KILLED` 拒;行情每 1 ms 一次 `touch_inst(P)`
- 步骤:
  1. 驱动到 t 9999
- 期望:
  - g8 的订单恰好 20 条,时刻为 t 0、t 500、…、t 9500
- 实现:`crates/hft-exec/tests/gate.rs::exe_381_backoff_bounded`

### EXE-382 连续执行失败达到 max_slice_fails 进入 Stuck{SliceFails},直到目标被替换或清除

- 规则:[门控拒绝与退避](README.md#门控拒绝与退避)、[状态机](README.md#状态机)
- 级别:table
- 里程碑:M2
- 前置:C0(`max_slice_fails = 3`)、T1;g8 目标 P −10.0
- 实现:`crates/hft-exec/tests/gate.rs::exe_382_slice_fails`

| 行 | 序列 | 期望 |
|---|---|---|
| EXE-382.1 | 2 次交易所拒单 | `fail_streak = 2`;Working |
| EXE-382.2 | 3 次交易所拒单 | `ExecDecision{State, state_to = Stuck, detail = 2 \| 5 << 8}`;`Note{ExecStuck}` P1;此后无新片 |
| EXE-382.3 | 接 .2,1 h 内持续推进 | 无订单 |
| EXE-382.4 | 接 .2,`clear_target` 后 `set_target`(同内容) | Working;`fail_streak = 0`;出一片 |
| EXE-382.5 | 2 次 `EncodeError` + 1 次交易所拒单 | 同 .2 |
| EXE-382.6 | 2 次拒单后 1 次部分成交,再 2 次拒单 | `fail_streak = 2`;Working |
| EXE-382.7 | g8 L3 引擎目标 3 次 `RiskReject` 后下一步 L3 重设目标 | `fail_streak = 0`;离开 Stuck |

### EXE-383 连续 IOC 零成交达到 max_nofill 进入 Stuck{NoLiquidity},stuck_after_ms 后再试

- 规则:[门控拒绝与退避](README.md#门控拒绝与退避)
- 级别:table
- 里程碑:M2
- 前置:C0(`max_nofill = 5`、`stuck_after_ms = 2000`)、T1(`cooldown_ms = 300`);g8 目标 P −10.0
- 实现:`crates/hft-exec/tests/gate.rs::exe_383_nofill`

| 行 | 序列 | 期望 |
|---|---|---|
| EXE-383.1 | 4 次零成交 | `nofill_streak = 4`;Working |
| EXE-383.2 | 第 5 次零成交于 t 1200 | `ExecDecision{State, state_to = Stuck, detail = 2 \| 3 << 8}` |
| EXE-383.3 | 接 .2,t 3199 推进 | 无订单 |
| EXE-383.4 | 接 .2,t 3200 推进 | 再发一片 |
| EXE-383.5 | 4 次零成交后部分成交 | `nofill_streak = 0`、`fail_streak = 0` |

### EXE-384 有效模式变化、CapCleared 与 venue_ready 恢复立即重置 wake_at 并标记待推进

- 规则:[门控拒绝与退避](README.md#门控拒绝与退避)
- 级别:table
- 里程碑:M2
- 前置:C0、T1;g8 目标 P −10.0;t 0 被门控拒,`wake_at = t 500`
- 实现:`crates/hft-exec/tests/gate.rs::exe_384_wake_reset`

| 行 | t 100 的事件 | 期望 |
|---|---|---|
| EXE-384.1 | `on_mode(a1, g8, CloseOnly, Live)` | `has_pending() = true`;t 100 `advance` 出一片 |
| EXE-384.2 | `on_risk(a1, CapCleared{Kill})` | 同上 |
| EXE-384.3 | `venue_ready(a1, B)` 由 false 变 true(经 `on_risk` 投递) | 同上 |

### EXE-385 OMS 本地发送失败按类别:不计次的按门控,EncodeError 计执行失败

- 规则:[门控拒绝与退避](README.md#门控拒绝与退避)
- 级别:table
- 里程碑:M2
- 前置:C0、T1;g8 一片已交出;OMS 回报该槽本地发送失败
- 实现:`crates/hft-exec/tests/gate.rs::exe_385_local_send_fail`

| 行 | 类别 | 期望 |
|---|---|---|
| EXE-385.1 | `KillBlocked` | 门控:`fail_streak = 0`、`wake_at = now + 500` |
| EXE-385.2 | `RateLimited` | 门控 |
| EXE-385.3 | `NoConn` | 门控 |
| EXE-385.4 | `RingFull` | 门控 |
| EXE-385.5 | `EncodeError` | `fail_streak = 1` |

### EXE-390 Shadow 下目标照常接受并带 SHADOW 标记

- 规则:[运行模式、Drain 与 Unknown](README.md#运行模式drain-与-unknown)
- 级别:unit
- 里程碑:M2
- 前置:C0、T1;g8 有效模式 Shadow
- 步骤:
  1. `set_target`(P −10.0)
- 期望:
  - `Ok`;`ExecDecision{TargetSet, eflags = SHADOW}`
- 实现:`crates/hft-exec/tests/modes.rs::exe_390_shadow_target_flag`

### EXE-391 Shadow 下第一片交出一次,随后 Stuck{Shadow},目标变化或回到 Live 前不再出片

- 规则:[运行模式、Drain 与 Unknown](README.md#运行模式drain-与-unknown)
- 级别:table
- 里程碑:M2
- 前置:C0、T1;g8 有效模式 Shadow;目标 P −10.0
- 实现:`crates/hft-exec/tests/modes.rs::exe_391_shadow_one_slice`

| 行 | 序列 | 期望 |
|---|---|---|
| EXE-391.1 | `advance`;`on_place_result(Shadowed)` | 一条 `ExecOrder`;随后 `ExecDecision{State, state_to = Stuck, detail = 2 \| 2 << 8}` |
| EXE-391.2 | 接 .1,60 s 内每 100 ms `touch_inst(P)` | 无订单 |
| EXE-391.3 | 接 .1,`set_target`(P −12.0) | 再交出一片,再 `Stuck{Shadow}` |
| EXE-391.4 | 接 .1,`on_mode(a1, g8, Shadow, Live)` | Working;出一片 |

### EXE-392 Live 下 open_allowed = false 的增仓与 Shadow 同样只交出一次

- 规则:[运行模式、Drain 与 Unknown](README.md#运行模式drain-与-unknown)
- 级别:unit
- 里程碑:M2
- 前置:C0、T1;g8 模式 Live、`open_allowed = false`;目标 P −10.0(增仓)
- 步骤:
  1. `advance`;`on_place_result(Shadowed)`;此后 10 s 每 100 ms 推进
- 期望:
  - 恰好一条订单;`Stuck{Shadow}`
- 实现:`crates/hft-exec/tests/modes.rs::exe_392_open_not_allowed_once`

### EXE-393 CloseOnly:增仓切片不发,配对开仓整笔不发,减仓照常

- 规则:[运行模式、Drain 与 Unknown](README.md#运行模式drain-与-unknown)
- 级别:table
- 里程碑:M2
- 前置:C0(`stuck_after_ms = 2000`);a1 有效模式 CloseOnly
- 实现:`crates/hft-exec/tests/modes.rs::exe_393_close_only`

| 行 | 目标 | 期望 |
|---|---|---|
| EXE-393.1 | g8 P 0 → −10.0 | 无订单;t + 2000 `Stuck{Gated(CLOSE_ONLY)}`(`detail = 2 \| 1 << 8`、`gate ⊇ CLOSE_ONLY`) |
| EXE-393.2 | g7 开仓 S +10 / P −10 | 两腿都无订单 |
| EXE-393.3 | g8 P −10.0 → −5.0 | 买 P 5.0 |

### EXE-394 非 HARD 的 Kill:增仓门控,减仓、补腿、自动平仓与 L3 照常

- 规则:[运行模式、Drain 与 Unknown](README.md#运行模式drain-与-unknown)
- 级别:table
- 里程碑:M2
- 前置:C0;a1 非 HARD `Kill{Account}`(`open_blocked = true`,风控以 `KILLED` 拒 Open)
- 实现:`crates/hft-exec/tests/modes.rs::exe_394_soft_kill`

| 行 | 场景 | 期望 |
|---|---|---|
| EXE-394.1 | g8 增仓目标 | 预检不出单;`Stuck{Gated}` |
| EXE-394.2 | g8 减仓目标 | 出单 |
| EXE-394.3 | g7 补腿条件成立 | 出 `HedgeFix` 单 |
| EXE-394.4 | g7 Broken 到期 | `AutoFix/Start` |
| EXE-394.5 | a1 L3 一步 | 出 `ForceStep` 单 |

### EXE-395 Draining 只停增仓切片

- 规则:[运行模式、Drain 与 Unknown](README.md#运行模式drain-与-unknown)
- 级别:table
- 里程碑:M2
- 前置:C0;引擎处于 Draining,风控以 `DRAINING` 拒 Open
- 实现:`crates/hft-exec/tests/modes.rs::exe_395_draining`

| 行 | 场景 | 期望 |
|---|---|---|
| EXE-395.1 | g8 增仓切片 | 被 `DRAINING` 拒,按门控(`fail_streak = 0`) |
| EXE-395.2 | g8 减仓切片 | 出单 |
| EXE-395.3 | g7 补腿 | 出单 |
| EXE-395.4 | g7 外来成交截止 | 出 `ExternalDefault` 单 |
| EXE-395.5 | Draining 中 `engine_flatten(a1)` | 设目标并出 `ForceStep` 单;`flatten_lock = 1` |

### EXE-396 drain_ready_acct 的各项条件

- 规则:[入口](README.md#入口全部在-sequencer-线程调用无分配)、[运行模式、Drain 与 Unknown](README.md#运行模式drain-与-unknown)
- 级别:table
- 里程碑:M2
- 前置:C0;a1 只有所列状态,a2 空闲
- 实现:`crates/hft-exec/tests/modes.rs::exe_396_drain_ready_acct`

| 行 | a1 的状态 | 期望 `drain_ready_acct(a1)` / `drain_ready()` |
|---|---|---|
| EXE-396.1 | 全部 cell Idle,无计时 | true / true |
| EXE-396.2 | g8 一片 IOC 在途 | false / false |
| EXE-396.3 | g7 补腿条件成立、冷却中 | false / false |
| EXE-396.4 | g7 `ext_deadline ≠ 0` | false / false |
| EXE-396.5 | g7 自动平仓 `Running` | false / false |
| EXE-396.6 | g8 L3 引擎目标未到达、无在途 | false / false |
| EXE-396.7 | g8 清仓目标未到达 | false / false |
| EXE-396.8 | 无组 cell 引擎目标未到达 | false / false |
| EXE-396.9 | g7 `HardAlign` 对齐目标未到达 | false / false |
| EXE-396.10 | g8 策略目标未到达、无在途、无计时 | true / true |
| EXE-396.11 | g7 Broken、`Primed` 未到期 | true / true |
| EXE-396.12 | g8 L3 引擎目标已到达(仍在) | true / true |
| EXE-396.13 | a1 同 .1,a2 同 .2 | true / false |

### EXE-397 Drain 中清仓:片间冷却期间 drain_ready 为假,到达后为真

- 规则:[运行模式、Drain 与 Unknown](README.md#运行模式drain-与-unknown)
- 级别:unit
- 里程碑:M2
- 前置:C0(`force_slice.cooldown_ms = 200`、`max_chunk_notional = 1000`);g8 P −20.0;Draining 中 t 0 `engine_flatten(a1)`
- 步骤:
  1. t 0 第一片买 P 9.9 成交;t 100 查询;t 200 第二片 9.9 成交;t 400 第三片 0.2 成交;t 400 查询
- 期望:
  - t 100:`drain_ready() = false`(无在途但目标未到达);t 400 到达后 `drain_ready() = true`
- 实现:`crates/hft-exec/tests/modes.rs::exe_397_drain_with_flatten`

### EXE-398 Unknown 槽:不重发、不反向,等收敛

- 规则:[运行模式、Drain 与 Unknown](README.md#运行模式drain-与-unknown)
- 级别:table
- 里程碑:M2
- 前置:C0、T1;g8 目标 P −10.0
- 实现:`crates/hft-exec/tests/modes.rs::exe_398_unknown_wait`

| 行 | 场景 | 期望 |
|---|---|---|
| EXE-398.1 | 一片进入 `Unknown`,1 min 内持续推进 | 无订单(策略目标不适用扣量例外) |
| EXE-398.2 | 该槽收敛为 `Filled` 4.0 | 按新净量继续出片 |
| EXE-398.3 | g7 `legs.has_unknown = true` 且裸量超阈值 | 无 `HedgeFix` |

### EXE-400 set_target 的每个拒绝原因

- 规则:[目标的接受与替换](README.md#目标的接受与替换)
- 级别:table
- 里程碑:M2
- 前置:C0、T0;g7 当前无目标,除各行所改
- 实现:`crates/hft-exec/tests/accept.rs::exe_400_reject_codes`

| 行 | 输入 | 期望返回与 `ExecDecision{TargetRejected}.detail` |
|---|---|---|
| EXE-400.1 | i2(绑定 a2)对 (a1, g7) 设目标 | `NotOwner`,`1 \| 1 << 8` |
| EXE-400.2 | i1 对不在其 GroupSpec 中的 `GroupId(0x0200_0009)` | `NotOwner` |
| EXE-400.3 | `legs[0].leg_ix = 2`(只有 2 条腿) | `BadLegs`,`1 \| 2 << 8` |
| EXE-400.4 | 两条 `leg_ix = 0` | `BadLegs` |
| EXE-400.5 | `Fixed`,`first_leg_ix = 5` | `SpecInvalid`,`1 \| 3 << 8` |
| EXE-400.6 | `MakerFirst`(feature 未启用) | `AlgoUnsupported`,`1 \| 4 << 8` |
| EXE-400.7 | S −1.00 | `SpotShort`,`1 \| 5 << 8` |
| EXE-400.8 | g7 Broken | `CellBroken`,`1 \| 6 << 8` |
| EXE-400.9 | a1 `force_active`,增大的目标 | `ForcedReduceActive`,`1 \| 7 << 8` |
| EXE-400.10 | 三腿 `PairOnFill` | `TooManyLegs`,`1 \| 8 << 8` |
| EXE-400.11 | a1 `flatten_lock`,远离 0 的目标 | `FlattenLocked`,`1 \| 9 << 8` |
| EXE-400.12 | 增仓 lcm 名义超阈值 | `LotMismatch`,`1 \| 10 << 8` |
| EXE-400.13 | 任一行被拒后 | 原有目标(若有)不变;`view` 不变 |

### EXE-401 与当前目标逐字段相同的目标视为已接受,不写决定、不清计数

- 规则:[目标的接受与替换](README.md#目标的接受与替换)
- 级别:unit
- 里程碑:M2
- 前置:C0、T1;g8 目标 P −10.0 进行中,`filled[0] = −3.0`、`fail_streak = 2`、`nofill_streak = 1`
- 步骤:
  1. 同一目标 `set_target` 1000 次
- 期望:
  - 每次返回 `Ok`;无 `ExecDecision`;`filled[0] = −3.0`、`fail_streak = 2`、`nofill_streak = 1`;`ExecOut.timers` 为空
- 实现:`crates/hft-exec/tests/accept.rs::exe_401_identical_target`

### EXE-402 任一字段不同即替换

- 规则:[目标的接受与替换](README.md#目标的接受与替换)
- 级别:table
- 里程碑:M2
- 前置:C0、T1;g8 当前目标 P −10.0、`Rebalance`、`Normal`、`band = 0.01`、`tag = 0x10`
- 实现:`crates/hft-exec/tests/accept.rs::exe_402_any_field_replaces`

| 行 | 改动的字段 | 期望 |
|---|---|---|
| EXE-402.1 | 腿目标量 −10.1 | `ExecDecision{TargetReplaced}` |
| EXE-402.2 | `reason = Exit` | 同上 |
| EXE-402.3 | `urgency = Urgent` | 同上 |
| EXE-402.4 | `band = 0.02` | 同上 |
| EXE-402.5 | `exec.slice.cooldown_ms = 301` | 同上 |
| EXE-402.6 | `tag = 0x11` | 同上 |

### EXE-403 替换的效果:清零已成与计数,进入 Working,不撤在途 IOC

- 规则:[目标的接受与替换](README.md#目标的接受与替换)
- 级别:unit
- 里程碑:M2
- 前置:C0、T1;g8 目标 P −10.0,`filled[0] = −3.0`、`fail_streak = 1`、`nofill_streak = 2`,一片 IOC 在途
- 步骤:
  1. `set_target`(P −12.0)
- 期望:
  - `ExecDecision{TargetReplaced}`;`filled[0] = 0`、`fail_streak = 0`、`nofill_streak = 0`;`state = Working`;`has_pending() = true`;`ExecOut.cancels` 为空
- 实现:`crates/hft-exec/tests/accept.rs::exe_403_replace_effects`

### EXE-404 Exit 目标与其他减仓目标同样校验

- 规则:[目标的接受与替换](README.md#目标的接受与替换)
- 级别:table
- 里程碑:M2
- 前置:C0、T0
- 实现:`crates/hft-exec/tests/accept.rs::exe_404_exit_validated`

| 行 | `Exit` 目标 | 期望 |
|---|---|---|
| EXE-404.1 | S 0 / P 0 | `Ok` |
| EXE-404.2 | S −1.00 / P 0 | `Err(SpotShort)` |
| EXE-404.3 | i2 发出 | `Err(NotOwner)` |

### EXE-405 clear_account 清除该账户全部目标与执行层计时,不清清仓锁

- 规则:[入口](README.md#入口全部在-sequencer-线程调用无分配)、[人工保持与恢复期抑制](README.md#人工保持与恢复期抑制)
- 级别:unit
- 里程碑:M2
- 前置:C0;s1(Sim)上:g20 = `GroupId(0x0300_0014)` Working(`fix.count = 1`、`ext_deadline ≠ 0`、`hold = MANUAL_HOLD`);g21 = `GroupId(0x0300_0015)` Broken(策略目标冻结,`auto_fix.phase = Primed`);g22 = `GroupId(0x0300_0016)` `serial = 1`;s1 `flatten_lock = 1`
- 步骤:
  1. `clear_account(s1)`
  2. `advance`
- 期望:
  - 步骤 1 后 `ExecOut` 全空
  - 步骤 2:g20 `State → Idle`、`TargetCleared`(`eflags = CLEARED_BY_ENGINE`)、通知 `Done{cleared_by_engine: true}`、`FixState` 与 `ext_deadline` 为 0、`hold = 0`(`Note{ExecManualHold}` Info,原因 4)、外来计时结束记录(`arg = 5`);g21 只清目标,`view.state = Broken`、`auto_fix` 全零;g22 `serial = 0`;无订单;Acct 行 `flatten_lock = 1`
- 实现:`crates/hft-exec/tests/accept.rs::exe_405_clear_account`

### EXE-415 无动作的推进不写 ExecDecision

- 规则:[`ExecDecision` 与进度通知](README.md#execdecision-与进度通知)
- 级别:unit
- 里程碑:M2
- 前置:C0、T1;g8 Working,冷却中(`wake_at` 未到),一片已终态
- 步骤:
  1. 冷却期内 `touch_inst(P)` 100 次,每次后 `advance`
- 期望:
  - 无 `ExecDecision`、无通知
- 实现:`crates/hft-exec/tests/decision.rs::exe_415_no_decision_on_noop`

### EXE-416 ExecUpdate 的投递时机

- 规则:[`ExecDecision` 与进度通知](README.md#execdecision-与进度通知)
- 级别:table
- 里程碑:M2
- 前置:C0、T1;g8 目标 P −10.0
- 实现:`crates/hft-exec/tests/decision.rs::exe_416_exec_update`

| 行 | 事件 | 期望通知 |
|---|---|---|
| EXE-416.1 | 一片终态,成交 −4.0 | `Progress{leg_ix = 0, d = −4.0}` 一条,`origin = Strategy` |
| EXE-416.2 | 一片终态,零成交 | 无 `Progress` |
| EXE-416.3 | 到达目标 | `Done{cleared_by_engine: false}` 一条;之后的推进不再有 `Done` |
| EXE-416.4 | 进入 `Stuck{StaleQuote}`,原因不变时再推进 | `Stuck(StaleQuote)` 一条,不重复 |
| EXE-416.5 | 原因由 `StaleQuote` 变为 `Gated(RATE)` | `Stuck(Gated(RATE))` 一条 |
| EXE-416.6 | 链接 Broken | `Broken(LinkBroken)` 一条 |
| EXE-416.7 | Broken 期间自动平仓一片成交 0.3 | `Progress{d = 0.3}`,`origin = Engine(AutoFix)` |

### EXE-417 每次到达目标通知一次 Done

- 规则:[`ExecDecision` 与进度通知](README.md#execdecision-与进度通知)
- 级别:prop
- 里程碑:M2
- 性质:任意序列中,每条 `Done{cleared_by_engine: false}` 都伴随同一 cell 的 `State → Idle`(到达),一次到达至多一条(到达后因仓位变化离开带、再次到达的再发一条)
- 生成:同 EXE-050
- 参考:断言
- 例数:PR 2 000
- 实现:`crates/hft-exec/tests/props.rs::exe_417_done_once`

### EXE-418 冷读者从 journal 记录重建的执行层状态不漏报,静止时与内部状态相等

- 规则:[`ExecDecision` 与进度通知](README.md#execdecision-与进度通知)
- 级别:prop
- 里程碑:M2
- 性质:只读 `ExecDecision` 序列与风控的 `CapSet`/`CapCleared` 注记,按"冷读者重建"表算出四项(每 cell"未到达的引擎目标"、每账户"清仓未完成"、每 cell"外来成交计时中"、每 cell"自动有界平仓执行中");每个输入之后,内部状态(非 Broken cell 的未到达引擎目标、`flatten_lock ∧` 前者、`ext_deadline ≠ 0`、`auto_fix.phase = Running`)成立 ⇒ 重建成立;该 cell 无在途订单时两者相等
- 生成:同 EXE-050,另含外来成交(各 cause)、策略处理动作、`clear_account`、HARD、`manual_hold`、`engine_flatten` 与 `Resume{Kill}`
- 参考:`tests/model.rs` 的重建函数(按 README 表逐行实现)
- 例数:PR 2 000
- 实现:`crates/hft-exec/tests/props.rs::exe_418_cold_reader_reconstruction`

### EXE-419 外来成交计时的每种结束都有记录

- 规则:[`ExecDecision` 与进度通知](README.md#execdecision-与进度通知)、[外来成交的默认处理](README.md#外来成交的默认处理b3)
- 级别:table
- 里程碑:M2
- 前置:C0;g7 `ExecDecision{ExternalSeen, arg = Timed}` 之后计时中
- 实现:`crates/hft-exec/tests/decision.rs::exe_419_ext_timing_end_records`

| 行 | 结束方式 | 期望该 cell 的结束记录 |
|---|---|---|
| EXE-419.1 | 截止到达 | `ExecDecision{ExternalDefault}` |
| EXE-419.2 | 替换目标 | `ExecDecision{ExternalSeen, detail = 6 \| 0 << 8 \| 5 << 16}` |
| EXE-419.3 | `clear_target` | 同 .2 |
| EXE-419.4 | `note_strategy_action` | 同 .2 |
| EXE-419.5 | Manual 成交置 `manual_hold` | 同 .2 |
| EXE-419.6 | `clear_account(a1)`(随后的 `advance`) | 同 .2 |

### EXE-420 引擎目标的设定与结束都有记录

- 规则:[`ExecDecision` 与进度通知](README.md#execdecision-与进度通知)、[保证金危急的默认减仓](README.md#保证金危急的默认减仓a5与-engine_flatten)
- 级别:table
- 里程碑:M2
- 前置:C0;g8 非 Broken
- 实现:`crates/hft-exec/tests/decision.rs::exe_420_engine_target_records`

| 行 | 序列 | 期望记录 |
|---|---|---|
| EXE-420.1 | L3 设目标 → 到达 | `ForceStep{Engine/Step}` … `State{state_to = Idle}` |
| EXE-420.2 | L3 设目标 → L3 回落清除 | `ForceStep{Engine/Step}` … `TargetCleared` |
| EXE-420.3 | 清仓目标 → 策略 `Exit` 0 替换 | `ForceStep{Engine/Step}` … `TargetReplaced` |
| EXE-420.4 | 无组 cell 装入 → 到达 | `ForceStep{Engine/Ungrouped}` … `State{state_to = Idle}`(`group = GROUP_NONE`) |
| EXE-420.5 | `HardAlign` → 到达 | `ForceStep{Engine/HardAlign}` … `State{state_to = Idle}` |

### EXE-430 ExecDecision.detail 打包往返,每个子码一行

- 规则:[`ExecDecision.detail` 打包与子码](README.md#execdecisiondetail-打包与子码)
- 级别:table
- 里程碑:M2
- 前置:`pack(table, code, arg)` 与 `unpack(detail)`(`src/decision.rs`)
- 实现:`crates/hft-exec/tests/detail.rs::exe_430_detail_roundtrip`

| 行 | table / code / arg | 期望 `detail` |
|---|---|---|
| EXE-430.1 | None / 0 / 0 | 0 |
| EXE-430.2 | TargetReject / NotOwner…LotMismatch(1–10) / 0 | `1 \| code << 8`(10 个值逐一) |
| EXE-430.3 | Stuck / Gated…MinOrder(1–8) / 0 | `2 \| code << 8`(8 个值) |
| EXE-430.4 | Broken / LinkBroken / RetryExhausted…ResumeTimeout(1–6) | `3 \| 1 << 8 \| arg << 16`(6 个值) |
| EXE-430.5 | Broken / ReverseFailed、FixExhausted、HedgeVeto / 0 | `3 \| 2 << 8`、`3 \| 3 << 8`、`3 \| 4 << 8` |
| EXE-430.6 | AutoFix / Start、Slice、Done / 0 | `4 \| 1 << 8`、`4 \| 2 << 8`、`4 \| 3 << 8` |
| EXE-430.7 | AutoFix / Failed / ChainLeg、MinOrder、Rejected、Residual、Deadline、SignFlip、RiskReject | `4 \| 4 << 8 \| arg << 16`,arg ∈ {1, 2, 4, 6, 7, 8, 9} |
| EXE-430.8 | Engine / Step、RiskReject、Ungrouped、HardAlign / 0 | `5 \| 1 << 8`、`5 \| 3 << 8`、`5 \| 4 << 8`、`5 \| 6 << 8` |
| EXE-430.9 | Engine / SerialFallback / StaleQuote、NotReady、ResumeHold、FirstUnknown | `5 \| 2 << 8 \| arg << 16`,arg ∈ {1, 4, 5, 6} |
| EXE-430.10 | Engine / CancelResting / 腿位图 0b1011 | `5 \| 5 << 8 \| 0b1011 << 16` |
| EXE-430.11 | External / Liquidation…Unknown(1–5) / Timed…Cleared(1–5) | `6 \| code << 8 \| arg << 16`(25 个组合) |
| EXE-430.12 | 每行的 `unpack(pack(…))` | 等于输入 |

### EXE-431 SDK 枚举的判别值与 detail 子码逐值相同

- 规则:[`ExecDecision.detail` 打包与子码](README.md#execdecisiondetail-打包与子码)
- 级别:table
- 里程碑:M2
- 前置:编译期断言 + 运行期遍历
- 实现:`crates/hft-exec/tests/detail.rs::exe_431_sdk_enum_codes`

| 行 | 枚举 | 期望 |
|---|---|---|
| EXE-431.1 | SDK `TargetReject` | 各变体 `as u8` = 表 1 的 code(1–10) |
| EXE-431.2 | SDK `StuckReason` | = 表 2(`Gated` = 1 … `MinOrder` = 8) |
| EXE-431.3 | SDK `BrokenReason` | = 表 3(1–4) |
| EXE-431.4 | `ExternalFill.cause` | = 表 6 的 code(1–5),与 hft-event 同值 |

### EXE-432 执行层订单的 OrderDecision.reason = ExecOrderKind 码

- 规则:[`ExecDecision.detail` 打包与子码](README.md#execdecisiondetail-打包与子码)
- 级别:table
- 里程碑:M2
- 前置:各类订单的 `ExecOrder.kind`(sequencer 原样写进 `OrderDecision.reason`)
- 实现:`crates/hft-exec/tests/detail.rs::exe_432_exec_order_kind`

| 行 | 订单 | 期望码 |
|---|---|---|
| EXE-432.1 | `TakerSlice` 切片 | 1(Slice) |
| EXE-432.2 | 配对第一腿 | 2(PairFirst) |
| EXE-432.3 | Simultaneous 第一腿 | 3(SimulFirst) |
| EXE-432.4 | 补腿 | 5(HedgeFix) |
| EXE-432.5 | 残余修剪 | 6(ResidualTrim) |
| EXE-432.6 | 外来成交默认处理 | 7(ExternalDefault) |
| EXE-432.7 | L3、清仓、无组 cell | 8(ForceStep) |
| EXE-432.8 | 命令补腿 | 9(CmdHedgeFix) |
| EXE-432.9 | 自动有界平仓 | 10(AutoFix) |
| EXE-432.10 | `MakerFirst` 码值 | 4(保留,core 不产生) |

### EXE-433 保留码从不出现

- 规则:[`ExecDecision.detail` 打包与子码](README.md#execdecisiondetail-打包与子码)、[Broken 处置](README.md#broken-处置)
- 级别:prop
- 里程碑:M2
- 性质:任意序列中,`AutoFixFail` 从不取 3、5;`SerialFallback` 原因从不取 2、3;`detail` 的 `table` ≤ 6
- 生成:同 EXE-050
- 参考:断言
- 例数:PR 2 000
- 实现:`crates/hft-exec/tests/props.rs::exe_433_reserved_codes_unused`

### EXE-434 Note 码的 const 表与 hft-event 登记一致

- 规则:[消费与产生的事件](README.md#消费与产生的事件)、[规划源文件](README.md#规划源文件)
- 级别:table
- 里程碑:M2
- 前置:`src/decision.rs` 导出的 Note `const` 表;hft-event 的 Note 登记表(`gen-schema --check` 读取)
- 实现:`crates/hft-exec/tests/detail.rs::exe_434_note_table`

| 行 | Note 码名 | 期望默认级别与触发用例 |
|---|---|---|
| EXE-434.1 | `ExecBroken` | P0;EXE-068 |
| EXE-434.2 | `ExecExternalDefault` | 按条件定级:发单 P0(EXE-294),不发单 P1(EXE-512) |
| EXE-434.3 | `ExecAutoFixFailed` | P0;EXE-245 |
| EXE-434.4 | `ExecSerialFallback` | P0;EXE-347 |
| EXE-434.5 | `ExecEngineOrderRejected` | P0;EXE-131 |
| EXE-434.6 | `ExecUngroupedReduce` | P0;EXE-354 |
| EXE-434.7 | `ExecGateHeldNaked` | P0;EXE-325、EXE-326 |
| EXE-434.8 | `ExecAutoFixDone` | P1;EXE-025 |
| EXE-434.9 | `ExecStuck` | P1;EXE-382 |
| EXE-434.10 | `ExecForceStep` | P1;EXE-360 |
| EXE-434.11 | `ExecManualExternal` | P1(按条件 Info);EXE-291 |
| EXE-434.12 | `ExecManualHold` | P1(解除为 Info);EXE-310、EXE-312 |
| EXE-434.13 | `ExecChainLegExcess` | P0;在 const 表中存在(触发用例属 optional 链上 feature) |
| EXE-434.14 | 全表 | 同一 source 内 `n` 唯一;名字集合与 hft-event 登记表中 `source = Exec` 的行相同 |

### EXE-440 定时器编码:timer_id = kind | account << 8,group 为 cell 的 group

- 规则:[定时器](README.md#定时器)
- 级别:table
- 里程碑:M2
- 前置:a1 = 1;g7 = 0x0100_0007
- 实现:`crates/hft-exec/tests/timers.rs::exe_440_timer_encoding`

| 行 | kind | 期望 `timer_id` / `on_timer` 收到的 u64 |
|---|---|---|
| EXE-440.1 | Wake = 1 | 0x0101 / 0x0100_0007_0000_0101 |
| EXE-440.2 | Fix = 2 | 0x0102 / 0x0100_0007_0000_0102 |
| EXE-440.3 | External = 3 | 0x0103 / 0x0100_0007_0000_0103 |
| EXE-440.4 | Continue = 4(全局唯一:account 位 0、`group = GROUP_NONE`) | 0x0004 / 0x0000_0000_0000_0004 |
| EXE-440.5 | AutoFix = 5 | 0x0105 / 0x0100_0007_0000_0105 |
| EXE-440.6 | Alarm = 6 | 0x0106 / 0x0100_0007_0000_0106 |
| EXE-440.7 | 无组 cell 的 Wake | `group = GROUP_NONE` |

### EXE-441 每 cell 同一 kind 至多一个定时器,改期时先撤后登

- 规则:[定时器](README.md#定时器)
- 级别:unit
- 里程碑:M2
- 前置:C0;g7 `ext_deadline = t 6000` 已登记 `Timer{Exec, External}`
- 步骤:
  1. `on_venue_gate_open` 使截止改为 t 15 000;`advance`
- 期望:
  - `ExecOut.timers` 依次为撤销(External, g7)、登记(External, g7, t 15 000);此后该 cell 只有一个 External 定时器
- 实现:`crates/hft-exec/tests/timers.rs::exe_441_one_per_kind`

### EXE-442 定时器从不使用 GROUP_ALL

- 规则:[定时器](README.md#定时器)
- 级别:prop
- 里程碑:M2
- 性质:任意序列中,`ExecOut.timers` 每条的 `group ≠ GROUP_ALL`;同一 (owner, timer_id, group) 在任一时刻至多一个有效登记
- 生成:同 EXE-050
- 参考:mock 定时器堆
- 例数:PR 2 000
- 实现:`crates/hft-exec/tests/props.rs::exe_442_timer_keys`

### EXE-443 早到的定时器只重新登记,不动作

- 规则:[定时器](README.md#定时器)
- 级别:table
- 里程碑:M2
- 前置:C0;g7 各项保存时刻如列;`on_timer` 在 t 1000 到达
- 实现:`crates/hft-exec/tests/timers.rs::exe_443_early_timer`

| 行 | kind / 保存时刻 | 期望 |
|---|---|---|
| EXE-443.1 | Fix / `fix.next_tsc = t 2000` | 重登 t 2000;无订单 |
| EXE-443.2 | External / `ext_deadline = t 6000` | 重登 t 6000;无 `ExternalDefault` |
| EXE-443.3 | AutoFix / `due_tsc = t 120 000` | 重登 t 120 000;无 `AutoFix` 决定 |

### EXE-444 已移除 cell 的定时器到期无动作

- 规则:[定时器](README.md#定时器)
- 级别:unit
- 里程碑:M2
- 前置:C0;g8 已由 `on_groups_changed` 移除;堆中残留其 `Timer{Exec, Wake}`
- 步骤:
  1. `on_timer(1 | 1 << 8 | g8 << 32)`
- 期望:
  - `ExecOut` 全空;`has_pending() = false`
- 实现:`crates/hft-exec/tests/timers.rs::exe_444_removed_cell_timer`

### EXE-450 节 21 各行结构的大小、对齐与关键偏移

- 规则:[快照与恢复](README.md#快照与恢复)
- 级别:table
- 里程碑:M2
- 前置:编译期 `const` 断言与 `offset_of!` 运行期复核
- 实现:`crates/hft-exec/tests/snapshot.rs::exe_450_row_layouts`

| 行 | 断言 | 期望 |
|---|---|---|
| EXE-450.1 | `size_of::<ExecRowHdr>()`;`offset_of!(len)` | 8;4 |
| EXE-450.2 | `size_of::<LegRef>()`;`inst`、`lot`、`min_qty`、`slot` 偏移 | 32;4、8、16、24 |
| EXE-450.3 | `size_of::<ActiveSliceRow>()`;`link_id`、`first`、`planned` 偏移 | 32;4、8、24 |
| EXE-450.4 | `size_of::<ExecCellRow>()` | 400 |
| EXE-450.5 | `ExecCellRow` 偏移:`state`、`group`、`owner`、`flags`、`deferred`、`cap_block`、`gated_bits`、`set_seq` | 1、4、8、12、13、14、20、32 |
| EXE-450.6 | `ExecCellRow` 偏移:`set_tsc`、`fix_first_tsc`、`ext_deadline`、`broken_since`、`tolerance` | 40、80、96、104、112 |
| EXE-450.7 | `ExecCellRow` 偏移:`filled`、`own_infl`、`active`、`auto_fix`、`legs` | 120、152、184、216、272 |
| EXE-450.8 | `size_of::<ExecAcctRow>()`;`force_kind`、`force_venues`、`force_frac` 偏移 | 24;3、8、16 |
| EXE-450.9 | `size_of::<ExecSpecRow>()`;`params` 偏移 | 136;8 |
| EXE-450.10 | `size_of::<ExecTargetRow>()`;`group`、`legs`、`n_legs`、`reason`、`urgency`、`band`、`exec`、`tag` 偏移 | 232;4、8、72、73、74、80、88、224 |
| EXE-450.11 | `size_of::<ExecGateRow>()`;`venue_open`、`gate_closed_tsc` 偏移 | 24;8、16 |
| EXE-450.12 | 每个行结构 `align_of` | ≤ 8,且无隐式填充(`IntoBytes` 派生通过) |

### EXE-451 节 21 的行顺序:Cell* → Acct* → Gate*,各按键升序,行头长度为各行体字节数

- 规则:[快照与恢复](README.md#快照与恢复)
- 级别:unit
- 里程碑:M2
- 前置:C0;a1:g7 Working、g8 Idle 无任何状态、无组 cell 有引擎目标;a2:g9 Broken;a1 `force_active`、a2 无账户状态;账户 a1、a2、s1
- 步骤:
  1. `snapshot`
- 期望:
  - 行序:Cell(a1, GROUP_NONE)、Cell(a1, g7)、Cell(a2, g9)、Acct(a1)、Gate(a1)、Gate(a2)、Gate(s1);无 g8 行、无 Acct(a2)
  - 每个 Cell 行 `ExecRowHdr{kind = 1, len = 632}`;Acct `{2, 24}`;Gate `{3, 24}`;全部整数小端
- 实现:`crates/hft-exec/tests/snapshot.rs::exe_451_row_order`

### EXE-452 "非空" cell 的判定:任一项非零即写一行

- 规则:[快照与恢复](README.md#快照与恢复)
- 级别:table
- 里程碑:M2
- 前置:C0;g8 除所列一项外全部为初始值
- 实现:`crates/hft-exec/tests/snapshot.rs::exe_452_nonempty_cell`

| 行 | 唯一非零项 | 期望 g8 Cell 行 |
|---|---|---|
| EXE-452.1 | 无(Idle、无目标) | 无 |
| EXE-452.2 | `has_target = 1`(到达后 Idle) | 有 |
| EXE-452.3 | `state = Stuck` | 有 |
| EXE-452.4 | `fix.count = 1` | 有 |
| EXE-452.5 | `ext_deadline ≠ 0` | 有 |
| EXE-452.6 | `auto_fix.phase = Failed` | 有 |
| EXE-452.7 | `serial = 1` | 有 |
| EXE-452.8 | `hold = MANUAL_HOLD_DEFERRED` | 有 |
| EXE-452.9 | `deferred = STATE` | 有 |

### EXE-453 ExecTargetRow 按逐字段显式行写出,填充与参数尾部写 0

- 规则:[快照与恢复](README.md#快照与恢复)
- 级别:table
- 里程碑:M2
- 前置:C0;g7 目标的内存表示在写出前把 `ExecSpec` 判别字节后的 7 B 与参数尾部填成 0xAA
- 实现:`crates/hft-exec/tests/snapshot.rs::exe_453_target_row_roundtrip`

| 行 | 算法 | 期望 |
|---|---|---|
| EXE-453.1 | `TakerSlice`(48 B 参数) | 行内 `exec.algo = 1`、`exec._pad` 全 0、`params[48..128]` 全 0、`LegTarget._pad` 全 0、`_pad0/_pad1/_pad2` 全 0;`restore` 后目标与原目标逐字段相等 |
| EXE-453.2 | `PairOnFill`(96 B) | `algo = 2`;`params[96..128]` 全 0;往返相等 |
| EXE-453.3 | `Simultaneous`(72 B) | `algo = 3`;`params[72..128]` 全 0;往返相等 |
| EXE-453.4 | `MakerFirst`(128 B,作为数据往返) | `algo = 4`;往返相等 |
| EXE-453.5 | 同一状态写两次 | 两次字节相同(`state_hash` 确定) |
| EXE-453.6 | `has_target = 0` | `ExecTargetRow` 232 B 全 0 |

### EXE-454 读回非法判别值与非零尾部报 RestoreError

- 规则:[快照与恢复](README.md#快照与恢复)
- 级别:table
- 里程碑:M2
- 前置:由合法快照逐行改一个字段后 `restore`
- 实现:`crates/hft-exec/tests/snapshot.rs::exe_454_restore_errors`

| 行 | 改动 | 期望 |
|---|---|---|
| EXE-454.1 | `ExecRowHdr.kind = 4` | `Err(RestoreError)` |
| EXE-454.2 | Cell 行 `len = 631` | `Err` |
| EXE-454.3 | `ExecCellRow.state = 0`;另一例 `= 5` | `Err` |
| EXE-454.4 | `stuck = 9` | `Err` |
| EXE-454.5 | `broken = 5` | `Err` |
| EXE-454.6 | `origin = 7`(`has_target = 1`) | `Err` |
| EXE-454.7 | `active.kind = 8` | `Err` |
| EXE-454.8 | `auto_fix.phase = 8` | `Err` |
| EXE-454.9 | `auto_fix.fail = 3`;另一例 `= 5` | `Err` |
| EXE-454.10 | `exec.algo = 0`(`has_target = 1`);另一例 `= 5` | `Err` |
| EXE-454.11 | `TakerSlice` 的 `params[48] = 1` | `Err` |
| EXE-454.12 | `has_target = 0` 而 `ExecTargetRow.tag = 1` | `Err` |
| EXE-454.13 | `LegRef.kind = 4` | `Err` |
| EXE-454.14 | `ExecAcctRow.force_kind = 3` | `Err` |
| EXE-454.15 | 未改动 | `Ok` |

### EXE-455 restore 只装载,不改任何 TSC 量

- 规则:[快照与恢复](README.md#快照与恢复)
- 级别:unit
- 里程碑:M2
- 前置:C0;含全部 TSC 字段非零的 cell(Working、Fix、External、Broken Running)
- 步骤:
  1. `snapshot` → X;`restore(X)`;`snapshot` → Y
- 期望:
  - X 与 Y 逐字节相同
- 实现:`crates/hft-exec/tests/snapshot.rs::exe_455_restore_keeps_tsc`

### EXE-456 recovery_end 按停机时间不计平移已登记的 TSC 字段

- 规则:[快照与恢复](README.md#快照与恢复)
- 级别:table
- 里程碑:M2
- 前置:`stop_tsc = S = 5 000 000 000 000`、`now = N = 1 000 000 000`;`hz_prev = 1 000 000 000 000`;`hz_cur` 如列(默认同 `hz_prev`);每行一个字段取所列值
- 实现:`crates/hft-exec/tests/snapshot.rs::exe_456_recovery_shift`

| 行 | 字段值 / `hz_cur` | 期望平移后 |
|---|---|---|
| EXE-456.1 | `wake_at = S + 300 000 000` | `N + 300 000 000` |
| EXE-456.2 | `ext_deadline = S − 200 000 000` | `N − 200 000 000` |
| EXE-456.3 | `broken_since = S − 2 000 000 000` | 1(饱和) |
| EXE-456.4 | `fix.first_tsc = 0` | 0 |
| EXE-456.5 | `due_tsc = S` | `N` |
| EXE-456.6 | `due_tsc = S + 1` | `N + 1` |
| EXE-456.7 | `wake_at = S + 300 000 000`,`hz_cur = 2 000 000 000 000` | `N + 600 000 000` |
| EXE-456.8 | `wake_at = S + 7`,`hz_prev = 2 000 000 000 000`、`hz_cur = 3 000 000 000 000` | `N + 10`(10.5 向零) |
| EXE-456.9 | `set_tsc = S − 300 000 000`,`hz_cur = 2 000 000 000 000` | `N − 600 000 000` |
| EXE-456.10 | `auto_fix.exec_ms = 4000` | 4000 |
| EXE-456.11 | Gate 行 `gate_closed_tsc = 77` | `N`(重设,不平移) |
| EXE-456.12 | 其余登记字段 `last_slice_tsc`、`gated_since`、`stuck_since`、`fix.next_tsc`、`auto_fix.started_tsc`、`auto_fix.run_mark_tsc` 各取 `S − 1 000 000` | 各为 `N − 1 000 000` |

### EXE-457 recovery_end 关闭闸门并为有在途链接的 cell 置 LINK_HOLD

- 规则:[快照与恢复](README.md#快照与恢复)
- 级别:unit
- 里程碑:M2
- 前置:C0;g7 `active.link_id = 9`;g8 `active.link_id = 0`(单腿切片在途)
- 步骤:
  1. `restore`;`recovery_end(N, S, hz, hz)`;`snapshot`
- 期望:
  - g7 `flags & 8 = 8`;g8 `flags & 8 = 0`;全部 Gate 行 `gate_open = 0`、`venue_open = 0`、`gate_closed_tsc = N`
- 实现:`crates/hft-exec/tests/snapshot.rs::exe_457_recovery_end_gate_linkhold`

### EXE-458 recovery_timers 按平移后的时刻为每 cell 每 kind 至多登记一条

- 规则:[快照与恢复](README.md#快照与恢复)、[定时器](README.md#定时器)
- 级别:unit
- 里程碑:M2
- 前置:C0;g7 有 `wake_at`、`fix.next_tsc`、`ext_deadline`、`due_tsc` 与告警时刻;g8 只有 `wake_at`;`recovery_end(N, S, hz, hz)` 已调用
- 步骤:
  1. 收集 `recovery_timers()`
- 期望:
  - g7 恰有 Wake、Fix、External、AutoFix、Alarm 各一条,时刻等于平移后的字段值(Alarm 按恢复后的告警时刻);g8 恰有 Wake 一条;无重复 (kind, cell)
- 实现:`crates/hft-exec/tests/snapshot.rs::exe_458_recovery_timers`

### EXE-459 因输出容量停下的待推进位随快照保存,恢复后继续

- 规则:[快照与恢复](README.md#快照与恢复)、[输出 `ExecOut`](README.md#输出-execout容量固定sequencer-预分配每次调用本-crate-之后排空)
- 级别:unit
- 里程碑:M2
- 前置:C0;同 EXE-022 第一批后停下,40 个 cell 待推进
- 步骤:
  1. `snapshot`;`restore`;`has_pending()`;按 `Continue` 推进完
- 期望:
  - 40 个 Cell 行 `flags & PENDING(1) = 1`;`restore` 后 `has_pending() = true`;续推写出的决定与未快照的实例逐条相同
- 实现:`crates/hft-exec/tests/snapshot.rs::exe_459_pending_roundtrip`

### EXE-460 从快照起步的验证回放沿用快照中的闸门与 venue 放行状态

- 规则:[快照与恢复](README.md#快照与恢复)
- 级别:unit
- 里程碑:M2
- 前置:C0;实盘实例在 t 0 `recovery_end`、t 1000 `on_venue_gate_open(a1, B)`、t 2000 写快照(`venue_open` 含 B,`gate_open = 0`);之后的输入序列 E 使多出腿在 B 的 g7 补腿
- 步骤:
  1. 回放实例:`restore`(不调 `recovery_end`)后喂 E
  2. 实盘实例喂 E
- 期望:
  - 两个实例的 `ExecDecision` 字节流相同;回放实例中 g7 的 `HedgeFix` 在同一输入处发出
- 实现:`crates/hft-exec/tests/snapshot.rs::exe_460_replay_uses_gate_rows`

### EXE-461 账户行与清仓锁、force_req 往返

- 规则:[快照与恢复](README.md#快照与恢复)
- 级别:table
- 里程碑:M2
- 前置:C0
- 实现:`crates/hft-exec/tests/snapshot.rs::exe_461_acct_row`

| 行 | 账户状态 | 期望 Acct 行与恢复后 |
|---|---|---|
| EXE-461.1 | `force_active`,`force_req = {venues {B, C}, frac 0.2}` | `force_active = 1`、`force_kind = 1`、`force_venues` 含 B 与 C、`force_frac = 0.2`;恢复后下一次 `advance` 设定同样的引擎目标 |
| EXE-461.2 | `flatten_lock`,`force_req = Flatten` | `flatten_lock = 1`、`force_kind = 2` |
| EXE-461.3 | 只有 `flatten_lock` | `flatten_lock = 1`、`force_kind = 0` |

### EXE-462 同一输入序列两次运行的 ExecDecision 字节流相同

- 规则:[快照与恢复](README.md#快照与恢复)、[`ExecDecision` 与进度通知](README.md#execdecision-与进度通知)
- 级别:prop
- 里程碑:M2
- 性质:同一输入序列在两个新建实例上运行,`ExecDecision`、订单、通知、定时器与 Note 的序列逐字节相同;任意位置插入"快照 → restore"后继续运行,结果仍相同
- 生成:同 EXE-050
- 参考:两次运行逐字节比较
- 例数:PR 1 000
- 实现:`crates/hft-exec/tests/props.rs::exe_462_deterministic`

### EXE-463 跨机恢复:各截止的剩余时长按纳秒一致

- 规则:[快照与恢复](README.md#快照与恢复)、[Broken 处置](README.md#broken-处置)
- 级别:table
- 里程碑:M2
- 前置:C0;旧主机 2.0 GHz 停机前:g7 `ext_deadline` 剩 3000 ms、Broken `due_tsc` 剩 60 000 ms(多出腿 venue 已放行)、`wake_at` 剩 100 ms;停机 10 min;新主机 3.0 GHz
- 实现:`crates/hft-exec/tests/snapshot.rs::exe_463_cross_host`

| 行 | 字段 | 期望 `recovery_end` 后剩余(按新主机 tick 换算成 ns) |
|---|---|---|
| EXE-463.1 | `ext_deadline` | 3 000 000 000 ns |
| EXE-463.2 | `due_tsc` | 60 000 000 000 ns |
| EXE-463.3 | `wake_at` | 100 000 000 ns |
| EXE-463.4 | 任一已平移字段 | ≠ 0 |

### EXE-464 replay verify:含 ExecDecision 的 journal 逐字节一致

- 规则:[快照与恢复](README.md#快照与恢复)、[`ExecDecision` 与进度通知](README.md#execdecision-与进度通知)
- 级别:replay
- 里程碑:M2
- 输入:`fixtures/replay/exec/`(双腿开仓、补腿、Broken 自动平仓、L3 串行、清仓锁、恢复期按 venue 放行各一段)
- 期望:`replayer verify` 重算的 `ExecDecision` 与 journal 中的逐字节相同,`state_hash` 一致
- 实现:`crates/hft-exec/tests/replay.rs::exe_464_replay_verify`
### EXE-475 new 之后的入口调用零分配

- 规则:[延迟与资源契约](README.md#延迟与资源契约)、[入口](README.md#入口全部在-sequencer-线程调用无分配)
- 级别:unit
- 里程碑:M2
- 前置:C0;计数全局分配器;64 个 cell
- 步骤:
  1. `Exec::new` 之后,依次调用除 `on_groups_changed`、`restore` 外的每个公开入口各 1000 次(随机合法参数),含 `advance`、`snapshot`(写入预分配缓冲)、`recovery_end`、`recovery_timers`
- 期望:
  - 步骤 1 期间本线程分配次数 = 0(hft-testkit `CountingAlloc`;无分配即无释放)
- 实现:`crates/hft-exec/tests/alloc.rs::exe_475_zero_alloc`

### EXE-476 依赖闭包只含允许的 crate

- 规则:[依赖](README.md#依赖)
- 级别:table
- 里程碑:M2
- 前置:`cargo metadata --format-version 1` 中 `hft-exec` 的依赖(含 dev 依赖)
- 实现:`crates/hft-exec/tests/deps.rs::exe_476_dependency_closure`

| 行 | 检查 | 期望 |
|---|---|---|
| EXE-476.1 | 直接依赖 | ⊆ {`hft-types`, `hft-refdata`, `hft-event`, `hft-strategy-sdk`}(dev 依赖另可含 `hft-testkit`、`proptest`、`criterion`) |
| EXE-476.2 | 闭包含 `hft-oms`、`hft-ledger`、`hft-risk`、`hft-sequencer`、`hft-journal`、`hft-rt`、`hft-strat-lib`、`hft-venue`、`venue-*`、`hft-strategy-registry` 或任何 `strategies/*` | 否 |
| EXE-476.3 | 闭包含 `tokio` | 否 |

### EXE-477 热路径规则集拦住 f64、HashMap、std::time 与 dyn

- 规则:[依赖](README.md#依赖)、[延迟与资源契约](README.md#延迟与资源契约)
- 级别:table
- 里程碑:M2
- 前置:`crates/hft-exec/clippy.toml`(= `tools/lint/clippy-hot.toml`)与 `src/lib.rs` 的 lint 头;clippy 的 `disallowed-types` 写不了基本类型与 trait object,f64 与 `dyn` 由 `cargo xtask hot-check` 的源码检查(N3/N6)拦,本用例检查配置与源码本身(不在测试里嵌套运行 clippy)
- 实现:`crates/hft-exec/tests/lint.rs::exe_477_disallowed`

| 行 | 检查 | 期望 |
|---|---|---|
| EXE-477.1 | `lib.rs` 含 `#![deny(clippy::float_arithmetic)]`;`src/` 去注释后无 `f32`/`f64` 记号 | 成立 |
| EXE-477.2 | `clippy.toml` 与 `tools/lint/clippy-hot.toml` 逐字节相同,且禁 `std::collections::HashMap` | 成立 |
| EXE-477.3 | `clippy.toml` 禁 `std::time::Instant::now` | 成立 |
| EXE-477.4 | `src/` 去注释后无 `dyn` 记号 | 成立 |
| EXE-477.5 | `lib.rs` 含 `#![forbid(unsafe_code)]`;`src/` 无 `HashMap`/`HashSet`/`Instant`/`SystemTime`(正式源码在 CI 的 `clippy -D warnings` 下零告警) | 成立 |

### EXE-480 基准:touch_inst,该 inst 无活动 cell

- 规则:[延迟与资源契约](README.md#延迟与资源契约)
- 级别:bench
- 里程碑:M2
- 基准名:`exec_step/idle`
- 实现:`crates/hft-exec/benches/exec.rs::exe_480_exec_step_idle`

### EXE-481 基准:advance 推进一个活动 cell、无切片

- 规则:[延迟与资源契约](README.md#延迟与资源契约)
- 级别:bench
- 里程碑:M2
- 基准名:`exec_step/hold`
- 实现:`crates/hft-exec/benches/exec.rs::exe_481_exec_step_hold`

### EXE-482 基准:advance 产生一笔配对第一腿(2 腿、各 10 档)

- 规则:[延迟与资源契约](README.md#延迟与资源契约)
- 级别:bench
- 里程碑:M2
- 基准名:`exec_step/slice_pair`
- 实现:`crates/hft-exec/benches/exec.rs::exe_482_exec_step_slice_pair`

### EXE-483 基准:advance 推进 Broken cell 并产生一片自动有界平仓

- 规则:[延迟与资源契约](README.md#延迟与资源契约)
- 级别:bench
- 里程碑:M2
- 基准名:`exec_step/auto_fix`
- 实现:`crates/hft-exec/benches/exec.rs::exe_483_exec_step_auto_fix`

### EXE-484 基准:set_target(校验 + 登记,含更接近 0 与 pair_lot)

- 规则:[延迟与资源契约](README.md#延迟与资源契约)
- 级别:bench
- 里程碑:M2
- 基准名:`exec/set_target`
- 实现:`crates/hft-exec/benches/exec.rs::exe_484_set_target`

### EXE-485 基准:on_order_update / on_link

- 规则:[延迟与资源契约](README.md#延迟与资源契约)
- 级别:bench
- 里程碑:M2
- 基准名:`exec/on_link`
- 实现:`crates/hft-exec/benches/exec.rs::exe_485_on_link`

### EXE-486 基准:on_external(含 manual_hold 置位与 Note)

- 规则:[延迟与资源契约](README.md#延迟与资源契约)
- 级别:bench
- 里程碑:M2
- 基准名:`exec/on_external`
- 实现:`crates/hft-exec/benches/exec.rs::exe_486_on_external`

### EXE-487 基准:on_risk / on_gate_open / on_venue_gate_open,一个账户 64 个 cell

- 规则:[延迟与资源契约](README.md#延迟与资源契约)
- 级别:bench
- 里程碑:M2
- 基准名:`exec/on_risk_64`
- 实现:`crates/hft-exec/benches/exec.rs::exe_487_on_risk_64`

### EXE-488 基准:resolve_broken

- 规则:[延迟与资源契约](README.md#延迟与资源契约)
- 级别:bench
- 里程碑:M2
- 基准名:`exec/resolve_broken`
- 实现:`crates/hft-exec/benches/exec.rs::exe_488_resolve_broken`

### EXE-489 基准:drain_ready,全部账户共 64 个 cell

- 规则:[延迟与资源契约](README.md#延迟与资源契约)
- 级别:bench
- 里程碑:M2
- 基准名:`exec/drain_ready_64`
- 实现:`crates/hft-exec/benches/exec.rs::exe_489_drain_ready_64`

### EXE-490 基准:L3 一步的目标设定,一个账户 8 个 cell(含 Broken)

- 规则:[延迟与资源契约](README.md#延迟与资源契约)
- 级别:bench
- 里程碑:M2
- 基准名:`exec/force_step_8`
- 实现:`crates/hft-exec/benches/exec.rs::exe_490_force_step_8`

### EXE-491 基准:无组永续持仓扫描,512 个永续 inst

- 规则:[延迟与资源契约](README.md#延迟与资源契约)
- 级别:bench
- 里程碑:M2
- 基准名:`exec/force_ungrouped_512`
- 实现:`crates/hft-exec/benches/exec.rs::exe_491_force_ungrouped_512`

### EXE-492 基准:限价与单笔名义

- 规则:[延迟与资源契约](README.md#延迟与资源契约)、[限价与单笔名义](README.md#限价与单笔名义)
- 级别:bench
- 里程碑:M2
- 基准名:`exec/price_cap`
- 实现:`crates/hft-exec/benches/exec.rs::exe_492_price_cap`

### EXE-493 基准:recovery_end,1 024 个 cell

- 规则:[延迟与资源契约](README.md#延迟与资源契约)、[快照与恢复](README.md#快照与恢复)
- 级别:bench
- 里程碑:M2
- 基准名:`exec/recovery_end_1024`
- 实现:`crates/hft-exec/benches/exec.rs::exe_493_recovery_end_1024`

### EXE-494 基准:set_target 中的 pair_lot 重算

- 规则:[延迟与资源契约](README.md#延迟与资源契约)、[两腿 lot 对齐](README.md#两腿-lot-对齐)
- 级别:bench
- 里程碑:M2
- 基准名:`exec/pair_lot`
- 实现:`crates/hft-exec/benches/exec.rs::exe_494_pair_lot`

### EXE-500 双腿开仓在部分成交与深度变化下到达带内,第二腿与第一腿成交同一事件发出

- 规则:[PairOnFill](README.md#paironfill)、[两腿 lot 对齐](README.md#两腿-lot-对齐)
- 级别:dst
- 里程碑:M2
- 前置:hft-testkit + hft-sim 共用撮合器;测试策略对 g7 设开仓目标 S +10.00 / P −10.0(T0)
- 故障:第一腿 IOC 随机部分成交(0–100%);深度每 50–500 ms 随机变化;第二腿回报乱序与重复;连接活性每 1 s 一次 pong
- 种子:PR 200(提交派生)+ 回归集;每夜日期派生
- 断言:最终两腿在带内且 `Done` 恰好一次;每个第一腿成交增量对应的第二腿 `OrderDecision` 与该成交在同一 Input 内;第一腿订单量全为 `pair_lot` 的整数倍;INV-3、INV-8、INV-19
- 实现:`crates/hft-testkit/src/scenarios/exec.rs::exe_500_pair_open_partial_fills`

### EXE-501 由多翻空的目标在任意成交拆分下无一笔穿越零

- 规则:[推进](README.md#推进advance-中每个-cellon_legs--每腿--depth_levels-档)
- 级别:dst
- 里程碑:M2
- 前置:测试策略对 g8 设目标 P +5.0 → −5.0
- 故障:IOC 随机部分成交、零成交、`Unknown` 后迟到成交
- 种子:PR 200 + 回归集;每夜日期派生
- 断言:模拟交易所真值中 P 净量从未在单笔订单内由正跨到负;最终到达 −5.0
- 实现:`crates/hft-testkit/src/scenarios/exec.rs::exe_501_flip_no_cross`

### EXE-502 Simultaneous:第二腿失败按 on_fail 处理,反向失败进入 Broken

- 规则:[Simultaneous](README.md#simultaneous)
- 级别:dst
- 里程碑:M2
- 前置:g7 `Simultaneous` 开仓目标
- 故障:分支 A:第二腿被交易所拒(`on_fail = Reverse`);分支 B:同样故障,`on_fail = Hold`;分支 C:反向单也被拒
- 种子:PR 100 + 回归集;每夜日期派生
- 断言:A 对已成交未对冲部分出反向 IOC、无 Broken;B 无反向单,LegTracker 记裸量、补腿按阈值处理;C cell `Broken{ReverseFailed}`、`Note{ExecBroken}` P0
- 实现:`crates/hft-testkit/src/scenarios/exec.rs::exe_502_simultaneous_on_fail`

### EXE-503 L3 期间价差扩大:限价按 cap_cross_px,风控不以价格带或名义拒绝

- 规则:[限价与单笔名义](README.md#限价与单笔名义)、[保证金危急的默认减仓](README.md#保证金危急的默认减仓a5与-engine_flatten)
- 级别:dst
- 里程碑:M2
- 前置:a1 L3,g8 P −10.0;`force_slice.max_cross = 0.005`、`rr_max_cross = 0.01`、`l3_max_cross = 0`
- 故障:P 对手价相对 mid 的偏离在 0.3%–2% 之间随机游走
- 种子:PR 100 + 回归集;每夜日期派生
- 断言:偏离 ≤ 1% 时每步照常成交;偏离 > 1% 时限价 = mid × 1.01,IOC 吃不到计零成交;全程无 `PX_BAND`/`NOTIONAL_*` 拒绝、无 `ExecEngineOrderRejected`;INV-15、INV-17
- 实现:`crates/hft-testkit/src/scenarios/exec.rs::exe_503_spread_widening_l3`

### EXE-504 目标执行中模式降为 CloseOnly:减仓继续、增仓 Stuck,恢复 Live 后继续

- 规则:[运行模式、Drain 与 Unknown](README.md#运行模式drain-与-unknown)、[门控拒绝与退避](README.md#门控拒绝与退避)
- 级别:dst
- 里程碑:M2
- 前置:g7 开仓目标进行中、g8 减仓目标进行中
- 故障:t 1 s 起有效模式 CloseOnly,t 20 s 回到 Live
- 种子:PR 100 + 回归集;每夜日期派生
- 断言:CloseOnly 期间 g7 无订单、`Stuck{Gated(CLOSE_ONLY)}`,g8 照常;Live 后 g7 继续;每个 cell 每 `gated_backoff_ms` 至多一条被拒意图
- 实现:`crates/hft-testkit/src/scenarios/exec.rs::exe_504_close_only_midway`

### EXE-505 执行层场景的通用断言:裸量超阈值时长有界,Broken 有界处置

- 规则:[补腿](README.md#补腿hedgefixc4)、[Broken 处置](README.md#broken-处置)
- 级别:dst
- 里程碑:M2
- 前置:全部执行层 DST 场景(EXE-500–EXE-504 与 scenarios.md 中发起方为执行层的场景)共用的检查器
- 故障:丢包、重复、乱序、断连、限速拒绝、部分成交、费用晚到、进程崩溃与停机
- 种子:随所属场景
- 断言:ledger 与模拟交易所一致;无槽泄漏;裸量超补腿阈值的运行时长 ≤ 对冲时延 + `fix_deadline_ms`(恢复期、`manual_hold`、`SerialFallback` 串行腿与 `HardAlign` 对齐目标期间不计);Broken 的裸量在无人工时于 `broken_auto_fix_ms` 加 `fix_deadline_ms` 的可执行时长内被处置或有 P0;INV-3、INV-7、INV-16、INV-20
- 实现:`crates/hft-testkit/src/scenarios/exec.rs::exe_505_common_checker`

### EXE-506 engine_reduce_pending 只看未到达的 MarginStress/Hard 引擎目标,口径与 drain_ready_acct 中该项相同

- 规则:[入口(全部在 sequencer 线程调用,无分配)](README.md#入口全部在-sequencer-线程调用无分配)、[运行模式、Drain 与 Unknown](README.md#运行模式drain-与-unknown)
- 级别:table
- 里程碑:M2
- 前置:C0、R0;a1 的 g7、g8 与 s1 的单腿 group g9(腿 0 = P)均无目标、无在途、无计时;每行只设所列状态,调用 `engine_reduce_pending()` 与 `drain_ready()`
- 实现:`crates/hft-exec/tests/drain.rs::exe_506_engine_reduce_pending`

| 行 | 状态 | 期望 `engine_reduce_pending()` / `drain_ready()` |
|---|---|---|
| EXE-506.1 | 全部空闲 | false / true |
| EXE-506.2 | g8 `reason = MarginStress` 引擎目标 P 0,P 净量 −1.0 未到达 | true / false |
| EXE-506.3 | a1 `engine_flatten` 后 g7 清仓目标(`reason = Hard`)未到达 | true / false |
| EXE-506.4 | a1 无组 cell(`GROUP_NONE`,P −0.5)`reason = MarginStress` 引擎目标未到达 | true / false |
| EXE-506.5 | g7 `HardAlign` 对齐目标未到达 | true / false |
| EXE-506.6 | s1(Sim)的 g9 `reason = Hard` 引擎目标未到达,其余空闲 | true / false |
| EXE-506.7 | g8 策略目标(`reason = Rebalance`)未到达、有在途切片 | false / false |
| EXE-506.8 | g7 外来成交计时 `ext_deadline ≠ 0` | false / false |
| EXE-506.9 | g8 `reason = MarginStress` 引擎目标已到达(仍在) | false / true |
| EXE-506.10 | g7 Broken、自动有界平仓 `Primed` 未到期 | false / true |
| EXE-506.11 | 每行调用前后 `ExecOut` | 两个函数都不写 `ExecOut`(调用后为空) |

### EXE-507 只减仓命令期间设的目标会增加任一腿 |仓位| 时整条以 ReduceOnlyCmd 拒绝

- 规则:[目标的接受与替换](README.md#目标的接受与替换)、[`ExecDecision.detail` 打包与子码](README.md#execdecisiondetail-打包与子码)
- 级别:table
- 里程碑:M2
- 前置:C0、R0、T0;g7 当前目标 S +2.00 / P −2.0(`reason = Rebalance`)已到达,净量 S +2.00、P −2.0;i1 调 `set_target(i1, t, ro, v, now, out)`,t 的腿与 `ro` 如列
- 实现:`crates/hft-exec/tests/accept.rs::exe_507_reduce_only_cmd_target`

| 行 | `ro` | 新目标(S / P) | 期望 |
|---|---|---|---|
| EXE-507.1 | true | +1.00 / −1.0 | `Ok`;当前目标替换为新目标 |
| EXE-507.2 | true | 0 / 0 | `Ok` |
| EXE-507.3 | true | +2.00 / −2.0(各腿 |目标| = |仓位|) | `Ok`(逐字段与当前目标相同,不登记 Wake) |
| EXE-507.4 | true | +2.01 / −2.0 | `Err(ReduceOnlyCmd)`;`ExecDecision{TargetRejected, detail = 1 \| 11 << 8}`;当前目标仍为 +2.00 / −2.0;无订单 |
| EXE-507.5 | true | +1.00 / +1.0(P 穿越零) | `Err(ReduceOnlyCmd)`;当前目标不变 |
| EXE-507.6 | true | +1.00 / −2.1(只有 P 增加) | `Err(ReduceOnlyCmd)`;不部分接受 S 的减少 |
| EXE-507.7 | true;g8 无仓位(P 0) | g8 P −0.1 | `Err(ReduceOnlyCmd)`(`pos = 0` 而目标非 0) |
| EXE-507.8 | false | +2.01 / −2.0 | `Ok`(不受只减仓约束) |
| EXE-507.9 | true;g7 Broken | +1.00 / −1.0 | `Err(CellBroken)`(按校验顺序 `CellBroken` 先于 `ReduceOnlyCmd`) |
| EXE-507.10 | true | `clear_target(i1, a1, g7)` | 目标清除,`ExecDecision{TargetCleared}`(`clear_target` 不受 `ro` 限制) |

### EXE-508 目标为 0 的腿完成与出片都以尘埃线为界

- 规则:[不交易带与紧急度](README.md#不交易带与紧急度)、[推进(`advance` 中每个 cell,O(n_legs) + 每腿 ≤ `depth_levels` 档)](README.md#推进advance-中每个-cellon_legs--每腿--depth_levels-档)
- 级别:table
- 里程碑:M2
- 前置:C0(`dust_notional = 5 USDT`);g8 目标 P 0、`urgency = Normal`、`band = 0.5`;P mark 如列;P 净量 −0.1;无在途;推进一次
- 实现:`crates/hft-exec/tests/band.rs::exe_508_zero_target_dust_boundary`

| 行 | P mark | 腿名义 | 期望 |
|---|---|---|---|
| EXE-508.1 | 49.99 | 4.999 USDT(尘埃线 − 0.001) | 无订单;`ExecDecision{State → Idle}`(完成) |
| EXE-508.2 | 50.00 | 5.000 USDT(= 尘埃线) | 无订单;`State → Idle` |
| EXE-508.3 | 50.01 | 5.001 USDT(尘埃线 + 0.001) | 出一片买 P 0.1;不写 `State → Idle` |

### EXE-509 recovery_end 不重放尾部时 stop_tsc 取所选快照记录的记录头 tsc

- 规则:[快照与恢复](README.md#快照与恢复)
- 级别:unit
- 前置:C0;快照记录头 `tsc = 4 000 000 000 000`,快照头测量字段 `snap_tsc = 3 999 999 000 000`;g7 `ext_deadline` = 记录头 tsc + 2000 ms;尾部为空;`now = N = 1 000 000 000`,`hz_prev = hz_cur`
- 里程碑:M2
- 步骤:
  1. `restore` 该快照;`recovery_end(now = N, stop_tsc = 4 000 000 000 000, hz, hz)`
- 期望:
  - g7 `ext_deadline = N + 2 000 000 000`(剩余 2000 ms;若按 `snap_tsc` 平移会得到 N + 2 001 000 000,不得出现)
- 实现:`crates/hft-exec/tests/restore.rs::exe_509_stop_tsc_from_record_header`

### EXE-510 目标为 0 的腿在任何紧急度下都不用带,只看尘埃线

- 规则:[不交易带与紧急度](README.md#不交易带与紧急度)
- 级别:table
- 里程碑:M2
- 前置:C0(`dust_notional = 5 USDT`、`band_floor_notional = 20 USDT`);g8 目标 P 0、`band = 0.5`(带宽 0.5 × 0.2 = 0.1);P 净量 −0.1(名义 10 USDT);无在途
- 实现:`crates/hft-exec/tests/band.rs::exe_510_urgent_zero_target_ignores_band`

| 行 | `urgency` | 期望 |
|---|---|---|
| EXE-510.1 | Urgent | 出一片买 P 0.1 |
| EXE-510.2 | Normal | 出一片买 P 0.1(名义 10 USDT > `dust_notional`,不按带宽 0.1 判为在带内) |
| EXE-510.3 | Passive | 出一片买 P 0.1 |

### EXE-511 第一腿量受第二腿名义上界 ≤ max_hedge_notional 约束,上界两个方向都按 mid × (1 + rr_max_cross)

- 规则:[PairOnFill](README.md#paironfill)、[对冲量上限](README.md#对冲量上限b6)
- 级别:table
- 里程碑:M2
- 前置:C0、T0;R0 但 `max_hedge_notional = 500 USDT`;mid 100.00、`rr_max_cross = 0.01`
- 实现:`crates/hft-exec/tests/pair.rs::exe_511_second_leg_worst_notional`

| 行 | 先手 | 第二腿估算价 | 期望第一腿量 |
|---|---|---|---|
| EXE-511.1 | S 买(第二腿 P 卖) | 101.00(卖出第二腿同样按 mid × 1.01,不按 99.00) | 4.9(q × 101.00 ≤ 500 → q ≤ 4.9504,取 `pair_lot` 0.1 倍数) |
| EXE-511.2 | P 卖(`first_leg = PerpFirst`,第二腿 S 买) | 101.00 | 4.9(q ≤ 4.9504) |
| EXE-511.3 | S 买,`max_hedge_notional = 2000` | 101.00 | 9.9(不受此约束) |

### EXE-512 截止到达而裸量在阈值内:不发单,仍写决定与 P1

- 规则:[外来成交的默认处理](README.md#外来成交的默认处理b3)
- 级别:unit
- 里程碑:M2
- 前置:C0;g7 无目标,Adl 使两腿同比例缩小:净量 S +1.00、P −1.0;`ext_deadline = t 6000`
- 步骤:
  1. t 6000 推进
- 期望:
  - `ExecDecision{ExternalDefault, qty = 0}` 与 `Note{ExecExternalDefault}` P1 各一条;无订单;`ext_deadline = 0`
- 实现:`crates/hft-exec/tests/external.rs::exe_512_deadline_within_threshold`

### EXE-513 人工保持与其他外来成交交错时不自动修剪尾量

- 规则:[目标单元](README.md#目标单元targetcell)
- 级别:unit
- 里程碑:M2
- 前置:EXE-050 的属性测试世界,已有现货与永续仓位
- 步骤:按固定操作 `(0,6402)、(7,15018)、(7,56772)、(7,35176)` 设置目标并交错投递外来成交;每步推进执行层并检查 EXE-050 性质
- 期望:人工保持未解除时不发配对尾量的 ResidualTrim,其他外来成交改变展示状态后保持仍有效
- 实现:`crates/hft-exec/tests/props.rs::exe_513_manual_hold_survives_other_external_events`

### EXE-514 ECF1 执行配置完整往返

- 规则:[配置项](README.md#配置项execconfigengineconfig-blob-的-exec-段每账户一行--全局默认行变更--drain--重启)
- 级别:unit
- 里程碑:M2
- 前置:全部 ExecParams 字段非默认值;一个 Live 账户使用默认行,一个 Sim 账户有覆盖
- 步骤:to_rows/from_rows 往返;颠倒账户输入顺序再编码
- 期望:所有字段、None 与 Sim 身份保持;账户输入顺序不改变规范行
- 实现:`crates/hft-exec/tests/config_blob.rs::exe_514_config_section_roundtrip`

### EXE-515 执行配置拒绝缺默认、重复账户和非法标志

- 规则:[配置项](README.md#配置项execconfigengineconfig-blob-的-exec-段每账户一行--全局默认行变更--drain--重启)
- 级别:unit
- 里程碑:M2
- 前置:完整默认行与一个无覆盖账户行
- 步骤:分别删默认行、重复账户、破坏 bool/填充、给无覆盖行写参数、写越界 u8
- 期望:全部返回 None
- 实现:`crates/hft-exec/tests/config_blob.rs::exe_515_config_section_rejects_malformed`

## 验收

`testkit` 强制目标原始事实与独立计算的跨组件验收归 [TK-109](../hft-testkit/TESTS.md#tk-109-真实保证金强制目标独立重算)。

`testkit` 强制目标、计时触发与执行单元的只读观察验收归 [TK-107](../hft-testkit/TESTS.md#tk-107-真实引擎逐事件观察与反例);诊断不进入持久状态。

- **完成标准**:本文件中里程碑 M2 的用例全部有实现且通过([docs/tests §1](../../docs/tests/README.md#1-地位)),`cargo xtask test-trace` 对本组件无未实现、多余与不一致项。
- **属性测试**:各 `prop` 用例的 PR 例数见用例;每夜与服务器累计运行中,EXE-050、EXE-024、EXE-261、EXE-368、EXE-417、EXE-418、EXE-462 合计 1 000 000 例无失败。
- **DST**:EXE-500–EXE-505 与 `docs/tests/scenarios.md` 中执行层参与的场景按 [testing §8 上线门槛](../../docs/testing.md#8-上线门槛) ④ 全过;EXE-505 的检查器在其中每个场景的每一步运行。
- **基准**:EXE-480–EXE-494 的数值上限只在 [benchmarks](../../benchmarks/README.md) 登记,参考机判定通过。
- **模拟账户**:Sim 账户连续运行期间(门槛 ⑦)报告中每个 `Stuck`/`Broken` 都有解释,每次 `ExecAutoFixDone`/`ExecAutoFixFailed` 都与 ledger 的成交对得上;判定与证据归 [testing §8](../../docs/testing.md#8-上线门槛)。
- **里程碑外**(optional 链上 feature 启用时按本文件格式补写,不在 M2 范围):多出的一腿是链上腿时补腿、残余修剪、外来成交默认处理不出单、写 `Note{ExecChainLegExcess}`(P0,`v[]` 按"补腿"一节)且同一裸量不重复写;自动有界平仓以 `ChainLeg`(1)失败;`hedge_fix_cmd` 返回 `ExecCmdErr::ChainLeg`;L3 对链上腿不设目标并写 `ExecChainLegExcess`;目标引用链上腿 → `SpecInvalid`。`MakerFirst` 的挂单、改单、撤单与升级到 `Urgent` 时改用 PairOnFill 的用例在该 feature 启用时补写(未启用时的拒绝见 EXE-196)。
- **README 原 DST 清单的去向**(执行层与其他组件共同参与的完整场景归 `docs/tests/scenarios.md`,本文件只写执行层的断言):

| 原条目 | 去向 |
|---|---|
| 1 双腿开仓、部分成交 | EXE-500 |
| 2 第二腿被拒 → SecondRetry;耗尽 → Broken → 自动平仓 | B2、BK-1;执行层侧 EXE-237–EXE-262 |
| 3 基础资产扣费、FeeOnly 晚到 | B4、FP-12;执行层侧 EXE-281 |
| 4 自身强平/ADL、Manual、L3 期间 ADL | B3、MD-18;执行层侧 EXE-290–EXE-300、EXE-310 |
| 5 L3 按步减仓、无组永续持仓 | A5、MS-4;执行层侧 EXE-340–EXE-360 |
| 6 解码单位错误 → HEDGE_LIMIT | B6;执行层侧 EXE-070、EXE-240 |
| 7 残余超阈值 → 自动补腿 | B-4、C4;执行层侧 EXE-205–EXE-216 |
| 8 模式降为 CloseOnly | EXE-504 |
| 9 Shadow 只一条切片 | EXE-391 |
| 10 第一腿/对冲腿 Unknown、迟到成交 | C1、FP-1;执行层侧 EXE-398 |
| 11 由多翻空不穿越零 | EXE-501、EXE-090 |
| 12 腿顺序 | C14;执行层侧 EXE-156–EXE-159 |
| 13 Simultaneous 失败与反向 | EXE-502 |
| 14、37 Drain 与 Drain 中清仓 | C11、MD-14;执行层侧 EXE-396、EXE-397 |
| 15、28 崩溃恢复与跨机恢复 | RC-11、BK-5;执行层侧 EXE-248、EXE-456、EXE-463 |
| 16、35 HedgeFix 命令、HARD 解除的 Broken 判定 | BK-7;执行层侧 EXE-220–EXE-232 |
| 17 自动有界平仓的各支 | BK-1、BK-2;执行层侧 EXE-237–EXE-262 |
| 18、33 L3/清仓覆盖 Broken | BK-3;执行层侧 EXE-025、EXE-351、EXE-362 |
| 19 无 client id 的第一腿成交 | LK-4 |
| 20 LINK_FUNDS 与 LINK_STALE | LK-1、LK-2;执行层侧 EXE-166、EXE-167 |
| 21 链上腿多出 | 里程碑外(见上) |
| 22 第二腿盘口 10 s 不变(必过) | QF-1;执行层侧 EXE-116 |
| 23 清仓锁 | FL-1;执行层侧 EXE-364–EXE-368 |
| 24 非 HARD 的 Kill | MD-11;执行层侧 EXE-394 |
| 25 两腿 lot 不同 | LT-1;执行层侧 EXE-180–EXE-188 |
| 26 L3 + 另一所 BBO 断 30 s(必过) | MS-3;执行层侧 EXE-347–EXE-349 |
| 27 价差扩大 | EXE-503 |
| 29 L3 时策略挂满 reduce-only 单 | MS-5;执行层侧 EXE-345、EXE-346 |
| 30 L3 中途重启 + 现货所断线 | MS-6;执行层侧 EXE-347 |
| 31、36 补腿进行中崩溃、另一所起不来 | BK-5;执行层侧 EXE-297、EXE-315、EXE-325 |
| 32 核销残余 | BK-6;执行层侧 EXE-283 |
| 34 SimReset | MD-20;执行层侧 EXE-405 |
