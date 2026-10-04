# Agent Note: 只构建 aarch64(neoverse-v1),x86 是可选的独立剖面

Status: proposed

> **第八轮(2026-10-02)**:补充——CI(aarch64 Linux)是唯一构建与测试环境,本地只写代码、不用 WSL2,服务器只安装 CI 制品;见 [development](../../../../docs/development.md)。

> **第八轮补充(2026-10-03)**:收窄第 3 条与风险——质量门在 `ubuntu-latest`(x86_64)运行,aarch64 只在按 tag 的发布构建、每夜 arm 专项与标签一致性集上原生运行;不再以自托管 Graviton runner 为退路;见 [ci-free-hosted](../architecture/2026-10-03-ci-free-hosted.md)。

> **第八轮第四次修订(2026-10-03)**:收窄——发布构建剖面拆为 `release`(thin LTO,正式版与开发构建的大多数二进制)与 `release-hot`(fat LTO,只用于正式版的 `hft-core` 与 `kill-switch`),`ci` 剖面关闭 LTO;aarch64 一致性集只在正式版运行;`ubuntu-24.04-arm` 在个人私有仓库可用已证实,在组织私有仓库 M1 第一周核实;见 [ci-org-and-profiles](../architecture/2026-10-03-ci-org-and-profiles.md)。

## Problem

早期设计按 c7i.metal(x86,Xeon 8488C)固定 `target-cpu=sapphirerapids`,理由是可选的裸金属 Onload 形态没有 aarch64 版本,一开始就用 x86 可以避免二次迁移([03 §5](../../../../design/03-网络与传输层.md))。第四轮把主机改成按需选择的主机类:`dev2`(现有 c7g.large,Graviton3)、`c8`(c8g.2xlarge,Graviton4),`metal` 为可选。于是实际运行的全是 Graviton,而规格仍写着 x86 发布目标——全局评审(耦合评审 C1)指出这会让发布产物在生产机上根本跑不起来,且 CI、hft-py wheel、hft-rt 主机检查各自假设了不同的架构。

## Proposal

第六轮裁决(2026-10-01 全局评审,主控记录)A1;取值的唯一归属是 [development#目标-cpu](../../../../docs/development.md#目标-cpu):

1. 唯一发布目标:`aarch64-unknown-linux-gnu` + `target-cpu=neoverse-v1`。Neoverse-V1 是 Graviton3 与 Graviton4 的公共基线,`dev2` 与 `c8` 跑**同一个二进制**,升级主机类只换配置与调优。
2. x86 只作为可选的 `metal` 剖面(触发条件:LeadLag 实测证明 A 类需要更低延迟,或需要裸金属 + Onload):独立构建、独立基线;CI 只对 x86_64 做 `cargo check` 以保持可编译。
3. CI 主矩阵跑在托管 arm64 runner;`hft-py` 出 aarch64 wheel;`hft-rt` 以 aarch64 为主路径(`CNTVCT_EL0`),主机检查项按 ISA 分表。
4. 换架构 = 版本变化:按恢复规则只能从 Drain 快照冷启动([recovery-epoch-and-boot-rules](../architecture/2026-10-01-recovery-epoch-and-boot-rules.md))。
5. 其他文档(hot-path-rules B2、benchmarks、deploy、hft-rt)只链接 development,不复写取值。

## Alternatives considered

**保持 x86 发布目标,一开始就上 c7i.metal。** 主机计划是先在现有 2 核主机、再到 c8;为一个可选、条件触发的 Onload 形态让全部时间付 metal 的费用(约 $3.9k/月按需)不划算。

**同时发布 aarch64 与 x86 两套产物。** 两套基线、两套基准、两套主机检查,CI 时间翻倍;迁移范围内没有任何 x86 生产主机。

**按各自主机族分别优化(c7g 用 neoverse-v1,c8g 用 neoverse-v2)。** 失去"同一二进制"的性质,升级主机类变成重新构建与重新验证;V2 相对 V1 的收益未经测量。

## Acceptance criteria

- 发布工作流 `release-aarch64.yml` 产出的归档在 `dev2` 与 `c8` 上启动自检都通过(CPU 特性检查)。
- CI 的 arm64 runner 跑完整 `cargo xtask ci`;托管 runner 的 CPU 特性是否满足 `neoverse-v1` 在一周内核实,不满足时 `ci` 剖面退到 `generic` 并记录。
- 仓库中除 development 外不再出现具体的 `target-cpu` 取值(`doc-check` 检查)。

## Risks

- 若真走 Onload/裸金属,需要一次 x86 迁移:那时作为可选剖面立项,同一份 journal 经 Drain 快照迁移;SIMD 解析路径需要两套实现(sonic-rs 已支持两种 ISA)。
- 托管 arm64 runner 的可用性与价格变化:可退到自托管 Graviton runner。
- Graviton 上的 PHC/硬件时间戳与 resctrl(MPAM)能力待核实([tools](../../../../tools/README.md#hostprobe))。
