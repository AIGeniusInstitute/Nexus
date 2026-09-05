# Nexus 自动化运维与 K8s 弹性部署方案

> 产物编号：任务二-8 · DevOps 与 K8s 弹性部署
> 基座：`~/Nexus`（基于 OpenAI Codex Harness，codex-rs 106 crate Rust 工作区）
> 日期：2026-09-06 · 配套图：`k8s-topology.svg` / `.png` · 交互报告：`devops-k8s-report.html`

---

## 0. 核心判断（结论先行）

> **一句话**：控制平面跑在 `nexus-control` 命名空间（长期有状态、多租户、强一致），执行平面跑在 `nexus-exec-{tenant}` 命名空间（一次性、单租户单任务、无状态、可销毁），两者仅经 app-server JSON-RPC 与对象存储通信。弹性靠 HPA + warm pool + 队列权重，安全靠 NetworkPolicy + Seccomp + 按租户 CMK，运维靠 GitOps + 沙箱自检 + OTel 全链路。

**五个决定成败的运维判断**：

| # | 判断 | 理由 |
|---|---|---|
| 1 | 控制面与执行面必须分命名空间部署 | 沙箱跑不受信代码（依赖安装、用户脚本、MCP stdio）；混在一起等于把整个平台暴露；分 ns 才能做 NetworkPolicy 隔离 |
| 2 | 沙箱 Pod 池必须 warm pool 预热 | 冷启动目标 < 5s 靠 PVC snapshot + 预热空闲 Pod；不预热则首次任务体验极差且容器运行时拉取镜像可能 > 30s |
| 3 | 沙箱启动自检不过禁止调度生产任务 | Linux 容器可能因宿主不支持 Landlock/seccomp 致 Codex OS 沙箱失效（路线图 §3.4.2）；自检 = 准入门控 |
| 4 | 出站 NetworkPolicy 默认全禁，仅放行两个白名单 | 执行面 Pod 的出站只能到 Model Gateway 和 MCP Gateway；其余全 deny，防数据外泄、C2 回连、依赖投毒外联 |
| 5 | 配置即政策：运行时注入 config.toml + execpolicy，任务结束即焚 | 沙箱内零长期密钥（路线图 §4.5）；配置不落盘到镜像，随 Pod 生命周期存在 |

---

## 1. K8s 部署拓扑

### 1.1 命名空间规划

| 命名空间 | 角色 | 生命周期 | 安全级别 |
|---|---|---|---|
| `nexus-control` | 控制平面（API/WS 网关、Temporal、审批/策略/计费/连接器/知识库、Postgres/Redis/MinIO） | 长期驻留 | restricted PSA |
| `nexus-exec-shared` | 共享池执行面（中小客户） | 分钟-小时级 | restricted PSA + NetworkPolicy |
| `nexus-exec-{tenant}` | 专属池执行面（大客户独立 ns） | 分钟-小时级 | restricted PSA + 独立密钥 |
| `nexus-infra` | 基础设施（Ingress、Cert-Manager、ExternalDNS、ArgoCD） | 长期驻留 | privileged（仅管理员） |
| `nexus-monitor` | 可观测（OTel Collector、Prometheus、Loki、Grafana） | 长期驻留 | restricted PSA |

### 1.2 控制面命名空间 `nexus-control`

```
nexus-control
├── Deployment: nexus-api-gateway        (HPA 3-20, CPU 70% / QPS)
├── Deployment: nexus-temporal-worker    (HPA 2-10, Workflow 队列深度)
├── Deployment: nexus-approval-center    (HPA 2-8)
├── Deployment: nexus-policy-center      (HPA 2-6)
├── Deployment: nexus-quota-billing      (HPA 2-6)
├── Deployment: nexus-connector-gov      (HPA 2-8)
├── Deployment: nexus-kb-rag             (HPA 2-10, QPS / 向量查询延迟)
├── StatefulSet: postgres-primary        (1 replica + 2 read replicas, 流复制)
├── StatefulSet: redis-bus               (3 节点 + Sentinel)
├── StatefulSet: minio                   (4 节点分布式, 跨区复制)
├── Deployment: otel-collector           (OTLP → ClickHouse + Prometheus + Loki)
└── CronJob: pg-backup / minio-replicate / velero-snapshot
```

**控制面 Deployment 关键配置**：

| 参数 | 值 | 理由 |
|---|---|---|
| `replicas` | 3（最小） | 控制面不可单点；3 副本跨 AZ |
| `strategy` | RollingUpdate（maxSurge=1, maxUnavailable=0） | 零中断发布 |
| `securityContext.runAsNonRoot` | true | 非 root 运行 |
| `securityContext.seccompProfile` | RuntimeDefault | 默认 seccomp |
| `topologySpreadConstraints` | maxSkew=1, across AZ | 跨 AZ 均匀分布 |
| `podAntiAffinity` | 同 Deployment 不共节点 | 单节点故障不全灭 |
| `readinessProbe` | /ready（依赖 DB 连通性） | 依赖就绪才接流量 |
| `livenessProbe` | /health | 进程存活 |
| `terminationGracePeriodSeconds` | 120 | 优雅停机（刷新事件、上传 rollout） |
| `revisionHistoryLimit` | 10 | 回滚深度 |

### 1.3 执行面命名空间 `nexus-exec-{tenant}`

