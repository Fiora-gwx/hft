# Agent Note: 冷账本的槽索引包含账户与 venue

Status: implemented

## Problem

OMS 的槽号属于各自的账户与 venue 表。JournalLedger 的规格和实现却只按 epoch、slot、gen 建索引。两个表的同号槽会互相覆盖,随后回报可能把成交和费用记入另一账户或另一条腿。

## Decision

槽身份采用 venue、account、epoch、slot、gen 五项。登记、查找、淘汰和从快照重建都使用同一个完整键。累计成交量和费用水位属于这个键对应的订单。

## Alternatives considered

只在查找后核对账户无法保留已经被覆盖的映射。把所有表的槽号改成全局编号会改变 OMS 与交易所 client id 契约,无需为冷读者的索引缺陷扩大协议改动。

## Consequences

索引只存在于冷读者内存,不改变 journal 或快照格式。LED-273 通过成功下单记录以及 OMS v1、v2 快照建立同号槽,检查跨 venue、跨账户的持仓、余额与迟到费用去重。
