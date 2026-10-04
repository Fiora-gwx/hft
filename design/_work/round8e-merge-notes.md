# M0-e 合并备忘(主控)

依据 `round8e-spec-brief.md`;12 个代理(J1–J12)2026-10-03 派出。各代理报告要点与跨文件待办逐条记在下面,全部完成后统一合并、跑 check_hft.py 与 heading_audit(对 round8e-pre 与 round8-pre 两个备份)。

## J11 横切文档(完成)
- 新名:hft-book `BookMetaRow`(32 B)、`TopRow`(64 B,无 rx_tsc)、`BookSet::restore`;hft-rt `HostNeeds.irq`;基准 `book_snapshot_topn20`、`book_snapshot_sections`。
- 误差界公式(latency-budget §2.1):两端误差较大者 + 相邻区间速率差 × 半区间。
- hft-book 新依赖 hft-strategy-sdk(快照读写类型)→ deps.toml 与 crates/AGENTS、architecture §7 已改。
- `acct-fetch` 结果经 `acct-<venue>` 环交回 gw(按 J7)。
- 待办:
  - [ ] J5:sequencer/ledger/risk/oms 中 `JournalSink::mark/rollback/AppendMark`、`snapshot_section` 残留;`keyframe_period_ms` 在 sequencer 登记;ReconcileReport 整组处理上界。
  - [ ] J6:journal-format 节 6、7 行格式链接 hft-book(v1)。
  - [ ] J12:deps.toml 加 hft-book → hft-strategy-sdk;hot-check D6 白名单三类。
  - [ ] J8:`HostNeeds.irq`;warm `park_us` 200–500;ENI-B2 启用条件与流量划分。
  - [ ] P1-48:hft-net、host-tuning 把 `kernel_rx` 写成"门槛"。

## J12 检查脚本(完成)
- 新硬错误类:IDREG、PATHREG、S3CRED、CFGDUP、CFGPATH、RINGHOME、CLIFLAG、RINGCAP、JRNLMEM、PAD、SIZECOPY、ENUMVAL、NOTESRC、BENCHDUP、BENCHOWN、BRIEFNAME;警告:IDOPT、S3REF、CFGREF、CLIREQ、CLISUB、BENCHCOPY、LINKTGT、TBDFMT。新参数 `--base DIR`、`--brief FILE`。
- 机检抓不到:P0-4 型(不同键不同默认)、散文上限、无 repr 的枚举、无反引号链接、表格内枚举。
- 现场(其他代理未完):problems=21;新类剩 IDREG 1、CFGPATH 1、RINGCAP 1(`acctreq-<venue>`)、PAD 1(`FillDelta`)、ENUMVAL 1(`ConnPhase`)、BRIEFNAME 4(`keyframe_period_ms`、`RiskParams.l3_max_cross`、`StressScope::Venue(v)`、`release-x86-cross.yml`)。
- 待办:
  - [ ] 全部完成后重跑并逐条清零。
  - [ ] J12 对 ④ 的解读(环只需在环容量表,不要求逐环进内存表)——同意。

## J1 architecture-v2(完成)
- 新编号:待测量 K1–K9;12 的 A25(分档快照流)、A26(M3 末探针代表性);锚点 00 §4.2(硬线与目标的理由)、11 §3.2(M3 末探针)。
- 成本改估:c32 ≈ $1,280/月(冷盘 ≥ 1.1 TB,gp3 $100–130,未核实)。
- 补了设计要求:M3 末探针至少覆盖一个资金费结算整点。
- 待办:
  - [ ] W3 P1-47 余项:hft-core:710、host-upgrade:51 硬线口径(J8、J10)。
  - [ ] deploy 冷盘 0.69 TB / 1024 GiB vs hft-core ≥ 1.1 TB(J10、J8)。
  - [ ] pipeline `hft-ops-bot`(J10)。
  - [ ] testkit `--steps/--trace` 与 tools 签名(J9、J6)。
  - [ ] BRIEFNAME 5 项;ADR 链接。
  - [ ] pc "小改白名单" 标题消失(J3 确认是有意改名)。
  - [ ] 核对三处措辞:hedge_host 只记录、`acct-<venue>` 环流向、BBO 补帧经 snap-fetch 交回 feed-bbo(与 J7/J9 定稿一致)。
  - [ ] hft-core 线程计划/latency-budget 链接 11 §3.2。