```
nexus-exec-{tenant}
├── Deployment: sandbox-pool-controller    (warm pool 管理器, 1 replica)
├── Pod(s): sandbox-pod-xxxx             (warm pool 预热 N 个空闲 Pod)
│   ├── Container: codex-app-server       (config.toml + execpolicy 运行时注入)
│   ├── Container: mcp-gateway-sidecar    (凭据注入 + 工具白名单 + 出站代理)
│   ├── Volume: workspace-pvc             (git worktree / PVC snapshot)
│   ├── Volume: config-secret             (config.toml + execpolicy, 任务结束即焚)
│   └── Volume: tmp                       (emptyDir, 可写 scratch)
├── NetworkPolicy: sandbox-default-deny   (默认全禁出站)
├── NetworkPolicy: sandbox-egress-allow    (仅 Model Gateway + MCP Gateway)
└── ResourceQuota: tenant-quota           (CPU/Mem/Pod 上限按租户档位)
```

**Sandbox Pod 结构（三容器 + 三卷）**：

| 组件 | 类型 | 职责 | 关键约束 |
|---|---|---|---|
| codex app-server | 主容器 | Agent 执行内核，JSON-RPC 服务 | 只读 rootfs, runAsNonRoot, seccomp, AppArmor |
| MCP Gateway sidecar | sidecar | 凭据注入 + 工具白名单 + 出站代理 + 审计脱敏 | 独立 Secret 挂载，不共享主容器凭据域 |
| Workspace PVC | 卷 | git worktree 或 PVC snapshot | ReadWriteOnce, 按租户 StorageClass |
| config-secret | 卷 | config.toml + execpolicy + AGENTS.md + enabled_tools | 投射卷（Projected Volume），任务结束即焚 |
| tmp emptyDir | 卷 | 可写 scratch 分区 | sizeLimit, tmpfs(RAM-backed) |

### 1.4 三档部署矩阵

| 档位 | 隔离方式 | namespace | 节点池 | 密钥 | 存储 | 适用 | 成本 |
|---|---|---|---|---|---|---|---|
| 共享池 | 逻辑隔离 | `nexus-exec-shared` | 共享节点池 | 共享 CMK 或派生 | 共享桶+租户前缀 | 中小客户/非敏感 | 低 |
| 专属池 | 独立节点池 | `nexus-exec-{tenant}` | 专用节点(污点+tolerations) | 按租户 CMK | 独立桶/前缀 | 大客户/合规 | 中 |
| 私有化 | 独立集群 | `nexus-exec-{tenant}`（独立 VPC 内集群） | 独立集群全部节点 | 独立 KMS/HSM | 独立 MinIO 集群 | 金融/政务/国企 | 高 |

**隔离四重取证**（向客户证明，缺一不可）：

1. **逻辑**：所有查询强制带 `tenant_id`，Postgres RLS 兜底防应用层漏加条件
2. **运行时**：namespace + 节点亲和性 + NetworkPolicy
3. **密钥**：按租户 CMK，租户禁用后其数据不可解密
4. **存储**：对象存储按租户前缀 + 独立桶策略，禁止跨前缀列举

### 1.5 高敏租户：Kata Containers / Firecracker

```yaml
# 高敏租户 Sandbox Pod 使用 Kata Containers runtimeClass
apiVersion: v1
kind: Pod
metadata:
  name: sandbox-kata-xxxx
  namespace: nexus-exec-finance-corp
spec:
  runtimeClassName: kata-containers    # 内核级隔离，每个 Pod 一个轻量虚拟机
  nodeSelector:
    node-role.kubernetes.io/kata: "true"
  tolerations:
  - key: "kata-exclusive"
    operator: "Equal"
    value: "true"
    effect: "NoSchedule"
  containers:
  - name: codex-app-server
    securityContext:
      privileged: false
      runAsNonRoot: true
      readOnlyRootFilesystem: true
      allowPrivilegeEscalation: false
      capabilities:
        drop: ["ALL"]
      seccompProfile:
        type: Localhost
        localhostProfile: "nexus-sandbox"
```

| 运行时 | 隔离级别 | 启动延迟 | 适用 |
|---|---|---|---|
| runc（默认） | 命名空间 + cgroups | ~1s | 共享池 |
| gVisor | 系统调用拦截 | ~2s | 中等敏感 |
| Kata Containers | 硬件级（QEMU 轻量虚拟机） | ~3s | 高敏租户 |
| Firecracker | 微虚拟机（Rust 实现） | ~2s | 高敏 + 高密度 |

---

## 2. 弹性伸缩

### 2.1 控制面 HPA 策略

| Deployment | 扩缩指标 | minReplicas | maxReplicas | 目标值 |
|---|---|---|---|---|
| API Gateway | CPU 70% + 自定义 QPS | 3 | 20 | CPU<70%, QPS<800/pod |
| Temporal Worker | 队列深度（活跃 Workflow 数） | 2 | 10 | <50/pod |
| 审批中心 | 活跃 Approval Ticket 数 | 2 | 8 | <30/pod |
| 策略中心 | QPS | 2 | 6 | <500/pod |
| 配额计费 | QPS | 2 | 6 | <300/pod |
| 连接器治理 | 活跃 MCP 连接数 | 2 | 8 | <100/pod |
| 知识库 RAG | 向量查询延迟 P95 | 2 | 10 | <200ms |

**HPA 关键 YAML**：

```yaml
apiVersion: autoscaling/v2
kind: HorizontalPodAutoscaler
metadata:
  name: nexus-api-gateway-hpa
  namespace: nexus-control
spec:
  scaleTargetRef:
    apiVersion: apps/v1
    kind: Deployment
    name: nexus-api-gateway
  minReplicas: 3
  maxReplicas: 20
  metrics:
  - type: Resource
    resource:
      name: cpu
      target:
        type: Utilization
        averageUtilization: 70
  - type: Pods
    pods:
      metric:
        name: http_requests_per_second
      target:
        type: AverageValue
        averageValue: "800"
  behavior:
    scaleUp:
      stabilizationWindowSeconds: 30
      policies:
      - type: Percent
        value: 100
        periodSeconds: 30
    scaleDown:
      stabilizationWindowSeconds: 300
      policies:
      - type: Percent
        value: 25
        periodSeconds: 60
```

