# 第五轮裁决:模式 / 账户 / 切换统一模型(2026-10-01)

来源:多模式多账户专项评审(29 条,P0 5 条)。核心问题:ModeTable 的一格同时存"期望状态"和"降级来源",导致 Shadow 冻结出场、自动降级覆写原值、Resume 无法精确撤销;另有共享交易所账户、命令默认落在视图账户、人工干预不入 journal 三类误交易路径。以下裁决优先于既有规格。

## M1 期望模式与上限(cap)分离 —— 唯一的模式模型

- **ModeTable 只存期望模式**,只由 `SetMode` 写。kill、自动降级、恢复锁存、条件门都是**独立的 cap 记录** `(source, scope, set_seq, params)`,互不覆盖。
- **有效模式** = 期望为 Shadow 时取 Shadow;否则取 min(期望, 全部生效 cap)。OMS 发出的 `RISK_REDUCING` 单**只受 HARD 约束**。
- `SetMode` 不清任何 cap;`Resume{source, scope}` 只删 (source, scope) 完全匹配的那一条;kill 字由剩余 cap 重新计算。
- 统一优先级表(写入 control-plane,其他规格链接它):

| 层 | 状态(作用域) | 挡住 | 仍放行 | 设置者 | 恢复 |
|---|---|---|---|---|---|
| 0 | HARD(全局) | 全部 Place/Amend、链上全部新意图 | 撤单、查询、死手刷新 | `Kill{Global, hard}`(Trader) | `Resume{Kill,Global}` + 确认 + kill 后新对账 ≤ Warn;未对冲量只能由 `HedgeFix` 命令处理 |
| 1 | DRAINING(进程) | 升级命令、Open | RISK_REDUCING | Drain / ReplaceRefdata / SIGTERM | 不可清除,随进程退出 |
| 2 | 时钟不可信(全局,条件型) | 策略意图 | OMS 的 RISK_REDUCING | `Timer{ClockBad}` | `ClockOk` 自动解除 |
| 3 | Kill:Global / Account / Venue(× 账户) | Open 及无 RISK_REDUCING 的单;按默认参数撤挂单 | Reduce / Flatten / Hedge | 命令 | 同级同目标 `Resume{Kill}` + 确认 |
| 4 | 锁存 CloseOnly:DailyLoss(账户)、Recon(账户, venue)、UnknownTimeout(账户, venue)、Broken/Reorg(账户, group)、Recovery(账户)、halted(策略, group) | Open | Reduce / Flatten / Hedge | sequencer 自动 | 各自 `Resume{source}` + 前置条件(见 M5–M7) |
| 5 | 条件门(不锁存):启动闸门未放行(只挡升级命令)、UNKNOWN_PENDING、EXPOSURE_GATE、VENUE_NOT_READY、STALE/NO_MID、链会话 Down | Open / 升级命令 | 其余 | 由输入推导 | 条件消失自动解除 |
| 6 | 期望模式(ModeTable) | Shadow:策略意图只记录;CloseOnly;Live | — | SetMode(升级:Trader + 确认 + 闸门放行) | SetMode |

- **Armed 与 Live 合并为 Live**:Live 永远遵守 `open_allowed`(现在的 ✅/👁 名单);要越过 👁 走参数覆盖。RunMode = Shadow / CloseOnly / Live。
- **Kill 级别简化为 Account / Venue / Global + HARD 位**;删除 Strategy 级 kill(策略层面用 SetMode)。
- 计算放在 sequencer(hft-risk 维护 cap 集合与有效模式);快照新增 cap 列表与闸门节(见 M10)。

## M2 Shadow 只在无仓位时生效

- `SetMode→Shadow` 仅当 (strategy, account, group) 净仓、在途、LegTracker 残余全为 0 时生效;否则有效模式为 CloseOnly、`CmdResult.detail=ShadowDeferred`,归零后自动落到 Shadow(写 Note)。
- 从 Shadow 到任何非 Shadow 都算**升级**(gateway 按迁移方向判定类别)。
- 有效模式每次变化都向策略投递 Mode 事件;降到 CloseOnly / Shadow 时撤销该 cell 的 Open 类挂单。

## M3 一个交易所(子)账户只属于一个下单系统

- 规则:任一时刻,一个交易所(子)账户只属于一个下单系统或实例。hft-core 从阶段 1 起使用**独立子账户**;新旧切换 = 旧系统平仓 + 资金划转到新子账户。
- 交易所不支持子账户、必须共享时(需记录原因):ownership 表 `(account, inst) → owner` 从阶段 1 第一天生效,放在 RiskParams;风控 #0 拒绝非 RISK_REDUCING 的 `NOT_OWNED` 意图;hft-core 禁用账户级 CancelAll 与账户级死手;kill-switch 只撤 client id 能解析为本系统的单;reconciler 读同一份 ownership。
- SetParams 校验:GroupId 全局唯一;任一 (account, inst) 同时至多被一个非 Shadow cell 引用(`InvalidArgs{InstOwned}`);影子版本用独立 GroupId 命名空间。
- canary(阶段 2)只做 Shadow + 回放比对,不建 gw、不载交易 key。

## M4 命令的账户语义

