# M0-e 合并修正清单(主控;合并代理 K1–K5 先读)

依据:`round8e-spec-brief.md`(权威)与 `round8e-merge-notes.md`(12 个修订代理的报告与跨文件待办)。本清单是**合并阶段**的权威裁决与分工;每个合并代理只改自己簇内的文件,改完逐条在报告里写"已改 / 已一致 / 无法改(原因)"。写作规则同简报 §0(中文、标识符英文、一件事实一个归属地、不重抄、不丢章节;完成后跑 `PYTHONIOENCODING=utf-8 python E:\trading\architecture-v2\_work\heading_audit.py E:\trading\_backup\round8e-pre` 与 `check_hft.py`)。

## 主控裁决(合并时以此为准)

- **R1 资金费结算时刻归一化只有一个实现**:hft-types `funding_settle_norm_ns(ts_ns, interval_s, anchor_s)`(取最近的结算边界)。hft-ledger 删除自己的 `norm_settle_ts` 与 `FUNDING_TS_QUANTUM_NS`,改为调用并链接 hft-types;venue-*、hft-sequencer、reconciler 同样只用它。
- **R2 核销残余的语义以 hft-oms 为准**:`WriteOffResidual{account, group}` 只处理**多出的那一腿**(单腿):把它的已成交裸量从该 group 移到 unassigned(ledger `AdjustDelta::WriteOff`),LegTracker 该腿 `residual` 清零,Broken 随之可解除;`write_off_residual` 返回单条 `WriteOff{venue, inst, leg_ix, qty}`。control-plane、hft-sequencer、hft-ledger、SDK、ops_bot 按此统一("residual 清零""返回的每条腿"等说法改掉)。
- **R3 `CancelAll`/Kill 撤单范围**:Live、PendingNew、PendingAmend(以 hft-oms 为准);control-plane 对齐。
- **R4 冒烟单与 Drain**:`Drain{cancel_resting = false}` 时 SMOKE 槽保留到进程退出(D-smoke 第 2 步用它验证交易所死手);其余情况按 `ttl_ms` 自动撤单。写进 hft-oms 冒烟单一节,launch 链接。
- **R5 SerialFallback 不设单独的启动宽限**:重启后 L3 出步须等本次运行的新 mark 或保证金报告确认(hft-risk 规则),已足够;hft-exec 已知限制链接该规则。
- **R6 标志文件**:护栏 tripped 标志改为持久路径 `/var/lib/hft/guard/tripped`(重启不消失,只能 `hft-dev2-guard clear` 清除);联调窗口标志保留 `/run/hft/window/active`(重启即结束窗口)。
- **R7 护栏二级动作不停 watchtower 与 ops-bot**(sim 自动重启后要出日报);自动重启 sim 的附加条件:此后无人手动启停过该单元。
- **R8 `smoke_far_bp` 保持 u16(bp)**,与 latency-probe `far_bp` 一致,不改类型。
- **R9 `AUTO_EXTERNAL`**:永续"同号缩小"包含缩到 0(全量 ADL);数量 = 引擎仓位 − 交易所仓位;reconciler、hft-sequencer、hft-risk 一致。
- **R10 ENI-B2 的出口地址表名为 `prod_bulk`**(deploy 已用);hft-core 同名。
- **R11 `MAX_BBO_CONN_SLOTS` 的值只在 hft-types**;hft-venue、hft-sequencer 改为只写名字并链接。
- **R12 `rx_tsc` 快照后为 0**(J5 的 `rx_from_seq` 规则)保留;SDK 写明"关键帧之后、本次运行收到该 inst 的首条 Bbo 之前 `rx_tsc = 0`,策略把 0 当作未知"。

## K1:SDK / strat-lib / strategies / adding-a-strategy / hft-exec / hft-oms

1. SDK:约第 646 行重抄的 BboView 聚合规则改为链接 hft-sequencer"连接活性";基准名 `seq/views_bbo` 改为引用 sequencer 的 `seq/bbo_view`(数值归 sequencer)。
2. SDK `ResetPreview`:给出 `reset_groups`(结构字段变化将重置的 group 数)与 `no_open_ms`(按 `Model::cold_start_ms`/`path_settle_ms` 与预热可用性估算的不能开仓时长)的估算方法,control-plane 的 `CmdPreview` 读它。
3. SDK:`MinorChange` 各变体的语义(相对旧值的判定方式;`None` = 任何改动都是大改;`Any` = 任何改动都是小改)与 recompute 重建点(路径节被丢弃时从下一个分位带边界 + 平滑窗长度之后比较)写清楚;testing ⑦ 引用。
4. SDK:`LegView` 注释链接 hft-oms 的在途划分表(`PendingCancel`/`PendingAmend` 计挂单在途);`OrderUpdate::Rejected.venue_code` 注释(本地发送失败时为 `SendStatus` 码);`InstRiskView.l3_max_cross`(0 = 不启用)与 hft-risk 一致;R12。
5. SDK/strategies:R2 的核销语义如有提及则统一;检查 `GroupMask` 残留(应为 `GroupResets`)。
6. hft-oms:约第 664 行"在途账本两类在途都计入,风控仓位检查读它"→ 写明风控 #7 对 Flatten/Hedge 只扣即时在途,`InflightBook` 提供按 (account, inst, side) 的只计即时在途读数(hft-risk `open_qty_immediate`/`pending_reduce_immediate` 读它)。
7. hft-oms:约第 679 行 Note 数值的归属措辞按 hft-event(数值 n 归 source crate 的 const 表,hft-event 只登记名字、来源、级别)。
8. hft-oms:R2、R3、R4。
9. hft-exec:R5(已知限制链接 hft-risk 的"恢复后 L3 出步须本次运行新数据"规则);核对 `risk_for(a, i).quote_rate`/`rate_backoff`、`v.venue_ready(a, v)` 与 SDK 现名一致(主控已在 SDK 补了这三项)。

