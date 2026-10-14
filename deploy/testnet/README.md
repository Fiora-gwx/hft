# 双交易所 demo 参数

使用 [hft-core 双 venue 模板](../../apps/hft-core/config/dual-testnet.toml)，Binance USDM 与 Bitget UTA USDT Futures 的 BTCUSDT 分别登记在 refdata。该示例只绑定非正式账户 `demo` 与实例 `demo-spread`；实例 kind_id 为 3。构建使用 registry 的全部 features，先检查 `hft-core --version` 和导出的 schema。

[参数示例](strategy.toml) 显式启用 cross-taker，单腿每笔上限 200 USDT、持仓上限 400 USDT；RiskParams 应独立约束相同或更小的限额。实际 tick、lot 与最低名义须从 demo exchange info 确认；若最小可交易订单超过 cap，应保持禁用并重新评估配置。该值只是可审阅的 demo 配置，不是收益假设。

以 CI 制品运行 `hft-cfg build --kind strategy --schemas schemas --refdata refdata.bin --in strategy.toml --out params.spb1`。将输出文件直接用于 hft-core 的 blobs.strategy_params，并填写其 SHA-256。启动默认 Shadow，启用参数不绕过模式或账户闸门。资金费策略使用集成分支 registry 已登记的实现和导出 schema；不要在 app 中补算法或硬编码其 kind。
