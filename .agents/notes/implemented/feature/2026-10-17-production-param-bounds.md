# Agent Note: 账户相关参数下界

Status: implemented

## Problem

离线工具需要按账户事实校验参数边界,但不能依赖具体策略代码或字段名。

## Decision

SDK 字段描述增加可选 prod_min,进入规范 schema 和摘要。hft-cfg 与生成的 validate 对正式账户检查默认值和覆盖;没有该元数据的既有策略行为不变。边界修订允许 schema version 大于最大字段 since,无需追加无意义的参数字段。

## Alternatives considered

工具按策略名称分支会破坏解耦。只在策略检查会让离线编码接受已知不合规的参数。只在工具检查不能保护运行时 SetParams。

## Consequences

策略显式采用元数据后才增加账户相关限制;策略语义变更另行登记。通用测试使用合成 schema,核心装配测试枚举已提供的 testnet 参数并执行实际 --check。负责人要求先合并此独立基础能力,再提交策略使用方。