### 2.2 Sandbox Pod 池弹性

**warm pool 机制**：

```
                    ┌─────────────────────────────────┐
                    │   sandbox-pool-controller       │
                    │   (Deployment, 1 replica)        │
                    └────────┬────────────────────────┘
                             │ 监控
                    ┌────────▼────────────────────────┐
                    │   warm pool 状态                 │
                    │   目标空闲 Pod: N=5（按租户权重）│
                    │   当前空闲: 3                     │
                    │   补充中: 2                       │
                    └────────┬────────────────────────┘
                             │
              ┌──────────────┼──────────────┐
              ▼              ▼              ▼
        ┌──────────┐  ┌──────────┐  ┌──────────┐
        │ Pod warm1│  │ Pod warm2│  │ Pod warm3│
        │ (空闲)   │  │ (空闲)   │  │ (空闲)   │
        └──────────┘  └──────────┘  └──────────┘
              │
              │ 任务到达 → controller 分配
              ▼
        ┌──────────────┐
        │ Pod warm1     │
        │ → 注入 config │
        │ → 注入凭据    │
        │ → 启动任务    │
        │ 状态: active  │
        └──────────────┘
              │
              │ 任务完成 → controller 回收
              ▼
        销毁 Pod → 上传 rollout → 结算 → 审计
        warm pool 检测缺口 → 补充新 Pod
```

| 参数 | 值 | 理由 |
|---|---|---|
| warm pool 大小 | N=5（默认），按租户权重调整 | 小租户共享，大租户专属 |
| 冷启动目标 | < 5s | warm pool + PVC snapshot + 预热镜像 |
| 空闲超时 | 15-30 min | 平衡预热成本与响应速度 |
| 最大并发 | 租户级上限 + 全局上限 | 超限进入排队 |
| 队列调度 | 按租户权重 + 优先级 + FIFO | 大客户高权重 |
| 销毁前必做 | 上传 rollout + 结算 + 审计 | 不可跳过 |

**租户权重队列**：

```yaml
# 租户配额配置（ConfigMap，运行时可调）
apiVersion: v1
kind: ConfigMap
metadata:
  name: tenant-weights
  namespace: nexus-control
data:
  weights.yaml: |
    tenants:
      tenant-small-a:
        weight: 1
        maxConcurrent: 2
        priority: 10
      tenant-large-b:
        weight: 5
        maxConcurrent: 10
        priority: 50
      tenant-enterprise-c:
        weight: 10
        maxConcurrent: 20
        priority: 100
        dedicated: true
        namespace: nexus-exec-tenant-c
```

### 2.3 多集群灾备

| 组件 | 灾备策略 | RPO | RTO |
|---|---|---|---|
| Postgres | 流复制（同步）到跨 AZ 只读副本 + 异地异步副本 | <1s（同步）/ <5min（异步） | <30s（故障转移） |
| Redis | Sentinel 自动故障转移 + AOF 持久化 | <1s | <10s |
| MinIO | 跨区复制（bucket replication） | <1min | 即时（只读降级） |
| 对象存储 rollout | 版本化 + 多区域副本 | 0（写即复制） | 即时 |
| K8s 集群 | 跨 AZ 多节点 + 控制面多副本 | — | <5min（Velero 恢复） |

**Postgres 流复制拓扑**：

```
     AZ-A                        AZ-B                        AZ-C
  ┌──────────┐  同步复制    ┌──────────┐  异步复制    ┌──────────┐
  │ Primary  │───────────→│ Replica1 │───────────→│ Replica2 │
  │ (读写)   │             │ (只读)   │             │ (异地灾备│
  └──────────┘             └──────────┘             │  只读)   │
       │                        │                   └──────────┘
       └── WAL 归档 → MinIO     └── 读取分流
```

### 2.4 节点亲和与专用节点

```yaml
# 执行面节点专用（污点 + tolerations）
apiVersion: v1
kind: Node
metadata:
  name: exec-node-01
  labels:
    node-role.kubernetes.io/nexus-exec: "true"
    nexus.io/tier: "shared"
spec:
  taints:
  - key: "nexus-exec"
    value: "true"
    effect: "NoSchedule"
---
# GPU 节点（本地模型推理）
apiVersion: v1
kind: Node
metadata:
  name: gpu-node-01
  labels:
    node-role.kubernetes.io/nexus-gpu: "true"
spec:
  taints:
  - key: "nvidia.com/gpu"
    value: "true"
    effect: "NoSchedule"
```

---

## 3. 自动化运维

### 3.1 CI/CD：GitOps（ArgoCD）

```
┌──────────────────────────────────────────────────────────┐
│                    Git 仓库（GitOps 单一真相源）            │
│  ├── deploy/k8s/         K8s 清单（Kustomize）             │
│  ├── deploy/overlays/    环境叠加（dev/staging/prod）       │
│  └── deploy/charts/      Helm Chart（可选）                 │
└──────────────────────┬────────────────────────────────────┘
                       │ git push
            ┌──────────▼──────────┐
            │     ArgoCD          │
            │  (GitOps 控制器)    │
            └──────────┬──────────┘
                       │ sync
     ┌─────────────────┼─────────────────┐
     ▼                 ▼                 ▼
  dev cluster    staging cluster    prod cluster
  (kind)         (云集群)           (多 AZ)
```

**镜像构建**：

