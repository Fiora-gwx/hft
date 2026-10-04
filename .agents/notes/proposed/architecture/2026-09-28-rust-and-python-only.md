# Agent Note: 只用 Rust 与 Python 两种语言

Status: proposed

> **第八轮(2026-10-02)**:Python 不再承担回测驱动;Python 只做研究与分钟级以上的选币/名单,秒级慢计算属于策略包的 `Model`;见 [strategy-sdk-decoupling](2026-10-02-strategy-sdk-decoupling.md)、[sim-accounts-not-backtest](2026-10-02-sim-accounts-not-backtest.md)。

## Problem

需求写的是"不限语言",但团队是两名工程师 + AI 代理;每多一种语言就多一套工具链、构建、依赖审计与招聘成本。需要决定每一层用什么语言,且让选择由证据而不是习惯决定。

## Proposal

[10 §1](../../../../design/10-语言分工与代码组织.md):

| 层 | 语言 |
|---|---|
| 一切事件级路径:venue 线程、sequencer、OMS、风控、策略、journal、chain-core、冷进程中的 journal-archiver/watchtower/reconciler/kill-switch/replayer | Rust |
| 研究、回测驱动、参数流水线、ops_bot、treasury | Python(经 [`hft-py`](../../../../crates/hft-py/README.md) 读 journal、调回放与记账;经 ctl-gateway 发命令) |
| 厂商网卡库(仅可选的裸金属形态下 `ef_vi` 直通时) | C,仅以 FFI 形式 |
| 链节点 | 作为独立进程运行(bsc-geth / reth-bsc / Agave),不链接 |
| 合约 | Solidity + Foundry |
| 部署与调优 | Bash + systemd 单元,沿用 releasectl 纪律 |

Python 不出现在任何事件级路径;不写 C++。

## Alternatives considered

**C++ 写热路径。** 没有任何证据显示 C++ 的 WS/TLS/JSON 栈快于 Rust 对应物(sonic-rs、rustls + aws-lc-rs、tungstenite),见 [research/04 §3–§4](../../../../design/research/04-oss-frameworks-and-patterns.md);两套工具链与内存安全的代价对一个管真钱的系统不值。

**保留 Python 执行内核。** 现状 tick-to-trade 3–5 ms、链上腿 1.3 s、信号到双腿完成 2.5–3 s,根因是 GIL、区块级轮询与串行执行([00 §1](../../../../design/00-目标与延迟预算.md)、[research/00 §2.7](../../../../design/research/00-existing-codebase-inventory.md))。

**Go / Java(LMAX 原生语言)。** GC 暂停进入尾部;Java 需要大量 GC 与 JIT 调优,并引入第二套构建工具链。

**nautilus_trader 作为主系统。** LGPL、绑定自身引擎;只允许作为可选的研究侧第二系统,以 Python 包动态使用([10 §3](../../../../design/10-语言分工与代码组织.md))。

**ops_bot 用 Rust 重写。** 现有 6.2k 行 Telegram 逻辑是老板的工作流,Python 直接迁移成本最低;它是冷进程,语言不影响延迟([cold-process-set](2026-09-30-cold-process-set.md))。

## Acceptance criteria

- `cargo xtask deps` 证明 hot/pure crate 闭包不含 `pyo3`;Python 包对热进程的写入口只有 ctl-gateway socket。
- 仓库内没有 C++ 源文件;C 只出现在可选裸金属形态的 FFI crate。
- Python 流水线输出经 `SetParams`/`ReplaceRefdata` 进入 journal,可回放。

## Risks

- AI 生成的 Rust 代码可能有隐蔽错误:以属性测试、金样、fuzz、DST、回放一致性作门槛,热路径人工评审([12 §3.2](../../../../design/12-风险与未决问题.md))。
- PyO3 边界的版本耦合:`hft-py` 与 journal schema 由 `gen-schema` 生成物绑定版本。
