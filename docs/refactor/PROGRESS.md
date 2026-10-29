# 引擎重构进度(`refactor/engine-v2`)

本文件是重构进度的唯一来源。设计稿在仓外:`E:\trade\handoff\hft-refactor-design-2026-10-29.md`(下称"设计",章节号指它)。每次会话开始先读本文件,结束前更新;里程碑完成时打 tag `refactor-m<N>` 并在下表附验收输出。决策理由进 Agent Note:[replay-as-research-tool](../../.agents/notes/proposed/architecture/2026-10-29-replay-as-research-tool.md)、[account-set-per-instance](../../.agents/notes/proposed/architecture/2026-10-29-account-set-per-instance.md)。

## 本周汇报(2026-10-30)

分支 `refactor/engine-v2` 从 `main` 6f6f420f 切出。M0 的四个工作项代码与文档已提交,本地(Windows,`cargo test`)相关 crate 测试全绿;尚缺两项只能在服务器上完成的验收(见 M0 表"待验收"),需要负责人安排。M1 的研究回放主体(`hft-replay` + `replayer research`)与 M3 的特征层(`hft-feature` + binary-maker 迁移,由并行代理在独立 worktree 完成后合入)已提交;两者在仓内的机制与确定性用例全绿,与实盘/`bm_sim.py` 的对账需要研究机。

| 完成 | 证据 | 下一步 | 风险 |
|---|---|---|---|
| 解码器校准字段合入、UTC 线改用 sequencer 实现 | `cargo test -p pm-md-probe` 4/4;`xtask deps`/`test-trace` PASS | 研究机上对 1 份 journal 与本地校准构建 diff | 无 journal 在本机 |
| REST `sent_tsc` = 写出时刻,REST 编码进 `EncodeStart/End` | `cargo test -p hft-net -p hft-venue` 全绿,新 VEN-409 | 实盘箱 24 h 看 `egress` 新分段 | 需负责人批准换 bundle |
| `pm_sim_config --live` 默认预签名,`--no-presign` | `cargo test -p hft-core --features polymarket --test config` 23/23 | 同上,开/关各 24 h | 同上 |
| 基线修订两条 Agent Note,AGENTS.md / design 07 §9 / 08 §5 同步 | doc-check 无新增问题 | — | — |
| `replayer research`:同一 sequencer + 策略回放 journal,被模拟账户由 hft-sim 撮合,变体 × 种子,Parquet | RPL-300–303、RSR-001–003 绿;replayer 全量 62/62;不模拟时与日志决策逐字段一致(RPL-301);release 约 23 万记录/秒(4.3 µs/记录,DST 负载) | 研究机:与 `bm_sim.py` 同参数同固定延迟对账、10 M 事件计时 | DST 负载不等于 PM 负载 |
| `hft-feature` + binary-maker 迁移 | BMK-034(旧代码先录 4 组参数的决策轨迹哈希,迁移后不变)、BMK 35/35、FEA 10/10;`tick_comp` 2.06 µs(旧 2.18) | `replayer verify` 对最近 7 天实盘 journal | 无 binary-maker 回放样本在仓内 |

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
| `feat/replay-research`:`ResearchDriver` 流式读、账户屏蔽、变体表、现有 hft-sim queue 模型、Parquet sink、多种子 + 区间重播种 | 待验收 | aa103ce9(hft-sim glue、`reseed_account`、`RepMsg::with_parts`)、d5f294f4 | RPL-300–303、RSR-001–003;**待**(研究机):(a) 28 份 journal 的决策一致率、(b) 与 `bm_sim.py` 逐窗相关 ≥ 0.95、(c) 10 M 事件 < 60 s;`--own` 与 `parity` 子命令未做 |
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
| `feat/hft-feature`:`Feature` 处理器 + 按需求值、定点导出列;节点库(波动环、参考中间价/失衡、秒级历史、大单流、剔除自有挂单的簿侧、lag 分位、z 分数、Φ/Φ⁻¹、isqrt、atanh/ln) | done | 3eb165ec | FEA-001–010;`feature_dag!` 宏未做(R-7:手写执行顺序) |
| `strat/bm-features-dag`:binary-maker 用 `MakerFeatures`,删除内联实现 | 待验收 | ff8c6fa3(BMK-034 先录旧轨迹)、3887be3f、合并 41281161 | BMK-034/035;快照字节与 `DESC_VERSION` 不变;**待**:`replayer verify` 对最近 7 天实盘 journal |
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
| SIM-230、SIM-231 | M1 | hft-sim journal 衔接;`reseed_account` |
| RSR-001–RSR-003 | M1 | 变体规格、Parquet 写出、被替换回报的账户字段 |
| RPL-300–RPL-303 | M1 | 研究回放确定性、不模拟时与日志一致、变体、CLI |
| FEA-001–FEA-010 | M3 | 特征节点 |
| BMK-034、BMK-035 | M3 | 决策轨迹哈希、特征导出行 |

## 执行中的技术决定

| 日期 | 决定 | 理由 |
|---|---|---|
| 2026-10-29 | 解码器依赖 `hft-sequencer`/`hft-strategy-sdk` 以复用 `UtcLineRow`,不在工具里复制 UTC 线 | 设计 R7"时钟只有一处实现"的第一步;工具是 cold tier,依赖闭包增大可接受 |
| 2026-10-29 | `RestPool::pending_sent` 与既有 `pending_conn` 并存,`submit` 签名不变 | 不改 hft-net 的公开提交接口与其测试;按标签取写出时刻 |
| 2026-10-30 | 研究回放中被模拟账户每个 (账户, venue, 资产) 保留第一条日志 `BalanceReport` | 账本现金从第一条 BalanceReport 初始化(`UNINIT`);全部屏蔽时 DST 里的第二腿 `LINK_FUNDS` 拒单 |
| 2026-10-30 | 研究回放丢弃日志里 owner ≠ Sys 的 `Timer`,由引擎堆触发 | 回放的订单流与实盘不同,堆定时器(OMS、风控、执行层、策略)必须跟随回放状态;Sys 定时器(滚段、日切、关键帧)是环境 |
| 2026-10-30 | 变体字段只作用于 schema 含它的被模拟实例;所有实例都没有才报错 | 一个 journal 可有多种策略;拼写错误仍会被发现 |
| 2026-10-30 | `hft-replay` 标 `scope: optional` | 上线不依赖研究回放(Agent Note:门槛不变) |
| 2026-10-30 | M3 由并行代理在 `E:\hft-m3` worktree 完成后合入 | 设计 §8.6:M1 ∥ M3,crate 不相交 |
| 2026-10-30 | M0 的两项服务器验收未完成时开始 M1 | M1 的代码不依赖这两项测量(解码器 diff、`egress` 分段);它们只阻塞 `refactor-m0` tag,不阻塞开发
| 2026-10-29 | 本机可编译与测试(Windows,Rust 1.97.1),每个工作项提交前在本机跑受影响包的测试 | 比"只推 CI"更快发现错误;结论仍以 CI 为准 |