| 镜像 | 基础 | 内容 | 大小 |
|---|---|---|---|
| `nexus-control` | `distroless/nodejs20` | API 网关 + 控制面服务 | ~150MB |
| `nexus-sandbox-python` | `python:3.12-slim` + codex 二进制 | Python 工具链 | ~800MB |
| `nexus-sandbox-node` | `node:20-slim` + codex 二进制 | Node.js 工具链 | ~700MB |
| `nexus-sandbox-rust` | `rust:1.75-slim` + codex 二进制 | Rust 工具链 | ~1.2GB |
| `nexus-sandbox-full` | `ubuntu:22.04` + 多语言 | 全栈 | ~2.5GB |

**多阶段 Dockerfile 示例（Python sandbox）**：

```dockerfile
# Stage 1: 构建 Codex 二进制
FROM rust:1.75 AS codex-builder
WORKDIR /build
COPY codex-rs/ .
RUN cargo build --release -p codex-app-server

# Stage 2: 运行时镜像
FROM python:3.12-slim AS runtime
# 安装最小系统依赖
RUN apt-get update && apt-get install -y --no-install-recommends \
    git ca-certificates bubblewrap && \
    rm -rf /var/lib/apt/lists/*

# 复制 codex 二进制
COPY --from=codex-builder /build/target/release/codex-app-server /usr/local/bin/

# 非 root 用户
RUN useradd -m -s /bin/bash nexus
USER nexus
WORKDIR /workspace

# 健康检查
HEALTHCHECK --interval=30s --timeout=5s --retries=3 \
  CMD pgrep codex-app-server || exit 1

# 入口由 K8s config-secret 注入
ENTRYPOINT ["codex-app-server"]
```

**发布策略**：

| 策略 | 适用 | 方法 |
|---|---|---|
| 蓝绿 | 控制面 | ArgoCD + service selector 切换 |
| 金丝雀 | 控制面 + 沙箱镜像 | ArgoCD Rollout + 流量百分比 |
| 滚动 | StatefulSet（Postgres/Redis/MinIO） | maxUnavailable=0 |
| 即时回滚 | 全部 | ArgoCD `argocd app rollback` |

### 3.2 配置注入

**控制面按租户生成配置，运行时注入 Pod，任务结束即焚**：

```
租户身份 + 角色 + 工作区 + 风险等级
         │
         ▼
┌─────────────────────┐
│  Config Generator    │  ← 控制面服务
│  (策略中心子模块)     │
└────────┬────────────┘
         │ 生成
    ┌────┴────┬────────┬───────────┬───────────┐
    ▼         ▼        ▼           ▼           ▼
 config.toml  execpolicy  enabled_   AGENTS.md   MCP
 (模型路由)   .rules      tools.json (企业规范)  声明
    │         │        │           │           │
    └─────────┴────────┴───────────┴───────────┘
                        │
                        ▼ 投射卷（Projected Volume）
              ┌──────────────────┐
              │  K8s Secret/CM   │  ← 任务级，TTL=任务时长
              │  (运行时注入)     │
              └────────┬─────────┘
                       ▼
              ┌──────────────────┐
              │  Sandbox Pod     │
              │  /etc/codex/     │  ← 只读挂载
              │  └── config.toml │
              │  └── execpolicy/ │
              │  └── tools.json  │
              │  └── AGENTS.md   │
              │  └── mcp.toml    │
              └──────────────────┘
                       │ 任务结束
                       ▼
              Pod 销毁 → Secret/CM 自动回收
              (config 不落盘到镜像，不残留)
```

**config.toml 生成示例**（按租户角色动态生成，不写真实密钥）：

```toml
# 生成时间: 2026-09-06T12:00:00Z
# 绑定: tenant=acme-corp, thread=th_abc, turn=tn_123
# TTL: 3600s

[model]
provider = "openai-compatible"
base_url = "http://model-gateway.nexus-control:8080/v1"
# 不写 API Key — 用令牌换取
auth_token_env = "NEXUS_TASK_TOKEN"

[sandbox]
mode = "workspace-write"          # 按角色 × 风险等级
linux = "landlock"                # Codex OS 沙箱

[approval]
policy = "on-failure"             # 失败才审批

[mcp]
server_config_dir = "/etc/codex/mcp"
# MCP 地址指向同 Pod 的 sidecar，不写真实凭据
```

### 3.3 沙箱启动自检

**自检清单（自检不过禁止调度生产任务）**：

| # | 检查项 | 方法 | 失败动作 |
|---|---|---|---|
| 1 | Landlock/seccomp/Seatbelt 可用性 | 容器内运行 `codex --sandbox-check` | 标记节点不可调度 |
| 2 | 出站仅两白名单地址 | `curl` Model Gateway + MCP Gateway，`curl` 其他地址应超时 | 拒绝启动 |
| 3 | 镜像无长期密钥 | 镜像扫描（Trivy）+ `grep` 常见密钥模式 | 镜像拒绝推送 |
| 4 | 只读 rootfs + 非 root | `mount -o ro /` + `id -u` != 0 | Pod 拒启 |
| 5 | 资源限额生效 | `/sys/fs/cgroup/.../memory.max` 存在且有限 | 拒绝调度 |
| 6 | NetworkPolicy 已附加 | `kubectl get networkpolicy -n {ns}` | 拒绝调度 |
| 7 | seccomp profile 已加载 | `/proc/self/status` Seccomp 字段 | 拒绝调度 |

