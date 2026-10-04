---
description: "M7 统一升级:dev2 上联调与缩小规模的模拟长跑通过后,在按 M3 末满订阅探针选定档位的升级主机(默认 c32;c16 只在探针达标且启用 epoll_pwait(0) 时)上建立同机的 prod 与 sim 两个实例——按设备序号挂交易网卡 ENI-B 与 sim 网卡 ENI-C(预建第二 AZ 的待命网卡)并分配出口地址 → 安装 → 停用 dev2 的 sim → 新机启动 sim 与只含测试网账户的 prod → 延迟验收(窗口前 1–2 天同机干扰 A/A,一个 7 天主网行情窗口、按 A/A 加长或区间过宽时按 2 天一段延长、至多 14 天,两实例同机;同机干扰按 2 小时一块、种子随机且两类块配平的开关顺序测)→ 窗口后并发存储测量、换 AZ 恢复演练与 sim 满规模复核门槛 ⑦ → 机外看门狗与维护静默演练 → 交给 launch D 开正式实盘;dev2 不迁移任何正式账户"
scope: core
---

# 统一升级主机(M7)

## 适用范围

- **M7 统一升级**([11 章](../../design/11-实施路线图.md)):新框架在 `dev2`(现有 `c7g.large`,与旧系统正式账户共机)上做开发测试、测试网联调与模拟长跑(M6);"大体没问题后"在一台新主机上建立最终形态——`prod` 与 `sim` 两个 hft-core 实例同机([sim-colocated-with-prod](../../.agents/notes/proposed/architecture/2026-10-03-sim-colocated-with-prod.md)),打开 `Spin` 线程、隔离核、大页、中断驱动收包等热路径配置,做延迟验收,然后按 [launch D](launch.md#d-正式实盘m8) 开正式实盘。
- **dev2 上没有正式实盘账户**([no-small-live](../../.agents/notes/proposed/architecture/2026-10-03-no-small-live.md)):本页不迁移任何 Live 仓位或 prod journal。dev2 上的测试网 `prod` journal 留在 dev2(只读保留);模拟账户不迁移状态,在新主机的 sim 实例上从 `initial_balances` 重新开始。
- **主机档位**([host-class-c32](../../.agents/notes/proposed/architecture/2026-10-03-host-class-c32.md)、[m3-end-probe](../../.agents/notes/proposed/architecture/2026-10-03-m3-end-probe.md)):必须同时容纳两个实例,且 BBO 线程、IRQ 专用核与每个下单 venue 的 gw 线程各占一核;默认 `c32`(`c8g.8xlarge`,32 vCPU / 64 GiB);`c16`(`c8g.4xlarge`,首批下单 venue ≤ 4)只在 M3 末满订阅探针达标且启用 `epoll_pwait(0)` 时保留为候选;`c8` 只能承载 prod、不能用作升级主机。档位与是否启用第二块交易网卡 ENI-B2([irq-split-reserve](../../.agents/notes/proposed/architecture/2026-10-03-irq-split-reserve.md))都按 M3 末探针的结论选定,宁可上更大档位,不砍首批 venue。线程计划、IRQ 核与内存表见 [hft-core 线程计划](../../apps/hft-core/README.md#线程计划)。
- 升级 = **换主机类配置 + 部署对应的调优**,不改代码、不重新构建:dev2 与升级主机跑同一个 aarch64 二进制([development 目标 CPU](../development.md#目标-cpu))。换到可选的 x86 `metal` 是换架构 = 版本变化,同样走本页。
- 升级后 dev2 停用 sim 实例,保留为开发构建的测试网联调环境、验证包的常态累计环境与机外看门狗的所在地(联调窗口才运行测试网实例、护栏照旧)。

## 前置条件

1. 升级的决定已由负责人做出并记录(依据:dev2 上 A、B 阶段的验收,[launch](launch.md));dev2 缩小规模的门槛 ⑦ 已满足(满规模复核在本页第 11 步之后,规则归 [testing §8](../testing.md#8-上线门槛))。
2. **主机档位已选定**:依据 M3 末满订阅探针(按需租用 `c8g.4xlarge`,按 c16 线程计划、6 个产品线满订阅加压舱深度、打开 raw journal 与 persist、测试网探针单;BBO 的 `nic_to_kernel` 在突发窗口的表现决定是否启用 ENI-B2;以正式版与探针清单安装,步骤在 [host-tuning](host-tuning.md),里程碑项归 [11 §3.2](../../design/11-实施路线图.md))的结论与 hft-core 的同机线程计划和内存表(两个实例的线程数 ≤ 核数、IRQ 专用核不放线程、每个下单 venue 的 gw 独占线程、sim 只占非隔离核、内存合计 + 系统余量 ≤ 实例内存),结论写进 ops-log 与探针的 Agent Note;deploy 清单 `hosts/tokyo-cex-1.toml`(M7)与 `hosts/tokyo-cex-1-live.toml`(M8)的 `instance_type`、`class` 与 `[eni_b2]` 已按结论更新,两份待命清单同步,生成器无错误([deploy 主机清单](../../deploy/README.md#主机清单))。核对(第 1、2 条):DRL-49。
3. 新实例已按 [az-probe](az-probe.md) 选定的 AZ/placement group 启动(AZ 探测在 M1.5 之前已完成),AMI 与调优已就位,`/etc/hft/host-local.toml` 写 `manifest = "tokyo-cex-1"`,`hostprobe verify` 对本档期望剖面通过(含 busy poll 四项、IRQ 落点、managed IRQ 与 XPS,[host-tuning](host-tuning.md))。核对(第 3、4 条):DRL-50。
4. `[需授权]` **网卡、出口地址与存储**(改外部系统,AWS):启动模板按清单的设备序号为实例挂 ENI-B(交易)、ENI-C(sim)与启用时的 ENI-B2;ENI-B 的私有 IP 关联 prod 的 EIP,ENI-C 的私有 IP 关联一个新 EIP(sim 出口),ENI-B2 的私有 IP 关联一个不进任何白名单的 EIP;清单 `[egress]` 写入这些地址,生成器据此写 `irq-plan.json` 与 `/etc/hft/egress.toml`;`hft-egress-ip.sh` 在新机上退出 0([deploy 出口 IP](../../deploy/README.md#出口-ip))。在第二 AZ 预建待命的 ENI-B/ENI-C(不挂任何实例),其私有 IP 写进待命清单。冷副本卷按清单 `[storage]` 建独立 gp3 卷(容量、预置吞吐与 IOPS 按清单)。安全组只对 dev2(与启用时的第三 AZ 看门狗)的私有 IP 放行 watchtower 的 `healthz_port`。
5. `[需授权]` **IAM**:升级主机的实例角色与各消费者角色按 [deploy 秘密与实例角色](../../deploy/README.md#秘密与实例角色) 授权(秘密前缀、`AssumeRole`、SSM 托管策略、消费者的 S3 前缀与 KMS 都以该节为准);实例角色没有 EC2 写权限,也没有 S3 权限。
6. 当前正式版已在新机上 `releasectl install` 并按组激活 `core`、`cold`、`sim`(冷,单元均未启动;[release-and-deploy](release-and-deploy.md));该版本的验证包已 `releasectl install-verify`;`preflight.sh` 通过(含模拟实例、出口地址、IMDS 锁定与凭证代理各项)。
7. 两个实例的配置由 deploy 按本档生成:prod `/etc/hft/hft-core-prod.toml`(`[[threads]]` 绑核、`blobs.engine` 本档 EngineConfig、journal 段本档值、`src_ip` 取 `egress.toml` 的 `prod` 列表;`instance.feed_env = "prod"`、`instance.order_env = "testnet"`,**refdata 只含测试网账户**,用于延迟验收与演练);sim `/etc/hft/hft-core-sim.toml`(`instance.name = "sim"`,线程计划只含非隔离核,`src_ip` 取 `sim` 列表,refdata 只含 Sim 账户)。两个 `hft-core --config … --check` 都退出 0(含 `EgressIpOverlap` 校验)。journal-archiver 的 prod source 前缀为 `hft-testnet`(对象键 `hft-testnet/<host>/prod/…`,测试网数据不进正式前缀,规则归 [journal-archiver](../../apps/journal-archiver/README.md));其凭证覆盖 `hft/` 与 `hft-testnet/` 两个前缀([deploy IMDS 锁定](../../deploy/README.md#imds-锁定与凭证代理))。
8. **IP 白名单**:A 阶段的测试网 key 加入新机 prod 出口 EIP(两地同时有效);sim 出口 EIP 不加入任何白名单。逐 key 打勾记录在 ops-log。核对:DRL-52。
9. 秘密:`hft-secrets --check` 通过;`hft-sim` 用户读不到秘密 tmpfs;以非 root 用户访问 IMDS 被拒(`preflight.sh` 各项通过)。核对(第 5–7、9 条):DRL-51。

## 步骤

1. `[只读]` 记录 dev2 状态:`/status`、`/accounts`(每个模拟账户的期望模式、有效模式、生效 cap);导出每个模拟实例当前的参数版本(`/params <模拟实例>`)与覆盖项版本,新 sim 实例沿用;写进 ops-log。
2. `[需授权]` 新机启动 `cold` 组(`hft-imds-lock`、`hft-cred-broker` 先 active;`hft-kill-switch` 先于任何 hft-core active,再 watchtower、reconciler、journal-archiver、ops_bot、pipeline、treasury);ops_bot、pipeline、treasury、watchtower、journal-archiver 的配置列出本机的 `prod` 与 `sim` 两个实例。
3. `[需授权]` **停用 dev2 的 sim 实例**:对每个模拟账户 `/mode <模拟实例> <a> closeonly` → 无挂单后 `systemctl stop hft-core@sim`(Drain);期望 journal 末尾是 Drain 快照。dev2 上的模拟 journal 已在 `hft/<dev2 的 host>/sim/` 前缀归档,不搬到新机。
4. `[需授权]` **新机启动 sim**:`systemctl start hft-core@sim`(配置 `config_source = file` 首次启动)。期望:`Note{StartupStep}` 完成、只有 Sim 账户、没有任何下单连接与秘密读取、行情 `ConnState{Up}`;`ss -tnp` 显示 sim 的全部交易所连接源地址在 `egress.toml` 的 `sim` 列表内(ENI-C);`ps -L -o psr` 显示 sim 线程只在非隔离核。核对(第 1–4 步):DRL-53。
5. **模拟实例恢复**:pipeline 以第 1 步记录的参数版本为基线下发 `SetParams` 到 sim 实例,覆盖项按实例原样迁入(`overrides/<模拟实例>.toml`);按 [launch Model 预热](launch.md#model-预热) 对每个模拟实例预热(新 journal 没有检查点);`/mode <模拟实例> <a> live`(Trader + 确认)。期望:`/sim` 出现成交与日报。核对:DRL-54。
6. `[需授权]` **新机启动 prod(测试网账户)**:`systemctl start hft-core@prod`(首次启动,`config_source = file`)。第一分钟验收同 [release-and-deploy](release-and-deploy.md) 步骤 16:测试网账户每个 venue 的闸门全 1(含 `IDENTITY`——新机出口地址上的 key 调用成功)、`HostProfile` 无不符、心跳在推进;`ss -tnp` 显示 prod 的交易所连接源地址在 `prod` 列表内(ENI-B)。核对:DRL-55。
7. **延迟验收**(门槛 ⑩):开始前写持久的测量窗口标志 `sudo install -D -m 0644 /dev/null /var/lib/hft/measure/active` 并 `systemctl stop hft-verify.service`(标志让 `hft-verify` 的 `ExecCondition=` 跳过,定时器照常到点也不会启动,主机重启后仍生效,[deploy 验证包](../../deploy/README.md#验证包与-hft-verify))。先做同机干扰的 **A/A**:连续 1–2 天 `hft-core@sim` 停止(块的两半都关,其余配置与窗口相同),`watchtower interference --aa` 给出各组 R 的区间与半宽;区间不含 1 时先查明块内顺序或时段带来的偏差再开窗口;窗口初始长度按 A/A 半宽外推决定(7 天,超过 `max_halfwidth` 时加长,至多 14 天;规则归 testing §8.2 第 7 条)。之后 prod 与 sim 同时运行(最终形态),实时行情 + 测试网/模拟盘下单回路,按 [latency-budget](../latency-budget.md) 的分段口径采集**一个主网行情窗口**的 prod 主机内分段;判定(硬线 = 验收下限、目标与两者之间的 Admin 签字、按事件加权的离线统计与 `rep_to_order` 样本、时钟误差过滤)唯一归属 [testing §8.2](../testing.md#82-延迟验收的测法)。不另取测试网行情窗口;latency-probe 的 `pair` 模式只作小样本确认(可在本主机任意 ≥ 1 天的测试网行情时段或 dev2 联调窗口取样,只记录)。同期跑 [host-tuning](host-tuning.md) D 节的主机层测量。验收不过时按 host-tuning 一次改一项调优,不回退到 dev2。测量过程:DRL-56;⑩ 的判定:CORE-407。
8. **同机干扰**(与第 7 步同期;判据与统计方法唯一归属 testing ⑩):选一个种子并记进 ops-log,写主机本地文件 `/etc/hft/measure.toml`(`seed = <种子>`、`start = <窗口开始的 UTC 偶数整点>`、`segment_days = [<初始窗口天数>]`、`funding_hours_utc = [0, 8, 16]`;`start` 必须是偶数 UTC 整点,与 testing ⑩ 的块边界一致;窗口按段排程——初始窗口一段、每次延长在 `segment_days` 末尾追加一段——键的含义与按资金费块配平的实现归 [deploy `hft-sim-alternate`](../../deploy/README.md#实现说明)),`sudo systemctl enable --now hft-sim-alternate.timer`(发布内的正式定时器,`Persistent=true`,主机重启后照常继续,[deploy systemd 单元](../../deploy/README.md#systemd-单元)):从 `start` 起以 2 小时为一块,块内先开还是先关由种子随机决定,资金费块与普通块各自配平,每个整点把 sim 切到本小时应处的状态(停止走 Drain,启动从检查点恢复);`watchtower interference` 判某组区间过宽时窗口按 2 天一段延长,窗口总长至多 14 天;窗口结束后 `sudo systemctl disable --now hft-sim-alternate.timer` 并确认 sim 处于运行。按 testing ⑩ 以块配对计算 prod 的 `tick_to_trade`、`rep_to_order` 与 `hedge_host` 的 p99 比值及其置信区间;另核对 sim 线程从不落在隔离核与 IRQ 专用核(`ps -eLo psr,comm` 采样;watchtower 的 `IsolatedCoreIntrusion` 是可选扩展,启用时同时核对零次)、`hft-sim.slice` 的内存与 CPU 用量在上限内、sim 停止时 IRQ 专用核的 `NET_RX` softirq 不变。用例:DRL-57。
9. **演练**:在新机上用测试网账户按 [incident-response kill 演练](incident-response.md#kill-演练) 做 kill、死手、`kill-switch fire`、`/stop` 与 `stop_core` 兜底(新机的 gw kill 字、kill-switch 出口、polkit 授权与触发套接字都要验证)。用例:DRL-25。
10. **机外看门狗**:在 dev2 的主机本地文件 `/etc/hft/offbox-watchdog.toml` 中把目标设为新机 ENI-A 私有 IP 的 `/healthz` 并启动 `hft-offbox-watchdog`([deploy 机外看门狗](../../deploy/README.md#机外看门狗));演练:临时从安全组移除 dev2 的放行或停 watchtower → ≤ 75 s 内 Telegram 与电话告警发出(判定时长加一个探测周期,取值归 deploy);恢复 → 恢复通知(DRL-26);在新机上 `sudo hft-maint start 10` 后再停一次 watchtower → 只有 Telegram、没有电话,截止后恢复电话告警(DRL-27)。[az-probe](az-probe.md) 的结论为升级主机与 dev2 同 AZ 时,`[需授权]` 在第三 AZ 开 `t4g.nano` 运行同一个看门狗并同样演练(DRL-28)。
11. 第 7 步的测量窗口(含延长)结束后,`sudo rm /var/lib/hft/measure/active` 恢复验证包运行(只到 M8 开始为止,[deploy 验证包](../../deploy/README.md#验证包与-hft-verify))。随后(都在 ⑩ 窗口之外):
    - **并发存储测量**:让 journal-archiver 补传积压、`hft-replay-verify` 运行、sim 写检查点同时进行 ≥ 1 h,记录 prod 的 persist 滞后 p99 与冷盘 `iostat`;超过 hft-core 的 `Storage` 阈值的一半 → 调高清单 `[storage]` 的 gp3 吞吐/IOPS 或调整批处理与 sim 的 I/O 带宽上限(取值归 hft-core)。用例:DRL-58。
    - **换 AZ 恢复演练** `[需授权]`(测试网账户;在 ⑦ 满规模复核开始之前):停原实例,按 [incident-response S](incident-response.md#s-交易主机失联) 第 1–3 步在第二 AZ 用待命 ENI-B/ENI-C、待命清单、新的冷副本卷与 `journal-archiver restore`([journal-archiver 从 S3 恢复](../../apps/journal-archiver/README.md#从-s3-恢复restore))重建并以 `--recover-from-snapshot` 启动 prod,sim 最后启动;再按同样的步骤迁回原 AZ(原实例换新的冷副本卷后 `restore`),终止第二 AZ 的实例、待命网卡回到未挂载。用例:DRL-60。
    - **⑦ 满规模复核**:sim 实例以满规模连续运行 ≥ 7 天,并以最终参数重跑压力用例(规则归 testing §8 ⑦;可与 launch D 的前期步骤并行)。用例:DRL-59。

    验收通过后,prod 保持只含测试网账户运行或 Drain 停止,等待 [launch D](launch.md#d-正式实盘m8)(正式子账户开通、切到正式配置)。
12. dev2 收尾 `[需授权]`:dev2 的 `hft-core@sim.service` 改为不开机自启;dev2 保留测试网联调(`hft-window` 才启动 `hft-core@prod`)、`hft-verify` 常态运行与机外看门狗。
13. 全部测试网 key 的白名单删除不再需要的 dev2 地址(若 dev2 不再联调)。
14. deploy 的主机清单与 runbook 中的"当前主机"改为新主机;ops_bot、watchtower 的实例标签改为新主机名。
15. (可选)dev2 不再需要时:`releasectl uninstall` 卸载全部 `hft-*` 单元、slice 与守护(不影响旧系统);测试网 key 的白名单删除 dev2 地址。正式实盘期间不卸载:机外看门狗与验证包累计都在 dev2 上。核对(第 12–15 步):DRL-61。

## 验收

- 主机档位的实测结论已记录;两个实例的线程计划与内存在本档内,生成器与 `--check` 无错误。
- sim 实例只含 Sim 账户、进程环境与文件系统中没有任何交易所 key;sim 的交易所连接全部来自 ENI-C 的出口地址,sim 线程从不在隔离核与 IRQ 专用核上;模拟账户恢复成交与日报。
- prod 冷启动 < 30 s,闸门全部通过;prod 的 refdata 只含测试网账户(正式账户在 launch D 才加入)。
- 门槛 ⑩ 按 [testing §8.2](../testing.md#82-延迟验收的测法) 判定通过(落在目标与硬线之间时有 Admin 签字);窗口前的 A/A 有记录;同机干扰满足 testing ⑩ 的判据。
- 并发存储测量的 persist 滞后 p99 有记录且在 hft-core 阈值内;sim 满规模复核 ⑦ 通过。
- 新机上的演练记录完整;换 AZ 恢复演练(去程与迁回)通过;机外看门狗、维护静默与电话告警演练通过;第二 AZ 的待命网卡与待命清单就绪。
- (若卸载 dev2)卸载后旧系统的进程与资源使用回到 hft 运行前的基线。

## 回退

- 新机启动失败或第一分钟验收不过:停新机的 hft 单元,修配置后重试;没有正式资金在新机上,不需要回退到 dev2 才能继续交易。
- 需要继续模拟长跑时:在 dev2 上重新启动 `hft-core@sim`(从 `initial_balances` 开始或沿用 dev2 的 journal),新机修好后再按第 3–5 步切换。同一时刻只运行一个 sim 实例,避免两个"sim"实例在 ops_bot 与 pipeline 中同名。
- 延迟验收不过**不是**回退理由:功能正确时留在新机,按 host-tuning 调整;主机档位不够时换更大的档位(重新走前置条件第 2–4 步)。
- sim 实例的任何故障不影响 prod:停 sim、修配置后再启。

## 记录

`ops-log/<date>-host-upgrade.md`:主机档位的依据(M3 末探针数据与结论)、新主机的实例 ID、AMI、发布版本、ENI-B/ENI-C(与启用时的 ENI-B2)与出口地址(私有 IP 与 EIP 分配 ID)、第二 AZ 待命网卡、各步 UTC 时间、dev2 sim 停用与新 sim 启动的时间与参数版本、延迟验收的报告、A/A 结果、窗口初始长度与每个延长段的起止、同机干扰的种子、块配对比值与置信区间、并发存储测量、换 AZ 恢复演练的时间线与耗时、⑦ 满规模复核的起止、演练记录、机外看门狗演练记录、IP 白名单变更清单(按 key ID)、授权人;各项按 [drills](../tests/drills.md) 的 `DRL-n` 证据归档。
