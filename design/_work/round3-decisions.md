# 第三轮裁决(红队评审之后,2026-10-01)

两份红队评审(热路径 34 条、链上/冷侧/运维 37 条)的结论:方向与细节密度到位,但存在接口断裂、恢复机制不确定、三处 Rust 所有权编不过、链上"bundle 有效期 ≠ 交易有效期"的资金风险,且阶段 1/1' 超载。以下裁决所有代理必须遵守;与本文件冲突的既有规格一律改。

## A. 范围收缩(阶段 1 / 1')

- A1 **阶段 1 单实例**:canary/第二实例全套(实例位的使用、`OTHER_INSTANCE`、共享用户流、专用核与出口 IP)推迟到阶段 2。`ClientOrderId` 的实例位保留(恒 0),不删。新版本对比在阶段 1 用"同实例 Shadow 策略 + 离线回放"。
- A2 **L2 单路**:BBO 双路赛马保留;L2 双路合并(`l2_race`、`gap_grace_us`)推迟。RSS 源端口预计算、TX 时间戳 errqueue、scratch TLS 预热全部推迟到有直方图证明需要时(阶段 1 结束的测量决定),规格保留为"阶段 2 可选",默认关闭。
- A3 **refdata 变更 = 重启(阶段 1)**:不做 EpochSwap 热替换;refdata 预装由 pipeline 每日生成的"交易宇宙"超集,SymbolGroup 成员、武装名单、阈值都在 `StrategyParams`(可热替换,按 (strategy, account) 一个 blob,内含全部 group)。`ReplaceRefdata` 命令阶段 1 = 写 ConfigBlob 后请求 Drain + 重启(冷启动 < 30 s)。热替换推迟到阶段 2,届时需列出允许的变更集。
- A4 **控制面阶段 1 最小集**:`Kill`、`SetMode`、`SetParams`、`SetRisk`、`CancelAll`、`Drain`、`Resume`(日亏/对账恢复)。其余命令保留规格但标 phase 2。角色阶段 1 只要 Observer / Trader / Ops 三个;确认令牌只用于升级类。
- A5 **阶段 1' 最小链上执行**:bsc 节点 + 两家 builder(48Club、BlockRazor)+ tx 生命周期状态机 + nonce 分配器 + 发送前用本机 `eth_call` 模拟(不用 revm)+ 最小接口(`submit_swap`、`query`、`list_open`、TxLifecycle 推送)+ **最小包装合约**(见 C1)。L0 池镜像、drift、PoolQuote、revm、chain-grpc-relay、UDP 可靠通道、其余三家 builder、出价自动调节、early_inclusion 推迟到阶段 3。
- A6 watchtower 阶段 1 只做 8 条规则(P0/P1 相关);冻结为草稿(规格保留,标 phase 3/4,不在阶段 1 评审范围):hft-chain-solana、venue-hyperliquid、法兰克福相关。
- A7 **硬依赖**:工程师 A 第 3–4 周交付 `hft-types`、`hft-event`、`hft-rt`、`hft-journal`(writer + reader);工程师 B 第 3–4 周只做节点、builder 连通性与投递 A/B 探测(Python 脚本即可);chain-core 第 5 周起基于 A 的交付开发。cdex 侧改造(客户端义务、钱包迁移、停用 BSCTrader 发送路径)排进路线图,负责人工程师 B,第 9–12 周。

## B. 热路径 / 核心

