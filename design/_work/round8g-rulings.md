# M0-g 合并裁决(主控;合并代理 M-A…M-E 先读)

来源:`round8g-merge-notes.md`(22 个测试规格代理的报告,第 3 节"留给别人"、第 5 节"需要负责人决定")。本文件对第 5 节的问题逐条裁决;裁决原则:**资金安全取保守**、**不新增功能,只补齐规则**、**一件事实一个归属地**。每条裁决同时改 README(规则)与受影响的 `TESTS.md` 用例(期望值、前置、新增或删除用例)。上一轮裁决 RF1–RF18(`round8g-tests-brief.md` §4)继续有效。

## L0 与 journal(M-A)

- **RG1 GroupId 永不复用**:StrategyParams blob 头加 `group_ctr_hwm`(只增不减);新 group 的计数器从 max(本 blob 内最大值, 上一版本 `group_ctr_hwm`) + 1 续号,hft-cfg 写入新的 hwm;`--against` 缺失时拒绝新增 group。规则:构造归 hft-types,续号与 hwm 归 tools hft-cfg,pipeline 链接。
- **RG2 hk 环单写者**:hk 环的唯一生产者是 housekeeping 线程;advisor 的 Note(如 `AdvisorLag`)经 advice 环送给 sequencer;启动路径在 housekeeping 线程启动前写完,不并发。hft-event 环表、hft-sequencer、hft-core 一致。
- **RG3 seq journal 的 CRC**:按 T2 的取值(ControlCmd、ConfigBlob、StateSnapshot、RawRx、RawTx 带 CRC,其余不带)。
- **RG4 gen-schema 基线**:CI 以 PR 目标分支的合并基点为基线;`--check` 同时检查 kind_id、Note 码、命令 `op`、快照节 tag 的"只追加、不复用"(T5 Q2)。
- **RG5 `OrderEvent.detail: u8`**:占用偏移 3 的填充字节(大小不变,RF1);QueryResult 时为订单状态码,CancelReject/AmendReject 时为 GONE 分类;venue 的 `codes.rs` 产出。
- **RG6 `HostClass` 归 hft-types**(L0,refdata、rt、core 共用);hft-rt 与 hft-refdata 改为使用它。
- **RG7 refdata 新增字段**:`AssetMeta.dust`(资产级尘埃线,risk/ledger/reconciler 共用,唯一来源);`InstMeta` 增加分档维持保证金率表与交易所价格限制带(hft-sim 与压力用例使用;缺失时 hft-sim 报 `MmrTiersMissing`)。
- **RG8 RF6 收紧**:非正式账户只有在**同一交易所的不同 `VenueId`** 上 UID 相同才只告警;同一 `VenueId` 上重复仍是加载错误。告警经 `RefData::warnings()`,hft-core 启动时以 `Note{RefdataWarning}`(P1)写出(登记 hft-event)。
- **RG9 `DurableMeta` 移出 `writer` feature**(archiver `restore` 要用 `write_sync`);hft-journal 另提供 `examples/testwriter.rs`(启用 writer)供冷读者测试。
- **RG10 `journal.disk.warn_pct`/`crit_pct`** 归 hft-core 的存储阈值配置,hft-journal 删除这两个键。
- **RG11 Note 码 0 保留**,`NoteDesc.code = 0` 校验拒绝。
- **RG12 两条缺载体的告警**登记 Note:`StopMarkerCorrupt`(P0,损坏的停机标记退化为全局 Kill)、`ModelCkptSkipped`(P1,检查点超盘上限被跳过)。

## 引擎(M-B)

