---
component: watchtower
prefix: WT
spec: README.md
---

# watchtower 测试规格

格式与规则见 [docs/tests](../../docs/tests/README.md)。规则的判定函数(阈值两侧、持续计时、同机干扰的 bootstrap)在 [hft-telemetry](../../crates/hft-telemetry/TESTS.md) 验证;本文件验证本 app 的装配:输入与游标、各规则在 watchtower 中的触发、恢复与级别、`/healthz`、日报、投递与告警记录的生命周期、配置与权限。用例在 `前置` 中显式给出所用的取值。

## 覆盖

| README 小节 | 用例 |
|---|---|
| 概要 | WT-133、WT-165、WT-195 |
| 目录 | 不适用:非规范 |
| 使用本组件 | WT-001–WT-006、WT-146 |
| 输入 | WT-010–WT-022、WT-039、WT-122、WT-202、WT-203、WT-204 |
| 直方图与 Prometheus 导出 | WT-030–WT-038、WT-063 |
| 机外健康检查(/healthz) | WT-050–WT-063、WT-187、WT-198、WT-199、WT-203 |
| 健康规则 | WT-070–WT-122、WT-196、WT-197、WT-200、WT-204–WT-206 |
| 日报 | WT-005、WT-130–WT-146、WT-188、WT-201 |
| 告警分级与投递 | WT-071、WT-075、WT-150–WT-162、WT-201 |
| 降级命令(可选扩展) | WT-002、WT-081、WT-165–WT-167 |
| 配置文件字段 | WT-001、WT-002、WT-036、WT-121、WT-170、WT-171、WT-207 |
| 实现说明 | WT-019–WT-022、WT-031、WT-037、WT-038、WT-074、WT-078、WT-097、WT-175–WT-178 |
| 延迟与资源契约 | WT-036、WT-185–WT-189 |
| 测试与验收 | 不适用:非规范 |
| 依赖 | WT-017、WT-167、WT-190 |
| 延伸阅读 | 不适用:非规范 |
| 已知限制与后续工作 | 不适用:非规范 |
| 开发备忘 | 不适用:非规范 |

## 用例

**约定**(用例只写与这里不同的部分;所写数值都是该用例的显式前置,不依赖实现的缺省值):

- **测试台 `WtHarness`**(`apps/watchtower/tests/common/mod.rs`):在进程内装配 watchtower 的全部线程,替换四个外部面——① journal:以 hft-testkit 的 journal 构造器在临时目录为每个实例写 seq journal(记录头时间由用例给出,段按用例滚动);② 时钟:假 UTC 时钟,由用例推进;③ 主机:假 `/proc`、cgroup 文件、`statvfs`、`chronyc`、`ETHTOOL_GSTATS`、systemd(每个单元的 `ActiveState` 由用例设定)、`HeartbeatBlock` 文件、标志文件与 `fatal.json`,都在临时根目录下;`host-probe` 每 1 s(假时钟)采样一次;④ 投递:假 Telegram、Bark 与 Grafana 端点,记录收到的每条消息(级别、规则、实例与账户标签、正文、时刻)。"投递一条 P1" = 假 Telegram 端点收到一条该规则的 P1 消息;"无告警" = 截至断言时刻假端点没有收到该规则的消息、`<state>/alerts.jsonl` 没有该规则的记录;"P2" = 假 Grafana 端点收到一条注释、Telegram 与 Bark 都没有收到。
- **配置 W0**:`schema = 1`;`instances = [{name = "prod", …}, {name = "sim", …}]`,两个实例的 `hft_core_config` 夹具都写 `watchdog.hb_stale_ms = 1000`;refdata(经各实例 journal 引导块的 `ConfigBlob`):`prod` 含账户 1(`kind = Live`、`is_prod = true`、label `acct-a`)与账户 2(`kind = Live`、`is_prod = false`、label `tn-b`),`sim` 含账户 11(`kind = Sim`、label `sim-c`);核心规则全部 `enabled = true`,可选扩展规则全部 `enabled = false`,全部 `actions_enabled = false`;`rules.FeedSilent.silent_ms = {binance-usdm = 2000, okx-swap = 5000, bitget-usdt-futures = 5000}`;`UnknownAge` 阈值 10 s;`rules.MarkDivergence.mark_div_bp = 50`、`.hold_s = 30`;`naked_usd = {1 = 5, 2 = 5, 11 = 5}`;`recon_warn_usd = {1 = 50, 2 = 50}`;`recon_p0_usd = {1 = 500, 2 = 500}`;`rules.FundingDiff.funding_diff_alert = {abs = 1 USDT, rel = 1%}`;`rules.FundingStale.funding_stale_s = 300`;`rules.ProcessDown.p0_fatal_codes = ["UidMismatch"]`;`PersistLag` 三线 50 ms / 1 s / 10 s;`healthz.recon_max_age_s = 90`;`paths.hostprofile` = 临时根下的 `hostprofile.json` 夹具,缺省不含 ENI-A、ENI-B、ENI-C 角色(dev2 形态;下文"主机剖面 X"指在夹具中加入该项);`paths.pipeline_state`、`paths.reconciler_status` 在临时根下;`alert.repeat_s = {p0 = 60, p1 = 600}`;`hist.max_series = 20000`、`hist.mem_frac = 0.5`;`calib_pairs = []`;`host.maint_file` 在临时根下;`HostPressure`、`GuardAction` 为 `enabled = false` 且不设 `host.guard_config`(dev2 用例另设)。
- **策略目录 S0**:hft-testkit 测试策略 `probe-taker`(`kind_id = 65280`)经 `hft-core schemas --out` 导出的 `index.json` 与 schema 文件;`notes` 描述符:码 1 `edge`(P1,`v[0]` = edge)、码 2 `info_tick`(Info)、码 3 `slow_path`(P2);`report` 描述符:`edges`(Count, Instance, 来源码 1)、`edge_sum`(Sum, Group, 码 1, `v_ix = 0`)、`edge_last`(Last, Instance)、`edge_max`(Max, Group)、`edge_p99`(P99, Instance)。`prod` 的策略实例 3 与 `sim` 的策略实例 13 都是该种类(各自 journal 有 `StrategyParams` 头)。
- **时刻**:`T0 = 2026-11-02T00:00:00Z`(Unix 秒 1793577600);"T0 + 5 s"按假时钟。窗口型规则在 `rules` 线程按 UTC 对齐的 10 s 节拍评估(T0 + 10 s、T0 + 20 s …);事件型规则在读到记录时判定。`prod`、`sim` 的 `HeartbeatBlock.state` 缺省为 `Running`、区 0 计数每 100 ms 前进;**心跳年龄** = 评估时刻 − `host-probe` 最近一次采样到该计数前进的时刻。

### WT-001 --check 对完整配置退出 0,不读 journal、不连接

