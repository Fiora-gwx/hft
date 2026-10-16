---
description: "部署与主机调优即代码:只安装 CI 产出的发布制品与验证包;主机清单(网卡按设备序号、MAC 只在运行期核对,第二 AZ 待命清单,M7 与 M8 两份清单切换 `verify`,M3 末探针清单,预留的 ENI-B2)→ GRUB/sysctl/网卡/IRQ/XPS/hugepages/resctrl/tuned/systemd 单元(核号由 hft-core 线程计划生成);中断驱动收包(busy poll 全关、交易网卡 IRQ 独占一核);slice 切分(根级 hftbatch.slice + CPUWeight=idle)与冷盘 I/O 带宽上限;hft-core@prod 与 hft-core@sim 两个实例的部署形态;身份、路径与 S3 前缀注册表(权限闭包:路径表 × 用户表 × 附加组);dev2 护栏(停机信号读 system.slice 压力且 hft 自身用量明显、tripped 标志、只自动重启 sim)与联调窗口标志;回放校验期间暂停 archiver/pipeline;验证包与 hft-verify 单元;kill-switch 本地触发、停机标记目录与 polkit;秘密按单元投递;IMDS 锁定(升级主机 cgroup BPF、dev2 只挡 hft 各 uid)与凭证代理;机外看门狗与维护静默;AMI、按组发布(没有 /opt/hft/current)与 releasectl、激活时 hostprobe 校验"
kind: dir
tier: cold
scope: core
status: spec
---

# deploy

## 概要

