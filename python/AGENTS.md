# AGENTS.md — python

本子树的规则补充根 [AGENTS.md](../AGENTS.md#常驻规则)。每个包的规格在 `python/<pkg>/README.md`(模板见 [crates/AGENTS.md](../crates/AGENTS.md#readme-模板),`kind: python-package`,`tier: cold`),测试规格在同目录 `TESTS.md`([格式](../docs/tests/README.md));pytest 函数名为 `test_<前缀小写>_<三位数>_<描述>`。

## 只在慢路径

- Python 只做分钟级以上的事(选币、参数、面板、日报、资金建议、研究);逐事件判断在 Rust,秒级慢计算属于策略包的 `Model`。
- 不写回测;策略表现看模拟账户([hft-sim](../crates/hft-sim/README.md))。

## 策略无关,按 schema 通用

- ops_bot、pipeline 框架、treasury 的代码与 README 不出现策略族名、交易所名分支或某个策略的命令名与字段名(`doc-check` 扫本目录);字段、单位、范围、Note 码表、命令与报表字段一律来自策略导出的 schema JSON(规格归 [strategies/registry](../strategies/registry/README.md))或运行时 `GetSchema`。新增策略不改这些包。
- 策略专属的 Python 代码**随策略包发布**:pipeline 插件在 `strategies/<name>/pipeline/`,经 entry point 组 `hft_pipeline.plugins` 被发现([pipeline 插件](pipeline/README.md#插件的打包与发现));ops_bot 的策略面板是 `hft_ops_bot.plugins` 的可选插件。本目录只有框架。
- 人工参数修改**按实例分键**(`overrides/<实例>.toml`),共享修改必须显式 `scope = pair`([pipeline 参数通道](pipeline/README.md#参数通道))。

## 唯一写入口是 ctl-gateway

- 对 hft-core 的一切写入都是 `ControlCmd`([control-plane](../docs/subsystems/control-plane.md)),经 hft-ctl 的纯 Python 客户端 `hftctl.py` 连本机 ctl socket 发出,结果以 `CmdResult` 为准;可用命令由进程 uid 与 issuer 角色在[权限表](../crates/hft-ctl-proto/README.md#权限表)中的上限决定。
- 唯一的旁路:ctl-gateway 不可达时,ops_bot 经 kill-switch 的本地触发套接字请求撤单或 `stop_core`([ops_bot 紧急停止](ops_bot/README.md#紧急停止stop));停止 hft-core 由 kill-switch 经 polkit 执行,Python 进程自己不对 `hft-*` 单元执行 start/stop/restart。
- 不写 hft-core 的文件、共享内存、配置;不调交易所下单、撤单、划转或提现接口。

## 读数据与编码

- 读 journal 只用 `hft_py.open_journal(...)` → Arrow;历史读 S3 Parquet(DuckDB/Polars)。事件 dtype、列名、定点指数来自 `cargo xtask gen-schema` 生成的 `python/_generated/`,不手写。
- 发给 hft-core 的数值是十进制字符串或定点整数,不经 Python `float`。**参数 blob 只有一份编解码实现:hft-strategy-sdk 的 `params`**;Python 只经 [hft-py](../crates/hft-py/README.md) 的 `hft_py.params.*` 调用它,不写 Python 版编解码器。refdata、`EngineConfig`、`RiskParams` 只由 [`tools/hft-cfg`](../tools/README.md) 生成。

## 谁持有什么 key

| 进程 | 交易 key | 只读 key | 其他秘密与凭证 |
|---|---|---|---|
| hft-core | 有(各 Live 账户的子账户 key,无提现权限、绑定 prod 出口 IP) | — | — |
| kill-switch | 撤单用 key | — | 告警 token |
| reconciler | — | 有 | — |
| ops_bot | **无** | **无** | Telegram bot token、Bark key;S3 只读短期凭证 |
| pipeline / treasury / research | **无** | 无(只用公共接口) | S3 只读短期凭证;pipeline 另可写 `params/` |

模拟账户不持 key;dev2 上只有测试网 key。Python 进程访问不到 IMDS,S3 凭证来自凭证代理写的 `/run/hft/creds/<user>/credentials`([deploy](../deploy/README.md#imds-锁定与凭证代理))。新进程要持只读 key 须写进其 README 并经评审。

## 版本与依赖锁定

- Python `>=3.12,<3.13`;每包(含插件包)一个 `pyproject.toml` + `requirements.lock`(`--generate-hashes`)。
- 构建与测试只在 CI 上做(质量门 `ops-py` 作业:`ruff`、`mypy --strict`、离线 `pytest`,[development](../docs/development.md#质量门quality-gatesyml));发布归档带全部 wheel 与**一个**合并的 `requirements.lock`,安装时离线建单一 venv([deploy](../deploy/README.md#发布归档与-releasectl))。

## 进程形态

- 每包一个 systemd 单元(`hft-ops-bot`、`hft-pipeline`、`hft-treasury`)与独立系统用户(`hft-opsbot`、`hft-pipeline`、`hft-treasury`);ops_bot 在 `hft-core.slice`,pipeline 与 treasury 在 `hftbatch.slice`;`dev2` 上受护栏约束([deploy](../deploy/README.md#dev2-护栏));周期任务在进程内调度,不用 cron。
- 每台主机有 `prod` 与 `sim` 两个 hft-core 实例:ops_bot、pipeline、treasury 在配置里列出两个,按策略实例所在的实例读 journal、发命令。
- 日志 → journald;状态写 `/var/lib/hft/<pkg>/`,原子写(`.tmp` + `os.replace`);涉及账户的输出带账户标签,模拟与测试网不与实盘合计。
