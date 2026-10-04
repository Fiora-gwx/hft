# AGENTS.md — deploy

本子树的规则补充根 [AGENTS.md](../AGENTS.md#常驻规则),沿用 Trading AGENTS.md(本仓库之外:`Trading/AGENTS.md`) 的发布纪律。规格在 [README](README.md),步骤在 [runbooks](../docs/runbooks/README.md),仓库与 CI 在 [development](../docs/development.md#仓库与-ci)。

## 只装 CI 制品

- 服务器只安装 CI 产出的归档与验证包,不编译、不装工具链(`preflight.sh` 发现编译工具即失败);服务器自己拉取。长时间 fuzz/DST 只运行验证包。
- 开发构建只用于 `dev2`;正式 Live 账户只运行正式版。不移动已发布的 tag。

## 调优即代码

- 每个低延迟设置(GRUB、sysctl、网卡/IRQ、hugepages、cpuset、chrony、nftables、单元与 slice、出口地址)与 dev2 护栏都来自本目录、随发布归档,不在主机上手改。
- 收包只有中断驱动:busy poll 全关、交易网卡 IRQ 独占一核,均为 hostprobe 的 hard 项([README](README.md#运行期调优))。
- 主机参数的唯一来源是 `hosts/<清单名>.toml`;核号、IRQ 核、内存与护栏数值归 [hft-core](../apps/hft-core/README.md#线程计划),生成器读取,**清单与单元里不手写核号**;MAC 不进清单与发布哈希。
- 一次只改一项调优,附 24 h 前后直方图对比。

## dev2 与旧系统共机

- 旧系统优先:除 kill-switch、护栏守护与机外看门狗外,`hft-*` 单元都在 `hft.slice` 的子 slice 或根级 `hftbatch.slice`(`CPUWeight=idle`)内,`MemorySwapMax=0`;守护只按停机信号(`system.slice` 压力等)停 hft(不停 watchtower 与 ops-bot)并写持久的 tripped 标志(人工清除),只自动重启 `hft-core@sim`;整机 PSI 与旧系统异常只告警。
- 联调窗口只经 `dev2/hft-window start|stop`;批处理单元以 `ExecCondition=` 查护栏与窗口标志([dev2 护栏](README.md#dev2-护栏))并互斥,不用 `Conflicts=` 与 `mask`。
- 任何 hft 脚本只操作 `hft-*` 单元,从不读写、启停旧系统的单元与文件。
- dev2 默认不做主机级调优,IMDS 只挡 hft 各 uid;**不放正式实盘账户**(`formal_live = false`、IAM 只读测试网前缀、预检拒绝正式端点)。

## 模拟实例

- `hft-core@sim` 以 `hft-sim` 运行,不取秘密,只用非隔离核、自己的 slice、ENI-C 与出口地址;单元里出现 `hft-secrets`、prod 运行目录、隔离核或 prod 出口地址即为缺陷。

## 发布纪律

- 归档 + `.sha256` + `RELEASE.json`(逐文件 SHA-256);冷生产:安装与激活不启动交易或采集单元;保留当前与恰好一个已验证的回滚版本。
- 没有 `/opt/hft/current`:按组激活,一律引用本组链接 `/opt/hft/current-<group>`;用户、组、路径与 S3 前缀只在 [README 注册表](README.md#身份路径与-s3-前缀)登记。
- 代码、协议、内核参数、单元文件走 CI 与发布;参数、名单、模式走控制命令。
- 秘密不进归档与仓库,启动时由 `hft-secrets` 取到 tmpfs;交易 key 无提现权限并只绑定 prod 出口 EIP。
- hft 的服务用户都访问不到 IMDS,冷进程用凭证代理发放的短期凭证([README](README.md#imds-锁定与凭证代理))。

## 激活即校验

- 激活最后一步运行 `hostprobe`;`pending_reboot` 时不得启动 hft-core。验收以"第一分钟已验证"为准。

## 脚本与单元

- Bash 脚本 `set -euo pipefail`、幂等、可 `--dry-run`,CI 跑 shellcheck(先去掉 CR)。
- 目录逐级显式设属主与权限;上线前以服务用户身份跑一遍单元命令。
- 每个单元显式 `User=` 与 `Slice=`;同一目录只由一个单元声明 `RuntimeDirectory=`;覆盖文件同时重写 `CPUAffinity`;不用 `Requires=`/`Requisite=` 依赖会重启的单元(改用 `ExecStartPre` 的 `is-active` 检查);配置错误的退出码设 `RestartPreventExitStatus`。
- polkit 按"用户 + 单元 + 动词"写死最小授权:守护只停白名单内的 `hft-*`、只启动 `hft-core@sim`,kill-switch 只停 `hft-core@prod`。
- hft-core、kill-switch 用 `Type=notify` + `WatchdogSec`;看门狗心跳只来自 housekeeping 线程。
