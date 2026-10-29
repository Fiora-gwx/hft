# 引擎重构进度(`refactor/engine-v2`)

本文件是重构进度的唯一来源。设计稿在仓外:`E:\trade\handoff\hft-refactor-design-2026-10-29.md`(下称"设计",章节号指它)。每次会话开始先读本文件,结束前更新;里程碑完成时打 tag `refactor-m<N>` 并在下表附验收输出。决策理由进 Agent Note:[replay-as-research-tool](../../.agents/notes/proposed/architecture/2026-10-29-replay-as-research-tool.md)、[account-set-per-instance](../../.agents/notes/proposed/architecture/2026-10-29-account-set-per-instance.md)。

## 本周汇报(2026-10-30)

分支 `refactor/engine-v2` 从 `main` 6f6f420f 切出。M0 的四个工作项代码与文档已提交,本地(Windows,`cargo test`)相关 crate 测试全绿;尚缺两项只能在服务器上完成的验收(见 M0 表"待验收"),需要负责人安排。

| 完成 | 证据 | 下一步 | 风险 |
|---|---|---|---|
| 解码器校准字段合入、UTC 线改用 sequencer 实现 | `cargo test -p pm-md-probe` 4/4;`xtask deps`/`test-trace` PASS | 研究机上对 1 份 journal 与本地校准构建 diff | 无 journal 在本机 |
| REST `sent_tsc` = 写出时刻,REST 编码进 `EncodeStart/End` | `cargo test -p hft-net -p hft-venue` 全绿,新 VEN-409 | 实盘箱 24 h 看 `egress` 新分段 | 需负责人批准换 bundle |
| `pm_sim_config --live` 默认预签名,`--no-presign` | `cargo test -p hft-core --features polymarket --test config` 23/23 | 同上,开/关各 24 h | 同上 |
| 基线修订两条 Agent Note,AGENTS.md / design 07 §9 / 08 §5 同步 | doc-check 无新增问题 | — | — |

## 约定

- 一条分支,里程碑只是分支内的阶段;每周把 `main` merge 进本分支。
- 每个工作项:README/TESTS 与代码同一提交;新增用例记在下面"待批准用例",负责人批量批准。
- 本地验证口径:Windows 工作站 `cargo test -p <受影响的包>` + `cargo xtask deps|hot-check|test-trace|doc-check`;结论以 CI 为准(仓规 CI 是唯一构建环境)。
- 并行:关键路径 M0 → M1 → M2 → M4;M1 ∥ M3、M2 ∥ M3、M4 ∥ M5;并行工作项落在不相交的 crate 上。

## 里程碑与工作项

状态:todo / doing / blocked / done / 待验收(代码完成,验收需要服务器或负责人)。

### M0 冻结与补测

| 工作项 | 状态 | 提交 | 验收 |
|---|---|---|---|
| `fix/pm-md-probe-calib-fields`:`journal` 子命令的 `u/e/sq/eu/org/lv/t_ns/trig/reason/ino/oflags`、`bbo`/`eclk`/`anchor` 行合入主干;引擎 UTC 用 `hft_sequencer::clock::UtcLineRow`(不再复制常数) | 待验收 | 8db12c10、15fe4a7d(本地分支 `tmp/journal-decoder-v3` 原样 cherry-pick)、02f55de9 | PMP-005 本地绿;**待**:研究机上同一 journal 与 v3 本地构建输出 diff = 0 |
| `feat/gw-rest-encode-stamps`:REST 下单 `SendReport.sent_tsc` 与 `RawTx` 取 `sendmsg` 返回时刻(`RestPool::pending_sent`);REST 编码含签名在 `EncodeStart/EncodeEnd` 之间 | 待验收 | ce401894 | VEN-409 本地绿;**待**:实盘 `egress` p50 现签 ≈ 80–90 µs / 预签 ≈ 10 µs |
| `ops/pm-presign-default-on`:`pm_sim_config --live` 默认写 `venues.pm.presign` = (2, 2, `0.01`, 60 000);`--no-presign` | 待验收 | b61dae5a | CORE-445.4 本地绿;**待**:开/关各 24 h 的 `egress` A/B(方法:同一参数,`--no-presign` 与默认各一段运行,`replayer latency` 或 journal 的 `SendReport.sent_tsc − OrderDecision.decided_tsc` 分位数) |
| `docs/baseline-amendment`:两条 Agent Note;AGENTS.md 两条规则;design 07 §9、08 §5;旧 Note 状态行 | done | 见本提交 | — |

