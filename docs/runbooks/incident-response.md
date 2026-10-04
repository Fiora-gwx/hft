---
description: "事故响应:P0/P1/P2 分级、通用首五分钟、按告警的处置(裸敞口与 Broken 自动平仓、残余核销、对账偏差、资金费差异、Unknown、崩溃与毒丸、停摆、行情与连接、存储退化、时钟、隔离核、秘密泄露、按账户 HARD、MarginStress(含串行兜底)、外部成交(强平/ADL/人工与 manual_hold)、交易所公告、dev2 护栏触发(tripped 标志、自动重启 sim)、老板紧急停止及其兜底(只撤单或 stop_core、停机标记、待补 Kill)、交易主机失联(授权表、待命清单、改看门狗目标))、kill 演练(测试网全量、正式账户只做 SmokeOrder 冒烟)"
scope: core
---

# 事故响应

## 适用范围

watchtower、kill-switch、reconciler、dev2 守护、机外看门狗发出的告警,以及任何人发现的异常。告警规则的定义与阈值在 [watchtower](../../apps/watchtower/README.md);本页只写处置。**降级永远允许,先降级再诊断**([09 §5.3](../../design/09-可观测性、风控与控制面.md))。

**谁能做什么**:降级与 kill 由 Trader/Admin 执行(不需要确认)。老板在 ops_bot 是 Viewer,只有一个写操作——**紧急停止** `/stop [账户]`:非 HARD、不带 flatten 的 `Kill{Global | Account}`,禁止新开仓、撤销挂单,已成交第一腿的对冲与平仓照常;确认绑定同一命令的预览、60 s 内有效(不比对状态版本);ctl-gateway 不可达时 ops_bot 改经 kill-switch 撤单,并在停引擎不会中断风险处置时停止 `hft-core@prod`(`stop_core`;会中断时停引擎要再确认一次 `/stop … core`,条件归 [ops_bot 紧急停止](../../python/ops_bot/README.md#紧急停止stop)第 6 步),记一条待补 Kill 并发 P0。Viewer 不能 `Resume`,也不能发任何其他写命令,停止之后由值班交易员接手(见 [R](#r-老板紧急停止stop之后))。`[需授权]` 步骤的授权人是老板或负责人(授权是人的决定,不需要 ops_bot 写权限)。角色定义见 [control-plane](../subsystems/control-plane.md),决定见 [viewer-emergency-stop](../../.agents/notes/proposed/architecture/2026-10-03-viewer-emergency-stop.md)。

**watchtower 只告警、不发命令**。自动降级全部来自 hft-risk 的 cap(时钟、存储、保证金与账户配置类是条件型 cap,对账、日亏、对冲失败等是按账户的 cap;清单与解除条件见 [hft-risk cap 记录](../../crates/hft-risk/README.md#cap-记录);行情与连接的可用性不设 cap,按报价可用逐单判定,见 F);本页中"引擎已自动……"都指这些 cap 与执行层的默认动作([hft-exec](../../crates/hft-exec/README.md))。

**模拟账户的告警**带"模拟"标签,不涉及真实资金;但模拟与实盘跑同一份代码,模拟上暴露的引擎缺陷(裸敞口、Broken 自动平仓、对账类除外)同样适用于实盘,按 P1 处理并检查实盘账户是否有同样迹象。

## 分级与响应时限

| 级别 | 含义 | 通道 | 响应 | 结束条件 |
|---|---|---|---|---|
| P0 | 资金风险:裸敞口、Broken 自动平仓失败(`ExecAutoFixFailed`)、自动处置被挡住过久(`ExecGateHeldNaked`:账户闸门未放行而有裸量,或自动有界平仓到期后迟迟不能开始)、有敞口的冷启动续接对冲超时(`OmsResumeTimeout`)、L3/flatten 退回串行减仓(`ExecSerialFallback`)、引擎减仓单被风控拒(`ExecEngineOrderRejected`)、对账偏差(`Recon` cap 或超名义阈值)、`ExternalFill`(人工网页成交除外)、`StaleGenFill`、有成交的外来订单、MarginStress L2/L3、日亏熔断、`Unknown` 超时、HARD 置位、死手刷新失败、sequencer 停摆、kill-switch 失活、存储严重退化、`/stop` 失败或 10 s 无结果、`/stop` 改经 kill-switch 兜底(只撤单或 `stop_core`)、`stop_core` 停机失败(停机标记作废为 `.aborted`)、待补 Kill 被拒、用户流缺口后自动入账的 `ExternalFill{Unknown}` 与之后补推成交的抵扣(`LedgerAutoHeldOffset`)、某把 key 的 UID 与 `venue_uid` 不符、交易主机失联(机外看门狗)、凭证代理的凭证已过期 | Telegram 告警群 + Bark critical + 电话类告警(供应商由负责人在上线门槛 ⑪ 之前选定;`/stop` 与待补 Kill 由 ops_bot 自己投递,交易主机失联由机外看门狗投递;维护静默期间看门狗只发 Telegram) | 5 分钟内有人 ack 并开始处置 | 敞口归零或受控,原因已知,postmortem 已开 |
| P1 | 可用性与需人跟进:Broken 已自动平仓(`ExecAutoFixDone`,`HedgeFailure` 待人工解除)、零敞口链接的续接超时(`OmsResumeTimeout`,第一腿尚无成交)、老板紧急停止(正常生效)、待补 Kill 已生效、人工网页成交(`ExecManualExternal`)与随之的 `manual_hold`(`ExecManualHold`)、资金费差异超 `funding_diff_alert`、`FundingCorrected`(含 `NoHistory` 自动入账)、venue 断连、行情静默、跨所标记价偏离(`MarkDivergence`)、限速风暴、MarginStress L1、实际杠杆超上限、p99 连续超预算(仅升级主机)、时钟偏移、隔离核闯入、archiver quarantine、验证包运行失败(`VerifyRun`)、dev2 护栏触发与护栏自动重启 sim、护栏的停机信号超阈值而 hft 用量不明显(只告警)、只含模拟账户的实例保持 Recovery(`RecoveryHeldSim`)、旧系统状态异常(只告警)、凭证代理刷新失败 | Telegram | 30 分钟内 | 条件消失或已降级且有计划 |
| P2 | 退化 | 日报 | 下一个工作日 | 记录结论 |

每个 P0 都写 [postmortem](../postmortem/README.md);P1 重复出现(一周 ≥ 3 次)也写。阈值与级别以 watchtower 规则为准,本表不重复数字。

## 通用首五分钟

1. `/ack <id>`(ops_bot),在告警群说明"我在处理"。
2. **先降级**(Trader/Admin,不需要确认),范围从小到大:`/mode <实例> <a> closeonly`(只停该实例开仓)→ `/kill venue <v>` → `/kill account <a>` → `/kill`(全部账户)。第二腿在途(`Unknown`、`PendingNew`)时不要用 `flatten`,先看 [A](#a-裸敞口nakedexposure断腿broken) 与 [C](#c-unknown-订单unknownage)。怀疑引擎行为本身错误(价格/数量/解码异常)时直接 `/kill account <a> hard`,见 L。
3. `[只读]` 看全局:`/status`(闸门、cap、连接、persist、reconciler、kill-switch、dev2 守护)、`/accounts`、`/p <a>`、`/risk <a>`、`/targets <a>`、`/alerts 20`。
4. 在 ops-log 记下 UTC 时间线(告警时间、ack、每个动作与其 `CmdResult`)。
5. 按下表跳到对应处置节。

演练:DRL-87。

## 处置

### A. 裸敞口(`NakedExposure`)、断腿(`Broken`)

某 (account, group) 的第二腿在重试与截止后仍未对冲,执行层把它标为 `Broken`,引擎置账户级 `HedgeFailure` cap,**该账户全部开仓被挡**(其他账户不受影响)。规则归 [hft-exec Broken 处置](../../crates/hft-exec/README.md),决定见 [broken-auto-fix](../../.agents/notes/proposed/architecture/2026-10-03-broken-auto-fix.md) 与 [resolve-broken-hard-only](../../.agents/notes/proposed/architecture/2026-10-03-resolve-broken-hard-only.md)。

**离开 Broken 的路**(都只减不增,都受 HARD 约束;前三条把裸量降到尘埃线以内(已成交名义 ≤ `dust_notional`)后,`Broken` 由引擎自动解除、cell 回到 Idle;账户级 `HedgeFailure` 一律要人工 `/resume hedgefailure <a>`):

1. **人工 `/hedgefix <a> <group>`**(Trader):**只平掉多出的一腿**(与自动平仓同一方向,但立即执行、由人选择时机);它不会补齐缺的一腿([control-plane `HedgeFix`](../subsystems/control-plane.md))。发出后本次 Broken 不再自动平仓(人已接手)。
2. **引擎减仓**:MarginStress L3 的默认减仓与 `/kill account <a> flatten` 对 Broken 的 group 同样生效,先对永续腿 reduce-only 减仓(`PerpFirst`,受 `max_reduce_notional`),再处理现货腿(见 M)。
3. **自动有界平仓**(无人工时):`Broken` 持续 `broken_auto_fix_ms`(ExecConfig,默认 2 min,只计引擎运行时间)后,执行层自动做**一次可执行的尝试**"平掉多出的一腿":永续腿 reduce-only;现货腿卖出,数量受库存与可用余额限制;按切片上限、IOC,总量 ≤ 当前裸量。哪些条件只是等待、哪些判失败与失败原因,以及计时口径(恢复期、`manual_hold` 与 HARD 期间、其他等待各怎么计)的唯一定义在 [hft-exec Broken 处置](../../crates/hft-exec/README.md#broken-处置) 的"计时";人要知道的只有:恢复期与 `manual_hold`、HARD 期间不会自动平仓,放行或解除后有完整的 `broken_auto_fix_ms` 窗口;等待太久的告警见下面处置步骤的第 7 步。
4. **核销残余**:裸量高于尘埃线却低于最小下单量,三条路都下不了单、`Resume{HedgeFailure}` 也解除不了(死锁)时——Broken 的 group,或以这样的残余结束、因此无法移除的非 Broken group——Admin 在私聊 `/writeoff <a> <group>`(`WriteOffResidual`,确认;只对静止的 group,预览显示多出的那一腿与其已成交裸量,规则归 [control-plane](../subsystems/control-plane.md) 与 [hft-oms 残余核销](../../crates/hft-oms/README.md#残余核销))→ 这一腿的裸量从该 group 移到未归属持仓(ledger `AdjustDelta::WriteOff`)、该 group **全部腿**的残余清零,`Broken` 随之可解除;核销的币量在交易所账户里仍在(引擎按持仓继续订阅该 inst),ops-log 记明并在日后人工处理。

| 结果 | 引擎 | 人 |
|---|---|---|
| 进入 `Broken`(P0) | 不再自动对冲这一组,开始计时 | 立即看 1–3 步,判断要不要在约 2 min 内自己处理 |
| `Note{ExecAutoFixDone}`(P1) | 裸量已在尘埃线以内,`Broken` 解除,cell 回到 Idle;`HedgeFailure` **仍在** | 第 5 步 |
| `Note{ExecAutoFixFailed}`(P0,带失败原因,码表归 [hft-exec](../../crates/hft-exec/README.md#broken-处置)) | 保持 `Broken`,本次 Broken 不再自动尝试 | 第 4 步 |

1. `/p <a>` 与 `/targets <a>` 看两腿;`/risk <a>` 看 Broken 持续时间与距自动平仓的剩余时间;交易所真值以 reconciler 最近报告为准(`/status` 中报告时间 < 60 s,否则先查 reconciler 是否在跑;没有"请求对账"命令,[control-plane](../subsystems/control-plane.md))。
2. 第二腿在途(链接已发出、`Unknown` 或 `PendingNew`)→ 先按 C 处理,不要人工补腿,避免重复对冲;自动平仓在这期间也在等待。
3. **自动平仓之前**,人只在以下情形介入:
   - 希望**立即**平掉多出的一腿、不等 `broken_auto_fix_ms`(例如行情正在朝不利方向走)→ `/hedgefix <a> <group>`(预览基于最新对账,核对差额与方向)。系统不提供"补齐缺的一腿"的命令:缺腿只能由策略在 `Broken` 解除、`HedgeFailure` 恢复后重新开仓。
   - 怀疑数量本身有问题(解码、单位、手续费扣币、对冲量超过上限被拦下的 `Broken`)→ `/kill account <a> hard`,阻止一切下单(含自动平仓),再查数量来源;不要绕过上限。
   - 其余情况不必抢:自动平仓只减不增,总量 ≤ 当前裸量。
4. **`ExecAutoFixFailed`**:按 `Note` 的原因处理多出的一腿——交易所拒单或无流动性时换时机再 `/hedgefix`;低于最小下单量 → 上面第 4 条路(核销残余);仍不行 → `[需授权]` 在交易所网页手工处理,之后按 B2 入账。裸量降到尘埃线以内后 `Broken` 自动解除,再做第 5 步。
5. **之后**:裸量为零(`/p <a>`、reconciler 下一轮 `Ignore`)、原因写进 ops-log 后,`/resume hedgefailure <a>`(Trader + 确认,前置条件见 hft-risk);该账户还有 Broken 的 group 时被拒(`CmdResult{Rejected, detail = BrokenRemaining}`),先按本节处理完全部 Broken。同一周出现 ≥ 3 次 `ExecAutoFixDone` 写 postmortem(说明执行层反复留下裸腿)。
6. 冷启动后不会立刻判 `Broken`:对冲截止只按引擎运行时间累计(停机时间不计);补腿、残余修剪、外来成交的默认处理与自动有界平仓要等**多出那一腿所在 venue** 放行才开始,期间不计截止(放行条件与计时归 [hft-exec 人工保持与恢复期抑制](../../crates/hft-exec/README.md#人工保持与恢复期抑制))。续接对冲只等**续接单所在 venue** 的对冲闸门子集(`IDENTITY`、`ACCOUNT_CFG`、`ORDER_CONNS`)且该链接无 `Unknown`;等待超过 `resume_hold_max_ms`(默认 30 s)→ 已有敞口的链接 `Broken` 并报 `Note{OmsResumeTimeout}`(P0),按本节处理;第一腿尚无成交的链接只报 P1、继续等 `Unknown` 收敛(规则归 [hft-oms](../../crates/hft-oms/README.md))。重启后第一分钟看到"待对冲"不要人工补腿。
7. **`Note{ExecGateHeldNaked}`(P0)**:自动处置被挡住超过 `gate_naked_alarm_ms`(hft-exec 参数),`v[]` 中的场合字段区分两种情形(场合、等待原因码与 `v[]` 归 [hft-exec 人工保持与恢复期抑制](../../crates/hft-exec/README.md#人工保持与恢复期抑制)):
   - **场合 1,账户启动闸门未放行**而存在 `Broken` 或超过补腿阈值的裸量:`/status` 看哪个 venue 的哪一位闸门没放行(多半是 `RECONCILED`:按 B 第 5 条处置对账差异)。
   - **场合 2,自动有界平仓已到期却一直不能开始**:按 `v[]` 的等待原因处理——venue 未放行同场合 1;group 不静止(有 Pending/`Unknown` 或在途链接)按 C;报价不可用或限速退避按 F;对手价越过越价上限或可用余额被挂单锁定时考虑 `/hedgefix`(时机由人定)或网页手工处理后按 B2 入账。
   - 两种情形下需要立即降风险时用 `/hedgefix` 或 `/kill account <a> flatten`(二者不受恢复期抑制)。

演练:DRL-12(launch B 第 14 步以 hft-sim 受控故障注入制造的 Broken)。

### B. 对账偏差(`ReconcileMismatch`)

触发:`Recon` cap 生效,或超过名义阈值的 `Warn`(规则见 [watchtower](../../apps/watchtower/README.md))。引擎已对相关 (account, venue) 禁开;对冲与平仓放行。资金费条目不在这里,见 [B3](#b3-资金费差异)。

1. `[只读]` 读报告明细:`/alerts` 详情或 journal 中同一 `report_id` 的 `ReconcileReport` 条目(`item`、引擎值、交易所值、差额);缺失成交的逐笔明细在 reconciler 的 `<state>/rounds/<report_id>.json`。
2. 分类:
   - **外来订单/成交**(交易所有、引擎无):人工网页下单(按 B2)、client id 解码错误、或我们自己的强平/ADL(应已是 `ExternalFill`,若不是 → 按 N 并报缺陷)。确认来源前不要撤。
   - **幽灵单**(引擎有、交易所无):对应槽通常处于 `Unknown` 或已被 kill-switch 撤掉;按 C 收敛。
   - **仓位差**:逐笔对照 journal 的成交与交易所成交历史,找缺失的成交(用户流断开期间最常见);迟到的旧代成交(`StaleGenFill`)已由引擎入账并报 P0,核对其数量。
   - **现货库存/余额差**:手续费按基础币扣除(净量)、人工划转(用 `/transfer` 登记)、非策略资产变化(用 `/baseline` 重新登记基线)。资金费结算造成的余额差由账本自动校正(B3),不在这里处理。
3. 修复后等 reconciler 连续 2 轮该 (account, venue) 全部 `Ignore`,且期间该 venue 的用户流没有 `Down`。
4. `[需授权]` `/resume recon <a> <venue>`(Trader + 确认)。`Resume` 只删这一条 cap、不改期望模式、不触发 HedgeFix,也不把残余直接设为 `Broken`(只有解除 HARD 的 Kill 时才按补腿口径判定 Broken,规则归 [hft-exec](../../crates/hft-exec/README.md));解除后仍有的未对冲量由补腿在新窗口里处理,补腿失败才进入 `Broken`,按 A 处理。
5. 冷启动后的对账:启动闸门 `RECONCILED` 要求这一轮(不含资金费条目)全部 `Ignore`,或每项差异已经 `AdoptExchange`(仓位、余额)或 `SetBalanceBaseline`(非策略资产)处置;差异未处理时该账户不会放行开仓,也不会自动解除 `Recovery`。

演练:DRL-106。

### B2. 人工在交易所网页操作之后(AdoptExchange)

人工网页操作不经 hft-core,必须记进 journal,否则对账差永远不消失。

1. 操作前 `/note <text>` 写明要做什么、哪个账户与资产;能先 kill 的先 kill。账户里有 `Broken` group、或人工操作本身会造成未对冲量时,先 `/kill account <a> hard`——HARD 下引擎不发任何单,包括 Broken 自动平仓,避免与人工操作重叠。人工成交被引擎识别为 `ExternalFill{Manual}` 后,该 cell 进入 `manual_hold`(见 N),补腿与自动平仓暂停到有人或策略接手。
2. 在交易所网页完成操作,记下订单号/成交号/划转号。
3. 等 reconciler 下一轮报差(`Recon` cap 生效、P0 属预期),取 `report_id`。
4. `[需授权]` `/adopt <a> <venue> <条目> <report_id> [entry_px]`(Trader + 确认):条目可以是永续仓位或现货余额;预览列出报告中的引擎值、交易所值与将写入 ledger 的调整,逐项核对与第 2 步的记录一致再确认。两腿都被人工改过时,两腿各 adopt 一次,期间该 group 保持 `Broken`(HARD 生效,不会自动平仓)。
5. 等连续两轮 `Ignore` → `/resume recon <a> <venue>`;再 `/resume kill <a>`(核对见 kill 演练);仍为 `Broken` 的 group 与 `HedgeFailure` 按 A 第 5 步解除。
6. ops-log 记录全过程与交易所单号。

演练:DRL-105、DRL-106。

### B3. 资金费差异

资金费条目**不进**启动闸门 `RECONCILED`、**不设** `Recon` cap,只告警(规则与 `funding_diff_alert` 阈值见 [watchtower](../../apps/watchtower/README.md),默认 1 USDT 或 1%,P1)。账本按交易所逐笔资金费流水自动校正:结算时刻早于 `funding_recon_lag_ms`(默认 10 min)的差额自动记为 `AdjustDelta::Funding` 并写 `Note{FundingCorrected}`,比对窗口为最近 48 h(规则归 [hft-ledger](../../crates/hft-ledger/README.md),决定见 [funding-recon-alert-only](../../.agents/notes/proposed/architecture/2026-10-03-funding-recon-alert-only.md))。

1. 正常情况下人**什么都不做**:不要为资金费差异发 `/adopt`、不要 `/resume recon`(它不会因资金费出现)。
2. `[只读]` 告警反复出现或单笔差额大时:看 `Note{FundingCorrected}` 的 venue、inst、结算时刻与金额;核对该 venue 的 `VenueCaps.funding_settle`(只推余额或只给费率的所,合成值与流水本来就可能有小差)与结算时刻的持仓。
3. 同一 venue 连续多期出现大差额(超过阈值的 10 倍,或方向一直相同)→ 当作缺陷:报给对应 venue(资金费解析或合成),写 postmortem;期间账本仍以交易所流水为准自动校正。
4. 引擎侧没有对应记录的资金费(`NoHistory`,例如停机期间结算)在比对窗口结束后由账本按交易所金额自动入账并写 `Note{FundingCorrected}`(P1),不需要人工。reconciler 在分级前已用同组报告的资金费条目修正结算资产的余额差,所以资金费不会以余额差的形式触发 `Recon`;仍出现在余额条目里的差额不是资金费造成的,按 B 处理(规则归 [hft-ledger](../../crates/hft-ledger/README.md)、[reconciler](../../apps/reconciler/README.md))。

### C. Unknown 订单(`UnknownAge`)

原则:`Unknown` 只能由证据收敛;超时、403/429/5xx、断连都不是证据;**Unknown 期间不做任何反向操作**([defensive-patterns](../defensive-patterns.md))。

1. `[只读]` `/orders <a>` 列出 Unknown 与在途链接。
2. 确认 gw 查单与 reconciler 查单在进行(journal 中的查询结果与 `ReconcileReport` 的收敛条目);都失败 → 检查该所 REST 可达性与限速(`RateLimitStorm`)。
3. 人工核对(只读):交易所网页/REST 按 client id 查单。查到结果后**不要**人工改状态;等 reconciler 把证据送进 journal。reconciler 持续失败 → `[需授权]` 重启 reconciler。
4. "查不到"不等于"没成交":对冲腿的"查不到"保持 Unknown,直到成交历史确认;已回收的槽仍保留墓碑,迟到成交照常入账并报 P0([hft-oms](../../crates/hft-oms/README.md))。
5. 该账户在 Unknown 收敛前保持禁开(引擎自动)。

演练:DRL-102(崩溃后由证据收敛)、DRL-10(断连期间产生的 `Unknown`)。

### D. hft-core 崩溃、重启风暴、拒绝启动

1. `systemctl status hft-core@prod`、`journalctl -u hft-core@prod -n 200`;看退出码:
   退出码的含义与 systemd 的重启行为归 [hft-core 命令行与退出码](../../apps/hft-core/README.md#命令行与退出码);本页只写处置:
   - **2(配置)**:最近的配置变更 → 回退配置文件;`hft-core --config … --check` 通过后再启动。身份不符(某把交易 key 的 UID ≠ `venue_uid`,P0)多半是 key 放错了账户——先在交易所禁用该 key,按 K 第 3 步换 key;kill-switch 自检与 reconciler 启动对自己的 key 做同样核对。`DustBelowMinNotional` 等装配校验失败按日志修正配置。
   - **3(主机)**:`/run/hft/hostprofile.json` 的不符项 → [host-tuning](host-tuning.md)(升级主机)或 dev2 护栏配置(dev2);不得改 `on_mismatch` 绕过。
   - **4(journal 恢复)**:不要删除任何 journal 文件;保全热段与冷副本,`journal-archiver check` 与 `replayer verify --from-seq <最后快照>` 定位;从较早快照恢复需负责人决定(`[需授权]`),用 hft-core 的 `--recover-from-snapshot`(带严格对账,[hft-core](../../apps/hft-core/README.md)),并写 postmortem。
   - **134(abort)**:panic 或 watchdog;保全 core 与日志;systemd 会自动重启。
2. **毒丸输入**:两层计数,都记在 `DurableMeta`([hft-core 崩溃隔离](../../apps/hft-core/README.md#崩溃隔离与从快照恢复)):
   - **恢复期**(重放 journal 尾部时反复崩溃,hft-core 的 `boot_guard`):达到 `poison_crash_limit` 次后不再重放尾部,从该快照起步并按 `tail_lost` 处理(`Note{TailSkipped}`,P0);
   - **运行期**(处理某条输入时反复崩溃,sequencer 按输入哈希计数):尾部照常重放,该输入在本进程生命周期内被隔离(`Note{InputQuarantined}`,P0)。
   两种情形下自动解除 `Recovery` 都被禁用:人工核对 `Note` 中给出的输入(字段归 [hft-event](../../crates/hft-event/README.md))、等严格对账通过后 `/resume recovery <a>`。被跳过或隔离的输入保存为回归金样。
3. **新版本崩溃后回滚**:新版本没有留下 Drain 快照时,旧版本不能直接从该尾部启动;按 [release-and-deploy 回退](release-and-deploy.md#回退) 的"崩溃后回滚"步骤用 `--recover-from-snapshot`。
4. **恢复不一致**:恢复时重算的决策与实际已发单不一致 → 引擎把该 (venue, account) 全部置 Unknown、闸门严格化、报 P0;按 C 与 B 处理后再解除。
5. 崩溃后确认:有死手的所在倒计时内撤单;kill-switch 已对每个 Live 账户的每个 venue 撤单(有死手的 venue 也再撤一次);`Note{JournalTailLost}` 是否出现。崩溃前未对冲的链接按 A 第 6 步续接。
6. 重启后若出现 `Note{ModelCheckpointDiscarded}` 或该实例没有可用检查点:策略 Model 从空起步、样本不足,按 [launch Model 预热](launch.md#model-预热) 重新预热;按 group 的 `Note{ModelGroupReset}` 由 pipeline 自动预热,只需在 `/pipeline <实例>` 核对完成。
7. 重启风暴:600 s 内第 3 次失败后 systemd 停止重启;每次冷启动后每个账户都带 `Recovery` cap,找到根因前不要升级模式;`systemctl reset-failed hft-core@prod` 后再启动 `[需授权]`。

演练:DRL-104(退出码 2)、DRL-102(`kill -9`)、DRL-86(崩溃后回滚);systemd 的重启行为由 CORE-015–CORE-017 验证。

### E. sequencer 停摆(`SequencerStall`)

1. systemd watchdog 会在 `WatchdogSec` 内 abort 并重启;kill-switch 会对各 venue 撤单。确认两者都已发生。
2. 进程仍存活且未被 abort(watchdog 失效):`[需授权]` `systemctl kill -s SIGABRT hft-core@prod`,保全 core。
3. 需要在 hft-core 不可用时撤单:按 G。

演练:DRL-103;与老板 `/stop` 同时发生时 DRL-95。

### F. 行情静默、断连、限速风暴、延迟超预算

1. 行情断开、静默超过 `conn_liveness_ms` 或重同步中时,该 inst 的报价不可用:风控以 `STALE_MID` 拒绝它的 Open(第二腿 venue 的报价不可用时第一腿以 `LINK_STALE` 拒绝),执行层需要这份报价的单(对冲、风险递减、自动平仓)等待报价恢复;连接恢复后自动继续,这一类没有 cap(规则归 [hft-risk 检查链](../../crates/hft-risk/README.md#检查链按廉价--昂贵短路适用范围以本表为唯一定义) 与 [hft-exec 报价可用](../../crates/hft-exec/README.md#报价可用连接健康))。`/status` 看连接状态、重同步与重连次数。单个 inst 长时间无推送但连接有 pong 仍视为报价有效;`MarkDivergence`(P1,同一资产某 venue 的标记价/中间价偏离其余 venue)是旁证:核对该 venue 该 inst 是否停牌、行情是否卡住,需要时对该 venue `/kill venue <v>` 并报 venue 缺陷。盘口长时间不变**不是**过期:对冲、风险递减单与自动平仓按 BBO 连接健康判定报价可用(连接 Live、不在重同步、最近任一帧——含 pong——在 `conn_liveness_ms` 内),规则归 [hft-venue](../../crates/hft-venue/README.md) 与 [hft-exec](../../crates/hft-exec/README.md)。
2. 限速:检查是否有进程在 prod 出口地址上额外请求(reconciler、kill-switch、保证金轮询);sim 实例与 pipeline 各用自己的出口地址([deploy 出口 IP](../../deploy/README.md#出口-ip)),不占 prod 的 IP 级限额——若 sim 的连接出现在 prod 地址上,是部署缺陷;退避是否生效。
3. 用户流出现未知事件类型只计数、不触发重同步;若计数突增,查该所是否改了协议(venue README 的 `ignorable_user_events`)。
4. p99 超预算只在升级主机上处置:对照 `IsolatedCoreIntrusion`、`IrqDrift`、`ClockOffset` 是否同时出现,以及 IRQ 是否仍落在 IRQ 专用核(`hostprobe verify`);sim 实例是否超出 `hft-sim.slice` 上限或有线程落到隔离核(`ps -L -o psr`);交易所侧(回执延迟上升而主机内分段不变)只记录。`dev2` 没有延迟验收,不处置延迟告警。

演练:DRL-10。

### G. 控制面不可用(需要撤单但命令发不进去)

1. `[需授权]` `kill-switch fire --account <a> --venue <v|all> --reason "<文本>" --yes`:用独立凭证撤全部挂单并核验。老板的 `/stop` 在 hft-core 不健康时会自动走同一动作(经 kill-switch 的本地触发套接字,只撤单、不平仓,见 R)。
2. 有死手的 venue:停止刷新即生效(hft-core 停止后自然到期)。
3. `kill-switch fire` 只撤单:hft-core 若仍存活、只是控制面不通,它可能继续下新单——`[需授权]` `systemctl stop hft-core@prod`(Drain;超时后 systemd 终止),再按 D 处理。经 ops_bot `/stop` 走的 `stop_core` 兜底会自动做这一步(R2)。

演练:DRL-101。

### H. 存储退化(persist 滞后、磁盘、热段内存)

housekeeping 把 persist 滞后与磁盘余量写成 `Timer{PersistLag|DiskLow}` 输入,hft-risk 据此设条件型 cap `Storage`:超过第一阈值**禁开**;超过第二阈值引擎自行 **Drain**(阈值见 [hft-risk](../../crates/hft-risk/README.md))。watchtower 只告警。

1. `PersistLag`:`iostat -x 1` 看冷副本卷;archiver 或其他进程是否占满 I/O。
2. `DiskLow`:确认 archiver retention 是否在删已提交小时(`journal-archiver check`);不手工删除 journal 文件,除非 `journal-archiver check` 证明该小时已提交且核验通过 `[需授权]`。扩卷(`[需授权]`,EBS 在线扩容)优先于删除。
3. `dev2`:tmpfs 热段占用接近 `hft.slice` 内存上限时,先按 hft-core 的"预算超支处理顺序"降 raw journal、缩订阅;不得关 mlock、不得启用 swap。干净 Drain 停止的实例的热段由 `hft-shm-release` 自动释放;崩溃停止的实例热段保留到下次启动恢复,不要手工删除([deploy dev2 护栏](../../deploy/README.md#dev2-护栏))。
4. 引擎已 Drain 时:修复后按 [release-and-deploy](release-and-deploy.md) 的启动与第一分钟验收重启 `[需授权]`。

演练:DRL-107。

### I. 时钟偏移(`ClockOffset`)

`ClockOffset` 告警是 watchtower 的可选扩展;未启用时由 `ClockBad` cap 的 `Note{CapSet}`(P1)发现同一情形。

1. `chronyc tracking`、`chronyc sources -v`;PHC refclock 是否可用(升级主机)。
2. 时钟不可信时 hft-risk 的条件型 cap `ClockBad` **只挡开仓**,对冲、平仓与硬动作照常;`ClockOk` 后自动解除。修复 chrony 即可,不需要升级模式;若期间有人手动降级过,恢复按 `[需授权]` 升级。

### J. 隔离核闯入、IRQ 漂移(只适用于 `c4`/`c8`/`c16`/`c32`/`metal`)

`IsolatedCoreIntrusion`、`IrqDrift` 告警是 watchtower 的可选扩展;未启用时由 `hostprobe verify` 复核(季度复验 DRL-66)与 hft-core 下次启动的主机检查发现。

1. 告警详情(或 `ps -eLo pid,psr,comm` 采样)里的 pid/comm;`ps -o pid,psr,comm -p <pid>`;属于系统服务则修正其 `AllowedCPUs`,属于 kworker 则重跑 `hft-workqueue.sh`;属于 sim 实例是部署缺陷(sim 只允许非隔离核):`systemctl stop hft-core@sim`,核对 `hft-sim.slice` 的 `AllowedCPUs` 与生成物,修正后再启。
2. IRQ 漂移:ENI-B 全部队列的 IRQ 必须在 IRQ 专用核、ENI-C 的在 sim 的 IRQ 核、ENI-A 在 hk 核([deploy 运行期调优](../../deploy/README.md#运行期调优));检查是否有人启用了 irqbalance;重跑 `hft-irq-pin.sh`;hostprobe 复核(IRQ 落点与 busy poll 参数是 hard 项,不符时 hft-core 下次启动会以退出码 3 拒绝)。

### K. 秘密泄露或可疑访问

1. `[需授权]` 立即在交易所后台禁用相关 key(交易、撤单、只读分别评估);交易 key 没有提现权限,资金不能被提走,但可以被用来交易。
2. `/kill account <a> hard`(该账户的一切下单立即停止,见 L);怀疑多个账户时 `/kill hard`。
3. 轮换 Secrets Manager 中的秘密并新建 key(权限与 IP 白名单按 [launch](launch.md) D 第 22 步;白名单只含 prod 出口 EIP);重启使用该秘密的单元;核对新 key 的 UID:hft-core 的 `IDENTITY` 闸门位(每把交易 key 分别核)、`kill-switch selftest`、reconciler 启动日志。
4. 检查 journal 与日志中不应出现秘密;postmortem。
5. 怀疑冷进程(ops_bot、pipeline、treasury 或其依赖)被利用:`[需授权]` 停该单元;`sudo /opt/hft/current-core/deploy/imds/hft-imds-lock.sh status`(IMDS 锁定的拦截计数)看是否有 hft 服务用户尝试访问 IMDS;它能拿到的只有凭证代理发给它的短期 S3 凭证(只覆盖其前缀,[deploy IMDS 锁定](../../deploy/README.md#imds-锁定与凭证代理)),在 IAM 中撤销该消费者角色的会话;交易 key 仍按第 1–3 步评估。

演练:DRL-108。

### L. 按账户 HARD

适用:某个账户的行为本身可疑(价格或数量异常、疑似解码/单位错误、key 疑似泄露、策略失控),此时宁可留下裸腿也不再发任何单。

1. `/kill account <a> hard`(Trader;群聊不可用,需私聊)。效果:该账户全部 `Place`/`Amend` 被拦,**包括**对冲、平仓与 Broken 自动平仓等风险递减单;撤单、查询、死手刷新照常;其他账户不受影响。置位即 P0,告警列出该账户当前的未对冲量。
2. 未对冲量由人决定处理方式:等原因查清后 `/hedgefix`,或 `[需授权]` 在交易所网页手工处理后按 B2 入账。
3. 解除:`/resume kill <a>`(Trader + 确认);前置条件是 kill 之后的新一轮对账满足 B 第 5 条的要求。解除本身不对冲;解除时裸量超过补腿阈值(超 `tol` 且高于尘埃线)、且不处于 `manual_hold` 的 group 被设为 `Broken`(规则归 [hft-exec](../../crates/hft-exec/README.md)),HARD 期间暂停的自动平仓在解除后重新给出完整的 `broken_auto_fix_ms` 窗口,窗口内无人工 `HedgeFix` 就按 A 自动平掉多出的一腿——不希望这样时,在解除**之前**先手工处理并入账,或解除后在窗口内 `/hedgefix`。
4. 全局 HARD(`/kill hard`)同理,作用于全部账户。

演练:DRL-91。

### M. 保证金压力(`MarginStress`)

档位、阈值与引擎动作的唯一归属是 [hft-risk](../../crates/hft-risk/README.md)(决定见 [margin-liq-distance](../../.agents/notes/proposed/architecture/2026-10-03-margin-liq-distance.md))。概要:每 (account, venue) 按**到强平价的距离** `liq_dist`(永续持仓中最近的那个,`liq_px` 取交易所报告与账本估算中更保守者)分档——L1 禁开;L2 通知策略在截止时间内减仓;L3 或截止已过 → 执行层按比例先减永续;保证金数据过期视为 L1;维持/保证金余额比率只做交叉校验(高到阈值时至少 L2)。**L3 与 `flatten` 同样作用于 `Broken` 的 group 与该账户不属于任何 group 的永续持仓**:设这类引擎目标时先撤该 inst 上本账户的策略挂单,再对永续腿 reduce-only 减仓(`PerpFirst`),再处理现货腿;另一腿不满足"报价可用、venue 就绪、下单连接 Up"时只要求永续腿可用,退回串行减仓(`SerialFallback`,见第 6 步)([hft-exec](../../crates/hft-exec/README.md))。

1. `[只读]` `/risk <a>`:各所 `liq_dist`(及决定它的 inst、`liq_px` 来源是交易所还是账本估算)、到各档的距离、交叉校验比率、各 inst 实际杠杆与 ADL 档、数据年龄;`/treasury <a>`:普涨情景下到各档的涨幅与划转建议。
2. **L1**(P1):确认是真实的保证金压力还是数据过期(保证金轮询被限速或用户流断开 → 按 F)。真实压力时按 treasury 建议准备划转。
3. **L2/L3**(P0):不要与引擎对着干——不要 `/resume`、不要放宽 `SetRisk`、不要手工加仓。可以做的:
   - `[需授权]` 人工把另一个所的闲置资金划到该所合约钱包(在交易所网页操作;交易 key 不能划转),完成后 `/transfer` 登记;
   - 需要比引擎更快地降风险时:`/kill account <a> flatten`(Trader + 确认;含 Broken 的 group)或对该实例 `/mode … closeonly`;
   - 观察 `/targets <a>` 中 `reason = MarginStress` 的减仓进度与两腿间隔(永续先减,现货随后)。
4. 压力解除后 cap 按 hft-risk 的解除条件(含滞回)自动清除;写 postmortem(L2 及以上),复核 treasury 阈值、策略的永续名义上限参数(归策略 README)与交易所实际杠杆(`/risk` 显示;超过 `RiskParams.max_perp_leverage_x100` 时该账户已被禁开)。
5. `liq_dist` 与比率给出的档位不一致(例如距离很远而比率很高):核对交易所 `liq_px` 与账本估算哪个异常,按 venue README 的口径报缺陷;期间以更保守的档位为准(引擎已如此处理)。
6. **串行兜底 `Note{ExecSerialFallback}`(P0)**:L3 或 `flatten` 减仓时另一腿(现货)的报价不可用、venue 未就绪或下单连接不在,执行层先减永续、现货腿等其恢复后再减——这段时间账户**有意**处于现货裸多。处置:按 F 查现货 venue 的连接;报价恢复后核对 `/targets <a>` 中现货腿开始减仓;长时间不恢复且价格朝不利方向走 → `[需授权]` 在交易所网页卖出现货并按 B2 入账。
7. **引擎减仓单被风控拒 `Note{ExecEngineOrderRejected}`(P0)**:减仓/平仓/补腿单被价格带或单笔名义检查拒绝,执行层按执行失败处理。核对 `RiskParams` 的 `rr_max_cross`、`max_order_notional` 与 ExecConfig 的 `force_slice` 是否一致(装配与 `SetRisk` 本应以 `ExecRiskMismatch` 拒绝不一致的配置);价差异常扩大时考虑 `/kill account <a> flatten` 或网页手工减仓。

演练:DRL-21、DRL-22。

### N. 外部成交:强平、ADL、人工、交割(`ExternalFill`)

引擎已在同一事件内把这笔成交记入 ledger 与 LegTracker;策略收到 `External` 事件。强平、ADL、交割与无法识别的外部成交报 P0,策略在 `external_deadline_ms` 内未处理时执行层按默认规则平掉多出的一腿;**人工网页成交(`cause = Manual`)执行层不做默认处理**,只报 P1 `Note{ExecManualExternal}`,并让该 cell 进入 **`manual_hold`**(`Note{ExecManualHold}`,P1):补腿、残余修剪、外来成交默认处理与自动有界平仓都暂停(不计次、不计截止),直到策略对该 group 下了非清除类的目标或订单、有人 `/hedgefix`,或出现引擎目标(L3/flatten/HARD 对齐)。之后由策略按自己的规则处理(portfolio-carry:该 group 冻结、不设新目标,直到 `clear_block` 命令或两腿重新对齐,规则归其 README)([hft-exec](../../crates/hft-exec/README.md))。

1. `[只读]` 告警详情的 `cause`(`Liquidation | Adl | Manual | Delivery | Unknown`)、inst、数量、价格;`/p <a>` 看另一腿;`/targets <a>` 看默认处理是否已发出。
2. **强平/ADL**:在交易所网页核对强平/ADL 记录与数量;核对另一腿已按净量处理(现货卖出受可用余额限制);查 `/risk <a>` 的保证金历史——强平发生在 L3 之前说明阈值或数据有问题,`[需授权]` 先 `/kill account <a>` 再查。
3. **人工**:有人用网页成交了——核对 ops-log 有无对应 `/note`;没有 → 按 K 排查;有 → 按策略 README 处理被冻结的 group(补齐或平掉另一腿后发该策略的解除命令;只想平掉多出的一腿时 `/hedgefix` 同时解除 `manual_hold`),对账差按 B2 入账。
4. **Unknown**:识别不了原因的外部成交按强平处理,并报缺陷给对应 venue(`own_liquidation_marker`)。用户流断开期间漏掉的 ADL 或强平:两轮对账都没有用户流断开、该 (account, venue) 的用户流恢复后已稳定至少一个对账周期,而后一轮仍见到"永续同号缩小、无在途、无 Unknown"的差异时,sequencer 在这条 `ReconcileReport` 内合成一笔 `ExternalFill{cause = Unknown}` 入账并 P0(不另写记录;置位条件归 [reconciler](../../apps/reconciler/README.md),合成归 [hft-sequencer](../../crates/hft-sequencer/README.md));交易所之后补推的同 inst、同方向强平/ADL 先抵扣这笔自动入账量,只入账超出部分并写 `Note{LedgerAutoHeldOffset}`(P0,规则归 [hft-ledger](../../crates/hft-ledger/README.md))——核对两者合计与交易所成交历史一致。现货差异与反向差异仍按 B/B2 人工处理。
5. 核对 reconciler 下一轮 `Ignore`;写 postmortem(强平/ADL 必写)。

演练:DRL-105(人工成交)、DRL-106(用户流断开期间的人工成交)。

### O. 交易所公告:下架、资金费结算周期变化、维护

1. 公告发现者(任何人)通知值班;Trader 经 ops_bot `/notice …` 或运维 CLI 发 `VenueNotice`(`Delisting` 是降级、立即生效;回执列出受影响的账户、group 与持仓)。
2. `Delisting{at}`:引擎对该 inst 设 `Delisting` cap(只平不开)并通知策略;核对策略在下架前完成退出,来不及时 `/kill account <a> flatten` 或手工处理后按 B2 入账。pipeline 下一轮会把该资产标为只平不开。
3. `FundingInterval{new_s}`:策略收到新周期;核对 `/risk`、资金费统计随之更新。
4. `Maintenance{from, to}`:维护窗口内该所的连接与对账告警属预期;窗口结束后核对闸门与对账。

演练:DRL-109。

### P. dev2 护栏触发

dev2 守护**只按停机信号**动作:旧系统所在 `system.slice` 的 CPU 与内存压力、`MemAvailable`、swap-in 速率持续超阈值,**且** hft 自身用量明显(`hft.slice` 或 `hftbatch.slice` 的 CPU 或内存超阈值;否则只告警)时,按两级动作停掉 hft 进程(一级停 `hftbatch.slice` 的成员,`hft-verify` 最先、`hft-replay-verify` 其次;二级先停测试网联调用的 `hft-core@prod` 与 `hft-reconciler`、再停 `hft-core@sim`;watchtower、ops_bot 与 kill-switch 不停),写标志 `/var/lib/hft/guard/tripped`(存在期间任何批处理单元都不能启动,定时器到点也只是跳过;整机重启后仍在),并直接发告警。整机 PSI、swap 已用量增长、旧系统单元重启、非 active 或状态文件过期**只告警、不停 hft**。二级停掉 sim 后,停机信号回落并持续规定时长、且此后无人手动启停过 sim,守护自动重启 `hft-core@sim`(每 UTC 日至多一次,P1;watchtower 照常出日报);它从不启动 prod、批处理与冷进程。信号、阈值、时长与动作见 [hft-core dev2 护栏](../../apps/hft-core/README.md#dev2-护栏),单元、标志与 slice 见 [deploy dev2 护栏](../../deploy/README.md#dev2-护栏)。**旧系统优先**。dev2 上没有正式实盘账户,停机不留下真实资金的风险。

1. 先确认旧系统正式账户正常(其进程、心跳、最近交易);旧系统异常由它自己的流程处理,与本页无关。
2. **只收到告警**(hft 未被停):`[只读]` 看 `/var/lib/hft/guard/events.jsonl` 与同时刻的资源信号;资源正常 → hft 照常运行,不做任何操作;怀疑 hft 影响旧系统(例如它卡住时 hft 的 CPU 或 I/O 偏高)→ Trader 可对 sim 实例 `/shutdown` 主动 Drain、`hft-window stop` 结束测试网联调窗口,并与旧系统维护者确认。
3. **hft 已被停**:`[只读]` `events.jsonl` 看触发的停机信号与停了哪些单元;两个 hft-core 是否干净 Drain(journal 末尾是 Drain 快照;干净停止的实例热段已由 `hft-shm-release` 释放,崩溃停止的保留);测试网账户是否留下挂单或裸腿(kill-switch 是否动作)——有则记录,测试网上的残留按 A/C 的步骤清理,不涉及真实资金。联调窗口内触发时批处理本来就停着,守护直接进入二级:先 `sudo …/hft-window stop`(它在 prod 热段未释放时会保留窗口标志并提示,按提示处理),再做下面的恢复。
4. 找原因:hft 的内存/CPU 是否超出 hft-core 的 dev2 预算;是否联调窗口与模拟长跑同时运行;是否同时有 collector 研究窗口等其他负载;`system.slice` 的压力是否真由 hft 引起(对照 `hft.slice`、`hftbatch.slice` 的 `cpu.stat` 与旧系统的负载;守护在 hft 用量不明显时只告警,不会停机)。
5. **恢复**:`hft-core@sim` 由守护在信号回落后自动重启(每 UTC 日至多一次;`events.jsonl` 有记录、P1 推送);核对它启动后闸门与模拟日报正常;非干净停机后的 sim 实例在没有 `RecoverMismatch`、没有毒丸时自动解除 `Recovery`,否则报 `Note{RecoveryHeldSim}`(P1):按 D 第 2 条核对后 `/resume recovery <模拟账户>`(规则归 [hft-risk](../../crates/hft-risk/README.md))。人在守护自动重启之前手动启停过 sim,守护不再自动重启它,由人决定何时启动。其余单元 `[需授权]` 在停机信号回落、旧系统稳定后由人恢复(watchtower 与 ops_bot 没有被停):先 `sudo hft-dev2-guard clear` 清除 tripped 标志(dev2 重启不会清除它),再 `systemctl start hft-journal-archiver hft-pipeline hft-treasury`(两个定时器没有被停,标志清除后到点照常启动批处理);测试网实例只在下一个联调窗口经 `hft-window start` 启动。同一 UTC 日第二次停掉 sim → 守护不再自动重启,人工决定:降低 hft 负载(减订阅、关 raw journal、减模拟实例、缩 `hft-verify` 预算)后再启动或另排时间。护栏停机与自动重启是否影响门槛 ⑦ 的计时,按 [testing §8](../testing.md#8-上线门槛)。
6. **退路**:同一周内护栏第二次因 hft 自身用量触发停机(只告警的不计)、旧系统的延迟/成交出现可测劣化,或 M3 标定演练不通过 → 写 postmortem 并请负责人决定是否另开一台 `c7g.large` 专给 hft(`[需授权]`,规则归 [hft-core dev2 护栏](../../apps/hft-core/README.md#dev2-护栏),部署见 [deploy dev2 护栏](../../deploy/README.md#dev2-护栏) 第 9、10 条)。
7. **M3 标定演练**(`[需授权]`,dev2 是旧系统的生产主机):按 [deploy dev2 护栏](../../deploy/README.md#dev2-护栏) 第 10 条让批处理满载 10 min,期望守护不动作、`system.slice` 的压力不升;结果记在本页 kill 演练记录表之后的 ops-log 中。

护栏演练归 hft-core 的测试规格:CORE-210–CORE-217(第 7 步的 M3 标定演练是 CORE-216,[hft-core TESTS](../../apps/hft-core/TESTS.md))。

### Q. 链上(optional)

只在 chain-core 启用(cdex 迁到新框架)后适用:链上异常先 `/kill chain`(chain-core `Pause{all}`);钱包 `Tainted` 按 [bsc-node 执行钱包运维](bsc-node.md#执行钱包运维) 处理;链上 tx 的 `Unknown` 只由回执证据收敛(终局块号 > `maxBlock` 且 nonce 未被本交易消耗才是 `Dropped`),Unknown 期间绝不在 CEX 做反向对冲([chain-core](../../apps/chain-core/README.md))。

### R. 老板紧急停止(`/stop`)之后

老板(Viewer)用 `/stop`(全部账户)或 `/stop <a>`(一个账户)发出的是**非 HARD、不带 flatten** 的 `Kill{Global | Account(a)}`:禁止新开仓、撤销挂单;已成交第一腿的对冲、平仓、MarginStress 减仓与 Broken 自动平仓等风险递减单照常放行(`RISK_REDUCING`);**持仓保留**。确认绑定同一命令的预览,预览后 60 s 内由发起人本人确认有效,不比对状态版本——事故中状态频繁变化也不会让确认失效([ops_bot 紧急停止](../../python/ops_bot/README.md#紧急停止stop))。交易员的 `/stop` 是同一个 Kill,立即发出。ops_bot 按结果走以下几条路之一,并推送给全部 Trader/Admin:

| 情形 | ops_bot 做了什么 | 级别 | 本节步骤 |
|---|---|---|---|
| **R1 正常生效**:`CmdResult` 在 10 s 内 ok | 经 ctl-gateway 发出 Kill | P1 | 1–5 |
| **R2 兜底 `stop_core`**:ctl socket 持续 10 s 不可连,或 10 s 无结果且心跳已过期;ops_bot 视图显示停引擎不会中断风险处置(条件归 [ops_bot 紧急停止](../../python/ops_bot/README.md#紧急停止stop)第 6 步;或会中断,但随后有人 `/stop … core` 确认) | 先记待补 Kill(`/var/lib/hft/ops_bot/pending_kill.json`),再请 kill-switch 写停机标记、撤该范围内 Live 账户的全部挂单并经 polkit `systemctl stop hft-core@prod`(Drain,不平仓;该实例全部账户停止交易),同时 P0 | P0 | 6 |
| **R2b 兜底只撤单**:同样不可达,但停引擎会中断风险处置(或视图停滞) | 先记待补 Kill,再请 kill-switch 只撤单(`cancel`);**hft-core 不停**(L3 减仓、对冲与补腿继续),P0 中提示可用 `/stop … core` 再确认停引擎 | P0 | 8 |
| **R3 失败**:Kill 被拒,或 10 s 内没有 `CmdResult` 而 ctl 仍可连、心跳新鲜 | P0 告警值班交易员,附失败原因 | P0 | 7 |

1. 值班交易员 `/ack`,联系老板问清原因(看到了什么、哪个账户),写进 ops-log。
2. `[只读]` `/status`、`/accounts`:Kill cap 的范围与 issuer(`tg:<老板>`)、各账户有效模式、挂单已撤、风险递减单是否在执行;`/p`、`/risk` 看持仓与保证金。
3. 按老板描述的异常跳到对应处置节(A–O);需要更强的动作时由交易员升级:`/kill account <a> flatten`(平仓,并置清仓锁)或 `/kill account <a> hard`(连风险递减单也停)。
4. 恢复只能由交易员做:原因查清、与老板确认后 `/resume kill <a>`(或全局范围;Trader + 确认),核对有效模式恢复、无意外的 cap 残留。不恢复时在 ops-log 写明保持停止的理由与复查时间。
5. 老板误触:同样走第 2 步核对后恢复;一周内两次以上误触 → 复核 ops_bot 的预览文案。
6. **R2(已经 `stop_core`)**:
   1. `[只读]` 推送里的 kill-switch 回执:各目标挂单是否核验为 0、`hft-core@prod` 是否已停止;`systemctl status hft-core@prod` 应为 inactive。回执表明 hft-core 未能停止(polkit 拒绝、Drain 超时后仍在):kill-switch 已把停机标记作废(改名为 `/var/lib/hft/kill-switch/stop-marker.json.aborted`)并 P0(规则归 [kill-switch](../../apps/kill-switch/README.md))——先确认该目录下已没有 `stop-marker.json`;是否另发 Kill(ctl 可连时 `/kill …`,否则按 G)、是否 `[需授权]` 手动 `systemctl stop hft-core@prod` 由人决定,手动停止后下次启动**不会**自动带 Kill cap,需要时在启动后立即发 Kill;仍停不下按 E 处理。`kill-switch status` 与交易所网页核对挂单已清零;持仓与未对冲量按 A 处理(hft-core 停止期间引擎不对冲,交易所死手照常)。演练:DRL-97。
   2. 找 ctl-gateway 不可达的原因(`journalctl -u hft-core@prod`、`Note{StartupStep}`、ctl socket 是否存在),按 D 处理。
   3. `[需授权]` 修复后启动 `hft-core@prod`:kill-switch 在停机前写了停机标记 `/var/lib/hft/kill-switch/stop-marker.json`,hft-core 启动时读到它,以一条 Input 写入同一范围的 Kill(非 HARD、不带 flatten,`issuer = cli:systemd`)并把标记改名 `.consumed`(规则归 [hft-core 启动顺序](../../apps/hft-core/README.md#启动顺序) 与 [kill-switch](../../apps/kill-switch/README.md))——**范围内的账户带 Kill cap 起步,不会自动恢复开仓**,必须由交易员按第 4 步 `/resume kill`。`/status` 核对 Kill cap 在、标记已改名。ctl 可达后 ops_bot 另外**先补发**待补 Kill(第二道保证,与已有的 Kill 合并;推送"待补 Kill 已生效",P1),补发完成前本地拒绝 `/resume`。补发被拒 → P0,看 `detail`。
   4. 范围外的账户(`/stop a` 时的其他账户)只有冷启动的 `Recovery` cap:干净 Drain 重启在闸门放行后自动解除,否则 `/resume recovery <a>`;范围内的账户按第 4 步恢复。
7. **R3(`/stop` 失败或超时)**:值班交易员立即用自己的账号重发降级:`/kill`(或 `/kill account <a>`,Trader 不需要确认);仍无结果 → 按 G(`kill-switch fire`,必要时 `systemctl stop hft-core@prod`)。事后在 postmortem 里写明失败原因(gateway 拒绝的 `detail`、超时时 hft-core 的状态)。
8. **R2b(只撤单,引擎仍在运行)**:
   1. `[只读]` kill-switch 回执:挂单已清零;P0 中列出的原因(哪个账户 MarginStress ≥ L2,哪些 group 是 `Broken`、有在途链接、清仓未完成、外来成交在计时或自动平仓在执行,或视图停滞)。
   2. 引擎仍在执行 L3 减仓、对冲与补腿:先按 M、A 处理这些情形;控制面恢复后按第 4 步恢复。控制面持续不可达、而引擎被判断为失控(下了不该下的单、或心跳停滞)→ 由交易员或老板发 `/stop [<a>] core`,预览并本人确认后经 kill-switch `stop_core` 停止(之后按第 6 步);停引擎会让 L3 与对冲中断,决定写进 ops-log。

演练:DRL-92–DRL-97。

### S. 交易主机失联

触发:机外看门狗(dev2 上的 `hft-offbox-watchdog`,[deploy 机外看门狗](../../deploy/README.md#机外看门狗))报升级主机的 `/healthz` 连续 60 s 不可达或不健康;或 ops_bot、watchtower 同时无响应;或 AWS 报告实例/AZ 故障。此时 watchtower、ops_bot 与 kill-switch 可能与 hft-core 一起消失:没有引擎撤单、对冲与保证金管理,只有交易所侧死手(部分 venue 没有)。

**授权表**(负责人维护;名单变化同一天更新本表):

| 角色 | 人 | 持有 | 可以做 |
|---|---|---|---|
| 网页兜底人 A | 待负责人指定 | 各所主账户与全部正式子账户的网页登录、划转与提现的二次验证 | 网页撤单、减仓(reduce-only)、主子账户间划转 |
| 网页兜底人 B | 待负责人指定 | 同上 | 同上 |
| 授权人 | 负责人或老板 | — | 授权 `[需授权]` 步骤(重建主机、换 AZ、恢复交易) |

1. **确认**(5 分钟内):`[只读]` 从 dev2 `curl` 升级主机的 `/healthz`;AWS 控制台看实例状态与 AZ 健康;尝试 SSH,再尝试 SSM 会话(破窗通道,[deploy AMI](../../deploy/README.md#ami-构建))。能经 SSH/SSM 登录且只是 hft 进程异常 → 按 D/E/G 处理,不走本节后续步骤。
2. **网页兜底**(无法登录,或登录后 hft-core 起不来且持仓有风险;由网页兜底人执行,`[需授权]`):
   1. 每个正式子账户在各所网页**撤销全部挂单**;核对有死手的 venue 已自动撤单。
   2. 看持仓与保证金(各所网页的强平价与保证金率):行情朝永续持仓不利方向走、强平距离接近 L2 阈值([hft-risk](../../crates/hft-risk/README.md))→ 永续 reduce-only 减仓到安全距离,必要时把另一所的闲置资金划到该所合约钱包(主账户操作)。现货腿暂不动,除非价格朝其不利方向走且另一腿已减。
   3. 每一笔网页操作在 ops-log 记下所、子账户、订单号/划转号、时刻;这些都是之后 B2 的输入。
3. **重建主机** `[需授权]`(失联超过 30 min 或 AWS 确认实例不可恢复):
   1. 用最近一个通过验证的 AMI 在同一 AZ/CPG 启动新实例,把原 ENI-B、ENI-C 摘下按清单的设备序号挂到新实例;AZ 故障时在第二 AZ 启动,挂上**预建的待命 ENI-B/ENI-C**,把 prod 与 sim 的 EIP 重新关联到待命网卡的私有 IP,`/etc/hft/host-local.toml` 写 `manifest = "<当前清单名>.standby"`(M8 起为 `tokyo-cex-1-live.standby`)(待命清单与它的 `host_expect` 随每个正式版发布,[deploy 主机清单](../../deploy/README.md#主机清单)),之后按 [az-probe](az-probe.md) 复测——**prod 的 EIP 必须保持不变**,否则全部 key 的 IP 白名单失效;实例角色、安全组(含机外看门狗的 `healthz_port` 放行)同原主机。
   2. 冷副本卷:同 AZ 时从故障实例摘下挂到新实例;不能摘时新建卷,以 `journal-archiver restore` 从 S3 取回最近已提交的小时与 DurableMeta,热段视为丢失;恢复代、epoch 安全间隔与对象键见 [journal-archiver 从 S3 恢复](../../apps/journal-archiver/README.md#从-s3-恢复restore)。
   3. 按 [release-and-deploy](release-and-deploy.md) 安装并激活当前正式版(冷;新主机还没有 `current-core`,按该页的首次安装引导);`preflight.sh` 与 `hostprobe` 通过。
   4. `[需授权]` **改看门狗目标**:新实例 ENI-A 的私有 IP 已变;在 dev2(与启用时的第三 AZ 看门狗)上编辑主机本地文件 `/etc/hft/offbox-watchdog.toml` 的目标 URL,`systemctl restart hft-offbox-watchdog`;安全组的 `healthz_port` 放行对新实例生效。期望看门狗日志出现对新地址的成功探测。
   5. 以 `hft-core --recover-from-snapshot <latest> --reason "<文本>"` 一次性启动 prod(从 S3 恢复时 epoch 另加安全间隔,避免与故障主机最后一段运行的客户端订单号撞号;按 `tail_lost` 处理:全部非终态订单 `Unknown`、每账户 `Recovery`、`RECONCILED` 严格,规则见 [hft-core 崩溃隔离与从快照恢复](../../apps/hft-core/README.md#崩溃隔离与从快照恢复)),成功启动后移除该参数。
   6. 快照之后的仓位与余额变化(含第 2 步的网页操作)以对账差异出现,按 B、B2、C 逐项 `AdoptExchange`/`SetBalanceBaseline`、收敛 `Unknown`;连续两轮 `Ignore` 后由交易员 `/resume recovery <a>`。sim 实例最后再启动。
4. 恢复后:机外看门狗收到对新地址的恢复通知;写 postmortem(必写),记录失联时长、网页操作清单与重建耗时。

演练:换 AZ 恢复 DRL-60(M7,测试网账户);机外看门狗与授权表 DRL-26–DRL-29。

## kill 演练

每月一次(DRL-100),上线门槛 ⑪ 要求一次([testing §8](../testing.md#8-上线门槛));launch 的 A 阶段在 dev2、C 阶段在升级主机上各做一次(DRL-25)。本节是 kill 演练范围的唯一归属;kill-switch 侧的用例与验收在 [kill-switch 测试与验收](../../apps/kill-switch/README.md#测试与验收)。

- **完整演练只用演练账户** = 交易所测试网/模拟盘账户(在 `order_env = "testnet"` 的 `prod` 实例上)。正式实盘开始后,升级主机上的 `prod` 只持正式账户,每月的完整演练改在 dev2 的测试网联调窗口里做(同一发布版本)。
- **正式账户只做零成交撤单冒烟**:每月一次,在维护窗口内、该实例先 `/mode … closeonly`,按 [launch D-smoke](launch.md#d-正式实盘m8)(D 第 30 步)的第 1、3、4 小步由 Admin 发 `SmokeOrder` 冒烟单(`/smoke`;OMS 自己的槽,不触发 `Recon`)、用 `kill-switch fire` 撤掉、核验为 0、对账保持 `Ignore`,再恢复原模式;另做 `kill-switch selftest` 与 `kill-switch dry-run`。**不对正式实例 SIGSTOP、SIGKILL**,不做 flatten 与 HARD 演练。用例:DRL-99。
- 模拟账户的 kill 与实盘一样经检查链生效,可以用来演练 ops_bot 流程,但模拟账户不经 gw,覆盖不到 gw 的 kill 字与 kill-switch。

完整演练步骤(演练账户):

1. 演练账户挂 1–2 笔远离市价的小单,并持有一笔最小名义的小仓位。
2. `/kill account <演练> flatten`(Trader + 确认):期望 `CmdResult` ok;journal 有 `Note{CapSet}`(Kill,该账户);挂单被撤、平仓单按风控执行(风险递减单放行);**其他账户不受影响**(`/accounts` 核对有效模式与挂单)。(DRL-88)
3. kill 期间对演练账户发一笔开仓意图(或等策略触发):被风控以 `KILLED` 拒绝、不出环;`/status` 显示该账户在各 gw 的 kill 位为 1(gw 以 `KillBlocked` 拒绝绕过风控的单,是第二道防线,规则归 [hft-venue](../../crates/hft-venue/README.md))。(DRL-89)
4. 恢复:`/resume kill <演练>`(Trader + 确认)。核对:① `CmdResult` 列出解除后仍生效的 cap(应无 Kill);② journal 有 `Note{CapCleared}`;③ **kill 字已清**——`/status` 显示该账户在各 gw 的 kill 位为 0;④ 期望模式未被改动;⑤ 解除本身没有产生任何 HedgeFix 意图。(DRL-90)
5. 按账户 HARD:重复 1–4,第 2 步改为 `/kill account <演练> hard`,核对第 2 步中平仓单**也被拦**、撤单照常、其他账户不受影响。(DRL-91)
6. **老板紧急停止**:老板以 Viewer 身份 `/stop <演练>` → 预览显示"紧急停止"、范围与"不平仓"提示 → 本人确认。核对:Kill cap 生效、挂单被撤、持仓未动、全部 Trader 收到推送;预览后另一人发了 `/mode` 等改变状态的命令,老板的确认仍有效;超过 60 s 才确认 → ops_bot 提示预览过期、需重新 `/stop`;Viewer 发 `/resume kill <演练>`、`/kill … flatten`、`/kill … hard` → 均被拒并记审计;由交易员按第 4 步恢复。(DRL-92、DRL-93)
7. **`stop_core` 兜底**:演练账户挂 1–2 笔远离市价的小单后,让 ctl 不可达(把演练用 ops_bot 配置中的 ctl socket 路径指向不存在的文件);老板 `/stop <演练>` 并确认。核对:约 10 s 后 ops_bot 写出 `pending_kill.json`、经 kill-switch 本地触发套接字发 `stop_core`(kill-switch 审计行的调用方 uid = ops_bot)、`/var/lib/hft/kill-switch/stop-marker.json` 写出、挂单在 kill-switch 核验时限内清零、`hft-core@prod` 经 polkit 停止(Drain 快照)、值班交易员收到 P0。随后恢复 socket 路径并 `[需授权]` 启动 `hft-core@prod`:演练账户带 Kill cap 起步、标记改名 `.consumed`;ops_bot 补发 Kill(P1 推送)、补发前 `/resume` 被本地拒绝、补发后交易员按第 4 步 `/resume kill` 恢复(DRL-94)。再做两个变体:"进程停摆"——`kill -STOP <演练实例 hft-core 的 pid>` → 心跳过期且 10 s 无结果 → 视图停滞,只撤单(R2b)、P0 提示 `/stop … core`;老板 `/stop <演练> core` 并确认 → `stop_core`(systemd 在 `TimeoutStopSec` 后终止被冻结的进程)(DRL-95)。"清仓未完成"——先 `/kill account <演练> flatten` 再私聊 `/kill account <演练> hard`(HARD 挡住清仓单,清仓目标不能到达),同样让 ctl 不可达 → 只撤单、hft-core 不停(DRL-96;在途链接、Broken 与 MarginStress ≥ L2 等其余触发条件无法在测试网上按计划制造,由 ops_bot 的用例覆盖)。停机失败(polkit 拒绝)时停机标记作废的处置按 R 第 6.1 步演练(DRL-97)。
8. 确认审计完整:ops_bot 审计表 + journal `ControlCmd`/`CmdResult`/审计 Note(含 `view_account`、`chat_type`)+ kill-switch 的触发记录。(DRL-98)

### kill 演练记录

| 日期 | 主机/实例/账户(测试网或正式) | 场景(SIGSTOP / SIGKILL / 干净停止 / ops_bot kill / 按账户 HARD / 老板 `/stop` / `stop_core` 兜底 / 正式账户零成交冒烟) | T0(UTC) | 各所挂单清零时间 | 结论 | 执行人 |
|---|---|---|---|---|---|---|
| | | | | | | |

## 记录

每次 P0/P1 在 ops-log 写时间线;P0 写 [postmortem](../postmortem/README.md),包括根因、为何现有防线没拦住、对 runbook/规则/代码的修正(同一天修正本页)。
