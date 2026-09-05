#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""生成 Nexus DevOps/K8s 部署拓扑图 SVG（深色主题，archify 风格）。
产出：k8s-topology.svg
转 PNG: rsvg-convert -w 2400 k8s-topology.svg -o k8s-topology.png
"""
import os
OUT = os.path.dirname(os.path.abspath(__file__))
BG="#0b141a"; PANEL="#13242e"; PANEL2="#173341"; TEXT="#e8f2f0"; MUTED="#9db8b4"
LINE="#24414e"; LINE2="#2e5a68"; ACCENT="#35c2b0"; ACCENT2="#028090"; GOLD="#e8b64c"
BLUE="#3b82f6"; RED="#ef4444"; PURPLE="#a855f7"
FONT="'Helvetica Neue',Helvetica,Arial,'PingFang SC','Microsoft YaHei','SimHei',sans-serif"

def open_svg(w,h):
    L=[f'<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 {w} {h}">']
    L.append(f'<style>text{{font-family:{FONT}}}</style>')
    L.append('<defs>')
    L.append(f'<marker id="a" viewBox="0 0 10 10" refX="9" refY="5" markerWidth="8" markerHeight="8" orient="auto"><path d="M0 0 L10 5 L0 10 z" fill="{ACCENT}"/></marker>')
    L.append(f'<marker id="ag" viewBox="0 0 10 10" refX="9" refY="5" markerWidth="8" markerHeight="8" orient="auto"><path d="M0 0 L10 5 L0 10 z" fill="{GOLD}"/></marker>')
    L.append(f'<marker id="ab" viewBox="0 0 10 10" refX="9" refY="5" markerWidth="8" markerHeight="8" orient="auto"><path d="M0 0 L10 5 L0 10 z" fill="{BLUE}"/></marker>')
    L.append(f'<marker id="ar" viewBox="0 0 10 10" refX="9" refY="5" markerWidth="8" markerHeight="8" orient="auto"><path d="M0 0 L10 5 L0 10 z" fill="{RED}"/></marker>')
    L.append(f'<linearGradient id="gt" x1="0" y1="0" x2="1" y2="1"><stop offset="0" stop-color="{ACCENT2}"/><stop offset="1" stop-color="{ACCENT}"/></linearGradient>')
    L.append(f'<linearGradient id="gg" x1="0" y1="0" x2="1" y2="1"><stop offset="0" stop-color="#b07d1a"/><stop offset="1" stop-color="{GOLD}"/></linearGradient>')
    L.append(f'<linearGradient id="gb" x1="0" y1="0" x2="1" y2="1"><stop offset="0" stop-color="#1e3a8a"/><stop offset="1" stop-color="{BLUE}"/></linearGradient>')
    L.append(f'<linearGradient id="gp" x1="0" y1="0" x2="1" y2="1"><stop offset="0" stop-color="#6b21a8"/><stop offset="1" stop-color="{PURPLE}"/></linearGradient>')
    L.append(f'<linearGradient id="gr" x1="0" y1="0" x2="1" y2="1"><stop offset="0" stop-color="#991b1b"/><stop offset="1" stop-color="{RED}"/></linearGradient>')
    L.append('</defs>')
    L.append(f'<rect width="{w}" height="{h}" fill="{BG}"/>')
    return L

def title(L,t,s,y=32):
    L.append(f'<text x="40" y="{y}" fill="{ACCENT}" font-size="22" font-weight="700">{t}</text>')
    if s: L.append(f'<text x="40" y="{y+24}" fill="{MUTED}" font-size="13">{s}</text>')

def node(L,x,y,w,h,t,fill=PANEL,stroke=LINE,fg=TEXT,size=12,bold=False,rx=8,grad=None):
    L.append(f'<rect x="{x}" y="{y}" width="{w}" height="{h}" rx="{rx}" fill="{grad or fill}" stroke="{stroke}" stroke-width="1.2"/>')
    L.append(f'<text x="{x+w/2}" y="{y+h/2+4}" text-anchor="middle" fill="{fg}" font-size="{size}" font-weight="{"700" if bold else "400"}">{t}</text>')

def edge(L,x1,y1,x2,y2,color=ACCENT,m="a",dash=None,w=1.6,curve=False):
    if curve:
        mx=(x1+x2)/2; d=f'M{x1} {y1} C {mx} {y1}, {mx} {y2}, {x2} {y2}'
    else:
        d=f'M{x1} {y1} L{x2} {y2}'
    dd=f' stroke-dasharray="{dash}"' if dash else ''
    L.append(f'<path d="{d}" fill="none" stroke="{color}" stroke-width="{w}" marker-end="url(#{m})"{dd}/>')

def cap(L,t,y,w=1600):
    L.append(f'<text x="40" y="{y}" fill="{MUTED}" font-size="11.5">{t}</text>')

def fin(L,fn):
    L.append('</svg>')
    s='\n'.join(L)
    s=s.replace("&","&amp;")
    s=s.replace("&amp;lt;","&lt;").replace("< 5","&lt; 5")  # escape bare < in text
    open(os.path.join(OUT,fn),'w').write(s); print("wrote",fn)

def fig():
    W,H=1600,1180; L=open_svg(W,H)
    title(L,"Nexus 自动化运维与 K8s 弹性部署 · 拓扑全景","控制面 ns nexus-control（上）↔ 执行面 ns nexus-exec-{tenant}（下）· 三档隔离矩阵侧栏 · NetworkPolicy 标注",40)

    # ========== 控制平面大框 ==========
    L.append(f'<rect x="40" y="90" width="1120" height="430" rx="14" fill="{PANEL}" stroke="{ACCENT2}" stroke-width="2"/>')
    L.append(f'<rect x="40" y="90" width="1120" height="34" rx="14" fill="url(#gt)"/>')
    L.append(f'<rect x="40" y="110" width="1120" height="14" fill="url(#gt)"/>')
    L.append(f'<text x="60" y="113" fill="#e8f2f0" font-size="14" font-weight="700">控制平面 namespace: nexus-control（长期有状态 · 多租户 · 强一致）</text>')

    # --- 第一行：控制面 Deployment ---
    deps=[("API Gateway\n+WS 网关\n(HPA)",0,ACCENT),
           ("任务编排器\nTemporal Worker\n(HPA)",1,ACCENT),
           ("审批中心\nHITL\n(HPA)",2,ACCENT),
           ("策略中心\nexecpolicy\n(HPA)",3,ACCENT),
           ("配额计费\n四维归因\n(HPA)",4,GOLD),
           ("连接器治理\nMCP Registry\n(HPA)",5,PURPLE),
           ("知识库\nRAG+ACL\n(HPA)",6,BLUE)]
    for n,i,c in deps:
        x=70+i*150
        node(L,x,140,140,72,"",fill=PANEL2,stroke=c)
        for k,ln in enumerate(n.split("\n")):
            fw="700" if k==0 else "400"
            L.append(f'<text x="{x+70}" y="{162+k*15}" text-anchor="middle" fill="{"#e8f2f0" if k==0 else MUTED}" font-size="10.5" font-weight="{fw}">{ln}</text>')

    # --- 第二行：StatefulSet（有状态服务） ---
    L.append(f'<rect x="70" y="230" width="340" height="120" rx="8" fill="{PANEL2}" stroke="{BLUE}" stroke-width="1.4"/>')
    L.append(f'<text x="240" y="258" text-anchor="middle" fill="{BLUE}" font-size="13" font-weight="700">Postgres 主从 + pgvector</text>')
    L.append(f'<text x="240" y="278" text-anchor="middle" fill="{MUTED}" font-size="10.5">StatefulSet · 流复制 · RLS · 分区表</text>')
    L.append(f'<text x="240" y="295" text-anchor="middle" fill="{MUTED}" font-size="10.5">tenant/user/thread/turn/item</text>')
    L.append(f'<text x="240" y="312" text-anchor="middle" fill="{MUTED}" font-size="10.5">approval/usage/audit(WORM)</text>')
    L.append(f'<text x="240" y="335" text-anchor="middle" fill="{ACCENT}" font-size="9.5">PVC 50Gi · StorageClass fast-ssd</text>')

    L.append(f'<rect x="430" y="230" width="300" height="120" rx="8" fill="{PANEL2}" stroke="{RED}" stroke-width="1.4"/>')
    L.append(f'<text x="580" y="258" text-anchor="middle" fill="{RED}" font-size="13" font-weight="700">Redis 事件总线</text>')
    L.append(f'<text x="580" y="278" text-anchor="middle" fill="{MUTED}" font-size="10.5">StatefulSet · Sentinel 哨兵</text>')
    L.append(f'<text x="580" y="295" text-anchor="middle" fill="{MUTED}" font-size="10.5">pub/sub · Leader 选举</text>')
    L.append(f'<text x="580" y="312" text-anchor="middle" fill="{MUTED}" font-size="10.5">Agent IPC · 并发计数器</text>')
    L.append(f'<text x="580" y="335" text-anchor="middle" fill="{ACCENT}" font-size="9.5">PVC 10Gi</text>')

    L.append(f'<rect x="750" y="230" width="360" height="120" rx="8" fill="{PANEL2}" stroke="{GOLD}" stroke-width="1.4"/>')
    L.append(f'<text x="930" y="258" text-anchor="middle" fill="{GOLD}" font-size="13" font-weight="700">MinIO 对象存储</text>')
    L.append(f'<text x="930" y="278" text-anchor="middle" fill="{MUTED}" font-size="10.5">StatefulSet · 跨区复制</text>')
    L.append(f'<text x="930" y="295" text-anchor="middle" fill="{MUTED}" font-size="10.5">artifacts/rollouts/snapshots</text>')
    L.append(f'<text x="930" y="312" text-anchor="middle" fill="{MUTED}" font-size="10.5">按租户前缀 + 按租户 CMK</text>')
    L.append(f'<text x="930" y="335" text-anchor="middle" fill="{ACCENT}" font-size="9.5">PVC 200Gi · 跨区复制</text>')

    # --- 第三行：可观测与 CI/CD ---
    obs=[("OTel Collector\n→ ClickHouse\n+Grafana",70,ACCENT),
         ("Prometheus\n指标+告警",230,ACCENT),
         ("Loki\n日志聚合",390,MUTED),
         ("ArgoCD\nGitOps",550,PURPLE),
         ("Vault/KMS\n按租户CMK",710,GOLD)]
    for n,x,c in obs:
        node(L,x,370,140,56,"",fill=PANEL2,stroke=c)
        for k,ln in enumerate(n.split("\n")):
            fw="700" if k==0 else "400"
            L.append(f'<text x="{x+70}" y="{388+k*14}" text-anchor="middle" fill="{"#e8f2f0" if k==0 else MUTED}" font-size="10" font-weight="{fw}">{ln}</text>')

    # --- NetworkPolicy 标注（控制面）---
    L.append(f'<rect x="70" y="440" width="1040" height="60" rx="6" fill="{PANEL2}" stroke="{ACCENT2}" stroke-width="1"/>')
    L.append(f'<text x="580" y="465" text-anchor="middle" fill="{ACCENT}" font-size="11.5" font-weight="700">NetworkPolicy: 控制面内部全通 + 入站仅 Ingress Controller + 执行面出站仅→Model Gateway/MCP Gateway</text>')
    L.append(f'<text x="580" y="485" text-anchor="middle" fill="{MUTED}" font-size="10.5">PodSecurity: restricted · Seccomp(Default) · AppArmor · PSA enforce</text>')

    # ========== 中间连接区 ==========
    edge(L,320,520,320,560,color=GOLD,m="ag",w=2.2)
    edge(L,600,520,600,560,color=GOLD,m="ag",w=2.2)
    edge(L,880,520,880,560,color=GOLD,m="ag",w=2.2)
    L.append(f'<text x="600" y="550" text-anchor="middle" fill="{GOLD}" font-size="11" font-weight="700">① 调度指令(K8s Job/BLPOP 队列)  ② app-server JSON-RPC  ③ 事件流回吐</text>')

    # ========== 执行平面大框 ==========
    L.append(f'<rect x="40" y="560" width="1120" height="340" rx="14" fill="{PANEL}" stroke="{GOLD}" stroke-width="2"/>')
    L.append(f'<rect x="40" y="560" width="1120" height="34" rx="14" fill="url(#gg)"/>')
    L.append(f'<rect x="40" y="580" width="1120" height="14" fill="url(#gg)"/>')
    L.append(f'<text x="60" y="583" fill="#e8f2f0" font-size="14" font-weight="700">执行平面 namespace: nexus-exec-{{tenant}}（一次性 · 单租户单任务 · 无状态 · 可销毁）</text>')

    # --- Sandbox Pod 池 ---
    L.append(f'<rect x="70" y="606" width="1060" height="280" rx="10" fill="#1a2210" stroke="{RED}" stroke-width="1.6" stroke-dasharray="6,4"/>')
    L.append(f'<text x="90" y="628" fill="#fecaca" font-size="12.5" font-weight="700">Sandbox Pod 池（warm pool 预热 N 空闲 Pod · 冷启动 < 5s · 空闲超时 15-30min 销毁）</text>')

    # Pod 1: 共享池
    L.append(f'<rect x="90" y="640" width="470" height="110" rx="8" fill="{PANEL2}" stroke="{ACCENT2}" stroke-width="1.2"/>')
    L.append(f'<text x="105" y="660" fill="{ACCENT}" font-size="11" font-weight="700">Pod: 共享池（逻辑隔离）</text>')
    # codex app-server
    node(L,100,668,130,70,"",fill="#0d1b20",stroke=ACCENT)
    for k,ln in enumerate(["codex","app-server","config.toml","+execpolicy","运行时注入"]):
        L.append(f'<text x="165" y="{682+k*12}" text-anchor="middle" fill="{"#e8f2f0" if k<2 else MUTED}" font-size="9.5" font-weight="{"700" if k<2 else "400"}">{ln}</text>')
    # MCP Gateway sidecar
    node(L,240,668,130,70,"",fill="#1a1224",stroke=PURPLE)
    for k,ln in enumerate(["MCP Gateway","sidecar","凭据注入","工具白名单","出站代理"]):
        L.append(f'<text x="305" y="{682+k*12}" text-anchor="middle" fill="{"#e8f2f0" if k<2 else MUTED}" font-size="9.5" font-weight="{"700" if k<2 else "400"}">{ln}</text>')
    # Workspace PVC
    node(L,380,668,80,70,"",fill="#0d1820",stroke=BLUE)
    for k,ln in enumerate(["Workspace","git","worktree","+PVC","snapshot"]):
        L.append(f'<text x="420" y="{682+k*12}" text-anchor="middle" fill="{"#e8f2f0" if k==0 else MUTED}" font-size="9" font-weight="{"700" if k==0 else "400"}">{ln}</text>')
    # 边到边
    edge(L,230,703,240,703,w=1.2,color=MUTED)
    edge(L,370,703,380,703,w=1.2,color=MUTED)

    # Pod 2: 专属池
    L.append(f'<rect x="580" y="640" width="470" height="110" rx="8" fill="{PANEL2}" stroke="{GOLD}" stroke-width="1.2"/>')
    L.append(f'<text x="595" y="660" fill="{GOLD}" font-size="11" font-weight="700">Pod: 专属池（独立节点池+独立 ns+独立密钥）</text>')
    node(L,590,668,130,70,"",fill="#0d1b20",stroke=ACCENT)
    for k,ln in enumerate(["codex","app-server","独立镜像","独立 config","独立 execpolicy"]):
        L.append(f'<text x="655" y="{682+k*12}" text-anchor="middle" fill="{"#e8f2f0" if k<2 else MUTED}" font-size="9.5" font-weight="{"700" if k<2 else "400"}">{ln}</text>')
    node(L,730,668,130,70,"",fill="#1a1224",stroke=PURPLE)
    for k,ln in enumerate(["MCP Gateway","独立凭据域","专属 CMK","工具白名单","审计脱敏"]):
        L.append(f'<text x="795" y="{682+k*12}" text-anchor="middle" fill="{"#e8f2f0" if k<2 else MUTED}" font-size="9.5" font-weight="{"700" if k<2 else "400"}">{ln}</text>')
    node(L,870,668,80,70,"",fill="#0d1820",stroke=BLUE)
    for k,ln in enumerate(["Workspace","专属 PVC","独立","StorageClass","节点亲和"]):
        L.append(f'<text x="910" y="{682+k*12}" text-anchor="middle" fill="{"#e8f2f0" if k==0 else MUTED}" font-size="9" font-weight="{"700" if k==0 else "400"}">{ln}</text>')
    edge(L,720,703,730,703,w=1.2,color=MUTED)
    edge(L,860,703,870,703,w=1.2,color=MUTED)

    # Pod 3: Kata/Firecracker 高敏
    L.append(f'<rect x="90" y="760" width="960" height="110" rx="8" fill="{PANEL2}" stroke="{RED}" stroke-width="1.2"/>')
    L.append(f'<text x="105" y="780" fill="{RED}" font-size="11" font-weight="700">Pod: 高敏租户（Kata Containers / Firecracker 微虚拟机 · 独立 VPC · 数据不出域）</text>')
    node(L,100,788,150,70,"",fill="#1a0d0d",stroke=RED)
    for k,ln in enumerate(["Kata/Firecracker","微虚拟机","内核级隔离","codex app-server","+沙箱自检"]):
        L.append(f'<text x="175" y="{802+k*12}" text-anchor="middle" fill="{"#fecaca" if k<3 else MUTED}" font-size="9.5" font-weight="{"700" if k<3 else "400"}">{ln}</text>')
    node(L,260,788,140,70,"",fill="#1a1224",stroke=PURPLE)
    for k,ln in enumerate(["MCP Gateway","硬件级隔离","独立 VPC","KMS 专属 CMK","数据不出域"]):
        L.append(f'<text x="330" y="{802+k*12}" text-anchor="middle" fill="{"#e8f2f0" if k<2 else MUTED}" font-size="9.5" font-weight="{"700" if k<2 else "400"}">{ln}</text>')
    node(L,410,788,100,70,"",fill="#0d1820",stroke=BLUE)
    for k,ln in enumerate(["Workspace","加密 PVC","独立集群","不出域"]):
        L.append(f'<text x="460" y="{802+k*14}" text-anchor="middle" fill="{"#e8f2f0" if k==0 else MUTED}" font-size="9" font-weight="{"700" if k==0 else "400"}">{ln}</text>')
    # 出站白名单标注
    node(L,520,788,500,70,"",fill="#1a2210",stroke=ACCENT2)
    L.append(f'<text x="770" y="815" text-anchor="middle" fill="{ACCENT}" font-size="11" font-weight="700">出站 NetworkPolicy（三档统一）</text>')
    L.append(f'<text x="770" y="833" text-anchor="middle" fill="{MUTED}" font-size="10">默认全禁出站 → 仅放行 Model Gateway + MCP Gateway 两个白名单地址</text>')
    L.append(f'<text x="770" y="850" text-anchor="middle" fill="{MUTED}" font-size="10">Seccomp/AppArmor · 只读 rootfs · 非 root · 资源限额(CPU/Mem/PID/FD)</text>')
    edge(L,250,823,260,823,w=1.2,color=MUTED)
    edge(L,400,823,410,823,w=1.2,color=MUTED)
    edge(L,510,823,520,823,w=1.2,color=MUTED)

    # ========== 右侧三档矩阵 ==========
    L.append(f'<rect x="1180" y="90" width="380" height="810" rx="14" fill="{PANEL}" stroke="{LINE2}" stroke-width="1.6"/>')
    L.append(f'<rect x="1180" y="90" width="380" height="34" rx="14" fill="{PANEL2}"/>')
    L.append(f'<rect x="1180" y="110" width="380" height="14" fill="{PANEL2}"/>')
    L.append(f'<text x="1370" y="113" text-anchor="middle" fill="{ACCENT}" font-size="13" font-weight="700">三档部署矩阵</text>')

    # 档位1: 共享池
    L.append(f'<rect x="1200" y="140" width="340" height="200" rx="8" fill="{PANEL2}" stroke="{ACCENT2}" stroke-width="1.2"/>')
    L.append(f'<text x="1215" y="162" fill="{ACCENT}" font-size="12" font-weight="700">① 共享池</text>')
    L.append(f'<text x="1215" y="180" fill="{MUTED}" font-size="10">逻辑隔离：namespace + tenant_id + NetworkPolicy</text>')
    items1=["namespace: nexus-exec-shared","行级 RLS(tenant_id) 兜底","共享 Pod 池 + 租户权重队列","网络策略：出站两白名单","KMS: 共享 CMK 或按租户派生","适用：中小客户 / 非敏感数据","成本：低"]
    for k,v in enumerate(items1):
        L.append(f'<text x="1215" y="{196+k*18}" fill="{TEXT if k<5 else MUTED}" font-size="10">• {v}</text>')

    # 档位2: 专属池
    L.append(f'<rect x="1200" y="355" width="340" height="200" rx="8" fill="{PANEL2}" stroke="{GOLD}" stroke-width="1.2"/>')
    L.append(f'<text x="1215" y="377" fill="{GOLD}" font-size="12" font-weight="700">② 专属池</text>')
    L.append(f'<text x="1215" y="395" fill="{MUTED}" font-size="10">独立节点池 + 独立 ns + 独立密钥</text>')
    items2=["namespace: nexus-exec-{tenant}","独立节点池(污点+tolerations)","独立 Secret(按租户 CMK)","专用 HPA + 并发上限","对象存储独立桶/前缀","适用：大客户 / 有合规要求","成本：中"]
    for k,v in enumerate(items2):
        L.append(f'<text x="1215" y="{411+k*18}" fill="{TEXT if k<5 else MUTED}" font-size="10">• {v}</text>')

    # 档位3: 私有化
    L.append(f'<rect x="1200" y="570" width="340" height="200" rx="8" fill="{PANEL2}" stroke="{RED}" stroke-width="1.2"/>')
    L.append(f'<text x="1215" y="592" fill="{RED}" font-size="12" font-weight="700">③ 私有化</text>')
    L.append(f'<text x="1215" y="610" fill="{MUTED}" font-size="10">独立 VPC / 独立集群 / 数据不出域</text>')
    items3=["独立 K8s 集群(VPC 内)","Kata/Firecracker 微虚拟机","独立 KMS / HSM","本地模型(vLLM/Ollama)","数据完全不出域","适用：金融/政务/国企","成本：高"]
    for k,v in enumerate(items3):
        L.append(f'<text x="1215" y="{626+k*18}" fill="{TEXT if k<5 else MUTED}" font-size="10">• {v}</text>')

    # 四重取证
    L.append(f'<rect x="1200" y="785" width="340" height="100" rx="8" fill="{PANEL2}" stroke="{RED}" stroke-width="1.4"/>')
    L.append(f'<text x="1215" y="807" fill="{RED}" font-size="11.5" font-weight="700">四重隔离取证</text>')
    proofs=["1. 逻辑: RLS(tenant_id) 兜底","2. 运行时: ns+节点亲和+NetworkPolicy","3. 密钥: 按租户 CMK，禁用不可解密","4. 存储: 独立前缀+禁止跨列举"]
    for k,v in enumerate(proofs):
        L.append(f'<text x="1215" y="{823+k*16}" fill="{TEXT}" font-size="10">• {v}</text>')

    # ========== 底部图例与标注 ==========
    L.append(f'<rect x="40" y="920" width="1520" height="38" rx="8" fill="{PANEL2}" stroke="{LINE2}"/>')
    L.append(f'<text x="60" y="944" fill="{MUTED}" font-size="11">图例：</text>')
    L.append(f'<rect x="110" y="932" width="16" height="14" fill="{PANEL2}" stroke="{ACCENT2}"/>')
    L.append(f'<text x="132" y="944" fill="{MUTED}" font-size="10.5">控制面 Deployment(HPA)</text>')
    L.append(f'<rect x="290" y="932" width="16" height="14" fill="{PANEL2}" stroke="{BLUE}"/>')
    L.append(f'<text x="312" y="944" fill="{MUTED}" font-size="10.5">StatefulSet(有状态)</text>')
    L.append(f'<rect x="440" y="932" width="16" height="14" fill="{PANEL2}" stroke="{RED}" stroke-dasharray="4,3"/>')
    L.append(f'<text x="462" y="944" fill="{MUTED}" font-size="10.5">Sandbox Pod(可销毁)</text>')
    L.append(f'<rect x="600" y="932" width="16" height="14" fill="{PANEL2}" stroke="{PURPLE}"/>')
    L.append(f'<text x="622" y="944" fill="{MUTED}" font-size="10.5">MCP Gateway 侧车</text>')
    L.append(f'<rect x="760" y="932" width="16" height="14" fill="{PANEL2}" stroke="{GOLD}"/>')
    L.append(f'<text x="782" y="944" fill="{MUTED}" font-size="10.5">对象存储/CMK</text>')
    L.append(f'<rect x="900" y="932" width="16" height="14" fill="{PANEL2}" stroke="{RED}"/>')
    L.append(f'<text x="922" y="944" fill="{MUTED}" font-size="10.5">高敏/Kata</text>')

    cap(L,"图 · Nexus K8s 部署拓扑全景（控制面 nexus-control ↔ 执行面 nexus-exec-{tenant}，三档隔离矩阵，NetworkPolicy 全标注）",975,W)
    fin(L,"k8s-topology.svg")

fig()
print("DONE")
