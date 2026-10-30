# AGENTS.md — crates

本子树的规则补充根 [AGENTS.md](../AGENTS.md#常驻规则)。`crates/` 里全部是引擎 crate;策略包在 [strategies/](../strategies/AGENTS.md)。

## 什么值得成为 crate

有**独立不变量要测**、有 **≥ 2 个使用方**、或需要**单独的延迟基准**,三者占一才建 crate。只是装配、线程布局、配置解析的代码留在 `apps/`。按当前职责命名,不按第一个实现或未来可能的扩展命名。

## tier

| tier | 含义 | 约束 |
|---|---|---|
| `pure` | 无 I/O、无线程、无全局状态 | 可被 PyO3、回放、mock、fuzz 直接使用;禁止 `std::time`、`std::net`、`std::fs`、线程 |
| `hot` | 在 tick-to-trade 路径上执行 | [hot-path-rules.md](../docs/hot-path-rules.md) 全部适用;公开函数标注预算 |
| `warm` | 在热进程内、不在 tick-to-trade 路径 | 不得持有热路径需要的锁;与热线程只经环或 journal 交互 |
| `cold` | 独立进程或离线工具 | 可用 tokio、tracing、serde_json;不得链接进 hot crate |
| `dev` | 测试与基准 | 只出现在 `[dev-dependencies]` |

一个 crate 可以同时提供 `pure` 核心与 `hot` 包装,但 tier 按最严格的公开入口标注。

## 依赖方向

```
hft-types ← hft-refdata ← hft-event                      (L0,被所有人依赖)
hft-strategy-sdk → L0                                     (策略接口,只依赖 L0)
hft-venue-proto ← venue-<x>                               (L1:trait 与纯协议)
hft-book  hft-oms  hft-risk  hft-ledger  hft-exec         (L1,彼此不依赖;oms/risk/ledger/exec 可依赖 hft-strategy-sdk 的视图与意图类型,book 只用其快照读写类型;RejectBits 在 hft-types;hft-risk 只输出 KillBits)
hft-strat-lib → L0 + hft-strategy-sdk ;  hft-feature → hft-strategy-sdk
hft-rt ;  hft-journal → hft-rt ;  hft-net → hft-rt + hft-event(只用 `ConnRole`);  hft-venue → hft-venue-proto + hft-net + hft-journal + hft-rt
hft-sequencer → hft-strategy-sdk + L1 + hft-journal + hft-rt(对 StrategySet 泛型;LiveDriver 写 KillWord = AtomicU32)
hft-ctl / hft-telemetry / hft-sim / hft-replay / hft-py / hft-testkit → 按需(各 README 的依赖节;都不依赖 hft-strategy-registry)
hft-ctl 的真实引擎 E2E 测试 dev 依赖:hft-sequencer + hft-oms + hft-ledger + hft-exec
optional 链上:hft-ext-proto → L0 ;  hft-amm → hft-types ;  hft-chain-evm → 以其 README 依赖节为准
strategies/<name> → hft-strategy-sdk + hft-strat-lib + hft-feature(细则见 strategies/AGENTS.md)
strategies/registry(hft-strategy-registry)→ 各 strategies/<name>(唯一策略清单;被 apps/hft-core、apps/replayer、tools/hft-pmcfg 依赖);dev: hft-ctl + hft-ctl-proto
```

- `apps/hft-core` 依赖 `hft-venue-proto` 作泛型协议装配；其 dev 依赖 `hft-testkit` + `hft-ctl-proto` + `hft-pmcfg`。
- **任何引擎 crate 都不依赖 `strategies/*` 与 `hft-strategy-registry`**(含 `[dev-dependencies]`,`hft-testkit` 也不例外):引擎 crate 的测试用 `hft-testkit` 的测试策略;唯一的策略清单是 `strategies/registry`,依赖它的只有 `apps/hft-core`、`apps/replayer` 与 `tools/hft-pmcfg`;需要真实策略的集成测试放 `strategies/registry/tests/` 或 `apps/hft-core/tests/`。
- `venue-<x>` 只依赖 `hft-types/hft-refdata/hft-event/hft-venue-proto` 与编解码库;不依赖 `hft-net`、`hft-venue`、`hft-sequencer`。
- `pure`/`hot` crate 不依赖 `cold` crate;`tokio` 不进 hot crate 的依赖闭包。
- `hft-journal` 的 `writer` 特性只由 apps/hft-core、apps/replayer(以及 optional 的 apps/chain-core)启用;其余 app 与 hft-py 只用读者([hft-journal](hft-journal/README.md#cargo-feature))。
- 本节的机器可读形式是 `tools/xtask/deps.toml`:`cargo xtask deps` 检查依赖图(含 dev 依赖),`doc-check` 检查本节与它一致、引擎 README 不出现策略族名(含小写 `cdex`;optional 链上组件只在概要的触发条件行写 CEX-DEX,[根规则](../AGENTS.md#常驻规则));doc-check 同样扫描 `python/` 框架包(Markdown 与 `.py`),策略的 pipeline 插件在 `strategies/<name>/pipeline/`,不在 `python/`;改依赖图要同时改这里、`deps.toml` 与 [architecture.md §7](../docs/architecture.md#7-组件分层)。

## 目录约定(实现时)

```
crates/<name>/
  README.md        规格与契约(本文件的模板)
  TESTS.md         测试规格:用例编号、输入与期望、覆盖表、实现函数名(格式归 docs/tests)
  Cargo.toml
  src/lib.rs       模块入口与公开 API
  src/types.rs     只放类型,无运行时代码(需要时)
  tests/           集成与属性测试
  benches/         Criterion 基准(热 crate 必有)+ budgets.toml(每个公开热入口的预算,bench-gate 读取)
  fuzz/            cargo-fuzz 目标(venue crate、hft-journal 读者、hft-event 解码必有)
  clippy.toml      本 crate 的 disallowed_methods/types(hot/pure crate 必有,见 hot-path-rules)
```

## README 模板

每个 crate/策略包/app/python 包的 README 按以下顺序,标题用原文:

```markdown
---
description: "一句话:谁在什么时候用它做什么"
kind: crate | app | python-package | dir
tier: pure | hot | warm | cold | dev
scope: core | optional               # core = 首次上线(新子账户上线 ccex 策略)必须具备、必须通过上线门槛;optional = 可选扩展(正文写触发条件,不写周次)
status: spec | impl | frozen          # spec = 只有规格;impl = 首次合入实现后;frozen = 冻结的可选组件:不随 core 规格同步,解冻时先按 core 规格复核
---

# <name>

## 概要
拥有什么、不拥有什么、谁调用、热路径位置,三到六句;optional 组件写触发条件(链上组件的触发条件行是唯一可以提 CEX-DEX 的地方);`frozen` 组件第一行写"冻结:不随 core 规格同步;解冻(触发条件同原文)时先按 core 规格复核",并列出已知待复核的评审编号(如"已知待复核:V3 #59、#60")。

## 目录

## 使用本组件
公开 API 草图(Rust 签名/trait/`#[repr(C)]` 布局,不写函数体)、配置项(参数名、类型、默认值来源)、消费与产生的事件、线程归属与调用约束。

## 实现说明
<details><summary>实现要点</summary>
设计要点、数据结构与内存布局、算法、状态机、不变量、规划的源文件表(文件 → 职责)。
</details>

## 延迟与资源契约
预算(p50/p99,指明分段与测量方式)、分配、系统调用、共享缓存行、对应的基准名。pure/cold 组件写吞吐或"无热路径约束"并说明原因。

## 测试与验收
只写一句"用例与验收见 [TESTS.md](TESTS.md)。";用例、覆盖表与整体验收写在同目录的 TESTS.md(格式归 docs/tests/README.md)。

## 依赖
允许的内部 crate 与外部库(带版本约束理由);明确禁止的依赖。

## 延伸阅读
链接 architecture-v2 章节、research 笔记、subsystems 页、相关 Agent Note;决策出处只在这里链一次。

## 已知限制与后续工作
当前规格有意不覆盖的情况、其后果、何时补。

### 开发备忘
<details><summary>给实现者</summary>
待核实项(写明核实方法)、开放问题、实现顺序建议。不放跨文档同步清单:需要别的组件改的,同一改动里直接改它的归属地。
</details>
```

## 其他规则

- **每个组件有 `TESTS.md`**(`scope: core` 且未冻结;格式、编号前缀与覆盖表归 [docs/tests](../docs/tests/README.md)):测试函数名以用例编号开头(`<前缀小写>_<三位数>_<描述>`,[§6](../docs/tests/README.md#6-与代码的对应)),`cargo xtask test-trace` 核对两边一一对应;改行为按 [§7](../docs/tests/README.md#7-修改流程设计基线-v1-之后) 同一提交改 README、`TESTS.md` 与代码,并记 [docs/changes.md](../docs/changes.md)。
- `src/types.rs` 只放类型;公开类型的文档注释写契约(单位、范围、所有权、失败),不写实现过程。
- 进快照的状态由本 crate 的 `snapshot(&self, w)` 一次写完全部节(不分片),节号与行版本在本 crate 登记([journal-format](../docs/subsystems/journal-format.md));时刻与期限字段用 `engine_ns` 或 UTC 日历量,不用 TSC;唯一例外是 [hot-path-rules D6](../docs/hot-path-rules.md#确定性d) 登记的 TSC 行,恢复时的处置按 D6。
- `#[repr(C)]` 布局、快照节字节布局与行版本号在标题旁注"草案,实现时以代码与编译期断言为准";码值与字段名必须全仓一致,线上整型码字段(`*kind`、`*level`、`*mode` 等)的注释写出码值或链接码表。
- 热 crate 的每个公开入口在 README "延迟与资源契约"里有一行预算与对应基准名;新增公开入口同一改动补上。
- `unsafe` 只允许在 `hft-rt`、`hft-journal`、`hft-net`、`hft-book` 内;每处带 `// SAFETY:`;其他 crate(含策略包)出现 `unsafe` 视为设计问题。
