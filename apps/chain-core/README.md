---
description: "链上执行进程(cold tier,optional,链上策略迁移时启用):装配 hft-chain-evm 的 free profile(免费公共 RPC + SwapGuard),拥有端点连接、专用执行钱包、chain journal、最小控制面与 hft-core 的外部 venue 会话;链上策略免费版的链上腿"
kind: app
tier: cold
scope: optional
status: frozen
---

# chain-core

## 概要

> **冻结**:不随 core 规格同步;解冻(触发条件同原文)时先按 core 规格复核。已知待复核:无(`architecture-v2/_work/review-2026-10-03-M0c-V3一致性.md` §2 中没有只涉及本组件的行)。

> **可选扩展:CEX-DEX 策略从旧框架迁移时启用。** 新框架首要服务 CEX-CEX 策略,链上策略继续在旧框架运行、迁移不急;本规格保留,启用前按届时的实际情况复核。下文中的"core""迁移范围内"指该迁移启用之后的范围;外部 venue 协议与链上事件编号段在架构上保留。

`chain-core` 是链上策略免费版的链上腿执行进程:它装配 [hft-chain-evm](../../crates/hft-chain-evm/README.md) 的 free profile(免费公共 RPC 与 WS、同一签名交易并发提交、≥ 2 个独立运营商回执、nonce 槽候选集合、`SwapGuard` 链上过期、gas 上限),拥有端点连接、**专用执行钱包**私钥、chain journal、自己的最小控制面与 hft-core 的会话端点。它不拥有 CEX 状态、策略或对冲逻辑——对 hft-core 它是一个外部 venue,经同机共享内存环连接([external-venue-protocol](../../docs/subsystems/external-venue-protocol.md)),两者都是迁移范围内的 core 组件。它是 **cold tier**:可以用 tokio、成熟的 RPC 客户端与离线 journal 写者,不进 hft-core 的热线程计划、不占隔离核。builder profile(付费 bundle、块流、自建节点)是可选扩展,本进程只保留 profile 抽象。Solana 实例 `chain-core@solana` 是可选扩展([hft-chain-solana](../../crates/hft-chain-solana/README.md))。

