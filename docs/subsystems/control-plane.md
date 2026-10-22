# 控制面

冷进程影响 hft-core 的**唯一**方式是控制命令:经 ctl-gateway 鉴权后写入 ctl 环,sequencer 把命令作为 `ControlCmd` 事件写进 journal 后执行,并写一条 `CmdResult`——命令与结果都可回放(kill-switch 的停机标记也不例外:hft-core 启动时把它转成一条 `Kill` 命令记进 journal,见[版本与确认](#版本与确认))([09 §5](../../design/09-可观测性、风控与控制面.md))。本页是**命令集语义、参数、类别、角色定义、账户语义、版本与二次确认协议、幂等、过期、结果码,以及模式与上限统一优先级表的唯一定义**。角色 × 命令 × 参数的**权限矩阵**由代码生成,唯一归属是 [hft-ctl-proto 权限表](../../crates/hft-ctl-proto/README.md#权限表),本页不重写;帧与 JSON 模式也在 hft-ctl-proto;记录体布局在 [hft-event](../../crates/hft-event/README.md);sequencer 侧的执行顺序在 [hft-sequencer](../../crates/hft-sequencer/README.md);模式状态(ModeTable、cap、有效模式、启动闸门、`ctl_state_version`、kill 投影)与每个 cap 来源的定义在 [hft-risk](../../crates/hft-risk/README.md)。状态查询不是命令:冷进程读 journal/Parquet(经 [hft-py](../../crates/hft-py/README.md))。**新增策略不需要新命令**:参数经通用的 `SetParams` 携带并按策略自带的 schema 校验,模式经通用的 `SetMode`,策略专属的人工操作经通用的 `StrategyCmd`,冷启动预热数据经通用的 `InjectWarmup`(后两者的载荷都由策略定义)。

## 模式与上限的统一优先级

下表手写,CI 与 hft-risk `mode` 的代码比对。

**期望模式**(`ModeTable`,每 cell = (strategy, account, group) 一格)只由 `SetMode` 写。kill、自动降级、恢复锁存、条件门都是**独立的 cap 记录** `(source, scope, set_seq, params)`,互不覆盖。**有效模式** = 期望为 Shadow 时取 Shadow(有仓位时推迟,见 `SetMode`);否则取 min(期望, 全部生效 cap)。OMS/执行层发出的 `RISK_REDUCING` 单只受 HARD 约束。`SetMode` 不清任何 cap;`Resume{source, scope}` 只删 (source, scope) 完全匹配的一条;kill 字由剩余 cap 重新计算。每个 cap 来源的作用域、设置条件与解除前置条件归 [hft-risk](../../crates/hft-risk/README.md);本表只定优先级、挡什么与解除命令。

| 层 | 状态(作用域) | 挡住 | 仍放行 | 设置者 | 解除 |
|---|---|---|---|---|---|
| 0 | HARD(全局,或按账户) | 作用域内全部 Place/Amend(含 `RISK_REDUCING`)、optional 链上的全部新意图 | 撤单、查询、死手刷新 | `Kill{Global, hard}`、`Kill{Account(a), hard}` | 同级同目标 `Resume{Kill}` + 确认,且满足 hft-risk 的 HARD 解除条件(kill 之后的新一轮对账全部 `Ignore`,或每项差异已 `AdoptExchange`;资金费条目不计);解除时不自动补腿:裸量超过补腿阈值的 group 判为 Broken(`resolve_broken`,判定归 [hft-exec](../../crates/hft-exec/README.md),调用点归 hft-sequencer),由 `HedgeFix` 处理,持续 `broken_auto_fix_ms` 无人处理时执行层做一次有界平仓(计时规则归 hft-exec) |
| 1 | `Draining`(进程) | 升级命令(带 `flatten` 的 `Kill` 除外)、Open | `RISK_REDUCING` | `Drain` / `ReplaceRefdata` / SIGTERM / `Storage` 超 Drain 阈值(存在未到达的 `reason ∈ {MarginStress, Hard}` 引擎目标时推迟,与 `exec.drain_ready()` 同口径,规则归 [hft-sequencer](../../crates/hft-sequencer/README.md)) | 不可清除,随进程退出 |
| 2 | `ClockBad`(全局,条件型) | Open(策略意图与升级命令) | Reduce / Flatten / Hedge 与硬动作 | `Timer{ClockBad}` | `Timer{ClockOk}` 自动解除 |
| 3 | `Kill`:Global / Account / Venue(× 账户) | Open 及无 `RISK_REDUCING` 的单;按默认参数撤挂单 | Reduce / Flatten / Hedge | `Kill` 命令(含 Viewer 的紧急停止、启动时由 kill-switch 停机标记触发的 `Kill`,见[版本与确认](#版本与确认)) | 同级同目标 `Resume{Kill}` + 确认(Trader/Admin) |
| 4 | 锁存 CloseOnly:`DailyLoss`(账户)、`Recon`(账户, venue)、`UnknownTimeout`(账户, venue)、`Broken`(账户, group)、`HedgeFailure`(账户)、`Delisting`(venue, inst)、`Recovery`(账户)、`Halted`(策略, 账户, group)、`Reorg`(账户, group;optional 链上腿) | Open | Reduce / Flatten / Hedge | sequencer 自动;`Delisting` 由 `VenueNotice{Delisting}` | 各自 `Resume{source}` + 确认 + hft-risk 的前置条件(如 `HedgeFailure`:该账户已无 `Broken`,否则 `CmdResult{Rejected, detail = BrokenRemaining}`) |
| 5 | 条件型 cap(不锁存,条件消失自动解除,不能被 `Resume`):`MarginStress`(账户;按各 venue 永续持仓到强平价的距离 `liq_dist` 分档:L1 禁开,L2 通知策略限期减仓,L3 引擎强制减仓;度量、阈值与滞回归 hft-risk)、`Storage`(进程;禁开)、`AccountCfg`(账户, venue);条件门:启动闸门未放行(只挡升级命令,带 `flatten` 的 `Kill` 除外)、`UNKNOWN_PENDING`、`EXPOSURE_GATE`、`VENUE_NOT_READY`、`STALE/NO_MID`、optional 链会话 Down | Open / 升级命令 | 其余(L2/L3 的减仓动作本身是 `RISK_REDUCING`) | 由输入推导:`MarginReport`、`Timer{PersistLag\|DiskLow}`、`AccountConfigReport`… | 条件消失自动解除(`MarginStress` 的档位与滞回、`Storage` 的阈值归 hft-risk) |
| 6 | 期望模式(ModeTable):Shadow / CloseOnly / Live | Shadow:策略意图风控评估后只记录(结果归 SDK 行为表);CloseOnly:Open | — | `SetMode` | `SetMode` |

- `RunMode` 只有 **Shadow / CloseOnly / Live**;Live 永远遵守 `GroupSpec.open_allowed`。**模拟不是模式,是账户类型**:模拟账户上的 cell 按同一张表运行,cap 与 kill 同样生效(经检查链;模拟账户的单不经 gw 的 kill 字)。
- 有效模式每次变化都向该 cell 的策略投递 Mode 事件;降到 CloseOnly / Shadow 时撤销该 cell 的 Open 类挂单。
- 没有策略级 kill:停一个策略实例用 `SetMode`。
- 带 `cancel_resting` 的 `Kill` 与 `Drain{cancel_resting = true}` 生效期间,撤单是持续义务:每次连接恢复或 Unknown 收敛为 Live 都重发;`Drain{cancel_resting = false}` 不撤挂单,也不产生这一义务。
- 干净 Drain 重启在闸门放行后可自动解除 `Recovery`(`auto_resume_after_clean_drain`);出现 `tail_lost` 或恢复不一致时不自动解除,启动闸门 `RECONCILED` 也按严格条件判定——条件归 [hft-risk](../../crates/hft-risk/README.md)(含只含 Sim 账户的实例在非干净重启后的自动解除)。

## 通用规则

策略参数的账户相关下界由 [SDK 参数 schema](../../crates/hft-strategy-sdk/README.md#schema-json-与-define_params) 的 prod_min 表达,离线编码与 SetParams validate 都检查正式账户;校验失败不替换生效参数。

### 信封

每条命令带:`cmd_id`(UUID,客户端生成,重试复用)、`issuer`(`tg:<from.id>`、`ops_bot`、`pipeline`、`reconciler`、`cli:<login>`、`cli:systemd`;watchtower 与 treasury 不发命令)、`issued_at_ns`、`expires_at_ns`(必填,≤ 签发后 300 s,推荐 30 s)、`account`、命令体与目标 `CmdScope`。gateway 写环时把信封填进 [hft-event](../../crates/hft-event/README.md#控制命令编码) 的 `CmdHdr`,并加上 `expires_tsc`、`peer_uid`、有效 `role`、`preview` 标志、`confirmed`、`expect_state_version`、`cmd_hash`(规范化命令的 SHA-256,**覆盖 `CmdHdr.account` 与完整载荷**)与 `base_version`;gateway 不复制任何 sequencer 状态。审计 `Note{CtlAudit}` 另记 `view_account`(bot 当时的默认账户视图)与 `chat_type`。

### 角色

| 角色 | 是谁 | 能做什么 |
|---|---|---|
| `Admin` | 引擎维护者 | Trader 的全部,加上 `SetRisk`、`ReplaceRefdata`(仅 `cli:<login>`)、`SmokeOrder`、`WriteOffResidual`、`SimFault`(仅 `cli:<login>`)与 optional 的连接维护命令 |
| `Trader` | 交易运维 | 模式、kill、解除、参数(仅 `cli:<login>`)、补腿、采纳、基线、交易所公告、模拟账户、划转登记、策略命令、预热注入、备注 |
| `Viewer` | 只读观察者;**老板 = Viewer** | ops_bot 面板(直接读 journal/Parquet)与 `GetSchema`;**唯一的写权限是紧急停止**:非 HARD、不带 flatten、`cancel_resting` 取默认值的 `Kill{Global}` 或 `Kill{Account(a)}`——禁止新开仓并撤销挂单,已成交第一腿的对冲与平仓照常(`RISK_REDUCING` 放行);必须经 [Viewer 紧急停止的确认](#版本与确认)(同一 issuer 的预览 `cmd_hash` + 60 s TTL,不比对 `ctl_state_version`);**不能 `Resume`**,不能发其他改变状态的命令;无状态效果的 `Annotate{OpsNote \| LocalWrite}`(ops_bot 代其记录本地写与兜底停止)除外;审计照常 |

进程主体(`ops_bot` 自身、pipeline、reconciler、`cli:systemd`)不是角色,权限按主体逐命令列出。逐命令的允许矩阵、群聊限制与主体表见 [hft-ctl-proto 权限表](../../crates/hft-ctl-proto/README.md#权限表);鉴权流程见 [hft-ctl](../../crates/hft-ctl/README.md)。

ctl 的角色只以 gateway 的签发者表为准:文件 `/etc/hft/ctl-principals.toml`(hft-core 配置 `paths.ctl_principals`,只在启动时加载、改动需重启 hft-core,deploy 生成;[hft-core 配置文件字段](../../apps/hft-core/README.md#配置文件字段)),格式与加载见 [hft-ctl 鉴权](../../crates/hft-ctl/README.md#鉴权);GitHub 侧的 Admin 名单 `.github/hft-admins.txt`(`skip_nightly`、`skip_verify` 与 CI 校验的签字字段用它,记 GitHub 登录名,归 [development](../development.md))不参与 ctl 鉴权。两份名单的人员由负责人维护一致(launch 检查清单核对一次)。

### 账户语义

1. **降级类**命令不带账户时作用于**全部账户**(`account = *`)。
2. **升级与中性**命令必须显式写 `account`(账户名,或第 9 条允许时写 `*`);缺省即拒(`InvalidArgs{AccountRequired}`)。bot 的默认账户视图只影响显示,不会被填进命令。
3. gateway 要求 `CmdHdr.account` 与载荷中的账户(`CmdScope.account` 或命令自带的 `account` 字段)一致,否则 `InvalidArgs`。
4. `account = *` 的升级命令要求全部目标账户的启动闸门已放行,否则整体拒绝(`PreconditionFailed{GateNotReleased}`)。
5. 确认预览第一行显示目标账户标签、账户类型(实盘/模拟)与对照账户当前的有效模式。
6. **账户绑定**:一个账户只绑定一个策略实例(refdata `AccountMeta.bound_instance`);带 `strategy` 的命令,其账户必须绑定到该实例(`InvalidArgs{AccountBound}`)。
7. **实例级命令**:`InjectWarmup` 只作用于策略实例的模型,不作用于任何账户;它的 `account` 必须是 `*`(编码 0),给出具体账户 → `InvalidArgs{AccountNotAllowed}`。这是第 2 条的唯一例外。
8. **group 的编码**:作用域里的 `group = *` 编码为 `GROUP_ALL`(`u32::MAX`),`CmdScope.group = GROUP_NONE`(0)→ `InvalidArgs`;`GROUP_NONE` 只出现在 `StrategyCmd.group`(实例级载荷)与 `AdoptExchange.group`(unassigned)([hft-types](../../crates/hft-types/README.md#id-与枚举))。具体的 group 必须符合构造规则(高 8 位是存在的策略实例、计数器在范围内),同一命令给出具体 `strategy` 时还须等于该 group 的高 8 位(`strategy` 为 `*` 或省略时编码为 0,由 group 指明实例);`HedgeFix`、`WriteOffResidual` 的 group 必须具体;否则 `InvalidArgs{BadGroup}`。
9. **逐命令的账户取值**(`normalize` 执行;"具体"= 一个账户名):
   - 必须是具体账户(缺省或 `*` → `InvalidArgs{AccountRequired}`):`SetParams`、`HedgeFix`、`WriteOffResidual`、`SmokeOrder`、`AdoptExchange`、`SetBalanceBaseline`、`SimReset`、`SimTransfer`、`SimFault`、`RecordTransfer`、`StrategyCmd`、`level = Account` 的 `Kill`;
   - 只能是 `*`(给出具体账户 → `InvalidArgs{AccountNotAllowed}`):`InjectWarmup`、`Drain`、`VenueNotice`、`BindSlot`、`Settle`、`ReconcileReport`、`level = Global` 的 `Kill`;其中中性的 `VenueNotice{FundingInterval | Maintenance}`、`BindSlot` 与 `Settle` 按第 2 条须显式写 `"*"`;
   - 名字或 `*` 均可(缺省按第 1、2 条):`SetMode`、`SetRisk`、`Resume`、`CancelAll`、`ReplaceRefdata`、`Annotate`、`level = Venue` 的 `Kill`(另须具体 `venue`,缺省或 `*` → `InvalidArgs{BadCombination}`)。

### 类别

| 类别 | 定义 | 规则 |
|---|---|---|
| **降级** | 只会减少风险敞口或停止行为:默认参数的 `Kill`(含 `hard`)、`SetMode → CloseOnly`、`SetMode → Shadow`、`CancelAll`、`Drain`、收紧的 `SetRisk`、`VenueNotice{Delisting}` | **永远允许**(对有权限的角色);不限速;不需确认(唯一例外:`CmdHdr.role = Viewer` 的 `Kill` 必须确认);任何状态下都执行(包括 DRAINING、HARD) |
| **升级** | 可能增加风险敞口或解除保护:`SetMode → Live`、`Resume`、`AdoptExchange`、`SetBalanceBaseline`、`WriteOffResidual`、`SmokeOrder`、放宽的 `SetRisk`、判定为升级的 `SetParams`(见下)、带 `flatten` 的 `Kill`、未声明 `reduce_only` 的 `StrategyCmd`(引擎不解释载荷、无法区分开与平,按最严类别处理;声明 `reduce_only` 的为中性,见命令表) | **二次确认**;启动闸门未放行或 DRAINING 期间 → `PreconditionFailed`。**唯一例外**:带 `flatten` 的 `Kill` 在启动闸门未放行与 DRAINING 期间照常执行(仍需确认)——它只减仓,恢复期正需要它 |
| **中性** | 不改变风险上限的维护或减仓操作:pipeline 主体的 `SetParams`、`ReplaceRefdata`、`HedgeFix`、`RecordTransfer`、`Annotate`、`SimReset`、`SimTransfer`、`SimFault`、`VenueNotice{FundingInterval \| Maintenance}`、`BindSlot`、`Settle`、`InjectWarmup`、optional 的 `RotateConn`/`Resubscribe`/`SnapshotNow` | `ReplaceRefdata`、`SimReset` 需确认 |
| **只读** | `GetSchema` | 不进 ctl 环,只审计 |

`SetMode` 的类别按**目标值**静态判定:`→ Live` = 升级;`→ CloseOnly`、`→ Shadow` = 降级。gateway 只用静态类别做角色上限;"放宽/收紧"与是否需要确认由 **sequencer** 按当前状态判定:

- `SetRisk` 按账户逐字段:任一上限变大、阈值变小、开关由关变开即为放宽(升级,需确认);否则为收紧(降级)。
- `SetParams` 只按引擎理解的部分判定,body 字段的数值不参与(它们由 schema 的 `min/max` 与 `RiskParams` 限额约束):
  1. **结构字段**:目标 (strategy, account) 是 **Live 实例**——绑定到 `AccountMeta.kind = Live` 账户、且至少一个 cell 的期望模式(`ModeTable`)为 Live 的策略实例(本条是该定义的唯一归属;全部 cell 期望为 Shadow 或 CloseOnly 的实例、Sim 账户上的实例都不是 Live 实例)——且新参数改动了任何 `FieldDesc.structural = true` 的字段([hft-strategy-sdk](../../crates/hft-strategy-sdk/README.md))→ 升级,任何主体都需确认;预览的 `CmdPreview.reset_groups` 与 `no_open_ms` 给出受影响的 group 数与预计不能开仓的时长。pipeline 插件不拥有结构字段(pipeline 侧拒绝,规则归 [pipeline](../../python/pipeline/README.md));pipeline 主体不确认,它若发出这样的 blob 只会得到 `NeedsConfirm`。因此 Live 实例的结构字段只经 `hft-cfg` + `cli:<login>` 修改。
  2. **人工主体**(`cli:<login>`):任一 group 的 `open_allowed` 由 0 变 1、或新增 `open_allowed = 1` 的 group 即为放宽(升级,需确认);其余中性。
  3. **pipeline 主体**:除第 1 条外恒为中性、不需确认,对 Live 账户同样如此——包括新增 group 与把 `open_allowed` 由 0 变 1(上界由 `RiskParams` 约束,pipeline 不升级 `RunMode`;人工覆盖项按策略实例分开,模拟实例上的试验不会流到实盘实例,规则归 pipeline)。作用于 Live 实例的人工参数编辑经 ops_bot 时,确认在 ops_bot 侧完成(本地确认后写 pipeline 的该实例覆盖项,[ops_bot](../../python/ops_bot/README.md))。

### 版本与确认

- 协议版本 `v = 2`。
- `StrategyParams`(每 (strategy, account))、`RiskParams`、refdata 各自带单调递增的 `version`;低于或等于当前值 → `StaleVersion`;可选 `base_version` 不符 → `VersionConflict`。
- hft-risk `mode` 维护 **`ctl_state_version`**:每次 `SetMode`、`Kill`、`Resume` 与任何 cap 增删 +1,随每个 `CmdResult` 返回。
- **二次确认(唯一协议)**:第一步发带 `preview` 标志的同一命令,sequencer 不改任何风险或模式状态、回 `CmdResult{preview}`(含 `ctl_state_version`、目标账户类型与是否需要确认;`status` 是该命令此刻经确认执行会得到的码——前置条件不满足时为相应的拒绝码,否则 `Applied`,不出现 `NeedsConfirm`/`ConfirmInvalid`;`rflags.PREVIEW` 置位,需要确认时另置 `NEEDS_CONFIRM`,Viewer 的 `Kill` 另置 `PREVIEW_TTL`);第二步用新 `cmd_id`、同一命令体、带 `confirmed = true` 重发。第二步与预览的**绑定方式**按命令分两种,由 sequencer 判定:
  1. **一般情形**:第二步带 `expect_state_version = <第一步的 ctl_state_version>`;需要确认而未确认 → `NeedsConfirm`;版本已变 → `ConfirmInvalid{StateChanged}`(从第一步重来)。`confirmed = true` 而 `expect_state_version = 0`(`CmdHdr` 中表示不检查):gateway 回 `invalid_request`;绕过 gateway 直接到达 sequencer 的同样不执行,回 `ConfirmInvalid{StateChanged}`——0 只对第 2 种绑定有效。
  2. **Viewer 紧急停止**(`CmdHdr.role = Viewer` 的 `Kill`):**不比对 `ctl_state_version`**(事故中 cap 频繁变化,不能让紧急停止反复失效)。预览时 sequencer 登记一条预览记录 `{cmd_hash, issuer, preview_engine_ns}`(至多 16 条,满时淘汰最旧;处理每次进程启动的 `EngineClock` 时清空,即重启后旧预览一律失效;进快照节 15,见 [journal-format](journal-format.md#快照记录结构));第二步(`expect_state_version` 写 0、被忽略)只在存在 `cmd_hash` 相同、`issuer` 相同(同一 `tg:<from.id>`)且 `now.engine_ns − preview_engine_ns ≤ 60 s`(常量 `VIEWER_CONFIRM_TTL_MS = 60 000`)的记录时执行,并消耗该记录;否则 → `ConfirmInvalid{NoValidPreview}`(从第一步重来)。未预览直接发 → `NeedsConfirm`。`cmd_hash` 覆盖账户与作用域,因此"全部账户"的预览不能用来确认某个账户的停止,反之亦然。
  
  gateway 不签发、不保存任何令牌;没有 Prepare/Commit 两种命令。ops_bot 按钮、`hft-cfg send` 与客户端的 `preview()`/`confirm()` 都是这一协议的包装。gateway 对每个写请求的响应带该命令规范形的 `cmd_hash`([hft-ctl-proto `CmdResponse`](../../crates/hft-ctl-proto/README.md#请求与响应)),供客户端显示与核对预览;确认请求不携带它,gateway 对同一命令体重新算出同一值。
- 任何写命令都可以先发 `preview`(sequencer 不改状态,只回预览);不需要确认的命令(如带 `hard` 的 `Kill`)客户端是否先预览由客户端决定(ops_bot 的做法归 ops_bot),sequencer 对直接发出的同一命令照常执行。
- **需要确认的情形**(sequencer 判定):升级类命令;`ReplaceRefdata`、`SimReset`;`CmdHdr.role = Viewer` 的 `Kill`(紧急停止:预览显示目标账户或"全部账户"、受影响 cell 数与挂单数,以及"60 s 内由本人确认")。
- 每个 `CmdResult` 回带 `engine_epoch`、各配置版本与 `ctl_state_version`。
- **紧急停止的兜底**:ctl-gateway 不可达 10 s(hft-core 心跳仍在或已过期)时,ops_bot 不经本页命令,而是经 [kill-switch](../../apps/kill-switch/README.md) 的本地触发套接字发 `stop_core`:kill-switch 撤单(只撤单、不平仓;只接受 ops_bot 的 uid),**先写停机标记** `/var/lib/hft/kill-switch/stop-marker.json`(fsync),再经 polkit 只允许 `systemctl stop hft-core@prod`(SIGTERM → `Drain`,不平仓);停机失败时 kill-switch 把标记作废为 `stop-marker.json.aborted` 并报 P0,是否另发 Kill 由人决定。hft-core 下次启动在启动步 5 只读 `stop-marker.json`,以 `issuer = cli:systemd` 写一条 Input `ControlCmd{Kill{scope, 非 HARD, 不带 flatten}}`,再把标记改名为 `.consumed`,因此干净重启也不会自动恢复开仓;该 Kill cap 照常由交易员 `Resume{Kill}` 解除。停引擎会中断正在进行的风险处置时(条件的唯一清单归 ops_bot),ops_bot 的兜底只撤单并发 P0,停引擎需要第二次确认。紧急停止失败或 10 s 内无结果同样 P0。触发协议、标记格式与作废规则归 kill-switch,启动时读标记归 [apps/hft-core](../../apps/hft-core/README.md),待补 Kill、二次确认与告警归 [ops_bot](../../python/ops_bot/README.md),polkit 规则归 [deploy](../../deploy/README.md)。

### 幂等

- gateway 缓存最近 4096 个 `cmd_id` 的结果;sequencer 状态里也有最近 4096 个(进快照)。重复 → `Duplicate`,不执行。
- 命令按"目标状态"表达,已在目标状态 → `NoOp`。

### 过期

sequencer 以该 `ControlCmd` 记录头的 `tsc` 与 `expires_tsc` 比较:过期 → `CmdResult{Expired}` 且不执行。降级命令同样有过期时间,ops_bot 对降级命令使用 300 s 上限。

### 结果码

下表是结果码的唯一定义。sequencer 产生的码写进 `CmdResult.status`,`CmdResult.detail` 放命令相关细节(下方 detail 码表);gateway 产生的码只进审计 `Note{CtlAudit}`,直接回给客户端(JSON 里写名字)。

| 码 | 名称 | 产生者 | 含义 |
|---|---|---|---|
| 0 | `Applied` | sequencer / gateway(`GetSchema`;`ReconcileReport` 整组写入 ctl 环) | 已执行(`SetMode→Shadow` 有仓位时 `detail = ShadowDeferred`) |
| 1 | `NoOp` | sequencer | 已处于目标状态 |
| 2 | `Duplicate` | gateway / sequencer | 同一 `cmd_id` 已处理 |
| 3 | `Expired` | gateway / sequencer | 到达或处理时已过期 |
| 10 | `Unauthorized` | gateway | uid 或 issuer 未知 |
| 11 | `Forbidden` | gateway | 角色不允许该命令/该参数组合/该聊天类型(Viewer 除紧急停止与 `Annotate{OpsNote \| LocalWrite}` 以外的一切写命令) |
| 12 | `NeedsConfirm` | sequencer | 按当前状态需要确认而 `confirmed = 0`(含 Viewer 未经预览确认的 `Kill`) |
| 13 | `ConfirmInvalid` | sequencer | 确认未能绑定到预览(detail `StateChanged`、`NoValidPreview`) |
| 20 | `InvalidArgs` | gateway / sequencer | 参数校验失败(detail 见下表 100–199) |
| 21 | `UnknownTarget` | sequencer | 策略实例/group/venue/account/inst 不存在 |
| 22 | `VersionConflict` | gateway / sequencer | `base_version` 不符 |
| 23 | `StaleVersion` | sequencer | 版本不递增 |
| 24 | `CapacityExceeded` | sequencer | 超过启动时的预分配容量(如全部实例的 cell 合计超过 `max_cells_total`);单个实例的 group 数超过 `max_groups_per_instance` 是另一层,由 SDK 在 gateway 解码时以 `ParamError::TooManyGroups` 拒绝(`InvalidArgs{ParamInvalid}`,kind `Strategy`) |
| 25 | `PreconditionFailed` | sequencer | 前置条件当前不满足,条件满足后可重发(detail 见下表 300–399) |
| 26 | `RoleInsufficient` | sequencer | `CmdHdr.role` 不足(第二道检查) |
| 27 | `Rejected` | sequencer / gateway(只有 `RefdataAssembly`) | 对命令**结果**的复核不通过:命令不生效、状态不变(detail 见下表 400–499) |
| 30 | `RateLimited` | gateway | issuer 写命令超限 |
| 31 | `Busy` | gateway | ctl 环满、Mailbox 未取走,或同一 (实例, 账户) 上一条 `SetParams` 的结果尚未读到([hft-ctl](../../crates/hft-ctl/README.md)) |
| 40 | `Internal` | 任意 | 不应发生;附 `Note` |

**detail 码**(`CmdResult.detail: u32 = code | arg << 16`;code 全表唯一、只追加、永不复用,0 = 无;arg 只对注明的码有意义,其余为 0):

| code | 名称 | 所属结果码 | 含义与 arg |
|---|---|---|---|
| 1 | `ShadowDeferred` | `Applied` | `SetMode→Shadow` 有仓位的 cell 暂落 CloseOnly,归零后自动落到 Shadow |
| 100 | `UnknownDiscriminant` | `InvalidArgs` | 未知命令、`cmd_ver` 或枚举判别值(含旧读者读到新命令变体) |
| 101 | `AccountRequired` | `InvalidArgs` | 升级/中性命令缺账户 |
| 102 | `AccountNotAllowed` | `InvalidArgs` | 只能作用于全部账户的命令或级别给出了具体账户(清单见[账户语义](#账户语义)第 9 条) |
| 103 | `AccountMismatch` | `InvalidArgs` | 载荷中的账户 ≠ `CmdHdr.account` |
| 104 | `OutOfScope` | `InvalidArgs` | `SetRisk` 改了账户作用域外的字段 |
| 105 | `AccountBound` | `InvalidArgs` | 账户未绑定到该策略实例 |
| 106 | `KindMismatch` | `InvalidArgs` | 实例的 `kind_id` 与 blob 不符 |
| 107 | `ScopeMismatch` | `InvalidArgs` | JSON 的 strategy/account 与 blob 头不一致 |
| 108 | `SchemaUnsupported` | `InvalidArgs` | 未编译进来的 schema 版本 |
| 109 | `SchemaMismatch` | `InvalidArgs` | `schema_digest` 与编译进来的 schema 不一致 |
| 110 | `ParamInvalid` | `InvalidArgs` | 参数被拒;arg = `kind | field << 4`(kind 4 位;field 12 位 = 字段下标,`0xFFF` = 未指明,只对 kind `Strategy` 有意义):kind `Strategy=1`(策略 `decode`/`validate` 拒绝)、`ModelMemBudget=2`(该 hft-core 实例全部策略实例的 `Model::mem_bytes` 之和超过 `advisor.mem_budget_mib`,或 2 × Σ `Model::snapshot_bytes` 超过 `advisor.checkpoint_max_mib`;预算取值归 [apps/hft-core](../../apps/hft-core/README.md))、`NonProdOnly=3`(`StrategyDesc.non_prod_only` 的策略试图绑定 `is_prod = true` 的账户);SDK 自身的 `ParamError`(如 `TooManyGroups`)同样归 kind `Strategy` |
| 111 | `GroupIdDup` | `InvalidArgs` | `GroupId` 在 blob 内或全局重复 |
| 112 | `BadGroup` | `InvalidArgs` | `GroupId` 不符合构造规则(高 8 位 ≠ 该 blob 的策略实例、计数器越界,因而也排除 `GROUP_NONE`/`GROUP_ALL`,[hft-types](../../crates/hft-types/README.md#id-与枚举))或 `GroupSpec` 结构非法 |
| 113 | `SameExchangeLegs` | `InvalidArgs` | 同一 `GroupSpec` 的两腿在同一交易所 |
| 114 | `LotMismatch` | `InvalidArgs` | 新增或指纹变化的 group 两条交易所腿的 lot 互不整除,1/1 对冲比下 [hft-types `pair_lot`](../../crates/hft-types/README.md#运算) 的名义 > `max_chunk_notional`(sequencer 通用校验,只查这两类 group,存量 group 不因价格变化使整条 `SetParams` 被拒;取整规则与名义所用价格归 [hft-exec 两腿 lot 对齐](../../crates/hft-exec/README.md#两腿-lot-对齐)) |
| 115 | `UnknownGroup` | `InvalidArgs` | group 不属于该 (实例, 账户) 的当前参数 |
| 116 | `TooLong` | `InvalidArgs` | 载荷超长 |
| 117 | `NotSim` | `InvalidArgs` | 命令只适用于模拟账户 |
| 118 | `NotLive` | `InvalidArgs` | 命令只适用于实盘账户 |
| 119 | `UnknownName` | `InvalidArgs` | 实例名、账户标签、inst 等名字无法解析 |
| 120 | `InstOwned` | `InvalidArgs` | 同一实例内某 (account, inst) 会被两个非 Shadow cell 引用 |
| 121 | `ExecRiskMismatch` | `InvalidArgs` | 任一账户的 `ExecConfig.force_slice.max_cross` 大于该账户可交易 inst 各价格带类别 `rr_max_cross` 的最小值,或 `force_slice.max_chunk_notional > RiskParams.max_order_notional`(判定归 [hft-risk](../../crates/hft-risk/README.md)) |
| 122 | `BadCombination` | `InvalidArgs` | 参数组合非法(`hard` 与 Venue、`flatten` 与 Global、`flatten` 与 `hard` 等;hft-risk `ModeErr::InvalidScope` 也映射到它) |
| 123 | `NotResumable` | `InvalidArgs` | `Resume` 的来源不可被命令清除(条件型来源与 `Draining`) |
| 124 | `GroupCheck` | `InvalidArgs` | refdata `check_group` 的其他失败;arg = [hft-refdata `LoadErrCode`](../../crates/hft-refdata/README.md#loaderrcode-码表) 的数值 |
| 125 | `BadValue` | `InvalidArgs` | 数值越界或不在允许集合内(如 `VenueNotice{FundingInterval}` 的周期、`SmokeOrder.ttl_ms`) |
| 126 | `DustBelowMinNotional` | `InvalidArgs` | 运行中 `SetParams` 新增或改腿的 group:该账户 `dust_notional` 低于新交易腿的最小名义(sequencer 发出;装配期同名校验见 [apps/hft-core](../../apps/hft-core/README.md)) |
| 200 | `StateChanged` | `ConfirmInvalid` | `expect_state_version` 与当前 `ctl_state_version` 不符 |
| 201 | `NoValidPreview` | `ConfirmInvalid` | Viewer 的 `Kill` 找不到同一 issuer、同一 `cmd_hash`、60 s 内且未被使用的预览 |
| 300 | `GateNotReleased` | `PreconditionFailed` | 目标账户的启动闸门未放行 |
| 301 | `Draining` | `PreconditionFailed` | DRAINING 期间的升级命令 |
| 302 | `ReconNotClean` | `PreconditionFailed` | 对账未清 |
| 303 | `ResumeLimit` | `PreconditionFailed` | `Resume{DailyLoss}` 当日次数已用完 |
| 304 | `SimNotIdle` | `PreconditionFailed` | `SimReset`:该 Sim 账户仍有有效模式 Live 的 cell、活动目标、Unknown 槽或非终态的 IOC/FOK 槽 |
| 305 | `InsufficientBalance` | `PreconditionFailed` | `SimTransfer` 余额不足 |
| 306 | `UnknownPresent` | `PreconditionFailed` | 仍有 Unknown 槽(含 hft-risk `ModeErr::UnknownPending`) |
| 307 | `NakedNotCleared` | `PreconditionFailed` | `Resume{Broken}` 时裸量仍超过尘埃线 |
| 308 | `GroupHasPosition` | `PreconditionFailed` | 被移除的 group 仍有持仓、库存、在途或活动目标 |
| 309 | `NoMatchingCap` | `PreconditionFailed` | `Resume` 找不到完全匹配的 cap |
| 310 | `StaleReport` | `PreconditionFailed` | `AdoptExchange` 的报告在 `view_seq` 之后已有成交 |
| 311 | `NoBalanceView` | `PreconditionFailed` | 该 (account, venue) 尚无余额视图 |
| 312 | `ConfigSourceNotJournal` | `PreconditionFailed` | `ReplaceRefdata` 要求 hft-core `config_source = journal` |
| 313 | `SmokeActive` | `PreconditionFailed` | 该 (account, venue) 已有一张未终结的冒烟单 |
| 314 | `SmokeNoPrice` | `PreconditionFailed` | 该 inst 报价不可用(hft-oms `SmokeErr::NoQuote`) |
| 315 | —(退役,永不复用) | — | — |
| 316 | `NotQuiescent` | `PreconditionFailed` | `WriteOffResidual` 的 group 不静止(定义归 [hft-oms 残余核销](../../crates/hft-oms/README.md#残余核销));`HedgeFix` 的 group 有活动链接、Unknown/Pending* 槽或即时在途(hft-exec `ExecCmdErr::NotQuiescent`) |
| 317 | `Hard` | `PreconditionFailed` | `HedgeFix` 时 HARD 生效(hft-exec `ExecCmdErr::Hard`) |
| 318 | `NothingToFix` | `PreconditionFailed` | `HedgeFix` 时已成交裸量 ≤ 尘埃线或取整后低于最小下单量(hft-exec `ExecCmdErr::NothingToFix`) |
| 319 | `QuoteUnavailable` | `PreconditionFailed` | `HedgeFix` 时多出那一腿报价不可用(hft-exec `ExecCmdErr::QuoteUnavailable`) |
| 320 | `ChainLeg` | `PreconditionFailed` | `HedgeFix` 时多出的一腿是链上腿(optional;hft-exec `ExecCmdErr::ChainLeg`) |
| 321 | `NotEnded` | `PreconditionFailed` | `Settle` 时引擎 UTC 早于该窗口的 `end_s` |
| 400 | `BrokenRemaining` | `Rejected` | `Resume{HedgeFailure}` 时该账户仍有 `Broken` cell(规则归 [hft-risk](../../crates/hft-risk/README.md)) |
| 401 | `RefdataRecheck` | `Rejected` | `ReplaceRefdata` 的新 blob 未通过 `RefData::load(新 blob, 当前)` 复核;arg = [hft-refdata `LoadErrCode`](../../crates/hft-refdata/README.md#loaderrcode-码表) 的数值,订阅容量不足(不是 `LoadErrCode`)时 arg = 0;旧 refdata 继续生效,不 Drain、不退出 |
| 402 | `RefdataAssembly` | `Rejected`(gateway) | `ReplaceRefdata` 的新 blob 未通过 ctl-gateway 的装配校验(与 hft-core 启动步 2 同一组函数);arg = 失败项:`AccountNamePrefix=1`、`MissingCredential=2`(Live 账户缺交易 key)、`SimConfig=3`(模拟撮合配置干校验失败)、`ThreadPlan=4`(线程计划未覆盖全部 venue)、`DustBelowMinNotional=5`(某账户的尘埃线低于其交易腿的最小名义);不写环、不进 sequencer、没有 `CmdResult`,旧 refdata 继续生效 |
| 403 | `SmokeRiskReject` | `Rejected` | 冒烟单被风控检查链拒绝(含 HARD;`SMOKE` 意图的放行口径归 hft-risk);arg = 置位的 `RejectBits` 中最低的位号 |

gateway 侧的拒绝(`Unauthorized`、`Forbidden`、`RateLimited`、`Busy`、gateway 产生的 `InvalidArgs` 与 `Rejected{RefdataAssembly}`)用同一码表,名字随响应 JSON 返回。hft-risk `ModeErr` 与 hft-exec `ExecCmdErr` 到本表的映射在 [hft-sequencer](../../crates/hft-sequencer/README.md#命令路径) 命令路径(`ModeErr::UnknownPending` → `PreconditionFailed{UnknownPresent}`;`ExecCmdErr::UnknownGroup` → `InvalidArgs{UnknownGroup}`,其余 `ExecCmdErr` → `PreconditionFailed{同名 detail}`,码 316–320)。新增 detail 码按 [cookbook](../cookbook/adding-a-control-command.md) 在本表追加。

Kill 的清仓进度与严格零仓判据归 [执行层](../../crates/hft-exec/README.md#清仓永续尾仓);清仓订单不申请 OMS link,满链接表不会阻断清仓,Applied 仅表示命令生效。

## 命令集

命令按 `scope` 分两类:**core** = 首次上线(新子账户上线 ccex 策略)必须具备、必须通过上线门槛的命令;**optional** = 已分配编码与语义、首次上线不实现的命令,未实现前 [hft-ctl-proto `perms.rs`](../../crates/hft-ctl-proto/README.md#权限表) 不含它们,gateway 返回 `Forbidden`,触发条件写在语义列。编码(载荷布局)与 `CmdKind` 编号归 [hft-event 控制命令编码](../../crates/hft-event/README.md#控制命令编码);本页的命令名与其变体名一一对应。

- core:`SetMode`、`SetParams`、`SetRisk`、`Kill`、`Resume`、`CancelAll`、`Drain`、`HedgeFix`、`AdoptExchange`、`SetBalanceBaseline`、`VenueNotice`、`SimReset`、`SimTransfer`、`RecordTransfer`、`Annotate`、`ReplaceRefdata`、`StrategyCmd`、`InjectWarmup`、`SmokeOrder`、`WriteOffResidual`、`SimFault`、`BindSlot`、`Settle`,只读请求 `GetSchema`,外加非命令输入 `ReconcileReport`。
- optional:`RotateConn`、`Resubscribe`、`SnapshotNow`。

| 命令 | scope | 参数 | 类别 | 语义 |
|---|---|---|---|---|
| `SetMode` | core | `strategy \| *`(实例名)、`group \| *`、`account`、`mode ∈ {Shadow, CloseOnly, Live}` | 按目标值 | 只写期望模式(`ModeTable`),不清任何 cap。`group = *` 同时写该 (strategy, account) 的 `default_mode`(当前和以后新增的 group)。目标 Shadow 时只有 (strategy, account, group) 净仓、在途、LegTracker 残余全为 0 的 cell 立即生效,其余有效模式为 CloseOnly、`detail = ShadowDeferred`,归零后自动落到 Shadow(写 `Note{ShadowDeferred}`)。升级:启动闸门未放行或 DRAINING → `PreconditionFailed` |
| `SetParams` | core | `strategy`(实例)、`account`、`blob`(SPB1:`StrategyParamsHdr` + `GroupSpec[]` + body,`hft-cfg` 或 pipeline 生成)、`base_version?` | 中性 / 按[类别](#类别)判定为升级时升级 | 整体替换该 (strategy, account) 的参数。gateway:容器解析、`strategy/account` 与 blob 头一致、**账户已绑定到该实例**(`AccountBound`)、种类与 schema 版本/摘要一致、每个 `GroupSpec` 的 refdata 一致性(含**同一 group 的任两腿不在同一交易所**,`InvalidArgs{SameExchangeLegs}`)、调装配处 `StrategySet::decode_params` 解码并 `validate` body(失败 → `InvalidArgs{ParamInvalid}`,kind `Strategy`)、`non_prod_only` 的策略不得绑定正式账户(`ParamInvalid`,kind `NonProdOnly`)、计算新参数下的 `Model::mem_bytes`/`snapshot_bytes` 与是否改动了结构字段(预算读 ECF1 段 5,规则归 [hft-ctl](../../crates/hft-ctl/README.md));sequencer 复核绑定与 `NonProdOnly`,并检查 `GroupId` 全局唯一(`GroupIdDup`)、同一实例内 (account, inst) 不被两个非 Shadow cell 引用(`InstOwned`)、有仓位的 group 不得被移除(`PreconditionFailed{GroupHasPosition}`)、新增或指纹变化的 group 两腿的公共 lot 可下单(`InvalidArgs{LotMismatch}`)、本 hft-core 实例全部策略实例的 Model 内存之和与检查点上界之和不超过预算(`ParamInvalid`,kind `ModelMemBudget`;预算取值归 [apps/hft-core](../../apps/hft-core/README.md))。移除 group 即删除其 cell 并撤销其定时器(写 Note);新 group 取 `default_mode`。参数通道:pipeline 按策略插件生成并以 `pipeline` 主体下发(恒为中性、不需确认,Live 账户上新增 group 与 `open_allowed` 0→1 亦然;上界由 `RiskParams` 约束,pipeline 不升级 `RunMode`);人工改动经 ops_bot 的参数编辑命令(命令名归 [ops_bot](../../python/ops_bot/README.md);它改 pipeline 的策略文件,由 pipeline 以自己的身份下发),或 `hft-cfg` 生成 blob 由 `cli:<login>` 按二次确认协议(预览 + `expect_state_version`)发送;Telegram 不直接发原始 `SetParams` |
| `SetRisk` | core | `account \| *`、`risk`(`RiskParams` 文档,`hft-cfg` 生成)、`base_version?` | 放宽升级 / 收紧降级(按账户逐字段,sequencer 判定) | 只替换该账户作用域内字段(`accounts[a]`、`venues(·,a)`、`groups(a,·)`);作用域外字段有改动 → `InvalidArgs{OutOfScope}`;全局字段只能 `account = *`;新值使执行配置与风控不一致(判定式见[结果码](#结果码) 121)→ `InvalidArgs{ExecRiskMismatch}`(sequencer 判定,规则归 [hft-exec](../../crates/hft-exec/README.md) 与 hft-risk) |
| `Kill` | core | `level ∈ {Account, Venue, Global}`、`scope`(account / venue,0 = 全部)、`cancel_resting`(默认 true)、`flatten`(默认 false)、`hard`(默认 false) | 默认参数(含 `hard`):降级;`flatten`:升级 | 设 `Kill` cap(同一目标再 Kill 时合并取更严),重算 kill 字;撤挂单优先 venue 级 `CancelAll`,且是持续义务。`hard` 置该条的 `cflags.HARD`:**与 Global 组合 = 全局 HARD;与 Account 组合 = 只对该账户 HARD**(其他账户的对冲照常);与 Venue 组合 → `InvalidArgs`。`flatten` 由执行层按[严格永续零目标](../../crates/hft-exec/README.md#清仓永续尾仓)平掉该账户敞口(含 Broken 组合:先对永续腿 reduce-only 减仓,规则归 [hft-exec](../../crates/hft-exec/README.md)),并置该账户的清仓锁 `flatten_lock`:直到 `Resume{Kill}` 前,该账户只接受比当前更接近 0 的目标与 Reduce/Flatten 意图(规则归 hft-exec 与 [hft-risk](../../crates/hft-risk/README.md));不得与 Global 或 `hard` 组合(`BadCombination`;HARD 下引擎单全停,清仓不会执行)。带 `flatten` 的 `Kill` 是升级类,但在启动闸门未放行与 DRAINING 期间照常执行(仍需确认)。**启动时的停机标记 Kill**:kill-switch `stop_core` 写下的停机标记由 hft-core 在启动步 5 转成一条 `issuer = cli:systemd` 的 Input `Kill`(范围同标记,非 HARD、不带 flatten;见[版本与确认](#版本与确认)),它与 Trader 发出的 Kill 完全相同。**Viewer 紧急停止**:只允许 `level ∈ {Global, Account}`、非 `hard`、非 `flatten`、`cancel_resting = true`——停开仓 + 撤挂单,对冲与平仓继续;必须先预览、再由同一 issuer 在 60 s 内确认(绑定预览的 `cmd_hash`,不比对 `ctl_state_version`,见[版本与确认](#版本与确认));其 cap 与 Trader 发出的完全相同,解除需 Trader/Admin 的 `Resume{Kill}` |
| `Resume` | core | `source ∈ {Kill, DailyLoss, Recon, UnknownTimeout, Broken, Reorg, Recovery, Halted, HedgeFailure, Delisting}`、`scope`(与被解除的 cap 完全一致) | 升级 | 只删完全匹配的一条 cap(无匹配 → `PreconditionFailed{NoMatchingCap}`);前置条件归 [hft-risk](../../crates/hft-risk/README.md);`Resume{HedgeFailure}` 时该账户仍有 `Broken` cell → `Rejected{BrokenRemaining}`。条件型来源(`ClockBad`、`MarginStress`、`Storage`、`AccountCfg`)与 `Draining` 不能被 `Resume` → `InvalidArgs{NotResumable}`。只有删除带 HARD 的 `Kill` 记录时才把超过补腿阈值的裸量判为 `Broken`(见[优先级表](#模式与上限的统一优先级)第 0 层);其余 `Resume`(含非 HARD 的 `Kill`)不判 Broken,放行后的补腿与外来成交默认处理按新窗口进行(规则归 [hft-exec](../../crates/hft-exec/README.md)) |
| `HedgeFix` | core | `account`、`group` | 中性 | 按最新对账与 LegTracker 两腿差**平掉多出的一腿**(`RISK_REDUCING`,数量与名义受对冲上限约束),预览基于最新对账;`Broken`/`Reorg` 的人工处理方式。`Broken` 持续 `broken_auto_fix_ms` 且期间无 `HedgeFix` 时,执行层另做一次自动有界平仓(规则归 [hft-exec](../../crates/hft-exec/README.md));无论人工还是自动,账户级 `HedgeFailure` cap 都仍需 `Resume` |
| `WriteOffResidual` | core | `account`、`group` | 升级(需确认;权限见 [hft-ctl-proto](../../crates/hft-ctl-proto/README.md#权限表)) | 适用于**静止的** group(`Broken` 或非 `Broken` 均可;静止的定义与多出那一腿的选择归 [hft-oms 残余核销](../../crates/hft-oms/README.md#残余核销)):把多出那一腿的已成交裸量从该 group 移到 unassigned,并清零该 group **全部腿**的 `residual`——OMS 调整 LegTracker,返回单条 `WriteOff{venue, inst, leg_ix, qty}`(没有多出的一腿时 `qty = 0`),ledger 对这一腿记 `AdjustDelta::WriteOff`(不计入价差 PnL,写 `Note{ResidualWrittenOff}`,语义归 [hft-ledger](../../crates/hft-ledger/README.md)),随后按 hft-risk 的口径重新判定 `Broken` 能否解除(执行顺序归 [hft-sequencer](../../crates/hft-sequencer/README.md))。用于"残余高于尘埃线却低于最小下单量"使 `Broken`/`HedgeFailure` 无法解除、或使静止的非 Broken group 无法移除的情形。group 不属于该账户绑定实例的当前参数 → `InvalidArgs{UnknownGroup}`;不静止 → `PreconditionFailed{NotQuiescent}`。预览显示多出的那一腿(venue、inst、将核销的量与名义)。实盘与模拟账户都适用;`HedgeFailure` cap 仍需 `Resume` |
| `SmokeOrder` | core | `account`、`venue`、`inst`、`side`、`ttl_ms`(缺省 10 000,1..=60 000) | 升级(需确认;只允许 Admin,见 [hft-ctl-proto](../../crates/hft-ctl-proto/README.md#权限表)) | **冒烟单**:验证正式端点的下单、客户端 id、交易所死手与撤单,不产生仓位。OMS 以 `origin = Command`、`OrderFlags::SMOKE` 建槽:只能 PostOnly,价格远离对手价(≥ `smoke_far_bp`),数量约为该 inst 的最小名义(价格、数量与取整的唯一定义在 [hft-oms](../../crates/hft-oms/README.md));`ttl_ms` 到期由 OMS 自动撤单(`Timer{Oms, SmokeTtl}`);槽与死手刷新按普通挂单处理,因此死手覆盖该 inst。不计入任何策略、不改任何目标;不经模式门(期望模式 Shadow/CloseOnly 下照常);风控按 [hft-risk](../../crates/hft-risk/README.md) 的冒烟单口径检查,被拒(含 HARD)→ `Rejected{SmokeRiskReject}`。OMS `SmokeErr` 的映射:`Busy`(每 (account, venue) 同时至多一张)→ `PreconditionFailed{SmokeActive}`,`NoQuote` → `PreconditionFailed{SmokeNoPrice}`,`BadInst` → `InvalidArgs{UnknownName}`。只对 Live 账户(`NotLive`);inst 不属于 `venue` 或 `venue` 不在该账户内 → gateway 先回 `InvalidArgs{UnknownName}`。`Applied` 时同一事件内有该单的 `OrderDecision`(`trigger_seq` = 本命令的 seq);用法见 [launch D-smoke](../runbooks/launch.md) |
| `AdoptExchange` | core | `account`、`venue`、`report_id`(一组已生效的 `ReconcileReport`)、`item ∈ {Position, Balance}`;Position:`inst`;Balance:`asset`、`group?`(缺省 = `GROUP_NONE`,即 unassigned);`entry_px?` | 升级 | 人工在交易所网页操作之后,把报告中该条目的对账差写成 ledger 调整(`Adjust` 类,不计入价差 PnL)并同量调整 LegTracker;Position 采纳永续仓位差,Balance 采纳现货库存差(归入指定 group 或 unassigned);`entry_px` 非 0 时按它记成本,否则按对账时 mark(永续)/bid(现货)。报告带 `view_seq`,该 inst/资产自 `view_seq` 以来有成交 → `PreconditionFailed{StaleReport}`。只对 Live 账户(`NotLive`)。OMS 不建槽;之后需一轮新对账才能 `Resume{Recon}`。语义细节归 [hft-ledger](../../crates/hft-ledger/README.md) |
| `SetBalanceBaseline` | core | `account`、`venue` | 升级 | 把该 (account, venue) 当前**非策略资产**(交易所余额中不属于任何 group 库存的部分)登记为基线:之后对账比较"交易所余额 − 基线"与 ledger 的策略库存,基线内的资产(平台币、尘埃、旧币)不再成为差异。需要该 (account, venue) 已有余额视图(`PreconditionFailed{NoBalanceView}`);只对 Live 账户。语义归 [hft-ledger](../../crates/hft-ledger/README.md) |
| `VenueNotice` | core | `venue`、`inst \| *`、`kind`:`Delisting{at_ns}` \| `FundingInterval{new_s}` \| `Maintenance{from_ns, to_ns}` | `Delisting`:降级;其余:中性 | 登记交易所公告(进快照):`Delisting` → 对 (venue, inst) 设 `Delisting` cap(该 inst 在全部账户只平不开)并向引用该 inst 的策略投递 Notice 事件,退出计划由策略决定;`FundingInterval` → 把该 inst 的结算周期视图改为 `new_s` 直到 `Funding` 事件给出新值,并通知策略;`Maintenance` → 登记维护窗口并通知策略,窗口内的门控同 refdata 维护窗口(hft-risk)。`inst = *` 作用于该 venue 全部 inst |
| `BindSlot` | core | `venue`、`inst`(refdata 符号)、`start_s`、`end_s`、`wire_id`(十进制 uint256,`0` = 解绑) | 中性 | 把运行期绑定 venue(交易所 id 不在 refdata 里,如 [venue-polymarket](../../crates/venue-polymarket/README.md#槽位绑定) 的结果代币)的一个槽位 inst 绑定到交易所 id,窗口 `[start_s, end_s)` 为 UTC 秒(进快照节 31)。前置:inst 属于该 venue、`start_s < end_s`(否则 `InvalidArgs{BadValue}`);该 inst 在任一账户有仓位 → `PreconditionFailed{GroupHasPosition}`,有挂单 → `PreconditionFailed{NotQuiescent}`;与现有绑定相同 → `NoOp`。已订阅的 inst 立即把新 id 交给行情任务(退订旧 id、订阅新 id、簿等待新快照),未订阅的在首次订阅前交付(重启后同样);有实盘账户的 venue 同时交给其下单网关(下单要用 token id,重启后重发)。只由运行期绑定 venue 的行情发现(pipeline)使用 |
| `Settle` | core | `venue`、`inst`(refdata 符号)、`end_s`、`payout`(十进制价格) | 中性 | 运行期绑定 venue 的一个槽位 inst 的窗口已结算:每单位按 `payout` 兑付(二元结果为 0 或 1)。前置:inst 属于该 venue、当前绑定的 `end_s` 等于命令的 `end_s`、`0 ≤ payout ≤ 1`(否则 `InvalidArgs{BadValue}`);引擎 UTC 早于 `end_s` → `PreconditionFailed{NotEnded}`;任一实盘账户在该 inst 有挂单 → `PreconditionFailed{NotQuiescent}`。对每个在该 inst 有库存或挂单的模拟账户,sim-gw 撤销其挂单并把库存按 `payout` 交割为计价资产(回报 `ExternalFill{cause = Delivery}`,语义归 [hft-sim](../../crates/hft-sim/README.md));对每个有库存的实盘账户,其下单网关把库存按 `payout` 报为 `ExternalFill{cause = Delivery}`(交易所的链上兑付由兑付进程另行完成,余额经账户轮询核对,[hft-venue](../../crates/hft-venue/README.md#出环outmsg--gwcmd));没有这样的账户 → `NoOp`。交割后槽位才能被 `BindSlot` 改绑。只由运行期绑定 venue 的行情发现(pipeline)在交易所公布结果后使用 |
| `SimReset` | core | `account` | 中性(需确认) | 只对 Sim 账户(`NotSim`);要求该账户全部 cell 有效模式不是 Live、无活动目标、无 Unknown 槽、无非终态的 IOC/FOK 槽(`PreconditionFailed{SimNotIdle}`);挂单由 sim-gw 撤销。ledger 把该账户的余额恢复为 refdata `SimAccountCfg.initial_balances`、仓位与库存归零(`Adjust` 类),LegTracker 同步归零;sequencer 向 sim-gw 投影 `SimReset`。语义归 [hft-sim](../../crates/hft-sim/README.md) |
| `SimTransfer` | core | `account`、`from_venue`、`to_venue`、`asset`、`amount` | 中性 | 只对 Sim 账户;模拟跨所划转:ledger 记资金流动(不计 PnL),sim-gw 同步调整两边余额;余额不足 → `PreconditionFailed` |
| `SimFault` | core | `account`、`venue`、`fault ∈ {Off, RejectNew, Disconnect, DelayReports}`、`delay_ms`(只用于 `DelayReports`,1..=60 000;其余写 0) | 中性(仅 Admin 经 `cli:<login>`,见 [hft-ctl-proto](../../crates/hft-ctl-proto/README.md#权限表)) | **受控故障注入**(Broken 处置等实盘式演练,如 [DRL-12](../tests/drills.md)):设置或撤销(`Off`)sim-gw 对该 (account, venue) 的故障——`RejectNew` 新单回拒单、`Disconnect` 该账户该 venue 的模拟连接断开、`DelayReports` 回报推迟 `delay_ms` 交付;同一 (account, venue) 的新故障替换旧故障,故障持续到 `Off`、被替换或 sim-gw 重启;`Off` 而当前无故障仍为 `Applied`(sim-gw 无动作)。只对 Sim 账户(`InvalidArgs{NotSim}`;`prod` 实例没有 Sim 账户,因此在 `prod` 实例上一律被拒);`venue` 不在该账户内 → `InvalidArgs{UnknownName}`;`delay_ms` 越界 → `InvalidArgs{BadValue}`。命令作为 Input 记 journal,回放确定;故障语义、状态与重启恢复归 [hft-sim](../../crates/hft-sim/README.md)。ops_bot 不映射 |
| `Annotate` | core | `kind ∈ {OpsNote, LocalWrite, Incident}`、`text`(UTF-8 ≤ 256 B)、`digest?` | 中性 | 只写审计,不改任何状态 |
| `CancelAll` | core | `account \| *`、`venue \| *`、`inst?` | 降级 | 范围内 `Live`、`PendingNew`、`PendingAmend` 槽撤单(可撤状态归 [hft-oms 撤单范围](../../crates/hft-oms/README.md)):venue 支持批量撤单时每 (venue, account) 一条 venue 级 `CancelAll`;否则逐槽撤单,每 Input 至多 K 条,其余经 `Timer{OmsContinue}` 续发 |
| `ReplaceRefdata` | core | `account`(用于预览)、`refdata` blob(`hft-cfg` 生成)、`version` | 中性(需确认;仅 Admin 经 `cli:<login>`,维护窗口人工发起;要求 hft-core `config_source = journal`) | **语义 = 写 `ConfigBlob` → 复核 → `Drain` → hft-core 以新 refdata 重启**(可选扩展:允许热替换的变更子集,触发条件:Drain + 重启的停机窗口在运营中不可接受)。ctl-gateway 先用 hft-core 启动步 2 的同一组装配校验函数核对新 blob 与当前配置的组合(账户名前缀、Live 账户的交易 key、模拟撮合配置干校验、尘埃线与最小名义、线程计划覆盖全部 venue):不通过 → gateway 直接回 `Rejected{RefdataAssembly}`(arg = 失败项),不写环、不进 sequencer、没有 `CmdResult`(只有 gateway 的响应与 `Note{CtlAudit}`)。sequencer 处理命令时先按[版本与确认](#版本与确认)检查 `version`(≤ 当前 → `StaleVersion`),再以 `RefData::load(新 blob, 当前)` 复核跨版本不变量(资产指数、合约乘数、账户身份与类型、实例):不通过 → `CmdResult{Rejected, detail = RefdataRecheck}`,旧 refdata 继续生效,不 Drain、不退出;通过 → `Applied` 并发起 Drain,Drain(含超时)完成后以退出码 75 退出由 systemd 重启。Drain 写快照前的再次复核与失败处理归 [apps/hft-core](../../apps/hft-core/README.md)(不通过时 sequencer 写 P1 `Note{RefdataRecheckFailed}`,码登记在 [hft-event](../../crates/hft-event/README.md#note-来源与码登记表)) |
| `Drain` | core | `cancel_resting`(默认 true;SIGTERM 取 hft-core `shutdown.cancel_open_orders`) | 降级 | 设不可清除的 `Draining` cap → `cancel_resting` 时 venue 级 `CancelAll`;执行层停止增仓切片,引擎减仓照常;等待执行层与 OMS 静止且没有未到达的 `MarginStress`/`Hard` 引擎目标(`exec.drain_ready()`;条件归 [hft-sequencer](../../crates/hft-sequencer/README.md),受 `drain_timeout_ms` 约束,超时写 `Note{DrainTimeout}`)→ Unknown 写进快照 → 快照 → 干净退出;期间升级命令返回 `PreconditionFailed{Draining}`。SIGTERM 路径由 ctl-gateway 以 `issuer = cli:systemd` 提交 |
| `RecordTransfer` | core | `account`、`from`、`to`、`asset`、`amount`、`ext_ref` | 中性 | 登记人工划转(按 treasury 的建议由人执行之后):ledger 记为资金流动、不计入 PnL;reconciler 以 `ext_ref` 核对,到账前该余额差不判级;不移动资金 |
| `StrategyCmd` | core | `strategy`(实例)、`account`、`group \| none`、`reduce_only`、`payload`(≤ 256 B,含义由该策略定义) | 升级;`reduce_only` 时中性(DRAINING、CloseOnly、闸门未放行时也可发,策略因此产生的 Open 意图一律被拒) | 策略专属人工操作的通用通道(如"人工开一组""平某组")。引擎不解释载荷:sequencer 校验账户绑定(`AccountBound`)、`group` 为 `none` 或属于该 (实例, 账户) 的当前参数(`UnknownGroup`),把载荷作为 `EventRef::Manual`([hft-strategy-sdk](../../crates/hft-strategy-sdk/README.md))投递给该实例的快速部分;策略据此产生的意图照常经模式门与检查链,载荷格式、限次与开平规则归策略。ops_bot 按策略在 schema 里导出的描述渲染该命令(命令名归 ops_bot) |
| `InjectWarmup` | core | `strategy`(实例)、`local_kind`、`chunk_no`、`last`、`payload`(≤ 4096 B,格式由该策略定义);`account` 必须为 `*` | 中性 | 冷启动预热数据的通用注入通道:作为 Input 记 journal(回放确定),advisor 在该命令 `Applied` 后把它作为 `EventRef::Warmup{local_kind, chunk_no, last, payload}` 交给该实例的 `Model`,策略声明接收时也投递给快速部分。生产者是 pipeline 的策略插件(从交易所 REST 历史数据或 collector 数据生成样本)或 Trader;一批数据由生产者产出为块列表,每块 ≤ 4096 B 且**自身完整**(块边界与块内格式归策略);客户端逐块发送,`chunk_no` 从 0 递增、末块 `last = 1`,不切字节([hft-ctl 客户端](../../crates/hft-ctl/README.md#客户端));sequencer 只校验实例存在与长度。不改变任何风险状态;启动闸门未放行、DRAINING 期间同样可用 |
| `RotateConn` | optional | `venue`、`account` | 中性 | 主备下单连接切换(无 Pending 槽时)。触发条件:运维需要不重启而人工切换主备下单连接(自动切换仍由 hft-venue 负责) |
| `Resubscribe` | optional | `venue`、`inst?` | 中性 | 行情重订阅/重同步。触发条件:hft-venue 的自动重同步不足以恢复、需要人工干预 |
| `SnapshotNow` | optional | — | 中性 | 下一次空闲时写 `StateSnapshot`。触发条件:排障需要在 Drain 之外临时取一致快照 |

新增命令的步骤见 [cookbook/adding-a-control-command](../cookbook/adding-a-control-command.md)。

### 只读请求:`GetSchema`

`GetSchema{strategy_kind}` 返回该策略种类的参数 schema JSON(格式由 [hft-strategy-sdk `params`](../../crates/hft-strategy-sdk/README.md) 定义),由 ctl-gateway 从编译进二进制的策略描述本地应答;不写 ctl 环、不产生 `ControlCmd`/`CmdResult`,只审计。全部角色与 pipeline 主体可用(逐主体权限见 [hft-ctl-proto 主体表](../../crates/hft-ctl-proto/README.md#主体与角色码))。ops_bot 用它按 schema 渲染参数面板与校验参数编辑命令的输入。

### 非命令输入:`ReconcileReport`

`ReconcileReport` 不是 `ControlCmd` 变体:reconciler 经 ctl-gateway 把一轮对账的 `count` 条记录(同一 `report_id`,`idx` 从 0)连续写进 ctl 环,**末条到达时整组生效**(OMS Unknown 收敛 + 风控分级 + 启动闸门 `RECONCILED` 位);未完成的分组写进快照。布局与 `item`/`severity` 码归 [hft-event](../../crates/hft-event/README.md#控制与系统0x04xx0x05xx)。没有"请求立即对账"命令:reconciler 周期运行,冷启动时看到 `Note{EpochStart}` 立即对账。reconciler 不对模拟账户对账。

- **人工处置只有两种命令**:仓位与余额差异经 `AdoptExchange` 采纳,非策略资产经 `SetBalanceBaseline` 登记为基线;没有逐条"确认/忽略"差异的命令。
- **资金费条目(`item = Funding`)只告警、不进闸门**:它不参与风控分级、不设 `Recon` cap、不计入 `RECONCILED` 与 HARD 解除条件;差额超过 `funding_diff_alert` 时 watchtower 报 P1,hft-ledger 对结算时刻早于 `funding_recon_lag_ms` 的差额按交易所金额自动校正(`AdjustDelta::Funding`,写 `Note{FundingCorrected}`),不需要任何命令。

### chain-core 的控制会话(optional)

链上策略接入时启用。chain-core 有独立的最小命令集(`Pause{hard}`、`Resume{source, scope}`、`ClearTaint`、`SetEndpoint`、`SetBidCap`、`Drain`),走本页相同的信封、类别、账户、二次确认、幂等、过期与结果码规则,命令与结果写进 chain journal;命令语义在 [chain-core](../../apps/chain-core/README.md),主体与权限在 [hft-ctl 的 chain-core 会话](../../crates/hft-ctl/README.md#chain-core-的-ctl-会话)。链上意图在 kill 下只看 `sflags.RISK_REDUCING`;hft-core → chain-core 心跳携带每个账户的 KillBits 位图。启用后 ops_bot 的"只停链上"= `Pause{all, hard = false}`,"全部停止"= hft-core `Kill{Global}` + chain-core `Pause{all}`,回执逐项列出。

## 与 ops_bot 命令的映射

ops_bot 是纯冷进程:读 journal/Parquet 出面板,写只经本页命令([09 §5.3](../../design/09-可观测性、风控与控制面.md))。**bot 的命令名、别名与交互的唯一归属是 [python/ops_bot](../../python/ops_bot/README.md)**,它的命令表为每个写操作标明对应的本页命令;本页只规定映射必须满足的规则:

1. 每个写操作恰好映射到本页一个命令(或 pipeline 的 `SetParams` 下发),类别与确认要求按本页;升级与需要确认的命令一律走二次确认协议(预览按钮 → 确认按钮带 `expect_state_version`;Viewer 紧急停止的确认按钮不带它,只能由发起预览的同一 `from.id` 在 60 s 内按下)。
2. 账户:bot 的默认账户视图只影响显示与预览,升级/中性命令的账户必须由用户显式给出;消息前缀带账户标签,模拟账户另标"模拟"。一个 hft-core 有多个账户(每个策略实例绑定自己的实盘或模拟账户)。
3. 策略参数:Telegram 不直接发原始 `SetParams`/`SetRisk`/`ReplaceRefdata`。参数编辑命令按 `GetSchema` 的 schema 通用渲染与校验输入,改 pipeline 的策略文件(本地确认,写 `Annotate{LocalWrite}`),由 pipeline 以自己的身份下发;风控参数与 refdata 经 `hft-cfg` + CLI。
4. 只读面板(引擎状态、有效模式与 cap、目标与执行进度、各所权益与保证金率、treasury 划转建议、模拟报告)直接读 journal/Parquet,不经 ctl-gateway;Viewer(老板)只能用只读面板、`GetSchema`、无状态效果的 `Annotate{OpsNote | LocalWrite}` 与紧急停止(映射为非 HARD、不带 flatten 的 `Kill{Account(a)}` 或不带账户时的 `Kill{Global}`,预览 → 确认两步;hft-core 不健康时的 kill-switch 兜底与 P0 告警见[版本与确认](#版本与确认);bot 命令名与中文提示归 ops_bot)。
5. kill 类入口必须能表达:不带账户 = 全部账户;`hard` 可与账户或全局组合;optional 的 chain-core 启用后,"全部停止"同时发 chain-core `Pause`,回执逐项列出。

## 延伸阅读

- 决策出处:[round8-decisions](../../design/_work/round8-decisions.md);[09 §4–5](../../design/09-可观测性、风控与控制面.md)、[13 O3、O4](../../design/13-设计评审与优化.md)
- [research/00 §1.5 control-v1、§4 opsd](../../design/research/00-existing-codebase-inventory.md)
- [hft-ctl](../../crates/hft-ctl/README.md)、[hft-ctl-proto](../../crates/hft-ctl-proto/README.md)、[hft-risk](../../crates/hft-risk/README.md)、[hft-sequencer](../../crates/hft-sequencer/README.md)、[hft-ledger](../../crates/hft-ledger/README.md)、[hft-sim](../../crates/hft-sim/README.md)、[python/ops_bot](../../python/ops_bot/README.md)、[event-model](event-model.md)