- B1 **Gate 成交**:以 `futures.orders`/`spot.orders` 的 update 为成交源,`cum = size − left`,均价取 `fill_price`/`avg_deal_price`;usertrades 只取 fee(按 trade id 去重关联)。`OrderEvent` 尾部追加 `avg_px`(累计均价);OMS 用 `avg_px × cum − filled_notional` 求增量名义。所有 venue 的回报必须能给出累计量,给不出的在 gw 内按 exec id 合成(Bybit `execution.fast`)。
- B2 **ConnState** 用 `_pad` 放 `account`(0 = 行情)与 `role`(Md / Order / UserStream);UserStream 断开同样通知 OMS(回报缺口 → 相关槽 VERIFY)。
- B3 **记录分类**:journal 记录分 Input / Output / Meta 三类。何时写快照、何时滚动、何时采样等**依赖运行环境的决定**由 live driver 当场写成 Input 记录(`Timer{Snapshot|Roll|Keyframe}`),回放照读;测量类(`LatencySample`、`Note{Measure}`、`ClockStatus`、`HostProfile`)是 Meta,占 seq 但**不参与验证比对与状态**;引导块比对屏蔽 Meta。
- B4 **恢复**:每个输入事件及其全部输出写完后落 `EventEnd` 标记(可用记录头 flags 位表示,不新增记录更好)。冷启动截断到最后一个完整事件;用 `RecoverDriver`:照常应用输入,输出与日志不一致只告警、不 Fatal,重新生成的输出写入新段并标 `Recovery`;**版本变化时从最近快照起步、不重放尾部**;随后与交易所真值对账,通过后才允许武装(武装需人工或满足自动条件)。验证回放(逐字节)只作为离线夜跑与 CI 工具。
- B5 **符号表**:静态最小完美哈希(确定性构建,一次探测)或双选桶;"≤ 2 探测"降为基准目标,不作为加载失败条件。
- B6 **L2Batch**:新增变长 kind `L2Batch`(一条交易所消息一条记录:头 + N 个 {px, qty, side});`L2Delta` 保留供需要逐档的读者但 feed 默认产出 L2Batch。sequencer 一次应用整条消息,只在整批应用后更新 mid、调用策略(不会在半条消息上调用策略)。
- B7 **bookTicker 独立连接 + 高优先级 bbo 环**:每个 venue 的 BBO 流与深度流不同连接;feed 到 sequencer 分 `bbo` 环(高优先级)与 `md` 环;sequencer 优先级:ctl > 回报 > bbo > advice > md。
- B8 **KillWord**:hft-rt 提供 `KillWord = CachePadded<AtomicU8>`;hft-core 启动期分配 `&'static [KillWord]`(每 gw 一个),传给 `Sequencer::boot` 与 `Risk::new`;Risk 只写不拥有;restore 不重建。
- B9 **所有权交接**:hft-rt 提供 `Mailbox<T: Send>`(单槽所有权交接,unsafe 封在 hft-rt 内),用于 gateway/warm → sequencer 的大对象(解析好的参数)。ctl 环统一为 `ByteSpsc`,容量按字节给(≥ 256 KiB,单条 ≤ capacity/4)。
- B10 **FeedCtx/Templates**:解码接口显式传 `rd: &RefData`;`FeedCtx` 只保存 venue 私有状态;模板随 refdata 一起构建(阶段 1 refdata 不热换,故模板启动期构建一次)。
- B11 **Shadow 不耗令牌**:风控拆 `evaluate(&self)`(只读)与 `commit(&mut self)`(扣令牌、计在途);Shadow 只 evaluate。
- B12 **删 sequencer 侧 T5 空跑**;阶段 1 不做任何"每 tick 空跑下单路径";由 `seq/bbo_fire` 冷热差测量决定阶段 2 是否加回(若加,需无副作用路径)。
- B13 **布局**:所有 `#[repr(C)]` 结构写显式 `_pad`,旁边写 `size_of`/`offset_of` 编译期断言;`LinkEntry` 修正;`OrderFlags` 扩为 `u16`;`fee_asset` 用 `AssetId(u16)`。含枚举字段的体用 `TryFromBytes`;尾部新增字段不得是枚举(或 0 必须是合法的"未提供"值)。
- B14 **输出扇出**:OMS/风控每个事件最多发 K 条(K 配置),剩余由 Input 记录 `Timer{OmsContinue}` 推进;Kill 优先用 venue 级 `CancelAll`。
- B15 **旧段回收**:写者滚动后经 SPSC 把旧段句柄交还 housekeeping,宽限期后 munmap + unlink;启动自检把"已 unlink 仍映射"计入大页短缺。同进程尾随读者预映射固定段槽位、不反复 munmap;mimalloc `purge_delay = -1`。
- B16 **IRQ 放 hk 核,热线程纯 busy poll**(`irqaffinity` 只含 hk 核);`IRQ_ON_HOT` 保持为真告警。feed/gw 核的 `nohz_full` 做 A/B,默认只给 sequencer 核。
- B17 **gw/feed 循环**:每 poll 完一个 socket 就检查一次出环;standby、REST、UserStream 每 N 圈 poll 一次;基准加"8 socket 空闲"用例。
- B18 **数据量**:阶段 0 用 S3 现有数据按目标订阅实算事件率;段大小、保留份数、冷盘容量按实测重定(当前数字标"待实测");验收改为"每个段都有完整引导块"。段滚动时刻 = 整点后第一个空闲轮且避开资金费结算前后 ±2 min;段头 config 表不设 16 的硬上限(变长)。
- B19 **缺失类型补齐**:`L2SnapshotPart`、`ExchIdHint`、`SlotId::UNRESOLVED`(在 hft-types)、`CtlRecord`、`TscUtc`;hft-rt 的 `Producer/Consumer` 名字为准;`JournalWriter::append` 返回的 seq 为唯一来源(sequencer 不自行计 seq);`GwCmd` 维护类 op 加 `SetEpoch{epoch, accept_prev}`;Rate 精度统一 1e-9。
- B20 **PHC**:用 `PTP_SYS_OFFSET_EXTENDED`,报告误差界;不设 200 ns 窗口。
- B21 **syscall 审计**用 bpftrace `raw_syscalls:sys_enter` 按线程名计数,不用 strace。
- B22 **mock 回环测生产二进制**:额外 CA 只在配置含 `endpoint_override` 时加载,生产配置禁止该项。
- B23 **未登记的交易所错误码默认进 Unknown**,不默认 Reject;每所 `codes.rs` 显式列出"结果未知"码(OKX 50004/50001/50013、Bybit 10000/10016 等)。写进 hft-venue-proto 的实现者契约。
- B24 **无过期保护的所**(Bitget、KuCoin 待核实):NotFound 只能由对账收敛,不能按超时定终态。
- B25 OKX:查单走 REST `GET /api/v5/trade/order?clOrdId=`;撤全部用 `batch-cancel-orders` + REST 挂单列表兜底(`mass-cancel` 只适用期权)。OKX `cum = accFillSz × ctVal`,fee 取 `fillFee`(增量)。
- B26 Binance:listenKey 每 30 min 续期(GwOp 维护类);`ACCOUNT_UPDATE m=FUNDING_FEE` → `FundingSettle`;REST 用独立 HMAC key(Ed25519 只给 ws-fapi 会话)。
- B27 **reduceOnly** 只用于 HedgeFix、Flatten 等"平掉多出一腿"的单;chain-first 的 LegLink 对冲是开仓,不带 reduceOnly。

