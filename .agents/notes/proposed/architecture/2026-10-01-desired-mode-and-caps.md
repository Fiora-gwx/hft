# Agent Note: 期望模式与 cap 分离——唯一的模式模型

Status: proposed

> **第八轮(2026-10-02)**:补充——新增 cap 来源 `MarginStress`、`HedgeFailure`、`Storage`、`Delisting` 与启动闸门位 `IDENTITY`、`ACCOUNT_CFG`(清单归 hft-risk);HARD 可按账户;模拟是账户类型而不是 `RunMode`;二次确认只有 PREVIEW + `expect_state_version` 一种。见 [margin-stress](2026-10-02-margin-stress.md)、[one-account-per-strategy-instance](2026-10-02-one-account-per-strategy-instance.md)、[sim-accounts-not-backtest](2026-10-02-sim-accounts-not-backtest.md)。期望模式与 cap 分离的模型不变。

> **第八轮补充(2026-10-03)**:补充——资金费差异不设 `Recon` cap;Viewer 可设非 HARD 的 Kill cap(不能解除);见 [funding-recon-alert-only](2026-10-03-funding-recon-alert-only.md)、[viewer-emergency-stop](2026-10-03-viewer-emergency-stop.md)。

## Problem

此前的 ModeTable 一格同时存两件事:操作者要的模式,以及系统因 kill、日亏、对账偏差、恢复、时钟等原因压低到的模式。多模式多账户专项评审(29 条,P0 5 条)指出由此产生的连锁问题:

- 自动降级覆写原值,恢复时不知道该回到哪里;
- 两个降级来源叠在同一格,解除其中一个就把另一个也解除了;`SetMode` 一写就抹掉未解除的降级;
- `Shadow` 也写在同一格,有仓位时切 Shadow 会冻结出场([defensive-patterns](../../../../docs/defensive-patterns.md#shadow-不能冻结出场));
- `Armed` 与 `Live` 两档的差别只是"是否遵守名单",却各占一种模式,增加了组合数。

## Proposal

裁决见 第五轮裁决(2026-10-01 模式/账户统一模型,主控记录) M1、M2、M6 与第六轮裁决 A4;层级表的唯一定义在 [control-plane](../../../../docs/subsystems/control-plane.md)(标注"由代码生成":生成器就绪前手写、CI 比对)。**唯一归属模块是 `hft-risk::mode`**([`hft-risk`](../../../../crates/hft-risk/README.md)):ModeTable、cap 集合、有效模式、启动闸门、`ctl_state_version`、kill 投影都在这里;sequencer 只调用 `apply()` 并分发 Mode 事件与撤单;`RunMode` 的枚举与编码下沉 `hft-types`。

1. **ModeTable 只存期望模式**,只由 `SetMode` 写。`RunMode` = `Shadow` / `CloseOnly` / `Live`;`Armed` 并入 `Live`,`Live` 永远遵守 `open_allowed` 名单。
2. kill、`DRAINING`、时钟不可信、锁存 CloseOnly(DailyLoss、Recon、UnknownTimeout、Broken/Reorg、Recovery、halted)、条件门都是**独立的 cap 记录** `(source, scope, set_seq, params)`,互不覆盖。
3. **有效模式** = 期望为 `Shadow` 时取 `Shadow`;否则取 min(期望, 全部生效 cap)。`RISK_REDUCING` 单只受 `HARD` 约束([kill-risk-reducing-passthrough](2026-10-01-kill-risk-reducing-passthrough.md))。
4. `SetMode` 不清任何 cap;`Resume{source, scope}` 只删 (source, scope) 完全匹配的一条;kill 字由剩余 cap 重算。
5. `Shadow` 只在 cell 无仓位、无在途、无 LegTracker 残余时生效;否则 `ShadowDeferred`(有效 `CloseOnly`,平仓照常),归零后自动落到 `Shadow`。
6. 有效模式每次变化向策略投递 Mode 事件;降到 `CloseOnly`/`Shadow` 时撤该 cell 的 Open 类挂单。快照含 cap 列表与 `Gates` 节。

7. **ctl-gateway 不复制 sequencer 状态**(round6 A4):`SetMode` 按**目标值**分类(→ `Live` 为升级;→ `CloseOnly`/`Shadow` 为降级);放宽判定、确认令牌与 `ctl_state_version` 校验都在 sequencer 内由 `hft-risk::mode` 完成——`CmdHdr.confirmed` 由 sequencer 复核,返回 `NeedsConfirm`/`StateChanged`;Prepare = 带 PREVIEW 标志的 `ControlCmd`,sequencer 回 `CmdResult{preview}`。gateway 只做传输、鉴权与角色上限。
8. cap 来源:Kill(+HARD)、`DRAINING`、Recovery、DailyLoss、Broken、UnknownTimeout、ClockBad;Reorg/Halted 为可选扩展(启用相应链上终局规则或 halted 策略控制时加入)。干净 Drain 重启在闸门通过后自动解除 Recovery(`auto_resume_after_clean_drain`,默认开),崩溃恢复仍需人工。

## Alternatives considered

**一格存模式 + 一个"降级前的模式"备份字段。** 只能记住一层;两个来源叠加时仍然无法精确撤销。

**按优先级把降级写成模式枚举的更多取值(如 `CloseOnlyDailyLoss`)。** 组合爆炸,且仍然是"一格一个值",叠加问题不变。

**gateway 维护一份模式与 cap 的副本来做分类与确认。** 两份状态必然漂移(gateway 看不到 sequencer 内的自动降级与 cap 变化的精确时刻),确认令牌会基于过期视图放行;把判定放在唯一写者里,gateway 只需要传输与鉴权。

**保留 `Armed`。** 它与 `Live` 的差别可以由名单与参数覆盖表达;多一种模式只多一组迁移规则与测试。

## Acceptance criteria

- testing 不变量 11–13 与 [§4.1](../../../../docs/testing.md#41-模式账户与人工干预) 的模式用例全部通过(多 cap 叠加、`ShadowDeferred`、日亏 Resume、时钟不可信)。
- 快照恢复后的 cap 集合与有效模式与连续运行相同,验证回放逐字节一致。
- control-plane、hft-risk、hft-strategies、ops_bot 的规格只链接层级表,不另写一份。

## Risks

- 有效模式的计算从"读一格"变成"取 min",每次 cap 变化要重算受影响的 cell:cap 变化是低频事件(命令与自动降级),热路径只读缓存的有效模式。
- 操作者心智模型改变("我 `SetMode Live` 了为什么还是 CloseOnly"):确认预览与 `/status` 必须同时显示期望模式与生效的 cap 列表。
