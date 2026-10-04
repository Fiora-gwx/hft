---
description: "独立的最小冷进程:监视 hft-core 心跳(共享内存计数 + journal 提交游标),心跳消失时用独立 REST 凭证按账户撤全部挂单;另接受 ops_bot(只认其 uid)经本地套接字的撤单与 stop_core 请求(撤单、写持久停机标记后经 polkit 只允许停 hft-core@prod,Drain、不平仓;hft-core 下次启动读到标记即带 Kill 起步;停机失败时标记作废为 .aborted);启动自检核对每把 key 的 UID;只覆盖 Live 账户,跳过 Sim 账户;只撤单、不平仓"
kind: app
tier: cold
scope: core
status: spec
---

# kill-switch

## 概要

`kill-switch` 是"停得安全"的最后一道:hft-core 崩溃或停摆时,有死手接口的交易所会在倒计时到期后自己撤单,没有死手接口的(core venue 中的 Binance 现货、Bitget)不会([09 §4.2](../../design/09-可观测性、风控与控制面.md))——kill-switch 为它们兜底,并对有死手的 venue 并行再撤一次。它只做五件事:监视每个 hft-core 实例的 `HeartbeatBlock` 与 seq journal 提交游标;心跳消失时用**自己的** REST 凭证对每个 **Live 账户**在其交易的每个 venue 撤全部挂单并核验;接受 ops_bot 经**本地触发套接字**发来的请求(只认 ops_bot 的系统 uid,用于控制面不可用时老板与交易员的紧急停止):`cancel` 只撤单,`stop_core` 撤单、写持久停机标记后经 polkit 停掉 `hft-core@prod`(SIGTERM → Drain,不平仓);启动自检用每把撤单 key 查交易所 UID,与 refdata 登记的 `venue_uid` 不符即 P0 并退出 2;把动作告警出去。Sim 账户没有交易所 key、没有真实挂单,一律跳过;只含 Sim 账户的 `sim` 实例目标表为空,不需要任何凭证,停摆时只告警。它**只撤单、不平仓**;不读业务状态、不下单、不连 ctl-gateway、不依赖 tokio;hft 的 crate 只用 `hft-rt` 的 `HeartbeatBlock` 只读映射;代码量目标 < 2,000 行,便于逐行审查。

## 目录

