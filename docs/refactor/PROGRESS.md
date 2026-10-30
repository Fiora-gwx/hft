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
| `feat/replay-research`:`ResearchDriver` 流式读、账户屏蔽、变体表、现有 hft-sim queue 模型、Parquet sink、多种子 + 区间重播种;`parity` 子命令 | 待验收 | aa103ce9(hft-sim glue、`reseed_account`、`RepMsg::with_parts`)、d5f294f4、b34add60(parity) | RPL-300–304、RSR-001–003;**待**(研究机):(a) 28 份 journal 的 `replayer parity` 一致率、(b) 与 `bm_sim.py` 逐窗相关 ≥ 0.95、(c) 10 M 事件 < 60 s;`--own` 移到 M2(依赖模拟场所的自有挂单叠加层) |
| `feat/sdk-views-stamps`:`bbo_stamp`(最近被应用的 Bbo 的 `MdStampView`,`MD_BBO`)、`stamps()`、`open_orders` | done | 10f96b40 | SEQ-448、SEQ-449、SDK-214;`md_count`/`md_stamp` 语义不变(BBO 戳单独存,不挤占既有环) |
| `feat/sdk-fak`:`Tif::FAK`(= `Ioc` 的别名) | done | 10f96b40 | SDK-214.3;Polymarket 把 `Ioc` 映射为 FAK 订单在 M2 `feat/pm-fak-gw` |

### M2 VenueSim 高保真 + 校准 + 吃单执行

| 工作项 | 状态 | 提交 | 验收 |
|---|---|---|---|
| `feat/venue-sim-pm`:hft-sim `VenueModel`(默认全关,默认下回报与 `state_hash` 不变):`MakerFill::QueueCap`、撤单竞速与墓碑(`cancel_us`、`tombstone`)、吃单延迟与慢尾(`taker`)、二元吃单费 `FeeModel::BinaryTaker`、GTD 到期、条件延迟池(`set_condition`);hft-replay 的条件计算(上一完整分钟 md lag p90 三分位、1 s 参考价变动、窗口相位)与 `--own` 叠加层 | 待验收 | 648ea1ce、5452840a(并行代理,合并 76a8bd8f) | SIM-232–239、RSR-004、RPL-305–306;**待**(研究机):§5.2 对账(88 窗每窗 \|Δ\| ≤ 0.3、撤单后成交比例 ≥ 6 %)。顺带修复 M1 的驱动从未调用 `on_utc`(区间重播种未生效) |
| `feat/replay-calib`:`replayer calib`(Ack/CancelAck 延迟池 + 条件标签 + 三分位阈值)、`--calib`(含研究侧 `latc.json` 格式) | 待验收 | 67ed01fe | RPL-307;吃单延迟、成交回报延迟与费率不由它估计 |
| `feat/paper-venue-sim`:sim-gw 用同一个 hft-sim `VenueModel` 与 `CondTracker`;`[sim] calib` 引用校准表(与 `research --calib` 同一份解析) | 待验收 | 965f20ba、94c7ef08 | SIM-240、CORE-447;"行情旁路环"不做:sim-gw 已从 seq journal 尾读行情,顺序与引擎一致,另开环只省一次读;**待**:`sim` 实例带校准表 2 账户 × 2 变体 48 h 长跑(服务器)
| `feat/pm-fak-gw` | blocked | | 仓内没有可核实的 Polymarket marketable/FAK 线上格式(金额/份额取整、`orderType`、过期);需要 py-clob-client-v2 `create_market_order` 的 BUY/SELL 金样(按 tick 的金额与份额取整及请求体)。实盘探针需负责人批准 |

### M3 特征层

