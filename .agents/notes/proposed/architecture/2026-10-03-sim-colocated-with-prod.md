# Agent Note: 模拟实例与实盘实例同机:独立 slice、非隔离核、独立出口 IP

Status: proposed

> **第八轮第四次修订(2026-10-03)**:收窄第 2、3 条——sim 用独立网卡 ENI-C(IRQ 钉在 sim 的一个核上,出口 = ENI-C 的私有 IP + EIP),不再是 ENI-B 的第二个私有 IP(dev2 只有一块网卡,仍用第二个私有 IP);主机档位默认候选 `c32`、下限候选 `c16`,M3 结束前按 M1.5 实测选定;同机干扰判据改为 `tick_to_trade` 与 `hedge_host` 的 p99 劣化 ≤ 5%,7 天内每小时交替开关 sim、相邻小时配对(唯一归属 testing ⑩);见 [rx-irq-driven](2026-10-03-rx-irq-driven.md)、[host-class-c32](2026-10-03-host-class-c32.md)。

> **第八轮第六次修订(2026-10-03)**:事实同步——批处理 slice 改为根级 `hftbatch.slice`(`CPUWeight=idle`),不在 `hft.slice` 之下;见 [dev2-root-batch-slice](../process/2026-10-03-dev2-root-batch-slice.md)。

## Problem

[sim-only-instance](2026-10-03-sim-only-instance.md) 让 dev2 上的模拟留在 prod 进程内,统一升级后搬到独立的 `sim` 实例,但"sim 实例放哪台主机"留作待定;`c8` 的线程计划与内存表没有为它留余量;两套行情连接走同一个出口 IP 时,sim 的连接与重连会占用 prod 的 IP 级限额([评审](../../../../design/_work/review-2026-10-03-M0b独立评审.md) §4 第 8 条)。用户决定模拟与实盘在同一台机器上([round8-decisions](../../../../design/_work/round8-decisions.md) R8-9 第 4 条);dev2 上也不再有实盘([no-small-live](2026-10-03-no-small-live.md))。

## Proposal

实例语义、线程计划与内存表归 [hft-core](../../../../apps/hft-core/README.md);slice、出口 IP 与单元归 [deploy](../../../../deploy/README.md);切换步骤归 [host-upgrade](../../../../docs/runbooks/host-upgrade.md)。

1. **任何主机上模拟都在独立的 `sim` 实例**:同一二进制,只含 Sim 账户,不持任何 key;prod 实例不配 Sim 账户。dev2 = sim 常驻 + 联调窗口里的测试网 `prod` 角色实例;升级主机 = prod + sim 同机。
2. **隔离**:sim 只用非隔离核(warm/cold),从不进 prod 的热核;独立的 `hft-sim.slice`(CPU、内存上限);独立的 journal 与冷盘目录、系统用户;**独立出口 IP**——主机的第二个私有 IP + EIP,sim 的交易所连接绑定它,不占 prod 的 IP 级限额,也不在任何 key 的白名单里。
3. **主机档位**必须同时容纳两个实例:`c8` 很可能不够,M7 前按实测选定,候选 `c16`(16 vCPU / 32 GB 级别);hft-core 给出"prod + sim 同机"一列;deploy 的隔离核列表由线程计划生成,不手写。
4. **资源切分**:`hft-core.slice`、`hft-sim.slice`、`hftbatch.slice` 各自上限;OOM 时 hft-core 在 hft 单元中最后被杀,kill-switch 不在这些 slice 里。

## Alternatives considered

**sim 放另一台主机。** 多一台机器的费用与运维;行情从不同接入点进来,校准多一个误差来源。用户决定同机。

**sim 与 prod 共用出口 IP。** 交易所按 IP 计连接数与请求权重(各所口径待核实),sim 的订阅与重连会挤占 prod 的额度;多一个私有 IP + EIP 的成本很低。

**sim 留在 prod 进程内(`rep-sim` 优先级最低)。** 共享 sequencer 时间片与故障域,延迟验收时混入模拟负载;[sim-only-instance](2026-10-03-sim-only-instance.md) 已在升级后否决它,dev2 上不再有实盘后也没有理由保留。

**sim 线程忙等或占隔离核。** sim 没有延迟验收,Park 足够;占热核直接损害 prod。

## Acceptance criteria

- 升级主机上 sim 运行与停止两种状态下,prod 的 `decide` 与 `tick_to_trade` p99 相差 ≤ 5%(延迟验收期间 7 天直方图对照)。
- sim 线程从不出现在隔离核上(watchtower `IsolatedCoreIntrusion` 零次);`hft-sim.slice` 的 `AllowedCPUs` 与隔离集合不相交(hostprobe `required`)。
- `ss -tnp` 显示 sim 的全部交易所连接源地址 = sim 私有 IP;prod 没有任何连接使用该地址。
- sim 实例的进程环境与文件系统中没有任何交易所 key。

## Risks

- 同一故障域:主机宕机同时停掉 prod 与 sim;sim 不持资金,可接受。
- 共享 L3 缓存与内存带宽可能抖动 prod 的尾延迟:由延迟验收判定;必要时降低 sim 订阅或用 resctrl/CAT(Graviton 上可用性待核实)。
- 主机档位升高的费用;内存、冷盘与 S3 按两个实例估算。
- dev2 上 sim 常驻、联调窗口再加一个测试网实例时资源紧张:测试网实例用小订阅档,护栏二级动作先停它。
