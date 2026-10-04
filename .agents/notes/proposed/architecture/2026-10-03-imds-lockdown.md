# Agent Note: 正式主机上锁定 IMDS,冷进程只用按前缀发放的短期凭证

Status: proposed

> **第八轮第五次修订(2026-10-03)**:事实同步——archiver 的消费者凭证覆盖 `hft/` 与 `hft-testnet/` 两个前缀,S3 对象键带主机段 `<prefix>/<host>/<instance>/…`;AMI 不装 `ec2-instance-connect`、装固定版本的 AWS CLI v2;DuckDB 与 polars 的 S3 凭证按凭证文件的 mtime 重建。规格见 [deploy IMDS 锁定](../../../../deploy/README.md#imds-锁定与凭证代理);依据 [round8e 简报](../../../../design/_work/round8e-spec-brief.md) §2 C-P0-8 与 W4 P2-4。

> **第八轮第六次修订(2026-10-03)**:收窄——锁定扩展到全部主机类:dev2 只对 hft 各 uid 加 nft 丢弃规则并运行凭证代理(不影响旧系统,见 [dev2-root-batch-slice](../process/2026-10-03-dev2-root-batch-slice.md));其他主机类改用挂在根 cgroup 的 BPF `connect4`/`connect6` 钩子代替 nftables 输出钩子(订单包不再经过任何规则,hostprobe 核对 `nf_conntrack` 未加载);消费者角色另含 SSE-KMS 授权,凭证代理有自己的告警 token。规格归 [deploy IMDS 锁定](../../../../deploy/README.md#imds-锁定与凭证代理)。

## Problem

升级主机的实例角色授权读取 `hft/live/*` 下的全部正式交易 key;ops_bot、pipeline、treasury 读 S3 也要用实例角色凭证。IMDS 对本机所有 uid 都可达,所以"ops_bot 不持 key"只在进程层面成立:面向 Telegram 的 Python 进程或其 PyPI 依赖一旦被利用,就能经 IMDS 取到角色凭证,读走全部正式交易 key 与 kill-switch key(后者事实上能下单)([V4 评审](../../../../design/_work/review-2026-10-03-M0c-V4运行与交付.md) P1-6)。

## Proposal

规范解法见 [round8d 简报](../../../../design/_work/round8d-spec-brief.md) O-P1-5;规格归 [deploy IMDS 锁定与凭证代理](../../../../deploy/README.md#imds-锁定与凭证代理)。

1. 只在 `formal_live = true` 的主机做;dev2 不做(实例角色与旧系统共用,锁定会影响旧系统)。
2. nftables 只放行 uid 0 访问 169.254.169.254;root 进程(`hft-secrets`、hostprobe、网卡脚本、SSM agent)照常使用实例角色。
3. root 的 `hft-cred-broker.service` 以实例角色对每个消费者 `AssumeRole`(带 session policy)出短期凭证,写到 `/run/hft/creds/<user>/credentials`(0400,45 min 刷新):archiver 可写 `hft/` 前缀,ops_bot/pipeline/treasury 只读各自前缀(pipeline 另可写参数版本库)。
4. preflight 检查"非 root uid 访问 IMDS 被拒"与凭证文件的属主、权限与有效期。可 `AssumeRole` 的消费者角色是 `[需授权]` 的 IAM 变更。

## Alternatives considered

**iptables owner 规则放行 root 与归档进程(评审的写法)。** 归档进程直接用实例角色仍能读 `hft/live/*`;改为凭证代理后,任何非 root 进程都拿不到能读秘密的凭证。

**IMDSv2 的跳数限制(hop limit = 1)。** 只能挡住容器内的访问,挡不住本机任意 uid。

**每个冷进程一把长期 IAM 用户密钥。** 长期密钥要存放、轮换,泄露面比 45 min 的会话凭证大。

**把 Python 冷进程搬到另一台主机。** 多一台主机的费用与运维;ops_bot 的紧急停止兜底依赖本机的 kill-switch 触发套接字。

## Acceptance criteria

- 正式主机上以 `hft-pipeline`、`hft-opsbot`、`hft-treasury`、`hft-archive` 任一用户访问 IMDS 均超时;root 访问正常。
- 用某个消费者的凭证读 `hft/live/*` 秘密或写别的 S3 前缀 → AccessDenied。
- 停掉凭证代理 1 h:冷进程在凭证到期前收到 P1、到期后 P0;恢复后不重启冷进程即可继续访问 S3。

## Risks

- 凭证代理成为冷进程访问 S3 的单点:失败时 archiver 停止上传、ops_bot 历史查询失败;不影响 hft-core 与 kill-switch。
- 各 SDK 对凭证文件的到期重读方式待核实;不支持时需要用 `credential_process` 或让进程定期重建客户端。
- root 进程仍可访问 IMDS;主机 root 被拿下时本方案不提供保护。
