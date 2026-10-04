---
component: deploy
prefix: DEP
spec: README.md
---

# deploy 测试规格

格式与规则见 [docs/tests](../docs/tests/README.md)。

**公共夹具**(`deploy/tests/fixtures/`;用例在 `前置` 中按名引用,并写出该用例依赖的取值):

- 清单夹具目录 `hosts/`:
  - `tokyo-cex-1`:`class = "c32"`、`instances = ["prod", "sim"]`、`formal_live = true`、`verify = true`、`instance_type = "c8g.8xlarge"`、`az_id = "apne1-az4"`、`placement_group = "hft-cpg-az4-a"`、`ena_driver = "2.17.3"`;`[eni_b]` `device_index = 1`、`queues = "max"`;`[eni_c]` `device_index = 2`、`queues = 1`;`[eni_b2]` `enabled = false`、`device_index = 3`;`[egress]` `prod = [{eni = "b", ip = "10.0.1.10", eip_alloc = "eipalloc-0b1"}]`、`sim = [{eni = "c", ip = "10.0.1.20", eip_alloc = "eipalloc-0c1"}]`;`[storage]` `cold_gib = 1100`;`[resctrl]` `enabled = false`。
  - `tokyo-cex-1-live`:与 `tokyo-cex-1` 只差 `verify = false`。
  - `tokyo-cex-1.standby`、`tokyo-cex-1-live.standby`:`host`、`class`、`formal_live`、`verify`、`instances` 与各自主清单相同;`az_id = "apne1-az1"`、`placement_group = "hft-cpg-az1-a"`;`[egress]` 私有 IP `10.0.2.10`(prod)与 `10.0.2.20`(sim),`eip_alloc` 与主清单相同。
  - `probe-c16`、`probe-c32`:`class` 分别为 `c16`、`c32`,`formal_live = false`、`verify = false`;`[egress]` `prod = 10.0.1.110`、`sim = 10.0.1.120`。
  - `dev2-1`:README 的 dev2 示例,`[eni_b] name = "ens5"`,`prod = [{ip = "172.31.10.5"}]`、`sim = [{ip = "172.31.10.6", eip_alloc = "eipalloc-0d2"}]`。
  - `c4-1`:`class = "c4"`、`instances = ["prod"]`、`formal_live = false`、`verify = false`、`[eni_b] device_index = 1`、`prod = [{eni = "b", ip = "10.0.1.40", eip_alloc = "eipalloc-0b4"}]`。
- hft-core 主机类档夹具(值抄自 hft-core 线程计划,只用于本文件):`c32`——hk 核 0、`eni_b` 核 1、隔离核 5–17(seq 核 5,发送热核 5–17)、冷核 18 与 29、批处理核 19–21 与 30–31(ENI-B2 未启用时另含 28)、`eni_c` 核 22、sim 核 23–27、预留 `eni_b2` 核 28;`c4`——hk 核 0、隔离核 1–3、seq 核 1;`dev2`——两核,无隔离核。
- 脚本测试的替身根:脚本以环境变量 `HFT_SYSROOT=<临时目录>` 运行,`/proc`、`/sys`、`/etc`、`/run`、`/var/lib`、`/dev/shm` 都落在其下;`systemctl`、`ethtool`、`ip`、`nft`、`bpftool`、`aws`、`runuser` 与 IMDS(`169.254.169.254` 的 HTTP 替身)由替身实现,记录调用序列。替身根与环境变量只用于测试,生产路径不变。
- 容器:`proc` 用例在 CI 的特权 systemd 容器(`ubuntu:24.04`,cgroup v2,systemd ≥ 255)中运行,按用例写明的清单夹具执行安装与激活;hft 二进制以夹具替身代替(遵守相同的 `sd_notify`、退出码与建文件行为)。

## 覆盖

| README 小节 | 用例 |
|---|---|
| 概要 | 不适用:非规范 |
| 目录 | 不适用:非规范 |
| 使用本组件 | 不适用:见子节 |
| 目录布局 | DEP-001、DEP-002、DEP-269 |
| 制品从哪里来 | DEP-003、DEP-004、DEP-005、CORE-046 |
| 主机清单 | DEP-010–DEP-019、DEP-149、DEP-265、TOOL-064 |
| GRUB | DEP-019、DEP-020、DEP-021、DEP-248 |
| 运行期调优 | DEP-030–DEP-044、DEP-210 |
| 出口 IP | DEP-033、DEP-034、DEP-050、DEP-051、CORE-168、CORE-171、CORE-176、CORE-177 |
| slice 与资源切分 | DEP-060–DEP-066、CORE-146、CORE-160、CORE-161、CORE-162、CORE-163 |
| dev2 护栏 | DEP-070–DEP-087、CORE-144、CORE-186、CORE-187、CORE-189、CORE-191、CORE-195、CORE-200、CORE-201、CORE-202、CORE-210、CORE-216 |
| hugepages 与 hugetlbfs | DEP-020、DEP-090、DEP-091、DEP-092、TOOL-063 |
| resctrl / CAT | DEP-095、DEP-096 |
| systemd 单元 | DEP-030、DEP-035、DEP-120–DEP-148、DEP-155、DEP-156、DEP-157、DEP-238、DEP-268、CORE-164 |
| 身份、路径与 S3 前缀 | DEP-150–DEP-160 |
| 秘密与实例角色 | DEP-051、DEP-165–DEP-168、CORE-041 |
| IMDS 锁定与凭证代理 | DEP-170–DEP-182 |
| 模拟实例(sim) | DEP-064、DEP-125、DEP-185、DEP-186、DEP-187、DEP-264、CORE-040、CORE-044、CORE-171 |
| 验证包与 hft-verify | DEP-190–DEP-198 |
| 机外看门狗 | DEP-149、DEP-200–DEP-205 |
| AMI 构建 | DEP-150、DEP-210、DEP-211 |
| 发布归档与 releasectl | DEP-004、DEP-014、DEP-168、DEP-179、DEP-186、DEP-196、DEP-220–DEP-238 |
| 单元组与分组激活 | DEP-148、DEP-187、DEP-240、DEP-241、DEP-242、CORE-047 |
| HostProfile | DEP-245–DEP-249、TOOL-106、TOOL-107、CORE-342 |
| 实现说明 | DEP-041、DEP-042、DEP-080、DEP-139、DEP-250–DEP-253、DEP-266、DEP-267、DEP-269 |
| 延迟与资源契约 | DEP-085、DEP-260–DEP-264、CORE-216、CORE-217 |
| 测试与验收 | 不适用:非规范(README 中只保留指向本文件的链接) |
| 依赖 | 不适用:非规范 |
| 延伸阅读 | 不适用:非规范 |
| 已知限制与后续工作 | 不适用:非规范 |
| 开发备忘 | 不适用:非规范 |

## 用例

### DEP-001 发布归档的部署树包含目录布局列出的每个文件

