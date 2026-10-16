# Agent Notes

这里只放一种设计文档。**Agent Note** 记录一个影响本仓库的决定或提案——**为什么**、**放弃了什么**、**需要什么验证**,也就是代码与 README 承载不了的部分。本文件规定 Agent Note 放在哪里、何时写、文件格式。改编自 DeepSeek Harness 的 `.agents/notes/README.md`(样例在 architecture-v2/research/_dsh/notes_README.md(未入库的资料缓存:`architecture-v2/research/_dsh/notes_README.md`));改编理由见 [dsh-style-documentation](proposed/process/2026-09-30-dsh-style-documentation.md)。

## 永续/现货产品形态(2026-10-16)

[产品形态](implemented/feature/2026-10-16-perp-spot-shape.md) 收窄两策略为指定永续空头与现货多头,其他形态及反向开仓禁用。

## 同时下单决定(2026-10-14)

[同时下单](implemented/feature/2026-10-14-cross-taker-simultaneous.md)取代[可执行价差](implemented/feature/2026-10-14-cross-taker-executable-spread.md)的 OnFill 时序决定。

## M3-lite 决定(2026-10-16)

[M3-lite](implemented/architecture/2026-10-15-m3-lite.md) 收窄两所测试网范围；负责人接受 2026-10-16 已完成运行，不要求连续 24 小时测试网长跑；正式上线门槛保持。

## M2-lite 决定(2026-10-14)

[M2-lite](implemented/architecture/2026-10-14-m2-lite.md) 收窄 M2 范围,推迟功能的用例改为里程碑 `M9`(上线后);changes.md 的批量确认结论见[签字批次 1](implemented/process/2026-10-14-signoff-batch-1.md)。

## 恢复缺口验证决定(2026-10-12)

[恢复缺口验证](implemented/bug-fix/2026-10-12-recovery-gap-verification.md) 记录不可变输入的已标记缺口、未知尾部报告与 Model 来源中断。

## Model 跨重启来源决定(2026-10-12)

| 当前 Note | 决定 | 被收窄的 Note |
|---|---|---|
| [Model 来源连续性](implemented/bug-fix/2026-10-12-model-restart-continuity.md) | 引擎逐运行恢复;Model 按合法起点与连续前缀续算 | [离线 Model 起点](implemented/architecture/2026-10-12-offline-model-origin.md) |

## 公开仓库 CI 决定(2026-10-11)

| 当前 Note | 决定 | 被部分取代的 Note |
|---|---|---|
| [公开仓库 CI](implemented/process/2026-10-10-public-repository-ci.md) | 公开仓库使用标准托管 runner;取消分钟门禁,保留发布质量检查 | [ci-quota-self-estimate](proposed/process/2026-10-03-ci-quota-self-estimate.md)、[ci-org-and-profiles](proposed/architecture/2026-10-03-ci-org-and-profiles.md)、[ci-free-hosted](proposed/architecture/2026-10-03-ci-free-hosted.md) |

## 原始对账凭据决定(2026-10-13)

[原始对账凭据](implemented/architecture/2026-10-13-reconcile-cash-receipts.md) 收窄 [auto_held 提案](proposed/architecture/2026-10-03-auto-external-guard.md) 的严格现金能力范围。

## 布局与命名

每条 Agent Note 有两个维度,都编码在**路径**里:`{lifecycle}/{class}/yyyy-mm-dd-topic.md`。

- **生命周期**(顶层目录)就是状态,状态变化时文件在目录间移动:
  - **`proposed/`** —— 实现前评审的提案;尚未构建(或只构建了一部分)。设计基线 v1 冻结时绝大多数决定在这里;组件实现(M1 起)合入时按"落地时移动"移到 `implemented/`。
  - **`implemented/`** —— 决定已经落地。文件记录决定了什么、否决了什么,并**随落地的事实保持更新**:代码后来挪了文件、改了 crate 名、改了配置键或默认值时,同一改动里更新该 Note 的事实(路径、名称、结构),不改决定本身。
  - **`rejected/`** —— 提案被考虑后否决。只在它的理由能阻止一个诱人的、有实质影响的错误时保留;否则删除。