### M1 研究回放 MVP + FAK

| 工作项 | 状态 | 提交 | 验收 |
|---|---|---|---|
| `feat/replay-research`:`ResearchDriver` 流式读、账户屏蔽、`--own`、变体表、现有 hft-sim queue 模型、Parquet sink、`parity` 子命令 | todo | | 设计 §8.2 M1 (a)–(e) |
| `feat/sdk-views-stamps`:`MD_BBO` stamp、`stamps()`、`open_orders` | todo | | |
| `feat/sdk-fak`:`Tif::Fak` | todo | | |

### M2 VenueSim 高保真 + 校准 + 吃单执行

| 工作项 | 状态 | 提交 | 验收 |
|---|---|---|---|
| `feat/venue-sim-pm` | todo | | 设计 §8.2 M2 |
| `feat/replay-calib` | todo | | |
| `feat/paper-venue-sim` | todo | | |
| `feat/pm-fak-gw` | todo | | 实盘探针需负责人批准 |

### M3 特征层

| 工作项 | 状态 | 提交 | 验收 |
|---|---|---|---|
| `feat/hft-feature` | todo | | 设计 §8.2 M3 |
| `strat/bm-features-dag` | todo | | 决策逐字节不变 |
| `feat/replay-features` | todo | | |

### M4 多账户 + Shadow 变体

| 工作项 | 状态 | 提交 | 验收 |
|---|---|---|---|
| `feat/refdata-account-set` | todo | | 设计 §8.2 M4 |
| `feat/gw-per-account` | todo | | |
| `feat/router-stp-agg-risk` | todo | | |
| `feat/prod-sim-accounts` | todo | | |
| `strat/bm-multi-account` | todo | | |

### M5 时钟 / 导出 / 退役

| 工作项 | 状态 | 提交 | 验收 |
|---|---|---|---|
| `fix/utc-line-single` | todo | | 设计 §8.2 M5 |
| `feat/hft-py-x86-wheel` | todo | | |
| `feat/archiver-parquet`(可选) | todo | | |
| `ops/retire-decoder` | todo | | |

### M6 清理 + 合入 main

| 工作项 | 状态 | 提交 | 验收 |
|---|---|---|---|
| `feat/run-mode-enum` | todo | | |
| `feat/hft-pmcfg` | todo | | |
| `docs/baseline-v2` | todo | | |

## 回归 journal 集

设计 §8.6 第 4 条建议:`j3_101614`、`j3_202114`、`j3_v6full`、`j3_v7live`、`j3_103721`、`s3_011040`、`s3_185413`(研究机 `~/data/jall/`)。哈希:待记录(需要在研究机上 `sha256sum`;本机没有这些文件)。

## 待批准用例

| 用例 | 工作项 | 内容 |
|---|---|---|
| PMP-005 | M0 解码器 | `journal` 的引擎 UTC 与 sequencer UTC 线一致 |
| VEN-409 | M0 REST 打点 | REST 订单 `sent_tsc`/`RawTx` 取写出返回时刻 |
| CORE-445.4 | M0 预签名默认 | 实盘配置默认预签名、`--no-presign` 不写 |

## 执行中的技术决定

| 日期 | 决定 | 理由 |
|---|---|---|
| 2026-10-29 | 解码器依赖 `hft-sequencer`/`hft-strategy-sdk` 以复用 `UtcLineRow`,不在工具里复制 UTC 线 | 设计 R7"时钟只有一处实现"的第一步;工具是 cold tier,依赖闭包增大可接受 |
| 2026-10-29 | `RestPool::pending_sent` 与既有 `pending_conn` 并存,`submit` 签名不变 | 不改 hft-net 的公开提交接口与其测试;按标签取写出时刻 |
| 2026-10-30 | M0 的两项服务器验收未完成时开始 M1 | M1 的代码不依赖这两项测量(解码器 diff、`egress` 分段);它们只阻塞 `refactor-m0` tag,不阻塞开发
| 2026-10-29 | 本机可编译与测试(Windows,Rust 1.97.1),每个工作项提交前在本机跑受影响包的测试 | 比"只推 CI"更快发现错误;结论仍以 CI 为准 |
