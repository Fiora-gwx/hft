# 2026-10-01 东京服务器实测原始数据

主机:`54.92.49.180`(c7g.large,apne1-az2)。全部为只读、低负载探测(`nice` 低优先级,无鉴权调用,不改动任何生产服务)。分析与结论见 [../../现状测量与升级分析-2026-10-01.md](../../现状测量与升级分析-2026-10-01.md)。

| 文件 | 内容 | 复现 |
|---|---|---|
| `net_probe.py` / `net_probe_20261001T0231.json` | 各所端点 DNS、TCP 建连 RTT(15 次)、TLS 握手(3 次) | 服务器上 `python3 net_probe.py out.json` |
| `md_probe.py` / `md_probe_20261001T0237.json` | 各所最快公开 BBO 频道 120 s:消息率、本地接收时间 − 交易所时间戳 | `<cdex venv>/python md_probe.py 120` |
| `leadlag.py` / `leadlag.json` | 2026-09-07 16–20 UTC 历史 book_ticker:跨所中间价同向跟随的命中率与延迟 | 先把 S3 `book_ticker/.../date=2026-09-07/hour=16..19` 拷到 `bt/<venue>/<SYM>/`,再运行 |

| `spin_probe.py` / `spin_before.txt` / `spin_after.txt` | 模拟忙等热线程(nice 19、绑核、20 s)被打断的频率与时长;停测试服务前后各一次 | `nice -n 19 python3 spin_probe.py 20 <cpu>` |

逐 IP 的 RTT 聚类、S3 manifest 数据量统计、主机与服务快照是一次性命令,结果直接记录在分析文档 §2–§4。