| 工作项 | 状态 | 提交 | 验收 |
|---|---|---|---|
| `feat/hft-feature`:`Feature` 处理器 + 按需求值、定点导出列;节点库(波动环、参考中间价/失衡、秒级历史、大单流、剔除自有挂单的簿侧、lag 分位、z 分数、Φ/Φ⁻¹、isqrt、atanh/ln) | done | 3eb165ec | FEA-001–010;`feature_dag!` 宏未做(R-7:手写执行顺序) |
| `strat/bm-features-dag`:binary-maker 用 `MakerFeatures`,删除内联实现 | 待验收 | ff8c6fa3(BMK-034 先录旧轨迹)、3887be3f、合并 41281161 | BMK-034/035;快照字节与 `DESC_VERSION` 不变;**待**:`replayer verify` 对最近 7 天实盘 journal |
| `feat/replay-features`:SDK `Strategy::FEATURES`/`export_features`(冷,缺省无)与 `StrategySet` 分派;`ColumnDesc` 等移到 SDK、hft-feature 再导出;`replayer features`(parity 式运行,决策点 + UTC 网格,可空列带 `unit`/`exp` 元数据)与 `research --features` | 待验收 | 0862567f | SDK-215、RSR-005、RPL-308、BMK-035.3;**待**(研究机):`features.parquet` 与 `fac_panel.py` 同窗同特征差 ≤ 1 定点单位(binary-maker 目前只导出 7 列;面板其余特征要先固化成节点);`--dag` 选项未做(列来自策略自己的 `FEATURES`,无需另选) |

### M4 多账户 + Shadow 变体

| 工作项 | 状态 | 提交 | 验收 |
|---|---|---|---|
| `feat/refdata-account-set`:refdata 接受一个实例绑定多个账户(`accounts_of`,主账户 = id 最小);sequencer 实例账户集合(≤ 8)覆盖 cell、模式、风控组表、执行层 cell、halt、行情闸门、连接通知、StrategyCmd;SDK `RefView::accounts`;hft-core 每实例一份参数 blob | done | b5155926 | REF-019(改写)、SEQ-451;单账户行为不变(sequencer 515、exec 283、refdata 84) |
| `feat/gw-per-account`:槽镜像按 (账户, 槽);hft-core 每账户基线、账户 REST 客户端、gw REST 池与私有连接(hft-net `BuildReq.account`);`AccountCapacity` 放宽到 12;`accounts[].src_ip`;撤单独立限速(`LimitBucket::Cancels`,PM 下单 20/s、撤单 40/s);`pm_sim_config --live-account` | 待验收 | c4100bac、78e8b906、55974594、091e3199(并行代理,合并 a4b9f99c) | VEN-411–413、CORE-446、PMK-037、REF-075.6/REF-009.23(改写);**未做**:hft-core 层 conn-builder 账户路由测试(参照的 mock-REST 测试在 Windows 不运行,由 VEN-413 在 gw 层覆盖);**注意**:gw 连接号 112–255 共 144 个,12 个 PM 账户(各约 11 条)接近上限;旧二进制读含 `Cancels` 的 refdata 会拒绝加载;**待**:实盘 2 账户 Shadow(需负责人批准)
| `feat/router-stp-agg-risk` | doing | b6796b5f(STP) | SEQ-452:策略下单与同实例其他账户的挂单(同 venue/inst、反向、价格可达)交叉时 `RejectBits::SELF_TRADE`,在风控之前;**未做**:`RoutePolicy` 路由、互补 token 的 STP(需要 refdata 的互补关系字段,负责人决定)、实例聚合限额(需要改 RiskParams `RKP1` 格式,负责人决定) |
| `feat/prod-sim-accounts` | 待验收 | 67bca491(hft-risk:Sim 账户 `ORDER_CONNS` 不要求热备)、ec3ec88a(`MAX_SIM_ACCOUNTS` 64)、9e5a096d、9359cfc9(RPL-309) | CORE-449、CORE-450、RSK-101.18、RPL-309 本地绿(Windows `hft-core --features polymarket` 全绿;WSL Ubuntu-22.04 `clippy`/`test -p hft-core --features polymarket` EXIT 0;`hft-risk`、`hft-sim`、`replayer`、`hft-testkit`、`hft-sequencer`、`hft-pmcfg`、`hft-replay` 全绿;`xtask deps`/`hot-check`/`test-trace`/`doc-check` PASS,doc-check 问题 0、警告数与改动前相同)。`prod` 配置无 Sim 账户时装配、`--check`、journal 不变;无 journal 字节/快照布局变化。**未做**:hft-pmcfg 生成带 Shadow 变体的 PM 实盘配置(`--shadow`)、`sim-venue` 的进程级(Linux)测试;**待**:实盘 Shadow 变体 ≥ 2 天(需负责人批准)、`sim` 实例 2 账户 × 2 变体 48 h;**负责人决定**:基线文档仍写"`prod` 不含 Sim 账户"(根 `AGENTS.md`、`apps/AGENTS.md`、Agent Note 2026-10-03-sim-only-instance、deploy/glossary/defensive-patterns/journal-format、design 01/07),未改,需批准后同步(可并入 `docs/baseline-v2`);`prod` 的 Sim 账户与 `sim` 实例的账户名同为 `sim-…`,可能重名(ops_bot 按名路由) |
| `strat/bm-multi-account` | todo | | |

