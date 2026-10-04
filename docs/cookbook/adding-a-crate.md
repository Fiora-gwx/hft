# Cookbook:新增一个 crate

逐文件清单,只适用于**引擎 crate**(`crates/*`)。新策略不是新引擎 crate,走 [adding-a-strategy](adding-a-strategy.md)。tier、依赖方向、README 模板、目录约定归 [crates/AGENTS.md](../../crates/AGENTS.md);分层地图归 [architecture.md §7](../architecture.md#7-组件分层)。本页只给步骤与每一步的验证方法。

所有"验证"步骤在 CI 上执行(GitHub 托管 runner,工作流见 [development](../development.md)):本地只写代码并推送分支,不要求本地能编译。只在 aarch64 上存在的代码路径(如 `CNTVCT_EL0` 读取)由 aarch64 构建作业先跑测试再打包,失败即不出制品。

## 1. 先确认它值得成为 crate

三者占一才建([crates/AGENTS.md](../../crates/AGENTS.md#什么值得成为-crate)):

- 有**独立的不变量要测**(自己的状态机、自己的属性测试);
- 有 **≥ 2 个使用方**(例如热进程与 PyO3、实盘与 mock);
- 需要**单独的延迟基准**。

只是装配、线程布局、配置解析 → 留在 `apps/<app>/`。只是 < 200 行的小工具 → 并入最近的已有 crate(`hft-rt`、`hft-event` 的先例见 [13 §5](../../design/13-设计评审与优化.md))。

名字描述当前职责,不按第一个实现或未来扩展命名:`hft-<职责>`;交易所纯协议固定为 `venue-<exchange>`。

**验证**:在 PR 描述里写出上面三条中满足的那一条及证据(使用方列表或基准名)。评审拒绝没有这一段的新 crate。

## 2. 选 tier 与层

1. tier:`pure` / `hot` / `warm` / `cold` / `dev`,按最严格的公开入口标注([crates/AGENTS.md](../../crates/AGENTS.md#tier))。
2. 层:在 [architecture.md §7](../architecture.md#7-组件分层) 的 L0–L5 中定位;依赖只能向下。
3. 若它会拥有跨 crate 的类型,确认 [architecture.md §10](../architecture.md#10-核心类型归属) 里没有别的归属;有冲突先解决归属再建。
4. 引擎 crate 不依赖任何 `strategies/*` 与 `hft-strategy-registry`(含 `[dev-dependencies]`;需要真实策略的集成测试放 `strategies/registry/tests/` 或 `apps/hft-core/tests/`,引擎 crate 自己的测试用 `hft-testkit` 的测试策略),也不为某个策略而建:给策略用的通用构件放进 [`hft-strat-lib`](../../crates/hft-strat-lib/README.md)(或接口放进 [`hft-strategy-sdk`](../../crates/hft-strategy-sdk/README.md)),某个策略专用的代码留在该策略包里。

**验证**:把拟定的依赖列表对照 [crates/AGENTS.md 依赖方向](../../crates/AGENTS.md#依赖方向),每条边都能在图中找到。

## 3. 建目录与 Cargo.toml

```
crates/<name>/
  README.md        规格(模板见 crates/AGENTS.md)
  TESTS.md         测试规格(格式归 docs/tests/README.md)
  Cargo.toml
  clippy.toml      仅 hot/pure:tools/lint/clippy-{hot,pure}.toml 的逐字节副本
  src/lib.rs       模块入口与公开 API
  src/types.rs     只放类型(需要时)
  tests/           集成与属性测试
  benches/         Criterion 基准(hot 必有)+ budgets.toml
  fuzz/            cargo-fuzz 目标(venue-*、解码器、journal 读者必有)
```

`Cargo.toml` 要点:

- `edition`、`rust-version`、`license`、`version` 用 `*.workspace = true` 继承;`publish = false`。
- `[lints] workspace = true`(工作区基线 lint)。
- 依赖写 `name.workspace = true`,版本只在根 `Cargo.toml` 的 `[workspace.dependencies]` 声明。
- `pure`/`hot` crate 不得出现 `tokio`、`tracing-subscriber`、`serde_json`、`reqwest`、`pyo3`([hot-path-rules B6](../hot-path-rules.md#构建b))。

根 `Cargo.toml` 的 `[workspace] members` 按字母序加入。

**验证**(CI):`cargo metadata --format-version 1 >/dev/null`;`cargo build -p <name>`。

## 4. lib.rs 顶部属性

| tier | 必须的属性块 |
|---|---|
| 全部(除 `hft-rt`、`hft-journal`、`hft-net`、`hft-book`) | `#![forbid(unsafe_code)]` |
| `hft-rt`、`hft-journal`、`hft-net`、`hft-book` | `#![deny(unsafe_op_in_unsafe_fn, clippy::undocumented_unsafe_blocks)]` |
| `hot`、`pure` | `#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unreachable, clippy::todo, clippy::unimplemented)]` |
| 下单路径上的 crate 与模块([N3](../hot-path-rules.md#数值n) 列出的,含 `hft-strategy-sdk`、`hft-exec`) | 另加 `#![deny(clippy::float_arithmetic)]` |
| 解码器(`venue-*`、`hft-event`) | 另加 `#![deny(clippy::indexing_slicing)]` |

属性块逐字从 `tools/lint/attrs-<tier>.rs` 复制,不删减。

**验证**:`cargo xtask hot-check -p <name>`——检查属性块完整、`clippy.toml` 与模板一致、没有未登记的 `#[allow]`。

## 5. 登记依赖方向

1. 在 `tools/xtask/deps.toml` 加该 crate 的 tier 与允许的直接依赖(机器可读版本)。
2. 同一改动更新 [crates/AGENTS.md 依赖方向](../../crates/AGENTS.md#依赖方向) 的图与 [architecture.md §7](../architecture.md#7-组件分层) 的分层图(两处是人读视图,`doc-check` 比对它们与 `deps.toml` 是否一致)。

**验证**(CI):`cargo xtask deps` 通过;故意加一条反向依赖、一条到 `strategies/*` 或 `hft-strategy-registry` 的依赖(含 dev 依赖)应失败(新 crate 的第一个 PR 里以单独提交试一次,看到 CI 失败后撤掉该提交)。

## 6. 写 README(规格)

1. 按 [README 模板](../../crates/AGENTS.md#readme-模板) 的节顺序写,标题用原文;frontmatter `status: spec`,首次合入实现时改为 `impl`。
2. hot crate:"延迟与资源契约"里每个公开入口一行——入口、p50/p99 预算、分段名(只用 [latency-budget](../latency-budget.md#2-分段名全仓唯一) 的名称)、基准名。
3. "测试与验收"只写一句链接本目录 `TESTS.md` 的话("用例与验收见 TESTS.md。");用例写进同目录的 `TESTS.md`:先在 [docs/tests §3](../tests/README.md#3-编号) 的前缀表登记本 crate 的前缀,格式、覆盖表(README 每个规范性小节都有用例或不适用的理由)与失败路径要求归 [docs/tests](../tests/README.md);测试函数名以用例编号开头([§6](../tests/README.md#6-与代码的对应))。
4. 相对链接:到设计文档 `../../../architecture-v2/…`,到其他 crate `../<y>/README.md`,到 docs `../../docs/…`。
5. 只写机制与参数名:不出现策略族名(含小写 `cdex`;optional 链上组件只在概要的触发条件行写 CEX-DEX)或某个策略的规则数字;决策出处只在"延伸阅读"里链一次。
6. 若 crate 有进快照或检查点的状态:提供 `snapshot(&self, w)` 一次写完本 crate 的全部节(快照不分片)并登记节号与行版本([journal-format](../subsystems/journal-format.md));时刻与期限字段用 `engine_ns`(SDK `Now.engine_ns`)或 UTC 日历量,不存 TSC(唯一例外是 [hot-path-rules D6](../hot-path-rules.md#确定性d) 登记的 TSC 行);若要判断报价是否可用,只用 `BboView` 的连接健康([S6](../hot-path-rules.md#策略-sdk-与执行层s))。

**验证**:`cargo xtask test-trace`(里程碑已到的用例都有同编号的测试函数,反之亦然);`cargo xtask doc-check`——章节顺序、frontmatter 字段(`scope` 只能是 `core`/`optional`)、链接可达、每个公开入口有预算行、引擎 README 无策略族名。

## 7. 基准与预算(hot crate 必做)

1. `benches/<entry>.rs`:Criterion 基准走公开入口,不为基准新增导出;输入由常量合成或来自 `fixtures/`,不联网。
2. `benches/budgets.toml`:每个基准名 → 参考机 p50/p99 预算(ns)。它是 `bench-gate` 读取的源常量;README 的预算行与它一致由 `doc-check` 比对。
3. 路径级门槛(跨 crate)不放这里,放 [benchmarks/](../../benchmarks/README.md)。

**验证**(CI):`cargo xtask bench-gate --crate <name>` 能找到每个基准并输出与预算的比较;预算缺失即失败。时间门槛只在参考机上阻断(M1.5 起按需租用,每个里程碑结束时运行 `bench-gate --reference`,[latency-budget §6](../latency-budget.md#6-验收方法)),CI 托管 runner 上只报告;基准本身仍须在新 crate 的第一次提交里存在。

## 8. Fuzz(venue、解码器、journal 读者必做)

1. `fuzz/fuzz_targets/<target>.rs`,入口与实盘解码入口相同;语料目录由 `cargo xtask fixtures` 从金样生成,不手工复制。
2. 在 `tools/xtask/fuzz-targets.toml` 登记:CI 每夜据此短跑轮转做回归冒烟(分钟预算见 [development](../development.md));验证包据此打包 fuzz 二进制,core 解码器在服务器上的累计 fuzz CPU 时间(只累计相关源路径与候选版本相同的运行,[testing §6.1](../testing.md#61-服务器上的累计运行))计入[上线门槛](../testing.md#8-上线门槛) ③(只卡正式实盘)。

**验证**(CI):`cargo xtask fuzz <target> --time 60` 零崩溃。

## 9. 让组件出现在地图上

1. 根 [README.md](../../README.md) 的组件一览表加一行(tier、范围、一句话)。**范围的唯一来源是 README frontmatter 的 `scope: core | optional`**(`core` = 首次上线必须具备,`optional` = 可选扩展),组件表由 `doc-check` 生成并比对,不一致即失败;optional 组件在概要写明"何时值得做"(触发条件),不写周次或阶段号。
2. 若拥有跨 crate 类型:更新 [architecture.md §10](../architecture.md#10-核心类型归属)。
3. 若引入新的线程或进程:更新 [architecture.md §3](../architecture.md#3-hft-core-线程) 或 §2,装配放 `apps/`。
4. 若决定有持久理由(为什么拆出来、放弃了什么):写 Agent Note([.agents/notes](../../.agents/notes/README.md))。
5. 新增 crate 是设计基线 v1 之后的变更:在 [docs/changes.md](../changes.md) 记一行(流程见 [docs/tests §7](../tests/README.md#7-修改流程设计基线-v1-之后))。

**验证**(CI):`cargo xtask doc-check`;`cargo xtask ci` 全绿。
