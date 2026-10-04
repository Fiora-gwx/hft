# M0-d 合并备忘(主控)

## 代理报告要点与待跨文件处理

### J3 portfolio-carry(完成)
- 新锚点:`strategies/portfolio-carry/README.md#小改白名单`(testing ⑦ 链接)。
- pc 内部 Note 21–24:`ManualExternal`、`PerpEquityCap`、`ExitRetry`、`NegFundingExit`;Advice 标志 `FUNDING_TRUNC`/`NEG_FUNDING`;阶段 `Fix{Manual}`。
- 待办:
  - [x] launch:67("杠杆必须等于 1/perp_margin_frac")、launch:103("CloseOnly 会平仓")→ 改为"实际杠杆 ≤ `max_perp_leverage_x100`,推荐 2x;pc 在 CloseOnly 下持有"(J10 文件)。
  - [x] pc 链接 `hft-sim#压力用例上线门槛必过项`,J8 改后核对锚点。
  - [x] pipeline 框架侧:`WarmupSnapshot` 提供"需预热 group 集合";收到 `Note{ModelGroupReset}`(含新 group)自动预热(J10)。
  - [x] SDK:单个 venue 回到 L0 是否有事件(V2 P2-6);`CapCleared{Delisting}` 是否投递策略(sequencer)。
  - [x] SDK:Model 收不到 Manual/Exec(V2 P2-8d)——开放问题,记录即可。

