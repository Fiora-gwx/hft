# hft-core 可执行配置

这是开发与测试网装配入口。生产主机的完整规格归 [README](../README.md)，当前限制归其“已知限制与后续工作”。配置相对路径均相对于 TOML 所在目录；`deny_unknown_fields` 拒绝未知字段。RDB1、ECF1、风险参数与策略参数文件必须提供 SHA-256，文件内容在启动时进入 seq journal。凭据只从路径读取，`--check` 不读凭据内容。

可选策略在构建时选择 `hft-strategy-registry/<feature>`，无需修改 app；名字与 kind 归 [registry](../../../strategies/registry/README.md)。CI 的完整 schema 校验同时选择 app 与 registry 并启用全部 feature；`--version` 列出该制品实际包含的策略。

离线演示不需要密钥或交易所。CI 构建 `hft-core` 与 `paper_config` example 后，将制品带到 Linux；`paper_config OUTPUT_DIR TMPFS_DIR USER` 生成完整 TOML、四个二进制配置及仅允许当前用户的 ctl principal 表。例如先运行 `paper_config /tmp/hft-paper /dev/shm/hft-paper "$USER"`，再运行 `hft-core --config /tmp/hft-paper/core.toml --dry-run --check`，最后运行 `hft-core --config /tmp/hft-paper/core.toml --dry-run --run-for-ms 10000`。每次使用新目录；保留旧 journal，不能把删除日志当作恢复。

引擎初始为 Shadow。等待 journal 的 ClockOk 与启动闸门通过后，通过已有 ctl 客户端对 `paper` 账户、`paper-probe` 实例发送 `SetMode Live`，按正常 preview/confirm 流程确认。探针收到固定 BBO 后发出远价 PostOnly，收到 Ack 后撤单；集成测试自动完成这一流程。未启用 Live 的 `--run-for-ms` 演示只验证启动与 Drain。模拟器每账户的连接回报展开为其配置 venue 集合的单主连接，ECF1 必须启用 `allow_missing_standby`；网络模式不替账户生成连接或身份报告。

`[[threads]]` 在此子集使用 `name`、`task`、可选 `core`、必填 `park_us`。任务为 `sequencer`、`persist`、`housekeeping`、`ctl-gateway`；离线另需 `paper-feed`、`sim-gw`，网络模式每 venue 另需 `feed:<配置名>`、`conn-builder:<配置名>`、有账户时的 `gw:<配置名>`。每个任务恰好一个独占线程；空闲 park 位于任务轮转外。

`clock.step_threshold_ns`、`clock.interval_ms`、`watchdog.hb_stale_ms` 与 `venues.<v>.stale_guard_ms` 均为必填正整数。线程名不超过 Linux comm 的 15 字节；park 时长遵守 [心跳约束](../README.md#线程计划)。网关订单槽位容量从 ECF1 的 OMS 配置取得。缺失键逐项报 `MissingField`，未知键与类型错误报单条脱敏 `ConfigParse`。

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