`deploy/` 拥有"一台主机从 AMI 到能跑 hft-core"所需的一切可复现产物:主机清单(每台主机一份 TOML,网卡、出口地址与主机类的唯一来源)、由清单与 hft-core 线程计划生成的内核启动参数、cpuset、IRQ/XPS 计划与 `/etc/hft/egress.toml`、运行期调优脚本、tuned 与 chrony 配置、全部 `hft-*` systemd 单元与 slice(hft-core 是模板单元 `hft-core@<instance>.service`)、`dev2` 护栏(slice 上限、只按资源信号动作的守护、联调窗口脚本)、验证包的安装与 `hft-verify` 单元、polkit 规则、按单元的秘密清单、IMDS 锁定与凭证代理、机外看门狗与维护静默、AMI 构建模板、发布归档的打包/获取/安装/激活脚本(沿用 Trading 仓库的 releasectl 流程),以及激活时的 `hostprobe` 校验。**服务器只安装 CI 产出的制品**:不在服务器上编译、不装工具链([development](../docs/development.md#仓库与-ci))。系统用户与组、安装与运行路径、S3 前缀的唯一登记处是[身份、路径与 S3 前缀](#身份路径与-s3-前缀)。它不拥有应用配置的语义(各 app README)、资源数值(线程计划、核号、内存与 tmpfs 大小、slice 上限、I/O 带宽上限、`OOMScoreAdjust` 与 dev2 护栏的全部数值唯一归属 [hft-core](../apps/hft-core/README.md#主机资源隔离slice核与出口-ip))与操作步骤(runbooks)。规则见 [deploy/AGENTS.md](AGENTS.md)。

每台主机上有两个 hft-core 实例(同一二进制、同一模板单元):`hft-core@prod` 与 `hft-core@sim`([模拟实例](#模拟实例sim))。`dev2` 不放正式实盘账户:`sim` 常驻,`prod` 只在交易所测试网/模拟盘联调窗口运行、只持测试网 key([no-small-live](../.agents/notes/proposed/architecture/2026-10-03-no-small-live.md));统一升级后的主机上 `prod`(M7 测试网下单做延迟验收,M8 起正式实盘)与 `sim` 同机,sim 只用非隔离核、独立 slice、独立网卡 ENI-C 与独立出口 IP([sim-colocated-with-prod](../.agents/notes/proposed/architecture/2026-10-03-sim-colocated-with-prod.md)、[rx-irq-driven](../.agents/notes/proposed/architecture/2026-10-03-rx-irq-driven.md))。

## 目录

- [使用本组件](#使用本组件)
  - [目录布局](#目录布局)
  - [制品从哪里来](#制品从哪里来)
  - [主机清单](#主机清单)
  - [GRUB](#grub)
  - [运行期调优](#运行期调优)
  - [出口 IP](#出口-ip)
  - [slice 与资源切分](#slice-与资源切分)
  - [dev2 护栏](#dev2-护栏)
  - [hugepages 与 hugetlbfs](#hugepages-与-hugetlbfs)
  - [resctrl / CAT](#resctrl--cat)
  - [systemd 单元](#systemd-单元)
  - [身份、路径与 S3 前缀](#身份路径与-s3-前缀)
  - [秘密与实例角色](#秘密与实例角色)
  - [IMDS 锁定与凭证代理](#imds-锁定与凭证代理)
  - [模拟实例(sim)](#模拟实例sim)
  - [验证包与 hft-verify](#验证包与-hft-verify)
  - [机外看门狗](#机外看门狗)
  - [AMI 构建](#ami-构建)
  - [发布归档与 releasectl](#发布归档与-releasectl)
  - [单元组与分组激活](#单元组与分组激活)
  - [HostProfile](#hostprofile)
- [实现说明](#实现说明)
- [延迟与资源契约](#延迟与资源契约)
- [测试与验收](#测试与验收)
- [依赖](#依赖)
- [延伸阅读](#延伸阅读)
- [已知限制与后续工作](#已知限制与后续工作)

## 使用本组件

### 目录布局

```
deploy/
  AGENTS.md  README.md
  hosts/            dev2-1.toml、tokyo-cex-1.toml(统一升级主机 M7 用,verify = true;默认 c32)、tokyo-cex-1-live.toml(M8 起用,verify = false)、
                    <清单名>.standby.toml(第二 AZ 待命清单,每份主清单一份)、probe-c16.toml 与 probe-c32.toml(M3 末满订阅探针)、research-1.toml;
                    test-<class>.toml(每个主机类一份,只供生成器与 CORE/DEP/TOOL 用例,不对应任何主机)
  gen/              生成器(由 cargo xtask gen-host 调用):清单 + hft-core 线程计划与内存表 → grub.cfg 片段、各 slice 的 cpuset 与 I/O 上限、
                    irq-plan.json(含 XPS)、两个实例配置的 [host]/[[threads]]/src_ip 段、/etc/hft/egress.toml、watchtower 的隔离核线程白名单、
                    期望剖面 tuning/<清单名>.toml
  grub/             50-hft.cfg.tmpl(GRUB_CMDLINE_LINUX_DEFAULT 模板;按 ISA 分 aarch64 与 x86 两份)
  sysctl/           90-hft.conf
  tuned/            hft-latency/tuned.conf
  net/              hft-eni-attach.sh(按设备序号找到 ENI-B/ENI-C/ENI-B2,配地址与源路由)、hft-net-tune.sh(ethtool/队列/合并/卸载/环/NAPI/XPS)、
                    hft-egress-ip.sh
  irq/              hft-irq-pin.sh、hft-workqueue.sh
  imds/             hft-imds-lock.sh(load|status)、hft-imds-connect.bpf.c(dev2 以外的主机类:cgroup connect4/connect6 钩子;对象文件由发布工作流编译进归档)、
                    hft-imds-dev2.nft(dev2:只丢弃 hft 各 uid 发往 IMDS 的包)
  creds/            hft-cred-broker(凭证代理)、consumers.toml.tmpl(消费者 → 角色与前缀)
  hugepages/        dev-hugepages2M.mount
  tmpfs/            dev-shm-hft-prod.mount、dev-shm-hft-sim.mount(热段专用 tmpfs;大小取 hft-core)
  resctrl/          hft-resctrl.sh
  chrony/           chrony.conf(PHC refclock)
  udev/             60-hft.rules(块设备调度器;升级主机类另把 /dev/cpu_dma_latency 设为 root:hft 0660)
  bin/              hft-unit-idle(ExecCondition 用:列出的单元有任一 active 即返回 1)、hft-shm-release(干净 Drain 后释放 tmpfs 热段)、
                    hft-sim-alternate(M7 同机干扰测量:按记录的种子决定每个整点 sim 运行或停止)、hft-maint(机外看门狗的维护静默)
  systemd/          hft-core@.service、其余 hft-*.service/.timer(含 hft-replay-resume.service、hft-sim-alternate.service/.timer)、hft.slice、
                    hft-core.slice、hft-sim.slice、hftbatch.slice(根级)、hft-*.target
  dev2/             各单元与 slice 的 dev2 覆盖文件(*.d/50-dev2.conf)、hft-dev2-guard(守护脚本,含 clear 子命令)与其单元、dev2-guard.toml.tmpl、
                    hft-window(联调窗口 start|stop)、hft-tune-dev2.sh(可选,默认不用)、tests/(护栏守护与 hft-shm-release 的测试)
  offbox/           hft-offbox-watchdog(机外看门狗,Python 标准库;装在 dev2)与 offbox-watchdog.toml.example(主机本地配置的样例)
  polkit/           50-hft-dev2-guard.rules(守护停白名单内的 hft-* 单元,另只可启动 hft-core@sim)、50-hft-kill-switch.rules(只停 hft-core@prod)
  secrets/          每个取秘密的单元一份 hft-secrets 清单模板(<unit>.toml.tmpl)与 IAM 策略模板(iam-dev2.json.tmpl、iam-prod.json.tmpl、
                    iam-consumers.json.tmpl)
  config-templates/ 冷进程配置模板(watchtower.toml.tmpl、reconciler.toml.tmpl)与 ctl-principals.toml.tmpl;随发布归档为 config-templates/
  ami/              tokyo-cex.pkr.hcl、provision/*.sh
  release/          package-release.sh、package-verify.sh、fetch-release.sh、releasectl.sh、install-release.sh、activate-release.sh、
                    rollback.sh、preflight.sh、record-deployment.py(改编自 Trading/deploy/scripts)
  grafana/          看板 JSON
```

[`tools/hostprobe`](../tools/README.md#hostprobe) 与 [`tools/hft-secrets`](../tools/README.md#hft-secrets)(冷工具)的源码与规格在 tools/,二进制随发布归档。

### 制品从哪里来

- 代码仓库与 CI 的规则归 [development.md](../docs/development.md#仓库与-ci):CI 只用 GitHub 托管的免费 runner,质量门在 `ubuntu-latest`,aarch64 制品与验证包在 `ubuntu-24.04-arm` 原生构建(应急时由 `release-x86-cross.yml` 交叉编译,同一归档格式);CI 从不连接服务器。
- 三类制品,都发布到公开仓库 `Fiora-gwx/hft` 的 Release,服务器用 `fetch-release.sh` 拉取:

| 类别 | 版本形如 | 来源 | 可以用于 |
|---|---|---|---|
| 正式版 | `vX.Y.Z` | 在通过 `main` CI 与当夜任务的提交上打带注释标签;标签不移动;发布工作流 `preflight` 核验 | 任何实例,包括持正式实盘账户(`is_prod = true`)的 `prod` |
| 开发构建 | `vX.Y.Z-dev.N`(Git SHA 在 `RELEASE.json`) | 对 CI 全绿的 PR 头提交经 `workflow_dispatch` 产出,或在 `main` 提交上打标签;pre-release,只保留最近 10 个 | 只用于 `dev2`:`sim` 实例与测试网联调用的 `prod` 实例(`order_env = "testnet"`,账户全部 `is_prod = false`);**含正式账户的配置拒绝启动开发构建**(`DevBuildWithProdAccount`,由 hft-core 启动检查) |
| 验证包 | `hft-verify-<ver>-aarch64.tar.zst`(随正式版;另可对任一已有标签的全绿提交单独产出) | `verify-bundle.yml`([development](../docs/development.md#验证包工作流verify-bundleyml)) | 只由 `hft-verify.service` 运行;不含任何生产二进制,安装不影响生产版本激活([验证包与 hft-verify](#验证包与-hft-verify)) |

- 前两类走同一条安装路径(校验 SHA-256、`RELEASE.json`、逐文件哈希、冷激活);验证包用 `releasectl install-verify`。`channel = "dev"` 的归档只能装在本机清单 `class = "dev2"` 的主机上,其他主机上 `releasectl install` 退出 1。服务器上没有 `cargo`、`rustc` 或任何构建工具,`preflight.sh` 检查到它们即失败(检查的命令名见[发布归档](#发布归档与-releasectl)的预检一行)。
- **租用实例**:M1.5 延迟探针与参考机([development 参考机](../docs/development.md#参考机))只解包运行 CI 产出的制品(基准制品等),不经 `releasectl install`、不需要主机清单与 `host_expect`;调优按 [host-tuning](../docs/runbooks/host-tuning.md) C 节逐项核对。**M3 末满订阅探针**要按 c16(或 c32)线程计划带隔离核、大页、IRQ 专核与 `egress.toml` 运行 prod 与 sim,因此按正式版安装:清单 `hosts/probe-c16.toml`、`hosts/probe-c32.toml`(`class` 分别为 `c16`、`c32`,`formal_live = false`、`verify = false`,`[egress]` 是为探针预建的 ENI-B/ENI-C 的固定私有 IP 与 EIP)随每个正式版发布,MAC 不进哈希,所以每次租用新实例不需要先发版;步骤在 [host-tuning](../docs/runbooks/host-tuning.md)。"开发构建只用于 dev2"的规则不受影响。

### 主机清单

`hosts/<清单名>.toml` 的字段即生成器输入。**清单不写核号**:隔离核、IRQ 核、hk 核、warm/冷核与 sim 实例的核全部由生成器按 `class` 从 [hft-core 线程计划](../apps/hft-core/README.md#线程计划)(含 prod + sim 同机的计划)算出,不手写。**清单不写 MAC**:网卡按启动模板的设备序号(`device_index`)识别,MAC 只在运行期经 IMDS 读出、写进 HostProfile,不进清单与发布哈希。下例是统一升级主机(`dev2` 清单见 [dev2 护栏](#dev2-护栏);可选的 x86 `metal` 用同一字段,另有 x86 GRUB 模板):

```toml
schema = 4
host = "tokyo-cex-1"                   # 逻辑主机名:S3 对象键的主机段与实例标签;待命清单与它相同
instance_type = "c8g.8xlarge"          # 默认 c32;c16(c8g.4xlarge)只在 M3 末满订阅探针达标且启用 epoll_pwait(0) 时保留为候选
az_id = "apne1-az4"                    # 以 AZ 探测报告为准(示例)
placement_group = "hft-cpg-az4-a"
kernel_min = "6.12"
ena_driver = "2.17.3"
class = "c32"                          # 主机类(dev2 | c4 | c8 | c16 | c32 | metal);生成器据此选 hft-core 的线程计划与内存档
instances = ["prod", "sim"]            # 本机运行的 hft-core 实例
formal_live = true                     # 本机允许正式实盘账户;dev2 与探针 = false(见"秘密与实例角色")
replay_verify = true                   # 是否安装 hft-replay-verify.service/.timer(缺省 true;dev2 退路 (a) 的原 dev2 写 false)
verify = true                          # 是否安装 hft-verify.service/.timer;本清单是 M7 用的 tokyo-cex-1,M8 起用 tokyo-cex-1-live(false)
[storage]
cold_gib = 1100                        # ≥ hft-core 的冷盘容量(公式与数值归 hft-core 内存节;1 TB = 10¹² B)
gp3_throughput_mibps = 500             # 冷盘 gp3 预置吞吐(初值;M7 并发测量后定,费用口径归 11 §8)
gp3_iops = 6000                        # 冷盘 gp3 预置 IOPS(同上)
[hugepages]
derive = true                          # 页数按 hft-core 本档 prod 的 journal 列表与引擎区求和推导(见 hugepages 节;sim 不用大页)
[eni_b]                                # 交易网卡:prod 的交易所连接;全部队列 IRQ 钉在 IRQ 专用核
device_index = 1                       # 启动模板中的挂载序号
queues = "max"                         # ENA 队列数取实例上限(M1.5 核实);XPS 按热核分配
[eni_c]                                # sim 网卡:sim 的全部交易所连接;队列 IRQ 钉在 sim 的 IRQ 核
device_index = 2
queues = 1
[eni_b2]                               # 预留的第二块交易网卡(批量行情与 REST);默认不启用,只在 c32/metal 上可启用
enabled = false
device_index = 3
queues = "max"
[egress]                               # 见"出口 IP";每项 = 一个私有 IP + 其关联的 EIP 分配 ID
prod = [{ eni = "b", ip = "10.0.1.10", eip_alloc = "eipalloc-…" }]   # 可多个(按 (venue, account) 分配,规则归 hft-core 多账户)
sim  = [{ eni = "c", ip = "10.0.1.20", eip_alloc = "eipalloc-…" }]   # ENI-C 的私有 IP + EIP,只给 sim 实例
[resctrl]
enabled = false                        # Graviton 上的可用性待核实(方法见开发备忘)
```

- ENI-A 是实例的主网卡(管理流:SSH、S3、GitHub、Telegram、pipeline/treasury 的公共接口),不写进清单;ENI-B、ENI-C(与启用时的 ENI-B2)在实例创建时由启动模板按 `device_index` 挂上(`[需授权]` 的 AWS 操作,[host-upgrade](../docs/runbooks/host-upgrade.md)),实例角色没有 EC2 写权限;主机上的脚本经 IMDS(root)按设备序号读出 MAC,只配置地址与路由并只读核对。
- **本机选用哪份清单**:主机本地文件 `/etc/hft/host-local.toml`(不随发布,由人在开机后写一次)的 `manifest = "<清单名>"`;安装校验 `RELEASE.json.host_expect[<清单名>]`([发布归档](#发布归档与-releasectl)),hostprobe 读同一清单的期望剖面。
- **M7 与 M8 两份清单**:统一升级主机在同一发布里有两份主清单——`tokyo-cex-1`(M7 测试网延迟验收,`verify = true`)与 `tokyo-cex-1-live`(M8 起正式实盘,`verify = false`);两者只有 `verify` 不同(生成器核对其余字段相同),`host` 同为 `tokyo-cex-1`。M8 切换只改 `host-local.toml` 的 `manifest` 后重新激活 `core` 组,不另发补丁版(步骤在 [launch D-23](../docs/runbooks/launch.md#d-正式实盘m8))。
- **第二 AZ 待命清单** `hosts/<清单名>.standby.toml`(每份主清单一份):`host`、`class`、`formal_live`、`verify`、`instances` 与主清单相同,`az_id`、`placement_group` 与 `[egress]` 的私有 IP 属于第二 AZ 里预建的 ENI-B/ENI-C(`[需授权]` 预建,平时不挂任何实例);EIP 分配 ID 与主清单相同(换 AZ 时把同一 EIP 重新关联到待命网卡,key 白名单不变)。每个正式版的 `host_expect` 含 `hosts/` 下全部清单(主清单、待命清单与探针清单),换 AZ 重建只需"挂网卡 + 安装当前正式版"([incident-response S](../docs/runbooks/incident-response.md#s-交易主机失联))。
- **`verify`**:`true` 时生成器安装 `hft-verify.service`/`.timer`;`false` 时不安装,`preflight.sh` 核对 `systemctl show -p LoadState hft-verify.service hft-verify.timer` 均为 `not-found`([验证包与 hft-verify](#验证包与-hft-verify))。
- **`[eni_b2]`(预留,默认 `enabled = false`)**:启用条件是 M3 末满订阅探针中 BBO 的 `nic_to_kernel` p99 在批量推送的突发窗口内显著上升(阈值由探针的 Agent Note 定,[irq-split-reserve](../.agents/notes/proposed/architecture/2026-10-03-irq-split-reserve.md))。启用后:ENI-B2 全部队列的 IRQ 落在 hft-core 线程计划预留的 IRQ 核(`host.irq_cpus.eni_b2`,`c32` 为核 28),只承载 `prod` 的批量流(深度、成交、标记价、REST 快照)连接,延迟敏感流(BBO、下单、用户流)留在 ENI-B;它的出口地址写在单独的列表 `[egress].prod_bulk`(每项 `{ eni = "b2", ip, eip_alloc }`);**这些 EIP 不进任何 key 的白名单**;哪些连接绑它归 hft-core 配置(`src_ip`;`gw:` 与 `acct-fetch` 不得使用)。只在线程计划为它预留了核的主机类上允许启用。
- 生成物:`grub.cfg` 片段(见 [GRUB](#grub))、各 slice 的 `AllowedCPUs` 与 I/O 带宽上限、`irq-plan.json`(ENI-A → hk 核、ENI-B 全部队列 → `eni_b` 核、ENI-C 全部队列 → `eni_c` 核、启用时 ENI-B2 → `eni_b2` 核;ENI-B 的 XPS 映射)、两个实例配置 `/etc/hft/hft-core-<instance>.toml` 的 `[host]` 段、`[[threads]]` 与 `src_ip` 项、`/etc/hft/egress.toml`、watchtower 的隔离核线程白名单、期望剖面。生成器在以下情形失败:清单有未登记的键(含任何核号或 MAC 字段;`dev2` 只允许下一条列出的键,`c4` 不允许 `[eni_c]`、`[eni_b2]`);sim 实例的任一核在隔离集合内或与 prod 的热核重叠;两个实例的线程计划合计超出本机核数或内存档;`[egress]` 的各列表(`prod`、`prod_bulk`、`sim`)有交集;`prod` 的地址不在 ENI-B、`prod_bulk` 的地址不在 ENI-B2 或 `sim` 的地址不在 ENI-C(`dev2` 除外);同机的升级主机类缺 `[eni_c]`;`[eni_b2].enabled = true` 而线程计划没有 `eni_b2` 核;待命清单与其主清单的 `host`/`class`/`formal_live`/`verify`/`instances` 有任一不同;`tokyo-cex-1` 与 `tokyo-cex-1-live` 除 `verify` 外有任一字段不同。
- `class = "dev2"` 的清单只有 `schema`、`host`、`instance_type`、`class`、`instances`、`formal_live = false`、`verify`、`replay_verify`、`[eni_b]`(唯一网卡名)与 `[egress]`(见[出口 IP](#出口-ip)),生成器不产生 GRUB 片段、隔离 cpuset 与 IRQ 计划,另产生 [dev2 护栏](#dev2-护栏)的单元与守护配置。`class = "c4"`(联调可选,不是升级主机类,[hft-core `c4` 计划](../apps/hft-core/README.md#c44-物理核联调可选))**没有 IRQ 专核**:清单不写 `[eni_c]`、`[eni_b2]`,生成器产生 GRUB 片段、隔离 cpuset 与大页(hft-core 不要求它们,hostprobe 只记为 `advisory`,见 [HostProfile](#hostprofile)),但不按升级主机布局生成 IRQ 计划——`irq-plan.json` 只把全部 IRQ(含 ENI-B)钉到 hk 核、没有 XPS 映射,`host.irq_cpus` 不出现,不装 `hft-sim.slice`;`system.slice`/`user.slice` 只用 hk 核。升级主机类 = 换清单与生成物,不改代码、不重新构建([host-upgrade](../docs/runbooks/host-upgrade.md))。

### GRUB

由 `grub/50-hft.cfg.tmpl` + 清单 + 线程计划生成;只用于 `c4`/`c8`/`c16`/`c32`/`metal`,`dev2` 不改内核参数。基线项(按 ISA 取舍,x86 专属项不进 aarch64 模板):

```
audit=0 nmi_watchdog=0 nowatchdog nosoftlockup
nohz=on skew_tick=1 isolcpus=domain,managed_irq,<hot> nohz_full=<seq 核> rcu_nocbs=<hot>,<IRQ 核> rcu_nocb_poll irqaffinity=<hk 核>,<IRQ 核>
default_hugepagesz=1G hugepagesz=1G hugepages=<n1g> hugepagesz=2M hugepages=<n2m>
# aarch64(Graviton,基准形态)另加:transparent_hugepage=madvise
# x86(metal)另加:transparent_hugepage=never mce=ignore_ce acpi_irq_nobalance clocksource=tsc tsc=reliable cpuidle.off=1 intel_idle.max_cstate=0
#   processor.max_cstate=0 idle=poll intel_pstate=disable nosmt mitigations=off pcie_aspm=off
```

`<hot>` 只含 prod 实例的 `Spin` 线程核(隔离集合,取自 hft-core 线程计划);sim 实例的核与 IRQ 核不在其中。`<IRQ 核>` = ENI-B 的 IRQ 专用核(升级主机类为核 1;启用 ENI-B2 时另加 `eni_b2` 核):它进 `rcu_nocbs` 与 `irqaffinity`,不进 `isolcpus` 与 `nohz_full`。`nohz_full` 只给 sequencer 核。THP 在 aarch64 上为 `madvise`、x86 为 `never`(规则 [hot-path-rules M2](../docs/hot-path-rules.md))。**`c4`**(联调可选,没有 IRQ 专用核,线程计划归 hft-core):`isolcpus`/`rcu_nocbs` = 核 1–3,`irqaffinity=0`,全部 IRQ(含 ENI-B)落在核 0;不装 ENI-C 与 `hft-sim.slice`。来源:[02 §4.1](../design/02-硬件与托管方案.md);Graviton 的差异项(THP `madvise`、无 C/P-state 项)见 [02 §4.3](../design/02-硬件与托管方案.md) 与 [research/05 §4.3](../design/research/05-hardware-hosting-os-tuning.md)。GRUB 改动需要重启,激活后 hostprobe 标 `pending_reboot`。

### 运行期调优

全部是 `hft-host-tune.service`(oneshot,开机在 `network-pre` 之后、任何 `hft-*` 应用单元之前)调用的幂等脚本,只安装在 `c4`/`c8`/`c16`/`c32`/`metal`;每项的预期效果与出处见 [02 §4.2](../design/02-硬件与托管方案.md)。标 **hard** 的项由 hostprobe 列为 `required`,不符时 hft-core 以退出码 3 拒绝启动([HostProfile](#hostprofile))。

| 项 | 设置 | 脚本 |
|---|---|---|
| irqbalance | 关 | `irq/hft-irq-pin.sh` |
| IRQ 落点(**hard**) | 升级主机类:ENI-A → hk 核(核 0);**ENI-B 全部队列 → IRQ 专用核(`host.irq_cpus.eni_b`,该核不放任何线程)**;启用时 ENI-B2 → `eni_b2` 核;ENI-C 全部队列 → `eni_c` 核;其余可迁移 IRQ → hk 核;`c4`:全部 IRQ → 核 0。NVMe 等 managed IRQ:`isolcpus=managed_irq` 只把它们排除出隔离核,hostprobe 另核对每个 managed IRQ 的 `effective_affinity` 不含 IRQ 专用核;IRQ 专用核上没有任何可运行任务(TASK_ON_HOT 的检查范围含 IRQ 核)。核号来自 `irq-plan.json` | `irq/hft-irq-pin.sh` |
| workqueue | 所有 workqueue cpumask → hk 掩码;开机后一次、5 s 后重放一次 | `irq/hft-workqueue.sh` |
| 网卡地址 | 按设备序号找到 ENI-B、ENI-C(与启用时的 ENI-B2),为每个私有 IP 建源地址策略路由;经 IMDS 只读核对网卡已挂载并读出 MAC | `net/hft-eni-attach.sh` |
| 出口地址 | 把 `[egress]` 的私有 IP 加到对应网卡并核对 EIP 关联(见[出口 IP](#出口-ip)) | `net/hft-egress-ip.sh` |
| 队列 / 合并 / 卸载 / 环 | `ethtool -L combined <queues>`(`max` = 实例上限);`-C adaptive-rx off rx-usecs 0 tx-usecs 0`;`-K gro off lro off tso off gso off sg off`;`-G rx 4096 tx 1024`(ENI-B、ENI-C、ENI-B2) | `net/hft-net-tune.sh` |
| NAPI(**hard**) | ENI-B:`/sys/class/net/<eni-b>/napi_defer_hard_irqs = 0`、`gro_flush_timeout = 0`(ENI-C 与 ENI-B2 同值) | `net/hft-net-tune.sh` |
| busy poll(**hard**) | `net.core.busy_poll = 0`、`net.core.busy_read = 0`;hft-core 的 conn-builder 另对每个 socket 显式设 `SO_BUSY_POLL = 0`([hft-net](../crates/hft-net/README.md)) | `sysctl/90-hft.conf` |
| RPS/XPS | RPS 全关;**XPS 按热核分配**:ENI-B 的每个 TX 队列映射给一组发送热核(gw、feed、seq 所在核;映射写在 `irq-plan.json`,队列数不少于发送热核时一核一队列),避免多个热核争同一 TX 队列锁;并发发送下的 `egress` 尾部待核实(方法:M3 末满订阅探针上对比 XPS 映射前后的 `egress` p99) | `net/hft-net-tune.sh` |
| qdisc | `default_qdisc=noqueue`;ENI-B、ENI-C、ENI-B2 `mq` | 同上 |
| TCP | `tcp_autocorking=0`、`tcp_slow_start_after_idle=0`、`tcp_no_metrics_save=1`;**不设** `tcp_timestamps=0`、`tcp_sack=0`、`tcp_fastopen=3` | `sysctl/90-hft.conf` |
| 缓冲 | `rmem_max`/`wmem_max`/`optmem_max` 16 MiB、`netdev_max_backlog=1000000`、`netdev_budget=600` | 同上 |
| 内存 | `swapoff -a`、`vm.swappiness=0`、`numa_balancing=0`、`compaction_proactiveness=0`、`min_free_kbytes=1000000`、`dirty_background_bytes=33554432`、`dirty_bytes=268435456`、`stat_interval=120`;KSM 关 | 同上 |
| 调度/杂项 | `timer_migration=0`、`watchdog=0`、`printk=3 3 3 3` | 同上 |
| 电源 | 性能调速器;`/dev/cpu_dma_latency` 只由升级主机类上的 `hft-core@prod` 打开并写 0(dev2 与 sim 实例不打开,规则归 hft-core;udev 规则把该设备设为 `root:hft` `0660`) | `tuned/hft-latency`、`udev/60-hft.rules` |
| 存储 | NVMe/EBS 调度器 `none`、`iostats=0`、`add_random=0`(调度器为 `none` 时 `io.weight` 与 `IOSchedulingClass` 不生效,磁盘隔离只用 I/O 带宽上限,见 [slice](#slice-与资源切分)) | udev 规则 |
| 系统 | 禁用 `snapd`、`unattended-upgrades`、`apport`、`amazon-cloudwatch-agent`、`motd-news`、`systemd-oomd`、`multipathd`;不安装 `ec2-instance-connect`(它以非 root 用户查询 IMDS,IMDS 锁定后失效);**保留 `amazon-ssm-agent`** 作破窗通道(在 `system.slice`,只落在非隔离核) | AMI provision |
| 时钟 | chrony:`refclock PHC /dev/ptp_ena poll 0 delay 0.000010 prefer`;link-local NTP 作备 | `chrony/chrony.conf` |
| 不做 | PREEMPT_RT、ENA Express、Onload-on-ENA、`SCHED_FIFO` 热线程、任何 busy poll | — |

### 出口 IP

交易所按出口 IP 计连接数与请求权重(各所口径待核实,方法见开发备忘)。prod 与 sim 同机时各用自己的网卡与出口地址;地址来自清单 `[egress]`,生成器写成 `/etc/hft/egress.toml`(`{prod = [...], sim = [...]}`,启用 ENI-B2 时另有 `prod_bulk = [...]`;两个 hft-core 实例读同一文件,配置项 `host.egress`):

| 出口 | 地址 | 谁用 | key 白名单 |
|---|---|---|---|
| `prod` | ENI-B 的私有 IP + EIP(可多个) | `hft-core@prod` 的交易所连接(下单、用户流、BBO;ENI-B2 未启用时也包括批量行情与 REST 快照);kill-switch 与 reconciler 的交易所请求经各自配置的 `egress_bind` 绑定 `prod` 列表中的地址 | **是**:每个正式账户的交易、只读、撤单 key 的 IP 白名单只含 `prod` 列表的 EIP |
| `prod_bulk`(只在启用 ENI-B2 时) | ENI-B2 的私有 IP + EIP | `hft-core@prod` 的批量行情(成交、标记价、资金费、深度)与公共 REST 快照连接 | **否**:不进任何 key 的白名单 |
| `sim` | **ENI-C** 的私有 IP + EIP(`dev2`:唯一网卡上的第二个私有 IP + EIP) | `hft-core@sim` 的全部交易所连接(只有公共行情与 REST 快照) | **否**:sim 不持 key;这个 EIP 不得出现在任何 key 的白名单里 |
| 管理 | ENI-A 主私有 IP(公网出口经其公网 IPv4 或 NAT,费用见 [11 §8](../design/11-实施路线图.md)) | SSH、SSM、S3、GitHub、Telegram、pipeline/treasury 的公共接口请求 | 否 |

- hft-core 启动步 2 校验本实例全部 `src_ip` 都在自己的列表内(`prod_bulk` 只允许批量行情与 REST 快照连接使用)、且各列表互不相交,否则退出 2(`EgressIpOverlap`);hostprobe 核对每个地址已配在清单指定的网卡上并关联 EIP,否则退出 3。**sim 连接不落在 ENI-B 队列上**由 hft-core 在每条连接收到首帧后自检(`SO_INCOMING_NAPI_ID` 经 netdev netlink 映射到网卡,内核 ≥ 6.9;规则归 [hft-core 主机资源隔离](../apps/hft-core/README.md#主机资源隔离slice核与出口-ip) 与 [hft-net](../crates/hft-net/README.md)):hostprobe 在 hft-core 建连之前运行,读不到别的进程的 socket。
- `net/hft-egress-ip.sh`(幂等):网卡上没有清单中的某个私有 IP 时 `ip addr add <ip>/<prefix> dev <eni>`,并由 `hft-eni-attach.sh` 为它建源地址路由;经 IMDS 核对该私有 IP 已分配给该网卡且有公网关联(`network/interfaces/macs/<mac>/ipv4-associations/`,路径待核实),否则退出非零,两个 hft-core 单元都不启动(单元的 `ExecStartPre` 核对它为 active)。networkd 重新配置网卡时是否清掉 `ip addr add` 加的地址**待核实**(方法:租用实例上 `networkctl reload` 后读 `ip -4 addr`);会清掉时改由 netplan drop-in 持久化(dev2 上这是改主机网络配置,`[需授权]`)。
- 私有 IP、EIP 与网卡的分配和关联是 `[需授权]` 的 AWS 操作,步骤在 [host-upgrade](../docs/runbooks/host-upgrade.md);脚本只读核对。
- `dev2` 只有一块网卡(清单里当作 ENI-B,没有 ENI-C):`prod` 列表 = 主私有 IP(测试网联调实例;旧系统也用这个地址),`sim` 列表 = 同一网卡上的**第二个私有 IP + EIP**——sim 常驻连着正式行情,若与旧系统共用主地址会占用旧系统在交易所的 IP 级限额,这与"旧系统优先"冲突。两张列表照样不相交,`EgressIpOverlap` 规则不需要例外;dev2 没有延迟验收,网卡队列共享不做处理。分配是 M6 开始前的 `[需授权]` AWS 操作([launch A](../docs/runbooks/launch.md#a-联调dev2测试网模拟盘))。

### slice 与资源切分

hft 的单元分属 `hft.slice` 之下的两个子 slice(`hft-core.slice`、`hft-sim.slice`)与一个**根级** slice `hftbatch.slice`。按 systemd 的命名规则,slice 名里的连字符表示层级(`hft-` 开头的 slice 都落在 `hft.slice` 之下),批处理 slice 因此命名为不带连字符的 `hftbatch.slice`,与 `hft.slice`、`system.slice` 同级。成员与取值的唯一归属是 [hft-core 主机资源隔离](../apps/hft-core/README.md#主机资源隔离slice核与出口-ip)(`MemoryMax`、`CPUQuota`、`CPUWeight`、`AllowedCPUs`、`IOReadBandwidthMax`/`IOWriteBandwidthMax`、各单元的 `Nice` 与 `OOMScoreAdjust`)与其整机内存表;生成器读取,本节不写数字,只定义单元归属与机制:

| slice | 成员 | 安装在 |
|---|---|---|
| `hft.slice` | `hft-core.slice` 与 `hft-sim.slice` 的父 slice;dev2 上带 hft 整体上限(让旧系统),其他主机类只作层级 | 全部主机类 |
| `hft-core.slice` | `hft-core@prod`、`hft-watchtower`、`hft-reconciler`、`hft-ops-bot`;dev2 上另含 `hft-core@sim` | 全部主机类 |
| `hft-sim.slice` | `hft-core@sim`(`AllowedCPUs` 由计划生成,永不含隔离核与 IRQ 专用核) | 只在 prod 与 sim 同机的升级主机类(`c16`/`c32`/`metal`);dev2 上不使用 |
| `hftbatch.slice`(根级) | `hft-verify`、`hft-replay-verify`、`hft-journal-archiver`、`hft-pipeline`、`hft-treasury`;`CPUWeight=idle` | 全部主机类 |
| `system.slice` | `hft-kill-switch`(不进任何 hft slice:护栏或 OOM 处理 hft 时兜底撤单仍在)、`hft-cred-broker`(运行 S3 消费者的主机,见 [IMDS 锁定](#imds-锁定与凭证代理))、`hft-dev2-guard` 与 `hft-offbox-watchdog`(dev2) | 见各单元 |

- **OOM 顺序**:hft-core 的 `OOMScoreAdjust` 在全部 hft 单元中最低(`hft-core@prod` 最后被杀,其次 `hft-core@sim`),批处理最高,其中 `hft-verify` 最先、`hft-replay-verify` 其次;数值归 hft-core。dev2 上全部 hft 单元整体为正值,保证旧系统后于任何 hft 单元被杀(数值同样归 hft-core)。
- **CPU**:批处理的让出由 `hftbatch.slice` 的 `CPUWeight=idle` 实现(写 cgroup `cpu.idle = 1`,需 systemd ≥ 252、内核 ≥ 5.15):这个 slice 作为整体相对同级的 `hft.slice` 与 `system.slice` 按 SCHED_IDLE 语义调度。线程级 `CPUSchedulingPolicy=idle` 只在同一 cgroup 内比较、不跨 cgroup 生效,本目录不依赖它,批处理单元只设 `Nice`(取 hft-core)。`hftbatch.slice` 不设 `CPUQuota`(被 `cpu.max` 节流的时间计入 cpu "some" 压力,会触发护栏)。生效核对:`/sys/fs/cgroup/hftbatch.slice/cpu.idle` = 1(preflight 与 hostprobe)。
- **磁盘 I/O**:冷副本卷的调度器为 `none`,`IOWeight=`/`IOSchedulingClass=` 不生效,本目录不使用;`hftbatch.slice` 与 `hft-sim.slice` 对冷副本卷设 `IOReadBandwidthMax=<cold_root> <rate>` 与 `IOWriteBandwidthMax=<cold_root> <rate>`(`io.max` 不依赖调度器;路径写冷副本根,systemd 取其所在块设备;取值归 hft-core)。冷盘的预置吞吐与 IOPS 写在清单 `[storage]`;M7 验收测 archiver、`hft-replay-verify` 与 sim 写检查点同时运行时 `prod` 的 persist 滞后 p99([host-upgrade](../docs/runbooks/host-upgrade.md))。
- 升级主机:`system.slice`/`user.slice` 的 `AllowedCPUs` 取 [hft-core 主机资源隔离](../apps/hft-core/README.md#主机资源隔离slice核与出口-ip)的表(非隔离核、不含 IRQ 专用核);任何 hft 以外的进程(含 SSM agent)不能落到隔离核与 IRQ 专用核。

### dev2 护栏

`dev2` 是现有服务器(`c7g.large`,2 vCPU、3.8 GB),**同时运行旧系统的正式账户**。新框架在其上只做开发测试、交易所测试网/模拟盘联调、模拟账户长跑与验证包的累计 fuzz/DST(批处理 slice 内、最低优先级),**不放正式实盘账户**(hft-core 另有启动检查 `ProdAccountOnDev2`),不做延迟验收,不编译;**旧系统优先**:hft 的资源占用不得影响它,资源冲突时停 hft。护栏的设计与全部数值(slice 上限、权重、Nice、I/O 带宽上限、tmpfs 大小、守护的信号、阈值、持续时间、两级动作与自动重启 sim 的条件)的唯一归属是 [hft-core dev2 护栏](../apps/hft-core/README.md#dev2-护栏);本节只定义实现它们的部署单元与脚本。决定见 [dev2-guard-system-slice-psi](../.agents/notes/proposed/process/2026-10-03-dev2-guard-system-slice-psi.md)。

dev2 清单示例:

```toml
schema = 4
host = "dev2-1"
instance_type = "c7g.large"
class = "dev2"
instances = ["prod", "sim"]            # sim 常驻;prod 只在测试网联调窗口运行(order_env = "testnet")
formal_live = false                    # 不放正式实盘账户(预检、IAM 与 hft-core 启动检查三重保证);IMDS 只对 hft 各 uid 锁定
verify = true                          # 验证包在 dev2 常态累计
[eni_b]
name = "ens5"                          # 唯一网卡
[egress]
prod = [{ ip = "<主私有 IP>" }]                                 # 测试网联调实例(旧系统也在这个地址)
sim  = [{ ip = "<第二个私有 IP>", eip_alloc = "eipalloc-…" }]  # sim 常驻,不占旧系统的交易所 IP 级限额
```

**1. slice**(`systemd/hft*.slice` + `dev2/*.slice.d/50-dev2.conf`):`hft.slice` 设 hft 常驻部分(`hft-core.slice`,dev2 上 sim 实例也在其中)的内存、`MemorySwapMax=0` 与 CPU 上限;根级 `hftbatch.slice` 另设自己的内存、`MemorySwapMax=0` 与 I/O 带宽上限,`CPUWeight=idle`、**不设 `CPUQuota`**:批处理在旧系统空闲时可以用满空闲 CPU,争用时按 SCHED_IDLE 语义让给同级的 `system.slice`(旧系统)与 `hft.slice`,它自己的等待不计入旧系统的压力(让出的实际程度在第 10 条的标定演练中实测)。两个 slice 的内存上限之和与联调窗口的规则归 hft-core 的 dev2 内存表。

**2. 单元覆盖文件**(`dev2/<unit>.d/50-dev2.conf`):`Nice=`(slice 没有 Nice 属性)、`CPUAffinity=0-1`(否则继承升级主机的核号而启动失败)、`hft-core@.service` 的 `WatchdogSec=10`(hft-core 要求 ≥ 10 s)、各单元的 `OOMScoreAdjust` 与 `MemoryMax`(取 hft-core 的 dev2 列)。`hft-core@.service` 另带:
   - `After=hft-dev2-guard.service` 与 `ExecStartPre=/usr/bin/systemctl is-active --quiet hft-dev2-guard.service`(守护未运行时两个实例都不能启动;`After=` 让开机时实例排在守护之后启动,不因守护尚未就绪而连续失败);不用 `Requisite=`/`Requires=`:守护按 `Restart=always` 重启时不得连带停止或重启 hft-core([deploy/AGENTS](AGENTS.md))。
   - `ExecStopPost=+/opt/hft/current-core/deploy/bin/hft-shm-release %i`:读该实例的 `HeartbeatBlock`(布局归 hft-rt),**只在干净 Drain**(`state = Stopped`,判据归 hft-core)时删除 `/dev/shm/hft/<instance>/` 下的热段文件(心跳块保留),把 tmpfs 页还给系统;状态不是 `Stopped` 时全部保留供恢复;每次决定写一行 `/var/lib/hft/guard/events.jsonl`(以 `runuser -u hft-guard` 追加,与 `clear` 相同)。
   - `hft-core@prod` 在 dev2 上**不设开机自启**,只在联调窗口经 `hft-window start` 启动;`hft-core@sim` 开机自启。
   - 批处理单元(`hft-verify`、`hft-replay-verify`、`hft-journal-archiver`、`hft-pipeline`、`hft-treasury`)的 `ExecCondition=` 在主表的条件(`tripped` 标志与回放并发规则,见 [systemd 单元](#systemd-单元))之后另加:② `/usr/bin/test ! -e /run/hft/window/active`(联调窗口内不启动);③ `/opt/hft/current-core/deploy/bin/hft-unit-idle hft-core@prod.service`(测试网实例在运行时不启动)。① `+/usr/bin/test ! -e /var/lib/hft/guard/tripped` 以 root 判定(`+`):守护的状态目录是 `0750`,批处理用户无权查看,不加 `+` 会把"看不到"当成"不存在"。dev2 上不使用 `Conflicts=`(它是双向的,批处理定时器会反过来停掉测试网实例),也不用 `systemctl mask`(对装在 `/etc/systemd/system` 的单元无效)。回放运行时暂停 archiver 与 pipeline 的规则在主表(全部主机类),dev2 不另覆盖。

**3. 联调窗口** `dev2/hft-window start|stop [--dry-run]`(Bash,`set -euo pipefail`、幂等;由人以 sudo 执行,`[需授权]`):
   - `start`:写窗口标志 `/run/hft/window/active`(内容:UTC 时刻与操作人 = `SUDO_USER`,无则 `root`;标志已在时不改写)→ `systemctl stop` 定时器 `hft-verify.timer`、`hft-replay-verify.timer` 与正在运行的批处理服务(`hft-verify`、`hft-replay-verify`、`hft-journal-archiver`、`hft-pipeline`、`hft-treasury`)→ 启动 `hft-reconciler` 与 `hft-core@prod`;窗口内 `hft.slice` 的峰值才在上限内(预算见 hft-core 的 dev2 内存表)。窗口标志使任何批处理单元在窗口内都不能启动(上面的 `ExecCondition=` ②)。
   - `stop`:`systemctl stop hft-core@prod`(SIGTERM → `Drain`,等 `HeartbeatBlock.state = Stopped`)与 `hft-reconciler` → 核对 `/dev/shm/hft/prod` 挂载的已用量(`statfs`)在 30 s 内回落到心跳块以内(只看文件列表不够:读者对已删除热段的映射同样占住 tmpfs 页,读者在段文件被删除后解除映射);**仍有热段或已用量不回落**(测试网实例在窗口内崩溃,或读者仍持有映射)→ 保留窗口标志、非零退出(否则批处理恢复后 hft 的总内存会超出 hft-core 的 dev2 内存表)并提示:仍有热段文件时"先 `hft-window start` 让测试网实例恢复,再 `hft-window stop`"(规则归 [hft-core dev2 护栏](../apps/hft-core/README.md#dev2-护栏));只是已用量不回落时列出仍映射着已删除热段的进程(读 `/proc/<pid>/maps`)→ 正常时删除窗口标志 → 启动 `hft-journal-archiver`、`hft-pipeline`、`hft-treasury` 与两个定时器。
   - 窗口标志在 `/run`(tmpfs)上:dev2 重启后窗口自然结束,测试网实例不自启,批处理照常恢复(tripped 标志仍在时除外,它在持久目录,见第 5 条)。

**4. journal 热段 tmpfs**:每个实例一个专用挂载 `tmpfs/dev-shm-hft-<instance>.mount`(`/dev/shm/hft/<instance>`,`size=` 取 [hft-core 主机资源隔离](../apps/hft-core/README.md#主机资源隔离slice核与出口-ip) 的值;挂载选项 `uid=<实例用户>,gid=hft-journal-ro,mode=2750`),热段不挤占系统 `/dev/shm`;`hft-core@<instance>.service` 以 `RequiresMountsFor=/dev/shm/hft/<instance>` 依赖它。

**5. 护栏守护** `hft-dev2-guard.service`(`dev2/hft-dev2-guard`,只用 Python 标准库,不依赖 venv;在 `system.slice`,不受 hft 上限影响;`Restart=always`;`RuntimeDirectory=hft/guard`(秘密)、`StateDirectory=hft/guard`(tripped 标志、计数与事件))。配置 `/etc/hft/dev2-guard.toml` 由生成器从 hft-core 的 dev2 档与 `dev2/dev2-guard.toml.tmpl` 生成,[watchtower](../apps/watchtower/README.md) 的护栏阈值文件配置键指向同一文件:`mode = "shared" | "dedicated"`(退路 (b) 的专用主机取后者)、采样周期、二级间隔、自动重启前的回落时长、因果判定阈值、旧系统的单元名与状态文件路径,以及每个信号一项 `{signal, kind, threshold, duration_s}`——`kind = "stop"`(停机信号)或 `"alert"`(只告警信号),`alert` 项另有 `level`(hft-core 护栏表该行的告警级别)。

1. 按 hft-core 给出的采样周期读三类信号:**停机信号**——**旧系统所在 `system.slice` 的 `cpu.pressure` 与 `memory.pressure`**(`/sys/fs/cgroup/system.slice/{cpu,memory}.pressure` 的 `some avg10`)、`MemAvailable`、swap-in 速率(`/proc/vmstat` 的 `pswpin` 增量);**告警信号**——整机 PSI(`/proc/pressure/{cpu,memory}`)与 swap 已用量增长;**旧系统状态**——hft-core 中写明的旧系统单元是否 active、其状态文件的更新年龄(只读);**hft 自身用量**——`hft.slice` 与 `hftbatch.slice` 的 `cpu.stat`(`usage_usec` 增量)与 `memory.current`。整机 PSI 只告警:它包含 hft 自己的等待,不能代表旧系统受到的影响。
2. **只有停机信号能停 hft,且要 hft 自身用量明显**:停机信号持续超过阈值,**且**同一时段 `hft.slice` 或 `hftbatch.slice` 的 CPU 或内存用量超过 hft-core 的阈值 → 执行 hft-core 规定的两级动作,并在第一次动作时写标志 `/var/lib/hft/guard/tripped`(内容:UTC 时刻、信号、级别;文件 `0644`;持久目录,守护重启与整机重启都不清除);停机信号超阈值而 hft 用量不明显(压力来自旧系统自身)→ 只告警(P1),不动作、不写标志、不计入第 9 条退路的触发次数(判定规则归 [hft-core dev2 护栏](../apps/hft-core/README.md#dev2-护栏))。一级:停 `hftbatch.slice` 的成员,**`hft-verify` 最先**,其次 `hft-replay-verify`,再其余。二级(一级之后信号仍在,间隔归 hft-core):先 `systemctl stop hft-core@prod`(测试网实例,若在运行)与 `hft-reconciler`(只在联调窗口运行),再 `systemctl stop hft-core@sim`(各自 SIGTERM → `Drain`;守护以阻塞的 `systemctl stop` 等到单元停止——实例写出 `HeartbeatBlock.state = Stopped` 后退出,或 `TimeoutStopSec` 到期被杀——不读心跳块;干净 Drain 后由 `hft-shm-release` 释放热段)。watchtower、ops-bot 与 kill-switch 不停(停机范围归 hft-core;sim 自动重启后要由 watchtower 出日报)。联调窗口内批处理已停,一级没有可停的单元,信号持续即进入二级。
3. **tripped 标志只能人工清除**:`sudo hft-dev2-guard clear`(`[需授权]`;删除标志并写 events.jsonl;`clear` 由 root 调用,但删除与追加都以 `runuser -u hft-guard` 执行,不在 `/var/lib/hft/guard/` 下留下 root 属主的文件);标志存在期间全部批处理单元被 `ExecCondition=` 拒绝启动(定时器到点也只是跳过)。
4. **只自动重启 sim**:二级动作停掉 `hft-core@sim` 之后,停机信号连续低于阈值达到 hft-core 规定的时长,且此后无人手动启停过该单元,守护 `systemctl start hft-core@sim`,**每 UTC 日至多一次**(计数在 `/var/lib/hft/guard/state.json`),发 P1。"无人启停"的判定:守护停 sim 后把该单元的 `InactiveEnterTimestampMonotonic`(`systemctl show`)记入 `state.json`,重启前再读一次;单元已在运行(`ActiveState` 不是 `inactive`/`failed`)或该值变了 → 不自动启动,写 events.jsonl。守护**从不启动** `hft-core@prod`、批处理单元、定时器或冷进程。同一 UTC 日第二次停掉 sim 后不再自动启动,等人工处理([incident-response P](../docs/runbooks/incident-response.md#p-dev2-护栏触发))。
5. **旧系统状态只告警**:旧系统单元重启、非 active 或状态文件过期 → 告警(级别归 hft-core),**不停 hft**;若同时伴随停机信号,由第 2 条触发。
6. 告警:用守护自己的 Telegram token(秘密前缀 `hft/ops/guard/`)直接发到告警群(不依赖 watchtower),内容含信号值、持续时间、动作;每次触发、清除与自动重启都写 `/var/lib/hft/guard/events.jsonl`。
7. 只允许操作名称在白名单内的 `hft-*` 单元(白名单写死在脚本里,与 polkit 规则 `polkit/50-hft-dev2-guard.rules` 一致):白名单只含两个 hft-core 实例、`hft-reconciler`(只在联调窗口运行)与批处理单元(不含 watchtower、ops-bot、kill-switch),对它们只授权 `stop`,另外**只对 `hft-core@sim.service` 授权 `start`**;**从不读写、启停旧系统的任何单元与文件**。
8. 守护自身异常(读不到 PSI 或 cgroup 压力文件、连续失败)→ 告警;进程退出由 systemd 重启(标志在 `StateDirectory`,守护重启不清除它)。

**6. 主机级调优默认不做**:网卡中断合并、IRQ 亲和等设置会影响同机旧系统;`dev2/hft-tune-dev2.sh apply|revert`(先备份原值到 `/var/lib/hft/tune-backup/<utc>.json`、逐项可回滚)保留为可选,只在负责人确认不影响旧系统后由运维显式执行,不随开机运行。dev2 上不改 GRUB、不隔离核、不预留大页;IMDS 只对 hft 各 uid 锁定(不影响旧系统,见 [IMDS 锁定](#imds-锁定与凭证代理))。

**7. 不放正式实盘账户**:见[秘密与实例角色](#秘密与实例角色)(IAM 只授权测试网与运维前缀;预检拒绝 `order_env = "prod"` 的配置与引用正式秘密的清单)。测试网联调实例的归档 source 设 `prefix = "hft-testnet"`(对象键 `hft-testnet/<host>/prod/…`),测试网数据不进正式前缀(规则归 [journal-archiver](../apps/journal-archiver/README.md));sim 实例照常归档到 `hft/<host>/sim/…`。

**8. 监控**:watchtower 的 `HostPressure` 规则与守护读同一份阈值([watchtower](../apps/watchtower/README.md));watchtower 以附加组 `hft-guard` 读守护的事件文件;守护的停机、清除与自动重启事件进日报。

**9. 退路**:两种退路((a) 批处理迁出、(b) 整体迁出)的触发条件与取舍归 [hft-core dev2 护栏](../apps/hft-core/README.md#dev2-护栏)(触发次数只计第 5 条第 2 项中 hft 自身用量明显的停机);都由负责人决定另开一台 `c7g.large`(`[需授权]`,约 $66/月)。deploy 侧:(a) 新主机只装 `hft-verify`、`hft-replay-verify`(回放读 S3)与只有 `files` source 的 `hft-journal-archiver`(把验证运行记录照常传到 `hft/verify/`),不装护栏与 hft-core,原 dev2 清单改 `verify = false`、`replay_verify = false`(`false` 时生成器不装 `hft-replay-verify.service`/`.timer`,`preflight.sh` 核对两者 `LoadState = not-found`);新主机的清单为 `class = "dev2"`、`instances = []`、`formal_live = false`、`verify = true`、无 `[egress]`,`instances` 为空时生成器只产生这三个单元、`hftbatch.slice`、IMDS 锁定与凭证代理(不产生 hft-core、tmpfs 挂载、护栏与看门狗的单元),期望剖面只含 `hftbatch.slice` 与 IMDS 各项;(b) 新主机清单 `hosts/dev2-2.toml` 仍为 `class = "dev2"`,护栏单元与守护照装,阈值按 hft-core 的"专用主机"列。

**10. M3 标定演练** `[需授权]`(dev2 是旧系统的生产主机):先读 `/sys/fs/cgroup/cgroup.subtree_control`(含 `cpu`)与 `/sys/fs/cgroup/hftbatch.slice/cpu.idle`(= 1)确认 idle 设置生效;再让 `hft-verify` 与 `hft-replay-verify` 在各自上限内满载运行 10 min,同时记录 `system.slice` 的 `cpu.pressure`/`memory.pressure`、`hftbatch.slice` 的 `cpu.stat` 与整机 PSI;通过 = 守护不动作,且 `system.slice` 的压力与演练前 10 min 相比不升(判据数值归 hft-core)。不通过 → 退路 (a) 提前启用。结果写 ops-log 与 incident-response P 节的演练记录。

### hugepages 与 hugetlbfs

- `c4`/`c8`/`c16`/`c32`/`metal`:启动期为 `prod` 实例预留 1 GiB 与 2 MiB 页。页数由生成器**按 hft-core 本档 prod 的实际 journal 列表求和**推导(`seq`、每个 `raw-feed-<venue>`、拆分时每个 `raw-feed-<venue>-bbo`、每个 `raw-gw-<venue>` 的热段 = `(2 + hot_keep) × segment_bytes`,加引擎区与余量;段大小唯一归属 [hft-core](../apps/hft-core/README.md#内存与-journal-段按主机类唯一归属),公式在 [hft-journal](../crates/hft-journal/README.md));`cargo xtask gen-host --check` 把推导结果与 hft-core 内存表的大页行逐档对账,不等即失败。冷副本卷容量见清单 `[storage]`。`sim` 实例与 `dev2` 的两个实例不用大页,热段在各自的 tmpfs 挂载(`dev-shm-hft-<instance>.mount`,大小取 hft-core)。
- 挂载:`/dev/hugepages`(1 GiB 页)承载 prod 的 `seq` 热段与进程内的簿、环、槽;`/dev/hugepages2M`(`hugepages/dev-hugepages2M.mount`)承载 prod 的 `raw-*` 热段与共享内存环(`rt.shm.dir` 默认值,[hft-rt](../crates/hft-rt/README.md))。挂载点 `root:root`、`0751`(只可穿越,不可列目录;journal 读者经它进入下层);`hft/` 与 `hft/prod/` 两级由 `hft-core@prod` 的 `ExecStartPre=+` 逐级创建,属主与模式见[路径表](#身份路径与-s3-前缀)(`hft/prod/` 为 `hft:hft-journal-ro` `2750`,热段文件经 setgid 继承属组,kill-switch 等读者只读映射)。
- hugetlbfs 与 tmpfs 上的热段在进程崩溃后保留、在重启后丢失;这是 journal 恢复规则的前提([08 §1](../design/08-数据面与回放.md))。hugetlb 页不计入 cgroup 的 `memory.max`(说明见 hft-core 内存表)。

### resctrl / CAT

- `hft-resctrl.service`:建组 `hot`(CPU = prod 的热线程核,L3 掩码)与 `rest`(互补掩码,含 sim 实例与冷进程)。可用性是部署前的验证项(假设 A12,[12 §2](../design/12-风险与未决问题.md));不可用时清单 `resctrl.enabled = false` 并记入 HostProfile:生成器不安装 `hft-resctrl.service`,期望剖面中 resctrl 各项为 `advisory`。`hot` 与 `rest` 的 L3 掩码互不相交、并集为全部可用的路。

### systemd 单元

所有单元沿用 Trading 的加固集合(`NoNewPrivileges`、`ProtectSystem=strict`、`ProtectHome`、`PrivateTmp`、`LockPersonality`、`SystemCallArchitectures=native`、`RestrictAddressFamilies=AF_UNIX AF_INET AF_INET6`、空 `CapabilityBoundingSet`,除非下表另注);以 root 运行的单元(用户表 root 一行)写 sysfs/procfs、挂 BPF、配网卡,不套用 `ProtectSystem=strict` 与空 `CapabilityBoundingSet`,其余各项照用,并显式写 `User=root`、`Slice=system.slice`。`ProtectSystem=strict` 下根文件系统只读:单元在自己的 `StateDirectory=`/`RuntimeDirectory=` 之外要写的路径都在下表写出 `ReadWritePaths=`(与[路径表](#身份路径与-s3-前缀)一致;`/dev/shm` 与 hugetlbfs 挂载不受它影响)。资源数值(`MemoryMax`、`Nice`、`CPUWeight`、`CPUQuota`、I/O 带宽上限、`OOMScoreAdjust`)由生成器取自 hft-core,下表不写数字;`dev2` 列只写覆盖文件改什么。各单元的 `ExecStart` 指向本组的发布链接([单元组与分组激活](#单元组与分组激活));取秘密的单元以 `ExecStartPre=+/opt/hft/current-<组>/bin/hft-secrets fetch --manifest /etc/hft/secrets/<unit>.toml --out /run/hft/<运行目录>/secrets --owner <单元用户> --mode 0400` 取到自己的 tmpfs 运行目录(主机级单元——护栏守护、机外看门狗、凭证代理——用 `current-core`)([秘密与实例角色](#秘密与实例角色));S3 消费者经凭证代理取凭证(`Environment=AWS_EC2_METADATA_DISABLED=true` 与指向自己凭证文件的环境变量,[IMDS 锁定](#imds-锁定与凭证代理))。冷进程的 `Restart=on-failure` 都带 `RestartPreventExitStatus=2 3 4`(配置错误不重启,规则归 [apps/AGENTS](../apps/AGENTS.md))。

| 单元 | Type / 看门狗 | 用户 | 关键设置 | `dev2` 覆盖 |
|---|---|---|---|---|
| `hft-host-tune.service`、`hft-eni-attach.service`、`hft-net-tune.service`、`hft-resctrl.service` | oneshot,`RemainAfterExit=yes` | root | 运行期调优(含 `hft-egress-ip.sh`);`Before=hft-core@prod.service hft-core@sim.service` | 不安装;只把 `net/hft-egress-ip.sh` 装成独立 oneshot `hft-egress-ip.service`(只加 sim 的第二个地址,不动网卡其他设置) |
| `hft-imds-lock.service` | oneshot,`RemainAfterExit=yes` | root | `ExecStart=/opt/hft/current-core/deploy/imds/hft-imds-lock.sh load`(dev2 以外:cgroup BPF;dev2:nft,见 [IMDS 锁定](#imds-锁定与凭证代理));`Before=` 全部 hft 冷进程单元与 `hft-core@` | 安装(nft 变体) |
| `hft-cred-broker.service` | simple,`Restart=always` | root | 只在运行 S3 消费者的主机(dev2 与统一升级主机,即清单 `class = "dev2"` 或 `formal_live = true`;探针与 `c4` 既不装凭证代理,也不装 S3 消费者单元 archiver、ops-bot、pipeline、treasury);`Slice=system.slice`;`RuntimeDirectory=hft/creds`、`RuntimeDirectoryPreserve=yes`;`ExecStartPre=…/hft-secrets …`(清单 `cred-broker.toml`:告警 token) | 安装 |
| `hft-hostprobe.service` | oneshot | root | 开机与激活时采集 HostProfile(只读),写 `/run/hft/hostprofile.json`;`Before=hft-core@prod.service hft-core@sim.service` | 安装 |
| `hft-core@.service`(模板;`%i` ∈ `prod`、`sim`) | `notify`,`WatchdogSec=3`,`TimeoutStartSec=45`,`TimeoutStopSec=20` | `%i = prod`:`hft`;`%i = sim`:`hft-sim`(按实例的 drop-in) | `ExecStart=/opt/hft/current-<组>/bin/hft-core --config /etc/hft/hft-core-%i.toml`(prod → `current-core`,sim → `current-sim`,按实例的 drop-in);`RuntimeDirectory=hft/core-%i`、`RuntimeDirectoryMode=0751`;`Restart=on-failure`、`RestartSec=1`、`StartLimitBurst=3`、`StartLimitIntervalSec=600`、`RestartPreventExitStatus=2 3 4`、`RestartForceExitStatus=75`;`LimitMEMLOCK=infinity`、`LimitNOFILE=65536`;`RestrictAddressFamilies=AF_UNIX AF_INET AF_INET6 AF_NETLINK`(conn-builder 经 netdev netlink 自检收包网卡,[hft-net](../crates/hft-net/README.md));`SupplementaryGroups=hft-ctl`(建 `ctl.sock` 并设属组);`ReadWritePaths=<cold_root>/%i`;`ExecStartPre=+` 逐级建立 `<cold_root>/%i/`(属主、模式与 ACL 见路径表);非 dev2:`After=hft-host-tune.service` 加 `ExecStartPre=/usr/bin/systemctl is-active --quiet hft-host-tune.service`(不用 `Requires=`,调优单元重跑不连带重启 hft-core);不 `Requires`/`BindsTo` 另一个实例。**prod drop-in**:`Slice=hft-core.slice`、`ExecStartPre=+…/hft-secrets …`(清单 `hft-core.toml`,输出 `/run/hft/core-prod/secrets`)、`SupplementaryGroups=hft-ctl hft-stopmark` 与 `ReadWritePaths=-/var/lib/hft/kill-switch`(读并改名 kill-switch 的停机标记,规则归 hft-core)、升级主机类上 `ExecStartPre=+` 另建 `/dev/hugepages/hft/prod/`、`/dev/hugepages2M/hft/prod/` 与 `/dev/shm/hft/prod/`(心跳块;升级主机上 prod 没有 tmpfs 挂载)、`/dev/cpu_dma_latency` 经 udev 规则对 `hft` 组可写、`CPUAffinity=` 与 `OOMScoreAdjust` 由线程计划与 hft-core 生成、`AmbientCapabilities=CAP_NET_ADMIN CAP_IPC_LOCK`(最小集待核实,方法:租用实例上逐项去掉后跑 `--check` 与一次启动)。**sim drop-in**:`Slice=hft-sim.slice`、**没有** `hft-secrets`、`CPUAffinity=` 只含 sim 的非隔离核、`RequiresMountsFor=/dev/shm/hft/sim` | 两个实例:`CPUAffinity=0-1`、`WatchdogSec` ≥ 10、`Nice`、`After=hft-dev2-guard.service` 加 `ExecStartPre=/usr/bin/systemctl is-active --quiet hft-dev2-guard.service`、`RequiresMountsFor=/dev/shm/hft/%i`、`ExecStopPost=+…/hft-shm-release %i`;sim 的 `Slice=hft-core.slice`;核对的调优单元是 `hft-egress-ip.service`(`After=` 加同样的 `is-active` 检查);prod 不开机自启;sim 开机自启 |
| `hft-kill-switch.service` | `notify`,`WatchdogSec=5` | `hft-ks` | `Restart=always`、`RestartPreventExitStatus=2 3 4`(UID 不符等配置错误退出 2 时不重启,缺位由 watchtower `ProcessDown` 报 P0);在 `system.slice`;**不** `Requires`/`BindsTo` hft-core;`RuntimeDirectory=hft/kill-switch`、`RuntimeDirectoryMode=0751`(本地触发套接字见下);`ExecStartPre=+…/hft-secrets …`(清单 `kill-switch.toml`:各 Live 账户的撤单 key 与告警 token);状态目录 `/var/lib/hft/kill-switch`(由本单元的 `ExecStartPre=+` 建立,`ReadWritePaths=/var/lib/hft/kill-switch`;`actions.jsonl` 与停机标记,标记的写法归 [kill-switch](../apps/kill-switch/README.md));`SupplementaryGroups=hft-journal-ro hft-ks-trigger hft-stopmark`;撤单请求经配置的 `egress_bind` 绑定 prod 出口地址;经 polkit 只能 `stop` `hft-core@prod.service`(见下) | 只改 `CPUAffinity` |
| `hft-journal-archiver.service` | simple | `hft-archive` | `Slice=hftbatch.slice`、`Restart=on-failure`;`SupplementaryGroups=hft-journal-ro hft-verify`(读冷副本、advisor 检查点与验证运行记录);`ReadWritePaths=<cold_root>`(在段旁写 `.uploaded`、经 `cold_retire` 删除已提交的段;写权限来自路径表的 ACL);`StateDirectory=hft/archiver`、`StateDirectoryMode=0751`,`ExecStartPre=+` 建立 `advisor-daily/`(见路径表);`ExecCondition=+/usr/bin/test ! -e /var/lib/hft/guard/tripped` 与 `/opt/hft/current-core/deploy/bin/hft-unit-idle hft-replay-verify.service`(回放运行时不启动);S3 凭证经凭证代理(写 `hft/` 与 `hft-testnet/` 两个前缀) | `Nice`;另加 dev2 的 `ExecCondition=`(见 dev2 护栏) |
| `hft-replay-verify.service` + `.timer` | oneshot;每日一次(UTC 01:30,前一日的段已封存后) | `hft-replay` | 用当前激活版本的 `replayer verify` 以每日模式校验本机冷副本的 `seq` journal,每个实例运行一次 `replayer verify --journal <cold_root>/<instance>/seq --instance <instance> --advice recompute --model-checkpoint /var/lib/hft/archiver/advisor-daily/<instance> --report-dir /var/lib/hft/replay-verify/<instance>`(命令行与选日规则归 [replayer verify](../apps/replayer/README.md#verify) 第 9 条):每次运行补跑**所有缺少报告的已封存 UTC 日**(补跑与迟到报告的计时规则归 [testing §6](../docs/testing.md#6-ci-运行矩阵),命令参数归 [replayer](../apps/replayer/README.md)),结果写 `/var/lib/hft/replay-verify/<instance>/<date>.json`(`StateDirectoryMode=0750`,watchtower 以附加组 `hft-replay` 读取);`Slice=hftbatch.slice`;只读冷副本与 `archiver/advisor-daily/`(`hft-journal-ro` 组)。**回放期间暂停其他批处理**(全部主机类;并发规则归 [hft-core 内存表](../apps/hft-core/README.md#内存与-journal-段按主机类唯一归属)):`ExecStartPre=+-/usr/bin/systemctl stop <本机已安装的 hft-verify.service、hft-journal-archiver.service、hft-pipeline.service>`(名单由生成器按清单 `verify` 生成;`-` 前缀使未加载的单元不导致启动失败;被中止的验证包写完最后一段记录再退出),archiver 与 pipeline 的 `ExecCondition=` 在回放运行时拒绝启动;恢复由 `OnSuccess=hft-replay-resume.service` 与 `OnFailure=hft-replay-resume.service` 完成(在本单元完全停止后触发,`hft-unit-idle` 不会再把处于 `deactivating` 的回放当作运行中);`ExecCondition=+/usr/bin/test ! -e /var/lib/hft/guard/tripped` | 护栏一级动作在 `hft-verify` 之后停它;另加 dev2 的 `ExecCondition=`;正在运行时两个 hft-core 的启动与 Drain 不等待它 |
| `hft-replay-resume.service` | oneshot | root | `ExecStart=/usr/bin/systemctl start --no-block hft-journal-archiver.service hft-pipeline.service`(只由 `hft-replay-verify` 的 `OnSuccess=`/`OnFailure=` 触发;两者的 `ExecCondition=` 照常判定,护栏或窗口标志存在时不启动) | 同左 |
| `hft-verify.service` + `.timer` | oneshot;定时器 `OnBootSec=15min`、`OnUnitInactiveSec=10min` | `hft-verify` | 只在清单 `verify = true` 的主机安装;`StateDirectory=hft/verify`;其余设置见[验证包与 hft-verify](#验证包与-hft-verify) | 护栏一级动作**最先**停它;另加 dev2 的 `ExecCondition=` |
| `hft-sim-alternate.service` + `.timer` | oneshot;定时器 `OnCalendar=*-*-* *:00:00`、`Persistent=true` | root | 只在 `c16`/`c32`/`metal` 安装,默认不启用;`ExecStart=/opt/hft/current-core/deploy/bin/hft-sim-alternate --config /etc/hft/measure.toml`(种子与起点在主机本地文件,不随发布);门槛 ⑩ 测量窗口内由 [host-upgrade](../docs/runbooks/host-upgrade.md) 启用,主机重启后照常继续 | 不安装 |
| `hft-watchtower.service` | simple | `hft-watch` | `Slice=hft-core.slice`;`Restart=on-failure`;`CPUAffinity=` 冷核;`SupplementaryGroups=hft-journal-ro hft-verify hft-replay hft-recon hft-guard hft-ack hft-archive hft-pipeline`(读 journal、验证运行记录、回放报告、reconciler 的 `fatal.json` 与护栏事件;`acks.jsonl` 的属组;archiver 的 `alerts.jsonl`(规则 `ArchiverAlert`)与 pipeline 各实例的 `status.json`(规则 `PipelineAlert`));`RuntimeDirectory=hft/watchtower`;`StateDirectory=hft/watchtower`、`StateDirectoryMode=0751`,`ExecStartPre=+` 建立 `acks.jsonl`(`hft-watch:hft-ack`,`0660`,ops_bot 追加写);`ExecStartPre=+…/hft-secrets …`(清单 `watchtower.toml`:告警 token);`/healthz` 监听 ENI-A 私有 IP(与 ENA 计数的网卡名一样由 watchtower 启动时从 `/run/hft/hostprofile.json` 读取,不由本目录生成,[watchtower](../apps/watchtower/README.md)),`/metrics` 只绑 127.0.0.1;读维护静默标志 `/run/hft/maint/until`([机外看门狗](#机外看门狗)) | `Nice` |
| `hft-reconciler.service` | simple | `hft-recon` | `Slice=hft-core.slice`;`Restart=on-failure`;`CPUAffinity=` 冷核;`SupplementaryGroups=hft-ctl hft-journal-ro`;`ExecStartPre=+…/hft-secrets …`(清单 `reconciler.toml`:各 Live 账户的只读 key);`RuntimeDirectory=hft/reconciler`;`StateDirectory=hft/reconciler`、`StateDirectoryMode=0750`;交易所请求经 `egress_bind` 绑定 prod 出口地址 | `Nice`;只在联调窗口运行(由 `hft-window` 启停) |
| `hft-ops-bot.service` | simple | `hft-opsbot` | `Slice=hft-core.slice`;`Restart=on-failure`;`CPUAffinity=` 冷核;`SupplementaryGroups=hft-ctl hft-ks-trigger hft-pipeline-api hft-ack hft-journal-ro hft-treasury`(`hft-treasury`:读 treasury 输出);`ReadWritePaths=/var/lib/hft/watchtower/acks.jsonl`;`StateDirectory=hft/ops_bot`、`StateDirectoryMode=0750`;`RuntimeDirectory=hft/ops_bot`;`ExecStartPre=+…/hft-secrets …`(清单 `ops-bot.toml`:本主机 bot 的 token 与 Bark key);Python 解释器为 `/opt/hft/current-cold/venv/bin/python`;S3 凭证经凭证代理 | `Nice` |
| `hft-pipeline.service` / `hft-treasury.service` | simple | `hft-pipeline` / `hft-treasury` | `Slice=hftbatch.slice`、`Restart=on-failure`;`SupplementaryGroups=hft-journal-ro`(pipeline 另加 `hft-ctl hft-pipeline-api`);pipeline:`RuntimeDirectory=hft/pipeline`、`RuntimeDirectoryMode=0751`,`StateDirectory=hft/pipeline`、`StateDirectoryMode=0750`,本地接口套接字 `/run/hft/pipeline/api.sock`(`hft-pipeline:hft-pipeline-api`,`0660`),`ExecCondition=+/usr/bin/test ! -e /var/lib/hft/guard/tripped` 与 `…/hft-unit-idle hft-replay-verify.service`;treasury:`StateDirectory=hft/treasury`、`StateDirectoryMode=0750`(组 `hft-treasury`,ops_bot 以附加组读);Python 解释器同上;S3 凭证经凭证代理 | `Nice`;另加 dev2 的 `ExecCondition=` |
| `hft-dev2-guard.service` | simple,`Restart=always` | `hft-guard` | 只在 `dev2` 安装;在 `system.slice`;`RuntimeDirectory=hft/guard`、`RuntimeDirectoryPreserve=yes`;`StateDirectory=hft/guard`、`StateDirectoryMode=0750`;`ExecStartPre=+…/hft-secrets …`(清单 `dev2-guard.toml`);polkit 只授权停白名单内的 `hft-*` 单元与启动 `hft-core@sim` | — |
| `hft-offbox-watchdog.service` | simple,`Restart=always` | `hft-offbox` | 只在 `dev2` 安装;在 `system.slice`;`RuntimeDirectory=hft/offbox`;`StateDirectory=hft/offbox`、`StateDirectoryMode=0750`(最近一次读到的 `maintenance_until`);`ExecStartPre=+…/hft-secrets …`(清单 `offbox-watchdog.toml`);见[机外看门狗](#机外看门狗) | — |
| `dev-shm-hft-prod.mount`、`dev-shm-hft-sim.mount` | — | — | 每实例的热段 tmpfs(大小取 hft-core;`uid=<实例用户>,gid=hft-journal-ro,mode=2750`);升级主机只装 `sim` 的,dev2 两个都装 | 两个都装 |
| `dev-hugepages.mount` 覆盖、`dev-hugepages2M.mount` | — | — | `c4`/`c8`/`c16`/`c32`/`metal`:hugetlbfs,`mode=0751`(挂载点只可穿越) | 不安装 |
| `hft.slice`、`hft-core.slice`、`hftbatch.slice` | — | — | 全部主机类安装;见[slice 与资源切分](#slice-与资源切分) | 带上限的覆盖文件 |
| `hft-sim.slice` | — | — | `c16`/`c32`/`metal` 安装 | 不安装 |
| `system.slice`、`user.slice` 覆盖 | — | — | `c8`/`c16`/`c32`/`metal`:`AllowedCPUs=` 取 [hft-core 主机资源隔离](../apps/hft-core/README.md#主机资源隔离slice核与出口-ip)的表;`c4`:hk 核(见[主机清单](#主机清单)) | **不安装**(旧系统照常在两个核上调度) |
| `hft-core.target` / `hft-cold.target` / `hft-sim.target` | — | — | 单元组([分组激活](#单元组与分组激活));`hft-core.target` 含 `hft-core@prod`,`hft-sim.target` 只含 `hft-core@sim` | 全部安装 |
| 可选:`hft-chain-core@.service`、`hft-chain-grpc-relay@.service`、`hft-chain.target`、`bsc-node.service` | — | — | 触发条件:链上策略迁到新框架(规格见 [chain-core](../apps/chain-core/README.md)、[chain-grpc-relay](../apps/chain-grpc-relay/README.md)、[bsc-node](../docs/runbooks/bsc-node.md));默认不安装 | — |

- **权限闭包**(部署不变量):每个单元用户对它要读写的每条路径——沿途每一级目录可穿越、终点可读或可写——由[路径表](#身份路径与-s3-前缀) × [用户表](#身份路径与-s3-前缀) × 单元的 `SupplementaryGroups=`/`ReadWritePaths=` 推出。创建属组为 G 的套接字或文件的进程必须在 G 里;唯一例外是属组由 setgid 目录继承(路径表中模式以 `2` 开头的目录),写者不必在该组。单元的 `SupplementaryGroups=` 与用户表逐项相同。`preflight.sh` 以每个服务用户身份(`runuser -u <user>`)按路径表核对一遍。
- 读 journal 的冷进程用户加入 `hft-journal-ro` 组;两个实例的热段与冷副本目录对该组只读(冷副本另以 ACL 对 `hft-archive` 开放写,见路径表)。
- **ctl socket**:每个实例一个,路径 `/run/hft/core-<instance>/ctl.sock`(本实例 `RuntimeDirectory` 下;tools 与 hft-ctl 的默认路径以此为准),属主 `<实例用户>:hft-ctl`,`0660`(实例用户在 `hft-ctl` 组,建 socket 后设属组);允许发命令的用户(`hft-opsbot`、`hft-pipeline`、`hft-recon` 与运维登录用户,见用户表)加入 `hft-ctl`;gateway 再按 uid 查权限表([hft-ctl](../crates/hft-ctl/README.md))。treasury 与 watchtower 不发命令,不在该组。
- **kill-switch 本地触发套接字**:`/run/hft/kill-switch/trigger.sock`(在 kill-switch 自己的 `RuntimeDirectory` 下),属主 `hft-ks:hft-ks-trigger`,`0660`;`hft-ks-trigger` 组只有 `hft-ks` 与 `hft-opsbot`;kill-switch 再按 `SO_PEERCRED` 的 uid 只接受 `trigger.ops_bot_user`(= `hft-opsbot`)。协议与动作(撤单,或 `stop_core` = 写停机标记、撤单后停 `hft-core@prod`)归 [kill-switch](../apps/kill-switch/README.md),调用方是 [ops_bot 紧急停止](../python/ops_bot/README.md#紧急停止stop);路径、属组与用户名以本节与[身份、路径与 S3 前缀](#身份路径与-s3-前缀)为准。
- **停机标记目录**:`/var/lib/hft/kill-switch/` 属主 `hft-ks:hft-stopmark`、`2770`(setgid,无粘滞位);`hft-stopmark` 组只有 `hft-ks` 与 `hft`:kill-switch 在 `systemctl stop` 之前写 `stop-marker.json`(`0660`),`hft-core@prod` 启动时只读这个文件名,读到后改名为 `.consumed`;`stop_core` 停机失败时 kill-switch 把它改名为 `stop-marker.json.aborted` 并 P0(规则归 [kill-switch](../apps/kill-switch/README.md) 与 [hft-core](../apps/hft-core/README.md#启动顺序))。`hft-sim` 不在该组。
- **kill-switch 的 polkit 授权**(`polkit/50-hft-kill-switch.rules`,装在每台有 `prod` 实例的主机):只允许 `subject.user == "hft-ks"` 对 `org.freedesktop.systemd1.manage-units` 且 `unit == "hft-core@prod.service"`、`verb == "stop"`(SIGTERM → `Drain`,不平仓);其他单元、其他动词(`start`、`restart`、`kill`)一律拒绝。`hft-core@sim` 不在授权内。
- **护栏守护的 polkit 授权**(`polkit/50-hft-dev2-guard.rules`,只在 dev2):`subject.user == "hft-guard"` 对白名单内单元(两个 hft-core 实例、`hft-reconciler` 与批处理单元)只允许 `stop`;`start` 只允许 `unit == "hft-core@sim.service"`;其他动词与单元一律拒绝。
- **每个单元自建自己的目录**:运行目录用本单元的 `RuntimeDirectory=hft/<unit>`(模板按实例分开,只由该单元声明),状态目录用 `StateDirectory=`,热段与共享内存子目录由本单元的 `ExecStartPre=+` 逐级创建;不依赖其他单元先启动建目录。跨单元读写只经附加组、ACL 与 `ReadWritePaths=`,全部登记在[身份、路径与 S3 前缀](#身份路径与-s3-前缀)。

### 身份、路径与 S3 前缀

本节是系统用户与组、`/opt/hft`、`/etc/hft`、`/run/hft`、`/var/lib/hft`、`/dev/shm/hft`、hugetlbfs 与冷盘下路径与 S3 前缀的**唯一登记处**;其他文档引用的名字必须在这里出现(`doc-check` 的身份与路径注册表检查读这三张表)。

**用户与组**(AMI provision 与 dev2 的安装脚本创建;每个主机类都创建下表全部用户与组——只在部分主机类安装的单元,其用户也创建,watchtower 的附加组 `hft-guard` 等因此处处存在;可选的 `hft-chain` 只在链上组件启用时创建;组成员只按下表,单元的 `SupplementaryGroups=` 与它逐项相同):

| 用户 | 单元 | 主组之外的附加组 |
|---|---|---|
| `hft` | `hft-core@prod` | `hft-ctl`(建 ctl socket)、`hft-stopmark`(读并改名停机标记) |
| `hft-sim` | `hft-core@sim` | `hft-ctl` |
| `hft-ks` | `hft-kill-switch` | `hft-journal-ro`(心跳与提交游标)、`hft-ks-trigger`(建触发套接字)、`hft-stopmark`(写停机标记) |
| `hft-archive` | `hft-journal-archiver` | `hft-journal-ro`、`hft-verify` |
| `hft-replay` | `hft-replay-verify` | `hft-journal-ro` |
| `hft-verify` | `hft-verify` | — |
| `hft-watch` | `hft-watchtower` | `hft-journal-ro`、`hft-verify`、`hft-replay`、`hft-recon`、`hft-guard`、`hft-ack`、`hft-archive`、`hft-pipeline` |
| `hft-recon` | `hft-reconciler` | `hft-ctl`、`hft-journal-ro` |
| `hft-opsbot` | `hft-ops-bot` | `hft-ctl`、`hft-ks-trigger`、`hft-pipeline-api`、`hft-ack`、`hft-journal-ro`、`hft-treasury` |
| `hft-pipeline` | `hft-pipeline` | `hft-ctl`、`hft-pipeline-api`(建本地接口套接字)、`hft-journal-ro` |
| `hft-treasury` | `hft-treasury` | `hft-journal-ro` |
| `hft-guard` | `hft-dev2-guard`(dev2) | — |
| `hft-offbox` | `hft-offbox-watchdog`(dev2;可选的第三 AZ 实例) | — |
| `hft-chain` | 可选:`hft-chain-core@<chain>`(链上组件启用时) | `hft-ctl`、`hft-journal-ro` |
| 运维登录用户 | 运维 CLI(`hft-cfg send` 等,issuer `cli:<login>`) | `hft-ctl`(AMI 写入运维公钥的账户;dev2 上是负责人的登录账户;增减成员是 `[需授权]`) |
| root | 调优单元、`hft-imds-lock`、`hft-cred-broker`、`hft-hostprobe`、`hft-egress-ip`、`hft-replay-resume`、`hft-sim-alternate`、各单元的 `ExecStartPre=+` | — |

组:`hft`(`hft-core@prod` 的主组;`/dev/cpu_dma_latency`)、`hft-journal-ro`(热段与冷副本只读)、`hft-ctl`(ctl socket)、`hft-ks-trigger`(kill-switch 触发套接字)、`hft-stopmark`(停机标记目录)、`hft-verify`(验证运行记录只读)、`hft-pipeline-api`(pipeline 本地接口)、`hft-ack`(watchtower 的 `acks.jsonl` 追加写),以及作只读用途的主组 `hft-replay`、`hft-recon`、`hft-guard`、`hft-treasury`、`hft-archive`、`hft-pipeline`。

**路径**(没有 `/opt/hft/current`、`/opt/hft/previous` 与 `/opt/hft/venv`):

| 路径 | 属主:组 / 模式 | 建立者 | 用途 |
|---|---|---|---|
| `/opt/hft/releases/<v>/`(含 `venv/`) | root / 0755 | `releasectl install` | 不可变的版本目录;`venv/` 是该版本离线建成的单一 Python 环境 |
| `/opt/hft/current-<group>`、`/opt/hft/previous-<group>`(`group` ∈ `core`、`cold`、`sim`,可选 `chain`) | 符号链接 | `releasectl activate` | 本组当前与回滚版本([单元组与分组激活](#单元组与分组激活)) |
| `/opt/hft/{installations,deployments,staging}/` | root / 0755 | releasectl | 安装与部署记录、下载暂存 |
| `/opt/hft/verify/<ver>/`、`/opt/hft/verify/current` | root / 0755 | `releasectl install-verify` | 验证包 |
| `/usr/local/sbin/{releasectl,hft-maint}`;dev2 另有 `/usr/local/sbin/{hft-window,hft-dev2-guard}` | root / 0755 | `core` 组激活 | 指向 `/opt/hft/current-core/deploy/…` 的符号链接 |
| `/usr/local/bin/{hostprobe,hft-cfg}`、`/usr/local/bin/{kill-switch,journal-archiver,replayer}`、`/usr/local/bin/{hft-pipeline,hft-treasury}`、`/usr/local/bin/hft-verify` | root / 0755 | `core` 组激活、`cold` 组激活(后两组)、`releasectl install-verify` | 运维命令的符号链接,依次指向 `current-core/bin/`、`current-cold/bin/`、`current-cold/venv/bin/` 与 `/opt/hft/verify/current/bin/`;runbook 中的这些命令名即指它们 |
| `/etc/hft/hft-core-<instance>.toml`、`/etc/hft/<app>.toml`、`/etc/hft/egress.toml`、`/etc/hft/secrets/<unit>.toml` | root / 0644 | `core` 组激活(生成物与模板) | 配置与秘密清单(只含引用) |
| `/etc/hft/host-local.toml`、`/etc/hft/measure.toml`(升级主机)、`/etc/hft/offbox-watchdog.toml`(dev2)、`/etc/hft/github-release-token`(0600) | root / 0644(token 除外) | 人(主机本地,不随发布) | 选用的清单名;门槛 ⑩ 同机干扰测量的种子与起点;看门狗目标;服务器拉取 token |
| `/run/hft/` | root / 0755 | systemd(各单元 `RuntimeDirectory=` 的父目录) | 运行期目录的父目录 |
| `/run/hft/core-<instance>/` | `<实例用户>` / 0751 | `hft-core@` 的 `RuntimeDirectory` | `ctl.sock`(`<实例用户>:hft-ctl` 0660)、`secrets/`(0700) |
| `/run/hft/kill-switch/` | `hft-ks` / 0751 | kill-switch 的 `RuntimeDirectory` | `trigger.sock`(`hft-ks:hft-ks-trigger` 0660)、`secrets/` |
| `/run/hft/{watchtower,reconciler,ops_bot,guard,offbox}/` | 各单元用户 | 各单元的 `RuntimeDirectory` | `secrets/` |
| `/run/hft/pipeline/` | `hft-pipeline` / 0751 | pipeline 的 `RuntimeDirectory` | `api.sock`(`hft-pipeline:hft-pipeline-api` 0660) |
| `/run/hft/chain-core-<chain>/`(可选) | `hft-chain` / 0751 | chain-core 的 `RuntimeDirectory` | `ctl.sock`(`hft-chain:hft-ctl` 0660),链上组件启用时 |
| `/run/hft/creds/<user>/credentials` | 该用户 / 0400(目录 0500) | `hft-cred-broker` | 消费者的短期 S3 凭证 |
| `/run/hft/window/active`、`/run/hft/maint/until` | root / 0644 | `hft-window`、`hft-maint` | 联调窗口标志;维护静默截止时刻 |
| `/run/hft/hostprofile.json` | root / 0644 | `hft-hostprobe` | 实际剖面;hft-core 与 watchtower(`/healthz` 监听地址、ENA 计数的网卡名)启动时读 |
| `/etc/hft/ctl-principals.toml` | root / 0644 | 生成器(`core` 组激活) | ctl 签发者表(hft-core 配置 `paths.ctl_principals`;两个实例启动时只读) |
| `/etc/hft/ctl-operators.toml` | root / 0644 | 人(主机本地,不随发布;修改 `[需授权]`) | 生成签发者表的人员输入:运维登录名与 Telegram 用户 id → 角色 |
| `/dev/shm/hft/` | root / 0755 | tmpfs 挂载单元(挂载点的父目录)或 `hft-core@prod` 的 `ExecStartPre=+` | 各实例热段目录的父目录 |
| `/dev/shm/hft/<instance>/` | `<实例用户>:hft-journal-ro` / 2750 | tmpfs 挂载单元(挂载选项);升级主机上的 `prod` 没有挂载,由 `hft-core@prod` 的 `ExecStartPre=+` 建目录 | 热段与心跳块(升级主机的 `prod` 只有心跳块) |
| `/dev/hugepages`、`/dev/hugepages2M` | `root:root` / 0751 | hugetlbfs 挂载单元 | 挂载点(只可穿越) |
| `/dev/hugepages/hft/`、`/dev/hugepages2M/hft/` | `hft:hft-journal-ro` / 0751 | `hft-core@prod` 的 `ExecStartPre=+` | 中间层(hft-core 可在其下建共享内存环目录) |
| `/dev/hugepages/hft/prod/`、`/dev/hugepages2M/hft/prod/` | `hft:hft-journal-ro` / 2750 | `hft-core@prod` 的 `ExecStartPre=+` | prod 的 `seq` 与 `raw-*` 热段 |
| `/dev/cpu_dma_latency` | `root:hft` / 0660 | udev 规则(只在升级主机类) | `hft-core@prod` 写 0 并持有(规则归 hft-core) |
| `<cold_root>` | root / 0755 | 冷副本卷挂载(AMI 或 dev2 现有 `/data`) | 冷盘根(hft-core `paths.journal_cold`) |
| `<cold_root>/<instance>/` | `<实例用户>:hft-journal-ro` / 2750,另有访问 ACL 与默认 ACL `u:hft-archive:rwx` | `hft-core@` 的 `ExecStartPre=+` | 冷副本、`meta/`(DurableMeta)与 advisor 检查点;子目录与文件继承默认 ACL:`hft-archive` 可在段旁写 `.uploaded` 并删除已提交的段,组 `hft-journal-ro` 仍只读(写者建子目录用 `0770`,使 ACL mask 不收窄具名用户条目);`ExecStartPre=+` 先按 2750 建目录再设两条 ACL,此后 `stat` 的组位显示 ACL mask `rwx`,组 `hft-journal-ro` 的权限由 ACL 条目 `group::r-x` 决定 |
| `/var/lib/hft/` | root / 0755 | systemd(`StateDirectory=`) | 各单元状态目录的父目录 |
| `/var/lib/hft/{ops_bot,pipeline}/` | 各单元用户 / 0750 | `StateDirectory=` | 各自状态;pipeline 的 `<实例>/` 子目录 0750、`status.json` 0640(组 `hft-pipeline`,watchtower 以附加组读) |
| `/var/lib/hft/treasury/` | `hft-treasury:hft-treasury` / 0750 | `StateDirectory=` | treasury 输出与 policy;ops_bot 以附加组 `hft-treasury` 读 |
| `/var/lib/hft/archiver/` | `hft-archive` / 0751 | `StateDirectory=` | 游标、提交证明、quarantine;`alerts.jsonl`(`hft-archive:hft-archive` 0640,watchtower 以附加组读) |
| `/var/lib/hft/archiver/advisor-daily/` | `hft-archive:hft-journal-ro` / 2750 | archiver 的 `ExecStartPre=+` | 每日 Model 检查点的本地副本;`hft-replay-verify` 经 `hft-journal-ro` 读 |
| `/var/lib/hft/watchtower/` | `hft-watch` / 0751 | `StateDirectory=` | 游标、日报;`acks.jsonl`(`hft-watch:hft-ack` 0660) |
| `/var/lib/hft/{reconciler,replay-verify,guard}/` | 各单元用户 / 0750(组为该用户主组) | `StateDirectory=` | 只经附加组被 watchtower 读取;`guard/` 另有 `tripped` 标志(`0644`,重启不消失,批处理单元以 `ExecCondition=+` 判定) |
| `/var/lib/hft/verify/` | `hft-verify:hft-verify` / 0750 | `hft-verify` 的 `StateDirectory=` | 验证运行记录与账本;watchtower 与 archiver 以附加组 `hft-verify` 只读 |
| `/var/lib/hft/offbox/` | `hft-offbox` / 0750 | `StateDirectory=` | 机外看门狗最近一次读到的 `maintenance_until`(看门狗重启后仍生效) |
| `/var/lib/hft/measure/active` | root / 0644(目录 0755) | 人(host-upgrade 第 7 步) | 门槛 ⑩ 测量窗口标志(持久,重启不消失;`hft-verify.service` 的 `ExecCondition=` 查它) |
| `/var/lib/hft/kill-switch/` | `hft-ks:hft-stopmark` / 2770 | kill-switch 的 `ExecStartPre=+` | `actions.jsonl`、`stop-marker.json`(消费后 `.consumed`,作废后 `.aborted`) |
| `/var/lib/hft/tune-backup/` | root | `hft-tune-dev2.sh`(dev2,可选) | 调优前原值 |

**S3 前缀**(桶 `gwx-trading-data`,区域 `ap-northeast-1`;对象布局归 [journal-archiver](../apps/journal-archiver/README.md)):

| 前缀 | 内容 | 写者 | 读者 |
|---|---|---|---|
| `hft/<host>/<instance>/…` | 正式与模拟实例的 journal Parquet、manifest、advisor 每日检查点 | `hft-archive` | ops_bot、pipeline、treasury;交易主机之外的 research 与 replayer(运维只读角色) |
| `hft-testnet/<host>/prod/…` | 测试网账户实例(dev2 联调窗口、M7 升级主机)的同样内容 | `hft-archive` | ops_bot、pipeline(treasury 不读测试网数据);交易主机之外的 research 与 replayer(运维只读角色) |
| `<hft 或 hft-testnet>/<host>/<instance>/private/params/` | pipeline 参数版本库与 `groups.json`;`…/sim/private/params/proposals/` 是研究提案 | `hft-pipeline`(提案由研究者以个人凭证写) | pipeline |
| `hft/verify/…` | 验证运行记录、账本快照、崩溃输入与 resolution(对象键含主机名) | `hft-archive` | 负责人(同步到本机后 `hft-verify ledger show --runs-from`;同步以 `hft-archive` 身份运行,见[运维命令的身份](#imds-锁定与凭证代理)) |
| `hft-bench/` | 参考机 `bench-gate --reference` 结果 | 负责人(租用实例) | 评审 |
| collector 前缀(取 pipeline 配置 `warmup.collector_prefix`;名称待核实,方法:读旧系统 collector 的上传配置并在 dev2 上 `aws s3 ls` 核对) | 旧系统 collector 上传的行情数据 | 旧系统(不归本目录) | `hft-pipeline`(只读;预热与交易宇宙的数据源) |

- `<host>` 是清单的 `host`(待命清单与主清单相同,M7 与 M8 两份清单也相同),所以 dev2 与升级主机的同名实例写不同的键;同一主机换 AZ 重建后沿用原前缀。
- **SSE-KMS**:`…/<instance>/private/*` 用 SSE-KMS(key = journal-archiver 的 `s3.private_kms_key_id`),其余前缀用桶默认加密。写者需要 `kms:GenerateDataKey` 与 `kms:Decrypt`,读者需要 `kms:Decrypt`;授权随[消费者角色](#秘密与实例角色)与运维只读角色发放,覆盖每个 SSE-KMS 前缀。研究提案的个人凭证同样要有该前缀的写与 `kms:GenerateDataKey`。
- **运维只读角色** `hft-ops-read`:负责人与研究者的个人 IAM 身份可扮演;`s3:GetObject`/`s3:ListBucket` 于 `hft/*`、`hft-testnet/*`、`hft-bench/*`,`kms:Decrypt` 于上述 key;交易主机之外运行的 replayer 与 [research](../python/research/README.md) 用它。创建是 `[需授权]` 的 IAM 变更。

### 秘密与实例角色

- 每个正式账户在每个 venue 的交易 key(下单会话 key 与 REST key;Binance 为两把,OKX/Bitget 为同一把,以 hft-core 为准)、只读 key、撤单 key 都存 Secrets Manager,名字前缀 `hft/live/<account>/<venue>/…`;测试网/模拟盘账户的 key 前缀 `hft/testnet/<account>/<venue>/…`;**运维 token 统一前缀 `hft/ops/<unit>/…`**(`<unit>` ∈ `kill-switch`、`watchtower`、`ops-bot`、`guard`、`offbox`、`cred-broker`;按主机区分的 token 在其下再加 `<host>/`,例如每台主机的 ops_bot 各有自己的 Telegram bot,`hft/ops/ops-bot/<host>/…`,两个 bot 同时 `run_polling` 不会冲突)。交易 key 不带提现权限、绑定 `prod` 出口 EIP 的白名单([launch](../docs/runbooks/launch.md));模拟账户没有 key。每把 key 的 UID 由使用它的进程在启动时核对(规则归 hft-core、kill-switch、reconciler)。
- **按单元投递**:每个取秘密的单元一份清单 `/etc/hft/secrets/<unit>.toml`(只含引用,模板在 `secrets/`)与一个运行目录,由该单元的 `ExecStartPre=+…/hft-secrets fetch` 取到 `/run/hft/<运行目录>/secrets/`(用法与威胁模型归 [tools/hft-secrets](../tools/README.md#hft-secrets)):

| 单元 | 清单 | 内容 |
|---|---|---|
| `hft-core@prod` | `hft-core.toml` | 各 Live 账户的交易 key |
| `hft-kill-switch` | `kill-switch.toml` | 各 Live 账户的撤单 key;`hft/ops/kill-switch/` 的告警 token |
| `hft-reconciler` | `reconciler.toml` | 各 Live 账户的只读 key |
| `hft-watchtower` | `watchtower.toml` | `hft/ops/watchtower/` 的 Telegram/Bark/电话通道 token |
| `hft-ops-bot` | `ops-bot.toml` | `hft/ops/ops-bot/<host>/` 的 bot token 与 Bark key |
| `hft-dev2-guard` | `dev2-guard.toml` | `hft/ops/guard/` 的 Telegram token |
| `hft-offbox-watchdog` | `offbox-watchdog.toml` | `hft/ops/offbox/` 的 Telegram token 与电话通道 |
| `hft-cred-broker` | `cred-broker.toml` | `hft/ops/cred-broker/` 的 Telegram token(刷新失败告警) |

- **IAM**(`secrets/iam-*.json.tmpl`;实例角色都**没有** EC2 写权限,网卡与地址的挂载由人在控制台或启动模板完成):
  - `dev2`(`formal_live = false`):实例角色与旧系统共用(旧系统只用它的 S3 写)。为 hft 增加的权限 = `secretsmanager:GetSecretValue` 于 `hft/testnet/*` 与 `hft/ops/*`(root 的 `hft-secrets`)、`sts:AssumeRole` 到 hft 各消费者角色(凭证代理)。hft 的 S3 读写——含 archiver 上传前的 `HEAD` 与 `private/*` 的 SSE-KMS——全部经消费者角色,不用实例角色。这是改生产主机 IAM 的 `[需授权]` 步骤,与"各消费者角色信任 dev2 实例角色"一起作为同一次变更,列在 [launch A](../docs/runbooks/launch.md#a-联调dev2测试网模拟盘)。
  - 升级主机(`formal_live = true`):实例角色授权 `hft/live/*`、`hft/testnet/*`(M7 延迟验收与测试网演练用测试网)与 `hft/ops/*`、`sts:AssumeRole` 到各消费者角色,以及托管策略 `AmazonSSMManagedInstanceCore`(SSM 是破窗通道,[AMI 构建](#ami-构建));实例角色没有 S3 权限,触及 S3 的运维命令按[运维命令的身份](#imds-锁定与凭证代理)运行。
  - 探针主机(`probe-*` 清单):租用实例的角色只授权 `hft/testnet/*` 与 `hft/ops/*`(探针不运行 S3 消费者)。
  - 消费者角色(`iam-consumers.json.tmpl`,每个消费者一个,信任 dev2 与升级主机的实例角色):S3 只覆盖[S3 前缀表](#身份路径与-s3-前缀)里登记给它的前缀(读者 `s3:GetObject`/`s3:ListBucket`,写者另加 `s3:PutObject`),KMS 按 SSE-KMS 一条(写者 `kms:GenerateDataKey` + `kms:Decrypt`,读者 `kms:Decrypt`)。新建或修改这些角色是 `[需授权]` 的 IAM 变更。
- **预检**(`preflight.sh`,`formal_live = false` 时):`/etc/hft/hft-core-prod.toml` 的 `instance.order_env` 必须是 `testnet`。默认 `instance.secrets_mode=aws_secrets_manager`：清单只引用 `hft/testnet/` 与 `hft/ops/` 前缀，本机角色对 `hft/live/_sentinel` 的 `DescribeSecret` 必须得到 `AccessDenied`。显式 `local_demo` 只用于非正式 demo 账户：`/etc/hft/secrets/hft-core.toml` 顶层 `mode=local_demo`，每个 `[[secret]]` 只有 `name`、`path`；源文件位于 `/run/secrets/<私有目录>/`，沿途目录 root:0700，普通文件 root:0400/0600，拒绝符号链接、路径穿越与超长文件。预检核对这些属性代替 IAM/Secrets Manager，不读取内容；IMDS 隔离、其他主机检查和 core 配置检查仍必需。`prepare-secrets.py` 先检查 core 的 demo 端点与 `is_prod=false`，再复制到 hft:0400 的 `/run/hft/core-prod/secrets/<name>`，目标集合须与配置全部 credential 路径相等；core 读取后 unlink 副本，root 源文件保留。正式 Live/production 无此模式。

### IMDS 锁定与凭证代理

全部主机类都锁定,按主机类分两种实现([imds-lockdown](../.agents/notes/proposed/architecture/2026-10-03-imds-lockdown.md)、[dev2-root-batch-slice](../.agents/notes/proposed/process/2026-10-03-dev2-root-batch-slice.md))。目的:实例角色能读秘密(升级主机上是全部正式交易 key),而 IMDS 默认对本机所有 uid 可达;面向 Telegram 的 Python 进程或其依赖被利用时不能借 IMDS 取走角色凭证。

1. **锁定**(`hft-imds-lock.service` 开机执行 `imds/hft-imds-lock.sh load`;`hft-imds-lock.sh status` 打印规则是否就位与拦截计数):
   - **dev2 以外的主机类**(`c4`/`c8`/`c16`/`c32`/`metal`,含探针清单的主机):以 `bpftool` 把 `imds/hft-imds-connect.bpf.o`(源码 `hft-imds-connect.bpf.c`,由发布工作流编译,[development](../docs/development.md#发布工作流release-aarch64yml))的两个程序 `hft_imds4` 与 `hft_imds6` 分别挂到根 cgroup 的 `connect4` 与 `connect6`(pin 在 `/sys/fs/bpf/hft-imds/connect4`、`/sys/fs/bpf/hft-imds/connect6`):非 root uid 对 `169.254.169.254` 与 `fd00:ec2::254` 的 `connect()` 返回 `EPERM` 并在 map `hft_imds_cnt` 中计数。理由:钩子只在建连时运行,订单包不经过任何过滤规则(热路径无逐包开销);本机不加载 nftables,hostprobe 核对 `nf_conntrack` 未加载([HostProfile](#hostprofile))。
   - **dev2**:加载 `imds/hft-imds-dev2.nft`:表 `inet hft_imds`,链 `out`(`type filter hook output priority 0; policy accept`),集合 `hft_uids` = 用户表中的服务用户(不含 root 与运维登录用户),两条规则 `meta skuid @hft_uids ip daddr 169.254.169.254 counter drop` 与 `meta skuid @hft_uids ip6 daddr fd00:ec2::254 counter drop`。理由:与旧系统同机,只按 hft 的 uid 丢弃,不在根 cgroup 挂程序、旧系统进程的 IMDS 访问不变;规则简单,`nft list table inet hft_imds` 即可审计;dev2 没有延迟验收,输出钩子的逐包开销不计。

   root 进程(`hft-secrets`、`hostprobe`、`hft-eni-attach`/`hft-egress-ip`、SSM agent、凭证代理)照常使用实例角色。副作用:以非 root 用户查询 IMDS 的工具失效——升级主机的 AMI 因此不安装 `ec2-instance-connect`,SSH 破窗用 SSM 会话或 AMI 内置的运维公钥([AMI 构建](#ami-构建));dev2 上只挡 hft 的 uid,旧系统照旧。
2. **凭证代理** `hft-cred-broker.service`(root,Python 标准库 + 固定版本的 AWS CLI v2;装在运行 S3 消费者的主机:dev2 与统一升级主机):以实例角色对每个消费者 `aws sts assume-role` 到其专属角色并附 session policy,把短期凭证写到 `/run/hft/creds/<user>/credentials`(属主 = 该用户,`0400`;目录 `0500`),每 45 min 刷新(凭证有效期 60 min);刷新失败 → 保留旧文件、P1 告警(本单元自己的 token,`hft/ops/cred-broker/`,与 watchtower 同群;同一消费者连续失败只发一次),此后每 1 min 重试;该消费者的凭证剩余有效期 < 5 min 时仍未刷新成功 → P0(一次)。
3. **消费者**(`creds/consumers.toml.tmpl`;前缀以[S3 前缀表](#身份路径与-s3-前缀)为准):`hft-archive`(S3 写 `hft/` 与 `hft-testnet/` 两个前缀,含 `hft/verify/`)、`hft-opsbot`(只读 `hft/*`、`hft-testnet/*` 的 Parquet)、`hft-pipeline`(只读其输入前缀与 collector 前缀 + 写本机的 `<hft 或 hft-testnet>/<host>/*/private/params/`)、`hft-treasury`(只读 `hft/*`);KMS 按 [SSE-KMS](#身份路径与-s3-前缀) 一条。消费者单元设 `Environment=AWS_EC2_METADATA_DISABLED=true` 与指向自己凭证文件的环境变量。**进程不重启也要用上新凭证**:boto3 与 Rust SDK 经共享凭证文件或 `credential_process` 在到期前重读(待核实:各 SDK 的重读时机,方法:租用实例上把刷新周期缩到 5 min 跑 2 h);DuckDB 的 S3 secret 与 polars(object_store)不随凭证文件刷新,Python 消费者每次发起 S3 读取前比较凭证文件的 mtime,变化时重建 DuckDB `CREATE SECRET` 与 polars 的 `storage_options`(规则写在各包的依赖节)。
4. **预检**(`preflight.sh`):以一个非 root 服务用户(`runuser -u hft-pipeline`)访问 IMDS 必须失败;`hft-imds-lock` active 且 `hft-imds-lock.sh status` 显示的程序或规则与发布内一致;dev2 以外另查 `ec2-instance-connect` 未安装;装有凭证代理的主机上,每个消费者的凭证文件存在、属主与权限正确、未过期,并以 `hft-archive` 的凭证向本机 `<prefix>/<host>/<instance>/private/_preflight` 写一个小对象(SSE-KMS,同键覆盖)再 `HEAD`,以每个读者消费者的凭证 `HEAD` 一次其前缀——核对 S3 与 KMS 授权。
5. **运维命令的身份**:主机上触及 S3 或 ctl 的运维命令以对应服务用户的身份运行,不用 root 的实例角色:`sudo -u <用户> env AWS_EC2_METADATA_DISABLED=true AWS_SHARED_CREDENTIALS_FILE=/run/hft/creds/<用户>/credentials <命令>`。`journal-archiver check` 与供 `hft-verify ledger show --runs-from <dir>` 使用的 `aws s3 sync s3://gwx-trading-data/hft/verify/ <dir>` 用 `hft-archive`(它能读 archiver 状态与本机验证运行记录,凭证覆盖 `hft/verify/`);`hft-pipeline …` 用 `hft-pipeline`(它同时是 ctl 主体 `pipeline`)。

### 模拟实例(sim)

模拟在一个**只含模拟账户、不持任何交易所 key** 的独立 hft-core 实例上运行,任何主机类都如此;prod 实例不含 Sim 账户(`SimAccountInProdInstance`)。实例的语义、线程计划与内存档归 [hft-core](../apps/hft-core/README.md#实例角色prod-与-sim),决定见 [sim-colocated-with-prod](../.agents/notes/proposed/architecture/2026-10-03-sim-colocated-with-prod.md)。本目录提供它的部署形态:

| 项 | `hft-core@prod` | `hft-core@sim` |
|---|---|---|
| 二进制 | `/opt/hft/current-core/bin/hft-core` | 同一发布的同一二进制,经 `/opt/hft/current-sim/bin/hft-core`(可先于 prod 激活新版本) |
| 配置 | `/etc/hft/hft-core-prod.toml`;dev2 上 `order_env = "testnet"`、refdata 只含测试网账户;升级主机上 M7 为 `testnet`、M8 起 `prod` | `/etc/hft/hft-core-sim.toml`,`instance.name = "sim"`;refdata 只含 `kind = Sim` 的账户 |
| 运行时段 | dev2:只在测试网联调窗口;升级主机:常驻 | 常驻 |
| 系统用户 | `hft` | `hft-sim`(不在 `hft` 与 `hft-stopmark` 组,读不到 prod 的运行目录、秘密 tmpfs 与停机标记) |
| 秘密 | `ExecStartPre=+…/hft-secrets`;配置有 `secrets_dir` | 无;配置没有 `secrets_dir`,单元里出现 `hft-secrets` 或 `/run/hft/core-prod/` 路径即 `preflight` 失败 |
| journal | `<cold_root>/prod/`;热段在大页(dev2 在 `dev-shm-hft-prod.mount`) | `<cold_root>/sim/`;热段在 `dev-shm-hft-sim.mount` |
| 核 | 线程计划的 hk/warm/hot 核 | 只用线程计划给 sim 的**非隔离核**,含 ENI-C 的 IRQ 核(`host.irq_cpus.eni_c`;`c32` 上它单独成核、不放线程,`c16` 上与 sim 的非热线程共用);从不占 prod 的热核与 ENI-B/ENI-B2 的 IRQ 专用核 |
| slice | `hft-core.slice` | `hft-sim.slice`(dev2:`hft-core.slice`) |
| 网卡与出口 IP | ENI-B(启用时另有 ENI-B2);`egress.toml` 的 `prod` 列表 | ENI-C;`egress.toml` 的 `sim` 列表(dev2:唯一网卡上的第二个私有 IP + EIP) |
| 行情连接 | 自己的连接 | 自己的连接(与 prod 不共享 feed),计入 sim 出口地址的交易所限额 |
| 读者 | watchtower、journal-archiver、ops_bot、pipeline、treasury、reconciler、kill-switch | watchtower、journal-archiver、ops_bot、pipeline、treasury(各自配置里列两个实例);kill-switch 判活但目标表为空;reconciler 不面向它 |
| ctl socket | `/run/hft/core-prod/ctl.sock`,`hft-ctl` 组 | `/run/hft/core-sim/ctl.sock`,同一 `hft-ctl` 组;权限表相同 |

- 两个实例互不依赖:任一个的停止、重启、激活不影响另一个;激活组分别为 `core` 与 `sim`([分组激活](#单元组与分组激活))。
- `preflight.sh` 另查:`hft-core-sim.toml` 的 `instance.name = "sim"` 且无 `secrets_dir`;sim 的 drop-in 无秘密路径;`hft-sim` 用户不在 `hft`、`hft-stopmark` 组;`hft-sim.slice` 的 `AllowedCPUs` 与隔离核集合、IRQ 专用核不相交;`egress.toml` 的两张列表不相交。
- 切换步骤(dev2 的 sim 实例停用、升级主机上建 prod 与 sim)在 [host-upgrade](../docs/runbooks/host-upgrade.md)。

### 验证包与 hft-verify

上线门槛 ③ fuzz 与 ④ DST 的累计在服务器上运行 CI 构建的验证包([verify-on-server](../.agents/notes/proposed/process/2026-10-03-verify-on-server.md));`hft-verify` 的命令与运行记录格式归 [tools](../tools/README.md#hft-verify),账本规则(按子进程 CPU 时间计量、按 `path_digest` 跨版本累计、失败分类)归 [testing §6.1](../docs/testing.md#61-服务器上的累计运行),内存上限归 [hft-core 内存表](../apps/hft-core/README.md#内存与-journal-段按主机类唯一归属)。

- **包** `hft-verify-<ver>-aarch64.tar.zst`(+ `.sha256`,`deploy/release/package-verify.sh` 打包,可复现):`fuzz/<target>`(固定 nightly + AddressSanitizer 的 libFuzzer 二进制,每个 core 解码器目标一个)、`bin/hft-dst`(`ci` 剖面的 DST 运行器,来自 [hft-testkit](../crates/hft-testkit/README.md))、`bin/hft-verify`(驱动)、`corpus/<target>/`(初始语料)、`VERIFY.json`(版本、Git SHA、nightly 日期、目标清单、每个目标组的 `path_digest`、门槛取值 `gates`、逐文件 SHA-256;字段归 [tools](../tools/README.md#hft-verify),`path_digest` 的求法归 [development 验证包工作流](../docs/development.md#验证包工作流verify-bundleyml))。
- **安装** `releasectl install-verify <ver>`:`fetch-release.sh --version <ver> --asset verify` 拉取 → 校验 `.sha256` 与 `VERIFY.json` 逐文件哈希 → 解包到 `/opt/hft/verify/<ver>/` → 切换 `/opt/hft/verify/current`;不改 `current-<group>`、不停任何单元、不影响生产版本激活;保留当前与上一个验证包版本。
- **单元** `hft-verify.service`(只在清单 `verify = true` 的主机安装):`Type=oneshot`;`User=hft-verify`;`ExecStart=/opt/hft/verify/current/bin/hft-verify run --budget-min <N>`(N 写在本主机的单元覆盖文件,默认 120);`StateDirectory=hft/verify`(`/var/lib/hft/verify/{ledger,runs,corpus,crashes}`,`StateDirectoryMode=0750`,组 `hft-verify`,archiver 与 watchtower 以附加组只读);`Slice=hftbatch.slice`、`Nice`/`OOMScoreAdjust`/`MemoryMax` 取 [hft-core](../apps/hft-core/README.md#主机资源隔离slice核与出口-ip)(全部 hft 单元中最先被杀)、`TimeoutStartSec` = 预算 + 10 min、`TimeoutStopSec=30`、`KillMode=mixed`(停单元时 SIGTERM 只发给 `hft-verify` 本身,由它中止子进程并写完最后一段记录,超时后 systemd 再 SIGKILL 全部);不联网(`PrivateNetwork=yes`、`RestrictAddressFamilies=AF_UNIX`);子进程的 ASan 与 libFuzzer 限额由 `hft-verify` 设置(取值规则归 [tools hft-verify](../tools/README.md#hft-verify));`ExecCondition=` 依次为 `+/usr/bin/test ! -e /var/lib/hft/guard/tripped`、`+/usr/bin/test ! -e /var/lib/hft/measure/active`(门槛 ⑩ 测量窗口,升级主机)与 `/opt/hft/current-core/deploy/bin/hft-unit-idle hft-replay-verify.service`(dev2 另加窗口标志与 `hft-core@prod.service`,见 dev2 护栏)。定时器 `hft-verify.timer`:`OnBootSec=15min`、`OnUnitInactiveSec=10min`(一次运行结束 10 min 后再起,条件不满足时自然跳过)。
- **S3 备份**:journal-archiver 的 `kind = files` source 上传 `/var/lib/hft/verify/{ledger,runs,crashes,resolutions}` 到前缀 `hft/verify/`(隐藏文件 `crashes/dst/.inflight.json` 不上传:它是每个种子都会改写的进行中标记,不是不可变文件;对象键与排除规则归 [journal-archiver 文件归档](../apps/journal-archiver/README.md#文件归档files-source));崩溃输入最小化后由人带回仓库做回归金样。
- **在哪台主机跑**:`dev2` 常态运行(护栏内、最低优先级;护栏一级动作最先停它);升级主机只在 M7 期间、门槛 ⑩ 的 7 天测量窗口之外运行(窗口由持久标志 `/var/lib/hft/measure/active` 挡住,主机重启后照样生效,步骤在 [host-upgrade](../docs/runbooks/host-upgrade.md));M8 切到 `tokyo-cex-1-live` 清单(`verify = false`)并重新激活后,升级主机上不再安装这两个单元(`preflight.sh` 核对 `LoadState = not-found`),后续版本的累计在 dev2 上继续。

### 机外看门狗

`hft-offbox-watchdog.service` 装在 `dev2`(与升级主机不在同一 AZ 时),在升级主机整机或 AZ 故障、watchtower 与 ops_bot 一起消失时仍能告警([offbox-watchdog](../.agents/notes/proposed/architecture/2026-10-03-offbox-watchdog.md));处置见 [incident-response S](../docs/runbooks/incident-response.md#s-交易主机失联)。

- `offbox/hft-offbox-watchdog`:只用 Python 标准库,不依赖 venv;用户 `hft-offbox`;`Slice=system.slice`(不受 hft 上限与 dev2 护栏影响,护栏白名单里没有它);`Restart=always`;`MemoryMax` 取 hft-core。
- 每 15 s `GET http://<升级主机 ENI-A 私有 IP>:<healthz_port>/healthz`(watchtower 的 `healthz_bind`,返回 hft-core 心跳年龄与状态、kill-switch 存活、最近一次对账时刻与 `maintenance_until`;内容归 [watchtower](../apps/watchtower/README.md));连续 60 s 不可达或报告不健康 → 告警,恢复后发一条恢复通知。
- **配置是主机本地文件** `/etc/hft/offbox-watchdog.toml`(目标 URL、周期 15 s、判定 60 s、通道;样例 `offbox/offbox-watchdog.toml.example`),不随发布、不进任何哈希;升级主机重建后 ENI-A 的私有 IP 改变,由人改这个文件并重启看门狗(`[需授权]`,incident-response S 节)。
- 告警通道独立于升级主机上的一切:自己的 Telegram bot token + 电话类告警(供应商由负责人在上线门槛 ⑪ 之前选定);token 由 `ExecStartPre=+…/hft-secrets` 从 `hft/ops/offbox/…` 取到本单元的 tmpfs。
- **维护静默**:升级主机上的计划操作(激活 `cold` 组会停 watchtower 与 kill-switch、GRUB 重启)前,运维在该主机执行 `sudo hft-maint start <分钟>`(正整数,> 30 按 30 写入,其他值退出 2;`bin/hft-maint` 写 `/run/hft/maint/until`,内容为一行十进制 Unix 秒(截止时刻;watchtower 按此读);`hft-maint stop` 删除,`status` 显示);watchtower 在 `/healthz` 中返回 `maintenance_until`,看门狗缓存最近一次读到的值(写进 `/var/lib/hft/offbox/`,看门狗自己重启后仍生效),在截止前不可达或不健康时**只发 Telegram、不打电话**,截止后恢复正常告警(静默期内开始、到截止仍未恢复的故障,在截止后的第一次轮询补打电话);超过 30 min 的值按 30 min 截断。watchtower 停止期间看门狗读不到延长,所以停 watchtower 的操作要在一次静默内完成(需要更久时按 runbook 分段,例如 [launch D-23](../docs/runbooks/launch.md#d-正式实盘m8))。执行顺序:`hft-maint start` → 等看门狗至少读到一次(≤ 15 s)→ 开始操作([release-and-deploy](../docs/runbooks/release-and-deploy.md))。
- 网络:升级主机的安全组只对 dev2(与可选的第三 AZ 看门狗)的私有 IP 放行 `healthz_port`(`[需授权]`);两台主机之间的私网可达性(同一 VPC 或对等连接)**待核实**(方法见开发备忘)。
- **第三 AZ 看门狗(条件启用)**:若 [az-probe](../docs/runbooks/az-probe.md) 证实 Binance 所在 AZ(即升级主机所在 AZ)与 dev2 相同,另开一台 `t4g.nano`(费用见 [11 §8](../design/11-实施路线图.md),`[需授权]`)在第三个 AZ 只运行同一个 `hft-offbox-watchdog`(从正式版归档的 `deploy/offbox/` 取两个文件手工安装,不安装发布归档;同一主机本地配置格式与 `hft/ops/offbox/` 秘密)。dev2 上的看门狗照常保留。
- 演练:停掉升级主机上的 watchtower 或屏蔽安全组 → ≤ 75 s 内(连续 60 s 不可达的判定加一个 15 s 轮询周期)Telegram 与电话告警到达;恢复后收到恢复通知;`hft-maint start 10` 期间同样操作只收到 Telegram(上线门槛 ⑪)。

### AMI 构建

- Packer 模板 `ami/tokyo-cex.pkr.hcl`(统一升级主机用):Ubuntu LTS(aws 内核 ≥ 6.12;具体版本在定档后定并记入清单)→ ENA 驱动(`ENA_PHC_INCLUDE=1`)→ 禁用系统杂项、移除 `ec2-instance-connect`(保留 SSM agent,见[运行期调优](#运行期调优))→ 安装 chrony、`linux-tools`(`rtla`、`perf`、`bpftool`)、`ethtool`、`acl`(`setfacl`)、`python3.12`、`python3.12-venv`、`zstd`、AWS CLI v2(官方 zip 安装器,固定版本并核对签名)→ 写入运维 SSH 公钥(破窗用;私钥由负责人保管)→ 按[身份、路径与 S3 前缀](#身份路径与-s3-前缀)创建用户与组(该运维账户加入 `hft-ctl`)→ 安装本目录的单元与脚本(不含应用二进制、不含任何编译工具链)。
- 构建后自动验证:启动一台实例运行 `hostprobe verify`,全部 `required` 项通过才发布 AMI ID;验收:隔离核 `osnoise` 24 h max < 20 µs([02 §6](../design/02-硬件与托管方案.md)),见 [host-tuning](../docs/runbooks/host-tuning.md)。最近一个通过验证的 AMI ID 记在 ops-log,供[交易主机失联](../docs/runbooks/incident-response.md#s-交易主机失联)时重建。
- 冷副本卷是独立的 gp3 EBS 卷(大小、预置吞吐与 IOPS 见清单 `[storage]`),不随 AMI;重建主机时可从故障实例摘下挂到新实例(同一 AZ)。
- `dev2` 不重建 AMI:只安装发布归档,不改主机镜像;用户与组由 dev2 的安装脚本按同一张表创建;冷副本 ACL 与 IMDS 规则依赖主机上已有的 `acl` 与 `nftables` 包(缺失时安装是 `[需授权]` 的主机改动)。

### 发布归档与 releasectl

沿用 Trading 的发布实现与纪律(Trading AGENTS.md(本仓库之外:`Trading/AGENTS.md`)、Trading `deploy/scripts/`),差异如下:

| 项 | Trading(现有) | hft |
|---|---|---|
| 归档名 | `trading-<v>-aarch64.tar.zst` | `hft-<v>-aarch64.tar.zst`(+ `.sha256`);唯一发布目标见 [development.md 目标 CPU](../docs/development.md#目标-cpu);x86 只是可选 `metal` 的独立剖面 |
| 安装根 | `/opt/trading/{releases,current,previous,installations,deployments,staging}` | `/opt/hft/{releases,installations,deployments,staging}` 加**按组的** `current-<group>`/`previous-<group>`;**没有** `current`/`previous`([单元组与分组激活](#单元组与分组激活));验证包在 `/opt/hft/verify/` |
| 归档布局 | `bin/`、`systemd/`、`polkit/`、`deploy/`、`schemas/`、`config/`、`python/`、`RELEASE.json` | `bin/`(hft-core、journal-archiver、watchtower、reconciler、kill-switch、replayer、hostprobe、hft-secrets、hft-cfg;可选 chain-core)、`systemd/`、`polkit/`(dev2 守护、kill-switch)、`deploy/`(本目录的脚本与生成物)、`schemas/`(`gen-schema` 输出;`schemas/strategies/` 下是 `hft-core schemas --out` 导出的各策略参数 schema:`<kind_id>-<name>.schema.json` + `index.json`,规格归 [strategies/registry](../strategies/registry/README.md))、`config-templates/`、`python/`(各包 wheel——含随策略包发布的 pipeline 插件 wheel([pipeline 插件](../python/pipeline/README.md#插件的打包与发现))、`hft_py` aarch64 wheel(规格见 [hft-py](../crates/hft-py/README.md))、wheelhouse、单一 `requirements.lock`)、`sbom.json`、`RELEASE.json` |
| `RELEASE.json` | `version, git_sha, target, rust_version, schemas{…}, files[…]` | `version, channel: "release" \| "dev", git_sha, ci_run_id, target, target_cpu, rust_version, profiles: {<bin>: "release" \| "release-hot"}, strategies: [{name, kind_id, version, schema_id, schema_ver}], schemas{event, journal, control, refdata, parquet}, host_expect: {<清单名>: sha256}, evidence: {ci_run_id, decoupling_check_run_id, nightly_run_id, overrides: [{input, actor}]}, sbom_sha256, files[{path,sha256,bytes}]`,`sort_keys`、不列自身;`host_expect` 覆盖 `hosts/` 下的每份清单(主清单、待命清单与探针清单),哈希对象是该清单生成的期望剖面,不含 MAC;`evidence` 是发布 `preflight` 所依据的运行与 Admin 覆盖(GitHub 按保留期删除运行记录后仍可追溯,字段含义归 [development](../docs/development.md#发布工作流release-aarch64yml)) |
| 可复现打包 | `tar --sort=name --mtime=@${SOURCE_DATE_EPOCH:-0} --owner=0 --group=0 --numeric-owner` + `zstd -19 -T4` | 同(验证包同样) |
| 获取 | 服务器拉取:`fetch-release.sh --version X`,token 文件 0600 | 同;公开仓库 `Fiora-gwx/hft` 的只读 token `/etc/hft/github-release-token`(root,0600):个人账户 `Fiora-gwx` 名下的 fine-grained token,只授权仓库 `Fiora-gwx/hft` 的 Contents 读,有效期 ≤ 1 年,由负责人创建(`[需授权]`),到期前 30 天在 ops-log 日历提醒并轮换(步骤在 [release-and-deploy](../docs/runbooks/release-and-deploy.md));开发构建 `--prerelease vX.Y.Z-dev.N`;验证包 `--asset verify` |
| 安装 | 校验 sha256 → 安全解包 → 文件集合严格相等 → 逐文件哈希 → 架构检查与 `--version` → 离线建单一 venv → 权限 → `installations/<v>.json` | 同;venv 建在该版本目录 `releases/<v>/venv`(安装不碰正在运行的版本);另校验 `host_expect[<本机清单名>]` 与本机按 `/etc/hft/host-local.toml` 选定的清单生成的期望剖面一致(`host-local.toml` 缺失、清单名不在 `host_expect` 中或哈希不符 → 退出 1);安全解包 = 拒绝绝对路径、含 `..` 的路径、链接与特殊文件、多于一个根目录;同一版本目录已存在 → 退出 1;离线建 venv 失败 → 版本目录改名为 `<v>.failed.<unix 秒>` 并退出 1;`channel = "dev"` 而本机清单不是 `dev2` → 退出 1;验证包走 `releasectl install-verify`([验证包](#验证包与-hft-verify)) |
| 激活 | 重新逐文件哈希 → 切 `current`/`previous` → `systemd-analyze verify` → 装单元 → `daemon-reload` → **disable --now 本组应用单元** → 记录 `pending` | 同;切的是本组的 `current-<group>`/`previous-<group>`;另装调优文件与 udev 规则(仅非 dev2 主机类)、`egress.toml`、polkit 规则、dev2 护栏文件(仅 dev2)、IMDS 锁定(按主机类)与凭证代理配置,按清单 `verify` 安装或删除 `hft-verify.*`,建 `/usr/local/{sbin,bin}` 下本组的命令链接 → `hostprobe` 写 `hostprofile.json` 并比较;激活的退出码 = 这一步 `hostprobe verify` 的退出码(此时链接与单元已切换、应用单元未启动),部署记录写入同一结果 |
| 预检 `preflight.sh` | 8 项 | preflight、fetch、hft-window、hft-dev2-guard 与 hft-shm-release 的 Python 入口在导入归档帮助模块前禁用字节码写入,保持严格文件集校验不变; 架构与 CPU 特性(`uname -m` 与 `RELEASE.json.target` 的架构相同,`/proc/cpuinfo` 含 `target_cpu` 要求的特性);冷副本卷独立(`<cold_root>` 与 `/` 的 `findmnt` 来源不同)且可用空间占卷容量的比例 ≥ hft-core 的 `storage.disk_free_block_pct`;chrony 同步(`chrony.service` active、`Leap status` 为 `Normal`、`Last offset` 绝对值 ≤ 5 ms);主机上无编译工具链(`command -v` 找不到:`dev2` 只查 `cargo`、`rustc`——旧系统可能装着 C 工具链;其余主机类查 `cargo`、`rustc`、`rustup`、`gcc`、`cc`、`clang`);S3 preflight([IMDS 锁定](#imds-锁定与凭证代理)第 4 条);每份 `/etc/hft/secrets/<unit>.toml` 的 `hft-secrets --check --manifest <清单>` 通过(检查模式归 [tools](../tools/README.md#hft-secrets));watchtower 配置的 `export.listen` 主机部分为 `127.0.0.1`;`/run/hft/hostprofile.json` 存在、`hft-watch` 可读,且 `dev2` 以外的主机类中 ENI-A 私有 IP 不是 `unreadable`;`/etc/hft/ctl-principals.toml` 存在、与生成器对当前 `ctl-operators.toml` 的输出逐字节相同;不存在 `/opt/hft/current` 与 `/opt/hft/venv`;`c4`/`c8`/`c16`/`c32`/`metal` 另查 hugetlbfs、隔离核、ENI-B(与同机时的 ENI-C、启用时的 ENI-B2)按设备序号就绪、`egress.toml` 地址就绪;`dev2` 另查各 slice 已装、守护 active、polkit 规则只覆盖 `hft-*`(且 `start` 只对 `hft-core@sim`)、批处理单元带 `ExecCondition=`;全部主机类查 `hftbatch.slice` 在根级且 `cpu.idle = 1`、[权限闭包](#systemd-单元)(以每个服务用户身份按路径表核对)与 [IMDS 锁定](#imds-锁定与凭证代理)各项;`formal_live = false` 时查[秘密与实例角色](#秘密与实例角色)各项;清单 `verify = false` 时 `hft-verify.service`/`.timer` 的 `LoadState = not-found`;[模拟实例](#模拟实例sim)各项 |
| 回滚 | `rollback.sh --version` = 以旧版本激活 | 同,按组 |
| `releasectl` 子命令 | `install`、`activate`、`rollback`、`record` | 另有 `verify --archive <f> --sha256-file <f>`(在临时根目录走完整安装路径与 `hft-core --check`,不影响运行中的系统)、`install-verify <ver>`、`uninstall`(停用并删除全部 `hft-*` 单元、slice、polkit 与 udev 规则、IMDS 锁定与 `/usr/local` 命令链接,保留 `/opt/hft` 与 `/var/lib/hft`,从不触碰旧系统;dev2 退役用) |
| 部署记录 | `deployments/<ts>-<v>.json` | 同字段,`catalog_version` 改为 `refdata_version`,加 `host_profile_hash`、`channel`、`group`、`manifest` |

操作步骤见 [release-and-deploy](../docs/runbooks/release-and-deploy.md)。新主机(还没有 `current-core`)的第一次安装用暂存目录里解包的同名脚本引导,步骤同在该页。

### 单元组与分组激活

- 安装是一个归档一个版本目录;激活按组进行:`releasectl activate --version <v> --group core|cold|sim`(可选的 `chain` 组在启用链上组件时出现)。每组有自己的符号链接 `/opt/hft/current-<group>` 与 `previous-<group>`。**没有 `/opt/hft/current`**:每个单元、脚本、配置默认值与 runbook 命令都引用本组链接——
  - `core`:`hft-core@prod`、主机级脚本与生成物(`/opt/hft/current-core/deploy/…`,含 releasectl、`hft-window`、`hft-maint`、`hft-unit-idle`、`hft-shm-release`、`hft-sim-alternate`、`imds/`)、期望剖面、`hostprobe` 与 `hft-cfg` 的命令链接;
  - `sim`:`hft-core@sim`(其 `paths.release_manifest` = `/opt/hft/current-sim/RELEASE.json`,prod 为 `current-core`;默认值归 hft-core);
  - `cold`:kill-switch、watchtower、reconciler、journal-archiver、`hft-replay-verify` 的 replayer、Python 包(解释器 `/opt/hft/current-cold/venv/bin/python`,schema 目录 `/opt/hft/current-cold/schemas/strategies`),以及 `kill-switch`、`journal-archiver`、`replayer`、`hft-pipeline`、`hft-treasury` 的命令链接([路径表](#身份路径与-s3-前缀))。
  
  组是激活单位,与 slice(资源单位)无关。验证包不属于任何组(`/opt/hft/verify/current`)。
- 激活某组只 `disable --now` 该组 target 下的单元(冷生产规则不变);其他组继续运行。回滚同样按组。
- 跨组的接口(ctl 协议、journal 读者)按各自版本规则前后兼容;不兼容的发布在 PR 中声明"需要多组同时激活"。`sim` 组可以先于 `core` 组激活新版本,用模拟账户观察新版本([release-and-deploy](../docs/runbooks/release-and-deploy.md));此时 sim 实例的 `channel` 检查读 `current-sim` 的清单。
- 调优文件、udev 规则、`egress.toml`、polkit 规则、IMDS 锁定与凭证代理配置属于主机,不属于任何组:只随 `core` 组激活安装;dev2 护栏文件与机外看门狗同样随 `core` 组安装,但守护与看门狗单元的启停独立于 hft-core(激活不停它们)。

### HostProfile

采集项、输出格式与 `hostprobe` 用法归 [tools/hostprobe](../tools/README.md#hostprobe);hft-core 如何使用归 [hft-core](../apps/hft-core/README.md#hostprofile)。deploy 只拥有**期望值**:

- 生成器从 `hosts/<清单名>.toml`、hft-core 线程计划与本目录的静态配置生成期望剖面 `deploy/tuning/<清单名>.toml`,其 SHA-256 写入 `RELEASE.json` 的 `host_expect[<清单名>]`。网卡以设备序号描述,MAC 不进期望剖面;hostprobe 把运行期读到的 MAC 写进 `hostprofile.json`(`advisory`,只记录)。
- 每项期望标 `required`(不符即 hft-core 按 `host.on_mismatch` 拒绝启动,退出 3)或 `advisory`。升级主机的 `required`:隔离核集合(= `host.hot_cpus`)、`nohz_full`、`rcu_nocbs` 含隔离核与 IRQ 专用核、hugepages 数、THP(aarch64 `madvise`)、ENA 驱动版本、ENI-B/ENI-C(与启用时的 ENI-B2)在清单的设备序号上且合并与卸载参数相符、**`net.core.busy_poll = 0`、`net.core.busy_read = 0`、ENI-B 的 `napi_defer_hard_irqs = 0` 与 `gro_flush_timeout = 0`**、**IRQ 落点**(ENI-B 全部队列在 IRQ 专用核、ENI-A 在 hk 核、ENI-C 在 `eni_c` 核、启用时 ENI-B2 在 `eni_b2` 核)、managed IRQ 的 `effective_affinity` 不含 IRQ 专用核、IRQ 专用核上没有可运行任务、XPS 映射与 `irq-plan.json` 一致、chrony 同步且误差界 < 40 µs、`hft-sim.slice` 的 `AllowedCPUs` 与隔离集合不相交、`egress.toml` 中每个地址已配在清单指定的网卡上并关联 EIP。升级主机类另有 `required`:根 cgroup 的 `connect4` 上挂着 `hft_imds4`、`connect6` 上挂着 `hft_imds6`([IMDS 锁定](#imds-锁定与凭证代理))、`nf_conntrack` 未加载(采集归 tools)、`hftbatch.slice` 在根级且 `cpu.idle = 1`、chrony 与 hft-core 合计的 PHC 读取频率不超过 [hft-rt](../crates/hft-rt/README.md) 给出的 ENA PHC 读取上限、IRQ 专用核上的 keepawake 线程登记表(默认为空:是否保持唤醒由 M1.5 的 A/B 决定,决定启用时在同一改动里登记线程名与单元)。EIP 关联的期望值是"该私有 IP 在 IMDS `ipv4-associations` 中有公网关联"(IMDS 不给分配 ID,`eip_alloc` 只供人工核对)。`c4` 的 `required` 只有 busy poll 四项、"全部 IRQ 在 hk 核"、chrony 同步与 IMDS 两项;隔离核、`nohz_full`、`rcu_nocbs` 与大页是 `advisory`(hft-core 对 `c4` 不检查核隔离与大页,生成器仍为它产生这些设置);没有 IRQ 专用核、ENI-C、ENI-B2 与 XPS 各项。`dev2` 的 `required`:`hft.slice`、`hft-core.slice` 与 `hftbatch.slice` 的各上限与 hft-core 一致、`hftbatch.slice` 在根级、`cpu.idle = 1` 且无 `CPUQuota`、`MemorySwapMax=0`、`hft-dev2-guard.service` 为 active、表 `inet hft_imds` 已加载且含上述两条规则、`egress.toml` 的每个地址已配在 `[eni_b]` 网卡上且写了 `eip_alloc` 的地址有公网关联;`advisory`:其余全部。
- `pending_reboot`:发布内 GRUB 片段中任一参数在当前 `/proc/cmdline` 中缺失或取值不同时置位(cmdline 中片段以外的参数与参数顺序不参与比较);`dev2` 没有 GRUB 片段,恒为 `false`。

## 实现说明

<details><summary>实现要点</summary>

- 生成器(`deploy/gen/`,由 `cargo xtask gen-host` 调用)是纯函数:清单 + hft-core 的主机类档(线程计划、IRQ 核、内存表、slice、I/O 带宽与 tmpfs 数值、dev2 护栏数值)→ 生成物;生成物提交到仓库,CI 校验"重新生成无差异",并把大页推导与 hft-core 内存表对账。核号只在 hft-core 的线程计划里出现一次。
- **冷进程配置模板**(`config-templates/`):生成器按清单填主机相关值,其余照模板。`watchtower.toml.tmpl` 对 hft-telemetry `rules::REGISTRY` 中的每条规则(含 `ArchiverAlert`、`PipelineAlert`)各有一段开关与阈值,并含每个本机实例的 `instances[].hft_core_config`、`paths.archiver_alerts` 与 `PipelineAlert` 读取 pipeline `status.json` 的状态根 `paths.pipeline_state`(键名归 [watchtower](../apps/watchtower/README.md#配置文件字段));`/healthz` 监听地址与 ENA 计数的网卡名不在模板里(watchtower 启动时从 HostProfile 读)。`reconciler.toml.tmpl` 由生成器填 `refdata_blob`(本机 prod 实例当前 refdata 文件的路径;`ReplaceRefdata` 之后须替换文件并重启 reconciler,限制归 [reconciler](../apps/reconciler/README.md))、`http.max_rps`(模板按 venue 给出)与 `egress_bind`(`[egress].prod` 的私有 IP)。
- **ctl 签发者表** `/etc/hft/ctl-principals.toml`(hft-core 配置 `paths.ctl_principals`,只在启动时加载,改动需重启 hft-core,[hft-core 配置文件字段](../apps/hft-core/README.md#配置文件字段)):生成器把 `config-templates/ctl-principals.toml.tmpl`(服务 uid 与其 issuer 形式)与主机本地文件 `/etc/hft/ctl-operators.toml`(运维登录名、Telegram 用户 id 与角色)合成一份(格式归 [hft-ctl 鉴权](../crates/hft-ctl/README.md#鉴权));`core` 组激活时写入。签发者表中的 Admin 与 `.github/hft-admins.txt` 的人员核对在 [launch D 第 32 步](../docs/runbooks/launch.md#d-正式实盘m8)。
- 调优脚本、守护、看门狗与凭证代理全部幂等:先读当前值,不同才写,写后读回核对;`--dry-run` 只打印差异。
- 守护、看门狗、`hft-shm-release` 与凭证代理只用 Python 标准库(不依赖 venv,venv 损坏时仍可运行);守护通过 `systemctl stop`/`start` 操作单元(polkit 授权),单元名白名单写死在脚本里并与 polkit 规则一致(停:`hft-core@prod.service`、`hft-core@sim.service`、`hft-reconciler.service` 与批处理单元;启:只有 `hft-core@sim.service`)。
- `hft-unit-idle`、`hft-window`、`hft-maint` 是 Bash;`hft-unit-idle` 只调用 `systemctl is-active`,任一列出的单元为 `active`/`activating`/`deactivating` 即退出 1(ExecCondition 的"跳过"语义),否则 0。标志文件的判定直接用 `/usr/bin/test ! -e <标志>`(存在时退出 1 = 跳过)。
- `hft-sim-alternate --config /etc/hft/measure.toml`(主机本地文件:`seed`、`start`、`segment_days`(各段天数的列表:初始窗口为第一段,每次延长追加一段)与 `funding_hours_utc`;取值规则归 [testing §8.2](../docs/testing.md#82-延迟验收的测法)):`start` 不是偶数 UTC 整点或缺任一键 → 退出 2;从 `start` 起每 2 小时一块,含 `funding_hours_utc` 任一整点的块是资金费块,其余是普通块;**排程按段生成**:每段内两类块各自按由 `seed` 与段序号派生的随机排列,排在前 ⌈n/2⌉ 位的块先开、其余先关(两类各自先开与先关的块数相差 ≤ 1);每次调用算出当前小时 sim 应处的状态并切换到它(幂等;停止走 Drain,启动从检查点恢复);当前时刻早于 `start` 或晚于全部段 → 按"开"处理(sim 运行)。同一配置文件给出的排程逐块相同;判据与配对方法归 [testing §8](../docs/testing.md#8-上线门槛)。
- 发布脚本以 Trading 的 `deploy/scripts/*.sh` 为基础改写,不从零重写。

**规划的文件**

| 文件 | 职责 |
|---|---|
| `hosts/*.toml` | 主机清单(含 M7/M8 两份主清单、待命清单与探针清单) |
| `gen/*.rs`(xtask 模块) | 清单 + 线程计划 → 生成物(含 `egress.toml`、`irq-plan.json`、期望剖面) |
| `net/*.sh`、`irq/*.sh`、`resctrl/*.sh` | 运行期调优(非 dev2);`net/hft-egress-ip.sh` 配出口地址 |
| `imds/hft-imds-lock.sh`、`imds/hft-imds-connect.bpf.c`、`imds/hft-imds-dev2.nft`、`creds/hft-cred-broker`、`creds/consumers.toml.tmpl` | IMDS 锁定(按主机类两种实现)与凭证代理 |
| `bin/hft-unit-idle`、`bin/hft-shm-release`、`bin/hft-sim-alternate`、`bin/hft-maint` | ExecCondition 判定;干净 Drain 后释放热段;同机干扰测量的开关;维护静默 |
| `systemd/hft-core@.service` 与 `hft-core@prod.service.d/`、`hft-core@sim.service.d/` | 模板单元与按实例的 drop-in |
| `systemd/hft-verify.service`、`systemd/hft-verify.timer` | 验证包运行(按清单 `verify` 安装) |
| `systemd/hft-replay-resume.service`、`systemd/hft-sim-alternate.service`、`systemd/hft-sim-alternate.timer` | 回放结束后恢复 archiver/pipeline;门槛 ⑩ 同机干扰测量的整点切换 |
| `systemd/hft.slice`、`systemd/hft-core.slice`、`systemd/hft-sim.slice`、`systemd/hftbatch.slice` | slice 层级(数值来自生成物;`hftbatch.slice` 在根级) |
| `tmpfs/dev-shm-hft-prod.mount`、`tmpfs/dev-shm-hft-sim.mount` | 每实例热段 tmpfs |
| `dev2/*.slice.d/50-dev2.conf`、`dev2/*.d/50-dev2.conf`、`dev2/hft-dev2-guard`、`dev2/hft-dev2-guard.service`、`dev2/dev2-guard.toml.tmpl`、`dev2/hft-window` | dev2 护栏与联调窗口 |
| `offbox/hft-offbox-watchdog`、`offbox/hft-offbox-watchdog.service`、`offbox/offbox-watchdog.toml.example` | 机外看门狗(dev2;可选第三 AZ) |
| `polkit/50-hft-dev2-guard.rules`、`polkit/50-hft-kill-switch.rules` | 守护与 kill-switch 的最小授权 |
| `secrets/<unit>.toml.tmpl`、`secrets/iam-dev2.json.tmpl`、`secrets/iam-prod.json.tmpl`、`secrets/iam-consumers.json.tmpl` | 按单元的秘密清单模板与按主机角色、消费者的 IAM 策略 |
| `sysctl/`、`tuned/`、`chrony/`、`udev/` | 静态配置(`udev/60-hft.rules` 含 `/dev/cpu_dma_latency` 的属组) |
| `systemd/*` | 其余单元 |
| `config-templates/*.tmpl` | 冷进程配置模板与 ctl 签发者表模板 |
| `hosts/test-<class>.toml`、`dev2/tests/` | 生成器测试清单;护栏守护与 `hft-shm-release` 的测试 |
| `ami/*` | AMI |
| `release/*` | 打包(生产归档与验证包)、获取、安装(含 `install-verify`)、激活、回滚、预检、记录 |

</details>

## 延迟与资源契约

dev2 预检的 `journal-memory-budget` 同时检查实例 tmpfs 挂载、cgroup 总预算、实际 journal 布局和滚动余量；预算与进程余量数字由 [hft-core](../apps/hft-core/README.md#内存与-journal-段按主机类唯一归属) 生成。tmpfs 页全额计入 MemoryMax，不能只按匿名 RSS 验收。

本目录不在任何运行路径上。它交付三类验收数字:

- **升级主机层面**([02 §6](../design/02-硬件与托管方案.md)):隔离核 `osnoise` 24 h max < 20 µs、p99.9 < 5 µs;到 Binance `minrtt` 比现状(dev2)降 ≥ 0.5 ms、p99 RTT 抖动(24 h 内 RTT 样本的 p99 − p50)< 100 µs;chrony PHC 误差界 < 40 µs;冷启动开机到 hostprobe 通过 < 3 min。chrony 误差界是主机层的验收;µs 级分段的跨时钟域换算按 `ClockStatus` 校准对插值并逐样本给误差界(规则归 [latency-budget](../docs/latency-budget.md)),不依赖这个界。`kernel_rx` 等主机内分段的预算归 latency-budget。
- **sim 同机干扰**:判据与测法唯一归属 [testing §8](../docs/testing.md#8-上线门槛) 门槛 ⑩(测量时用 `bin/hft-sim-alternate` 按种子交替开关 sim,步骤在 [host-upgrade](../docs/runbooks/host-upgrade.md));本目录保证 sim 线程在隔离核与 IRQ 专用核上出现 0 次、sim 的网卡 IRQ 不落在 prod 的任何核上。
- **`dev2` 护栏**:旧系统正式账户的关键指标(信号 → 下单延迟分布、进程 CPU 与内存、`system.slice` 压力)在 hft 运行与不运行时无可测劣化;守护从信号满足二级条件到第一个 hft-core 收到 SIGTERM ≤ 一个采样周期;M3 标定演练通过。阈值与对照方法归 [hft-core](../apps/hft-core/README.md#dev2-护栏)。

## 测试与验收

用例与验收见 [TESTS.md](TESTS.md)。

## 依赖

- 工具:`ethtool`、`iproute2`、`nftables`(dev2)、`bpftool` 与 `clang`(CI 编译 BPF 对象)、`acl`、`rtla`、`perf`、`chrony`、`tuned`、`packer`、AWS CLI v2(AMI 内固定版本;只在 `hft-eni-attach`、`hft-egress-ip` 的只读核对、凭证代理与 AMI 构建)、`shellcheck`(CI)。
- 仓库:`tools/hostprobe`、`tools/hft-secrets`、`cargo xtask gen-host`;线程计划与数值来自 [hft-core](../apps/hft-core/README.md);验证包内容来自 [development](../docs/development.md#验证包工作流verify-bundleyml) 与 [tools hft-verify](../tools/README.md#hft-verify)。
- 现有:Trading `deploy/scripts/`(发布脚本基础)、`deploy/systemd/`(加固集合)、`deploy/polkit/`(dev2 守护的授权写法)。

## 延伸阅读

- [round8-decisions](../design/_work/round8-decisions.md)、[round8f 简报](../design/_work/round8f-spec-brief.md)、[02 §2、§4–6](../design/02-硬件与托管方案.md)、[research/05 §3–4](../design/research/05-hardware-hosting-os-tuning.md)、[10 §6](../design/10-语言分工与代码组织.md)、[11](../design/11-实施路线图.md)
- Trading AGENTS.md(本仓库之外:`Trading/AGENTS.md`)(发布纪律与事故教训)
- [hft-core](../apps/hft-core/README.md)(线程计划、IRQ 核、内存表、slice、I/O 与 tmpfs 数值、出口 IP 校验、dev2 数值、sim 实例、HostProfile 消费、停机标记)、[kill-switch](../apps/kill-switch/README.md)(触发套接字协议、`stop_core` 与停机标记)、[watchtower](../apps/watchtower/README.md)(`/healthz` 与 `maintenance_until`)、[development](../docs/development.md)(仓库、CI 与验证包工作流)
- Agent Notes:[rx-irq-driven](../.agents/notes/proposed/architecture/2026-10-03-rx-irq-driven.md)、[irq-split-reserve](../.agents/notes/proposed/architecture/2026-10-03-irq-split-reserve.md)、[host-class-c32](../.agents/notes/proposed/architecture/2026-10-03-host-class-c32.md)、[verify-on-server](../.agents/notes/proposed/process/2026-10-03-verify-on-server.md)、[dev2-guard-system-slice-psi](../.agents/notes/proposed/process/2026-10-03-dev2-guard-system-slice-psi.md)、[imds-lockdown](../.agents/notes/proposed/architecture/2026-10-03-imds-lockdown.md)、[offbox-watchdog](../.agents/notes/proposed/architecture/2026-10-03-offbox-watchdog.md)、[manual-hold-and-stop-marker](../.agents/notes/proposed/architecture/2026-10-03-manual-hold-and-stop-marker.md)、[sim-colocated-with-prod](../.agents/notes/proposed/architecture/2026-10-03-sim-colocated-with-prod.md)、[no-small-live](../.agents/notes/proposed/architecture/2026-10-03-no-small-live.md)、[sim-only-instance](../.agents/notes/proposed/architecture/2026-10-03-sim-only-instance.md)、[ci-org-and-profiles](../.agents/notes/proposed/architecture/2026-10-03-ci-org-and-profiles.md)、[dev2-root-batch-slice](../.agents/notes/proposed/process/2026-10-03-dev2-root-batch-slice.md)
- runbooks:[launch](../docs/runbooks/launch.md)、[release-and-deploy](../docs/runbooks/release-and-deploy.md)、[host-upgrade](../docs/runbooks/host-upgrade.md)、[host-tuning](../docs/runbooks/host-tuning.md)、[incident-response](../docs/runbooks/incident-response.md)

冷激活在替换链接前查询本组单元的 LoadState/ActiveState，仅跳过 not-found 且 inactive 的单元，其余先 disable --now；查询失败关闭。安装单元并 daemon-reload 后再次 disable --now，保证首次安装也保持冷态。core 主组为 hft-ctl，附加 hft 和 hft-stopmark，ctl socket 继承受控组。dev2 清单 eni_b.device_index 可显式提供 IMDS device-index（默认 1）；单网卡测试主机使用 0，hostprobe 按该角色映射核对原有网卡/出口条件。

## 已知限制与后续工作

测试网窗口通过 `dev2/hft-window start|stop` 控制，开窗先阻止并停止批处理，确认 prod 停止且其 tmpfs 已用量只剩心跳页后才删除窗口标志；失败保留标志。生成的 prod 单元检查窗口与持久 tripped。预检读取实际 cgroup 的 CPU、memory/swap、I/O 限额及 OOMScoreAdjust，拒绝 dev2 隔离核配置；不运行压力测试。冷激活先停旧实例再切组链接，并建立当前装配需要的 hft 用户和组；首次激活在启动前创建 `/data/prod`，供 systemd 写路径沙箱及冷盘预检使用，后续 ExecStartPre 仍校正目录模式。

- 可执行护栏入口是 `dev2/hft-dev2-guard`，配置模板为 `dev2/dev2-guard.toml.tmpl`；支持只读 `--dry-run`，`clear` 在 root 调用时先切换到 `hft-guard` 再写事件。采样器读取 proc/cgroup 与只读健康导出文件，动作使用固定 hft 单元白名单；事件写 `events.jsonl`，告警输出 journald。Telegram 直发与旧系统主单元状态尚未接线，不能据此宣称 dev2 共机或测试网验收完成。`bin/hft-shm-release` 只在有效的本实例心跳为 Stopped 时删除 `.jrn`/`.jrn.preparing`，保留心跳与其他证据，拒绝符号链接树。

- [生成器](gen/generate.py)经 `cargo xtask gen-host` 调用，当前只接受 [dev2 prod 测试清单](hosts/test-dev2-prod.toml)的字段集合与角色组合；拒绝 sim、升级主机与验证服务。生成单元、slice、tmpfs、出口地址、安装计划及 hostprobe 的资源期望；内存、I/O、冷盘门限读取 hft-core 的归属表。输出不包含内核、IRQ 或凭证代理；dev2 的 IMDS 服务调用 [uid 隔离脚本](imds/dev2_lock.py)，单事务替换 hft 自有 nft 表，不改其他表，不支持升级主机 BPF。测试清单的地址只供夹具使用；[Binance 测试主机清单](hosts/binance-testnet-20261015.toml)登记获准的独立测试主机实测接口与私有地址，仅允许非正式 prod/testnet 装配。完整主机矩阵仍未验收。
- [发布工具](release/release.py)提供安全解包、逐文件校验、离线 venv、安装记录和按组冷激活；未装配的组拒绝激活，应用单元停止名单不越组，最后运行 hostprobe 并记录 pending。安装的架构检查覆盖全部 `bin/` 文件，版本探针目前只对 hft-core 执行。下载器读取 root 0600 token，按精确版本下载并核对 SHA-256；跨域重定向去除认证头。
- [打包器](release/package.py)接收 [CI staging](release/stage.py) 从 Cargo 可执行目标、原生 aarch64 二进制、导出 schema、生成物与 wheelhouse 构造的 `dist/stage/` 和元数据；缺失元数据、SBOM 或主机期望均失败。CycloneDX 清单来自 Cargo.lock 与 wheel 文件；本地 wheel 以哈希钉住并加入安装锁。CPU 特性取原生 runner 的完整用户态特性集合，目标主机须全部满足这一保守兼容包络。`release-aarch64.yml` 的 `artifact_only` 只上传制品，不创建 tag 或 Release，也不代替主机验收。
- [预检](release/preflight.py)只验当前 dev2 prod/testnet 装配：架构与 CPU 特性、独立冷卷及容量、chrony、无 Rust 工具链、分组路径、guard、slice、IMDS uid 集及双栈规则、正式秘密访问拒绝、hft-secrets 检查、hft-core 配置检查和重新采集的 hostprobe 期望。`preflight.toml` 的冷盘门限由生成器从 hft-core 默认表生成，因当前运行配置尚不接受 storage 段而独立安装。缺任一输入或命令即失败；完整 S3/凭证代理、ctl 签发者生成、服务用户权限闭包和升级主机矩阵未覆盖。秘密清单、已加载的 IMDS 规则和实测主机配置缺失仍阻止服务启动；hft-secrets 使用 tools 的实现。
- CI 容器验证 root 清除标志、降权后的事件属主及 Stopped/Draining 热段释放。cgroup CPU 让出、OOM 顺序、loop 设备 I/O、真实 systemd 启停、ENI/EIP、Secrets Manager/IAM 和 aarch64 指令集仍需专门的特权环境或真实主机；未把替身命令执行记作这些用例通过。

- `dev2` 护栏的全部数值待标定(M3 前在 dev2 上实测:先只跑守护与只读采集一周,再逐步加 hft 进程,最后做 dev2 护栏第 10 条的标定演练);旧系统状态的探测方式需与其维护者确认(只用于告警,不触发停机)。dev2 上 sim 常驻、联调窗口再加一个测试网实例时资源最紧:窗口内批处理由窗口标志与 `hft-window` 停止。
- 升级主机的档位(默认 `c32`;`c16` 只在 M3 末满订阅探针达标且启用 `epoll_pwait(0)` 时保留)与是否启用 ENI-B2 在 M3 末探针后选定;清单、aarch64 GRUB 模板与 AMI 在定档后编写;x86 `metal` 的清单与模板只在可选的 metal 剖面立项时编写。
- 出口 IP 隔离只解决按 IP 计的限额;按账户或 UID 计的限额与 sim 无关(sim 不持账户)。
- dev2 的实例角色与旧系统共用:hft 的 uid 被挡在 IMDS 之外,但旧系统的进程仍能以该角色读 `hft/testnet/*` 与 `hft/ops/*` 的秘密并扮演 hft 的消费者角色(dev2 上只有测试网 key,不放正式账户)。
- dev2 退路 (a) 的新主机上没有 watchtower:`hft-verify` 与 `hft-replay-verify` 的结果只在 S3 与本机文件里,负责人每日查看(`hft-verify ledger show`、回放报告),直到另行部署告警。
- 机外看门狗的观察点是 dev2(必要时加第三 AZ 的 `t4g.nano`):dev2 上的 watchtower 会报看门狗进程退出(`ProcessDown`),但 dev2 整机宕机且没有第三 AZ 实例时没有任何告警。
- `/run` 上的标志(窗口、维护)在整机重启后不存在(窗口随之结束);tripped 标志在 `/var/lib/hft/guard/`,重启后仍在,只能 `hft-dev2-guard clear` 清除。dev2 重启属人工或整机事件,之后按 incident-response P 节核对。
- 以 root 运行的单元不套用 `ProtectSystem=strict` 与空能力集;按单元收紧到最小能力集(逐单元写 `CapabilityBoundingSet=`)是后续工作。
- colo 形态(Onload、GNSS PTP)不在本目录范围。
- 可选扩展:第二台 warm standby 主机的清单(触发条件:单机故障的停机时间不可接受)。

### 开发备忘

<details><summary>给实现者</summary>

- **待核实**:Graviton(c8g)上 ENA 的队列数、中断合并可调项与 `napi_defer_hard_irqs`/`gro_flush_timeout` 的 sysfs 可写性、XPS 可写性与 TX 队列数、HW RX 时间戳与 PTP(假设 A7)、resctrl 是否可用、NVMe managed IRQ 的实际落点;ENA ntuple 在后挂 ENI 上是否可用(调研结论冲突:[research/01 §5.1](../design/research/01-network-kernel-bypass-tls.md) 与 [02 §2.1](../design/02-硬件与托管方案.md));核实方法:M1.5 的租用实例上 `ethtool -l/-c/-k/-U`、读写 sysfs 并读回、读 `/proc/irq/*/effective_affinity_list` 与 `/proc/interrupts` 的 nvme 行。
- **待核实**:Ubuntu 上 ENI-B/ENI-C 的附加私有 IP 是否需要手工 `ip addr add` 与源地址路由(netplan/cloud-init 不自动配置后挂网卡的附加地址)、networkd 重配是否清掉它们、IMDS `ipv4-associations` 与按设备序号取 MAC 的路径;核实方法:在租用实例上分配后读 `ip -4 addr`、`ip rule` 与 IMDS,再 `networkctl reload` 复读。
- **待核实**:各所按 IP 的连接数与权重口径(sim 出口单独计的前提);核实方法:读 venue README 的限速表,升级前在新主机上用两个出口分别试连。
- **待核实**:dev2 的 cgroup 版本与 PSI(v2 才有 `MemorySwapMax`、子 slice 上限、`system.slice/cpu.pressure` 与 `memory.pressure`);核实方法:服务器上 `stat -fc %T /sys/fs/cgroup`(应为 `cgroup2fs`)、`ls /proc/pressure` 与 `cat /sys/fs/cgroup/system.slice/cpu.pressure`。
- **待核实**:dev2 上旧系统的单元名与所在 slice(守护的白名单与 `CPUWeight` 对比需要);核实方法:`systemctl list-units` 与 `systemd-cgls`,只读。
- **待核实**:dev2 的 systemd 与内核版本支持 `CPUWeight=idle`(systemd ≥ 252、内核 ≥ 5.15)及 cgroup `cpu.idle` 让出的实际程度;核实方法:`systemctl --version`、`uname -r` 只读,M3 标定演练读 `system.slice` 与 `hftbatch.slice` 的 `cpu.stat`。
- **待核实**:aws 内核上 cgroup `connect4`/`connect6` 程序挂在根 cgroup 时对全部进程生效、与 systemd 自己挂的程序共存(`BPF_F_ALLOW_MULTI`);冷副本卷的文件系统(ext4/xfs)默认支持 POSIX ACL 且默认 ACL 被 hft-core 新建的子目录继承;核实方法:M1.5 租用实例上以非 root 用户 `curl` IMDS、`getfacl` 新建子目录。
- **待核实**:Ubuntu AMI 是否预装 `ec2-instance-connect`、移除后 SSM 破窗与 SSH 公钥登录可用;各 SDK(boto3、Rust aws-sdk)对共享凭证文件的到期重读;dev2 与升级主机的私网可达性(机外看门狗);核实方法:在租用实例上实测。
- 实现顺序:发布脚本改写(按组链接、按版本 venv)→ slice、tmpfs 挂载与 dev2 护栏(守护 + polkit + `hft-window` + 标志与 `ExecCondition` + `hft-shm-release`)→ 验证包安装与 `hft-verify` 单元 → 秘密前缀、按单元清单与 IAM 模板 → `hft-core@.service` 模板与 `sim` 组(dev2 上先用)→ 清单与生成器(含 `egress.toml`、`irq-plan.json`、大页对账)→ (M1.5 起在租用实例上)sysctl/GRUB/IRQ/XPS/ENI/出口地址 → hostprobe 期望剖面 → IMDS 锁定与凭证代理 → 机外看门狗与 `hft-maint` → AMI。

</details>