- 规则:[使用本组件](README.md#使用本组件)、[配置文件字段](README.md#配置文件字段)
- 级别:proc
- 里程碑:M5
- 前置:Linux;W0 与 S0 写在临时根下;`strace -f -e trace=openat,connect,bind,socket`
- 步骤:
  1. 运行 `watchtower --check --config <W0>`
- 期望:
  - 退出码 0,stderr 为空
  - strace 中没有对两个实例 journal 目录、`heartbeat_shm`、`paths.replay_reports`、`paths.verify_runs` 下任何文件的 `openat`;没有 `connect`、`bind`
  - S0 的 `index.json` 与 `65280-probe-taker.schema.json` 各被 `openat` 至少一次(摘要校验)
- 实现:`apps/watchtower/tests/proc/cli.rs::wt_001_check_ok_no_io`

### WT-002 --check 收集全部配置错误后一次打印并退出 2

- 规则:[配置文件字段](README.md#配置文件字段)、[降级命令(可选扩展)](README.md#降级命令可选扩展)、[健康规则](README.md#健康规则)、[输入](README.md#输入)
- 级别:table
- 里程碑:M5
- 前置:以 W0 为底,每行只做所列改动后运行 `watchtower --check --config <cfg>`
- 实现:`apps/watchtower/tests/config.rs::wt_002_check_errors`

| 行 | 改动 | 期望 |
|---|---|---|
| WT-002.1 | 删掉 `rules.MarkDivergence.mark_div_bp` | 退出 2;一行错误,指明 `rules.MarkDivergence.mark_div_bp` 缺失 |
| WT-002.2 | 删掉 `rules.FeedSilent.enabled` | 退出 2;错误指明 `rules.FeedSilent.enabled` |
| WT-002.3 | `rules.FeedSilent.actions_enabled = true` | 退出 2;错误指明该键必须为 `false` |
| WT-002.4 | 删掉 `rules.SimDrift.actions_enabled` | 退出 2 |
| WT-002.5 | 删掉 `interference.max_halfwidth` | 退出 2;错误指明 `interference.max_halfwidth` |
| WT-002.6 | S0 的 `index.json` 中 `probe-taker` 摘要改动一个十六进制位 | 退出 2;错误含 `DigestMismatch` 与文件名 |
| WT-002.7 | `rules.HostPressure.enabled = true`,不设 `host.guard_config` | 退出 2;错误指明 `host.guard_config` |
| WT-002.8 | `instances[0].name = "test"` | 退出 2 |
| WT-002.9 | 两个 `instances` 都叫 `prod` | 退出 2 |
| WT-002.10 | `schema = 2` | 退出 2 |
| WT-002.11 | 加未知键 `rules.MarginStress.liq_dist_reduce = 0.15` | 退出 2(未知键;本 app 没有强平距离阈值) |
| WT-002.12 | `instances[0].hft_core_config` 指向不存在的文件 | 退出 2 |
| WT-002.13 | 删掉 `budgets` | 退出 2 |
| WT-002.14 | 同时做 .1、.3、.8 三处改动 | 退出 2;stderr 恰好三行错误,依次对应三处改动在配置文件中的出现顺序 |

### WT-003 rules 子命令打印配置中的阈值

- 规则:[使用本组件](README.md#使用本组件)、[健康规则](README.md#健康规则)
- 级别:unit
- 前置:W0;另一份 W0',只把 `rules.FeedSilent.silent_ms.binance-usdm` 设为 2500
- 里程碑:M5
- 步骤:
  1. 对 W0 运行 `watchtower rules --config <W0>`
  2. 对 W0' 运行同一命令
- 期望:
  - 两次输出的规则名集合都等于 hft-telemetry `rules` 模块注册的规则名集合(核心与可选扩展),每条一行
  - 步骤 1 的 `FeedSilent` 行含 `binance-usdm = 2000`,步骤 2 的含 `binance-usdm = 2500`;其余行两次逐字节相同
  - `DualFeedGap`、`ClockOffset`、`ShadowDivergence` 行显示 `enabled = false`
- 实现:`apps/watchtower/tests/cli.rs::wt_003_rules_prints_config`

### WT-004 replay 子命令重跑一小时的命中与生产告警记录一致,不投递、不写状态

- 规则:[使用本组件](README.md#使用本组件)、[实现说明](README.md#实现说明)
- 级别:integ
- 里程碑:M5
- 前置:W0;`prod` journal 的 T0 小时内依次有:`Note{CapSet}`(cap `Recon`,账户 1、`binance-usdm`)、`Note{ExternalFill}`(cause `Liquidation`)、`Note{FundingCorrected}`、每 10 s 一条 `Note{AdvisorLag}`(T0 + 600 s 起 2 min)、三个 venue 的 `Mark` 使资产 AIN 在 `okx-swap` 上偏离 60 bp 持续 60 s;`WtHarness` 以 `run` 形态尾随该小时并产出 `alerts.jsonl`
- 步骤:
  1. 记下 `paths.state` 下全部文件的内容哈希
  2. 运行 `watchtower replay --config <W0> --journal <prod journal 目录> --hour 2026-11-02T00`
- 期望:
  - 输出的命中列表(规则、scope、级别、首次命中时刻)与步骤 1 前 `alerts.jsonl` 中首次命中落在该小时的记录逐项相等,共 5 条(`ReconcileMismatch` P0、`ExternalFill` P0、`FundingDiff` P1、`AdvisorLag` P1、`MarkDivergence` P1)
  - 运行期间假投递端收到 0 条消息;`paths.state` 下全部文件的哈希与步骤 1 相同
- 实现:`apps/watchtower/tests/replay.rs::wt_004_replay_matches_production`

### WT-005 daily 子命令离线重算的日报与生产日报逐字段相等

- 规则:[使用本组件](README.md#使用本组件)、[日报](README.md#日报)
- 级别:integ
- 里程碑:M5
- 前置:W0 + S0;两个实例 2026-11-02 全天夹具 journal(含成交、`LatencySample`、资金费、策略 Note、`ReconcileReport`);`WtHarness` 以 `run` 形态在 2026-11-03 00:05 写出生产日报 `daily/2026-11-02.json`
- 步骤:
  1. 运行 `watchtower daily --config <W0> --journal <夹具> --date 2026-11-02`
- 期望:
  - 输出 JSON 与生产日报逐字段相等(含 `latency`、`accounts`、`strategies`、`funding`、`calibration`、`alerts`、`gates` 七节);运行期间不写 `paths.state`
- 实现:`apps/watchtower/tests/daily.rs::wt_005_daily_offline_equals_production`

### WT-006 interference 不带 --instance 时读 prod 的按小时直方图

- 规则:[使用本组件](README.md#使用本组件)、[日报](README.md#日报)
- 级别:unit
- 里程碑:M5
- 前置:`<state>/hourly/` 中有 `prod` 与 `sim` 两个实例 7 天的按小时直方图,两者样本不同;`interference.*` 按 [testing §8.2](../../docs/testing.md#82-延迟验收的测法) 填写(`block_hours = 2`、`min_n_each_side = 10000`、`max_ratio_ub = 1.05`、`max_halfwidth = 0.03`、`conf = 0.95`、`resamples = 2000`、`hard_stages = [tick_to_trade, rep_to_order]`、`funding_hours_utc = [0, 8, 16]`)
- 步骤:
  1. `watchtower interference --config … --from 2026-11-02 --to 2026-11-08 --seed 42`
  2. 同上另加 `--instance prod`
- 期望:
  - 两次输出逐字节相同;每组的 `n_on + n_off` 等于 `prod` 直方图的样本数,与 `sim` 的样本数无关
- 实现:`apps/watchtower/tests/interference.rs::wt_006_default_instance_prod`

### WT-010 重启后从游标续读,已投递的告警不重复

- 规则:[输入](README.md#输入)
- 级别:integ
- 里程碑:M5
- 前置:W0;`prod` journal 有 5 条 `Note{ExternalFill}`(cause `Adl`,seq 分别为 101、205、309、412、530)
- 步骤:
  1. 尾随到 seq 320 后让 watchtower 以 SIGTERM 退出
  2. 重新启动,尾随到 journal 末尾
- 期望:
  - `alerts.jsonl` 中 `ExternalFill` 记录恰好 5 条,scope 中的 seq 两两不同;假 Telegram 端点收到的 `ExternalFill` 首发消息恰好 5 条(不计 60 s 重复)
  - 步骤 2 结束后 `cursor.json` 中 `prod` 的 `last_seq` = 该 journal 最后一条记录的 seq
- 实现:`apps/watchtower/tests/cursor.rs::wt_010_resume_from_cursor`

### WT-011 游标按实例记录段号、首 seq、段头哈希与最后 seq

- 规则:[输入](README.md#输入)
- 级别:unit
- 里程碑:M5
- 前置:W0;`prod` journal 有段 1(`first_seq = 1`)、段 2(`first_seq = 1001`);`sim` 只有段 1
- 步骤:
  1. 尾随到 `prod` seq 1500、`sim` seq 40 后读 `cursor.json`
- 期望:
  - `prod` 项 = `{segment_no = 2, first_seq = 1001, header_sha256 = 段 2 的段头 0..3072 的 SHA-256, last_seq = 1500}`;`sim` 项 = `{segment_no = 1, first_seq = 1, header_sha256 = sim 段 1 的段头哈希, last_seq = 40}`
- 实现:`apps/watchtower/tests/cursor.rs::wt_011_cursor_fields`

### WT-012 游标所指的段不存在或段头哈希不符时从最新段起点重新尾随并报 P2

- 规则:[输入](README.md#输入)
- 级别:table
- 里程碑:M5
- 前置:W0;`cursor.json` 中 `prod` 指向段 7(`last_seq = 50000`);watchtower 停止期间按行改动 `prod` journal,当前最新段为段 9(`first_seq = 52001`),`sim` 游标有效;重启后在 T0 + 10 min 断言
- 实现:`apps/watchtower/tests/cursor.rs::wt_012_cursor_segment_missing`

| 行 | 改动 | 期望 |
|---|---|---|
| WT-012.1 | 删除段 7 的冷副本与热段 | `prod` 从段 9 的第一条记录起尾随;一条 P2,规则 `CursorReset`,scope `prod`;当日日报 `prod` 节标 `incomplete = true`;`sim` 节不标 |
| WT-012.2 | 段 7 换成另一 journal 的同号段(段头 SHA-256 不同) | 同 .1 |
| WT-012.3 | 不改动 | 从段 7 的 seq 50001 续读;无 `CursorReset` |

### WT-013 新 journal 的 seq 小于旧游标时不按旧 last_seq 跳过

- 规则:[输入](README.md#输入)
- 级别:integ
- 里程碑:M5
- 前置:W0;`cursor.json` 中 `prod` 指向旧 journal 段 40(`last_seq = 4000000`);`prod` 的 journal 根被整体替换为新 journal(段 1、2,seq 1..2000,含 3 条 `Note{ExternalFill}`,seq 50、900、1800);`sim` 游标指向其段 3 的 seq 7000,`sim` journal 未变,之后另有 200 条记录
- 步骤:
  1. 不移动 `cursor.json`,重启 watchtower,尾随到两个 journal 末尾
- 期望:
  - `prod` 从新 journal 最新段(段 2)的第一条记录起尾随:seq 1800 的 `ExternalFill` 产生 P0,seq 50 与 900(段 1 内)不产生;一条 `CursorReset` P2
  - `sim` 从 seq 7001 续读,处理的记录数恰好 200,无 `CursorReset`
- 实现:`apps/watchtower/tests/cursor.rs::wt_013_journal_root_replaced`

### WT-014 cursor.json 整体缺失时每个实例从最新段起点尾随,当日日报标不完整

- 规则:[输入](README.md#输入)
- 级别:integ
- 里程碑:M5
- 前置:W0;`prod` journal 有段 1–3,段 3 内已有 2 条 `Note{ExternalFill}`(上次运行已告警,记录在 `alerts.jsonl`);`cursor.json` 被移走
- 步骤:
  1. 启动 watchtower,尾随到末尾
- 期望:
  - `prod` 与 `sim` 各从自己最新段的第一条记录起尾随(段 1、2 的记录不被读取)
  - 段 3 的 2 条 `ExternalFill` 再次产生告警记录(新 `id`,共 4 条);当日日报两个实例节都标 `incomplete = true`;各一条 `CursorReset` P2
- 实现:`apps/watchtower/tests/cursor.rs::wt_014_cursor_file_missing`

### WT-015 cursor.json 不可解析时按整体缺失处理

- 规则:[输入](README.md#输入)
- 级别:unit
- 前置:W0;`cursor.json` 内容为 `{"prod": {"segment_no": 3,`(截断)
- 里程碑:M5
- 步骤:
  1. 启动 watchtower
- 期望:
  - 进程不退出;行为与 WT-014 相同(两个实例从最新段起点尾随、各一条 `CursorReset` P2、日报标不完整);新的 `cursor.json` 可解析
- 实现:`apps/watchtower/tests/cursor.rs::wt_015_cursor_unparsable`

### WT-016 游标先写 .new 再 rename

- 规则:[输入](README.md#输入)
- 级别:proc
- 里程碑:M5
- 前置:Linux;W0;`strace -f -e trace=openat,rename,renameat,renameat2`
- 步骤:
  1. 运行 watchtower 30 s,期间 `prod` journal 持续追加
- 期望:
  - 每次游标更新的系统调用序列为打开 `<state>/cursor.json.new` 写入 → rename 到 `<state>/cursor.json`;没有以写方式直接打开 `cursor.json`
- 实现:`apps/watchtower/tests/proc/cursor.rs::wt_016_cursor_atomic_write`

### WT-017 journal 与心跳块只读映射

- 规则:[输入](README.md#输入)、[依赖](README.md#依赖)
- 级别:proc
- 前置:Linux;W0;由 hft-testkit 的写者进程持续写 `prod` 热段与心跳块;`strace -f -e trace=openat,mmap`
- 里程碑:M5
- 步骤:
  1. 运行 watchtower 10 s 后读 `/proc/<pid>/maps`
- 期望:
  - 热段、冷副本与 `heartbeat` 文件的映射权限都是 `r--s` 或 `r--p`;strace 中对这些路径的 `openat` 都带 `O_RDONLY`,没有 `O_RDWR`/`O_WRONLY`
- 实现:`apps/watchtower/tests/proc/readonly.rs::wt_017_readonly_mappings`

### WT-018 watchtower 被强杀不影响写者

- 规则:[输入](README.md#输入)
- 级别:proc
- 里程碑:M5
- 前置:Linux;hft-testkit 写者进程每 1 ms 追加并 `publish` 一个事件、每 100 ms 前进心跳计数;watchtower 尾随它
- 步骤:
  1. 运行 5 s 后 `kill -9` watchtower,再运行 5 s
- 期望:
  - 写者在 `kill -9` 前后 5 s 内各追加的事件数之差 ≤ 1%;写者没有收到 `SIGPIPE`、没有任何错误返回;心跳计数在 `kill -9` 之后继续前进
- 实现:`apps/watchtower/tests/proc/isolation.rs::wt_018_kill_does_not_affect_writer`

### WT-019 run 启动时策略目录摘要不符即退出 2

- 规则:[输入](README.md#输入)、[实现说明](README.md#实现说明)
- 级别:integ
- 里程碑:M5
- 前置:W0;S0 的 `index.json` 中 `probe-taker` 摘要被改动
- 步骤:
  1. 以 `run` 启动
- 期望:
  - 进程以退出码 2 结束;没有读任何 journal 记录(游标文件未创建);假投递端 0 条消息
- 实现:`apps/watchtower/tests/catalog.rs::wt_019_run_digest_mismatch_exits`

### WT-020 journal 中出现目录里没有的 kind_id 时报 P1 并按 unregistered 计数,不退出

- 规则:[实现说明](README.md#实现说明)、[健康规则](README.md#健康规则)
- 级别:integ
- 里程碑:M5
- 前置:W0 + S0;`prod` journal 有策略实例 5 的 `StrategyParams` 头(`kind_id = 65400`,S0 中没有)与它写的 3 条 `Note{source = Strategy}`(码 1)
- 步骤:
  1. 尾随到末尾,推进到次日 00:05
- 期望:
  - 一条 P1(规则 `StrategyNote`,详情含 `kind_id = 65400` 与"schema 目录不同步");进程仍在运行
  - `hft_strategy_notes_total{instance="prod",strategy="5",note="unregistered"}` = 3;日报 `strategies` 节实例 5 的 `unregistered` 计数 = 3
- 实现:`apps/watchtower/tests/catalog.rs::wt_020_unknown_kind_id`

### WT-021 发布切换后重新加载策略目录

- 规则:[实现说明](README.md#实现说明)
- 级别:integ
- 里程碑:M5
- 前置:W0;`strategy_schemas` 经"当前发布"符号链接指向目录 S0;另备目录 S1 = S0 + 种类 `probe-extra`(`kind_id = 65300`,码 1 P1 `extra_hit`)
- 步骤:
  1. 写 `probe-extra` 实例 6 的 `StrategyParams` 头与一条码 1 的策略 Note → 期望 P1 `StrategyNote`(目录不同步)
  2. 把符号链接切到 S1,之后再写一条同样的 Note
- 期望:
  - 步骤 2 的 Note 产生 P1 `StrategyNote`,消息含描述符名 `extra_hit`;切换后不再出现"目录不同步"的告警
- 实现:`apps/watchtower/tests/catalog.rs::wt_021_reload_after_release_switch`

### WT-022 重新加载策略目录失败时保留旧目录并报 P1

- 规则:[实现说明](README.md#实现说明)
- 级别:integ
- 里程碑:M5
- 前置:同 WT-021,但 S1 的 `index.json` 摘要与文件不符
- 步骤:
  1. 把符号链接切到 S1
  2. 写一条 `probe-taker` 实例 3 的码 1 Note
- 期望:
  - 步骤 1 后一条 P1(详情含 `DigestMismatch`),进程不退出
  - 步骤 2 的 Note 仍按 S0 的描述符投递 P1,消息含名 `edge`
- 实现:`apps/watchtower/tests/catalog.rs::wt_022_reload_failure_keeps_old`

### WT-030 prod 与 sim 的延迟样本落在不同序列

- 规则:[直方图与 Prometheus 导出](README.md#直方图与-prometheus-导出)
- 级别:integ
- 里程碑:M5
- 前置:W0;标准校准(两条 `ClockStatus`,误差界 100 ns);`prod` 一条行情触发的 `LatencySample`(账户 1、`binance-usdm`、`inst_class = perp-major`);`sim` 一条同样的样本(账户 11)
- 步骤:
  1. 推进到下一个 10 s 导出节拍,取 `/metrics` 文本
- 期望:
  - `hft_latency_count_total{instance="prod",venue="binance-usdm",inst_class="perp-major",stage="tick_to_trade"}` = 1,`instance="sim"` 的同名序列 = 1;没有任何序列的计数为 2
- 实现:`apps/watchtower/tests/hist.rs::wt_030_sim_separate_series`

### WT-031 每个维度三份直方图:10 s 窗口、按 UTC 小时与按日

- 规则:[直方图与 Prometheus 导出](README.md#直方图与-prometheus-导出)、[实现说明](README.md#实现说明)
- 级别:integ
- 里程碑:M5
- 前置:W0;`prod` 的 `tick_to_trade` 样本:T0 + 1 s 五条、T0 + 15 s 三条、T0 + 3601 s 两条;`sim` 整个 00 时 `Running`
- 步骤:
  1. 推进到 T0 + 3610 s;再推进到 2026-11-03T00:05:00Z
- 期望:
  - 窗口 [T0, T0 + 10 s) 的计数 5、[T0 + 10 s, T0 + 20 s) 的计数 3(前一窗口的样本不再计入)
  - T0 + 3600 s 之后 `<state>/hourly/2026-11-02.json` 有 00 时一项:`tick_to_trade` `n = 8`、`sim = On`;01 时一项在 01 时关闭后出现,`n = 2`
  - 日报 `latency` 节 `tick_to_trade` 的 `count` = 10
- 实现:`apps/watchtower/tests/hist.rs::wt_031_three_histograms`

### WT-032 重启后按小时直方图从状态目录恢复,窗口与按日从空开始

- 规则:[输入](README.md#输入)、[直方图与 Prometheus 导出](README.md#直方图与-prometheus-导出)
- 级别:integ
- 里程碑:M5
- 前置:W0;00 时已关闭(`hourly` 文件中 `n = 8`);01 时内 T0 + 3700 s 前有 4 个样本
- 步骤:
  1. T0 + 3700 s 时以 SIGTERM 重启 watchtower;重启后再来 3 个样本;推进到 T0 + 7200 s
- 期望:
  - 00 时一项仍为 `n = 8`(未被重写为 0);01 时一项 `n = 3`(重启前未关闭的小时只含重启后的样本,文件不含 01 时的半小时残留)
  - 重启后第一个 10 s 窗口只含重启后的样本
- 实现:`apps/watchtower/tests/hist.rs::wt_032_restart_restores_hourly`

### WT-033 /metrics 只绑回环、每 10 s 导出

- 规则:[直方图与 Prometheus 导出](README.md#直方图与-prometheus-导出)、[配置文件字段](README.md#配置文件字段)
- 级别:proc
- 里程碑:M5
- 前置:Linux;W0(`export.listen = "127.0.0.1:9464"`、`export.interval_s = 10`);网络命名空间中另有地址 10.0.0.5
- 步骤:
  1. 启动后 `ss -ltnp`;从 10.0.0.5 连接 9464;从 127.0.0.1 连续 25 s 每秒 GET `/metrics`
- 期望:
  - 监听地址恰好 `127.0.0.1:9464`;从 10.0.0.5 的连接被拒(`ECONNREFUSED`)
  - 25 s 内 `hft_latency_ns` 的值恰好变化 2 或 3 次,两次变化之间相隔 10 s(± 1 s)
- 实现:`apps/watchtower/tests/proc/export.rs::wt_033_metrics_loopback_period`

### WT-034 指标族与输入事件的对应

- 规则:[直方图与 Prometheus 导出](README.md#直方图与-prometheus-导出)
- 级别:table
- 里程碑:M5
- 前置:W0;每行只注入所列输入;在下一个导出节拍取 `/metrics` 文本;gauge 的定义归 [hft-telemetry](../../crates/hft-telemetry/README.md#保证金与账户指标),本用例只核对名字、标签与取值
- 实现:`apps/watchtower/tests/export.rs::wt_034_metric_families`

| 行 | 输入 | 期望的导出行 |
|---|---|---|
| WT-034.1 | `binance-usdm` 连接 `md-0` 的 7 条 `Bbo` | `hft_msgs_total{venue="binance-usdm",conn="md-0"} 7` |
| WT-034.2 | `ConnState` 使 `okx-swap` 的 `gw-0` 处于 Up | `hft_conn_state{venue="okx-swap",conn="gw-0"} 1` |
| WT-034.3 | 2 条 `OrderEvent`,`reject_code = -1003` | `hft_reject_total{venue="binance-usdm",code="-1003"} 2` |
| WT-034.4 | 3 条 `Note{RateLimitHit}`(`bitget-usdt-futures`) | `hft_ratelimit_hit_total{venue="bitget-usdt-futures"} 3` |
| WT-034.5 | 账户 1 的 2 条 `ExecDecision`(算法 `PairOnFill`、原因 `Rebalance`) | `hft_exec_decisions_total{account="1",algo="PairOnFill",reason="Rebalance"} 2` |
| WT-034.6 | 账户 1 一条 `ExternalFill{cause = Adl}` | `hft_external_fills_total{account="1",venue="binance-usdm",cause="Adl"} 1` |
| WT-034.7 | 最新 `MarginReport` 保证金率 0.15 | `hft_margin_ratio{account="1",venue="binance-usdm"} 0.15` |
| WT-034.8 | 多头,mark 100,交易所 `liq_px` 80,ledger 估算 85 | `hft_liq_dist{account="1",venue="binance-usdm"} 0.15` |
| WT-034.9 | `MarginReport` 在 6 s 前 | `hft_margin_data_age_ms{account="1",venue="binance-usdm"} 6000` |
| WT-034.10 | `Note{CapSet}`(`MarginStress`,档位 2) | `hft_margin_stress_level{account="1"} 2` |
| WT-034.11 | 某永续 inst(`perp-major`)最近 `Funding` 在 400 s 前 | `hft_funding_data_age_s{venue="binance-usdm",inst_class="perp-major"} 400` |
| WT-034.12 | 资产 AIN 在 `okx-swap` 偏离 −59.64 bp | `hft_mark_div_bp{asset="AIN",venue="okx-swap"} -59.64` |
| WT-034.13 | 一个第二腿样本,`leg_gap` = 2 000 000 ns | `hft_leg_gap_ns` 直方图 count 1 |
| WT-034.14 | `prod` 的 `seq` 段头 `committed_len − synced_len` 折合 12 ms | `journal_persist_lag_ms{instance="prod",journal="seq"} 12` |
| WT-034.15 | `ClockStatus` offset 35 000 ns | `hft_clock_offset_ns 35000` |
| WT-034.16 | 冷盘 `statvfs` 剩余 30% | `hft_disk_free_ratio{mount="<cold_root>"} 0.3` |
| WT-034.17 | `/proc/meminfo` `MemAvailable: 1048576 kB` | `hft_mem_available_bytes 1073741824` |
| WT-034.18 | `/proc/pressure/memory` `some avg10=4.20` | `hft_psi{resource="memory"} 4.2` |
| WT-034.19 | 一个槽进入 `Unknown` 后 7 s | `hft_unknown_age_s 7` |
| WT-034.20 | 策略实例 3 的 `Note{AdvisorLag}`,`v[1] = 2500` | `hft_advisor_lag_ms{instance="prod",strategy="3"} 2500` |
| WT-034.21 | 策略实例 3 的码 1 Note 两条、码 99 一条 | `hft_strategy_notes_total{instance="prod",strategy="3",kind="65280",note="edge",level="P1"} 2` 与 `…note="unregistered"…} 1` |
| WT-034.22 | 账户 1 一条 `Note{FundingCorrected}` | `hft_funding_corrected_total{account="1",venue="binance-usdm"} 1` |
| WT-034.23 | sim-gw 测量输出 `sim_tail_lag_ms = 820` | `hft_sim_tail_lag_ms{instance="sim"} 820` |
| WT-034.24 | 误差界超限的 1 个样本 | `unsynced_cal` 计数器 +1;该样本不在 `hft_latency_ns` 的分位数中 |

### WT-035 sim_tail_lag_ms 再大也不触发任何规则

- 规则:[直方图与 Prometheus 导出](README.md#直方图与-prometheus-导出)
- 级别:integ
- 里程碑:M5
- 前置:W0;sim-gw 测量输出 `sim_tail_lag_ms = 3 600 000` 持续 30 min,其他输入无异常
- 步骤:
  1. 推进 30 min
- 期望:
  - 导出 `hft_sim_tail_lag_ms{instance="sim"} 3600000`;`alerts.jsonl` 为空;假投递端 0 条消息
- 实现:`apps/watchtower/tests/hist.rs::wt_035_sim_tail_lag_record_only`

### WT-036 序列数上限取 max_series 与内存折算的较小者,超出并入 other

- 规则:[直方图与 Prometheus 导出](README.md#直方图与-prometheus-导出)、[配置文件字段](README.md#配置文件字段)、[延迟与资源契约](README.md#延迟与资源契约)
- 级别:table
- 里程碑:M5
- 前置:`hist.mem_frac = 0.5`;每序列字节取夹具常数 16 384(实际值以基准 `telemetry/series_bytes` 为准);本单元 cgroup `memory.max` 与 `hist.max_series` 按行
- 实现:`apps/watchtower/tests/hist.rs::wt_036_series_cap`

| 行 | `memory.max` | `hist.max_series` | 期望上限 |
|---|---|---|---|
| WT-036.1 | 167 772 160(160 MiB) | 20 000 | 5 120 |
| WT-036.2 | 268 435 456(256 MiB) | 4 000 | 4 000 |
| WT-036.3 | `max`(无上限) | 20 000 | 20 000 |
| WT-036.4 | 167 772 160 | 20 000;注入 5 121 个不同维度键 | 前 5 120 个键各成序列;第 5 121 个并入 `other`,`series_overflow` = 1 |
| WT-036.5 | 167 772 160 | 20 000;注入 5 120 个键 | 无 `other` 序列,`series_overflow` = 0 |

### WT-037 含 PHC 端点的样本等下一条 ClockStatus 才进直方图

- 规则:[实现说明](README.md#实现说明)
- 级别:integ
- 里程碑:M5
- 前置:W0;`ClockStatus` 在 T0、T0 + 1 s;T0 + 0.4 s 一条含 `hw_rx` 的行情触发样本
- 步骤:
  1. 推进到 T0 + 0.9 s 读窗口计数;再写 T0 + 1 s 的 `ClockStatus` 并读
- 期望:
  - T0 + 0.9 s 时 `tick_to_trade` 计数 0;读到 T0 + 1 s 的 `ClockStatus` 之后计数 1
- 实现:`apps/watchtower/tests/hist.rs::wt_037_phc_waits_next_clockstatus`

### WT-038 hw_rx 为软件时间戳的样本进 ts_src=sw 标签

- 规则:[实现说明](README.md#实现说明)
- 级别:integ
- 里程碑:M5
- 前置:W0;两条行情触发样本,一条 `hw_rx` 来源为 HW,一条为 SW(`SW_TS`)
- 步骤:
  1. 取导出文本
- 期望:
  - 两条样本分别计入 `ts_src="hw"` 与 `ts_src="sw"` 的序列,各 count 1;没有同时含两者的序列
- 实现:`apps/watchtower/tests/hist.rs::wt_038_sw_samples_separate`

### WT-039 ENA 整形计数的读取与按小时增量

- 规则:[输入](README.md#输入)、[日报](README.md#日报)
- 级别:table
- 里程碑:M5
- 前置:假 `ETHTOOL_GSTATS`;主机剖面的交易与 sim 网卡接口名按行;`ens5` 的 `pps_allowance_exceeded` 在 T0 为 100、T0 + 1800 s 为 160
- 实现:`apps/watchtower/tests/host.rs::wt_039_ena_counters`

| 行 | 主机剖面中的 ENI-B / ENI-C 接口名 | 期望 |
|---|---|---|
| WT-039.1 | `["ens5"]` | `hft_ena_allowance_exceeded_total{ifname="ens5",kind="pps"}` 从 100 增长到 160;次日日报 `latency` 节 00 时的 ENA 增量 `{ens5, pps} = 60` |
| WT-039.2 | 无(dev2 形态) | 一次 `SIOCETHTOOL` 调用都没有;导出中没有 `hft_ena_allowance_exceeded_total` |
| WT-039.3 | `["ens5"]`,`bw_in_allowance_exceeded` 不变 | `kind="bw_in"` 的增量 0;日报不列该项的增长小时 |

### WT-050 主机剖面没有 ENI-A 时不开 /healthz 监听

- 规则:[机外健康检查(/healthz)](README.md#机外健康检查healthz)
- 级别:proc
- 里程碑:M5
- 前置:Linux;W0(主机剖面不含 ENI-A,`healthz_port = 9465`)
- 步骤:
  1. 启动后 `ss -ltn`
- 期望:
  - 没有任何地址在 9465 上监听;唯一的 TCP 监听是 `127.0.0.1:9464`
- 实现:`apps/watchtower/tests/proc/healthz.rs::wt_050_no_bind_no_listener`

### WT-051 健康时返回 200 与完整字段

- 规则:[机外健康检查(/healthz)](README.md#机外健康检查healthz)
- 级别:integ
- 里程碑:M5
- 前置:W0,另设主机剖面 ENI-A 私有 IP `127.0.0.1`、`healthz_port = 19465`;`host = "tokyo-cex-1"`;`prod` `Running`、心跳年龄 ≤ 200 ms,最近一组 `ReconcileReport` 在 21 s 前;`sim` `Running`;`hft-kill-switch` active;维护静默文件不存在
- 步骤:
  1. `GET /healthz`
- 期望:
  - 状态 200,`Content-Type: application/json`
  - JSON 键集合恰为 `{v, host, utc_ns, ok, reasons, instances, kill_switch, recon, watchtower, maintenance_until}`;`v = 2`、`host = "tokyo-cex-1"`、`ok = true`、`reasons = []`、`maintenance_until = null`
  - `instances` = `[{name = "prod", state = "Running", hb_age_ms ≤ 1000, live_accounts = 2}, {name = "sim", state = "Running", hb_age_ms ≤ 1000, live_accounts = 0}]`;`kill_switch.active = true`;`recon.age_s = 21`;`watchtower.sample_age_ms ≤ 2000`
- 实现:`apps/watchtower/tests/healthz.rs::wt_051_healthy_200`

### WT-052 /healthz 以外的方法与路径返回 404

- 规则:[机外健康检查(/healthz)](README.md#机外健康检查healthz)
- 级别:table
- 里程碑:M5
- 前置:同 WT-051
- 实现:`apps/watchtower/tests/healthz.rs::wt_052_other_paths_404`

| 行 | 请求 | 期望 |
|---|---|---|
| WT-052.1 | `GET /` | 404 |
| WT-052.2 | `GET /metrics`(发到 healthz 端口) | 404 |
| WT-052.3 | `GET /healthz/x` | 404 |
| WT-052.4 | `POST /healthz` | 404 |
| WT-052.5 | `GET /healthz?verbose=1` | 200,内容与 `GET /healthz` 的键集合相同 |

### WT-053 /healthz 响应不含账户名、key、仓位或金额

- 规则:[机外健康检查(/healthz)](README.md#机外健康检查healthz)
- 级别:integ
- 里程碑:M5
- 前置:同 WT-051;账户 1 有永续仓位 3.5 张、权益 12 345.67 USDT;秘密夹具中的 key 字符串 `AKIATESTKEY0001`
- 步骤:
  1. 分别在健康与 `ok = false`(`prod` 心跳停滞)时 `GET /healthz`
- 期望:
  - 两次响应体都不含 `acct-a`、`tn-b`、`sim-c`、`AKIATESTKEY0001`、`3.5`、`12345.67`;除 WT-051 列出的键与 `instances[]` 的四个键外没有其他键
- 实现:`apps/watchtower/tests/healthz.rs::wt_053_no_sensitive_fields`

### WT-054 有 Live 账户的实例心跳停滞时 ok = false

- 规则:[机外健康检查(/healthz)](README.md#机外健康检查healthz)
- 级别:table
- 里程碑:M5
- 前置:同 WT-051;按行设定 `HeartbeatBlock.state` 与心跳年龄(`hb_stale_ms = 1000`)
- 实现:`apps/watchtower/tests/healthz.rs::wt_054_heartbeat_stale`

| 行 | 实例、状态、心跳年龄 | 期望 |
|---|---|---|
| WT-054.1 | `prod` `Running`,1 000 ms | 200,`ok = true` |
| WT-054.2 | `prod` `Running`,1 001 ms | 503,`ok = false`,`reasons = ["hb_stale:prod"]` |
| WT-054.3 | `prod` `Warmup`,1 001 ms | 503,`reasons` 含 `hb_stale:prod` |
| WT-054.4 | `prod` `Draining`,1 001 ms | 503,`reasons` 含 `hb_stale:prod` |
| WT-054.5 | `prod` `Starting`,5 000 ms | 200 |
| WT-054.6 | `prod` `Stopped`(单元 inactive),心跳不再前进 | 200,`instances[0].state = "Stopped"` |
| WT-054.7 | `sim` `Running`,5 000 ms(只有 Sim 账户) | 200 |

### WT-055 有 Live 账户的实例单元非 active 且状态不是 Stopped 时 ok = false

- 规则:[机外健康检查(/healthz)](README.md#机外健康检查healthz)
- 级别:table
- 里程碑:M5
- 前置:同 WT-051;按行设定单元 `ActiveState` 与 `HeartbeatBlock.state`
- 实现:`apps/watchtower/tests/healthz.rs::wt_055_unit_down`

| 行 | 单元与状态 | 期望 |
|---|---|---|
| WT-055.1 | `hft-core@prod` `failed`,`state = Running` | 503,`reasons` 含 `unit_down:prod` |
| WT-055.2 | `hft-core@prod` `activating`(自动重启),`state = Draining` | 503,`reasons` 含 `unit_down:prod` |
| WT-055.3 | `hft-core@prod` `inactive`,`state = Stopped` | 200 |
| WT-055.4 | `hft-core@sim` `failed`,`state = Running` | 200(`sim` 没有 Live 账户) |

### WT-056 kill-switch 非 active 时 ok = false

- 规则:[机外健康检查(/healthz)](README.md#机外健康检查healthz)
- 级别:table
- 里程碑:M5
- 前置:同 WT-051
- 实现:`apps/watchtower/tests/healthz.rs::wt_056_kill_switch_down`

| 行 | `hft-kill-switch` 的 `ActiveState` | 期望 |
|---|---|---|
| WT-056.1 | `active` | 200,`kill_switch.active = true` |
| WT-056.2 | `inactive` | 503,`kill_switch.active = false`,`reasons` 含 `kill_switch_down` |
| WT-056.3 | `failed` | 503,同上 |

### WT-057 对账报告过旧时 ok = false

- 规则:[机外健康检查(/healthz)](README.md#机外健康检查healthz)
- 级别:table
- 里程碑:M5
- 前置:同 WT-051(`healthz.recon_max_age_s = 90`);按行设定 `prod` 状态与最近一组 `ReconcileReport` 的年龄
- 实现:`apps/watchtower/tests/healthz.rs::wt_057_recon_age`

| 行 | `prod` 状态,对账年龄 | 期望 |
|---|---|---|
| WT-057.1 | `Running`,90 s | 200 |
| WT-057.2 | `Running`,91 s | 503,`reasons` 含 `recon_stale:prod`,`recon.age_s = 91` |
| WT-057.3 | `Warmup`,300 s | 200 |
| WT-057.4 | `Running`,本次运行尚无 `ReconcileReport`,进入 `Running` 已 91 s | 503,`reasons` 含 `recon_stale:prod` |
| WT-057.5 | `Running`,本次运行尚无 `ReconcileReport`,进入 `Running` 已 90 s | 200 |

### WT-058 watchtower 自身主机采样停滞超过 10 s 时 ok = false

- 规则:[机外健康检查(/healthz)](README.md#机外健康检查healthz)
- 级别:table
- 里程碑:M5
- 前置:同 WT-051;从 T0 起让 `host-probe` 阻塞(不再产出采样),按行在 T0 之后的时刻 `GET /healthz`
- 实现:`apps/watchtower/tests/healthz.rs::wt_058_self_stall`

| 行 | 请求时刻 | 期望 |
|---|---|---|
| WT-058.1 | T0 + 10 000 ms | 200,`watchtower.sample_age_ms = 10000` |
| WT-058.2 | T0 + 10 001 ms | 503,`reasons` 含 `watchtower_stalled` |

### WT-059 维护静默截止时刻的读取、截断与失效

- 规则:[机外健康检查(/healthz)](README.md#机外健康检查healthz)
- 级别:table
- 里程碑:M5
- 前置:同 WT-051;当前时刻 T0(1793577600);`host.maint_file` 内容按行(写入后立即请求)
- 实现:`apps/watchtower/tests/healthz.rs::wt_059_maintenance_until`

| 行 | 文件内容 | 期望 |
|---|---|---|
| WT-059.1 | 文件不存在 | `maintenance_until = null`,200 |
| WT-059.2 | `1793578200\n`(T0 + 10 min) | `maintenance_until = 1793578200000000000`,`ok = true`,`reasons = []` |
| WT-059.3 | `1793577599\n`(已过) | `null` |
| WT-059.4 | `abc\n` | `null` |
| WT-059.5 | 空文件 | `null` |
| WT-059.6 | `1793580300\n`(T0 + 45 min) | `maintenance_until = 1793579400000000000`(T0 + 30 min),`reasons` 含 `maintenance_truncated`,`ok = true`,状态 200 |
| WT-059.7 | `1793579400\n`(T0 + 30 min) | `1793579400000000000`,`reasons` 不含 `maintenance_truncated` |
| WT-059.8 | `1793579401\n`(T0 + 30 min + 1 s) | `1793579400000000000`,`reasons` 含 `maintenance_truncated` |
| WT-059.9 | `1793578200\n`,同时 `hft-core@prod` `failed`、`state = Running` | 503,`ok = false`,`maintenance_until = 1793578200000000000`(静默不改变 `ok`) |

### WT-060 每次请求都重新读维护静默文件

- 规则:[机外健康检查(/healthz)](README.md#机外健康检查healthz)
- 级别:integ
- 里程碑:M5
- 前置:同 WT-051
- 步骤:
  1. `GET /healthz`;写 `host.maint_file` = T0 + 10 min;再 `GET`;删除文件;再 `GET`
- 期望:
  - 三次的 `maintenance_until` 依次为 `null`、`1793578200000000000`、`null`
- 实现:`apps/watchtower/tests/healthz.rs::wt_060_maint_file_read_each_request`

### WT-061 维护静默期间 watchtower 自己的 P0 照常投递

- 规则:[机外健康检查(/healthz)](README.md#机外健康检查healthz)、[告警分级与投递](README.md#告警分级与投递)
- 级别:integ
- 里程碑:M5
- 前置:W0;`host.maint_file` = T0 + 20 min
- 步骤:
  1. T0 + 1 min 写一条 `Note{ExternalFill}`(cause `Liquidation`,账户 1)
- 期望:
  - 2 s 内假 Telegram 与 Bark(critical)各收到一条 P0;T0 + 2 min 再次重复投递
- 实现:`apps/watchtower/tests/healthz.rs::wt_061_maintenance_does_not_silence_self`

### WT-062 /healthz 不等规则线程

- 规则:[机外健康检查(/healthz)](README.md#机外健康检查healthz)、[实现说明](README.md#实现说明)
- 级别:integ
- 里程碑:M5
- 前置:同 WT-051;从 T0 起让 `rules` 线程在一次评估中阻塞 30 s(注入)
- 步骤:
  1. T0 + 15 s 起每秒 `GET /healthz`,共 10 次
- 期望:
  - 10 次都在 50 ms 内返回 200;`watchtower.sample_age_ms` 每次 ≤ 2000
- 实现:`apps/watchtower/tests/healthz.rs::wt_062_independent_of_rules_thread`

### WT-063 /healthz 绑 ENI-A 地址时 /metrics 仍只在回环

- 规则:[机外健康检查(/healthz)](README.md#机外健康检查healthz)、[直方图与 Prometheus 导出](README.md#直方图与-prometheus-导出)
- 级别:proc
- 里程碑:M5
- 前置:Linux 网络命名空间,地址 10.0.0.5 模拟 ENI-A;W0 另设主机剖面 ENI-A 私有 IP `10.0.0.5`、`healthz_port = 9465`
- 步骤:
  1. 启动后 `ss -ltn`;从命名空间外经 10.0.0.5 分别连接 9465 与 9464
- 期望:
  - 监听恰好两个:`10.0.0.5:9465` 与 `127.0.0.1:9464`;经 10.0.0.5 连接 9465 返回 200,连接 9464 被拒
- 实现:`apps/watchtower/tests/proc/healthz.rs::wt_063_healthz_eni_metrics_loopback`

### WT-070 SequencerStall 按实例状态与账户类型定级

- 规则:[健康规则](README.md#健康规则)
- 级别:table
- 里程碑:M5
- 前置:W0(`hb_stale_ms = 1000`);在 T0 + 10 s 评估;按行设定实例状态与心跳最后一次被采样到前进的时刻;第 .7 行的 `prod` refdata 只有账户 2(测试网 Live)
- 实现:`apps/watchtower/tests/rules_liveness.rs::wt_070_sequencer_stall`

| 行 | 实例、状态、最后前进的采样时刻 | 期望(T0 + 10 s) |
|---|---|---|
| WT-070.1 | `prod` `Running`,T0 + 9.000 s(年龄 1 000 ms) | 无告警 |
| WT-070.2 | `prod` `Running`,T0 + 8.999 s(年龄 1 001 ms) | 投递 P0 `SequencerStall`,scope `prod` |
| WT-070.3 | `prod` `Warmup`,T0 + 8.999 s | P0 |
| WT-070.4 | `prod` `Draining`,T0 + 5 s | 无 `SequencerStall` |
| WT-070.5 | `prod` `Starting`,T0 + 5 s | 无告警 |
| WT-070.6 | `prod` `Stopped`,T0 − 60 s | 无告警 |
| WT-070.7 | `prod`(只有测试网 Live 账户)`Running`,T0 + 8.999 s | P1 |
| WT-070.8 | `sim` `Running`,T0 + 8.999 s | P1,scope `sim` |

### WT-071 心跳恢复后 P1 记录关闭,P0 继续重复到 ack

- 规则:[健康规则](README.md#健康规则)、[告警分级与投递](README.md#告警分级与投递)
- 级别:integ
- 里程碑:M5
- 前置:W0;`sim` 与 `prod` 的心跳都从 T0 + 8 s 起停止,T0 + 15 s 起恢复;T0 + 100 s 由 ops_bot 在 `acks.jsonl` 追加 `prod` 那条记录的 ack
- 步骤:
  1. 推进到 T0 + 700 s
- 期望:
  - T0 + 10 s:`sim` P1 一条、`prod` P0 一条
  - T0 + 20 s 评估不再命中:`sim` 记录关闭,T0 + 610 s 没有重复投递
  - `prod` P0 在 T0 + 70 s 重复一次;T0 + 100 s ack 之后不再重复,记录关闭(T0 + 130 s 起没有 `SequencerStall` 投递)
- 实现:`apps/watchtower/tests/rules_liveness.rs::wt_071_stall_recovery`

### WT-072 ThreadStall 只看已 claim 的槽且只在 Warmup/Running 时判定

- 规则:[健康规则](README.md#健康规则)
- 级别:table
- 里程碑:M5
- 前置:W0;`prod` 的 `n_slots = 6`(区 2..7 已 claim,区 5 名为 `feed-md:binance-usdm`);在 T0 + 10 s 评估
- 实现:`apps/watchtower/tests/rules_liveness.rs::wt_072_thread_stall`

| 行 | 条件 | 期望 |
|---|---|---|
| WT-072.1 | 区 5 最后前进于 T0 + 8.999 s,其余区每 100 ms 前进,`Running` | P1 `ThreadStall`,scope `(prod, 5)`,详情含 `feed-md:binance-usdm` |
| WT-072.2 | 区 5 最后前进于 T0 + 9.000 s | 无告警 |
| WT-072.3 | 区 20(未 claim,计数恒为 0) | 无告警 |
| WT-072.4 | 只有区 0 停滞 | 只有 `SequencerStall`,无 `ThreadStall` |
| WT-072.5 | `sim` 区 3 停滞 | P1 `ThreadStall`,scope `(sim, 3)` |
| WT-072.6 | `prod` `Stopped`,区 2..7 全部不再前进 | 无 `ThreadStall` |

### WT-073 ProcessDown 按单元、状态与 fatal.json 定级

- 规则:[健康规则](README.md#健康规则)、[实现说明](README.md#实现说明)
- 级别:table
- 里程碑:M5
- 前置:W0;`host.fatal_files = {hft-reconciler = <临时根>/reconciler/fatal.json}`;按行设定单元的 `ActiveState`、`HeartbeatBlock.state` 与 `fatal.json`;在下一个评估节拍断言;第 .7 行的 `prod` refdata 只有账户 2
- 实现:`apps/watchtower/tests/rules_liveness.rs::wt_073_process_down`

| 行 | 单元与状态 | 期望 |
|---|---|---|
| WT-073.1 | `hft-core@prod` `failed`,`state = Running` | P0 `ProcessDown`,scope `hft-core@prod` |
| WT-073.2 | `hft-core@prod` `activating`(自动重启),`state = Draining` | P0 |
| WT-073.3 | `hft-core@prod` `inactive`,`state = Stopped` | 无告警 |
| WT-073.4 | `hft-core@prod` `inactive`,心跳块不存在 | 无告警(本次开机未启动) |
| WT-073.5 | `hft-core@prod` `inactive`,`state = Draining`(停止超时被杀) | P0 |
| WT-073.6 | `hft-core@sim` `failed`,`state = Running` | P1 |
| WT-073.7 | `hft-core@prod`(只有测试网账户)`failed`,`state = Running` | P1 |
| WT-073.8 | `hft-kill-switch` `inactive` | P0 |
| WT-073.9 | `hft-kill-switch` `failed` | P0 |
| WT-073.10 | `hft-reconciler` `failed`,无 `fatal.json` | P1 |
| WT-073.11 | `hft-reconciler` `failed`,`fatal.json = {code: "UidMismatch", account: 1, venue: "binance-usdm", utc_ns: …}` | P0,详情含 `UidMismatch`、账户 `acct-a` 与 `binance-usdm` |
| WT-073.12 | `hft-reconciler` `activating`(自动重启),`fatal.json = {code: "IdentityUnverified", …}` | P1,详情含 `IdentityUnverified` |
| WT-073.13 | `hft-reconciler` `inactive`(联调窗口结束时被停) | 无告警 |
| WT-073.14 | `hft-journal-archiver` `inactive`(回放期间暂停) | 无告警 |
| WT-073.15 | `hft-journal-archiver` `failed` | P1 |
| WT-073.16 | `hft-offbox-watchdog` `failed` | P1 |

### WT-074 fatal.json 只在单元非 active 时读取

- 规则:[实现说明](README.md#实现说明)
- 级别:integ
- 里程碑:M5
- 前置:W0;`hft-reconciler` `active`;`fatal.json` 存在(`code = "UidMismatch"`,上一次失败遗留)
- 步骤:
  1. 运行 5 个评估节拍
- 期望:
  - 假文件系统记录对 `fatal.json` 的读取 0 次;无 `ProcessDown`
- 实现:`apps/watchtower/tests/rules_liveness.rs::wt_074_fatal_json_only_when_down`

### WT-075 单元恢复 active 后 ProcessDown 的 P1 记录关闭

- 规则:[健康规则](README.md#健康规则)、[告警分级与投递](README.md#告警分级与投递)
- 级别:integ
- 里程碑:M5
- 前置:W0;`hft-reconciler` 在 T0 + 5 s 起 `failed`(无 `fatal.json`),T0 + 25 s 起 `active`
- 步骤:
  1. 推进到 T0 + 700 s
- 期望:
  - T0 + 10 s 投递 P1;T0 + 30 s 评估不再命中、记录关闭;T0 + 610 s 无重复
- 实现:`apps/watchtower/tests/rules_liveness.rs::wt_075_process_up_closes`

### WT-076 FeedSilent 只在 Running 时按本次运行的最近行情计时

- 规则:[健康规则](README.md#健康规则)
- 级别:table
- 里程碑:M5
- 前置:W0(`silent_ms`:`binance-usdm` 2000、`okx-swap` 5000);在 T0 + 10 s 评估;`prod` 自 T0 − 1 h 起 `Running`(本次运行的 `EpochStart` 在其前),除另行说明;按行设定最近一条行情事件的时刻
- 实现:`apps/watchtower/tests/rules_market.rs::wt_076_feed_silent`

| 行 | 条件 | 期望 |
|---|---|---|
| WT-076.1 | `prod` `binance-usdm` 最近 `Bbo` 在 T0 + 8.000 s | 无告警 |
| WT-076.2 | 同上,在 T0 + 7.999 s | P1 `FeedSilent`,scope `(prod, binance-usdm)` |
| WT-076.3 | `okx-swap` 最近行情在 T0 + 5.000 s | 无告警 |
| WT-076.4 | `okx-swap` 最近行情在 T0 + 4.999 s | P1,scope `(prod, okx-swap)` |
| WT-076.5 | `prod` `Draining`,`binance-usdm` 最近行情在 T0 − 60 s | 无告警 |
| WT-076.6 | `prod` `Stopped` | 无告警 |
| WT-076.7 | `EpochStart` 在 T0 + 5 s,`Running` 始于 T0 + 7.999 s,其后无行情 | P1(从进入 `Running` 起 2 001 ms) |
| WT-076.8 | 上一次运行的最近行情在 T0 − 10 s,`EpochStart` 在 T0 + 5 s,`Running` 始于 T0 + 9 s,其后无行情 | 无告警(年龄从进入 `Running` 起 1 000 ms) |
| WT-076.9 | `sim` `binance-usdm` 最近行情在 T0 + 7.999 s | P1,scope `(sim, binance-usdm)` |

### WT-077 一个 venue 的 mark 静止而另两个同向移动时报 MarkDivergence

- 规则:[健康规则](README.md#健康规则)、[hft-telemetry 告警判定所需的派生量](../../crates/hft-telemetry/README.md#告警判定所需的派生量)
- 级别:integ
- 里程碑:M5
- 前置:W0(`mark_div_bp = 50`、`hold_s = 30`);`prod` 订阅资产 AIN 在 `binance-usdm`、`okx-swap`、`bitget-usdt-futures` 的永续;T0 前三者 `Mark` 都是 100.00;T0 起 `binance-usdm` 与 `bitget-usdt-futures` 每秒一条 `Mark` 100.60,`okx-swap` 的最近一条 `Mark` 在 T0 − 0.5 s(100.00)之后再无更新
- 步骤:
  1. 推进到 T0 + 60 s
- 期望:
  - T0 + 10 s 至 T0 + 40 s 的评估无告警(连续超阈值的第一次评估在 T0 + 10 s,到 T0 + 40 s 持续 30 s,不大于 `hold_s`)
  - T0 + 50 s 投递 P1 `MarkDivergence`,scope `(prod, AIN, okx-swap)`;详情列出三个 venue 的值(100.60、100.00、100.60)、中位数 100.60、`okx-swap` 的偏离 −59.64 bp 与其最近更新时刻 T0 − 0.5 s
  - `binance-usdm`、`bitget-usdt-futures` 无告警(偏离 0)
- 实现:`apps/watchtower/tests/rules_market.rs::wt_077_mark_divergence_static_venue`

### WT-078 MarkDivergence 的持续时长按连续超阈值的评估计

- 规则:[健康规则](README.md#健康规则)、[实现说明](README.md#实现说明)
- 级别:table
- 里程碑:M5
- 前置:同 WT-077,但 `okx-swap` 自 T0 + 1 s 起偏离 60 bp,按行恢复到与另两者相同的值
- 实现:`apps/watchtower/tests/rules_market.rs::wt_078_mark_divergence_hold`

| 行 | 偏离存在的区间 | 期望 |
|---|---|---|
| WT-078.1 | [T0 + 1 s, T0 + 25 s) | 无告警 |
| WT-078.2 | [T0 + 1 s, T0 + 45 s) | 无告警(T0 + 40 s 时持续 30 s) |
| WT-078.3 | [T0 + 1 s, T0 + 55 s) | T0 + 50 s P1 |
| WT-078.4 | [T0 + 1 s, T0 + 25 s) ∪ [T0 + 35 s, T0 + 85 s) | T0 + 40 s 重新起算;T0 + 80 s P1 |

### WT-079 MarkDivergence 阈值两侧

- 规则:[健康规则](README.md#健康规则)
- 级别:table
- 里程碑:M5
- 前置:同 WT-077;`binance-usdm` 与 `bitget-usdt-futures` 恒为 100.00,`okx-swap` 自 T0 + 1 s 起为所列值,持续到 T0 + 60 s
- 实现:`apps/watchtower/tests/rules_market.rs::wt_079_mark_divergence_threshold`

| 行 | `okx-swap` | 期望 |
|---|---|---|
| WT-079.1 | 99.50(偏离 −50.00 bp) | 无告警 |
| WT-079.2 | 99.49(偏离 −51.00 bp) | T0 + 50 s P1 |

### WT-080 MarkDivergence 的参照与适用范围

- 规则:[健康规则](README.md#健康规则)
- 级别:table
- 里程碑:M5
- 前置:W0;各行的值自 T0 + 1 s 起保持到 T0 + 60 s
- 实现:`apps/watchtower/tests/rules_market.rs::wt_080_mark_divergence_scope`

| 行 | 订阅与取值 | 期望 |
|---|---|---|
| WT-080.1 | 只有 `binance-usdm` 订阅 AIN | 无告警,`hft_mark_div_bp` 无 AIN 序列 |
| WT-080.2 | `binance-usdm` 100.00、`okx-swap` 101.20(中位数 100.60) | T0 + 50 s 两条 P1:`(prod, AIN, binance-usdm)` 与 `(prod, AIN, okx-swap)`,偏离各 ±59.64 bp |
| WT-080.3 | 现货 `binance-spot` BBO 99.98/100.02(中间价 100.00),永续 `binance-usdm` 与 `okx-swap` 的 `Mark` 100.00 | 无告警 |
| WT-080.4 | 现货 `binance-spot` 中间价 100.60,两个永续 `Mark` 100.00 | T0 + 50 s P1,scope `(prod, AIN, binance-spot)` |

### WT-081 偏离消失后 MarkDivergence 记录关闭且不影响任何报价判定

- 规则:[健康规则](README.md#健康规则)、[降级命令(可选扩展)](README.md#降级命令可选扩展)
- 级别:integ
- 里程碑:M5
- 前置:同 WT-077;T0 + 52 s 起 `okx-swap` 恢复为 100.60
- 步骤:
  1. 推进到 T0 + 700 s
- 期望:
  - T0 + 50 s 一条 P1;T0 + 60 s 记录关闭;T0 + 650 s 无重复
  - 运行期间 watchtower 没有连接任何 ctl socket,`prod` journal 中没有任何来自 watchtower 的记录(journal 的写者集合与运行前相同)
- 实现:`apps/watchtower/tests/rules_market.rs::wt_081_mark_divergence_recovers`

### WT-082 UnknownAge 超过 10 s 报 P0,收敛后仍重复到 ack

- 规则:[健康规则](README.md#健康规则)、[告警分级与投递](README.md#告警分级与投递)
- 级别:integ
- 里程碑:M5
- 前置:W0;账户 1、`binance-usdm` 的一个槽在 T0 进入 `Unknown`(`Note{OmsTransition}`),T0 + 25 s 离开;T0 + 90 s ops_bot ack
- 步骤:
  1. 推进到 T0 + 200 s
- 期望:
  - T0 + 10 s 评估(年龄 10 s)无告警;T0 + 20 s 投递 P0 `UnknownAge`,scope `(prod, 1, binance-usdm)`
  - T0 + 80 s 重复一次(条件已消失仍重复);T0 + 90 s ack 后记录关闭,T0 + 140 s 无重复
- 实现:`apps/watchtower/tests/rules_oms.rs::wt_082_unknown_age`

### WT-083 ReconcileMismatch 按条目与 cap 定级

- 规则:[健康规则](README.md#健康规则)
- 级别:table
- 里程碑:M5
- 前置:W0(账户 1:`recon_warn_usd = 50`、`recon_p0_usd = 500`);每行一组 `ReconcileReport`(账户 1、`binance-usdm`)或一条注记
- 实现:`apps/watchtower/tests/rules_recon.rs::wt_083_reconcile_mismatch`

| 行 | 输入 | 期望 |
|---|---|---|
| WT-083.1 | `Note{CapSet}`,cap 来源 `Recon` | P0 `ReconcileMismatch` |
| WT-083.2 | `Position` 条目 `Warn`,名义 49.999999 | 无告警 |
| WT-083.3 | 同上,名义 50.000000 | P1 |
| WT-083.4 | 同上,名义 499.999999 | P1 |
| WT-083.5 | 同上,名义 500.000000 | P0 |
| WT-083.6 | `Balance` 条目 `Warn`,名义 60 | P1 |
| WT-083.7 | `OpenOrder{slot = 0}`,`exch_qty = 0.5` | P1 |
| WT-083.8 | `OpenOrder{slot = 0}`,`exch_qty = 0` | 无告警 |
| WT-083.9 | `Summary.sflags` 含 `HISTORY_UNCHECKED` | P0,在读到该记录时即投递(不等评估节拍) |
| WT-083.10 | `Position` 条目 `Warn`,名义 120,`sflags` 含 `AUTO_EXTERNAL` | P1,详情含"已由引擎自动入账为外来成交" |
| WT-083.11 | `Funding` 条目,差额名义 600 | 无 `ReconcileMismatch` |
| WT-083.12 | `Position` 条目 `Ignore`,名义 900 | 无告警 |

### WT-084 ReconcileMismatch 的名义阈值按账户取

- 规则:[健康规则](README.md#健康规则)、[配置文件字段](README.md#配置文件字段)
- 级别:table
- 里程碑:M5
- 前置:W0,另设 `recon_warn_usd.2 = 10`、`recon_p0_usd.2 = 100`;每行一条 `Position` `Warn` 条目
- 实现:`apps/watchtower/tests/rules_recon.rs::wt_084_per_account_thresholds`

| 行 | 账户、名义 | 期望 |
|---|---|---|
| WT-084.1 | 账户 2,20 | P1 |
| WT-084.2 | 账户 1,20 | 无告警 |
| WT-084.3 | 账户 2,100 | P0 |

### WT-085 FundingCorrected 由 FundingDiff 报一次 P1

- 规则:[健康规则](README.md#健康规则)、[日报](README.md#日报)
- 级别:integ
- 里程碑:M5
- 前置:W0;一条 `Note{FundingCorrected}`(账户 1、`binance-usdm`、inst P、结算时刻 S、金额 +0.2 USDT)
- 步骤:
  1. 尾随;推进到次日 00:05
- 期望:
  - 一条 P1 `FundingDiff`,scope `(1, binance-usdm, P, S)`;`EngineNote` 无告警
  - 日报 `funding` 节 `(1, binance-usdm)`:校正 1 笔、合计 +0.2 USDT
- 实现:`apps/watchtower/tests/rules_funding.rs::wt_085_funding_corrected`

### WT-086 资金费逐笔差额的阈值与同一笔只报一次

- 规则:[健康规则](README.md#健康规则)
- 级别:table
- 里程碑:M5
- 前置:W0(`funding_diff_alert = {abs = 1 USDT, rel = 1%}`);每行一条 `ReconcileReport{item = Funding}`(账户 1、`binance-usdm`、inst P、结算时刻 S),`exch_amount` 与差额按行(金额最小单位 1e-6 USDT)
- 实现:`apps/watchtower/tests/rules_funding.rs::wt_086_funding_diff`

| 行 | `exch_amount`,`|exch − engine|` | 期望 |
|---|---|---|
| WT-086.1 | 50,1.000000 | 无告警 |
| WT-086.2 | 50,1.000001 | P1 |
| WT-086.3 | 50,1.5;随后到达同一 (1, `binance-usdm`, P, S) 的 `Note{FundingCorrected}` | 一条 P1;`FundingCorrected` 只使该记录 `count = 2`,不另投递 |
| WT-086.4 | 50,1.5,同一条目在相邻两组报告中各出现一次 | 一条记录,`count = 2`,投递一次 |
| WT-086.5 | 200,1.5(阈值 2) | 无告警 |
| WT-086.6 | 200,2.000001 | P1 |
| WT-086.7 | 0(交易所无此笔),`engine_amount` 1.5 | P1(阈值 1) |

### WT-087 外来成交按 cause 定级

- 规则:[健康规则](README.md#健康规则)
- 级别:table
- 里程碑:M5
- 前置:W0;账户 1 的 group G 两腿(`binance-usdm` 永续空 −10、`binance-spot` 现货多 +10);每行在 `prod` journal 写一个事件,事件内的记录按行
- 实现:`apps/watchtower/tests/rules_external.rs::wt_087_external_fill_levels`

| 行 | 事件内容 | 期望 |
|---|---|---|
| WT-087.1 | Input `ExternalFill{cause = Liquidation, qty = 4}` + `Note{ExternalFill}`(P0) | 2 s 内 P0 `ExternalFill`;详情含 `Liquidation`、名义、另一腿(现货 +10) |
| WT-087.2 | 同上,`cause = Adl` | P0 |
| WT-087.3 | 同上,`cause = Manual`,`Note{ExternalFill}` 级别 P1 | P1 |
| WT-087.4 | 同上,`cause = Manual`,`Note{ExternalFill}` 级别写成 P0 | P1 |
| WT-087.5 | 同上,`cause = Delivery` | P0 |
| WT-087.6 | 同上,`cause = Unknown` | P0 |
| WT-087.7 | Input `ReconcileReport`(`Position` 条目带 `AUTO_EXTERNAL`)+ 带 `AUTO_RECON` 的 `Note{ExternalFill}`(cause `Unknown`),无 `ExternalFill` 记录 | P0 `ExternalFill` |
| WT-087.8 | Input `OrderEvent` + `Note{OmsForeignFill}` + `Note{ExternalFill}`(cause `Unknown`) | 一条 P0 `ExternalFill`;`Note{OmsForeignFill}` 不产生告警 |

### WT-088 同一笔外来成交只产生一条告警

- 规则:[健康规则](README.md#健康规则)
- 级别:integ
- 里程碑:M5
- 前置:W0;一个事件内有 Input `ExternalFill{cause = Adl}` 与 ledger 的 `Note{ExternalFill}`
- 步骤:
  1. 尾随该事件,推进 30 s
- 期望:
  - `alerts.jsonl` 恰好一条 `ExternalFill` 记录(`count = 1`);没有 `EngineNote` 记录;首发投递恰好一次
- 实现:`apps/watchtower/tests/rules_external.rs::wt_088_one_alert_per_fill`

### WT-089 未入账的外来成交不按 ExternalFill 告警

- 规则:[健康规则](README.md#健康规则)
- 级别:table
- 里程碑:M5
- 前置:W0
- 实现:`apps/watchtower/tests/rules_external.rs::wt_089_unbooked_external`

| 行 | 事件内容 | 期望 |
|---|---|---|
| WT-089.1 | Input `ExternalFill{cause = Adl}`,同一事件内没有 `Note{ExternalFill}`(ledger 去重) | 无 `ExternalFill` 告警 |
| WT-089.2 | Input `ExternalFill{cause = Adl}` + `Note{LedgerAutoHeldOffset}`(P0,全部抵扣),无 `Note{ExternalFill}` | 无 `ExternalFill` 告警;一条 P0 `EngineNote`,码名 `LedgerAutoHeldOffset` |

### WT-090 StaleGenFill 消费两种迟到成交注记

- 规则:[健康规则](README.md#健康规则)
- 级别:table
- 里程碑:M5
- 前置:W0
- 实现:`apps/watchtower/tests/rules_oms.rs::wt_090_stale_gen_fill`

| 行 | 输入 | 期望 |
|---|---|---|
| WT-090.1 | `Note{OmsStaleGenFill}` | P0 `StaleGenFill`;无 `EngineNote` |
| WT-090.2 | `Note{OmsLateFill}` | P0 `StaleGenFill`;无 `EngineNote` |

### WT-091 ForeignOrderFill 按是否带成交定级

- 规则:[健康规则](README.md#健康规则)
- 级别:table
- 里程碑:M5
- 前置:W0
- 实现:`apps/watchtower/tests/rules_oms.rs::wt_091_foreign_order_fill`

| 行 | 输入 | 期望 |
|---|---|---|
| WT-091.1 | `Note{OmsForeignFillViaOrderEvent}`(记录级别 P1) | P0 `ForeignOrderFill` |
| WT-091.2 | `Note{OmsForeignOrder}` | P1 `ForeignOrderFill` |

### WT-092 EngineNote 按记录级别投递,其他规则消费的码不重复报

- 规则:[健康规则](README.md#健康规则)
- 级别:table
- 里程碑:M5
- 前置:W0;每行一条 `prod` 的引擎 Note
- 实现:`apps/watchtower/tests/rules_notes.rs::wt_092_engine_note_levels`

| 行 | Note(`level`) | 期望 |
|---|---|---|
| WT-092.1 | `Note{ExecSerialFallback}`(P0) | P0 `EngineNote`;消息含码名 `ExecSerialFallback` 与 hft-telemetry `note_text` 给出的 `v[]` 解释 |
| WT-092.2 | `Note{ExecManualHold}`(P1,置位) | P1 |
| WT-092.3 | `Note{ExecManualHold}`(Info,解除) | 无告警 |
| WT-092.4 | `Note{AdviceStale}`(P2) | P2;次日日报 `alerts` 节 `EngineNote` 计数 1;Telegram 0 条 |
| WT-092.5 | `Note{EpochStart}`(Info) | 无告警 |
| WT-092.6 | `Note{CtlAudit}` | 无告警 |
| WT-092.7 | `Note{OmsResumeTimeout}`(P0) | P0 |
| WT-092.8 | `Note{OmsResumeTimeout}`(P1) | P1 |
| WT-092.9 | 依次 `Note{FundingCorrected}`、`Note{ExternalFill}`、`Note{OmsStaleGenFill}`、`Note{OmsLateFill}`、`Note{OmsForeignOrder}`、`Note{OmsForeignFillViaOrderEvent}`、`Note{ExecAutoFixDone}`、`Note{ExecAutoFixFailed}`、`Note{RecoverMismatch}`、`Note{TailSkipped}`、`Note{AdvisorLag}`、`Note{CapSet}`、`Note{CapCleared}`、`Note{MarginLevel}` | 没有任何 `EngineNote` 记录(这些码只由各自的规则处理) |
| WT-092.10 | `source = Strategy` 的 Note | 无 `EngineNote` 记录 |

### WT-093 EngineNote 的指纹是 (instance, source, code, account, group)

- 规则:[健康规则](README.md#健康规则)、[告警分级与投递](README.md#告警分级与投递)
- 级别:integ
- 里程碑:M5
- 前置:W0;T0 + 1 s、T0 + 30 s 两条 `Note{OmsHedgePxStale}`(P1,账户 1,group G1);T0 + 40 s 一条同码(group G2)
- 步骤:
  1. 推进到 T0 + 60 s
- 期望:
  - 两条记录:G1 的 `count = 2`、投递 1 次;G2 的 `count = 1`、投递 1 次
- 实现:`apps/watchtower/tests/rules_notes.rs::wt_093_engine_note_fingerprint`

### WT-094 未在本 app 中出现的新引擎码按记录级别告警

- 规则:[健康规则](README.md#健康规则)
- 级别:unit
- 里程碑:M5
- 前置:W0;一条 `source = Exec`、`code = note_code(Exec, 0x7F)`(登记表与 `notes_catalog` 都没有)、`level = P1` 的 Note;另扫描 `apps/watchtower/src/**/*.rs`
- 步骤:
  1. 尾随该 Note
  2. 列出源码中出现的引擎 Note 码名
- 期望:
  - 步骤 1:一条 P1 `EngineNote`,消息含 source 与码的十六进制
  - 步骤 2:出现的码名集合是 WT-092.9 所列各码的子集(本 app 没有按码写的告警表)
- 实现:`apps/watchtower/tests/rules_notes.rs::wt_094_new_engine_code_zero_change`

### WT-095 NakedExposure 的名义阈值与持续时长

- 规则:[健康规则](README.md#健康规则)、[实现说明](README.md#实现说明)
- 级别:table
- 里程碑:M5
- 前置:W0(`naked_usd` 账户 1 与 11 均为 5);以 hft-ledger 从 journal 重建的 group G 两腿名义差,自 T0 + 1 s 起为所列值,按行持续
- 实现:`apps/watchtower/tests/rules_exposure.rs::wt_095_naked_exposure`

| 行 | 名义差与持续 | 期望 |
|---|---|---|
| WT-095.1 | 6,到 T0 + 55 s | T0 + 50 s P0,scope `(prod, 1, G)` |
| WT-095.2 | 6,到 T0 + 45 s | 无告警 |
| WT-095.3 | 5.000000,持续 120 s | 无告警 |
| WT-095.4 | 5.000001,持续 120 s | T0 + 50 s P0 |
| WT-095.5 | `sim` 账户 11 的 group,6,持续 120 s | T0 + 50 s P0,scope `(sim, 11, G)` |

### WT-096 HedgeBroken、HedgeAutoFix、RecoveryAnomaly 与 DailyLossTrip 的触发与级别

- 规则:[健康规则](README.md#健康规则)
- 级别:table
- 里程碑:M5
- 前置:W0;每行一条注记
- 实现:`apps/watchtower/tests/rules_events.rs::wt_096_event_rules`

| 行 | 输入 | 期望 |
|---|---|---|
| WT-096.1 | `Note{CapSet}`,cap 来源 `Broken`(group G) | P0 `HedgeBroken` |
| WT-096.2 | `Note{CapSet}`,cap 来源 `HedgeFailure`(账户 1) | P0 `HedgeBroken` |
| WT-096.3 | `Note{ExecAutoFixDone}`(平掉 `binance-usdm` 腿 4 张) | P1 `HedgeAutoFix`,详情含腿与数量 4 |
| WT-096.4 | `Note{ExecAutoFixFailed}` | P0 `HedgeAutoFix`,详情含失败原因码 |
| WT-096.5 | `Note{RecoverMismatch}` | P0 `RecoveryAnomaly` |
| WT-096.6 | `Note{TailSkipped}` | P0 `RecoveryAnomaly` |
| WT-096.7 | `Note{CapSet}`,cap 来源 `DailyLoss`(账户 1) | P0 `DailyLossTrip` |
| WT-096.8 | WT-096.1 之后 `Note{CapCleared}`(`Broken`) | 不另投递;.1 的 P0 每 60 s 重复直到 ack |

### WT-097 AdvisorLag 只读注记,追赶期间只有一条去重告警

- 规则:[健康规则](README.md#健康规则)、[实现说明](README.md#实现说明)
- 级别:integ
- 里程碑:M5
- 前置:W0;策略实例 3 的 `Note{source = Advisor, code = AdvisorLag}` 自 T0 起每 10 s 一条,到 T0 + 300 s(共 31 条,`v[1]` = 6 000..9 000);另一实例 4 的 Advice `basis_seq` 落后 1 h,但没有 `AdvisorLag` 注记
- 步骤:
  1. 推进到 T0 + 1 000 s
  2. 对同一 journal 运行 `watchtower replay --hour 2026-11-02T00`
- 期望:
  - 实例 3:一条 P1 记录,scope `(prod, 3)`,`count = 31`;投递在 T0 与 T0 + 600 s 各一次;T0 + 900 s 起记录关闭(最近一次命中已满 600 s)
  - 实例 4:无告警
  - 步骤 2 的命中与步骤 1 相同(规则不读系统时钟)
- 实现:`apps/watchtower/tests/rules_events.rs::wt_097_advisor_lag`

### WT-098 StrategyNote 按描述符级别投递

- 规则:[健康规则](README.md#健康规则)、[实现说明](README.md#实现说明)
- 级别:table
- 里程碑:M5
- 前置:W0 + S0;每行一条或两条策略 Note(`source = Strategy`)
- 实现:`apps/watchtower/tests/rules_strategy.rs::wt_098_strategy_note`

| 行 | 输入 | 期望 |
|---|---|---|
| WT-098.1 | `prod` 实例 3,码 1,group G1 | P1 `StrategyNote`;消息含描述符名 `edge` 与 `v[0]` 的含义"edge" |
| WT-098.2 | 实例 3,码 3 | P2;Telegram 0 条 |
| WT-098.3 | 实例 3,码 2 | 无告警;次日日报 `strategies` 节码 2 计数 1 |
| WT-098.4 | 实例 3,码 99 | P2;`hft_strategy_notes_total{…note="unregistered"…}` +1 |
| WT-098.5 | 实例 3,码 1,group G1,T0 与 T0 + 300 s 各一条,观察到 T0 + 590 s | 一条记录 `count = 2`,投递 1 次 |
| WT-098.6 | `sim` 实例 13,码 1 | P1;消息带账户标签 `sim-c` 与"模拟" |

### WT-099 ReplayVerify 按报告内容定级

- 规则:[健康规则](README.md#健康规则)
- 级别:table
- 里程碑:M5
- 前置:W0;`paths.replay_reports/prod/2026-11-01.json` 在 2026-11-02T02:00Z 出现,内容按行(字段归 [testing §3.5](../../docs/testing.md#35-验证回放))
- 实现:`apps/watchtower/tests/rules_verify.rs::wt_099_replay_verify_report`

| 行 | 报告 | 期望 |
|---|---|---|
| WT-099.1 | `verify = "fail"`、`first_divergent_seq = 812345`、`component = "hft-oms"` | P1 `ReplayVerify`,scope `(prod, 2026-11-01)`,详情含 812345 与 `hft-oms` |
| WT-099.2 | `verify = "pass"`、`recompute = "fail"` | P1 |
| WT-099.3 | `verify = "pass"`、`recompute = "skipped"` | P2 |
| WT-099.4 | `verify = "pass"`、`recompute = "pass"` | 无告警 |

### WT-100 ReplayVerify:06:00 UTC 仍缺前一日报告

- 规则:[健康规则](README.md#健康规则)、[日报](README.md#日报)
- 级别:table
- 里程碑:M5
- 前置:W0;`prod` 冷副本有 2026-11-01 已封存的 `seq` 段,该日没有报告;`<state>/hourly/2026-11-01.json` 按行记录当日的标志与实例状态;评估时刻按行
- 实现:`apps/watchtower/tests/rules_verify.rs::wt_100_replay_report_missing`

| 行 | 条件 | 期望 |
|---|---|---|
| WT-100.1 | 2026-11-02T05:59:50Z 评估,当日无标志 | 无告警 |
| WT-100.2 | 06:00:00 评估,当日无标志、实例全天运行 | P1 `ReplayVerify`,scope `(prod, 2026-11-01)` |
| WT-100.3 | 06:00:00,11-01 的 12:00–14:00 联调窗口标志存在 | P2 |
| WT-100.4 | 06:00:00,当前 `/var/lib/hft/guard/tripped` 存在 | P2 |
| WT-100.5 | 06:00:00,`prod` 在 11-01 有 1 小时 `Stopped` | P2 |
| WT-100.6 | 06:00:00,当前联调窗口标志存在 | P2 |

### WT-101 补跑的报告逐日判定,通过的日子告警关闭

- 规则:[健康规则](README.md#健康规则)
- 级别:integ
- 里程碑:M5
- 前置:W0;2026-10-30、10-31、11-01 三日都缺报告且当日都有联调窗口(各一条 P2 记录);2026-11-02T08:00Z 补跑写出三份报告:10-30 与 10-31 `pass/pass`,11-01 `verify = "fail"`
- 步骤:
  1. 推进到 08:00:10
- 期望:
  - 10-30、10-31 的两条 P2 记录关闭,之后不再出现;11-01 的记录升为 P1 并立即投递(详情含分歧 seq)
- 实现:`apps/watchtower/tests/rules_verify.rs::wt_101_backfilled_reports`

### WT-102 某日缺报告超过 3 天升为 P1

- 规则:[健康规则](README.md#健康规则)
- 级别:table
- 里程碑:M5
- 前置:W0;2026-10-29 缺报告且当日有联调窗口(已有 P2 记录);评估时刻按行
- 实现:`apps/watchtower/tests/rules_verify.rs::wt_102_missing_over_three_days`

| 行 | 评估时刻 | 期望 |
|---|---|---|
| WT-102.1 | 2026-11-02T00:00:00Z(距 10-30 00:00 恰 72 h) | 仍为 P2,无新投递 |
| WT-102.2 | 2026-11-02T00:00:10Z | 记录升为 P1,立即投递一条 P1 |

### WT-103 VerifyRun 按运行记录中的失败与疑似定级

- 规则:[健康规则](README.md#健康规则)
- 级别:table
- 里程碑:M5
- 前置:W0;`paths.verify_runs` 下新出现 `runs/tokyo-cex-1-20261102T010000Z/`,段记录按行(字段归 [tools hft-verify](../../tools/README.md#hft-verify))
- 实现:`apps/watchtower/tests/rules_verify.rs::wt_103_verify_run`

| 行 | 段记录 | 期望 |
|---|---|---|
| WT-103.1 | fuzz 段 `target = "binance_bbo"`、`crash_sha1 = ["ab12…"]` | P1 `VerifyRun`,详情含 `binance_bbo` 与 `hft-verify crash list` |
| WT-103.2 | DST 段 `violations = 1`、`failed_seeds = [S]` | P1,详情含种子 S |
| WT-103.3 | DST 段 `exit = "signal:SIGSEGV"`、`failed_seeds = [S]` | P1 |
| WT-103.4 | fuzz 段只有 `suspect_sha1 = ["cd34…"]` | P2,详情提示由 CI 复现 |
| WT-103.5 | fuzz 段同时有 `crash_sha1` 与 `suspect_sha1` | 一条 P1 |
| WT-103.6 | 段 `exit = "interrupted"`,无崩溃、疑似与失败种子 | 无告警 |
| WT-103.7 | 全部段 `exit = "ok"`,无崩溃与疑似 | 无告警 |

### WT-104 定时器启用而 48 h 没有新的运行记录时报 P2

- 规则:[健康规则](README.md#健康规则)
- 级别:table
- 里程碑:M5
- 前置:W0;最新段记录的 `end_utc` = T0 − 48 h;`hft-verify.timer` 的 `ActiveState` 按行;评估时刻按行
- 实现:`apps/watchtower/tests/rules_verify.rs::wt_104_verify_run_stale`

| 行 | 定时器,评估时刻 | 期望 |
|---|---|---|
| WT-104.1 | `active`,T0 | 无告警 |
| WT-104.2 | `active`,T0 + 10 s | P2 `VerifyRun` |
| WT-104.3 | `inactive`(未安装),T0 + 7 d | 无告警 |

### WT-105 MarginStress 跟随 hft-risk 的档位定级并带详情

- 规则:[健康规则](README.md#健康规则)、[hft-risk 保证金分档](../../crates/hft-risk/README.md#保证金分档marginstress)
- 级别:integ
- 里程碑:M5
- 前置:W0;账户 1 在 `binance-usdm` 的永续仓位;journal 依次:T0 `liq_dist` 0.30(无 cap);T0 + 1 s `Note{CapSet}`(`MarginStress`,档位 1)与 `Risk` 事件(`liq_dist` 0.24);T0 + 2 s 档位 2(`Note{MarginLevel}` 档位 2,`liq_dist` 0.14,截止 T0 + 32 s);T0 + 3 s 档位 3(`liq_dist` 0.09)
- 步骤:
  1. 推进到 T0 + 5 s
- 期望:
  - 依次投递 P1(scope `(prod, 1, 1)`)、P0(`(prod, 1, 2)`)、P0(`(prod, 1, 3)`),各在读到记录的 2 s 内
  - 每条详情含 `binance-usdm` 的 `liq_dist`、决定它的 inst、取值来源(交易所 `liq_px` 或 ledger 估算)、维持/保证金比率;P0(档位 2)另含截止 T0 + 32 s
  - T0 时(无 cap)无告警;档位 1 的 P1 记录在档位升高后的下一次评估(T0 + 10 s)关闭,之后不再重复
- 实现:`apps/watchtower/tests/rules_margin.rs::wt_105_margin_stress_tiers`

### WT-106 MarginStress 不自设阈值:比率触发的 L2 照报 P0

- 规则:[健康规则](README.md#健康规则)
- 级别:integ
- 里程碑:M5
- 前置:W0;`liq_dist` 0.30、保证金比率 0.82;hft-risk 因比率交叉校验写 `Note{CapSet}`(`MarginStress`,档位 2)
- 步骤:
  1. 尾随该记录
- 期望:
  - 一条 P0 `MarginStress`,scope `(prod, 1, 2)`,详情含比率 0.82;W0 中没有任何强平距离或比率阈值键(未知键见 WT-002.11)
- 实现:`apps/watchtower/tests/rules_margin.rs::wt_106_margin_stress_ratio_l2`

### WT-107 MarginStress 降档后按新档位开新记录

- 规则:[健康规则](README.md#健康规则)、[告警分级与投递](README.md#告警分级与投递)
- 级别:integ
- 里程碑:M5
- 前置:WT-105 之后;T0 + 60 s hft-risk 降到档位 1(`Note{CapSet}` 档位 1)
- 步骤:
  1. 推进到 T0 + 70 s
- 期望:
  - T0 + 60 s 一条新的 P1 记录(新 `id`,scope `(prod, 1, 1)`);档位 2、3 的 P0 记录在 ack 之前继续每 60 s 重复
- 实现:`apps/watchtower/tests/rules_margin.rs::wt_107_margin_stress_downgrade`

### WT-108 MarginDataStale 读 journal 中 RiskParams 的 margin_stale_ms

- 规则:[健康规则](README.md#健康规则)
- 级别:table
- 里程碑:M5
- 前置:W0;journal 中生效的 `RiskParams` `margin_stale_ms = 15000`;在 T0 + 20 s 评估;按行设定账户 1、`binance-usdm` 最近 `MarginReport` 的时刻
- 实现:`apps/watchtower/tests/rules_margin.rs::wt_108_margin_data_stale`

| 行 | 最近 `MarginReport` | 期望 |
|---|---|---|
| WT-108.1 | T0 + 5.000 s(年龄 15 000 ms) | 无告警 |
| WT-108.2 | T0 + 4.999 s(年龄 15 001 ms) | P1 `MarginDataStale`,scope `(prod, 1, binance-usdm)` |
| WT-108.3 | 同 .2,且 hft-risk 写 `Note{CapSet}`(`MarginStress`,档位 1,原因数据过期) | `MarginDataStale` P1 与 `MarginStress` P1 各一条 |

### WT-109 FundingStale 的三种条件

- 规则:[健康规则](README.md#健康规则)
- 级别:table
- 里程碑:M5
- 前置:W0(`funding_stale_s = 300`);`prod` 订阅永续 inst P(`binance-usdm`);结算时刻 S;评估时刻与输入按行
- 实现:`apps/watchtower/tests/rules_funding.rs::wt_109_funding_stale`

| 行 | 条件 | 期望 |
|---|---|---|
| WT-109.1 | 评估时 P 的最近 `Funding` 年龄 300 s,下次结算时刻未到 | 无告警 |
| WT-109.2 | 年龄 301 s | P1 `FundingStale`,scope `(prod, binance-usdm, P)` |
| WT-109.3 | 最近 `Funding` 给出的下次结算时刻 S 已过 60 s,其后无 `Funding` | 无告警 |
| WT-109.4 | S 已过 61 s,其后无 `Funding` | P1 |
| WT-109.5 | P 有持仓,S + 300 s 评估,S 之后无 `FundingSettle` | 无告警 |
| WT-109.6 | P 有持仓,S + 301 s 评估,S 之后无 `FundingSettle` | P1 |
| WT-109.7 | P 有持仓,S + 120 s 出现合成的 `FundingSettle{synth}`,S + 400 s 评估 | 无告警 |
| WT-109.8 | P 无持仓,S + 400 s 评估,S 之后无 `FundingSettle` | 无告警 |

### WT-110 AccountConfigDrift 只在闸门放行后报,按原因定级

- 规则:[健康规则](README.md#健康规则)
- 级别:table
- 里程碑:M5
- 前置:W0;账户 1、`binance-usdm`;"放行"指本次运行已有该账户 `released = 1` 的 `Note{StartupGate}`
- 实现:`apps/watchtower/tests/rules_account.rs::wt_110_account_config_drift`

| 行 | 输入 | 期望 |
|---|---|---|
| WT-110.1 | 放行后 `AccountConfigReport.mismatches = POS_MODE` | P0 `AccountConfigDrift`,scope `(prod, 1, binance-usdm)` |
| WT-110.2 | 放行后 `mismatches = UID` | P0 |
| WT-110.3 | 放行前 `mismatches = MARGIN_MODE` | 无 `AccountConfigDrift` |
| WT-110.4 | 放行后 `Note{CapSet}`(`AccountCfg`,`params[0] = LEVERAGE`),`mismatches = 0` | P1 |
| WT-110.5 | 放行后 `mismatches = POS_MODE` 且 `Note{CapSet}`(`AccountCfg`,`params[0] = MISMATCH \| LEVERAGE`) | 该 scope 一条记录,级别 P0 |

### WT-111 PersistLag 三条线

- 规则:[健康规则](README.md#健康规则)
- 级别:table
- 里程碑:M5
- 前置:W0;`prod` 的 `seq` journal 段头 `committed_len − synced_len` 折算的滞后序列,使评估时 60 s 窗口 p99 为所列值
- 实现:`apps/watchtower/tests/rules_storage.rs::wt_111_persist_lag`

| 行 | p99(60 s) | 期望 |
|---|---|---|
| WT-111.1 | 50 ms | 无告警 |
| WT-111.2 | 50 ms + 1 ns | P2 `PersistLag`,scope `(prod, seq)` |
| WT-111.3 | 1 s | P2 |
| WT-111.4 | 1 s + 1 ns | P1 |
| WT-111.5 | 10 s | P1 |
| WT-111.6 | 10 s + 1 ns | P0 |
| WT-111.7 | 依次 .2、.4、.6 三个窗口 | 同一 scope 的记录依次升到 P2、P1、P0,每次升级立即投递 |

### WT-112 StorageDegraded:cap 置位 P1,存储原因的 Drain 才是 P0

- 规则:[健康规则](README.md#健康规则)、[hft-risk](../../crates/hft-risk/README.md)
- 级别:table
- 里程碑:M5
- 前置:W0;`prod` journal 的注记序列按行
- 实现:`apps/watchtower/tests/rules_storage.rs::wt_112_storage_degraded`

| 行 | 注记序列 | 期望 |
|---|---|---|
| WT-112.1 | `Note{CapSet}`(`Storage`,persist 滞后档位 1) | P1 `StorageDegraded` |
| WT-112.2 | `Storage` 档位 2,随后 `Note{CapSet}`(`Draining`) | 先 P1,`Draining` 出现时升为 P0 并投递 |
| WT-112.3 | `Storage` 档位 2,5 min 内没有 `Draining`(引擎减仓目标未到达,Drain 推迟) | 只有 P1 |
| WT-112.4 | 无 `Storage` cap,命令 `Drain` 产生的 `Note{CapSet}`(`Draining`) | 无 `StorageDegraded` |
| WT-112.5 | .1 之后 `Note{CapCleared}`(`Storage`) | 下一次评估记录关闭,不再重复 |

### WT-113 SimDrift 需要连续两次差异超过 dust

- 规则:[健康规则](README.md#健康规则)
- 级别:table
- 里程碑:M5
- 前置:W0;Sim 账户 11 的 sim-gw 真值报告(`PositionReport`)与 ledger 重建的持仓;差 1 lot 大于该资产 `dust`
- 实现:`apps/watchtower/tests/rules_sim.rs::wt_113_sim_drift`

| 行 | 相邻两次报告的差异 | 期望 |
|---|---|---|
| WT-113.1 | 1 lot、1 lot | 第二次报告时 P1 `SimDrift`,scope `(sim, 11)` |
| WT-113.2 | 1 lot、0 | 无告警 |
| WT-113.3 | 两次都 ≤ `dust` | 无告警 |
| WT-113.4 | Live 账户 1 的 `PositionReport` 与 ledger 两次差 1 lot | 无 `SimDrift` |

### WT-114 HostPressure 按护栏阈值文件判定(dev2)

- 规则:[健康规则](README.md#健康规则)、[hft-core dev2 护栏](../hft-core/README.md#dev2-护栏)
- 级别:table
- 里程碑:M5
- 前置:W0,另设 `rules.HostPressure.enabled = true`、`host.guard_config` = 夹具 G0;G0 按护栏表写:停机信号 `MemAvailable` < 600 MiB、旧系统 cgroup `memory.pressure some avg10` > 5%、`cpu.pressure some avg10` > 20%、swap-in > 1 000 页/min;只告警信号:整机 PSI memory > 10%(P2)、cpu > 40%(P2)、swap 已用量 5 min 内增长 > 64 MiB(P1);`sim` 实例单元的 `MemoryMax = 603979776`(576 MiB);每行在 T0 + 3 s 的一次采样中注入所列值,其余采样在阈值内;T0 + 10 s 评估
- 实现:`apps/watchtower/tests/rules_host.rs::wt_114_host_pressure`

| 行 | 注入 | 期望 |
|---|---|---|
| WT-114.1 | `MemAvailable` 599 MiB | P1 `HostPressure`,scope `MemAvailable` |
| WT-114.2 | `MemAvailable` 600 MiB | 无告警 |
| WT-114.3 | 旧系统 cgroup `memory.pressure` 5.01% | P1 |
| WT-114.4 | 同上 5.00% | 无告警 |
| WT-114.5 | `cpu.pressure` 20.01% | P1 |
| WT-114.6 | `pswpin` 60 s 内增长 1 001 页 | P1 |
| WT-114.7 | 整机 PSI memory 10.01% | P2(进日报) |
| WT-114.8 | 整机 PSI cpu 40.01% | P2 |
| WT-114.9 | swap 已用量 5 min 内增长 65 MiB | P1 |
| WT-114.10 | swap 已用量 5 min 内增长 64 MiB | 无告警 |
| WT-114.11 | `hft-core@sim` `memory.current` = 543581799(> 90%) | P1,scope `hft-core@sim` |
| WT-114.12 | `memory.current` = 543581798(< 90% × 603979776) | 无告警 |

### WT-115 HostPressure 单次采样即报,早于护栏动作

- 规则:[健康规则](README.md#健康规则)
- 级别:integ
- 里程碑:M5
- 前置:同 WT-114;`MemAvailable` 只在 T0 + 3、4、5 s 三次采样中为 590 MiB;护栏事件文件夹具按护栏规则(连续 3 次、每 5 s 采样)生成,对这段压力不产生动作
- 步骤:
  1. 推进到 T0 + 30 s
- 期望:
  - T0 + 10 s 投递 P1 `HostPressure`;护栏事件文件中没有动作行;无 `GuardAction`
- 实现:`apps/watchtower/tests/rules_host.rs::wt_115_host_pressure_single_sample`

### WT-116 HostPressure 的信号种类与级别从护栏阈值文件读取

- 规则:[健康规则](README.md#健康规则)
- 级别:integ
- 里程碑:M5
- 前置:同 WT-114,但 `host.guard_config` = G1(与 G0 只差一项:整机 PSI memory 的级别为 P1)
- 步骤:
  1. 注入整机 PSI memory 10.01%
- 期望:
  - 投递 P1(不是 P2);watchtower 二进制与 W0 其余键未变
- 实现:`apps/watchtower/tests/rules_host.rs::wt_116_levels_from_guard_file`

### WT-117 非 dev2 主机类不启用护栏规则也不另报 MemAvailable

- 规则:[健康规则](README.md#健康规则)
- 级别:integ
- 里程碑:M5
- 前置:W0 按升级主机形态(`HostPressure`、`GuardAction` `enabled = false`,无 `host.guard_config`);注入 `MemAvailable` 100 MiB 持续 60 s
- 步骤:
  1. 启动并推进 60 s
- 期望:
  - 进程仍在运行(未退出);`alerts.jsonl` 中没有任何以 `MemAvailable` 为输入的记录;`watchtower --check` 对同一配置退出 0
- 实现:`apps/watchtower/tests/rules_host.rs::wt_117_non_dev2_disabled`

### WT-118 GuardAction 按护栏事件定级

- 规则:[健康规则](README.md#健康规则)
- 级别:table
- 里程碑:M5
- 前置:W0,另设 `rules.GuardAction.enabled = true`、`host.guard_events` 指向夹具;每行在事件文件追加一行;第 .3 行的 `prod` refdata 含 `is_prod = true` 的 Live 账户(dev2 上的异常配置)
- 实现:`apps/watchtower/tests/rules_host.rs::wt_118_guard_action`

| 行 | 事件 | 期望 |
|---|---|---|
| WT-118.1 | 一级动作(停批处理) | P1 `GuardAction` |
| WT-118.2 | 二级动作(停 `hft-core@sim` 与 `hft-core@prod`) | P1 |
| WT-118.3 | 二级动作,`prod` 有正式 Live 账户 | P0 |
| WT-118.4 | 自动重启 `hft-core@sim` | P1 |
| WT-118.5 | 停机信号越线而 hft 用量不明显,只告警 | P1 |
| WT-118.6 | 旧系统健康异常,只告警 | P1 |

### WT-119 护栏锁存标志每 24 h 提醒一次,ack 不停止,重启后延续

- 规则:[健康规则](README.md#健康规则)、[告警分级与投递](README.md#告警分级与投递)
- 级别:integ
- 里程碑:M5
- 前置:W0 + `GuardAction` 启用;`/var/lib/hft/guard/tripped`(临时根)在 T0 出现;T0 + 1 h ops_bot ack;T0 + 2 h watchtower 重启(标志仍在);T0 + 30 h 标志被清除
- 步骤:
  1. 推进到 T0 + 50 h
- 期望:
  - 投递时刻恰为 T0 + 10 s(首次评估)与 T0 + 24 h 10 s;T0 + 10 min 等 10 min 节拍上没有重复;ack 与重启都不改变 T0 + 24 h 10 s 的那次提醒
  - 标志清除后的评估记录关闭;T0 + 48 h 10 s 没有提醒
- 实现:`apps/watchtower/tests/rules_host.rs::wt_119_tripped_daily_reminder`

### WT-120 core 配置中关闭的可选扩展规则不评估

- 规则:[健康规则](README.md#健康规则)
- 级别:table
- 里程碑:M5
- 前置:W0(可选扩展规则 `enabled = false`);每行注入一个会使该规则命中的输入,推进 10 min
- 实现:`apps/watchtower/tests/rules_optional.rs::wt_120_optional_rules_off`

| 行 | 输入 | 期望 |
|---|---|---|
| WT-120.1 | `ClockStatus` offset 2 ms、`state = Unsynced` | 无 `ClockOffset` |
| WT-120.2 | 10 s 窗口 `tick_to_trade` p99 超过 `budgets` 中的预算,连续 3 窗口 | 无 `StageP99OverBudget`、`StageP99Sustained` |
| WT-120.3 | 60 s 内 11 条 `Note{RateLimitHit}` | 无 `RateLimitStorm` |
| WT-120.4 | 最近对账在 10 min 前 | 无 `ReconcileStale` |

### WT-121 每条规则都有判定函数与模板配置项

- 规则:[健康规则](README.md#健康规则)、[配置文件字段](README.md#配置文件字段)
- 级别:unit
- 里程碑:M5
- 前置:hft-telemetry `rules` 模块的规则注册表;deploy 的 watchtower 配置模板
- 步骤:
  1. 比较注册表中的规则名集合、模板中 `rules.<name>` 的名字集合与本 app 规则装配表的名字集合
- 期望:
  - 三个集合相等;每个名字在模板中都有 `enabled`、`actions_enabled` 与该规则全部阈值键;以模板运行 `watchtower --check` 退出 0
- 实现:`apps/watchtower/tests/config.rs::wt_121_rule_registry_complete`

### WT-122 ArchiverAlert 按 archiver 告警行的级别投递

- 规则:[健康规则](README.md#健康规则)、[输入](README.md#输入)
- 级别:table
- 里程碑:M5
- 前置:W0,另设 `paths.archiver_alerts` = 临时根下的 `archiver/alerts.jsonl`;每行向该文件追加一行(字段归 [journal-archiver](../journal-archiver/README.md#上传与删除纪律))
- 实现:`apps/watchtower/tests/rules_events.rs::wt_122_archiver_alert`

| 行 | 追加的行 | 期望 |
|---|---|---|
| WT-122.1 | `{level: "P1", kind: "Quarantine", prefix: "hft", instance: "prod", journal: "seq", hour: "2026-11-02T03", …}` | 2 s 内 P1 `ArchiverAlert`,详情含 `Quarantine`、`hft`、`prod`、`seq` 与小时 |
| WT-122.2 | `{level: "P0", kind: "ChainBreak", …, journal: "raw-gw-binance-usdm", …}` | P0,每 60 s 重复直到 ack |
| WT-122.3 | 与 .1 相同 scope 的第二行 | 同一记录 `count = 2`,不另投递 |
| WT-122.4 | 不可解析的行 | 跳过;进程不退出;下一行照常处理 |
| WT-122.5 | watchtower 重启 | 重启前已读过的行不再投递(游标与 journal 游标一起存在 `cursor.json`) |

### WT-130 日报在 00:05 UTC 写出,每个被监视实例一节

- 规则:[日报](README.md#日报)
- 级别:integ
- 里程碑:M5
- 前置:W0 + S0;2026-11-02 全天的两个实例夹具 journal
- 步骤:
  1. 推进到 2026-11-03T00:04:59Z 检查文件;再推进到 00:05:10 检查
- 期望:
  - 00:04:59 时 `<state>/daily/2026-11-02.json` 不存在
  - 00:05:10 时文件存在,顶层恰有 `prod`、`sim` 两节,每节的键集合恰为 `{latency, accounts, strategies, funding, calibration, alerts, gates}`
- 实现:`apps/watchtower/tests/daily.rs::wt_130_daily_written_at_0005`

### WT-131 accounts 节等于 account_report,模拟条目带"模拟"

- 规则:[日报](README.md#日报)
- 级别:integ
- 里程碑:M5
- 前置:W0;`prod` 账户 1 有 group G1、G2 的成交与资金费,账户 2 无交易;`sim` 账户 11 有 group G1 的模拟成交
- 步骤:
  1. 推进到次日 00:05:10,读日报;对同一 journal 以 hft-telemetry `account_report` 计算
- 期望:
  - `prod.accounts` 恰有 4 行:账户 1、账户 1 × G1、账户 1 × G2、账户 2(账户 2 没有 group,不出 group 行),逐字段等于 `account_report` 的输出
  - `sim.accounts` 有 2 行(账户 11、账户 11 × G1),每行 `label = "sim-c"`、`simulated = true`;`sim` 节的 `latency`、`strategies`、`funding`、`alerts` 各项都带 `simulated = true`
- 实现:`apps/watchtower/tests/daily.rs::wt_131_accounts_section`

### WT-132 strategies 节按 notes 与 report 描述符通用汇总

- 规则:[日报](README.md#日报)、[hft-telemetry 策略报告](../../crates/hft-telemetry/README.md#策略报告strategy_report)
- 级别:integ
- 里程碑:M5
- 前置:W0 + S0;`prod` 实例 3 当日策略 Note(按 seq):seq 100 码 1 group G1 `v[0] = 10`、seq 101 码 1 G2 `v[0] = 5`、seq 102 码 1 G1 `v[0] = 20`、seq 103 码 2、seq 104 码 99
- 步骤:
  1. 推进到次日 00:05:10,读 `prod.strategies`
- 期望:
  - 实例 3 一组:码 1(`edge`、P1)计数 3,码 2(`info_tick`、Info)计数 1,码 3 计数 0,`unregistered` 计数 1
  - `report`:`edges` 一行 3;`edge_sum` G1 行 30、G2 行 5;`edge_last` 一行 20;`edge_max` G1 行 20、G2 行 5;`edge_p99` 一行 20 并带 `low_n`;每行带名、单位与聚合方式
  - 整组与同一 journal 上 hft-telemetry `strategy_report` 的输出逐字段相等
- 实现:`apps/watchtower/tests/daily.rs::wt_132_strategies_section`

### WT-133 只新增一个测试策略的 schema 文件即出报,watchtower 零改动

- 规则:[日报](README.md#日报)、[概要](README.md#概要)
- 级别:integ
- 里程碑:M5
- 前置:同一个已构建的 watchtower 二进制;策略目录 S1 = S0 + 种类 `probe-extra`(`kind_id = 65300`,码 1 `extra_hit` P1,report `hits`(Count))
- 步骤:
  1. 以 S1 启动;`prod` journal 写 `probe-extra` 实例 6 的 `StrategyParams` 头与 2 条码 1 Note;推进到次日 00:05:10
- 期望:
  - 两条 Note 合并为一条 P1 记录(`count = 2`),消息含 `extra_hit`;日报 `prod.strategies` 有实例 6 一组,`hits = 2`
  - `apps/watchtower` 的源码与二进制与步骤前相同(未重新构建)
- 实现:`apps/watchtower/tests/daily.rs::wt_133_new_strategy_zero_change`

### WT-134 funding 节计入资金费校正与 FundingDiff 告警

- 规则:[日报](README.md#日报)
- 级别:integ
- 里程碑:M5
- 前置:W0;当日账户 1、`binance-usdm` 两条 `Note{FundingCorrected}`(+0.10、−0.03 USDT,不同结算时刻)与一条触发 `FundingDiff` 的 `Funding` 条目(第三个结算时刻)
- 步骤:
  1. 推进到次日 00:05:10
- 期望:
  - `prod.funding` 中 `(1, binance-usdm)`:`corrected_count = 2`、`corrected_sum = +0.07 USDT`、`funding_diff_alerts = 3`
- 实现:`apps/watchtower/tests/daily.rs::wt_134_funding_section`

### WT-135 calibration 节按 calib_pairs 生成

- 规则:[日报](README.md#日报)、[hft-sim 与正式实盘并行校准](../../crates/hft-sim/README.md#与正式实盘并行校准)
- 级别:table
- 里程碑:M5
- 前置:W0;账户 1 与账户 11 同一日的成交;`calib_pairs` 按行
- 实现:`apps/watchtower/tests/daily.rs::wt_135_calibration_section`

| 行 | `calib_pairs` | 期望 |
|---|---|---|
| WT-135.1 | `[{live = "acct-a", sim = "sim-c"}]` | 日报 `calibration` 恰有一行 `(acct-a, sim-c)`,各指标差异与 hft-telemetry 对这两个账户 `AccountDaily` 逐字段相减的结果相等,并带是否达标 |
| WT-135.2 | `[]` | 两个实例节的 `calibration` 都是空数组 |

### WT-136 alerts 节列出当日各规则次数、P0/P1 与 ack

- 规则:[日报](README.md#日报)
- 级别:integ
- 里程碑:M5
- 前置:W0;当日产生 2 条 `ExternalFill` P0(一条在 01:00 被 `tg:1001` ack)、1 条 `FeedSilent` P1、1 条 `EngineNote` P2
- 步骤:
  1. 推进到次日 00:05:10
- 期望:
  - `prod.alerts.counts` = `{ExternalFill: 2, FeedSilent: 1, EngineNote: 1}`;P0/P1 列表 3 项,其中一项 `acked_by = "tg:1001"`、`acked_utc_ns` = 01:00 对应值,其余两项无 ack 字段;P2 不在列表中
- 实现:`apps/watchtower/tests/daily.rs::wt_136_alerts_section`

### WT-137 gates 节列出冷启动、闸门、检查点、验证报告与验证运行

- 规则:[日报](README.md#日报)
- 级别:integ
- 里程碑:M5
- 前置:W0;当日 `prod` 一次冷启动(停机标记写入的启动 `Kill`)与账户 1 的 `Note{StartupGate}`(released);`Note{ModelPathReset}`(实例 3,原因 VersionMismatch)与 `Note{ModelCheckpointDiscarded}`(实例 4,原因 Corrupt);当日收到 2026-10-31(补跑)与 2026-11-01 两份验证报告,11-01 的 `recompute = "skipped"`;10-29 仍缺报告;`paths.verify_runs` 当日 3 次运行,1 次有失败、1 次只有疑似
- 步骤:
  1. 推进到次日 00:05:10
- 期望:
  - `prod.gates` 含:一次冷启动与启动 `Kill`;账户 1 的放行时刻;实例 3 "只丢路径状态节"、实例 4 "整份丢弃,原因 Corrupt";报告 10-31(补跑)与 11-01(recompute skipped);缺报告的日子 `[2026-10-29]`;验证运行 `{runs: 3, failures: 1, suspects: 1}`
- 实现:`apps/watchtower/tests/daily.rs::wt_137_gates_section`

### WT-138 latency 节的分段统计、按小时项与 ENA 增量

- 规则:[日报](README.md#日报)
- 级别:integ
- 里程碑:M5
- 前置:W0,另设主机类 `dev2`;当日 `prod` 1 000 个 `tick_to_trade` 样本(其中 20 个误差界超限);`sim` 在 00:00–00:30 `Running`、00:30 起 `Stopped`、01:00 起 `Running`;`sim_tail_lag_ms` 取值 100..900;主机剖面 ENI-B 接口名 `ens5`,01 时 `pps_allowance_exceeded` 增 7
- 步骤:
  1. 推进到次日 00:05:10
- 期望:
  - 每个分段一项 `{p50, p99, p999, max, count}`,`tick_to_trade` 的 `count = 980`,标注 `host_class = "dev2"`、`informational = true`;`unsynced_cal = 20` 与误差界分布
  - 按小时项:00 时 `sim = "Mixed"`,01 时 `sim = "On"`,每项含 `tick_to_trade`、`rep_to_order`、`hedge_host` 的 p50/p99/n
  - `sim_tail_lag_ms` 的 p50/p99/max;ENA 增量 01 时 `{ens5, pps} = 7`
- 实现:`apps/watchtower/tests/daily.rs::wt_138_latency_section`

### WT-140 interference 输出与 hft-telemetry 一致,参数取自配置

- 规则:[日报](README.md#日报)、[testing §8.2](../../docs/testing.md#82-延迟验收的测法)
- 级别:table
- 里程碑:M5
- 前置:`<state>/hourly/` 中手造 7 天按小时直方图(2 小时一块、块内先开先关由种子决定;"开"小时 × 1.08,与 TEL-070 同一数据);`interference.*` 同 WT-006,`max_ratio_ub` 按行;`--seed 42`
- 实现:`apps/watchtower/tests/interference.rs::wt_140_matches_telemetry`

| 行 | `max_ratio_ub` | 期望 |
|---|---|---|
| WT-140.1 | 1.05 | 块表(每块类别、先开或先关、是否排除)、两类块的先开/先关计数、每组 `(n_on, n_off, R, 区间, 半宽, 判定)` 与以同一 `InterferenceParams` 直接调用 `interference` 的结果逐项相等;`tick_to_trade` 组判 `Fail`,`pass = false` |
| WT-140.2 | 1.10 | 同一组判 `Pass`,`pass = true`(配置值传入计算,代码中不写死) |

### WT-141 同一种子两次输出逐字节相同,种子写进报告

- 规则:[日报](README.md#日报)
- 级别:unit
- 里程碑:M5
- 前置:WT-140.1 的数据与配置
- 步骤:
  1. `watchtower interference … --seed 42` 运行两次
- 期望:
  - 两次输出逐字节相同;输出含 `seed = 42`
- 实现:`apps/watchtower/tests/interference.rs::wt_141_seed_deterministic`

### WT-142 含 Mixed 小时的块被排除

- 规则:[日报](README.md#日报)
- 级别:unit
- 里程碑:M5
- 前置:WT-140.1 的数据,把 2026-11-03T04:00 那小时的 `sim` 状态改为 `Mixed`
- 步骤:
  1. 运行 `watchtower interference … --seed 42`
- 期望:
  - 块 [04:00, 06:00) 在块表中 `excluded = true`;各组的 `n_on + n_off` 比 WT-140.1 少该块两小时的样本数
- 实现:`apps/watchtower/tests/interference.rs::wt_142_mixed_block_excluded`

### WT-143 区间过宽而上界越线时判"延长窗口"

- 规则:[日报](README.md#日报)
- 级别:unit
- 里程碑:M5
- 前置:与 TEL-071 同一数据(每小时样本乘 [0.6, 1.4] 的小时因子,"开" × 1.06);配置同 WT-140.1
- 步骤:
  1. 运行 `watchtower interference … --seed 42`
- 期望:
  - 该组判定为 `Extend`,文本输出含"区间过宽、延长窗口";`pass = false`、`extend = true`;没有判"不通过"的组
- 实现:`apps/watchtower/tests/interference.rs::wt_143_extend_verdict`

### WT-144 --aa 对 sim 全关的日子按配平规则贴标签并输出半宽

- 规则:[日报](README.md#日报)
- 级别:unit
- 里程碑:M5
- 前置:两天 48 小时 `sim` 全部 `Off` 的按小时直方图;配置同 WT-140.1
- 步骤:
  1. `watchtower interference … --from 2026-11-02 --to 2026-11-03 --seed 7 --aa`
- 期望:
  - 块表每块两小时被贴"开/关"标签,资金费块与普通块各自先开与先关之差 ≤ 1;每组判定都是 `RecordOnly`,输出各组区间半宽;`pass = false`
- 实现:`apps/watchtower/tests/interference.rs::wt_144_aa_mode`

### WT-145 interference 只读按小时直方图

- 规则:[日报](README.md#日报)
- 级别:proc
- 里程碑:M5
- 前置:Linux;WT-140.1 的数据;`strace -f -e trace=openat,connect`
- 步骤:
  1. 运行 `watchtower interference … --seed 42`
- 期望:
  - 打开的文件只有配置、`<state>/hourly/*.json`;没有打开任何 journal 段、心跳块或 `alerts.jsonl`;没有 `connect`;状态目录下没有新文件
- 实现:`apps/watchtower/tests/proc/interference.rs::wt_145_reads_hourly_only`

### WT-146 --from 与 --to 按 UTC 日闭区间取小时

- 规则:[使用本组件](README.md#使用本组件)
- 级别:table
- 里程碑:M5
- 前置:`<state>/hourly/` 有 2026-11-01 至 11-09 的直方图,每小时 600 样本;配置同 WT-140.1;`--from 2026-11-02 --to 2026-11-08`
- 实现:`apps/watchtower/tests/interference.rs::wt_146_date_range`

| 行 | 小时 | 期望 |
|---|---|---|
| WT-146.1 | 2026-11-01T23:00 | 不参与 |
| WT-146.2 | 2026-11-02T00:00 | 参与 |
| WT-146.3 | 2026-11-08T23:00 | 参与 |
| WT-146.4 | 2026-11-09T00:00 | 不参与 |

### WT-150 P0 经 Telegram 与 Bark critical 投递

- 规则:[告警分级与投递](README.md#告警分级与投递)
- 级别:integ
- 里程碑:M5
- 前置:W0;电话通道未配置(供应商未选定)
- 步骤:
  1. 写一条 `Note{ExternalFill}`(cause `Liquidation`)
- 期望:
  - 假 Telegram 端点收到一条 P0(发往 `alert.telegram_chat_id`,所用 bot token 等于 `alert.telegram_token_secret` 指向的秘密文件内容);假 Bark 端点收到一条 `level = critical`、key 等于 `alert.bark_key_secret` 的内容;没有电话类请求
- 实现:`apps/watchtower/tests/alert.rs::wt_150_p0_channels`

### WT-151 P0 每 60 s 重复直到 ack

- 规则:[告警分级与投递](README.md#告警分级与投递)
- 级别:integ
- 里程碑:M5
- 前置:W0(`repeat_s.p0 = 60`);T0 一条 `Note{RecoverMismatch}`;T0 + 130 s 由 ops_bot 追加 ack
- 步骤:
  1. 推进到 T0 + 300 s
- 期望:
  - 投递时刻 T0、T0 + 60 s、T0 + 120 s;T0 + 180 s 起没有投递;记录 `acked_by` 与 `acked_utc_ns` 被填写
- 实现:`apps/watchtower/tests/alert.rs::wt_151_p0_repeat_until_ack`

### WT-152 P1 每 10 min 重复直到条件消失或 ack

- 规则:[告警分级与投递](README.md#告警分级与投递)
- 级别:table
- 里程碑:M5
- 前置:W0(`repeat_s.p1 = 600`);`binance-usdm` 行情从 T0 起停止(`FeedSilent` 窗口型);按行恢复或 ack
- 实现:`apps/watchtower/tests/alert.rs::wt_152_p1_repeat`

| 行 | 后续 | 期望投递时刻 |
|---|---|---|
| WT-152.1 | 一直不恢复,不 ack,观察到 T0 + 1 300 s | T0 + 10 s、T0 + 610 s、T0 + 1 210 s |
| WT-152.2 | T0 + 700 s 恢复行情 | T0 + 10 s、T0 + 610 s;T0 + 710 s 记录关闭,T0 + 1 210 s 无投递 |
| WT-152.3 | T0 + 100 s ack,行情不恢复 | 只有 T0 + 10 s |

### WT-153 P2 只写 Grafana 注释与日报,不重复

- 规则:[告警分级与投递](README.md#告警分级与投递)
- 级别:integ
- 里程碑:M5
- 前置:W0;T0 一条 `Note{AdviceStale}`(P2)
- 步骤:
  1. 推进到次日 00:05:10
- 期望:
  - 假 Grafana 端点在 T0 收到一条注释,之后不再收到;Telegram、Bark 0 条;日报 `alerts.counts.EngineNote = 1`
- 实现:`apps/watchtower/tests/alert.rs::wt_153_p2_annotation_only`

### WT-154 ops_bot 不在时投递照常

- 规则:[告警分级与投递](README.md#告警分级与投递)
- 级别:integ
- 里程碑:M5
- 前置:W0;没有 ops_bot 进程,`acks.jsonl` 不存在
- 步骤:
  1. 写一条 `Note{ExecAutoFixFailed}`
- 期望:
  - P0 照常投递并每 60 s 重复;`alerts.jsonl` 有该记录;进程没有报错退出
- 实现:`apps/watchtower/tests/alert.rs::wt_154_independent_of_ops_bot`

### WT-155 告警记录的字段与指纹

- 规则:[告警分级与投递](README.md#告警分级与投递)
- 级别:unit
- 里程碑:M5
- 前置:W0;一条 `FeedSilent` 命中(scope `(prod, binance-usdm)`)
- 步骤:
  1. 读 `alerts.jsonl` 的该记录;重启 watchtower 后让同一 scope 再次命中(新记录)
- 期望:
  - 记录键集合为 `{id, fingerprint, level, rule, scope, first_utc_ns, last_utc_ns, count, details}`(ack 后另有 `acked_by`、`acked_utc_ns`);`level = "P1"`、`rule = "FeedSilent"`、`count = 1`、`first_utc_ns = last_utc_ns`
  - `fingerprint` = hash(`FeedSilent`, `(prod, binance-usdm)`);重启后同一 scope 的新记录 `fingerprint` 与之相同、`id` 不同
- 实现:`apps/watchtower/tests/alert.rs::wt_155_record_fields`

### WT-156 消息带实例与账户标签,模拟账户另带"模拟"

- 规则:[告警分级与投递](README.md#告警分级与投递)
- 级别:table
- 里程碑:M5
- 前置:W0;每行一条注记
- 实现:`apps/watchtower/tests/alert.rs::wt_156_labels`

| 行 | 注记 | 期望消息 |
|---|---|---|
| WT-156.1 | `prod` 账户 1 的 `Note{ExternalFill}` | 含 `prod` 与 `acct-a`,不含"模拟" |
| WT-156.2 | `prod` 账户 2 的 `Note{ExternalFill}` | 含 `prod` 与 `tn-b` |
| WT-156.3 | `sim` 账户 11 的 `Note{ExternalFill}` | 含 `sim`、`sim-c` 与"模拟" |

### WT-157 ack 从 acks.jsonl 读入,watchtower 不写该文件

- 规则:[告警分级与投递](README.md#告警分级与投递)
- 级别:table
- 里程碑:M5
- 前置:W0;已有一条未 ack 的 P0 记录 `id = A`;按行向 `acks.jsonl` 追加一行
- 实现:`apps/watchtower/tests/alert.rs::wt_157_ack_file`

| 行 | 追加的行 | 期望 |
|---|---|---|
| WT-157.1 | `{"id": "A", "by": "tg:1001", "at_utc_ns": …}` | 记录 A 的 `acked_by = "tg:1001"`;不再重复 |
| WT-157.2 | `{"id": "Z", …}`(不存在的 id) | 忽略;A 继续每 60 s 重复 |
| WT-157.3 | `{"id": "A",`(不可解析) | 跳过该行;A 继续重复;进程不退出 |
| WT-157.4 | 任一行 | 运行期间对 `acks.jsonl` 的打开都是只读(无 `O_WRONLY`/`O_RDWR`) |

### WT-158 Telegram 失败时退避重试,P0 的 60 s 重复不被推迟

- 规则:[告警分级与投递](README.md#告警分级与投递)
- 级别:integ
- 里程碑:M5
- 前置:W0;假 Telegram 端点在 [T0, T0 + 40 s) 对每个请求返回 502,T0 + 40 s 起返回 200;Bark 端点始终返回 200
- 步骤:
  1. T0 写一条 `Note{ExecAutoFixFailed}`;推进到T0 + 130 s
- 期望:
  - Bark 在 T0 收到第一条;Telegram 的首发在 T0 + 40 s 之后的第一次重试成功,重试间隔逐次不减小;T0 + 60 s 与 T0 + 120 s 两次重复都按时投递(Telegram 与 Bark 各一条)
- 实现:`apps/watchtower/tests/alert.rs::wt_158_retry_keeps_p0_cadence`

### WT-159 Telegram 返回 429 时按 retry_after 重试

- 规则:[告警分级与投递](README.md#告警分级与投递)
- 级别:integ
- 里程碑:M5
- 前置:W0;假 Telegram 端点对首个请求返回 429、`retry_after = 5`,之后 200
- 步骤:
  1. T0 写一条 `Note{TailSkipped}`
- 期望:
  - 第二次请求时刻 ≥ T0 + 5 s 且 ≤ T0 + 6 s;该 P0 被投递一次(不重复投递同一条首发)
- 实现:`apps/watchtower/tests/alert.rs::wt_159_rate_limited_retry`

### WT-160 记录关闭后同一指纹再命中开新记录

- 规则:[告警分级与投递](README.md#告警分级与投递)
- 级别:integ
- 里程碑:M5
- 前置:W0;`binance-usdm` 行情在 [T0, T0 + 30 s) 停止,T0 + 30 s 恢复,[T0 + 100 s, T0 + 150 s) 再停
- 步骤:
  1. 推进到 T0 + 200 s
- 期望:
  - 两条 `FeedSilent` 记录,`id` 不同、`fingerprint` 相同;各投递一次(T0 + 10 s、T0 + 110 s)
- 实现:`apps/watchtower/tests/alert.rs::wt_160_reopen_after_close`

### WT-161 重启后恢复未关闭的记录,沿用重复节拍

- 规则:[告警分级与投递](README.md#告警分级与投递)
- 级别:integ
- 里程碑:M5
- 前置:W0;T0 一条 `Note{RecoverMismatch}` 的 P0(未 ack);T0 + 30 s watchtower 重启
- 步骤:
  1. 推进到 T0 + 130 s
- 期望:
  - 重启后没有该记录的重复首发;T0 + 60 s 与 T0 + 120 s 各重复一次,`id` 与重启前相同;`alerts.jsonl` 中该 `id` 只有一条首发记录
- 实现:`apps/watchtower/tests/alert.rs::wt_161_restart_restores_open_records`

### WT-162 孤立的事件型 P1 只投递一次,P0 重复到 ack

- 规则:[告警分级与投递](README.md#告警分级与投递)
- 级别:table
- 里程碑:M5
- 前置:W0(`repeat_s = {p0 = 60, p1 = 600}`);T0 写入所列的一条记录,之后同指纹不再命中;观察到 T0 + 1 300 s,不 ack
- 实现:`apps/watchtower/tests/alert.rs::wt_162_isolated_event_alerts`

| 行 | 记录 | 期望 |
|---|---|---|
| WT-162.1 | `Note{ExecAutoFixDone}`(P1) | 只在 T0 投递一次;T0 + 600 s 的评估记录关闭(同指纹最近一次命中已满 600 s) |
| WT-162.2 | `Note{ExecAutoFixFailed}`(P0) | T0 首发,此后每 60 s 重复,到 T0 + 1 260 s 共 22 次;记录不关闭 |
| WT-162.3 | `Note{FundingCorrected}`(P1),T0 + 300 s 同一指纹再来一条 | T0 首发;T0 + 600 s 重复一次(最近 600 s 内有命中);T0 + 900 s 关闭 |

### WT-165 watchtower 不连接 ctl socket,不发任何命令

- 规则:[降级命令(可选扩展)](README.md#降级命令可选扩展)、[概要](README.md#概要)
- 级别:proc
- 里程碑:M5
- 前置:Linux;W0;同一临时根下有 hft-ctl gateway 的 `ctl.sock`(属组 `hft-ctl`、`0660`);输入使 `FeedSilent`、`MarginStress`、`ReconcileMismatch`、`StorageDegraded` 都命中;`strace -f -e trace=connect`
- 步骤:
  1. 运行 10 min
- 期望:
  - strace 中没有对任何 `ctl.sock` 的 `connect`;gateway 的审计中来自本进程 uid 的请求数为 0
- 实现:`apps/watchtower/tests/proc/no_commands.rs::wt_165_never_connects_ctl`

### WT-166 watchtower 的 uid 不是控制面主体

- 规则:[降级命令(可选扩展)](README.md#降级命令可选扩展)、[hft-ctl-proto 主体与角色码](../../crates/hft-ctl-proto/README.md#主体与角色码)
- 级别:integ
- 里程碑:M5
- 前置:hft-ctl gateway 以 hft-ctl-proto 的主体表装配;测试客户端以 `SO_PEERCRED` uid 映射为 `hft-watch` 连接
- 步骤:
  1. 发送 `SetMode → Live`(issuer 任填)
- 期望:
  - gateway 回 `cmd_id` 全零的 `unauthorized` 并关闭连接;ctl 环中没有记录;主体表中不存在 uid `hft-watch`
- 实现:`apps/watchtower/tests/no_commands.rs::wt_166_not_a_principal`

### WT-167 默认构建不链接 hft-ctl 客户端

- 规则:[降级命令(可选扩展)](README.md#降级命令可选扩展)、[依赖](README.md#依赖)
- 级别:unit
- 里程碑:M5
- 前置:`cargo metadata` 解析 `apps/watchtower` 默认 feature 的依赖闭包
- 步骤:
  1. 在闭包中查找 `hft-ctl`
- 期望:
  - 闭包中没有 `hft-ctl`;源码中 `hft_ctl` 的使用全部在可选扩展的 feature 门之后
- 实现:`apps/watchtower/tests/deps.rs::wt_167_no_ctl_client_by_default`

### WT-170 未写出的非规则键取缺省值

- 规则:[配置文件字段](README.md#配置文件字段)
- 级别:table
- 前置:W0 去掉本行所列键后加载
- 里程碑:M5
- 实现:`apps/watchtower/tests/config.rs::wt_170_defaults`

| 行 | 键 | 期望的生效值 |
|---|---|---|
| WT-170.1 | `paths.replay_reports` | `/var/lib/hft/replay-verify` |
| WT-170.2 | `paths.verify_runs` | `/var/lib/hft/verify/runs` |
| WT-170.3 | `export.listen` / `export.interval_s` | `127.0.0.1:9464` / 10 |
| WT-170.4 | `healthz_port` | 9465 |
| WT-170.5 | `healthz.recon_max_age_s` | 90 |
| WT-170.6 | `hist.sig_digits` / `hist.max_ns` | 3 / 10 000 000 000 |
| WT-170.7 | `hist.max_series` / `hist.mem_frac` | 20 000 / 0.5 |
| WT-170.8 | `host.fatal_files` | `{hft-reconciler = "/var/lib/hft/reconciler/fatal.json"}` |
| WT-170.9 | `host.maint_file` | `/run/hft/maint/until` |
| WT-170.10 | `calib_pairs` | `[]` |
| WT-170.11 | `alert.repeat_s` | `{p0 = 60, p1 = 600}` |
| WT-170.12 | `paths.state` | `/var/lib/hft/watchtower` |
| WT-170.13 | `host.guard_events` | `/var/lib/hft/guard/events.jsonl` |
| WT-170.14 | `chain` | `[]` |
| WT-170.15 | `paths.archiver_alerts` | `/var/lib/hft/archiver/alerts.jsonl` |

### WT-171 规则阈值与资金相关键没有缺省值

- 规则:[配置文件字段](README.md#配置文件字段)
- 级别:table
- 里程碑:M5
- 前置:W0 去掉本行所列键后运行 `watchtower --check`
- 实现:`apps/watchtower/tests/config.rs::wt_171_required_keys`

| 行 | 去掉的键 | 期望 |
|---|---|---|
| WT-171.1 | `rules.FundingDiff.funding_diff_alert` | 退出 2 |
| WT-171.2 | `rules.ProcessDown.p0_fatal_codes` | 退出 2 |
| WT-171.3 | `rules.MarkDivergence.hold_s` | 退出 2 |
| WT-171.4 | `rules.FeedSilent.silent_ms` | 退出 2 |
| WT-171.5 | `naked_usd` | 退出 2 |
| WT-171.6 | `recon_warn_usd` | 退出 2 |
| WT-171.7 | `instances` | 退出 2 |
| WT-171.8 | `strategy_schemas` | 退出 2 |
| WT-171.9 | `alert.telegram_chat_id` | 退出 2 |
| WT-171.10 | `alert.bark_key_secret` | 退出 2 |

### WT-175 线程按实现说明命名

- 规则:[实现说明](README.md#实现说明)
- 级别:proc
- 里程碑:M5
- 前置:Linux;W0,另设主机剖面 ENI-A 私有 IP `127.0.0.1`、`healthz_port = 19465`
- 步骤:
  1. 启动 5 s 后读 `/proc/<pid>/task/*/comm`
- 期望:
  - 含 `tail-prod`、`tail-sim`、`tail-raw`、`host-probe`、`rules`、`daily`、`export`、`healthz`、`alert` 各恰好一个;主机剖面没有 ENI-A 时没有 `healthz` 线程
- 实现:`apps/watchtower/tests/proc/threads.rs::wt_175_thread_names`

### WT-176 事件型规则在读到记录时判定,不等评估节拍

- 规则:[实现说明](README.md#实现说明)
- 级别:table
- 里程碑:M5
- 前置:W0;评估节拍在 T0、T0 + 10 s;每行在 T0 + 0.5 s 写入所列记录
- 实现:`apps/watchtower/tests/rules_events.rs::wt_176_event_rules_immediate`

| 行 | 记录 | 期望 |
|---|---|---|
| WT-176.1 | `Note{ExternalFill}`(Liquidation) | 投递时刻 < T0 + 2.5 s |
| WT-176.2 | `Note{OmsStaleGenFill}` | < T0 + 2.5 s |
| WT-176.3 | `Note{CapSet}`(`MarginStress`,档位 2) | < T0 + 2.5 s |
| WT-176.4 | `Summary` 带 `HISTORY_UNCHECKED` | < T0 + 2.5 s |
| WT-176.5 | 一条使 `FeedSilent` 成立的停流(窗口型) | 在 T0 + 10 s 的评估投递,不早于它 |

### WT-177 投递阻塞不拖住尾随

- 规则:[实现说明](README.md#实现说明)
- 级别:integ
- 里程碑:M5
- 前置:W0;假 Telegram 端点对每个请求挂起 60 s 后才返回;T0 起 `prod` 持续写 `LatencySample` 与一条 `Note{ExternalFill}`
- 步骤:
  1. 推进 60 s,每秒读一次游标与 `/metrics`
- 期望:
  - 游标 `last_seq` 落后 journal 末尾的时间每次 ≤ 1 s;10 s 窗口的计数照常更新;Bark 在写入后 2 s 内收到该 P0
- 实现:`apps/watchtower/tests/alert.rs::wt_177_delivery_does_not_block_tail`

### WT-178 账户与裸敞口由 hft-ledger 从 journal 重建,与热进程同值

- 规则:[实现说明](README.md#实现说明)
- 级别:integ
- 里程碑:M5
- 前置:W0;`prod` 夹具 journal 一天(成交、外来成交、资金费、采纳),末尾为 Drain 快照
- 步骤:
  1. 尾随到末尾,取 watchtower 重建的每 (账户, group) 两腿净量、现金与 `PnlBreakdown`
- 期望:
  - 与 Drain 快照中 ledger 节的同一组值逐值相等
- 实现:`apps/watchtower/tests/ledger.rs::wt_178_ledger_rebuild_matches_snapshot`

### WT-185 事件写入到进入直方图的延迟

- 规则:[延迟与资源契约](README.md#延迟与资源契约)
- 级别:host
- 里程碑:M7
- 门槛:⑩
- 环境:统一升级主机,`prod` 与 `sim` 运行,watchtower 尾随热段
- 授权:无
- 步骤:连续 24 h,对每个 `LatencySample` 记录其 journal 提交时刻与 watchtower 将其计入 10 s 直方图的时刻(watchtower 调试计数器)
- 通过标准:两者之差的 p99 < 1 s
- 证据:`ops-log/watchtower/WT-185-<日期>.json`
- 实现:`tests/host/WT-185.sh`

### WT-186 规则命中到投递的延迟

- 规则:[延迟与资源契约](README.md#延迟与资源契约)
- 级别:proc
- 里程碑:M5
- 前置:Linux;W0;真实 watchtower 进程,假 Telegram 端点记录接收时刻;hft-testkit 写者向 `prod` journal 发布记录
- 步骤:
  1. 100 次:发布一条 `Note{ExternalFill}`(事件型 P0),记录发布时刻与接收时刻
  2. 100 次:让 `UnknownAge` 在某次评估命中,记录评估时刻与接收时刻;其中 10 次让 Telegram 首个请求返回 502
- 期望:
  - 步骤 1 每次接收 − 发布 < 2 s;步骤 2 每次接收 − 评估 < 5 s(含一次重试)
- 实现:`apps/watchtower/tests/proc/latency.rs::wt_186_hit_to_delivery`

### WT-187 /healthz 响应时间与采样年龄

- 规则:[延迟与资源契约](README.md#延迟与资源契约)、[机外健康检查(/healthz)](README.md#机外健康检查healthz)
- 级别:host
- 里程碑:M7
- 门槛:⑪
- 环境:统一升级主机;dev2 上的 `hft-offbox-watchdog` 每 15 s 请求一次
- 授权:无
- 步骤:连续 24 h 记录看门狗每次请求的往返时间与响应中的 `watchtower.sample_age_ms`
- 通过标准:往返时间全部 < 50 ms(不含网络往返,以 watchtower 侧的处理时长计);`sample_age_ms` 全部 ≤ 2 000
- 证据:`ops-log/watchtower/WT-187-<日期>.csv`
- 实现:`tests/host/WT-187.sh`

### WT-188 日报在 00:05 之后 5 min 内生成

- 规则:[延迟与资源契约](README.md#延迟与资源契约)、[日报](README.md#日报)
- 级别:host
- 里程碑:M6
- 门槛:⑦
- 环境:dev2,`sim` 实例连续运行期间
- 授权:无
- 步骤:连续 7 天检查 `/var/lib/hft/watchtower/daily/<date>.json` 的写出时刻
- 通过标准:7 个文件都存在,写出时刻都在当日 00:05:00 与 00:10:00 之间
- 证据:`ops-log/watchtower/WT-188-<起始日期>.txt`(文件列表与 mtime)
- 实现:`tests/host/WT-188.sh`

### WT-189 CPU 与内存在单元上限内

- 规则:[延迟与资源契约](README.md#延迟与资源契约)
- 级别:host
- 里程碑:M5
- 环境:dev2;另在 M7 的统一升级主机上重复
- 授权:无
- 步骤:连续 24 h 读本单元 cgroup 的 `cpu.stat` 与 `memory.peak`,并读 `/metrics` 中的序列数
- 通过标准:任一 60 s 窗口的 CPU 用量 ≤ 1 核;`memory.peak` ≤ 该主机类内存表给本单元的 `MemoryMax`;序列数 ≤ WT-036 的上限,超出部分计入 `series_overflow`
- 证据:`ops-log/watchtower/WT-189-<主机>-<日期>.json`
- 实现:`tests/host/WT-189.sh`

### WT-190 依赖方向

- 规则:[依赖](README.md#依赖)
- 级别:table
- 里程碑:M5
- 前置:夹具工作区,按行给 `apps/watchtower` 加依赖后运行 `cargo xtask deps`
- 实现:`apps/watchtower/tests/deps.rs::wt_190_dependency_rules`

| 行 | 加入的依赖 | 期望 |
|---|---|---|
| WT-190.1 | `hft-journal`,启用 `writer` | 退出 1 |
| WT-190.2 | `strategies/latency-probe` | 退出 1 |
| WT-190.3 | `hft-strategy-registry` | 退出 1 |
| WT-190.4 | `hft-sequencer` | 退出 1 |
| WT-190.5 | `hft-venue` | 退出 1 |
| WT-190.6 | `hft-telemetry`、`hft-ledger`、`hft-rt`(只读映射) | 退出 0 |

### WT-195 真实 hft-core 测试实例停流时只告警、不发命令

- 规则:[概要](README.md#概要)、[健康规则](README.md#健康规则)
- 级别:proc
- 里程碑:M5
- 前置:Linux CI;hft-testkit 假交易所 + hft-core 测试实例(只含测试网形态账户)+ watchtower(W0,`silent_ms.binance-usdm = 2000`)
- 步骤:
  1. 运行 60 s 后让假交易所停止推送行情 30 s
- 期望:
  - 停流后第一个评估节拍投递 P1 `FeedSilent`;hft-core 的 journal 中没有任何 `ControlCmd` 记录,`Note{CtlAudit}` 中没有来自 watchtower 进程的请求
- 实现:`apps/watchtower/tests/proc/integration.rs::wt_195_feed_silent_no_commands`

### WT-196 dev2 上停掉 kill-switch 报 ProcessDown P0

- 规则:[健康规则](README.md#健康规则)
- 级别:host
- 里程碑:M6
- 环境:dev2,联调窗口外(`prod` 未运行),`hft-kill-switch` active
- 授权:[需授权]
- 步骤:`systemctl stop hft-kill-switch`;等待 30 s;`systemctl start hft-kill-switch`
- 通过标准:停止后 20 s 内告警群收到 P0 `ProcessDown`(scope `hft-kill-switch`)且 Bark critical 响铃;启动后该记录在 ack 前继续每 60 s 重复,ack 后停止
- 证据:`ops-log/watchtower/WT-196-<日期>.md`(告警截图与 `alerts.jsonl` 片段)
- 实现:`tests/host/WT-196.sh`

### WT-197 dev2 护栏演练中 watchtower 的告警

- 规则:[健康规则](README.md#健康规则)、[hft-core dev2 护栏](../hft-core/README.md#dev2-护栏)
- 级别:drill
- 里程碑:M5
- 环境:dev2,护栏守护运行;与 hft-core 的护栏演练(其验收第 1、5、6、7、9 条)同时进行
- 授权:[需授权]
- 步骤:按 [hft-core dev2 护栏](../hft-core/README.md#dev2-护栏) 的验收步骤执行:① 在 `hft.slice` 内制造内存压力;② 批处理满载使整机 cpu PSI 升高而 `system.slice` 的 PSI 不升;③ 在 hft 两个 slice 之外制造 CPU 争用使旧系统所在 cgroup 的 `cpu.pressure` 越线;④ 二级动作停掉 `sim` 后让资源回落;⑤ 锁存标志存在时重启主机
- 通过标准:① `HostPressure` P1 的投递时刻早于护栏事件文件中第一条动作行,随后 `GuardAction` P1(一级、二级各一条);② 只有整机 PSI 的 P2(进日报),无 P1,护栏无动作;③ `HostPressure` P1 与 `GuardAction` P1("hft 用量不明显,未动作");④ 守护自动重启 `hft-core@sim` 时 `GuardAction` P1;⑤ 主机重启后锁存提醒的记录仍在,24 h 后再提醒一次
- 证据:`ops-log/watchtower/WT-197-<日期>.md`,并记入 [incident-response](../../docs/runbooks/incident-response.md) 的演练记录
- 实现:`tests/host/WT-197.sh`

### WT-198 停掉 watchtower 或阻断 /healthz 后机外看门狗在 60 s 内告警

- 规则:[机外健康检查(/healthz)](README.md#机外健康检查healthz)
- 级别:drill
- 里程碑:M7
- 门槛:⑪
- 环境:统一升级主机 + dev2 上的 `hft-offbox-watchdog`
- 授权:[需授权]
- 步骤:按 [release-and-deploy](../../docs/runbooks/release-and-deploy.md) 的维护静默步骤:① 不开静默,`systemctl stop hft-watchtower`;恢复;② 在安全组移除 dev2 的放行;恢复;③ `sudo hft-maint start 10`,等看门狗读到一次后重复 ①;`hft-maint stop`
- 通过标准:① 与 ② 各在 60 s 内收到看门狗的 Telegram 与电话告警,恢复后收到恢复通知;③ 只收到 Telegram、没有电话;静默截止后恢复电话告警
- 证据:`ops-log/watchtower/WT-198-<日期>.md`(告警记录与时刻)
- 实现:`tests/host/WT-198.sh`

### WT-199 升级主机上 /healthz 对实例停滞、kill-switch 停止与干净停止的响应

- 规则:[机外健康检查(/healthz)](README.md#机外健康检查healthz)
- 级别:host
- 里程碑:M7
- 环境:统一升级主机,`prod` 只含测试网 Live 账户(M7);维护窗口内
- 授权:[需授权]
- 步骤:① `sudo hft-maint start 10`;`kill -STOP` `hft-core@prod` 主进程 3 s 后 `kill -CONT`;② `systemctl stop hft-kill-switch` 30 s 后启动;③ `prod` 以 `/shutdown` 干净停止后重启;每步期间每秒从 dev2 `curl` 一次 `/healthz`
- 通过标准:① `kill -STOP` 期间出现 503 且 `reasons` 含 `hb_stale:prod`,`maintenance_until` 为启动静默后约 600 s 的时刻;恢复后 200;② 停止期间 503 且 `reasons` 含 `kill_switch_down`;③ 停止后 200 且 `instances[0].state = "Stopped"`;全过程 `/metrics` 从 dev2 不可达
- 证据:`ops-log/watchtower/WT-199-<日期>.log`(curl 输出)
- 实现:`tests/host/WT-199.sh`

### WT-200 每条核心规则至少被演练触发一次

- 规则:[健康规则](README.md#健康规则)
- 级别:drill
- 里程碑:M6
- 门槛:⑦
- 环境:dev2 的 `sim` 实例与联调窗口内的测试网 `prod` 角色实例(核心规则中需要实盘条件的,用 hft-testkit 夹具 journal 经 `watchtower replay` 触发)
- 授权:[需授权]
- 步骤:按 [incident-response](../../docs/runbooks/incident-response.md) 的演练清单逐条制造每条核心规则的触发条件
- 通过标准:规则表中每条核心规则都有一次演练记录,记录含触发时刻、收到的告警级别与 scope,且级别与规则表一致
- 证据:incident-response 演练记录中每条规则一行,文件名含 `WT-200`
- 实现:`tests/host/WT-200.sh`

### WT-201 模拟账户长跑期间日报按时生成且无漏报

- 规则:[日报](README.md#日报)、[告警分级与投递](README.md#告警分级与投递)
- 级别:host
- 里程碑:M6
- 门槛:⑦
- 环境:dev2 的 `sim` 实例,门槛 ⑦ dev2 段的计时期间(另在 M7 满规模段重复)
- 授权:无
- 步骤:每日对前一 UTC 日运行 `watchtower replay`(逐小时)与 `watchtower daily`,与生产 `alerts.jsonl` 和日报比较
- 通过标准:每天的离线命中与生产告警记录逐项相等(无漏报);离线日报与生产日报逐字段相等;每天的日报都存在
- 证据:每日比较结果,文件名含 `WT-201` 与日期,归档到门槛 ⑦ 记录
- 实现:`tests/host/WT-201.sh`

### WT-202 非 root 的本单元用户能读 ENA 统计

- 规则:[输入](README.md#输入)
- 级别:host
- 里程碑:M7
- 环境:统一升级主机(或 M1.5 探针实例),`hft-watch` 用户
- 授权:无
- 步骤:以 `hft-watch` 身份运行 watchtower 的 ENA 采样自检,对照 `ethtool -S <交易网卡>`
- 通过标准:各 `*_allowance_exceeded` 计数的名称与取值与 `ethtool -S` 一致;没有 `EPERM`
- 证据:`ops-log/watchtower/WT-202-<主机>.txt`
- 实现:`tests/host/WT-202.sh`

### WT-203 主机剖面缺失或不可用时退出 2

- 规则:[输入](README.md#输入)、[机外健康检查(/healthz)](README.md#机外健康检查healthz)
- 级别:table
- 里程碑:M5
- 前置:W0;各行改 `paths.hostprofile` 指向的文件;主机类按行
- 实现:`apps/watchtower/tests/config.rs::wt_203_hostprofile_errors`

| 行 | 主机类 / 主机剖面 | 期望 |
|---|---|---|
| WT-203.1 | `c32` / 文件不存在 | 退出 2,stderr 含 `paths.hostprofile` |
| WT-203.2 | `c32` / 文件不是 JSON | 退出 2 |
| WT-203.3 | `c32` / ENI-A 的私有 IP 为 `unreadable` | 退出 2 |
| WT-203.4 | `dev2` / 不含 ENI-A | 启动完成(进程在运行,无退出码),不监听 9465 |
| WT-203.5 | `c32` / ENI-A `127.0.0.1`,运行中把文件改为 ENI-A `10.9.9.9` | 监听仍在 `127.0.0.1:9465`(只在启动时读) |

### WT-204 PipelineAlert 按 pipeline 告警项的级别投递

- 规则:[健康规则](README.md#健康规则)、[输入](README.md#输入)
- 级别:table
- 里程碑:M5
- 前置:W0;`<paths.pipeline_state>/sim/status.json` 的 `alerts` 初值为空;每行原子替换该文件
- 实现:`apps/watchtower/tests/rules_events.rs::wt_204_pipeline_alert`

| 行 | `alerts` 的变化 | 期望 |
|---|---|---|
| WT-204.1 | 加入 `{level: "P1", code: "VersionConflict", detail: "v42", at: …}` | 2 s 内 P1 `PipelineAlert`,详情含 `sim`、`VersionConflict` 与 `v42` |
| WT-204.2 | 加入 `{level: "P2", code: "WriteOffNeeded", group: "16777317", …}` | P2(Grafana 注释),Telegram 无 |
| WT-204.3 | 行 1 的项保持不变,文件被重写一次 | 不重复投递 |
| WT-204.4 | 文件内容不可解析 | 跳过;不退出;下一次合法内容照常处理 |

### WT-205 ReconcilerAlert 按 reconciler status.json 的 alert 字段告警

- 规则:[健康规则](README.md#健康规则)
- 级别:table
- 里程碑:M5
- 前置:W0;`paths.reconciler_status` 的文件含一项 `{account: "acct-a", venue: "binance-usdm", unknown_query_fail_rounds: 0, funding_query_fail_rounds: 0, alert: null}`;每行原子替换该文件
- 实现:`apps/watchtower/tests/rules_events.rs::wt_205_reconciler_alert`

| 行 | 该项的变化 | 期望 |
|---|---|---|
| WT-205.1 | `unknown_query_fail_rounds = 3`,`alert = "P1"` | P1 `ReconcilerAlert`,详情含 `acct-a`、`binance-usdm`、3、0 |
| WT-205.2 | 行 1 之后 `unknown_query_fail_rounds = 4`,`alert = "P1"` | 不另投递(同一记录 `count` 加 1) |
| WT-205.3 | 行 2 之后 `alert = null` | 记录条件消失,无新投递 |
| WT-205.4 | `unknown_query_fail_rounds = 2`,`alert = null` | 无告警 |

### WT-206 StartupSlow 按 StartupStep 结束记录的标志位告警

- 规则:[健康规则](README.md#健康规则)
- 级别:table
- 里程碑:M5
- 前置:W0;`prod` journal 中追加一条 `Note{StartupStep}` 结束记录(`v[1] = 1`),`v[0]` 与 `v[3]` 按行
- 实现:`apps/watchtower/tests/rules_events.rs::wt_206_startup_slow`

| 行 | `v[0]` / `v[3]` | 期望 |
|---|---|---|
| WT-206.1 | 7 / 0 | 无告警 |
| WT-206.2 | 6 / 1(bit 0) | P1 `StartupSlow`,详情含步 6 与"超过启动预算" |
| WT-206.3 | 7 / 2(bit 1) | P1,详情含"闸门超时" |
| WT-206.4 | 7 / 6(bit 1、bit 2) | P0 |
| WT-206.5 | 7 / 4(只有 bit 2) | 无 `StartupSlow`(`tail_lost` 另由 `EngineNote` 的 `JournalTailLost` 报) |

### WT-207 新增路径键的默认值

- 规则:[配置文件字段](README.md#配置文件字段)
- 级别:table
- 里程碑:M5
- 前置:W0 去掉下列键后加载配置
- 实现:`apps/watchtower/tests/config.rs::wt_207_path_defaults`

| 行 | 键 | 期望默认值 |
|---|---|---|
| WT-207.1 | `paths.pipeline_state` | `/var/lib/hft/pipeline` |
| WT-207.2 | `paths.reconciler_status` | `/var/lib/hft/reconciler/status.json` |
| WT-207.3 | `paths.hostprofile` | `/run/hft/hostprofile.json` |
| WT-207.4 | `healthz_port` | 9465 |

## 验收

- **M5 完成**:本文件中里程碑 ≤ M5、级别为 `unit`、`table`、`integ`、`proc` 的用例全部有同编号的测试函数且在 CI 通过(`cargo xtask test-trace`,规则见 [docs/tests §6](../../docs/tests/README.md#6-与代码的对应));WT-189(dev2)与 WT-197 的记录归档。
- **门槛 ⑦**([testing §8](../../docs/testing.md#8-上线门槛)):模拟账户连续运行期间每日日报按时生成(WT-188)、无漏报(WT-201,对照 journal 离线重跑);每条核心规则至少被演练触发一次(WT-200,记录在 [incident-response](../../docs/runbooks/incident-response.md))。
- **门槛 ⑩**:同机干扰报告由 `watchtower interference` 产出(WT-140–WT-146 在 CI 通过;判据与数值归 [testing §8.2](../../docs/testing.md#82-延迟验收的测法));WT-185 的记录。
- **门槛 ⑪**:机外看门狗演练中停掉 watchtower 或阻断 `/healthz` 后 60 s 内看门狗告警到达、维护静默期间只发 Telegram(WT-198);WT-187、WT-199 的记录。