- 降级类命令不带账户参数时作用于**全部账户**;升级与中性命令必须显式写账户名,缺省即拒。
- gateway 要求 `CmdHdr.account == CmdScope.account`,否则 `InvalidArgs`;`cmd_hash` 覆盖二者与完整 CmdScope。
- 确认预览第一行显示目标账户标签与另一个账户的当前有效模式;Prepare 令牌绑定 sequencer 的 `ctl_state_version`(每次 SetMode/Kill/Resume/cap 变化 +1,随 CmdResult 返回),版本变了令牌失效。
- 审计与 `CtlAudit` Note 增加 `view_account`、`chat_type`。
- Kill 的 `flatten` 需 Trader + 确认且不得与 Global 组合;`hard`(原 `no_hedge`)需 Trader;Observer 与群聊只能发默认参数的 Kill。
- 权限的唯一归属是 hft-ctl 的权限表;control-plane、ops_bot、09 章链接它,不另写。

## M5 人工干预

- 解除任何 kill 或锁存**都不触发自动 HedgeFix**;解除时存在的未对冲量标为 Broken,只能由 `HedgeFix` 命令处理(预览基于最新对账)。
- 新增升级类命令 `AdoptExchange{account, venue, inst, report_id}`:把对账差写成 ledger 调整(class = Adjust),用于人工在交易所网页操作之后。阶段 1 即提供。
- `Annotate` 提前到阶段 1。

## M6 恢复、闸门与日亏

- 冷启动恢复不写 cell,加 `Recovery(account)` cap;闸门放行后由 `Resume{Recovery, account}`(Trader + 确认)解除。阶段 1 无自动武装(同步修正 refdata-restart note 的验收)。
- `RECONCILED` 闸门位要求该账户每个有 gw 的 venue 各有一组 ≤ Warn 的对账报告。
- 对账恢复条件(唯一定义):同一 (account, venue) 连续两轮 Ignore,且期间该 venue 无 UserStream Down。
- 日亏熔断按账户锁存;日切只重置 PnL 基线、不清锁存;`Resume{DailyLoss}` 把触发线重设为"当前 day_pnl − 额度";熔断不改 cell,Resume 后无需再 SetMode。
- 时钟不可信是条件型 cap(拒策略意图、放行对冲),恢复即解除;Unknown 超时不暂停对冲;watchtower 不为时钟发命令。

## M7 Drain 与 refdata

- Drain 设不可清除的 `DRAINING` cap;期间升级命令返回 `PreconditionFailed{Draining}`;由 ReplaceRefdata 触发的 Drain 超时仍以 75 退出;Drain 只等 Pending*,Unknown 写进快照。
- `ReplaceRefdata` 只由 Ops 在维护窗口人工发起,需确认,预览列出两个账户各自的挂单与 Live cell;pipeline 只产出 blob 与差异报告。

## M8 参数与 cell 生命周期

- RiskParams 的 `GroupLimits` 以 (account, group) 为键;`SetRisk` 带账户作用域,gateway 拒绝改动作用域外字段;放宽/收紧按账户逐字段判定。
- 新 group 取该 (strategy, account) 的 `default_mode`(由 `group=*` 写入,含"当前和以后");移除 group 即删除 cell;有仓位的 group 不得被移除;`account=*` 的升级命令要求全部目标账户已放行,否则整体拒绝。
- 策略定时器以 `GroupId` 为键(不用 gix 下标);被移除 group 的定时器在应用参数时撤销并写 Note。
- 阶段 1 不开放人工 SetParams;阈值经 pipeline 策略文件(`/pl set`)走同一条参数通道,避免人工与流水线 `VersionConflict` 僵局。

## M9 链上与"全停"

- 链上意图在 kill 下统一只看 `sflags.RISK_REDUCING`;hft-core → chain-core 心跳携带**每个账户的 KillBits 位图**,变化写成 chain journal Input。
- `/kill chain` = `Pause{all, hard=false}`(放行风险递减意图);`/kill all` = hft-core `Kill{Global}` + chain-core `Pause{all}` + 并行期旧引擎停止接口,回执逐项列出。
- 并行期新 bot 的 cdex 相关命令(`/engine*`)在 hft-core 没有 cdex 策略时直接拒绝并提示用旧 bot。

## M10 杂项

- kill-switch:新 (pid, boot_id) 的心跳计数一前进(Warmup 或 Running)就重新武装。
- 快照新增节 `Gates`;`RiskState` 节每条 cap 记录 `{source, scope, set_seq, params}`;未完成的对账分组写进快照;登记 Note 码 `CapSet/CapCleared/StartupGate/ShadowDeferred`。
- 门控拒绝(KillBlocked、Paused、LINK_DOWN、cap)不计入 Throttle 的 `consec_fail`;只有交易所或链上的执行结果才计入。
- out 环按 (venue, account) 配额,RISK_REDUCING 优先出环;DST 增加"测试账户限速风暴下正式账户 leg_gap p99 不变"。
- watchtower 阶段 1 只告警、不发命令;行情/时钟/磁盘类降级由 hft-risk 的条件型 cap 负责。
- 演练改为 `/kill account test flatten`;恢复步骤写 `Resume{Kill}` 并核对 kill 字已清。
