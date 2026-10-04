# Agent Note: 模拟账户让位于实盘;统一升级后在只含模拟账户的独立 hft-core 实例运行

Status: proposed

> **第八轮第三次修订(2026-10-03)**:部分取代——dev2 上模拟也不再留在 prod 进程内:任何主机上模拟都在独立的 `sim` 实例(dev2 = sim 常驻 + 联调窗口的测试网 `prod` 角色实例);统一升级后 sim 与 prod 同机,sim 只用非隔离核、独立 `hft-sim.slice`、独立出口 IP(第二个私有 IP + EIP);主机档位 M7 前按实测选定(候选 `c16`);本 Note 的"模拟放到另一台主机"与"两套连接走同一 IP"的风险随之关闭;见 [sim-colocated-with-prod](2026-10-03-sim-colocated-with-prod.md)。

> **第八轮第四次修订(2026-10-03)**:补充——升级主机上 sim 实例用独立网卡 ENI-C,不再与 prod 共享交易网卡的队列与中断;见 [sim-colocated-with-prod](2026-10-03-sim-colocated-with-prod.md) 的同轮状态行与 [rx-irq-driven](2026-10-03-rx-irq-driven.md)。

## Problem

[sim-accounts-not-backtest](2026-10-02-sim-accounts-not-backtest.md) 把模拟账户放进实盘的热 sequencer:`rep-sim` 环优先级(2)高于实盘 `bbo`(3),派发不分实盘与模拟,`c8` 的核 1 同时承载 persist、advisor 镜像与 sim-gw;dev2 上 `max_instances = 4` 与 `sim_pool = 4` 冲突([独立评审](../../../../design/_work/review-2026-10-03-M0独立评审.md) P1-3、P1-4)。总目标是延迟优先(R8-2),模拟账户却能在热路径上与实盘抢时间。用户决定按推荐处理([round8-decisions](../../../../design/_work/round8-decisions.md) R8-8 第 3 条)。

## Proposal

调度规则归 [hft-sequencer](../../../../crates/hft-sequencer/README.md);实例、线程计划与启动校验归 [hft-core](../../../../apps/hft-core/README.md);部署形态归 [deploy 模拟实例](../../../../deploy/README.md#模拟实例sim);切换步骤归 [host-upgrade](../../../../docs/runbooks/host-upgrade.md)。

1. **dev2 阶段留在进程内,优先级最低**:输入环优先级 `ctl > rep(实盘) > bbo > advice > md > rep-sim`;每轮 sim 处理量 ≤ `sim_max_per_round`(默认 32);同一事件派发时实盘实例先于模拟实例。dev2 每个研究中的策略预置 `sim_pool = 2` 个模拟实例,`max_instances = 8`。
2. **统一升级后独立实例**:模拟在 `instance = "sim"` 的 hft-core 实例运行——同一二进制,refdata 只含 Sim 账户,配置没有 `secrets_dir`、不加载任何交易所 key,自己的行情连接与 journal;实盘实例 `instance = "prod"` 不含 Sim 账户。启动校验:`LiveAccountInSimInstance`、`SimAccountInProdInstance`。journal 实例名只允许 `prod | sim`。
3. **部署**:`hft-core-sim.service`,独立用户 `hft-sim`,只用冷核,激活组 `sim`(可先于 prod 激活新版本做模拟验证)。
4. 它不交易任何交易所账户,不违反"同一账户只由一个系统交易"([one-exchange-account-one-system](2026-10-01-one-exchange-account-one-system.md))。
5. `c8` 线程计划中 `persist` 不与 `advisor` 同核。

## Alternatives considered

**模拟始终留在实盘进程内。** 共享 sequencer 预算与故障域;即使优先级最低,仍占用每轮时间片与内存,延迟验收时混入模拟负载。

**模拟用另一套代码或独立模拟器进程。** 失去"模拟 = 实盘减去交易所"的性质;[sim-accounts-not-backtest](2026-10-02-sim-accounts-not-backtest.md) 否决的是"另一套代码",本方案仍是同一二进制、同一回报路径。

**dev2 上就拆成两个实例。** dev2 只有 2 vCPU、3.8 GB 并与 cdex 共机,两套行情连接与 journal 放不下;dev2 不做延迟验收,最低优先级已足够。

**模拟放到另一台主机。** 多一台机器的费用;行情从不同位置接入,校准时还要区分网络差异。

## Acceptance criteria

- dev2 DST:sim 回报洪泛时,实盘回报与 BBO 的排队延迟不增加(每轮 sim ≤ 32 条,实盘先派发)。
- `c8`:sim 实例运行与停止时,prod 的 `decide` p99 差 ≤ 5%。
- sim 实例的进程环境与文件系统中没有任何交易所 key;含 Live 账户 → `LiveAccountInSimInstance`,prod 含 Sim 账户 → `SimAccountInProdInstance`。
- 激活 `sim` 组不停 prod,反之亦然。

## Risks

- 两套行情连接走同一 IP:各所每 IP 的连接与订阅上限待核实。
- journal 与归档量增加一份;冷盘与 S3 容量按两实例估算。
- 两个实例的行情到达时刻略有不同,模拟与实盘校准时的差异多一个来源;校准报告标出。
- 运维面对两个实例:ops_bot、pipeline、treasury、watchtower 的配置列出两个实例,降级命令不带账户时对两者各发一条。
