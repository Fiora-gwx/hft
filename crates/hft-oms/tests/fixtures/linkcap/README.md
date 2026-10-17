# probe-spin-01 链接耗尽回放片段

来源为 2026-10-17 c4 的公开回放导出 `probe-spin-01-replay-public.tgz`,原制品提交 `fd2dc3bae9174fe182a318bf9937d9a564b0b04f`。本目录只保留非秘密的 refdata/engine blob 和原样 OMS 快照节,不含凭据或登录帧。

原 seq journal SHA-256 为 `edda6eaeab61ab92c277f0927ffe65c0980c32c430d2404e40b986dc46a53aa6`。31 条获准 First 之后 seq 4262 首次写 `reject = CAPACITY(0x40000000)`。seq 7154(TSC `19093497467020`)的快照保存 16 条 Hedging:第 1 条 owed 1000,其余各 990,全部 hedge_open 0。Kill 后 seq 6100 的清仓首腿也被 CAPACITY 拒绝。

| 文件 | 提取规则 | SHA-256 |
|---|---|---|
| `oms-7154.bin` | seq 7154 的 StateSnapshot 去掉 96 B SnapshotHdr,按原顺序仅拼接节 2、3、12 的 SectionHdr 与体,不改字段 | `c492671c54e8cfc36c3143f4798c37c800831213ce0c4513797b331c24486d91` |
| `refdata.bin` | 导出 configuration/refdata.bin 原样 | `9006ed9b02c078cb6d1f999df127deea0f238835c8a11a3f238a6ee2730b4902` |
| `engine.bin` | 导出 configuration/engine.bin 原样 | `09e5cad061aab00532e7d2513d740b58ab04a6d00235edebf2b0419d48cd26ac` |

`oms_201_recorded_probe_snapshot_reclaims_all_sixteen_orphans` 用生产 RefData/EngineConfig 解码和 Oms::restore 恢复,重放 Sweep,断言原 16 条链接全部完成,净仓不变、group 对冲腿 residual 增加 15850。同一输入重复执行的快照逐字节相同。原始旧版本 journal 的决定流包含缺陷,本测试不要求修复版本继续产生旧 CAPACITY 决定。