### M5 时钟 / 导出 / 退役

| 工作项 | 状态 | 提交 | 验收 |
|---|---|---|---|
| `fix/utc-line-single`:`UTC_SLEW_PPM`/`UTC_RATE_TOL_PPM` 只在 hft-rt 定义,sequencer 复用;两条线机制不同(引擎线超前量衰减、进快照;发布线降倍率)保持,交叉验证 | 待验收 | b597310c | SEQ-450;**待**:24 h 漂移 < 1 ppm(实盘 `UtcAnchor` 残差);"第二份常数"的机检未加(常数已改为再导出) |
| `feat/hft-py-x86-wheel`:`research-build.yml`(x86_64 `replayer` + `hft_py` wheel,`HFT_GIT_SHA`);`events()` 按 `t0` 选起点段 | 待验收 | dd2d3f3d | PY-170;**待**:CI 上跑一次 workflow,研究机 `pip install` 冒烟 |
| `feat/archiver-parquet`(可选) | todo | | |
| `ops/retire-decoder` | todo | | |

### M6 清理 + 合入 main

| 工作项 | 状态 | 提交 | 验收 |
|---|---|---|---|
| `feat/run-mode-enum`:`config::Assembly{Live{prod_orders}, Sim}` 启动时解析一次,配置校验、日志预算、`runtime::run`、venue 装配都读它 | done | 9ce2b6b3 | CORE-448;`--check` 新旧二进制对仓内 3 份配置与生成的 PM sim/maker/live/双账户配置结果相同(含拒绝路径的错误码与退出码);回放装配属于 replayer 的 driver,不进此枚举 |
| `feat/hft-pmcfg`:Polymarket 配置生成器从 `apps/hft-core/tests/support` 移到 `tools/hft-pmcfg`(lib + bin `hft-pmcfg`);`--account LABEL:UID:ROLE KEYS_DIR`(可重复,ROLE 目前只收 `any`),`--live-account` 保留为简写;hft-core `tests/support` 为薄包装、`pm_sim_config` example 为同一命令行的薄壳;hft-pmcfg 成为 registry 第三个使用方;scaffold-gate 复制全部 registry 使用方的清单 | done | 37331170(scaffold-gate)、475e93a8 | PMC-001–006 6/6;hft-core(`--features polymarket`)全绿;迁移前 `pm_sim_config` 与迁移后 `hft-pmcfg`/shim 7 组命令行 42 个文件 `cmp` 全同;`xtask deps`/`hot-check`/`test-trace`/`doc-check` PASS(doc-check 问题数 0、警告数与改动前相同);本地 `scaffold-gate --keep` 构建/DST/`--check` 各步通过(只因工作区未提交而在第 4 步报 FAIL) |
| `docs/baseline-v2` | doing | 2c41d208(architecture §7 L5、§9 研究回放) | replayer、hft-replay、hft-sim、hft-sequencer、hft-strategy-sdk 的 README 已随各工作项对齐;**待**:合入 main 前全仓再读一遍(M4 余项完成后) |

## 回归 journal 集

设计 §8.6 第 4 条建议:`j3_101614`、`j3_202114`、`j3_v6full`、`j3_v7live`、`j3_103721`、`s3_011040`、`s3_185413`(研究机 `~/data/jall/`)。哈希:待记录(需要在研究机上 `sha256sum`;本机没有这些文件)。

