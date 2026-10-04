---
description: "研究查询库(可选):把 S3 Parquet(已封存小时)与当日 journal(经 hft-py)统一成 Arrow/Polars,提供对齐、价差、延迟分布、成交漏斗与模拟结果分析(模拟账户报告、模拟 vs 实盘校准);不做回测"
kind: python-package
tier: cold
scope: optional
status: spec
---

# research

## 概要

`research` 是研究侧的 Python 库(`hft_research`)与 notebook 约定:它让研究者用一套 API 读"历史 + 今天"的同一份数据——已封存小时来自 S3 `hft/<host>/<instance>/` 下的交易 journal Parquet([journal-archiver](../../apps/journal-archiver/README.md)),当前未封存部分来自 journal(经 [hft-py](../../crates/hft-py/README.md) 转 Arrow)——并在接缝处按 seq 去重。它提供常用查询(跨所 tick 对齐、价差序列、延迟分段分布、成交漏斗、PnL 分解)与**模拟结果分析**(模拟账户的表现、同参数模拟与实盘的校准差异)。**不做回测**:不提供历史回放评估、参数网格扫描或任何"如果当时"的收益估计([sim-accounts-not-backtest](../../.agents/notes/proposed/architecture/2026-10-02-sim-accounts-not-backtest.md));策略表现只在模拟账户与实盘上观察。它不下发命令、不持任何 key、不在交易主机上运行;现有 `analysis/` 的 DuckDB 工作流保持可用。

**可选扩展**。触发条件:模拟账户与实盘积累到需要系统化分析(跨账户对比、校准、漏斗诊断),现有 `analysis/` 的 DuckDB 脚本与 ops_bot 日报不够用时。

## 目录