## C. 链上

- C1 **每笔交易有链上过期**:阶段 1' 起用最小包装合约(`require(block.number <= maxBlock)` + minOut 校验 + 出价在合约内转给 builder,InTx),`maxBlock` = bundle 的 `maxBlockNumber`;Router 路径同时设 `deadline`。**DROPPED 仅当终局块号 > maxBlock 且 nonce 未被本交易消耗**;迟到上链只会 revert(只花 gas)。出价一律 InTx,不用独立的 N+1 转账。DST 增加"泄露/重组回灌后迟到上链"故障。
- C2 **chain-core 控制面**:最小 ctl socket,复用 hft-ctl 协议与权限表:`Pause`(降级,永远允许)、`Resume`(确认)、`ClearTaint`、`ReplaceRoutes`、`SetBidCap`;命令写 chain journal;ops_bot 加 `/kill chain`。chain-core 定义 `Drain`(停收新意图、等在途有结论、干净退出)。
- C3 **intent_id 唯一定义在 hft-event**;chain-core 删自己的定义只链接。去重键 `(intent_id, 意图体哈希)`:同 id 不同体 → `Rejected(IdCollision)` + P0,永不回放旧状态。
- C4 **chain-core refdata**:阶段 1' 用独立 `routes.toml`,经 ctl `ReplaceRoutes` 热替换并作为 ConfigBlob 记入 chain journal;阶段 3 由 hft-core 经外部 venue 协议转发 ConfigBlob,块边界切换并重新 Hello。
- C5 **临时接口改为 CPG 内网 TCP + 每帧 HMAC**(复用外部 venue 协议帧格式);cdex 留在自己的实例,不搬到热机。
- C6 `SwapIntentMsg` 加 `RISK_REDUCING`;chain-core 只在 HARD kill 全拒。
- C7 临时接口客户端义务补:`IncludedOk → Unknown(REORG) → Dropped` 时 cdex 必须回收已发的 CEX 对冲(对齐 hft-ledger"终局前重组");LOG_MISMATCH/BAL_MISMATCH 的处理;验收用例。
- C8 会话变化时 chain-core 主动推送上一会话全部未终结或 24 h 内的意图;hft-core 遇到未知 intent_id → NakedExposure P0。
- C9 **专用执行钱包**:阶段 1' 启用新钱包,只由 chain-core 发交易;增加 `submit_transfer`(目的地址白名单);`ClearTaint` 解除;尘埃转入不触发 Tainted(余额核对只看目标 token 与预期方向,容忍外部转入)。
- C10 **授权**:上币(onboarding)时完成授权(含 Permit2 `approve(token, spender, amount, expiration)` 建模),不在平仓路径上临时授权。
- C11 reconciler 链上钱包:按终局块号读余额,只与该块前已 Finalized 的意图比;有在途 swap 的钱包本轮不判级;节点历史状态保留需覆盖补核对窗口(PBSS 128 块不够时用 `balanceOf` at finalized 立即读,或节点保留更长历史——写明)。
- C12 drift(阶段 3)比较用"块 N 结束时的状态摘要";L1 预取只对有意图的路由。
- C13 **BSC 节点运维**:新增 `docs/runbooks/bsc-node.md` 与 `deploy` 的节点单元:快照同步、硬分叉升级日程(订阅 BEP)、剪枝、监控、revm/eth_call 对分叉的金样。
- C14 **发布**:chain-core 与 hft-core 分属不同单元组,各自激活;hft-core 发版不停 chain-core。
- C15 chain-core persist 独占一个 warm 核。
- C16 链上事实:0.45 s 出块、≈ 1.125 s 终局(Fermi,2026-01-14)、最低 0.05 gwei 已核实;"两家 builder 占 87–96%"、bloXroute bundle 块上限、Blocksmith/Jetbldr 端点仍为待核实,规格里不得写成事实。

## D. 运维与文档

- D1 ops_bot 群聊允许降级类命令(`/kill`、`/closeonly`),与 defensive-patterns 一致。
- D2 incident-response 与规格对齐(删除不存在的 `Halt`、`NeverAccepted`、`round_id`、`clean_streak`;恢复用 `Resume{ReconDeviation}`)。
- D3 replayer 加 `--chain`(ChainSeq 回放驱动)或删去 chain-core 验收中对它的引用——选前者但标 phase 3。
- D4 hft-py 补 `refdata.Builder`、`params.Builder` 与 ctl 客户端(或明确这些在纯 Python 包里);wheel 规格统一为一种(abi3-py310,单一 `/opt/hft/venv`)。
- D5 CI:CI 构建用 `x86-64-v3`,发布构建用 `sapphirerapids` 且只在目标机族上跑基准;`cargo deny advisories` 为非阻塞告警(独立任务),licenses/bans 阻塞。
- D6 Agent Notes:阶段 1 冻结新增 note 的格式检查(doc-check 只查链接);新增本轮关键决定的 notes(tx 链上过期、RecoverDriver、阶段 1 范围收缩、L2Batch、refdata 阶段 1 重启)。