## J3 portfolio-carry(完成)
- "小改白名单"改名"小改属性"(有意)。pc 新名:阶段 `Trim`、`Note{TrimStep}`(25)、`trim_steps`、`neg_funding_exit`(开关与期数拆开)、`tag_seq`(tag 低 8 位)、`MINORDER_RESIDUAL`、`pend_exit`、`path_settle_ms`、`cold_start_ms`、`PATH_VERSION = 1`、`ckpt/path.bin`、基准 `band_tick`、`snapshot_path`。
- 自定(请复核):`funding_window_ms` 下限 4 h → 8 h;`slice_depth_levels` 1–10 默认 1;分位类 `minor_change = Abs(0.05)`;心跳每 10 s 全部 group(Advice ≈ 130 MB/日)。
- 待办:
  - [ ] J10:ADR `model-path-section`、`manual-hold-and-stop-marker` 落地(pc 链接);WarmupSnapshot 给解码后的生效参数;pipeline 框架 README "policy 中声明的下限"、"格式变化换 local_kind" 改。
  - [ ] J2:SDK 视图暴露 `dust_notional`(RiskStateView 或 ExecView);strategies/AGENTS 与 cookbook 的"小改白名单"改。
  - [ ] J7:fixtures 登记 `ckpt/path.bin`。
  - [ ] J9:testing ⑦ "group 增删与 open_allowed 变化"口径,删"小改白名单"。

## J2 SDK / strategies(完成)
- 新名:`Model::path_settle_ms`、`Model::cold_start_ms`;`StrategySet::{model_path_version, model_snapshot_path, model_restore_path, model_path_settle_ms, reset_preview}`、`ResetPreview`;`ResetCause{Structural=1, Fingerprint=2, PctConfig=3}`;`GroupResets` 取代 `GroupMask`;`SubsBuilder::model_own(OwnEvMask)`;`Views::risk_for(a, i) -> InstRiskView{max_px_dev, ioc_max_cross, rr_max_cross, l3_max_cross}`(**删除 `RiskStateView.rr_max_cross`**);`BboView.rx_ts_src: RxTsSrc{None=0, Hw=1, Sw=2}`;`SnapshotReader::desc_version()`;`params::minor_change_check`、`MajorChange`;`FirstLeg` 无数据枚举 + `PairParams.first_leg_ix`;显式码值 `LinkRole`、`TargetReason`、`Urgency`、`HedgeKind`;基准 `model/<kind>/snapshot_path`、`model/<kind>/own_event`;latency-probe `target_period_ms`、Note 8 `TargetStuck`、9 `TargetRejected`;cdex Note 16 `ManualExternal`。
- 待办:
  - [ ] 全仓 grep `RiskStateView.rr_max_cross`、`Fixed(ix)`、`GroupMask` 跟改(exec/risk/sequencer)。
  - [ ] J5:`risk_for`、`rx_ts_src` 适配器;BBO 健康成员 = up 且非 resync;advisor 路径节顺序、model tick UTC 对齐、`model_own` 投递、`ResetGroups(GroupResets)`;SetParams 查检查点盘;`SnapshotReader::desc_version`;`seq/views_access` 预算。
  - [ ] J6:`Note{ModelPathReset}` v[];hft-types:154 `MAX_BBO_CONN_SLOTS` 归 hft-types;control-plane 结构参数预览。
  - [ ] J8:检查点文件路径节头;replayer recompute 跳过窗口。
  - [ ] J9:⑦ 用 `minor_change_check`;recompute 例外规则。
  - [ ] J4:`first_leg_ix`;切片越价按 `risk_for`(Open 用 `ioc_max_cross`)。
  - [ ] J10:插件不拥有结构字段、预热去重、`relax = None` 按可能放宽;3 篇 ADR(`model-path-section`、`manual-hold-and-stop-marker`、`latency-acceptance-evidence`)。
  - [ ] SDK 暴露 `dust_notional`(J3 需要)——核对 J2 是否已加。
  - [ ] W3 P1-27 `OrderIntent.reason`(等 hft-event)。

