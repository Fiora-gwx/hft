# Agent Note: 测试加两层:解码器 fuzz 与确定性模拟(DST)

Status: proposed

> **第八轮(2026-10-02)**:DST 的 mock 交易所撮合与 `hft-sim` 共用一套撮合器;门槛口径改为"上线前"、交付按里程碑(DST 随 M2);见 [sim-accounts-not-backtest](../architecture/2026-10-02-sim-accounts-not-backtest.md)。

> **第八轮补充(2026-10-03)**:收窄验收——门槛改为累计量:每个 core 解码器累计 fuzz ≥ 2 h 零 panic,DST 累计 ≥ 200 000 种子零违反且覆盖 ≥ 7 次每夜运行,任何失败使对应计数清零;每夜批量受免费额度约束,不在 dev2 上跑;见 [ci-free-hosted](../architecture/2026-10-03-ci-free-hosted.md)。

> **第八轮第四次修订(2026-10-03)**:收窄——③④ 的累计在服务器上运行 CI 构建的验证包(dev2 常态运行,M7 的升级主机在门槛 ⑩ 测量窗口之外运行),不在 CI 每夜累计;"≥ 7 次每夜运行"改为"≥ 7 次运行,分布在 ≥ 7 个不同 UTC 日";开正式实盘的版本须补跑;见 [verify-on-server](../process/2026-10-03-verify-on-server.md)。

## Problem

单热进程架构的主要代价是爆炸半径:`panic = "abort"` 下任何一个解码器 panic 就停掉全部 venue、全部账户([12 §3.2](../../../../design/12-风险与未决问题.md))。解码器面对的是交易所随时可能变化的外部输入。另一方面,OMS、风控、sequencer 的缺陷往往只在特定交错下出现(fill-before-ack、重连中的回报、第二腿被拒同时 kill),单元测试与金样覆盖不到;代码大量由 AI 代理生成,隐蔽错误的先验更高。

## Proposal

[13 O8](../../../../design/13-设计评审与优化.md)、[10 §5](../../../../design/10-语言分工与代码组织.md)、[testing](../../../../docs/testing.md):

1. **Fuzz**:每个 `venue-<x>` 的行情、回报、REST 响应解码器,`hft-journal` 读者,`hft-event` 解码,各一个 `cargo-fuzz` 目标;语料 = 金样输入 + 变异;十进制解析另有对照参考实现的差分目标。每夜每目标 ≥ 1 h;解码器改动的 PR 10 min;新 venue 上线前连续 24 h。崩溃输入最小化后成为金样。
2. **DST**:[`hft-testkit`](../../../../crates/hft-testkit/README.md) 提供种子驱动的 mock 交易所 + 故障注入(断连、序号缺口、重复/乱序/延迟回报、灰色故障、限速拒绝、部分成交、第二腿被拒、对账不一致);被测 sequencer/OMS/风控以回放驱动单线程运行,一个种子决定全部交错。每次提交 ≥ 500 种子,每夜 ≥ 50,000;失败打印种子与 journal 片段,种子进回归集。
3. 不变量清单固定在 [testing §4](../../../../docs/testing.md#4-必须在-dst-与属性测试中持续成立的不变量)。

4. 第三轮新增的 DST 故障与用例:链上"泄露/重组回灌后迟到上链"([tx-onchain-expiry](../architecture/2026-10-01-tx-onchain-expiry.md))、未登记错误码、无过期保护的所 NotFound;崩溃尾部恢复与版本变化冷启动经 `RecoverDriver`([recover-driver](../architecture/2026-10-01-recover-driver.md))。
5. 不变量增加:OMS 槽不泄漏、第二腿数量 = 实际第一腿成交、同种子同结果、每钱包在途 nonce ≤ 1(清单见 testing §4)。

## Alternatives considered

**只靠属性测试(proptest-state-machine)。** 覆盖单个状态机很好,但不覆盖 sequencer 层面多个组件的交错与定时器;两者互补,属性测试保留。

**turmoil / madsim。** 它们劫持 tokio 或 libc 级时间与熵,只覆盖 tokio 控制面;我们的热路径没有 tokio,sequencer 本身已是"事件进、意图出"的纯状态机,自写 DST 驱动更直接([research/04 §7](../../../../design/research/04-oss-frameworks-and-patterns.md))。

**只靠影子与生产观察。** 事故发生在真钱上才发现;而且无法复现。

**用多进程隔离解码器以缩小爆炸半径。** 引入跨进程状态与延迟,违背单写者形态([single-writer-sequencer-and-journal](../architecture/2026-09-28-single-writer-sequencer-and-journal.md));fuzz 是更便宜的保险。

## Acceptance criteria

- 上线前每个 core 解码器 fuzz 连续 24 h 零 panic([testing §8](../../../../docs/testing.md#8-上线门槛))。
- DST 随 M2(决策核心)交付,覆盖 [05 §8](../../../../design/05-CEX执行层.md) 的全部失败模式,每夜 ≥ 50,000 种子零不变量违反。
- 任一 DST 失败种子能在开发机上以 `cargo xtask dst --seed S` 逐事件复现。

## Risks

- DST 的 mock 交易所行为与真实交易所不一致,给出虚假信心:mock 的回报时序参数来自 journal 实测分布,影子运行作为最后一道。
- fuzz 需要 nightly 工具链:固定日期,只用于 fuzz,不用于产物([development](../../../../docs/development.md#工具链))。
- 夜间计算量:fuzz + DST + 回放在参考机夜间窗口内完成,超时即拆分调度。