- [使用本组件](#使用本组件)
  - [数据访问 API](#数据访问-api)
  - [时间戳与口径规则](#时间戳与口径规则)
  - [研究查询](#研究查询)
  - [模拟结果分析](#模拟结果分析)
  - [研究提案](#研究提案)
- [实现说明](#实现说明)
- [延迟与资源契约](#延迟与资源契约)
- [测试与验收](#测试与验收)
- [依赖](#依赖)
- [延伸阅读](#延伸阅读)
- [已知限制与后续工作](#已知限制与后续工作)

## 使用本组件

### 数据访问 API

```python
from hft_research import Data

d = Data(
    s3_root="s3://gwx-trading-data/hft",                    # 运维只读角色 hft-ops-read
    journal_roots={"prod": "/data/research/journal-sync/prod",   # 可选:当日数据的只读同步副本;
                   "sim": "/data/research/journal-sync/sim"},   # 模拟账户只在 sim 实例
    instance="prod",
    cache_dir="/data/research/cache",                       # Parquet 本地缓存(按对象 SHA-256 命名)
)

d.dataset(name: str, *, venue: str | list[str] | None = None, symbol: str | list[str] | None = None,
          account: str | list[str] | None = None, t0: str | int, t1: str | int,
          columns: list[str] | None = None, time_col: str = "rx_ns") -> pl.LazyFrame
# 便捷封装(同签名,name 固定):
d.bbo(...); d.trades(...); d.l2_delta(...); d.funding(...); d.mark(...)
d.order_events(...); d.decisions(...); d.exec_decisions(...); d.external_fills(...); d.margin_reports(...)
d.latency(stage: str, ...); d.notes(kind: str, ...)
d.accounts(at_seq: int | None = None) -> pl.DataFrame   # 账户表:名称、kind(Live/Sim)、绑定实例
d.config_at(seq: int, kind: str) -> bytes               # 某 seq 时刻生效的 ConfigBlob 内容(refdata/参数)
d.params_at(seq: int, account: str) -> dict             # 该账户当时生效的参数,经 hft_py.params.decode 按 schema 解码为字段表
d.manifest(journal: str, hour: str) -> dict             # 该小时 manifest(核验用)
```

- 返回 `polars.LazyFrame`;调用方 `.collect()` 或交给 DuckDB(`duckdb.from_arrow`)。
- 统一规则:对 `[t0, t1)`,已有 manifest 的小时读 Parquet;没有 manifest 的小时读 journal;两段按 `(instance, journal, seq)` 去重后按 `seq` 排序拼接。journal 不可用时只返回已封存部分并发出 `IncompleteRangeWarning`(不静默)。
- 定点列保持整数;`with_float(px_cols=True)` 按 KV 元数据的 `px_exp/qty_exp` 生成 `px_f64` 等**派生**列,原列不变。
- 每个与账户相关的结果带 `account_kind` 列;任何把模拟与实盘合计的聚合必须显式传 `mix_kinds=True`,否则抛错。
- 对象键带主机段:`<prefix>/<host>/<instance>/…`(前缀与主机段登记在 [deploy 身份、路径与 S3 前缀](../../deploy/README.md#身份路径与-s3-前缀),布局归 [journal-archiver](../../apps/journal-archiver/README.md));查询按 `(host, instance)` 选数据,dev2 与升级主机的同名实例是两份数据。测试网联调实例的数据在独立前缀 `hft-testnet/<host>/prod/`,用 `s3_root="s3://gwx-trading-data/hft-testnet"` 读;测试网账户的成交只用于接口与执行质量诊断,不作为策略表现。模拟实例(dev2 或升级主机)照常在 `hft/<host>/sim/` 下。

### 时间戳与口径规则

- 五个时间列语义固定([event-model](../../docs/subsystems/event-model.md)):`ex_ts_ns`、`ex_send_ns`、`hw_rx_ns`、`rx_ns`、`proc_ns`。每个查询函数要求显式 `time_col`,默认 `rx_ns`。
- **不同数据集不混算**:`latency()` 按 `(venue, stage)` 分组返回,拒绝把不同 venue 或不同 stage 合并成一个分位数;`hw_rx_ns = 0` 或软件时间戳的样本单独成组;不同主机类(`dev2`/升级主机)的延迟不合并。
- 时钟状态:`d.clock_ok(t0, t1)` 读 `ClockStatus`;offset > 200 µs 的时段内跨所时间差类结果标记不可信。
- 回放顺序只用 `seq`;按时间排序只用于展示。

### 研究查询

| 函数 | 输出 | 用途 |
|---|---|---|
| `align_bbo(a, b, tolerance_ns, time_col)` | as-of 对齐的双边 BBO | 跨所价差、tick 对齐 |
| `basis_series(asset, perp_venue, spot_venue, freq)` | 永续 × 现货的基差序列(买卖两侧分别给出) | 跨所基差与资金费研究 |
| `funding_history(asset, venue, n)` | 已实现资金费与实际结算周期 | 资金费研究、结算周期变化 |
| `latency_table(stage, by=["venue"])` | p50/p90/p99/p99.9/max 与计数 | 延迟报告 |
| `fills_pnl(account, t0, t1)` | `hft_py.pnl()` 的分解(资金费、基差、手续费、相对到达价的滑点、`adjust`) | 与 ops_bot `/pnl` 同口径 |
| `signal_funnel(account, group)` | 意图 → 风控放行 → 发出 → 成交 各级计数与拒绝原因(`RejectBits`) | 利用率与漏斗诊断 |
| `exec_quality(account, t0, t1)` | 每个目标的切片数、成交率、两腿间隔、到达价滑点、`Stuck`/`Broken` 次数 | 执行层表现 |

所有函数是 LazyFrame → LazyFrame 的纯变换;重的聚合下推到 Parquet 扫描。

### 模拟结果分析

模拟账户的事件与实盘是同一组事件,按 `AccountMeta.kind` 区分([hft-sim](../../crates/hft-sim/README.md))。

```python
from hft_research import sim

sim.report(account: str, t0, t1) -> pl.DataFrame
# 每日一行:PnL 分解、成交率、两腿间隔、裸敞口(时长与峰值)、保证金占用与最高保证金率、换手

sim.compare(accounts: list[str], t0, t1) -> pl.DataFrame
# 多个模拟账户(不同参数)的并列对比;附各自的参数差异(params_at 解码后逐字段 diff)

sim.calibration(sim_account: str, live_account: str, t0, t1) -> CalibrationReport
# 同参数并行期间:按意图配对(同一策略输入事件 → 两边的意图与成交),给出
#   滑点差(相对到达价,按名义分桶)、成交率差、两腿间隔差、下单到回执延迟分布差;
#   以及建议的 SimAccountCfg 调整(latency_model 分布、consume_decay_ms、费率档),供人工确认后经 refdata 变更回写
```

- 校准结果只给建议,不自动改配置;回写 `SimAccountCfg` 走 refdata 变更([hft-cfg](../../tools/README.md) → `ReplaceRefdata`)。
- `replayer sim` 的排障重演产出的 journal 也能用本库读,但其结果**不作为收益结论**:`sim.report` 对来源为重演的 journal 抛错(除非传 `debug=True`,输出标"排障重演")。
- `replayer sim --ab <paramsA> <paramsB>`(默认关闭,规格归 [replayer](../../apps/replayer/README.md))只输出同一段已录 journal 上两组参数的**差值**,报告首行标注"相对比较,不是收益估计,不得用于定仓";本库只把它当作研究提案的参考附件,不把差值并入 `sim.report`/`sim.compare`,也不据此生成覆盖项数值。

### 研究提案

research 不直接下发。选中的参数改动经以下步骤:

1. notebook 在数据上形成假设与候选字段值(以策略 schema 的字段名表达)。
2. `hft-research propose --instance <模拟实例> --overrides <toml> --note <说明>`:写成 pipeline 的带到期时间的覆盖项提案(S3 `hft/<host>/sim/private/params/proposals/`,`<host>` 是该模拟实例所在主机;由研究者以有该前缀写权限与 `kms:GenerateDataKey` 的个人凭证执行;`private/` 用 SSE-KMS,[deploy S3 前缀](../../deploy/README.md#身份路径与-s3-前缀))。
3. 交易员在 ops_bot 审阅后,经 `/params <模拟实例> set …` 先作用于一个**模拟账户**实例([pipeline 参数通道](../pipeline/README.md#参数通道));观察期结束后用 `sim.compare` 与 `sim.calibration` 出报告。
4. 报告通过后,交易员对实盘实例发同样的修改(二次确认);provenance 记录提案 id。

## 实现说明

<details><summary>实现要点</summary>

- DuckDB 用 `httpfs` 读 S3(运维只读角色 `hft-ops-read`,含读 `private/` 所需的 `kms:Decrypt`,[deploy](../../deploy/README.md#身份路径与-s3-前缀));Polars 用 `scan_parquet`;两者共享 `cache_dir`(对象按 manifest 中的 SHA-256 命名,命中即不下载,下载后核验哈希)。
- 已封存/未封存的分界由 manifest 是否存在决定,而不是由时钟推断。
- `hft_py` 调用(journal 读取、`pnl`、参数解码)全部经 PyO3 调 Rust,同一口径;Python 侧不复刻这些逻辑。
- notebook 放 `python/research/notebooks/`,提交前用 `nbstripout` 清输出;可复用的函数必须进 `src/hft_research/`。

**规划的源文件**

| 文件 | 职责 |
|---|---|
| `src/hft_research/data.py` | `Data`:分界、读取、拼接、去重 |
| `src/hft_research/s3cache.py` | 本地缓存与哈希核验 |
| `src/hft_research/timecols.py` | 时间列与口径检查 |
| `src/hft_research/queries.py` | 研究查询函数 |
| `src/hft_research/sim.py` | 模拟账户报告、对比、校准 |
| `src/hft_research/propose.py` | 提案写入 |
| `src/hft_research/cli.py` | `hft-research propose` 等 |

</details>

## 延迟与资源契约

离线,无热路径约束。

| 指标 | 目标 |
|---|---|
| 单 symbol 单日 `bbo` 查询(缓存命中) | < 2 s |
| `sim.report` 单账户 30 天(缓存命中) | < 30 s |
| `sim.calibration` 单对账户 7 天 | < 2 min |

## 测试与验收

| 层 | 用例 |
|---|---|
| 拼接 | 构造"前 N 小时 Parquet + 当前小时 journal"且有重叠 seq → 无重复、按 seq 连续;journal 缺失 → 警告而非静默 |
| 口径 | `latency()` 传入两个 venue 且未分组 → 抛错;不同主机类不合并;模拟与实盘未声明 `mix_kinds` 就合计 → 抛错 |
| 一致性 | `fills_pnl` 与 ops_bot `/pnl` 相等;`sim.report` 与 watchtower/archiver 的模拟日报相等 |
| 校准 | 构造两账户同输入、已知滑点差的 journal 片段 → `calibration` 求出的差值与构造值一致 |
| 重演 | 对来源为 `replayer sim` 的 journal 调 `sim.report` → 抛错 |
| 缓存 | 对象 SHA-256 与 manifest 不符 → 删除缓存并抛错 |

验收:正式实盘开始后,至少一份同一校准对的模拟 vs 实盘校准报告产出并被采纳回写;现有 `analysis/` 中的 24 h 套利分析能用本库在新数据上重跑。

## 依赖

- 内部:`hft-py`(含 `params.load_schemas`/`decode`,参数解码只用它)、`python/_generated`(`gen-schema` 生成)、发布归档中的 `schemas/strategies/`。
- 外部:`polars`、`duckdb`、`pyarrow`、`boto3`、`numpy`;notebook 环境另加 `jupyterlab`、`matplotlib`(不进包依赖)。
- 禁止:`hftctl.py`(research 不发命令)、交易所私有接口、`hft_py` 的回放驱动用于收益评估。

## 延伸阅读

- [round8-decisions](../../design/_work/round8-decisions.md)、[08](../../design/08-数据面与回放.md)、[research/06](../../design/research/06-data-plane-replay-observability.md)
- [journal-archiver](../../apps/journal-archiver/README.md)(布局与 manifest)、[hft-sim](../../crates/hft-sim/README.md)、[replayer](../../apps/replayer/README.md)、[hft-py](../../crates/hft-py/README.md)、[pipeline](../pipeline/README.md)

## 已知限制与后续工作

- 当日 journal 在交易主机上;研究主机读取需要只读同步(`rsync` 每 5 min 同步冷副本)或只研究已封存小时。
- 只读新框架的交易 journal,它只覆盖交易所需的订阅;大范围行情研究由独立的现有 collector 提供(不同 schema 与前缀),两者对照时按 `ex_ts_ns` 与接收 UTC 对齐。
- ClickHouse 热查询层按量触发再上;接口保持 `Data` 不变。

### 开发备忘

<details><summary>给实现者</summary>

- `with_float` 派生列只用于画图与探索;任何回写到参数的数值必须按 schema 从十进制重新生成。
- `sim.calibration` 的意图配对依赖模拟与实盘实例收到同一输入事件序列;两边参数版本不同的时段要剔除(用 `params_at` 判定)。

</details>