## J4 exec / OMS(完成)
- 新名:exec `ExecOut.cancel_resting`/`CancelResting{account, inst}`;`LinkNotice.flags`/`LinkUpdate.flags`(`RESUME_HOLD = 1`)、cell `LINK_HOLD`;`Engine` 子码 `CancelResting=5`、`HardAlign=6`;SerialFallback 原因 `NotReady=4`、`ResumeHold=5`、`FirstUnknown=6`(2、3 作废);`AutoFixFail::RiskReject=9`(3、5 作废);`AutoFixPhase::WaitHold=7`;`ExecOrderKind` 1–10;节 21 `ExecCellRow` 400 B、`ExecAcctRow` 24 B;OMS `cancel_resting_on(a, inst, now, cause_seq, out)`、`smoke_intent(...) -> Result<OrderIntent, SmokeErr{Busy, NoQuote, BadInst}>`、`on_smoke_ttl(a, v, …)`、`write_off_residual(a, g) -> Result<WriteOff{venue, inst, leg_ix, qty}>`(单腿)、`ApprovedIntent.smoke_ttl_ms`、`ForeignFill.trade_id/cum_filled`、`SlotFlags::SMOKE_DUE=64`、`OutItem::Link=4`、`TableRow.smoke_slot`、码表 `OrderState`、`TransitionCause`、`OmsAnomaly` v[0](`SmokeFilled=3` → P0);`LegState` 192 B / 行 200 B;exec cell 640 B。
- 待办:
  - [ ] J2 SDK:`InstRiskView` 补 `quote_rate`、`rate_backoff`;`Views::venue_ready(a, v)`;`LegView` 注释链接 OMS 在途划分;`Rejected.venue_code` 注释(SendStatus)。
  - [ ] J5:`ModeOutcome` 撤单项 `open_only`;#7 Flatten/Hedge 只扣即时在途;`SMOKE` 在 #3 CloseOnly 放行;`l3_max_cross`;`Origin::Command` 用于冒烟;`price_hedge` tick;ForeignFill→ExternalFill `fill_seq = cum_filled`;GROUP_NONE `on_external`;SimReset 前置"无非终态 IOC";ledger `AdjustDelta::WriteOff` = 移出 group 归属。
  - [ ] J6:control-plane `CancelAll` 可撤状态含 `PendingAmend`;hft-event `LinkRole` 归属措辞;核销口径(control-plane "residual 清零" vs OMS 移出 group 归属,单腿)统一。
  - [ ] J9:testing DST/不变量:自动平仓零成交只重试(`NoFill`、`Gated` 作废)。
  - [ ] J8:`DustBelowMinNotional`;exec cell 640 B 复核内存表。