- **RG13 冷读者的常规成交**:hft-ledger 提供 journal 冷消费者 `JournalLedger`:从 `OrderDecision{New}`(Output)维护最小的槽映射 (epoch, slot, gen) → (account, inst, side, group),对 `OrderEvent` 的成交增量按 `cum_filled` 去重入账,另消费 `ExternalFill`(Input)、`AUTO_RECON` 注记、`Note{OmsForeignFill}`、`FundingSettle`、`AdjustDelta`。属性/DST 断言:每个快照点上冷消费者的持仓、库存与余额等于引擎 ledger。reconciler 的引擎视图与 hft-py 都用它,不另写实现。
- **RG14 `RoundClosed`** 不带 `tag`,改带 `group`。
- **RG15 对冲截止**从"最早一笔尚未对冲完的成交"起算(不是第一腿首次成交)。
- **RG16 定时器堆**:`max_timers` 在装配时按最坏需求校验;运行中堆满是引擎缺陷 → `Fatal(TimerOverflow)` abort(进入崩溃恢复)。删除"输入环满的轮次按空闲处理"这句。
- **RG17 `link_notional`** 两个方向都用 `(1 + rr_max_cross)`(更保守,接受)。
- **RG18 外来成交截止到达但裸量在阈值内**:不发单时写 P1,发单时写 P0。
- **RG19 目标为 0 的腿**以 |仓位| ≤ 尘埃线为完成条件,不看不交易带。
- **RG20 冷读者重建允许多报**(偏向"仍在处置中",ops_bot 因此偏向只撤单),接受;Drain 不等 Broken 中尚未到期的自动平仓,接受(Broken 进快照,重启后按计时规则继续)。
- **RG21 风控与 sequencer 接口**:T9 报告第 3 节列出的 sequencer 侧改动全部做(`on_legs` 于快照起步、`on_groups_changed` 先于 `apply(GroupsChanged)`、`ModeView` 适配器、`ModeCfg`、`CLEAN_DRAIN`、`daily_loss_resumed`、`StorageRecheck`、`keys_required`、`Risk::notional`、`UnknownPending → PreconditionFailed{UnknownPresent}`);hft-exec 提供 `engine_reduce_pending()`。
- **RG22 确认请求的 `expect_state_version = 0`**:sequencer 对升级类确认同样拒绝(Kill 的确认例外),与 gateway 一致。
- **RG23 下架**:venue 级 cap,不改 cell 的有效模式,只撤该 inst 的 Open 挂单(T9 的口径,接受);INV-11、INV-13 按 T22 的收窄接受。
- **RG24 sim-gw 受控故障注入**:hft-sim 增加只对 Sim 账户生效的故障注入命令(拒单、断连、延迟回报),经控制面 Admin 命令或 sim 实例配置触发,用于 Broken 处置演练(DRL-12)与 DST 之外的实盘式演练;prod 实例拒绝。
- **RG25 MS-2/MS-2a 拆分**接受;scenarios 的节内参数基线(同页显式写出数值)视为满足"前置显式设参数"。

## venue 层(M-C)

- **RG26 hft-venue-proto 补齐 T14 报告第 3 节列出的接口缺口**:REST 形态的订单操作出口(全撤、查单、挂单列表)、会话维护请求(listenKey 续期,`SessionKeepalive`)、`ReportOutcome` 的控制提示字段(时钟越界、限速、用户流终止、维护预告)、批量响应逐条的结果未知、`NAME` 与 VenueId 常量名的关系、`default_caps` 的平台币 AssetId 由 refdata 注入、`classify_report` 的可忽略事件名、`decode_bbo_snapshot` 带 inst 与批量容量、REST 解码拿到发起请求的 ReqRef。
- **RG27 只读 REST 查询的解码入口**:hft-venue-proto 增加 reconciler/kill-switch 用的只读查询族(挂单列表、成交历史、资金费流水、余额与持仓查询)的编码与解码接口;三家 venue 各补金样用例。规格现在写定,实现随 M5。
- **RG28 单帧事件数超过 `FRAME_EVENTS_MAX`**:整帧按解码错误处理并计数(不部分丢弃)。
- **RG29 改单能力**:`VenueCaps` 增加 `amend_qty_down_only`(Binance 现货);OMS 拒绝该 venue 上改价或增量的 Amend(`AMEND_UNSUPPORTED`)。
- **RG30 OKX `margin_ratio = mmr/eq`**(遵循契约 17)接受;Bitget 现货高级(多资产)模式按 OKX 同样判 `ACCT_TYPE`;T14 第 5 节第 3 条的暂定默认值接受,保持"待核实"标注。
- **RG31 可选扩展的用例**:允许 `状态:可选扩展:<扩展名>`,`test-trace` 只在 `tools/xtask/test-trace.toml` 的 `enabled_extensions` 列出该扩展时要求实现;hft-net 的 RSS/TX 戳/scratch 预热、增量深度(BOOK-102/106、hft-venue 清单)、replayer 的可选项都用此状态(主控改 docs/tests/README 与 check_tests)。
- **RG32 补帧放行** `update_id ≥ last_fwd`(断线期间未变的报价会再转发一次)接受;host/testnet 实测要求改默认值时,同一提交更新默认值并在 `docs/changes.md` 记一行。