相对旧框架的 Python 链上引擎(迁移前照常运行、不做改动),本进程从结构上消除的问题与对照依据见 [hft-chain-evm 改进点](../../crates/hft-chain-evm/README.md#相对旧框架链上引擎的改进点):传输错误只进 `Unknown`、本地 nonce、链上过期、多运营商回执、事件驱动的证据检测、`TxLifecycle` 进 seq journal 后同一事件内发 CEX 对冲(`leg_gap`,替代现在中位 146 ms、p90 30 s 的 Python 对冲)、统一门控与记账、可回放。**不改变的**:包含速度与被抢跑(公共 mempool),只报告不设目标。

## 目录

- [使用本组件](#使用本组件)
  - [部署与线程](#部署与线程)
  - [交易的链上过期与 DROPPED](#交易的链上过期与-dropped)
  - [专用执行钱包](#专用执行钱包)
  - [控制面(ctl)](#控制面ctl)
  - [与 hft-core 的会话](#与-hft-core-的会话)
  - [chain journal](#chain-journal)
  - [配置文件字段](#配置文件字段)
- [实现说明](#实现说明)
- [延迟与资源契约](#延迟与资源契约)
- [测试与验收](#测试与验收)
- [依赖](#依赖)
- [延伸阅读](#延伸阅读)
- [已知限制与后续工作](#已知限制与后续工作)

## 使用本组件

```
chain-core --config /etc/hft/chain-core-bsc.toml [--check]      # hft-chain-core@bsc.service(单元组 hft-chain.target)
```

退出码与 fail-loud 规则同 [apps/AGENTS.md](../AGENTS.md#配置与-fail-loud)(2 配置、3 主机、4 journal 恢复)。chain-core 与 hft-core 属于不同单元组、各自激活([deploy 单元组与分组激活](../../deploy/README.md#单元组与分组激活))。

### 部署与线程

与 hft-core 的 `prod` 实例同机。正式实盘只在统一升级主机上(`dev2` 不放正式实盘账户,只用于测试网与本地分叉联调);下表的预算按最紧的 `dev2`(现有 c7g.large,2 vCPU、3.8 GB)给出,主机类是部署配置,升级主机同样适用。cold tier 不使用 hft-core 的 `[[threads]]` 热线程计划([apps/hft-core 线程计划](../hft-core/README.md#线程计划)),线程形态如下,全部不绑核、不隔离核、不忙等:

| 线程 | 内容 | 调度 |
|---|---|---|
| tokio 运行时(`current_thread`,1 个线程) | 端点 I/O(预检、投递、回执、块头、余额、每块 Multicall3 报价)、ctl socket、会话环的轮询任务 | `nice = 5` |
| `chain-seq` | `ChainSeq` 同步状态机(单写者,私钥只在此线程);经有界通道收输入、发输出,空闲时阻塞等待 | `nice = 5` |
| `chain-journal` | chain journal 离线写者与周期 `fdatasync` | `nice = 10` |

- 与 hft-core 的会话环([`hft-rt::ShmSpsc`](../../crates/hft-rt/README.md))由 tokio 侧一个任务以"有数据即处理、无数据等 eventfd 或 1 ms 定时"的方式轮询,不忙等;`TxLifecycle` 到 hft-core 的额外延迟 ≤ 1 ms,对块级的链上腿可以忽略。
- **dev2 预算**:常驻内存 ≤ 200 MB、CPU 平均 ≤ 0.2 核;与 hft-core、现有进程的合计以 [apps/hft-core 线程计划 dev2 联合预算](../hft-core/README.md#线程计划) 为准;cgroup `MemoryMax=256M`、`CPUQuota=50%`,超出内存上限即被 OOM 而不是拖垮同机进程。watchtower 的 `HostPressure` 盯 swap 与 PSI。
- 公共 RPC 往返以毫秒到数十毫秒计,进程绝大部分时间处于等待;端点请求全部受令牌桶约束。

链上事实以已核实的为准:出块 0.45 s、终局 ≈ 1.125 s(Fermi,2026-01-14)、最低 gasPrice 0.05 gwei;免费端点的限额与域划分仍待核实。

### 交易的链上过期与 DROPPED

- 每笔 swap 经 `SwapGuard`([contracts](../../contracts/README.md)):`block.number <= maxBlock`、`minOut`;`maxBlock = 发送时块号 + N`(默认 3),无出价。
- 过期后上链只会 revert(`IncludedReverted{LATE_LANDING}`,只花 gas),不产生仓位。
- `Dropped` 仅当终局块号 > `maxBlock` 且 nonce 未被本交易消耗,或 nonce 已被同槽另一候选消耗(`NONCE_SUPERSEDED`);其余(超时、403/429/5xx、断连、限速)保持 `Unknown`。可用独立来源 < 2 持续 N 块 → P0 转人工,不推断。规则本体在 [hft-chain-evm](../../crates/hft-chain-evm/README.md)。

### 专用执行钱包

- 每账户一个专用钱包,只由 chain-core 发交易;迁移时接管旧框架的执行钱包(旧引擎停止且无在途交易后,私钥从旧系统配置中移除,步骤见 [migration runbook](../../docs/runbooks/migration.md))。
- 授权在上币时完成(ERC20 与 Permit2 `approve(token, spender, amount, expiration)`),交易路径上不授权;`Transfer` 只能到白名单地址。
- `Tainted`:nonce 被未知交易消耗或终局余额方向相反/短缺 → 拒绝该钱包新意图 + P0;人工核对后 `ClearTaint`。外部转入不触发。
- gas:失败交易上链花 gas,记入发起它的 group;普通意图日 gas 上限;`risk_reducing` 意图另有 gas 硬上限,超过 → 不发送、P0、转人工(B-27)。

### 控制面(ctl)

- ctl socket `/run/hft/chain-core-<chain>/ctl.sock`,复用 [hft-ctl](../../crates/hft-ctl/README.md) 的线协议与确认令牌;**角色与权限只在 [hft-ctl 的 chain-core 会话表](../../crates/hft-ctl/README.md#chain-core-的-ctl-会话)**。每条命令先作为 `ControlCmd` 写入 chain journal 再执行。
- 命令集、cap 模型与放行规则的唯一定义在 [hft-chain-evm 门控与控制命令](../../crates/hft-chain-evm/README.md):硬 cap 拒绝全部新意图;软 cap 只放行带 `RISK_REDUCING` 的意图;不按方向或路由推断。
- **全停**:`/kill chain` = `Pause{all, hard = false}`;`/kill all` = ops_bot 同时发 hft-core `Kill{Global}` 与本进程 `Pause{all}`,回执逐项列出;hft-core 的每账户 KillBits 位图经会话心跳传入,变化先写 chain journal 再生效。
- SIGTERM 等价于 `Drain`(`drain_timeout_s` 内等在途槽有结论,超时仍有 `Unknown` 则写 `Note` 后退出,下次启动先取证)。

### 与 hft-core 的会话

协议的唯一定义在 [external-venue-protocol](../../docs/subsystems/external-venue-protocol.md);`intent_id` 的定义归 [hft-event](../../crates/hft-event/README.md)。链上槽数量一律是代币数量:Buy 取 `amount_out`、Sell 取 `amount_in`(B-7)。会话建立或变化时主动推送上一会话全部未终结或 24 h 内的意图;路由由 hft-core 在 `Hello` 时经协议转发 refdata `ConfigBlob`,块边界切换。会话 `Down` 期间在途交易照常推进,不接受新意图。

### chain journal

- 与 hft-core 同一格式;journal 名 `chain-bsc`,`ChainSeq` 是唯一写者;离线写者(普通文件 + 周期 `fdatasync`),目录与命名见 [journal-format](../../docs/subsystems/journal-format.md#三类-journal)。
- 内容:块头与终局头、端点健康度、预检与报价批次、每端点投递结果、回执证据(含来源域)、钱包余额与 `Tainted`、`TxLifecycle`、gas 记账、KillBits 位图变化、ctl 命令、路由 `ConfigBlob`、段首与每 60 s 的 `ChainState`。
- 恢复:截断到最后一个完整事件,快照 + 其后事件重建;在途槽**先**向多个独立域重新取证、钱包按终局块核对、nonce 用 ≥ 2 个独立域的 `latest` 计数校准,再接受新意图。

### 配置文件字段

`/etc/hft/chain-core-<chain>.toml`,`schema = 1`。`ChainConfig` 各键(`send.profile`、`endpoints[]`、`free.*`、`evidence.*`、`gas.*`、`nonce.*`)的语义与初值见 [hft-chain-evm](../../crates/hft-chain-evm/README.md) 的"配置"一节。

| 键 | 含义 | 默认 |
|---|---|---|
| `instance.chain` / `instance.name` | 链;路径与单元名组件 | 必填 |
| `host.class` | 主机类(与 hft-core 相同的取值);决定资源上限的核对 | 必填 |
| `paths.journal` / `paths.secrets_dir` / `paths.ctl_socket` | journal 目录等 | 必填 |
| `node.expected_chain_id` | BSC = 56;端点 `eth_chainId` 不符 → 该端点禁用,全部不符退出 2 | 必填 |
| `wallets[]` | `{account, address, key_secret, guard, transfer_allowlist}`;地址与私钥推导不符退出 2 | 必填 |
| `contract.code_hash` | `SwapGuard` 字节码哈希;启动时向 ≥ 2 个独立域读 code 比对,不符退出 3 | 必填 |
| `link.path` | 与 hft-core 的会话环目录 | 必填 |
| `drain_timeout_s` | `Drain` 等在途结论的上限 | 30 |

## 实现说明

<details><summary>实现要点</summary>

启动:解析配置(全部错误一次报出)→ 载入秘密 → 打开/恢复 chain journal → 读 `SwapGuard` code hash → 端点探活(`eth_chainId`、`eth_blockNumber`)并核对 `receipt` 角色 ≥ 3 个独立域 → 钱包 nonce 校准 → 在途槽重新取证 → 打开 ctl socket → 等 hft-core `Hello` 并接收路由 `ConfigBlob`。退出(SIGTERM / `Drain`):停收非 `risk_reducing` 意图 → 等在途结论或超时 → 快照 → 退出 0。

| 文件 | 职责 |
|---|---|
| `src/main.rs`、`src/config.rs` | 参数、退出码、TOML 校验(链 id、钱包推导、code hash、端点域数、主机类) |
| `src/runtime.rs` | tokio 运行时、`chain-seq` 线程、journal 线程的接线 |
| `src/ctl.rs`、`src/link.rs` | ctl socket;hft-core 会话 |
| `src/recovery.rs` | 恢复与重新取证 |

</details>

## 延迟与资源契约

cold tier,无主机内延迟门槛。报告项([latency-budget §4](../../docs/latency-budget.md)):`chain_decide`、`chain_include_blocks`、`chain_include_seen`、证据 → `TxLifecycle` 出。资源:dev2 上常驻内存 ≤ 200 MB、CPU 平均 ≤ 0.2 核(联合预算见 [apps/hft-core](../hft-core/README.md#线程计划));冷启动 < 30 s(含重新取证)。

## 测试与验收

| 层 | 用例 |
|---|---|
| 配置 | 链 id 不符、钱包推导不符、code hash 不符、`receipt` 独立域 < 3 → 退出 2/3 |
| 生命周期(属性 + 故障注入 + 分叉 + 测试网 + DST) | 见 [hft-chain-evm 测试与验收](../../crates/hft-chain-evm/README.md#测试与验收) |
| ctl 与门控 | `Pause` 立即生效;`Resume` 只删匹配 cap;KillBits 位图变化先入 journal;`Drain` 退出 0 |
| 恢复 | 在 `Sent` 与 `IncludedOk` 之间 SIGKILL → 重启后先重新取证再接新意图 |
| 资源 | dev2 上与 hft-core、现有进程同机运行 24 h:内存 ≤ 200 MB、CPU 平均 ≤ 0.2 核、无 swap 增长 |
| 回放 | chain journal → `replayer verify --chain` 重建槽表与钱包状态([replayer](../replayer/README.md)) |

**迁移验收**:

| 项 | 类别 | 判据 |
|---|---|---|
| 测试 | 正确性 | 属性、故障注入、本地分叉、测试网、DST 全绿 |
| Shadow 运行(只预检、不发送) | 对照 | 链上意图判定与同期旧引擎日志**离线**对照(第七轮裁决 R7-4),差异逐条有解释 |
| 迁移切换后的正式实盘 | 改进项 | 传输错误被当作失败 0 次;重复 swap 0 次;过期后成交 0 次(被 `SwapGuard` 拦下的记 `LATE_LANDING`);单源回执当成交 0 次;未解释的 `Unknown`/`Tainted` 0 次 |
| 迁移切换后的正式实盘 | 报告项 | 证据检测延迟、`leg_gap`、revert 率与 gas 合计;包含块数与被抢跑率(**不改变**,只报告) |

## 依赖

- 内部:`hft-types`、`hft-event`、`hft-refdata`、`hft-rt`(会话环)、`hft-journal`(离线写者)、`hft-chain-evm`、[hft-ext-proto](../../crates/hft-ext-proto/README.md)、[hft-ctl-proto](../../crates/hft-ctl-proto/README.md)。
- 外部:`tokio`、`alloy`、`rustls`、`serde`/`toml`。
- 禁止:`hft-sequencer`、`hft-oms`;被 hft-core 链接。

## 延伸阅读

- [06 链上执行层](../../design/06-链上执行层.md)、[11 交付与迁移计划](../../design/11-实施路线图.md)、[research/03](../../design/research/03-onchain-bsc-solana.md)
- [external-venue-protocol](../../docs/subsystems/external-venue-protocol.md)、[hft-chain-evm](../../crates/hft-chain-evm/README.md)、[contracts](../../contracts/README.md)、[migration runbook](../../docs/runbooks/migration.md)
- [Agent Note:chain-core free 档](../../.agents/notes/proposed/architecture/2026-10-01-chain-core-free-profile.md)

## 已知限制与后续工作

- 不改变包含速度与被抢跑;可选扩展 builder profile 的触发条件见 hft-chain-evm(届时评估热化:去掉 tokio、进 hft-core 线程计划,以及 [chain-grpc-relay](../chain-grpc-relay/README.md))。
- 免费端点会改限额或下线;端点表靠配置维护,`SetEndpoint` 手工启停。
- 与 hft-core、现有进程同机:任一方失控会影响其他方;靠 cgroup 上限与 watchtower 告警兜底。
- 可选扩展:Solana 实例、F 类纯链上 backrun、欧洲发送点。

### 开发备忘

<details><summary>给实现者</summary>

- **待核实**:免费端点的限额与域划分(只读探测 24 h);`finalized` 头在各公共端点上的查询方式;publicnode WS 的稳定性。
- 实现顺序(按依赖):hft-chain-evm 纯状态机与属性测试 → 端点 I/O → ctl 与门控 → 与 hft-core 会话 → 故障注入、分叉、测试网 → Shadow → 迁移切换(正式实盘)。

</details>
