---
component: registry
prefix: REG
spec: README.md
---

# hft-strategy-registry 测试规格

格式与规则见 [docs/tests](../../docs/tests/README.md)。

真实跨所策略场景在 `tests/cross_taker_e2e.rs`,覆盖范围归 [cross-taker/TESTS.md](../cross-taker/TESTS.md)。REG-008 金样同时验证全部 feature 导出与默认构建投影;REG-009 的版本扰动按目标策略身份定位,不依赖其在 index 中的位置。

资金费策略的同事件双腿、限价/深度、失败修复、结算及重放场景在 `tests/portfolio_carry.rs`,覆盖范围归 [portfolio-carry/TESTS.md](../portfolio-carry/TESTS.md)。

离线工具的共享夹具对照在 `tests/calibration.rs`（cross-taker feature）;直接调用真实信号并驱动 carry Strategy 校验目标数量,覆盖归 [strat-calib/TESTS.md](../../tools/strat-calib/TESTS.md)。

探针的真实引擎场景与确定性故障回归在 `tests/latency_probe_scenarios.rs`、`tests/latency_probe_dst.rs`,覆盖范围归 [latency-probe/TESTS.md](../latency-probe/TESTS.md)。

## 覆盖

| README 小节 | 用例 |
|---|---|
| 概要 | REG-001、REG-005、REG-011、REG-015 |
| 目录 | 不适用:非规范 |
| 使用本组件 | REG-001、REG-002、REG-004–REG-014、REG-016 |
| 实现说明 | REG-003、REG-004、REG-010、REG-012、REG-013 |
| 延迟与资源契约 | 不适用:由 SDK-198 覆盖 |
| 测试与验收 | 不适用:非规范(用例与验收在本文件) |
| 依赖 | REG-015 |
| 延伸阅读 | 不适用:非规范 |
| 已知限制与后续工作 | 不适用:非规范 |
| 开发备忘 | 不适用:非规范 |

## 用例

**共用夹具**:`Strategies` 为本 crate 的 `strategy_set!` 生成的集合;"默认构建"指不加 feature 的 `cargo build -p hft-strategy-registry`,"全 feature 构建"指 `--all-features`;`LP` = `latency_probe::LatencyProbe::DESC`;临时目录 `tmp` 由测试创建。默认列表为 latency-probe 与 portfolio-carry;策略级场景用例归各策略 TESTS.md。

### REG-001 默认构建的集合包含 latency-probe,且不含测试策略段