```yaml
# 沙箱启动自检 initContainer
apiVersion: v1
kind: Pod
metadata:
  name: sandbox-with-preflight
spec:
  initContainers:
  - name: preflight-check
    image: nexus-sandbox-python:latest
    command:
    - bash
    - -c
    - |
      set -e
      # 1. 沙箱可用性
      codex --sandbox-check || { echo "SANDBOX_CHECK_FAILED"; exit 1; }
      # 2. 出站白名单
      curl -sSf --max-time 3 http://model-gateway.nexus-control:8080/health || { echo "MODEL_GW_UNREACHABLE"; exit 1; }
      curl -sSf --max-time 3 http://localhost:9090/health || { echo "MCP_GW_UNREACHABLE"; exit 1; }
      # 3. 非 root
      [ "$(id -u)" -ne 0 ] || { echo "RUNNING_AS_ROOT"; exit 1; }
      # 4. 只读 rootfs
      touch /test-write 2>/dev/null && { echo "ROOTFS_WRITABLE"; rm -f /test-write; exit 1; } || true
      # 5. 资源限额
      [ -f /sys/fs/cgroup/memory.max ] || { echo "NO_MEM_LIMIT"; exit 1; }
      echo "PREFLIGHT_OK"
    securityContext:
      runAsNonRoot: true
      readOnlyRootFilesystem: true
      allowPrivilegeEscalation: false
      capabilities:
        drop: ["ALL"]
  containers:
  - name: codex-app-server
    # ...主容器配置
```

### 3.4 备份灾备

| 备份对象 | 方法 | 频率 | 保留 | 恢复目标 |
|---|---|---|---|---|
| Postgres | `pg_dump` + CronJob → MinIO | 每日全备 + 每小时增量 | 30 天 + 12 月归档 | <30min |
| MinIO 对象 | 跨区复制（bucket replication） | 实时 | 按策略 | 即时 |
| rollout 文件 | 版本化 + 跨区副本 | 写即复制 | 无限（版本化） | 即时 |
| PV 快照 | Velero + CSI snapshot | 每日 | 7 天 | <5min |
| K8s 资源 | Velero backup（含 ConfigMap/Secret） | 每日 | 7 天 | <10min |
| Redis | AOF + RDB → MinIO | 每 5min | 1 天 | <1min |

**DR Runbook 摘要**：

```
1. 检测故障 → Prometheus 告警 → PagerDuty 通知
2. 确认故障范围 → kubectl get nodes / pods -A
3. 故障转移：
   a. Postgres: 提升只读副本为新主 → 更新 Service
   b. Redis: Sentinel 自动故障转移 → 更新连接
   c. MinIO: 切换到灾备区域端点
4. 恢复 K8s 资源 → velero restore
5. 验证 → 健康检查 + 烟雾测试
6. 复盘 → issue 沉淀 + 预防措施
```

### 3.5 监控告警

**全链路 Trace（OTel → ClickHouse + Grafana）**：

```
用户请求 → API Gateway → Temporal Workflow → Sandbox Pod → codex app-server
  → Tool Call → MCP Gateway → 外部 API
  → Model Gateway → 模型采样
  → 事件回吐 → Postgres → WebSocket → 用户
```

每一段都带 OTel span，串联形成完整 trace，可按 `tenant_id → thread_id → turn_id → item_seq` 精确定位。

**Prometheus 指标**：

| 指标 | 类型 | 告警阈值 |
|---|---|---|
| `nexus_concurrent_tasks` | Gauge | > 租户并发上限 → 超限告警 |
| `nexus_task_duration_p95` | Histogram | > 300s → 延迟告警 |
| `nexus_task_error_rate` | Counter | > 5% → 错误率告警 |
| `nexus_cost_per_task` | Gauge | 超日均 3σ → 成本异常告警 |
| `nexus_sandbox_escape_attempts` | Counter | > 0 → 沙箱逃逸告警（P0） |
| `nexus_cross_tenant_access_denied` | Counter | > 0 → 跨租户越权告警（P0） |
| `nexus_warm_pool_size` | Gauge | < 2 → warm pool 不足告警 |
| `nexus_pg_replication_lag` | Gauge | > 5s → 复制延迟告警 |

**告警分级**：

| 级别 | 触发条件 | 通知方式 | 响应时间 |
|---|---|---|---|
| P0 | 沙箱逃逸 / 跨租户越权 / 数据外泄 | PagerDuty + IM + 电话 | <5min |
| P1 | 控制面宕机 / Postgres 不可用 / API 网关 5xx > 10% | PagerDuty + IM | <15min |
| P2 | 并发超限 / 延迟 P95 > 300s / warm pool < 2 | IM 群 | <1h |
| P3 | 成本异常 / 资源使用率 > 85% | 日报 | <1d |

### 3.6 安全运维

| 维度 | 措施 |
|---|---|
| 镜像安全 | Trivy 扫描（CI 门禁，CRITICAL 阻断推送）+ 镜像签名（Cosign） |
| 网络策略 | 执行面默认全禁出站，仅放行 Model Gateway + MCP Gateway |
| Seccomp | `RuntimeDefault` 或自定义 `nexus-sandbox` profile（禁 ptrace/mount/...） |
| AppArmor | 每个执行面节点加载 `nexus-sandbox` profile |
| PSA | `restricted` 级别（禁止 privileged/hostPath/hostNetwork） |
| 密钥管理 | HashiCorp Vault 或云 KMS，按租户 CMK，IRSA/Workload Identity 取短期凭证 |
| 审计 | 所有 API 调用、工具执行、模型采样写入 WORM 审计日志 |
| 红队演练 | 每季度跨租户越权演练（从 A 租户沙箱尝试访问 B 的资源） |

**NetworkPolicy 关键清单**：

