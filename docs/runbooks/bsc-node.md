---
description: "可选:tokyo-chain 上 BSC 全节点的运维(首次部署与快照同步、监控、客户端升级与硬分叉、剪枝、故障处理)与 chain-core 专用执行钱包的运维;只在 cdex 迁到新框架、chain-core 启用后适用"
scope: optional
---

# BSC 节点与执行钱包运维

**范围**:本页整体是 **optional**。**执行钱包运维**一节在 chain-core 启用时适用(触发条件:决定把 cdex 迁到新框架,[ccex-first-scope](../../.agents/notes/proposed/architecture/2026-10-02-ccex-first-scope.md));**自建 BSC 节点**是其上的又一层可选扩展(触发条件:免费公共 RPC 的回执独立来源长期不足、被限流影响执行,或启用 builder profile);默认 chain-core 只用免费公共 RPC,下文节点各节在启用时适用。

## 适用范围

- tokyo-chain(i7ie/i4i,与 tokyo-cex 同 CPG)上的 bsc-geth(或评估中的 reth-bsc)全节点,单元 `bsc-node.service`([deploy](../../deploy/README.md#systemd-单元))。它是 [chain-core](../../apps/chain-core/README.md) 的权威状态源:`eth_call` 模拟、回执、nonce 与终局判定都依赖它([06 §2.1](../../design/06-链上执行层.md))。
- chain-core 的**专用执行钱包**:启用、授权、补 gas、资金调拨、`Tainted` 处理、退役。
- 链上事实以已核实为准:出块 0.45 s、终局 ≈ 1.125 s(Fermi,2026-01-14 起)、最低 gasPrice 0.05 gwei;其他数字(builder 份额等)待核实(方法:启用本页时对照 BNB Chain 官方文档与区块浏览器的统计),不作为操作依据。

## 前置条件

1. 实例在胜出 AZ 的 CPG 内([az-probe](az-probe.md)),两块本地 NVMe(≥ 2 × 4 TB,RAID0,`none` 调度器,[02 §2.3](../../design/02-硬件与托管方案.md));安全组只放行 tokyo-cex 的私网地址访问 8545/8546。
2. 节点二进制按发布纪律进入归档(不在节点上编译),版本与 SHA-256 记在 `deploy/hosts/tokyo-chain-1.toml`。
3. chain-core 可以被暂停:任何会让节点不可用的操作前先 `/kill chain`(chain-core `Pause{all}`)。

## 步骤

### A. 首次部署与快照同步 `[需授权]`(创建 AWS 资源)

1. 从官方快照源(BNB Chain 官方快照,或 48Club 提供的快照;以官方文档当时的推荐为准)下载最新 PBSS 快照到 NVMe,核对发布方给出的校验和。
2. 节点配置(`/etc/bsc/config.toml` + 命令行,随发布归档):路径式状态存储(`--state.scheme path`)、`--tries-verify-mode none`(快节点模式,[06 §2.1](../../design/06-链上执行层.md))、HTTP/WS 只绑私网地址、IPC 开启、`newHeads`/`logs` 订阅开启、静态 peer 列表(含 BlockRazor Full Node Sync,如已开通)。
3. **历史状态保留**:PBSS 默认只保留最近 128 块的状态(≈ 58 s)。reconciler 按终局块立即读余额在窗口内;若需要回补更早的块,在此配置更长的历史状态保留(参数名以所用客户端版本文档为准),并按保留块数重估磁盘([reconciler 钱包扫描](../../apps/reconciler/README.md#链上钱包余额扫描可选扩展))。
4. 启动 `bsc-node.service`,等待追平:本地最新块与两个公共来源(区块浏览器 API、另一运营商 RPC)的差 ≤ 1 块并保持 30 min。
5. 验收:本机(tokyo-cex → tokyo-chain,CPG)`eth_call` p99 < 1 ms([11 §3](../../design/11-实施路线图.md));`eth_getBlockByNumber("finalized")` 落后最新块 2–3 块;运行**分叉金样**(见 C.3)全部一致。

### B. 日常监控(watchtower / Prometheus)

| 指标 | 阈值 | 处理 |
|---|---|---|
| 本地头落后公共来源 | > 2 块持续 30 s → P1;> 10 块 → P0 并 `/kill chain` | 查 peers、磁盘 I/O、CPU;必要时重启节点(先 `/kill chain`) |
| 终局头落后最新块 | > 10 块 | 可能是网络终局问题或节点问题;对照公共来源 |
| peers 数 | < 配置下限 | 检查静态 peer 与安全组 |
| NVMe 可用空间 | < 20% P1、< 10% P0 | 按 D 剪枝或扩容 |
| `eth_call` 延迟(chain-core 侧直方图) | p99 > 3 ms 持续 10 min | 节点负载或 CPG 网络;与 chain-core 的 `chain_decide` 一起看 |

### C. 客户端升级与硬分叉

1. **订阅**:BSC 的 BEP 与客户端 release(GitHub releases、官方公告频道);每个硬分叉在 ops 日历上登记激活高度/时间,至少提前 2 周排升级。
2. **预演**:在一台临时实例上用新版本从快照启动并追平;期间保持生产节点不动。
3. **分叉金样**(`fixtures/chain/bsc/`):一组固定块高上的 `eth_call` 请求与期望结果——包装合约的模拟调用、Router 报价、ERC-20 `balanceOf`、Permit2 授权查询;启用可选的 revm 定价时,同一组金样也用于 revm。新版本对金样逐项比对,全部一致才可切换;分叉改变语义(预期内的差异)时,在 PR 中更新金样并说明。
4. **切换** `[需授权]`:`/kill chain` → chain-core `Drain`(或确认无在途意图)→ 停旧节点 → 启新版本(同一数据目录,若版本要求迁移按官方说明)→ 追平 → 金样 → chain-core 启动/`/resume chain`。硬分叉激活点前 ≥ 24 h 完成。
5. 回退:保留上一版本二进制;新版本追不平或金样失败 → 回到旧版本(硬分叉激活后不可回退到不支持分叉的版本,此时只能修复前进)。

### D. 剪枝与磁盘

1. PBSS 下状态自动剪枝;链数据(区块、收据)增长按周记录。可用空间 < 20% 时按客户端文档做离线剪枝 `[需授权]`(节点停机期间 chain-core 必须 `Pause`),或扩 NVMe(换更大实例)。
2. 剪枝前后都跑一遍 A.5 的验收。

### E. 故障

- 节点不可用:chain-core 的模拟与回执失去权威源——立即 `/kill chain`;付费 RPC 只作回执备份,**不用于模拟与发送前校验**。
- 节点数据损坏:从最新官方快照重新同步(A.1–A.5);在途意图的结论由 chain-core 用回执备份源与重同步后的节点取证。

### 执行钱包运维

chain-core 的执行钱包**只由 chain-core 发交易**;任何其他来源的发送都会让钱包进入 `Tainted`([chain-core](../../apps/chain-core/README.md))。

1. **启用新钱包** `[需授权]`:在 Secrets Manager/KMS 生成私钥(不在任何开发机上出现明文);地址登记到 `chain-core-bsc.toml` 的 `wallets[]`,`transfer_allowlist` 只列本组织的归集地址与交易所充值地址;发布配置变更(chain 组激活)。
2. **上币授权**:每个新路由上线前,由运维以该钱包经 chain-core 发授权交易(ERC-20 `approve` 或 Permit2 `approve(token, spender, amount, expiration)`,金额与到期按上币清单);授权不在平仓路径上临时进行。授权完成后再 `ReplaceRoutes` 上线该路由。
3. **补 gas**:BNB < 0.0015 时 reconciler 给 `Warn`;从归集地址向执行钱包转 BNB(外部转入不触发 `Tainted`),建议补到 0.02 BNB。
4. **资金调拨**:从执行钱包转出只能用 chain-core 的 `submit_transfer`(目的地址必须在白名单);转入由人工从归集地址或交易所提现完成。每次调拨写 ops-log 并用 `RecordTransfer` 登记。
5. **`Tainted` 处理**(P0):先 `/kill chain`;在区块浏览器与本机节点上列出该钱包自最近一次正常状态以来的全部交易与 nonce;若存在不是 chain-core 发出的交易 → 视为私钥泄露,按 [incident-response K](incident-response.md#k-秘密泄露或可疑访问) 处理,余额转移到新钱包(新钱包按第 1 步启用),旧钱包永久退役;若只是读数错误或外部转入被误判 → 修正后 `[需授权]` ctl `ClearTaint{wallet}`(确认),写 ops-log。
6. **旧钱包退役**(cdex 迁移时,见 [migration](migration.md)):cdex 的 BSCTrader 发送路径停用后,旧钱包余额经人工转入新执行钱包;旧钱包私钥归档后不再使用。

## 验收

- A.5 全部通过;连续 7 天本地头落后 ≤ 2 块的时间占比 ≥ 99.9%。
- 每次升级:金样全部一致、切换期间 chain-core 无 `Unknown` 超时。
- 执行钱包:7 天零次非 chain-core 发出的交易;每次 `Tainted` 都有 ops-log 结论。

## 回退

见 C.5;钱包相关操作不可回退(链上交易),只能以新交易修正,务必先在测试账户钱包上演练。

## 记录

`ops-log/<date>-bsc-node-<操作>.md`:节点版本、快照来源与校验和、追平耗时、金样结果、硬分叉登记;钱包操作记录交易哈希与授权人。