- 规则:[使用本组件](README.md#使用本组件)、[实现说明](README.md#实现说明)
- 级别:unit
- 里程碑:M2
- 前置:默认构建
- 步骤:
  1. 读 `Strategies::kinds()`
- 期望:
  - 含一项 `name = "latency-probe"`、`kind_id = 5`、`non_prod_only = true`、`schema.id = 5`
  - 每一项 `schema.id == kind_id as u32`;没有 `kind_id` ∈ [0xFF00, 0xFFFF] 的项
- 实现:`strategies/registry/tests/kinds.rs::reg_001_default_kinds`

### REG-002 加入 portfolio-carry 后默认构建的集合与顺序

- 规则:[使用本组件](README.md#使用本组件)
- 级别:unit
- 里程碑:M4
- 前置:默认构建
- 步骤:
  1. 读 `Strategies::kinds()`
- 期望:
  - 默认恰好两项,按列表顺序:(`latency-probe`, 5, `non_prod_only = true`)、(`portfolio-carry`, 1, `non_prod_only = false`);启用 cross-taker 时在既有首位加入 kind 3,不改变 core 项顺序
- 实现:`strategies/registry/tests/kinds.rs::reg_002_default_kinds_m4`

### REG-003 默认与全 feature 两种构建都能编译,编译期唯一性断言通过

- 规则:[实现说明](README.md#实现说明)、[已知限制与后续工作](README.md#已知限制与后续工作)
- 级别:table
- 里程碑:M2
- 前置:CI 构建矩阵作业;本 crate `Cargo.toml` 中声明的全部 feature
- 实现:`strategies/registry/tests/build_matrix.rs::reg_003_build_matrix`

| 行 | 构建 | 期望 |
|---|---|---|
| REG-003.1 | 默认构建 | 编译成功;`kinds()` 只含 `scope = core` 的策略 |
| REG-003.2 | 全 feature 构建 | 编译成功;`kinds()` 中每个 kind_id、name、`schema.id` 互不相同,并比默认构建多出 `Cargo.toml` 中每个 feature 门控的那一行 |

### REG-004 kind_id 账本检查

- 规则:[使用本组件](README.md#使用本组件)、[实现说明](README.md#实现说明)
- 级别:table
- 里程碑:M2
- 前置:`tests/kind_ids.rs` 的检查函数 `check_ledger(readme_text, kinds)`;`kinds` 取全 feature 构建的 `Strategies::kinds()`;每行把本 README 的 kind_id 登记表做所列改动后作为输入
- 实现:`strategies/registry/tests/kind_ids.rs::reg_004_ledger`

| 行 | 对登记表的改动 | 期望 |
|---|---|---|
| REG-004.1 | 不改 | `Ok` |
| REG-004.2 | 删去 kind_id 4 的行 | `Err`:编号不连续(缺 4) |
| REG-004.3 | 再加一行 kind_id 5 | `Err`:编号重复 |
| REG-004.4 | 删去 latency-probe 的行 | `Err`:`kinds()` 中的 5 不在表中 |
| REG-004.5 | latency-probe 行的 crate 列改为 `latency-probe2` | `Err`:名字不符 |
| REG-004.6 | latency-probe 行的 crate 列改为"退役:latency-probe" | `Err`:已编译的策略被标退役 |
| REG-004.7 | latency-probe 行的 scope 列去掉 `non_prod_only` | `Err`:`non_prod_only` 不符 |
| REG-004.8 | 追加一行 kind_id 6(尚无代码) | `Ok` |
| REG-004.9 | 追加一行 kind_id 7、没有 6 | `Err`:编号不连续 |
| REG-004.10 | 删去 0xFF00–0xFFFF 行 | `Ok`(测试段不参与连续性检查) |

### REG-005 `export_schemas` 按 kind_id 升序返回各策略文件,最后是 `index.json`

- 规则:[使用本组件](README.md#使用本组件)、[实现说明](README.md#实现说明)
- 级别:unit
- 里程碑:M2
- 前置:默认构建
- 步骤:
  1. 调 `export_schemas()`
- 期望:
  - 长度 = `kinds()` 的项数 + 1;前 N 项的文件名按 kind_id 升序,含 `"5-latency-probe.schema.json"`;最后一项文件名为 `"index.json"`
  - `5-latency-probe.schema.json` 的内容与 SDK `params::schema_json(&LP, LP.schema.version)` 逐字节相同
- 实现:`strategies/registry/tests/schemas.rs::reg_005_export_list`

### REG-006 `index.json` 的格式

- 规则:[使用本组件](README.md#使用本组件)
- 级别:unit
- 里程碑:M2
- 前置:默认构建;REG-005 的最后一项内容
- 步骤:
  1. 按字节检查,并用 JSON 解析
- 期望:
  - 顶层是数组(第一个字节 `[`),没有包装对象;字符串之外没有空白字符
  - 每个元素恰好 6 个键,顺序为 `kind_id`、`name`、`file`、`schema_id`、`schema_version`、`schema_digest`;元素按 `kind_id` 升序
  - latency-probe 的元素:`kind_id = 5`、`name = "latency-probe"`、`file = "5-latency-probe.schema.json"`、`schema_id = 5`、`schema_version = 1`、`schema_digest` = 16 个 `[0-9a-f]` 字符,等于 `format!("{:016x}", params::schema_digest(&LP, 1))`
- 实现:`strategies/registry/tests/schemas.rs::reg_006_index_format`

### REG-007 `schema_digest` 写成补零的 16 位小写十六进制

- 规则:[使用本组件](README.md#使用本组件)
- 级别:table
- 里程碑:M2
- 前置:`index.json` 生成函数内部的摘要格式化(`#[cfg(test)]` 直接调用)
- 实现:`strategies/registry/src/lib.rs::reg_007_digest_hex`

| 行 | 摘要 u64 | 期望字符串 |
|---|---|---|
| REG-007.1 | 0 | `"0000000000000000"` |
| REG-007.2 | 0xAB | `"00000000000000ab"` |
| REG-007.3 | 0x0123_4567_89AB_CDEF | `"0123456789abcdef"` |
| REG-007.4 | u64::MAX | `"ffffffffffffffff"` |

### REG-008 导出结果与已提交的 schema 金样逐字节相同

- 规则:[使用本组件](README.md#使用本组件)
- 级别:golden
- 里程碑:M2
- 输入:`export_schemas()` 的全部输出(默认构建)
- 期望文件:`fixtures/strategy-schemas/`(每个 `<kind_id>-<name>.schema.json` 与 `index.json`;目录中没有多余或缺少的文件)
- 实现:`strategies/registry/tests/schemas.rs::reg_008_golden`

### REG-009 改参数 schema 不升 `schema.version` 或不更新金样时 CI 失败

- 规则:[使用本组件](README.md#使用本组件)
- 级别:table
- 里程碑:M2
- 前置:`tests/schemas.rs` 的比较函数 `compare_golden(export, golden_dir)`;每行用当前导出与一份改造过的金样目录比较
- 实现:`strategies/registry/tests/schemas.rs::reg_009_golden_discipline`

| 行 | 金样目录相对当前导出的差异 | 期望 |
|---|---|---|
| REG-009.1 | 无 | `Ok` |
| REG-009.2 | latency-probe 的 `schema_digest` 不同、`schema_version` 相同(参数 schema 改了没升版本) | `Err(VersionNotBumped{kind_id: 5})` |
| REG-009.3 | `schema_digest` 与 `schema_version` 都不同,文件内容不同(升了版本、没更新金样) | `Err(GoldenStale{file: "5-latency-probe.schema.json"})` |
| REG-009.4 | 只有 `notes` 段不同、`schema_digest` 相同(改了描述符、没更新金样) | `Err(GoldenStale{file: "5-latency-probe.schema.json"})` |
| REG-009.5 | 多出一个 `9-foo.schema.json` | `Err(GoldenStale{file: "9-foo.schema.json"})` |

### REG-010 已提交的列表与 `index.json` 不得含测试策略段

- 规则:[实现说明](README.md#实现说明)、[使用本组件](README.md#使用本组件)
- 级别:table
- 里程碑:M2
- 前置:`tests/schemas.rs` 的检查函数 `check_no_test_range(index_json)`;每行一份 `index.json` 文本
- 实现:`strategies/registry/tests/schemas.rs::reg_010_no_test_range`

| 行 | `index.json` 中的 kind_id 集合 | 期望 |
|---|---|---|
| REG-010.1 | {5} | `Ok` |
| REG-010.2 | {5, 0xFF00} | `Err`,指出 65280 |
| REG-010.3 | {5, 0xFFFF} | `Err`,指出 65535 |
| REG-010.4 | {5, 0xFEFF} | `Ok` |
| REG-010.5 | 当前导出与 `fixtures/strategy-schemas/index.json` | 都 `Ok` |

### REG-011 `hft-core schemas --out` 原样写出 `export_schemas` 的每一项

- 规则:[使用本组件](README.md#使用本组件)、[概要](README.md#概要)
- 级别:proc
- 里程碑:M2
- 前置:CI 构建的 `hft-core` 二进制;空目录 `tmp`
- 步骤:
  1. `hft-core schemas --out tmp`
- 期望:
  - 退出 0;`tmp` 中的文件集合与 `export_schemas()` 的文件名集合相同,每个文件与对应内容逐字节相同
- 实现:`apps/hft-core/tests/schemas_cli.rs::reg_011_cli_writes_export`

### REG-012 `kinds()` 中只有 latency-probe 为 `non_prod_only`

- 规则:[实现说明](README.md#实现说明)、[使用本组件](README.md#使用本组件)
- 级别:unit
- 里程碑:M2
- 前置:全 feature 构建
- 步骤:
  1. 遍历 `Strategies::kinds()`
- 期望:
  - `non_prod_only = true` 的恰好一项,是 `kind_id = 5`;其余各项为 `false`
- 实现:`strategies/registry/tests/non_prod_only.rs::reg_012_flags`

### REG-013 正式账户上的 latency-probe 实例 `SetParams` 被拒,非正式账户照常

- 规则:[实现说明](README.md#实现说明)、[使用本组件](README.md#使用本组件)
- 级别:integ
- 里程碑:M3
- 前置:registry 的 `Strategies` 实例化 hft-testkit 场景(真实 sequencer 与 ctl-gateway 校验);refdata:账户 `P9`(Live,`is_prod = true`)、`T1`(Live 测试网,`is_prod = false`)、`S1`(Sim);latency-probe 的三个实例分别绑定 P9、T1、S1;参数为 latency-probe TESTS 的 `PP0` 与单腿 quote 组
- 步骤:
  1. 对三个实例各发一次 `SetParams`
- 期望:
  - P9 的实例:`CmdResult{InvalidArgs, detail = ParamInvalid}`,`arg & 0xF = 3`(`NonProdOnly`);该实例没有被创建(无 `on_params` 调用),journal 中没有它的 `StrategyParams` 生效记录
  - T1、S1 的实例:`CmdResult{Applied}`
- 实现:`strategies/registry/tests/non_prod_only.rs::reg_013_set_params_binding`
- 覆盖:三类账户各用独立引擎，ctl-gateway 的生产校验接到真实 sequencer；另绕过 gateway 注入正式账户参数，核对 `NonProdOnly`、零创建/回调与零生效版本。

### REG-014 `Strategies::schema_json` 与导出的文件内容相同

- 规则:[使用本组件](README.md#使用本组件)、[延迟与资源契约](README.md#延迟与资源契约)
- 级别:table
- 里程碑:M2
- 前置:默认构建
- 实现:`strategies/registry/tests/schemas.rs::reg_014_get_schema_same`

| 行 | 调用 | 期望 |
|---|---|---|
| REG-014.1 | `Strategies::schema_json(5)` | `Some(s)`,`s` 与 `5-latency-probe.schema.json` 的导出内容逐字节相同 |
| REG-014.2 | `Strategies::schema_json(0xFF01)` | `None` |
| REG-014.3 | `Strategies::schema_json(6)` | `None` |

### REG-015 只有 hft-core 与 replayer 依赖本 crate,本 crate 不正式依赖引擎 crate

- 规则:[依赖](README.md#依赖)、[概要](README.md#概要)、[使用本组件](README.md#使用本组件)
- 级别:table
- 里程碑:M2
- 前置:`tools/xtask/tests/fixtures/` 下的小型假仓库,`deps.toml` 与主仓库相同;每行在干净副本上只做一处改动,运行 `cargo xtask deps`;`strategy_set!` 调用位置的规则(含 SDK 自己 `tests/` 的例外)见 SDK-002,不在此重复
- 实现:`tools/xtask/tests/deps_registry.rs::reg_015_registry_edges`

| 行 | 改动 | 期望 |
|---|---|---|
| REG-015.1 | 不改 | 退出 0 |
| REG-015.2 | `apps/watchtower` 依赖 `hft-strategy-registry` | 退出 1 |
| REG-015.3 | `tools/hft-cfg` 依赖 `hft-strategy-registry` | 退出 1 |
| REG-015.4 | `crates/hft-py` 依赖 `hft-strategy-registry` | 退出 1 |
| REG-015.5 | `crates/hft-testkit` 的 `[dev-dependencies]` 加 `hft-strategy-registry`(hft-testkit 不得依赖本 crate) | 退出 1 |
| REG-015.6 | 本 crate 的 `[dependencies]` 加 `hft-sequencer` | 退出 1 |
| REG-015.7 | 本 crate 的 `[dev-dependencies]` 加 `hft-testkit` 与 `hft-sequencer` | 退出 0 |
| REG-015.8 | `apps/replayer` 依赖本 crate | 退出 0 |
| REG-015.9 | `apps/hft-core` 直接依赖 `strategies/latency-probe` | 退出 1(只经 registry) |

### REG-016 加入 portfolio-carry 后导出两份 schema,`index.json` 两个元素

- 规则:[使用本组件](README.md#使用本组件)
- 级别:unit
- 里程碑:M4
- 前置:默认构建
- 步骤:
  1. 调 `export_schemas()`
- 期望:
  - 3 项,依次 `"1-portfolio-carry.schema.json"`、`"5-latency-probe.schema.json"`、`"index.json"`;`index.json` 两个元素的 `kind_id` 依次 1、5
- 实现:`strategies/registry/tests/schemas.rs::reg_016_export_with_carry`

## 验收

- 里程碑已到的用例全部有同编号的实现且通过(`cargo xtask test-trace`,规则见 [docs/tests §6](../../docs/tests/README.md#6-与代码的对应))。
- schema 金样(REG-008、REG-009)与构建矩阵(REG-003)在每个 PR 上运行;改参数 schema 必须同时提升 `schema.version` 并更新 `fixtures/strategy-schemas/`,改 `notes`/`report`/`cmds` 只更新金样(`schema_digest` 不变)。
- 依赖方向:REG-015 与 SDK-001、SDK-002(`strategy_set!` 只出现在本 crate、hft-testkit 与 SDK 自己的 `tests/`)全部通过;引擎 crate(含 dev 依赖)与 hft-testkit 都不依赖本 crate。
- 新增一个策略时,只有 `strategies/<new>`、本 crate 的列表(`strategy_set!` 一行与 kind_id 登记表一行)、`Cargo.toml` 与 schema 金样有 diff(本 crate 的 `tests/` 可同时加该策略的引擎级测试);由 SDK-004、SDK-007 的解耦门槛用例检查。
- 各策略的引擎级场景、DST 与恢复测试放在本 crate 的 `tests/`,用例编号归各策略的 `TESTS.md`(如 latency-probe 的 LPR-065–LPR-071)。

产品形态回归 `tests/product_shape.rs::product_shape_rejects_wrong_venue_market_order_asset_and_missing_reference` 验证默认禁用配置也检查 venue/市场/同 base/腿顺序及缺引用,覆盖 validate_params、create、on_params,拒绝后合法配置仍可应用。
