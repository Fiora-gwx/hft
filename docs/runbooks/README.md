---
description: "生产操作手册的索引与写法规则:从联调到正式实盘的上线(含 SmokeOrder 冒烟单与正式实例 Shadow)、发布部署(含验证包、维护静默)、事故响应(含交易主机失联与授权表)、M7 统一升级(prod 与 sim 同机)与主机调优、AZ 探测;链上策略迁移与 BSC 节点为可选"
kind: dir
tier: cold
scope: core
status: spec
---

# runbooks

## 概要

本目录是生产操作的唯一步骤归属地:每页是一件操作的编号步骤、每步的验证方法与失败时的处理。设计理由不在这里(→ [architecture-v2](../../design/README.md)、Agent Notes),组件契约不在这里(→ 各 README),事故叙述不在这里(→ [postmortem](../postmortem/README.md))。沿用 Trading AGENTS.md(本仓库之外:`Trading/AGENTS.md`) 的运维纪律:冷生产、授权与执行分离、以"第一分钟已验证"而非"已调度"作为完成标准、记录每一步的起止时间。

## 目录

| 页 | 什么时候用 | scope |
|---|---|---|
| [launch.md](launch.md) | 一个策略实例从联调到正式实盘:A dev2 测试网/模拟盘联调 → B dev2 模拟账户 ≥ 14 天(缩小规模的门槛 ⑦)→ C 统一升级主机(延迟验收、sim 满规模复核 ⑦、机外看门狗)→ D 正式实盘(子账户开通与核验、杠杆与禁借币、预热、签字的初始限额、测试网到正式的切换、SmokeOrder 冒烟 D-smoke、正式实例 Shadow 3–7 天 D-shadow、切 Live);没有小额实盘;回退 = Drain | core |
| [release-and-deploy.md](release-and-deploy.md) | 发布新版本(按单元组 core/cold/sim 激活,没有 `/opt/hft/current`,sim 可先行;改动交易所适配的版本先在 dev2 测试网冒烟)、新主机首次安装引导、维护静默、开发构建在 dev2 上的安装、验证包安装、回滚(含崩溃后回滚)、refdata 变更(Drain + 重启)、拉取 token 轮换 | core |
| [incident-response.md](incident-response.md) | P0/P1/P2 告警处置(含 Broken 自动平仓前后、残余核销、资金费差异)、kill 演练(测试网账户全量,正式账户只做 SmokeOrder 冒烟)、按账户 HARD、MarginStress(按强平距离,含串行兜底)、外部成交(强平/ADL/人工与 `manual_hold`)、交易所公告、dev2 护栏触发(tripped 标志、自动重启 sim)、老板紧急停止之后(只撤单或 `stop_core` 兜底、停机标记、待补 Kill 与失败)、交易主机失联(授权表、网页兜底、待命清单重建、改看门狗目标) | core |
| [host-upgrade.md](host-upgrade.md) | M7 统一升级:按 M3 末满订阅探针选定主机档位(默认 `c32`)与是否启用 ENI-B2 → ENI-B 与 sim 的 ENI-C 及出口地址、第二 AZ 待命网卡 → 新机启动 sim 与只含测试网账户的 prod(同机)→ 延迟验收与同机干扰 → 并发存储测量与 sim 满规模复核 ⑦ → 机外看门狗与维护静默 → 交给 launch D;dev2 不迁移正式账户 | core |
| [host-tuning.md](host-tuning.md) | `c4`/`c8`/`c16`/`c32`/`metal` 的新主机/新 AMI 上线、租用实例(M1.5、M3 末探针、参考机)的网卡与 IRQ 核对、任何调优改动、季度复验(dev2 不适用) | core |
| [az-probe.md](az-probe.md) | M1.5 之前选 AZ/CPG、第二 AZ 与是否需要第三 AZ 看门狗;每周复测交易所端点(在生产 CPG 的探针与生产机上,不在 dev2) | core |
| [migration.md](migration.md) | 可选:链上策略(cdex)从旧框架迁到新框架(触发条件:决定迁移) | optional |
| [bsc-node.md](bsc-node.md) | 可选:chain-core 执行钱包运维与自建 BSC 节点(触发条件:cdex 迁移、chain-core 启用) | optional |

## 使用本组件

### 写法规则

