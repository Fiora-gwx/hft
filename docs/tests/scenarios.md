# 跨组件场景测试规格

格式与规则见 [docs/tests](README.md)。本页是跨组件场景用例的唯一归属:崩溃恢复与冷启动、模式与人工干预、恢复与对账、执行正确性、断腿与保证金、OMS 与执行的失败路径。全局不变量见 [invariants.md](invariants.md),需要真实主机、交易所或人工的演练见 [drills.md](drills.md);测试层次、CI 运行矩阵与上线门槛归 [testing](../testing.md)。

## 1. 编号族登记

编号是稳定 id,其他 README 与 `TESTS.md` 直接引用;删除的编号不复用。

| 族 | 含义 | 本页小节 |
|---|---|---|
| `RC-n` | 崩溃恢复与冷启动 | §3 |
| `MD-n` | 模式、账户与人工干预 | §4 |
| `B-n` | 恢复、OMS 与对账 | §5 |
| `A5`、`B1`–`B6`、`C1`–`C15` | 执行正确性 | §6 |
| `BK-n` | 断腿与 `Broken` 处置 | §7 |
| `QF-n` | 第二腿盘口长时间不变而连接健康 | §7 |
| `LK-n` | 带 `LegLink` 的开仓检查与对冲定价 | §7 |
| `LT-n` | 两腿 lot 对齐 | §7 |
| `FL-n` | flatten 锁 | §7 |
| `MS-n` | MarginStress 与单边行情压力(`MS-2a` 是 MS-2 以上线策略最终参数运行的版本,门槛 ⑦) | §7 |
| `AX-n` | 用户流缺口里的 ADL(`AUTO_EXTERNAL`) | §7 |
| `FD-n` | 资金费差异与校正 | §7 |
| `AL-n` | advisor 落后与建议新鲜度 | §7 |
| `FP-n` | OMS 与执行的必测失败路径 | §8 |
| `INV-n` | 全局不变量 | [invariants.md](invariants.md) |
| `DRL-n` | 主机、测试网与运维演练 | [drills.md](drills.md) |

**未使用与保留的编号**:`B-9`、`B-11`、`B-13`、`B-15`、`B-16`、`B-19`、`B-20`、`B-21`、`C3`、`C5`、`MS-1` 未使用,不再分配。`B-7`(链上买卖的成交量一律是基础资产数量)、`FP-13`(链上 403/429/超时、`IncludedReverted`/`Dropped`、迟到上链、回执与日志不符)与 [INV-10](invariants.md#2-可选扩展的不变量) 属于可选扩展(链上),里程碑外;编号保留,用例在 [hft-chain-evm](../../crates/hft-chain-evm/README.md) 解冻时写进它的 `TESTS.md`。

## 2. 约定与公共夹具

### 2.1 时间与数值

- `t` 是场景开始后的引擎时间(`Now.engine_ns`,跨重启连续、停机不计);场景开始 = 全部账户的启动闸门放行、`Warmup` 结束后的第一条 Input,另有说明的除外。"停机 30 min"指墙钟,引擎时间不前进。
- 时长写成 ms 或 s;数量与价格写十进制,定点指数取 §2.2 的资产指数(BTC `px_exp = −4`、`qty_exp = −8`,名义 `Notional` 指数 −6)。
- "同一 Input 内"指处理该 Input 产生的全部 Output 记录位于它的 `EVENT_END` 之前([hft-sequencer 每条 Input 的处理顺序](../../crates/hft-sequencer/README.md#每条-input-的处理顺序process))。
- "无 X"指从所述时刻到场景结束,journal 的 Output 记录与出环帧中都没有 X。

### 2.2 世界 W1(两所、两腿)

DST 与集成用例默认在 W1 上运行;用例只写与 W1 不同的设定。venue 行为只取本夹具的 `VenueCaps` 与 hft-testkit 的 venue 行为层,不代表该交易所的真实行为。

| 项 | 取值 |
|---|---|
| venue | `VS`:`VenueId = 2`(`BINANCE_SPOT`),`Exchange::Binance`,现货;`VP`:`VenueId = 5`(`OKX_SWAP`),`Exchange::Okx`,USDT 永续;`VQ`:`VenueId = 11`(`BITGET_UTA_SPOT`),`Exchange::Bitget`,现货(只在写明时使用) |
| inst | `S1` = BTC-USDT 现货 @ `VS`:tick 0.01 USDT,lot 0.00001 BTC,`min_qty` 0.00001 BTC,`min_notional` 5 USDT;`P1` = BTC-USDT 永续 @ `VP`:tick 0.1 USDT,lot 0.001 BTC,`min_qty` 0.001 BTC,`min_notional` 5 USDT,`max_leverage` 50 |
| 资产 | BTC `AssetExp{px_exp: −4, qty_exp: −8}`;USDT 金额指数 −6 |
| 账户 | `a`、`b`:`AccountKind::Live`,`is_prod = false`,`venues = {VS, VP}`,`uid_policy = Verified`;`a` 绑定策略实例 1,`b` 绑定策略实例 2 |
| 初始余额 | 每个账户:`VS` 上 100,000 USDT、0 BTC;`VP` 上 50,000 USDT 保证金、`P1` 仓位 0、实际杠杆 5×(单向持仓、全仓) |
| `VenueCaps` | `VS`:`fee_asset_rule = Quote`,`reduce_only = false`,`stale_guard = RecvWindow`;`VP`:`fee_asset_rule = Quote`,`reduce_only = true`,`funding_settle = PerSymbolPush`,`margin_data = Push`,`stale_guard = RecvWindow`;二者 `ignorable_user_events` 各含一个事件名 `"evt_ignorable"` |
| 行情 | t = 0 时 `S1` BBO 59,999.99 / 60,000.00(各 2 BTC);`P1` BBO 60,010.0 / 60,010.1(各 5 BTC);mark = 指数 = 60,010.0;两条 BBO 连接每 `bbo_ping_ms` 回 pong;无其他变化时行情由种子产生 ±0.01% 的随机游走 |
| group | `g1` = 策略实例 1 的 group:第一腿 `S1` 买,第二腿 `P1` 卖,ratio 1,`open_allowed = 1`;账户 `b` 的 `g2` 同构 |
| 策略 | 实例 1、2 为 hft-testkit 测试策略:用 `OrderIntent` 与 `LegLink` 的用例用 `probe-taker`,用 `TargetIntent` 的用例用 `probe-target`([hft-testkit 解耦门槛夹具](../../crates/hft-testkit/README.md#5-解耦门槛夹具));期望模式 `Live` |
| 风控基线 | `RiskParams.max_perp_leverage_x100 = 500`(实际杠杆 5× 在上限内,`ACCOUNT_CFG` 放行);其余 `RiskParams`、`ExecConfig`、`OmsConfig` 与组件配置由各节的参数基线(如 §6 的 R6)或用例 `前置` 给出 |
| 运行 | `prod` 角色实例,`feed_env = order_env` = DST 的 `MockVenue`;kill-switch 与 reconciler 由 `CtlActor`、`ReconActor` 代表([hft-testkit DST 框架](../../crates/hft-testkit/README.md#6-dst-框架)) |

**W1-SIM**:与 W1 相同,但账户 `s` 为 `AccountKind::Sim`、label `"sim-s"`,订单进 `SimActor`;实例名 `"sim"`,refdata 只含 Sim 账户。

### 2.3 级别与落点

| 级别 | 本页的用法 | `实现` 落点 |
|---|---|---|
| `dst` | 种子决定行情噪声、回报时延与输入交错,`故障` 给出确定的故障时间表 | `crates/hft-testkit/src/scenarios/<主题>.rs::<编号小写>_<描述>`(`Scenario::new("<编号>", …)` 注册) |
| `integ` | 进程内、确定的步骤序列(假交易所 + 真实 sequencer),不需要种子 | `crates/hft-testkit/src/scenarios/<主题>.rs` 或仓库根 `tests/recover/<主题>.rs` |
| `prop` | 组件内的属性测试,本页定义跨组件的期望 | 发起方组件的 `crates/<x>/tests/` |
| `proc` | 多个真实进程(hft-core、kill-switch、ops_bot…)与 socket,Linux CI | 发起方 app 的 `apps/<x>/tests/` 或 `python/<x>/tests/` |
| `table` | 一张输入 → 期望表,每行一个子用例 | 同 `integ` |

编号转函数名:小写、`-` 换成 `_`(`RC-1` → `rc_1_…`,`B1` → `b1_…`,`B-1` → `b_1_…`);Python(pytest)前面另加 `test_`(`MD-12` → `test_md_12_…`,[docs/tests §6](README.md#6-与代码的对应))。用例的附加变体写成表,行号 `<编号>.<k>`,实现为同一函数的 `rstest` case。

### 2.4 种子集

- **标准集**:PR 由提交 SHA 派生 200 个种子 + `fixtures/dst-seeds/` 回归集;CI 每夜由日期派生(回归冒烟,不计入 ④);服务器上 `hft-dst` 运行新种子,计入门槛 ④([testing §6.1](../testing.md#61-服务器上的累计运行))。
- **registry 集**:以已注册生产策略在 `strategies/registry/tests/` 运行;改动 `strategies/` 或引擎 crate 的 PR 与 CI 每夜运行;不进 `hft-dst`,不计入 ④。

## 3. 崩溃恢复与冷启动

本节的"崩溃"指丢弃 sequencer 实例(hft-testkit `CrashFault` 或测试直接 drop),只保留 journal 段、冷副本与 `DurableMeta`;"重启"指以 `RecoverDriver` 按 `RecoverPlan` 重建,随后实盘驱动(假时钟 `ManualTsc`)写本次运行的 `EngineClock`。"闸门输入齐备"指每个 Live 账户在 `VS`、`VP` 上依次收到:`ConnState{Order, Up}`、`ConnState{UserStream, Up}`、热备 Up、两条 BBO 首帧、模式一致且 UID 相符的 `AccountConfigReport`(含 `P1` 杠杆 5×)、`Timer{Sys, KillSelfTestOk}`、`Timer{Sys, ClockOk}`,以及 `Note{EpochStart}` 之后一组全部 `Ignore`、无 `TRUNCATED`、`skipped_inflight = 0`、无 `USERSTREAM_DOWN` 的 `ReconcileReport`;用例另有说明的项除外。`Note{PartialEventDropped}`、`Note{TailSkipped}` 的写出与退出码映射属于 hft-core 启动步骤,本节断言它们依据的恢复结果。

### RC-1 崩溃点落在一个输入事件中途时截断到上一个完整事件,半个事件的输出从未出环

- 规则:[冷启动恢复(RecoverDriver)](../../crates/hft-sequencer/README.md#冷启动恢复recoverdriver)、[每条 Input 的处理顺序](../../crates/hft-sequencer/README.md#每条-input-的处理顺序process)、[写者](../../crates/hft-journal/README.md#写者)、[提交游标](../subsystems/journal-format.md#提交游标)、[启动顺序](../../apps/hft-core/README.md#启动顺序)
- 级别:dst
- 里程碑:M5
- 门槛:⑤
- 前置:W1;`keyframe_period_ms = 60000`,`keyframe_max_events = 200000`;实例 1 为 probe-taker,按种子行情在 g1 上发带 `LegLink` 的两腿意图(第一腿 `S1` 买 0.01 BTC IOC,第二腿 `P1` 卖,`HedgePlan{OnFill, on_fail = Retry}`);`hedge_deadline_ms = 3000`,`hedge_retry_min_ms = 100`
- 故障:`CrashFault{at_step}` 一次,`at_step` 由种子选在某个 Input 的处理中途——该 Input 与它的至少一条 Output 已 `append`、`end_event` 未调用;种子集中至少 30% 的崩溃点落在第一腿成交回报(`OrderEvent{Fill}`)的处理中途(其第二腿 `OrderDecision{New}` 已 `append`);崩溃后停机 0 s 重启一次
- 种子:标准集(§2.4)
- 断言:
  - 重启后 journal 中崩溃前的最后一条记录是崩溃前最后一个带 `EVENT_END` 的记录;被截事件的 Input 与 Output 都不在 journal 中,新段 `SegmentRoll{Begin}` 的 seq = 截断后末条 seq + 1
  - `MockVenue` 收到的出环帧中没有 `trigger_seq` = 被截 Input seq 的帧
  - `RecoverDriver` 报告的被截事件 `InputHash` = `xxh3_64(kind ‖ body)`(取被截 Input 的 kind 与体字节)
  - 被截的是第一腿成交回报时:该槽恢复后为 `Unknown`;查询收敛(结果带这笔成交)的同一 Input 内产生 `link_role = Second` 的第二腿 `OrderDecision`;该槽的 `FillOut` 合计等于交易所累计成交量,不多不少
  - 每步 INV-2、INV-6、INV-7、INV-9;场景结束且无在途时 INV-16
- 实现:`crates/hft-testkit/src/scenarios/recover.rs::rc_1_half_event_truncated`
- 备注:`Note{PartialEventDropped{seq, kind}}`(P1)由 hft-core 启动步 4 依据同一恢复结果写出,用例归 apps/hft-core 的 `TESTS.md`;sequencer 内部的同类 DST 见 hft-sequencer 的 `TESTS.md`

### RC-2 关键帧写到一半崩溃时从上一个完整快照起步,新段带完整引导块

- 规则:[快照](../../crates/hft-sequencer/README.md#快照)、[冷启动恢复(RecoverDriver)](../../crates/hft-sequencer/README.md#冷启动恢复recoverdriver)、[段滚动与引导块](../subsystems/journal-format.md#段滚动与引导块)、[快照记录结构](../subsystems/journal-format.md#快照记录结构)、[恢复与校验](../../crates/hft-journal/README.md#恢复与校验)
- 级别:integ
- 里程碑:M5
- 门槛:⑤
- 前置:W1;`keyframe_period_ms = 1000`,`keyframe_max_events = 100000`;实例 1、2 为 probe-target,t = 0.1 s 各设一次 `TakerSlice` 目标(`S1` +0.01 BTC);段内已有关键帧 K1(t = 0 后第一次到期写出,seq = k1)
- 步骤:
  1. 运行到 t = 1.0 s 后第一个空闲轮,实盘驱动交出 `Timer{Sys, Snapshot}`;`StateSnapshot{purpose = Keyframe}` 多段记录写出第 2 段之后丢弃 sequencer 实例
  2. 停机 60 s 后重启(`RecoverPlan.start = Latest`)
  3. 闸门输入齐备
- 期望:
  - 步骤 2:`RecoveryReport.rebuild_from` 指向 K1;`Timer{Sys, Snapshot}` 事件与未完成的快照多段记录全部被截,其字节清零
  - 步骤 2:从 K1 起重放 K1 之后、截断点之前的全部 Input,重放出的每条 Output 与 journal 逐字节相同;无 `Note{RecoverMismatch}`
  - 步骤 2:新段段头 `roll_reason = Recovery`;引导块依次为 `SegmentRoll{Begin}`、`HostProfile`、`ClockStatus`、`ConfigBlob{boot = 1}` × 5(refdata、EngineConfig、实例 1 与实例 2 的 `StrategyParams`、`RiskParams`)、`StateSnapshot{purpose = Recovery}`、每个 Live 簿一条 `L2Snapshot{origin = Keyframe}`(条数 = 快照头 `n_books`)、`SegmentRoll{BootDone}`(`boot_records` = 10 + `n_books`);其后第一条普通记录是 `EngineClock`(`source = Tail`),同一事件内 `Note{EpochStart}`
  - 步骤 3 之后:`replayer verify` 从新段引导块起步逐字节通过
- 实现:`tests/recover/roll.rs::rc_2_partial_keyframe_restarts_from_previous`

| 行 | 相对主例的变化 | 断言 |
|---|---|---|
| RC-2.1 | 崩溃落在整点滚动的引导块中途:新段已写 `SegmentRoll{Begin}`、`HostProfile`、`ClockStatus`,未到 `BootDone` | 该新段按 `Preparing` 整段删除(热段与已复制的冷副本);恢复从上一段最后一个完整快照起步;重开的段与被删段同号,`roll_reason = Recovery`,引导块完整 |
| RC-2.2 | 崩溃落在 Drain 的 `StateSnapshot{purpose = Drain}` 写出中途 | journal 末尾没有完整的 Drain 快照:恢复照常重放尾部,恢复标志不含 `CLEAN_DRAIN`;闸门放行后 `Recovery` 仍在,`Resume{Recovery}` 返回 `Applied` 后才解除 |

### RC-3 尾部含已发出未回执的订单时槽进入 Unknown,收敛前该 group 不开仓

- 规则:[冷启动恢复(RecoverDriver)](../../crates/hft-sequencer/README.md#冷启动恢复recoverdriver)、[恢复:停机时间不计与闸门续接](../../crates/hft-oms/README.md#恢复停机时间不计与闸门续接)、[Unknown 收敛流程](../../crates/hft-oms/README.md#unknown-收敛流程)、[启动闸门](../../crates/hft-risk/README.md#启动闸门)、[检查链](../../crates/hft-risk/README.md#检查链按廉价--昂贵短路适用范围以本表为唯一定义)
- 级别:integ
- 里程碑:M5
- 门槛:⑤
- 前置:W1;实例 1 为 probe-target;`ack_timeout_ms[VS] = 2000`,`unknown_query_interval_ms = 1000`,`not_found_grace_ms[VS] = 2000`,`unknown_alarm_ms = 30000`,`unknown_blocks_open = true`,`auto_resume_after_clean_drain = true`
- 步骤:
  1. t = 0:实例 1 设 g1 目标 `S1` = +0.01 BTC、`P1` = 0(`TakerSlice`);执行层发 IOC 买 0.01 BTC @ 60,000.00(槽 s1,`OrderDecision{New}`),gw 回 `SendReport{Place, Written}`;交易所不回报
  2. t = 0.3 s:崩溃;停机 10 min;重启
  3. 闸门输入除 `NO_UNKNOWN` 与 `RECONCILED` 外齐备,其中 `ConnState{Order, Up}`(`VS`)到达
  4. 实例 1 把 g1 目标改为 `S1` = +0.01、`P1` = −0.01 BTC
  5. `QueryResult{slot = s1, status = Filled, cum_filled = 0.01, avg_px = 60,000.00}` 到达
  6. `VS`、`VP` 各一组全部 `Ignore`、`skipped_inflight = 0` 的 `ReconcileReport`
  7. `Resume{Recovery, account = a}`(已确认)
- 期望:
  - 步骤 2 之后:s1 为 `Unknown`(`Note{OmsTransition}` 迁移原因 `RecoveryEnd` = 20),登记一条 `Timer{Oms, UnknownQuery}`;`venue_ready(VS, a) = false`;账户 a 有 `Recovery` cap
  - 步骤 3:`ConnState{Order, Up}` 的同一 Input 内对 s1 发一条 `OrderDecision{Query}`;账户 a 的 `gate_bits` 中 `NO_UNKNOWN` = 0,`released = false`
  - 步骤 4 到步骤 7 之前:有效模式为 `CloseOnly`(`Recovery` cap),journal 中无 `P1` 的 `OrderDecision{New, reject = 0}`,出环帧中无 `P1` 的新单
  - 步骤 5:s1 → `Filled`,`FillOut{qty = 0.01}` 入账,ledger `S1` 库存 0.01 BTC;`Note{OmsVenueReady}` 一条;`NO_UNKNOWN` 置位
  - 步骤 6:`RECONCILED` 置位,闸门放行(`Note{StartupGate}` 的 `v[3] = 1`);`Recovery` 仍在(尾部非空,不是干净 Drain)
  - 步骤 7:`CmdResult{Applied}` 与 `CapCleared{Recovery}`;同一 Input 内出环 `P1` 卖 0.01 BTC 的 `OrderDecision{New, reject = 0}`
- 实现:`tests/recover/unknown_tail.rs::rc_3_unacked_order_unknown_blocks_open`

| 行 | 相对主例的变化 | 断言 |
|---|---|---|
| RC-3.1 | 步骤 5 改为 `QueryResult{slot = s1, status = NotFound}`,之后 `ReconcileReport` 的 `UnknownResolved{slot = s1, history_checked = 0}`,再一组 `UnknownResolved{slot = s1, order_status = NotFound, exch_qty = 0, history_checked = 1}` | NotFound 后 s1 仍为 `Unknown`;引擎时间越过 s1 的 `t_state + not_found_grace_ms` 之后的那次 NotFound 置 `HIST_WAIT` 并写 `Note{OmsNeedsRestQuery}`(`trades = 1`);`history_checked = 0` 的条目后仍为 `Unknown`;`history_checked = 1` 的条目后 s1 → `Rejected`(`reject_code = 0`,迁移原因 `Reconcile` = 13),`Note{OmsVenueReady}` 一条 |
| RC-3.2 | 步骤 5 不发生,s1 持续 `Unknown` | 恢复后引擎时间 29 s 时的 `Timer{Oms, Sweep}` 不产生下列两项;32 s 时已各有一条:`level = P0` 的 `Note{OmsUnknown}`,`UnknownTimeout{a, VS}` 的 `Note{CapSet}` |

### RC-4 版本变化后冷启动只从末尾的 Drain 快照起步,否则以 VersionNeedsDrain 拒绝启动

- 规则:[版本变化与回滚](../../crates/hft-sequencer/README.md#版本变化与回滚)、[冷启动恢复(RecoverDriver)](../../crates/hft-sequencer/README.md#冷启动恢复recoverdriver)、[干净 Drain 重启自动解除 Recovery](../../crates/hft-risk/README.md#干净-drain-重启自动解除-recovery)、[命令行与退出码](../../apps/hft-core/README.md#命令行与退出码)
- 级别:integ
- 里程碑:M5
- 门槛:⑤
- 前置:W1;两个构建 A、B:`WriterEnv.build_git` 不同,实例 1(probe-taker)的 `StrategyDesc.version` A = 3、B = 4(B 的 `restore` 接受 3 与 4);`auto_resume_after_clean_drain = true`
- 步骤:
  1. 构建 A 运行到 t = 5 s,账户 a 的 g1 持有 `S1` +0.01、`P1` −0.01 BTC;`Drain{cancel_resting = true}` 以 `Done` 结束(journal 末尾 `StateSnapshot{purpose = Drain}`,其后无 Input)
  2. 以构建 B 冷启动
  3. 闸门输入齐备
  4. `Resume{Recovery, account = a}`(已确认)
- 期望:
  - 步骤 2:以末尾 Drain 快照起步(`start = DrainTail`),重放的 Input 数 = 0;实例 1 的 `restore` 读到 `desc_version = 3` 并成功;新段引导块的 `StateSnapshot` 中 g1 的腿净量为 +0.01 / −0.01
  - 步骤 3:闸门放行,`Recovery` 仍在(版本不同,不满足干净重启条件 ①),无 `CapCleared{Recovery}`
  - 步骤 4:`CmdResult{Applied}`,`CapCleared{Recovery}`
- 实现:`tests/recover/version.rs::rc_4_version_change_needs_drain_snapshot`

| 行 | 相对主例的变化 | 断言 |
|---|---|---|
| RC-4.1 | 步骤 1 改为构建 A 在 t = 5 s 崩溃(末尾不是 Drain 快照,最近关键帧之后有 120 条 Input) | `Sequencer::boot` 返回 `Err(BootError::VersionNeedsDrain)`;journal 无新段、无新记录;`DurableMeta.engine_epoch` 不变(hft-core 以退出码 4 退出,映射归 hft-core) |
| RC-4.2 | 构建 B 的 `StrategyDesc.version` = 5(只接受 4 与 5),Drain 快照节 5 的 `desc_version = 3` | `Sequencer::boot` 返回 `Err(BootError::SnapshotUndecodable)`;journal 无新段 |

### RC-5 版本不变而尾部输出与重算输出不一致时报 RecoverMismatch 并对受影响账户收紧闸门

- 规则:[恢复时决策不一致](../../crates/hft-sequencer/README.md#恢复时决策不一致)、[冷启动恢复(RecoverDriver)](../../crates/hft-sequencer/README.md#冷启动恢复recoverdriver)、[启动闸门](../../crates/hft-risk/README.md#启动闸门)、[干净 Drain 重启自动解除 Recovery](../../crates/hft-risk/README.md#干净-drain-重启自动解除-recovery)
- 级别:integ
- 里程碑:M9
- 门槛:⑤
- 前置:W1;同一构建;实例 1、2 为 probe-taker;最近关键帧 K 在 t = 1.0 s
- 步骤:
  1. 运行到 t = 2.0 s;测试以 `JournalWriter` 写出 K 之后的尾部:Input 与实际运行相同,其中一条账户 a 在 `VS` 的 `OrderDecision{New, inst = S1}` 的 `px` 改为重算值 + 0.01 USDT(1 tick),记录 CRC 按改后字节计算;随后崩溃
  2. 同一构建重启
  3. 闸门输入齐备
- 期望:
  - 步骤 2:恢复不中止;写 `Note{RecoverMismatch}`(P0,`v[0]` = `VS` \| a << 8,`v[1]` = 被改那条 `OrderDecision` 的 seq)恰好一条;(`VS`, a) 的全部非终态槽为 `Unknown`(含崩溃前 `Live` 的挂单)并各登记一条 `Timer{Oms, UnknownQuery}`;账户 a 的恢复标志含 `RECOVER_MISMATCH`,账户 b 的不含
  - 步骤 3:账户 a 的 `RECONCILED` 只由 `Note{EpochStart}` 之后到达的报告置位;闸门放行后账户 a 的 `Recovery` 仍在,开仓意图得到 `OrderDecision{reject = CLOSE_ONLY}`
  - 恢复后续跑的状态哈希 = 从新段 `StateSnapshot{purpose = Recovery}` 起重放的哈希(INV-6)
- 实现:`tests/recover/mismatch.rs::rc_5_recover_mismatch_scoped_to_venue_account`

| 行 | 相对主例的变化 | 断言 |
|---|---|---|
| RC-5.1 | 被改的是一条 `CmdResult`(不带账户)的 `detail` | 受影响范围 = 全部 (venue, account):`VS`/`VP` × a/b 的非终态槽全为 `Unknown`;每个受影响的 (venue, account) 一条 `Note{RecoverMismatch}`(共 4 条,`v[1]` 相同);a、b 的恢复标志都含 `RECOVER_MISMATCH` |
| RC-5.2 | 尾部未改(重算与 journal 一致) | 无 `Note{RecoverMismatch}`;a、b 的恢复标志都不含 `RECOVER_MISMATCH` |

### RC-6 同一恢复区间连续崩溃 poison_crash_limit 次后第三次启动跳过尾部

- 规则:[毒丸输入隔离](../../crates/hft-sequencer/README.md#毒丸输入隔离)、[崩溃隔离与从快照恢复](../../apps/hft-core/README.md#崩溃隔离与从快照恢复)、[持久元数据](../../crates/hft-journal/README.md#持久元数据engine_epoch-等)、[干净 Drain 重启自动解除 Recovery](../../crates/hft-risk/README.md#干净-drain-重启自动解除-recovery)
- 级别:dst
- 里程碑:M5
- 门槛:⑤
- 前置:W1;`startup.poison_crash_limit = 2`;`keyframe_period_ms = 60000`;实例 1、2 为 probe-taker
- 故障:t = 10 s 第一次崩溃(随种子落在任意 Input 中途),恢复区间 R = (最近快照 seq, 截断后末条 seq];第 1、2 次重启各在重放 R 的同一 Input 中途 `CrashFault`(`repeat = 2`);第 3 次重启不注入故障
- 种子:标准集(§2.4)
- 断言:
  - 第 1 次重启前 `boot_guard = {R, attempts = 1}` 已 `write_sync`;第 2 次重启前 `attempts = 2`
  - 第 3 次启动:`RecoverPlan{start = Latest, replay_tail = false}`,`flags` 含 `POISON` 与 `TAIL_LOST`;`Note{TailSkipped}`(P0,`v[0..2]` = R 的 seq 区间,`v[2]` = 1 即 Poison)一条;R 内的 Input 一条也不被应用;新段从该快照起步
  - 第 3 次启动后全部非终态槽为 `Unknown`;a、b 的 `RECONCILED` 只由 `EpochStart` 之后的报告置位;闸门放行后 `Recovery` 不自动解除
  - 第 3 次启动步 5 成功后 `boot_guard.attempts = 0`
  - 每步 INV-2、INV-6、INV-7
- 实现:`crates/hft-testkit/src/scenarios/recover.rs::rc_6_poison_tail_skipped_on_third_boot`

| 行 | 相对主例的变化 | 断言 |
|---|---|---|
| RC-6.1 | 只有第 1 次重启在重放 R 时崩溃 | 第 2 次启动照常重放 R(`attempts = 2` 写入后开始),成功后 `attempts = 0`;无 `Note{TailSkipped}`;flags 不含 `POISON` |
| RC-6.2 | 运行期层:同一 `ReconcileReport` 体在两次运行中各使处理中途崩溃一次(两次都是被截的半个事件),之后该体再次到达 | 第 3 次启动照常重放尾部;`RecoverPlan.quarantine` 含该 `InputHash`,`flags` 含 `POISON`;该体再到达时不交 `process`、不写成 Input,只写 Meta `Note{InputQuarantined}`(P0,`v[1] = 2`);闸门放行后 `Recovery` 不自动解除 |

### RC-7 新版本崩溃后回滚旧版本从旧版本写的 Drain 快照起步,epoch 单调且对账差异逐项处置后才放行

- 规则:[版本变化与回滚](../../crates/hft-sequencer/README.md#版本变化与回滚)、[崩溃隔离与从快照恢复](../../apps/hft-core/README.md#崩溃隔离与从快照恢复)、[段滚动与引导块](../subsystems/journal-format.md#段滚动与引导块)、[对账闸门与 HARD 解除的严格条件](../../crates/hft-risk/README.md#对账闸门与-hard-解除的严格条件)、[人工干预的采纳](../../crates/hft-ledger/README.md#人工干预的采纳adoptexchange)、[启动闸门与严格放行](../../apps/reconciler/README.md#启动闸门与严格放行)
- 级别:integ
- 里程碑:M5
- 门槛:⑤
- 前置:W1;构建 A(旧)与 B(新)的 `build_git` 不同,实例 1 的 `StrategyDesc.version` A = 3(接受 2、3)、B = 4(接受 3、4);`VS` 的 `VenueCaps.flags` 含 `BATCH_CANCEL`;`DurableMeta.engine_epoch` 初值 10
- 步骤:
  1. 构建 A 运行 5 s 后 `Drain{cancel_resting = true}`,得到 Drain 快照 D_A(epoch 11)
  2. 构建 B 冷启动(epoch 12)、运行 60 s:写出关键帧 K_B1、K_B2(`desc_version = 4`);账户 a 在 `P1` 上卖出成交 0.01 BTC(手续费 0.24 USDT);在 `VS` 挂一张 GTC 买 0.01 BTC @ 59,000.00 未成交
  3. 构建 B 崩溃
  4. 构建 A 以 `RecoverPlan{start = NewestReadable, replay_tail = false, flags = FROM_SNAPSHOT | TAIL_LOST}` 启动(`--recover-from-snapshot latest` 的计划)
  5. 闸门输入齐备,唯一例外是 a 的首组报告:`VP` 组 G1 含 `Position{P1, engine_qty = 0, exch_qty = −0.01}` 与 `Balance{USDT, engine_qty = 50,000.00, exch_qty = 49,999.76}`;`VS` 组 G2 含 `OpenOrder{slot = 0, inst = S1, exch_qty = 0.01}`
  6. `Resume{Recovery, account = a}`
  7. `AdoptExchange{a, VP, P1, report_id = G1, Position, group = GROUP_NONE}`,`AdoptExchange{a, VP, USDT, report_id = G1, Balance}`
  8. `CancelAll{a, VS}`;之后 `VS` 下一组报告 G3 全部 `Ignore`
  9. `Resume{Recovery, account = a}`
- 期望:
  - 步骤 4:起步快照为 D_A(K_B1、K_B2 的 `desc_version = 4` 不可解,跳过);尾部不重放;epoch = max(D_A 的 11, DurableMeta 的 12) + 1 = 13,在 `EngineClock` 之前可由 `view().engine_epoch()` 读到;新段段头 `roll_reason = RecoverFromSnapshot`,引导块含 `Note{RecoverFromSnapshot}`(`v[0]` = D_A 的 seq、`v[1..3]` = 跳过的 seq 区间);新段中有 `Note{TailSkipped}`(P0,`v[2]` = 3 即 Operator)一条;`EngineClock.source = Snapshot`,`base_engine_ns` = D_A 的 `SnapshotHdr.engine_ns`
  - 步骤 4 之后:`replayer verify` 从新段引导块起步逐字节通过
  - 步骤 6:`CmdResult{PreconditionFailed, detail = GateNotReleased}`
  - 步骤 7:两条 `CmdResult{Applied}`,ledger 写两条 `AdjustDelta`(`Position` qty −0.01、`Balance` qty −0.24)与两条 `Note{Adjusted}`;账户级无 group 的 `P1` 持仓为 −0.01,g1 的 LegTracker 不变;`VP` 的 `RECONCILED` 置位
  - 步骤 8:出环一条 `GwCmd{CancelAll}`(a、`VS`);G3 到达后 `VS` 的 `RECONCILED` 置位,闸门放行;`Recovery` 仍在(`FROM_SNAPSHOT`)
  - 步骤 9:`CmdResult{Applied}`,`CapCleared{Recovery}`
- 实现:`tests/recover/rollback.rs::rc_7_rollback_from_old_drain_snapshot`
- 备注:CLI 参数到 `RecoverPlan` 的映射、退出码 4 与 `NoDecodableSnapshot` 归 apps/hft-core 的 `TESTS.md`

### RC-8 对账不通过时保持 Recovery,只有最新一组全部 Ignore 或逐项处置后闸门才放行

- 规则:[对账闸门与 HARD 解除的严格条件](../../crates/hft-risk/README.md#对账闸门与-hard-解除的严格条件)、[启动闸门](../../crates/hft-risk/README.md#启动闸门)、[对账分级](../../crates/hft-risk/README.md#对账分级全系统唯一一次)、[ReconcileReport](../../apps/reconciler/README.md#reconcilereport)、[人工干预的采纳](../../crates/hft-ledger/README.md#人工干预的采纳adoptexchange)
- 级别:integ
- 里程碑:M5
- 门槛:⑤
- 前置:W1;账户 a 崩溃后重启(非干净);a 的闸门输入除 `RECONCILED` 外齐备;`recon_gate_items = 64`;BTC 的 `AssetMeta.dust` = 0.00001
- 步骤:
  1. `VP` 组 R1:`Summary` + `Funding{inst = P1, exch_amount = −3.00, engine_amount = 0}`,其余条目全部 `Ignore`
  2. `VS` 组 R2:`Balance{BTC, engine_qty = 0.01000, exch_qty = 0.01200}`
  3. `Resume{Recovery, account = a}`
  4. `AdoptExchange{a, VS, BTC, report_id = R2, Balance, group = GROUP_NONE}`
  5. `Resume{Recovery, account = a}`
- 期望:
  - 步骤 1:`VP` 的 `RECONCILED` 置位(`Funding` 条目不参与)
  - 步骤 2:`VS` 的 `RECONCILED` 不置位,`released = false`
  - 步骤 3:`CmdResult{PreconditionFailed, detail = GateNotReleased}`,`Recovery` 仍在
  - 步骤 4:`CmdResult{Applied}`,`AdjustDelta{Balance, qty = +0.002}`;`VS` 的 `RECONCILED` 置位;`Note{StartupGate}` 的 `v[3] = 1`
  - 步骤 5:`CmdResult{Applied}`,`CapCleared{Recovery}`
- 实现:`tests/recover/recon_gate.rs::rc_8_failed_recon_holds_recovery`

| 行 | 相对主例的变化 | 断言 |
|---|---|---|
| RC-8.1 | R2 全部 `Ignore` 但 `Summary.sflags` 含 `TRUNCATED` | `VS` 的 `RECONCILED` 不置位;下一组无 `TRUNCATED` 且全部 `Ignore` 后置位 |
| RC-8.2 | R2 全部 `Ignore` 但 `skipped_inflight = 1` | 同上,不置位 |
| RC-8.3 | R2 全部 `Ignore` 但 `Summary.sflags` 含 `USERSTREAM_DOWN` | 同上,不置位 |
| RC-8.4 | 步骤 4 之前到达新组 R3(仍含同一条 `Balance{BTC}` 差异);步骤 4 引用 R2 | 以 R3 为准:步骤 4 之后 `VS` 的 `RECONCILED` 仍不置位;下一组 R4 全部 `Ignore` 后置位 |
| RC-8.5 | R2 改为只含 `Funding{inst = INST_NONE}` 钱包条目,其余 `Ignore` | `VS` 的 `RECONCILED` 置位 |

### RC-9 干净 Drain 重启在闸门放行时自动解除 Recovery,不产生 resolve_broken

- 规则:[干净 Drain 重启自动解除 Recovery](../../crates/hft-risk/README.md#干净-drain-重启自动解除-recovery)、[Drain](../../crates/hft-sequencer/README.md#drain)、[HARD 解除时的 Broken 判定](../../crates/hft-exec/README.md#hard-解除时的-broken-判定resolve_broken)、[命令路径](../../crates/hft-sequencer/README.md#命令路径)、[启动顺序](../../apps/hft-core/README.md#启动顺序)
- 级别:integ
- 里程碑:M5
- 门槛:⑤
- 前置:W1;同一构建;`auto_resume_after_clean_drain = true`;`dust_notional = 10 USDT`,`fix_tolerance_default = 0.15`;账户 a 的 g1 持有 `S1` +0.01000、`P1` −0.010 BTC(无裸量)
- 步骤:
  1. `Drain{cancel_resting = true}` 以 `Done` 结束;重启(尾部为空)
  2. 闸门输入齐备
  3. 实例 1 设 g1 目标 `S1` = +0.02、`P1` = −0.02 BTC
- 期望:
  - 步骤 1:恢复标志为 `CLEAN_DRAIN`,不含 `TAIL_LOST`、`RECOVER_MISMATCH`、`POISON`、`FROM_SNAPSHOT`;无 Unknown 槽
  - 步骤 2:闸门放行的同一 `apply` 删除 `Recovery`(`CapCleared{Recovery}`);从重启到场景结束无 `Broken`/`HedgeFailure` 的 `CapSet`
  - 步骤 3:增仓切片出环(`OrderDecision{New, reject = 0}`)
- 实现:`tests/recover/clean_drain.rs::rc_9_clean_drain_auto_resumes`

| 行 | 相对主例的变化 | 断言 |
|---|---|---|
| RC-9.1 | g1 带容差内残余:`S1` +0.01000、`P1` −0.009 BTC(裸量 0.001 BTC × 60,000 = 60 USDT > `dust_notional`,裸量比例 0.1 ≤ 0.15) | `Recovery` 照常自动解除;无 `Broken`、无 `HedgeFailure`;`VS`、`VP` 放行后执行层不发 `HedgeFix`(`ExecDecision` 中无 `HedgeFix`) |
| RC-9.2 | 重启前存在停机标记 `stop-marker.json`(`v = 1`、`scope = "*"`) | `EngineClock` 之后第一条 Input 为 `ControlCmd{Kill{Global, hard = false, flatten = false}}`(issuer `cli:systemd`);`Recovery` 照常自动解除;Kill cap 仍在,有效模式为 `CloseOnly`,步骤 3 的增仓切片不发(无 `OrderDecision{New}`);`Resume{Kill}` 之后出环 |
| RC-9.3 | `auto_resume_after_clean_drain = false` | 闸门放行后 `Recovery` 仍在;`Resume{Recovery}` 返回 `Applied` 后解除 |
| RC-9.4 | Drain 快照之后 journal 中还有 1 条 Input(`Timer{Sys, ConflateReport}`) | 恢复标志不含 `CLEAN_DRAIN`;闸门放行后 `Recovery` 仍在 |
| RC-9.5 | Drain 以 `DrainTimeout` 结束,快照中有一条 `Hedging` 链接 | 恢复标志不含 `CLEAN_DRAIN`;闸门放行后 `Recovery` 仍在;该链接为 `RESUME_HOLD` |

### RC-10 长停机后冷启动不在恢复瞬间判 Broken,链接在第二腿 venue 的对冲闸门子集满足后续接一次

- 规则:[恢复:停机时间不计与闸门续接](../../crates/hft-oms/README.md#恢复停机时间不计与闸门续接)、[OnFill](../../crates/hft-oms/README.md#onfill)、[启动闸门](../../crates/hft-risk/README.md#启动闸门)、[冷启动恢复(RecoverDriver)](../../crates/hft-sequencer/README.md#冷启动恢复recoverdriver)
- 级别:integ
- 里程碑:M5
- 门槛:⑤
- 前置:W1;实例 1 为 probe-taker;`hedge_deadline_ms = 3000`,`hedge_retry_min_ms = 100`,`hedge_retry_max = 3`,`resume_hold_max_ms = 30000`,`P1` 所属价格带类别 `rr_max_cross = 0.005`
- 步骤:
  1. t = 0:g1 链接 L 的第一腿 `S1` 买 0.01 BTC 全部成交;第二腿 `P1` 卖 0.01 IOC 发出,t = 1.15 s 零成交 `Expired`;HedgeRetry 登记在 t = 1.25 s
  2. t = 1.2 s 的 Input 处理完后崩溃(重试单尚未发出);停机 30 min(> `hedge_deadline_ms`);重启
  3. 重启后引擎时间 +4.0 s:`VP` 的 `ConnState{Order, Up}`、`ConnState{UserStream, Up}`、UID 相符的 `AccountConfigReport`、`Timer{Sys, KillSelfTestOk}{a}` 到齐(`VP` 的热备与 a 的对账报告尚未到)
  4. 续接单 IOC 全部成交 0.01 BTC
- 期望:
  - 步骤 2 之后:L 为 `Hedging` 且带 `RESUME_HOLD`,`held_ms = 1200`;从重启到步骤 3 无 `Note{OmsHedgeBroken}`、无 `Broken` 的 `CapSet`;旧 HedgeRetry 定时器到期时无 `HedgeRequest`
  - 步骤 3 的最后一条 Input 内:`oms.on_hedge_gate_open(a, VP)`,出环 `OrderDecision{New, link_role = SecondRetry, inst = P1, side = Sell, qty = 0.01}`(`oflags` 含 `RISK_REDUCING`、不含 `REDUCE_ONLY`);`Note{OmsResumeRetry}`(P1,`v = [L, 1200, 0.01, 0.01]`);L 清 `RESUME_HOLD`,`first_fill_tsc` = 当前 − 1200 ms
  - 步骤 4:L → `Done`;全程无 `Broken`
- 实现:`tests/recover/long_outage.rs::rc_10_long_outage_resumes_link_once`

| 行 | 相对主例的变化 | 断言 |
|---|---|---|
| RC-10.1 | 步骤 4 改为续接单零成交,随后 `P1` 盘口变为 59,000.0 / 61,000.0,此后每次 HedgeRetry 都以 `BeyondCross` 门控 | 门控不计次;续接后引擎时间 1 700 ms 时 L 仍为 `Hedging`,2 000 ms 时已 `Broken{Deadline}`(停机与续接前的等待不计,剩余 3000 − 1200 = 1800 ms),同时 `Note{OmsHedgeBroken}`(P0)、`Broken{a, g1}` 与 `HedgeFailure{a}` 的 `CapSet` |
| RC-10.2 | 步骤 3 的输入一直不到 | 恢复后引擎时间 29 s 时无下列输出;31 s 时已有:L `Broken{ResumeTimeout}`,`Note{OmsResumeTimeout}`(P0,`v[2]` = 1200,`v[4]` = 1 即 GATE,`v[5]` = 0.01 BTC 的 raw 值) |

### RC-11 跨重启的计时按停机不计续算,同开机、跨开机与换机三种情形结果相同

- 规则:[定时器](../../crates/hft-sequencer/README.md#定时器)、[引擎时间](../../crates/hft-sequencer/README.md#引擎时间engine_ns)、[快照与恢复](../../crates/hft-exec/README.md#快照与恢复)、[Broken 处置](../../crates/hft-exec/README.md#broken-处置)、[保证金分档](../../crates/hft-risk/README.md#保证金分档marginstress)、[令牌桶](../../crates/hft-risk/README.md#令牌桶)
- 级别:integ
- 里程碑:M5
- 门槛:⑤
- 前置:W1;上次运行 `tsc_hz_milli` = 2,500,000,000,000;`deleverage_deadline_ms = 30000`,`liq_dist_reduce = 0.15`,`liq_dist_force = 0.10`,`broken_auto_fix_ms = 120000`;崩溃前状态:实例 1 的一个策略定时器在崩溃时还剩 400 ms(截止引擎时间 E_d);账户 a 处于 MarginStress L2 已 12 000 ms(`deadline_ens` 还剩 18 000 ms);账户 b 的 g2 `Broken`(现货腿多出)已 50 000 ms,`VS`、`VP` 都已放行
- 步骤:
  1. 崩溃;停机 20 min;按变体重启(主例:同开机,`tsc_hz_milli` 不变,TSC 继续增长)
  2. 重启后引擎时间 +1.0 s:a 的新 `MarginReport` 与 `P1` 的 `MarkPrice` 到达,`liq_dist` = 0.13
  3. 重启后引擎时间 +1.5 s:b 的 `VP` 满足 `hedge_gate_ok` 且 `RECONCILED` 置位(`on_venue_gate_open(b, VP)`);+2.0 s:b 的 `VS` 同样放行(`on_venue_gate_open(b, VS)`,该 cell 两腿 venue 中最后放行的一个)
- 期望:
  - 策略定时器:恢复后引擎时间前进 399 ms 时未投递,前进 401 ms 时已投递,投递时 `now.engine_ns ≥ E_d`
  - MarginStress:恢复后引擎时间 17 999 ms 时仍为 L2,18 001 ms 时为 L3(`Note{MarginLevel}` level 3,`DEADLINE_L3`),同一 Input 输出第一步 `ForceDeleverage{a, VP, frac = force_step}`
  - Broken:自动有界平仓不按剩余 70 000 ms 开始;`VS` 放行之后引擎时间 119 999 ms 时尚未开始,120 001 ms 时已写 `ExecDecision{AutoFix, detail = AutoFix/Start}`
  - 令牌桶:重启后第一条 Input 时 (`VS`, a) 的 `tokens_10s` 等于崩溃前最后读数加上 (该 Input 的 engine_ns − 崩溃前末条的 engine_ns) 对应的补充量,停机 20 min 不补充
  - 新段 `StateSnapshot{purpose = Recovery}`:除已登记的 TSC 字段(节 8 `TimerRow.deadline_tsc`、OMS 与执行层登记的 TSC 字段)外,各节字节在三种情形相同;已登记 TSC 字段换算成相对快照头 `snap_tsc` 的 ns 时长后在三种情形相同(±1 tick)
- 实现:`tests/recover/timers.rs::rc_11_timers_resume_without_downtime`
- 备注:Broken 的计时口径(恢复期不计,多出那一腿所在 venue 放行时重给完整窗口)归 [hft-exec Broken 处置](../../crates/hft-exec/README.md#broken-处置)

| 行 | 相对主例的变化 | 断言 |
|---|---|---|
| RC-11.1 | 跨开机:重启后 TSC 从 1,000,000 起计(小于崩溃时的 `stop_tsc`),`tsc_hz_milli` 不变 | 与主例的全部断言相同;平移后无任何定时器落在 `now` 之后超过其剩余时长 + 1 ms |
| RC-11.2 | 换机:新主机 `tsc_hz_milli` = 3,000,000,000,000 | 与主例的全部断言相同;400 ms 剩余的定时器平移后 `deadline_tsc − now` = 1,200,000,000 tick(±1) |

### RC-12 改参数、换 refdata 或发新版本后重启沿用 Model 检查点,布局或 schema 变化时丢弃

- 规则:[Model 检查点](../../crates/hft-strategy-sdk/README.md#model-检查点)、[模型检查点](../../crates/hft-sequencer/README.md#模型检查点)、[advisor 与 Model](../../crates/hft-sequencer/README.md#advisor-与-model)
- 级别:integ
- 里程碑:M9
- 门槛:⑤
- 前置:W1;实例 1 为 probe-target(带 Model,`LAYOUT_VERSION = 1`、`PATH_VERSION = 1`、`schema.id` = S);`advisor.model_checkpoint_ms = 60000`;参考运行 = 同一 journal 不停机连续运行得到的 Advice 序列
- 步骤:
  1. 运行 130 s(检查点 `last_seq` = c);`SetParams` 把实例 1 的参数 version 2 → 3(非结构字段);再运行 10 s 后 Drain 退出
  2. 以 `build_git` 不同、`LAYOUT_VERSION`、`PATH_VERSION`、`schema.id` 都不变的构建重启
- 期望:
  - 无 `Note{ModelCheckpointDiscarded}`、无 `Note{ModelPathReset}`;advisor 以 c 处生效的参数(version 2)调一次 `model_on_params`,重放到 version 3 的 `SetParams` 时再调一次
  - 追赶结束后产出的 Advice 载荷与参考运行在同一 `basis_seq` 处逐字节相同
- 实现:`tests/recover/model_ckpt.rs::rc_12_model_checkpoint_survives_params_and_release`

| 行 | 相对主例的变化 | 断言 |
|---|---|---|
| RC-12.1 | 步骤 1 改为 `ReplaceRefdata`(refdata version + 1,宇宙不变)后 Drain、重启 | 无 `Note{ModelCheckpointDiscarded}`;Advice 与参考运行逐字节相同 |
| RC-12.2 | 新构建的 `LAYOUT_VERSION = 2` | `Note{ModelCheckpointDiscarded}`(P1,`v[0]` = 实例 1,`v[1]` = 3 即 KeyMismatch);Model 以空状态起步 |
| RC-12.3 | 新构建的 `schema.id` ≠ S | 同 RC-12.2(`v[1]` = 3) |
| RC-12.4 | 只有 `PATH_VERSION` 变为 2 | 无 `Note{ModelCheckpointDiscarded}`;`Note{ModelPathReset}`(P2,`v[0]` = 1,`v[1]` = c,`v[2]` = 实例 1 的 `path_settle_ms`,`v[3]` = 1,`v[4]` = 2);原始样本节照常恢复;`basis_seq ≥ recompute_from_seq` 的 Advice 与参考运行逐字节相同 |

## 4. 模式、账户与人工干预

本节是原 testing §4.1 的场景;模型见 [defensive-patterns](../defensive-patterns.md) 与 [control-plane](../subsystems/control-plane.md)。命令写成 `命令{字段}`;"经确认"指先发带 `preview` 的同一命令、再以新 `cmd_id`、`confirmed = 1`、`expect_state_version` = 预览返回值重发([版本与确认](../subsystems/control-plane.md#版本与确认))。除另有说明,Trader 的 issuer 为 `cli:ops`,Admin 为 `cli:admin`;场景开始时 cap 集合为空(各账户 `Recovery` 已解除),全部 cell 期望 `Live`。

### MD-1 期望模式改为 Shadow 时有仓位的 cell 暂落 CloseOnly,仓位归零后自动落到 Shadow

- 规则:[模式与上限的统一优先级](../subsystems/control-plane.md#模式与上限的统一优先级)、[命令集](../subsystems/control-plane.md#命令集)、[有效模式](../../crates/hft-risk/README.md#有效模式)、[模式与 cap 的驱动](../../crates/hft-sequencer/README.md#模式与-cap-的驱动)、[运行模式下意图的行为表](../../crates/hft-strategy-sdk/README.md#运行模式下意图的行为表)
- 级别:dst
- 里程碑:M5
- 门槛:④
- 前置:W1;实例 1 为 `probe-taker`;t = 0 时 g1 持仓 `S1` +0.10000 BTC、`P1` −0.100 BTC,无在途,LegTracker 残余 0;`probe-taker` 参数:t = 10 s 以平仓链接平掉 g1(第一腿 `S1` 卖 0.10000,第二腿 `P1` 买 0.100,`HedgePlan.reduce_only = 1`),另自 t = 2 s 起每 1 s 试发一笔 0.001 BTC 的 Open 链接;`conn_liveness_ms = 2000`
- 故障:t = 1 s Trader 发 `SetMode{strategy = 1, group = g1, account = a, mode = Shadow}`(降级,不确认);两腿回报时延由种子在 10–200 ms 内抽取
- 种子:标准集(§2.4)
- 断言:
  - t = 1 s 的 Input 内:`CmdResult{Applied, detail = ShadowDeferred}`;`Note{ShadowDeferred}` 的 `v[2] = 0`;向实例 1 投递 `EventRef::Mode{old = Live, new = CloseOnly}`;`mode.effective(1, a, g1) = CloseOnly{deferred_shadow: true}`
  - 从 t = 2 s 到平仓完成,每笔 Open 链接的 `OrderDecision.reject = CLOSE_ONLY`、无对应出环帧;t = 10 s 的平仓第一腿(Reduce)与成交后的第二腿(Hedge,带 `RISK_REDUCING`)都出环
  - 两腿净量、在途与残余都为 0 的那条 Input 内:`Note{ShadowDeferred}` 的 `v[2] = 1`;`EventRef::Mode{old = CloseOnly, new = Shadow}`;此后策略意图的决策记录带 `SHADOW` 位,出环帧中无实例 1 的订单
  - 全程该 cell 的期望值为 `Shadow`,cap 集合为空;INV-11、INV-13 每步成立
- 实现:`crates/hft-testkit/src/scenarios/modes.rs::md_1_shadow_deferred_until_flat`

| 行 | 相对主例的变化 | 断言 |
|---|---|---|
| MD-1.1 | t = 0 时 g1 已平(两腿 0、无在途、残余 0) | t = 1 s:`CmdResult{Applied}`、`detail = 0`;`EventRef::Mode{Live → Shadow}`;journal 中无 `Note{ShadowDeferred}` |
| MD-1.2 | 平仓完成前 t = 5 s 经确认发 `SetMode{1, g1, a, Live}` | t = 5 s:`CmdResult{Applied}`;`EventRef::Mode{CloseOnly → Live}`;此后无 `v[2] = 1` 的 `Note{ShadowDeferred}` |

### MD-2 Resume 只删完全匹配的一条 cap,SetMode 不清任何 cap

- 规则:[模式与上限的统一优先级](../subsystems/control-plane.md#模式与上限的统一优先级)、[有效模式](../../crates/hft-risk/README.md#有效模式)、[cap 记录](../../crates/hft-risk/README.md#cap-记录)、[检查链](../../crates/hft-risk/README.md#检查链按廉价--昂贵短路适用范围以本表为唯一定义)、[对账分级](../../crates/hft-risk/README.md#对账分级全系统唯一一次)
- 级别:dst
- 里程碑:M2
- 门槛:④
- 前置:W1;实例 1 为 `probe-taker`,自 t = 0 起每 1 s 试发一笔 0.001 BTC 的 Open 链接(第一腿 `S1`,链接腿 `P1`);场景开始于 UTC 日界后 1 h,`equity_sod(a) = 150,000 USDT`;`RiskParams.accounts[a]`:`daily_loss_abs = 50 USDT`、`daily_loss_kill = 0.05`(额度 = min(50, 7,500) = 50 USDT)、`daily_loss_max_resumes = 1`、`recon_closeonly_notional = 100 USDT`;t = 0 时 g1 持仓 `S1` +0.10000 BTC、`P1` −0.100 BTC
- 故障:
  1. t = 10 s `MarketActor` 改变 `S1` 报价(`P1` 不变),使 ledger 的 `day_pnl(a)` = −60.00 USDT(价格按 ledger 的估值规则反推)→ `DailyLoss{a}`
  2. t = 20 s 与 t = 80 s `ReconActor` 对 (a, VP) 的两组报告各含同一条 `P1` 仓位差 +0.010 BTC(名义约 600 USDT)→ 第二组后 `Recon{a, VP}`
  3. t = 30 s Trader 发 `SetMode{1, g1, a, CloseOnly}`;t = 100 s 经确认发 `Resume{DailyLoss, a}`;t = 110 s 经确认发 `SetMode{1, g1, a, Live}`
  4. t = 140 s、t = 200 s 两组 (a, VP) 报告全部 Ignore、`skipped_inflight = 0`;t = 150 s 与 t = 210 s 各经确认发 `Resume{Recon, a, VP}`
- 种子:标准集(§2.4)
- 断言:
  - t = 10 s:`CapSet{DailyLoss}`,`params[0]` = −50 USDT;`EventRef::Mode{Live → CloseOnly}`
  - t = 80 s:`CapSet{Recon}`,作用域 (a, VP);cell 有效模式不变(venue 级 cap 不进 cell 计算)
  - t = 100 s:`CmdResult{Applied}`;之后 cap 集合 = {`Recon{a, VP}`};期望值仍为 `CloseOnly`,有效模式 `CloseOnly`
  - t = 110 s:`CmdResult{Applied}`,cap 集合仍为 {`Recon{a, VP}`};`EventRef::Mode{CloseOnly → Live}`;t ∈ [110 s, 210 s) 的每笔 Open 链接 `OrderDecision.reject = RECON_DEVIATION`(#3 查链接腿 venue VP 的 `venue_level`),无对应出环帧
  - t = 150 s(只有一轮 Ignore):`CmdResult{PreconditionFailed, detail = ReconNotClean}`,cap 集合与 `ctl_state_version` 不变
  - t = 210 s:`CmdResult{Applied}`;cap 集合为空;t = 211 s 的 Open 链接 `reject = 0`、第一腿出环
  - INV-11、INV-13 每步成立
- 实现:`crates/hft-sequencer/tests/scenarios_risk.rs::md_2_resume_removes_exact_cap_only`
- 备注:MD-2.1–2.4 在同一函数;实现方式:以真实 sequencer 的确定性步骤序列在 hft-sequencer B0 夹具上实现,映射 A≙`VS`、B≙`VP`、a1≙`S1`、b1≙`P1`、账户 1/2≙`a`/`b`,价格与阈值取 B0(见该文件头);W1 种子化 DST 版本待测试策略能发出 S1 买入第一腿后补

| 行 | 相对主例的变化 | 断言 |
|---|---|---|
| MD-2.1 | 第 2 步改为 t = 80 s Trader 发 `Kill{Account(a)}`(账户级,进 cell 计算);第 4 步改为 t = 210 s 经确认发 `Resume{Kill, Account(a)}` | t = 110 s:`Applied`,有效模式仍为 `CloseOnly`、无 `EventRef::Mode`;Open 的 `reject = KILLED`;t = 210 s 后 `EventRef::Mode{CloseOnly → Live}`,Open 出环 |
| MD-2.2 | t = 120 s 再次经确认发 `Resume{DailyLoss, a}` | `CmdResult{PreconditionFailed, detail = NoMatchingCap}`;`ctl_state_version` 不变 |
| MD-2.3 | t = 210 s 改发 `Resume{Recon, a, VS}`(作用域不符) | `PreconditionFailed{NoMatchingCap}`;`Recon{a, VP}` 仍在 |
| MD-2.4 | t = 210 s 改发 `Resume{MarginStress, a}` | `CmdResult{InvalidArgs, detail = NotResumable}` |

### MD-3 日亏 Resume 把触发线重设为当前亏损减额度,日切不清锁存

- 规则:[当日亏损(锁存)](../../crates/hft-risk/README.md#当日亏损锁存)、[cap 记录](../../crates/hft-risk/README.md#cap-记录)、[环境决定写成 Input](../../crates/hft-sequencer/README.md#环境决定写成-input)
- 级别:dst
- 里程碑:M2
- 门槛:④
- 前置:同 MD-2 的账户参数(额度 50 USDT,`daily_loss_max_resumes = 1`);手续费与资金费为 0(`VenueCaps` 费率 0);g1 持仓 `S1` +0.10000 BTC、`P1` −0.100 BTC;`day_pnl(a)` 只由故障表中的 `S1` 报价变化改变(价格按 ledger 的估值规则反推),`P1` 不变;时钟可信
- 故障:t = 10 s `day_pnl` = −60.00 USDT;t = 100 s 经确认发 `Resume{DailyLoss, a}`;t = 150 s `day_pnl` = −109.99 USDT;t = 200 s `day_pnl` = −110.01 USDT;t = 300 s 经确认发 `Resume{DailyLoss, a}`;下一个 UTC 日界 `Timer{DayRoll}`;日界后 10 s 经确认发 `Resume{DailyLoss, a}`
- 种子:标准集(§2.4)
- 断言:
  - t = 10 s:`CapSet{DailyLoss}`,`params[0]` = −50.000000 USDT(raw −50,000,000,指数 −6)
  - t = 100 s:`CmdResult{Applied}`;节 17 `RiskAcctRow.loss_trigger` = −110.000000 USDT、`resumes_today = 1`
  - t = 150 s:无 `CapSet{DailyLoss}`(−109.99 > −110)
  - t = 200 s:`CapSet{DailyLoss}`,`params[0]` = −110 USDT
  - t = 300 s:`CmdResult{PreconditionFailed, detail = ResumeLimit}`,cap 仍在
  - 日界的 `Timer{DayRoll}`:`day_pnl` 基线归零、`resumes_today = 0`、cap 集合仍含 `DailyLoss{a}`、有效模式仍为 `CloseOnly`
  - 日界后 10 s:`CmdResult{Applied}`,cap 删除;新触发线 = 0 − 50 = −50 USDT
- 实现:`crates/hft-sequencer/tests/scenarios_risk.rs::md_3_daily_loss_resume_resets_trigger`
- 备注:MD-3.1、MD-3.2 在同一函数(当日 PnL 的 1e-6 USDT 精度由一笔 USDT 手续费定位);实现方式:以真实 sequencer 的确定性步骤序列在 hft-sequencer B0 夹具上实现,映射 A≙`VS`、B≙`VP`、a1≙`S1`、b1≙`P1`、账户 1/2≙`a`/`b`,价格与阈值取 B0(见该文件头);W1 种子化 DST 版本待测试策略能发出 S1 买入第一腿后补

| 行 | 相对主例的变化 | 断言 |
|---|---|---|
| MD-3.1 | t = 10 s 的 `day_pnl` 恰为 −50.000000 USDT | 无 `CapSet{DailyLoss}`(触发条件为严格小于) |
| MD-3.2 | t = 10 s 的 `day_pnl` 为 −50.000001 USDT(低 1 raw) | `CapSet{DailyLoss}`,`params[0]` = −50 USDT |

### MD-4 时钟不可信与 Halted 只拦 Open,平仓、减仓与对冲照常放行

- 规则:[时钟](../../crates/hft-risk/README.md#时钟)、[cap 记录](../../crates/hft-risk/README.md#cap-记录)、[检查链](../../crates/hft-risk/README.md#检查链按廉价--昂贵短路适用范围以本表为唯一定义)、[模式与上限的统一优先级](../subsystems/control-plane.md#模式与上限的统一优先级)、[保证金危急的默认减仓](../../crates/hft-exec/README.md#保证金危急的默认减仓a5与-engine_flatten)
- 级别:dst
- 里程碑:M2
- 门槛:④
- 前置:W1;实例 1 为 `probe-taker`;g1 持仓 `S1` +0.10000 BTC、`P1` −0.100 BTC;`RiskParams.accounts[a].margin`:`liq_dist_block_open = 0.25`、`liq_dist_reduce = 0.15`、`liq_dist_force = 0.10`、`liq_dist_hyst = 0.02`、`force_step = 0.20`、`force_step_interval_ms = 5000`;`probe-taker` 在 t = 6 s 发一笔 0.001 BTC 的 Open 链接,t = 7 s 发平仓链接(第一腿 `S1` 卖 0.05000,第二腿 `P1` 买 0.050,`reduce_only = 1`)
- 故障:t = 5 s `HkActor` 注入不可信的 `ClockStatus`,驱动写 `Timer{ClockBad}`;t = 8 s `margin_path` 使 (a, VP) 的 `liq_dist` 降到 0.09;t = 15 s `liq_dist` 回到 0.40(MarginStress 回到 L0);t = 20 s 驱动写 `Timer{ClockOk}`;t = 21 s `probe-taker` 再发一笔 Open 链接
- 种子:标准集(§2.4)
- 断言:
  - t = 5 s:cap 集合含 `ClockBad`(全局,`cflags` 含 `CONDITIONAL`),`CapSet` 注记;`ctl_state_version` +1
  - t = 6 s 的 Open:`OrderDecision.reject = CLOCK`,无出环
  - t = 7 s 的平仓第一腿(Reduce)出环;其成交后的第二腿(Hedge,`RISK_REDUCING`)在成交的同一 Input 内出环
  - t ∈ [8 s, 15 s) 每一步 `ForceDeleverage` 的引擎单(Flatten,`origin = Engine`,永续腿先)出环,`reject = 0`
  - t = 20 s:`CapCleared{ClockBad}`;t = 21 s 的 Open `reject = 0`
  - INV-12 每步成立
- 实现:`crates/hft-sequencer/tests/scenarios_risk.rs::md_4_clockbad_halted_block_open_only`
- 备注:MD-4.2 在同一函数,MD-4.1 见 `md_4_1_…`;实现方式:以真实 sequencer 的确定性步骤序列在 hft-sequencer B0 夹具上实现,映射 A≙`VS`、B≙`VP`、a1≙`S1`、b1≙`P1`、账户 1/2≙`a`/`b`,价格与阈值取 B0(见该文件头);W1 种子化 DST 版本待测试策略能发出 S1 买入第一腿后补;策略侧 `Throttle` 对 Flatten 与硬动作绕过 `cooldown`、`in_flight` 的放行表归 [hft-strat-lib Throttle](../../crates/hft-strat-lib/README.md#throttle),由其 `TESTS.md` 覆盖;本场景只断言引擎侧的放行

| 行 | 相对主例的变化 | 断言 |
|---|---|---|
| MD-4.1 | 无时钟故障;t = 6 s 起 `MockVenue` 对实例 1 的 Open 第一腿连拒 3 次(交易所 `Reject`),`probe-taker` 的 `max_consec_fail = 3` 使它调 `Ctx::request_halt(g1)`;t = 12 s 策略发 Flatten(`S1` 卖 0.10000) | 第 3 次拒单的 Input 内 `CapSet{Halted}`,作用域 (1, a, g1);`EventRef::Mode{Live → CloseOnly}`;之后 Open `reject = CLOSE_ONLY`;t = 12 s 的 Flatten 出环;经确认 `Resume{Halted, (1, a, g1)}` 后 `CapCleared{Halted}`、`Mode{CloseOnly → Live}` |
| MD-4.2 | t = 10 s 经确认发 `Resume{ClockBad}` | `CmdResult{InvalidArgs, detail = NotResumable}`;`ClockBad` 仍在 |

### MD-5 一个账户的限速风暴不推迟另一个账户的对冲,风险递减单优先出环

- 规则:[令牌桶](../../crates/hft-risk/README.md#令牌桶)、[输出扇出](../../crates/hft-oms/README.md#输出扇出)、[输出扇出](../../crates/hft-sequencer/README.md#输出扇出)、[检查链](../../crates/hft-risk/README.md#检查链按廉价--昂贵短路适用范围以本表为唯一定义)
- 级别:dst
- 里程碑:M9
- 门槛:④
- 前置:W1;实例 1(账户 a)为 `probe-target`,目标使它在 `P1` 上连续发 Open 切片(每片 0.001 BTC,`cooldown_ms = 0`);实例 2(账户 b)为 `probe-taker`,每 2 s 发一笔 0.001 BTC 的 PairOnFill 开仓链接(第一腿 `S1` 买,第二腿 `P1` 卖);OMS `max_out_per_event = 16`、`out_quota[(VP, a)] = out_quota[(VP, b)] = 64`;VP 的 `RateLimitTable` 10 s 窗口 300、分钟窗口 1,200,`rate_headroom = 0.10`(容量 270 / 1,080);(VP, a)、(VP, b) 的 `rr_reserve = 0.20`、`backoff_base_ms = 500`、`backoff_max_ms = 8000`
- 故障:t ∈ [0, 60 s) `MockVenue` 对账户 a 在 VP 上的每个 Place 回限速错误(`RateLimitHit`);账户 b 不受影响
- 种子:标准集(§2.4)
- 断言:
  - 账户 a 每次 `RateLimitHit` 的 Input 内:(VP, a) 两个窗口桶清零;第 k 次连续命中的退避时长 = min(500 × 2^k, 8000) ms;退避期内 a 的 Open 意图 `reject = RATE_BACKOFF`
  - 节 17 中 (VP, b) 的 `RiskBucketRow` 只随 b 自己的 `commit` 扣减,a 的命中不改变它
  - 账户 b 每笔第一腿成交:第二腿 `OrderDecision{action = New, role = Second}` 在该成交的同一 Input 内产生,其 `GwCmd` 在该 Input 的出环批次内交出(不经 `Timer{OmsContinue}`),即使此刻 (VP, a) 的扇出队列非空
  - (VP, a) 的扇出队列内,带 `RISK_REDUCING` 的决策(撤单、减仓)排在 Open 决策之前交出
  - INV-3、INV-8 每步成立
- 实现:`crates/hft-testkit/src/scenarios/modes.rs::md_5_rate_storm_isolated_per_account`
- 备注:账户 b 的 `leg_gap` p99 相对无干扰时的涨幅由基准 `leg-link/noisy-neighbor` 在参考机上判定([benchmarks](../../benchmarks/README.md#路径基准与门槛))

| 行 | 相对主例的变化 | 断言 |
|---|---|---|
| MD-5.1 | 无限速错误;账户 a 的 Open 切片把 (VP, a) 的 10 s 窗口令牌耗到 54(= 270 × 0.20);随后 a 发一笔 `P1` 减仓(Reduce,数量 ≤ \|仓位\|) | 剩余 54 时 a 的 Open `reject = RATE`;a 的减仓单 `reject = 0` 并出环;风险递减单可把该桶扣到 0 |
| MD-5.2 | 两次命中相隔 9 s(> `backoff_max_ms` 后才再次命中) | 第二次命中的退避时长回到 500 ms(k = 0) |

### MD-6 命令账户字段不一致被拒,升级与中性命令必须显式写账户,降级命令缺账户作用于全部账户

- 规则:[账户语义](../subsystems/control-plane.md#账户语义)、[结果码](../subsystems/control-plane.md#结果码)、[gateway 侧的命令校验](../../crates/hft-ctl/README.md#gateway-侧的命令校验)、[命令路径](../../crates/hft-sequencer/README.md#命令路径)
- 级别:table
- 里程碑:M5
- 门槛:④
- 前置:W1;hft-ctl gateway 与 sequencer 同进程(假交易所);请求以 JSON 经 gateway 提交,issuer `cli:ops`(Trader);`VS`、`VP` 的 `VenueCaps.flags` 含 `BATCH_CANCEL`;a、b 在 VP 上各有 2 张 Open 挂单(GTC);全部账户闸门已放行
- 实现:`apps/hft-core/tests/ctl_account.rs::md_6_command_account_semantics`

| 行 | 输入 | 期望 |
|---|---|---|
| MD-6.1 | `Kill{level = Account, scope.account = "b"}`,信封 `account = "a"` | gateway 回 `InvalidArgs{AccountMismatch}`;ctl 环中只有一条 `Note{CtlAudit}`、无 `ControlCmd`;a、b 的 cap 集合不变 |
| MD-6.2 | 同 6.1 的命令绕过 gateway 直接写进 ctl 环(`CmdHdr.account = a`,`CmdScope.account = b`) | `CmdResult{InvalidArgs, detail = AccountMismatch}`;cap 集合与 `ctl_state_version` 不变 |
| MD-6.3 | `SetMode{strategy = 1, group = g1, mode = Live}`,不带 `account` | gateway 回 `InvalidArgs{AccountRequired}`;无 `ControlCmd` |
| MD-6.4 | `HedgeFix{group = g1}`,不带 `account`(中性) | gateway 回 `InvalidArgs{AccountRequired}` |
| MD-6.5 | `SetMode{strategy = *, group = *, mode = CloseOnly}`,不带 `account`(降级) | `CmdResult{Applied}`;a、b 全部 cell 期望值为 `CloseOnly`,各 cell 一条 `EventRef::Mode{Live → CloseOnly}` |
| MD-6.6 | `CancelAll{venue = VP}`,不带 `account` | `CmdResult{Applied}`;a、b 各一条 venue 级 `GwCmd{CancelAll}` 进 `out-VP` |
| MD-6.7 | `InjectWarmup{instance = 1, local_kind = 1, chunk_no = 0, last = 1}`,带 `account = "a"` | gateway 回 `InvalidArgs{AccountNotAllowed}` |

### MD-7 预览之后状态版本变化使一般命令的确认失效,pipeline 的参数下发不需确认

- 规则:[版本与确认](../subsystems/control-plane.md#版本与确认)、[类别](../subsystems/control-plane.md#类别)、[命令路径](../../crates/hft-sequencer/README.md#命令路径)、[权限表](../../crates/hft-ctl-proto/README.md#权限表)
- 级别:integ
- 里程碑:M5
- 门槛:④
- 前置:W1;cell (1, a, g1) 期望 `CloseOnly`;实例 1 的 g1 `open_allowed = 0`;`CtlActor` 直接写 ctl 环(`CmdHdr.role` 与 issuer 按行给出);hft-testkit 测试策略的参数 schema 至少有一个 `FieldDesc.structural = true` 的字段 F_s 与一个非结构字段 F_n
- 步骤:
  1. Trader 发带 `preview` 的 `SetMode{1, g1, a, Live}`
  2. 驱动写 `Timer{ClockBad}`(cap 增加)
  3. Trader 以 `confirmed = 1`、`expect_state_version` = 步骤 1 的返回值发同一命令
  4. Trader 重发预览,再以新返回值确认
- 期望:
  - 步骤 1:`CmdResult{preview}`,带 `ctl_state_version = V` 与"需要确认";ModeTable 与 cap 集合不变
  - 步骤 2 后 `ctl_state_version = V + 1`
  - 步骤 3:`CmdResult{ConfirmInvalid, detail = StateChanged}`;期望值仍为 `CloseOnly`
  - 步骤 4:`CmdResult{Applied}`;期望值为 `Live`;`ctl_state_version = V + 2`
- 实现:`crates/hft-testkit/src/scenarios/modes.rs::md_7_confirm_binds_state_version`

| 行 | 相对主例的变化 | 断言 |
|---|---|---|
| MD-7.1 | 去掉步骤 2 | 步骤 3 返回 `Applied` |
| MD-7.2 | 步骤 3 改为不带 `confirmed` 直接发 | `CmdResult{NeedsConfirm}`;期望值不变 |
| MD-7.3 | issuer `pipeline`(进程主体)发 `SetParams{1, a}`:新增 group g3(`open_allowed = 1`)并把 g1 的 `open_allowed` 由 0 改为 1,不带 `confirmed` | `CmdResult{Applied}`;g3 的 cell 取 `default_mode`;无 `NeedsConfirm` |
| MD-7.4 | Trader(`cli:ops`)发同一 `SetParams`,不带 `confirmed` | `NeedsConfirm`;生效参数版本不变;经确认后 `Applied` |
| MD-7.5 | Trader 发只改 F_n 的 `SetParams`,不带 `confirmed` | `Applied`(中性) |
| MD-7.6 | `pipeline` 发改动 F_s 的 `SetParams`(实例 1 绑定 Live 账户 a) | `NeedsConfirm`;生效参数版本与 F_s 不变 |
| MD-7.7 | Trader 预览 `Kill{Account(a), flatten}` 后发生步骤 2,再以旧版本确认 | `ConfirmInvalid{StateChanged}`(只有 `CmdHdr.role = Viewer` 的 `Kill` 不比对版本,见 MD-11) |

### MD-8 人工参数覆盖项按实例生效,模拟实例上的修改不流到正式实例

- 规则:[参数通道](../../python/pipeline/README.md#参数通道)、[实例配置](../../python/pipeline/README.md#实例配置)、[参数面板与修改](../../python/ops_bot/README.md#参数面板与修改)、[类别](../subsystems/control-plane.md#类别)
- 级别:proc
- 里程碑:M5
- 门槛:④
- 前置:Linux CI;同机运行 `hft-core@prod`(账户 a,Live,`is_prod = false`,`order_env = feed_env = "testnet"`,行情与下单端点指向本机未监听端口,venue 保持 `Down`)与 `hft-core@sim`(账户 `sim-s`),hft-pipeline 与 ops_bot(测试驱动直接调用其命令处理函数,不经 Telegram 网络);pipeline 实例 `pt-live`(core `prod`,账户 a,策略实例 1)与 `pt-sim`(core `sim`,账户 `sim-s`,策略实例 1,同一 `strategy_kind`),校准对 `p1 = {live: pt-live, sim: pt-sim}`;`overrides/` 为空;非结构字段 F_n 当前值 5;pt-live 的 cell 期望 `Live`
- 步骤:
  1. Trader `/params pt-sim set F_n 7`,在预览上确认
  2. pipeline 下一轮
  3. 在 `overrides/pair-p1.toml` 写入一项不带 `scope` 的 F_n = 9,pipeline 下一轮
  4. Trader `/params pt-live set F_n 8`,不点确认;再发一次并确认
- 期望:
  - 步骤 1:只写 `overrides/pt-sim.toml`(该项 `scope = "instance"`,`override_version` +1);预览首行为模拟账户标签、不含"实盘"
  - 步骤 2:sim 实例 journal 中一条 issuer `pipeline` 的 `SetParams`,`CmdResult{Applied}`,F_n = 7;prod 实例生效参数中的 F_n 仍为 5
  - 步骤 3:`pair-p1.toml` 加载失败,该文件的项不进入任何实例的合并结果;两个实例生效参数中的 F_n 不变(prod 5,sim 7)
  - 步骤 4:未确认时 `overrides/pt-live.toml` 不变、无 `SetParams`;确认后写入 pt-live 文件、ops_bot 发 `Annotate{LocalWrite}`,prod 实例的 `SetParams` 以 `pipeline` 主体下发并 `Applied`(不带确认);预览首行标"实盘"
- 实现:`python/pipeline/tests/test_overrides.py::test_md_8_overrides_scoped_per_instance`

| 行 | 相对主例的变化 | 断言 |
|---|---|---|
| MD-8.1 | `pair-p1.toml` 中该项写明 `scope = "pair"` | 下一轮两个实例各一条 `SetParams`,F_n = 9;`/params pair p1 set` 的预览列出两个实例并要求确认 |
| MD-8.2 | pt-live 的 cell 期望 `Live`;Trader `/params pt-live set F_s 3`(结构字段,当前值 2) | pt-live 是 Live 实例(绑定 Live 账户且有期望为 Live 的 cell);`preview_override` 拒绝,提示"结构字段须经 hft-cfg + 运维 CLI";`overrides/pt-live.toml` 不变;无 `SetParams` |
| MD-8.3 | pt-live 的全部 cell 期望 `Shadow`;同 MD-8.2 的命令,在预览上确认 | pt-live 不是 Live 实例;预览不因结构字段被拒、首行不标"实盘";确认后写入 `overrides/pt-live.toml`;下一轮 prod 实例一条 issuer `pipeline` 的 `SetParams`,F_s = 3,`CmdResult{Applied}`,无 `NeedsConfirm` |

### MD-9 account = * 的升级命令在任一目标账户闸门未放行时整体被拒

- 规则:[账户语义](../subsystems/control-plane.md#账户语义)、[启动闸门](../../crates/hft-risk/README.md#启动闸门)、[命令路径](../../crates/hft-sequencer/README.md#命令路径)
- 级别:integ
- 里程碑:M9
- 门槛:④
- 前置:W1;实例 1、2 的全部 cell 期望 `CloseOnly`;账户 a 闸门已放行;账户 b 除 `RECONCILED` 外各位已置位:`ReconActor` 对 (b, VP) 的首组报告含一条未处置的 Warn 仓位差 0.010 BTC
- 步骤:
  1. Trader 经确认发 `SetMode{strategy = *, group = *, account = *, mode = Live}`
  2. `ReconActor` 对 (b, VP) 发一组全部 Ignore 的报告
  3. Trader 经确认再发步骤 1 的命令
- 期望:
  - 步骤 1:确认后 `CmdResult{PreconditionFailed, detail = GateNotReleased}`;a、b 全部 cell 期望值仍为 `CloseOnly`;无 `EventRef::Mode`;`ctl_state_version` 不变
  - 步骤 2:`Note{StartupGate}` 中 b 的 `v[3]`(released)= 1
  - 步骤 3:`CmdResult{Applied}`;a、b 全部 cell 期望 `Live`,每个 cell 一条 `EventRef::Mode{CloseOnly → Live}`
- 实现:`crates/hft-testkit/src/scenarios/modes.rs::md_9_star_upgrade_needs_all_gates`

| 行 | 相对主例的变化 | 断言 |
|---|---|---|
| MD-9.1 | 步骤 1 改为 `account = "a"` | `Applied`;只有 a 的 cell 期望变为 `Live`,b 不变 |

### MD-10 Kill 的 hard 只与 Global 或 Account 组合,账户级 HARD 不影响其他账户,Sim 账户的 Kill 经检查链生效

- 规则:[Kill](../../crates/hft-risk/README.md#kill)、[kill 字](../../crates/hft-risk/README.md#kill-字唯一的跨线程共享可变量)、[命令集](../subsystems/control-plane.md#命令集)、[命令路径](../../crates/hft-sequencer/README.md#命令路径)、[模拟账户](../../crates/hft-sequencer/README.md#模拟账户)
- 级别:table
- 里程碑:M9
- 门槛:④
- 前置:W1(10.7、10.8 用 W1-SIM);a、b 各有一条在途 PairOnFill 链接,第一腿在命令之后 1 s 成交;a、b 在 VP 各挂 2 张 GTC Open 单;`VS`、`VP` 的 `VenueCaps.flags` 含 `BATCH_CANCEL`;命令由 `CtlActor` 以 Trader 身份直接写 ctl 环;降级命令不确认
- 实现:`crates/hft-testkit/src/scenarios/modes.rs::md_10_kill_parameter_combinations`

| 行 | 输入 | 期望 |
|---|---|---|
| MD-10.1 | `Kill{Global, hard}` | `CmdResult{Applied}`;各 gw 的 kill 字置 `GLOBAL \| HARD`;a、b 的第二腿 `OrderDecision.reject = HARD`、不出环;撤单 `open_only = false`,VP 上 a、b 各一条 `GwCmd{CancelAll}` |
| MD-10.2 | `Kill{Account(a), hard}` | 服务 a 的 gw 置 `account(ix_a) \| account_hard(ix_a)`;a 的第二腿 `reject = HARD`;b 的第二腿 `reject = 0` 并在成交的同一 Input 内出环;b 的 Open 出环 |
| MD-10.3 | `Kill{Venue(VP), hard}` | `CmdResult{InvalidArgs, detail = BadCombination}`;cap 集合与 kill 字不变 |
| MD-10.4 | `Kill{Global, flatten}` | `InvalidArgs{BadCombination}` |
| MD-10.5 | `Kill{Account(a), flatten, hard}` | `InvalidArgs{BadCombination}` |
| MD-10.6 | `Kill{Account(a)}`(默认参数) | a 的 Open `reject = KILLED`;a 在 VP 的 2 张 GTC 被撤;a 的第二腿 `reject = 0` 并出环;b 的 cap、挂单与订单不受影响 |
| MD-10.7 | W1-SIM:`Kill{Account(s)}` | s 的 Open `reject = KILLED`(检查链 #3);kill 字不变(Sim 账户不占位);s 的第二腿进 `out-sim` |
| MD-10.8 | W1-SIM:`Kill{Account(s), hard}` | s 的全部 Place `reject = HARD`(检查链 #1);撤单照常进 `out-sim` |

### MD-11 Viewer 的紧急停止绑定预览的 cmd_hash 与 60 s TTL,不比对状态版本

- 规则:[角色](../subsystems/control-plane.md#角色)、[版本与确认](../subsystems/control-plane.md#版本与确认)、[权限表](../../crates/hft-ctl-proto/README.md#权限表)、[命令路径](../../crates/hft-sequencer/README.md#命令路径)、[输出扇出](../../crates/hft-oms/README.md#输出扇出)
- 级别:integ
- 里程碑:M5
- 门槛:④、⑪
- 前置:W1;hft-ctl gateway 与 sequencer 同进程(假交易所);gateway 的 issuer 表:`tg:1001`、`tg:1002` 为 Viewer,`tg:2001` 为 Trader;账户 a 在 VP 挂 2 张 GTC Open 单,另有一条在途 PairOnFill 链接(第一腿在 t0 + 61 s 成交);`VIEWER_CONFIRM_TTL_MS = 60000`
- 步骤:
  1. t0:`tg:1001` 发带 `preview` 的 `Kill{Account(a)}`(非 HARD、不带 flatten、`cancel_resting = true`),gateway 响应带 `cmd_hash` H
  2. t0 + 10 s:驱动写 `Timer{ClockBad}`;`tg:2001` 发 `Kill{Account(b)}`
  3. t0 + 60,000 ms:`tg:1001` 以 `confirmed = 1`、`expect_state_version = 0` 发同一命令
- 期望:
  - 步骤 1:`CmdResult{preview}`;cap 集合不变;节 15 的预览表多一条 `ViewerPreviewRow{cmd_hash = H, issuer = "tg:1001", preview_engine_ns = t0}`
  - 步骤 2 后 `ctl_state_version` 比步骤 1 时大 2
  - 步骤 3:`CmdResult{Applied}`;cap 集合含 `Kill{Account(a)}`(`CANCEL_RESTING`,非 HARD);与该命令配对的 `Note{CtlAudit}` 记 issuer `tg:1001`、role Viewer;该预览记录被删除;a 的 2 张 GTC 被撤;此后 a 的 Open `reject = KILLED`;t0 + 61 s 第一腿成交的同一 Input 内第二腿(`RISK_REDUCING`)出环
- 实现:`apps/hft-core/tests/viewer_stop.rs::md_11_viewer_stop_binds_preview`

| 行 | 相对主例的变化 | 断言 |
|---|---|---|
| MD-11.1 | 步骤 3 在 t0 + 60,001 ms | `CmdResult{ConfirmInvalid, detail = NoValidPreview}`;cap 集合无 `Kill{Account(a)}` |
| MD-11.2 | 步骤 3 由 `tg:1002` 发出 | `ConfirmInvalid{NoValidPreview}` |
| MD-11.3 | 步骤 3 的命令改为 `Kill{Account(b)}` | `ConfirmInvalid{NoValidPreview}` |
| MD-11.4 | 步骤 1 预览 `Kill{Global}`,步骤 3 确认 `Kill{Account(a)}` | `ConfirmInvalid{NoValidPreview}` |
| MD-11.5 | 步骤 3 之后 `tg:1001` 再发一次同一确认 | 第二次 `ConfirmInvalid{NoValidPreview}` |
| MD-11.6 | 不预览,`tg:1001` 直接发 `confirmed = 0` 的同一命令 | `CmdResult{NeedsConfirm}` |
| MD-11.7 | 步骤 1 与 3 之间 hft-core 重启(处理 `EngineClock` 清空预览表) | `ConfirmInvalid{NoValidPreview}` |
| MD-11.8 | `tg:1001` 发 `Kill{Account(a), hard}`、`Kill{Account(a), flatten}`、`Kill{Venue(VP)}` 或 `cancel_resting = false` 的 `Kill{Account(a)}` | 各自由 gateway 回 `Forbidden`;ctl 环中只有 `Note{CtlAudit}`,无 `ControlCmd` |
| MD-11.9 | `tg:1001` 发 `Resume{Kill, Account(a)}` 或 `SetMode{1, g1, a, CloseOnly}` | gateway 回 `Forbidden` |
| MD-11.10 | `tg:1001` 发 `Annotate{OpsNote, "x"}` | `CmdResult{Applied}`;状态不变 |
| MD-11.11 | `CmdHdr.role = Viewer` 的 `Kill{Account(a), hard}` 绕过 gateway 直接写进 ctl 环 | `CmdResult{RoleInsufficient}` |

### MD-12 控制面不可达 10 s 时紧急停止经 kill-switch 撤单并停止 hft-core,恢复后先补 Kill

- 规则:[紧急停止](../../python/ops_bot/README.md#紧急停止stop)、[本地触发](../../apps/kill-switch/README.md#本地触发ops_bot)、[停止 hft-core](../../apps/kill-switch/README.md#停止-hft-corestop_core)、[版本与确认](../subsystems/control-plane.md#版本与确认)、[启动顺序](../../apps/hft-core/README.md#启动顺序)
- 级别:proc
- 里程碑:M5
- 门槛:④、⑪
- 前置:GitHub 托管 runner(systemd 为 PID 1);按 deploy 生成器安装 `hft-core@prod`、`hft-kill-switch`、`hft-ops-bot` 单元、用户组与 polkit 规则;`hft-core@prod` 以 W1 refdata 运行(`order_env = feed_env = "testnet"`,行情与下单端点指向本机未监听端口);kill-switch 的 `[credentials.a.*]`、`[credentials.b.*]` 的 `base_url` 指向本机假 REST 服务(a、b 在 VS、VP 上各有 2 张挂单);ops_bot 由测试驱动直接调用命令处理函数,`tg:1001` 为 Viewer;ops_bot 配置 `stop.ctl_unreachable_s = 10`、`stop.result_timeout_s = 10`、`ops.view_stale_ms = 3000`;kill-switch `trigger.stop_wait_s = 30`;ops_bot 视图中 prod 实例无 MarginStress ≥ L2、无 Broken、无在途链接、无清仓锁、无外来成交计时、无自动平仓执行中
- 步骤:
  1. t = 0 把 `hft-core@prod` 的 ctl socket 文件改名(连接失败,心跳照常前进)
  2. t = 1 s `tg:1001` 发 `/stop a`,在预览上确认
  3. 等 kill-switch 的 `Done`;随后恢复 socket 文件名并 `systemctl start hft-core@prod`
  4. hft-core 心跳新鲜且 ctl 可连后,`tg:2001`(Trader)发 `/resume kill a`
- 期望:
  - 步骤 2:预览显示"控制面不可达,确认后若仍不可达将经 kill-switch 撤单并停止 hft-core"并给确认按钮;确认后 ops_bot 每秒重试 ctl,自首次失败起 10 s 时先写 `pending_kill.json` 一条 `{instance: "prod", scope: "a", state: "pending"}`,再经触发套接字发 `{action: "stop_core", instance: "prod", account: "a", issuer: "tg:1001"}`
  - kill-switch:100 ms 内回 `Accepted{targets: [(a, VS), (a, VP)]}`;假 REST 先收到 a 的撤单、无 b 的请求;`/var/lib/hft/kill-switch/stop-marker.json` 内容 `{"v": 1, "scope": "a", …}`;执行 `systemctl stop --no-block hft-core@prod.service`;hft-core 以 Drain 结束(`HeartbeatBlock.state = Stopped`,退出码 0);第二次撤单后回 `Done{core_stop: "Stopped", marker: true}`,a 的每个目标 `remaining = 0`
  - ops_bot 发 P0"紧急停止经 kill-switch 兜底:已撤单,hft-core@prod 已停止"
  - 步骤 3:新运行的 journal 中 `EngineClock` 之后第一条 Input 是 issuer `cli:systemd` 的 `Kill{Account(a)}`;ops_bot 在发任何其他命令之前以 issuer `ops_bot` 补发 `Kill{Account(a)}`,结果 `NoOp`,该条目转 `applied` 并移出文件,发 `Annotate{OpsNote}` 与 P1"待补 Kill 已生效"
  - 步骤 4:待补条目处理完之前 `/resume` 被 ops_bot 本地拒绝("待补 Kill 未完成");处理完之后经确认 `Resume{Kill, Account(a)}` → `Applied`
- 实现:`python/ops_bot/tests/test_stop_fallback.py::test_md_12_stop_fallback_via_kill_switch`
- 备注:前置依赖 GitHub 托管 runner 上 systemd 为 PID 1 并能装 polkit 规则,待核实(方法:M1 第一周在 CI 上试装);做不到则本用例改为 `drill`(M5),步骤与期望不变

| 行 | 相对主例的变化 | 断言 |
|---|---|---|
| MD-12.1 | ops_bot 视图中账户 a 的 MarginStress 为 L2 | 只发 `action = "cancel"`;`Done.core_alive = true`、无 `core_stop`;hft-core 不停;P0 含"确认要停请发 `/stop a core`";无停机标记文件 |
| MD-12.2 | 视图中有 Broken 的 group、在途链接(含 `RESUME_HOLD`)、未完成的清仓锁、外来成交计时、自动平仓执行中,或 journal 尾随超过 `ops.view_stale_ms` 未前进(每项各跑一次) | 同 12.1 |
| MD-12.3 | 12.1 之后 `tg:1001` 发 `/stop a core` 并确认,ctl 仍不可达 | 发 `stop_core`,结果同主例 |
| MD-12.4 | polkit 拒绝 `systemctl stop` | `Done{core_stop: "Denied", marker: false}`;`stop-marker.json` 被改名为 `stop-marker.json.aborted`;kill-switch P0;ops_bot P0 加粗"hft-core 进程仍在运行" |
| MD-12.5 | uid 不是 `hft-opsbot` 的进程连接触发套接字 | 不读载荷即关闭;`actions.jsonl` 一条 `trigger_denied`;P1;假 REST 收到 0 个请求 |
| MD-12.6 | ops_bot 在步骤 2 之后宕机;期间 Trader 经 CLI 设并经确认解除 `Kill{Account(a)}`;ops_bot 重启 | 不补发 Kill;发 `Annotate{OpsNote}`("已由人工恢复,未补发")、移除条目、P1 |
| MD-12.7 | 步骤 1 改为 `SIGSTOP` hft-core@prod 主进程(ctl socket 仍可连接、无响应;心跳过期,journal 尾随超过 `ops.view_stale_ms` 未前进);兜底完成后 `tg:1001` 发 `/stop a core` 并确认 | 10 s 内无 `CmdResult` → ops_bot P0"紧急停止失败",随即走兜底;因视图停滞只发 `action = "cancel"`,结果同 12.1(hft-core 不停、无停机标记文件);`/stop a core` 确认后发 `stop_core`,systemd 在 `TimeoutStopSec` 后终止进程,`Done{core_stop: "Inactive", marker: true}` |

### MD-13 停机标记使 stop_core 之后的干净重启带 Kill 起步,直到交易员 Resume{Kill}

- 规则:[启动顺序](../../apps/hft-core/README.md#启动顺序)、[命令路径](../../crates/hft-sequencer/README.md#命令路径)、[干净 Drain 重启自动解除 Recovery](../../crates/hft-risk/README.md#干净-drain-重启自动解除-recovery)、[停止 hft-core](../../apps/kill-switch/README.md#停止-hft-corestop_core)、[命令集](../subsystems/control-plane.md#命令集)
- 级别:dst
- 里程碑:M5
- 门槛:④、⑪
- 前置:W1(`prod` 实例);上一次运行以 `Drain` 快照结束,其后无 Input,快照中无 `Unknown` 槽、无 `Broken`/`HedgeFailure`、无未结束的链接;`auto_resume_after_clean_drain = true`;引导输入:停机标记存在,`scope = "*"`;实例 1、2 各自每 1 s 试发一笔 Open 链接
- 故障:t = 0 以 `RecoverDriver` 重启;各连接、对账与身份核验在 t ≤ 6 s 内就绪;t = 30 s Trader 经确认发 `Resume{Kill, Global}`
- 种子:标准集(§2.4)
- 断言:
  - 新运行 journal 中 `EngineClock` 之后的第一条 Input 是 `ControlCmd{Kill{level = Global, hard = 0, flatten = 0, cancel_resting = 1}}`,issuer `cli:systemd`,结果 `CmdResult{Applied}`;两者之间只有 `Note{EpochStart}`
  - a、b 闸门放行的 Input 内各有一条 `CapCleared{Recovery}`;此后 cap 集合 = {`Kill{Global}`};t ∈ [放行, 30 s) 的 Open `reject = KILLED`
  - t = 30 s:`CmdResult{Applied}`,cap 集合为空;下一笔 Open `reject = 0` 并出环
  - 对这段 journal 运行验证回放,`ControlCmd` 与 `CmdResult` 逐字节相同
- 实现:`crates/hft-testkit/src/scenarios/modes.rs::md_13_stop_marker_kill_survives_clean_restart`
- 备注:标记文件的读取、改名为 `.consumed`、内容不可解析时按 `Kill{Global}` 并 P0 的处理由 hft-core 的 `TESTS.md` 覆盖;本场景从"标记已被 hft-core 读出"起步

| 行 | 相对主例的变化 | 断言 |
|---|---|---|
| MD-13.1 | 标记 `scope = "a"` | 第一条 Input 为 `Kill{Account(a)}`;b 的 Open 在放行后出环;a 的 Open `reject = KILLED` |
| MD-13.2 | ops_bot 宕机,待补 Kill 未补发(journal 中无 issuer `ops_bot` 的 `Kill`) | 与主例相同:标记产生的 `Kill{Global}` 在 `Resume{Kill}` 之前一直挡住 Open |
| MD-13.3 | 只有 `stop-marker.json.aborted`(停机失败后作废) | `EngineClock` 之后无 `ControlCmd`;放行时 `CapCleared{Recovery}`,之后 Open 出环 |
| MD-13.4 | 改名前崩溃:同一标记在下一次启动再次被读出 | 第二次启动的 `Kill{Global}` 结果为 `CmdResult{NoOp}`(cap 已在),`ctl_state_version` 不变 |

### MD-14 Drain 期间只停增仓,升级命令被拒,清仓与 L3 做完才结束

- 规则:[Drain](../../crates/hft-sequencer/README.md#drain)、[运行模式、Drain 与 Unknown](../../crates/hft-exec/README.md#运行模式drain-与-unknown)、[类别](../subsystems/control-plane.md#类别)、[存储降级](../../crates/hft-risk/README.md#存储降级storage)、[存储降级](../../crates/hft-sequencer/README.md#存储降级)
- 级别:dst
- 里程碑:M5
- 门槛:④
- 前置:W1;实例 1 为 `probe-target`,g1 目标为两腿各 0.100 BTC、当前 0.050,增仓切片每片 0.010;实例 2 的 cell (2, b, g2) 期望 `CloseOnly`;g1 有一条在途链接:第一腿已成交,第二腿 IOC 已发、t = 1.5 s 回报成交;`shutdown.drain_timeout_ms = 30000`;`conn_liveness_ms = 2000`
- 故障:t = 1 s Trader 发 `Drain{cancel_resting = true}`;t = 2 s Trader 经确认发 `SetMode{2, g2, b, Live}`;t = 3 s Trader 经确认发 `Kill{Account(a), flatten}`
- 种子:标准集(§2.4)
- 断言:
  - t = 1 s:`CapSet{Draining}`,`params[0] = 1`;a、b 每个 venue 一条 `GwCmd{CancelAll}`;此后 g1 的增仓切片 `reject = DRAINING`
  - t = 1.5 s 的第二腿回报照常入账,链接进入 `Done`
  - t = 2 s:`CmdResult{PreconditionFailed, detail = Draining}`
  - t = 3 s:预览后确认,`CmdResult{Applied}`;`exec.engine_flatten(a)`:a 的永续腿 reduce-only 单先于现货腿出环,`reject = 0`
  - `StepOutcome::Done{restart: false, cause: Drain}` 出现在 a 两腿都到达 0 之后的第一个满足条件的 Input,早于 t = 31 s;此刻无 `Open`/`Hedging` 链接、无非终态 IOC、无 `HedgeRetry` 定时器、`exec.drain_ready() = true`;随后写 `purpose = Drain` 的快照;无 `Note{DrainTimeout}`
  - INV-18 在 `Done` 时成立;INV-12 每步成立
- 实现:`crates/hft-testkit/src/scenarios/modes.rs::md_14_drain_finishes_flatten_before_done`

| 行 | 相对主例的变化 | 断言 |
|---|---|---|
| MD-14.1 | t = 3 s 起 `P1` 的 BBO 连接停止回帧直到结束(清仓永续腿无法定价) | t = 31 s:`Note{DrainTimeout}` P0,`v[0..4]` 的位图含 a,`v[6] ≥ 1`;照常写 Drain 快照;`Done{cause: Drain}` |
| MD-14.2 | 不发 `Kill{flatten}`;t = 3 s 起 (a, VP) 的 `liq_dist` 降到 0.09(L3,`force_step = 0.20`,`force_step_interval_ms = 5000`),t = 13 s 回到 0.40 | t ∈ [3 s, 13 s) 每步 `ForceDeleverage` 照常出单;`Done` 不早于最后一步引擎目标到达 |
| MD-14.3 | 不发 `Drain`;t = 1 s 驱动写 `Timer{PersistLag}`(档位 2),此时 (a, VP) 处于 L3 且引擎目标未到达 | `Storage` cap 生效(Open `reject = CLOSE_ONLY`);在引擎目标到达之前无 `Draining` cap;到达后的第一条 Input 内 `CapSet{Draining}`,最终 `Done{cause: StorageCritical}` |
| MD-14.4 | 同 14.3,但 a 只有已到达的清仓目标(`Kill{Account(a), flatten}` 已平到 0,清仓锁仍在) | `Timer{PersistLag}` 的同一 Input 内 `CapSet{Draining}`(清仓锁不使其推迟) |
| MD-14.5 | `Drain{cancel_resting = false}`;a 有一张不带链接的 GTC Open 挂单与一张带 LegLink 的挂单第一腿 | 只撤带 LegLink 的第一腿;不带链接的 GTC 保留;无 venue 级 `CancelAll`;之后 `ConnState{Up}` 不触发撤单重发 |

### MD-15 ReplaceRefdata 触发的 Drain 超时后照常复核并以重启请求结束,重启后新 refdata 生效、Recovery 在位

- 规则:[refdata 变更](../../apps/hft-core/README.md#refdata-变更drain--重启)、[Drain](../../crates/hft-sequencer/README.md#drain)、[命令路径](../../crates/hft-sequencer/README.md#命令路径)、[命令行与退出码](../../apps/hft-core/README.md#命令行与退出码)、[干净 Drain 重启自动解除 Recovery](../../crates/hft-risk/README.md#干净-drain-重启自动解除-recovery)
- 级别:dst
- 里程碑:M5
- 门槛:④
- 前置:W1;生效 refdata `version = 7`;`instance.config_source = "journal"`;`shutdown.drain_timeout_ms = 20000`;账户 a 的 g1 有一条在途链接,其第二腿 IOC 自 t = 0.5 s 发出后 `MockVenue` 不回任何回报(槽停在 `PendingNew`);新 blob `version = 8` 只比 v7 多一个 inst `S2` = ETH-USDT @ `VS`,`RefData::load(v8, v7)` 通过;Admin issuer `cli:admin`
- 故障:t = 1 s Admin 经确认发 `ReplaceRefdata{version = 8}`;t = 21 s Drain 超时;Drain 末的装配校验通过
- 种子:标准集(§2.4)
- 断言:
  - t = 1 s:`CmdResult{Applied}`;`CapSet{Draining}`,`params[0] = 0`;本次运行的生效 refdata 仍为 v7
  - t = 21 s:`Note{DrainTimeout}` P0,`v[4] = 1`;随后一条 `Timer{Sys, RefdataRecheck}`,复核通过;Drain 快照节 9 指向 v8 的 `ConfigBlob`;`StepOutcome::Done{restart: true, cause: ReplaceRefdata}`(hft-core 映射为退出码 75)
  - 重启后生效 refdata 为 v8(`S2` 可解析);a、b 各有 `Recovery` cap;a、b 的闸门放行时都无 `CapCleared{Recovery}`(快照中账户 a 有未结束的链接;干净重启条件 ③ 对整个实例判定一次,所以 b 同样不置 `CLEAN_DRAIN`);原 `PendingNew` 槽恢复为 `Unknown` 并发出查询
- 实现:`crates/hft-testkit/src/scenarios/modes.rs::md_15_refdata_drain_timeout_restarts_with_new_blob`

| 行 | 相对主例的变化 | 断言 |
|---|---|---|
| MD-15.1 | v8 把账户 b 的 `bound_instance` 改为实例 3;命令时 b 无仓位,Drain 期间 b 的一张未撤 GTC Open 成交(`cancel_resting = false` 不撤它) | Drain 末复核失败:`Note{RefdataRecheckFailed}` P1,`v = [3, b]`;快照节 9 仍指 v7;`Done{restart: false, cause: Drain}`(退出码 0) |
| MD-15.2 | v8 改变 BTC 的 `AssetExp` | t = 1 s:`CmdResult{Rejected, detail = RefdataRecheck}`,arg = 33(`AssetChanged`);无 `Draining` cap,进程继续运行 |

### MD-16 冷启动不改期望模式,只加 Recovery cap;闸门放行后仍需 Resume{Recovery},闸门未放行时 Kill{flatten} 可用

- 规则:[冷启动恢复](../../crates/hft-sequencer/README.md#冷启动恢复recoverdriver)、[启动闸门](../../crates/hft-risk/README.md#启动闸门)、[对账闸门与 HARD 解除的严格条件](../../crates/hft-risk/README.md#对账闸门与-hard-解除的严格条件)、[类别](../subsystems/control-plane.md#类别)、[启动顺序](../../apps/hft-core/README.md#启动顺序)
- 级别:dst
- 里程碑:M5
- 门槛:④
- 前置:W1;崩溃前期望模式 (1, a, g1) `Live`、(2, b, g2) `CloseOnly`;a 的 g1 持仓 `S1` +0.10000 / `P1` −0.100,b 的 g2 持仓 `S1` +0.05000 / `P1` −0.050;无在途;`auto_resume_after_clean_drain = true`
- 故障:t = 0 `CrashFault`(非干净停机)后以 `RecoverDriver` 重启;t = 1–5 s 各连接 Up、`AccountConfigReport`(UID 一致、`uid_keys` 覆盖全部交易 key)与 `Timer{Sys, KillSelfTestOk}` 到达;`ReconActor` 在 `EpochStart` 之后对 (a, VS)、(a, VP) 各发一组全部 Ignore 的报告,对 (b, VP) 发一组含一条 Warn 仓位差的报告;t = 8 s Trader 经确认发 `Resume{Recovery, b}`;t = 10 s Trader 经确认发 `Kill{Account(b), flatten}`;t = 20 s Trader 经确认发 `Resume{Recovery, a}`
- 种子:标准集(§2.4)
- 断言:
  - 恢复后 ModeTable 与崩溃前快照相同,journal 中无 `SetMode`;a、b 各一条 `CapSet{Recovery}`(Info)
  - (a, v) 的 `IDENTITY` 位只在该 venue 的 `AccountConfigReport` 与该账户的 `KillSelfTestOk` 都到达之后置位;a 放行的 Input 内无 `CapCleared{Recovery}`(非干净重启)
  - a 放行后、t = 20 s 之前,a 的 Open `reject = CLOSE_ONLY`
  - t = 8 s:`CmdResult{PreconditionFailed, detail = GateNotReleased}`
  - t = 10 s:`CmdResult{Applied}`;b 的永续腿 reduce-only 单先出环,随后现货卖单,两腿逐步到 0;期间 b 的 `RECONCILED` 位仍为 0
  - t = 20 s:`CmdResult{Applied}`,`CapCleared{Recovery}`;此后 a 的 Open 出环
- 实现:`crates/hft-testkit/src/scenarios/recover.rs::md_16_cold_start_adds_recovery_cap_only`

### MD-17 hft-core 以新的 (pid, boot_id) 重启并开始心跳后,kill-switch 回到判活

- 规则:[判活与触发](../../apps/kill-switch/README.md#判活与触发)、[撤单目标](../../apps/kill-switch/README.md#撤单目标)、[优雅退出与 kill 语义](../../apps/hft-core/README.md#优雅退出与-kill-语义)
- 级别:proc
- 里程碑:M5
- 门槛:④
- 前置:GitHub 托管 runner(systemd 为 PID 1);按 deploy 生成器安装 `hft-core@sim`(只含 Sim 账户、无 key;`watchdog.hb_stale_ms = 1000`;`Restart=on-failure`、`RestartSec=1`)与 `hft-kill-switch`(`hft_core_config` 只列 sim 实例,`poll_ms = 100`、`hb_stale_factor = 2`、`hb_stale_hard_ms = 5000`,`[credentials]` 为空);本机假 REST 服务记录收到的请求;告警通道指向本机桩
- 步骤:
  1. 启动 `hft-core@sim`,等 `HeartbeatBlock.state = Running`
  2. 对 hft-core 主进程 `kill -9`
  3. 等 systemd 拉起新进程
- 期望:
  - 步骤 1:`kill-switch status` 显示 sim 实例为 `Watching`
  - 步骤 2:≤ 2.2 s 内两个信号都停 → `Fire`;目标表为空,不发任何 HTTP 请求(假 REST 计数 0);一条 P1 告警(实例 `sim`、旧 `(pid, boot_id)`、"无 Live 目标");状态 `Fired`
  - 步骤 3:新进程的心跳计数前进、`state = Warmup`(闸门尚未放行)时状态回到 `Watching`;旧 `(pid, boot_id)` 不再触发
- 实现:`apps/kill-switch/tests/instance_change.rs::md_17_new_instance_rewatched`
- 备注:前置依赖 GitHub 托管 runner 上 systemd 为 PID 1,待核实(方法:M1 第一周在 CI 上试装);做不到则本用例改为 `drill`(M5),步骤与期望不变

| 行 | 相对主例的变化 | 断言 |
|---|---|---|
| MD-17.1 | 步骤 2 改为 `systemctl stop hft-core@sim`(Drain 后 `state = Stopped`) | 状态回到 `Idle`;无 `Fire`、无告警 |
| MD-17.2 | 步骤 3 中新进程在写心跳之前卡住(心跳块不存在) | 状态保持 `Fired`,不回到 `Watching` |

### MD-18 人工在交易所网页平掉一条腿后该 cell 进入 manual_hold,解除前不产生任何自动对冲单

- 规则:[人工保持与恢复期抑制](../../crates/hft-exec/README.md#人工保持与恢复期抑制)、[外来成交的默认处理](../../crates/hft-exec/README.md#外来成交的默认处理b3)、[命令集](../subsystems/control-plane.md#命令集)、[对账分级](../../crates/hft-risk/README.md#对账分级全系统唯一一次)
- 级别:dst
- 里程碑:M9
- 门槛:④
- 前置:W1;实例 1 为 `probe-target`,g1 目标 `S1` +0.10000 / `P1` −0.100 已到达(cell `Idle`);`ExecConfig`:`external_deadline_ms = 5000`、`broken_auto_fix_ms = 120000`、`fix_tolerance_default = 0.02`、`fix_deadline_ms = 10000`、`dust_notional = 5 USDT`
- 故障:t = 1 s `MockVenue` 按 `external_fills(cause = Manual)` 在账户 a 的 `P1` 上成交买入 0.100(人工网页平仓,用户流照常推送),gw 产出 `ExternalFill{cause = Manual}`;t = 30 s 策略对 g1 调 `clear_target`;t = 200 s 策略对 g1 调 `set_target(S1 = 0, P1 = 0)`
- 种子:标准集(§2.4)
- 断言:
  - t = 1 s 的 Input 内:ledger 与 LegTracker 中 `P1` 腿为 0;`Note{ExecManualHold}`(P1),`v = [a | g1 << 8, 1, 1, P1, Buy, 0.100, 0.10000]`;`Note{ExecManualExternal}` 级别 Info;策略收到 `EventRef::External`;目标暂停为 `Stuck{External}`
  - 从 t = 1 s 到 t = 200 s:journal 中无 (a, g1) 的 `HedgeFix`、`ResidualTrim`、`ExternalDefault` 订单或 `ExecDecision{ExternalDefault}`,无 `CapSet{Broken}`,无 `Note{ExecAutoFixDone}`/`Note{ExecAutoFixFailed}`
  - t = 30 s:无 `manual_hold` 解除注记
  - t = 200 s:`Note{ExecManualHold}`(Info),`v = [a | g1 << 8, 0, 1]`;随后 `S1` 卖单(Reduce)出环,g1 两腿到 0
- 实现:`crates/hft-testkit/src/scenarios/modes.rs::md_18_manual_fill_holds_cell`

| 行 | 相对主例的变化 | 断言 |
|---|---|---|
| MD-18.1 | t = 200 s 改为 Trader 发 `HedgeFix{a, g1}` | `Note{ExecManualHold}` 解除原因 2;`S1` 卖 0.10000 的 `HedgeFix` 单(`RISK_REDUCING`)出环 |
| MD-18.2 | t = 50 s (a, VP) 进入 L3 | 第一步 `ForceDeleverage` 设下引擎目标的 Input 内 `Note{ExecManualHold}` 解除原因 3 |
| MD-18.3 | 人工在 `P1` 上买入 0.150(仓位由 −0.100 变为 +0.050,变号)且用户流不推送 | `ReconActor` 两轮同一仓位差 → `CapSet{Recon}`(a, VP),条目不带 `AUTO_EXTERNAL`;经确认 `AdoptExchange{a, VP, Position, P1, report_id}` → `Applied`,ledger 写 `Adjust` 类 +0.150、LegTracker 同量调整;之后两轮全部 Ignore,经确认 `Resume{Recon, a, VP}` → `Applied` |
| MD-18.4 | 人工在 `VS` 卖出 0.05000 BTC 现货且用户流不推送 | `ReconActor` 的 (a, VS) 报告出现 BTC `Balance` 条目(引擎 0.10000、交易所 0.05000);经确认 `AdoptExchange{a, VS, Balance, BTC, group = g1}` → g1 现货库存 0.05000;下一轮该条目 Ignore |

### MD-19 一个账户的 venue 集合含同一交易所的两个 VenueId 时加载失败,两腿同所的 GroupSpec 被拒

- 规则:[数据结构](../../crates/hft-refdata/README.md#数据结构)、[LoadErrCode 码表](../../crates/hft-refdata/README.md#loaderrcode-码表)、[命令路径](../../crates/hft-sequencer/README.md#命令路径)、[结果码](../subsystems/control-plane.md#结果码)
- 级别:table
- 里程碑:M5
- 门槛:④
- 前置:W1;另有 `VB`:`VenueId = 4`,`Exchange::Binance`,USDT 永续,inst `B1` = BTC-USDT 永续;`SetParams` 由 issuer `pipeline` 直接写 ctl 环,实例 1、账户 a,生效参数 `version = 3`
- 实现:`crates/hft-testkit/src/scenarios/modes.rs::md_19_same_exchange_rejected`

| 行 | 输入 | 期望 |
|---|---|---|
| MD-19.1 | refdata 中 Live 账户 a 的 `venues = {VS, VB}` | `RefData::load` → `Err`,码 `SameExchangeMarkets`(21) |
| MD-19.2 | W1-SIM 的 Sim 账户 s 的 `venues = {VS, VB}` | 同 19.1 |
| MD-19.3 | 账户 a 的 `venues = {VS, VP}` | `RefData::load` → `Ok` |
| MD-19.4 | `SetParams{version = 4}`,新增 group g3,两腿都是 `S1` | `CmdResult{InvalidArgs, detail = SameExchangeLegs}`;生效参数仍为 `version = 3`;g1 的 Open 照常出环 |
| MD-19.5 | `SetParams{version = 4}`,新增 group g3,两腿为 `S1` 与 `P1` | `CmdResult{Applied}`;生效参数 `version = 4` |

### MD-20 Sim 账户的订单只进 out-sim,SimReset 与 SimTransfer 只对 Sim 账户且重置后不残留 Broken

- 规则:[模拟账户](../../crates/hft-sequencer/README.md#模拟账户)、[命令路径](../../crates/hft-sequencer/README.md#命令路径)、[命令集](../subsystems/control-plane.md#命令集)、[模拟账户是什么](../../crates/hft-sim/README.md#模拟账户是什么)、[cap 记录](../../crates/hft-risk/README.md#cap-记录)
- 级别:dst
- 里程碑:M4
- 门槛:④
- 前置:W1-SIM;实例 1 为 `probe-taker`,绑定 s;`SimAccountCfg.initial_balances` = {(VS, USDT, 100,000)、(VP, USDT, 50,000)};`hedge_deadline_ms = 3000`
- 故障:
  1. t = 10 s 策略发 Open 链接(第一腿 `S1` 买 0.01000,链接腿 `P1` 卖 0.010);t = 10.001 s Trader 发 `SimTransfer{s, VP → VS, USDT, 49,990}`(早于第一腿成交回报)
  2. t = 20 s Trader 发 `SetMode{1, g1, s, CloseOnly}`;t = 21 s Trader 经确认发 `SimReset{s}`;t = 22 s Trader 经确认发 `Resume{HedgeFailure, s}`
- 种子:标准集(§2.4)
- 断言:
  - 全程 s 的每个出环 `GwCmd` 只进 `out-sim`,任何 `out-<venue>` 中无 s 的帧;kill 字不含 s 的位
  - 第 1 步:`SimTransfer` → `Applied`,ledger 与 `SimActor` 的 VP USDT 各减 49,990;第二腿被 `SimActor` 以 `InsufficientMargin`(n = 10)拒绝,重试同样被拒;不晚于第一腿成交后 3,000 ms 出现 `CapSet{Broken}`(s, g1)与 `CapSet{HedgeFailure}`(s)
  - t = 21 s:`CmdResult{Applied}`;同一 Input 内 ledger 余额 = `initial_balances`、仓位与库存为 0,LegTracker 清零,执行层目标清除,`CapCleared{Broken}`;`HedgeFailure{s}` 仍在;`SimActor` 随后发的 `PositionReport`/`BalanceReport` 等于 `initial_balances`、仓位 0
  - t = 22 s:`CmdResult{Applied}`,`CapCleared{HedgeFailure}`
- 实现:`crates/hft-testkit/src/scenarios/sim.rs::md_20_sim_account_isolated_and_resettable`
- 备注:reconciler 不对 Sim 账户对账、kill-switch 不以 Sim 账户为目标,分别由 reconciler、kill-switch 的 `TESTS.md` 覆盖(kill-switch 侧的端到端见 MD-17)

| 行 | 相对主例的变化 | 断言 |
|---|---|---|
| MD-20.1 | t = 21 s 时 s 有一张已发出、未终态的 IOC | `CmdResult{PreconditionFailed, detail = SimNotIdle}`;余额与 cap 不变 |
| MD-20.2 | 去掉 t = 20 s 的 `SetMode`(cell 有效模式 `Live`) | `PreconditionFailed{SimNotIdle}` |
| MD-20.3 | 在含 Live 账户 a 的混跑测试实例中发 `SimReset{a}` 与 `SimTransfer{a, …}` | 各回 `CmdResult{InvalidArgs, detail = NotSim}` |
| MD-20.4 | `SimTransfer` 的金额超过 ledger 中 VP 的可用 USDT | `CmdResult{PreconditionFailed, detail = InsufficientBalance}` |

### MD-21 Admin 的冒烟单在 Shadow 与 CloseOnly 下照样挂出远离对手价的最小 PostOnly,到期自动撤销

- 规则:[冒烟单](../../crates/hft-oms/README.md#冒烟单smokeorder)、[命令集](../subsystems/control-plane.md#命令集)、[命令路径](../../crates/hft-sequencer/README.md#命令路径)、[检查链](../../crates/hft-risk/README.md#检查链按廉价--昂贵短路适用范围以本表为唯一定义)
- 级别:integ
- 里程碑:M5
- 门槛:④
- 前置:W1;cell (1, a, g1) 期望 `CloseOnly`;`smoke_far_bp = 0.03`;`conn_liveness_ms = 2000`;`S1` ask = 60,000.00;Admin 以 `CmdHdr.role = Admin`、issuer `cli:admin` 直接写 ctl 环;`ReconActor` 每 5 s 一组报告
- 步骤:
  1. t = 0 Admin 经确认发 `SmokeOrder{account = a, venue = VS, inst = S1, side = Buy, ttl_ms = 10000}`
  2. 运行到 t = 15 s
- 期望:
  - 步骤 1:`CmdResult{Applied}`;同一 Input 内一条 `OrderDecision{action = New}`,`trigger_seq` = 该命令的 seq,`tif = PostOnly`,限价 58,200.00(60,000.00 × 0.97,向下取整到 tick),数量 0.00010 BTC(5 × 1.1 / 58,200 ≈ 0.0000945,向上取整到 lot 0.00001),槽带 `SMOKE`、`origin = Command`、`group = GROUP_NONE`;`GwCmd` 进 `out-VS`
  - 实例 1 不收到该槽的 `OrderUpdate`;无 `ExecDecision`,g1 目标不变
  - t = 10 s 的 `Timer{Oms, SmokeTtl}` 内对该槽发撤单(迁移原因 `SmokeTtl`);`CancelAck` 后 `smoke_slot = 0`
  - t = 5 s 与 t = 10 s 的对账报告中无该订单的孤儿单条目
- 实现:`crates/hft-testkit/src/scenarios/modes.rs::md_21_smoke_order_lifecycle`

| 行 | 相对主例的变化 | 断言 |
|---|---|---|
| MD-21.1 | cell 期望 `Shadow` | 与主例相同(冒烟单不经模式门) |
| MD-21.2 | `CmdHdr.role = Trader` | `CmdResult{RoleInsufficient}`;无槽 |
| MD-21.3 | `Kill{Account(a), hard}` 生效中 | `CmdResult{Rejected, detail = SmokeRiskReject}`,arg = 5(`HARD` 的位号);无槽 |
| MD-21.4 | t = 2 s 再发一张 (a, VS) 的冒烟单;t = 11 s(第一张已终态)再发一张 | t = 2 s:`PreconditionFailed{SmokeActive}`;t = 11 s:`Applied` |
| MD-21.5 | `S1` 的 BBO 连接自 t = −5 s 起停止回帧(静默超过 `conn_liveness_ms`) | `PreconditionFailed{SmokeNoPrice}` |
| MD-21.6 | W1-SIM:`SmokeOrder{account = s, …}` | `CmdResult{InvalidArgs, detail = NotLive}` |

### MD-22 模拟回报洪峰期间 rep-sim 排在 md 之后、每轮有上限,实盘输入的取出顺序不变

- 规则:[主循环](../../crates/hft-sequencer/README.md#主循环step)、[环与容量](../../crates/hft-sequencer/README.md#环与容量)、[模拟账户](../../crates/hft-sequencer/README.md#模拟账户)
- 级别:integ
- 里程碑:M4
- 门槛:④
- 前置:同进程混跑(只用于测试):账户 a(Live,实例 1)与 s(Sim,实例 2),两个实例都订阅 `P1`,且对每条 `P1` 的 `Bbo` 各发一个 Open 意图;sequencer 以 `LiveDriver` 驱动进程内的 hft-rt 环,测试线程在两次 `step` 之间写环;`sim_max_per_round = 32`、`rings[Rep] = 32/64`、`rings[Bbo] = 32/128`、`md_round_ns = 50000`
- 步骤:
  1. 运行 A:预先写入 `rep-sim` 500 条回报、`bbo-VP` 10 条 `Bbo`、`md-VP` 20 条 `Trade`、`rep-VP` 5 条回报,反复调 `step` 直到全部环为空
  2. 运行 B:同一份输入,但 `rep-sim` 为空
- 期望:
  - 运行 A 每一轮取出的 `rep-sim` 记录 ≤ 32 条,且都位于该轮最后一条 md 记录之后;500 条在 16 轮内取完
  - 每条 `P1` `Bbo` 引起的两条 Open 决策中,实例 1(Live 账户)的在前、实例 2(Sim 账户)的在后
  - 运行 A 的 journal 去掉 `rep-sim` 记录后,实盘 Input(`ctl`、`rep-VP`、`bbo-VP`、`md-VP`)的顺序与运行 B 逐条相同
- 实现:`crates/hft-testkit/src/scenarios/sim.rs::md_22_sim_flood_yields_to_live`

### MD-23 只含 Sim 账户的实例非干净重启后模拟真值由快照与 journal 确定地恢复,无不一致与毒丸时自动解除 Recovery

- 规则:[重启恢复](../../crates/hft-sim/README.md#重启恢复)、[干净 Drain 重启自动解除 Recovery](../../crates/hft-risk/README.md#干净-drain-重启自动解除-recovery)、[启动顺序](../../apps/hft-core/README.md#启动顺序)、[实例角色](../../apps/hft-core/README.md#实例角色prod-与-sim)、[恢复时决策不一致](../../crates/hft-sequencer/README.md#恢复时决策不一致)
- 级别:dst
- 里程碑:M4
- 门槛:④
- 前置:W1-SIM,refdata 只含 Sim 账户 `sim-s1`(实例 1)与 `sim-s2`(实例 2),均为 `probe-taker`;`auto_resume_after_clean_drain = true`;t = 60 s 时 s1 有 2 张 GTC Open 挂单在 `P1` 上,持仓 `S1` +0.02000 / `P1` −0.020
- 故障:t = 60 s `CrashFault`(非干净),随后以 `RecoverDriver` 重启;`SimActor` 按 hft-sim 的重启恢复步骤以 `seed_account_from` 重建;重启后再运行 120 s
- 种子:标准集(§2.4)
- 断言:
  - `SimActor` 重建之后:s1 各 (venue, asset) 余额 = 快照节 22 `Cash.total`;`P1` 仓位 −0.020、入场价 = `cost / |qty|`;订单表含 2 张挂单,剩余量 = `qty − filled`
  - OMS 对恢复后的 `Unknown` 槽发 `Query`,`SimActor` 回 `QueryResult{New}`,两槽回到 `Live`;无 `NotFound`,查询不产生成交
  - 闸门放行的 Input 内 s1、s2 各一条 `CapCleared{Recovery}`;无 `Note{RecoveryHeldSim}`
  - 重启后每个无在途的时刻 INV-16 成立;对同一 journal 做两次恢复,`SimActor` 的状态哈希相同
- 实现:`crates/hft-testkit/src/scenarios/sim.rs::md_23_sim_instance_restart_restores_truth`
- 备注:`sim` 实例不加载 key 即可装配、refdata 含 Live 账户时退出 2(`LiveAccountInSimInstance`)由 hft-core 的 `TESTS.md` 覆盖;kill-switch 监视 `sim` 实例时目标表为空见 MD-17

| 行 | 相对主例的变化 | 断言 |
|---|---|---|
| MD-23.1 | 尾部重放中 (VP, s1) 的一条 `OrderDecision` 与日志不一致 | `Note{RecoverMismatch}` P0;s1 不自动解除,放行的 Input 内写 `Note{RecoveryHeldSim}`(P1,`v[0]` = s1,`v[1]` 含 `RECOVER_MISMATCH`);s2 照常 `CapCleared{Recovery}` |
| MD-23.2 | 同一输入连续崩溃达 `poison_crash_limit = 2` | 两个账户都写 `Note{RecoveryHeldSim}`(`v[1]` 含 `POISON`),`Recovery` 保留到人工 `Resume{Recovery}` |
| MD-23.3 | 崩溃时 `SimActor` 有一笔 s2 的已撮合未交付成交 | 重启后 ledger 与 `SimActor` 都不含这笔成交,INV-16 成立 |

### MD-24 预热块作为 Input 记 journal 并按 journal 顺序投递给 Model,回放结果相同

- 规则:[预热注入](../../crates/hft-strategy-sdk/README.md#预热注入)、[命令路径](../../crates/hft-sequencer/README.md#命令路径)、[advisor 与 Model](../../crates/hft-sequencer/README.md#advisor-与-model)、[命令集](../subsystems/control-plane.md#命令集)、[权限表](../../crates/hft-ctl-proto/README.md#权限表)
- 级别:integ
- 里程碑:M9
- 门槛:④
- 前置:W1;实例 1 为 `probe-target`,其 `Model` 接收 `local_kind = 7` 的预热,策略声明 `SubsBuilder::warmup(Part::Both)`;命令由 `CtlActor` 直接写 ctl 环,`CmdHdr.account = 0`(`*`)
- 步骤:
  1. issuer `pipeline` 依次发 `InjectWarmup{instance = 1, local_kind = 7, chunk_no = 0, last = 0, payload 4096 B}`、`{chunk_no = 1, last = 0, payload 4000 B}`、`{chunk_no = 2, last = 1, payload 100 B}`
  2. 以 `VerifyDriver` 回放这段 journal;以注入前的 Model 检查点为起点重算 Advice
- 期望:
  - 步骤 1:三条命令各 `CmdResult{Applied}`;三条 `ControlCmd` 都是 journal 的 Input;每条的同一 Input 内向快速部分投递 `EventRef::Warmup{7, chunk_no, last, payload}`;advisor 在看到 `Applied` 之后按 journal 顺序(0、1、2)向 Model 投递
  - 步骤 2:回放的 Output 记录逐字节相同;重算的 Advice 载荷与 journal 中的逐字节相同
- 实现:`crates/hft-testkit/src/scenarios/modes.rs::md_24_warmup_injected_as_input`

| 行 | 相对主例的变化 | 断言 |
|---|---|---|
| MD-24.1 | 一块 `payload` 为 4,097 B | `CmdResult{InvalidArgs, detail = TooLong}`;不投递 `EventRef::Warmup` |
| MD-24.2 | issuer `tg:1001`,`CmdHdr.role = Viewer` | `CmdResult{RoleInsufficient}` |
| MD-24.3 | 三块按 `chunk_no` 2、0、1 的顺序到达 | 按 journal 顺序 2、0、1 投递,不重排 |
| MD-24.4 | 账户 a 的闸门未放行且 `Draining` 生效 | 三条仍 `Applied` |
| MD-24.5 | `instance = 9`(不存在) | `CmdResult{UnknownTarget}` |

### MD-25 快照含闸门、全部 cap 与未完成的对账分组,从快照起步的状态与连续运行相同

- 规则:[快照](../../crates/hft-sequencer/README.md#快照)、[快照行格式](../../crates/hft-risk/README.md#快照行格式节-4171823草案实现时以代码与编译期断言为准)、[mode 模块](../../crates/hft-risk/README.md#mode-模块唯一的模式状态)、[冷启动恢复](../../crates/hft-sequencer/README.md#冷启动恢复recoverdriver)
- 级别:dst
- 里程碑:M9
- 门槛:④
- 前置:W1;t = 50 s 时生效的 cap:`Kill{Account(b)}`、`DailyLoss{a}`、`Recon{a, VP}`、`ClockBad`;账户 b 的 `RECONCILED` 未置位;(a, VS) 的一组 `ReconcileReport`(`count = 3`)已到达 2 条
- 故障:t = 50 s 驱动写 `Timer{Sys, Snapshot}`(关键帧);t = 51 s 该对账组第 3 条到达;t = 52 s `CrashFault`,以 `RecoverDriver` 重启
- 种子:标准集(§2.4)
- 断言:
  - 关键帧节 4 含 4 条 `CapRecord`,与当时 cap 集合逐字段相同;节 18 `Gates` 中 b 的 `RECONCILED` 位为 0、`ctl_state_version` 等于运行中的值;节 19 有 `ReconPendingHdr{account = a, venue = VS, count = 3, received = 2}`
  - 以 `VerifyDriver` 从该关键帧起回放到 t = 52 s:每条 Input 之后的 cap 集合、每个 cell 的有效模式与状态哈希都与连续运行相同;t = 51 s 的第 3 条使该组生效,两边结果相同
  - `RecoverDriver` 重启后:cap 集合 = 崩溃前集合 ∪ {`Recovery{a}`、`Recovery{b}`};ModeTable 与崩溃前相同
  - INV-6 每步成立
- 实现:`crates/hft-testkit/src/scenarios/recover.rs::md_25_snapshot_restores_caps_and_gates`

## 5. 恢复、OMS 与对账

"崩溃""重启"与"闸门输入齐备"的含义同 §3 引言。本节的 `integ` 用例由测试驱动按步骤交出 Input(实现 sequencer 的 `Driver`),行情、回报与命令都写成 journal 记录。

### B-1 撤单、改单、查单与全撤的 SendReport 未写出时目标槽状态不变,只有新单失败迁到 Rejected

- 规则:[SendReport 处理](../../crates/hft-oms/README.md#sendreport-处理)、[状态机](../../crates/hft-oms/README.md#状态机)、[Unknown 收敛流程](../../crates/hft-oms/README.md#unknown-收敛流程)
- 级别:prop
- 里程碑:M2
- 门槛:④
- 性质:对任意非链接槽状态与任意 `SendReport{op, status}`:`op = Place` 且 status 为确定未写出 → 槽 `Rejected`、`reject_code = status`、InflightBook 与 LegTracker 的在途减去该槽剩余量、拥有者恰好收到一条 `OrderUpdate::Rejected{bits = LOCAL_SEND_FAIL, venue_code = status}`;`op ∈ {Cancel, Amend}` 且确定未写出 → 目标槽 `OrderState` 不变,`CANCEL_SENT`/`AMEND_SENT` 清零,恰好一条 `Note{OmsCancelNotSent}`;`op = Query` 且确定未写出 → 槽状态不变、`query_count` 不变,`QueryUnsupported` 另有一条 `Note{OmsNeedsRestQuery}`;`op = CancelAll` 且确定未写出 → 范围内槽状态不变、`CANCEL_SENT` 清零、恰好一条 `Note{OmsCancelNotSent}`;`status = WriteError` → `Place` 槽 `Unknown` 并查询,`Cancel`/`Amend` 目标槽置 `VERIFY` 并查询,`CancelAll` 范围内槽置 `VERIFY`;`status = Written` → 只记 `sent_tsc`(`Query` 另 `query_count += 1`);每步 INV-2、INV-7
- 生成:1–64 个非链接槽,初始状态由合法操作序列(place、Ack、cancel、amend、下单连接断开)产生,覆盖 {`PendingNew`、`Live`、`PendingCancel`、`PendingAmend`、`Unknown`};每槽 1–4 条 `SendReport`,`op` 从 {Place, Cancel, Amend, Query, CancelAll} 中取该槽此刻确有在途操作的一种(`Query` 只对 `Unknown` 或带 `VERIFY` 的槽),`status` 从 {`Written`、`KillBlocked`、`RateLimited`、`NoConn`、`EncodeError`、`RingFull`、`WriteError`} 抽取,`QueryUnsupported` 只配 `Query`;5% 的 `SendReport` 重复一次
- 参考:`crates/hft-oms/tests/model.rs` 中 SendReport 处理表的朴素实现(按 `(op, status)` 查表)
- 例数:PR 2 000
- 实现:`crates/hft-oms/tests/prop_send_report.rs::b_1_unwritten_send_report_keeps_target_state`
- 备注:链接腿的本地发送失败计次见 FP-2 与 hft-oms 的 `TESTS.md`

### B-2 冷启动后每个非终态槽都收到 MirrorRestore 并查询,无法查询的槽保持 Unknown 并转交 reconciler

- 规则:[恢复后的 gw 映射](../../crates/hft-oms/README.md#恢复后的-gw-映射)、[Unknown 收敛流程](../../crates/hft-oms/README.md#unknown-收敛流程)、[冷启动恢复(RecoverDriver)](../../crates/hft-sequencer/README.md#冷启动恢复recoverdriver)、[Unknown 订单收敛](../../apps/reconciler/README.md#unknown-订单收敛)
- 级别:integ
- 里程碑:M5
- 门槛:④、⑤
- 前置:W1;`VP` 的 venue 行为层对 `Query` 一律回 `SendReport{Query, QueryUnsupported}`;`unknown_query_interval_ms = 1000`;崩溃前 a 有两个非终态槽:`VS` 上 s1(IOC 买 0.01,`PendingNew`,`exch_order_id = 0`),`VP` 上 s2(IOC 卖 0.01,`Live`,已部分成交 0.004,`exch_order_id = 7001`)
- 步骤:
  1. 崩溃;重启;sequencer 处理 `EngineClock`
  2. `VS`、`VP` 的 `ConnState{Order, Up}` 到达
  3. `VP` 对 s2 的查询回 `SendReport{Query, QueryUnsupported}`
  4. `ReconcileReport` 含 `UnknownResolved{slot = s2, epoch, gen 与 s2 相同, order_status = Canceled, exch_qty = 0.006}`
- 期望:
  - 步骤 1:每个 gw 先收到 `GwCmd{SetEpoch{epoch = 新 epoch, accept_prev = true}}`,随后 `VS` gw 收到一条 `GwCmd{MirrorRestore{slot = s1, gen, epoch = 上一 epoch, inst = S1, exch_order_id = 0}}`,`VP` gw 收到一条 `{slot = s2, …, exch_order_id = 7001}`;s1、s2 为 `Unknown`
  - 步骤 2:同一 Input 内对 s1、s2 各发一条 `OrderDecision{Query}`
  - 步骤 3:s2 保持 `Unknown`,`query_count` 不变,写 `Note{OmsNeedsRestQuery}`(指向 s2)
  - 步骤 4:s2 先应用成交增量 0.002 再 → `Cancelled`(`filled = 0.006`);`VP` 的 `venue_ready` 恢复时写 `Note{OmsVenueReady}`
- 实现:`tests/recover/mirror_restore.rs::b_2_nonterminal_slots_mirrored_and_queried`

| 行 | 相对主例的变化 | 断言 |
|---|---|---|
| B-2.1 | `VS` 上有 300 个非终态槽 | `VS` gw 收到 300 条 `MirrorRestore`,每个槽恰好一条;单次 `mirror_restore` 至多 256 条,其余经扇出队列在后续 `Timer{OmsContinue}` 中交出;全部交出之前不丢弃、不重复 |
| B-2.2 | 步骤 2 中 `VS` 的连接到 t = +5 s 才 Up | 连接 Down 期间不发 `Query`(每秒的 `Timer{Oms, UnknownQuery}` 只重新登记);Up 的那条 Input 内对 s1 发一条 `Query` |

### B-3 旧代数的回报在重启后按 (epoch, slot, gen) 匹配,不迁移占用同一槽号的新单

- 规则:[回报匹配与成交增量](../../crates/hft-oms/README.md#回报匹配与成交增量)、[墓碑与迟到成交](../../crates/hft-oms/README.md#墓碑与迟到成交c1)、[ClientOrderId](../../crates/hft-oms/README.md#clientorderid)、[冷启动恢复(RecoverDriver)](../../crates/hft-sequencer/README.md#冷启动恢复recoverdriver)
- 级别:integ
- 里程碑:M5
- 门槛:④、⑤
- 前置:W1;`slots[(VS, a)] = 1`(只有槽 s1),`reuse_quarantine_ms = 5000`,`tombstone_gens = 2`,`tombstone_keep_ms = 3600000`;`DurableMeta.engine_epoch` = 11;s1 的第 1 代(epoch 11、gen 1)为 IOC 买 0.01,终态 `Cancelled`、`filled = 0.005`、`exch_order_id = 9001`
- 步骤:
  1. t = 5.1 s 的 `Timer{Oms, Sweep}` 回收 s1,写入墓碑(gen 1,`filled = 0.005`);t = 6 s 崩溃;重启
  2. 新单占用 s1(epoch 12、gen 2,IOC 买 0.01),`Ack` 后为 `Live`
  3. 到达 `OrderEvent{PartialFill, epoch = 11, slot = s1, gen = 1, cum_filled = 0.007, exch_order_id = 9001}`
  4. 到达 `OrderEvent{PartialFill, epoch = 11, slot = s1, gen = 1, cum_filled = 0.007}`(重复)
- 期望:
  - 步骤 1:`view().engine_epoch()` 在 `EngineClock` 之前 = max(快照 11, DurableMeta 11) + 1 = 12;每个 gw 的第一条订单类 `GwCmd` 之前已有 `GwCmd{SetEpoch{epoch = 12, accept_prev = true}}`
  - 步骤 3:命中墓碑:`FillOut{qty = 0.002, flags = LATE}` 入账,按墓碑的 group 与腿更新 LegTracker,`Note{OmsLateFill}`(P0);当前槽(12, s1, 2)的状态、`filled` 与拥有者均无变化,不产生任何 `OrderDecision`
  - 步骤 4:按重复处理(`d ≤ 0`),无 `FillOut`、无新 Note
- 实现:`tests/recover/epoch.rs::b_3_old_generation_report_hits_tombstone`
- 备注:`DurableMeta.engine_epoch` 在第一笔订单前 `write_sync` 由 hft-core 启动步 5 执行,用例归 apps/hft-core 的 `TESTS.md`

| 行 | 相对主例的变化 | 断言 |
|---|---|---|
| B-3.1 | 步骤 3 发生在墓碑过期之后(引擎时间超过 `tombstone_keep_ms`) | 按 `StaleGen`:`ForeignFill{qty = 0.002, exch_order_id = 9001}` 由 sequencer 在本 Input 内构造为 `ExternalFill{cause = Unknown}` 入账;`Note{OmsStaleGenFill}`(P0,`v[2] = 1`)与 `Note{OmsForeignFill}`(`path` = 2)各一条;当前槽无变化 |
| B-3.2 | 同 B-3.1,但回报的 `exch_order_id = 0` | 不入账;`Note{OmsStaleGenFill}`(P0,`v[2] = 0`);无 `Note{OmsForeignFill}` |

### B-4 第二腿在途或有抑制条件时补腿检查不发自动 HedgeFix、不计截止

- 规则:[补腿](../../crates/hft-exec/README.md#补腿hedgefixc4)、[LegTracker](../../crates/hft-oms/README.md#legtracker)、[人工保持与恢复期抑制](../../crates/hft-exec/README.md#人工保持与恢复期抑制)、[Broken 处置](../../crates/hft-exec/README.md#broken-处置)
- 级别:dst
- 里程碑:M2
- 门槛:④
- 前置:W1;实例 1 为 probe-target,g1 目标 `S1` = +0.01、`P1` = −0.01 BTC(`PairOnFill`);`fix_tolerance_default = 0.05`,`dust_notional = 10 USDT`,`fix_cooldown_ms = 500`,`fix_max = 3`,`fix_deadline_ms = 5000`
- 故障:t = 0 第一腿 `S1` 买 0.01 全部成交(回报不带费用,`fee_asset = ASSET_NONE`);`MockVenue` 把第二腿(`P1` 卖 0.01 IOC)的全部回报延迟 800 ms;t = 0.1 s 第一腿到达一条 `OrderEvent{FeeOnly, fee = 0.00001, fee_asset = BTC}`(LegTracker 变化触发补腿检查)
- 种子:标准集(§2.4)
- 断言:
  - 0 < t < 0.8 s:g1 的 `LegView.naked_qty` = 0.00001(第二腿在途计入裸量),`open_links = 1`;执行层无 `ExecDecision{HedgeFix}`,cell 的 `fix_first_tsc = 0`、`fix_count = 0`
  - t = 0.8 s 第二腿全部成交后链接 `Done`;已成交裸量 0.00001 × 60,000 = 0.6 USDT ≤ `dust_notional`,到场景结束无 `HedgeFix`
  - 每步 INV-8、INV-15、INV-19
- 实现:`crates/hft-sequencer/tests/scenarios_hedge.rs::b_4_inflight_second_leg_suppresses_hedge_fix`
- 备注:`Broken` 的有界自动平仓见 BK-1,恢复期抑制的完整场景见 BK-5;B-4.1、B-4.4 在同一函数,B-4.3 见 `b_4_3_broken_group_no_hedge_fix`;B-4.2、B-4.5 待 W1 种子化 DST;实现方式同 B1(真实 sequencer 在 B0 夹具上的确定性步骤序列,映射与阈值见该文件头)

| 行 | 相对主例的变化 | 断言 |
|---|---|---|
| B-4.1 | 第二腿在 t = 0.8 s 零成交 `Expired`,HedgeRetry 的 `SecondRetry` 在 t = 0.9 s 发出、t = 1.7 s 才成交 | 0.8–1.7 s 之间已成交裸量 0.01(比例 1.0 > 0.05)但链接仍为 `Hedging`:无 `HedgeFix`,`fix_first_tsc = 0` |
| B-4.2 | 第二腿 `SecondRetry` 发出前 `VP` 下单连接断开,第二腿槽为 `PendingNew` → `Unknown` | `has_unknown` 期间无 `HedgeFix`、`fix_first_tsc = 0`;收敛后才按触发条件判断 |
| B-4.3 | 第二腿持续被拒直到 `hedge_deadline_ms = 3000` 到期,g1 `Broken` | g1 处于 `Broken` 期间执行层不发 `HedgeFix`(只有自动有界平仓路径,见 BK-1) |
| B-4.4 | t = 0.9 s 到达 g1 的 `ExternalFill{cause = Manual}`(`P1` 买 0.01,平掉对冲腿) | `manual_hold` 置位(`Note{ExecManualHold}` P1);此后无 `HedgeFix`、`ResidualTrim`、`ExternalDefault`,`FixState` 为零 |
| B-4.5 | 崩溃后重启,g1 已成交裸量 0.01(现货多出,无活动链接、无在途),`VS` 满足 `hedge_gate_ok` 而 `RECONCILED` 未置位 | `VS` 放行前无 `HedgeFix`,`fix_first_tsc` 照记(≠ 0)但不判截止(无 `Broken{FixExhausted}`);`on_venue_gate_open(a, VS)` 的同一 Input 内 `fix_first_tsc` 与 `fix_count` 清零,之后的推进中按触发条件发出第一条 `HedgeFix` |

### B-5 第二腿被拒后行情静止时由 HedgeRetry 定时器重试,门控拒绝不计次但累计时长

- 规则:[OnFill](../../crates/hft-oms/README.md#onfill)、[LegLink 与 HedgePlan](../../crates/hft-oms/README.md#leglink-与-hedgeplan)、[对冲定价](../../crates/hft-oms/README.md#对冲定价)、[检查链](../../crates/hft-risk/README.md#检查链按廉价--昂贵短路适用范围以本表为唯一定义)
- 级别:dst
- 里程碑:M2
- 门槛:④
- 前置:W1;实例 1 为 probe-taker;`hedge_retry_min_ms = 100`,`hedge_retry_max = 3`,`hedge_deadline_ms = 3000`,`conn_liveness_ms = 2000`,`bbo_ping_ms = 1000`
- 故障:t = 0 起 `S1`、`P1` 的 BBO 不再变化,两条 BBO 连接每 1000 ms 回 pong;t = 0 第一腿 `S1` 买 0.01 全部成交;第二腿及其每次重试都被交易所以已登记的拒单码 `Reject`
- 种子:标准集(§2.4)
- 断言:
  - 第二腿被拒后恰好发出 3 条 `SecondRetry`,每条的 `OrderDecision.trigger_seq` 指向一条 `Timer{Oms, HedgeRetry}` Input,相邻两条的引擎时间间隔 ≥ 100 ms;它们之间没有任何行情 Input 改变 `S1`、`P1` 的报价
  - 第 3 条 `SecondRetry` 被拒的同一 Input 内:链接 `Broken{RetryExhausted}`,`Note{OmsHedgeBroken}`(P0),`Broken{a, g1}` 与 `HedgeFailure{a}` 的 `CapSet`
  - 每步 INV-3、INV-8、INV-14
- 实现:`crates/hft-sequencer/tests/scenarios_hedge.rs::b_5_hedge_retry_timer_drives_retries`
- 备注:B-5.2 在同一函数,B-5.1 见 `b_5_1_hard_kill_gates_retries_until_deadline`;实现方式同 B1(真实 sequencer 在 B0 夹具上的确定性步骤序列,映射与阈值见该文件头)

| 行 | 相对主例的变化 | 断言 |
|---|---|---|
| B-5.1 | 第一腿 IOC 在 t = −0.05 s 发出,t = −0.01 s 设 `Kill{Account(a), hard}`,第一腿成交回报 t = 0 到达;第二腿与每次重试都在风控 #1 被拒 | 每条 `Timer{Oms, HedgeRetry}` 恰好产生一条 `OrderDecision{link_role = SecondRetry, reject = HARD}`,无出环帧;链接 `retries` 保持 0;t = 2.9 s 时链接仍为 `Hedging`,t = 3.2 s 时已 `Broken{Deadline}`,`Note{OmsHedgeBroken}`(P0) |
| B-5.2 | 第二腿零成交 `Expired`(IOC 未全成)而非 `Reject` | 与主例相同:3 条 `SecondRetry` 后 `Broken{RetryExhausted}` |

### B-6 venue 未就绪只拒 Open,对冲与减仓放行

- 规则:[检查链](../../crates/hft-risk/README.md#检查链按廉价--昂贵短路适用范围以本表为唯一定义)、[分类](../../crates/hft-risk/README.md#分类)、[Unknown 收敛流程](../../crates/hft-oms/README.md#unknown-收敛流程)
- 级别:table
- 里程碑:M2
- 门槛:④
- 前置:W1 的风控参数;`unknown_blocks_open = false`;(`VS`, a) 有一个 `Unknown` 槽(`venue_ready(VS, a) = false`),(`VP`, a) 就绪;账户 a 的 g1 持有 `S1` +0.01、`P1` −0.01;其余检查(报价、名义、仓位、令牌)的输入都满足;逐行调用 `risk.classify` + `risk.evaluate`
- 实现:`crates/hft-risk/tests/venue_ready.rs::b_6_venue_not_ready_blocks_open_only`

| 行 | 意图 | 期望 |
|---|---|---|
| B-6.1 | 策略 `place`:`S1` 买 0.01 IOC(Open,无链接) | `Err(VENUE_NOT_READY)` |
| B-6.2 | OMS `Second`(开仓链接,`reduce_only = 0`):`S1` 买 0.01 | `Ok`(Hedge,#4 不适用) |
| B-6.3 | OMS `SecondRetry`,同 B-6.2 | `Ok` |
| B-6.4 | 执行层 `HedgeFix`:`S1` 卖 0.005 | `Ok` |
| B-6.5 | 策略 `place`:`S1` 卖 0.005(Reduce,≤ 持仓) | `Ok` |
| B-6.6 | 执行层 Flatten(引擎目标 `reason = Hard`):`S1` 卖 0.01 | `Ok` |
| B-6.7 | 策略 `place`:`P1` 卖 0.01 Open,链接腿在 `VS`(`link_venue = VS`) | `Err(VENUE_NOT_READY)` |
| B-6.8 | 策略 `place`:`P1` 卖 0.01 Open,无链接 | `Ok` |
| B-6.9 | 冒烟单(`origin = Command`,`SMOKE`):`S1` 买,PostOnly | `Err(VENUE_NOT_READY)` |

### B-8 换 boot_id 或 TSC 频率的冷启动按停机不计平移全部已登记 TSC 量

- 规则:[定时器](../../crates/hft-sequencer/README.md#定时器)、[快照与恢复](../../crates/hft-exec/README.md#快照与恢复)、[恢复:停机时间不计与闸门续接](../../crates/hft-oms/README.md#恢复停机时间不计与闸门续接)、[令牌桶](../../crates/hft-risk/README.md#令牌桶)
- 级别:integ
- 里程碑:M5
- 门槛:④、⑤
- 前置:W1;上次运行 `tsc_hz_milli` = 2,500,000,000,000;崩溃时(`stop_tsc` 处)堆中 `Timer{Oms, HedgeRetry}` 在 `stop_tsc` 之后 80 ms、`Timer{Exec, Fix}` 在之后 300 ms、`Timer{Exec, Wake}` 在之前 50 ms(未处理);一个墓碑的 `freed_tsc` 在之前 1000 ms;g1 的 `LegState.last_fill_tsc` 在之前 2000 ms;(`VS`, a) 的 `tokens_10s` = 7
- 步骤:
  1. 崩溃;停机 2 h;在另一台主机上重启:`boot_id` 不同,`tsc_hz_milli` = 3,000,000,000,000,恢复结束时 `now = run_tsc()` = 4,000,000,000
- 期望:
  - `StateSnapshot{purpose = Recovery}` 头 `snap_tsc` = 4,000,000,000
  - 堆:`HedgeRetry` 的 `deadline_tsc` = 4,240,000,000,`Fix` = 4,900,000,000,`Wake` = 3,850,000,000(各 ±1);`Wake` 在 `EngineClock` 所在轮的下一轮作为 Input 交出
  - OMS:墓碑 `freed_tsc` = 1,000,000,000;`last_fill_tsc` = 1(按公式为负,取下限 1);值为 0 的已登记字段仍为 0
  - `OmsSweep` 在堆中恰好一条(快照中的旧条目被替换)
  - (`VS`, a) 的 `tokens_10s` 在 `EngineClock` 之后第一条 Input 处 = 7 + (该 Input 的 engine_ns − 崩溃前末条的 engine_ns)× 补充速率;停机 2 h 不补充
- 实现:`tests/recover/tsc_shift.rs::b_8_boot_id_change_shifts_registered_tsc`

| 行 | 相对主例的变化 | 断言 |
|---|---|---|
| B-8.1 | 同开机重启:`tsc_hz_milli` 不变,`now` = `stop_tsc` + 停机时长的 tick 数 | 按同一公式:`HedgeRetry` = now + 200,000,000、`Fix` = now + 750,000,000、`Wake` = now − 125,000,000;剩余时长与主例相同(80 ms、300 ms、已过期) |

### B-10 kill 或 Drain 的撤单义务期间断线重连,每次 Up 与每次收敛都重发撤单

- 规则:[模式与 cap 的驱动](../../crates/hft-sequencer/README.md#模式与-cap-的驱动)、[输出扇出](../../crates/hft-oms/README.md#输出扇出)、[Kill](../../crates/hft-risk/README.md#kill)、[Drain](../../crates/hft-sequencer/README.md#drain)
- 级别:dst
- 里程碑:M2
- 门槛:④
- 前置:W1;`VS`、`VP` 的 `VenueCaps.flags` 不含 `BATCH_CANCEL`;`cancel_timeout_ms = 2000`;t = −1 s 两条 Admin `SmokeOrder`(已确认,`ttl_ms = 60000`)在 a 的 `VS`、`VP` 上各挂一张冒烟单 m1、m2,均为 `Live`
- 故障:t = 0 `Kill{Account(a), cancel_resting = true}`(非 HARD);t = 0.01 s(m1 的撤单已发出、`CancelAck` 之前)`VS` 下单连接断开,t = 3 s 重连;m2 的撤单回 `CancelReject`(非"已成交/不存在"原因),t = 5 s `VP` 下单连接断开、t = 6 s 重连;t = 10 s `Resume{Kill}`;t = 11 s 新冒烟单 m3 挂在 `VS`;t = 12 s `VS` 断开、t = 13 s 重连
- 种子:标准集(§2.4)
- 断言:
  - t = 0:m1、m2 各一条 `OrderDecision{Cancel}`(逐槽,`open_only = true` 范围)
  - m1:断开时 `PendingCancel` → `Unknown`;t = 3 s 的 `ConnState{Order, Up}` 内发 `Query`;查询收敛为 `Live` 的同一 Input 内再发一条 `OrderDecision{Cancel}`;随后 `CancelAck` → `Cancelled`
  - m2:`CancelReject` 后为 `Live`;t = 6 s 的 `ConnState{Order, Up}` 同一 Input 内再发一条 `OrderDecision{Cancel}`,随后 `Cancelled`
  - t = 10 s 之后:t = 13 s 的 `ConnState{Order, Up}` 不为 m3 发撤单,m3 保持 `Live` 直到 `ttl_ms` 到期
  - 每步 INV-5、INV-7
- 实现:`crates/hft-sequencer/tests/scenarios_risk.rs::b_10_cancel_obligation_reissued_on_reconnect`
- 备注:B-10.1 在同一函数;撤单持续义务按账户重发(任一 venue 的 Up 都重发该账户全部应撤的单),所以 m2 的 CancelReject 之后 `VS` 在 t = 3 s 的 Up 即重撤 m2,`VP` 断线时它进 `Unknown`,t = 6 s 的 Up 发查询,查询收敛为 Live 的同一 Input 内再撤;实现方式:以真实 sequencer 的确定性步骤序列在 hft-sequencer B0 夹具上实现,映射 A≙`VS`、B≙`VP`、a1≙`S1`、b1≙`P1`、账户 1/2≙`a`/`b`,价格与阈值取 B0(见该文件头);W1 种子化 DST 版本待测试策略能发出 S1 买入第一腿后补

| 行 | 相对主例的变化 | 断言 |
|---|---|---|
| B-10.1 | t = 0 改为 `Drain{cancel_resting = true}`;a 另有一张链接第二腿 IOC 在途(`RISK_REDUCING`,`PendingNew`) | 撤单范围不限于非 `RISK_REDUCING`(`open_only = false`),该 IOC 同样收到 `OrderDecision{Cancel}`;m1、m2 的重发规则同主例;全部槽终态且 `exec.drain_ready()` 为真后 Drain 以 `Done` 结束 |

### B-12 只带费用的回报只累加费用并按净量补差 ledger 与 LegTracker,不迁移状态

- 规则:[成交净量](../../crates/hft-oms/README.md#成交净量b4)、[回报处理表](../../crates/hft-oms/README.md#回报处理表)、[成交增量与净量](../../crates/hft-ledger/README.md#成交增量与净量)、[墓碑与迟到成交](../../crates/hft-oms/README.md#墓碑与迟到成交c1)
- 级别:prop
- 里程碑:M2
- 门槛:④
- 性质:任意成交与 `FeeOnly` 的交错下(含 `FeeOnly` 晚于槽回收、命中墓碑或代数不符):`FeeOnly` 从不改变任何槽的 `OrderState`;每条 `FeeOnly` 恰好产出一条 `FeeOut`;命中槽或墓碑且费用资产为该 inst 基础资产时,ledger 库存与 LegTracker 该腿 `net` 在同一 Input 内同量补差(现货买、卖都是 `delta = −fee`);找不到槽与墓碑时 `FeeOut.group = GROUP_NONE` 并写 `Note{OmsFeeUnattributed}`(P1),无净量补差;全部回报处理完后每个 (account, group, inst) 的 ledger 持仓 = LegTracker 该腿 `net`;每步 INV-1、INV-16(账本部分)
- 生成:1–32 张 `S1` 现货 IOC 买或卖(W1,`fee_asset_rule = ReceivedAsset`),每张 1–4 笔成交回报,其中 50% 不带费用(`fee_asset = ASSET_NONE`)并各配一条 `FeeOnly`(费用 0–0.1% × qty,BTC 与 USDT 各半);`FeeOnly` 插在成交之后的随机位置(含槽回收之后、墓碑过期之后);10% 的 `FeeOnly` 带错误 gen;墓碑过期之后与 gen 错误的 `FeeOnly` 只用 USDT 计费
- 参考:净量规则的朴素实现(买 `net = qty − fee_base`、卖 `net = qty + fee_base`,按 (account, inst) 累加)
- 例数:PR 1 000
- 实现:`crates/hft-sequencer/tests/prop_fee_only.rs::b_12_fee_only_adjusts_net_without_transition`

### B-14 对账报告之后该 inst 又有成交时 AdoptExchange 以 StaleReport 拒绝

- 规则:[命令路径](../../crates/hft-sequencer/README.md#命令路径)、[人工干预的采纳](../../crates/hft-ledger/README.md#人工干预的采纳adoptexchange)、[ReconcileReport](../../apps/reconciler/README.md#reconcilereport)、[结果码](../subsystems/control-plane.md#结果码)
- 级别:integ
- 里程碑:M9
- 门槛:④
- 前置:W1;账户 a 在 `VP` 的 `P1` 引擎仓位 −0.010 BTC(属 g1),交易所仓位 −0.012 BTC
- 步骤:
  1. `ReconcileReport` 组 501(`view_seq` = S0)含 `Position{P1, engine_qty = −0.010, exch_qty = −0.012}`
  2. seq > S0 的 `OrderEvent{PartialFill}`:a 在 `P1` 卖出成交 0.001
  3. `AdoptExchange{a, VP, P1, report_id = 501, Position}`(已确认)
- 期望:
  - 步骤 3:`CmdResult{PreconditionFailed, detail = StaleReport}`(detail 码 310);无 `AdjustDelta`、无 `Note{Adjusted}`;ledger 与 LegTracker 的 `P1` 仓位不变;(`VP`, a) 无 `Gate(Disposed)` 处置
- 实现:`crates/hft-testkit/src/scenarios/recon.rs::b_14_adopt_after_new_fill_is_stale`

| 行 | 相对主例的变化 | 断言 |
|---|---|---|
| B-14.1 | 去掉步骤 2 | `CmdResult{Applied}`;`AdjustDelta{Position, qty = −0.002, px_src = Mark}` 与 `Note{Adjusted}`;g1 的 `P1` 腿净量 −0.012;处置以 `Disposed{Adopted}` 计入闸门 |
| B-14.2 | 报告条目改为 `Balance{BTC}`(`VS`),步骤 2 改为 `S1` 成交 | `CmdResult{PreconditionFailed, detail = StaleReport}` |
| B-14.3 | 步骤 2 的成交在另一 inst `S1` 上 | `CmdResult{Applied}`(陈旧判定只看被采纳的 inst/资产) |

### B-17 跨日停机后启动的首轮写一次 DayRoll,重复或倒序的 DayRoll 被忽略

- 规则:[环境决定写成 Input](../../crates/hft-sequencer/README.md#环境决定写成-input)、[当日亏损(锁存)](../../crates/hft-risk/README.md#当日亏损锁存)
- 级别:integ
- 里程碑:M5
- 门槛:④、⑤
- 前置:W1;`last_day_rolled` = 2026-10-03;崩溃前最后一条 Input 的 UTC = 2026-10-03 23:59:30;`ClockStatus` 为 `Synced`
- 步骤:
  1. 崩溃;停机到 2026-10-05 00:10:00 UTC;重启;实盘驱动跑首轮
  2. t = +5 s:a 在 `S1` 成交使 `day_pnl(a)` = −12.50 USDT
  3. 测试驱动交出 `Timer{Sys, DayRoll{day = 2026-10-05}}`(重复)与 `Timer{Sys, DayRoll{day = 2026-10-04}}`(倒序)
- 期望:
  - 步骤 1:`EngineClock` 所在轮的下一轮恰好一条 `Timer{Sys, DayRoll{day = 2026-10-05}}`,无 `day = 2026-10-04` 的记录;处理后 `last_day_rolled` = 2026-10-05,`day_pnl(a)` = 0,`resumes_today(a)` = 0
  - 步骤 3:两条都被忽略:`last_day_rolled` 仍为 2026-10-05,`day_pnl(a)` 仍为 −12.50 USDT
- 实现:`tests/recover/day_roll.rs::b_17_cross_day_restart_rolls_once`

| 行 | 相对主例的变化 | 断言 |
|---|---|---|
| B-17.1 | 重启时时钟不可信,t = +4 s 才写 `Timer{Sys, ClockOk}` | `ClockOk` 之前无 `DayRoll`;`ClockOk` 所在轮或其下一轮写 `DayRoll{day = 2026-10-05}`,只写一条 |
| B-17.2 | 停机到 2026-10-03 23:59:50 UTC(同一 UTC 日) | 重启后到 UTC 日变化前无 `DayRoll` |

### B-18 journal 段接近写满时先在空闲轮滚动,持续不空闲越过硬上限才在事件边界强制滚动,写满则 Fatal

- 规则:[写者](../../crates/hft-journal/README.md#写者)、[环境决定写成 Input](../../crates/hft-sequencer/README.md#环境决定写成-input)、[journal 满](../../crates/hft-sequencer/README.md#journal-满)、[段滚动与引导块](../subsystems/journal-format.md#段滚动与引导块)
- 级别:integ
- 里程碑:M5
- 门槛:④
- 前置:W1;seq journal `page = Normal`,`segment_bytes = 4194304`,`size_idle_pct = 75`,`size_valve_pct = 90`;persist 替身按需提供预创建段;测试驱动每轮至少交出一条 Input(无空闲轮),直到步骤 2
- 步骤:
  1. 写到 `used × 100 ≥ 75 × segment_bytes` 后再持续 100 轮不空闲
  2. 连续两轮无 Input(两个空闲轮)
  3. 恢复每轮有 Input
- 期望:
  - 步骤 1:`needs_roll` 返回 `(Size, IdleRound)`;100 轮内无 `Timer{Sys, Roll}`
  - 步骤 2 的第二个主循环轮次(上一个轮次空闲、这一轮次开始时输入环全空):a 段交出 `Timer{Sys, Roll}`;旧段末条为 `SegmentRoll{End}`,新段以完整引导块开头(`SegmentRoll{Begin}` … `SegmentRoll{BootDone}`)
- 实现:`tests/recover/journal_full.rs::b_18_segment_rolls_on_idle_then_hard_limit`

| 行 | 相对主例的变化 | 断言 |
|---|---|---|
| B-18.1 | 一直没有空闲轮 | 用量越过 90% 后的下一个事件边界交出 `Timer{Sys, Roll}`(`needs_roll` 返回 `(Size, EventBoundary)`),不等空闲轮 |
| B-18.2 | 越过 90% 时 persist 替身不提供预创建段 | 每条 `Timer{Sys, Roll}` 不写任何记录、不改状态(`roll` 返回 `NextNotReady`),下一个事件边界再写一条;替身提供段后的那一次完成滚动 |
| B-18.3 | 同 B-18.2,替身一直不提供预创建段 | 预留区之外放不下下一条记录时 `append` 返回 `AppendErr::Full`,该条不写任何字节;`step` 返回 `Fatal(JournalFull)`;之后无任何出环帧 |

### B-22 引导块写到一半崩溃时整段删除并从上一段恢复,事件内的 Meta 不置 EVENT_END

- 规则:[段滚动与引导块](../subsystems/journal-format.md#段滚动与引导块)、[提交游标](../subsystems/journal-format.md#提交游标)、[写者](../../crates/hft-journal/README.md#写者)、[恢复与校验](../../crates/hft-journal/README.md#恢复与校验)
- 级别:integ
- 里程碑:M5
- 门槛:④、⑤
- 前置:W1;当前段号 N,其中有关键帧 K
- 步骤:
  1. 整点滚动:旧段 N 写 `SegmentRoll{End}` 并封存;新段 N+1 写到 `StateSnapshot{purpose = Boot}` 之后、`SegmentRoll{BootDone}` 之前崩溃;崩溃前一个从段 N 尾部跟读的读者在运行
  2. 重启
- 期望:
  - 步骤 1:读者未交付段 N+1 的任何记录
  - 步骤 2:段 N+1(热段与已复制的冷副本)整段删除;恢复从段 N 的 K 起步;重开的段号仍为 N+1,`roll_reason = Recovery`,引导块完整;`journal-verify` 对该 journal 无第 4 项(引导块)问题
- 实现:`tests/recover/boot_block.rs::b_22_partial_boot_block_segment_deleted`

| 行 | 相对主例的变化 | 断言 |
|---|---|---|
| B-22.1 | 不崩溃;处理一条新单的首个 `OrderEvent{Ack}`,事件内写出 `Note{OmsTransition}`(Output)之后再写 `LatencySample`(Meta) | 该事件只有一条记录带 `EVENT_END`,即最后一条非 Meta 记录;`LatencySample` 的 flags 不含 `EVENT_END`;事件之外独立写入的 `ClockStatus` 带 `EVENT_END` |
| B-22.2 | 在 B-22.1 的 `LatencySample` 追加之后、`end_event` 之前崩溃 | 整个事件(Input、它的 Output 与 `LatencySample`)被截;恢复后 journal 末条是上一个事件带 `EVENT_END` 的记录 |

## 6. 执行正确性

本节用例在 W1(§2.2)上运行,另用参数基线 **R6**;用例 `前置` 写"R6"并只列出与它不同的值。R6 中 BBO 价位只按用例脚本变化(不做 §2.2 的随机游走),种子只决定回报时延(0–20 ms)与输入交错,使数量与价格断言确定。金额单位 USDT。

- `RiskParams`:`conn_liveness_ms = 2,000`;`max_perp_leverage_x100 = 500`;`l3_max_cross = 0`;`rate_headroom = 0.10`;BTC 价格带类别 `max_px_dev = 0.01`、`ioc_max_cross = 0.002`、`rr_max_cross = 0.004`。账户 `a`、`b` 各一行:`max_order_notional = 5,000`、`max_hedge_notional = 10,000`、`max_reduce_notional = 5,000`、`max_account_notional = 200,000`、`account_notional_from_equity = 10`、`daily_loss_kill = 0.05`、`daily_loss_abs = 5,000`、`daily_loss_max_resumes = 1`、`recon_closeonly_notional = 100`、`unknown_blocks_open = true`、`funds_buffer = 0.05`;保证金 `liq_dist_block_open = 0.25`、`liq_dist_reduce = 0.15`、`liq_dist_force = 0.10`、`liq_dist_hyst = 0.02`、`margin_ratio_crosscheck = 0.80`、`margin_ratio_hyst = 0.05`、`deleverage_deadline_ms = 30,000`、`force_step = 0.20`、`force_step_interval_ms = 5,000`、`margin_stale_ms = 15,000`、`mark_stale_ms = 10,000`。group `g1`、`g2`:`max_group_notional = 50,000`、`max_pos_qty = 2 BTC`、`naked_gate = 0.5`、`naked_gate_min = 1,000`。每 (venue, account):`max_venue_notional = 100,000`、`inst_bucket_per_s = 10`、`backoff_base_ms = 1,000`、`backoff_max_ms = 30,000`、`rr_reserve = 0.20`。
- `ExecConfig`(`a`、`b`):`dust_notional = 5`、`fix_tolerance_default = 0.02`、`fix_max = 3`、`fix_cooldown_ms = 1,000`、`fix_deadline_ms = 10,000`、`broken_auto_fix_ms = 120,000`、`external_deadline_ms = 5,000`、`gate_naked_alarm_ms = 300,000`、`stuck_after_ms = 2,000`、`gated_backoff_ms = 500`、`max_slice_fails = 3`、`max_nofill = 5`、`force_slice = {depth_frac 0.3, depth_levels 5, max_cross 0.004, max_chunk_notional 1,000, min_chunk_notional 0, cooldown_ms 200}`。
- `OmsConfig`:`hedge_deadline_ms = 3,000`、`hedge_retry_max = 3`、`hedge_retry_min_ms = 100`、`resume_hold_max_ms = 30,000`、`ack_timeout_ms = 2,000`、`cancel_timeout_ms = 2,000`、`ioc_timeout_ms = 1,000`、`unknown_query_interval_ms = 1,000`、`unknown_max_queries = 5`、`unknown_alarm_ms = 30,000`、`not_found_grace_ms = 2,000`、`reuse_quarantine_ms = 5,000`、`tombstone_gens = 2`、`tombstone_keep_ms = 3,600,000`。
- 其他:hft-venue `bbo_ping_ms = 1,000`、`gw.unmatched_hold_ms = 5,000`;hft-ledger `funding_tol_abs = 0.05`、`funding_tol_rel = 0.005`、`funding_recon_lag_ms = 600,000`、`funding_recon_window_ms = 172,800,000`;hft-core `shutdown.drain_timeout_ms = 20,000`;ECF1 段 5 `auto_resume_after_clean_drain = true`。
- 链接开仓:写作"`probe-taker` 在 t 发 S1 买 q 带开仓链接"时,意图为 `OrderIntent{inst S1, side 买, qty q, tif Ioc, 限价 60,000.00, LegLink{role First, tolerance 0.02, hedge HedgePlan{kind OnFill, venue VP, account a, inst P1, side 卖, px_mode Cross, px_param 0.002, max_slip 0.002, ratio 1/1, on_fail Retry, reduce_only 0, residual Defer, tif Ioc}}}`;第二腿默认全部成交。

### B1 第二腿 venue 被 kill 时带 LegLink 的开仓整笔被拒且第一腿不发

- 规则:[检查链](../../crates/hft-risk/README.md#检查链按廉价--昂贵短路适用范围以本表为唯一定义)、[风控所需的链接信息(B1)](../../crates/hft-oms/README.md#风控所需的链接信息b1)、[cap 记录](../../crates/hft-risk/README.md#cap-记录)、[意图管线](../../crates/hft-sequencer/README.md#意图管线)
- 级别:dst
- 里程碑:M2
- 门槛:④
- 前置:W1、R6;`probe-taker` 在 t = 1 s 发 S1 买 0.02 BTC 带开仓链接;账户 `b` 的实例 2 在同一时刻发同构意图(g2)
- 故障:t = 0.5 s `CtlActor` 投递 `Kill{Venue(VP), account a, cancel_resting = true}`(Trader,已确认)
- 种子:标准集(§2.4)
- 断言:
  - t = 1 s 那条意图只产生一条 `OrderDecision{inst S1, reject = KILLED}`;从 t = 1 s 到场景结束,`VS`、`VP` 的出环帧中无账户 `a` 的 `Place`;账户 `a` 的 `InflightBook.open_buy[(a, S1)] = 0`,`VS`、`VP` 上账户 `a` 的三个令牌桶余量与 t = 1 s 之前相同
  - 账户 `b` 的意图 `OrderDecision.reject = 0`,第一腿出环,第二腿在其成交的同一 Input 内出环
  - 每步 INV-3、INV-4、INV-5 成立
- 实现:`crates/hft-sequencer/tests/scenarios.rs::b1_link_open_rejected_when_hedge_venue_killed`
- 备注:B1.2、B1.3、B1.6 另见同文件 `b1_2_…`、`b1_3_…`、`b1_6_…`;实现方式:先以真实 sequencer 的确定性步骤序列在 hft-sequencer B0 夹具上实现,映射 A≙`VS`、B≙`VP`、a1≙`S1`、b1≙`P1`、账户 1/2≙`a`/`b`,阈值取 B0 参数,见该文件头;W1 种子化 DST 版本待测试策略能发出 S1 买入第一腿后补

| 行 | 相对主例的变化 | 断言 |
|---|---|---|
| B1.1 | 不发 Kill;t = 0.5 s `VP` 账户 `a` 下单连接 `ConnState{Order, Down}` | 第一腿 `reject = VENUE_NOT_READY`,无出环 |
| B1.2 | 不发 Kill;t = 0.5 s 账户 `a` 在 `P1` 上的一张 g1 单腿 IOC(无链接)因下单连接断而进入 `Unknown`,t = 0.6 s 连接恢复 | t = 1 s 第一腿 `reject = VENUE_NOT_READY`(#4 中先于 `UNKNOWN_PENDING`);该槽经查询收敛为 `Cancelled` 之后的下一条同构意图 `reject = 0` 并出环 |
| B1.3 | 不发 Kill;t = 1 s 时 `VP` 账户 `a` 的 `tokens_10s` 余量分别为 容量 × 0.20 − 1、容量 × 0.20、容量 × 0.20 + 1,不在退避期(三次运行) | 前两次第一腿 `reject = RATE`、无出环;第三次 `reject = 0`,`VP` 的桶不因第一腿扣减(只查不扣) |
| B1.4 | 不发 Kill;`VP` 的 BBO 连接自 t = −3 s 起无任何帧 | `reject = LINK_STALE`(边界见 LK-1) |
| B1.5 | 不发 Kill;t = 1 s 时 `VP` 账户 `a` 的可用保证金为 100 | `reject = LINK_FUNDS`(边界见 LK-2) |
| B1.6 | 不发 Kill;t = 0.5 s 账户 `a` 的 g1 因上一条链接第二腿持续被拒、超过 `hedge_deadline_ms` 而 `Broken` | 同一 Input 内 `CapSet{Broken, (a, g1)}` 与 `CapSet{HedgeFailure, a}`;t = 1 s 起账户 `a` 的任何 Open(含无链接的单腿 Open)`reject = CLOSE_ONLY`,账户 `b` 不受影响;`Broken` 仍在时 `Resume{HedgeFailure, a}` 返回 `CmdResult{Rejected, detail = BrokenRemaining}`;以 `HedgeFix` 命令平掉多出的一腿、`CapCleared{Broken}` 之后同一 `Resume` 返回 `Applied`,其后的 Open `reject = 0` |

### B2 开仓链接的第二腿 IOC 未成交后以 SecondRetry 重试且不带 reduceOnly

- 规则:[LegLink 与 HedgePlan](../../crates/hft-oms/README.md#leglink-与-hedgeplan)、[OnFill](../../crates/hft-oms/README.md#onfill)、[RISK_REDUCING 判定](../../crates/hft-oms/README.md#risk_reducing-判定)、[分类](../../crates/hft-risk/README.md#分类)、[补腿(HedgeFix,C4)](../../crates/hft-exec/README.md#补腿hedgefixc4)
- 级别:dst
- 里程碑:M2
- 门槛:④
- 前置:W1、R6;账户 `a` 在 `P1` 仓位 0;`probe-taker` 在 t = 1 s 发 S1 买 0.02 带开仓链接
- 故障:第一腿 t = 1.05 s 全部成交 0.02;第一条第二腿(P1 卖 0.02 IOC)在 t = 1.10 s 回 `Expired`、`cum_filled = 0`;之后的重试单全部成交
- 种子:标准集(§2.4)
- 断言:
  - 第二腿 `OrderDecision{link_role Second, inst P1, side 卖, qty 0.02}` 与第一腿成交在同一 Input 内,`oflags` 含 `RISK_REDUCING`、不含 `REDUCE_ONLY`
  - `Expired` 的 Input 内登记 `Timer{Oms, HedgeRetry}`,到期时刻 ≥ 该 Input 的 tsc + 100 ms;到期 Input 内恰好一条 `OrderDecision{link_role SecondRetry, qty 0.02, reject = 0}`,`oflags` 含 `RISK_REDUCING`、不含 `REDUCE_ONLY`,出环帧不带交易所 reduceOnly 参数
  - 从 t = 1 s 到场景结束无 `reject = REDUCE_INVALID`、无 `MockVenue` 以 reduceOnly 为由的拒单;链接最终 `LinkUpdate{Done}`,`LinkEntry.retries = 1`
  - 全程无 `ExecDecision{action HedgeFix}`、无 `link_role = HedgeFix` 的 `OrderDecision`
  - 每步 INV-8、INV-15、INV-19 成立
- 实现:`crates/hft-sequencer/tests/scenarios.rs::b2_open_link_retry_is_second_retry_not_reduce_only`
- 备注:B2.1 见 `b2_1_…`;实现方式:先以真实 sequencer 的确定性步骤序列在 hft-sequencer B0 夹具上实现,映射 A≙`VS`、B≙`VP`、a1≙`S1`、b1≙`P1`、账户 1/2≙`a`/`b`,阈值取 B0 参数,见该文件头;W1 种子化 DST 版本待测试策略能发出 S1 买入第一腿后补

| 行 | 相对主例的变化 | 断言 |
|---|---|---|
| B2.1 | 平仓链接:场景开始前 g1 持 S1 +0.02、P1 −0.02;策略发 S1 卖 0.02 带链接,`HedgePlan{side 买, reduce_only = 1}` | 第二腿与 `SecondRetry` 的 `oflags` 都含 `RISK_REDUCING` 与 `REDUCE_ONLY` |
| B2.2 | 第二腿与之后每条重试都 IOC 零成交 | `SecondRetry` 恰好 3 条;第 4 次失败的 Input 内链接 `Broken{RetryExhausted}`、`Note{OmsHedgeBroken}`(P0)、`CapSet{Broken, (a, g1)}` 与 `CapSet{HedgeFailure, a}`;此后无该链接的重试 |

### B3 自身 ADL 外来成交入账后在 external_deadline_ms 到期时平掉多出的一腿

- 规则:[外部成交](../../crates/hft-ledger/README.md#外部成交externalfill)、[外来成交(B3)](../../crates/hft-oms/README.md#外来成交b3)、[外来成交的默认处理(B3)](../../crates/hft-exec/README.md#外来成交的默认处理b3)、[每条 Input 的处理顺序](../../crates/hft-sequencer/README.md#每条-input-的处理顺序process)
- 级别:dst
- 里程碑:M4
- 门槛:④
- 前置:W1、R6;场景开始前 g1 经一条已 `Done` 的链接持 S1 +0.02、P1 −0.02,g1 无目标;`probe-taker` 参数设为收到 `External` 不动作
- 故障:t = 1 s `MockVenue` 推送账户 `a` 在 `VP` 的 `ExternalFill{cause Adl, inst P1, side 买, qty 0.02, px 60,010.0, trade_id 7001}`
- 种子:标准集(§2.4)
- 断言:
  - t = 1 s Input 内:ledger `Note{ExternalFill}`(P0,`cause = Adl`,group g1、P1 腿);P1 仓位 0;LegTracker g1:S1 net +0.02、P1 net 0;`ExecDecision{ExternalSeen}`;实例 1 收到 `EventRef::External`,其 `deadline_tsc` = 该 Input 的 tsc + 5,000 ms 的折算值;登记 `Timer{Exec, External}`
  - t ∈ [1 s, 6 s) 无 `link_role = HedgeFix` 的 `OrderDecision`;t = 6 s 的 `Timer{Exec, External}` Input 内:`ExecDecision{ExternalDefault}`、`Note{ExecExternalDefault}`(P0)、一条 `OrderDecision{inst S1, side 卖, qty 0.02, tif Ioc, link_role HedgeFix}`,`oflags` 含 `RISK_REDUCING`
  - 该单成交后 g1 已成交裸量为 0;场景结束前无第二条 `ExecDecision{ExternalDefault}`
  - 每步 INV-1、INV-15、INV-19 成立
- 实现:`crates/hft-testkit/src/scenarios/exec_correctness.rs::b3_adl_external_fill_default_flatten_after_deadline`

| 行 | 相对主例的变化 | 断言 |
|---|---|---|
| B3.1 | `cause = Liquidation` | 同主例,`Note{ExternalFill}` 的 cause 字段为 `Liquidation` |
| B3.2 | t = 3 s 策略对 g1 `Ctx::place` S1 卖 0.02(Reduce) | t = 3 s Input 内 `ExecDecision{ExternalSeen, detail = External/0/Cleared}`;t ∈ [3 s, 场景结束] 无 `ExecDecision{ExternalDefault}`、无 `Note{ExecExternalDefault}` |
| B3.3 | `cause = Manual` | ledger `Note{ExternalFill}` 为 P1;`Note{ExecManualHold}`(P1,置位);`Note{ExecManualExternal}` 实际级别 Info;不登记 `Timer{Exec, External}`;t ∈ [1 s, 130 s] 无 `ExecDecision{ExternalDefault}`、无 `link_role = HedgeFix` 的单、无 `ExecDecision{AutoFix}` |
| B3.4 | t = 2 s 同一 `trade_id = 7001` 的 `ExternalFill` 再到一次 | `apply_external` 返回 `None`;t = 2 s Input 不产生 `ExecDecision`、不入队 `EventRef::External`;P1 仓位与 LegTracker 与 t = 1 s 之后相同 |

### B4 现货买入以基础币扣费时仓位、库存与对冲量都按净量

- 规则:[成交增量与净量](../../crates/hft-ledger/README.md#成交增量与净量)、[成交净量(B4)](../../crates/hft-oms/README.md#成交净量b4)、[OnFill](../../crates/hft-oms/README.md#onfill)、[TakerSlice](../../crates/hft-exec/README.md#takerslice)、[补腿(HedgeFix,C4)](../../crates/hft-exec/README.md#补腿hedgefixc4)
- 级别:dst
- 里程碑:M2
- 门槛:④
- 前置:W1、R6;`VS` 的 `fee_asset_rule = ReceivedAsset`;`probe-taker` 在 t = 1 s 发 S1 买 0.05 带开仓链接
- 故障:第一腿 t = 1.05 s 全部成交:`qty 0.05`、`fee 0.0001`、`fee_asset = BTC`;第二腿全部成交
- 种子:标准集(§2.4)
- 断言:
  - 第一腿成交 Input 内:`FillDelta.net_qty = 0.0499`;ledger S1 `InstPos.qty = 0.0499`、`inv(BTC, g1) = 0.0499`;LegTracker S1 net +0.0499
  - 同一 Input 内第二腿 `OrderDecision{P1 卖, qty 0.049}`(0.0499 按 lot 0.001 向零取整),`HedgeRequest.hedge_cap_qty = 0.0509`;`residual[g1][P1] = 0.0009`
  - 第二腿成交后已成交裸量 0.0009 BTC(54 USDT > 5)而比例 0.0009 / 0.0499 ≈ 0.018 < 0.02:场景结束前无 `ExecDecision{HedgeFix}`
  - 每步 INV-8、INV-16 成立
- 实现:`crates/hft-testkit/src/scenarios/exec_correctness.rs::b4_base_asset_fee_hedges_net_qty`

| 行 | 相对主例的变化 | 断言 |
|---|---|---|
| B4.1 | 成交回报不带费用(`fee_asset = ASSET_NONE`),t = 1.5 s 到达 `FeeOnly{fee 0.0001, fee_asset BTC}` | 成交 Input 内第二腿 `qty 0.05`(按毛量);`FeeOnly` Input 内 `inv(BTC, g1) = 0.0499`、`NetAdjust.delta = −0.0001`、LegTracker S1 net 0.0499、`residual[g1][P1] = −0.0001`;比例 0.0001 / 0.05 = 0.002 < 0.02,无 `HedgeFix` |
| B4.2 | 主例结束后,账户 `a` 在 `VS` 另有一张已 Ack 的策略 GTC 卖单 0.0100(BTC 可用余额 0.0399);t = 2 s `probe-target` 对 g1 下 `TargetIntent{legs [S1 → 0], exec TakerSlice{max_chunk_notional 5,000}, reason Exit}` | 首片 `OrderDecision{S1 卖, qty 0.0399}`;每片数量 ≤ 发出时 min(`inv(BTC, g1)`, 可用余额) |

### B5 首轮对账有 8 USDT 余额差时启动闸门在处置前不放行

- 规则:[对账闸门与 HARD 解除的严格条件](../../crates/hft-risk/README.md#对账闸门与-hard-解除的严格条件)、[启动闸门](../../crates/hft-risk/README.md#启动闸门)、[干净 Drain 重启自动解除 Recovery](../../crates/hft-risk/README.md#干净-drain-重启自动解除-recovery)、[启动闸门与严格放行](../../apps/reconciler/README.md#启动闸门与严格放行)、[命令路径](../../crates/hft-sequencer/README.md#命令路径)
- 级别:dst
- 里程碑:M5
- 门槛:④
- 前置:W1、R6;从一份干净 Drain 快照冷启动(同版本、尾部为空、无 `Unknown`/`Broken`);t = 2 s 之前账户 `a` 除 `RECONCILED` 外各闸门位已置位;`ReconActor` 对 (a, VP) 在 t = 2 s 提交一组全部 `Ignore` 的报告
- 故障:t = 2 s `ReconActor` 对 (a, VS) 提交 `report_id = 11`:`Summary` + `Balance{asset USDT, engine_qty 100,000, exch_qty 100,008, diff 8, severity Warn}`;t = 5 s `CtlActor` 发 `AdoptExchange{a, VS, item Balance, asset USDT, report_id 11}`(Admin,已确认)
- 种子:标准集(§2.4)
- 断言:
  - t ∈ [2 s, 5 s):账户 `a` 的 `Note{StartupGate}` 新 bits 无 `RECONCILED`,`released = false`;此间 `Resume{Recovery, a}` 返回 `PreconditionFailed{GateNotReleased}`;账户 `a` 的 Open `reject = CLOSE_ONLY`
  - t = 5 s Input 内:ledger `Note{Adjusted}`,USDT `total = 100,008`;`Gate(Disposed{Adopted})` 使 (a, VS) 置 `RECONCILED`;账户 `a` 放行(`Note{StartupGate}` 的 `v[3] = 1`),同一 `apply` 产出 `CapCleared{Recovery}`
  - 每步 INV-11 成立
- 实现:`crates/hft-testkit/src/scenarios/exec_correctness.rs::b5_reconciled_gate_waits_for_disposition`

| 行 | 相对主例的变化 | 断言 |
|---|---|---|
| B5.1 | (a, VS) 的报告只有一条 `Funding` 条目(差 3)与 `Summary`,其余 `Ignore` | t = 2 s 即置 `RECONCILED`;`Funding` 条目不进 `ReconGateInput` |
| B5.2 | 冷启动标 `TAIL_LOST` | t = 5 s 放行,`Recovery{a}` 保留;`Resume{Recovery, a}` 返回 `Applied` 后才 `CapCleared{Recovery}` |
| B5.3 | 冷启动出现 `RecoverMismatch`(见 C2) | 同 B5.2 |
| B5.4 | 放行后 t = 10 s `Kill{Account(a), hard}`(`set_seq = S`);t = 20 s `Resume{Kill}` 时 (a, VS) 最近一组报告的 `view_seq < S` | `PreconditionFailed{ReconNotClean}`;t = 25 s (a, VS)、(a, VP) 各一组 `view_seq > S` 且全部 `Ignore` 的报告之后同一 `Resume{Kill}` 返回 `Applied` |
| B5.5 | 差异条目改为 `Balance{asset BNB}`(BNB 不属于任何 group),t = 5 s 改发 `SetBalanceBaseline{a, VS}` | `Gate(Disposed{Baselined})`,t = 5 s 放行 |
| B5.6 | 差异条目改为孤儿单 `OpenOrder{slot = 0}` | t = 5 s `CancelAll{a, VS}` 之后仍不放行;下一组 (a, VS) 报告全部 `Ignore` 时放行 |

### B6 第二腿价格解码放大 1,000 倍时对冲单被 HEDGE_LIMIT 截住并判 Broken

- 规则:[对冲量上限(B6)](../../crates/hft-exec/README.md#对冲量上限b6)、[检查链](../../crates/hft-risk/README.md#检查链按廉价--昂贵短路适用范围以本表为唯一定义)、[回报匹配与成交增量](../../crates/hft-oms/README.md#回报匹配与成交增量)、[意图管线](../../crates/hft-sequencer/README.md#意图管线)、[令牌桶](../../crates/hft-risk/README.md#令牌桶)
- 级别:dst
- 里程碑:M2
- 门槛:④
- 前置:W1、R6;`probe-taker` 在 t = 1 s 发 S1 买 0.02 带开仓链接
- 故障:第一腿出环后、成交前(t = 1.02 s),`MockVenue` 发出 `P1` 的 BBO 60,010,000.0 / 60,010,100.0(价格按错误单位放大 1,000 倍,连接健康);t = 1.05 s 第一腿全部成交 0.02
- 种子:标准集(§2.4)
- 断言:
  - 成交 Input 内第二腿 `OrderDecision{link_role Second, qty 0.02, reject = HEDGE_LIMIT}`(名义约 1,200,200 > 10,000);出环帧中无 `P1` 单
  - 同一 Input 内链接 `Broken{HedgeLimit}`、`Note{OmsHedgeBroken}`(P0)、`CapSet{Broken, (a, g1)}`、`CapSet{HedgeFailure, a}`;不登记 `HedgeRetry`;场景结束前无该链接的 `SecondRetry`
  - 每步 INV-15 成立
- 实现:`crates/hft-sequencer/tests/scenarios.rs::b6_unit_error_hedge_vetoed_by_hedge_limit`
- 备注:B6.2 见 `b6_2_…`;实现方式:先以真实 sequencer 的确定性步骤序列在 hft-sequencer B0 夹具上实现,映射 A≙`VS`、B≙`VP`、a1≙`S1`、b1≙`P1`、账户 1/2≙`a`/`b`,阈值取 B0 参数,见该文件头;W1 种子化 DST 版本待测试策略能发出 S1 买入第一腿后补

| 行 | 相对主例的变化 | 断言 |
|---|---|---|
| B6.1 | 价格不变;第一腿 IOC(`qty 0.02`)的成交回报 `cum_filled = 20`(数量放大 1,000 倍) | 同一 Input 内先出第二腿 `P1` 卖 0.020(对冲基数不超过槽的 `qty`),再 `Note{OmsOverfill}`(P0)与链接 `Broken{Overfill}`;该成交的 `FillDelta.flags` 含 `OVERFILL`;t ∈ [1.05 s, 场景结束] 出环帧中无 `P1` 数量 > 0.021 BTC 的单 |
| B6.2 | 价格不变;第一腿出环后、成交前,`probe-taker` 在 `P1` 上发若干限价远离对手价的单腿 IOC Open(零成交),把 `VP` 账户 `a` 的两个窗口桶耗到恰剩 容量 × 0.20 | 下一条 `P1` 单腿 Open `reject = RATE`;随后第一腿成交触发的第二腿(Hedge,`RISK_REDUCING`)`reject = 0` 并出环,两个窗口桶余量各减 1 |

### A5 MarginStress 按到强平距离 30% → 24% → 14% → 9% 依次进入 L0、L1、L2、L3

- 规则:[保证金分档(MarginStress)](../../crates/hft-risk/README.md#保证金分档marginstress)、[保证金数据](../../crates/hft-ledger/README.md#保证金数据)、[保证金危急的默认减仓(A5)与 engine_flatten](../../crates/hft-exec/README.md#保证金危急的默认减仓a5与-engine_flatten)、[PairOnFill](../../crates/hft-exec/README.md#paironfill)
- 级别:dst
- 里程碑:M4
- 门槛:④
- 前置:W1、R6;场景开始前 `probe-target` 以目标 `{S1 +1.0, P1 −1.0}`(PairOnFill,`reason = Rebalance`)建仓完毕并保持该目标;`VP` 每 1 s 推 `MarkPrice` 60,010.0、每 5 s 推一条 `MarginReport`,报告的维持保证金与保证金余额使 ledger `est_liq_dist` 恒为 0.60、`reported_ratio` 恒为 0.10;报告中 `P1` 的 `liq_px = 60,010.0 × (1 + d)`,`d` 即下面的 `rep_liq_dist`
- 故障:`rep_liq_dist` 在 t ∈ [0, 10) s 为 0.30,t = 10 s 起 0.24,t = 20 s 起 0.14,t = 25 s 起 0.09
- 种子:标准集(§2.4)
- 断言:
  - t < 10 s 无 `MarginStress` cap;t = 10 s 的 `MarginReport` Input 内 `CapSet{MarginStress, a, level 1}`(P1),实例 1 收到 `RiskEvent::MarginStress{VP, L1}`;此后账户 `a` 的 Open `reject = CLOSE_ONLY`
  - t = 20 s:cap `level = 2`;`RiskEvent::MarginStress{L2}` 带 `deadline_tsc`,对应 `deadline_ens` = 该 Input 的 `engine_ns` + 30,000 ms;`Note{MarginLevel}`(P1);登记 `Timer{Risk, MarginDeadline}`
  - t = 25 s:cap `level = 3`;`Note{MarginLevel}`(P0);同一 Input 内 `ForceDeleverage{a, VP, frac 0.20}`,g1 引擎目标 `{S1 +0.8, P1 −0.8}`(`reason = MarginStress`、`urgency = Urgent`),`ExecDecision{ForceStep, detail = Engine/Step}`,S1 与 P1 各一条 `cancel_resting`,实例 1 收到 `Stuck{EngineOverride}`;第一笔引擎单为 `P1` 买、`REDUCE_ONLY`、`Flatten` 类
  - t = 30 s 仍为 L3:第二步引擎目标 `{S1 +0.64, P1 −0.64}`
  - 每笔 `P1` 减仓单使其仓位向 0 移动且不越过 0;每步 INV-3、INV-17 成立
- 实现:`crates/hft-testkit/src/scenarios/exec_correctness.rs::a5_margin_stress_levels_by_liq_dist`
- 备注:L3 覆盖 `Broken` cell 的路径由 BK-3 覆盖

| 行 | 相对主例的变化 | 断言 |
|---|---|---|
| A5.1 | `rep_liq_dist` 恒 0.30;`reported_ratio` 自 t = 10 s 起 0.82,t = 40 s 起依次为 0.750000001、0.75、0.749999999(每值保持一个报告周期) | t = 10 s `level = 2`(比率交叉校验);0.750000001 与 0.75 时保持 L2;0.749999999 的报告处理后 `CapCleared{MarginStress}`(逐档降到 L0) |
| A5.2 | `rep_liq_dist` 恒 0.26,报告使 `est_liq_dist` = 0.23 | `liq_dist = 0.23`,`level = 1` |
| A5.3 | t = 0 之后不再有 `MarginReport`(`MarkPrice` 照常) | `engine_ns` ≤ 15,000 ms 的 Input 处理后无 cap;首条 `engine_ns` > 15,000 ms 的 Input(含 `Timer{Risk, MarginStale}`)处理后 `level = 1` |
| A5.4 | `rep_liq_dist` 自 t = 20 s 起保持 0.14 | t = 50 s 的 `Timer{Risk, MarginDeadline}` Input 内 `level = 3` 与第一步 `ForceDeleverage`;之后 `rep_liq_dist` 回到 0.17 时仍为 L3,回到 0.170000001 的报告处理后降到 L1 |
| A5.5 | 进入 L1 后 `rep_liq_dist` 于 t = 15 s 回到 0.27、t = 20 s 回到 0.270000001(之后保持) | 0.27 时保持 L1;0.270000001 的报告处理后 `CapCleared{MarginStress}`、`RiskEvent::MarginStress{level None}` |
| A5.6 | `rep_liq_dist` 自 t = 10 s 起分别为 0.250000001、0.25、0.249999999(三次运行) | 第一次无 cap;后两次 `level = 1` |
| A5.7 | t = 24 s 策略把目标改为 `{S1 0, P1 0}`(`reason = Exit`) | t = 25 s 的引擎目标为 `{S1 0, P1 0}`(各腿取更接近 0 者),不是 `{+0.8, −0.8}`;之后每步目标保持 0 |

### C1 对冲腿查单 NotFound 后保持 Unknown 直到成交历史确认

- 规则:[Unknown 收敛流程](../../crates/hft-oms/README.md#unknown-收敛流程)、[墓碑与迟到成交(C1)](../../crates/hft-oms/README.md#墓碑与迟到成交c1)、[OnFill](../../crates/hft-oms/README.md#onfill)、[Unknown 订单收敛](../../apps/reconciler/README.md#unknown-订单收敛)
- 级别:dst
- 里程碑:M2
- 门槛:④
- 前置:W1、R6,另设 `hedge_deadline_ms = 60,000`;`VP` 的 `stale_guard = RecvWindow`;`probe-taker` 在 t = 1 s 发 S1 买 0.02 带开仓链接
- 故障:第一腿 t = 1.05 s 全部成交;第二腿(P1 卖 0.02 IOC,槽 s2)写出后 t = 1.06 s `VP` 账户 `a` 下单连接 `Down`,t = 2 s `Up`;`MockVenue` 对 s2 的按 client id 查询一律回 `NotFound`;`ReconActor` 对 s2 的 `UnknownResolved` 在 t = 4 s 给 `history_checked = 0`,在 t = 8 s 给 `history_checked = 1`、`exch_qty = 0`
- 种子:标准集(§2.4)
- 断言:
  - t = 1.06 s 起 s2 为 `Unknown`;t = 2 s 的 `ConnState{Order, Up}` Input 内一条 `OrderDecision{action Query, slot s2}`
  - `now − sent_tsc > 2,000 ms` 之后的第一条 `QueryResult{NotFound}` Input 内 s2 置 `HIST_WAIT`、写 `Note{OmsNeedsRestQuery, trades = 1}`,状态仍为 `Unknown`
  - t = 4 s 报告之后 s2 仍为 `Unknown`、`query_count` 不变;t ∈ [1.06 s, 8 s) 无 `link_role = SecondRetry` 的 `OrderDecision`
  - t = 8 s 报告的 Input 内 s2 → `Rejected`(`reject_code = 0`,迁移原因 `Reconcile`);同一 Input 内登记 `HedgeRetry`,到期 Input 内一条 `SecondRetry{qty 0.02}`
  - 每步 INV-2、INV-4、INV-8 成立
- 实现:`crates/hft-sequencer/tests/scenarios_hedge.rs::c1_hedge_leg_notfound_waits_history`
- 备注:C1.3 见 `c1_3_deadline_while_unknown`;C1.1、C1.2 待 W1 种子化 DST;实现方式同 B1(真实 sequencer 在 B0 夹具上的确定性步骤序列,映射与阈值见该文件头)

| 行 | 相对主例的变化 | 断言 |
|---|---|---|
| C1.1 | 另一条链接的第二腿(P1 卖 0.02 IOC,槽 s3,代 g)成交 0.005 后 `Expired`;s3 经 `reuse_quarantine_ms` 后被 sweep 回收;t = 20 s 到达 `OrderEvent{slot s3, gen g, cum_filled 0.010}` | t = 20 s Input 内 `FillOut{flags LATE, qty 0.005}` 入账,P1 仓位再减 0.005;`Note{OmsLateFill}`(P0);s3 当前代的状态不变;该 Input 无 OMS 产生的 `OrderDecision`;该链接不重开 |
| C1.2 | `VP` 的 `stale_guard = ExpireAt` | s2 为链接腿:超过 `not_found_grace_ms` 后仍为 `Unknown`,只在 t = 8 s `history_checked = 1` 时收敛,与主例相同 |
| C1.3 | `hedge_deadline_ms = 3,000` | 第一腿成交 3,000 ms 之后的第一个判定时机(sweep 或 s2 的 `on_unknown_query`)链接 `Broken{Deadline}`、`Note{OmsHedgeBroken}`(P0);此时 s2 仍为 `Unknown` |

### C2 恢复重放与日志输出不一致时受影响的 (venue, account) 全部非空闲槽置 Unknown

- 规则:[恢复时决策不一致](../../crates/hft-sequencer/README.md#恢复时决策不一致)、[冷启动恢复(RecoverDriver)](../../crates/hft-sequencer/README.md#冷启动恢复recoverdriver)、[干净 Drain 重启自动解除 Recovery](../../crates/hft-risk/README.md#干净-drain-重启自动解除-recovery)、[对账闸门与 HARD 解除的严格条件](../../crates/hft-risk/README.md#对账闸门与-hard-解除的严格条件)
- 级别:dst
- 里程碑:M9
- 门槛:④
- 前置:W1、R6;t = 10 s 时账户 `a` 在 `VS` 与 `VP` 上各有一张已 `Filled`、尚在 `reuse_quarantine_ms` 内未回收的槽(`f_s`、`f_p`)
- 故障:t = 10 s `CrashFault`;重启前把 journal 尾部一条 `OrderDecision{account a, venue VS}` 的 `px` 改动 1 tick(只改该 Output 记录)
- 种子:标准集(§2.4)
- 断言:
  - 恢复结束时写 `Note{RecoverMismatch}`(P0,`venue = VS`、`account = a`、`seq` = 被改记录的 seq);`f_s` 为 `Unknown` 且有一条查询;`f_p` 仍为 `Filled`
  - 账户 `a` 的 `RecoveryFlags` 含 `RECOVER_MISMATCH`;`RECONCILED` 只由 `EpochStart` 之后各 venue 的报告置位;闸门放行后 `Recovery{a}` 保留,直到 `Resume{Recovery, a}`
  - 每步 INV-2、INV-6 成立
- 实现:`crates/hft-testkit/src/scenarios/exec_correctness.rs::c2_recover_mismatch_forces_unknown`

| 行 | 相对主例的变化 | 断言 |
|---|---|---|
| C2.1 | 改动的是尾部一条 `CmdResult`(不带账户) | (VS, a)、(VP, a)、(VS, b)、(VP, b) 的未回收 `Filled` 槽都变为 `Unknown`;账户 `a`、`b` 都置 `RECOVER_MISMATCH` |

### C4 自动补腿在门控拒绝期间不计次、不自激,到 fix_deadline_ms 判 Broken

- 规则:[补腿(HedgeFix,C4)](../../crates/hft-exec/README.md#补腿hedgefixc4)、[门控拒绝与退避](../../crates/hft-exec/README.md#门控拒绝与退避)、[HARD 解除时的 Broken 判定](../../crates/hft-exec/README.md#hard-解除时的-broken-判定resolve_broken)、[令牌桶](../../crates/hft-risk/README.md#令牌桶)、[SendReport 处理](../../crates/hft-oms/README.md#sendreport-处理)
- 级别:dst
- 里程碑:M2
- 门槛:④
- 前置:W1、R6,另设 `VS` 账户 `a` 的 `backoff_base_ms = 20,000`;`probe-taker` 在 t = 1 s 发 S1 买 0.02 带开仓链接,`HedgePlan.on_fail = Hold`
- 故障:第一腿 t = 1.05 s 全部成交;第二腿成交 0.01 后 t = 1.10 s `Expired`(链接按 `Hold` 置 `Done`,g1 已成交裸量 +0.01 BTC 在 S1);同一时刻 `MockVenue` 对账户 `a` 在 `VS` 的另一张单回限速拒绝,产生 `RateLimitHit{VS, a}`(退避 20,000 ms)
- 种子:标准集(§2.4)
- 断言:
  - t ≥ 1.10 s 的补腿意图 `OrderDecision{inst S1, side 卖, qty 0.01, link_role HedgeFix, reject = RATE_BACKOFF}`;相邻两条被拒意图间隔 ≥ 500 ms;t ∈ [1.10 s, 11.10 s] 这类意图 ≤ 21 条
  - 补腿首次满足触发的时刻 + 10,000 ms 之后的第一次推进(≤ 11.60 s)内 `ExecDecision{State → Broken, detail = Broken/FixExhausted}`、`Note{ExecBroken}`(P0)、`CapSet{Broken, (a, g1)}`;此前无 `Broken`
  - 全程无 `link_role = HedgeFix` 的出环帧
  - 每步 INV-14、INV-15 成立
- 实现:`crates/hft-sequencer/tests/scenarios_hedge.rs::c4_auto_hedge_fix_gated_bounded`
- 备注:C4.1 见 `c4_1_rejected_fixes_exhaust`;C4.2 待 W1 种子化 DST;实现方式同 B1(真实 sequencer 在 B0 夹具上的确定性步骤序列,映射与阈值见该文件头)

| 行 | 相对主例的变化 | 断言 |
|---|---|---|
| C4.1 | 不注入限速;`MockVenue` 对每张补腿单回交易所拒单 | 补腿单恰好 3 张,相邻两张的 `OrderDecision` 间隔 ≥ 1,000 ms;第 3 张被拒的 Input 内 `Broken{FixExhausted}`、`Note{ExecBroken}`(P0) |
| C4.2 | t = 1.20 s `Kill{Account(a), hard}`;t = 20 s 前两 venue 各有一组全部 `Ignore` 且 `view_seq` 晚于该 Kill 的报告;t = 20 s `Resume{Kill}` | t ∈ [1.20 s, 20 s) 无 `link_role = HedgeFix` 的 `OrderDecision`(含被拒的);t = 20 s Input 内 `resolve_broken(a, g1)` 为真 → `CapSet{Broken, (a, g1)}`,该 Input 无 `HedgeFix` 订单 |

### C6 启动时永续账户为双向持仓则 ACCOUNT_CFG 不放行

- 规则:[启动闸门](../../crates/hft-risk/README.md#启动闸门)、[cap 记录](../../crates/hft-risk/README.md#cap-记录)、[永续杠杆上限](../../crates/hft-risk/README.md#永续杠杆上限)、[启动顺序](../../apps/hft-core/README.md#启动顺序)
- 级别:dst
- 里程碑:M3
- 门槛:④
- 前置:W1、R6,另设 `max_perp_leverage_x100 = 300`、`P1` 实际杠杆 3.00;`VP` 的 `required_position_mode = OneWay`、`required_margin_mode = Cross`;`VS` 的 `required_spot_borrow = Some(false)`;从干净 Drain 快照冷启动
- 故障:t = 1 s (a, VP) 的 `AccountConfigReport{position_mode Hedge, margin_mode Cross, P1 杠杆 3.00}`;t = 5 s 同一账户报告改为 `position_mode OneWay`
- 种子:标准集(§2.4)
- 断言:
  - t ∈ [1 s, 5 s):账户 `a` 的 `gate_bits` 无 `ACCOUNT_CFG`(`Note{StartupGate}`),`released = false`;账户 `a` 无 `AccountCfg` cap
  - t = 5 s Input 内 `VP` 进入 `cfg_ok_venues`;其余位已满足,账户 `a` 放行
- 实现:`crates/hft-testkit/src/scenarios/exec_correctness.rs::c6_account_cfg_gate_and_cap`
- 备注:测试网部分见 drills.md

| 行 | 相对主例的变化 | 断言 |
|---|---|---|
| C6.1 | 放行后 t = 10 s 报告 `margin_mode = Isolated`,t = 15 s 恢复 `Cross` | t = 10 s `CapSet{AccountCfg{a, VP}, params[0] = 1}`(MISMATCH,P1);t ∈ [10 s, 15 s) 账户 `a` 涉及 `VP` 的 Open(含以 P1 为链接腿的 S1 开仓)`reject = CLOSE_ONLY`,对冲与减仓 `reject = 0`;t = 15 s `CapCleared{AccountCfg}` |
| C6.2 | 放行后 (a, VS) 的报告带 `AUTO_BORROW` | `CapSet{AccountCfg{a, VS}, params[0] = 1}`;`VS` 上的 Open `reject = CLOSE_ONLY` |
| C6.3 | 放行后 `P1` 杠杆依次报告 2.99、3.00、3.01 | 前两条无 cap;3.01 的 Input 内 `CapSet{AccountCfg{a, VP}, params[0] = 2}`(LEVERAGE,P1) |
| C6.4 | 放行后一条非 `PARTIAL` 报告未列出 `P1` 的杠杆 | 同 C6.3 的 3.01 一行 |
| C6.5 | 放行前 `P1` 杠杆报告为 3.01,模式一致 | `ACCOUNT_CFG` 不置位;`hedge_gate_ok(a, VP) = true`(杠杆不属于对冲闸门子集) |

### C7 登记余额基线之后平台币余额不再报差

- 规则:[余额初始化、校验与基线](../../crates/hft-ledger/README.md#余额初始化校验与基线)、[仓位与已实现](../../crates/hft-ledger/README.md#仓位与已实现平均成本法)、[人工干预的采纳](../../crates/hft-ledger/README.md#人工干预的采纳adoptexchange)、[现货库存与余额基线](../../apps/reconciler/README.md#现货库存与余额基线)
- 级别:dst
- 里程碑:M5
- 门槛:④
- 前置:W1、R6;`VS` 的 `fee_asset_rule = ReceivedAssetUnlessDiscount{token = BNB}`;账户 `a` 在 `VS` 的初始余额另有 3.5 BNB(不属于任何 group);`ReconActor` 自 t = 30 s 起每 30 s 一组报告
- 故障:t = 5 s 交易所真值中 BNB 增加 0.2(空投,用户流不推);t = 40 s `SetBalanceBaseline{a, VS}`(Trader,已确认);t = 50 s 起 g1 的 S1 成交以 BNB 扣费,每笔 0.0001 BNB
- 种子:标准集(§2.4)
- 断言:
  - t = 30 s 报告含 `Balance{asset BNB, engine_qty 3.5, exch_qty 3.7, severity Warn}`
  - t = 40 s Input 内 BNB 条目 `baseline = 3.7`、`unassigned = 0`、`total = 3.7`、置 `NON_STRATEGY`;`Note{BaselineSet}`;`Gate(Disposed{Baselined})`
  - t = 60 s 起各组报告无 BNB 条目;每笔 BNB 扣费从 `baseline` 扣除,`inv(BTC, g1)` 不因扣费变化
- 实现:`crates/hft-testkit/src/scenarios/exec_correctness.rs::c7_balance_baseline_silences_platform_coin`

| 行 | 相对主例的变化 | 断言 |
|---|---|---|
| C7.1 | 不空投;g1 持 S1 +0.02;t = 5 s 交易所真值中 BTC 少 0.005(网页手工卖出,用户流不推);t = 40 s 改发 `AdoptExchange{a, VS, item Balance, asset BTC, group g1, report_id = t = 30 s 那组}` | t = 30 s 报告含 `Balance{asset BTC, engine_qty 0.02, exch_qty 0.015, Warn}`;t = 40 s `inv(BTC, g1) = 0.015`、LegTracker S1 net +0.015、`Note{Adjusted}`;t = 60 s 报告无 BTC 条目 |

### C8 按账户 HARD 只拦该账户的全部下单与改单

- 规则:[Kill](../../crates/hft-risk/README.md#kill)、[kill 字](../../crates/hft-risk/README.md#kill-字唯一的跨线程共享可变量)、[检查链](../../crates/hft-risk/README.md#检查链按廉价--昂贵短路适用范围以本表为唯一定义)、[RISK_REDUCING 判定](../../crates/hft-oms/README.md#risk_reducing-判定)、[崩溃隔离与从快照恢复](../../apps/hft-core/README.md#崩溃隔离与从快照恢复)
- 级别:dst
- 里程碑:M9
- 门槛:④
- 前置:W1、R6;账户 `a`、`b` 各有一条第一腿已成交 0.02、第二腿在 `HedgeRetry` 中的链接(第二腿首单 IOC 零成交),以及一张已 Ack 的策略 GTC 开仓挂单
- 故障:t = 1 s `Kill{Account(a), hard}`(Admin,已确认);此后 `MockVenue` 对重试单全部成交
- 种子:标准集(§2.4)
- 断言:
  - t = 1 s Input 内 `CapSet{Kill, a, cflags HARD}`、kill 字重写;账户 `a` 的全部挂单(含 `RISK_REDUCING` 的)以 `open_only = false` 撤单,撤单帧出环
  - t ≥ 1 s 账户 `a` 的 `SecondRetry` 意图 `reject = HARD`,出环帧中无账户 `a` 的 `Place`/`Amend`;账户 `a` 的查询照常出环
  - 账户 `b` 的 `SecondRetry` `reject = 0` 并出环成交;账户 `b` 的新 Open `reject = 0`
  - 每步 INV-5、INV-12 成立
- 实现:`crates/hft-testkit/src/scenarios/exec_correctness.rs::c8_account_hard_blocks_only_that_account`
- 备注:毒丸由 RC-6 覆盖;从快照恢复由 RC-7 覆盖

### C9 交易 key 的交易所 UID 与 venue_uid 不符时 hft-core Drain 后以退出码 2 退出

- 规则:[启动顺序](../../apps/hft-core/README.md#启动顺序)、[启动闸门](../../crates/hft-risk/README.md#启动闸门)、[身份核验(UID)](../../apps/kill-switch/README.md#身份核验uid)、[启动核验(只读 key)](../../apps/reconciler/README.md#启动核验只读-key)、[数据结构](../../crates/hft-refdata/README.md#数据结构)
- 级别:proc
- 里程碑:M5
- 门槛:④、⑪
- 前置:Linux CI;hft-core `prod` 实例连接进程外的假交易所,refdata 为 W1 的账户 `a`、`b`;`AccountMeta` 中 (a, VP) 的 `venue_uid = "10000001"`、`uid_policy = Verified`;假交易所对 (a, VP) 的 REST key 返回 UID `"10000002"`,对会话 key 返回 `"10000001"`;其余 key 的 UID 与 refdata 一致
- 步骤:
  1. 启动 hft-core,等待步 6 完成 (a, VP) 的账户查询
  2. 等待进程退出,上限 60 s
- 期望:
  - journal 中 `AccountConfigReport{a, VP}` 的 `mismatches` 含 `UID`、`uid_keys` 含 REST;其后依次有 `CapSet{Kill, a, cflags HARD}`、`Note{IdentityMismatch}`(P0)、`Draining` cap、`purpose = Drain` 的快照
  - 账户 `a` 的 `IDENTITY` 位全程未置位;进程退出码 2
- 实现:`apps/hft-core/tests/identity.rs::c9_uid_mismatch_drains_and_exits_2`

| 行 | 相对主例的变化 | 期望 |
|---|---|---|
| C9.1 | 会话 key 的 UID 不符,REST key 相符 | 同主例 |
| C9.2 | kill-switch 的撤单 key 对 (a, VP) 查到的 UID 不符 | kill-switch `run` 在进入 `Watching` 之前发 P0,退出码 2 |
| C9.3 | reconciler 的只读 key 对 (a, VP) 查到的 UID 不符 | 写 `<state>/fatal.json`(`code = "UidMismatch"`,UID 只记末 4 位)后退出码 2;退出前未发出任何对账请求 |
| C9.4 | 测试构建让服务账户 `a` 的 gw 的 kill 字回读丢掉最高位(kill 自检失败) | journal 中无账户 `a` 的 `Timer{Sys, KillSelfTestOk}`;账户 `a` 的 `IDENTITY` 不置位、不放行;进程不退出 |
| C9.5 | (a, VP) 的 UID 查询超时(报告带 `UNREADABLE`) | 不 Drain、不退出;`IDENTITY` 不置位;告警级别 P1 |
| C9.6 | 账户 `a` 为 `is_prod = false`、`uid_policy = Unverifiable`,UID 查询返回 `UNREADABLE` | kill 自检通过且收到 (a, VP) 的一条 `AccountConfigReport` 后 `IDENTITY` 的 VP 位置位,同一 `apply` 写 `Note{IdentityUnverified}`(P1,`v[0]` 低 8 位 = VP 的 `VenueId`、其上 8 位 = `a` 的 `AccountId`);不 Drain |

### C10 persist 滞后到禁开档时设全局 Storage cap 且只拦 Open

- 规则:[存储降级(Storage)](../../crates/hft-risk/README.md#存储降级storage)、[存储降级](../../crates/hft-sequencer/README.md#存储降级)、[Drain](../../crates/hft-sequencer/README.md#drain)
- 级别:dst
- 里程碑:M5
- 门槛:④
- 前置:W1、R6;两个账户各持 g1/g2 的 S1 +0.02、P1 −0.02
- 故障:t = 1 s `HkActor` 注入 `Timer{Sys, PersistLag}`(档位 1);t = 10 s 注入 `Timer{Sys, StorageOk}`
- 种子:标准集(§2.4)
- 断言:
  - t = 1 s Input 内 `CapSet{Storage, params[0] = 1, params[1] = 0}`;t ∈ [1 s, 10 s) 两账户的 Open `reject = CLOSE_ONLY`,第二腿与减仓 `reject = 0`;无 `Draining` cap
  - t = 10 s Input 内 `CapCleared{Storage}`;其后 Open `reject = 0`
  - 每步 INV-12、INV-13 成立
- 实现:`crates/hft-testkit/src/scenarios/exec_correctness.rs::c10_persist_lag_storage_cap`
- 备注:`storage.*` 门限的默认值与边界用例归 hft-core 的 `TESTS.md`;本用例从 `Timer` 起

| 行 | 相对主例的变化 | 断言 |
|---|---|---|
| C10.1 | t = 5 s 注入 `Timer{Sys, PersistLag}`(档位 2) | t = 5 s Input 内 `CapSet{Draining, params[0] = 1}` 与 venue 级 `CancelAll`;Drain 完成后 `purpose = Drain` 快照,`Done{cause StorageCritical, restart false}` |
| C10.2 | 改为 `Timer{Sys, DiskLow}`(档位 1);另一次运行在 t = 5 s 注入 `Timer{Sys, DiskLow}`(档位 2) | 分别同主例(cap `params[1] = 1`)与 C10.1 |
| C10.3 | t = 4 s 账户 `a` 进入 L3(g1 有未到达的 `MarginStress` 引擎目标);t = 5 s 注入 `Timer{Sys, PersistLag}`(档位 2) | t = 5 s 不发起 Drain,`drain_deferred() = true`;L3 引擎单照常出环;引擎目标全部到达的那条 Input 内恰好发起一次 Drain(`Done{cause StorageCritical}`);t ≥ 5 s 的 Open 一直 `reject = CLOSE_ONLY` |
| C10.4 | t = 3 s `Kill{Account(a), flatten}`,清仓在 t = 4.5 s 全部到达(清仓锁仍在);t = 5 s 注入 `Timer{Sys, PersistLag}`(档位 2) | t = 5 s Input 内即发起 Drain |

### C11 Drain 等在途链接与已 Ack 的 IOC 终态后才写快照

- 规则:[Drain](../../crates/hft-sequencer/README.md#drain)、[Drain 静止判定(支持 C11)](../../crates/hft-oms/README.md#drain-静止判定支持-c11)、[运行模式、Drain 与 Unknown](../../crates/hft-exec/README.md#运行模式drain-与-unknown)
- 级别:dst
- 里程碑:M5
- 门槛:④
- 前置:W1、R6;`probe-taker` 在 t = 1 s 发 S1 买 0.02 带开仓链接
- 故障:第一腿 t = 1.02 s `Ack`;t = 1.03 s `CtlActor` 发 `Drain{cancel_resting = false}`;第一腿 t = 1.20 s 全部成交;第二腿 t = 1.30 s 全部成交
- 种子:标准集(§2.4)
- 断言:
  - t = 1.03 s Input 内 `CapSet{Draining, params[0] = 0}`;此后策略 Open `reject = DRAINING`
  - 第二腿在第一腿成交的同一 Input 内出环,`reject = 0`
  - `purpose = Drain` 快照的 seq 大于第二腿成交 Input 的 seq;快照时账户 `a` 的 `quiescent(a)` 为空、`exec.drain_ready() = true`;无 `Note{DrainTimeout}`
  - 每步 INV-18 成立
- 实现:`crates/hft-testkit/src/scenarios/exec_correctness.rs::c11_drain_waits_link_and_ioc_terminal`

| 行 | 相对主例的变化 | 断言 |
|---|---|---|
| C11.1 | 第二腿首单 `Expired` 零成交,`SecondRetry` 在 t = 1.45 s 成交 | 快照 seq > `SecondRetry` 成交 Input 的 seq;`HedgeRetry` 到期之前不写快照 |
| C11.2 | 另设 `hedge_deadline_ms = 60,000`、`shutdown.drain_timeout_ms = 5,000`;t = 1.10 s 起 `P1` 的 BBO 为 59,000.0 / 61,000.0(连接健康,买一低于 mid × (1 − 0.004)),第二腿一直按 `BeyondCross` 门控 | t = 1.03 s + 5,000 ms 时 `Note{DrainTimeout}`(P0,`v[0..4]` 的账户位图 bit a = 1,`v[4] = 1`);仍写 `purpose = Drain` 快照;下次启动账户 `a` 不满足干净重启,`Recovery{a}` 不自动解除 |

### C12 用户流里的可忽略与未知事件只计数,不置 VERIFY、不查询、不断流

- 规则:[实现方必须遵守的契约](../../crates/hft-venue-proto/README.md#实现方必须遵守的契约)(契约 20)、[GwRuntime::poll_once 的循环结构](../../crates/hft-venue/README.md#4-gwruntimepoll_once-的循环结构)、[REPORT_DECODE](../../crates/hft-oms/README.md#report_decode)、[Unknown 收敛流程](../../crates/hft-oms/README.md#unknown-收敛流程)
- 级别:integ
- 里程碑:M3
- 门槛:④
- 前置:进程内 `GwRuntime<P>`(`P` = venue-okx 的 `okx-swap` 协议,venue `VP`)+ 真实 sequencer,回报帧由测试注入;`VenueCaps.ignorable_user_events = ["evt_ignorable"]`;`gw.unknown_evt_note_every = 1,000`;`decode_fail_down_n = 3`;账户 `a` 在 `VP` 有 3 个 `Live` 的 GTC 槽
- 步骤:
  1. 注入 10,000 条事件名为 `"evt_ignorable"` 的用户流帧
  2. 注入 10,000 条事件名为 `"evt_new_type"`(不在名单、解码器不认得)的用户流帧
  3. 注入 1 条订单推送类、内容截断的用户流帧
- 期望:
  - 步骤 1、2 之后:`user_evt_ignored{VP} = 10,000`、`user_evt_unknown{VP} = 10,000`;Meta `Note` 条数 = 1 + ⌊10,000 / 1,000⌋ = 11;sequencer 未收到 `ConnState`;3 个槽无 `VERIFY`;无 `OrderDecision{action Query}`;用户流连接未重连
  - 步骤 3 之后:`ConnState{UserStream, Degraded, reason REPORT_DECODE}`;3 个槽置 `VERIFY` 并各有一条 `Query`
- 实现:`apps/hft-core/tests/user_events.rs::c12_ignorable_and_unknown_user_events_only_counted`
- 备注:各 venue 的事件分类金样归各 `venue-*` 的 `TESTS.md`

### C13 全仓资金费只推余额的 venue 由 sequencer 按结算时刻持仓合成每 inst 的资金费

- 规则:[资金费合成](../../crates/hft-sequencer/README.md#资金费合成)、[资金费](../../crates/hft-ledger/README.md#资金费)、[资金费对账校正](../../crates/hft-ledger/README.md#资金费对账校正)、[资金费比对](../../apps/reconciler/README.md#资金费比对)
- 级别:dst
- 里程碑:M9
- 门槛:④
- 前置:W1、R6;`VP` 的 `funding_settle = BalanceOnlyPush`;g1 持 P1 −0.5;`P1` 的 `funding_interval_s = 28,800`,下一结算时刻 `T` = 场景开始后的第一个 UTC 08:00;`T` 时 `MarketLast.mark_px = 60,010.0`、`funding_rate = 0.0001`
- 故障:`T + 1.5 s` `MockVenue` 推 `FundingSettle{account a, inst INST_NONE, settle_ts T + 1.2 s, amount +3.000500}`;`T + 3 s` 同一推送再到一次
- 种子:标准集(§2.4)
- 断言:
  - 结算触发(`Timer{Sys, FundingSettle{VP}}`,或首条把 `next_funding_ns` 推过 `T` 的 `Funding`)的 Input 内一条 Output `FundingSettle{synth 1, account a, inst P1, settle_ts T, amount +3.000500}`;g1 的 `funding` 增加 3.0005;`VP` 保证金钱包不变
  - `T + 1.5 s` Input 内钱包增加 3.0005;(a, VP, T) 的已入钱包额 = 3.0005;残差 0,无 `Note{FundingMismatch}`
  - `T + 3 s` 的重复推送:钱包与已入钱包额不变
- 实现:`crates/hft-testkit/src/scenarios/exec_correctness.rs::c13_balance_only_push_funding_synth`

| 行 | 相对主例的变化 | 断言 |
|---|---|---|
| C13.1 | 推送金额为 +3.10 | 残差 0.0995 > max(0.05, 0.005 × 3.10) → `Note{FundingMismatch}`(P1);钱包按 3.10 入账 |
| C13.2 | 没有推送;`T + 600,001 ms` 时 `ReconActor` 报 `Funding{inst INST_NONE, exch_ts T, exch_amount 3.0005, engine_amount 0}` | `AdjustDelta{kind Funding}`,钱包与 `funding_residual` 各增加 3.0005;`Note{FundingCorrected}`(P1,`flags` 含 `NEW_RECORD`);下一组报告无该笔条目;`RECONCILED` 与 `Recon` cap 不变 |
| C13.3 | 结算时刻 `P1` 仓位为 0 | 无合成 `FundingSettle` |

### C14 MarginStress 与 Hard 退出强制永续腿先,常规开仓按 ByLiquidity

- 规则:[PairOnFill](../../crates/hft-exec/README.md#paironfill)、[保证金危急的默认减仓(A5)与 engine_flatten](../../crates/hft-exec/README.md#保证金危急的默认减仓a5与-engine_flatten)、[目标的接受与替换](../../crates/hft-exec/README.md#目标的接受与替换)
- 级别:table
- 里程碑:M4
- 门槛:④
- 前置:W1、R6;g1 的腿号:S1 = 0、P1 = 1;减仓行(C14.1–C14.3)起始 S1 +0.10、P1 −0.10,增仓行(C14.4–C14.7)起始 0;`probe-target` 按各行下 PairOnFill 目标;"深度"指对手侧前 5 档可见深度名义;期望看该目标第一条 `ExecDecision{Slice}` 的 `first_leg_ix` 与第一笔订单的 inst
- 实现:`crates/hft-testkit/src/scenarios/exec_correctness.rs::c14_leg_order`

| 行 | 目标与条件 | 期望 |
|---|---|---|
| C14.1 | 目标 `{0, 0}`,`reason = MarginStress`,`first_leg = SpotFirst`,S1 深度 50,000、P1 200,000 | `first_leg_ix = 1`,第一笔为 P1 买 |
| C14.2 | 目标 `{0, 0}`,`reason = Hard`,`first_leg = ByLiquidity`,深度同 C14.1 | `first_leg_ix = 1` |
| C14.3 | 账户 `a` 处于 L3 默认减仓期,策略目标 `{+0.05, −0.05}`(每腿不超过引擎目标),`reason = Rebalance`,`first_leg = SpotFirst` | 目标被接受,`first_leg_ix = 1` |
| C14.4 | 目标 `{+0.2, −0.2}`,`reason = Rebalance`,`first_leg = ByLiquidity`,S1 深度 50,000、P1 200,000 | `first_leg_ix = 0`(深度小者先) |
| C14.5 | 同 C14.4,两腿深度都是 100,000 | `first_leg_ix = 0`(`leg_ix` 小者) |
| C14.6 | 同 C14.4,`first_leg = PerpFirst` | `first_leg_ix = 1` |
| C14.7 | 同 C14.4,`first_leg = Fixed`、`first_leg_ix = 2` | `ExecDecision{TargetRejected, detail = TargetReject/SpecInvalid}`,无切片 |

### C15 VenueNotice{Delisting} 设该 inst 的 Delisting cap 并把公告投递给策略

- 规则:[下架与公告(VenueNotice)](../../crates/hft-risk/README.md#下架与公告venuenotice)、[检查链](../../crates/hft-risk/README.md#检查链按廉价--昂贵短路适用范围以本表为唯一定义)、[命令路径](../../crates/hft-sequencer/README.md#命令路径)、[FundingStats](../../crates/hft-strat-lib/README.md#fundingstats)
- 级别:dst
- 里程碑:M3
- 门槛:④
- 前置:W1、R6;g1 持 S1 +0.02、P1 −0.02
- 故障:t = 1 s `CtlActor` 发 `VenueNotice{venue VP, inst P1, kind Delisting{at_ns = t + 7 d}}`(Trader);t = 20 s 发 `Resume{Delisting, (VP, P1)}`
- 种子:标准集(§2.4)
- 断言:
  - t = 1 s Input 内 `CapSet{Delisting, (VP, P1), params[0] = at_ns}`(Info);实例 1 收到 `EventRef::Notice`
  - t ∈ (1 s, 20 s):本腿或链接腿为 `P1` 的 Open `reject = CLOSE_ONLY`;S1 单腿 Open(无链接)`reject = 0`;`P1` 的减仓与对冲单 `reject = 0`
  - t = 20 s 之后 `P1` 的 Open `reject = 0`
- 实现:`crates/hft-testkit/src/scenarios/exec_correctness.rs::c15_delisting_notice_blocks_open_on_inst`

| 行 | 相对主例的变化 | 断言 |
|---|---|---|
| C15.1 | 不发公告;t = 1 s `P1` 的 `Funding` 事件 `interval_s = 3,600`(此前 28,800) | 同一 Input 内 `MarketLast.prev_interval_s = 28,800`、`interval_s = 3,600`;实例 1 收到的 `FundingView{interval_s 3,600, prev_interval_s 28,800}`;把该视图交给 `FundingStats::observe` 返回 `IntervalChanged{old_s 28,800, new_s 3,600}`,其后 `summary.flags` 含 `INTERVAL_CHANGED` |

## 7. 断腿、第二腿、保证金、资金费与建议新鲜度

本节用例是上线门槛 ④ 的组成部分;QF-1、MS-2、MS-3 是 ④ 点名的必过项,MS-2a、MS-4 是 ⑦ 的必过项([testing §8](../testing.md#8-上线门槛))。

### BK-1 现货腿多出的 Broken cell 在 broken_auto_fix_ms 到期时恰好开始一次自动有界平仓并卖出多出的现货

- 规则:[Broken 处置](../../crates/hft-exec/README.md#broken-处置)、[限价与单笔名义](../../crates/hft-exec/README.md#限价与单笔名义)、[人工保持与恢复期抑制](../../crates/hft-exec/README.md#人工保持与恢复期抑制)、[断腿](../../crates/hft-risk/README.md#断腿broken自动有界平仓l3-与-flatten)、[OnFill](../../crates/hft-oms/README.md#onfill)
- 级别:dst
- 里程碑:M2
- 门槛:④
- 前置:W1,账户 `a`、group `g1`(leg_ix 0 = `S1`,1 = `P1`),`probe-taker` 参数 `close_on = []`;`S1` BBO 固定 59,999.99 / 60,000.00、`P1` BBO 固定 60,010.0 / 60,010.1(本用例关闭随机游走);`hedge_deadline_ms = 3000`、`hedge_retry_max = 3`、`hedge_retry_min_ms = 100`;`broken_auto_fix_ms = 120000`、`fix_deadline_ms = 10000`、`dust_notional = 5 USDT`、`max_nofill = 5`、`gated_backoff_ms = 500`、`stuck_after_ms = 2000`、`force_slice = {depth_frac 0.3, depth_levels 5, max_cross 0.005, max_chunk_notional 10,000 USDT, min_chunk_notional 0, cooldown_ms 200}`;`RiskParams`:`conn_liveness_ms = 2000`,两 inst 同一价格带类别 `rr_max_cross = 0.01`、`ioc_max_cross = 0.005`,`max_order_notional = 10,000 USDT`、`max_hedge_notional = 10,000 USDT`、`max_reduce_notional = 2,000 USDT`、`funds_buffer = 0.05`
- 故障:
  - t = 0:`probe-taker` 发第一腿 `S1` 买 0.100 IOC,`LegLink{role = First, hedge = OnFill, on_fail = Retry, ratio 1/1, residual = Hold}`;t = 0.05 s 全部成交于 60,000.00
  - `VP` 对 `P1` 的每一张卖单回已登记的拒单码(`Reject`)
  - 链接 `Broken` 的时刻记为 `t_b`;全程无人工 `HedgeFix`
- 种子:标准集(§2.4)
- 断言:
  - 第二腿 `Second` 与 3 张 `SecondRetry` 都被拒,第 4 次被拒的同一 Input 内:`Note{OmsHedgeBroken}`(P0)、`CapSet{Broken(a, g1)}`、`CapSet{HedgeFailure(a)}`、`ExecDecision{State, state_to = Broken, detail = Broken/LinkBroken, arg = 1(RetryExhausted)}`;`t_b ≤ 3.05 s`
  - `t_b` 到 `t_b + 119,999 ms`:journal 中无 `ExecDecision{AutoFix}`,无 `reason = AutoFix(10)` 的 `OrderDecision`
  - `t_b + 120,000 ms` 的推进写 `ExecDecision{AutoFix, detail = AutoFix/Start, target_qty = 0.100}`;此后每片是 `S1` 卖出 IOC,`OrderDecision{reason = AutoFix, link_role = HedgeFix}`,`oflags` 含 `RISK_REDUCING`、不含 `REDUCE_ONLY`,`hedge_cap_qty` = 发出时 `naked_filled` + 0.00001;限价 = `cap_cross_px(Sell, 59,999.99, 59,999.995, 0.005, rr 0.01, tick 0.01)` = 59,700.00;每片 `⌈qty × 59,700.00⌉ ≤ 2,000 USDT`,三片依次 0.03350、0.03350、0.03300 BTC,相邻两片 `OrderDecision` 的引擎时间间隔 ≥ 200 ms
  - 第 3 片成交的同一 Input 内依次有 `ExecDecision{AutoFix, detail = AutoFix/Done}`、`Note{ExecAutoFixDone}`(P1,`v = [a | g1 << 8, 0, 0.100, 0.100, 0, 3]`)、`CapCleared{Broken(a, g1)}`;cell `state = Idle`;`HedgeFailure(a)` 保留到场景结束(无 `CapCleared{HedgeFailure}`)
  - 本 Broken 期 `AutoFix/Start` 恰好 1 条、终态(`AutoFix/Done` 或 `AutoFix/Failed`)恰好 1 条;每步 INV-15、INV-19、INV-20 成立
- 实现:`crates/hft-sequencer/tests/scenarios_risk.rs::bk_1_spot_excess_auto_fix_once`
- 备注:BK-1.2、1.3、1.5–1.8 在同一函数(BK-1.3 的静默从 t_b + 117 s 开始:静默超过 `conn_liveness_ms` 才算报价不可用);BK-1.1、1.4、1.9、1.10 待补;实现方式:以真实 sequencer 的确定性步骤序列在 hft-sequencer B0 夹具上实现,映射 A≙`VS`、B≙`VP`、a1≙`S1`、b1≙`P1`、账户 1/2≙`a`/`b`,价格与阈值取 B0(见该文件头);W1 种子化 DST 版本待测试策略能发出 S1 买入第一腿后补

| 行 | 相对主例的变化 | 断言 |
|---|---|---|
| BK-1.1 | `S1` 改为每 100 ms 一条随机游走 Bbo;`t_b + 119 s` `VS` 对账户 `a` 回限速拒绝(`backoff_base_ms = 6000`),退避到 `t_b + 125 s` | `t_b + 120 s` 到退避结束前无 `AutoFix/Start`、无 `ExecAutoFixFailed`;`AutoFix/Start` 的时刻 ∈ [`t_b + 125 s`, `t_b + 125.1 s`] |
| BK-1.2 | `t_b + 119 s` 到 `t_b + 130 s` `S1` BBO 为 59,000.00 / 61,000.00(bid < mid × 0.99 = 59,400.00) | 期间无 `AutoFix/Start`、无失败注记;BBO 恢复为 59,999.99 / 60,000.00 的那条 `Bbo` 的同一 Input 内写 `AutoFix/Start` |
| BK-1.3 | `t_b + 119 s` 到 `t_b + 130 s` `S1` 的 BBO 连接停止回任何帧(`now − conn_live_tsc` > 2,000 ms) | 期间无 `AutoFix/Start`、无失败注记;连接恢复的那条 `ConnState{Up}` 或 `Bbo` 的同一 Input 内写 `AutoFix/Start` |
| BK-1.4 | 开始后第 1 片被风控以 `RATE` 拒;第 2–6 片 `S1` IOC 零成交;第 7 片起成交 | `RATE` 之后 ≥ 500 ms 才发下一片,写 `ExecDecision{AutoFix, detail = AutoFix/Slice, gate = RATE}`;第 5 次零成交后等 2,000 ms 再发;全程无 `ExecAutoFixFailed`、无 P0 注记;最终 `Note{ExecAutoFixDone}` 的 `v[5]` = 实际片数 |
| BK-1.5 | 开始后 `S1` IOC 一律零成交 | 第一张在 `AutoFix/Start + 10,000 ms`(可执行时长)之后终态的片之后:`Note{ExecAutoFixFailed}`(P0,`v[2] = 7`(Deadline)、`v[4] = 0`、`v[5] = 0.100`、`v[6] = 0.100`)与 `ExecDecision{AutoFix, detail = AutoFix/Failed, arg = 7}`;cell 保持 `Broken`;此后到场景结束无 `reason = AutoFix` 的新单 |
| BK-1.6 | 开始后第 1 片被 `VS` 以已登记拒单码拒绝 | 同一 Input 内 `Note{ExecAutoFixFailed}`(`v[2] = 4`(Rejected))与 `AutoFix/Failed, arg = 4`;cell 保持 `Broken`;此后无第二条 `AutoFix/Start` |
| BK-1.7 | `t_b + 60 s` Trader 发 `HedgeFix{a, g1}` 并确认 | `ExecDecision{CmdHedgeFix}` 与 `S1` 卖 0.100 IOC(`reason = CmdHedgeFix(9)`);到场景结束无 `AutoFix/Start` |
| BK-1.8 | `t_b + 60 s` `Kill{Account(a), hard}`;`ReconActor` 每 10 s 一组干净报告;`t_b + 90 s` `Resume{Kill}` 并确认 | `t_b + 60 s` 到 `t_b + 90 s` 无 AutoFix 订单;`t_b + 209,999 ms` 前无 `AutoFix/Start`,`t_b + 210 s` 开始(解除时重给完整窗口) |
| BK-1.9 | `t_b + 30 s` 交易员在网页卖出 `S1` 0.040(`ExternalFill{cause = Manual}`);`t_b + 50 s` `probe-taker` 对 `g1` 下 `S1` 卖 0.010 的 Reduce 单 | `t_b + 30 s`:`Note{ExecManualHold}`(P1,`v[1] = 1`)、ledger `Note{ExternalFill}` P1;`t_b + 50 s`:`Note{ExecManualHold}`(`v[1] = 0`,原因 1 = Strategy);`t_b + 169,999 ms` 前无 `AutoFix/Start`,`t_b + 170 s` 开始且 `target_qty = 0.050` |
| BK-1.10 | `t_b + 60 s` 崩溃,停机 600 s(墙钟)后重启;重启后引擎时间 `t_r + 20 s` 时 `venue_gate_ok(a, VS)` 首次为真 | 重启前已过的 60 s 与停机都不计:`t_r + 139,999 ms` 前无 `AutoFix/Start`,`t_r + 140 s` 开始 |

### BK-2 永续腿多出的 Broken cell 的自动有界平仓是永续 reduce-only 买入

- 规则:[Broken 处置](../../crates/hft-exec/README.md#broken-处置)、[限价与单笔名义](../../crates/hft-exec/README.md#限价与单笔名义)、[分类](../../crates/hft-risk/README.md#分类)
- 级别:dst
- 里程碑:M2
- 门槛:④
- 前置:同 BK-1 的参数与固定 BBO;`probe-taker` 以 `P1` 卖为第一腿、`S1` 买为第二腿(`g1` 的 leg_ix 不变:0 = `S1`,1 = `P1`)
- 故障:t = 0 第一腿 `P1` 卖 0.100 IOC,t = 0.05 s 全部成交于 60,010.0;`VS` 对 `S1` 的每一张买单回已登记的拒单码;链接 `Broken` 的时刻记为 `t_b`;无人工 `HedgeFix`
- 种子:标准集(§2.4)
- 断言:
  - `t_b + 120,000 ms` 写 `AutoFix/Start`(`target_qty = 0.100`),之前无 AutoFix 订单
  - 每片是 `P1` 买入 IOC,`oflags` 含 `RISK_REDUCING` 与 `REDUCE_ONLY`,`link_role = HedgeFix`;限价 = `cap_cross_px(Buy, 60,010.1, 60,010.05, 0.005, rr 0.01, tick 0.1)` = 60,310.1;四片依次 0.033、0.033、0.033、0.001 BTC,每片 `⌈qty × 60,310.1⌉ ≤ 2,000 USDT`
  - 末片成交的同一 Input 内 `Note{ExecAutoFixDone}`(P1,`v = [a | g1 << 8, 1, 0.100, 0.100, 0, 4]`)与 `CapCleared{Broken(a, g1)}`;`HedgeFailure(a)` 保留
  - 全程无 `S1` 的 AutoFix 订单;每步 INV-15、INV-17、INV-20 成立
- 实现:`crates/hft-sequencer/tests/scenarios_risk.rs::bk_2_perp_excess_reduce_only_buy`
- 备注:实现方式:以真实 sequencer 的确定性步骤序列在 hft-sequencer B0 夹具上实现,映射 A≙`VS`、B≙`VP`、a1≙`S1`、b1≙`P1`、账户 1/2≙`a`/`b`,价格与阈值取 B0(见该文件头);W1 种子化 DST 版本待测试策略能发出 S1 买入第一腿后补

### BK-3 L3 与 flatten 对 Broken cell 照常生效,先 reduce-only 减永续腿再减现货

- 规则:[保证金危急的默认减仓](../../crates/hft-exec/README.md#保证金危急的默认减仓a5与-engine_flatten)、[Broken 处置](../../crates/hft-exec/README.md#broken-处置)、[断腿](../../crates/hft-risk/README.md#断腿broken自动有界平仓l3-与-flatten)、[每条 Input 的处理顺序](../../crates/hft-sequencer/README.md#每条-input-的处理顺序process)
- 级别:dst
- 里程碑:M2
- 门槛:④
- 前置:W1,账户 `a`、`g1`;`P1` mark 60,000.0;`RiskParams`:`liq_dist_block_open = 0.25`、`liq_dist_reduce = 0.15`、`liq_dist_force = 0.10`、`liq_dist_hyst = 0.02`、`force_step = 0.20`、`force_step_interval_ms = 5000`、`max_reduce_notional = 30,000 USDT`、`rr_max_cross = 0.01`;`force_slice.max_chunk_notional = 30,000 USDT`;`broken_auto_fix_ms = 120000`、`dust_notional = 5 USDT`
- 故障:
  - t = 0:第一腿 `P1` 卖 0.400 成交;第二腿 `S1` 买只成交 0.300,其余被 `VS` 拒到计次耗尽 → `g1` `Broken`(LegTracker `S1` +0.300、`P1` −0.400,裸永续空 0.100),时刻 `t_b`
  - `t_b + 10 s`:`margin_path` 使 `VP` 的 `liq_dist` 降到 0.09 并保持到 `t_b + 60 s`
- 种子:标准集(§2.4)
- 断言:
  - `t_b + 10 s` 那条 Input 内:`Note{MarginLevel}`(L3)、`ExecDecision{ForceStep, detail = Engine/CancelResting, arg = 3}`、`ExecDecision{ForceStep, detail = Engine/Step}`,`g1` 的引擎目标 = `P1` −0.320、`S1` 0.240(`reason = MarginStress`、`urgency = Urgent`);cell `state` 仍为 `Broken`
  - 先发 `P1` 买 0.080 IOC(`Flatten`,`REDUCE_ONLY`,`origin = Engine`,`ExecDecision.algo = Engine`),`P1` 到达 −0.320 之前无 `S1` 的减仓单;之后 `S1` 卖 0.060
  - 每步只减不增、不越过 0;`t_b + 120 s` 时引擎目标在位,无 `AutoFix/Start`(等待条件"有引擎目标")
  - 每步 INV-17、INV-19、INV-20 成立
- 实现:`crates/hft-sequencer/tests/scenarios_risk.rs::bk_3_l3_and_flatten_cover_broken`
- 备注:BK-3.1 在同一函数(B0 盘口 1 BTC × `depth_frac` 0.3,永续腿分片);BK-3.2、3.3 待补;实现方式:以真实 sequencer 的确定性步骤序列在 hft-sequencer B0 夹具上实现,映射 A≙`VS`、B≙`VP`、a1≙`S1`、b1≙`P1`、账户 1/2≙`a`/`b`,价格与阈值取 B0(见该文件头);W1 种子化 DST 版本待测试策略能发出 S1 买入第一腿后补

| 行 | 相对主例的变化 | 断言 |
|---|---|---|
| BK-3.1 | 不进入 L3;`t_b + 10 s` `Kill{Account(a), flatten}` 并确认 | 引擎目标 `P1` 0、`S1` 0(`reason = Hard`);先 `P1` 买 0.400 reduce-only,`P1` 到 0 后 `S1` 卖 0.300;两腿都到 0 的那条 Input 内 `CapCleared{Broken(a, g1)}`;`flatten_lock(a)` 保持到场景结束 |
| BK-3.2 | 同 BK-3.1,但 `max_reduce_notional = force_slice.max_chunk_notional = 6,050 USDT`(`P1` 每片 0.100) | `P1` 第一片成交后 `S1` +0.300、`P1` −0.300,裸量 0:该 Input 内 `CapCleared{Broken(a, g1)}` 与 `ExecDecision{State, Broken → Working}`;引擎目标(0, 0)保留,无 `TargetCleared`;此后 `P1` 与 `S1` 继续减到 0 |
| BK-3.3 | 不进入 L3、无 Kill;`t_b + 120 s` 自动有界平仓开始,其最后一片成交使裸量 ≤ 5 USDT | 该 Input 中 `Note{ExecAutoFixDone}` 的 seq 小于 `Note{CapCleared}`(`Broken`)的 seq;`AutoFix/Done` 恰好一条,无 `AutoFix/Failed` |

### BK-4 长停机后冷启动不在恢复瞬间判 Broken,对冲闸门子集满足后先续接一次

- 规则:[恢复:停机时间不计与闸门续接](../../crates/hft-oms/README.md#恢复停机时间不计与闸门续接)、[LegLink 与 HedgePlan](../../crates/hft-oms/README.md#leglink-与-hedgeplan)、[启动闸门](../../crates/hft-risk/README.md#启动闸门)、[断腿](../../crates/hft-risk/README.md#断腿broken自动有界平仓l3-与-flatten)
- 级别:dst
- 里程碑:M5
- 门槛:④、⑤
- 前置:W1,账户 `a`、`g1`,`probe-taker`;`hedge_deadline_ms = 3000`、`resume_hold_max_ms = 30000`、`hedge_retry_max = 3`、`hedge_retry_min_ms = 100`
- 故障:
  - t = 0.04 s `VP` 下单连接断开;t = 0 第一腿 `S1` 买 0.100 IOC,t = 0.05 s 成交;第二腿 `P1` 卖 0.100 本地发送失败(`NoConn`,门控),之后 HedgeRetry 一直门控
  - t = 1.25 s 进程崩溃(`CrashFault`),停机 30 min(墙钟);重启后 `VP` 的 kill 自检、UID、账户配置与下单连接在引擎时间 `t_g = t_r + 8 s` 全部满足(`hedge_gate_open` 列出 (a, VP));`VP` 接受此后的卖单并成交
- 种子:标准集(§2.4)
- 断言:
  - `recovery_end`:该链接置 `RESUME_HOLD`,`held_ms = 1,200`;`t_r` 到 `t_g` 无 `Broken`、无 `Note{OmsHedgeBroken}`、无 `CapSet{Broken}`
  - `t_g` 那条 Input 内:`SecondRetry`(`P1` 卖 0.100,`link_role = SecondRetry`)出环,`Note{OmsResumeRetry}`(P1,`v = [link_id, 1200, 0.100, 0.100]`),清 `RESUME_HOLD` 的 `LinkUpdate`
  - 续接单成交后链接 `Done`;到场景结束无 `Broken`、无 `HedgeFailure`
- 实现:`crates/hft-testkit/src/scenarios/broken.rs::bk_4_long_downtime_resume_once`

| 行 | 相对主例的变化 | 断言 |
|---|---|---|
| BK-4.1 | `t_g` 的续接单被 `VP` 拒,此后 `P1` 的报价连接静默(重试一律 `StaleQuote` 门控) | `t_g + 1,800 ms` 之前无 `Broken`;其后第一次 `on_hedge_retry` 判定时链接 `Broken`(`LinkBroken = Deadline(2)`)、`Note{OmsHedgeBroken}`(P0)、`CapSet{Broken(a, g1)}` |
| BK-4.2 | 崩溃时第一腿 IOC 已发出、无任何回报;重启后该槽 `Unknown`,查询在 `t_r + 40 s` 才返回 `Filled 0.100` | `held_ms = 0`;[`t_r + 30 s`, `t_r + 31 s`] 内的 sweep 写 `Note{OmsResumeTimeout}`(实际级别 P1,`v[4] = 2`(UNKNOWN)、`v[5] = 0`)与清 `RESUME_HOLD` 的 `LinkUpdate`;无 `Broken`、无 `HedgeFailure`、无 P0;`t_r + 40 s` 那条 `QueryResult` 的同一 Input 内第二腿 `P1` 卖 0.100(`link_role = Second`)出环 |

### BK-5 补腿进行中崩溃后,多出那一腿所在 venue 放行前不补腿、不判截止,放行时重给完整窗口

- 规则:[人工保持与恢复期抑制](../../crates/hft-exec/README.md#人工保持与恢复期抑制)、[Broken 处置](../../crates/hft-exec/README.md#broken-处置)、[补腿](../../crates/hft-exec/README.md#补腿hedgefixc4)、[外来成交的默认处理](../../crates/hft-exec/README.md#外来成交的默认处理b3)、[启动闸门](../../crates/hft-risk/README.md#启动闸门)
- 级别:dst
- 里程碑:M5
- 门槛:④、⑤
- 前置:W1,账户 `a`(`g1`)与 `b`(`g2`),两个实例的 `probe-taker` 参数 `close_on = []`、链接 `on_fail = Hold`、`tolerance = 0.02`;`dust_notional = 5 USDT`、`fix_max = 3`、`fix_cooldown_ms = 1000`、`fix_deadline_ms = 10000`、`external_deadline_ms = 5000`、`broken_auto_fix_ms = 120000`、`gate_naked_alarm_ms = 300000`
- 故障:
  - t = 0:`a` 的第一腿 `S1` 买 0.100 成交;第二腿 `P1` 卖 0.100 IOC 只成交 0.050,链接按 `Hold` 结束(`S1` +0.100、`P1` −0.050);第二腿终态的同一 Input 内补腿 `S1` 卖 0.050 出环,被 `VS` 以已登记拒单码拒绝(`fix.count = 1`,进入 1,000 ms 冷却)
  - t = 0 时 `b` 的 `g2` 为 `S1` +0.100、`P1` −0.100;t = 0.3 s `ExternalFill{cause = Unknown, S1 卖 0.050}`(账户 `b`)→ `g2` 永续多出 0.050,外来成交截止 = 5.3 s
  - t = 0.6 s 进程崩溃,停机 120 s(墙钟);重启后(引擎时间 `t_r`):`VS` 下单连接与用户流 `t_r + 5 s` Up、`VS` 对账报告 `t_r + 15 s` 干净;`VP` 下单连接 `t_r + 12 s` Up、`VP` 对账报告 `t_r + 18 s` 干净;此后两所接受并成交全部订单
- 种子:标准集(§2.4)
- 断言:
  - `t_r` 到 `t_r + 15 s`:`(a, g1)` 无 `reason ∈ {HedgeFix, ResidualTrim, ExternalDefault}` 的 `OrderDecision`,无 `Note{ExecBroken}`、无 `ExecDecision{State → Broken}`(停机前已过的冷却与 `fix_deadline_ms` 都不判)
  - `t_r + 15 s` 那条 Input(`venue_gate_open` 列出 (a, VS))内:`S1` 卖 0.050 IOC 出环(`reason = HedgeFix(5)`、`link_role = HedgeFix`);此后补腿的截止从这一刻起算
  - `(b, g2)`:`t_r` 到 `t_r + 22,999 ms` 无 `ExecDecision{ExternalDefault}`;`t_r + 23 s`(= `VP` 放行 + 5,000 ms)写 `ExecDecision{ExternalDefault}`、`Note{ExecExternalDefault}`(P0)与 `P1` 买 0.050 IOC(`REDUCE_ONLY`)
  - 到场景结束无 `Note{ExecGateHeldNaked}`;每步 INV-19、INV-20 成立
- 实现:`crates/hft-testkit/src/scenarios/broken.rs::bk_5_crash_during_fix_waits_venue_gate`

| 行 | 相对主例的变化 | 断言 |
|---|---|---|
| BK-5.1 | 崩溃前 `g1` 已 `Broken`(`S1` +0.100、`P1` 0);重启后 `VS` 照主例放行,`VP` 下单连接到场景结束都不 Up(账户启动闸门不放行) | `t_r + 135 s`(= `VS` 放行 + 120,000 ms)写 `AutoFix/Start` 并卖出 `S1`,不等账户闸门;`t_r + 134,999 ms` 前无 `AutoFix/Start`;自动平仓完成后无 `ExecGateHeldNaked` |
| BK-5.2 | 同 BK-5.1,但到场景结束都不 Up 的是 `VS` 的下单连接,`VP` 照主例放行;场景运行到 `t_r + 900 s` | 无 AutoFix 订单;`t_r + 300 s` 写 `Note{ExecGateHeldNaked}`(P0,`v[0] = a | g1 << 8`、`v[1] = 1`(场合 1)、`v[2] = 1`(Venue)、`v[3] = 300000`、`v[4] = 0`、`v[5] = 0.100`);到场景结束只有这一条 |
| BK-5.3 | 不崩溃;`g1` 于 `t_b` `Broken`(`S1` +0.100),账户闸门已放行;`t_b + 119 s` 起 `S1` 的 BBO 连接 Down 400 s | 到期后不开始;`t_b + 120 s + 300,000 ms` 写 `Note{ExecGateHeldNaked}`(P0,`v[1] = 2`(场合 2)、`v[2] = 4`(Quote)、`v[3] = 300000`);只此一条 |

### BK-6 低于最小下单量的残余由 WriteOffResidual 核销后 Broken 解除并可 Resume

- 规则:[残余与尘埃](../../crates/hft-exec/README.md#残余与尘埃)、[Broken 处置](../../crates/hft-exec/README.md#broken-处置)、[残余核销](../../crates/hft-oms/README.md#残余核销)、[断腿](../../crates/hft-risk/README.md#断腿broken自动有界平仓l3-与-flatten)、[cap 记录](../../crates/hft-risk/README.md#cap-记录)、[命令集](../subsystems/control-plane.md#命令集)
- 级别:dst
- 里程碑:M9
- 门槛:④
- 前置:W1,但 `S1` 的 `min_qty` = 0.001 BTC(lot 仍 0.00001,`min_notional` 5 USDT);`dust_notional = 5 USDT`、`broken_auto_fix_ms = 120000`、`max_reduce_notional = 10,000 USDT`、`force_slice.max_chunk_notional = 10,000 USDT`;`probe-taker` `close_on = []`;Admin 主体 `cli:admin1`
- 故障:
  - t = 0:第一腿 `S1` 买 0.1005 成交(OnFill:`q_fill = 0.100`,`g1` 在 `P1` 腿的余数 = 0.0005);第二腿 `P1` 卖 0.100 及其重试全部被 `VP` 拒 → `t_b` `Broken`(`S1` +0.1005)
  - `t_b + 120 s` 自动有界平仓第一片 `S1` 卖 0.1005,`S1` 买一档只有 0.100,IOC 成交 0.100
  - `t_b + 200 s` Admin 发 `WriteOffResidual{a, g1}` 并确认;`t_b + 210 s` Trader 发 `Resume{HedgeFailure, a}` 并确认;`t_b + 220 s` 第一腿 `S1` 买 0.0995 成交,第二腿 `P1` 照常接受
- 种子:标准集(§2.4)
- 断言:
  - 第一片终态后:裸量 0.0005 BTC(约 30 USDT > 5 USDT)、低于 `min_qty` → `Note{ExecAutoFixFailed}`(P0,`v[2] = 2`(MinOrder)、`v[4] = 0.100`、`v[5] = 0.1005`、`v[6] = 0.0005`),cell 保持 `Broken`
  - `t_b + 200 s` 那条 Input 内:`WriteOff{VS, S1, leg 0, qty 0.0005}` 转 `AdjustDelta::WriteOff`,ledger `inv(BTC, g1)` −0.0005、`unassigned(BTC)` +0.0005,`Note{ResidualWrittenOff}`;`g1` 两腿的 `residual` 都为 0;`CapCleared{Broken(a, g1)}`;无任何订单
  - `t_b + 210 s`:`CmdResult{Applied}` 与 `CapCleared{HedgeFailure(a)}`
  - `t_b + 220 s` 的 OnFill 第二腿数量 = 0.099(旧余数 0.0005 未并入),`g1` 在 `P1` 腿的余数 = 0.0005
- 实现:`crates/hft-testkit/src/scenarios/broken.rs::bk_6_write_off_residual_unblocks`
- 备注:装配期 `dust_notional` 小于交易腿 `min_notional` 最大值时 hft-core 退出 2(`DustBelowMinNotional`,[装配期交叉校验](../../apps/hft-core/README.md#内存与-journal-段按主机类唯一归属))与 `SetParams` 的同一校验由 hft-core 与 hft-sequencer 的 `TESTS.md` 覆盖

| 行 | 相对主例的变化 | 断言 |
|---|---|---|
| BK-6.1 | `t_b + 190 s`(核销之前)Trader 发 `Resume{HedgeFailure, a}` | `CmdResult{Rejected, detail = BrokenRemaining}`;`HedgeFailure(a)` 与 `Broken(a, g1)` 都在 |
| BK-6.2 | 核销时 `g1` 有一张非终态 IOC(`probe-taker` 在 `t_b + 199.9 s` 下的 `S1` 卖单,回报延迟 1 s) | `CmdResult{PreconditionFailed, detail = NotQuiescent(316)}`;LegTracker、余数与 ledger 不变,无 `Note{ResidualWrittenOff}` |
| BK-6.3 | 账户 `b` 的 `g2` 非 Broken:策略平仓后静止于 `S1` +0.0005、`P1` 0;Admin 核销后 pipeline 发移除 `g2` 的 `SetParams` | 核销:`CmdResult{Applied}`,`g2` 两腿净量 0;随后 `SetParams` → `CmdResult{Applied}`(无 `GroupHasPosition`);不核销直接移除 → `PreconditionFailed{GroupHasPosition}` |

### BK-7 resolve_broken 只在解除 HARD 时判定,容差内残余与人工保持的 cell 不判 Broken

- 规则:[HARD 解除时的 Broken 判定](../../crates/hft-exec/README.md#hard-解除时的-broken-判定resolve_broken)、[断腿](../../crates/hft-risk/README.md#断腿broken自动有界平仓l3-与-flatten)、[干净 Drain 重启自动解除 Recovery](../../crates/hft-risk/README.md#干净-drain-重启自动解除-recovery)、[模式与 cap 的驱动](../../crates/hft-sequencer/README.md#模式与-cap-的驱动)
- 级别:dst
- 里程碑:M2
- 门槛:④、⑤
- 前置:W1,`probe-taker` 链接 `tolerance = 0.02`、`on_fail = Hold`、`close_on = []`;`dust_notional = 5 USDT`、`external_deadline_ms = 5000`;`auto_resume_after_clean_drain = true`
- 故障:
  - t = 0:`a` 的第一腿 `S1` 买 0.100 成交,第二腿 `P1` 卖 0.100 IOC 只成交 0.099(`g1`:裸量 0.001 BTC,名义约 60 USDT,比例 1% ≤ 2%)
  - t = 1 s:`b` 的 `g2`(`S1` +0.100、`P1` −0.100)收到 `ExternalFill{cause = Unknown, S1 卖 0.050}`
  - t = 2 s `Drain` 返回 `Done`(非 `DrainTimeout`);同版本重启,尾部为空;`VS`、`VP` 分别在 `t_r + 15 s`、`t_r + 18 s` 放行(同 BK-5)
- 种子:标准集(§2.4)
- 断言:
  - 重启后到场景结束:无 `resolve_broken` 调用产生的 `CapSet{Broken}`、无 `HedgeFailure`;账户闸门放行的 Input 内 `CapCleared{Recovery}`(auto)
  - `g1` 无补腿单(容差内)
  - `g2`:`t_r + 23 s`(= `VP` 放行 + 5,000 ms)写 `ExecDecision{ExternalDefault}` 并发 `P1` 买 0.050(新窗口,不按停机前的截止)
- 实现:`crates/hft-sequencer/tests/scenarios_risk.rs::bk_7_resolve_broken_only_on_hard_release`
- 备注:BK-7.1 在同一函数;Drain 等待 `g2` 外来成交的默认处理(hft-sequencer Drain 第 3 步),所以 `g2` 的 ExternalDefault 在重启前(1 s + 5 s)发出,重启后不再处理;实现方式:以真实 sequencer 的确定性步骤序列在 hft-sequencer B0 夹具上实现,映射 A≙`VS`、B≙`VP`、a1≙`S1`、b1≙`P1`、账户 1/2≙`a`/`b`,价格与阈值取 B0(见该文件头);W1 种子化 DST 版本待测试策略能发出 S1 买入第一腿后补

| 行 | 相对主例的变化 | 断言 |
|---|---|---|
| BK-7.1 | 不 Drain;t = 5 s `Kill{Account(a)}`(非 HARD),t = 10 s `Resume{Kill}` 并确认 | `Resume` 的 Input 内只有 `CapCleared{Kill}`,无 `CapSet{Broken}`、无订单 |
| BK-7.2 | 不 Drain;账户 `a` 另有 group `g3`(`S2` = ETH-USDT 现货 @ `VS`,lot 0.0001;`P2` = ETH-USDT 永续 @ `VP`,lot 0.01;ETH 3,000 USDT;`S2` +1.00、`P2` −1.00);`g1` 起始为 `S1` +0.100、`P1` −0.100;t = 5 s `Kill{Account(a), hard}`;t = 6 s `ExternalFill{cause = Unknown, P1 买 0.010}`(`g1` 裸量 10%);t = 7 s `ExternalFill{cause = Manual, S2 卖 0.10}`(`g3` 裸量 10%,置 `manual_hold`);`ReconActor` 每 10 s 一组干净报告;t = 20 s `Resume{Kill}` 并确认 | t = 5–20 s 无任何 `Place`/`Amend` 出环;t = 20 s 那条 Input 内:`CapCleared{Kill}`、`CapSet{Broken(a, g1)}`、`CapSet{HedgeFailure(a)}`、`g1` 的 `ExecDecision{State → Broken}`;`g3` 无 `CapSet{Broken}`;该 Input 无任何 `OrderDecision{action = New}`;`g1` 的自动有界平仓不早于 t = 140 s 开始 |

### QF-1 第二腿盘口 10 s 不变而连接健康时照常定价发出第二腿

- 规则:[对冲定价](../../crates/hft-oms/README.md#对冲定价)、[OnFill](../../crates/hft-oms/README.md#onfill)、[报价可用](../../crates/hft-exec/README.md#报价可用连接健康)、[连接活性](../../crates/hft-sequencer/README.md#连接活性)、[BBO 连接健康](../../crates/hft-venue/README.md#3a-bbo-连接健康)、[检查链](../../crates/hft-risk/README.md#检查链按廉价--昂贵短路适用范围以本表为唯一定义)
- 级别:dst
- 里程碑:M2
- 门槛:④
- 前置:W1,账户 `a`、`g1`,`probe-taker`;`feed.bbo_ping_ms = 1000`、`conn_liveness_ms = 2000`、`hedge_deadline_ms = 3000`、`hedge_retry_min_ms = 100`;`HedgePlan{px_mode = Cross, px_param = 0.002, max_slip = 0.001}`;`P1` 价格带类别 `rr_max_cross = 0.01`
- 故障:`P1` 的 BBO 自 t = −10 s 起固定为 60,010.0 / 60,010.1,无任何行情记录;其 BBO 连接每 1,000 ms 回 pong(`ConnState{Alive}`),不重同步;t = 0 第一腿 `S1` 买 0.100,t = 0.2 s 全部成交
- 种子:标准集(§2.4)
- 断言:
  - t = 0 的第一腿 Open 未被 `LINK_STALE` 拒
  - t = 0.2 s 那条成交回报的同一 Input 内第二腿出环:`OrderDecision{link_role = Second, P1 卖 0.100, IOC}`,限价 = `cap_cross_px(Sell, 60,010.0, 60,010.05, 0.002, rr 0.01, tick 0.1)` = 59,890.0
  - t = 0 到 t = 10 s:无 `Note{OmsHedgePxStale}`、无 `Timer{Oms, HedgeRetry}` 登记、无 `RejectBits::STALE_MID`、无 `Broken`
  - 每步 INV-3、INV-8、INV-19 成立
- 实现:`crates/hft-sequencer/tests/scenarios_hedge.rs::qf_1_quiet_book_still_hedges`
- 备注:上线门槛 ④/⑦ 点名的必过项;QF-1.1 在同一函数,QF-1.4、QF-1.6 见 `qf_1_4_stale_at_fill_gated_then_retried`;QF-1.2、QF-1.3、QF-1.5 待 W1 种子化 DST;实现方式同 B1(真实 sequencer 在 B0 夹具上的确定性步骤序列,映射与阈值见该文件头)

| 行 | 相对主例的变化 | 断言 |
|---|---|---|
| QF-1.1 | t = 5 s 另一次开仓:第一腿 `S1` 买 0.050 带 `LegLink`(第二腿 `P1`) | 第一腿出环,无 `LINK_STALE` |
| QF-1.2 | t = 5 s `margin_path` 使 `VP` 进入 L3(`liq_dist_force = 0.10`,`liq_dist` 0.09) | 同一 Input 内 `P1` 的 `Flatten` 买单出环,无 `STALE_MID` |
| QF-1.3 | 另一条 `Broken` 的 group 多出永续腿,其自动有界平仓在 t = 5 s 到期 | t = 5 s 写 `AutoFix/Start`,`P1` reduce-only 买单出环 |
| QF-1.4 | `P1` 的 BBO 连接最后一帧在 t = −1.9 s,t = 1.0 s 恢复回帧 | t = 0 的 Open 放行(`now − conn_live_tsc` = 1,900 ms);t = 0.2 s 第二腿门控 `StaleQuote`:`Note{OmsHedgePxStale}`(P1)、登记 HedgeRetry、`retries` 不变;t = 1.0 s 之后的第一次 HedgeRetry(≤ t = 1.1 s)发出 `SecondRetry` `P1` 卖 0.100;无 `Broken` |
| QF-1.5 | t = 0.1 s 起 `P1` 的 BBO 连接 `ConnState{Resync, inst = 0}`,t = 1.0 s 下一条连接级 `Up` | 同 QF-1.4 |
| QF-1.6 | 同 QF-1.4,但连接到场景结束不恢复 | t = 3.2 s(第一腿成交 + 3,000 ms)之前无 `Broken`;其后第一次 HedgeRetry 判定时 `LinkBroken = Deadline(2)`、`Note{OmsHedgeBroken}`(P0)、`CapSet{Broken(a, g1)}` |

### LK-1 第二腿 venue 的 BBO 连接不健康时带 LegLink 的 Open 整体被拒且第一腿不发

- 规则:[检查链](../../crates/hft-risk/README.md#检查链按廉价--昂贵短路适用范围以本表为唯一定义)、[连接活性](../../crates/hft-sequencer/README.md#连接活性)、[报价可用](../../crates/hft-exec/README.md#报价可用连接健康)、[对冲定价](../../crates/hft-oms/README.md#对冲定价)、[BBO 连接健康](../../crates/hft-venue/README.md#3a-bbo-连接健康)
- 级别:dst
- 里程碑:M2
- 门槛:④
- 前置:W1,账户 `a`、`g1`,`probe-taker`(`spread_th` 使 `S1` 的触发 Bbo 必然发开仓);`conn_liveness_ms = 2000`、`feed.bbo_ping_ms = 1000`;`P1` 只有一条 BBO 连接(槽 0)
- 故障:t = 10.0 s `P1` 的 BBO 连接 `ConnState{Down, inst = 0}`;t = 10.5 s `S1` 推一条触发开仓的 Bbo
- 种子:标准集(§2.4)
- 断言:
  - t = 10.5 s 那条 Input 内:第一腿意图的 `OrderDecision{reject = LINK_STALE}`(只此一位),无 OMS 槽、无出环帧,`IntentHandle` = `Rejected(LINK_STALE)`
  - 无第二腿;t = 10.5 s 起到连接恢复,`g1` LegTracker 两腿净量不变
- 实现:`crates/hft-sequencer/tests/scenarios.rs::lk_1_unhealthy_second_leg_conn_rejects_open`
- 备注:LK-1.5 见 `lk_1_5_…`;实现方式:先以真实 sequencer 的确定性步骤序列在 hft-sequencer B0 夹具上实现,映射 A≙`VS`、B≙`VP`、a1≙`S1`、b1≙`P1`、账户 1/2≙`a`/`b`,阈值取 B0 参数,见该文件头;W1 种子化 DST 版本待测试策略能发出 S1 买入第一腿后补

| 行 | 相对主例的变化 | 断言 |
|---|---|---|
| LK-1.1 | `P1` 连接 t = 10.0 s 起 `ConnState{Resync, inst = 0}`(连接仍 Up) | 同主例(`LINK_STALE`) |
| LK-1.2 | `P1` 连接最后一帧在 t = 10.000 s,此后静默;触发 Bbo 在 t = 11.999 s | 放行:第一腿出环 |
| LK-1.3 | 同上,触发 Bbo 在 t = 12.000 s(`now − conn_live_tsc` = 2,000 ms) | 放行:第一腿出环 |
| LK-1.4 | 同上,触发 Bbo 在 t = 12.001 s | `OrderDecision{reject = LINK_STALE}`,第一腿不发 |
| LK-1.5 | 同 LK-1.2–LK-1.4,另有一条已在 `Hedging` 的链接在同一时刻到期重试 `P1` 卖单 | 11.999 s 与 12.000 s:该重试单出环;12.001 s:该重试单门控 `StaleQuote`(`Note{OmsHedgePxStale}`),与 Open 的判定一致 |
| LK-1.6 | `P1` BBO 自 t = 0 起不变,连接每 1,000 ms 回 pong;触发 Bbo 在 t = 10.5 s | 放行(盘口不变不算不健康,见 QF-1) |

### LK-2 第二腿不可负担时带 LegLink 的 Open 被 LINK_FUNDS 拒,恰好相等时放行

- 规则:[检查链](../../crates/hft-risk/README.md#检查链按廉价--昂贵短路适用范围以本表为唯一定义)、[PairOnFill](../../crates/hft-exec/README.md#paironfill)、[意图管线](../../crates/hft-sequencer/README.md#意图管线)
- 级别:dst
- 里程碑:M2
- 门槛:④
- 前置:W1,账户 `a`、`g1`;`probe-taker` 以 `P1` 卖 0.100 为第一腿、`S1` 买 0.100 为第二腿;`S1` BBO 固定 59,990.00 / 60,010.00(mid 60,000.00),`rr_max_cross = 0.01` → `link_notional` = 0.100 × 60,000.00 × 1.01 = 6,060.000000 USDT;`funds_buffer = 0.05`;`max_hedge_notional = 10,000 USDT`;账户 `a` 在 `VS` 的 USDT `free` = 6,362.999999(raw 6362999999,指数 −6),无在途现货买单
- 故障:t = 1 s `probe-taker` 发开仓
- 种子:标准集(§2.4)
- 断言:t = 1 s 那条 Input 内第一腿 `OrderDecision{reject = LINK_FUNDS}`,无出环帧;此后 1 s 内无该 group 的成交
- 实现:`crates/hft-sequencer/tests/scenarios.rs::lk_2_unaffordable_second_leg_rejects_open`
- 备注:LK-2.4–2.7;主例与 LK-2.1–2.3 见 `lk_2_spot_…`;实现方式:先以真实 sequencer 的确定性步骤序列在 hft-sequencer B0 夹具上实现,映射 A≙`VS`、B≙`VP`、a1≙`S1`、b1≙`P1`、账户 1/2≙`a`/`b`,阈值取 B0 参数,见该文件头;W1 种子化 DST 版本待测试策略能发出 S1 买入第一腿后补

| 行 | 相对主例的变化 | 断言 |
|---|---|---|
| LK-2.1 | `VS` USDT `free` = 6,363.000000(= `link_notional` × 1.05) | 放行,第一腿出环 |
| LK-2.2 | `VS` USDT `free` = 6,363.000001 | 放行 |
| LK-2.3 | `VS` USDT `free` = 6,400.000000,另有一张 0.001 BTC @ 60,000.00 的在途现货买单(在途名义 60 USDT) | `avail_quote` = 6,340.000000 < 6,363 → `LINK_FUNDS` |
| LK-2.4 | 第一腿 `S1` 买、第二腿 `P1` 卖;`P1` BBO 59,990.0 / 60,010.0;`AccountConfigReport` 给 `P1` 杠杆 5×;`MarginReport.avail` = 1,272.600000(= 6,060 / 5 × 1.05) | 放行 |
| LK-2.5 | 同 LK-2.4,`avail` = 1,272.599999 | `LINK_FUNDS` |
| LK-2.6 | 同 LK-2.4,`avail` = 1,500.000000;t = 0.5 s 新的 `AccountConfigReport` 给 `P1` 杠杆 4× | t = 1 s 的开仓按 4× 判定(需 1,590.750000)→ `LINK_FUNDS` |
| LK-2.7 | 同 LK-2.6,但无新的 `AccountConfigReport`(杠杆仍 5×) | 放行(需 1,272.600000) |

### LK-3 第二腿对手价超滑点但不超越价上限时按上限定价发出,超上限时门控等待

- 规则:[对冲定价](../../crates/hft-oms/README.md#对冲定价)、[OnFill](../../crates/hft-oms/README.md#onfill)、[检查链](../../crates/hft-risk/README.md#检查链按廉价--昂贵短路适用范围以本表为唯一定义)
- 级别:dst
- 里程碑:M2
- 门槛:④
- 前置:W1,账户 `a`、`g1`,`probe-taker`;`HedgePlan{px_mode = Cross, px_param = 0.002, max_slip = 0.001}`;`P1` `rr_max_cross = 0.01`;`hedge_deadline_ms = 3000`、`hedge_retry_min_ms = 100`;链接建立时 `P1` BBO 60,000.0 / 60,000.1(`ref_px` = 60,000.0)
- 故障:t = 0 第一腿 `S1` 买 0.100;t = 0.1 s `P1` BBO 变为 59,880.0 / 59,880.1(偏离 0.2% > `max_slip`);t = 0.2 s 第一腿全部成交
- 种子:标准集(§2.4)
- 断言:
  - t = 0.2 s 的同一 Input 内第二腿 `P1` 卖 0.100 IOC 出环,限价 = `cap` = 59,880.05 × 0.99 向上取到 tick = 59,281.3,`capped = true`
  - `Note{OmsHedgeSlipCapped}`(P1,`v = [link_id, 60,000.0, 59,880.0, 59,281.3]`)恰好一条;无 `retries` 增加、无 HedgeRetry 登记
  - 第二腿成交后链接 `Done`
- 实现:`crates/hft-sequencer/tests/scenarios.rs::lk_3_slip_capped_beyond_cross_gated`
- 备注:实现方式:先以真实 sequencer 的确定性步骤序列在 hft-sequencer B0 夹具上实现,映射 A≙`VS`、B≙`VP`、a1≙`S1`、b1≙`P1`、账户 1/2≙`a`/`b`,阈值取 B0 参数,见该文件头;W1 种子化 DST 版本待测试策略能发出 S1 买入第一腿后补

| 行 | 相对主例的变化 | 断言 |
|---|---|---|
| LK-3.1 | t = 0.1 s `P1` BBO 变为 58,000.0 / 62,000.0(bid < mid × 0.99 = 59,400.0),t = 1.0 s 恢复为 60,000.0 / 60,000.1 | t = 0.2 s 门控 `BeyondCross`:`Note{OmsHedgePxGated}`(P1)、HedgeRetry 登记、`retries` 不变;t = 1.0 s 后第一次重试发出 `SecondRetry`;无 `Broken` |
| LK-3.2 | 同 LK-3.1,BBO 到场景结束不恢复 | t = 3.2 s 前无 `Broken`;其后第一次重试判定时 `LinkBroken = Deadline(2)` 与 `Note{OmsHedgeBroken}`(P0) |

### LK-4 不带 client id 的第一腿成交按归属时刻二选一处理且同一成交不被对冲两次

- 规则:[回报匹配与成交增量](../../crates/hft-oms/README.md#回报匹配与成交增量)、[外来成交](../../crates/hft-oms/README.md#外来成交b3)、[OnFill](../../crates/hft-oms/README.md#onfill)、[外部成交](../../crates/hft-ledger/README.md#外部成交externalfill)、[gw 循环结构](../../crates/hft-venue/README.md#4-gwruntimepoll_once-的循环结构)、[契约 22](../../crates/hft-venue-proto/README.md#实现方必须遵守的契约)
- 级别:dst
- 里程碑:M4
- 门槛:④
- 前置:W1,账户 `a`、`g1`,`probe-taker` `close_on = []`;`gw.unmatched_hold_ms = 5000`(由 venue 行为层模拟)、`external_deadline_ms = 5000`、`tolerance = 0.02`
- 故障:t = 0 第一腿 `S1` 买 0.100 IOC;t = 0.05 s 交易所推送该单的成交(0.100 @ 60,000.00,`exch_order_id = X`)但不带 client id;该单的 Ack 在 t = 2.0 s 到达
- 种子:标准集(§2.4)
- 断言:
  - t = 2.0 s 成交以该槽的 `(epoch, slot, gen)` 入环;同一 Input 内第二腿 `P1` 卖 0.100 出环
  - 全程无 `ExternalFill`、无 `Note{OmsExtDedup}`;ledger `S1` 持仓 = 0.100
- 实现:`crates/hft-testkit/src/scenarios/link.rs::lk_4_fill_without_client_id_hedged_once`

| 行 | 相对主例的变化 | 断言 |
|---|---|---|
| LK-4.1 | Ack 在 t = 6.0 s 到达 | t = 5.05 s:`ExternalFill{cause = Unknown, S1 买 0.100, exch_order_id = X}`,ledger `Note{ExternalFill}`(P0),`g1` 现货腿 +0.100,`ext_held[X] = 0.100`;t = 6.0 s:槽成交 `d = 0.100` 全部被扣除 → 无 `FillOut`,`Note{OmsExtDedup}`(P1);同一 Input 内第二腿 `P1` 卖 0.100(= min(0.100, 裸量 0.100))出环;ledger `S1` 持仓 = 0.100(不是 0.200);t = 10.05 s `Note{ExecExternalDefault}`(P0,`qty = 0`) |
| LK-4.2 | 同 LK-4.1,`external_deadline_ms = 500` | t = 5.55 s 外来成交默认处理卖出 `S1` 0.100;t = 6.0 s OnFill 的 `naked_dir` = 0:`Note{OmsHedgeNakedCapped}`(P1,`v = [link_id, 0.100, 0, 0]`),无第二腿,`hedge_req` 不变,链接 `Done` |
| LK-4.3 | 同 LK-4.1,交易所在 t = 6.0 s 以相同 `trade_id` 再推一次该成交 | ledger 按契约 22 的键去重:不入账、无输出;仓位不变 |

### LT-1 两腿 lot 一方整除另一方时按较粗 lot 切片,不能整除且 lcm 名义超限的 SetParams 被拒

- 规则:[两腿 lot 对齐](../../crates/hft-exec/README.md#两腿-lot-对齐)、[OnFill](../../crates/hft-oms/README.md#onfill)、[命令路径](../../crates/hft-sequencer/README.md#命令路径)
- 级别:dst
- 里程碑:M2
- 门槛:④
- 前置:W1,但 `S1` lot 0.0001、`min_qty` 0.0001;`P1` lot 0.01、`min_qty` 0.01;`probe-target` 对 `g1` 发 `TargetIntent{S1 +0.130, P1 −0.130, PairOnFill{slice.max_chunk_notional = 3,000 USDT, depth_frac 1.0, depth_levels 1}}`;`force_slice.max_chunk_notional = 300 USDT`;`max_order_notional = 10,000 USDT`、`max_hedge_notional = 10,000 USDT`、`rr_max_cross = 0.01`
- 故障:t = 1 s 发出目标;每片 IOC 全部成交
- 种子:标准集(§2.4)
- 断言:
  - cell 的 `pair_lot` = 0.01;每片第一腿量是 0.01 的整数倍,依次为 0.04、0.04、0.04、0.01
  - 每片第二腿 = 该片第一腿成交净量,是对方 lot 的整数倍且 ≥ 其 `min_qty`;每片后 `g1` 两条腿的余数都为 0
  - 目标到达后 `ExecDecision{State → Idle}`,两腿净量 +0.130 / −0.130
- 实现:`crates/hft-sequencer/tests/scenarios_hedge.rs::lt_1_lot_alignment_and_mismatch`
- 备注:B0 价格下切片为 0.05/0.05/0.03(断言取 `pair_lot` 整数倍与合计);LT-1.1–LT-1.3 见 `lt_1_1_set_params_lcm_vs_force_chunk`;实现方式同 B1(真实 sequencer 在 B0 夹具上的确定性步骤序列,映射与阈值见该文件头)

| 行 | 相对主例的变化 | 断言 |
|---|---|---|
| LT-1.1 | pipeline 发 `SetParams` 新增 group `g5`(`S3` = X-USDT 现货 @ `VS`,lot 0.003;`P3` = X-USDT 永续 @ `VP`,lot 0.002;`P3` mark 60,000;lcm 0.006 → 360 USDT) | `CmdResult{Rejected, InvalidArgs, detail = LotMismatch}`;旧参数继续生效,group 表不变 |
| LT-1.2 | 同 LT-1.1,`force_slice.max_chunk_notional = 360 USDT` | `CmdResult{Applied}` |
| LT-1.3 | 同 LT-1.1,`force_slice.max_chunk_notional = 359.999999 USDT` | `InvalidArgs{LotMismatch}` |

### FL-1 flatten 锁期间远离 0 的目标与 Open 意图被拒,更接近 0 的目标与减仓意图放行

- 规则:[清仓锁](../../crates/hft-risk/README.md#清仓锁flatten_lock)、[保证金危急的默认减仓](../../crates/hft-exec/README.md#保证金危急的默认减仓a5与-engine_flatten)、[目标的接受与替换](../../crates/hft-exec/README.md#目标的接受与替换)、[检查链](../../crates/hft-risk/README.md#检查链按廉价--昂贵短路适用范围以本表为唯一定义)
- 级别:dst
- 里程碑:M2
- 门槛:④
- 前置:W1;账户 `a`、`b` 的 `g1`、`g2` 都为 `S1` +0.100、`P1` −0.100;两个实例同时运行 `probe-target`(目标)与直接下单意图(测试脚本经 `Ctx::place`);Trader 主体 `cli:trader1`
- 故障:
  - t = 1.000 s `Kill{Account(a), flatten}` 并确认(同一 Input 内 `P1` 买 0.100 的清仓 IOC 出环,t = 1.050 s 成交)
  - t = 1.010 s 实例 1 经 `Ctx::place` 发 `S1` 买 0.010 的 Open 意图与 `S1` 卖 0.010 的 Reduce 意图
  - t = 1.020 s 实例 1 对 `g1` 发 `TargetIntent{S1 +0.200, P1 −0.200, reason = Rebalance}`;t = 1.030 s 发 `TargetIntent{S1 0, P1 0, reason = Exit}`
  - t = 1.040 s 实例 2 对 `g2` 发同样的 `TargetIntent{+0.200, −0.200}` 与 `S1` 买 0.010 的 Open 意图
  - t = 10 s `Resume{Kill}` 并确认;t = 10.5 s 实例 1 重发 `TargetIntent{S1 +0.200, P1 −0.200, reason = Rebalance}`
- 种子:标准集(§2.4)
- 断言:
  - t = 1.010 s:Open 意图 `OrderDecision{reject = KILLED}`;Reduce 意图放行出环
  - t = 1.020 s:`ExecDecision{TargetRejected, detail = TargetReject/FlattenLocked(9)}`,引擎目标(0, 0)不变
  - t = 1.030 s:被接受并替换引擎目标,腿顺序 `PerpFirst`、`urgency = Urgent`,此后该 cell 的订单 `origin = Engine`
  - t = 1.040 s:账户 `b` 的目标被接受、Open 意图放行(不受 `a` 的锁影响)
  - t = 10 s:`CapCleared{Kill}`,`flatten_lock(a)` 清除;t = 10.5 s 的目标被接受(`TargetSet` 或 `TargetReplaced`),无 `FlattenLocked`
- 实现:`crates/hft-sequencer/tests/scenarios_risk.rs::fl_1_flatten_lock_rejects_away_from_zero`
- 备注:被接受的 (0, 0) `Exit` 目标是策略目标(hft-exec“目标的接受与替换”),清仓锁使它不能被清除;实现方式:以真实 sequencer 的确定性步骤序列在 hft-sequencer B0 夹具上实现,映射 A≙`VS`、B≙`VP`、a1≙`S1`、b1≙`P1`、账户 1/2≙`a`/`b`,价格与阈值取 B0(见该文件头);W1 种子化 DST 版本待测试策略能发出 S1 买入第一腿后补

### MS-2 单边 +30%(1 h 内)行情下引擎先于强平完成减仓,模拟真值中不发生强平

- 规则:[保证金分档](../../crates/hft-risk/README.md#保证金分档marginstress)、[保证金危急的默认减仓](../../crates/hft-exec/README.md#保证金危急的默认减仓a5与-engine_flatten)、[压力用例](../../crates/hft-sim/README.md#压力用例上线门槛必过项)
- 级别:dst
- 里程碑:M4
- 门槛:④、⑦
- 前置:W1,账户 `a` 满仓:`g1` 为 `S1` +1.000、`P1` −1.000(永续名义 60,000 USDT);`VP` 保证金余额 18,000 USDT(保证金余额 / 名义 = 0.30),`P1` 在 refdata 中的分档维持保证金率为单档 0.5%,交易所杠杆 5×;`margin_data = Push`(每 1 s 一条 `MarginReport`);`RiskParams` 保证金段:`liq_dist_block_open = 0.25`、`liq_dist_reduce = 0.15`、`liq_dist_force = 0.10`、`liq_dist_hyst = 0.02`、`margin_ratio_crosscheck = 0.80`、`margin_ratio_hyst = 0.05`、`deleverage_deadline_ms = 30000`、`force_step = 0.20`、`force_step_interval_ms = 5000`、`margin_stale_ms = 15000`、`mark_stale_ms = 10000`;`max_reduce_notional = 20,000 USDT`、`force_slice.max_chunk_notional = 20,000 USDT`;`probe-taker` `close_on = []`(只由引擎减仓)
- 故障:`one_sided_move(P1, +30%, 3_600_000, Linear)` 与 `one_sided_move(S1, +30%, 3_600_000, Linear)`(对永续空头不利的方向),之后保持 30 min;运行到 t = 5,400 s
- 种子:标准集(§2.4)
- 断言:
  - `MockVenue` 的 `SimExchange` 到场景结束没有产出 `ExternalFill{cause = Liquidation}`;journal 中无 `ExternalFill{Liquidation}`
  - 每步按模拟真值算的 `|liq_px − mark| / mark` > 0(无持仓时不计)
  - MarginStress 的首次进入时刻满足 L1 < L2 < L3(进入了的档);进入 L3 后每一步写 `Note{ExecForceStep}` 与 `ExecDecision{ForceStep}`
  - 每步 INV-3、INV-15、INV-17 成立
- 实现:`crates/hft-testkit/src/scenarios/margin.rs::ms_2_one_sided_30_no_liquidation`
- 备注:上线门槛 ④/⑦ 点名的必过项(本用例是测试策略版,⑦ 的必过项见 MS-2a)。"不利方向"与 hft-sim 现行方向定义的对齐:策略研究后定(R8-12)

| 行 | 相对主例的变化 | 断言 |
|---|---|---|
| MS-2.1 | `one_sided_move(…, Gap{at_ms: 2_400_000, gap_pct: 3%})`(第 40 min 跳空 3%) | 同主例 |
| MS-2.2 | 只对 `P1` 调 `one_sided_move`,`S1` 原样 | 同主例 |
| MS-2.3 | 账户 `b` 同样持有 `g2` `S1` +0.500、`P1` −0.500,保证金余额 / 名义 = 0.30;两个账户同时运行 | 两个账户各自满足主例断言 |

### MS-2a 每个上线策略以最终参数运行 one-sided-30 时零强平

- 规则:[压力用例](../../crates/hft-sim/README.md#压力用例上线门槛必过项)、[保证金分档](../../crates/hft-risk/README.md#保证金分档marginstress)
- 级别:dst
- 里程碑:M4
- 门槛:⑦
- 前置:已注册的上线策略,`StrategyParams` 与 `RiskParams` 取计划用于正式实盘的最终版本;起点为 ⑦ 长跑 journal 中被测 Sim 账户永续名义 ≥ 参数上限 90% 的快照;被测账户每个交易中的永续 inst 在 refdata 中有分档维持保证金率,`MarginModel.mmr = RefdataTiers`,杠杆取 `LeverageChoice::Operating`
- 故障:`StressSpec{case = OneSided30, path = OneSided{pct_bp: 3000, ramp_ms: 3_600_000, hold_ms: 1_800_000}, scope = AllInsts, qty_scale_bp = 10000}`,方向为对被测账户永续净持仓不利的一侧;一个策略的永续腿分属几个账户时对每个持永续仓位的账户各跑一次
- 种子:registry 集(§2.4)
- 断言:
  - 每次运行的 `StressVerdict{gate = true, liquidations = 0, passed = true}`,`min_liq_dist` > 0
  - 同一输入两次运行的 `StressVerdict` 逐字段相等
  - 某永续 inst 缺分档维持保证金率 → `StressTransform::new` 返回 `SimCfgErr::MmrTiersMissing`,用例拒绝运行(不按固定费率退化)
- 实现:`strategies/registry/tests/stress.rs::ms_2a_one_sided_30_launch_strategies`
- 备注:"不利方向"与 hft-sim 现行方向定义的对齐:策略研究后定(R8-12)

### MS-3 L3 期间另一所 BBO 断开 30 s 时永续腿照常按步 reduce-only 减仓,不等现货腿

- 规则:[保证金危急的默认减仓](../../crates/hft-exec/README.md#保证金危急的默认减仓a5与-engine_flatten)、[`ExecDecision.detail` 打包与子码](../../crates/hft-exec/README.md#execdecisiondetail-打包与子码)、[保证金分档](../../crates/hft-risk/README.md#保证金分档marginstress)、[报价可用](../../crates/hft-exec/README.md#报价可用连接健康)
- 级别:dst
- 里程碑:M4
- 门槛:④
- 前置:W1,账户 `a`:`g1` 为 `S1` +1.000、`P1` −1.000,另有不属于任何 group 的 `P1` −0.050(账户 `P1` 合计 −1.050);`P1` mark 60,000.0;`liq_dist_force = 0.10`、`liq_dist_hyst = 0.02`、`liq_dist_reduce = 0.15`、`liq_dist_block_open = 0.25`、`force_step = 0.20`、`force_step_interval_ms = 5000`;`max_reduce_notional = 30,000 USDT`、`force_slice.max_chunk_notional = 30,000 USDT`、`l3_max_cross = 0`;`conn_liveness_ms = 2000`;`dust_notional = 5 USDT`
- 故障:`S1` 的 BBO 连接自 t = −0.5 s 起停止回任何帧,t = 30 s 恢复;t = 0 起 `margin_path` 使 `VP` 的 `liq_dist` = 0.09 并保持到 t = 40 s
- 种子:标准集(§2.4)
- 断言:
  - t = 0 那条 Input 内:`Note{MarginLevel}`(L3);`g1` 的 `ExecDecision{ForceStep, detail = Engine/Step}` 与 `ExecDecision{ForceStep, detail = Engine/SerialFallback, arg = 1(StaleQuote)}`;`Note{ExecSerialFallback}`(P0,`v[1] = 0`、`v[2] = 1`、`v[3] = −1.000`、`v[4] = 1.000`);`P1` 买 0.200 IOC(`Flatten`、`REDUCE_ONLY`、`origin = Engine`)出环
  - 无组 cell:`ExecDecision{ForceStep, group = GROUP_NONE, detail = Engine/Ungrouped}`、`Note{ExecUngroupedReduce}`(P0,`v[0] = a`、`v[2] = 1`、`v[4] = 1`)、`P1` 买 0.010 reduce-only
  - `g1` 永续腿的引擎目标在 t = 0、5、10、15、20、25、30 s 依次为 −0.800、−0.640、−0.512、−0.409、−0.327、−0.261、−0.208(每步 × 0.8 后按 lot 0.001 向零取整);无组持仓的目标依次为 −0.040、−0.032、−0.025、−0.020、−0.016、−0.012、−0.009
  - t = 0 到 t = 30 s:无 `S1` 订单,`g1` 无带 `LegLink` 的订单(不切回 `PairOnFill`);t = 30 s 之后 `S1` 卖单把现货减到当时的引擎目标,之前永续腿始终先于现货腿到达各步目标
  - 每步 INV-3、INV-17、INV-19 成立
- 实现:`crates/hft-testkit/src/scenarios/margin.rs::ms_3_l3_other_venue_down_perp_still_reduces`
- 备注:上线门槛 ④/⑦ 点名的必过项

| 行 | 相对主例的变化 | 断言 |
|---|---|---|
| MS-3.1 | 不进入 L3;t = 0 `Kill{Account(a), flatten}` 并确认 | 引擎目标 0、`reason = Hard`;`SerialFallback`(arg 1)同主例;`P1` 买单按每片 ≤ 30,000 USDT 把 `g1` 永续减到 0;无组持仓减到 0,`Note{ExecUngroupedReduce}` 的 `v[1] = 0xFF`;`S1` 在 t = 30 s 之后卖到 0 |
| MS-3.2 | `S1` 报价照常,`VS` 下单连接 t = −0.5 s 到 t = 30 s Down(`venue_ready(a, VS) = false`) | `ExecDecision{ForceStep, detail = Engine/SerialFallback, arg = 4(NotReady)}`;其余同主例 |
| MS-3.3 | `S1` 断开延长到 t = 60 s;t = 32 s `liq_dist` 升到 0.30(档位回到 L0) | t = 32 s 那条 Input 内 `ExecDecision{ForceStep, detail = Engine/HardAlign}`:`g1` 引擎目标 `P1` = 当时永续净量、`S1` = 同量正数(≤ 现货净量),`reason = Hard`;t = 32–60 s 无补腿单、无 `Broken`;t = 60 s 之后 `S1` 卖到对齐量,到达后通知 `Done{engine}`、`serial` 清零 |

### MS-4 L3 路径变体中 MarginStress 依次经过 L1、L2、L3 且零强平

- 规则:[压力用例](../../crates/hft-sim/README.md#压力用例上线门槛必过项)、[保证金分档](../../crates/hft-risk/README.md#保证金分档marginstress)、[保证金危急的默认减仓](../../crates/hft-exec/README.md#保证金危急的默认减仓a5与-engine_flatten)
- 级别:dst
- 里程碑:M4
- 门槛:⑦
- 前置:同 MS-2a 的起点与最终参数;第一次运行杠杆取 `LeverageChoice::RiskCap`(`RiskParams.max_perp_leverage_x100` 允许的上限,向下取整到整数倍)
- 故障:`StressSpec{case = L3Path, path = OneSided{pct_bp: 3000, ramp_ms: 3_600_000, hold_ms: 1_800_000}, scope = AllInsts}`;该次运行未进入 L3 时改用 `Operating` 杠杆、`pct_bp = 4500` 再运行一次
- 种子:registry 集(§2.4)
- 断言:
  - 采用的那次运行:`first_level_tsc[0] < first_level_tsc[1] < first_level_tsc[2]`,三者都 ≠ 0;`deleverage_steps` ≥ 1
  - 每一次运行 `liquidations = 0`;`StressVerdict.passed = true`
- 实现:`strategies/registry/tests/stress.rs::ms_4_l3_path_levels_in_order`
- 备注:上线门槛 ⑦ 点名的必过项

| 行 | 相对主例的变化 | 断言 |
|---|---|---|
| MS-4.1 | 第一次运行(`RiskCap`、+30%)未进入 L3 | 第二次运行(`Operating`、+45%)被采用,`escalated = true`;两次都 `liquidations = 0` |
| MS-4.2 | 两次运行都未进入 L3 | `StressVerdict.passed = false`(用例未覆盖 L3 路径) |
| MS-4.3 | 任一次运行出现一条 `ExternalFill{Liquidation}` | `liquidations = 1`,`passed = false` |

### MS-5 设引擎减仓目标时先撤策略挂单,撤单确认前按即时在途截量,永续腿不被 REDUCE_INVALID 卡住

- 规则:[保证金危急的默认减仓](../../crates/hft-exec/README.md#保证金危急的默认减仓a5与-engine_flatten)、[输出扇出](../../crates/hft-oms/README.md#输出扇出)、[LegTracker](../../crates/hft-oms/README.md#legtracker)、[检查链](../../crates/hft-risk/README.md#检查链按廉价--昂贵短路适用范围以本表为唯一定义)
- 级别:dst
- 里程碑:M4
- 门槛:④
- 前置:W1,账户 `a`:`g1` 为 `S1` +1.000、`P1` −1.000;`P1` mark 60,000.0;策略已在 `P1` 上挂出 4 张已 Ack 的 reduce-only GTC 买单(各 0.250 @ 58,000.0,合计 = |仓位|);`liq_dist_force = 0.10`、`force_step = 0.20`、`force_step_interval_ms = 5000`;`max_reduce_notional = 30,000 USDT`、`force_slice.max_chunk_notional = 30,000 USDT`
- 故障:t = 0 `margin_path` 使 `VP` 的 `liq_dist` = 0.09(L3)并保持到 t = 20 s;4 张 GTC 的撤单确认在 t = 0.3 s 到达;t = 3 s 策略再挂一张 reduce-only GTC 买单 0.300 @ 58,000.0
- 种子:标准集(§2.4)
- 断言:
  - t = 0 那条 Input 内:`ExecDecision{ForceStep, detail = Engine/CancelResting, arg = 3}`、对 4 张 GTC 各一条 `OrderDecision{action = Cancel}`;同一 Input 内(撤单确认之前)`P1` 买 0.200 IOC(`Flatten`、`REDUCE_ONLY`)出环,数量 ≤ |仓位| − 同向即时在途 = 1.000
  - 全程无 `reject = REDUCE_INVALID` 的 `OrderDecision`
  - t = 5 s 的下一步:再写 `Engine/CancelResting`,t = 3 s 挂出的 GTC 被撤;`P1` 继续按步减
- 实现:`crates/hft-testkit/src/scenarios/margin.rs::ms_5_cancel_resting_before_engine_reduce`

| 行 | 相对主例的变化 | 断言 |
|---|---|---|
| MS-5.1 | 不进入 L3;t = 0 `Kill{Account(a), flatten}` 并确认 | 引擎目标 0(`reason = Hard`);同样先撤 4 张 GTC(`Engine/CancelResting`),撤单确认前 `P1` 买单照常出环;无 `REDUCE_INVALID` |

### MS-6 L3 中途重启且现货所断线时,清仓中的 cell 不等链接续接,直接 SerialFallback 减永续

- 规则:[保证金危急的默认减仓](../../crates/hft-exec/README.md#保证金危急的默认减仓a5与-engine_flatten)、[恢复:停机时间不计与闸门续接](../../crates/hft-oms/README.md#恢复停机时间不计与闸门续接)、[保证金分档](../../crates/hft-risk/README.md#保证金分档marginstress)、[快照与恢复](../../crates/hft-exec/README.md#快照与恢复)
- 级别:dst
- 里程碑:M4
- 门槛:④、⑤
- 前置:W1,账户 `a`:`g1` 为 `S1` +1.000、`P1` −1.000;`P1` mark 60,000.0;`liq_dist_force = 0.10`、`force_step = 0.20`、`force_step_interval_ms = 5000`;`resume_hold_max_ms = 30000`、`hedge_deadline_ms = 3000`;`max_reduce_notional = 30,000 USDT`、`force_slice.max_chunk_notional = 30,000 USDT`
- 故障:
  - t = 0 `margin_path` 使 `liq_dist` = 0.09(L3):`g1` 按 `PairOnFill`(永续先)发第一腿 `P1` 买 0.200,t = 0.05 s 成交;`VS` 下单连接 t = 0.04 s 断开,OnFill 的第二腿 `S1` 卖 0.200 本地发送失败(门控)
  - t = 1.0 s 进程崩溃,停机 120 s;重启后(`t_r`):`VS` 的行情与下单连接在 `t_r + 0.5 s` Up(`venue_ready(a, VS)` 为真、`S1` 报价可用),但 `VS` 的 `AccountConfigReport` 到场景结束不到达(`hedge_gate_ok(a, VS)` 为假);`VP` 的行情连接与账户推送在 `t_r + 1 s` 才 Up,本次运行第一条 `MarkPrice` 与 `MarginReport`(`liq_dist` 0.09)都在 `t_r + 1 s` 到达
- 种子:标准集(§2.4)
- 断言:
  - 恢复后:链接 `RESUME_HOLD`,`held_ms = 950`;`force_active(a)` 与 cell 的引擎目标从快照恢复
  - `t_r` 到 `t_r + 1 s`:无 `ForceDeleverage` 输出(`MarginForce` 到期也不出步)
  - `t_r + 1 s` 那条 `MarginReport` 的 Input 内:L3 出步,`ExecDecision{ForceStep, detail = Engine/SerialFallback, arg = 5(ResumeHold)}`、`Note{ExecSerialFallback}`(P0,`v[2] = 5`),`P1` 买 reduce-only 单(不带 `LegLink`)出环
  - [`t_r + 30 s`, `t_r + 31 s`] 内的 sweep:`Note{OmsResumeTimeout}`(P0,`v[4] = 1`(GATE)、`v[5] = 0.200`)、链接 `Broken{ResumeTimeout}`、`CapSet{Broken(a, g1)}`、`CapSet{HedgeFailure(a)}`;此后 `g1`(已 Broken)的永续腿仍按步减
  - 每步 INV-17、INV-19 成立
- 实现:`crates/hft-testkit/src/scenarios/margin.rs::ms_6_l3_restart_spot_down_serial_fallback`

| 行 | 相对主例的变化 | 断言 |
|---|---|---|
| MS-6.1 | `VS` 的 `AccountConfigReport` 在 `t_r + 10 s` 到达,`hedge_gate_ok(a, VS)` 由此满足 | `t_r + 10 s` 那条 Input 内续接:`SecondRetry` `S1` 卖 0.200、`Note{OmsResumeRetry}`(P1);无 `OmsResumeTimeout`、无 `Broken`;cell 仍走串行腿直到到达引擎目标 |
| MS-6.2 | t = 0 用 `Kill{Account(a), flatten}`(确认)代替 L3,其余相同 | 恢复后 `flatten_lock(a)` 保持;`t_r` 之后第一次推进写 `Engine/SerialFallback, arg = 5`,`P1` reduce-only 买单出环,不等链接续接 |

### AX-1 用户流缺口里漏掉的 ADL 经两轮稳定对账后在 ReconcileReport 内确定地合成外来成交入账

- 规则:[ReconcileReport](../../apps/reconciler/README.md#reconcilereport)、[对账分级](../../crates/hft-risk/README.md#对账分级全系统唯一一次)、[每条 Input 的处理顺序](../../crates/hft-sequencer/README.md#每条-input-的处理顺序process)、[外部成交](../../crates/hft-ledger/README.md#外部成交externalfill)、[外来成交的默认处理](../../crates/hft-exec/README.md#外来成交的默认处理b3)
- 级别:dst
- 里程碑:M9
- 门槛:④
- 前置:W1,账户 `a`:`g1` 为 `S1` +1.000、`P1` −1.000;`P1` mark 60,010.0;`ReconActor` 对 (a, VP) 每 30 s 一轮(视图点 `S0` = t = 30、60、90、120 s),`user_stream_lag = 2000 ms`;`recon_closeonly_notional = 100,000 USDT`;`external_deadline_ms = 5000`、`tolerance = 0.02`、`dust_notional = 5 USDT`;`probe-taker` `close_on = []`
- 故障:t = 0 `VP` 用户流 `ConnState{UserStream, Down}`;t = 10 s 模拟交易所对账户 `a` 的 `P1` 仓位做 ADL(真值 −1.000 → −0.700),不推送任何成交;t = 20 s 用户流 `Up`
- 种子:标准集(§2.4)
- 断言:
  - t = 30 s 的报告:`Position{P1, engine_qty −1.000, exch_qty −0.700}`(`Warn`),不带 `AUTO_EXTERNAL`;无外来成交入账
  - t = 60 s 的报告:同一条目带 `AUTO_EXTERNAL`;sequencer 复核通过,在这条 `ReconcileReport` Input 内入账 `ExternalFill{cause = Unknown, side = Buy, qty = 0.300, px = 60,010.0, trade_id = 0, exch_order_id = 0, fill_seq = 该组 report_id}`:ledger `Note{ExternalFill}`(P0,`aflags` 含 `AUTO_RECON`)、`auto_held(a, P1, Buy)` = 0.300、`g1` 永续腿净量 −0.700;journal 中没有对应的 Input 类 `ExternalFill` 记录;处置以 `Gate(Disposed{…, AutoExternal})` 报告
  - 同一 Input 内策略实例 1 收到一条 `EventRef::External`;t = 65 s 外来成交默认处理卖出 `S1` 0.300(`ExecDecision{ExternalDefault}`、`Note{ExecExternalDefault}` P0)
  - t = 90 s 的报告:该 inst 的仓位差在容差内(无 `Position` 条目或为 `Ignore`);reconciler 的引擎视图与 hft-py 经 hft-ledger 的 journal 消费路径对该注记调同一 `auto_external_fill`,得到的 `auto_held` 与 `P1` 持仓与热进程相同;`replayer verify` 对该段 journal 逐字节一致
  - 无 `Recon` cap(名义 18,000 USDT < 100,000)
- 实现:`crates/hft-testkit/src/scenarios/external.rs::ax_1_userstream_gap_adl_auto_external`
- 备注:reconciler 进程侧的置位与引擎视图由 reconciler 的 `TESTS.md` 覆盖;watchtower 的 `ExternalFill` P0 与 `ReconcileMismatch` 详情由 watchtower 的 `TESTS.md` 覆盖

| 行 | 相对主例的变化 | 断言 |
|---|---|---|
| AX-1.1 | ADL 到 0(真值 −1.000 → 0) | t = 60 s 合成 `Buy 1.000`;`g1` 永续腿净量 0 |
| AX-1.2 | t = 70 s 用户流补推同一笔真实 ADL 成交 `ExternalFill{cause = Adl, Buy 0.300}` | ledger 抵扣:`Note{LedgerAutoHeldOffset}`(P0,`v[3] = 0.300`、`v[4] = 0.300`、`v[5] = 0`),`apply_external` 返回 `None`;无 `external_adjust`、无 `exec.on_external`、无 `EventRef::External`、无第二次外来成交默认处理;`P1` 持仓仍 −0.700 |
| AX-1.3 | 同 AX-1.2,但补推的成交为 `Buy 0.400` | `off = 0.300`,只入账 0.100:`P1` 持仓 −0.600,`auto_held = 0`,`Note{LedgerAutoHeldOffset}` 的 `v[4] = 0.300`、`v[5] = 0` |
| AX-1.4 | 用户流在 t = 35 s 才 `Up`(t = 30 s 的报告带 `USERSTREAM_DOWN`) | t = 60 s 的条目不带 `AUTO_EXTERNAL`;t = 90 s 的条目带并入账 |
| AX-1.5 | 用户流 t = 20 s Up 之后,t = 45 s Down、t = 46 s Up | t = 60 s 不置位(最近一次 Up 晚于 t = 30 s 的视图点);t = 90 s 置位并入账 |
| AX-1.6 | 用户流 t = 59 s Down、t = 59.5 s Up(t = 60 s 的报告带 `USERSTREAM_DOWN`) | t = 60 s 与 t = 90 s 都不置位;t = 120 s 置位并入账 |
| AX-1.7 | 差异为交易所仓位变大(真值 −1.200)或变号(真值 +0.100),或为现货 `Balance` 差 | 三轮都不带 `AUTO_EXTERNAL`、无合成成交;条目保持 `Warn`,直到人工 `AdoptExchange` |
| AX-1.8 | `recon_closeonly_notional = 10,000 USDT` | t = 60 s 同一 Input 内另有 `CapSet{Recon(a, VP)}`;t = 90、120 s 两轮 `Ignore` 之前 `Resume{Recon, a, VP}` 被拒,之后被接受 |

### FD-1 资金费逐笔差异由账本自动校正一次,下一轮该笔不再报差

- 规则:[资金费比对](../../apps/reconciler/README.md#资金费比对)、[资金费对账校正](../../crates/hft-ledger/README.md#资金费对账校正)、[对账分级](../../crates/hft-risk/README.md#对账分级全系统唯一一次)
- 级别:dst
- 里程碑:M9
- 门槛:④
- 前置:W1,账户 `a`:`g1` `P1` −1.000;`VP` 为 `PerSymbolPush`;`funding_recon_lag_ms = 600000`、`funding_recon_window_ms = 172800000`;reconciler `funding_dust = 0.01 USDT`;`ReconActor` 每 30 s 一轮
- 故障:UTC 08:00:00 结算;08:00:01 推送 `FundingSettle{P1, +12.000 USDT}` 入账;交易所逐笔流水为该笔 +9.000 USDT(差 −3.000);对账在 08:15:00 与 08:15:30 各一轮
- 种子:标准集(§2.4)
- 断言:
  - 08:15:00 的报告含 `Funding{a, VP, P1, exch_ts_ns = 08:00:00, exch_amount 9.000, engine_amount 12.000, diff −3.000}`;同一 Input 内 ledger `AdjustDelta{kind = Funding, inst = P1, asset = USDT, group = g1, qty = −3.000}`,`VP` USDT `total` 与 `wallet` 各 −3.000,`g1` 的 `PnlBreakdown.funding` −3.000;`Note{FundingCorrected}`(P1,`v[3] = 9.000`、`v[4] = 12.000`、`v[5] = −3.000`、`v[7] = 0`)
  - `RECONCILED(a, VP)` 不因该条目改变;无 `CapSet{Recon}`
  - 08:15:30 的报告无该笔的 `Funding` 条目;到场景结束 (a, VP, P1, 08:00:00) 的 `Note{FundingCorrected}` 恰好一条
- 实现:`crates/hft-testkit/src/scenarios/funding.rs::fd_1_funding_diff_auto_corrected_once`
- 备注:watchtower `FundingDiff` 对这一笔只报一次 P1,由 watchtower 的 `TESTS.md` 覆盖

| 行 | 相对主例的变化 | 断言 |
|---|---|---|
| FD-1.1 | 对账视图点 `S0` = 结算 + 599,999 ms | 该视图点的报告中无该笔 `Funding` 条目(晚于 `S0 − funding_recon_lag_ms`),无校正 |
| FD-1.2 | `S0` = 结算 + 600,000 ms | 该视图点的报告中有该笔条目并校正 |
| FD-1.3 | `S0` = 结算 + 600,001 ms | 该视图点的报告中有该笔条目并校正 |
| FD-1.4 | `S0` = 结算 + 172,800,001 ms(早于 48 h 窗口) | 无条目、无校正 |
| FD-1.5 | `S0` = 结算 + 172,800,000 ms | 有条目并校正 |

### FD-2 资金费差额同额出现在结算资产余额上时余额条目经资金费修正后为 Ignore

- 规则:[资金费比对](../../apps/reconciler/README.md#资金费比对)、[对账分级](../../crates/hft-risk/README.md#对账分级全系统唯一一次)、[启动闸门](../../crates/hft-risk/README.md#启动闸门)、[资金费对账校正](../../crates/hft-ledger/README.md#资金费对账校正)
- 级别:dst
- 里程碑:M9
- 门槛:④
- 前置:同 FD-1;另:引擎视图 `VP` USDT 余额 50,000.000,交易所余额 49,997.000;USDT 的 `AssetMeta.dust` = 0.01;场景从冷启动开始(`EpochStart` 之后第一组报告决定 `RECONCILED(a, VP)`)
- 故障:同 FD-1;第一组报告即在 08:15:00
- 种子:标准集(§2.4)
- 断言:
  - 08:15:00 这一组:`funding_adj(USDT) = −3.000`,余额比较 `49,997.000 − (50,000.000 − 3.000) = 0` → 无 `Warn` 的 `Balance` 条目;`RECONCILED(a, VP)` 置位;无 `CapSet{Recon}`
  - ledger 恰好一条 `AdjustDelta::Funding`(−3.000);08:15:30 这一组:无 `Funding` 条目,USDT 余额差 0,无 `Balance` 条目,ledger 无第二条校正
- 实现:`crates/hft-testkit/src/scenarios/funding.rs::fd_2_balance_diff_offset_by_funding_items`

| 行 | 相对主例的变化 | 断言 |
|---|---|---|
| FD-2.1 | 08:15:00 这一轮交易所资金费流水查询失败(本组无 `Funding` 条目) | 不修正:`Balance{USDT, engine 50,000.000, exch 49,997.000}` `Warn`;`RECONCILED(a, VP)` 不置位;ledger 无校正 |

### AL-1 策略按 Advice 的依据而非到达时刻判过期,advisor 落后时写 AdvisorLag

- 规则:[Advice 载荷](../../crates/hft-strategy-sdk/README.md#advice-载荷)、[Advice 路由](../../crates/hft-sequencer/README.md#advice-路由)、[advisor 与 Model](../../crates/hft-sequencer/README.md#advisor-与-model)、[AdvisorLag](../../crates/hft-event/README.md#控制与系统0x04xx0x05xx)
- 级别:dst
- 里程碑:M9
- 门槛:④
- 前置:W1,实例 1 为 `probe-target`,其 Model 每次 `Timer{Sys, AdvisorTick}`(每 100 ms)产出一条 `Advice`;快速部分用 hft-strat-lib `Expiring` 缓存 Advice,参数 `advice_max_age_ms = 1000`;时间表每 500 ms 一个增仓目标点(t = 10.0、10.5、11.0 … s),到点时缓存中无未过期 Advice 则跳过该点;`advisor_max_lag_ms = 2000`
- 故障:`AdvisorActor` 注入落后:t ∈ [10 s, 13 s) Model 处理 journal 落后 3,000 ms(按 `engine_ns`),这段时间产出的 Advice 随之推迟 3,000 ms 交付;t ≥ 13 s 恢复即时
- 种子:标准集(§2.4)
- 断言:
  - 每条投递给策略的 `EventRef::Advice` 的 `basis_seq`、`basis_tsc` 与 journal 中该 Advice 记录的 `AdviceHdr` 相同(sequencer 不加过期标志)
  - 时间表点 t = 10.0、10.5 s 发目标(`ExecDecision{TargetSet | TargetReplaced}`,依据 t = 9.9 s 的 Advice);t = 11.0、11.5、12.0、12.5 s 无该实例的目标决定;依据落在 [10 s, 13 s) 的 Advice 到达时 `now.tsc − basis_tsc` = 3,000 ms,判过期,不使缓存变新;t = 13.0 s 起各点照常发目标(依据即时交付的 Advice)
  - 落后量首次超过 2,000 ms 之后写一条 `Note{source = Advisor, code = AdvisorLag}`(P1,`v[0]` = 实例 1,`v[1]` > 2000);落后期间每 10 s 至多一条,本场景恰好一条
  - 同一 journal 的 `replayer verify` 逐字节一致(判定只依赖 journal 中的 `basis_tsc` 与 Input 的 tsc)
- 实现:`crates/hft-testkit/src/scenarios/advisor.rs::al_1_advice_expiry_by_basis`
- 备注:watchtower 对 `AdvisorLag` 报 P1、不另设阈值,由 watchtower 的 `TESTS.md` 覆盖

| 行 | 相对主例的变化 | 断言 |
|---|---|---|
| AL-1.1 | 落后与推迟都是 2,000 ms | 无 `AdvisorLag` |
| AL-1.2 | 落后与推迟都是 2,001 ms | 恰好一条 `AdvisorLag` |
| AL-1.3 | t = 20 s 进程重启;advisor 从检查点续算,追赶期间产出依据上次启动记录的 Advice(`basis_seq < Now.boot_seq`) | 这些 Advice 全部判过期(即使 `basis_tsc` 与当前 tsc 之差 < 1,000 ms);第一条 `basis_seq ≥ boot_seq` 的 Advice 才判新鲜 |

## 8. OMS 与执行的必测失败路径

本节每个用例只断言 OMS、风控、执行层与 ledger 在该失败路径上的联动;延续到断腿处置、第二腿定价等场景的部分在 `备注` 指向对应编号,不在这里重复。本节用例的 MockVenue 回报时延固定为 5 ms(另有写明的除外),种子只决定行情噪声与同一时刻到达的 Input 的交错。

### FP-1 下单后连接断开且无回执时槽进入 Unknown,重连后经查询收敛,期间该 group 不开新仓

- 规则:[Unknown 收敛流程](../../crates/hft-oms/README.md#unknown-收敛流程)、[回报处理表](../../crates/hft-oms/README.md#回报处理表)、[OnFill](../../crates/hft-oms/README.md#onfill)、[检查链](../../crates/hft-risk/README.md#检查链按廉价--昂贵短路适用范围以本表为唯一定义)
- 级别:dst
- 里程碑:M2
- 门槛:④
- 前置:W1;账户 `a`、group `g1`;`unknown_blocks_open = true`;`ack_timeout_ms[VS] = 2000`;`unknown_query_interval_ms = 1000`;`unknown_max_queries = 5`;`unknown_alarm_ms = 30000`;`hedge_deadline_ms = 3000`;`hedge_retry_min_ms = 100`;`rr_max_cross = 0.002`;`conn_liveness_ms = 2000`;`bbo_ping_ms = 1000`;probe-taker 在 t = 0 发带 `LegLink` 的 Open:`S1` 买 0.01 BTC(raw 1,000,000,`qty_exp = −8`)IOC 限价 60,030.00,`HedgePlan{kind = OnFill, P1 卖, ratio 1/1, on_fail = Retry, px_mode = Cross, px_param = 0.0005, max_slip = 0.0005}`
- 故障:
  - t = 0.001 s 该单 `SendReport{Place, Written}`;t = 0.003 s MockVenue 在真值中全部成交(60,000.00 × 0.01,手续费 0.6 USDT,`fee_asset = USDT`),但丢弃该单的全部回报
  - t = 0.004 s `VS` 的下单连接与用户流 `ConnState{Down}`(`role = Order`、`role = UserStream`)
  - t = 1.0 s probe-taker 在 `g1` 再发一笔同样的 Open
  - t = 3.0 s 两条连接 `ConnState{Up}`;MockVenue 对随后的 `Query` 回 `QueryResult{status = Filled, cum_filled = 0.01, avg_px = 60,000.00}`
- 种子:标准集(§2.4)
- 断言:
  - t = 0.004 s 那条 Input 内:槽 s1 `PendingNew → Unknown`(`Note{OmsTransition}` 的 cause = `ConnDown`(15),`prev_state = PendingNew`),`Note{OmsUnknown}` 级别 P1,给实例 1 的 `OrderUpdate{Unknown}`;`venue_ready(VS, a) = false`;`legs(a, g1).has_unknown = true`,`S1` 腿 `inflight_immediate = +0.01`;本 Input 没有 `OrderDecision{Query}`(连接 Down)
  - t ∈ (0.004 s, 3.0 s):t = 1.0 s 的 Open 写成 `OrderDecision{reject = VENUE_NOT_READY}`(唯一一位),不出环;s1 没有迁到 `Rejected` 或 `Cancelled`;ledger 的 `S1` 库存保持 0;InflightBook `imm_buy(a, S1) = 0.01`;t = 1.004 s 与 2.004 s 的 `Timer{Oms, UnknownQuery}` 到期不发 `Query`、重新登记,`query_count = 0`
  - t = 3.0 s 的 `ConnState{Order, Up}` 同一 Input 内恰好一条 `OrderDecision{Query, slot = s1}`;其 `SendReport{Query, Written}` 之后 `query_count = 1`
  - `QueryResult` 那条 Input 内:s1 `Unknown → Filled`(cause = `QueryResult`(12));`FillOut{qty = 0.01, net_qty = 0.01, flags = 0}`;ledger `S1` 库存 0.01;同一 Input 内 `OrderDecision{New, inst = P1, side = Sell, qty = 0.01, tif = Ioc, link_role = Second}`,`oflags` 含 `RISK_REDUCING`、不含 `REDUCE_ONLY`;`Note{OmsVenueReady}` 恰好一条;`venue_ready(VS, a) = true`,`has_unknown = false`
  - 全程每步 INV-1、INV-2、INV-4、INV-7 成立
- 实现:`crates/hft-sequencer/tests/scenarios.rs::fp_1_conn_down_no_ack_converges_by_query`
- 备注:连接断开期间链接截止不计(第一腿尚无已知成交,`first_fill_tsc = 0`);恢复后的续接见 BK-4;FP-1.1–1.3 见 `fp_1_1_…`、`fp_1_2_…`、`fp_1_3_…`;实现方式:先以真实 sequencer 的确定性步骤序列在 hft-sequencer B0 夹具上实现,映射 A≙`VS`、B≙`VP`、a1≙`S1`、b1≙`P1`、账户 1/2≙`a`/`b`,阈值取 B0 参数,见该文件头;W1 种子化 DST 版本待测试策略能发出 S1 买入第一腿后补

| 行 | 相对主例的变化 | 断言 |
|---|---|---|
| FP-1.1 | MockVenue 真值中该 IOC 零成交到期;t = 3.0 s 后回 `QueryResult{status = Expired, cum_filled = 0}` | s1 `Unknown → Cancelled`;链接 `LinkUpdate{state = Done}`(第一腿零成交终态);全程没有 `link_role = Second` 的 `OrderDecision`;ledger 不变;`Note{OmsVenueReady}` 一条 |
| FP-1.2 | 两条连接 t = 0.004 s 断开后保持 40 s,t = 40.0 s 恢复并按主例收敛 | t ∈ [30.004 s, 31.004 s] 内 sweep 对 s1 再写一条 `Note{OmsUnknown}`(level = P0);同一 Input 设 `UnknownTimeout{a, VS}` cap(`Note{CapSet}` P1);cap 存续期间 `a` 在 `VS` 上的 Open 被拒,风险递减单不受它影响;收敛之后经预览与确认的 `Resume{UnknownTimeout, a, VS}` 得 `CmdResult{Applied}` 并删除该 cap |
| FP-1.3 | t = 3.0 s 连接恢复后 MockVenue 对 s1 的 `Query` 一律不回(查询超时) | 每 1 000 ms 一条 `Query`;第 6 条 `SendReport{Query, Written}` 之后 `query_count = 6 > unknown_max_queries`,同一 Input 写 `Note{OmsUnknownStuck}`(P0)恰好一条;s1 始终为 `Unknown`,不迁到任何终态 |

### FP-2 交易所限速拒单使该 (venue, account) 令牌桶清零并按指数退避

- 规则:[令牌桶](../../crates/hft-risk/README.md#令牌桶)、[检查链](../../crates/hft-risk/README.md#检查链按廉价--昂贵短路适用范围以本表为唯一定义)、[产生与消费的事件](../../crates/hft-venue/README.md#产生与消费的事件)、[SendReport 处理](../../crates/hft-oms/README.md#sendreport-处理)、[Throttle](../../crates/hft-strat-lib/README.md#throttle)
- 级别:dst
- 里程碑:M2
- 门槛:④
- 前置:W1;refdata `RateLimitTable(VS, a)`:10 s 窗口 100 单、60 s 窗口 1 200 单;`rate_headroom = 0.2`(两窗口容量 80 与 960,补充速率 8/s 与 16/s);`rr_reserve = 0.2`;`inst_bucket_per_s = 50`;`backoff_base_ms = 1000`;`backoff_max_ms = 30000`;`max_pos_qty = 1 BTC`;`g1` 已有 `S1` +0.01、`P1` −0.01(已对冲);MockVenue 的拒单码表把 `-1015` 登记为确定拒绝(限速,请求未处理),并在回该码时同时产出 `ConnState{Degraded, role = Order, reason = RATE_LIMIT}`(11);t = 0 时两窗口令牌满
- 故障:
  - t = 0 probe-taker 在 `S1` 发单腿 Open(买 0.01 IOC,无链接);MockVenue 回 `OrderEvent{Reject, reject_code = 0xFFFFFC09}`(`-1015` 按契约 8 的 i32 补码)与上述 `ConnState`
  - t = 0.5 s:策略发一笔 Open(`S1` 买 0.01);策略另发一笔 `S1` 卖 0.005 的 Reduce 意图(分类 `Reduce`、`RISK_REDUCING`)
  - t = 1.5 s:同样各发一笔
  - t = 13.0 s:策略再发一笔 Open
- 种子:标准集(§2.4)
- 断言:
  - t = 0 的回报:该槽 `PendingNew → Rejected`,给实例 1 的 `OrderUpdate::Rejected{bits = 0, venue_code = 0xFFFFFC09}`;处理该 `ConnState` 的 Input 内两窗口令牌为 0、`backoff_until_ens = t0 + 1 000 ms`(k = 0),写 `Note{RateLimitHit}`(source = Risk,P2)恰好一条;`risk_for(a, S1).rate_backoff = true`;`venue_ready(VS, a)` 保持 true(限速不是连接失效)
  - t = 0.5 s:Open 与 Reduce 单都 `reject = RATE_BACKOFF`([检查链](../../crates/hft-risk/README.md#检查链按廉价--昂贵短路适用范围以本表为唯一定义) #8 先查退避;退避期内桶已清零,所以退避先于令牌);两者都不出环
  - t = 1.5 s:`rate_backoff = false`;Reduce 单放行出环(10 s 窗口剩余约 12 个令牌 ≥ 1);Open 仍 `reject = RATE`(10 s 窗口剩余 ≤ 16)
  - t = 13.0 s:Open 放行出环(两窗口剩余分别 > 16 与 > 192)
  - 门控拒绝不改变 OMS 与执行层的执行失败计数(INV-14);每步 INV-12
- 实现:`crates/hft-sequencer/tests/scenarios.rs::fp_2_rate_limit_drains_bucket_and_backs_off`
- 备注:策略侧 `Throttle` 的退避镜像由 hft-strat-lib 的属性测试覆盖;本例只断言引擎侧与视图 `rate_backoff`;FP-2.1–2.3 见 `fp_2_1_…`、`fp_2_2_…`、`fp_2_3_…`;实现方式:先以真实 sequencer 的确定性步骤序列在 hft-sequencer B0 夹具上实现,映射 A≙`VS`、B≙`VP`、a1≙`S1`、b1≙`P1`、账户 1/2≙`a`/`b`,阈值取 B0 参数,见该文件头;W1 种子化 DST 版本待测试策略能发出 S1 买入第一腿后补

| 行 | 相对主例的变化 | 断言 |
|---|---|---|
| FP-2.1 | t = 1.2 s(上次退避结束 0.2 s 后)第二次限速拒单 | k = 1,退避 2 000 ms(`backoff_until_ens = t + 2 000 ms`);再写一条 `Note{RateLimitHit}` |
| FP-2.2 | 另有一条链接:第一腿 `P1` 卖 0.01 在 t = −0.05 s 出环、t = 0.2 s 成交,第二腿为 `S1` 买 0.01(落在 `VS` 的退避期内) | t = 0.2 s 第二腿 `reject = RATE_BACKOFF`,OMS 经 `hedge_gated` 登记 `Timer{Oms, HedgeRetry}`,到期时刻 = max(0.2 s + 100 ms, 退避截止 1.0 s) = 1.0 s;`retries = 0`;1.0 s(`now ≥ backoff_until_ens`,不在退避期)的 `SecondRetry` 放行出环 |
| FP-2.3 | MockVenue 不回限速码;gw 侧发送计数兜底使该单回 `SendReport{Place, RateLimited}`(3) | 槽 → `Rejected`(`reject_code = 3`),`OrderUpdate::Rejected{bits = LOCAL_SEND_FAIL, venue_code = 3}`;无 `Note{RateLimitHit}`、令牌桶不清零;若该单是第二腿,按门控处理(`retries` 不变) |

### FP-3 成交先于 Ack 到达时槽直接进入 Filled,后到的 Ack 幂等忽略

- 规则:[回报处理表](../../crates/hft-oms/README.md#回报处理表)、[状态机](../../crates/hft-oms/README.md#状态机)、[OnFill](../../crates/hft-oms/README.md#onfill)、[延迟样本](../../crates/hft-oms/README.md#延迟样本)
- 级别:dst
- 里程碑:M2
- 门槛:④
- 前置:W1;`P1` BBO 固定为 60,010.0 / 60,010.1;`rr_max_cross = 0.002`;`conn_liveness_ms = 2000`;probe-taker 在 t = 0 发带 `LegLink` 的 Open:`S1` 买 0.01 IOC 限价 60,030.00,`HedgePlan{OnFill, P1 卖, ratio 1/1, on_fail = Retry, px_mode = Cross, px_param = 0.0005, max_slip = 0.0005}`;该链接 `ref_px = 60,010.0`
- 故障:MockVenue 按 60,000.00 全部成交,手续费 0.6 USDT;回报顺序为 t = 0.010 s `Fill{cum_filled = 0.01}`,t = 0.020 s `Ack`(乱序)
- 种子:标准集(§2.4)
- 断言:
  - `Fill` 那条 Input 内:s1 `PendingNew → Filled`,槽置 `FILL_BEFORE_ACK` 与 `ACKED`;`FillOut{qty = 0.01, net_qty = 0.01}`;ledger `S1` 库存 0.01、USDT −600.6;同一 Input 内 `OrderDecision{New, P1, Sell, qty = 0.01, px = 59,980.0, tif = Ioc, link_role = Second}`(限价 = max(60,010.0 × (1 − 0.0005), 60,010.05 × (1 − 0.002)) 向上取整到 0.1);合成一条 `LatencySample{action = New}`
  - `Ack` 那条 Input:没有 `Note{OmsTransition}`,`OmsStats` 的 dup 计数加 1;无 `FillOut`、无 `OrderDecision`;ledger 与 LegTracker 不变;不再合成 `LatencySample`
  - INV-2、INV-8、INV-19
- 实现:`crates/hft-testkit/src/scenarios/failpath.rs::fp_3_fill_before_ack_goes_filled`

| 行 | 相对主例的变化 | 断言 |
|---|---|---|
| FP-3.1 | 回报为 t = 0.010 s `PartialFill{cum_filled = 0.004}`、0.020 s `Ack`、0.030 s `Fill{cum_filled = 0.01}` | 第一条:`PendingNew → Live`(置 `FILL_BEFORE_ACK`),第二腿 `qty = 0.004`;`Ack`:dup;第三条:`Live → Filled`,第二腿 `qty = 0.006`;两条第二腿合计 0.01 |
| FP-3.2 | `Fill{cum_filled = 0.01}` 重复到达两次(同一 `cum_filled`) | 第二条只计 dup;第二腿只发一条(合计 0.01) |

### FP-4 撤单因已成交被拒时按回报成交量更新,不重试撤单

- 规则:[回报处理表](../../crates/hft-oms/README.md#回报处理表)、[状态机](../../crates/hft-oms/README.md#状态机)、[Unknown 收敛流程](../../crates/hft-oms/README.md#unknown-收敛流程)
- 级别:dst
- 里程碑:M2
- 门槛:④
- 前置:W1;`unknown_query_interval_ms = 1000`;probe-taker 在 t = 0 发 `S1` 买 0.01 PostOnly 限价 59,999.99,`group = GROUP_NONE`,`max_rest_ms = 0`;t = 0.005 s `Ack`(s1 `Live`,置 `ACKED`)
- 故障:
  - t = 0.990 s MockVenue 在真值中按 59,999.99 全部成交该单
  - t = 1.000 s 策略撤 s1(`OrderDecision{Cancel}`,s1 → `PendingCancel`,置 `CANCEL_SENT`)
  - 回报顺序:t = 1.005 s `CancelReject{detail = GONE}`;t = 1.100 s `Fill{cum_filled = 0.01, avg_px = 59,999.99}`(成交回报被延迟)
- 种子:标准集(§2.4)
- 断言:
  - `CancelReject` 那条 Input:s1 `PendingCancel → Unknown`(`filled = 0 < qty`),同一 Input 一条 `OrderDecision{Query, slot = s1}`;不产出第二条 `OrderDecision{Cancel}`;`CANCEL_SENT` 清除;InflightBook `imm_buy(a, S1)` 增加 0.01(Unknown 按即时在途计)
  - `Fill` 那条 Input:s1 `Unknown → Filled`;`FillOut{qty = 0.01}`;ledger `S1` 库存 +0.01(记 `GROUP_NONE`);给实例 1 的 `OrderUpdate{Filled(0.01)}`
  - 随后到达的 `QueryResult{Filled, cum_filled = 0.01}`:终态,只计 dup,无 `FillOut`
  - 场景结束时 s1 的 `OrderDecision{Cancel}` 恰好一条;INV-1、INV-2、INV-7
- 实现:`crates/hft-testkit/src/scenarios/failpath.rs::fp_4_cancel_reject_filled_no_retry`

| 行 | 相对主例的变化 | 断言 |
|---|---|---|
| FP-4.1 | 回报顺序对调:t = 1.005 s `Fill{cum_filled = 0.01}`,t = 1.100 s `CancelReject{detail = GONE}` | `Fill`:`PendingCancel → Filled`;`CancelReject`:终态 dup;无 `Query`;`OrderDecision{Cancel}` 恰好一条 |
| FP-4.2 | 交易所未成交该单,`CancelReject` 的 `detail` 为其他原因(非 `GONE`) | s1 `PendingCancel → Live`,`CANCEL_SENT` 清除;OMS 不自行再撤;直到策略再次调用 `cancel` 之前 `OrderDecision{Cancel}` 只有 1 条 |

### FP-5 第二腿被拒时按 HedgePlan 以 SecondRetry 重试,计次耗尽后链接 Broken

- 规则:[OnFill](../../crates/hft-oms/README.md#onfill)、[LegLink 与 HedgePlan](../../crates/hft-oms/README.md#leglink-与-hedgeplan)、[分类](../../crates/hft-risk/README.md#分类)、[断腿](../../crates/hft-risk/README.md#断腿broken自动有界平仓l3-与-flatten)、[PairOnFill](../../crates/hft-exec/README.md#paironfill)
- 级别:dst
- 里程碑:M2
- 门槛:④
- 前置:W1;`hedge_retry_max = 3`;`hedge_retry_min_ms = 100`;`hedge_deadline_ms = 3000`;`rr_max_cross = 0.002`;MockVenue 的拒单码表把 `51008`(保证金不足)登记为确定拒绝;probe-taker 在 t = 0 发 FP-3 同样的带链接 Open
- 故障:第一腿在 t = 0.010 s 全部成交(0.01 @ 60,000.00);此后 `P1` 上本链接的每一张卖单都被 MockVenue 以 `OrderEvent{Reject, reject_code = 51008}` 拒绝
- 种子:标准集(§2.4)
- 断言:
  - 第二腿 `role = Second` 1 张、`role = SecondRetry` 恰好 3 张,每张新槽、新 client id,`qty = 0.01`,`oflags` 含 `RISK_REDUCING`、不含 `REDUCE_ONLY`,风控分类 `Hedge`;相邻两张的出环间隔 ≥ 100 ms
  - 第 1、2、3 次拒绝后 `retries` 依次为 1、2、3 并各登记一个 `Timer{Oms, HedgeRetry}`;第 4 次拒绝(第 3 张 `SecondRetry`)的那条 Input 内:`retries = 4 > hedge_retry_max`,`LinkUpdate{state = Broken, broken_reason = RetryExhausted}`、`Note{OmsHedgeBroken}`(P0)、`Broken{a, g1}` 与 `HedgeFailure{a}` 的 `CapSet`,实例 1 收到 `RiskEvent::HedgeBroken{group = g1}`,执行层该 cell 为 `Broken`
  - 之后 `a` 的任何 Open 被拒(`CLOSE_ONLY`);LegTracker `g1` 的 `S1` 腿 net = +0.01、`P1` 腿 net = 0
  - INV-8、INV-14、INV-15、INV-19
- 实现:`crates/hft-sequencer/tests/scenarios_hedge.rs::fp_5_second_leg_rejected_retries_then_broken`
- 备注:`Broken` 之后的自动有界平仓见 BK-1;门控拒绝(报价、kill、限速)不计次见 FP-6、FP-2.2;FP-5.1 见 `fp_5_1_reverse_on_reject`,FP-5.3 见 `fp_5_3_hold_on_reject`;FP-5.2 待 W1 种子化 DST;实现方式同 B1(真实 sequencer 在 B0 夹具上的确定性步骤序列,映射与阈值见该文件头)

| 行 | 相对主例的变化 | 断言 |
|---|---|---|
| FP-5.1 | `HedgePlan.on_fail = Reverse` | 第一次拒绝后不发 `SecondRetry`,同一 Input 内 `HedgeRequest{role = Reverse}` → `OrderDecision{New, S1, Sell, qty = 0.01, tif = Ioc, link_role = Reverse}`,风控分类 `Hedge`、`RISK_REDUCING`,`VS` 不支持 reduceOnly 所以 `oflags` 不含 `REDUCE_ONLY`;发出反向单的同一调用内链接 `hedge_req = hedge_filled + hedge_open`(`owed = 0`);反向单成交后 `first_net = 0`、LegTracker `S1` 腿 net = 0,链接 `Done`;全程无 `SecondRetry`、无 `Broken` |
| FP-5.2 | `hedge_deadline_ms = 250`;MockVenue 回报时延固定 40 ms | 链接在 `now − first_fill_tsc > 250 ms` 之后的第一个判定点(失败处理或 `on_hedge_retry`)置 `Broken{Deadline}`,此时 `SecondRetry` 少于 3 张 |
| FP-5.3 | `HedgePlan.on_fail = Hold` | 第一次拒绝后无重试、无反向;链接 `Done`;LegTracker 保留 `S1` 腿 +0.01 的裸量,交执行层补腿规则 |

### FP-6 第二腿盘口长时间不变而连接健康时照常定价发出

- 规则:[对冲定价](../../crates/hft-oms/README.md#对冲定价)、[报价可用(连接健康)](../../crates/hft-exec/README.md#报价可用连接健康)、[连接活性](../../crates/hft-sequencer/README.md#连接活性)
- 级别:dst
- 里程碑:M2
- 门槛:④
- 前置:W1;`conn_liveness_ms = 2000`;`bbo_ping_ms = 1000`;`rr_max_cross = 0.002`;`hedge_retry_min_ms = 100`;`hedge_deadline_ms = 3000`;`P1` BBO 自 t = −10 s 起固定为 60,010.0 / 60,010.1,其连接每 1 000 ms 回 pong(`ConnState{Alive}`);probe-taker 在 t = 0 发 FP-3 同样的带链接 Open
- 故障:第一腿 t = 0.2 s 全部成交(0.01 @ 60,000.00)
- 种子:标准集(§2.4)
- 断言:
  - t = 0.2 s 那条 Input 内 `OrderDecision{New, P1, Sell, qty = 0.01, px = 59,980.0, link_role = Second}` 出环;`ApprovedIntent.hedge_px` 的 `capped = false`、`slipped = false`
  - 全程无 `Note{OmsHedgePxStale}`、无 `Timer{Oms, HedgeRetry}` 登记、`retries = 0`、无 `Broken`
- 实现:`crates/hft-sequencer/tests/scenarios_hedge.rs::fp_6_quiet_second_leg_book_still_priced`
- 备注:跨组件的完整期望(风险递减单、自动平仓、L3 减仓同样照常)见 QF-1;FP-6.1、FP-6.2 见 `fp_6_1_conn_down_gates_until_up`;实现方式同 B1(真实 sequencer 在 B0 夹具上的确定性步骤序列,映射与阈值见该文件头)

| 行 | 相对主例的变化 | 断言 |
|---|---|---|
| FP-6.1 | `P1` 的 BBO 连接 t = 0.1 s `ConnState{Down}`,t = 1.0 s `ConnState{Up}` 并到达一条 BBO(60,010.0 / 60,010.1) | t = 0.2 s:`hedge_gated(StaleQuote)`,`Note{OmsHedgePxStale}`(P1)恰好一条,`Timer{Oms, HedgeRetry}` 于 0.3 s;`retries = 0`;1.0 s 之后第一个到期的 HedgeRetry 发出 `SecondRetry`(`qty = 0.01, px = 59,980.0`);无 `Broken` |
| FP-6.2 | MockVenue 把第一腿的成交回报延迟到 t = 2.5 s;`P1` 的 BBO 连接自 t = 0.001 s 起不回任何帧,直到场景结束 | t = 2.5 s(`now − conn_live_tsc > 2 000 ms`):`StaleQuote` 门控,`retries` 始终为 0;在 `now − first_fill_tsc > 3 000 ms` 之后第一个 HedgeRetry 到期时(≤ 5.6 s)`Broken{Deadline}` + `Note{OmsHedgeBroken}`(P0) |

### FP-7 第二腿对手价超滑点时按越价上限定价发出,越过上限时门控等待

- 规则:[对冲定价](../../crates/hft-oms/README.md#对冲定价)、[检查链](../../crates/hft-risk/README.md#检查链按廉价--昂贵短路适用范围以本表为唯一定义)、[OnFill](../../crates/hft-oms/README.md#onfill)
- 级别:dst
- 里程碑:M2
- 门槛:④
- 前置:W1;`rr_max_cross = 0.002`;`max_slip = 0.0005`;`px_param = 0.0005`;`hedge_retry_min_ms = 100`;`hedge_deadline_ms = 3000`;probe-taker 在 t = 0 发 FP-3 同样的带链接 Open,链接 `ref_px = 60,010.0`
- 故障:t = 0.1 s `P1` BBO 变为 59,960.0 / 59,960.1(偏离 = 50 / 60,010.0 ≈ 0.000833 > `max_slip`);t = 0.2 s 第一腿全部成交
- 种子:标准集(§2.4)
- 断言:
  - t = 0.2 s 那条 Input 内 `OrderDecision{New, P1, Sell, qty = 0.01, tif = Ioc, px = 59,840.2, link_role = Second}`(上限 = 59,960.05 × 0.998 = 59,840.1299 向上取整到 0.1);`hedge_px` 的 `capped = true`、`slipped = true`;`Note{OmsHedgeSlipCapped}`(P1,`v = [link_id, 60,010.0, 59,960.0, 59,840.2]`)恰好一条;风控放行该单(无 `PX_BAND`)
  - 超滑点本身不计次:`retries = 0`;IOC 在 59,840.2 成交后链接 `Done`
- 实现:`crates/hft-sequencer/tests/scenarios_hedge.rs::fp_7_slippage_priced_at_cap_not_failed`
- 备注:跨组件期望见 LK-3;FP-7.2 在同一函数,FP-7.1 见 `fp_7_1_beyond_cross_gated_until_back`;实现方式同 B1(真实 sequencer 在 B0 夹具上的确定性步骤序列,映射与阈值见该文件头)

| 行 | 相对主例的变化 | 断言 |
|---|---|---|
| FP-7.1 | t = 0.1 s `P1` BBO 变为 59,700.0 / 60,100.0(中价 59,900.0,上限 59,780.2,买一已越过上限);t = 1.0 s 恢复为 59,960.0 / 59,960.1 | t = 0.2 s `hedge_gated(BeyondCross)`,`Note{OmsHedgePxGated}`(P1)一条,`retries = 0`,不出环;1.0 s 之后第一个 HedgeRetry 发出 `SecondRetry{px = 59,840.2}` 并写 `Note{OmsHedgeSlipCapped}`;无 `Broken` |
| FP-7.2 | 主例中 MockVenue 对 59,840.2 的 IOC 零成交到期 | `retries = 1`(执行结果类失败),`Timer{Oms, HedgeRetry}` 于 +100 ms |

### FP-8 本机时钟漂移超过 recvWindow 时交易所以时间窗拒单并标记连接 CLOCK_SKEW,时钟不可信只停新开仓

- 规则:[时钟](../../crates/hft-risk/README.md#时钟)、[检查链](../../crates/hft-risk/README.md#检查链按廉价--昂贵短路适用范围以本表为唯一定义)、[产生与消费的事件](../../crates/hft-venue/README.md#产生与消费的事件)、[结果未知码表](../../crates/venue-binance/README.md#结果未知码表)
- 级别:integ
- 里程碑:M3
- 门槛:④
- 前置:W1,`VS` 由进程内 `GwRuntime<P>`(`P` = venue-binance 的 `Spot` 协议)+ 真实 sequencer 承载,交易所侧回报帧由测试注入(与 C12 同一夹具,不走 socket);`recvWindow = 5000`;`max_pos_qty = 1 BTC`;交易所侧时钟比本机快 6 000 ms(测试对 `VS` 的每个下单请求注入 `-1021` 时间窗拒单回报);账户 `a` 的 `g1` 已有 `S1` +0.01、`P1` −0.01(已对冲)
- 步骤:
  1. 策略在 `S1` 发单腿 Open:买 0.01 IOC,限价 60,030.00
  2. 驱动写 Input `Timer{Sys, ClockBad}`
  3. 策略再发一笔 `S1` 单腿 Open;策略另发一笔 `S1` 卖 0.005 的 Reduce 意图(分类 `Reduce`、`RISK_REDUCING`)
  4. 驱动写 Input `Timer{Sys, ClockOk}`;策略再发一笔 `S1` 单腿 Open
- 期望:
  - 步骤 1:`VS` 回 `-1021`;gw 产出 `OrderEvent{Reject, reject_code = 0xFFFFFC03}` 与 `ConnState{Degraded, role = Order, reason = CLOCK_SKEW}`(9);槽 `PendingNew → Rejected`,`OrderUpdate::Rejected{bits = 0, venue_code = 0xFFFFFC03}`;没有槽进入 `Unknown`;`venue_ready(VS, a)` 保持 true;实例 1 收到 `EventRef::Conn`
  - 步骤 2:`ClockBad` cap(`cflags` 含 `CONDITIONAL`)的 `CapSet`;`ctl_state_version` 加 1
  - 步骤 3:Open 的 `OrderDecision.reject = CLOCK`,不出环;`Reduce` 单放行出环(`RISK_REDUCING`)
  - 步骤 4:`CapCleared{ClockBad}`;Open 不再因 `CLOCK` 被拒
- 实现:`apps/hft-core/tests/clock_skew.rs::fp_8_clock_skew_reject_and_clockbad_blocks_open`
- 备注:`Timer{ClockOk|ClockBad}` 由 housekeeping 按时钟状态写出,与交易所的时间窗拒单是两条独立的观测;`-1021` 的码表登记由 venue-binance 的金样覆盖

### FP-9 未登记的交易所错误码使槽进入 Unknown 并告警,不按 Reject 释放敞口

- 规则:[回报处理表](../../crates/hft-oms/README.md#回报处理表)、[Unknown 收敛流程](../../crates/hft-oms/README.md#unknown-收敛流程)、[实现方必须遵守的契约](../../crates/hft-venue-proto/README.md#实现方必须遵守的契约)、[OnFill](../../crates/hft-oms/README.md#onfill)
- 级别:dst
- 里程碑:M2
- 门槛:④
- 前置:W1;`unknown_query_interval_ms = 1000`;`hedge_retry_min_ms = 100`;`hedge_deadline_ms = 3000`;MockVenue 的拒单码表不含 `59999`;probe-taker 在 t = 0 发 FP-3 同样的带链接 Open;第一腿 t = 0.010 s 全部成交
- 故障:第二腿(`P1` 卖 0.01)出环后,MockVenue 在真值中按 60,010.0 全部成交,但回给本系统的是未登记码 `59999`,按契约 8 产出 `OrderEvent{OutcomeUnknown, reject_code = 59999}`;t = 0.5 s 对该单的 `Query` 回 `QueryResult{Filled, cum_filled = 0.01, avg_px = 60,010.0}`
- 种子:标准集(§2.4)
- 断言:
  - `OutcomeUnknown` 那条 Input:第二腿槽 `PendingNew → Unknown`(cause = `OutcomeUnknown`(14)),`Note{OmsUnknown}`(P1),同一 Input 一条 `OrderDecision{Query}`;没有 `Rejected` 迁移、没有给拥有者的 `Rejected`;InflightBook `imm_sell(a, P1) = 0.01` 不变;LegTracker `P1` 腿 `inflight_immediate = −0.01`,`naked_dir = 0`
  - `Unknown` 期间:没有 `SecondRetry`、没有 `Reverse`;`venue_ready(VP, a) = false`,`a` 在 `VP` 上的 Open 被拒(`VENUE_NOT_READY`)
  - `QueryResult` 那条 Input:`Unknown → Filled`;`FillOut{qty = 0.01}`;链接 `Done`;`Note{OmsVenueReady}` 一条
  - INV-1、INV-4、INV-19
- 实现:`crates/hft-testkit/src/scenarios/failpath.rs::fp_9_unregistered_code_goes_unknown`
- 备注:各 venue 对未登记码产出 `OutcomeUnknown` 的映射由 hft-venue-proto 契约 8 的检查与各 `venue-*` 金样覆盖

| 行 | 相对主例的变化 | 断言 |
|---|---|---|
| FP-9.1 | 真值中第二腿未执行;t = 0.5 s 的 `Query` 回 `QueryResult{Rejected, cum_filled = 0}` | `Unknown → Rejected`;此时才按 OnFill 第 6 条计一次执行失败(`retries = 1`)并登记 HedgeRetry,其后 `SecondRetry{qty = 0.01}` 出环 |
| FP-9.2 | 单腿 Open(`S1` 买 0.01,无链接)回未登记码;真值中未执行 | 收敛前 InflightBook `imm_buy(a, S1) = 0.01` 保持;`QueryResult{Rejected}` 之后才减为 0 |

### FP-10 无过期保护的所查单 NotFound 不定终态,直到成交历史确认

- 规则:[Unknown 收敛流程](../../crates/hft-oms/README.md#unknown-收敛流程)、[实现方必须遵守的契约](../../crates/hft-venue-proto/README.md#实现方必须遵守的契约)、[Unknown 订单收敛](../../apps/reconciler/README.md#unknown-订单收敛)、[SendReport 处理](../../crates/hft-oms/README.md#sendreport-处理)
- 级别:dst
- 里程碑:M2
- 门槛:④
- 前置:W1,`VS` 的 `stale_guard = None`;`not_found_grace_ms[VS] = 2000`;`unknown_query_interval_ms = 1000`;`unknown_max_queries = 5`;`unknown_alarm_ms = 30000`;probe-taker 在 t = 0 发单腿 Open:`S1` 买 0.01 IOC(无链接,`group = GROUP_NONE`)
- 故障:
  - 该单 `SendReport{Place, WriteError}`(`sent_tsc` 记为 t = 0.001 s);MockVenue 真值中该单不存在
  - MockVenue 对按 client id 的 `Query` 一律回 `QueryResult{status = NotFound}`
  - `ReconActor` 收到 `Note{OmsNeedsRestQuery, trades = 1}` 后:第一次请求回 `UnknownResolved{history_checked = 0}`,t = 10.0 s 回 `UnknownResolved{history_checked = 1, exch_qty = 0}`(epoch、slot、gen 与 s1 完全匹配)
- 种子:标准集(§2.4)
- 断言:
  - `WriteError` 那条 Input:s1 → `Unknown`(cause = `WriteError`(18))并发 `Query`
  - `now − sent_tsc ≤ 2 000 ms` 内到达的 NotFound:s1 保持 `Unknown`,不置 `HIST_WAIT`,无 `trades = 1` 的 `Note{OmsNeedsRestQuery}`
  - 第一条 `now − sent_tsc > 2 000 ms` 的 NotFound 那条 Input:置 `HIST_WAIT`,写 `Note{OmsNeedsRestQuery, trades = 1}`;此后每次 `Timer{Oms, UnknownQuery}` 到期改为重写该 Note、不发 `Query`;`query_count` 不再增加
  - `history_checked = 0` 的条目只记录,s1 仍 `Unknown`
  - t = 10.0 s 的条目:s1 `Unknown → Rejected`(`reject_code = 0`,cause = `Reconcile`(13)),`OrderUpdate::Rejected{bits = 0, venue_code = 0}`;InflightBook 此时才释放;t < 10.0 s 的任何时刻 s1 都不是终态
- 实现:`crates/hft-testkit/src/scenarios/failpath.rs::fp_10_notfound_waits_trade_history`

| 行 | 相对主例的变化 | 断言 |
|---|---|---|
| FP-10.1 | `VS` 的 `stale_guard = RecvWindow`,`recvWindow = 5000`,`not_found_grace_ms[VS] = 5500`;真值中该单在 t = 4.0 s 才撮合,部分成交 0.004 后到期 | t = 6.0 s 的 NotFound 之后置 `HIST_WAIT`,s1 不定终态;`history_checked = 1, exch_qty = 0.004, order_status = Expired` 的条目到达时先应用增量 0.004(价格按委托价,`FillOut.flags` 含 `PX_EST`,另写 `Note{OmsFillPxEstimated}`)再迁 `Cancelled` |
| FP-10.2 | `VS` 的 `stale_guard = ExpireAt`,`not_found_grace_ms[VS] = 2000`(非链接槽) | 第一条 `now − sent_tsc > 2 000 ms` 的 NotFound 那条 Input:s1 → `Rejected`(cause = `NotFoundExpire`(22)),不请求成交历史 |
| FP-10.3 | 同 FP-10.2,但 s1 是带 `LegLink` 的第一腿 | 按主例的成交历史确认收敛,不出现 `NotFoundExpire` |
| FP-10.4 | 对象换成第二腿:FP-3 同样的带链接 Open,第一腿 t = 0.01 s 全部成交;第二腿(`P1` 卖 0.01)`SendReport{WriteError}` 进 `Unknown`,其 `Query` 一律 NotFound,真值中该单不存在,成交历史确认 t = 10.0 s 才到达;`hedge_deadline_ms = 3000` | `Unknown` 期间无 `SecondRetry`;`now − first_fill_tsc > 3 000 ms` 之后第一个 sweep 或该槽 `Timer{Oms, UnknownQuery}` 判定点上链接 `Broken{Deadline}` + `Note{OmsHedgeBroken}`(P0);t = 10.0 s 第二腿 → `Rejected`,`P1` 腿 `inflight_immediate` 回到 0,`S1` 腿 +0.01 的裸量交 Broken 处置(BK-1) |

### FP-11 自身强平或 ADL 的外来成交入账,另一腿交执行层默认处理

- 规则:[外部成交](../../crates/hft-ledger/README.md#外部成交externalfill)、[外来成交(B3)](../../crates/hft-oms/README.md#外来成交b3)、[外来成交的默认处理(B3)](../../crates/hft-exec/README.md#外来成交的默认处理b3)、[每条 Input 的处理顺序](../../crates/hft-sequencer/README.md#每条-input-的处理顺序process)
- 级别:dst
- 里程碑:M4
- 门槛:④
- 前置:W1;账户 `a` 的 `g1`:`S1` 腿 net +0.01、`P1` 腿 net −0.01,无在途、无目标;probe-taker 参数设为对 `EventRef::External` 不动作;`external_deadline_ms = 5000`;`dust_notional = 10 USDT`;`fix_tolerance_default = 0.05`;`max_hedge_notional = 10 000 USDT`
- 故障:t = 1.0 s MockVenue 在 `P1` 上强平本账户空头 0.004(买 0.004 @ 60,500.0),产出 `ExternalFill{cause = Liquidation, inst = P1, side = Buy, qty = 0.004, trade_id = 9001}`
- 种子:标准集(§2.4)
- 断言:
  - t = 1.0 s 那条 Input:hft-ledger 入账(`P1` 仓位 −0.006),`Note{ExternalFill}`(source = Ledger,P0,`cause = Liquidation`);`ExternalApplied{group = g1, leg = P1, net_qty = +0.004, none_qty = 0}`;LegTracker `P1` 腿 net −0.006;执行层返回截止 = 1.0 s + 5 000 ms 并登记 `Timer{Exec, External}`;实例 1 收到 `EventRef::External`,`deadline_tsc` 对应 6.0 s
  - t ∈ (1.0 s, 6.0 s):无执行层订单
  - t = 6.0 s:`ExecDecision{ExternalDefault}`、`Note{ExecExternalDefault}`(P0);`OrderDecision{New, S1, Sell, qty = 0.004, tif = Ioc, link_role = HedgeFix}`,`RISK_REDUCING`,`hedge_cap_qty = 0.00401`;成交后 `g1` 已成交裸量为 0
  - INV-1、INV-15、INV-16、INV-19
- 实现:`crates/hft-testkit/src/scenarios/failpath.rs::fp_11_liquidation_booked_then_default_handling`
- 备注:跨组件的完整期望(`Manual` 的人工保持、P0 告警路由)见 B3 与 MD-18

| 行 | 相对主例的变化 | 断言 |
|---|---|---|
| FP-11.1 | `cause = Adl` | 同主例,`Note{ExternalFill}` 的 cause = `Adl` |
| FP-11.2 | 同一 `ExternalFill`(同 `trade_id = 9001`)在 t = 1.5 s 重复到达 | 第二条 `apply_external` 返回 `None`:ledger、LegTracker、执行层计时都不变,策略不再收到 `EventRef::External` |
| FP-11.3 | t = 3.0 s 策略对 `g1` 下单(`Ctx::place` 一笔 `S1` 卖 0.004) | 外来成交截止清除,写 `ExecDecision{ExternalSeen, detail = External/0/Cleared}`;t = 6.0 s 无 `ExternalDefault` |

### FP-12 基础币扣费时对冲与平仓按净量,现货卖出不超过可用余额

- 规则:[成交增量与净量](../../crates/hft-ledger/README.md#成交增量与净量)、[成交净量(B4)](../../crates/hft-oms/README.md#成交净量b4)、[OnFill](../../crates/hft-oms/README.md#onfill)、[TakerSlice](../../crates/hft-exec/README.md#takerslice)
- 级别:dst
- 里程碑:M2
- 门槛:④
- 前置:W1,`VS` 的 `fee_asset_rule = ReceivedAsset`;probe-taker 在 t = 0 发 FP-3 同样的带链接 Open;group `g1` 的余数为 0
- 故障:第一腿全部成交 0.01 @ 60,000.00,回报带 `fee = 0.00001 BTC`、`fee_asset = BTC`
- 种子:标准集(§2.4)
- 断言:
  - 成交那条 Input:`FillOut{qty = 0.01, net_qty = 0.00999}`;ledger `S1` 库存 0.00999;LegTracker `S1` 腿 net +0.00999;第二腿 `qty = 0.009`(0.00999 按 `P1` lot 0.001 向零取整),`residual[g1][P1] = 0.00099`;第二腿的 `basis = 0.00999`、`hedge_cap_qty = 0.01099`
  - 第二条同样的链接(第一腿 0.01、扣 0.00001 BTC):第二腿 `qty = 0.010`(0.00999 + 0.00099 = 0.01098 向零取整),余数 0.00098
  - 第二腿与链接的对冲基数不出现毛量 0.01;INV-8
- 实现:`crates/hft-testkit/src/scenarios/failpath.rs::fp_12_base_fee_hedges_net_qty`
- 备注:跨组件期望见 B4

| 行 | 相对主例的变化 | 断言 |
|---|---|---|
| FP-12.1 | 成交回报不带费用(`fee_asset = ASSET_NONE`),t = 0.5 s 到达 `FeeOnly{fee = 0.00001 BTC}` | 成交时 `net_qty = 0.01`,第二腿 `qty = 0.010`;`FeeOnly` 那条 Input:槽状态不变,ledger `apply_fee` 返回 `NetAdjust{delta = −0.00001}`,LegTracker `S1` 腿 net 0.00999,`residual[g1][P1] = −0.00001` |
| FP-12.2 | 本行实例 1 为 probe-target;主例的第一条链接之后,`VS` 的 `BalanceReport` 给出 BTC 可用 0.00995(冻结 0.00004);probe-target 对 `g1` 设退出目标 0/0 | `S1` 卖出切片 `qty = 0.00995`(= min(group 库存 0.00999, 可用余额 0.00995));任何时刻都没有数量超过当时可用余额的 `S1` 卖单 |
