# Binance / Bitget 测试环境

此入口只支持非正式 Live 账户、新 journal、Linux。旧 journal 返回 `RecoveryRequired`，保留日志，不能删除后假装恢复。真实 demo 的行情/登录/成交字段仍需验证；mock 测试不是交易所验收证据。

1. 从 GitHub CI 下载同一提交的 aarch64 制品，核对 SHA-256 与 RELEASE.json。构建只在 GitHub hosted runner：`cargo build -p hft-core -p hft-strategy-registry --all-features --target aarch64-unknown-linux-gnu`；服务器不编译。先用 `hft-core --version` 核对所需 kind_id，`hft-core schemas --out schemas` 导出同一制品的 schema。策略仅由 registry 提供；未出现的 kind 不能靠配置启用。
2. 选 [Binance](config/binance-testnet.toml)、[Bitget](config/bitget-demo.toml) 或 [双 venue](config/dual-testnet.toml) 模板。RDB1 必须有对应 testnet endpoints、交易规则和非正式 Live 账户绑定；两家 venue 可共用一个逻辑 AccountId，但交易所身份分别登记。ECF1 配置 feeds/gws、OMS 容量、身份双 key 闸门和 thread plan；风险与策略 blob 按导出 schema 生成。所有 blob 的真实 SHA-256 填入 TOML；零哈希占位不能启动。参数与两腿例子见 [部署示例](../../deploy/testnet/README.md)。
3. Binance Demo Ed25519 私钥使用原始 32 字节或 PKCS#8 DER/PEM 文件。Windows 原始 key 位于 `E:\secrets\binance-testnet\`，通过机密部署流程安装到 Linux `/run/secrets/`，不复制进仓库、journal 或终端输出。Demo USDM accountConfig 无 UID 时只能显式使用非正式 Unverifiable，并让 REST/WS 指向相同 API key/私钥；`rest_sign_alg = "ed25519"`。完整 Verified 跨主机 UID/提现权限核验不在该入口，不使用正式 key。
4. Bitget 使用 UTA demo API key、HMAC secret 和 passphrase 文件；session_seed_path 在此是 HMAC secret 的路径。只授权交易和账户读取，不授权提现。账户为 basic unified、单向、全仓，杠杆在 refdata 上限以内且无债务。REST 自动带 `paptrading: 1`，WS 使用 demo private/public 主机。现货每笔显式 `autoBorrow=no`。具体 wire 事实归 [协议说明](../../crates/venue-bitget/README.md)。

   无 IAM 的专用 demo 主机可显式配置 `instance.secrets_mode="local_demo"`，默认仍是 `aws_secrets_manager`。源文件仅经 scp 放入 root 0700 的 `/run/secrets/<目录>/`，以 checksum 核对、不输出内容；manifest 的 `mode`、`name/path` 和运行时目标路径遵循 [deploy 秘密模式](../../deploy/README.md#秘密与实例角色)。不修改生成的 unit 绕过预检，使用发布内的准备器。`prod` 在此是交易所连接的进程角色；`order_env=prod` 或任何 `is_prod=true` 的正式账户都拒绝 local_demo。
5. 填实际 `src_ip` / `bind_device`；RDB1 地址必须都是相应 demo 环境。`hft-core check-config --config core.toml` 与 `--check` 等价，只检查文件，不读取 key、不联网。它不证明交易所规则正确。检查成功后运行 `hft-core --config core.toml`；启动 REST 会撤销该专用账户的全部遗留挂单，任一步失败都不会启动策略任务。
6. 等待配置、身份、连接、时钟、kill 自检和初始对账闸门，保持 Shadow 观察。Unverifiable 的 `Resume` 必须按控制面的身份确认流程带 `ack_identity`；缺闸门不能跳过。用现有 ctl 客户端 preview/confirm 设置 Live。观察 journal 的 SendReport、OrderEvent、余额/持仓差异、FundingMismatch、AccountCfg 与连接状态，以及 stderr 的 heartbeat/clock_synced。周期差异只告警，不覆盖账本；查询失败关闭配置闸门后重试。Binance countdownCancelAll 在启动时设置，之后仅响应 sequencer 的 DeadManRefresh，不由独立 worker 永久续命。
7. 停止用 SIGTERM（或 ctl Drain），停止开仓并撤单，等待退出与冷盘落盘。超时/断连后订单仍视为 Unknown，须到 demo 确认订单；不能把请求失败当作撤单成功。保留 seq/raw journal、退出计数和 CI 提交用于排障。再次启动使用恢复功能前，需人工核对并明确建立新的测试会话。

本地与 CI 的 `account_process` 测试使用独立子进程、脚本 REST 和假 key，覆盖两家协议的撤空、基线、重认证、失败与差异；不访问真实交易所。Linux `paper_process` 验证控制 socket、订单闭环及 SIGTERM。正式上线门槛仍以 [launch](../../docs/runbooks/launch.md) 为准。