- 每页结构:**适用范围 → 前置条件 → 步骤 → 验收 → 回退 → 记录**;frontmatter 带 `description` 与 `scope`。
- 步骤编号;每步写:要执行的命令(或 ops_bot 命令)、**期望看到什么**(输出、指标、journal 事件)、不符时跳到哪一步或哪个事故处理节。
- 标记:`[需授权]` = 启停交易/采集、改变 `RunMode` 升级、删除数据或 S3 对象、改外部系统(AWS 资源、交易所设置、key)——执行前必须有授权人的明确授权,授权人与时间写进记录;`[只读]` = 不改变任何状态。
- 数字(阈值、预算)不在 runbook 里重新定义,链接其归属地(app README、[latency-budget](../latency-budget.md)、[testing §8](../testing.md#8-上线门槛))。
- 演练与上线检查点的步骤旁标出测试用例编号:runbook 定义的写 `DRL-n`([docs/tests/drills.md](../tests/drills.md)),组件 README 定义的写该组件 `TESTS.md` 的编号;一次执行的通过标准与证据只写在用例里,runbook 不重抄。
- 记录:每次执行在 `ops-log/<yyyy-mm-dd>-<操作>.md`(运维仓库或共享盘)写每一步的开始/结束 UTC 时间、处理的数据量、结论;把耗时拆成数据传输、门槛/构建、部署/激活、验收、清理、等待/阻塞。

### 值班与授权

ops_bot 的角色定义归 [control-plane](../subsystems/control-plane.md);本表只写运维分工:

| 人 | ops_bot 角色 | 可以执行 |
|---|---|---|
| 老板 | Viewer | 全部只读面板;唯一的写操作是**紧急停止** `/stop [账户] [core]`(非 HARD、不平仓的 kill:停开仓、撤挂单,对冲与平仓照常;确认绑定预览、60 s 内有效;ctl-gateway 不可达时经 kill-switch 撤单,风险状态允许时再停止 hft-core(`stop_core`),否则要 `/stop … core` 再确认,并 P0;不能恢复,恢复由交易员按 [incident-response R](incident-response.md#r-老板紧急停止stop之后) 做);授权 `[需授权]` 步骤(授权是人的决定,不需要 ops_bot 写权限);发现异常时通知值班 |
| 负责人 | Admin | 授权 `[需授权]` 步骤;Trader 的全部;冒烟单 `SmokeOrder` 与残余核销 `WriteOffResidual`(ops_bot 私聊或运维 CLI);refdata 与风控参数(`ReplaceRefdata`、`SetRisk`)只经运维 CLI |
| 值班交易员 | Trader | 全部 `[只读]`;全部降级与 kill(永远允许、不需确认);取得授权后执行需授权步骤 |
| 网页兜底人 A、B(待负责人指定) | — | 交易主机失联时在各所网页撤单、减仓与划转;名单与持有的权限只在 [incident-response S](incident-response.md#s-交易主机失联) 的授权表维护 |

## 实现说明

<details><summary>实现要点</summary>

无代码。页内命令引用的工具:`releasectl`(deploy/release,含 `install-verify`)、`hft-window`、`hft-maint`、`hft-dev2-guard clear`、`hft-sim-alternate`、`hft-imds-lock.sh`(deploy)、`hostprobe`、`kill-switch`、`replayer`(含 `compare`)、`hft-verify`、`journal-archiver check`、`hft-cfg`、`hft-pipeline`、`hft-treasury`、`rtla`、`ethtool`、`chronyc`、`ss`、`aws s3 sync`。命令名对应的路径(`/usr/local/{sbin,bin}` 下的链接)与以服务用户身份运行的写法归 [deploy](../../deploy/README.md#身份路径与-s3-前缀)。新增工具命令时同步更新引用它的 runbook。服务器上没有 cargo,runbook 中只出现发布归档内的二进制与脚本,以及 AMI 安装的系统工具。

</details>

## 延迟与资源契约

不适用:runbook 不是运行组件。各页的验收数字链接到其归属地。

## 测试与验收

用例与验收见 [docs/tests/drills.md](../tests/drills.md)(`DRL-n`)。本目录的两条规则:

- 每页在首次用于正式账户之前,在 `dev2` 或 M7 升级主机上以测试网或模拟账户完整走一遍并留记录(DRL-112);az-probe 不在 dev2 上运行,首次执行即在探针上。[incident-response](incident-response.md) 的 kill 演练每月一次(DRL-100)。
- `cargo xtask doc-check` 检查本目录链接与"每页有步骤与验收节"。

## 依赖

[deploy/](../../deploy/README.md)、[apps/](../../apps/AGENTS.md)、[python/ops_bot](../../python/ops_bot/README.md)、[watchtower](../../apps/watchtower/README.md)、[treasury](../../python/treasury/README.md)。

## 延伸阅读

- Trading docs/runbooks(本仓库之外:`Trading/docs/runbooks`)(现有 deploy、acceptance、latency、telegram-ops 等,作为写法参考)
- [11 实施路线图](../../design/11-实施路线图.md)(里程碑 M0–M8,含 M1.5 延迟探针)、[12 §3](../../design/12-风险与未决问题.md)

## 已知限制与后续工作

- 可选扩展:第二台 warm standby 主机的切换手册(触发条件:单机故障的停机时间不可接受)。
- 可选扩展:Solana、colo 的运维页(触发条件:对应能力立项)。

### 开发备忘

<details><summary>给实现者</summary>

- 事故处理中发现 runbook 步骤缺失或错误:同一天修正本目录并在 postmortem 中引用修正提交。

</details>
