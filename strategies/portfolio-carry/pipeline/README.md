---
description: "portfolio-carry 的 pipeline 插件包(Python,随策略发布):pipeline 框架按实例周期调用它,用分钟级以上的公共数据选币、给出 per-group 的 liq_score 与 restricted,并为需要预热的 group(实例冷启动、新加入或被重置的 group)生成 BasisWarmup / FundingWarmup 预热块"
kind: python-package
tier: cold
scope: core
status: spec
---

# portfolio-carry pipeline 插件(`hft_plugin_portfolio_carry`)

## 概要

本包是 [portfolio-carry](../README.md) 策略的慢路径插件:分发名 `hft-plugin-portfolio-carry`,导入名 `hft_plugin_portfolio_carry`,源码在 `strategies/portfolio-carry/pipeline/`,与策略 crate 同仓同版本发布。[pipeline 框架](../../../python/pipeline/README.md)经入口点发现它,按每个策略实例的周期调用 `compute`(名单与 per-group 字段)、`explain`(解释)与 `warmup`(预热块)。它**拥有**:币池规则、`liq_score` 与 `restricted` 两个字段的取值、预热样本的取数口径与 `BasisWarmup`/`FundingWarmup` 块的生成。它**不拥有**:调度、输入快照的 I/O、schema 校验与参数编码、人工覆盖项、版本库与下发、预热注入(框架);168 h 分位、资金费实时统计、打分与分配(策略的 `Model`);字段名、单位、默认值与预热载荷布局的定义(策略 README,唯一归属)。它不持有任何 key,只用公共接口;所有函数是"快照进、结果出"的纯函数。

## 目录