### J12 冻结 + 检查脚本(完成)
- check_hft.py 新硬错误类:STATUS、HISTORY、SYNCQ、LENGTH、LAYOUT、NOTEREG、RESID8D;警告:HISTWEAK、SYNCREG、CODEVAL、IMPLPAD。冻结组件跳过内容类检查。
- 篇幅计量:每个词须含字母或数字(J11 写 docs/AGENTS 口径时对齐)。
- 冻结组件已知额外问题(解冻时复核):hft-ext-proto `KillMap` 56 B vs 实际 152 B;链上组件残留 "round 3"/"R7-4"。
- 待办:
  - [x] 全部完成后重跑;NOTEREG 关注 J4 新名 `ExecUngroupedReduce` 是否被 J6 登记;旧名 `ForeignOrder`、`Transition`。
  - [x] CODEVAL 4 条:MarginRow.level、CmdDedupRow.status、ConnState.reason、SealRecord.reason。
  - [x] hft-venue `IdEntry` 128 B(V3 #102)。

### J11 横切文档(完成)
- 新名:`HostClass::C32 = 6`;`journal.reader.tail_poll_us`(50)、基准 `journal_retire_tlb`;锚点 latency-budget `#31-p999-记录项`、hft-rt `#线程计划的执行任务轮转与空闲策略`。
- 口径:LegLink 第二腿延迟样本只计入 `hedge_host`,不计入 `tick_to_trade`(telemetry、testing 须一致)。
- D6 白名单加了快照节 26 `ConnLive`(恢复时作废,不平移)——J5 确认。
- 待办:
  - [x] J10 ADR `2026-10-01-thread-plan-and-host-classes.md` 改 hft-rt 新锚点。
  - [x] J5 sequencer:节 26 冷启动/恢复结束作废;平移公式。
  - [x] J7 hft-venue "意图→帧写出 10/30 µs" 改链接 latency-budget `egress`。
  - [x] J6 hft-refdata `C32 = 6` 与 hft-rt 一致;control-plane:13 HARD 解除后未对冲量与 hft-exec 一致(V1 P2-3)。
  - [x] J8 hft-core 环容量表加 ctl 环 ≥ 256 KiB(hft-rt 删了 `rt.ctl.capacity_bytes`)。
  - [x] J9 testing ⑩ 硬项与同机干扰唯一指标;锚点 benchmarks `#回归阈值`、`#参考机与-ci`、testing `§4.4`;`seq/l2burst`。

### J8 hft-core / replayer / hft-sim(完成)
- 新约定/名:sim 账户名前缀 `sim-`(`AccountNamePrefix` 退出 2);`host.irq_cpus = {eni_a, eni_b, eni_c}`;`venues.<v>.max_sub_insts`(原 `venues.<v>.max_insts`);`advisor.checkpoint_chunk_kib` 1024;`feed:<venue>` = routes All;`snap-fetch` 每实例一个任务,`rest_pool.feed` 归 snap-fetch;环 `snapreq-`/`snap-` 64/4 MiB(dev2 16/1 MiB)。
- dev2 内存:sim 576、prod 384、watchtower 160、archiver 256、replay-verify 384、hft-verify 384、guard 32、offbox 32;hft.slice 1,376;batch 512;hft 合计 1,504;满载 MemAvailable ≈ 1,071;tmpfs sim 224M、prod 192M;watchtower `max_series` ≤ 4,000,archiver `row_group_bytes` ≤ 16 MiB。
- c32:隔离 5–17;IRQ 0/1/22;prod MemoryMax 2,048 + 大页;slice 4,352/6,272/7,040。c16:隔离 4–11;IRQ 0/1/13。c8 降级布局。
- replayer `--stress` 名:`one-sided-30`、`l3-path`、`basis-widen`、`fast-30`、`gw-down`、`thin-book`、`core-restart`、`gate`、`all`。
- 待办:
  - [x] J10/J6:launch、ops_bot、refdata 跟进 `sim-` 前缀。
  - [x] J10 deploy:`host.irq_cpus` 生成与 hostprobe。
  - [x] J5/J6:`max_sub_insts` 改名一致。
  - [x] J7:`feed:<venue>`、单个 `snap-fetch`、`rest_pool.feed` 命名一致。
  - [x] J11 hft-journal:RAM 热段缺失但冷副本末段已封存且以 Drain 快照结尾 → `tail_lost = false`(`hft-shm-release` 依赖)。
  - [x] J5 advisor:写检查点期间不向该 Model 投递事件。
  - [x] testing/tools:被 OOM 杀的 `hft-verify` 运行记为中止,不算失败;联调窗口后 replay-verify 补跑窗口内每一天。
  - [x] 根 AGENTS 残留:"不在 dev2 上跑 fuzz/DST"、"sim 出口 = 第二私有 IP"(升级主机为 ENI-C)。
  - [x] J9:watchtower/archiver dev2 配置上限。
  - [x] 风险:dev2 余量 47 MiB;hft-verify 与 archiver 同跑易 OOM(可接受:中止不清零)。

### J1 architecture-v2(完成)
- 11 §3.1 = M1.5,§4.1 = ③④ 在服务器;12 A23、A24;待测量 §J。
- 成本估:c32(c8g.8xlarge)≈ $1,169/月,加冷盘 ≈ $1,250;c8 + 冷盘 ≈ $370。
- 待办:
  - [x] 报告给用户:11 §3.1 "纵切片代码在 M1.5 先写最小可用,其余 M2/M3 补全"需确认(属于排期,可按推荐)。
  - [x] latency-budget §3 "出处"列与新 00 §4.1 对齐(J11 文件,我合并时改)。
  - [x] 12 §3.2 重复两行(小)。

### J2 SDK / strategies(完成)
- latency-probe kind_id 5。新名:`GroupSpecExt`、`GroupMask`、`SectionDesc/ElemDesc/ElemTy/LenRule/SectionScope`、`Relax`、`ParamError::{OverrideOrder, Padding}`、`SubsErr{Full, NoFeed}`、`RefView::insts_of`、`LiquidationView`、`RiskStateView.max_order_notional`、`StrategySet::{model_mem_bytes, model_snapshot_bytes, model_layout, model_snapshot_chunk}`、基准 `seq/views_bbo`、`OrderUpdateKind::Rejected.venue_code: u32`、strat-lib `FUNDING_RING = 256`、`FundingSummary` `TRUNCATED`、`PctConfig::layout_eq`;cross-taker Note `Unarmed` → `NotReady`。
- **删除 `BboView.upd_tsc`(改用 `rx_tsc`)**——全仓 grep 跟改。
- `RiskStateView.rr_max_cross` = 按账户各价格带类别最小值(保守)。
- 待办:
  - [x] grep `upd_tsc`、`&BboView` 全仓跟改(exec/oms/risk/sequencer/pc)。
  - [x] J6:hft-ctl:66 → `decode_params` 四参数 + `validate_params`;control-plane 码表补 `InstOwned`。
  - [x] `#94` `conn_state` 改名(J5、J7)。
  - [x] 订阅账户外 venue 行情(`NoFeed`):sequencer/hft-core 支持"只开 feed 的 venue"(开放问题,写进 hft-core 已知限制即可)。
  - [x] latency-probe 需测试网 `RiskParams.max_px_dev` ≥ 300 bp(launch/testnet 配置说明);journal ≈ 50 MB/天/探测点。

### J4 exec / OMS(完成)
- 新 Note:`ExecUngroupedReduce`、`OmsCancelNotSent`(J6 已登记)。补回 OMS 三节("SendReport 处理""REPORT_DECODE""恢复后的 gw 映射")。
- 签名/名:OMS `on_hedge_gate_open(a, v)`(取代 OMS 的 `on_gate_open`);`recovery_end(now, stop_tsc, hz_prev, hz_cur)`(exec 与 OMS);`cancel_all`/`reissue_cancels` 加 `open_only: bool`;`OmsOut.timers`(`OmsTimerReq`)需 sequencer 排空;`ApprovedIntent.lev_x100` 风控填;InflightBook `buy_notional`、`open_margin` 增量和;`Capacities.max_cells_per_inst` 进 ECF1 段 3;`LinkBroken` None=0…ResumeTimeout=6;`AutoFixFail::SignFlip = 8`;节 21 `serial`、`force_req`;节 3 `OmsCountersHdr.resume_hold_until`、`TableRow.hedge_gate`;`SlotExt.lev_x100`。
- 待办:
  - [x] J2 SDK `LegTrack.in_flight` 拆两个字段;`ExternalFillView.deadline_tsc = 0` 语义;`Rejected.venue_code` 填 `SendStatus`。
  - [x] J5 sequencer 跟改上述签名(检查 J5 报告)。
  - [x] V1 P2-2/P2-3 control-plane:13(J5/J6);P2-4 gw 去重环(J7/J5);P2-9 reconciler 按需查询(J9);P2-10、P2-11 标记价断流 → L1(J5)。
  - [x] 开放:SetParams 与 set_target 的 LotMismatch 阈值未完全统一(只影响开仓,已入开发备忘)。

### J6 L0 / control / subsystems / tools(完成)
- Note 码:`code = source << 24 | n`,n 由各 source crate const 表给;新登记 Exec*/Oms*/`ModelGroupReset`,补登记 `MarginLevel` 等。
- control-plane:`CmdResult.detail = code | arg << 16`;新结果码 `27 Rejected`(detail `400 BrokenRemaining`、`401 RefdataRecheck`);`110 ParamInvalid` arg kind `Strategy=1, ModelMemBudget=2, NonProdOnly=3`;`120 InstOwned`、`121 ExecRiskMismatch`;`304 SimNotIdle`(删 `SimBusy`、`BrokenPresent`)。
- `MarketHdr` 48 B(`sw_rx_ns` @40),Bbo 88、Trade 80、L2BatchHdr 72;`LatencySample` v2 128 B(`sw_rx_ns`、`link_role`;Second/SecondRetry = hedge_host 样本);`ReconcileReport` v3 104 B;`SnapshotHdr.engine_ns` i64。
- `cap_cross_px` 多 `tick` 参数。`HostClass` c32=6。xtask `scaffold-gate`(按路径触发的门槛)vs `decoupling-gate`(PR 级)。
- 新名:`INST_ALL`、`ASSET_NONE`、`INST_NONE`;`RefData::account_of() -> Option`;`InstanceMultiBound`、`DeadManRange`;`CmdResponse.detail`;`GatewayConfig.advisor_mem_budget_mib`;`ClientError::ChunkTooLong`;`tools/hft-verify/gates.toml`;`refdata.max_universe_insts`。
- 待办:
  - [x] J9:testing/hft-testkit `scaffold-gate` 改名;watchtower `Note{ForeignOrder}` → `OmsForeignOrder`;telemetry `Note{Transition}` → `OmsTransition`;#99 reconciler "角色"。
  - [x] J5:#89 `QUERY_FAILED` → `UNREADABLE`;`SimNotIdle` 条件含"无 Unknown 槽";`Resume{HedgeFailure}` → `Rejected{BrokenRemaining}`;`ReplaceRefdata` 复核失败 → `Rejected{RefdataRecheck}`;ECF1 段 3 加 `max_groups_per_instance`;只用 `max_subs_per_inst`。
  - [x] J4 文件:`cap_cross_px` 带 tick;exec `max_cells_per_inst` / ledger `max_groups_per_inst` → `max_subs_per_inst`(注意:J4 把 `max_cells_per_inst` 放进 ECF1 段 3 —— 需判断是否同义;"每 inst 被引用上限" = `max_subs_per_inst`)。
  - [x] J7:`INST_NONE`/`ASSET_NONE`;`DeadManSpec` ms;feed 填 `MarketHdr.sw_rx_ns`;gw 填 `ExternalFill.fill_seq`。
  - [x] J8 文件:`refdata.max_insts` → `refdata.max_universe_insts`;ECF1 段 4 环容量表;恢复 `CapacityShrink`。
  - [x] hft-rt `C32=6`(J11 已做,核对)。#109 旧字段名在 replayer、hft-py、defensive-patterns。
  - [x] J10:deploy `VERIFY.json` 带 `gates` 段;ops_bot 代 Viewer 发 `Annotate`。
  - [x] 全仓按 `MarketHdr` 48 B 重算的估算(journal 段大小)核对。

### J7 venue / net(完成)
- 新名:环与 journal(feed 拆两任务时)`feedctl-<v>-bbo`、`snapreq-<v>-bbo`、`snap-<v>-bbo`、`raw-feed-<v>-bbo`;`FeedRoutes`、`feed:<v>`(All)、`SnapRecHdr`、`InitErr::BboSlots`;配置 `snap.parallel`、`snap.ring_bytes`、`snap.req_ring`、`feed.resync_slots`、`gw.dedup_reseed_ms`;契约 25(snap-fetch 用自己的 FeedCtx);`TradeKey::Funding{settle_ts_ns, disc}`;`ReportOutcome.raw_code`;`ExchIdHint` 在 venue-proto;raw-gw `Connect` 标记体 `[account, role]`;§6a 去重环重启重建;`IdEntry` 改为 64 B。
- 待办:
  - [x] J6 文件:`MAX_BBO_CONN_SLOTS = 8` 放 hft-types;journal-format 登记 `raw-feed-<v>-bbo`;hft-event `ExternalFill.trade_id` 注释链接契约 22;hft-cfg 校验 VenueMeta 流数与入向上限 ≤ 协议默认。
  - [x] J5/J8:live driver 按 hft-venue §1 把 `FeedCmd` 分发到一个或两个环。
  - [x] J8 文件(hft-core):`venues.<v>.raw` → `feed.raw.*`;删 `rest_pool.feed`;内存表加 `-bbo` journal 与 `snapreq-<v>-bbo` 等;`rest_endpoints` 按 venue 端点表顺序校验。**注意 J8 写的是 `snapreq-<venue>`/`snap-<venue>` 而 J7 的 bbo 拆分会产生 `-bbo` 变体——核对哪个任务发快照请求(feed-md 才需要快照;bbo 若也需 BBO_REFRESH 补帧走 REST)**。
  - [x] J11 文件:hft-rt `feed.frames_per_loop` → `frames_per_poll`;crates/AGENTS 依赖补 hft-net → hft-event(只用 ConnRole)。
  - [x] hft-oms:384、hft-ledger:243、defensive-patterns:391 去重键改链接契约 22。
  - [x] J9:kill-switch countdownCancelAll 引用 venue-binance 开发备忘 10;reconciler Binance `history_checked` 判据。

### J9 cold apps / testing / benchmarks(完成)
- testing 新节:§6.1 验证包与账本;§8.1 ⑦ 计时;§8.2 ⑩ 口径;MS-3、MS-4(= hft-sim `l3-path`)。
- 新名:kill-switch `action = cancel|stop_core`、`Rejected{StopNotAllowed}`、`Done.core_stop`、`trigger.stop_core_unit`、`trigger.stop_wait_s`;watchtower `EngineNote`、`VerifyRun`、`ClockSkew` → `ClockOffset`、`watchtower interference`、`healthz_port`、`healthz.recon_max_age_s`、`hist.max_series`/`hist.mem_frac`、`host.guard_config`、`paths.verify_runs`;telemetry `interference()`、`HourStat`、`InterferenceReport`;reconciler `notfound_grace_ms`、`notfound_confirm_rounds`(删 `funding_lookback_h`);archiver `sources[].kind`、`advisor_dir`、files source `include`/`snapshot`/`settle_s`、`host_id`、`paths.advisor_daily`;testkit `hft-dst run|repro|list`、`DstRunResult`、feature `dst-bin`。
- 口径决定(报告给用户确认):⑩ `tick_to_trade` 硬线 = 验收下限 30/100 µs,基准形态 22/75 为目标,中间须 Admin 签字;`hedge_host` 同机干扰改为"开 sim 小时 vs 关 sim 小时"合并比较(每小时样本太少)。
- 待办:
  - [x] J7/J6 文件:venue-binance、hft-net、hft-ctl 中计入 ③ 的 fuzz 目标按 testing §3.3 对齐。
  - [x] J10:development fuzz 估算改链接 testing §6.1;incident-response §I `ClockSkew` → `ClockOffset`。
  - [x] J5:hft-ledger 接受 BalanceOnlyPush 所账户级 Funding 条目(`inst = INST_NONE`);(account, venue, settle_ts) 已入钱包额;`FundingCorrected` P1;hft-risk 写明 `sflags` 三位用法。
  - [x] deploy 护栏阈值文件路径与格式(`host.guard_config`)。
  - [x] latency-probe pair 模式:主网行情 + 测试网下单可能成交不足 → pair 模式应用测试网行情(`feed_env = testnet`)?核对 latency-probe README。

### J10 python / deploy / dev docs / runbooks / notes(完成)
- 新名:`hft-imds-lock.service`、`bin/hft-unit-idle`、`bin/hft-sim-alternate`、`release/package-verify.sh`;清单 `[storage] cold_gib`、`[egress]` 每项 `eni = "b"|"c"`;用户 `hft-offbox`、组 `hft-verify`、秘密前缀 `hft/ops/offbox/`;ctl socket `/run/hft/core-<instance>/ctl.sock`;ops_bot `stop.ctl_unreachable_s` 10、`push.note_min_level` P2、`issuer = ops_bot`;pipeline `groups.json`、`WarmupSnapshot.groups`;IR 锚点 `#s-交易主机失联`;polkit `50-hft-kill-switch.rules`。
- 待办:
  - [x] J9 文件:watchtower `ExternalFill` 规则对 Manual 报 P0 → 改 P1(D9-c);kill-switch:220–225 正式账户演练 SIGSTOP → 以 IR 为准;testing §6 "每周在 dev2 跑 az-probe"(#117)删;journal-archiver:184、210(#115)。
  - [x] J8 文件:hft-core:340 `Conflicts=` 残留(J8 可能已改,核对);hft-verify 内存行、护栏新信号、D11(J8 已做,核对)。
  - [x] J11 文件:根 AGENTS.md 69、73、74 行(c16、第二私有 IP、"不在 dev2 上跑 fuzz/DST"、与 Trading 共享额度);defensive-patterns `/closeonly`(#111)。
  - [x] J2 文件:cookbook `/b`(#111)。
  - [x] J6 文件:hft-refdata:277 `python/hft_schema/` → `python/_generated/`;hft-ctl-proto 权限表允许 `issuer = ops_bot` 发 Kill 与 Annotate。
  - [x] J1:11 §8 费用口径(参考机、c32、ENI-C EIP、公网 IPv4)核对。
  - [x] 风险(报告用户):D-smoke 无经 hft-core 的正式挂单入口,写成网页手工挂单 → reconciler 报外来挂单(可接受,launch 写明预期);`stop_core` 停整个 prod 实例。
  - [x] `hft-verify` 的 Nice/OOMScoreAdjust 两处归属 → deploy 改为链接 hft-core。

### J5 risk / ledger / sequencer(完成)
- 新名:ECF1 段 5 `ctl_bytes_per_round`、`snapshot_slice_bytes`、`snapshot_max_defer_ms`、`replay_per_round`、`advisor_mem_budget_mib`;节 5 `GroupSeqRow`;节 10 v4(`bbo_member_mask`、`bbo_hw_rx_ns`、`mark_ens`);节 26 键 (venue, slot);`GateRow.conns_ok_venues`;ledger `round_dust_notional`(原 `dust_notional`)、`restore_marks`、`apply_funding_push`、`mark_oldest_ens`;基准 `seq/head_check`。
- 待办:
  - [x] advisor 写检查点:整个 advisor 停在 `last_seq`(J5)vs SDK/hft-core "只停该 Model" → 统一为整个 advisor 停顿。
  - [x] `desc_version` u16 vs SDK `StrategyDesc.version` u32 → SDK 断言 ≤ u16::MAX 或改宽。
  - [x] SimReset 前置:hft-oms `sim_reset` 无非终态槽 vs control-plane 允许挂单(sim-gw 撤)→ 对齐。
  - [x] hft-journal `JournalSink::mark()/rollback()`;hft-book、hft-oms、hft-exec `snapshot_section(sec, w)`。
  - [x] hft-core 装配写 ECF1 `advisor_mem_budget_mib`。
  - [x] journal-format 节 5/10/26、GateRow 与 J5 一致。
  - [x] ledger `dust_notional` → `round_dust_notional` 的引用方。

## 合并结论(2026-10-03)

全部条目已核对或修正;裁决写入 `round8d-spec-brief.md` §10。check_hft.py:problems=0(warnings 1 = HISTWEAK 误报)。heading_audit 对 round8d-pre:4 个改名,均为有意。对 round8-pre 的审计发现 hft-sequencer "心跳"一节在第八轮初次修订时丢失,已恢复;其余差异为有意重构(回测系统删除、pipeline 插件迁移、cookbook 重排等)。

开放(报告用户):⑩ 硬线口径(§10-9);M1.5 只写最小纵切片代码(J1);`sim-` 前缀约定;dev2 余量 47 MiB;c32 ≈ $1,250/月;D-smoke 走网页手工挂单(reconciler 会报外来挂单,launch 写明预期);`stop_core` 停整个 prod 实例。
