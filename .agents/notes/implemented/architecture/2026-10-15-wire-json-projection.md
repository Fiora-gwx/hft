# Agent Note: Wire JSON 字段投影

Status: implemented

## Problem

BBO 的字段很少，通用反序列化与重复扫描占据可观的 ingest CPU 时间。必须保留未使用字段的语法检查、使用字段的严格转换及全部既有输出语义。

## Decision

在 hft-venue-proto 提供借用切片投影，以 memchr 查找字符串边界。Binance 常见 BBO 字段顺序直接匹配；Bitget 一次遍历选取 arg 与首个 data。peek 与私有消息分派只提取判别字段。未知值扫描语法边界，不建立 token 表、不转换数字。转义、非 ASCII、指数、深层对象及重复字段回退 Sonic；回退前检查 UTF-8，防止原有 LazyValue visitor 对非法字节 panic。

## Alternatives considered

全量字段投影仍需对每个键分派，在本机试验中比原解析更慢。只查找字段子串会接受字符串内伪字段或非法结构，不能保持拒绝语义。完全替换 JSON 语法实现会扩大审计范围；保守快路径保留严格参考解码器。

## Consequences

无资金或风控语义变更。新增 memchr 直接依赖，锁文件版本复用已有依赖。差分变异测试比较输出与错误，预热后零分配测试覆盖真实入口。性能证据见 [测量报告](../../../../benchmarks/reports/2026-10-15-wire-json.md)；共享 Windows 的尾延迟不作参考机验收，aarch64 验收仍需独立运行。
