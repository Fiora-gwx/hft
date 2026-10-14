---
component: hft-core
prefix: CORE
spec: README.md
---

# hft-core 测试规格

当前实现覆盖说明：`tests/config.rs` 验证 CORE-002、CORE-012、CORE-045 的可执行子集，含控制批次原子发布、分片头重组、模拟回报解码、模拟连接与行情的启动闸门及真实 LiveDriver 的 Drain；`tests/paper_process.rs::core_048_sim_instance_boots_without_keys` 以固定行情和真实 `SimExchange` 启动进程，等待 ClockOk 后经控制 socket 启用 registry 策略，检查发送、Ack、CancelAck、SIGTERM Drain、冷副本及重启拒绝。CORE-048 的部署目录、共享心跳块与完整主机验收仍待完成；本说明不改变原用例期望或里程碑。

格式与规则见 [docs/tests](../../docs/tests/README.md)。

## 覆盖

| README 小节 | 用例 |
|---|---|
| 概要 | 不适用:非规范 |
| 目录 | 不适用:非规范 |
| 使用本组件 | 不适用:见子节 |
| 命令行与退出码 | CORE-001–CORE-017、CORE-299、CORE-412 |
| 策略装配 | CORE-030–CORE-035 |
| 实例角色(prod 与 sim) | CORE-040–CORE-050 |
| 线程计划 | CORE-060–CORE-090 |
| 内存与 journal 段(按主机类唯一归属) | CORE-100–CORE-122、CORE-130–CORE-148、CORE-412 |
| 主机资源隔离(slice、核与出口 IP) | CORE-160–CORE-177 |
| dev2 护栏 | CORE-180–CORE-217 |
| 启动顺序 | CORE-220–CORE-279、CORE-289、CORE-409–CORE-411、CORE-415 |
| 崩溃隔离与从快照恢复 | CORE-280–CORE-288 |
| 优雅退出与 kill 语义 | CORE-300–CORE-314 |
| refdata 变更(Drain + 重启) | CORE-320–CORE-326 |
| 多账户 | CORE-330、CORE-168、CORE-247、CORE-313 |
| HostProfile | CORE-340–CORE-343、CORE-419 |
| HeartbeatBlock | CORE-350–CORE-352 |
| 配置文件字段 | CORE-360–CORE-367、CORE-047、CORE-227–CORE-230、CORE-246、CORE-413、CORE-414、CORE-416 |
| 实现说明 | CORE-380–CORE-387、CORE-409、CORE-415–CORE-418、CORE-420 |
| 延迟与资源契约 | CORE-400–CORE-408、CORE-267 |
| 测试与验收 | 不适用:非规范 |
| 依赖 | CORE-390 |
| 延伸阅读 | 不适用:非规范 |
| 已知限制与后续工作 | 不适用:非规范 |
| 开发备忘 | 不适用:非规范 |

## 用例

约定(各用例的 `前置` 引用它们,不另抄):

