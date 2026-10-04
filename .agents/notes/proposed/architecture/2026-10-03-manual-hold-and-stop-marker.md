# Agent Note: 人工外来成交后的 manual_hold;stop_core 的持久停机标记与额外确认

Status: proposed

> **第八轮第六次修订(2026-10-03)**:收窄第 1–3 条——停机标记格式以 [kill-switch](../../../../apps/kill-switch/README.md) 为准(含 `"v":1`);`stop_core` 停机失败(`core_stop ∉ {Stopped, Inactive}`)时 kill-switch 把标记改名为 `stop-marker.json.aborted` 并 P0,hft-core 启动只读 `stop-marker.json`,作废后是否另发 Kill 由人决定([incident-response R](../../../../docs/runbooks/incident-response.md#r-老板紧急停止stop之后));"停引擎会中断风险处置"另含清仓锁未完成、外来成交计时中与自动平仓执行中,待补 Kill 补发前先查该范围是否已被人工 Kill 又 Resume([ops_bot 紧急停止](../../../../python/ops_bot/README.md#紧急停止stop));`manual_hold` 的解除与延迟置位细则归 [hft-exec](../../../../crates/hft-exec/README.md)。依据 [round8f 简报](../../../../design/_work/round8f-spec-brief.md) F1(g)、X1 P2-1、2、12、13。

## Problem

两处人工决定会被系统自动撤销:人工在交易所网页成交(`ExternalFill{Manual}`)后执行层不做默认处理,但自动补腿随即平掉另一腿;老板 `/stop` 经 kill-switch `stop_core` 停掉 hft-core 后,下一次干净重启会自动解除 `Recovery`、恢复开仓,唯一的接续是 ops_bot 的待补 Kill,它与闸门放行竞态,ops_bot 不在时停止被静默撤销。另外 `stop_core` 会同时停掉 L3 减仓与在途对冲;`Kill{flatten}` 在启动闸门未放行与 Drain 期间被拒,而那正是需要它的时候([W1 评审](../../../../design/_work/review-2026-10-03-M0d-W1资金正确性.md) P1-1、P1-6、§5-1/2/6)。

## Proposal

用户决定按推荐执行([round8-decisions](../../../../design/_work/round8-decisions.md) R8-11;规范见 [round8e 简报](../../../../design/_work/round8e-spec-brief.md) E7)。`manual_hold` 归 [hft-exec](../../../../crates/hft-exec/README.md),停机标记的写入归 [kill-switch](../../../../apps/kill-switch/README.md)、读取归 [hft-core 启动顺序](../../../../apps/hft-core/README.md#启动顺序),ops_bot 侧归 [ops_bot 紧急停止](../../../../python/ops_bot/README.md#紧急停止stop),目录权限归 [deploy](../../../../deploy/README.md#身份路径与-s3-前缀),处置归 [incident-response R](../../../../docs/runbooks/incident-response.md#r-老板紧急停止stop之后)。

1. **`manual_hold`**(cell 标志):收到 `ExternalFill{Manual}` 置位(`Note{ExecManualHold}`,P1);期间抑制补腿、残余修剪、外来成交默认处理与自动有界平仓的计时;只由策略对该 group 的非清除类动作、人工 `HedgeFix` 或引擎目标(L3/flatten/Hard)解除。
2. **停机标记**:kill-switch 在 `systemctl stop` 之前写 `/var/lib/hft/kill-switch/stop-marker.json`(`{req_id, scope, utc_ns}`,fsync);hft-core 启动步 5 读到 → 写一条 Input `ControlCmd{Kill{scope, 非 HARD, 不带 flatten}}`(issuer `cli:systemd`)→ 标记改名 `.consumed`;解除只经交易员 `/resume kill`。
3. **额外确认**:任一账户 MarginStress ≥ L2、有 Broken 或在途链接时,ops_bot 的兜底只撤单并 P0;停引擎要再确认一次 `/stop … core`。
4. **`Kill{flatten}`** 在启动闸门未放行与 Drain 期间也允许(仍需确认),是升级类的例外。

## Alternatives considered

**`manual_hold` 设超时自动解除。** 人工操作的意图无法从时间推断,超时后补腿照样会撤销人的决定;用户选择不超时。

**hft-core 启动时只置不可自动解除的 `Recovery`。** 与 Kill cap 等价但多一条规则;Kill cap 已有完整的解除路径与审计。

**只靠 ops_bot 的待补 Kill。** 它依赖 ops_bot 存活且先于闸门放行,保留为第二道保证。

**`stop_core` 总是停引擎。** 停引擎会中断正在进行的 L3 减仓与对冲,在保证金危急时可能更糟;让人看过风险状态后再决定。

## Acceptance criteria

- DST:人工外来成交后,无策略动作时不出现任何补腿或自动平仓单;策略 `set_target` 或人工 `HedgeFix` 后恢复。
- 演练:`stop_core` 后重启 prod,范围内账户带 Kill cap 起步、标记改名 `.consumed`,`/resume kill` 前不开仓;有在途链接时兜底只撤单、hft-core 不停,`/stop … core` 确认后才停。
- `Kill{flatten}` 在闸门未放行时经确认执行。

## Risks

- `manual_hold` 期间人或策略都不动作时,裸腿会一直保持;P1 告警与日报列出处于 `manual_hold` 的 cell。
- 停机标记目录由 kill-switch 与 hft-core 两个用户共享(`hft-stopmark` 组);目录权限出错时标记写不出,只剩待补 Kill 一道保证,预检核对目录属组与模式。
