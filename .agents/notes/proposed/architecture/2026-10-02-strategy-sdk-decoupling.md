# Agent Note: 策略与引擎解耦:策略 SDK、一策略一 crate、静态装配、schema 驱动参数

Status: proposed

> **第八轮补充(2026-10-03)**:补充——Model 检查点键只含 `kind_id`、状态布局版本、`schema.id`、实例与 `last_seq`,不随参数、refdata 与发版失效;历史样本经 `InjectWarmup` 作为 Input 注入 Model;参数编解码只有 SDK `params` 一份实现,Python 经 hft-py 调用;见 [warmup-injection](2026-10-03-warmup-injection.md)。

## Problem

修订前的规格把全部策略放在一个 crate 的封闭枚举 `StrategyKind` 里;参数是封闭枚举 `ParamsBody`(新增策略要改 hft-cfg 与 ctl 协议);慢计算要在 hft-event 登记全局 `AdviceKind`,载荷固定 `[i64; 6]`;sequencer 内写死资金公式 `wallet × leverage × deployable`;OMS 的 LegTracker 内写死 cdex 的 5% 补腿阈值与残余清理;`hft-strat-lib` 混着通用构件与 cdex 专用规则;pipeline 按策略写死([问题清单 A1](../../../../design/_work/round8-问题清单与修改制作方案.md))。新增或修改策略必然改引擎,违背用户的硬要求([round8-decisions](../../../../design/_work/round8-decisions.md) R8-3)。

## Proposal

接口的唯一归属是 [`hft-strategy-sdk`](../../../../crates/hft-strategy-sdk/README.md);目录规则见 [strategies/AGENTS.md](../../../../strategies/AGENTS.md)。

1. **SDK 是策略唯一依赖的稳定接口**:`Strategy`(快速部分)、`Model`(慢速部分)、`ParamSet`/`ParamSchemaDesc`、`Views`、`Ctx`、`EventRef`、`OrderIntent`、`TargetIntent`、`ExecSpec`、`StrategySet` 与 `strategy_set!`。SDK 只依赖 L0;L1 引擎 crate 可以依赖 SDK 的视图与意图类型;**任何引擎 crate 都不依赖 `strategies/*`**。
2. **一策略一 crate**(`strategies/<name>`),只依赖 SDK 与 `hft-strat-lib`;静态编译进同一二进制,装配处(`apps/hft-core`、`apps/replayer`、`hft-testkit`)的宏调用里加一行,生成静态分派的枚举,无 `dyn`。改策略代码要重新发布,参数热更新。
3. **策略包 = 快速部分 + 慢速部分 + 参数 schema**:快速部分在 sequencer 内,遵守热路径规则;`Model` 在 advisor 线程运行(允许 f64 与分配,必须确定),输出不透明的 `Advice` 载荷(≤ 4096 B)进入 journal,回放从日志读。
4. **参数**:`StrategyParamsHdr` + `GroupSpec`(引擎理解:订阅、账户绑定、LegTracker、ledger 归属)+ `body`(schema 序编码,由策略的 `ParamSet::decode` 解码);hft-cfg、control-plane、pipeline、ops_bot 按导出的 schema JSON 通用处理,新增策略不改它们的代码;新增只读命令 `GetSchema`。
5. **风控边界**:引擎负责全账户硬限额、kill、保证金率、裸腿、限速、对账、交易所能力约束;策略负责自己的进出场与止损。引擎只提供原始权益数据(`PortfolioView`),不计算"可用资金"。
6. **机检**:`cargo xtask deps` 查依赖方向;`doc-check` 查引擎 README 不出现策略族名;CI 解耦门槛:新增第二个测试策略时引擎 crate 的 diff 为零。

## Alternatives considered

**保留单 crate 封闭枚举。** 每加一个策略都要改 hft-strategies、hft-event(`AdviceKind`)、hft-cfg、ctl 协议;正是 R8-3 要消除的耦合。

**动态插件(`dyn Strategy`、dylib、WASM)。** 热路径禁止 `dyn`([hot-path-rules](../../../../docs/hot-path-rules.md));Rust 没有稳定 ABI;动态加载使回放的代码身份不确定。用户选择静态编译 + 参数热更新(R8-3 第 1 条)。

**慢计算放在 Python 流水线,经参数下发。** 分钟级以上的名单可以;秒级的分位、资金费统计、跨币分配需要与 journal 同步且可回放,只能在 advisor 里确定地计算。Python 只做研究与分钟级以上的选币/名单。

**保留 `[i64; 6]` 的 Advice 并扩大登记表。** 仍要改引擎才能新增种类,且装不下分配向量;不透明载荷让引擎零改动。

## Acceptance criteria

- `cargo xtask deps`:任何引擎 crate 的依赖闭包不含 `strategies/*`;策略 crate 只依赖 SDK 与 `hft-strat-lib`。
- `doc-check`:引擎 crate README 中策略族名出现 0 次。
- CI 中新增第二个测试策略的提交,引擎 crate 目录 diff 为 0 行([testing §8](../../../../docs/testing.md#8-上线门槛) ⑧)。
- 不启动 advisor 的 `replayer verify` 逐字节一致(Advice 全部来自 journal)。

## Risks

- SDK 本身变成新的耦合点:SDK 的改动按引擎改动对待,版本与兼容规则写在 SDK README。
- 泛型视图与静态分派的成本:预算写进 SDK README 并有基准;策略数增加只增加枚举分支。
- 不透明载荷引擎无法校验:策略自己版本化 `local_kind`,解码失败按"无 Advice"处理。