## J8 hft-core / replayer / hft-sim(完成)
- 数字:c16 prod 合计 9,984 MiB、大页 1G×6 + 2M×1,728;c32 prod 10,624 MiB、2M×1,792;c16/c32 batch slice 4,992;hft 合计 c16 ≈ 22.5 GiB、c32 ≈ 24.9 GiB;c32 冷进程核 18、29,批处理 19–21、30–31(+28 未启用 ENI-B2 时),system.slice `0,18,29`;I/O 上限 dev2 批处理读 40/写 20 MB/s、升级主机批处理读 100/写 40、sim 写 30;gp3 ≥ 3,000 IOPS、≥ 250 MB/s;护栏旧系统 cgroup cpu 压力 > 20%(待标定);环 `acctreq`/`acct` 64 / 2 MiB、md-bulk 4 MiB(增量深度 16 MiB)、rep-bulk 1 MiB;dev2 sim `max_instances` 6、`max_cells_total` 160、advisor 160 MiB。
- 名:`stop-marker.json.consumed`;replayer `latency`、`--stress-spec`、`--keep-going`、`--emit-journal`、`latency.json`/`latency.hdr`、基准 `latency_day`;hft-sim `StressCase::Custom`;`host.irq_cpus.eni_b2`;c16 新计划:两条 feed-bbo + 一条 feed-md;升级主机 warm/adv `Park{300}`;`[[accounts]].label`。
- 待办:
  - [ ] J10:deploy:263 护栏二级动作不停 watchtower/ops-bot(J8 定,合并时采用);自动重启加"此后无人启停过该单元";`hft-shm-release` 去 `$SERVICE_RESULT`;hostprobe 去 NAPI 检查(改 conn-builder netlink 自检);c4 IRQ 布局;system.slice 核号链接 hft-core;IR R/P 节;launch:83 `label`。
  - [ ] J7:NAPI 归属自检在 conn-builder(netlink);`feed.conns` 字段 `{stream, src_ip}`。
  - [ ] J5:ReplaceRefdata Drain 末复核的 Input 与"撤销待生效引用";SetParams 查检查点盘;运行中新增 group 的 `DustBelowMinNotional` 复核。
  - [ ] J6:`cli:systemd` 启动 Kill 权限;复核失败 P1 Note 码。
  - [ ] J11:hft-rt:254 dev2 空闲策略;architecture c8 例外;c16 新计划在 latency-budget/A2/11 的描述。

## J7 venue / net(完成)
- 新名:hft-venue `feed.ready_poll`、`feed.depth`、`feed.topn_levels`、`feed.snapshot_part_levels`;`acctreq-<venue>` 环;`AcctReq`、`AcctRecHdr`、`AcctFetchTask`;`ConnState.reason` 新码 `CONN_LOST`、`NEW_SUB`、`REQUESTED`;venue-proto `StreamKind`、`RouteSpec.topn_levels`、`type AcctCtx`、`AcctQuery.weight`、`classify(.., route)`、契约 26、27;hft-net `ReadyPoll`、`ReadyHandle`、`ConnSet`、`NetErr`、`Connector::napi_ifindex`;基准 `venue_gw_margin_push`;删 `venue_feed_all_bbo_check`。
- 待办:
  - [ ] J6:`FeedCmd::Subscribe` 加 `streams`(位值同 SDK `EvMask`);hft-event 环拓扑 `acct-<venue>` 写成 "acct-fetch → gw,hft-venue 内部字节环"。
  - [ ] J5:inst 订阅 `EvMask` 并集变化时重发 `FeedCmd::Subscribe`。
  - [ ] J1 措辞核对:`acct-<venue>` 流向(J1 写成进 sequencer 的环)→ 改为交回 gw。

