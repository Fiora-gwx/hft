# AGENTS.md — apps

本子树的规则补充根 [AGENTS.md](../AGENTS.md#常驻规则)。每个 app 的规格在 `apps/<name>/README.md`(模板见 [crates/AGENTS.md](../crates/AGENTS.md#readme-模板),`kind: app`)。

## 只装配

- app 只做:解析配置、校验、建环与共享内存、创建线程并绑核、载入秘密、连接、把库 crate 与策略包接起来。业务逻辑在 crate 里。
- 需要单元测试其不变量的代码不属于 app,搬进对应 crate([什么值得成为 crate](../crates/AGENTS.md#什么值得成为-crate))。

## 策略装配只在这里

- 唯一的策略清单是 [`strategies/registry`](../strategies/registry/README.md);依赖它的只有 `apps/hft-core`、`apps/replayer` 与配置生成器 `tools/hft-pmcfg`,三者由它取得同一个策略集合。需要真实策略的集成测试放 `apps/hft-core/tests/` 或 `strategies/registry/tests/`。新增策略 = 新的 `strategies/<name>` + 注册表一行,全部 app 零改动。
- 其他 app 不依赖任何策略包;需要策略信息时读 journal 里的通用事件与 `hft-core schemas --out <dir>` 导出的 schema JSON。

## 测试规格

- 每个 app 有 `TESTS.md`(格式与编号前缀归 [docs/tests](../docs/tests/README.md));测试函数名以用例编号开头([§6](../docs/tests/README.md#6-与代码的对应)),`cargo xtask test-trace` 核对。多进程用例(`proc`)与需要真实策略的集成测试放 `apps/<name>/tests/`。
- 改行为按 [§7](../docs/tests/README.md#7-修改流程设计基线-v1-之后):同一提交改 README、`TESTS.md` 与代码,并记 [docs/changes.md](../docs/changes.md)。

## 线程与核只在这里

- 线程名、数量、承载的任务、绑定的核与空闲策略只在 app 的 `[[threads]]` 配置与 README 线程计划里出现(按主机类);库 crate 不创建线程,只提供可轮转的任务([architecture.md §3](../docs/architecture.md#3-hft-core-线程))。
- 同一台主机上全部 app 的线程与核、加载校验清单、内存表与 `dev2` 护栏数值都归 [hft-core](hft-core/README.md#线程计划);新增任务或线程先改那里,再改 `deploy/` 的主机清单与生成物。隔离核列表由线程计划生成,不手写。
- 每个下单 venue 的 `gw:<venue>` 独占线程,所有主机类无例外;升级主机类上 `feed-bbo:*` 不与 `feed-md:*` 同线程([hot-path-rules T8](../docs/hot-path-rules.md#线程t))。

## 配置与 fail-loud

- 每个实例一份 TOML,顶层 `schema = <u32>`;解析用 `deny_unknown_fields`。
- hft-core 实例名只允许 `prod`、`sim`:`sim` 只含 Sim 账户、不引用交易所 key;`prod` 不含 Sim 账户;两个实例同机时配置、journal、冷盘目录、ctl socket、slice、网卡与出口 IP 分开([hft-core](hft-core/README.md))。
- 影响资金或安全的字段(账户、账户绑定、账户类型、限额、RunMode 上限、kill 行为)**没有默认值**,缺失即拒绝启动。
- 加载时收集**全部**错误后一次性打印并以退出码 2 退出;主机前置条件不符退出码 3;journal 恢复失败退出码 4。systemd 单元对 2/3/4 设 `RestartPreventExitStatus`。
- 秘密只以 Secrets Manager/KMS 的引用出现在配置里。
- 每个 app 支持 `--check`:只做加载与校验,不连接、不写 journal,退出码同上。

## 与部署单元一一对应

- 每个 app 一个二进制 `bin/<app>`、一个 systemd 单元 `hft-<app>[@<instance>].service`([deploy/](../deploy/README.md)),随同一个不可变发布归档;二进制只由 CI 构建,服务器不编译。
- 激活发布不启动任何单元(冷生产);启动是单独的授权操作([release-and-deploy](../docs/runbooks/release-and-deploy.md))。

## 冷进程不碰热写接口

- `tier: cold` 的 app 读 journal 只用 `hft-journal` 的读者与离线 API,写入热进程只经 `hft-ctl` 客户端;不构造 `JournalWriter`/`Persister`,不依赖 `hft-venue`、`hft-net` 的运行时与 `hft-sequencer`。例外只有 `replayer`:以 `JournalWriter::open_offline` 写普通目录,以回放驱动使用 `hft-sequencer`。
- `hft-core` 的依赖闭包里没有 `tokio`,也不依赖 `hft-telemetry`;optional 的 chain-core 按 cold tier 实现;`cargo xtask ci` 检查以上规则。
- 冷进程日志用 `tracing` → journald;热进程线程内只写定长 `Note` 记录。