- [使用本组件](#使用本组件)
  - [判活与触发](#判活与触发)
  - [撤单目标](#撤单目标)
  - [撤单动作](#撤单动作)
  - [本地触发(ops_bot)](#本地触发ops_bot)
  - [停止 hft-core(stop_core)](#停止-hft-corestop_core)
  - [身份核验(UID)](#身份核验uid)
  - [各所死手能力与兜底](#各所死手能力与兜底)
  - [配置文件字段](#配置文件字段)
- [实现说明](#实现说明)
- [延迟与资源契约](#延迟与资源契约)
- [测试与验收](#测试与验收)
- [依赖](#依赖)
- [延伸阅读](#延伸阅读)
- [已知限制与后续工作](#已知限制与后续工作)

## 使用本组件

```
kill-switch run      --config /etc/hft/kill-switch.toml            # hft-kill-switch.service,常驻
kill-switch --check  --config …                                     # 只加载与校验(配置、目标表、凭证引用),不连接交易所、不打开套接字
kill-switch status   --config …                                     # 每实例判活状态、上次动作、生效的目标表、各目标 UID 核验状态、未消费与已作废的停机标记
kill-switch dry-run  --config … [--account <name>] [--venue bitget-spot]  # 只查挂单,打印将撤什么
kill-switch fire     --config … --account <name|all> --venue <v|all> --reason "<文本>" [--yes]   # 人工触发,不看心跳
kill-switch selftest --config …                                     # 每个凭证调一次只读签名接口并核对 UID
```

`fire` 用于 hft-core 心跳仍在但无法经控制环撤单的情况(如 ctl-gateway 失效、gw 线程卡死),由有主机权限的人执行,步骤见 [incident-response](../../docs/runbooks/incident-response.md);ops_bot 走[本地触发](#本地触发ops_bot),不经 shell;不带 `--yes` 时打印将撤的目标并从终端读一行,输入 `yes` 才执行,其他输入退出 1、不发任何请求。退出码:1 = `fire`、`selftest` 的动作未完成(核验超时仍有残留挂单、凭证查询失败);2 = 配置错误、UID 不符、已发布格式版本不符;3 = 主机前置条件(`egress_bind` 不是本机地址、`trigger.socket` 所在目录不存在);本 app 不使用 4(它不恢复 journal)。单元按全仓统一设置 `RestartPreventExitStatus=2 3 4`([deploy](../../deploy/README.md#systemd-单元)),2 与 3 不进入重启。

### 判活与触发

每个被监视实例一个状态机,轮询周期 `poll_ms`(100 ms):

```
            shm 不存在 / state=Stopped
   ┌────────────────────────────────────────┐
   ▼                                        │
 Idle ──(state∈{Warmup,Running,Draining} 且计数前进)──▶ Watching
                                                          │ 任一信号 hb_stale_factor × hb_stale_ms 内不前进
                                                          ▼
                                                       Suspect ──(停滞的信号恢复前进)──▶ Watching
                                                          │ 两个信号都已 hb_stale_factor × hb_stale_ms 不前进
                                                          │   或:单一信号持续 hb_stale_hard_ms
                                                          ▼
                                                        Fire ──▶ Verify ──(全部目标挂单为 0)──▶ Fired
                                                                   │ 超过 verify_timeout_s 仍有挂单     │
                                                                   └──▶ Fired(P0:残留挂单清单)        │
   Fired ──(新 (pid, boot_id) 的心跳计数前进,状态为 Warmup 或 Running)──────────────────────────────▶ Watching
```

- 信号一:`hft_rt::HeartbeatBlock` 区 0 的 sequencer 心跳计数与区 1 `HkLine` 的实例状态(类型与布局归 [hft-rt](../../crates/hft-rt/README.md),位置与状态迁移见 [hft-core](../hft-core/README.md#heartbeatblock))。信号二:该实例 `seq` journal 当前热段段头的 `committed_len`([journal-format 段文件布局](../../docs/subsystems/journal-format.md)),只读映射段头 4 KiB;段 `retired` 或新段出现时切换。二者都停才立即触发;只有一个停时等到 `hb_stale_hard_ms` 再触发,并 P0 告警"信号不一致"。各时长按"自上次观察到前进起 ≥ 该时长"判定,以 `poll_ms` 为粒度;`Watching` 与 `Suspect` 中读到 `state = Stopped` 或心跳文件不存在 → `Idle`。
- `state = Stopped` 是干净退出(hft-core 已按 Drain 处理挂单),不触发;`state` 停在 `Draining` 且计数不前进(优雅退出超时被 SIGKILL)会触发。dev2 护栏停 hft-core 时走同一判定,kill-switch 本身不在 hft 的 slice 内,不会被护栏一并停掉([hft-core dev2 护栏](../hft-core/README.md#dev2-护栏))。
- `Fired` 是锁存态:同一 (pid, boot_id) 只触发一次。**回到 `Watching`**:出现新的 (pid, boot_id),其心跳计数开始前进且状态为 `Warmup` 或 `Running`——不等闸门放行(`Warmup` 期间 hft-core 已可能有挂单)。进程启动时若发现某实例已处于 stale 状态,按正常流程触发:状态 ∈ {Warmup, Running, Draining} 的实例直接进入 `Watching`,停滞计时从 kill-switch 启动时刻起算;`latch.json` 记录该 (pid, boot_id) 已触发过的直接进入 `Fired`。
- **交接**:撤单进行中出现同一实例的新 (pid, boot_id) 且心跳前进,kill-switch 完成**当前这一轮**(每个目标一次撤单 + 一次核验)后停止,剩余挂单交给新进程(冷启动后由对账与查单收敛)。
- **映射健壮性**:每次轮询比较心跳文件与热段文件的 inode;变化即重新映射;映射期间文件被截断导致的 SIGBUS 被捕获,标记映射失效并在下一轮重新映射,判活不中断、不误触发。已映射的文件被删除(`fstat` 的 `st_nlink = 0`,如 `hft-shm-release` 在干净 Drain 后释放热段)→ 立即解除该映射(不占住已释放的 tmpfs 或大页);心跳文件被删除按"shm 不存在"回到 `Idle`。
- 进程存活检查(`kill(pid, 0)`、`/proc/<pid>/stat`)只写进告警详情,不参与判定——停摆的进程也是存活的。
- 本地触发不改变本状态机:触发后的撤单轮次与心跳判活互不影响,同一 (pid, boot_id) 之后停摆仍会按上图 `Fire`;`stop_core` 引起的干净 Drain 以 `Stopped` 结束,不触发。

### 撤单目标

- 目标表 = 被监视实例的 hft-core 配置中**每个 Live 账户 × 它在 `venues.<v>.gw.accounts` 中出现的每个 venue**;账户类型与 `venue_uid` 取自 refdata `AccountMeta`(kill-switch 只读 hft-core 配置与 refdata blob 的账户表)。启动时与每次检测到 hft-core 配置变更时重建(housekeeping 线程每 1 s 比较 hft-core 配置文件与 refdata blob 的 (inode, mtime);重建出的新目标先过[身份核验](#身份核验uid);新目标缺凭证同启动时退出 2;文件不可解析 → 保留原表并 P0),`status` 打印生效的表。
- **Sim 账户永远不是目标**:配置中为 Sim 账户写凭证 → 退出 2。
- **目标表为空的实例**(全部账户为 Sim,即 `instance.name = "sim"` 的 hft-core,dev2 与升级主机上都是如此):照常判活,不需要也不接受任何凭证;状态机到 `Fire` 时不发任何请求,只发一条 P1 告警(实例名、`(pid, boot_id)`、"无 Live 目标"),直接进 `Fired`。这不是配置错误,不退出。
- **dev2 联调窗口**:指向测试网端点的 `prod` 角色实例,其 Live 账户是测试网/模拟盘账户;kill-switch 用测试网 key 与测试网 `base_url` 监视它,规则与正式实盘相同。
- 每个账户只绑定一个策略实例,交易所账户上的全部挂单都属于本系统该账户([hft-core 多账户](../hft-core/README.md#多账户)),所以撤"该 (account, venue) 的全部挂单",不做按 client id 的过滤。
- 每个目标必须有凭证;缺凭证 → 启动失败(退出 2)。`exclude` 只允许排除有交易所侧死手的 venue,且每次触发都在告警里列出被排除的目标。

### 撤单动作

对每个目标并行执行(每目标一个 OS 线程,阻塞 HTTP);心跳触发、`fire` 与本地触发共用这一套:

1. 查挂单:`GET` 全部挂单(不带 symbol),得到 symbol 集合与订单数。
2. 撤单:按 symbol 调"撤该 symbol 全部挂单"接口;该所没有时逐单撤。
3. 核验:再次 `GET` 全部挂单,为空则该目标完成;否则按 `retry_backoff_ms`(200, 400, 800, … 上限 5,000)重复 2–3,直到 `verify_timeout_s`(60)。
4. 限速:429/418/等价错误码 → 按响应的退避提示等待;不因限速放弃,直到超时。
5. 每个请求与结果(不含签名与 key)追加到 `<state>/actions.jsonl`;动作开始、每目标结果、整体结果各发一条告警。
6. 同一目标已有轮次在进行时,新的触发并入该轮(不并发两轮),回执引用正在进行的轮次。

只撤单,不平仓:仓位在 hft-core 恢复后由交易员决定(强平风险由交易所保证金机制与 hft-core 恢复后的 MarginStress 规则处理)。

### 本地触发(ops_bot)

控制面不可用时 ops_bot 的 `/stop` 经本进程执行。何时走本地触发、何时用 `cancel` 或 `stop_core`(含需要第二次确认的情形)、待补 Kill 的记录与补发,唯一归属 [ops_bot](../../python/ops_bot/README.md);本节是 kill-switch 侧的契约:

- **套接字**:Unix 域 `SOCK_SEQPACKET`,路径 `trigger.socket`(`/run/hft/kill-switch/trigger.sock`,在本单元的 `RuntimeDirectory` 下),属主 `hft-ks:hft-ks-trigger`、权限 0660,`hft-ks-trigger` 组只有 `hft-ks` 与 `hft-opsbot`(用户、组与单元设置归 [deploy](../../deploy/README.md));不监听任何网络端口。
- **只认 ops_bot**:每个连接先取 `SO_PEERCRED`,`uid` 必须等于启动时由 `trigger.ops_bot_user` 解析出的 uid(root 与其他 hft 用户同样拒绝);不符 → 不读载荷即关闭、写 `actions.jsonl`(`trigger_denied`,对端 uid/pid)、P1(同一对端 10 min 内只告警一次)。
- **请求**(一个报文,≤ 1 KiB,UTF-8 JSON):

| 字段 | 类型 | 含义 |
|---|---|---|
| `v` | u8 | 协议版本,当前 1 |
| `req_id` | string | UUID;同一 `req_id` 在 `trigger.dedupe_s`(600)内重复(自首次收到起 ≤ 600 s)→ 返回已有结果,不重做(原请求仍在进行 → 回同一轮的 `Accepted`,完成时同样收到 `Done`) |
| `action` | string | `cancel`(缺省)或 `stop_core`([停止 hft-core](#停止-hft-corestop_core)) |
| `instance` | string | `prod`、`sim` 或 `*`(全部被监视实例);`action = stop_core` 时只能是 `prod` |
| `account` | string | 账户名或 `*`(该实例全部 Live 账户);决定撤单范围 |
| `issuer` | string | `tg:<from.id>`,只写审计 |
| `reason` | string | ≤ 200 字节,只写审计与告警 |

- **校验**(按序取第一个失败):报文超过 1 KiB、不是 UTF-8 JSON、缺 `action` 以外的任一字段、字段类型不符、`v ≠ 1`、`action` 不是两个取值之一、`reason` 超过 200 字节 → `BadRequest`;`instance` 不是被监视的实例名或 `*` → `UnknownInstance`;`action = stop_core` 而 `instance ≠ prod` → `StopNotAllowed`;账户名不在该实例(`*` 时任一实例)→ `UnknownAccount`;`instance = sim`、或账户是 Sim 账户 → `SimAccount`;范围解析出的目标(去掉 `exclude` 后)为空 → `NoTarget`。
- **回执**(同一连接上的报文):收到后 ≤ 100 ms 回 `Accepted{req_id, action, targets: [{account, venue}]}` 或 `Rejected{req_id, reason}`(`BadRequest`、`UnknownInstance`、`UnknownAccount`、`SimAccount`、`NoTarget`、`StopNotAllowed`);撤单轮次(`stop_core` 还包括写标记、停机与复核轮)结束后回 `Done{req_id, action, targets: [{account, venue, before, remaining, ok}], core_alive, core_stop?, marker?, elapsed_ms}` 并关闭连接。`core_alive` = 撤单轮次结束时该实例心跳仍在前进;`core_stop` 与 `marker`(停机标记已落盘且未作废)只在 `stop_core` 时出现(取值见下节)。
- **`cancel` 的动作**:目标 = 该账户(或全部 Live 账户)在被监视实例中的全部目标,执行[撤单动作](#撤单动作) 1–6;不看心跳状态,不改变判活状态机;只撤单,不下单、不平仓、不改任何配置。`instance = sim` 或只命中 Sim 账户 → `Rejected{SimAccount}`/`NoTarget`。
- 告警:每次触发(含被拒)发一条,内容为 `issuer`、动作、范围、结果、`core_alive` 与 `core_stop`。

### 停止 hft-core(stop_core)

`action = stop_core` 是控制面不可用时唯一能挡住"仍在运行的 hft-core 继续下新单"的兜底:

1. **先撤单**:按 `account` 范围执行一轮[撤单动作](#撤单动作)(与 `cancel` 相同)。
2. **写停机标记**(在 `systemctl stop` 之前):把 `/var/lib/hft/kill-switch/stop-marker.json` 写成 `{"v": 1, "req_id": …, "scope": "*" | "<账户名>", "utc_ns": …}`(`scope` 取请求的 `account`;账户名即 refdata `AccountMeta.label`),先写 `.new`、`fsync` 文件、rename、再 `fsync` 目录。路径固定,不随本 app 的状态目录配置改变;hft-core 在启动步 5 读它:写一条 Input `ControlCmd{Kill{scope, 非 HARD, 不带 flatten}}`(issuer `cli:systemd`)后把它改名为 `.consumed`(读取与放行规则归 [hft-core](../hft-core/README.md#启动顺序))。目录 `/var/lib/hft/kill-switch` 为 `hft-ks:hft-stopmark`、`2770`(setgid),由本单元的 `ExecStartPre=+` 建立;标记以 `0660` 创建;`hft-stopmark` 组只有 `hft-ks` 与 `hft-core@prod` 的运行用户,所以 hft-core 能读标记并在同一目录内改名(用户、组与单元设置归 [deploy](../../deploy/README.md))。已有一份未消费的标记而 `scope` 不同 → 合并为 `"*"`(只放宽到更大的范围),`req_id` 取本次;两次请求都写 `actions.jsonl`。写入失败(磁盘满、权限)→ **照常停机**,`Done.marker = false` 并 P0(此时只剩 ops_bot 的待补 Kill 接续这次停止);成功为 `true`。除第 6 条的作废外,kill-switch 不删除或改名标记。
3. **再停机**:执行 `systemctl stop --no-block <trigger.stop_core_unit>`(固定为 `hft-core@prod.service`;以 `execve` 直接调用 `/usr/bin/systemctl`,参数固定,不经 shell)。授权只来自 polkit 规则:`hft-ks` 用户只被允许对这一个单元执行 `stop`(规则文件归 [deploy](../../deploy/README.md));kill-switch 不持有其他任何 systemd 权限。hft-core 收到 SIGTERM 后按 `shutdown.cancel_open_orders` 执行 `Drain`(不平仓,[hft-core 优雅退出](../hft-core/README.md#优雅退出与-kill-语义))。
4. **等结果**:在 `trigger.stop_wait_s`(默认 30,> hft-core 的 `TimeoutStopSec`)内观察该实例的 `HeartbeatBlock.state` 与单元状态(以 `HkLine.pid` 的进程是否已不存在判定单元已停),得到 `core_stop`:`Stopped`(Drain 完成、`state = Stopped`)、`Inactive`(单元已停但 `state ≠ Stopped`,即 Drain 超时被终止;此时判活状态机照常对它 `Fire`)、`Timeout`(到时仍未停)、`Failed`(`systemctl` 返回非 0 或超时,含 polkit 拒绝)、`Denied`(polkit 拒绝;只在 dev2 核实 stderr 文本之后启用,见实现说明的停机调用,启用前不产出)。
5. **再核验一次**:停机结束(或超时)后对同一范围再执行一轮撤单与核验,覆盖第 1 步与停机之间可能新挂的单。
6. **停机失败时作废标记**:`core_stop ∉ {Stopped, Inactive}`(`Timeout`、`Denied`、`Failed`)→ 把 `stop-marker.json` 改名为 `/var/lib/hft/kill-switch/stop-marker.json.aborted`(覆盖上一份;rename 后 `fsync` 目录;合并进来的旧范围一并作废),`Done.marker = false`,P0(详情:结果、`systemctl` 的退出码与"停机标记已作废")。hft-core 启动只读 `stop-marker.json`([hft-core 启动顺序](../hft-core/README.md#启动顺序)),所以这次未生效的停止不会在之后某次重启时注入 Kill;是否另发 Kill 由人决定([incident-response R](../../docs/runbooks/incident-response.md#r-老板紧急停止stop之后))。任何结果都写 `actions.jsonl`。kill-switch **从不启动**任何单元,也不对 `sim` 实例或其他单元执行任何 systemd 操作;恢复 hft-core 是人的授权操作——它重启后带 Kill cap 起步(来自停机标记),交易员 `/resume kill` 才恢复开仓;ops_bot 的待补 Kill 是第二道接续(归 ops_bot)。

### 身份核验(UID)

撤错账户的挂单与漏撤同样危险,所以每把撤单 key 在使用前核对它属于哪个交易所账户:

1. `run` 启动时(进入 `Watching` 之前)与 `selftest`:对每个 (Live 账户, venue) 的凭证调用该所账户信息接口,取交易所 UID,与 refdata `AccountMeta` 中该 (account, venue) 的 `venue_uid` 比较。
2. **不符** → 经本进程的告警通道发 P0(账户、venue、两个 UID 只显示末 4 位),然后退出 2(配置错误,systemd 不重启;kill-switch 缺位由 watchtower `ProcessDown` 另报 P0)。
3. **查询失败**(超时、5xx、限速)→ 该目标标 `UidUnverified` 并 P0,每 30 s 重试直到核验通过;期间该目标照常参与撤单(只撤单的动作对错账户的伤害小于漏撤),`status` 显示未核验目标。
4. 配置变更后新出现的目标,先核验再加入目标表(不符同样退出 2)。

| venue | UID 来源(端点与字段以各 `venue-*` README 为准,全部待核实,方法见开发备忘) |
|---|---|
| `binance-usdm` / `binance-spot` | `GET /api/v3/account` 的 `uid`(USDⓈ-M 的 key 是否可调此现货端点待核实,方法见开发备忘) |
| `okx-swap` / `okx-spot` | `GET /api/v5/account/config` 的 `uid`(子账户 UID,不是 `mainUid`) |
| `bitget-usdt-futures` / `bitget-spot` | 账户信息接口的 `userId`(UTA 路径待核实,方法见开发备忘) |

### 各所死手能力与兜底

| venue | 交易所侧死手(语义以 `venue-*` README 为准) | kill-switch 默认 | 撤单接口(待核实项见开发备忘) |
|---|---|---|---|
| `binance-usdm` | `countdownCancelAll`,按 symbol 设置,hft-core 对每个有挂单的 symbol 刷新(端点与语义待核实,[venue-binance 开发备忘 10](../../crates/venue-binance/README.md#开发备忘)) | 兜底也撤 | `GET /fapi/v1/openOrders`;`DELETE /fapi/v1/allOpenOrders?symbol=` |
| `binance-spot` | **无** | 撤(唯一兜底) | `GET /api/v3/openOrders`;`DELETE /api/v3/openOrders?symbol=` |
| `okx-swap` / `okx-spot` | `cancel-all-after`(对现货是否生效待核实,方法见开发备忘) | 兜底也撤 | `GET /api/v5/trade/orders-pending`;`POST /api/v5/trade/cancel-batch-orders` |
| `bitget-usdt-futures` / `bitget-spot` | **无** | 撤(唯一兜底) | UTA 查未成交订单与按 symbol 撤全部(端点待核实,方法见开发备忘) |
| 可选扩展:Gate、Bybit、KuCoin、Hyperliquid | 见各 `venue-*` | 启用时补一行 | 启用时核实(方法同开发备忘) |

交易所死手是第一道(不依赖我们的任何进程),kill-switch 是第二道;对有死手的所同时执行两道,撤单更快且互不依赖。能力位以 refdata `VenueCaps.dms` 为准。

### 配置文件字段

`/etc/hft/kill-switch.toml`,`schema = 1`。所有字段必填(无默认值的安全组件)。本文件与凭证只在启动时读取(目标表另随 hft-core 配置变更重建,见[撤单目标](#撤单目标));改动它们(如 M7 → M8 换成正式账户与正式 key)后须重启本单元,步骤归 [launch](../../docs/runbooks/launch.md)。

| 键 | 类型 | 含义 | 推荐值 / 来源 |
|---|---|---|---|
| `poll_ms` | u32 | 判活轮询周期 | 100 |
| `hft_core_config` | [path] | 被监视实例的 hft-core 配置(只读;每个实例一项,实例名 `prod` 或 `sim`):停摆窗口取其 `watchdog.hb_stale_ms`(唯一归属),心跳与热段路径取其 `paths.heartbeat_shm` 与 `paths.journal_ram`,目标表取其 Live 账户与 `venues.<v>.gw.accounts` | `/etc/hft/hft-core-<instance>.toml`(每实例一项) |
| `refdata_blob` | path | 读 `AccountMeta.kind`、`venue_uid` 与 `VenueCaps.dms` | 与 hft-core `blobs.refdata` 同一文件 |
| `hb_stale_factor` | u8 | 两信号都停 `hb_stale_factor × hb_stale_ms` 即触发 | 2 |
| `hb_stale_hard_ms` | u32 | 单信号停多久触发 | 5 000 |
| `verify_timeout_s` | u32 | 撤单核验上限 | 60 |
| `exclude` | [`{account, venue}`] | 不撤的目标(只允许有交易所侧死手的 venue) | `[]` |
| `trigger.socket` | path | 本地触发套接字 | `/run/hft/kill-switch/trigger.sock` |
| `trigger.ops_bot_user` | string | 唯一允许连接触发套接字的系统用户(ops_bot 的运行用户);启动时解析为 uid,不存在 → 退出 2 | `hft-opsbot`([deploy](../../deploy/README.md) 的用户表) |
| `trigger.stop_core_unit` | string | `stop_core` 停止的单元;加载时必须等于 `hft-core@prod.service`,否则退出 2 | `hft-core@prod.service` |
| `trigger.stop_wait_s` | u32 | `stop_core` 等待停机结果的上限(> hft-core 的 `TimeoutStopSec`) | 30 |
| `egress_bind` | IP | 撤单与查询请求的源地址(`bind()` 后连接) | `[egress].prod` 的私有 IP([deploy 出口 IP](../../deploy/README.md#出口-ip));dev2 为其唯一出口地址 |
| `trigger.dedupe_s` | u32 | `req_id` 去重窗口 | 600 |
| `[credentials.<account>.<venue>]` | `{secret, base_url}` | 撤单 key(secrets 文件名)与 REST 基址(联调窗口为测试网基址);只允许 Live 账户;全部被监视实例都只有 Sim 账户时整表为空 | [`hft-secrets`](../../tools/README.md#hft-secrets) 预取 |
| `alert.telegram_chat_id` / `.telegram_token_secret` / `.bark_key_secret` | | 独立告警通道 | 与 watchtower 相同的群,独立 token |
| `paths.state` | path | `actions.jsonl`、锁存状态 | `/var/lib/hft/kill-switch`(停机标记的路径固定为该目录下的 `stop-marker.json`,不随本键改变) |

凭证要求:每 (Live 账户, venue) 一把**只用于 kill-switch** 的 key,权限为交易(大多数所没有"只撤单"权限),禁提现,IP 白名单只含 `[egress].prod` 的 EIP(与 hft-core 交易 key 同一出口,出口表归 [deploy](../../deploy/README.md#出口-ip));与 hft-core 的交易 key、reconciler 的只读 key 分开([python/AGENTS.md](../../python/AGENTS.md) 的持 key 划分)。正式账户开通时同时开这把 key([launch](../../docs/runbooks/launch.md))。

## 实现说明

<details><summary>实现要点</summary>

- 单进程、无异步运行时:主线程跑判活状态机;`trigger` 线程阻塞在触发套接字的 `accept`,每个连接在该线程内校验与解析后把请求交给撤单编排;触发时每个目标派生一个线程执行撤单,主线程继续判活;`housekeeping` 线程每 1 s `sd_notify(WATCHDOG=1)`,并每 30 s 重试 `UidUnverified` 的目标。
- 撤单编排是唯一入口:心跳 `Fire`、`fire` 子命令与本地触发都经它;它按 (account, venue) 持有"进行中轮次"表,实现[撤单动作](#撤单动作)第 6 条的并入。`stop_core` 是编排之上的一个顺序动作(撤单 → 写停机标记 → 停机 → 等结果(失败则作废标记)→ 再撤一轮),同一时刻最多一个进行中的 `stop_core`,重复请求并入它(范围不同时标记按[停止 hft-core](#停止-hft-corestop_core)第 2 条合并为 `"*"`,复核轮按 `"*"` 撤单)。
- 停机调用:`std::process::Command` 以固定路径 `/usr/bin/systemctl` 与固定参数 `["stop", "--no-block", <stop_core_unit>]` 执行,清空环境变量,超时 10 s;退出码 0 之外一律记 `Failed`(含 10 s 超时),stderr 原文写进 P0 详情。`Denied` 的判定(stderr 含 `Access denied` 或 `Interactive authentication required`)待核实(方法:开发备忘第 2 条;不同 systemd 版本的文本可能不同):dev2 核实文本之后才启用,启用即改本条与对应用例。kill-switch 单元的沙箱设置需允许经 D-Bus 调用 systemd(单元设置归 deploy)。
- HTTP:`ureq` + rustls,连接先 `bind()` 到 `egress_bind` 再连;预先建立并保活每个 REST 基址的连接(每 30 s 一个轻量签名 GET 保温,同时充当 selftest)。
- 签名:Binance HMAC-SHA256(kill-switch 用 HMAC 类型的 key)、OKX/Bitget HMAC-SHA256 + passphrase。每所的"查挂单/撤全部/查 UID/签名"写在 `src/venues/<x>.rs`,约 100–170 行,不依赖 `venue-*`(见依赖节理由)。
- 时钟:签名时间戳用 `SystemTime`;启动时与各所 `/time` 比较,偏差 > 1 s → P1。
- 锁存状态持久化到 `<state>/latch.json`,kill-switch 自身重启后不重复触发同一次;触发请求的 `req_id` 去重表也写在这里。
- 目标表构造:解析 hft-core 配置的 `[[accounts]]`、`venues.*.gw.accounts` 与 refdata 账户表(只读取账户类型、`venue_uid` 与 `dms` 能力位;refdata blob 的布局按 [hft-refdata](../../crates/hft-refdata/README.md) 已发布格式只读解析,校验版本)。

**规划的源文件**

| 文件 | 职责 |
|---|---|
| `src/main.rs` | 子命令、配置加载(全部字段必填)、`--check` |
| `src/watch.rs` | 判活状态机、两信号读取 |
| `src/targets.rs` | 目标表构造与校验(Live 账户、凭证、`exclude`) |
| `src/identity.rs` | 每把 key 的 UID 核验与 `UidUnverified` 重试 |
| `src/fire.rs` | 撤单编排(唯一入口)、核验、重试、并入、动作日志 |
| `src/stop_core.rs` | `stop_core` 顺序动作、停机标记的原子写入、合并与停机失败时的作废、`systemctl` 调用、停机结果判定 |
| `src/trigger.rs` | 触发套接字、`SO_PEERCRED` 校验、请求解析、回执、`req_id` 去重 |
| `src/venues/{binance_usdm,binance_spot,okx,bitget}.rs` | 每所查挂单、撤全部、查 UID、签名;可选扩展 venue 启用时加文件 |
| `src/alert.rs` | Telegram/Bark 直接投递 |
| `src/heartbeat.rs` | `HeartbeatBlock` 只读映射;`seq` 热段段头按 journal-format 已发布布局只读解析 |

</details>

## 延迟与资源契约

| 指标 | 目标 | 测量 |
|---|---|---|
| 心跳停止 → 首个撤单请求发出 | ≤ `hb_stale_factor × hb_stale_ms` + 200 ms(默认 2.2 s) | 演练记录 |
| 心跳停止 → 全部目标挂单核验为 0 | ≤ 5 s(p100,演练) | 演练记录 + 交易所订单历史 |
| 本地触发:请求到达 → `Accepted` | ≤ 100 ms | 单元测试 + 演练 |
| 本地触发:请求到达 → 首个撤单请求发出 | ≤ 500 ms | 演练记录 |
| `stop_core`:请求到达 → `systemctl stop` 发出 | ≤ 首轮撤单核验完成 + 100 ms(含停机标记的两次 `fsync`) | 演练记录 |
| 常驻资源 | < 1% 核、`MemoryMax` 按 [hft-core 内存表](../hft-core/README.md#内存与-journal-段按主机类唯一归属)、`OOMScoreAdjust=-900`、`CPUWeight=1000` | systemd |

## 测试与验收

用例与验收见 [TESTS.md](TESTS.md)。

## 依赖

- 内部:只有 `hft-rt`(仅 `HeartbeatBlock` 的只读映射)。journal 段头、hft-core 配置的相关键与 refdata 账户表按已发布格式只读解析,不依赖 `hft-journal`/`hft-refdata`;任一格式版本不符即拒绝启动(退出 2)。
- 外部:`ureq`(rustls)、`rustls`、`hmac`、`sha2`、`base64`、`serde`、`serde_json`、`toml`、`libc`(mmap、`sd_notify`、`SO_PEERCRED`、`getpwnam`)。
- 禁止:`tokio`、`venue-*`、`hft-rt` 以外的任何 `hft-*` crate、D-Bus 客户端库(停机只经 `systemctl` 子进程)。理由:它必须在"其他代码有缺陷"时仍然工作,依赖越少越能逐行审查([13 O4](../../design/13-设计评审与优化.md));每所撤单与查 UID 逻辑重复写一遍(约 100–170 行)是有意的代价。

## 延伸阅读

- [09 §4.1–4.2](../../design/09-可观测性、风控与控制面.md)、[05](../../design/05-CEX执行层.md)、[12](../../design/12-风险与未决问题.md)(单机 SPOF、爆炸半径)
- [hft-core 优雅退出与 kill 语义](../hft-core/README.md#优雅退出与-kill-语义)、[hft-risk](../../crates/hft-risk/README.md)(kill 级别)、[ops_bot 紧急停止](../../python/ops_bot/README.md)
- [incident-response](../../docs/runbooks/incident-response.md)、[launch](../../docs/runbooks/launch.md)
- [round8-decisions](../../design/_work/round8-decisions.md)(紧急停止兜底、kill-switch 只撤单、控制面失效时的 `stop_core`)

## 已知限制与后续工作

- 只撤单不平仓;hft-core 长时间起不来时的仓位处置是人工决定。
- `cancel` 动作时 hft-core 若仍在运行(`core_alive = true`),策略可能重新挂单;kill-switch 不反复撤。控制面不可用时 ops_bot 的兜底用 `stop_core`,其余情形由收到 P0 的交易员按 incident-response 停 hft-core(Drain)或用 HARD kill。
- `stop_core` 停的是整个 `prod` 实例(全部账户进入 Drain),即使请求只针对一个账户的撤单;这是有意的:控制面不可用时没有按账户禁开的通道。重启后的 Kill 只覆盖停机标记里的范围,其余账户按正常闸门恢复。
- 停机标记写入失败时(`marker = false`),重启后的禁开只靠 ops_bot 的待补 Kill;两者都失效时停止会被下一次干净启动撤销——P0 告警是这一情形的唯一提示。停机失败而标记被作废(`.aborted`)时同样如此,是否另发 Kill 由人决定。
- `UidUnverified` 的目标照常撤单:查询失败与撤单成功同时发生时,一把配错的 key 可能撤掉别的账户的挂单;P0 持续到核验通过,换 key 的流程见 [launch](../../docs/runbooks/launch.md)。
- 与 hft-core 同机:整机故障时 kill-switch 同时失效,只剩交易所侧死手(Binance 现货、Bitget 此时无兜底);整机失联由机外看门狗发现并告警(看门狗所在主机归 [deploy](../../deploy/README.md)),处置见 [incident-response "交易主机失联"](../../docs/runbooks/incident-response.md#s-交易主机失联)。可选扩展:在另一台主机部署第二个 kill-switch(判活改为经网络读心跳广播)——触发条件:没有死手的 venue 上的挂单名义在运营中成为主要风险。
- 大多数所没有"只撤单"权限的 key;kill-switch 的 key 事实上能下单,靠 IP 白名单与只存在于本进程内存来约束。

### 开发备忘

<details><summary>给实现者</summary>

- **待核实**:Bitget UTA 的"查全部挂单 / 按 symbol 撤全部 / 账户信息(UID)"端点、权重与返回格式;OKX `cancel-all-after` 是否覆盖现货;Binance 只开了合约权限的 key 能否调 `GET /api/v3/account` 取 `uid`(不能时改用该所其他带 UID 的只读接口)。方法:官方 API 文档 + 测试网/模拟盘账户实测,写进对应 `venue-*` README 的开发备忘并回填本表。
- **待核实**:kill-switch 单元在 deploy 的加固集合(`NoNewPrivileges`、`ProtectSystem=strict`、`RestrictAddressFamilies`)下能否经 `systemctl` 调用 systemd 的 D-Bus 并由 polkit 判定;方法:在 dev2 的 systemd 容器测试里以 `hft-ks` 用户执行一次 `stop_core` 与一次越权操作(停其他单元,期望被拒)。
- refdata blob 的账户表(含 `venue_uid`)与 `VenueCaps.dms` 只读解析:与 hft-refdata 编码对齐版本字段;不一致即退出 2。
- 触发套接字的路径、属组、ops_bot 运行用户名与 polkit 规则以 deploy 的单元文件为准(`RuntimeDirectory=hft/kill-switch`,套接字 `hft-ks:hft-ks-trigger` 0660);改动时两处同 PR 修改。
- 实现顺序:watch + 目标表 + Binance 现货 → 身份核验 → Bitget → 本地触发 `cancel` → `stop_core` → 演练 → Binance USD-M、OKX。

</details>