## J9 cold apps / testing / benchmarks(完成)
- 新名:testing 用例 BK-5、BK-6、MS-5、MS-6、AX-1,日报字段 `generated_utc`;telemetry `EventWeightedCfg`/`EwKey`/`EventWeightedReport`、`InterferenceParams`/`BlockRow`/`GroupVerdict`、`cal_max_err_ns`,基准 `telemetry/event_weighted`、`telemetry/interference`;testkit `--crash-dir/inflight.json`、`invariant = "panic"`;kill-switch 标记格式 `{"v":1, req_id, scope: "*"|"<label>", utc_ns}`、`Done.marker`;watchtower `/healthz` v2、`host.maint_file`(默认 `/run/hft/maint/until`)、`interference.*`;archiver 配置 `host` 取代 `host_id`、`check --prefix`;基准 `feed-ingest/poll-cadence`。
- 口径:全量 ADL 到 `exch_qty = 0` 算"同号缩小"(AUTO_EXTERNAL);HashTrail 改在标签工作流同提交上跑 x86(每夜不再存)。
- 待办:
  - [ ] J6 tools:`hft-verify` 读 `inflight.json`、信号分类、疑似失败与 CI 复现结论、分段 CPU 秒、`path_digest`;scaffold-gate `--with-model --with-pipeline` + `hft-core --check` + 复用 target。
  - [ ] J10:标签工作流跑 x86 HashTrail;development 链接 testing §6 超时值;CI 复现疑似失败的作业;`main` 推送 cancel-in-progress;host-upgrade 第 8 步 2 h 块、`--start` 偶数 UTC 整点;deploy 让 hft-core 能读并改名 `/var/lib/hft/kill-switch` 下的标记;launch/IR 改用 SmokeOrder。
  - [ ] J8:hft-core 步 5 解析 scope(`"*"` → Global,否则 Account);hft-core:186/710/711 链接 testing;replayer 路径节丢弃日写 `recompute_from_seq`。
  - [ ] J5:删关键帧分片两个名字;`AUTO_EXTERNAL` 数量定义(含交易所仓位为 0)。
  - [ ] J2:`MinorChange` 语义与 recompute 重建点。

## J6 L0 / control / subsystems / tools(完成)
- 码值:`CmdKind` `0x001A SmokeOrder`(16 B)、`0x001B WriteOffResidual`(8 B)(Admin、确认、升级类);`ParamInvalid` arg = `kind | field << 4`(`0xFFF` 未指明);detail `313 SmokeActive`、`314 SmokeNoPrice`、`315 GroupNotBroken`、`316 NotQuiescent`(PreconditionFailed)、`402 RefdataAssembly`(gateway Rejected,arg 1–5)、`403 SmokeRiskReject`;`ModeErr::InvalidScope` → `BadCombination`;`TooManyGroups` → kind `Strategy`;`OrderFlags::SMOKE = 1<<5`;`Timer{Oms, SmokeTtl=5}`(`timer_id = 5 | account<<8 | venue<<16`);Note `ExecManualHold`(Exec,P1)、`ModelPathReset`(Advisor,P2)、`ResidualWrittenOff`(Ledger,P1);`ExternalFill` Note 按 cause 分级;`LatencySample` v3(`trigger_kind`,136 B);`L2SnapHdr.origin TopN=3`;`ReconcileReport.sflags AUTO_EXTERNAL=8`;`CmdPreview.reset_groups/no_open_ms`、`CmdResult` v3;`CmdResponse.cmd_hash`;`RefdataAssemblyCheck`;`LoadErrCode::InstanceIdRange`;`GroupId::compose`、`GroupId::N_MAX`;hft-types `funding_settle_norm_ns`;tools `cargo xtask path-digest`、`fuzz --repro`、`hft-verify crash resolve`、`resolutions/`。
- 待办:
  - [ ] J5:hft-risk 237/327/388/389 行 Note 产出方改为 sequencer(NOTESRC 5 条);hft-ledger `norm_settle_ts` 坏锚点 → 链接/包装 hft-types `funding_settle_norm_ns`。
  - [ ] J8:hft-core 626/820 ReplaceRefdata 装配校验失败 → gateway 直接回 `Rejected{RefdataAssembly}`(不是 CmdResult)。
  - [ ] hft-venue:208、hft-sequencer:330 重写 `MAX_BBO_CONN_SLOTS = 8` → 链接 hft-types。
  - [ ] J4:hft-oms `smoke_far_bp` u16 bp → `Rate`;hft-oms:679 Note 数值归属措辞。
  - [ ] J10:ops_bot `hft-opsbot`、`CmdResponse.cmd_hash`、Viewer Annotate、二次确认与群聊放行链接权限表。
  - [ ] J2:SDK 给出 `reset_groups`/`no_open_ms` 估算口径(`ResetPreview`)。

