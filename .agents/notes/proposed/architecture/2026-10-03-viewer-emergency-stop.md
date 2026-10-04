# Agent Note: 老板(Viewer)唯一的写权限:非 HARD、不平仓的紧急停止

Status: proposed

> **第八轮第三次修订(2026-10-03)**:收窄第 2、3 条——`/stop` = 非 HARD、不带 flatten 的 `Kill{Global | Account}`:禁止新开仓、撤销挂单,已成交第一腿的对冲与平仓照常;Viewer Kill 的确认绑定预览返回的 `cmd_hash` + 60 s TTL,**不比对** `ctl_state_version`(其他命令仍比对);hft-core 心跳过期或 ctl-gateway 不可达时 ops_bot 改为触发 kill-switch 的本地撤单入口(只接受 ops_bot uid)并 P0 告警;`/stop` 失败或 10 s 无结果 → P0;kill-switch 仍只撤单、不平仓。依据 [round8-decisions](../../../../design/_work/round8-decisions.md) R8-9 第 5 条与[评审](../../../../design/_work/review-2026-10-03-M0b独立评审.md) P1-8;规格见 [ops_bot 紧急停止](../../../../python/ops_bot/README.md#紧急停止stop)、[incident-response R](../../../../docs/runbooks/incident-response.md#r-老板紧急停止stop之后)。

> **第八轮第四次修订(2026-10-03)**:收窄第三次修订的兜底——ctl-gateway 持续 10 s 不可达(hft-core 心跳新鲜或过期都一样)时,ops_bot 经 kill-switch 本地触发套接字发 `stop_core`:kill-switch 撤单后经 polkit 只执行 `systemctl stop hft-core@prod`(SIGTERM → Drain,不平仓);ops_bot 把这次停止记为待补 Kill(`/var/lib/hft/ops_bot/pending_kill.json`),hft-core 恢复、ctl 可达后先自动补发 Kill,再允许交易员 `/resume`;交易员与 Admin 的 `/stop` 就是默认参数的 Kill,立即发出、不需确认。依据 [round8-decisions](../../../../design/_work/round8-decisions.md) R8-10 与 [round8d 简报](../../../../design/_work/round8d-spec-brief.md) D9-e;规格见 [ops_bot 紧急停止](../../../../python/ops_bot/README.md#紧急停止stop)、[incident-response R](../../../../docs/runbooks/incident-response.md#r-老板紧急停止stop之后)。

> **第八轮第五次修订(2026-10-03)**:收窄第四次修订的兜底——ctl-gateway 不可达时,任一账户 MarginStress ≥ L2、有 Broken 或在途链接则 ops_bot 只经 kill-switch 撤单并 P0,停引擎要再确认一次 `/stop … core`;`stop_core` 前 kill-switch 写持久停机标记,hft-core 重启时带 Kill cap、必须人工 `/resume kill`;Viewer 确认绑定同一命令的预览(sequencer 重算 `cmd_hash`)。见 [manual-hold-and-stop-marker](2026-10-03-manual-hold-and-stop-marker.md);依据 [round8e 简报](../../../../design/_work/round8e-spec-brief.md) E7。

## Problem

用户此前决定老板在 ops_bot 是只读的 Viewer([round8-decisions](../../../../design/_work/round8-decisions.md) R8-6 第 4 条)。老板经常是最先看到异常的人(价格离谱、账户权益异常、交易所公告),但只能在群里喊人或打电话;夜间值班交易员响应要几分钟到更久,其间策略可能继续开仓。用户决定给 Viewer 增加唯一的写权限"紧急停止"(R8-8 第 3 条)。

## Proposal

角色与权限归 [control-plane](../../../../docs/subsystems/control-plane.md) 与 [hft-ctl-proto 权限表](../../../../crates/hft-ctl-proto/README.md);ops_bot 行为归 [ops_bot 紧急停止](../../../../python/ops_bot/README.md#紧急停止stop);接手步骤归 [incident-response R](../../../../docs/runbooks/incident-response.md)。

1. **范围**:Viewer 只能发非 HARD、不带 `flatten`、`cancel_resting` 取默认值的 `Kill{Global}` 或 `Kill{Account(a)}`;不能 `Resume`,不能发任何其他写命令;审计照常(`ControlCmd`、`CmdResult`、gateway 审计 Note)。
2. **确认**:必须经 PREVIEW + `expect_state_version` 二次确认,只有发起预览的同一 `from.id` 能确认(群聊同样);gateway 拒绝未确认的 Viewer Kill。
3. **效果**:Kill cap 挡开仓、撤开仓类挂单;对冲、平仓、MarginStress 减仓、Broken 自动平仓等风险递减单照常放行([kill-risk-reducing-passthrough](2026-10-01-kill-risk-reducing-passthrough.md));持仓保留。
4. **入口**:ops_bot `/stop [账户]`(提示"紧急停止");发出后推送给全部 Trader/Admin,由交易员接手并决定何时 `Resume`。
5. **演练**:kill 演练与上线门槛 ⑪ 包含一次老板 `/stop`。

## Alternatives considered

**保持纯只读。** 发现异常到有人降级之间的延迟完全取决于电话能否打通。

**把老板设为 Trader。** 权限过大:升级模式、恢复、参数修改都需要判断与值班上下文。

**允许 `flatten`。** 会在不利价格平掉携带仓位,结果不可逆,而且需要判断是否有在途第二腿;留给交易员。

**允许 HARD。** HARD 连对冲与平仓都拦,会有意留下裸腿;只适合交易员在确认引擎行为错误时使用。

**不要确认、像交易员的降级一样立即生效。** 群聊里误触会停掉全部账户;确认只多几秒,换来防误触。

## Acceptance criteria

- 权限矩阵:Viewer 对 `GetSchema` 与已确认的默认参数 `Kill{Global}`/`Kill{Account}` 放行,对其他一切写命令与参数组合(含 `flatten`、`hard`、`Venue`、`Resume`)返回 `Forbidden`。
- ops_bot:`/stop` 的预览含"不会平仓"提示与受影响账户;他人点确认不执行;状态变化后重新预览;Trader 收到推送。
- 演练记录:老板 `/stop` → Kill cap 生效、持仓未动 → 交易员恢复。

## Risks

- 老板在关键时刻误停(例如调仓进行到一半):非 HARD 下对冲与平仓继续,不会留下裸腿;代价是开仓机会。
- 停止后持仓仍在、资金费照常结算:提示文案写明;平仓与否由交易员决定。
- 所有 Viewer 都获得这项权限:Viewer 名单保持最小。