- 规则:[目录布局](README.md#目录布局)
- 级别:table
- 里程碑:M3
- 前置:夹具提交上运行 `deploy/release/package-release.sh v0.0.1 <git_sha> dist`(`SOURCE_DATE_EPOCH = 0`),解包到临时目录;根目录为 `hft-v0.0.1-aarch64/`
- 实现:`deploy/tests/test_layout.py::dep_001_archive_layout`

| 行 | 归档内路径 | 期望 |
|---|---|---|
| DEP-001.1 | `deploy/grub/` | 恰有两份 `50-hft.cfg` 模板,一份标 aarch64、一份标 x86 |
| DEP-001.2 | `deploy/sysctl/90-hft.conf`、`deploy/tuned/hft-latency/tuned.conf`、`deploy/chrony/chrony.conf`、`deploy/udev/60-hft.rules` | 4 个文件都存在 |
| DEP-001.3 | `deploy/net/` | 恰有 `hft-eni-attach.sh`、`hft-net-tune.sh`、`hft-egress-ip.sh`,模式 0755 |
| DEP-001.4 | `deploy/irq/` | 恰有 `hft-irq-pin.sh`、`hft-workqueue.sh`,模式 0755 |
| DEP-001.5 | `deploy/imds/` | `hft-imds-lock.sh`(0755)、`hft-imds-connect.bpf.c`、`hft-imds-connect.bpf.o`(ELF,机器类型 BPF)、`hft-imds-dev2.nft` |
| DEP-001.6 | `deploy/creds/` | `hft-cred-broker`(0755)、`consumers.toml.tmpl` |
| DEP-001.7 | `deploy/hugepages/`、`deploy/tmpfs/` | `dev-hugepages2M.mount`;`dev-shm-hft-prod.mount`、`dev-shm-hft-sim.mount` |
| DEP-001.8 | `deploy/resctrl/` | `hft-resctrl.sh`(0755) |
| DEP-001.9 | `deploy/bin/` | 恰有 `hft-unit-idle`、`hft-shm-release`、`hft-sim-alternate`、`hft-maint`,模式 0755 |
| DEP-001.10 | `deploy/dev2/` | `hft-dev2-guard`(0755)、`hft-dev2-guard.service`、`dev2-guard.toml.tmpl`、`hft-window`(0755)、`hft-tune-dev2.sh`(0755),以及每个带 dev2 覆盖的单元与 slice 一个 `<名>.d/50-dev2.conf` |
| DEP-001.11 | `deploy/offbox/` | `hft-offbox-watchdog`(0755)、`hft-offbox-watchdog.service`、`offbox-watchdog.toml.example` |
| DEP-001.12 | `deploy/polkit/` 与归档顶层 `polkit/` | 都恰有 `50-hft-dev2-guard.rules`、`50-hft-kill-switch.rules` |
| DEP-001.13 | `deploy/secrets/` | `iam-dev2.json.tmpl`、`iam-prod.json.tmpl`、`iam-consumers.json.tmpl`,以及 `hft-core`、`kill-switch`、`reconciler`、`watchtower`、`ops-bot`、`dev2-guard`、`offbox-watchdog`、`cred-broker` 各一份 `<unit>.toml.tmpl`;没有 `hft-core-sim` 或 `sim` 的模板 |
| DEP-001.14 | `deploy/release/` | `package-release.sh`、`package-verify.sh`、`fetch-release.sh`、`releasectl.sh`、`install-release.sh`、`activate-release.sh`、`rollback.sh`、`preflight.sh`、`record-deployment.py` |
| DEP-001.15 | `deploy/tuning/` | 夹具 `hosts/` 中每份清单恰有一个 `<清单名>.toml` |
| DEP-001.16 | `deploy/ami/` | `tokyo-cex.pkr.hcl` 与 `provision/*.sh` |
| DEP-001.17 | `bin/` | 含 `hostprobe` 与 `hft-secrets` |

### DEP-002 部署文件与生成物中没有单一的 current、previous 与 venv 路径

- 规则:[目录布局](README.md#目录布局)、[单元组与分组激活](README.md#单元组与分组激活)
- 级别:unit
- 里程碑:M3
- 前置:DEP-001 的解包目录;夹具 `hosts/` 全部清单的生成物
- 步骤:
  1. 在 `deploy/`、`systemd/`、`polkit/`、`config-templates/` 与全部生成物中搜索正则 `/opt/hft/(current|previous)(/|"|'|\s|$)` 与 `/opt/hft/venv`
- 期望:
  - 两个正则的命中数都为 0;按组的链接 `/opt/hft/current-<group>`、`/opt/hft/previous-<group>` 不算命中
- 实现:`deploy/tests/test_layout.py::dep_002_no_ungrouped_links`

### DEP-003 打包按版本形式写出制品名与 `channel`

- 规则:[制品从哪里来](README.md#制品从哪里来)、[发布归档与 releasectl](README.md#发布归档与-releasectl)
- 级别:table
- 里程碑:M3
- 前置:夹具提交;`SOURCE_DATE_EPOCH = 0`;每行运行一次对应的打包脚本
- 实现:`deploy/tests/test_release.py::dep_003_artifact_names`

| 行 | 命令 | 期望 |
|---|---|---|
| DEP-003.1 | `package-release.sh v1.2.3 <sha> dist` | 写出 `dist/hft-v1.2.3-aarch64.tar.zst` 与 `.sha256`;`RELEASE.json.version = "v1.2.3"`、`channel = "release"` |
| DEP-003.2 | `package-release.sh v1.2.3-dev.4 <sha> dist` | `hft-v1.2.3-dev.4-aarch64.tar.zst`;`channel = "dev"` |
| DEP-003.3 | `package-verify.sh v1.2.3 <sha> dist` | `dist/hft-verify-v1.2.3-aarch64.tar.zst` 与 `.sha256`;归档内没有 `bin/hft-core` 及任何生产二进制 |

### DEP-004 开发构建只能装在 dev2 清单的主机上

- 规则:[制品从哪里来](README.md#制品从哪里来)、[发布归档与 releasectl](README.md#发布归档与-releasectl)
- 级别:table
- 里程碑:M3
- 前置:替身根;`/etc/hft/host-local.toml` 的 `manifest` 按行;归档为 DEP-003 产出、其余校验都能通过
- 实现:`deploy/tests/test_release.py::dep_004_dev_build_dev2_only`

| 行 | 本机清单 | 归档 `channel` | 期望 |
|---|---|---|---|
| DEP-004.1 | `dev2-1` | `dev` | `releasectl install` 退出 0;`/opt/hft/releases/v1.2.3-dev.4/` 存在 |
| DEP-004.2 | `tokyo-cex-1` | `dev` | 退出 1;没有新的版本目录;`current-*` 链接不变 |
| DEP-004.3 | `probe-c32` | `dev` | 退出 1 |
| DEP-004.4 | `c4-1` | `dev` | 退出 1 |
| DEP-004.5 | `tokyo-cex-1` | `release` | 退出 0 |

### DEP-005 M3 末探针用探针清单安装当前正式版并能启动两个实例

- 规则:[制品从哪里来](README.md#制品从哪里来)
- 级别:host
- 里程碑:M3
- 环境:按需租用的 `c8g.4xlarge` 或 `c8g.8xlarge`(预建 ENI-B/ENI-C 与 EIP 已挂),Ubuntu 24.04
- 授权:`[需授权]`(租用实例、挂网卡与 EIP 是 AWS 操作)
- 步骤:
  1. 写 `/etc/hft/host-local.toml` 的 `manifest = "probe-c16"`(或 `probe-c32`),按 [host-tuning](../docs/runbooks/host-tuning.md) 装当前正式版并激活 `core`、`sim` 组,重启使 GRUB 生效
  2. `hostprobe verify --expected /opt/hft/current-core/deploy/tuning/<清单名>.toml --out /run/hft/hostprofile.json`
  3. 启动 `hft-core@prod`(测试网 key)与 `hft-core@sim`
  4. 探针结束后在各所控制台核对测试网 key 的 IP 白名单
- 通过标准:步骤 1 的 `host_expect` 校验通过(安装退出 0);步骤 2 退出 0;步骤 3 两个实例都到 `Running`;步骤 4 白名单中没有探针清单 `[egress]` 的公网地址
- 证据:`ops-log/<UTC 日期>-DEP-005.md`(安装输出、`hostprofile.json` 的 `verify` 段、白名单截图)
- 实现:`deploy/tests/host/DEP-005.sh`

### DEP-010 生成器对每个失败条件退出 1,对允许的差异退出 0

- 规则:[主机清单](README.md#主机清单)
- 级别:table
- 里程碑:M3
- 前置:夹具 `hosts/` 与 hft-core `c32`、`dev2`、`c4` 档;每行只做该行改动,运行 `cargo xtask gen-host --host <行内清单>`
- 实现:`tools/xtask/tests/deploy_gen.rs::dep_010_manifest_failures`

| 行 | 改动 | 期望 |
|---|---|---|
| DEP-010.1 | `tokyo-cex-1` 加顶层键 `cores = [5, 6]` | 退出 1;报未登记键 `cores` |
| DEP-010.2 | `tokyo-cex-1` 的 `[eni_b]` 加 `mac = "0a:1b:2c:3d:4e:5f"` | 退出 1;报未登记键 `eni_b.mac` |
| DEP-010.3 | `c32` 档 sim 的 `seq` 线程绑核 5(隔离集合内) | 退出 1 |
| DEP-010.4 | sim 的 `seq` 线程绑核 1(ENI-B IRQ 专用核) | 退出 1 |
| DEP-010.5 | sim 的 `seq` 线程绑核 12(prod 的 `gw` 热核) | 退出 1 |
| DEP-010.6 | sim 档加一个绑核 32 的线程(`c8g.8xlarge` 只有 0–31) | 退出 1 |
| DEP-010.7 | `c32` 档 sim 的内存档加 64 GiB(两实例合计超出本机内存档) | 退出 1 |
| DEP-010.8 | `[egress].sim` 的 IP 改为 `10.0.1.10`(与 `prod` 相同) | 退出 1 |
| DEP-010.9 | `[egress].prod` 项 `eni = "c"` | 退出 1 |
| DEP-010.10 | `[egress].sim` 项 `eni = "b"` | 退出 1 |
| DEP-010.11 | `[eni_b2].enabled = true`、`prod_bulk = [{eni = "b2", ip = "10.0.1.12", eip_alloc = "eipalloc-0b2"}]`,清单改为 `class = "c16"`、`instance_type = "c8g.4xlarge"`(`c16` 档不预留 `eni_b2` 核) | 退出 1;报 ENI-B2 启用而线程计划没有 `eni_b2` 核 |
| DEP-010.12 | `c32` 档(预留核 28)启用 ENI-B2,`prod_bulk` 项 `eni = "b"` | 退出 1 |
| DEP-010.13 | 同 DEP-010.12 的档,`prod_bulk` 的 IP 改为 `10.0.1.10`(与 `prod` 相交) | 退出 1 |
| DEP-010.14 | `tokyo-cex-1` 删去 `[eni_c]` | 退出 1 |
| DEP-010.15 | `tokyo-cex-1.standby` 的 `verify = false` | 退出 1 |
| DEP-010.16 | `tokyo-cex-1.standby` 的 `class = "c16"` | 退出 1 |
| DEP-010.17 | `tokyo-cex-1.standby` 的 `instances = ["prod"]` | 退出 1 |
| DEP-010.18 | `tokyo-cex-1.standby` 的 `formal_live = false` | 退出 1 |
| DEP-010.19 | `tokyo-cex-1.standby` 的 `host = "tokyo-cex-2"` | 退出 1 |
| DEP-010.20 | `tokyo-cex-1-live` 的 `ena_driver = "2.17.4"` | 退出 1 |
| DEP-010.21 | 未改动(`tokyo-cex-1-live` 只有 `verify` 不同) | 退出 0 |
| DEP-010.22 | 未改动(待命清单的 `az_id`、`placement_group` 与私有 IP 不同于主清单) | 退出 0 |
| DEP-010.23 | `dev2-1` 的 `[egress]` 项没有 `eni` 键 | 退出 0(dev2 例外) |
| DEP-010.24 | `dev2-1` 加 `[hugepages] derive = true` | 退出 1(dev2 不允许的键) |
| DEP-010.25 | `c4-1` 加 `[eni_c] device_index = 2` | 退出 1 |
| DEP-010.26 | `c4-1` 加 `[eni_b2] enabled = false` | 退出 1 |

### DEP-011 每个主机类生成的文件集合

- 规则:[主机清单](README.md#主机清单)
- 级别:table
- 里程碑:M3
- 前置:夹具 `hosts/` 与 hft-core 各档;运行 `cargo xtask gen-host`,取每份清单的生成物文件名集合
- 实现:`tools/xtask/tests/deploy_gen.rs::dep_011_outputs_by_class`

| 行 | 清单 | 期望 |
|---|---|---|
| DEP-011.1 | `tokyo-cex-1` | 有 GRUB 片段、`hft.slice`/`hft-core.slice`/`hft-sim.slice`/`hftbatch.slice`/`system.slice`/`user.slice` 的 cpuset 与 I/O 设置、`irq-plan.json`、`hft-core-prod.toml` 与 `hft-core-sim.toml` 的 `[host]`/`[[threads]]`/`src_ip` 段、`egress.toml`、watchtower 隔离核线程白名单、`tuning/tokyo-cex-1.toml`;没有任何 `dev2/` 覆盖文件与 `dev2-guard.toml` |
| DEP-011.2 | `dev2-1` | 没有 GRUB 片段、隔离 cpuset、`irq-plan.json`、`system.slice`/`user.slice` 覆盖与 `hft-sim.slice`;有各单元与 slice 的 `50-dev2.conf`、`dev2-guard.toml`、`egress.toml`、`tuning/dev2-1.toml` |
| DEP-011.3 | `c4-1` | 有 GRUB 片段、隔离 cpuset、`irq-plan.json`;没有 `hft-sim.slice`、`hft-core-sim.toml` 段与 ENI-C 项 |
| DEP-011.4 | `probe-c32` | 文件集合与 `tokyo-cex-1` 相同,但没有 `hft-verify.service`/`.timer`,也没有 `hft-cred-broker.service` 与 archiver、ops-bot、pipeline、treasury 的单元 |
| DEP-011.5 | `tokyo-cex-1-live` | 与 `tokyo-cex-1` 相同,但没有 `hft-verify.service`/`.timer` |

### DEP-012 线程计划改一个核,依赖它的生成物同时变化

- 规则:[主机清单](README.md#主机清单)、[实现说明](README.md#实现说明)
- 级别:unit
- 里程碑:M3
- 前置:夹具 `tokyo-cex-1` 的生成物已提交;`c32` 档中把 `feed-md-bg` 从核 11 移到核 18,冷核由 18 改为 11
- 步骤:
  1. 不重新生成,运行 `cargo xtask gen-host --check`
  2. 运行 `cargo xtask gen-host --host tokyo-cex-1`,比较新旧生成物
- 期望:
  - 步骤 1 退出 1
  - 步骤 2 后以下五处都变化:GRUB 片段的 `isolcpus` 由 `5-17` 变为 `5-10,12-18`;`hft-core.slice` 的 `AllowedCPUs`;`irq-plan.json` 的 ENI-B XPS 映射含核 18、不含核 11;`hft-core-prod.toml` 的 `host.hot_cpus`;watchtower 隔离核线程白名单
- 实现:`tools/xtask/tests/deploy_gen.rs::dep_012_core_change_propagates`

### DEP-013 `host_expect` 覆盖每份清单且只随该清单的期望剖面变化

- 规则:[主机清单](README.md#主机清单)、[HostProfile](README.md#hostprofile)
- 级别:unit
- 里程碑:M3
- 前置:夹具 `hosts/` 共 8 份清单(`tokyo-cex-1`、`tokyo-cex-1-live`、两份待命清单、`probe-c16`、`probe-c32`、`dev2-1`、`c4-1`);`SOURCE_DATE_EPOCH = 0`
- 步骤:
  1. 打包,读 `RELEASE.json.host_expect`
  2. 把 `tokyo-cex-1` 的 `[eni_c].queues` 改为 2,重新生成并打包
- 期望:
  - 步骤 1:键集合恰为这 8 份清单名;每个值 = 归档内 `deploy/tuning/<清单名>.toml` 的 SHA-256
  - 步骤 2:只有 `host_expect["tokyo-cex-1"]` 改变,其余 7 个值不变
- 实现:`tools/xtask/tests/deploy_gen.rs::dep_013_host_expect_coverage`

### DEP-014 安装只接受 `host-local.toml` 选定且在 `host_expect` 中的清单

- 规则:[主机清单](README.md#主机清单)、[发布归档与 releasectl](README.md#发布归档与-releasectl)
- 级别:table
- 里程碑:M3
- 前置:替身根;归档为夹具正式版 `v1.2.3`,其余校验都能通过
- 实现:`deploy/tests/test_release.py::dep_014_manifest_selection`

| 行 | `/etc/hft/host-local.toml` | 期望 |
|---|---|---|
| DEP-014.1 | `manifest = "tokyo-cex-1"` | `releasectl install` 退出 0 |
| DEP-014.2 | `manifest = "tokyo-cex-1-live"` | 退出 0 |
| DEP-014.3 | `manifest = "tokyo-cex-1.standby"` | 退出 0 |
| DEP-014.4 | `manifest = "nosuch"` | 退出 1;没有新的版本目录 |
| DEP-014.5 | 文件不存在 | 退出 1 |
| DEP-014.6 | `manifest = "tokyo-cex-1"`,归档中 `tuning/tokyo-cex-1.toml` 与 `RELEASE.json` 一起被改写(逐文件哈希自洽,`host_expect` 未改) | 退出 1 |

### DEP-015 M8 只改本机清单选择并重新激活 core 组,`hft-verify.*` 随之删除

- 规则:[主机清单](README.md#主机清单)、[验证包与 hft-verify](README.md#验证包与-hft-verify)
- 级别:proc
- 里程碑:M7
- 前置:容器;`host-local.toml` 为 `tokyo-cex-1`,`v1.2.3` 已安装并激活 `core`、`cold`、`sim` 三组;`hft-verify.timer` 已启用
- 步骤:
  1. 把 `host-local.toml` 改为 `manifest = "tokyo-cex-1-live"`,运行 `releasectl activate --version v1.2.3 --group core`
  2. 运行 `preflight.sh`
- 期望:
  - 步骤 1 后 `systemctl show -p LoadState hft-verify.service hft-verify.timer` 两行都是 `LoadState=not-found`;`/opt/hft/current-core` 仍指向 `releases/v1.2.3`;没有安装新的版本目录
  - 步骤 2 退出 0
- 实现:`deploy/tests/container/test_manifest_switch.py::dep_015_m8_switch_drops_verify`

### DEP-016 `verify` 标志决定 `hft-verify.*` 是否生成与回放的暂停名单

- 规则:[主机清单](README.md#主机清单)、[systemd 单元](README.md#systemd-单元)
- 级别:table
- 里程碑:M3
- 前置:夹具清单的生成物;读 `hft-replay-verify.service` 的 `ExecStartPre=` 行
- 实现:`tools/xtask/tests/deploy_gen.rs::dep_016_verify_flag`

| 行 | 清单(`verify`) | 期望 |
|---|---|---|
| DEP-016.1 | `tokyo-cex-1`(true) | 有 `hft-verify.service` 与 `hft-verify.timer`;暂停行为 `ExecStartPre=+-/usr/bin/systemctl stop hft-verify.service hft-journal-archiver.service hft-pipeline.service` |
| DEP-016.2 | `tokyo-cex-1-live`(false) | 没有 `hft-verify.*`;暂停行为 `ExecStartPre=+-/usr/bin/systemctl stop hft-journal-archiver.service hft-pipeline.service` |
| DEP-016.3 | `dev2-1`(true) | 同 DEP-016.1 |

### DEP-017 待命清单在第二 AZ 安装当前正式版

- 规则:[主机清单](README.md#主机清单)
- 级别:host
- 里程碑:M7
- 环境:第二 AZ 中按启动模板新建的升级主机实例,已挂预建的待命 ENI-B/ENI-C,EIP 已重新关联到待命网卡
- 授权:`[需授权]`(建实例、挂网卡、改 EIP 关联)
- 步骤:
  1. 写 `host-local.toml` 的 `manifest = "tokyo-cex-1.standby"`(M8 起为 `tokyo-cex-1-live.standby`)
  2. 按 [incident-response S](../docs/runbooks/incident-response.md#s-交易主机失联) 安装并激活当前正式版(不另发版)
  3. `hostprobe verify` 对 `tuning/<清单名>.toml`
- 通过标准:步骤 2 安装退出 0(`host_expect` 校验通过);步骤 3 退出 0;S3 对象键的主机段仍为 `tokyo-cex-1`
- 证据:`ops-log/<UTC 日期>-DEP-017.md`
- 实现:`deploy/tests/host/DEP-017.sh`

### DEP-018 `dev2` 与 `c4` 的生成物按各自主机类的布局

- 规则:[主机清单](README.md#主机清单)
- 级别:table
- 里程碑:M3
- 前置:夹具 `dev2-1`、`c4-1` 的生成物
- 实现:`tools/xtask/tests/deploy_gen.rs::dep_018_dev2_and_c4_layout`

| 行 | 对象 | 期望 |
|---|---|---|
| DEP-018.1 | `c4-1` 的 `irq-plan.json` | 全部 IRQ(含 ENI-B 的每个队列)映射到核 0;没有 `xps` 键 |
| DEP-018.2 | `c4-1` 的 `hft-core-prod.toml` `[host]` 段 | 没有 `irq_cpus` 键;`hot_cpus = "1-3"` |
| DEP-018.3 | `c4-1` 的 `system.slice`、`user.slice` 覆盖 | 都是 `AllowedCPUs=0` |
| DEP-018.4 | `dev2-1` 的 `egress.toml` | `prod = ["172.31.10.5"]`、`sim = ["172.31.10.6"]` |
| DEP-018.5 | `dev2-1` 的单元集合 | 有 `hft-egress-ip.service`;没有 `hft-host-tune`、`hft-eni-attach`、`hft-net-tune`、`hft-resctrl`、`hft-sim-alternate` 的单元 |
| DEP-018.6 | `tokyo-cex-1` 的 `system.slice`、`user.slice` 覆盖 | 都是 `AllowedCPUs=0,18,29` |

### DEP-019 启用 ENI-B2 只改清单与生成物

- 规则:[主机清单](README.md#主机清单)、[GRUB](README.md#grub)
- 级别:table
- 里程碑:M3
- 前置:`tokyo-cex-1` 改为 `[eni_b2].enabled = true`、`prod_bulk = [{eni = "b2", ip = "10.0.1.12", eip_alloc = "eipalloc-0b2"}]`;`c32` 档已预留 `eni_b2` 核 28(档不改动)
- 实现:`tools/xtask/tests/deploy_gen.rs::dep_019_eni_b2_enable`

| 行 | 生成物 | 期望 |
|---|---|---|
| DEP-019.1 | `irq-plan.json` | ENI-B2 全部队列 → 核 28 |
| DEP-019.2 | GRUB 片段 | `rcu_nocbs=1,5-17,28`、`irqaffinity=0,1,28`;`isolcpus` 仍为 `domain,managed_irq,5-17` |
| DEP-019.3 | `egress.toml` | 多一张 `prod_bulk = ["10.0.1.12"]`,与 `prod`、`sim` 不相交 |
| DEP-019.4 | `hftbatch.slice` | `AllowedCPUs` 不含 28 |
| DEP-019.5 | `hft-core-prod.toml` | `host.irq_cpus` 含 `eni_b2 = 28` |
| DEP-019.6 | 期望剖面 | 多出 ENI-B2 的设备序号 3、ENI-B2 队列 IRQ 在核 28 两项 `required` |
| DEP-019.7 | 改动集合 | `git diff --name-only` 只含 `deploy/hosts/tokyo-cex-1.toml` 与生成物,没有 `crates/`、`apps/`、`tools/`、`python/` 下的文件 |

### DEP-020 `c32` aarch64 的 GRUB 片段

- 规则:[GRUB](README.md#grub)
- 级别:table
- 里程碑:M3
- 前置:夹具 `tokyo-cex-1`、hft-core `c32` 档(大页行:1 GiB 页 `n1g`、2 MiB 页 `n2m` 取该档内存表的值);生成的 `GRUB_CMDLINE_LINUX_DEFAULT` 按空格拆成参数集合
- 实现:`tools/xtask/tests/deploy_gen.rs::dep_020_grub_c32`

| 行 | 参数 | 期望 |
|---|---|---|
| DEP-020.1 | `isolcpus` | `domain,managed_irq,5-17` |
| DEP-020.2 | `nohz_full` | `5` |
| DEP-020.3 | `rcu_nocbs` / `rcu_nocb_poll` | `1,5-17` / 存在 |
| DEP-020.4 | `irqaffinity` | `0,1` |
| DEP-020.5 | 大页 | `default_hugepagesz=1G hugepagesz=1G hugepages=<n1g> hugepagesz=2M hugepages=<n2m>`,两个页数等于 hft-core 内存表 |
| DEP-020.6 | THP | `transparent_hugepage=madvise` |
| DEP-020.7 | 基线项 | `audit=0`、`nmi_watchdog=0`、`nowatchdog`、`nosoftlockup`、`nohz=on`、`skew_tick=1` 都在 |
| DEP-020.8 | x86 专属项 | 没有 `mce=`、`acpi_irq_nobalance`、`clocksource=`、`tsc=`、`cpuidle.off`、`intel_idle.max_cstate`、`processor.max_cstate`、`idle=poll`、`intel_pstate`、`nosmt`、`mitigations=`、`pcie_aspm` |
| DEP-020.9 | sim 与 ENI-C 的核 | 22–27 不出现在 `isolcpus`、`nohz_full`、`rcu_nocbs` 中 |

### DEP-021 `c4` 的 GRUB 片段与 `dev2` 没有片段

- 规则:[GRUB](README.md#grub)
- 级别:table
- 里程碑:M3
- 前置:夹具 `c4-1`、`dev2-1` 与对应 hft-core 档
- 实现:`tools/xtask/tests/deploy_gen.rs::dep_021_grub_c4_dev2`

| 行 | 清单 / 参数 | 期望 |
|---|---|---|
| DEP-021.1 | `c4-1` / `isolcpus` | `domain,managed_irq,1-3` |
| DEP-021.2 | `c4-1` / `nohz_full` | `1` |
| DEP-021.3 | `c4-1` / `rcu_nocbs` | `1-3` |
| DEP-021.4 | `c4-1` / `irqaffinity` | `0` |
| DEP-021.5 | `dev2-1` | 不生成 GRUB 片段;生成物中没有任何 `isolcpus`、`hugepages=` 字样 |

### DEP-030 调优单元只装在 `c4`/`c8`/`c16`/`c32`/`metal`,排在应用单元之前

- 规则:[运行期调优](README.md#运行期调优)、[systemd 单元](README.md#systemd-单元)
- 级别:table
- 里程碑:M3
- 前置:夹具 `tokyo-cex-1`、`c4-1`、`dev2-1` 的生成物
- 实现:`tools/xtask/tests/deploy_units.rs::dep_030_tuning_units`

| 行 | 清单 / 单元 | 期望 |
|---|---|---|
| DEP-030.1 | `tokyo-cex-1` / `hft-host-tune.service` | `Type=oneshot`、`RemainAfterExit=yes`、`User=root`、`Slice=system.slice`、`After=network-pre.target`、`Before=hft-core@prod.service hft-core@sim.service` |
| DEP-030.2 | `tokyo-cex-1` / `hft-eni-attach.service`、`hft-net-tune.service` | 同上的 `Type`、`RemainAfterExit`、`User`、`Before` |
| DEP-030.3 | `c4-1` / 上述三个单元 | 都存在;`hft-resctrl.service` 不存在(`resctrl.enabled = false`) |
| DEP-030.4 | `dev2-1` | 上述单元都不存在;只有 `hft-egress-ip.service`(`Type=oneshot`、`RemainAfterExit=yes`、`User=root`,`ExecStart` 为 `/opt/hft/current-core/deploy/net/hft-egress-ip.sh`) |

### DEP-031 IRQ 落点按 `irq-plan.json` 写入,irqbalance 关闭

- 规则:[运行期调优](README.md#运行期调优)
- 级别:table
- 里程碑:M3
- 前置:替身根;`/proc/interrupts` 夹具含 ENI-A 2 个队列 IRQ(40、41)、ENI-B 8 个(50–57)、ENI-C 1 个(60)、另一个可迁移 IRQ 70、NVMe managed IRQ 80;`irq-plan.json` 为 `tokyo-cex-1` 的生成物;运行 `irq/hft-irq-pin.sh`
- 实现:`deploy/tests/test_tune.py::dep_031_irq_pin`

| 行 | 对象 | 期望 |
|---|---|---|
| DEP-031.1 | irqbalance | 替身记录 `systemctl disable --now irqbalance.service` 恰好一次 |
| DEP-031.2 | IRQ 40、41 | `smp_affinity_list` = `0` |
| DEP-031.3 | IRQ 50–57 | 每个都是 `1` |
| DEP-031.4 | IRQ 60 | `22` |
| DEP-031.5 | IRQ 70 | `0` |
| DEP-031.6 | IRQ 80(managed) | 不写其 `smp_affinity_list`(替身记录中没有对它的写) |
| DEP-031.7 | 同一夹具,`irq-plan.json` 取 `c4-1` 的生成物 | IRQ 40–57、70 都是 `0`;没有 ENI-C |

### DEP-032 workqueue cpumask 开机写一次、5 s 后重放一次

- 规则:[运行期调优](README.md#运行期调优)
- 级别:unit
- 里程碑:M3
- 前置:替身根含 `/sys/devices/virtual/workqueue/cpumask` 与两个 workqueue 的 `cpumask`(初值 `ffffffff`);hk 掩码 = `1`(核 0);替身时钟
- 步骤:
  1. 在 t = 0 运行 `irq/hft-workqueue.sh`,推进替身时钟到 t = 6 s
- 期望:
  - t = 0 与 t = 5 s 各写入一次,每次把全局与两个 workqueue 的 `cpumask` 写为 `1`;t = 5 s 之后没有写入
  - 结束时三个文件内容都是 `1`
- 实现:`deploy/tests/test_tune.py::dep_032_workqueue_replay`

### DEP-033 按设备序号找到网卡,为每个私有 IP 建源地址路由

- 规则:[运行期调优](README.md#运行期调优)、[出口 IP](README.md#出口-ip)
- 级别:table
- 里程碑:M3
- 前置:替身根;IMDS 替身的 `network/interfaces/macs/` 下 `0a:00:00:00:00:01`(`device-number = 1`,接口 `ens6`)与 `0a:00:00:00:00:02`(`device-number = 2`,接口 `ens7`);清单 `tokyo-cex-1`;运行 `net/hft-eni-attach.sh`
- 实现:`deploy/tests/test_tune.py::dep_033_eni_attach`

| 行 | 条件 | 期望 |
|---|---|---|
| DEP-033.1 | 两块网卡都已挂载 | 退出 0;`ip rule` 替身中 `from 10.0.1.10` 指向一张默认路由经 `ens6` 的表,`from 10.0.1.20` 指向一张默认路由经 `ens7` 的表;没有任何 EC2 写调用 |
| DEP-033.2 | IMDS 中没有 `device-number = 2` 的网卡 | 退出非 0;stderr 指出 `eni_c device_index 2` |
| DEP-033.3 | 再运行一次(DEP-033.1 之后) | 零写入(`ip rule`/`ip route` 替身没有新增调用) |
| DEP-033.4 | MAC 的去向 | MAC 只在运行期用来定位接口;`/etc/hft/` 下与归档内任何文件都不含 MAC |

### DEP-034 出口地址脚本只补缺失的地址并核对公网关联

- 规则:[出口 IP](README.md#出口-ip)、[运行期调优](README.md#运行期调优)
- 级别:table
- 里程碑:M3
- 前置:替身根;清单 `tokyo-cex-1`(DEP-033 的网卡映射);IMDS 替身 `local-ipv4s` 与 `ipv4-associations/` 按行;运行 `net/hft-egress-ip.sh`
- 实现:`deploy/tests/test_tune.py::dep_034_egress_ip`

| 行 | 条件 | 期望 |
|---|---|---|
| DEP-034.1 | `ens7` 上没有 `10.0.1.20`,IMDS 显示已分配且有公网关联 | 恰好一次 `ip addr add 10.0.1.20/<prefix> dev ens7`;退出 0 |
| DEP-034.2 | 两个地址都已配在网卡上 | 没有 `ip addr add`;退出 0 |
| DEP-034.3 | IMDS 中 `10.0.1.20` 没有分配给 `ens7` 的 MAC | 退出非 0;不执行 `ip addr add` |
| DEP-034.4 | `10.0.1.20` 已分配但 `ipv4-associations/` 中没有映射到它的公网地址 | 退出非 0 |
| DEP-034.5 | 清单 `dev2-1`,`ens5` 上只有 `172.31.10.5` | 只 `ip addr add 172.31.10.6/<prefix> dev ens5`;不调用 `ethtool`,不改 `ens5` 的其他设置 |

### DEP-035 调优单元失败时两个 hft-core 单元不启动,重跑不连带重启

- 规则:[运行期调优](README.md#运行期调优)、[systemd 单元](README.md#systemd-单元)
- 级别:proc
- 里程碑:M3
- 前置:容器;`c32` 布局的单元(调优脚本以替身代替);`hft-core@sim` 的替身二进制按 `Type=notify` 运行
- 步骤:
  1. 让 `hft-host-tune.service` 以退出 1 失败;`systemctl start hft-core@sim`
  2. 修好后 `systemctl start hft-host-tune hft-core@sim`;记录 `hft-core@sim` 的 `MainPID` 与 `NRestarts`
  3. `systemctl restart hft-host-tune`
  4. dev2 布局:让 `hft-egress-ip.service` 失败,`systemctl start hft-core@sim`
- 期望:
  - 步骤 1:`hft-core@sim` 在 `ExecStartPre=/usr/bin/systemctl is-active --quiet hft-host-tune.service` 失败,单元 `failed`,替身二进制从未运行
  - 步骤 3 后 `MainPID` 与 `NRestarts` 不变
  - 步骤 4:`hft-core@sim` 同样在 `ExecStartPre` 失败
- 实现:`deploy/tests/container/test_units.py::dep_035_tune_prereq`

### DEP-036 网卡队列、合并、卸载与环的设置

- 规则:[运行期调优](README.md#运行期调优)
- 级别:table
- 里程碑:M3
- 前置:替身根;`ethtool -l` 替身报告 `ens6` 预设上限 8、`ens7` 上限 2;清单 `tokyo-cex-1`;运行 `net/hft-net-tune.sh`
- 实现:`deploy/tests/test_tune.py::dep_036_net_tune`

| 行 | 网卡 | 期望的 `ethtool` 调用 |
|---|---|---|
| DEP-036.1 | `ens6`(ENI-B,`queues = "max"`) | `-L ens6 combined 8`;`-C ens6 adaptive-rx off rx-usecs 0 tx-usecs 0`;`-K ens6 gro off lro off tso off gso off sg off`;`-G ens6 rx 4096 tx 1024` |
| DEP-036.2 | `ens7`(ENI-C,`queues = 1`) | `-L ens7 combined 1`;合并、卸载与环同 DEP-036.1 |
| DEP-036.3 | ENI-A | 没有任何调用 |
| DEP-036.4 | 启用 ENI-B2(设备序号 3,`ens8`,上限 8) | `ens8` 同 DEP-036.1 的四条 |

### DEP-037 NAPI 参数在每块交易与 sim 网卡上为 0

- 规则:[运行期调优](README.md#运行期调优)
- 级别:table
- 里程碑:M3
- 前置:替身根;`/sys/class/net/{ens6,ens7}/napi_defer_hard_irqs` 初值 2、`gro_flush_timeout` 初值 20000;运行 `net/hft-net-tune.sh`
- 实现:`deploy/tests/test_tune.py::dep_037_napi_zero`

| 行 | 文件 | 期望 |
|---|---|---|
| DEP-037.1 | `ens6/napi_defer_hard_irqs` | `0` |
| DEP-037.2 | `ens6/gro_flush_timeout` | `0` |
| DEP-037.3 | `ens7` 的两项 | 都是 `0` |
| DEP-037.4 | 替身让 `ens6/gro_flush_timeout` 写后读回仍为 20000 | 脚本退出非 0,stderr 含该路径 |

### DEP-038 `90-hft.conf` 的取值与不设的项

- 规则:[运行期调优](README.md#运行期调优)
- 级别:table
- 里程碑:M3
- 前置:读 `deploy/sysctl/90-hft.conf`,按 `键 = 值` 解析
- 实现:`deploy/tests/test_tune.py::dep_038_sysctl_values`

| 行 | 键 | 期望 |
|---|---|---|
| DEP-038.1 | `net.core.busy_poll`、`net.core.busy_read` | 都是 `0` |
| DEP-038.2 | `net.ipv4.tcp_autocorking`、`net.ipv4.tcp_slow_start_after_idle`、`net.ipv4.tcp_no_metrics_save` | `0`、`0`、`1` |
| DEP-038.3 | `net.core.default_qdisc` | `noqueue` |
| DEP-038.4 | `net.core.rmem_max`、`net.core.wmem_max`、`net.core.optmem_max` | 都是 `16777216` |
| DEP-038.5 | `net.core.netdev_max_backlog`、`net.core.netdev_budget` | `1000000`、`600` |
| DEP-038.6 | `vm.swappiness`、`kernel.numa_balancing`、`vm.compaction_proactiveness` | 都是 `0` |
| DEP-038.7 | `vm.min_free_kbytes`、`vm.dirty_background_bytes`、`vm.dirty_bytes` | `1000000`、`33554432`、`268435456` |
| DEP-038.8 | `vm.stat_interval`、`kernel.timer_migration`、`kernel.watchdog` | `120`、`0`、`0` |
| DEP-038.9 | `kernel.printk` | `3 3 3 3` |
| DEP-038.10 | `net.ipv4.tcp_timestamps`、`net.ipv4.tcp_sack`、`net.ipv4.tcp_fastopen` | 三个键都不出现 |

### DEP-039 内存、KSM、电源、存储与时钟的静态配置

- 规则:[运行期调优](README.md#运行期调优)
- 级别:table
- 里程碑:M3
- 前置:读归档中的 `hft-host-tune` 调用的内存脚本、`udev/60-hft.rules`、`tuned/hft-latency/tuned.conf`、`chrony/chrony.conf`;替身根运行 `hft-host-tune.service` 的 `ExecStart`
- 实现:`deploy/tests/test_tune.py::dep_039_static_config`

| 行 | 对象 | 期望 |
|---|---|---|
| DEP-039.1 | swap 与 KSM | 替身记录 `swapoff -a`;`/sys/kernel/mm/ksm/run` 写为 `0` |
| DEP-039.2 | `60-hft.rules` 的 `cpu_dma_latency` 行 | `KERNEL=="cpu_dma_latency"`,`OWNER="root"`、`GROUP="hft"`、`MODE="0660"` |
| DEP-039.3 | `60-hft.rules` 的块设备行 | 对 `nvme*` 设 `queue/scheduler` = `none`、`queue/iostats` = `0`、`queue/add_random` = `0` |
| DEP-039.4 | `dev2-1` 的激活安装集合 | 不含 `60-hft.rules`、`90-hft.conf` 与 tuned 配置 |
| DEP-039.5 | `tuned.conf` | `[cpu]` 段 `governor=performance` |
| DEP-039.6 | `chrony.conf` | 含 `refclock PHC /dev/ptp_ena poll 0 delay 0.000010 prefer`;含一条 `server 169.254.169.123`,该行没有 `prefer` |
| DEP-039.7 | `c4-1` 的 `60-hft.rules` | 有块设备行,没有 `cpu_dma_latency` 行 |

### DEP-040 XPS 按发送热核分配,RPS 全关,qdisc 按网卡设置

- 规则:[运行期调优](README.md#运行期调优)
- 级别:table
- 里程碑:M3
- 前置:替身根;`ens6` 有 16 个 TX 队列、16 个 RX 队列;发送热核 = 5–17(13 个,队列数不少于热核数);`irq-plan.json` 为 `tokyo-cex-1` 的生成物(16 队列版本);运行 `net/hft-net-tune.sh`
- 实现:`deploy/tests/test_tune.py::dep_040_xps_rps_qdisc`

| 行 | 对象 | 期望 |
|---|---|---|
| DEP-040.1 | `ens6/queues/tx-*/xps_cpus` | 核 5–17 中每一核恰好出现在一个 TX 队列的掩码里;没有两个热核共用一个队列;写入值与 `irq-plan.json` 逐队列相同 |
| DEP-040.2 | 任一 TX 队列的掩码 | 不含核 0、1、18–31 |
| DEP-040.3 | `ens6`、`ens7` 的 `queues/rx-*/rps_cpus` | 全部为 `0` |
| DEP-040.4 | 根 qdisc | `ens6`、`ens7` 为 `mq`(替身记录 `tc qdisc replace dev <if> root mq`) |

### DEP-041 调优脚本幂等、`--dry-run` 零写入、读回不符非零退出

- 规则:[实现说明](README.md#实现说明)、[运行期调优](README.md#运行期调优)
- 级别:table
- 里程碑:M3
- 前置:替身根(初值与期望不同);每行对一个脚本依次:运行一次、再运行一次、在新替身根上以 `--dry-run` 运行、在"写后读回原值"的替身根上运行
- 实现:`deploy/tests/test_tune.py::dep_041_idempotent_dry_run`

| 行 | 脚本 | 期望 |
|---|---|---|
| DEP-041.1 | `irq/hft-irq-pin.sh` | 第二次运行零写入;`--dry-run` 零写入并在 stdout 逐项列出差异(当前值 → 目标值);读回不符时退出非 0 |
| DEP-041.2 | `irq/hft-workqueue.sh` | 同上 |
| DEP-041.3 | `net/hft-eni-attach.sh` | 同上 |
| DEP-041.4 | `net/hft-net-tune.sh` | 同上 |
| DEP-041.5 | `net/hft-egress-ip.sh` | 同上 |
| DEP-041.6 | `resctrl/hft-resctrl.sh` | 同上 |
| DEP-041.7 | `imds/hft-imds-lock.sh load` | 第二次运行不新增挂载或规则;`--dry-run` 零写入 |
| DEP-041.8 | `dev2/hft-tune-dev2.sh apply` | 第二次运行零写入;`--dry-run` 零写入 |

### DEP-042 Bash 脚本以 `set -euo pipefail` 开头并通过 shellcheck

- 规则:[实现说明](README.md#实现说明)
- 级别:unit
- 里程碑:M3
- 前置:`deploy/` 下全部 Bash 脚本(含 `bin/hft-unit-idle`、`bin/hft-maint`、`dev2/hft-window`、`release/*.sh`)
- 步骤:
  1. 去掉 CR 后对每个脚本运行 `shellcheck -S warning`
  2. 读每个脚本 shebang 之后的第一条非注释语句
- 期望:
  - 步骤 1 每个脚本退出 0
  - 步骤 2 每个脚本都是 `set -euo pipefail`
- 实现:`deploy/tests/test_layout.py::dep_042_shellcheck`

### DEP-043 不做的项在生成物与单元中都不存在

- 规则:[运行期调优](README.md#运行期调优)
- 级别:table
- 里程碑:M3
- 前置:夹具全部清单的生成物与单元文件
- 实现:`tools/xtask/tests/deploy_units.rs::dep_043_not_done_items`

| 行 | 检查 | 期望 |
|---|---|---|
| DEP-043.1 | 任一单元的 `CPUSchedulingPolicy=` | 不出现 |
| DEP-043.2 | 任一单元的 `Environment=`/`LD_PRELOAD` 含 `onload` | 不出现 |
| DEP-043.3 | `hft-net-tune.sh` 的调用 | 没有开启 ENA Express 的 `ethtool` 或 EC2 调用 |
| DEP-043.4 | 任一 sysctl 或网卡设置把 `busy_poll`、`busy_read` 或 `SO_BUSY_POLL` 设为非 0 | 不出现 |

### DEP-044 升级主机上收包 hard 项被改后 hostprobe 报出、hft-core 拒绝启动

- 规则:[运行期调优](README.md#运行期调优)、[HostProfile](README.md#hostprofile)
- 级别:host
- 里程碑:M7
- 环境:统一升级主机(`tokyo-cex-1`),M7 测量窗口之外,`hft-core@prod` 已停止
- 授权:`[需授权]`(在生产主机上临时改内核参数与 IRQ 亲和)
- 步骤:
  1. `sysctl -w net.core.busy_read=50`,运行 `hostprobe verify`,`systemctl start hft-core@prod`;恢复后重跑 `hft-host-tune`
  2. 把 ENI-B 一个队列的 IRQ 写到核 3,重复步骤 1 的验证与启动;恢复
  3. 以 `systemd-run --scope --slice=probetest.slice taskset -c 1 stress-ng --cpu 1` 在核 1 上运行一个进程(根级临时 slice,不受 `system.slice`/`user.slice` 的核限制),重复;结束进程
  4. 改写 ENI-B 一个 TX 队列的 `xps_cpus`,重复;恢复
- 通过标准:步骤 1–3 每次 `hostprobe verify` 退出 1 且 stderr 有对应键(`net.core.busy_read`、该 IRQ 的落点、IRQ 专用核占用),`hft-core@prod` 以 3 退出(`HostProfile` 或 `HostCheck`)且未重启;步骤 4 `hostprobe verify` 退出 1 并列出该队列的 XPS 键;每次恢复后 `hostprobe verify` 退出 0
- 证据:`ops-log/<UTC 日期>-DEP-044.md`
- 实现:`deploy/tests/host/DEP-044.sh`

### DEP-050 `egress.toml` 的生成与列表划分

- 规则:[出口 IP](README.md#出口-ip)
- 级别:table
- 里程碑:M3
- 前置:夹具清单的生成物 `/etc/hft/egress.toml`
- 实现:`tools/xtask/tests/deploy_gen.rs::dep_050_egress_toml`

| 行 | 清单 | 期望 |
|---|---|---|
| DEP-050.1 | `tokyo-cex-1` | 恰有 `prod = ["10.0.1.10"]`、`sim = ["10.0.1.20"]` 两个键 |
| DEP-050.2 | `tokyo-cex-1` 启用 ENI-B2(DEP-019 的改动) | 另有 `prod_bulk = ["10.0.1.12"]` |
| DEP-050.3 | `dev2-1` | `prod = ["172.31.10.5"]`、`sim = ["172.31.10.6"]` |
| DEP-050.4 | `c4-1` | `prod = ["10.0.1.40"]`;没有 `sim` 地址 |
| DEP-050.5 | 任一清单 | 三张列表两两不相交 |

### DEP-051 每把正式 key 的 IP 白名单只含 prod 出口 EIP

- 规则:[出口 IP](README.md#出口-ip)、[秘密与实例角色](README.md#秘密与实例角色)
- 级别:drill
- 里程碑:M8
- 环境:统一升级主机切 Live 之前;各所正式账户的 API 管理页
- 授权:`[需授权]`(登录交易所账户管理页,只读)
- 步骤:按 [launch D](../docs/runbooks/launch.md#d-正式实盘m8) 的 key 核对步骤,逐个账户、逐个 venue 记录交易、只读、撤单三类 key 的 IP 白名单与提现权限
- 通过标准:每把 key 的白名单 ⊆ `egress.toml` 的 `prod` 列表关联的 EIP;没有 `sim`、`prod_bulk` 的 EIP;每把 key 都没有提现权限
- 证据:`ops-log/<UTC 日期>-DEP-051.md`(逐 key 的截图或导出)
- 实现:`deploy/tests/host/DEP-051.sh`

### DEP-060 `hftbatch.slice` 在根级且 `cpu.idle = 1`

- 规则:[slice 与资源切分](README.md#slice-与资源切分)
- 级别:proc
- 里程碑:M3
- 前置:容器;安装 `tokyo-cex-1` 布局的 slice 文件;`systemd-run --slice=hftbatch.slice sleep 600` 与 `systemd-run --slice=hft-core.slice sleep 600` 各一个
- 步骤:
  1. 读 `systemd-cgls --no-pager` 与 `/sys/fs/cgroup/hftbatch.slice/cpu.idle`、`/sys/fs/cgroup/cgroup.subtree_control`
- 期望:
  - `hftbatch.slice` 是根 cgroup 的直接子节点;`hft-core.slice` 位于 `/sys/fs/cgroup/hft.slice/hft-core.slice`
  - `cpu.idle` 的内容为 `1`;根的 `cgroup.subtree_control` 含 `cpu`
  - `/sys/fs/cgroup/hftbatch.slice/cpu.max` 为 `max 100000`(没有 `CPUQuota`)
- 实现:`deploy/tests/container/test_slices.py::dep_060_root_batch_slice`

### DEP-061 争用时 `hftbatch.slice` 让出 CPU

- 规则:[slice 与资源切分](README.md#slice-与资源切分)
- 级别:proc
- 里程碑:M3
- 前置:容器;两个满载进程都绑在核 0:一个在 `system.slice`(`systemd-run --slice=system.slice taskset -c 0 stress-ng --cpu 1`),一个在 `hftbatch.slice`
- 步骤:
  1. 运行 30 s,读两个 slice 的 `cpu.stat` `usage_usec` 增量
- 期望:
  - `hftbatch.slice` 的增量 < 两者之和的 5%
- 实现:`deploy/tests/container/test_slices.py::dep_061_batch_yields_cpu`

### DEP-062 `hftbatch.slice` 内存压满时 OOM 只在该 slice 内且先杀 `hft-verify`

- 规则:[slice 与资源切分](README.md#slice-与资源切分)
- 级别:proc
- 里程碑:M3
- 前置:容器;`hftbatch.slice` `MemoryMax=256M`、`MemorySwapMax=0`;`hft-verify.service` 与 `hft-replay-verify.service` 的替身各分配内存,`OOMScoreAdjust` 分别为 +950、+900;`hft-core@sim` 替身在 `hft-core.slice` 运行(`OOMScoreAdjust=-800`)
- 步骤:
  1. 两个批处理替身各分配到 150 MiB 并持续增长
- 期望:
  - 内核 OOM 记录中第一个被杀的进程属于 `hft-verify.service`;`hft-core@sim` 的 `MainPID` 不变;`hft-core.slice` 的 `memory.events` `oom_kill` 为 0
- 实现:`deploy/tests/container/test_slices.py::dep_062_batch_oom_order`

### DEP-063 `hft.slice` 压满时冷进程先于 `hft-core@prod` 被杀

- 规则:[slice 与资源切分](README.md#slice-与资源切分)
- 级别:proc
- 里程碑:M3
- 前置:容器;`hft.slice` `MemoryMax=512M`;`hft-core.slice` 内运行 `hft-core@prod`(`OOMScoreAdjust=-900`)与 `hft-watchtower`(−300)替身
- 步骤:
  1. 两个替身同时增长内存直到触发 cgroup OOM
- 期望:
  - 第一个被杀的是 `hft-watchtower` 的进程;此时 `hft-core@prod` 的 `MainPID` 不变
- 实现:`deploy/tests/container/test_slices.py::dep_063_core_killed_last`

### DEP-064 `hft-sim.slice` 的进程不能把亲和设到隔离核

- 规则:[slice 与资源切分](README.md#slice-与资源切分)、[模拟实例(sim)](README.md#模拟实例sim)
- 级别:proc
- 里程碑:M7
- 前置:4 核容器;缩小的夹具:隔离核 1–2、`hft-sim.slice` `AllowedCPUs=3`
- 步骤:
  1. 在 `hft-sim.slice` 中运行的进程调用 `sched_setaffinity(0, {1})`
  2. 同一进程调用 `sched_setaffinity(0, {3})`
- 期望:
  - 步骤 1 返回 `EINVAL`,`/proc/self/status` 的 `Cpus_allowed_list` 仍为 `3`
  - 步骤 2 返回 0
- 实现:`deploy/tests/container/test_slices.py::dep_064_sim_affinity_bounded`

### DEP-065 冷副本根上的 I/O 带宽上限生效

- 规则:[slice 与资源切分](README.md#slice-与资源切分)
- 级别:proc
- 里程碑:M3
- 前置:特权容器;`<cold_root>` 挂在一个 loop 设备上;`hftbatch.slice` 设 `IOWriteBandwidthMax=<cold_root> 20M`、`IOReadBandwidthMax=<cold_root> 40M`
- 步骤:
  1. 在 `hftbatch.slice` 中以 `dd oflag=direct bs=1M count=200` 写 `<cold_root>/x`,再以 `iflag=direct` 读回
  2. 读该 slice 的 `io.max` 与 `io.stat`
- 期望:
  - `io.max` 中该设备行为 `rbps=40000000 wbps=20000000`(systemd 的带宽后缀按 1000 进位)
  - 写吞吐 ≤ 22,000,000 B/s,读吞吐 ≤ 44,000,000 B/s(上限 × 1.1)
- 实现:`deploy/tests/container/test_slices.py::dep_065_cold_io_limits`

### DEP-066 `system.slice`/`user.slice` 的核限制只装在 `c4`/`c8`/`c16`/`c32`/`metal`

- 规则:[slice 与资源切分](README.md#slice-与资源切分)
- 级别:table
- 里程碑:M3
- 前置:夹具 `tokyo-cex-1`、`c4-1`、`dev2-1` 的生成物
- 实现:`tools/xtask/tests/deploy_units.rs::dep_066_system_slice_cpus`

| 行 | 清单 | 期望 |
|---|---|---|
| DEP-066.1 | `tokyo-cex-1` | `system.slice.d/` 与 `user.slice.d/` 覆盖都是 `AllowedCPUs=0,18,29`(不含隔离核 5–17 与 IRQ 专用核 1、22) |
| DEP-066.2 | `c4-1` | 两者都是 `AllowedCPUs=0` |
| DEP-066.3 | `dev2-1` | 两个覆盖都不存在 |

### DEP-070 dev2 单元覆盖文件的内容

- 规则:[dev2 护栏](README.md#dev2-护栏)、[systemd 单元](README.md#systemd-单元)
- 级别:table
- 里程碑:M3
- 前置:夹具 `dev2-1` 与 hft-core `dev2` 档(冷进程 `Nice=10`、批处理 `Nice=19`;`OOMScoreAdjust` 与 `MemoryMax` 取该档,数值由 CORE-144、CORE-163 覆盖);读生成的 `dev2/<unit>.d/50-dev2.conf` 与合并后的单元(`systemd-analyze cat-config` 等价的合并结果)
- 实现:`tools/xtask/tests/deploy_units.rs::dep_070_dev2_overrides`

| 行 | 单元 | 期望 |
|---|---|---|
| DEP-070.1 | 基础单元带 `CPUAffinity=` 的每个单元 | 覆盖文件写 `CPUAffinity=0-1`(覆盖文件同时重写 `CPUAffinity`) |
| DEP-070.2 | `hft-core@.service` | `WatchdogSec=10`;`After=hft-dev2-guard.service` 与 `ExecStartPre=/usr/bin/systemctl is-active --quiet hft-dev2-guard.service`;`After=hft-egress-ip.service` 与 `ExecStartPre=/usr/bin/systemctl is-active --quiet hft-egress-ip.service`;`RequiresMountsFor=/dev/shm/hft/%i`;`ExecStopPost=+/opt/hft/current-core/deploy/bin/hft-shm-release %i` |
| DEP-070.3 | `hft-core@.service` 合并结果 | 没有 `hft-host-tune.service` 的 `After=` 与 `is-active` 检查;没有 `Requires=`、`Requisite=`、`BindsTo=` |
| DEP-070.4 | `hft-core@sim` | `Slice=hft-core.slice`;`systemctl is-enabled` 为 `enabled` |
| DEP-070.5 | `hft-core@prod` | `systemctl is-enabled` 为 `disabled`(不开机自启) |
| DEP-070.6 | `hft-verify` | `ExecCondition=` 依次为 `+/usr/bin/test ! -e /var/lib/hft/guard/tripped`、`/opt/hft/current-core/deploy/bin/hft-unit-idle hft-replay-verify.service`、`/usr/bin/test ! -e /run/hft/window/active`、`/opt/hft/current-core/deploy/bin/hft-unit-idle hft-core@prod.service`;没有测量窗口标志一行 |
| DEP-070.7 | `hft-journal-archiver`、`hft-pipeline` | 同 DEP-070.6 的四行,顺序相同 |
| DEP-070.8 | `hft-replay-verify`、`hft-treasury` | `+/usr/bin/test ! -e /var/lib/hft/guard/tripped`、`/usr/bin/test ! -e /run/hft/window/active`、`…/hft-unit-idle hft-core@prod.service` 三行,顺序相同 |
| DEP-070.9 | 冷进程(watchtower、reconciler、ops-bot)与批处理单元 | `Nice=10`(冷进程)、`Nice=19`(批处理) |
| DEP-070.10 | 全部 dev2 单元与覆盖文件 | 不出现 `Conflicts=`;安装脚本不调用 `systemctl mask` |
| DEP-070.11 | `hft.slice.d/50-dev2.conf`、`hftbatch.slice.d/50-dev2.conf` | 都有 `MemoryMax=`、`MemorySwapMax=0`;后者另有 `CPUWeight=idle`、两条冷盘带宽上限,没有 `CPUQuota=`(数值由 CORE-160、CORE-161 覆盖) |

### DEP-071 `hft-window start` 先写标志、再停批处理、最后启动测试网实例

- 规则:[dev2 护栏](README.md#dev2-护栏)
- 级别:unit
- 里程碑:M6
- 前置:替身根;替身时钟 `2026-10-04T08:00:00Z`;`SUDO_USER=alice`;`systemctl` 替身中 `hft-journal-archiver` 与 `hft-pipeline` 为 active,其余批处理 inactive;窗口标志不存在
- 步骤:
  1. 运行 `dev2/hft-window start`
- 期望:
  - `/run/hft/window/active` 存在,内容含 `2026-10-04T08:00:00Z` 与 `alice`;它的写入早于第一次 `systemctl` 调用
  - 两个定时器的 `stop` 早于任何批处理服务的 `stop`;当时 active 的 `hft-journal-archiver.service` 与 `hft-pipeline.service` 都被 `stop`;在第一条 `start` 之前五个批处理服务都是 inactive
  - 之后依次 `start hft-reconciler.service`、`start hft-core@prod.service`;没有其他 `start`
  - 退出 0
- 实现:`deploy/dev2/tests/test_window.py::dep_071_window_start_order`

### DEP-072 `hft-window` 幂等、`--dry-run` 零写入

- 规则:[dev2 护栏](README.md#dev2-护栏)、[实现说明](README.md#实现说明)
- 级别:table
- 里程碑:M6
- 前置:同 DEP-071;`SUDO_USER` 按行
- 实现:`deploy/dev2/tests/test_window.py::dep_072_window_idempotent`

| 行 | 操作 | 期望 |
|---|---|---|
| DEP-072.1 | DEP-071 之后再 `start`(`SUDO_USER=bob`) | 标志内容与 mtime 不变(仍为 `alice`);结束时单元状态与第一次之后相同;退出 0 |
| DEP-072.2 | 标志不存在时 `start --dry-run` | 不创建标志;没有改变状态的 `systemctl` 调用(只有查询);stdout 列出将执行的步骤 |
| DEP-072.3 | 窗口内 `stop --dry-run` | 不删除标志;没有改变状态的 `systemctl` 调用 |
| DEP-072.4 | 未设 `SUDO_USER`(root 直接执行)时 `start` | 标志内容含 `root` |

### DEP-073 `hft-window stop` 的成功路径

- 规则:[dev2 护栏](README.md#dev2-护栏)
- 级别:unit
- 里程碑:M6
- 前置:替身根;窗口标志存在;`systemctl stop hft-core@prod` 替身返回后 `/dev/shm/hft/prod/` 只剩 `heartbeat`,`statvfs` 已用量 4096 B;心跳块 `state = Stopped`
- 步骤:
  1. 运行 `dev2/hft-window stop`
- 期望:
  - 先 `stop hft-core@prod.service` 与 `stop hft-reconciler.service`;再删除 `/run/hft/window/active`;之后 `hft-journal-archiver.service`、`hft-pipeline.service`、`hft-treasury.service`、`hft-verify.timer`、`hft-replay-verify.timer` 都被 `start`(全部在标志删除之后)
  - 退出 0
- 实现:`deploy/dev2/tests/test_window.py::dep_073_window_stop_ok`

### DEP-074 热段残留或已用量不回落时 `hft-window stop` 保留标志并非零退出

- 规则:[dev2 护栏](README.md#dev2-护栏)
- 级别:table
- 里程碑:M6
- 前置:替身根;窗口标志存在;替身时钟,`stop hft-core@prod` 在 t = 0 返回;"已用量回落"的判据 = `statvfs` 已用量 ≤ 4096 B(心跳块),等待上限 30 s
- 实现:`deploy/dev2/tests/test_window.py::dep_074_window_stop_residue`

| 行 | 条件 | 期望 |
|---|---|---|
| DEP-074.1 | `/dev/shm/hft/prod/` 还有 `seq-000041.seg`(测试网实例被 `kill -9`) | 退出 1;标志仍在;没有 `start` 调用;stderr 含"先 `hft-window start` 让测试网实例恢复,再 `hft-window stop`" |
| DEP-074.2 | 只剩 `heartbeat`,已用量 64 MiB 持续到 t = 31 s;`/proc/4242/maps` 含 `/dev/shm/hft/prod/seq-000041.seg (deleted)` | 在 t = 30 s 判定失败:退出 1;标志仍在;没有 `start` 调用;stderr 列出 pid `4242` |
| DEP-074.3 | 已用量在 t = 29 s 回落到 4096 B | 退出 0;标志删除;批处理与定时器启动 |
| DEP-074.4 | 已用量在 t = 30 s 恰好回落 | 退出 0 |
| DEP-074.5 | 已用量 4097 B 持续到 t = 31 s | 退出 1 |

### DEP-075 dev2 重启后窗口结束、`sim` 排在守护之后启动

- 规则:[dev2 护栏](README.md#dev2-护栏)
- 级别:host
- 里程碑:M6
- 环境:dev2,联调窗口内(`hft-window start` 已执行),`hft-core@sim` 运行
- 授权:`[需授权]`(重启旧系统所在的生产主机)
- 步骤:
  1. 重启 dev2,开机后等 5 min
  2. 读 `/run/hft/window/active`、`systemctl show hft-core@sim -p NRestarts -p ActiveEnterTimestampMonotonic`、`systemctl show hft-dev2-guard -p ActiveEnterTimestampMonotonic`、`systemctl is-active hft-core@prod`
- 通过标准:标志不存在;`hft-core@sim` active、`NRestarts = 0`,其 `ActiveEnterTimestampMonotonic` 晚于守护的;`hft-core@prod` inactive;批处理定时器 active(`tripped` 不存在时)
- 证据:`ops-log/<UTC 日期>-DEP-075.md`
- 实现:`deploy/tests/host/DEP-075.sh`

### DEP-076 每实例 tmpfs 挂载单元的选项

- 规则:[dev2 护栏](README.md#dev2-护栏)、[systemd 单元](README.md#systemd-单元)
- 级别:table
- 里程碑:M3
- 前置:夹具 `dev2-1`、`tokyo-cex-1` 的生成物;大小取 hft-core 档(由 CORE-164 覆盖)
- 实现:`tools/xtask/tests/deploy_units.rs::dep_076_tmpfs_mounts`

| 行 | 清单 / 单元 | 期望 |
|---|---|---|
| DEP-076.1 | `dev2-1` / `dev-shm-hft-prod.mount` | `What=tmpfs`、`Where=/dev/shm/hft/prod`、`Type=tmpfs`;`Options=` 的选项集合 = {`size=<hft-core 值>`、`uid=hft`、`gid=hft-journal-ro`、`mode=2750`} |
| DEP-076.2 | `dev2-1` / `dev-shm-hft-sim.mount` | `Where=/dev/shm/hft/sim`;选项含 `uid=hft-sim`、`gid=hft-journal-ro`、`mode=2750` |
| DEP-076.3 | `tokyo-cex-1` | 只有 `dev-shm-hft-sim.mount`;`hft-core@sim` 有 `RequiresMountsFor=/dev/shm/hft/sim` |

### DEP-077 护栏守护单元的设置与安装范围

- 规则:[dev2 护栏](README.md#dev2-护栏)、[systemd 单元](README.md#systemd-单元)
- 级别:table
- 里程碑:M3
- 前置:夹具 `dev2-1`、`tokyo-cex-1` 的生成物
- 实现:`tools/xtask/tests/deploy_units.rs::dep_077_guard_unit`

| 行 | 项 | 期望 |
|---|---|---|
| DEP-077.1 | 用户与 slice | `User=hft-guard`、`Slice=system.slice`、没有 `SupplementaryGroups=` |
| DEP-077.2 | 重启 | `Type=simple`、`Restart=always` |
| DEP-077.3 | 目录 | `RuntimeDirectory=hft/guard`、`RuntimeDirectoryPreserve=yes`、`StateDirectory=hft/guard`、`StateDirectoryMode=0750` |
| DEP-077.4 | 秘密 | `ExecStartPre=+/opt/hft/current-core/bin/hft-secrets fetch --manifest /etc/hft/secrets/dev2-guard.toml --out /run/hft/guard/secrets --owner hft-guard --mode 0400` |
| DEP-077.5 | 解释器 | `ExecStart` 以 `/usr/bin/python3` 运行 `/opt/hft/current-core/deploy/dev2/hft-dev2-guard`,不经任何 venv |
| DEP-077.6 | `tokyo-cex-1` | 没有 `hft-dev2-guard.service` 与 `50-hft-dev2-guard.rules` |

### DEP-078 护栏配置文件由生成器写出,停机与只告警信号分开

- 规则:[dev2 护栏](README.md#dev2-护栏)
- 级别:table
- 里程碑:M3
- 前置:hft-core `dev2` 档夹具:采样周期 5 s、二级间隔 30 s、回落时长 30 min、因果判定 0.25 核 / 256 MiB、`MemAvailable` < 600 MiB 连续 3 次、`system.slice` `memory.pressure` > 5% 30 s、`cpu.pressure` > 20% 60 s、swap-in > 1,000 页/min 2 min、整机 PSI 内存 > 10% 30 s 与 CPU > 40% 60 s(P2)、swap 已用量 5 min 增长 > 64 MiB(P1)、旧系统健康(P1);专用主机列只有 `MemAvailable` < 300 MiB;生成 `/etc/hft/dev2-guard.toml`
- 实现:`tools/xtask/tests/deploy_gen.rs::dep_078_guard_config`

| 行 | 清单 / 项 | 期望 |
|---|---|---|
| DEP-078.1 | `dev2-1` / `mode` | `"shared"` |
| DEP-078.2 | `dev2-1` / 四个停机信号 | 各一项 `kind = "stop"`,阈值与持续时间同前置;没有 `level` 键 |
| DEP-078.3 | `dev2-1` / 整机 PSI 两项 | `kind = "alert"`、`level = "P2"` |
| DEP-078.4 | `dev2-1` / swap 已用量增长、旧系统健康 | `kind = "alert"`、`level = "P1"` |
| DEP-078.5 | `dev2-1` / 其余键 | 采样周期 5、二级间隔 30、回落时长 1800 s、因果阈值 0.25 核与 256 MiB、旧系统单元名与状态文件路径都存在 |
| DEP-078.6 | `dev2-2`(DEP-087 的夹具,退路 (b))| `mode = "dedicated"`;停机信号只有 `MemAvailable` < 300 MiB;没有 `system.slice` 压力、swap-in 与旧系统各项 |
| DEP-078.7 | `dev2-1` 的 watchtower 配置 | 护栏阈值文件键的值为 `/etc/hft/dev2-guard.toml` |
| DEP-078.8 | 把 hft-core `dev2` 档的 `MemAvailable` 阈值改为 500 MiB 后重新生成 | `dev2-guard.toml` 该项变为 500 MiB;`hft.slice.d/50-dev2.conf` 不变 |
| DEP-078.9 | 把 hft-core `dev2` 档的 `hft.slice` `MemoryMax` 改为 1400M 后重新生成 | `hft.slice.d/50-dev2.conf` 变为 `MemoryMax=1400M` |

### DEP-079 护栏的 polkit 授权只覆盖白名单单元的 stop 与 `hft-core@sim` 的 start

- 规则:[dev2 护栏](README.md#dev2-护栏)、[systemd 单元](README.md#systemd-单元)
- 级别:table
- 里程碑:M3
- 前置:`polkit/50-hft-dev2-guard.rules` 在规则求值器中执行(动作 `org.freedesktop.systemd1.manage-units`,`unit` 与 `verb` 按行;未匹配的请求按 polkit 默认处理 = 非交互拒绝)
- 实现:`deploy/tests/test_polkit.py::dep_079_guard_polkit`

| 行 | 用户 | 单元 | 动词 | 期望 |
|---|---|---|---|---|
| DEP-079.1 | `hft-guard` | `hft-core@prod.service` | `stop` | 允许 |
| DEP-079.2 | `hft-guard` | `hft-core@sim.service` | `stop` | 允许 |
| DEP-079.3 | `hft-guard` | `hft-reconciler.service` | `stop` | 允许 |
| DEP-079.4 | `hft-guard` | `hft-verify.service`、`hft-replay-verify.service`、`hft-journal-archiver.service`、`hft-pipeline.service`、`hft-treasury.service` | `stop` | 每个都允许 |
| DEP-079.5 | `hft-guard` | `hft-core@sim.service` | `start` | 允许 |
| DEP-079.6 | `hft-guard` | `hft-core@prod.service` | `start` | 拒绝 |
| DEP-079.7 | `hft-guard` | `hft-verify.service` | `start` | 拒绝 |
| DEP-079.8 | `hft-guard` | `hft-verify.timer` | `start` | 拒绝 |
| DEP-079.9 | `hft-guard` | `hft-core@sim.service` | `restart` | 拒绝 |
| DEP-079.10 | `hft-guard` | `hft-core@sim.service` | `kill` | 拒绝 |
| DEP-079.11 | `hft-guard` | `hft-watchtower.service`、`hft-ops-bot.service`、`hft-kill-switch.service`、`hft-offbox-watchdog.service` | `stop` | 每个都拒绝 |
| DEP-079.12 | `hft-guard` | `trading-collector.service`(旧系统) | `stop` | 拒绝 |
| DEP-079.13 | `hft-guard` | `ssh.service` | `stop` | 拒绝 |
| DEP-079.14 | `hft-watch` | `hft-core@sim.service` | `stop` | 拒绝 |

### DEP-080 守护脚本的单元名白名单与 polkit 规则一致

- 规则:[dev2 护栏](README.md#dev2-护栏)、[实现说明](README.md#实现说明)
- 级别:unit
- 里程碑:M3
- 前置:`dev2/hft-dev2-guard` 源码与 `polkit/50-hft-dev2-guard.rules`
- 步骤:
  1. 从脚本读出写死的停止名单与启动名单;从规则文件读出允许 `stop` 与 `start` 的单元集合
- 期望:
  - 两个停止集合相等,都是 {`hft-core@prod.service`、`hft-core@sim.service`、`hft-reconciler.service`、`hft-verify.service`、`hft-replay-verify.service`、`hft-journal-archiver.service`、`hft-pipeline.service`、`hft-treasury.service`}
  - 两个启动集合相等,都是 {`hft-core@sim.service`}
  - 脚本中出现的每个 `systemctl` 目标单元名都以 `hft-` 开头
- 实现:`deploy/dev2/tests/test_guard.py::dep_080_whitelist_matches_polkit`

### DEP-081 `hft-dev2-guard clear` 删除标志并以守护用户写事件,不留 root 属主的文件

- 规则:[dev2 护栏](README.md#dev2-护栏)
- 级别:proc
- 里程碑:M3
- 前置:容器;`/var/lib/hft/guard/` 由 `hft-dev2-guard.service` 建立(`hft-guard`,0750);`tripped`(`hft-guard`,0644)与 `events.jsonl`(2 行)存在;另一用例中 `events.jsonl` 不存在
- 步骤:
  1. 以 root 运行 `hft-dev2-guard clear`
  2. 在 `events.jsonl` 不存在的状态下再建 `tripped` 并以 root 运行 `clear`
- 期望:
  - 步骤 1:`tripped` 不存在;`events.jsonl` 为 3 行,末行动作为"清除标志";退出 0
  - 步骤 2:`events.jsonl` 新建,属主 `hft-guard`
  - 两步之后 `find /var/lib/hft/guard -uid 0` 输出为空
- 实现:`deploy/dev2/tests/container/test_guard_proc.py::dep_081_clear_no_root_files`

### DEP-082 `tripped` 标志的属主、模式与位置

- 规则:[dev2 护栏](README.md#dev2-护栏)
- 级别:proc
- 里程碑:M3
- 前置:容器;守护以单元运行;注入使一级动作成立的信号(判定由 CORE-186 覆盖)
- 步骤:
  1. 等一级动作完成,`stat /var/lib/hft/guard/tripped`
  2. `systemctl restart hft-dev2-guard`,再 `stat`
- 期望:
  - 步骤 1:属主 `hft-guard`,模式 0644,路径在 `StateDirectory`(`/var/lib/hft/guard/`)下
  - 步骤 2:文件仍在,inode 与内容不变
- 实现:`deploy/dev2/tests/container/test_guard_proc.py::dep_082_tripped_file`

### DEP-083 批处理单元必须以 root 判定 `tripped`

- 规则:[dev2 护栏](README.md#dev2-护栏)
- 级别:proc
- 里程碑:M3
- 前置:容器;`/var/lib/hft/guard/` 0750 `hft-guard`;`tripped` 存在;`hft-treasury.service` 按生成物安装;另装一个只用于本用例的对照单元 `hft-treasury-noplus.service`(同一单元,但第一条 `ExecCondition=` 去掉 `+`)
- 步骤:
  1. `systemctl start hft-treasury.service`
  2. `systemctl start hft-treasury-noplus.service`
- 期望:
  - 步骤 1:单元未进入 active,`journalctl -u hft-treasury` 有 `exec-condition` 跳过记录
  - 步骤 2:对照单元进入 active(以 `hft-treasury` 身份看不到标志)——证明 `+` 必需
- 实现:`deploy/dev2/tests/container/test_guard_proc.py::dep_083_tripped_needs_root`

### DEP-084 `hft-shm-release` 每次决定写一行事件,以守护用户追加

- 规则:[dev2 护栏](README.md#dev2-护栏)
- 级别:proc
- 里程碑:M3
- 前置:容器;`/var/lib/hft/guard/events.jsonl` 不存在;`/dev/shm/hft/sim/` 有心跳块与 2 个段文件;心跳 `state` 按步骤(删除判定本身由 CORE-195 覆盖)
- 步骤:
  1. `state = Stopped`,以 root 运行 `/opt/hft/current-core/deploy/bin/hft-shm-release sim`
  2. 换一组段文件,`state = Draining`,再运行一次
- 期望:
  - 每步后 `events.jsonl` 恰好多一行合法 JSON,含实例名 `sim` 与心跳 `state`(`Stopped` / `Draining`)以及是否删除
  - `events.jsonl` 属主为 `hft-guard`;`find /var/lib/hft/guard -uid 0` 为空
- 实现:`deploy/dev2/tests/container/test_guard_proc.py::dep_084_shm_release_event`

### DEP-085 二级动作从条件满足到第一个 `stop` 不超过一个采样周期

- 规则:[延迟与资源契约](README.md#延迟与资源契约)、[dev2 护栏](README.md#dev2-护栏)
- 级别:unit
- 里程碑:M3
- 前置:替身时钟与信号源;采样周期 5 s;一级动作在 t = 0 完成;`MemAvailable` 保持 500 MiB、`hft.slice` 用量 300 MiB(二级条件在 t = 30 s 的采样成立,判定由 CORE-187 覆盖)
- 步骤:
  1. 驱动采样到 t = 40 s
- 期望:
  - 第一条 `systemctl stop hft-core@prod.service` 在 t = 30 s 的采样处理内发出(替身时钟读数 < 35 s);守护对每个 `stop` 都等调用返回后才发下一条
  - 守护没有读取 `/dev/shm/hft/*/heartbeat`(替身的文件访问记录)
- 实现:`deploy/dev2/tests/test_guard.py::dep_085_level_two_latency`

### DEP-086 `hft-tune-dev2.sh` 先备份再改,`revert` 逐项还原,不随开机运行

- 规则:[dev2 护栏](README.md#dev2-护栏)
- 级别:table
- 里程碑:M3
- 前置:替身根;`ens5` 的 `rx-usecs` 初值 20、`adaptive-rx on`;替身时钟 `2026-10-04T08:00:00Z`
- 实现:`deploy/dev2/tests/test_tune_dev2.py::dep_086_tune_dev2_apply_revert`

| 行 | 操作 | 期望 |
|---|---|---|
| DEP-086.1 | `apply` | 第一次写入之前已存在 `/var/lib/hft/tune-backup/2026-10-04T08:00:00Z.json`,含每项原值(`rx-usecs = 20`、`adaptive-rx = on`) |
| DEP-086.2 | `apply` 后 `revert` | 每项恢复为备份中的值;退出 0 |
| DEP-086.3 | 备份文件写入失败 | `apply` 退出非 0 且没有修改任何项 |
| DEP-086.4 | 生成物 | 没有任何单元或定时器调用 `hft-tune-dev2.sh` |

### DEP-087 退路 (a)/(b) 的清单形态

- 规则:[dev2 护栏](README.md#dev2-护栏)
- 级别:table
- 里程碑:M3
- 前置:另一个夹具目录 `hosts-fallback/`(不与 `hosts/` 一起打包,DEP-013 的计数不含它)含 `batch-1`(`class = "dev2"`、`instances = []`、`formal_live = false`、`verify = true`、无 `[egress]`)、`dev2-2`(`class = "dev2"`、护栏 `dedicated`)与改为 `verify = false` 的 `dev2-1`(退路 (a) 的原主机)
- 实现:`tools/xtask/tests/deploy_gen.rs::dep_087_fallback_manifests`

| 行 | 清单 | 期望 |
|---|---|---|
| DEP-087.1 | `batch-1` 的单元集合 | 恰为 `hft-verify.service`/`.timer`、`hft-replay-verify.service`/`.timer`、`hft-replay-resume.service`、`hft-journal-archiver.service`、`hftbatch.slice`、`hft-imds-lock.service`、`hft-cred-broker.service`、`hft-hostprobe.service`;没有 `hft-core@`、tmpfs 挂载、护栏与看门狗单元 |
| DEP-087.2 | `batch-1` 的 archiver 配置 | 只有 `kind = "files"` 的 source |
| DEP-087.3 | `batch-1` 的期望剖面 | 只含 `hftbatch.slice` 与 IMDS 各项 |
| DEP-087.4 | `dev2-1`(`verify = false`) | 没有 `hft-verify.*`;护栏、两个实例与 archiver、pipeline、treasury 照常 |
| DEP-087.5 | `dev2-2` | 护栏单元与守护照装;`dev2-guard.toml` 为 `mode = "dedicated"` |

### DEP-090 hft-core 内存表加一个 `-bbo` 段时 2 MiB 页数随之增加

- 规则:[hugepages 与 hugetlbfs](README.md#hugepages-与-hugetlbfs)
- 级别:unit
- 里程碑:M3
- 前置:夹具 `tokyo-cex-1`、hft-core `c32` 档;新增段 `raw-feed-okx-swap-bbo`:`segment_bytes = 64 MiB`、`hot_keep = 2`,在 hft-core 内存表同时加对应的大页行
- 步骤:
  1. 记录生成的 2 MiB 页数 `n0`;加入新段后重新生成,记录 `n1`;运行 `cargo xtask gen-host --check`
- 期望:
  - `n1 − n0 = 128`(= (2 + 2) × 64 MiB ÷ 2 MiB)
  - `--check` 退出 0(推导与 hft-core 内存表对账一致);1 GiB 页数不变
- 实现:`tools/xtask/tests/deploy_gen.rs::dep_090_hugepages_follow_journals`

### DEP-091 hugetlbfs 挂载单元与挂载点权限

- 规则:[hugepages 与 hugetlbfs](README.md#hugepages-与-hugetlbfs)、[systemd 单元](README.md#systemd-单元)
- 级别:table
- 里程碑:M3
- 前置:夹具 `tokyo-cex-1`、`dev2-1` 的生成物
- 实现:`tools/xtask/tests/deploy_units.rs::dep_091_hugetlbfs_mounts`

| 行 | 对象 | 期望 |
|---|---|---|
| DEP-091.1 | `dev-hugepages2M.mount` | `What=hugetlbfs`、`Where=/dev/hugepages2M`、`Type=hugetlbfs`、选项含 `pagesize=2M`、`mode=0751` |
| DEP-091.2 | `dev-hugepages.mount` 覆盖 | 选项含 `pagesize=1G`、`mode=0751` |
| DEP-091.3 | `dev2-1` | 两个都不安装 |

### DEP-092 `hft-core@prod` 的 `ExecStartPre=+` 逐级建立热段与冷副本目录

- 规则:[hugepages 与 hugetlbfs](README.md#hugepages-与-hugetlbfs)、[身份、路径与 S3 前缀](README.md#身份路径与-s3-前缀)
- 级别:proc
- 里程碑:M3
- 前置:容器(`/dev/hugepages`、`/dev/hugepages2M` 以 `root:root` 0751 的 tmpfs 代替 hugetlbfs);`tokyo-cex-1` 布局;`<cold_root>` = `/data`(root 0755);`hft-core@prod` 替身二进制只做 `sd_notify(READY=1)`
- 步骤:
  1. 首次 `systemctl start hft-core@prod`
  2. 把 `/dev/hugepages/hft/prod/` 改成 0777,`systemctl restart hft-core@prod`
- 期望:
  - 步骤 1 后:`/dev/hugepages/hft/` 与 `/dev/hugepages2M/hft/` 为 `hft:hft-journal-ro` 0751;两者下的 `prod/` 为 `hft:hft-journal-ro` 2750;`/dev/shm/hft/prod/` 为 `hft:hft-journal-ro` 2750;`/data/prod/` 为 `hft:hft-journal-ro`,带 DEP-154 的 ACL
  - 步骤 2 后 `/dev/hugepages/hft/prod/` 恢复为 2750;其他目录的模式与 ACL 不变
- 实现:`deploy/tests/container/test_units.py::dep_092_prod_dirs`

### DEP-095 resctrl 关闭时不安装单元,期望剖面为 advisory

- 规则:[resctrl / CAT](README.md#resctrl--cat)
- 级别:table
- 里程碑:M3
- 前置:夹具 `tokyo-cex-1`,`[resctrl].enabled` 按行
- 实现:`tools/xtask/tests/deploy_gen.rs::dep_095_resctrl_flag`

| 行 | `enabled` | 期望 |
|---|---|---|
| DEP-095.1 | `false` | 没有 `hft-resctrl.service`;期望剖面中 resctrl 各项都标 `advisory` |
| DEP-095.2 | `true` | 有 `hft-resctrl.service`(`Type=oneshot`、`User=root`、`Before=hft-core@prod.service hft-core@sim.service`) |

### DEP-096 resctrl 的 `hot` 与 `rest` 组

- 规则:[resctrl / CAT](README.md#resctrl--cat)
- 级别:unit
- 里程碑:M7
- 前置:替身根含 `/sys/fs/resctrl/info/L3/cbm_mask` = `fff`、`num_closids` = 16;`tokyo-cex-1` 的 `resctrl.enabled = true`
- 步骤:
  1. 运行 `resctrl/hft-resctrl.sh`
- 期望:
  - 建出 `/sys/fs/resctrl/hot` 与 `/sys/fs/resctrl/rest`;`hot/cpus_list` = `5-17`;`rest/cpus_list` = 其余核(0–4、18–31)
  - 两组 `schemata` 的 L3 掩码按位与为 0,按位或为 `fff`
- 实现:`deploy/tests/test_tune.py::dep_096_resctrl_groups`

### DEP-120 每个单元套用加固集合,root 单元按例外处理

- 规则:[systemd 单元](README.md#systemd-单元)
- 级别:table
- 里程碑:M3
- 前置:夹具 `tokyo-cex-1` 与 `dev2-1` 的全部 `.service` 生成物(含 drop-in 合并后)
- 实现:`tools/xtask/tests/deploy_units.rs::dep_120_hardening`

| 行 | 单元 | 期望 |
|---|---|---|
| DEP-120.1 | 每个非 root 单元 | `NoNewPrivileges=yes`、`ProtectSystem=strict`、`ProtectHome=yes`、`PrivateTmp=yes`、`LockPersonality=yes`、`SystemCallArchitectures=native` 都在 |
| DEP-120.2 | 非 root 单元中除下面三行之外的每个 | `RestrictAddressFamilies=AF_UNIX AF_INET AF_INET6`、`CapabilityBoundingSet=`(空) |
| DEP-120.3 | `hft-core@prod`、`hft-core@sim` | `RestrictAddressFamilies=AF_UNIX AF_INET AF_INET6 AF_NETLINK` |
| DEP-120.4 | `hft-core@prod` | `AmbientCapabilities=CAP_NET_ADMIN CAP_IPC_LOCK`、`CapabilityBoundingSet=CAP_NET_ADMIN CAP_IPC_LOCK`;`hft-core@sim` 的两项为空 |
| DEP-120.5 | `hft-verify` | `RestrictAddressFamilies=AF_UNIX`、`PrivateNetwork=yes` |
| DEP-120.6 | 每个 `User=root` 的单元 | 没有 `ProtectSystem=strict`,没有空的 `CapabilityBoundingSet=`;DEP-120.1 中其余四项都在 |

### DEP-121 每个单元显式写 `User=` 与 `Slice=`

- 规则:[systemd 单元](README.md#systemd-单元)
- 级别:table
- 里程碑:M3
- 前置:夹具 `tokyo-cex-1`、`dev2-1` 的全部 `.service` 生成物;slice 成员与数值的归属见 CORE-160
- 实现:`tools/xtask/tests/deploy_units.rs::dep_121_explicit_user_slice`

| 行 | 单元 | 期望 `User=` / `Slice=` |
|---|---|---|
| DEP-121.1 | `hft-core@prod` | `hft` / `hft-core.slice` |
| DEP-121.2 | `hft-core@sim` | `hft-sim` / `hft-sim.slice`(`tokyo-cex-1`)、`hft-core.slice`(`dev2-1`) |
| DEP-121.3 | `hft-kill-switch` | `hft-ks` / `system.slice` |
| DEP-121.4 | `hft-watchtower`、`hft-reconciler`、`hft-ops-bot` | `hft-watch`、`hft-recon`、`hft-opsbot` / `hft-core.slice` |
| DEP-121.5 | `hft-journal-archiver`、`hft-replay-verify`、`hft-verify`、`hft-pipeline`、`hft-treasury` | `hft-archive`、`hft-replay`、`hft-verify`、`hft-pipeline`、`hft-treasury` / `hftbatch.slice` |
| DEP-121.6 | `hft-dev2-guard`、`hft-offbox-watchdog`(`dev2-1`) | `hft-guard`、`hft-offbox` / `system.slice` |
| DEP-121.7 | `hft-host-tune`、`hft-eni-attach`、`hft-net-tune`、`hft-imds-lock`、`hft-cred-broker`、`hft-hostprobe`、`hft-egress-ip`、`hft-replay-resume`、`hft-sim-alternate`(各自安装的清单) | `root` / `system.slice` |
| DEP-121.8 | 任一 `.service` | 都有 `User=` 与 `Slice=` 两行 |

### DEP-122 同一运行目录只由一个单元声明

- 规则:[systemd 单元](README.md#systemd-单元)
- 级别:unit
- 里程碑:M3
- 前置:夹具 `tokyo-cex-1`、`dev2-1` 的全部单元(模板按 `prod`、`sim` 展开)
- 步骤:
  1. 收集每个单元的 `RuntimeDirectory=` 值
- 期望:
  - 值集合恰为 {`hft/core-prod`、`hft/core-sim`、`hft/kill-switch`、`hft/creds`、`hft/watchtower`、`hft/reconciler`、`hft/ops_bot`、`hft/pipeline`}(`tokyo-cex-1`),`dev2-1` 另有 `hft/guard`、`hft/offbox`
  - 每个值恰好出现在一个单元里
- 实现:`tools/xtask/tests/deploy_units.rs::dep_122_runtime_dir_unique`

### DEP-123 不用 `Requires=`/`Requisite=`/`BindsTo=` 依赖会重启的单元

- 规则:[systemd 单元](README.md#systemd-单元)
- 级别:unit
- 里程碑:M3
- 前置:夹具全部清单的单元与覆盖文件(合并后)
- 步骤:
  1. 收集所有 `Requires=`、`Requisite=`、`BindsTo=`、`PartOf=` 的目标
- 期望:
  - 没有任何目标是 `hft-*.service`;`hft-kill-switch` 与两个 hft-core 实例之间没有任何这类依赖;两个 hft-core 实例之间也没有
  - `RequiresMountsFor=` 不计入(允许)
- 实现:`tools/xtask/tests/deploy_units.rs::dep_123_no_hard_deps`

### DEP-124 重启策略与配置错误退出码

- 规则:[systemd 单元](README.md#systemd-单元)
- 级别:table
- 里程碑:M3
- 前置:夹具 `tokyo-cex-1`、`dev2-1` 的单元
- 实现:`tools/xtask/tests/deploy_units.rs::dep_124_restart_policy`

| 行 | 单元 | 期望 |
|---|---|---|
| DEP-124.1 | `hft-core@` | `Restart=on-failure`、`RestartSec=1`、`StartLimitBurst=3`、`StartLimitIntervalSec=600`、`RestartPreventExitStatus=2 3 4`、`RestartForceExitStatus=75` |
| DEP-124.2 | `hft-kill-switch` | `Restart=always`、`RestartPreventExitStatus=2 3 4` |
| DEP-124.3 | `hft-watchtower`、`hft-reconciler`、`hft-ops-bot`、`hft-journal-archiver`、`hft-pipeline`、`hft-treasury` | `Restart=on-failure`、`RestartPreventExitStatus=2 3 4` |
| DEP-124.4 | `hft-cred-broker`、`hft-dev2-guard`、`hft-offbox-watchdog` | `Restart=always` |

### DEP-125 `hft-core@` 模板与两个实例的 drop-in

- 规则:[systemd 单元](README.md#systemd-单元)、[模拟实例(sim)](README.md#模拟实例sim)
- 级别:table
- 里程碑:M3
- 前置:夹具 `tokyo-cex-1`;`<cold_root>` = `/data`;合并后的 `hft-core@prod.service` 与 `hft-core@sim.service`
- 实现:`tools/xtask/tests/deploy_units.rs::dep_125_core_template`

| 行 | 项 | 期望 |
|---|---|---|
| DEP-125.1 | 两个实例共有 | `Type=notify`、`WatchdogSec=3`、`TimeoutStartSec=45`、`TimeoutStopSec=20`、`RuntimeDirectoryMode=0751`、`LimitMEMLOCK=infinity`、`LimitNOFILE=65536`、`After=hft-host-tune.service`、`ExecStartPre=/usr/bin/systemctl is-active --quiet hft-host-tune.service` |
| DEP-125.2 | `prod` 的 `ExecStart` / `RuntimeDirectory` | `/opt/hft/current-core/bin/hft-core --config /etc/hft/hft-core-prod.toml` / `hft/core-prod` |
| DEP-125.3 | `sim` 的 `ExecStart` / `RuntimeDirectory` | `/opt/hft/current-sim/bin/hft-core --config /etc/hft/hft-core-sim.toml` / `hft/core-sim` |
| DEP-125.4 | `prod` 的 `SupplementaryGroups` / `ReadWritePaths` | `hft-ctl hft-stopmark` / `/data/prod` 与 `-/var/lib/hft/kill-switch` |
| DEP-125.5 | `sim` 的 `SupplementaryGroups` / `ReadWritePaths` | `hft-ctl` / `/data/sim` |
| DEP-125.6 | `prod` 的秘密 | `ExecStartPre=+/opt/hft/current-core/bin/hft-secrets fetch --manifest /etc/hft/secrets/hft-core.toml --out /run/hft/core-prod/secrets --owner hft --mode 0400` |
| DEP-125.7 | `sim` | 合并结果中没有 `hft-secrets`、`/run/hft/core-prod`、`secrets_dir` 字样;有 `RequiresMountsFor=/dev/shm/hft/sim` |
| DEP-125.8 | `sim` 的 `CPUAffinity` | `23-27` |
| DEP-125.9 | `prod` 的 `ExecStartPre=+` 建目录 | 依次建立 `/dev/hugepages/hft/`、`/dev/hugepages/hft/prod/`、`/dev/hugepages2M/hft/`、`/dev/hugepages2M/hft/prod/`、`/dev/shm/hft/prod/`、`/data/prod/`(结果由 DEP-092 覆盖) |

### DEP-126 kill-switch 单元

- 规则:[systemd 单元](README.md#systemd-单元)
- 级别:table
- 里程碑:M5
- 前置:夹具 `tokyo-cex-1` 的 `hft-kill-switch.service`
- 实现:`tools/xtask/tests/deploy_units.rs::dep_126_kill_switch_unit`

| 行 | 项 | 期望 |
|---|---|---|
| DEP-126.1 | 类型与看门狗 | `Type=notify`、`WatchdogSec=5` |
| DEP-126.2 | 目录 | `RuntimeDirectory=hft/kill-switch`、`RuntimeDirectoryMode=0751`、`ReadWritePaths=/var/lib/hft/kill-switch`;`ExecStartPre=+` 以 `hft-ks:hft-stopmark` 2770 建立 `/var/lib/hft/kill-switch` |
| DEP-126.3 | 组 | `SupplementaryGroups=hft-journal-ro hft-ks-trigger hft-stopmark` |
| DEP-126.4 | 秘密 | `ExecStartPre=+/opt/hft/current-cold/bin/hft-secrets fetch --manifest /etc/hft/secrets/kill-switch.toml --out /run/hft/kill-switch/secrets --owner hft-ks --mode 0400` |
| DEP-126.5 | 依赖 | 没有指向 `hft-core@*` 的 `Requires=`、`BindsTo=`、`After=` |

### DEP-127 kill-switch 以配置错误退出时不重启

- 规则:[systemd 单元](README.md#systemd-单元)
- 级别:proc
- 里程碑:M5
- 前置:容器;`hft-kill-switch` 的替身二进制按步骤以给定退出码退出
- 步骤:
  1. 替身以退出码 2 退出
  2. 重置后替身以退出码 1 退出
- 期望:
  - 步骤 1 后单元 `failed`、`NRestarts = 0`
  - 步骤 2 后单元被重启(`NRestarts ≥ 1`)
- 实现:`deploy/tests/container/test_units.py::dep_127_kill_switch_exit2`

### DEP-128 kill-switch 的 polkit 授权只允许停 `hft-core@prod`

- 规则:[systemd 单元](README.md#systemd-单元)
- 级别:table
- 里程碑:M5
- 前置:`polkit/50-hft-kill-switch.rules` 在规则求值器中执行(动作 `org.freedesktop.systemd1.manage-units`)
- 实现:`deploy/tests/test_polkit.py::dep_128_kill_switch_polkit`

| 行 | 用户 | 单元 | 动词 | 期望 |
|---|---|---|---|---|
| DEP-128.1 | `hft-ks` | `hft-core@prod.service` | `stop` | 允许 |
| DEP-128.2 | `hft-ks` | `hft-core@sim.service` | `stop` | 拒绝 |
| DEP-128.3 | `hft-ks` | `hft-core@prod.service` | `start` | 拒绝 |
| DEP-128.4 | `hft-ks` | `hft-core@prod.service` | `restart` | 拒绝 |
| DEP-128.5 | `hft-ks` | `hft-core@prod.service` | `kill` | 拒绝 |
| DEP-128.6 | `hft-ks` | `hft-watchtower.service` | `stop` | 拒绝 |
| DEP-128.7 | `hft-opsbot` | `hft-core@prod.service` | `stop` | 拒绝 |
| DEP-128.8 | `hft` | `hft-core@prod.service` | `stop` | 拒绝 |

### DEP-129 kill-switch 规则装在每台有 `prod` 实例的主机

- 规则:[systemd 单元](README.md#systemd-单元)
- 级别:table
- 里程碑:M5
- 前置:夹具清单的激活安装集合
- 实现:`tools/xtask/tests/deploy_gen.rs::dep_129_kill_switch_rules_scope`

| 行 | 清单 | 期望 |
|---|---|---|
| DEP-129.1 | `tokyo-cex-1` | 安装 `50-hft-kill-switch.rules` |
| DEP-129.2 | `dev2-1` | 安装 `50-hft-kill-switch.rules` 与 `50-hft-dev2-guard.rules` |
| DEP-129.3 | `c4-1` | 安装 `50-hft-kill-switch.rules` |
| DEP-129.4 | `batch-1`(DEP-087,`instances = []`) | 两份规则都不安装 |

### DEP-130 实例用户能把 ctl socket 设为 `hft-ctl` 组,不在组内则不能

- 规则:[systemd 单元](README.md#systemd-单元)、[身份、路径与 S3 前缀](README.md#身份路径与-s3-前缀)
- 级别:proc
- 里程碑:M3
- 前置:容器;`hft-core@prod` 替身二进制在 `/run/hft/core-prod/ctl.sock` 建 Unix socket,`chown :hft-ctl`、`chmod 0660`;对照单元 `hft-core@prod` 的副本去掉 `SupplementaryGroups=`
- 步骤:
  1. 启动按生成物安装的 `hft-core@prod`
  2. 启动对照单元
- 期望:
  - 步骤 1:`stat` 为 `hft:hft-ctl` 0660;`/run/hft/core-prod/` 为 `hft` 0751
  - 步骤 2:替身的 `chown` 返回 `EPERM`,单元以替身的失败码退出
- 实现:`deploy/tests/container/test_units.py::dep_130_ctl_socket_group`

### DEP-131 kill-switch 触发套接字只有 `hft-ks-trigger` 组成员能连接

- 规则:[systemd 单元](README.md#systemd-单元)、[身份、路径与 S3 前缀](README.md#身份路径与-s3-前缀)
- 级别:table
- 里程碑:M5
- 前置:容器;kill-switch 替身建 `/run/hft/kill-switch/trigger.sock`(`hft-ks:hft-ks-trigger` 0660);每行以该用户身份 `connect()`;`SO_PEERCRED` 的检查由 kill-switch 自己的用例覆盖
- 实现:`deploy/tests/container/test_perm.py::dep_131_trigger_socket`

| 行 | 用户 | 期望 |
|---|---|---|
| DEP-131.1 | `hft-opsbot` | 连接成功 |
| DEP-131.2 | `hft` | `EACCES` |
| DEP-131.3 | `hft-pipeline` | `EACCES` |
| DEP-131.4 | `hft-watch` | `EACCES` |
| DEP-131.5 | root | 连接成功(文件权限不拦 root,由 kill-switch 按 uid 拒绝) |

### DEP-132 停机标记目录:setgid、无粘滞位,只有 `hft-ks` 与 `hft` 能读写

- 规则:[systemd 单元](README.md#systemd-单元)、[身份、路径与 S3 前缀](README.md#身份路径与-s3-前缀)
- 级别:table
- 里程碑:M5
- 前置:容器;`hft-kill-switch` 已启动一次(目录由其 `ExecStartPre=+` 建立);每行按用户身份执行
- 实现:`deploy/tests/container/test_perm.py::dep_132_stop_marker_dir`

| 行 | 用户 / 操作 | 期望 |
|---|---|---|
| DEP-132.1 | `stat /var/lib/hft/kill-switch` | `hft-ks:hft-stopmark`,模式 2770,没有粘滞位 |
| DEP-132.2 | `hft-ks` 写 `stop-marker.json`(umask 0007) | 成功;文件 `hft-ks:hft-stopmark` 0660 |
| DEP-132.3 | `hft` 读 `stop-marker.json` 并改名为 `stop-marker.json.consumed` | 成功 |
| DEP-132.4 | `hft-ks` 把另一份 `stop-marker.json` 改名为 `stop-marker.json.aborted` | 成功 |
| DEP-132.5 | `hft-sim` 列目录、读 `stop-marker.json` | 都 `EACCES` |
| DEP-132.6 | `hft-opsbot` 读 `stop-marker.json` | `EACCES` |

### DEP-133 每个单元自建自己的目录,不依赖其他单元先启动

- 规则:[systemd 单元](README.md#systemd-单元)
- 级别:table
- 里程碑:M5
- 前置:容器;每行在一个全新容器里只启动该行的单元(替身二进制),之前没有任何其他 hft 单元运行过
- 实现:`deploy/tests/container/test_units.py::dep_133_self_created_dirs`

| 行 | 单元 | 期望(启动成功,且下列路径的属主:组 / 模式) |
|---|---|---|
| DEP-133.1 | `hft-core@prod`(`tokyo-cex-1`) | `/run/hft/core-prod` `hft` 0751;`/dev/shm/hft/prod` `hft:hft-journal-ro` 2750;`/data/prod` `hft:hft-journal-ro`(带 ACL) |
| DEP-133.2 | `hft-core@sim`(`dev2-1`,先挂 tmpfs) | `/run/hft/core-sim` `hft-sim` 0751;`/dev/shm/hft/sim` `hft-sim:hft-journal-ro` 2750;`/data/sim` `hft-sim:hft-journal-ro` |
| DEP-133.3 | `hft-kill-switch` | `/run/hft/kill-switch` `hft-ks` 0751;`/var/lib/hft/kill-switch` `hft-ks:hft-stopmark` 2770 |
| DEP-133.4 | `hft-journal-archiver` | `/var/lib/hft/archiver` `hft-archive` 0751;`/var/lib/hft/archiver/advisor-daily` `hft-archive:hft-journal-ro` 2750 |
| DEP-133.5 | `hft-watchtower` | `/run/hft/watchtower` `hft-watch`;`/var/lib/hft/watchtower` `hft-watch` 0751;`acks.jsonl` `hft-watch:hft-ack` 0660 |
| DEP-133.6 | `hft-replay-verify` | `/var/lib/hft/replay-verify` `hft-replay:hft-replay` 0750 |
| DEP-133.7 | `hft-verify` | `/var/lib/hft/verify` `hft-verify:hft-verify` 0750 |
| DEP-133.8 | `hft-reconciler` | `/run/hft/reconciler` `hft-recon`;`/var/lib/hft/reconciler` `hft-recon:hft-recon` 0750 |
| DEP-133.9 | `hft-ops-bot` | `/run/hft/ops_bot` `hft-opsbot`;`/var/lib/hft/ops_bot` `hft-opsbot` 0750 |
| DEP-133.10 | `hft-pipeline` | `/run/hft/pipeline` `hft-pipeline` 0751;`/var/lib/hft/pipeline` `hft-pipeline` 0750 |
| DEP-133.11 | `hft-treasury` | `/var/lib/hft/treasury` `hft-treasury:hft-treasury` 0750 |
| DEP-133.12 | `hft-dev2-guard` | `/run/hft/guard` `hft-guard`;`/var/lib/hft/guard` `hft-guard:hft-guard` 0750 |
| DEP-133.13 | `hft-offbox-watchdog` | `/run/hft/offbox` `hft-offbox`;`/var/lib/hft/offbox` `hft-offbox` 0750 |
| DEP-133.14 | `hft-cred-broker` | `/run/hft/creds` root |

### DEP-134 全部单元通过 `systemd-analyze verify`

- 规则:[systemd 单元](README.md#systemd-单元)
- 级别:unit
- 里程碑:M3
- 前置:夹具 `tokyo-cex-1`、`dev2-1`、`c4-1` 的单元与覆盖文件装入临时 unit 目录;二进制路径以同名替身占位
- 步骤:
  1. 对每份清单运行 `systemd-analyze verify`,参数含 `hft-core@prod.service`、`hft-core@sim.service` 与其余全部单元
- 期望:
  - 每次退出 0,stderr 没有 `Unknown key` 或 `Failed to` 行
- 实现:`deploy/tests/test_units_static.py::dep_134_systemd_analyze`

### DEP-135 以服务用户身份运行每个单元的检查命令

- 规则:[systemd 单元](README.md#systemd-单元)
- 级别:table
- 里程碑:M5
- 前置:容器;夹具正式版已安装并激活,配置为夹具配置;每行以 `runuser -u <用户> --` 运行
- 实现:`deploy/tests/container/test_units.py::dep_135_check_as_service_user`

| 行 | 用户 / 命令 | 期望 |
|---|---|---|
| DEP-135.1 | `hft` / `/opt/hft/current-core/bin/hft-core --config /etc/hft/hft-core-prod.toml --check` | 退出 0 |
| DEP-135.2 | `hft-sim` / `/opt/hft/current-sim/bin/hft-core --config /etc/hft/hft-core-sim.toml --check` | 退出 0 |
| DEP-135.3 | `hft-ks` / `/opt/hft/current-cold/bin/kill-switch --check --config /etc/hft/kill-switch.toml` | 退出 0 |
| DEP-135.4 | `hft-watch` / `/opt/hft/current-cold/bin/watchtower --check --config /etc/hft/watchtower.toml` | 退出 0 |
| DEP-135.5 | `hft-recon` / `/opt/hft/current-cold/bin/reconciler --check --config /etc/hft/reconciler.toml` | 退出 0 |
| DEP-135.6 | `hft-archive` / `/opt/hft/current-cold/bin/journal-archiver --check --config /etc/hft/journal-archiver.toml` | 退出 0 |
| DEP-135.7 | `hft-opsbot`、`hft-pipeline`、`hft-treasury` / `/opt/hft/current-cold/venv/bin/python -c "import ops_bot"`(依次 `pipeline`、`treasury`) | 退出 0 |

### DEP-136 `hft-replay-verify` 单元与定时器

- 规则:[systemd 单元](README.md#systemd-单元)
- 级别:table
- 里程碑:M5
- 前置:夹具 `tokyo-cex-1` 的 `hft-replay-verify.service` 与 `.timer`
- 实现:`tools/xtask/tests/deploy_units.rs::dep_136_replay_verify_unit`

| 行 | 项 | 期望 |
|---|---|---|
| DEP-136.1 | 类型与定时 | `Type=oneshot`;定时器 `OnCalendar=*-*-* 01:30:00 UTC` |
| DEP-136.2 | 目录与组 | `StateDirectory=hft/replay-verify`、`StateDirectoryMode=0750`、`SupplementaryGroups=hft-journal-ro` |
| DEP-136.3 | 暂停 | `ExecStartPre=+-/usr/bin/systemctl stop …`(名单见 DEP-016) |
| DEP-136.4 | 条件 | `ExecCondition=+/usr/bin/test ! -e /var/lib/hft/guard/tripped` |
| DEP-136.5 | 恢复 | `OnSuccess=hft-replay-resume.service`、`OnFailure=hft-replay-resume.service` |
| DEP-136.6 | 程序 | `ExecStart` 以 `/opt/hft/current-cold/bin/replayer verify` 开头 |

### DEP-137 回放期间 archiver 与 pipeline 停止且不能启动,结束后恢复

- 规则:[systemd 单元](README.md#systemd-单元)
- 级别:table
- 里程碑:M5
- 前置:容器;`hft-journal-archiver`、`hft-pipeline` 替身 active;`hft-replay-verify` 替身运行 60 s 后按行结束;`tripped` 与窗口标志都不存在
- 实现:`deploy/tests/container/test_replay_pause.py::dep_137_pause_and_resume`

| 行 | 条件 | 期望 |
|---|---|---|
| DEP-137.1 | 回放运行中第 10 s 读状态 | archiver、pipeline 都 inactive |
| DEP-137.2 | 回放运行中 `systemctl start hft-journal-archiver hft-pipeline` | 两者都未进入 active,`journalctl` 有 `exec-condition` 跳过记录 |
| DEP-137.3 | 回放以 0 退出 | 10 s 内两者都 active;`hft-replay-resume` 运行过一次 |
| DEP-137.4 | 回放以 1 退出 | 同 DEP-137.3 |
| DEP-137.5 | 回放中途 `systemctl stop hft-replay-verify` | 同 DEP-137.3 |
| DEP-137.6 | `verify = false` 的布局(`hft-verify.service` 未安装) | 回放以 0 结束;单元结果 `success`;`ExecStartPre` 中对未加载单元的 `stop` 失败被忽略 |
| DEP-137.7 | 回放运行中 `systemctl start hft-verify` | 未进入 active(`exec-condition` 跳过) |
| DEP-137.8 | 回放运行中启动与停止 `hft-core@sim` | 两者都在 5 s 内完成,不等回放 |

### DEP-138 `hft-replay-resume` 只发起启动,标志存在时 archiver 与 pipeline 仍不启动

- 规则:[systemd 单元](README.md#systemd-单元)
- 级别:table
- 里程碑:M5
- 前置:容器;dev2 布局;archiver 与 pipeline inactive
- 实现:`deploy/tests/container/test_replay_pause.py::dep_138_resume_respects_flags`

| 行 | 条件 | 期望 |
|---|---|---|
| DEP-138.1 | 单元文件 | `Type=oneshot`、`User=root`、`ExecStart=/usr/bin/systemctl start --no-block hft-journal-archiver.service hft-pipeline.service` |
| DEP-138.2 | 无标志,`systemctl start hft-replay-resume` | 两者 active |
| DEP-138.3 | `tripped` 存在 | 两者仍 inactive,各有 `exec-condition` 跳过记录;`hft-replay-resume` 结果 `success` |
| DEP-138.4 | `/run/hft/window/active` 存在 | 同 DEP-138.3 |

### DEP-139 `hft-unit-idle` 按单元状态返回

- 规则:[实现说明](README.md#实现说明)、[systemd 单元](README.md#systemd-单元)
- 级别:table
- 里程碑:M3
- 前置:`systemctl is-active` 替身按行返回状态;运行 `bin/hft-unit-idle <单元…>`
- 实现:`deploy/tests/test_unit_idle.py::dep_139_unit_idle`

| 行 | 列出的单元与状态 | 期望退出码 |
|---|---|---|
| DEP-139.1 | `a` = `active` | 1 |
| DEP-139.2 | `a` = `activating` | 1 |
| DEP-139.3 | `a` = `deactivating` | 1 |
| DEP-139.4 | `a` = `inactive` | 0 |
| DEP-139.5 | `a` = `failed` | 0 |
| DEP-139.6 | `a` 未加载(`is-active` 输出 `inactive`,退出 4) | 0 |
| DEP-139.7 | `a` = `inactive`、`b` = `activating` | 1 |
| DEP-139.8 | 替身记录 | 只有 `systemctl is-active` 调用 |

### DEP-140 watchtower 单元

- 规则:[systemd 单元](README.md#systemd-单元)
- 级别:table
- 里程碑:M5
- 前置:夹具 `tokyo-cex-1` 的 `hft-watchtower.service`;`c32` 冷核 18、29
- 实现:`tools/xtask/tests/deploy_units.rs::dep_140_watchtower_unit`

| 行 | 项 | 期望 |
|---|---|---|
| DEP-140.1 | 组 | `SupplementaryGroups=hft-journal-ro hft-verify hft-replay hft-recon hft-guard hft-ack` |
| DEP-140.2 | 目录 | `RuntimeDirectory=hft/watchtower`;`StateDirectory=hft/watchtower`、`StateDirectoryMode=0751`;`ExecStartPre=+` 建立 `acks.jsonl`(`hft-watch:hft-ack` 0660) |
| DEP-140.3 | 秘密 | `ExecStartPre=+/opt/hft/current-cold/bin/hft-secrets fetch --manifest /etc/hft/secrets/watchtower.toml --out /run/hft/watchtower/secrets --owner hft-watch --mode 0400` |
| DEP-140.4 | 核 | `CPUAffinity=18,29` |
| DEP-140.5 | 重启 | `Restart=on-failure` |

### DEP-141 reconciler 单元与 dev2 上只在窗口运行

- 规则:[systemd 单元](README.md#systemd-单元)
- 级别:table
- 里程碑:M5
- 前置:夹具 `tokyo-cex-1`、`dev2-1` 的 `hft-reconciler.service`
- 实现:`tools/xtask/tests/deploy_units.rs::dep_141_reconciler_unit`

| 行 | 项 | 期望 |
|---|---|---|
| DEP-141.1 | 组与目录 | `SupplementaryGroups=hft-ctl hft-journal-ro`;`RuntimeDirectory=hft/reconciler`;`StateDirectory=hft/reconciler`、`StateDirectoryMode=0750` |
| DEP-141.2 | 秘密 | `--manifest /etc/hft/secrets/reconciler.toml --out /run/hft/reconciler/secrets --owner hft-recon` |
| DEP-141.3 | `tokyo-cex-1` | `systemctl is-enabled` 为 `enabled` |
| DEP-141.4 | `dev2-1` | `systemctl is-enabled` 为 `disabled`(由 `hft-window` 启停) |

### DEP-142 ops-bot 单元

- 规则:[systemd 单元](README.md#systemd-单元)、[IMDS 锁定与凭证代理](README.md#imds-锁定与凭证代理)
- 级别:table
- 里程碑:M5
- 前置:夹具 `tokyo-cex-1` 的 `hft-ops-bot.service`
- 实现:`tools/xtask/tests/deploy_units.rs::dep_142_ops_bot_unit`

| 行 | 项 | 期望 |
|---|---|---|
| DEP-142.1 | 组 | `SupplementaryGroups=hft-ctl hft-ks-trigger hft-pipeline-api hft-ack hft-journal-ro hft-treasury` |
| DEP-142.2 | 写路径与目录 | `ReadWritePaths=/var/lib/hft/watchtower/acks.jsonl`;`StateDirectory=hft/ops_bot`、`StateDirectoryMode=0750`;`RuntimeDirectory=hft/ops_bot` |
| DEP-142.3 | 解释器 | `ExecStart` 以 `/opt/hft/current-cold/venv/bin/python` 开头 |
| DEP-142.4 | 凭证 | `Environment=AWS_EC2_METADATA_DISABLED=true` 与 `AWS_SHARED_CREDENTIALS_FILE=/run/hft/creds/hft-opsbot/credentials` |
| DEP-142.5 | 秘密 | `--manifest /etc/hft/secrets/ops-bot.toml --out /run/hft/ops_bot/secrets --owner hft-opsbot` |

### DEP-143 pipeline 与 treasury 单元

- 规则:[systemd 单元](README.md#systemd-单元)
- 级别:table
- 里程碑:M5
- 前置:夹具 `tokyo-cex-1` 的 `hft-pipeline.service`、`hft-treasury.service`
- 实现:`tools/xtask/tests/deploy_units.rs::dep_143_pipeline_treasury_units`

| 行 | 单元 / 项 | 期望 |
|---|---|---|
| DEP-143.1 | pipeline / 组 | `SupplementaryGroups=hft-journal-ro hft-ctl hft-pipeline-api` |
| DEP-143.2 | pipeline / 目录 | `RuntimeDirectory=hft/pipeline`、`RuntimeDirectoryMode=0751`、`StateDirectory=hft/pipeline`、`StateDirectoryMode=0750` |
| DEP-143.3 | pipeline / 条件 | `ExecCondition=+/usr/bin/test ! -e /var/lib/hft/guard/tripped`、`ExecCondition=/opt/hft/current-core/deploy/bin/hft-unit-idle hft-replay-verify.service`,顺序如此 |
| DEP-143.4 | treasury / 组与目录 | `SupplementaryGroups=hft-journal-ro`;`StateDirectory=hft/treasury`、`StateDirectoryMode=0750` |
| DEP-143.5 | 两者 / 凭证 | 各自的 `AWS_SHARED_CREDENTIALS_FILE=/run/hft/creds/<用户>/credentials` 与 `AWS_EC2_METADATA_DISABLED=true` |

### DEP-144 journal-archiver 单元

- 规则:[systemd 单元](README.md#systemd-单元)
- 级别:table
- 里程碑:M5
- 前置:夹具 `tokyo-cex-1` 的 `hft-journal-archiver.service`;`<cold_root>` = `/data`
- 实现:`tools/xtask/tests/deploy_units.rs::dep_144_archiver_unit`

| 行 | 项 | 期望 |
|---|---|---|
| DEP-144.1 | 组与写路径 | `SupplementaryGroups=hft-journal-ro hft-verify`;`ReadWritePaths=/data` |
| DEP-144.2 | 目录 | `StateDirectory=hft/archiver`、`StateDirectoryMode=0751`;`ExecStartPre=+` 以 `hft-archive:hft-journal-ro` 2750 建 `/var/lib/hft/archiver/advisor-daily` |
| DEP-144.3 | 条件 | `ExecCondition=+/usr/bin/test ! -e /var/lib/hft/guard/tripped`、`ExecCondition=/opt/hft/current-core/deploy/bin/hft-unit-idle hft-replay-verify.service` |
| DEP-144.4 | 凭证 | `AWS_SHARED_CREDENTIALS_FILE=/run/hft/creds/hft-archive/credentials`、`AWS_EC2_METADATA_DISABLED=true` |

### DEP-145 凭证代理与 S3 消费者单元的安装范围

- 规则:[systemd 单元](README.md#systemd-单元)、[IMDS 锁定与凭证代理](README.md#imds-锁定与凭证代理)
- 级别:table
- 里程碑:M5
- 前置:夹具清单的生成物
- 实现:`tools/xtask/tests/deploy_gen.rs::dep_145_cred_broker_scope`

| 行 | 清单 | 期望 |
|---|---|---|
| DEP-145.1 | `tokyo-cex-1`(`formal_live = true`) | 有 `hft-cred-broker.service`(`RuntimeDirectory=hft/creds`、`RuntimeDirectoryPreserve=yes`、`Slice=system.slice`,秘密清单 `cred-broker.toml`)与 archiver、ops-bot、pipeline、treasury 单元 |
| DEP-145.2 | `dev2-1` | 同上 |
| DEP-145.3 | `probe-c32`(`formal_live = false`) | 五个单元都没有 |
| DEP-145.4 | `c4-1` | 五个单元都没有 |

### DEP-146 hostprobe 与 IMDS 锁定单元的次序

- 规则:[systemd 单元](README.md#systemd-单元)
- 级别:table
- 里程碑:M3
- 前置:夹具 `tokyo-cex-1` 的生成物
- 实现:`tools/xtask/tests/deploy_units.rs::dep_146_probe_and_imds_order`

| 行 | 单元 | 期望 |
|---|---|---|
| DEP-146.1 | `hft-hostprobe.service` | `Type=oneshot`、`User=root`;`ExecStart` 为 `/opt/hft/current-core/bin/hostprobe verify --expected /opt/hft/current-core/deploy/tuning/tokyo-cex-1.toml --out /run/hft/hostprofile.json`;`Before=hft-core@prod.service hft-core@sim.service`;开机启用 |
| DEP-146.2 | `hft-imds-lock.service` | `Type=oneshot`、`RemainAfterExit=yes`、`User=root`、`ExecStart=/opt/hft/current-core/deploy/imds/hft-imds-lock.sh load`;`Before=` 含 `hft-core@prod.service`、`hft-core@sim.service`、`hft-kill-switch.service`、`hft-watchtower.service`、`hft-reconciler.service`、`hft-ops-bot.service`、`hft-journal-archiver.service`、`hft-pipeline.service`、`hft-treasury.service`、`hft-replay-verify.service`、`hft-verify.service` |

### DEP-147 同机干扰测量单元与可选的链上单元默认不启用

- 规则:[systemd 单元](README.md#systemd-单元)
- 级别:table
- 里程碑:M7
- 前置:夹具 `tokyo-cex-1`、`probe-c16`、`dev2-1`、`c4-1` 的生成物与激活结果
- 实现:`tools/xtask/tests/deploy_gen.rs::dep_147_optional_units`

| 行 | 对象 | 期望 |
|---|---|---|
| DEP-147.1 | `tokyo-cex-1` 的 `hft-sim-alternate.timer` | 存在;`OnCalendar=*-*-* *:00:00`、`Persistent=true`;激活后 `is-enabled` 为 `disabled` |
| DEP-147.2 | `hft-sim-alternate.service` | `Type=oneshot`、`User=root`、`ExecStart=/opt/hft/current-core/deploy/bin/hft-sim-alternate --config /etc/hft/measure.toml` |
| DEP-147.3 | `probe-c16` | 有两个单元(`c16`) |
| DEP-147.4 | `dev2-1`、`c4-1` | 两个单元都没有 |
| DEP-147.5 | 任一清单 | 没有 `hft-chain-core@.service`、`hft-chain-grpc-relay@.service`、`hft-chain.target`、`bsc-node.service` |

### DEP-148 单元组的 target 成员

- 规则:[systemd 单元](README.md#systemd-单元)、[单元组与分组激活](README.md#单元组与分组激活)
- 级别:table
- 里程碑:M4
- 前置:夹具 `tokyo-cex-1` 的三个 target
- 实现:`tools/xtask/tests/deploy_units.rs::dep_148_group_targets`

| 行 | target | 期望的成员 |
|---|---|---|
| DEP-148.1 | `hft-core.target` | `hft-core@prod.service` |
| DEP-148.2 | `hft-sim.target` | 只有 `hft-core@sim.service` |
| DEP-148.3 | `hft-cold.target` | `hft-kill-switch`、`hft-watchtower`、`hft-reconciler`、`hft-journal-archiver`、`hft-replay-verify.timer`、`hft-ops-bot`、`hft-pipeline`、`hft-treasury` |
| DEP-148.4 | 三个 target | 都不含 `hft-dev2-guard`、`hft-offbox-watchdog`、`hft-cred-broker`、`hft-imds-lock`、`hft-hostprobe` 与调优单元 |

### DEP-149 watchtower 配置模板中由部署给出的项

- 规则:[主机清单](README.md#主机清单)、[机外看门狗](README.md#机外看门狗)
- 级别:table
- 里程碑:M5
- 前置:以夹具 `tokyo-cex-1` 与 `dev2-1` 渲染 `config-templates/` 中的 `watchtower.toml`;同机干扰参数的取值抄自 [testing §8.2](../docs/testing.md#82-延迟验收的测法):`block_hours = 2`、`min_n_each_side = 10000`、`max_ratio_ub = 1.05`、`max_halfwidth = 0.03`、`funding_hours_utc = [0, 8, 16]`
- 实现:`deploy/tests/test_config_templates.py::dep_149_watchtower_template`

| 行 | 清单 / 键 | 期望 |
|---|---|---|
| DEP-149.1 | 两份 / `interference` 表 | 上述五个键的值与前置相同 |
| DEP-149.2 | 两份 / `export.listen` | `127.0.0.1:9464` |
| DEP-149.3 | 两份 / 主机剖面项 | 没有写 IP 地址或网卡名的键;`paths.hostprofile` 为 `/run/hft/hostprofile.json`(监听地址与网卡由 watchtower 启动时从 HostProfile 读) |
| DEP-149.4 | `dev2-1` / `rules.HostPressure.enabled`、`rules.GuardAction.enabled` | 都是 `true`;护栏阈值文件键为 `/etc/hft/dev2-guard.toml` |
| DEP-149.5 | `tokyo-cex-1` / `rules.HostPressure.enabled`、`rules.GuardAction.enabled` | 都是 `false`;没有护栏阈值文件键 |
| DEP-149.6 | 两份 / `rules.ArchiverAlert.enabled`、`rules.PipelineAlert.enabled` | 都是 `true` |

### DEP-150 用户、主组与附加组按用户表创建,每个主机类都创建全部用户

- 规则:[身份、路径与 S3 前缀](README.md#身份路径与-s3-前缀)、[AMI 构建](README.md#ami-构建)
- 级别:table
- 里程碑:M3
- 前置:两个容器分别运行 AMI provision 的建用户步骤(升级主机)与 dev2 的安装脚本;读 `getent passwd`、`id <用户>`
- 实现:`deploy/tests/container/test_identity.py::dep_150_users_and_groups`

| 行 | 用户 | 期望(两个容器相同;`id -gn` / 附加组集合) |
|---|---|---|
| DEP-150.1 | `hft` | `hft` / {`hft-ctl`、`hft-stopmark`} |
| DEP-150.2 | `hft-sim` | `hft-sim` / {`hft-ctl`} |
| DEP-150.3 | `hft-ks` | `hft-ks` / {`hft-journal-ro`、`hft-ks-trigger`、`hft-stopmark`} |
| DEP-150.4 | `hft-archive` | `hft-archive` / {`hft-journal-ro`、`hft-verify`} |
| DEP-150.5 | `hft-replay` | `hft-replay` / {`hft-journal-ro`} |
| DEP-150.6 | `hft-verify` | `hft-verify` / {} |
| DEP-150.7 | `hft-watch` | `hft-watch` / {`hft-journal-ro`、`hft-verify`、`hft-replay`、`hft-recon`、`hft-guard`、`hft-ack`、`hft-archive`、`hft-pipeline`} |
| DEP-150.8 | `hft-recon` | `hft-recon` / {`hft-ctl`、`hft-journal-ro`} |
| DEP-150.9 | `hft-opsbot` | `hft-opsbot` / {`hft-ctl`、`hft-ks-trigger`、`hft-pipeline-api`、`hft-ack`、`hft-journal-ro`、`hft-treasury`} |
| DEP-150.10 | `hft-pipeline` | `hft-pipeline` / {`hft-ctl`、`hft-pipeline-api`、`hft-journal-ro`} |
| DEP-150.11 | `hft-treasury` | `hft-treasury` / {`hft-journal-ro`} |
| DEP-150.12 | `hft-guard`、`hft-offbox` | 两个容器里都存在;附加组为空 |
| DEP-150.13 | 运维登录用户(夹具 `ops`) | 附加组含 `hft-ctl` |
| DEP-150.14 | `hft-chain` | 两个容器里都不存在(链上组件未启用) |

### DEP-151 每个组的成员恰如用户表

- 规则:[身份、路径与 S3 前缀](README.md#身份路径与-s3-前缀)
- 级别:table
- 里程碑:M3
- 前置:同 DEP-150;`getent group <组>` 加上以该组为主组的用户
- 实现:`deploy/tests/container/test_identity.py::dep_151_group_members`

| 行 | 组 | 期望的成员集合 |
|---|---|---|
| DEP-151.1 | `hft` | {`hft`}(不含 `hft-sim`) |
| DEP-151.2 | `hft-journal-ro` | {`hft-ks`、`hft-archive`、`hft-replay`、`hft-watch`、`hft-recon`、`hft-opsbot`、`hft-pipeline`、`hft-treasury`} |
| DEP-151.3 | `hft-ctl` | {`hft`、`hft-sim`、`hft-recon`、`hft-opsbot`、`hft-pipeline`、`ops`} |
| DEP-151.4 | `hft-ks-trigger` | {`hft-ks`、`hft-opsbot`} |
| DEP-151.5 | `hft-stopmark` | {`hft-ks`、`hft`} |
| DEP-151.6 | `hft-verify` | {`hft-verify`、`hft-archive`、`hft-watch`} |
| DEP-151.7 | `hft-pipeline-api` | {`hft-pipeline`、`hft-opsbot`} |
| DEP-151.8 | `hft-ack` | {`hft-watch`、`hft-opsbot`} |
| DEP-151.9 | `hft-replay`、`hft-recon`、`hft-guard` | 各为 {同名用户、`hft-watch`} |
| DEP-151.10 | `hft-treasury` | {`hft-treasury`、`hft-opsbot`} |
| DEP-151.11 | `hft-archive` | {`hft-archive`、`hft-watch`} |
| DEP-151.12 | `hft-pipeline` | {`hft-pipeline`、`hft-watch`} |

### DEP-152 单元的 `SupplementaryGroups=` 与用户表逐项相同

- 规则:[身份、路径与 S3 前缀](README.md#身份路径与-s3-前缀)、[systemd 单元](README.md#systemd-单元)
- 级别:unit
- 里程碑:M3
- 前置:夹具 `tokyo-cex-1`、`dev2-1` 的全部单元(合并 drop-in 后);用户表的机读版本 `deploy/gen/identity.toml`
- 步骤:
  1. 对每个单元比较 `SupplementaryGroups=` 的集合与其 `User=` 在用户表中的附加组集合
- 期望:
  - 每个单元两集合相等(没有 `SupplementaryGroups=` 的单元对应空集);`hft-core@sim` 为 {`hft-ctl`},`hft-core@prod` 为 {`hft-ctl`、`hft-stopmark`}
- 实现:`tools/xtask/tests/deploy_perm.rs::dep_152_groups_match_table`

### DEP-153 路径表中每条路径的属主、组与模式

- 规则:[身份、路径与 S3 前缀](README.md#身份路径与-s3-前缀)
- 级别:table
- 里程碑:M5
- 前置:容器;`tokyo-cex-1` 布局安装、激活三组并把每个单元启动一次(替身二进制);`<cold_root>` = `/data`;`v1.2.3` 为当前版本,`v1.2.2` 为回滚版本;另一个 dev2 布局容器用于 DEP-153.13
- 实现:`deploy/tests/container/test_identity.py::dep_153_path_table`

| 行 | 路径 | 期望(属主:组 / 模式) |
|---|---|---|
| DEP-153.1 | `/opt/hft/releases/v1.2.3/`、其中的 `venv/` | `root:root` / 0755;版本目录内没有组或其他用户可写的文件 |
| DEP-153.2 | `/opt/hft/current-core`、`current-cold`、`current-sim`、`previous-core` | 符号链接,分别指向 `/opt/hft/releases/v1.2.3`(三个 current)与 `v1.2.2` |
| DEP-153.3 | `/opt/hft/installations/`、`/opt/hft/deployments/`、`/opt/hft/staging/` | `root:root` / 0755 |
| DEP-153.4 | `/etc/hft/hft-core-prod.toml`、`/etc/hft/egress.toml`、`/etc/hft/secrets/hft-core.toml` | `root:root` / 0644 |
| DEP-153.5 | `/etc/hft/host-local.toml` / `/etc/hft/github-release-token` | `root` 0644 / `root` 0600 |
| DEP-153.6 | `/run/hft/` | `root:root` / 0755 |
| DEP-153.7 | `/run/hft/core-prod/` | `hft` / 0751 |
| DEP-153.8 | `/run/hft/core-prod/ctl.sock` | `hft:hft-ctl` / 0660 |
| DEP-153.9 | `/run/hft/core-prod/secrets/` | `hft` / 0700 |
| DEP-153.10 | `/run/hft/kill-switch/trigger.sock`、`/run/hft/pipeline/api.sock` | `hft-ks:hft-ks-trigger` 0660、`hft-pipeline:hft-pipeline-api` 0660 |
| DEP-153.11 | `/run/hft/creds/hft-archive/` 与其 `credentials` | `hft-archive` 0500 / `hft-archive` 0400 |
| DEP-153.12 | `/run/hft/hostprofile.json` | `root` 0644 |
| DEP-153.13 | `/dev/shm/hft/`、`/dev/hugepages`、`/dev/hugepages2M` | `root:root` 0755、0751、0751 |
| DEP-153.14 | `/dev/cpu_dma_latency` | `root:hft` 0660 |
| DEP-153.15 | `/var/lib/hft/` | `root:root` / 0755 |
| DEP-153.16 | `/var/lib/hft/treasury/` | `hft-treasury:hft-treasury` / 0750 |
| DEP-153.17 | `/var/lib/hft/watchtower/acks.jsonl` | `hft-watch:hft-ack` / 0660 |
| DEP-153.18 | dev2 容器:`/dev/shm/hft/prod/`、`/dev/shm/hft/sim/`(挂载后) | `hft:hft-journal-ro` 2750、`hft-sim:hft-journal-ro` 2750 |
| DEP-153.19 | `/usr/local/sbin/releasectl`、`/usr/local/bin/hostprobe` | 符号链接,指向 `/opt/hft/current-core/deploy/release/releasectl.sh`、`/opt/hft/current-core/bin/hostprobe` |

### DEP-154 冷副本目录的 ACL 让 `hft-archive` 可写,组只读

- 规则:[身份、路径与 S3 前缀](README.md#身份路径与-s3-前缀)
- 级别:proc
- 里程碑:M5
- 前置:容器;`/data` 为支持 POSIX ACL 的 ext4 loop 卷;`hft-core@prod` 的 `ExecStartPre=+` 已建 `/data/prod/`
- 步骤:
  1. `getfacl -p /data/prod`
  2. 以 `hft` 身份 `mkdir -m 0770 /data/prod/2026-10-04`,在其中写段文件 `seq-000001.seg`(模式 0640),再 `getfacl` 新目录
  3. 以 `hft-archive` 身份在新目录中写 `seq-000001.seg.uploaded`,再删除 `seq-000001.seg`
  4. 以 `hft-replay` 身份在新目录中写文件、删除文件
  5. 以 `hft` 身份 `mkdir -m 0750 /data/prod/bad`,以 `hft-archive` 在其中写文件
- 期望:
  - 步骤 1:访问 ACL 为 `user::rwx`、`user:hft-archive:rwx`、`group::r-x`、`mask::rwx`、`other::---`;默认 ACL 为同样五条;`stat -c %a` 显示 `2770`(组位为 mask)
  - 步骤 2:新目录属组 `hft-journal-ro`、带 setgid,默认 ACL 继承,`mask::rwx`
  - 步骤 3 两个操作都成功;`.uploaded` 的属组为 `hft-journal-ro`
  - 步骤 4 两个操作都 `EACCES`
  - 步骤 5 `EACCES`(mask 被 0750 收窄为 `r-x`)——对照说明写者必须用 0770
- 实现:`deploy/tests/container/test_perm.py::dep_154_cold_acl`

### DEP-155 权限闭包:每个单元用户对它要用的每条路径有所需权限

- 规则:[systemd 单元](README.md#systemd-单元)、[身份、路径与 S3 前缀](README.md#身份路径与-s3-前缀)
- 级别:table
- 里程碑:M5
- 前置:由用户表、路径表与生成的单元(`SupplementaryGroups=`、`ReadWritePaths=`、`RuntimeDirectory=`、`StateDirectory=`)推出的权限模型;`<cold_root>` = `/data`;"穿越"= 沿途每级目录有 `x`;"读""写"另含终点的 `r`/`w`(建或删文件 = 父目录 `wx`);"写"另要求不被 `ProtectSystem=strict` 挡住(路径在 `/dev` 下,或在本单元的运行/状态目录或 `ReadWritePaths=` 内)
- 实现:`tools/xtask/tests/deploy_perm.rs::dep_155_closure_model`

| 行 | 用户(单元) | 路径 | 操作 | 期望 |
|---|---|---|---|---|
| DEP-155.1 | `hft`(`hft-core@prod`) | `/run/hft/core-prod/` | 建 `ctl.sock` 并设属组 `hft-ctl` | 允许 |
| DEP-155.2 | `hft` | `/run/hft/core-prod/secrets/<文件>` | 读并 `unlink` | 允许 |
| DEP-155.3 | `hft` | `/etc/hft/hft-core-prod.toml`、`/etc/hft/egress.toml`、`/run/hft/hostprofile.json`、`/opt/hft/current-core/RELEASE.json` | 读 | 允许 |
| DEP-155.4 | `hft` | `/dev/hugepages/hft/prod/`、`/dev/hugepages2M/hft/prod/` | 建段文件 | 允许(穿越 0751 的挂载点与中间层) |
| DEP-155.5 | `hft` | `/dev/shm/hft/prod/heartbeat` | 建文件,属组为 `hft-journal-ro` | 允许(setgid 目录继承属组) |
| DEP-155.6 | `hft` | `/data/prod/<子目录>` | 以 0770 建子目录与段文件 | 允许 |
| DEP-155.7 | `hft` | `/var/lib/hft/kill-switch/stop-marker.json` | 读并改名为 `.consumed` | 允许 |
| DEP-155.8 | `hft` | `/dev/cpu_dma_latency` | 以写打开 | 允许 |
| DEP-155.9 | `hft-sim`(`hft-core@sim`) | `/run/hft/core-sim/`、`/dev/shm/hft/sim/`、`/data/sim/` | 建文件 | 允许 |
| DEP-155.10 | `hft-sim` | `/run/hft/core-prod/secrets/<文件>` | 读 | 拒绝 |
| DEP-155.11 | `hft-sim` | `/run/hft/core-prod/` | 列目录 | 拒绝 |
| DEP-155.12 | `hft-sim` | `/var/lib/hft/kill-switch/` | 列目录、读标记 | 拒绝 |
| DEP-155.13 | `hft-sim` | `/dev/hugepages/hft/prod/<段>` | 读 | 拒绝 |
| DEP-155.14 | `hft-sim` | `/dev/cpu_dma_latency` | 以写打开 | 拒绝 |
| DEP-155.15 | `hft-ks`(`hft-kill-switch`) | `/run/hft/kill-switch/` | 建 `trigger.sock` 并设属组 `hft-ks-trigger` | 允许 |
| DEP-155.16 | `hft-ks` | `/var/lib/hft/kill-switch/` | 写 `stop-marker.json`(属组 `hft-stopmark`)、改名为 `.aborted`、追加 `actions.jsonl` | 允许 |
| DEP-155.17 | `hft-ks` | `/dev/shm/hft/prod/heartbeat`、`/dev/shm/hft/sim/heartbeat` | 读 | 允许 |
| DEP-155.18 | `hft-ks` | `/dev/hugepages/hft/prod/<seq 热段>` | 只读映射 | 允许 |
| DEP-155.19 | `hft-ks` | 同上 | 写 | 拒绝 |
| DEP-155.20 | `hft-ks` | `/run/hft/core-prod/ctl.sock` | 连接 | 拒绝(不在 `hft-ctl`) |
| DEP-155.21 | `hft-archive`(`hft-journal-archiver`) | `/data/prod/<子目录>/<段>` | 读;写 `<段>.uploaded`;删除段 | 允许 |
| DEP-155.22 | `hft-archive` | `/data/sim/<子目录>/<段>` | 同上 | 允许 |
| DEP-155.23 | `hft-archive` | `/var/lib/hft/archiver/advisor-daily/` | 建文件 | 允许 |
| DEP-155.24 | `hft-archive` | `/var/lib/hft/verify/runs/<文件>`、`/var/lib/hft/verify/ledger/ledger.json` | 读 | 允许 |
| DEP-155.25 | `hft-archive` | `/run/hft/creds/hft-archive/credentials` | 读 | 允许 |
| DEP-155.26 | `hft-archive` | `/run/hft/creds/hft-opsbot/credentials` | 读 | 拒绝 |
| DEP-155.27 | `hft-replay`(`hft-replay-verify`) | `/data/prod/<子目录>/<段>` | 读 | 允许 |
| DEP-155.28 | `hft-replay` | 同上 | 写或删除 | 拒绝 |
| DEP-155.29 | `hft-replay` | `/var/lib/hft/archiver/advisor-daily/<文件>` | 读(穿越 0751 的 `archiver/`) | 允许 |
| DEP-155.30 | `hft-replay` | `/var/lib/hft/archiver/` | 列目录 | 拒绝 |
| DEP-155.31 | `hft-replay` | `/var/lib/hft/replay-verify/prod/2026-10-03.json` | 建文件 | 允许 |
| DEP-155.32 | `hft-verify`(`hft-verify`) | `/var/lib/hft/verify/{ledger,runs,corpus,crashes}/` | 建文件 | 允许 |
| DEP-155.33 | `hft-verify` | `/opt/hft/verify/current/bin/hft-verify` | 执行 | 允许 |
| DEP-155.34 | `hft-watch`(`hft-watchtower`) | `/dev/shm/hft/prod/`、`/dev/hugepages/hft/prod/`、`/data/sim/` 下的段 | 读 | 允许 |
| DEP-155.35 | `hft-watch` | `/var/lib/hft/verify/runs/<文件>`、`/var/lib/hft/replay-verify/sim/<日期>.json`、`/var/lib/hft/reconciler/fatal.json` | 读 | 允许 |
| DEP-155.36 | `hft-watch` | `/var/lib/hft/guard/events.jsonl`、`/var/lib/hft/guard/tripped`(dev2) | 读 | 允许 |
| DEP-155.37 | `hft-watch` | `/var/lib/hft/watchtower/acks.jsonl` | 读写 | 允许 |
| DEP-155.38 | `hft-watch` | `/run/hft/maint/until`、`/run/hft/window/active` | 读 | 允许 |
| DEP-155.39 | `hft-watch` | `/run/hft/core-prod/ctl.sock` | 连接 | 拒绝 |
| DEP-155.40 | `hft-recon`(`hft-reconciler`) | `/run/hft/core-prod/ctl.sock` | 连接 | 允许 |
| DEP-155.41 | `hft-recon` | `/var/lib/hft/reconciler/fatal.json` | 建文件 | 允许 |
| DEP-155.42 | `hft-opsbot`(`hft-ops-bot`) | `/run/hft/core-prod/ctl.sock`、`/run/hft/core-sim/ctl.sock` | 连接 | 允许 |
| DEP-155.43 | `hft-opsbot` | `/run/hft/kill-switch/trigger.sock`、`/run/hft/pipeline/api.sock` | 连接 | 允许 |
| DEP-155.44 | `hft-opsbot` | `/var/lib/hft/watchtower/acks.jsonl` | 追加 | 允许 |
| DEP-155.45 | `hft-opsbot` | `/var/lib/hft/watchtower/` | 建新文件 | 拒绝 |
| DEP-155.46 | `hft-opsbot` | `/var/lib/hft/treasury/<文件>` | 读 | 允许 |
| DEP-155.47 | `hft-pipeline`(`hft-pipeline`) | `/run/hft/pipeline/` | 建 `api.sock` 并设属组 `hft-pipeline-api` | 允许 |
| DEP-155.48 | `hft-pipeline` | `/run/hft/core-sim/ctl.sock` | 连接 | 允许 |
| DEP-155.49 | `hft-pipeline` | `/run/hft/kill-switch/trigger.sock` | 连接 | 拒绝 |
| DEP-155.50 | `hft-pipeline` | `/dev/shm/hft/sim/` 下的段 | 读 | 允许 |
| DEP-155.51 | `hft-treasury`(`hft-treasury`) | `/var/lib/hft/treasury/` | 建文件 | 允许 |
| DEP-155.52 | `hft-treasury` | `/run/hft/core-prod/ctl.sock` | 连接 | 拒绝 |
| DEP-155.53 | `hft-guard`(`hft-dev2-guard`) | `/var/lib/hft/guard/{tripped,state.json,events.jsonl}` | 建与写 | 允许 |
| DEP-155.54 | `hft-guard` | `/sys/fs/cgroup/system.slice/memory.pressure`、`/sys/fs/cgroup/hft.slice/memory.current`、`/proc/pressure/cpu`、`/proc/vmstat` | 读 | 允许 |
| DEP-155.55 | `hft-guard` | `/dev/shm/hft/sim/heartbeat` | 读 | 拒绝(守护不读心跳块) |
| DEP-155.56 | `hft-offbox`(`hft-offbox-watchdog`) | `/var/lib/hft/offbox/`;`/etc/hft/offbox-watchdog.toml` | 建文件;读 | 允许 |
| DEP-155.57 | `ops`(运维登录用户) | `/run/hft/core-prod/ctl.sock` | 连接 | 允许 |
| DEP-155.58 | 不在 `hft-ctl` 的普通用户 `nobody2` | `/run/hft/core-prod/ctl.sock` | 连接 | 拒绝 |
| DEP-155.59 | `hft-watch` | `/var/lib/hft/archiver/alerts.jsonl`、`/var/lib/hft/pipeline/prod-pc/status.json`、`/run/hft/hostprofile.json` | 读 | 允许 |
| DEP-155.60 | `hft-opsbot` | `/var/lib/hft/archiver/alerts.jsonl` | 读 | 拒绝 |
| DEP-155.61 | `hft-watch` | `/var/lib/hft/archiver/alerts.jsonl`、`/var/lib/hft/pipeline/prod-pc/status.json` | 写 | 拒绝 |
| DEP-155.62 | `hft`、`hft-sim` | `/etc/hft/ctl-principals.toml` | 读 / 写 | 允许 / 拒绝 |

### DEP-156 在容器中逐行执行权限闭包

- 规则:[systemd 单元](README.md#systemd-单元)、[身份、路径与 S3 前缀](README.md#身份路径与-s3-前缀)
- 级别:proc
- 里程碑:M5
- 前置:容器;`tokyo-cex-1` 布局(DEP-155.36、DEP-155.53–56 用 dev2 布局容器)安装、激活并启动全部单元一次;段、标记与凭证文件按 DEP-155 各行预先建好
- 步骤:
  1. 对 DEP-155 的每一行,以 `systemd-run --wait --pipe` 带上该单元的 `User=`、`SupplementaryGroups=`、`ProtectSystem=strict`、`ReadWritePaths=`、`RuntimeDirectory=`/`StateDirectory=`(只引用,不重建)属性运行一条执行该操作的命令
- 期望:
  - 每行的结果与 DEP-155 的"期望"列一致(允许 = 命令退出 0;拒绝 = `EACCES`、`EPERM` 或 `EROFS`)
- 实现:`deploy/tests/container/test_perm.py::dep_156_closure_exec`

### DEP-157 建属组为 G 的文件或套接字的进程必须在 G 里,setgid 目录除外

- 规则:[systemd 单元](README.md#systemd-单元)
- 级别:table
- 里程碑:M5
- 前置:容器;每行以 `systemd-run` 按行给出的用户与附加组运行
- 实现:`deploy/tests/container/test_perm.py::dep_157_group_ownership`

| 行 | 进程 | 操作 | 期望 |
|---|---|---|---|
| DEP-157.1 | `hft-pipeline`,附加组去掉 `hft-pipeline-api` | 建 `/run/hft/pipeline/api.sock` 后 `chown :hft-pipeline-api` | `EPERM` |
| DEP-157.2 | `hft-ks`,附加组去掉 `hft-ks-trigger` | 建 `trigger.sock` 后 `chown :hft-ks-trigger` | `EPERM` |
| DEP-157.3 | `hft`(不在 `hft-journal-ro`) | 在 `/dev/shm/hft/prod/`(2750,组 `hft-journal-ro`)建 `heartbeat` | 成功,属组为 `hft-journal-ro` |
| DEP-157.4 | `hft` | 在去掉 setgid 位的同一目录建文件 | 属组为 `hft`(说明继承来自 setgid) |

### DEP-158 消费者角色的 S3 与 KMS 授权只覆盖登记给它的前缀

- 规则:[身份、路径与 S3 前缀](README.md#身份路径与-s3-前缀)、[IMDS 锁定与凭证代理](README.md#imds-锁定与凭证代理)
- 级别:table
- 里程碑:M5
- 前置:以 `host = "tokyo-cex-1"`、`kms_key = arn:aws:kms:ap-northeast-1:000000000000:key/test` 渲染 `secrets/iam-consumers.json.tmpl`;桶 `gwx-trading-data`;按角色收集 `(Action, Resource 或 s3:prefix 条件)`
- 实现:`deploy/tests/test_iam.py::dep_158_consumer_policies`

| 行 | 角色 | 期望 |
|---|---|---|
| DEP-158.1 | `hft-archive` | `s3:GetObject`、`s3:ListBucket`、`s3:PutObject` 于 `hft/*` 与 `hft-testnet/*`(含 `hft/verify/*`);`kms:GenerateDataKey`、`kms:Decrypt` 于该 key |
| DEP-158.2 | `hft-opsbot` | `s3:GetObject`、`s3:ListBucket` 于 `hft/*` 与 `hft-testnet/*`;`kms:Decrypt`;没有 `s3:PutObject` |
| DEP-158.3 | `hft-pipeline` | `s3:GetObject`、`s3:ListBucket` 于 `hft/*`、`hft-testnet/*` 与 collector 前缀(取 pipeline 配置 `warmup.collector_prefix`,夹具 `collector/`);`s3:PutObject` 只于 `hft/tokyo-cex-1/*/private/params/*` 与 `hft-testnet/tokyo-cex-1/*/private/params/*`;`kms:GenerateDataKey`、`kms:Decrypt` |
| DEP-158.4 | `hft-treasury` | `s3:GetObject`、`s3:ListBucket` 只于 `hft/*`;`kms:Decrypt`;没有 `hft-testnet/*` |
| DEP-158.5 | 每个角色 | 没有 `s3:DeleteObject`、`s3:*`、`kms:*`;没有 `hft-bench/*` |
| DEP-158.6 | 每个角色的信任策略 | 只信任 dev2 与升级主机的实例角色 |

### DEP-159 消费者凭证在真实桶上的授权边界

- 规则:[身份、路径与 S3 前缀](README.md#身份路径与-s3-前缀)、[IMDS 锁定与凭证代理](README.md#imds-锁定与凭证代理)
- 级别:host
- 里程碑:M5
- 环境:dev2,凭证代理运行
- 授权:`[需授权]`(向生产桶写测试对象)
- 步骤:
  1. 以 `hft-archive` 的凭证文件写 `hft-testnet/dev2-1/prod/_t/DEP-159` 与 `hft/dev2-1/sim/private/_t/DEP-159`(SSE-KMS)
  2. 以 `hft-opsbot` 的凭证 `GetObject` 第 1 步的私有对象,再尝试 `PutObject` 到 `hft/dev2-1/sim/_t/x`
  3. 以 `hft-treasury` 的凭证 `GetObject` `hft-testnet/dev2-1/prod/_t/DEP-159`
  4. `ls -l /run/hft/creds/*/credentials`
- 通过标准:步骤 1 两次写都成功;步骤 2 读成功、写 `AccessDenied`;步骤 3 `AccessDenied`;步骤 4 每个文件属主为对应用户、模式 `-r--------`
- 证据:`ops-log/<UTC 日期>-DEP-159.md`
- 实现:`deploy/tests/host/DEP-159.sh`

### DEP-160 运维只读角色 `hft-ops-read` 的授权

- 规则:[身份、路径与 S3 前缀](README.md#身份路径与-s3-前缀)
- 级别:table
- 里程碑:M5
- 前置:渲染运维只读角色的策略模板(同 DEP-158 的参数)
- 实现:`deploy/tests/test_iam.py::dep_160_ops_read_role`

| 行 | 项 | 期望 |
|---|---|---|
| DEP-160.1 | S3 | `s3:GetObject`、`s3:ListBucket` 于 `hft/*`、`hft-testnet/*`、`hft-bench/*`;没有任何写动作 |
| DEP-160.2 | KMS | 只有 `kms:Decrypt` 于该 key |
| DEP-160.3 | 信任 | 只信任模板参数给出的负责人与研究者 IAM 身份 |

### DEP-165 每个单元的秘密清单只引用允许的前缀

- 规则:[秘密与实例角色](README.md#秘密与实例角色)
- 级别:table
- 里程碑:M5
- 前置:以夹具账户(`bn-1` 于 `binance-usdm`)渲染 `secrets/<unit>.toml.tmpl`,`host` 按行;解析每项的 `secret_id`
- 实现:`deploy/tests/test_secrets.py::dep_165_manifest_prefixes`

| 行 | 清单(主机) | 期望的 `secret_id` 前缀集合 |
|---|---|---|
| DEP-165.1 | `hft-core.toml`(`tokyo-cex-1`) | 只有 `hft/live/bn-1/binance-usdm/` |
| DEP-165.2 | `hft-core.toml`(`dev2-1`,测试网账户 `bn-t1`) | 只有 `hft/testnet/bn-t1/binance-usdm/` |
| DEP-165.3 | `kill-switch.toml`(`tokyo-cex-1`) | `hft/live/bn-1/binance-usdm/`(撤单 key)与 `hft/ops/kill-switch/` |
| DEP-165.4 | `reconciler.toml`(`tokyo-cex-1`) | `hft/live/bn-1/binance-usdm/`(只读 key) |
| DEP-165.5 | `watchtower.toml` | 只有 `hft/ops/watchtower/` |
| DEP-165.6 | `ops-bot.toml`(`tokyo-cex-1`) | 只有 `hft/ops/ops-bot/tokyo-cex-1/` |
| DEP-165.7 | `dev2-guard.toml`、`offbox-watchdog.toml`、`cred-broker.toml` | 分别只有 `hft/ops/guard/`、`hft/ops/offbox/`、`hft/ops/cred-broker/` |
| DEP-165.8 | 任一 `dev2-1` 渲染出的清单 | 没有 `hft/live/` |
| DEP-165.9 | 模板集合 | 没有给 `hft-core@sim` 的清单 |

### DEP-166 取秘密的单元用 `ExecStartPre=+` 取到自己的运行目录

- 规则:[秘密与实例角色](README.md#秘密与实例角色)、[systemd 单元](README.md#systemd-单元)
- 级别:table
- 里程碑:M5
- 前置:夹具 `tokyo-cex-1`、`dev2-1` 的单元;每行取该单元唯一一条含 `hft-secrets` 的行
- 实现:`tools/xtask/tests/deploy_units.rs::dep_166_secret_fetch_lines`

| 行 | 单元 | 期望的整行 |
|---|---|---|
| DEP-166.1 | `hft-core@prod` | `ExecStartPre=+/opt/hft/current-core/bin/hft-secrets fetch --manifest /etc/hft/secrets/hft-core.toml --out /run/hft/core-prod/secrets --owner hft --mode 0400` |
| DEP-166.2 | `hft-kill-switch` | `ExecStartPre=+/opt/hft/current-cold/bin/hft-secrets fetch --manifest /etc/hft/secrets/kill-switch.toml --out /run/hft/kill-switch/secrets --owner hft-ks --mode 0400` |
| DEP-166.3 | `hft-reconciler` | `… /opt/hft/current-cold/bin/hft-secrets … reconciler.toml --out /run/hft/reconciler/secrets --owner hft-recon --mode 0400` |
| DEP-166.4 | `hft-watchtower` | `… current-cold … watchtower.toml --out /run/hft/watchtower/secrets --owner hft-watch --mode 0400` |
| DEP-166.5 | `hft-ops-bot` | `… current-cold … ops-bot.toml --out /run/hft/ops_bot/secrets --owner hft-opsbot --mode 0400` |
| DEP-166.6 | `hft-dev2-guard`(`dev2-1`) | `… current-core … dev2-guard.toml --out /run/hft/guard/secrets --owner hft-guard --mode 0400` |
| DEP-166.7 | `hft-offbox-watchdog`(`dev2-1`) | `… current-core … offbox-watchdog.toml --out /run/hft/offbox/secrets --owner hft-offbox --mode 0400` |
| DEP-166.8 | `hft-cred-broker` | `… current-core … cred-broker.toml --out /run/hft/creds/secrets --owner root --mode 0400` |
| DEP-166.9 | 其余单元(含 `hft-core@sim`) | 没有含 `hft-secrets` 的行 |

### DEP-167 实例角色的 IAM 模板按主机角色授权

- 规则:[秘密与实例角色](README.md#秘密与实例角色)
- 级别:table
- 里程碑:M5
- 前置:以 `account_id = 000000000000` 渲染 `secrets/iam-dev2.json.tmpl` 与 `secrets/iam-prod.json.tmpl`
- 实现:`deploy/tests/test_iam.py::dep_167_instance_roles`

| 行 | 模板 | 期望 |
|---|---|---|
| DEP-167.1 | `iam-dev2` | `secretsmanager:GetSecretValue` 只于 `hft/testnet/*` 与 `hft/ops/*`;`sts:AssumeRole` 只于 hft 各消费者角色;没有 `hft/live/*`、没有 `s3:*` 动作 |
| DEP-167.2 | `iam-prod` | `secretsmanager:GetSecretValue` 于 `hft/live/*`、`hft/testnet/*`、`hft/ops/*`;`sts:AssumeRole` 于各消费者角色;附托管策略 `AmazonSSMManagedInstanceCore`;没有 `s3:*` 动作 |
| DEP-167.3 | 两个模板 | 没有任何 `ec2:` 写动作(`Attach*`、`Associate*`、`Create*`、`Modify*`、`Assign*`) |

### DEP-168 `formal_live = false` 的主机预检拒绝正式端点与正式秘密

- 规则:[秘密与实例角色](README.md#秘密与实例角色)、[发布归档与 releasectl](README.md#发布归档与-releasectl)
- 级别:table
- 里程碑:M5
- 前置:替身根,清单 `dev2-1`;其余预检项都通过;`aws secretsmanager describe-secret --secret-id hft/live/_sentinel` 的替身结果按行;hft-core 自身的启动拒绝由 CORE-041、CORE-046 覆盖
- 实现:`deploy/tests/test_preflight.py::dep_168_not_formal_live`

| 行 | 条件 | 期望 |
|---|---|---|
| DEP-168.1 | `hft-core-prod.toml` 的 `instance.order_env = "prod"` | `preflight.sh` 退出 1,失败项含 `order_env` |
| DEP-168.2 | `order_env = "testnet"`,哨兵返回 `AccessDeniedException` | 退出 0 |
| DEP-168.3 | `/etc/hft/secrets/kill-switch.toml` 引用 `hft/live/bn-1/binance-usdm/cancel` | 退出 1,失败项指出该清单 |
| DEP-168.4 | 哨兵调用成功(返回元数据) | 退出 1 |
| DEP-168.5 | 哨兵调用返回网络错误 | 退出 1(只有 `AccessDenied` 算通过) |
| DEP-168.6 | 清单 `tokyo-cex-1`(`formal_live = true`)、`order_env = "prod"` | 不做本组三项,退出 0 |

### DEP-170 dev2 以外的主机类加载 BPF connect 钩子

- 规则:[IMDS 锁定与凭证代理](README.md#imds-锁定与凭证代理)
- 级别:unit
- 里程碑:M5
- 前置:替身根;清单 `tokyo-cex-1`;`bpftool` 替身记录调用并模拟内核挂载表
- 步骤:
  1. 运行 `imds/hft-imds-lock.sh load`
  2. 再运行一次
- 期望:
  - 步骤 1 后:程序 `hft_imds4` 挂在根 cgroup 的 `connect4`、`hft_imds6` 挂在 `connect6`;`/sys/fs/bpf/hft-imds/connect4` 与 `/sys/fs/bpf/hft-imds/connect6` 存在;对象文件取自 `/opt/hft/current-core/deploy/imds/hft-imds-connect.bpf.o`;没有任何 `nft` 调用
  - 步骤 2 后两个钩子各仍只挂一个 `hft_imds*` 程序
- 实现:`deploy/tests/test_imds.py::dep_170_bpf_load`

### DEP-171 dev2 加载按 uid 丢弃的 nft 表

- 规则:[IMDS 锁定与凭证代理](README.md#imds-锁定与凭证代理)
- 级别:unit
- 里程碑:M5
- 前置:替身根;清单 `dev2-1`;`nft` 替身记录调用并模拟规则集;用户表全部用户已建(DEP-150),夹具运维登录用户 `ops`
- 步骤:
  1. 运行 `imds/hft-imds-lock.sh load`,读 `nft list table inet hft_imds` 的替身输出
  2. 再运行一次
- 期望:
  - 步骤 1:表 `inet hft_imds`、链 `out`(`type filter hook output priority 0; policy accept`);集合 `hft_uids` = {`hft`、`hft-sim`、`hft-ks`、`hft-archive`、`hft-replay`、`hft-verify`、`hft-watch`、`hft-recon`、`hft-opsbot`、`hft-pipeline`、`hft-treasury`、`hft-guard`、`hft-offbox`},不含 root 与 `ops`;恰好两条规则:`meta skuid @hft_uids ip daddr 169.254.169.254 counter drop` 与 `meta skuid @hft_uids ip6 daddr fd00:ec2::254 counter drop`;没有任何 `bpftool` 调用
  - 步骤 2 后规则仍恰好两条
- 实现:`deploy/tests/test_imds.py::dep_171_nft_load`

### DEP-172 `hft-imds-lock.sh status` 报告规则是否就位与拦截计数

- 规则:[IMDS 锁定与凭证代理](README.md#imds-锁定与凭证代理)
- 级别:table
- 里程碑:M5
- 前置:替身根;按行设定替身中的挂载或规则状态
- 实现:`deploy/tests/test_imds.py::dep_172_status`

| 行 | 状态 | 期望 |
|---|---|---|
| DEP-172.1 | `tokyo-cex-1`,两个程序已挂,计数 map 中 7 | 退出 0;输出两个程序名与计数 7 |
| DEP-172.2 | `tokyo-cex-1`,`connect6` 上没有 `hft_imds6` | 退出非 0;输出缺失的钩子 |
| DEP-172.3 | `dev2-1`,表与两条规则在,计数 3 与 0 | 退出 0;输出两条规则的计数 |
| DEP-172.4 | `dev2-1`,表不存在 | 退出非 0 |

### DEP-173 BPF 钩子使非 root 进程连不上 IMDS,root 与其他地址不受影响

- 规则:[IMDS 锁定与凭证代理](README.md#imds-锁定与凭证代理)
- 级别:proc
- 里程碑:M5
- 前置:特权容器(cgroup v2,内核支持 `cgroup/connect4`);一个 dummy 网卡带 `169.254.169.254/32` 与 `fd00:ec2::254/128`,其上监听一个 TCP 替身;`tokyo-cex-1` 布局,`hft-imds-lock.service` 已运行
- 步骤:
  1. 以 `hft-pipeline` 身份 `connect()` 到 `169.254.169.254:80` 与 `[fd00:ec2::254]:80`
  2. 以 root 身份连接同一地址
  3. 以 `hft-pipeline` 身份连接 `10.99.0.1:80`(同一替身的另一地址)
  4. 读计数
- 期望:
  - 步骤 1 两次都返回 `EPERM`;步骤 2、3 连接成功
  - 计数 = 2(只计步骤 1)
- 实现:`deploy/tests/container/test_imds_proc.py::dep_173_bpf_blocks_nonroot`

### DEP-174 dev2 的 nft 规则只挡 hft 的 uid

- 规则:[IMDS 锁定与凭证代理](README.md#imds-锁定与凭证代理)
- 级别:proc
- 里程碑:M5
- 前置:特权容器;同 DEP-173 的 dummy 网卡与替身;dev2 布局,`hft-imds-lock.service` 已运行;另建一个不在用户表中的用户 `legacy`(模拟旧系统)
- 步骤:
  1. 以 `hft-opsbot` 身份连接 `169.254.169.254:80`(超时 2 s)
  2. 以 `legacy` 与 root 身份连接同一地址
- 期望:
  - 步骤 1 连接超时;第一条规则计数加 1
  - 步骤 2 两次都成功;计数不再变化
- 实现:`deploy/tests/container/test_imds_proc.py::dep_174_nft_blocks_hft_uids`

### DEP-175 每个主机类都安装 IMDS 锁定,机制按主机类

- 规则:[IMDS 锁定与凭证代理](README.md#imds-锁定与凭证代理)
- 级别:table
- 里程碑:M5
- 前置:夹具清单的激活安装集合
- 实现:`tools/xtask/tests/deploy_gen.rs::dep_175_imds_by_class`

| 行 | 清单 | 期望 |
|---|---|---|
| DEP-175.1 | `tokyo-cex-1`、`probe-c32`、`c4-1` | 安装并启用 `hft-imds-lock.service`;`load` 走 BPF 分支;不安装 nftables 规则文件 |
| DEP-175.2 | `dev2-1` | 安装并启用;`load` 走 nft 分支 |
| DEP-175.3 | AMI provision(升级主机) | 不安装 `ec2-instance-connect` 包;安装 `bpftool` |

### DEP-176 凭证代理为每个消费者写只属于它的短期凭证文件

- 规则:[IMDS 锁定与凭证代理](README.md#imds-锁定与凭证代理)
- 级别:unit
- 里程碑:M5
- 前置:替身根;`consumers.toml` 渲染自 `creds/consumers.toml.tmpl`(`host = "tokyo-cex-1"`);`aws sts assume-role` 替身返回到期时间 = 调用时刻 + 60 min 的凭证;替身时钟 t = 0
- 步骤:
  1. 启动 `hft-cred-broker`,推进到 t = 1 min
- 期望:
  - 对 `hft-archive`、`hft-opsbot`、`hft-pipeline`、`hft-treasury` 各调用一次 `assume-role`,目标为该消费者的专属角色,附带 session policy(其前缀与 DEP-158 中该角色相同)
  - 四个 `/run/hft/creds/<用户>/credentials` 都存在:文件属主为该用户、模式 0400;目录属主为该用户、模式 0500;内容含 `aws_access_key_id`、`aws_secret_access_key`、`aws_session_token`
  - 每个文件以"写临时文件再改名"的方式出现(替身记录中没有对目标路径的截断写)
- 实现:`deploy/tests/test_creds.py::dep_176_writes_credentials`

### DEP-177 凭证代理的刷新周期、失败重试与告警时点

- 规则:[IMDS 锁定与凭证代理](README.md#imds-锁定与凭证代理)
- 级别:table
- 里程碑:M5
- 前置:替身时钟;消费者 `hft-archive` 的凭证在 t = 0 签发、t = 60 min 到期;告警替身记录级别与时刻;`assume-role` 替身按行在某些时刻失败
- 实现:`deploy/tests/test_creds.py::dep_177_refresh_and_alerts`

| 行 | `assume-role` 结果 | 期望 |
|---|---|---|
| DEP-177.1 | t = 45 min 成功 | t = 45 min 换为新凭证(新到期 t = 105 min);没有告警;下次刷新在 t = 90 min |
| DEP-177.2 | t = 45 min 失败,之后每次都失败 | t = 45 min 一条 P1;文件内容不变;t = 46…54 min 每分钟各重试一次,没有新告警;t = 55 min(剩余 5 min,不小于 5 min)无 P0;t = 56 min 一条 P0;t = 57 min 及以后不再发 P0 |
| DEP-177.3 | t = 45 min 失败,t = 50 min 成功 | 只有 t = 45 min 的一条 P1;t = 50 min 换为新凭证 |
| DEP-177.4 | 告警通道 | 两种告警都经 `hft/ops/cred-broker/` 的 token 发出 |

### DEP-178 消费者配置与 S3 前缀表一致

- 规则:[IMDS 锁定与凭证代理](README.md#imds-锁定与凭证代理)、[身份、路径与 S3 前缀](README.md#身份路径与-s3-前缀)
- 级别:table
- 里程碑:M5
- 前置:渲染 `creds/consumers.toml.tmpl`(`host = "tokyo-cex-1"`)
- 实现:`deploy/tests/test_creds.py::dep_178_consumers_config`

| 行 | 消费者 | 期望 |
|---|---|---|
| DEP-178.1 | `hft-archive` | 写 `hft/`、`hft-testnet/`(含 `hft/verify/`) |
| DEP-178.2 | `hft-opsbot` | 只读 `hft/*`、`hft-testnet/*` |
| DEP-178.3 | `hft-pipeline` | 只读输入前缀与 collector 前缀;写 `hft/tokyo-cex-1/*/private/params/` 与 `hft-testnet/tokyo-cex-1/*/private/params/` |
| DEP-178.4 | `hft-treasury` | 只读 `hft/*` |
| DEP-178.5 | 集合 | 恰为这四个消费者;没有 `hft`、`hft-sim`、`hft-ks` 等其他用户 |

### DEP-179 IMDS 相关的预检项

- 规则:[IMDS 锁定与凭证代理](README.md#imds-锁定与凭证代理)、[发布归档与 releasectl](README.md#发布归档与-releasectl)
- 级别:table
- 里程碑:M5
- 前置:替身根,清单 `tokyo-cex-1`(DEP-179.6 用 `dev2-1`);其余预检项都通过;S3 替身按行;`_preflight` 对象键 = `<prefix>/<host>/<instance>/private/_preflight`(`prod` 的前缀按其归档 source:`order_env = "testnet"` 时为 `hft-testnet`)
- 实现:`deploy/tests/test_preflight.py::dep_179_imds_items`

| 行 | 条件 | 期望 |
|---|---|---|
| DEP-179.1 | 以 `runuser -u hft-pipeline` 访问 IMDS 成功 | `preflight.sh` 退出 1,失败项为 IMDS |
| DEP-179.2 | `hft-imds-lock.service` inactive | 退出 1 |
| DEP-179.3 | `status` 显示的程序名不是 `hft_imds4`/`hft_imds6` | 退出 1 |
| DEP-179.4 | `ec2-instance-connect` 已安装 | 退出 1 |
| DEP-179.5 | `ec2-instance-connect` 已安装,清单 `dev2-1` | 不判这一项(dev2 不检查) |
| DEP-179.6 | `hft-opsbot` 的凭证文件模式 0644 | 退出 1 |
| DEP-179.7 | `hft-pipeline` 的凭证已过期 | 退出 1 |
| DEP-179.8 | `hft-archive` 写 `hft-testnet/tokyo-cex-1/prod/private/_preflight`(SSE-KMS)后 `HEAD` 成功;写 `hft/tokyo-cex-1/sim/private/_preflight` 成功;每个读者 `HEAD` 其前缀成功 | 退出 0;同键覆盖,不新增其他对象 |
| DEP-179.9 | 同上,但 `hft-treasury` 的 `HEAD` 返回 `AccessDenied` | 退出 1,失败项含 `hft-treasury` |

### DEP-180 消费者单元关闭 IMDS 并指向自己的凭证文件

- 规则:[IMDS 锁定与凭证代理](README.md#imds-锁定与凭证代理)
- 级别:table
- 里程碑:M5
- 前置:夹具 `tokyo-cex-1` 的单元
- 实现:`tools/xtask/tests/deploy_units.rs::dep_180_consumer_env`

| 行 | 单元 | 期望的 `Environment=` |
|---|---|---|
| DEP-180.1 | `hft-journal-archiver` | `AWS_EC2_METADATA_DISABLED=true`、`AWS_SHARED_CREDENTIALS_FILE=/run/hft/creds/hft-archive/credentials` |
| DEP-180.2 | `hft-ops-bot` | 同上,文件为 `/run/hft/creds/hft-opsbot/credentials` |
| DEP-180.3 | `hft-pipeline` | 文件为 `/run/hft/creds/hft-pipeline/credentials` |
| DEP-180.4 | `hft-treasury` | 文件为 `/run/hft/creds/hft-treasury/credentials` |
| DEP-180.5 | 其他非 root 单元 | 没有 `AWS_SHARED_CREDENTIALS_FILE` |

### DEP-181 凭证刷新后消费者不重启也能读 S3

- 规则:[IMDS 锁定与凭证代理](README.md#imds-锁定与凭证代理)
- 级别:host
- 里程碑:M5
- 环境:dev2;凭证代理的刷新周期临时改为 5 min、凭证有效期 15 min(单元覆盖文件)
- 授权:`[需授权]`(在 dev2 上改凭证代理的运行参数)
- 步骤:
  1. 记录 `hft-ops-bot` 与 `hft-pipeline` 的 `MainPID`;运行 2 h,每 10 min 让两者各发起一次 S3 读取(ops_bot 的报表命令、pipeline 的一次 dry-run 取数)
  2. 恢复原参数
- 通过标准:期间两个 `MainPID` 不变;每次读取成功(没有 `ExpiredToken`);凭证文件 mtime 至少变化 20 次
- 证据:`ops-log/<UTC 日期>-DEP-181.md`
- 实现:`deploy/tests/host/DEP-181.sh`

### DEP-182 运维命令以服务用户身份访问 S3

- 规则:[IMDS 锁定与凭证代理](README.md#imds-锁定与凭证代理)
- 级别:host
- 里程碑:M5
- 环境:dev2
- 授权:无(只读)
- 步骤:
  1. `sudo -u hft-archive env AWS_EC2_METADATA_DISABLED=true AWS_SHARED_CREDENTIALS_FILE=/run/hft/creds/hft-archive/credentials journal-archiver check`
  2. 同一身份 `aws s3 sync s3://gwx-trading-data/hft/verify/ /tmp/DEP-182/`
  3. `sudo -u hft-archive env AWS_EC2_METADATA_DISABLED=true aws sts get-caller-identity`(不给凭证文件)
- 通过标准:步骤 1、2 退出 0;步骤 3 失败(没有凭证,IMDS 被挡)
- 证据:`ops-log/<UTC 日期>-DEP-182.md`
- 实现:`deploy/tests/host/DEP-182.sh`

### DEP-185 模拟实例的部署形态

- 规则:[模拟实例(sim)](README.md#模拟实例sim)
- 级别:table
- 里程碑:M4
- 前置:夹具 `tokyo-cex-1` 与 `dev2-1` 的生成物与配置模板
- 实现:`tools/xtask/tests/deploy_gen.rs::dep_185_sim_form`

| 行 | 项 | 期望 |
|---|---|---|
| DEP-185.1 | 二进制 | `hft-core@sim` 的 `ExecStart` 经 `/opt/hft/current-sim/bin/hft-core` |
| DEP-185.2 | 配置 | `/etc/hft/hft-core-sim.toml`:`instance.name = "sim"`,没有 `secrets_dir` 与任何 `keys` |
| DEP-185.3 | journal | 冷副本根 `/data/sim`;热段根 `/dev/shm/hft/sim` |
| DEP-185.4 | 核(`tokyo-cex-1`) | `CPUAffinity=23-27`;`[[threads]]` 中 sim 线程的核 ⊆ {23–27} |
| DEP-185.5 | 出口 | `hft-core-sim.toml` 的每个 `src_ip` 都在 `egress.toml` 的 `sim` 列表 |
| DEP-185.6 | ctl socket | `paths.ctl_socket = "/run/hft/core-sim/ctl.sock"` |
| DEP-185.7 | 读者配置 | watchtower、archiver、ops_bot、pipeline、treasury 的配置列出 `prod` 与 `sim` 两个实例;kill-switch 的 `sim` 目标表为空;reconciler 不列 `sim` |

### DEP-186 模拟实例的预检项

- 规则:[模拟实例(sim)](README.md#模拟实例sim)、[发布归档与 releasectl](README.md#发布归档与-releasectl)
- 级别:table
- 里程碑:M4
- 前置:替身根,清单 `tokyo-cex-1`(隔离核 5–17、IRQ 专用核 1 与 22);其余预检项都通过;每行只做该行改动
- 实现:`deploy/tests/test_preflight.py::dep_186_sim_items`

| 行 | 改动 | 期望 |
|---|---|---|
| DEP-186.1 | 无 | `preflight.sh` 退出 0 |
| DEP-186.2 | `hft-core-sim.toml` 的 `instance.name = "prod"` | 退出 1 |
| DEP-186.3 | `hft-core-sim.toml` 有 `paths.secrets_dir` | 退出 1 |
| DEP-186.4 | sim 的 drop-in 含 `hft-secrets` | 退出 1 |
| DEP-186.5 | sim 的 drop-in 含 `/run/hft/core-prod/` | 退出 1 |
| DEP-186.6 | `hft-sim` 加入组 `hft` | 退出 1 |
| DEP-186.7 | `hft-sim` 加入组 `hft-stopmark` | 退出 1 |
| DEP-186.8 | `hft-sim.slice` 的 `AllowedCPUs=17,23-27` | 退出 1(含隔离核 17) |
| DEP-186.9 | `AllowedCPUs=22-27` | 退出 1(含 IRQ 专用核 22) |
| DEP-186.10 | `egress.toml` 的 `sim` 含 `10.0.1.10` | 退出 1 |

### DEP-187 两个实例互不依赖,激活各自的组不影响对方

- 规则:[模拟实例(sim)](README.md#模拟实例sim)、[单元组与分组激活](README.md#单元组与分组激活)
- 级别:proc
- 里程碑:M4
- 前置:容器;`tokyo-cex-1` 布局,`v1.2.3` 已激活三组;两个 hft-core 替身都 active,记录 `MainPID`
- 步骤:
  1. 安装 `v1.2.4`,`releasectl activate --version v1.2.4 --group sim`
  2. `releasectl activate --version v1.2.4 --group core`
  3. `systemctl stop hft-core@prod`,再 `systemctl restart hft-core@sim`
- 期望:
  - 步骤 1 后 `hft-core@sim` inactive(冷生产),`hft-core@prod` 的 `MainPID` 不变;`current-sim` → `v1.2.4`,`current-core` 仍为 `v1.2.3`
  - 步骤 2 后 `hft-core@prod` inactive;`hft-core@sim` 的状态不受这一步影响
  - 步骤 3 中 `hft-core@sim` 得到新的 `MainPID` 并回到 active,`hft-core@prod` 保持 inactive(没有被启动)
- 实现:`deploy/tests/container/test_groups.py::dep_187_instances_independent`

### DEP-190 验证包可复现打包,内容与 `VERIFY.json` 一致

- 规则:[验证包与 hft-verify](README.md#验证包与-hft-verify)
- 级别:unit
- 里程碑:M3
- 前置:夹具提交;`SOURCE_DATE_EPOCH = 0`;夹具目标清单 2 个 core 解码器目标 `decode_bbo_binance`、`decode_ack_okx`
- 步骤:
  1. 运行两次 `deploy/release/package-verify.sh v1.2.3 <sha> dist`(第二次写 `dist2`)
- 期望:
  - 两份归档逐字节相同,`.sha256` 相同
  - 归档含 `fuzz/decode_bbo_binance`、`fuzz/decode_ack_okx`、`bin/hft-dst`、`bin/hft-verify`、`corpus/decode_bbo_binance/`、`corpus/decode_ack_okx/`、`VERIFY.json`
  - `VERIFY.json` 的键为 `format`(= 1)、`version`、`git_sha`、`nightly_date`、`targets`、`path_digest`、`gates`、`files`(字段归 tools);`path_digest` 恰有 `decode_bbo_binance`、`decode_ack_okx`、`dst` 三项;`files` 覆盖包内除 `VERIFY.json` 外的全部文件,每项 SHA-256 与归档内文件相同
- 实现:`deploy/tests/test_verify_bundle.py::dep_190_package_verify`

### DEP-191 `releasectl install-verify` 校验、切换并只保留两个版本

- 规则:[验证包与 hft-verify](README.md#验证包与-hft-verify)
- 级别:table
- 里程碑:M3
- 前置:替身根;`/opt/hft/current-core` → `releases/v1.2.3`;`fetch-release.sh --asset verify` 的替身返回夹具验证包;`systemctl` 替身记录调用
- 实现:`deploy/tests/test_verify_bundle.py::dep_191_install_verify`

| 行 | 条件 | 期望 |
|---|---|---|
| DEP-191.1 | 首次安装 `v1.2.3` | 退出 0;`/opt/hft/verify/v1.2.3/` 存在;`/opt/hft/verify/current` → 它;`current-core` 不变;没有任何 `systemctl` 改变状态的调用 |
| DEP-191.2 | 归档改动一个字节 | 退出 1;`verify/current` 不变 |
| DEP-191.3 | 归档完整但 `VERIFY.json` 中一个文件的哈希不符 | 退出 1 |
| DEP-191.4 | 已有 `v1.2.1`(上一个)与 `v1.2.2`(当前),安装 `v1.2.3` | `verify/` 下只剩 `v1.2.2` 与 `v1.2.3`;`current` → `v1.2.3` |
| DEP-191.5 | `/usr/local/bin/hft-verify` | 指向 `/opt/hft/verify/current/bin/hft-verify` |

### DEP-192 `hft-verify` 单元与定时器

- 规则:[验证包与 hft-verify](README.md#验证包与-hft-verify)
- 级别:table
- 里程碑:M3
- 前置:夹具 `tokyo-cex-1`(升级主机类)的 `hft-verify.service`/`.timer`;预算取默认
- 实现:`tools/xtask/tests/deploy_units.rs::dep_192_verify_unit`

| 行 | 项 | 期望 |
|---|---|---|
| DEP-192.1 | 基本 | `Type=oneshot`、`User=hft-verify`、`Slice=hftbatch.slice` |
| DEP-192.2 | 预算默认值 | `ExecStart=/opt/hft/verify/current/bin/hft-verify run --budget-min 120`;`TimeoutStartSec=130min` |
| DEP-192.3 | 停止 | `TimeoutStopSec=30`、`KillMode=mixed` |
| DEP-192.4 | 状态目录 | `StateDirectory=hft/verify`、`StateDirectoryMode=0750` |
| DEP-192.5 | 网络 | `PrivateNetwork=yes`、`RestrictAddressFamilies=AF_UNIX` |
| DEP-192.6 | 条件(升级主机) | `ExecCondition=` 依次为 `+/usr/bin/test ! -e /var/lib/hft/guard/tripped`、`+/usr/bin/test ! -e /var/lib/hft/measure/active`、`/opt/hft/current-core/deploy/bin/hft-unit-idle hft-replay-verify.service` |
| DEP-192.7 | 定时器 | `OnBootSec=15min`、`OnUnitInactiveSec=10min` |

### DEP-193 停止 `hft-verify` 时 SIGTERM 只发给驱动进程

- 规则:[验证包与 hft-verify](README.md#验证包与-hft-verify)
- 级别:proc
- 里程碑:M3
- 前置:容器;`hft-verify` 驱动替身启动一个子进程,子进程记录收到的信号;驱动收到 SIGTERM 后向子进程转发 SIGTERM 并在 2 s 后退出
- 步骤:
  1. 启动单元,`systemctl stop hft-verify`
  2. 另一轮:驱动替身忽略 SIGTERM,`systemctl stop hft-verify`
- 期望:
  - 步骤 1:子进程收到的 SIGTERM 发送者是驱动的 pid(`si_pid`),不是 systemd;单元在 3 s 内 inactive
  - 步骤 2:30 s 后驱动与子进程都被 SIGKILL;单元结果 `timeout`
- 实现:`deploy/tests/container/test_units.py::dep_193_verify_kill_mode`

### DEP-194 `hft-verify` 单元内没有网络

- 规则:[验证包与 hft-verify](README.md#验证包与-hft-verify)
- 级别:proc
- 里程碑:M3
- 前置:容器;驱动替身尝试三种操作并记录结果
- 步骤:
  1. 在单元内创建 `AF_INET` socket;创建 `AF_UNIX` socket;`getaddrinfo("github.com")`
- 期望:
  - `AF_INET` 返回 `EAFNOSUPPORT`;`AF_UNIX` 成功;域名解析失败
- 实现:`deploy/tests/container/test_units.py::dep_194_verify_no_network`

### DEP-195 `hft-verify` 在回放、`tripped` 或测量窗口时跳过

- 规则:[验证包与 hft-verify](README.md#验证包与-hft-verify)
- 级别:table
- 里程碑:M3
- 前置:容器;`tokyo-cex-1` 布局;每行单独设置条件后 `systemctl start hft-verify`
- 实现:`deploy/tests/container/test_units.py::dep_195_verify_conditions`

| 行 | 条件 | 期望 |
|---|---|---|
| DEP-195.1 | `hft-replay-verify` 运行中 | 未进入 active;`exec-condition` 跳过 |
| DEP-195.2 | `/var/lib/hft/guard/tripped` 存在 | 跳过 |
| DEP-195.3 | `/var/lib/hft/measure/active` 存在 | 跳过 |
| DEP-195.4 | 测量窗口标志存在时重启容器(标志在持久目录) | 开机 15 min 后定时器到点,仍跳过 |
| DEP-195.5 | 三项都不满足 | 进入 active |

### DEP-196 `verify = false` 的主机上没有 `hft-verify.*`,装着就预检失败

- 规则:[验证包与 hft-verify](README.md#验证包与-hft-verify)、[主机清单](README.md#主机清单)
- 级别:table
- 里程碑:M7
- 前置:替身根,清单 `tokyo-cex-1-live`;其余预检项都通过
- 实现:`deploy/tests/test_preflight.py::dep_196_verify_false`

| 行 | 条件 | 期望 |
|---|---|---|
| DEP-196.1 | 激活后 | `systemctl show -p LoadState hft-verify.service hft-verify.timer` 都是 `not-found`;`preflight.sh` 退出 0 |
| DEP-196.2 | 手工放回 `hft-verify.timer` | `preflight.sh` 退出 1,失败项含 `hft-verify.timer` |
| DEP-196.3 | 手工放回 `hft-verify.service` | 退出 1 |

### DEP-197 验证运行记录经 archiver 的 `files` source 备份

- 规则:[验证包与 hft-verify](README.md#验证包与-hft-verify)
- 级别:table
- 里程碑:M5
- 前置:渲染 `dev2-1` 的 `journal-archiver.toml` 配置模板;排除规则与对象键的行为由 journal-archiver 的用例覆盖
- 实现:`deploy/tests/test_config_templates.py::dep_197_verify_files_source`

| 行 | 项 | 期望 |
|---|---|---|
| DEP-197.1 | source 种类与根 | 恰有一个 `kind = "files"` 的 source,根为 `/var/lib/hft/verify` |
| DEP-197.2 | 包含 | `ledger/**`、`runs/**`、`crashes/**`、`resolutions/**`;不含 `corpus/` |
| DEP-197.3 | 前缀 | `hft/verify` |

### DEP-198 `hft-verify` 的安装范围随清单

- 规则:[验证包与 hft-verify](README.md#验证包与-hft-verify)
- 级别:table
- 里程碑:M7
- 前置:夹具清单的激活安装集合
- 实现:`tools/xtask/tests/deploy_gen.rs::dep_198_verify_scope`

| 行 | 清单 | 期望 |
|---|---|---|
| DEP-198.1 | `dev2-1` | 安装 `hft-verify.service`/`.timer`,定时器启用 |
| DEP-198.2 | `tokyo-cex-1`(M7) | 安装;条件含测量窗口标志(DEP-192.6) |
| DEP-198.3 | `tokyo-cex-1-live`(M8) | 不安装 |
| DEP-198.4 | `probe-c16`、`probe-c32`、`c4-1` | 不安装 |

### DEP-200 机外看门狗单元

- 规则:[机外看门狗](README.md#机外看门狗)、[systemd 单元](README.md#systemd-单元)
- 级别:table
- 里程碑:M5
- 前置:夹具 `dev2-1` 与 `tokyo-cex-1` 的生成物;`MemoryMax` 取 hft-core `dev2` 档(由 CORE-144 覆盖)
- 实现:`tools/xtask/tests/deploy_units.rs::dep_200_offbox_unit`

| 行 | 项 | 期望 |
|---|---|---|
| DEP-200.1 | 基本 | `User=hft-offbox`、`Slice=system.slice`、`Restart=always`、`Type=simple` |
| DEP-200.2 | 目录 | `RuntimeDirectory=hft/offbox`;`StateDirectory=hft/offbox`、`StateDirectoryMode=0750` |
| DEP-200.3 | 解释器 | `/usr/bin/python3` 运行 `/opt/hft/current-core/deploy/offbox/hft-offbox-watchdog`,不经 venv |
| DEP-200.4 | 安装范围 | `dev2-1` 有;`tokyo-cex-1` 没有 |
| DEP-200.5 | 激活 | 激活 `core`、`cold`、`sim` 任一组都不停止它(`systemctl` 替身中没有对它的 `stop`) |

### DEP-201 看门狗按 15 s 轮询,连续 60 s 失败才告警,恢复发一条通知

- 规则:[机外看门狗](README.md#机外看门狗)
- 级别:table
- 里程碑:M5
- 前置:替身时钟与 HTTP 替身;配置 `/etc/hft/offbox-watchdog.toml`:周期 15 s、判定 60 s;`maintenance_until = null`;告警替身记录 Telegram 与电话两个通道
- 实现:`deploy/offbox/tests/test_watchdog.py::dep_201_poll_and_alert`

| 行 | `/healthz` 结果序列(t = 0、15、30…) | 期望 |
|---|---|---|
| DEP-201.1 | t = 0 起连接失败,持续到 t = 60 | 轮询时刻恰为 0、15、30、45、60;t = 45 无告警;t = 60 一条 Telegram 与一次电话 |
| DEP-201.2 | 同上,继续失败到 t = 120 | 没有第二条告警 |
| DEP-201.3 | 接 DEP-201.2,t = 135 返回 200 且 `ok = true` | 一条恢复通知 |
| DEP-201.4 | t = 0–45 失败,t = 60 成功 | 没有告警,没有恢复通知 |
| DEP-201.5 | t = 0 起返回 200 但 `ok = false`(心跳过期),持续到 t = 60 | t = 60 告警(Telegram 与电话) |
| DEP-201.6 | 失败与成功交替(每 15 s 一换) | 没有告警 |

### DEP-202 维护静默期间只发 Telegram,截止后恢复电话

- 规则:[机外看门狗](README.md#机外看门狗)
- 级别:table
- 里程碑:M5
- 前置:同 DEP-201;t = 0 的 `/healthz` 返回 `maintenance_until = t + 10 min`
- 实现:`deploy/offbox/tests/test_watchdog.py::dep_202_maintenance_silence`

| 行 | 条件 | 期望 |
|---|---|---|
| DEP-202.1 | t = 15 s 起不可达 | t = 75 s 一条 Telegram,没有电话 |
| DEP-202.2 | 接 DEP-202.1,不可达持续到 t = 10 min 之后 | 截止后的第一次轮询(t = 10 min 0 s 或之后最近的一次)发一次电话 |
| DEP-202.3 | `maintenance_until = t + 45 min` | 按 t + 30 min 处理:t = 30 min 之后的告警带电话 |
| DEP-202.4 | 静默期内看门狗进程重启(`/var/lib/hft/offbox/` 中已缓存截止时刻),重启后 `/healthz` 不可达 | 告警只有 Telegram |
| DEP-202.5 | `maintenance_until = null` | 同 DEP-201.1 |
| DEP-202.6 | 缓存文件 | `/var/lib/hft/offbox/` 下的文件内容为最近一次读到的截止时刻 |

### DEP-203 看门狗配置是主机本地文件,不进归档与哈希

- 规则:[机外看门狗](README.md#机外看门狗)
- 级别:unit
- 里程碑:M5
- 前置:DEP-001 的归档
- 步骤:
  1. 在 `RELEASE.json.files` 与 `host_expect` 的期望剖面中搜索 `offbox-watchdog.toml`
- 期望:
  - `files` 中只有 `deploy/offbox/offbox-watchdog.toml.example`;没有 `etc/hft/offbox-watchdog.toml`;期望剖面中没有看门狗的目标 URL
- 实现:`deploy/tests/test_layout.py::dep_203_offbox_config_local`

### DEP-204 `hft-maint` 的参数、截断与子命令

- 规则:[机外看门狗](README.md#机外看门狗)
- 级别:table
- 里程碑:M5
- 前置:替身根;替身时钟 `2026-10-04T08:00:00Z`;每行从 `/run/hft/maint/until` 不存在开始(DEP-204.7、DEP-204.8 除外)
- 实现:`deploy/tests/test_maint.py::dep_204_hft_maint`

| 行 | 命令 | 期望 |
|---|---|---|
| DEP-204.1 | `hft-maint start 10` | 退出 0;文件内容恰为一行 `1791101400`(十进制 Unix 秒);模式 0644 |
| DEP-204.2 | `hft-maint start 30` | 内容 `1791102600` |
| DEP-204.3 | `hft-maint start 31` | 内容 `1791102600` |
| DEP-204.4 | `hft-maint start 45` | 内容 `1791102600` |
| DEP-204.5 | `hft-maint start 1` | 内容 `1791100860` |
| DEP-204.6 | `hft-maint start 0`、`start -5`、`start abc` | 每个退出 2;文件不存在 |
| DEP-204.7 | 文件存在时 `hft-maint stop` | 退出 0;文件不存在 |
| DEP-204.8 | 文件不存在时 `hft-maint stop` | 退出 0 |
| DEP-204.9 | `hft-maint start 10` 后 `hft-maint status` | stdout 含 `2026-10-04T08:10:00Z` |

### DEP-205 机外看门狗演练

- 规则:[机外看门狗](README.md#机外看门狗)
- 级别:drill
- 里程碑:M7
- 门槛:⑪
- 环境:统一升级主机(watchtower 运行,`healthz_bind` 为 ENI-A 私有 IP)与 dev2 上的 `hft-offbox-watchdog`
- 授权:`[需授权]`(停生产主机上的 watchtower、改安全组)
- 步骤:按 [release-and-deploy](../docs/runbooks/release-and-deploy.md) 的维护静默顺序与 [incident-response S](../docs/runbooks/incident-response.md#s-交易主机失联):① 停 watchtower 90 s 再启动;② 在安全组中撤掉 `healthz_port` 的放行 90 s 再恢复;③ `hft-maint start 10`,等看门狗读到一次(≤ 15 s)后重复 ①
- 通过标准:①② 各在不可达开始后 60–75 s 内收到 Telegram 与电话告警,恢复后收到一条恢复通知;③ 只收到 Telegram、没有电话;全程 dev2 护栏触发与否都不影响看门狗(`hft-offbox-watchdog` 的 `NRestarts` 不变)
- 证据:`ops-log/<UTC 日期>-DEP-205.md`(告警截图与时刻)
- 实现:`deploy/tests/host/DEP-205.sh`

### DEP-210 AMI 模板与 provision 脚本的内容

- 规则:[AMI 构建](README.md#ami-构建)、[运行期调优](README.md#运行期调优)
- 级别:table
- 里程碑:M7
- 前置:静态解析 `ami/tokyo-cex.pkr.hcl` 与 `ami/provision/*.sh`
- 实现:`deploy/tests/test_ami.py::dep_210_ami_template`

| 行 | 项 | 期望 |
|---|---|---|
| DEP-210.1 | 安装的包 | 含 `chrony`、`linux-tools-*`(含 `bpftool`、`perf`、`rtla`)、`ethtool`、`acl`、`python3.12`、`python3.12-venv`、`zstd`;AWS CLI v2 用官方 zip 安装器、写死版本并核对签名 |
| DEP-210.2 | 禁用的服务 | `snapd`、`unattended-upgrades`、`apport`、`amazon-cloudwatch-agent`、`motd-news`、`systemd-oomd`、`multipathd` 都被禁用 |
| DEP-210.3 | 移除与保留 | 移除 `ec2-instance-connect`;保留并启用 `amazon-ssm-agent` |
| DEP-210.4 | 编译工具 | 不安装 `gcc`、`clang`、`build-essential`、`cargo`、`rustc`;不放任何应用二进制 |
| DEP-210.5 | 用户与组 | 按用户表创建(同 DEP-150);写入运维 SSH 公钥的账户加入 `hft-ctl` |
| DEP-210.6 | ENA | 以 `ENA_PHC_INCLUDE=1` 构建并安装 ENA 驱动 |

### DEP-211 新 AMI 实例的构建后验证

- 规则:[AMI 构建](README.md#ami-构建)
- 级别:host
- 里程碑:M7
- 门槛:⑩
- 环境:用新 AMI 启动的 `c8g.8xlarge`(启动模板挂 ENI-B/ENI-C),装当前正式版、`host-local.toml` 为 `tokyo-cex-1`
- 授权:`[需授权]`(启动实例)
- 步骤:
  1. `hostprobe verify --expected /opt/hft/current-core/deploy/tuning/tokyo-cex-1.toml --out /run/hft/hostprofile.json`
  2. 以 SSM 会话登录一次;以运维 SSH 公钥登录一次
  3. 读 `amazon-ssm-agent` 主进程与其线程的 `Cpus_allowed_list`
  4. `hostprobe osnoise --cpus 5-17 --duration 24h`
- 通过标准:步骤 1 退出 0;步骤 2 两种登录都成功;步骤 3 的核集合 ⊆ {0、18、29};步骤 4 max < 20 µs、p99.9 < 5 µs;通过后 AMI ID 记入 ops-log
- 证据:`ops-log/<UTC 日期>-DEP-211.md`
- 实现:`deploy/tests/host/DEP-211.sh`

### DEP-220 生产归档的布局与 `RELEASE.json` 字段

- 规则:[发布归档与 releasectl](README.md#发布归档与-releasectl)
- 级别:table
- 里程碑:M3
- 前置:DEP-003.1 产出的 `hft-v1.2.3-aarch64.tar.zst`,解包
- 实现:`deploy/tests/test_release.py::dep_220_archive_and_manifest`

| 行 | 项 | 期望 |
|---|---|---|
| DEP-220.1 | 根 | 恰有一个根目录 `hft-v1.2.3-aarch64/` |
| DEP-220.2 | `bin/` | 含 `hft-core`、`journal-archiver`、`watchtower`、`reconciler`、`kill-switch`、`replayer`、`hostprobe`、`hft-secrets`、`hft-cfg`,都是 aarch64 ELF |
| DEP-220.3 | 顶层目录 | 恰为 `bin/`、`systemd/`、`polkit/`、`deploy/`、`schemas/`、`config-templates/`、`python/` 与文件 `sbom.json`、`RELEASE.json` |
| DEP-220.4 | `schemas/strategies/` | 含 `index.json` 与每个策略一个 `<kind_id>-<name>.schema.json` |
| DEP-220.5 | `python/` | 各包 wheel、`hft_py` 的 aarch64 wheel、`wheelhouse/` 与唯一的 `requirements.lock` |
| DEP-220.6 | `RELEASE.json` 的键集合 | 恰为 `version`、`channel`、`git_sha`、`ci_run_id`、`target`、`target_cpu`、`rust_version`、`profiles`、`strategies`、`schemas`、`host_expect`、`evidence`、`sbom_sha256`、`files`;`schemas` 的键为 `event`、`journal`、`control`、`refdata`、`parquet`;`evidence` 的键为 `ci_run_id`、`decoupling_check_run_id`、`nightly_run_id`、`overrides` |
| DEP-220.7 | `RELEASE.json` 的序列化 | 每个对象的键按字典序;`files` 中没有 `RELEASE.json` 自身;每项 `{path, sha256, bytes}` 与实际文件相符 |
| DEP-220.8 | `sbom_sha256` | 等于 `sbom.json` 的 SHA-256 |
| DEP-220.9 | `.sha256` 文件 | 内容为归档的 SHA-256 与文件名 |

### DEP-221 生产归档可复现

- 规则:[发布归档与 releasectl](README.md#发布归档与-releasectl)
- 级别:unit
- 里程碑:M3
- 前置:同一夹具提交,`SOURCE_DATE_EPOCH = 1759536000`
- 步骤:
  1. 在两个不同的临时目录各运行一次 `package-release.sh v1.2.3 <sha> dist`
  2. 列出 tar 条目
- 期望:
  - 两份归档逐字节相同
  - tar 条目按名字排序;每条 mtime = 1759536000;uid、gid 都为 0,以数字记录
- 实现:`deploy/tests/test_release.py::dep_221_reproducible`

### DEP-222 `fetch-release.sh` 按版本、预发布与验证包取资产

- 规则:[发布归档与 releasectl](README.md#发布归档与-releasectl)
- 级别:table
- 里程碑:M3
- 前置:GitHub API 替身(仓库 `<org>/hft`);`/etc/hft/github-release-token` 为 `root` 0600;替身记录请求
- 实现:`deploy/tests/test_release.py::dep_222_fetch`

| 行 | 命令 | 期望 |
|---|---|---|
| DEP-222.1 | `fetch-release.sh --version v1.2.3` | 下载 `hft-v1.2.3-aarch64.tar.zst` 与 `.sha256` 到 `/opt/hft/staging/`;请求带该 token |
| DEP-222.2 | `fetch-release.sh --prerelease v1.2.3-dev.4` | 下载 `hft-v1.2.3-dev.4-aarch64.tar.zst` |
| DEP-222.3 | `fetch-release.sh --version v1.2.3 --asset verify` | 下载 `hft-verify-v1.2.3-aarch64.tar.zst` 与 `.sha256` |
| DEP-222.4 | 替身对该版本返回 404 | 退出非 0 |

### DEP-223 安装成功路径不碰运行中的版本

- 规则:[发布归档与 releasectl](README.md#发布归档与-releasectl)
- 级别:unit
- 里程碑:M3
- 前置:替身根;`current-core`、`current-cold`、`current-sim` 都 → `releases/v1.2.2`;记录 `releases/v1.2.2/` 的文件树哈希(含 `venv/`);`host-local.toml` 为 `tokyo-cex-1`
- 步骤:
  1. `releasectl install --archive hft-v1.2.3-aarch64.tar.zst --sha256-file hft-v1.2.3-aarch64.tar.zst.sha256`
- 期望:
  - 退出 0;`/opt/hft/releases/v1.2.3/` 存在,属主 `root:root`,没有组或其他用户可写的条目
  - `releases/v1.2.3/venv/` 存在,由 `pip install --no-index` 从包内 wheelhouse 建成(替身记录中没有网络请求)
  - `/opt/hft/installations/v1.2.3.json` 存在,含 `version` 与归档 SHA-256
  - 三个 `current-*` 链接不变;`releases/v1.2.2/` 的文件树哈希不变;没有任何 `systemctl` 调用
- 实现:`deploy/tests/test_release.py::dep_223_install_ok`

### DEP-224 安装在每个校验失败时拒绝且不留下版本目录

- 规则:[发布归档与 releasectl](README.md#发布归档与-releasectl)
- 级别:table
- 里程碑:M3
- 前置:替身根,同 DEP-223;每行只对夹具归档做该行改动(需要时同步改 `.sha256` 使前面的校验通过)
- 实现:`deploy/tests/test_release.py::dep_224_install_rejects`

| 行 | 改动 | 期望 |
|---|---|---|
| DEP-224.1 | 归档改一个字节,`.sha256` 不变 | 退出 1 |
| DEP-224.2 | tar 中加条目 `../evil` | 退出 1 |
| DEP-224.3 | tar 中加绝对路径条目 `/etc/evil` | 退出 1 |
| DEP-224.4 | tar 中加符号链接条目 | 退出 1 |
| DEP-224.5 | tar 中加字符设备条目 | 退出 1 |
| DEP-224.6 | tar 中有两个根目录 | 退出 1 |
| DEP-224.7 | 多一个 `RELEASE.json.files` 没有的文件 | 退出 1 |
| DEP-224.8 | 少一个 `RELEASE.json.files` 列出的文件 | 退出 1 |
| DEP-224.9 | 一个文件内容改变(`files` 中哈希不变) | 退出 1 |
| DEP-224.10 | 一个文件的 `bytes` 与实际不同 | 退出 1 |
| DEP-224.11 | `bin/hft-core` 换成 x86-64 ELF | 退出 1 |
| DEP-224.12 | `bin/hft-core --version` 以 1 退出 | 退出 1 |
| DEP-224.13 | `releases/v1.2.3/` 已存在 | 退出 1;原目录不变 |
| DEP-224.14 | 离线建 venv 失败 | 退出 1;版本目录改名为 `releases/v1.2.3.failed.<unix 秒>` |
| DEP-224.15 | 每行(DEP-224.14 除外) | 没有新的 `releases/v1.2.3/`;`current-*` 链接不变 |

### DEP-225 激活成功路径

- 规则:[发布归档与 releasectl](README.md#发布归档与-releasectl)、[单元组与分组激活](README.md#单元组与分组激活)
- 级别:unit
- 里程碑:M3
- 前置:替身根,清单 `tokyo-cex-1`;`v1.2.2` 为三组当前版本,`v1.2.3` 已安装;`systemctl`、`systemd-analyze` 替身记录调用;`hostprobe verify` 替身退出 0
- 步骤:
  1. `releasectl activate --version v1.2.3 --group core`
- 期望:
  - 先重新逐文件哈希 `releases/v1.2.3/`;`previous-core` → `v1.2.2`、`current-core` → `v1.2.3`,两次切换都以"建临时链接再改名"完成
  - `systemd-analyze verify` 在切换之后调用;随后安装单元、`daemon-reload`;`disable --now` 的对象只有 `hft-core.target` 下的单元
  - 安装调优文件与 udev 规则、`egress.toml`、`50-hft-kill-switch.rules`(不装 `50-hft-dev2-guard.rules`)、IMDS 锁定与凭证代理配置;安装 `hft-verify.*`(`verify = true`);建立 `/usr/local/sbin/{releasectl,hft-maint}` 与 `/usr/local/bin/{hostprobe,hft-cfg}`,都指向 `current-core` 下
  - 最后一步调用 `hostprobe verify`;`deployments/<ts>-v1.2.3.json` 含 `version`、`git_sha`、归档 SHA-256、`current`、`previous`、`refdata_version`、`host_profile_hash`、`channel = "release"`、`group = "core"`、`manifest = "tokyo-cex-1"`、验收结果 `pending`
  - 退出 0;`current-cold` 与 `current-sim` 仍为 `v1.2.2`
- 实现:`deploy/tests/test_release.py::dep_225_activate_ok`

### DEP-226 激活在校验失败时不切换或回退切换

- 规则:[发布归档与 releasectl](README.md#发布归档与-releasectl)
- 级别:table
- 里程碑:M3
- 前置:同 DEP-225;每行只做该行改动
- 实现:`deploy/tests/test_release.py::dep_226_activate_rejects`

| 行 | 条件 | 期望 |
|---|---|---|
| DEP-226.1 | 安装后改动 `releases/v1.2.3/bin/hft-cfg` 一个字节 | 退出 1;链接不变;没有 `disable` 调用 |
| DEP-226.2 | `--version v9.9.9`(未安装) | 退出 1 |
| DEP-226.3 | `/opt/hft/current-core` 是普通目录 | 退出 1 |
| DEP-226.4 | `current-core` 指向 `/tmp/x` | 退出 1 |
| DEP-226.5 | `systemd-analyze verify` 失败 | 退出 1;`current-core` → `v1.2.2`、`previous-core` 恢复为激活前的目标 |
| DEP-226.6 | `hostprobe verify` 退出 1 | 激活退出 1;链接已切换、单元已安装、应用单元都未启动;部署记录写入该结果 |
| DEP-226.7 | `hostprobe verify` 退出 2 | 激活退出 2 |

### DEP-227 冷生产:激活不启动交易或采集单元

- 规则:[发布归档与 releasectl](README.md#发布归档与-releasectl)
- 级别:proc
- 里程碑:M5
- 前置:容器;`tokyo-cex-1` 布局,三组都在 `v1.2.2` 且全部应用单元 active
- 步骤:
  1. `releasectl activate --version v1.2.3 --group cold`
- 期望:
  - `hft-cold.target` 下每个单元为 inactive 且 `disabled`;`hft-core@prod`、`hft-core@sim` 的 `MainPID` 不变;`hft-cred-broker`、`hft-imds-lock` 的状态不变
  - 激活过程中没有任何单元从 inactive 变为 active
- 实现:`deploy/tests/container/test_groups.py::dep_227_cold_production`

### DEP-228 回滚即以旧版本激活该组

- 规则:[发布归档与 releasectl](README.md#发布归档与-releasectl)
- 级别:unit
- 里程碑:M3
- 前置:替身根;`current-core` → `v1.2.3`、`previous-core` → `v1.2.2`;`current-cold` → `v1.2.3`
- 步骤:
  1. `releasectl rollback --version v1.2.2 --group core`
- 期望:
  - `current-core` → `v1.2.2`、`previous-core` → `v1.2.3`;`current-cold` 不变;调用序列与 DEP-225 的激活相同
- 实现:`deploy/tests/test_release.py::dep_228_rollback`

### DEP-229 `releasectl verify` 在临时根走完整安装路径,不影响运行中的系统

- 规则:[发布归档与 releasectl](README.md#发布归档与-releasectl)
- 级别:table
- 里程碑:M3
- 前置:替身根;记录 `/opt/hft` 的文件树哈希;`hft-core --check` 替身的退出码按行
- 实现:`deploy/tests/test_release.py::dep_229_verify_subcommand`

| 行 | 条件 | 期望 |
|---|---|---|
| DEP-229.1 | 完好归档,`--check` 退出 0 | 退出 0;`/opt/hft` 文件树哈希不变;没有 `systemctl` 调用;临时根被删除 |
| DEP-229.2 | 归档改一个字节 | 退出 1 |
| DEP-229.3 | `hft-core --check` 退出 2 | 退出非 0 |

### DEP-230 `releasectl uninstall` 删除全部 hft 单元与规则,保留数据,不碰旧系统

- 规则:[发布归档与 releasectl](README.md#发布归档与-releasectl)
- 级别:proc
- 里程碑:M6
- 前置:容器;dev2 布局已安装、激活三组;另装一个模拟旧系统的 `trading-collector.service`(active)及其文件 `/etc/systemd/system/trading-collector.service` 与 `/opt/trading/`
- 步骤:
  1. `releasectl uninstall`
- 期望:
  - `/etc/systemd/system` 下没有 `hft-*`、`hft*.slice`、`hft-*.target` 与它们的 `.d/` 目录;没有 `/etc/polkit-1/rules.d/50-hft-*`;没有 `/etc/udev/rules.d/60-hft.rules`;`nft list table inet hft_imds` 失败(表已删除);`/usr/local/sbin`、`/usr/local/bin` 下没有指向 `/opt/hft` 的链接
  - `/opt/hft/` 与 `/var/lib/hft/` 的文件树哈希不变
  - `trading-collector.service` 仍 active,`MainPID` 不变;其单元文件与 `/opt/trading/` 的哈希不变
- 实现:`deploy/tests/container/test_groups.py::dep_230_uninstall`

### DEP-231 部署记录只新建、不覆盖

- 规则:[发布归档与 releasectl](README.md#发布归档与-releasectl)
- 级别:unit
- 里程碑:M3
- 前置:替身根;`deployments/` 中已有 `20261004T075900Z-v1.2.3.json`(`pending`);替身时钟 `2026-10-04T08:00:00Z`;`current-core` → `v1.2.3`
- 步骤:
  1. `releasectl record --version v1.2.3 --archive-sha256 <64 位十六进制> --acceptance-result passed`
  2. `releasectl record --version v1.2.2 --archive-sha256 <64 位十六进制>`(`v1.2.2` 不是当前版本)
- 期望:
  - 步骤 1:新文件 `20261004T080000Z-v1.2.3.json` 的验收结果为 `passed`;已有记录字节不变
  - 步骤 2:退出非 0,不写文件
- 实现:`deploy/tests/test_release.py::dep_231_record_immutable`

### DEP-232 新主机没有 `current-core` 时用暂存目录的脚本引导

- 规则:[发布归档与 releasectl](README.md#发布归档与-releasectl)
- 级别:unit
- 里程碑:M3
- 前置:替身根,`/opt/hft/` 下没有任何 `current-*`;归档已解包在 `/opt/hft/staging/hft-v1.2.3-aarch64/`
- 步骤:
  1. 运行 `/opt/hft/staging/hft-v1.2.3-aarch64/deploy/release/releasectl.sh install …`,再 `activate --version v1.2.3 --group core`
- 期望:
  - 两步都退出 0;`current-core` → `releases/v1.2.3`;没有 `previous-core`
- 实现:`deploy/tests/test_release.py::dep_232_bootstrap`

### DEP-235 预检的通用项

- 规则:[发布归档与 releasectl](README.md#发布归档与-releasectl)、[制品从哪里来](README.md#制品从哪里来)
- 级别:table
- 里程碑:M3
- 前置:替身根,清单 `tokyo-cex-1`;`RELEASE.json.target = "aarch64-unknown-linux-gnu"`;hft-core 的 `storage.disk_free_block_pct = 15`;ENI-A 主私有 IP(IMDS 替身)`10.0.0.5`;基线全部通过(DEP-235.1),每行只破坏一项;失败时 stdout 列出失败项名
- 实现:`deploy/tests/test_preflight.py::dep_235_common_items`

| 行 | 条件 | 期望 |
|---|---|---|
| DEP-235.1 | 基线 | `preflight.sh` 退出 0 |
| DEP-235.2 | `uname -m` = `x86_64` | 退出 1 |
| DEP-235.3 | `/proc/cpuinfo` 缺 `target_cpu` 要求的一个特性 | 退出 1 |
| DEP-235.4 | `/data` 与 `/` 的 `findmnt` 来源相同 | 退出 1 |
| DEP-235.5 | `/data` 可用空间占比 15.00% | 退出 0 |
| DEP-235.6 | 可用空间占比 14.99% | 退出 1 |
| DEP-235.7 | `chrony.service` inactive | 退出 1 |
| DEP-235.8 | `Leap status` = `Not synchronised` | 退出 1 |
| DEP-235.9 | `Last offset` = 0.005000 s | 退出 0 |
| DEP-235.10 | `Last offset` = −0.005001 s | 退出 1 |
| DEP-235.11 | `PATH` 中有 `cargo` | 退出 1 |
| DEP-235.12 | `PATH` 中有 `gcc` | 退出 1 |
| DEP-235.13 | `/opt/hft/current` 存在 | 退出 1 |
| DEP-235.14 | `/opt/hft/venv` 存在 | 退出 1 |
| DEP-235.15 | watchtower 的 `export.listen = "0.0.0.0:9464"` | 退出 1 |
| DEP-235.16 | `/run/hft/hostprofile.json` 中 ENI-A 私有 IP 为 `unreadable` | 退出 1 |
| DEP-235.17 | `hft-secrets --check --manifest /etc/hft/secrets/watchtower.toml` 退出 1 | 退出 1,失败项指出该清单 |
| DEP-235.18 | `/sys/fs/cgroup/hftbatch.slice/cpu.idle` = 0 | 退出 1 |
| DEP-235.19 | `hftbatch.slice` 位于 `/sys/fs/cgroup/hft.slice/` 之下 | 退出 1 |
| DEP-235.20 | 清单 `dev2-1`,HostProfile 中 ENI-A 私有 IP 为 `unreadable` | 该项通过 |
| DEP-235.21 | `/run/hft/hostprofile.json` 不存在 | 退出 1 |
| DEP-235.22 | `/etc/hft/ctl-principals.toml` 与生成器对当前 `ctl-operators.toml` 的输出差一个字节 | 退出 1,指出该文件 |
| DEP-235.23 | `PATH` 中有 `rustup`;另一次有 `clang`;另一次有 `cc` | 每次退出 1 |

### DEP-236 升级主机类的预检项

- 规则:[发布归档与 releasectl](README.md#发布归档与-releasectl)
- 级别:table
- 里程碑:M7
- 前置:同 DEP-235 的基线;隔离核期望 5–17
- 实现:`deploy/tests/test_preflight.py::dep_236_upgrade_class_items`

| 行 | 条件 | 期望 |
|---|---|---|
| DEP-236.1 | `/dev/hugepages2M` 未挂 hugetlbfs | 退出 1 |
| DEP-236.2 | `/sys/devices/system/cpu/isolated` = `5-16` | 退出 1 |
| DEP-236.3 | IMDS 中设备序号 1 的网卡不存在 | 退出 1 |
| DEP-236.4 | IMDS 中设备序号 2(ENI-C)不存在 | 退出 1 |
| DEP-236.5 | `10.0.1.20` 未配在 ENI-C 上 | 退出 1 |
| DEP-236.6 | `[eni_b2].enabled = true` 而设备序号 3 不存在 | 退出 1 |
| DEP-236.7 | 清单 `c4-1` 且没有设备序号 2 | 该项通过(不检查 ENI-C) |

### DEP-237 dev2 的预检项

- 规则:[发布归档与 releasectl](README.md#发布归档与-releasectl)、[dev2 护栏](README.md#dev2-护栏)
- 级别:table
- 里程碑:M3
- 前置:替身根,清单 `dev2-1`;基线全部通过;每行只破坏一项
- 实现:`deploy/tests/test_preflight.py::dep_237_dev2_items`

| 行 | 条件 | 期望 |
|---|---|---|
| DEP-237.1 | 基线 | 退出 0 |
| DEP-237.2 | `hft.slice` 的 dev2 覆盖未安装 | 退出 1 |
| DEP-237.3 | `hft-dev2-guard.service` inactive | 退出 1 |
| DEP-237.4 | `50-hft-dev2-guard.rules` 允许停 `trading-collector.service` | 退出 1 |
| DEP-237.5 | 规则允许 `start` `hft-core@prod.service` | 退出 1 |
| DEP-237.6 | `hft-treasury.service` 缺 `ExecCondition=` | 退出 1 |
| DEP-237.7 | `PATH` 中有 `gcc` 与 `cc`(旧系统的 C 工具链) | 退出 0(dev2 只查 Rust 工具链) |
| DEP-237.8 | `PATH` 中有 `cargo` | 退出 1 |
| DEP-237.9 | `PATH` 中有 `rustc` | 退出 1 |

### DEP-238 预检以每个服务用户身份核对路径表

- 规则:[发布归档与 releasectl](README.md#发布归档与-releasectl)、[systemd 单元](README.md#systemd-单元)
- 级别:table
- 里程碑:M5
- 前置:容器;`tokyo-cex-1` 布局,DEP-156 全部通过的状态;每行只破坏一项后运行 `preflight.sh`
- 实现:`deploy/tests/container/test_perm.py::dep_238_preflight_closure`

| 行 | 破坏 | 期望 |
|---|---|---|
| DEP-238.1 | 无 | 闭包项通过 |
| DEP-238.2 | `chmod 2700 /var/lib/hft/archiver/advisor-daily` | 退出 1;输出含 `hft-replay` 与该路径 |
| DEP-238.3 | `gpasswd -d hft-opsbot hft-ack` | 退出 1;输出含 `hft-opsbot` 与 `/var/lib/hft/watchtower/acks.jsonl` |
| DEP-238.4 | `setfacl -x u:hft-archive /data/prod` | 退出 1;输出含 `hft-archive` 与 `/data/prod` |
| DEP-238.5 | `chmod 0700 /dev/hugepages` | 退出 1;输出含 `hft-ks` |

### DEP-240 组链接与命令链接

- 规则:[单元组与分组激活](README.md#单元组与分组激活)、[身份、路径与 S3 前缀](README.md#身份路径与-s3-前缀)
- 级别:table
- 里程碑:M4
- 前置:替身根,清单 `dev2-1`;依次激活 `core` → `v1.2.2`、`cold` → `v1.2.1`、`sim` → `v1.2.3-dev.1`
- 实现:`deploy/tests/test_release.py::dep_240_group_links`

| 行 | 路径 | 期望 |
|---|---|---|
| DEP-240.1 | `/opt/hft/current-core`、`current-cold`、`current-sim` | 分别 → `releases/v1.2.2`、`v1.2.1`、`v1.2.3-dev.1` |
| DEP-240.2 | `/opt/hft/current`、`/opt/hft/previous` | 都不存在 |
| DEP-240.3 | `/usr/local/sbin/releasectl`、`hft-maint`、`hft-window`、`hft-dev2-guard` | 指向 `/opt/hft/current-core/deploy/…` 下的对应脚本 |
| DEP-240.4 | `/usr/local/bin/hostprobe`、`hft-cfg` | 指向 `/opt/hft/current-core/bin/` |
| DEP-240.5 | `/usr/local/bin/kill-switch`、`journal-archiver`、`replayer` | 指向 `/opt/hft/current-cold/bin/` |
| DEP-240.6 | `/usr/local/bin/hft-pipeline`、`hft-treasury` | 指向 `/opt/hft/current-cold/venv/bin/` |
| DEP-240.7 | `hft-core@sim` 的 `paths.release_manifest` 生效值 | `/opt/hft/current-sim/RELEASE.json`(其 `channel = "dev"`) |

### DEP-241 激活某组只停该组 target 下的单元

- 规则:[单元组与分组激活](README.md#单元组与分组激活)
- 级别:table
- 里程碑:M5
- 前置:容器;dev2 布局,三组都在 `v1.2.2`,`hft-core@sim`、冷进程、护栏、看门狗、凭证代理都 active;每行激活一个组到 `v1.2.3`,读各单元 `MainPID`
- 实现:`deploy/tests/container/test_groups.py::dep_241_group_scope`

| 行 | 激活的组 | 期望 |
|---|---|---|
| DEP-241.1 | `core` | 只有 `hft-core.target` 下的单元变为 inactive;`hft-core@sim`、冷进程的 `MainPID` 不变 |
| DEP-241.2 | `sim` | 只有 `hft-core@sim` 变为 inactive |
| DEP-241.3 | `cold` | `hft-cold.target` 下的单元 inactive;两个 hft-core 实例不变 |
| DEP-241.4 | 每一行 | `hft-dev2-guard`、`hft-offbox-watchdog`、`hft-cred-broker` 的 `MainPID` 不变 |

### DEP-242 主机级文件只随 core 组安装

- 规则:[单元组与分组激活](README.md#单元组与分组激活)
- 级别:table
- 里程碑:M4
- 前置:替身根,清单 `tokyo-cex-1`;记录 `/etc/sysctl.d/90-hft.conf`、`/etc/udev/rules.d/60-hft.rules`、`/etc/hft/egress.toml`、`/etc/polkit-1/rules.d/50-hft-kill-switch.rules` 与凭证代理的消费者配置的 mtime 与哈希
- 实现:`deploy/tests/test_release.py::dep_242_host_files_core_only`

| 行 | 激活 | 期望 |
|---|---|---|
| DEP-242.1 | `cold` 到新版本 | 上述文件与凭证代理的消费者配置的 mtime 与哈希都不变 |
| DEP-242.2 | `sim` 到新版本 | 同上 |
| DEP-242.3 | `core` 到新版本(其中 `egress.toml` 有变化) | `egress.toml` 更新为新版本的内容 |

### DEP-245 升级主机类的期望剖面

- 规则:[HostProfile](README.md#hostprofile)
- 级别:table
- 里程碑:M3
- 前置:夹具 `tokyo-cex-1` 生成的 `deploy/tuning/tokyo-cex-1.toml`;hft-rt 的 ENA PHC 读取上限取 125 次/s
- 实现:`tools/xtask/tests/deploy_profile.rs::dep_245_upgrade_profile`

| 行 | 项 | 期望(值;级别) |
|---|---|---|
| DEP-245.1 | 隔离核集合 / `nohz_full` | `5-17` / `5`;`required` |
| DEP-245.2 | `rcu_nocbs` | 包含 1 与 5–17;`required` |
| DEP-245.3 | 大页数(1 GiB、2 MiB)/ THP | hft-core 内存表的值 / `madvise`;`required` |
| DEP-245.4 | ENA 驱动 / ENI-B、ENI-C 的设备序号 | `2.17.3` / 1、2;`required` |
| DEP-245.5 | 合并与卸载 | `adaptive-rx off`、`rx-usecs 0`、`tx-usecs 0`;`gro`、`lro`、`tso`、`gso`、`sg` 关;`required` |
| DEP-245.6 | `net.core.busy_poll`、`net.core.busy_read`、ENI-B 的 `napi_defer_hard_irqs`、`gro_flush_timeout` | 都是 0;`required` |
| DEP-245.7 | IRQ 落点 | ENI-B 全部队列 → 1;ENI-A → 0;ENI-C → 22;`required` |
| DEP-245.8 | managed IRQ 的 `effective_affinity` / IRQ 专用核上的可运行任务 | 不含 1 与 22 / 无;`required` |
| DEP-245.9 | XPS | 与 `irq-plan.json` 逐队列相同;`required` |
| DEP-245.10 | chrony | 同步且误差界 < 40 µs;`required` |
| DEP-245.11 | `hft-sim.slice` 的 `AllowedCPUs` | 与 5–17、1、22 不相交;`required` |
| DEP-245.12 | 出口地址 | `10.0.1.10` 在设备序号 1、`10.0.1.20` 在设备序号 2,各自在 IMDS `ipv4-associations` 中有公网关联;`required` |
| DEP-245.13 | IMDS | `connect4` 上 `hft_imds4`、`connect6` 上 `hft_imds6`;`nf_conntrack` 未加载;`required` |
| DEP-245.14 | `hftbatch.slice` | 在根级、`cpu.idle = 1`;`required` |
| DEP-245.15 | PHC 读取 | chrony 与 hft-core 合计 ≤ 125 次/s;`required` |
| DEP-245.16 | keepawake 线程登记表 | 空列表;`required`(IRQ 专用核上任何线程都按占用报告) |
| DEP-245.17 | 网卡 MAC | 不出现在期望剖面中(运行期读到的 MAC 由 hostprobe 记为 `advisory`) |
| DEP-245.18 | resctrl(`enabled = false`) | `advisory` |

### DEP-246 `c4` 的期望剖面

- 规则:[HostProfile](README.md#hostprofile)
- 级别:table
- 里程碑:M3
- 前置:夹具 `c4-1` 生成的期望剖面
- 实现:`tools/xtask/tests/deploy_profile.rs::dep_246_c4_profile`

| 行 | 项 | 期望 |
|---|---|---|
| DEP-246.1 | `required` 的项集合 | 恰为 busy poll 四项、"全部 IRQ 在核 0"、chrony 同步、IMDS 两项(BPF 程序、`nf_conntrack` 未加载) |
| DEP-246.2 | 隔离核 `1-3`、`nohz_full`、`rcu_nocbs`、大页 | 都有期望值,标 `advisory` |
| DEP-246.3 | IRQ 专用核、ENI-C、ENI-B2、XPS | 都没有对应项 |

### DEP-247 dev2 的期望剖面

- 规则:[HostProfile](README.md#hostprofile)
- 级别:table
- 里程碑:M3
- 前置:夹具 `dev2-1` 与 hft-core `dev2` 档生成的期望剖面
- 实现:`tools/xtask/tests/deploy_profile.rs::dep_247_dev2_profile`

| 行 | 项 | 期望(级别 `required`) |
|---|---|---|
| DEP-247.1 | slice 上限 | `hft.slice`、`hft-core.slice`、`hftbatch.slice` 的各上限等于 hft-core `dev2` 档;`MemorySwapMax = 0` |
| DEP-247.2 | `hftbatch.slice` | 在根级、`cpu.idle = 1`、没有 `CPUQuota` |
| DEP-247.3 | 护栏 | `hft-dev2-guard.service` 的 `ActiveState = active` |
| DEP-247.4 | IMDS | 表 `inet hft_imds` 已加载,含 DEP-171 的两条规则 |
| DEP-247.5 | 出口地址 | `172.31.10.5` 与 `172.31.10.6` 配在 `ens5`;`172.31.10.6` 有公网关联;`172.31.10.5` 不检查公网关联 |
| DEP-247.6 | 其余项 | 都是 `advisory` |

### DEP-248 `pending_reboot` 的判定

- 规则:[HostProfile](README.md#hostprofile)
- 级别:table
- 里程碑:M3
- 前置:`tokyo-cex-1` 的 GRUB 片段(DEP-020 的参数集合);`/proc/cmdline` 按行;判定函数取自期望剖面的生成实现
- 实现:`tools/xtask/tests/deploy_profile.rs::dep_248_pending_reboot`

| 行 | `/proc/cmdline` | 期望 |
|---|---|---|
| DEP-248.1 | 片段全部参数,另有 `BOOT_IMAGE=/vmlinuz root=UUID=x ro` | `false` |
| DEP-248.2 | 片段全部参数,顺序打乱 | `false` |
| DEP-248.3 | `isolcpus=domain,managed_irq,5-16`,其余相同 | `true` |
| DEP-248.4 | 缺 `rcu_nocb_poll` | `true` |
| DEP-248.5 | 清单 `dev2-1`(没有片段),任意 cmdline | `false` |

### DEP-249 升级主机上手改 required 项与未重启的 GRUB 改动

- 规则:[HostProfile](README.md#hostprofile)
- 级别:host
- 里程碑:M7
- 环境:统一升级主机,M7 测量窗口之外
- 授权:`[需授权]`(改生产主机的内核参数,需重启恢复)
- 步骤:
  1. `hft-core@prod` 停止时把 2 MiB 大页数减 1(写 `/sys/kernel/mm/hugepages/hugepages-2048kB/nr_hugepages`),运行 `hostprobe verify`;恢复
  2. 激活一个只改了 GRUB 片段的版本,不重启,运行 `hostprobe verify` 并读 `hostprofile.json`
  3. 重启后再运行一次
- 通过标准:步骤 1 退出 1,stderr 有大页的键与 `required`;步骤 2 `pending_reboot = true`;步骤 3 `pending_reboot = false`、退出 0
- 证据:`ops-log/<UTC 日期>-DEP-249.md`
- 实现:`deploy/tests/host/DEP-249.sh`

### DEP-250 生成器是纯函数,生成物与已提交的一致

- 规则:[实现说明](README.md#实现说明)
- 级别:unit
- 里程碑:M3
- 前置:夹具全部清单;已提交的生成物
- 步骤:
  1. 以 `TZ=UTC` 与 `TZ=Asia/Tokyo`、不同的当前时刻与不同的工作目录各运行一次 `cargo xtask gen-host`
- 期望:
  - 两次输出逐字节相同,且与已提交的生成物相同;运行期间没有读取 `deploy/hosts/`、hft-core 主机类档与 `deploy/` 静态配置之外的文件(文件访问记录)
- 实现:`tools/xtask/tests/deploy_gen.rs::dep_250_pure_generator`

### DEP-251 守护、看门狗、`hft-shm-release` 与凭证代理只用 Python 标准库

- 规则:[实现说明](README.md#实现说明)
- 级别:unit
- 里程碑:M3
- 前置:`dev2/hft-dev2-guard`、`offbox/hft-offbox-watchdog`、`bin/hft-shm-release`、`creds/hft-cred-broker` 的源码;Python 3.12 的 `sys.stdlib_module_names`
- 步骤:
  1. 解析每个文件的全部 `import` 与 `from … import`;读 shebang
- 期望:
  - 每个导入的顶层模块都在 `sys.stdlib_module_names` 中
  - shebang 都是 `#!/usr/bin/python3`;没有 `/opt/hft/current-cold/venv` 的引用
  - `hft-cred-broker` 只以子进程调用 `aws`(AWS CLI v2),不导入 `boto3`
- 实现:`deploy/tests/test_layout.py::dep_251_stdlib_only`

### DEP-252 `hft-sim-alternate` 的配置校验与按小时切换

- 规则:[实现说明](README.md#实现说明)
- 级别:table
- 里程碑:M7
- 前置:替身时钟与 `systemctl` 替身;`/etc/hft/measure.toml` 默认为 `seed = 42`、`start = "2026-11-02T00:00:00Z"`、`segment_days = [7]`、`funding_hours_utc = [0, 8, 16]`;每行只改该行的项;`hft-core@sim` 初始 active
- 实现:`deploy/tests/test_sim_alternate.py::dep_252_config_and_switch`

| 行 | 条件 | 期望 |
|---|---|---|
| DEP-252.1 | `start = "2026-11-02T01:00:00Z"`(奇数整点) | 退出 2;没有 `systemctl` 调用 |
| DEP-252.2 | `start = "2026-11-02T00:30:00Z"` | 退出 2 |
| DEP-252.3 | 缺 `segment_days` | 退出 2 |
| DEP-252.4 | 当前时刻 `2026-11-01T23:30:00Z`(早于 `start`),sim inactive | `systemctl start hft-core@sim.service` |
| DEP-252.5 | 当前时刻 `2026-11-09T00:30:00Z`(晚于全部段),sim inactive | `systemctl start hft-core@sim.service` |
| DEP-252.6 | 当前时刻在某块的第一小时,该块按排程"先关",sim active | `systemctl stop hft-core@sim.service` |
| DEP-252.7 | 同一小时内第二次调用 | 没有 `start`/`stop` 调用 |
| DEP-252.8 | 当前时刻在块的第二小时,该块"先关",sim inactive | `systemctl start hft-core@sim.service` |

### DEP-253 `hft-sim-alternate` 的排程按段配平且可复现

- 规则:[实现说明](README.md#实现说明)
- 级别:prop
- 里程碑:M7
- 性质:对同一配置,排程逐块相同;每段内资金费块(起点为 00、08、16 UTC 的块)与普通块各自"先开"与"先关"的块数相差 ≤ 1,且先开块数 = ⌈n/2⌉;在 `segment_days` 末尾追加一段不改变已有各段的排程;7 天的一段有 21 个资金费块与 63 个普通块,先开块数分别为 11 与 32
- 生成:`seed` ∈ u64 全域;`segment_days` 从 {`[7]`、`[7, 2]`、`[7, 2, 2]`、`[8]`、`[14]`} 抽取;`start` 为 2026–2030 年间任一偶数 UTC 整点
- 参考:按段与类别计数的断言;同配置两次计算的逐块比较
- 例数:PR 500
- 实现:`deploy/tests/test_sim_alternate.py::dep_253_schedule_balanced`

### DEP-260 升级主机隔离核的 osnoise

- 规则:[延迟与资源契约](README.md#延迟与资源契约)
- 级别:host
- 里程碑:M7
- 门槛:⑩
- 环境:统一升级主机,`hft-core@prod` 与 `hft-core@sim` 运行,`hft-verify` 不运行
- 授权:`[需授权]`(24 h 占用生产主机的跟踪设施)
- 步骤:
  1. `hostprobe osnoise --cpus 5-17 --duration 24h`
- 通过标准:每个隔离核 max < 20 µs、p99.9 < 5 µs
- 证据:`ops-log/<UTC 日期>-DEP-260.md`
- 实现:`deploy/tests/host/DEP-260.sh`

### DEP-261 到 Binance 的 RTT 改善与抖动

- 规则:[延迟与资源契约](README.md#延迟与资源契约)
- 级别:host
- 里程碑:M7
- 门槛:⑩
- 环境:统一升级主机与 dev2(现状基线),同一 24 h
- 授权:无(只读测量)
- 步骤:
  1. 两台主机各以 `ss -ti` 采集到 Binance 下单端点连接的 `minrtt` 与 RTT 分布 24 h
- 通过标准:升级主机的 `minrtt` 比 dev2 低 ≥ 0.5 ms;升级主机 RTT 的抖动(p99 − p50)< 100 µs
- 证据:`ops-log/<UTC 日期>-DEP-261.md`
- 实现:`deploy/tests/host/DEP-261.sh`

### DEP-262 chrony 的 PHC 误差界

- 规则:[延迟与资源契约](README.md#延迟与资源契约)
- 级别:host
- 里程碑:M7
- 门槛:⑩
- 环境:统一升级主机
- 授权:无(只读)
- 步骤:
  1. 每 10 s 运行一次 `hostprobe collect`(只取时钟一类),持续 24 h
- 通过标准:参考源为 PHC;每个采样中 hostprobe 读到的 chrony 误差界 < 40 µs
- 证据:`ops-log/<UTC 日期>-DEP-262.md`
- 实现:`deploy/tests/host/DEP-262.sh`

### DEP-263 冷启动开机到 hostprobe 通过不超过 3 min

- 规则:[延迟与资源契约](README.md#延迟与资源契约)
- 级别:host
- 里程碑:M7
- 环境:统一升级主机,M7 测量窗口之外
- 授权:`[需授权]`(重启生产主机)
- 步骤:
  1. 重启;读 `systemd-analyze` 的开机时刻与 `hft-hostprobe.service` 的 `ExecMainExitTimestampMonotonic` 与退出码
- 通过标准:从内核启动到 `hft-hostprobe.service` 以 0 结束 < 180 s
- 证据:`ops-log/<UTC 日期>-DEP-263.md`
- 实现:`deploy/tests/host/DEP-263.sh`

### DEP-264 sim 线程不落在隔离核与 IRQ 专用核,sim 网卡 IRQ 不落在 prod 的核

- 规则:[延迟与资源契约](README.md#延迟与资源契约)、[模拟实例(sim)](README.md#模拟实例sim)
- 级别:host
- 里程碑:M7
- 门槛:⑩
- 环境:统一升级主机,门槛 ⑩ 的测量窗口内(两个实例运行)
- 授权:无(只读采样)
- 步骤:
  1. 每 1 s 读 `hft-core@sim` 全部线程的 `/proc/<pid>/task/*/stat` 的 `processor` 字段;每 1 min 读 `/proc/interrupts` 中 ENI-C 各队列的行
- 通过标准:sim 线程出现在核 1、5–17、22、28 上的次数为 0;ENI-C 队列的中断计数只在核 22 增长
- 证据:`ops-log/<UTC 日期>-DEP-264.md`
- 实现:`deploy/tests/host/DEP-264.sh`

### DEP-265 清单键 replay_verify 决定是否安装 hft-replay-verify

- 规则:[主机清单](README.md#主机清单)、[dev2 护栏](README.md#dev2-护栏)
- 级别:table
- 里程碑:M5
- 前置:夹具清单 `dev2-1`、`tokyo-cex-1`,每行只改所列键后运行生成器;安装与预检在替身根上
- 实现:`deploy/tests/test_gen.py::dep_265_replay_verify_key`

| 行 | 清单与改动 | 期望 |
|---|---|---|
| DEP-265.1 | `dev2-1`,不写 `replay_verify` | 生成物含 `hft-replay-verify.service` 与 `.timer`(缺省 true) |
| DEP-265.2 | `dev2-1`,`replay_verify = false`、`verify = false`(退路 (a) 的原 dev2) | 生成物不含 `hft-replay-verify.*` 与 `hft-verify.*`;`hft-verify.service`、`hft-journal-archiver.service`、`hft-pipeline.service` 的 `hft-unit-idle` 参数不再列 `hft-replay-verify.service` |
| DEP-265.3 | 接 DEP-265.2,安装后运行 `preflight.sh` | `hft-replay-verify.service`、`.timer` 的 `LoadState = not-found`,该项通过 |
| DEP-265.4 | 接 DEP-265.2,替身根上残留 `hft-replay-verify.timer` | `preflight.sh` 退出 1,指出该单元 |
| DEP-265.5 | `dev2-1`,`replay_verify = "no"` | 生成失败(类型不符),退出 1 |
| DEP-265.6 | 退路 (a) 新主机清单(`instances = []`、`verify = true`、`replay_verify = true`) | 生成物恰含 `hft-verify.*`、`hft-replay-verify.*`、`hft-journal-archiver.service`、`hftbatch.slice`、IMDS 锁定与凭证代理 |

### DEP-266 ctl 签发者表由模板与主机本地人员文件生成

- 规则:[实现说明](README.md#实现说明)、[身份、路径与 S3 前缀](README.md#身份路径与-s3-前缀)
- 级别:table
- 里程碑:M5
- 前置:`config-templates/ctl-principals.toml.tmpl`(服务 uid `hft-opsbot`、`hft-pipeline`、`hft-recon` 与各自 issuer 形式);主机本地 `/etc/hft/ctl-operators.toml` 按行;生成后以 hft-ctl 的 `PrincipalTable::load` 读回
- 实现:`deploy/tests/test_gen.py::dep_266_ctl_principals`

| 行 | `ctl-operators.toml` | 期望 |
|---|---|---|
| DEP-266.1 | 运维 `alice`(Admin)、Telegram 用户 `1001`(Viewer)、`2001`(Trader) | `/etc/hft/ctl-principals.toml` 生成,`root:root` 0644;`load` 成功;`cli:alice` → Admin,`tg:1001` → Viewer,`tg:2001` → Trader;三个服务 uid 与模板相同 |
| DEP-266.2 | 同一输入生成两次 | 两次逐字节相同 |
| DEP-266.3 | 文件不存在 | 生成失败,退出 1,不写 `ctl-principals.toml` |
| DEP-266.4 | `alice` 的角色写 `Root`(不存在的角色) | 生成失败,退出 1 |
| DEP-266.5 | 运维与服务 uid 同名(`hft-opsbot`) | 生成失败,退出 1 |
| DEP-266.6 | 生成后以 `hft`、`hft-sim` 身份读 / 写该文件 | 读允许;写拒绝(`EACCES`) |

### DEP-267 冷进程配置模板:watchtower 的规则段与 reconciler 的生成键

- 规则:[实现说明](README.md#实现说明)
- 级别:table
- 里程碑:M5
- 前置:以夹具 `tokyo-cex-1`(`[egress].prod` 私有 IP `10.0.1.10`)渲染 `config-templates/` 的 `watchtower.toml` 与 `reconciler.toml`;hft-telemetry `rules::REGISTRY` 的名字集合 N(夹具版本含 `ArchiverAlert`、`PipelineAlert`)
- 实现:`deploy/tests/test_config_templates.py::dep_267_cold_templates`

| 行 | 文件 / 键 | 期望 |
|---|---|---|
| DEP-267.1 | `watchtower.toml` / `rules.<name>` 的名字集合 | 等于 N;每段有 `enabled` |
| DEP-267.2 | `watchtower.toml` / `instances` | `prod`、`sim` 各一项,`hft_core_config` 分别为 `/etc/hft/hft-core-prod.toml`、`/etc/hft/hft-core-sim.toml` |
| DEP-267.3 | `watchtower.toml` / `paths.archiver_alerts` | `/var/lib/hft/archiver/alerts.jsonl` |
| DEP-267.4 | `watchtower.toml` / 主机剖面项 | 没有写 IP 地址或网卡名的键 |
| DEP-267.5 | `reconciler.toml` / `egress_bind` | `10.0.1.10` |
| DEP-267.6 | `reconciler.toml` / `refdata_blob`、`http.max_rps` | `refdata_blob` 为本机 prod 实例当前 refdata 文件的路径;`http.max_rps` 对 `accounts` 用到的每个 venue 各一项 |
| DEP-267.7 | 以渲染结果运行 `reconciler --check` 与 `watchtower --check` | 都退出 0 |
| DEP-267.8 | `watchtower.toml` / `paths.pipeline_state` | `/var/lib/hft/pipeline`(`PipelineAlert` 读其下 `<实例>/status.json`) |

### DEP-268 回放校验单元以 replayer 的每日模式运行

- 规则:[systemd 单元](README.md#systemd-单元)
- 级别:unit
- 里程碑:M4
- 前置:夹具 `tokyo-cex-1` 的生成物;`<cold_root>` = `/data`
- 步骤:
  1. 读 `hft-replay-verify.service` 的 `ExecStart=`
- 期望:
  - 每个本机实例(`prod`、`sim`)恰一条命令:`/opt/hft/current-cold/bin/replayer verify --journal /data/<instance>/seq --instance <instance> --advice recompute --model-checkpoint /var/lib/hft/archiver/advisor-daily/<instance> --report-dir /var/lib/hft/replay-verify/<instance>`
  - 命令中没有 `--report`、`--from-seq`、`--to-seq`
- 实现:`deploy/tests/test_units.py::dep_268_replay_verify_daily_mode`

### DEP-269 每个主机类有一份测试清单且生成器对它们无差异

- 规则:[目录布局](README.md#目录布局)、[实现说明](README.md#实现说明)
- 级别:unit
- 里程碑:M3
- 前置:仓库中的 `deploy/hosts/test-*.toml` 与已提交的生成物
- 步骤:
  1. 列出测试清单的 `class`
  2. 对每份运行 `cargo xtask gen-host --host <名> --check`
- 期望:
  - 步骤 1 的集合恰为 {`dev2`、`c4`、`c8`、`c16`、`c32`、`metal`},每类一份
  - 步骤 2 每次退出 0;`deploy/dev2/tests/` 存在且含护栏守护与 `hft-shm-release` 的测试入口
- 实现:`deploy/tests/test_gen.py::dep_269_test_manifests`

## 验收

- **完成标准**:里程碑已到的 `unit`/`table`/`prop`/`proc` 用例全部有同编号的实现并通过,`cargo xtask test-trace` 三张清单为空([docs/tests §6](../docs/tests/README.md#6-与代码的对应))。`tools/xtask/tests/deploy_*.rs` 与 `deploy/tests/test_*.py` 每个 PR 运行;`deploy/tests/container/` 与 `deploy/dev2/tests/container/` 的 `proc` 用例在改动 `deploy/` 或 hft-core 线程计划的 PR 上运行,并在每夜任务全量运行。
- **属性测试例数**:DEP-253 PR 500 例,每夜 50 000 例。
- **主机与演练的证据**(不进 CI,按 `证据` 字段归档,上线门槛按编号核对,门槛定义见 [testing §8](../docs/testing.md#8-上线门槛)):
  - M3:DEP-005(M3 末探针);dev2 护栏的标定演练由 CORE-216 给出。
  - M5:DEP-159、DEP-181、DEP-182。
  - M6:DEP-075。
  - M7:DEP-017、DEP-044、DEP-211、DEP-249,以及门槛 ⑩ 的主机层证据 DEP-260–DEP-264;门槛 ⑪ 的证据 DEP-205。
  - M8:DEP-051(切 Live 前)。
- **合并到 `main` 前**:`cargo xtask gen-host --check` 与 DEP-042(shellcheck)通过;改动单元、路径或组的 PR 同时更新 DEP-150–DEP-157 的对应行。