## J5 risk / ledger / sequencer(完成)
- 新名:ECF1 段 5 `keyframe_period_ms`、`advisor_ckpt_max_mib`、`auto_resume_after_clean_drain`、`allow_missing_standby`;`RingClass` 1–7;`FatalReason::ClockRange`;`ParsedConfig.model_ckpt/structural_changed`;删 `DoneCause::DrainTimeout`;节 10 `bbo_flags.SW_TS`;节 19 `ReconPendingHdr.n_stored`;基准 `seq/recon_funding_item`、`seq/recon_group`、`risk/view_for`;risk `view_for → InstRiskView`、`open_qty_immediate`、`pending_reduce_immediate`、`ModeCancel`、`GateChange`、`DrainCause::IdentityMismatch{account, venue}`、`ModeErr::NotResumable`、`GateEvent::OrderConns{primary_up, standby_up}`、MarginRow 88 B v3(`mark_ens`)、`Disposition::AutoExternal`;ledger `norm_settle_ts`、`FUNDING_TS_QUANTUM_NS`、`aflags.AUTO_RECON`、`FillDelta._pad2/_pad3`。
- 裁定点:AUTO_EXTERNAL 由 sequencer 在本 Input 内合成 ExternalFill(不另写 Input);停机标记 Kill 由实盘驱动在 EngineClock 之后写入;`rx_from_seq` 规则使快照后 `BboView.rx_tsc` 为 0 直到收到新 Bbo。
- 待办:
  - [ ] J6 文件:journal-format `state_hash` "按写出顺序";GateRow 加 `standby_ok_venues`(56 B,v4)。
  - [ ] J8 文件:hft-cfg 写 `advisor.checkpoint_max_mib`、`startup.allow_missing_standby` 进 ECF1 段 5;启动核对 TOML 与段 5 预算。
  - [ ] hft-ctl:算 `model_ckpt`、`structural_changed`,从段 5 读预算。
  - [ ] J4 文件:OMS:655 "风控仓位检查计两类在途之和" → 只即时;`InflightBook` 按 (account, inst, side) 只计即时在途读数。
  - [ ] SDK/exec 对齐:`InstRiskView` 补 `quote_rate`、`rate_backoff`;`l3_max_cross` 在 `InstRiskView`(exec 读 `risk_for`)。
  - [ ] 资金费归一化:hft-types `funding_settle_norm_ns`(J6)vs ledger `norm_settle_ts`(J5)→ ledger 包装 hft-types;reconciler 用 hft-types。
  - [ ] dp/hpr 平移公式归属 hft-exec(J11 已做?核对)。
  - [ ] SDK:646 重抄 BboView 聚合规则 → 链接 sequencer;`seq/views_bbo` → `seq/bbo_view`。
  - [ ] 裁定:W1 P2-16 SerialFallback 启动宽限(exec "不设宽限" vs 简报)→ 主控裁定。

## J10 python / deploy / dev docs / runbooks / notes(完成)
- 新名:组 `hft-pipeline-api`、`hft-ack`、`hft-stopmark`;可选用户 `hft-chain`;`/run/hft/maint/until`、`/run/hft/pipeline/api.sock`、`/etc/hft/host-local.toml`(`manifest`)、`hosts/<host>.standby.toml`、`/usr/local/sbin/{releasectl,hft-maint}`;清单 `device_index`、`queues = "max"`、`[storage].gp3_throughput_mibps/gp3_iops`、`[egress].prod_bulk`;秘密 `hft/ops/<unit>/`、`/etc/hft/secrets/<unit>.toml`;CI `quota-gate`、`QuotaSkip`、`CI_MINUTES_USED`、`fixtures/dst/hashtrail.toml`;ops_bot `/smoke`、`/writeoff`、`/stop … core`;`hft-dev2-guard clear`;`hft-sim-alternate --seed --start`;pipeline `host`、`hft.instances[].s3_prefix`、`next_ctr`、`WarmupSnapshot.params`;ops_bot `secrets_dir`;venv 按版本 `/opt/hft/current-cold/venv`;`nightly-arm` 10 min。
- 风险:tripped/窗口标志在 /run,重启后消失。

