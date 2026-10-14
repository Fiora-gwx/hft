# 双交易所 demo 参数

使用 [hft-core 双 venue 模板](../../apps/hft-core/config/dual-testnet.toml)，Binance USDM 与 Bitget UTA USDT Futures 的 BTCUSDT 分别登记在 refdata。该示例只绑定非正式账户 `demo` 与实例 `demo-spread`；实例 kind_id 为 3。构建使用 registry 的全部 features，先检查 `hft-core --version` 和导出的 schema。

[参数示例](strategy.toml) 显式启用 cross-taker，单腿每笔上限 200 USDT、持仓上限 400 USDT；RiskParams 应独立约束相同或更小的限额。实际 tick、lot 与最低名义须从 demo exchange info 确认；若最小可交易订单超过 cap，应保持禁用并重新评估配置。该值只是可审阅的 demo 配置，不是收益假设。

以 CI 制品运行 `hft-cfg build --kind strategy --schemas schemas --refdata refdata.bin --in strategy.toml --out params.spb1`。将输出文件直接用于 hft-core 的 blobs.strategy_params，并填写其 SHA-256。启动默认 Shadow，启用参数不绕过模式或账户闸门。资金费策略使用集成分支 registry 已登记的实现和导出 schema；不要在 app 中补算法或硬编码其 kind。


构建与安装使用 `release-aarch64.yml` 的 `workflow_dispatch`，选择已通过草稿 PR CI 的 SHA、唯一 `dev_n`，设置 `artifact_only = true`。下载该运行的 `release-dist`，在主机上核对 `.sha256`；`RELEASE.json` 记录提交、CI 证据、CPU 特性、schema 和逐文件摘要。安装器不编译。原生 runner 的 CPU 特性是保守包络，首台 AWS 主机的 preflight 必须通过，不能绕过不匹配。

以 root 准备 `/etc/hft/host-local.toml`、主机清单、测试网配置与 blob、秘密引用、签发者和独立冷卷，按[发布步骤](../../docs/runbooks/release-and-deploy.md)安装归档，再运行 `activate-release.sh --version <版本> --group core`。激活保持交易单元停止；单独启动护栏、IMDS 和 slice 后运行预检。联调只能由 `deploy/dev2/hft-window start` 开窗，`stop` 走正常 Drain。失败保留窗口标志，确认 prod 停止后再关闭窗口。

进程故障或干净退出后都先关闭窗口、人工撤空并核对仓位和余额。当前运行时拒绝旧 journal；归档 seq/raw 段后保留原实例 `meta/engine_epoch` 和 `core.lock`，下一会话使用同一 cold root，禁止删除或回退 epoch。整目录换盘或恢复仍属延期的恢复流程，不能靠新建空目录绕过。