- [使用本组件](#使用本组件)
  - [包与注册](#包与注册)
  - [venue 角色](#venue-角色)
  - [外部输入](#外部输入)
  - [policy 字段](#policy-字段)
  - [币池规则](#币池规则)
  - [输出字段](#输出字段)
  - [预热](#预热)
- [实现说明](#实现说明)
- [延迟与资源契约](#延迟与资源契约)
- [测试与验收](#测试与验收)
- [依赖](#依赖)
- [延伸阅读](#延伸阅读)
- [已知限制与后续工作](#已知限制与后续工作)

## 使用本组件

### 包与注册

```
strategies/portfolio-carry/pipeline/
  pyproject.toml                    # [project.entry-points."hft_pipeline.plugins"] portfolio-carry = "hft_plugin_portfolio_carry:PLUGIN"
  src/hft_plugin_portfolio_carry/   # 见实现说明的源文件表
  tests/                            # 离线 fixtures 驱动
  README.md
```

```python
# plugin.py —— 实现框架的插件协议(hft_pipeline.api.Plugin;协议与数据类的定义归框架);__init__.py 重导出 PLUGIN
PLUGIN: Plugin = PortfolioCarryPlugin()

class PortfolioCarryPlugin:
    strategy_kind = 1                                  # = StrategyDesc.kind_id
    api_version: int                                   # 构建时的 hft_pipeline.api 版本(框架拒绝更高的)
    owned_fields = frozenset({"liq_score", "restricted"})
    required_inputs: tuple[InputSpec, ...]             # 见"外部输入"
    warmup_inputs: tuple[InputSpec, ...]               # 见"预热"
    def compute(self, snap: Snapshot, policy: Policy) -> PluginOutput: ...
    def explain(self, snap: Snapshot, policy: Policy, asset: str) -> list[ExplainStep]: ...
    def warmup(self, snap: WarmupSnapshot, policy: Policy) -> WarmupOut: ...
```

- 实例配置的 `plugin = "portfolio-carry"`(入口点名)选中本包;框架核对 `strategy_kind` 与实例的 `strategy_kind` 一致,不一致 → 加载失败。
- 本包只 import 框架的公开 API `hft_pipeline.api`;框架只经入口点加载本包,框架代码里不出现本策略的名字。
- 同一快照两次调用 `compute`/`warmup` 结果逐字节相同;时间只取 `snap.ts`,不读系统时钟,不做 I/O。

### venue 角色

实例配置按角色给出两个 venue:`perp`(做空永续的 venue)与 `spot`(持有现货的 venue);**两者必须属于不同交易所**(按 refdata `VenueMeta` 的交易所判定;框架在加载实例配置时拒绝同所,本包在 `compute` 中再断言一次)。

### 外部输入

`required_inputs`(公共接口,无 key;每个交易所一个令牌桶域,由框架统一限速;端点按 venue 所属交易所从 `sources.py` 的端点表选择):

| 输入 | 角色 | 内容 | 必需 | 用途 |
|---|---|---|---|---|
| `vol_24h` | `perp`、`spot` | 每个 inst 的 24 h 计价成交额(USDT) | 是 | 过滤、`liq_score` |
| `open_interest` | `perp` | 未平仓量,换算为 USDT | 是 | 过滤、`liq_score` |
| `funding_hist` | `perp` | 最近 `funding_lookback` 次结算的已实现费率与结算周期 | 是 | 过滤与排序(不下发统计值) |
| `listing` | `perp`、`spot` | 上架时间 | 否(缺失时该条件按不满足) | 过滤 |
| `trading_status` | `perp`、`spot` | 暂停/下架标记(接口提供时) | 否 | 过滤与 P1 提示 |

框架快照另给:当前 refdata 交易宇宙(两个角色各有哪些 inst)、每 group 的仓位/在途/残余与 `open_allowed`、当前生效的策略参数(框架解码后的字段值)与版本。任一**必需**输入失败 → `PluginOutput.ok = False`(框架只允许收紧);交易所限速时读到空列表按失败处理,不当作"无币"。

### policy 字段

policy 文件由框架按实例管理(实例配置的 `policy`);本包读取以下字段。默认值均为**暂定**,在模拟账户上研究后写入 policy;阈值未定前用静态名单。策略参数(窗口、采样间隔、覆盖下限等)不在 policy 里重复:预热需要的值从快照中**当前生效的策略参数**读取(见[预热](#预热)),结构参数热更后自动跟随。

| 字段 | 类型 | 默认 | 含义 |
|---|---|---|---|
| `universe_mode` | `static` \| `auto` | `static` | `static` = 只用 `static_assets`(雏形做法:固定名单);`auto` = 按下节规则选币 |
| `static_assets` | [资产] | 空 | `static` 模式的名单;`auto` 模式下作为必选名单 |
| `min_vol_usd` | Decimal | 2 000 000 | 永续与现货 24 h 成交额各自下限 |
| `min_oi_usd` | Decimal | 2 000 000 | 永续未平仓量下限 |
| `funding_lookback` | int | 21 | 统计的结算次数(8 h 周期约 7 天) |
| `min_pos_funding_frac` | Decimal | 0.6 | 其中正费率占比下限 |
| `min_listed_days` | int | 30 | 两腿上架天数下限 |
| `sort_key` | `funding_sum` \| `liq` | `funding_sum` | 排序键:`funding_lookback` 期已实现费率之和(折日化后比较)或 `liq_score` |
| `max_groups` | int | 32 | 名单上限(≤ 策略实例的 group 上限;模拟实例默认 ≤ 32 group,见[策略资源契约](../README.md#延迟与资源契约)) |
| `drop_after_rounds` | int | 6 | 无仓位且连续这么多轮不满足条件才移除 |
| `restricted_assets` | [资产] | 空 | 输出 `restricted = true` 的资产 |
| `liq_floor_usd` | Decimal | 100 000 | `liq_score` 的基准额 |
| `warmup.kline_ms` | int | 60 000 | K 线来源的样本间隔;须是生效的 `pct_sample_ms` 的整数倍且 ≤ 生效的 `warmup_max_step_ms`,否则不用 K 线来源并在 notes 中说明 |

### 币池规则

1. 候选 = 当前交易宇宙中两个角色都有 inst 的资产(`static` 模式下再与 `static_assets` 求交)。
2. 过滤(`auto` 模式):永续与现货 24 h 成交额 ≥ `min_vol_usd`;OI ≥ `min_oi_usd`;近 `funding_lookback` 次结算中正费率占比 ≥ `min_pos_funding_frac`;上架 ≥ `min_listed_days`;交易状态正常。
3. 按 `sort_key` 降序取前 `max_groups` 个(`static_assets` 优先占位)。
4. **保留**:已有仓位、在途或残余的 group 始终保留;不满足过滤条件时 `open_allowed = false`(只平不开;一轮不满足即置位,与策略[强制逐步减仓](../README.md#强制逐步减仓)的开始节奏如何配合,策略研究后定)。
5. **移除**:无仓位且连续 `drop_after_rounds` 轮不满足条件的 group 才移除,避免名单抖动。
6. **group 身份**:`GroupId = (StrategyId << 24) | 实例内计数器`,由框架单调分配、永不复用(语义与构造规则归 [hft-types](../../../crates/hft-types/README.md)):一直留在名单中的 group 保持原 id;被移除后再次入选的资产按新 group 分配新 id(策略对新 id 从空状态起步并经预热就绪,不继承旧 id 的统计与阈值)。本包只输出资产与腿,不输出也不推断 id。
7. 不在交易宇宙中的候选进入 `universe_candidates`(供 hft-cfg 生成次日 refdata)。
8. 交易状态异常(暂停、下架标记)只在 `notes` 与 `explain` 中标记并 P1 提示;正式的下架处理由 Admin 发 `VenueNotice`,本包与框架都不发该命令。

### 输出字段

`owned_fields` 只有两个 per-group 字段,单位与范围以[策略参数表](../README.md#参数表)为准:

| 字段 | 计算 |
|---|---|
| `liq_score` | `clamp(log10(min(OI_perp, vol_perp, vol_spot) / liq_floor_usd), 0, 3)`(雏形 `compute_liquidity_score` 的公式,基准额参数化);用 `decimal` 计算、四舍六入五成双到 schema 的 `Rate` 精度 |
| `restricted` | 资产在 `restricted_assets` 中 |

- `GroupOut.open_allowed` 按币池规则给出;`reason` 写入选/保留/只平不开的原因。
- **不输出** `force_exit`:它是人工覆盖项(按实例分开,框架管理),插件输出它会被框架拒绝(不在 `owned_fields`)。`owned_fields` 里没有结构字段(框架拒绝拥有 `structural` 字段的插件)。其余字段取 policy 默认值、覆盖项与 schema 默认值。
- 资金费的已实现统计与分位由策略 `Model` 计算;本包只把资金费历史用于过滤与排序,不下发统计值。

### 预热

`warmup` 为框架指定的 group 生成策略 Model 需要的历史样本,载荷布局的唯一归属是[策略 README 的预热载荷](../README.md#预热载荷)。框架在 `WarmupSnapshot` 中给出本次需要预热的 group 集合(判定与字段归[框架](../../../python/pipeline/README.md#model-预热)):实例首次启动、`Note{ModelCheckpointDiscarded}` 之后为当前名单的全部 group;`Note{ModelGroupReset}` 之后为它列出的 group(新加入的 group、`GroupSpec` 指纹变化的 group、结构参数热更时的全部 group)。本包只为集合内的 group 生成块(同一 group 因"新增 GroupId"与 `ModelGroupReset` 两个原因同时入选时的去重归框架;本包对重复请求输出相同的块,注入幂等)。回看窗口、采样间隔与覆盖下限取 `WarmupSnapshot` 中当前生效的策略参数(`pct_window_ms`、`pct_sample_ms`、`pct_min_coverage`、`warmup_max_step_ms`;框架解码后给出,接口归框架),不在 policy 里另设。

| 批 | `local_kind` | 来源(按框架 `warmup.sources` 的顺序尝试) | 内容 |
|---|---|---|---|
| `FundingWarmup` | 2 | 永续 venue 的资金费历史接口 | 回看生效的 `pct_window_ms` 内的逐期结算 `{at_ns, rate, interval_s}`(多于 256 期时只取最近 256 期,与策略资金费环 `FUNDING_RING` 相同);每块一个 group、≤ 170 条 |
| `BasisWarmup` | 1 | ① collector 的 BBO Parquet(两侧报价,`src = 1`,回看生效的 `pct_window_ms`,按生效的 `pct_sample_ms` 采样,同一采样点取两腿各自该点前最后一条报价);② 两个 venue 的 1 分钟 K 线收盘(`src = 2`,`step_ms = warmup.kline_ms`,开/平两侧同值) | 开仓侧 `(perp_bid − spot_ask)/spot_ask` 与平仓侧 `(perp_ask − spot_bid)/spot_bid`,`Rate` 存 i32,缺失 = `i32::MIN`;每块一个 group、≤ 509 个样本 |

- **块**:每块自身完整、≤ 4096 B(`BasisWarmup` 头 24 B + 509 × 8 B = 4096 B),可独立应用;`WarmupOut` 按块序列交给框架,框架一块一个 `chunk_no`、不再切分,每批最后一块 `last = true`(框架接口见 [python/pipeline](../../../python/pipeline/README.md#插件接口))。块的字节由 `encode.py` 按小端写出,与策略 crate 的 `src/warmup.rs` 共用金样 `fixtures/strategies/portfolio-carry/warmup/`。
- **样本数**:每 group 的覆盖率写入 `WarmupOut.notes`;低于生效的 `pct_min_coverage` 的 group 照常写入(由 Model 自己判断就绪),本次请求的全部 group 都低于它 → `ok = False`(框架不注入、P1)。
- **数据量**:满窗(168 h / 5 s)每 group ≈ 238 块,64 group ≈ 1.5 万块、约 62 MiB;1 分钟 K 线满窗每 group 20 块。全部 group 的注入只在实例首次启动、检查点丢弃或结构参数热更后发生;新加入或指纹变化的 group 只注入这些 group。注入速率由框架的 `warmup_rate_per_s` 限制。
- **价格口径**:基差只用两腿同一时刻(同一采样点)的价格;任一腿该点无数据 → 该点缺失,不插值(补齐规则在策略 Model 的 `PercentileStats`)。

## 实现说明

<details><summary>实现要点</summary>

- 纯函数 + 输入快照:I/O(取数、注入、写状态)全部在框架;本包对同一快照的输出逐字节相同,便于用 `rounds/<ts>/` 重放。
- 数值只用 `decimal.Decimal` 与整数,不经 `float`;`Rate` 与 `Notional` 按 schema 的定点指数量化(HalfEven)。
- 端点表 `sources.py`:按 refdata `VenueMeta.exchange` 选择每个输入的公共端点与解析函数,框架据 `InputSpec` 执行请求;端点、权重与可回溯长度待核实(开发备忘)。

| 文件 | 职责 |
|---|---|
| `src/hft_plugin_portfolio_carry/__init__.py` | 版本、重导出 `PLUGIN`(入口点指向它) |
| `src/hft_plugin_portfolio_carry/plugin.py` | `PortfolioCarryPlugin`、`PLUGIN`、`InputSpec` 声明 |
| `src/hft_plugin_portfolio_carry/universe.py` | 币池规则(过滤、排序、保留、移除、宇宙候选) |
| `src/hft_plugin_portfolio_carry/fields.py` | `liq_score`、`restricted` |
| `src/hft_plugin_portfolio_carry/warmup.py` | 取样口径、采样对齐、覆盖率 |
| `src/hft_plugin_portfolio_carry/encode.py` | `BasisWarmup`/`FundingWarmup` 块的字节编码(只编码预热载荷;参数编码只经框架调用的 `hft_py.params`) |
| `src/hft_plugin_portfolio_carry/sources.py` | 各交易所公共端点与解析 |
| `src/hft_plugin_portfolio_carry/explain.py` | `explain` 步骤 |
| `tests/` | 见测试 |

</details>

## 延迟与资源契约

冷进程,无热路径约束;运行在框架进程内(资源上限归框架的 systemd 单元)。

| 指标 | 目标 |
|---|---|
| 一轮 `compute`(不含取数) | < 5 s(64 个候选资产 × 两角色) |
| 一轮墙钟(含框架取数) | < 2 min(远小于实例周期 `cadence_min` 的一半;超过一半时框架报 P1,周期取值与规则归[框架实例配置](../../../python/pipeline/README.md#实例配置)) |
| 一次 `warmup`(64 group、168 h,不含取数) | < 60 s;取数 + 生成 < 10 min(受令牌桶约束);只预热部分 group 时按 group 数等比减少 |
| 外部请求 | 每轮交易所权重 < 该 IP 限额 10 %(框架令牌桶执行) |

## 测试与验收

| 层 | 用例 |
|---|---|
| 纯度 | 同一快照两次 `compute`/`warmup` 结果逐字节相同;源码不出现 I/O、`time.time`/`datetime.now`、`float` 运算(静态检查) |
| 币池 | 过滤每一条的边界;有仓位的 group 不满足条件 → 保留且 `open_allowed = false`;无仓位连续 `drop_after_rounds` 轮不满足 → 移除,少一轮 → 保留;`static` 模式只出名单内资产;两个角色 venue 同交易所 → 断言失败;输出中不含 `GroupId`(id 由框架分配,移除后再入选的资产由框架给新 id,框架测试) |
| 字段 | `liq_score` 与雏形 `compute_liquidity_score` 在网格输入上一致(量化后);只输出 `owned_fields` 中的字段;从不输出 `force_exit` |
| 失败路径 | 必需输入失败或限速返回空列表 → `ok = False`;非必需输入缺失 → 对应条件按不满足 |
| 预热 | 编码结果与 `fixtures/strategies/portfolio-carry/warmup/` 金样逐字节相同(策略 crate 测试解码同一批文件);每块 ≤ 4096 B 且自身完整;collector 源缺某段 → 该段缺失点为 `i32::MIN`、不插值;K 线源 `src = 2`、`step_ms = warmup.kline_ms`;覆盖率低于生效 `pct_min_coverage` 的 group 照常写入并记 notes;本次请求的全部 group 低于它 → `ok = False`;快照中的生效参数把 `pct_window_ms` 从 168 h 改为 72 h → 回看窗口随之变为 72 h(policy 不改);`warmup.kline_ms` 不是生效 `pct_sample_ms` 的整数倍 → 不用 K 线来源;快照只请求 2 个 group 时只输出这 2 个 group 的块(块头 `group` 全部在请求集合内),与全量输出中这 2 个 group 的块逐字节相同 |
| 整轮重放 | 用 `rounds/<ts>/` 快照经框架重跑,输出 blob 逐字节相同(框架测试) |

验收:模拟账户运行期间([上线门槛](../../../docs/testing.md#8-上线门槛) ⑦)每轮成功率 ≥ 99%;新实例一次预热完成后,策略 `PortfolioAdvice.n_warmed` 等于名单 group 数、下一个模型 tick 各 group 就绪(资金费历史到达且覆盖 ≥ 下限时)。

## 依赖

- 内部:框架公开 API `hft_pipeline.api`(插件协议、快照与输出数据类,与框架同版本发布)。
- 外部:`polars`(快照表计算);标准库 `decimal`、`struct`。版本锁定随 python 统一的 `requirements.lock`([python/AGENTS.md](../../../python/AGENTS.md))。
- 禁止:框架内部模块、`hft_py`(参数编码由框架调用)、网络与文件 I/O、系统时钟、任何交易所 key、写 hft 文件或共享内存、自写参数编解码。

## 延伸阅读

- [portfolio-carry](../README.md)(字段、单位、默认值与预热载荷的唯一归属)、[pipeline 框架](../../../python/pipeline/README.md)(插件协议、调度、覆盖项、注入)、[strategies/AGENTS.md](../../AGENTS.md)
- [hft-strategy-sdk 预热注入](../../../crates/hft-strategy-sdk/README.md#预热注入)、[warmup-injection](../../../.agents/notes/proposed/architecture/2026-10-03-warmup-injection.md)
- [第八轮裁决](../../../design/_work/round8-decisions.md) R8-12("策略研究后定"的暂缓项)
- 雏形数据来源:`E:\trading\Trading\portfolio-arb\src\market_meta.rs`(OI 与 24 h 成交额)、`src\score.rs`(`compute_liquidity_score`)、`src\funding.rs`

## 已知限制与后续工作

- 选币阈值未定:先用 `static` 模式的人工名单,在模拟账户上研究后切到 `auto`。
- 预热载荷由本包按策略定义的格式写、由 Rust 策略读,两侧一致只靠共用金样;格式变更时提升载荷首字节的版本号 `ver`(策略对未知版本按"无数据"处理,过渡期同时接受新旧版本),只有数据种类变化才追加新的 `local_kind`([SDK 版本与兼容规则](../../../crates/hft-strategy-sdk/README.md#版本与兼容规则))。
- 基差样本只能来自录过的 venue 与时段或 K 线近似;没有两侧报价历史的 venue 只能用 `src = 2`(策略以 `WARM_APPROX` 标出)。

### 开发备忘

<details><summary>给实现者</summary>

- **待核实**:每个 core venue 所属交易所的公共接口中 OI、24 h 成交额、资金费历史、1 分钟 K 线、上架时间与交易状态的端点、权重与可回溯长度;核实方法:在服务器上逐个请求并存金样(本机连不上交易所),结论写入 `sources.py` 的端点表(交易所名只出现在那里)。
- **待核实**:collector 的 BBO Parquet 对新框架交易宇宙的覆盖(哪些 venue 与 symbol 有数据);核实方法:读 collector 的 catalog 与 S3 前缀清单。
- 实现顺序:`plugin.py` + `universe.py`(`static` 模式)→ `fields.py` → `encode.py` + 金样 → `warmup.py` → `auto` 模式 → `explain.py`。
</details>
