# Agent Note: 行情采集与交易数据分离,collector 保持独立

Status: proposed

> **第八轮(2026-10-02)**:验收中的 BBO 一致性对照改为首批 core venue(Binance、OKX、Bitget);见 [ccex-first-scope](2026-10-02-ccex-first-scope.md)。

## Problem

此前的设计(01 §8、04 §1/§8、08 §9)让新框架"吸收" collector:venue 线程的 raw + seq journal 取代独立采集进程,`journal-archiver` 接替 processor/uploader 并覆盖 collector 的全部数据集,collector 在切换后下线。用户决定(第四轮裁决 R4-1)行情采集**不迁入**新框架:现有 collector(`Trading` 仓库,v0.3.7,collection-policy 控制、150+ 合约 × 多所、`book_ticker.v2` 等 Parquet 与 S3 管线)服务研究,工作正常;把研究订阅搬进交易进程会增加热线程负载(扩大订阅 = 更多解码、更多 journal 写),而新框架首先要在 2 核主机上运行。

## Proposal

1. collector 保持独立,服务研究;新框架不吸收它,不复刻它的数据集与 schema。
2. 新框架的 journal(raw + seq)只记录**它自己交易所需的订阅**,用途是回放、审计、复盘交易决策;订阅集合由交易需要决定,不为研究扩大。
3. `journal-archiver` 缩为"交易 journal 的归档与 Parquet 化",写独立 S3 前缀 `hft/<instance>/…`,**不要求与 collector 的 Parquet schema 兼容**;对照时按共同的时间戳语义(交易所事件时间 `ex_ts_ns`、接收 UTC)对齐([journal-archiver](../../../../apps/journal-archiver/README.md))。
4. 两套解码器并存;collector 的 raw WAL 作为新框架解码器金样的来源之一,另有离线的"同一时段两边 BBO 序列一致性"对照(`cargo xtask fixtures bbo-diff`,[fixtures](../../../../fixtures/README.md#与-collector-的离线-bbo-一致性对照))。
5. 2 核主机上不同时长期运行 collector 与新框架实盘;collector 用于研究窗口时按 collection-policy 单独排期。
6. 删改各处"collector 被吸收、不再有独立采集进程"的说法(01 §8、04 §1/§8、08 §9、11 的建仓步骤、hft 的 architecture、journal-archiver、fixtures、cutover、research)。

## Alternatives considered

**按原设计吸收 collector。** 新框架的订阅要覆盖研究需要的 150+ 合约 × 多所,热线程负载与 journal 体积成倍增加,2 核主机跑不下;且 collector 已稳定运行,替换它没有交易收益。

**新框架 archiver 输出与 collector 兼容的 schema。** 两边的事件模型不同(定点按资产指数、五时间戳、`seq`),强行兼容会把 collector 的历史包袱带进新 schema;研究需要的只是时间对齐。

**只保留一套解码器(collector 改用新框架的 venue crate)。** 需要改动稳定运行的 collector 并重新验证其全部数据集;收益是少维护一套解码器。等新框架的 venue crate 在生产中稳定后可再评估(新 Agent Note)。

## Acceptance criteria

- 规格与设计书中不再出现"collector 被吸收 / 下线"的说法;cutover runbook 不包含 collector 下线步骤。
- `journal-archiver` 的全部对象键在 `hft/<instance>/` 下,`s3.prefix` 与 collector 前缀重叠时加载失败。
- `fixtures bbo-diff` 对 Binance USDT-M 与 Gate 的同一 1 h 窗口:交集上 BBO 价量逐条相等(不等条目 = 0)。
- 新框架的订阅集合只由 refdata/`StrategyParams` 的交易需要决定(`dev2` ≤ 64 个合约)。

## Risks

- 两套解码器对同一报文的解释可能分歧(时间戳单位、序号规则);由 BBO 一致性对照与金样发现,分歧以交易所文档为准修正各自一方。
- 研究若需要新框架独有的字段(网卡 HW RX、`proc_tsc`),只在交易订阅范围内可得;更大范围需要单独决策是否扩大订阅(会增加热线程负载)。
- 两套 S3 数据的保留与成本分别核算;collector 的存储不再计入新框架的容量规划。