```yaml
# 执行面默认全禁出站
apiVersion: networking.k8s.io/v1
kind: NetworkPolicy
metadata:
  name: sandbox-default-deny-egress
  namespace: nexus-exec-shared
spec:
  podSelector:
    matchLabels:
      app: sandbox-pod
  policyTypes:
  - Egress
  egress: []   # 默认全禁
---
# 仅放行 Model Gateway + MCP Gateway + DNS
apiVersion: networking.k8s.io/v1
kind: NetworkPolicy
metadata:
  name: sandbox-egress-allow
  namespace: nexus-exec-shared
spec:
  podSelector:
    matchLabels:
      app: sandbox-pod
  policyTypes:
  - Egress
  egress:
  - to:    # Model Gateway
    - namespaceSelector:
        matchLabels:
          name: nexus-control
      podSelector:
        matchLabels:
          app: model-gateway
    ports:
    - protocol: TCP
      port: 8080
  - to:    # MCP Gateway (同 Pod sidecar, 经 localhost)
    - podSelector:
        matchLabels:
          app: sandbox-pod
    ports:
    - protocol: TCP
      port: 9090
  - to:    # DNS (kube-system)
    - namespaceSelector:
        matchLabels:
          name: kube-system
    ports:
    - protocol: UDP
      port: 53
```

---

## 4. 关键 K8s 清单概要

### 4.1 Namespace + PSA

```yaml
apiVersion: v1
kind: Namespace
metadata:
  name: nexus-control
  labels:
    name: nexus-control
    pod-security.kubernetes.io/enforce: restricted
    pod-security.kubernetes.io/audit: restricted
    pod-security.kubernetes.io/warn: restricted
---
apiVersion: v1
kind: Namespace
metadata:
  name: nexus-exec-shared
  labels:
    name: nexus-exec-shared
    pod-security.kubernetes.io/enforce: restricted
    pod-security.kubernetes.io/audit: restricted
    pod-security.kubernetes.io/warn: restricted
```

### 4.2 控制面 Deployment（API Gateway）

```yaml
apiVersion: apps/v1
kind: Deployment
metadata:
  name: nexus-api-gateway
  namespace: nexus-control
spec:
  replicas: 3
  strategy:
    type: RollingUpdate
    rollingUpdate:
      maxSurge: 1
      maxUnavailable: 0
  selector:
    matchLabels:
      app: nexus-api-gateway
  template:
    metadata:
      labels:
        app: nexus-api-gateway
    spec:
      securityContext:
        runAsNonRoot: true
        runAsUser: 1000
        fsGroup: 1000
        seccompProfile:
          type: RuntimeDefault
      containers:
      - name: gateway
        image: nexus-control:latest
        ports:
        - containerPort: 8080
          name: http
        - containerPort: 8081
          name: ws
        env:
        - name: DATABASE_URL
          valueFrom:
            secretKeyRef:
              name: postgres-credentials
              key: url
        - name: REDIS_URL
          valueFrom:
            secretKeyRef:
              name: redis-credentials
              key: url
        resources:
          requests:
            cpu: 500m
            memory: 512Mi
          limits:
            cpu: 2000m
            memory: 2Gi
        readinessProbe:
          httpGet:
            path: /ready
            port: 8080
          initialDelaySeconds: 10
          periodSeconds: 5
        livenessProbe:
          httpGet:
            path: /health
            port: 8080
          initialDelaySeconds: 15
          periodSeconds: 10
      topologySpreadConstraints:
      - maxSkew: 1
        topologyKey: topology.kubernetes.io/zone
        whenUnsatisfiable: DoNotSchedule
        labelSelector:
          matchLabels:
            app: nexus-api-gateway
      terminationGracePeriodSeconds: 120
      revisionHistoryLimit: 10
```

### 4.3 Postgres StatefulSet

```yaml
apiVersion: apps/v1
kind: StatefulSet
metadata:
  name: postgres-primary
  namespace: nexus-control
spec:
  serviceName: postgres-primary
  replicas: 1           # Primary；只读副本单独 StatefulSet
  selector:
    matchLabels:
      app: postgres
      role: primary
  template:
    metadata:
      labels:
        app: postgres
        role: primary
    spec:
      securityContext:
        runAsNonRoot: true
        runAsUser: 999   # postgres
        fsGroup: 999
      containers:
      - name: postgres
        image: postgres:16-alpine
        env:
        - name: POSTGRES_DB
          value: nexus
        - name: POSTGRES_USER
          valueFrom:
            secretKeyRef:
              name: postgres-credentials
              key: username
        - name: POSTGRES_PASSWORD
          valueFrom:
            secretKeyRef:
              name: postgres-credentials
              key: password
        - name: PGDATA
          value: /var/lib/postgresql/data
        ports:
        - containerPort: 5432
        resources:
          requests:
            cpu: 1000m
            memory: 2Gi
          limits:
            cpu: 4000m
            memory: 8Gi
        volumeMounts:
        - name: data
          mountPath: /var/lib/postgresql/data
        - name: wal-archive
          mountPath: /var/lib/postgresql/wal
  volumeClaimTemplates:
  - metadata:
      name: data
    spec:
      accessModes: ["ReadWriteOnce"]
      storageClassName: fast-ssd
      resources:
        requests:
          storage: 50Gi
  - metadata:
      name: wal-archive
    spec:
      accessModes: ["ReadWriteOnce"]
      storageClassName: fast-ssd
      resources:
        requests:
          storage: 10Gi
```

### 4.4 Redis StatefulSet

```yaml
apiVersion: apps/v1
kind: StatefulSet
metadata:
  name: redis-bus
  namespace: nexus-control
spec:
  serviceName: redis-bus
  replicas: 3
  selector:
    matchLabels:
      app: redis-bus
  template:
    metadata:
      labels:
        app: redis-bus
    spec:
      securityContext:
        runAsNonRoot: true
        runAsUser: 999
        fsGroup: 999
      containers:
      - name: redis
        image: redis:7-alpine
        command:
        - redis-server
        - --appendonly
        - "yes"
        - --requirepass
        - $(REDIS_PASSWORD)
        ports:
        - containerPort: 6379
        resources:
          requests:
            cpu: 200m
            memory: 256Mi
          limits:
            cpu: 1000m
            memory: 1Gi
        volumeMounts:
        - name: data
          mountPath: /data
  volumeClaimTemplates:
  - metadata:
      name: data
    spec:
      accessModes: ["ReadWriteOnce"]
      storageClassName: fast-ssd
      resources:
        requests:
          storage: 10Gi
```