- **类别**(第二层目录)是决定的*种类*,见[分类](#分类)。

文件名里的日期是该主题**首次提出**的日期。Agent Note 之间、以及文档指向 Agent Note 的引用一律用相对 Markdown 链接,目标写成相对路径(例如 `../../implemented/architecture/2026-mm-dd-topic.md`),不用裸文字或编号——链接可被机器检查,也能在目录间移动后被修复。

不建集中的 `INDEX.md`:目录树本身就是清单,浏览生命周期/类别目录或全文搜索即可。下面只列**当前范围的入口**——一轮裁决改变了多条旧决定时,在这里列出新 Note 与被它取代或收窄的旧 Note,旧 Note 自己在 `Status:` 行之后带一行"第 N 轮"状态说明并链接新 Note。

### 第八轮入口(2026-10-02)

| 新 Note | 决定 | 被取代或收窄的旧 Note |
|---|---|---|
| [ccex-first-scope](proposed/architecture/2026-10-02-ccex-first-scope.md) | 新框架首要服务 CEX-CEX;链上组件 optional;core = 首次上线所需;交付 = 新子账户上线 | [build-test-migrate](proposed/process/2026-10-01-build-test-migrate.md)、[chain-core-free-profile](proposed/architecture/2026-10-01-chain-core-free-profile.md)、[chain-first-cex-dex](proposed/architecture/2026-09-28-chain-first-cex-dex.md)、[tx-onchain-expiry](proposed/architecture/2026-10-01-tx-onchain-expiry.md)、[thread-plan-and-host-classes](proposed/architecture/2026-10-01-thread-plan-and-host-classes.md) |
| [strategy-sdk-decoupling](proposed/architecture/2026-10-02-strategy-sdk-decoupling.md) | 策略 SDK、一策略一 crate、静态装配、schema 驱动参数、不透明 Advice | [warm-compute-lane](proposed/architecture/2026-09-30-warm-compute-lane.md)、[rust-and-python-only](proposed/architecture/2026-09-28-rust-and-python-only.md)、[cold-process-set](proposed/architecture/2026-09-30-cold-process-set.md) |
| [exec-layer-targets](proposed/architecture/2026-10-02-exec-layer-targets.md) | 目标意图 + 执行层;对冲量上限;按原因的腿顺序 | [kill-risk-reducing-passthrough](proposed/architecture/2026-10-01-kill-risk-reducing-passthrough.md) |
| [sim-accounts-not-backtest](proposed/architecture/2026-10-02-sim-accounts-not-backtest.md) | 不做回测评估;模拟账户;journal 重演只排障 | [single-writer-sequencer-and-journal](proposed/architecture/2026-09-28-single-writer-sequencer-and-journal.md)、[fuzz-and-deterministic-simulation](proposed/testing/2026-09-30-fuzz-and-deterministic-simulation.md)、[accounts-first-class](proposed/architecture/2026-09-30-accounts-first-class.md) |
| [venue-caps](proposed/architecture/2026-10-02-venue-caps.md) | 交易所差异写进 `VenueCaps` | [refdata-component](proposed/architecture/2026-09-30-refdata-component.md)、[venue-protocol-and-runtime-split](proposed/architecture/2026-09-30-venue-protocol-and-runtime-split.md) |
| [margin-stress](proposed/architecture/2026-10-02-margin-stress.md) | 账户级保证金分档;`ExternalFill`;treasury core | [desired-mode-and-caps](proposed/architecture/2026-10-01-desired-mode-and-caps.md)、[cold-process-set](proposed/architecture/2026-09-30-cold-process-set.md) |
| [one-account-per-strategy-instance](proposed/architecture/2026-10-02-one-account-per-strategy-instance.md) | 一个账户只绑定一个策略实例;身份核验;按账户 HARD | [one-exchange-account-one-system](proposed/architecture/2026-10-01-one-exchange-account-one-system.md)、[accounts-first-class](proposed/architecture/2026-09-30-accounts-first-class.md)、[kill-risk-reducing-passthrough](proposed/architecture/2026-10-01-kill-risk-reducing-passthrough.md) |

另有只做事实同步的旧 Note(首批交易所、启动闸门、AdoptExchange 扩展、CI 唯一构建环境):[collector-stays-separate](proposed/architecture/2026-10-01-collector-stays-separate.md)、[l2-batch-and-bbo-lane](proposed/architecture/2026-10-01-l2-batch-and-bbo-lane.md)、[recovery-epoch-and-boot-rules](proposed/architecture/2026-10-01-recovery-epoch-and-boot-rules.md)、[adopt-exchange-manual-intervention](proposed/architecture/2026-10-01-adopt-exchange-manual-intervention.md)、[build-target-aarch64](proposed/process/2026-10-01-build-target-aarch64.md)。裁决原文见 [round8-decisions](../../design/_work/round8-decisions.md)。

### 第八轮补充入口(2026-10-03)

独立评审后的用户决定([round8-decisions](../../design/_work/round8-decisions.md) R8-8,评审见 [review-2026-10-03](../../design/_work/review-2026-10-03-M0独立评审.md)):

| 新 Note | 决定 | 被取代或收窄的旧 Note |
|---|---|---|
| [broken-auto-fix](proposed/architecture/2026-10-03-broken-auto-fix.md) | Broken 持续 2 min 无人工时自动有界平掉多出的一腿一次;L3 与 flatten 覆盖 Broken;冷启动不立刻判 Broken | [exec-layer-targets](proposed/architecture/2026-10-02-exec-layer-targets.md)、[margin-stress](proposed/architecture/2026-10-02-margin-stress.md)、[kill-risk-reducing-passthrough](proposed/architecture/2026-10-01-kill-risk-reducing-passthrough.md)、[adopt-exchange-manual-intervention](proposed/architecture/2026-10-01-adopt-exchange-manual-intervention.md)、[recovery-epoch-and-boot-rules](proposed/architecture/2026-10-01-recovery-epoch-and-boot-rules.md) |
| [no-same-exchange-spot-perp](proposed/architecture/2026-10-03-no-same-exchange-spot-perp.md) | 不做同一交易所的现货 × 永续;一个账户在每个交易所只用一个市场;删除余额域 | [ccex-first-scope](proposed/architecture/2026-10-02-ccex-first-scope.md)、[one-account-per-strategy-instance](proposed/architecture/2026-10-02-one-account-per-strategy-instance.md)、[venue-caps](proposed/architecture/2026-10-02-venue-caps.md) |
| [sim-only-instance](proposed/architecture/2026-10-03-sim-only-instance.md) | 模拟让位于实盘(`rep-sim` 最低);统一升级后在只含模拟账户、不持 key 的 `sim` 实例运行 | [sim-accounts-not-backtest](proposed/architecture/2026-10-02-sim-accounts-not-backtest.md)、[accounts-first-class](proposed/architecture/2026-09-30-accounts-first-class.md)、[warm-compute-lane](proposed/architecture/2026-09-30-warm-compute-lane.md)、[thread-plan-and-host-classes](proposed/architecture/2026-10-01-thread-plan-and-host-classes.md)、[one-exchange-account-one-system](proposed/architecture/2026-10-01-one-exchange-account-one-system.md) |
| [warmup-injection](proposed/architecture/2026-10-03-warmup-injection.md) | Model 检查点不随参数、refdata 与发版失效;`InjectWarmup` 注入历史样本 | [strategy-sdk-decoupling](proposed/architecture/2026-10-02-strategy-sdk-decoupling.md) |
| [funding-recon-alert-only](proposed/architecture/2026-10-03-funding-recon-alert-only.md) | 资金费差异只告警、不进闸门;账本按交易所流水自动校正 | [recovery-epoch-and-boot-rules](proposed/architecture/2026-10-01-recovery-epoch-and-boot-rules.md)、[adopt-exchange-manual-intervention](proposed/architecture/2026-10-01-adopt-exchange-manual-intervention.md)、[desired-mode-and-caps](proposed/architecture/2026-10-01-desired-mode-and-caps.md) |
| [ci-free-hosted](proposed/architecture/2026-10-03-ci-free-hosted.md) | CI 与旧项目一致只用免费托管 runner;fuzz/DST 门槛改为累计量(每夜时长与累计地点已被第四次修订的 [ci-org-and-profiles](proposed/architecture/2026-10-03-ci-org-and-profiles.md)、[verify-on-server](proposed/process/2026-10-03-verify-on-server.md) 改写) | [build-target-aarch64](proposed/process/2026-10-01-build-target-aarch64.md)、[fuzz-and-deterministic-simulation](proposed/testing/2026-09-30-fuzz-and-deterministic-simulation.md)、[ccex-first-scope](proposed/architecture/2026-10-02-ccex-first-scope.md) |
| [viewer-emergency-stop](proposed/architecture/2026-10-03-viewer-emergency-stop.md) | 老板(Viewer)唯一的写权限:非 HARD、不平仓、带确认的紧急停止;不能恢复 | [kill-risk-reducing-passthrough](proposed/architecture/2026-10-01-kill-risk-reducing-passthrough.md)、[desired-mode-and-caps](proposed/architecture/2026-10-01-desired-mode-and-caps.md) |

`ci-free-hosted` 按[分类](#分类)属于 `process`,为与本轮其他文件的链接一致放在 `architecture/`;移动时同一改动修复入站链接。

### 第八轮第三次修订入口(2026-10-03)

第二次独立评审后的用户决定([round8-decisions](../../design/_work/round8-decisions.md) R8-9,评审见 [review-2026-10-03-M0b](../../design/_work/review-2026-10-03-M0b独立评审.md)):

| 新 Note | 决定 | 被取代或收窄的旧 Note |
|---|---|---|
| [no-small-live](proposed/architecture/2026-10-03-no-small-live.md) | 不设小额实盘:A dev2 测试 + 测试网/模拟盘联调 → B dev2 模拟账户 ≥ 14 天 → C 统一升级主机(prod + sim 同机,延迟验收)→ D 正式实盘;dev2 不放正式实盘账户;正式实盘的限额由运营在 `RiskParams` 设定 | [sim-accounts-not-backtest](proposed/architecture/2026-10-02-sim-accounts-not-backtest.md)、[ci-free-hosted](proposed/architecture/2026-10-03-ci-free-hosted.md) |
| [sim-colocated-with-prod](proposed/architecture/2026-10-03-sim-colocated-with-prod.md) | 任何主机上模拟都在独立 `sim` 实例;升级后与 prod 同机:非隔离核、`hft-sim.slice`、独立出口 IP(第二个私有 IP + EIP);主机档位 M7 前定(候选 `c16`) | [sim-only-instance](proposed/architecture/2026-10-03-sim-only-instance.md)、[thread-plan-and-host-classes](proposed/architecture/2026-10-01-thread-plan-and-host-classes.md) |
| [margin-liq-distance](proposed/architecture/2026-10-03-margin-liq-distance.md) | MarginStress 按到强平价的距离分档(25% / 15% / 10%,待标定),比率只做交叉校验;永续杠杆按账户实际配置读取;"单边 +30%"压力用例 | [margin-stress](proposed/architecture/2026-10-02-margin-stress.md) |
| [quote-freshness-conn-health](proposed/architecture/2026-10-03-quote-freshness-conn-health.md) | 报价可用 = BBO 连接健康(`conn_live_tsc`、非重同步、WS ping);`max_mid_age_ms` 退役 | [broken-auto-fix](proposed/architecture/2026-10-03-broken-auto-fix.md) |
| [engine-time](proposed/architecture/2026-10-03-engine-time.md) | 跨重启连续、停机不计的 `Now.engine_ns`;策略、ledger、risk 的持久状态不存 TSC | [recovery-epoch-and-boot-rules](proposed/architecture/2026-10-01-recovery-epoch-and-boot-rules.md) |

另有不新建 Note、只加状态行的收窄:[viewer-emergency-stop](proposed/architecture/2026-10-03-viewer-emergency-stop.md)(`/stop` 的确认绑定预览 `cmd_hash` + 60 s TTL、不比对状态版本;hft-core 不健康时经 kill-switch 本地撤单入口 + P0)、[funding-recon-alert-only](proposed/architecture/2026-10-03-funding-recon-alert-only.md)(`NoHistory` 自动按交易所金额入账)。`no-small-live` 按[分类](#分类)属于 `process`,与 `ci-free-hosted` 同理放在 `architecture/`。

### 第八轮第四次修订入口(2026-10-03)

第三次独立评审(五路)后的用户决定([round8-decisions](../../design/_work/round8-decisions.md) R8-10,规格见 [round8d 简报](../../design/_work/round8d-spec-brief.md),评审汇总见 [review-2026-10-03-M0c 五路汇总](../../design/_work/review-2026-10-03-M0c五路汇总.md)):

| 新 Note | 决定 | 被取代或收窄的旧 Note |
|---|---|---|
| [ci-org-and-profiles](proposed/architecture/2026-10-03-ci-org-and-profiles.md) | hft 仓库放免费 GitHub 组织(老板 Read,以发布 `preflight` 代替受保护分支;2026-10-07 起改为个人私有仓库 `Fiora-gwx/hft`,见状态行);`release`/`release-hot`/`ci`/`bench` 剖面;`ci.yml` 触发收窄、`decoupling.yml`、`ops-py`;PR 头出开发构建;分钟预算重写 | [ci-free-hosted](proposed/architecture/2026-10-03-ci-free-hosted.md)、[build-target-aarch64](proposed/process/2026-10-01-build-target-aarch64.md) |
| [verify-on-server](proposed/process/2026-10-03-verify-on-server.md) | ③④ 的累计在服务器上运行 CI 构建的验证包(dev2 常态、M7 升级主机在 ⑩ 窗口外);只有失败清零;正式版须补跑 | [ci-free-hosted](proposed/architecture/2026-10-03-ci-free-hosted.md)、[fuzz-and-deterministic-simulation](proposed/testing/2026-09-30-fuzz-and-deterministic-simulation.md) |
| [latency-probe-milestone](proposed/architecture/2026-10-03-latency-probe-milestone.md) | M1.5 延迟探针(按需租 c8g 跑纵切片)、参考机从 M1.5 起按需租用、只能绑定非正式账户的延迟探针策略 | — |
| [rx-irq-driven](proposed/architecture/2026-10-03-rx-irq-driven.md) | 收包只用中断驱动:busy poll 全关、交易网卡 IRQ 独占一核、sim 用独立 ENI-C、hostprobe 硬校验 | [kernel-tcp-no-bypass-phase1](proposed/architecture/2026-09-28-kernel-tcp-no-bypass-phase1.md)、[sim-colocated-with-prod](proposed/architecture/2026-10-03-sim-colocated-with-prod.md)、[sim-only-instance](proposed/architecture/2026-10-03-sim-only-instance.md) |
| [host-class-c32](proposed/architecture/2026-10-03-host-class-c32.md) | 主机类追加 `c32` 作为默认候选,`c16` 为下限候选,`c8` 只承载 prod;M3 结束前按 M1.5 实测选定 | [thread-plan-and-host-classes](proposed/architecture/2026-10-01-thread-plan-and-host-classes.md)、[sim-colocated-with-prod](proposed/architecture/2026-10-03-sim-colocated-with-prod.md) |
| [feed-split-and-snap-fetch](proposed/architecture/2026-10-03-feed-split-and-snap-fetch.md) | feed 按 BBO 与深度拆成两个任务;REST 快照移到 warm 任务 `snap-fetch` | [l2-batch-and-bbo-lane](proposed/architecture/2026-10-01-l2-batch-and-bbo-lane.md)、[quote-freshness-conn-health](proposed/architecture/2026-10-03-quote-freshness-conn-health.md) |
| [sequencer-md-preempt](proposed/architecture/2026-10-03-sequencer-md-preempt.md) | sequencer 逐条处理 md,每条后先看 ctl/rep/bbo 环头;md 每轮按档数与时间片限额 | [l2-batch-and-bbo-lane](proposed/architecture/2026-10-01-l2-batch-and-bbo-lane.md) |
| [offbox-watchdog](proposed/architecture/2026-10-03-offbox-watchdog.md) | dev2 上的机外看门狗、watchtower `/healthz`、交易主机失联的授权表与网页兜底 | [cold-process-set](proposed/architecture/2026-09-30-cold-process-set.md) |
| [imds-lockdown](proposed/architecture/2026-10-03-imds-lockdown.md) | 正式主机只有 root 能访问 IMDS;冷进程用凭证代理按前缀发放的短期凭证 | [cold-process-set](proposed/architecture/2026-09-30-cold-process-set.md) |
| [shadow-before-live](proposed/process/2026-10-03-shadow-before-live.md) | 切 Live 前做零成交冒烟与正式实例 Shadow 3–7 天;初始限额与第一次放宽条件是签字必填数值;正式账户的 kill 演练只做零成交冒烟 | [no-small-live](proposed/architecture/2026-10-03-no-small-live.md) |

另有不新建 Note、只加状态行的收窄(第四次修订):[viewer-emergency-stop](proposed/architecture/2026-10-03-viewer-emergency-stop.md)(ctl-gateway 不可达时 kill-switch 的 `stop_core`、待补 Kill、交易员 `/stop` 立即生效)、[warmup-injection](proposed/architecture/2026-10-03-warmup-injection.md)(结构参数重置与新增 group 的自动预热)、[accounts-first-class](proposed/architecture/2026-09-30-accounts-first-class.md)、[one-exchange-account-one-system](proposed/architecture/2026-10-01-one-exchange-account-one-system.md)、[one-account-per-strategy-instance](proposed/architecture/2026-10-02-one-account-per-strategy-instance.md)、[margin-stress](proposed/architecture/2026-10-02-margin-stress.md)、[build-test-migrate](proposed/process/2026-10-01-build-test-migrate.md)(删去"小额实盘")。`ci-org-and-profiles` 按[分类](#分类)属于 `process`,与 `ci-free-hosted` 同理放在 `architecture/`。

### 第八轮第五次修订入口(2026-10-03)

第四次独立评审(五路)后的用户决定([round8-decisions](../../design/_work/round8-decisions.md) R8-11,规格见 [round8e 简报](../../design/_work/round8e-spec-brief.md),评审汇总见 [review-2026-10-03-M0d 五路汇总](../../design/_work/review-2026-10-03-M0d五路汇总.md)):

| 新 Note | 决定 | 被取代或收窄的旧 Note |
|---|---|---|
| [keyframe-one-shot](proposed/architecture/2026-10-03-keyframe-one-shot.md) | 关键帧在空闲轮一次写完(≤ 300 µs),周期 300 s;删除按节分片与 journal 写者的 `mark/rollback` | [sequencer-md-preempt](proposed/architecture/2026-10-03-sequencer-md-preempt.md) |
| [latency-acceptance-evidence](proposed/process/2026-10-03-latency-acceptance-evidence.md) | ⑩ 硬线 = 验收下限、基准形态为目标(之间签字);1 s 校准对插值与误差过滤;`rep_to_order`;按事件加权的离线统计;不另取测试网窗口;干扰按 bootstrap 上界判定 | [latency-probe-milestone](proposed/architecture/2026-10-03-latency-probe-milestone.md) |
| [m3-end-probe](proposed/architecture/2026-10-03-m3-end-probe.md) | AZ 探测前移到 M1.5 之前;M1.5 扩大测量;M3 末满订阅探针定主机档位与是否拆 IRQ | [latency-probe-milestone](proposed/architecture/2026-10-03-latency-probe-milestone.md)、[host-class-c32](proposed/architecture/2026-10-03-host-class-c32.md) |
| [irq-split-reserve](proposed/architecture/2026-10-03-irq-split-reserve.md) | c32 预留第二 IRQ 核与 ENI-B2(默认不启用);IRQ 核进 `rcu_nocbs`;多 socket 行情线程 `epoll_pwait(0)`、BBO 每次至多 8 帧、短读即停;c16 只在探针达标时保留 | [rx-irq-driven](proposed/architecture/2026-10-03-rx-irq-driven.md)、[host-class-c32](proposed/architecture/2026-10-03-host-class-c32.md)、[feed-split-and-snap-fetch](proposed/architecture/2026-10-03-feed-split-and-snap-fetch.md) |
| [core-market-data-scope](proposed/architecture/2026-10-03-core-market-data-scope.md) | core 行情不含全量增量深度;需要深度时用分档快照流 TopN;snap-fetch 只做 BBO 补帧 | [feed-split-and-snap-fetch](proposed/architecture/2026-10-03-feed-split-and-snap-fetch.md)、[l2-batch-and-bbo-lane](proposed/architecture/2026-10-01-l2-batch-and-bbo-lane.md) |
| [model-path-section](proposed/architecture/2026-10-03-model-path-section.md) | Model 检查点分原始样本节与带 `PATH_VERSION` 的路径状态节;分位带对齐 UTC | [warmup-injection](proposed/architecture/2026-10-03-warmup-injection.md) |
| [manual-hold-and-stop-marker](proposed/architecture/2026-10-03-manual-hold-and-stop-marker.md) | 人工外来成交后 cell 级 `manual_hold`;`stop_core` 的持久停机标记(重启带 Kill cap)与额外确认;`Kill{flatten}` 在闸门未放行时也允许 | [viewer-emergency-stop](proposed/architecture/2026-10-03-viewer-emergency-stop.md) |
| [smoke-order-command](proposed/process/2026-10-03-smoke-order-command.md) | 正式端点冒烟改用 Admin 专用的 `SmokeOrder` 命令 | [shadow-before-live](proposed/process/2026-10-03-shadow-before-live.md) |
| [dev2-guard-system-slice-psi](proposed/process/2026-10-03-dev2-guard-system-slice-psi.md) | dev2 护栏的停机信号读 `system.slice` 压力;批处理不设 CPU 配额;tripped 与窗口标志文件;只自动重启 sim、每天至多一次 | [verify-on-server](proposed/process/2026-10-03-verify-on-server.md) |

另有不新建 Note、只加状态行的收窄或事实同步:[ci-org-and-profiles](proposed/architecture/2026-10-03-ci-org-and-profiles.md)(1,500 min 规则、`release-x86-cross.yml`、作业超时、`ci` 剖面 `panic = "unwind"`)、[offbox-watchdog](proposed/architecture/2026-10-03-offbox-watchdog.md)(维护静默、主机本地的看门狗目标、第二 AZ 待命清单、第三 AZ 看门狗)、[imds-lockdown](proposed/architecture/2026-10-03-imds-lockdown.md)(消费者前缀、对象键主机段、AMI 工具)、[single-event-header](proposed/architecture/2026-09-30-single-event-header.md)(`MarketHdr` 48 B)。

### 第八轮第六次修订入口(2026-10-03)

第五次独立评审(五路)后的用户决定([round8-decisions](../../design/_work/round8-decisions.md) R8-12,规格见 [round8f 简报](../../design/_work/round8f-spec-brief.md),评审汇总见 [review-2026-10-03-M0e 五路汇总](../../design/_work/review-2026-10-03-M0e五路汇总.md));本轮之后规格冻结为设计基线 v1:

| 新 Note | 决定 | 被取代或收窄的旧 Note |
|---|---|---|
| [resolve-broken-hard-only](proposed/architecture/2026-10-03-resolve-broken-hard-only.md) | `resolve_broken` 只在解除 HARD 时产生、按补腿口径、跳过 `manual_hold`;恢复期抑制按多出那一腿所在 venue 的对冲闸门与对账判定,期间不计时;闸门久不放行而有裸量 → P0 | [broken-auto-fix](proposed/architecture/2026-10-03-broken-auto-fix.md) |
| [auto-external-guard](proposed/architecture/2026-10-03-auto-external-guard.md) | `AUTO_EXTERNAL` 要求两轮无用户流断开且用户流稳定一个周期;合成成交在 Input 内合成、不写记录;ledger 以 `auto_held` 抵扣补推 | [margin-stress](proposed/architecture/2026-10-02-margin-stress.md) |
| [position-driven-subscription](proposed/architecture/2026-10-03-position-driven-subscription.md) | 引擎订阅含有持仓或库存的 inst 与每个 `ref_inst` | [margin-liq-distance](proposed/architecture/2026-10-03-margin-liq-distance.md) |
| [dev2-root-batch-slice](proposed/process/2026-10-03-dev2-root-batch-slice.md) | 根级 `hftbatch.slice` + `CPUWeight=idle`;Sim 实例自动解除 Recovery;护栏只因 hft 自身用量停机;dev2 只对 hft 的 uid 锁 IMDS 并运行凭证代理 | [dev2-guard-system-slice-psi](proposed/process/2026-10-03-dev2-guard-system-slice-psi.md)、[verify-on-server](proposed/process/2026-10-03-verify-on-server.md)、[imds-lockdown](proposed/architecture/2026-10-03-imds-lockdown.md)、[sim-colocated-with-prod](proposed/architecture/2026-10-03-sim-colocated-with-prod.md) |
| [ci-quota-self-estimate](proposed/process/2026-10-03-ci-quota-self-estimate.md) | CI 用量自估、Admin `quota_override`、`hotfix` 标签、额度耗尽时经个人私有镜像出包并每季度演练(2026-10-07 起镜像应急与 DRL-111 取消,见状态行) | [ci-org-and-profiles](proposed/architecture/2026-10-03-ci-org-and-profiles.md) |
| [testnet-uid-unverifiable](proposed/process/2026-10-03-testnet-uid-unverifiable.md) | 读不到 UID 的测试网账户用 `uid_policy = "Unverifiable"`(Admin 签字) | [one-account-per-strategy-instance](proposed/architecture/2026-10-02-one-account-per-strategy-instance.md) |
| [design-baseline-v1](proposed/process/2026-10-03-design-baseline-v1.md) | 规格冻结为设计基线 v1;此后只走实现驱动的变更;测试规格(`TESTS.md`)先行 | [dsh-style-documentation](proposed/process/2026-09-30-dsh-style-documentation.md) |

另有不新建 Note、只加状态行的收窄或事实同步(第六次修订):[keyframe-one-shot](proposed/architecture/2026-10-03-keyframe-one-shot.md)(事件数触发、最大推迟、实盘不算 `state_hash`)、[latency-acceptance-evidence](proposed/process/2026-10-03-latency-acceptance-evidence.md)(⑩ 的样本口径、上下界、SW 锚退路与 A/A)、[manual-hold-and-stop-marker](proposed/architecture/2026-10-03-manual-hold-and-stop-marker.md)(作废的停机标记 `.aborted`、额外确认条件、待补 Kill 的补发前核对)、[smoke-order-command](proposed/process/2026-10-03-smoke-order-command.md)(Drain 期间 SMOKE 槽保留)、[l2-batch-and-bbo-lane](proposed/architecture/2026-10-01-l2-batch-and-bbo-lane.md)(补第五次修订状态行)。

### 第八轮第七次修订入口(M0-g,2026-10-04)

测试规格先行(R8-12 第 4 条;简报 [round8g](../../design/_work/round8g-tests-brief.md),裁决 [round8g-rulings](../../design/_work/round8g-rulings.md)):不新建 Note,只加状态行或事实同步:[design-baseline-v1](proposed/process/2026-10-03-design-baseline-v1.md)(基线 = M0-f 与 M0-g 合并后、`test-trace`、变更记录口径)、[smoke-order-command](proposed/process/2026-10-03-smoke-order-command.md)(删除"在交易所价格带内")、[testnet-uid-unverifiable](proposed/process/2026-10-03-testnet-uid-unverifiable.md)(`uid_policy` 小写取值、非正式账户的 UID 重复只告警)、[sequencer-md-preempt](proposed/architecture/2026-10-03-sequencer-md-preempt.md)(删除 `md_levels_per_round`)。

## 分类

类别是封闭集合,`cargo xtask doc-check` 拒绝其他目录名。新增类别需要同时修改本节与 `doc-check` 的集合。

| 类别 | 覆盖 |
|---|---|
| `architecture` | 关于**交付的源码**的结构性决定:进程与线程拓扑、crate 关系、事件与状态模型、数值表示、传输与存储形态 |
| `process` | 代码**周围**的工具、策略与流程:CI 门槛、文档体系、发布、路线图与投入顺序 |
| `testing` | 测试基础设施与策略 |
| `feature` | 新的面向使用者(交易员、运维、研究)的能力 |
| `bug-fix` | 修正缺陷,或关闭一个事故暴露的缺口(通常与 [defensive-patterns](../../docs/defensive-patterns.md) 的一节对应) |
| `simplification` | 删除代码、行为或表面积而不增加能力 |

`architecture` 与 `process` 的分界:architecture 关于我们交付的源码;process 关于周围的工具与工作流。没有 `refactor` 类:它与 `simplification` 重叠,后者的判据"可观察行为是否改变"已经覆盖。

## 何时写

- 在同一个 PR 里新增或更新 Agent Note,**只**为代码、测试与现有文档解释不了的**持久决策理由**。实质性的未来工作从 `proposed/` 开始;已经做出的决定从 `implemented/` 开始。
- 更新已经拥有该决定的 Note 即满足规则,不要新建重复的 Note。
- 机械改动或局部改动(改名、格式、局部 bug 修复且无新的取舍)不需要 Agent Note。
- 以下情形**必须**有 Agent Note:推翻某个 README 里的规格;新增或删除 crate/进程;改变事件模型、journal 格式、线程拓扑、依赖方向;放宽任何延迟门槛;引入新的外部依赖到 hot crate;改变发布或测试门槛。
- 每条 Note 链接它的证据:[architecture-v2](../../design/README.md) 的章节、`research/` 笔记、基准或直方图输出、事故复盘。

## 取代与合并

- 一条 Agent Note **不会被编辑成另一个决定**。决定改变时写一条新 Note 取代它,两条互相链接:新 Note 在 `## Problem` 或 `## Alternatives considered` 里链接旧 Note;旧 Note 移到 `rejected/`(提案被取代)或保留在 `implemented/` 并在正文开头加一句"已被 <新 Note 的相对链接> 取代"(已落地的决定被部分或全部取代)。
- 已落地的 Note 被**完全**取代后,可以合并进当前的拥有者并删除:合并前,拥有者必须保留旧 Note 独有的全部理由、备选方案、后果、所需验证与已知覆盖缺口,并修复所有入站链接。部分取代不适用:两条都保留并互相链接。
- `implemented/` Note 为追踪其决定现在落在哪里而做的编辑(路径、名称)是**必须的**,不是禁止的。
- `proposed/` Note 被**部分**取代或收窄时留在原处(入站链接不断),在 `Status:` 行之后空一行加一条引用块 `> **第 N 轮(日期)**:<哪部分被取代或收窄>;见 <新 Note 链接>。`,正文不改写;完全被取代的提案移到 `rejected/`。

## 文件格式

每条活动的 Agent Note 遵循同一格式。标题与正文用中文;下面的机器检查标记(头部与节标题)保持英文原样。

**格式检查冻结**:`cargo xtask doc-check` 对 Agent Note 只查相对链接可达与类别目录属于封闭集合;本节的头部、`Status:` 与目录一致、必需节等规则是评审约定,不由机器阻断。需要机器检查时以一条 process 类 Agent Note 解冻;下文的"doc-check 交叉检查"、"doc-check 拒绝"、"doc-check 失败"均指解冻之后。

### 头部

每条 Agent Note 的前三行恰好是:

```markdown
# Agent Note: <标题>

Status: <status>
```

之后一个空行。`Status:` 取三种形式之一,必须与文件所在的生命周期目录一致(doc-check 交叉检查):

- `Status: proposed`
- `Status: implemented`
- `Status: rejected — <一行原因>`

状态不带日期、不带括注:文件名有首次提出日期,git 有其余一切。"以修正形式接受"写在正文里决定所在的位置。

### 正文骨架

每条 Agent Note 的正文以 `## Problem` 开头——动机,写成不依赖解决方案也能成立。其后按生命周期:反复出现的节只用下面的规范名;真正专有的技术节(线程拓扑、数据布局、协议、迁移步骤)可以自由放在必需节之间。

#### `proposed/`

```markdown
## Problem
## Proposal
…专有节…
## Alternatives considered
## Acceptance criteria
## Risks
```

`## Proposal` 是拟做的改变,可以用将来时——计划、迁移步骤、开放问题在工作未完成时属于这里。`## Acceptance criteria` 写什么可观察的状态算完成(带数字与测量方式)。`## Risks` 同时覆盖可能出错的地方与这个改变有意放弃的东西。

#### `implemented/`

```markdown
## Problem
## Decision
…专有节…
## Alternatives considered
## Consequences
```

`## Decision` 用现在时描述落地的现实,整个文件随其保持更新。`## Consequences` 记录这个取舍付出了什么**和**换来了什么。提案期的标题在这里被 doc-check 拒绝:`## Proposal`、`## Plan`、`## Migration plan`、`## Acceptance criteria` 不得出现在 implemented Note 中。现在时的 `## Testing`、`## Deferred`、`## Related` 可以有。

#### `rejected/`

被否决的 Note 就是冻结的提案:保留它在提案期有的节(包括 `## Acceptance criteria`),结论在 `Status:` 行。只有头部、`## Problem`、`## Proposal` 与下面的备选方案要求适用。

### Alternatives considered —— 必填

每条 Agent Note 都有 `## Alternatives considered`:每个真实的备选方案以及它为什么输了,每个方案一个粗体开头的段落,或对有争议的方案用 `### Why not <X>?` 子节。只记录决定而不记录它胜过了什么,就会招来反复争论——而避免反复争论正是 Agent Note 存在的理由。备选方案是记录下来的,不是编出来的:写被实际讨论或调研过的方案,并链接证据。

### 生命周期之间移动

在生命周期目录之间移动文件意味着同一改动里更新 `Status:` 行并满足目标目录的骨架,否则 doc-check 失败。`proposed/` → `implemented/`:把 `## Proposal` 改写为现在时的 `## Decision`,把 `## Acceptance criteria` 与 `## Risks` 并入 `## Consequences`(或改成现在时的 `## Testing`/`## Verification`,写明现在由什么固定该行为),删去计划、写实际落地的内容。`proposed/` → `rejected/`:只在 `Status:` 行加原因并冻结文件。

### 篇幅

一条 Note 通常 1–3 KB。超过 5 KB 时先检查是否把规格(属于 README)或操作步骤(属于 cookbook/runbook)写了进来。
