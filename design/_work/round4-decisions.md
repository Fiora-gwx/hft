# 第四轮裁决(2026-10-01,用户决定后)

用户决定:builder/块流/自建节点/Solana 付费服务继续暂缓;cdex 按新框架优化但**先用免费资源**;行情采集**不迁入**新框架;新框架必须能在现有 2 核服务器上运行(老板先看效果),之后按效果升级到 4–8 核。以下裁决优先于既有规格。

## R4-1 行情采集与交易数据分离

- 现有 collector(`E:\trading\Trading`,v0.3.7,collection-policy 控制、150+ 合约 × 多所、`book_ticker.v2` 等 Parquet、S3 管线)**保持独立**,服务研究分析;新框架不吸收它,不复刻它的数据集与 schema。
- 新框架的 journal(raw + seq)只记录**它自己交易所需的订阅**,用途是回放、审计、复盘交易决策;它不是研究采集系统。订阅集合由交易需要决定,不为研究扩大(扩大订阅会增加热线程负载)。
- `journal-archiver` 的范围缩小为"交易 journal 的归档与 Parquet 化",写到独立的 S3 前缀(如 `s3://gwx-trading-data/hft/<instance>/...`),**不要求与 collector 的 Parquet schema 兼容**;研究需要对照时,用两者共同的时间戳语义(交易所事件时间、接收 UTC)按时间对齐。
- 两套解码器并存是可以接受的代价;用 collector 的 raw WAL 作为新框架 venue 解码器金样的来源之一,并加一个"同一时段两边 BBO 序列一致性"的对照检查(离线)。
- 资源:2 核服务器上不同时长期运行 collector 与新框架实盘;collector 用于研究窗口时按 collection-policy 单独排期。
- 删除/改写设计书与规格里"collector 被 venue 线程吸收、不再有独立采集进程"的说法(01 §8、04 §1/§8、08 §9、11 阶段计划、hft 的 architecture/journal-archiver/fixtures 等)。

## R4-2 线程计划数据化 + 2 核兼容(主机类 `dev2`)

- 线程布局由配置驱动,不写死在代码里:`[[threads]]` 每项 = `{ name, core = <u16|none>, idle = <Spin|SpinThenPark{spin_us, park_us}|Park{park_us}>, tasks = [...] }`;task ∈ `sequencer`、`feed:<venue>`、`gw:<venue>`、`ext:<chain>`、`housekeeping`、`persist`、`ctl-gateway`、`conn-builder`、`advisor`。同一线程内按固定顺序轮流调用各 task 的 `poll_once()`/`step()`(每个 task 每轮有处理上限,保证公平);环与事件语义不变,确定性不受影响(sequencer 的输入顺序本来就记录在 journal)。
- 每个 task 的 `poll_once` 必须非阻塞、空闲时开销小,并返回"本轮是否做了事";线程按 `idle` 策略在连续空闲后退让(`SpinThenPark`:先忙等 `spin_us`,再 `park` 最多 `park_us` 或被 eventfd/futex 唤醒)。2 核主机上热任务也必须用 `SpinThenPark`,否则会抢走 cdex 的 CPU。
- 主机类(配置 `host.class`,与实际 CPU 不符则拒绝启动):
  - `dev2`(现有 c7g.large,2 vCPU,3.8 GB):线程 A = `feed:binance-usdm` + `gw:binance-usdm` + `feed:gate` + `gw:gate`;线程 B = `sequencer`;线程 C(不绑核,低优先级)= 全部 warm 任务;均 `SpinThenPark`;不隔离核、不用大页;journal 热段放 tmpfs(`/dev/shm` 具名文件,普通 4K 页,段 64–256 MB,预触摸 + mlock);订阅限制为几十个合约;hft-core 常驻内存预算 ≤ 600 MB;**无延迟验收,只做功能、正确性与影子**;与 cdex 同机,watchtower 盯 swap/PSI。
  - `c4`(4 物理核):已有的 4 核降级表(feed+gw 合并);影子可用,延迟验收不作要求。
  - `c8`(8 物理核,c8g.2xlarge):已有的小主机表;阶段 1 延迟验收在这一档做。
  - `metal`:阶段 4。
  - 升级 = 换主机类配置 + 部署对应的 GRUB/cpuset,**不改代码、不重新构建**(同一 ARM64 二进制;x86 则换构建目标)。
- tmpfs 热段与 R2 不冲突:R2 的问题是磁盘文件 mmap 的回写写保护;tmpfs/shm 页在内存里,没有回写,只是 4K 页的 TLB 开销更大。
- `hft-journal` 增加"小段 + 4K 页"模式;`hft-rt` 提供 `IdlePolicy` 与 park/unpark;`apps/hft-core` 的核分配表改为"线程计划"并给出 dev2/c4/c8/metal 四份;`deploy` 给出 dev2 的最小调优(不改 GRUB、不隔离核;只做 NIC 合并参数与 IRQ 不与热线程同核的建议,且可回滚)。

## R4-3 cdex 用免费资源接入新框架(chain-core "free" 档)

- builder/块流/自建节点不是 cdex 的前提。chain-core 增加 **free 档**(阶段 1' 默认):
  - 数据源:多个免费公共 RPC(现在 cdex 用的 bsc-dataseed1/2、defibit、ninicoin、nodereal 免费档等)的 HTTP,以及提供 WebSocket 的免费端点(有则订阅 `newHeads`/目标池 `logs`,没有则按块轮询);每个端点独立令牌桶与冷却(吸取并行 multicall 被封 IP 的教训),失败端点自动降权。
  - 发送:同一笔已签名交易并发提交到 K 个公共 RPC(同一 tx hash,幂等);可选配置免费的私有交易 RPC 端点(是否免费、是否可靠**待核实**,只作为配置项,不写成依赖)。无 bundle,`revertingTxHashes` 不适用:失败交易会上链并花 gas。
  - 交易保护:**SwapGuard 包装合约仍然必需**,而且比 builder 档更重要——公共 mempool 里的交易可能滞留很久后才上链;`maxBlock = 发送时块号 + N`(N 可配,默认 3),DROPPED = 终局块号 > maxBlock 且 nonce 未被本交易消耗;minOut 不满足则 revert(只花 gas)。部署合约的 gas 是唯一花费(约 1 美元级)。
  - 回执与证据:多个公共 RPC 交叉确认(≥ 2 源一致才 IncludedOk);WS `newHeads` 可用时按块拉回执,否则轮询(间隔可配,默认 150 ms);余额核对保留。
  - 定价:先用 `eth_call`(Quoter / 路由模拟),L0 池镜像在免费 WS logs 可用后再开(阶段 3)。
  - 不改变的:包含速度与被抢跑(这需要 builder 通道,付费,暂缓);在规格与分析文档里如实写明。
- 相对现有 Python cdex 的改进点(写进分析文档与 chain-core 验收):tx 生命周期状态机(传输错误只进 Unknown)、本地 nonce 分配器、SwapGuard 链上过期、事件驱动的证据检测、收到证据后同一事件内发 CEX 对冲(现在中位 146 ms、p90 30 s)、统一风控/kill/记账、journal 可回放。
- builder 档保留为同一 chain-core 的另一个"发送后端"配置(阶段 3+,付费,暂缓);代码上发送后端是可替换的(trait),free 与 builder 共用生命周期状态机与 nonce 分配器。
- 路线:阶段 1 新框架先做 CEX(Binance + Gate);阶段 1' chain-core free 档经临时接口服务现有 Python cdex(dev2 主机上同机回环 TCP + HMAC);阶段 3 cdex 的策略逻辑与 CEX 腿迁入 hft-core。
