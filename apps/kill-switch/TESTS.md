---
component: kill-switch
prefix: KS
spec: README.md
---

# kill-switch 测试规格

格式与规则见 [docs/tests](../../docs/tests/README.md)。本文件覆盖判活状态机、目标表、撤单动作、本地触发(`cancel` 与 `stop_core`)、停机标记的写入、合并与作废、身份核验与配置;hft-core 启动时读停机标记、写 `Kill` 与改名 `.consumed` 的用例归 [apps/hft-core](../hft-core/README.md#启动顺序),polkit 规则与单元权限的部署侧用例归 deploy,ops_bot 何时选择 `cancel`/`stop_core` 与待补 Kill 的用例归 [ops_bot](../../python/ops_bot/README.md)。

## 覆盖

| README 小节 | 用例 |
|---|---|
| 概要 | KS-080、KS-119、KS-120 |
| 目录 | 不适用:非规范 |
| 使用本组件 | KS-001–KS-007 |
| 判活与触发 | KS-040–KS-057、KS-172–KS-174、KS-190 |
| 撤单目标 | KS-022–KS-026 |
| 撤单动作 | KS-006、KS-070–KS-080、KS-192 |
| 本地触发(ops_bot) | KS-055、KS-057、KS-090–KS-098、KS-170、KS-191 |
| 停止 hft-core(stop_core) | KS-021、KS-110–KS-121、KS-171 |
| 身份核验(UID) | KS-007、KS-026、KS-081、KS-140–KS-142、KS-150 |
| 各所死手能力与兜底 | KS-024、KS-071、KS-150、KS-151 |
| 配置文件字段 | KS-002、KS-020、KS-021、KS-023、KS-049、KS-115 |
| 实现说明 | KS-057、KS-076、KS-077、KS-081–KS-083、KS-099、KS-112、KS-116、KS-121 |
| 延迟与资源契约 | KS-042、KS-094、KS-160–KS-162、KS-173 |
| 测试与验收 | 不适用:非规范(README 中只保留指向本文件的链接) |
| 依赖 | KS-002、KS-008 |
| 延伸阅读 | 不适用:非规范 |
| 已知限制与后续工作 | KS-096、KS-142 |
| 开发备忘 | KS-150、KS-171 |

## 用例

**基线 `K0`**(`apps/kill-switch/tests/common/mod.rs::k0()`;用例 `前置` 写"K0"时以下取值都显式生效,用例另列的取值覆盖它们;不依赖任何默认值——本 app 的配置没有默认值):

- 配置 `ks-k0.toml`:`schema = 1`、`poll_ms = 100`、`hft_core_config = [<tmp>/hft-core-prod.toml, <tmp>/hft-core-sim.toml]`、`refdata_blob = <tmp>/refdata.blob`、`hb_stale_factor = 2`、`hb_stale_hard_ms = 5000`、`verify_timeout_s = 60`、`exclude = []`、`trigger.socket = <tmp>/trigger.sock`、`trigger.ops_bot_user = "hft-opsbot"`(测试用户表中 uid 1100)、`trigger.stop_core_unit = "hft-core@prod.service"`、`trigger.stop_wait_s = 30`、`egress_bind = 127.0.0.2`、`trigger.dedupe_s = 600`、`[credentials.<account>.<venue>]` 每个 Live 目标一项(`base_url` 指向该 venue 的假交易所)、`alert.*` 指向测试告警接收端、`paths.state = <tmp>/state`。
- `prod` 实例的 hft-core 配置:`watchdog.hb_stale_ms = 1000`;`paths.heartbeat_shm`、`paths.journal_ram` 在 `<tmp>/shm/prod/` 下;账户 `acct-a`、`acct-b`(Live);`venues.binance-usdm.gw.accounts = [acct-a]`、`venues.bitget-spot.gw.accounts = [acct-a]`、`venues.binance-spot.gw.accounts = [acct-b]`、`venues.bitget-usdt-futures.gw.accounts = [acct-b]`。由此目标表 `T` = {`(acct-a, binance-usdm)`、`(acct-a, bitget-spot)`、`(acct-b, binance-spot)`、`(acct-b, bitget-usdt-futures)`}。
- `sim` 实例的 hft-core 配置:`watchdog.hb_stale_ms = 1000`,只有 Sim 账户 `sim-a`,没有任何凭证;其目标表为空。
- refdata:`acct-a` 的 `venue_uid`:`binance-usdm` = `"100000001"`、`bitget-spot` = `"200000001"`;`acct-b`:`binance-spot` = `"100000002"`、`bitget-usdt-futures` = `"200000002"`;`VenueCaps.dms`:`binance-usdm` 有,`binance-spot`、`bitget-*` 无。
- 心跳与提交游标:测试以 hft-rt `HeartbeatBlock::create` 在 `<tmp>/shm/<instance>/heartbeat` 建块,手动推进区 0 计数、设置 `HkLine.state`、`pid`、`boot_id`;`seq` 热段段头由测试写,手动推进 `committed_len`。"计数前进"= 每 100 ms 加 1;"游标前进"= 每 100 ms 增加 4 096。
- 时钟与轮询:假时钟,每推进 100 ms 调一次判活 `poll`;判活阈值 `W = hb_stale_factor × hb_stale_ms = 2 000 ms`。
- 假交易所 `FakeVenue`(每 venue 一个本地 HTTP 服务):实现查全部挂单、按 symbol 撤全部(或逐单撤)、账户信息(UID)、`/time`;可注入 429/418(带退避提示)、5xx、超时、撤单后残留;记录收到的每个请求与连接的本地源地址。初始每个目标在 2 个 symbol 上各 1 笔挂单。
- `systemctl` 由注入的 `SystemctlRunner` 记录 argv、环境与耗时并按脚本返回(退出码、stderr);停机标记目录由测试构建的 `MarkerDir` 注入点指向 `<tmp>/markers`(生产构建固定为 `/var/lib/hft/kill-switch`,见 KS-115)。
- 告警:测试告警接收端记录每条消息的级别与正文。

### KS-001 --check 只加载与校验,不连接、不建套接字

- 规则:[使用本组件](README.md#使用本组件)
- 级别:table
- 里程碑:M5
- 前置:K0;以 `kill-switch --check --config ks-k0.toml` 运行,每行只改列出的一处
- 实现:`apps/kill-switch/tests/cli.rs::ks_001_check_only`

| 行 | 改动 | 期望 |
|---|---|---|
| KS-001.1 | 无 | 退出 0;假交易所收到 0 个请求;`trigger.socket` 不存在;没有 `systemctl` 调用 |
| KS-001.2 | 同时缺 `poll_ms` 并给 Sim 账户 `sim-a` 写一项凭证 | 退出 2;stderr 同时列出两个错误(一次收集全部错误) |
| KS-001.3 | `acct-b` 缺 `bitget-usdt-futures` 的凭证 | 退出 2;假交易所收到 0 个请求 |

### KS-002 配置错误、UID 不符与格式版本不符退出 2

- 规则:[使用本组件](README.md#使用本组件)、[配置文件字段](README.md#配置文件字段)、[依赖](README.md#依赖)
- 级别:table
- 里程碑:M5
- 前置:K0;`kill-switch run`;每行只改列出的一处
- 实现:`apps/kill-switch/tests/cli.rs::ks_002_exit_2`

| 行 | 改动 | 期望 |
|---|---|---|
| KS-002.1 | 配置不是合法 TOML | 退出 2 |
| KS-002.2 | `schema = 2` | 退出 2 |
| KS-002.3 | 多出未知键 `foo = 1` | 退出 2 |
| KS-002.4 | `acct-a` 在 `binance-usdm` 的 UID 查询返回 `"100000009"` | P0 后退出 2 |
| KS-002.5 | 心跳块 `HkLine.layout_version = 2` | 退出 2 |
| KS-002.6 | 心跳块 `magic` 错一字节 | 退出 2 |
| KS-002.7 | refdata blob 的格式版本与本 app 认识的不同 | 退出 2 |
| KS-002.8 | `seq` 热段段头的格式版本不认识 | 退出 2 |
| KS-002.9 | hft-core 配置的 `schema` 不认识 | 退出 2 |
| KS-002.10 | 第 1–9 行的任一行 | 进程从不以退出码 4 结束 |

### KS-003 主机前置条件不满足退出 3

- 规则:[使用本组件](README.md#使用本组件)
- 级别:table
- 里程碑:M5
- 前置:K0;`kill-switch run`
- 实现:`apps/kill-switch/tests/cli.rs::ks_003_exit_3`

| 行 | 改动 | 期望 |
|---|---|---|
| KS-003.1 | `egress_bind = 192.0.2.7`(不是本机地址) | 退出 3;假交易所收到 0 个请求 |
| KS-003.2 | `trigger.socket` 所在目录不存在 | 退出 3 |
| KS-003.3 | 两者都是本机可用的值 | 进程常驻,判活开始 |

### KS-004 status 打印判活状态、目标表、核验状态与停机标记

- 规则:[使用本组件](README.md#使用本组件)
- 级别:unit
- 里程碑:M5
- 前置:K0;`kill-switch run` 在运行,`prod` 处于 `Watching`,`sim` 处于 `Idle`;`(acct-b, bitget-usdt-futures)` 的 UID 查询超时(`UidUnverified`);`<tmp>/markers` 中有 `stop-marker.json`(`scope "acct-a"`)与 `stop-marker.json.aborted`
- 步骤:
  1. `kill-switch status --config ks-k0.toml`
- 期望:
  - 输出含:`prod Watching`、`sim Idle`;各实例上次动作(无);`prod` 的 4 个目标与 `sim` 的空目标表;3 个目标 `verified`、`(acct-b, bitget-usdt-futures)` `UidUnverified`;未消费标记 `scope = acct-a` 与一份已作废标记
  - 退出 0;`status` 本身不向任何交易所发请求
- 实现:`apps/kill-switch/tests/cli.rs::ks_004_status`

### KS-005 dry-run 只查挂单、打印将撤什么

- 规则:[使用本组件](README.md#使用本组件)
- 级别:table
- 里程碑:M5
- 前置:K0
- 实现:`apps/kill-switch/tests/cli.rs::ks_005_dry_run`

| 行 | 命令行 | 期望 |
|---|---|---|
| KS-005.1 | `dry-run` | 对 4 个目标各发 1 个查挂单请求;输出每个目标的 symbol 与笔数;0 个撤单请求;退出 0 |
| KS-005.2 | `dry-run --account acct-a` | 只查 `acct-a` 的 2 个目标 |
| KS-005.3 | `dry-run --venue bitget-spot` | 只查 `(acct-a, bitget-spot)` |

### KS-006 fire 不看心跳,按 --account 与 --venue 撤单

- 规则:[使用本组件](README.md#使用本组件)、[撤单动作](README.md#撤单动作)
- 级别:table
- 里程碑:M5
- 前置:K0;`prod` 心跳每 100 ms 前进
- 实现:`apps/kill-switch/tests/cli.rs::ks_006_fire`

| 行 | 命令行 / 终端输入 | 期望 |
|---|---|---|
| KS-006.1 | `fire --account acct-a --venue all --reason x --yes` | `acct-a` 的 2 个目标全部撤单并核验为 0;`acct-b` 的目标收到 0 个请求;退出 0 |
| KS-006.2 | `fire --account all --venue bitget-spot --reason x --yes` | 只有 `(acct-a, bitget-spot)` 被撤 |
| KS-006.3 | 不带 `--yes`,终端输入 `yes` | 打印将撤的目标后执行;退出 0 |
| KS-006.4 | 不带 `--yes`,终端输入 `no` | 打印将撤的目标;0 个撤单请求;退出 1 |
| KS-006.5 | `--yes`,`(acct-a, bitget-spot)` 撤单后始终残留 1 笔 | 60 s 后退出 1;P0 含残留清单 |
| KS-006.6 | 任一行 | 判活状态机不变(`prod` 仍为 `Watching`) |

### KS-007 selftest 用每把凭证查 UID

- 规则:[使用本组件](README.md#使用本组件)、[身份核验(UID)](README.md#身份核验uid)
- 级别:table
- 里程碑:M5
- 前置:K0;`kill-switch selftest --config ks-k0.toml`
- 实现:`apps/kill-switch/tests/cli.rs::ks_007_selftest`

| 行 | 假交易所的 UID 应答 | 期望 |
|---|---|---|
| KS-007.1 | 4 个目标都与 refdata 相同 | 每个目标恰 1 个只读签名请求;退出 0 |
| KS-007.2 | `(acct-a, bitget-spot)` 返回 `"200000009"` | P0(两个 UID 只显示末 4 位 `0001`、`0009`);退出 2 |
| KS-007.3 | `(acct-b, binance-spot)` 返回 401(key 过期) | P0;退出 1 |
| KS-007.4 | `sim` 实例(无凭证) | 不发请求,直接通过 |

### KS-008 依赖闭包只含 hft-rt 与列出的外部库

- 规则:[依赖](README.md#依赖)
- 级别:unit
- 里程碑:M5
- 前置:`tools/xtask/deps.toml`
- 步骤:
  1. `cargo xtask deps` 检查 `kill-switch` 的依赖闭包(不含 dev 依赖)
- 期望:
  - `hft-*` 依赖只有 `hft-rt`;闭包中没有 `tokio`、任何 `venue-*`、`hft-journal`、`hft-refdata` 与 D-Bus 客户端库
  - 外部直接依赖是 {`ureq`、`rustls`、`hmac`、`sha2`、`base64`、`serde`、`serde_json`、`toml`、`libc`} 的子集
- 实现:`tools/xtask/tests/deps_apps.rs::ks_008_kill_switch_dep_closure`

### KS-020 每个必填字段缺失即退出 2

- 规则:[配置文件字段](README.md#配置文件字段)
- 级别:table
- 里程碑:M5
- 前置:K0;每行删去列出的一个键后 `kill-switch run`
- 实现:`apps/kill-switch/tests/config.rs::ks_020_required_fields`

| 行 | 删去的键 | 期望 |
|---|---|---|
| KS-020.1 | `poll_ms` | 退出 2(不取任何默认值) |
| KS-020.2 | `hft_core_config` | 退出 2 |
| KS-020.3 | `refdata_blob` | 退出 2 |
| KS-020.4 | `hb_stale_factor` | 退出 2 |
| KS-020.5 | `hb_stale_hard_ms` | 退出 2 |
| KS-020.6 | `verify_timeout_s` | 退出 2 |
| KS-020.7 | `exclude` | 退出 2 |
| KS-020.8 | `trigger.socket` | 退出 2 |
| KS-020.9 | `trigger.ops_bot_user` | 退出 2 |
| KS-020.10 | `trigger.stop_core_unit` | 退出 2 |
| KS-020.11 | `trigger.stop_wait_s` | 退出 2 |
| KS-020.12 | `egress_bind` | 退出 2 |
| KS-020.13 | `trigger.dedupe_s` | 退出 2 |
| KS-020.14 | `alert.telegram_chat_id` | 退出 2 |
| KS-020.15 | `alert.telegram_token_secret` | 退出 2 |
| KS-020.16 | `alert.bark_key_secret` | 退出 2 |
| KS-020.17 | `paths.state` | 退出 2 |
| KS-020.18 | `[credentials.acct-a.binance-usdm]` 的 `base_url` | 退出 2 |

### KS-021 trigger.stop_core_unit 只能是 hft-core@prod.service,ops_bot 用户必须存在

- 规则:[配置文件字段](README.md#配置文件字段)、[停止 hft-core(stop_core)](README.md#停止-hft-corestop_core)
- 级别:table
- 里程碑:M5
- 前置:K0
- 实现:`apps/kill-switch/tests/config.rs::ks_021_trigger_fields`

| 行 | 改动 | 期望 |
|---|---|---|
| KS-021.1 | `trigger.stop_core_unit = "hft-core@sim.service"` | 退出 2 |
| KS-021.2 | `trigger.stop_core_unit = "hft-core@prod"`(缺后缀) | 退出 2 |
| KS-021.3 | `trigger.stop_core_unit = "hft-core@prod.service"` | 进程常驻,判活开始 |
| KS-021.4 | `trigger.ops_bot_user = "nobody-such"` | 退出 2 |
| KS-021.5 | `trigger.ops_bot_user = "hft-opsbot"` | 启动时解析为 uid 1100 |

### KS-022 目标表只含 Live 账户 × 它在 gw.accounts 中出现的 venue

- 规则:[撤单目标](README.md#撤单目标)
- 级别:unit
- 里程碑:M5
- 前置:K0;`prod` 的 hft-core 配置另有一个 Sim 账户 `sim-x`(只为本用例;它在 `venues.binance-usdm.gw.accounts` 中出现)
- 步骤:
  1. 启动 `kill-switch run`,调 `status`
- 期望:
  - `prod` 的目标表恰为 `T` 的 4 项,不含 `sim-x`;交易所账户上带其他 client id 的挂单也按目标撤(目标按 (account, venue),不按 client id 过滤)
- 实现:`apps/kill-switch/tests/targets.rs::ks_022_live_targets_only`

### KS-023 凭证规则:Sim 账户写凭证、Live 目标缺凭证都退出 2

- 规则:[撤单目标](README.md#撤单目标)、[配置文件字段](README.md#配置文件字段)
- 级别:table
- 里程碑:M5
- 前置:K0
- 实现:`apps/kill-switch/tests/targets.rs::ks_023_credentials`

| 行 | 改动 | 期望 |
|---|---|---|
| KS-023.1 | 加 `[credentials.sim-a.binance-usdm]` | 退出 2 |
| KS-023.2 | 删去 `[credentials.acct-a.bitget-spot]` | 退出 2 |
| KS-023.3 | 凭证引用的 secrets 文件不存在 | 退出 2 |
| KS-023.4 | 4 个目标的凭证齐全 | 进程常驻,判活开始 |

### KS-024 exclude 只允许有交易所侧死手的 venue,且每次触发都列出被排除的目标

- 规则:[撤单目标](README.md#撤单目标)、[各所死手能力与兜底](README.md#各所死手能力与兜底)
- 级别:table
- 里程碑:M5
- 前置:K0
- 实现:`apps/kill-switch/tests/targets.rs::ks_024_exclude`

| 行 | `exclude` / 动作 | 期望 |
|---|---|---|
| KS-024.1 | `[{account = "acct-a", venue = "bitget-spot"}]`(无死手) | 退出 2 |
| KS-024.2 | `[{account = "acct-b", venue = "binance-spot"}]`(无死手) | 退出 2 |
| KS-024.3 | `[{account = "acct-a", venue = "binance-usdm"}]`(有死手) | 进程常驻,目标表 3 项 |
| KS-024.4 | 024.3 的配置下 `prod` 停摆 | 只撤 3 个目标;开始告警与整体结果告警都列出被排除的 `(acct-a, binance-usdm)` |
| KS-024.5 | `exclude = []` 时 `prod` 停摆 | 有死手的 `(acct-a, binance-usdm)` 同样被撤 |

### KS-025 只含 Sim 账户的实例:不需要凭证,停摆只发 P1

- 规则:[撤单目标](README.md#撤单目标)
- 级别:unit
- 里程碑:M5
- 前置:K0(`sim` 实例没有任何凭证)
- 步骤:
  1. 启动后 `status`
  2. `sim` 心跳以 `state = Running` 前进 1 s 后停止,两个信号都停 2 000 ms
  3. 同时 `prod` 心跳也停摆
- 期望:
  - 步骤 1:进程常驻(不因缺凭证退出),`sim` 目标表为空
  - 步骤 2:`sim` 进入 `Fired`;恰有一条 P1(实例 `sim`、`(pid, boot_id)`、"无 Live 目标");对 `sim` 0 个 REST 请求;进程不退出
  - 步骤 3:`prod` 照常对 4 个目标撤单
- 实现:`apps/kill-switch/tests/targets.rs::ks_025_sim_only_instance`

### KS-026 hft-core 配置变更后 1 s 内重建目标表,新目标先过身份核验

- 规则:[撤单目标](README.md#撤单目标)、[身份核验(UID)](README.md#身份核验uid)
- 级别:table
- 里程碑:M5
- 前置:K0;kill-switch 已运行;本配置预先含 `[credentials.acct-c.binance-usdm]`;refdata 中 `acct-c`(Live)的 `binance-usdm` UID 为 `"100000003"`
- 实现:`apps/kill-switch/tests/targets.rs::ks_026_rebuild_on_config_change`

| 行 | 改动 | 期望 |
|---|---|---|
| KS-026.1 | 给 `prod` 的 hft-core 配置加 `acct-c`,`venues.binance-usdm.gw.accounts` 加 `acct-c`;UID 应答相同 | 1 000 ms 内(housekeeping 下一次比较)对 `(acct-c, binance-usdm)` 发 1 个 UID 查询,通过后目标表 5 项 |
| KS-026.2 | 同上但 UID 应答 `"100000009"` | P0 后退出 2;目标从未加入 |
| KS-026.3 | 加 `acct-d`(本配置没有它的凭证) | 退出 2 |
| KS-026.4 | hft-core 配置被写成不可解析的内容 | 保留原 4 项目标;P0;进程不退出 |
| KS-026.6 | 只改文件的 mtime、内容不变 | 重建结果与原表相同,不发 UID 查询 |

### KS-040 Idle 的停留与进入 Watching 的条件

- 规则:[判活与触发](README.md#判活与触发)
- 级别:table
- 里程碑:M5
- 前置:K0;只看 `prod` 实例;每行从 kill-switch 已运行、`prod` 为 `Idle` 开始,按行设定心跳并推进 60 s
- 实现:`apps/kill-switch/tests/watch.rs::ks_040_idle_and_watching`

| 行 | 心跳块 | 期望 |
|---|---|---|
| KS-040.1 | 心跳文件不存在 | 始终 `Idle`;0 个 REST 请求 |
| KS-040.2 | `state = Starting`,计数不变 | 始终 `Idle`;0 个 REST 请求 |
| KS-040.3 | `state = Warmup`,计数前进 | 第一次看到计数前进的那次 `poll` 进入 `Watching` |
| KS-040.4 | `state = Running`,计数前进 | 进入 `Watching` |
| KS-040.5 | `state = Draining`,计数前进 | 进入 `Watching` |
| KS-040.6 | `state = Stopped`,计数不变 | 始终 `Idle`;0 个 REST 请求 |

### KS-041 Watching 与 Suspect 中读到 Stopped 或心跳文件消失即回到 Idle

- 规则:[判活与触发](README.md#判活与触发)
- 级别:table
- 里程碑:M5
- 前置:K0;`prod` 为 `Watching`(`Running`,两信号都在前进);t0 起按行改变心跳,再推进 10 s
- 实现:`apps/kill-switch/tests/watch.rs::ks_041_back_to_idle`

| 行 | t0 时的变化 | 期望 |
|---|---|---|
| KS-041.1 | `state = Stopped`,计数与游标停止 | t0 之后第一次 `poll` 进入 `Idle`;0 个 REST 请求 |
| KS-041.2 | 删除心跳文件 | 进入 `Idle`;心跳映射解除 |
| KS-041.3 | 计数停 2 000 ms(进入 `Suspect`)后 t0 + 2 100 ms 置 `state = Stopped` | t0 + 2 100 ms 的 `poll` 进入 `Idle`;0 个 REST 请求 |

### KS-042 两个信号都停 W 即触发

- 规则:[判活与触发](README.md#判活与触发)、[延迟与资源契约](README.md#延迟与资源契约)
- 级别:table
- 里程碑:M5
- 前置:K0(`W = 2 000 ms`,`poll_ms = 100`);`prod` 为 `Watching`;计数与游标最后一次前进在 t0
- 实现:`apps/kill-switch/tests/watch.rs::ks_042_both_stalled_fire`

| 行 | 时刻 | 期望 |
|---|---|---|
| KS-042.1 | t0 + 1 900 ms 的 `poll` | 仍为 `Watching`;0 个 REST 请求 |
| KS-042.2 | t0 + 2 000 ms 的 `poll` | 依次进入 `Suspect`、`Fire`;状态迁移记录中 `Fire` 紧跟 `Suspect` |
| KS-042.3 | 042.2 之后 | 4 个目标的第一个撤单请求的发出时刻都 ≤ t0 + 2 200 ms;没有"信号不一致"的 P0 |

### KS-043 单一信号停滞时等到 hb_stale_hard_ms 再触发并报信号不一致

- 规则:[判活与触发](README.md#判活与触发)
- 级别:table
- 里程碑:M5
- 前置:K0(`W = 2 000 ms`、`hb_stale_hard_ms = 5 000`);`prod` 为 `Watching`
- 实现:`apps/kill-switch/tests/watch.rs::ks_043_single_signal`

| 行 | 信号(t0 起) | 期望 |
|---|---|---|
| KS-043.1 | 计数停,游标前进;观察 t0 + 2 000 ms | `Suspect`;0 个 REST 请求 |
| KS-043.2 | 同上;观察 t0 + 4 900 ms | 仍为 `Suspect` |
| KS-043.3 | 同上;观察 t0 + 5 000 ms | `Fire`;P0 告警含"信号不一致" |
| KS-043.4 | 游标停,计数前进;观察 t0 + 2 000 ms、4 900 ms、5 000 ms | 依次 `Suspect`、`Suspect`、`Fire`;P0 含"信号不一致" |
| KS-043.5 | 计数停,游标前进;t0 + 3 000 ms 计数恢复 | t0 + 3 000 ms 的 `poll` 回到 `Watching`;0 个 REST 请求 |
| KS-043.6 | 计数 t0 停,游标 t0 + 1 000 ms 停 | t0 + 3 000 ms 的 `poll` 进入 `Fire`(两者都已停 ≥ W);没有"信号不一致"的 P0 |

### KS-044 Draining 停摆触发,Stopped 不触发

- 规则:[判活与触发](README.md#判活与触发)
- 级别:table
- 里程碑:M5
- 前置:K0;`prod` 为 `Watching`;t0 起两信号都停
- 实现:`apps/kill-switch/tests/watch.rs::ks_044_draining_vs_stopped`

| 行 | t0 时的 `state` | 期望 |
|---|---|---|
| KS-044.1 | `Draining`(优雅退出超时被 SIGKILL 的情形) | t0 + 2 000 ms 进入 `Fire`,4 个目标撤单 |
| KS-044.2 | `Stopped` | 进入 `Idle`,0 个 REST 请求 |
| KS-044.3 | `Running` | t0 + 2 000 ms 进入 `Fire` |

### KS-045 同一 (pid, boot_id) 只触发一次

- 规则:[判活与触发](README.md#判活与触发)
- 级别:unit
- 里程碑:M5
- 前置:K0;`prod`(pid 4242、boot_id B1)已 `Fired`
- 步骤:
  1. 同一 pid 与 boot_id 的计数恢复前进 5 s
  2. 再停 10 s
- 期望:
  - 状态始终为 `Fired`;步骤 1–2 期间 0 个新的 REST 请求、0 条新的触发告警
- 实现:`apps/kill-switch/tests/watch.rs::ks_045_fire_once_per_process`

### KS-046 Fired 只在新的 (pid, boot_id) 以 Warmup 或 Running 前进时回到 Watching

- 规则:[判活与触发](README.md#判活与触发)
- 级别:table
- 里程碑:M5
- 前置:K0;`prod`(pid 4242、boot_id B1)已 `Fired`;每行换成新的心跳块内容
- 实现:`apps/kill-switch/tests/watch.rs::ks_046_fired_to_watching`

| 行 | 新心跳块 | 期望 |
|---|---|---|
| KS-046.1 | pid 5000、B1,`Starting`,计数 0 | 仍为 `Fired` |
| KS-046.2 | pid 5000、B1,`Warmup`,计数前进 | 回到 `Watching`(不等闸门放行) |
| KS-046.3 | pid 5000、B1,`Running`,计数前进 | 回到 `Watching` |
| KS-046.4 | pid 4242、B2(重启后 pid 复用),`Warmup`,计数前进 | 回到 `Watching` |
| KS-046.5 | pid 5000、B1,`Draining`,计数前进 | 仍为 `Fired` |
| KS-046.6 | 046.2 之后新进程两信号都停 2 000 ms | 再次 `Fire`(新的一对 (pid, boot_id)) |

### KS-047 kill-switch 启动时对已停摆的实例照样走判活流程触发

- 规则:[判活与触发](README.md#判活与触发)
- 级别:table
- 里程碑:M5
- 前置:K0;启动 kill-switch 时 `prod` 的心跳块已存在,按行设定
- 实现:`apps/kill-switch/tests/watch.rs::ks_047_startup_stale`

| 行 | 启动时的心跳块 / `latch.json` | 期望 |
|---|---|---|
| KS-047.1 | `Running`,计数与游标都不再前进;`latch.json` 无该实例记录 | 启动(UID 核验完成)即 `Watching`;启动后 2 000 ms 进入 `Fire` |
| KS-047.2 | 同上,`latch.json` 记有该 (pid, boot_id) 已触发 | 直接 `Fired`,0 个撤单请求 |
| KS-047.3 | `Stopped` | `Idle` |
| KS-047.4 | `Running`,计数前进 | `Watching`,不触发 |

### KS-048 进程存活与否不参与判定,只写进告警详情

- 规则:[判活与触发](README.md#判活与触发)
- 级别:unit
- 里程碑:M5
- 前置:K0;`HkLine.pid` 指向一个仍存活的进程(测试进程自身的子进程,`SIGSTOP` 中)
- 步骤:
  1. 两信号都停 2 000 ms
- 期望:
  - 进入 `Fire`,4 个目标撤单;触发告警详情含该 pid 与"进程存活"
- 实现:`apps/kill-switch/tests/watch.rs::ks_048_liveness_only_in_detail`

### KS-049 轮询粒度:阈值按"自上次前进起 ≥ 时长"在下一次轮询判定

- 规则:[判活与触发](README.md#判活与触发)、[配置文件字段](README.md#配置文件字段)
- 级别:table
- 里程碑:M5
- 前置:K0,但 `poll_ms = 300`;`poll` 在 t0 + 300 × k 时刻执行;两信号最后一次前进在 t0
- 实现:`apps/kill-switch/tests/watch.rs::ks_049_poll_granularity`

| 行 | 时刻 | 期望 |
|---|---|---|
| KS-049.1 | t0 + 1 800 ms | `Watching` |
| KS-049.2 | t0 + 2 100 ms(第一次 ≥ 2 000 ms 的轮询) | `Fire` |

### KS-050 段滚动时提交游标跟随新热段,不误触发

- 规则:[判活与触发](README.md#判活与触发)
- 级别:table
- 里程碑:M5
- 前置:K0;`prod` 为 `Watching`,计数一直前进
- 实现:`apps/kill-switch/tests/watch.rs::ks_050_segment_switch`

| 行 | 热段变化 | 期望 |
|---|---|---|
| KS-050.1 | 旧段置 `retired`,同一次 `poll` 前新段出现且 `committed_len` 前进 | 下一次 `poll` 起读新段;状态保持 `Watching` |
| KS-050.2 | 旧段 `retired` 后 1 000 ms 新段才出现 | 期间不进入 `Fire`;新段出现后读新段 |
| KS-050.3 | 旧段 `retired` 后新段一直不出现,计数照常前进 | 游标停滞 ≥ 2 000 ms 进入 `Suspect`;5 000 ms 时 `Fire` 并报"信号不一致" |

### KS-051 心跳文件被替换(inode 变化)时重新映射

- 规则:[判活与触发](README.md#判活与触发)
- 级别:unit
- 里程碑:M5
- 前置:K0;`prod` 为 `Watching`
- 步骤:
  1. 以 rename 方式换成新的心跳文件(新 inode,同 pid,计数从 1 000 继续前进)
  2. 推进 5 s
- 期望:
  - 下一次 `poll` 起读到新文件的计数;状态保持 `Watching`;旧映射已解除;0 个 REST 请求
- 实现:`apps/kill-switch/tests/watch.rs::ks_051_remap_on_inode_change`

### KS-052 映射中文件被截断引发的 SIGBUS 被捕获,不误触发

- 规则:[判活与触发](README.md#判活与触发)
- 级别:unit
- 里程碑:M5
- 前置:K0;`prod` 为 `Watching`
- 步骤:
  1. 在两次 `poll` 之间把心跳文件截断为 0 字节
  2. 下一次 `poll`(读映射触发 SIGBUS)
  3. 再下一次 `poll` 前把文件恢复为 4 096 字节、计数继续前进
- 期望:
  - 步骤 2 进程不崩溃,该映射标记为失效;状态不变(仍 `Watching`)
  - 步骤 3 之后重新映射,读到前进的计数;全程 0 个 REST 请求
- 实现:`apps/kill-switch/tests/watch.rs::ks_052_sigbus_caught`

### KS-053 已映射的文件被删除时立即解除映射

- 规则:[判活与触发](README.md#判活与触发)
- 级别:table
- 里程碑:M5
- 前置:K0;`prod` 先 `Stopped`(干净退出)后
- 实现:`apps/kill-switch/tests/watch.rs::ks_053_unmap_deleted`

| 行 | 动作 | 期望 |
|---|---|---|
| KS-053.1 | 删除 `seq` 热段文件(`fstat` 得 `st_nlink = 0`) | 下一次 `poll` 解除该映射(本进程的映射表中不再有该文件) |
| KS-053.2 | 删除心跳文件 | 下一次 `poll` 解除映射,状态 `Idle` |

### KS-054 撤单进行中出现新进程:完成当前一轮后停止,交给新进程

- 规则:[判活与触发](README.md#判活与触发)
- 级别:unit
- 里程碑:M5
- 前置:K0;`prod` 停摆触发;`(acct-a, bitget-spot)` 每次撤单后都残留 1 笔(进入重试)
- 步骤:
  1. 第 3 次重试的撤单请求发出后,新的 (pid, boot_id) 以 `Warmup` 计数前进
  2. 推进 30 s
- 期望:
  - 该目标完成当前这一轮(1 个撤单 + 1 个核验)后不再发请求;其他目标若已核验为 0 不再请求
  - 状态 `Watching`;一条告警列出交给新进程的残留挂单
- 实现:`apps/kill-switch/tests/watch.rs::ks_054_handover_to_new_process`

### KS-055 本地触发不改变判活状态机

- 规则:[判活与触发](README.md#判活与触发)、[本地触发(ops_bot)](README.md#本地触发ops_bot)
- 级别:unit
- 里程碑:M5
- 前置:K0;`prod` 为 `Watching`
- 步骤:
  1. ops_bot 发 `cancel`,`instance prod`、`account *`,等到 `Done`
  2. 之后两信号都停 2 000 ms
- 期望:
  - 步骤 1 期间与之后状态迁移记录中没有任何迁移
  - 步骤 2 照常进入 `Fire`,对 4 个目标撤单
- 实现:`apps/kill-switch/tests/watch.rs::ks_055_trigger_does_not_touch_fsm`

### KS-056 判活状态机只走允许的迁移

- 规则:[判活与触发](README.md#判活与触发)
- 级别:prop
- 里程碑:M5
- 性质:迁移集合 ⊆ {Idle→Watching、Watching→Idle、Watching→Suspect、Suspect→Watching、Suspect→Idle、Suspect→Fire、Fire→Verify、Verify→Fired、Fired→Watching};`Fire` 只从 `Suspect` 进入;同一 (pid, boot_id) 至多一次 `Fire`;读到 `state = Stopped` 的那次 `poll` 从不进入 `Fire`;`Fired→Watching` 时新 (pid, boot_id) 与触发时不同且 `state ∈ {Warmup, Running}`
- 生成:每 100 ms 一步、共 1–600 步;每步随机:计数前进或停、游标前进或停、`state` 变化、换 (pid, boot_id)、段滚动、心跳文件删除或替换
- 参考:按 README 状态图写的参考状态机
- 例数:PR 1 000
- 实现:`apps/kill-switch/tests/watch.rs::ks_056_fsm_transitions_allowed`

### KS-057 锁存状态与 req_id 去重表在 kill-switch 重启后保留

- 规则:[实现说明](README.md#实现说明)、[本地触发(ops_bot)](README.md#本地触发ops_bot)
- 级别:unit
- 里程碑:M5
- 前置:K0;`prod`(pid 4242、B1)已 `Fired`;ops_bot 在 100 s 前以 `req_id R` 完成一次 `cancel`
- 步骤:
  1. 重启 kill-switch(`prod` 心跳仍停摆)
  2. ops_bot 以同一 `req_id R` 再发
- 期望:
  - `<state>/latch.json` 含 `prod` 的 (4242, B1) 与 `R` 的结果;步骤 1 后 `prod` 直接 `Fired`,0 个撤单请求
  - 步骤 2 返回与 100 s 前相同的 `Done`,0 个 REST 请求
- 实现:`apps/kill-switch/tests/watch.rs::ks_057_latch_persisted`

### KS-070 查全部挂单、按 symbol 撤、再核验

- 规则:[撤单动作](README.md#撤单动作)
- 级别:unit
- 里程碑:M5
- 前置:K0;只看目标 `(acct-b, binance-spot)`:挂单在 `BTCUSDT` 与 `ETHUSDT` 各 1 笔
- 步骤:
  1. 以 `fire --account acct-b --venue binance-spot --reason t --yes` 触发
- 期望:
  - 该目标依次收到:`GET /api/v3/openOrders`(不带 symbol)、`DELETE /api/v3/openOrders?symbol=BTCUSDT` 与 `…?symbol=ETHUSDT`(两者顺序不限)、`GET /api/v3/openOrders` 返回空;之后不再请求
  - 每个请求先 `bind()` 到 `egress_bind` 再连接(假交易所看到的源地址为 127.0.0.2)
- 实现:`apps/kill-switch/tests/fire.rs::ks_070_list_cancel_verify`

### KS-071 没有按 symbol 撤全部的接口时逐单撤

- 规则:[撤单动作](README.md#撤单动作)、[各所死手能力与兜底](README.md#各所死手能力与兜底)
- 级别:unit
- 里程碑:M5
- 前置:K0,另加 Live 账户 `acct-o`(venue `okx-swap`,有凭证);该目标有 3 笔挂单
- 步骤:
  1. `fire --account acct-o --venue okx-swap --reason t --yes`
- 期望:
  - `GET /api/v5/trade/orders-pending` 之后,`POST /api/v5/trade/cancel-batch-orders` 的请求体合计覆盖这 3 个订单号;再 `GET` 为空后结束
- 实现:`apps/kill-switch/tests/fire.rs::ks_071_per_order_cancel`

### KS-072 核验仍有挂单时按 200、400、800 … 5 000 ms 退避重试

- 规则:[撤单动作](README.md#撤单动作)
- 级别:table
- 里程碑:M5
- 前置:K0;`verify_timeout_s = 60`;`(acct-b, binance-spot)` 每次撤单后都残留 1 笔;t = 0 为第一次核验返回非空的时刻
- 实现:`apps/kill-switch/tests/fire.rs::ks_072_retry_backoff`

| 行 | 第 k 次重试 | 期望的撤单请求时刻(相对上一次核验) |
|---|---|---|
| KS-072.1 | 1 | 200 ms |
| KS-072.2 | 2 | 400 ms |
| KS-072.3 | 3 | 800 ms |
| KS-072.4 | 4 | 1 600 ms |
| KS-072.5 | 5 | 3 200 ms |
| KS-072.6 | 6 | 5 000 ms(上限) |
| KS-072.7 | 7 及以后 | 5 000 ms |

### KS-073 超过 verify_timeout_s 仍有挂单:停止重试、进入 Fired 并报 P0 残留清单

- 规则:[撤单动作](README.md#撤单动作)、[判活与触发](README.md#判活与触发)
- 级别:table
- 里程碑:M5
- 前置:K0;`verify_timeout_s = 60`;`prod` 停摆触发,t = 0 为 `Fire` 时刻
- 实现:`apps/kill-switch/tests/fire.rs::ks_073_verify_timeout`

| 行 | `(acct-b, binance-spot)` 的挂单 | 期望 |
|---|---|---|
| KS-073.1 | 在 t = 59 900 ms 的核验中为 0 | `Fired`;没有残留 P0 |
| KS-073.2 | 始终残留 1 笔 `ETHUSDT` | t = 60 000 ms 后不再请求;`Fired`;P0 列出 `(acct-b, binance-spot, ETHUSDT, 1)` |
| KS-073.3 | 其他 3 个目标在 1 s 内为 0 | 它们只各有一轮请求,不随该目标重试 |

### KS-074 限速错误按响应的退避提示等待,不因限速放弃

- 规则:[撤单动作](README.md#撤单动作)
- 级别:table
- 里程碑:M5
- 前置:K0;`(acct-b, binance-spot)`
- 实现:`apps/kill-switch/tests/fire.rs::ks_074_rate_limit`

| 行 | 假交易所对第一个撤单请求的应答 | 期望 |
|---|---|---|
| KS-074.1 | 429,`Retry-After: 2` | 下一个请求 ≥ 2 000 ms 之后;最终核验为 0 |
| KS-074.2 | 418,`Retry-After: 5` | 下一个请求 ≥ 5 000 ms 之后;最终核验为 0 |
| KS-074.3 | 连续 429 直到 60 s | 60 s 时停止,P0 残留清单;期间没有因限速提前放弃 |

### KS-075 部分失败重试直到核验为 0

- 规则:[撤单动作](README.md#撤单动作)
- 级别:unit
- 里程碑:M5
- 前置:K0;`(acct-b, binance-spot)` 的 `DELETE …?symbol=BTCUSDT` 第一次返回 500
- 步骤:
  1. 触发该目标
- 期望:
  - 第一次核验仍见 `BTCUSDT` 1 笔;200 ms 后重撤 `BTCUSDT`;第二次核验为 0;没有 P0
- 实现:`apps/kill-switch/tests/fire.rs::ks_075_partial_failure_retry`

### KS-076 每个目标独立线程并行,慢目标不拖累其他目标

- 规则:[撤单动作](README.md#撤单动作)、[实现说明](README.md#实现说明)
- 级别:unit
- 里程碑:M5
- 前置:K0;`(acct-b, binance-spot)` 的每个应答延迟 5 s
- 步骤:
  1. `prod` 两信号都停 2 000 ms,触发
- 期望:
  - 其余 3 个目标的第一个撤单请求都在 `Fire` 后 200 ms 内发出,并在慢目标完成之前核验为 0;判活 `poll` 期间照常每 100 ms 执行
- 实现:`apps/kill-switch/tests/fire.rs::ks_076_parallel_targets`

### KS-077 同一目标已有轮次时新的触发并入,不并发两轮

- 规则:[撤单动作](README.md#撤单动作)、[实现说明](README.md#实现说明)
- 级别:unit
- 里程碑:M5
- 前置:K0;`prod` 停摆触发,`(acct-a, bitget-spot)` 每个应答延迟 1 s
- 步骤:
  1. 触发后 500 ms,ops_bot 发 `cancel`,`account acct-a`
- 期望:
  - `Accepted` 引用正在进行的轮次;该目标同一时刻至多一个在途请求(假交易所的请求区间不重叠);轮次结束后 ops_bot 收到 `Done`,其 `targets` 来自同一轮
- 实现:`apps/kill-switch/tests/fire.rs::ks_077_merge_into_running_round`

### KS-078 actions.jsonl 记录每个请求与结果,不含签名与 key

- 规则:[撤单动作](README.md#撤单动作)
- 级别:unit
- 里程碑:M5
- 前置:K0;凭证的 key 为 `KS-KEY-XYZ`、secret 为 `ks-test-secret`
- 步骤:
  1. 触发一轮 4 个目标的撤单
  2. 读 `<state>/actions.jsonl`
- 期望:
  - 每个 REST 请求一行(目标、方法、不含签名参数的路径、状态码、耗时)、每个目标结果一行、整体结果一行
  - 文件中不出现 `KS-KEY-XYZ`、`ks-test-secret`、`signature=` 与 `OK-ACCESS-SIGN`
- 实现:`apps/kill-switch/tests/fire.rs::ks_078_actions_log`

### KS-079 动作开始、每个目标结果与整体结果各发一条告警

- 规则:[撤单动作](README.md#撤单动作)
- 级别:unit
- 里程碑:M5
- 前置:K0
- 步骤:
  1. `prod` 停摆触发,4 个目标都核验为 0
- 期望:
  - 告警接收端恰好收到 6 条:开始 1 条、每个目标 1 条(4 条)、整体结果 1 条
- 实现:`apps/kill-switch/tests/fire.rs::ks_079_alerts_per_round`

### KS-080 任何路径都不下单、不平仓

- 规则:[撤单动作](README.md#撤单动作)、[概要](README.md#概要)
- 级别:prop
- 里程碑:M5
- 性质:心跳触发、`fire`、本地 `cancel` 与 `stop_core` 的任意组合下,假交易所收到的请求只属于 {查挂单、撤单、账户信息(UID)、`/time`、保温 GET};没有下单、改单、平仓、划转或修改设置的端点
- 生成:K0;每个样本 1–5 次触发,随机注入 429/5xx/超时/残留与心跳变化
- 参考:每个 venue 的端点白名单
- 例数:PR 300
- 实现:`apps/kill-switch/tests/fire.rs::ks_080_never_places_orders`

### KS-081 每 30 s 对每个 REST 基址发一个签名 GET 保温并充当自检

- 规则:[实现说明](README.md#实现说明)、[身份核验(UID)](README.md#身份核验uid)
- 级别:table
- 里程碑:M5
- 前置:K0;4 个 REST 基址
- 实现:`apps/kill-switch/tests/fire.rs::ks_081_keepalive_selftest`

| 行 | 时刻 / 应答 | 期望 |
|---|---|---|
| KS-081.1 | 启动后 30 s,全部 200 | 每个基址恰 1 个签名 GET;没有告警 |
| KS-081.2 | 60 s 时 `bitget-spot` 返回 401 | P0("selftest 失败",目标 `(acct-a, bitget-spot)`);进程继续运行,判活不变 |
| KS-081.3 | 启动后 29 900 ms | 尚无保温请求 |

### KS-082 启动时与各所时间比较,偏差超过 1 s 报 P1

- 规则:[实现说明](README.md#实现说明)
- 级别:table
- 里程碑:M5
- 前置:K0;各假交易所 `/time` 返回本地时间加列出的偏差
- 实现:`apps/kill-switch/tests/fire.rs::ks_082_clock_skew`

| 行 | 偏差 | 期望 |
|---|---|---|
| KS-082.1 | +1 000 ms | 没有 P1 |
| KS-082.2 | +1 001 ms | 一条 P1(venue、偏差) |
| KS-082.3 | −1 001 ms | 一条 P1 |
| KS-082.4 | +999 ms | 没有 P1 |

### KS-083 各所请求签名与测试向量一致

- 规则:[实现说明](README.md#实现说明)
- 级别:table
- 里程碑:M5
- 前置:secret `ks-test-secret`;时间戳 1790000000000 ms(ISO 形式 `2026-09-21T14:13:20.000Z`)
- 实现:`apps/kill-switch/tests/sign.rs::ks_083_signature_vectors`

| 行 | venue / 待签内容 | 期望签名 |
|---|---|---|
| KS-083.1 | Binance,查询串 `timestamp=1790000000000&recvWindow=5000` | 十六进制 `4d1bb023f0127d69efd6f33cfa06902b90abfd606773e48f39d9f7d7d8c00a95` |
| KS-083.2 | Binance,`symbol=BTCUSDT&timestamp=1790000000000&recvWindow=5000` | `1260ef107542255a12a0d21eacfd4a1232ef620c2effe28e0fc61047c25125e4` |
| KS-083.3 | OKX,`2026-09-21T14:13:20.000Z` + `GET` + `/api/v5/trade/orders-pending` | base64 `gmC7sRJQ6P4qD/9wSk8nFHgaX0y3d2lYyqBuoobN52Y=` |
| KS-083.4 | OKX,同时间戳 + `GET` + `/api/v5/account/config` | `YfQRgwQanQcBFpK6fXE8C7dn7ZNf5pb81gk0qgUXEnc=` |
| KS-083.5 | OKX,同时间戳 + `POST` + `/api/v5/trade/cancel-batch-orders` + 请求体 `[{"instId":"BTC-USDT","ordId":"1"}]` | `9rIHA16H9+PgUQVJd0uUunV7/pVfDkQ+RBHPmt1ppWQ=` |
| KS-083.6 | Bitget,`1790000000000` + `GET` + `/fixture/path?symbol=BTCUSDT`(签名函数的测试路径;真实端点待核实,方法:KS-150 在模拟盘上核对) | `gMtq4TF61vr3Mxqs12xajZlkWLjmW1uTFJsCNizxMrA=` |

### KS-090 触发套接字是 SOCK_SEQPACKET,不监听任何网络端口

- 规则:[本地触发(ops_bot)](README.md#本地触发ops_bot)
- 级别:unit
- 里程碑:M5
- 前置:K0
- 步骤:
  1. 启动后检查 `trigger.socket` 的类型
  2. 枚举本进程的 socket
- 期望:
  - `trigger.socket` 为 `SOCK_SEQPACKET` 的 Unix 域监听者
  - 本进程没有 TCP 或 UDP 监听 socket(出站的交易所连接除外)
- 实现:`apps/kill-switch/tests/trigger.rs::ks_090_seqpacket_only`

### KS-091 只认 ops_bot 的 uid:其他对端不读载荷即关闭、记审计、限频告警

- 规则:[本地触发(ops_bot)](README.md#本地触发ops_bot)
- 级别:table
- 里程碑:M5
- 前置:K0;对端凭证由测试注入;每个被拒连接都先写入一条合法的 `cancel` 请求
- 实现:`apps/kill-switch/tests/trigger.rs::ks_091_peer_uid`

| 行 | 对端 uid / pid / 时刻 | 期望 |
|---|---|---|
| KS-091.1 | 1100 / 777 / t | 读取并校验载荷,回 `Accepted` |
| KS-091.2 | 0(root)/ 778 / t | 不读载荷即关闭;`actions.jsonl` 一行 `trigger_denied`(uid 0、pid 778);一条 P1;0 个 REST 请求 |
| KS-091.3 | 1101 / 779 / t | 同 091.2 |
| KS-091.4 | 1101 / 779 / t + 599 s | 关闭并记 `trigger_denied`;没有新的 P1 |
| KS-091.5 | 1101 / 779 / t + 601 s | 关闭并记 `trigger_denied`;一条新的 P1 |

### KS-092 触发请求的校验与每个拒绝原因

- 规则:[本地触发(ops_bot)](README.md#本地触发ops_bot)
- 级别:table
- 里程碑:M5
- 前置:K0;对端 uid 1100;基准请求 `{"v":1,"req_id":"6f1c0000-0000-4000-8000-000000000001","action":"cancel","instance":"prod","account":"acct-a","issuer":"tg:1001","reason":"t"}`,每行只改一处;对"`acct-c`"的行,K0 另加 Live 账户 `acct-c`(只在 `binance-usdm`,有凭证)并 `exclude = [{account = "acct-c", venue = "binance-usdm"}]`
- 实现:`apps/kill-switch/tests/trigger.rs::ks_092_request_validation`

| 行 | 改动 | 期望 |
|---|---|---|
| KS-092.1 | 报文 1 025 字节(`reason` 外填充空白) | `Rejected{BadRequest}` |
| KS-092.2 | 报文恰 1 024 字节 | `Accepted` |
| KS-092.3 | 报文不是 UTF-8 | `Rejected{BadRequest}` |
| KS-092.4 | 报文不是 JSON | `Rejected{BadRequest}` |
| KS-092.5 | 缺 `req_id` | `Rejected{BadRequest}` |
| KS-092.6 | 缺 `issuer` | `Rejected{BadRequest}` |
| KS-092.7 | `"v":2` | `Rejected{BadRequest}` |
| KS-092.8 | `"v":"1"` | `Rejected{BadRequest}` |
| KS-092.9 | `"action":"nuke"` | `Rejected{BadRequest}` |
| KS-092.10 | `reason` 201 字节 | `Rejected{BadRequest}` |
| KS-092.11 | `reason` 200 字节 | `Accepted` |
| KS-092.12 | `"instance":"staging"` | `Rejected{UnknownInstance}` |
| KS-092.13 | `"action":"stop_core","instance":"sim"` | `Rejected{StopNotAllowed}` |
| KS-092.14 | `"action":"stop_core","instance":"*"` | `Rejected{StopNotAllowed}` |
| KS-092.15 | `"account":"acct-x"` | `Rejected{UnknownAccount}` |
| KS-092.16 | `"instance":"sim","account":"*"` | `Rejected{SimAccount}` |
| KS-092.17 | `"instance":"*","account":"sim-a"` | `Rejected{SimAccount}` |
| KS-092.18 | `"account":"acct-c"` | `Rejected{NoTarget}` |
| KS-092.19 | `"instance":"staging","action":"nuke"` | `Rejected{BadRequest}`(按校验顺序取第一个) |
| KS-092.20 | 第 1、3–10、12–19 行的每一行 | 0 个 REST 请求;0 次 `systemctl` 调用;一条告警;`actions.jsonl` 一行(含拒绝原因) |

### KS-093 缺 action 时按 cancel 处理

- 规则:[本地触发(ops_bot)](README.md#本地触发ops_bot)
- 级别:unit
- 里程碑:M5
- 前置:K0;对端 uid 1100
- 步骤:
  1. 发不含 `action` 的基准请求(`account acct-a`)
- 期望:
  - `Accepted{action = cancel, targets = [(acct-a, binance-usdm), (acct-a, bitget-spot)]}`;只撤这两个目标;0 次 `systemctl` 调用
- 实现:`apps/kill-switch/tests/trigger.rs::ks_093_default_cancel`

### KS-094 cancel 的回执:100 ms 内 Accepted,结束后 Done 字段齐全

- 规则:[本地触发(ops_bot)](README.md#本地触发ops_bot)、[延迟与资源契约](README.md#延迟与资源契约)
- 级别:unit
- 里程碑:M5
- 前置:K0;`prod` 心跳前进中;`acct-a` 每个目标 2 笔挂单
- 步骤:
  1. 发 `cancel`,`account acct-a`,记录各报文时刻
- 期望:
  - 收到请求后 ≤ 100 ms 回 `Accepted{req_id, action = cancel, targets = [(acct-a, binance-usdm), (acct-a, bitget-spot)]}`;收到请求后 ≤ 500 ms 发出第一个撤单请求
  - 结束后 `Done{req_id, action = cancel, targets = [{account acct-a, venue binance-usdm, before 2, remaining 0, ok true}, {… bitget-spot …}], core_alive = true, elapsed_ms}`;没有 `core_stop` 与 `marker` 键;随后关闭连接
- 实现:`apps/kill-switch/tests/trigger.rs::ks_094_cancel_receipts`

### KS-095 账户与实例为 * 时的范围

- 规则:[本地触发(ops_bot)](README.md#本地触发ops_bot)
- 级别:table
- 里程碑:M5
- 前置:K0;对端 uid 1100
- 实现:`apps/kill-switch/tests/trigger.rs::ks_095_scope_star`

| 行 | `instance` / `account` | 期望 `Accepted.targets` |
|---|---|---|
| KS-095.1 | `prod` / `*` | `T` 的 4 项 |
| KS-095.2 | `*` / `*` | `T` 的 4 项(`sim` 没有 Live 目标) |
| KS-095.3 | `*` / `acct-b` | `(acct-b, binance-spot)`、`(acct-b, bitget-usdt-futures)` |

### KS-096 core_alive 反映撤单轮次结束时心跳是否仍在前进

- 规则:[本地触发(ops_bot)](README.md#本地触发ops_bot)
- 级别:table
- 里程碑:M5
- 前置:K0;`cancel`,`account acct-a`
- 实现:`apps/kill-switch/tests/trigger.rs::ks_096_core_alive`

| 行 | `prod` 心跳 | 期望 |
|---|---|---|
| KS-096.1 | 全程前进 | `Done.core_alive = true` |
| KS-096.2 | 轮次结束前已停止 1 s | `Done.core_alive = false` |

### KS-097 同一 req_id 在 600 s 内重复返回已有结果、不重做

- 规则:[本地触发(ops_bot)](README.md#本地触发ops_bot)
- 级别:table
- 里程碑:M5
- 前置:K0;`trigger.dedupe_s = 600`;请求 `R` 在 t 首次收到并在 t + 2 s 完成
- 实现:`apps/kill-switch/tests/trigger.rs::ks_097_req_id_dedupe`

| 行 | 重发时刻 | 期望 |
|---|---|---|
| KS-097.1 | t + 1 s(原请求仍在进行) | 回同一轮的 `Accepted`,完成时收到相同的 `Done`;没有新的请求 |
| KS-097.2 | t + 599 s | 返回已有的 `Done`;0 个 REST 请求 |
| KS-097.3 | t + 600 s | 返回已有的 `Done`;0 个 REST 请求 |
| KS-097.4 | t + 601 s | 作为新请求执行一轮撤单 |

### KS-098 每次触发(含被拒)发一条告警

- 规则:[本地触发(ops_bot)](README.md#本地触发ops_bot)
- 级别:table
- 里程碑:M5
- 前置:K0;对端 uid 1100
- 实现:`apps/kill-switch/tests/trigger.rs::ks_098_trigger_alerts`

| 行 | 请求 | 期望告警 |
|---|---|---|
| KS-098.1 | `cancel`,`acct-a`,完成 | 一条,含 `issuer = tg:1001`、动作 `cancel`、范围 `acct-a`、结果、`core_alive` |
| KS-098.2 | `Rejected{UnknownAccount}` | 一条,含拒绝原因 |
| KS-098.3 | `stop_core`,`acct-a`,`core_stop = Stopped` | 一条,另含 `core_stop = Stopped` |

### KS-099 触发处理不阻塞判活

- 规则:[实现说明](README.md#实现说明)
- 级别:unit
- 里程碑:M5
- 前置:K0;`(acct-a, bitget-spot)` 的每个应答延迟 5 s
- 步骤:
  1. ops_bot 发 `cancel`,`account acct-a`
  2. 发出后 100 ms 起 `prod` 两信号都停
- 期望:
  - 判活 `poll` 仍每 100 ms 执行;步骤 2 起 2 000 ms 时 `prod` 进入 `Fire`;`(acct-a, bitget-spot)` 的心跳轮次并入进行中的轮次(不并发两轮)
- 实现:`apps/kill-switch/tests/trigger.rs::ks_099_trigger_does_not_block_watch`

### KS-110 stop_core 的成功路径:撤单 → 写标记 → 停机 → 等结果 → 复核轮

- 规则:[停止 hft-core(stop_core)](README.md#停止-hft-corestop_core)
- 级别:unit
- 里程碑:M5
- 前置:K0;`prod` 心跳前进;`SystemctlRunner` 返回 0;收到 `systemctl` 后 3 s 测试把 `prod` 心跳置 `Stopped`;请求 `stop_core`,`instance prod`、`account acct-a`
- 步骤:
  1. 发请求,记录操作日志(REST 请求、文件操作、`fsync`、`systemctl`)
- 期望:
  - 顺序:`acct-a` 两个目标的查挂单、撤单与核验完成 → 写 `stop-marker.json.new` → `fsync(文件)` → `rename` 为 `stop-marker.json` → `fsync(目录)` → 执行 `systemctl` → 观察到 `Stopped` → 对 `acct-a` 的两个目标再查挂单与核验一轮
  - `Done{action = stop_core, core_stop = Stopped, marker = true, core_alive = false, …}`
  - 判活状态机由 `Watching` 回到 `Idle`,没有 `Fire`
- 实现:`apps/kill-switch/tests/stop_core.rs::ks_110_happy_path_order`

### KS-111 停机标记的内容、权限与范围

- 规则:[停止 hft-core(stop_core)](README.md#停止-hft-corestop_core)
- 级别:table
- 里程碑:M5
- 前置:K0;`<tmp>/markers` 为空;请求的 `req_id = R1`,收到时 UTC `utc_ns = U`
- 实现:`apps/kill-switch/tests/stop_core.rs::ks_111_marker_content`

| 行 | `account` | 期望 |
|---|---|---|
| KS-111.1 | `acct-a` | `stop-marker.json` 内容为 `{"v":1,"req_id":"R1","scope":"acct-a","utc_ns":U}`;权限 `0660`;目录里没有残留的 `.new` |
| KS-111.2 | `*` | `"scope":"*"` |

### KS-112 systemctl 以固定路径与固定参数直接执行

- 规则:[停止 hft-core(stop_core)](README.md#停止-hft-corestop_core)、[实现说明](README.md#实现说明)
- 级别:unit
- 里程碑:M5
- 前置:K0;`SystemctlRunner` 记录调用
- 步骤:
  1. 发 `stop_core`,`account *`
- 期望:
  - 恰一次调用:程序 `/usr/bin/systemctl`、argv `["stop", "--no-block", "hft-core@prod.service"]`、环境变量为空、不经 shell、超时 10 s
- 实现:`apps/kill-switch/tests/stop_core.rs::ks_112_systemctl_fixed`

### KS-113 已有未消费的标记时合并范围

- 规则:[停止 hft-core(stop_core)](README.md#停止-hft-corestop_core)
- 级别:table
- 里程碑:M5
- 前置:K0;按行预置 `<tmp>/markers` 的内容;新请求 `req_id = R2`
- 实现:`apps/kill-switch/tests/stop_core.rs::ks_113_marker_merge`

| 行 | 已有文件 / 新请求的 `account` | 期望 `stop-marker.json` |
|---|---|---|
| KS-113.1 | `stop-marker.json{scope "acct-a", req_id R1}` / `acct-b` | `scope = "*"`、`req_id = R2` |
| KS-113.2 | `stop-marker.json{scope "acct-a"}` / `acct-a` | `scope = "acct-a"`、`req_id = R2` |
| KS-113.3 | `stop-marker.json{scope "*"}` / `acct-a` | `scope = "*"`、`req_id = R2` |
| KS-113.4 | 只有 `stop-marker.json.consumed{scope "acct-a"}` / `acct-b` | `scope = "acct-b"`;`.consumed` 不变 |
| KS-113.5 | 只有 `stop-marker.json.aborted{scope "acct-a"}` / `acct-b` | `scope = "acct-b"`;`.aborted` 不变 |
| KS-113.6 | 任一行 | `actions.jsonl` 记录原标记与新请求两者 |

### KS-114 标记写入失败时照常停机、marker = false 并报 P0

- 规则:[停止 hft-core(stop_core)](README.md#停止-hft-corestop_core)
- 级别:table
- 里程碑:M5
- 前置:K0;`SystemctlRunner` 返回 0,随后 `prod` 到 `Stopped`
- 实现:`apps/kill-switch/tests/stop_core.rs::ks_114_marker_write_failure`

| 行 | 注入 | 期望 |
|---|---|---|
| KS-114.1 | 标记目录不可写(`EACCES`) | 照常执行 `systemctl`;`Done.marker = false`、`core_stop = Stopped`;一条 P0;目录中没有 `stop-marker.json` |
| KS-114.2 | 写 `.new` 时 `ENOSPC` | 同上 |
| KS-114.3 | `rename` 失败 | 同上;原有的 `stop-marker.json`(若有)内容不变 |

### KS-115 生产构建的停机标记路径固定,不随 paths.state 改变

- 规则:[停止 hft-core(stop_core)](README.md#停止-hft-corestop_core)、[配置文件字段](README.md#配置文件字段)
- 级别:unit
- 里程碑:M5
- 前置:生产构建(不启用测试的 `MarkerDir` 注入点);`paths.state = /tmp/ks-state`
- 步骤:
  1. 读出编译进来的标记路径常量
  2. 检查生产构建里没有改写该路径的配置项或环境变量
- 期望:
  - 常量为 `/var/lib/hft/kill-switch/stop-marker.json`,作废名为 `stop-marker.json.aborted`;`paths.state` 只影响 `actions.jsonl` 与 `latch.json` 的位置
  - `MarkerDir` 注入点只在 `cfg(test)` 构建中存在
- 实现:`apps/kill-switch/tests/stop_core.rs::ks_115_marker_path_fixed`

### KS-116 core_stop 的每种结果

- 规则:[停止 hft-core(stop_core)](README.md#停止-hft-corestop_core)、[实现说明](README.md#实现说明)
- 级别:table
- 里程碑:M5
- 前置:K0;`trigger.stop_wait_s = 30`;`stop_core`,`account acct-a`;t = 0 为 `systemctl` 返回的时刻
- 实现:`apps/kill-switch/tests/stop_core.rs::ks_116_core_stop_outcomes`

| 行 | 注入 | 期望 |
|---|---|---|
| KS-116.1 | 退出 0;t = 3 s 时 `state = Stopped` | `core_stop = Stopped`、`marker = true` |
| KS-116.2 | 退出 0;t = 29 900 ms 时 `state = Stopped` | `core_stop = Stopped` |
| KS-116.3 | 退出 0;t = 20 s 时 `HkLine.pid` 的进程不存在而 `state = Draining` | `core_stop = Inactive`、`marker = true`(标记保留);判活状态机照常对该 (pid, boot_id) `Fire` |
| KS-116.4 | 退出 0;到 t = 30 000 ms 进程仍在且 `state ≠ Stopped` | `core_stop = Timeout`、`marker = false` |
| KS-116.5 | 退出 1,stderr `Failed to stop hft-core@prod.service: Access denied` | `core_stop = Failed`(`Denied` 未启用),不等待;`marker = false`;P0 详情含该 stderr 原文 |
| KS-116.6 | 退出 1,stderr `Interactive authentication required.` | `core_stop = Failed` |
| KS-116.7 | 退出 5,stderr `Unit hft-core@prod.service not loaded.` | `core_stop = Failed` |
| KS-116.8 | `systemctl` 10 001 ms 未返回 | 子进程被终止;`core_stop = Failed` |
| KS-116.9 | `systemctl` 9 999 ms 返回 0,随后 `Stopped` | `core_stop = Stopped` |

### KS-117 停机失败时把标记作废为 .aborted 并报 P0

- 规则:[停止 hft-core(stop_core)](README.md#停止-hft-corestop_core)
- 级别:table
- 里程碑:M5
- 前置:K0;`<tmp>/markers` 预置一份旧的 `stop-marker.json.aborted`(内容 `X`);本次 `stop_core` 合并了已有的 `scope "acct-b"` 标记(合并后 `"*"`)
- 实现:`apps/kill-switch/tests/stop_core.rs::ks_117_abort_marker`

| 行 | `core_stop` | 期望 |
|---|---|---|
| KS-117.1 | `Timeout` | `stop-marker.json` 被改名为 `stop-marker.json.aborted`(覆盖 `X`,新内容 `scope = "*"`),随后 `fsync(目录)`;目录中没有 `stop-marker.json`;`Done.marker = false`;一条 P0,含结果、`systemctl` 退出码与"停机标记已作废" |
| KS-117.2 | `Failed`(由 polkit 拒绝产生,行 KS-116.5 的注入) | 同上 |
| KS-117.3 | `Failed` | 同上 |
| KS-117.4 | `Stopped` 或 `Inactive` | 不作废;`stop-marker.json` 保留 |

### KS-118 停机结束或超时后总有一轮复核撤单

- 规则:[停止 hft-core(stop_core)](README.md#停止-hft-corestop_core)
- 级别:table
- 里程碑:M5
- 前置:K0;`stop_core`,`account acct-a`;首轮撤单核验为 0 之后、`systemctl` 之前,假交易所在 `(acct-a, bitget-spot)` 新出现 1 笔挂单
- 实现:`apps/kill-switch/tests/stop_core.rs::ks_118_second_round`

| 行 | `core_stop` | 期望 |
|---|---|---|
| KS-118.1 | `Stopped` | 复核轮撤掉新出现的 1 笔;`Done.targets` 中该目标 `remaining = 0` |
| KS-118.2 | `Inactive` | 同上 |
| KS-118.3 | `Timeout` | 同上 |
| KS-118.4 | `Failed`(由 polkit 拒绝产生) | 同上 |
| KS-118.5 | `Failed` | 同上 |

### KS-119 stop_core 从不启动单元、从不操作其他单元

- 规则:[停止 hft-core(stop_core)](README.md#停止-hft-corestop_core)
- 级别:prop
- 里程碑:M5
- 性质:任意 `stop_core` 场景(各种 `core_stop`、并入、写标记失败、请求 `instance` 取任意值)中,每次 `systemctl` 调用的 argv 都恰为 `["stop", "--no-block", "hft-core@prod.service"]`;没有 `start`、`restart`、`kill` 或其他单元名;`instance` 不是 `prod` 的请求 0 次调用
- 生成:K0;每个样本 1–4 个触发请求(`cancel`/`stop_core` 随机,`instance` 取 `prod`、`sim`、`*`),随机注入 `SystemctlRunner` 结果与心跳变化
- 参考:固定 argv
- 例数:PR 300
- 实现:`apps/kill-switch/tests/stop_core.rs::ks_119_never_starts_units`

### KS-120 除作废外 kill-switch 不删除或改名标记

- 规则:[停止 hft-core(stop_core)](README.md#停止-hft-corestop_core)
- 级别:prop
- 里程碑:M5
- 性质:标记目录上的文件操作 ⊆ {创建并写 `stop-marker.json.new`、`rename(.new → stop-marker.json)`、`rename(stop-marker.json → stop-marker.json.aborted)`(只在 `core_stop ∈ {Timeout, Denied, Failed}` 时)};从不 `unlink`;从不读写 `stop-marker.json.consumed`
- 生成:同 KS-119
- 参考:文件操作白名单
- 例数:PR 300
- 实现:`apps/kill-switch/tests/stop_core.rs::ks_120_marker_ops_whitelist`

### KS-121 同一时刻至多一个 stop_core,并入的请求扩大标记范围

- 规则:[实现说明](README.md#实现说明)、[停止 hft-core(stop_core)](README.md#停止-hft-corestop_core)
- 级别:unit
- 里程碑:M5
- 前置:K0;第一个 `stop_core`(`account acct-a`)已写标记并调用 `systemctl`,正在等结果
- 步骤:
  1. 第二个连接发 `stop_core`,`account acct-b`
  2. `prod` 到 `Stopped`
- 期望:
  - 第二个请求的 `Accepted` 引用进行中的那次;`systemctl` 总共调用 1 次
  - `stop-marker.json` 被改写为 `scope = "*"`
  - 复核轮覆盖 `T` 的 4 个目标;两个连接都收到 `Done{core_stop = Stopped, marker = true}`
- 实现:`apps/kill-switch/tests/stop_core.rs::ks_121_single_stop_core`

### KS-140 启动时核验全部目标的 UID 后才进入判活

- 规则:[身份核验(UID)](README.md#身份核验uid)
- 级别:unit
- 里程碑:M5
- 前置:K0;UID 应答都与 refdata 相同,每个延迟 300 ms;`prod` 心跳前进
- 步骤:
  1. 启动 `kill-switch run`
- 期望:
  - 4 个目标各 1 个账户信息请求;全部返回之前状态机没有任何迁移,之后 `prod` 进入 `Watching`
- 实现:`apps/kill-switch/tests/identity.rs::ks_140_verify_before_watch`

### KS-141 UID 不符报 P0(只显示末 4 位)并退出 2

- 规则:[身份核验(UID)](README.md#身份核验uid)
- 级别:unit
- 里程碑:M5
- 前置:K0;`(acct-a, binance-usdm)` 的 UID 应答为 `"100009999"`
- 步骤:
  1. 启动 `kill-switch run`
- 期望:
  - 一条 P0,含账户 `acct-a`、venue `binance-usdm`、`…0001` 与 `…9999`;告警正文与 `actions.jsonl` 中不出现完整的 `100000001` 与 `100009999`
  - 退出 2;0 个撤单请求
- 实现:`apps/kill-switch/tests/identity.rs::ks_141_uid_mismatch`

### KS-142 UID 查询失败:标 UidUnverified、P0、每 30 s 重试,期间照常撤单

- 规则:[身份核验(UID)](README.md#身份核验uid)、[已知限制与后续工作](README.md#已知限制与后续工作)
- 级别:table
- 里程碑:M5
- 前置:K0;`(acct-b, bitget-usdt-futures)` 的 UID 查询在启动时超时;t = 0 为启动完成时刻
- 实现:`apps/kill-switch/tests/identity.rs::ks_142_uid_unverified`

| 行 | 时刻 / 事件 | 期望 |
|---|---|---|
| KS-142.1 | t = 0 | 该目标 `UidUnverified`;一条 P0;进程不退出,`status` 列出该目标 |
| KS-142.2 | t = 29 900 ms | 尚未重试 |
| KS-142.3 | t = 30 000 ms,查询仍失败 | 重试 1 次;仍 `UidUnverified` |
| KS-142.4 | t = 60 000 ms,查询返回相同 UID | 状态清为已核验 |
| KS-142.5 | 142.1 之后 `prod` 停摆 | 该目标照常参与撤单 |
| KS-142.6 | t = 60 000 ms,查询返回不同 UID | P0 后退出 2 |

### KS-150 各所撤单、查挂单与 UID 接口在测试网/模拟盘上可用

- 规则:[各所死手能力与兜底](README.md#各所死手能力与兜底)、[身份核验(UID)](README.md#身份核验uid)、[开发备忘](README.md#开发备忘)
- 级别:testnet
- 里程碑:M6
- 环境:dev2 联调窗口;Binance 演示环境(现货与 USDⓈ-M)、OKX 模拟盘(现货与永续)、Bitget 模拟盘(现货与 USDT 永续)各一个测试账户,各配一把 kill-switch 专用 key
- 授权:`[需授权]`
- 步骤:
  1. 每个 venue 挂 2 笔远离市价的最小名义 PostOnly 单
  2. `kill-switch selftest`,再 `kill-switch fire --account <测试账户> --venue <v> --reason drill --yes`
  3. 记录每个请求的端点、权重/限速应答与返回字段
- 通过标准:每个 venue 的 UID 字段读得到并与登记值一致(Binance USDⓈ-M key 能否调现货账户接口、OKX `uid` 而非 `mainUid`、Bitget UTA 的 `userId`);查挂单与撤单接口按 README 表执行后挂单为 0;结果回填各 `venue-*` README 开发备忘与本 README 的表
- 证据:`KS-150-<日期>.md`(端点、应答样本去除标识后的副本)
- 实现:`tests/host/KS-150.md::ks_150_testnet_endpoints`

### KS-151 有死手的交易所两道都执行,先后记录在案

- 规则:[各所死手能力与兜底](README.md#各所死手能力与兜底)
- 级别:testnet
- 里程碑:M6
- 环境:同 KS-150,只用 Binance USDⓈ-M 与 OKX 永续
- 授权:`[需授权]`
- 步骤:
  1. 指向测试网的 `hft-core@prod` 挂单并刷新死手
  2. `systemctl kill -s SIGSTOP hft-core@prod`,记录 T0
- 通过标准:挂单在 T0 + 5 s 内为 0;交易所死手撤单与 kill-switch 撤单的时刻都有记录(以交易所订单历史的撤单来源区分)
- 证据:kill 演练记录表一行与 `KS-151-<日期>.md`
- 实现:`tests/host/KS-151.md::ks_151_dms_and_switch`

### KS-160 心跳停止到首个撤单请求发出不超过 W + 200 ms

- 规则:[延迟与资源契约](README.md#延迟与资源契约)
- 级别:unit
- 里程碑:M5
- 前置:K0(`W = 2 000 ms`);假交易所应答延迟 0
- 步骤:
  1. t0 起两信号都停
- 期望:
  - 4 个目标的第一个撤单请求都在 t0 + 2 200 ms 之前发出
- 实现:`apps/kill-switch/tests/latency.rs::ks_160_first_cancel_bound`

### KS-161 stop_core 到达到 systemctl 发出不超过首轮核验完成 + 100 ms

- 规则:[延迟与资源契约](README.md#延迟与资源契约)
- 级别:unit
- 里程碑:M5
- 前置:K0;每次 `fsync` 注入 20 ms 延迟
- 步骤:
  1. 发 `stop_core`,`account acct-a`
- 期望:
  - `systemctl` 发出时刻 − 首轮最后一个目标核验完成时刻 ≤ 100 ms(含两次 `fsync` 共 40 ms)
- 实现:`apps/kill-switch/tests/latency.rs::ks_161_stop_core_latency`

### KS-162 常驻资源与单元资源设置

- 规则:[延迟与资源契约](README.md#延迟与资源契约)
- 级别:host
- 里程碑:M5
- 环境:dev2,`hft-kill-switch.service` 以 deploy 生成的单元运行,监视 `hft-core@sim`
- 授权:无
- 步骤:
  1. `systemctl show hft-kill-switch -p CPUWeight -p OOMScoreAdjust -p MemoryMax -p Slice`
  2. 用 `pidstat 1 600` 采 10 min CPU 占用
- 通过标准:`CPUWeight=1000`、`OOMScoreAdjust=-900`、`MemoryMax` 等于 hft-core 内存表登记值、`Slice=system.slice`;10 min 平均 CPU < 1% 核
- 证据:`KS-162-<日期>.txt`
- 实现:`tests/host/KS-162.sh::ks_162_resources`

### KS-170 触发套接字与停机标记目录在主机上的属主与权限

- 规则:[本地触发(ops_bot)](README.md#本地触发ops_bot)、[停止 hft-core(stop_core)](README.md#停止-hft-corestop_core)
- 级别:host
- 里程碑:M5
- 环境:dev2,deploy 生成的单元
- 授权:无
- 步骤:
  1. `stat -c '%U:%G %a' /run/hft/kill-switch/trigger.sock /var/lib/hft/kill-switch`
  2. 以 `hft-opsbot` 与另一个普通用户各连一次触发套接字
  3. 触发一次 `stop_core`(测试网实例)后 `stat` 标记文件
- 通过标准:套接字 `hft-ks:hft-ks-trigger 660`、目录 `hft-ks:hft-stopmark 2770`;`hft-opsbot` 能连接,普通用户得到 `EACCES`;标记文件 `hft-ks:hft-stopmark 660`
- 证据:`KS-170-<日期>.txt`
- 实现:`tests/host/KS-170.sh::ks_170_socket_and_marker_perms`

### KS-171 deploy 的加固单元下经 systemctl 停 hft-core@prod,越权被拒

- 规则:[停止 hft-core(stop_core)](README.md#停止-hft-corestop_core)、[开发备忘](README.md#开发备忘)
- 级别:host
- 里程碑:M5
- 环境:dev2 的 systemd 容器测试,`hft-ks` 用户,单元带 deploy 的加固集合(`NoNewPrivileges`、`ProtectSystem=strict`、`RestrictAddressFamilies`)
- 授权:`[需授权]`
- 步骤:
  1. 以 ops_bot 身份发 `stop_core`(`hft-core@prod` 为测试网实例)
  2. 在同一单元环境里以 `hft-ks` 执行 `/usr/bin/systemctl stop --no-block hft-core@sim.service`
- 通过标准:步骤 1 `core_stop = Stopped`;步骤 2 被 polkit 拒绝,记录其退出码与 stderr 原文;该文本即启用 `Denied` 的依据(启用时同一 PR 改 README 停机调用一条与 KS-116.5/6 的期望)
- 证据:`KS-171-<日期>.txt`
- 实现:`tests/host/KS-171.sh::ks_171_polkit_under_hardening`

### KS-172 干净 Drain 后热段被删除,kill-switch 解除映射,tmpfs 已用量回落

- 规则:[判活与触发](README.md#判活与触发)
- 级别:host
- 里程碑:M5
- 环境:dev2,`hft-core@sim` 与 kill-switch 以 deploy 单元运行
- 授权:`[需授权]`
- 步骤:
  1. 记录 `/dev/shm/hft/sim` 的 tmpfs 已用量
  2. `systemctl stop hft-core@sim`(Drain 到 `Stopped`,`hft-shm-release` 删除热段)
  3. 2 s 后再记录已用量,并查 kill-switch 的 `/proc/<pid>/maps`
- 通过标准:步骤 3 已用量回落到 sim 启动前的水平;`maps` 中没有已删除的热段文件
- 证据:`KS-172-<日期>.txt`
- 实现:`tests/host/KS-172.sh::ks_172_tmpfs_released`

### KS-173 kill -9、kill -STOP 触发撤单,干净停止不动作

- 规则:[判活与触发](README.md#判活与触发)、[延迟与资源契约](README.md#延迟与资源契约)
- 级别:testnet
- 里程碑:M6
- 环境:dev2 联调窗口,`hft-core@prod`(测试网账户)挂单中
- 授权:`[需授权]`
- 步骤:
  1. `kill -9` hft-core,记录 T0,观察挂单
  2. 重启后 `kill -STOP`,记录 T0
  3. 重启后 `systemctl stop hft-core@prod`
- 通过标准:步骤 1、2 全部挂单在 T0 + 5 s 内为 0;步骤 3 kill-switch 没有任何撤单请求(状态 `Stopped` → `Idle`)
- 证据:kill 演练记录表三行
- 实现:`tests/host/KS-173.md::ks_173_e2e_signals`

### KS-174 dev2 护栏二级动作停 hft-core 且 Drain 超时时 kill-switch 撤单

- 规则:[判活与触发](README.md#判活与触发)
- 级别:drill
- 里程碑:M6
- 环境:dev2 联调窗口,`hft-core@prod`(测试网)挂单中,护栏阈值临时调低以触发二级动作
- 授权:`[需授权]`
- 步骤:按 [incident-response P](../../docs/runbooks/incident-response.md#p-dev2-护栏触发) 的演练方式触发护栏二级动作,并让 hft-core 的 Drain 卡住直到 `TimeoutStopSec`
- 通过标准:hft-core 被终止后 kill-switch 在 `W` + 200 ms 内开始撤单,全部挂单为 0;kill-switch 自身未被护栏停止(单元仍 active)
- 证据:kill 演练记录表一行
- 实现:`tests/host/KS-174.md::ks_174_guard_stop_drain_timeout`

### KS-190 测试网完整演练:停摆、撤单与回到判活

- 规则:[判活与触发](README.md#判活与触发)、[撤单动作](README.md#撤单动作)
- 级别:drill
- 里程碑:M6
- 门槛:⑪
- 环境:dev2 联调窗口;Binance 演示环境(现货与 USDⓈ-M)、OKX 模拟盘、Bitget 模拟盘(现货与永续)的测试账户,经指向测试网的 `hft-core@prod`
- 授权:`[需授权]`
- 步骤:按 [incident-response kill 演练](../../docs/runbooks/incident-response.md#kill-演练):① 各所各挂 2 笔远离市价的最小名义 PostOnly 单;② 记录 T0,`systemctl kill -s SIGSTOP hft-core@prod`;③ 观察告警时间线,在交易所确认全部挂单消失并记录每所撤单完成时间;④ `SIGCONT` 或重启,确认 kill-switch 在新 (pid, boot_id) 以 `Warmup` 前进时回到 `Watching`;⑤ 再做一次 `SIGKILL` 与一次干净 `systemctl stop hft-core@prod`
- 通过标准:②、⑤ 的 `SIGKILL` 全部挂单在 T0 + 5 s 内撤销;有死手的 venue 的两道先后都有记录;④ 回到 `Watching`;⑤ 的干净停止无动作
- 证据:incident-response kill 演练记录表
- 实现:`tests/host/KS-190.md::ks_190_testnet_drill`

### KS-191 控制面不可达时经本地触发的 stop_core 演练

- 规则:[停止 hft-core(stop_core)](README.md#停止-hft-corestop_core)、[本地触发(ops_bot)](README.md#本地触发ops_bot)
- 级别:drill
- 里程碑:M6
- 门槛:⑪
- 环境:同 KS-190;演练用 ops_bot 配置中的 ctl socket 路径指向不存在的文件
- 授权:`[需授权]`
- 步骤:按 [incident-response kill 演练](../../docs/runbooks/incident-response.md#kill-演练) 第 7 步:hft-core 运行中并挂单;老板 `/stop <账户>` 并确认;之后恢复 socket 路径并启动 `hft-core@prod`;再做一次 hft-core 已 `SIGSTOP` 的变体
- 通过标准:ops_bot 发出 `stop_core`;kill-switch 只撤该账户挂单;`systemctl` 之前已写下 `stop-marker.json`;`hft-core@prod` 收到 SIGTERM 并 Drain 到 `Stopped`;`Done.core_stop = Stopped`、`marker = true`;P0 到达值班交易员;重启后由标记写入 `Kill`、标记改名 `.consumed`、闸门放行后仍不开仓(hft-core 侧);`SIGSTOP` 变体 `core_stop = Inactive` 且判活状态机同时 `Fire`;全部撤单在触发后 5 s 内完成
- 证据:kill 演练记录表一行与 `KS-191-<日期>.md`
- 实现:`tests/host/KS-191.md::ks_191_stop_core_drill`

### KS-192 正式账户的零成交撤单冒烟

- 规则:[撤单动作](README.md#撤单动作)、[各所死手能力与兜底](README.md#各所死手能力与兜底)
- 级别:drill
- 里程碑:M8
- 环境:升级主机,正式 `hft-core@prod`,维护窗口内,该实例先 `/mode … closeonly`
- 授权:`[需授权]`
- 步骤:按 [launch D-smoke](../../docs/runbooks/launch.md#d-正式实盘m8) 的第 1、3、4 小步:Admin 发 `SmokeOrder` 挂出冒烟单;`kill-switch fire --account <a> --venue <v> --reason smoke --yes` 撤掉;核验为 0;另做 `kill-switch selftest`;恢复原模式
- 通过标准:冒烟单被 kill-switch 撤销且核验为 0;下一轮对账保持 `Ignore`;`selftest` 通过;全程没有对正式实例做 `SIGSTOP`、`SIGKILL`、flatten 或 HARD
- 证据:kill 演练记录表一行(正式账户零成交冒烟)
- 实现:`tests/host/KS-192.md::ks_192_prod_smoke`


## 验收

- 本 app 在 M5 完成的标准:里程碑为 M5 的用例全部有同编号的实现且通过([docs/tests §1](../../docs/tests/README.md#1-地位));M6 的测试网与演练用例在联调窗口完成并留证据。
- 属性测试(KS-056、KS-080、KS-119、KS-120):PR 按各用例的例数;CI 每夜以 100 倍例数运行一次。
- 延迟契约的演练项(心跳停止 → 全部目标挂单为 0 ≤ 5 s 等)以 KS-151、KS-173、KS-190、KS-191 的演练记录为证据;数值只在 [延迟与资源契约](README.md#延迟与资源契约) 登记。
- 上线门槛 ⑪ 的 kill 演练([testing §8](../../docs/testing.md#8-上线门槛)):KS-190、KS-191 的记录齐全;正式实盘之后每月一次 KS-192(演练范围的唯一归属是 [incident-response kill 演练](../../docs/runbooks/incident-response.md#kill-演练))。
