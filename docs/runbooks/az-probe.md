---
description: "M1.5 延迟探针之前的 AZ/spine 探测:在 az1/az2/az4 × 每 AZ 3–5 个 CPG 的 Spot 探针上测全部交易所端点的握手 minRTT、WS 应用往返与首帧到达分布,选出 (AZ, CPG) 与第二 AZ(待命),判定是否需要第三 AZ 看门狗,之后每周复测"
scope: core
---

# AZ / spine 探测

## 适用范围

- **M1.5 延迟探针之前**做第一次:M1.5 探针、M3 末满订阅探针与参考机的租用实例都开在本页选出的 AZ;同一结论决定 M7 统一升级主机(默认 `c32`;可选扩展的 tokyo-chain 同 CPG)落在哪个 AZ 与哪个 CPG([host-upgrade](host-upgrade.md)、[02 §2.1](../../design/02-硬件与托管方案.md)、[11 §1](../../design/11-实施路线图.md)),验证假设 A1(Binance 在 apne1-az4,同 AZ 可省 0.5–1 ms,[12 §2](../../design/12-风险与未决问题.md))。
- 同一报告还给出:**第二 AZ**(待命清单与预建 ENI 所在,[deploy 主机清单](../../deploy/README.md#主机清单))与**是否需要第三 AZ 看门狗**(升级主机所在 AZ 与 dev2 相同时,[deploy 机外看门狗](../../deploy/README.md#机外看门狗))。
- 之后**每周**复测(交易所端点与我们的实例都会随扩缩容移动);交易所公告迁移时(如 Bybit 11-12 迁 apne1-az4)加测。
- 首次探测用的脚本是 `architecture-v2/research/_src/` 下的三个无依赖 Python 脚本;`tools/az-probe` 是它们的产品化版本(同样的测量,加上 PHC 首帧分布与报告生成),就绪后替换。

## 前置条件

1. AWS 账户的 AZ 名 → AZ-ID 映射(AZ 名每账户随机,只有 AZ-ID 跨账户一致):
   `aws ec2 describe-availability-zones --region ap-northeast-1 --query 'AvailabilityZones[].[ZoneName,ZoneId]' --output text`
2. 探针实例规格:`c7i.large`(支持 PTPG/PHC,可测首帧单向分布)或 `c8g.large`;Spot;预算 < $50([02 §7](../../design/02-硬件与托管方案.md))。
3. 脚本:probe.py(未入库的资料缓存:`architecture-v2/research/_src/probe.py`)(端点握手 RTT + IP 归属分类)、probe2.py(未入库的资料缓存:`architecture-v2/research/_src/probe2.py`)(每个主机名**全部 A 记录**逐个测 TCP 握手,9 次取最小,找出各 AZ 的 LB 节点)、ws_rtt.py(未入库的资料缓存:`architecture-v2/research/_src/ws_rtt.py`)(TCP/TLS/WS 升级耗时与应用层请求往返,N = 25)。基线结果:probe2_result.txt(未入库的资料缓存:`architecture-v2/research/_src/probe2_result.txt`)(从 apne1-az2 测得 Binance 全部 LB 节点 1.29–1.83 ms,Gate 有 0.38/0.42 ms 同 AZ 节点)。
4. 交易所端点清单:probe2.py 的 `HOSTS`(Binance fstream/ws-fapi/fapi/api1–4/stream/ws-api/stream-sbe/fix-oe、Gate、Coinbase INTX);补齐 OKX、Bybit、Bitget、KuCoin、Hyperliquid 的主机名(probe.py 已列出大部分),Binance 合约另测 `/public`、`/market`、`/private` 三条路由所在主机([04 §5](../../design/04-行情接入层.md))。

## 步骤

### A. 布置探针 `[需授权]`(创建 AWS 资源)

1. 对 apne1-az1、apne1-az2、apne1-az4 各创建 3–5 个 Cluster Placement Group(不同 CPG 落在不同 spine):
   `aws ec2 create-placement-group --group-name probe-<az>-<n> --strategy cluster`
2. 每个 CPG 启动 1 台 Spot 探针(同一 AMI,Ubuntu + chrony + python3);若探针规格支持,同时加入该 AZ 的 PTPG 以获得 PHC(可选,只影响步骤 C 的单向分布)。
3. 每台记录:实例 id、AZ-ID、CPG 名、私网 IP、启动时间;`chronyc tracking` 确认时钟同步。

### B. 握手与往返(每台,≥ 1 小时,覆盖一个活跃时段)

4. 每 10 分钟跑一次并追加输出(带 UTC 时间戳与实例标识):
   - `python3 probe2.py >> probe2-<az>-<n>.txt`(全部 A 记录的握手 minRTT);
   - `python3 probe.py >> probe-<az>-<n>.txt`(端点 RTT + IP 属于 AWS 哪个服务/区域或 Cloudflare);
   - `python3 ws_rtt.py >> wsrtt-<az>-<n>.txt`(WS 握手分段与应用往返 min/p50/p90/max)。
5. 对已建立的长连接取内核视角:`ss -ti state established '( dport = :443 )'` 中每条连接的 `minrtt`(ICMP 常被限速,不用 ping)。

### C. 首帧到达分布(若探针有 PHC)

6. 订阅 Binance USD-M `bookTicker`(`/public`)、`aggTrade`(`/market`)与 OKX、Bitget 的 BBO 频道(频道名与交易所时间戳字段以 [venue-okx](../../crates/venue-okx/README.md)、[venue-bitget](../../crates/venue-bitget/README.md) 为准),记录每帧 `HW RX 时间戳 − 交易所事件时间`(Binance `E`/`T`)1 小时;无 PHC 的探针只记录软件接收时间并标注"时钟误差 ±1 ms 级,仅作形态参考"([research/05 §4.4](../../design/research/05-hardware-hosting-os-tuning.md))。

### D. 判定

7. 汇总表(每个 (AZ, CPG) 一行,每个交易所主机名一列):最小 RTT、各 LB 节点的 RTT 分布、ws_rtt 应用往返 p50/p90、首帧分布 p50/p99。
8. 判据(按优先级):
   1. **Binance**(fstream、ws-fapi、fapi、api1–4)出现 ~0.3 ms 级节点的 AZ 即 Binance 所在 AZ;预期 az4,若为 az1 同样迁([research/02](../../design/research/02-exchange-connectivity.md))。
   2. 同 AZ 内选 CPG:首批 core venue(Binance、OKX、Bitget)握手 minRTT 之和最小、ws_rtt p90 最小的 CPG;差异 < 20 µs 视为相同,再看 optional 的 Gate/Bybit/KuCoin。
   3. 记录每个交易所各自的最优 AZ;若最优 AZ 分裂(例如某所在另一 AZ 且差 > 0.5 ms),写入报告,作为可选扩展"按所分实例"的评估输入(触发条件:某所分裂差 > 0.5 ms 且该所是主要交易腿)。
   4. **第二 AZ**:胜出 AZ 之外、首批 core venue 握手 minRTT 之和最小的 AZ;它不能与胜出 AZ 相同。
   5. **第三 AZ 看门狗**:dev2 的 AZ-ID(只读查 IMDS `placement/availability-zone-id`)与胜出 AZ 相同 → 报告写"需要第三 AZ 看门狗",由负责人决定开 `t4g.nano`(`[需授权]`)。
9. 输出报告 `ops-log/<date>-az-probe.md`:原始文件、汇总表、胜出 (AZ-ID, CPG)、第二 AZ、dev2 所在 AZ 与看门狗结论、与 az2 现状的差值(预期 Binance 降 0.5–1 ms,[02 §6](../../design/02-硬件与托管方案.md))。A–D 与第 12 步的核对:DRL-69(工具报告的覆盖:TOOL-098)。

### E. 落地与清理

10. `[需授权]` 在胜出 CPG 中(或在该 AZ 新建 CPG 后重测确认)启动生产实例(M7 时按 [host-upgrade](host-upgrade.md) 选定的主机类:默认 `c32`(`c8g.8xlarge`),`c16`(`c8g.4xlarge`)只在 M3 末满订阅探针达标时;可选 `metal` 为 `c7i.metal-24xl`),并建立 On-Demand Capacity Reservation 锁定容量;可选扩展的 tokyo-chain(i7ie/i4i)同 CPG;关联 PTPG(若该实例支持)。
11. 把胜出的 AZ-ID 与 CPG 写进 `deploy/hosts/<清单名>.toml`(升级主机的 M7 与 M8 两份清单都改),第二 AZ 写进各自的待命清单 `deploy/hosts/<清单名>.standby.toml`(PR);`[需授权]` 在第二 AZ 预建待命的 ENI-B/ENI-C。
12. `[需授权]` 终止探针实例、删除未使用的 CPG;保留 1 台最小探针用于每周复测(或按周临时启动)。
13. 胜出 AZ 只决定**新框架的统一升级主机**开在哪里;现有系统(collector + cdex)不搬、不改([11](../../design/11-实施路线图.md))。新主机开机后测一次 `minrtt`,与 az2 现状对比。落地与回退的核对:DRL-70。

## 验收

- 覆盖三个 AZ、每 AZ ≥ 3 个 CPG、每台 ≥ 1 小时且包含一个活跃时段。
- Binance 所在 AZ 有明确结论(存在 ~0.3 ms 级节点,或三 AZ 均无并有解释)。
- 新主机到 Binance 的 `minrtt` 比 az2 现状降 ≥ 0.5 ms,p99 RTT 抖动 < 100 µs([02 §6](../../design/02-硬件与托管方案.md))。

## 回退

探测不改变生产。若新 AZ 的主机指标未达预期,新系统改在原 AZ 开机即可;现有系统不受影响。

## 每周复测

1. 周一 UTC 00:00–02:00 以外的活跃时段,在生产 CPG 的探针与生产机本身各跑一次步骤 4–5(不在 dev2 上跑:dev2 不在生产 AZ,结果不代表生产主机)(生产机只跑 probe2.py 与 `ss -ti`,不跑 ws_rtt 的新连接风暴;注意交易所的每 IP 连接数限制)。
2. 与上周比较:任一交易所最小 RTT 变化 > 0.2 ms → P2 并在周报中分析;Binance 同 AZ 节点消失 → P1,启动 A–D 的完整探测。
3. 结果追加到 `ops-log/az-probe-weekly.md`。核对:DRL-71(生产机上的资源占用:TOOL-099)。

## 记录

首次探测的报告写 `ops-log/<date>-az-probe.md`(第 9 步),每周复测追加到 `ops-log/az-probe-weekly.md`(每周复测第 3 步);每步的起止 UTC 时间与耗时拆分按[写法规则](README.md#写法规则)。
