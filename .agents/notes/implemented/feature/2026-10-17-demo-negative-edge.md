# Agent Note: demo 负净边的真实双腿链路测量

Status: implemented

## Problem

负责人于 2026-10-17 要求用真实 cross-taker 测量 DEMO 的 TD 链路。给定 ETH 扣费后可执行价差约 −14 至 −16 bp,生产正 min_edge 契约阻止开火。该数值来自负责人观测,此改动不声称完成交易所实测。

## Decision

负开仓和平仓阈值仅绑定账户 is_prod=false 可用。生产启用的 min_edge 继续严格为正,close_edge 也禁止负数;收窄[产品形态决定](2026-10-16-perp-spot-shape.md)中人工负退出阈值的范围。默认与组覆盖、禁用与空组都受检查。具体值与试验频率归 [deploy 参数](../../../../deploy/testnet/README.md)。

SDK 提供通用 prod_min 元数据,进入 schema 摘要,hft-cfg 不依赖策略名称或代码。策略再次校验账户事实。参数 schema v3 是边界修订,允许 version 高于最大字段 since,无需虚构新的可调字段;旧 blob 须用同一发布 schema 重建。真实双腿 IOC、费用、风控和执行修复不变。

## Alternatives considered

放宽生产阈值会扩大正式资金行为,不采纳。独立探针策略不能测量要求的真实策略路径,不采纳。把策略名硬编码进 hft-cfg 破坏 schema 解耦,不采纳。

## Consequences

允许 demo 上有意接受负收益,不构成收益结论或正式上线许可。生产负 close_edge 配置必须修正。生产与非生产参数入口、同事件双腿、冷却、回放/DST 和实际 --check 均由测试覆盖;主机端运行与 TD 实测在发布安装后进行。语义登记为待评审。
