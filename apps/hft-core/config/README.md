# hft-core 可执行配置

这是开发与测试网装配入口。生产主机的完整规格归 [README](../README.md)，当前限制归其“已知限制与后续工作”。配置相对路径均相对于 TOML 所在目录；`deny_unknown_fields` 拒绝未知字段。RDB1、ECF1、风险参数与策略参数文件必须提供 SHA-256，文件内容在启动时进入 seq journal。实际 prod 启动在配置语法解析后、blob 与线程计划校验前读取并删除凭据普通文件；共享路径只读一次，原始缓冲交接后清零。`--check` 与 Sim 不读、不删凭据。

可选策略在构建时选择 `hft-strategy-registry/<feature>`，无需修改 app；名字与 kind 归 [registry](../../../strategies/registry/README.md)。CI 的完整 schema 校验同时选择 app 与 registry 并启用全部 feature；`--version` 列出该制品实际包含的策略。

离线演示不需要密钥或交易所。CI 构建 `hft-core` 与 `paper_config` example 后，将制品带到 Linux；`paper_config OUTPUT_DIR TMPFS_DIR USER` 生成完整 TOML、四个二进制配置及仅允许当前用户的 ctl principal 表。例如先运行 `paper_config /tmp/hft-paper /dev/shm/hft-paper "$USER"`，再运行 `hft-core --config /tmp/hft-paper/core.toml --dry-run --check`，最后运行 `hft-core --config /tmp/hft-paper/core.toml --dry-run --run-for-ms 10000`。每次使用新目录；保留旧 journal，不能把删除日志当作恢复。

Polymarket 配置由生成器 [hft-pmcfg](../../../tools/hft-pmcfg/README.md) 写出(命令行与多账户的 `--account` 归它;`--features polymarket` 的 example `pm_sim_config` 是同一命令行的薄壳)。Polymarket 行情模拟(构建 `--features polymarket` 的 `hft-core`):`hft-pmcfg OUTPUT_DIR TMPFS_DIR SRC_IP BIND_DEVICE [USER] [PIPELINE_USER]` 生成 `--dry-run` 配置,唯一的 venue `pm` 为 `polymarket-clob` 且 `live_feed = true`(公共行情、订单只进 sim-gw、无密钥),refdata 含 4 个币 × 2 个窗口 × Up/Down 共 16 个槽位 inst(`btc-up-0` 等,tick 0.001),Sim 账户 `sim-pm` 绑定 book-observer,每个槽位一个单腿 group。`USER` 为 Admin 的 cli 主体,`PIPELINE_USER` 是发现进程运行用的系统用户(pipeline 主体,发 `BindSlot`)。槽位在绑定前不订阅;绑定由发现进程按窗口写入(见 [pm-md-probe](../../../tools/pm-md-probe/README.md))。