## 合并 K3(完成)
- 新名:`Note{RefdataRecheckFailed}`(Sequencer 源,P1);`FeedCmd::Subscribe{inst, streams: u8}`;hft-cfg `build --kind engine --core-config`;hft-ctl `GatewayConfig.advisor_ckpt_max_mib`、`Gateway::new(.., active_params)`、SetParams 未读结果时 `Busy`;GateRow 56 B v4。
- 跟进:
  - [ ] K1 hft-oms:残余核销第 3 步"全部 residual 清零" → "该腿";约 721 行可撤状态直接列三种。
  - [ ] K2 sequencer:`Note{RefdataRecheckFailed}` 的 v[];按 `streams` 重发 Subscribe。
  - [ ] K4 hft-core:约 630 行写名 `RefdataRecheckFailed`;装配传 `active_params`、`GatewayConfig.advisor_ckpt_max_mib`。
  - [ ] event-model:182 重抄 BboView 聚合规则 → 链接 sequencer(主控)。

## 合并 K1(完成)
- 新接口(复核):`Model::WARMUP: bool`(缺省 false)用于 `ResetPreview.no_open_ms` 估算;`ResetPreview{reset_groups: u32, no_open_ms: u32}`;OMS `InflightBook.imm_buy/imm_sell`;`on_smoke_ttl(.., keep)`;`recovery_end` 对未终结冒烟单置 `SMOKE_DUE`。
- 跟进:
  - [ ] K2:`on_smoke_ttl` 传 `keep`(Draining 且 cancel_resting = false);`Draining` cap 带 `cancel_resting` 并进快照;约 470 行撤单持续义务只限 cancel_resting = true;`ParsedConfig::StrategyParams` 带 `ResetPreview`;RiskView 即时读数读 `imm_buy/imm_sell`;`Timer{RefdataRecheck}` NOTEREG。
  - [ ] 主控:portfolio-carry 与 cdex 声明 `Model::WARMUP = true`(pc 第 530 行时长改按 SDK 规则)。
  - [x] brief §7 tripped 路径改为 `/var/lib/hft/guard/tripped`。

## 合并结论(2026-10-03)
- 12 个修订代理 + 5 个合并代理(K1–K5,裁决 R1–R12 见 `round8e-merge-fixes.md`)全部完成;主控补:SDK `InstRiskView.quote_rate/rate_backoff`、`Views::venue_ready`、`RiskStateView.dust_notional`;sequencer `venue_ready`/`dust_notional` 视图行、`reset_preview`、`on_smoke_ttl keep`、Draining 的 `cancel_resting`、**引擎自身订阅**(交易腿 BBO,永续腿 MARK + FUNDING);hft-event `Timer{Sys, RefdataRecheck=18}`;control-plane `126 DustBelowMinNotional`;event-model 聚合规则改链接;reconciler/hft-py 用 `ledger.funding_grid`;pc/cdex `WARMUP = true`;testing nightly-arm 10 min、`conformance-x86`;二级动作含 `hft-reconciler`(deploy 白名单同步);11 §3.2 锚点。
- check_hft.py:problems=0(warnings 6:CFGREF 5 为 app 自有键的启发式误报,HISTWEAK 1)。heading_audit:对 round8e-pre 丢 1(pc "小改白名单" → "小改属性",有意);对 round8d-pre 4 处均为有意改名。
