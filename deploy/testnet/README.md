# 双交易所 demo 参数

使用 [hft-core 双 venue 模板](../../apps/hft-core/config/dual-testnet.toml)，refdata 第 0 腿为 `binance-usdm:ETHUSDT`（USDⓈ-M 永续，VenueId 1），第 1 腿为 `bitget-uta-spot:ETHUSDT`（SPOT，VenueId 11），同 base/USDT。只开空永续、多现货，卖出现货仅关闭所持非 baseline 库存；不配置借币或自动借币。构建使用 registry 的全部 features，先检查 `hft-core --version` 和导出的 schema。

[价差参数](strategy.toml) 绑定非正式账户 `demo`、实例 `demo-spread`（kind_id 3、schema v3），单腿每笔上限 25 USDT、持仓上限 50 USDT，`close_edge = 0`。[资金费参数](carry.toml) 绑定同一账户、实例 `demo-carry`（kind_id 1、schema v2），每腿上限 25 USDT，实际永续杠杆上限 1 倍，预期持有期基差成本暂取 10 bp。两份参数是互斥示例，一个账户同一时刻只能绑定一个实例；切换前 Drain、核对平仓并重新生成账户绑定。要同时运行必须使用独立交易所子账户与独立绑定。

[负净边测量参数](strategy-negedge.toml) 是负责人 2026-10-17 要求的 demo-only 变体,使用真实 cross-taker 同事件双腿 IOC。`min_edge = close_edge = -0.0030`（−30 bp）、`slip = 0`、单腿每笔 25 USDT、持仓 50 USDT、`cooldown_ms = 7s`。两侧可执行净边通过时约每 7 秒尝试一对,已有仓位优先减仓,可持续开平;断连、Unknown、报价年龄、失败锁存或限额可能使实际频率更低。该参数接受手续费后的预期亏损,仅测 TD 链路,不作为收益证据。

该变体要求 refdata 账户 `is_prod = false` 且 core 使用 `order_env = "testnet"`;生产账户的任何负开/平仓阈值在 hft-cfg 和策略检查中都被拒绝。以同一制品的 schema 用下文命令构建时,将输入换为 `strategy-negedge.toml`,输出接入 dual-testnet 的 strategy_params 并更新 SHA-256,随后执行 `hft-core --config <双 venue 配置> --check`。CI 的 core_033_testnet_variants_check_production_bounds 使用此文件和离线 ETH 双 venue 规则完成实际 --check,并验证生产账户拒绝;真实主机仍须用当前账户 refdata/blob 完整复核。参数 schema v3 的边界摘要要求重新生成 SPB1。

RiskParams 应独立约束相同或更小的限额。实际 tick、lot、最低名义、手续费及现货可用余额须从 demo 账户/交易规则确认；若最小可交易订单超过 cap，应保持禁用并重新评估配置。参数只是待标定的 demo 假设。负净边测量只使用下述专用参数。现货基准库存由账户快照/分配流程确定，不用于开反向仓。

选币证据为 2026-10-16 09:22:57 UTC 收到的 Bitget demo `spot / ETHUSDT / books1` 快照：bid 2453.84 × 1000 ETH，ask 2454.14 × 3010 ETH；来源为[官方 demo WebSocket](https://www.bitget.com/docs/uta/demo-trading/websocket) 的 `wss://wspap.bitget.com/v3/ws/public`。该样本只证明当时有双边报价，不证明持续流动性或账户下单权限。启动前重查新鲜深度、交易规则和 spot 账户能力；新形态的真实下单验收仍待执行，已接受的 M3-lite 旧形态运行不能替代它。

以 CI 制品运行 `hft-cfg build --kind strategy --schemas schemas --refdata refdata.bin --in strategy.toml --out params.spb1`；选择资金费示例时输入为 `carry.toml`。将输出文件直接用于 hft-core 的 blobs.strategy_params，并填写其 SHA-256。启动默认 Shadow，启用参数不绕过模式或账户闸门。两个策略的固定产品形态还会在 registry 加载与 SetParams 时检查。


构建与安装使用 `release-aarch64.yml` 的 `workflow_dispatch`，选择经批准且 CI 通过的 SHA、唯一 `dev_n`，设置 `artifact_only = true`；本轮专用主机只接受合并后 main 制品。下载该运行的 `release-dist`，在主机上核对 `.sha256`；`RELEASE.json` 记录提交、CI 证据、编译 CPU 包络、schema 和逐文件摘要。安装器不编译；CPU 参数与包络归 [构建目标](../../docs/development.md#目标-cpu)，主机预检必须通过，不能绕过不匹配。

以 root 准备 `/etc/hft/host-local.toml`、主机清单、测试网配置与 blob、秘密引用、签发者和独立冷卷，按[发布步骤](../../docs/runbooks/release-and-deploy.md)安装归档，再运行 `activate-release.sh --version <版本> --group core`。激活保持交易单元停止；单独启动护栏、IMDS 和 slice 后运行预检。联调只能由 `deploy/dev2/hft-window start` 开窗，`stop` 走正常 Drain。失败保留窗口标志，确认 prod 停止后再关闭窗口。

进程故障或干净退出后都先关闭窗口、人工撤空并核对仓位和余额。当前运行时拒绝旧 journal；归档 seq/raw 段后保留原实例 `meta/engine_epoch` 和 `core.lock`，下一会话使用同一 cold root，禁止删除或回退 epoch。整目录换盘或恢复仍属延期的恢复流程，不能靠新建空目录绕过。
