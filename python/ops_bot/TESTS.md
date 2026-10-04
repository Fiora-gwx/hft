---
component: ops_bot
prefix: OPS
spec: README.md
---

# ops_bot 测试规格

格式与规则见 [docs/tests](../../docs/tests/README.md)。

## 覆盖

| README 小节 | 用例 |
|---|---|
| 概要 | OPS-154、OPS-155、OPS-210、OPS-211 |
| 目录 | 不适用:非规范 |
| 使用本组件 | OPS-002、OPS-004 |
| 角色与鉴权 | OPS-010–OPS-018、OPS-085、OPS-087、OPS-223 |
| 账户、标签与视图 | OPS-006、OPS-007、OPS-020–OPS-024、OPS-220 |
| 命令的账户语义 | OPS-030–OPS-033、OPS-062、MD-6 |
| 二次确认 | OPS-040–OPS-055、OPS-085、MD-7、MD-11 |
| 紧急停止(/stop) | OPS-060–OPS-085、OPS-090–OPS-102、OPS-221、OPS-224、MD-11、MD-12 |
| 命令表 | OPS-012、OPS-013、OPS-086、OPS-110–OPS-130、OPS-220、OPS-222 |
| 参数面板与修改 | OPS-140–OPS-149、OPS-154 |
| 策略命令与描述符 | OPS-150–OPS-155 |
| 策略插件(optional) | OPS-160、OPS-161 |
| 推送 | OPS-080、OPS-081、OPS-170–OPS-179 |
| 审计 | OPS-101、OPS-180–OPS-183 |
| 配置文件字段 | OPS-001–OPS-005、OPS-048、OPS-173、OPS-174、OPS-224、OPS-225 |
| 实现说明 | OPS-079、OPS-083、OPS-091、OPS-099、OPS-140、OPS-160、OPS-190–OPS-193、OPS-212 |
| 延迟与资源契约 | OPS-200、OPS-201 |
| 测试与验收 | 不适用:非规范 |
| 依赖 | OPS-114、OPS-210–OPS-212 |
| 延伸阅读 | 不适用:非规范 |
| 已知限制与后续工作 | 不适用:非规范 |
| 开发备忘 | 不适用:非规范 |

## 用例

**夹具**(实现在 `python/ops_bot/tests/conftest.py`;各用例的"前置"按名字引用,参数一律显式赋值,不取包内默认值;用例改动其中某项时在"前置"里写出新值):

- `cfg_t()`:`/etc/hft/ops_bot.toml` 的测试副本,逐键赋值——`schema = 4`;`telegram.token_secret = "ops-bot-token"`;`secrets_dir = <tmp>/secrets`(含文件 `ops-bot-token`,内容 `123:abc`,与 `bark-key`);`users` = `V1 {id: 1001, name: "boss", role: Viewer, push_accounts: ["live-a"]}`、`V2 {id: 1002, name: "viewer2", role: Viewer}`(不写 `push_accounts`)、`T1 {id: 2001, name: "trader1", role: Trader, push_accounts: ["live-a", "live-b"]}`、`T2 {id: 2002, name: "trader2", role: Trader, push_accounts: ["live-a"]}`、`A1 {id: 3001, name: "admin1", role: Admin, push_accounts: ["live-a"]}`;`groups = [{id: -1001, name: "ops"}]`;`hft.instances` = `prod` 与 `sim` 两项(`ctl_socket`、`heartbeat_path`、`journal_cold`、`journal_ram`、`config_path` 都指向 `<tmp>/<实例>/` 下的假对象);`pipeline.socket = <tmp>/pipeline/api.sock`;`treasury.state = <tmp>/treasury`;`watchtower.state = <tmp>/watchtower`;`killswitch.trigger_socket = <tmp>/ks/trigger.sock`;`stop.result_timeout_s = 10`;`stop.health_timeout_ms = 1000`;`stop.ctl_unreachable_s = 10`;`page.groups = [-1001]`;`page.bark_secret = "bark-key"`;`schemas.dir = <tmp>/schemas`;`plugins = []`;`limits.writes_per_min = 12`;`confirm.ttl_s = 60`;`push.summary_every_min = 20`;`push.daily_at_local = "00:00"`;`push.tz = "Asia/Shanghai"`;`push.note_min_level = "P2"`;`paths.state = <tmp>/state`。
- `rd_t()`:两个实例当前生效的 refdata。`prod`:账户 `live-a`(`kind = Live`,`is_prod = true`,venues `VS`(现货,交易所 E1)与 `VP`(USDT 永续,交易所 E2),绑定策略实例 `pc-1`,种类 `probe-target`)、`live-b`(Live,`is_prod = true`,venues `VS`、`VP`,绑定 `pc-2`,种类 `probe-taker`)、`tn-c`(Live,端点指向测试网,绑定 `pc-3`,种类 `probe-target`);`sim`:`sim-a`(`kind = Sim`,绑定 `sim-1`,`probe-target`)、`sim-b`(Sim,绑定 `sim-2`,`probe-taker`)。inst `BTC/VS`(lot 0.00001)、`BTC/VP`(lot 0.001);`live-a` 的 group `g1 = 16777217`(`pc-1` 的第 1 个,腿 0 `BTC/VS`、腿 1 `BTC/VP`)。两个实例的 hft-core 配置 `watchdog.hb_stale_ms = 1000`;每个账户 `ExecConfig.dust_notional = 5 USDT`。
- `pairs_t()`:pipeline 的校准对 `p1 = {live: pc-1, sim: sim-1}`(ops_bot 经 pipeline 本地接口 `status` 读到)。
- `schemas_t()`:`<tmp>/schemas/` = hft-testkit `TestStrategies` 导出的 `65280-probe-taker.schema.json`、`65281-probe-target.schema.json` 与 `index.json`(夹具文件在 `python/ops_bot/tests/fixtures/schemas/`);`probe-target` 的字段至少含 Rate 字段 `open_th`(per_group,`relax = Down`)、DurationMs 字段 `cooldown_ms`、Notional 字段 `cap`、Bool 字段 `force_exit`(per_group,`manual_only`)、Enum 字段 `mode`(`["a", "b"]`)、structural 字段 `pct_bin`;`cmds` 含 `close`(`op = 1`,`reduce_only = true`,字段 `group_frac` Rate ∈ [0, 1])与 `open`(`op = 2`,`reduce_only = false`,字段 `notional` Notional ∈ [0, 10 000]);`notes` 含 `CashShort`(code 1,P1,`v[0]` = 缺口 Notional)与 `Info1`(code 2,Info);`report` 含 `gate_cost_blocked`(Count,Instance)。`probe-taker` 的 `cmds` 为空。
- 假对象:`FakeTg`(注入 update:`update_id`、`from.id`、`chat.id`、`chat.type`;记录发出、编辑的消息与按钮)、`FakeCtl[实例]`(记录每个 `CmdRequest` 的 JSON;按脚本回 `CmdResponse`;可设"不可连"(连接被拒)或"连接耗时 d ms";`CmdResult` 经 `FakeJournal` 投递)、`FakeJournal[实例]`(向 ops_bot 的尾随任务追加事件;可设"停止前进")、`FakeHB[实例]`(`state` 与心跳年龄)、`FakeKS`(在 `killswitch.trigger_socket` 上监听,记录请求,按脚本回 `Accepted`/`Rejected`/`Done`;可设"不监听")、`FakePipeline`(在 `pipeline.socket` 上应答 `status`、`preview_override`、`apply_override`、`run_warmup`)、`FakeBark`(记录 critical 推送)、`clock`(假时钟:单调时刻与 UTC 同步推进,起点 `t0 = 2026-10-05T00:00:00Z`)。
- `boot()`:以 `cfg_t()`、`rd_t()`、`schemas_t()` 启动 ops_bot;两个实例 `FakeHB.state = Running`、心跳年龄 100 ms,`FakeCtl` 可连,`FakeKS` 监听;`<tmp>/state/pending_kill.json` 不存在。
- 记法:"本地拒绝(`<码>`)" = 回复一条含拒绝原因的消息、`audit` 表一行 `result = denied: <码>`、该 update 之后 `FakeCtl` 与 `FakePipeline` 收到 0 个请求;"发出 X" = `FakeCtl` 收到一条 `cmd.type = X` 的请求;`preview` = 请求带 `preview: true`;"立即发出" = 不带 `preview`、不带 `confirmed`;"确认发出" = 新 `cmd_id`、同一命令体、`confirmed: true`、`expect_state_version` = 预览响应的 `ctl_state_version`。

### OPS-001 配置文件各键的默认值