### 4.5 MinIO StatefulSet

```yaml
apiVersion: apps/v1
kind: StatefulSet
metadata:
  name: minio
  namespace: nexus-control
spec:
  serviceName: minio
  replicas: 4
  selector:
    matchLabels:
      app: minio
  template:
    metadata:
      labels:
        app: minio
    spec:
      containers:
      - name: minio
        image: minio/minio:latest
        command:
        - minio
        - server
        - /data
        - --console-address
        - ":9001"
        env:
        - name: MINIO_ROOT_USER
          valueFrom:
            secretKeyRef:
              name: minio-credentials
              key: root-user
        - name: MINIO_ROOT_PASSWORD
          valueFrom:
            secretKeyRef:
              name: minio-credentials
              key: root-password
        ports:
        - containerPort: 9000
          name: s3
        - containerPort: 9001
          name: console
        volumeMounts:
        - name: data
          mountPath: /data
  volumeClaimTemplates:
  - metadata:
      name: data
    spec:
      accessModes: ["ReadWriteOnce"]
      storageClassName: standard
      resources:
        requests:
          storage: 200Gi
```

### 4.6 Sandbox Pod（带 warm pool 标签）

```yaml
apiVersion: v1
kind: Pod
metadata:
  name: sandbox-pod-warm-001
  namespace: nexus-exec-shared
  labels:
    app: sandbox-pod
    nexus.io/status: warm    # warm pool 标签
    nexus.io/tenant: ""      # 空表示未分配
spec:
  securityContext:
    runAsNonRoot: true
    runAsUser: 1000
    fsGroup: 1000
    seccompProfile:
      type: Localhost
      localhostProfile: nexus-sandbox
  initContainers:
  - name: preflight-check
    # ...（见 §3.3）
  containers:
  - name: codex-app-server
    image: nexus-sandbox-python:latest
    securityContext:
      runAsNonRoot: true
      readOnlyRootFilesystem: true
      allowPrivilegeEscalation: false
      capabilities:
        drop: ["ALL"]
    env:
    - name: NEXUS_TASK_TOKEN
      valueFrom:
        secretKeyRef:
          name: task-token-001    # 任务级 Secret，TTL=任务时长
          key: token
    volumeMounts:
    - name: config
      mountPath: /etc/codex
      readOnly: true
    - name: workspace
      mountPath: /workspace
    - name: tmp
      mountPath: /tmp
    resources:
      requests:
        cpu: 500m
        memory: 1Gi
      limits:
        cpu: 2000m
        memory: 4Gi
  - name: mcp-gateway-sidecar
    image: nexus-mcp-gateway:latest
    securityContext:
      runAsNonRoot: true
      readOnlyRootFilesystem: true
      allowPrivilegeEscalation: false
      capabilities:
        drop: ["ALL"]
    ports:
    - containerPort: 9090
    env:
    - name: MCP_CREDENTIALS
      valueFrom:
        secretKeyRef:
          name: mcp-credentials-001  # 任务级凭据
          key: credentials
    volumeMounts:
    - name: tmp
      mountPath: /tmp
  volumes:
  - name: config
    projected:
      sources:
      - secret:
          name: codex-config-001       # config.toml
      - secret:
          name: execpolicy-001         # execpolicy.rules
      - configMap:
          name: agents-md-001          # AGENTS.md
      - configMap:
          name: enabled-tools-001      # enabled_tools.json
  - name: workspace
    persistentVolumeClaim:
      claimName: workspace-pvc-001     # git worktree / snapshot
  - name: tmp
    emptyDir:
      medium: Memory                    # tmpfs
      sizeLimit: 512Mi
  tolerations:
  - key: "nexus-exec"
    operator: "Equal"
    value: "true"
    effect: "NoSchedule"
  terminationGracePeriodSeconds: 120
```

### 4.7 Service + Ingress

```yaml
apiVersion: v1
kind: Service
metadata:
  name: nexus-api-gateway
  namespace: nexus-control
spec:
  selector:
    app: nexus-api-gateway
  ports:
  - name: http
    port: 8080
    targetPort: 8080
  - name: ws
    port: 8081
    targetPort: 8081
---
apiVersion: networking.k8s.io/v1
kind: Ingress
metadata:
  name: nexus-ingress
  namespace: nexus-control
  annotations:
    cert-manager.io/cluster-issuer: letsencrypt-prod
    nginx.ingress.kubernetes.io/websocket-services: ws
    nginx.ingress.kubernetes.io/proxy-body-size: 100m
    nginx.ingress.kubernetes.io/proxy-read-timeout: 3600
spec:
  ingressClassName: nginx
  tls:
  - hosts:
    - nexus.example.com
    secretName: nexus-tls
  rules:
  - host: nexus.example.com
    http:
      paths:
      - path: /api
        pathType: Prefix
        backend:
          service:
            name: nexus-api-gateway
            port:
              name: http
      - path: /ws
        pathType: Prefix
        backend:
          service:
            name: nexus-api-gateway
            port:
              name: ws
```

### 4.8 CronJob（Postgres 备份）