## K2:hft-risk / hft-ledger / hft-sequencer

1. R1:hft-ledger 删 `norm_settle_ts`、`FUNDING_TS_QUANTUM_NS`(签名、规则段、源文件表、测试、依赖节),改为使用并链接 hft-types `funding_settle_norm_ns`;hft-sequencer 中的引用同改。
2. R2(ledger `AdjustDelta::WriteOff` = 单腿移到 unassigned;sequencer 命令路径"返回的那条腿")。
3. R9(sequencer 复核与合成 `ExternalFill` 的数量;risk `Disposition::AutoExternal`)。
4. R11:hft-sequencer 约第 330–332 行不重写 `MAX_BBO_CONN_SLOTS` 的值。
5. hft-sequencer:inst 订阅的 `EvMask` 并集变化时重发 `FeedCmd::Subscribe`(带 `streams`,字段归 hft-event)。
6. hft-sequencer:`ReplaceRefdata` 的 Drain 末二次复核(Input 记录形态与"撤销待生效引用"的语义,与 hft-core 一致);`SetParams` 一并复核检查点盘上限(`advisor_ckpt_max_mib`);运行中新增 group 时复核 `DustBelowMinNotional`(新 group 的交易腿最小名义)。
7. hft-risk:与 hft-oms 的即时在途读数对齐(K1 第 6 条)。

## K3:hft-types / hft-event / hft-refdata / hft-ctl-proto / hft-ctl / docs/subsystems / tools / hft-venue-proto / hft-venue / hft-net / venue-* / adding-a-venue / adding-an-event-kind / adding-a-control-command / fixtures

1. journal-format:`state_hash` = 各节内容按**写出顺序**(与 hft-sequencer 快照节写出顺序一致)的 xxh3-128。
2. journal-format:节 18 `GateRow` 增 `standby_ok_venues: u64`(56 B,行版本 4);hft-risk 已引用。
3. hft-event:`FeedCmd::Subscribe` 增 `streams` 字段(位值与 SDK `EvMask` 相同);环拓扑中 `acct-<venue>` 写成"acct-fetch → gw,hft-venue 内部字节环",`acctreq-<venue>` 同;`LinkRole` 归属措辞"类型归 SDK、语义归 hft-oms"。
4. control-plane:R2、R3;`ReplaceRefdata` 装配校验失败由 gateway 直接回 `Rejected{RefdataAssembly}`(不进 sequencer、无 CmdResult);`ReplaceRefdata` Drain 末复核失败的 P1 Note 码登记(hft-event)。
5. tools:约第 258 行"按主机本地清单的 MAC"改为"按设备序号经 IMDS 识别网卡,MAC 只在运行期核对";hostprobe 采集 managed IRQ 的 `effective_affinity`、IRQ 核上的任务、XPS 映射;约第 226 行 az-probe 的运行地点(M1.5 前、生产 AZ,不在 dev2);`bench-gate --smoke` 用 `ci` 剖面;scaffold-gate 复用主 target 目录、`--with-model --with-pipeline` 与一次 `hft-core --check`;`hft-verify` 读 hft-testkit 的 `inflight.json`、信号分类(SIGABRT/SIGSEGV 失败、SIGKILL 中止)、疑似失败与 CI 复现结论、分段 CPU 秒、`path_digest`(核对已写的是否齐全)。
6. tools hft-cfg:把 `advisor.checkpoint_max_mib`、`startup.allow_missing_standby`、`advisor.mem_budget_mib` 写进 ECF1 段 5(字段名以 hft-sequencer 为准:`advisor_ckpt_max_mib`、`allow_missing_standby`、`advisor_mem_budget_mib`);hft-ctl 的 SetParams 校验从段 5 读预算并计算 `model_ckpt`、`structural_changed`。
7. R11:hft-venue 约第 266 行不重写 `MAX_BBO_CONN_SLOTS` 的值。
8. fixtures:登记 Model 路径节金样 `fixtures/strategies/<name>/ckpt/path.bin` 与 DST 金样 `fixtures/dst/hashtrail.toml`。
9. hft-venue/hft-net:sim 连接的 NAPI 归属自检在 conn-builder 以 netlink 做(`Connector::napi_ifindex`),hostprobe 不做;`feed.conns` 字段为 `{stream, src_ip}`,与 hft-core 一致。
10. R1:venue-* 中资金费结算时刻若有自己的归一化说法,改为链接 hft-types。

