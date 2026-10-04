# Agent Note: 恢复的代数与开机规则:epoch、MirrorRestore、boot_id、日切、版本变化

Status: proposed

> **第八轮(2026-10-02)**:收窄第 7 条——出现 `tail_lost` 或 RecoverMismatch 时禁用 `auto_resume_after_clean_drain`;`RECONCILED` 要求本轮全部 `Ignore` 或每项差异已采纳/确认;同一 seq 连续崩溃 `poison_crash_limit` 次按毒丸隔离并以 `tail_lost` 处理。规则归 [hft-risk](../../../../crates/hft-risk/README.md) 与 [hft-sequencer](../../../../crates/hft-sequencer/README.md),证据见 [评审 P0-5](../../../../design/_work/review-2026-10-02-主策略适配与执行正确性.md)。

> **第八轮补充(2026-10-03)**:收窄——`RECONCILED` 不含资金费条目;差异只经 `AdoptExchange`(仓位、余额)或 `SetBalanceBaseline`(非策略资产)处置,不存在单独的确认命令;冷启动时对冲截止只计引擎运行时间,闸门放行时未对冲的链接先 `SecondRetry`;见 [funding-recon-alert-only](2026-10-03-funding-recon-alert-only.md)、[broken-auto-fix](2026-10-03-broken-auto-fix.md)。

> **第八轮第三次修订(2026-10-03)**:补充——策略、strat-lib、ledger、risk 的持久状态改存跨重启连续、停机不计的 `Now.engine_ns`,TSC 只在单次启动内使用;OMS/exec 的 `recovery_end` 平移保留;毒丸分两层:hft-core `boot_guard` 管恢复期崩溃、sequencer 按输入哈希管运行期崩溃,计数都在 `DurableMeta`;见 [engine-time](2026-10-03-engine-time.md)。

## Problem

全局评审(场景评审 35 条)沿"崩溃 → 重启 → 恢复 → 继续交易"走了一遍,发现 [recover-driver](2026-10-01-recover-driver.md) 之外还有一组跨开机的缺陷,都会在重启后造成错误交易:

- 订单槽的代数只在内存里递增,重启后与交易所上仍存在的旧单撞号,旧回报被应用到新订单(B-3);
- 恢复后 gw 的私有映射(交易所字符串 id → slot)是空的,非终态订单没人查询(B-2);
- 快照里的定时器、退避、令牌桶以 TSC 表示,换 `boot_id` 后原点不同(B-8);
- 日切只在跨零点那一刻触发,恰好停机的那一天永远不切(B-17);
- 新版本二进制从旧 journal 的任意尾部起步,行为与生成该尾部的版本不同(B-3);
- journal 写满与引导块写到一半崩溃都没有确定的处理(B-18、B-22)。

## Proposal

第六轮裁决(2026-10-01 全局评审,主控记录)B-2、B-3、B-8、B-17、B-18、B-22,A11;实现归 [`hft-sequencer`](../../../../crates/hft-sequencer/README.md)、[`hft-oms`](../../../../crates/hft-oms/README.md)、[`hft-journal`](../../../../crates/hft-journal/README.md):

1. **epoch**:`engine_epoch` 在发出第一笔订单前持久化到冷副本并 fsync;启动取 max(快照, 持久值) + 1;`OrderEvent` 携带 epoch,OMS 按 (epoch, slot, gen) 匹配。
2. **MirrorRestore**:恢复并 `SetEpoch` 之后,sequencer 为每个非终态槽向 gw 下发 `MirrorRestore{slot, gen, epoch, inst, exch_order_id}`;无法编码查询的槽保持 `Unknown` 并转交 reconciler。
3. **boot_id**:快照中的 TSC 量与 `boot_id` 同存;不同 → 定时器按 UTC 剩余时长重建,退避与令牌桶清零,`t_state` 取恢复时刻,周期性系统定时器重新登记。
4. **日切**:快照存 `last_day_rolled`;时钟可信且 UTC 日 > 该值时(含启动后第一轮)写 `Timer{DayRoll{day}}`,更小或相等的忽略。
5. **版本变化**(含换架构,[build-target-aarch64](../process/2026-10-01-build-target-aarch64.md)):只在 journal 末尾是 Drain 快照时允许冷启动;否则先用旧二进制恢复并 Drain。
6. **journal 边界**:超过安全阈值在下一个事件边界滚动,预留 2 份引导块空间;`AppendErr::Full` → `Fatal(JournalFull)` 并 abort,交给死手与 kill-switch;引导块写到一半崩溃 → 该段按 Preparing 整段删除;事件内产生的 Meta 不置 `EVENT_END`。
7. **干净 Drain**:同版本、尾部为空、无 `Unknown`/Broken 的 Drain 重启在闸门通过后自动解除 `Recovery`(`auto_resume_after_clean_drain`,默认开);崩溃恢复仍需人工 `Resume{Recovery}`。

## Alternatives considered

**代数只存在快照里。** 快照之后、崩溃之前发出的订单代数会在重启后被重用;必须在第一笔订单前持久化。

**重启后让 reconciler 全量收敛,不查单。** 对账周期 30–60 s,期间非终态订单的成交无人处理;`MirrorRestore` 让 gw 立即查询,reconciler 只兜底无法编码查询的那部分。

**TSC 换算成 UTC 存进快照、恢复时换回。** 等价于本方案的第 3 条,但把换算散在每个定时器上;统一以 `boot_id` 判定更容易测试。

**版本变化时照常从任意尾部恢复,不一致只告警。** RecoverDriver 的"只告警"前提是同版本;新版本在旧尾部上重新生成的输出可能发出旧版本没发过的订单。

## Acceptance criteria

- [testing §4.2](../../../../docs/testing.md#42-恢复oms-与对账场景) 的 B-2、B-3、B-8、B-17、B-18、B-22 用例全部通过。
- 混沌:`kill -9` 后在另一台主机(不同 `boot_id`)上以 `config_source = journal` 冷启动,定时器与令牌桶行为正确,交易所侧无重复 client id。
- 发布演练:新版本在非 Drain 尾部上启动被拒,在 Drain 快照上启动成功。

## Risks

- epoch 持久化在第一笔订单前多一次 fsync(毫秒级,只在启动时):计入冷启动 < 30 s 预算。
- `Fatal(JournalFull)` 让进程退出:由死手与 kill-switch 保证停得安全;安全阈值与预留空间按 hft-journal 的段大小推导,监控余量。
- 自动解除 Recovery 的"干净"判定若有漏洞会过早放开:条件只包含可机器验证的事实,崩溃路径永远人工。
