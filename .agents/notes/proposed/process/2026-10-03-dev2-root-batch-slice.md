# Agent Note: dev2 批处理放到根级 hftbatch.slice 并设 CPUWeight=idle;护栏只因 hft 自身用量停机;dev2 也锁 IMDS

Status: proposed

## Problem

批处理所在的 slice 名字带连字符,按 systemd 的命名规则落在 `hft.slice` 之下。单元级 `CPUSchedulingPolicy=idle` 与 `CPUWeight=1` 只在 `hft.slice` 内部比较;在根一级,调度器比较的是 `hft.slice`(权重 50)与旧系统所在的 `system.slice`(100),批处理满载时仍能拿走争用核约 1/3,"争用时完全让出"不成立。另外:只含 Sim 账户的实例非干净重启后带 Recovery,护栏自动重启救不回交易;旧系统自身造成的压力照样停 hft 并计入退路次数;dev2 不锁 IMDS,hft 的非 root 进程能拿到与旧系统共用的角色凭证([X4 评审](../../../../design/_work/review-2026-10-03-M0e-X4运行与交付.md) N-P1-1、P2-1、P2-2、P2-8、§5-1/4/6/7)。

## Proposal

用户决定按推荐执行([round8-decisions](../../../../design/_work/round8-decisions.md) R8-12;规范见 [round8f 简报](../../../../design/_work/round8f-spec-brief.md) F4)。单元与机制归 [deploy](../../../../deploy/README.md#slice-与资源切分),数值与护栏规则归 [hft-core](../../../../apps/hft-core/README.md#dev2-护栏),Sim 实例的 Recovery 解除归 hft-risk 与 hft-core。

1. 批处理放到**根级** `hftbatch.slice`(全部主机类;名字不带连字符),设 `CPUWeight=idle`(cgroup `cpu.idle`,需 systemd ≥ 252、内核 ≥ 5.15);批处理单元保留 `Nice=19`,不再依赖 `CPUSchedulingPolicy=idle`。
2. 只含 Sim 账户的实例非干净重启后,无 `RecoverMismatch`、无毒丸时自动解除 Recovery;否则写 `Note{RecoveryHeldSim}`(P1)。
3. 护栏只在 hft 自身用量明显(`hft.slice` 或 `hftbatch.slice` 的 CPU 或内存超过阈值)时停机,否则只告警;退路的"一周 ≥ 2 次"只计这种情况。
4. dev2 上只对 hft 各 uid 加 IMDS 的 nft 丢弃规则,并在 dev2 也运行 `hft-cred-broker`;旧系统不受影响。

## Alternatives considered

**对整个 `hft.slice` 设 idle。** 符合"旧系统优先",但旧系统满载时 sim 也会被饿到触发看门狗(评审 §5-1 的备选)。

**保留子 slice,把权重调到 1。** 根一级比较的仍是 `hft.slice` 的权重,批处理与 sim、冷进程共用这一份,改子 slice 权重解决不了跨 cgroup 的比较。

**护栏照旧按停机信号停 hft,不看 hft 用量。** 旧系统自己的高峰会反复停掉模拟长跑并把 dev2 推向退路。

## Acceptance criteria

- M3 标定演练前 `/sys/fs/cgroup/hftbatch.slice/cpu.idle` = 1;批处理满载 10 min,`system.slice` 的 `cpu.pressure` 与演练前相比不升,护栏不动作。
- 护栏演练:旧系统自身制造的压力 → 只告警、不写 tripped;hft 满载引起的压力 → 两级动作照常。
- dev2 上 hft 服务用户访问 IMDS 被丢弃,旧系统进程照常;冷进程经凭证代理读写 S3。

## Risks

- `cpu.idle` 让出的实际程度与 dev2 的 systemd 版本待核实;不足时按 hft-core 的退路处理。
- dev2 的实例角色仍与旧系统共用,旧系统进程仍能取到 hft 的测试网与运维秘密;dev2 不放正式账户,接受这一点。
