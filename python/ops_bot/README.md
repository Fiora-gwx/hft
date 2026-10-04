---
description: "Telegram 冷进程:按 from.id 分 Admin/Trader/Viewer 角色(老板 = Viewer:只读 + `/note` 备注 + 唯一改变状态的写操作「紧急停止」/stop,确认绑定同一命令的预览 + 60 s;ctl-gateway 不可达时经 kill-switch 撤单,风险状态允许时再 stop_core 停止 hft-core@prod,否则要求 /stop … core 再确认;记为待补 Kill、恢复后自动补发,并自己发 P0;Admin 私聊的 /smoke 冒烟单),按策略 schema 与描述符通用地展示账户、目标、风险、参数、策略 Note 与报表面板,把运维意图翻译成控制命令,人工参数修改按实例分键(校准对显式),模拟账户标注「模拟」;策略专属工作流是随策略包发布的可选插件"
kind: python-package
tier: cold
scope: core
status: spec
---

# ops_bot

## 概要

`ops_bot` 是操作新框架的唯一 Telegram 入口:它经 `hft_py` 读 journal(以及 watchtower、pipeline、treasury 的状态),渲染账户、策略实例、目标与执行进度、保证金与风险、参数、策略 Note 与模拟报告面板;把运维意图翻译成 `ControlCmd` 经 ctl-gateway 发给 hft-core,从 journal 的 `CmdResult` 读回结果。它不持有任何交易所 key、不下单、不改 hft 的文件、不启停 systemd 单元([python/AGENTS.md](../AGENTS.md))。权限按 Telegram `from.id` 分角色 `Admin`、`Trader`、`Viewer`。**老板是 Viewer**:全部只读面板,外加唯一的写操作**紧急停止** `/stop [账户]`——非 HARD、不带 flatten 的 `Kill{Global | Account}`:禁止新开仓、撤销挂单,已成交第一腿的对冲与平仓照常;确认绑定同一命令的预览,60 s 内有效;ctl-gateway 不可达时改经 kill-switch 撤单,并在停引擎不会中断风险处置时停止 `hft-core@prod`(`stop_core`;会中断时停引擎要再确认一次 `/stop … core`,条件见[紧急停止](#紧急停止stop)第 6 步),记一条"待补 Kill",hft-core 恢复后自动补发,并直接发 P0;不能恢复。面板、参数编辑与策略命令按各策略导出的 schema JSON 与描述符通用处理:新增策略不改本包代码,本包的代码与文档不出现策略族名。策略专属的工作流(例如链上策略的面板与命令)是随策略包发布的可选插件;在链上策略迁到新框架之前,它的旧 bot 照常服务旧框架,本包不替代它。

## 目录

- [使用本组件](#使用本组件)
  - [角色与鉴权](#角色与鉴权)
  - [账户、标签与视图](#账户标签与视图)
  - [命令的账户语义](#命令的账户语义)
  - [二次确认](#二次确认)
  - [紧急停止(/stop)](#紧急停止stop)
  - [命令表](#命令表)
  - [参数面板与修改](#参数面板与修改)
  - [策略命令与描述符](#策略命令与描述符)
  - [策略插件(optional)](#策略插件optional)
  - [推送](#推送)
  - [审计](#审计)
  - [配置文件字段](#配置文件字段)
- [实现说明](#实现说明)
- [延迟与资源契约](#延迟与资源契约)
- [测试与验收](#测试与验收)
- [依赖](#依赖)
- [延伸阅读](#延伸阅读)
- [已知限制与后续工作](#已知限制与后续工作)

## 使用本组件

```
hft-ops-bot --config /etc/hft/ops_bot.toml        # hft-ops-bot.service,系统用户 hft-opsbot
```

### 角色与鉴权

角色的定义、每个命令与参数组合所需的角色、群聊规则与确认要求的**唯一归属**是 [control-plane](../../docs/subsystems/control-plane.md) 与 [hft-ctl-proto 权限表](../../crates/hft-ctl-proto/README.md#权限表)。ops_bot 不另写权限表:它在本地按同一张表做一次预判(给出友好提示),gateway 再判一次并以 gateway 结果为准。ops_bot 侧的约定:

| 角色 | 谁 | ops_bot 侧行为 |
|---|---|---|
| `Viewer` | 老板及其他只看的人 | 全部只读命令;唯一改变状态的写命令是 `/stop [账户] [core]`(预览 + 本人确认,见[紧急停止](#紧急停止stop));`/note`(`Annotate{OpsNote}`,只进 journal、无状态效果)也可用;其他写命令(含 `/kill`、`/resume`、`/ack`、`/sub`)在本地即拒绝并记审计(`denied: viewer`),不发往 gateway;Viewer 的推送订阅只由配置 `users[].push_accounts` 决定;推送照常 |
| `Trader` | 交易员 | 只读 + 写命令(模式、kill、`Drain`、恢复、补腿、采纳、基线、交易所公告、模拟账户操作、划转登记、备注、参数修改请求、策略命令、预热、告警确认 `/ack`、推送订阅 `/sub`);`/stop` 即默认参数的 Kill,立即发出、不需确认 |
| `Admin` | 负责人/运维 | Trader 的全部;私聊中的冒烟单 `/smoke`(`SmokeOrder`)与 `/writeoff`(`WriteOffResidual`);`SetRisk` 与 `ReplaceRefdata` 只经运维 CLI(`cli:<login>`),Telegram 不提供 |

鉴权步骤(每条 update):

1. `from.id` 在 `users` 表中,否则静默丢弃并记日志(`unauthorized update dropped: user=… chat=…`)。
2. 聊天类型:`private` → 按用户角色;`group`/`supergroup` 且 `chat.id` 在 `groups` 表中 → 只读面板(下表"只读面板"的全部命令,含 `/account`、`/menu`、`/help`、`/start`)对全部角色放行;写命令按它发出的 `ControlCmd` 在[权限表](../../crates/hft-ctl-proto/README.md#权限表)"群聊"列放行(`/stop … core` 同 `/stop`);会发 `Annotate` 的写命令(`/note` 与本地写 `/params … set`、`/warmup`、`/sub`;`Annotate` 的群聊列为"—")只在私聊;`/ack` 只写 watchtower 的 `acks.jsonl`、不发命令,群里放行;不放行的回复"请私聊执行"并记审计(`denied: private_only`);其他聊天(未登记的群、channel)→ 丢弃并记日志。
3. 本地按权限表预判,不满足即拒绝并记本地审计(`denied`)。
4. 写命令限速:每用户最近 60 s 内(滑动窗口)至多 `limits.writes_per_min` 条;计入的是通过第 3 步、将被发出或生成预览的写命令,被限速拒绝的那条本身不计入(审计 `denied: rate_limited`);确认/取消按钮不计;`/stop`(含 `/stop … core`)不受限速、不计入。
5. 发往 hft-core 的命令带 `issuer = tg:<from.id>` 与该用户的角色,并附 `view_account` 与 `chat_type`,由 gateway 写进与该命令配对的 `CtlAudit` 审计 Note;gateway 以"uid 上限 ∩ issuer 角色"再判一次,每个请求(含被拒的)都进 journal 审计——ops_bot 的检查不是唯一防线。

BotFather 设置:`/setjoingroups` 关闭(只加入预先配置的群)、`/setprivacy` 开启、`/setcommands` 由本包按启用的插件与用户角色生成(Viewer 的命令菜单只含只读命令、`/note` 与 `/stop`)。

### 账户、标签与视图

- 账户表(`AccountId` → 账户名 `AccountMeta.label` → `kind` → 绑定的策略实例 → 所在 hft-core 实例 `prod`/`sim`)从各 hft-core 实例当前生效的 refdata(`AccountMeta`,[hft-refdata](../../crates/hft-refdata/README.md))只读加载,不在 ops_bot 里另写一份;每个账户只绑定一个策略实例([one-account-per-strategy-instance](../../.agents/notes/proposed/architecture/2026-10-02-one-account-per-strategy-instance.md))。模拟账户只在 `instance.name = "sim"` 的实例上;dev2 上 `prod` 角色实例只含测试网账户、只在联调窗口运行,升级主机上 `prod` 与 `sim` 同机([deploy 模拟实例](../../deploy/README.md#模拟实例sim))。命令按账户所在的实例发往对应的 ctl socket。两个实例的账户名不相交由 hft-core 的前缀规则保证([hft-core 实例角色](../../apps/hft-core/README.md#实例角色prod-与-sim));ops_bot 启动时与任一实例的 refdata 变化后仍比对两份账户表,有同名账户 → 启动时退出 2,运行中则对该名字的一切命令本地拒绝并 P1。
- **标签**:每条涉及账户的消息以账户标签开头——实盘 `<b>[💰实盘·<名称>]</b>`,模拟 `<b>[🧪模拟·<名称>]</b>`,测试网账户(refdata 中指向测试网端点的 Live 账户)`<b>[🧪测试网·<名称>]</b>`(已带标签的不重复);与账户无关的系统消息只带实例标签 `[<host>/<instance>]`。模拟与测试网账户的金额、PnL、成交在任何汇总里都不与实盘合计,汇总行分"实盘合计"与"模拟合计"两行(测试网账户计入"模拟合计")。
- `/accounts`:全部账户一屏摘要(按实盘/模拟分组:权益、各 venue 的 `liq_dist` 与 MarginStress 档位、裸敞口、期望模式与有效模式、生效 cap)。
- `/account <名称>`:切换该用户的**只读默认视图**,立即生效;它对任何写命令都不是默认值。只读命令可直接带账户名(`/p <名称>`)而不切换视图。

### 命令的账户语义

规则本身归 [control-plane 账户语义](../../docs/subsystems/control-plane.md);ops_bot 的行为:

- **降级命令不带账户参数时作用于全部账户**:`/kill` 与 `/stop` = `Kill{Global}`;`/kill venue <v>` = 每个账户在该 venue 上的 kill;`/mode <实例> closeonly` 不写账户 = 该实例绑定的账户。`Kill{Global}` 只作用于收到它的 hft-core 实例,所以 `/stop`、`/kill`(含 `/kill venue <v>`)不带账户时与 `/shutdown`(`Drain`)ops_bot 向本机每个运行中的实例(`prod` 与 `sim`;"运行中" = 该实例心跳新鲜,判定同[紧急停止](#紧急停止stop)第 3 步,其余实例在回执中列为"未运行")各发一条,回执逐实例列出;Viewer 的 `/stop` 对每个实例各发一条预览,合成一条预览消息、一个确认按钮,确认时对每个实例各发确认。
- **升级与中性命令必须显式写账户名**,缺省即本地拒绝("请写明账户"),不回退到视图账户;ops_bot 发出的 `CmdHdr.account` 与 `CmdScope.account` 永远相同。例外:`/warmup` 作用于策略实例、不带账户(`account = *`,control-plane 的实例级命令)。
- 模拟账户与实盘账户用同一套命令;只对模拟账户有效的命令(`/simreset`、`/simtransfer`)对实盘账户本地即拒。

### 二次确认

协议的唯一定义在 [control-plane 版本与确认](../../docs/subsystems/control-plane.md#版本与确认) 与 [hft-ctl-proto 二次确认](../../crates/hft-ctl-proto/README.md#二次确认),哪些命令与参数组合需要确认以 [权限表](../../crates/hft-ctl-proto/README.md#权限表)的"确认"列为准;ops_bot 不保存任何确认令牌:

1. 需要确认的命令,以及命令表中写明"预览"的中性命令(`/hedgefix`、`/transfer`),先以 `preview` 标志发出;sequencer 不改状态,回 `CmdResult{preview}`(含是否需要确认与当前 `ctl_state_version`;字段归 [hft-event](../../crates/hft-event/README.md))。gateway 的响应 `CmdResponse.cmd_hash` 是该命令规范形的哈希([hft-ctl-proto 请求与响应](../../crates/hft-ctl-proto/README.md#请求与响应)):ops_bot 把它记在本地短 id 上,预览消息显示其前 8 位,并核对确认响应的 `cmd_hash` 与之相同(不同 → 提示并重新预览);确认请求不携带它。
2. ops_bot 生成业务预览:第一行固定为**目标账户标签(实盘/模拟/测试网)+ 该账户当前期望模式、有效模式与生效 cap**,随后一行是对照账户(同一校准对里的另一个账户,若有)的有效模式;之后是"将从 X 变为 Y"、受影响的 group、各腿与名义。消息带内联按钮 `confirm:<id>` / `cancel:<id>`;`<id>` 是本地短 id,绑定 `(from.id, chat.id, cmd 规范形, ctl_state_version, 预览时刻)`,`confirm.ttl_s` 后失效。
3. **只有发起预览的同一 `from.id` 点确认才有效**(群聊里同样如此)→ ops_bot 用**新的 `cmd_id`** 发同一命令,带 `confirmed = true` 与 `expect_state_version`;sequencer 判定版本已变 → `ConfirmInvalid{StateChanged}`,ops_bot 提示"状态已变化"并重新生成预览。他人点击、过期、重复点击 → 不执行。
4. **Viewer 的 `/stop` 例外**:确认绑定**同一命令的预览**与 TTL(预览后 60 s 内),**不发** `expect_state_version`、不比对状态版本——事故中 cap 与模式频繁变化,不能让老板的确认反复失效。确认时用 `hftctl.confirm(…, state_version=None)` 以同一命令规范形、`confirmed = true` 重发;gateway 对它重算 `cmd_hash`,sequencer 与预览时登记的 `{cmd_hash, issuer}` 记录匹配(规则归 [control-plane 版本与确认](../../docs/subsystems/control-plane.md#版本与确认));ops_bot 只按第 1 步显示与核对 `cmd_hash`,不在请求中传递它。
5. 执行后编辑原消息为 `CmdResult`(含 `detail`,如 `ShadowDeferred`);10 s 内未见结果 → 显示"已发出,结果待定",随后补发(`/stop` 另按[紧急停止](#紧急停止stop)第 5 步处理)。

不经 gateway 的本地写(参数修改请求,见[参数面板与修改](#参数面板与修改))使用同一形状:先预览(带对方返回的版本号),确认时带"期望版本"提交,版本已变即失效;本地写执行后发 `Annotate{LocalWrite, digest}` 进 journal。

- 幂等:`cmd_id = uuid5("tg:{update_id}:{canonical cmd}")`;Telegram 重投同一 update 得到 gateway 缓存的结果,不重复执行。
- 过期:降级命令 `expires_at_ns` = 签发后 300 s,其他 30 s([control-plane 过期](../../docs/subsystems/control-plane.md))。

### 紧急停止(/stop)

老板(Viewer)唯一的写操作;Trader/Admin 也可以用,对他们 `/stop` 就是默认参数的 Kill(立即发出、不需确认,权限表见 [hft-ctl-proto](../../crates/hft-ctl-proto/README.md#权限表))。决定见 [viewer-emergency-stop](../../.agents/notes/proposed/architecture/2026-10-03-viewer-emergency-stop.md),接手步骤见 [incident-response R](../../docs/runbooks/incident-response.md#r-老板紧急停止stop之后)。

1. `/stop` 或 `/stop <账户>`;另外只接受尾参数 `core`(第 6 步的第二次确认);其他参数(`flatten`、`hard`、`venue`、`cancel_resting` 等)一律本地拒绝并审计。命令总是 `Kill{Global}` 或 `Kill{Account(a)}`,`hard = false`、不带 `flatten`、`cancel_resting` 取默认。
2. **预览**(只对 Viewer):ops_bot 以 `preview` 发该 Kill,预览标题为"⚠️ 紧急停止",列出:范围、受影响的每个账户(标签、当前有效模式、持仓摘要、挂单数)、以及固定提示"禁止新开仓并撤销挂单;**不会平仓**,持仓保留,已成交第一腿的对冲与平仓照常;恢复需交易员"。预览本身失败(ctl 不可达、hft-core 无响应)时,预览改为显示"控制面不可达,确认后若仍不可达将经 kill-switch 撤单并停止 hft-core",照样给出确认按钮。Trader/Admin 跳过本步。
3. **发出时的健康判定**(Viewer:同一 `from.id`、预览后 60 s 内确认时;Trader/Admin:收到命令时):对每个目标实例读 `HeartbeatBlock`(与 `/status` 同一读法:"新鲜"即 [hft-rt 判活规则](../../crates/hft-rt/README.md#等待与存活)成立,心跳年龄——距 ops_bot 最近一次看到区 0 计数前进的时长——> 该实例 `watchdog.hb_stale_ms` 或 `state ∉ {Warmup, Running, Draining}` 即"过期")并试连 ctl socket(单次连接 ≤ `stop.health_timeout_ms` 内建立算可连)。
4. **正常路径**(ctl 可连):发出 Kill(Viewer 按[二次确认](#二次确认)第 4 步);发出后 `stop.result_timeout_s`(10 s)内(含边界)收到 `CmdResult` ok → 推送"紧急停止已生效"(P1,见[推送](#推送))。
5. **失败**:Kill 被拒(任何 `detail`)→ ops_bot **自己发 P0**("紧急停止失败",附 `detail` 与实例状态)给值班交易员。10 s 内没有 `CmdResult` → 同样 P0;此时 ctl 已不可连或心跳已过期 → 同时走第 6 步。
6. **兜底**(ctl 自第一次失败起持续 `stop.ctl_unreachable_s`(10 s)不可连——期间每秒重试,一旦可连即回到第 4 步——心跳新鲜或过期都一样;或第 5 步超时且心跳过期):只对 `prod` 实例。ops_bot 先把这次停止写进待补 Kill 文件(第 7 步),再按 `prod` 实例的风险状态选择动作,请求都经 kill-switch 的本地触发套接字(`killswitch.trigger_socket`,只接受 ops_bot 的 uid,[deploy](../../deploy/README.md#systemd-单元))发出(`req_id`、`action`、实例 `prod`、账户名或 `*`、`issuer = "tg:<from.id>"`、`reason`;字段与回执归 [kill-switch](../../apps/kill-switch/README.md)):
   - **停引擎会中断风险处置时只撤单**:ops_bot 内存视图中该实例任一账户 MarginStress ≥ L2、有 `Broken` 的 group、有在途的 LegLink(含 `RESUME_HOLD`)、清仓锁 `flatten_lock` 的清仓未完成、有外来成交的默认处理在计时、或有 Broken 自动平仓正在执行,或视图本身停滞(journal 尾随超过 `ops.view_stale_ms` 未前进,状态未知按"成立"处理)→ 只发 `action = cancel`。各项只读 journal 重建,不漏报、可以多报:MarginStress 档位与 `Broken` 按 hft-risk 的 `Note{CapSet}`/`Note{CapCleared}`(`CapRecord` 的来源与 `level`);清仓未完成、外来成交计时、自动有界平仓执行中按 [hft-exec 冷读者重建](../../crates/hft-exec/README.md#execdecision-与进度通知)表;在途的 LegLink = 该实例存在 `link_role ∈ {First, Second, SecondRetry, Reverse}` 且尚无终态 `OrderEvent` 的订单(含 Unknown),或某 group 按 hft-ledger 从 journal 重建的已成交两腿差 `|Σ net| × mark` > 该账户 `ExecConfig.dust_notional`(取自该实例当前生效的 `EngineConfig`,经 `hft_py` 的 `config_at`;与 watchtower `NakedExposure` 同一来源)。`cancel` 时:kill-switch 撤目标账户的全部挂单,**hft-core 不停**(L3 减仓、对冲与补腿继续)。ops_bot 自己发 P0("紧急停止经 kill-switch 兜底:已撤单;引擎未停止——<原因>;停止引擎会中断减仓、对冲与补腿,确认要停请发 `/stop [<账户>] core`")。
   - **否则撤单并停引擎**(`action = stop_core`):kill-switch 先写停机标记(hft-core 下次启动带 Kill cap,规则归 kill-switch 与 hft-core),撤目标账户的全部挂单,再经 polkit 执行 `systemctl stop hft-core@prod`(SIGTERM → `Drain`,不平仓;该实例的**全部账户**随之停止交易)。ops_bot 自己发 P0("紧急停止经 kill-switch 兜底:已撤单,hft-core@prod 已停止",附 kill-switch 回执)。
   - **`/stop [<账户>] core`(第二次确认)**:任何能发 `/stop` 的角色都可用;ops_bot 先预览"将经 kill-switch 停止 hft-core@prod:该实例全部账户停止交易,L3 减仓、对冲与补腿中断",同一 `from.id` 在 `confirm.ttl_s` 内确认后:ctl 已可连 → 按第 4 步发 Kill 并回复"控制面已恢复,按正常路径停止"(Viewer 的这次确认即视为本人确认:ops_bot 随即以同一 issuer 发 Kill 的 `preview` 与确认);仍不可连 → 先写待补 Kill 条目(第 7 步),再发 `stop_core`(同上一条)。
   - 回执表明 hft-core 未能停止(`core_stop ∈ {Timeout, Denied, Failed}`)→ P0 中加粗"hft-core 进程仍在运行,策略可能重新下单——按 incident-response R2 处理";`Done.marker = false`(停机标记写入失败,或因停机失败被 kill-switch 作废为 `.aborted`)→ P0 另注明"停机标记未生效,重启后的禁开只靠待补 Kill",该待补 Kill 条目保持 `pending`;`Rejected` 或套接字不可连 → P0 注明"兜底也失败,按 incident-response G 手动 `kill-switch fire`"。目标只有模拟账户时没有真实挂单可撤:只发 P1"模拟实例不健康,`/stop` 未生效"。
7. **待补 Kill**:兜底路径的停止不经 hft-core、不进 journal,所以 ops_bot 把它记进 `/var/lib/hft/ops_bot/pending_kill.json`(原子写):每条 `{req_id, instance: "prod", scope: "*" | <账户名>, issuer, reason, at_utc, state = pending | applied}`。`stop_core` 时 hft-core 启动已按 kill-switch 的停机标记带上 Kill cap,补发是第二道保证(只撤单的兜底没有停机标记,补发是唯一的接续)。`prod` 实例重新可用(心跳新鲜且 ctl 可连)后,ops_bot **先于任何其他命令**处理每个未完成的条目:先查 journal——同一范围(`*` 对应 `Kill{Global}`,账户名对应该账户的 `Kill{Account}`)在条目的 `at_utc` 之后已有 Kill 被设、随后又被 `Resume{Kill}` 解除(ops_bot 不在时交易员已用 CLI 接手并恢复)→ 不补发,只发 `Annotate{OpsNote}`(`req_id`、范围、"已由人工恢复,未补发")、移除该条目并 P1 告知;否则以自身主体(`issuer = ops_bot`,降级、不需确认)补发同一范围的 Kill(`hard = false`、不带 `flatten`;Kill 已在时合并,无副作用);`CmdResult` 为 `Applied` 或 `NoOp` → 该条 `applied`,发 `Annotate{OpsNote}`(`req_id`、范围、结果)与 P1"待补 Kill 已生效",随后从文件移除(审计表保留);被拒 → P0,每 30 s 重试。存在未 `applied` 的条目时,ops_bot 对其范围内账户(`*` = `prod` 实例的全部账户)的 `/resume`(任何 source、任何角色)一律本地拒绝,提示"待补 Kill 未完成"(审计 `denied: pending_kill`);`/status` 列出这些条目。文件不存在 = 没有条目;残留的 `pending_kill.json.tmp` 在启动时删除;文件不可解析 → 改名为 `pending_kill.json.corrupt-<utc_ns>`、P0,并按一条 `{scope: "*", issuer: "ops_bot", at_utc: 该文件的 mtime, state: pending}` 的条目处理(状态未知按"成立"处理)。
8. 任何结果都推送给全部 Trader/Admin 私聊与已登记的群("紧急停止:<范围>,发起人 <名字>,时间,路径 = 正常/兜底/失败"),附 incident-response R 的链接;紧急停止推送不受订阅影响。
9. Viewer 发 `/resume` → 本地拒绝,提示"恢复请联系交易员"。

### 命令表

命令的语义、参数与类别以 [control-plane 命令集](../../docs/subsystems/control-plane.md) 为唯一定义,所需角色以 [权限表](../../crates/hft-ctl-proto/README.md#权限表) 为准。下表只写 ops_bot 侧的映射与行为(源:`J` = 经 `hft_py` 读 journal 与 S3 Parquet;`W` = watchtower 状态;`P` = pipeline 本地接口;`T` = treasury 输出;`H` = `HeartbeatBlock`)。

**只读面板**

| 命令 | 内容 | 源 |
|---|---|---|
| `/status`、`/health` | 每实例 `HeartbeatBlock` 状态与心跳年龄、按账户与 venue 的启动闸门(含 `IDENTITY`、`ACCOUNT_CFG`)、生效 cap、各 venue `ConnState`、persist 与 archiver 滞后、reconciler 最近报告、kill-switch 状态、各策略实例的 Model 检查点与预热状态;`dev2` 上另显示护栏守护状态 | J + W + P + H |
| `/accounts`、`/account <a>` | 见上节 | J |
| `/p <a>` | `PortfolioView`:每 venue 现金(free/locked)、现货库存(按 group 或 unassigned)、永续持仓与未实现、保证金余额、可用保证金、保证金率、合计权益 | J |
| `/risk <a>` | MarginStress 档位;每 venue 的 `liq_dist`、决定它的 inst、`liq_px` 来源(交易所报告/账本估算)与到各档的距离;交叉校验比率;各 inst 实际杠杆(`AccountConfigReport`)与 ADL 档;DailyLoss 余量;HedgeFailure/Broken(含 Broken 持续时间与距自动平仓的剩余时间);清仓锁;最近 `ExternalFill`、`ExecAutoFixDone/Failed`、`FundingCorrected` | J |
| `/targets <a>` | 每 group 的当前目标(各腿目标净量、原因、紧急度、不交易带)、执行进度(`TargetView`)、最近 `ExecDecision` | J |
| `/orders <a>` | 在途订单、Unknown、链接状态 | J |
| `/pnl <a> [日期]` | PnL 分解(资金费、基差、手续费、相对到达价的滑点、`adjust`,其中资金费自动校正单列);历史按日读 Parquet | J |
| `/sim <a> [日期]` | 模拟账户日报(PnL 分解、成交率、两腿间隔、裸敞口、保证金占用、策略报表字段);`/sim <模拟> vs <实盘>` 给同一校准对并行期间的校准差异 | J(archiver/watchtower 产出) |
| `/notes <a> [n]` | 该账户策略实例最近的策略 Note,按描述符渲染(名称、级别、各 `v[]` 的含义与单位) | J |
| `/params <实例> [group]`、`/params pair <校准对>`、`/schema <策略>` | 见[参数面板与修改](#参数面板与修改) | J + P + schema JSON / `GetSchema` |
| `/treasury [a]` | treasury `latest.json` 的 `status`(`ok`/`incomplete`/`stale`)、`headroom`(两个方向的 L1/L2/L3 与 `approximate`)、`in_transit` 与划转建议(字段归 [treasury 输出与登记](../treasury/README.md#输出与登记)) | T |
| `/pipeline [实例]` | pipeline 每实例最近一轮的状态、下发结果与预热状态 | P |
| `/alerts [n]`、`/audit [n]` | watchtower 告警列表;本地审计 + journal 中 gateway 审计 Note + kill-switch 触发记录 | W;本地 + J |
| `/menu`、`/help`、`/start` | 按钮面板、手册;`/help` 本地渲染,hft 不可用时也能用;`/start` 只显示欢迎与角色 | — |

**写命令**(Viewer 只能用 `/stop` 与 `/note`,以 [权限表](../../crates/hft-ctl-proto/README.md#权限表) 为准)

| 命令 | → `ControlCmd` | ops_bot 侧行为 |
|---|---|---|
| `/stop [<a>] [core]` | `Kill{Global \| Account(a)}`(非 HARD、不带 flatten);兜底时 kill-switch `cancel` 或 `stop_core` | 全部角色可用;Viewer 走 PREVIEW + 确认(同一命令的预览 + 60 s),Trader/Admin 立即发出;`core` 是兜底只撤单之后停引擎的第二次确认(全部角色都要预览 + 确认);见[紧急停止](#紧急停止stop) |
| `/mode <实例> [group] <a> <shadow\|closeonly\|live>` | `SetMode` | `→ live` 走二次确认;回执显示有效模式与 `ShadowDeferred` |
| `/kill [account <a> \| venue <v> \| all] [flatten] [hard]` | `Kill` | 默认参数立即发出;`flatten` 走确认且不得与 Global 组合(对 `Broken` 的 group 同样生效,先减永续腿;同时置账户级清仓锁,直到 `/resume kill`;启动闸门未放行与 Drain 期间同样可用,类别规则归 control-plane);`hard` 可与 `account <a>` 或 Global 组合(按账户 HARD),是降级、不需确认、立即发出,回执列出该范围当前未对冲量 |
| `/resume <source> <scope>` | `Resume` | source 含 `kill`、`dailyloss`、`recon`、`unknown`、`recovery`、`broken`、`hedgefailure`、`halted` 等(以 control-plane 为准);只删完全匹配的一条 cap,不触发自动 HedgeFix;前置条件不满足时显示缺的条件与建议的下一步(如先 `/adopt`);该账户有未完成的待补 Kill([紧急停止](#紧急停止stop)第 7 步)时本地拒绝 |
| `/hedgefix <a> <group>` | `HedgeFix` | 只平掉多出的一腿;预览基于最新对账与 LegTracker;任一腿读取失败即中止(不当作 0);多出那一腿的差额低于该腿 inst 的 1 lot 回复无需修复;有在途意图则拒绝;对 `Broken` 的 group 发出后,引擎不再对它自动平仓 |
| `/adopt <a> <venue> <条目> <report_id> [entry_px]` | `AdoptExchange` | 条目可为仓位或余额(现货库存);预览列出报告里的引擎值、交易所值与将写入的调整;资金费差异不经此命令(账本自动校正) |
| `/baseline <a> <venue>` | `SetBalanceBaseline` | 预览列出将被登记为基线的非策略资产 |
| `/transfer <a> <from> <to> <asset> <amount> <ref>` | `RecordTransfer` | 登记人工执行的划转;预览显示两端当前余额 |
| `/notice <venue> <inst\|*> delist <UTC> \| funding <秒> \| maint <UTC> <UTC>` | `VenueNotice` | `delist` 是降级、立即发出;其余为中性;回执列出受影响的账户、group 与持仓 |
| `/simreset <模拟账户>`、`/simtransfer <模拟账户> <from> <to> <asset> <amount>` | `SimReset`、`SimTransfer` | 只对 `kind = Sim` 的账户;回执带"模拟"标签 |
| `/smoke <a> <venue> <inst> <buy\|sell> [ttl_ms]` | `SmokeOrder` | 只限 Admin、只在私聊;PREVIEW + 确认(预览列出账户标签、inst、方向、按 `smoke_far_bp` 定出的价格与最小名义数量、`ttl_ms`);`Applied` 后按 `trigger_seq`(= 本命令的 seq,[control-plane](../../docs/subsystems/control-plane.md))找到同一事件的 `OrderDecision`,回显其订单号;用于 [launch D-smoke](../../docs/runbooks/launch.md#d-正式实盘m8) 与每月正式账户冒烟 |
| `/writeoff <a> <group>` | `WriteOffResidual` | 只限 Admin、只在私聊;PREVIEW + 确认(预览显示**多出的那一腿**——venue、inst、`leg_ix` 与将从该 group 移到未归属持仓的已成交裸量,以及该 group 各腿将被清零的残余;适用于静止的 group,Broken 或非 Broken 均可,语义归 [hft-oms 残余核销](../../crates/hft-oms/README.md#残余核销));用于高于尘埃线却低于最小下单量的残余死锁([incident-response A](../../docs/runbooks/incident-response.md#a-裸敞口nakedexposure断腿broken)) |
| `/cmd <实例> <a> [group\|none] <命令名> [字段=值 …]` | `StrategyCmd` | 见[策略命令与描述符](#策略命令与描述符) |
| `/warmup <实例>` | (本地)→ pipeline 本地接口 `run_warmup` → pipeline 发 `InjectWarmup` | Trader;回显 pipeline 的进度(块数、`last`、结果);见 [pipeline 预热](../pipeline/README.md#model-预热) |
| `/shutdown` | `Drain` | 降级、立即发出(不需确认);回执列出全部账户的挂单与 Live cell 及 Drain 进度 |
| `/params <实例> set …`、`/params pair <校准对> set …` | (本地)→ pipeline 覆盖项 → pipeline 发 `SetParams` | 见下节 |
| `/note <文字>` | `Annotate{OpsNote}` | 全部角色(含 Viewer)可用;无状态效果 |
| `/ack <id>` | (本地)写 watchtower 的 `acks.jsonl`(追加一行 `{"id": "<告警 id>", "by": "tg:<from.id>", "at_utc_ns": <整数>}`);`ops-<n>` 形式的 id 另在本地 `pages` 表记为已确认 | — |
| `/sub <a> on\|off` | (本地)推送订阅 | — |

Telegram 不直接发原始 `SetParams`、`SetRisk`、`ReplaceRefdata`、`InjectWarmup`:策略参数与预热数据经 pipeline 下发,风控参数与 refdata 经 `hft-cfg` + 运维 CLI。

### 参数面板与修改

全部按策略 schema 通用处理([hft-strategy-sdk](../../crates/hft-strategy-sdk/README.md) 的 `ParamSchemaDesc`)。schema JSON 来自发布归档的 `schemas/strategies/`(`hft-core schemas --out` 导出的 `<kind_id>-<name>.schema.json` + `index.json`,[strategies/registry](../../strategies/registry/README.md)),经 `hft_py.params.load_schemas(dir)` 载入;归档里没有或与运行时版本不符时用 `GetSchema{strategy_kind}` 取,按 `(schema_id, schema_ver)` 缓存:

- `/schema <策略>`:字段表(名称、单位、范围、默认值、是否按 group 覆盖、说明)。
- `/params <实例> [group]`:从 journal 中当前生效的 `StrategyParams`(`ConfigBlob`)用 `hft_py.params.decode(schema, blob)` 解码出每个字段的值,按 `Unit` 渲染(`Rate` 显示为 bp/%,`Notional` 显示为 USDT,`DurationMs` 显示为时长,`Enum` 显示名称);标出与默认值不同的字段、per-group 覆盖与 `open_allowed`、每个覆盖项的来源(本实例 / 校准对);显示参数版本、来源(`ParamSource`)与生成时间。`/params pair <校准对>` 列出该对的共享覆盖项与两个实例的当前版本。
- `/params <实例> set <字段> [group] <值>`(Trader):**只写该实例**的覆盖项(`scope = instance`)。ops_bot 先按 schema 做本地校验(单位换算、范围、是否允许按 group);再请求 pipeline 本地接口 `preview_override(target = "instance:<实例>", …)` 生成预览(字段级差异、新旧版本、是否属于放宽、是否作用于 Live 实例),预览带覆盖文件的当前版本;本人确认时带该期望版本提交,pipeline 写入并在这一轮(或立即)以自己的身份下发 `SetParams`([pipeline 参数通道](../pipeline/README.md#参数通道))。结果在 `CmdResult` 到达后回显。作用于 Live 实例的人工修改一律需要确认;预览首行标出"实盘"。
- `/params pair <校准对> set <字段> [group] <值>`(Trader):写校准对共享的覆盖项(显式 `scope = pair`),同时作用于对内的实盘与模拟实例;预览列出两个实例的差异,总是需要确认。这是让模拟实例上的修改进入实盘实例的唯一捷径,其他情况下模拟实例上的试验不会流到实盘。
- 同一参数修改只作用于预览中列出的实例;要在模拟账户上先试,对模拟实例发 `/params <模拟实例> set …`。

### 策略命令与描述符

每个策略在 `StrategyDesc` 中给出三类描述符并随 schema JSON 导出([hft-strategy-sdk 策略描述符](../../crates/hft-strategy-sdk/README.md)):`notes`(Note 码表:码、名、级别、`v[]` 含义)、`report`(日报的策略专项字段:名、单位、聚合方式)、`cmds`(人工命令 `CmdDesc{op, name, reduce_only, fields, doc}`)。ops_bot 只按描述符通用处理,不认识任何策略:

- `/notes`、推送与日报中的策略 Note 和报表字段按 `notes`/`report` 渲染;未登记的码显示为原始数值并记 P2。
- `/cmd <实例> <a> [group|none] <命令名> [字段=值 …]`:按 `cmds` 中同名的描述符解析字段(同参数的单位换算与范围检查),用 `hft_py.params.encode_cmd` 编码载荷(`{ver = 1, op, n, _pad[5]} + i64[n]`,n ≤ 31,≤ 256 B),发 `StrategyCmd`。描述符 `reduce_only = true`(平仓类)时命令带 `reduce_only` 标志,按中性命令处理(Drain/CloseOnly 期间也允许,不需确认);否则按升级命令走 PREVIEW + 确认。`cmds` 为空的策略回复"该策略不接受人工命令"。
- **单 group 强制退出**是这套通用机制的典型用法:策略可在 `cmds` 中声明 `reduce_only` 的退出命令,并在 schema 中提供 per-group、只由人工设置的强制退出参数;ops_bot 不为它们写专门代码——前者走 `/cmd <实例> <a> <group> <命令名>`,后者走 `/params <实例> set <字段> <group> true`。命令名、字段名与语义(是否受成本门约束、何时解除)只在该策略的 README 定义。

### 策略插件(optional)

策略专属的面板与命令以插件形式随策略包发布(`strategies/<name>/` 内的 Python 包,经 entry point 组 `hft_ops_bot.plugins` 发现,与 [pipeline 插件](../pipeline/README.md#插件的打包与发现)同一做法);本包只提供注册命令、面板与推送格式的接口,插件经同一鉴权、确认与审计路径,不得绕过 `hftctl.py` 或读写其他进程的文件,不新增通用代码路径。

- 当前唯一计划中的插件属于 optional 的链上策略,触发条件:决定把链上策略迁到新框架([ccex-first-scope](../../.agents/notes/proposed/architecture/2026-10-02-ccex-first-scope.md))。启用后增加:链上策略面板(价差/成本线/持仓与资金视图)、流水线命令(经该策略的 pipeline 插件)、链上腿相关的 `/kill chain`、`/kill all`(hft-core `Kill{Global}` + chain-core `Pause{all}`,回执逐项列出、任一项失败不影响另一项)、`/resume chain`、`/bridge`(只显示 treasury 的链上钱包建议)、开平仓推送;旧 bot 的人工开平仓映射为 `/cmd`(该策略的 `cmds`)。旧命令语义与旧 bot 的对照表随插件一起写进该策略包,来源以 `analysis/util0929` 与 `analysis/strat0926/orig5` 的旧 bot 为准。

### 推送

| 推送 | 周期 / 触发 | 内容 | 源 |
|---|---|---|---|
| 紧急停止 | 任何人的 `/stop` 得到结果(正常、兜底或失败) | 范围、发起人、时间、路径;兜底附 kill-switch 回执(含 hft-core 是否已停止);发给全部 Trader/Admin 与已登记的群 | 本地 + J |
| **P0:紧急停止失败 / 兜底** | 见[紧急停止](#紧急停止stop)第 5、6 步(含"已撤单、引擎未停止,需 `/stop … core`");待补 Kill 被拒(第 7 步) | 由 ops_bot **自己投递**:全部 Trader/Admin 私聊 + `page.groups` + Bark critical(`page.bark_secret`);同一次 `/stop` 只投一次;每条带本地 id `ops-<n>`(`n` 为 SQLite 中递增的整数),`/ack ops-<n>` 记为已确认(照常写 `acks.jsonl`);未确认则自上次投递起每 5 min 重投一次,直到被 `/ack` | 本地 |
| 待补 Kill 已生效 | 第 7 步补发的 Kill 得到 ok | 范围、原 `req_id`、补发时刻(P1) | 本地 + J |
| 成交与目标进度 | journal 中该账户的成交、目标完成/卡住(`Exec{Done\|Stuck\|Broken}`)、`ExecAutoFixDone/Failed` | 各腿成交量与价格、手续费(按扣费资产)、两腿间隔、目标剩余;按账户合并,每账户每分钟至多一条摘要 | J |
| 保证金与外部成交 | `MarginStress` 档位变化、`ExternalFill` | 档位、`liq_dist` 与阈值距离;外部成交的原因(强平/ADL/人工/交割)与另一腿状态。P0 本身由 watchtower 投递,这里只是账户视图的补充 | J |
| 策略 Note | 级别不低于 `push.note_min_level` 的策略 Note(级别为 `NoteLevel`:`P0` > `P1` > `P2` > `Info`,码值归 [hft-event](../../crates/hft-event/README.md)) | 按 `notes` 描述符渲染 | J |
| 定时报告 | 每 `push.summary_every_min` 分钟 | 订阅账户的权益、敞口、有效模式摘要 | J |
| treasury 划转建议 | treasury 每轮写出 `latest.json` 后(按其 `round_ts` 每轮至多处理一次) | `suggestions[]` 中 `notify = true` 的建议:from → to、资产、金额、成本、时延、紧急度、原因;按 `users[].push_accounts` 订阅过滤,带账户标签;`urgency = urgent` 的同样受订阅约束、不与其他建议合并;去重与"同一建议 `renotify_min` 内不重复、紧急不受限"由 treasury 在 `notify` 中判定,ops_bot 不复算 | T |
| 日报 | 每天 `push.daily_at_local` | watchtower 日报(告警统计、dev2 护栏触发次数)+ 每账户 PnL 分解 + 策略报表字段 + 模拟账户日报 + 模拟/实盘校准差异 + treasury 摘要(每账户 `headroom.day_min_l2`) | W + J + T |
| 命令结果 | `CmdResult` | 见确认流程 | J |

- 推送按用户订阅:`users[].push_accounts`(默认只订阅实盘账户);`/sub <a> on|off` 调整。紧急停止相关推送不受订阅影响。
- 其他 P0/P1 告警**不由 ops_bot 投递**(watchtower 直接投递,ops_bot 宕机不吞告警,[watchtower](../../apps/watchtower/README.md));ops_bot 只展示与 ack。唯一例外是上表中 `/stop` 的失败、兜底与待补 Kill:那时 hft-core 可能与 watchtower 一起不可用,由发起停止的进程自己投递;另外,只有 ops_bot 知道的本包状态(两个实例出现同名账户、`/stop` 目标只有模拟实例而未生效)的 P1 由 ops_bot 投递到全部 Trader/Admin 私聊。

### 审计

- 本地:`/var/lib/hft/ops_bot/ops.sqlite3`(WAL、`synchronous=FULL`),表 `requests`(幂等缓存)、`audit`(每条命令:时间、from.id、chat.id、命令规范形、角色判定、预览的 `ctl_state_version`、结果、`/stop` 的路径与 `req_id`;结果取 `ok`、gateway 或 `CmdResult` 的结果码名、`unreachable`(ctl socket 不可连),或本地拒绝 `denied: <原因>`,原因为 `viewer`、`role`、`private_only`、`rate_limited`、`bad_args`、`account_required`、`not_sim`、`pending_kill`、`dup_account`;确认按钮的无效点击记 `denied: not_initiator`、`denied: expired` 或 `denied: used`)、`pending_confirm`(本地短 id → 发起人 from.id、规范形、期望版本、预览时刻)、`pages`(ops_bot 自己投递的 P0:id、内容、各次投递时刻、是否已 `/ack`)、`state`(每用户默认视图与订阅);待补 Kill 单独存 `pending_kill.json`(第 7 步),不放进 SQLite,保证 SQLite 损坏时仍可读。只读命令也记审计。
- journal:发往 hft-core 的命令本身是 `ControlCmd` 事件(带 issuer),结果是 `CmdResult`;gateway 对每个请求(含被拒的)另写审计 Note(含 `view_account`、`chat_type`);本地写(参数修改请求、预热请求、订阅变更)另发 `Annotate{LocalWrite, digest}`(发往所涉策略实例或账户所在的 hft-core 实例),使"所有命令与结果进 journal"成立([09 §5.3](../../design/09-可观测性、风控与控制面.md))。经 kill-switch 兜底的撤单与停止不经 hft-core:kill-switch 写自己的触发记录;hft-core 恢复后 ops_bot 补发 Kill 并发一条 `Annotate{OpsNote}` 记录这次兜底(`req_id`、范围、结果)。这两条以 ops_bot 自身主体(`issuer = ops_bot`)发出:记录的是 ops_bot 的兜底动作,不是某个用户的命令(主体与权限以 [hft-ctl-proto 权限表](../../crates/hft-ctl-proto/README.md#权限表)为准)。

### 配置文件字段

`/etc/hft/ops_bot.toml`,`schema = 4`。`schema` 不是 4、必填键缺失、类型不符或越出类型范围、`users` 中同一 `id` 出现两次、`plugins` 中有未发现的 entry point 名 → 启动退出 2。`schemas.dir` 存在但 `hft_py.params.load_schemas` 报 `ParamsError`(index 与文件不一致)→ 记 P1("schema 目录损坏")并照常启动,全部策略改用 `GetSchema` 取 schema(`/stop` 与只读面板不依赖 schema 目录,紧急停止通道必须可用);`schemas.dir` 不存在或缺某个策略的文件时按需用 `GetSchema`。

| 键 | 类型 | 含义 | 默认 / 来源 |
|---|---|---|---|
| `telegram.token_secret` | string | bot token 在 `secrets_dir` 中的文件名(hft-secrets 清单 `ops-bot.toml` 的项名;每台主机一个 bot,秘密在 `hft/ops/ops-bot/<host>/`,[deploy 秘密](../../deploy/README.md#秘密与实例角色));读入后立即 `unlink` | 必填 |
| `secrets_dir` | path | 本单元的秘密 tmpfs 目录 | `/run/hft/ops_bot/secrets` |
| `users` | [`{id: i64, name, role: Admin\|Trader\|Viewer, push_accounts: [string]}`] | 用户表;每人恰好一个角色 | 必填 |
| `groups` | [`{id: i64, name}`] | 允许的群(权限表"群聊"列放行的命令;紧急停止推送目标) | `[]` |
| `hft.instances` | [`{name: "prod"\|"sim", journal_cold, journal_ram, ctl_socket, config_path, heartbeat_path}`] | 本机的 hft-core 实例(dev2:`sim` 常驻、`prod` 只在联调窗口) | 必填 |
| `pipeline.socket` / `treasury.state` / `watchtower.state` | path | 其他冷进程的接口:pipeline 本地接口套接字(属组 `hft-pipeline-api`)、treasury 输出(属组 `hft-treasury`)、watchtower 状态目录(`acks.jsonl` 在其中,属组 `hft-ack`);ops_bot 经这些附加组访问,组与路径归 [deploy](../../deploy/README.md#身份路径与-s3-前缀) | `/run/hft/pipeline/api.sock` / `/var/lib/hft/treasury` / `/var/lib/hft/watchtower` |
| `killswitch.trigger_socket` | path | kill-switch 本地触发套接字 | `/run/hft/kill-switch/trigger.sock` |
| `stop.result_timeout_s` | u16 | `/stop` 等待 `CmdResult` 的上限 | 10 |
| `stop.health_timeout_ms` | u16 | 确认时试连 ctl socket 的上限 | 1 000 |
| `stop.ctl_unreachable_s` | u16 | ctl 持续不可连多久后走 `stop_core` 兜底 | 10 |
| `ops.view_stale_ms` | u32 | 视图停滞阈值:journal 尾随任务多久未前进算停滞(只用于[紧急停止](#紧急停止stop)第 6 步的只撤单判定;与心跳过期阈值 `watchdog.hb_stale_ms` 分开,后者与 `ClockStatus` 的 1 s 节拍相同,会把正常的空闲间隔误判为停滞) | 3 000 |
| `page.groups` / `page.bark_secret` | [i64] / string | `/stop` 失败与兜底的 P0 投递目标:告警群与 Bark critical key 的 secrets 名 | 必填 |
| `schemas.dir` | path | `hft-core schemas --out` 导出的 schema JSON 目录(缺失或版本不符时用 `GetSchema` 取) | `/opt/hft/current-cold/schemas/strategies` |
| `plugins` | [string] | 启用的策略插件(entry point 名;全部为 optional) | `[]` |
| `limits.writes_per_min` | u16 | 每用户写命令限速(`/stop` 不计) | 12 |
| `confirm.ttl_s` | u16 | 本地确认按钮有效期;`/stop` 的 TTL 与 gateway 一致,固定 60 | 60 |
| `push.summary_every_min` / `push.daily_at_local` / `push.tz` | u16 / `HH:MM` / string | 定时推送 | 20 / `00:00` / `Asia/Shanghai` |
| `push.note_min_level` | `P0\|P1\|P2\|Info`(`NoteLevel`) | 策略 Note 推送的最低级别 | `P2` |
| `paths.state` | path | SQLite、`pending_kill.json` 与状态 | `/var/lib/hft/ops_bot` |

## 实现说明

<details><summary>实现要点</summary>

- `python-telegram-bot` 22.x 的 `run_polling(["message", "callback_query"])`;httpx/httpcore 日志级别设为 WARNING(token 不进 journald)。工程做法沿用现有采集系统 bot(`E:\trading\Trading\python\telegram_bot`):鉴权双重检查、`uuid5` 幂等、SQLite 审计、渲染与分片、离线 `/help`。
- 面板数据:每个 hft-core 实例一个后台任务尾随其 seq journal(`hft_py.open_journal(...).tail()`),维护内存视图(按账户的权益数据、模式与 cap、目标与执行进度、`CmdResult` 索引、当前生效的参数 blob、策略 Note);命令处理只读该视图,不在命令路径上扫 journal。历史查询读 S3 Parquet。
- 健康判定:`HeartbeatBlock` 的只读映射与 `/status` 共用(经 `hft_py`,布局归 [hft-rt](../../crates/hft-rt/README.md));心跳过期阈值取该实例 hft-core 配置的 `watchdog.hb_stale_ms`(与 kill-switch 同一口径)。`/stop` 的确认处理器不经过 journal 尾随任务,避免 journal 卡住时连判定都做不了。
- schema 驱动:`schema.py` 用 `hft_py.params.load_schemas` 载入 schema 与描述符,按 `FieldDesc` 生成渲染器与输入校验(单位换算、范围检查与枚举名都经 `hft_py.params.parse_value`/`format_value`);参数 blob 的解码用 `hft_py.params.decode`,策略命令载荷的编码用 `hft_py.params.encode_cmd`——它们包装 hft-strategy-sdk 的 `params` 模块,是全仓唯一的编解码实现;本包不写任何解析或编码逻辑。
- 发命令:hft-ctl 的纯 Python 客户端 `hftctl.py`(只用标准库);`send(preview=…)`/`confirm(…, state_version=<v> | None)` 返回 gateway 响应,超时后按 `cmd_id` 补查;journal 尾随任务按 `cmd_id` 匹配 `CmdResult` 更新消息。
- kill-switch 触发:`ks_trigger.py` 只用标准库 Unix socket;每次请求(`cancel` 或 `stop_core`)一个新的 `req_id`(uuid4),等受理回执 ≤ 1 s、完成回执 ≤ kill-switch 的核验时限(`stop_core` 另加 hft-core 的 `TimeoutStopSec`);结果写本地审计。
- 兜底动作的选择按[紧急停止](#紧急停止stop)第 6 步的条件与重建规则读内存视图中 `prod` 实例的状态;判定与结果写审计。
- 待补 Kill:`pending_kill.py` 在发 `stop_core` **之前**原子写 `pending_kill.json`,进程启动时先读它;journal 尾随任务看到 `prod` 心跳恢复且 ctl 可连时,先按第 7 步查该范围在条目时刻之后是否已被 Kill 又 Resume,再决定补发或只写 Annotate;补发与 `/resume` 的本地拒绝都以该文件为准。
- 插件:经 entry point 组 `hft_ops_bot.plugins` 发现;只注册命令、面板与推送格式。

**规划的源文件**

| 文件 | 职责 |
|---|---|
| `src/hft_ops_bot/main.py` | 启动、配置、polling、插件发现 |
| `src/hft_ops_bot/auth.py` | 用户/群/角色判定、限速 |
| `src/hft_ops_bot/commands/*.py` | `status.py`、`accounts.py`、`risk.py`、`targets.py`、`control.py`(mode/kill/resume/hedgefix/adopt/baseline/transfer/notice/shutdown)、`stop.py`(紧急停止)、`sim.py`、`params.py`、`strategy_cmd.py`(`/cmd`、`/notes`)、`warmup.py`、`treasury.py` |
| `src/hft_ops_bot/schema.py` | schema 与描述符缓存、按 `Unit` 渲染与解析 |
| `src/hft_ops_bot/confirm.py` | PREVIEW → 确认(同一 from.id)→ 带期望版本(Viewer 的 `/stop` 不带)以同一规范形提交;本地短 id |
| `src/hft_ops_bot/health.py` | `HeartbeatBlock` 读取、ctl socket 试连 |
| `src/hft_ops_bot/ks_trigger.py` | kill-switch 本地触发套接字客户端(`stop_core`) |
| `src/hft_ops_bot/pending_kill.py` | 待补 Kill 文件、恢复后补发、`/resume` 前置检查 |
| `src/hft_ops_bot/page.py` | `/stop` 失败与兜底的 P0 投递(Telegram + Bark) |
| `src/hft_ops_bot/view.py` | journal 尾随与内存视图(每实例一份) |
| `src/hft_ops_bot/push.py` | 紧急停止通知、成交与目标推送、策略 Note、定时报告、日报 |
| `src/hft_ops_bot/render.py` | HTML、分片、账户标签(实盘/模拟/测试网) |
| `src/hft_ops_bot/audit.py` | SQLite 审计 |

</details>

## 延迟与资源契约

冷进程,无热路径约束;单元在 `hft-core.slice`([deploy slice](../../deploy/README.md#slice-与资源切分))。

| 指标 | 目标 |
|---|---|
| 降级命令:收到 update → `ControlCmd` 写入 ctl socket | < 500 ms(不含 Telegram 网络) |
| `/stop`:确认点击 → `ControlCmd` 写入 ctl socket(正常路径)或触发请求写入 kill-switch 套接字(兜底) | < 500 ms(健康判定 ≤ `stop.health_timeout_ms` 另计);预览生成 < 2 s |
| `/stop` 失败或兜底 → P0 发出 | 判定成立后 < 2 s |
| 面板命令响应 | < 2 s(内存视图) |
| 成交推送:journal 事件 → 推送发出 | < 5 s |
| CPU / 内存 | 上限在 systemd 单元中设置;`dev2` 的取值归 [hft-core](../../apps/hft-core/README.md#内存与-journal-段按主机类唯一归属) |

## 测试与验收

用例与验收见 [TESTS.md](TESTS.md)。

## 依赖

- 内部:`hft-py`(journal 读取、`HeartbeatBlock` 只读映射、`pnl`、`params.load_schemas`/`load_schema`/`decode`/`encode_cmd`)、hft-ctl 的 `hftctl.py`、`python/_generated`(`gen-schema` 生成)、发布归档中的 `schemas/strategies/`;kill-switch 的本地触发套接字(协议与 `stop_core` 归 [kill-switch](../../apps/kill-switch/README.md))。
- 外部:`python-telegram-bot==22.3`(及其锁定依赖)、`httpx`(Bark)、`polars`、`duckdb`、`pyarrow`;Python `>=3.12,<3.13`。读 S3 Parquet 的凭证:dev2 与升级主机上都是凭证代理写的 `/run/hft/creds/hft-opsbot/credentials`,本进程访问不到 IMDS([deploy](../../deploy/README.md#imds-锁定与凭证代理));凭证每 45 min 轮换,DuckDB 的 S3 secret 与 polars 的 `storage_options` 不会自己刷新——每次历史查询前比较凭证文件的 mtime,变化时重建它们。
- 禁止:交易所 SDK 与 key;`subprocess` 调用 `systemctl`/`sudo`;写 hft-core 的任何文件(watchtower 的 `acks.jsonl`、pipeline 本地接口与 kill-switch 触发套接字是这些进程提供的接口);按策略名或交易所名写分支(插件除外);自写参数或命令载荷的编解码。

## 延伸阅读

- [round8-decisions](../../design/_work/round8-decisions.md)(R8-8 第 3 条、R8-9 第 5 条:老板紧急停止)、[09 §5.3、§6](../../design/09-可观测性、风控与控制面.md)、[12 §1](../../design/12-风险与未决问题.md)
- [viewer-emergency-stop](../../.agents/notes/proposed/architecture/2026-10-03-viewer-emergency-stop.md)、[imds-lockdown](../../.agents/notes/proposed/architecture/2026-10-03-imds-lockdown.md)、[control-plane](../../docs/subsystems/control-plane.md)(命令、角色、确认)、[hft-ctl](../../crates/hft-ctl/README.md)、[kill-switch](../../apps/kill-switch/README.md)、[hft-strategy-sdk](../../crates/hft-strategy-sdk/README.md)(schema 与描述符)、[hft-sim](../../crates/hft-sim/README.md)(模拟账户)、[watchtower](../../apps/watchtower/README.md)、[pipeline](../pipeline/README.md)、[treasury](../treasury/README.md)、[incident-response](../../docs/runbooks/incident-response.md)
- 现有:Trading telegram_bot(本仓库之外:`Trading/python/telegram_bot`)、telegram-ops runbook(本仓库之外:`Trading/docs/runbooks/telegram-ops.md`)

## 已知限制与后续工作

- 群聊里只能发[权限表](../../crates/hft-ctl-proto/README.md#权限表)"群聊"列放行的命令;`flatten`、`hard` 与一切升级命令必须私聊。
- Viewer 改变状态的写权限只有 `/stop`(另可 `/note` 备注):它不能恢复、不能平仓、不能 HARD;需要这些动作时由交易员执行。
- `/stop` 的 `stop_core` 兜底停止的是整个 `hft-core@prod` 实例,范围大于单账户的 `/stop a`;恢复后其他账户按冷启动规则由交易员 `/resume recovery`,只有原范围带待补 Kill。
- kill-switch 自身不可用、或 polkit 停止失败时,兜底只剩 P0 与人工 `kill-switch fire` / `systemctl stop`(incident-response G、R2)。
- 参数修改只能经 pipeline 覆盖项生效;某个策略实例没有 pipeline 插件时,覆盖项由 pipeline 框架以"只有 policy 默认值 + 覆盖项"的方式下发([pipeline](../pipeline/README.md#插件接口))。
- 权限、类别与 cap 的语义随 control-plane / hft-ctl-proto / hft-risk 变化;ops_bot 只同步提示文案,不复制规则。

### 开发备忘

<details><summary>给实现者</summary>

- 实现顺序:鉴权 + 只读面板(`/status`、`/accounts`、`/p`、`/risk`)→ `/kill`、`/stop`(含健康判定、`stop_core` 兜底、待补 Kill 与 P0 投递)、`/mode`、`/resume`、`/adopt`、`/baseline` → 确认流程 → `/hedgefix`、`/targets` → schema 面板与按实例/校准对的 `/params set` → `/notes`、`/cmd`、`/warmup` → 模拟账户命令与日报 → 推送;策略插件最后、按需。
- 新 bot 使用新 token,不与现有旧 bot 共用;旧 bot 继续服务旧框架。dev2 与升级主机各运行一个 ops_bot,各用自己的 bot 与 token(同一 token 被两个进程 `run_polling` 会互相报 409 冲突);M7 之后 dev2 上的每月完整 kill 演练用 dev2 的 bot。
- `hft_py.params.encode_cmd`、`HeartbeatBlock` 读取函数的名字以 [hft-py](../../crates/hft-py/README.md) 为准;`hftctl.confirm` 的签名以 [hft-ctl](../../crates/hft-ctl/README.md) 为准。
- **待核实**:Bark critical 推送在静音模式下的响铃行为与限速;核实方法:用测试 key 在值班手机上实测。
- **待核实**:冷进程实测内存(polars、duckdb、pyarrow 的导入与一次历史查询)与 dev2 内存表中本单元一行(归 hft-core)是否相符;核实方法:dev2 上 `systemd-cgtop` 与 `memory.peak` 记录一周。

</details>