## 冷进程、控制面、运维(M-D)

- **RG33 ctl 签发者表**:`/etc/hft/ctl-principals.toml`,归 hft-core 配置(只在启动时加载,改动需重启),deploy 生成,launch 检查清单与 `.github/hft-admins.txt` 核对一次(RF5)。
- **RG34 `CmdPreview`** 增加字段承载 `WriteOffResidual` 的"多出那一腿"(venue、inst、leg_ix、qty);CtlAudit 的 `v[]` 加对端 uid。
- **RG35 kill-switch 停机结果**:systemctl 返回非 0 一律按 `Failed`;`Denied` 只在 dev2 核实 stderr 文本后启用(待核实)。
- **RG36 JSON 线格式约定**(T15 定的字段名、整数用十进制字符串、`MAX_BLOB = 786 432`、错误分层)接受,作为 Python 客户端与金样的合同。
- **RG37 reconciler**:`pending_in` 超过 24 h 未到账转 `Warn`;每日模式遇检查点键不符记 `model_no_start` 并继续后面的日子;`UnknownResolved` 一律 `Ignore`;`refdata_blob` 读文件(启动核验用),已知限制写明 `ReplaceRefdata` 之后需重启 reconciler。
- **RG38 watchtower**:`ArchiverAlert` 核心规则接受;告警生命周期口径接受;`healthz_bind` 与 `host.ena_ifaces` 由 watchtower 启动时从 `hostprofile.json` 读取(不由 deploy 生成);pipeline 的 `status.json` 告警由 watchtower 投递(新增规则 `PipelineAlert`,与 `ArchiverAlert` 同形)。
- **RG39 ops_bot**:视图停滞阈值单独设 `ops.view_stale_ms = 3000`;schema 目录损坏时退回 `GetSchema` 继续运行(`/stop` 通道必须可用);`/stop … core` 在控制面恢复后以同一身份补发 Kill 接受;pipeline 遇 `VersionConflict` **停止该实例的发布并报 P1**,不自动导入为覆盖项。
- **RG40 deploy**:dev2 预检的"无编译工具链"只查 Rust 工具链(`cargo`、`rustc`),其他主机类查全部;退路 (a) 的清单加 `replay_verify = true|false`;pipeline 读取的 collector S3 前缀登记为只读前缀(名称待核实);root 单元的最小能力集收紧列为已知限制(后续工作)。
- **RG41 hft-verify**:按 slice 剩余内存算出的限额 ≤ 0 时跳过本次并记 `skipped = insufficient_memory`;收到 SIGTERM 后子进程 10 s 不退出则 SIGKILL;DST 框架验收"每个植入缺陷在 2 000 个种子内被发现"接受。
- **RG42 每个 PR 的 HashTrail** 核对 50 个一致性种子,500 个在每夜与正式版标签运行(development 与 tools 一致)。
- **RG43 门槛 ⑪ 的证据** = `门槛` 字段含 ⑪ 的用例;看门狗告警时限按 deploy 口径(连续 60 s 不可达才告警)改为 ≤ 75 s。
- **RG44 hft-core**:`sim` 实例允许 `endpoint_override`(CI 用假交易所),启动时写 P1 Note;毒丸守卫与 systemd 启动限制的交互写进已知限制;演练 8 的阈值(+1 个百分点)标"待标定"。
- **RG45 测试夹具**:testkit 的场景源文件统一为 `crates/hft-testkit/src/scenarios/<主题>.rs`,不变量检查函数 `src/dst/invariants.rs`(检查器 `I<n>` = INV-n);T22 第 3 节列出的测试策略能力(结构字段、`advice_max_age_ms`、按 `engine_ns` 注入落后、可换版本的 Model 替身)写进 testkit;FP-8 与 C12 一样落在 `apps/hft-core/tests`。
- **RG46 MD-12、MD-17 的 proc 用例**:若 GitHub 托管 runner 上做不到 systemd 为 PID 1 + polkit,降为 `drill`(M5),由 M1 第一周的 CI 核实决定。

## 文档入口与其余(M-E)

- RF16 全部落实;development 的质量门加 `cargo xtask test-trace`;HashTrail 按 RG42;两个季度演练链接 DRL-110/111;smoke-order ADR 删除"价格带内";hot-path-rules D6 登记 `gate_closed_tsc`;N3 与 hot-check 豁免 `crates/hft-strat-lib/src/model/`;architecture-v2 中与本轮改动相关的说法只改为链接。
