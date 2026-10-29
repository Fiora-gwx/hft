# AGENTS.md — strategies

本子树的规则补充根 [AGENTS.md](../AGENTS.md#常驻规则) 与 [crates/AGENTS.md](../crates/AGENTS.md)。一个策略一个目录 `strategies/<name>/`:Rust crate(crate 名 = 目录名)+ 可选的 pipeline 插件包 `pipeline/`(Python,随策略发布)。接口在 [hft-strategy-sdk](../crates/hft-strategy-sdk/README.md);步骤在 [cookbook:新增策略](../docs/cookbook/adding-a-strategy.md)。

## 常驻规则

- **依赖**:只依赖 `hft-strategy-sdk`、`hft-strat-lib` 与 `hft-feature`(L0 类型经 SDK 重导出);不依赖引擎 crate 与其他策略 crate;外部库只允许纯计算库并在 README 写理由(`cargo xtask deps`)。
- **交易所无关**:策略代码、参数名、策略 README 不出现交易所名;差异只经 `Views::caps`/`inst`/`fee` 读取。插件按交易所取公共数据,交易所名只出现在插件的数据源表与源码里。
- **两腿不同交易所**:引擎强制([SDK](../crates/hft-strategy-sdk/README.md#strategy-与-model))。
- **快慢分工**:快速部分只做有界的整数判断与意图构造;打分、分位、统计、跨 group 分配放 Model,只经 Advice 交给快速部分([确定性规则](../crates/hft-strategy-sdk/README.md#确定性规则))。
- **时间与报价**:持久时刻存 `engine_ns` 或 UTC ns,快照与检查点不存 TSC;报价可用性按连接健康(`quote_live`);Advice 按 `basis_seq`/`basis_tsc` 判过期([时间轴](../crates/hft-strategy-sdk/README.md#时间轴))。
- **参数 schema**:可调量全部用 `define_params!` 声明,默认值注明来源(雏形值 / 实测 / 暂定);决定 Model 存储解释的字段标 `structural`,只许人工设定的标 `manual_only`,有单调放宽方向的标 `relax`,门槛 ⑦ 的小改范围标 `minor_change`。
- **描述符**:`notes`、`report`、`cmds` 必填;只产生测量样本的策略设 `non_prod_only`。
- **Model**:声明 `mem_bytes`/`snapshot_bytes` 上界与 `LAYOUT`;检查点 = 原始样本节 + 路径状态节(`PATH_VERSION`),恢复后续算与连续运行逐字节相同;`on_params` 按 GroupId 指纹与结构参数重置 group 并报告;长窗口统计定义预热载荷,由插件生产([Model 检查点](../crates/hft-strategy-sdk/README.md#model-检查点))。
- **风控边界**:账户限额、kill、保证金分档、裸腿、限速、对账归引擎;策略负责进出场与止损,必须处理 `Risk`、`External`、`Notice`、`Mode`;`Broken` 不是终态。
- **一行注册**:新增策略只改 `strategies/<name>` 与 [registry](registry/README.md) 一行;引擎 crate、app(含 hft-core 规格)与 `python/` 框架零改动(门槛 ⑧;group 数超过每实例上限的例外见 [cookbook](../docs/cookbook/adding-a-strategy.md))。需要真实策略的引擎级测试放 `registry/tests/`。
- **研究中的规则**:写明"规则未定、在模拟账户上研究";结论进 Agent Note;不做历史回测。

## 模块约定(hot-check 按路径施加规则)

| 路径 | 内容 | 规则集([hot-path-rules](../docs/hot-path-rules.md)) |
|---|---|---|
| `src/lib.rs` | `StrategyDesc`、`impl Strategy`(转发到 `fast`) | hot |
| `src/fast/**` | 快速部分 | hot;模块根 `#![deny(clippy::float_arithmetic)]` |
| `src/model/**` | Model | warm + D;唯一允许 f64 与 `model_math` |
| `src/params.rs`、`src/desc.rs` | `define_params!` 与 `check()`;描述符常量 | 冷路径;禁 f64 |
| `src/advice.rs`、`src/warmup.rs` | Advice 与预热载荷类型 | 只含整数 |
| `tests/`、`benches/` | 单测、夹具、金样、基准 | dev |
| `pipeline/` | 插件包,只依赖 `hft_pipeline.api` | Python 冷进程([python/AGENTS.md](../python/AGENTS.md)) |

`src/fast/` 与 `src/lib.rs` 不得 `use` `src/model/`;两部分只经 `advice.rs` 与 `params.rs` 交互。无 Model 用 `NoModel<Params>`。

## kind_id

登记表(只追加、永不复用)在 [registry](registry/README.md);`schema.id = kind_id as u32`。

## README

模板同 [crates/AGENTS.md](../crates/AGENTS.md#readme-模板),`kind: crate`,`tier: hot`。"使用本组件"含 group 形态、参数表、事件处理表、Note 码表与报表字段表、策略命令表、Advice 与预热载荷表(有时);"延迟与资源契约"登记 SDK 规定的全部入口;参数表写出每个字段的 `minor_change`(门槛 ⑦ 的小改判定,规则见 [testing ⑦](../docs/testing.md#8-上线门槛)),不另写白名单。插件包 README `kind: python-package`、`tier: cold`。core 策略包另有 `TESTS.md`([格式与编号](../docs/tests/README.md);portfolio-carry 随策略研究再写)。

## 上线流程

单测与场景 → 确定性、恢复与 DST → 模拟账户(每个上线策略按门槛 ⑦)→ 门槛全过后正式实盘;步骤见 [cookbook](../docs/cookbook/adding-a-strategy.md)。
