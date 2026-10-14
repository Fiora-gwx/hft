---
description: "全仓唯一列出真实策略的地方:一次 strategy_set! 调用生成 StrategySet,外加 export_schemas();hft-core 与 replayer 依赖它而不直接依赖任何策略 crate;需要真实策略的引擎级测试也放在这里"
kind: crate
tier: hot
scope: core
status: impl
---

# hft-strategy-registry

## 概要

`hft-strategy-registry`(目录 `strategies/registry`)是**全仓唯一列出真实策略的地方**:它包含唯一一次列出真实策略的 `strategy_set!` 调用(生成 `Strategies: StrategySet`)与冷路径函数 `export_schemas()`。只有 [apps/hft-core](../../apps/hft-core/README.md) 与 [apps/replayer](../../apps/replayer/README.md) 依赖本 crate,不直接依赖任何 `strategies/*`,所以实盘、模拟与回放天然使用同一列表。[hft-testkit](../../crates/hft-testkit/README.md) **不依赖**本 crate:它只带 CI 测试策略;需要真实策略的引擎级测试(场景、DST、恢复)放在本 crate 的 `tests/`(或 `apps/hft-core/tests/`),以 `Strategies` 为类型参数驱动 hft-testkit 的通用场景。它没有自己的逻辑:生成的分派代码在 sequencer 线程的热路径上运行(故 tier 为 `hot`),分派规则与预算归 [hft-strategy-sdk](../../crates/hft-strategy-sdk/README.md#装配strategyset-与-strategy_set)。新增策略 = 新 crate + 本 crate 里一行;app 与引擎 crate 都不改。

## 目录

- [使用本组件](#使用本组件)
- [实现说明](#实现说明)
- [延迟与资源契约](#延迟与资源契约)
- [测试与验收](#测试与验收)
- [依赖](#依赖)
- [延伸阅读](#延伸阅读)
- [已知限制与后续工作](#已知限制与后续工作)

## 使用本组件

当前列表包含默认的 latency-probe、portfolio-carry 与 feature 门控的 cross-taker。追加策略保留既有项顺序,按 kind_id 排序的 schema 导出不受注册顺序影响。后者的真实引擎/模拟撮合、参数恢复和报价门控测试位于 `tests/cross_taker_e2e.rs`;schema 金样保存全部已实现策略,默认构建只比较启用项。

```rust
// src/lib.rs —— 真实策略的列表只写在这一处;每行一个策略,optional 策略用 Cargo feature 门控。
hft_strategy_sdk::strategy_set! {
    pub struct Strategies {
        portfolio_carry::PortfolioCarry,                              // kind_id 1(core)
        latency_probe::LatencyProbe,                                  // 5(core;non_prod_only)
        #[cfg(feature = "cdex")]        cdex::Cdex,                   // 2(optional)
        #[cfg(feature = "cross-taker")] cross_taker::CrossTaker,      // 3(optional)
        #[cfg(feature = "maker-taker")] maker_taker::MakerTaker,      // 4(optional)
    }
}

/// 冷路径:每个策略的规范 schema JSON(SDK params::schema_json(&DESC, DESC.schema.version),含参数字段与 notes/report/cmds 描述符),
/// 返回 (文件名 "<kind_id>-<name>.schema.json", 内容),按 kind_id 升序;最后一项是 ("index.json", 内容)(格式见下)。
pub fn export_schemas() -> Vec<(String, String)>;
```

- **使用方**:`Sequencer<Strategies>`、`Advisor<Strategies>`、ctl-gateway(`Strategies::decode_params`/`validate_params`、`model_mem_bytes` 预算、`kinds()` 的 `non_prod_only`、`cmds`、`GetSchema`)在 hft-core;replayer 用同一类型做 `replay verify` 与模拟重演。没有第三个使用方。
- **测试**:hft-testkit 的 `TestStrategies` 只含测试策略(kind_id 0xFF00–0xFFFF,由 hft-testkit 自己的一次 `strategy_set!` 调用生成),供引擎 crate 的测试使用,不含真实策略。[解耦门槛](../../docs/testing.md#8-上线门槛) ⑧ 在本 crate 的**补丁副本**(CI 临时生成,列表末尾加一行 CI 测试策略)上构建 hft-core,检查引擎 crate 与 app 的 diff 为零;补丁副本不提交。**真实策略的引擎级测试**(各策略 README 列出的引擎场景、DST、恢复与 `replay verify` 夹具)放在本 crate 的 `tests/`:`[dev-dependencies]` 为 hft-testkit 与引擎 crate,用 `Strategies` 实例化 hft-testkit 的通用场景与 DST 驱动(对 `S: StrategySet` 泛型);依赖方向是本 crate(dev)→ hft-testkit,反向不允许。
- **schema 导出**(全仓唯一方式):`hft-core schemas --out <dir>` 把 `export_schemas()` 返回的每一项原样写成 `<dir>/<文件名>`,即各策略的 `<kind_id>-<name>.schema.json`(kind_id 十进制)与 `<dir>/index.json`。**`index.json` 的格式唯一归属本节**,由 `export_schemas()` 生成:顶层是 JSON 数组,每个元素 `{"kind_id", "name", "file", "schema_id", "schema_version", "schema_digest"}`(按 kind_id 升序,键按此顺序;`name` = `StrategyDesc.name`,`file` = 该策略的文件名,`schema_digest` = SDK `params::schema_digest(&DESC, schema.version)` 的 u64 值写成 16 位小写十六进制字符串、高位在前、补零),没有包装对象,写法与 SDK 规范 schema JSON 相同(无多余空白、数值为十进制整数);[tools](../../tools/README.md) 与 [hft-py](../../crates/hft-py/README.md) 按此读取。发布制品附带该目录。`hft-cfg`、`hft-ctl`、hft-py(pipeline、ops_bot)、watchtower、journal-archiver 只读这些 JSON,参数与策略命令的编解码只经 SDK 的 `params` 模块(Python 侧经 hft-py),不链接本 crate,也没有第二个编码器。

**kind_id 登记**(只追加、永不复用;各策略在自己的 `StrategyDesc` 里声明,宏在编译期检查唯一;本表是账本,由 `tests/kind_ids.rs` 检查,规则见实现说明):

| kind_id | crate | scope | Cargo feature |
|---|---|---|---|
| 1 | [portfolio-carry](../portfolio-carry/README.md) | core | 默认 |
| 2 | [cdex](../cdex/README.md) | optional | `cdex` |
| 3 | [cross-taker](../cross-taker/README.md) | optional | `cross-taker` |
| 4 | [maker-taker](../maker-taker/README.md) | optional | `maker-taker` |
| 5 | [latency-probe](../latency-probe/README.md) | core(`non_prod_only`:只能绑定非正式账户) | 默认 |
| 0xFF00–0xFFFF | 测试策略(hft-testkit 的 `TestStrategies`、SDK 自己 `tests/` 的宏测试与解耦门槛的补丁副本;不进发布制品) | — | — |

新策略取下一个编号(已登记的最大值 + 1);`schema.id` 约定为 `kind_id as u32`。退役的策略保留其行,crate 列写“退役:<原名>”。

## 实现说明

<details><summary>实现要点</summary>

- 列表只存在于本 crate 的一次 `strategy_set!` 调用;`strategy_set!` 接受行上的 `#[cfg(...)]`。hft-testkit 的测试列表与 hft-strategy-sdk 自己 `tests/` 中的宏测试是另外两次调用,只含测试策略(kind_id 0xFF00–0xFFFF),与本 crate 没有共享的宏或回调。
- **账本检查**(`tests/kind_ids.rs`,以 `include_str!` 读本 README 的 kind_id 登记表):真实策略行(0xFF00–0xFFFF 段以外)的 kind_id 从 1 起连续、不重复;启用全部 feature 时 `Strategies::kinds()` 的每一项都在表中,`name` 与 crate 列相同、未标退役,且 `non_prod_only` 为真当且仅当 scope 列写明 `non_prod_only`。
- 编译期断言(由 `strategy_set!` 生成):`kind_id`、`name`、`schema.id` 各自唯一;启用全部 feature 时同样成立。kind_id 0xFF00–0xFFFF 保留给测试策略:已提交的列表与发布制品的 `index.json` 不得含该段(`tests/schemas.rs` 检查)。
- `non_prod_only` 的策略(当前只有 latency-probe)照常编进全部构建;它不能绑定正式账户由 hft-core 装配与 ctl-gateway 保证([SDK](../../crates/hft-strategy-sdk/README.md#strategy-与-model)),不靠 feature 裁剪。
- `export_schemas()` 按 kind_id 升序输出;各策略文件的内容由 SDK 生成,本 crate 不改写,只按上节格式生成 `index.json`。

| 文件 | 职责 |
|---|---|
| `src/lib.rs` | `strategy_set!` 调用(`Strategies`)、`export_schemas` |
| `tests/schemas.rs` | 导出结果与已提交的 schema 金样比对;`index.json` 顶层为数组 |
| `tests/kind_ids.rs` | kind_id 账本检查(见上) |
| `tests/calibration.rs` | 离线工具共享夹具与真实策略信号/目标数量对照;见 [strat-calib](../../tools/strat-calib/README.md) |
| `tests/non_prod_only.rs` | `Strategies::kinds()` 中 latency-probe 的 `non_prod_only = true`、其他真实策略为 false;hft-testkit 场景里对绑定到 `is_prod = true` 账户的 latency-probe 实例发 `SetParams` → `ParamInvalid{NonProdOnly}`(装配期退出 2 `ProbeOnProdAccount` 的用例在 [apps/hft-core](../../apps/hft-core/README.md) 的测试里) |
| `tests/<strategy>_scenarios.rs`、`tests/<strategy>_dst.rs`、`tests/<strategy>_recovery.rs` | 各策略 README 列出的引擎场景、DST 与恢复测试(hft-testkit 驱动,`Strategies` 为类型参数);`cargo xtask dst --strategy <name>` 运行对应的 DST 目标 |

</details>

## 延迟与资源契约

本 crate 没有自己的热路径入口;生成的分派开销按 SDK 的 `sdk/set_dispatch`(p50 ≤ 2 ns、p99 ≤ 5 ns)与各策略 README 的预算验收。`export_schemas()` 只在打包(`hft-core schemas`)时运行(冷);`GetSchema` 直接用 `Strategies::schema_json`。

## 测试与验收

`tests/portfolio_carry.rs` 用真实 sequencer、mock 成交和 journal 验证 [CARRY-002/010](../portfolio-carry/TESTS.md),覆盖双向 Simultaneous 入场/退出、双侧深度与限价、非对称成交/拒单/在途断线的确定性故障种子、实际资金费去重与逐字节重放。

`tests/latency_probe_scenarios.rs` 与 `tests/latency_probe_dst.rs` 通过真实引擎和模拟交易所验证挂撤、跨网关触发关联、双腿/目标退出、价格带与 Broken 自动修复;每个场景验证 journal 重放。当前覆盖与剩余长时间/故障矩阵归 [探针测试规格](../latency-probe/TESTS.md)。

用例与验收见 [TESTS.md](TESTS.md)。

`REG-013` 使用同一单腿 quote 参数分别驱动正式 Live、测试网 Live 与 Sim 账户。ctl-gateway 和 sequencer 均检查正式账户限制；正式账户没有策略创建或参数回调，另外两类经真实命令路径生效。

## 依赖

- 内部:`hft-strategy-sdk`;列表中的每个 `strategies/*` crate(optional 的经 Cargo feature)。`[dev-dependencies]`:hft-testkit 与它驱动的引擎 crate(只用于 `tests/`)。
- 外部:无。
- 禁止:正式依赖(非 dev)任何引擎 crate(本 crate 只装配策略,不依赖 sequencer、OMS 等);被任何引擎 crate 或 hft-testkit 依赖。

## 延伸阅读

- [hft-strategy-sdk:装配](../../crates/hft-strategy-sdk/README.md#装配strategyset-与-strategy_set)、[SDK:规范 schema JSON 与描述符](../../crates/hft-strategy-sdk/README.md#策略描述符note报表策略命令)、[strategies/AGENTS.md](../AGENTS.md)、[cookbook:新增策略](../../docs/cookbook/adding-a-strategy.md)

## 已知限制与后续工作

- 策略静态编译进二进制:新增或修改策略需要重新发布(参数可热更新)。
- optional 策略的启用由 Cargo feature 决定,发布制品的 feature 组合由 [deploy](../../deploy/README.md) 规定。

### 开发备忘

<details><summary>给实现者</summary>

- 实现顺序:SDK 的 `strategy_set!` 支持行级 `#[cfg]` → 本 crate → hft-core `schemas` 子命令 → schema 金样 → 各策略的引擎级测试(依赖 hft-testkit 的场景与 DST 驱动对 `S: StrategySet` 泛型,见 [hft-testkit](../../crates/hft-testkit/README.md))。
</details>
