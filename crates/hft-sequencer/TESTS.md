---
component: hft-sequencer
prefix: SEQ
spec: README.md
---

# hft-sequencer 测试规格

格式与规则见 [docs/tests](../../docs/tests/README.md)。

## 覆盖

| README 小节 | 用例 |
|---|---|
| 概要 | SEQ-018、SEQ-310 |
| 目录 | 不适用:非规范 |
| 使用本组件 | 不适用:见子节 |
| 装配与运行(由 apps/hft-core、apps/replayer、hft-testkit 调用) | SEQ-006、SEQ-011、SEQ-013、SEQ-014、SEQ-075、SEQ-107、SEQ-347、SEQ-376–SEQ-381、SEQ-384–SEQ-395、SEQ-404–SEQ-406、SEQ-422–SEQ-424 |
| 策略集(`StrategySet`) | SEQ-015–SEQ-018、SEQ-340 |
| 只读视图(`SeqViews`,实现 SDK `Views`) | SEQ-019–SEQ-029、SEQ-181 |
| 环与容量 | SEQ-001–SEQ-007、SEQ-058、SEQ-059、SEQ-064、SEQ-066、SEQ-086、SEQ-087、SEQ-100、SEQ-221、SEQ-231、SEQ-279、SEQ-288、SEQ-345、SEQ-346、SEQ-367 |
| advisor | SEQ-030、SEQ-031、SEQ-051、SEQ-339 |
| 实现说明 | 不适用:见子节 |
| 记录分类 | SEQ-020、SEQ-052–SEQ-054 |
| 主循环(`step`) | SEQ-008–SEQ-010、SEQ-012、SEQ-055–SEQ-077、SEQ-101、SEQ-217、SEQ-319、SEQ-343、SEQ-344、SEQ-346、SEQ-370 |
| 心跳 | SEQ-010、SEQ-073、SEQ-078 |
| 环境决定写成 Input | SEQ-036、SEQ-055、SEQ-079–SEQ-098、SEQ-244 |
| BBO 合并与 CONFLATED | SEQ-095、SEQ-099–SEQ-103 |
| 引擎时间(engine_ns) | SEQ-008、SEQ-042、SEQ-104–SEQ-112 |
| 连接活性 | SEQ-022、SEQ-028、SEQ-113–SEQ-120、SEQ-143、SEQ-173 |
| 每条 Input 的处理顺序(`process`) | SEQ-373–SEQ-375、SEQ-382–SEQ-383、SEQ-411–SEQ-415、SEQ-074、SEQ-080、SEQ-104、SEQ-121–SEQ-169、SEQ-184、SEQ-216、SEQ-341、SEQ-344、SEQ-359、SEQ-360、SEQ-364、SEQ-369、SEQ-370、SEQ-371、SEQ-372、TK-070、TK-106、TK-115、SEQ-416–SEQ-420、SEQ-425–SEQ-431 |
| 意图管线 | SEQ-127、SEQ-129、SEQ-170–SEQ-181、SEQ-300、SEQ-352、SEQ-360 |
| 目标意图与执行层 | SEQ-135、SEQ-141、SEQ-182–SEQ-187、SEQ-341、SEQ-365、SEQ-397 |
| Advice 路由 | SEQ-169、SEQ-188–SEQ-191 |
| advisor 与 Model | SEQ-030–SEQ-044、SEQ-290、SEQ-367 |
| 模型检查点 | SEQ-045–SEQ-051、SEQ-363、SEQ-399–SEQ-403 |
| 资金费合成 | SEQ-092、SEQ-192–SEQ-196、SEQ-361 |
| 模式与 cap 的驱动 | SEQ-132、SEQ-183、SEQ-197–SEQ-213、SEQ-342、SEQ-353、SEQ-368 |
| 模拟账户 | SEQ-067、SEQ-179、SEQ-201、SEQ-214–SEQ-217、SEQ-298 |
| 输出扇出 | SEQ-218、SEQ-219、SEQ-343 |
| 通知队列与续推 | SEQ-071、SEQ-072、SEQ-084、SEQ-133、SEQ-204、SEQ-220–SEQ-223 |
| journal 满 | SEQ-013、SEQ-083 |
| 出环满 | SEQ-224、SEQ-225 |
| inst → 策略的稠密索引 | SEQ-016、SEQ-226–SEQ-233 |
| 定时器 | SEQ-079、SEQ-215、SEQ-234–SEQ-241、SEQ-261、SEQ-349–SEQ-351、SEQ-407–SEQ-410 |
| 快照 | SEQ-017、SEQ-085、SEQ-088–SEQ-090、SEQ-109、SEQ-112、SEQ-120、SEQ-164、SEQ-190、SEQ-242–SEQ-248、SEQ-250–SEQ-255、SEQ-337、SEQ-356 |
| 冷启动恢复(RecoverDriver) | SEQ-108、SEQ-205、SEQ-206、SEQ-237、SEQ-256–SEQ-267、SEQ-342、SEQ-366、SEQ-398 |
| 恢复时决策不一致 | SEQ-268、SEQ-269、SEQ-362、SEQ-396 |
| 毒丸输入隔离 | SEQ-270–SEQ-272 |
| 版本变化与回滚 | SEQ-257、SEQ-272–SEQ-274 |
| 启动闸门 | SEQ-094、SEQ-157、SEQ-160、SEQ-207、SEQ-210、SEQ-275、SEQ-345 |
| 存储降级 | SEQ-093、SEQ-276、SEQ-277 |
| 命令路径 | SEQ-009、SEQ-040、SEQ-122、SEQ-185、SEQ-186、SEQ-189、SEQ-211、SEQ-235、SEQ-255、SEQ-278–SEQ-304、SEQ-348、SEQ-354、SEQ-355、SEQ-357、SEQ-358、SEQ-365 |
| Drain | SEQ-187、SEQ-209、SEQ-222、SEQ-284、SEQ-305–SEQ-309、SEQ-370、SEQ-421 |
| 确定性规则 | SEQ-043、SEQ-053、SEQ-076、SEQ-097、SEQ-254、SEQ-267、SEQ-310–SEQ-313、SEQ-344 |
| 状态哈希 | SEQ-054、SEQ-245、SEQ-247、SEQ-249、SEQ-311、SEQ-314、SEQ-396 |
| 规划源文件 | SEQ-310、SEQ-317–SEQ-339 |
| 延迟与资源契约 | SEQ-315–SEQ-339 |
| 测试与验收 | 不适用:非规范 |
| 依赖 | SEQ-018 |
| 延伸阅读 | 不适用:非规范 |
| 已知限制与后续工作 | 不适用:非规范 |
| 开发备忘 | 不适用:非规范 |

## 用例

**基线装配 `B0`**(`crates/hft-sequencer/tests/common/mod.rs::b0()`;下面每个用例的 `前置` 写"B0"时,以下取值都显式生效,用例另列的参数覆盖它们;不依赖任何默认值):

- 策略集:本 crate 测试内的脚本化测试集合 `TestSet`(`tests/common/strat.rs`;kind_id 0xFF01 taker、0xFF02 target(带 Model)、0xFF03 np(`non_prod_only`),描述符版本 3;`TestSetV4`/`TestSetV5` 只改描述符版本),行为由线程内脚本钩子给出,不用 hft-testkit 的 `TestStrategies`(其行为固定,做不出逐用例的意图)。
- refdata(id 稠密:refdata 加载要求 inst/账户/实例连续编号,空位用填充行):venue A = `VenueId 2`(`BINANCE_SPOT`,现货)、venue B = `VenueId 3`(`GATE_USDT_FUT`,USDT 永续);inst 1–22,其中 `a1`(`InstId 11`,A 上 BTC/USDT,tick 0.01、lot 0.0001、`min_notional` 5 USDT)、`a2`(`InstId 12`,A 上 ETH/USDT,lot 0.001)、`a3`(`InstId 13`,A 上 BTC/USDC)、`b1`(`InstId 21`,B 上 BTC-USDT 永续,tick 0.1、lot 0.001、`min_notional` 5 USDT)、`b2`(`InstId 22`,B 上 ETH-USDT 永续,lot 0.01),其余为 A 上的填充现货;账户 1、2 = `AccountKind::Live`(`is_prod = false`,venues {A, B}),账户 9 = `Sim`(venues {A, B},初始 USDT 余额),账户 3–8 为无 venue 的 Live 填充行;实例 1–9(实例 2 为 target,其余 taker),s1(绑定账户 1)、s2(绑定账户 2)、s9(绑定账户 9);group `g1 = 0x0100_0001`(s1,腿 a1 + b1)、`g2 = 0x0200_0001`(s2,腿 a1 + b1)、`g9 = 0x0900_0001`(s9,腿 a1 + b1),ETH 用例另用腿 a2 + b2 的 group;订单限速每 (venue, 账户) 10 s 100 单、60 s 600 单。
- 组件配置:OMS `max_out_per_event`(K)= 16、`links_per_account = 4`、每张槽表 16 槽;`ExecConfig` 含账户 1–9(9 为 Sim 行);`RiskParams` 每账户一行(`max_order_notional` 10 000、`max_hedge_notional` 20 000、`max_reduce_notional` 10 000 USDT,`max_perp_leverage_x100 = 2 000`),账户 1、2 每 venue 一行;`keys_required` 每 (Live 账户, venue) = 3。
- EngineConfig 段 3:`max_insts = 64`、`max_cells_total = 32`、`max_subs_per_inst = 4`、`max_groups_per_instance = 8`、`max_timers = 256`、`recent_cmd_ids = 64`、`max_instances = 4`;段 5:rings ctl 4/4、hk 4/4、rep 32/64、chain 32/0、bbo 32/128、advice 16/16,`md_round_ns = 50 000`、`sim_max_per_round = 32`、`ctl_bytes_per_round = 8 192`、`conflate_backlog = 64`、`notify_per_continue = 64`、`keyframe_period_ms = 300 000`、`keyframe_max_events = 1 500 000`、`replay_per_round = 1 024`、`advisor_mem_budget_mib = 256`、`advisor_ckpt_max_mib = 1 024`、`auto_resume_after_clean_drain = 1`、`allow_missing_standby = 0`;OMS `max_out_per_event`(K)= 16。由此 `U = max_cells_total + 128 = 160`,`NotifyBacklog` 容量 `4U = 640`。
- 时钟与驱动:实盘驱动装配测试内的内存环(`TestIo`:每环一个队列,出环无上限,除非用例另设)与手动推进的 TSC 源,`tsc_hz_milli = 1 000 000 000 000`(1 GHz,1 tick = 1 ns),起点 TSC 1 000 000;UTC 起点 2026-10-01T00:30:00Z;`EngineClock.base_engine_ns = 0`;"稳态"(`H::steady`)= 引导块与 `EngineClock` 已处理;A、B 的 md 连接与各 inst 成员连接 Up(槽 0);账户 1、2、9 在 A、B 的主/热备下单连接与用户流 Up;`ClockOk`;账户 1、2 每 venue 一条 `AccountConfigReport`(B 上 b1、b2 杠杆 3×)与一条对账汇总(`count = 1`)、`KillSelfTestOk`;账户 1、2 在 A、B 各一条 USDT 100 000 的 `BalanceReport` 与 B 上一条 `MarginReport`;a1、b1、a2、b2 各一条 BBO;每个 cell 一条已确认的 `SetMode{Live}`——此后全部账户启动闸门已放行、全部 cell 期望模式 Live、无 cap。
- "环头检查"指 README 主循环 d/e/f 段的 Acquire 读;"交出"指 `Driver::take` 返回 true。

### SEQ-001 段 5 每个字段的默认值

- 规则:[环与容量](README.md#环与容量)
- 级别:table
- 里程碑:M2
- 前置:`SeqParamsRow::default()`;不经 hft-cfg
- 实现:`crates/hft-sequencer/tests/params.rs::seq_001_seq_params_defaults`

| 行 | 字段 | 期望 |
|---|---|---|
| SEQ-001.1 | `rings[1]`(Ctl) | `{per_ring: 4, total: 4}` |
| SEQ-001.2 | `rings[2]`(Hk) | `{4, 4}` |
| SEQ-001.3 | `rings[3]`(Rep) | `{32, 64}` |
| SEQ-001.4 | `rings[4]`(Chain) | `per_ring = 32` |
| SEQ-001.5 | `rings[5]`(Bbo) | `{32, 128}` |
| SEQ-001.6 | `rings[6]`(Advice) | `{16, 16}` |
| SEQ-001.7 | `rings[0]`、`rings[7]` | `{0, 0}` |
| SEQ-001.8 | `md_round_ns` | 50 000 |
| SEQ-001.9 | `sim_max_per_round` | 32 |
| SEQ-001.10 | `ctl_bytes_per_round` | 8 192 |
| SEQ-001.11 | `conflate_backlog` | 64 |
| SEQ-001.12 | `notify_per_continue` | 64 |
| SEQ-001.13 | `keyframe_period_ms` | 300 000 |
| SEQ-001.14 | `keyframe_max_events` | 1 500 000 |
| SEQ-001.15 | `replay_per_round` | 1 024 |
| SEQ-001.16 | `auto_resume_after_clean_drain` | 1 |
| SEQ-001.17 | `keyframe_max_events` × 60 s ÷ 20 000 000(`seq/replay_throughput` 下限,每分钟事件数) | 4.5 s ≤ 5 s(hft-core 恢复尾部约束;任一值改动使它 > 5 s 时本行失败) |

### SEQ-002 SeqParamsRow 与 RingLimit 的大小、对齐与字段偏移

- 规则:[环与容量](README.md#环与容量)
- 级别:unit
- 里程碑:M2
- 前置:无运行时状态
- 步骤:
  1. 取 `size_of`、`align_of` 与 `offset_of`
- 期望:
  - `size_of::<RingLimit>() == 4`,`align_of == 2`
  - `size_of::<SeqParamsRow>() == 72`,`align_of == 4`
  - 偏移:`rings` 0、`md_round_ns` 32、`ctl_bytes_per_round` 36、`keyframe_period_ms` 40、`keyframe_max_events` 44、`advisor_mem_budget_mib` 48、`advisor_ckpt_max_mib` 52、`sim_max_per_round` 56、`conflate_backlog` 58、`notify_per_continue` 60、`replay_per_round` 62、`auto_resume_after_clean_drain` 64、`allow_missing_standby` 65、`_pad` 66
- 实现:`crates/hft-sequencer/tests/params.rs::seq_002_seq_params_layout`

### SEQ-003 段 5 的编码往返与按前缀读取

- 规则:[环与容量](README.md#环与容量)
- 级别:table
- 前置:B0 的段 5 行;经 hft-refdata `EngineConfig::load` 调本 crate 的 `EngineValidator`
- 里程碑:M2
- 实现:`crates/hft-sequencer/tests/params.rs::seq_003_seq_params_roundtrip`

| 行 | 输入 | 期望 |
|---|---|---|
| SEQ-003.1 | B0 行编码后再解码 | 72 字节逐字节相同,各字段与 B0 相等 |
| SEQ-003.2 | `row_size = 80`,行尾追加 8 字节 0xAA | 读前 72 字节,各字段与 B0 相等,加载成功 |
| SEQ-003.3 | 段 5 有 2 行 | `Err(LoadErrCode::Structure)` |
| SEQ-003.4 | 段 5 有 0 行 | `Err(LoadErrCode::Structure)` |

### SEQ-004 段 3 与段 5 的取值校验

- 规则:[环与容量](README.md#环与容量)
- 级别:table
- 里程碑:M2
- 前置:B0;每行只改所列字段,其余为 B0 值
- 实现:`crates/hft-sequencer/tests/params.rs::seq_004_engine_validator`

| 行 | 改动 | 期望 |
|---|---|---|
| SEQ-004.1 | `auto_resume_after_clean_drain = 2` | `Err(ValueRange)` |
| SEQ-004.2 | `allow_missing_standby = 1` | `Ok` |
| SEQ-004.3 | `_pad[0] = 1` | `Err(ValueRange)` |
| SEQ-004.4 | `rings[0] = {1, 1}` | `Err(ValueRange)` |
| SEQ-004.5 | `rings[7] = {0, 1}` | `Err(ValueRange)` |
| SEQ-004.6 | `rings[5] = {0, 128}` | `Err(ValueRange)` |
| SEQ-004.7 | `rings[5] = {127, 128}` | `Ok` |
| SEQ-004.8 | `rings[5] = {128, 128}` | `Ok` |
| SEQ-004.9 | `rings[5] = {129, 128}` | `Err(ValueRange)` |
| SEQ-004.10 | `rings[4] = {32, 0}`(Chain 只用 `per_ring`) | `Ok` |
| SEQ-004.11 | `md_round_ns = 0` | `Err(ValueRange)` |
| SEQ-004.12 | `md_round_ns = 1` | `Ok` |
| SEQ-004.13 | `keyframe_max_events = 0` | `Err(ValueRange)` |
| SEQ-004.14 | `replay_per_round = 0` | `Err(ValueRange)` |
| SEQ-004.15 | `ctl_bytes_per_round = 0` | `Err(ValueRange)` |
| SEQ-004.16 | 段 3 `max_insts = 8191` | `Ok` |
| SEQ-004.17 | 段 3 `max_insts = 8192` | `Ok` |
| SEQ-004.18 | 段 3 `max_insts = 8193` | `Err(ValueRange)` |
| SEQ-004.19 | 段 3 `max_timers = 0` | `Err(ValueRange)` |

### SEQ-005 RingClass 的码值

- 规则:[环与容量](README.md#环与容量)
- 级别:table
- 里程碑:M2
- 前置:无运行时状态;`RingClass::try_from(u8)`
- 实现:`crates/hft-sequencer/tests/params.rs::seq_005_ring_class_codes`

| 行 | 输入 | 期望 |
|---|---|---|
| SEQ-005.1 | 1 | `Ctl` |
| SEQ-005.2 | 2 | `Hk` |
| SEQ-005.3 | 3 | `Rep` |
| SEQ-005.4 | 4 | `Chain` |
| SEQ-005.5 | 5 | `Bbo` |
| SEQ-005.6 | 6 | `Advice` |
| SEQ-005.7 | 0 | `Err`(保留) |
| SEQ-005.8 | 7 | `Err`(保留) |
| SEQ-005.9 | 8 | `Err`(越界) |

### SEQ-006 恢复后占用超过新容量时 boot 返回 CapacityShrink

- 规则:[环与容量](README.md#环与容量)、[装配与运行](README.md#装配与运行由-appshft-coreappsreplayerhft-testkit-调用)
- 级别:table
- 里程碑:M2
- 前置:B0 的 `max_timers = 512`,使用完整 ECF1/RKP1 配置,各账户内 group 的 inst 不重复;真实行情经 Ctx 登记定时器后执行 Drain,已提交快照含订阅 inst 10 个、cell 6 个、策略实例 3 个、s1 有 3 个 group、定时器堆 40 条。`T = engine_timer_demand + 快照中的策略定时器数`,断言 `40 < T < 512`;以只改所列容量的新 EngineConfig 调 `boot(Journal(RecoverPlan{start: DrainTail, replay_tail: false, ..}))`
- 实现:`crates/hft-sequencer/tests/boot.rs::seq_006_capacity_shrink`

| 行 | 新容量 | 期望 |
|---|---|---|
| SEQ-006.1 | `max_insts = 9` | `Err(BootError::CapacityShrink{what: Insts})` |
| SEQ-006.2 | `max_insts = 10` | `Ok` |
| SEQ-006.3 | `max_cells_total = 5` | `Err(CapacityShrink{what: Cells})` |
| SEQ-006.4 | `max_cells_total = 6` | `Ok` |
| SEQ-006.5 | `max_instances = 2` | `Err(CapacityShrink{what: Instances})` |
| SEQ-006.6 | `max_groups_per_instance = 2` | `Err(CapacityShrink{what: GroupsPerInstance})` |
| SEQ-006.7 | `max_timers = T - 1` | `Err(CapacityShrink{what: Timers})`,堆虽放得下但策略配额不足 |
| SEQ-006.8 | `max_timers = T` | `Ok`,保留引擎预留量与全部策略定时器 |
| SEQ-006.9 | `max_insts = 9` 且 `max_timers = 39` | `Err(CapacityShrink{what: Insts})`(按 `CapKind` 顺序报第一项) |
| SEQ-006.10 | `max_cells_total = 5,max_instances = 2,max_groups_per_instance = 2,max_timers = 39` | `Cells`,不能先报组件解码或定时器构造错误 |
| SEQ-006.11 | `max_instances = 2,max_groups_per_instance = 2,max_timers = 39` | `Instances` |
| SEQ-006.12 | `max_groups_per_instance = 2,max_timers = 39` | `GroupsPerInstance` |
| SEQ-006.13 | `max_timers = 39` | `Timers`,实际堆占用超限 |

### SEQ-007 缩容到恰好等于占用后按新容量运行

- 规则:[环与容量](README.md#环与容量)
- 级别:unit
- 里程碑:M2
- 前置:SEQ-006 的快照;新 EngineConfig `max_cells_total = 6`,其余同 B0;boot 成功并进入稳态
- 步骤:
  1. 投递 pipeline 主体的 `SetParams`,为 s1 新增一个 group(使 cell 数为 7)
- 期望:
  - `CmdResult{CapacityExceeded}`;旧参数继续生效(s1 的 group 数仍为 3);输出中没有 `CapSet`、`OrderDecision`
  - 恢复引导块的 Engine ConfigBlob 版本加一,段 3 为新容量,其余配置节不变;仅凭 journal 解码得到 `max_cells_total = 6`
  - 冷恢复先设置 Recovery 与新闸门,再移除旧进程的 Draining;补齐新增腿的连接、行情和杠杆报告,人工 Resume{Recovery} 后全部 cell 的有效模式为 Live,再验证容量拒绝;目标已为 Live 的 SetMode 返回 NoOp,首次设为 Live 返回 Applied
  - 第二次真实 Drain 前设置账户 Kill;按相同容量再恢复,Engine ConfigBlob 身份不变,Kill 原记录保留,Recovery 仍生效,Draining 消失
  - 启用 `replay-io` 时,三次运行与恢复后的拒绝命令经独立验证器全部重算,另从第一次 Recovery 快照起验证;两者 Output 与快照哈希均无分歧
- 实现:`crates/hft-sequencer/tests/boot.rs::seq_007_shrunk_capacity_applies`

### SEQ-008 进程启动后的第一个事件是 EngineClock 与 EpochStart

- 规则:[主循环](README.md#主循环step)、[引擎时间](README.md#引擎时间engine_ns)
- 级别:unit
- 里程碑:M2
- 前置:B0;`boot(Empty{configs})`;第一次 `step` 之前 ctl、rep-A、bbo-A 各已有 1 条
- 步骤:
  1. `step()`
  2. `step()`
- 期望:
  - 步骤 1 后:引导块(`SegmentRoll{BootDone}`)之后恰好 2 条记录:`EngineClock`(Input)、`Note{Sequencer, EpochStart}`(Output,带 `EVENT_END`);三个环各仍有 1 条;返回 `Busy`
  - 步骤 2 交出的第一条 Input 是 ctl 的那条,其后依次是 rep-A、bbo-A 的那条
- 实现:`crates/hft-sequencer/tests/boot.rs::seq_008_first_round_engine_clock_only`

### SEQ-009 停机标记的 Kill 紧跟 EngineClock、先于任何环输入

- 规则:[主循环](README.md#主循环step)、[命令路径](README.md#命令路径)
- 级别:unit
- 里程碑:M2
- 前置:B0;实盘驱动带停机标记 `Kill{Global}`;bbo-A 已有 1 条
- 步骤:
  1. `step()`
- 期望:
  - 引导块之后的 Input 顺序:`EngineClock`、`ControlCmd{Kill{Global}, issuer = cli:systemd, hard = 0, flatten = 0}`、bbo-A 的 `Bbo`
  - Kill 那一事件输出 `CmdResult{Applied}`(不需确认,此时闸门尚未放行)与 `Note{Risk, CapSet}`(`CapRecord.source = Kill`,scope Global)
- 实现:`crates/hft-sequencer/tests/boot.rs::seq_009_stop_marker_kill_second`

### SEQ-010 没有任何输入的一轮返回 Idle

- 规则:[主循环](README.md#主循环step)、[心跳](README.md#心跳)
- 级别:unit
- 里程碑:M2
- 前置:B0 稳态;全部环为空;堆顶 deadline 在 TSC + 10 ms
- 步骤:
  1. 记下 `committed_len` 与心跳计数 h,`step()`
- 期望:
  - 返回 `Idle`;`committed_len` 不变;心跳计数 = h + 1
- 实现:`crates/hft-sequencer/tests/boot.rs::seq_010_idle_round`

### SEQ-011 boot 之后 view() 的初始读数

- 规则:[装配与运行](README.md#装配与运行由-appshft-coreappsreplayerhft-testkit-调用)
- 级别:unit
- 里程碑:M2
- 前置:B0;`boot(Empty{configs})`,尚未 `step`
- 步骤:
  1. 读 `view()` 的各方法
- 期望:
  - `wants_continue() == false`、`notify_headroom() == 640`、`snapshot_requested() == false`、`draining() == false`、`refdata_recheck_due() == false`、`stop_engine_ns() == 0`、`next_funding_settle() == None`(无持仓)
- 实现:`crates/hft-sequencer/tests/boot.rs::seq_011_initial_view`

### SEQ-012 交出顺序就是 journal 顺序

- 规则:[主循环](README.md#主循环step)
- 级别:prop
- 里程碑:M2
- 性质:任意多环到达时刻表下,Input 记录的 seq 顺序 = `take` 交出的顺序;每条环元素至多交出一次(合并丢弃的 `Bbo` 除外),未交出的仍在环里;`VerifyDriver` 重放这段 journal 的 Output 逐字节相同
- 生成:ctl/rep-A/rep-B/bbo-A/bbo-B/advice/md-A/rep-sim 各 0–300 条,到达时刻在 0–5 ms 内均匀抽取;每条处理耗时 0–20 µs;到期定时器 0–20 个
- 参考:testkit 记录的 take 序列与 journal 中 Input 的 seq 序列逐项相等
- 例数:PR 500;每夜 20 000
- 实现:`crates/hft-sequencer/tests/determinism.rs::seq_012_take_order_is_journal_order`

### SEQ-013 append 返回 Full 时 step 返回 Fatal(JournalFull) 且不出环

- 规则:[journal 满](README.md#journal-满)、[装配与运行](README.md#装配与运行由-appshft-coreappsreplayerhft-testkit-调用)
- 级别:unit
- 里程碑:M2
- 前置:B0 稳态;s1 在下一条 bbo-A 上发一笔 a1 买单;测试 `JournalSink` 对该 `Bbo` 之后的第一条 Output 的 append 返回 `AppendErr::Full`
- 步骤:
  1. 投递该 `Bbo`,`step()`
- 期望:
  - 返回 `Fatal(JournalFull)`;out-A 环为空;`committed_len` 不超过该 `Bbo` 之前的值(该事件未 publish)
- 实现:`crates/hft-sequencer/tests/boot.rs::seq_013_journal_full_fatal`

### SEQ-014 被替换的参数对象经 retire 交还

- 规则:[装配与运行](README.md#装配与运行由-appshft-coreappsreplayerhft-testkit-调用)
- 级别:unit
- 里程碑:M2
- 前置:B0 稳态;s1 生效参数 version 1(sha256 = H1)
- 步骤:
  1. 投递 pipeline 主体的 `SetParams{s1, version 2}`(只改阈值类字段)
- 期望:
  - `CmdResult{Applied}`;同一事件内 `drv.retire` 恰好被调用一次,参数为 `ParsedConfig::StrategyParams{sha256 = H1, ..}`;实盘驱动把它推进反向 Mailbox,不发唤醒
- 实现:`crates/hft-sequencer/tests/boot.rs::seq_014_retire_old_params`

### SEQ-015 策略实例在其第一份 StrategyParams 生效时创建

- 规则:[策略集](README.md#策略集strategyset)
- 级别:unit
- 里程碑:M2
- 前置:B0,但 s2 尚无 `StrategyParams`
- 步骤:
  1. 投递一条 bbo-A(a1)
  2. 投递 `SetParams{s2, version 1}`
  3. 投递 `SetParams{s2, version 2}`
- 期望:
  - 步骤 1:s2 不在实例表,没有任何投递给 s2
  - 步骤 2:`S::create` 被调用一次;`CmdResult{Applied}`
  - 步骤 3:`on_params` 被调用一次,`S::create` 调用次数仍为 1
- 实现:`crates/hft-sequencer/tests/strategies.rs::seq_015_instance_created_on_first_params`

### SEQ-016 派发顺序先实盘实例、后模拟实例,各按 StrategyId 升序

- 规则:[策略集](README.md#策略集strategyset)、[inst → 策略的稠密索引](README.md#inst--策略的稠密索引)
- 级别:unit
- 里程碑:M2
- 前置:B0 改为实例 s3(Live 账户 3)、s1(Sim 账户 9)、s2(Live 账户 2),三者都订阅 a1 的 BBO
- 步骤:
  1. 投递一条 bbo-A(a1)
- 期望:
  - `dispatch_order = [2, 3, 1]`;三个实例收到 `EventRef::Bbo` 的顺序为 s2、s3、s1
- 实现:`crates/hft-sequencer/tests/strategies.rs::seq_016_dispatch_order_live_first`

### SEQ-017 StrategyDesc.version 写进节 5 行头

- 规则:[策略集](README.md#策略集strategyset)、[快照](README.md#快照)
- 级别:unit
- 里程碑:M2
- 前置:B0 稳态;probe-taker 的 `StrategyDesc.version = 3`
- 步骤:
  1. 投递 `SnapshotNow`,等其 `StateSnapshot{OnDemand}` 写出
- 期望:
  - 节 5 中 s1 行的 `StrategyRowHdr{strategy: 1, account: 1, kind_id: 0xFF01, desc_version: 3, params_version: 1}`,`n_groups = 1`,其后一条 `GroupSeqRow{group: g1}`
- 实现:`crates/hft-sequencer/tests/strategies.rs::seq_017_desc_version_in_section5`

### SEQ-018 策略只经静态分派进入本 crate

- 规则:[策略集](README.md#策略集strategyset)、[依赖](README.md#依赖)
- 级别:table
- 里程碑:M2
- 前置:扫描 `crates/hft-sequencer/src/**/*.rs` 与 `Cargo.toml`(含 `[dev-dependencies]`)
- 实现:`crates/hft-sequencer/tests/lint.rs::seq_018_static_dispatch_and_deps`

| 行 | 检查 | 期望 |
|---|---|---|
| SEQ-018.1 | 源码中的 `dyn ` | 0 处 |
| SEQ-018.2 | 依赖 `hft-strategy-registry` 或任何 `strategies/*` | 不存在;人为加入后 `cargo xtask deps` 失败 |
| SEQ-018.3 | 依赖 `hft-strat-lib`、`hft-sim`、`hft-net`、`hft-venue`、`venue-*`、`tokio` | 均不存在;人为加入任一项后 `cargo xtask deps` 失败 |
| SEQ-018.4 | 依赖任何 `tier: cold` 的 crate | 不存在 |
| SEQ-018.5 | 源码中的 `std::thread::spawn`、`std::fs`、`std::net` | 0 处(本 crate 不创建线程、不做 journal 之外的 I/O) |

### SEQ-019 Now 的字段取自当前 Input 与段头

- 规则:[只读视图](README.md#只读视图seqviews实现-sdk-views)
- 级别:unit
- 里程碑:M2
- 前置:B0 稳态;`EngineClock` 的记录头 tsc = 1 000 000、seq = 50、`base_engine_ns = 0`;段头 `tsc_hz_milli = 10^12`、`calib_tsc = 1 000 000`、`calib_realtime_ns = 1 790 000 000 000 000 000`;最近一条 `Timer{ClockOk}`
- 步骤:
  1. 在 TSC = 3 000 000 交出一条 bbo-A,s1 在 `on_event` 中读 `v.now()`
- 期望:
  - `now.tsc = 3 000 000`;`now.seq` = 该 `Bbo` 的 seq;`now.engine_ns = 2 000 000`;`now.boot_seq` = `Note{EpochStart}` 的 seq(51);`now.tsc_per_ms = 1 000 000`;`now.utc_ns = 1 790 000 000 002 000 000`;`now.clock_ok = true`
- 实现:`crates/hft-sequencer/tests/views.rs::seq_019_now_fields`

### SEQ-020 clock_ok 只跟随 Timer{ClockOk|ClockBad},不读 Meta ClockStatus

- 规则:[只读视图](README.md#只读视图seqviews实现-sdk-views)、[记录分类](README.md#记录分类)
- 级别:unit
- 里程碑:M2
- 前置:B0 稳态;`clock_ok = true`
- 步骤:
  1. hk 环送入一条 `ClockStatus{Unsynced}`(Meta)后投递一条 bbo-A
  2. 驱动写 `Timer{Sys, ClockBad}` 后投递一条 bbo-A
  3. 驱动写 `Timer{Sys, ClockOk}` 后投递一条 bbo-A
- 期望:
  - 步骤 1:s1 读到 `now.clock_ok = true`,有效模式不变
  - 步骤 2:`now.clock_ok = false`;同一 Timer 事件内 `risk.mode.apply(Clock{ok: false})`
  - 步骤 3:`now.clock_ok = true`
- 实现:`crates/hft-sequencer/tests/views.rs::seq_020_clock_ok_from_timer_only`

### SEQ-021 bbo(i).rx_tsc 只在晚于 rx_from_seq 时可见

- 规则:[只读视图](README.md#只读视图seqviews实现-sdk-views)
- 级别:table
- 里程碑:M2
- 前置:B0 稳态;`boot_seq = 51`;每行从上一行的状态继续
- 实现:`crates/hft-sequencer/tests/views.rs::seq_021_rx_tsc_visibility`

| 行 | 动作 | 期望 `bbo(a1).rx_tsc` |
|---|---|---|
| SEQ-021.1 | 本次运行尚未收到 a1 的 `Bbo` | 0 |
| SEQ-021.2 | 收到 a1 的 `Bbo{rx_tsc = 5 000}`(seq 60,被应用) | 5 000 |
| SEQ-021.3 | 写关键帧(`at_seq = 70`) | 0 |
| SEQ-021.4 | 收到 a1 的 `Bbo{rx_tsc = 9 000}`(seq 80,被应用) | 9 000 |
| SEQ-021.5 | 从 SEQ-021.3 的关键帧起步的 `VerifyDriver` 在 seq 75 处 | 0(与实盘相同) |

### SEQ-022 bbo(i).hw_rx_ns 与 SW_TS 取自最近一条被应用的 Bbo

- 规则:[只读视图](README.md#只读视图seqviews实现-sdk-views)、[连接活性](README.md#连接活性)
- 级别:unit
- 里程碑:M2
- 前置:B0 稳态
- 步骤:
  1. 投递 a1 的 `Bbo{hw_rx_ns = 111, 记录头无 SW_TS}`
  2. 投递 a1 的 `Bbo{hw_rx_ns = 222, 记录头带 SW_TS}`
  3. 投递 a1 的 `Bbo{hw_rx_ns = 333}`,其 `MarketHdr` 序号使簿判为 OLDER(不被应用)
- 期望:
  - 步骤 1 后 `hw_rx_ns = 111`、`MarketLast.bbo_flags & SW_TS = 0`
  - 步骤 2 后 `hw_rx_ns = 222`、`bbo_flags & SW_TS = 2`
  - 步骤 3 后仍为 222 与 `SW_TS`
- 实现:`crates/hft-sequencer/tests/views.rs::seq_022_hw_rx_and_sw_ts`

### SEQ-023 FundingView.upd_tsc 与 rx_tsc 同一可见性规则

- 规则:[只读视图](README.md#只读视图seqviews实现-sdk-views)
- 级别:unit
- 里程碑:M2
- 前置:B0 稳态
- 步骤:
  1. 投递 b1 的 `Funding{rx_tsc = 7 000}`
  2. 写关键帧
  3. 读 `funding(b1).upd_tsc`
- 期望:
  - 步骤 1 后 `upd_tsc = 7 000`;步骤 3 读到 0;`funding_rate`、`next_funding_ns` 在关键帧前后不变
- 实现:`crates/hft-sequencer/tests/views.rs::seq_023_funding_upd_tsc_visibility`

### SEQ-024 leverage(a, i) 换成百分之一倍并向下取整

- 规则:[只读视图](README.md#只读视图seqviews实现-sdk-views)
- 级别:table
- 里程碑:M2
- 前置:B0;账户 1 在 b1 上 `Risk::leverage` 按行设置
- 实现:`crates/hft-sequencer/tests/views.rs::seq_024_leverage_x100_floor`

| 行 | `Risk::leverage(1, b1)` | 期望 `leverage(1, b1)` |
|---|---|---|
| SEQ-024.1 | 10.00 倍 | `Some(1000)` |
| SEQ-024.2 | 12.349 倍 | `Some(1234)` |
| SEQ-024.3 | 12.3401 倍 | `Some(1234)` |
| SEQ-024.4 | 0.009 倍 | `Some(0)` |
| SEQ-024.5 | 未知 | `None` |

### SEQ-025 传给 exec.advance 的视图不含执行层

- 规则:[只读视图](README.md#只读视图seqviews实现-sdk-views)
- 级别:unit
- 里程碑:M5
- 前置:B0 稳态;s2 对 g2 有活动目标
- 步骤:
  1. 投递一条 bbo-A,使 `exec.has_pending()` 为真;在 `exec.advance` 内经视图读 `target(2, g2)`;同一 Input 中 s2 的 `on_event` 内读 `target(2, g2)`
- 期望:
  - `advance` 内读到 `None`;策略读到 `Some`,其值等于 `exec.view(2, g2)`
- 实现:`crates/hft-sequencer/tests/views.rs::seq_025_advance_view_without_exec`

### SEQ-026 RiskView 的第二腿可负担性读数

- 规则:[只读视图](README.md#只读视图seqviews实现-sdk-views)
- 级别:unit
- 里程碑:M2
- 前置:B0 稳态;账户 1 在 A 的 `CashView.free` = 1 000 USDT,在途买单名义 300 USDT;在 B 的 `MarginState.rep_avail` = 800 USDT,`InflightBook.open_margin(1, B)` = 150 USDT
- 步骤:
  1. 读 `RiskView` 适配器的 `avail_quote(1, A)` 与 `avail_margin(1, B)`
- 期望:
  - `avail_quote = 700 USDT`;`avail_margin = 650 USDT`
- 实现:`crates/hft-sequencer/tests/views.rs::seq_026_affordability_reads`

### SEQ-027 open_qty_immediate 只计即时在途

- 规则:[只读视图](README.md#只读视图seqviews实现-sdk-views)
- 级别:unit
- 里程碑:M2
- 前置:B0 稳态;账户 1 在 b1 上有在途 IOC 卖 0.002 与 GTC 卖 0.005
- 步骤:
  1. 读 `RiskView` 的 `open_qty` 与 `open_qty_immediate`
- 期望:
  - `open_qty = 0.007`;`open_qty_immediate = 0.002`
- 实现:`crates/hft-sequencer/tests/views.rs::seq_027_open_qty_immediate`

### SEQ-028 RiskView::bbo 与 Views::bbo 是同一投影

- 规则:[只读视图](README.md#只读视图seqviews实现-sdk-views)、[连接活性](README.md#连接活性)
- 级别:prop
- 里程碑:M5
- 性质:任意连接与簿状态下,`RiskView::bbo(i)`、`Views::bbo(i)`、传给 `price_hedge` 与执行层的 `BboView` 逐字段相等
- 生成:每 inst 0–8 个成员槽,各槽 `up`/`resync`/`live_tsc` 随机;inst 的 `RESYNC` 随机;簿状态 ∈ {Live, Resyncing};Top 随机
- 参考:三处读数两两相等
- 例数:PR 1 000;每夜 100 000
- 实现:`crates/hft-sequencer/tests/views.rs::seq_028_risk_view_bbo_same_projection`

### SEQ-029 视图方法的数据来源

- 规则:[只读视图](README.md#只读视图seqviews实现-sdk-views)
- 级别:table
- 里程碑:M2
- 前置:B0 稳态;每行在同一状态下比较视图读数与所列组件的直接读数
- 实现:`crates/hft-sequencer/tests/views.rs::seq_029_view_sources`

| 行 | 视图方法 | 期望等于 |
|---|---|---|
| SEQ-029.1 | `mode(1, g1)` | `risk.mode.effective(CellKey{1, 1, g1})` |
| SEQ-029.2 | `venue_ready(1, B)` | `oms.venue_ready(B, 1)`;账户 1 在 B 有一个 Unknown 槽时为 false |
| SEQ-029.3 | `order(h)` | `oms.slot(h)` 的视图 |
| SEQ-029.4 | `legs(1, g1)` | `oms.legs(1, g1)` |
| SEQ-029.5 | `target(2, g2)`(策略侧) | `exec.view(2, g2)` |
| SEQ-029.6 | `risk(1)` | `risk.view(1, &now)`,其中 `dust_notional` = 账户 1 的 `ExecConfig.dust_notional` |
| SEQ-029.7 | `inst(b1)`、`caps(B)`、`fee(1, b1)` | refdata `InstMeta`、`VenueCaps`、`FeeSchedule` |
| SEQ-029.8 | `funding(b1)`、`mark(b1)` | 本 crate `MarketLast` 的 b1 行 |
| SEQ-029.9 | `portfolio(1)` | ledger `VenueBook`/`Equity` 加 OMS 在途量 |

### SEQ-030 poll_once 每次只做一批记录或一块检查点,从不阻塞

- 规则:[advisor](README.md#advisor)、[advisor 与 Model](README.md#advisor-与-model)
- 级别:unit
- 里程碑:M2
- 前置:B0;`Advisor<TestStrategies, MemCkptStore>`(内存实现的 `CheckpointStore`);journal 已提交 5 000 条 Input;一个检查点作业进行中(还剩 3 块)
- 步骤:
  1. 连续调用 `poll_once` 3 次
  2. 再调用 1 次
- 期望:
  - 步骤 1:每次恰好一次 `write_chunk`,镜像已应用的末条 seq 不变,Model 收到的事件数不变;第 3 次之后 `commit` 被调用一次
  - 步骤 2:镜像前进 ≥ 1 条记录,不调用任何 `CheckpointStore` 方法;四次调用都返回(无阻塞、无睡眠)
- 实现:`crates/hft-sequencer/tests/advisor.rs::seq_030_poll_once_bounded`

### SEQ-031 outbox 按 (strategy, account, group, local_kind) 只留最新一条

- 规则:[advisor](README.md#advisor)、[advisor 与 Model](README.md#advisor-与-model)
- 级别:unit
- 里程碑:M2
- 前置:B0;advice 环剩余 0 字节(满);s2 的 Model 在同一 `poll_once` 内对 (2, 2, g2, kind 7) 产出 3 条载荷 P1、P2、P3,对 (2, 2, g2, kind 8) 产出 1 条 Q1
- 步骤:
  1. `poll_once`
  2. 腾出 advice 环空间后 `poll_once`
- 期望:
  - 步骤 1 后 outbox 有 2 条:P3 与 Q1;`advice_coalesced` 增加 2;环内已有记录不变
  - 步骤 2 后环内按入 outbox 的顺序新增 P3、Q1 两条;outbox 为空
- 实现:`crates/hft-sequencer/tests/advisor.rs::seq_031_outbox_keeps_latest`

### SEQ-032 advice 环放不下下一条时留在 outbox,不阻塞、不丢弃

- 规则:[advisor 与 Model](README.md#advisor-与-model)
- 级别:table
- 里程碑:M2
- 前置:B0;下一条待推的 advice 记录(`AdviceHdr` 32 B + 载荷 512 B)共 544 B
- 实现:`crates/hft-sequencer/tests/advisor.rs::seq_032_advice_ring_space`

| 行 | advice 环剩余字节 | 期望 |
|---|---|---|
| SEQ-032.1 | 543 | 不推入,留在 outbox;环内已有记录不变;`poll_once` 返回 |
| SEQ-032.2 | 544 | 推入 |
| SEQ-032.3 | 545 | 推入 |

### SEQ-033 AdviceHdr 的 basis_seq 与 basis_tsc 取 Model 已消费的末条记录

- 规则:[advisor 与 Model](README.md#advisor-与-model)
- 级别:unit
- 里程碑:M2
- 前置:B0;s2 的 Model 在消费 seq 900(记录头 tsc = 77 000)的 `Bbo` 后产出一条 Advice
- 步骤:
  1. `poll_once`,读推入环的 `AdviceHdr`
- 期望:
  - `{strategy: 2, basis_seq: 900, basis_tsc: 77 000}`;sequencer 把它投递为 `EventRef::Advice{basis_seq: 900, basis_tsc: 77 000}`,与环内值逐字节相同
- 实现:`crates/hft-sequencer/tests/advisor.rs::seq_033_basis_seq_tsc`

### SEQ-034 镜像在每个 seq 处的状态等于 VerifyDriver

- 规则:[advisor 与 Model](README.md#advisor-与-model)
- 级别:integ
- 里程碑:M9
- 前置:B0;一段含 200 000 条 Input 的 DST journal(含 `SetParams`、`Kill`、Advice、关键帧、段滚动)
- 步骤:
  1. `MirrorDriver` 从第一个引导块起步跑完;每 1 000 条按状态哈希规则算一次哈希
  2. `VerifyDriver` 跑同一 journal,在相同 seq 处算哈希
- 期望:
  - 两组哈希逐项相等;镜像没有出环、没有写 journal,Meta 记录被跳过
- 实现:`crates/hft-sequencer/tests/advisor.rs::seq_034_mirror_matches_verify`

### SEQ-035 Model 只收 SDK 投递表规定的变体

- 规则:[advisor 与 Model](README.md#advisor-与-model)
- 级别:table
- 里程碑:M2
- 前置:B0;s2 的 Model 订阅 a1 的 BBO(`Part::Model`)、`model_own(ORDER)`,不订阅 `EXEC`、`MANUAL`;每行投递一条 Input 后检查 Model 收到的 `EventRef`
- 实现:`crates/hft-sequencer/tests/advisor.rs::seq_035_model_delivery_table`

| 行 | Input | 期望 Model 收到 |
|---|---|---|
| SEQ-035.1 | bbo-A(a1) | 一条 `Bbo{inst: a1}` |
| SEQ-035.2 | bbo-A(a2,未订阅) | 无 |
| SEQ-035.3 | s2 的一张订单的 `OrderEvent{Ack}` | 一条 `Order`,与快速部分收到的同值 |
| SEQ-035.4 | 账户 2 的 `ExternalFill` | 无(快速部分收到 `External`) |
| SEQ-035.5 | 账户 2 的 `MarginReport` 档位变化 | 无 `Risk` |
| SEQ-035.6 | g2 的有效模式变化 | 无 `Mode` |
| SEQ-035.7 | B 的 `ConnState{Down}` | 无 `Conn` |
| SEQ-035.8 | 目标进度通知 | 无 `Exec`(未订阅 `model_own(EXEC)`) |
| SEQ-035.9 | 本实例的 `Advice` | 无 |

### SEQ-036 Model 的时间刻度只来自 journal 中的 Timer{Sys, AdvisorTick}

- 规则:[advisor 与 Model](README.md#advisor-与-model)、[环境决定写成 Input](README.md#环境决定写成-input)
- 级别:unit
- 里程碑:M2
- 前置:B0;s2 订阅 1 s 模型 tick 与 a1 BBO;engine_ns 0 s 的首条 `AdvisorTick` 建立跨界基准,随后写关键帧;1 s、2 s、3 s 各有一条 `Timer{Sys, AdvisorTick}`,2.5 s 处另有 1 000 条 `Bbo`
- 步骤:
  1. 分别以每批 1、137 条 Input 镜像重放这段 journal,记录 `model_tick` 的投递位置;追平后再空轮询 100 次
- 期望:
  - 恰好 3 次 tick,分别在三条 `AdvisorTick` 记录处;Bbo 的数量与 advisor 的实际耗时不改变 tick 次数与位置
- 实现:`crates/hft-sequencer/tests/advisor/acceptance.rs::seq_036_model_tick_from_journal`

### SEQ-037 model_on_params 返回 ResetGroups 时每个被重置的 group 写一条 ModelGroupReset

- 规则:[advisor 与 Model](README.md#advisor-与-model)
- 级别:unit
- 里程碑:M2
- 前置:B0;s2 有 g2 与 `g2b = 0x0200_0002`;测试 Model 对 version 5 返回 `ResetGroups([g2, g2b], 原因 Structural)`
- 步骤:
  1. 投递 `SetParams{s2, version 5}`,advisor 镜像应用它
- 期望:
  - advisor 经 hk 环写出 2 条 Meta `Note{Advisor, ModelGroupReset}`:`Note.strategy = 2`,`Note.group` 分别为 g2、g2b,`v[0] = 1`(Structural),`v[1] = 5`
- 实现:`crates/hft-sequencer/tests/advisor.rs::seq_037_model_group_reset_notes`

### SEQ-038 ResetAll 的注记原因码为 4

- 规则:[advisor 与 Model](README.md#advisor-与-model)
- 级别:unit
- 里程碑:M2
- 前置:同 SEQ-037,测试 Model 对 version 6 返回 `ResetAll`
- 步骤:
  1. 投递 `SetParams{s2, version 6}`
- 期望:
  - g2、g2b 各一条 `Note{ModelGroupReset}`,`v[0] = 4`、`v[1] = 6`
- 实现:`crates/hft-sequencer/tests/advisor.rs::seq_038_reset_all_reason_4`

### SEQ-039 重启追赶时重写的 ModelGroupReset 与原注记的去重键相同

- 规则:[advisor 与 Model](README.md#advisor-与-model)
- 级别:unit
- 里程碑:M2
- 前置:SEQ-037 之后写了一份 `last_seq` 早于该 `SetParams` 的模型检查点;进程重启
- 步骤:
  1. advisor 从检查点续算并重放到该 `SetParams` 之后
- 期望:
  - 再写出 2 条 `Note{ModelGroupReset}`,其 (strategy, group, `v[1]`) 与 SEQ-037 的两条逐一相同
- 实现:`crates/hft-sequencer/tests/advisor/acceptance.rs::seq_039_reset_note_dedup_key_on_catchup`

### SEQ-040 Applied 的 InjectWarmup 依序投递给 Model

- 规则:[advisor 与 Model](README.md#advisor-与-model)、[命令路径](README.md#命令路径)
- 级别:unit
- 里程碑:M2
- 前置:B0;s2 的 Model 存在;快速部分未声明 `warmup(Part::Both)`
- 步骤:
  1. 依次投递 `InjectWarmup{instance: 2, local_kind: 3, chunk_no: 0, 1, 2, last: 0, 0, 1}`(主体 pipeline,`account = *`)
  2. 投递 `InjectWarmup{instance: 7}`(实例不存在)
- 期望:
  - 步骤 1:三条都 `CmdResult{Applied}`;Model 依次收到 3 个 `EventRef::Warmup`,`chunk_no` 为 0、1、2,`last` 只在第三个为 true;快速部分没有收到 `Warmup`
  - 步骤 2:`CmdResult{UnknownTarget}`;Model 没有收到任何 `Warmup`
- 实现:`crates/hft-sequencer/tests/advisor.rs::seq_040_warmup_to_model`

### SEQ-041 AdvisorLag 每实例每 10 s 至多一条,回落后不再写

- 规则:[advisor 与 Model](README.md#advisor-与-model)
- 级别:unit
- 里程碑:M2
- 前置:B0;hft-core `advisor_max_lag_ms = 5 000`;已提交时间从 engine_ns 6 s 起,以读者的消费边界使 s2 的 Model 落后 6 000 ms,持续 25 s;之后回落到 1 000 ms 持续 30 s,不改引擎状态或告警计数
- 步骤:
  1. 运行全过程,收集 hk 环上的 `Note{Advisor, AdvisorLag}`
- 期望:
  - 落后期间恰好 3 条(相对观察起点 0 s、10 s、20 s 处),`v[0] = 2`、`v[1] = 6 000`;回落之后 0 条
- 实现:`crates/hft-sequencer/tests/advisor/acceptance.rs::seq_041_advisor_lag_rate_limited`

### SEQ-042 AdvisorLag 跨重启按 engine_ns 连续计算

- 规则:[advisor 与 Model](README.md#advisor-与-model)、[引擎时间](README.md#引擎时间engine_ns)
- 级别:unit
- 里程碑:M2
- 前置:B0,引导块写入完整 ECF1/RKP1 配置供跨运行重建;上次运行末条记录 engine_ns = 100 s;停机 10 min;新运行从检查点续算,检查点 `last_seq` 处的 engine_ns = 99 s
- 步骤:
  1. 新运行 TSC 重置、运行 6 s,已提交末条 engine_ns = 106 s;advisor 从旧检查点启动,每批 1 条 Input,首批镜像仍未追到检查点
  2. 存储适配器把非文件字段 `last_ens` 提示改成 −123,继续运行至追平
- 期望:
  - s2 落后量 = 106 s − 99 s = 7 s;另一个实例 s1 持有 98 s 的检查点,落后量为 8 s。两者各一条告警,`v[1]` 分别为 7 000、8 000,`v[3]` 与 seq 差额按各自检查点位置计算。停机 10 min、镜像正在补读的旧前缀及错误时间提示均不计入
- 实现:`crates/hft-sequencer/tests/advisor/acceptance.rs::seq_042_lag_continuous_across_restart`

### SEQ-043 Model 两次运行产出相同的 Advice 载荷序列

- 规则:[advisor 与 Model](README.md#advisor-与-model)、[确定性规则](README.md#确定性规则)
- 级别:integ
- 里程碑:M2
- 前置:B0;同一段 50 000 条 Input 的 journal;advice 环容量两次运行不同(64 KiB 与 4 KiB)
- 步骤:
  1. 两次从同一引导块起运行 advisor,收集推入环的 `(AdviceHdr, payload)`
- 期望:
  - 两次的 `(basis_seq, local_kind, payload)` 序列在去掉被 outbox 覆盖的条目后逐字节相同;被覆盖的条目数分别计入各自的 `advice_coalesced`
  - 每条生成的 256 B 载荷含 Model 累计消费数与 Input seq,独立从 journal 计数核对;按固定 64 次 poll 的消费周期,参考字节环与单键 outbox 逐批核对实际送出字节及覆盖数。4 KiB 行必须实际产生更多覆盖,两行最终均送出末条载荷
- 实现:`crates/hft-sequencer/tests/advisor/acceptance.rs::seq_043_model_deterministic`

### SEQ-044 Advice 是 Input:验证回放不运行 Model

- 规则:[advisor 与 Model](README.md#advisor-与-model)
- 级别:unit
- 里程碑:M2
- 前置:B0;一段含 300 条 `Advice` 的 journal
- 步骤:
  1. `VerifyDriver` 重放,装配一个任何调用都 panic 的 Model
- 期望:
  - 重放通过(Output 逐字节一致),Model 未被调用;策略收到的 300 条 `EventRef::Advice` 与日志中的载荷逐字节相同
- 实现:`crates/hft-sequencer/tests/advisor.rs::seq_044_verify_does_not_run_model`

### SEQ-045 模型检查点按块写出,期间整个 advisor 停在 last_seq

- 规则:[模型检查点](README.md#模型检查点)
- 级别:unit
- 里程碑:M2
- 前置:B0;s2 的 Model 状态 30 MiB(路径状态节 1 MiB + 29 MiB);`advisor.checkpoint_chunk_kib = 1 024`;`advisor.model_checkpoint_ms = 60 000` 到期;journal 在检查点期间持续增加 10 000 条
- 步骤:
  1. 连续 `poll_once` 直到 `commit`
- 期望:
  - `begin` 一次(`CkptHdr.last_seq` = 开始时 Model 已消费的末条 seq L);`write_chunk` 30 次,每次 `poll_once` 恰好一次,第一块是路径状态节;`commit` 一次
  - 期间镜像已应用的末条 seq 始终为 L,任何 Model 都没有收到事件;`commit` 之后的 `poll_once` 从 L + 1 继续
- 实现:`crates/hft-sequencer/tests/advisor.rs::seq_045_chunked_checkpoint_pauses_advisor`

### SEQ-046 检查点加续算的 Advice 与连续运行逐字节相同,与块大小无关

- 规则:[模型检查点](README.md#模型检查点)
- 级别:table
- 里程碑:M2
- 前置:B0;一段 100 000 条 Input 的 journal;在 seq 40 000 处写检查点,进程在 seq 60 000 处重启,advisor 从检查点续算到末尾
- 实现:`crates/hft-sequencer/tests/advisor/acceptance.rs::seq_046_checkpoint_resume_equals_continuous`

| 行 | `checkpoint_chunk_kib` | 期望 |
|---|---|---|
| SEQ-046.1 | 256 | seq 60 000 之后产出的 `(basis_seq, local_kind, payload)` 与不中断运行逐字节相同 |
| SEQ-046.2 | 1 024 | 同上 |
| SEQ-046.3 | 4 096 | 同上 |

逐条比较合并前的全部生成载荷,不靠丢弃 outbox 覆盖条目缩小恢复验证范围;独立按 journal 计算 Model 累计量,最终检查点头与体也与连续运行相同。此用例使用小状态 Model,大状态分块停顿另由 SEQ-045 验收。

### SEQ-047 检查点丢弃的原因码

- 规则:[模型检查点](README.md#模型检查点)
- 级别:table
- 里程碑:M2
- 前置:B0;s2 有一份有效检查点 C(`last_seq` = 40 000,热段可读);每行只改所列一项后重启
- 实现:`crates/hft-sequencer/tests/advisor.rs::seq_047_checkpoint_discard_reasons`

| 行 | 改动 | 期望 |
|---|---|---|
| SEQ-047.1 | 删除 C | `S::create_model` 空状态;`Note{Advisor, ModelCheckpointDiscarded}` `v[0] = 2`、`v[1] = 1`(Missing) |
| SEQ-047.2 | 翻转 C 的一个字节 | 同上,`v[1] = 2`(Corrupt) |
| SEQ-047.3 | 布局描述完全不变,只将 `Model::LAYOUT_VERSION` + 1 | `v[1] = 3`(KeyMismatch) |
| SEQ-047.4 | `schema.id` 不同 | `v[1] = 3` |
| SEQ-047.5 | 实例的 `kind_id` 不同 | `v[1] = 3` |
| SEQ-047.6 | 检查点属于实例 1 | `v[1] = 3` |
| SEQ-047.7 | 热段与冷副本都已不含 seq 40 000 | `v[1] = 4`(SeqNotInJournal) |
| SEQ-047.8 | `model_restore` 返回错误 | `v[1] = 5`(RestoreErr) |
| SEQ-047.9 | 改布局描述,显式 `LAYOUT_VERSION` 不变 | 不丢弃,不以描述的哈希充当键;布局兼容性由版本声明负责 |

### SEQ-048 参数版本、refdata 版本与二进制变化不使检查点失效

- 规则:[模型检查点](README.md#模型检查点)
- 级别:unit
- 里程碑:M9
- 前置:B0;s2 有效检查点 C(`last_seq` = 40 000);seq 45 000 处有 `SetParams{s2, version 9}`;重启时换了二进制 SHA 与 refdata version
- 步骤:
  1. 重启,advisor 加载 C 并续算到末尾
- 期望:
  - 没有 `ModelCheckpointDiscarded`;`model_restore` 一次;续算中镜像应用 seq 45 000 的 `SetParams` 时调 `model_on_params`;产出的 Advice 与不中断运行逐字节相同
- 实现:`crates/hft-sequencer/tests/advisor.rs::seq_048_checkpoint_survives_version_changes`

### SEQ-049 路径状态节版本不符只丢该节

- 规则:[模型检查点](README.md#模型检查点)
- 级别:unit
- 里程碑:M2
- 前置:B0;C 的路径状态节 `PATH_VERSION = 1`,二进制为 2
- 步骤:
  1. 重启,advisor 加载 C
- 期望:
  - `Note{Advisor, ModelPathReset}`:`Note.strategy = 2`、`v[0] = 1`、`v[1] = 40 000`、`v[3] = 1`、`v[4] = 2`;没有 `ModelCheckpointDiscarded`;原始样本节照常恢复
- 实现:`crates/hft-sequencer/tests/advisor.rs::seq_049_path_section_reset`

### SEQ-050 恢复检查点后以 last_seq 处的参数调一次 model_on_params,镜像追到 last_seq 不投递

- 规则:[模型检查点](README.md#模型检查点)
- 级别:unit
- 里程碑:M2
- 前置:B0;C 的 `last_seq` = 40 000;seq 30 000 处生效的是 s2 的 version 4;最近的完整快照在 seq 35 000
- 步骤:
  1. 重启,advisor 加载 C
- 期望:
  - `model_restore` 之后 `model_on_params` 恰好一次,参数为 version 4;镜像从 seq 35 000 的快照起步重放到 40 000,这期间 s2 的 Model 收到 0 个事件;从 seq 40 001 起照常投递
- 实现:`crates/hft-sequencer/tests/advisor.rs::seq_050_restore_then_params_once`

### SEQ-051 检查点写失败时本代作废、上一代保留

- 规则:[模型检查点](README.md#模型检查点)、[advisor](README.md#advisor)
- 级别:table
- 里程碑:M2
- 前置:B0;s2 已有上一代检查点 C1;本周期写 C2
- 实现:`crates/hft-sequencer/tests/advisor.rs::seq_051_checkpoint_write_failure`

| 行 | 故障 | 期望 |
|---|---|---|
| SEQ-051.1 | `commit` 返回 `CkptIoErr`(超出磁盘上限) | C1 仍是 `load` 返回的那一份;下一个 `model_checkpoint_ms` 周期再次 `begin` |
| SEQ-051.2 | 第 3 块 `write_chunk` 返回错误 | 不调 `commit`;C1 保留;advisor 照常追赶 |
| SEQ-051.3 | `model_snapshot_chunk` 写出的节体超过 `Model::snapshot_bytes` | 本代作废,C1 保留 |

### SEQ-052 记录分类与 hft-event 的 KindInfo.role 一致

- 规则:[记录分类](README.md#记录分类)
- 级别:table
- 里程碑:M5
- 前置:B0;每行构造一条该类记录,读 sequencer 写入时用的分类与 `KindInfo.role`
- 实现:`crates/hft-sequencer/tests/records.rs::seq_052_record_roles`

| 行 | 记录 | 期望分类 |
|---|---|---|
| SEQ-052.1 | `Bbo`、`L2Batch`、`L2Snapshot{origin = TopN}`、`L2Snapshot{origin = Exchange}`、`Trade`、`Funding`、`MarkPrice` | Input |
| SEQ-052.2 | `OrderEvent`、`SendReport`、`PositionReport`、`BalanceReport`、`ExternalFill`、`MarginReport`、`AccountConfigReport`、`ConnState`、`Advice` | Input |
| SEQ-052.3 | `FundingSettle{synth = 0}`(交易所推送) | Input |
| SEQ-052.4 | `FundingSettle{synth = 1}` | Output |
| SEQ-052.5 | ctl 环到达的 `ControlCmd`、`ConfigBlob`、`ReconcileReport`;`Timer`(全部 owner) | Input |
| SEQ-052.6 | 引导块中的 `ConfigBlob{boot = 1}`、`L2Snapshot{origin = Keyframe}`;`StateSnapshot`、`CmdResult`、`OrderDecision`、`ExecDecision` | Output |
| SEQ-052.7 | `Note{source ∈ Sequencer, Strategy, Risk, Oms, Ledger, Exec}` | Output |
| SEQ-052.8 | `SegmentRoll`、`LatencySample`、`Note{Measure}`、`ClockStatus`、`HostProfile` | Meta |
| SEQ-052.9 | 经输入环到达的 `Note{Venue}`、`Note{CtlAudit}`、`Note{Housekeeping}`、`Note{Advisor}`、`Note{Boot}`;`Note{InputQuarantined}` | Meta |

### SEQ-053 Meta 记录不影响状态与 Output

- 规则:[记录分类](README.md#记录分类)、[确定性规则](README.md#确定性规则)
- 级别:prop
- 里程碑:M5
- 性质:同一 Input 序列中任意位置插入任意 Meta 记录(`ClockStatus`、`Note{Housekeeping}`、`Note{Venue}`、`Note{CtlAudit}`、`HostProfile`),处理后的状态哈希与 Output 序列(按测量掩码比较)与不插入时相同
- 生成:B0 上随机 Input 序列 100–2 000 条;Meta 插入 0–200 条,位置均匀
- 参考:不插入 Meta 的同一序列
- 例数:PR 300;每夜 10 000
- 实现:`crates/hft-sequencer/tests/records.rs::seq_053_meta_does_not_affect_state`

### SEQ-054 验证回放屏蔽测量字段、不屏蔽其他字段

- 规则:[记录分类](README.md#记录分类)、[状态哈希](README.md#状态哈希)
- 级别:table
- 里程碑:M5
- 前置:B0;一段含 `OrderDecision` 与关键帧的 journal;每行改日志中一个字段后跑 `VerifyDriver`
- 实现:`crates/hft-sequencer/tests/records.rs::seq_054_measure_mask`

| 行 | 改动 | 期望 |
|---|---|---|
| SEQ-054.1 | 某条 `OrderDecision.decided_tsc` + 1 | 通过 |
| SEQ-054.2 | 某个 `SnapshotHdr.snap_tsc`、`boot_id`、`snap_utc_ns` | 通过 |
| SEQ-054.3 | 某条 `SegmentRoll`、`ClockStatus`、`HostProfile` 的任意字节 | 通过 |
| SEQ-054.4 | 某条 `OrderDecision.qty` 的最低位 | 失败,报告首个不一致记录的 seq |
| SEQ-054.5 | 某个关键帧节 10 的一个字节 | 失败 |

### SEQ-055 实盘驱动 a 段的交出顺序

- 规则:[主循环](README.md#主循环step)、[环境决定写成 Input](README.md#环境决定写成-input)
- 级别:table
- 里程碑:M2
- 前置:B0;每行在一轮开始前同时满足所列条件,记录这一轮 a 段交出的 Input 序列
- 实现:`crates/hft-sequencer/tests/preempt.rs::seq_055_round_a_order`

| 行 | 这一轮开始时的条件 | 期望 a 段交出 |
|---|---|---|
| SEQ-055.1 | 进程启动后的首轮;另有 2 个到期定时器、OMS 待发、bbo 环非空 | 只有 `EngineClock` |
| SEQ-055.2 | 首轮且有停机标记 | `EngineClock`、`ControlCmd{Kill}`(cli:systemd) |
| SEQ-055.3 | 关键帧可写(到期、上一轮空闲、环全空、`wants_continue()` 为假);另有 1 个到期定时器 | 只有 `Timer{Sys, Snapshot}`;该定时器在下一轮交出 |
| SEQ-055.4 | 背压(余量 300 < 2U = 320);另有 1 个到期定时器 | 只有 `Timer{Sys, NotifyContinue}` |
| SEQ-055.5 | 到期定时器 k1 < k2(堆键)、OMS 待发、通知积压非空、`DayRoll` 到期、`ConflateReport` 到期 | `Timer(k1)`、`Timer(k2)`、`Timer{Oms, OmsContinue}`、`Timer{Sys, NotifyContinue}`、`Timer{Sys, DayRoll}`、`Timer{Sys, ConflateReport}` |
| SEQ-055.6 | `refdata_recheck_due()` 为真、1 个到期定时器 | `Timer{Sys, RefdataRecheck}`、`Timer(k1)` |

### SEQ-056 每轮至多交出 16 个到期定时器

- 规则:[主循环](README.md#主循环step)
- 级别:unit
- 里程碑:M2
- 前置:B0 稳态;20 个 Strategy 定时器同时到期(堆键 k1 < … < k20);ctl 环 1 条、rep-A 环 1 条
- 步骤:
  1. `step()`
  2. `step()`
- 期望:
  - 步骤 1:交出 k1–k16 的 16 条 `Timer`、ctl 那 1 条;交出 rep-A 之前的中途检查发现 k17 已到期,这一轮结束,rep-A 仍在环里
  - 步骤 2:先交 k17–k20,再交 rep-A 那条
- 实现:`crates/hft-sequencer/tests/preempt.rs::seq_056_due_timers_per_round`

### SEQ-057 ctl 与 hk 每轮各至多 4 条

- 规则:[主循环](README.md#主循环step)
- 级别:unit
- 里程碑:M2
- 前置:B0 稳态;ctl 环 6 条 100 B 的命令、hk 环 6 条 `Note{Housekeeping}`
- 步骤:
  1. `step()`
- 期望:
  - 这一轮交出 ctl 4 条、hk 4 条;各剩 2 条在环里
- 实现:`crates/hft-sequencer/tests/preempt.rs::seq_057_ctl_hk_per_round`

### SEQ-058 ctl 每轮载荷字节上限,至少取一条

- 规则:[主循环](README.md#主循环step)、[环与容量](README.md#环与容量)
- 级别:table
- 里程碑:M2
- 前置:B0 稳态(`ctl_bytes_per_round = 8 192`);每行 ctl 环按所列载荷大小依次排队
- 实现:`crates/hft-sequencer/tests/preempt.rs::seq_058_ctl_bytes_per_round`

| 行 | ctl 环中的载荷字节 | 期望这一轮交出的 ctl 条数 |
|---|---|---|
| SEQ-058.1 | 4 096、4 096、4 096 | 2(合计 8 192) |
| SEQ-058.2 | 4 096、4 097 | 1 |
| SEQ-058.3 | 4 096、4 095、1 | 3(合计 8 192) |
| SEQ-058.4 | 9 000 | 1(至少一条) |
| SEQ-058.5 | 100 × 6 条 | 4(条数上限) |

### SEQ-059 实盘回报每环每轮至多 32 条、合计至多 64 条,chain-in 共用合计

- 规则:[主循环](README.md#主循环step)、[环与容量](README.md#环与容量)
- 级别:table
- 里程碑:M2
- 前置:B0 稳态;没有到期定时器;每行各环的积压条数如列
- 实现:`crates/hft-sequencer/tests/preempt.rs::seq_059_rep_round_limits`

| 行 | 积压 | 期望这一轮交出 |
|---|---|---|
| SEQ-059.1 | rep-A 40 | rep-A 32 |
| SEQ-059.2 | rep-A 40、rep-B 40 | rep-A 32、rep-B 32(合计 64) |
| SEQ-059.3 | rep-A 40、rep-B 40、rep-C 40 | 三环合计 64,每环 ≤ 32 |
| SEQ-059.4 | rep-A 30、chain-in 40 | rep-A 30、chain-in 32(chain-in 只受 `per_ring` 与合计 64 约束) |
| SEQ-059.5 | rep-A 63 后到达 1 条 ctl | ctl 不受 rep 上限影响;rep-A 32 |

### SEQ-060 bbo 每环每轮至多 32 条、合计至多 128 条

- 规则:[主循环](README.md#主循环step)
- 级别:table
- 里程碑:M2
- 前置:B0 稳态;没有到期定时器;各环积压 ≤ 64(不触发合并)
- 实现:`crates/hft-sequencer/tests/preempt.rs::seq_060_bbo_round_limits`

| 行 | 积压 | 期望这一轮交出 |
|---|---|---|
| SEQ-060.1 | bbo-A 50 | 32 |
| SEQ-060.2 | bbo-A..bbo-E 各 50(5 环) | 合计 128,每环 ≤ 32 |
| SEQ-060.3 | bbo-A 31 | 31 |

### SEQ-061 每处理完一条 Bbo 先交出新到的 ctl

- 规则:[主循环](README.md#主循环step)
- 级别:unit
- 里程碑:M2
- 前置:B0 稳态;bbo-A 积压 10 条(B1–B10);testkit 在 sequencer 处理 B3 期间向 ctl 环写一条 `Kill{Account(1)}`
- 步骤:
  1. `step()`
- 期望:
  - 这一轮 Input 顺序:B1、B2、B3、`ControlCmd{Kill}`、B4…B10;Kill 的 `CmdResult{Applied}` 在 B4 之前
- 实现:`crates/hft-sequencer/tests/preempt.rs::seq_061_ctl_preempts_bbo`

### SEQ-062 每处理完一条 Bbo 先交出新到的实盘回报

- 规则:[主循环](README.md#主循环step)
- 级别:unit
- 里程碑:M2
- 前置:B0 稳态;bbo-A 积压 10 条;处理 B3 期间 rep-B 到达 2 条 `OrderEvent`
- 步骤:
  1. `step()`
- 期望:
  - Input 顺序:B1、B2、B3、rep-B 的 2 条、B4…B10
- 实现:`crates/hft-sequencer/tests/preempt.rs::seq_062_rep_preempts_bbo`

### SEQ-063 让位取出的输入计入同一轮的上限

- 规则:[主循环](README.md#主循环step)
- 级别:unit
- 里程碑:M2
- 前置:B0 稳态;ctl 环 4 条;bbo-A 积压 10 条;处理 B2 期间 ctl 环再到 1 条
- 步骤:
  1. `step()`
  2. `step()`
- 期望:
  - 步骤 1:ctl 4 条、B1–B10;第 5 条 ctl 不在这一轮(ctl 这一轮已达 4 条)
  - 步骤 2:第 5 条 ctl 在 b 段交出
- 实现:`crates/hft-sequencer/tests/preempt.rs::seq_063_preempted_count_against_limits`

### SEQ-064 advice 每轮至多 16 条,每条之后检查 ctl 与实盘回报

- 规则:[主循环](README.md#主循环step)、[环与容量](README.md#环与容量)
- 级别:unit
- 里程碑:M2
- 前置:B0 稳态;advice 环积压 20 条(V1–V20);处理 V5 期间 rep-A 到达 1 条
- 步骤:
  1. `step()`
- 期望:
  - Input 顺序:V1–V5、rep-A 那条、V6–V16;V17–V20 留在环里
- 实现:`crates/hft-sequencer/tests/preempt.rs::seq_064_advice_limit_and_preempt`

### SEQ-065 每处理完一条 md 先交出新到的 ctl、实盘回报与 Bbo

- 规则:[主循环](README.md#主循环step)
- 级别:unit
- 里程碑:M3
- 前置:B0 稳态;`md_round_ns = 10 000 000`(本用例不让时间片截止);md-A 积压 2 000 条 20 档 `L2Batch`(M1…);处理 M5 期间 bbo-A 到 1 条,处理 M9 期间 rep-B 到 1 条,处理 M12 期间 ctl 到 1 条
- 步骤:
  1. `step()`
- 期望:
  - Input 顺序中:M5 之后紧接该 `Bbo`,再 M6;M9 之后紧接 rep-B 那条,再 M10;M12 之后紧接 ctl 那条,再 M13
- 实现:`crates/hft-sequencer/tests/preempt.rs::seq_065_bbo_rep_ctl_preempt_md`

### SEQ-066 md 时间片:已过去 ≥ md_round_ns 时不再交 md

- 规则:[主循环](README.md#主循环step)、[环与容量](README.md#环与容量)
- 级别:table
- 里程碑:M2
- 前置:B0 稳态(`md_round_ns = 50 000`);md-A 积压 100 条;手动 TSC 使每条 md 的处理恰好耗时 p ns;其他环为空
- 实现:`crates/hft-sequencer/tests/preempt.rs::seq_066_md_time_slice`

| 行 | p(ns) | 期望这一轮交出的 md 条数 |
|---|---|---|
| SEQ-066.1 | 20 000 | 3(交第 4 条前已过 60 000) |
| SEQ-066.2 | 24 999 | 3(交第 3 条前已过 49 998) |
| SEQ-066.3 | 25 000 | 2(交第 3 条前已过 50 000) |
| SEQ-066.4 | 25 001 | 2 |
| SEQ-066.5 | 60 000 | 1(第一条总被交出) |

### SEQ-067 rep-sim 排在 md 之后,每轮至多 sim_max_per_round 条,实盘环全空时仍被取出

- 规则:[主循环](README.md#主循环step)、[模拟账户](README.md#模拟账户)
- 级别:table
- 里程碑:M2
- 前置:B0 稳态(`sim_max_per_round = 32`);每行列出各环积压
- 实现:`crates/hft-sequencer/tests/preempt.rs::seq_067_rep_sim_last_and_bounded`

| 行 | 积压 | 期望这一轮 |
|---|---|---|
| SEQ-067.1 | rep-sim 1 000,其余为空 | rep-sim 32 条 |
| SEQ-067.2 | rep-sim 1 000、rep-A 10、bbo-A 10、md-A 10 | 先 rep-A 10、bbo-A 10、md-A 10,最后 rep-sim 32 |
| SEQ-067.3 | rep-sim 31 | 31 |
| SEQ-067.4 | rep-sim 33 | 32,下一轮 1 |

### SEQ-068 c–g 段每交出一条之前做定时器中途检查

- 规则:[主循环](README.md#主循环step)
- 级别:table
- 里程碑:M2
- 前置:B0 稳态;堆顶 deadline D;bbo-A 与 md-A 各积压 50 条;每行在交出第 3 条 `Bbo` 之前把 TSC 设为所列值
- 实现:`crates/hft-sequencer/tests/preempt.rs::seq_068_timer_mid_check`

| 行 | 检查时 TSC | 期望 |
|---|---|---|
| SEQ-068.1 | D − 1 | 交出第 3 条 `Bbo`,这一轮继续 |
| SEQ-068.2 | D | `take` 返回 false,这一轮结束;下一轮第一条 Input 是该 `Timer`,其记录头 tsc ≥ D |
| SEQ-068.3 | D + 1 | 同 SEQ-068.2 |

### SEQ-069 HedgeRetry 到期后最多等一条环输入就被写成 Timer

- 规则:[主循环](README.md#主循环step)
- 级别:unit
- 里程碑:M2
- 前置:B0 稳态;链接 L 的 `HedgeRetry` 在 D 到期;bbo-A 与 md-A 各积压 100 条;第 k 条 md 处理期间 TSC 越过 D
- 步骤:
  1. 连续 `step()` 直到该 `Timer{Oms, HedgeRetry}` 被交出
- 期望:
  - 该 `Timer` 之前、TSC 越过 D 之后最多只有一条环输入(第 k 条 md 本身);bbo/md 的积压未取完
- 实现:`crates/hft-sequencer/tests/preempt.rs::seq_069_hedge_retry_waits_one_input`

### SEQ-070 b 段不做定时器中途检查

- 规则:[主循环](README.md#主循环step)
- 级别:unit
- 里程碑:M2
- 前置:B0 稳态;ctl 环 3 条、rep-A 环 1 条;这一轮 a 段结束时有一个定时器恰好到期(D = 当前 TSC)
- 步骤:
  1. `step()`
- 期望:
  - 这一轮交出 ctl 3 条;rep-A 那条不交出(交出 c 段第一条之前的中途检查截止);下一轮 a 段先交该定时器,随后交 rep-A 那条
- 实现:`crates/hft-sequencer/tests/preempt.rs::seq_070_no_mid_check_in_b`

### SEQ-071 背压轮只交 NotifyContinue 与实盘回报

- 规则:[主循环](README.md#主循环step)、[通知队列与续推](README.md#通知队列与续推)
- 级别:unit
- 里程碑:M2
- 前置:B0 稳态;`NotifyBacklog` 占用 340(余量 300 < 2U = 320);ctl 1、rep-A 3、bbo-A 5、advice 1、md-A 5、rep-sim 2 条;每条 rep 产生 0 条通知
- 步骤:
  1. `step()`
  2. `step()`
- 期望:
  - 步骤 1:交出 `Timer{Sys, NotifyContinue}`(投递 64 条积压)、rep-A 3 条;ctl、bbo、advice、md、rep-sim 一条都不取
  - 步骤 2:余量 364 ≥ 320,这一轮不是背压轮,按 b–g 段依次取 ctl、bbo、advice、md、rep-sim
- 实现:`crates/hft-sequencer/tests/preempt.rs::seq_071_backpressure_round`

该调度边界用例直接准备带身份的策略 Timer 通知积压,逐条核对 Driver 交出的环及首条 NotifyContinue;实盘回报重复既有余额事实,不产生新通知。真实通知的生成、完整载荷与回放由 SEQ-220–223 覆盖,本例不把预填队列当作可回放输入。

### SEQ-072 背压的两个阈值

- 规则:[主循环](README.md#主循环step)、[通知队列与续推](README.md#通知队列与续推)
- 级别:table
- 里程碑:M2
- 前置:SEQ-071 的全部竞争环与受控积压(U = 160);rep-A 积压 3 条,每条产生 0 条通知;每行给出这一轮开始时的余量 H0。核对 64 条通知投递及首条 Timer;背压行的其他环保持原积压,非背压行全部取出
- 实现:`crates/hft-sequencer/tests/preempt.rs::seq_072_backpressure_thresholds`

| 行 | H0 | 期望 |
|---|---|---|
| SEQ-072.1 | 319 | 背压轮 |
| SEQ-072.2 | 320 | 非背压轮(按 a–g 段取全部环) |
| SEQ-072.3 | 321 | 非背压轮 |
| SEQ-072.4 | 95(NotifyContinue 后余量 159) | 背压轮;rep-A 一条都不取 |
| SEQ-072.5 | 96(NotifyContinue 后余量 160) | 背压轮;rep-A 3 条都取出 |

### SEQ-073 恢复、验证与镜像驱动每轮至多交出 replay_per_round 条并各打一次心跳

- 规则:[主循环](README.md#主循环step)、[心跳](README.md#心跳)
- 级别:unit
- 里程碑:M2
- 前置:B0(`replay_per_round = 1 024`);一段 2 500 条 Input 的 journal;`VerifyDriver`
- 步骤:
  1. 连续 `step()` 直到返回非 `Busy`
- 期望:
  - 三次 `Busy`,依次处理 1 024、1 024、452 条;心跳计数增加 3
- 实现:`crates/hft-sequencer/tests/preempt.rs::seq_073_replay_per_round`

### SEQ-074 一轮结束时 publish 尚未 publish 的完整事件

- 规则:[主循环](README.md#主循环step)、[每条 Input 的处理顺序](README.md#每条-input-的处理顺序process)
- 级别:unit
- 里程碑:M2
- 前置:B0 稳态;md-A 3 条 `Trade`(不产生出环)
- 步骤:
  1. `step()`
- 期望:
  - 返回时 `committed_len` 覆盖这 3 条 `Trade` 及其 Output;这一轮 `publish` 恰好一次
- 实现:`crates/hft-sequencer/tests/preempt.rs::seq_074_publish_at_round_end`

### SEQ-075 实盘驱动取出即交出,不先攒批

- 规则:[主循环](README.md#主循环step)、[装配与运行](README.md#装配与运行由-appshft-coreappsreplayerhft-testkit-调用)
- 级别:unit
- 里程碑:M2
- 前置:B0 稳态;bbo-A 积压 5 条(不触发合并)
- 步骤:
  1. `step()`,在每次 `take` 返回时读 bbo-A 的环头
- 期望:
  - 第 k 次 `take` 返回后 bbo-A 恰好已被消费 k 条;每条 `Bbo` 的记录头 tsc 是交出它时的 TSC,5 个值严格递增
- 实现:`crates/hft-sequencer/tests/preempt.rs::seq_075_take_one_at_a_time`

### SEQ-076 让位与停止位置不同的两次运行都能逐字节验证

- 规则:[主循环](README.md#主循环step)、[确定性规则](README.md#确定性规则)
- 级别:dst
- 里程碑:M3
- 前置:B0;同一份行情与回报到达脚本
- 故障:两次运行的处理耗时分布不同(一次 ×1、一次 ×3),md 突发 10×、bbo 突发每轮取满批
- 种子:PR 200(提交派生)+ 回归集;每夜日期派生
- 断言:两次运行的 journal 中 Input 的取出顺序、md 停止位置与 `Timer` 位置不同;各自的 `VerifyDriver` 逐字节通过;INV-6、INV-9
- 实现:`crates/hft-testkit/src/scenarios/seq.rs::seq_076_preempt_positions_verify`

### SEQ-077 实盘回报与 Bbo 的等待上界

- 规则:[主循环](README.md#主循环step)
- 级别:dst
- 里程碑:M3
- 前置:B0;md-A 积压 2 000 条 `L2Batch`、bbo-A 积压 128 条(含合并)、advice 积压 16 条
- 故障:在上述积压期间按 Poisson(每 200 µs)注入 `Bbo`、实盘回报与 ctl 命令
- 种子:PR 200 + 回归集;每夜日期派生
- 断言:每条注入的实盘回报在 journal 中的位置与其到达时刻之间最多隔一条 `Bbo`、Advice 或 md 消息;每条注入的 `Bbo` 最多隔一条 md 消息;每条 ctl 同实盘回报;取出位置与"每处理完一条 md 查看 ctl/rep/bbo 环头、每处理完一条 bbo 或 advice 查看 ctl/rep 环头"一致
- 实现:`crates/hft-testkit/src/scenarios/seq.rs::seq_077_preempt_wait_bounds`

### SEQ-078 实盘每轮 step 打一次心跳

- 规则:[心跳](README.md#心跳)
- 级别:unit
- 里程碑:M2
- 前置:B0 稳态;心跳计数 h
- 步骤:
  1. 依次执行 3 次 `step()`:第一次有 5 条输入,后两次无输入
- 期望:
  - 心跳计数 = h + 3(Busy 与 Idle 各计一次)
- 实现:`crates/hft-sequencer/tests/preempt.rs::seq_078_heartbeat_per_step`

### SEQ-079 到期的堆内定时器写成 Timer Input,记录头 tsc 不早于 deadline

- 规则:[环境决定写成 Input](README.md#环境决定写成-input)、[定时器](README.md#定时器)
- 级别:unit
- 里程碑:M2
- 前置:B0 稳态;s1 登记 `ctx.timer_at(deadline_tsc = 9 000 000, tag = 9)`(group g1)
- 步骤:
  1. TSC 推进到 9 000 500,`step()`
- 期望:
  - 交出一条 `Timer{owner = 17(16 + StrategyId 1), timer_id = 9, group = g1, deadline_tsc = 9 000 000}`,记录头 tsc = 9 000 500;s1 收到 `EventRef::Timer{tag: 9, group: g1, gix: 0}`;堆中不再有该条
- 实现:`crates/hft-sequencer/tests/env.rs::seq_079_heap_timer_to_input`

### SEQ-080 Timer 按 owner 分派

- 规则:[环境决定写成 Input](README.md#环境决定写成-input)、[每条 Input 的处理顺序](README.md#每条-input-的处理顺序process)
- 级别:table
- 里程碑:M2
- 前置:B0 稳态;venue B 的 `DeadManSpec.refresh_ms = 10 000`、`timeout_ms = 30 000`;每行交出所列 `Timer`
- 实现:`crates/hft-sequencer/tests/env.rs::seq_080_timer_owner_dispatch`

| 行 | Timer | 期望 |
|---|---|---|
| SEQ-080.1 | `{Oms, Sweep}` | `oms.on_sweep` 一次;下一条 Sweep 在 +1 000 ms 进堆 |
| SEQ-080.2 | `{Oms, HedgeRetry, link 7}` | `oms.on_hedge_retry(7, ..)` 一次;其 `OmsOut` 按排空顺序处理 |
| SEQ-080.3 | `{Oms, SmokeTtl, account 1, venue B}` | `oms.on_smoke_ttl(1, B, keep, ..)` 一次(`keep` 见 SEQ-081) |
| SEQ-080.4 | `{Exec, Wake, account 2, group g2}` | `exec.on_timer(timer_id, now)` 一次,随后按需 `exec.advance` |
| SEQ-080.5 | `{Risk, MarginDeadline, account 1, venue B}` | `risk.on_timer` 一次,其 `MarginOut` 被执行 |
| SEQ-080.6 | `{Strategy 17, tag 9, group g1}` | s1 收到 `EventRef::Timer{tag: 9, group: g1}`;其他实例收不到 |
| SEQ-080.7 | `{Sys, DeadMan, venue B, account 1}` | out-B 一条 `GwCmd{DeadManRefresh, arg: 30 000}`;下一条 DeadMan 在 +10 000 ms 进堆 |
| SEQ-080.8 | `{Sys, ConnRotate, venue B}`,账户 1 在 B 无 Pending 槽 | out-B 一条 `GwCmd{Rotate}` |
| SEQ-080.9 | 同上,账户 1 在 B 有一个 `PendingNew` 槽 | 没有 `GwCmd{Rotate}` |
| SEQ-080.10 | `{Sys, SessionKeepalive, venue B, account 1}` | out-B 一条 `GwCmd{SessionKeepalive}` |

### SEQ-081 冒烟单到期撤单的 keep 取自 Draining cap

- 规则:[环境决定写成 Input](README.md#环境决定写成-input)
- 级别:table
- 里程碑:M5
- 前置:B0 稳态;账户 1 在 B 有一张冒烟单;交出 `Timer{Oms, SmokeTtl, account 1, venue B}`
- 实现:`crates/hft-sequencer/tests/env.rs::seq_081_smoke_ttl_keep`

| 行 | 状态 | 期望 `keep` |
|---|---|---|
| SEQ-081.1 | 无 `Draining` | false |
| SEQ-081.2 | `Draining{cancel_resting = true}` | false |
| SEQ-081.3 | `Draining{cancel_resting = false}` | true |

### SEQ-082 整点滚动在第一个空闲轮写,避开资金费结算窗口

- 规则:[环境决定写成 Input](README.md#环境决定写成-input)
- 级别:table
- 里程碑:M2
- 前置:B0 稳态;venue B 订阅 b1,下次结算时刻 08:00:00Z;每行在所列 UTC 时刻出现"上一轮空闲、这一轮开始时环全空、`wants_continue()` 为假"的轮次,此前同一小时内没有写过整点滚动
- 实现:`crates/hft-sequencer/tests/env.rs::seq_082_hourly_roll_funding_window`

| 行 | 该轮 UTC | 期望 |
|---|---|---|
| SEQ-082.1 | 05:00:00.500 | a 段写 `Timer{Sys, Roll}`(timer_id 2) |
| SEQ-082.2 | 05:00:00.500,但上一轮不空闲 | 不写;等下一个满足条件的轮次 |
| SEQ-082.3 | 08:00:00.100 | 不写 |
| SEQ-082.4 | 08:01:59.999 | 不写 |
| SEQ-082.5 | 08:02:00.000 | 写 |
| SEQ-082.6 | 07:57:59.999(07:00 整点尚未滚动) | 写 |
| SEQ-082.7 | 07:58:00.000(07:00 整点尚未滚动) | 不写;推迟到 08:02:00.000 之后的第一个空闲轮 |

### SEQ-083 段用量触发的两级滚动

- 规则:[环境决定写成 Input](README.md#环境决定写成-input)、[journal 满](README.md#journal-满)
- 级别:table
- 里程碑:M2
- 前置:B0;journal `size_idle_pct = 75`、`size_valve_pct = 90`;每行的 `needs_roll` 返回值与轮次状态如列
- 实现:`crates/hft-sequencer/tests/env.rs::seq_083_size_roll_two_levels`

| 行 | `needs_roll` 与状态 | 期望 |
|---|---|---|
| SEQ-083.1 | `(Size, IdleRound)`,持续不空闲 | 不写 `Timer{Sys, Roll}` |
| SEQ-083.2 | `(Size, IdleRound)`,出现空闲轮 | 下一轮 a 段写 `Timer{Sys, Roll}` |
| SEQ-083.3 | `(Size, IdleRound)`,空闲轮落在结算前后 2 min 窗口内 | 照常写(段用量触发不避开结算窗口) |
| SEQ-083.4 | `(Size, EventBoundary)`,持续不空闲 | 下一轮 a 段写,不等空闲轮 |
| SEQ-083.5 | `(Size, EventBoundary)`,在结算窗口内 | 下一轮 a 段写 |
| SEQ-083.6 | `None` | 不写 |

### SEQ-084 续推未完时不写 Roll

- 规则:[环境决定写成 Input](README.md#环境决定写成-input)、[通知队列与续推](README.md#通知队列与续推)
- 级别:unit
- 里程碑:M5
- 前置:B0 稳态;`needs_roll = (Size, EventBoundary)`;OMS 待发队列尚余 20 条(K = 16)
- 步骤:
  1. 连续 `step()` 直到出现 `Timer{Sys, Roll}`
- 期望:
  - 先后出现两条 `Timer{Oms, OmsContinue}`(16 条、4 条);`Timer{Sys, Roll}` 只在第二条之后、`wants_continue()` 变假后的那一轮出现
- 实现:`crates/hft-sequencer/tests/env.rs::seq_084_roll_waits_continue`

### SEQ-085 关键帧到期后只在空闲轮之后的那一轮作为唯一 Input 写

- 规则:[环境决定写成 Input](README.md#环境决定写成-input)、[快照](README.md#快照)
- 级别:table
- 里程碑:M2
- 前置:B0 稳态;关键帧已到期;每行描述这一轮开始时的状态
- 实现:`crates/hft-sequencer/tests/keyframe.rs::seq_085_keyframe_round_conditions`

| 行 | 这一轮开始时 | 期望 |
|---|---|---|
| SEQ-085.1 | 上一轮空闲、环全空、`wants_continue()` 为假 | 这一轮只交 `Timer{Sys, Snapshot}`(timer_id 1)一条 Input |
| SEQ-085.2 | 上一轮空闲,bbo-A 有 1 条 | 不写;这一轮照常取 bbo-A |
| SEQ-085.3 | 上一轮 `Busy`,环全空 | 不写 |
| SEQ-085.4 | 到期时通知积压非空(其间每轮 a 段交 `NotifyContinue`,没有空闲轮) | 积压清空之前不写;清空后出现空闲轮,其下一轮才写 |
| SEQ-085.5 | 到期时 OMS 待发非空 | 待发发完之前不写;之后出现空闲轮,其下一轮才写 |
| SEQ-085.6 | 未到期、上一轮空闲、环全空 | 不写,返回 `Idle` |

### SEQ-086 关键帧的时间触发边界

- 规则:[环境决定写成 Input](README.md#环境决定写成-input)、[环与容量](README.md#环与容量)
- 级别:table
- 里程碑:M2
- 前置:B0(`keyframe_period_ms = 300 000`,1 GHz);上一个关键帧的 `Timer{Sys, Snapshot}` 记录头 tsc = T0;其后 Input 数 < `keyframe_max_events`;每行在所列 TSC 出现满足 SEQ-085.1 的轮次
- 实现:`crates/hft-sequencer/tests/keyframe.rs::seq_086_time_trigger_boundary`

| 行 | 轮次开始 TSC | 期望 |
|---|---|---|
| SEQ-086.1 | T0 + 299 999 000 000 | 不写 |
| SEQ-086.2 | T0 + 300 000 000 000 | 写 |
| SEQ-086.3 | T0 + 300 001 000 000 | 写 |

### SEQ-087 关键帧的事件数触发边界

- 规则:[环境决定写成 Input](README.md#环境决定写成-input)、[环与容量](README.md#环与容量)
- 级别:table
- 里程碑:M2
- 前置:B0 改 `keyframe_max_events = 1 000`;距上次关键帧不到 `keyframe_period_ms`;每行在上次关键帧之后已有所列条数的 Input 时出现满足 SEQ-085.1 的轮次
- 实现:`crates/hft-sequencer/tests/keyframe.rs::seq_087_event_count_trigger_boundary`

| 行 | 上次关键帧之后的 Input 数 | 期望 |
|---|---|---|
| SEQ-087.1 | 999 | 不写 |
| SEQ-087.2 | 1 000 | 写 |
| SEQ-087.3 | 1 001 | 写 |

### SEQ-088 关键帧到期后持续不空闲 2 个周期时在事件边界强制写

- 规则:[环境决定写成 Input](README.md#环境决定写成-input)、[快照](README.md#快照)
- 级别:table
- 里程碑:M2
- 前置:B0 改 `keyframe_period_ms = 1 000`;关键帧在 TSC T 到期;之后每一轮都有输入(没有空闲轮),bbo-A 始终非空;每行描述轮次开始时的状态
- 实现:`crates/hft-sequencer/tests/keyframe.rs::seq_088_max_deferral`

| 行 | 轮次开始时 | 期望 |
|---|---|---|
| SEQ-088.1 | TSC = T + 1 999 000 000,`wants_continue()` 为假 | 不写 |
| SEQ-088.2 | TSC = T + 2 000 000 000,`wants_continue()` 为假 | 这一轮只交 `Timer{Sys, Snapshot}`,bbo-A 留到下一轮 |
| SEQ-088.3 | TSC = T + 2 000 000 000,`wants_continue()` 为真 | 这一轮交 `OmsContinue` 等,不写;`wants_continue()` 变假后的那一轮只交 `Timer{Sys, Snapshot}` |
| SEQ-088.4 | SEQ-088.2 写出后,`VerifyDriver` 重放 | 在同一位置读到该 `Timer` 并逐字节生成同一关键帧 |

### SEQ-089 每个 Live 簿在每个关键帧周期内恰好一条 L2 关键帧

- 规则:[环境决定写成 Input](README.md#环境决定写成-input)、[快照](README.md#快照)
- 级别:unit
- 里程碑:M9
- 前置:B0 改 `keyframe_period_ms = 1 000`;4 个 inst 的簿为 Live,第 5 个为 `Resyncing`;运行 3 000 ms,每 5 ms 一条 bbo
- 步骤:
  1. 统计每个 [k × 1 000 ms, (k + 1) × 1 000 ms) 窗口内的 `Timer{Keyframe, inst}` 与 `L2Snapshot{origin = Keyframe}`
- 期望:
  - 每个窗口内 4 个 Live 簿各恰好 1 条 `Timer{Keyframe, inst}`(owner 2,timer_id = InstId)及紧随的 1 条 `L2Snapshot{Keyframe}`;`Resyncing` 的簿 0 条;同一窗口的 4 条不在同一轮内全部交出
- 实现:`crates/hft-sequencer/tests/keyframe.rs::seq_089_l2_keyframe_amortized`

### SEQ-090 SnapshotNow 被 Applied 后在第一个空闲轮写按需快照

- 规则:[环境决定写成 Input](README.md#环境决定写成-input)、[快照](README.md#快照)
- 级别:unit
- 里程碑:M2
- 前置:B0 稳态;bbo-A 持续 3 轮有输入
- 步骤:
  1. 投递 `SnapshotNow` 命令
  2. 连续 `step()`,从第 4 次 `step()` 起环为空
- 期望:
  - 步骤 1:`CmdResult{Applied}`;`view().snapshot_requested()` 为真
  - 步骤 2:前 3 轮不写;第一个空闲轮之后的那一轮 a 段交 `Timer{Sys, SnapshotNow}`(timer_id 7),同一事件内一条 `StateSnapshot{purpose = OnDemand}`;之后 `snapshot_requested()` 为假
- 实现:`crates/hft-sequencer/tests/env.rs::seq_090_snapshot_now`

### SEQ-091 DayRoll 的写入与幂等

- 规则:[环境决定写成 Input](README.md#环境决定写成-input)
- 级别:table
- 里程碑:M2
- 前置:B0 稳态;节 20 `last_day_rolled = 20727`(2026-10-01);每行从 B0 稳态开始
- 实现:`crates/hft-sequencer/tests/env.rs::seq_091_day_roll`

| 行 | 动作 | 期望 |
|---|---|---|
| SEQ-091.1 | UTC 到 2026-10-02T00:00:00.001Z,时钟可信 | a 段写 `Timer{Sys, DayRoll}`(timer_id = 3 \| 20728 << 8 = 5 306 371);处理时依次 `risk.on_day_roll(20728, ..)`、`ledger.on_day_roll(now.engine_ns)`;`last_day_rolled = 20728` |
| SEQ-091.2 | UTC 到 2026-10-02T00:00:00.001Z,最近一条为 `Timer{ClockBad}` | 不写 |
| SEQ-091.3 | 再交出一条 day 20728 的 `Timer{DayRoll}` | 不调 risk 与 ledger;状态哈希不变 |
| SEQ-091.4 | 交出 day 20726 的 `Timer{DayRoll}` | 同 SEQ-091.3 |
| SEQ-091.5 | 停机跨两日后重启(UTC 为 20729 日),时钟可信 | `EngineClock` 之后的第一个 a 段写 day 20729 的 `DayRoll`(不补写 20728) |

### SEQ-092 FundingSettle 定时在 UTC 到达结算时刻时写

- 规则:[环境决定写成 Input](README.md#环境决定写成-input)、[资金费合成](README.md#资金费合成)
- 级别:table
- 里程碑:M9
- 前置:B0 稳态;venue B 的 `VenueCaps.funding_settle = RateOnly`;账户 1 持有 b1 空 0.010;`next_funding_settle() = (B, 08:00:00Z)`
- 实现:`crates/hft-sequencer/tests/env.rs::seq_092_funding_settle_timer`

| 行 | 状态 | 期望 |
|---|---|---|
| SEQ-092.1 | UTC 07:59:59.999,时钟可信 | 不写 |
| SEQ-092.2 | UTC 08:00:00.000,时钟可信 | 写 `Timer{Sys, FundingSettle}`(timer_id = 15 \| 2 << 8 = 527) |
| SEQ-092.3 | UTC 08:00:00.000,时钟不可信 | 不写;时钟恢复可信的那一轮写 |
| SEQ-092.4 | 账户 1 无 b1 持仓 | `next_funding_settle() = None`,不写 |

### SEQ-093 存储定时器翻成 ModeInput::Storage

- 规则:[环境决定写成 Input](README.md#环境决定写成-input)、[存储降级](README.md#存储降级)
- 级别:table
- 里程碑:M2
- 前置:B0 稳态;每行交出所列 `Timer`
- 实现:`crates/hft-sequencer/tests/env.rs::seq_093_storage_timers`

| 行 | Timer(timer_id) | 期望 `risk.mode.apply` 的输入 |
|---|---|---|
| SEQ-093.1 | `PersistLag` level 1(268) | `Storage{kind: PersistLag, level: 1}` |
| SEQ-093.2 | `PersistLag` level 2(524) | `Storage{kind: PersistLag, level: 2}` |
| SEQ-093.3 | `DiskLow` level 1(269) | `Storage{kind: DiskLow, level: 1}` |
| SEQ-093.4 | `StorageOk`(14) | `Storage{kind: Both, level: 0}` |

### SEQ-094 KillSelfTestOk 翻成闸门输入,Meta KillSelfTest 不影响状态

- 规则:[环境决定写成 Input](README.md#环境决定写成-input)、[启动闸门](README.md#启动闸门)
- 级别:unit
- 里程碑:M2
- 前置:B0,账户 1 的启动闸门未放行
- 步骤:
  1. hk 环送入 Meta `Note{Boot, KillSelfTest}`(account 1)
  2. 交出 `Timer{Sys, KillSelfTestOk}`(timer_id = 16 | 1 << 16 = 65 552)
- 期望:
  - 步骤 1 后闸门位不变,无 `Note{StartupGate}`
  - 步骤 2:`risk.mode.apply(Gate(KillSelfTest{account: 1}))` 一次;闸门位变化时同一事件内一条 `Note{StartupGate}`(`v[0] = 1`)
- 实现:`crates/hft-sequencer/tests/env.rs::seq_094_kill_self_test_ok`

### SEQ-095 ConflateReport 与 AdvisorTick 每 1 s 一条

- 规则:[环境决定写成 Input](README.md#环境决定写成-input)、[BBO 合并与 CONFLATED](README.md#bbo-合并与-conflated)
- 级别:unit
- 里程碑:M2
- 前置:B0 稳态;运行 3.5 s;第 1 秒内合并丢弃 30 条 `Bbo`
- 步骤:
  1. 收集 `Timer{Sys, ConflateReport}`(timer_id 10)、`Timer{Sys, AdvisorTick}`(timer_id 6)及其 Output
- 期望:
  - 各 3 条;第一条 `ConflateReport` 的事件内 `Note{Sequencer, Conflated}` 的计数为 30;`AdvisorTick` 事件内没有 Output
- 实现:`crates/hft-sequencer/tests/env.rs::seq_095_conflate_report_and_advisor_tick`

### SEQ-096 恢复、验证与镜像驱动从不生成环境决定

- 规则:[环境决定写成 Input](README.md#环境决定写成-input)
- 级别:unit
- 里程碑:M2
- 前置:B0;一段 journal,其中处理期间多次越过整点、关键帧到期与定时器到期,但日志里只有 2 条 `Timer{Sys, Roll}` 与 1 条 `Timer{Sys, Snapshot}`
- 步骤:
  1. 分别以 `RecoverDriver`、`VerifyDriver`、`MirrorDriver` 重放
- 期望:
  - 三种驱动交出的 `Timer` 序列与日志中的逐条相同;没有任何额外的 `Timer` 或 `EngineClock`
- 实现:`crates/hft-sequencer/tests/env.rs::seq_096_replay_drivers_no_env_inputs`

### SEQ-097 不同空闲模式下环境决定位置不同,但都可逐字节验证

- 规则:[环境决定写成 Input](README.md#环境决定写成-input)、[确定性规则](README.md#确定性规则)
- 级别:unit
- 里程碑:M3
- 前置:B0;同一输入到达脚本跑两次:第一次每 50 ms 有一个空闲轮,第二次只在 1 s 处有空闲轮;两次都越过一个整点并到期一次关键帧
- 步骤:
  1. 两次运行各自 `VerifyDriver` 重放
- 期望:
  - 两次运行中 `Timer{Sys, Roll}`、`Timer{Sys, Snapshot}` 的 seq 不同;两次各自验证逐字节通过;`SegmentRoll` 不参与比对
- 实现:`crates/hft-sequencer/tests/env.rs::seq_097_env_positions_differ_verify_passes`

### SEQ-098 ms 与 TSC 的换算取当前段头的 tsc_hz_milli

- 规则:[环境决定写成 Input](README.md#环境决定写成-input)
- 级别:unit
- 里程碑:M2
- 前置:B0;当前段头 `tsc_hz_milli = 2 000 000 000 000`(2 GHz);s1 用 `Now::tsc_of` 登记 500 ms 后的定时器
- 步骤:
  1. 读该定时器的 `deadline_tsc`;`VerifyDriver` 在另一台 3 GHz 主机上重放同一 journal
- 期望:
  - `deadline_tsc = 登记时 tsc + 1 000 000 000`;重放得到同一值(取段头,不取主机频率)
- 实现:`crates/hft-sequencer/tests/env.rs::seq_098_ms_tsc_from_segment_header`

### SEQ-099 BBO 积压超过阈值时合并,只留每个 inst 的最后一条

- 规则:[BBO 合并与 CONFLATED](README.md#bbo-合并与-conflated)
- 级别:unit
- 里程碑:M2
- 前置:B0 稳态(`conflate_backlog = 64`);bbo-A 积压 70 条:前 32 条中 a1 20 条、a2 11 条、inst `a3`(`InstId 13`,A 上另一现货)1 条
- 步骤:
  1. `step()`;1 s 后的 `ConflateReport`
- 期望:
  - 前 32 条交出 3 条 `Bbo`:a1 的第 20 条与 a2 的第 11 条带 `CONFLATED`,a3 那条不带;其余 29 条不在 journal 中;剩余 38 条积压(≤ 64)按普通方式逐条交出
  - `Note{Conflated}` 的计数增加 29
- 实现:`crates/hft-sequencer/tests/conflate.rs::seq_099_conflate_keeps_last`

### SEQ-100 合并只在积压大于 conflate_backlog 时启用

- 规则:[BBO 合并与 CONFLATED](README.md#bbo-合并与-conflated)、[环与容量](README.md#环与容量)
- 级别:table
- 里程碑:M2
- 前置:B0 稳态(`conflate_backlog = 64`);bbo-A 中全部为 a1
- 实现:`crates/hft-sequencer/tests/conflate.rs::seq_100_conflate_threshold`

| 行 | bbo-A 积压 | 期望 |
|---|---|---|
| SEQ-100.1 | 63 | 不合并;这一轮 32 条逐条进 journal |
| SEQ-100.2 | 64 | 不合并 |
| SEQ-100.3 | 65 | 合并:一次取 32 条,交出 1 条带 `CONFLATED` 的 a1 |

### SEQ-101 合并后交出的各条之间照常做环头检查与定时器中途检查

- 规则:[BBO 合并与 CONFLATED](README.md#bbo-合并与-conflated)、[主循环](README.md#主循环step)
- 级别:unit
- 里程碑:M2
- 前置:B0 稳态;bbo-A 积压 70 条,合并后交出 a1、a2、a3 三条;处理第一条期间 ctl 环到 1 条
- 步骤:
  1. `step()`
- 期望:
  - Input 顺序:a1 的 `Bbo`、ctl 那条、a2 的 `Bbo`、a3 的 `Bbo`
- 实现:`crates/hft-sequencer/tests/conflate.rs::seq_101_checks_between_conflated`

### SEQ-102 深度与成交从不合并

- 规则:[BBO 合并与 CONFLATED](README.md#bbo-合并与-conflated)
- 级别:unit
- 里程碑:M2
- 前置:B0 稳态;md-A 积压 10 000 条(`Trade` 与 `L2Batch` 交替,同一 inst)
- 步骤:
  1. 连续 `step()` 直到 md-A 为空
- 期望:
  - journal 中 md 记录恰好 10 000 条,顺序与入环顺序相同,没有一条带 `CONFLATED`
- 实现:`crates/hft-sequencer/tests/conflate.rs::seq_102_md_never_conflated`

### SEQ-103 被合并丢弃的 Bbo 对任何状态不可见

- 规则:[BBO 合并与 CONFLATED](README.md#bbo-合并与-conflated)
- 级别:unit
- 里程碑:M2
- 前置:B0 稳态;运行 R1:bbo-A 积压 70 条触发合并;运行 R2:只向 bbo-A 送 R1 实际交出的那些 `Bbo`(同 `CONFLATED` 位)
- 步骤:
  1. 比较两次运行处理完后的状态哈希与 Output
- 期望:
  - 状态哈希相同;Output 逐字节相同(`Note{Conflated}` 计数除外,R2 为 0)
- 实现:`crates/hft-sequencer/tests/conflate.rs::seq_103_dropped_bbo_invisible`

### SEQ-104 处理 EngineClock 设基准、清空本次运行状态并写 EpochStart

- 规则:[引擎时间](README.md#引擎时间engine_ns)、[每条 Input 的处理顺序](README.md#每条-input-的处理顺序process)
- 级别:unit
- 里程碑:M2
- 前置:B0;从快照恢复,快照的节 26 有 3 行、a1 的 `bbo_member_mask = 0b11`、`bbo_flags = RESYNC`、Viewer 预览表 2 条;`engine_epoch = 7`
- 步骤:
  1. 交出 `EngineClock{base_engine_ns = 5 000 000 000, tsc_hz_milli = 10^12}`,记录头 tsc = 1 000 000,seq = 100
- 期望:
  - 节 25 = `{base_engine_ns: 5 000 000 000, base_tsc: 1 000 000, tsc_hz_milli: 10^12, clock_seq: 100}`
  - 同一事件内 `Note{Sequencer, EpochStart}`,seq 101,`v[0] = 7`、`v[1] = 5 000 000 000`、`v[2] = 100`,带 `EVENT_END`;`Now.boot_seq = 101`
  - 节 26 为空;a1 的 `bbo_member_mask = 0`、`bbo_flags & RESYNC = 0`;Viewer 预览表为空
  - `ledger.on_run_start(5 000 000 000)` 与 `risk.set_run_start(5 000 000 000)` 各一次
- 实现:`crates/hft-sequencer/tests/clock.rs::seq_104_engine_clock_processing`

### SEQ-105 engine_ns 的换算结果

- 规则:[引擎时间](README.md#引擎时间engine_ns)
- 级别:table
- 里程碑:M2
- 前置:`base_engine_ns = 1 000`,`base_tsc = 0`;每行给 H(`tsc_hz_milli`)与 Δ
- 实现:`crates/hft-sequencer/tests/clock.rs::seq_105_engine_ns_values`

| 行 | H | Δ | 期望 `engine_ns` |
|---|---|---|---|
| SEQ-105.1 | 10^12(1 GHz) | 0 | 1 000 |
| SEQ-105.2 | 10^12 | 1 | 1 001 |
| SEQ-105.3 | 3 × 10^12 | 10 | 1 003 |
| SEQ-105.4 | 3 × 10^12 | 3 | 1 001 |
| SEQ-105.5 | 1.05 × 10^12 | 105 | 1 100 |
| SEQ-105.6 | 1.05 × 10^12 | 104 | 1 099 |
| SEQ-105.7 | 8 × 10^12 | 2^58 − 1 | 1 000 + 36 028 797 018 963 967 |
| SEQ-105.8 | 15 625 000 001 | 1 | 1 063 |

### SEQ-106 乘数-移位实现与定义式逐位相同

- 规则:[引擎时间](README.md#引擎时间engine_ns)
- 级别:prop
- 里程碑:M2
- 性质:`q' + 校正` = ⌊Δ × 10^12 / H⌋(u128 精确计算),对全部生成值逐位相等
- 生成:Δ ∈ {0, 1, 2^58 − 1} ∪ 均匀 [0, 2^58);H ∈ {15 625 000 001, 15 625 000 002, 10^12, 1.05 × 10^12, 8 × 10^12} ∪ 均匀 [15 625 000 001, 8 × 10^12]
- 参考:u128 定义式
- 例数:PR 100 000;每夜 10 000 000
- 实现:`crates/hft-sequencer/tests/clock.rs::seq_106_mult_shift_exact`

### SEQ-107 超出换算范围时 Fatal(ClockRange)

- 规则:[引擎时间](README.md#引擎时间engine_ns)、[装配与运行](README.md#装配与运行由-appshft-coreappsreplayerhft-testkit-调用)
- 级别:table
- 里程碑:M2
- 前置:B0;每行交出所列 `EngineClock` 或之后的 Input
- 实现:`crates/hft-sequencer/tests/clock.rs::seq_107_clock_range`

| 行 | 输入 | 期望 `step` 结果 |
|---|---|---|
| SEQ-107.1 | `EngineClock{tsc_hz_milli = 15 625 000 000}` | `Fatal(ClockRange)`(`mult = 2^64`) |
| SEQ-107.2 | `EngineClock{tsc_hz_milli = 15 625 000 001}` | `Busy` |
| SEQ-107.3 | `EngineClock{tsc_hz_milli = 15 624 999 999}` | `Fatal(ClockRange)` |
| SEQ-107.4 | 1 GHz 基准后,Δ = 2^58 − 1 的一条 `Bbo` | `Busy` |
| SEQ-107.5 | 1 GHz 基准后,Δ = 2^58 的一条 `Bbo` | `Fatal(ClockRange)` |
| SEQ-107.6 | 记录头 tsc = `base_tsc − 1` 的一条 `Bbo` | `Fatal(ClockRange)` |

### SEQ-108 stop_engine_ns 的取值

- 规则:[引擎时间](README.md#引擎时间engine_ns)、[冷启动恢复](README.md#冷启动恢复recoverdriver)
- 级别:table
- 里程碑:M2
- 前置:B0;每行 boot 后读 `view().stop_engine_ns()`;第 2–4 行还核对 Recovery 快照头的 engine_ns。第 4 行在目标快照之后保留一个更晚的关键帧,确保确实选择旧快照
- 补充步骤:第 2–4 行在恢复引导块写完、尚未消费新 EngineClock 时再次崩溃恢复;新 TSC = 7、频率为原来的两倍。两次恢复快照及新 EngineClock.base_engine_ns 均保持表中时间,推进 2 000 000 tick 后的快照增加 1 000 000 ns,从第二次恢复快照逐字节回放通过
- 实现:`crates/hft-sequencer/tests/clock.rs::seq_108_stop_engine_ns`

| 行 | 启动方式 | 期望 |
|---|---|---|
| SEQ-108.1 | `Empty{configs}` | 0 |
| SEQ-108.2 | `Journal{start: Latest, replay_tail: true}`,最后一条已应用记录的 engine_ns = 123 456 789 | 123 456 789 |
| SEQ-108.3 | `Journal{start: Latest, replay_tail: false}`,所选快照 `SnapshotHdr.engine_ns = 100 000` | 100 000 |
| SEQ-108.4 | `Journal{start: AtSeq(s), replay_tail: false}`(`--recover-from-snapshot`),该快照 `engine_ns = 42 000` | 42 000 |

### SEQ-109 快照头的 engine_ns 是 at_seq 处的引擎时间

- 规则:[引擎时间](README.md#引擎时间engine_ns)、[快照](README.md#快照)
- 级别:unit
- 里程碑:M2
- 前置:B0 稳态;`base_engine_ns = 0`、`base_tsc = 1 000 000`,1 GHz
- 步骤:
  1. 在记录头 tsc = 6 000 000 的 `Timer{Sys, SnapshotNow}` 处写按需快照
- 期望:
  - `SnapshotHdr.at_seq` = 该 `Timer` 的 seq;`SnapshotHdr.engine_ns` = 该 `Timer` 的 engine_ns = 5 000 000
- 实现:`crates/hft-sequencer/tests/clock.rs::seq_109_snapshot_engine_ns`

### SEQ-110 停机时间不计入引擎时间,同开机、跨开机与换机都连续

- 规则:[引擎时间](README.md#引擎时间engine_ns)
- 级别:table
- 里程碑:M2
- 前置:B0;上次运行最后一条已应用记录 engine_ns = X = 7 200 000 000 000(2 h);停机 10 min;新运行 `EngineClock.base_engine_ns = stop_engine_ns()`
- 实现:`crates/hft-sequencer/tests/clock.rs::seq_110_engine_ns_continuous`

| 行 | 新运行 | 期望:`EngineClock` 之后经过所列 TSC 的 Input 的 engine_ns |
|---|---|---|
| SEQ-110.1 | 同开机,1 GHz,TSC 接着增长 | +10^9 tick → X + 10^9 |
| SEQ-110.2 | 跨开机,1 GHz,TSC 从 5 000 起 | +10^9 tick → X + 10^9 |
| SEQ-110.3 | 换机,3 GHz | +3 × 10^9 tick → X + 10^9 |

### SEQ-111 从中途快照起步时用节 25 的基准

- 规则:[引擎时间](README.md#引擎时间engine_ns)
- 级别:unit
- 里程碑:M2
- 前置:B0;一段 journal,`EngineClock` 在 seq 100,关键帧在 seq 50 000
- 步骤:
  1. 连续运行时记录 seq 50 001–60 000 每条 Input 的 `Now.engine_ns`
  2. `VerifyDriver` 从 seq 50 000 的关键帧起步重放,记录同一区间
- 期望:
  - 两组 engine_ns 逐条相同;起步时 `E` 等于关键帧节 25
- 实现:`crates/hft-sequencer/tests/clock.rs::seq_111_start_from_snapshot_uses_section25`

### SEQ-112 运行中的段滚动不改引擎时钟基准

- 规则:[引擎时间](README.md#引擎时间engine_ns)、[快照](README.md#快照)
- 级别:unit
- 里程碑:M2
- 前置:B0 稳态;`E = {0, 1 000 000, 10^12, 100}`
- 步骤:
  1. 交出 `Timer{Sys, Roll}`,读新段引导块 `StateSnapshot{Boot}` 的节 25;再交出一条 `Bbo`
- 期望:
  - 节 25 = `{0, 1 000 000, 10^12, 100}`;新段没有 `EngineClock`;`Bbo` 的 engine_ns = 记录头 tsc − 1 000 000
- 实现:`crates/hft-sequencer/tests/clock.rs::seq_112_roll_keeps_base`

### SEQ-113 BBO 连接上各 Input 对连接活性的作用

- 规则:[连接活性](README.md#连接活性)
- 级别:table
- 里程碑:M2
- 前置:B0 稳态;venue A:槽 0、槽 1 都 `up = true`、`resync = false`、`live_tsc = 1 000`;a1 是槽 0 与槽 1 的成员(`bbo_member_mask = 0b11`),a2 只是槽 0 的成员;每行从此状态开始投递一条 Input
- 实现:`crates/hft-sequencer/tests/liveness.rs::seq_113_conn_live_transitions`

| 行 | Input | 期望 |
|---|---|---|
| SEQ-113.1 | `Bbo{a1, conn 0, rx_tsc 5 000}`(被应用) | 槽 0 `live_tsc = 5 000`;a1 的 `RESYNC` 清除;记 `bbo_hw_rx_ns` |
| SEQ-113.2 | `Bbo{a1, conn 0, rx_tsc 800}` | 槽 0 `live_tsc` 仍为 1 000 |
| SEQ-113.3 | `Bbo{a3, conn 0}`(a3 原非成员) | a3 的 `bbo_member_mask` bit 0 置位 |
| SEQ-113.4 | `ConnState{Up, inst = 0, conn 2, rx_tsc 6 000}` | 槽 2:`up = true`、`resync = false`、`live_tsc = 6 000` |
| SEQ-113.5 | `ConnState{Up, inst = a3, conn 1, rx_tsc 7 000}` | a3 bit 1 置位;槽 1 `live_tsc = 7 000` |
| SEQ-113.6 | `ConnState{Down, inst = a1, conn 1}` | a1 的 mask = 0b01;槽 1 `up` 仍为 true |
| SEQ-113.7 | `ConnState{Down, inst = 0, conn 0}` | 槽 0 `up = false`;a1 的 mask = 0b10,a2 的 mask = 0 |
| SEQ-113.8 | `ConnState{Alive, conn 0, rx_tsc 9 000}` | 槽 0 `live_tsc = 9 000`;没有任何 `EventRef::Conn` 入队 |
| SEQ-113.9 | `ConnState{Resync, inst = 0, conn 0}`,随后 `Bbo{a1, conn 0}` | 槽 0 `resync = true`,`Bbo` 不清它;再来 `ConnState{Up, inst = 0, conn 0}` 后 `resync = false` |
| SEQ-113.10 | `ConnState{Resync, inst = a1, BBO_REFRESH, conn 0}` | a1 `bbo_flags & RESYNC = 1`;a2 不变;a1 的下一条被应用的 `Bbo` 清除它 |
| SEQ-113.11 | `ConnState{Rotated, conn 0}` | 节 26 与各 inst 的成员位、`RESYNC` 不变 |
| SEQ-113.12 | `ConnState{Degraded, conn 0}` | 同 SEQ-113.11 |

### SEQ-114 BboView 的连接健康按成员槽聚合

- 规则:[连接活性](README.md#连接活性)
- 级别:table
- 里程碑:M2
- 前置:B0 稳态;a1 的簿为 Live、`bbo_flags & RESYNC = 0`(除非另列);每行设定 a1 的成员槽状态后读 `bbo(a1)`
- 实现:`crates/hft-sequencer/tests/liveness.rs::seq_114_bbo_view_projection`

| 行 | 成员槽状态 | 期望 `conn_up` / `conn_live_tsc` / `resync` |
|---|---|---|
| SEQ-114.1 | 槽 0 up(live 5 000)、槽 1 up(live 7 000) | true / 7 000 / false |
| SEQ-114.2 | 槽 0 up + resync(live 9 000)、槽 1 up(live 7 000) | true / 7 000 / false |
| SEQ-114.3 | 槽 0 up + resync(9 000)、槽 1 up + resync(7 000) | true / 9 000 / true |
| SEQ-114.4 | 槽 0 是成员但 `up = false`(live 9 000)、槽 1 up(7 000) | true / 7 000 / false |
| SEQ-114.5 | 两个成员都 `up = false` | false / 0 / true |
| SEQ-114.6 | 同 SEQ-114.1,a1 的 `RESYNC` 置位 | true / 7 000 / true |
| SEQ-114.7 | 同 SEQ-114.1,a1 的簿处于 `Resyncing` | true / 7 000 / true |
| SEQ-114.8 | 没有任何成员 | false / 0 / true |

### SEQ-115 繁忙连接上的安静 inst 随连接的存活证据保持健康

- 规则:[连接活性](README.md#连接活性)
- 级别:unit
- 里程碑:M2
- 前置:B0 稳态;a1、a2 都只是槽 0 的成员;10 s 内只有 a2 的 `Bbo`(每 100 ms 一条),a1 没有
- 步骤:
  1. 每条 a2 的 `Bbo` 之后读 `bbo(a1).conn_live_tsc`
- 期望:
  - 每次读到的值等于刚处理的 a2 `Bbo` 的 `MarketHdr.rx_tsc`(单调前移);`resync = false`
- 实现:`crates/hft-sequencer/tests/liveness.rs::seq_115_quiet_inst_on_busy_conn`

### SEQ-116 安静行情下 Alive 使连接保持健康且不投递给策略

- 规则:[连接活性](README.md#连接活性)
- 级别:unit
- 里程碑:M2
- 前置:B0 稳态;a1 是槽 0 的成员;10 s 内没有任何 `Bbo`,槽 0 每 1 s 一条 `ConnState{Alive}`
- 步骤:
  1. 第 10 条 `Alive` 之后读 `bbo(a1)`
- 期望:
  - `conn_live_tsc` = 第 10 条 `Alive` 的 `rx_tsc`;10 条 `Alive` 都没有使任何实例收到 `EventRef::Conn`;报价(Top)未变
- 实现:`crates/hft-sequencer/tests/liveness.rs::seq_116_alive_keeps_quiet_market_live`

### SEQ-117 赛马胜出连接退役后,槽复用不继承成员关系

- 规则:[连接活性](README.md#连接活性)
- 级别:unit
- 里程碑:M2
- 前置:B0 稳态;a1 是槽 0 与槽 1 的成员,两槽都 up
- 步骤:
  1. `ConnState{Down, inst = 0, conn 0}`
  2. 新连接占用槽 0:`ConnState{Up, inst = 0, conn 0}`
  3. `ConnState{Up, inst = a1, conn 0}`
- 期望:
  - 步骤 1 后 `bbo(a1).conn_up = true`(槽 1),mask = 0b10
  - 步骤 2 后 a1 的 mask 仍为 0b10
  - 步骤 3 后 mask = 0b11
- 实现:`crates/hft-sequencer/tests/liveness.rs::seq_117_slot_reuse_membership`

### SEQ-118 全部成员断开后直到某成员 Up 都不健康

- 规则:[连接活性](README.md#连接活性)
- 级别:unit
- 里程碑:M2
- 前置:B0 稳态;a1 是槽 0、槽 1 的成员
- 步骤:
  1. 槽 0、槽 1 先后 `ConnState{Down, inst = 0}`
  2. 槽 1 `ConnState{Up, inst = 0}`、`ConnState{Up, inst = a1, conn 1}`
- 期望:
  - 步骤 1 后 `bbo(a1)`:`conn_up = false`、`conn_live_tsc = 0`、`resync = true`
  - 步骤 2 后 `conn_up = true`、`resync = false`
- 实现:`crates/hft-sequencer/tests/liveness.rs::seq_118_all_members_down`

### SEQ-119 深度与成交连接的记录不改连接活性

- 规则:[连接活性](README.md#连接活性)
- 级别:table
- 里程碑:M2
- 前置:B0 稳态(`MAX_BBO_CONN_SLOTS = 8`);节 26 快照为 S0
- 实现:`crates/hft-sequencer/tests/liveness.rs::seq_119_depth_conn_ignored`

| 行 | Input | 期望 |
|---|---|---|
| SEQ-119.1 | `ConnState{Alive, conn 8}` | 节 26 = S0;无投递 |
| SEQ-119.2 | `ConnState{Down, inst = 0, conn 8}` | 节 26 与各 inst 成员位不变;深度簿按"引擎处理"表失效 |
| SEQ-119.3 | `ConnState{Alive, conn 7}` | 槽 7 的 `live_tsc` 更新(仍是 BBO 槽) |
| SEQ-119.4 | `Trade{a1, conn 8}` | 节 26 = S0 |

### SEQ-120 节 10 与节 26 进快照,从关键帧起步的验证回放与连续运行一致

- 规则:[连接活性](README.md#连接活性)、[快照](README.md#快照)
- 级别:unit
- 里程碑:M2
- 前置:B0;一段 journal:关键帧 K 之前 a1 有成员槽 0、1,槽 1 处于 resync;K 之后 1 000 条 `Bbo` 与 `ConnState`
- 步骤:
  1. `VerifyDriver` 从 K 起步重放
- 期望:
  - 每条 Input 之后 `bbo(a1)` 的 `conn_up`、`conn_live_tsc`、`resync` 与连续运行逐条相同;Output 逐字节一致
- 实现:`crates/hft-sequencer/tests/liveness.rs::seq_120_liveness_from_keyframe`

### SEQ-121 先记录、后生效:Input 先 append,seq 只取自写者

- 规则:[每条 Input 的处理顺序](README.md#每条-input-的处理顺序process)
- 级别:unit
- 里程碑:M2
- 前置:B0 稳态;测试 `JournalSink` 记录每次 `append` 与组件调用的先后,并让 `append` 返回 seq 1 000(与驱动的任何计数无关)
- 步骤:
  1. 投递一条 bbo-A
- 期望:
  - `append(Bbo)` 早于 `Top::apply` 与任何组件调用;s1 读到 `now.seq = 1 000`;该事件的 Output 的 `trigger_seq = 1 000`
- 实现:`crates/hft-sequencer/tests/process.rs::seq_121_append_before_effect`

### SEQ-122 ControlCmd 的过期检查

- 规则:[每条 Input 的处理顺序](README.md#每条-input-的处理顺序process)、[命令路径](README.md#命令路径)
- 级别:table
- 里程碑:M2
- 前置:B0 稳态;`Kill{Account(1)}` 的 `expires_tsc = E`;每行该命令的记录头 tsc 如列
- 实现:`crates/hft-sequencer/tests/process.rs::seq_122_cmd_expiry`

| 行 | 记录头 tsc | 期望 |
|---|---|---|
| SEQ-122.1 | E − 1 | 执行,`CmdResult{Applied}` |
| SEQ-122.2 | E | 执行,`CmdResult{Applied}` |
| SEQ-122.3 | E + 1 | 只写 `CmdResult{Expired}`;无 `CapSet`,`ctl_state_version` 不变,不进去重表 |

### SEQ-123 Output 的记录头 tsc 等于触发它的 Input,体内带 trigger_seq

- 规则:[每条 Input 的处理顺序](README.md#每条-input-的处理顺序process)
- 级别:unit
- 里程碑:M2
- 前置:B0 稳态;s1 在 bbo-A(记录头 tsc = 4 000 000、seq = 300)上发一笔 a1 买单
- 步骤:
  1. 投递该 `Bbo`
- 期望:
  - 该事件的每条 Output(`OrderDecision`、`Note`)记录头 tsc = 4 000 000;`OrderDecision.trigger_seq = 300`
- 实现:`crates/hft-sequencer/tests/process.rs::seq_123_output_tsc_and_trigger_seq`

### SEQ-124 EVENT_END 落在本 Input 的最后一条非 Meta 记录上

- 规则:[每条 Input 的处理顺序](README.md#每条-input-的处理顺序process)
- 级别:table
- 里程碑:M2
- 前置:B0 稳态;每行投递一条产生所列记录的 Input
- 实现:`crates/hft-sequencer/tests/process.rs::seq_124_event_end_placement`

| 行 | 本事件的记录(按顺序) | 期望带 `EVENT_END` 的记录 |
|---|---|---|
| SEQ-124.1 | `Trade` | `Trade` |
| SEQ-124.2 | `Bbo`、`OrderDecision`、`LatencySample`(Meta) | `OrderDecision`;`LatencySample` 不带 |
| SEQ-124.3 | `OrderEvent`、`OrderDecision`、`Note{Oms}`、`Note{Measure}`(Meta) | `Note{Oms}` |
| SEQ-124.4 | `Timer{Sys, Snapshot}`、`StateSnapshot{Keyframe}` | `StateSnapshot` |

### SEQ-125 出环之前先 publish;同一 Input 的多条出环先全部追加再依次推环

- 规则:[每条 Input 的处理顺序](README.md#每条-input-的处理顺序process)
- 级别:unit
- 里程碑:M2
- 前置:B0 稳态;s1 在一条 bbo 上同时发 a1 买单与 b1 卖单(两条出环,分别到 out-A、out-B);测试环在每次推入时记录当时的 `committed_len`
- 步骤:
  1. 投递该 `Bbo`
- 期望:
  - 两条 `OrderDecision` 都已追加、`end_event` 一次、`publish` 一次之后才推第一条;两次推入时的 `committed_len` 都 ≥ 第二条 `OrderDecision` 的末尾;推入顺序 = `OrderDecision` 的追加顺序
- 实现:`crates/hft-sequencer/tests/process.rs::seq_125_publish_before_push`

### SEQ-126 decided_tsc 写进新单、撤单与改单的决策、出环与 SlotStamps

- 规则:[每条 Input 的处理顺序](README.md#每条-input-的处理顺序process)
- 级别:table
- 里程碑:M2
- 前置:B0 稳态;`drv.measure()` 在本 Input 返回 8 888;s1 在同一 bbo 上:新单 a1、撤单槽 h1、改单槽 h2
- 实现:`crates/hft-sequencer/tests/process.rs::seq_126_decided_tsc_all_ops`

| 行 | 操作 | 期望 |
|---|---|---|
| SEQ-126.1 | 新单 | `OrderDecision.decided_tsc = 8 888`;出环 `GwCmd` 带 8 888;新槽 `SlotStamps.decided_tsc = 8 888` |
| SEQ-126.2 | 撤单 h1 | 同上(`op = Cancel`) |
| SEQ-126.3 | 改单 h2 | 同上(`op = Amend`) |
| SEQ-126.4 | 两次运行 `measure()` 返回不同值 | 状态哈希与其余 Output 字段相同 |

### SEQ-127 撤单、改单与新单传本 Input 的 TrigStamps

- 规则:[每条 Input 的处理顺序](README.md#每条-input-的处理顺序process)、[意图管线](README.md#意图管线)
- 级别:unit
- 里程碑:M2
- 前置:B0 稳态;bbo-A 一条:`MarketHdr{ex_ts_ns: 11, ex_send_ns: 12, hw_rx_ns: 13, sw_rx_ns: 14, rx_tsc: 15}`,记录头 tsc = 16、seq = 17;s1 在其上发新单、撤 h1、改 h2
- 步骤:
  1. 投递该 `Bbo`
- 期望:
  - `oms.place` 的 `ApprovedIntent` 与 `oms.cancel`、`oms.amend` 的参数都带 `TrigStamps{ex_ts_ns: 11, ex_send_ns: 12, hw_rx_ns: 13, sw_rx_ns: 14, rx_tsc: 15, proc_tsc: 16, trigger_seq: 17, kind: Bbo}`
- 实现:`crates/hft-sequencer/tests/process.rs::seq_127_trig_stamps_passed`

### SEQ-128 OmsOut 按固定顺序排空

- 规则:[每条 Input 的处理顺序](README.md#每条-input-的处理顺序process)
- 级别:unit
- 里程碑:M2
- 前置:B0 稳态;构造一条 `OrderEvent`,使 OMS 在同一调用中产出 hedges 1、fills 1、fees 1、updates 2(一条 EXEC 槽、一条策略槽)、links 1、timers 1、notes 1、latency 1
- 步骤:
  1. 投递该 `OrderEvent`,记录 sequencer 的下游调用序列
- 期望:
  - 调用顺序:对冲定价与意图管线(第二腿)→ `ledger.apply_fill` → `ledger.apply_fee` → `exec.on_order_update`(EXEC 槽)→ 策略槽的 `EventRef::Order` 入队 → `exec.on_link` → `OrderDecision` 追加与出环登记 → 定时器进堆 → `Note`、`LatencySample` 追加;此后才调 `risk.on_pnl`/`risk.on_legs`
- 实现:`crates/hft-sequencer/tests/process.rs::seq_128_oms_out_drain_order`

### SEQ-129 对冲快路径排在本 Input 的最前

- 规则:[每条 Input 的处理顺序](README.md#每条-input-的处理顺序process)、[意图管线](README.md#意图管线)
- 级别:unit
- 里程碑:M2
- 前置:B0 稳态;s1 的 PairOnFill 链接:第一腿 a1 买 0.01 已 Ack;第二腿 b1 卖
- 步骤:
  1. 投递第一腿 `OrderEvent{Fill, cum_filled 0.01}`
- 期望:
  - 该事件的第一条 Output 是第二腿 b1 卖 0.010 的 `OrderDecision{role = Second}`,先于任何 `Note{Ledger}` 与策略产生的记录;out-B 恰好一条 `GwCmd`
- 实现:`crates/hft-sequencer/tests/process.rs::seq_129_hedge_fast_path_first`

### SEQ-130 成交与费用的净量补差在同一事件内转交 OMS 与执行层

- 规则:[每条 Input 的处理顺序](README.md#每条-input-的处理顺序process)
- 级别:table
- 里程碑:M2
- 前置:B0 稳态;s2 的 g2 现货腿买单(EXEC 槽)
- 实现:`crates/hft-sequencer/tests/process.rs::seq_130_net_adjust_same_event`

| 行 | Input | 期望(同一事件内) |
|---|---|---|
| SEQ-130.1 | `OrderEvent{Fill}`,ledger `apply_fill` 返回 `net_adjust = −0.00001` | `oms.net_adjust(2, g2, a1, −0.00001)` 与 `exec.on_leg_adjust(2, g2)` 各一次 |
| SEQ-130.2 | `OrderEvent{FeeOnly}`,`apply_fee` 返回 `NetAdjust(−0.00002)` | OMS 不迁移槽;`oms.net_adjust` 与 `exec.on_leg_adjust` 各一次 |
| SEQ-130.3 | `OrderEvent{FeeOnly}`,`apply_fee` 不返回 `NetAdjust` | 不调 `net_adjust` 与 `on_leg_adjust` |
| SEQ-130.4 | 账户 1 的 b1 卖单 0.001 的 `OrderEvent{Fill}`(计价资产手续费) | `FillDelta.net_qty` = +0.001(OMS `FillOut.net_qty` 有符号,交 ledger 时取绝对值);`apply_fill` 不返回 `net_adjust`,LegTracker b1 腿 `net` = −0.001 |

### SEQ-131 OrderUpdate 按槽的拥有者路由

- 规则:[每条 Input 的处理顺序](README.md#每条-input-的处理顺序process)
- 级别:table
- 里程碑:M2
- 前置:B0 稳态;每行一张槽的 `OrderEvent{Ack}`
- 实现:`crates/hft-sequencer/tests/process.rs::seq_131_update_routing`

| 行 | 槽 | 期望 |
|---|---|---|
| SEQ-131.1 | 带 `EXEC` 标志(s2 的执行层切片) | `exec.on_order_update` 一次;s2 不收到 `EventRef::Order` |
| SEQ-131.2 | 带 `SMOKE` 标志(冒烟单) | 不投递给任何策略,也不调 `exec.on_order_update` |
| SEQ-131.3 | s1 经 `Ctx::place` 下的单 | s1 收到 `EventRef::Order`;s2、s9 收不到 |

### SEQ-132 同一 Input 内 exec.on_risk 在全部 on_order_update 与 on_link 之后

- 规则:[每条 Input 的处理顺序](README.md#每条-input-的处理顺序process)、[模式与 cap 的驱动](README.md#模式与-cap-的驱动)
- 级别:unit
- 里程碑:M2
- 前置:B0 稳态;s2 的 g2 处于 `Broken`,执行层的自动有界平仓剩最后一片(EXEC 槽,b1 买 0.003),该片成交后已成交裸量 ≤ 尘埃线且 group 静止
- 步骤:
  1. 投递该片的 `OrderEvent{Fill, cum_filled 0.003}`
- 期望:
  - 同一事件内调用顺序:`exec.on_order_update` → `risk.on_legs` 返回 `AutoClear{Broken}` → `apply` → `exec.on_risk(CapCleared{Broken})`;输出含 `ExecDecision{ExecAutoFixDone}` 与 `Note{Risk, CapCleared}`(Broken);`HedgeFailure` cap 仍在
- 实现:`crates/hft-sequencer/tests/process.rs::seq_132_on_risk_after_updates`

### SEQ-133 NotifyQueue 的分区容量合计 128,分区满时按产生顺序进积压

- 规则:[每条 Input 的处理顺序](README.md#每条-input-的处理顺序process)、[通知队列与续推](README.md#通知队列与续推)
- 级别:table
- 里程碑:M5
- 前置:B0 稳态;每行在一个 Input 内使所列分区产生所列条数的通知
- 实现:`crates/hft-sequencer/tests/notify_backlog.rs::seq_133_notify_partitions`

| 行 | 分区与条数 | 期望 |
|---|---|---|
| SEQ-133.1 | 编译期 | 分区容量 OMS 16、执行层 8、风控 8、模式 64、行情订阅 8、其他 24,合计 128(`const` 断言) |
| SEQ-133.2 | OMS 16 | 本 Input 全部投递;积压 0 |
| SEQ-133.3 | OMS 17 | 前 16 条本 Input 投递,第 17 条进 `NotifyBacklog`;`wants_continue()` 为真 |
| SEQ-133.4 | 模式 64 | 全部投递 |
| SEQ-133.5 | 模式 65,随后同一 Input 内 OMS 1 | 前 64 条模式通知投递;第 65 条与其后的 OMS 那条都按产生顺序进积压(分区满之后产生的一律进积压) |

### SEQ-134 策略投递的顺序

- 规则:[每条 Input 的处理顺序](README.md#每条-input-的处理顺序process)
- 级别:unit
- 里程碑:M2
- 前置:B0 稳态;s1、s2、s9 都订阅 a1 的 BBO;一条 `Kill{Global}` 使 s9 的 g9、s1 的 g1、s2 的 g2 依次产生 `Mode` 通知
- 步骤:
  1. 投递 bbo-A(a1)
  2. 投递该 `Kill{Global}`
- 期望:
  - 步骤 1:投递顺序 s1、s2、s9
  - 步骤 2:`EventRef::Mode` 的投递顺序 s1、s2、s9(实盘实例先);同一实例内按入队顺序
- 实现:`crates/hft-sequencer/tests/process.rs::seq_134_delivery_order`

### SEQ-135 Ctx 中各类请求的去向

- 规则:[每条 Input 的处理顺序](README.md#每条-input-的处理顺序process)、[目标意图与执行层](README.md#目标意图与执行层)
- 级别:table
- 里程碑:M2
- 前置:B0 稳态;s1 在一次 `on_event` 中依次调用所列 `Ctx` 方法
- 实现:`crates/hft-sequencer/tests/process.rs::seq_135_ctx_application`

| 行 | Ctx 调用 | 期望(同一 Input 内,按调用顺序) |
|---|---|---|
| SEQ-135.1 | `target(TargetIntent{account 1, group g1})` | `exec.set_target(owner = 1, ..)` 一次 |
| SEQ-135.2 | `clear_target(1, g1)` | `exec.clear_target(1, 1, g1, ..)` 一次 |
| SEQ-135.3 | `place(a1 买 0.01)` | 进意图管线;`exec.note_strategy_action(1, g1)` 一次 |
| SEQ-135.4 | `cancel(h1)` | `oms.cancel(h1, &trig, ..)` |
| SEQ-135.5 | `request_halt(g1, reason)` | `apply(Auto(Halted{strategy 1, account 1, group g1}))`;`Note{Risk, CapSet}` |
| SEQ-135.6 | `timer_at(t, tag 5)` | 堆中新增 `(owner 17, timer_id 5, group g1)` |
| SEQ-135.7 | `note(..)` | 追加一条 `Note{Strategy}`,`Note.strategy = 1` |

### SEQ-136 每次调用执行层带 out 的入口之后 ExecOut 已被排空

- 规则:[每条 Input 的处理顺序](README.md#每条-input-的处理顺序process)
- 级别:prop
- 里程碑:M2
- 性质:任意 Input 序列中,每次调用 `exec` 带 `out` 的入口(`set_target`、`clear_target`、`on_external`、`force_deleverage`、`engine_flatten`、`hedge_fix_cmd`、`advance`)返回后、下一次调用执行层之前,`ExecOut` 的全部向量为空(调试断言)
- 生成:B0 上 100–2 000 条随机 Input,含目标、成交、外来成交、MarginReport L3、Kill flatten
- 参考:调试断言 `exec_out.is_empty()` 不触发
- 例数:PR 300;每夜 10 000
- 实现:`crates/hft-sequencer/tests/process.rs::seq_136_exec_out_drained_after_each_call`

### SEQ-137 ExecOut 按固定顺序排空

- 规则:[每条 Input 的处理顺序](README.md#每条-input-的处理顺序process)
- 级别:unit
- 里程碑:M2
- 前置:B0 稳态;构造一次 `exec.advance`,其 `ExecOut` 含 decisions 1、cancel_resting 1(account 2, b1)、orders 1、cancels 1、broken 1、timers 1、notify 1、notes 1
- 步骤:
  1. 投递触发该 `advance` 的 bbo,记录下游调用序列
- 期望:
  - 顺序:`ExecDecision` 追加 → `oms.cancel_resting_on(2, b1, ..)` → 订单进意图管线并 `exec.on_place_result` 回填 → 撤单交 OMS → `apply(Auto(Broken))` → 定时器进堆 → 通知入队给 s2 → `Note{Exec}` 追加
- 实现:`crates/hft-sequencer/tests/process.rs::seq_137_exec_out_drain_order`

### SEQ-138 只在有待推进 cell 时调 advance

- 规则:[每条 Input 的处理顺序](README.md#每条-input-的处理顺序process)
- 级别:table
- 里程碑:M2
- 前置:B0 稳态;s2 对 g2 有活动目标,腿 a1、b1
- 实现:`crates/hft-sequencer/tests/process.rs::seq_138_advance_only_when_pending`

| 行 | Input | 期望 `exec.advance` 调用次数 |
|---|---|---|
| SEQ-138.1 | bbo-A(a1) | 1(`touch_inst(a1)` 使 cell 待推进) |
| SEQ-138.2 | bbo-A(a2,无 cell 引用) | 0 |
| SEQ-138.3 | `Trade{a1}` | 0 |

### SEQ-139 任何路径设 Broken 后同一 Input 内对该 group 调一次 on_legs

- 规则:[每条 Input 的处理顺序](README.md#每条-input-的处理顺序process)
- 级别:table
- 里程碑:M2
- 前置:B0 稳态;每行在一个 Input 内经所列路径对 (1, g1) 设 `Broken`
- 实现:`crates/hft-sequencer/tests/process.rs::seq_139_on_legs_after_broken`

| 行 | 路径 | 期望 |
|---|---|---|
| SEQ-139.1 | 链接 `Broken{Deadline}`(`LinkUpdate`) | `apply(Auto(Broken{1, g1}))` 后 `risk.on_legs(1, g1, &LegSummary)` 一次 |
| SEQ-139.2 | 对冲单被 `HEDGE_LIMIT` 拒绝 | 同上 |
| SEQ-139.3 | 执行层 `ExecOut.broken` | 同上 |
| SEQ-139.4 | `resolve_broken` 判定为真 | 同上 |
| SEQ-139.5 | 只有 LegTracker 变化(成交),未设 Broken | `on_legs` 一次(随 LegTracker 变化) |

### SEQ-140 LegSummary 的投影

- 规则:[每条 Input 的处理顺序](README.md#每条-input-的处理顺序process)
- 级别:table
- 里程碑:M2
- 前置:B0 稳态;g1 两腿:a1 现货 +0.010、b1 永续 −0.007;b1 mark 50 000 USDT;每行补充槽状态
- 实现:`crates/hft-sequencer/tests/process.rs::seq_140_leg_summary_projection`

| 行 | 槽状态 | 期望 `LegSummary` |
|---|---|---|
| SEQ-140.1 | 无在途 | `naked_filled_notional` = 两腿净量之和 0.003 的绝对值 × 50 000 = 150 USDT;`quiescent = true` |
| SEQ-140.2 | b1 有一张 `PendingNew` 槽 | `quiescent = false` |
| SEQ-140.3 | b1 有一张 `Unknown` 槽 | `quiescent = false` |
| SEQ-140.4 | b1 即时在途 IOC 0.003(Live) | `quiescent = false`(`inflight_immediate ≠ 0`) |

### SEQ-141 每个 Input 至多两轮策略投递,第二次投递中的目标变更留给下一相关 Input

- 规则:[每条 Input 的处理顺序](README.md#每条-input-的处理顺序process)、[目标意图与执行层](README.md#目标意图与执行层)
- 级别:unit
- 里程碑:M5
- 前置:B0 稳态;s2 收到第 6 步的 `EventRef::Exec` 进度通知时设一个新目标并下一笔策略单
- 步骤:
  1. 投递触发执行层推进的 bbo-A
- 期望:
  - 同一 Input 内:s2 在第二次投递中收到 `Exec` 通知;其策略单经意图管线出环;新目标交 `exec.set_target`,但本 Input 内 `exec.advance` 只调了一次;执行层登记一条 deadline ≤ 当前 TSC 的 `Timer{Exec, Wake}`,下一轮被交出并推进该目标;s2 在本 Input 内没有第三次投递
- 实现:`crates/hft-sequencer/tests/process.rs::seq_141_two_delivery_rounds`

### SEQ-142 Bbo 的处理步骤

- 规则:[每条 Input 的处理顺序](README.md#每条-input-的处理顺序process)
- 级别:unit
- 里程碑:M2
- 前置:B0 稳态;s1 订阅 a1 的 BBO;s2 对 g2 有活动目标
- 步骤:
  1. 投递被应用的 `Bbo{a1}`,记录调用序列
- 期望:
  - 顺序:`Top::apply` → 连接活性更新 → `ledger.on_ref_px(.., now.engine_ns)` → `exec.touch_inst(a1)` → s1 入队 `EventRef::Bbo{inst: a1, b}`,`b` 与处理后的 `v.bbo(a1)` 逐字段相同
- 实现:`crates/hft-sequencer/tests/process.rs::seq_142_bbo_steps`

### SEQ-143 被判为 DUPLICATE 或 OLDER 的 Bbo 不更新换算率

- 规则:[每条 Input 的处理顺序](README.md#每条-input-的处理顺序process)、[连接活性](README.md#连接活性)
- 级别:unit
- 里程碑:M2
- 前置:B0 稳态
- 步骤:
  1. 投递一条被 hft-book 判为 OLDER 的 `Bbo{a1, conn 0, rx_tsc 9 000}`
- 期望:
  - 不调 `ledger.on_ref_px`;Top 不变;槽 0 的 `live_tsc` 前移到 9 000(活性证据照常);a1 的 `RESYNC` 不被清除
- 实现:`crates/hft-sequencer/tests/process.rs::seq_143_stale_bbo_no_ref_px`

### SEQ-144 L2Batch 整批应用之后策略才看到簿

- 规则:[每条 Input 的处理顺序](README.md#每条-input-的处理顺序process)
- 级别:unit
- 里程碑:M3
- 前置:B0 稳态;a1 簿为 Live 并订阅深度;s1 订阅 a1 的 `BOOK`
- 步骤:
  1. 投递一条含 20 档更新的 `L2Batch`
- 期望:
  - s1 只收到一次 `EventRef::Book{inst: a1}`,收到时 `v.book()` 已含全部 20 档;`ledger.on_ref_px` 与 `exec.touch_inst(a1)` 各一次,都在整批应用之后
- 实现:`crates/hft-sequencer/tests/process.rs::seq_144_l2batch_whole_batch`

### SEQ-145 L2Snapshot{origin = TopN} 整替换后订阅者收到 Book

- 规则:[每条 Input 的处理顺序](README.md#每条-input-的处理顺序process)
- 级别:unit
- 里程碑:M3
- 前置:B0 稳态;a1 订阅分档深度(TopN 流)
- 步骤:
  1. 投递 `L2Snapshot{origin = TopN, 10 档}`
- 期望:
  - hft-book `apply_snapshot` 一次(无序号链、无 REST 重同步);s1 收到一次 `EventRef::Book`;`ledger.on_ref_px` 与 `exec.touch_inst` 各一次
- 实现:`crates/hft-sequencer/tests/process.rs::seq_145_l2snapshot_topn`

### SEQ-146 REST 快照分片按 md 逐条应用,其后的 ConnState{Up, inst} 使簿回到 Live

- 规则:[每条 Input 的处理顺序](README.md#每条-input-的处理顺序process)
- 级别:unit
- 里程碑:M3
- 前置:B0 稳态;a1 启用增量深度,簿处于 `Resyncing`;REST 快照分 3 片
- 步骤:
  1. 依次投递 3 条 `L2Snapshot{origin = Exchange}` 分片,中间穿插 1 条 bbo-A
  2. 投递 `ConnState{Up, inst = a1}`(md 连接)
- 期望:
  - 3 片各为一条 Input,`Bbo` 在分片之间被交出;步骤 1 后簿仍非 Live;步骤 2 后簿为 Live
- 实现:`crates/hft-sequencer/tests/process.rs::seq_146_rest_snapshot_parts`

### SEQ-147 Trade、Funding、IndexPrice、Liquidation 更新 MarketLast 并入队

- 规则:[每条 Input 的处理顺序](README.md#每条-input-的处理顺序process)
- 级别:table
- 里程碑:M2
- 前置:B0 稳态;s2 订阅 b1 的 `TRADE`、`FUNDING`、`INDEX`、`LIQUIDATION`
- 实现:`crates/hft-sequencer/tests/process.rs::seq_147_market_last_updates`

| 行 | Input | 期望 |
|---|---|---|
| SEQ-147.1 | `Trade{b1, px 50 010, qty 0.5}` | 节 10 b1 行 `trade_px`、`trade_qty`、`trade_ts_ns` 更新;s2 收到 `EventRef::Trade` |
| SEQ-147.2 | `IndexPrice{b1, 50 005}` | `index_px = 50 005`;s2 收到 `EventRef::Index` |
| SEQ-147.3 | `Liquidation{b1}` | s2 收到 `EventRef::Liquidation` |
| SEQ-147.4 | `Funding{b1, rate 0.0001, next 08:00, interval_s 0}` | `funding_rate`、`next_funding_ns` 更新;`interval_s` 不变;不调 `ledger.on_funding_interval` |

### SEQ-148 Funding 带新的结算周期时记录上一周期并通知 ledger

- 规则:[每条 Input 的处理顺序](README.md#每条-input-的处理顺序process)
- 级别:table
- 里程碑:M2
- 前置:B0 稳态;b1 行 `interval_s = 28 800`、`prev_interval_s = 28 800`;每行从上一行的状态继续
- 实现:`crates/hft-sequencer/tests/process.rs::seq_148_funding_interval_change`

| 行 | Input | 期望 |
|---|---|---|
| SEQ-148.1 | `Funding{b1, interval_s 3 600}` | `prev_interval_s = 28 800`、`interval_s = 3 600`;`ledger.on_funding_interval(b1, 3 600)` 一次 |
| SEQ-148.2 | `Funding{b1, interval_s 3 600}` | 两字段不变;`ledger.on_funding_interval(b1, 3 600)` 一次 |
| SEQ-148.3 | `Funding{b1, interval_s 0}` | 两字段不变;不调 `on_funding_interval` |
| SEQ-148.4 | `Funding{b1, rx_tsc 4 000}` | `funding(b1).upd_tsc = 4 000` |

### SEQ-149 MarkPrice 经 ledger 投影为 MarginSample 交风控

- 规则:[每条 Input 的处理顺序](README.md#每条-input-的处理顺序process)
- 级别:unit
- 里程碑:M2
- 前置:B0 稳态;账户 1 持有 b1 空 0.010;s2 订阅 b1 的 `MARK`
- 步骤:
  1. 投递 `MarkPrice{b1, 50 100}`(engine_ns = 7 000 000)
- 期望:
  - 节 10 b1 行 `mark_px = 50 100`、`mark_ens = 7 000 000`;调用顺序 `ledger.on_mark(.., 7 000 000, &mut touched)` → 对 touched 中的 (1, B) 调一次 `risk.on_margin(MarginSample{mark_ens = mark_oldest_ens, ..})` 并执行 `MarginOut` → `exec.touch_inst(b1)` → s2 入队 `EventRef::Mark{px: 50 100}`
- 实现:`crates/hft-sequencer/tests/process.rs::seq_149_mark_price_to_margin`

### SEQ-150 ExternalFill 完整归属到 group:入账、LegTracker、执行层截止、策略事件

- 规则:[每条 Input 的处理顺序](README.md#每条-input-的处理顺序process)
- 级别:unit
- 里程碑:M2
- 前置:B0 稳态;g1 的 b1 腿 −0.010;收到 `ExternalFill{account 1, b1, Buy 0.004, cause Liquidation, trade_id 77}`;hft-exec 对它返回截止 D
- 步骤:
  1. 投递该 `ExternalFill`
- 期望:
  - `ledger.apply_external(e, auto_recon = false, ..)` 返回 `ExternalApplied{group: g1, net_qty: +0.004, none_qty: 0}`
  - 依次 `oms.external_adjust(1, B, g1, b1, +0.004, ..)` 一次、`exec.on_external(1, view{group: g1, qty: 0.004})` 一次、`risk.on_pnl`、`risk.on_legs(1, g1, ..)`
  - s1 收到一条 `EventRef::External{gix: 0}`,`deadline_tsc = D`;输出含 `Note{Ledger, ExternalFill}`(P0)
- 实现:`crates/hft-sequencer/tests/process.rs::seq_150_external_fill_attributed`

### SEQ-151 ExternalFill 超出该腿的部分记 GROUP_NONE 并另调一次 on_external

- 规则:[每条 Input 的处理顺序](README.md#每条-input-的处理顺序process)
- 级别:unit
- 里程碑:M9
- 前置:B0 稳态;g1 的 b1 腿 −1.000,账户另有无组 b1 空 0.004;全量强平 `ExternalFill{b1, Buy 1.004}`
- 步骤:
  1. 投递该 `ExternalFill`
- 期望:
  - `ExternalApplied{group: g1, net_qty: +1.000, none_qty: +0.004}`;`oms.external_adjust(.., g1, .., +1.000)` 一次(LegTracker 的 b1 腿为 0,不越过零)
  - `exec.on_external` 两次:`view{group: g1, qty: 1.000}` 与 `view{group: GROUP_NONE, qty: 0.004}`;s1 收到两条 `EventRef::External`,第二条 `gix = GIX_NONE`
- 实现:`crates/hft-sequencer/tests/process.rs::seq_151_external_fill_split_none`

### SEQ-152 不可归属的 ExternalFill 只调一次 on_external

- 规则:[每条 Input 的处理顺序](README.md#每条-input-的处理顺序process)
- 级别:unit
- 里程碑:M2
- 前置:B0 稳态;账户 1 没有引用 a2 的非 Shadow cell;`ExternalFill{account 1, a2, Sell 0.5, cause Manual}`
- 步骤:
  1. 投递该 `ExternalFill`
- 期望:
  - `exec.on_external` 恰好一次,视图 `group = GROUP_NONE`、`qty = 0.5`,返回 0;s1 恰好一条 `EventRef::External{gix: GIX_NONE, deadline_tsc: 0}`;没有 `qty = 0` 的调用或事件
- 实现:`crates/hft-sequencer/tests/process.rs::seq_152_external_fill_unattributed`

### SEQ-153 重复的 ExternalFill 到此为止

- 规则:[每条 Input 的处理顺序](README.md#每条-input-的处理顺序process)
- 级别:unit
- 里程碑:M2
- 前置:SEQ-150 处理之后
- 步骤:
  1. gw 重订阅后再投递同一 `ExternalFill{trade_id 77}`
- 期望:
  - `apply_external` 返回 `None`;本事件除 `ExternalFill` Input 本身外没有任何 Output;不调 `oms.external_adjust`、`exec.on_external`、`risk.on_pnl`;无策略事件
- 实现:`crates/hft-sequencer/tests/process.rs::seq_153_external_fill_duplicate`

### SEQ-154 被 auto_held 全部抵扣的外来成交不调执行层

- 规则:[每条 Input 的处理顺序](README.md#每条-input-的处理顺序process)
- 级别:unit
- 里程碑:M9
- 前置:B0 稳态;此前 `AUTO_EXTERNAL` 合成使 `auto_held[(1, b1, Buy)] = 2`
- 步骤:
  1. 投递交易所补推的 `ExternalFill{account 1, b1, Buy 2, cause Adl}`
- 期望:
  - ledger 写 `Note{Ledger, LedgerAutoHeldOffset}`(P0);`apply_external` 返回 `None`;不调 `oms.external_adjust`、`exec.on_external`;无 `EventRef::External`;LegTracker 与执行层状态的哈希不变
- 实现:`crates/hft-sequencer/tests/process.rs::seq_154_auto_held_full_offset`

### SEQ-155 OMS 防御路径的 ForeignFill 按 ExternalFill{cause = Unknown} 处理

- 规则:[每条 Input 的处理顺序](README.md#每条-input-的处理顺序process)
- 级别:table
- 里程碑:M9
- 前置:B0 稳态;账户 1 在 B;每行投递使 OMS 产出所列 `ForeignFill` 的 `OrderEvent`
- 实现:`crates/hft-sequencer/tests/process.rs::seq_155_foreign_fill_dedup_keys`

| 行 | ForeignFill | 期望 |
|---|---|---|
| SEQ-155.1 | `{trade_id 501, exch_order_id 9, cum_filled 0.002, qty 0.002}` | 构造 `ExternalFill{cause: Unknown, trade_id: 501}` 入账;同一事件内无另写的 Input 记录 |
| SEQ-155.2 | 同 SEQ-155.1 再来一次 | ledger 去重返回 `None`,无执行层调用 |
| SEQ-155.3 | `{trade_id 0, exch_order_id 9, cum_filled 0.003, qty 0.003}` | `fill_seq` = 0.003 的 raw 值;入账 |
| SEQ-155.4 | `{trade_id 0, exch_order_id 9, cum_filled 0.005, qty 0.002}` | `fill_seq` = 0.005 的 raw 值;与 SEQ-155.3 不同键,也入账 |
| SEQ-155.5 | `VerifyDriver` 重放 SEQ-155.1–155.4 | 账本与 Output 逐字节相同(合成在 Input 内确定发生) |

### SEQ-156 MarginReport 的处理与 MarginOut 的执行

- 规则:[每条 Input 的处理顺序](README.md#每条-input-的处理顺序process)
- 级别:table
- 里程碑:M2
- 前置:B0 稳态;账户 1 在 B 持有 b1 空;每行投递一条使 `risk.on_margin` 产出所列 `MarginOut` 的 `MarginReport`
- 实现:`crates/hft-sequencer/tests/process.rs::seq_156_margin_report`

| 行 | MarginOut | 期望 |
|---|---|---|
| SEQ-156.1 | 任意 | 先 `ledger.on_margin(m, now.engine_ns)`,再 `risk.on_margin`(`truncated` = 记录头 `ITEMS_TRUNCATED` 位) |
| SEQ-156.2 | `mode`(MarginStress L1 cap) | `risk.mode.apply`;`Note{CapSet}` |
| SEQ-156.3 | `events`(`RiskEvent::MarginStress`) | s1 入队 `EventRef::Risk`;`exec.on_risk(1, &e, now)` 一次 |
| SEQ-156.4 | `force`(frac 0.25) | `exec.force_deleverage(1, B, 0.25, now, &mut out)` 一次;其 `ExecOut` 立即排空 |
| SEQ-156.5 | `timers`(engine_ns 截止 = 当前 + 5 s) | 按 `now.tsc_of` 换成本次运行 TSC(向上取整)进堆:deadline = 当前 TSC + 5 000 000 000 |
| SEQ-156.6 | `notes` | 原样追加 |

### SEQ-157 AccountConfigReport 交杠杆与闸门

- 规则:[每条 Input 的处理顺序](README.md#每条-input-的处理顺序process)、[启动闸门](README.md#启动闸门)
- 级别:unit
- 里程碑:M2
- 前置:B0,账户 1 闸门未放行
- 步骤:
  1. 投递 `AccountConfigReport{account 1, venue B, mismatches 0, lev_items: [b1 10x]}`
- 期望:
  - `risk.on_account_cfg(hdr, &lev_items)` 一次,随后 `risk.mode.apply(Gate(AccountCfg{account: 1, venue: B, mismatches: 0}))`;闸门位变化写 `Note{StartupGate}`;之后 `leverage(1, b1) = Some(1000)`
- 实现:`crates/hft-sequencer/tests/process.rs::seq_157_account_config_report`

### SEQ-158 交易所推送的 FundingSettle 按 VenueCaps.funding_settle 处理

- 规则:[每条 Input 的处理顺序](README.md#每条-input-的处理顺序process)
- 级别:table
- 里程碑:M9
- 前置:B0 稳态;账户 1 持有 b1 空 0.010;每行设定 venue B 的 `funding_settle` 与推送
- 实现:`crates/hft-sequencer/tests/process.rs::seq_158_funding_settle_push`

| 行 | funding_settle / 推送 | 期望 |
|---|---|---|
| SEQ-158.1 | `PerSymbolPush` / `FundingSettle{b1, 08:00}` | `ledger.apply_funding` 一次,`risk.on_pnl` 一次 |
| SEQ-158.2 | `PerSymbolPush` / 同一推送重放 | `apply_funding` 判重,无入账、无 `on_pnl` 变化 |
| SEQ-158.3 | `BalanceOnlyPush` / `FundingSettle{inst = INST_NONE, 金额 −1.2}` | `ledger.apply_funding_push` 一次,钱包入账 −1.2 |
| SEQ-158.4 | `BalanceOnlyPush` / 推送与合成记录不一致 | ledger 返回 `Mismatch` → 一条 `Note{Ledger, FundingMismatch}` |
| SEQ-158.5 | `PerSymbolPush` / 推送结算时刻 08:00:02(晚 2 s) | 经 `funding_settle_norm_ns` 归一化为 08:00:00 后配对 |

SEQ-158 补充验收:RateOnly 的实额输入替换已有合成额,重复实额无动作,验证重放逐字节相同。实现:`tests/funding.rs::seq_158_rate_only_actual_settlement_replaces_synth_and_replays`。

### SEQ-159 BalanceReport 与 PositionReport

- 规则:[每条 Input 的处理顺序](README.md#每条-input-的处理顺序process)
- 级别:table
- 里程碑:M2
- 前置:B0 稳态
- 实现:`crates/hft-sequencer/tests/process.rs::seq_159_balance_position_reports`

| 行 | Input 与状态 | 期望 |
|---|---|---|
| SEQ-159.1 | `BalanceReport{1, A}`,账户 1 在 A 无在途 | `ledger.on_balance(b, quiet = true, now.engine_ns)` |
| SEQ-159.2 | 同上,账户 1 在 A 有一张 Live 挂单 | `quiet = false` |
| SEQ-159.3 | `PositionReport{1, b1, −0.010}`,引擎 −0.010 | 无 Output |
| SEQ-159.4 | `PositionReport{1, b1, −0.012}`,引擎 −0.010 | 一条 `Note`;仓位、LegTracker 不变 |

### SEQ-160 非 Alive 的下单与深度连接 ConnState 的处理

- 规则:[每条 Input 的处理顺序](README.md#每条-input-的处理顺序process)、[启动闸门](README.md#启动闸门)
- 级别:table
- 里程碑:M3
- 前置:B0,账户 1 闸门未放行;s1 订阅 B 的连接事件
- 实现:`crates/hft-sequencer/tests/process.rs::seq_160_conn_state_order_and_depth`

| 行 | Input | 期望 |
|---|---|---|
| SEQ-160.1 | gw 主下单连接 `ConnState{Up, role = Order, venue B, account 1}`,用户流已 Up,热备 Down | `oms.on_conn_state` 一次;s1 收到 `EventRef::Conn`;`apply(Gate(OrderConns{account: 1, venue: B, primary_up: true, standby_up: false}))` |
| SEQ-160.2 | 热备 `ConnState{Up}` | `Gate(OrderConns{.., primary_up: true, standby_up: true})` |
| SEQ-160.3 | 深度连接 `ConnState{Down, conn 8}`(a1 订阅深度) | a1 的簿失效(非 Live);s1 收到 `EventRef::Conn` |
| SEQ-160.4 | 深度连接 `ConnState{Alive, conn 8}` | 无任何状态变化与投递 |

### SEQ-161 对账报告的 Funding 条目到达即处理、不暂存

- 规则:[每条 Input 的处理顺序](README.md#每条-input-的处理顺序process)
- 级别:unit
- 里程碑:M2
- 前置:B0 稳态;`report_id = 900` 的一组共 3 条:第 1 条 `Funding`(`inst = INST_NONE` 钱包条目)、第 2 条仓位、第 3 条余额
- 步骤:
  1. 投递第 1 条
  2. 投递第 2 条
- 期望:
  - 步骤 1:`ledger.reconcile_funding(it, now.utc_ns, now.engine_ns)` 一次;返回 `Corrected` 时其 `PnlDelta` 交 `risk.on_pnl`;节 19 中 report 900 的 `ReconPendingHdr{n_stored: 0, count: 3, received: 1}`
  - 步骤 2:节 19 `{n_stored: 1, count: 3, received: 2}`;不调 `oms.on_reconcile`
- 实现:`crates/hft-sequencer/tests/recon.rs::seq_161_funding_items_immediate`

### SEQ-162 同组末条到达时整组处理

- 规则:[每条 Input 的处理顺序](README.md#每条-input-的处理顺序process)
- 级别:unit
- 里程碑:M2
- 前置:SEQ-161 步骤 2 之后
- 步骤:
  1. 投递第 3 条(余额条目)
- 期望:
  - 顺序:① 无 `AUTO_EXTERNAL` 条目,跳过 → ② 整组 `oms.on_reconcile`、`risk.on_reconcile`,闸门输入交 `mode.apply` → ③ `ledger.funding_window_sweep(1, venue)` 一次,`Corrected` 交 `risk.on_pnl`;节 19 中 report 900 消失
- 实现:`crates/hft-sequencer/tests/recon.rs::seq_162_group_completion`

### SEQ-163 只含 Funding 条目的报告组不影响闸门与 Recon

- 规则:[每条 Input 的处理顺序](README.md#每条-input-的处理顺序process)
- 级别:unit
- 里程碑:M2
- 前置:B0,账户 1 闸门未放行、`RECONCILED` 未置位;report 901 只有 2 条 `Funding` 条目,其中 1 条移出 `funding_recon_lag_ms` 窗口且无历史
- 步骤:
  1. 依次投递两条
- 期望:
  - 每条到达即 `reconcile_funding`;末条后 `funding_window_sweep` 对移出窗口的那条入账、`Corrected` 进 `risk.on_pnl`;闸门位与 `Recon` cap 不变,无 `Note{StartupGate}`
- 实现:`crates/hft-sequencer/tests/recon.rs::seq_163_funding_only_group`

### SEQ-164 跨重启的未收齐报告组从节 19 继续

- 规则:[每条 Input 的处理顺序](README.md#每条-input-的处理顺序process)、[快照](README.md#快照)
- 级别:unit
- 里程碑:M2
- 前置:report 902 共 3 条,已到 2 条(非 Funding)后写 Drain 快照并干净重启
- 步骤:
  1. 新运行投递第 3 条
- 期望:
  - 整组处理恰好一次,`oms.on_reconcile` 收到 3 条条目,与不重启时的调用参数相同
- 实现:`crates/hft-sequencer/tests/recon.rs::seq_164_recon_pending_across_restart`

### SEQ-165 AUTO_EXTERNAL 复核通过时在本 Input 内合成外来成交

- 规则:[每条 Input 的处理顺序](README.md#每条-input-的处理顺序process)
- 级别:table
- 里程碑:M9
- 前置:B0 稳态;账户 1 在 B 永续 b1 空 10(引擎 `engine_qty = −10`);用户流自最近一次 Up 起已稳定 ≥ 1 个对账周期,本组与上一组 `Summary` 都无 `USERSTREAM_DOWN`;账户 1 在 B 无 Unknown 槽、b1 无非终态槽、b1 自 `view_seq` 以来无成交;b1 `mark_px = 50 000`;每行一组只含该仓位条目(带 `AUTO_EXTERNAL`)的报告
- 实现:`crates/hft-sequencer/tests/recon.rs::seq_165_auto_external_applied`

| 行 | `exch_qty` | 期望 |
|---|---|---|
| SEQ-165.1 | −8 | 调 `ledger.auto_external_fill(1, B, b1, −2, 50 000, report_id)`,按 `ExternalFill` 行入账(`auto_recon = true`):引擎仓位 −8;`Note{Ledger, ExternalFill}` P0 带 `AUTO_RECON`;LegTracker 更新;s1 收到 `EventRef::External`;随后 `Gate(Disposed{.., AutoExternal})`,该条目不再挡 `RECONCILED` |
| SEQ-165.2 | 0(全量 ADL) | 合成 `qty = 10`(Buy),入账后引擎仓位 0 |
| SEQ-165.3 | 任意通过的行 | 本事件中除 `ReconcileReport` 本身外没有任何新的 Input 记录(合成不另写记录) |

### SEQ-166 AUTO_EXTERNAL 复核不通过时不入账、照常分级

- 规则:[每条 Input 的处理顺序](README.md#每条-input-的处理顺序process)
- 级别:table
- 里程碑:M9
- 前置:同 SEQ-165,`exch_qty = −8`;每行只破坏所列一项
- 实现:`crates/hft-sequencer/tests/recon.rs::seq_166_auto_external_rejected`

| 行 | 不满足的条件 | 期望 |
|---|---|---|
| SEQ-166.1 | 本组 `Summary` 带 `USERSTREAM_DOWN` | 不调 `auto_external_fill`;引擎仓位仍 −10;条目照常交 `oms.on_reconcile`/`risk.on_reconcile` 分级;无 `Disposed` |
| SEQ-166.2 | 上一组带 `USERSTREAM_DOWN` | 同上 |
| SEQ-166.3 | 用户流最近一次 Up 晚于上一组的 `view_seq` | 同上 |
| SEQ-166.4 | 账户 1 在 B 有一个 Unknown 槽 | 同上 |
| SEQ-166.5 | b1 有一张非终态槽 | 同上 |
| SEQ-166.6 | b1 在 `view_seq` 之后有一笔成交 | 同上 |
| SEQ-166.7 | 引擎仓位 −9 ≠ 条目 `engine_qty` −10 | 同上 |
| SEQ-166.8 | `exch_qty = +2`(反号) | 同上 |
| SEQ-166.9 | `exch_qty = −12`(绝对值更大) | 同上 |

### SEQ-167 AUTO_EXTERNAL 的回放、重放与冷读者

- 规则:[每条 Input 的处理顺序](README.md#每条-input-的处理顺序process)
- 级别:table
- 里程碑:M9
- 前置:SEQ-165.1 的 journal
- 实现:`crates/hft-sequencer/tests/recon.rs::seq_167_auto_external_replay`

| 行 | 动作 | 期望 |
|---|---|---|
| SEQ-167.1 | `VerifyDriver` 重放 | Output 逐字节一致;合成成交恰好入账一次 |
| SEQ-167.2 | 同一报告组的条目被重新投递一次 | `apply_external` 返回 `None`(按 `report_id` 去重),仓位仍 −8 |
| SEQ-167.3 | hft-ledger 的 journal 消费路径读这段 journal | 重建的账本中 b1 仓位为 −8,含这笔合成成交 |

### SEQ-168 AUTO_EXTERNAL 入账后交易所补推同向真实 ADL

- 规则:[每条 Input 的处理顺序](README.md#每条-input-的处理顺序process)
- 级别:unit
- 里程碑:M9
- 前置:SEQ-165.1 之后(`auto_held[(1, b1, Buy)] = 2`)
- 步骤:
  1. 投递 gw 补推的 `ExternalFill{b1, Buy 2, cause Adl}`
- 期望:
  - 同 SEQ-154:`Note{LedgerAutoHeldOffset}`;仓位仍 −8;LegTracker、执行层与策略都无变化
- 实现:`crates/hft-sequencer/tests/recon.rs::seq_168_real_adl_after_auto_external`

### SEQ-169 Advice Input 交给 Advice 路由

- 规则:[每条 Input 的处理顺序](README.md#每条-input-的处理顺序process)、[Advice 路由](README.md#advice-路由)
- 级别:unit
- 里程碑:M2
- 前置:B0 稳态;advice 环一条 `AdviceHdr{strategy 2, account 2, group g2, local_kind 7, len 16}`
- 步骤:
  1. `step()`
- 期望:
  - journal 中该 `Advice` 记录(变长,`AdviceHdr` + 16 B 载荷)原样写入后,s2 收到 `EventRef::Advice{gix: 0, local_kind: 7, payload: 16 B}`
- 实现:`crates/hft-sequencer/tests/advice.rs::seq_169_advice_input_routed`

### SEQ-170 对冲请求被门控时不出环、不计次

- 规则:[意图管线](README.md#意图管线)
- 级别:table
- 里程碑:M2
- 前置:B0 稳态;s1 的 PairOnFill 链接 L(link_id 7):第一腿 a1 买 0.01 成交,OMS 产出第二腿 b1 卖 0.010 的 `HedgeRequest{role: Second}`;每行设定第二腿的条件
- 实现:`crates/hft-sequencer/tests/intents.rs::seq_170_hedge_gated`

| 行 | 条件 | 期望 |
|---|---|---|
| SEQ-170.1 | b1 的 BBO 成员连接全部 `up = false` | `price_hedge` 返回 `StaleQuote`;`oms.hedge_gated(7, HedgeGate::Px(StaleQuote), ..)`;无第二腿 `OrderDecision`、无 `GwCmd`;堆中有 `Timer{Oms, HedgeRetry, link 7}`;链接的重试计次不变 |
| SEQ-170.2 | 对手价越过 `rr_max_cross` | `price_hedge` 返回 `BeyondCross`;`hedge_gated(7, Px(BeyondCross))`;其余同上 |
| SEQ-170.3 | 风控以 `KILLED` 位拒绝(B 被 kill) | 一条 `OrderDecision{reject = KILLED}`,不扣令牌、不计在途、不分配槽;`hedge_gated(7, HedgeGate::Risk(KILLED))`;无 `GwCmd` |
| SEQ-170.4 | 风控以 `HEDGE_LIMIT` 拒绝 | `OrderDecision{reject = HEDGE_LIMIT}`;`oms.hedge_vetoed(7)`(链接 `Broken{HedgeLimit}`);`apply(Auto(Broken{1, g1}))`;一条 P0 注记;无 `GwCmd` |

### SEQ-171 执行层订单被 HEDGE_LIMIT 拒绝时回填并设 Broken

- 规则:[意图管线](README.md#意图管线)
- 级别:unit
- 里程碑:M2
- 前置:B0 稳态;s2 的 g2 补腿单(执行层 `origin = Engine`,`hedge_cap_qty = 0.004`)数量 0.006(执行层按规则不会产生这样的单:测试经 `testing` 钩子 `Engine::test_exec_hedge_cap` 改写下一条 Hedge 类执行层订单的上限,由 `HedgeFix` 命令触发)
- 步骤:
  1. 触发执行层发出该单
- 期望:
  - `OrderDecision{reject = HEDGE_LIMIT}`;`exec.on_place_result(id, Rejected(HEDGE_LIMIT))`;其后 `apply(Auto(Broken{2, g2}))` 恰好一次;一条 P0 注记(执行层 `ExecBroken{HedgeVeto}`);同一 Input 内 `risk.on_legs(2, g2, ..)` 一次;无出环
- 实现:`crates/hft-sequencer/tests/intents.rs::seq_171_exec_order_hedge_limit`

### SEQ-172 对手价超出 max_slip 但未越过 rr_max_cross 时按上限定价出环

- 规则:[意图管线](README.md#意图管线)
- 级别:unit
- 里程碑:M2
- 前置:B0 稳态;b1 的价格带类别 `rr_max_cross = 0.005`;`HedgePlan.max_slip = 0.002`;第二腿 b1 买 0.010;b1 mid 50 000,卖一偏离 mid 0.4%;连接健康
- 步骤:
  1. 投递第一腿成交
- 期望:
  - 同一 Input 内第二腿 `OrderDecision` 放行并出环,限价 = `price_hedge` 的返回值(`capped = true`),不调 `hedge_gated`
- 实现:`crates/hft-sequencer/tests/intents.rs::seq_172_overslip_priced_at_cap`

### SEQ-173 第二腿盘口长时间不变而连接健康时照常对冲

- 规则:[意图管线](README.md#意图管线)、[连接活性](README.md#连接活性)
- 级别:unit
- 里程碑:M2
- 前置:B0 稳态;`conn_liveness_ms = 1 000`;b1 的最后一条 `Bbo` 在 10 s 前,其成员连接每 500 ms 一条 `ConnState{Alive}`
- 步骤:
  1. 投递第一腿 a1 成交
- 期望:
  - 同一 Input 内第二腿 `OrderDecision` 放行并出环;无 `StaleQuote`、无 `hedge_gated`
- 备注:跨组件的完整场景见 QF-1
- 实现:`crates/hft-sequencer/tests/intents.rs::seq_173_quiet_book_still_hedges`

### SEQ-174 模式门只作用于策略意图与执行层的 Target 增仓单

- 规则:[意图管线](README.md#意图管线)
- 级别:table
- 里程碑:M2
- 前置:B0 稳态;每行设定 g1 或 g2 的有效模式与意图来源
- 实现:`crates/hft-sequencer/tests/intents.rs::seq_174_mode_gate`

| 行 | 有效模式 / 意图 | 期望 |
|---|---|---|
| SEQ-174.1 | Shadow / s1 `place` Open | `OrderDecision{flags & SHADOW}`;无 `GwCmd`;`IntentHandle = Shadowed` |
| SEQ-174.2 | Shadow / s1 `place` Close | 同 SEQ-174.1 |
| SEQ-174.3 | Live,g1 `open_allowed = false` / s1 `place` Open | `OrderDecision{reject = MODE_SHADOW}`;无 `GwCmd` |
| SEQ-174.4 | Live,`open_allowed = false` / s1 `place` Close | 不被模式门拦,进 `classify` |
| SEQ-174.5 | Shadow / 执行层 `origin = Target` 增仓单 | `OrderDecision{SHADOW}` |
| SEQ-174.6 | Shadow / 执行层 `origin = Target` 减仓单 | 不被模式门拦 |
| SEQ-174.7 | Shadow / OMS 第二腿 | 不被模式门拦,出环 |
| SEQ-174.8 | Shadow / 执行层 `origin = Engine` 订单 | 不被模式门拦 |
| SEQ-174.9 | 账户 HARD / OMS 第二腿 | 被风控拒绝(HARD) |

### SEQ-175 Shadow 或被拒的意图不扣令牌、不计在途、不分配槽

- 规则:[意图管线](README.md#意图管线)
- 级别:prop
- 里程碑:M5
- 性质:任意意图序列中,每条结果为 Shadowed 或 Rejected 的意图处理前后,hft-risk 令牌桶、OMS `InflightBook` 与已分配槽数逐字段不变
- 生成:B0 上 1–500 条策略意图,有效模式 ∈ {Live, CloseOnly, Shadow} 随机切换,令牌桶 0–10 个令牌,数量 0.0001–1
- 参考:处理前后三项状态的快照相等(对 Shadowed/Rejected 的意图)
- 例数:PR 1 000;每夜 50 000
- 容量回归:订单槽满时写单条 `CAPACITY` 决策,slot/gen 为 0,不出环、不增加在途、不扣令牌;从此前快照回放同样拒绝。
- 实现:`crates/hft-sequencer/tests/intents.rs::seq_175_capacity_rejection_is_journaled_and_replayable`

### SEQ-176 带 LegLink 的第一腿 Open 投影 link 字段

- 规则:[意图管线](README.md#意图管线)
- 级别:table
- 里程碑:M2
- 前置:B0 稳态;b1 mid 50 000;b1 价格带类别 `rr_max_cross = 0.005`;`HedgePlan.ratio = 1`;s1 发第一腿 a1 的意图 0.01
- 实现:`crates/hft-sequencer/tests/intents.rs::seq_176_link_fields`

| 行 | 第二腿方向 | 期望 `IntentView` |
|---|---|---|
| SEQ-176.1 | b1 卖 | `link_venue = B`、`link_account = 1`、`link_inst = b1`、`link_side = Sell`、`link_qty = 0.010`、`link_notional = 0.010 × 50 000 × 1.005 = 502.5 USDT` |
| SEQ-176.2 | b1 买 | 同上,`link_side = Buy`,`link_notional = 502.5 USDT` |
| SEQ-176.3 | `HedgePlan.ratio = 0.5` | `link_qty = 0.005`、`link_notional = 251.25 USDT` |

### SEQ-177 以同一 mid 定价的第二腿名义不超过 link_notional

- 规则:[意图管线](README.md#意图管线)
- 级别:prop
- 性质:第二腿 mid 不变时,`price_hedge` 给出的第二腿限价 × 数量 ≤ 第一腿 Open 时投影的 `link_notional`;若第一腿以 `link_notional ≤ max_hedge_notional` 放行,第二腿不被 #6 以名义拒绝
- 生成:mid 1–100 000;`rr_max_cross` 0.0005–0.02;对手价偏离 mid −3%…+3%;方向 Buy/Sell;`ratio` 0.1–2;数量 1–10 000 lot
- 参考:按 `cap_cross_px` 的纯函数逐例计算
- 例数:PR 5 000;每夜 500 000
- 里程碑:M2
- 实现:`crates/hft-sequencer/tests/intents.rs::seq_177_second_leg_within_link_notional`

### SEQ-178 Simultaneous 第二腿与 Hedge 类意图的投影字段

- 规则:[意图管线](README.md#意图管线)
- 级别:table
- 里程碑:M2
- 前置:B0 稳态
- 实现:`crates/hft-sequencer/tests/intents.rs::seq_178_peer_and_hedge_cap`

| 行 | 意图 | 期望 `IntentView` |
|---|---|---|
| SEQ-178.1 | Simultaneous 第二腿(`HedgeRequest.peer_venue = A`、`peer_account = 1`) | `peer_venue = A`、`peer_account = 1` |
| SEQ-178.2 | OMS `HedgeRequest{hedge_cap_qty = 0.011}` | `hedge_cap_qty = 0.011` |
| SEQ-178.3 | 执行层 `ExecOrder{hedge_cap_qty = 0.004}` | `hedge_cap_qty = 0.004` |
| SEQ-178.4 | 带 `OrderFlags::SMOKE` 的冒烟单 | `flags & SMOKE ≠ 0` |

### SEQ-179 放行后按账户类型出环,永续 Open 带 lev_x100

- 规则:[意图管线](README.md#意图管线)、[模拟账户](README.md#模拟账户)
- 级别:table
- 里程碑:M2
- 前置:B0 稳态;每行一笔放行的意图
- 实现:`crates/hft-sequencer/tests/intents.rs::seq_179_commit_place_route`

| 行 | 意图 | 期望 |
|---|---|---|
| SEQ-179.1 | s1(Live 账户 1)a1 买 | `risk.commit` → `oms.place` → `OrderDecision` → out-A 一条 `GwCmd`;out-sim 为空 |
| SEQ-179.2 | s9(Sim 账户 9)a1 买 | out-sim 一条 `GwCmd`;out-A 为空 |
| SEQ-179.3 | s1 b1 永续 Open,`Risk::leverage(1, b1) = 12.349 倍` | `ApprovedIntent.lev_x100 = 1234` |
| SEQ-179.4 | s1 b1 永续 Open,杠杆未知 | `lev_x100 = 0` |
| SEQ-179.5 | s1 a1 现货买 | `lev_x100 = 0` |

### SEQ-180 意图结果回填到发起方

- 规则:[意图管线](README.md#意图管线)
- 级别:table
- 里程碑:M2
- 前置:B0 稳态
- 实现:`crates/hft-sequencer/tests/intents.rs::seq_180_result_backfill`

| 行 | 来源与结果 | 期望 |
|---|---|---|
| SEQ-180.1 | 策略意图放行 | 同一 Input 内 `IntentHandle` 结果 `Placed` |
| SEQ-180.2 | 策略意图被拒(bits = `RATE`) | `Rejected(RATE)` |
| SEQ-180.3 | 策略意图 Shadow | `Shadowed` |
| SEQ-180.4 | 执行层订单放行 | `exec.on_place_result(id, Placed(slot))` |
| SEQ-180.5 | 冒烟单放行 | 该命令的 `CmdResult{Applied}`,同一事件内有该单的 `OrderDecision`,`trigger_seq` = 命令 seq |

### SEQ-181 InflightBook.open_margin 的增量维护与逐单求和一致

- 规则:[意图管线](README.md#意图管线)、[只读视图](README.md#只读视图seqviews实现-sdk-views)
- 级别:prop
- 里程碑:M2
- 性质:任意永续 Open 下单、成交、撤单序列之后,`InflightBook.open_margin(a, v)` 等于对全部在途永续 Open 槽按其 `lev_x100` 逐单求和的结果
- 生成:B0 上 1–300 笔 b1 永续 Open,杠杆 1–20 倍随机,回报从 {Ack, PartialFill, Fill, Cancel} 随机抽取
- 参考:逐单求和的参考实现
- 例数:PR 1 000;每夜 50 000
- 实现:`crates/hft-sequencer/tests/intents.rs::seq_181_open_margin_incremental`

### SEQ-182 目标意图以发出它的实例为 owner 交给执行层

- 规则:[目标意图与执行层](README.md#目标意图与执行层)
- 级别:table
- 里程碑:M2
- 前置:B0 稳态
- 实现:`crates/hft-sequencer/tests/exec_glue.rs::seq_182_target_owner`

| 行 | 动作 | 期望 |
|---|---|---|
| SEQ-182.1 | s2 `target(account 2, group g2)` | `exec.set_target(owner = 2, ..)`;同一 Input 内 `exec.advance` 推进并输出 `ExecDecision` 与切片 |
| SEQ-182.2 | s2 `target(account 1, group g1)`(不是自己的账户) | 执行层拒绝:`ExecDecision{TargetRejected}`;s2 收到拒绝通知;无切片 |
| SEQ-182.3 | s2 `clear_target(2, g2)` | `exec.clear_target(2, 2, g2, ..)`;已有仓位不变 |

### SEQ-183 有效模式变化调 exec.on_mode

- 规则:[目标意图与执行层](README.md#目标意图与执行层)、[模式与 cap 的驱动](README.md#模式与-cap-的驱动)
- 级别:unit
- 里程碑:M2
- 前置:B0 稳态;g2 有效模式 Live
- 步骤:
  1. 投递 `SetMode{(2, 2, g2) → CloseOnly}`
- 期望:
  - `exec.on_mode(2, g2, Live, CloseOnly, now)` 一次;s2 收到 `EventRef::Mode{old: Live, new: CloseOnly}`
- 实现:`crates/hft-sequencer/tests/exec_glue.rs::seq_183_on_mode`

### SEQ-184 L3 引擎目标的 cancel_resting 转成 cancel_resting_on

- 规则:[目标意图与执行层](README.md#目标意图与执行层)、[每条 Input 的处理顺序](README.md#每条-input-的处理顺序process)
- 级别:unit
- 里程碑:M4
- 前置:B0 稳态;s2 在 b1 上挂一张 reduce-only GTC 买单(非 EXEC 槽);账户 2 的 `MarginReport` 使 hft-risk 进入 L3 并产出 `force`
- 步骤:
  1. 投递该 `MarginReport`
- 期望:
  - 同一 Input 内:`exec.force_deleverage(2, B, ..)`;其 `ExecOut.cancel_resting` 被转成 `oms.cancel_resting_on(2, b1, ..)`;出环一条撤该 GTC 单的 `GwCmd{Cancel}`;`ExecDecision` 记录引擎目标 `reason = MarginStress`
- 实现:`crates/hft-sequencer/tests/exec_glue.rs::seq_184_cancel_resting_on`

### SEQ-185 Kill{Account, flatten} 调 engine_flatten,覆盖 Broken 的 group

- 规则:[目标意图与执行层](README.md#目标意图与执行层)、[命令路径](README.md#命令路径)
- 级别:unit
- 里程碑:M2
- 前置:B0 稳态;g2 处于 `Broken`(现货 a1 +0.010、永续 b1 −0.005)
- 步骤:
  1. 投递已确认的 `Kill{Account(2), flatten}`(Trader)
- 期望:
  - `CmdResult{Applied}`;`exec.engine_flatten(2, ..)` 一次;同一 Input 内先出环 b1 的 reduce-only 买单(永续腿先),现货腿在永续腿到达后才发
- 实现:`crates/hft-sequencer/tests/exec_glue.rs::seq_185_engine_flatten_covers_broken`

### SEQ-186 HedgeFix 命令经执行层出单

- 规则:[目标意图与执行层](README.md#目标意图与执行层)、[命令路径](README.md#命令路径)
- 级别:unit
- 里程碑:M2
- 前置:B0 稳态;g1 `Broken`,多出现货 a1 +0.004
- 步骤:
  1. 投递 `HedgeFix{account 1, group g1}`
- 期望:
  - `exec.hedge_fix_cmd(1, g1, ..)` 一次;其订单(a1 卖 0.004)进意图管线并出环;`CmdResult{Applied}`
- 实现:`crates/hft-sequencer/tests/exec_glue.rs::seq_186_hedge_fix_cmd`

### SEQ-187 Draining 期间不开始新的增仓切片

- 规则:[目标意图与执行层](README.md#目标意图与执行层)、[Drain](README.md#drain)
- 级别:unit
- 里程碑:M2
- 前置:B0 稳态;s2 的 g2 目标 +0.05,已完成 +0.02;另一 cell 的目标是减仓
- 步骤:
  1. 投递 `Drain{cancel_resting: false}`
  2. 投递 10 条 bbo
- 期望:
  - 步骤 2 中没有任何 g2 增仓切片的 `OrderDecision`;减仓 cell 的切片照常出环
- 实现:`crates/hft-sequencer/tests/exec_glue.rs::seq_187_draining_no_increase`

### SEQ-188 Advice 路由检查

- 规则:[Advice 路由](README.md#advice-路由)
- 级别:table
- 里程碑:M2
- 前置:B0 稳态;s2 的 g2 `spec_seq = 1 000`;每行一条 advice
- 实现:`crates/hft-sequencer/tests/advice.rs::seq_188_advice_routing_checks`

| 行 | AdviceHdr | 期望 |
|---|---|---|
| SEQ-188.1 | `strategy 5`(不存在) | 丢弃;`Note{Sequencer, AdviceMisrouted}` |
| SEQ-188.2 | `strategy 2, len 4 097` | 丢弃;`AdviceMisrouted` |
| SEQ-188.3 | `strategy 2, len 4 096` | 投递 |
| SEQ-188.4 | `strategy 2, group GROUP_ALL` | 丢弃;`AdviceMisrouted` |
| SEQ-188.5 | `strategy 2, group 0x0100_0001`(高 8 位 = 1 ≠ 2) | 丢弃;`AdviceMisrouted` |
| SEQ-188.6 | `strategy 2, account 1`(账户 1 绑定 s1) | 丢弃;`AdviceMisrouted` |
| SEQ-188.7 | `strategy 2, account 0, group g2, basis_seq 1 000` | 投递 `EventRef::Advice{gix: 0}` |
| SEQ-188.8 | `strategy 2, group 0x0200_0009`(高 8 位 = 2,s2 当前无此 group) | 丢弃;`Note{Sequencer, AdviceStale}` |
| SEQ-188.9 | `strategy 2, group g2, basis_seq 999` | 丢弃;`AdviceStale` |
| SEQ-188.10 | `strategy 2, group GROUP_NONE, basis_seq 1` | 投递 `gix = GIX_NONE`(不做 `spec_seq` 检查) |
| SEQ-188.11 | 任意一行 | 只投递给 s2;s1、s9 收不到 |

### SEQ-189 spec_seq 只在 group 指纹变化或新增时更新

- 规则:[Advice 路由](README.md#advice-路由)、[命令路径](README.md#命令路径)
- 级别:table
- 里程碑:M2
- 前置:B0 稳态;g2 无持仓,`spec_seq = 1 000`;每行投递一条 `SetParams{s2}`,其记录 seq 为 2 000
- 实现:`crates/hft-sequencer/tests/advice.rs::seq_189_spec_seq_updates`

| 行 | 参数变化 | 期望 |
|---|---|---|
| SEQ-189.1 | 只改阈值类字段(指纹不变) | g2 `spec_seq = 1 000`;此后 `basis_seq 1 500` 的 advice 投递 |
| SEQ-189.2 | g2 的现货腿从 a1 换到另一 venue 的同资产 inst(指纹变化) | g2 `spec_seq = 2 000`;`basis_seq 1 500` 的 advice 丢弃并写 `AdviceStale`;`basis_seq 2 000` 的投递 |
| SEQ-189.3 | 新增 group `0x0200_0002` | 该 group `spec_seq = 2 000` |

### SEQ-190 spec_seq 进节 5,从关键帧起步的验证回放丢弃同样的 Advice

- 规则:[Advice 路由](README.md#advice-路由)、[快照](README.md#快照)
- 级别:unit
- 里程碑:M2
- 前置:SEQ-189.2 之后写关键帧 K,其后有 3 条 `basis_seq 1 500` 与 2 条 `basis_seq 2 100` 的 advice
- 步骤:
  1. `VerifyDriver` 从 K 起步重放
- 期望:
  - 节 5 中 g2 的 `GroupSeqRow.spec_seq = 2 000`;重放中 3 条被丢弃并写 `AdviceStale`,2 条投递;Output 与实盘逐字节相同
- 实现:`crates/hft-sequencer/tests/advice.rs::seq_190_spec_seq_in_snapshot`

### SEQ-191 引擎不给 Advice 加过期标志,也不按时长丢弃

- 规则:[Advice 路由](README.md#advice-路由)
- 级别:table
- 里程碑:M2
- 前置:B0;重启后 `boot_seq = 5 000`
- 实现:`crates/hft-sequencer/tests/advice.rs::seq_191_no_engine_staleness`

| 行 | Advice | 期望 |
|---|---|---|
| SEQ-191.1 | `basis_seq 4 800`(检查点续算的追赶期) | 原样投递,`basis_seq = 4 800`、`basis_tsc` = 头部值;无 Note |
| SEQ-191.2 | `basis_tsc` 比 `now.tsc` 早 60 s | 原样投递 |
| SEQ-191.3 | 回放同一 journal | 投递的 `basis_seq`/`basis_tsc` 与 advisor 写入的逐字节相同 |

### SEQ-192 next_funding_settle 取需合成 venue 上有持仓、未结算 inst 的最小结算时刻

- 规则:[资金费合成](README.md#资金费合成)
- 级别:table
- 里程碑:M2
- 前置:B0 稳态;另有 venue C(`VenueId 3`,USDT 永续)与 inst `c1`;每行设定能力与持仓
- 实现:`crates/hft-sequencer/tests/funding.rs::seq_192_next_funding_settle`

| 行 | 设定 | 期望 `next_funding_settle()` |
|---|---|---|
| SEQ-192.1 | B `RateOnly`,b1 有持仓、`next_funding_ns = 08:00`;C `BalanceOnlyPush`,c1 有持仓、`next_funding_ns = 04:00` | `(C, 04:00)` |
| SEQ-192.2 | 同上,但 c1 无持仓 | `(B, 08:00)` |
| SEQ-192.3 | B `PerSymbolPush`,b1 有持仓;无其他 | `None` |
| SEQ-192.4 | B `RateOnly`,b1 已结算 08:00(`settled_cut_ns = 08:00`),`next_funding_ns` 仍为 08:00 | `None` |

### SEQ-193 定时触发的资金费合成

- 规则:[资金费合成](README.md#资金费合成)
- 级别:unit
- 里程碑:M2
- 前置:B0 稳态;B `RateOnly`、`funding_grid` 为 8 h;账户 1 持有 b1 空 0.010、账户 2 持有 b1 多 0.020;b1 `mark_px = 50 000`、`funding_rate = 0.0001`、`next_funding_ns = 08:00`
- 步骤:
  1. 交出 `Timer{Sys, FundingSettle, venue B}`
- 期望:
  - 按 (account, inst) 升序:`ledger.synth_funding(1, b1, 50 000, 0.0001, 08:00)` → Output `FundingSettle{synth = 1, account 1, settle_ts_ns = 08:00}` → `ledger.apply_funding` → `risk.on_pnl`;随后账户 2 同样一组;金额绝对值分别为 0.05 与 0.10 USDT(方向按 hft-ledger)
  - b1 行 `settled_cut_ns = 08:00`
- 实现:`crates/hft-sequencer/tests/funding.rs::seq_193_timer_triggered_synthesis`

### SEQ-194 定时与 Funding 事件两种先后顺序都只合成一次

- 规则:[资金费合成](README.md#资金费合成)
- 级别:table
- 里程碑:M2
- 前置:同 SEQ-193
- 实现:`crates/hft-sequencer/tests/funding.rs::seq_194_synthesis_once`

| 行 | 顺序 | 期望 |
|---|---|---|
| SEQ-194.1 | `Timer{FundingSettle}` 先,然后 `Funding{b1, next 16:00}` | 合成只在 Timer 事件内发生一次;`Funding` 事件内没有 `FundingSettle{synth = 1}` |
| SEQ-194.2 | `Funding{b1, next 16:00}` 先(把 `next_funding_ns` 推过未结算的 08:00) | 合成在 `Funding` 事件内、`MarketLast` 更新之前发生一次;之后的 `Timer{FundingSettle}` 不再合成 |
| SEQ-194.3 | `Funding{b1, next 08:00}`(未推过) | 不合成 |

### SEQ-195 不需合成的 venue 不合成

- 规则:[资金费合成](README.md#资金费合成)
- 级别:unit
- 里程碑:M2
- 前置:B0 稳态;B `PerSymbolPush`;账户 1 持有 b1
- 步骤:
  1. 投递 `Funding{b1, next 16:00}`(越过 08:00)
- 期望:
  - 没有 `FundingSettle{synth = 1}`;驱动不写 `Timer{FundingSettle}`
- 实现:`crates/hft-sequencer/tests/funding.rs::seq_195_per_symbol_push_not_synthesized`

### SEQ-196 合成记录的结算时刻是归一化后的值

- 规则:[资金费合成](README.md#资金费合成)
- 级别:unit
- 里程碑:M9
- 前置:B0 稳态;B `BalanceOnlyPush`、`funding_grid` 8 h、锚 0;b1 `next_funding_ns = 08:00:00.003`(交易所给出的时刻带 3 ms 偏差)
- 步骤:
  1. 触发合成;再投递推送 `FundingSettle{inst = INST_NONE, ts 08:00:02}`
- 期望:
  - Output `FundingSettle{synth = 1}.settle_ts_ns = 08:00:00.000`;推送按 `funding_settle_norm_ns` 归一化为 08:00 后与合成记录配对,钱包只入一次
- 实现:`crates/hft-sequencer/tests/funding.rs::seq_196_normalized_settle_time`

### SEQ-197 Input 翻成 ModeInput

- 规则:[模式与 cap 的驱动](README.md#模式与-cap-的驱动)
- 级别:table
- 里程碑:M5
- 前置:B0 稳态;每行投递所列 Input,记录 `risk.mode.apply` 收到的 `ModeInput`
- 实现:`crates/hft-sequencer/tests/mode.rs::seq_197_mode_input_mapping`

| 行 | Input | 期望 `ModeInput` |
|---|---|---|
| SEQ-197.1 | `SetMode{(1, 1, g1) → CloseOnly}` | `SetMode{scope, CloseOnly}` |
| SEQ-197.2 | `Kill{Account(1)}` | `Kill(KillCmd{..})` |
| SEQ-197.3 | `Resume{Kill, Account(1)}` | `Resume{source: Kill, scope}` |
| SEQ-197.4 | 链接 `Broken{Deadline}` | `Auto(CapRecord{source: Broken, ..})` |
| SEQ-197.5 | 策略 `request_halt(g1)` | `Auto(CapRecord{source: Halted, ..})` |
| SEQ-197.6 | `Timer{ClockBad}` | `Clock{ok: false}` |
| SEQ-197.7 | `Timer{DiskLow, level 2}` | `Storage{DiskLow, 2}` |
| SEQ-197.8 | `VenueNotice{B, b1, Delisting}` | `Notice(..)` |
| SEQ-197.9 | gw 主下单连接 `ConnState{Up}` | `Gate(OrderConns{..})` |
| SEQ-197.10 | `AdoptExchange` 处置 | `Gate(Disposed{.., how: Adopted})` |
| SEQ-197.11 | g1 两腿成交后变为 flat | `CellFlat{cell: (1, 1, g1), flat: true}` |
| SEQ-197.12 | `SetParams` 新增一个 group、移除一个 | `GroupsChanged{added: [..1 项], removed: [..1 项]}` |
| SEQ-197.13 | 恢复结束 | 每个账户 `Auto(Recovery{account})` 与 `RecoveryFlags{account, flags}` |

### SEQ-198 ① 有效模式变化投递 Mode 事件,进出 Shadow 时同步 ledger

- 规则:[模式与 cap 的驱动](README.md#模式与-cap-的驱动)
- 级别:table
- 里程碑:M2
- 前置:B0 稳态;g1 无仓位;每行投递一条命令
- 实现:`crates/hft-sequencer/tests/mode.rs::seq_198_cells_changes`

| 行 | 命令 | 期望 |
|---|---|---|
| SEQ-198.1 | `SetMode{(1, 1, g1) → Shadow}` | s1 收到 `EventRef::Mode{old: Live, new: Shadow}`;`ledger.set_cell_shadow((1, 1, g1), true)` |
| SEQ-198.2 | 接着 `SetMode{→ Live}` | `Mode{Shadow → Live}`;`set_cell_shadow(.., false)` |
| SEQ-198.3 | `Kill{Account(1)}`(非 HARD) | `Mode{Live → CloseOnly}`;不调 `set_cell_shadow` |

### SEQ-199 ① 推迟落 Shadow 写 ShadowDeferred

- 规则:[模式与 cap 的驱动](README.md#模式与-cap-的驱动)
- 级别:unit
- 里程碑:M2
- 前置:B0 稳态;g1 有仓位(a1 +0.010、b1 −0.010)
- 步骤:
  1. 投递 `SetMode{(1, 1, g1) → Shadow}`
  2. 两腿平到 0
- 期望:
  - 步骤 1:`CmdResult{Applied, detail = ShadowDeferred}`;有效模式 `CloseOnly{deferred_shadow: true}`;`Note{Sequencer, ShadowDeferred}` `v[0] = 1 | 1 << 8`、`v[1] = g1`、`v[2] = 0`
  - 步骤 2:落到 Shadow 的那个 Input 内一条 `ShadowDeferred`,`v[2] = 1`
- 实现:`crates/hft-sequencer/tests/mode.rs::seq_199_shadow_deferred_notes`

### SEQ-200 ② 撤单范围与 open_only 原样转交 OMS

- 规则:[模式与 cap 的驱动](README.md#模式与-cap-的驱动)
- 级别:table
- 里程碑:M2
- 前置:B0 稳态;账户 1 在 A、B 各有 Open 挂单 2 张与 RISK_REDUCING 挂单 1 张
- 实现:`crates/hft-sequencer/tests/mode.rs::seq_200_cancels`

| 行 | 命令 | 期望 |
|---|---|---|
| SEQ-200.1 | `SetMode{(1, 1, g1) → CloseOnly}` | `oms.cancel_open(CellKey{1, 1, g1}, ..)` 一次 |
| SEQ-200.2 | `Kill{Account(1)}`(非 HARD) | `oms.cancel_all(Account(1), open_only = true, ..)` |
| SEQ-200.3 | `Kill{Account(1), hard}` | `oms.cancel_all(Account(1), open_only = false, ..)` |

### SEQ-201 ③ kill 状态变化由实盘驱动写进各 gw 的 KillWord

- 规则:[模式与 cap 的驱动](README.md#模式与-cap-的驱动)、[模拟账户](README.md#模拟账户)
- 级别:table
- 里程碑:M2
- 前置:B0 稳态;gw-A、gw-B 各一个 `KillWord`;账户 9 是 Sim
- 实现:`crates/hft-sequencer/tests/mode.rs::seq_201_kill_words`

| 行 | 动作 | 期望 |
|---|---|---|
| SEQ-201.1 | `Kill{Account(1)}` | 同一 Input 内两个 `KillWord` 以 `Release` 写入 `risk.mode.kill_bits()` 的对应值;`kill_changed = true` |
| SEQ-201.2 | `Kill{Account(9)}` | 两个 `KillWord` 不变;s9 的 Open 被检查链拒绝 |
| SEQ-201.3 | `VerifyDriver` 重放 SEQ-201.1 | 不写任何 `KillWord`;Output 逐字节相同 |

### SEQ-202 ④ 删除带 HARD 的 Kill 记录时按候选逐个判定 Broken

- 规则:[模式与 cap 的驱动](README.md#模式与-cap-的驱动)
- 级别:table
- 里程碑:M2
- 前置:B0,账户 2 有 `Kill{Account(2), hard}`;s2 有 5 个 group,`oms.naked_groups` 列出 g2a–g2e;每行是该 group 的状态
- 实现:`crates/hft-sequencer/tests/mode.rs::seq_202_resolve_broken_on_hard_release`

| 行 | group 状态 | 期望(`Resume{Kill, Account(2)}` 删除 HARD 记录的同一 Input 内) |
|---|---|---|
| SEQ-202.1 | 无活动链接,已成交裸量超过补腿阈值(裸量 > `tol` 且名义 > 尘埃线) | `exec.resolve_broken(2, g, ..)` 为真 → `apply(Auto(Broken{2, g}))`,随后 `risk.on_legs(2, g, ..)`;无任何订单 |
| SEQ-202.2 | 裸量在容差内 | 不设 Broken |
| SEQ-202.3 | 名义 ≤ 尘埃线 | 不设 Broken |
| SEQ-202.4 | cell 处于 `manual_hold` | 不设 Broken |
| SEQ-202.5 | 有 `Hedging` 链接 | 不设 Broken(由 OMS 按 `hedge_deadline_ms` 判定) |
| SEQ-202.6 | 全部行 | 没有任何 `HedgeFix` 订单或 `ExecDecision{HedgeFix}` |

### SEQ-203 ④ 只有删除 HARD 的 Kill 记录才产出 resolve_broken

- 规则:[模式与 cap 的驱动](README.md#模式与-cap-的驱动)
- 级别:table
- 里程碑:M2
- 前置:B0 稳态;账户 2 有一个 group 带超过补腿阈值的裸量;每行解除所列 cap
- 实现:`crates/hft-sequencer/tests/mode.rs::seq_203_resolve_broken_only_hard`

| 行 | 解除 | 期望 |
|---|---|---|
| SEQ-203.1 | 非 HARD 的 `Resume{Kill}` | 不调 `oms.naked_groups`;无 `Broken` |
| SEQ-203.2 | `Resume{DailyLoss}` | 同上 |
| SEQ-203.3 | Recovery 解除(启动闸门放行) | 同上 |
| SEQ-203.4 | `Resume{Kill}` 删除 HARD 记录 | 调 `oms.naked_groups` |

### SEQ-204 ④ naked_groups 超过 64 个时游标续取、不漏不重

- 规则:[模式与 cap 的驱动](README.md#模式与-cap-的驱动)、[通知队列与续推](README.md#通知队列与续推)
- 级别:table
- 里程碑:M5
- 前置:B0 改 `max_cells_total = 256`、`max_groups_per_instance = 200`;账户 2 有 n 个带裸量的 group,全部满足 SEQ-202.1
- 实现:`crates/hft-sequencer/tests/mode.rs::seq_204_naked_groups_cursor`

| 行 | n | 期望 |
|---|---|---|
| SEQ-204.1 | 64 | 删除 HARD 的 Input 内判定全部 64 个;`wants_continue()` 为假 |
| SEQ-204.2 | 65 | 本 Input 判定 64 个;下一轮 `Timer{Sys, NotifyContinue}` 判定第 65 个 |
| SEQ-204.3 | 150 | 共 3 个 Input 判定完(64、64、22),每个 group 恰好一次 `exec.resolve_broken`;回放逐字节一致 |

### SEQ-205 ⑤ 对冲闸门子集满足时续接 RESUME_HOLD 链接

- 规则:[模式与 cap 的驱动](README.md#模式与-cap-的驱动)、[冷启动恢复](README.md#冷启动恢复recoverdriver)
- 级别:unit
- 里程碑:M5
- 前置:恢复后;账户 1 链接 L(第二腿在 B)处于 `RESUME_HOLD`;账户 1 启动闸门未放行
- 步骤:
  1. 投递使 hft-risk `hedge_gate_ok(1, B)` 由假变真的 `AccountConfigReport{1, B}`
- 期望:
  - `ModeOutcome.hedge_gate_open` 含 (1, B);同一 Input 内 `oms.on_hedge_gate_open(1, B, now, seq, ..)` 一次;L 的 `SecondRetry` 进意图管线并出环;账户 1 仍未放行
- 实现:`crates/hft-sequencer/tests/mode.rs::seq_205_hedge_gate_open`

### SEQ-206 ⑤ 恢复期按 venue 放行时调 exec.on_venue_gate_open

- 规则:[模式与 cap 的驱动](README.md#模式与-cap-的驱动)、[冷启动恢复](README.md#冷启动恢复recoverdriver)
- 级别:unit
- 里程碑:M5
- 前置:恢复后;账户 2 的 g2 多出永续腿在 B;A 断线(账户 2 未放行);B 的 `hedge_gate_ok` 已满足,`RECONCILED` 在 B 尚未置位
- 步骤:
  1. 投递使 B 的 `RECONCILED` 置位的对账报告组末条
- 期望:
  - `ModeOutcome.venue_gate_open` 含 (2, B);同一 Input 内 `exec.on_venue_gate_open(2, B, now)` 一次;不调 `exec.on_gate_open`
- 实现:`crates/hft-sequencer/tests/mode.rs::seq_206_venue_gate_open`

### SEQ-207 ⑤ 启动闸门放行时调 exec.on_gate_open

- 规则:[模式与 cap 的驱动](README.md#模式与-cap-的驱动)、[启动闸门](README.md#启动闸门)
- 级别:unit
- 里程碑:M5
- 前置:恢复后;账户 1 只差 `FEEDS` 一位
- 步骤:
  1. 投递使 `FEEDS` 置位的 Input
- 期望:
  - `ModeOutcome.released` 含 1;同一 Input 内 `exec.on_gate_open(1, now)` 一次;`Note{StartupGate}` `v[0] = 1`、`v[3] = 1`;无 `Broken`
- 实现:`crates/hft-sequencer/tests/mode.rs::seq_207_gate_open_released`

### SEQ-208 ⑥ risk_events 投递给绑定实例并交执行层

- 规则:[模式与 cap 的驱动](README.md#模式与-cap-的驱动)
- 级别:unit
- 里程碑:M2
- 前置:B0 稳态
- 步骤:
  1. 投递 `Kill{Account(2)}`
- 期望:
  - `ModeOutcome.risk_events` 的 `CapSet` 事件:s2 收到 `EventRef::Risk{a: 2}`;`exec.on_risk(2, &e, now)` 一次;g2 有效模式变化另调 `exec.on_mode`;s1 不收到
- 实现:`crates/hft-sequencer/tests/mode.rs::seq_208_risk_events`

### SEQ-209 ⑦ 内部 Drain 的两种原因

- 规则:[模式与 cap 的驱动](README.md#模式与-cap-的驱动)、[Drain](README.md#drain)
- 级别:table
- 里程碑:M5
- 前置:B0 稳态,账户 1 已放行
- 实现:`crates/hft-sequencer/tests/mode.rs::seq_209_internal_drain_causes`

| 行 | Input | 期望 |
|---|---|---|
| SEQ-209.1 | `Timer{PersistLag, level 2}`,无未到达的引擎减仓目标 | 同一 Input 内设 `Draining{cancel_resting = true}`、venue 级 `CancelAll`;Drain 结束时 `Done{restart: false, cause: StorageCritical}` |
| SEQ-209.2 | `AccountConfigReport{1, B, mismatches = UID}`(放行后) | 账户 1 HARD;`Note{Sequencer, IdentityMismatch}` P0,`v[0] = 2 \| 1 << 8`;内部 Drain;`Done{restart: false, cause: IdentityMismatch}` |
| SEQ-209.3 | 同 SEQ-209.2,但发生在闸门放行前 | 同 SEQ-209.2 |

### SEQ-210 ⑧ 闸门位变化与 cap 注记由 sequencer 写出

- 规则:[模式与 cap 的驱动](README.md#模式与-cap-的驱动)、[启动闸门](README.md#启动闸门)
- 级别:unit
- 里程碑:M5
- 前置:恢复后,账户 1 `gate_bits = 0b0101`
- 步骤:
  1. 投递使 `gate_bits` 变为 `0b0111` 的 Input
- 期望:
  - 一条 `Note{Sequencer, StartupGate}`:`v[0] = 1`、`v[1] = 0b0101`、`v[2] = 0b0111`、`v[3] = 0`;`ModeOutcome.notes` 中的 `CapSet`/`CapCleared`/`RecoveryHeldSim` 原样追加
- 实现:`crates/hft-sequencer/tests/mode.rs::seq_210_gate_change_notes`

### SEQ-211 VenueNotice 另把 Notice 投递给订阅该 inst 的实例

- 规则:[模式与 cap 的驱动](README.md#模式与-cap-的驱动)、[命令路径](README.md#命令路径)
- 级别:unit
- 里程碑:M2
- 前置:B0 稳态;s1 订阅 b1,s2 不订阅 b1(只订阅 a1)
- 步骤:
  1. 投递 `VenueNotice{B, b1, Delisting, a = 下架时刻}`
- 期望:
  - `apply(Notice)` → `Delisting` cap;节 24 新增 `VenueNoticeRow{venue: 2, kind: 1, inst: b1}`;s1 收到 `EventRef::Notice{inst: b1}`;s2 收不到
- 实现:`crates/hft-sequencer/tests/mode.rs::seq_211_venue_notice_delivery`

### SEQ-212 ModeOutcome 各项按 ①–⑧ 的顺序执行

- 规则:[模式与 cap 的驱动](README.md#模式与-cap-的驱动)
- 级别:unit
- 里程碑:M5
- 前置:B0 稳态;构造一次 `apply`,其 `ModeOutcome` 同时含 cells、cancels、kill_changed、hedge_gate_open、released、risk_events、gate_changes、notes
- 步骤:
  1. 投递触发它的 Input,记录下游调用序列
- 期望:
  - 顺序:Mode 事件入队与 `set_cell_shadow` → `oms.cancel_*` → 写 `KillWord` → `oms.on_hedge_gate_open` → `exec.on_gate_open` → `EventRef::Risk` 入队与 `exec.on_risk` → `Note{StartupGate}` 与 cap 注记追加
- 实现:`crates/hft-sequencer/tests/mode.rs::seq_212_outcome_execution_order`

### SEQ-213 撤单持续义务:断线重连与 Unknown 收敛时重发撤单

- 规则:[模式与 cap 的驱动](README.md#模式与-cap-的驱动)
- 级别:table
- 里程碑:M2
- 前置:B0 稳态;账户 1 在 B 有 3 张挂单;每行先生效所列 cap,再依次投递 B 的 `ConnState{Up}` 与一个槽由 Unknown 收敛为 Live 的回报
- 实现:`crates/hft-sequencer/tests/mode.rs::seq_213_cancel_obligation`

| 行 | 生效的 cap | 期望 |
|---|---|---|
| SEQ-213.1 | `Kill{Account(1), cancel_resting}`(非 HARD) | 两次各调 `oms.reissue_cancels(scope, open_only = true, ..)` |
| SEQ-213.2 | `Kill{Account(1), hard, cancel_resting}` | 两次 `reissue_cancels(.., open_only = false, ..)` |
| SEQ-213.3 | `Draining{cancel_resting = true}`(`params[0] = 1`) | 两次 `reissue_cancels(.., open_only = false, ..)` |
| SEQ-213.4 | `Draining{cancel_resting = false}` | 不调 `reissue_cancels` |
| SEQ-213.5 | 无 cap | 不调 `reissue_cancels` |

### SEQ-214 模拟账户的订单只进 out-sim,回报经 rep-sim 记为 Input

- 规则:[模拟账户](README.md#模拟账户)
- 级别:unit
- 里程碑:M2
- 前置:B0 稳态;testkit 的模拟 gw 读 out-sim、写 rep-sim
- 步骤:
  1. s9 下 a1 买单;模拟 gw 回 `OrderEvent{Ack}` 与 `Fill`
  2. `VerifyDriver` 重放
- 期望:
  - out-A 与 out-B 为空;out-sim 一条 `GwCmd`;两条回报作为 Input 记入 journal;验证逐字节通过
- 实现:`crates/hft-sequencer/tests/sim_route.rs::seq_214_sim_orders_out_sim`

### SEQ-215 模拟账户不登记 DeadMan、ConnRotate 与 SessionKeepalive

- 规则:[模拟账户](README.md#模拟账户)、[定时器](README.md#定时器)
- 级别:unit
- 里程碑:M2
- 前置:B0;启动后堆中的周期性系统定时器
- 步骤:
  1. 枚举堆
- 期望:
  - 账户 1、2 在有死手接口的 venue 上各有 `DeadMan`;没有任何 `account = 9` 的 `DeadMan`、`SessionKeepalive`;`ConnRotate` 只为实盘 gw 的 venue 登记
- 实现:`crates/hft-sequencer/tests/sim_route.rs::seq_215_sim_no_periodic_timers`

### SEQ-216 同一事件派发时实盘实例先于模拟实例,订单先出环

- 规则:[模拟账户](README.md#模拟账户)、[每条 Input 的处理顺序](README.md#每条-input-的处理顺序process)
- 级别:unit
- 里程碑:M2
- 前置:B0 稳态;s1 与 s9 订阅 a1 的 BBO,都在同一条 `Bbo` 上下单
- 步骤:
  1. 投递该 `Bbo`
- 期望:
  - s1 先收到 `EventRef::Bbo`;s1 的 `OrderDecision` 在 s9 的之前追加;out-A 的推入先于 out-sim
- 实现:`crates/hft-sequencer/tests/sim_route.rs::seq_216_live_before_sim`

### SEQ-217 模拟负载不劣化实盘延迟

- 规则:[模拟账户](README.md#模拟账户)、[主循环](README.md#主循环step)
- 级别:dst
- 里程碑:M9
- 前置:B0 同进程混跑;实盘账户 1 持续成对开仓
- 故障:rep-sim 积压 1 000 条并持续注入每轮数百条回报
- 种子:PR 100 + 回归集;每夜日期派生
- 断言:每轮先取完实盘各环的配额,rep-sim 每轮 ≤ 32;实盘 `leg_gap` 与 `bbo_fire` 的 p99 与无模拟负载时相比劣化 ≤ 10%;实盘环全空的轮次 rep-sim 仍被取出;INV-6
- 实现:`crates/hft-testkit/src/scenarios/seq.rs::seq_217_sim_yields_to_live`

### SEQ-218 每个 Input 至多出环 K 条,其余经 OmsContinue 续发

- 规则:[输出扇出](README.md#输出扇出)
- 级别:unit
- 里程碑:M2
- 前置:B0 稳态(K = 16);一个 Input 使 OMS 产生 40 条逐槽撤单
- 步骤:
  1. 投递该 Input;连续 `step()`
- 期望:
  - 该 Input 出环 16 条;下一轮 a 段 `Timer{Oms, OmsContinue}` 出环 16 条;再下一轮 8 条;之后 `wants_continue()` 为假;OMS 待发队列进快照(节 12 `Out` 行)
- 实现:`crates/hft-sequencer/tests/fanout.rs::seq_218_k_per_event`

### SEQ-219 Kill 与 Drain 用 venue 级 CancelAll

- 规则:[输出扇出](README.md#输出扇出)
- 级别:unit
- 里程碑:M2
- 前置:B0 稳态;账户 1 在 A 有 120 张挂单、在 B 有 80 张;两个 venue 都支持批量撤单
- 步骤:
  1. 投递 `Kill{Account(1), cancel_resting}`
- 期望:
  - 出环恰好 2 条 `GwCmd{CancelAll}`(A、B 各一条);没有逐槽撤单
- 实现:`crates/hft-sequencer/tests/fanout.rs::seq_219_kill_uses_cancel_all`

### SEQ-220 大面积模式变化的通知不丢失,按原顺序续推

- 规则:[通知队列与续推](README.md#通知队列与续推)
- 级别:unit
- 里程碑:M2
- 前置:B0 改 `max_cells_total = 256`(U = 384,积压容量 1 536)、`max_groups_per_instance = 64`、`max_timers = 4096`、`notify_per_continue = 64`;4 个稠密账户只用 A 的现货,各有 64 个不重叠的单腿组。实盘实例 2/3/4、模拟实例 1,全部经原始连接、配置、对账、时钟、行情与 SetMode 输入到达 Live;配置内容完整写入 journal
- 步骤:
  1. 投递 `Kill{Global}`;连续 `step()` 直到 `wants_continue()` 为假
- 期望:
  - Kill 的 Input 投递 64 条 `Mode` 通知;其后 4 个 `NotifyContinue` 分别投递 64/64/64/4 条。合计 256 条 Mode 与各账户一条 CapSet 全部恰好投递一次;每批按实例调度顺序投递,同一实例内保持产生顺序(实盘实例的在前)
  - 每批投递的 `Now.seq` 与实际续推 Input 对齐;通知清空后写关键帧,完整回放和 Kill 前关键帧窗口逐字节一致
- 实现:`crates/hft-sequencer/tests/notify_backlog.rs::seq_220_mass_mode_change`

### SEQ-221 每个 NotifyContinue 至多投递 notify_per_continue 条

- 规则:[通知队列与续推](README.md#通知队列与续推)、[环与容量](README.md#环与容量)
- 级别:table
- 里程碑:M2
- 前置:SEQ-220 装配,四实例组数为 `[64, n − 6, 1, 1]`;实际 Kill 产生模式变化与 4 条 CapSet,先投递 64 条,留下 n 条;不直接填队列
- 实现:`crates/hft-sequencer/tests/notify_backlog.rs::seq_221_notify_per_continue`

| 行 | n | 期望 `NotifyContinue` 次数 |
|---|---|---|
| SEQ-221.1 | 63 | 1 |
| SEQ-221.2 | 64 | 1 |
| SEQ-221.3 | 65 | 2(64 + 1) |

每行核对完整通知身份、旧/新模式、cap 来源/作用域、实际续推 Input 数和队列清空,再验证完整 journal 与 Kill 前窗口。

### SEQ-222 续推未完时不写 Snapshot 与 Roll,Drain 不结束

- 规则:[通知队列与续推](README.md#通知队列与续推)、[Drain](README.md#drain)
- 级别:unit
- 里程碑:M2
- 前置:SEQ-220 的配置,关键帧周期 1000 ms;关键帧已到期、`needs_roll = (Size, IdleRound)`;分别覆盖普通续推、续推中收到真实 Drain 命令两行,Drain 的 OMS/执行层退出条件均已满足
- 步骤:
  1. 投递 `Kill{Global}`;连续 `step()`,其间有空闲轮
- 期望:
  - 积压非空期间没有 `Timer{Sys, Snapshot}`、`Timer{Sys, Roll}` 或任何快照;不返回 Done
  - 普通行清空后,周期 Snapshot 与软 Roll 在满足空闲条件后写入;Drain 行在最后一批通知投递后于同一 Input 完成快照与发布。两行均验证完整及续推前窗口
- 实现:`crates/hft-sequencer/tests/notify_backlog.rs::seq_222_continue_blocks_snapshot_roll_drain`

### SEQ-223 积压从不溢出、从不丢弃

- 规则:[通知队列与续推](README.md#通知队列与续推)
- 级别:prop
- 里程碑:M2
- 性质:任意命令与回报交错下,`NotifyBacklog` 占用始终 ≤ 4U;每条产生的通知恰好投递一次,同一实例内顺序与产生顺序相同
- 生成:SEQ-220 装配;固定六条交替 Kill/Resume 命令确保触发背压,再从 {`Kill{Global}`、`Resume{Kill}`、`SetMode` 全部 cell 的 Live/CloseOnly/Shadow、实盘余额回报、bbo} 抽取 1–500 条输入。命令待取时不再预签下一条确认,允许通知继续积压;结束后有界续推至清空
- 参考:不限容量的参考队列
- 断言:仅由已提交命令及独立模式转换生成参考,不读实际队列或模式推算期望;逐实例比对每个 Mode 的组与旧/新模式、每个 CapSet/CapCleared 的账户、来源与作用域;重复 Kill 与不存在的 Resume 不产生 cap 通知。每轮核对占用、峰值、余量、产生/交付差及每个 NotifyContinue 的 64 条上限;重复与缺失不能相互抵消
- 例数:PR 300;每夜 20 000
- 实现:`crates/hft-sequencer/tests/notify_backlog.rs::seq_223_backlog_never_overflows`

### SEQ-224 出环失败时合成 SendReport{RingFull} 作为下一条 Input

- 规则:[出环满](README.md#出环满)
- 级别:unit
- 里程碑:M2
- 前置:B0 稳态;out-B 环满
- 步骤:
  1. s1 下 b1 卖单;`step()`
  2. `VerifyDriver` 重放
- 期望:
  - 该单的 `OrderDecision` 已写;紧接着的下一条 Input 是 `SendReport{status = RingFull}`(记录为 Input);OMS 把该槽迁到 `Rejected(LOCAL_SEND_FAIL)`;一条 P0 告警
  - 重放逐字节一致
- 实现:`crates/hft-sequencer/tests/ring_full.rs::seq_224_ring_full_synth_send_report`

### SEQ-225 feedctl 环满时 FeedCmd 暂存、合并并在每轮开头续推

- 规则:[出环满](README.md#出环满)
- 级别:table
- 里程碑:M2
- 前置:B0 稳态;`feedctl-A` 容量 4 条
- 实现:`crates/hft-sequencer/tests/ring_full.rs::seq_225_feedctl_staging`

| 行 | 动作 | 期望 |
|---|---|---|
| SEQ-225.1 | 一个 Input 产生 6 条 `Subscribe`(inst 1–6) | 推入 1–4;5、6 暂存;没有合成任何 Input |
| SEQ-225.2 | 下一轮开头,环腾出 2 条 | 按原顺序推 5、6 |
| SEQ-225.3 | 暂存中 inst 5 的 `Subscribe{streams = BBO}` 之后又产生 inst 5 的 `Subscribe{BBO \| MARK}` | 暂存只留后者,推出的是 `{BBO \| MARK}` |
| SEQ-225.4 | 启动时 300 个订阅 inst | 300 条 `Subscribe` 最终全部推出,每 inst 一条,无丢失 |
| SEQ-225.5 | `VerifyDriver` 重放 | 不产生 `FeedCmd`;Output 逐字节一致 |

### SEQ-226 订阅 inst 首次订阅时分到稠密下标,进程生命周期内不回收

- 规则:[inst → 策略的稠密索引](README.md#inst--策略的稠密索引)
- 级别:unit
- 里程碑:M2
- 前置:B0 稳态;s1 订阅 a1、b1(`sub_ix` 0、1)
- 步骤:
  1. `SetParams` 使 s1 改订 a2、移除 a1
  2. `SetParams` 使 s1 重新订阅 a1
- 期望:
  - 步骤 1 后 a2 `sub_ix = 2`;a1 仍占 `sub_ix = 0`(CSR 行为空)
  - 步骤 2 后 a1 的 `sub_ix` 仍为 0
- 实现:`crates/hft-sequencer/tests/subs.rs::seq_226_sub_ix_not_recycled`

### SEQ-227 CSR 行内按派发顺序与 gix 排序

- 规则:[inst → 策略的稠密索引](README.md#inst--策略的稠密索引)
- 级别:unit
- 里程碑:M5
- 前置:B0 改实例 s3(Live)、s1(Sim)、s2(Live);s2 的 gix 0 与 gix 1 都订阅 a1,s3、s1 各一个 group 订阅 a1
- 步骤:
  1. 投递 bbo-A(a1)
- 期望:
  - a1 的 CSR 行 = `[(s2, 0), (s2, 1), (s3, 0), (s1, 0)]`;投递顺序相同
- 实现:`crates/hft-sequencer/tests/subs.rs::seq_227_csr_row_order`

### SEQ-228 订阅并集变化时出环 FeedCmd::Subscribe,不变时不出环

- 规则:[inst → 策略的稠密索引](README.md#inst--策略的稠密索引)
- 级别:table
- 里程碑:M2
- 前置:B0 稳态;a2 当前下发的并集为 `BBO`(s2 的快速部分订阅);每行一条 `SetParams`
- 实现:`crates/hft-sequencer/tests/subs.rs::seq_228_subscribe_on_union_change`

| 行 | 变化 | 期望 |
|---|---|---|
| SEQ-228.1 | s2 的 Model 新订阅 a2 的 `TRADE` | 出环一条 `FeedCmd::Subscribe{a2, streams = BBO \| TRADE}` |
| SEQ-228.2 | s1 新订阅 a2 的 `BBO` | 并集不变,不出环 |
| SEQ-228.3 | 全部实例退订 a2,且 a2 无持仓、不是 `ref_inst` | 出环 `FeedCmd::Unsubscribe{a2}` |
| SEQ-228.4 | 新增 group 使新 inst b2 首次被订阅 | 出环 `Subscribe{b2, ..}`;b2 分到新的 `sub_ix` |

### SEQ-229 引擎自身的持仓驱动订阅

- 规则:[inst → 策略的稠密索引](README.md#inst--策略的稠密索引)
- 级别:table
- 里程碑:M2
- 前置:B0 稳态;EngineConfig 段 2 的 `ref_inst` = {a2};每行设定状态后读 `FeedCmd` 下发的并集
- 实现:`crates/hft-sequencer/tests/subs.rs::seq_229_position_driven_subscription`

| 行 | 状态 | 期望并集(引擎部分) |
|---|---|---|
| SEQ-229.1 | g1 绑定 a1(现货腿)与 b1(永续腿),策略未订阅任何事件 | a1 ⊇ `BBO`;b1 ⊇ `BBO \| MARK \| FUNDING` |
| SEQ-229.2 | 账户 1 有无组的 b1 永续空 0.004(`InstPos.qty ≠ 0`),不属于任何 group | b1 ⊇ `BBO \| MARK \| FUNDING` |
| SEQ-229.3 | 账户 1 有 a3 现货库存 0.5(unassigned) | a3 ⊇ `BBO` |
| SEQ-229.4 | `ref_inst` a2 无任何策略订阅 | a2 ⊇ `BBO` |
| SEQ-229.5 | 只为引擎订阅的 b1 `Funding` 到达 | 不投递给任何策略;`MarketLast` 更新 |

### SEQ-230 持仓在 0 与非 0 之间变化时同一 Input 内重算并集

- 规则:[inst → 策略的稠密索引](README.md#inst--策略的稠密索引)
- 级别:table
- 里程碑:M3
- 前置:B0 稳态;s1 只订阅 b1 的 `BBO`;每行从上一行的状态继续
- 实现:`crates/hft-sequencer/tests/subs.rs::seq_230_union_follows_position`

| 行 | 动作 | 期望(同一 Input 内) |
|---|---|---|
| SEQ-230.1 | `WriteOffResidual` 后移除 g1,账户 1 仍有无组 b1 空 0.004 | 不出 `Unsubscribe`;b1 并集仍为 `BBO \| MARK \| FUNDING` |
| SEQ-230.2 | 该无组持仓被平到 0 | 出环 `Subscribe{b1, streams = BBO}`(只剩策略订阅的部分) |
| SEQ-230.3 | 外来成交使账户 1 的 b1 再变为非 0 | 出环 `Subscribe{b1, BBO \| MARK \| FUNDING}` |

### SEQ-231 max_insts 用尽时持仓驱动的 inst 不订阅

- 规则:[inst → 策略的稠密索引](README.md#inst--策略的稠密索引)、[环与容量](README.md#环与容量)
- 级别:unit
- 里程碑:M3
- 前置:B0 改 `max_insts = 3`;a1、b1、a2 已占满 3 个 `sub_ix`
- 步骤:
  1. 外来成交使账户 1 获得 b2 永续持仓
- 期望:
  - b2 不分 `sub_ix`,不出 `FeedCmd::Subscribe`;外来成交照常入账;后续没有 b2 的标记价输入(hft-risk 按标记价断流处理,不在本 crate 断言)
- 实现:`crates/hft-sequencer/tests/subs.rs::seq_231_max_insts_exhausted`

### SEQ-232 启动时每个并集非空的 inst 各下发一次 Subscribe

- 规则:[inst → 策略的稠密索引](README.md#inst--策略的稠密索引)
- 级别:unit
- 里程碑:M2
- 前置:B0;从快照恢复,快照前最后下发的并集各不相同;本次运行的并集:a1、b1、a2 非空,a3 为空
- 步骤:
  1. 启动并运行到稳态
- 期望:
  - feedctl 上恰好 3 条 `Subscribe`(a1、b1、a2 各一条,`streams` = 本次运行的并集);a3 无记录;快照中没有"上次下发的并集"这一字段
- 实现:`crates/hft-sequencer/tests/subs.rs::seq_232_startup_subscribe_once`

### SEQ-233 group 级事件经 group → (instance_ix, gix) 索引投递

- 规则:[inst → 策略的稠密索引](README.md#inst--策略的稠密索引)
- 级别:unit
- 里程碑:M2
- 前置:B0 稳态;s2 有 g2(gix 0)与 g2b(gix 1)
- 步骤:
  1. 使 g2b 的有效模式变化
- 期望:
  - s2 收到 `EventRef::Mode{gix: 1}`;s1、s9 收不到
- 实现:`crates/hft-sequencer/tests/subs.rs::seq_233_group_index_delivery`

### SEQ-234 策略定时器以 GroupId 为键,投递时现查 gix

- 规则:[定时器](README.md#定时器)
- 级别:unit
- 里程碑:M2
- 前置:B0 稳态;s2 有 g2(gix 0)与 g2b(gix 1);第三组使用互不重叠的 FILL 现货/永续腿;s2 为 g2b 登记 tag 4 的定时器
- 步骤:
  1. `SetParams` 在 g2 之前插入新 group g2c,使 g2b 的 gix 变为 2
  2. 定时器到期
  3. 从 journal 配置独立执行完整回放和参数替换前关键帧窗口回放
- 期望:
  - 堆中条目键为 `(owner 18, timer_id 4, group g2b)`,`SetParams` 前后不变;到期时 s2 收到 `EventRef::Timer{tag: 4, group: g2b, gix: 2}`
  - 截止前不投递,到期仅投递一次,堆与身份登记同时清空;两种回放的 Output、快照和恢复状态均一致
- 实现:`crates/hft-sequencer/tests/timers.rs::seq_234_timer_keyed_by_group`

### SEQ-235 被 SetParams 移除的 group 的定时器在应用参数时撤销

- 规则:[定时器](README.md#定时器)、[命令路径](README.md#命令路径)
- 级别:unit
- 里程碑:M2
- 前置:B0 稳态;s2 为 g2b(无持仓、无目标)登记 2 个定时器
- 步骤:
  1. `SetParams` 移除 g2b
- 期望:
  - 同一 Input 内堆中不再有 `group = g2b` 的条目;写一条 `Note`(撤销的条数 2);之后不会出现这两个 `Timer`
- 实现:`crates/hft-sequencer/tests/timers.rs::seq_235_removed_group_timers_cancelled`

### SEQ-236 堆按 (deadline_tsc, owner, timer_id, group) 整序

- 规则:[定时器](README.md#定时器)
- 级别:prop
- 里程碑:M2
- 性质:任意登记与撤销序列之后,到期定时器的交出顺序等于按 `(deadline_tsc, owner, timer_id, group)` 升序排序的结果,与登记顺序无关
- 生成:0–256 个条目,`deadline_tsc` 只取 4 个不同值(制造大量相同截止),owner、timer_id、group 随机;随机撤销 0–50%
- 参考:`BTreeSet` 排序
- 例数:PR 1 000;每夜 50 000
- 实现:`crates/hft-sequencer/tests/timers.rs::seq_236_heap_total_order`

### SEQ-237 恢复结束时堆内定时器的平移

- 规则:[定时器](README.md#定时器)、[冷启动恢复](README.md#冷启动恢复recoverdriver)
- 级别:table
- 里程碑:M2
- 前置:`stop_tsc = 10 000 000 000`;本次运行 `now = 500 000`(跨开机);每行一个堆条目的 `deadline_tsc` 与两次运行的 `tsc_hz_milli`
- 实现:`crates/hft-sequencer/tests/timers.rs::seq_237_heap_shift_formula`

| 行 | deadline_tsc | hz_prev / hz_cur | 期望平移后 |
|---|---|---|---|
| SEQ-237.1 | 10 300 000 000(剩 0.3 s) | 10^12 / 10^12 | 300 500 000 |
| SEQ-237.2 | 9 000 000 000(已过期 1 s) | 10^12 / 10^12 | `max(500 000 − 1 000 000 000, 1)` = 1 |
| SEQ-237.3 | 10 000 000 000(恰为 stop_tsc) | 10^12 / 10^12 | 500 000 |
| SEQ-237.4 | 10 300 000 000 | 10^12 / 3 × 10^12 | 500 000 + 900 000 000 = 900 500 000 |
| SEQ-237.5 | 0 | 任意 | 0 |

### SEQ-238 执行层的恢复定时器与堆中同键条目合并,保留较早者

- 规则:[定时器](README.md#定时器)
- 级别:table
- 里程碑:M2
- 前置:真实引擎设置 `gate_naked_alarm_ms = 7 000` 后准备关键帧,堆中 `(Exec, Wake | 2 << 8, g2)` 在恢复 TSC = 1 000 后 deadline = 5 000;执行层 cell 独立保存下表截止,AutoFix 行的状态为 Broken/Primed
- 步骤:从该关键帧真实重启,断言恢复堆的完整键和数量;处理 EngineClock 并保存后续关键帧,由 journal 配置独立执行该窗口回放
- 期望:同键仅一条且取较早截止,异键两条均保留;Broken 行另有一条按新闸门关闭时刻登记的 Alarm,截止由配置 `gate_naked_alarm_ms` 独立计算;窗口与实际恢复的 Output、快照和恢复状态一致
- 实现:`crates/hft-sequencer/tests/timers.rs::seq_238_recovery_timers_merge`

| 行 | `exec.recovery_timers()` 给出的同键条目 | 期望堆中该键 |
|---|---|---|
| SEQ-238.1 | deadline 4 000 | 一条,4 000 |
| SEQ-238.2 | deadline 6 000 | 一条,5 000 |
| SEQ-238.3 | 键不同(`AutoFix`),deadline 4 000 | AutoFix 4 000 / Wake 5 000 各一条;另有 Alarm = 1 000 + `gate_naked_alarm_ms` × 10^6,共三条 |

### SEQ-239 周期性系统定时器每次启动重新登记,替换旧条目;更早的 OmsSweep 保留

- 规则:[定时器](README.md#定时器)
- 级别:table
- 里程碑:M2
- 前置:真实引擎关键帧的堆夹具中有 `OmsSweep` 与 (B, 1) 的 `DeadMan` 各一条;旧 DeadMan 剩余 2 000 ms,venue B `refresh_ms = 10 000`;恢复 TSC = 500 000
- 步骤:
  1. 快照中 `OmsSweep` 平移后剩余 1 500 ms:恢复并处理 `EngineClock`
  2. 快照中 `OmsSweep` 平移后剩余 400 ms:恢复并处理 `EngineClock`
  3. 每种剩余时长分别在同频率与三倍频率重启,推进到新运行 9 999/10 000 ms,从准备好的关键帧开始独立窗口回放
- 期望:
  - 步骤 1 后堆中 `OmsSweep` 恰好一条(deadline = 本次运行的登记时刻 + 1 000 ms),(B, 1) 的 `DeadMan` 恰好一条(+10 000 ms);没有快照中的旧条目
  - 步骤 2 后 `OmsSweep` 恰好一条,deadline = 快照条目平移后的值(登记时刻 + 400 ms);`DeadMan` 同步骤 1
  - 9 999 ms 时尚无 (B, 1) 的 DeadManRefresh,10 000 ms 时只有一次;窗口及后续实际重启的 Output、快照和恢复状态一致
- 实现:`crates/hft-sequencer/tests/timers.rs::seq_239_periodic_timers_reregistered`

| 行 | OmsSweep 剩余 | 新频率 / 旧频率 | 恢复后 OmsSweep | 恢复后 DeadMan |
|---|---|---|---|---|
| SEQ-239.1 | 1 500 ms | 1 | +1 000 ms,一条 | +10 000 ms,一条 |
| SEQ-239.2 | 1 500 ms | 3 | +1 000 ms,一条 | +10 000 ms,一条 |
| SEQ-239.3 | 400 ms | 1 | +400 ms,一条 | +10 000 ms,一条 |
| SEQ-239.4 | 400 ms | 3 | +400 ms,一条 | +10 000 ms,一条 |

### SEQ-240 按 engine_ns 记截止的登记向上取整到本次运行的 TSC

- 规则:[定时器](README.md#定时器)
- 级别:table
- 里程碑:M2
- 前置:本次运行 `tsc_hz_milli = 1.05 × 10^12`、`base_engine_ns = 0`、`base_tsc = 0`;hft-risk 经 `MarginOut.timers` 登记 engine_ns 截止 d
- 实现:`crates/hft-sequencer/tests/timers.rs::seq_240_engine_ns_deadline_ceil`

| 行 | d(ns) | 期望 `deadline_tsc` |
|---|---|---|
| SEQ-240.1 | 100 | 105 |
| SEQ-240.2 | 101 | 107(106.05 向上取整) |
| SEQ-240.3 | 0 | 0 |

### SEQ-241 DeadMan 由 sequencer 驱动,sequencer 停住时刷新停止

- 规则:[定时器](README.md#定时器)
- 级别:unit
- 里程碑:M2
- 前置:B0 稳态;venue B `refresh_ms = 10 000`、`timeout_ms = 30 000`
- 步骤:
  1. 运行 35 s
  2. 停止调用 `step` 30 s
- 期望:
  - 步骤 1:out-B 恰好 3 条 `GwCmd{DeadManRefresh, arg: 30 000}`(10 s、20 s、30 s)
  - 步骤 2:期间 out-B 没有新的 `DeadManRefresh`
- 实现:`crates/hft-sequencer/tests/timers.rs::seq_241_dead_man_refresh`

### SEQ-242 段滚动的引导块记录顺序

- 规则:[快照](README.md#快照)
- 级别:unit
- 里程碑:M2
- 前置:B0 稳态;3 个 Live 簿(a1、b1、a2);2 个实例有参数(s1、s2)
- 步骤:
  1. 交出 `Timer{Sys, Roll}`
- 期望:
  - 旧段:`SegmentRoll{End}`(Meta)后封存
  - 新段依次:`SegmentRoll{Begin}`、`HostProfile`、`ClockStatus`、`ConfigBlob{boot = 1}` × 5(refdata、Engine、s1 的 `StrategyParams`、s2 的 `StrategyParams`、`RiskParams`)、`StateSnapshot{Boot}`(`n_books = 3`)、3 条 `L2Snapshot{origin = Keyframe}`、`SegmentRoll{BootDone}`,随后一次 `publish`
  - 滚动期间没有任何 `GwCmd`、`FeedCmd` 出环
- 实现:`crates/hft-sequencer/tests/snapshot.rs::seq_242_bootblock_order`

### SEQ-243 sim 实例的引导块另含 SimConfig

- 规则:[快照](README.md#快照)
- 级别:unit
- 里程碑:M4
- 前置:`instance.name = sim`,只含 Sim 账户 9;s9 有参数
- 步骤:
  1. 交出 `Timer{Sys, Roll}`
- 期望:
  - `ConfigBlob{boot = 1}` 依次为 refdata、Engine、`SimConfig`、s9 的 `StrategyParams`、`RiskParams` 中的全部 5 条(顺序按 journal-format)
- 实现:`crates/hft-sequencer/tests/snapshot.rs::seq_243_sim_bootblock_sim_config`

### SEQ-244 roll 返回 NextNotReady 时不写引导块,下一轮重试,回放复现

- 规则:[快照](README.md#快照)、[环境决定写成 Input](README.md#环境决定写成-input)
- 级别:unit
- 里程碑:M2
- 前置:B0 稳态;`needs_roll = (Size, EventBoundary)`;测试 `JournalSink::roll` 第一次返回 `RollErr::NextNotReady`,第二次成功
- 步骤:
  1. 连续两轮 `step()`
  2. `VerifyDriver` 重放
- 期望:
  - 第一次 `step()`:`Timer{Sys, Roll}` 事件内没有任何记录、状态哈希不变,当前段继续
  - 第二次 `step()` 的 a 段再写一条 `Timer{Sys, Roll}`,其后 `SegmentRoll{End}` 与完整引导块
  - 重放时第一条 `Timer{Roll}` 后不生成引导块、第二条后生成,逐字节一致
- 实现:`crates/hft-sequencer/tests/snapshot.rs::seq_244_roll_next_not_ready`

### SEQ-245 关键帧在一个事件内一次写完,头部 state_hash 为 0

- 规则:[快照](README.md#快照)、[状态哈希](README.md#状态哈希)
- 级别:unit
- 里程碑:M2
- 前置:B0 改 `max_cells_total = 256`,状态使快照约 2 MiB;关键帧到期,上一轮空闲
- 步骤:
  1. `step()`
- 期望:
  - 这一轮唯一的 Input 是 `Timer{Sys, Snapshot}`;同一事件内一条 `StateSnapshot{purpose = Keyframe}`(多段记录);本事件 `end_event` 一次、`publish` 一次;`SnapshotHdr.state_hash` 16 字节全为 0;`out_hash` 非 0
- 实现:`crates/hft-sequencer/tests/keyframe.rs::seq_245_keyframe_one_shot`

### SEQ-246 关键帧的节内容与按需快照逐字节相同

- 规则:[快照](README.md#快照)
- 级别:unit
- 里程碑:M2
- 前置:B0 稳态;在两个相邻、中间无其他 Input 的位置分别写关键帧 K 与按需快照 O
- 步骤:
  1. 比较 K 与 O 的节
- 期望:
  - 各节(`SectionHdr` 与节体)逐字节相同;头部只有 `purpose`、`state_hash`(K 为 0,O 为计算值)、`at_seq` 与测量字段不同
- 实现:`crates/hft-sequencer/tests/keyframe.rs::seq_246_keyframe_sections_equal_on_demand`

### SEQ-247 读者按节内容算出的关键帧哈希等于连续运行的状态哈希

- 规则:[状态哈希](README.md#状态哈希)、[快照](README.md#快照)
- 级别:unit
- 里程碑:M2
- 前置:SEQ-246 的 K;连续运行在 K 的 `at_seq` 处按状态哈希规则计算的哈希 H
- 步骤:
  1. 对 K 的各节按写出顺序(节 16 除外)计算 xxh3-128
- 期望:
  - 结果 = H = O 的 `SnapshotHdr.state_hash`;`replayer verify` 在 K 处逐字节通过
- 实现:`crates/hft-sequencer/tests/keyframe.rs::seq_247_reader_side_keyframe_hash`

### SEQ-248 各组件快照节的写出顺序

- 规则:[快照](README.md#快照)
- 级别:unit
- 里程碑:M2
- 前置:B0 稳态,各节非空
- 步骤:
  1. 写按需快照,按体内顺序读各 `SectionHdr.section`
- 期望:
  - 顺序为 1、13、22、2、3、12、4、14、17、18、23、5、8、9、10、15、16、19、20、24、25、26、27、29、6、7、21(稳态有启动对账留下的视图,无成交所以没有节 28);`SnapshotHdr.n_sections = 27`
- 实现:`crates/hft-sequencer/tests/snapshot.rs::seq_248_section_write_order`

### SEQ-249 state_hash 不计节 16,out_hash 屏蔽测量字段

- 规则:[状态哈希](README.md#状态哈希)
- 级别:table
- 里程碑:M2
- 前置:B0 稳态;两次运行的 Input 序列相同,只有所列差异;各写一份按需快照
- 实现:`crates/hft-sequencer/tests/hash.rs::seq_249_hash_exclusions`

| 行 | 差异 | 期望 |
|---|---|---|
| SEQ-249.1 | 最近一条 `ClockStatus`(Meta)不同,使节 16 不同 | `state_hash` 相同 |
| SEQ-249.2 | `drv.measure()` 返回值不同(`decided_tsc` 不同) | `out_hash` 相同 |
| SEQ-249.3 | 主机不同(`snap_tsc`、`boot_id`、`snap_utc_ns` 不同) | `state_hash`、`out_hash` 相同;验证比对通过 |
| SEQ-249.4 | 某条 `OrderDecision.qty` 不同 | `out_hash` 不同 |
| SEQ-249.5 | 节 16 的 count 不合法 | 对应快照比对失败,不能随校准行体一起屏蔽 |
| SEQ-249.6 | 仅替换某条快照节 16 的行体,保留原 ClockStatus | 哈希与业务输出不变,快照比对通过 |

通过真实 hk 记录检查节 16 的全部字段和来源 seq,包含所选快照后首条 Input 之前与运行期两次记录;比较完整 Input 流,完整及窗口回放并读取回放引擎序列化状态。未提交 Meta 尾部不更新恢复状态;真实滚段快照取本段引导块的 ClockStatus。修改 qty 后独立重算日志的 out_hash,仍须在订单与快照两处发现分歧;恢复尾部比对同样覆盖校准豁免。主机行的 boot_id 由日志副本提供,内存驱动不生成进程 UUID。

### SEQ-250 从快照起步时重建派生缓存,不产生任何记录

- 规则:[快照](README.md#快照)
- 级别:unit
- 里程碑:M2
- 前置:快照含 s1、s2 的参数;节 10 中 b1 行 `mark_px = 50 000`、`mark_ens = 7 000 000`、`interval_s = 3 600`,a1 行 `interval_s = 0`
- 步骤:
  1. `VerifyDriver` 从该快照起步,在处理第一条 Input 之前检查调用记录
- 期望:
  - 对 s1、s2 各调一次 `ledger.on_groups_changed` 与 `risk.on_groups_changed`;`model_mem`、`model_ckpt` 已重算;`ledger.restore_marks` 一次(含 b1 的 `(50 000, 7 000 000)`);`ledger.on_funding_interval(b1, 3 600)` 一次、a1 不调;`rx_from_seq` = 快照 `at_seq`;journal 没有新增记录
- 实现:`crates/hft-sequencer/tests/snapshot.rs::seq_250_start_from_snapshot_rebuild`

`testkit` 的有界记录在真实调用完成处采集,测试比较完整有序调用表,并检查实例预算、资金费缓存与原 journal 字节;继续处理输入后记录仍在。完整与所选快照窗口均从 journal 的 ECF1/RKP1 配置独立验证。

### SEQ-251 本 crate 快照行的大小与关键字段偏移

- 规则:[快照](README.md#快照)
- 级别:table
- 里程碑:M2
- 前置:编译期常量
- 实现:`crates/hft-sequencer/tests/snapshot.rs::seq_251_row_layouts`

| 行 | 结构 | 期望 |
|---|---|---|
| SEQ-251.1 | `StrategyRowHdr` | 24 B;`kind_id` 2、`desc_version` 6、`params_version` 8、`len` 16、`n_groups` 20 |
| SEQ-251.2 | `GroupSeqRow` | 16 B;`spec_seq` 8 |
| SEQ-251.3 | `TimerRow` | 24 B;`timer_id` 8、`owner` 12、`group` 16 |
| SEQ-251.4 | `ActiveConfigRow` | 64 B |
| SEQ-251.5 | `MarketLastRow` | 112 B;`funding_rate` 32、`mark_px` 48、`settled_cut_ns` 72、`bbo_member_mask` 80、`bbo_flags` 81、`bbo_hw_rx_ns` 88、`mark_ens` 96、`prev_interval_s` 104 |
| SEQ-251.6 | `ReconPendingHdr` | 16 B;`n_stored` 10、`count` 12、`received` 14 |
| SEQ-251.7 | `CmdDedupHdr` / `CmdDedupRow` | 8 B / 32 B;`cmd_seq` 16、`status` 24 |
| SEQ-251.8 | `ViewerPreviewRow` | 64 B;`issuer` 32、`preview_engine_ns` 56 |
| SEQ-251.9 | `ClockCalRow` | 40 B;`tsc_hz_milli` 24、`from_seq` 32 |
| SEQ-251.10 | `VenueNoticeRow` | 32 B;`inst` 4、`a` 8、`cmd_seq` 24 |
| SEQ-251.11 | `ConnLiveRow` | 16 B;`flags` 2、`live_tsc` 8 |
| SEQ-251.12 | `StrategyTimersHdr` / `StrategyTimerRow` | 16 B / 24 B;头 next_id 8;登记 deadline_tsc 8、tag 16、group 20 |

### SEQ-252 各节的行版本

- 规则:[快照](README.md#快照)
- 级别:table
- 里程碑:M2
- 前置:B0 稳态;写一份按需快照
- 实现:`crates/hft-sequencer/tests/snapshot.rs::seq_252_row_versions`

| 行 | 节 | 期望 `SectionHdr.version` |
|---|---|---|
| SEQ-252.1 | 5 | 2 |
| SEQ-252.2 | 8 | 2 |
| SEQ-252.3 | 9 | 1 |
| SEQ-252.4 | 10 | 6 |
| SEQ-252.5 | 15 | 2 |
| SEQ-252.6 | 16 | 1 |
| SEQ-252.7 | 19 | 2 |
| SEQ-252.8 | 24 | 1 |
| SEQ-252.9 | 26 | 2 |
| SEQ-252.10 | 27 | 1 |

### SEQ-253 快照行的兼容读取

- 规则:[快照](README.md#快照)
- 级别:table
- 里程碑:M2
- 前置:B0;构造所列快照后 `boot(Journal(Latest))`
- 实现:`crates/hft-sequencer/tests/snapshot.rs::seq_253_row_compat`

| 行 | 快照 | 期望 |
|---|---|---|
| SEQ-253.1 | 节 10 为 v5 行(104 B,无 `prev_interval_s`),b1 `interval_s = 3 600` | 读出 `prev_interval_s = 3 600` |
| SEQ-253.2 | 节 10 行尾多 8 字节(更新版本追加的字段) | 按前缀读取,其余字段相同 |
| SEQ-253.3 | 含未知节号 99(`len = 40`) | 按 `SectionHdr.len` 跳过,恢复成功 |
| SEQ-253.4 | 节 8 的 `len` 与 `count × 24` 不符 | `Err(BootError::SnapshotUndecodable{section: 8})` |

前三行检查恢复后每条节 10 行的完整已知前缀,并验证新 Recovery 快照之后的输入/输出;旧格式或未知节不能逐字节再生,原快照的冷自检必须保持 Size/RoundTrip 失败,不算旧格式回放通过。

### SEQ-254 快照行按键升序、填充字节为零

- 规则:[快照](README.md#快照)、[确定性规则](README.md#确定性规则)
- 级别:prop
- 里程碑:M2
- 性质:任意状态写出的快照中,节 5(按 strategy,其 `GroupSeqRow` 按 GroupId)、节 10(按 inst)、节 24(按 (venue, inst, kind))、节 26(按 (venue, slot))的行严格升序;全部 `_pad`/`_rsv` 字节为 0;节 26 只含本次运行用过的槽
- 生成:B0 上随机 Input 序列 100–5 000 条,含连接起落、VenueNotice、SetParams
- 参考:按规则检查快照字节
- 例数:PR 300;每夜 10 000
- 实现:`crates/hft-sequencer/tests/snapshot.rs::seq_254_rows_sorted_and_zero_padded`

每例包含连接 Up/Down、成功 VenueNotice、成功 SetParams 与行情混排;每次参数替换的 ConfigBlob/命令各算一条实际 Input。按实际日志核对 100–5 000 条范围,保留至少 300 例并允许 `PROPTEST_CASES` 提高至夜间数量。检查上述四节的全部填充、行键及所有节头保留字;中途真实重启,处理新 EngineClock 后必须清空旧槽,后续只保留该次运行输入实际触及的连接槽。

### SEQ-255 生效公告在 inst 退出 refdata 或维护窗口结束后的下一次快照时清除

- 规则:[命令路径](README.md#命令路径)、[快照](README.md#快照)
- 级别:unit
- 里程碑:M2
- 前置:B0 稳态;节 24 有 `Maintenance{venue B, a = 10:00, b = 10:30}`
- 步骤:
  1. UTC 10:30 之前写按需快照
  2. UTC 10:31 之后写按需快照
- 期望:
  - 步骤 1 的节 24 含该行;步骤 2 的节 24 不含
- 实现:`crates/hft-sequencer/tests/snapshot.rs::seq_255_venue_notice_expiry`

### SEQ-256 截断到最后一个 EVENT_END 后的状态等于连续运行到该事件

- 规则:[冷启动恢复](README.md#冷启动恢复recoverdriver)
- 级别:prop
- 里程碑:M2
- 性质:在任意记录处截断 journal(含半个事件)后 `boot(Journal(Latest, replay_tail = true))`,状态哈希等于连续运行处理完最后一个完整事件时的哈希;半个事件的 Input 视为未发生,其 `InputHash = xxh3_64(kind ‖ body)` 被报告
- 生成:B0 上 2 000 条 Input 的 journal;截断位置在全部记录边界与记录内部均匀抽取
- 参考:连续运行时每个事件末的状态哈希表
- 例数:PR 300;每夜 20 000
- 实现:`crates/hft-sequencer/tests/recover.rs::seq_256_truncate_to_event_end`

### SEQ-257 恢复按 RecoverPlan.start 选快照

- 规则:[冷启动恢复](README.md#冷启动恢复recoverdriver)、[版本变化与回滚](README.md#版本变化与回滚)
- 级别:table
- 里程碑:M2
- 前置:journal 中依次有快照 S1(seq 1 000,Keyframe)、S2(seq 5 000,Drain,其后无 Input)
- 实现:`crates/hft-sequencer/tests/recover.rs::seq_257_snapshot_selection`

| 行 | `start` | 期望起步快照 |
|---|---|---|
| SEQ-257.1 | `Latest` | S2 |
| SEQ-257.2 | `DrainTail` | S2 |
| SEQ-257.3 | `AtSeq(4 999)` | S1 |
| SEQ-257.4 | `NewestReadable`,S2 节 5 的 `desc_version` 不被当前策略接受 | S1 |
| SEQ-257.5 | `Latest`,S2 节 5 的 `desc_version` 不被接受 | `Err(SnapshotUndecodable{section: 5})` |

### SEQ-258 尾部重放比较输出、不出环、不改 TSC 量

- 规则:[冷启动恢复](README.md#冷启动恢复recoverdriver)
- 级别:unit
- 里程碑:M2
- 前置:B0 运行到 seq 9 000 崩溃;最近快照在 seq 8 000;尾部含 3 笔已出环的 `OrderDecision`
- 步骤:
  1. `boot(Journal(Latest, replay_tail = true))`
- 期望:
  - 重放 seq 8 001–9 000;重新生成的 Output 与日志逐条一致;各 out 环与 feedctl 环为空;重放期间 OMS 槽与执行层的 TSC 字段等于快照装载值加重放中按旧运行 tsc 更新的值(与崩溃前同一时刻的值相同)
- 实现:`crates/hft-sequencer/tests/recover.rs::seq_258_tail_replay_no_emit`

### SEQ-259 replay_tail = false 时全部账户置 TAIL_LOST

- 规则:[冷启动恢复](README.md#冷启动恢复recoverdriver)
- 级别:unit
- 里程碑:M2
- 前置:同 SEQ-258
- 步骤:
  1. `boot(Journal(Latest, replay_tail = false, flags = TAIL_LOST))`
- 期望:
  - 不读 seq 8 001 之后的记录;恢复结束时账户 1、2、9 各 `apply(RecoveryFlags{account, flags ⊇ TAIL_LOST})`;`stop_tsc` = 快照所在记录的记录头 tsc
- 实现:`crates/hft-sequencer/tests/recover.rs::seq_259_no_tail_tail_lost`

### SEQ-260 恢复结束使用本次运行的 TSC,结果进入 Recovery 快照

- 规则:[冷启动恢复](README.md#冷启动恢复recoverdriver)
- 级别:unit
- 里程碑:M2
- 前置:上次运行 1 GHz,最后一条已应用记录 tsc = 10 000 000 000;本次运行 3 GHz,`drv.run_tsc()` 返回 500 000
- 步骤:
  1. 完成恢复
- 期望:
  - `oms.recovery_end(500 000, 10 000 000 000, 10^12, 3 × 10^12, ..)` 与 `exec.recovery_end(500 000, 10 000 000 000, 10^12, 3 × 10^12)` 各一次;随后逐条登记 `exec.recovery_timers()`;新段 `StateSnapshot{purpose = Recovery}` 的 `snap_tsc = 500 000`,其节 8 是平移后的值
- 实现:`crates/hft-sequencer/tests/recover.rs::seq_260_recovery_end_now`

### SEQ-261 跨重启的剩余时长与停机前一致

- 规则:[冷启动恢复](README.md#冷启动恢复recoverdriver)、[定时器](README.md#定时器)
- 级别:table
- 里程碑:M2
- 前置:停机前:策略定时器剩 2 s、`HedgeRetry` 剩 0.5 s、执行层 `AutoFix` 剩 90 s;停机 10 min
- 实现:`crates/hft-sequencer/tests/recover.rs::seq_261_remaining_durations_preserved`

| 行 | 重启方式 | 期望 |
|---|---|---|
| SEQ-261.1 | 同开机(TSC 继续增长) | 三者恢复后分别在 2 s、0.5 s、90 s(运行时间)后到期 |
| SEQ-261.2 | 跨开机(TSC 从远小于 `stop_tsc` 的值重新计数) | 同上;没有任何定时器落到 1 s 之后的极远将来 |
| SEQ-261.3 | 换机(TSC 频率 1 GHz → 3 GHz) | 同上(按纳秒相同) |
| SEQ-261.4 | 新段 `replayer verify` 从 Recovery 快照起步 | 逐字节通过 |

### SEQ-262 恢复结束对每个账户加 Recovery,不写任何 cell

- 规则:[冷启动恢复](README.md#冷启动恢复recoverdriver)
- 级别:unit
- 里程碑:M2
- 前置:快照中 g1 期望模式 Live、g2 期望模式 Shadow
- 步骤:
  1. 完成恢复
- 期望:
  - 账户 1、2、9 各一次 `apply(Auto(Recovery{account}))` 与 `apply(RecoveryFlags{..})`;节 14 的期望模式行与快照逐字节相同;没有 `SetMode` 类输入
- 实现:`crates/hft-sequencer/tests/recover.rs::seq_262_recovery_cap_no_cells`

### SEQ-263 新 epoch 的取值与下发

- 规则:[冷启动恢复](README.md#冷启动恢复recoverdriver)
- 级别:table
- 里程碑:M2
- 前置:快照节 3 的 `engine_epoch` 与 `RecoverPlan.epoch` 如列
- 实现:`crates/hft-sequencer/tests/recover.rs::seq_263_epoch`

| 行 | 快照 / `RecoverPlan.epoch` | 期望 |
|---|---|---|
| SEQ-263.1 | 7 / 9 | `view().engine_epoch() = 10`;节 3 = 10;`Note{EpochStart}.v[0] = 10`;每个 gw 一条 `GwCmd{SetEpoch{epoch: 10, accept_prev: true, prev_epoch: 7}}`(`prev_epoch` 取快照节 3 的值,不是 9),在 `EpochStart` 事件之后 |
| SEQ-263.2 | 9 / 7 | 10 |
| SEQ-263.3 | 8 / 8 | 9 |

### SEQ-264 恢复的新段以 Recovery 引导块开头,随后是 EngineClock

- 规则:[冷启动恢复](README.md#冷启动恢复recoverdriver)
- 级别:unit
- 里程碑:M2
- 前置:同 SEQ-258
- 步骤:
  1. 完成恢复并 `step()` 一次
- 期望:
  - 新段段头 `roll_reason = Recovery`;引导块含 `StateSnapshot{purpose = Recovery}`(反映 `recovery_end` 之后的状态);`BootDone` 之后第一条记录是 `EngineClock`,`base_engine_ns = view().stop_engine_ns()`
- 实现:`crates/hft-sequencer/tests/recover.rs::seq_264_recovery_segment`

### SEQ-265 恢复后按 (venue, account) 分批下发 MirrorRestore

- 规则:[冷启动恢复](README.md#冷启动恢复recoverdriver)
- 级别:unit
- 里程碑:M2
- 前置:恢复后账户 1 在 B 有 300 个非终态槽,其中 1 个无法编码查询
- 步骤:
  1. 运行到 `MirrorRestore` 下发完毕
- 期望:
  - out-B 收到 2 批 `GwCmd{MirrorRestore}`(256 + 43 条);无法编码的那个槽保持 `Unknown`
- 实现:`crates/hft-sequencer/tests/recover.rs::seq_265_mirror_restore_batches`

### SEQ-266 恢复时不判 Broken:Hedging 链接等对冲闸门子集满足后续接一次

- 规则:[冷启动恢复](README.md#冷启动恢复recoverdriver)
- 级别:unit
- 里程碑:M5
- 前置:停机前 s1 链接 L 处于 `Hedging`(第一腿已成交、第二腿在 B 未成交);`hedge_deadline_ms = 3 000`;停机 10 min
- 步骤:
  1. 恢复;运行 5 s,B 的对冲闸门子集尚未满足
  2. B 的闸门子集满足(早于账户 1 整体放行)
  3. 续接单再次被拒,之后再运行 3 s
- 期望:
  - 步骤 1:L 为 `RESUME_HOLD`,没有 `Broken`
  - 步骤 2:同一 Input 内 `oms.on_hedge_gate_open(1, B, ..)`,L 的 `SecondRetry` 出环
  - 步骤 3:按只计运行时间的 `hedge_deadline_ms` 判定,到期后 `Broken`
  - `recovery_end` 的结果进入 Recovery 快照,新段 `replayer verify` 通过
- 实现:`crates/hft-sequencer/tests/recover.rs::seq_266_no_broken_at_startup`

### SEQ-267 进程在任意点崩溃后恢复,状态 = f(journal)

- 规则:[冷启动恢复](README.md#冷启动恢复recoverdriver)、[确定性规则](README.md#确定性规则)
- 级别:dst
- 里程碑:M5
- 前置:B0;策略集 + 执行层 + 风控 + OMS + ledger + 共用撮合器
- 故障:在事件中途、段滚动中途、引导块中途、关键帧写出中途各随机注入进程崩溃,重启走 `RecoverDriver`
- 种子:PR 200 + 回归集;每夜日期派生
- 断言:每次恢复后的状态哈希等于从最近完整快照 + 后续完整事件重建的哈希;半个事件的输出从未出环;INV-2、INV-6、INV-7、INV-9
- 备注:跨组件期望见 RC-1、RC-2
- 实现:`crates/hft-testkit/src/scenarios/seq.rs::seq_267_crash_anywhere_recovers`

### SEQ-268 恢复时决策不一致的受影响范围

- 规则:[恢复时决策不一致](README.md#恢复时决策不一致)
- 级别:table
- 里程碑:M2
- 前置:尾部重放中人为使所列记录与日志不同;账户 1、2 都在 A、B 有非空闲槽
- 实现:`crates/hft-sequencer/tests/mismatch.rs::seq_268_mismatch_scope`

| 行 | 不一致的记录 | 期望 `oms.force_unknown` 的范围 |
|---|---|---|
| SEQ-268.1 | 账户 1 在 B 的 `OrderDecision` | 只 (B, 1) |
| SEQ-268.2 | 账户 2 在 A 的 `ExecDecision` | 只 (A, 2) |
| SEQ-268.3 | 账户 1 在 B 的 `FundingSettle{synth = 1}` | 只 (B, 1) |
| SEQ-268.4 | 一条 `CmdResult` | 全部 (venue, account):(A, 1)、(B, 1)、(A, 2)、(B, 2) |
| SEQ-268.5 | 一条无账户的 `Note` | 全部 (venue, account) |

### SEQ-269 不一致时全部非空闲槽置 Unknown、RECOVER_MISMATCH、P0,不中止恢复

- 规则:[恢复时决策不一致](README.md#恢复时决策不一致)
- 级别:unit
- 里程碑:M2
- 前置:尾部 seq 8 500 处账户 1 在 B 的 `OrderDecision` 与重算结果不同;(B, 1) 有 Live 槽 2 个、PendingCancel 1 个
- 步骤:
  1. 完成恢复
- 期望:
  - 恢复不返回错误;恢复结束时 `oms.force_unknown(B, 1)` 返回 3,三个槽都为 `Unknown`
  - `Note{Sequencer, RecoverMismatch}` P0,`v[0] = 2 | 1 << 8`、`v[1] = 8 500`
  - 账户 1 的 `RecoveryFlags ⊇ RECOVER_MISMATCH`;闸门放行后 `Recovery` 不自动解除
- 实现:`crates/hft-sequencer/tests/mismatch.rs::seq_269_mismatch_handling`

### SEQ-270 运行期毒丸:截掉的半个事件的 InputHash 被报告并隔离

- 规则:[毒丸输入隔离](README.md#毒丸输入隔离)
- 级别:unit
- 里程碑:M2
- 前置:上次运行在处理 `ReconcileReport` R 时崩溃,R 是被截掉的半个事件;本次 `RecoverPlan{quarantine: [hash(R)], flags ⊇ POISON}`
- 步骤:
  1. 完成恢复并运行;ctl 环先后到达与 R 字节相同的报告 2 次,以及一条不同的报告 R'
- 期望:
  - `boot` 报告的半事件哈希 = `xxh3_64(R 的 kind ‖ R 的体)`
  - 两次 R:都不交 `process`、不写成 Input,各写一条 Meta `Note{Boot, InputQuarantined}`(P0);R' 照常处理
  - 新段 `replayer verify` 通过
- 实现:`crates/hft-sequencer/tests/poison.rs::seq_270_runtime_poison_quarantine`

### SEQ-271 隔离集合只在本进程生命周期内有效

- 规则:[毒丸输入隔离](README.md#毒丸输入隔离)
- 级别:unit
- 里程碑:M2
- 前置:SEQ-270 之后干净 Drain 重启,`RecoverPlan.quarantine` 为空
- 步骤:
  1. R 再次到达
- 期望:
  - R 被交 `process` 并写成 Input
- 实现:`crates/hft-sequencer/tests/poison.rs::seq_271_quarantine_process_lifetime`

### SEQ-272 两层毒丸隔离与从快照恢复

- 规则:[毒丸输入隔离](README.md#毒丸输入隔离)、[版本变化与回滚](README.md#版本变化与回滚)
- 级别:dst
- 里程碑:M5
- 前置:B0;hft-core `startup.poison_crash_limit = 2`
- 故障:A:实盘处理某条 Input 时连续崩溃 2 次(运行期);B:重放同一区间时连续崩溃 2 次(恢复期)
- 种子:PR 50 + 回归集;每夜日期派生
- 断言:A 的第三次启动尾部照常重放、该哈希进隔离集合、再到达时只有 `Note{InputQuarantined}`、`POISON` 置位;B 的第三次启动从快照起步、不重放尾部;两层计数互不干扰;两者都禁用自动解除 `Recovery`;INV-6
- 备注:跨组件期望见 RC-6
- 实现:`crates/hft-testkit/src/scenarios/seq.rs::seq_272_poison_two_layers`

### SEQ-273 版本变化时的启动方式

- 规则:[版本变化与回滚](README.md#版本变化与回滚)
- 级别:table
- 里程碑:M2
- 前置:journal 由上一版本写出;每行描述二进制与 journal 末尾
- 实现:`crates/hft-sequencer/tests/version.rs::seq_273_version_change_boot`

| 行 | 情形与 RecoverPlan | 期望 |
|---|---|---|
| SEQ-273.1 | 版本相同,`Latest` + `replay_tail = true` | 从最近快照起步并重放尾部 |
| SEQ-273.2 | 版本不同,末尾是 Drain 快照、其后无 Input,`DrainTail` + `replay_tail = false` | 从 Drain 快照起步、不重放;节 5 交策略 `restore`(当前版本 = 快照 `desc_version` + 1,被接受) |
| SEQ-273.3 | 版本不同,末尾不是 Drain 快照,`Latest` | `Err(BootError::VersionNeedsDrain)` |
| SEQ-273.4 | 同 SEQ-273.3,`NewestReadable` + `replay_tail = false` + `FROM_SNAPSHOT \| TAIL_LOST` | 从本二进制能解析的最新快照起步;全部账户 `TAIL_LOST`、`FROM_SNAPSHOT` |
| SEQ-273.5 | 版本不同,节 5 `desc_version` 比当前旧两个版本,`DrainTail` | `Err(SnapshotUndecodable{section: 5})` |

### SEQ-274 新版本崩溃后回滚旧版本

- 规则:[版本变化与回滚](README.md#版本变化与回滚)
- 级别:integ
- 里程碑:M2
- 前置:旧版本 V1 干净 Drain 写出快照 D;新版本 V2 从 D 起步运行后崩溃(末尾不是 Drain 快照)
- 步骤:
  1. 以 V1 `boot(Journal(Latest))`
  2. 以 V1 `boot(Journal(NewestReadable, replay_tail = false, FROM_SNAPSHOT | TAIL_LOST))`
- 期望:
  - 步骤 1:`VersionNeedsDrain`
  - 步骤 2:从 V1 能解析的最新快照(D 或 V2 写的、V1 可读的快照)起步成功;每个账户 `Recovery` cap 且闸门按严格条件;新 epoch > V2 的 epoch
- 备注:跨组件期望见 RC-7
- 实现:`crates/hft-sequencer/tests/version.rs::seq_274_rollback_from_snapshot`

### SEQ-275 启动闸门的输入翻成 Gate 事件

- 规则:[启动闸门](README.md#启动闸门)
- 级别:table
- 里程碑:M5
- 前置:恢复后,账户 1 闸门未放行;每行投递所列 Input
- 实现:`crates/hft-sequencer/tests/gates.rs::seq_275_gate_inputs`

| 行 | Input | 期望 `apply` 的输入 |
|---|---|---|
| SEQ-275.1 | 账户 1 在 B 的 Unknown 槽由 2 个变为 0 个 | `Gate(Unknown{account: 1, count: 0})` |
| SEQ-275.2 | a1 的簿进入 Live、BBO 首帧到达 | `Gate(Feeds{account: 1, ok: true})`(全部订阅 inst 可用时) |
| SEQ-275.3 | 对账报告组末条 | `Gate(Recon(&ReconGateInput))` |
| SEQ-275.4 | `SetBalanceBaseline{1, A}` | 每个资产一条 `Gate(Disposed{.., how: Baselined})` |
| SEQ-275.5 | `Timer{KillSelfTestOk, account 1}` | `Gate(KillSelfTest{account: 1})` |
| SEQ-275.6 | 任一行导致 `gate_bits` 变化 | 同一事件内一条 `Note{StartupGate}` |

### SEQ-276 存储档位的 cap 与内部 Drain

- 规则:[存储降级](README.md#存储降级)
- 级别:table
- 里程碑:M2
- 前置:B0 稳态;没有未到达的 `reason ∈ {MarginStress, Hard}` 引擎目标;每行从上一行的状态继续
- 实现:`crates/hft-sequencer/tests/storage.rs::seq_276_storage_levels`

| 行 | Input | 期望 |
|---|---|---|
| SEQ-276.1 | `Timer{PersistLag, level 1}` | `Note{CapSet}`(Storage);s1 的 Open 意图被拒;Close 放行 |
| SEQ-276.2 | `Timer{StorageOk}` | `Note{CapCleared}`(Storage);Open 放行 |
| SEQ-276.3 | `Timer{DiskLow, level 2}` | 同一 Input 内 `Draining{cancel_resting = true}`、venue 级 `CancelAll`;Drain 完成后 `Done{restart: false, cause: StorageCritical}` |

### SEQ-277 存在未到达的引擎减仓目标时推迟 StorageCritical 的 Drain

- 规则:[存储降级](README.md#存储降级)
- 级别:integ
- 里程碑:M5
- 前置:B0 稳态;账户 2 已有 `Kill{Account(2), flatten}`(引擎目标 0、`reason = Hard`,两片之间冷却中,未到达)
- 步骤:
  1. 交出 `Timer{PersistLag, level 2}`
  2. 引擎目标的最后一片成交,目标到达(清仓锁仍在,`Resume{Kill}` 之前不解除)
- 期望:
  - 步骤 1:`Storage` cap 生效(禁开);没有 `CapSet{Draining}`;清仓切片继续出环;`mode.apply` 收到的视图中 `engine_reduce_pending() = true`(转调 `exec.engine_reduce_pending()`);`risk.mode.drain_deferred() = true`
  - 步骤 1 与 2 之间的每条 Input:执行层推进之后恰好调一次 `apply(StorageRecheck)`,无 `drain`
  - 步骤 2:承载最后一片成交的那条 `OrderEvent` Input 内,`exec.advance` 之后的 `apply(StorageRecheck)` 产出 `drain = StorageCritical`,同一 Input 内 `CapSet{Draining}` 并进入 Drain 流程,最终 `Done{restart: false, cause: StorageCritical}`;清仓锁仍在不阻止 Drain
- 备注:推迟条件与推迟结束后的产出时机归 hft-risk(RF7);跨组件场景见 C10
- 实现:`crates/hft-sequencer/tests/storage.rs::seq_277_storage_drain_deferred_by_engine_target`

### SEQ-278 命令路径的复核顺序

- 规则:[命令路径](README.md#命令路径)
- 级别:table
- 里程碑:M2
- 前置:B0 稳态;每行的命令同时不满足所列多项,期望只报复核顺序中的第一项
- 实现:`crates/hft-sequencer/tests/commands.rs::seq_278_check_order`

| 行 | 同时不满足 | 期望 `CmdResult` |
|---|---|---|
| SEQ-278.1 | 已过期 + `cmd_id` 重复 | `Expired` |
| SEQ-278.2 | `cmd_id` 重复 + 角色不足 | `Duplicate` |
| SEQ-278.3 | 角色不足 + 账户不一致 | `RoleInsufficient` |
| SEQ-278.4 | 账户不一致 + 需要确认未确认 | `InvalidArgs{AccountMismatch}` |
| SEQ-278.5 | 需要确认未确认 + 前置条件不满足 | `NeedsConfirm` |

### SEQ-279 cmd_id 去重表保留最近 recent_cmd_ids 个

- 规则:[命令路径](README.md#命令路径)、[环与容量](README.md#环与容量)
- 级别:table
- 里程碑:M2
- 前置:B0(`recent_cmd_ids = 64`);命令 C(`Kill{Account(1)}`)已 `Applied`;之后有 n 条不同 `cmd_id` 的命令
- 实现:`crates/hft-sequencer/tests/commands.rs::seq_279_cmd_dedup_window`

| 行 | n,然后重发 C | 期望 |
|---|---|---|
| SEQ-279.1 | 63 | `CmdResult{Duplicate}`,不执行 |
| SEQ-279.2 | 64 | C 已被淘汰,照常执行(返回 `NoOp`,已在目标状态) |
| SEQ-279.3 | 0,干净重启后重发 | `Duplicate`(节 15 进快照) |

### SEQ-280 账户语义复核

- 规则:[命令路径](README.md#命令路径)
- 级别:table
- 里程碑:M2
- 前置:B0 稳态
- 实现:`crates/hft-sequencer/tests/commands.rs::seq_280_account_semantics`

| 行 | 命令 | 期望 |
|---|---|---|
| SEQ-280.1 | `CmdHdr.account = 1`,`CmdScope.account = 2` | `InvalidArgs{AccountMismatch}` |
| SEQ-280.2 | `AdoptExchange`(无 scope)载荷 `account = 2`,`CmdHdr.account = 1` | `InvalidArgs{AccountMismatch}` |
| SEQ-280.3 | 两者一致 | 进入下一项复核 |

### SEQ-281 PREVIEW 不改任何状态

- 规则:[命令路径](README.md#命令路径)
- 级别:table
- 里程碑:M2
- 前置:B0 稳态;每行发带 PREVIEW 标志的命令
- 实现:`crates/hft-sequencer/tests/commands.rs::seq_281_preview_no_state_change`

| 行 | 命令 | 期望 |
|---|---|---|
| SEQ-281.1 | Trader `SetMode{→ Live}` | `CmdResult{preview}`;状态哈希与 `ctl_state_version` 不变 |
| SEQ-281.2 | pipeline `SetParams` | 同上;参数不变 |
| SEQ-281.3 | Viewer `Kill{Account(1)}` | 状态哈希中只有预览表多一条 `{cmd_hash, issuer, preview_engine_ns}`;模式与 cap 不变 |

### SEQ-282 需要确认的命令与版本复核

- 规则:[命令路径](README.md#命令路径)
- 级别:table
- 里程碑:M2
- 前置:B0 稳态,`ctl_state_version = 40`;Trader 发 `Resume{Kill, Account(1)}`(升级类)
- 实现:`crates/hft-sequencer/tests/commands.rs::seq_282_confirm_and_version`

| 行 | 确认字段 | 期望 |
|---|---|---|
| SEQ-282.1 | `confirmed = 0` | `NeedsConfirm` |
| SEQ-282.2 | `confirmed = 1`、`expect_state_version = 40` | 执行 |
| SEQ-282.3 | `confirmed = 1`、`expect_state_version = 39` | `ConfirmInvalid{StateChanged}` |
| SEQ-282.4 | 降级命令 `Kill{Account(1)}`,`confirmed = 0` | 执行(不需确认) |
| SEQ-282.5 | 任一行 | `CmdResult` 带当前 `ctl_state_version` |

### SEQ-283 SetParams 的确认判定

- 规则:[命令路径](README.md#命令路径)
- 级别:table
- 里程碑:M2
- 前置:B0 稳态;"Live 实例"取 control-plane 的定义;s1(账户 1 Live)有一个期望模式 Live 的 cell;s2(账户 2 Live)的全部 cell 期望模式为 Shadow;s9 在 Sim 账户;各行 `confirmed = 0`
- 实现:`crates/hft-sequencer/tests/commands.rs::seq_283_set_params_confirmation`

| 行 | 主体 / 目标 / 变化 | 期望 |
|---|---|---|
| SEQ-283.1 | pipeline / s1 / 新增 group | `Applied` |
| SEQ-283.2 | pipeline / s1 / g1 `open_allowed` 0 → 1 | `Applied` |
| SEQ-283.3 | pipeline / s9 / 新增 group | `Applied` |
| SEQ-283.4 | `cli:<login>` / s1 / 新增 `open_allowed = 1` 的 group | `NeedsConfirm` |
| SEQ-283.5 | `cli:<login>` / s1 / g1 `open_allowed` 0 → 1 | `NeedsConfirm` |
| SEQ-283.6 | `cli:<login>` / s1 / 只改阈值类字段 | `Applied` |
| SEQ-283.7 | pipeline / s1 / `structural_changed = true` | `NeedsConfirm` |
| SEQ-283.8 | `cli:<login>` / s1 / `structural_changed = true` | `NeedsConfirm` |
| SEQ-283.9 | pipeline / s2 / `structural_changed = true` | `Applied` |
| SEQ-283.10 | pipeline / s9 / `structural_changed = true` | `Applied` |

### SEQ-284 Draining 与闸门未放行期间的升级命令

- 规则:[命令路径](README.md#命令路径)、[Drain](README.md#drain)
- 级别:table
- 里程碑:M2
- 前置:每行的状态如列
- 实现:`crates/hft-sequencer/tests/commands.rs::seq_284_draining_and_gate`

| 行 | 状态 / 命令 | 期望 |
|---|---|---|
| SEQ-284.1 | `Draining` / 已确认的 `SetMode{→ Live}` | `PreconditionFailed{Draining}` |
| SEQ-284.2 | `Draining` / 已确认的 `Kill{Account(1), flatten}` | `Applied`;`exec.engine_flatten(1, ..)` |
| SEQ-284.3 | `Draining` / 未确认的 `Kill{Account(1), flatten}` | `NeedsConfirm` |
| SEQ-284.4 | 账户 1 闸门未放行 / 已确认的 `Kill{Account(1), flatten}` | `Applied` |
| SEQ-284.5 | 账户 1 闸门未放行 / 已确认的 `Resume{Recovery, Account(1)}` | `PreconditionFailed{GateNotReleased}` |

### SEQ-285 SetParams 第 1 项:绑定、种类与腿的复核

- 规则:[命令路径](README.md#命令路径)
- 级别:table
- 里程碑:M2
- 前置:B0 稳态;每行一条 pipeline 主体的 `SetParams`,其余字段合法
- 实现:`crates/hft-sequencer/tests/commands.rs::seq_285_set_params_binding`

| 行 | 违反项 | 期望 |
|---|---|---|
| SEQ-285.1 | `SetParams{strategy 1, account 2}`(账户 2 绑定 s2) | `InvalidArgs{AccountBound}`;旧参数生效 |
| SEQ-285.2 | `hdr.kind_id = 0xFF02`,s1 的 `InstanceMeta.kind_id = 0xFF01` | `InvalidArgs{KindMismatch}` |
| SEQ-285.3 | 种类 `non_prod_only = true`,账户 `is_prod = true` | `InvalidArgs{ParamInvalid}`,arg kind = `NonProdOnly` |
| SEQ-285.4 | 同 SEQ-285.3,账户 `is_prod = false` | 通过本项 |
| SEQ-285.5 | 一条腿的 venue 不在账户内 | `InvalidArgs{GroupCheck}`,arg = 41(`LoadErrCode::LegVenueNotInAccount`) |
| SEQ-285.6 | 同一 GroupSpec 两腿在同一交易所 | `InvalidArgs{SameExchangeLegs}` |
| SEQ-285.7 | 两腿报价资产不同 | `InvalidArgs{GroupCheck}`,arg = 43(`LoadErrCode::QuoteMismatch`) |

### SEQ-286 SetParams 第 2、3 项:版本与 GroupId

- 规则:[命令路径](README.md#命令路径)
- 级别:table
- 里程碑:M2
- 前置:B0 稳态;s1 当前 version 5
- 实现:`crates/hft-sequencer/tests/commands.rs::seq_286_set_params_version_group_id`

| 行 | 输入 | 期望 |
|---|---|---|
| SEQ-286.1 | version 5 | `StaleVersion` |
| SEQ-286.2 | version 4 | `StaleVersion` |
| SEQ-286.3 | version 6 | 通过本项 |
| SEQ-286.4 | version 6,`base_version = 4` | `VersionConflict` |
| SEQ-286.5 | 含 `GroupId = GROUP_NONE` | `InvalidArgs{BadGroup}` |
| SEQ-286.6 | 含 `GroupId = GROUP_ALL` | `InvalidArgs{BadGroup}` |
| SEQ-286.7 | 含 `GroupId = 0x0200_0005`(高 8 位 2 ≠ 1) | `InvalidArgs{BadGroup}` |
| SEQ-286.8 | 同一 blob 内两个 `0x0100_0003` | `InvalidArgs{GroupIdDup}` |

### SEQ-287 SetParams 第 4、5 项:InstOwned 与 GroupHasPosition

- 规则:[命令路径](README.md#命令路径)
- 级别:table
- 里程碑:M2
- 前置:B0 稳态;s1 的 g1(Live)引用 a1、b1
- 实现:`crates/hft-sequencer/tests/commands.rs::seq_287_inst_owned_group_has_position`

| 行 | 输入与状态 | 期望 |
|---|---|---|
| SEQ-287.1 | 新增 Live group 也引用 (1, a1) | `InvalidArgs{InstOwned}` |
| SEQ-287.2 | 新增 group 引用 (1, a1),期望模式 Shadow | 通过本项 |
| SEQ-287.3 | 移除 g1,g1 的 a1 有库存 0.001 | `PreconditionFailed{GroupHasPosition}` |
| SEQ-287.4 | 移除 g1,g1 有活动目标 | `PreconditionFailed{GroupHasPosition}` |
| SEQ-287.5 | 移除 g1,g1 有在途单 | `PreconditionFailed{GroupHasPosition}` |
| SEQ-287.6 | g1 有 b1 持仓,把现货腿换到另一 venue(指纹变化) | `PreconditionFailed{GroupHasPosition}` |
| SEQ-287.7 | 移除无持仓、无库存、无在途、无目标的 group | 通过本项 |

### SEQ-288 SetParams 第 6 项:容量

- 规则:[命令路径](README.md#命令路径)、[环与容量](README.md#环与容量)
- 级别:table
- 里程碑:M2
- 前置:B0(`max_cells_total = 32`、`max_subs_per_inst = 4`、`max_insts = 64`);当前 cell 30 个,a1 被 3 个 cell 引用,订阅 inst 63 个
- 实现:`crates/hft-sequencer/tests/commands.rs::seq_288_set_params_capacity`

| 行 | 新参数 | 期望 |
|---|---|---|
| SEQ-288.1 | 新增 2 个 cell(合计 32) | 通过 |
| SEQ-288.2 | 新增 3 个 cell(合计 33) | `CapacityExceeded`;旧参数生效 |
| SEQ-288.3 | 新增 1 个引用 a1 的 cell(引用数 4) | 通过 |
| SEQ-288.4 | 新增 2 个引用 a1 的 cell(引用数 5) | `CapacityExceeded` |
| SEQ-288.5 | 新增 1 个新 inst(订阅 64) | 通过 |
| SEQ-288.6 | 新增 2 个新 inst(订阅 65) | `CapacityExceeded` |

### SEQ-289 SetParams 第 7 项:两腿 lot 对齐与尘埃线

- 规则:[命令路径](README.md#命令路径)
- 级别:table
- 里程碑:M2
- 前置:B0 稳态;账户 1 `ExecConfig.force_slice.max_chunk_notional = 1 000 USDT`、`dust_notional = 10 USDT`;每行新增一个两腿 group(或改已有 group)
- 实现:`crates/hft-sequencer/tests/commands.rs::seq_289_lot_and_dust`

| 行 | 腿与参考价 | 期望 |
|---|---|---|
| SEQ-289.1 | lot 0.001 与 0.01(整除) | 通过 |
| SEQ-289.2 | lot 0.004 与 0.01(`pair_lot` = 0.02),mark 50 000(名义 1 000) | 通过 |
| SEQ-289.3 | 同上,mark 50 001(名义 1 000.02) | `InvalidArgs{LotMismatch}` |
| SEQ-289.4 | 同上,两腿都无 mark,有簿 mid 49 000 | 通过(用 mid) |
| SEQ-289.5 | 同上,无 mark、无 mid | `InvalidArgs{LotMismatch}` |
| SEQ-289.6 | 存量 group 的 lot 0.004/0.01 因 mark 涨到 60 000 超标,本次只改阈值字段 | 通过(存量 group 不查) |
| SEQ-289.7 | 新增 group 交易腿 `min_notional = 10 USDT` | 通过(等于 `dust_notional`) |
| SEQ-289.8 | 新增 group 交易腿 `min_notional = 10.01 USDT` | `InvalidArgs{DustBelowMinNotional}`;旧参数生效 |

### SEQ-290 SetParams 第 8 项:Model 内存与检查点预算

- 规则:[命令路径](README.md#命令路径)、[advisor 与 Model](README.md#advisor-与-model)
- 级别:table
- 里程碑:M2
- 前置:B0(`advisor_mem_budget_mib = 256`、`advisor_ckpt_max_mib = 1 024`);s1 以外实例的 Σ `model_mem` = 200 MiB、Σ `model_ckpt` = 300 MiB;每行给 s1 新参数的 `model_mem` 与 `model_ckpt`
- 实现:`crates/hft-sequencer/tests/commands.rs::seq_290_model_budget`

| 行 | s1 新 `model_mem` / `model_ckpt` | 期望 |
|---|---|---|
| SEQ-290.1 | 56 MiB / 100 MiB(合计 256 MiB;2 × 400 = 800 MiB) | 通过 |
| SEQ-290.2 | 56 MiB + 1 B / 100 MiB | `InvalidArgs{ParamInvalid}`,kind `ModelMemBudget`;旧参数生效 |
| SEQ-290.3 | 10 MiB / 212 MiB(2 × 512 = 1 024 MiB) | 通过 |
| SEQ-290.4 | 10 MiB / 212 MiB + 1 B | `ParamInvalid{ModelMemBudget}` |

### SEQ-291 SetParams 的应用步骤

- 规则:[命令路径](README.md#命令路径)
- 级别:table
- 里程碑:M5
- 前置:B0 稳态;s2 有 g2、g2b;新参数移除 g2b、新增 g2c,`default_mode = Shadow`
- 实现:`crates/hft-sequencer/tests/commands.rs::seq_291_set_params_apply`

| 行 | 检查 | 期望(同一 Input 内) |
|---|---|---|
| SEQ-291.1 | 策略 | `on_params` 一次 |
| SEQ-291.2 | 订阅 | CSR 重建;需要时出环 `FeedCmd` |
| SEQ-291.3 | 模式 | `apply(GroupsChanged{added: [g2c], removed: [g2b]})`;g2c 期望模式 Shadow |
| SEQ-291.4 | 组件 group 表 | OMS、ledger、执行层的 group 表同步;`risk.on_groups_changed` 一次 |
| SEQ-291.5 | Advice | g2c `spec_seq` = 本命令 seq |
| SEQ-291.6 | 清理 | g2b 的定时器与目标撤销,各写 `Note` |
| SEQ-291.7 | `on_params` 返回 Err | `InvalidArgs{ParamInvalid}`,kind `Strategy`;以上各项都不发生,旧参数生效 |

### SEQ-292 SetRisk 的拒绝

- 规则:[命令路径](README.md#命令路径)
- 级别:table
- 里程碑:M5
- 前置:B0 稳态;账户 1 `force_slice.max_chunk_notional = 1 000 USDT`
- 实现:`crates/hft-sequencer/tests/commands.rs::seq_292_set_risk`

| 行 | 新 `RiskParams` | 期望 |
|---|---|---|
| SEQ-292.1 | 账户 1 `max_order_notional = 999 USDT` | `InvalidArgs{ExecRiskMismatch}`;旧参数生效 |
| SEQ-292.2 | 账户 1 `max_order_notional = 1 000 USDT` | `Applied`;`risk.set_params(p, scope)` |
| SEQ-292.3 | 行数超过预分配 | `CapacityExceeded`;旧参数生效 |

### SEQ-293 ModeErr 到 CmdResult 的映射

- 规则:[命令路径](README.md#命令路径)
- 级别:table
- 里程碑:M5
- 前置:B0;每行构造使 `risk.mode.apply` 返回所列 `ModeErr` 的已确认命令
- 实现:`crates/hft-sequencer/tests/commands.rs::seq_293_mode_err_mapping`

| 行 | ModeErr | 期望 `CmdResult` |
|---|---|---|
| SEQ-293.1 | `GateNotReleased` | `PreconditionFailed{GateNotReleased}` |
| SEQ-293.2 | `Draining` | `PreconditionFailed{Draining}` |
| SEQ-293.3 | `NakedNotCleared` | `PreconditionFailed{NakedNotCleared}` |
| SEQ-293.4 | `ReconNotClean` | `PreconditionFailed{ReconNotClean}` |
| SEQ-293.5 | `ResumeLimit` | `PreconditionFailed{ResumeLimit}` |
| SEQ-293.6 | `NoMatchingCap` | `PreconditionFailed{NoMatchingCap}` |
| SEQ-293.7 | `BrokenRemaining`(`Resume{HedgeFailure}`,账户仍有 Broken) | `Rejected{BrokenRemaining}` |
| SEQ-293.8 | `NotResumable` | `InvalidArgs{NotResumable}` |
| SEQ-293.9 | `InvalidScope` | `InvalidArgs{BadCombination}` |

### SEQ-294 Kill 的参数组合

- 规则:[命令路径](README.md#命令路径)
- 级别:table
- 里程碑:M2
- 前置:B0 稳态;Trader 发出,需要确认的已确认
- 实现:`crates/hft-sequencer/tests/commands.rs::seq_294_kill_combinations`

| 行 | 命令 | 期望 |
|---|---|---|
| SEQ-294.1 | `Kill{Global, hard}` | `Applied` |
| SEQ-294.2 | `Kill{Account(1), hard}` | `Applied`;账户 2 不受影响 |
| SEQ-294.3 | `Kill{Venue(B), hard}` | `InvalidArgs{BadCombination}` |
| SEQ-294.4 | `Kill{Account(1), hard, flatten}` | `InvalidArgs{BadCombination}` |
| SEQ-294.5 | `Kill{Global, flatten}` | `InvalidArgs{BadCombination}` |
| SEQ-294.6 | `Kill{Account(9)}`(Sim) | `Applied`;s9 的 Open 被检查链拒绝 |

### SEQ-295 停机标记的 Kill 不随 Recovery 自动解除

- 规则:[命令路径](README.md#命令路径)
- 级别:integ
- 里程碑:M5
- 前置:干净 Drain 写快照;重启时实盘驱动带停机标记 `Kill{Global}`;`auto_resume_after_clean_drain = 1`
- 步骤:
  1. 重启,闸门放行
  2. s1 发 Open 意图
  3. 已确认的 `Resume{Kill, Global}` 之后 s1 再发 Open 意图
  4. `cli:systemd` 主体发 `Resume{Kill}`
- 期望:
  - 步骤 1:`Kill` 在闸门放行前被应用,cap 进快照;放行时 `Recovery` 自动解除,Kill cap 仍在
  - 步骤 2:Open 被拒
  - 步骤 3:Open 放行
  - 步骤 4:`RoleInsufficient`
- 备注:跨组件场景见 MD-13
- 实现:`crates/hft-sequencer/tests/commands.rs::seq_295_stop_marker_kill_persists`

### SEQ-296 AdoptExchange

- 规则:[命令路径](README.md#命令路径)
- 级别:table
- 里程碑:M9
- 前置:B0 稳态;report 950 已生效(`view_seq = 10 000`),其中 (1, B, b1) 仓位差异 +0.002
- 实现:`crates/hft-sequencer/tests/commands.rs::seq_296_adopt_exchange`

| 行 | 命令与状态 | 期望 |
|---|---|---|
| SEQ-296.1 | `AdoptExchange{1, B, b1, 950, Position, group g1}`,b1 在 seq 10 000 之后无成交 | `ledger.apply_adjust(AdjustDelta{..})`;`oms.adopt_adjust(1, g1, b1, +0.002)`;`Gate(Disposed{.., Adopted})`;不建 OMS 槽;`Applied` |
| SEQ-296.2 | 同上,`group = GROUP_NONE` | 不调 `oms.adopt_adjust`;其余同上 |
| SEQ-296.3 | b1 在 seq 10 001 有一笔成交 | `PreconditionFailed{StaleReport}`;账本不变 |

### SEQ-297 SetBalanceBaseline、RecordTransfer 与 VenueNotice 的种类

- 规则:[命令路径](README.md#命令路径)
- 级别:table
- 里程碑:M2
- 前置:B0 稳态
- 实现:`crates/hft-sequencer/tests/commands.rs::seq_297_baseline_transfer_notice`

| 行 | 命令 | 期望 |
|---|---|---|
| SEQ-297.1 | `SetBalanceBaseline{1, A}`,ledger 返回 2 个资产 | 2 条 `Gate(Disposed{.., Baselined})` |
| SEQ-297.2 | `RecordTransfer{..}` | `ledger.apply_transfer(kind = Record)` 一次;无出环 |
| SEQ-297.3 | `VenueNotice{B, *, Maintenance, 10:00, 10:30}` | 节 24 新增 `{venue 2, kind 3, inst 0}`;`apply(Notice)` |
| SEQ-297.4 | `VenueNotice{B, b1, FundingInterval, 3 600}` | 节 24 新增 `{kind 2, inst b1}` |

### SEQ-298 SimReset 与 SimTransfer

- 规则:[命令路径](README.md#命令路径)、[模拟账户](README.md#模拟账户)
- 级别:table
- 里程碑:M2
- 前置:B0 稳态;Sim 账户 9 余额 USDT 1 000
- 实现:`crates/hft-sequencer/tests/commands.rs::seq_298_sim_reset_transfer`

| 行 | 命令与状态 | 期望 |
|---|---|---|
| SEQ-298.1 | `SimReset{1}`(Live 账户) | `InvalidArgs{NotSim}` |
| SEQ-298.2 | `SimReset{9}`,g9 有效模式 Live | `PreconditionFailed{SimNotIdle}` |
| SEQ-298.3 | `SimReset{9}`,g9 有活动目标 | `SimNotIdle` |
| SEQ-298.4 | `SimReset{9}`,有 Unknown 槽 | `SimNotIdle` |
| SEQ-298.5 | `SimReset{9}`,有非终态 IOC 槽 | `SimNotIdle` |
| SEQ-298.6 | `SimReset{9}`,只有挂单,g9 处于 `Broken` 与 `HedgeFailure` | 经 out-sim 交模拟 gw;同一事件内 `ledger.sim_reset`、`oms.sim_reset(9)`、`exec.clear_account(9)`、对每个 group `risk.on_legs` → g9 `AutoClear{Broken}`;`HedgeFailure` 仍在 |
| SEQ-298.7 | `SimTransfer{9, 2 000 USDT 转出}` | `PreconditionFailed{InsufficientBalance}` |
| SEQ-298.8 | `SimTransfer{9, 500 USDT 转出}` | `ledger.apply_transfer(kind = Sim)`;out-sim 一条 `SimTransfer` |
| SEQ-298.9 | `SimTransfer{1, ..}` | `InvalidArgs{NotSim}` |

### SEQ-299 WriteOffResidual

- 规则:[命令路径](README.md#命令路径)
- 级别:table
- 里程碑:M2
- 前置:B0 稳态;Admin 发出;每行的 group 状态如列
- 实现:`crates/hft-sequencer/tests/commands.rs::seq_299_write_off_residual`

| 行 | 状态 / 确认 | 期望 |
|---|---|---|
| SEQ-299.1 | g1 `Broken`,b1 永续残余 −0.0004(高于尘埃线、低于最小下单量),静止 / 已确认 | `oms.write_off_residual(1, g1)` 返回 `WriteOff{qty: −0.0004}` → `ledger.apply_adjust(AdjustDelta{kind: WriteOff})`(该量进 unassigned)→ `exec.on_leg_adjust(1, g1)` → `risk.on_legs` → `AutoClear{Broken}`;`HedgeFailure` 保留;无任何订单;`Applied` |
| SEQ-299.2 | g1 非 Broken、静止,残余低于最小下单量 / 已确认 | 同样核销;之后可被 `SetParams` 移除 |
| SEQ-299.3 | g1 无裸量,只有对冲腿余数 / 已确认 | `WriteOff{qty: 0}`;余数清零;账不变,只写注记 |
| SEQ-299.4 | g1 有即时在途 / 已确认 | `PreconditionFailed{NotQuiescent}` |
| SEQ-299.5 | 任意 / 未确认 | `NeedsConfirm` |
| SEQ-299.6 | group 不属于账户 1 绑定实例的当前参数 | `InvalidArgs{UnknownGroup}` |
| SEQ-299.7 | `VerifyDriver` 重放 SEQ-299.1 | 逐字节一致 |

### SEQ-300 SmokeOrder

- 规则:[命令路径](README.md#命令路径)、[意图管线](README.md#意图管线)
- 级别:table
- 里程碑:M2
- 前置:B0 稳态;每行 `SmokeOrder{account 1, B, b1, Buy, ttl_ms 30 000}`,除另列外为 Admin 已确认
- 实现:`crates/hft-sequencer/tests/commands.rs::seq_300_smoke_order`

| 行 | 状态 | 期望 |
|---|---|---|
| SEQ-300.1 | g1 有效模式 Shadow | `oms.smoke_intent` 生成 PostOnly 意图;不经模式门;出环一条带 `OrderFlags::SMOKE` 的单;`CmdResult{Applied}`,同一事件内其 `OrderDecision.trigger_seq` = 命令 seq;没有任何策略收到 `EventRef::Order` |
| SEQ-300.2 | CloseOnly | 同样出环 |
| SEQ-300.3 | 账户 1 HARD | `Rejected{SmokeRiskReject}` |
| SEQ-300.4 | (1, B) 已有一张冒烟单 | `PreconditionFailed{SmokeActive}` |
| SEQ-300.5 | b1 报价不健康 | `PreconditionFailed{SmokeNoPrice}` |
| SEQ-300.6 | Trader 发出 | `RoleInsufficient` |
| SEQ-300.7 | `account 9`(Sim) | `InvalidArgs{NotLive}` |
| SEQ-300.8 | SEQ-300.1 之后 `ttl_ms` 到期 | `Timer{Oms, SmokeTtl}` 交 `oms.on_smoke_ttl` 撤单 |
| SEQ-300.9 | SEQ-300.1 的单成交 | 成交入账(`GROUP_NONE`);不投递给任何策略 |

### SEQ-301 StrategyCmd

- 规则:[命令路径](README.md#命令路径)
- 级别:table
- 里程碑:M2
- 前置:B0 稳态;s1 声明了命令;Trader 发出
- 实现:`crates/hft-sequencer/tests/commands.rs::seq_301_strategy_cmd`

| 行 | 命令与状态 | 期望 |
|---|---|---|
| SEQ-301.1 | `StrategyCmd{1, g1, 32 B}`,已确认 | 同一 Input 内只有 s1 收到 `EventRef::Manual{gix: 0, reduce_only: false}` |
| SEQ-301.2 | `StrategyCmd{1, g2}` | `InvalidArgs{UnknownGroup}` |
| SEQ-301.3 | `StrategyCmd{7, ..}` | `UnknownTarget` |
| SEQ-301.4 | 不带 `reduce_only`、未确认 | `NeedsConfirm` |
| SEQ-301.5 | 不带 `reduce_only`、`Draining` | `PreconditionFailed{Draining}` |
| SEQ-301.6 | 带 `reduce_only`、`Draining`,s1 在该次投递里发 Open 与 Close | Open 被拒(`CLOSE_ONLY`,不出环);Close 出环;两者的 `OrderDecision.trigger_seq` = 命令 seq |
| SEQ-301.7 | 带 `reduce_only`,s1 设增加 b1 腿仓位的目标 | 执行层 `TargetRejected` |
| SEQ-301.8 | `StrategyCmd{1, GROUP_NONE}` | s1 收到 `Manual{gix: GIX_NONE}` |

### SEQ-302 InjectWarmup 的 sequencer 侧

- 规则:[命令路径](README.md#命令路径)
- 级别:table
- 里程碑:M2
- 前置:B0 稳态;s2 声明 `warmup(Part::Both)`,s1 未声明
- 实现:`crates/hft-sequencer/tests/commands.rs::seq_302_inject_warmup`

| 行 | 命令与状态 | 期望 |
|---|---|---|
| SEQ-302.1 | `InjectWarmup{instance 2, chunk 0, last 1, 4 096 B}` | `Applied`;同一 Input 内 s2 快速部分收到 `EventRef::Warmup` |
| SEQ-302.2 | `InjectWarmup{instance 1, ..}` | `Applied`;s1 快速部分收不到 |
| SEQ-302.3 | 账户 1 闸门未放行、`Draining` | 都 `Applied` |
| SEQ-302.4 | `instance 7` | `UnknownTarget` |
| SEQ-302.5 | ctl 环 20 条记录长 4 096 B 的 `InjectWarmup`(载荷 = 4 096 − 记录头)后跟一条 `Kill{Account(1)}` | 每轮至多 2 条预热块;`Kill` 在 ≤ 11 轮内被处理(10 轮预热块之后的下一轮) |

### SEQ-303 Viewer 紧急停止

- 规则:[命令路径](README.md#命令路径)
- 级别:table
- 里程碑:M2
- 前置:B0 稳态;Viewer issuer V1;`VIEWER_CONFIRM_TTL_MS = 60 000`;命令 K = 非 HARD、不带 flatten、`cancel_resting = true` 的 `Kill{Account(1)}`;每行从"V1 已在 engine_ns T 预览 K"开始(除另列)
- 实现:`crates/hft-sequencer/tests/viewer_confirm.rs::seq_303_viewer_emergency_stop`

| 行 | 动作 | 期望 |
|---|---|---|
| SEQ-303.1 | 未预览,直接发 K(`confirmed = 0`) | `NeedsConfirm` |
| SEQ-303.2 | T + 60 000 ms,V1 带确认发 K | 应用;该预览被删除;期间 `ctl_state_version` 变化 10 次不影响 |
| SEQ-303.3 | T + 60 000 ms + 1 ns | `ConfirmInvalid{NoValidPreview}` |
| SEQ-303.4 | 另一 Viewer V2 带确认发 K | `NoValidPreview` |
| SEQ-303.5 | V1 带确认发 `Kill{Account(2)}` | `NoValidPreview`(`cmd_hash` 不同) |
| SEQ-303.6 | SEQ-303.2 之后再用同一预览确认 | `NoValidPreview` |
| SEQ-303.7 | 预览表已有 16 条,再预览第 17 条 | 最旧一条被淘汰,表仍 16 条 |
| SEQ-303.8 | 进程重启后带确认发 K | `NoValidPreview`(处理 `EngineClock` 时清空) |
| SEQ-303.9 | Viewer 发 `Kill{Account(1), hard}`、`Kill{.., flatten}`、`Kill{Venue(B)}`、`Resume{..}`、`SetMode{..}` | 各 `RoleInsufficient` |
| SEQ-303.10 | Trader 预览后 `ctl_state_version` 变化再确认 | `ConfirmInvalid{StateChanged}` |

### SEQ-304 ReplaceRefdata 的命令时复核

- 规则:[命令路径](README.md#命令路径)
- 级别:table
- 里程碑:M2
- 前置:B0 稳态,`instance.config_source = journal`;Admin 已确认
- 实现:`crates/hft-sequencer/tests/commands.rs::seq_304_replace_refdata_recheck`

| 行 | 新 blob | 期望 |
|---|---|---|
| SEQ-304.1 | 改了已有资产的 `AssetExp` | `Rejected{RefdataRecheck}`,arg = 33(`LoadErrCode::AssetChanged`);无 Drain;旧 refdata 生效;进程照常运行 |
| SEQ-304.2 | 账户 1 换绑实例,账户 1 有 b1 持仓 | `Rejected{RefdataRecheck}`,arg = 51(`LoadErrCode::BindingChangedWithPosition`) |
| SEQ-304.3 | 新订阅需求超过 `max_insts` | `Rejected{RefdataRecheck}`,arg = 0 |
| SEQ-304.4 | 同时有 SEQ-304.1 与 SEQ-304.2 的问题 | arg = 33(载入失败优先) |
| SEQ-304.5 | 合法 | `Applied`;记为待生效引用;同一 Input 内设 `Draining{cancel_resting = false}` |

### SEQ-305 Drain 第 1、2 步:设 Draining 并按 cancel_resting 撤单

- 规则:[Drain](README.md#drain)
- 级别:table
- 里程碑:M2
- 前置:B0 稳态,A、B 的 `VenueCaps.flags` 含 `BATCH_CANCEL`;账户 1 在 A 有普通 GTC 挂单 2 张、带 LegLink 的挂单第一腿 1 张;在 B 有普通挂单 1 张
- 实现:`crates/hft-sequencer/tests/drain.rs::seq_305_drain_cancel`

| 行 | 命令 | 期望(同一 Input 内) |
|---|---|---|
| SEQ-305.1 | `Drain{cancel_resting: true}` | `Draining` cap(`params[0] = 1`);A、B 各一条 venue 级 `GwCmd{CancelAll}` |
| SEQ-305.2 | `Drain{cancel_resting: false}` | `Draining` cap(`params[0] = 0`);只撤带 LegLink 的那张第一腿;3 张普通挂单保留 |
| SEQ-305.3 | Drain 之后的 `Resume{Draining}` | `InvalidArgs{NotResumable}`(不可清除) |
| SEQ-305.4 | SIGTERM 经 ctl-gateway 以 `cli:systemd` 提交的 `Drain{cancel_resting: false}` | 与 SEQ-305.2 相同 |

### SEQ-306 Drain 第 3 步:各等待条件分别阻止 Drain 结束

- 规则:[Drain](README.md#drain)
- 级别:table
- 里程碑:M5
- 前置:B0 稳态,`shutdown.drain_timeout_ms = 60 000`;每行在 `Drain{cancel_resting: true}` 时只存在所列一项未完成,该项在 Drain 开始后 2 s 完成
- 实现:`crates/hft-sequencer/tests/drain.rs::seq_306_drain_wait_conditions`

| 行 | 未完成项 | 期望 |
|---|---|---|
| SEQ-306.1 | 第一腿已成交、第二腿处于 HedgeRetry 间隙(`LINKS_ACTIVE`、`HEDGE_RETRY_PENDING`) | 链接到 Done 或 Broken 之前不写 Drain 快照 |
| SEQ-306.2 | IOC 已 Ack 未终态(`IOC_NONTERMINAL`) | 终态之前不写 |
| SEQ-306.3 | 一张 `PendingNew` 槽 | 之前不写 |
| SEQ-306.4 | L3 引擎目标未到达(两片之间冷却中) | `exec.drain_ready()` 为假;到达之前不写 |
| SEQ-306.5 | Drain 中发出的 `Kill{Account(1), flatten}` 清仓未完成 | 到达之前不写 |
| SEQ-306.6 | 通知积压非空 | 清空之前不写 |
| SEQ-306.7 | `naked_groups` 游标未完 | 取完之前不写 |
| SEQ-306.8 | 一个 `Unknown` 槽 | 不等待;快照中该槽为 `Unknown` |
| SEQ-306.9 | 每一行完成之后 | 下一个满足条件的 Input 之后写 `StateSnapshot{purpose = Drain}`,`step` 返回 `Done{restart: false, cause: Drain}` |

### SEQ-370 结束运行的事件在 step 返回 Done 之前 publish

- 规则:[主循环](README.md#主循环step)、[Drain](README.md#drain)
- 级别:unit
- 里程碑:M2
- 前置:B0 稳态;无挂单、无在途
- 步骤:
  1. ctl 环投递已确认的 Admin `Drain{cancel_resting: false}`;`step()` 直到返回非 `Busy`
- 期望:
  - 返回 `Done{restart: false, cause: Drain}`;`CmdResult{Applied}`
  - 返回时 `committed` = 全部记录数(该命令事件与 `StateSnapshot{purpose = Drain}` 都已 publish,虽然这一事件没有出环)
- 实现:`crates/hft-sequencer/tests/drain.rs::seq_370_done_publishes_the_last_event`

### SEQ-307 Drain 期间不发增仓切片,减仓照常

- 规则:[Drain](README.md#drain)
- 级别:unit
- 里程碑:M5
- 前置:B0 稳态;s2 的 g2 目标增仓尚余 0.03;账户 2 处于 L3,减仓目标尚余 2 片
- 步骤:
  1. `Drain{cancel_resting: false}`,运行到结束
- 期望:
  - Drain 开始后没有 g2 增仓切片的 `OrderDecision`;L3 的 2 片照常出环;两片都到达后才写 Drain 快照
- 实现:`crates/hft-sequencer/tests/drain.rs::seq_307_drain_increase_stops_reduce_continues`

### SEQ-308 ReplaceRefdata 发起的 Drain 末二次复核

- 规则:[Drain](README.md#drain)
- 级别:table
- 里程碑:M5
- 前置:SEQ-304.5 之后(待生效 blob R2);Drain 第 3 步满足
- 实现:`crates/hft-sequencer/tests/drain.rs::seq_308_refdata_recheck_at_drain_end`

| 行 | 情形 | 期望 |
|---|---|---|
| SEQ-308.1 | `refdata_recheck_due()` | 为真;不写快照;下一轮 a 段最先交 `Timer{Sys, RefdataRecheck}` |
| SEQ-308.2 | Timer arg = 0,`RefData::load(R2, 当前)`、绑定、容量都通过 | 写 Drain 快照,节 9 指向 R2;`Done{restart: true, cause: ReplaceRefdata}` |
| SEQ-308.3 | Timer arg = 4(装配失败项 `ThreadPlan`) | 撤销待生效引用;`Note{Sequencer, RefdataRecheckFailed}` P1,`v[0] = 1`、`v[1] = 4`;Drain 快照节 9 仍指旧 refdata;`Done{restart: false, cause: Drain}` |
| SEQ-308.4 | arg = 0,`RefData::load` 失败(`AssetChanged`) | 同上,`v[0] = 2`、`v[1] = 33` |
| SEQ-308.5 | arg = 0,换绑账户的挂单在 Drain 期间成交 | 同上,`v[0] = 3`、`v[1]` = 该账户 |
| SEQ-308.6 | arg = 0,订阅容量不足 | 同上,`v[0] = 4`、`v[1] = 0` |
| SEQ-308.7 | `VerifyDriver` 重放 SEQ-308.3 | 读日志中的同一 `Timer`,不重跑装配校验;逐字节一致 |

### SEQ-309 Drain 超时写 DrainTimeout 并照常写快照

- 规则:[Drain](README.md#drain)
- 级别:table
- 里程碑:M5
- 前置:B0 稳态,`shutdown.drain_timeout_ms = 5 000`;每行在超时时刻仍未完成所列项
- 实现:`crates/hft-sequencer/tests/drain.rs::seq_309_drain_timeout`

| 行 | 超时时未完成 | 期望 |
|---|---|---|
| SEQ-309.1 | 账户 1 一条 `Hedging` 链接(未对冲名义 500 USDT) | `Note{Sequencer, DrainTimeout}` P0:`v[0]` 的 bit 1 置位(v[0..4] 其余位为 0)、`v[4] = 1`、`v[5]` = 500 USDT 的 raw、`v[6] = 0`;写 Drain 快照;`Done{restart: false, cause: Drain}` |
| SEQ-309.2 | 账户 2 一个未到达的 `Hard` 引擎目标 | P0;`v[0]` bit 2;`v[6] = 1` |
| SEQ-309.3 | 只有通知积压未清(无链接、无引擎目标) | P1 |
| SEQ-309.4 | 由 `StorageCritical` 发起、超时 | `Done.cause = StorageCritical` |
| SEQ-309.5 | 由 `ReplaceRefdata` 发起、超时 | 先交 `Timer{Sys, RefdataRecheck}` 并复核,再写快照;复核通过 `Done{restart: true, cause: ReplaceRefdata}`,不通过 `Done{restart: false, cause: Drain}` |
| SEQ-309.6 | SEQ-309.1 之后重启 | 快照中有未结束的链接;Recovery 不自动解除 |

### SEQ-310 确定性规则的源码约束

- 规则:[确定性规则](README.md#确定性规则)
- 级别:table
- 里程碑:M2
- 前置:本 crate 的 `clippy.toml` 与 `src/`;受控夹具逐行注入违规,串行运行 `cargo clippy` 使用的实际 `clippy-driver`,断言退出状态与对应诊断;正例允许经 IO 接口读 TSC
- 实现:`crates/hft-sequencer/tests/determinism.rs::seq_310_determinism_lints`

| 行 | 加入的违规 | 期望 |
|---|---|---|
| SEQ-310.1 | `std::time::Instant::now()` | clippy 失败(`disallowed_methods`) |
| SEQ-310.2 | `std::time::SystemTime::now()` | 失败 |
| SEQ-310.3 | 对 `HashMap` 的 `iter()` | 失败(`disallowed_types`) |
| SEQ-310.4 | `thread_local!` | 失败 |
| SEQ-310.5 | `static mut` | 失败 |
| SEQ-310.6 | `rand::thread_rng()` | 失败 |
| SEQ-310.7 | 决策路径模块(`src/intents.rs`、`src/process.rs`)出现 `f64` | 失败 |
| SEQ-310.8 | `src/driver/live/` 内读 TSC | 通过(允许位置) |

### SEQ-311 同一 journal 两次、两台主机上验证结果相同

- 规则:[确定性规则](README.md#确定性规则)、[状态哈希](README.md#状态哈希)
- 级别:integ
- 里程碑:M5
- 前置:一段 DST 生成的 journal(500 000 条 Input,含关键帧、按需快照、Drain 快照);CI 的 x86_64 与 aarch64 作业
- 步骤:
  1. 每个作业以 `VerifyDriver` 跑两次,输出每个快照处的 `out_hash` 与按读者规则计算的 `state_hash`
- 期望:
  - 两个作业 × 两次共四份结果逐项相同;Output 与日志逐字节一致(测量字段除外)
- 实现:`crates/hft-sequencer/tests/determinism.rs::seq_311_cross_host_same_hash`

### SEQ-312 同一 Input 的处理结果不依赖处理耗时

- 规则:[确定性规则](README.md#确定性规则)
- 级别:prop
- 里程碑:M5
- 性质:对同一 Input 序列(按 journal 顺序交出),任意注入的处理延迟(手动 TSC 在组件调用之间前进)不改变状态哈希与 Output(测量字段除外)
- 生成:B0 上 100–2 000 条 Input;每次组件调用后 TSC 前进 0–100 µs 随机
- 参考:不注入延迟的同一序列
- 例数:PR 300;每夜 10 000
- 实现:`crates/hft-sequencer/tests/determinism.rs::seq_312_independent_of_processing_time`

### SEQ-313 Output 结构体的填充字节为零,含枚举的体按 TryFromBytes 读

- 规则:[确定性规则](README.md#确定性规则)
- 级别:table
- 里程碑:M5
- 前置:B0;运行 10 000 条 Input 后扫描全部 Output
- 实现:`crates/hft-sequencer/tests/determinism.rs::seq_313_zero_padding_try_from_bytes`

| 行 | 检查 | 期望 |
|---|---|---|
| SEQ-313.1 | 全部 Output 的填充与保留字节 | 全为 0 |
| SEQ-313.2 | 日志中一条 `CmdResult` 的结果码被改成未登记值后 `VerifyDriver` 读取 | 读取报错(`TryFromBytes` 失败),验证失败并报该记录 seq |

### SEQ-314 out_hash 按 Output 字节流式累积,在每个快照处比较

- 规则:[状态哈希](README.md#状态哈希)
- 级别:table
- 里程碑:M2
- 前置:B0;一段 journal 含快照 S1、S2,两者之间 1 000 条 Output
- 实现:`crates/hft-sequencer/tests/hash.rs::seq_314_out_hash_and_verify`

| 行 | 动作 | 期望 |
|---|---|---|
| SEQ-314.1 | 计算 S1 与 S2 之间全部 Output(测量字段按偏移置零)的 xxh3-128 | 等于 S2 的 `out_hash` |
| SEQ-314.2 | `VerifyDriver` 重放 | 在 S1、S2 处比较 `out_hash`;S2 是非关键帧时另比较 `state_hash` |
| SEQ-314.3 | 改动 S1 与 S2 之间一条 Output | S2 处 `out_hash` 不一致;报告首个不一致记录的 seq |
| SEQ-314.4 | `RecoverDriver` 遇到同样的不一致 | 不失败,按"恢复时决策不一致"处理 |
| SEQ-314.5 | `MirrorDriver` 遇到同样的不一致 | 不比较,继续 |
| SEQ-314.6 | 连续 Keyframe、OnDemand;从段首与各中间快照分别起播 | 区间重算相等,逐条 Output 无差异;边界快照不进入相邻区间 |
| SEQ-314.7 | Live L2 簿后滚段,从新段快照起播至下一快照 | ConfigBlob 计入滚段快照前区间,L2 关键帧计入后区间,恢复后的累计量一致 |
| SEQ-314.8 | 无挂单 Drain 在命令同一事件内写 CmdResult、模式注记与快照 | 快照 out_hash 包含本事件中先写出的 Output;从前一快照验证通过 |

实现使用 1 000 条过期命令的 CmdResult 形成固定长度区间,另以独立 xxh3 累计器从日志重算全部快照头。反例修改首条 CmdResult 与按篡改流重算的区间末 out_hash,验证驱动必须同时报告首条差异与末快照差异;恢复驱动记 RECOVER_MISMATCH,镜像继续。

### SEQ-315 运行期零分配

- 规则:[延迟与资源契约](README.md#延迟与资源契约)
- 级别:unit
- 里程碑:M3
- 前置:B0 稳态;计数分配器;预热 1 000 条 Input 之后开始计数
- 步骤:
  1. 处理 100 000 条混合 Input(行情、回报、命令含 `SetParams`、关键帧、段滚动、Drain)
- 期望:
  - sequencer 线程上的分配与释放次数均为 0;被替换的参数对象经 `retire` 交出,不在本线程 drop
- 回报路径回归:`tests/sdk_alloc.rs::seq_315_order_reports_reuse_hot_buffers` 覆盖 Ack、部分/全部成交、重复回报与成交后 Reject;扣除内存 journal 测试接收器的逐记录分配后,引擎分配为零。全量混合 Input 验收仍归 M3。
- 实现:`crates/hft-sequencer/tests/sdk_alloc.rs::seq_315_order_reports_reuse_hot_buffers`

### SEQ-316 sequencer 线程在稳态下不做系统调用

- 规则:[延迟与资源契约](README.md#延迟与资源契约)
- 级别:host
- 里程碑:M3
- 环境:dev2,`hft-core@prod` 联调窗口,行情订阅 3 所
- 授权:无
- 步骤:
  1. 对 sequencer 线程 `perf trace -t <tid> -s` 计 60 s(无关键帧与段滚动的时段)
- 通过标准:系统调用计数为 0;sequencer 不向任何 Park 线程发唤醒(`futex` 唤醒计数为 0)
- 证据:`perf trace` 摘要输出,文件名 `SEQ-316-<日期>.txt`,随 M3 联调记录归档到 ops-log
- 实现:`tests/host/SEQ-316.sh`

### SEQ-317 每条 Input 的固定开销

- 规则:[延迟与资源契约](README.md#延迟与资源契约)
- 级别:bench
- 里程碑:M2
- 基准名:`seq/overhead_per_event`
- 实现:`crates/hft-sequencer/benches/seq.rs::seq_317_overhead_per_event`

### SEQ-318 环头检查

- 规则:[延迟与资源契约](README.md#延迟与资源契约)
- 级别:bench
- 里程碑:M2
- 基准名:`seq/head_check`
- 实现:`crates/hft-sequencer/benches/seq.rs::seq_318_head_check`

### SEQ-319 md 突发下的 BBO 等待逐样本断言

- 规则:[延迟与资源契约](README.md#延迟与资源契约)、[主循环](README.md#主循环step)
- 级别:bench
- 里程碑:M3
- 基准名:`seq/l2burst`
- 实现:`crates/hft-sequencer/benches/l2burst.rs::seq_319_l2burst`

### SEQ-320 ConnState{Alive}

- 规则:[延迟与资源契约](README.md#延迟与资源契约)
- 级别:bench
- 里程碑:M2
- 基准名:`seq/conn_alive`
- 实现:`crates/hft-sequencer/benches/seq.rs::seq_320_conn_alive`

### SEQ-321 BBO 连接退役

- 规则:[延迟与资源契约](README.md#延迟与资源契约)
- 级别:bench
- 里程碑:M2
- 基准名:`seq/conn_down`
- 实现:`crates/hft-sequencer/benches/seq.rs::seq_321_conn_down`

### SEQ-322 Views::bbo 按值投影

- 规则:[延迟与资源契约](README.md#延迟与资源契约)
- 级别:bench
- 里程碑:M2
- 基准名:`seq/bbo_view`
- 实现:`crates/hft-sequencer/benches/seq.rs::seq_322_bbo_view`

### SEQ-323 NotifyContinue 投递 64 条

- 规则:[延迟与资源契约](README.md#延迟与资源契约)
- 级别:bench
- 里程碑:M2
- 基准名:`seq/notify_continue`
- 实现:`crates/hft-sequencer/benches/seq.rs::seq_323_notify_continue`

### SEQ-324 Bbo 未命中策略

- 规则:[延迟与资源契约](README.md#延迟与资源契约)
- 级别:bench
- 里程碑:M2
- 基准名:`seq/bbo_miss`
- 实现:`crates/hft-sequencer/benches/seq.rs::seq_324_bbo_miss`

### SEQ-325 Bbo 未命中策略并推进一个目标

- 规则:[延迟与资源契约](README.md#延迟与资源契约)
- 级别:bench
- 里程碑:M2
- 基准名:`seq/bbo_target_idle`
- 实现:`crates/hft-sequencer/benches/seq.rs::seq_325_bbo_target_idle`

### SEQ-326 Bbo 命中策略到出环

- 规则:[延迟与资源契约](README.md#延迟与资源契约)
- 级别:bench
- 里程碑:M2
- 基准名:`seq/bbo_fire`
- 实现:`crates/hft-sequencer/benches/seq.rs::seq_326_bbo_fire`

### SEQ-327 Bbo 经执行层切片到出环

- 规则:[延迟与资源契约](README.md#延迟与资源契约)
- 级别:bench
- 里程碑:M2
- 基准名:`seq/bbo_exec_fire`
- 实现:`crates/hft-sequencer/benches/seq.rs::seq_327_bbo_exec_fire`

### SEQ-328 L2Batch 20 档整批应用

- 规则:[延迟与资源契约](README.md#延迟与资源契约)
- 级别:bench
- 里程碑:M2
- 基准名:`seq/l2batch_20`
- 实现:`crates/hft-sequencer/benches/seq.rs::seq_328_l2batch_20`

### SEQ-329 成交回报到第二腿出环

- 规则:[延迟与资源契约](README.md#延迟与资源契约)
- 级别:bench
- 里程碑:M2
- 基准名:`seq/fill_to_hedge`
- 实现:`crates/hft-sequencer/benches/seq.rs::seq_329_fill_to_hedge`

### SEQ-330 Advice 路由

- 规则:[延迟与资源契约](README.md#延迟与资源契约)
- 级别:bench
- 里程碑:M2
- 基准名:`seq/advice_route`
- 实现:`crates/hft-sequencer/benches/seq.rs::seq_330_advice_route`

### SEQ-331 MarginReport 处理

- 规则:[延迟与资源契约](README.md#延迟与资源契约)
- 级别:bench
- 里程碑:M2
- 基准名:`seq/margin_report`
- 实现:`crates/hft-sequencer/benches/seq.rs::seq_331_margin_report`

### SEQ-332 ExternalFill 处理

- 规则:[延迟与资源契约](README.md#延迟与资源契约)
- 级别:bench
- 里程碑:M2
- 基准名:`seq/external_fill`
- 实现:`crates/hft-sequencer/benches/seq.rs::seq_332_external_fill`

### SEQ-333 对账报告的 Funding 条目

- 规则:[延迟与资源契约](README.md#延迟与资源契约)
- 级别:bench
- 里程碑:M2
- 基准名:`seq/recon_funding_item`
- 实现:`crates/hft-sequencer/benches/seq.rs::seq_333_recon_funding_item`

### SEQ-334 对账报告的整组处理

- 规则:[延迟与资源契约](README.md#延迟与资源契约)
- 级别:bench
- 里程碑:M2
- 基准名:`seq/recon_group`
- 实现:`crates/hft-sequencer/benches/seq.rs::seq_334_recon_group`

### SEQ-335 InjectWarmup 4 KiB

- 规则:[延迟与资源契约](README.md#延迟与资源契约)
- 级别:bench
- 里程碑:M2
- 基准名:`seq/inject_warmup`
- 实现:`crates/hft-sequencer/benches/seq.rs::seq_335_inject_warmup`

### SEQ-336 StrategyCmd 路由

- 规则:[延迟与资源契约](README.md#延迟与资源契约)
- 级别:bench
- 里程碑:M2
- 基准名:`seq/strategy_cmd`
- 实现:`crates/hft-sequencer/benches/seq.rs::seq_336_strategy_cmd`

### SEQ-337 关键帧一次写完

- 规则:[延迟与资源契约](README.md#延迟与资源契约)、[快照](README.md#快照)
- 级别:bench
- 里程碑:M2
- 基准名:`seq/snapshot_keyframe`
- 实现:`crates/hft-sequencer/benches/seq.rs::seq_337_snapshot_keyframe`

### SEQ-338 恢复与验证回放吞吐

- 规则:[延迟与资源契约](README.md#延迟与资源契约)
- 级别:bench
- 里程碑:M3
- 基准名:`seq/replay_throughput`
- 实现:`crates/hft-sequencer/benches/seq.rs::seq_338_replay_throughput`

### SEQ-339 advisor 镜像吞吐

- 规则:[延迟与资源契约](README.md#延迟与资源契约)、[advisor](README.md#advisor)
- 级别:bench
- 里程碑:M9
- 基准名:`seq/mirror_throughput`
- 实现:`crates/hft-sequencer/benches/seq.rs::seq_339_mirror_throughput`

### SEQ-340 两个测试策略绑定不同账户互不影响

- 规则:[策略集](README.md#策略集strategyset)
- 级别:integ
- 里程碑:M9
- 前置:B0;s1(probe-taker,账户 1)与 s2(probe-target,账户 2)同时运行;对 s1 注入 `Kill{Account(1)}` 与连续拒单
- 步骤:
  1. 运行 10 000 条 Input
- 期望:
  - s2 的意图结果、目标进度与 Output 序列与 s1 不存在时的同一运行逐条相同;s1 的事件从不投递给 s2
- 备注:"新增第二个测试策略时引擎 crate diff 为零"由 `cargo xtask scaffold-gate` 执行(门槛 ⑧,归 tools 与 hft-testkit)
- 实现:`crates/hft-sequencer/tests/strategies.rs::seq_340_instances_isolated`

### SEQ-341 HARD 下不自动有界平仓,AutoClear 只在裸量降到尘埃线以下且静止时发生

- 规则:[每条 Input 的处理顺序](README.md#每条-input-的处理顺序process)、[目标意图与执行层](README.md#目标意图与执行层)
- 级别:table
- 里程碑:M2
- 前置:B0 稳态;g2 `Broken`;每行在一个 Input 之后读 `on_legs` 的结果
- 实现:`crates/hft-sequencer/tests/process.rs::seq_341_broken_auto_clear_conditions`

| 行 | 状态 | 期望 |
|---|---|---|
| SEQ-341.1 | 自动有界平仓使已成交裸量 ≤ 尘埃线,group 静止 | `AutoClear{Broken}`;`HedgeFailure` 仍在 |
| SEQ-341.2 | `HedgeFix` 使已成交裸量 ≤ 尘埃线,group 静止 | 同上 |
| SEQ-341.3 | 平仓 IOC 已发出未终态 | 不解除 |
| SEQ-341.4 | 平仓 IOC 零成交终结 | 仍 `Broken` |
| SEQ-341.5 | L3 `ForceDeleverage` 作用于 Broken 的 g2 | 产生永续腿先的减仓单并出环 |

### SEQ-342 干净重启时容差内残余不判 Broken

- 规则:[模式与 cap 的驱动](README.md#模式与-cap-的驱动)、[冷启动恢复](README.md#冷启动恢复recoverdriver)
- 级别:dst
- 里程碑:M5
- 前置:B0;g2 带容差内、高于尘埃线的残余;`auto_resume_after_clean_drain = 1`
- 故障:干净 Drain 后重启;重启后 1–5 s 内各连接随机先后 Up
- 种子:PR 100 + 回归集;每夜日期派生
- 断言:闸门放行、`Recovery` 自动解除;全程无 `Broken`、无 `HedgeFailure`、无 `resolve_broken`;补腿与外来成交默认处理按放行后的新窗口进行;`Note{StartupGate}` 与 `ShadowDeferred` 都由 sequencer 写;INV-20
- 备注:跨组件期望见 RC-9、BK-7
- 实现:`crates/hft-testkit/src/scenarios/seq.rs::seq_342_clean_restart_no_broken`

### SEQ-343 一个账户的限速风暴不影响另一个账户

- 规则:[输出扇出](README.md#输出扇出)、[主循环](README.md#主循环step)
- 级别:dst
- 里程碑:M9
- 前置:B0;账户 1、2 都在 B 持续成对开仓
- 故障:账户 1 在 B 持续收到限速拒绝并堆满其出环配额
- 种子:PR 100 + 回归集;每夜日期派生
- 断言:账户 2 的 `leg_gap` p99 与无风暴时相比不变(差 ≤ 一个直方图桶);账户 2 的出环不被账户 1 的待发项挡住
- 备注:跨组件期望见 MD-5
- 实现:`crates/hft-testkit/src/scenarios/seq.rs::seq_343_account_isolation`

### SEQ-344 sequencer 全组件 DST

- 规则:[主循环](README.md#主循环step)、[每条 Input 的处理顺序](README.md#每条-input-的处理顺序process)、[确定性规则](README.md#确定性规则)
- 级别:dst
- 里程碑:M2
- 前置:B0;策略集 + 执行层 + 风控 + OMS + ledger + 共用撮合器
- 故障:进程崩溃后 `RecoverDriver` 重启、md 突发、bbo 突发、回报乱序与重复、连接起落、限速、出环满、存储档位变化
- 种子:PR 200(提交派生)+ 回归集;每夜日期派生;服务器累计量按 testing §8
- 断言:全部 `INV-n` 每步成立;每个快照处 `VerifyDriver` 逐字节一致;零违反
- 实现:`crates/hft-testkit/src/scenarios/seq.rs::seq_344_full_dst`

### SEQ-345 段 5 的两个开关原样交给 hft-risk

- 规则:[环与容量](README.md#环与容量)、[启动闸门](README.md#启动闸门)
- 级别:table
- 里程碑:M5
- 前置:B0;每行只改所列开关;干净 Drain 后重启,全部连接 Up、对账干净
- 实现:`crates/hft-sequencer/tests/gates.rs::seq_345_seq_params_switches`

| 行 | 开关与状态 | 期望 |
|---|---|---|
| SEQ-345.1 | `auto_resume_after_clean_drain = 1` | 闸门放行时 `Recovery` 自动解除(`Note{CapCleared}`) |
| SEQ-345.2 | `auto_resume_after_clean_drain = 0` | 闸门放行后 `Recovery` 仍在,需 `Resume{Recovery}` |
| SEQ-345.3 | `allow_missing_standby = 0`,账户 1 在 B 的热备 Down | `ORDER_CONNS` 不置位,账户 1 不放行 |
| SEQ-345.4 | `allow_missing_standby = 1`,同上 | `ORDER_CONNS` 置位 |

### SEQ-346 replay_per_round 的边界

- 规则:[环与容量](README.md#环与容量)、[主循环](README.md#主循环step)
- 级别:table
- 里程碑:M5
- 前置:B0(`replay_per_round = 1 024`);`VerifyDriver`;journal 共 n 条 Input
- 实现:`crates/hft-sequencer/tests/preempt.rs::seq_346_replay_per_round_boundary`

| 行 | n | 期望 `Busy` 的轮数 |
|---|---|---|
| SEQ-346.1 | 1 023 | 1 |
| SEQ-346.2 | 1 024 | 1 |
| SEQ-346.3 | 1 025 | 2(1 024 + 1) |

### SEQ-347 journal 与配置错误使 boot 失败

- 规则:[装配与运行](README.md#装配与运行由-appshft-coreappsreplayerhft-testkit-调用)
- 级别:table
- 里程碑:M5
- 前置:B0;每行构造所列故障后 `boot(Journal(Latest))`
- 实现:`crates/hft-sequencer/tests/boot.rs::seq_347_boot_errors`

| 行 | 故障 | 期望 |
|---|---|---|
| SEQ-347.1 | 段链哈希断裂 | `Err(BootError::Journal(..))` |
| SEQ-347.2 | 引导块中的 `ConfigBlob{Engine}` 解析失败 | `Err(BootError::Config(..))` |
| SEQ-347.3 | 快照节 10 的 `SectionHdr.len` 越过快照体 | `Err(BootError::SnapshotUndecodable{section: 10})` |

### SEQ-348 CancelAll 与 Annotate 命令

- 规则:[命令路径](README.md#命令路径)
- 级别:table
- 里程碑:M5
- 前置:B0 稳态;账户 1 在 A 有 Open 挂单 2 张、RISK_REDUCING 挂单 1 张;Trader 发出
- 实现:`crates/hft-sequencer/tests/commands.rs::seq_348_cancel_all_annotate`

| 行 | 命令 | 期望 |
|---|---|---|
| SEQ-348.1 | `CancelAll{account 1, venue A}` | `oms.cancel_all(scope(1, A), open_only = false, ..)`;3 张都发撤单(批量时一条 `GwCmd{CancelAll}`);模式、cap 与 `ctl_state_version` 不变;`Applied` |
| SEQ-348.2 | `CancelAll{account *, venue *}` | 全部 (venue, account) 范围 |
| SEQ-348.3 | `Annotate{OpsNote, "x"}` | `Applied`;状态哈希不变;无其他 Output |

### SEQ-349 boot 按引擎登记方的定时器上界校验 max_timers

- 规则:[定时器](README.md#定时器)
- 级别:table
- 里程碑:M2
- 前置:B0,但 OMS `links_per_account = 4`、每张表 `slots = 16`(账户 1、2、9 在 A、B 上共 5 张表);`D` = 该配置下的 `engine_timer_demand`:执行层 3 × 32 + 1 = 97,OMS 3 × 4 + 5 × 16 + 1 = 93,风控 3 × 3(账户 1、2、9 各一个永续 venue)= 9,系统周期定时器数按 boot 登记的条目数计(记为 n_sys),`D = 199 + n_sys`
- 实现:`crates/hft-sequencer/tests/timers.rs::seq_349_timer_capacity_at_boot`

| 行 | `max_timers` | 期望 |
|---|---|---|
| SEQ-349.1 | D − 1 | `Err(BootError::TimerCapacity{demand: D})` |
| SEQ-349.2 | D | `Err(BootError::TimerCapacity{demand: D})`(须严格大于) |
| SEQ-349.3 | D + 1 | `Ok`;策略定时器配额 = 1 |

### SEQ-350 策略定时器超出共享配额时 timer_at 返回 CtxFull,不进堆

- 规则:[定时器](README.md#定时器)
- 级别:unit
- 里程碑:M2
- 前置:B0,`max_timers = D + 2`(D 同 SEQ-349);稳态;每行独立真实引擎。同行情多回调行给 s1 配置两个合法 group 并订阅同一参考 inst
- 步骤:分别通过单个 BBO 回调、s1/s2 竞争、s1 两个 group 回调、真实到期 Timer 通知和直接 Advice 请求超过两格的定时器;写关键帧后再发一次请求,执行完整日志与该快照窗口回放
- 期望:各行前两次 `Ok`,其余在调用时 `Err(CtxFull)`;满计数等于实际拒绝次数,登记与堆中策略条目恰好两个。满配额快照恢复后仍在调用时拒绝,两个回放均无分歧;策略登记之外的物理堆容量可由引擎 owner 填满且无溢出
- 实现:`crates/hft-sequencer/tests/timers.rs::seq_350_strategy_timer_quota`

### SEQ-351 引擎登记方使定时器堆满时 step 返回 Fatal(TimerOverflow)

- 规则:[定时器](README.md#定时器)
- 级别:unit
- 里程碑:M2
- 前置:B0;测试特性 `testing` 把堆的有效容量压到当前条目数(模拟引擎缺陷)
- 步骤:
  1. 交出一条使 OMS 登记 `HedgeRetry` 的 `OrderEvent{Reject}`(第二腿被拒)
- 期望:
  - `step()` 返回 `Fatal(TimerOverflow)`;该 Input 之后无任何出环帧
- 实现:`crates/hft-sequencer/tests/timers.rs::seq_351_timer_overflow_fatal`

### SEQ-352 策略链接结构不合法时只写 INVALID 决策,不经检查链

- 规则:[意图管线](README.md#意图管线)
- 级别:table
- 里程碑:M2
- 前置:B0 稳态;s1 在 `a1` 上 `place` 买 0.01 IOC,带 LegLink(第二腿 `b1` 卖,ratio 1/1),各行改所列字段
- 实现:`crates/hft-sequencer/tests/intents.rs::seq_352_link_valid_precheck`

| 行 | 改动 | 期望 |
|---|---|---|
| SEQ-352.1 | 无(`role = First`、OnFill、`Retry`、`Ioc`) | 照常经模式门与检查链,出环 |
| SEQ-352.2 | `role = Second` | 一条 `OrderDecision{reject = INVALID}`;`Ctx::outcome = Rejected(INVALID)`;`risk.evaluate` 未被调用;令牌、在途、槽都不变;无出环 |
| SEQ-352.3 | `kind = Simultaneous`、`on_fail = Retry` | 同 .2 |
| SEQ-352.4 | 期望模式 Shadow 且 `role = Second` | 同 .2(结构检查先于模式门,不写 `SHADOW`) |

### SEQ-353 OMS 的 P0 级 OmsUnknown 在同一 Input 内设 UnknownTimeout cap

- 规则:[模式与 cap 的驱动](README.md#模式与-cap-的驱动)
- 级别:table
- 里程碑:M2
- 前置:B0 稳态;OMS `unknown_alarm_ms = 30 000`;账户 1 在 B 上槽 s 于 t 进入 `Unknown`
- 实现:`crates/hft-sequencer/tests/mode.rs::seq_353_unknown_timeout_cap`

| 行 | 输入 | 期望 |
|---|---|---|
| SEQ-353.1 | t + 30 001 ms 的 `Timer{Oms, Sweep}` | OMS 输出 P0 `Note{OmsUnknown}`;同一 Input 内 `CapSet{UnknownTimeout, account 1, venue B}` |
| SEQ-353.2 | t + 30 000 ms 的 `Timer{Oms, Sweep}` | 无 P0 `OmsUnknown`;无 `CapSet` |
| SEQ-353.3 | .1 之后再一次 Sweep | 无第二条 `CapSet`(OMS 只升级一次) |
| SEQ-353.4 | .1 之后 `Resume{UnknownTimeout, 1, B}`,s 仍 `Unknown` | `CmdResult{PreconditionFailed, detail = UnknownPresent}` |

### SEQ-354 绕过 gateway 的确认带 expect_state_version = 0 时升级命令被拒,Kill 例外

- 规则:[命令路径](README.md#命令路径)
- 级别:table
- 里程碑:M5
- 前置:B0 稳态;`ctl_state_version = 5`;直接向 ctl 环写 `confirmed = 1`、`expect_state_version = 0` 的命令(主体 `cli:alice`,Admin)
- 实现:`crates/hft-sequencer/tests/commands.rs::seq_354_confirm_without_version`

| 行 | 命令 | 期望 |
|---|---|---|
| SEQ-354.1 | `SetMode{account 1, Live}`(升级) | `ConfirmInvalid{StateChanged}`;状态不变 |
| SEQ-354.2 | `Resume{Kill, Account(1)}`(升级) | `ConfirmInvalid{StateChanged}` |
| SEQ-354.3 | 同 .1,`expect_state_version = 5` | `Applied` |
| SEQ-354.4 | `Kill{Account(1)}` | `Applied`(Kill 不比对版本) |
| SEQ-354.5 | `SetMode{account 1, Shadow}`(降级,无需确认) | `Applied` |

### SEQ-355 SimFault 只对 Sim 账户、非 prod 实例生效并经 out-sim 下发

- 规则:[命令路径](README.md#命令路径)
- 级别:table
- 里程碑:M4
- 前置:B0 稳态;主体 Admin `cli:alice`;实例角色如列
- 实现:`crates/hft-sequencer/tests/commands.rs::seq_355_sim_fault_routing`

| 行 | 实例角色 / 账户 | 期望 |
|---|---|---|
| SEQ-355.1 | `sim` / 9(Sim) | `Applied`;同一 Input 内 `out-sim` 一条 `GwCmd{SimFault}`,账户 9 |
| SEQ-355.2 | `sim` / 1(Live) | `InvalidArgs{NotSim}`;无出环 |
| SEQ-355.3 | `prod` / 9 | `Rejected{Forbidden}`;无出环 |
| SEQ-355.4 | `sim` / 9,PREVIEW | `CmdResult{preview, status = Applied}`;无出环 |

### SEQ-356 从快照起步重建 risk 的 on_legs 缓存,不执行其输出、不写记录

- 规则:[快照](README.md#快照)
- 级别:unit
- 里程碑:M5
- 前置:B0;快照中账户 1 的 g1 静止、`Broken{1, g1}` 在位
- 步骤:
  1. 从该快照起步(验证驱动)
  2. 交出下一条与 g1 无关的 `Bbo`
- 期望:
  - 步骤 1:`risk.on_legs` 对每个 group 各调用一次;无任何 Output 记录;`Broken{1, g1}` 仍在
  - 步骤 2 之后:状态哈希与连续运行在同一 seq 的哈希相同
- 实现:`crates/hft-sequencer/tests/recover.rs::seq_356_on_legs_rebuilt_from_snapshot`

### SEQ-357 SetParams 先调 on_groups_changed 再 apply(GroupsChanged),杠杆超限在同一 Input 内禁开

- 规则:[命令路径](README.md#命令路径)
- 级别:unit
- 里程碑:M5
- 前置:B0 稳态;账户 1 在 B 上实际杠杆 5×,`RiskParams.max_perp_leverage_x100 = 300`(该账户原无永续腿)
- 步骤:
  1. `SetParams` 给 s1 新增引用 `b1` 的 group g3
- 期望:
  - `risk.on_groups_changed` 的调用先于 `apply(GroupsChanged)`;同一 Input 内 g3 的有效模式为 `CloseOnly`(`lev_bad`);`CmdResult = Applied`
- 实现:`crates/hft-sequencer/tests/commands.rs::seq_357_groups_changed_order`

### SEQ-358 命令错误到 CmdResult 的映射

- 规则:[命令路径](README.md#命令路径)
- 级别:table
- 里程碑:M5
- 前置:B0 稳态;主体 Admin
- 实现:`crates/hft-sequencer/tests/commands.rs::seq_358_error_mapping`

| 行 | 触发 | 期望 |
|---|---|---|
| SEQ-358.1 | `ModeErr::UnknownPending` | `PreconditionFailed{UnknownPresent}` |
| SEQ-358.2 | `SetRisk` 使 `RiskParamError::Invalid` | `InvalidArgs{ParamInvalid}`;旧参数生效 |
| SEQ-358.3 | `HedgeFix` 得 `ExecCmdErr::UnknownGroup` | `InvalidArgs{UnknownGroup}` |
| SEQ-358.4 | `ExecCmdErr::Hard` / `NotQuiescent` / `NothingToFix` / `QuoteUnavailable` / `ChainLeg` | `PreconditionFailed{同名 detail}`;无出环 |
| SEQ-358.5 | `SetParams` 新实例 `S::create` 返回 `Err` | `InvalidArgs{ParamInvalid}`;实例未建立 |
| SEQ-358.6 | `ReplaceRefdata{version}` ≤ 当前且 blob 无法载入 | `Rejected{StaleVersion}`(版本先于载入复核) |
| SEQ-358.7 | `Resume{DailyLoss}` 的 PREVIEW,该日已解除 1 次(`daily_loss_max_resumes = 1`) | `CmdResult{preview, status = PreconditionFailed, detail = ResumeLimit}`(不是 `NeedsConfirm`) |

### SEQ-359 OMS 防御路径的外来成交经 external_from_oms_note 构造,与 Note 逐字段一致

- 规则:[每条 Input 的处理顺序(`process`)](README.md#每条-input-的处理顺序process)
- 级别:unit
- 里程碑:M9
- 前置:B0 稳态;账户 1 在 A 上
- 步骤:
  1. 交出一条 `StaleGen` 带成交的 `OrderEvent`(`exch_order_id = 77`、`cum_filled = 0.002`)
- 期望:
  - 同一 Input 内 `Note{OmsForeignFill}` 一条;交给 ledger 的 `ExternalFill` 等于 `external_from_oms_note(该 Note)`(未列字段为 0)
- 实现:`crates/hft-sequencer/tests/external.rs::seq_359_foreign_fill_from_note`

### SEQ-360 OMS 定时器、对冲门控的 retry_at 与 ApprovedIntent 的对冲字段

- 规则:[每条 Input 的处理顺序(`process`)](README.md#每条-input-的处理顺序process)、[意图管线](README.md#意图管线)
- 级别:table
- 里程碑:M2
- 前置:B0 稳态;账户 1 g1 链接 L(第一腿 `a1`,第二腿 `b1`);B 的 `b1` BBO 60 000.0 / 60 000.1
- 实现:`crates/hft-sequencer/tests/intents.rs::seq_360_oms_hooks`

| 行 | 输入 | 期望 |
|---|---|---|
| SEQ-360.1 | `Timer{Oms, UnknownQuery}`(槽 s) | 调 `oms.on_unknown_query`,不调 `on_sweep` |
| SEQ-360.2 | L 的第二腿被检查链以 `RATE_BACKOFF` 拒,(B, 1) `backoff_until = E` | `hedge_gated(L, Risk(RATE_BACKOFF), retry_at = now.tsc_of(E), …)` |
| SEQ-360.3 | 第二腿被 `RATE` 拒 | `retry_at = 0` |
| SEQ-360.4 | s1 发带 LegLink 的第一腿(第二腿卖) | `ApprovedIntent.hedge_ref_px = 60 000.0`(卖取 bid) |
| SEQ-360.5 | OMS 第二腿请求放行 | `ApprovedIntent.hedge_px` = 第 1 步 `price_hedge` 的结果 |

### SEQ-361 合成 FundingSettle 的 rx_tsc 写 0

- 规则:[资金费合成](README.md#资金费合成)
- 级别:unit
- 里程碑:M2
- 前置:B0 稳态;B 的 `funding_settle = BalanceOnlyPush`;账户 1 持 `b1` −0.01;下次结算时刻 T
- 步骤:
  1. 交出 `Timer{Sys, FundingSettle{B}}`(UTC = T)
- 期望:
  - 一条 `FundingSettle{synth = 1}` Output,`rx_tsc = 0`
- 实现:`crates/hft-sequencer/tests/funding.rs::seq_361_synth_rx_tsc_zero`

### SEQ-362 恢复时决策不一致的每个受影响 (venue, account) 恰好一条 RecoverMismatch

- 规则:[恢复时决策不一致](README.md#恢复时决策不一致)
- 级别:table
- 里程碑:M2
- 前置:B0;尾部重放产生的 Output 与日志不一致如列
- 实现:`crates/hft-sequencer/tests/mismatch.rs::seq_362_one_note_per_pair`

| 行 | 不一致 | 期望 |
|---|---|---|
| SEQ-362.1 | (A, 1) 的两条 `OrderDecision` | 一条 `Note{RecoverMismatch}`(P0),`seq` = 第一条不一致记录,venue A、账户 1 |
| SEQ-362.2 | (A, 1) 与 (B, 2) 各一条 | 两条 |
| SEQ-362.3 | 一条 `CmdResult`(不带账户) | 每个有表的 (venue, account) 各一条 |
| SEQ-362.4 | 无 | 无 |

### SEQ-363 从模型检查点续算时 model_tick 与连续运行在同一条 AdvisorTick 上投递

- 规则:[模型检查点](README.md#模型检查点)
- 级别:unit
- 里程碑:M9
- 前置:B0;s2 的 Model `model_tick(every_ms = 60 000)`;journal 中 `AdvisorTick` 的 UTC 依次为 00:30:59.5、00:31:00.5;检查点 `last_seq` 是 00:30:59.5 那条 AdvisorTick 之后的一条记录
- 步骤:
  1. 从该检查点续算
- 期望:
  - 00:31:00.5 的 AdvisorTick 投递一次 tick(前一条取 00:30:59.5,跨过 00:31:00 边界);投递序列与连续运行逐条相同
- 实现:`crates/hft-sequencer/tests/advisor.rs::seq_363_model_tick_resume`

### SEQ-364 BalanceReport 的 quiet 判定以 balance_check_quiet_ms 为界

- 规则:[每条 Input 的处理顺序(`process`)](README.md#每条-input-的处理顺序process)
- 级别:table
- 里程碑:M9
- 前置:B0 稳态;ledger `balance_check_quiet_ms = 5 000`;(A, 1) 最近一笔成交在 engine_ns F,无非终态单
- 实现:`crates/hft-sequencer/tests/ledger_bridge.rs::seq_364_balance_quiet_boundary`

| 行 | `BalanceReport` 的 engine_ns | 期望 `quiet` |
|---|---|---|
| SEQ-364.1 | F + 4 999 ms | false |
| SEQ-364.2 | F + 5 000 ms | false |
| SEQ-364.3 | F + 5 001 ms | true |
| SEQ-364.4 | F + 6 000 ms,另有 `a1` 的 Live GTC 挂单 | false |

### SEQ-365 带 reduce_only 的 StrategyCmd 期间设的目标以 ro = true 交执行层

- 规则:[命令路径](README.md#命令路径)、[目标意图与执行层](README.md#目标意图与执行层)
- 级别:unit
- 里程碑:M5
- 前置:B0 稳态;s2(probe-target)的 g2 当前净量 `a1` +0.01、`b1` −0.01;s2 对 `Manual` 的处理是设 g2 目标 +0.02 / −0.02
- 步骤:
  1. `StrategyCmd{s2, g2, reduce_only = true}`
- 期望:
  - `exec.set_target(.., ro = true, ..)`;`ExecDecision{TargetRejected, detail = 1 | 11 << 8}`;`Ctx::outcome = TargetRejected(ReduceOnlyCmd)`;`trigger_seq` = 该命令的 seq;`CmdResult = Applied`
- 实现:`crates/hft-sequencer/tests/commands.rs::seq_365_reduce_only_target`

### SEQ-366 恢复结束时干净重启条件对整个实例判定一次,CLEAN_DRAIN 给全部账户或都不给

- 规则:[冷启动恢复](README.md#冷启动恢复recoverdriver)
- 级别:table
- 里程碑:M5
- 前置:B0;同一构建;journal 末尾是 Drain 快照
- 实现:`crates/hft-sequencer/tests/recover.rs::seq_366_clean_drain_flag`

| 行 | 快照内状态 | 期望(账户 1、2、9 的 `RecoveryFlags`) |
|---|---|---|
| SEQ-366.1 | 全部账户无 Unknown、无 Broken/HedgeFailure、无未结束链接、`exec.engine_reduce_pending() = false`,快照之后无 Input | 都含 `CLEAN_DRAIN` |
| SEQ-366.2 | 只有账户 2 有一个 Unknown 槽 | 都不含 |
| SEQ-366.3 | 账户 9(Sim)有未到达的 `reason = Hard` 引擎目标(Drain 超时写出的快照) | 都不含 |
| SEQ-366.4 | 同 .1,但快照之后还有一条 Input | 都不含 |

### SEQ-367 advisor 的 Note 经 advice 环送达,hk 环只有 housekeeping 一个生产者

- 规则:[环与容量](README.md#环与容量)、[advisor 与 Model](README.md#advisor-与-model)
- 级别:integ
- 里程碑:M9
- 前置:B0;实盘驱动;`advisor_max_lag_ms = 1 000`;advisor 被阻塞 3 s
- 步骤:
  1. 解除阻塞
- 期望:
  - `Note{AdvisorLag}` 经 advice 环到达并作为 Meta 记录写入;hk 环在整个过程中的写入方只有 housekeeping 线程(testkit 环包装记下的生产者线程 id 唯一)
- 实现:`crates/hft-sequencer/tests/advisor.rs::seq_367_advisor_note_via_advice_ring`

### SEQ-368 交易所限速拒单调 on_rate_limit_hit 并追加其 Note

- 规则:[模式与 cap 的驱动](README.md#模式与-cap-的驱动)
- 级别:unit
- 里程碑:M2
- 前置:B0 稳态
- 步骤:
  1. 交出 (A, 1) 的 `ConnState{Degraded, role Order, reason RATE_LIMIT}`
- 期望:
  - 同一 Input 内调 `risk.on_rate_limit_hit(A, 1, now)` 一次,其返回的 `Note{RateLimitHit}` 写入输出;之后 (A, 1) 的 Open 被 `RATE_BACKOFF` 拒
- 实现:`crates/hft-sequencer/tests/mode.rs::seq_368_rate_limit_hit_note`

### SEQ-369 不含 Summary 的对账组不调 apply(Gate(Recon))

- 规则:[每条 Input 的处理顺序(`process`)](README.md#每条-input-的处理顺序process)
- 级别:unit
- 里程碑:M9
- 前置:B0;账户 1 在 B 上启动闸门 `RECONCILED` 未放行
- 步骤:
  1. 交出只含一条 `UnknownResolved` 条目、无 `Summary` 的 `ReconcileReport` 组
- 期望:
  - `oms.on_reconcile` 应用该条目;`risk.on_reconcile` 返回 `gate = None`;无 `apply(Gate(Recon))`;`RECONCILED(1, B)` 仍未放行
- 实现:`crates/hft-sequencer/tests/recon.rs::seq_369_group_without_summary`

### SEQ-372 同一外来单的两次防御路径报告各按累计量增量入账(OMS 自己的 ForeignFill 不登记 ext_held)

- 规则:[每条 Input 的处理顺序(`process`)](README.md#每条-input-的处理顺序process)
- 级别:unit
- 里程碑:M2
- 前置:B0 稳态;账户 1 在 B 上无 `b1` 持仓
- 步骤:
  1. 交出 `OrderEvent{slot = SLOT_UNRESOLVED, detail = Buy, inst = b1, exch_order_id = 9, cum_filled = 0.002}`
  2. 交出同一单的 `OrderEvent{…, cum_filled = 0.005}`
- 期望:
  - 两步各一条 `Note{OmsForeignFill}`,`v[1]`(入账量)分别为 0.002 与 0.003 的 raw;无 `Note{OmsExtDedup}`;`ledger.pos(1, b1).qty` = 0.005
- 备注:回归——sequencer 曾把 OMS 自己的 `ForeignFill` 经 `oms.external_adjust` 登记进 `ext_held[9]`,第二次报告被再扣 0.002(只入账 0.001)
- 实现:`crates/hft-sequencer/tests/process.rs::seq_372_foreign_fill_increments_not_held`

### SEQ-371 对账 Funding 条目的 Note{FundingCorrected} 写在同一 Input 内

- 规则:[每条 Input 的处理顺序(`process`)](README.md#每条-input-的处理顺序process)
- 级别:unit
- 里程碑:M2
- 前置:B0 稳态;账户 1 在 B 上无资金费入账记录
- 步骤:
  1. 交出只含一条 `Funding` 条目(`inst = b1`,`exch_ts_ns` = 当前 UTC 前 30 min 的结算时刻,`exch_amount = −1.5`,`engine_amount = 0`)的 `ReconcileReport`
- 期望:
  - `ledger.reconcile_funding` 返回 `Corrected`;同一 Input 的输出含 `Note{source = Ledger, code = FundingCorrected}`(`v[5]` = −1.5 的 raw)
- 备注:回归——该注记曾留在 ledger 的注记缓冲里,到下一个冲刷注记的 Input 才写出,冷读者因而晚一个事件入账
- 实现:`crates/hft-sequencer/tests/recon.rs::seq_371_funding_corrected_note_same_input`

### SEQ-373 测试诊断缓冲有界且输出关联缺失显式失败

- 规则:[每条 Input 的处理顺序(`process`)](README.md#每条-input-的处理顺序process)
- 级别:unit
- 里程碑:M2
- 前置:`testkit` feature;新建决策、对冲 place 失败及冷恢复诊断缓冲
- 步骤:各依次写入 512、513 条观察,重置事件观察后关联正确与缺失的 journal 序号
- 期望:三种缓冲的 512 条有效;第 513 条失败且不扩容;事件重置清空决策/place 观察,决策缺失序号置失败标志;冷恢复记录与溢出错误不随事件重置清除,溢出读取返回错误
- 关联:完整边界的 `StateView::ledger`/`refdata` 只读性、缺失视图失败及真实冷热逐项核对由 [TK-115](../hft-testkit/TESTS.md#tk-115-每条快照在其完成边界检查-inv-6) 与 [LED-267](../hft-ledger/TESTS.md#led-267-dst-中每个快照点-journalledger-等于引擎-ledger) 验证
- 实现:`crates/hft-sequencer/src/audit.rs::seq_373_diagnostic_capacity_is_explicit`

### SEQ-374 人工保持阻止配对目标的尾差下单并逐字节回放

- 规则:[每条 Input 的处理顺序(`process`)](README.md#每条-input-的处理顺序process)
- 级别:replay
- 里程碑:M2
- 前置:真实引擎、已有两腿仓位和策略 PairOnFill 目标,保存快照
- 输入:快照后的 Manual、Adl 外来成交和报价 Input,以及在线执行产生的完整 Output 流
- 步骤:依次投递 Manual 与 Adl 外来成交,每次后推进四次报价;从快照以 VerifyDriver 重放完整 Input 尾段
- 期望:manual_hold 保持且没有成功 New 决策,包含尾差修复;回放 Output 逐字节一致
- 实现:`crates/hft-sequencer/tests/sdk_contract/target.rs::seq_374_manual_hold_pair_tail_verify`

### SEQ-375 迟到成交解除恢复保持时单次对冲并验证回放

- 规则:[每条 Input 的处理顺序(`process`)](README.md#每条-input-的处理顺序process)
- 级别:replay
- 里程碑:M2
- 前置:真实引擎中 OnFill 第一腿在途,快照后非干净重启并恢复连接
- 输入:恢复后的快照、旧 epoch 第一腿全成 Input 及其在线 Output 流
- 步骤:保存恢复后的快照,投递旧 epoch 第一腿全成;从快照验证回放
- 期望:同一成交只有一张成功 Second 且数量等于净成交量;无重复 SecondRetry;Output 逐字节一致
- 实现:`crates/hft-sequencer/tests/sdk_contract/target.rs::seq_375_late_fill_resume_verify`

### SEQ-376 完整 ECF1 决策配置只凭 blob 往返

- 规则:[装配与运行](README.md#装配与运行由-appshft-coreappsreplayerhft-testkit-调用)
- 级别:unit
- 里程碑:M2
- 前置:B0,完整 key 槽位事实;反转 gw 次序,修改 OMS、账本、执行层与装配标量
- 步骤:encode_blob 后,仅传 blob/refdata/原 journal 时钟与 epoch 给 decode_blob
- 期望:全部决策配置字段相同,gw 顺序保持;同一版本与 created_utc_ns 重编码逐字节相同
- 实现:`crates/hft-sequencer/tests/config_blob.rs::seq_376_complete_config_roundtrip`

### SEQ-377 缺失配置不得退回默认值

- 规则:[装配与运行](README.md#装配与运行由-appshft-coreappsreplayerhft-testkit-调用)
- 级别:unit
- 里程碑:M2
- 前置:SEQ-376 的完整配置
- 步骤:分别删除段 1/2/3/5/7/8;删 key 槽位事实、重复 gw、非法 key 位、漏账户或未知 feed venue
- 期望:decode_blob 全部返回 ConfigErr,没有隐式补配置
- 实现:`crates/hft-sequencer/tests/config_blob.rs::seq_377_missing_config_does_not_use_defaults`

### SEQ-378 完整配置 journal 可独立装配验证驱动

- 规则:[装配与运行](README.md#装配与运行由-appshft-coreappsreplayerhft-testkit-调用)、[确定性规则](README.md#确定性规则)
- 级别:replay
- 里程碑:M2
- 输入:B0 空启动写完整 ECF1/RKP1 的引导块,放行后现货买入与永续卖出各一笔、12 次 BBO 与两个关键帧
- 期望:从 journal 取 refdata/engine blob 重新解码装配,VerifyDriver 从段首快照重放全部尾部,无输出/快照分歧;风险解码不使用进程内参数标记
- 实现:`crates/hft-sequencer/tests/config_blob.rs::seq_378_replay_config_only_from_journal`

### SEQ-379 快照配置只能引用此前完整记录

- 规则:[装配与运行](README.md#装配与运行由-appshft-coreappsreplayerhft-testkit-调用)
- 级别:unit
- 里程碑:M2
- 前置:SEQ-378 的完整引导块
- 步骤:分别删除 refdata/engine/risk/strategy blob、移至快照之后、损坏字节或截短;另传不存在的快照 seq 或冲突的段头频率
- 期望:from_snapshot 全部失败,不使用未来记录或外部配置补缺
- 实现:`crates/hft-sequencer/tests/config_blob.rs::seq_379_snapshot_config_requires_preceding_verified_blob`

### SEQ-380 完整配置仅改变配置引用与总状态摘要

- 规则:[装配与运行](README.md#装配与运行由-appshft-coreappsreplayerhft-testkit-调用)、[状态哈希](README.md#状态哈希)
- 级别:unit
- 里程碑:M2
- 前置:B0 相同参数分别使用测试配置标记与完整 ECF1/RKP1,每次运行重置策略脚本
- 步骤:两边放行后现货买入、永续卖出各一笔,再处理相同的 12 次 BBO 和关键帧
- 期望:实际产生订单;所有非配置/快照记录逐字节相同;每个快照仅节 9 不同,其他节的版本、行数和内容全相同;总状态摘要必须随配置引用改变
- 实现:`crates/hft-sequencer/tests/config_blob.rs::seq_380_complete_config_preserves_business_outputs`

### SEQ-381 journal 装配遵循启动与恢复时钟顺序

- 规则:[装配与运行](README.md#装配与运行由-appshft-coreappsreplayerhft-testkit-调用)、[冷启动恢复](README.md#冷启动恢复recoverdriver)
- 级别:replay
- 里程碑:M2
- 输入:SEQ-378 的完整 journal;首个引导快照的节 25 全零,后续关键帧已有锚点;以两倍 TSC 频率重启
- 步骤:从各快照独立读取配置;删除引导块 ClockStatus 作反例;重启后读取 Recovery 快照并验证其后的 EngineClock 和输出
- 期望:首次锚点尚未建立也能装配;缺 ClockStatus 失败;Recovery 节 25 保留旧频率,装配使用新段频率;新运行输出逐字节一致
- 实现:`crates/hft-sequencer/tests/config_blob.rs::seq_381_config_clock_follows_boot_and_recovery_order`

### SEQ-382 账本注记在原始成交事件内写出且跨快照不丢失

- 规则:[每条 Input 的处理顺序](README.md#每条-input-的处理顺序process)
- 级别:replay
- 里程碑:M2
- 前置:B0 稳态,账户 1 的 g1 现货腿买入 0.001;平仓前写关键帧
- 输入:两行分别卖出 0.001、随成交扣 0.01 USDT,或卖出 0.0009 后对买单补推 0.00001 BTC FeeOnly(剩余名义低于轮次尘埃阈值)
- 步骤:检查轮次结束的事件边界;写关键帧后再投递对账汇总;从结束前、结束后快照分别验证回放;副本依次删除、重复、篡改 RoundClosed
- 期望:各行恰好一条 RoundClosed,开量 0.001、平量分别 0.001/0.0009、净盈亏分别 −0.011/−0.500905 USDT,与原始 OrderEvent 同一 EVENT_END;轮次累计清零,ledger 无待写注记;后续对账不重复输出;两处快照起步均逐字节一致,三种注记反例均失败
- 实现:`crates/hft-sequencer/tests/process.rs::seq_382_ledger_notes_stay_in_originating_event_across_snapshots`

### SEQ-383 行情估值触发的轮次注记在同一事件写出

- 规则:[每条 Input 的处理顺序](README.md#每条-input-的处理顺序process)
- 级别:replay
- 里程碑:M2
- 前置:B0 稳态,账本 round_dust_notional = 50 USDT,g1 对应腿只买入 0.001 BTC;L2 行为现货腿启用 5 档;变价前写关键帧
- 输入:三行分别为现货 BBO、永续 MarkPrice、现货 TopN L2Snapshot,估值从不低于 50000 降至 49999
- 步骤:检查原始行情事件的注记;写关键帧并投递对账汇总;从变价前后快照分别验证回放
- 期望:没有 OrderDecision,持仓仍为 0.001;各行恰好一条 RoundClosed,开量 0.001、平量 0,与原始行情同一 EVENT_END;ledger 无待写注记且轮次 PnL 清零;后续对账不重复写出;两处快照起步均逐字节一致
- 实现:`crates/hft-sequencer/tests/process.rs::seq_383_price_round_notes_are_journaled_before_snapshots`

### SEQ-384 回放比较保留逻辑体长度和业务尾零

- 规则:[装配与运行](README.md#装配与运行由-appshft-coreappsreplayerhft-testkit-调用)
- 级别:table
- 里程碑:M2
- 前置:同 schema 的独立 ReplaySink
- 输入:尾字节为零的 OrderDecision 与 StateSnapshot;独立 ReplaySink
- 步骤:分别在日志侧和重算侧删 1 B、追加 1/8 B 零;改 SHADOW 位;只改测量字段或记录头 TSC
- 期望:长度变化和 SHADOW 变化失败;测量字段、TSC 与传输标志变化通过;不能把业务尾零当填充删除
- 实现:`crates/hft-sequencer/tests/replay_bytes.rs::seq_384_replay_body_length_is_significant`

| 行 | 改动 | 期望 |
|---|---|---|
| SEQ-384.1 | 原始体不变 | 一致 |
| SEQ-384.2 | 日志侧删除 1 B 零 | 分歧 |
| SEQ-384.3 | 重算侧删除 1 B 零 | 分歧 |
| SEQ-384.4 | 日志侧或重算侧追加 1/8 B 零 | 全部分歧 |
| SEQ-384.5 | SHADOW 位变化 | 分歧 |
| SEQ-384.6 | 测量字段、记录头 TSC 或传输标志变化 | 一致 |
| SEQ-384.7 | Multi 快照体追加 1 B 零 | 分歧 |

### SEQ-385 真实磁盘 journal 独立装配并逐快照起播验证

- 规则:[装配与运行](README.md#装配与运行由-appshft-coreappsreplayerhft-testkit-调用)
- 级别:replay
- 里程碑:M2
- 前置:Linux、live/replay-io features;B0 每张 OMS 表 4096 槽(世代计数节保证快照跨片),max_timers = 32768 满足对应启动预留,以真实 JournalWriter::open_offline 写普通临时目录
- 输入:启动放行、按需快照、BBO、关键帧、滚段与再写关键帧
- 步骤:关闭写者,JournalReader 经 Log::from_reader 读取;每个 StateSnapshot 先独立恢复并重新序列化自检,再分别只从该日志装配 EngineSetup 与 VerifyDriver
- 期望:至少 4 个快照和 2 段;实际包含大于单片的快照与物理 seq 间隔,逐条末片 seq 等于下一逻辑记录的首 seq 减一,末条单片和越界查询也核对,有 CmdResult;所有起点均处理完全部 Input、Output 无分歧且无残留
- 实现:`crates/hft-sequencer/tests/replay_file.rs::seq_385_file_journal_independent_verify`

### SEQ-386 文件读取错误不能返回成功的部分日志

- 规则:[装配与运行](README.md#装配与运行由-appshft-coreappsreplayerhft-testkit-调用)
- 级别:table
- 里程碑:M2
- 前置:SEQ-385 的实际磁盘输入
- 步骤:分别限制为 1 条记录、1 B 逻辑体、32 B 重组上限;篡改首段 magic 后读取;恢复原文件再读取
- 期望:前两行 Limit,重组行 ReassemblyCap,损坏行失败;完好输入成功,读取前后源文件字节不变
- 实现:`crates/hft-sequencer/tests/replay_file.rs::seq_386_file_read_failures_do_not_return_partial_log`

| 行 | 条件 | 期望 |
|---|---|---|
| SEQ-386.1 | 记录上限 1 | Limit |
| SEQ-386.2 | 逻辑体上限 1 B | Limit |
| SEQ-386.3 | 重组上限 32 B | ReassemblyCap |
| SEQ-386.4 | 首段 magic 损坏 | 失败 |
| SEQ-386.5 | 恢复来源文件 | 读取成功,字节未被读取器修改 |

### SEQ-387 指定起点快照自检拒绝损坏和恢复丢失

- 规则:[装配与运行](README.md#装配与运行由-appshft-coreappsreplayerhft-testkit-调用)
- 级别:table
- 里程碑:M2
- 前置:SEQ-385 的实际文件 journal,同时包含头部 state_hash 为零的关键帧与非零引导快照;各行修改独立的内存副本
- 实现:`crates/hft-sequencer/tests/replay_file.rs::seq_387_snapshot_self_check_rejects_corruption_and_loss`

| 行 | 条件 | 期望 |
|---|---|---|
| SEQ-387.1 | 每个快照保持原值,或将声明哈希置零 | 自算非零哈希与恢复后重算值一致 |
| SEQ-387.2 | 每个快照声明哈希翻转 1 bit | DeclaredHash,不回退快照 |
| SEQ-387.3 | 快照尾追加 1 B 零 | Sections |
| SEQ-387.4 | 头部节数置零 | Sections |
| SEQ-387.5 | purpose 置零 | Header |
| SEQ-387.6 | 头部截短 1 B | Header |
| SEQ-387.7 | 重复首节并重算节数和哈希 | Sections |
| SEQ-387.8 | 追加合法但恢复路径不识别的节 65535,重算节数和哈希 | RoundTrip,节号 65535 |
| SEQ-387.9 | 风控节版本置为 65535,重算哈希 | Restore 失败 |
| SEQ-387.10 | 请求不存在的快照 seq | Missing;原日志仍通过自检 |

### SEQ-388 回放配置只在已消费前缀内可见

- 规则:[装配与运行](README.md#装配与运行由-appshft-coreappsreplayerhft-testkit-调用)
- 级别:table
- 里程碑:M2
- 前置:B0 的有效 StrategyParams;每行覆盖 Verify、Recover、Mirror 三种驱动
- 实现:`crates/hft-sequencer/tests/replay_config.rs::seq_388_replay_config_is_visible_only_in_consumed_prefix`

| 行 | 输入或动作 | 期望 |
|---|---|---|
| SEQ-388.1 | 配置记录位于当前游标或未来 | Missing,按 SHA 查询同样不可见 |
| SEQ-388.2 | 游标推进到配置之后 | 成功解析,内容逐字节相同 |
| SEQ-388.3 | 游标退回配置之前 | 再次 Missing |
| SEQ-388.4 | extend 追加配置,尚未消费;随后推进 | 先 Missing,后成功 |
| SEQ-388.5 | kind、encoding、len、version、SHA 各一行只改一项 | 全部 Missing |
| SEQ-388.6 | 最近同引用内容翻转 1 bit 或截短 1 B,之前仍有合法副本 | 两行均失败,不回退 |
| SEQ-388.7 | 相同内容不同 version 的两个引用均在前缀内 | 两者均可解析,不因 SHA 去重丢身份 |
| SEQ-388.8 | end 小于当前游标,配置只在 end 之后 | Missing |

### SEQ-389 验证驱动检出从未来配置满足的真实命令

- 规则:[装配与运行](README.md#装配与运行由-appshft-coreappsreplayerhft-testkit-调用)
- 级别:table
- 里程碑:M2
- 前置:B0 稳态,有效参数版本 2 已放入测试 Mailbox;正常与故意颠倒网关写入顺序的两份真实引擎 journal
- 实现:`crates/hft-sequencer/tests/replay_config.rs::seq_389_verify_rejects_a_command_satisfied_by_future_config`

| 行 | 配置和命令顺序 | 期望 |
|---|---|---|
| SEQ-389.1 | ConfigBlob → SetParams | 实盘夹具 Applied,Verify 无分歧 |
| SEQ-389.2 | SetParams → ConfigBlob | 实盘夹具仍 Applied,Verify 在该命令的 CmdResult seq 检出分歧 |

### SEQ-390 重命名乱序文件与独立后缀通过真实回放

- 规则:[装配与运行](README.md#装配与运行由-appshft-coreappsreplayerhft-testkit-调用)
- 级别:integ
- 里程碑:M2
- 前置:SEQ-385 的实际文件 journal,多段快照与两个完整引导段;读取基准日志后关闭读者
- 步骤:将两段移动为 z-first.data 与 a-second.data,不保留原名称;分别反序传入两段、只传入第二段,经 open_files → Log::from_reader → 每个快照自检 → 独立装配 Verify
- 期望:物理 seq、标志、TSC、逻辑体、段时钟及边界与基准对应部分一致;每个快照恢复后无 Output 分歧或遗漏,回放完成;来源字节保持不变
- 实现:`crates/hft-sequencer/tests/replay_file.rs::seq_390_original_file_order_and_selected_segment_verify`

### SEQ-391 离线 runner 选择快照并按闭区间比较

- 规则:[装配与运行](README.md#装配与运行由-appshft-coreappsreplayerhft-testkit-调用)
- 级别:integ
- 里程碑:M2
- 前置:SEQ-385 的真实文件输入
- 步骤:默认起点与逐快照起点分别调用 runner;篡改第一条 CmdResult,分别取完整窗口、分歧之前、之后和仅该 seq
- 期望:正常输入全部快照通过,实际比较数大于零,普通滚段只计一次运行;完整与单 seq 窗口检出首个分歧并停止;窗口外分歧不计,预热仍完整
- 实现:`crates/hft-sequencer/tests/replay_file.rs::seq_391_offline_runner_snapshot_and_window`

### SEQ-392 多出输出不吞输入,尾部遗漏不可跳过

- 规则:[装配与运行](README.md#装配与运行由-appshft-coreappsreplayerhft-testkit-调用)
- 级别:unit
- 里程碑:M2
- 前置:相邻的 Input 与 Output 记录
- 步骤:Input 前生成额外 Output,随后消费原 Input 和 Output;另只回放至尾部 Output 前并收尾两次;独立运行边界置于 0 和第二条记录前,生成与后段相同的 Output;开首错停止后尝试继续追加
- 期望:额外输出锚定下一 Input 的 seq,Input 保留且仍按原 seq 投递;末尾遗漏只报告一次并保留期望字节;边界外输出不能匹配或消费,分歧锚定前段末 seq 加 1(空前段为 1);停止后不再追加或取输入
- 实现:`crates/hft-sequencer/tests/replay_bytes.rs::seq_392_extra_output_preserves_input_and_missing_tail_fails`

### SEQ-393 零声明哈希仍自检,失败和未支持运行不得误报成功

- 规则:[装配与运行](README.md#装配与运行由-appshft-coreappsreplayerhft-testkit-调用)
- 级别:table
- 里程碑:M2
- 前置:SEQ-385 的真实文件输入
- 步骤:全部快照分别填零或正确哈希;独立损坏起点及后续快照声明;请求首条 Begin 为起点;将后续快照标为 Recovery;keep_going 下篡改多条 CmdResult
- 期望:零和正确声明均通过;起点损坏失败,后续声明错误为该快照分歧;缺起点和引导块外的 Recovery 快照明确失败;keep_going 保留多处诊断且不静默丢条
- 实现:`crates/hft-sequencer/tests/replay_file.rs::seq_393_runner_snapshot_declarations_and_failures`

| 行 | 输入 | 期望 |
|---|---|---|
| SEQ-393.1 | 全部快照声明置零 | 无分歧 |
| SEQ-393.2 | 全部快照填自算哈希 | 无分歧 |
| SEQ-393.3 | 起点声明翻转 1 bit | Snapshot 错误 |
| SEQ-393.4 | 后续快照声明翻转 1 bit | 该快照 seq 的分歧 |
| SEQ-393.5 | 起点为首条 Begin | NoSnapshot |
| SEQ-393.6 | 引导块外的后续快照标 Recovery | RunBoundary |
| SEQ-393.7 | 多个 CmdResult 被改,keep_going | 多处诊断,不触发首错停止 |

### SEQ-394 独立运行边界损坏在重放前失败

- 规则:[装配与运行](README.md#装配与运行由-appshft-coreappsreplayerhft-testkit-调用)、[已知限制与后续工作](README.md#已知限制与后续工作)
- 级别:table
- 里程碑:M2
- 前置:SEQ-385 的真实双段文件输入,第二段为普通滚段
- 步骤:各行独立篡改第二段引导记录并调用 runner
- 期望:边界损坏为 RunBoundary;错位的 RecoverMismatch 行 RecoveryRange,均无成功结果
- 实现:`crates/hft-sequencer/tests/replay_file.rs::seq_394_invalid_run_boundaries_fail_before_replay`

| 行 | 修改 | 期望 |
|---|---|---|
| SEQ-394.1 | Begin 替成 Note | RunBoundary |
| SEQ-394.2 | BootDone 替成 Note | RunBoundary |
| SEQ-394.3 | HostProfile 替成 Note | RunBoundary |
| SEQ-394.4 | 引导快照替成 Note | RunBoundary |
| SEQ-394.5 | 重复 HostProfile | RunBoundary |
| SEQ-394.6 | 重复引导快照 | RunBoundary |
| SEQ-394.7 | 普通滚段的引导快照标 Recovery | RunBoundary |
| SEQ-394.8 | build_git 变化,首个 Input 仍为普通定时器 | RunBoundary |
| SEQ-394.9 | ClockStatus 替成 RecoverMismatch 注记 | RecoveryRange |
| SEQ-394.10 | 首次运行 BootDone 后、EngineClock 前插入 CmdResult | RunBoundary,输出不得被起播跳过 |

### SEQ-395 空时钟锚点只在当前进程的引导块有效

- 规则:[装配与运行](README.md#装配与运行由-appshft-coreappsreplayerhft-testkit-调用)
- 级别:replay
- 里程碑:M2
- 输入:完整 journal 已处理 EngineClock 与后续输入,首次 Boot 快照的节 25 全零
- 步骤:以两倍频率、FROM_SNAPSHOT/TAIL_LOST 从首次快照恢复且不重放尾部;从新 Recovery 快照独立装配并逐字节验证其后输出;分别将空锚点快照放入普通 Hourly 引导块、标成 Keyframe、在其前插入 EngineClock 或 BootDone
- 期望:旧运行尾部的 EngineClock 不妨碍新进程引导块保留全零锚点,装配使用新频率且回放一致;四种反例均报 SnapshotUndecodable 的节 25
- 实现:`crates/hft-sequencer/tests/config_blob.rs::seq_395_unstarted_clock_is_local_to_process_boot`

### SEQ-396 恢复注记计入引导快照之后的输出区间

- 规则:[恢复时决策不一致](README.md#恢复时决策不一致)、[状态哈希](README.md#状态哈希)
- 级别:replay
- 里程碑:M2
- 输入:SEQ-268 的真实恢复日志,分别篡改 OrderDecision 与无账户 CmdResult,正式恢复产生一条或多条 RecoverMismatch 注记
- 步骤:恢复启动 TSC 已越过关键帧期限,先处理自动关键帧,再经 hk 环请求两条关键帧;独立按 schema 屏蔽测量字段与传输标志,累计引导快照之后的 Output;另算遗漏恢复注记的反例哈希,从恢复快照严格验证后续运行
- 期望:实际写出一条自动与两条请求关键帧,三个区间的哈希均与日志一致,首区间与遗漏注记的哈希不同;注记数量等于受影响的账户/venue 数,恢复后的逐字节回放无分歧
- 实现:`crates/hft-sequencer/tests/mismatch.rs::seq_396_recovery_notes_belong_to_next_output_interval`

### SEQ-397 清仓拒单的退避状态从两侧快照重放一致

- 规则:[目标意图与执行层](README.md#目标意图与执行层)、[hft-exec 补腿](../hft-exec/README.md#补腿hedgefixc4)
- 级别:replay
- 里程碑:M2
- 输入:真实 sequencer、OMS、ledger、risk、exec;永续 venue 的 10 秒桶只有一个令牌,建仓已消耗;已成交现货 +0.01、永续 −0.01
- 步骤:完整启动配置写入 journal;快照后投递永续 ADL 卖出 0.005,提交非 HARD 的账户 Kill/flatten;退避期间请求关键帧,推进 500 ms 后再请求关键帧;分别只用 ADL 前和首次退避中的快照及其前置配置独立装配,逐字节验证后续 journal
- 期望:至少两条永续清仓决策被 RATE 拒绝;退避中的补腿窗口未启动,没有 Broken;两次验证均无输出或快照分歧
- 实现:`crates/hft-sequencer/tests/sdk_contract/target.rs::seq_397_rejected_force_backoff_replays_from_both_sides`

### SEQ-398 指定快照恢复审计只覆盖完整提交尾部

- 规则:[冷启动恢复](README.md#冷启动恢复recoverdriver)
- 级别:unit
- 里程碑:M2
- 前置:完整配置的真实 journal,含多个快照及后续 BBO;再追加一条未提交、没有 EVENT_END 的 BBO
- 步骤:分别从首次快照和恰在提交尾部的最新快照恢复,不重放尾部;检查新段的 Boot 注记,请求新关键帧并独立验证恢复后日志;另将 FROM_SNAPSHOT 与 replay_tail = true 同时传入
- 期望:每次恰有一条 RecoverFromSnapshot 注记,使用实际快照 seq;首次快照后的范围止于已提交边界,不含半事件,最新快照的空范围为 0/0;恢复后回放无分歧,矛盾配置返回 BootError::Config
- 实现:`crates/hft-sequencer/tests/config_blob.rs::seq_398_snapshot_recovery_audit_uses_committed_tail`

### SEQ-399 Model 检查点文件符合独立字节金样

- 规则:[模型检查点](README.md#模型检查点)、[文件格式](../../apps/hft-core/README.md#内存与-journal-段按主机类唯一归属)
- 级别:unit
- 里程碑:M2
- 前置:84 B 固定金样,按文件偏移与独立逐位 Castagnoli CRC 计算得到;路径版本 7、路径 3 B、样本 9 B
- 步骤:解码所有键与分节;重新编码;更改运行期 len、checksum、last_ens 后再编码;另往返路径版本为 0 的空节
- 期望:与固定金样逐字节相等,解码的体借用原文件;运行期元数据不进入保留字段,长度和 CRC 由字节计算;空节合法
- 实现:`crates/hft-sequencer/tests/checkpoint_file.rs::seq_399_checkpoint_file_matches_independent_golden`

### SEQ-400 Model 检查点损坏与长度上界显式失败

- 规则:[模型检查点](README.md#模型检查点)
- 级别:table
- 里程碑:M2
- 前置:SEQ-399 金样;校验结构字段时重算 CRC,校验 CRC 时只翻位
- 实现:`crates/hft-sequencer/tests/checkpoint_file.rs::seq_400_checkpoint_file_rejects_corruption_and_limits`

| 行 | 输入 | 期望 |
|---|---|---|
| SEQ-400.1 | 每个截断位置,或尾部追加字节 | 失败,不接受部分文件或尾部垃圾 |
| SEQ-400.2 | 上限小于体长,或声明体长为 0、19、21、u64::MAX | SizeLimit 或 Length,不按声明值分配 |
| SEQ-400.3 | magic、文件版本或填充/保留位非法 | 对应格式错误 |
| SEQ-400.4 | 键、CRC、路径或样本单点翻位 | Checksum |
| SEQ-400.5 | path_len 越界/溢出、路径版本 0 却非空、体长 0–7 | Path |
| SEQ-400.6 | 编码超过上限、首块不足 8 B、首块版本与元数据不同 | 失败,不产出文件 |

### SEQ-401 检查点经真实文件恢复后 Model 连续性不变

- 规则:[模型检查点](README.md#模型检查点)
- 级别:integ
- 里程碑:M2
- 前置:B0 的 s2 Model,路径版本 7;每条 BBO 产出带当前 engine_ns 的 Advice
- 步骤:消费 8 条 BBO 后分块生成检查点,编码并写入临时文件;重新读取、解码,从 journal 取得 last_seq 的 engine_ns,装配另一个 advisor;两路各处理相同的后续 20 条 BBO
- 期望:文件首块为版本后长度,样本累计量非零且恢复时不重复消费前缀;后续 20 条 Advice 的完整头和载荷逐字节相等,累计状态及下一份检查点一致;原文件未改变,引擎 journal 验证无分歧
- 实现:`crates/hft-sequencer/tests/advisor.rs::seq_401_checkpoint_file_resume_matches_continuous_model`

### SEQ-402 不同 Model 在各自检查点边界应用参数

- 规则:[模型检查点](README.md#模型检查点)
- 级别:integ
- 里程碑:M2
- 前置:B0 的 s1、s2 都订阅 a1 的 Model BBO;初始参数阈值分别 7、17;advisor batch = 65535
- 步骤:先保存 s1 的检查点,投递一条 BBO、将 s1 参数改为 8、再投递 BBO;随后保存 s2 的检查点,把未改变 Model 状态的末条 AdvisorStats Meta 作为其 last_seq;再投递 BBO 使该 Meta 随完整输入提交、将 s2 参数改为 18、再投递 BBO。两个 Model 从这两份不同边界的检查点共同恢复
- 期望:Meta 提交前不向恢复读者可见,后继 BBO 完整提交后可见;两实例的参数调用分别恰为 7→8、17→18;每条回调仅在本实例 last_seq 之后,看到该事件生效参数;Meta 边界不使追赶卡住或越过下一 Input;最终检查点与不中断运行一致,引擎 journal 严格回放无分歧
- 实现:`crates/hft-sequencer/tests/advisor.rs::seq_402_each_checkpoint_releases_at_its_own_record_boundary`

### SEQ-403 Model 重算同一基准事件只匹配最终产出且一对一消费

- 规则:[装配与运行](README.md#装配与运行由-appshft-coreappsreplayerhft-testkit-调用)、[advisor](README.md#advisor)
- 级别:integ
- 里程碑:M2
- 前置:B0 真实 advisor,完整引擎与风控配置由正式编解码器写入并读取 journal,在目标 BBO 前保存实际 Model 检查点;同一 BBO 回调连续 emit 同键 P1/P2/P3,实际环只发 P3;启用 replay-io
- 步骤:将真实环的 P3 经 LiveDriver 记入 journal 后重算;分别把记录载荷改成 P1、重复投递原始 P3
- 期望:原始比较 1 条、coalesced = 2、无缺起点;P1 虽在原始产出中出现仍被判分歧,重算字节为 P3;第二次记录 P3 无可配对产出,不能重复消费第一条
- 实现:`crates/hft-sequencer/tests/advisor.rs::seq_403_recompute_requires_latest_same_basis_and_one_to_one_matching`

真实文件的恢复缺口、元数据反例及 Model 来源隔离由 [RPL-275–278](../../apps/replayer/TESTS.md#rpl-275-恢复缺口的逐运行验证与未知尾部报告) 覆盖。

本地文件装配、检查点身份与窗口、只读保护的跨组件覆盖见 [RPL-266–RPL-269](../../apps/replayer/TESTS.md#rpl-266-真实-advisor-文件重算与检查点连续性)。真实 Drain 后的 advisor 跨运行追赶、Model 样本累计、两种路径丢弃原因与停机不计时由 [RPL-058–RPL-060](../../apps/replayer/TESTS.md#rpl-058-窗口内的-modelpathreset-注记按其-v1v2-计算重建点) 验证,缺起点与注记反例见 [RPL-272](../../apps/replayer/TESTS.md#rpl-272-重启后的来源缺失与路径注记反例)。

### SEQ-404 离线快照组件诊断与独立 HashTrail 一致

- 规则:[装配与运行](README.md#装配与运行由-appshft-coreappsreplayerhft-testkit-调用)
- 级别:table
- 里程碑:M2
- 前置:含 25 个正式节号的冷快照,节 4 有三条 CapRecord;启用 replay-io
- 步骤:逐一改变六个组件的全部 15 个所属节及 sequencer 的节 8/25;再同时改 ledger/risk,双向删除 exec 节
- 期望:逐节归属正确,六个哈希与独立 testkit 读者一致;原始记录不变,逻辑体偏移准确;同时变化按 ledger 优先,缺节有明确 presence 和单侧字节,完整相等时无差异
- 实现:`crates/hft-sequencer/tests/snapshot_diff.rs::seq_404_snapshot_components_match_independent_hashtrail`

| 行 | 输入 | 期望 |
|---|---|---|
| SEQ-404.1 | 两份相同快照 | 无差异 |
| SEQ-404.2 | 分别改变 15 个组件节和 8/25 两节 | 六组件哈希与独立 HashTrail 相同,归属及字节偏移按逐项期望 |
| SEQ-404.3 | 同时改变节 22 与 23 | 首个组件 hft-ledger |
| SEQ-404.4 | 任一侧缺少节 21 | missing_section,仅存在侧有字节 |

### SEQ-405 快照字段与未知布局不混淆

- 规则:[装配与运行](README.md#装配与运行由-appshft-coreappsreplayerhft-testkit-调用)
- 级别:table
- 里程碑:M2
- 前置:SEQ-404 快照,改变第二条 cap 的字段;期望名称与偏移手工按布局给出
- 步骤:分别改 source、level、cflags、填充、scope.account/group/inst、set_seq、params;比较两侧相同未知版本、非法行数;另改变单侧版本、ClockCal、ClockCal 加 EngineClock、仅声明哈希、组件之间的节顺序和未知节
- 期望:九个字段给准确行号/嵌套路径/完整字段字节;未知版本和非法行数只给带原因的字节偏移;节头差异指向 version;ClockCal 不改变状态哈希且无组件,同时存在的 EngineClock 差异优先;声明哈希仅指头部;节间顺序归 sequencer,未知节不猜组件
- 实现:`crates/hft-sequencer/tests/snapshot_diff.rs::seq_405_snapshot_fields_versions_and_metadata_are_distinguished`

| 行 | 输入 | 期望 |
|---|---|---|
| SEQ-405.1 | 第二行九个 cap 字段分别变化 | 嵌套名称、宽度、两侧偏移与原始字节符合布局 |
| SEQ-405.2 | 同为未知行版本或非法行数 | 无行号,字节偏移附 unknown_row_version/invalid_row_layout |
| SEQ-405.3 | 仅单侧节版本变化 | sections[4].version |
| SEQ-405.4 | 仅 ClockCal / ClockCal 加 EngineClock | 前者哈希相同且无组件,后者归 hft-sequencer 节 25 |
| SEQ-405.5 | 仅声明哈希变化 | 头部 state_hash,无组件 |
| SEQ-405.6 | 组件间节顺序变化 / 未知节体变化 | 前者归 sequencer 的 section 字段,后者不猜组件 |

### SEQ-406 损坏快照的诊断边界明确失败

- 规则:[装配与运行](README.md#装配与运行由-appshft-coreappsreplayerhft-testkit-调用)
- 级别:table
- 里程碑:M2
- 前置:SEQ-404 的完整快照,损坏分别出现在 logged 与 regenerated 侧
- 步骤:截短快照头;把 n_sections 改成 u32 最大值;节长不对齐/超过文件;非零保留字段;重复节号;末节截短
- 期望:全部返回明确的 side、逻辑体偏移与 Header/SectionCount/SectionFraming/DuplicateSection,不分配声明数量的数组、不输出误导性的组件或字段
- 实现:`crates/hft-sequencer/tests/snapshot_diff.rs::seq_406_snapshot_diagnostics_reject_malformed_framing`

| 行 | 两侧分别注入的损坏 | 错误 |
|---|---|---|
| SEQ-406.1 | 头部只有 95 字节 | Header,偏移 0 |
| SEQ-406.2 | n_sections 为 u32 最大值 | SectionCount,偏移 4 |
| SEQ-406.3 | 节长为 1 或 u32 最大值 / 节保留位为 1 | SectionFraming,节 4 起点 |
| SEQ-406.4 | 节 4 改成重复的节 1 | DuplicateSection,原节 4 起点 |
| SEQ-406.5 | 末节截短一字节 | SectionFraming,节 21 起点 |

### SEQ-407 同 tag 的策略定时器按登记身份取消并按调用顺序释放配额

- 规则:[定时器](README.md#定时器)、[Ctx 应用](README.md#每条-input-的处理顺序process)
- 级别:unit
- 里程碑:M2
- 前置:B0,策略配额两格;s1 对同 group、tag 登记 +10 ms 与 +20 ms 两个定时器,保存关键帧
- 步骤:取消第二个 TimerId,重复取消该 ID 并取消未知 ID;登记一个 +30 ms 定时器后再尝试登记;推进至原来两个截止,生成末尾快照;分别从完整日志和关键帧窗口验证
- 期望:只第一次取消释放一格,新登记成功且再登记返回 CtxFull;原第一个定时器仍按时投递,被取消的截止不投递;登记表与堆的条数一致,两个回放无分歧
- 实现:`crates/hft-sequencer/tests/timers.rs::seq_407_cancel_one_timer_registration_and_reuse_quota`

### SEQ-415 外来累计结算的冷读与完整和窗口回放

- 规则:[每条 Input 的处理顺序](README.md#每条-input-的处理顺序process)
- 级别:replay
- 里程碑:M2
- 前置:B0 的永续开启 PERP_CASH
- 输入:两个身份路径,累计量 100000/200000/300000,累计现金 0/−1170000/−1180000
- 步骤:SLOT_UNRESOLVED 与 StaleGen 各交付三个完整累计回报,重复每条,首笔后关键帧;从首个与中间快照冷读,执行完整和窗口 verify
- 期望:每次正量仅一条 OmsForeignFill,钱包按累计原始现金改变,冷热钱包和仓位逐单位相等;重复不改变;所有真实回放通过,缺注记或错误数量的反例失败
- 实现:`crates/hft-sequencer/tests/settlement.rs::seq_415_foreign_cash_survives_checkpoints_cold_consumption_and_replay`

### SEQ-416 原始凭据补账及确认的完整回放

- 规则:[实现规格](README.md#每条-input-的处理顺序process)
- 级别:integ
- 里程碑:M2
- 前置:B0 严格永续结算,多空 × 部分减仓/全平四行
- 步骤:按原始现金合计 −1170003、费用合计 397 补账;部分减仓一笔,全平分为不同现金和费用的两笔;保存关键帧,逆序交付确认及每笔重复副本,再重试旧批次;执行完整与窗口回放和独立冷账本重建
- 期望:钱包只变动 −1170400,确认只清待确认量;补账前与待确认快照窗口、完整输入回放及独立冷账本一致
- 实现:`crates/hft-sequencer/tests/settlement.rs::seq_416_reconciliation_receipts_rebuild_wallet_and_pending_confirmation`

### SEQ-417 原始凭据和输出证据损坏必须暴露

- 规则:[实现规格](README.md#每条-input-的处理顺序process)
- 级别:integ
- 里程碑:M2
- 前置:SEQ-416 产生的完整 journal
- 步骤:分别隐藏或修改 ReconcileCash、派生成交注记,或只改原始费用而保留输出;执行验证回放和独立冷账本重建
- 期望:五行验证回放均失败;坏注记冷端报错,原始金额变化由冷账本独立算出不同余额
- 实现:`crates/hft-sequencer/tests/settlement.rs::seq_417_reconciliation_cash_requires_all_raw_facts_and_output_evidence`

### SEQ-418 整组预检拒绝坏凭据,不留下部分补账

- 规则:[每条 Input 的处理顺序](README.md#每条-input-的处理顺序process)
- 级别:integ
- 里程碑:M2
- 前置:B0 严格永续结算,空头仓位数量 300000,同一组两笔买入补账
- 步骤:第二笔分别缺现金覆盖、缺手续费资产、重复本组身份、数量超出差额、方向相反、使用已入账身份;处理后写关键帧
- 期望:六行均返回 Idle(不停机);同一事件恰好一条 P0 `ReconcileRejected{reason = 3}` 与一条 `ReconcileCash{applied = 0}`;第一笔也不入账,钱包、仓位、去重身份和 auto_held 保持原值,不输出财务成交注记;冷账本从处理前关键帧起步得到同一钱包,完整与从关键帧起步的验证回放无差异
- 实现:`crates/hft-sequencer/tests/settlement.rs::seq_418_bad_receipt_batch_cannot_partially_book`

### SEQ-419 单笔重复证明贯通真实引擎与快照窗口回放

- 规则:[每条 Input 的处理顺序](README.md#每条-input-的处理顺序process)
- 级别:integ
- 里程碑:M2
- 前置:自足配置的真实引擎、严格结算、外来去重容量 2
- 步骤:防御/StaleGen 两路径与正/零 ID 四组;原件入账后三笔其他成交淘汰普通键;两次重复原件和累计回报,从入账前、入账后、淘汰后三个快照冷读与回放,并完整回放
- 期望:钱包始终等于原始现金与费用之和;重复无订单或入账注记,对应 OMS 路径告警恰一条且 v[2] 为零;冷热节 1/13/22 逐字节相等,全部回放无差异
- 实现:`crates/hft-sequencer/tests/settlement.rs::seq_419_original_duplicate_proof_survives_fifo_cold_restore_and_replay`

### SEQ-420 未证明的重复不改变钱包或发出对冲

- 规则:[每条 Input 的处理顺序](README.md#每条-input-的处理顺序process)
- 级别:integ
- 里程碑:M2
- 前置:真实引擎已入账一笔严格结算 Unknown 原件
- 步骤:八行分别改变成交 ID、数量、现金、费用、方向、覆盖,或先交付第二个不同成交键,以及同成交键不同订单号的未入账副本
- 期望:对应 OmsCash 错误 Fatal,钱包和仓位不变,无新订单决策
- 实现:`crates/hft-sequencer/tests/settlement.rs::seq_420_unproven_receipt_overlap_stops_without_wallet_or_hedge_change`

### SEQ-421 恢复后旧 Drain 执行状态不进入新运行

- 规则:[装配与运行](README.md#装配与运行由-appshft-coreappsreplayerhft-testkit-调用)、[Drain](README.md#drain)
- 级别:table
- 里程碑:M2
- 前置:自足配置真实引擎,关键帧之后开始 Drain;两种 cancel_resting 各覆盖空仓立即完成与 IOC 尚未应答两行
- 步骤:从该关键帧恢复并重放旧 Drain;新运行再发相反 cancel_resting 的 Drain;独立执行从初始启动和关键帧开始的全部运行回放
- 期望:旧尾部实际重放且无分歧;恢复结束的 DrainState 全字段为初始值,Draining cap 消失而 Recovery 保留;新运行首轮不自行结束,新命令按本次参数与 engine_ns 重新开始;两种窗口均验证两次运行,无普通或恢复分歧
- 实现:`crates/hft-sequencer/tests/recover.rs::seq_421_recovery_clears_old_drain_progress_before_the_new_run`

| 行 | 旧 Drain 的 cancel_resting | 重启前状态 | 新 Drain 的 cancel_resting |
|---|---|---|---|
| SEQ-421.1 | false | 无在途,已完成 | true |
| SEQ-421.2 | true | 无在途,已完成 | false |
| SEQ-421.3 | false | IOC 未应答,未完成 | true |
| SEQ-421.4 | true | IOC 未应答,未完成 | false |

### SEQ-422 内存物理序号逐条对照真实文件写者

- 规则:[装配与运行](README.md#装配与运行由-appshft-coreappsreplayerhft-testkit-调用)
- 级别:unit
- 里程碑:M2
- 前置:SEQ-385 的真实引擎、4096 槽表、多片快照及双段 journal;每次写入同时交给真实文件写者和物理编号 MemJournal
- 步骤:引擎完成输入、关键帧和滚段后封闭文件;真实读者重组,与保留的内存原件对照并逐快照回放
- 期望:每次 append 和引导块结束返回的 seq 当场相同;真实读者重组后,逻辑体、首末片 seq、TSC、业务标志及事件边界逐条相同,仅排除传输 CRC/PART;每段 BootDone 计数等于物理记录数且大于逻辑记录数;两个来源的全部快照独立起播均无分歧
- 实现:`crates/hft-sequencer/tests/replay_file.rs::seq_422_memory_physical_sequences_match_real_writer_and_reader`

### SEQ-423 多片记录截尾与已裁发布前缀恢复

- 规则:[装配与运行](README.md#装配与运行由-appshft-coreappsreplayerhft-testkit-调用)
- 级别:table
- 里程碑:M2
- 前置:独立 MemJournal,无故障钩子,逻辑体使用 Keyframe 快照标记
- 步骤:默认逻辑与显式物理模式分别逐行写快照并提交;追加未完成两片记录,分别由 journal 截尾或调用方先裁到发布前缀再截尾
- 期望:序号在 append 返回前分配;完整末尾续号在末片之后,未完成记录的分片索引被清除;保留数、发布数与全部原件相同,Log 不携带已裁尾部索引;默认模式每行只占一个 seq
- 实现:`crates/hft-sequencer/tests/mem_journal.rs::seq_423_memory_multipart_boundaries_and_truncation`

| 行 | 逻辑体长度 | 物理片数 |
|---|---|---|
| SEQ-423.1 | SnapshotHdr 长度 | 1 |
| SEQ-423.2 | PART_MAX_CHUNK | 1 |
| SEQ-423.3 | PART_MAX_CHUNK + 1 | 2 |
| SEQ-423.4 | 2 × PART_MAX_CHUNK | 2 |
| SEQ-423.5 | 2 × PART_MAX_CHUNK + 1 | 3 |

### SEQ-424 物理模式 Meta 的独立、引导与事件边界

- 规则:[装配与运行](README.md#装配与运行由-appshft-coreappsreplayerhft-testkit-调用)
- 级别:unit
- 里程碑:M2
- 前置:物理编号 MemJournal,连续两段
- 步骤:每段先写引导 Meta,随后写独立 ClockStatus,再写 Timer 及其同事件尾部 ClockStatus
- 期望:Begin 与引导 Meta 不提前标 EVENT_END,BootDone 标记;独立 Meta 自带标记且无需 end_event 即能发布;开放事件中的 Meta 不带标记,完成点落在 Timer
- 实现:`crates/hft-sequencer/tests/mem_journal.rs::seq_424_physical_meta_boundaries_distinguish_boot_and_open_events`

### SEQ-425 终态与基础币费用在同一事件刷新敞口

- 规则:[每条 Input 的处理顺序](README.md#每条-input-的处理顺序process)
- 级别:table
- 里程碑:M2
- 前置:真实 sequencer,敞口闸门最小名义 100 USDT;仅此夹具将执行层尘埃阈值设为 1000 USDT,隔离自动减仓,不改变风控闸门
- 步骤:分别让 0.010 BTC 订单成交 0.001 后 Expired,以及 0.004 BTC 完整成交后补扣 0.003 BTC FeeOnly;处理前后各写关键帧,随后请求新开仓
- 期望:两行净仓 0.001 BTC、无在途,同事件实际调用 risk.on_legs,新单按约 50 USDT 敞口通过;从首次启动及两侧关键帧重放输出全部相等
- 实现:`crates/hft-sequencer/tests/process.rs::seq_425_terminal_reports_and_base_fees_refresh_exposure_before_snapshot_replay`

| 行 | 回报 | 处理前名义 USDT | 处理后名义 USDT | 后续开仓 |
|---|---|---|---|---|
| SEQ-425.1 | 0.010 BTC 部分成交 0.001 后 Expired | 约 500,含在途 | 约 50 | 通过 |
| SEQ-425.2 | 0.004 BTC 成交后扣 0.003 BTC FeeOnly | 约 200 | 约 50 | 通过 |

### SEQ-426 不可解码或同号在途的凭据批次不应用且不停机

- 规则:[每条 Input 的处理顺序](README.md#每条-input-的处理顺序process)
- 级别:table
- 里程碑:M2
- 前置:B0 严格永续结算;空头 300000 已入账;已有一次完整的干净报告(report 801);批次 report_id = 802,一笔买入凭据
- 实现:`crates/hft-sequencer/tests/settlement.rs::seq_426_undecodable_or_in_flight_batch_is_not_applied_and_does_not_stop`

| 行 | 批次 | 期望 |
|---|---|---|
| SEQ-426.1 | 体截去最后 1 字节 | Idle;一条 `ReconcileRejected{report_id = 802, reason = 1}`;无 ReconcileCash;钱包与仓位不变;冷账本与验证回放无差异 |
| SEQ-426.2 | 先以普通 ReconcileReport 送入 report 802 的首条(count = 3),再送完整批次 | Idle;一条 `ReconcileRejected{reason = 2}` 与 `ReconcileCash{applied = 0}`;在途分组仍为 (802, count 3, received 1);钱包与仓位不变;冷账本与验证回放无差异 |

### SEQ-427 凭据确认不比较送达时刻,金额不一致只告警

- 规则:[每条 Input 的处理顺序](README.md#每条-input-的处理顺序process)
- 级别:table
- 里程碑:M2
- 前置:B0 严格永续结算;一笔买入 0.001 BTC 凭据(trade_id 41,现金 −1170003 raw)已由批次补账,auto_held = 100000
- 实现:`crates/hft-sequencer/tests/settlement.rs::seq_427_receipt_confirmation_ignores_timestamps_and_mismatch_does_not_stop`

| 行 | 同一成交的 WS 推送 | 期望 |
|---|---|---|
| SEQ-427.1 | `ex_ts_ns` 与 `rx_tsc` 不同,其余相同 | Idle;auto_held 归零;钱包不变;无 `ReceiptMismatch` |
| SEQ-427.2 | 同上且现金少 1 raw | Idle;恰好一条 `ReceiptMismatch{v[5] = 16}`;auto_held 仍 100000;钱包不变 |

两行都从补账前与待确认后的关键帧起步做冷账本与验证回放,无差异。

### SEQ-428 不带品种的回报不使严格现金引擎停机

- 规则:[每条 Input 的处理顺序](README.md#每条-input-的处理顺序process)
- 级别:table
- 里程碑:M2
- 前置:B0 严格永续结算,AC1 在 B1 上 5 张已 Ack 的订单(第一张卖 0.001 BTC)
- 实现:`crates/hft-sequencer/tests/settlement.rs::seq_428_report_without_inst_does_not_stop_a_strict_cash_engine`

| 行 | 回报 | 期望 |
|---|---|---|
| SEQ-428.1 | 第一张 `Reject{inst = 0}`(Binance WS API 错误响应的形态) | Idle |
| SEQ-428.2 | 第一张 `Fill{inst = 0}`,带完整现金事实 | Idle;钱包按现金与费用变化,仓位 −100000;冷账本相同;验证回放无差异 |

### SEQ-429 最近成交 seq 经快照恢复后守卫判定不变

- 规则:[每条 Input 的处理顺序](README.md#每条-input-的处理顺序process)、[快照](README.md#快照)
- 级别:integ
- 里程碑:M2
- 前置:B0(非严格现金);AC1 已有一次干净的完整报告(report 801);策略在第一条 A1 报价时在 B1 卖出 0.001 BTC
- 步骤:
  1. 写关键帧 K0;订单 Ack 后记下 `view_seq` = 当前末条 seq;交付全部成交;写关键帧 K1
  2. 送 report 802:`view_seq` 取第 1 步的值,Position 条目 `AUTO_EXTERNAL`,`engine_qty` = −0.001,`exch_qty` = −0.0005
  3. 送 report 803:同第 2 步,`view_seq` 取当前末条 seq
  4. 写关键帧,做完整与从 K0、K1 起步的验证回放
- 期望:
  - K1 的节 28 恰好一行 (AC1, B1, seq > view_seq);K0 没有节 28(SEQ-429.1、.2)
  - 第 2 步后仓位仍 −0.001(成交晚于报告视图,AUTO_EXTERNAL 不适用)(SEQ-429.3)
  - 第 3 步后仓位为 −0.0005(SEQ-429.4,对照)
  - 三种验证回放均无差异(从 K1 起步时守卫判定与连续运行相同)
- 实现:`crates/hft-sequencer/tests/settlement.rs::seq_429_latest_fill_seq_survives_snapshot_recovery`

### SEQ-430 AdoptExchange 的对账视图经快照恢复后守卫判定不变

- 规则:[快照](README.md#快照)、[每条 Input 的处理顺序](README.md#每条-input-的处理顺序process)
- 级别:integ
- 里程碑:M2
- 前置:B0(非严格现金)稳态;策略在 B1 卖出 0.001 BTC 并全部成交;`RECON_VIEW_REPORTS = 16`
- 步骤:
  1. report 802 的 `view_seq` 取成交之前、report 803 取成交之后,两者 Position 条目都是交易所 −0.0005、引擎 −0.001;写关键帧 K
  2. 确认后的 `AdoptExchange{report 802}`,再 `AdoptExchange{report 803}`
  3. 登记 report 804(当前视图,交易所 −0.0004),再送 16 个只有 Summary 的报告,然后 `AdoptExchange{report 804}`
  4. 写关键帧,做完整与从初始关键帧、K 起步的验证回放
- 期望:
  - K 的节 29 最后两份报告为 802、803(SEQ-430.1)
  - 802 → `PreconditionFailed{StaleReport}`,仓位不变(SEQ-430.2);803 → `Applied`,仓位 −0.0005(SEQ-430.3)
  - 804 被淘汰 → `PreconditionFailed{StaleReport}`,仓位仍 −0.0005(SEQ-430.4)
  - 三种验证回放均无差异;关闭节 29 的恢复时从 K 起步的回放出现差异
- 实现:`crates/hft-sequencer/tests/settlement.rs::seq_430_adopt_stale_guard_and_retained_views_survive_recovery`

### SEQ-431 对账视图表的淘汰与溢出有界且确定

- 规则:[快照](README.md#快照)
- 级别:table
- 里程碑:M2
- 前置:空 `ReconViews`;`RECON_VIEW_DIFFS = 4096`、`RECON_VIEW_REPORTS = 16`;每行 Position 引擎 10、交易所 4(差量 −6)
- 实现:`crates/hft-sequencer/tests/settlement.rs::seq_431_recon_view_table_overflow_is_defined_and_bounded`

| 行 | 输入 | 期望 |
|---|---|---|
| SEQ-431.1 | report 1 一行,再 report 2 的 4096 个不同品种 | 均不报溢出;report 1 被淘汰;report 2 的差量 −6 |
| SEQ-431.2 | report 2 再来第 4097、4098 个不同品种 | 第 4097 行返回溢出(仅一次);report 2 标记 OVERFLOW,差量全部丢弃(查得 0) |
| SEQ-431.3 | 再登记 19 个新 report(非 Position 条目) | 只保留 16 份,最旧的 report 2 与前 3 个新 report 被淘汰 |

## 验收

本组件在 M2 的完成标准(门槛编号与累计量的唯一定义在 [testing §8](../../docs/testing.md#8-上线门槛);数值上限在 [benchmarks](../../benchmarks/README.md)):

1. **用例**:里程碑 ≤ 当前里程碑的全部用例有同编号的测试函数、场景或基准,`cargo xtask test-trace` 三张清单为空([docs/tests §6](../../docs/tests/README.md#6-与代码的对应))。
2. **属性测试**:PR 跑各用例 `例数` 的 PR 值;每夜跑其每夜值,零失败(失败的最小化反例进回归集)。
3. **DST**:SEQ-076、SEQ-077、SEQ-217、SEQ-267、SEQ-272、SEQ-342、SEQ-343、SEQ-344 的 PR 种子与回归集全过;每夜日期派生种子全过;服务器累计量与零违反按 testing §8 的门槛 ③④。
4. **基准**:SEQ-317–SEQ-339 在参考机上不超过 [延迟与资源契约](README.md#延迟与资源契约) 的 p50/p99 与 benchmarks 登记的上限;SEQ-319(`seq/l2burst`)的逐样本断言从 M2 起每次基准运行都通过;主机内延迟的验收数值与测法归 testing ⑩ 与 [latency-budget](../../docs/latency-budget.md)。
5. **验证回放**:夜跑对当日 journal 跑 `replayer verify`,Output 逐字节一致(Meta 与测量字段除外);M4 起模拟账户在实时行情上连续运行期间每夜 `replay verify` 通过。
6. **主机项**:SEQ-316 在 M3 联调窗口内留存证据;冷启动 < 30 s 的演练归 [apps/hft-core](../../apps/hft-core/README.md#启动顺序)。
7. **跨组件场景**:与本组件相关的场景(RC-1–RC-12、MD-5、MD-13、MD-14、QF-1、BK-4、BK-7、AX-1、C2、C10、C13)的编号与期望在 [scenarios](../../docs/tests/README.md#2-文件与范围);本文件的用例只覆盖 sequencer 侧的期望。


### SEQ-408 策略持有的 TimerId 经崩溃、Drain 与连续重启保持有效

- 规则:[定时器](README.md#定时器)、[快照](README.md#快照)
- 级别:table
- 里程碑:M2
- 前置:B0 真实引擎,journal 含完整 ECF1/RKP1;测试策略在自己的 snapshot/restore 中保存 TimerId;六次登记的截止乱序,两条具有完全相同的 deadline/tag/group;一次取消和一次到期留下编号空洞
- 步骤:分别从崩溃与 Drain 恢复,TSC 归零附近且频率变为三倍;策略用恢复的 ID 取消两个登记与两个旧 ID,再登记一次;第二次崩溃后恢复原频率,推进至到期;独立执行完整及关键帧窗口回放
- 期望:首次恢复保留 ID 2/4/5/6,下一次分配为 7;第二次恢复保留 5/6/7;取消的一条相同 tag 登记不影响另一条,三条存活登记各到期一次;三次运行的 Output、快照字节与恢复状态均一致;Drain 结束后的段尾 Meta 不导致 Unfinished,该运行的 Input 与 Output 仍全部检查
- 实现:`crates/hft-sequencer/tests/timer_recovery.rs::seq_408_timer_ids_survive_crash_drain_and_repeated_restart`

| 行 | 第一次恢复起点 | 第二次恢复 | 期望 |
|---|---|---|---|
| SEQ-408.1 | 崩溃,最新关键帧并重放尾部 | 再次崩溃,最新关键帧 | 保存的 ID 可精确取消;存活 5/6/7 各触发一次;三次运行完整及窗口回放通过 |
| SEQ-408.2 | DrainTail,不重放尾部 | 再次崩溃,最新关键帧 | 与上一行相同;Drain 不重置 ID 计数器 |

### SEQ-409 缺失或不一致的定时器身份历史拒绝恢复

- 规则:[定时器](README.md#定时器)、[快照](README.md#快照)
- 级别:table
- 里程碑:M2
- 前置:真实引擎产生的关键帧;不借助非零声明哈希阻断,直接检验恢复路径
- 步骤:逐项删除或重复节 27、改变行版本、清零计数器、令计数器不超过存活 ID、清零或重复 ID、修改截止/tag/group、污染填充、替换成未知实例、删除实例计数行、复制节 8 的物理定时器、伪造超出体长的登记数;另在定时器已全部到期的空堆样本中删除节 27
- 期望:所有反例返回 `SnapshotUndecodable{section: 27}`,无静默补号或漏记;登记与堆核对保留重复键的条数
- 实现:`crates/hft-sequencer/tests/timer_recovery.rs::seq_409_invalid_or_missing_timer_identity_history_is_rejected`

| 行 | 反例 | 期望 |
|---|---|---|
| SEQ-409.1 | 存活登记非空时删除节 27 | SnapshotUndecodable{27} |
| SEQ-409.2 | 存活登记已清空时删除节 27 | SnapshotUndecodable{27} |
| SEQ-409.3 | 不支持的行版本 | SnapshotUndecodable{27} |
| SEQ-409.4 | next_id 为 0 | SnapshotUndecodable{27} |
| SEQ-409.5 | next_id 等于最大的存活 ID | SnapshotUndecodable{27} |
| SEQ-409.6 | 存活 ID 为 0 | SnapshotUndecodable{27} |
| SEQ-409.7 | 重复存活 ID | SnapshotUndecodable{27} |
| SEQ-409.8 | 登记截止与堆不同 | SnapshotUndecodable{27} |
| SEQ-409.9 | 登记 tag 与堆不同 | SnapshotUndecodable{27} |
| SEQ-409.10 | 登记 group 与堆不同 | SnapshotUndecodable{27} |
| SEQ-409.11 | 非零保留字段 | SnapshotUndecodable{27} |
| SEQ-409.12 | 未绑定的实例 | SnapshotUndecodable{27} |
| SEQ-409.13 | 缺一个实例的计数行 | SnapshotUndecodable{27} |
| SEQ-409.14 | 堆中额外复制一个物理键相同的定时器 | SnapshotUndecodable{27} |
| SEQ-409.15 | 重复节 27 | SnapshotUndecodable{27} |
| SEQ-409.16 | 登记数超出体长 | SnapshotUndecodable{27} |

### SEQ-410 空定时器堆恢复后不复用已消耗的 ID

- 规则:[定时器](README.md#定时器)
- 级别:unit
- 里程碑:M2
- 前置:journal 含完整 ECF1/RKP1;策略实际登记 ID 1–7,全部已取消或到期,保存关键帧
- 步骤:崩溃重启;策略登记一个新定时器并等其到期;保存末尾快照;执行完整及关键帧窗口回放
- 期望:恢复的存活登记为空,计数器为 8,新返回值为 8;该定时器只到期一次;两次运行的 Output、快照字节与恢复状态均一致
- 实现:`crates/hft-sequencer/tests/timer_recovery.rs::seq_410_empty_heap_retains_consumed_timer_ids`

### SEQ-411 实际现金贯通热引擎、冷读者和完整及窗口回放

- 规则:[每条 Input 的处理顺序](README.md#每条-input-的处理顺序process)
- 级别:table
- 里程碑:M2
- 前置:B0 真实引擎,完整 ECF1/RKP1,永续 venue 开启 PERP_CASH;五笔事实取 LED-275 小例,金额按 B0 的 USDT 指数 −8 换算;策略由不同报价确定下单,无外部计数器
- 步骤:真实下单与 Ack 后按行交付成交,每笔重复;一笔改为 FeeOnly 晚费;下单前、下单后及三笔已入账时保存关键帧
- 期望:每笔钱包等于初始值 + 原始现金 − 费用,最终扣 1180000 与 18007900;冷读者从三个关键帧起步均得出相同仓位及钱包,下单前快照恢复当前 epoch;完整回放与三个窗口的 Output 和状态验证无分歧
- 实现:`crates/hft-sequencer/tests/settlement.rs::seq_411_cash_wallet_matches_cold_reader_and_full_window_replay`

| 行 | 交付顺序 |
|---|---|
| SEQ-411.1 | 1、2、3、4、5 |
| SEQ-411.2 | 1、2、4、3、5 |
| SEQ-411.3 | 5、3、4、2、1 |

### SEQ-412 坏结算事实在对冲和入账前停止

- 规则:[每条 Input 的处理顺序](README.md#每条-input-的处理顺序process)
- 级别:table
- 里程碑:M2
- 前置:真实引擎、开启 PERP_CASH 的永续第一腿,OnFill 第二腿为现货
- 步骤:逐行送入坏事实;冲突行先入一笔有效部分成交
- 期望:明确 Fatal,坏事件无 OrderDecision,钱包和账本仓位不变,不调用 ledger.apply_fill
- 实现:`crates/hft-sequencer/tests/settlement.rs::seq_412_bad_cash_stops_before_hedge_and_wallet_booking`

| 行 | 输入 | Fatal |
|---|---|---|
| SEQ-412.1 | 成交缺现金覆盖 | OmsCash(Missing) |
| SEQ-412.2 | 同水位异额 | OmsCash(Conflict) |
| SEQ-412.3 | 错误 epoch | OmsCash(Untracked) |
| SEQ-412.4 | 价格为零 | Settlement(InvalidPrice) |
| SEQ-412.5 | 外来成交缺现金覆盖 | Settlement(MissingCash) |

### SEQ-413 外来结算贯通冷读与关键帧去重回放

- 规则:[每条 Input 的处理顺序](README.md#每条-input-的处理顺序process)
- 级别:table
- 里程碑:M2
- 前置:与 SEQ-411 相同的 B0、现金事实和三种交付排列;事件为各自具有独立 trade_id 的 Manual ExternalFill
- 步骤:逐笔交付并重复,第三笔入账后、重复前保存关键帧;从起始与该关键帧冷读,执行完整和两个窗口回放
- 期望:每步钱包等于原始现金与费用的独立累计,最终扣 1180000 与 18007900;重复不生成订单,冷读仓位及去重键与热账本相同;恢复后重复仍不入账,Output 与快照无回放分歧
- 实现:`crates/hft-sequencer/tests/settlement.rs::seq_413_external_cash_dedup_cold_snapshots_and_full_window_replay`

| 行 | 交付顺序 |
|---|---|
| SEQ-413.1 | 1、2、3、4、5 |
| SEQ-413.2 | 1、2、4、3、5 |
| SEQ-413.3 | 5、3、4、2、1 |

### SEQ-414 余额校验事实贯通完整视图与回放

- 规则:[每条 Input 的处理顺序](README.md#每条-input-的处理顺序process)
- 级别:unit
- 里程碑:M2
- 前置:B0 严格现金能力、无在途订单、已初始化钱包;选取前置关键帧
- 步骤:发送差额 1000 USDT 的余额报告,再发送与钱包相等的报告;每次冷读至完成边界,末尾保存关键帧并运行完整及前后窗口回放
- 期望:每个报告有且仅有一条 BalanceCheck;Drift 先置位再清除,完整现金、summary/vflags 与冷账本相等,钱包始终不变;完整与两个窗口回放无差异
- 反例:篡改校验事实的原始余额或把该 Output 伪装成 Meta,完整后缀回放必须发现分歧
- 实现:`crates/hft-sequencer/tests/settlement.rs::seq_414_balance_check_gate_matches_cold_view_and_full_window_replay`
