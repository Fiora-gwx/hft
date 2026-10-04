# Agent Note: 冷进程集合:合并同类、kill-switch 独立最小、ops_bot 用 Python

Status: proposed

> **第八轮(2026-10-02)**:treasury 转 core(监控 + 划转建议,不自动划转);pipeline 改为 core 框架 + 每策略插件;ops_bot 按策略 schema 通用处理,cdex 工作流为可选插件;见 [ccex-first-scope](2026-10-02-ccex-first-scope.md)、[margin-stress](2026-10-02-margin-stress.md)、[strategy-sdk-decoupling](2026-10-02-strategy-sdk-decoupling.md)。

> **第八轮第四次修订(2026-10-03)**:补充——kill-switch 增加 `stop_core` 动作(撤单后经 polkit 只停 `hft-core@prod`,Drain、不平仓),由 ops_bot 在 ctl-gateway 不可达时触发;新增 dev2 上的机外看门狗(Python 标准库)与正式主机上的凭证代理(root,只发按 S3 前缀限定的短期凭证);各冷进程按 `prod`/`sim` 两个实例配置;见 [offbox-watchdog](2026-10-03-offbox-watchdog.md)、[imds-lockdown](2026-10-03-imds-lockdown.md)、[viewer-emergency-stop](2026-10-03-viewer-emergency-stop.md)、[sim-colocated-with-prod](2026-10-03-sim-colocated-with-prod.md)。

## Problem

早期设计列了一长串冷进程:journal 归档、metrics-exporter、risk-monitor、reconciler、kill-switch、ops-bot、replayer 等([09](../../../../design/09-可观测性、风控与控制面.md))。其中 metrics-exporter 与 risk-monitor 做的是同一件事(尾随 journal 做统计与规则);ops-bot 若用 Rust 重写,要重做老板依赖的 6.2k 行 Telegram 工作流。每多一个进程就多一个部署单元、一套健康检查与一个故障域。

## Proposal

[13 O4](../../../../design/13-设计评审与优化.md)、[architecture.md §2](../../../../docs/architecture.md#2-部署单元):

| 进程 | 职责 | 语言 |
|---|---|---|
| [`journal-archiver`](../../../../apps/journal-archiver/README.md) | 尾随 seq/raw/chain journal → Parquet → S3,沿用现有 `parquet-store`/`s3-store`/`catalog` | Rust |
| [`watchtower`](../../../../apps/watchtower/README.md) | 指标导出与健康检查合一:直方图、告警规则、行情静默、时钟偏移、隔离核闯入;**只做 8 条 P0/P1 规则且只告警、不发命令**(行情/时钟/磁盘类降级由 `hft-risk` 的条件型 cap 负责),其余规则为可选扩展(按运维需要逐条加入) | Rust |
| [`reconciler`](../../../../apps/reconciler/README.md) | REST 对账 → `ReconcileReport` 经控制环 | Rust |
| [`kill-switch`](../../../../apps/kill-switch/README.md) | **独立且最小**:只含 REST 撤单与心跳监视;为没有死手接口的所(Binance 现货、Bitget)兜底 | Rust |
| [`replayer`](../../../../apps/replayer/README.md)、[`mock-exchange`](../../../../apps/mock-exchange/README.md) | 离线 | Rust |
| [`ops_bot`](../../../../python/ops_bot/README.md) | Telegram:读 journal 经 `hft-py`,发命令经 ctl-gateway Unix socket;不持有任何 key | Python |
| [`pipeline`](../../../../python/pipeline/README.md)、`treasury` | 名单/池子/参数、资金建议 | Python |

所有冷进程只读 journal;对热进程的唯一写入口是 ctl-gateway。

只有一个 hft-core 实例(没有灰度第二实例,见 [accounts-first-class](2026-09-30-accounts-first-class.md)),watchtower 与 ops_bot 只面对一个实例的 journal;ops_bot 在迁移时覆盖现有 Telegram 工作流。

## Alternatives considered

**metrics-exporter 与 risk-monitor 分开。** 两个进程读同一 journal、维护两份相同的派生状态;规则与指标天然耦合(p99 超预算既是指标也是告警)。

**kill-switch 并入 watchtower。** kill-switch 必须在 watchtower 出错时仍然工作;依赖越少越可靠,所以它不链接 watchtower 的规则引擎与导出栈。

**ops_bot 用 Rust。** 冷进程,语言不影响延迟;Python 直接迁移现有逻辑,老板工作流 1:1 映射,风险最低([rust-and-python-only](2026-09-28-rust-and-python-only.md))。

## Acceptance criteria

- 生产主机上的冷进程恰为上表;每个都有 systemd 单元、cgroup CPU 配额、禁入隔离核。
- kill-switch 在 hft-core 心跳消失后于刷新周期(5–30 s)内完成撤单(混沌测试)。
- ops_bot 进程的环境与文件系统中没有任何交易所 key(部署检查)。

## Risks

- watchtower 承担过多职责:按规则模块组织,单个规则的故障不影响导出;若规模增长再拆。
- ops_bot 经 PyO3 读 journal 与 Rust 版本耦合:`hft-py` 与 `gen-schema` 生成物同版本发布。
