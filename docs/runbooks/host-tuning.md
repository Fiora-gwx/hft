---
description: "统一升级主机(默认 `c32`,`c16` 只在 M3 末探针达标时;M7 起)及其他非 dev2 主机类的调优与验收:中断驱动收包(busy poll 全关、交易网卡 IRQ 独占一核且进 rcu_nocbs、managed IRQ 不落 IRQ 核、XPS 按热核分配、sim 用 ENI-C)、按 02 §4 清单逐项落地并用 02 §6 指标验收;租用实例(M1.5、参考机)只做逐项核对,M3 末满订阅探针以正式版与探针清单安装;任何调优改动都按一次一项、24 h 对比执行;dev2 不适用"
scope: core
---

# 主机调优与验收

## 适用范围

- 只适用于 `c4`/`c8`/`c16`/`c32`/`metal` 主机类:M7 统一升级([host-upgrade](host-upgrade.md))时的新主机或新 AMI 首次上线。`dev2` 与旧系统正式账户共机,不做主机级调优,只有 [dev2 护栏](../../deploy/README.md#dev2-护栏)。
- M1.5 延迟探针与参考机的租用实例([development 参考机](../development.md#参考机)):不安装生产归档,按 C 节逐项核对(只读记录 ENA 队列数、IRQ 布局、managed IRQ 落点、NAPI 参数、XPS 可写性与 HW RX 时间戳能否用),不做 D 节的 24 h 验收;结论写进对应探针的 Agent Note。核对:DRL-68。
- **M3 末满订阅探针**按正式版安装([deploy 制品](../../deploy/README.md#制品从哪里来)):`[需授权]` 租用 `c8g.4xlarge`(c16 计划;c32 计划另租 `c8g.8xlarge`)并挂上为探针预建的 ENI-B/ENI-C → `[需授权]` 把探针清单 `[egress].prod` 的 EIP 加入所用测试网 key 的 IP 白名单 → 按 [release-and-deploy](release-and-deploy.md) 的新主机首次安装引导安装当前正式版,`/etc/hft/host-local.toml` 写 `manifest = "probe-c16"`(或 `probe-c32`)→ 本页 B、C 节(含 GRUB 重启)→ 启动 kill-switch、只含测试网账户的 prod 与 sim → 按探针计划测量;结束后 `[需授权]` 从白名单撤销探针 EIP 并退租。不做 D 节的 24 h 验收;结论写进探针的 Agent Note。核对:DRL-67。
- 任何调优项改动(`deploy/` 下 GRUB、sysctl、网卡、IRQ、hugepages、resctrl、tuned、chrony 的变化;DRL-65)。
- 每季度一次复验,以及 AWS 维护/实例替换之后(DRL-66)。

调优项本身的定义与生成在 [deploy/README](../../deploy/README.md#运行期调优);本页只写执行与验收。原则:**先测、一次改一项、24 h 直方图对比**([02 §4](../../design/02-硬件与托管方案.md))。

## 前置条件

1. 主机清单 `deploy/hosts/<host>.toml` 已合入,生成物无差异(CI 中 `cargo xtask gen-host --check` 通过)。
2. 实例位于胜出的 (AZ, CPG) 与关联的 PTPG([az-probe](az-probe.md));Capacity Reservation 已建。
3. 发布归档已安装并激活(冷)([release-and-deploy](release-and-deploy.md) 步骤 1–12)。
4. hft-core 未运行(`systemctl is-active hft-core@prod` = inactive)。

## 步骤

### A. 基线(只在改动前执行)[只读]

1. 记录当前 HostProfile:`sudo hostprobe collect --out /tmp/hp-before.json`([tools/hostprobe](../../tools/README.md#hostprobe))。
2. 记录 24 h 基线(若是改动而不是首次上线):
   - `rtla osnoise top -c <hot> -d 24h -q > osnoise-before.txt`(或只在 3 个代表核上跑并保留其余核空闲);
   - 改动前 24 h 的 watchtower 日报(各 stage p50/p99/p99.9/max)。
3. 若改动涉及网卡:`ethtool -S <eni-b> > ethS-before.txt`。

### B. 内核启动参数

4. `[需授权]` 确认 `/etc/default/grub.d/50-hft.cfg` 为发布内生成物(`diff` 与 `/opt/hft/current-core/deploy/grub/`),`update-grub`,重启(之前在本机 `sudo hft-maint start 30`,机外看门狗只发 Telegram)。
5. 重启后:`cat /proc/cmdline`,逐项核对 [deploy GRUB](../../deploy/README.md#grub)(`irqaffinity` = hk 核与 IRQ 专用核;`rcu_nocbs` 含隔离核与 IRQ 专用核;aarch64 上 `transparent_hugepage=madvise`);`cat /sys/devices/system/cpu/isolated` = 生成的隔离集合(prod 的全部 `Spin` 核);`cat /sys/devices/system/cpu/nohz_full` = 只有 sequencer 核;`cat /sys/devices/system/cpu/smt/active` = 0(只适用于 x86 `metal`;Graviton 无 SMT)。
6. `grep Huge /proc/meminfo`:`HugePages_Total` 与清单一致;hft-core 未运行时 1 GiB 与 2 MiB 页的 `free_hugepages` 等于清单中的数量;`mount | grep hugetlbfs` 显示 `/dev/hugepages`(pagesize=1G)与 `/dev/hugepages2M`。B 节的核对:DRL-62。

### C. 运行期调优

7. `systemctl status hft-host-tune hft-eni-attach hft-net-tune hft-resctrl` 全部 `active (exited)` 且退出码 0。
8. IRQ(以 `irq-plan.json` 为准,核号来自 hft-core 线程计划):`grep -E 'ena|nvme' /proc/interrupts` 与 `cat /proc/irq/*/smp_affinity_list`——ENI-A 的 IRQ 只在 hk 核(核 0);**ENI-B 全部队列只在 IRQ 专用核(核 1)**,且该核上没有任何线程与可运行任务(`ps -eL -o psr,comm` 在该核上无条目);启用时 ENI-B2 全部队列只在 `eni_b2` 核;ENI-C 全部队列只在 `eni_c` 核;NVMe 的 managed IRQ:`cat /proc/irq/<n>/effective_affinity_list` 既不含隔离核也不含 IRQ 专用核;`systemctl is-active irqbalance` = inactive。`c4` 没有 IRQ 专用核:全部 IRQ 在核 0。
9. workqueue:`for f in /sys/devices/virtual/workqueue/*/cpumask; do cat $f; done | sort -u` 只有 hk 掩码。
10. 网卡(ENI-B、ENI-C 与启用时的 ENI-B2 各做一遍;网卡按清单的设备序号找到,MAC 由 `hft-eni-attach` 从 IMDS 读出):`ethtool -l <eni>`(combined = 清单 `queues`,`max` 即实例上限)、`ethtool -c`(adaptive-rx off、rx-usecs 0、tx-usecs 0)、`ethtool -k`(gro/lro/tso/gso/sg off)、`ethtool -g`(rx 4096、tx 1024);`cat /sys/class/net/<eni>/napi_defer_hard_irqs` = 0、`cat /sys/class/net/<eni>/gro_flush_timeout` = 0;ENI-B 的 `cat /sys/class/net/<eni-b>/queues/tx-*/xps_cpus` 与 `irq-plan.json` 的 XPS 映射一致。
11. sysctl:`sysctl -a 2>/dev/null | grep -F -f <(cut -d= -f1 /opt/hft/current-core/deploy/sysctl/90-hft.conf)` 与文件一致;特别核对 `net.core.busy_poll = 0`、`net.core.busy_read = 0`(收包只有中断驱动,[rx-irq-driven](../../.agents/notes/proposed/architecture/2026-10-03-rx-irq-driven.md))。
12. 时钟:`chronyc tracking`(Reference ID 为 PHC,`Leap status: Normal`);`cat /sys/class/net/<eni>/device/phc_error_bound`(或驱动暴露的等价项)换算误差界 < 40 µs。
13. resctrl(若启用):`cat /sys/fs/resctrl/hot/cpus_list`、`schemata` 与清单一致;不可用则确认清单 `resctrl.enabled = false`。
14. 系统杂项:`systemctl list-units --state=running | grep -E 'snapd|unattended|apport|cloudwatch|oomd|multipathd'` 为空;`dpkg -l ec2-instance-connect` 未安装;`amazon-ssm-agent` 保留作破窗通道,`taskset -cp $(pgrep -f amazon-ssm-agent)` 只含非隔离核且不含 IRQ 专用核;`hft-imds-lock` active,`sudo /opt/hft/current-core/deploy/imds/hft-imds-lock.sh status` 显示 IMDS 程序已挂在根 cgroup 上,`lsmod | grep nf_conntrack` 为空([deploy IMDS 锁定](../../deploy/README.md#imds-锁定与凭证代理))。
15. 汇总:`sudo hostprobe verify --expected /opt/hft/current-core/deploy/tuning/<清单名>.toml --out /run/hft/hostprofile.json`(清单名取 `/etc/hft/host-local.toml`),退出码 0(全部 `required` 项相符,含 busy poll 四项、IRQ 落点、managed IRQ 与 XPS),`pending_reboot = false`。C 节的核对:DRL-63。

### D. 验收测量

16. `[只读]` 隔离核抖动 24 h:`rtla osnoise hist -c <hot> -d 24h`;另在 2 个隔离核上 `rtla timerlat hist -c <core> -d 1h`(只在没有热线程时测)。
17. `[需授权]` 启动 hft-core(只连行情即可,或同时运行只含模拟账户的 `hft-core@sim`;M7 延迟验收期间 prod 只含测试网账户,M8 起为正式账户),运行 24 h;从 watchtower 读 `kernel_rx` 与诊断分段 `nic_to_kernel`/`kernel_to_user` 的直方图(分段定义与预算归 [latency-budget](../latency-budget.md))。
18. 网卡落点:hft-core 在每条连接收到首帧后的 NAPI 自检(规则归 [hft-core 主机资源隔离](../../apps/hft-core/README.md#主机资源隔离slice核与出口-ip))全部通过——prod 的连接全部属于 ENI-B(启用时批量连接属于 ENI-B2),sim 的连接全部属于 ENI-C;`ethtool -S <eni-b>` 与 `ethtool -S <eni-c>` 的队列计数只随各自实例的流量增长;停掉 sim 时 ENI-C 计数停止、IRQ 专用核的 softirq 时间(`/proc/softirqs` 的 `NET_RX`)不变。
19. 到交易所:`ss -ti state established '( dport = :443 )'` 取每条连接 `minrtt`;hft-core 每连接 ping 直方图 p99 抖动。D 节的核对:DRL-64(隔离核 `osnoise` 由 TOOL-112 判定)。

## 验收

| 指标 | 目标([02 §6](../../design/02-硬件与托管方案.md)) | 来源(步骤) |
|---|---|---|
| 隔离核 `osnoise` 24 h | max < 20 µs,p99.9 < 5 µs | 16 |
| `kernel_rx`(中断驱动) | 只记录,不是门槛;预算按主机类分写,归 [latency-budget §3](../latency-budget.md#3-cex-路径预算);⑩ 的判定归 [testing §8.2](../testing.md#82-延迟验收的测法) | 17 |
| 定时器超调(若仍有定时器) | p99 < 10 µs | 16 |
| 到 Binance `minrtt` | 比搬迁前降 ≥ 0.5 ms;p99 RTT 抖动 < 100 µs | 19 |
| chrony PHC 误差界 | < 40 µs | 12 |
| prod 连接在 ENI-B、sim 连接在 ENI-C | 100 % | 18 |
| busy poll 四项、IRQ 落点、managed IRQ 与 XPS | hostprobe `required` 全部相符 | 8、10、11、15 |

调优改动的验收:改动后的 24 h 与步骤 2 基线对比,目标分段 p99 不劣化;若声称改善,附直方图对比(p50/p99/p99.9/max 与样本数)(DRL-65)。

## 回退

- 运行期项:`releasectl rollback --version <上一版>`(冷),重启 `hft-host-tune` 系列单元或重启主机,重跑步骤 7–15。
- GRUB 项:回滚版本 + `update-grub` + 重启 `[需授权]`。
- 若某项在 24 h 对比中劣化:回退该项,在 Agent Note 中记录证据(`.agents/notes/rejected/tuning/`)。

## 记录

写 `ops-log/<date>-host-tuning-<host>.md`:AMI ID、发布版本、hostprobe JSON 的 SHA-256、每项验收数字与原始文件路径、授权人。验收数字同时贴进 M7 延迟验收记录([host-upgrade](host-upgrade.md))与评审(只看直方图与回放)。
