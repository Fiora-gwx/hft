# latency-report 验证

扩展既有验收项，不分配新的冻结用例编号。

| 用例 | 行为 |
|---|---|
| RT-002 | 环满不阻塞，保留 FIFO、serial 缺口与最终 dropped |
| RT-166 | 启用记录后的循环仍为零分配 |
| NET-125 | 一个 read 内多个帧关联 ID 各异，保留共同 RX 戳与解密戳 |
| NET-152 | 发送短写及后续 flush 保留同一密文区间，不依赖残留线程上下文 |
| VEN-199 | SendReport 不因测量而重复或改变，订单发送语义保持 |
| VEN-068 | raw 序号与 frame 关联在真实 journal 读回后吻合，不以同读时间猜匹配 |
| SEQ-053/054 | 测量不进入状态/输出哈希，不改变回放比较 |
| CORE-012 | 非法容量在 --check 拒绝；默认关闭 |
| CORE-220 | 干净停止后测量文件非空、记录对齐、serial 递增、completion 已关闭 |
| RPL-203 | 逐事件 CSV 与分位数一致；MD-only、多 fill、丢失、歧义和负间隔显式报告；同 slot/gen 的不同 venue/account 回报不得串单，缺身份不关联；SW 前后校准插值、无 PHC 时的 REALTIME 速率误差、缺锚点/Stepped/超界过滤 |