## 待批准用例

| 用例 | 工作项 | 内容 |
|---|---|---|
| PMP-005 | M0 解码器 | `journal` 的引擎 UTC 与 sequencer UTC 线一致 |
| VEN-409 | M0 REST 打点 | REST 订单 `sent_tsc`/`RawTx` 取写出返回时刻 |
| CORE-445.4 | M0 预签名默认 | 实盘配置默认预签名、`--no-presign` 不写 |
| SIM-230、SIM-231 | M1 | hft-sim journal 衔接;`reseed_account` |
| SEQ-448、SEQ-449、SDK-214 | M1 | `bbo_stamp`/`stamps`、`open_orders`、提供的默认方法与 `Tif::FAK` |
| RPL-304 | M1 | parity |
| SIM-232–SIM-239、RSR-004、RPL-305–RPL-307 | M2 | 场所模型、校准、`--own`、`replayer calib` |
| REF-019(改写)、SEQ-451、VEN-411、SEQ-452 | M4 | 账户集合、gw 每账户镜像、自成交防护 |
| SEQ-450、PY-170 | M5 | UTC 线交叉验证、起点段 |
| RSR-001–RSR-003 | M1 | 变体规格、Parquet 写出、被替换回报的账户字段 |
| RPL-300–RPL-303 | M1 | 研究回放确定性、不模拟时与日志一致、变体、CLI |
| FEA-001–FEA-010 | M3 | 特征节点 |
| SDK-215、RSR-005、RPL-308 | M3 | 策略特征导出钩子、可空特征列、`replayer features` |
| SIM-240、CORE-447 | M2 | 条件跟踪的三个分量、`[sim] calib` |
| CORE-448 | M6 | 运行装配 |
| CORE-446、VEN-412、VEN-413、PMK-037(新增);CORE-441.5/.6、REF-075.6、REF-009.23(改写) | M4 | 多 Live 账户 gw 装配、撤单限速桶 |
| CORE-449、CORE-450、RSK-101.18、RPL-309(新增);CORE-040.3/.4、CORE-045.1/.2、SIM-002、SIM-003.17/.18(改写) | M4 | `prod` 实例内的 Sim 账户与 sim-venue、决策屏障只对 Sim 账户、Sim 账户不要求热备、含 Sim 账户的 prod journal verify、`MAX_SIM_ACCOUNTS` 64 |
| BMK-034、BMK-035 | M3 | 决策轨迹哈希、特征导出行 |
| PMC-001–PMC-006(新增);TOOL-021(期望扩展:hft-pmcfg 清单副本) | M6 | hft-pmcfg 命令行、`--account`、多账户输出、工具 = 库逐字节、引擎基底 = SeqRig |

## 执行中的技术决定