```yaml
apiVersion: batch/v1
kind: CronJob
metadata:
  name: pg-backup
  namespace: nexus-control
spec:
  schedule: "0 2 * * *"           # 每日 02:00
  successfulJobsHistoryLimit: 3
  failedJobsHistoryLimit: 5
  jobTemplate:
    spec:
      template:
        spec:
          restartPolicy: OnFailure
          containers:
          - name: pg-backup
            image: postgres:16-alpine
            env:
            - name: PGPASSWORD
              valueFrom:
                secretKeyRef:
                  name: postgres-credentials
                  key: password
            command:
            - bash
            - -c
            - |
              set -e
              TIMESTAMP=$(date +%Y%m%d-%H%M%S)
              pg_dump -h postgres-primary.nexus-control -U nexus nexus | \
                gzip > /backup/nexus-${TIMESTAMP}.sql.gz
              # 上传到 MinIO
              mc cp /backup/nexus-${TIMESTAMP}.sql.gz minio/backups/postgres/
              # 清理 30 天前的备份
              find /backup -name "nexus-*.sql.gz" -mtime +30 -delete
            volumeMounts:
            - name: backup
              mountPath: /backup
          volumes:
          - name: backup
            emptyDir: {}
```

### 4.9 ResourceQuota（租户级配额）

```yaml
apiVersion: v1
kind: ResourceQuota
metadata:
  name: tenant-quota-shared
  namespace: nexus-exec-shared
spec:
  hard:
    requests.cpu: "32"
    requests.memory: 64Gi
    limits.cpu: "64"
    limits.memory: 128Gi
    pods: "50"
    persistentvolumeclaims: "20"
```

### 4.10 PodDisruptionBudget

```yaml
apiVersion: policy/v1
kind: PodDisruptionBudget
metadata:
  name: api-gateway-pdb
  namespace: nexus-control
spec:
  minAvailable: 2
  selector:
    matchLabels:
      app: nexus-api-gateway
---
apiVersion: policy/v1
kind: PodDisruptionBudget
metadata:
  name: postgres-pdb
  namespace: nexus-control
spec:
  maxUnavailable: 0
  selector:
    matchLabels:
      app: postgres
      role: primary
```

---

## 5. 运维流程

### 5.1 发布流程

```
开发 → CI(测试+镜像构建) → 推送镜像 → 更新 GitOps 仓库
  → ArgoCD 检测变更 → 生成 diff → 人工审批 → 同步
  → 金丝雀(10%流量) → 验证 → 全量发布
  → 异常 → ArgoCD 回滚(argocd app rollback)
```

### 5.2 沙箱生命周期

```
1. warm pool 控制器维持 N 个空闲 Pod
2. 任务到达 → controller 选取空闲 Pod → 注入 config/凭据/AGENTS.md
3. initContainer 自检 → 通过 → 启动 codex app-server
4. 事件流回传 → 控制面消费 → 写 Postgres → 推 WS
5. 任务完成 → 上传 rollout → 结算 → 审计
6. Pod 销毁 → Secret/CM 回收 → warm pool 补充新 Pod
```

### 5.3 故障应急

| 场景 | 检测 | 响应 |
|---|---|---|
| 沙箱 Pod OOM | liveness probe 失败 | 重启 Pod → 若持续 → 节点驱逐 |
| Postgres 主库宕机 | pg_replication_lag 告警 | 提升只读副本 → 更新 Service |
| 网络策略误配 | 跨租户越权告警 | 立即隔离 ns → 紧急修复 → 红队验证 |
| 镜像投毒 | Trivy 扫描 | 阻止部署 → 镜像下架 → 审计影响范围 |
| 磁盘满 | node_disk_pressure | 扩容 PVC → 清理 → 告警提前 |
| 模型 API 限流 | Model Gateway 4xx/5xx | 降级到备模型 → 告知用户 → 排队 |

---

## 6. 与八层架构对齐

| 层 | K8s 对应 | 运维职责 |
|---|---|---|
| L1 接入 | Ingress + Cert-Manager + ExternalDNS | TLS 证书轮转、域名管理 |
| L2 网关 | Deployment(API Gateway) + HPA | 流量管理、限流配置 |
| L3 控制面 | Deployment(Temporal/审批/策略/计费/连接器/知识库) + HPA | 工作流编排、策略下发 |
| L4 执行面 | Pod(sandbox) + warm pool + NetworkPolicy | 沙箱调度、自检、销毁 |
| L5 Harness | 容器内 codex app-server | 镜像构建、版本管理 |
| L6 模型 | Deployment(Model Gateway) | 模型路由、故障转移 |
| L7 存储 | StatefulSet(Postgres/Redis/MinIO) + CronJob | 备份灾备、容量规划 |
| 贯穿安全 | NetworkPolicy + Seccomp + PSA + KMS | 红队演练、密钥轮转 |

---

## 7. 验收清单

| # | 验收项 | 方法 | 状态 |
|---|---|---|---|
| 1 | 控制面 3 副本跨 AZ 部署 | `kubectl get pods -n nexus-control -o wide` | □ |
| 2 | HPA 生效（CPU > 70% 自动扩容） | 压测 + `kubectl get hpa` | □ |
| 3 | warm pool 维持 N 空闲 Pod | `kubectl get pods -l nexus.io/status=warm` | □ |
| 4 | 冷启动 < 5s | 端到端计时 | □ |
| 5 | 沙箱自检阻断不合规 Pod | 故意配错 → 自检失败 → Pod 不启动 | □ |
| 6 | NetworkPolicy 阻断出站 | `curl` 非白名单地址超时 | □ |
| 7 | Postgres 流复制可用 | 主库写入 → 只读副本读取一致 | □ |
| 8 | 备份可恢复 | `velero restore` + 数据校验 | □ |
| 9 | OTel trace 全链路 | 用户→任务→工具→模型 span 完整 | □ |
| 10 | 跨租户越权告警 | 红队演练 → P0 告警触发 | □ |
| 11 | GitOps 发布 + 回滚 | ArgoCD 发布 → `argocd app rollback` | □ |
| 12 | 三档隔离矩阵验证 | 共享/专属/私有化各跑一次 | □ |