- **配置构造器** `support::cfg::<主机类>_<实例>()`(`apps/hft-core/tests/support/cfg.rs`)按 README 的线程计划表、内存表、容量表与环容量表生成一份能通过步 1–2 的配置与 blob(refdata、`StrategyParams`、`RiskParams`、EngineConfig,SHA-256 已填),例如 `dev2_sim()`、`dev2_prod()`(两个下单 venue:`binance-usdm`、`okx-swap`)、`c4_prod()`(一个下单 venue)、`c8_prod()`(两个)、`c16_prod()`(四个)、`c16_sim()`、`c32_prod()`(六个)、`c32_sim()`。每个配置有 6 个 `[venues.*]`(三所 × 现货/USDT 永续),`prod` 的账户为 `bn-1`、`okx-1`(`is_prod` 随 `order_env`),`sim` 的账户为 `sim-a`、`sim-b`。`prod` 构造器默认 `order_env = "testnet"`、全部账户 `is_prod = false`,`feed_env` 在 `dev2` 为 `"testnet"`、其他主机类为 `"prod"`;`paths.release_manifest` 指向 `channel = "release"` 的测试清单;构造器不含 `endpoint_override`。用例只写相对构造器的改动。
- **测试主机清单** `test-<主机类>`(`tools/xtask` 的测试夹具,格式同 `deploy/hosts/<name>.toml`,`class` 为该主机类,全部下单 venue 启用,ENI-B2 不启用,除非用例另写)供 `cargo xtask gen-host --host` 使用。
- **主机夹具** `support::sysroot::<主机类>()`:hft-rt `SysRoot` 夹具目录,CPU 数、内存、`isolated`、cmdline 与该主机类一致;`proc` 用例在 CI 的 Linux 容器中以 `rt.host.mode = "dev"`、`host.on_mismatch = "degrade"` 运行,除非用例另写。
- **假交易所** `support::fake_venue`:hft-testkit 的假交易所经本机 TLS 端点接受 `venue-*` 协议;用到它的 `proc`/`integ` 用例另加 `venues.<v>.endpoint_override` 与 `test.extra_ca` 指向它(`prod` 实例用 `order_env = "testnet"`;`sim` 实例直接允许)。
- 错误行指 stderr 中形如 `hft-core: error[<码名>] <详情>` 的行([命令行与退出码](README.md#命令行与退出码));"错误列表"是这些行按出现顺序的码名序列。

### CORE-001 sequencer 的结束原因按表映射为退出码

- 规则:[命令行与退出码](README.md#命令行与退出码)
- 级别:table
- 里程碑:M3
- 前置:`main::exit_code_for(outcome, drain_timed_out)` 纯函数;无其他参数
- 实现:`apps/hft-core/tests/exit_codes.rs::core_001_done_cause_to_exit_code`

| 行 | 输入 | 期望 |
|---|---|---|
| CORE-001.1 | `Done{cause: Drain}`,未超时 | `ExitCode(0)` |
| CORE-001.2 | `Done{cause: ReplaceRefdata}`,未超时 | `ExitCode(75)` |
| CORE-001.3 | `Done{cause: StorageCritical}`,未超时 | `ExitCode(3)` |
| CORE-001.4 | `Done{cause: IdentityMismatch}`,未超时 | `ExitCode(2)` |
| CORE-001.5 | `Done{cause: Drain}`,`drain_timed_out = true` | `ExitCode(0)` |
| CORE-001.6 | `Done{cause: ReplaceRefdata}`,`drain_timed_out = true` | `ExitCode(75)` |
| CORE-001.7 | `Done{cause: StorageCritical}`,`drain_timed_out = true` | `ExitCode(3)` |
| CORE-001.8 | `Done{cause: IdentityMismatch}`,`drain_timed_out = true` | `ExitCode(2)` |
| CORE-001.9 | `Fatal(JournalFull)` | `Abort`(进程以 SIGABRT 结束,shell 观察到 134) |
| CORE-001.10 | `Fatal(ClockRange)` | `Abort` |
| CORE-001.11 | `Fatal(TimerOverflow)` | `Abort` |

### CORE-002 `--version` 打印版本信息与已注册策略集合并退出 0

- 规则:[命令行与退出码](README.md#命令行与退出码)、[策略装配](README.md#策略装配)
- 级别:proc
- 里程碑:M3
- 前置:生产 registry 的默认 feature 构建(`portfolio-carry` kind_id 1、`latency-probe` kind_id 5)
- 步骤:
  1. 运行 `hft-core --version`
- 期望:
  - 退出码 0;stdout 依次有 `version`、`git_sha`(40 个十六进制字符)、`target_cpu = neoverse-v1`(aarch64 构建)、schema 版本表、每个已注册策略一行 `(name, kind_id, version, schema_id, schema_version)`,行数 = `StrategySet::kinds().len()` = 2
  - 每行的 `(kind_id, schema_id, schema_version)` 与同一二进制 `schemas --out` 写出的 `index.json` 条目逐项相等
  - 没有读取任何配置文件(`strace -e openat` 中没有 `/etc/hft/`)
- 实现:`apps/hft-core/tests/cli.rs::core_002_version_output`

### CORE-003 `schemas --out` 的输出与金样逐字节相同

- 规则:[策略装配](README.md#策略装配)
- 级别:golden
- 里程碑:M3
- 输入:生产 registry 构建的二进制运行 `hft-core schemas --out <tmp>`
- 期望文件:`fixtures/strategy-schemas/`(每策略 `<kind_id>-<name>.schema.json` 与 `index.json`)
- 实现:`apps/hft-core/tests/schemas.rs::core_003_schemas_match_golden`

### CORE-004 `schemas --out` 的目录不可写时以 `Usage` 退出 2

- 规则:[命令行与退出码](README.md#命令行与退出码)
- 级别:proc
- 里程碑:M3
- 前置:`<tmp>/ro` 目录模式 `0555`,测试以非 root 运行
- 步骤:
  1. 运行 `hft-core schemas --out <tmp>/ro`
- 期望:
  - 退出码 2;stderr 恰好一行 `hft-core: error[Usage] …`,详情含路径 `<tmp>/ro`
  - `<tmp>/ro` 下没有新文件
- 实现:`apps/hft-core/tests/cli.rs::core_004_schemas_out_unwritable`

### CORE-005 命令行参数错误一律 `Usage` 退出 2

- 规则:[命令行与退出码](README.md#命令行与退出码)
- 级别:table
- 里程碑:M3
- 前置:`main::parse_args(argv)` 纯函数;`<cfg>` 为任一存在的配置路径
- 实现:`apps/hft-core/tests/cli.rs::core_005_usage_errors`

| 行 | 输入 argv | 期望 |
|---|---|---|
| CORE-005.1 | `--config <cfg> --foo` | `Err(Usage)`,详情含 `--foo` |
| CORE-005.2 | (空) | `Err(Usage)`,详情含 `--config` |
| CORE-005.3 | `--config <cfg> --recover-from-snapshot latest` | `Err(Usage)`,详情含 `--reason` |
| CORE-005.4 | `--config <cfg> --recover-from-snapshot 12a --reason x` | `Err(Usage)` |
| CORE-005.5 | `--config <cfg> --recover-from-snapshot -1 --reason x` | `Err(Usage)` |
| CORE-005.6 | `--config <cfg> --recover-from-snapshot 18446744073709551616 --reason x` | `Err(Usage)`(u64::MAX + 1) |
| CORE-005.7 | `--config <cfg> --recover-from-snapshot 18446744073709551615 --reason x` | `Ok(Recover{sel: AtSeq(18446744073709551615), reason: "x"})` |
| CORE-005.8 | `--config <cfg> --recover-from-snapshot latest --reason "rollback v0.4"` | `Ok(Recover{sel: Latest, reason: "rollback v0.4"})` |
| CORE-005.9 | `--config <cfg> --check --recover-from-snapshot latest --reason x` | `Err(Usage)` |
| CORE-005.10 | `--config <cfg> --check` | `Ok(Check)` |
| CORE-005.11 | `--config <cfg>` | `Ok(Run)` |
| CORE-005.12 | `schemas` | `Err(Usage)`,详情含 `--out` |

### CORE-006 步 1–2 的错误一次全部打印,第一条进 `sd_notify(STATUS)`

- 规则:[命令行与退出码](README.md#命令行与退出码)
- 级别:proc
- 里程碑:M3
- 前置:`dev2_sim()` 去掉 `shutdown.cancel_open_orders` 与 `startup.allow_missing_standby`,并把 `blobs.engine.sha256` 改成全 0;`NOTIFY_SOCKET` 指向测试套接字
- 步骤:
  1. 运行 `hft-core --config <cfg>`
- 期望:
  - 退出码 2;stderr 恰好三行错误,码名集合为 {`MissingField`(`shutdown.cancel_open_orders`)、`MissingField`(`startup.allow_missing_standby`)、`BlobHash`(`blobs.engine`)}
  - 测试套接字收到的最后一条 `STATUS=` 等于 stderr 的第一条错误行;没有收到 `READY=1`
  - journal 根下没有新段文件,`paths.heartbeat_shm` 不存在
- 实现:`apps/hft-core/tests/cli.rs::core_006_errors_printed_at_once`

### CORE-007 TOML 语法错误在解析时终止,错误列表只有一条

- 规则:[命令行与退出码](README.md#命令行与退出码)
- 级别:unit
- 里程碑:M3
- 前置:`dev2_sim()` 的 TOML 文本在第 3 行末尾追加一个未闭合的 `"`,另去掉 `shutdown.cancel_open_orders`
- 步骤:
  1. 调用 `config::load_and_validate(text, &blobs, &sysroot)`
- 期望:
  - 返回的错误列表长度 1,码名 `ConfigParse`,详情含行号 3;列表中没有 `MissingField`
- 实现:`apps/hft-core/tests/cli.rs::core_007_toml_syntax_error_single`

### CORE-008 未知键按 `deny_unknown_fields` 拒绝,错误列表只有一条

- 规则:[命令行与退出码](README.md#命令行与退出码)、[配置文件字段](README.md#配置文件字段)
- 级别:unit
- 里程碑:M3
- 前置:`dev2_sim()` 的 `[startup]` 表加一行 `budget_secs = 30`(拼错的键),其余不变
- 步骤:
  1. 调用 `config::load_and_validate`
- 期望:
  - 错误列表恰好一条 `ConfigParse`,详情含 `budget_secs`
- 实现:`apps/hft-core/tests/cli.rs::core_008_unknown_key_single`

### CORE-009 同时有退出 2 类与退出 3 类错误时以 2 退出且两类都打印

- 规则:[命令行与退出码](README.md#命令行与退出码)
- 级别:unit
- 里程碑:M3
- 前置:`c32_prod()` 去掉 `startup.allow_missing_standby`;主机夹具 `sysroot::c16()`(16 vCPU、32 GiB);`rt.host.mode = "prod"`、`host.on_mismatch = "refuse"`
- 步骤:
  1. 调用 `startup::run_steps_1_2(&cfg, &sysroot)` 并取退出码
- 期望:
  - 错误列表含 `MissingField`(`startup.allow_missing_standby`)与 `HostCheck`(详情含 `CLASS_MISMATCH`),共 2 条
  - 退出码 2
- 实现:`apps/hft-core/tests/cli.rs::core_009_exit2_wins_over_exit3`

### CORE-010 只有退出 3 类错误时以 3 退出

- 规则:[命令行与退出码](README.md#命令行与退出码)
- 级别:unit
- 里程碑:M3
- 前置:`c32_prod()` 不改;主机夹具 `sysroot::c16()`;`rt.host.mode = "prod"`、`host.on_mismatch = "refuse"`
- 步骤:
  1. 调用 `startup::run_steps_1_2`
- 期望:
  - 错误列表恰好一条 `HostCheck`(`CLASS_MISMATCH`);退出码 3
- 实现:`apps/hft-core/tests/cli.rs::core_010_exit3_only_host_errors`

### CORE-011 `--check` 全部通过时退出 0 且不产生任何运行副作用

- 规则:[命令行与退出码](README.md#命令行与退出码)
- 级别:proc
- 里程碑:M3
- 前置:`dev2_prod()`;`paths.secrets_dir` 指向不存在的目录;`paths.journal_ram`、`paths.journal_cold` 指向空目录;`DurableMeta` 目录预置 `engine_epoch = 7`
- 步骤:
  1. 在 `strace -f -e trace=openat,connect,mlockall,mmap` 下运行 `hft-core --config <cfg> --check`
- 期望:
  - 退出码 0;stderr 没有错误行
  - 两个 journal 根与 `paths.heartbeat_shm` 所在目录下没有新文件;`DurableMeta` 的 `engine_epoch` 仍为 7,`meta/` 目录 mtime 不变
  - strace 中没有 `connect`、没有 `mlockall`、没有对 `paths.secrets_dir` 的 `openat`、没有 `MAP_HUGETLB` 的 `mmap`
- 实现:`apps/hft-core/tests/check.rs::core_011_check_has_no_side_effects`

### CORE-012 `--check` 与真实启动对同一配置给出相同的错误行与退出码

- 规则:[命令行与退出码](README.md#命令行与退出码)
- 级别:proc
- 里程碑:M3
- 前置:`dev2_sim()` 的 `instance.name = "sim2"`,并去掉 `shutdown.cancel_open_orders`
- 步骤:
  1. 运行 `hft-core --config <cfg> --check`,记录 stderr 与退出码
  2. 运行 `hft-core --config <cfg>`,记录 stderr 与退出码
- 期望:
  - 两次退出码都是 2;两次的错误行逐行相同,码名为 `BadValue`(`instance.name`)与 `MissingField`(`shutdown.cancel_open_orders`)
- 实现:`apps/hft-core/tests/check.rs::core_012_check_matches_start`

### CORE-013 `--check` 不读、不删除秘密文件

- 规则:[命令行与退出码](README.md#命令行与退出码)
- 级别:proc
- 里程碑:M3
- 前置:`dev2_prod()`;`paths.secrets_dir` 下有 `bn-1.session`、`bn-1.rest`、`okx-1.key` 三个 `0400` 文件,记录其 SHA-256 与 inode
- 步骤:
  1. 运行 `hft-core --config <cfg> --check`
- 期望:
  - 退出码 0;三个文件仍在,SHA-256 与 inode 不变;`strace` 中没有对它们的 `openat` 与 `unlink`
- 实现:`apps/hft-core/tests/check.rs::core_013_check_keeps_secret_files`

### CORE-014 `--check` 只核对秘密引用:Live 账户缺 REST key 的引用报 `MissingCredential`

- 规则:[命令行与退出码](README.md#命令行与退出码)、[实例角色(prod 与 sim)](README.md#实例角色prod-与-sim)
- 级别:proc
- 里程碑:M3
- 前置:`dev2_prod()` 中账户 `bn-1` 的 `keys.binance-usdm` 只有 `session`、没有 `rest`;秘密目录不存在
- 步骤:
  1. 运行 `hft-core --config <cfg> --check`
- 期望:
  - 退出码 2;错误列表恰好一条 `MissingCredential`,详情含 `bn-1` 与 `binance-usdm`
- 实现:`apps/hft-core/tests/check.rs::core_014_check_secret_refs_only`

### CORE-015 退出码 2、3、4 之后 systemd 不重启

- 规则:[命令行与退出码](README.md#命令行与退出码)
- 级别:host
- 里程碑:M3
- 环境:dev2,`hft-core@sim` 已按 deploy 安装;联调窗口外;护栏守护 active
- 授权:`[需授权]`(在旧系统的生产主机上启停 hft 单元)
- 步骤:
  1. 以 drop-in 把 `hft-core@sim` 的配置换成去掉 `shutdown.cancel_open_orders` 的副本,`systemctl start hft-core@sim`,等 10 s
  2. 换成 `host.mem_budget_mib = 1` 的副本(步 3 退出 3),重复
  3. 换成冷副本段链被截断一段的 journal 目录(步 4 退出 4),重复;最后移除 drop-in、恢复原 journal
- 通过标准:三次 `systemctl show hft-core@sim -p ExecMainStatus -p NRestarts` 依次为 `2/0`、`3/0`、`4/0`,单元状态 `failed`;`journalctl -u hft-core@sim` 中各有一行对应码名(`MissingField`、`MemBudget`、`Journal`)
- 证据:`ops-log/<UTC 日期>-CORE-015.md`(三次 `systemctl show` 与 `journalctl` 摘录)
- 实现:`apps/hft-core/tests/host/CORE-015.sh`

### CORE-016 退出码 75 之后 systemd 立即重启一次

- 规则:[命令行与退出码](README.md#命令行与退出码)、[refdata 变更(Drain + 重启)](README.md#refdata-变更drain--重启)
- 级别:host
- 里程碑:M4
- 环境:dev2,`hft-core@sim` 运行中(`instance.config_source = "journal"`),闸门已全过
- 授权:`[需授权]`
- 步骤:
  1. 按 [release-and-deploy refdata 变更](../../docs/runbooks/release-and-deploy.md#refdata-变更) 以 Admin 发 `ReplaceRefdata`(只改一个 Sim 账户的 `fee_tier`)并确认
  2. 观察单元 30 s
- 通过标准:`ExecMainStatus=75` 的一次退出之后 `NRestarts` 加 1,重启间隔 ≤ 2 s;重启后新段引导块中的 `ConfigBlob{Refdata}` 版本 = 新版本
- 证据:`ops-log/<UTC 日期>-CORE-016.md`
- 实现:`apps/hft-core/tests/host/CORE-016.sh`

### CORE-017 SIGABRT 退出按 `on-failure` 重启,600 s 内第 4 次启动被 systemd 截停

- 规则:[命令行与退出码](README.md#命令行与退出码)、[崩溃隔离与从快照恢复](README.md#崩溃隔离与从快照恢复)
- 级别:host
- 里程碑:M3
- 环境:dev2,`hft-core@sim` 运行中
- 授权:`[需授权]`
- 步骤:
  1. `systemctl reset-failed hft-core@sim` 后 `systemctl restart hft-core@sim`(本窗口内第 1 次启动),等 `HeartbeatBlock.state = Running`
  2. `kill -ABRT <pid>`,等 5 s;在新 pid 上重复,共 3 次
  3. 等 5 s 后读单元状态
- 通过标准:前两次 abort 后各在 ≥ 1 s 后出现新 pid(`RestartSec=1`,即第 2、3 次启动);第 3 次 abort 之后单元为 `failed`,`systemctl show -p Result` 为 `start-limit-hit`,没有第 4 个 pid
- 证据:`ops-log/<UTC 日期>-CORE-017.md`
- 实现:`apps/hft-core/tests/host/CORE-017.sh`

### CORE-030 `StrategyParams` 引用未注册的 `kind_id` 时以 `UnknownStrategyKind` 退出 2

- 规则:[策略装配](README.md#策略装配)
- 级别:unit
- 里程碑:M3
- 前置:`dev2_sim()`;refdata 中实例 `strat-1` 的 `InstanceMeta.kind_id = 999`(默认 feature 的 registry 只有 `kind_id` 1 与 5);`StrategyParams` 中该实例的头 `kind_id = 999`
- 步骤:
  1. 调用 `config::load_and_validate`
- 期望:
  - 错误列表含 `UnknownStrategyKind`,详情含 999;退出码 2
- 实现:`apps/hft-core/tests/assembly.rs::core_030_unknown_strategy_kind`

### CORE-031 账户与策略实例的绑定必须一一对应

- 规则:[策略装配](README.md#策略装配)
- 级别:table
- 里程碑:M3
- 前置:`dev2_sim()`(账户 `sim-a` 绑定实例 `s1`,`sim-b` 绑定 `s2`,`StrategyParams` 各有一份)
- 实现:`apps/hft-core/tests/assembly.rs::core_031_binding_one_to_one`

| 行 | 改动 | 期望 |
|---|---|---|
| CORE-031.1 | 不改 | 无错误 |
| CORE-031.2 | 删除 `s2` 的 `StrategyParams` | `BindingMismatch`(详情 `sim-b`/`s2`) |
| CORE-031.3 | 再加一份 `(strategy = s1, account = sim-b)` 的参数 | `BindingMismatch`(详情 `s1`) |
| CORE-031.4 | 去掉 `sim-b`(连同它的 `[[accounts]]` 与 `s2` 的参数) | 无错误 |

### CORE-032 `non_prod_only` 的策略实例绑到 `is_prod = true` 的账户时以 `ProbeOnProdAccount` 退出 2

- 规则:[策略装配](README.md#策略装配)
- 级别:unit
- 里程碑:M3
- 前置:`c32_prod()` 的 `order_env = "prod"`(全部账户 `is_prod = true`);账户 `bn-1` 绑定 `latency-probe` 实例(描述符 `non_prod_only = true`)
- 步骤:
  1. 调用 `config::load_and_validate`
- 期望:
  - 错误列表含 `ProbeOnProdAccount`,详情含 `bn-1`;退出码 2
- 实现:`apps/hft-core/tests/assembly.rs::core_032_probe_on_prod_account`

### CORE-033 `non_prod_only` 的策略实例绑到测试网账户时通过

- 规则:[策略装配](README.md#策略装配)
- 级别:unit
- 里程碑:M3
- 前置:`c32_prod()` 的 `order_env = "testnet"`、`feed_env = "prod"`(全部账户 `is_prod = false`);`bn-1` 绑定 `latency-probe` 实例
- 步骤:
  1. 调用 `config::load_and_validate`
- 期望:
  - 错误列表为空
- 实现:`apps/hft-core/tests/assembly.rs::core_033_probe_on_testnet_ok`

### CORE-034 只在 registry 登记的新测试策略不改 hft-core 即可装配

- 规则:[策略装配](README.md#策略装配)
- 级别:proc
- 里程碑:M3
- 前置:解耦门槛的 registry 补丁副本(列表末尾加一行 CI 测试策略,kind_id 0xFF10,步骤归 tools `scaffold-gate`);`dev2_sim()` 的 `sim-b` 改绑该策略的实例
- 步骤:
  1. 以该特性构建 hft-core,运行 `--check`
  2. `git diff --stat -- apps/hft-core crates` 对比基线提交
- 期望:
  - 步骤 1 退出码 0;`--version` 的策略行数为 3,其中一行 `kind_id = 65296`(0xFF10)
  - 步骤 2 输出为空(hft-core 与引擎 crate 零改动)
- 实现:`apps/hft-core/tests/scaffold.rs::core_034_new_strategy_zero_core_change`

### CORE-035 ctl-gateway 的 `GetSchema` 与 `schemas --out` 来自同一策略集合

- 规则:[策略装配](README.md#策略装配)
- 级别:integ
- 里程碑:M3
- 前置:`dev2_sim()` 运行到步 8;ctl socket 以 Viewer 证书连接
- 步骤:
  1. 对每个 `kind_id` 发 `GetSchema`
  2. 同一二进制运行 `schemas --out <tmp>`
- 期望:
  - 每个响应的 schema 摘要等于 `<tmp>/index.json` 中同一 `kind_id` 的摘要;响应条数 = 2
- 实现:`apps/hft-core/tests/assembly.rs::core_035_get_schema_same_set`

### CORE-040 实例角色与账户类型不符时退出 2

- 规则:[实例角色(prod 与 sim)](README.md#实例角色prod-与-sim)
- 级别:table
- 里程碑:M3
- 前置:各行从所列构造器起;`config::load_and_validate`
- 实现:`apps/hft-core/tests/instance_role.rs::core_040_account_kind_by_role`

| 行 | 配置 | 期望错误列表 |
|---|---|---|
| CORE-040.1 | `dev2_sim()` 不改 | 空 |
| CORE-040.2 | `dev2_sim()` 的 `instance.name = "prod2"` | 含 `BadValue`(`instance.name`) |
| CORE-040.3 | `dev2_prod()` 的 refdata 加一个 Sim 账户 `sim-x` | 含 `SimAccountInProdInstance` |
| CORE-040.4 | `c32_prod()` 的 refdata 加一个 Sim 账户 `sim-x` | 含 `SimAccountInProdInstance` |
| CORE-040.5 | `dev2_sim()` 的 refdata 加一个 Live 账户 `sim-live`(`is_prod = false`) | 含 `LiveAccountInSimInstance` |
| CORE-040.6 | `c32_sim()` 的 refdata 加一个 Live 账户 `sim-live` | 含 `LiveAccountInSimInstance` |

### CORE-041 `is_prod`、`order_env`、`feed_env` 与主机类的组合

- 规则:[实例角色(prod 与 sim)](README.md#实例角色prod-与-sim)、[配置文件字段](README.md#配置文件字段)
- 级别:table
- 里程碑:M3
- 前置:各行从所列构造器起;主机夹具与 `host.class` 一致
- 实现:`apps/hft-core/tests/instance_role.rs::core_041_env_combinations`

| 行 | 配置 | 期望错误列表 |
|---|---|---|
| CORE-041.1 | `dev2_prod()`(`order_env = feed_env = "testnet"`,全部 `is_prod = false`) | 空 |
| CORE-041.2 | `c32_prod()`,`order_env = "testnet"`,`bn-1.is_prod = true` | 恰好 {`EnvAccountMismatch`} |
| CORE-041.3 | `c32_prod()`,`order_env = "prod"`,`okx-1.is_prod = false` | 恰好 {`EnvAccountMismatch`} |
| CORE-041.4 | `c32_prod()`,`order_env = "prod"`、`feed_env = "testnet"` | 含 `EnvCombo` |
| CORE-041.5 | `c32_prod()`,`order_env = "testnet"`、`feed_env = "testnet"` | 空 |
| CORE-041.6 | `c32_prod()`,`order_env = "testnet"`,不写 `feed_env` | 空(`feed_env` 取 `prod`) |
| CORE-041.7 | `dev2_prod()`,`order_env = "prod"`,全部 `is_prod = true` | 恰好 {`InstanceHostClass`、`ProdAccountOnDev2`} |
| CORE-041.8 | `dev2_prod()`,`order_env = "testnet"`,`bn-1.is_prod = true` | 恰好 {`EnvAccountMismatch`、`ProdAccountOnDev2`} |
| CORE-041.9 | `c4_prod()`,`order_env = "prod"`,全部 `is_prod = true` | 恰好 {`InstanceHostClass`} |
| CORE-041.10 | `c8_prod()`,`order_env = "prod"`,全部 `is_prod = true` | 空 |
| CORE-041.11 | `c32_sim()` 加 `instance.order_env = "testnet"` | 含 `EnvCombo` |
| CORE-041.12 | `c32_sim()` 加 `instance.feed_env = "testnet"` | 含 `EnvCombo` |
| CORE-041.13 | `dev2_sim()` 但 `host.class = "c8"`(主机夹具 `c8`,EngineConfig 按 `c8`) | 含 `InstanceHostClass` |
| CORE-041.14 | `dev2_sim()` 但 `host.class = "c4"`(同上按 `c4`) | 含 `InstanceHostClass` |
| CORE-041.15 | `c16_sim()` | 空 |
| CORE-041.16 | `c32_prod()` 不写 `instance.order_env` | 含 `MissingField`(`instance.order_env`) |

### CORE-042 账户名前缀按实例角色判定

- 规则:[实例角色(prod 与 sim)](README.md#实例角色prod-与-sim)
- 级别:table
- 里程碑:M3
- 前置:改动同时作用于 refdata `AccountMeta.label` 与 `[[accounts]].label`;`config::load_and_validate`
- 实现:`apps/hft-core/tests/instance_role.rs::core_042_account_name_prefix`

| 行 | 配置 | 期望错误列表 |
|---|---|---|
| CORE-042.1 | `dev2_sim()`,账户名 `sim-a` | 空 |
| CORE-042.2 | `dev2_sim()`,`sim-a` 改名 `research-a` | 恰好 {`AccountNamePrefix`} |
| CORE-042.3 | `dev2_sim()`,改名 `SIM-a`(大小写不同) | 恰好 {`AccountNamePrefix`} |
| CORE-042.4 | `dev2_sim()`,改名 `sima`(缺 `-`) | 恰好 {`AccountNamePrefix`} |
| CORE-042.5 | `dev2_prod()`,`bn-1` 改名 `sim-bn` | 恰好 {`AccountNamePrefix`} |
| CORE-042.6 | `dev2_prod()`,`bn-1` 改名 `simbn` | 空 |

### CORE-043 `[[accounts]]` 必须与 refdata `AccountMeta` 一致

- 规则:[实例角色(prod 与 sim)](README.md#实例角色prod-与-sim)、[多账户](README.md#多账户)
- 级别:table
- 里程碑:M3
- 前置:`dev2_prod()`;只改配置文件,refdata 不变
- 实现:`apps/hft-core/tests/instance_role.rs::core_043_accounts_match_refdata`

| 行 | 改动 | 期望错误列表 |
|---|---|---|
| CORE-043.1 | `bn-1` 的 `[[accounts]].label` 写成 `bn-2` | 恰好 {`AccountMismatch`} |
| CORE-043.2 | `bn-1` 的 `[[accounts]].id` 由 1 改为 9(refdata 无 9) | 恰好 {`AccountMismatch`} |
| CORE-043.3 | 删除 `okx-1` 的 `[[accounts]]` 条目 | 含 `AccountMismatch`(详情 `okx-1`) |

### CORE-044 秘密与 key 只属于 `prod` 实例的 Live 账户

- 规则:[实例角色(prod 与 sim)](README.md#实例角色prod-与-sim)
- 级别:table
- 里程碑:M3
- 前置:`config::load_and_validate`
- 实现:`apps/hft-core/tests/instance_role.rs::core_044_secrets_by_role`

| 行 | 配置 | 期望错误列表 |
|---|---|---|
| CORE-044.1 | `dev2_sim()` 加 `paths.secrets_dir = "/run/hft/core-sim/secrets"` | 恰好 {`SecretsInSim`} |
| CORE-044.2 | `dev2_sim()` 的 `sim-a` 加 `keys.binance-usdm = {session = "x", rest = "x"}` | 恰好 {`SecretsInSim`} |
| CORE-044.3 | `dev2_prod()` 去掉 `paths.secrets_dir` | 恰好 {`MissingField`(`paths.secrets_dir`)} |
| CORE-044.4 | `dev2_prod()` 的 `okx-1` 去掉 `keys.okx-swap` | 恰好 {`MissingCredential`} |
| CORE-044.5 | `dev2_prod()` 的 `okx-1` 的 `keys.okx-swap = {session = "okx-1.key", rest = "okx-1.key"}`(同一把) | 空 |
| CORE-044.6 | `dev2_prod()` 的 `bn-1` 去掉 `keys.binance-usdm.session` | 恰好 {`MissingCredential`} |

### CORE-045 `[sim]` 只属于 `sim` 实例,`endpoint_override` 只属于测试网配置

- 规则:[实例角色(prod 与 sim)](README.md#实例角色prod-与-sim)、[配置文件字段](README.md#配置文件字段)
- 级别:table
- 里程碑:M3
- 前置:`config::load_and_validate`
- 实现:`apps/hft-core/tests/instance_role.rs::core_045_sim_section_and_overrides`

| 行 | 配置 | 期望错误列表 |
|---|---|---|
| CORE-045.1 | `dev2_prod()` 加 `[sim] seed = 1` | 恰好 {`SimSectionInProd`} |
| CORE-045.2 | `dev2_prod()` 加一项 `[[sim.latency_models]]` | 恰好 {`SimSectionInProd`} |
| CORE-045.3 | `dev2_sim()` 去掉 `sim.seed` | 恰好 {`MissingField`(`sim.seed`)} |
| CORE-045.4 | `dev2_sim()` 去掉全部 `[[sim.latency_models]]` | 恰好 {`MissingField`(`sim.latency_models`)} |
| CORE-045.5 | `c32_prod()`,`order_env = "prod"`,加 `venues.binance-usdm.endpoint_override` | 恰好 {`EndpointOverrideInProd`} |
| CORE-045.6 | `c32_prod()`,`order_env = "prod"`,加 `test.extra_ca` | 恰好 {`EndpointOverrideInProd`} |
| CORE-045.7 | `c32_prod()`,`order_env = "testnet"`,加 `venues.binance-usdm.endpoint_override` 与 `test.extra_ca` | 空 |
| CORE-045.8 | `dev2_sim()` 加 `venues.binance-usdm.endpoint_override` 与 `test.extra_ca` | 空 |

### CORE-046 `dev` 构建不得配 `is_prod = true` 的账户,只读本实例所属激活组的清单

- 规则:[实例角色(prod 与 sim)](README.md#实例角色prod-与-sim)
- 级别:table
- 里程碑:M3
- 前置:`<tmp>/current-core/RELEASE.json` 与 `<tmp>/current-sim/RELEASE.json` 两份清单,`channel` 按行设定;`prod` 配置的 `paths.release_manifest` 指向前者,`sim` 的指向后者
- 实现:`apps/hft-core/tests/instance_role.rs::core_046_release_channel`

| 行 | 配置与清单 | 期望错误列表 |
|---|---|---|
| CORE-046.1 | `c32_prod()`、`order_env = "prod"`;core 组 `channel = "dev"` | 恰好 {`DevBuildWithProdAccount`} |
| CORE-046.2 | `c32_prod()`、`order_env = "testnet"`;core 组 `channel = "dev"` | 空 |
| CORE-046.3 | `c32_prod()`、`order_env = "prod"`;core 组 `release`,sim 组 `dev` | 空 |
| CORE-046.4 | `c32_sim()`;sim 组 `dev`,core 组 `release` | 空 |
| CORE-046.5 | `c32_prod()`、`order_env = "prod"`;`paths.release_manifest` 指向不存在的文件 | 恰好 {`FileRead`} |

### CORE-047 `paths.release_manifest` 的默认值按实例取所属激活组

- 规则:[配置文件字段](README.md#配置文件字段)
- 级别:table
- 里程碑:M3
- 前置:配置不写 `paths.release_manifest`;`config::resolved(&cfg)`
- 实现:`apps/hft-core/tests/defaults.rs::core_047_release_manifest_default`

| 行 | 配置 | 期望 |
|---|---|---|
| CORE-047.1 | `c32_prod()` | `/opt/hft/current-core/RELEASE.json` |
| CORE-047.2 | `c32_sim()` | `/opt/hft/current-sim/RELEASE.json` |

### CORE-048 `sim` 实例无 key、无 gw 也能启动并过闸门,全部路径在 `sim` 名下

- 规则:[实例角色(prod 与 sim)](README.md#实例角色prod-与-sim)
- 级别:proc
- 里程碑:M4
- 前置:`dev2_sim()`;feed 指向假交易所行情端点;没有 `paths.secrets_dir` 目录;kill-switch 不运行;`startup.gate_timeout_s = 15`
- 步骤:
  1. 启动 `hft-core --config <cfg>`,等 `HeartbeatBlock.state = Running`
- 期望:
  - 15 s 内每个 Sim 账户写出 `Note{StartupGate}` 且 `released = 1`
  - `/proc/<pid>/task/*/comm` 中没有以 `gw-` 开头的线程,也没有承载 `acct-fetch` 的线程
  - journal 段只在 `<journal_ram>/sim/` 与 `<cold_root>/sim/`,心跳块在 `/dev/shm/hft/sim/heartbeat`,ctl socket 在 `/run/hft/core-sim/`;strace 中没有 `openat` 任何 `secrets` 路径
  - `HkLine.instance_id = 2`
- 实现:`apps/hft-core/tests/instance_role.rs::core_048_sim_instance_boots_without_keys`

### CORE-049 `prod` 与 `sim` 两个实例同机同时运行时资源互不相交

- 规则:[实例角色(prod 与 sim)](README.md#实例角色prod-与-sim)
- 级别:proc
- 里程碑:M4
- 前置:`dev2_prod()` 与 `dev2_sim()` 在同一容器内各起一个进程,各用自己的 journal、冷副本、ctl socket 与心跳路径;`prod` 连假交易所
- 步骤:
  1. 两个实例都到 `Running`
  2. 对 `prod` 发 SIGKILL,等 5 s
- 期望:
  - 步骤 1 后两份 journal 的 `seq` 段文件不在同一目录;两个 ctl socket 路径不同;`prod` 的 `HkLine.instance_id = 1`,`sim` 的 = 2
  - 步骤 2 后 `sim` 的心跳计数在这 5 s 内持续前进,`sim` 的 journal 中没有 `Drain`;`sim` 的 `HeartbeatBlock.state` 仍为 `Running`
- 实现:`apps/hft-core/tests/instance_role.rs::core_049_two_instances_disjoint`

### CORE-050 `sim` 实例的 journal 经 `replayer verify --instance sim` 逐字节通过

- 规则:[实例角色(prod 与 sim)](README.md#实例角色prod-与-sim)
- 级别:replay
- 里程碑:M4
- 输入:CORE-048 运行 10 min 后封存的 `sim` 实例 `seq` journal(指针 `fixtures/replay/core-sim-10min.pointer.toml`)
- 期望:`replayer verify --instance sim` 退出 0,报告的不一致记录数为 0
- 实现:`apps/hft-core/tests/replay.rs::core_050_sim_journal_verifies`

### CORE-060 `[[threads]]` 语法错误归线程计划第 0 条

- 规则:[线程计划](README.md#线程计划)
- 级别:table
- 里程碑:M3
- 前置:从 `dev2_sim()` 起,只改 `[[threads]]`;`config::load_and_validate`
- 实现:`apps/hft-core/tests/thread_plan.rs::core_060_thread_syntax`

| 行 | 改动 | 期望错误列表 |
|---|---|---|
| CORE-060.1 | 线程 `hk` 改名为 15 字节的 `housekeeping-01` | 空 |
| CORE-060.2 | 改名为 16 字节的 `housekeeping-012` | 恰好 {`ThreadPlan{0}`} |
| CORE-060.3 | 线程 `adv` 改名为 `hk`(与已有线程重名) | 恰好 {`ThreadPlan{0}`} |
| CORE-060.4 | `persist` 线程 `core = "none"`、`nice = 10` | 空 |
| CORE-060.5 | `seq` 线程(`core = 1`)加 `nice = 5` | 恰好 {`ThreadPlan{0}`} |
| CORE-060.6 | `warm` 线程的 tasks 加 `feed-xyz:binance-usdm` | 恰好 {`ThreadPlan{0}`} |
| CORE-060.7 | `feed` 线程的 tasks 加 `feed:gate-futures`(`[venues]` 无此键) | 恰好 {`ThreadPlan{0}`} |
| CORE-060.8 | `seq` 线程 `idle = "Busy"` | 恰好 {`BadValue`} |
| CORE-060.9 | `seq` 线程 `core = 65536` | 恰好 {`ConfigParse`} |
| CORE-060.10 | `seq` 线程 `tasks = []` | 恰好 {`ThreadPlan{0}`} |

### CORE-061 加载校验第 1 条:必需任务与按条件存在的任务

- 规则:[线程计划](README.md#线程计划)
- 级别:table
- 里程碑:M3
- 前置:各行从所列构造器起,只改 `[[threads]]` 与所列的 refdata/配置;`config::load_and_validate`
- 实现:`apps/hft-core/tests/thread_plan.rs::core_061_required_tasks`

| 行 | 改动 | 期望错误列表 |
|---|---|---|
| CORE-061.1 | `dev2_sim()` 不改 | 空 |
| CORE-061.2 | `dev2_sim()` 去掉 `sequencer`(连同 `seq` 线程) | 含 `ThreadPlan{1}` |
| CORE-061.3 | `dev2_sim()` 去掉 `housekeeping` | 含 `ThreadPlan{1}` |
| CORE-061.4 | `dev2_sim()` 去掉 `persist` | 含 `ThreadPlan{1}` |
| CORE-061.5 | `dev2_sim()` 去掉 `ctl-gateway` | 含 `ThreadPlan{1}` |
| CORE-061.6 | `dev2_sim()` 去掉 `conn-builder` | 含 `ThreadPlan{1}` |
| CORE-061.7 | `dev2_sim()` 去掉 `snap-fetch` | 含 `ThreadPlan{1}` |
| CORE-061.8 | `dev2_sim()` 去掉 `feed:okx-spot`(`[venues.okx-spot]` 仍在) | 含 `ThreadPlan{1}` |
| CORE-061.9 | `dev2_sim()` 的 `feed:okx-spot` 换成只有 `feed-bbo:okx-spot` | 含 `ThreadPlan{1}` |
| CORE-061.10 | `dev2_sim()` 在 `feed:okx-spot` 之外再加 `feed-bbo:okx-spot` 与 `feed-md:okx-spot` | 含 `ThreadPlan{1}` |
| CORE-061.11 | `dev2_sim()` 的 `feed:okx-spot` 换成 `feed-bbo:okx-spot` + `feed-md:okx-spot`(同一线程 `feed`) | 空 |
| CORE-061.12 | `dev2_sim()` 把 `ctl-gateway` 同时写进 `warm` 与 `adv` 两个线程 | 含 `ThreadPlan{1}` |
| CORE-061.13 | `dev2_prod()` 去掉 `gw-okx-swap` 线程(`okx-1` 仍在 `okx-swap` 交易) | 含 `ThreadPlan{1}` |
| CORE-061.14 | `dev2_prod()` 去掉 `acct-fetch` | 含 `ThreadPlan{1}` |
| CORE-061.15 | `dev2_sim()` 加线程 `gw-x`(`gw:binance-usdm`) | 含 `ThreadPlan{1}` |
| CORE-061.16 | `dev2_sim()` 在 `warm` 加 `acct-fetch` | 含 `ThreadPlan{1}` |
| CORE-061.17 | `dev2_sim()` 去掉 `sim-gw`(连同 `sim` 线程) | 含 `ThreadPlan{1}` |
| CORE-061.18 | `dev2_prod()` 加线程 `sim`(`sim-gw`) | 含 `ThreadPlan{1}` |
| CORE-061.19 | `dev2_sim()` 去掉 `advisor`(两个实例都绑带 `Model` 的 `portfolio-carry`) | 含 `ThreadPlan{1}` |
| CORE-061.20 | `dev2_sim()` 改为两个实例都绑 `NoModel` 的 `latency-probe`,并去掉 `advisor` | 空 |
| CORE-061.21 | `dev2_sim()` 加 `chain.enabled = true` 而无 `ext:` 任务 | 含 `ThreadPlan{1}` |

### CORE-062 加载校验第 2 条:`sequencer` 独占一个线程

- 规则:[线程计划](README.md#线程计划)
- 级别:unit
- 里程碑:M3
- 前置:`dev2_sim()` 的 `seq` 线程 tasks 改为 `["sequencer", "ctl-gateway"]`,`warm` 去掉 `ctl-gateway`
- 步骤:
  1. 调用 `layout::validate(&plan, &ctx)`
- 期望:
  - 错误列表含 `ThreadPlan{2}`(另因 warm 与热任务同线程含 `ThreadPlan{3}`),不含其他条号
- 实现:`apps/hft-core/tests/thread_plan.rs::core_062_sequencer_owns_thread`

### CORE-063 加载校验第 3 条:warm 任务不与热任务同线程

- 规则:[线程计划](README.md#线程计划)
- 级别:table
- 里程碑:M3
- 前置:`c32_prod()`(`chain.enabled = false`,行 .4 另设);只改所列线程;`layout::validate`
- 实现:`apps/hft-core/tests/thread_plan.rs::core_063_warm_not_with_hot`

| 行 | 改动 | 期望错误列表 |
|---|---|---|
| CORE-063.1 | `feed-md-bn` 加 `snap-fetch`(从 `warm` 移来) | 恰好 {`ThreadPlan{3}`} |
| CORE-063.2 | `feed-bbo-okx` 加 `ctl-gateway`(从 `warm` 移来) | 恰好 {`ThreadPlan{3}`} |
| CORE-063.3 | `feed-md-bg` 加 `advisor`(去掉 `adv` 线程) | 恰好 {`ThreadPlan{3}`} |
| CORE-063.4 | `chain.enabled = true`,新线程 `ext`(`core = 18`、`Park{300}`)承载 `ext:bsc` 与 `persist`(去掉原 `persist` 线程) | 含 `ThreadPlan{3}` |

### CORE-064 加载校验第 4 条:每个 `gw:` 独占线程(含 dev2)

- 规则:[线程计划](README.md#线程计划)
- 级别:table
- 里程碑:M3
- 前置:各行从所列构造器起;`layout::validate`
- 实现:`apps/hft-core/tests/thread_plan.rs::core_064_gw_owns_thread`

| 行 | 改动 | 期望错误列表 |
|---|---|---|
| CORE-064.1 | `dev2_prod()` 不改(`gw-bn-usdm`、`gw-okx-swap` 各一个 `gw:`) | 空 |
| CORE-064.2 | `dev2_prod()` 的 `gw-bn-usdm` 加 `feed:binance-usdm`(从 `feed` 移来) | 恰好 {`GwSharesThread`} |
| CORE-064.3 | `dev2_prod()` 把 `gw:okx-swap` 移进 `gw-bn-usdm`(去掉 `gw-okx-swap`) | 恰好 {`GwSharesThread`} |
| CORE-064.4 | `dev2_prod()` 的 `gw-bn-usdm` 加 `acct-fetch`(从 `warm` 移来) | 恰好 {`GwSharesThread`、`ThreadPlan{3}`} |
| CORE-064.5 | `c32_prod()` 把 `gw:okx-spot` 移进 `gw-okx-swap` | 含 `GwSharesThread` |

### CORE-065 加载校验第 5 条:`persist` 独占,`sim-gw` 不与 advisor/persist 同线程,升级主机类的 persist 核约束

- 规则:[线程计划](README.md#线程计划)
- 级别:table
- 里程碑:M3
- 前置:各行从所列构造器起(`c32_prod()`:`host.hk_cpus = [0]`、`host.irq_cpus = {eni_a = 0, eni_b = 1, eni_c = 22}`);`layout::validate`
- 实现:`apps/hft-core/tests/thread_plan.rs::core_065_persist_and_sim_gw`

| 行 | 改动 | 期望错误列表 |
|---|---|---|
| CORE-065.1 | `dev2_sim()` 的 `persist` 线程加 `ctl-gateway` | 恰好 {`ThreadPlan{5}`} |
| CORE-065.2 | `dev2_sim()` 把 `sim-gw` 移进 `adv` 线程 | 恰好 {`ThreadPlan{5}`} |
| CORE-065.3 | `dev2_sim()` 把 `sim-gw` 移进 `persist` 线程 | 含 `ThreadPlan{5}` |
| CORE-065.4 | `c32_prod()` 的 `warm` 线程 `core` 由 3 改为 2(与 `persist` 同核) | 恰好 {`ThreadPlan{5}`} |
| CORE-065.5 | `c32_prod()` 的 `persist` 线程 `core` 由 2 改为 0(hk 核) | 恰好 {`ThreadPlan{5}`} |
| CORE-065.6 | `c32_prod()` 的 `persist` 线程 `core` 改为 1(ENI-B IRQ 核) | 恰好 {`ThreadPlan{5}`、`ThreadPlan{6}`} |
| CORE-065.7 | `c32_prod()` 的 `adv` 线程 `core` 由 4 改为 2 | 恰好 {`ThreadPlan{5}`} |
| CORE-065.8 | `c16_prod()` 不改(`warm`/`adv` 同在核 3,`persist` 在核 2) | 空 |
| CORE-065.9 | `dev2_prod()` 不改(`persist` 线程 `core = "none"`) | 空 |

### CORE-066 加载校验第 6 条:Spin 绑核、热线程与 IRQ 专核

- 规则:[线程计划](README.md#线程计划)
- 级别:table
- 里程碑:M3
- 前置:`c32_prod()`(`host.hot_cpus = 5-17`);行 .6、.7 的 `c32_prod_eni_b2()` 另设 `host.irq_cpus.eni_b2 = 28`;`layout::validate`
- 实现:`apps/hft-core/tests/thread_plan.rs::core_066_spin_and_irq_cores`

| 行 | 改动 | 期望错误列表 |
|---|---|---|
| CORE-066.1 | `c32_prod()` 不改 | 空 |
| CORE-066.2 | `feed-md-bg` 线程 `core = "none"`(仍 `Spin`) | 含 `ThreadPlan{6}` |
| CORE-066.3 | `gw-bg-spot` 线程 `core` 由 17 改为 16(与 `gw-bg-fut` 同核) | 含 `ThreadPlan{6}` |
| CORE-066.4 | `feed-md-okx` 线程 `idle = { SpinThenPark = { spin_us = 20, park_us = 1000 } }` | 含 `ThreadPlan{6}` |
| CORE-066.5 | `adv` 线程 `core` 由 4 改为 1(ENI-B IRQ 核) | 恰好 {`ThreadPlan{6}`} |
| CORE-066.6 | `c32_prod_eni_b2()` 的 `adv` 线程 `core` 改为 28 | 恰好 {`ThreadPlan{6}`} |
| CORE-066.7 | `c32_prod()`(未启用 ENI-B2)的 `adv` 线程 `core` 改为 28 | 空 |
| CORE-066.8 | `c16_sim()` 不改(`hk`/`persist`/`warm`/`adv` 在 ENI-C IRQ 核 13 上;ENI-C 的 IRQ 核不在第 6 条内) | 空 |
| CORE-066.9 | `c32_sim()` 的 `hk` 线程 `core` 改为 1 | 含 `ThreadPlan{6}` |

### CORE-067 加载校验第 7 条:dev2 无 Spin;全部主机类 `park_us × 4 < hb_stale_ms × 1000`(单位换算)

- 规则:[线程计划](README.md#线程计划)
- 级别:table
- 里程碑:M3
- 前置:各行设定 `watchdog.hb_stale_ms` 与所改线程的 `idle`;`layout::validate`
- 实现:`apps/hft-core/tests/thread_plan.rs::core_067_no_spin_on_dev2_and_park_bound`

| 行 | 配置 | 期望错误列表 |
|---|---|---|
| CORE-067.1 | `dev2_sim()`,`seq` 线程 `idle = "Spin"` | 恰好 {`ThreadPlan{7}`、`ThreadPlan{8}`} |
| CORE-067.2 | `dev2_prod()`,`gw-bn-usdm` 线程 `idle = "Spin"` | 恰好 {`ThreadPlan{7}`} |
| CORE-067.3 | `dev2_sim()`,`hb_stale_ms = 1000`,`hk` 线程 `Park{park_us = 249999}` | 空 |
| CORE-067.4 | 同上,`Park{park_us = 250000}` | 恰好 {`ThreadPlan{7}`} |
| CORE-067.5 | 同上,`Park{park_us = 250001}` | 恰好 {`ThreadPlan{7}`} |
| CORE-067.6 | `dev2_sim()`,`hb_stale_ms = 2000`,`hk` 线程 `Park{park_us = 499999}` | 空 |
| CORE-067.7 | 同上,`Park{park_us = 500000}` | 恰好 {`ThreadPlan{7}`} |
| CORE-067.8 | `dev2_sim()`,`hb_stale_ms = 1000`,`seq` 线程 `SpinThenPark{spin_us = 20, park_us = 250000}` | 恰好 {`ThreadPlan{7}`} |
| CORE-067.9 | `c32_prod()`,`hb_stale_ms = 1000`,`warm` 线程 `Park{park_us = 250000}` | 恰好 {`ThreadPlan{7}`} |
| CORE-067.10 | `c32_prod()`,`hb_stale_ms = 1000`(`hk` 为 `Park{5000}`、`warm` 为 `Park{300}`) | 空 |
| CORE-067.11 | `dev2_sim()`,`hb_stale_ms = 1000`(全部 `park_us` ≤ 10000) | 空 |

### CORE-068 加载校验第 8 条:`sim` 实例不 Spin、不绑 `host.hot_cpus` 内的核

- 规则:[线程计划](README.md#线程计划)
- 级别:table
- 里程碑:M3
- 前置:`c32_sim()`(`host.hot_cpus = 5-17`,与同机 `prod` 相同);`layout::validate`
- 实现:`apps/hft-core/tests/thread_plan.rs::core_068_sim_off_hot_cores`

| 行 | 改动 | 期望错误列表 |
|---|---|---|
| CORE-068.1 | 不改(核 23–27,全部 `Park`) | 空 |
| CORE-068.2 | `seq` 线程 `core` 由 23 改为 5 | 恰好 {`SimOnHotCore`} |
| CORE-068.3 | `seq` 线程 `core` 改为 17 | 恰好 {`SimOnHotCore`} |
| CORE-068.4 | `seq` 线程 `core` 改为 24(与 `feed` 同核,两者都是 `Park`) | 空 |
| CORE-068.5 | `seq` 线程 `idle = "Spin"`(核 23) | 恰好 {`ThreadPlan{8}`} |
| CORE-068.6 | `adv` 线程 `core = "none"` | 空 |

### CORE-069 加载校验第 9 条:线程数 ≤ 30

- 规则:[线程计划](README.md#线程计划)、[HeartbeatBlock](README.md#heartbeatblock)
- 级别:table
- 里程碑:M3
- 前置:`c32_prod()`(17 个线程 + 加的线程;加的线程为 `core = "none"`、`Park{10000}`、`nice = 10`,各承载从 `warm` 拆出的一个 warm 任务或一个 `feed-md:` 拆分);`layout::validate`
- 实现:`apps/hft-core/tests/thread_plan.rs::core_069_thread_count_limit`

| 行 | 线程总数 | 期望错误列表 |
|---|---|---|
| CORE-069.1 | 29 | 不含 `ThreadPlan{9}` |
| CORE-069.2 | 30 | 不含 `ThreadPlan{9}` |
| CORE-069.3 | 31 | 含 `ThreadPlan{9}` |

### CORE-070 加载校验第 10 条:`housekeeping` 不与可阻塞任务同线程且 `nice = 0`

- 规则:[线程计划](README.md#线程计划)
- 级别:table
- 里程碑:M3
- 前置:`dev2_sim()`;`layout::validate`
- 实现:`apps/hft-core/tests/thread_plan.rs::core_070_housekeeping_thread`

| 行 | 改动 | 期望错误列表 |
|---|---|---|
| CORE-070.1 | `hk` 线程加 `persist`(去掉 `persist` 线程) | 含 `ThreadPlan{10}` |
| CORE-070.2 | `hk` 线程 `nice = 5` | 恰好 {`ThreadPlan{10}`} |
| CORE-070.3 | `hk` 线程加 `ctl-gateway`(从 `warm` 移来),`nice = 0` | 空 |

### CORE-071 加载校验第 11 条:warm 任务的 `poll_once` 不阻塞

- 规则:[线程计划](README.md#线程计划)
- 级别:proc
- 里程碑:M3
- 前置:`dev2_prod()` 连假交易所,到 `Running` 后行情静止、无命令、无快照请求;`strace -f -T` 附着到 `warm`、`adv`、`sim`(`dev2_sim()` 另起一次)线程
- 步骤:
  1. 采集 10 s
- 期望:
  - 这些线程的系统调用中,除 park 用的 `futex`/`epoll_pwait`(超时 ≤ 该线程 `park_us`)之外,没有超时为无穷或 > `park_us` 的阻塞调用,也没有 `nanosleep`/`clock_nanosleep`
  - 每次 park 的实际时长 ≤ `park_us` + 2 ms
- 实现:`apps/hft-core/tests/thread_plan.rs::core_071_warm_poll_never_blocks`

### CORE-072 dev2 的 `hft-core@*` 单元 `WatchdogSec` ≥ 10 s

- 规则:[线程计划](README.md#线程计划)
- 级别:host
- 里程碑:M3
- 环境:dev2,按 deploy 安装后
- 授权:无(只读 `systemctl show`)
- 步骤:
  1. `systemctl show hft-core@sim hft-core@prod -p WatchdogUSec`
- 通过标准:两个单元的 `WatchdogUSec` 都 ≥ `10s`
- 证据:`ops-log/<UTC 日期>-CORE-072.md`
- 实现:`apps/hft-core/tests/host/CORE-072.sh`

### CORE-073 加载校验第 12 条:`host.hot_cpus` 等于 `prod` 全部 Spin 线程的核集合

- 规则:[线程计划](README.md#线程计划)、[配置文件字段](README.md#配置文件字段)
- 级别:table
- 里程碑:M3
- 前置:`layout::validate`
- 实现:`apps/hft-core/tests/thread_plan.rs::core_073_hot_cpus_equal_spin_cores`

| 行 | 配置 | 期望错误列表 |
|---|---|---|
| CORE-073.1 | `c32_prod()`,`host.hot_cpus = 5-17` | 空 |
| CORE-073.2 | `c32_prod()`,`host.hot_cpus = 5-16` | 恰好 {`ThreadPlan{12}`} |
| CORE-073.3 | `c32_prod()`,`host.hot_cpus = 5-18` | 恰好 {`ThreadPlan{12}`} |
| CORE-073.4 | `c4_prod()`,`host.hot_cpus = 1-3` | 空 |
| CORE-073.5 | `c4_prod()`,`host.hot_cpus = 1-2` | 恰好 {`ThreadPlan{12}`} |
| CORE-073.6 | `c16_prod()`,`host.hot_cpus = 4-11` | 空 |

### CORE-074 `gen-host` 按线程计划生成 GRUB 参数、IRQ 计划与 slice 核集合

- 规则:[线程计划](README.md#线程计划)、[主机资源隔离(slice、核与出口 IP)](README.md#主机资源隔离slice核与出口-ip)
- 级别:table
- 里程碑:M3
- 前置:各行用 deploy 的主机类模板(线程计划即本页各主机类表,全部下单 venue 启用);输出为 `cargo xtask gen-host --host test-<主机类>` 的生成物
- 实现:`tools/xtask/tests/gen_host.rs::core_074_kernel_and_slice_sets`

| 行 | 主机类 | 期望 |
|---|---|---|
| CORE-074.1 | `c8` | `isolcpus=3-7`、`nohz_full=3`、`rcu_nocbs=1,3-7`、`irqaffinity=0,1`;IRQ 计划 ENI-A → 0、ENI-B 全部队列 → 1;`system.slice`/`user.slice` `AllowedCPUs=0`;`hft-core.slice` `AllowedCPUs=0,2-7`;`hftbatch.slice` `AllowedCPUs=0` |
| CORE-074.2 | `c16` | `isolcpus=4-11`、`nohz_full=4`、`rcu_nocbs=1,4-11`、`irqaffinity=0,1`;ENI-C 全部队列 → 13;`system.slice`/`user.slice` `AllowedCPUs=0,12`;`hft-core.slice` `0,2-12`;`hft-sim.slice` `13-15`;`hftbatch.slice` `12` |
| CORE-074.3 | `c32` | `isolcpus=5-17`、`nohz_full=5`、`rcu_nocbs=1,5-17`、`irqaffinity=0,1`;ENI-C → 22;`system.slice`/`user.slice` `AllowedCPUs=0,18,29`;`hft-core.slice` `0,2-18,29`;`hft-sim.slice` `23-27`;`hftbatch.slice` `19-21,28,30-31` |
| CORE-074.4 | `c4` | `isolcpus=1-3`;不生成 IRQ 计划,输出中没有 `irqaffinity` |

### CORE-075 少于表中下单 venue 时空出的 gw 核并入批处理核集合

- 规则:[线程计划](README.md#线程计划)
- 级别:unit
- 里程碑:M3
- 前置:`c32` 模板只启用 4 个下单 venue(`binance-usdm`、`binance-spot`、`okx-swap`、`okx-spot`;`gw-bg-*` 两线程不生成)
- 步骤:
  1. 运行 `cargo xtask gen-host --host test-c32`
- 期望:
  - `isolcpus=5-15`;`hftbatch.slice` 的 `AllowedCPUs` 含 16 与 17;`host.hot_cpus` 生成为 `5-15`
- 实现:`tools/xtask/tests/gen_host.rs::core_075_spare_gw_cores_to_batch`

### CORE-076 `c32` 启用 ENI-B2 后核 28 成为 IRQ 专核并移出批处理集合

- 规则:[线程计划](README.md#线程计划)
- 级别:unit
- 里程碑:M3
- 前置:`c32` 模板加 `host.irq_cpus.eni_b2 = 28`,主机清单加 `[eni_b2]`
- 步骤:
  1. 运行 `cargo xtask gen-host --host test-c32`
- 期望:
  - `irqaffinity=0,1,28`;`rcu_nocbs` 含 28;IRQ 计划把 ENI-B2 全部队列落在 28;`hftbatch.slice` 的 `AllowedCPUs=19-21,30-31`(不含 28);`egress.toml` 多出 `prod_bulk` 表
- 实现:`tools/xtask/tests/gen_host.rs::core_076_eni_b2_core_28`

### CORE-077 hostprobe 报告的 `isolcpus` 与期望不符时以 `HostProfile` 退出 3

- 规则:[线程计划](README.md#线程计划)、[HostProfile](README.md#hostprofile)
- 级别:host
- 里程碑:M7
- 环境:统一升级主机(`c32`),`prod` 未运行
- 授权:`[需授权]`(改 GRUB 并重启主机)
- 步骤:
  1. 把 GRUB `isolcpus` 临时改为 `5-16` 并重启;运行 hostprobe
  2. `systemctl start hft-core@prod`;之后恢复 GRUB 并重启
- 通过标准:步骤 2 退出码 3,`journalctl` 有一行 `error[HostProfile]`,详情含 `isolcpus`;`/dev/shm/hft/prod/heartbeat` 未创建
- 证据:`ops-log/<UTC 日期>-CORE-077.md`
- 实现:`apps/hft-core/tests/host/CORE-077.sh`

### CORE-078 NVMe 等 managed IRQ 落在 IRQ 专核时以 `HostProfile` 退出 3

- 规则:[线程计划](README.md#线程计划)
- 级别:host
- 里程碑:M7
- 环境:统一升级主机(`c32`)
- 授权:`[需授权]`
- 步骤:
  1. 构造 hostprofile 夹具:某 NVMe 队列 `effective_affinity = 1`(或在探针主机上实测到该落点),运行 hostprobe 写 `/run/hft/hostprofile.json`
  2. `systemctl start hft-core@prod`
- 通过标准:退出码 3;`error[HostProfile]` 详情含 `effective_affinity` 与核 1
- 证据:`ops-log/<UTC 日期>-CORE-078.md`
- 实现:`apps/hft-core/tests/host/CORE-078.sh`

### CORE-079 IRQ 专核上的可运行任务:白名单外拒绝启动,白名单内的 keepawake 放行

- 规则:[线程计划](README.md#线程计划)
- 级别:host
- 里程碑:M7
- 环境:统一升级主机(`c32`)
- 授权:`[需授权]`
- 步骤:
  1. 在核 1 上以 `taskset -c 1` 跑一个不在 hostprobe 白名单里的忙循环进程,运行 hostprobe,`systemctl start hft-core@prod`
  2. 停掉它,改跑 hostprobe 白名单中的 keepawake 线程,重新运行 hostprobe 并启动
- 通过标准:步骤 1 退出码 3(`error[HostProfile]`,详情含核 1);步骤 2 走到 `Note{StartupStep}` 步 3 结束且无错误行
- 证据:`ops-log/<UTC 日期>-CORE-079.md`
- 实现:`apps/hft-core/tests/host/CORE-079.sh`

### CORE-080 加载校验第 13 条:升级主机类 `prod` 的 BBO 不与深度同线程

- 规则:[线程计划](README.md#线程计划)
- 级别:table
- 里程碑:M3
- 前置:各行从所列构造器起;`layout::validate`
- 实现:`apps/hft-core/tests/thread_plan.rs::core_080_bbo_not_with_md`

| 行 | 改动 | 期望错误列表 |
|---|---|---|
| CORE-080.1 | `c32_prod()` 把 `feed-md:binance-usdm` 移进 `feed-bbo-bn` | 恰好 {`BboSharesWithMd`} |
| CORE-080.2 | `c32_prod()` 把 `binance-spot` 的一对任务换成 `feed:binance-spot`,放在 `feed-bbo-bn` | 恰好 {`BboSharesWithMd`} |
| CORE-080.3 | `c8_prod()` 把 `feed-md:okx-swap` 移进 `feed-bbo` | 恰好 {`BboSharesWithMd`} |
| CORE-080.4 | `c16_prod()` 不改(`feed-bbo-a`/`-b` 跨 venue 合并 BBO) | 空 |
| CORE-080.5 | `c32_prod()` 把三个 `feed-md-*` 合并为一个线程(核 9),去掉核 10、11 的线程,`host.hot_cpus` 相应去掉 10、11 | 空 |
| CORE-080.6 | `c4_prod()` 不改(`feed:<v>`) | 空 |
| CORE-080.7 | `dev2_prod()` 不改(`feed:<v>`) | 空 |
| CORE-080.8 | `c32_sim()` 不改(`feed:<v>`) | 空 |

### CORE-081 行情任务名决定 `FeedRuntime` 的路由集合

- 规则:[线程计划](README.md#线程计划)、[实现说明](README.md#实现说明)
- 级别:table
- 里程碑:M3
- 前置:`venues::feed_routes(task_name)` 纯函数
- 实现:`apps/hft-core/tests/venues.rs::core_081_feed_routes_by_task`

| 行 | 任务名 | 期望 |
|---|---|---|
| CORE-081.1 | `feed-bbo:okx-swap` | `Routes::Bbo` |
| CORE-081.2 | `feed-md:okx-swap` | `Routes::Md` |
| CORE-081.3 | `feed:okx-swap` | `Routes::All` |

### CORE-082 本页各主机类的线程计划表都能通过加载校验

- 规则:[线程计划](README.md#线程计划)、[`dev2`:现有 `c7g.large`](README.md#dev2现有-c7glarge2-vcpu38-gb与旧系统同机)、[`c4`](README.md#c44-物理核联调可选)、[`c8`](README.md#c8c8g2xlarge8-物理核16-gib只承载-prod)、[`c16`](README.md#c16c8g4xlarge有条件的候选首批下单-venue--4)、[`c32`](README.md#c32c8g8xlarge统一升级主机默认候选prod--sim-同机)
- 级别:table
- 里程碑:M3
- 前置:各行用构造器(即本页表格的逐线程转写);主机夹具与主机类一致;`config::load_and_validate`
- 实现:`apps/hft-core/tests/thread_plan.rs::core_082_documented_plans_validate`

| 行 | 构造器 | 期望 |
|---|---|---|
| CORE-082.1 | `dev2_sim()` | 错误列表为空;线程 7 个 |
| CORE-082.2 | `dev2_prod()` | 空;线程 8 个(`gw-*` 2 个) |
| CORE-082.3 | `c4_prod()` | 空;线程 7 个 |
| CORE-082.4 | `c8_prod()` | 空;线程 9 个 |
| CORE-082.5 | `c16_prod()` | 空;线程 12 个 |
| CORE-082.6 | `c16_sim()` | 空;线程 7 个 |
| CORE-082.7 | `c32_prod()` | 空;线程 17 个 |
| CORE-082.8 | `c32_sim()` | 空;线程 7 个 |

### CORE-083 `c4`、`c8` 放不下超出表中数量的下单 venue

- 规则:[`c4`](README.md#c44-物理核联调可选)、[`c8`](README.md#c8c8g2xlarge8-物理核16-gib只承载-prod)
- 级别:table
- 里程碑:M3
- 前置:新增的 gw 线程只能用表中已有的核;`layout::validate`
- 实现:`apps/hft-core/tests/thread_plan.rs::core_083_host_class_gw_capacity`

| 行 | 配置 | 期望错误列表 |
|---|---|---|
| CORE-083.1 | `c4_prod()` 加第二个下单 venue,`gw-<v2>` 放在核 3 | 含 `ThreadPlan{6}` |
| CORE-083.2 | `c8_prod()` 加第三个下单 venue,`gw-<v3>` 放在核 7 | 含 `ThreadPlan{6}` |

### CORE-084 线程创建后先绑核、设 timerslack、nice、命名并 claim 心跳槽

- 规则:[线程计划](README.md#线程计划)、[HeartbeatBlock](README.md#heartbeatblock)
- 级别:proc
- 里程碑:M3
- 前置:`dev2_prod()` 连假交易所,`CPUAffinity` 不限;到 `Running`
- 步骤:
  1. 读 `/proc/<pid>/task/*/{comm,status,timerslack_ns,stat}` 与心跳块
- 期望:
  - 每个 `[[threads]]` 条目恰好一个线程,`comm` = `name`;`core` 为数字的线程 `Cpus_allowed_list` 恰为该核;`timerslack_ns = 1`;`nice` 列等于配置(`persist`、`warm` 为 10,`adv` 为 15,其余 0)
  - 心跳块 `n_slots` = 线程数 8,每个槽的 `name`、`core` 与配置相同;承载多个任务的线程(`warm`)`kind = 10`(`Mixed`)
- 实现:`apps/hft-core/tests/thread_plan.rs::core_084_thread_setup_and_slots`

### CORE-085 Spin 线程持有的生产端一律不发唤醒,非 Spin 生产端拿消费端的 `Unparker`

- 规则:[线程计划](README.md#线程计划)
- 级别:table
- 里程碑:M3
- 前置:`layout::wake_plan(&cfg)` 给出每条环与通道的生产端线程、消费端线程与 `wake`
- 实现:`apps/hft-core/tests/thread_plan.rs::core_085_wake_plan`

| 行 | 配置与对象 | 期望 |
|---|---|---|
| CORE-085.1 | `c32_prod()`,生产端在 `Spin` 线程上的全部环(`bbo-*`、`md-*`、`md-*-bulk`、`rep-*`、`rep-*-bulk`、`out-*`、`feedctl-*`、`snapreq-*`、`acctreq-*`)与 `RetiredSegment` 通道 | 每一条 `wake = None` |
| CORE-085.2 | `c32_prod()`,housekeeping(`Park`)→ ctl-gateway(`Park`)的 SIGTERM 内部通道 | `wake = Some`,`Unparker` 属于 `warm` 线程 |
| CORE-085.3 | `c32_prod()`,全部环 | 没有任何 `Unparker` 属于 `Spin` 线程 |
| CORE-085.4 | `dev2_sim()`,`bbo-binance-usdm`(`feed` 为 `SpinThenPark` → `seq` 为 `SpinThenPark`) | `wake = Some`,`Unparker` 属于 `seq` 线程 |

### CORE-086 所有线程都不用 `SCHED_FIFO`

- 规则:[线程计划](README.md#线程计划)
- 级别:proc
- 里程碑:M3
- 前置:`dev2_prod()` 连假交易所,到 `Running`
- 步骤:
  1. 对每个线程 `chrt -p <tid>`
- 期望:
  - 每个线程的策略都是 `SCHED_OTHER`
- 实现:`apps/hft-core/tests/thread_plan.rs::core_086_no_sched_fifo`

### CORE-087 `check_host` 按 `host.class` 判定主机类

- 规则:[线程计划](README.md#线程计划)、[HostProfile](README.md#hostprofile)
- 级别:table
- 里程碑:M3
- 前置:`rt.host.mode = "prod"`;构造器与主机夹具按行;`startup::run_steps_1_2`
- 实现:`apps/hft-core/tests/hostcheck.rs::core_087_host_class_check`

| 行 | 配置 / 主机夹具 | 期望 |
|---|---|---|
| CORE-087.1 | `c32_prod()` / `sysroot::c32()` | 无错误 |
| CORE-087.2 | `c32_prod()` / 16 vCPU、32 GiB | 退出码 3,`HostCheck`(`CLASS_MISMATCH`) |
| CORE-087.3 | `dev2_sim()` / 2 vCPU、3.8 GiB | 无错误 |
| CORE-087.4 | `c16_prod()` / 16 vCPU、28 GiB | 退出码 3,`HostCheck`(`CLASS_MISMATCH`) |

### CORE-088 同一二进制只换 `host.class` 与 `[[threads]]` 即可在各主机类通过 `--check`

- 规则:[线程计划](README.md#线程计划)
- 级别:proc
- 里程碑:M3
- 前置:同一构建产物;`dev2_prod()`、`c16_prod()`、`c32_prod()` 三份配置分别配对应的主机夹具(`rt.host.sysroot`)
- 步骤:
  1. 三次运行 `hft-core --config <cfg> --check`
- 期望:
  - 三次退出码都是 0;二进制的 SHA-256 三次相同
- 实现:`apps/hft-core/tests/thread_plan.rs::core_088_same_binary_all_classes`

### CORE-089 同一发布在 dev2、`c16` 探针与 `c32` 上启动并过闸门

- 规则:[线程计划](README.md#线程计划)
- 级别:host
- 里程碑:M7
- 环境:dev2(`sim` 与联调窗口内的 `prod`);M3 末探针的 `c8g.4xlarge`(`c16` 计划)与 `c8g.8xlarge`(`c32` 计划)
- 授权:`[需授权]`(租用实例与启动 hft 单元)
- 步骤:
  1. 同一发布归档在三台主机上激活,只用各自主机类的配置,`systemctl start hft-core@prod hft-core@sim`
- 通过标准:三台主机上 `RELEASE.json` 的二进制哈希相同;每个实例在 `startup.gate_timeout_s` 内写出全部账户 `released = 1` 的 `Note{StartupGate}`
- 证据:`ops-log/<UTC 日期>-CORE-089.md`
- 实现:`apps/hft-core/tests/host/CORE-089.sh`

### CORE-090 按实例与线程计划建环:名字与存在条件

- 规则:[线程计划](README.md#线程计划)
- 级别:table
- 里程碑:M3
- 前置:`startup::ring_plan(&cfg)` 给出要建的环名集合
- 实现:`apps/hft-core/tests/rings.rs::core_090_ring_set_by_instance`

| 行 | 配置 | 期望 |
|---|---|---|
| CORE-090.1 | `dev2_prod()` | 含每 venue 的 `bbo-<v>`、`md-<v>`、`md-<v>-bulk`、`feedctl-<v>`;只对 `binance-usdm`、`okx-swap` 有 `rep-<v>`、`rep-<v>-bulk`、`out-<v>`、`acctreq-<v>`、`acct-<v>`;有 `ctl`、`hk`、`advice`;没有 `out-sim`、`rep-sim`、`rep-sim-bulk`、任何 `-bbo` 后缀的环 |
| CORE-090.2 | `dev2_sim()` | 有 `out-sim`、`rep-sim`、`rep-sim-bulk`;没有任何 `rep-<v>`、`out-<v>`、`acctreq-<v>`、`acct-<v>` |
| CORE-090.3 | `c32_prod()` | 每 venue 另有 `feedctl-<v>-bbo`;有 BBO 补帧的 venue 另有 `snapreq-<v>-bbo`、`snap-<v>-bbo`;raw journal 列表含 6 个 `raw-feed-<v>-bbo` |
| CORE-090.4 | `dev2_sim()` 的两个实例都绑 `NoModel` 的 `latency-probe` 并去掉 `advisor` | 没有 `advice` |

### CORE-100 每个 journal 的常驻热段 = (2 + `hot_keep`) × `segment_bytes`

- 规则:[内存与 journal 段(按主机类唯一归属)](README.md#内存与-journal-段按主机类唯一归属)
- 级别:table
- 里程碑:M3
- 前置:`budget::hot_resident(segment_bytes, hot_keep)` 纯函数,单位字节
- 实现:`apps/hft-core/tests/budget.rs::core_100_hot_resident`

| 行 | 输入 | 期望 |
|---|---|---|
| CORE-100.1 | 48 MiB、`hot_keep = 1`(`dev2` `sim` 的 `seq`) | 144 MiB |
| CORE-100.2 | 4 MiB、0(`dev2` 的 `raw-feed-<v>`) | 8 MiB |
| CORE-100.3 | 1 GiB、1(升级主机类 `prod` 的 `seq`) | 3 GiB |
| CORE-100.4 | 2 GiB、2(`metal` 的 `seq`) | 8 GiB |
| CORE-100.5 | 2 MiB、0(`dev2` `prod` 的 `raw-gw-<v>`) | 4 MiB |

### CORE-101 由线程计划与 `[venues]` 唯一得出 journal 列表

- 规则:[内存与 journal 段(按主机类唯一归属)](README.md#内存与-journal-段按主机类唯一归属)、[启动顺序](README.md#启动顺序)
- 级别:table
- 里程碑:M3
- 前置:`budget::journal_list(&cfg)`
- 实现:`apps/hft-core/tests/budget.rs::core_101_journal_list`

| 行 | 配置 | 期望 |
|---|---|---|
| CORE-101.1 | `dev2_sim()` | `seq` + 6 个 `raw-feed-<v>`,共 7 个 |
| CORE-101.2 | `dev2_prod()` | `seq` + 6 个 `raw-feed-<v>` + `raw-gw-binance-usdm`、`raw-gw-okx-swap`,共 9 个 |
| CORE-101.3 | `c4_prod()` | `seq` + 6 个 `raw-feed-<v>` + 1 个 `raw-gw-<v>`,共 8 个 |
| CORE-101.4 | `c32_prod()` | `seq` + 6 个 `raw-feed-<v>` + 6 个 `raw-feed-<v>-bbo` + 6 个 `raw-gw-<v>`,共 19 个 |

### CORE-102 内存表各列由实际配置求和得到 `host.mem_budget_mib`

- 规则:[内存与 journal 段(按主机类唯一归属)](README.md#内存与-journal-段按主机类唯一归属)
- 级别:table
- 里程碑:M3
- 前置:各行的构造器(段大小、`hot_keep`、页、advisor 预算按内存表);`budget::compute(&cfg)` 返回各部分 MiB 与合计
- 实现:`apps/hft-core/tests/budget.rs::core_102_budget_columns`

| 行 | 构造器 | 期望(`seq` / `raw-feed` / `-bbo` / `raw-gw` / 引擎区 / sim-gw / advisor / 镜像 / 其余 = 合计;其中大页) |
|---|---|---|
| CORE-102.1 | `dev2_sim()` | 144 / 48 / 0 / 0 / 64 / 48 / 160 / 48 / 64 = 576;大页 0 |
| CORE-102.2 | `dev2_prod()` 扩为 6 个下单 venue(每 venue 一个 Live 账户、一个 `gw-*` 线程) | 96 / 48 / 0 / 24 / 72 / 0 / 48 / 32 / 64 = 384;大页 0 |
| CORE-102.3 | `c4_prod()` | 768 / 768 / 0 / 32 / 1,024 / 0 / 256 / 384 / 128 = 3,360;大页 1,568 |
| CORE-102.4 | `c8_prod()` | 3,072 / 768 / 768 / 128 / 2,048 / 0 / 384 / 768 / 256 = 8,192;大页 6,784 |
| CORE-102.5 | `c16_prod()` | 3,072 / 1,536 / 1,536 / 256 / 2,048 / 0 / 512 / 768 / 256 = 9,984;大页 8,448 |
| CORE-102.6 | `c32_prod()` | 3,072 / 1,536 / 1,536 / 384 / 2,048 / 0 / 1,024 / 768 / 256 = 10,624;大页 8,576 |
| CORE-102.7 | `c16_sim()` | 1,536 / 96 / 0 / 0 / 1,024 / 256 / 512 / 768 / 128 = 4,320;大页 0 |
| CORE-102.8 | `c32_sim()` | 1,536 / 96 / 0 / 0 / 1,024 / 512 / 2,048 / 768 / 160 = 6,144;大页 0 |

### CORE-103 内存预算超出时步 3 以 `MemBudget` 退出 3 并列出各项求和

- 规则:[内存与 journal 段(按主机类唯一归属)](README.md#内存与-journal-段按主机类唯一归属)
- 级别:table
- 里程碑:M3
- 前置:`dev2_sim()`(求和 576 MiB);只改 `host.mem_budget_mib`;`startup::step3_budget(&cfg)`
- 实现:`apps/hft-core/tests/budget.rs::core_103_budget_boundary`

| 行 | `host.mem_budget_mib` | 期望 |
|---|---|---|
| CORE-103.1 | 577 | 通过 |
| CORE-103.2 | 576 | 通过 |
| CORE-103.3 | 575 | `Err(MemBudget)`,详情依次列出九项求和与合计 576、预算 575 |

### CORE-104 大页数按 journal 列表求和,与内存表与 GRUB 生成值逐项一致

- 规则:[内存与 journal 段(按主机类唯一归属)](README.md#内存与-journal-段按主机类唯一归属)
- 级别:table
- 里程碑:M3
- 前置:deploy 各主机类模板;`cargo xtask gen-host --host test-<主机类> --check`
- 实现:`tools/xtask/tests/gen_host.rs::core_104_hugepage_counts`

| 行 | 主机类 | 期望 |
|---|---|---|
| CORE-104.1 | `c4` | 1 GiB 页 0、2 MiB 页 848;`--check` 通过 |
| CORE-104.2 | `c8` | 1 GiB 页 6、2 MiB 页 896 |
| CORE-104.3 | `c16` | 1 GiB 页 6、2 MiB 页 1,728 |
| CORE-104.4 | `c32` | 1 GiB 页 6、2 MiB 页 1,792 |

### CORE-105 改段大小而不改内存表时 `gen-host --check` 失败

- 规则:[内存与 journal 段(按主机类唯一归属)](README.md#内存与-journal-段按主机类唯一归属)
- 级别:unit
- 里程碑:M3
- 前置:`c32` 模板中只把 `journal.raw-feed-binance-usdm-bbo.segment_bytes` 由 128 MiB 改成 256 MiB
- 步骤:
  1. 运行 `cargo xtask gen-host --host test-c32 --check`
- 期望:
  - 非零退出;输出列出不符项:2 MiB 页数(重算 1,920,表与 GRUB `hugepages=` 为 1,792)
- 实现:`tools/xtask/tests/gen_host.rs::core_105_segment_drift_detected`

### CORE-106 步 3 用实际配置再算一次预算(不依赖生成器)

- 规则:[内存与 journal 段(按主机类唯一归属)](README.md#内存与-journal-段按主机类唯一归属)
- 级别:unit
- 里程碑:M3
- 前置:`dev2_sim()`;`host.mem_budget_mib = 576`;只把 `journal.seq.segment_bytes` 改为 49 MiB(`hot_keep = 1`)
- 步骤:
  1. 调用 `startup::step3_budget`
- 期望:
  - `Err(MemBudget)`,详情中 `seq` 一项为 147 MiB,合计 579 MiB
- 实现:`apps/hft-core/tests/budget.rs::core_106_step3_recomputes_budget`

### CORE-107 段大小须 ≥ 16 × 快照大小(各主机类实算)

- 规则:[内存与 journal 段(按主机类唯一归属)](README.md#内存与-journal-段按主机类唯一归属)
- 级别:host
- 里程碑:M3
- 环境:dev2;按目标订阅回放现有采集数据(方法见内存表后的"待实测"),每个主机类列的配置各跑一次
- 授权:无(离线回放)
- 步骤:
  1. 对每列配置取引导块(快照)字节数的最大值 `snap_max`
- 通过标准:每列 `seq` 的 `segment_bytes` ≥ 16 × `snap_max`;不满足的列记入证据并按调节旋钮改表
- 证据:`ops-log/<UTC 日期>-CORE-107.md`(每列 `snap_max` 与比值)
- 实现:`apps/hft-core/tests/host/CORE-107.sh`

### CORE-108 Model 内存与检查点预算在装配时按字节与 MiB 换算比较

- 规则:[内存与 journal 段(按主机类唯一归属)](README.md#内存与-journal-段按主机类唯一归属)
- 级别:table
- 里程碑:M4
- 前置:`dev2_sim()`;`advisor.mem_budget_mib = 160`、`advisor.checkpoint_max_mib = 1024`(ECF1 段 5 同值);用 hft-testkit 的 `probe-target`(其 `Model::mem_bytes`、`snapshot_bytes` 由参数直接给定)绑两个实例;`budget::check_model(&cfg, &params)`
- 实现:`apps/hft-core/tests/budget.rs::core_108_model_budget`

| 行 | Σ `mem_bytes` / Σ `snapshot_bytes` | 期望 |
|---|---|---|
| CORE-108.1 | 167,772,160 B(= 160 MiB) / 512 MiB | 通过 |
| CORE-108.2 | 167,772,161 B / 512 MiB | `ModelMemBudget` |
| CORE-108.3 | 100 MiB / 536,870,912 B(2 × = 1,024 MiB) | 通过 |
| CORE-108.4 | 100 MiB / 536,870,913 B | `ModelMemBudget` |

### CORE-109 配置的 advisor 预算须与 ECF1 段 5 两两相等

- 规则:[内存与 journal 段(按主机类唯一归属)](README.md#内存与-journal-段按主机类唯一归属)、[配置文件字段](README.md#配置文件字段)
- 级别:table
- 里程碑:M3
- 前置:`dev2_sim()`(配置与段 5 都是 160 / 1024,`allow_missing_standby` 都为 false);`config::load_and_validate`
- 实现:`apps/hft-core/tests/budget.rs::core_109_segment5_equality`

| 行 | 改动 | 期望错误列表 |
|---|---|---|
| CORE-109.1 | 不改 | 空 |
| CORE-109.2 | 配置 `advisor.mem_budget_mib = 161` | 恰好 {`ModelMemBudget`} |
| CORE-109.3 | 配置 `advisor.checkpoint_max_mib = 1023` | 恰好 {`ModelMemBudget`} |
| CORE-109.4 | 配置 `startup.allow_missing_standby = true` | 恰好 {`StandbyFlagMismatch`} |
| CORE-109.5 | 去掉配置的 `advisor.mem_budget_mib` | 恰好 {`MissingField`(`advisor.mem_budget_mib`)} |

### CORE-110 运行中多绑一个 group 使 Model 内存超预算的 `SetParams` 被拒,旧参数继续生效

- 规则:[内存与 journal 段(按主机类唯一归属)](README.md#内存与-journal-段按主机类唯一归属)
- 级别:integ
- 里程碑:M4
- 前置:`dev2_sim()`,`advisor.mem_budget_mib = 160`;`probe-target` 每 group 声明 `mem_bytes` = 2 MiB;两个实例共 80 个 group(Σ = 160 MiB,恰好等于预算);进程到 `Running`
- 步骤:
  1. 对实例 `s1` 发 `SetParams`,group 数加 1
- 期望:
  - 启动成功(步 2 无错误);步骤 1 的 `CmdResult` 为 `InvalidArgs{ParamInvalid}`,kind = `ModelMemBudget`(2)
  - 之后 `s1` 生效参数的 SHA-256 与步骤 1 前相同;advisor 仍有 80 个 group 的 Model 状态
- 实现:`apps/hft-core/tests/budget.rs::core_110_setparams_model_budget`

### CORE-111 不得关 `mlock`:`mlockall` 失败时以 `HostCheck` 退出 3

- 规则:[内存与 journal 段(按主机类唯一归属)](README.md#内存与-journal-段按主机类唯一归属)
- 级别:proc
- 里程碑:M3
- 前置:`dev2_sim()`;`rt.host.mode = "prod"`;以 `prlimit --memlock=65536:65536` 启动
- 步骤:
  1. 运行 `hft-core --config <cfg>`
- 期望:
  - 退出码 3;错误行 `HostCheck`,详情含 `MLOCK_FAILED`
- 实现:`apps/hft-core/tests/budget.rs::core_111_mlock_failure_exit3`

### CORE-112 raw 模式与 `feed.raw_dups` 按主机类与实例取值

- 规则:[内存与 journal 段(按主机类唯一归属)](README.md#内存与-journal-段按主机类唯一归属)
- 级别:table
- 里程碑:M3
- 前置:deploy 各主机类模板生成的实例配置;读每个 `venues.<v>.feed.raw.{bbo, md}` 与 `feed.raw_dups`
- 实现:`tools/xtask/tests/gen_host.rs::core_112_raw_modes`

| 行 | 主机类 / 实例 | 期望 |
|---|---|---|
| CORE-112.1 | `dev2` `sim`、`dev2` `prod` | `{off, off}` |
| CORE-112.2 | `c4` `prod` | `{on, sampled}` |
| CORE-112.3 | `c8`、`c16`、`c32` `prod` | `{on, on}`,`feed.raw_dups = false` |
| CORE-112.4 | `c16`、`c32` `sim` | `{off, off}` |

### CORE-113 EngineConfig 的容量各列按容量表生成

- 规则:[内存与 journal 段(按主机类唯一归属)](README.md#内存与-journal-段按主机类唯一归属)
- 级别:table
- 里程碑:M3
- 前置:`tools/hft-cfg` 按各主机类与实例生成 EngineConfig;读 ECF1 段 3 与 `MAX_SIM_ACCOUNTS` 内的 Sim 账户数
- 实现:`tools/hft-cfg/tests/capacity.rs::core_113_capacity_columns`

| 行 | 列 | 期望(`max_insts` / `max_cells_total` / `max_instances` / `max_groups_per_instance` / `max_subs_per_inst` / `max_timers` / `recent_cmd_ids` / Sim 账户上限) |
|---|---|---|
| CORE-113.1 | `dev2` `sim` | 96 / 160 / 6 / 64 / 8 / 1024 / 4096 / 16 |
| CORE-113.2 | `dev2` `prod` | 64 / 64 / 4 / 64 / 8 / 1024 / 4096 / 0 |
| CORE-113.3 | `c4` | 1024 / 256 / 8 / 64 / 8 / 4096 / 4096 / 0 |
| CORE-113.4 | `c8`/`c16`/`c32` `prod` | 4096 / 256 / 8 / 64 / 8 / 4096 / 4096 / 0 |
| CORE-113.5 | `c16` `sim` | 4096 / 512 / 16 / 64 / 8 / 4096 / 4096 / 16 |
| CORE-113.6 | `c32` `sim` | 4096 / 1024 / 16 / 64 / 8 / 4096 / 4096 / 16 |
| CORE-113.7 | 每列 | `ModeOutcome.cells` 与 `ModeOutcome.cancels` 容量 = 该列 `max_cells_total` |

### CORE-114 装配期交叉校验:容量与连接活性

- 规则:[内存与 journal 段(按主机类唯一归属)](README.md#内存与-journal-段按主机类唯一归属)
- 级别:table
- 里程碑:M3
- 前置:`dev2_sim()`(`max_cells_total = 160`、`max_insts = 96`、`feedctl` 环容量 256);`config::load_and_validate`
- 实现:`apps/hft-core/tests/cross_checks.rs::core_114_capacity_cross_checks`

| 行 | 改动 | 期望错误列表 |
|---|---|---|
| CORE-114.1 | 参数共 160 个 cell | 空 |
| CORE-114.2 | 参数共 161 个 cell | 恰好 {`CellCapacity`} |
| CORE-114.3 | Σ `venues.<v>.max_sub_insts` = 96(6 × 16) | 空 |
| CORE-114.4 | 把 `binance-usdm` 的 `max_sub_insts` 改为 17(Σ = 97) | 恰好 {`SubCapacity`} |
| CORE-114.5 | 某账户 `RiskParams.conn_liveness_ms = 2000`,该 venue `bbo_ping_ms = 1000` | 空 |
| CORE-114.6 | 同上,`conn_liveness_ms = 1999` | 恰好 {`ConnLiveness`} |
| CORE-114.7 | 改用 `c32_prod()`(`max_insts = 4096`):每个 venue `max_sub_insts = 100`,EngineConfig 段 4 的 `FeedCtl` 行 `cap = 256`,再把 `binance-usdm` 的 `max_sub_insts` 改为 128 | 空 |
| CORE-114.8 | 同上,`binance-usdm` 的 `max_sub_insts = 129` | 恰好 {`FeedCtlCapacity`} |

### CORE-115 `ExecRiskMismatch`:执行配置与风控参数的一致性在装配时检查

- 规则:[内存与 journal 段(按主机类唯一归属)](README.md#内存与-journal-段按主机类唯一归属)
- 级别:table
- 里程碑:M3
- 前置:`c32_prod()`;账户 `bn-1` 可交易 inst 的价格带类别 `rr_max_cross` 最小值为 50 bp,`max_order_notional = 1000`;条件的判定函数是 hft-risk 的一致性函数;`config::load_and_validate`
- 实现:`apps/hft-core/tests/cross_checks.rs::core_115_exec_risk_mismatch`

| 行 | `bn-1` 的 `ExecConfig.force_slice` | 期望错误列表 |
|---|---|---|
| CORE-115.1 | `max_cross = 50 bp`、`max_chunk_notional = 1000` | 空 |
| CORE-115.2 | `max_cross = 51 bp`、`max_chunk_notional = 1000` | 恰好 {`ExecRiskMismatch`},详情含 `bn-1` |
| CORE-115.3 | `max_cross = 50 bp`、`max_chunk_notional = 1000.01` | 恰好 {`ExecRiskMismatch`} |

### CORE-116 `DustBelowMinNotional`:尘埃线不得低于交易腿最小名义的最大值(Live 与 Sim)

- 规则:[内存与 journal 段(按主机类唯一归属)](README.md#内存与-journal-段按主机类唯一归属)
- 级别:table
- 里程碑:M3
- 前置:账户绑定实例的参数中全部 group 交易腿 inst 的 `InstMeta.min_notional` 为 {5, 10}(最大值 10 USDT);`config::load_and_validate`
- 实现:`apps/hft-core/tests/cross_checks.rs::core_116_dust_vs_min_notional`

| 行 | 配置与 `ExecConfig.dust_notional` | 期望错误列表 |
|---|---|---|
| CORE-116.1 | `c32_prod()` 的 `bn-1`,10 | 空 |
| CORE-116.2 | `c32_prod()` 的 `bn-1`,9.99 | 恰好 {`DustBelowMinNotional`},详情含 `bn-1` |
| CORE-116.3 | `c32_prod()` 的 `bn-1`,10.01 | 空 |
| CORE-116.4 | `dev2_sim()` 的 `sim-a`(Sim 账户),9.99 | 恰好 {`DustBelowMinNotional`} |
| CORE-116.5 | `c32_prod()` 的 `bn-1`,10;另一个 group 加一条 `min_notional = 20` 的腿 | 恰好 {`DustBelowMinNotional`} |

### CORE-117 缩容:快照占用超过新容量时以 `CapacityShrink` 退出 2,先降占用再换即可启动

- 规则:[内存与 journal 段(按主机类唯一归属)](README.md#内存与-journal-段按主机类唯一归属)
- 级别:proc
- 里程碑:M4
- 前置:`dev2_sim()` 的 EngineConfig 改为 `max_cells_total = 256`(其余同列),参数共 200 个 cell,`config_source = "file"`;运行到 `Running`
- 步骤:
  1. 发 `Drain`,等退出 0;把 `blobs.engine` 换成 `max_cells_total = 128` 的 EngineConfig,启动
  2. 换回 256 的 EngineConfig 启动,经 `SetParams` 把 cell 降到 100,`Drain`;再换 128 的 EngineConfig 启动
- 期望:
  - 步骤 1 退出码 2,错误行 `CapacityShrink`,详情含 cell 200 > 128;在步 5 之后退出,没有写出 `EngineClock`
  - 步骤 2 最后一次启动到 `Running`,无错误行
- 实现:`apps/hft-core/tests/budget.rs::core_117_capacity_shrink_exit2`

### CORE-118 缩容检查覆盖每类占用

- 规则:[内存与 journal 段(按主机类唯一归属)](README.md#内存与-journal-段按主机类唯一归属)
- 级别:table
- 里程碑:M4
- 前置:`budget::check_occupancy(&snapshot_usage, &capacities)` 纯函数;容量取 `dev2` `sim` 列
- 实现:`apps/hft-core/tests/budget.rs::core_118_shrink_each_kind`

| 行 | 快照占用 | 期望 |
|---|---|---|
| CORE-118.1 | inst 96、cell 160、实例 6、Sim 账户 16、单实例 group 64 | `Ok` |
| CORE-118.2 | inst 97 | `Err(CapacityShrink{what: Insts})` |
| CORE-118.3 | 实例 7 | `Err(CapacityShrink{what: Instances})` |
| CORE-118.4 | Sim 账户 17 | `Err(CapacityShrink{what: SimAccounts})` |
| CORE-118.5 | 某实例 group 65 | `Err(CapacityShrink{what: GroupsPerInstance})` |

### CORE-119 扩容不受限:占用低于新容量即可启动

- 规则:[内存与 journal 段(按主机类唯一归属)](README.md#内存与-journal-段按主机类唯一归属)
- 级别:unit
- 里程碑:M4
- 前置:快照 cell 100;新 EngineConfig `max_cells_total = 256`(原 128)
- 步骤:
  1. 调用 `budget::check_occupancy`
- 期望:
  - `Ok`
- 实现:`apps/hft-core/tests/budget.rs::core_119_capacity_growth_ok`

### CORE-120 正式实例从空 journal 以 `config_source = file` 首次启动,不要求 Drain 快照

- 规则:[内存与 journal 段(按主机类唯一归属)](README.md#内存与-journal-段按主机类唯一归属)
- 级别:proc
- 里程碑:M3
- 前置:`dev2_prod()`,`config_source = "file"`;journal 热段根与冷副本根都为空目录;`DurableMeta` 不存在
- 步骤:
  1. 启动,等步 5 结束
- 期望:
  - 无错误行;`EngineClock.source = Fresh`(1);`Note{EpochStart}` 的 `v[0]` = 1;`ConfigBlob{Refdata}` 的内容 = `blobs.refdata` 文件字节
- 实现:`apps/hft-core/tests/recovery.rs::core_120_first_start_empty_journal`

### CORE-121 测试网实例的热段残留时正式实例冷热冲突,以 `Journal` 退出 4

- 规则:[内存与 journal 段(按主机类唯一归属)](README.md#内存与-journal-段按主机类唯一归属)
- 级别:proc
- 里程碑:M3
- 前置:热段根 `<journal_ram>/prod/` 留有测试网实例 SIGKILL 后的 `seq` 热段(`synced_len > 0`);冷副本根 `<cold_root>/prod/` 已搬空
- 步骤:
  1. 以正式配置(同一 `instance.name = "prod"`,空冷副本)启动
- 期望:
  - 退出码 4;错误行 `Journal`;热段文件未被修改(SHA-256 不变)
- 实现:`apps/hft-core/tests/recovery.rs::core_121_testnet_leftover_conflict`

### CORE-122 两层毒丸计数分键存放,`EngineClock.base_engine_ns` 取上次运行的末值

- 规则:[内存与 journal 段(按主机类唯一归属)](README.md#内存与-journal-段按主机类唯一归属)、[崩溃隔离与从快照恢复](README.md#崩溃隔离与从快照恢复)
- 级别:proc
- 里程碑:M3
- 前置:`dev2_sim()` 运行到 `Running`,最后一条已提交记录的 `engine_ns = E`
- 步骤:
  1. SIGKILL 后重启,等步 5 结束
- 期望:
  - `DurableMeta` 目录下 `boot_guard` 与 `poison_input` 是两个不同的键;本次 `EngineClock.base_engine_ns = E`、`source = Tail`(2)
- 实现:`apps/hft-core/tests/recovery.rs::core_122_durable_meta_keys`

### CORE-130 `CkptHdr` 与 `PathSecHdr` 的大小、对齐与字段偏移

- 规则:[内存与 journal 段(按主机类唯一归属)](README.md#内存与-journal-段按主机类唯一归属)
- 级别:table
- 里程碑:M4
- 前置:编译期断言与 `memoffset::offset_of!`
- 实现:`apps/hft-core/tests/ckpt_layout.rs::core_130_ckpt_header_layout`

| 行 | 断言 | 期望 |
|---|---|---|
| CORE-130.1 | `size_of::<CkptHdr>()` / `align_of` | 64 / 8 |
| CORE-130.2 | `magic` / `hdr_ver` / `kind_id` / `layout_version` 偏移 | 0 / 8 / 10 / 12 |
| CORE-130.3 | `schema_id` / `strategy` / `account` / `_pad0` 偏移 | 16 / 20 / 21 / 22 |
| CORE-130.4 | `last_seq` / `body_len` / `crc32c` / `_rsv` 偏移 | 24 / 32 / 40 / 44 |
| CORE-130.5 | `size_of::<PathSecHdr>()` / `align_of`;`path_version` / `path_len` 偏移 | 8 / 4;0 / 4 |
| CORE-130.6 | 新写出文件的 `magic` / `hdr_ver` | `*b"HFTCKPT1"` / 1 |

### CORE-131 检查点编解码往返,CRC 覆盖整个文件且本字段按 0 计

- 规则:[内存与 journal 段(按主机类唯一归属)](README.md#内存与-journal-段按主机类唯一归属)
- 级别:prop
- 里程碑:M4
- 性质:任意键、路径节与样本节写出后读回,键、`path_version`、两节字节逐字节相等;`body_len` = 文件长 − 64;`crc32c` = 把 40..44 置 0 后整个文件的 CRC32C
- 生成:`kind_id` ∈ u16;`layout_version`、`schema_id` ∈ u32;`strategy`、`account` ∈ u8;`last_seq` ∈ u64;`path_version` ∈ {0, 1..=u32::MAX};`path_len` 0–64 KiB(`path_version = 0` 时为 0);样本节 0–4 MiB
- 参考:`ckpt::encode`/`ckpt::decode` 与独立实现的 CRC32C
- 例数:PR 2 000
- 实现:`apps/hft-core/tests/ckpt_format.rs::core_131_ckpt_roundtrip`

### CORE-132 损坏的检查点判 `Corrupt`,路径版本不符只丢路径节

- 规则:[内存与 journal 段(按主机类唯一归属)](README.md#内存与-journal-段按主机类唯一归属)
- 级别:table
- 里程碑:M4
- 前置:从一份合法文件(`body_len = 8 + 4096 + 65536`,`path_len = 4096`,当前 `Model::PATH_VERSION = 3`)按行改动;`ckpt::decode(bytes, path_version_now = 3)`
- 实现:`apps/hft-core/tests/ckpt_format.rs::core_132_ckpt_corruption`

| 行 | 改动 | 期望 |
|---|---|---|
| CORE-132.1 | 不改 | `Ok`,路径节与样本节都交出 |
| CORE-132.2 | `magic = *b"HFTCKPT2"` | `Err(Corrupt)` |
| CORE-132.3 | `hdr_ver = 2` | `Err(Corrupt)` |
| CORE-132.4 | 文件截掉最后 1 字节(`body_len` 不变) | `Err(Corrupt)` |
| CORE-132.5 | 样本节中翻转 1 位 | `Err(Corrupt)`(CRC) |
| CORE-132.6 | `path_len = 4096 + 65536 + 1`(超出体长,CRC 重算) | `Err(Corrupt)` |
| CORE-132.7 | `path_len = 4096 + 65536`(样本节为空,CRC 重算) | `Ok`,样本节长 0 |
| CORE-132.8 | `path_version = 2`(CRC 重算) | `Ok`,只交出样本节;路径节丢弃 |
| CORE-132.9 | `PATH_VERSION = 0` 的 Model:`path_version = 0`、`path_len = 0` | `Ok`,路径节为空 |
| CORE-132.10 | `_pad0` 或 `_rsv` 非零,CRC 重算 | `Err(Corrupt)` |
| CORE-132.11 | 体长不足 8 B,或 `path_version = 0` 且 `path_len > 0`,CRC 重算 | `Err(Corrupt)` |

### CORE-133 检查点的可用性只由五项键决定

- 规则:[内存与 journal 段(按主机类唯一归属)](README.md#内存与-journal-段按主机类唯一归属)、[实现说明](README.md#实现说明)
- 级别:table
- 里程碑:M4
- 前置:实例 `(strategy = 1, account = 2)`,当前 `kind_id = 0xFF02`、`LAYOUT_VERSION = 4`、`schema.id = 77`;journal 可读区间为 seq 1000–5000;advisor 启动读检查点
- 实现:`apps/hft-core/tests/ckpt_load.rs::core_133_ckpt_key_rules`

| 行 | 文件中的键或环境 | 期望 |
|---|---|---|
| CORE-133.1 | 全部一致,`last_seq = 4000` | `Model::restore` 被调用,从 seq 4001 重放;无 `ModelCheckpointDiscarded` |
| CORE-133.2 | `kind_id = 0xFF03` | `Note{ModelCheckpointDiscarded}`,`v[1] = 3`(KeyMismatch),空 Model |
| CORE-133.3 | `layout_version = 5` | 同上,`v[1] = 3` |
| CORE-133.4 | `schema_id = 78` | 同上,`v[1] = 3` |
| CORE-133.5 | `strategy = 9` | 同上,`v[1] = 3` |
| CORE-133.6 | `account = 9` | 同上,`v[1] = 3` |
| CORE-133.7 | `last_seq = 999`(已不可读) | `v[1] = 4`(SeqNotInJournal) |
| CORE-133.8 | 当前参数版本与写出时不同 | 采用;追上后调用一次 `model_on_params` |
| CORE-133.9 | 当前 refdata 版本与写出时不同 | 采用 |
| CORE-133.10 | 二进制 SHA 与写出时不同 | 采用 |
| CORE-133.11 | 目录中没有该实例的 `.ckpt` 与 `.prev` | `v[1] = 1`(Missing) |
| CORE-133.12 | 键、CRC 都相符,`Model::restore` 返回错误 | `v[1] = 5`(RestoreErr),空 Model |
| CORE-133.13 | 键相符,`path_version = 2` 而当前 `PATH_VERSION = 3` | 采用样本节;一条 `Note{ModelPathReset}`,`v[0] = 1`(VersionMismatch);无 `ModelCheckpointDiscarded` |

### CORE-134 当前代不可用时用 `.prev`,两代都坏才丢弃

- 规则:[实现说明](README.md#实现说明)
- 级别:table
- 里程碑:M4
- 前置:实例的 `.ckpt` 与 `.prev` 各一份合法文件(`last_seq` 分别为 4000、3000,均可读);按行损坏
- 实现:`apps/hft-core/tests/ckpt_load.rs::core_134_prev_generation_fallback`

| 行 | 状态 | 期望 |
|---|---|---|
| CORE-134.1 | `.ckpt` 样本节翻转 1 位 | 从 `.prev` 恢复,从 seq 3001 重放;无 `ModelCheckpointDiscarded` |
| CORE-134.2 | `.ckpt` 与 `.prev` 都翻转 1 位 | 空 Model;一条 `Note{ModelCheckpointDiscarded}`,`v[1] = 2`(Corrupt) |
| CORE-134.3 | `.ckpt` 缺失,`.prev` 合法 | 从 `.prev` 恢复 |

### CORE-135 检查点恢复后续算的 Advice 与不中断运行逐字节一致

- 规则:[实现说明](README.md#实现说明)
- 级别:integ
- 里程碑:M4
- 前置:`dev2_sim()` 绑 `portfolio-carry`;回放夹具 `fixtures/replay/core-advisor-2h.pointer.toml` 驱动;`advisor.model_checkpoint_ms = 600000`
- 步骤:
  1. 连续运行 2 h,记录全部 `Advice` Input
  2. 另一次运行在 70 min 处 SIGKILL,重启后跑到 2 h
- 期望:
  - 步骤 2 重启后从 60 min 处的检查点恢复(无 `ModelCheckpointDiscarded`);`basis_seq` ≥ 重启 `EpochStart` 的 seq 的 Advice 载荷与步骤 1 同一 `basis_seq` 的载荷逐字节相同
- 实现:`apps/hft-core/tests/advisor.rs::core_135_restore_matches_continuous`

### CORE-136 以 `config_source = file` 改参数后重启:检查点仍采用,新参数经 `model_on_params` 生效

- 规则:[实现说明](README.md#实现说明)
- 级别:integ
- 里程碑:M4
- 前置:`dev2_sim()` 绑 `portfolio-carry`,有一份可用检查点;`blobs.strategy_params` 换成改了一个非结构参数的版本,`config_source = "file"`
- 步骤:
  1. 重启,等 advisor 追上 journal 末尾
- 期望:
  - 无 `ModelCheckpointDiscarded`;追上之后恰好一次 `model_on_params`,参数为新版本(测试替身记录调用);之后的 Advice 按新参数计算
- 实现:`apps/hft-core/tests/advisor.rs::core_136_params_change_keeps_ckpt`

### CORE-137 30 MiB 的 Model 分块写出:每次 `poll_once` 至多一块,期间 advisor 停在同一 `last_seq`

- 规则:[内存与 journal 段(按主机类唯一归属)](README.md#内存与-journal-段按主机类唯一归属)
- 级别:unit
- 里程碑:M4
- 前置:hft-testkit `probe-target` 的 Model 参数使 `snapshot_bytes` = 30 MiB、`PATH_VERSION = 1`;`advisor.checkpoint_chunk_kib = 1024`;用 hft-core 的 `CheckpointStore` 实现写入临时目录
- 步骤:
  1. 触发一次检查点,反复 `poll_once` 直到收尾
  2. 另以 `checkpoint_chunk_kib = 1048576`(一次写完)写同一状态
- 期望:
  - 步骤 1 第一次 `write_chunk` 是路径节;之后每次 `poll_once` 至多一次 `write_chunk`,每块 ≤ 1,048,576 字节;共 ≥ 31 次 `poll_once`;期间 journal 读者位置不变,Model 没有收到任何事件,全部块属于同一 `last_seq`
  - 两次写出的最终文件除 `crc32c` 外逐字节相同(`crc32c` 也相同);从任一份恢复的 Model 状态哈希相同
- 实现:`apps/hft-core/tests/ckpt_store.rs::core_137_chunked_write`

### CORE-138 收尾顺序保证崩溃后总有一代完整可读

- 规则:[内存与 journal 段(按主机类唯一归属)](README.md#内存与-journal-段按主机类唯一归属)
- 级别:table
- 里程碑:M4
- 前置:目录中已有 `.ckpt`(代 A)与 `.prev`(代 Z);写代 B 时在各步之后注入 abort;重启后 advisor 读检查点
- 实现:`apps/hft-core/tests/ckpt_store.rs::core_138_finalize_crash_points`

| 行 | abort 时刻 | 期望 |
|---|---|---|
| CORE-138.1 | 临时文件回填头之前 | 采用代 A;临时文件被删除 |
| CORE-138.2 | 临时文件 fsync 之后、旧文件改名 `.prev` 之前 | 采用代 A |
| CORE-138.3 | 旧文件改名 `.prev` 之后、新文件 rename 之前 | `.ckpt` 缺失,采用 `.prev`(代 A) |
| CORE-138.4 | rename 之后、fsync 目录之前 | 采用代 B |

### CORE-139 写完后两代合计超出 `checkpoint_max_mib` 时跳过本次写、保留已有两代

- 规则:[内存与 journal 段(按主机类唯一归属)](README.md#内存与-journal-段按主机类唯一归属)
- 级别:table
- 里程碑:M4
- 前置:只有一个带 Model 的实例;`advisor.checkpoint_max_mib = 64`;目录中已有 `.ckpt` 30 MiB(代 A)与 `.prev` 30 MiB(代 Z);触发一次检查点,本代大小按行
- 实现:`apps/hft-core/tests/ckpt_store.rs::core_139_disk_limit`

| 行 | 本代大小 | 期望 |
|---|---|---|
| CORE-139.1 | 34 MiB(写完后两代 34 + 30 = 64 MiB) | 写出;`.ckpt` = 新代、`.prev` = 代 A,代 Z 被删除 |
| CORE-139.2 | 34 MiB + 1 B | 跳过:`CheckpointStore` 返回错误,`.ckpt` 与 `.prev` 的字节与 mtime 不变,目录中没有临时文件;`advice` 环上恰好一条 `Note{ModelCkptSkipped}`(P1);下一周期再次尝试并得到同一结果(再一条) |
| CORE-139.3 | 行 1 | `advice` 环上没有 `Note{ModelCkptSkipped}` |

### CORE-140 检查点周期按事件时间,Drain 时另写一次

- 规则:[内存与 journal 段(按主机类唯一归属)](README.md#内存与-journal-段按主机类唯一归属)、[配置文件字段](README.md#配置文件字段)
- 级别:table
- 里程碑:M4
- 前置:`advisor.model_checkpoint_ms = 600000`;回放驱动事件时间从 0 起;墙钟时间不推进
- 实现:`apps/hft-core/tests/ckpt_store.rs::core_140_checkpoint_period`

| 行 | 事件时间推进到 | 期望已完成的检查点次数 |
|---|---|---|
| CORE-140.1 | 599,999 ms | 0 |
| CORE-140.2 | 600,000 ms | 1 |
| CORE-140.3 | 1,199,999 ms | 1 |
| CORE-140.4 | 1,200,000 ms | 2 |
| CORE-140.5 | 900,000 ms 后发 `Drain` | 2(第二次在快照之前写完) |

### CORE-141 检查点目录的默认值与权限

- 规则:[内存与 journal 段(按主机类唯一归属)](README.md#内存与-journal-段按主机类唯一归属)、[配置文件字段](README.md#配置文件字段)
- 级别:proc
- 里程碑:M4
- 前置:`dev2_sim()` 不写 `advisor.checkpoint_dir`,`paths.journal_cold = <tmp>/cold`;`<tmp>/cold/sim/` 由测试按 deploy 路径表建好(属组 `hft-journal-ro`、setgid)
- 步骤:
  1. 运行到写出第一份检查点
- 期望:
  - 文件在 `<tmp>/cold/sim/advisor/<strategy_instance>.ckpt`;文件属组 `hft-journal-ro`,组权限有 `r`、无 `w`
- 实现:`apps/hft-core/tests/ckpt_store.rs::core_141_default_dir_and_mode`

### CORE-142 `c32` `sim` 的 Drain 检查点在 `hft-sim.slice` 写带宽上限内于 `TimeoutStopSec` 前写完

- 规则:[内存与 journal 段(按主机类唯一归属)](README.md#内存与-journal-段按主机类唯一归属)
- 级别:host
- 里程碑:M7
- 环境:统一升级主机(`c32`),`sim` 实例按已登记策略满规模运行,`hft-sim.slice` 写上限 30 MB/s
- 授权:`[需授权]`
- 步骤:
  1. `systemctl stop hft-core@sim`,记录 Drain 开始到 `HeartbeatBlock.state = Stopped` 的时长与检查点文件的 mtime
- 通过标准:单元在 `TimeoutStopSec=20` 内到 `Stopped`;每个实例的 `.ckpt` mtime 晚于 Drain 开始;否则记录不通过并按开发备忘的待核实项处理
- 证据:`ops-log/<UTC 日期>-CORE-142.md`
- 实现:`apps/hft-core/tests/host/CORE-142.sh`

### CORE-143 冷盘占用与容量按表估算核对

- 规则:[内存与 journal 段(按主机类唯一归属)](README.md#内存与-journal-段按主机类唯一归属)
- 级别:host
- 里程碑:M6
- 环境:dev2,`sim` 常驻 ≥ 7 天,`retention.local_days` 按实测调低后的值
- 授权:无(只读 `du`)
- 步骤:
  1. `du -sb <cold_root>/sim <cold_root>/prod` 与 advisor、验证包、archiver 暂存目录
- 通过标准:hft 冷盘合计 ≤ 60 GB
- 证据:`ops-log/<UTC 日期>-CORE-143.md`
- 实现:`apps/hft-core/tests/host/CORE-143.sh`

### CORE-144 整机 `MemoryMax`、slice 上限与合计按主机类生成

- 规则:[内存与 journal 段(按主机类唯一归属)](README.md#内存与-journal-段按主机类唯一归属)、[主机资源隔离(slice、核与出口 IP)](README.md#主机资源隔离slice核与出口-ip)
- 级别:table
- 里程碑:M3
- 前置:`cargo xtask gen-host --host test-<主机类>` 的单元覆盖文件与 slice 文件
- 实现:`tools/xtask/tests/gen_host.rs::core_144_memory_max_tables`

| 行 | 主机类 | 期望 |
|---|---|---|
| CORE-144.1 | `dev2` | `hft-core@prod` 384M、`hft-core@sim` 576M、watchtower 160M、reconciler 128M、ops-bot 128M、archiver 256M、pipeline 128M、treasury 64M、replay-verify 384M、verify 384M、kill-switch 64M、cred-broker 64M、dev2-guard 32M、offbox-watchdog 32M;`hft.slice` `MemoryMax=1376M`、`hftbatch.slice` `MemoryMax=512M` |
| CORE-144.2 | `c8` | `hft-core@prod` 1408M;`hft-core.slice` 3200M、`hftbatch.slice` 2048M;没有 `hft-sim.slice` |
| CORE-144.3 | `c16` | `hft-core@prod` 1536M、`hft-core@sim` 4320M;`hft-core.slice` 3840M、`hft-sim.slice` 4400M、`hftbatch.slice` 4992M |
| CORE-144.4 | `c32` | `hft-core@prod` 2048M、`hft-core@sim` 6144M;`hft-core.slice` 4352M、`hft-sim.slice` 6272M、`hftbatch.slice` 4992M;大页预留 + 三个 slice + `system.slice` 中的 hft 单元 = 25,536 MiB |
| CORE-144.5 | 每类 | 每个 hft 单元 `MemorySwapMax=0` |

### CORE-145 批处理并发规则在全部主机类生效

- 规则:[内存与 journal 段(按主机类唯一归属)](README.md#内存与-journal-段按主机类唯一归属)
- 级别:host
- 里程碑:M3
- 环境:dev2(`verify = true`);另在一台 `verify = false`、未装 `hft-verify` 的主机(M7 升级主机或探针主机)
- 授权:`[需授权]`
- 步骤:
  1. 启动 `hft-replay-verify`;运行期间 `systemctl start hft-journal-archiver hft-pipeline hft-verify`
  2. 等它结束
- 通过标准:步骤 1 中三个单元都未进入 active,`journalctl -u <单元>` 各有一行 `exec-condition` 跳过记录;archiver 与 pipeline 在回放开始前已停止;步骤 2 后 archiver 与 pipeline 恢复 active;未装 `hft-verify` 的主机上回放以 0 退出
- 证据:`ops-log/<UTC 日期>-CORE-145.md`
- 实现:`apps/hft-core/tests/host/CORE-145.sh`

### CORE-146 `hftbatch.slice` 内存不足时先杀 `hft-verify`,再杀 `hft-replay-verify`,不波及 hft-core

- 规则:[内存与 journal 段(按主机类唯一归属)](README.md#内存与-journal-段按主机类唯一归属)、[主机资源隔离(slice、核与出口 IP)](README.md#主机资源隔离slice核与出口-ip)
- 级别:host
- 里程碑:M3
- 环境:dev2,联调窗口外;`hft-core@sim` 运行
- 授权:`[需授权]`
- 步骤:
  1. 让 `hft-verify` 与 `hft-replay-verify` 同时运行,并以测试负载把两者的内存推到 slice 上限 512 MiB 之上
- 通过标准:内核 OOM 记录中第一个被杀的是 `hft-verify`,其次 `hft-replay-verify`;`hft-core@sim` 的 pid 不变;`hft-verify` 的运行记为中止
- 证据:`ops-log/<UTC 日期>-CORE-146.md`(`journalctl -k` 的 OOM 行)
- 实现:`apps/hft-core/tests/host/CORE-146.sh`

### CORE-147 dev2 两个实例空载 1 h 的常驻内存与 CPU

- 规则:[内存与 journal 段(按主机类唯一归属)](README.md#内存与-journal-段按主机类唯一归属)、[延迟与资源契约](README.md#延迟与资源契约)
- 级别:host
- 里程碑:M6
- 环境:dev2,联调窗口内(两个实例都运行),行情正常订阅、无下单
- 授权:`[需授权]`
- 步骤:
  1. 运行 1 h,读两个单元的 `memory.peak` 与 `cpu.stat` 增量
- 通过标准:`hft-core@sim` `memory.peak` ≤ 576 MiB,`hft-core@prod` ≤ 384 MiB;两者合计 CPU < 8%(2 vCPU 的 8% 即 0.16 核)
- 证据:`ops-log/<UTC 日期>-CORE-147.md`
- 实现:`apps/hft-core/tests/host/CORE-147.sh`

### CORE-148 dev2 常态与联调窗口两种状态满载 1 h,`MemAvailable` ≥ 1,000 MiB

- 规则:[内存与 journal 段(按主机类唯一归属)](README.md#内存与-journal-段按主机类唯一归属)、[dev2 护栏](README.md#dev2-护栏)
- 级别:host
- 里程碑:M3
- 环境:dev2;常态 = `sim` + 批处理满载;联调窗口 = `sim` + 测试网 `prod` + reconciler(批处理停)
- 授权:`[需授权]`
- 步骤:
  1. 两种状态各运行 1 h,每 5 s 采 `/proc/meminfo`
- 通过标准:两段的 `MemAvailable` 最小值都 ≥ 1,048,576,000 B(`hft-cred-broker` 运行中,其间至少一次凭证刷新);`hft.slice` 的 `memory.peak` ≤ 1,376 MiB
- 证据:`ops-log/<UTC 日期>-CORE-148.md`
- 实现:`apps/hft-core/tests/host/CORE-148.sh`

### CORE-160 slice 归属与批处理调度属性按主机类生成

- 规则:[主机资源隔离(slice、核与出口 IP)](README.md#主机资源隔离slice核与出口-ip)
- 级别:table
- 里程碑:M3
- 前置:`cargo xtask gen-host --host test-<主机类>` 的生成物
- 实现:`tools/xtask/tests/gen_host.rs::core_160_slice_membership`

| 行 | 主机类 / 对象 | 期望 |
|---|---|---|
| CORE-160.1 | 每类 / `hftbatch.slice` | 文件名 `hftbatch.slice`(根级,不在 `hft.slice` 之下);`CPUWeight=idle`;没有 `CPUQuota=` |
| CORE-160.2 | 每类 / 批处理单元(`hft-verify`、`hft-replay-verify`、`hft-journal-archiver`、`hft-pipeline`、`hft-treasury`) | `Slice=hftbatch.slice`、`Nice=19` |
| CORE-160.3 | `c16`、`c32` / `hft-core@sim` | `Slice=hft-sim.slice`;`hft-sim.slice` 为 `hft.slice` 的子 slice |
| CORE-160.4 | `dev2` / `hft-core@sim` | `Slice=hft-core.slice` |
| CORE-160.5 | 每类 / `hft-kill-switch`、`hft-dev2-guard`(dev2)、`hft-offbox-watchdog`(dev2) | `Slice=system.slice` |
| CORE-160.6 | `c16` / `hft-sim.slice` | `AllowedCPUs=13-15`、`CPUQuota=300%` |
| CORE-160.7 | `c32` / `hft-sim.slice` | `AllowedCPUs=23-27`、`CPUQuota=500%` |
| CORE-160.8 | `dev2` / `hft.slice` | `MemoryMax=1376M`、`MemorySwapMax=0`、`CPUQuota=100%`、`CPUWeight=50` |
| CORE-160.9 | `c8`、`c16`、`c32` / `hft.slice` | 没有 `MemoryMax=` 与 `CPUQuota=` |
| CORE-160.10 | 每类 / 全部单元与 slice | 没有 `IOWeight=`、`IOSchedulingClass=` |

### CORE-161 冷盘 I/O 带宽上限按主机类生成

- 规则:[主机资源隔离(slice、核与出口 IP)](README.md#主机资源隔离slice核与出口-ip)
- 级别:table
- 里程碑:M3
- 前置:`cargo xtask gen-host` 生成物;冷盘设备为主机清单的冷盘卷
- 实现:`tools/xtask/tests/gen_host.rs::core_161_io_bandwidth`

| 行 | 主机类 / slice | 期望 |
|---|---|---|
| CORE-161.1 | `dev2` / `hftbatch.slice` | 对冷盘设备 `IOReadBandwidthMax` 40 MB/s、`IOWriteBandwidthMax` 20 MB/s |
| CORE-161.2 | `c32` / `hftbatch.slice` | 读 100 MB/s、写 40 MB/s |
| CORE-161.3 | `c32` / `hft-sim.slice` | 写 30 MB/s,无读上限 |
| CORE-161.4 | 每类 / `hft-core.slice` | 没有带宽上限 |

### CORE-162 IRQ 专核不在任何 slice 的 `AllowedCPUs` 里

- 规则:[主机资源隔离(slice、核与出口 IP)](README.md#主机资源隔离slice核与出口-ip)
- 级别:table
- 里程碑:M3
- 前置:`cargo xtask gen-host` 生成物,取全部 slice 的 `AllowedCPUs` 并集
- 实现:`tools/xtask/tests/gen_host.rs::core_162_irq_cores_outside_slices`

| 行 | 主机类 | 期望 |
|---|---|---|
| CORE-162.1 | `c32` | 并集不含 1、22;不启用 ENI-B2 时含 28 |
| CORE-162.2 | `c32` 启用 ENI-B2 | 并集不含 1、22、28 |
| CORE-162.3 | `c16` | 并集不含 1;含 13(`hft-sim.slice`) |

### CORE-163 OOM 优先级按主机类生成,dev2 整体上移

- 规则:[主机资源隔离(slice、核与出口 IP)](README.md#主机资源隔离slice核与出口-ip)
- 级别:table
- 里程碑:M3
- 前置:`cargo xtask gen-host` 生成物的 `OOMScoreAdjust=`
- 实现:`tools/xtask/tests/gen_host.rs::core_163_oom_score_adjust`

| 行 | 主机类 | 期望 |
|---|---|---|
| CORE-163.1 | `c32` | `hft-core@prod` −900、`hft-core@sim` −800、kill-switch −900、watchtower/reconciler/ops-bot/`hft-cred-broker` −300、archiver/pipeline/treasury +300、`hft-replay-verify` +900、`hft-verify` +950 |
| CORE-163.2 | `dev2` | `hft-core@sim` 与 `hft-core@prod` +100、watchtower/reconciler/ops-bot +300、archiver/pipeline/treasury +600、`hft-replay-verify` +900、`hft-verify` +950;kill-switch、`hft-dev2-guard`、`hft-offbox-watchdog` −900;`hft-cred-broker` +300 |

### CORE-164 tmpfs 挂载大小与依赖按实例生成

- 规则:[主机资源隔离(slice、核与出口 IP)](README.md#主机资源隔离slice核与出口-ip)
- 级别:table
- 里程碑:M3
- 前置:`cargo xtask gen-host` 生成物
- 实现:`tools/xtask/tests/gen_host.rs::core_164_tmpfs_mounts`

| 行 | 主机类 / 实例 | 期望 |
|---|---|---|
| CORE-164.1 | `dev2` / `sim` | `dev-shm-hft-sim.mount`,`size=224M`;`hft-core@sim` 带 `RequiresMountsFor=/dev/shm/hft/sim` |
| CORE-164.2 | `dev2` / `prod` | `dev-shm-hft-prod.mount`,`size=192M` |
| CORE-164.3 | `c32` / `sim` | `size=1700M` |
| CORE-164.4 | `c32` / `prod` | 没有 tmpfs 挂载单元(热段在 hugetlbfs) |

### CORE-165 tmpfs 空间检查扣除本实例已持有的段,不扣别的实例的段

- 规则:[主机资源隔离(slice、核与出口 IP)](README.md#主机资源隔离slice核与出口-ip)、[启动顺序](README.md#启动顺序)
- 级别:table
- 里程碑:M3
- 前置:`budget::shm_space_ok(avail, held_self, need)` 纯函数,单位字节;`need` = 192 MiB(`dev2` `sim`)
- 实现:`apps/hft-core/tests/budget.rs::core_165_shm_space_check`

| 行 | `avail` / `held_self` | 期望 |
|---|---|---|
| CORE-165.1 | 192 MiB / 0 | `Ok` |
| CORE-165.2 | 192 MiB − 1 B / 0 | `Err(ShmSpace)` |
| CORE-165.3 | 40 MiB / 152 MiB | `Ok` |
| CORE-165.4 | 40 MiB / 152 MiB − 1 B | `Err(ShmSpace)` |

### CORE-166 SIGKILL 后本实例热段留在 tmpfs:重启通过空间检查并复用

- 规则:[主机资源隔离(slice、核与出口 IP)](README.md#主机资源隔离slice核与出口-ip)
- 级别:proc
- 里程碑:M3
- 前置:`dev2_sim()`;测试 tmpfs `size=224M`;运行到 `Running` 后另占满 tmpfs,使剩余可用 < 48 MiB(不足一份新 `seq` 段)
- 步骤:
  1. SIGKILL,再启动
- 期望:
  - 无 `ShmSpace` 错误;步 4 复用原热段(段文件 inode 不变);到 `Running`
- 实现:`apps/hft-core/tests/shm.rs::core_166_reuse_own_segments`

### CORE-167 别的实例的段占满剩余空间时本实例以 `ShmSpace` 退出 3

- 规则:[主机资源隔离(slice、核与出口 IP)](README.md#主机资源隔离slice核与出口-ip)
- 级别:proc
- 里程碑:M3
- 前置:`dev2_sim()`,`rt.host.mode = "dev"`;同一 tmpfs 中放入 `prod` 实例目录下 200 MiB 的段文件,剩余可用 24 MiB;本实例无已有段
- 步骤:
  1. 启动
- 期望:
  - 退出码 3;错误行 `ShmSpace`;没有建心跳块
- 实现:`apps/hft-core/tests/shm.rs::core_167_other_instance_fills_space`

### CORE-168 出口 IP 的划分校验

- 规则:[主机资源隔离(slice、核与出口 IP)](README.md#主机资源隔离slice核与出口-ip)、[多账户](README.md#多账户)
- 级别:table
- 里程碑:M3
- 前置:`host.egress` = `{prod = [10.0.1.10, 10.0.1.11], sim = [10.0.3.10]}`(行 .6–.8 另有 `prod_bulk = [10.0.2.10]`);`config::load_and_validate`
- 实现:`apps/hft-core/tests/egress.rs::core_168_egress_partition`

| 行 | 配置 | 期望错误列表 |
|---|---|---|
| CORE-168.1 | `c32_prod()`,全部 `feed.conns[].src_ip` 与 `gw.src_ip` 取自 `prod` | 空 |
| CORE-168.2 | `c32_prod()`,某 `feed.conns` 条目 `src_ip = 10.0.3.10` | 恰好 {`EgressIpOverlap`} |
| CORE-168.3 | `c32_sim()`,全部 `src_ip = 10.0.3.10` | 空 |
| CORE-168.4 | `c32_sim()`,某条 `src_ip = 10.0.1.10` | 恰好 {`EgressIpOverlap`} |
| CORE-168.5 | `egress.toml` 的 `sim = [10.0.1.11]`(与 `prod` 相交),`c32_prod()` | 含 `EgressIpOverlap` |
| CORE-168.6 | `c32_prod()`,`stream = md` 的条目用 `10.0.2.10` | 空 |
| CORE-168.7 | `c32_prod()`,`stream = bbo` 的条目用 `10.0.2.10` | 恰好 {`EgressIpOverlap`} |
| CORE-168.8 | `c32_prod()`,`venues.okx-swap.gw.src_ip.okx-1 = 10.0.2.10` | 恰好 {`EgressIpOverlap`} |
| CORE-168.9 | `host.egress` 指向不存在的文件 | 恰好 {`FileRead`} |
| CORE-168.10 | `c32_prod()` 的 `venues.binance-usdm.gw.accounts = [bn-1, bn-2]`,`gw.src_ip` 两个账户都是 `10.0.1.10` | 恰好 {`SharedAccountIp`} |
| CORE-168.11 | 同上,`bn-2` 改为 `10.0.1.11` | 空 |

### CORE-169 snap-fetch 复用第一条 `stream = md` 的 `src_ip`,acct-fetch 复用该账户的 `gw.src_ip`

- 规则:[配置文件字段](README.md#配置文件字段)
- 级别:unit
- 里程碑:M3
- 前置:`c32_prod()` 的 `venues.okx-swap.feed.conns = [{stream = bbo, src_ip = 10.0.1.10}, {stream = md, src_ip = 10.0.2.10}, {stream = md, src_ip = 10.0.1.11}]`,`prod_bulk = [10.0.2.10]`;`venues.okx-swap.gw.src_ip.okx-1 = 10.0.1.11`
- 步骤:
  1. 调用 `venues::snap_src_ip(&cfg, "okx-swap")` 与 `venues::acct_src_ip(&cfg, "okx-swap", "okx-1")`
- 期望:
  - 前者返回 `10.0.2.10`;后者返回 `10.0.1.11`
- 实现:`apps/hft-core/tests/egress.rs::core_169_snap_fetch_src_ip`

### CORE-170 同机时 `sim` 的全部线程只在 sim 核上运行

- 规则:[主机资源隔离(slice、核与出口 IP)](README.md#主机资源隔离slice核与出口-ip)
- 级别:host
- 里程碑:M7
- 环境:统一升级主机(`c32`),`prod` 与 `sim` 都在 `Running`
- 授权:`[需授权]`
- 步骤:
  1. 读 `sim` 进程每个线程的 `/proc/<pid>/task/<tid>/status` `Cpus_allowed_list`,并以 `perf sched` 采样 60 s 记录各线程实际运行的核
- 通过标准:每个线程的 `Cpus_allowed_list` ⊆ `23-27`;采样中 `sim` 线程没有出现在核 1、5–17、22、28 上
- 证据:`ops-log/<UTC 日期>-CORE-170.md`
- 实现:`apps/hft-core/tests/host/CORE-170.sh`

### CORE-171 `sim` 的交易所连接源地址全是 sim 出口 IP,建连自检落在 ENI-C

- 规则:[主机资源隔离(slice、核与出口 IP)](README.md#主机资源隔离slice核与出口-ip)
- 级别:host
- 里程碑:M7
- 环境:同 CORE-170
- 授权:`[需授权]`
- 步骤:
  1. `ss -tnp` 列出 `sim` 进程的全部 TCP 连接;读 conn-builder 的建连自检结果(`ConnState` 记录中的网卡)
- 通过标准:每条连接的本端地址 ∈ `egress.toml` 的 `sim` 列表;自检网卡全部是 ENI-C;`prod` 的连接本端地址 ∈ `prod`(或 `prod_bulk`,只限 md 与公共 REST)
- 证据:`ops-log/<UTC 日期>-CORE-171.md`
- 实现:`apps/hft-core/tests/host/CORE-171.sh`

### CORE-172 `sim` 的一个 `src_ip` 被路由到 ENI-B 时该连接不交接、该 venue 保持 Down

- 规则:[主机资源隔离(slice、核与出口 IP)](README.md#主机资源隔离slice核与出口-ip)
- 级别:host
- 里程碑:M7
- 环境:同 CORE-170;`sim` 停止
- 授权:`[需授权]`(改主机路由)
- 步骤:
  1. 加一条策略路由,使 `sim` 某 venue(`okx-swap`)的 `src_ip` 的出口走 ENI-B;启动 `sim`
  2. 观察 2 min 后删除该路由
- 通过标准:步骤 1 后 `sim` journal 中 `okx-swap` 的连接级 `ConnState` 一直为 `Down`,该 venue 没有任何 inst 可用;其余 venue `Up`;ENI-B 上没有来自 `sim` 进程的已交接连接(`ss -tnp`)
- 证据:`ops-log/<UTC 日期>-CORE-172.md`
- 实现:`apps/hft-core/tests/host/CORE-172.sh`

### CORE-173 ENI-B 的 IRQ 核上只有 ENI-B 队列的中断增长

- 规则:[主机资源隔离(slice、核与出口 IP)](README.md#主机资源隔离slice核与出口-ip)
- 级别:host
- 里程碑:M7
- 环境:同 CORE-170,两个实例满订阅运行
- 授权:`[需授权]`
- 步骤:
  1. 间隔 60 s 读两次 `/proc/interrupts`
- 通过标准:核 1 列只有 ENI-B 队列的计数增长;ENI-C 队列只在核 22 增长;ENI-A 只在核 0
- 证据:`ops-log/<UTC 日期>-CORE-173.md`
- 实现:`apps/hft-core/tests/host/CORE-173.sh`

### CORE-174 `sim` 压到 CPU 与内存上限时 `prod` 不受 OOM 影响

- 规则:[主机资源隔离(slice、核与出口 IP)](README.md#主机资源隔离slice核与出口-ip)
- 级别:host
- 里程碑:M7
- 环境:同 CORE-170
- 授权:`[需授权]`
- 步骤:
  1. 在 `hft-sim.slice` 内跑 `stress-ng` 把 CPU 压满 `CPUQuota=500%`、内存推到 `MemoryMax` 之上,持续 10 min
- 通过标准:`hft-core@prod` 的 pid 不变、`journalctl -k` 中没有选中 `prod` 的 OOM 记录;`prod` 的心跳计数持续前进;同机干扰的测量按 [testing §8.2](../../docs/testing.md#82-延迟验收的测法) 另行判定
- 证据:`ops-log/<UTC 日期>-CORE-174.md`
- 实现:`apps/hft-core/tests/host/CORE-174.sh`

### CORE-175 冷盘四方并发时 `prod` 的 persist 滞后在 hft-journal 预算内

- 规则:[内存与 journal 段(按主机类唯一归属)](README.md#内存与-journal-段按主机类唯一归属)、[主机资源隔离(slice、核与出口 IP)](README.md#主机资源隔离slice核与出口-ip)
- 级别:host
- 里程碑:M7
- 环境:同 CORE-170;冷盘 gp3 按主机清单预置
- 授权:`[需授权]`
- 步骤:
  1. 同时运行 archiver(补传积压)、`hft-replay-verify`、`sim` 的检查点写出与 `prod` 满订阅,持续 1 h
- 通过标准:`prod` 的 `journal_persist_lag_ms` p99 ≤ [hft-journal 延迟与资源契约](../../crates/hft-journal/README.md#延迟与资源契约) 的预算
- 证据:`ops-log/<UTC 日期>-CORE-175.md`
- 实现:`apps/hft-core/tests/host/CORE-175.sh`

### CORE-176 hostprobe 发现出口 IP 未配在对应网卡或未关联 EIP 时以 `HostProfile` 退出 3

- 规则:[主机资源隔离(slice、核与出口 IP)](README.md#主机资源隔离slice核与出口-ip)、[HostProfile](README.md#hostprofile)
- 级别:host
- 里程碑:M7
- 环境:统一升级主机(`c32`),两个实例都停止
- 授权:`[需授权]`(改 ENI 地址与 EIP 关联)
- 步骤:
  1. 把 `sim` 列表中的一个 IP 从 ENI-C 移除,运行 hostprobe,启动 `hft-core@sim`
  2. 恢复该 IP 但解除其 EIP 关联,重复;最后恢复
- 通过标准:两次都退出码 3,错误行 `HostProfile`,详情分别含该 IP 与"未配在 ENI-C""未关联 EIP"的项名
- 证据:`ops-log/<UTC 日期>-CORE-176.md`
- 实现:`apps/hft-core/tests/host/CORE-176.sh`

### CORE-177 dev2 上 `sim` 的连接从唯一网卡的第二个私有 IP 出站

- 规则:[主机资源隔离(slice、核与出口 IP)](README.md#主机资源隔离slice核与出口-ip)
- 级别:host
- 里程碑:M4
- 环境:dev2,`hft-core@sim` 运行
- 授权:无(只读 `ss`)
- 步骤:
  1. `ss -tnp` 列出 `sim` 进程的 TCP 连接
- 通过标准:每条连接的本端地址都是 `egress.toml` 的 `sim` 地址(网卡的第二个私有 IP),没有一条使用旧系统的主私有 IP
- 证据:`ops-log/<UTC 日期>-CORE-177.md`
- 实现:`apps/hft-core/tests/host/CORE-177.sh`

### CORE-180 停机信号 `MemAvailable` < 600 MiB 连续 3 次(15 s)才动作

- 规则:[dev2 护栏](README.md#dev2-护栏)
- 级别:table
- 里程碑:M3
- 前置:护栏决策逻辑以 5 s 采样驱动(假时钟);共机模式;`hft.slice` `memory.current = 300 MiB`(因果判定成立);其余信号低于阈值;`FakeSystemctl` 记录调用
- 实现:`deploy/dev2/tests/test_guard.py::core_180_mem_available_signal`

| 行 | 连续采样的 `MemAvailable`(MiB) | 期望 |
|---|---|---|
| CORE-180.1 | 599, 599 | 无动作 |
| CORE-180.2 | 599, 599, 599 | 第 3 个采样执行一级动作 |
| CORE-180.3 | 600, 600, 600, 600 | 无动作 |
| CORE-180.4 | 599, 599, 650, 599, 599 | 无动作(连续计数被打断) |

### CORE-181 旧系统所在 cgroup 的 `memory.pressure` `some avg10` > 5% 持续 30 s 才动作

- 规则:[dev2 护栏](README.md#dev2-护栏)
- 级别:table
- 里程碑:M3
- 前置:同 CORE-180;旧系统 cgroup 为 `system.slice`
- 实现:`deploy/dev2/tests/test_guard.py::core_181_legacy_memory_pressure`

| 行 | 采样序列 | 期望 |
|---|---|---|
| CORE-181.1 | 5.00% 持续 60 s | 无动作 |
| CORE-181.2 | 5.01% 从 t = 0 起,观察到 t = 25 s | 无动作 |
| CORE-181.3 | 5.01% 从 t = 0 起,到 t = 30 s | t = 30 s 的采样执行一级动作 |

### CORE-182 旧系统所在 cgroup 的 `cpu.pressure` `some avg10` > 20% 持续 60 s 才动作

- 规则:[dev2 护栏](README.md#dev2-护栏)
- 级别:table
- 里程碑:M3
- 前置:同 CORE-180,但因果判定由 `hftbatch.slice` CPU 均值 0.5 核成立
- 实现:`deploy/dev2/tests/test_guard.py::core_182_legacy_cpu_pressure`

| 行 | 采样序列 | 期望 |
|---|---|---|
| CORE-182.1 | 20.00% 持续 120 s | 无动作 |
| CORE-182.2 | 20.01% 到 t = 55 s | 无动作 |
| CORE-182.3 | 20.01% 到 t = 60 s | t = 60 s 执行一级动作 |

### CORE-183 swap-in 速率 > 1,000 页/min 持续 2 min 才动作

- 规则:[dev2 护栏](README.md#dev2-护栏)
- 级别:table
- 里程碑:M3
- 前置:同 CORE-180;速率 = 相邻采样 `pswpin` 增量 × 12(页/min)
- 实现:`deploy/dev2/tests/test_guard.py::core_183_swap_in_rate`

| 行 | 每 5 s 的 `pswpin` 增量 | 期望 |
|---|---|---|
| CORE-183.1 | 83(996 页/min)持续 5 min | 无动作 |
| CORE-183.2 | 84(1,008 页/min)持续 115 s | 无动作 |
| CORE-183.3 | 84 持续 120 s | 第 120 s 执行一级动作 |

### CORE-184 因果判定:hft 自身用量不明显时只告警,不写 `tripped`、不计入退路

- 规则:[dev2 护栏](README.md#dev2-护栏)
- 级别:table
- 里程碑:M3
- 前置:`MemAvailable` = 500 MiB 连续 3 次;两个 slice 的用量按行(CPU 为该 15 s 窗口内 `usage_usec` 增量折算的均值核数)
- 实现:`deploy/dev2/tests/test_guard.py::core_184_causality`

| 行 | `hft.slice` CPU / 内存;`hftbatch.slice` CPU / 内存 | 期望 |
|---|---|---|
| CORE-184.1 | 0.25 核 / 256 MiB;0.25 核 / 256 MiB | 无停机调用;一条 P1 告警,文本含"hft 用量不明显,未动作";`tripped` 不存在;`events.jsonl` 新增一行,动作为"未动作" |
| CORE-184.2 | 0.25 核 / 256 MiB + 1 B;0 / 0 | 一级动作 |
| CORE-184.3 | 0 / 0;0.2501 核 / 0 | 一级动作 |
| CORE-184.4 | 0.3 核 / 0;0 / 0 | 一级动作 |

### CORE-185 只告警的信号从不触发停机

- 规则:[dev2 护栏](README.md#dev2-护栏)
- 级别:table
- 里程碑:M3
- 前置:停机类信号全部低于阈值;因果判定成立(`hft.slice` 300 MiB);每行单独
- 实现:`deploy/dev2/tests/test_guard.py::core_185_alert_only_signals`

| 行 | 信号 | 期望 |
|---|---|---|
| CORE-185.1 | 整机 PSI `memory some avg10` 10.01% 持续 30 s | 一条 P2;无 systemctl 调用;无 `tripped` |
| CORE-185.2 | 整机 PSI `memory some avg10` 10.00% 持续 60 s | 无告警 |
| CORE-185.3 | 整机 PSI `cpu some avg10` 40.01% 持续 60 s | 一条 P2;无 systemctl 调用 |
| CORE-185.4 | swap 已用量 5 min 内增长 64 MiB + 1 B | 一条 P1;无 systemctl 调用 |
| CORE-185.5 | swap 已用量 5 min 内增长 64 MiB | 无告警 |
| CORE-185.6 | 旧系统主单元 `activating` 持续 30 s | 一条 P1(含"hft 未动作");无 systemctl 调用 |
| CORE-185.7 | 旧系统状态文件年龄 31 s 持续 30 s | 一条 P1;无 systemctl 调用 |
| CORE-185.8 | 旧系统主单元非 active 30 s,同时 `MemAvailable` 500 MiB 连续 3 次 | 一级动作(按资源信号处理)且一条旧系统健康 P1 |

### CORE-186 一级动作:先写 `tripped`,再按序停批处理

- 规则:[dev2 护栏](README.md#dev2-护栏)
- 级别:unit
- 里程碑:M3
- 前置:联调窗口外;五个批处理单元都 active;`MemAvailable` 500 MiB 连续 3 次,因果判定成立
- 步骤:
  1. 驱动第 3 个采样
- 期望:
  - `/var/lib/hft/guard/tripped`(测试根下)先于第一次 systemctl 调用写出,内容含 UTC 时刻、信号名 `MemAvailable` 与级别 1
  - systemctl 调用依次为 `stop hft-verify`、`stop hft-replay-verify`、`stop hft-journal-archiver`、`stop hft-pipeline`、`stop hft-treasury`;没有对 hft-core 实例的调用
  - 一条 P1 告警经护栏自己的 Telegram token 发出
- 实现:`deploy/dev2/tests/test_guard.py::core_186_level_one_sequence`

### CORE-187 二级动作:一级后 30 s 信号仍在且 `hft.slice` 用量仍明显

- 规则:[dev2 护栏](README.md#dev2-护栏)
- 级别:table
- 里程碑:M3
- 前置:t = 0 执行了一级动作;此后 `MemAvailable` 保持 500 MiB;`hft-core@prod`(测试网)与 `hft-reconciler` active;`TimeoutStopSec=20`
- 实现:`deploy/dev2/tests/test_guard.py::core_187_level_two`

| 行 | 条件 | 期望 |
|---|---|---|
| CORE-187.1 | `hft.slice` 300 MiB,观察到 t = 25 s | 无二级动作 |
| CORE-187.2 | `hft.slice` 300 MiB,到 t = 30 s | systemctl 调用依次为 `stop hft-core@prod`、`stop hft-reconciler`、`stop hft-core@sim`;之后等待两个实例 `HeartbeatBlock.state = Stopped` 或 20 s 超时;一条 P1(不是 P0) |
| CORE-187.3 | 一级后 `hft.slice` 0.1 核 / 100 MiB(压力来自已停的批处理之外) | 无二级动作;一条 P1"hft 用量不明显,未动作" |
| CORE-187.4 | 每一行 | 从不出现对 `hft-watchtower`、`hft-ops-bot`、`hft-kill-switch` 的调用 |

### CORE-188 联调窗口内一级无可停单元,信号持续 30 s 即进入二级

- 规则:[dev2 护栏](README.md#dev2-护栏)
- 级别:unit
- 里程碑:M3
- 前置:`/run/hft/window/active` 存在,批处理全部 inactive;`hft.slice` 300 MiB;`MemAvailable` 500 MiB 从 t = −10 s 起
- 步骤:
  1. 驱动到 t = 0(第 3 个采样)与 t = 30 s
- 期望:
  - t = 0:写 `tripped`,没有 systemctl 调用
  - t = 30 s:二级动作的三次 `stop` 按 CORE-187.2 的顺序
- 实现:`deploy/dev2/tests/test_guard.py::core_188_window_level_two`

### CORE-189 二级停掉 `sim` 后,信号回落满 30 min 自动启动一次

- 规则:[dev2 护栏](README.md#dev2-护栏)
- 级别:table
- 里程碑:M4
- 前置:t = 0 二级动作停掉 `hft-core@sim`(该单元此后无人工启停);本 UTC 日尚未自动重启;停机类信号自 t = 60 s 起全部低于阈值,除非行内另写
- 实现:`deploy/dev2/tests/test_guard.py::core_189_auto_restart_sim`

| 行 | 条件 | 期望 |
|---|---|---|
| CORE-189.1 | 观察到 t = 60 s + 29 min 55 s | 无 `start` 调用 |
| CORE-189.2 | 到 t = 60 s + 30 min | 恰好一次 `systemctl start hft-core@sim.service`;一条 P1;`tripped` 仍在 |
| CORE-189.3 | t = 60 s + 20 min 时 `MemAvailable` 再次 590 MiB 一个采样 | 30 min 从该采样之后重新计;t = 60 s + 30 min 时无 `start` |
| CORE-189.4 | t = 10 min 时有人 `systemctl start hft-core@sim`,t = 12 min 又 `stop` | 之后不自动启动 |

### CORE-190 同一 UTC 日第二次被二级动作停掉后不再自动启动,跨过 UTC 0 点也不

- 规则:[dev2 护栏](README.md#dev2-护栏)
- 级别:unit
- 里程碑:M4
- 前置:UTC 23:00 第一次二级动作停 `sim`,23:31 自动启动;23:40 第二次二级动作停 `sim`;23:41 起信号全部低于阈值
- 步骤:
  1. 驱动到次日 UTC 02:00
- 期望:
  - 23:40 之后没有 `start` 调用;23:40 发出一条 P1,内容为等人工启动
- 实现:`deploy/dev2/tests/test_guard.py::core_190_second_stop_same_day`

### CORE-191 护栏只启动 `hft-core@sim`,从不清除 `tripped`

- 规则:[dev2 护栏](README.md#dev2-护栏)
- 级别:prop
- 里程碑:M3
- 性质:任意信号序列下,护栏发出的 systemctl 调用 ⊆ {对白名单单元的 `stop`} ∪ {`start hft-core@sim.service`};从不 `start` `hft-core@prod`、任何批处理单元或其定时器;从不删除 `tripped`;每个 UTC 日至多一次 `start`
- 生成:24 h 的 5 s 采样;每个信号按分段随机游走,在阈值两侧各 50% 时间;两个 slice 的用量在因果阈值两侧随机;随机插入窗口开关与人工启停
- 参考:断言上述集合包含关系与计数
- 例数:PR 200;每夜 5 000
- 实现:`deploy/dev2/tests/test_guard.py::core_191_guard_command_set`

### CORE-192 `events.jsonl` 记录每次触发、自动重启与标志变化

- 规则:[dev2 护栏](README.md#dev2-护栏)
- 级别:unit
- 里程碑:M3
- 前置:依次驱动:一次只告警的触发(CORE-184.1 的条件)、一次一级、一次二级、一次自动重启、一次 `hft-dev2-guard clear`
- 步骤:
  1. 读 `events.jsonl`
- 期望:
  - 恰好 5 行,每行一个 JSON 对象,含时刻、信号名与值、`hft.slice` 与 `hftbatch.slice` 的 CPU 与内存用量、动作;5 行的动作依次为"未动作""一级""二级""自动重启 sim""清除标志"
  - 按动作统计一周内的停机次数(退路 (b) 的口径)= 2(只计一级与二级)
- 实现:`deploy/dev2/tests/test_guard.py::core_192_events_jsonl`

### CORE-193 `tripped` 不因守护重启而消失,只由 `clear` 清除

- 规则:[dev2 护栏](README.md#dev2-护栏)
- 级别:unit
- 里程碑:M3
- 前置:`tripped` 存在
- 步骤:
  1. 重启护栏进程(新实例读同一状态目录),驱动 10 min 的低于阈值采样
  2. 运行 `hft-dev2-guard clear`
- 期望:
  - 步骤 1 后 `tripped` 仍在,字节不变
  - 步骤 2 后 `tripped` 不存在;`events.jsonl` 新增一行"清除标志"
- 实现:`deploy/dev2/tests/test_guard.py::core_193_tripped_survives_restart`

### CORE-194 专用主机模式(退路 (b))只看 `MemAvailable` < 300 MiB,不做因果判定

- 规则:[dev2 护栏](README.md#dev2-护栏)
- 级别:table
- 里程碑:M3
- 前置:护栏配置为专用主机列;`hft.slice` 0.1 核 / 100 MiB
- 实现:`deploy/dev2/tests/test_guard.py::core_194_dedicated_host_mode`

| 行 | 采样 | 期望 |
|---|---|---|
| CORE-194.1 | `MemAvailable` 299 MiB 连续 3 次 | 一级动作 |
| CORE-194.2 | `MemAvailable` 300 MiB 连续 6 次 | 无动作 |
| CORE-194.3 | 采样源中旧系统 cgroup 的 `cpu.pressure` 为 90% | 不读取该信号,无动作、无告警 |
| CORE-194.4 | swap-in 每 5 s 增量 200 持续 5 min | 无动作 |

### CORE-195 `hft-shm-release` 只在 `state = Stopped` 时删除热段

- 规则:[dev2 护栏](README.md#dev2-护栏)、[HeartbeatBlock](README.md#heartbeatblock)
- 级别:table
- 里程碑:M3
- 前置:测试目录模拟 `/dev/shm/hft/sim/`:心跳块(`state` 按行)与 3 个段文件;以 `hft-shm-release sim` 运行(路径根指向测试目录)
- 实现:`deploy/tests/test_shm_release.py::core_195_release_only_when_stopped`

| 行 | `HkLine.state` | 期望 |
|---|---|---|
| CORE-195.1 | 5(`Stopped`) | 3 个段文件被删除;心跳块保留 |
| CORE-195.2 | 4(`Draining`) | 不删除任何文件 |
| CORE-195.3 | 3(`Running`) | 不删除 |
| CORE-195.4 | 1(`Starting`) | 不删除 |
| CORE-195.5 | 心跳块不存在 | 不删除 |
| CORE-195.6 | 心跳块 `magic` 错 | 不删除 |

### CORE-196 干净 Drain 释放热段后,下次从冷副本恢复且 `tail_lost = false`

- 规则:[dev2 护栏](README.md#dev2-护栏)、[启动顺序](README.md#启动顺序)
- 级别:proc
- 里程碑:M3
- 前置:`dev2_sim()`,运行到 `Running`;`auto_resume_after_clean_drain = true`
- 步骤:
  1. SIGTERM,等退出 0;运行 `hft-shm-release sim`
  2. 启动
- 期望:
  - 步骤 1 后热段根下只剩心跳块
  - 步骤 2:步 4 没有 `Note{JournalTailLost}`;`StartupStep` 步 7 结束记录 `v[3]` 的 bit 2 = 0;闸门通过时 `Recovery` 自动解除(hft-risk 的 `CapCleared` 注记)
- 实现:`apps/hft-core/tests/shm.rs::core_196_clean_drain_release_recover`

### CORE-197 SIGKILL 或 Drain 超时被杀时热段保留

- 规则:[dev2 护栏](README.md#dev2-护栏)
- 级别:table
- 里程碑:M3
- 前置:`dev2_sim()` 运行到 `Running`;停机方式按行;之后运行 `hft-shm-release sim`
- 实现:`apps/hft-core/tests/shm_release.rs::core_197_hot_segments_kept_on_unclean_stop`

| 行 | 停机方式 | 期望 |
|---|---|---|
| CORE-197.1 | SIGKILL | 段文件全部保留;心跳 `state = 3` |
| CORE-197.2 | SIGTERM 后 Drain 卡住(测试钩子使 Drain 不结束),2 s 后 SIGKILL | 段文件全部保留;心跳 `state = 4` |

### CORE-200 护栏守护不在运行时 `hft-core@*` 不启动,守护重启不连带 hft-core

- 规则:[dev2 护栏](README.md#dev2-护栏)
- 级别:host
- 里程碑:M3
- 环境:dev2
- 授权:`[需授权]`
- 步骤:
  1. `systemctl stop hft-dev2-guard`;`systemctl start hft-core@sim`
  2. `systemctl start hft-dev2-guard`;`systemctl start hft-core@sim`,等 `Running`
  3. `systemctl restart hft-dev2-guard`
- 通过标准:步骤 1 `hft-core@sim` 在 `ExecStartPre` 失败、未产生 hft-core 进程;步骤 2 启动成功;步骤 3 后 `hft-core@sim` 的 pid 与 `ActiveEnterTimestamp` 不变
- 证据:`ops-log/<UTC 日期>-CORE-200.md`
- 实现:`apps/hft-core/tests/host/CORE-200.sh`

### CORE-201 `hft-window start` 的顺序:写标志、停批处理与定时器、再启 reconciler 与 `prod`

- 规则:[dev2 护栏](README.md#dev2-护栏)
- 级别:host
- 里程碑:M6
- 环境:dev2,批处理定时器启用,`hft-journal-archiver` active
- 授权:`[需授权]`
- 步骤:
  1. `hft-window start`,记录各单元状态变化的时间戳(`journalctl -o short-precise`)
- 通过标准:`/run/hft/window/active` 的 mtime 早于任何单元状态变化;`hft-verify.timer`、`hft-replay-verify.timer` 与正在运行的批处理单元在 `hft-reconciler`、`hft-core@prod` 启动之前停止;再执行一次 `hft-window start` 不改变任何单元状态
- 证据:`ops-log/<UTC 日期>-CORE-201.md`
- 实现:`apps/hft-core/tests/host/CORE-201.sh`

### CORE-202 读者仍映射已删除热段时 `hft-window stop` 保留窗口标志

- 规则:[dev2 护栏](README.md#dev2-护栏)
- 级别:host
- 里程碑:M6
- 环境:dev2,联调窗口内,`hft-core@prod` 运行
- 授权:`[需授权]`
- 步骤:
  1. 用测试读者进程 `mmap` 一个 `prod` 热段文件并保持映射
  2. `hft-window stop`
  3. 结束测试读者,再 `hft-window stop`
- 通过标准:步骤 2:`/dev/shm/hft/prod` 的 `statvfs` 已用量高于心跳块所占的 4 KiB,窗口标志保留,批处理未启动,一条 P1;步骤 3:已用量回落到 4 KiB,标志删除,常驻批处理单元与两个定时器恢复
- 证据:`ops-log/<UTC 日期>-CORE-202.md`
- 实现:`apps/hft-core/tests/host/CORE-202.sh`

### CORE-203 批处理单元的 `ExecCondition=` 逐项检查

- 规则:[dev2 护栏](README.md#dev2-护栏)
- 级别:host
- 里程碑:M3
- 环境:dev2,联调窗口外,`hft-replay-verify` 未运行
- 授权:`[需授权]`
- 步骤:
  1. 依次在三种状态下 `systemctl start hft-treasury`:`tripped` 存在;`/run/hft/window/active` 存在;`hft-core@prod` active(另两项不存在)
  2. 三项都不存在时再 `systemctl start hft-treasury`
- 通过标准:步骤 1 三次都被跳过(单元未进入 active,`journalctl` 有 `exec-condition` 跳过记录);步骤 2 进入 active
- 证据:`ops-log/<UTC 日期>-CORE-203.md`
- 实现:`apps/hft-core/tests/host/CORE-203.sh`

### CORE-210 护栏演练 1:`hft.slice` 内存压力依次触发一级与二级

- 规则:[dev2 护栏](README.md#dev2-护栏)
- 级别:drill
- 里程碑:M3
- 环境:dev2,联调窗口外,`sim` 常驻,批处理定时器启用
- 授权:`[需授权]`(在旧系统的生产主机上制造内存压力)
- 步骤:按 [incident-response P](../../docs/runbooks/incident-response.md#p-dev2-护栏触发) 的演练记录要求;在 `hft-core.slice` 中以 `systemd-run --slice=hft-core.slice stress-ng --vm 1` 推高占用,使 `MemAvailable` < 600 MiB 持续 ≥ 60 s;之后停止压力并按 P 节恢复(含 `hft-dev2-guard clear`)
- 通过标准:`events.jsonl` 依次有一级与二级;一级的第一个 `stop` 是 `hft-verify`;二级后两个 hft-core 实例 `Stopped` 或 inactive;`tripped` 存在;旧系统主单元 `NRestarts` 不变、状态文件每 ≤ 30 s 更新、其 cgroup 两项 PSI 的每个采样都低于护栏阈值
- 证据:`ops-log/<UTC 日期>-CORE-210.md`(`events.jsonl` 片段、旧系统指标)
- 实现:`apps/hft-core/tests/host/CORE-210.sh`

### CORE-211 护栏演练 2:旧系统状态文件停更只告警

- 规则:[dev2 护栏](README.md#dev2-护栏)
- 级别:drill
- 里程碑:M3
- 环境:dev2,两个 hft-core 实例运行(`prod` 在联调窗口内)
- 授权:`[需授权]`(改护栏配置,不动旧系统单元)
- 步骤:把护栏读取的旧系统状态文件路径指向一个停止更新的测试文件,保持 2 min,再恢复(步骤记录按 [incident-response P](../../docs/runbooks/incident-response.md#p-dev2-护栏触发))
- 通过标准:只有旧系统健康 P1(含"hft 未动作");两个实例 pid 不变,两份 journal 中这段时间没有 `Drain`;无 `tripped`
- 证据:`ops-log/<UTC 日期>-CORE-211.md`
- 实现:`apps/hft-core/tests/host/CORE-211.sh`

### CORE-212 护栏演练 3:联调窗口的开关与窗口内崩溃

- 规则:[dev2 护栏](README.md#dev2-护栏)
- 级别:drill
- 里程碑:M6
- 环境:dev2
- 授权:`[需授权]`
- 步骤:按 [incident-response P](../../docs/runbooks/incident-response.md#p-dev2-护栏触发):① `hft-window start`;手工 `systemctl start hft-replay-verify`;② `hft-window stop`;③ 再 `hft-window start`,`kill -9` 测试网 `prod`,`hft-window stop`;④ 按提示再 `start`、等干净 Drain 后 `stop`
- 通过标准:① 批处理全部 inactive,`hft-replay-verify` 被 `ExecCondition` 跳过,`hft.slice` `memory.peak` ≤ 1,376 MiB;② `/dev/shm/hft/prod` 已用量 = 4 KiB,标志删除,批处理恢复后 hft 各 slice 当前用量合计 ≤ 1,376 MiB;③ 标志保留、批处理不恢复、一条 P1;④ 标志删除、批处理恢复
- 证据:`ops-log/<UTC 日期>-CORE-212.md`
- 实现:`apps/hft-core/tests/host/CORE-212.sh`

### CORE-213 护栏演练 5:`system.slice` 外的换页触发 swap-in 信号

- 规则:[dev2 护栏](README.md#dev2-护栏)
- 级别:drill
- 里程碑:M3
- 环境:dev2,联调窗口外,`hft-verify` 运行中(`hftbatch.slice` CPU > 0.25 核)
- 授权:`[需授权]`
- 步骤:以 `systemd-run --scope -p Slice=user.slice stress-ng --vm 2 --vm-bytes <使主机换页的量>` 制造换页,保持 3 min(记录按 [incident-response P](../../docs/runbooks/incident-response.md#p-dev2-护栏触发))
- 通过标准:swap-in 速率首次 > 1,000 页/min 之后 120–125 s 内出现一级动作(`events.jsonl` 信号名为 swap-in);一级之前没有二级
- 证据:`ops-log/<UTC 日期>-CORE-213.md`
- 实现:`apps/hft-core/tests/host/CORE-213.sh`

### CORE-214 护栏演练 6:`tripped` 挡住定时器,`clear` 后恢复,主机重启后仍挡

- 规则:[dev2 护栏](README.md#dev2-护栏)
- 级别:drill
- 里程碑:M3
- 环境:dev2,接 CORE-210 或 CORE-213 的一级动作之后
- 授权:`[需授权]`(含重启主机)
- 步骤:① 等 15 min(其间 `hft-verify.timer` 至少到点一次);② 重启主机,开机后等 15 min;③ `hft-dev2-guard clear`,等下一次定时器到点(步骤记录按 [incident-response P](../../docs/runbooks/incident-response.md#p-dev2-护栏触发))
- 通过标准:①② 期间没有任何批处理单元进入 active,定时器到点时有 `exec-condition` 跳过记录;② 开机后 `tripped` 仍在;③ 之后批处理按定时器启动
- 证据:`ops-log/<UTC 日期>-CORE-214.md`
- 实现:`apps/hft-core/tests/host/CORE-214.sh`

### CORE-215 护栏演练 7:二级停 `sim` 后的自动重启与当日上限

- 规则:[dev2 护栏](README.md#dev2-护栏)
- 级别:drill
- 里程碑:M4
- 环境:dev2,联调窗口外
- 授权:`[需授权]`
- 步骤:按 CORE-210 的方法触发二级;压力撤除后等 35 min;当日再触发一次二级并撤除压力,等 35 min(记录按 [incident-response P](../../docs/runbooks/incident-response.md#p-dev2-护栏触发))
- 通过标准:第一次撤除后 30–31 min 内 `hft-core@sim` 被启动一次(`events.jsonl` 动作为自动重启);第二次之后 35 min 内没有启动;全程 `hft-core@prod`、批处理单元与定时器没有被护栏启动(`journalctl` 中启动请求的发起者均非 `hft-guard`)
- 证据:`ops-log/<UTC 日期>-CORE-215.md`
- 实现:`apps/hft-core/tests/host/CORE-215.sh`

### CORE-216 护栏演练 8:M3 标定——批处理满载 10 min 护栏不动作、旧系统 PSI 不升

- 规则:[dev2 护栏](README.md#dev2-护栏)
- 级别:drill
- 里程碑:M3
- 环境:dev2(旧系统的生产主机),`sim` 常驻
- 授权:`[需授权]`
- 步骤:按 deploy 的"M3 标定演练"与 [incident-response P](../../docs/runbooks/incident-response.md#p-dev2-护栏触发):① 读根 `cgroup.subtree_control`(含 `cpu`)与 `hftbatch.slice/cpu.idle`;② 记录旧系统 cgroup 两项 PSI 10 min 作基线;③ `hft-verify` 与 `hft-replay-verify` 满载 10 min
- 通过标准:① `cpu.idle = 1`;③ 期间 `events.jsonl` 无一级、二级;旧系统 cgroup 两项 `some avg10` 的 10 min 均值 ≤ 基线均值 + 1 个百分点,每个采样低于 5% / 20%;不通过 → 启用退路 (a)
- 证据:`ops-log/<UTC 日期>-CORE-216.md`
- 备注:"+ 1 个百分点"待标定;标定值只改 README 第 8 条,本用例随之更新
- 实现:`apps/hft-core/tests/host/CORE-216.sh`

### CORE-217 护栏演练 9:hft 空载时外部 CPU 争用只告警、不计入退路

- 规则:[dev2 护栏](README.md#dev2-护栏)
- 级别:drill
- 里程碑:M3
- 环境:dev2,只有 `sim` 常驻,无批处理运行
- 授权:`[需授权]`
- 步骤:以 `systemd-run --scope -p Slice=user.slice stress-ng --cpu 2` 运行 3 min,使旧系统 cgroup 的 `cpu.pressure` > 20% 超过 60 s(记录按 [incident-response P](../../docs/runbooks/incident-response.md#p-dev2-护栏触发))
- 通过标准:只有 P1(含"hft 用量不明显,未动作");无 `tripped`;无任何 `stop` 调用;`events.jsonl` 有该次触发且动作为"未动作";按 CORE-192 的口径统计的停机次数不变
- 证据:`ops-log/<UTC 日期>-CORE-217.md`
- 实现:`apps/hft-core/tests/host/CORE-217.sh`

### CORE-220 每步开始与结束各一条 `Note{StartupStep}`,`READY=1` 恰在步 5 结束之后

- 规则:[启动顺序](README.md#启动顺序)
- 级别:proc
- 里程碑:M3
- 前置:`dev2_prod()` 连假交易所;`NOTIFY_SOCKET` 指向测试套接字;`startup.budget_s = 30`、`startup.gate_timeout_s = 15`
- 步骤:
  1. 启动,等全部账户闸门通过
- 期望:
  - journal 中恰好 16 条 `Note{StartupStep}`,`(v[0], v[1])` 依次为 (1,0)、(1,1)、(2,0)、(2,1) … (8,0)、(8,1);`v[2]` 单调不减;开始记录的 `v[3] = 0`,结束记录的 `v[3] = 0`
  - 步 1–3 的 6 条在 journal 中位于步 4 开始记录之前,`v[2]` 为各自发生时的时刻(小于步 4 开始的 `v[2]`)
  - 测试套接字收到恰好一次 `READY=1`,在步 5 结束记录之后、步 6 开始记录之前;每步开始时 `STATUS=` 更新一次
- 实现:`apps/hft-core/tests/startup.rs::core_220_startup_step_notes`

### CORE-221 `StartupStep` 结束记录的 `v[3]` 标志

- 规则:[启动顺序](README.md#启动顺序)
- 级别:table
- 里程碑:M3
- 前置:`dev2_prod()` 连假交易所;各行设定参数与故障
- 实现:`apps/hft-core/tests/startup.rs::core_221_startup_step_flags`

| 行 | 参数与故障 | 期望 |
|---|---|---|
| CORE-221.1 | `startup.budget_s = 30`,无故障 | 步 7 结束记录 `v[3] = 0`;随后写出步 8 的两条 |
| CORE-221.2 | `startup.budget_s = 1`,假交易所对账户查询延迟 1.5 s 应答 | 步 7 结束记录 `v[3]` 的 bit 0 = 1 |
| CORE-221.3 | `startup.gate_timeout_s = 2`,reconciler 不运行 | 步 7 在开始后 2 s 结束,`v[3]` 的 bit 1 = 1;此时没有步 8 的记录 |
| CORE-221.4 | 同 .3,5 s 后启动 reconciler、闸门全过 | 闸门全过时写出步 8 的开始与结束两条 |
| CORE-221.5 | 删除 RAM 热段后启动(冷副本末段未封存) | 步 7 结束记录 `v[3]` 的 bit 2 = 1 |

### CORE-222 步 1 读入 Live 账户的交易 key 后立即删除秘密文件,`sim` 不读

- 规则:[启动顺序](README.md#启动顺序)、[实现说明](README.md#实现说明)
- 级别:proc
- 里程碑:M3
- 前置:`dev2_prod()` 连假交易所;`paths.secrets_dir` 下有 `bn-1.session`、`bn-1.rest`、`okx-1.key`
- 步骤:
  1. 启动,在步 2 开始记录写出时检查秘密目录
- 期望:
  - 三个文件都已不存在;`strace` 中每个文件恰好一次 `openat` 与一次 `unlink`,都在步 2 开始之前
  - 会话 key 的内存只交给 `conn-builder`,REST key 只交给 gw 与 `acct-fetch`(测试构建的交接记录)
- 实现:`apps/hft-core/tests/secrets.rs::core_222_unlink_after_read`

### CORE-223 步 2 失败退出时秘密文件也已删除

- 规则:[启动顺序](README.md#启动顺序)
- 级别:proc
- 里程碑:M3
- 前置:同 CORE-222,另把 `[[threads]]` 中 `gw-okx-swap` 去掉(步 2 报 `ThreadPlan{1}`)
- 步骤:
  1. 启动
- 期望:
  - 退出码 2;三个秘密文件都不存在
- 实现:`apps/hft-core/tests/secrets.rs::core_223_unlink_before_step2_exit`

### CORE-224 读入的 key 缓冲在交出后被清零

- 规则:[启动顺序](README.md#启动顺序)
- 级别:unit
- 里程碑:M3
- 前置:`secrets::read_and_unlink(dir, names)` 读入两把测试 key(各 64 B,字节全为 0xAB);测试分配器记录被释放块的内容
- 步骤:
  1. 把 key 交给模拟的 conn-builder 与 gw 后丢弃 `secrets` 模块自己的缓冲
- 期望:
  - 被释放的两个缓冲在释放前每个字节都是 0
- 实现:`apps/hft-core/tests/secrets.rs::core_224_key_buffers_zeroized`

### CORE-225 步 2 的 refdata 与跨 blob 错误以 `LoadErrCode` 名报出并退出 2

- 规则:[启动顺序](README.md#启动顺序)、[命令行与退出码](README.md#命令行与退出码)
- 级别:table
- 里程碑:M3
- 前置:`config::load_and_validate`;各行从所列构造器改 refdata 或参数 blob(SHA-256 同步更新)
- 实现:`apps/hft-core/tests/startup.rs::core_225_refdata_errors_by_name`

| 行 | 改动 | 期望错误列表 |
|---|---|---|
| CORE-225.1 | `dev2_prod()` 的 `bn-1` 的 `venues` 同时含 `binance-usdm` 与 `binance-spot` | 含 `SameExchangeMarkets` |
| CORE-225.2 | `dev2_sim()` 的 `sim-a` 的 `venues` 同时含 `okx-swap` 与 `okx-spot` | 含 `SameExchangeMarkets` |
| CORE-225.3 | `dev2_prod()` 的 `RiskParams` 引用不存在的账户 9 | 含 `DanglingRef` |
| CORE-225.4 | `dev2_prod()` 的 `StrategyParams` 某 group 腿引用不存在的 inst | 含 `LegNotTradable`,详情含该 inst |
| CORE-225.5 | `c32_prod()`(`order_env = "prod"`),两个账户在 `binance-usdm` 登记同一 `venue_uid` | 含 `DuplicateVenueUid` |
| CORE-225.6 | `c32_prod()`(`order_env = "testnet"`),同上(两个账户都在同一 `VenueId` `binance-usdm`) | 含 `DuplicateVenueUid`(同一 `VenueId` 上重复,非正式账户也是加载错误) |
| CORE-225.7 | `dev2_prod()` 加 `[venues.gate-futures]`(默认构建未编进 gate 协议) | 含 `UnknownVenue` |
| CORE-225.8 | `c32_prod()` 的 EngineConfig `host_class = c16`(5) | 含 `HostClassMismatch` |
| CORE-225.9 | `c32_prod()`(`order_env = "testnet"`),`bn-1` 在 `binance-usdm`、`bn-2` 在 `binance-spot` 登记同一 `venue_uid` | 不含 `DuplicateVenueUid`;启动继续,告警按 CORE-410 |

### CORE-226 Sim 账户的 `SimAccountCfg` 经 `SimExchange::new` 干校验

- 规则:[启动顺序](README.md#启动顺序)
- 级别:table
- 里程碑:M4
- 前置:`dev2_sim()`(`[[sim.latency_models]]` 1 项、`[[sim.margin_models]]` 1 项);`config::load_and_validate`
- 实现:`apps/hft-core/tests/startup.rs::core_226_sim_dry_check`

| 行 | 改动 | 期望错误列表 |
|---|---|---|
| CORE-226.1 | 不改 | 空 |
| CORE-226.2 | `sim-a` 的 `latency_model = 1` | 恰好 {`SimConfig`},详情含 `LatencyModelIndex` |
| CORE-226.3 | `sim-a` 的 `margin_model = 1` | 恰好 {`SimConfig`},详情含 `MarginModelIndex` |
| CORE-226.4 | `sim-a` 的 `fee_tier` 在 refdata `Fees` 中没有对应行 | 恰好 {`SimConfig`},详情含 `FeeTier` |
| CORE-226.5 | 加第二项 `[[sim.latency_models]]`,`sim-a` 的 `latency_model = 1` | 空 |

### CORE-227 EngineConfig 段 4 的环容量行校验

- 规则:[配置文件字段](README.md#配置文件字段)
- 级别:table
- 里程碑:M3
- 前置:`c32_prod()` 的 EngineConfig 段 4(按环容量表的 14 行:码 1–11、15–17);`config::load_and_validate`
- 实现:`apps/hft-core/tests/rings.rs::core_227_ring_cap_rows`

| 行 | 段 4 改动 | 期望错误列表 |
|---|---|---|
| CORE-227.1 | 不改 | 空 |
| CORE-227.2 | 删除 `Bbo`(4)行 | 恰好 {`EngineMismatch`} |
| CORE-227.3 | `Md`(5)行出现两次 | 恰好 {`EngineMismatch`} |
| CORE-227.4 | 加一行 `kind = 18` | 恰好 {`EngineMismatch`} |
| CORE-227.5 | 加一行 `kind = 0` | 恰好 {`EngineMismatch`} |
| CORE-227.6 | `Rep`(7,元素)`cap = 16000` | 恰好 {`EngineMismatch`} |
| CORE-227.7 | `Rep` `cap = 16384` | 空 |
| CORE-227.8 | `Ctl`(1,字节)`cap = 262000` | 空(字节环不要求 2 的幂) |
| CORE-227.9 | 加一行 `RepSim`(12) | 恰好 {`EngineMismatch`} |
| CORE-227.10 | 把 `c32_sim()` 的 EngineConfig(含 `RepSim`、`OutSim`、`RepSimBulk`,无 `Rep`/`Out`)配给 `c32_prod()` | 含 `EngineMismatch` |

### CORE-228 `RingCapRow` 的布局与 `RingCapKind` 码值

- 规则:[配置文件字段](README.md#配置文件字段)
- 级别:table
- 里程碑:M3
- 前置:编译期断言;`RingCapKind::from_u8`
- 实现:`apps/hft-core/tests/rings.rs::core_228_ring_cap_row_layout`

| 行 | 断言 | 期望 |
|---|---|---|
| CORE-228.1 | `size_of::<RingCapRow>()` / `align_of` | 8 / 4 |
| CORE-228.2 | `kind` / `_pad` / `cap` 偏移 | 0 / 1 / 4 |
| CORE-228.3 | 码 1–17 | 依次 `Ctl`、`Hk`、`FeedCtl`、`Bbo`、`Md`、`MdBulk`、`Rep`、`Out`、`RepBulk`、`AcctReq`、`Acct`、`RepSim`、`OutSim`、`RepSimBulk`、`Advice`、`SnapReq`、`Snap` |
| CORE-228.4 | 码 0、18、255 | `None` |
| CORE-228.5 | 单位:元素环(`Hk`、`FeedCtl`、`Bbo`、`Md`、`Rep`、`Out`、`AcctReq`、`RepSim`、`OutSim`、`SnapReq`) | `Unit::Elements`;其余 `Unit::Bytes` |

### CORE-229 环容量各列按环容量表生成

- 规则:[配置文件字段](README.md#配置文件字段)
- 级别:table
- 里程碑:M3
- 前置:`tools/hft-cfg` 按主机类与实例生成 EngineConfig;读段 4
- 实现:`tools/hft-cfg/tests/capacity.rs::core_229_ring_cap_columns`

| 行 | 列 | 期望(`Bbo` / `Md` / `MdBulk` / `Rep` / `Out` / `RepBulk` / `AcctReq` / `Acct` / `RepSim` / `OutSim` / `RepSimBulk` / `SnapReq` / `Snap`;`Ctl` 256 KiB、`Hk` 256、`Advice` 256 KiB 各列相同) |
|---|---|---|
| CORE-229.1 | 升级主机类 `prod` | 65536 / 65536 / 4 MiB / 16384 / 4096 / 1 MiB / 64 / 2 MiB / 无 / 无 / 无 / 64 / 4 MiB |
| CORE-229.2 | `c16`/`c32` `sim` | 65536 / 65536 / 4 MiB / 无 / 无 / 无 / 无 / 无 / 16384 / 4096 / 1 MiB / 64 / 4 MiB |
| CORE-229.3 | `dev2` `prod` | 8192 / 8192 / 1 MiB / 4096 / 4096 / 256 KiB / 16 / 2 MiB / 无 / 无 / 无 / 16 / 1 MiB |
| CORE-229.4 | `dev2` `sim` | 8192 / 8192 / 1 MiB / 无 / 无 / 无 / 无 / 无 / 4096 / 1024 / 256 KiB / 16 / 1 MiB |

### CORE-230 `FeedCtl` 容量 = max(256, 2 × 最大 `max_sub_insts`) 向上取 2 的幂

- 规则:[配置文件字段](README.md#配置文件字段)
- 级别:table
- 里程碑:M3
- 前置:`hft_cfg::feedctl_cap(max_sub_insts_max)` 纯函数
- 实现:`tools/hft-cfg/tests/capacity.rs::core_230_feedctl_capacity`

| 行 | 各 venue `max_sub_insts` 的最大值 | 期望 |
|---|---|---|
| CORE-230.1 | 16 | 256 |
| CORE-230.2 | 128 | 256 |
| CORE-230.3 | 129 | 512 |
| CORE-230.4 | 256 | 512 |
| CORE-230.5 | 257 | 1024 |

### CORE-231 步 3 在普通页上映射并预触摸引擎区,`mlockall` 锁定

- 规则:[启动顺序](README.md#启动顺序)
- 级别:proc
- 里程碑:M3
- 前置:`dev2_sim()`;步 3 结束后读 `/proc/<pid>/smaps` 与 `/proc/<pid>/status`
- 步骤:
  1. 启动到 `Running`,再等 10 s
- 期望:
  - 引擎区(环、簿、OMS 槽、快照暂存)没有 `hugetlb` 映射;`VmLck` ≥ 引擎区大小
  - 步 3 结束之后到第 10 s,`seq` 与 `feed` 线程的 `minflt` 不增长
- 实现:`apps/hft-core/tests/startup.rs::core_231_engine_area_locked`

### CORE-232 每个 gw 一个 `KillWord`,交给 `LiveDriver`

- 规则:[启动顺序](README.md#启动顺序)、[优雅退出与 kill 语义](README.md#优雅退出与-kill-语义)
- 级别:unit
- 里程碑:M3
- 前置:`c32_prod()`(6 个 `gw:`);`startup::allocate(&cfg)`
- 步骤:
  1. 取分配结果
- 期望:
  - `kill_words.len() = 6`;每个元素地址按 128 B 对齐、相邻元素地址差 128;初值 0;`LiveDriver` 持有的切片与 gw 线程拿到的是同一地址
- 实现:`apps/hft-core/tests/startup.rs::core_232_kill_word_per_gw`

### CORE-233 升级主机类的 `prod` 打开 `/dev/cpu_dma_latency` 写 0 并持有到退出

- 规则:[启动顺序](README.md#启动顺序)
- 级别:host
- 里程碑:M7
- 环境:统一升级主机(`c32`)
- 授权:`[需授权]`
- 步骤:
  1. `systemctl start hft-core@prod`,`Running` 后 `ls -l /proc/<pid>/fd` 并读 `/dev/cpu_dma_latency` 的当前请求值
  2. `systemctl stop hft-core@prod`
- 通过标准:步骤 1 有一个 fd 指向 `/dev/cpu_dma_latency`,其写入值为 4 字节 0;步骤 2 退出后不再有进程持有该设备
- 证据:`ops-log/<UTC 日期>-CORE-233.md`
- 实现:`apps/hft-core/tests/host/CORE-233.sh`

### CORE-234 dev2 的两个实例与 `sim` 实例从不打开 `/dev/cpu_dma_latency`

- 规则:[启动顺序](README.md#启动顺序)、[`dev2`:现有 `c7g.large`](README.md#dev2现有-c7glarge2-vcpu38-gb与旧系统同机)
- 级别:host
- 里程碑:M3
- 环境:dev2;`hft-core@sim` 与联调窗口内的 `hft-core@prod`
- 授权:`[需授权]`
- 步骤:
  1. 以 `strace -f -e trace=openat` 包装的 drop-in 启动两个实例到 `Running`
- 通过标准:两份 strace 中都没有 `/dev/cpu_dma_latency`
- 证据:`ops-log/<UTC 日期>-CORE-234.md`
- 实现:`apps/hft-core/tests/host/CORE-234.sh`

### CORE-235 步 4 截掉半个事件时写 `Note{PartialEventDropped}` 并记 `InputHash`

- 规则:[启动顺序](README.md#启动顺序)、[崩溃隔离与从快照恢复](README.md#崩溃隔离与从快照恢复)
- 级别:proc
- 里程碑:M3
- 前置:`dev2_sim()`;测试钩子在某条 Input(seq = S,kind = K)的输出写到一半时 SIGKILL
- 步骤:
  1. 重启,等步 4 结束
- 期望:
  - 一条 `Note{PartialEventDropped}`,字段 `seq = S`、`kind = K`;journal 中 seq ≥ S 的旧记录不再可见
  - `DurableMeta` 的 `poison_input = {hash = xxh3_64(K ‖ body), count = 1}`
- 实现:`apps/hft-core/tests/recovery.rs::core_235_partial_event_dropped`

### CORE-236 `tail_lost` 的判定与 `Note{JournalTailLost}`

- 规则:[启动顺序](README.md#启动顺序)
- 级别:table
- 里程碑:M3
- 前置:`dev2_sim()`;按行准备热段与冷副本;启动到步 5 结束
- 实现:`apps/hft-core/tests/recovery.rs::core_236_tail_lost`

| 行 | 状态 | 期望 |
|---|---|---|
| CORE-236.1 | RAM 热段存在(SIGKILL 之后) | `tail_lost = false`;无 `Note{JournalTailLost}` |
| CORE-236.2 | 热段不存在;冷副本末段有 `.seal`,末条输入之后是 Drain 快照 | `tail_lost = false`;无 `JournalTailLost` |
| CORE-236.3 | 热段不存在;冷副本末段有 `.seal`,以段滚动的 `SegmentRoll{End}` 结尾(无 Drain 快照) | `tail_lost = true`;一条 `Note{JournalTailLost}` |
| CORE-236.4 | 热段不存在;冷副本末段无 `.seal`(persist 中途) | `tail_lost = true`;一条 `JournalTailLost`;全部非终态槽 `Unknown` |

### CORE-237 冷副本段链断裂时以 `Journal` 退出 4

- 规则:[启动顺序](README.md#启动顺序)
- 级别:proc
- 里程碑:M3
- 前置:`dev2_sim()` 停机后删掉 RAM 热段,并把冷副本中间一段的段头 `prev_header_sha256` 翻转 1 位
- 步骤:
  1. 启动
- 期望:
  - 退出码 4;错误行 `Journal`;没有写出新段
- 实现:`apps/hft-core/tests/recovery.rs::core_237_cold_chain_broken_exit4`

### CORE-238 版本变化:末尾是 Drain 快照时起步,否则 `VersionNeedsDrain` 退出 4

- 规则:[启动顺序](README.md#启动顺序)
- 级别:table
- 里程碑:M3
- 前置:旧版本二进制运行 `dev2_sim()` 后按行停机,再用新版本二进制(schema 版本表不同)启动
- 实现:`apps/hft-core/tests/recovery.rs::core_238_version_change`

| 行 | 旧版本的停机方式 | 期望 |
|---|---|---|
| CORE-238.1 | SIGTERM(Drain 快照结尾) | 启动继续到步 6,`RecoverPlan.start = DrainTail`,`replay_tail` 无尾部可放 |
| CORE-238.2 | SIGKILL | 退出码 4,错误行 `VersionNeedsDrain` |

### CORE-239 快照不可解时以 `SnapshotUndecodable` 退出 4

- 规则:[启动顺序](README.md#启动顺序)
- 级别:proc
- 里程碑:M3
- 前置:`dev2_sim()` 的最新完整快照节 5 的 `desc_version` 改为策略 `restore` 不接受的值(段 CRC 同步重算)
- 步骤:
  1. 启动
- 期望:
  - 退出码 4;错误行 `SnapshotUndecodable`,详情含节号 5
- 实现:`apps/hft-core/tests/recovery.rs::core_239_snapshot_undecodable_exit4`

### CORE-240 恢复期决策不一致只告警,不中止启动

- 规则:[启动顺序](README.md#启动顺序)
- 级别:integ
- 里程碑:M3
- 前置:`dev2_prod()` 连假交易所;在尾部一条 `OrderDecision` 的记录体中改一个字节(CRC 同步,模拟重算输出不同)
- 步骤:
  1. 重启
- 期望:
  - 一条 `Note{RecoverMismatch}`,`v[0]` 为该记录的 `venue | account << 8`;进程继续到步 6(有步 6 开始记录),不以 4 退出
- 实现:`apps/hft-core/tests/recovery.rs::core_240_recover_mismatch_continues`

### CORE-241 epoch = max(快照, `DurableMeta`) + 1

- 规则:[启动顺序](README.md#启动顺序)、[崩溃隔离与从快照恢复](README.md#崩溃隔离与从快照恢复)
- 级别:table
- 里程碑:M3
- 前置:`recovery::next_epoch(snapshot_epoch, durable_epoch)` 纯函数(缺失记为 0)
- 实现:`apps/hft-core/tests/recovery.rs::core_241_next_epoch`

| 行 | 快照 / `DurableMeta` | 期望 |
|---|---|---|
| CORE-241.1 | 5 / 5 | 6 |
| CORE-241.2 | 5 / 9 | 10 |
| CORE-241.3 | 9 / 5 | 10 |
| CORE-241.4 | 缺失 / 缺失(空 journal) | 1 |
| CORE-241.5 | 3 / 7(`--recover-from-snapshot` 选中较旧快照) | 8 |

### CORE-242 epoch 在任何订单发出之前 `write_sync`;落盘后崩溃则下次再加 1

- 规则:[启动顺序](README.md#启动顺序)
- 级别:proc
- 里程碑:M3
- 前置:`dev2_prod()` 连假交易所,上次运行的 epoch 为 4,有一张 `Unknown` 待查的挂单
- 步骤:
  1. 启动;记录 `DurableMeta` 的 `engine_epoch` 写入时刻与假交易所收到第一帧下单请求或查单请求的时刻
  2. 另一次:在 `write_sync` 返回之后、`EngineClock` 写出之前注入 abort,再启动
- 期望:
  - 步骤 1:`engine_epoch = 5` 的 `write_sync` 早于假交易所收到的任何下单类帧;`Note{EpochStart}` 的 `v[0] = 5`
  - 步骤 2:被中断的一次把 `engine_epoch` 写成 5;再次启动后 `EpochStart` 的 `v[0] = 6`
- 实现:`apps/hft-core/tests/recovery.rs::core_242_epoch_synced_before_orders`

### CORE-243 `EngineClock` 恰好一条,先于其他 Input;`HostProfile` 与 `ConfigBlob{HostManifest}` 在它之前

- 规则:[启动顺序](README.md#启动顺序)、[HostProfile](README.md#hostprofile)
- 级别:prop
- 里程碑:M3
- 性质:任意时刻 SIGKILL 后重启,本次运行的新段中:`HostProfile` 与 `ConfigBlob{HostManifest}` 各一条,位于第一条 Input 之前;第一条 Input 是 `EngineClock`,本次运行没有第二条;处理它的同一事件内有 `Note{EpochStart}`
- 生成:`dev2_prod()` 连假交易所满速行情与随机下单;SIGKILL 时刻在 [0, 10 s) 均匀抽取
- 参考:读 journal 逐条断言
- 例数:PR 5;每夜 100
- 实现:`apps/hft-core/tests/recovery.rs::core_243_single_engine_clock_first`

### CORE-244 随机时刻 SIGKILL × 100 后重启的状态哈希与 `replayer verify` 一致

- 规则:[启动顺序](README.md#启动顺序)、[崩溃隔离与从快照恢复](README.md#崩溃隔离与从快照恢复)
- 级别:proc
- 里程碑:M3
- 前置:`dev2_prod()` 连假交易所,满速行情与随机下单
- 步骤:
  1. 循环 100 次:运行 [1 s, 60 s) 内随机时长后 SIGKILL,重启到步 5 结束,记录 `Recovery` 快照的状态哈希
  2. 对整段 journal 运行 `replayer verify`
- 期望:
  - 每次记录的哈希等于 `replayer verify` 在同一位置算出的哈希;`replayer verify` 退出 0
- 备注:每夜运行(100 次循环约 1 h)
- 实现:`apps/hft-core/tests/recovery.rs::core_244_sigkill_x100_hash_matches`

### CORE-245 `config_source` 决定恢复时生效的 refdata 与参数

- 规则:[启动顺序](README.md#启动顺序)、[配置文件字段](README.md#配置文件字段)
- 级别:table
- 里程碑:M3
- 前置:`dev2_sim()` 运行中经 `SetParams` 把 `s1` 的参数换成版本 B(journal 中最新 `ConfigBlob` 为 B);配置文件的 `blobs.strategy_params` 仍是版本 A;干净 Drain 后按行重启
- 实现:`apps/hft-core/tests/recovery.rs::core_245_config_source`

| 行 | `instance.config_source` | 期望 |
|---|---|---|
| CORE-245.1 | `"journal"` | 生效参数为 B;新段引导块重述的 `ConfigBlob` 为 B |
| CORE-245.2 | `"file"` | 生效参数为 A;本次运行写出 `ConfigBlob` A |
| CORE-245.3 | 不写 | `MissingField`(`instance.config_source`),退出 2 |

### CORE-246 `sim` 实例在步 5 把 `[sim]` 全表写成 `ConfigBlob`,`prod` 不写

- 规则:[配置文件字段](README.md#配置文件字段)
- 级别:table
- 里程碑:M4
- 前置:启动到步 5 结束,读新段
- 实现:`apps/hft-core/tests/recovery.rs::core_246_sim_config_blob`

| 行 | 配置 | 期望 |
|---|---|---|
| CORE-246.1 | `dev2_sim()` | 恰好一条 `ConfigBlob{SimConfig}`(`BlobKind` 6,与 `ConfigBlob{HostManifest}` 同一步写出),内容的规范编码与配置的 `SimTuning`、两张模型表一致;段滚动后新段引导块再次重述 |
| CORE-246.2 | `dev2_prod()` | 没有 `ConfigBlob{SimConfig}` |

### CORE-247 步 6 的连接:每 venue BBO 赛马与一条 md,每 (venue, Live 账户) 主 + 热备

- 规则:[启动顺序](README.md#启动顺序)、[多账户](README.md#多账户)
- 级别:integ
- 里程碑:M3
- 前置:`dev2_prod()` 连假交易所;每 venue `feed.bbo_conns = 2`;`okx-swap` 有两个 Live 账户 `okx-1`、`okx-2`
- 步骤:
  1. 启动到步 6 结束
- 期望:
  - 假交易所对每 venue 看到 2 条 BBO 连接与 1 条 md 连接;对 `binance-usdm` 的 `bn-1` 看到 2 条已登录的下单会话,对 `okx-swap` 的 `okx-1`、`okx-2` 各 2 条
  - `gw-okx-swap` 一个线程轮询这 4 条下单会话(其余线程没有这些 fd)
  - 某 inst 在收到首帧 BBO(或 BBO 补帧)之前不进入可用状态
- 实现:`apps/hft-core/tests/connect.rs::core_247_connections_per_venue_account`

### CORE-248 会话 key 与 REST key 的 UID 都并入 `AccountConfigReport`

- 规则:[启动顺序](README.md#启动顺序)
- 级别:integ
- 里程碑:M3
- 前置:`dev2_prod()` 连假交易所;`bn-1` 的会话 key 与 REST key 是两把,假交易所对两把都返回 UID `U1`(= refdata `venue_uid`)
- 步骤:
  1. 启动到步 6 结束
- 期望:
  - `bn-1` 在 `binance-usdm` 的 `AccountConfigReport.uid_keys` 覆盖两把 key,`UID` 位为 0;该报告经 `rep-binance-usdm` 进入 journal(Input),其前有 acct-fetch 的原始响应 raw 记录
- 实现:`apps/hft-core/tests/connect.rs::core_248_uid_from_both_keys`

### CORE-249 预算内连不齐的 venue 判 Down,不阻塞 `optional` venue 之外的闸门

- 规则:[启动顺序](README.md#启动顺序)、[配置文件字段](README.md#配置文件字段)
- 级别:table
- 里程碑:M3
- 前置:`dev2_prod()` 连假交易所;假交易所拒绝 `okx-spot` 的全部连接;`okx-1` 只交易 `okx-swap`;`startup.gate_timeout_s = 15`
- 实现:`apps/hft-core/tests/startup.rs::core_249_venue_down`

| 行 | `venues.okx-spot.optional` | 期望 |
|---|---|---|
| CORE-249.1 | true | 步 6 在开始后 8 s 内结束,`okx-spot` 连接级 `ConnState{Down}`;`bn-1`、`okx-1` 闸门通过 |
| CORE-249.2 | false | `okx-spot` `Down`;用到 `okx-spot` 行情的账户 `FEEDS` 为 0;其余 venue 照常 Up |

### CORE-250 sim-gw 为每个 Sim 账户发 `ConnState{Up}` 与 `AccountConfigReport`

- 规则:[启动顺序](README.md#启动顺序)
- 级别:integ
- 里程碑:M4
- 前置:`dev2_sim()`(两个 Sim 账户)
- 步骤:
  1. 启动到步 7 结束
- 期望:
  - `sim-a`、`sim-b` 各一条 sim-gw 的 `ConnState{Up}` 与一条 `AccountConfigReport`;两个账户的 `ORDER_CONNS` 置位
- 实现:`apps/hft-core/tests/connect.rs::core_250_sim_gw_conn_up`

### CORE-251 身份核验:REST key 指向另一子账户时放行前 Drain 后退出 2

- 规则:[启动顺序](README.md#启动顺序)
- 级别:integ
- 里程碑:M3
- 前置:`dev2_prod()` 连假交易所,`shutdown.cancel_open_orders = true`;`okx-1` 的 REST key 在假交易所对应 UID `U9`(≠ refdata 的 `U2`);`okx-1` 有一张上次运行留下的挂单
- 步骤:
  1. 启动
- 期望:
  - `AccountConfigReport` 的 `UID` 位置位且无 `UNREADABLE`;一条 `Note{IdentityMismatch}`,`v[0]` = `okx-swap | okx-1 << 8`;随后 Drain(假交易所收到该挂单的撤单)
  - 进程以退出码 2 结束,stderr 有 `error[IdentityMismatch]`;`okx-1` 的 `IDENTITY` 位全程为 0
- 实现:`apps/hft-core/tests/identity.rs::core_251_rest_key_mismatch_before_release`

### CORE-252 身份核验:放行后会话重建时发现不符同样 Drain 后退出 2

- 规则:[启动顺序](README.md#启动顺序)
- 级别:integ
- 里程碑:M3
- 前置:同 CORE-251,但 UID 一开始相符,进程到 `Running`;之后假交易所断开 `okx-1` 的下单连接,重连时会话 key 对应 UID `U9`
- 步骤:
  1. 等会话重建
- 期望:
  - 一条 `Note{IdentityMismatch}`;Drain 撤单;退出码 2
- 实现:`apps/hft-core/tests/identity.rs::core_252_mismatch_after_release`

### CORE-253 只换会话 key 同样判不符

- 规则:[启动顺序](README.md#启动顺序)
- 级别:integ
- 里程碑:M3
- 前置:同 CORE-251,但 REST key 的 UID 相符,会话 key 的 UID 为 `U9`
- 步骤:
  1. 启动
- 期望:
  - `Note{IdentityMismatch}`;退出码 2
- 实现:`apps/hft-core/tests/identity.rs::core_253_session_key_mismatch`

### CORE-254 UID 查询超时或读不到时闸门保持关闭,不退出

- 规则:[启动顺序](README.md#启动顺序)
- 级别:table
- 里程碑:M3
- 前置:`dev2_prod()` 连假交易所;`startup.gate_timeout_s = 5`;按行设置假交易所对 `okx-1` 的 UID 查询行为;观察 60 s
- 实现:`apps/hft-core/tests/identity.rs::core_254_uid_unreadable`

| 行 | 假交易所行为 | 期望 |
|---|---|---|
| CORE-254.1 | UID 查询不应答(超时) | 进程存活 60 s;`okx-1` 的 `IDENTITY` 为 0;没有 `Note{IdentityMismatch}`,没有 Drain |
| CORE-254.2 | 应答中没有 UID 字段(报告带 `UNREADABLE`) | 同上 |

### CORE-255 测试网账户 key 指向另一子账户时退出 2(真实测试网)

- 规则:[启动顺序](README.md#启动顺序)
- 级别:testnet
- 里程碑:M6
- 环境:dev2 联调窗口;`prod` 测试网实例;两个属于不同测试网子账户的 key
- 授权:`[需授权]`(换测试网 key)
- 步骤:
  1. 把 `okx-1` 的 REST key 换成另一子账户的 key,`systemctl start hft-core@prod`
  2. 恢复后,只换会话 key,重复
- 通过标准:两次都有 `Note{IdentityMismatch}`、Drain 后 `ExecMainStatus=2`、`NRestarts` 不增加
- 证据:`ops-log/<UTC 日期>-CORE-255.md`
- 实现:`apps/hft-core/tests/host/CORE-255.sh`

### CORE-256 闸门输入:`FEEDS` 与 `ORDER_CONNS`

- 规则:[启动顺序](README.md#启动顺序)
- 级别:table
- 里程碑:M3
- 前置:`dev2_prod()` 连假交易所;`startup.gate_timeout_s = 15`;按行设置;读 `Note{StartupGate}` 的位
- 实现:`apps/hft-core/tests/gates.rs::core_256_feeds_and_order_conns`

| 行 | 设置 | 期望 |
|---|---|---|
| CORE-256.1 | 无故障,`allow_missing_standby = false` | `bn-1` 的 `FEEDS`、`ORDER_CONNS` 都置位 |
| CORE-256.2 | `bn-1` 的热备登录被拒,`allow_missing_standby = false`(段 5 同值) | `ORDER_CONNS` 为 0 |
| CORE-256.3 | 同上,`allow_missing_standby = true`(段 5 同值) | `ORDER_CONNS` 置位 |
| CORE-256.4 | `bn-1` 订阅的一个 inst 一直没有 BBO 首帧,BBO 补帧也失败 | `FEEDS` 为 0 |
| CORE-256.5 | `bn-1` 订阅一个 TopN 深度 inst,深度首个快照未到 | `FEEDS` 为 0;快照到后置位 |

### CORE-257 闸门输入:`RECONCILED`

- 规则:[启动顺序](README.md#启动顺序)
- 级别:table
- 里程碑:M5
- 前置:`dev2_prod()` 连假交易所与 reconciler;`startup.gate_timeout_s = 15`
- 实现:`apps/hft-core/tests/gates.rs::core_257_reconciled`

| 行 | 对账结果 | 期望 |
|---|---|---|
| CORE-257.1 | reconciler 不运行 | 15 s 后步 7 结束,`v[3]` bit 1 = 1;`RECONCILED` 为 0 |
| CORE-257.2 | `EpochStart` 之后全部 `Ignore` | `RECONCILED` 置位 |
| CORE-257.3 | 一项余额差异 `Warn`,未处置 | `RECONCILED` 为 0 |
| CORE-257.4 | 同 .3,随后 `AdoptExchange` | 下一份报告后置位 |
| CORE-257.5 | 只有资金费差额 | `RECONCILED` 置位 |
| CORE-257.6 | `EpochStart` 之前的一份报告全部 `Ignore`,之后无报告 | `RECONCILED` 为 0 |

### CORE-258 闸门输入:`CLOCK` 的偏移边界

- 规则:[启动顺序](README.md#启动顺序)、[配置文件字段](README.md#配置文件字段)
- 级别:table
- 里程碑:M3
- 前置:`startup.max_clock_offset_us = 500`;housekeeping 的 `ClockStatus` 由测试时钟源注入
- 实现:`apps/hft-core/tests/gates.rs::core_258_clock_gate`

| 行 | `ClockStatus` | 期望 `CLOCK` |
|---|---|---|
| CORE-258.1 | `Synced`,offset 500 µs | 置位 |
| CORE-258.2 | `Synced`,offset 501 µs | 0 |
| CORE-258.3 | `Synced`,offset 499 µs | 置位 |
| CORE-258.4 | 非 `Synced`,offset 0 | 0 |

### CORE-259 闸门输入:`ACCOUNT_CFG`

- 规则:[启动顺序](README.md#启动顺序)
- 级别:table
- 里程碑:M3
- 前置:`dev2_prod()` 连假交易所;假交易所的账户配置按行
- 实现:`apps/hft-core/tests/gates.rs::core_259_account_cfg`

| 行 | 账户配置 | 期望 |
|---|---|---|
| CORE-259.1 | 单向持仓、全仓、非多资产、现货借币关闭 | `ACCOUNT_CFG` 置位 |
| CORE-259.2 | 双向持仓 | 0 |
| CORE-259.3 | 逐仓 | 0 |
| CORE-259.4 | 现货借币开启(现货 venue) | 0 |
| CORE-259.5 | 放行后经用户流推送改为双向持仓 | hft-risk 设 `AccountCfg` cap(`Note` 由 hft-risk 写);`HeartbeatBlock.state` 仍为 `Running` |

### CORE-260 闸门输入:`IDENTITY` 需要两把 key 的 UID 与 kill 自检

- 规则:[启动顺序](README.md#启动顺序)
- 级别:table
- 里程碑:M3
- 前置:`dev2_prod()` 连假交易所
- 实现:`apps/hft-core/tests/gates.rs::core_260_identity_inputs`

| 行 | 条件 | 期望 `IDENTITY` |
|---|---|---|
| CORE-260.1 | 两把 key 的 UID 相符,kill 自检通过 | 置位 |
| CORE-260.2 | 只核对了 REST key(会话 key 的 UID 未随交接带回) | 0 |
| CORE-260.3 | UID 相符,但该账户的 kill 自检失败(没有 `Timer{Sys, KillSelfTestOk}`) | 0 |
| CORE-260.4 | `uid_policy = "unverifiable"` 的测试网账户,报告带 `UNREADABLE`,kill 自检通过,一次签名只读请求成功 | 置位;一条 `Note{IdentityUnverified}` |

### CORE-261 闸门未放行时升级命令被拒,带确认的 `Kill{flatten}` 被接受;放行前 Live cell 为 `CloseOnly`

- 规则:[启动顺序](README.md#启动顺序)
- 级别:integ
- 里程碑:M3
- 前置:`dev2_prod()` 连假交易所;reconciler 不运行(闸门不放行);上次运行时 `bn-1` 的 cell `g1` 期望模式为 `Live`
- 步骤:
  1. 发 `Resume{Recovery, account = bn-1}`(Trader)
  2. 发 `Kill{Account(bn-1), flatten}` 并确认
  3. 读 `g1` 的有效模式
- 期望:
  - 步骤 1:`CmdResult` 为 `PreconditionFailed`
  - 步骤 2:`CmdResult` 为 `Applied`
  - 步骤 3:有效模式为 `CloseOnly`,期望模式仍为 `Live`
- 实现:`apps/hft-core/tests/gates.rs::core_261_escalation_blocked_before_release`

### CORE-262 干净 Drain 重启后闸门通过即自动解除 `Recovery`

- 规则:[启动顺序](README.md#启动顺序)
- 级别:proc
- 里程碑:M5
- 前置:`dev2_prod()` 连假交易所与 reconciler;EngineConfig `auto_resume_after_clean_drain = true`;无挂单、无 `Unknown`、无 `Broken`
- 步骤:
  1. SIGTERM(退出 0),同版本重启
- 期望:
  - 每个账户的 `Note{StartupGate}` `released = 1` 的同一事件内,`Recovery` cap 被删除(hft-risk 的 `CapCleared` 注记,auto);无需 `Resume{Recovery}`
- 实现:`apps/hft-core/tests/gates.rs::core_262_clean_drain_auto_resume`

### CORE-263 非干净重启的 `prod` 实例不自动解除 `Recovery`

- 规则:[启动顺序](README.md#启动顺序)
- 级别:table
- 里程碑:M5
- 前置:`dev2_prod()`(测试网账户)连假交易所与 reconciler,`auto_resume_after_clean_drain = true`;按行制造重启原因
- 实现:`apps/hft-core/tests/startup.rs::core_263_no_auto_resume_prod`

| 行 | 重启原因 | 期望 |
|---|---|---|
| CORE-263.1 | SIGKILL | 闸门放行后 `Recovery` cap 仍在;`Resume{Recovery}`(Trader)后删除 |
| CORE-263.2 | 干净 Drain 但删除 RAM 热段且冷副本末段未封存(`tail_lost`) | 仍在 |
| CORE-263.3 | 干净 Drain,同版本,但 `--recover-from-snapshot latest` 启动 | 仍在 |

### CORE-264 只含 Sim 账户的实例非干净重启后的 `Recovery` 解除

- 规则:[启动顺序](README.md#启动顺序)
- 级别:table
- 里程碑:M4
- 前置:`dev2_sim()`,`auto_resume_after_clean_drain = true`;按行制造重启原因
- 实现:`apps/hft-core/tests/startup.rs::core_264_sim_instance_auto_resume`

| 行 | 重启原因 | 期望 |
|---|---|---|
| CORE-264.1 | SIGKILL(尾部非空) | 闸门放行时 `Recovery` 自动删除;无 `Note{RecoveryHeldSim}` |
| CORE-264.2 | SIGKILL,尾部重放注入 `RecoverMismatch` | 不删除;放行的同一事件内一条 `Note{RecoveryHeldSim}`,`v[0]` = 账户、`v[1]` 含 `RECOVER_MISMATCH` |
| CORE-264.3 | 毒丸跳过尾部(CORE-282 的方法) | 不删除;`Note{RecoveryHeldSim}`,`v[1]` 含 `POISON` |
| CORE-264.4 | `--recover-from-snapshot latest` | 自动删除 |

### CORE-265 实例判定:只有全部账户都是 Sim 才按 Sim 实例规则

- 规则:[启动顺序](README.md#启动顺序)
- 级别:table
- 里程碑:M4
- 前置:`startup::sim_only_instance(&refdata)` 纯函数
- 实现:`apps/hft-core/tests/startup.rs::core_265_sim_only_instance`

| 行 | refdata 中本实例的账户 | 期望 |
|---|---|---|
| CORE-265.1 | `sim-a`、`sim-b`(Sim) | true |
| CORE-265.2 | `bn-1`、`okx-1`(Live) | false |

### CORE-266 恢复尾部上界:`keyframe_max_events` ÷ 回放吞吐下限 ≤ 5 s

- 规则:[启动顺序](README.md#启动顺序)
- 级别:unit
- 里程碑:M3
- 前置:取 hft-sequencer 段 5 的 `keyframe_max_events` 默认值与 `seq/replay_throughput` 的预算下限(事件/分钟)两个常量
- 步骤:
  1. 计算 `keyframe_max_events × 60 / 吞吐下限`
- 期望:
  - 结果 ≤ 5.0 s(现值 1,500,000 × 60 / 20,000,000 = 4.5 s);任一常量改动使结果 > 5.0 s 时本测试失败
- 实现:`apps/hft-core/tests/startup.rs::core_266_tail_bound_constants`

### CORE-267 关键帧之后满 `keyframe_max_events` 条输入时步 5 在同版本预算内

- 规则:[启动顺序](README.md#启动顺序)、[延迟与资源契约](README.md#延迟与资源契约)
- 级别:bench
- 里程碑:M3
- 基准名:`startup-cold`(两组:空尾部;尾部 = `keyframe_max_events` 条)
- 实现:`benchmarks/startup-cold/main.rs::core_267_startup_cold`

### CORE-268 kill 自检经真实 `KillWord` 判定每个账户的两种 kill

- 规则:[启动顺序](README.md#启动顺序)
- 级别:table
- 里程碑:M3
- 前置:`c32_prod()`;`binance-usdm` 上 12 个 Live 账户 `bn-1`…`bn-12`(`venue_account_ix` 0–11),取两端 `bn-1`(ix = 0)与 `bn-12`(ix = 11);自检在 gw 线程上执行 `load(Acquire)` 与 `kill_admit`
- 实现:`apps/hft-core/tests/kill_selftest.rs::core_268_selftest_matrix`

| 行 | 写入字的投影 / 合成单 | 期望 `kill_admit` |
|---|---|---|
| CORE-268.1 | `{Kill(Account bn-1)}` / Place,无 `RISK_REDUCING` | false |
| CORE-268.2 | `{Kill(Account bn-1)}` / Place,`RISK_REDUCING` | true |
| CORE-268.3 | `{Kill(Account bn-1), hard}` / Place,无 `RISK_REDUCING` | false |
| CORE-268.4 | `{Kill(Account bn-1), hard}` / Place,`RISK_REDUCING` | false |
| CORE-268.5 | `{Kill(Account bn-12)}` / Place,`RISK_REDUCING` | true |
| CORE-268.6 | `{Kill(Account bn-12), hard}` / Place,`RISK_REDUCING` | false |
| CORE-268.7 | 每行结束后 | 自检对比结果与 kill 放行表一致,两个账户各一条 `Timer{Sys, KillSelfTestOk}` |

### CORE-269 kill 自检逐位回读 32 位,检出位宽截断

- 规则:[启动顺序](README.md#启动顺序)
- 级别:unit
- 里程碑:M3
- 前置:`c32_prod()`;测试构建把 `binance-usdm` 的 `KillWord` 换成只保留低 8 位的替身(store 时丢弃 bit 8–31)
- 步骤:
  1. 运行步 3 的 kill 自检
- 期望:
  - 第 ③ 步在 bit 8 失败;`binance-usdm` 上的账户没有 `Timer{Sys, KillSelfTestOk}`,`IDENTITY` 为 0;`Note{KillSelfTest}` 的 `v[1]` 标出失败;其他 venue 的账户自检通过
- 实现:`apps/hft-core/tests/kill_selftest.rs::core_269_bit_truncation_detected`

### CORE-270 放行函数忽略 `account_hard(ix)` 时自检失败

- 规则:[启动顺序](README.md#启动顺序)
- 级别:unit
- 里程碑:M3
- 前置:`c32_prod()`;测试构建把 `kill_admit` 换成不看 `ACCOUNT_HARD_MASK` 的错误版本
- 步骤:
  1. 运行 kill 自检
- 期望:
  - 第 ② 步在 `{Kill(Account a), hard}` + `RISK_REDUCING` 的组合上失败;全部 Live 账户没有 `Timer{Sys, KillSelfTestOk}`
- 实现:`apps/hft-core/tests/kill_selftest.rs::core_270_ignored_account_hard_detected`

### CORE-271 kill 自检不发帧,结束后每个字回到 0

- 规则:[启动顺序](README.md#启动顺序)
- 级别:integ
- 里程碑:M3
- 前置:`dev2_prod()` 连假交易所
- 步骤:
  1. 启动,在步 3 结束记录写出时读全部 `KillWord`;记录假交易所在步 3 期间收到的帧
- 期望:
  - 全部 `KillWord` = 0;假交易所在步 3 期间收到 0 帧(连接在步 6 才建)
  - Sim 账户(另以 `dev2_sim()` 运行)不占任何 `KillWord`,也没有 `KillSelfTestOk`
- 实现:`apps/hft-core/tests/kill_selftest.rs::core_271_no_frames_words_zero`

### CORE-272 没有停机标记时不写任何 Kill

- 规则:[启动顺序](README.md#启动顺序)
- 级别:unit
- 里程碑:M3
- 前置:`dev2_prod()`;`/var/lib/hft/kill-switch/`(测试根)为空
- 步骤:
  1. 启动到步 5 结束后 1 s
- 期望:
  - `EngineClock` 之后没有 `ControlCmd{Kill}`;目录仍为空
- 实现:`apps/hft-core/tests/stop_marker.rs::core_272_absent_no_kill`

### CORE-273 停机标记按 `scope` 写成 `cli:systemd` 的非 HARD、不带 flatten 的 Kill

- 规则:[启动顺序](README.md#启动顺序)
- 级别:table
- 里程碑:M3
- 前置:`dev2_prod()`;`stop-marker.json` 内容按行;启动到步 5 结束
- 实现:`apps/hft-core/tests/stop_marker.rs::core_273_marker_to_kill`

| 行 | `stop-marker.json` | 期望 `EngineClock` 之后的第一条 Input |
|---|---|---|
| CORE-273.1 | `{"v": 1, "req_id": "r1", "scope": "*", "utc_ns": 1}` | `ControlCmd{Kill{Global, hard = false, flatten = false}}`,issuer `cli:systemd` |
| CORE-273.2 | `{"v": 1, "req_id": "r2", "scope": "okx-1", "utc_ns": 1}` | `ControlCmd{Kill{Account(okx-1)}}`,非 HARD、无 flatten |
| CORE-273.3 | `{"v": 1, "req_id": "r3", "scope": "nobody", "utc_ns": 1}` | `ControlCmd{Kill{Global}}`;其前恰好一条 `Note{StopMarkerCorrupt}`(P0) |
| CORE-273.4 | `{"v": 2, "req_id": "r4", "scope": "okx-1", "utc_ns": 1}` | `ControlCmd{Kill{Global}}`;其前恰好一条 `Note{StopMarkerCorrupt}`(P0) |
| CORE-273.5 | 截断的 JSON(文件只有前 14 字节 `{"v": 1, "sco`) | `ControlCmd{Kill{Global}}`;其前恰好一条 `Note{StopMarkerCorrupt}`(P0) |
| CORE-273.6 | 行 1、2 的文件 | journal 中没有 `Note{StopMarkerCorrupt}` |

### CORE-274 标记在 persist 越过该 Input 之后才改名为 `.consumed`

- 规则:[启动顺序](README.md#启动顺序)
- 级别:proc
- 里程碑:M3
- 前置:`dev2_prod()`;`stop-marker.json`(`scope = "*"`)存在,另有一份旧的 `stop-marker.json.consumed`;测试钩子把 persist 暂停到放行为止
- 步骤:
  1. 启动,等 Kill Input 写出,检查目录
  2. 放行 persist,等 2 s,检查目录
- 期望:
  - 步骤 1:仍是 `stop-marker.json`
  - 步骤 2:`stop-marker.json` 不存在;`stop-marker.json.consumed` 的内容 = 本次标记的字节(旧的被覆盖);冷副本的 `synced_len` 已覆盖该 Input
- 实现:`apps/hft-core/tests/stop_marker.rs::core_274_rename_after_persist`

### CORE-275 改名前崩溃,下次启动再写一条同样的 Kill

- 规则:[启动顺序](README.md#启动顺序)
- 级别:proc
- 里程碑:M3
- 前置:同 CORE-274,persist 暂停
- 步骤:
  1. 启动,Kill Input 写出后 SIGKILL;放开 persist 钩子再启动
- 期望:
  - 第二次启动 `EngineClock` 之后的第一条 Input 仍是 `ControlCmd{Kill{Global}}`;之后标记改名为 `.consumed`;两次 Kill 应用后 kill cap 只有一条(hft-risk 合并)
- 实现:`apps/hft-core/tests/stop_marker.rs::core_275_crash_before_rename`

### CORE-276 只有 `.aborted` 或 `.consumed` 时不写 Kill、文件原样保留;`sim` 实例忽略标记

- 规则:[启动顺序](README.md#启动顺序)
- 级别:table
- 里程碑:M3
- 前置:按行准备目录(记录每个文件的 SHA-256 与 mtime);启动到步 5 结束后 2 s
- 实现:`apps/hft-core/tests/stop_marker.rs::core_276_only_live_marker_name_read`

| 行 | 实例 / 目录内容 | 期望 |
|---|---|---|
| CORE-276.1 | `dev2_prod()` / 只有 `stop-marker.json.aborted` | 无 Kill;文件字节与 mtime 不变;`strace` 中没有对它的 `openat` |
| CORE-276.2 | `dev2_prod()` / 只有 `stop-marker.json.consumed` | 同上 |
| CORE-276.3 | `dev2_sim()` / `stop-marker.json` 存在 | 无 Kill;文件未改名、未读取 |

### CORE-277 停机标记的 Kill 不随 `Recovery` 自动解除而解除

- 规则:[启动顺序](README.md#启动顺序)
- 级别:proc
- 里程碑:M5
- 前置:`dev2_prod()` 连假交易所与 reconciler;`auto_resume_after_clean_drain = true`;干净 Drain 后放入 `stop-marker.json`(`scope = "bn-1"`)
- 步骤:
  1. 重启,等闸门放行
  2. 让 `bn-1` 绑定的 `latency-probe` 实例(`quote` 模式,`enabled = true`)发一张开仓探测单
  3. 发 `Resume{Kill}`(`/resume kill`,Trader,确认),再发一张 Open
- 期望:
  - 步骤 1:`Recovery` 自动解除;`Kill{Account(bn-1)}` cap 仍在
  - 步骤 2:该单被风控拒绝(kill),假交易所未收到
  - 步骤 3:Kill cap 删除;Open 单到达假交易所
- 实现:`apps/hft-core/tests/stop_marker.rs::core_277_kill_survives_auto_resume`

### CORE-278 停机标记的 Kill 与 ops_bot 补发的同范围 Kill 先后到达都幂等

- 规则:[启动顺序](README.md#启动顺序)
- 级别:integ
- 里程碑:M5
- 前置:`dev2_prod()`;`stop-marker.json` `scope = "bn-1"`;启动后 ops_bot 发 `Kill{Account(bn-1)}`(默认参数)
- 步骤:
  1. 启动,等 ops_bot 的 Kill 被处理
- 期望:
  - 两条 `ControlCmd{Kill}` 都 `Applied`;第二条处理后 cap 集合与 kill 字和第一条处理后相同
- 实现:`apps/hft-core/tests/stop_marker.rs::core_278_duplicate_kill_idempotent`

### CORE-279 升级主机类的 `prod` 把引擎区放在 1 GiB 页上

- 规则:[启动顺序](README.md#启动顺序)
- 级别:host
- 里程碑:M7
- 环境:统一升级主机(`c32`),GRUB 已按大页表预留
- 授权:`[需授权]`
- 步骤:
  1. `systemctl start hft-core@prod`,`Running` 后读 `/proc/<pid>/smaps` 与 `hugetlb.1GB.current`
- 通过标准:引擎区所在映射 `KernelPageSize: 1048576 kB`;`hugetlb.1GB.current` > 0 且 ≤ 6 GiB;单元 `memory.current` ≤ 2048 MiB
- 证据:`ops-log/<UTC 日期>-CORE-279.md`
- 实现:`apps/hft-core/tests/host/CORE-279.sh`

### CORE-280 `boot_guard` 的计数:同一区间加 1,不同区间置 1,先落盘再重放,成功后清零

- 规则:[崩溃隔离与从快照恢复](README.md#崩溃隔离与从快照恢复)
- 级别:table
- 里程碑:M3
- 前置:`recovery::boot_guard_next(stored, interval)` 纯函数;`DurableMeta` 用测试目录
- 实现:`apps/hft-core/tests/recovery.rs::core_280_boot_guard_update`

| 行 | 已存 `{snapshot_seq, tail_end_seq, attempts}` / 本次区间 | 期望 |
|---|---|---|
| CORE-280.1 | `{100, 200, 1}` / `(100, 200]` | `{100, 200, 2}` |
| CORE-280.2 | `{100, 200, 1}` / `(100, 201]` | `{100, 201, 1}` |
| CORE-280.3 | `{100, 200, 1}` / `(150, 200]` | `{150, 200, 1}` |
| CORE-280.4 | 无记录 / `(100, 200]` | `{100, 200, 1}` |
| CORE-280.5 | 步 5 成功结束 | `attempts = 0` 经 `write_sync` 落盘 |
| CORE-280.6 | 新值的 `write_sync` 返回之前 | `RecoverDriver` 尚未被调用(调用顺序记录) |

### CORE-281 启动时 `attempts ≥ poison_crash_limit` 才跳过尾部(边界)

- 规则:[崩溃隔离与从快照恢复](README.md#崩溃隔离与从快照恢复)、[配置文件字段](README.md#配置文件字段)
- 级别:table
- 里程碑:M3
- 前置:`recovery::plan(stored_boot_guard, interval, poison_crash_limit)`;本次区间与已存区间相同
- 实现:`apps/hft-core/tests/recovery.rs::core_281_poison_limit_boundary`

| 行 | `poison_crash_limit` / 已存 `attempts` | 期望 `RecoverPlan` |
|---|---|---|
| CORE-281.1 | 2 / 1 | `replay_tail = true`,`flags` 无 `POISON`;存 `attempts = 2` |
| CORE-281.2 | 2 / 2 | `replay_tail = false`,`flags = POISON \| TAIL_LOST`;`Note{TailSkipped{from = 101, to = 200, reason = Poison}}`(`v[0] = 101`、`v[1] = 200`、`v[2] = 1`) |
| CORE-281.3 | 3 / 2 | `replay_tail = true` |
| CORE-281.4 | 3 / 3 | `replay_tail = false`,`POISON \| TAIL_LOST` |

### CORE-282 重放必崩的尾部:第 1、2 次重启在步 5 崩溃,第 3 次跳过尾部

- 规则:[崩溃隔离与从快照恢复](README.md#崩溃隔离与从快照恢复)
- 级别:proc
- 里程碑:M3
- 前置:`dev2_prod()` 连假交易所,`poison_crash_limit = 2`;测试构建在重放到 seq = P 的 Input 时 abort(实盘处理该 Input 不崩);运行到 P 之后 SIGKILL;进程由测试直接拉起(不经 systemd)
- 步骤:
  1. 依次启动三次
- 期望:
  - 第 1、2 次在步 5 以 SIGABRT 结束;`DurableMeta` 的 `boot_guard.attempts` 依次为 1、2
  - 第 3 次:不重放尾部,`Note{TailSkipped{reason = Poison}}`(`v[2] = 1`),全部非终态槽 `Unknown`,`StartupStep` 步 7 结束记录 `v[3]` bit 2 = 1;闸门放行后 `Recovery` 不自动解除
  - 被跳过的尾部仍在旧段;`replayer verify --from-seq <snapshot_seq>` 在 seq = P 处复现 abort
- 实现:`apps/hft-core/tests/poison.rs::core_282_boot_guard_skips_tail`

### CORE-283 处理必崩、重启后同样输入再到达:第 3 次启动照常重放并隔离该输入

- 规则:[崩溃隔离与从快照恢复](README.md#崩溃隔离与从快照恢复)
- 级别:proc
- 里程碑:M3
- 前置:`dev2_prod()` 连假交易所,`poison_crash_limit = 2`;测试构建在实盘处理内容为 X 的对账报告时 abort;reconciler 每次启动后都重新生成内容 X 的报告;进程由测试直接拉起
- 步骤:
  1. 首次启动(处理 X 时崩溃),再启动两次
- 期望:
  - 第 2 次启动后 `poison_input = {hash = H(X), count = 1}`,处理 X 时再次崩溃;第 3 次启动 `count = 2`,尾部照常重放,X 再到达时不进入 journal,写一条 `Note{InputQuarantined}`(`v[1] = 2`);进程继续运行
  - `boot_guard` 与 `poison_input` 是 `DurableMeta` 中的两个键
- 实现:`apps/hft-core/tests/poison.rs::core_283_input_quarantined`

### CORE-284 `--recover-from-snapshot` 的快照选择

- 规则:[崩溃隔离与从快照恢复](README.md#崩溃隔离与从快照恢复)
- 级别:table
- 里程碑:M3
- 前置:journal 中有快照 S1(seq 1000)、S2(seq 5000)、S3(seq 9000);按行把某些快照设为本二进制不可解
- 实现:`apps/hft-core/tests/recover_from_snapshot.rs::core_284_snapshot_selection`

| 行 | 参数 / 不可解的快照 | 期望 |
|---|---|---|
| CORE-284.1 | `latest` / 无 | 选 S3 |
| CORE-284.2 | `latest` / S3 | 选 S2 |
| CORE-284.3 | `latest` / S1、S2、S3 | 退出 4,`NoDecodableSnapshot` |
| CORE-284.4 | `5000` / 无 | 选 S2 |
| CORE-284.5 | `4999` / 无 | 选 S1 |
| CORE-284.6 | `5000` / S2 | 退出 4,`SnapshotUndecodable`(不回退到 S1) |
| CORE-284.7 | `999` / 无 | 退出 4,`NoDecodableSnapshot` |

### CORE-285 从快照起步:跳过版本检查、段链不断、新段引导块带标记

- 规则:[崩溃隔离与从快照恢复](README.md#崩溃隔离与从快照恢复)
- 级别:proc
- 里程碑:M3
- 前置:journal 末条 seq = 12000,由新版本二进制写出(不是 Drain 快照结尾);旧版本二进制能解码的最新快照 seq = 5000;`--reason "rollback v0.4"`
- 步骤:
  1. 旧二进制 `--recover-from-snapshot latest --reason "rollback v0.4"` 启动,到步 5 结束
- 期望:
  - 无 `VersionNeedsDrain`;新段的第一条记录 seq = 12001,段头 `prev_header_sha256` 等于上一段段头的 SHA-256
  - 新段引导块带 `RecoverFromSnapshot{snapshot_seq = 5000, skipped_from = 5001, skipped_to = 12000, reason = "rollback v0.4", issuer = cli:systemd}`;`Note{RecoverFromSnapshot}` 一条;`Note{TailSkipped{reason = Operator}}`(`v[2] = 3`)一条
  - `EngineClock.source = Snapshot`(3),`base_engine_ns` = 快照的 `engine_ns`;全部非终态槽 `Unknown`、每账户 `Recovery` cap
- 实现:`apps/hft-core/tests/recover_from_snapshot.rs::core_285_marker_and_chain`

### CORE-286 回滚全流程:`VersionNeedsDrain` → 从旧 Drain 快照起步 → 对账处置后才可恢复

- 规则:[崩溃隔离与从快照恢复](README.md#崩溃隔离与从快照恢复)
- 级别:proc
- 里程碑:M5
- 前置:`dev2_prod()` 连假交易所与 reconciler;旧版本运行后 SIGTERM(Drain 快照 D,epoch 7);新版本启动、运行(epoch 8)后 SIGKILL;假交易所在新版本运行期间改变了 `bn-1` 的仓位
- 步骤:
  1. 旧二进制正常启动
  2. 旧二进制 `--recover-from-snapshot latest --reason r`
  3. 发 `Resume{Recovery, bn-1}`;按对账差异发 `AdoptExchange`;再发 `Resume{Recovery, bn-1}`
  4. 对整段 journal 运行 `replayer verify`
- 期望:
  - 步骤 1 退出码 4,`VersionNeedsDrain`
  - 步骤 2 选中 D;`Note{EpochStart}` 的 `v[0] = 9`
  - 步骤 3 第一次 `PreconditionFailed`(`RECONCILED` 为 0);`AdoptExchange` 后第二次 `Applied`
  - 步骤 4 在 `RecoverFromSnapshot` 引导块处重新起步,两段各自一致,退出 0
- 实现:`apps/hft-core/tests/recover_from_snapshot.rs::core_286_rollback_flow`

### CORE-287 `sim` 实例 `--recover-from-snapshot` 后按 Sim 实例条件自动解除

- 规则:[崩溃隔离与从快照恢复](README.md#崩溃隔离与从快照恢复)
- 级别:proc
- 里程碑:M4
- 前置:`dev2_sim()`,`auto_resume_after_clean_drain = true`
- 步骤:
  1. SIGKILL 后以 `--recover-from-snapshot latest --reason r` 启动
- 期望:
  - `Note{TailSkipped{reason = Operator}}` 一条;闸门放行时 `Recovery` 自动删除;无 `Note{RecoveryHeldSim}`
- 实现:`apps/hft-core/tests/recover_from_snapshot.rs::core_287_sim_auto_resume`

### CORE-288 一次性传参:成功启动后移除 drop-in(演练)

- 规则:[崩溃隔离与从快照恢复](README.md#崩溃隔离与从快照恢复)
- 级别:drill
- 里程碑:M5
- 环境:dev2,`hft-core@sim`
- 授权:`[需授权]`
- 步骤:按 [incident-response D](../../docs/runbooks/incident-response.md#d-hft-core-崩溃重启风暴拒绝启动) 与 [release-and-deploy 回退](../../docs/runbooks/release-and-deploy.md#回退) 的步骤,用 drop-in 传 `--recover-from-snapshot latest --reason <文本>` 启动,成功后移除 drop-in 并 `daemon-reload`,再 `systemctl restart hft-core@sim`
- 通过标准:第一次启动的新段带 `RecoverFromSnapshot` 引导块;移除后的重启没有新的 `RecoverFromSnapshot`,`EngineClock.source = Tail`
- 证据:`ops-log/<UTC 日期>-CORE-288.md`
- 实现:`apps/hft-core/tests/host/CORE-288.sh`

### CORE-289 改绑账户而快照中该账户有仓位、挂单或 `Unknown` 时以 `BindingChangedWithPosition` 退出 2

- 规则:[启动顺序](README.md#启动顺序)
- 级别:table
- 里程碑:M3
- 前置:`dev2_prod()`,`config_source = "file"`;干净 Drain 后换入把 `bn-1` 改绑另一实例的 refdata(SHA-256 同步),按行准备快照中 `bn-1` 的状态,再启动
- 实现:`apps/hft-core/tests/recovery.rs::core_289_rebind_with_position`

| 行 | 快照中 `bn-1` 的状态 | 期望 |
|---|---|---|
| CORE-289.1 | 无仓位、无挂单、无 `Unknown` | 启动继续;新绑定生效 |
| CORE-289.2 | `binance-usdm` 上有 0.001 BTC 仓位 | 退出码 2,错误行 `BindingChangedWithPosition`,详情含 `bn-1`;没有写出 `EngineClock` |
| CORE-289.3 | 有一张非终态挂单 | 同上 |

### CORE-299 大页映射失败时以 `Hugepages` 退出 3

- 规则:[启动顺序](README.md#启动顺序)、[命令行与退出码](README.md#命令行与退出码)
- 级别:proc
- 里程碑:M3
- 前置:`c32_prod()` 配 `sysroot::c32()`,`rt.host.mode = "dev"`(`HUGEPAGES_SHORT` 不拒绝);运行环境没有空闲的 1 GiB 页
- 步骤:
  1. 启动
- 期望:
  - 退出码 3;stderr 恰好一条错误行 `Hugepages`;journal 根下没有段文件
- 实现:`apps/hft-core/tests/startup.rs::core_299_hugepages_exit3`

### CORE-300 SIGTERM:经 ctl-gateway 提交 `cli:systemd` 的 `Drain`,按序封存后退出 0

- 规则:[优雅退出与 kill 语义](README.md#优雅退出与-kill-语义)
- 级别:proc
- 里程碑:M3
- 前置:`dev2_prod()` 连假交易所;`shutdown.cancel_open_orders = true`;`bn-1` 有 2 张挂单
- 步骤:
  1. 到 `Running` 后对进程发 SIGTERM
- 期望:
  - journal 依次有:`ControlCmd{Drain{cancel_resting = true}}`(issuer `cli:systemd`)、两张挂单的撤单决策、`purpose = Drain` 的快照;每个 journal 末段以 `End` 结束并 `Sealed`
  - 假交易所收到两张撤单;退出码 0;退出时 `HkLine.state = 5`;冷副本 `synced_len` = 热段 `committed_len`
- 实现:`apps/hft-core/tests/shutdown.rs::core_300_sigterm_drain_sequence`

### CORE-301 SIGINT 与 SIGTERM 同一路径

- 规则:[优雅退出与 kill 语义](README.md#优雅退出与-kill-语义)
- 级别:proc
- 里程碑:M3
- 前置:同 CORE-300
- 步骤:
  1. 发 SIGINT
- 期望:
  - journal 中有 `ControlCmd{Drain}`(issuer `cli:systemd`);退出码 0;`state = 5`
- 实现:`apps/hft-core/tests/shutdown.rs::core_301_sigint_same_path`

### CORE-302 `shutdown.cancel_open_orders = false` 时 SIGTERM 不撤挂单

- 规则:[优雅退出与 kill 语义](README.md#优雅退出与-kill-语义)
- 级别:proc
- 里程碑:M3
- 前置:同 CORE-300,`shutdown.cancel_open_orders = false`
- 步骤:
  1. 发 SIGTERM
- 期望:
  - `ControlCmd{Drain{cancel_resting = false}}`;假交易所没有收到撤单;两张挂单在 Drain 快照中为非终态;退出码 0
- 实现:`apps/hft-core/tests/shutdown.rs::core_302_sigterm_keeps_orders`

### CORE-303 Drain 期间有已发第一腿、未发第二腿的链接时等它结束才写快照

- 规则:[优雅退出与 kill 语义](README.md#优雅退出与-kill-语义)
- 级别:integ
- 里程碑:M3
- 前置:`dev2_prod()` 连假交易所;`latency-probe` 的 `pair` 模式在 `binance-usdm` 下第一腿;假交易所把第一腿成交回报延迟 500 ms
- 步骤:
  1. 第一腿发出后 100 ms 发 SIGTERM
- 期望:
  - 第二腿的 `OrderDecision{New}` 写在 Drain 快照之前;快照中该链接已结束(无未结束链接);退出码 0
- 实现:`apps/hft-core/tests/shutdown.rs::core_303_drain_waits_leg_link`

### CORE-304 Drain 超时:`Note{DrainTimeout}` 后按当前状态封存,退出码按原因

- 规则:[优雅退出与 kill 语义](README.md#优雅退出与-kill-语义)、[配置文件字段](README.md#配置文件字段)
- 级别:table
- 里程碑:M3
- 前置:`dev2_prod()` 连假交易所;`shutdown.drain_timeout_ms = 8000`;假交易所对撤单永不回执(Drain 等待条件不满足);按行触发 Drain
- 实现:`apps/hft-core/tests/shutdown.rs::core_304_drain_timeout`

| 行 | 触发 | 期望 |
|---|---|---|
| CORE-304.1 | SIGTERM | 一条 `Note{DrainTimeout}`,写在 SIGTERM 之后 ≥ 8,000 ms;之后快照、封存;退出码 0 |
| CORE-304.2 | 存储危急(`Timer{Sys, DiskLow}` 档位 2) | `Note{DrainTimeout}`;退出码 3 |
| CORE-304.3 | SIGTERM,假交易所在 7,000 ms 时回执全部撤单 | 没有 `DrainTimeout`;退出码 0 |

### CORE-305 干净退出后 kill-switch 不动作

- 规则:[优雅退出与 kill 语义](README.md#优雅退出与-kill-语义)、[HeartbeatBlock](README.md#heartbeatblock)
- 级别:proc
- 里程碑:M5
- 前置:`dev2_prod()` 与 kill-switch 都连假交易所;`bn-1` 无挂单
- 步骤:
  1. SIGTERM hft-core,等退出,再观察 30 s
- 期望:
  - kill-switch 没有发出任何撤单请求;其状态记录"心跳停止"的判定为干净退出(`state = Stopped`)
- 实现:`apps/hft-core/tests/shutdown.rs::core_305_kill_switch_idle_after_clean_exit`

### CORE-306 `StorageCritical` 触发的 Drain 完成后退出 3

- 规则:[优雅退出与 kill 语义](README.md#优雅退出与-kill-语义)、[命令行与退出码](README.md#命令行与退出码)
- 级别:proc
- 里程碑:M5
- 前置:`dev2_sim()`;`storage.disk_free_drain_pct = 5`;冷盘测试卷可用空间被压到 4%
- 步骤:
  1. 等 housekeeping 采样
- 期望:
  - `Timer{Sys, DiskLow}`(档位 2)Input;随后 Drain、快照;退出码 3;stderr 有 `error[StorageCritical]`
- 实现:`apps/hft-core/tests/shutdown.rs::core_306_storage_critical_exit3`

### CORE-307 SIGHUP 被忽略

- 规则:[优雅退出与 kill 语义](README.md#优雅退出与-kill-语义)
- 级别:proc
- 里程碑:M3
- 前置:`dev2_sim()` 到 `Running`
- 步骤:
  1. 发 SIGHUP,等 5 s
- 期望:
  - 进程存活、pid 不变;这 5 s 内 journal 没有 `ControlCmd` 与 `ConfigBlob`;`state = 3`
- 实现:`apps/hft-core/tests/shutdown.rs::core_307_sighup_ignored`

### CORE-308 `Drain` 命令走同一 sequencer 路径并退出 0

- 规则:[优雅退出与 kill 语义](README.md#优雅退出与-kill-语义)
- 级别:proc
- 里程碑:M3
- 前置:`dev2_prod()`;ctl socket 以 Trader 证书连接
- 步骤:
  1. 发 `Drain{cancel_resting = true}` 并确认
- 期望:
  - journal 中 `ControlCmd{Drain}` 的 issuer 为该 Trader;Drain 快照;退出码 0
- 实现:`apps/hft-core/tests/shutdown.rs::core_308_drain_command_exit0`

### CORE-309 崩溃时不做清理:RAM 热段存活,心跳停在非 `Stopped`

- 规则:[优雅退出与 kill 语义](README.md#优雅退出与-kill-语义)
- 级别:proc
- 里程碑:M3
- 前置:`dev2_prod()` 到 `Running`
- 步骤:
  1. SIGKILL
- 期望:
  - 热段文件仍在,最后一段的 `committed_len` > 0 且未 `Sealed`;`HkLine.state = 3`;重启后走完整启动顺序(16 条 `StartupStep`)
- 实现:`apps/hft-core/tests/shutdown.rs::core_309_crash_keeps_hot_segments`

### CORE-310 sequencer 停摆时 housekeeping 停发 `WATCHDOG=1`

- 规则:[优雅退出与 kill 语义](README.md#优雅退出与-kill-语义)、[HeartbeatBlock](README.md#heartbeatblock)
- 级别:proc
- 里程碑:M3
- 前置:`dev2_sim()`;`watchdog.hb_stale_ms = 1000`;`NOTIFY_SOCKET` 指向测试套接字;测试钩子可让 sequencer 主循环卡住
- 步骤:
  1. `Running` 后观察 3 s 的 `WATCHDOG=1`
  2. 卡住 sequencer,观察 3 s
- 期望:
  - 步骤 1:每秒至少一次 `WATCHDOG=1`
  - 步骤 2:卡住之后 2 s(= 2 × `hb_stale_ms`)起不再收到 `WATCHDOG=1`;其他线程的心跳计数仍在前进
- 实现:`apps/hft-core/tests/shutdown.rs::core_310_stall_stops_watchdog`

### CORE-311 systemd `WatchdogSec` 到期发 SIGABRT

- 规则:[优雅退出与 kill 语义](README.md#优雅退出与-kill-语义)
- 级别:host
- 里程碑:M3
- 环境:dev2,测试构建的 `hft-core@sim`(带可远程触发的 sequencer 卡住钩子)
- 授权:`[需授权]`
- 步骤:
  1. 触发卡住,等 `WatchdogSec` + 5 s
- 通过标准:`systemctl show -p Result` 为 `watchdog`;`ExecMainStatus` 对应 SIGABRT;单元随后按 `on-failure` 重启
- 证据:`ops-log/<UTC 日期>-CORE-311.md`
- 实现:`apps/hft-core/tests/host/CORE-311.sh`

### CORE-312 `shutdown.cancel_open_orders` 没有默认值

- 规则:[优雅退出与 kill 语义](README.md#优雅退出与-kill-语义)
- 级别:unit
- 里程碑:M3
- 前置:`dev2_prod()` 去掉 `shutdown.cancel_open_orders`
- 步骤:
  1. `config::load_and_validate`
- 期望:
  - 恰好一条 `MissingField`(`shutdown.cancel_open_orders`)
- 实现:`apps/hft-core/tests/shutdown.rs::core_312_cancel_open_orders_required`

### CORE-313 按账户 HARD 只封该账户:账户 b 的出环不受影响

- 规则:[优雅退出与 kill 语义](README.md#优雅退出与-kill-语义)、[多账户](README.md#多账户)
- 级别:integ
- 里程碑:M3
- 前置:`dev2_prod()` 连假交易所;`binance-usdm` 上 `bn-1`(ix 0)、`bn-2`(ix 1)两个 Live 账户,各有 `latency-probe` 实例下探测单
- 步骤:
  1. 发 `Kill{Account(bn-1), hard}` 并确认,观察 30 s
- 期望:
  - `binance-usdm` 的 `KillWord` 的 bit 20(`account_hard(0)`)为 1,bit 9(`account(1)`)与 bit 21(`account_hard(1)`)为 0
  - 这 30 s 内假交易所继续收到 `bn-2` 的下单与撤单;`bn-1` 的 Place/Amend 全部在 gw 以 `KillBlocked` 拒绝(含 `RISK_REDUCING`),撤单仍发出
- 实现:`apps/hft-core/tests/kill.rs::core_313_account_hard_isolated`

### CORE-314 Sim 账户的 kill 经检查链生效,不写任何 `KillWord`

- 规则:[优雅退出与 kill 语义](README.md#优雅退出与-kill-语义)
- 级别:integ
- 里程碑:M4
- 前置:`dev2_sim()`;`sim-a` 的策略持续下 Open 与 Reduce 单
- 步骤:
  1. `Kill{Account(sim-a)}`;观察 10 s;再 `Kill{Account(sim-a), hard}`;观察 10 s
- 期望:
  - 第一段:Open 被风控拒绝,Reduce 进入 `out-sim`;第二段:Open 与 Reduce 都被拒绝
  - 进程中没有分配任何 `KillWord`(`kill_words.len() = 0`);`out-<venue>` 环不存在
- 实现:`apps/hft-core/tests/kill.rs::core_314_sim_kill_via_risk_chain`

### CORE-320 `config_source = "file"` 时 `ReplaceRefdata` 被拒

- 规则:[refdata 变更(Drain + 重启)](README.md#refdata-变更drain--重启)
- 级别:integ
- 里程碑:M3
- 前置:`dev2_sim()`,`instance.config_source = "file"`;Admin 经 `cli:<login>` 连接
- 步骤:
  1. 发 `ReplaceRefdata{blob, version}` 并确认
- 期望:
  - `CmdResult` 为 `PreconditionFailed`;没有 `ConfigBlob{Refdata}`、没有 Drain
- 实现:`apps/hft-core/tests/refdata_change.rs::core_320_file_source_rejected`

### CORE-321 ctl-gateway 的装配校验不通过:`Rejected{RefdataAssembly}`,命令不进 sequencer

- 规则:[refdata 变更(Drain + 重启)](README.md#refdata-变更drain--重启)
- 级别:table
- 里程碑:M3
- 前置:`instance.config_source = "journal"`;Admin 发 `ReplaceRefdata` 并确认;新 blob 按行
- 实现:`apps/hft-core/tests/refdata_change.rs::core_321_gateway_assembly_check`

| 行 | 实例 / 新 refdata 的问题 | 期望响应 |
|---|---|---|
| CORE-321.1 | `dev2_sim()` / 新增账户 `research-x`(Sim,缺 `sim-` 前缀) | `Rejected{RefdataAssembly}`,arg = 1 |
| CORE-321.2 | `dev2_prod()` / 新增 Live 账户 `bn-9`,配置中没有它的 key | arg = 2 |
| CORE-321.3 | `dev2_sim()` / `sim-a` 的 `latency_model = 5`(表只有 1 项) | arg = 3 |
| CORE-321.4 | `dev2_prod()` / 新增 venue `bitget-spot` 上的 inst 与账户,线程计划没有它的行情任务 | arg = 4 |
| CORE-321.5 | `dev2_prod()` / 把 `bn-1` 某交易腿 inst 的 `min_notional` 改为高于 `dust_notional` | arg = 5 |
| CORE-321.6 | 每行 | journal 中没有该命令的 `ControlCmd` 与 `CmdResult`;一条 `Note{CtlAudit}`,结果码 `Rejected`(27);生效 refdata 不变;进程不退出 |

### CORE-322 sequencer 复核不通过:`CmdResult{Rejected, detail = RefdataRecheck}`,不 Drain

- 规则:[refdata 变更(Drain + 重启)](README.md#refdata-变更drain--重启)
- 级别:integ
- 里程碑:M3
- 前置:`dev2_prod()`,`config_source = "journal"`;`bn-1` 在 inst `I` 上有仓位;新 blob 删除 `I`
- 步骤:
  1. Admin 发 `ReplaceRefdata` 并确认
- 期望:
  - `CmdResult{Rejected, detail = RefdataRecheck}`;没有 `ControlCmd{Drain}`;生效 refdata 版本不变;进程继续运行
- 实现:`apps/hft-core/tests/refdata_change.rs::core_322_recheck_rejects`

### CORE-323 通过:写 `ConfigBlob` → Drain → 复核通过 → 退出 75 → 重启后新 refdata 生效

- 规则:[refdata 变更(Drain + 重启)](README.md#refdata-变更drain--重启)
- 级别:proc
- 里程碑:M3
- 前置:`dev2_prod()` 连假交易所,`config_source = "journal"`;新 blob 只改一个费率档的数值(inst、资产、账户与绑定不变);测试进程监督者在退出码 75 时立即重启
- 步骤:
  1. Admin 发 `ReplaceRefdata` 并确认;等重启后的闸门放行
- 期望:
  - journal 依次:`ControlCmd{ReplaceRefdata}` 的 `CmdResult{Applied}`、`ConfigBlob{Refdata}`(新版本)、`ControlCmd{Drain{cancel_resting = false}}`、`Timer{Sys, RefdataRecheck}`(通过)、Drain 快照;退出码 75
  - 重启后尾部为空,生效 refdata 为新版本;从退出到全部闸门通过 < 30 s;`Recovery` 按干净 Drain 自动解除
- 实现:`apps/hft-core/tests/refdata_change.rs::core_323_replace_restart_75`

### CORE-324 Drain 末复核不通过:旧 refdata 保留,以 0 退出,人工启动后按旧 refdata 运行

- 规则:[refdata 变更(Drain + 重启)](README.md#refdata-变更drain--重启)
- 级别:proc
- 里程碑:M3
- 前置:`dev2_prod()` 连假交易所;新 blob 把 `bn-1` 改绑另一实例(此时 `bn-1` 无仓位,有一张挂单);`ReplaceRefdata` 的 Drain 不撤挂单;假交易所在 Drain 期间让该挂单成交
- 步骤:
  1. Admin 发 `ReplaceRefdata` 并确认,等进程退出
  2. 人工启动
  3. 对整段 journal 运行 `replayer verify`
- 期望:
  - 步骤 1:`Timer{Sys, RefdataRecheck}` 的结果为不通过;`Note{RefdataRecheckFailed}`;Drain 快照中生效 refdata 为旧版本、没有待生效引用;退出码 0
  - 步骤 2:生效 refdata 为旧版本,`bn-1` 仍绑原实例
  - 步骤 3:退出 0
- 实现:`apps/hft-core/tests/refdata_change.rs::core_324_drain_recheck_fails_exit0`

### CORE-325 Drain 超时同样先做复核

- 规则:[refdata 变更(Drain + 重启)](README.md#refdata-变更drain--重启)
- 级别:integ
- 里程碑:M3
- 前置:同 CORE-323,但假交易所对撤单与查单永不回执;`shutdown.drain_timeout_ms = 8000`
- 步骤:
  1. Admin 发 `ReplaceRefdata` 并确认
- 期望:
  - `Timer{Sys, RefdataRecheck}` 写在 Drain 快照之前,`Note{DrainTimeout}` 之后;复核通过时退出码 75
- 实现:`apps/hft-core/tests/refdata_change.rs::core_325_timeout_still_rechecks`

### CORE-326 启动步 2 与 `RefdataAssemblyCheck` 是同一组函数

- 规则:[refdata 变更(Drain + 重启)](README.md#refdata-变更drain--重启)
- 级别:prop
- 里程碑:M3
- 性质:对任意 (refdata, 配置) 组合,`RefdataAssemblyCheck` 返回的失败项集合 = 启动步 2 对同一组合给出的错误中属于 {`AccountNamePrefix`、`MissingCredential`、`SimConfig`、线程计划未覆盖 venue、`DustBelowMinNotional`} 的集合(按 arg 码 1–5 映射)
- 生成:从 `dev2_prod()`、`dev2_sim()` 起,随机增删账户(前缀合规或不合规)、增删 key 引用、改 `latency_model` 下标、增删 venue 与其行情任务、改 `min_notional` 与 `dust_notional`
- 参考:直接调用启动步 2 的校验器并过滤
- 例数:PR 500
- 实现:`apps/hft-core/tests/refdata_change.rs::core_326_same_assembly_functions`

### CORE-330 两个 Live 账户同 venue 同时下单:client id 不冲突,限速桶独立

- 规则:[多账户](README.md#多账户)
- 级别:integ
- 里程碑:M3
- 前置:`dev2_prod()` 连假交易所;`binance-usdm` 上 `bn-1`、`bn-2`;两者的 `RiskParams` 限速各 10 单/s;`bn-1` 的策略以 50 单/s 发单,`bn-2` 以 5 单/s
- 步骤:
  1. 运行 10 s
- 期望:
  - 假交易所收到的全部 client order id 两两不同;`bn-1` 被风控限速拒绝的单数 > 0,`bn-2` 被限速拒绝的单数 = 0;两个账户的下单连接不同(源地址分别为各自的 `gw.src_ip`)
- 实现:`apps/hft-core/tests/multi_account.rs::core_330_client_ids_and_buckets`

### CORE-340 `check_host` 的 hard 位使启动以 `HostCheck` 退出 3,非 hard 位只写进 `HostProfile`

- 规则:[HostProfile](README.md#hostprofile)
- 级别:table
- 里程碑:M3
- 前置:`rt.host.mode = "prod"`;主机夹具按行;`startup::run_steps_1_2`
- 实现:`apps/hft-core/tests/hostcheck.rs::core_340_hard_and_soft_bits`

| 行 | 配置 / 夹具 | 期望 |
|---|---|---|
| CORE-340.1 | `c32_prod()` / `sysroot::c32()`,`isolated` 不含核 9 | 退出码 3,`HostCheck`,详情含 `HOT_NOT_ISOLATED` |
| CORE-340.2 | `c32_prod()` / `sysroot::c32()`,`numa_balancing = 1` | 无错误;`HostProfile.host_checks` 含 `NUMA_BALANCING_ON` |
| CORE-340.3 | `c32_sim()` / `sysroot::c32()`,`isolated = 5-17`,`sim` 线程绑核 23–27 | 无错误(`HostNeeds.warm` = 23–27 不在 `isolated`) |
| CORE-340.4 | `dev2_sim()` / `sysroot::dev2()`,`isolated` 为空 | 无错误(dev2 不检查隔离) |

### CORE-341 装配按本页填入 `HostNeeds`

- 规则:[HostProfile](README.md#hostprofile)
- 级别:table
- 里程碑:M3
- 前置:`hostcheck::needs(&cfg, &held)`;本实例已持有的段 `held` 由夹具给出
- 实现:`apps/hft-core/tests/hostcheck.rs::core_341_host_needs`

| 行 | 配置 | 期望 |
|---|---|---|
| CORE-341.1 | `dev2_sim()`,`held.shm_bytes = 0` | `shm_bytes` = 192 MiB;`irq` = ∅;`warm` = 全部 `core` 非 `none` 的非热核 {0, 1} |
| CORE-341.2 | `c32_prod()` | `irq` = {1};`warm` = {0, 2, 3, 4};`hugepages_1g` 与 `hugepages_2m` = 大页表的 6 与 1,792 |
| CORE-341.3 | `c32_prod_eni_b2()` | `irq` = {1, 28} |
| CORE-341.4 | `c4_prod()` | `irq` = ∅ |

### CORE-342 hostprofile.json 缺失、不符或待重启时按 `host.on_mismatch` 处理

- 规则:[HostProfile](README.md#hostprofile)
- 级别:table
- 里程碑:M3
- 前置:`dev2_sim()`;`host.expect_profile` 为测试期望;`paths.hostprofile` 按行
- 实现:`apps/hft-core/tests/hostcheck.rs::core_342_on_mismatch`

| 行 | hostprofile.json / `host.on_mismatch` | 期望 |
|---|---|---|
| CORE-342.1 | 与期望一致 / `refuse` | 无错误 |
| CORE-342.2 | 文件不存在 / `refuse` | 退出码 3,`HostProfile` |
| CORE-342.3 | 文件不存在 / `degrade` | 继续启动;一条 `Note{HostDegraded}` |
| CORE-342.4 | `sysctl` 一项与期望不符 / `refuse` | 退出码 3,`HostProfile`,详情含该项 |
| CORE-342.5 | 同上 / `degrade` | 继续;`Note{HostDegraded}` |
| CORE-342.6 | `pending_reboot = true` / `refuse` | 退出码 3,`HostProfile`,详情含 `pending_reboot` |

### CORE-343 hostprofile.json 的内容原样写成 `ConfigBlob{HostManifest}`

- 规则:[HostProfile](README.md#hostprofile)
- 级别:proc
- 里程碑:M3
- 前置:`dev2_sim()`;hostprofile.json 为一份 3 KiB 的夹具
- 步骤:
  1. 启动到步 5 结束
- 期望:
  - 一条 `ConfigBlob{HostManifest}`,内容字节 = 夹具文件字节;一条 `HostProfile` 事件,`host_checks` 等于 `check_host` 的非 hard 位
- 实现:`apps/hft-core/tests/hostcheck.rs::core_343_host_manifest_blob`

### CORE-350 心跳块的路径、大小、权限与槽

- 规则:[HeartbeatBlock](README.md#heartbeatblock)
- 级别:proc
- 里程碑:M3
- 前置:`dev2_prod()`;`paths.heartbeat_shm` 的父目录按 deploy 路径表建为属组 `hft-journal-ro`、`2750`
- 步骤:
  1. 启动到步 3 结束
- 期望:
  - 文件大小 4096 B,模式 `0640`,属主为运行用户,属组 `hft-journal-ro`;`HkLine.magic = *b"HFTHB\0\x01\0"`、`layout_version = 1`、`instance_id = 1`、`pid` = 进程 pid
  - 区 0 交给 sequencer;区 1 由 housekeeping 每 1 s 更新 `hk_utc_ns`
- 实现:`apps/hft-core/tests/heartbeat.rs::core_350_block_path_mode_slots`

### CORE-351 心跳状态按启动与退出推进,运行中不回退

- 规则:[HeartbeatBlock](README.md#heartbeatblock)、[实现说明](README.md#实现说明)
- 级别:table
- 里程碑:M3
- 前置:`dev2_prod()` 连假交易所与 reconciler;读 `HkLine.state` 的变化序列
- 实现:`apps/hft-core/tests/heartbeat.rs::core_351_state_sequence`

| 行 | 过程 | 期望序列 |
|---|---|---|
| CORE-351.1 | 启动 → 闸门全过 → SIGTERM | 1 → 2 → 3 → 4 → 5 |
| CORE-351.2 | 启动,闸门未全过(reconciler 不运行)时 SIGTERM | 1 → 2 → 4 → 5 |
| CORE-351.3 | 闸门全过后假交易所断开 `bn-1` 的全部连接 30 s | 保持 3(不回到 2) |
| CORE-351.4 | 启动后在 `Warmup` 时 SIGKILL | 停在 2 |
| CORE-351.5 | SIGTERM 后测试钩子把 persist 暂停 3 s | 4 保持到 persist 刷完,之后才 5 |

### CORE-352 判活:状态在 {2, 3, 4} 且区 0 计数在 `hb_stale_ms` 内前进

- 规则:[HeartbeatBlock](README.md#heartbeatblock)
- 级别:table
- 里程碑:M3
- 前置:`housekeeping::alive(state, progressed_within_hb_stale)` 纯函数;`watchdog.hb_stale_ms = 1000`
- 实现:`apps/hft-core/tests/heartbeat.rs::core_352_liveness_rule`

| 行 | `state` / 区 0 计数最近一次前进距今 | 期望 |
|---|---|---|
| CORE-352.1 | 3 / 500 ms | alive |
| CORE-352.2 | 3 / 2000 ms | stalled |
| CORE-352.3 | 2 / 500 ms | alive |
| CORE-352.4 | 4 / 500 ms | alive |
| CORE-352.5 | 5 / 任意 | 干净退出(不是 stalled) |
| CORE-352.6 | 1 / 500 ms | 不判 alive |

### CORE-360 每个必填字段缺失各报一条 `MissingField`,全部缺失时一次报全

- 规则:[配置文件字段](README.md#配置文件字段)
- 级别:table
- 里程碑:M3
- 前置:从所列构造器删去该键;`config::load_and_validate`
- 实现:`apps/hft-core/tests/config_fields.rs::core_360_required_fields`

| 行 | 构造器 / 删去的键 | 期望错误列表 |
|---|---|---|
| CORE-360.1 | `dev2_sim()` / `schema` | 恰好 {`MissingField`(`schema`)} |
| CORE-360.2 | `dev2_sim()` / `instance.name` | 恰好 {`MissingField`(`instance.name`)} |
| CORE-360.3 | `dev2_sim()` / `instance.config_source` | 恰好 {`MissingField`(`instance.config_source`)} |
| CORE-360.4 | `dev2_sim()` / `paths.journal_ram` | 恰好 {`MissingField`(`paths.journal_ram`)} |
| CORE-360.5 | `dev2_sim()` / `paths.journal_ram_raw` | 恰好 {`MissingField`(`paths.journal_ram_raw`)} |
| CORE-360.6 | `dev2_sim()` / `paths.journal_cold` | 恰好 {`MissingField`(`paths.journal_cold`)} |
| CORE-360.7 | `dev2_sim()` / `paths.ctl_socket` | 恰好 {`MissingField`(`paths.ctl_socket`)} |
| CORE-360.8 | `dev2_sim()` / `paths.heartbeat_shm` | 恰好 {`MissingField`(`paths.heartbeat_shm`)} |
| CORE-360.9 | `dev2_sim()` / `blobs.refdata` | 恰好 {`MissingField`(`blobs.refdata`)} |
| CORE-360.10 | `dev2_sim()` / `blobs.strategy_params` | 恰好 {`MissingField`(`blobs.strategy_params`)} |
| CORE-360.11 | `dev2_sim()` / `blobs.risk_params` | 恰好 {`MissingField`(`blobs.risk_params`)} |
| CORE-360.12 | `dev2_sim()` / `blobs.engine` | 恰好 {`MissingField`(`blobs.engine`)} |
| CORE-360.13 | `dev2_sim()` / `host.class` | 恰好 {`MissingField`(`host.class`)} |
| CORE-360.14 | `dev2_sim()` / `host.mem_budget_mib` | 恰好 {`MissingField`(`host.mem_budget_mib`)} |
| CORE-360.15 | `c32_prod()` / `host.irq_cpus` | 恰好 {`MissingField`(`host.irq_cpus`)} |
| CORE-360.16 | `dev2_sim()` / 全部 `[[threads]]` | 恰好 {`MissingField`(`threads`)} |
| CORE-360.17 | `dev2_sim()` / `venues.okx-spot.protocol` | 恰好 {`MissingField`(`venues.okx-spot.protocol`)} |
| CORE-360.18 | `dev2_sim()` / `venues.okx-spot.optional` | 恰好 {`MissingField`(`venues.okx-spot.optional`)} |
| CORE-360.19 | `dev2_sim()` / `venues.okx-spot.feed.conns` | 恰好 {`MissingField`(`venues.okx-spot.feed.conns`)} |
| CORE-360.20 | `dev2_sim()` / `venues.okx-spot.max_sub_insts` | 恰好 {`MissingField`(`venues.okx-spot.max_sub_insts`)} |
| CORE-360.21 | `dev2_prod()` / `venues.binance-usdm.gw.accounts` | 恰好 {`MissingField`(`venues.binance-usdm.gw.accounts`)} |
| CORE-360.22 | `dev2_prod()` / `venues.binance-usdm.gw.src_ip` | 恰好 {`MissingField`(`venues.binance-usdm.gw.src_ip`)} |
| CORE-360.23 | `dev2_prod()` / `bn-1` 的 `[[accounts]].deadman` | 恰好 {`MissingField`(`accounts.bn-1.deadman`)} |
| CORE-360.24 | `dev2_sim()` / `startup.allow_missing_standby` | 恰好 {`MissingField`(`startup.allow_missing_standby`)} |
| CORE-360.25 | `dev2_sim()` / `host.on_mismatch` | 恰好 {`MissingField`(`host.on_mismatch`)} |
| CORE-360.26 | `dev2_sim()` / `host.hot_cpus` | 恰好 {`MissingField`(`host.hot_cpus`)} |
| CORE-360.27 | `dev2_sim()` / `host.hk_cpus` | 恰好 {`MissingField`(`host.hk_cpus`)} |
| CORE-360.28 | `dev2_sim()` / `host.expect_profile` | 恰好 {`MissingField`(`host.expect_profile`)} |
| CORE-360.29 | `dev2_sim()` / `advisor.mem_budget_mib` | 恰好 {`MissingField`(`advisor.mem_budget_mib`)} |
| CORE-360.30 | `dev2_sim()` / 同时删去 `paths.journal_ram`、`paths.journal_ram_raw`、`paths.journal_cold`、`paths.ctl_socket`、`paths.heartbeat_shm`、`shutdown.cancel_open_orders`、`host.on_mismatch` | 恰好这 7 个键各一条 `MissingField` |

### CORE-361 可选字段的默认值

- 规则:[配置文件字段](README.md#配置文件字段)
- 级别:table
- 里程碑:M3
- 前置:`dev2_sim()` 中不写下列键;`config::resolved(&cfg)`
- 实现:`apps/hft-core/tests/defaults.rs::core_361_defaults`

| 行 | 键 | 期望默认值 |
|---|---|---|
| CORE-361.1 | `instance.feed_env` | `prod` |
| CORE-361.2 | `paths.hostprofile` | `/run/hft/hostprofile.json` |
| CORE-361.3 | `host.egress` | `/etc/hft/egress.toml` |
| CORE-361.4 | `advisor.checkpoint_dir`(`paths.journal_cold = /data/hft`) | `/data/hft/sim/advisor` |
| CORE-361.5 | `advisor.model_checkpoint_ms` | 600000 |
| CORE-361.6 | `advisor.checkpoint_max_mib` | 1024 |
| CORE-361.7 | `advisor.checkpoint_chunk_kib` | 1024 |
| CORE-361.8 | `advisor.advisor_max_lag_ms` | 5000 |
| CORE-361.9 | `storage.persist_lag_block_ms` / `.persist_lag_drain_ms` | 1000 / 10000 |
| CORE-361.10 | `storage.disk_free_block_pct` / `.disk_free_drain_pct` | 15 / 5 |
| CORE-361.11 | `startup.budget_s` / `startup.gate_timeout_s` | 30 / 15 |
| CORE-361.12 | `startup.max_clock_offset_us` | 500 |
| CORE-361.13 | `startup.poison_crash_limit` | 2 |
| CORE-361.14 | `shutdown.drain_timeout_ms` | 8000 |
| CORE-361.15 | `watchdog.hb_stale_ms` | 1000 |
| CORE-361.16 | `chain.enabled` / `chain.session_timeout_ms` | false / 300 |
| CORE-361.17 | `paths.ctl_principals` | `/etc/hft/ctl-principals.toml` |

### CORE-362 存储档位的门限边界

- 规则:[配置文件字段](README.md#配置文件字段)、[实现说明](README.md#实现说明)
- 级别:table
- 里程碑:M5
- 前置:`storage.persist_lag_block_ms = 1000`、`.persist_lag_drain_ms = 10000`、`.disk_free_block_pct = 15`、`.disk_free_drain_pct = 5`;冷盘卷容量 1,000 GiB;`housekeeping::storage_levels(lag_ms, free_bytes, total_bytes)` 纯函数
- 实现:`apps/hft-core/tests/housekeeping.rs::core_362_storage_levels`

| 行 | persist 滞后 / 可用空间 | 期望 (persist 档, 磁盘档) |
|---|---|---|
| CORE-362.1 | 999 ms / 500 GiB | (0, 0) |
| CORE-362.2 | 1000 ms / 500 GiB | (1, 0) |
| CORE-362.3 | 9999 ms / 500 GiB | (1, 0) |
| CORE-362.4 | 10000 ms / 500 GiB | (2, 0) |
| CORE-362.5 | 0 / 150 GiB(= 15%) | (0, 0) |
| CORE-362.6 | 0 / 150 GiB − 1 B | (0, 1) |
| CORE-362.7 | 0 / 50 GiB(= 5%) | (0, 1) |
| CORE-362.8 | 0 / 50 GiB − 1 B | (0, 2) |

### CORE-363 存储档位只在变化时经 `hk` 环写成 Input

- 规则:[实现说明](README.md#实现说明)
- 级别:unit
- 里程碑:M5
- 前置:门限同 CORE-362;housekeeping 依次采到 persist 滞后 500、1200、1300、600 ms(可用空间 500 GiB)
- 步骤:
  1. 驱动四次采样
- 期望:
  - `hk` 环依次收到恰好两条:`PersistLag` 档位 1(第 2 次采样)、`StorageOk`(第 4 次);第 3 次采样不产生消息;`LiveDriver` 把它们写成 `Timer{Sys, PersistLag}` 与 `Timer{Sys, StorageOk}` 两条 Input
- 实现:`apps/hft-core/tests/housekeeping.rs::core_363_storage_level_changes_only`

### CORE-364 `venues.<v>.protocol` 的取值

- 规则:[配置文件字段](README.md#配置文件字段)、[实现说明](README.md#实现说明)
- 级别:table
- 里程碑:M3
- 前置:默认 feature 构建;`config::load_and_validate` 只看该键的错误
- 实现:`apps/hft-core/tests/config_fields.rs::core_364_venue_protocol`

| 行 | 取值 | 期望 |
|---|---|---|
| CORE-364.1 | `binance-usdm`、`binance-spot`、`okx-swap`、`okx-spot`、`bitget-usdt-futures`、`bitget-spot` 各一 | 无该键错误;`VenueSet` 变体分别为 `BinanceUsdm`、`BinanceSpot`、`OkxSwap`、`OkxSpot`、`BitgetUsdtFutures`、`BitgetSpot` |
| CORE-364.2 | `binance` | `BadValue` |
| CORE-364.3 | `gate-futures`(未启用该 feature) | `UnknownVenue` |

### CORE-365 `feed.conns` 的 `stream` 只允许 `bbo` 与 `md`

- 规则:[配置文件字段](README.md#配置文件字段)
- 级别:unit
- 里程碑:M3
- 前置:`dev2_sim()` 的 `venues.okx-swap.feed.conns` 加一条 `{stream = "depth", src_ip = <sim IP>}`
- 步骤:
  1. `config::load_and_validate`
- 期望:
  - 恰好一条 `BadValue`,详情含 `stream`
- 实现:`apps/hft-core/tests/config_fields.rs::core_365_feed_stream_values`

### CORE-366 `[sim]` 模型表的两种形式都能解析

- 规则:[配置文件字段](README.md#配置文件字段)
- 级别:unit
- 里程碑:M4
- 前置:`dev2_sim()` 的 `[[sim.latency_models]]` = 两项:`{kind = "Fixed", entry_us = 800, resp_us = 400}`、`{kind = "Empirical", entry = [[5000, 600], [9900, 2500]], resp = [[5000, 300], [9900, 1200]]}`;`[[sim.margin_models]]` = `{mmr = "RefdataTiers", leverage = 3, liq_fee = "0.005", liq_slippage_bp = 20}` 与 `{mmr = { Flat = "0.005" }, leverage = 5, liq_fee = "0.005", liq_slippage_bp = 20}`
- 步骤:
  1. `config::load_and_validate`,取 `SimSetup`
- 期望:
  - 无错误;`SimSetup` 的延迟模型表 2 项、保证金模型表 2 项,值与配置相同;`SimAccountCfg.latency_model = 1` 指向 `Empirical` 那一项
- 实现:`apps/hft-core/tests/config_fields.rs::core_366_sim_model_tables`

### CORE-367 `journal.page` 在 tmpfs 主机类与 `sim` 实例只能是 `normal`

- 规则:[配置文件字段](README.md#配置文件字段)
- 级别:table
- 里程碑:M3
- 前置:`config::load_and_validate`
- 实现:`apps/hft-core/tests/config_fields.rs::core_367_journal_page`

| 行 | 配置 | 期望错误列表 |
|---|---|---|
| CORE-367.1 | `dev2_sim()`,`journal.page = "huge"` | 恰好 {`BadValue`} |
| CORE-367.2 | `c32_sim()`,`journal.page = "huge"` | 恰好 {`BadValue`} |
| CORE-367.3 | `c32_prod()`,`journal.page = "huge"` | 空 |

### CORE-380 启动状态机的边与退出码

- 规则:[实现说明](README.md#实现说明)、[启动顺序](README.md#启动顺序)
- 级别:table
- 里程碑:M3
- 前置:`startup::Machine` 的迁移函数 `next(state, event)`
- 实现:`apps/hft-core/tests/startup.rs::core_380_state_machine`

| 行 | 状态 / 事件 | 期望 |
|---|---|---|
| CORE-380.1 | `Load` / 成功 | `Validate` |
| CORE-380.2 | `Load` / 失败 | `Exit(2)` |
| CORE-380.3 | `Validate` / 装配错误 | `Exit(2)` |
| CORE-380.4 | `Validate` / 只有主机错误 | `Exit(3)` |
| CORE-380.5 | `Allocate` / 预算、tmpfs 或大页不足 | `Exit(3)` |
| CORE-380.6 | `JournalRecover` / 失败 | `Exit(4)` |
| CORE-380.7 | `StateRestore` / 快照不可解或 `VersionNeedsDrain` | `Exit(4)` |
| CORE-380.8 | `StateRestore` / `CapacityShrink` 或 `BindingChangedWithPosition` | `Exit(2)` |
| CORE-380.9 | `StateRestore` / 成功 | `Warmup`(发 `READY=1`) |
| CORE-380.10 | `Warmup` / 全部账户闸门通过 | `Running` |
| CORE-380.11 | `Warmup` / Drain | `Draining` |
| CORE-380.12 | `Running` / 闸门条件失效 | `Running` |
| CORE-380.13 | `Running` / Drain(SIGTERM、命令、`IdentityMismatch`、`StorageCritical`) | `Draining` |
| CORE-380.14 | `Draining` / `Done` 且段已封存、persist 刷完 | `Stopped`,按 cause 退出 |
| CORE-380.15 | `Validate` / 全部账户闸门通过 | 非法迁移(`Err(IllegalTransition)`) |
| CORE-380.16 | `Stopped` / 任意 | 非法迁移 |

### CORE-381 主线程屏蔽信号,SIGTERM 只经 housekeeping 的 signalfd 处理

- 规则:[实现说明](README.md#实现说明)
- 级别:proc
- 里程碑:M3
- 前置:`dev2_sim()` 到 `Running`
- 步骤:
  1. 读每个线程 `/proc/<pid>/task/<tid>/status` 的 `SigBlk`
  2. 用 `tgkill` 把 SIGTERM 发给主线程
- 期望:
  - 主线程 `SigBlk` 含 SIGTERM、SIGINT、SIGHUP;housekeeping 线程有指向 signalfd 的 fd
  - 步骤 2 后进程不被默认动作终止,而是写出 `ControlCmd{Drain}`(issuer `cli:systemd`)并以 0 退出
- 实现:`apps/hft-core/tests/signals.rs::core_381_main_thread_masks_signals`

### CORE-382 housekeeping 每 1000 ms 一条 `ClockStatus`

- 规则:[实现说明](README.md#实现说明)、[线程计划](README.md#线程计划)
- 级别:unit
- 里程碑:M3
- 前置:假时钟驱动 housekeeping 任务
- 步骤:
  1. 推进 10,000 ms
- 期望:
  - 恰好 10 条 `ClockStatus`,相邻两条的单调时钟间隔都是 1000 ms(判定变化时的 `HkMsg::Sys` 见 CORE-418)
- 实现:`apps/hft-core/tests/housekeeping.rs::core_382_clock_status_period`

### CORE-383 Sim 账户的订单从不进入任何 `out-<venue>`

- 规则:[实现说明](README.md#实现说明)
- 级别:prop
- 里程碑:M4
- 性质:`sim` 实例中任意订单序列执行后,每条 `GwCmd` 都在 `out-sim`;`out-<venue>` 环不存在;sim-gw 的每条回报都作为 Input 进 journal
- 生成:`dev2_sim()` 两个 Sim 账户,随机 Place/Cancel/Amend 1–500 条,随机行情
- 参考:断言环名集合与 journal 中回报 Input 数 = sim-gw 发出的回报数
- 例数:PR 200
- 实现:`apps/hft-core/tests/sim_gw.rs::core_383_sim_orders_never_leave`

### CORE-384 sim-gw 线程暂停 10 s 后恢复,回报照常交付、不丢

- 规则:[实现说明](README.md#实现说明)
- 级别:integ
- 里程碑:M4
- 前置:`dev2_sim()`;`sim-a` 有 20 张挂单将被行情成交
- 步骤:
  1. 测试钩子暂停 `sim` 线程 10 s,其间行情推动成交;恢复后运行 5 s
- 期望:
  - 恢复后 5 s 内 journal 收到 20 张单的全部成交回报;每张单的成交量之和 = 模拟撮合的成交量;没有重复回报
- 实现:`apps/hft-core/tests/sim_gw.rs::core_384_pause_resume_no_loss`

### CORE-385 预热注入分块进入 Model,journal 回放一致

- 规则:[实现说明](README.md#实现说明)
- 级别:integ
- 里程碑:M4
- 前置:`dev2_sim()` 绑 `portfolio-carry`,Model 为空(`Note{ModelCheckpointDiscarded{Missing}}`);pipeline 以 Trader 身份按块发 `InjectWarmup`(每块 ≤ 4096 B,共 50 块,最后一块 `last = true`)
- 步骤:
  1. 发全部块并逐条确认
  2. 对 journal 运行 `replayer verify`
- 期望:
  - 50 条 `ControlCmd{InjectWarmup}` Input 按 `chunk_no` 0–49 依次在 journal 中;Model 收到 50 次 `EventRef::Warmup`,策略报告就绪
  - 步骤 2 退出 0
- 实现:`apps/hft-core/tests/advisor.rs::core_385_inject_warmup_chunks`

### CORE-386 步 6 之后热线程不再 `mmap`/`malloc`

- 规则:[实现说明](README.md#实现说明)、[延迟与资源契约](README.md#延迟与资源契约)
- 级别:proc
- 里程碑:M3
- 前置:`dev2_prod()` 连假交易所;测试构建的全局分配器按线程计数;`strace -f -e trace=mmap,munmap,brk` 附着
- 步骤:
  1. 到 `Running` 后满速行情与随机下单 60 s
- 期望:
  - 这 60 s 内 `seq`、`feed`、`gw-*` 线程的分配计数增量为 0,strace 中这些线程没有 `mmap`/`brk`;`adv` 与 `sim`(另以 `dev2_sim()` 运行)线程不受此限
- 实现:`apps/hft-core/tests/startup.rs::core_386_no_alloc_after_step6`

### CORE-387 venue 接线:有 Live 账户的 venue 才挂 `AcctFetchTask` 与账户环

- 规则:[实现说明](README.md#实现说明)
- 级别:unit
- 里程碑:M3
- 前置:`dev2_prod()`(下单 venue 为 `binance-usdm`、`okx-swap`)
- 步骤:
  1. 调用 `venues::wire(&cfg)`,取 `acct-fetch` 与 `snap-fetch` 的子任务列表
- 期望:
  - `acct-fetch` 恰有两个子任务(`binance-usdm`、`okx-swap`);`snap-fetch` 的子任务数 = hft-venue §1 的存在条件对 6 个 venue 求值为真的个数(测试调用同一判定函数)
- 实现:`apps/hft-core/tests/venues.rs::core_387_acct_fetch_wiring`

### CORE-390 hft-core 的依赖闭包里没有异步运行时、遥测与 AWS SDK

- 规则:[依赖](README.md#依赖)
- 级别:unit
- 里程碑:M3
- 前置:`cargo metadata --format-version 1` 对 `hft-core` 的解析结果(默认 feature 与 `chain` feature 各一次)
- 步骤:
  1. 求依赖闭包
- 期望:
  - 闭包中没有 `tokio`、`async-std`、`smol`、`hft-telemetry`、`aws-sdk-*`、`aws-config`;源码中没有 `serde_json::Value`(`rg` 计数为 0);`hft-journal` 启用了 `writer` feature
  - 在测试分支给 hft-core 加 `tokio` 依赖后,本测试与 `cargo xtask ci` 都失败
- 实现:`apps/hft-core/tests/deps.rs::core_390_dependency_closure`

### CORE-400 冷启动(进程启动 → 全部闸门通过)在升级主机类 < 30 s

- 规则:[延迟与资源契约](README.md#延迟与资源契约)、[启动顺序](README.md#启动顺序)
- 级别:host
- 里程碑:M7
- 环境:统一升级主机(`c32`;`c16` 候选时在探针主机上另跑一次),`prod` 连测试网
- 授权:`[需授权]`
- 步骤:
  1. 冷启动 20 次(每次前干净 Drain),每次读 `Note{StartupStep}`
- 通过标准:每次步 8 结束记录的 `v[2]` < 30,000,000 µs;各步结束与开始的差分别不超过启动顺序表的该步预算(步 1 0.3 s、步 2 0.5 s、步 3 4 s、步 4 3 s、步 5 6 s、步 6 8 s、步 7 6 s)的次数记入证据;dev2 只记录不判定
- 证据:`ops-log/<UTC 日期>-CORE-400.md`(20 次的分步时长)
- 实现:`apps/hft-core/tests/host/CORE-400.sh`

### CORE-401 优雅退出 ≤ 10 s

- 规则:[延迟与资源契约](README.md#延迟与资源契约)、[优雅退出与 kill 语义](README.md#优雅退出与-kill-语义)
- 级别:host
- 里程碑:M7
- 环境:同 CORE-400
- 授权:`[需授权]`
- 步骤:
  1. 运行中 `systemctl stop hft-core@prod` 10 次
- 通过标准:每次 journal 中 `ControlCmd{Drain}` 与末条记录的 `utc_ns` 差 ≤ 10 s
- 证据:`ops-log/<UTC 日期>-CORE-401.md`
- 实现:`apps/hft-core/tests/host/CORE-401.sh`

### CORE-402 advisor 检查点单块编码与写入

- 规则:[延迟与资源契约](README.md#延迟与资源契约)
- 级别:bench
- 里程碑:M4
- 基准名:`core/advisor_ckpt_chunk`
- 实现:`apps/hft-core/benches/advisor_ckpt.rs::core_402_advisor_ckpt_chunk`

### CORE-403 热线程只出现允许的系统调用

- 规则:[延迟与资源契约](README.md#延迟与资源契约)
- 级别:host
- 里程碑:M7
- 环境:同 CORE-400,满订阅与测试网探测单
- 授权:`[需授权]`
- 步骤:
  1. bpftrace `raw_syscalls:sys_enter` 按线程名统计 10 min
- 通过标准:`seq` 线程 0 次;`Spin` 线程没有 `futex`(唤醒)与对 eventfd 的 `write`;其余调用都在 [hot-path-rules T7](../../docs/hot-path-rules.md#线程t) 的允许表内
- 证据:`ops-log/<UTC 日期>-CORE-403.md`
- 实现:`apps/hft-core/tests/host/CORE-403.sh`

### CORE-404 warm 线程发现新数据的延迟 ≤ `park_us`

- 规则:[延迟与资源契约](README.md#延迟与资源契约)、[线程计划](README.md#线程计划)
- 级别:host
- 里程碑:M7
- 环境:同 CORE-400,warm 与 advisor 线程 `Park{300}`
- 授权:`[需授权]`
- 步骤:
  1. 统计快照请求、建连交接、账户查询的入环到取出时间差 1 h
- 通过标准:三类各自的 p99 ≤ 300 µs;最大值记入证据
- 证据:`ops-log/<UTC 日期>-CORE-404.md`
- 实现:`apps/hft-core/tests/host/CORE-404.sh`

### CORE-405 步 3 之后热线程没有缺页

- 规则:[延迟与资源契约](README.md#延迟与资源契约)
- 级别:host
- 里程碑:M7
- 环境:同 CORE-400
- 授权:`[需授权]`
- 步骤:
  1. 步 3 结束后对每个热线程 `perf stat -e page-faults -t <tid>` 1 h
- 通过标准:每个热线程 0 次
- 证据:`ops-log/<UTC 日期>-CORE-405.md`
- 实现:`apps/hft-core/tests/host/CORE-405.sh`

### CORE-406 `sim` 的尾随滞后只记录、进日报

- 规则:[延迟与资源契约](README.md#延迟与资源契约)
- 级别:host
- 里程碑:M4
- 环境:dev2,`sim` 常驻 24 h
- 授权:无(只读)
- 步骤:
  1. 读 watchtower 当日日报
- 通过标准:日报含 `sim` 实例的 `sim_tail_lag_ms` 分位数;`prod` 实例没有该指标
- 证据:`ops-log/<UTC 日期>-CORE-406.md`
- 实现:`apps/hft-core/tests/host/CORE-406.sh`

### CORE-407 门槛 ⑩:统一升级主机上的主机内延迟与同机干扰

- 规则:[延迟与资源契约](README.md#延迟与资源契约)
- 级别:host
- 里程碑:M7
- 环境:统一升级主机,`prod`(`feed_env = "prod"`、`order_env = "testnet"`)与 `sim` 同机,7 天测量窗口
- 授权:`[需授权]`
- 步骤:按 [testing §8.2](../../docs/testing.md#82-延迟验收的测法) 的测法采集与统计
- 通过标准:testing §8.2 的判定结论为通过(各判定分段不劣于硬线;同机干扰判定通过)
- 证据:`ops-log/<UTC 日期>-CORE-407.md`(判定输出与直方图归档位置)
- 实现:`apps/hft-core/tests/host/CORE-407.sh`

### CORE-408 升级主机类 `prod` 的 persist 滞后在 hft-journal 预算内

- 规则:[延迟与资源契约](README.md#延迟与资源契约)
- 级别:host
- 里程碑:M7
- 环境:同 CORE-407
- 授权:`[需授权]`
- 步骤:
  1. 读 7 天的 `journal_persist_lag_ms` 直方图
- 通过标准:p99 ≤ [hft-journal 延迟与资源契约](../../crates/hft-journal/README.md#延迟与资源契约) 的预算
- 证据:`ops-log/<UTC 日期>-CORE-408.md`
- 实现:`apps/hft-core/tests/host/CORE-408.sh`

### CORE-409 hk 环在步 5 之前只有主线程写,之后只有 housekeeping 写

- 规则:[实现说明](README.md#实现说明)、[启动顺序](README.md#启动顺序)
- 级别:integ
- 里程碑:M3
- 前置:`dev2_prod()` 连假交易所;测试构建的 hk 环生产端记录每次 push 的线程名;`advisor.advisor_max_lag_ms = 5000`,advisor 被测试钩子阻塞 6 s(引擎时间)使其写 `AdvisorLag`
- 步骤:
  1. 启动,等全部账户闸门通过;再运行 10 s
- 期望:
  - `READY=1` 之前 hk 环的全部 push 来自主线程,其中含步 1–5 的 `Note{StartupStep}` 与每个通过账户一条 `HkMsg::Sys`(`KillSelfTestOk`);`READY=1` 之后的全部 push 来自 `hk` 线程;没有任何时刻两个线程都 push 过
  - `Note{AdvisorLag}` 经 `advice` 环到达(hk 环的 push 记录里没有它)
  - 步 6–8 的 `Note{StartupStep}` 不出现在 hk 环的 push 记录里,journal 中仍各有开始与结束两条
- 实现:`apps/hft-core/tests/startup.rs::core_409_hk_ring_single_writer`

### CORE-410 `RefData::warnings()` 的每一项在启动时写一条 `Note{RefdataWarning}`(P1)

- 规则:[启动顺序](README.md#启动顺序)
- 级别:proc
- 里程碑:M3
- 前置:`c32_prod()`,`order_env = "testnet"`;行 CORE-225.9 的 refdata(两个非正式账户在同一交易所的两个 `VenueId` 上 UID 相同,`warnings()` 恰好 1 项)
- 步骤:
  1. 启动,等步 5 结束
  2. 换成 `warnings()` 为空的 refdata 再启动一次
- 期望:
  - 步骤 1:journal 中恰好一条 `Note{RefdataWarning}`(P1),位于步 4 开始记录之前;进程继续启动(无退出)
  - 步骤 2:journal 中没有 `Note{RefdataWarning}`
- 实现:`apps/hft-core/tests/startup.rs::core_410_refdata_warning_note`

### CORE-411 停机标记损坏时 `Note{StopMarkerCorrupt}` 先于 `Kill{Global}`

- 规则:[启动顺序](README.md#启动顺序)
- 级别:proc
- 里程碑:M3
- 前置:`dev2_prod()`;`stop-marker.json` 为 0 字节文件
- 步骤:
  1. 启动到步 5 结束
- 期望:
  - `EngineClock` 之后依次为 Meta `Note{StopMarkerCorrupt}`(P0)与 Input `ControlCmd{Kill{Global, hard = false, flatten = false}}`(issuer `cli:systemd`);文件随后按 CORE-274 改名为 `.consumed`
- 实现:`apps/hft-core/tests/stop_marker.rs::core_411_corrupt_marker_note`

### CORE-412 新增的装配错误码与退出码 2

- 规则:[命令行与退出码](README.md#命令行与退出码)、[内存与 journal 段(按主机类唯一归属)](README.md#内存与-journal-段按主机类唯一归属)
- 级别:table
- 里程碑:M3
- 前置:`dev2_prod()`;`config::load_and_validate` 与装配(不连接);`venues.binance-usdm.gw.id_tomb_ms = 120000`;引擎定时器需求 `engine_timer_demand` 由测试构建的 hft-sequencer 函数算出,记为 W
- 实现:`apps/hft-core/tests/assembly.rs::core_412_assembly_error_names`

| 行 | 改动 | 期望 |
|---|---|---|
| CORE-412.1 | OMS `tombstone_keep_ms = 120000` | 无错误 |
| CORE-412.2 | OMS `tombstone_keep_ms = 119999` | 恰好 {`TombstoneKeep`},详情含 `binance-usdm`;退出 2 |
| CORE-412.3 | OMS `tombstone_keep_ms = 120001` | 无错误 |
| CORE-412.4 | `max_timers = W` | 恰好 {`TimerCapacity`};退出 2(须严格大于) |
| CORE-412.5 | `max_timers = W − 1` | 恰好 {`TimerCapacity`};退出 2 |
| CORE-412.6 | `max_timers = W + 1` | 无错误 |
| CORE-412.7 | `ExecConfig` 使 `Exec::new` 返回 `ExecConfigError::ZeroLimit` | 恰好 {`ExecConfig`},详情含 `ZeroLimit`;退出 2 |
| CORE-412.8 | `venues.binance-usdm.feed.bbo_conns = 0` | 恰好 {`VenueInit`},详情含 `binance-usdm`、`feed-bbo`/`feed` 任务名与 `Cfg`;退出 2 |
| CORE-412.9 | `paths.ctl_principals` 指向不存在的文件 | 恰好 {`CtlPrincipals`};退出 2 |
| CORE-412.10 | `paths.ctl_principals` 的文件含未知主体 uid 名 | 恰好 {`CtlPrincipals`},详情为 hft-ctl `PrincipalsErr` 的值名;退出 2 |

### CORE-413 签发者表只在启动时加载,运行中改文件不生效

- 规则:[配置文件字段](README.md#配置文件字段)
- 级别:proc
- 里程碑:M3
- 前置:`dev2_prod()` 连假交易所;签发者表中 `cli:alice` 为 Trader;`write_rate_per_min = 12`
- 步骤:
  1. 启动,等闸门通过;`cli:alice` 发 `Annotate{OpsNote}`
  2. 改签发者表删除 `cli:alice`,不重启;`cli:alice` 再发一次
  3. 重启 hft-core;`cli:alice` 再发一次
- 期望:
  - 步骤 1、2 都得到 `Applied`;步骤 3 得到 `unauthorized`(按 hft-ctl 鉴权),journal 中有对应的 `Note{CtlAudit}`
- 实现:`apps/hft-core/tests/ctl_principals.rs::core_413_principals_loaded_at_start`

### CORE-414 生效的 `endpoint_override` 在步 5 写 `Note{EndpointOverride}`(P1)

- 规则:[配置文件字段](README.md#配置文件字段)
- 级别:proc
- 里程碑:M3
- 前置:`dev2_sim()`;行 1 配 `venues.binance-usdm.endpoint_override` 与 `test.extra_ca` 指向假交易所,行 2 不配
- 步骤:
  1. 按行 1 启动到步 5 结束
  2. 按行 2 启动到步 5 结束
  3. `dev2_prod()`(`order_env = "testnet"`)配 `venues.okx-swap.endpoint_override` 与 `test.extra_ca`,启动到步 5 结束
- 期望:
  - 步骤 1:journal 中恰好一条 `Note{EndpointOverride}`(P1,`v[0]` = `binance-usdm` 的 VenueId),启动继续
  - 步骤 2:没有 `Note{EndpointOverride}`
  - 步骤 3:恰好一条 `Note{EndpointOverride}`,`v[0]` = `okx-swap` 的 VenueId
- 实现:`apps/hft-core/tests/sim_instance.rs::core_414_endpoint_override_note`

### CORE-415 `sim` 实例非干净重启后在步 6 之前按快照 seed 模拟交易所

- 规则:[启动顺序](README.md#启动顺序)、[实现说明](README.md#实现说明)
- 级别:integ
- 里程碑:M4
- 前置:`dev2_sim()`;Sim 账户 `sim-a` 初始 USDT 10 000;运行中买入 `BTCUSDT` 永续 0.01 后 SIGKILL;`sim-b` 从未收到真值报告
- 步骤:
  1. 重启,在 sim-gw 启动前截取它的 `SimExchange` 状态
  2. 首次启动(空 journal)时同样截取
- 期望:
  - 步骤 1:`sim-a` 的余额与仓位等于快照节 22、节 1 的值(仓位 0.01),且 `seed_account_from` 在步 5 结束记录之后、sim-gw 线程启动之前被调用恰好一次;`sim-b` 未被 seed,保持 `initial_balances`;`SimSetup.tsc_hz_milli` 等于本次运行 `EngineClock` 的 `tsc_hz_milli`
  - 步骤 2:没有调用 `seed_account_from`
- 实现:`apps/hft-core/tests/sim_instance.rs::core_415_sim_seed_before_step6`

### CORE-416 装配把 `ready_poll_on` 的结果与 `max_sub_insts` 写进每个 `FeedCfg`

- 规则:[配置文件字段](README.md#配置文件字段)、[实现说明](README.md#实现说明)
- 级别:table
- 里程碑:M3
- 前置:装配函数 `venues::feed_cfgs(cfg)`;`venues.binance-usdm.max_sub_insts = 40`;`feed.ready_poll = auto`
- 实现:`apps/hft-core/tests/venues.rs::core_416_feed_cfg_wiring`

| 行 | 所在线程 idle / 该线程计划连接数之和 | 期望 `FeedCfg` |
|---|---|---|
| CORE-416.1 | `Spin` / 3 | `ready = true`,`max_sub_insts = 40` |
| CORE-416.2 | `Spin` / 2 | `ready = false` |
| CORE-416.3 | `SpinThenPark` / 4 | `ready = false` |
| CORE-416.4 | `Spin` / 4,`feed.ready_poll = off` | `ready = false` |

### CORE-417 拆分形态下 live driver 按 `feed_cmd_dest` 推送 `FeedCmd`

- 规则:[实现说明](README.md#实现说明)
- 级别:integ
- 里程碑:M3
- 前置:`c32_prod()`,`binance-usdm` 拆成 `feed-bbo:binance-usdm` 与 `feed-md:binance-usdm`;假交易所
- 步骤:
  1. 让 sequencer 产出 `FeedCmd::Resync{inst}` 一条、`FeedCmd::Subscribe{inst}` 一条
- 期望:
  - 每条出现在的 `feedctl` 环与 `hft_venue::feed_cmd_dest` 对该条的返回一致(`Md` → 只在 `feedctl-binance-usdm`;`Both` → 两个环各一条);其他环中没有它
- 实现:`apps/hft-core/tests/venues.rs::core_417_feedcmd_dest_routing`

### CORE-418 每个校准节拍恰好一条 `ClockStatus`,判定变化时另一条 `HkMsg::Sys`

- 规则:[实现说明](README.md#实现说明)
- 级别:table
- 里程碑:M3
- 前置:假时钟与假 `DriftTracker` 驱动 housekeeping;`rt.clock.step_threshold_ns = 50000`;每行连续 3 个节拍的判定按行
- 实现:`apps/hft-core/tests/housekeeping.rs::core_418_clock_tick_outputs`

| 行 | 3 个节拍的判定 | 期望 hk 环输出 |
|---|---|---|
| CORE-418.1 | Synced、Synced、Synced(之前为 Synced) | 3 条 `ClockStatus`;0 条时钟判定 `HkMsg::Sys` |
| CORE-418.2 | Synced、Stepped、Synced | 3 条 `ClockStatus`(第 2 条 `state = Stepped`);2 条 `HkMsg::Sys`(节拍 2、3) |
| CORE-418.3 | Stepped、Stepped、Unsynced | 3 条 `ClockStatus`;3 条 `HkMsg::Sys` |

### CORE-419 `rt.host.mode` 决定 hard 位是拒绝启动还是只告警

- 规则:[HostProfile](README.md#hostprofile)
- 级别:table
- 里程碑:M3
- 前置:`c32_prod()` 配 `sysroot::c32()` 夹具,夹具使 `check_host` 恰好置一个 hard 位 `ISOLCPUS`;`host.on_mismatch = "degrade"`
- 实现:`apps/hft-core/tests/hostcheck.rs::core_419_host_mode`

| 行 | `rt.host.mode` | 期望 |
|---|---|---|
| CORE-419.1 | `prod` | `HostCheck` 退出 3,详情含 `ISOLCPUS` |
| CORE-419.2 | `dev` | 启动继续;`HostProfile.host_checks` 含 `ISOLCPUS` |
| CORE-419.3 | `prod`,夹具无 hard 位 | 启动继续 |

### CORE-420 装配把实例角色、keys_required 与 allow_missing_standby 交给 sequencer

- 规则:[实现说明](README.md#实现说明)
- 级别:table
- 里程碑:M3
- 前置:`config::load_and_validate` 与装配(不连接);读出装配交给 `Sequencer::boot` 的参数
- 实现:`apps/hft-core/tests/assembly.rs::core_420_sequencer_handoff`

| 行 | 配置 | 期望 |
|---|---|---|
| CORE-420.1 | `dev2_prod()` | 实例角色 `prod`;每个 (Live 账户, `binance-usdm` \| `okx-swap`) 的 `keys_required = 3`(`SESSION \| REST`) |
| CORE-420.2 | `dev2_sim()` | 实例角色 `sim`;每个 Sim 账户的 `keys_required = 0` |
| CORE-420.3 | `dev2_prod()`,`startup.allow_missing_standby = true`,ECF1 段 5 同为 true | `ModeCfg.allow_missing_standby = true` |
| CORE-420.4 | `dev2_prod()`,两处都为 false | `ModeCfg.allow_missing_standby = false` |

## 验收

- 本组件完成的标准:里程碑已到的非 `host`/`testnet`/`drill` 用例全部有同编号的实现且通过([docs/tests §1](../../docs/tests/README.md#1-地位)、§6 的追溯检查)。
- 属性测试的每夜例数:CORE-131 100 000;CORE-191 5 000;CORE-243 100;CORE-326 10 000;CORE-383 5 000。CORE-244(100 次 SIGKILL)每夜运行。
- 基准 `startup-cold`(CORE-267)与 `core/advisor_ckpt_chunk`(CORE-402)按 [benchmarks](../../benchmarks/README.md) 登记的数值在参考机上判定。
- `host`/`drill` 用例的证据按各用例的 `证据` 字段归档:dev2 护栏的演练(CORE-148、CORE-210–CORE-217)与护栏、批处理的主机用例在 M3 末之前完成,其中演练 8(CORE-216)不通过即按 [dev2 护栏](README.md#dev2-护栏) 启用退路 (a);统一升级主机的用例(CORE-077–CORE-079、CORE-089、CORE-142、CORE-170–CORE-176、CORE-233、CORE-279、CORE-400–CORE-408)在 M7 完成。
- 上线门槛的唯一定义在 [testing §8](../../docs/testing.md#8-上线门槛);上线步骤见 [launch](../../docs/runbooks/launch.md)。