- 规则:[配置文件字段](README.md#配置文件字段)
- 级别:table
- 里程碑:M5
- 前置:配置文件只写必填键(`telegram.token_secret`、`users`、`hft.instances`、`page.groups`、`page.bark_secret`)与 `schema = 4`;本用例是默认值的唯一用例
- 实现:`python/ops_bot/tests/test_config.py::test_ops_001_config_defaults`

| 行 | 键 | 期望 |
|---|---|---|
| OPS-001.1 | `secrets_dir` | `/run/hft/ops_bot/secrets` |
| OPS-001.2 | `groups` | `[]` |
| OPS-001.3 | `pipeline.socket` | `/run/hft/pipeline/api.sock` |
| OPS-001.4 | `treasury.state` | `/var/lib/hft/treasury` |
| OPS-001.5 | `watchtower.state` | `/var/lib/hft/watchtower` |
| OPS-001.6 | `killswitch.trigger_socket` | `/run/hft/kill-switch/trigger.sock` |
| OPS-001.7 | `stop.result_timeout_s` | 10 |
| OPS-001.8 | `stop.health_timeout_ms` | 1 000 |
| OPS-001.9 | `stop.ctl_unreachable_s` | 10 |
| OPS-001.10 | `schemas.dir` | `/opt/hft/current-cold/schemas/strategies` |
| OPS-001.11 | `plugins` | `[]` |
| OPS-001.12 | `limits.writes_per_min` | 12 |
| OPS-001.13 | `confirm.ttl_s` | 60 |
| OPS-001.14 | `push.summary_every_min` / `push.daily_at_local` / `push.tz` | 20 / `"00:00"` / `"Asia/Shanghai"` |
| OPS-001.15 | `push.note_min_level` | `P2` |
| OPS-001.16 | `paths.state` | `/var/lib/hft/ops_bot` |
| OPS-001.18 | `ops.view_stale_ms` | 3 000 |
| OPS-001.17 | 未写 `push_accounts` 的用户的订阅 | `rd_t()` 中全部 `kind = Live` 且非测试网的账户(`live-a`、`live-b`) |

### OPS-002 配置不合法时启动退出 2

- 规则:[配置文件字段](README.md#配置文件字段)
- 级别:table
- 里程碑:M5
- 前置:`cfg_t()`,每行只改一处;`rd_t()`
- 实现:`python/ops_bot/tests/test_config.py::test_ops_002_bad_config_exit2`

| 行 | 改动 | 期望 |
|---|---|---|
| OPS-002.1 | `schema = 3` | 退出码 2;未调用 Telegram `getUpdates` |
| OPS-002.2 | `schema = 5` | 退出码 2 |
| OPS-002.3 | 删去 `telegram.token_secret` | 退出码 2 |
| OPS-002.4 | 删去 `users` | 退出码 2 |
| OPS-002.5 | 删去 `hft.instances` | 退出码 2 |
| OPS-002.6 | 删去 `page.groups` | 退出码 2 |
| OPS-002.7 | 删去 `page.bark_secret` | 退出码 2 |
| OPS-002.8 | `users` 中 `id = 2001` 出现两次(角色 Trader 与 Admin) | 退出码 2 |
| OPS-002.9 | `users[0].role = "Owner"` | 退出码 2 |
| OPS-002.10 | `hft.instances[0].name = "dev"` | 退出码 2 |
| OPS-002.11 | `plugins = ["no-such-plugin"]`(无此 entry point) | 退出码 2 |
| OPS-002.12 | `push.note_min_level = "P3"` | 退出码 2 |
| OPS-002.13 | `push.daily_at_local = "24:00"` | 退出码 2 |
| OPS-002.14 | `push.daily_at_local = "23:59"` | 启动成功(对照行) |
| OPS-002.15 | `push.tz = "Mars/Base"` | 退出码 2 |

### OPS-003 u16 数值键在类型边界上的取舍

- 规则:[配置文件字段](README.md#配置文件字段)
- 级别:table
- 里程碑:M5
- 前置:`cfg_t()`;每行把列出的键设为给定值,其余不变;对 `stop.result_timeout_s`、`stop.health_timeout_ms`、`stop.ctl_unreachable_s`、`limits.writes_per_min`、`confirm.ttl_s`、`push.summary_every_min` 六个键逐一运行
- 实现:`python/ops_bot/tests/test_config.py::test_ops_003_u16_bounds`

| 行 | 取值 | 期望 |
|---|---|---|
| OPS-003.1 | 65 535 | 启动成功,读回 65 535 |
| OPS-003.2 | 65 536 | 退出码 2 |
| OPS-003.3 | −1 | 退出码 2 |
| OPS-003.4 | `"10"`(字符串) | 退出码 2 |

### OPS-004 bot token 读入后立即删除且不进日志

- 规则:[配置文件字段](README.md#配置文件字段)、[实现说明](README.md#实现说明)
- 级别:integ
- 里程碑:M5
- 前置:`cfg_t()`;`<tmp>/secrets/ops-bot-token` 内容 `123:abc`;日志捕获器挂在根 logger 上(DEBUG 级别)
- 步骤:
  1. `boot()`
  2. 注入 V1 的 `/status`,让 `FakeTg` 记录一次 HTTP 请求 URL(含 token)
- 期望:
  - 步骤 1 之后 `<tmp>/secrets/ops-bot-token` 不存在
  - `logging.getLogger("httpx").level` 与 `logging.getLogger("httpcore").level` 均为 `WARNING`
  - 捕获的全部日志行中没有子串 `123:abc`
- 实现:`python/ops_bot/tests/test_config.py::test_ops_004_token_unlinked_not_logged`

### OPS-005 token 文件不存在时启动退出 2

- 规则:[配置文件字段](README.md#配置文件字段)
- 级别:unit
- 里程碑:M5
- 前置:`cfg_t()`;`<tmp>/secrets/` 中没有 `ops-bot-token`
- 步骤:
  1. 启动 ops_bot
- 期望:
  - 退出码 2;没有任何 Telegram 请求
- 实现:`python/ops_bot/tests/test_config.py::test_ops_005_missing_token_exit2`

### OPS-006 两个实例的 refdata 有同名账户时启动退出 2

- 规则:[账户、标签与视图](README.md#账户标签与视图)
- 级别:unit
- 里程碑:M5
- 前置:`cfg_t()`;`rd_t()` 改为 `sim` 实例另有一个名为 `live-a` 的 Sim 账户
- 步骤:
  1. 启动 ops_bot
- 期望:
  - 退出码 2;stderr 含 `live-a`;没有任何 Telegram 请求
- 实现:`python/ops_bot/tests/test_accounts.py::test_ops_006_duplicate_name_at_start_exit2`

### OPS-007 运行中出现同名账户时该名字的命令本地拒绝并 P1

- 规则:[账户、标签与视图](README.md#账户标签与视图)
- 级别:integ
- 里程碑:M5
- 前置:`boot()`
- 步骤:
  1. `FakeJournal[sim]` 追加一条生效的 refdata `ConfigBlob`,其中新增 Sim 账户 `live-b`
  2. T1 私聊发 `/kill account live-b`
  3. T1 私聊发 `/kill account live-a`
  4. T1 私聊发 `/p live-b`
- 期望:
  - 尾随任务处理完步骤 1 的记录后(步骤 2 之前)T1、T2、A1 私聊各收到一条 P1 消息,文本含 `live-b`;V1、V2 没有收到
  - 步骤 2:本地拒绝(`dup_account`)
  - 步骤 3:发出 `Kill`,`account = "live-a"`
  - 步骤 4:本地拒绝(`dup_account`)
- 实现:`python/ops_bot/tests/test_accounts.py::test_ops_007_duplicate_name_at_runtime`

### OPS-010 未知 from.id 的 update 静默丢弃并只记日志

- 规则:[角色与鉴权](README.md#角色与鉴权)
- 级别:unit
- 里程碑:M5
- 前置:`boot()`;日志捕获器
- 步骤:
  1. `from.id = 9999`(不在 `users`)在私聊(`chat.id = 9999`)发 `/status`
  2. 同一人在已登记群 `-1001` 发 `/kill`
- 期望:
  - 两步都没有回复消息;`audit` 表行数不变;`FakeCtl` 收到 0 个请求
  - 日志恰有两行 `unauthorized update dropped: user=9999 chat=9999` 与 `unauthorized update dropped: user=9999 chat=-1001`
- 实现:`python/ops_bot/tests/test_auth.py::test_ops_010_unknown_user_dropped`

### OPS-011 聊天类型决定 update 是否处理

- 规则:[角色与鉴权](README.md#角色与鉴权)
- 级别:table
- 里程碑:M5
- 前置:`boot()`;发送者 T1;命令 `/status`;日志捕获器
- 实现:`python/ops_bot/tests/test_auth.py::test_ops_011_chat_types`

| 行 | `chat.type` / `chat.id` | 期望 |
|---|---|---|
| OPS-011.1 | `private` / 2001 | 回复 `/status` 面板;`audit` 一行 `result = ok`、`chat.id = 2001` |
| OPS-011.2 | `group` / −1001(已登记) | 在群 −1001 回复面板;`audit` 一行 `result = ok` |
| OPS-011.3 | `supergroup` / −1001(已登记) | 同 OPS-011.2 |
| OPS-011.4 | `group` / −1009(未登记) | 无回复;`audit` 行数不变;日志一行含 `chat=-1009` |
| OPS-011.5 | `channel` / −1001 | 无回复;`audit` 行数不变;日志一行 |

### OPS-012 只读面板命令对三种角色都放行且不发控制命令

- 规则:[角色与鉴权](README.md#角色与鉴权)、[命令表](README.md#命令表)
- 级别:table
- 里程碑:M5
- 前置:`boot()`;每行由 V1、T1、A1 各在私聊发一次;`FakeJournal` 已有 `live-a`、`sim-a` 的权益、目标、订单与 Note 记录;`FakePipeline.status` 返回 `pc-1` 最近一轮;`<tmp>/treasury/latest.json` 存在;`<tmp>/watchtower` 有 2 条告警;`schemas_t()` 含 `probe-target`
- 实现:`python/ops_bot/tests/test_auth.py::test_ops_012_readonly_all_roles`
- 备注:每行的期望对三个发送者都成立:各得到一条非空回复;`audit` 三行 `result = ok`;`FakeCtl` 收到的写命令请求数为 0(`/schema` 只在 OPS-120 的情形下发只读的 `GetSchema`)

| 行 | 命令 | 期望(三种角色相同) |
|---|---|---|
| OPS-012.1 | `/status` | 回复含 `prod`、`sim` 两节 |
| OPS-012.2 | `/health` | 与同一时刻 `/status` 的正文逐字相同 |
| OPS-012.3 | `/accounts` | 回复含 `live-a`、`live-b`、`tn-c`、`sim-a`、`sim-b` |
| OPS-012.4 | `/account live-b` | 回复"默认视图:live-b";该用户 `state` 表视图 = `live-b` |
| OPS-012.5 | `/p live-a` | 回复以 `<b>[💰实盘·live-a]</b>` 开头 |
| OPS-012.6 | `/risk live-a` | 回复以 `<b>[💰实盘·live-a]</b>` 开头 |
| OPS-012.7 | `/targets live-a` | 回复含 `16777217` |
| OPS-012.8 | `/orders live-a` | 回复以账户标签开头 |
| OPS-012.9 | `/pnl live-a` | 回复以账户标签开头 |
| OPS-012.10 | `/sim sim-a` | 回复以 `<b>[🧪模拟·sim-a]</b>` 开头 |
| OPS-012.11 | `/sim sim-a vs live-a` | 回复含两个账户标签 |
| OPS-012.12 | `/notes live-a 5` | 回复以账户标签开头 |
| OPS-012.13 | `/params pc-1` | 回复含 `open_th` |
| OPS-012.14 | `/params pc-1 16777217` | 回复含 `16777217` |
| OPS-012.15 | `/params pair p1` | 回复含 `pc-1`、`sim-1` |
| OPS-012.16 | `/schema probe-target` | 回复含字段名 `open_th`、`cooldown_ms` |
| OPS-012.17 | `/treasury live-a` | 回复以账户标签开头 |
| OPS-012.18 | `/pipeline pc-1` | 回复含 `pc-1` |
| OPS-012.19 | `/alerts 2` | 回复含 2 条告警 |
| OPS-012.20 | `/audit 5` | 回复含 ≤ 5 条审计记录 |
| OPS-012.21 | `/menu` | 回复带内联按钮 |
| OPS-012.22 | `/help` | 回复手册正文 |
| OPS-012.23 | `/start` | 回复欢迎语与该发送者的角色名(`Viewer`/`Trader`/`Admin`) |

### OPS-013 私聊中每条写命令对每种角色的处理

- 规则:[角色与鉴权](README.md#角色与鉴权)、[命令表](README.md#命令表)、[二次确认](README.md#二次确认)、[紧急停止(/stop)](README.md#紧急停止stop)
- 级别:table
- 里程碑:M5
- 前置:`boot()`;每行由 V1、T1、A1 各在私聊发一次(互不影响:每次发送前清空 `FakeCtl`、`FakePipeline` 记录与限速窗口);`FakeCtl` 对 `preview` 回 `ctl_state_version = 41`;记号见夹具小节:"拒(c)" = 本地拒绝(`c`);"预览 X" = 发出带 `preview: true` 的 X 并显示带确认按钮的预览,未点确认前没有第二个请求;"发 X" = 立即发出 X;"本地预览" = 不向 `FakeCtl` 发请求,回复带确认按钮的预览
- 实现:`python/ops_bot/tests/test_auth.py::test_ops_013_write_commands_by_role`

| 行 | 命令 | V1(Viewer) | T1(Trader) | A1(Admin) |
|---|---|---|---|---|
| OPS-013.1 | `/stop live-a` | 预览 `Kill{level: Account, account: live-a, hard: false, flatten: false, cancel_resting: true}` | 发同一 `Kill` | 发同一 `Kill` |
| OPS-013.2 | `/stop` | 向 `prod`、`sim` 各预览 `Kill{level: Global, account: "*"}`,一条预览消息、一个确认按钮 | 向 `prod`、`sim` 各发 `Kill{Global}` | 同 T1 |
| OPS-013.3 | `/stop live-a core` | 本地预览(文本含"将经 kill-switch 停止 hft-core@prod") | 本地预览 | 本地预览 |
| OPS-013.4 | `/note hello` | 发 `Annotate{kind: OpsNote, text: "hello"}` | 同 V1 | 同 V1 |
| OPS-013.5 | `/kill` | 拒(viewer) | 向 `prod`、`sim` 各发 `Kill{Global}`(默认参数) | 同 T1 |
| OPS-013.6 | `/kill all` | 拒(viewer) | 同 OPS-013.5 | 同 T1 |
| OPS-013.7 | `/kill account live-a` | 拒(viewer) | 发 `Kill{Account, account: live-a}` 到 `prod` | 同 T1 |
| OPS-013.8 | `/kill venue VP` | 拒(viewer) | 向 `prod`、`sim` 各发 `Kill{level: Venue, venue: VP, account: "*"}` | 同 T1 |
| OPS-013.9 | `/kill account live-a flatten` | 拒(viewer) | 预览 `Kill{Account, live-a, flatten: true}` | 同 T1 |
| OPS-013.10 | `/kill account live-a hard` | 拒(viewer) | 发 `Kill{Account, live-a, hard: true}`(无预览) | 同 T1 |
| OPS-013.11 | `/kill hard` | 拒(viewer) | 向 `prod`、`sim` 各发 `Kill{Global, hard: true}` | 同 T1 |
| OPS-013.12 | `/mode pc-1 live-a closeonly` | 拒(viewer) | 发 `SetMode{strategy: pc-1, group: "*", account: live-a, mode: CloseOnly}` | 同 T1 |
| OPS-013.13 | `/mode pc-1 live-a shadow` | 拒(viewer) | 发 `SetMode{…, mode: Shadow}` | 同 T1 |
| OPS-013.14 | `/mode pc-1 live-a live` | 拒(viewer) | 预览 `SetMode{…, mode: Live}` | 同 T1 |
| OPS-013.15 | `/resume kill live-a` | 拒(viewer),回复"恢复请联系交易员" | 预览 `Resume{source: Kill, account: live-a}` | 同 T1 |
| OPS-013.16 | `/hedgefix live-a 16777217` | 拒(viewer) | 预览 `HedgeFix{account: live-a, group: "16777217"}` | 同 T1 |
| OPS-013.17 | `/adopt live-a VP position 77` | 拒(viewer) | 预览 `AdoptExchange{account: live-a, venue: VP, item: Position, report_id: "77"}` | 同 T1 |
| OPS-013.18 | `/baseline live-a VS` | 拒(viewer) | 预览 `SetBalanceBaseline{account: live-a, venue: VS}` | 同 T1 |
| OPS-013.19 | `/transfer live-a VS VP USDT 100 r1` | 拒(viewer) | 预览 `RecordTransfer{account: live-a, from: VS, to: VP, asset: USDT, amount: "100", ref: "r1"}` | 同 T1 |
| OPS-013.20 | `/notice VP * delist 2026-10-06T00:00:00Z` | 拒(viewer) | 发 `VenueNotice{kind: Delisting, venue: VP, inst: "*", account: "*"}` | 同 T1 |
| OPS-013.21 | `/notice VP BTC/VP funding 14400` | 拒(viewer) | 发 `VenueNotice{kind: FundingInterval, new_s: "14400", account: "*"}` | 同 T1 |
| OPS-013.22 | `/notice VP * maint 2026-10-06T00:00:00Z 2026-10-06T01:00:00Z` | 拒(viewer) | 发 `VenueNotice{kind: Maintenance, account: "*"}` | 同 T1 |
| OPS-013.23 | `/simreset sim-a` | 拒(viewer) | 预览 `SimReset{account: sim-a}` 到 `sim` | 同 T1 |
| OPS-013.24 | `/simtransfer sim-a VS VP USDT 100` | 拒(viewer) | 发 `SimTransfer{account: sim-a, …}` 到 `sim` | 同 T1 |
| OPS-013.25 | `/smoke live-a VP BTC/VP buy` | 拒(viewer) | 拒(role) | 预览 `SmokeOrder{account: live-a, venue: VP, inst: BTC/VP, side: Buy}` |
| OPS-013.26 | `/writeoff live-a 16777217` | 拒(viewer) | 拒(role) | 预览 `WriteOffResidual{account: live-a, group: "16777217"}` |
| OPS-013.27 | `/cmd pc-1 live-a 16777217 close group_frac=0.5` | 拒(viewer) | 发 `StrategyCmd{strategy: pc-1, account: live-a, group: "16777217", reduce_only: true}`(无预览) | 同 T1 |
| OPS-013.28 | `/cmd pc-1 live-a none open notional=100` | 拒(viewer) | 预览 `StrategyCmd{…, group: "none", reduce_only: false}` | 同 T1 |
| OPS-013.29 | `/warmup pc-1` | 拒(viewer) | `FakePipeline` 收到 `run_warmup(instance = "pc-1")`;`FakeCtl` 0 个请求 | 同 T1 |
| OPS-013.30 | `/shutdown` | 拒(viewer) | 向 `prod`、`sim` 各发 `Drain{account: "*"}` | 同 T1 |
| OPS-013.31 | `/params pc-1 set cooldown_ms 30s` | 拒(viewer) | `FakePipeline` 收到 `preview_override(target = "instance:pc-1", field = "cooldown_ms", value = 30000)` | 同 T1 |
| OPS-013.32 | `/params pair p1 set cooldown_ms 30s` | 拒(viewer) | `FakePipeline` 收到 `preview_override(target = "pair:p1", …)` | 同 T1 |
| OPS-013.33 | `/ack w-17` | 拒(viewer) | `<tmp>/watchtower/acks.jsonl` 追加一行 `{"id": "w-17", "by": "tg:2001", "at_utc_ns": <t0 的 ns>}` | 同 T1(`by = "tg:3001"`) |
| OPS-013.34 | `/sub live-b on` | 拒(viewer) | `state` 表该用户订阅含 `live-b` | 同 T1 |
| OPS-013.35 | `/stop live-a flatten` | 拒(bad_args) | 拒(bad_args) | 拒(bad_args) |

### OPS-014 已登记群里每条命令按群聊规则放行或要求私聊

- 规则:[角色与鉴权](README.md#角色与鉴权)
- 级别:table
- 里程碑:M5
- 前置:`boot()`;每行由 V1、T1、A1 各在群 −1001 发一次(`chat.type = group`);"私"= 回复"请私聊执行"并记 `denied: private_only`、没有请求;其余记号同 OPS-013
- 实现:`python/ops_bot/tests/test_auth.py::test_ops_014_group_chat_rules`

| 行 | 命令 | V1 | T1 | A1 |
|---|---|---|---|---|
| OPS-014.1 | `/status` | 在群里回复面板 | 同 V1 | 同 V1 |
| OPS-014.2 | `/account live-b` | 切换 V1 的视图 | 切换 T1 的视图 | 切换 A1 的视图 |
| OPS-014.3 | `/stop live-a` | 预览,预览消息发在群里 | 发 `Kill{Account, live-a}`,请求 `chat_type = "group"` | 同 T1 |
| OPS-014.4 | `/stop live-a core` | 本地预览,发在群里 | 本地预览 | 本地预览 |
| OPS-014.5 | `/kill` | 拒(viewer) | 向两个实例各发 `Kill{Global}` | 同 T1 |
| OPS-014.6 | `/kill venue VP` | 拒(viewer) | 向两个实例各发 `Kill{Venue, VP}` | 同 T1 |
| OPS-014.7 | `/kill account live-a flatten` | 私 | 私 | 私 |
| OPS-014.8 | `/kill account live-a hard` | 私 | 私 | 私 |
| OPS-014.9 | `/mode pc-1 live-a closeonly` | 拒(viewer) | 发 `SetMode{…, CloseOnly}` | 同 T1 |
| OPS-014.10 | `/mode pc-1 live-a shadow` | 私 | 私 | 私 |
| OPS-014.11 | `/mode pc-1 live-a live` | 私 | 私 | 私 |
| OPS-014.12 | `/resume kill live-a` | 私 | 私 | 私 |
| OPS-014.13 | `/hedgefix live-a 16777217` | 私 | 私 | 私 |
| OPS-014.14 | `/adopt live-a VP position 77` | 私 | 私 | 私 |
| OPS-014.15 | `/baseline live-a VS` | 私 | 私 | 私 |
| OPS-014.16 | `/transfer live-a VS VP USDT 100 r1` | 私 | 私 | 私 |
| OPS-014.17 | `/notice VP * delist 2026-10-06T00:00:00Z` | 私 | 私 | 私 |
| OPS-014.18 | `/notice VP BTC/VP funding 14400` | 私 | 私 | 私 |
| OPS-014.19 | `/simreset sim-a` | 私 | 私 | 私 |
| OPS-014.20 | `/simtransfer sim-a VS VP USDT 100` | 私 | 私 | 私 |
| OPS-014.21 | `/smoke live-a VP BTC/VP buy` | 私 | 私 | 私 |
| OPS-014.22 | `/writeoff live-a 16777217` | 私 | 私 | 私 |
| OPS-014.23 | `/cmd pc-1 live-a 16777217 close group_frac=0.5` | 私 | 私 | 私 |
| OPS-014.24 | `/warmup pc-1` | 私 | 私 | 私 |
| OPS-014.25 | `/shutdown` | 私 | 私 | 私 |
| OPS-014.26 | `/params pc-1 set cooldown_ms 30s` | 私 | 私 | 私 |
| OPS-014.27 | `/note hello` | 私 | 私 | 私 |
| OPS-014.28 | `/sub live-b on` | 私 | 私 | 私 |
| OPS-014.29 | `/ack w-17` | 拒(viewer) | 追加 `acks.jsonl` 一行 | 同 T1 |

### OPS-015 发往 gateway 的请求带 issuer、聊天类型与视图账户,且从不补全账户

- 规则:[角色与鉴权](README.md#角色与鉴权)、[账户、标签与视图](README.md#账户标签与视图)
- 级别:table
- 里程碑:M5
- 前置:`boot()`;T1 已 `/account live-b`
- 实现:`python/ops_bot/tests/test_auth.py::test_ops_015_request_envelope`

| 行 | 输入 | 期望(`FakeCtl` 收到的请求) |
|---|---|---|
| OPS-015.1 | 私聊 `/kill account live-a` | `issuer = "tg:2001"`、`chat_type = "private"`、`view_account = "live-b"`、`account = "live-a"`、`v = 2` |
| OPS-015.2 | 群 −1001(`group`)`/kill account live-a` | `chat_type = "group"` |
| OPS-015.3 | 群 −1001(`supergroup`)`/kill account live-a` | `chat_type = "group"` |
| OPS-015.4 | 私聊 `/mode pc-1 closeonly`(不写账户) | `account = "live-a"`(`pc-1` 绑定的账户),不是视图账户 `live-b` |

### OPS-016 本地预判通过而 gateway 拒绝时以 gateway 结果为准

- 规则:[角色与鉴权](README.md#角色与鉴权)
- 级别:unit
- 前置:`boot()`;`FakeCtl[prod]` 对 `issuer = tg:2001` 的一切写命令回 `{status: ok, code: Forbidden}`(模拟 gateway 的 issuer 表把 T1 配成 Viewer)
- 里程碑:M5
- 步骤:
  1. T1 私聊发 `/kill account live-a`
- 期望:
  - `FakeCtl[prod]` 收到 1 个 `Kill` 请求;回复含 `Forbidden`;`audit` 该行 `result = Forbidden`
- 实现:`python/ops_bot/tests/test_auth.py::test_ops_016_gateway_has_last_word`

### OPS-017 写命令按每用户 60 s 滑动窗口限速

- 规则:[角色与鉴权](README.md#角色与鉴权)
- 级别:table
- 里程碑:M5
- 前置:`boot()`;`limits.writes_per_min = 12`;发送者 T1(私聊);写命令用 `/kill account live-a`(立即发出,计入);`t` 为相对第 1 条的时刻
- 实现:`python/ops_bot/tests/test_auth.py::test_ops_017_rate_limit`

| 行 | 输入 | 期望 |
|---|---|---|
| OPS-017.1 | t = 0 … 11 s 每秒一条,共 12 条 | 12 条都发出 |
| OPS-017.2 | 接 .1,t = 59 s 第 13 条 | 本地拒绝(`rate_limited`) |
| OPS-017.3 | 接 .2,t = 60 s 第 14 条(t = 0 那条已滑出窗口) | 发出 |
| OPS-017.4 | 接 .1,t = 30 s 发 `/stop live-a` 5 次 | 5 条都发出;之后 t = 31 s 的 `/kill account live-a` 仍被拒(`rate_limited`) |
| OPS-017.5 | 接 .1,t = 30 s 点击之前一条 `/mode … live` 预览的确认按钮 | 确认请求发出,不被限速 |
| OPS-017.6 | 接 .1,t = 30 s 发 `/status` | 回复面板(只读不计入) |
| OPS-017.7 | 新的窗口:先发 12 条被拒(viewer) 的命令(V1 发),再由 V1 发 `/note x` | `/note` 发出(被拒的不计入) |
| OPS-017.8 | 新的窗口:12 条 `/mode pc-1 live-a live`(只生成预览、不确认),第 13 条 `/kill account live-a` | 第 13 条被拒(`rate_limited`):生成预览计入 |
| OPS-017.9 | 接 .2 被拒之后,t = 60 s 的条数计算 | 被拒的第 13 条不计入:t = 60 s 起窗口内 11 条,第 14 条发出 |
| OPS-017.10 | T1 用满 12 条后,T2 在 t = 30 s 发 `/kill account live-a` | 发出(按用户计) |

### OPS-018 BotFather 命令菜单按启用的插件与角色生成

- 规则:[角色与鉴权](README.md#角色与鉴权)
- 级别:table
- 里程碑:M5
- 前置:`cfg_t()`;`plugins = ["t-plugin"]`(测试插件,注册命令 `/tplug`,写命令);`FakeTg` 记录 `setMyCommands` 调用(每个用户 scope 一次)
- 实现:`python/ops_bot/tests/test_auth.py::test_ops_018_command_menus`

| 行 | 用户 | 期望菜单 |
|---|---|---|
| OPS-018.1 | V1 | 只读面板命令全集 + `/note` + `/stop`;不含 `/kill`、`/resume`、`/ack`、`/sub`、`/tplug` |
| OPS-018.2 | T1 | V1 的菜单 + 全部 Trader 写命令 + `/tplug`;不含 `/smoke`、`/writeoff` |
| OPS-018.3 | A1 | T1 的菜单 + `/smoke`、`/writeoff` |
| OPS-018.4 | 插件改为 `plugins = []` 重启后的 T1 | 不含 `/tplug` |

### OPS-020 账户表从两个实例的 refdata 只读加载,命令按账户所在实例路由

- 规则:[账户、标签与视图](README.md#账户标签与视图)
- 级别:table
- 里程碑:M5
- 前置:`boot()`;发送者 T1(私聊)
- 实现:`python/ops_bot/tests/test_accounts.py::test_ops_020_routing_by_instance`

| 行 | 命令 | 期望 |
|---|---|---|
| OPS-020.1 | `/kill account live-a` | 只有 `FakeCtl[prod]` 收到请求 |
| OPS-020.2 | `/kill account sim-a` | 只有 `FakeCtl[sim]` 收到请求 |
| OPS-020.3 | `/simtransfer sim-b VS VP USDT 5` | 只有 `FakeCtl[sim]` 收到请求 |
| OPS-020.4 | `/kill account nosuch` | 本地拒绝(`bad_args`),回复"未知账户 nosuch" |
| OPS-020.5 | `FakeJournal[prod]` 追加新 refdata(新增 Live 账户 `live-d`,绑定 `pc-4`)后 `/kill account live-d` | 只有 `FakeCtl[prod]` 收到请求;`/accounts` 列出 `live-d` |

### OPS-021 消息按账户类型带标签,系统消息只带实例标签

- 规则:[账户、标签与视图](README.md#账户标签与视图)
- 级别:table
- 里程碑:M5
- 前置:`boot()`;主机名 `h1`;渲染函数 `render.label(account, text)` 与 `render.system(instance, text)`
- 实现:`python/ops_bot/tests/test_render.py::test_ops_021_account_labels`

| 行 | 输入 | 期望输出前缀 |
|---|---|---|
| OPS-021.1 | `live-a`,`"x"` | `<b>[💰实盘·live-a]</b>` |
| OPS-021.2 | `sim-a`,`"x"` | `<b>[🧪模拟·sim-a]</b>` |
| OPS-021.3 | `tn-c`(测试网端点的 Live 账户),`"x"` | `<b>[🧪测试网·tn-c]</b>` |
| OPS-021.4 | `live-a`,`"<b>[💰实盘·live-a]</b> x"`(已带标签) | 输出中标签恰好出现 1 次 |
| OPS-021.5 | 系统消息,实例 `prod` | `[h1/prod]`,不含任何账户标签 |

### OPS-022 汇总行把实盘与模拟、测试网分开合计

- 规则:[账户、标签与视图](README.md#账户标签与视图)
- 级别:unit
- 里程碑:M5
- 前置:`boot()`;视图中权益:`live-a` 10 000、`live-b` 20 000、`tn-c` 1 000、`sim-a` 5 000、`sim-b` 7 000 USDT
- 步骤:
  1. T1 发 `/accounts`
- 期望:
  - 回复中"实盘合计"行 = 30 000;"模拟合计"行 = 13 000(含 `tn-c` 的 1 000);没有把 5 个账户相加为 43 000 的行
  - 实盘组列出 `live-a`、`live-b`;模拟组列出 `tn-c`、`sim-a`、`sim-b`
- 实现:`python/ops_bot/tests/test_accounts.py::test_ops_022_totals_split`

### OPS-023 /accounts 每个账户一行摘要

- 规则:[账户、标签与视图](README.md#账户标签与视图)
- 级别:unit
- 里程碑:M5
- 前置:`boot()`;视图中 `live-a`:权益 10 000;`VP` 的 `liq_dist = 0.35`、MarginStress L0;裸敞口 0;期望模式 Live、有效模式 CloseOnly;生效 cap `Kill{Account}`
- 步骤:
  1. V1 发 `/accounts`
- 期望:
  - `live-a` 行含 `10000`、`VP liq_dist 35.0%`、`L0`、裸敞口 `0`、`期望 Live`、`有效 CloseOnly`、`Kill`
- 实现:`python/ops_bot/tests/test_accounts.py::test_ops_023_accounts_summary`

### OPS-024 /account 只切换只读默认视图

- 规则:[账户、标签与视图](README.md#账户标签与视图)
- 级别:table
- 里程碑:M5
- 前置:`boot()`;发送者 T1;T1 的视图初值为空
- 实现:`python/ops_bot/tests/test_accounts.py::test_ops_024_view_only_for_reads`

| 行 | 输入序列 | 期望 |
|---|---|---|
| OPS-024.1 | `/account live-b`,再 `/p` | 第二条回复以 `<b>[💰实盘·live-b]</b>` 开头 |
| OPS-024.2 | `/account live-b`,再 `/p live-a` | 回复是 `live-a`;之后 `/p` 仍是 `live-b`(带名字的只读命令不切换视图) |
| OPS-024.3 | `/account live-b`,再 `/mode pc-2 live` | 本地拒绝(`account_required`),回复"请写明账户";`FakeCtl` 0 个请求 |
| OPS-024.4 | `/account nosuch` | 回复"未知账户";视图不变 |
| OPS-024.5 | 视图为空时 `/p` | 回复"请写明账户或先 /account";没有面板 |
| OPS-024.6 | T1 `/account live-b` 后 T2 发 `/p` | T2 的视图不受影响(按用户存于 `state` 表) |

### OPS-030 降级命令不带账户时作用于全部账户,向每个运行中的实例各发一条

- 规则:[命令的账户语义](README.md#命令的账户语义)
- 级别:table
- 里程碑:M5
- 前置:`boot()`;发送者 T1(私聊)
- 实现:`python/ops_bot/tests/test_account_semantics.py::test_ops_030_downgrade_all_accounts`

| 行 | 输入 | 期望 |
|---|---|---|
| OPS-030.1 | `/kill` | `FakeCtl[prod]`、`FakeCtl[sim]` 各收到 1 条 `Kill{level: Global, account: "*"}`;回复逐实例列出两条结果 |
| OPS-030.2 | `/stop` | 同 OPS-030.1 |
| OPS-030.3 | `/kill venue VP` | 两个实例各收到 1 条 `Kill{level: Venue, venue: VP, account: "*"}` |
| OPS-030.4 | `/shutdown` | 两个实例各收到 1 条 `Drain{account: "*"}`;回复逐实例列出挂单数、Live cell 数与 Drain 进度 |
| OPS-030.5 | `FakeHB[prod].state = Stopped` 时 `/kill` | 只有 `FakeCtl[sim]` 收到请求;回复中 `prod` 一行为"未运行" |
| OPS-030.6 | `FakeHB[sim]` 心跳年龄 1 001 ms 时 `/kill` | 只有 `FakeCtl[prod]` 收到请求;`sim` 为"未运行" |
| OPS-030.7 | `FakeHB[sim]` 心跳年龄 1 000 ms 时 `/kill` | 两个实例都收到请求 |
| OPS-030.8 | `/mode pc-1 closeonly`(不写账户) | `FakeCtl[prod]` 收到 `SetMode{strategy: pc-1, account: live-a, mode: CloseOnly}` |

### OPS-031 升级与中性命令缺账户时本地拒绝,不回退到视图

- 规则:[命令的账户语义](README.md#命令的账户语义)
- 级别:table
- 里程碑:M5
- 前置:`boot()`;发送者 T1,视图 = `live-a`
- 实现:`python/ops_bot/tests/test_account_semantics.py::test_ops_031_account_required`

| 行 | 输入 | 期望 |
|---|---|---|
| OPS-031.1 | `/mode pc-1 live` | 本地拒绝(`account_required`),回复"请写明账户" |
| OPS-031.2 | `/mode pc-1 16777217 live` | 本地拒绝(`account_required`) |
| OPS-031.3 | `/resume kill` | 本地拒绝(`account_required`) |
| OPS-031.4 | `/cmd pc-1 none open notional=100` | 本地拒绝(`account_required`) |
| OPS-031.5 | `/kill account live-a flatten`(对照) | 预览发出,`account = "live-a"` |
| OPS-031.6 | `/notice VP BTC/VP funding 14400` | 发出时 `account` 显式为 `"*"`(不是缺省) |

### OPS-032 ops_bot 发出的命令账户与作用域账户一致,/warmup 不带账户

- 规则:[命令的账户语义](README.md#命令的账户语义)
- 级别:prop
- 里程碑:M5
- 性质:对 ops_bot 能发出的每条 `ControlCmd`,请求 JSON 中只有顶层 `account` 一处账户字段,其值等于命令解析出的目标账户(`"*"` 或账户名);`/warmup` 不产生 `ControlCmd`,传给 pipeline 的只有实例名
- 生成:从 OPS-013 的写命令模板中随机取命令,账户名从 `rd_t()` 的 5 个账户与 `"*"` 中取,group 从 `{16777217, "*", "none"}` 取,发送者从 T1、A1 取
- 参考:命令解析器的预期目标账户表(测试内手写的 `expected_account(cmd)`)
- 例数:PR 500
- 实现:`python/ops_bot/tests/test_account_semantics.py::test_ops_032_account_fields_consistent`

### OPS-033 只对模拟账户有效的命令对实盘账户本地拒绝

- 规则:[命令的账户语义](README.md#命令的账户语义)
- 级别:table
- 里程碑:M5
- 前置:`boot()`;发送者 T1
- 实现:`python/ops_bot/tests/test_account_semantics.py::test_ops_033_sim_only_commands`

| 行 | 输入 | 期望 |
|---|---|---|
| OPS-033.1 | `/simreset live-a` | 本地拒绝(`not_sim`) |
| OPS-033.2 | `/simtransfer live-a VS VP USDT 100` | 本地拒绝(`not_sim`) |
| OPS-033.3 | `/simreset tn-c` | 本地拒绝(`not_sim`)(测试网账户 `kind = Live`) |
| OPS-033.4 | `/simreset sim-a` | 预览 `SimReset{account: sim-a}` 发往 `sim`;回执带 `<b>[🧪模拟·sim-a]</b>` |
| OPS-033.5 | `/simtransfer sim-a VS VP USDT 100` | 立即发 `SimTransfer`;回执带"模拟"标签 |

### OPS-040 需要确认的命令先预览,预览首行是目标账户标签与模式

- 规则:[二次确认](README.md#二次确认)
- 级别:unit
- 里程碑:M5
- 前置:`boot()`;`confirm.ttl_s = 60`;视图中 `live-a` 期望模式 CloseOnly、有效模式 CloseOnly、生效 cap `Recovery`;同一校准对的 `sim-a` 有效模式 Live;`FakeCtl[prod]` 对 preview 回 `{code: Applied, preview: {needs_confirm: true}, ctl_state_version: 41, cmd_hash: "9f3c1a7b" + 56 个十六进制字符}`
- 步骤:
  1. T1 私聊发 `/mode pc-1 live-a live`
- 期望:
  - `FakeCtl[prod]` 收到 1 个请求:`preview: true`,无 `confirmed`、无 `expect_state_version`
  - 预览消息第 1 行 = `<b>[💰实盘·live-a]</b> 期望 CloseOnly · 有效 CloseOnly · cap Recovery`;第 2 行含 `sim-a` 与 `有效 Live`;正文含"将从 CloseOnly 变为 Live"、受影响的 group `16777217`;含 `9f3c1a7b`(`cmd_hash` 前 8 位)
  - 消息带两个按钮,回调数据为 `confirm:<id>`、`cancel:<id>`;`pending_confirm` 表一行:`id`、`from.id = 2001`、`chat.id = 2001`、规范形、期望版本 41、预览时刻 `t0`
- 实现:`python/ops_bot/tests/test_confirm.py::test_ops_040_preview_message`

### OPS-041 同一发起人在有效期内确认后以新 cmd_id 带期望版本重发同一命令

- 规则:[二次确认](README.md#二次确认)
- 级别:unit
- 里程碑:M5
- 前置:OPS-040 之后;`clock` 前进 30 s
- 步骤:
  1. T1 点击 `confirm:<id>`
- 期望:
  - `FakeCtl[prod]` 收到第 2 个请求:`cmd_id` ≠ 第 1 个;`cmd` 与第 1 个逐字段相同;`confirmed: true`;`expect_state_version: 41`;`preview` 不为 true;请求 JSON 中没有键 `cmd_hash`、`token`
  - `pending_confirm` 中该 id 被删除
- 实现:`python/ops_bot/tests/test_confirm.py::test_ops_041_confirm_resend`

### OPS-042 确认有效期在 confirm.ttl_s 处分界

- 规则:[二次确认](README.md#二次确认)
- 级别:table
- 里程碑:M5
- 前置:同 OPS-040;`confirm.ttl_s = 60`;每行从新的预览开始,在预览后给定时刻由 T1 点确认
- 实现:`python/ops_bot/tests/test_confirm.py::test_ops_042_ttl_boundary`

| 行 | 点击时刻 | 期望 |
|---|---|---|
| OPS-042.1 | 59 s | 发出确认请求 |
| OPS-042.2 | 60 s | 发出确认请求 |
| OPS-042.3 | 61 s | 不发请求;回复"预览已过期,请重新发起";`audit` 一行 `denied: expired` |
| OPS-042.4 | `confirm.ttl_s = 30`,点击 31 s | 不发请求;`denied: expired` |

### OPS-043 他人点击、重复点击与取消都不执行

- 规则:[二次确认](README.md#二次确认)
- 级别:table
- 里程碑:M5
- 前置:`boot()`;`confirm.ttl_s = 60`;每行从新的预览开始:T1 在群 −1001 发 `/stop live-a core`(本地预览,所有角色都要确认,群聊放行);`FakeCtl[prod]` 可连
- 实现:`python/ops_bot/tests/test_confirm.py::test_ops_043_invalid_clicks`

| 行 | 动作 | 期望 |
|---|---|---|
| OPS-043.1 | T2 在群里点 `confirm:<id>` | 不执行;`audit` 一行 `denied: not_initiator`;之后 T1 点击仍执行 |
| OPS-043.2 | A1 在群里点 `confirm:<id>` | 同 OPS-043.1 |
| OPS-043.3 | T1 点两次 `confirm:<id>`(间隔 1 ms) | 只执行一次(`FakeCtl[prod]` 收到 1 个 `Kill`);第二次 `denied: used` |
| OPS-043.4 | T1 点 `cancel:<id>` 后再点 `confirm:<id>` | 取消后回复"已取消";确认 `denied: used`;`FakeCtl` 0 个请求 |
| OPS-043.5 | 同一 `<id>` 的回调来自另一个 `chat.id`(T1 私聊) | 不执行;`denied: not_initiator` |
| OPS-043.6 | T1 与 T2 同时(同一事件循环迭代)点击 | 只有 T1 的执行;T2 `denied: not_initiator` |

### OPS-044 版本已变时重新生成预览

- 规则:[二次确认](README.md#二次确认)
- 级别:unit
- 里程碑:M5
- 前置:OPS-040 之后;`FakeCtl[prod]` 对确认回 `{code: ConfirmInvalid, detail: {name: "StateChanged"}}`,对随后的 preview 回 `ctl_state_version = 43`
- 步骤:
  1. T1 点击 `confirm:<id>`
- 期望:
  - 回复"状态已变化";`FakeCtl[prod]` 随即收到第 3 个请求:同一命令体、`preview: true`;新预览消息带新的 `<id2>`,`pending_confirm` 中 `<id2>` 的期望版本 = 43
  - 在 T1 点击 `<id2>` 之前,`FakeCtl[prod]` 没有第 4 个请求
- 实现:`python/ops_bot/tests/test_confirm.py::test_ops_044_state_changed_repreview`

### OPS-045 确认响应的 cmd_hash 与预览不同时提示并重新预览

- 规则:[二次确认](README.md#二次确认)
- 级别:unit
- 里程碑:M5
- 前置:OPS-040 之后;`FakeCtl[prod]` 对确认回 `{code: Applied, cmd_hash: "0000aaaa…"}`(前 8 位不同于预览的 `9f3c1a7b`)
- 步骤:
  1. T1 点击确认
- 期望:
  - 原消息被编辑为该 `CmdResult`;另回复一条含"cmd_hash 不一致"的提示;`FakeCtl[prod]` 收到一个新的 `preview` 请求
- 实现:`python/ops_bot/tests/test_confirm.py::test_ops_045_hash_mismatch`

### OPS-046 执行后编辑原消息,10 s 未见结果先显示结果待定

- 规则:[二次确认](README.md#二次确认)
- 级别:table
- 里程碑:M5
- 前置:OPS-041 的确认在 t = 0 发出;`FakeCtl` 回 `accepted`(`code = None`);`CmdResult` 经 `FakeJournal[prod]` 在给定时刻到达
- 实现:`python/ops_bot/tests/test_confirm.py::test_ops_046_result_or_pending`

| 行 | `CmdResult` 到达时刻 | 期望 |
|---|---|---|
| OPS-046.1 | 9 999 ms,`Applied` | 原消息编辑为 `Applied`;从未出现"已发出,结果待定" |
| OPS-046.2 | 10 001 ms,`Applied` | 10 000 ms 后原消息显示"已发出,结果待定";10 001 ms 后改为 `Applied` |
| OPS-046.3 | 3 s,`Applied`,`detail = ShadowDeferred`(`/mode … shadow`) | 原消息含 `ShadowDeferred` |
| OPS-046.4 | `CmdResult` 先于 gateway 的 `accepted` 响应到达(乱序) | 原消息编辑为 `Applied` 一次;之后到达的 `accepted` 不把它改回"结果待定" |

### OPS-047 Viewer 的 /stop 确认不带期望版本、不比对状态版本

- 规则:[二次确认](README.md#二次确认)、[紧急停止(/stop)](README.md#紧急停止stop)
- 级别:unit
- 里程碑:M5
- 前置:`boot()`;V1 私聊 `/stop live-a`,预览响应 `ctl_state_version = 41`、`cmd_hash = "77aa…"`;之后 T1 发 `/mode pc-1 live-a closeonly`,`ctl_state_version` 变为 42
- 步骤:
  1. 预览后 30 s,V1 点确认
- 期望:
  - `FakeCtl[prod]` 收到确认请求:`cmd` 与预览请求逐字段相同、`confirmed: true`、无 `expect_state_version`(或为 0)、无 `cmd_hash` 键;即 `hftctl.confirm(…, state_version=None)`
  - `FakeCtl` 回 `Applied` → 回复"紧急停止已生效"
- 实现:`python/ops_bot/tests/test_confirm.py::test_ops_047_viewer_stop_no_version`

### OPS-048 Viewer 的 /stop 有效期固定 60 s,不随 confirm.ttl_s

- 规则:[二次确认](README.md#二次确认)、[配置文件字段](README.md#配置文件字段)
- 级别:table
- 里程碑:M5
- 前置:`boot()`;V1 私聊 `/stop live-a`;在给定时刻 V1 点确认
- 实现:`python/ops_bot/tests/test_confirm.py::test_ops_048_viewer_ttl_fixed`

| 行 | `confirm.ttl_s` | 点击时刻 | 期望 |
|---|---|---|---|
| OPS-048.1 | 60 | 59 s | 发出确认 |
| OPS-048.2 | 60 | 60 s | 发出确认 |
| OPS-048.3 | 60 | 61 s | 不发;回复"预览已过期,请重新 /stop";`denied: expired` |
| OPS-048.4 | 120 | 61 s | 不发;`denied: expired` |
| OPS-048.5 | 30 | 45 s | 发出确认 |

### OPS-049 Viewer 确认被 gateway 判为无有效预览时按紧急停止失败处理

- 规则:[二次确认](README.md#二次确认)、[紧急停止(/stop)](README.md#紧急停止stop)
- 级别:table
- 里程碑:M5
- 前置:`boot()`;V1 私聊 `/stop live-a` 并在 20 s 时确认;`FakeCtl[prod]` 对确认回给定结果
- 实现:`python/ops_bot/tests/test_confirm.py::test_ops_049_viewer_confirm_rejected`

| 行 | 确认的结果 | 期望 |
|---|---|---|
| OPS-049.1 | `ConfirmInvalid{NoValidPreview}`(hft-core 重启清空了预览表) | P0"紧急停止失败",含 `NoValidPreview`,投递到 T1、T2、A1 私聊、群 −1001 与 Bark;不向 `FakeKS` 发请求 |
| OPS-049.2 | `NeedsConfirm` | 同 OPS-049.1,含 `NeedsConfirm` |
| OPS-049.3 | `Applied`(对照) | P1"紧急停止已生效";没有 P0 |

### OPS-050 换了账户的确认与预览不是同一命令

- 规则:[二次确认](README.md#二次确认)
- 级别:unit
- 里程碑:M5
- 前置:`boot()`;V1 私聊 `/stop live-a`,得到预览按钮 `<id>`;测试直接调用 `confirm.py` 的确认处理,把 `<id>` 绑定的规范形改成 `Kill{Account(live-b)}`(模拟被篡改的本地记录)
- 步骤:
  1. V1 点 `confirm:<id>`
- 期望:
  - 发出的确认请求 `account = "live-b"`;`FakeCtl`(按 gateway 规则)回 `ConfirmInvalid{NoValidPreview}`;ops_bot 按 OPS-049.1 发 P0
- 实现:`python/ops_bot/tests/test_confirm.py::test_ops_050_changed_account_no_valid_preview`
- 备注:sequencer 侧的判定由 MD-11 覆盖;本用例只验证 ops_bot 发出的是同一形状的确认且不携带令牌

### OPS-051 中性命令中写明"预览"的走同一确认流程,其余不需确认的立即发出

- 规则:[二次确认](README.md#二次确认)、[命令表](README.md#命令表)
- 级别:table
- 里程碑:M5
- 前置:`boot()`;发送者 T1(私聊)
- 实现:`python/ops_bot/tests/test_confirm.py::test_ops_051_preview_or_direct`

| 行 | 输入 | 期望 |
|---|---|---|
| OPS-051.1 | `/hedgefix live-a 16777217` | 先 `preview`;确认后带 `confirmed: true` 与期望版本 |
| OPS-051.2 | `/transfer live-a VS VP USDT 100 r1` | 同 OPS-051.1 |
| OPS-051.3 | `/kill account live-a` | 立即发出,不带 `preview`/`confirmed` |
| OPS-051.4 | `/mode pc-1 live-a closeonly` | 立即发出 |
| OPS-051.5 | `/notice VP * delist 2026-10-06T00:00:00Z` | 立即发出 |
| OPS-051.6 | `/simtransfer sim-a VS VP USDT 100` | 立即发出 |
| OPS-051.7 | `/note x` | 立即发出 |
| OPS-051.8 | `/cmd pc-1 live-a 16777217 close group_frac=0.5` | 立即发出,`reduce_only: true` |

### OPS-052 本地写用同一形状的预览与期望版本

- 规则:[二次确认](README.md#二次确认)、[参数面板与修改](README.md#参数面板与修改)
- 级别:unit
- 里程碑:M5
- 前置:`boot()`;`FakePipeline.preview_override` 回 `{diff: [{field: "cooldown_ms", old: 20000, new: 30000}], override_version: 7, relax: false, structural: false, instances: ["pc-1"], live: true}`
- 步骤:
  1. T1 私聊发 `/params pc-1 set cooldown_ms 30s`
  2. T1 点确认
- 期望:
  - 步骤 1:预览消息含 `cooldown_ms`、`20000 → 30000`(按 DurationMs 显示为 `20s → 30s`)、`override_version 7`
  - 步骤 2:`FakePipeline` 收到 `apply_override(target = "instance:pc-1", field = "cooldown_ms", group = None, value = 30000, expect_override_version = 7)`;之后 `FakeCtl[prod]` 收到 `Annotate{kind: LocalWrite}`,其 `digest` 非空
- 实现:`python/ops_bot/tests/test_confirm.py::test_ops_052_local_write_same_shape`

### OPS-053 本地写的期望版本已变时失效并重新预览

- 规则:[二次确认](README.md#二次确认)
- 级别:unit
- 里程碑:M5
- 前置:OPS-052 步骤 1 之后;`FakePipeline.apply_override` 回 `{error: "VersionChanged", override_version: 8}`
- 步骤:
  1. T1 点确认
- 期望:
  - 回复"版本已变化";`FakePipeline` 随即收到新的 `preview_override`;没有 `Annotate{LocalWrite}` 请求
- 实现:`python/ops_bot/tests/test_confirm.py::test_ops_053_local_write_version_changed`

### OPS-054 同一 update 重投时 cmd_id 相同,不重复执行

- 规则:[二次确认](README.md#二次确认)
- 级别:table
- 里程碑:M5
- 前置:`boot()`;发送者 T1;`FakeCtl[prod]` 按 gateway 规则对重复 `cmd_id` 回 `Duplicate` 与缓存的结果
- 实现:`python/ops_bot/tests/test_confirm.py::test_ops_054_idempotent_cmd_id`

| 行 | 输入 | 期望 |
|---|---|---|
| OPS-054.1 | `update_id = 500`,`/kill account live-a`,投递两次 | 两次处理得到的 `cmd_id` 相同 = `uuid5("tg:500:<规范形>")`;sequencer 侧只执行一次(`FakeCtl` 记录的执行次数为 1);`audit` 中该 update 只有一行执行结果 |
| OPS-054.2 | `update_id = 500` 与 `501`,文本相同 | 两个 `cmd_id` 不同 |
| OPS-054.3 | `update_id = 500`,两次投递的规范形相同而间隔 10 min | `cmd_id` 仍相同 |

### OPS-055 降级命令的过期时间为签发后 300 s,其他为 30 s

- 规则:[二次确认](README.md#二次确认)
- 级别:table
- 里程碑:M5
- 前置:`boot()`;发送者 T1;请求在 `t0` 签发
- 实现:`python/ops_bot/tests/test_confirm.py::test_ops_055_expiry`

| 行 | 命令 | 期望 `expires_at_ns − issued_at_ns` |
|---|---|---|
| OPS-055.1 | `/kill account live-a` | 300 000 000 000 |
| OPS-055.2 | `/stop live-a` | 300 000 000 000 |
| OPS-055.3 | `/mode pc-1 live-a closeonly` | 300 000 000 000 |
| OPS-055.4 | `/notice VP * delist 2026-10-06T00:00:00Z` | 300 000 000 000 |
| OPS-055.5 | `/shutdown` | 300 000 000 000 |
| OPS-055.6 | `/mode pc-1 live-a live`(预览与确认两个请求) | 各自 30 000 000 000 |
| OPS-055.7 | `/resume kill live-a` | 30 000 000 000 |
| OPS-055.8 | `/note x` | 30 000 000 000 |
| OPS-055.9 | `/cmd pc-1 live-a 16777217 close group_frac=0.5` | 30 000 000 000 |

### OPS-060 Viewer 的 /stop 预览列出范围、受影响账户与固定提示

- 规则:[紧急停止(/stop)](README.md#紧急停止stop)
- 级别:unit
- 里程碑:M5
- 前置:`boot()`;视图中 `live-a` 有效模式 Live、`BTC/VP` 永续空 0.010、`BTC/VS` 现货 0.010、挂单 2 张
- 步骤:
  1. V1 私聊发 `/stop live-a`
- 期望:
  - `FakeCtl[prod]` 收到 1 个 `preview` 请求:`Kill{level: Account, account: live-a, hard: false, flatten: false, cancel_resting: true}`
  - 预览标题为"⚠️ 紧急停止";正文含范围 `live-a`、`<b>[💰实盘·live-a]</b>`、`有效 Live`、持仓摘要(`BTC/VP −0.010`、`BTC/VS 0.010`)、`挂单 2`,以及原文"禁止新开仓并撤销挂单;**不会平仓**,持仓保留,已成交第一腿的对冲与平仓照常;恢复需交易员"
  - 带确认与取消按钮
- 实现:`python/ops_bot/tests/test_stop.py::test_ops_060_viewer_preview`

### OPS-061 Viewer 的 /stop 经控制面生效:确认后 Kill 生效并推送

- 规则:[紧急停止(/stop)](README.md#紧急停止stop)、[推送](README.md#推送)
- 级别:integ
- 里程碑:M5
- 前置:OPS-060 之后;`stop.result_timeout_s = 10`;`FakeCtl[prod]` 对确认回 `accepted`,`CmdResult{Applied}` 在确认后 2 s 到达
- 步骤:
  1. 预览后 20 s,V1 点确认
- 期望:
  - 确认请求同 OPS-047;`FakeKS` 收到 0 个请求
  - T1、T2、A1 的私聊与群 −1001 各收到一条 P1 推送,正文含"紧急停止:live-a"、"发起人 boss"、确认时刻、`路径 = 正常`与 incident-response R 的链接
  - `audit` 该行 `result = Applied`、路径 `normal`
- 实现:`python/ops_bot/tests/test_stop.py::test_ops_061_viewer_normal_path`

### OPS-062 Viewer 不带账户的 /stop 对每个实例各预览各确认

- 规则:[紧急停止(/stop)](README.md#紧急停止stop)、[命令的账户语义](README.md#命令的账户语义)
- 级别:unit
- 里程碑:M5
- 前置:`boot()`
- 步骤:
  1. V1 私聊发 `/stop`
  2. V1 点确认
- 期望:
  - 步骤 1:`FakeCtl[prod]` 与 `FakeCtl[sim]` 各收到 1 个 `preview` 的 `Kill{level: Global, account: "*"}`;V1 只收到 1 条预览消息、1 个确认按钮,正文分两节列出 `prod` 与 `sim` 的账户
  - 步骤 2:两个实例各收到 1 个确认请求;回执逐实例列出
- 实现:`python/ops_bot/tests/test_stop.py::test_ops_062_viewer_global_two_instances`

### OPS-063 Trader 与 Admin 的 /stop 立即发出默认参数的 Kill

- 规则:[紧急停止(/stop)](README.md#紧急停止stop)
- 级别:table
- 里程碑:M5
- 前置:`boot()`
- 实现:`python/ops_bot/tests/test_stop.py::test_ops_063_trader_admin_immediate`

| 行 | 发送者与命令 | 期望 |
|---|---|---|
| OPS-063.1 | T1 `/stop live-a` | 立即发 `Kill{Account, live-a, hard: false, flatten: false, cancel_resting: true}`;无预览消息 |
| OPS-063.2 | A1 `/stop live-a` | 同 OPS-063.1 |
| OPS-063.3 | T1 `/stop` | `prod`、`sim` 各立即发 `Kill{Global}` |
| OPS-063.4 | T1 `/stop live-a`,`CmdResult{Applied}` 在 1 s 到达 | P1 推送"紧急停止已生效",发起人 trader1 |

### OPS-064 /stop 只接受账户与尾参数 core

- 规则:[紧急停止(/stop)](README.md#紧急停止stop)
- 级别:table
- 里程碑:M5
- 前置:`boot()`;每行由 V1、T1、A1 各发一次(私聊)
- 实现:`python/ops_bot/tests/test_stop.py::test_ops_064_stop_args`

| 行 | 输入 | 期望(三种角色相同) |
|---|---|---|
| OPS-064.1 | `/stop live-a flatten` | 本地拒绝(`bad_args`) |
| OPS-064.2 | `/stop live-a hard` | 本地拒绝(`bad_args`) |
| OPS-064.3 | `/stop venue VP` | 本地拒绝(`bad_args`) |
| OPS-064.4 | `/stop live-a cancel_resting=false` | 本地拒绝(`bad_args`) |
| OPS-064.5 | `/stop live-a core extra` | 本地拒绝(`bad_args`) |
| OPS-064.6 | `/stop live-a live-b` | 本地拒绝(`bad_args`) |
| OPS-064.7 | `/stop nosuch` | 本地拒绝(`bad_args`),回复"未知账户" |
| OPS-064.8 | `/stop core` | 本地预览"将经 kill-switch 停止 hft-core@prod…"(范围 `*`) |

### OPS-065 预览本身失败时预览改为提示控制面不可达并照样给出确认

- 规则:[紧急停止(/stop)](README.md#紧急停止stop)
- 级别:table
- 里程碑:M5
- 前置:`boot()`;V1 私聊发 `/stop live-a`;`stop.health_timeout_ms = 1000`
- 实现:`python/ops_bot/tests/test_stop.py::test_ops_065_preview_unreachable`

| 行 | 预览时的 `FakeCtl[prod]` | 期望 |
|---|---|---|
| OPS-065.1 | 不可连 | 预览消息含"控制面不可达,确认后若仍不可达将经 kill-switch 撤单并停止 hft-core",带确认按钮 |
| OPS-065.2 | 可连但 10 s 内不回响应(hft-core 无响应) | 同 OPS-065.1 |
| OPS-065.3 | 可连且回 `CmdResult{preview}`(对照) | 预览为 OPS-060 的正文 |

### OPS-066 心跳新鲜与过期的分界

- 规则:[紧急停止(/stop)](README.md#紧急停止stop)
- 级别:table
- 里程碑:M5
- 前置:`boot()`;实例 `prod` 的 `watchdog.hb_stale_ms = 1000`;T1 发 `/stop live-a`,`FakeCtl[prod]` 可连、接受 Kill 但 10 s 内不产生 `CmdResult`(第 5 步超时);`FakeJournal[prod]` 每 100 ms 投递一条 `ClockStatus`(视图不停滞);视图中无第 6 步的其他条件;下表设定超时时刻 `FakeHB[prod]` 的状态
- 实现:`python/ops_bot/tests/test_stop.py::test_ops_066_heartbeat_fresh_boundary`

| 行 | `state` / 心跳年龄 | 期望(t = 10 s 起) |
|---|---|---|
| OPS-066.1 | `Running` / 999 ms | 只发 P0"紧急停止失败(10 s 无结果)";之后 60 s 内 `FakeKS` 0 个请求 |
| OPS-066.2 | `Running` / 1 000 ms | 同 OPS-066.1 |
| OPS-066.3 | `Running` / 1 001 ms | P0 + 立即(t = 10 s)走兜底:`FakeKS` 收到 `stop_core` |
| OPS-066.4 | `Draining` / 100 ms | 同 OPS-066.1 |
| OPS-066.5 | `Warmup` / 100 ms | 同 OPS-066.1 |
| OPS-066.6 | `Stopped` / 100 ms | 同 OPS-066.3 |
| OPS-066.7 | `Starting` / 100 ms | 同 OPS-066.3 |

### OPS-067 ctl socket 单次连接耗时在 stop.health_timeout_ms 处分界

- 规则:[紧急停止(/stop)](README.md#紧急停止stop)
- 级别:table
- 里程碑:M5
- 前置:`boot()`;`stop.health_timeout_ms = 1000`;`FakeCtl[prod]` 每次连接耗时为下表值;T1 发 `/stop live-a`
- 实现:`python/ops_bot/tests/test_stop.py::test_ops_067_connect_timeout_boundary`

| 行 | 连接耗时 | 期望 |
|---|---|---|
| OPS-067.1 | 999 ms | 可连,发出 Kill(第 4 步路径) |
| OPS-067.2 | 1 000 ms | 可连,发出 Kill |
| OPS-067.3 | 1 001 ms | 记为一次不可连;进入第 6 步的每秒重试;t = 10 s 起走兜底 |

### OPS-068 Kill 被拒时自己发 P0 并附 detail,不触发 kill-switch

- 规则:[紧急停止(/stop)](README.md#紧急停止stop)
- 级别:table
- 里程碑:M5
- 前置:`boot()`;T1 发 `/stop live-a`;`FakeCtl[prod]` 回下表结果;`FakeHB[prod]` 新鲜
- 实现:`python/ops_bot/tests/test_stop.py::test_ops_068_kill_rejected_p0`

| 行 | 结果 | 期望 |
|---|---|---|
| OPS-068.1 | `Forbidden` | P0"紧急停止失败",含 `Forbidden` 与 `prod` 的心跳状态;投递到 T1、T2、A1 私聊、群 −1001、Bark;`FakeKS` 0 个请求 |
| OPS-068.2 | `Expired` | 同上,含 `Expired` |
| OPS-068.3 | `Busy` | 同上,含 `Busy` |
| OPS-068.4 | `Internal` | 同上,含 `Internal` |
| OPS-068.5 | `RoleInsufficient` | 同上,含 `RoleInsufficient` |
| OPS-068.6 | `InvalidArgs{UnknownName}` | 同上,含 `UnknownName` |
| OPS-068.7 | `NoOp`(Kill 已在) | P1"紧急停止已生效";没有 P0 |

### OPS-069 等待 CmdResult 的上限在 stop.result_timeout_s 处分界

- 规则:[紧急停止(/stop)](README.md#紧急停止stop)
- 级别:table
- 里程碑:M5
- 前置:`boot()`;`stop.result_timeout_s = 10`;T1 在 t = 0 发 `/stop live-a`;`FakeCtl[prod]` 回 `accepted`;`CmdResult{Applied}` 在下表时刻到达;心跳新鲜、ctl 可连
- 实现:`python/ops_bot/tests/test_stop.py::test_ops_069_result_timeout_boundary`

| 行 | 到达时刻 | 期望 |
|---|---|---|
| OPS-069.1 | 9 999 ms | P1"紧急停止已生效";没有 P0 |
| OPS-069.2 | 10 000 ms | P1;没有 P0 |
| OPS-069.3 | 10 001 ms | 10 000 ms 之后发出 P0"紧急停止失败(10 s 无结果)";10 001 ms 时原消息编辑为 `Applied`;`FakeKS` 0 个请求 |

### OPS-070 超时之后 ctl 不可连而心跳新鲜时,自首次失败起持续 10 s 才兜底

- 规则:[紧急停止(/stop)](README.md#紧急停止stop)
- 级别:unit
- 里程碑:M5
- 前置:`boot()`;`stop.result_timeout_s = 10`、`stop.ctl_unreachable_s = 10`;T1 在 t = 0 发 `/stop live-a`,Kill 被接受但没有 `CmdResult`;t = 10 s 起 `FakeCtl[prod]` 不可连;`FakeHB[prod]` 一直新鲜;`FakeJournal[prod]` 每 100 ms 投递一条 `ClockStatus`;视图中无第 6 步的条件
- 步骤:
  1. 推进到 t = 25 s
- 期望:
  - t = 10 s 发 P0"紧急停止失败(10 s 无结果)"
  - t = 10 s … 20 s 每秒一次连接尝试(共 11 次)
  - t = 20 s 写 `pending_kill.json` 并向 `FakeKS` 发 `stop_core`;t < 20 s 时 `FakeKS` 0 个请求
- 实现:`python/ops_bot/tests/test_stop.py::test_ops_070_timeout_then_unreachable`

### OPS-071 ctl 从一开始就不可连时按 stop.ctl_unreachable_s 分界

- 规则:[紧急停止(/stop)](README.md#紧急停止stop)
- 级别:table
- 里程碑:M5
- 前置:`boot()`;`stop.ctl_unreachable_s = 10`;T1 在 t = 0 发 `/stop live-a`;`FakeCtl[prod]` 自 t = 0 不可连,在下表时刻恢复;`FakeJournal[prod]` 每 100 ms 投递一条 `ClockStatus`;视图中无第 6 步的条件
- 实现:`python/ops_bot/tests/test_stop.py::test_ops_071_unreachable_boundary`

| 行 | 恢复时刻 | 期望 |
|---|---|---|
| OPS-071.1 | 3 s | t = 3 s 的尝试成功,发出 Kill(第 4 步路径);`FakeKS` 0 个请求;没有 P0 |
| OPS-071.2 | 9 s | t = 9 s 发出 Kill;`FakeKS` 0 个请求 |
| OPS-071.3 | 10 s(t = 10 s 那次尝试成功) | 发出 Kill;`FakeKS` 0 个请求 |
| OPS-071.4 | 不恢复 | t = 0 … 10 s 共 11 次尝试;t = 10 s 写 `pending_kill.json` 并发 `stop_core`;t = 9 s 时 `FakeKS` 0 个请求 |
| OPS-071.5 | 不恢复,`FakeHB[prod]` 过期(年龄 5 000 ms) | 同 OPS-071.4(心跳新鲜或过期都一样) |

### OPS-072 兜底 stop_core:先写待补 Kill,再发触发请求并处理回执

- 规则:[紧急停止(/stop)](README.md#紧急停止stop)
- 级别:integ
- 里程碑:M5
- 前置:`boot()`;V1 在 t = 0 预览 `/stop live-a`(预览失败,OPS-065.1),t = 5 s 确认;`FakeCtl[prod]` 自 t = 0 不可连;`FakeJournal[prod]` 每 100 ms 投递一条 `ClockStatus`;视图中无第 6 步的条件;`FakeKS` 在请求到达时读取 `pending_kill.json`,100 ms 回 `Accepted{targets: [(live-a, VS), (live-a, VP)]}`,2 s 回 `Done{core_alive: false, core_stop: "Stopped", marker: true, targets: [… remaining: 0, ok: true]}`
- 步骤:
  1. 推进到 t = 30 s
- 期望:
  - t = 15 s(确认后首次失败起 10 s)`FakeKS` 收到请求;此时它读到的 `pending_kill.json` 已含一条 `{instance: "prod", scope: "live-a", issuer: "tg:1001", state: "pending"}`,`req_id` 与请求相同
  - 请求 = `{v: 1, req_id: <uuid4>, action: "stop_core", instance: "prod", account: "live-a", issuer: "tg:1001", reason: <非空,≤ 200 字节>}`
  - 收到 `Done` 后 P0"紧急停止经 kill-switch 兜底:已撤单,hft-core@prod 已停止",附各目标 `remaining = 0`;投递到 T1、T2、A1 私聊、群 −1001、Bark 各一次
  - 紧急停止推送"路径 = 兜底";`audit` 该行路径 `fallback_stop_core`、`req_id` 同上
- 实现:`python/ops_bot/tests/test_stop.py::test_ops_072_fallback_stop_core`

### OPS-073 停引擎会中断风险处置的每一项条件都使兜底只撤单

- 规则:[紧急停止(/stop)](README.md#紧急停止stop)
- 级别:table
- 里程碑:M5
- 前置:同 OPS-071.4(T1 `/stop live-a`,ctl 一直不可连);t = 10 s 判定前,`FakeJournal[prod]` 已投递下表事件,并每 100 ms 投递一条 `ClockStatus`(行 OPS-073.26、OPS-073.27 除外);`live-a` 的 `ExecConfig.dust_notional = 5 USDT`;`watchdog.hb_stale_ms = 1000`;`ops.view_stale_ms = 3000`;"cancel" = `FakeKS` 收到 `action: "cancel"`、无 `stop_core`、P0 文案含"已撤单;引擎未停止——<原因>"(列出成立的每一项)与"确认要停请发 `/stop live-a core`";"stop_core" = 收到 `action: "stop_core"`
- 实现:`python/ops_bot/tests/test_stop.py::test_ops_073_fallback_conditions`

| 行 | 视图中的事件 | 期望 |
|---|---|---|
| OPS-073.1 | `Note{CapSet}`:`MarginStress`,`live-a`,`level = 1` | stop_core |
| OPS-073.2 | `Note{CapSet}`:`MarginStress`,`live-a`,`level = 2` | cancel,原因含 `MarginStress L2` |
| OPS-073.3 | `Note{CapSet}`:`MarginStress`,`level = 3` | cancel |
| OPS-073.4 | `MarginStress` `level = 2` 之后 `Note{CapCleared}` 同一 cap | stop_core |
| OPS-073.5 | `Note{CapSet}`:`Broken`,`(live-a, 16777217)` | cancel,原因含 `Broken 16777217` |
| OPS-073.6 | `Broken` 之后 `Note{CapCleared{Broken}}` | stop_core |
| OPS-073.7 | `OrderDecision{New, link_role = First}`,无后续 `OrderEvent` | cancel,原因含"在途链接" |
| OPS-073.8 | 第一腿 `OrderDecision{link_role = First}` 后 `OrderEvent{OutcomeUnknown}` | cancel |
| OPS-073.9 | 第一腿 `Fill`(终态)、第二腿 `OrderDecision{link_role = Second}` 无后续 | cancel |
| OPS-073.10 | `OrderDecision{link_role = SecondRetry}` 无后续 | cancel |
| OPS-073.11 | `OrderDecision{link_role = Reverse}` 无后续 | cancel |
| OPS-073.12 | `OrderDecision{link_role = HedgeFix}` 无后续,其他条件都不成立 | stop_core |
| OPS-073.13 | 链接订单都已终态;按 ledger 重建的 g1 两腿差 `|Σ net|` = 0.0001 BTC,mark 60 000(6 USDT) | cancel |
| OPS-073.14 | 同上,mark 50 000(5.000000 USDT) | stop_core |
| OPS-073.15 | 同上,mark 50 000.01(5.000001 USDT) | cancel |
| OPS-073.16 | `Note{CapSet}`:`Kill{Account(live-a)}` 带 `flatten`,其后 `ExecDecision{ForceStep, detail = Engine/Step}`,无结束事件 | cancel,原因含"清仓未完成" |
| OPS-073.17 | 同上,其后该 cell `ExecDecision{State, state_to = Idle}` | stop_core |
| OPS-073.18 | `ExecDecision{ExternalSeen, detail = External/1/Timed}` | cancel,原因含"外来成交计时" |
| OPS-073.19 | 同上,其后 `ExecDecision{ExternalDefault}` | stop_core |
| OPS-073.20 | 同上,其后 `ExecDecision{ExternalSeen, detail = External/0/Cleared}` | stop_core |
| OPS-073.21 | g1 的 `Note{CapSet}`:`Broken`,其后 `ExecDecision{AutoFix, detail = AutoFix/Start}` | cancel,原因含 `Broken 16777217` 与"自动平仓执行中" |
| OPS-073.22 | 同上,其后 `AutoFix/Done` | cancel,原因含 `Broken`、不含"自动平仓执行中" |
| OPS-073.23 | 同 OPS-073.21,其后 `AutoFix/Failed` | 同 OPS-073.22 |
| OPS-073.24 | 同 OPS-073.21,其后 `ExecDecision{CmdHedgeFix}` | 同 OPS-073.22 |
| OPS-073.25 | 同 OPS-073.21,其后全局 HARD 的 `Note{CapSet}` | 同 OPS-073.22 |
| OPS-073.26 | 视图中无条件事件;journal 尾随在判定时刻已 3 000 ms 未前进 | stop_core |
| OPS-073.27 | 同上,3 001 ms 未前进 | cancel,原因含"视图停滞" |
| OPS-073.29 | 同上,2 999 ms 未前进 | stop_core |
| OPS-073.28 | `sim` 实例的 `sim-a` 有 `MarginStress level = 3`,`prod` 无条件 | stop_core(只看 `prod`) |
| OPS-073.29 | 条件属于 `live-b`(同一 `prod` 实例)而 `/stop live-a` | cancel(按实例判定) |
| OPS-073.30 | 同 OPS-073.21,其后 g1 的 `Note{CapCleared{Broken}}`(无 `AutoFix/Done`) | stop_core |

### OPS-074 只撤单的兜底同样先写待补 Kill,hft-core 不停

- 规则:[紧急停止(/stop)](README.md#紧急停止stop)
- 级别:unit
- 里程碑:M5
- 前置:OPS-073.2 的情形;`FakeKS` 回 `Accepted`、`Done{action: "cancel", core_alive: true}`(无 `core_stop`、无 `marker`)
- 步骤:
  1. 推进到 t = 15 s
- 期望:
  - `FakeKS` 收到请求时 `pending_kill.json` 已有该条目(`scope = "live-a"`、`state = "pending"`)
  - 请求 `action = "cancel"`、`instance = "prod"`、`account = "live-a"`
  - P0 正文 = "紧急停止经 kill-switch 兜底:已撤单;引擎未停止——MarginStress L2;停止引擎会中断减仓、对冲与补腿,确认要停请发 `/stop live-a core`"
- 实现:`python/ops_bot/tests/test_stop.py::test_ops_074_cancel_only_keeps_core`

### OPS-075 /stop … core 的第二次确认

- 规则:[紧急停止(/stop)](README.md#紧急停止stop)
- 级别:table
- 里程碑:M5
- 前置:OPS-074 之后(t = 15 s);`confirm.ttl_s = 60`;每行发送者先发 `/stop live-a core`,收到本地预览"将经 kill-switch 停止 hft-core@prod:该实例全部账户停止交易,L3 减仓、对冲与补腿中断"
- 实现:`python/ops_bot/tests/test_stop.py::test_ops_075_stop_core_second_confirm`

| 行 | 动作 | 期望 |
|---|---|---|
| OPS-075.1 | V1 在 30 s 后确认,ctl 仍不可连 | 新增一条待补 Kill 条目(新 `req_id`),随后 `FakeKS` 收到 `stop_core`(`account = "live-a"`,`issuer = "tg:1001"`) |
| OPS-075.2 | T1 在 30 s 后确认,ctl 仍不可连 | 同 OPS-075.1,`issuer = "tg:2001"` |
| OPS-075.3 | V1 在 60 s 时确认 | 执行 |
| OPS-075.4 | V1 在 61 s 时确认 | 不执行;`denied: expired`;`FakeKS` 无新请求 |
| OPS-075.5 | T2 点 V1 预览的确认 | 不执行;`denied: not_initiator` |
| OPS-075.6 | V1 确认时 ctl 已可连 | 回复 `控制面已恢复,按正常路径停止`;`FakeCtl[prod]` 先收到 `Kill{Account(live-a)}` 的 `preview`,随即收到同一命令的确认(`issuer = "tg:1001"`,无 `expect_state_version`);`FakeKS` 无新请求;不新增待补条目 |
| OPS-075.7 | T1 确认时 ctl 已可连 | 回复同上;`FakeCtl[prod]` 收到立即发出的 `Kill{Account(live-a)}` |
| OPS-075.8 | T1 发 `/stop live-a core` 后不确认 | `FakeKS` 无新请求(Trader 也必须确认) |

### OPS-076 kill-switch 拒绝或不可达时 P0 注明兜底也失败

- 规则:[紧急停止(/stop)](README.md#紧急停止stop)
- 级别:table
- 里程碑:M5
- 前置:同 OPS-071.4;`FakeKS` 按下表行为;"兜底失败 P0" = P0 正文含"兜底也失败,按 incident-response G 手动 `kill-switch fire`"与原因
- 实现:`python/ops_bot/tests/test_stop.py::test_ops_076_kill_switch_failures`

| 行 | `FakeKS` | 期望 |
|---|---|---|
| OPS-076.1 | `Rejected{reason: "BadRequest"}` | 兜底失败 P0,含 `BadRequest`;待补条目保持 `pending` |
| OPS-076.2 | `Rejected{reason: "UnknownInstance"}` | 同上,含 `UnknownInstance` |
| OPS-076.3 | `Rejected{reason: "UnknownAccount"}` | 同上,含 `UnknownAccount` |
| OPS-076.4 | `Rejected{reason: "SimAccount"}` | 同上,含 `SimAccount` |
| OPS-076.5 | `Rejected{reason: "NoTarget"}` | 同上,含 `NoTarget` |
| OPS-076.6 | `Rejected{reason: "StopNotAllowed"}` | 同上,含 `StopNotAllowed` |
| OPS-076.7 | 不监听(套接字文件不存在) | 兜底失败 P0,含"套接字不可连" |
| OPS-076.8 | 连接被拒(`ECONNREFUSED`) | 同 OPS-076.7 |
| OPS-076.9 | 1 000 ms 回 `Accepted` | 不算失败,继续等 `Done` |
| OPS-076.10 | 1 001 ms 才回 `Accepted` | 1 000 ms 时判为兜底失败 P0,含"未受理" |

### OPS-077 Done 回执的停机结果与停机标记决定 P0 文案

- 规则:[紧急停止(/stop)](README.md#紧急停止stop)
- 级别:table
- 里程碑:M5
- 前置:同 OPS-072(无条件,发 `stop_core`);`FakeKS` 的 `Done` 取下表值
- 实现:`python/ops_bot/tests/test_stop.py::test_ops_077_done_receipt_text`

| 行 | `core_stop` / `marker` | 期望 P0 正文 |
|---|---|---|
| OPS-077.1 | `Stopped` / `true` | 含"已撤单,hft-core@prod 已停止";不含"仍在运行"、不含"停机标记未生效" |
| OPS-077.2 | `Inactive` / `true` | 同 OPS-077.1 |
| OPS-077.3 | `Timeout` / `false` | 加粗"hft-core 进程仍在运行,策略可能重新下单——按 incident-response R2 处理";另含"停机标记未生效,重启后的禁开只靠待补 Kill" |
| OPS-077.4 | `Denied` / `false` | 同 OPS-077.3 |
| OPS-077.5 | `Failed` / `false` | 同 OPS-077.3 |
| OPS-077.6 | `Stopped` / `false`(标记写入失败) | 含"已停止"与"停机标记未生效,重启后的禁开只靠待补 Kill";不含"仍在运行" |
| OPS-077.7 | 以上每一行之后 | `pending_kill.json` 中该条目 `state = "pending"` |

### OPS-078 目标只有模拟账户时只发 P1,不经 kill-switch

- 规则:[紧急停止(/stop)](README.md#紧急停止stop)
- 级别:table
- 里程碑:M5
- 前置:`boot()`;`FakeCtl[sim]` 自 t = 0 不可连;`stop.ctl_unreachable_s = 10`
- 实现:`python/ops_bot/tests/test_stop.py::test_ops_078_sim_only_target`

| 行 | 输入 | 期望(t = 15 s 时) |
|---|---|---|
| OPS-078.1 | V1 `/stop sim-a` 并确认 | T1、T2、A1 私聊各一条 P1"模拟实例不健康,`/stop` 未生效";`FakeKS` 0 个请求;`pending_kill.json` 不存在;没有 P0 |
| OPS-078.2 | T1 `/stop sim-a` | 同 OPS-078.1 |
| OPS-078.3 | T1 `/stop`,`FakeCtl[prod]` 也不可连 | `prod` 走兜底(`stop_core`,`account = "*"`)并 P0;`sim` 只发 P1 |
| OPS-078.4 | T1 `/stop`,`FakeCtl[prod]` 可连并 `Applied` | `prod` 的 Kill `Applied`,P1"紧急停止已生效";`sim` 只发 P1"模拟实例不健康" |

### OPS-079 每次触发请求用新的 uuid4 作 req_id

- 规则:[紧急停止(/stop)](README.md#紧急停止stop)、[实现说明](README.md#实现说明)
- 级别:unit
- 里程碑:M5
- 前置:OPS-074(发 `cancel`)之后接 OPS-075.1(发 `stop_core`)
- 步骤:
  1. 收集两次触发请求的 `req_id`
- 期望:
  - 两个 `req_id` 都是合法 UUID 且版本号为 4;两者不同;与各自待补条目的 `req_id` 一一相同
- 实现:`python/ops_bot/tests/test_stop.py::test_ops_079_req_id_uuid4`

### OPS-080 /stop 的 P0 只投一次,未确认时每 5 min 重投

- 规则:[推送](README.md#推送)、[紧急停止(/stop)](README.md#紧急停止stop)
- 级别:table
- 里程碑:M5
- 前置:OPS-072 的 P0 在时刻 `tp` 首次投递,id `ops-1`;"一次投递" = T1、T2、A1 私聊、群 −1001 与 `FakeBark` 各一条
- 实现:`python/ops_bot/tests/test_push.py::test_ops_080_p0_redelivery`

| 行 | 动作 | 期望 |
|---|---|---|
| OPS-080.1 | 无 `/ack`,推进到 tp + 299 s | 仍只有首次那 1 次投递 |
| OPS-080.2 | 无 `/ack`,推进到 tp + 300 s | 第 2 次投递 |
| OPS-080.3 | 无 `/ack`,推进到 tp + 600 s | 第 3 次投递 |
| OPS-080.4 | T1 在 tp + 400 s 发 `/ack ops-1` | `pages` 表 `ops-1` 已确认;`acks.jsonl` 追加一行 `id = "ops-1"`;tp + 600 s 无投递 |
| OPS-080.5 | 同一次 `/stop` 的 `Accepted` 与 `Done` 都到达 | P0 只在 `Done` 后投递一次(不为 `Accepted` 另投) |
| OPS-080.6 | V1 发 `/ack ops-1` | 本地拒绝(`viewer`);仍按 OPS-080.2 重投 |

### OPS-081 紧急停止的结果推送给全部 Trader/Admin 与群,不受订阅影响

- 规则:[紧急停止(/stop)](README.md#紧急停止stop)、[推送](README.md#推送)
- 级别:table
- 里程碑:M5
- 前置:`boot()`;T2 的 `push_accounts = ["live-a"]`;A1 发 `/sub live-a off`(订阅为空)
- 实现:`python/ops_bot/tests/test_push.py::test_ops_081_stop_push_ignores_subscription`

| 行 | 事件 | 期望 |
|---|---|---|
| OPS-081.1 | T1 `/stop live-b`,`CmdResult{Applied}` | T1、T2、A1 私聊与群 −1001 各一条 `紧急停止:live-b,发起人 trader1,…,路径 = 正常`,含 incident-response R 链接 |
| OPS-081.2 | V1 `/stop live-b` 兜底 `stop_core` | 同上,"路径 = 兜底",附 kill-switch 回执(含 hft-core 是否已停止) |
| OPS-081.3 | T1 `/stop live-b` 被拒 | 同上,"路径 = 失败" |
| OPS-081.4 | 同 OPS-081.1 | V1、V2 私聊没有该推送(只推给 Trader/Admin 与群;发起人 V1 只在自己的会话里得到回复) |

### OPS-082 /stop 不受限速

- 规则:[角色与鉴权](README.md#角色与鉴权)、[紧急停止(/stop)](README.md#紧急停止stop)
- 级别:unit
- 里程碑:M5
- 前置:`boot()`;`limits.writes_per_min = 12`;T1 在 60 s 内已发 12 条 `/kill account live-a`
- 步骤:
  1. T1 在同一窗口内发 20 条 `/stop live-a` 与 1 条 `/stop live-a core`
- 期望:
  - 20 条 `Kill` 都发出;`/stop live-a core` 生成本地预览;`audit` 中没有 `denied: rate_limited` 的 `/stop` 行
- 实现:`python/ops_bot/tests/test_stop.py::test_ops_082_stop_not_rate_limited`

### OPS-083 /stop 的确认处理不依赖 journal 尾随任务

- 规则:[实现说明](README.md#实现说明)、[紧急停止(/stop)](README.md#紧急停止stop)
- 级别:unit
- 里程碑:M5
- 前置:`boot()`;V1 预览 `/stop live-a`;之后让 `prod` 的 journal 尾随任务阻塞(假 `tail()` 永不返回)
- 步骤:
  1. V1 点确认
- 期望:
  - 确认后 500 ms 内 `FakeCtl[prod]` 收到确认请求;健康判定读取了 `FakeHB[prod]`
- 实现:`python/ops_bot/tests/test_stop.py::test_ops_083_confirm_independent_of_tail`

### OPS-084 Viewer 发 /resume 被拒并提示联系交易员

- 规则:[紧急停止(/stop)](README.md#紧急停止stop)
- 级别:table
- 里程碑:M5
- 前置:`boot()`;发送者 V1(私聊)
- 实现:`python/ops_bot/tests/test_stop.py::test_ops_084_viewer_resume_denied`

| 行 | 输入 | 期望 |
|---|---|---|
| OPS-084.1 | `/resume kill live-a` | 本地拒绝(`viewer`),回复"恢复请联系交易员" |
| OPS-084.2 | `/resume recovery live-a` | 同上 |
| OPS-084.3 | `/resume dailyloss live-a` | 同上 |

### OPS-085 Viewer 的 /stop 预览只有发起人本人能确认

- 规则:[紧急停止(/stop)](README.md#紧急停止stop)、[二次确认](README.md#二次确认)
- 级别:table
- 里程碑:M5
- 前置:`boot()`;每行从 V1 新发的 `/stop live-a` 预览开始(预览在给定聊天中),预览后 10 s 发生下表点击
- 实现:`python/ops_bot/tests/test_stop.py::test_ops_085_only_initiator_confirms`

| 行 | 预览所在聊天与点击者 | 期望 |
|---|---|---|
| OPS-085.1 | 群 −1001,T1 点确认 | 不执行;`denied: not_initiator`;`FakeCtl[prod]` 只有预览请求 |
| OPS-085.2 | 群 −1001,A1 点确认 | 同 OPS-085.1 |
| OPS-085.3 | 群 −1001,V2(另一个 Viewer)点确认 | 同 OPS-085.1 |
| OPS-085.4 | 群 −1001,V1 点确认 | 发出确认,`issuer = "tg:1001"`、`chat_type = "group"` |
| OPS-085.5 | 私聊 1001,V1 点确认 | 发出确认 |

### OPS-086 采纳余额条目后对账两轮 Ignore 再恢复 Recon

- 规则:[命令表](README.md#命令表)
- 级别:integ
- 里程碑:M5
- 前置:进程内 hft-ctl gateway + sequencer(hft-testkit 假交易所与 ReconActor);`live-a` 有 `Recon{live-a, VS}` cap,原因是 `VS` USDT 余额差 +50(报告 `report_id = 77`);ops_bot 的 `FakeCtl[prod]` 换成真实 ctl socket
- 步骤:
  1. T1 发 `/resume recon live-a` 并确认
  2. T1 发 `/adopt live-a VS USDT 77` 并确认
  3. ReconActor 写两轮全部 `Ignore` 的 `ReconcileReport`
  4. T1 发 `/resume recon live-a` 并确认
- 期望:
  - 步骤 1:`CmdResult{PreconditionFailed, detail = ReconNotClean}`,回复含建议"先 /adopt"
  - 步骤 2:`CmdResult{Applied}`
  - 步骤 4:`CmdResult{Applied}`;之后 cap 集合中没有 `Recon{live-a, VS}`
- 实现:`python/ops_bot/tests/test_commands.py::test_ops_086_adopt_then_resume_recon`

### OPS-087 BotFather 设置禁止加入任意群、开启隐私模式

- 规则:[角色与鉴权](README.md#角色与鉴权)
- 级别:host
- 里程碑:M5
- 环境:dev2 与升级主机各自的 bot(各用自己的 token)
- 授权:无
- 步骤:
  1. 对每个 bot 调 Telegram `getMe`
- 通过标准:`can_join_groups = false`、`can_read_all_group_messages = false`
- 证据:`getMe` 的 JSON 输出(去掉 token),文件名含 `OPS-087` 与主机名
- 实现:`python/ops_bot/tests/host/OPS-087.sh`

### OPS-090 待补 Kill 条目的字段

- 规则:[紧急停止(/stop)](README.md#紧急停止stop)
- 级别:unit
- 里程碑:M5
- 前置:OPS-072 的情形(V1 `/stop live-a`,兜底 `stop_core`);兜底判定时刻 `tf = t0 + 15 s`
- 步骤:
  1. 读 `<tmp>/state/pending_kill.json`
- 期望:
  - JSON 数组恰 1 项:`{req_id: <与触发请求相同>, instance: "prod", scope: "live-a", issuer: "tg:1001", reason: <与触发请求相同>, at_utc: tf, state: "pending"}`,没有其他键
- 实现:`python/ops_bot/tests/test_pending_kill.py::test_ops_090_entry_fields`

### OPS-091 待补 Kill 以临时文件加 os.replace 原子写入,写入中途崩溃不丢旧条目

- 规则:[紧急停止(/stop)](README.md#紧急停止stop)、[实现说明](README.md#实现说明)
- 级别:table
- 里程碑:M5
- 前置:`pending_kill.json` 已有一条 `scope = "live-b"` 的 `pending` 条目;OPS-071.4 的情形在 t = 10 s 要写入 `scope = "live-a"` 的新条目;测试在给定位置注入进程崩溃(`os._exit`),随后以同一 `paths.state` 重启 ops_bot(`boot()` 其余不变)
- 实现:`python/ops_bot/tests/test_pending_kill.py::test_ops_091_atomic_write_crash`

| 行 | 崩溃位置 | 期望(重启之后) |
|---|---|---|
| OPS-091.1 | 写完 `pending_kill.json.tmp`、`os.replace` 之前 | `pending_kill.json.tmp` 被删除;`pending_kill.json` 只有 `live-b` 一条;崩溃前 `FakeKS` 收到 0 个请求 |
| OPS-091.2 | `os.replace` 之后、向 `FakeKS` 发请求之前 | 文件有 `live-b`、`live-a` 两条 `pending`;`FakeKS` 0 个请求;`prod` 可用后按 OPS-093 补发两条 |
| OPS-091.3 | 任一时刻读 `pending_kill.json` | 解析成功(不出现半个 JSON) |

### OPS-092 待补条目只在 prod 心跳新鲜且 ctl 可连时处理

- 规则:[紧急停止(/stop)](README.md#紧急停止stop)
- 级别:table
- 里程碑:M5
- 前置:`pending_kill.json` 有一条 `{scope: "live-a", at_utc: t0, state: "pending"}`;`boot()` 之后的 `prod` 状态按下表;`FakeJournal[prod]` 中 `t0` 之后没有 `Kill`/`Resume`
- 实现:`python/ops_bot/tests/test_pending_kill.py::test_ops_092_availability_gate`

| 行 | `FakeHB[prod]` / `FakeCtl[prod]` | 期望(60 s 内) |
|---|---|---|
| OPS-092.1 | 新鲜 / 可连 | 补发 1 条 `Kill` |
| OPS-092.2 | 新鲜 / 不可连 | 不补发 |
| OPS-092.3 | 过期(年龄 1 001 ms)/ 可连 | 不补发 |
| OPS-092.4 | `Stopped` / 可连 | 不补发 |
| OPS-092.5 | 先 OPS-092.2,30 s 后 ctl 变为可连 | 变为可连后补发 1 条 |

### OPS-093 补发以 ops_bot 主体发默认参数的 Kill,结果为 Applied 或 NoOp 即完成

- 规则:[紧急停止(/stop)](README.md#紧急停止stop)、[审计](README.md#审计)
- 级别:table
- 里程碑:M5
- 前置:同 OPS-092.1;`FakeCtl[prod]` 对补发的 `Kill` 回下表结果
- 实现:`python/ops_bot/tests/test_pending_kill.py::test_ops_093_reissue`

| 行 | 结果 | 期望 |
|---|---|---|
| OPS-093.1 | `Applied` | 补发请求 = `Kill{level: Account, account: live-a, hard: false, flatten: false, cancel_resting: true}`,`issuer = "ops_bot"`,无 `preview`/`confirmed`;随后 `Annotate{kind: OpsNote}`(`issuer = "ops_bot"`,正文含原 `req_id`、范围 `live-a`、结果 `Applied`);T1、T2、A1 私聊各一条 P1"待补 Kill 已生效"(范围、原 `req_id`、补发时刻);该条目从文件移除;`audit` 有补发一行 |
| OPS-093.2 | `NoOp`(停机标记的 Kill 已在) | 同 OPS-093.1,结果写 `NoOp` |
| OPS-093.3 | 条目 `scope = "*"` 且 `Applied` | 补发 `Kill{level: Global, account: "*"}` |

### OPS-094 补发先于其他任何发往 prod 的命令

- 规则:[紧急停止(/stop)](README.md#紧急停止stop)
- 级别:unit
- 里程碑:M5
- 前置:同 OPS-092.2(ctl 不可连);T1 在不可连期间发 `/kill account live-b`(进入第 6 步之外的普通写命令,回复"控制面不可达"),随后 ctl 变为可连的同一刻 T1 再发 `/mode pc-2 live-b closeonly`
- 步骤:
  1. 推进到 ctl 可连后 5 s
- 期望:
  - `FakeCtl[prod]` 收到的第一条写命令是补发的 `Kill{Account(live-a)}`(`issuer = "ops_bot"`);`SetMode` 在它之后
  - 第一条 `/kill account live-b` 回复"控制面不可达",没有被补发
- 实现:`python/ops_bot/tests/test_pending_kill.py::test_ops_094_reissue_first`

### OPS-095 补发前查 journal:同一范围在条目时刻之后已被 Kill 又被 Resume 时不补发

- 规则:[紧急停止(/stop)](README.md#紧急停止stop)
- 级别:table
- 里程碑:M5
- 前置:`pending_kill.json` 一条 `{req_id: R, scope: <列>, at_utc: t0, state: pending}`;ops_bot 在 `t0` 到 `t0 + 120 s` 之间宕机;`FakeJournal[prod]` 中有下表记录;`prod` 可用后 ops_bot 启动
- 实现:`python/ops_bot/tests/test_pending_kill.py::test_ops_095_journal_check`

| 行 | 条目 `scope` | journal 中的记录 | 期望 |
|---|---|---|---|
| OPS-095.1 | `live-a` | `t0 + 5 s` `ControlCmd{Kill{Account(live-a)}}` `Applied`(issuer `cli:alice`),`t0 + 60 s` `Resume{Kill, Account(live-a)}` `Applied` | 不补发;`Annotate{OpsNote}` 正文含 `R`、`live-a`、"已由人工恢复,未补发";条目移除;P1 告知 |
| OPS-095.2 | `live-a` | 只有 `t0 + 5 s` 的 `Kill{Account(live-a)}` | 补发,结果 `NoOp` → 按 OPS-093.2 完成 |
| OPS-095.3 | `live-a` | `Kill` 在 `t0 − 60 s`、`Resume{Kill}` 在 `t0 − 10 s` | 补发 |
| OPS-095.4 | `live-a` | `t0` 之后对 `live-b` 的 `Kill` 与 `Resume{Kill}` | 补发 `Kill{Account(live-a)}` |
| OPS-095.5 | `*` | `t0` 之后对 `live-a` 的 `Kill` 与 `Resume{Kill}` | 补发 `Kill{Global}` |
| OPS-095.6 | `*` | `t0` 之后 `Kill{Global}` 与 `Resume{Kill, Global}` | 不补发;按 OPS-095.1 处理 |
| OPS-095.7 | `live-a` | `t0` 之后 `Resume{Kill, Account(live-a)}` 先于 `Kill`(之后没有 Resume) | 补发 |
| OPS-095.8 | `live-a` | 启动时 `cli:systemd` 的停机标记 `Kill{Account(live-a)}`(`t0 + 30 s`),无 Resume | 补发,结果 `NoOp` |

### OPS-096 补发被拒时发 P0 并每 30 s 重试

- 规则:[紧急停止(/stop)](README.md#紧急停止stop)、[推送](README.md#推送)
- 级别:table
- 里程碑:M5
- 前置:同 OPS-092.1;补发在 `tr` 时刻发出;`FakeCtl[prod]` 对前 2 次补发回 `Forbidden`,第 3 次回 `Applied`
- 实现:`python/ops_bot/tests/test_pending_kill.py::test_ops_096_reissue_retry`

| 行 | 时刻 | 期望 |
|---|---|---|
| OPS-096.1 | `tr` | 第 1 次补发被拒;P0(自投递,含 `Forbidden`)一次;条目仍 `pending` |
| OPS-096.2 | `tr + 29 s` | 仍只有 1 次补发 |
| OPS-096.3 | `tr + 30 s` | 第 2 次补发(`cmd_id` 与第 1 次不同);被拒;不重复投递同一 P0(按 OPS-080 的重投规则) |
| OPS-096.4 | `tr + 60 s` | 第 3 次补发 `Applied`;条目移除;之后 `tr + 90 s` 无补发 |

### OPS-097 待补条目未完成时其范围内账户的 /resume 一律本地拒绝

- 规则:[紧急停止(/stop)](README.md#紧急停止stop)、[命令表](README.md#命令表)
- 级别:table
- 里程碑:M5
- 前置:`boot()`;`FakeCtl[prod]` 不可连(条目不会被处理);`pending_kill.json` 条目按下表
- 实现:`python/ops_bot/tests/test_pending_kill.py::test_ops_097_resume_blocked`

| 行 | 条目 `scope` | 发送者与命令 | 期望 |
|---|---|---|---|
| OPS-097.1 | `live-a` | T1 `/resume kill live-a` | 本地拒绝(`pending_kill`),回复"待补 Kill 未完成" |
| OPS-097.2 | `live-a` | T1 `/resume recovery live-a` | 同上 |
| OPS-097.3 | `live-a` | A1 `/resume dailyloss live-a` | 同上 |
| OPS-097.4 | `live-a` | T1 `/resume broken live-a 16777217` | 同上 |
| OPS-097.5 | `live-a` | T1 `/resume kill live-b` | 不受此限(`FakeCtl` 不可连 → 回复"控制面不可达") |
| OPS-097.6 | `*` | T1 `/resume kill live-b` | 本地拒绝(`pending_kill`) |
| OPS-097.7 | `*` | T1 `/resume kill sim-a` | 不受此限(`sim` 实例:预览发往 `FakeCtl[sim]`) |
| OPS-097.8 | 条目已移除(OPS-093.1 之后) | T1 `/resume kill live-a` | 预览发出 |

### OPS-098 /status 列出未完成的待补条目

- 规则:[紧急停止(/stop)](README.md#紧急停止stop)、[命令表](README.md#命令表)
- 级别:unit
- 里程碑:M5
- 前置:`pending_kill.json` 两条:`{req_id: R1, scope: live-a, at_utc: t0, state: pending}`、`{req_id: R2, scope: "*", at_utc: t0 + 60 s, state: pending}`
- 步骤:
  1. V1 发 `/status`
- 期望:
  - 回复含"待补 Kill"一节,两行分别含 `R1`、`live-a`、`t0` 与 `R2`、`*`、`t0 + 60 s`
- 实现:`python/ops_bot/tests/test_pending_kill.py::test_ops_098_status_lists_pending`

### OPS-099 ops_bot 在待补期间重启后先读文件并照常补发

- 规则:[紧急停止(/stop)](README.md#紧急停止stop)、[实现说明](README.md#实现说明)
- 级别:integ
- 里程碑:M5
- 前置:OPS-072 之后(条目 `pending`,`prod` 已停);ops_bot 进程被 `SIGKILL`
- 步骤:
  1. 重启 ops_bot(`FakeCtl[prod]` 仍不可连)
  2. 启动 `prod`(`FakeHB` 新鲜、`FakeCtl` 可连)
- 期望:
  - 步骤 1 后、处理任何 update 之前已读入条目:此时 T1 `/resume kill live-a` 被本地拒绝(`pending_kill`)
  - 步骤 2 后补发 `Kill{Account(live-a)}`,`Applied` 后条目移除
- 实现:`python/ops_bot/tests/test_pending_kill.py::test_ops_099_restart_recovers_pending`

### OPS-100 待补文件缺失、残留临时文件与损坏的处理

- 规则:[紧急停止(/stop)](README.md#紧急停止stop)
- 级别:table
- 里程碑:M5
- 前置:`boot()` 之前按下表准备 `<tmp>/state/`;`FakeCtl[prod]` 可连、`FakeHB[prod]` 新鲜;`FakeJournal[prod]` 中没有 `Kill`/`Resume`
- 实现:`python/ops_bot/tests/test_pending_kill.py::test_ops_100_missing_tmp_corrupt`

| 行 | `<tmp>/state/` | 期望 |
|---|---|---|
| OPS-100.1 | 没有 `pending_kill.json` | 没有补发;`/resume kill live-a` 预览发出 |
| OPS-100.2 | 只有 `pending_kill.json.tmp`(内容为一条条目) | 启动后 `.tmp` 被删除;没有补发 |
| OPS-100.3 | `pending_kill.json` 内容为 `{"req_` | 改名为 `pending_kill.json.corrupt-<启动时 utc_ns>`;P0(自投递)含"pending_kill.json 损坏";按 `{scope: "*", issuer: "ops_bot", at_utc: <原文件 mtime>}` 补发 `Kill{Global}` 到 `prod` |
| OPS-100.4 | 同 OPS-100.3 但 `FakeCtl[prod]` 不可连 | `/resume kill live-a`、`/resume kill live-b` 本地拒绝(`pending_kill`);`/resume kill sim-a` 不受此限 |

### OPS-101 SQLite 损坏时待补 Kill 仍可读并处理

- 规则:[审计](README.md#审计)、[紧急停止(/stop)](README.md#紧急停止stop)
- 级别:unit
- 里程碑:M5
- 前置:`pending_kill.json` 一条 `scope = "live-a"` 的 `pending` 条目;`<tmp>/state/ops.sqlite3` 被截断为 100 字节
- 步骤:
  1. `boot()`(`prod` 可用)
- 期望:
  - 补发 `Kill{Account(live-a)}` 并在 `Applied` 后移除条目;T1 私聊收到 P1"待补 Kill 已生效"
- 实现:`python/ops_bot/tests/test_pending_kill.py::test_ops_101_sqlite_corrupt_pending_survives`
- 备注:SQLite 损坏时其他功能的处理不在本用例

### OPS-102 同一范围的两条待补条目各补发一次

- 规则:[紧急停止(/stop)](README.md#紧急停止stop)
- 级别:unit
- 里程碑:M5
- 前置:`pending_kill.json` 两条 `scope = "live-a"`(先 `cancel` 兜底、后 `/stop live-a core` 的 `stop_core`,`req_id` 分别 R1、R2);`prod` 可用;`FakeCtl[prod]` 对第一条回 `Applied`,第二条回 `NoOp`
- 步骤:
  1. 推进 10 s
- 期望:
  - 两条补发 `Kill{Account(live-a)}`;两条 `Annotate{OpsNote}` 分别含 R1、R2;文件为空数组
- 实现:`python/ops_bot/tests/test_pending_kill.py::test_ops_102_two_entries_same_scope`

### OPS-110 /status 汇总每个实例的健康与闸门

- 规则:[命令表](README.md#命令表)、[实现说明](README.md#实现说明)
- 级别:unit
- 里程碑:M5
- 前置:`boot()`;`FakeHB[prod]` `Running`、心跳年龄 120 ms;视图中 `live-a` 的启动闸门 `IDENTITY` 已放行、`ACCOUNT_CFG` 未放行(`VP`);生效 cap `Recovery{live-a}`;`ConnState{VP, Order, Up}`;watchtower 状态中 persist 滞后 30 ms、archiver 滞后 2 h;最近一份 reconciler 报告 `report_id = 88`;kill-switch 对 `prod` 的判活状态 `Watching`;`pc-1` 的 Model 检查点已载入、预热 `done`;主机不是 dev2
- 步骤:
  1. V1 发 `/status`
- 期望:
  - `prod` 节含 `Running`、`120 ms`、`live-a IDENTITY ✓`、`live-a/VP ACCOUNT_CFG ✗`、`Recovery`、`VP Order Up`、`persist 30 ms`、`archiver 2 h`、`report 88`、`kill-switch Watching`、`pc-1 检查点 已载入`、`预热 done`
  - 不含"护栏"一节
- 实现:`python/ops_bot/tests/test_panels.py::test_ops_110_status_contents`

### OPS-111 dev2 上 /status 另显示护栏守护状态

- 规则:[命令表](README.md#命令表)
- 级别:unit
- 里程碑:M5
- 前置:同 OPS-110,主机为 dev2;watchtower 状态中护栏 `tripped = false`、守护单元 `active`
- 步骤:
  1. V1 发 `/status`
- 期望:
  - 回复含"护栏"一节,内容 `active`、`未触发`
- 实现:`python/ops_bot/tests/test_panels.py::test_ops_111_status_dev2_guard`

### OPS-112 hft-core 不可用时 /help 与 /status 仍响应

- 规则:[命令表](README.md#命令表)
- 级别:table
- 里程碑:M5
- 前置:`boot()`;`FakeHB[prod]` 文件不存在(`open_ro` 失败)、`FakeCtl[prod]` 不可连、`FakeJournal[prod]` 停止前进
- 实现:`python/ops_bot/tests/test_panels.py::test_ops_112_offline_commands`

| 行 | 命令 | 期望 |
|---|---|---|
| OPS-112.1 | `/help` | 回复手册正文(本地渲染) |
| OPS-112.2 | `/status` | `prod` 节显示"不可用";`sim` 节照常 |
| OPS-112.3 | `/kill account live-a`(T1) | 回复"控制面不可达"并列出 `prod`;`FakeKS` 0 个请求;`audit` 该行 `result = unreachable` |
| OPS-112.4 | `/stop live-a`(T1) | 按第 6 步兜底(见 OPS-071.4) |

### OPS-113 账户面板的字段

- 规则:[命令表](README.md#命令表)
- 级别:table
- 里程碑:M5
- 前置:`boot()`;`FakeJournal[prod]` 中 `live-a` 的 `PortfolioView`、`MarginReport`、`AccountConfigReport`、目标、订单、`ExecDecision` 与 `ExternalFill` 记录(夹具文件 `tests/fixtures/journal/live_a_panels.jsonl`);发送者 V1
- 实现:`python/ops_bot/tests/test_panels.py::test_ops_113_panel_fields`

| 行 | 命令 | 期望回复含 |
|---|---|---|
| OPS-113.1 | `/p live-a` | 每 venue 现金 free/locked、现货库存(按 group 与 unassigned)、永续持仓与未实现、保证金余额、可用保证金、保证金率、合计权益 |
| OPS-113.2 | `/risk live-a` | MarginStress 档位;`VP` 的 `liq_dist`、决定它的 inst、`liq_px` 来源(`交易所`/`账本估算`)与到 L1/L2/L3 的距离;交叉校验比率;`BTC/VP` 实际杠杆与 ADL 档;DailyLoss 余量;Broken 持续时长与距自动平仓的剩余时间;清仓锁;最近 `ExternalFill`、`ExecAutoFixDone`、`FundingCorrected` |
| OPS-113.3 | `/targets live-a` | `16777217` 各腿目标净量、原因、紧急度、不交易带、`TargetView` 进度、最近 `ExecDecision` |
| OPS-113.4 | `/orders live-a` | 在途订单、Unknown 槽、链接状态 |
| OPS-113.5 | `/pnl live-a` | 资金费、基差、手续费、滑点、`adjust`,资金费自动校正单列 |

### OPS-114 历史 PnL 按日读 S3 Parquet,凭证文件变化时重建凭证

- 规则:[命令表](README.md#命令表)、[依赖](README.md#依赖)
- 级别:table
- 里程碑:M5
- 前置:`boot()`;`/run/hft/creds/hft-opsbot/credentials` 的测试副本 `<tmp>/creds/credentials`;S3 用本地 Parquet 目录模拟;记录 DuckDB `CREATE SECRET` 与 polars `storage_options` 的构造次数
- 实现:`python/ops_bot/tests/test_panels.py::test_ops_114_history_creds_refresh`

| 行 | 动作 | 期望 |
|---|---|---|
| OPS-114.1 | T1 `/pnl live-a 2026-10-03` | 回复为 `2026-10-03` 的 PnL 分解,数据来自 Parquet;构造次数 1 |
| OPS-114.2 | 接 .1,凭证文件 mtime 不变,再查 `2026-10-02` | 构造次数仍为 1 |
| OPS-114.3 | 接 .2,凭证文件被改写(mtime + 45 min)后再查 | 查询前重建:构造次数 2 |

### OPS-115 模拟日报与校准差异

- 规则:[命令表](README.md#命令表)
- 级别:table
- 里程碑:M5
- 前置:`boot()`;archiver/watchtower 产出的 `sim-a` 日报与 `sim-a` vs `live-a` 校准差异夹具
- 实现:`python/ops_bot/tests/test_panels.py::test_ops_115_sim_report`

| 行 | 命令 | 期望 |
|---|---|---|
| OPS-115.1 | `/sim sim-a 2026-10-04` | 以 `<b>[🧪模拟·sim-a]</b>` 开头;含 PnL 分解、成交率、两腿间隔、裸敞口、保证金占用与 `report` 字段 `gate_cost_blocked` |
| OPS-115.2 | `/sim sim-a vs live-a` | 含两个账户标签与滑点差、成交率差、两腿间隔差 |
| OPS-115.3 | `/sim live-a` | 回复"不是模拟账户";没有日报 |

### OPS-116 /treasury 与 /pipeline 读其他冷进程的输出

- 规则:[命令表](README.md#命令表)
- 级别:table
- 里程碑:M5
- 前置:`boot()`;`<tmp>/treasury/latest.json` 的 `live-a` 项 `status = "ok"`、`headroom.up = {L1, L2, L3, approximate: false}`、`in_transit` 1 条、`suggestions` 1 条;`FakePipeline.status` 返回 `pc-1` 最近一轮
- 实现:`python/ops_bot/tests/test_panels.py::test_ops_116_treasury_pipeline_panels`

| 行 | 命令或条件 | 期望 |
|---|---|---|
| OPS-116.1 | `/treasury live-a` | 含 `ok`、上涨与下跌方向的 L1/L2/L3、在途 1 条、建议 1 条(from → to、金额) |
| OPS-116.2 | `latest.json` 中 `status = "stale"` | 回复含 `stale`,不列建议 |
| OPS-116.3 | `latest.json` 不存在 | 回复"treasury 无输出";不抛异常 |
| OPS-116.4 | `/pipeline pc-1` | 含最近一轮时间、下发结果、预热状态 |
| OPS-116.5 | `FakePipeline` 不监听 | 回复"pipeline 不可达" |

### OPS-117 /alerts 与 /audit 的来源

- 规则:[命令表](README.md#命令表)、[审计](README.md#审计)
- 级别:unit
- 里程碑:M5
- 前置:`boot()`;watchtower 状态 3 条告警;本地 `audit` 4 行;`FakeJournal[prod]` 2 条 `Note{CtlAudit}`;kill-switch 的 `actions.jsonl` 测试副本 1 条触发记录
- 步骤:
  1. T1 发 `/alerts 10`
  2. T1 发 `/audit 10`
- 期望:
  - 步骤 1:3 条告警
  - 步骤 2:按时间合并 4 + 2 + 1 = 7 条,每条标出来源(本地 / gateway / kill-switch)
- 实现:`python/ops_bot/tests/test_panels.py::test_ops_117_alerts_audit_sources`

### OPS-118 /start 只显示欢迎与角色

- 规则:[命令表](README.md#命令表)
- 级别:unit
- 里程碑:M5
- 前置:`boot()`
- 步骤:
  1. A1 发 `/start`
- 期望:
  - 回复只有欢迎语与 `Admin`;不含面板数据;不读视图(`FakeJournal` 读取次数不变)
- 实现:`python/ops_bot/tests/test_panels.py::test_ops_118_start`

### OPS-119 /mode 回执显示有效模式与 ShadowDeferred

- 规则:[命令表](README.md#命令表)
- 级别:table
- 里程碑:M5
- 前置:`boot()`;发送者 T1
- 实现:`python/ops_bot/tests/test_commands.py::test_ops_119_mode_receipt`

| 行 | 输入与结果 | 期望 |
|---|---|---|
| OPS-119.1 | `/mode pc-1 live-a shadow`,`Applied{ShadowDeferred}`,视图有效模式 CloseOnly | 回执含 `ShadowDeferred` 与`有效 CloseOnly` |
| OPS-119.2 | `/mode pc-1 live-a live` 确认后 `Applied`,有效模式仍 CloseOnly(cap `Recovery`) | 回执含`有效 CloseOnly`与生效 cap `Recovery` |
| OPS-119.3 | `/mode pc-1 16777217 live-a closeonly` | 请求 `group = "16777217"` |

### OPS-120 /kill 的参数组合

- 规则:[命令表](README.md#命令表)
- 级别:table
- 里程碑:M5
- 前置:`boot()`;发送者 T1(私聊);视图中 `live-a` 的未对冲量 g1 0.0005 BTC
- 实现:`python/ops_bot/tests/test_commands.py::test_ops_120_kill_combinations`

| 行 | 输入 | 期望 |
|---|---|---|
| OPS-120.1 | `/kill account live-a flatten` | 预览 `Kill{Account, live-a, flatten: true}`;确认后带期望版本 |
| OPS-120.2 | `/kill flatten` | 本地拒绝(`bad_args`),回复"flatten 不得与全局组合" |
| OPS-120.3 | `/kill all flatten` | 本地拒绝(`bad_args`) |
| OPS-120.4 | `/kill venue VP flatten` | 本地拒绝(`bad_args`) |
| OPS-120.5 | `/kill account live-a flatten hard` | 本地拒绝(`bad_args`) |
| OPS-120.6 | `/kill account live-a hard` | 立即发 `Kill{Account, live-a, hard: true}`;回执列出 `live-a` 的未对冲量 `16777217 0.0005` |
| OPS-120.7 | `/kill hard` | `prod`、`sim` 各立即发 `Kill{Global, hard: true}`;回执逐实例列出未对冲量 |
| OPS-120.8 | `/kill venue VP hard` | 本地拒绝(`bad_args`) |
| OPS-120.9 | `/kill account live-a` 时 `prod` 处于 Drain(gateway 回 `Applied`) | 立即发出,回执 `Applied` |
| OPS-120.10 | `/kill account live-a flatten` 时启动闸门未放行 | 照常预览并确认发出(不本地拦截) |

### OPS-121 /resume 只删完全匹配的一条 cap,前置条件不满足时给出下一步

- 规则:[命令表](README.md#命令表)
- 级别:table
- 里程碑:M5
- 前置:`boot()`;发送者 T1;每行预览后确认,`FakeCtl[prod]` 回下表结果
- 实现:`python/ops_bot/tests/test_commands.py::test_ops_121_resume`

| 行 | 输入与结果 | 期望 |
|---|---|---|
| OPS-121.1 | `/resume kill live-a`,`Applied` | 请求 `Resume{source: Kill, account: live-a}`;此后 60 s 内 `FakeCtl[prod]` 没有 `HedgeFix` 请求 |
| OPS-121.2 | `/resume recon live-a`,`PreconditionFailed{ReconNotClean}` | 回复含 `ReconNotClean` 与建议"先 /adopt" |
| OPS-121.3 | `/resume hedgefailure live-a`,`Rejected{BrokenRemaining}` | 回复含 `BrokenRemaining` |
| OPS-121.4 | `/resume kill live-a`,`PreconditionFailed{NoMatchingCap}` | 回复含 `NoMatchingCap` |
| OPS-121.5 | `/resume nosuch live-a` | 本地拒绝(`bad_args`),回复列出可用 source |
| OPS-121.6 | `/resume halted live-a`,`Applied` | 请求 `source = Halted` |

### OPS-122 /hedgefix 的本地检查与预览

- 规则:[命令表](README.md#命令表)
- 级别:table
- 里程碑:M5
- 前置:`boot()`;发送者 T1;g1 两腿:腿 0 `BTC/VS`(lot 0.00001)、腿 1 `BTC/VP`(lot 0.001);视图值按下表
- 实现:`python/ops_bot/tests/test_commands.py::test_ops_122_hedgefix_checks`

| 行 | 视图 | 期望 |
|---|---|---|
| OPS-122.1 | 腿 0 净 +0.012、腿 1 净 −0.010,无在途,最新对账一致 | 预览"多出腿 0(BTC/VS)0.002,平掉 0.002";确认后发 `HedgeFix` |
| OPS-122.2 | 腿 1 的仓位读取失败(视图缺该行) | 回复"读取失败,中止";`FakeCtl` 0 个请求 |
| OPS-122.3 | 腿 0 +0.0105、腿 1 −0.010:多出腿 0 的 0.0005 ≥ 腿 0 的 lot 0.00001(虽然 < 腿 1 的 lot 0.001) | 预览发出 |
| OPS-122.4 | 腿 0 +0.010009、腿 1 −0.010:多出 0.000009 < 腿 0 的 lot 0.00001 | 回复"无需修复";`FakeCtl` 0 个请求 |
| OPS-122.5 | 有在途意图(视图中 g1 有 `inflight_qty ≠ 0`) | 本地拒绝,回复"有在途意图" |
| OPS-122.6 | 腿 0 +0.01001、腿 1 −0.010:多出 0.00001 = 腿 0 的 lot | 预览发出 |

### OPS-123 /adopt、/baseline、/transfer 的预览内容

- 规则:[命令表](README.md#命令表)
- 级别:table
- 里程碑:M5
- 前置:`boot()`;发送者 T1;`FakeJournal[prod]` 最近一份 `ReconcileReport{report_id: 77}` 中 `live-a/VP BTC/VP` 引擎 −0.010、交易所 −0.012,另一条 `live-a/VS USDT` 余额引擎 1 000、交易所 1 050,一条资金费差异;`live-a/VS` 余额有非策略资产 BNB 2
- 实现:`python/ops_bot/tests/test_commands.py::test_ops_123_adopt_baseline_transfer_preview`

| 行 | 输入 | 期望 |
|---|---|---|
| OPS-123.1 | `/adopt live-a VP BTC/VP 77` | 预览列出 引擎 −0.010、交易所 −0.012、将写入的调整 −0.002 |
| OPS-123.2 | `/adopt live-a VS USDT 77` | 预览列出 1 000 / 1 050 / +50(余额条目) |
| OPS-123.3 | `/adopt live-a VP BTC/VP 77 61000` | 请求带 `entry_px = "61000"` |
| OPS-123.4 | `/adopt live-a VP funding 77` | 本地拒绝(`bad_args`),回复"资金费差异由账本自动校正" |
| OPS-123.5 | `/baseline live-a VS` | 预览列出将登记为基线的 `BNB 2` |
| OPS-123.6 | `/transfer live-a VS VP USDT 100 r1` | 预览列出 `VS` 与 `VP` 的当前 USDT 余额 |

### OPS-124 /notice 的三个子类

- 规则:[命令表](README.md#命令表)
- 级别:table
- 里程碑:M5
- 前置:`boot()`;发送者 T1;`FakeCtl[prod]` 回执 `Applied`,视图中 `BTC/VP` 被 g1 引用、`live-a` 持有 −0.010
- 实现:`python/ops_bot/tests/test_commands.py::test_ops_124_notice`

| 行 | 输入 | 期望 |
|---|---|---|
| OPS-124.1 | `/notice VP BTC/VP delist 2026-10-06T00:00:00Z` | 立即发 `VenueNotice{kind: Delisting, venue: VP, inst: BTC/VP, at_ns: "1791244800000000000", account: "*"}`;回执列出 `live-a`、`16777217`、持仓 −0.010 |
| OPS-124.2 | `/notice VP BTC/VP funding 14400` | 立即发 `VenueNotice{kind: FundingInterval, new_s: "14400", account: "*"}` |
| OPS-124.3 | `/notice VP BTC/VP funding 1800`,gateway 回 `InvalidArgs{BadValue}` | 回复含 `BadValue` |
| OPS-124.4 | `/notice VP * maint 2026-10-06T01:00:00Z 2026-10-06T00:00:00Z`,gateway 回 `InvalidArgs{BadValue}` | 回复含 `BadValue` |
| OPS-124.5 | `/notice VP BTC/VP delist tomorrow` | 本地拒绝(`bad_args`),回复"时间须为 UTC ISO 格式" |

### OPS-125 /smoke 的预览、确认与订单号回显

- 规则:[命令表](README.md#命令表)
- 级别:unit
- 里程碑:M5
- 前置:`boot()`;发送者 A1(私聊);`FakeCtl[prod]` 的预览 `body` 含 `px = "57000.0"`、`qty = "0.001"`、`ttl_ms = "10000"`;确认后 `CmdResult{Applied}` 的 `seq = 9001`;`FakeJournal[prod]` 随后投递 `OrderDecision{trigger_seq: 9001, client_order_id: "c-77"}`
- 步骤:
  1. A1 发 `/smoke live-a VP BTC/VP buy 10000`
  2. A1 点确认
- 期望:
  - 步骤 1:预览含 `<b>[💰实盘·live-a]</b>`、`BTC/VP`、`buy`、价格 `57000.0`、数量 `0.001`、`ttl_ms 10000`
  - 步骤 2:确认请求 `SmokeOrder{… ttl_ms: "10000"}`、`confirmed: true`;`Applied` 后回复含订单号 `c-77`
- 实现:`python/ops_bot/tests/test_commands.py::test_ops_125_smoke_flow`

### OPS-126 /writeoff 预览多出的一腿与将清零的残余

- 规则:[命令表](README.md#命令表)
- 级别:unit
- 里程碑:M5
- 前置:`boot()`;发送者 A1(私聊);视图中 g1 腿 0 `BTC/VS` 已成交净 +0.0105、腿 1 `BTC/VP` −0.010,残余 腿 0 0.0005、腿 1 0;g1 静止(无在途、无 Pending/Unknown)
- 步骤:
  1. A1 发 `/writeoff live-a 16777217`
- 期望:
  - 预览含"多出的一腿:VS BTC/VS leg_ix 0,0.0005 移到未归属持仓"与"各腿残余清零:腿 0 0.0005 → 0,腿 1 0 → 0";`FakeCtl[prod]` 收到 `preview` 的 `WriteOffResidual{account: live-a, group: "16777217"}`
- 实现:`python/ops_bot/tests/test_commands.py::test_ops_126_writeoff_preview`

### OPS-127 /warmup 经 pipeline 本地接口执行并回显进度

- 规则:[命令表](README.md#命令表)、[审计](README.md#审计)
- 级别:table
- 里程碑:M5
- 前置:`boot()`;发送者 T1
- 实现:`python/ops_bot/tests/test_commands.py::test_ops_127_warmup`

| 行 | `FakePipeline.run_warmup` | 期望 |
|---|---|---|
| OPS-127.1 | 回 `{chunks: 600, last: true, result: "Applied"}` | 回复含 `600`、`last`、`Applied`;`FakeCtl[prod]` 收到 1 个 `Annotate{LocalWrite}`;没有 `InjectWarmup` 请求 |
| OPS-127.2 | 回 `{result: "failed", reason: "no sample"}` | 回复含 `failed` 与原因 |
| OPS-127.3 | 不监听 | 回复"pipeline 不可达";没有 `Annotate` |

### OPS-128 /note、/ack、/sub 的本地效果

- 规则:[命令表](README.md#命令表)、[推送](README.md#推送)
- 级别:table
- 里程碑:M5
- 前置:`boot()`
- 实现:`python/ops_bot/tests/test_commands.py::test_ops_128_note_ack_sub`

| 行 | 输入 | 期望 |
|---|---|---|
| OPS-128.1 | V1 私聊 `/note 老板看到 VP 价格异常` | 发 `Annotate{kind: OpsNote, text: "老板看到 VP 价格异常"}`,`issuer = "tg:1001"`;无状态变化 |
| OPS-128.2 | T1 私聊 `/sub live-b off` | `state` 表 T1 订阅 = `[live-a]`;`FakeCtl[prod]` 收到 1 个 `Annotate{LocalWrite}` |
| OPS-128.3 | T1 私聊 `/sub sim-a on` | 订阅 = `[live-a, live-b, sim-a]` |
| OPS-128.4 | T1 私聊 `/sub nosuch on` | 本地拒绝(`bad_args`) |
| OPS-128.5 | T1 在群 −1001 `/ack w-17` | `acks.jsonl` 追加一行;回复在群里 |

### OPS-129 Telegram 发不出原始 SetParams、SetRisk、ReplaceRefdata、InjectWarmup

- 规则:[命令表](README.md#命令表)
- 级别:prop
- 里程碑:M5
- 性质:任意角色在任意聊天类型发任意文本,`FakeCtl` 收到的请求的 `cmd.type` 都不属于 {`SetParams`, `SetRisk`, `ReplaceRefdata`, `InjectWarmup`}
- 生成:发送者 ∈ {V1, T1, A1};`chat.type` ∈ {private, group};文本 = OPS-013 的全部命令模板、以 `/` 开头的随机命令名(含 `/setparams`、`/setrisk`、`/replacerefdata`、`/injectwarmup`)与随机参数
- 参考:断言集合为空
- 例数:PR 2 000
- 实现:`python/ops_bot/tests/test_commands.py::test_ops_129_no_raw_privileged_cmds`

### OPS-130 未登记的命令名回复帮助

- 规则:[命令表](README.md#命令表)
- 级别:unit
- 里程碑:M5
- 前置:`boot()`
- 步骤:
  1. T1 私聊发 `/setrisk live-a x=1`
- 期望:
  - 回复"未知命令"并附 `/help` 提示;`audit` 一行 `denied: bad_args`;`FakeCtl` 0 个请求
- 实现:`python/ops_bot/tests/test_commands.py::test_ops_130_unknown_command`

### OPS-140 schema 从发布目录载入,缺失或与运行时版本不符时用 GetSchema 并按版本缓存

- 规则:[参数面板与修改](README.md#参数面板与修改)、[实现说明](README.md#实现说明)
- 级别:table
- 里程碑:M5
- 前置:`boot()`;`schemas_t()` 中 `probe-target` 的 `(schema_id, schema_ver) = (65281, 2)`;`FakeCtl[prod]` 对 `GetSchema{strategy_kind: "probe-target"}` 回 `body` = 一份 `schema_ver = 3` 的 schema JSON;视图中 `pc-1` 当前生效的 `StrategyParams` 头为下表版本
- 实现:`python/ops_bot/tests/test_schema.py::test_ops_140_schema_source`

| 行 | 条件 | 期望 |
|---|---|---|
| OPS-140.1 | 生效 blob 的 `schema_ver = 2` | `/schema probe-target` 用目录中的 schema;`FakeCtl` 0 个 `GetSchema` |
| OPS-140.2 | 生效 blob 的 `schema_ver = 3` | 发 1 个 `GetSchema`;回复字段表来自运行时 schema |
| OPS-140.3 | 接 .2,再发 `/params pc-1` 与 `/schema probe-target` | 仍只有 1 个 `GetSchema`(按 `(65281, 3)` 缓存) |
| OPS-140.4 | `<tmp>/schemas/` 中没有 `probe-target` 的文件 | 发 1 个 `GetSchema` |
| OPS-140.5 | `index.json` 与文件不一致(`hft_py.params.load_schemas` 抛 `ParamsError`) | 照常启动并记 P1;`/schema probe-target` 发 1 个 `GetSchema`(详见 OPS-225) |

### OPS-141 /schema 列出每个字段的名称、单位、范围、默认值、是否按 group 与说明

- 规则:[参数面板与修改](README.md#参数面板与修改)
- 级别:golden
- 里程碑:M5
- 输入:`schemas_t()` 的 `probe-taker` 与 `probe-target`,分别发 `/schema probe-taker`、`/schema probe-target`
- 期望文件:`python/ops_bot/tests/golden/schema_render/<kind_id>.txt`(人工按 schema JSON 核对)
- 实现:`python/ops_bot/tests/test_schema.py::test_ops_141_schema_render_golden`

### OPS-142 /params 按单位渲染当前生效参数

- 规则:[参数面板与修改](README.md#参数面板与修改)
- 级别:golden
- 里程碑:M5
- 输入:`fixtures/params/` 中两个测试策略的每个 blob 作为 `pc-1`(或 `pc-2`)当前生效的 `ConfigBlob`;分别发 `/params pc-1`、`/params pc-1 16777217`
- 期望文件:`python/ops_bot/tests/golden/params_render/<case>.txt`:每个字段的值与 `fixtures/params/<case>` 的解码金样一致;`Rate` 显示为 bp 或 %、`Notional` 为 USDT、`DurationMs` 为时长、`Enum` 为名称、`Bool` 为 `true`/`false`
- 实现:`python/ops_bot/tests/test_schema.py::test_ops_142_params_render_golden`

### OPS-143 /params 标出非默认值、覆盖来源、open_allowed 与版本

- 规则:[参数面板与修改](README.md#参数面板与修改)
- 级别:unit
- 里程碑:M5
- 前置:`boot()`;`pc-1` 生效 blob:版本 42、`ParamSource = Pipeline`、`generated_at` = `t0 − 5 min`;默认值中 `cooldown_ms = 30 000`(schema 默认 20 000);g1 的 `open_th` 覆盖 = 60 bp、`open_allowed = false`;`FakePipeline.status` 报告 `cooldown_ms` 的覆盖项来自 `pair:p1`、`open_th` 的覆盖项来自 `instance:pc-1`
- 步骤:
  1. T1 发 `/params pc-1`
- 期望:
  - `cooldown_ms` 行标"≠ 默认"并注明来源"校准对 p1";`open_th` 的 g1 行显示 `60bp` 并注明来源"本实例";g1 显示 `open_allowed = false`
  - 页脚含`版本 42`、`Pipeline`、生成时间 `t0 − 5 min`
- 实现:`python/ops_bot/tests/test_schema.py::test_ops_143_params_annotations`

### OPS-144 /params pair 列出共享覆盖项与两个实例的版本

- 规则:[参数面板与修改](README.md#参数面板与修改)
- 级别:unit
- 里程碑:M5
- 前置:`boot()`;`pairs_t()`;`FakePipeline.status` 报告 `pair-p1.toml` 一项 `cooldown_ms = 30000`,`pc-1` 版本 42、`sim-1` 版本 17
- 步骤:
  1. T1 发 `/params pair p1`
- 期望:
  - 回复含 `cooldown_ms 30s`、`pc-1 v42`、`sim-1 v17`
- 实现:`python/ops_bot/tests/test_schema.py::test_ops_144_params_pair_panel`

### OPS-145 参数修改的本地校验按 schema 拒绝非法输入

- 规则:[参数面板与修改](README.md#参数面板与修改)
- 级别:table
- 里程碑:M5
- 前置:`boot()`;发送者 T1;`probe-target` 字段:`open_th` Rate [0, 10 %] per_group;`cooldown_ms` DurationMs [0, 600 000];`cap` Notional [0, 10^12 raw];`mode` Enum `["a", "b"]`;`force_exit` Bool per_group;每行通过时期望 `FakePipeline` 收到 `preview_override(…, value = <列值>)`,拒绝时本地拒绝(`bad_args`)且 `FakePipeline` 0 个请求;单位换算与范围检查经 `hft_py.params.parse_value`(PY-134),测试以真实 hft-py wheel 运行
- 实现:`python/ops_bot/tests/test_params_edit.py::test_ops_145_local_validation`

| 行 | 输入 | 期望 |
|---|---|---|
| OPS-145.1 | `/params pc-1 set open_th 16777217 50bp` | `value = 5000000`,`group = 16777217` |
| OPS-145.2 | `/params pc-1 set open_th 16777217 0.5%` | `value = 5000000` |
| OPS-145.3 | `/params pc-1 set open_th 16777217 10%` | `value = 100000000`(上界) |
| OPS-145.4 | `/params pc-1 set open_th 16777217 10.0000001%` | 拒绝,回复含"超出范围" |
| OPS-145.5 | `/params pc-1 set open_th 16777217 -1bp` | 拒绝,"超出范围" |
| OPS-145.6 | `/params pc-1 set cooldown_ms 5m` | `value = 300000` |
| OPS-145.7 | `/params pc-1 set cooldown_ms 600001` | 拒绝,"超出范围" |
| OPS-145.8 | `/params pc-1 set cooldown_ms 600000` | `value = 600000` |
| OPS-145.9 | `/params pc-1 set cooldown_ms 16777217 5s` | 拒绝,"该字段不允许按 group 覆盖" |
| OPS-145.10 | `/params pc-1 set cap 1500` | `value = 1500000000` |
| OPS-145.11 | `/params pc-1 set mode b` | `value = 1` |
| OPS-145.12 | `/params pc-1 set mode c` | 拒绝,回复列出 `a`、`b` |
| OPS-145.13 | `/params pc-1 set nosuch 1` | 拒绝,"未知字段" |
| OPS-145.14 | `/params pc-1 set open_th 16777217 abc` | 拒绝,"无法解析" |
| OPS-145.15 | `/params pc-1 set open_th 16777217 0.0000000001` | 拒绝,"无法解析"(小数位超过 1e-9) |
| OPS-145.16 | `/params pc-1 set force_exit 16777217 true` | `value = 1`,`group = 16777217` |
| OPS-145.17 | `/params pc-9 set cooldown_ms 5s`(无此实例) | 拒绝,"未知实例" |

### OPS-146 按实例的参数修改只写该实例,作用于 Live 实例时预览首行标"实盘"

- 规则:[参数面板与修改](README.md#参数面板与修改)
- 级别:table
- 里程碑:M5
- 前置:`boot()`;`pairs_t()`;发送者 T1;`FakePipeline.preview_override` 按 `target` 回 `instances` 与 `live`(下表)
- 实现:`python/ops_bot/tests/test_params_edit.py::test_ops_146_instance_scoped_edit`

| 行 | 输入 | 期望 |
|---|---|---|
| OPS-146.1 | `/params sim-1 set cooldown_ms 30s`(`instances = ["sim-1"]`,`live = false`) | `preview_override(target = "instance:sim-1")`;预览首行不含"实盘";确认后 `apply_override(target = "instance:sim-1", …)`;没有任何以 `instance:pc-1` 或 `pair:p1` 为 target 的请求 |
| OPS-146.2 | `/params pc-1 set cooldown_ms 30s`(`instances = ["pc-1"]`,`live = true`) | 预览首行含"实盘";未确认前没有 `apply_override` |
| OPS-146.3 | 接 .2,确认后 `FakeJournal[prod]` 投递 pipeline 发出的 `SetParams` 的 `CmdResult{Applied}` | 原消息追加回显 `Applied` |
| OPS-146.4 | `/params pc-1 set open_th 16777217 40bp`,预览带 `relax = true` | 预览含"放宽" |

### OPS-147 校准对的共享修改列出两个实例且总是需要确认

- 规则:[参数面板与修改](README.md#参数面板与修改)
- 级别:unit
- 里程碑:M5
- 前置:`boot()`;`pairs_t()`;`FakePipeline.preview_override(target = "pair:p1")` 回两个实例的字段差异与 `live = true`
- 步骤:
  1. T1 发 `/params pair p1 set cooldown_ms 30s`
  2. T1 点确认
- 期望:
  - 步骤 1:预览分两节列出 `pc-1` 与 `sim-1` 的差异;带确认按钮;没有 `apply_override`
  - 步骤 2:`apply_override(target = "pair:p1", …, expect_override_version = <预览值>)`
- 实现:`python/ops_bot/tests/test_params_edit.py::test_ops_147_pair_edit`

### OPS-148 结构字段作用于 Live 实例时预览即被拒,不给确认按钮

- 规则:[参数面板与修改](README.md#参数面板与修改)
- 级别:unit
- 里程碑:M5
- 前置:`boot()`;`FakePipeline.preview_override` 对 `pct_bin`(structural)在 `instance:pc-1` 上回 `{rejected: "结构字段须经 hft-cfg + 运维 CLI"}`
- 步骤:
  1. T1 发 `/params pc-1 set pct_bin 1bp`
- 期望:
  - 回复含"结构字段须经 hft-cfg + 运维 CLI";消息不带确认按钮;`pending_confirm` 无新行;没有 `apply_override`
- 实现:`python/ops_bot/tests/test_params_edit.py::test_ops_148_structural_live_rejected`

### OPS-149 pipeline 本地接口不可达时参数修改明确失败

- 规则:[参数面板与修改](README.md#参数面板与修改)
- 级别:unit
- 里程碑:M5
- 前置:`boot()`;`FakePipeline` 不监听
- 步骤:
  1. T1 发 `/params pc-1 set cooldown_ms 30s`
- 期望:
  - 回复"pipeline 不可达";没有确认按钮;没有 `Annotate{LocalWrite}`
- 实现:`python/ops_bot/tests/test_params_edit.py::test_ops_149_pipeline_unreachable`

### OPS-150 策略 Note 按描述符渲染,未登记的码显示原始值并记 P2

- 规则:[策略命令与描述符](README.md#策略命令与描述符)
- 级别:table
- 里程碑:M5
- 前置:`boot()`;`FakeJournal[prod]` 中 `pc-1` 的 Note:`{code: 1, level: P1, group: 16777217, v: [12500000, 0…]}` 与 `{code: 99, level: P2, v: [7, 8, 0…]}`;日志捕获器
- 实现:`python/ops_bot/tests/test_descriptors.py::test_ops_150_notes_render`

| 行 | 输入 | 期望 |
|---|---|---|
| OPS-150.1 | `/notes live-a 5` 中 code 1 | 显示为 `CashShort`、`P1`、`16777217`、`缺口 12.5 USDT`(按描述符 `v[0]` 的 Notional 单位) |
| OPS-150.2 | 同上中 code 99 | 显示为 `code 99`、`v = [7, 8, 0, 0, 0, 0, 0, 0]`;日志一行级别 P2,含 `unregistered note code 99` |

### OPS-151 /cmd 按描述符解析字段并经 hft_py 编码载荷

- 规则:[策略命令与描述符](README.md#策略命令与描述符)
- 级别:table
- 里程碑:M5
- 前置:`boot()`;发送者 T1;`hft_py.params.encode_cmd` 被包装为记录调用的探针(返回真实结果);`probe-target` 的 `cmds`:`close`(`op = 1`,`reduce_only = true`,`group_frac` Rate [0, 1])、`open`(`op = 2`,`reduce_only = false`,`notional` Notional [0, 10 000])
- 实现:`python/ops_bot/tests/test_descriptors.py::test_ops_151_cmd_encode`

| 行 | 输入 | 期望 |
|---|---|---|
| OPS-151.1 | `/cmd pc-1 live-a 16777217 close group_frac=0.5` | `encode_cmd` 被调用 1 次(命令 `close`,值 `[500000000]`);请求 `payload_b64` 解码后 = `01 01 01 00 00 00 00 00` + `500000000` 的 i64 小端,共 16 B;`reduce_only: true`;立即发出 |
| OPS-151.2 | `/cmd pc-1 live-a none open notional=100` | `encode_cmd` 值 `[100000000]`;`reduce_only: false`;先预览 |
| OPS-151.3 | `/cmd pc-1 live-a 16777217 close group_frac=1.5` | 本地拒绝(`bad_args`),"超出范围";`encode_cmd` 0 次 |
| OPS-151.4 | `/cmd pc-1 live-a 16777217 close frac=0.5` | 本地拒绝,"未知字段" |
| OPS-151.5 | `/cmd pc-1 live-a 16777217 shutdown` | 本地拒绝,"未知命令名" |
| OPS-151.6 | `/cmd pc-2 live-b none open notional=1`(`probe-taker` 的 `cmds` 为空) | 回复"该策略不接受人工命令";`FakeCtl` 0 个请求 |
| OPS-151.7 | `/cmd pc-1 live-b 16777217 close group_frac=0.5`(账户未绑定该实例) | 本地拒绝(`bad_args`),"账户未绑定该实例" |

### OPS-152 reduce_only 的策略命令在 Drain 与 CloseOnly 期间照常可发

- 规则:[策略命令与描述符](README.md#策略命令与描述符)
- 级别:integ
- 里程碑:M5
- 前置:进程内 hft-ctl gateway + sequencer(hft-testkit 假交易所),`pc-1` = `probe-target`;`pc-1` 处于 Drain;ops_bot 的 `FakeCtl[prod]` 换成真实 ctl socket
- 步骤:
  1. T1 发 `/cmd pc-1 live-a 16777217 close group_frac=0.5`
  2. T1 发 `/cmd pc-1 live-a none open notional=100` 并确认
- 期望:
  - 步骤 1:没有预览;`CmdResult{Applied}`;journal 中该 `ControlCmd` 的 `reduce_only = 1`
  - 步骤 2:`CmdResult{PreconditionFailed, detail = Draining}`
- 实现:`python/ops_bot/tests/test_descriptors.py::test_ops_152_reduce_only_during_drain`

### OPS-153 单 group 强制退出参数经通用参数修改生效

- 规则:[策略命令与描述符](README.md#策略命令与描述符)
- 级别:unit
- 里程碑:M5
- 前置:`boot()`;`probe-target` 的 `force_exit`(Bool,per_group,`manual_only`)
- 步骤:
  1. T1 发 `/params pc-1 set force_exit 16777217 true` 并确认
- 期望:
  - `apply_override(target = "instance:pc-1", field = "force_exit", group = 16777217, value = 1, …)`;本包没有任何名为 `force_exit` 的代码分支(源码中不出现该字面量)
- 实现:`python/ops_bot/tests/test_descriptors.py::test_ops_153_force_exit_via_params`

### OPS-154 两个测试策略的 schema 下全部 schema 驱动命令可用,新增策略本包 diff 为零

- 规则:[策略命令与描述符](README.md#策略命令与描述符)、[参数面板与修改](README.md#参数面板与修改)、[概要](README.md#概要)
- 级别:integ
- 里程碑:M5
- 前置:`boot()`;`schemas_t()` 先只含 `probe-taker`,再加入 `probe-target` 的 schema 文件并重启(不改本包任何文件)
- 步骤:
  1. 只含 `probe-taker` 时依次发 `/schema probe-taker`、`/params pc-2`、`/notes live-b`
  2. 加入 `probe-target` 后依次发 `/schema probe-target`、`/params pc-1`、`/params pc-1 set cooldown_ms 30s`、`/notes live-a`、`/cmd pc-1 live-a 16777217 close group_frac=0.5`
- 期望:
  - 每条命令都得到非错误回复;步骤 2 前后 `python/ops_bot/src/` 的文件哈希不变
- 实现:`python/ops_bot/tests/test_descriptors.py::test_ops_154_generic_two_strategies`

### OPS-155 本包源码与 README 不含策略族名

- 规则:[概要](README.md#概要)、[策略命令与描述符](README.md#策略命令与描述符)
- 级别:table
- 里程碑:M5
- 前置:`cargo xtask doc-check` 的名字越界检查,扫描对象为 `python/ops_bot/`;每行在临时副本里改一处
- 实现:`python/ops_bot/tests/test_static.py::test_ops_155_no_strategy_names`

| 行 | 临时改动 | 期望 |
|---|---|---|
| OPS-155.1 | 不改 | `doc-check` 退出 0 |
| OPS-155.2 | `src/hft_ops_bot/render.py` 加一行注释,内容为 [registry kind_id 登记表](../../strategies/registry/README.md#使用本组件) 中任一策略 crate 名 | 退出非 0,报告该文件与行号 |
| OPS-155.3 | README 加同一个策略 crate 名 | 退出非 0 |
| OPS-155.4 | `src/hft_ops_bot/commands/params.py` 加一个以该策略名(下划线形式)作字符串比较的 `if` 分支 | 退出非 0 |

### OPS-160 策略插件经 entry point 发现,只载入配置中启用的

- 规则:[策略插件(optional)](README.md#策略插件optional)、[实现说明](README.md#实现说明)
- 级别:table
- 里程碑:M5
- 前置:测试 venv 中装有两个测试插件包,entry point 组 `hft_ops_bot.plugins`:`t-plugin`(注册写命令 `/tplug`,映射为 `StrategyCmd`)与 `t-other`
- 实现:`python/ops_bot/tests/test_plugins.py::test_ops_160_plugin_discovery`

| 行 | `plugins` | 期望 |
|---|---|---|
| OPS-160.1 | `[]` | `/tplug` 回复"未知命令" |
| OPS-160.2 | `["t-plugin"]` | `/tplug` 可用;`t-other` 未载入(其注册函数调用次数 0) |
| OPS-160.3 | `["t-plugin", "t-other"]` | 两者都载入 |

### OPS-161 插件命令经同一鉴权、确认与审计路径

- 规则:[策略插件(optional)](README.md#策略插件optional)
- 级别:table
- 里程碑:M5
- 前置:`cfg_t()` 且 `plugins = ["t-plugin"]`;`/tplug pc-1 live-a` 声明为升级类(需确认)
- 实现:`python/ops_bot/tests/test_plugins.py::test_ops_161_plugin_same_path`

| 行 | 输入 | 期望 |
|---|---|---|
| OPS-161.1 | V1 私聊 `/tplug pc-1 live-a` | 本地拒绝(`viewer`) |
| OPS-161.2 | T1 在群 −1001 `/tplug pc-1 live-a` | 回复"请私聊执行"(`denied: private_only`) |
| OPS-161.3 | T1 私聊 `/tplug pc-1 live-a` | 先 `preview`,确认后发出;请求 `issuer = "tg:2001"`;`audit` 有该行 |

### OPS-170 成交与目标进度按账户合并,每账户每分钟至多一条摘要

- 规则:[推送](README.md#推送)
- 级别:table
- 里程碑:M5
- 前置:`boot()`;T1 订阅 `live-a`、`live-b`;`FakeJournal[prod]` 在下表时刻投递 `live-a` 的成交(`FillDelta`,腿 0 买 0.001 @ 60 000,手续费 0.06 USDT)
- 实现:`python/ops_bot/tests/test_push.py::test_ops_170_fill_summary_rate`

| 行 | 成交时刻 | 期望(T1 私聊) |
|---|---|---|
| OPS-170.1 | `t0`、`t0 + 10 s`、`t0 + 59.999 s` | `t0` 一条摘要;另 2 笔并入下一条,`t0 + 60 s` 之前没有第二条 |
| OPS-170.2 | 接 .1 | 第二条摘要在 `t0 + 60 s` 发出,含 2 笔成交 |
| OPS-170.3 | `live-a` 与 `live-b` 同在 `t0` 各一笔 | 两条摘要(按账户分开),各带自己的账户标签 |
| OPS-170.4 | 摘要内容 | 含各腿成交量与价格、手续费(按扣费资产 `USDT`)、两腿间隔、目标剩余 |

### OPS-171 执行层进度事件与自动平仓结果推送

- 规则:[推送](README.md#推送)
- 级别:table
- 里程碑:M5
- 前置:`boot()`;T1 订阅 `live-a`
- 实现:`python/ops_bot/tests/test_push.py::test_ops_171_exec_progress_push`

| 行 | journal 事件 | 期望(T1 私聊,并入每分钟摘要) |
|---|---|---|
| OPS-171.1 | `ExecDecision{State, state_to = Idle}`(目标完成) | 摘要含 `16777217 完成` |
| OPS-171.2 | `ExecDecision{State, state_to = Stuck, detail = Stuck/NoLiquidity}` | 摘要含 `Stuck NoLiquidity` |
| OPS-171.3 | `ExecDecision{State, state_to = Broken}` | 摘要含 `Broken` |
| OPS-171.4 | `Note{ExecAutoFixDone}` | 摘要含 `ExecAutoFixDone` |
| OPS-171.5 | `Note{ExecAutoFixFailed}` | 摘要含 `ExecAutoFixFailed` |

### OPS-172 保证金档位变化与外部成交推送只作账户视图补充

- 规则:[推送](README.md#推送)
- 级别:table
- 里程碑:M5
- 前置:`boot()`;T1 订阅 `live-a`;`live-a` 的 `VP` 永续空 0.010、`liq_dist` 由 0.25 变为 0.12
- 实现:`python/ops_bot/tests/test_push.py::test_ops_172_margin_external_push`

| 行 | journal 事件 | 期望 |
|---|---|---|
| OPS-172.1 | `Note{CapSet}` `MarginStress` level 1 → 2 | T1 收到推送:档位 `L2`、`liq_dist 12.0%` 与到各档阈值的距离;Bark 0 条(P0 由 watchtower 投递) |
| OPS-172.2 | `ExternalFill{cause: Adl}` 在 `BTC/VP`,另一腿 `BTC/VS` 有效模式 Live | T1 收到推送:原因 `ADL`、另一腿状态 |
| OPS-172.3 | 同 .1,但 T1 已 `/sub live-a off` | T1 没有该推送 |

### OPS-173 策略 Note 按 push.note_min_level 推送

- 规则:[推送](README.md#推送)、[配置文件字段](README.md#配置文件字段)
- 级别:table
- 里程碑:M5
- 前置:`boot()`;T1 订阅 `live-a`;`pc-1` 依次产生 `NoteLevel` 为 P0、P1、P2、Info 的策略 Note 各一条(测试直接写 journal,P0 只为本用例构造)
- 实现:`python/ops_bot/tests/test_push.py::test_ops_173_note_min_level`

| 行 | `push.note_min_level` | 期望 T1 收到的 Note 级别 |
|---|---|---|
| OPS-173.1 | `P2` | P0、P1、P2 |
| OPS-173.2 | `P1` | P0、P1 |
| OPS-173.3 | `P0` | P0 |
| OPS-173.4 | `Info` | P0、P1、P2、Info |

### OPS-174 定时报告与日报的时刻

- 规则:[推送](README.md#推送)、[配置文件字段](README.md#配置文件字段)
- 级别:table
- 里程碑:M5
- 前置:`boot()` 于 `2026-10-05T15:00:00Z`;`push.summary_every_min = 20`;`push.daily_at_local = "00:00"`;`push.tz = "Asia/Shanghai"`;T1 订阅 `live-a`、`live-b`
- 实现:`python/ops_bot/tests/test_push.py::test_ops_174_schedules`

| 行 | 时刻 | 期望 |
|---|---|---|
| OPS-174.1 | 启动后 19 min 59 s | 尚无定时报告 |
| OPS-174.2 | 启动后 20 min | 一条定时报告:`live-a`、`live-b` 的权益、敞口、有效模式 |
| OPS-174.3 | `2026-10-05T15:59:59Z` | 无日报 |
| OPS-174.4 | `2026-10-05T16:00:00Z`(上海 00:00) | 一条日报 |
| OPS-174.5 | `push.tz = "UTC"`,`2026-10-06T00:00:00Z` | 一条日报;`2026-10-05T16:00:00Z` 无日报 |

### OPS-175 日报内容

- 规则:[推送](README.md#推送)
- 级别:unit
- 里程碑:M5
- 前置:`boot()`;watchtower 日报(告警 5 条、dev2 护栏触发 1 次);`live-a` PnL 分解;`pc-1` 的 `report` 字段 `gate_cost_blocked = 3`;`sim-a` 日报与 `sim-a` vs `live-a` 校准差异;treasury `latest.json` 中 `live-a` 的 `headroom.day_min_l2 = 0.153`
- 步骤:
  1. 推进到日报时刻
- 期望:
  - 日报依次含:告警统计 `5`、护栏触发 `1`;`live-a` PnL 分解;`gate_cost_blocked 3`(按 `report` 描述符的名与单位);`sim-a` 日报;校准差异;treasury 摘要 `live-a 15.3%`
  - 实盘与模拟的金额分在"实盘合计"与"模拟合计"两行
- 实现:`python/ops_bot/tests/test_push.py::test_ops_175_daily_contents`

### OPS-176 treasury 划转建议按 notify 标志与订阅推送

- 规则:[推送](README.md#推送)
- 级别:table
- 里程碑:M5
- 前置:`boot()`;T1 订阅 `live-a`、`live-b`;T2 订阅 `live-a`;`<tmp>/treasury/latest.json` 按下表写出(每次写出带新的 `round_ts`)
- 实现:`python/ops_bot/tests/test_push.py::test_ops_176_treasury_push`

| 行 | `latest.json` | 期望 |
|---|---|---|
| OPS-176.1 | `round_ts = R1`;`live-a` 一条建议 `notify = true`、`urgency = normal` | T1、T2 各一条推送,含 from → to、资产、金额、成本、时延、紧急度、原因,带 `<b>[💰实盘·live-a]</b>` |
| OPS-176.2 | 同一 `round_ts = R1` 的文件被再次写出(内容相同) | 不再推送 |
| OPS-176.3 | `round_ts = R2`;同一建议 `notify = false` | 不推送 |
| OPS-176.4 | `round_ts = R3`;`live-b` 两条建议,一条 `urgency = urgent`、一条 `normal`,都 `notify = true` | T1 收到 2 条(紧急的单独一条,不与另一条合并);T2 收到 0 条(未订阅 `live-b`) |
| OPS-176.5 | `round_ts = R4`;`sim-a` 一条 `notify = true` | T1 收到 0 条(默认未订阅模拟账户) |

### OPS-177 订阅默认只含实盘账户,/sub 调整

- 规则:[推送](README.md#推送)
- 级别:table
- 里程碑:M5
- 前置:`boot()`;各账户在 `t0` 各有一笔成交
- 实现:`python/ops_bot/tests/test_push.py::test_ops_177_subscription`

| 行 | 用户 | 期望收到成交摘要的账户 |
|---|---|---|
| OPS-177.1 | V2(未写 `push_accounts`) | `live-a`、`live-b` |
| OPS-177.2 | V1(`push_accounts = ["live-a"]`) | `live-a` |
| OPS-177.3 | T1 先 `/sub sim-a on` | `live-a`、`live-b`、`sim-a` |
| OPS-177.4 | T1 先 `/sub live-a off` | `live-b` |

### OPS-178 其他 P0/P1 告警不由 ops_bot 投递

- 规则:[推送](README.md#推送)
- 级别:unit
- 里程碑:M5
- 前置:`boot()`;watchtower 状态目录新增一条 P0 告警 `NakedExposure`(id `w-90`)
- 步骤:
  1. 推进 60 s
  2. T1 发 `/alerts 1`
- 期望:
  - 步骤 1:`FakeTg` 没有发出含 `NakedExposure` 的消息;`FakeBark` 0 条
  - 步骤 2:回复含 `w-90` 与 `NakedExposure`
- 实现:`python/ops_bot/tests/test_push.py::test_ops_178_other_alerts_not_delivered`

### OPS-179 ops_bot 自己投递只有它知道的 P1

- 规则:[推送](README.md#推送)
- 级别:table
- 里程碑:M5
- 前置:`boot()`
- 实现:`python/ops_bot/tests/test_push.py::test_ops_179_own_p1`

| 行 | 情形 | 期望 |
|---|---|---|
| OPS-179.1 | OPS-007 的同名账户 | T1、T2、A1 私聊各一条 P1;群 −1001 与 Bark 0 条 |
| OPS-179.2 | OPS-078.1 的"模拟实例不健康" | 同上 |
| OPS-179.3 | OPS-093.1 的"待补 Kill 已生效" | T1、T2、A1 私聊各一条 P1 |

### OPS-180 每条命令一行审计,只读命令也记

- 规则:[审计](README.md#审计)
- 级别:table
- 里程碑:M5
- 前置:`boot()`
- 实现:`python/ops_bot/tests/test_audit.py::test_ops_180_audit_rows`

| 行 | 输入 | 期望 `audit` 新增一行的列 |
|---|---|---|
| OPS-180.1 | T1 私聊 `/p live-a` | `from_id = 2001`、`chat_id = 2001`、规范形 `/p live-a`、角色 `Trader`、`result = ok`、`ctl_state_version` 空 |
| OPS-180.2 | T1 `/mode pc-1 live-a live` 并确认,`Applied` | 预览一行(`ctl_state_version = 41`)、确认一行(`result = Applied`) |
| OPS-180.3 | V1 `/kill` | `result = denied: viewer` |
| OPS-180.4 | V1 `/stop live-a` 兜底 `stop_core` | 路径 `fallback_stop_core`、`req_id` = 触发请求的 `req_id` |
| OPS-180.5 | T2 点 T1 预览的确认 | `result = denied: not_initiator` |

### OPS-181 SQLite 以 WAL 与 synchronous=FULL 打开,待补 Kill 不在其中

- 规则:[审计](README.md#审计)
- 级别:unit
- 里程碑:M5
- 前置:`boot()`
- 步骤:
  1. 打开 `<tmp>/state/ops.sqlite3`
- 期望:
  - `PRAGMA journal_mode` = `wal`;ops_bot 连接上 `PRAGMA synchronous` = 2(FULL);存在表 `requests`、`audit`、`pending_confirm`、`pages`、`state`;不存在任何保存 `pending_kill` 条目的表
- 实现:`python/ops_bot/tests/test_audit.py::test_ops_181_sqlite_setup`

### OPS-182 本地写在 journal 中留下 Annotate{LocalWrite}

- 规则:[审计](README.md#审计)、[二次确认](README.md#二次确认)
- 级别:table
- 里程碑:M5
- 前置:`boot()`;发送者 T1
- 实现:`python/ops_bot/tests/test_audit.py::test_ops_182_localwrite_annotate`

| 行 | 本地写 | 期望 `FakeCtl` 收到的 `Annotate` |
|---|---|---|
| OPS-182.1 | `/params pc-1 set cooldown_ms 30s` 确认后 `apply_override` 成功 | `Annotate{kind: LocalWrite}` 发往 `prod`,`issuer = "tg:2001"`,`digest` = 64 位十六进制 |
| OPS-182.2 | `/warmup sim-1` 成功 | 发往 `sim` |
| OPS-182.3 | `/sub live-b off` | 发往 `prod`(`live-b` 所在实例) |
| OPS-182.4 | `apply_override` 失败 | 0 个 `Annotate` |
| OPS-182.5 | `/sub sim-a on` | 发往 `sim` |

### OPS-183 兜底停止的 journal 记录以 ops_bot 主体补写

- 规则:[审计](README.md#审计)
- 级别:unit
- 里程碑:M5
- 前置:OPS-093.1 的情形
- 步骤:
  1. 检查补发 `Kill` 与 `Annotate{OpsNote}` 的请求
- 期望:
  - 两者 `issuer = "ops_bot"`;`Annotate` 正文含 `req_id`、范围与结果;不出现 `issuer = "tg:1001"` 的补发
- 实现:`python/ops_bot/tests/test_audit.py::test_ops_183_fallback_annotate_issuer`

### OPS-190 只订阅 message 与 callback_query 两类 update

- 规则:[实现说明](README.md#实现说明)
- 级别:unit
- 里程碑:M5
- 前置:`boot()`
- 步骤:
  1. 读 `FakeTg` 收到的 `getUpdates` 参数
  2. 注入一条 `edited_message` 的 update
- 期望:
  - `allowed_updates = ["message", "callback_query"]`
  - 步骤 2 无回复、无审计行
- 实现:`python/ops_bot/tests/test_impl.py::test_ops_190_allowed_updates`

### OPS-191 命令处理只读内存视图,不在命令路径上扫 journal

- 规则:[实现说明](README.md#实现说明)
- 级别:unit
- 里程碑:M5
- 前置:`boot()`;视图已追上 `FakeJournal` 的 10 000 条记录;给 `hft_py.open_journal(...).events` 与 `.tail` 装计数探针
- 步骤:
  1. T1 发 `/p live-a`、`/risk live-a`、`/targets live-a`、`/orders live-a`、`/notes live-a`
- 期望:
  - 步骤 1 期间 `events` 调用 0 次;`tail` 只由尾随任务调用;每条回复 < 2 s
- 实现:`python/ops_bot/tests/test_impl.py::test_ops_191_handlers_use_view`

### OPS-192 每个实例一份视图,读 HeartbeatBlock 与 /status 同一函数

- 规则:[实现说明](README.md#实现说明)
- 级别:unit
- 里程碑:M5
- 前置:`boot()`;给 `hft_py.heartbeat.read` 装计数探针
- 步骤:
  1. V1 发 `/status`
  2. T1 发 `/stop live-a`
- 期望:
  - 两步都经 `hft_py.heartbeat.read("prod")` 读心跳(各 ≥ 1 次);`view.py` 维护 2 个视图对象(`prod`、`sim`),`FakeJournal[sim]` 的事件不出现在 `prod` 视图
- 实现:`python/ops_bot/tests/test_impl.py::test_ops_192_heartbeat_shared_reader`

### OPS-193 gateway 响应超时后按 cmd_id 补查,CmdResult 按 cmd_id 匹配

- 规则:[实现说明](README.md#实现说明)
- 级别:table
- 里程碑:M5
- 前置:`boot()`;发送者 T1;`/kill account live-a` 的 `cmd_id = C`
- 实现:`python/ops_bot/tests/test_impl.py::test_ops_193_result_by_cmd_id`

| 行 | 情形 | 期望 |
|---|---|---|
| OPS-193.1 | `FakeCtl[prod]` 回 `accepted` | ops_bot 调 `hftctl.result(C)`;原消息最终编辑为补查得到的结果 |
| OPS-193.2 | `FakeJournal[prod]` 先投递 `cmd_id = D` 的 `CmdResult`,再投递 `C` 的 | 原消息只按 `C` 的结果编辑 |

### OPS-200 延迟与资源契约:降级命令与 /stop 的处理时长

- 规则:[延迟与资源契约](README.md#延迟与资源契约)
- 级别:table
- 里程碑:M5
- 前置:`boot()`;`FakeTg` 注入 update 的时刻记为 0;`FakeCtl`、`FakeKS` 记录收到首字节的时刻;`stop.health_timeout_ms = 1000`;每行运行 100 次取最大值
- 实现:`python/ops_bot/tests/test_latency.py::test_ops_200_command_latency`

| 行 | 路径 | 期望 |
|---|---|---|
| OPS-200.1 | T1 `/kill account live-a` → `FakeCtl[prod]` 收到请求 | < 500 ms |
| OPS-200.2 | V1 点 `/stop live-a` 的确认 → `FakeCtl[prod]` 收到确认请求(第 4 步路径) | < 500 ms |
| OPS-200.3 | V1 `/stop live-a` → 预览消息发出 | < 2 s |
| OPS-200.4 | 兜底判定成立 → `FakeKS` 收到请求 | < 500 ms + 实际健康判定耗时 |
| OPS-200.5 | 失败或兜底判定成立 → P0 发出(`FakeTg` 收到) | < 2 s |

### OPS-201 延迟与资源契约:面板与成交推送

- 规则:[延迟与资源契约](README.md#延迟与资源契约)
- 级别:table
- 里程碑:M5
- 前置:`boot()`;视图已载入 1 天的 `live-a` 记录;每行运行 100 次取最大值
- 实现:`python/ops_bot/tests/test_latency.py::test_ops_201_panel_push_latency`

| 行 | 路径 | 期望 |
|---|---|---|
| OPS-201.1 | `/p live-a`、`/risk live-a`、`/targets live-a`、`/status` 各自 update → 回复发出 | < 2 s |
| OPS-201.2 | `FakeJournal[prod]` 写出成交 → 该账户的推送发出(首条摘要,不受每分钟合并影响) | < 5 s |

### OPS-210 源码与依赖中没有交易所 SDK、systemctl 调用与自写编解码

- 规则:[依赖](README.md#依赖)、[概要](README.md#概要)
- 级别:table
- 里程碑:M5
- 前置:静态检查脚本 `python/ops_bot/tests/test_static.py` 扫描 `src/` 与 `requirements.lock`;每行在临时副本中加入违规内容
- 实现:`python/ops_bot/tests/test_static.py::test_ops_210_forbidden_imports`

| 行 | 临时改动 | 期望 |
|---|---|---|
| OPS-210.1 | 不改 | 检查通过 |
| OPS-210.2 | `src/` 中 `import subprocess` | 检查失败,报告文件与行 |
| OPS-210.3 | `src/` 中字符串 `"systemctl"` 或 `"sudo"` | 检查失败 |
| OPS-210.4 | `requirements.lock` 加 `ccxt` | 检查失败 |
| OPS-210.5 | `src/hft_ops_bot/schema.py` 或 `src/hft_ops_bot/commands/` 中 `import struct` | 检查失败(编解码只经 `hft_py.params`) |
| OPS-210.6 | `src/` 中出现 `"apiKey"`、`"secretKey"` 字面量 | 检查失败 |

### OPS-211 ops_bot 只写自己的状态目录与他进程提供的接口

- 规则:[依赖](README.md#依赖)、[概要](README.md#概要)
- 级别:integ
- 里程碑:M5
- 前置:`boot()`;用 `sys.addaudithook` 记录每个以写方式打开的路径与每个 `connect` 的 Unix socket 路径
- 步骤:
  1. 依次运行 OPS-013 的全部行、OPS-072、OPS-093.1、OPS-128.5
- 期望:
  - 以写方式打开的路径都在 `<tmp>/state/` 下,或等于 `<tmp>/watchtower/acks.jsonl`
  - 连接的 socket 只有 `hft.instances[*].ctl_socket`、`pipeline.socket`、`killswitch.trigger_socket`
  - 没有对 `hft.instances[*].config_path`、`journal_*`、`heartbeat_path` 的写打开
- 实现:`python/ops_bot/tests/test_static.py::test_ops_211_write_paths`

### OPS-212 本包的参数与命令编解码只调用 hft_py.params

- 规则:[依赖](README.md#依赖)、[实现说明](README.md#实现说明)
- 级别:unit
- 里程碑:M5
- 前置:`boot()`;`hft_py.params.decode` 与 `encode_cmd` 被替换为返回哨兵值的桩(`decode` → 固定字段表 `{cooldown_ms: 12345}`;`encode_cmd` → 16 字节 `b"\x01\x01\x01" + b"\x00"*13`)
- 步骤:
  1. T1 发 `/params pc-1`
  2. T1 发 `/cmd pc-1 live-a 16777217 close group_frac=0.5`
- 期望:
  - 步骤 1 回复中 `cooldown_ms` 显示 `12.345s`(来自桩)
  - 步骤 2 请求的 `payload_b64` 解码后与桩返回的 16 字节逐字节相同
- 实现:`python/ops_bot/tests/test_impl.py::test_ops_212_codec_only_via_hft_py`

### OPS-220 老板以 Viewer 身份走完全部只读面板

- 规则:[命令表](README.md#命令表)、[账户、标签与视图](README.md#账户标签与视图)
- 级别:drill
- 里程碑:M6
- 门槛:⑪
- 环境:dev2 测试网联调窗口:`prod`(测试网账户)与 `sim` 实例运行;本机 ops_bot 使用 dev2 的 bot token
- 授权:无
- 步骤:
  1. 老板按 OPS-012 的命令清单在私聊中逐条发出(账户、风险、目标、PnL、模拟报告、参数、策略 Note)
  2. 每条回复由老板确认"可读",交易员在 ops-log 记录
- 通过标准:OPS-012 的 23 条命令都有回复且老板确认可读;`audit` 表中老板的行全部 `result = ok`
- 证据:`ops-log` 条目与 `audit` 表导出,文件名含 `OPS-220`
- 实现:`python/ops_bot/tests/host/OPS-220.sh`

### OPS-221 紧急停止与 stop_core 兜底演练(含待补 Kill 补发)

- 规则:[紧急停止(/stop)](README.md#紧急停止stop)
- 级别:drill
- 里程碑:M6
- 门槛:⑪
- 环境:dev2 测试网联调窗口的 `prod` 实例与演练账户;kill-switch 与 ops_bot 按发布归档安装
- 授权:`[需授权]`(启动 `hft-core@prod`)
- 步骤:按 [incident-response kill 演练](../../docs/runbooks/incident-response.md#kill-演练) 第 6、7 步(含两个变体)执行,由交易员按第 4 步恢复
- 通过标准:第 6 步的核对项全部成立;第 7 步:约 10 s 后写出 `pending_kill.json`、kill-switch 审计行调用方 uid = `hft-opsbot`、停机标记写出、挂单在 kill-switch 核验时限内清零、`hft-core@prod` 停止、P0 送达;重启后补发 Kill(P1)且补发前 `/resume` 被拒;"进程停摆"变体只撤单、P0 提示 `/stop … core`;"有在途链接"变体只撤单
- 证据:[kill 演练记录](../../docs/runbooks/incident-response.md#kill-演练记录) 一行 + ops_bot `audit` 导出 + kill-switch `actions.jsonl`,文件名含 `OPS-221`
- 实现:`docs/runbooks/incident-response.md#kill-演练`

### OPS-222 交易员工作流演练

- 规则:[命令表](README.md#命令表)、[参数面板与修改](README.md#参数面板与修改)、[策略命令与描述符](README.md#策略命令与描述符)
- 级别:drill
- 里程碑:M6
- 门槛:⑪
- 环境:dev2 测试网联调窗口;`pc-1` 与其校准对的模拟实例运行测试策略
- 授权:无
- 步骤:
  1. 交易员依次完成:`/mode` 切换(CloseOnly → Live 并确认)、`/kill account` 与 `/resume kill`、`/params <实例> set` 与 `/params pair <对> set`、单 group 强制退出(`/cmd … <reduce_only 命令>` 与 `/params <实例> set <强制退出字段> <group> true`)、`/warmup`、`/simreset`
- 通过标准:每一步的 `CmdResult` 或 pipeline 结果为成功;journal 中有对应的 `ControlCmd`/`CmdResult` 与 `Annotate{LocalWrite}`
- 证据:ops-log 条目与 journal 片段摘录,文件名含 `OPS-222`
- 实现:`python/ops_bot/tests/host/OPS-222.sh`

### OPS-223 模拟与实盘运行期间零越权执行

- 规则:[角色与鉴权](README.md#角色与鉴权)
- 级别:host
- 里程碑:M6
- 门槛:⑪
- 环境:dev2(M6 起)与升级主机(M7 起)上运行中的 ops_bot
- 授权:无
- 步骤:
  1. 每周导出 ops_bot `audit` 表与 journal 中全部 `Note{CtlAudit}` 与 `ControlCmd`
  2. 对每条被执行的写命令,核对其 issuer 的角色在[权限表](../../crates/hft-ctl-proto/README.md#权限表)中允许该命令与参数组合、聊天类型
- 通过标准:越权执行条数 = 0
- 证据:核对脚本输出,文件名含 `OPS-223` 与日期
- 实现:`python/ops_bot/tests/host/OPS-223.sh`

### OPS-224 视图停滞只按 ops.view_stale_ms 判定,不按心跳过期阈值

- 规则:[紧急停止(/stop)](README.md#紧急停止stop)、[配置文件字段](README.md#配置文件字段)
- 级别:unit
- 里程碑:M5
- 前置:同 OPS-073(ctl 一直不可连,`prod` 视图中无条件事件);`watchdog.hb_stale_ms = 1000`、`ops.view_stale_ms = 3000`;`FakeJournal[prod]` 在判定前 1 500 ms 起不再投递任何记录,`HeartbeatBlock` 新鲜
- 步骤:
  1. t = 10 s 到达兜底判定
- 期望:
  - `FakeKS` 收到 `action: "stop_core"`(1 500 ms 未前进不算停滞);P0 文案不含"视图停滞"
- 实现:`python/ops_bot/tests/test_stop.py::test_ops_224_view_stale_threshold_separate`

### OPS-225 schema 目录损坏时记 P1 并以 GetSchema 继续运行,紧急停止照常可用

- 规则:[配置文件字段](README.md#配置文件字段)、[参数面板与修改](README.md#参数面板与修改)
- 级别:unit
- 里程碑:M5
- 前置:`boot()`,其中 `<tmp>/schemas/index.json` 与文件不一致(`hft_py.params.load_schemas` 抛 `ParamsError`);`FakeCtl[prod]` 可连,对 `GetSchema{strategy_kind: "probe-target"}` 回一份 schema JSON,对 Kill 回 `Applied`
- 步骤:
  1. 启动 ops_bot
  2. T1 发 `/schema probe-target`
  3. V1 发 `/stop live-a` 并在 60 s 内确认
- 期望:
  - 步骤 1:进程运行(不退出);告警群收到一条内容含"schema 目录损坏"的 P1
  - 步骤 2:`FakeCtl` 收到 1 个 `GetSchema`;回复字段表来自该 schema
  - 步骤 3:`FakeCtl` 收到 Kill;推送"紧急停止已生效"
- 实现:`python/ops_bot/tests/test_schema.py::test_ops_225_corrupt_schema_dir_falls_back`

## 验收

- 本文件中里程碑 ≤ 当前里程碑的 `unit`/`table`/`prop`/`golden`/`integ` 用例全部有同编号的 pytest 函数并在 CI 质量门 `ops-py` 作业中离线通过;`prop` 用例的例数按各用例的 `例数`,每夜各跑 10 倍。
- 跨进程的紧急停止兜底与停机标记由 [MD-12](../../docs/tests/scenarios.md)、[MD-13](../../docs/tests/scenarios.md) 在 CI 的多进程作业中验证;Viewer 确认的 sequencer 侧判定由 MD-11 验证。
- 上线门槛 ⑪ 的 ops_bot 工作流([testing §8](../../docs/testing.md#8-上线门槛))以 OPS-220(老板走完只读面板)、OPS-221(`/stop` 与 `stop_core` 兜底演练,含待补 Kill 的自动补发,由交易员恢复)、OPS-222(交易员工作流)与 OPS-223(模拟与实盘运行期间零越权执行)的证据为准。
