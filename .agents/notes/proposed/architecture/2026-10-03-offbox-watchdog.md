# Agent Note: 机外看门狗与交易主机失联的人工兜底

Status: proposed

> **第八轮第五次修订(2026-10-03)**:收窄——`/healthz` 返回 `maintenance_until`(升级主机上 `hft-maint start <分钟>`,≤ 30 min),静默期间看门狗只发 Telegram;看门狗目标是 dev2 上的主机本地文件,换机重建时由人修改(incident-response S 节);主机清单的 MAC 不进发布哈希、预建第二 AZ 待命清单;az-probe 证实升级主机与 dev2 同 AZ 时另开第三 AZ 的 `t4g.nano` 看门狗。规格见 [deploy 机外看门狗](../../../../deploy/README.md#机外看门狗);依据 [round8e 简报](../../../../design/_work/round8e-spec-brief.md) E14(f)(g)。

## Problem

watchtower 的 `/metrics` 只绑 127.0.0.1,规格里说的"研究主机上的 Prometheus up==0"既抓不到回环地址,研究主机也不在预算里;电话告警通道未选型;kill-switch 与 hft-core 同机。升级主机宕机或 AZ 故障时,watchtower、ops_bot、kill-switch 同时消失:没有任何告警,老板的 `/stop` 无响应,行情朝持仓不利方向走时无人减仓;也没有"整机失联"的处置手册与网页操作授权([V4 评审](../../../../design/_work/review-2026-10-03-M0c-V4运行与交付.md) P1-8、§4-7/8)。

## Proposal

用户决定按推荐执行,两名网页兜底人的名字由负责人指定([round8-decisions](../../../../design/_work/round8-decisions.md) R8-10;[round8d 简报](../../../../design/_work/round8d-spec-brief.md) D8)。单元归 [deploy 机外看门狗](../../../../deploy/README.md#机外看门狗),`/healthz` 归 [watchtower](../../../../apps/watchtower/README.md),处置归 [incident-response S](../../../../docs/runbooks/incident-response.md#s-交易主机失联)。

1. `hft-offbox-watchdog.service` 装在 dev2(与升级主机不同 AZ):只用 Python 标准库、`system.slice`、独立 Telegram token + 电话类告警(供应商待选型);每 15 s 请求升级主机 ENI-A 私有 IP 上的 `/healthz`,连续 60 s 不可达或不健康 → 告警。
2. watchtower 新增 `healthz` 监听(绑 ENI-A 私有 IP,安全组只放行 dev2):hft-core 心跳年龄与状态、kill-switch 存活、最近一次对账时刻;与只绑回环的 `/metrics` 分开。
3. incident-response 新增"交易主机失联":授权表(两名网页兜底人,持有各所网页权限与划转二次验证)、各所网页撤单/减仓步骤、从 AMI + EBS/S3 重建并以 `--recover-from-snapshot` 恢复的顺序;SSM agent 保留为破窗通道。
4. 看门狗与电话告警演练通过是正式实盘的前置(⑪)。

## Alternatives considered

**在另一台主机部署第二个 kill-switch(经网络读心跳)。** 能在整机失联时撤单,但要把撤单 key 放到第二台主机、扩大 key 的暴露面;kill-switch README 把它保留为可选扩展,触发条件是无死手 venue 的挂单成为主要风险。

**CloudWatch 告警(实例状态检查)。** 需要给角色加 CloudWatch 权限并依赖 AWS 侧告警投递;它看不到 hft-core 是否在推进心跳。

**只靠交易所死手。** Binance 现货、Bitget 没有死手;死手只撤单,不处理持仓。

## Acceptance criteria

- 演练:停 watchtower 或从安全组移除 dev2 的放行 → 60 s 内 Telegram 与电话告警到达值班手机;恢复后收到恢复通知。
- 授权表两人的名字已填写,两人各自完成一次测试网/模拟盘上的网页撤单演练。
- 一次重建演练(新实例 + 摘挂 ENI 与冷盘卷 + `--recover-from-snapshot`)的耗时写进 ops-log。

## Risks

- 只有一个观察点:dev2 上的 watchtower 能报看门狗进程退出,但 dev2 整机宕机时看门狗与它一起消失,升级主机上没有反向监控。
- dev2 与升级主机的私网可达性(同一 VPC 或对等连接)待核实;不可达时需要改为经公网 + 认证,攻击面变大。
- 网页兜底依赖两个人的可用性与手机二次验证;夜间响应时间无法保证,只能把它写进授权表的值班约定。
