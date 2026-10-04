# Agent Note: dev2 护栏的停机信号读旧系统所在 system.slice 的压力;批处理不设 CPU 配额

Status: proposed

> **第八轮第六次修订(2026-10-03)**:部分取代第 1 条——批处理放到根级 `hftbatch.slice` 并设 `CPUWeight=idle`(单元级 `CPUSchedulingPolicy=idle` 不跨 cgroup 生效,不再依赖);收窄第 2、4 条——停机另需 hft 自身用量明显,否则只告警,退路的触发次数只计这种情况;二级动作另停联调窗口里的 `hft-reconciler`;回放期间暂停 archiver 与 pipeline 是全部主机类的规则。见 [dev2-root-batch-slice](2026-10-03-dev2-root-batch-slice.md)。

## Problem

dev2 护栏用整机 PSI 作停机信号,而 hft 自己给 `hftbatch.slice` 设了 30% 的 CPU 配额。Linux 5.7 起被 `cpu.max` 节流的时间计入 cpu "some" 压力,验证包或每日回放一满载,整机压力就在 70% 左右,约 60 s 后护栏一级动作把它停掉:③④ 在 dev2 上几乎不累计,⑦ 要求的每日回放跑不完。护栏"只停不启"又会被 `hft-verify.timer` 10 min 后绕过;`IOWeight`/`IOSchedulingClass` 在调度器 `none` 下不生效;`systemctl mask` 对装在 `/etc/systemd/system` 的单元无效;护栏停掉 sim 后周末无人恢复,⑦ 因停机超过 24 h 重新计时([W4 评审](../../../../design/_work/review-2026-10-03-M0d-W4运行与交付.md) P0-1、P1-1、P1-5、P1-7、P1-10、§5-1/7)。

## Proposal

用户决定按推荐执行([round8-decisions](../../../../design/_work/round8-decisions.md) R8-11;规范见 [round8e 简报](../../../../design/_work/round8e-spec-brief.md) E9、E10)。数值归 [hft-core dev2 护栏](../../../../apps/hft-core/README.md#dev2-护栏),单元、脚本与标志归 [deploy dev2 护栏](../../../../deploy/README.md#dev2-护栏),处置归 [incident-response P](../../../../docs/runbooks/incident-response.md#p-dev2-护栏触发)。

1. `hftbatch.slice` 去掉 `CPUQuota`,改 `CPUWeight=1`;全部批处理单元 `CPUSchedulingPolicy=idle`、`Nice=19`;磁盘隔离改用 `IOReadBandwidthMax=`/`IOWriteBandwidthMax=`。
2. 停机信号读旧系统所在 `system.slice` 的 `cpu.pressure`/`memory.pressure`,加上 `MemAvailable` 与 swap-in 速率;整机 PSI 只告警。
3. 护栏一级/二级动作时写持久标志 `/var/lib/hft/guard/tripped`(重启不消失);全部批处理单元的 `ExecCondition=` 先查它,只有人工 `hft-dev2-guard clear` 才解除。联调窗口是标志文件 `/run/hft/window/active`(重启即结束窗口),不用 `mask`。
4. 二级动作只停两个 hft-core 实例,不停 watchtower 与 ops-bot(sim 自动重启后要出日报)。护栏停掉 sim 后,资源信号回落到阈值以下 ≥ 30 min 且此后无人手动启停过该单元,守护自动重启 `hft-core@sim`,每 UTC 日至多一次;从不启动 prod 与批处理;polkit 只为 `hft-core@sim.service` 增加 `start`。
5. M3 标定加演练:批处理满载 10 min,护栏不动作、`system.slice` 压力不升(`[需授权]`,生产主机);不通过 → 提前启用 dev2 退路(另开 `c7g.large`)。

## Alternatives considered

**保留 `CPUQuota`,把整机 PSI 阈值调高。** 节流本身就让压力常驻高位,阈值调到不触发时也失去了保护旧系统的意义。

**把两个定时器加进护栏白名单一并停掉。** 定时器之外还有手动启动与 `hft-window stop` 的恢复路径;标志文件让所有启动路径走同一个判定。

**护栏只停不启,停 sim 改为夜间电话告警。** 每次资源尖峰都要人在夜里恢复模拟长跑;自动重启限定为 sim、每天一次,不碰任何持有真实资金的进程。

**直接启用 dev2 退路。** 每月约 $66;先用演练证明共机可行,不行再开。

## Acceptance criteria

- M3 标定演练通过:批处理满载 10 min 守护不动作,`system.slice` 的压力与演练前相同。
- 护栏演练:tripped 存在时 `hft-verify.timer` 到点不启动批处理;sim 被停后信号回落规定时长内自动重启一次,同日第二次不重启;polkit 拒绝启动 sim 以外的单元。
- dev2 上验证包与每日回放在一周内按计划累计,护栏触发记录在日报中有解释。

## Risks

- `CPUWeight=1` + idle 调度下,旧系统持续繁忙时批处理几乎不前进;③④ 的累计速度以实测为准,不够时启用退路。
- 窗口标志在 `/run`,dev2 重启后窗口结束;tripped 标志持久,重启后批处理仍被挡住,直到人工清除(重启属人工或整机事件,之后按 incident-response P 节核对)。