## K4:apps/hft-core / apps/replayer / crates/hft-sim / apps/{reconciler,watchtower,kill-switch,journal-archiver} / hft-testkit / hft-telemetry / docs/testing.md / benchmarks

1. hft-core:约第 626、820 行 `ReplaceRefdata` 装配校验失败 → gateway 直接回 `Rejected{RefdataAssembly}`(不是 CmdResult);Drain 末复核与 K2 第 6 条一致。
2. hft-core 启动步 5:停机标记 `scope` 解析(`"*"` → Global,否则按 `AccountMeta.label` 找账户 → Account);与 kill-switch 标记格式一致。
3. hft-core:约第 186、710、711 行 ⑦/⑩ 的口径改为链接 testing;IO 带宽上限取值行在 hft-core(删除任何 `IOWeight` 残留);R10 `prod_bulk`;`HostNeeds.irq` 由装配传入;exec cell 640 B 后复核引擎区内存;启动时核对 TOML 与 ECF1 段 5 的 advisor 预算与 `allow_missing_standby`。
4. hft-core:R6、R7(护栏规则的唯一归属在 hft-core;deploy 引用)。
5. replayer:路径节被丢弃的日子在报告写 `recompute_from_seq`(从下一个分位带边界 + 平滑窗之后比较)。
6. watchtower:c4 的 `HostPressure` 与 hft-core 一致(c4 没有护栏时不报或只用 MemAvailable);prod journal 根被替换(M7→M8)后的游标规则(launch D-23 第 4 步依赖);CFGREF 警告的三个键(`instance.journal_cold`、`host.guard_config`、`host.maint_file`)写明是本 app 配置键还是引用 hft-core 键,避免误读。
7. kill-switch:约第 279 行的待核实改为 deploy 已定的 `/var/lib/hft/kill-switch`、组 `hft-stopmark`、模式 `2770`(由 kill-switch 的 `ExecStartPre=+` 建立);hft-core 能读并改名标记。
8. testing:一致性集引用金样 `fixtures/dst/hashtrail.toml`(标签工作流在同一提交上跑 x86 HashTrail);⑦ 写明"group 增删与 `open_allowed` 变化"的口径(按 SDK `minor_change_check`:group 增删与 `open_allowed` 变化是否重新计时——推荐:增删 group 与 `open_allowed` 0↔1 不重新计时,但新增 group 自身须满 7 天才计入 ⑦ 证据);删除残留的"小改白名单"说法;R9 在 reconciler 的口径。
9. reconciler:R1(只用 hft-types 的归一化);R9。

## K5:deploy / docs/development.md / docs/runbooks / python / .agents/notes / crates/hft-py / 根 AGENTS.md / architecture-v2(README、00–13、现状、待测量)

1. deploy:R6(tripped 标志持久路径 `/var/lib/hft/guard/tripped`)、R7;`hft-shm-release` 只看 hft-core 规定的判据(去掉 `$SERVICE_RESULT`);c4 的 IRQ 布局单写(c4 没有 IRQ 专核);system.slice 的核号改为链接 hft-core。
2. launch:约第 83 行账户字段 `label`;D-smoke 第 2 步按 R4。
3. development:标签工作流在同一提交上跑 x86 `HashTrail` 并与 `fixtures/dst/hashtrail.toml` 比对;链接 testing §6 的超时值而不重写;新增"复现疑似失败"的 CI 作业(`workflow_dispatch`,输入验证包版本与崩溃输入);确认 `main` 推送不 cancel-in-progress。
4. host-upgrade 第 8 步:同机干扰按 2 h 块、块内顺序按种子随机、`hft-sim-alternate --start` 取偶数 UTC 整点。
5. pipeline 框架 README:删除"policy 中声明的下限"与"格式变化换 `local_kind`"的说法(格式变化按 SDK 规则提升载荷版本);`WarmupSnapshot.params` 为解码后的生效参数。
6. ops_bot:`hft-opsbot`、`CmdResponse.cmd_hash`、Viewer 可发 `Annotate`、二次确认与群聊放行链接 hft-ctl-proto 权限表;R2(`/writeoff` 预览显示多出的那一腿)。
7. hft-py:约第 123、153 行 `/opt/hft/venv` → 按版本 venv(`/opt/hft/current-cold/venv`)。根 AGENTS.md 约第 37 行 bench-gate 的描述与 benchmarks/tools 一致(W3 P2-66),篇幅仍 ≤ 1,800。
8. architecture-v2:11 §8 补 gp3 预置吞吐/IOPS 与 t4g.nano 的费用;核对三处措辞——`hedge_host` 只记录(对冲路径硬线由 `rep_to_order` 承担)、`acct-<venue>` 为 acct-fetch → gw 的 hft-venue 内部环(不进 sequencer)、BBO 补帧经 snap-fetch 交回 feed-bbo;c16 新线程计划(两条 feed-bbo + 一条 feed-md)在 11 与 02 的描述;hft-core 线程计划与 latency-budget 链接 11 §3.2(若属 K4/hft 文件则只在 architecture-v2 侧改)。
