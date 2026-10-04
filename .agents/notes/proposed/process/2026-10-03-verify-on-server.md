# Agent Note: 累计门槛 ③ fuzz 与 ④ DST 在服务器上跑 CI 构建的验证包

Status: proposed

> **第八轮第五次修订(2026-10-03)**:收窄第 2–4 条——③ 按子进程 CPU 时间计量(每目标 ≥ 2 CPU·h),跨版本只累计 `path_digest` 与候选版本相同的运行(取代"旧验证包累计的量只算在旧版本上"),SIGABRT/SIGSEGV 一律失败;dev2 批处理不设 CPU 配额、护栏 tripped 标志挡住定时器,见 [dev2-guard-system-slice-psi](2026-10-03-dev2-guard-system-slice-psi.md);M8 起正式主机的清单 `verify = false`,不再用 `mask`。规则归 testing §6.1;依据 [round8e 简报](../../../../design/_work/round8e-spec-brief.md) E10、E12。

> **第八轮第六次修订(2026-10-03)**:事实同步——第 4 条的候选版本补跑按子进程 CPU 时间计(每个目标 ≥ 15 CPU·min),Risks 第 2 条"用旧验证包累计的量只算在旧版本上"已由第五次修订的 `path_digest` 规则取代;第 2 条的批处理改在根级 `hftbatch.slice`(`CPUWeight=idle`),见 [dev2-root-batch-slice](2026-10-03-dev2-root-batch-slice.md);M8 起靠同一发布里的两份主机清单切换 `verify`([deploy 主机清单](../../../../deploy/README.md#主机清单))。

## Problem

上线门槛 ③(每个 core 解码器累计 fuzz ≥ 2 h)与 ④(DST 新种子累计 ≥ 200,000)原定在 CI 每夜累计。免费额度下每夜 20 min 扣掉构建只剩 5–12 min,fuzz 需 60–144 夜、DST 需 28–465 夜,任一失败清零,现实 4–8 个月;累计账本作为 artifact 还要占 500 MB 的存储配额([V4 评审](../../../../design/_work/review-2026-10-03-M0c-V4运行与交付.md) P0-1)。评审推荐在临时 Spot 实例上集中跑,用户没有采纳这一项,决定改在服务器上运行([round8-decisions](../../../../design/_work/round8-decisions.md) R8-10 第 1 条)。

## Proposal

落地规格见 [round8d 简报](../../../../design/_work/round8d-spec-brief.md) D2;工作流归 [development](../../../../docs/development.md#验证包工作流verify-bundleyml),包与单元归 [deploy](../../../../deploy/README.md#验证包与-hft-verify),驱动归 [tools](../../../../tools/README.md#hft-verify),账本规则归 [testing §6.1](../../../../docs/testing.md#61-服务器上的累计运行)。

1. **服务器仍不编译**:CI 在 arm runner 上构建验证包(每个 core 解码器的 ASan libFuzzer 二进制、`ci` 剖面的 `hft-dst`、驱动 `hft-verify`、初始语料),作为正式版 Release 的资产。
2. 服务器以 `releasectl install-verify` 安装,`hft-verify.service` 在批处理 slice 内以最低优先级(`Nice=19`、idle 调度、最先被 OOM 杀)运行,不联网;dev2 护栏一级动作最先停它;测试网联调窗口与每日回放运行时不启动。
3. **在哪跑**:dev2 常态运行;升级主机只在 M7 且门槛 ⑩ 的 7 天测量窗口之外运行;M8 起升级主机不再运行。
4. 账本规则:只有失败清零,漏跑不清零;"≥ 7 次运行,分布在 ≥ 7 个不同 UTC 日";账本与运行记录丢失按本机与 S3 的运行记录重建。开正式实盘所用的版本必须补跑(每个目标 ≥ 15 min、DST ≥ 20,000 种子、零失败)。
5. CI 每夜只做回归冒烟,不计入 ③④。

## Alternatives considered

**临时 Spot 实例集中跑满(评审推荐)。** 1–3 h、< $3,但每个正式版候选都要有人开关 EC2;用户选择让已有服务器在空闲时持续累计。

**留在 CI 每夜累计。** 免费额度下需要 4–8 个月,额度紧张时还会挤占 PR 与发布。

**在服务器上编译 fuzz 目标。** 违反"服务器不编译、不装工具链"的发布纪律([deploy/AGENTS.md](../../../../deploy/AGENTS.md))。

## Acceptance criteria

- dev2 上 `hft-verify` 运行期间,旧系统的 PSI 与成交延迟分布与不运行时相比无可测劣化(护栏对照);护栏一级动作首先停止的单元是 `hft-verify`。
- 账本在 3–8 天内达到 ③ 的 fuzz 累计(估算:batch slice 约 0.3 核,fuzz 12 CPU·h、DST 6–39 CPU·h;以实测为准)。
- `hft-verify ledger rebuild` 在删除本机账本后从运行记录重建出相同的累计值(演练)。
- 开正式实盘的版本有补跑记录。

## Risks

- dev2 资源本来就紧:`hft-verify` 的内存行由 hft-core 内存表给出,超出时先缩预算;护栏一周触发 ≥ 2 次按 dev2 退路处理。
- 服务器上的累计依赖验证包与生产二进制同源(同一 SHA);用旧验证包累计的量只算在旧版本上,规则归 testing §6。
- ASan 二进制在 Graviton 上的内存开销与稳定性待实测。