Polymarket 实盘(`hft-pmcfg --live UID KEYS_DIR [--day-loss-carry YYYY-MM-DD USDC] [--daily-loss USDC] [--daily-loss-resumes N] ...`):账户 `pm-live`,风控单笔 10、账户与 venue 55、slot group 25、当日亏损 15(USDC;`daily_loss_kill = 1.0`,额度恒为 `daily_loss_abs`)。`--daily-loss USDC`(十进制,> 0,至多 6 位小数,如 `25`、`7.5`)把 `daily_loss_abs` 改为该值;`--daily-loss-resumes N`(0–20)设该账户的 `daily_loss_max_resumes`(每 UTC 日 `Resume{DailyLoss}` 的次数,每次按 `day_pnl − 额度` 重设触发线,语义归 [hft-risk 当日亏损](../../../crates/hft-risk/README.md)),缺省取 hft-risk 默认 1。这三个选项只用于 `--live`:无 `--live` 或与 `--maker` 同用时生成器拒绝;取值不合法时拒绝,不生成配置。Polymarket 只以 USDC 计价且没有 USDC/USDT 参考簿,ledger 配置把 USDC 列入 `par_assets`(按 1.0 折 USDT);否则 USDC 金额全部折成 0,当日亏损与持有名义上限失效,ledger 加载时拒绝([hft-ledger USDT 统一计价](../../../crates/hft-ledger/README.md#usdt-统一计价))。每次启动用新目录即新 journal,当日 PnL 从 0 起;同一 UTC 日重启时用 `--day-loss-carry` 写入此前各次运行当日已实现的合计 PnL(< 0,USDC,取交易所成交与结算的外部账;盈利不填),它进入 ECF1 段 8,在新 journal 首次日切时计入当日账,低于 −额度(缺省 −15)即 `DailyLoss`([hft-sequencer](../../../crates/hft-sequencer/README.md#装配与运行由-appshft-coreappsreplayerhft-testkit-调用))。从同一 journal 的快照恢复时不需要也不应填写:当日 PnL 与 `DailyLoss` 锁存随快照保留。

binary-maker 参数值(`hft-pmcfg --maker …` 或 `--live …` 加可重复的 `--param NAME=RAW`):在 schema 默认值(实盘另在 `micro_s2_c5_cap10_mc20` 参数)之后把字段 `NAME` 设为原始值 `RAW`(schema 的原始单位:`Rate` 为 1e-9、时长为毫秒、份数为整数;如 `--param pair_margin=-20000000`);字段名不在 binary-maker 的 schema 中、`RAW` 不是整数或超出该字段的 `[min, max]` 时拒绝,不生成配置;没有 `--maker` 或 `--live` 时拒绝。

引擎初始为 Shadow。等待 journal 的 ClockOk 与启动闸门通过后，通过已有 ctl 客户端对 `paper` 账户、`paper-probe` 实例发送 `SetMode Live`，按正常 preview/confirm 流程确认。探针收到固定 BBO 后发出远价 PostOnly，收到 Ack 后撤单；集成测试自动完成这一流程。未启用 Live 的 `--run-for-ms` 演示只验证启动与 Drain。模拟器每账户的连接回报展开为其配置 venue 集合的单主连接，ECF1 必须启用 `allow_missing_standby`；网络模式不替账户生成连接或身份报告。

`[[threads]]` 在此子集使用 `name`、`task`、可选 `core`、必填 `park_us` 与 `nice`。`nice` 为 0–19，housekeeping 必须为 0；模板的 persist 与控制/建连线程为 10。每个线程在执行任务前设置 `SCHED_OTHER`、`timerslack_ns = 1`、nice、名称及可选绑核；设置失败退出 3，所有工作线程的设置成功后才允许 READY。任务为 `sequencer`、`persist`、`housekeeping`、`ctl-gateway`；离线另需 `paper-feed`、`sim-gw`，网络模式每 venue 另需 `feed:<配置名>`、`conn-builder:<配置名>`、有账户时的 `gw:<配置名>`。每个任务恰好一个独占线程；空闲 park 位于任务轮转外。

`clock.step_threshold_ns`、`clock.interval_ms`、`watchdog.hb_stale_ms` 与 `venues.<v>.stale_guard_ms` 均为必填正整数;`clock.anchor_interval_ms`(`UtcAnchor` 重锚周期,按 `interval_ms` 节拍向上取整)可选,缺省 10 000,必须为正。线程名不超过 Linux comm 的 15 字节；park 时长遵守 [心跳约束](../README.md#线程计划)。网关订单槽位容量从 ECF1 的 OMS 配置取得。缺失键逐项报 `MissingField`，凭据引用缺失报 `MissingCredential`，未知键与类型错误报单条脱敏 `ConfigParse`。加载还检查 [执行/风险与尘埃线约束](../README.md#内存与-journal-段按主机类唯一归属)，失败发生在运行资源创建前。

`threads.idle` 可选 `"park"`（默认，保留 `park_us`）或 `"spin"`。Spin 只允许 sequencer/feed/gw 热线程，必须显式绑定且无其他配置线程共用该核；启动还要求该核位于 Linux isolated CPU 列表，否则在 READY 前退出 3。低延迟主机可使用用户态持续轮询，dev2 模板保持 Park；这不启用网卡/socket 的内核 busy poll。

`binance-testnet.toml` 是网络模式模板，使用 `prod/testnet` 和非正式 Live 账户。先生成该账户的 RDB1、ECF1、风险与 registry 参数文件并填入哈希；模板中的零哈希故意不能通过校验。RDB1 的 Testnet endpoints 应包含下列地址，不能把浏览器网站 `demo.binance.com` 当作 API URL：

| EndpointRole | URL |
|---|---|
| Rest | `https://demo-fapi.binance.com` |
| MdPublic | `wss://demo-fstream.binance.com/public/ws` |
| MdMarket | `wss://demo-fstream.binance.com/market/ws` |
| OrderWs | `wss://testnet.binancefuture.com/ws-fapi/v1` |
| UserData | `wss://demo-fstream.binance.com/ws`，动态 listenKey 需 bootstrap |

来源：[Binance REST/市场流测试网说明](https://developers.binance.me/en/docs/products/derivatives-trading-usds-futures/general-info)、[官方 USDT-M SDK](https://github.com/binance/binance-connector-js/tree/master/clients/derivatives-trading-usds-futures)。路径由 `venue-binance` 的 route 表生成。REST 密钥和 Ed25519 session 密钥分别配置 API key 文件；Ed25519 支持原始 32 字节、PKCS#8 DER/PEM，HMAC secret 为 ASCII。文件不入库，日志不回显解析失败的 TOML 或凭据内容。`src_ip` 与 `bind_device` 必须按测试机实际出口填写。

账户链路、同进程装配与限制见 [测试网 runbook](../RUNBOOK-testnet.md)。Bitget 默认启用，模板为 [bitget-demo.toml](bitget-demo.toml)，HMAC 文件用 `session_seed_path` 并提供 `passphrase_path`。

`instance.feed_env = "prod"` 选择 RDB1 Prod 的 `MdPublic`、`MdMarket` 和公开快照 `Rest`；这三项必须齐全且属于对应协议的公共主机白名单。省略时当前单任务线程子集保持 `testnet`。生产环境表禁止 `OrderWs`/`UserData` 等私有角色；账户 bootstrap、签名 REST、网关与私有流始终选择 Testnet 表，`local_demo` 对这些地址继续逐项验证 demo 白名单。公共连接不取得凭据，两个环境的 journal 数据按同次配置记录，不把 demo 回报 RTT 当主网 RTT。

[dual-testnet.toml](dual-testnet.toml) 装配 `binance-usdm`（VenueId 1）与 `bitget-spot`（VenueId 11），对应 RDB1 的 ETHUSDT 永续与现货。两者共用逻辑账户号，分别持有各所非正式账户凭据；现货账户禁用借币。RDB1 的 Bitget Testnet 端点为 Rest `https://api.bitget.com`、公共流 `wss://wspap.bitget.com/v3/ws/public`、私有/订单流 `wss://wspap.bitget.com/v3/ws/private`，demo 标记由协议生成。参数、资金上限与选币证据归 [demo 配置](../../../deploy/testnet/README.md)；零哈希与示例出口地址必须替换后再校验。

`journal.segment_bytes` 与 `journal.hot_keep` 为 seq 的配置；可选的 `journal.raw_feed`、`journal.raw_gw` 各含必填的 `segment_bytes`、`hot_keep`，缺省时沿用 seq 配置。raw 允许零保留，尺寸须页对齐且在 2–256 MiB 内。双 venue 模板按 [dev2 journal 表](../README.md#内存与-journal-段按主机类唯一归属)分开配置，实际预算与打开 writer 使用同一布局；容量检查读取实例子目录的文件系统，支持每实例独立 tmpfs 挂载。

可选 `[startup]` 同时提供正整数 `budget_s` 与 `gate_timeout_s`；省略时沿用 [启动顺序](../README.md#启动顺序) 的预算。它们只控制 StartupStep 的超时标志，不能放行账户闸门。`--check` 不发送 STATUS。
