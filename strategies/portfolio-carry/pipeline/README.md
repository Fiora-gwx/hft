---
description: "portfolio-carry 慢路径插件的研究边界;当前 NoModel 策略由人工 schema 参数与 GroupTable 配置,不加载插件"
kind: python-package
tier: cold
scope: core
status: spec
---

# portfolio-carry pipeline

## 概要

当前 [策略](../README.md) 使用 NoModel,不要求 Python 插件。此目录保留慢路径研究边界,没有可安装包或 entry point。人工 GroupTable 与参数经控制环记录后生效。

## 目录

- [使用本组件](#使用本组件)
- [实现说明](#实现说明)
- [延迟与资源契约](#延迟与资源契约)
- [测试与验收](#测试与验收)
- [依赖](#依赖)
- [延伸阅读](#延伸阅读)
- [已知限制与后续工作](#已知限制与后续工作)

## 使用本组件

无 compute/explain/warmup 实现、owned_fields 或预热载荷。策略参数与两腿 GroupSpec 定义见[策略规格](../README.md#参数表)。不输出旧研究方案的 liq_score/restricted/历史分位参数。

## 实现说明

若后续引入自动名单或历史模型,插件只经 [hft_pipeline.api](../../../python/pipeline/README.md) 注册,基于冻结输入快照计算;不得私自加载交易凭证或直接修改引擎状态。group 的保留、资金费统计与预热契约须与对应策略版本一起评审。

## 延迟与资源契约

无热路径约束,无运行时实例。

## 测试与验收

当前策略不依赖插件;执行链路验收见[策略 TESTS](../TESTS.md)。未来插件需要公共接口离线 fixtures、确定性和 schema 所有权测试。

## 依赖

未来插件只依赖 hft_pipeline.api;当前没有依赖或 Python 包。

## 延伸阅读

- [范围决定](../../../.agents/notes/implemented/feature/2026-10-14-bounded-funding-carry.md)

## 已知限制与后续工作

自动选币、流动性评分与历史预热均未实现,不作为当前策略启动前提。

### 开发备忘

自动名单与模型的字段所有权在实现插件前与策略 schema 一起评审。