| 日期 | 决定 | 理由 |
|---|---|---|
| 2026-10-29 | 解码器依赖 `hft-sequencer`/`hft-strategy-sdk` 以复用 `UtcLineRow`,不在工具里复制 UTC 线 | 设计 R7"时钟只有一处实现"的第一步;工具是 cold tier,依赖闭包增大可接受 |
| 2026-10-29 | `RestPool::pending_sent` 与既有 `pending_conn` 并存,`submit` 签名不变 | 不改 hft-net 的公开提交接口与其测试;按标签取写出时刻 |
| 2026-10-30 | 研究回放中被模拟账户每个 (账户, venue, 资产) 保留第一条日志 `BalanceReport` | 账本现金从第一条 BalanceReport 初始化(`UNINIT`);全部屏蔽时 DST 里的第二腿 `LINK_FUNDS` 拒单 |
| 2026-10-30 | 研究回放丢弃日志里 owner ≠ Sys 的 `Timer`,由引擎堆触发 | 回放的订单流与实盘不同,堆定时器(OMS、风控、执行层、策略)必须跟随回放状态;Sys 定时器(滚段、日切、关键帧)是环境 |
| 2026-10-30 | Linux 专用代码(`run_linux` 等)在本机 WSL Ubuntu-22.04 离线检查:`CARGO_HOME` 的 registry 链到 Windows 的缓存,`cargo check/clippy/test --offline`,target 目录在 WSL 内 | Windows 上 aws-lc-sys 交叉编译缺 C 工具链;WSL 无外网;Ubuntu-24.04 的根分区被重挂为只读(未动) |
| 2026-10-30 | sim-gw 不另开行情旁路环,继续尾读 seq journal | 尾读即引擎看到的顺序,条件跟踪与撮合都按它;旁路环只省一次读,且要另处理与决策屏障的先后 |
| 2026-10-30 | 变体字段只作用于 schema 含它的被模拟实例;所有实例都没有才报错 | 一个 journal 可有多种策略;拼写错误仍会被发现 |
| 2026-10-30 | `hft-replay` 标 `scope: optional` | 上线不依赖研究回放(Agent Note:门槛不变) |
| 2026-10-30 | M3 由并行代理在 `E:\hft-m3` worktree 完成后合入 | 设计 §8.6:M1 ∥ M3,crate 不相交 |
| 2026-10-30 | `Tif::FAK` 是 `Tif::Ioc` 的别名,不新增枚举值 | FAK 与 IOC 订单语义相同;新增值要改全仓 `matches!(tif, Ioc | Fok)` 的十余处,漏一处即错;Polymarket 在适配器里把 Ioc 映射为 FAK |
| 2026-10-30 | BBO 戳用单独的 `bbo_stamp`,不进 `md_stamp` 环 | 放进环会改变 `md_count` 的计数语义与 SEQ-447;`MarketLastRow` 进快照、不能加字段 |
| 2026-10-30 | M0 的两项服务器验收未完成时开始 M1 | M1 的代码不依赖这两项测量(解码器 diff、`egress` 分段);它们只阻塞 `refactor-m0` tag,不阻塞开发
| 2026-10-29 | 本机可编译与测试(Windows,Rust 1.97.1),每个工作项提交前在本机跑受影响包的测试 | 比"只推 CI"更快发现错误;结论仍以 CI 为准 |
| 2026-10-30 | `prod` 实例的 sim-venue 尾读 seq journal 取行情(与 sim-gw 同一 `paper::Gateway`),不做 feed 任务的 md 旁路环 | 尾读即引擎看到的顺序,决策屏障顺序天然成立、条件跟踪与 sim-gw 相同;旁路环要改 feed 热路径(多写一份 `FeedMsg`)并另处理与屏障的先后;代价是 sim-venue 多读一次 seq journal(warm 线程、非隔离核) |
| 2026-10-30 | 任务名 `sim-venue` 只用于 `prod` 实例,`sim` 实例保持 `sim-gw` | 已部署的 sim 配置不改;两者同一实现 |
| 2026-10-30 | `prod` 实例的撮合设置沿用 `[sim]` 表(有 Sim 账户 ⇔ 有 `[sim]`),不得有 `quotes`、`quote_interval_ms`;`quote_interval_ms` 在 `prod` 形状里可省略,`sim` 实例仍必填(错误码不变) | 不新增配置键;`seed`、延迟、保证金模型与 `calib` 与 sim 实例同一份解析 |
| 2026-10-30 | hft-risk:Sim 账户的 `ORDER_CONNS` 不看热备 | hft-risk README 已写"模拟账户 `standby_up` 恒为真",代码只在 `allow_missing_standby` 时成立;`prod` 实例不能为 Sim 账户放宽 Live 账户的热备要求。`sim` 实例强制 `allow_missing_standby = 1`、DST 给 Sim 账户发热备,行为都不变 |
| 2026-10-30 | sim-venue 的"最低优先级"按线程 `nice` 不低于计划中任何线程判定,核隔离在线程启动时查 `/sys/devices/system/cpu/isolated` | 本装配一线程一任务,无法"与 housekeeping 同线程";`nice` 与核是现有配置能表达的两项 |
| 2026-10-30 | RPL-309 用 TK-058 的世界(两个 taker、账户 b 为 Sim)加 `WorldSpec.role_prod`,不用 RPL-012 的 `probe-target` | `probe-target` 的 Sim 账户在 RPL-012 世界里不下单(一次性计划点),验证不到 Sim 回报 |
