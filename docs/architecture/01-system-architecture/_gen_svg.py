#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""生成 Nexus 系统架构图 SVG（深色主题，archify 风格）。
产出：system-architecture.svg（八层分层 + 控制平面/执行平面切分 + 13步生命周期）
转 PNG: rsvg-convert -w 2400 system-architecture.svg -o system-architecture.png
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
    L.append(f'<linearGradient id="gt" x1="0" y1="0" x2="1" y2="1"><stop offset="0" stop-color="{ACCENT2}"/><stop offset="1" stop-color="{ACCENT}"/></linearGradient>')
    L.append(f'<linearGradient id="gg" x1="0" y1="0" x2="1" y2="1"><stop offset="0" stop-color="#b07d1a"/><stop offset="1" stop-color="{GOLD}"/></linearGradient>')
    L.append(f'<linearGradient id="gb" x1="0" y1="0" x2="1" y2="1"><stop offset="0" stop-color="#1e3a8a"/><stop offset="1" stop-color="{BLUE}"/></linearGradient>')
    L.append(f'<linearGradient id="gp" x1="0" y1="0" x2="1" y2="1"><stop offset="0" stop-color="#6b21a8"/><stop offset="1" stop-color="{PURPLE}"/></linearGradient>')
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
    # 转义文本内容中的裸 &（SVG defs/marker/path 不含 &，安全）
    s=s.replace("&","&amp;")
    open(os.path.join(OUT,fn),'w').write(s); print("wrote",fn)

# ---- 图1 八层分层架构 ----
def fig1():
    W,H=1600,980; L=open_svg(W,H)
    title(L,"Nexus 企业级 AI Agent 平台 · 八层分层架构","控制平面(L1-L3) / 执行平面(L4) / Harness(L5,黑盒不改) / 模型(L6) / 存储治理(L7) · 安全合规贯穿",40)
    layers=[
        ("L1 接入层 · Access","Web 门户 · IM Bot(飞书/钉钉/企微/Slack) · IDE 插件 · OpenAPI+Webhook · CLI","url(#gb)","#dbeafe","否（自建）"),
        ("L2 网关层 · Gateway","API Gateway · WebSocket 网关 · 认证中间件(OIDC/SAML/SCIM) · 限流 · 配额预扣","url(#gb)","#dbeafe","否（自建）"),
        ("L3 控制平面 · Control Plane","身份租户 · 任务编排(Temporal) · 审批中心 · 策略中心 · 配额计费 · 连接器治理 · 知识库/RAG","url(#gt)","#d7f2ee","否（自建核心）"),
        ("L4 执行平面 · Execution","Runtime 池调度 · 沙箱 Pod · MCP Gateway 侧车 · Workspace 供给 · 凭据代理","url(#gg)","#f7ecd2","薄壳+复用"),
        ("L5 Harness · Agent 内核","Agent Loop(run_turn七阶段) · Tool Router · ExecPolicy · OS 沙箱 · 上下文压缩 · Skills/Hooks","url(#gp)","#ede9fe","是（黑盒不改）"),
        ("L6 模型层 · Model","Model Gateway · 多模型路由 · Responses 代理 · Token 计量 · 故障转移","url(#gt)","#d7f2ee","部分复用"),
        ("L7 存储与治理 · Storage & Governance","Postgres(RLS+分区) · 对象存储 · 向量库(pgvector) · 审计日志(WORM) · OTel · 评测中心","url(#gb)","#dbeafe","否（自建）"),
    ]
    y=90
    for i,(n,s,g,fg,rc) in enumerate(layers):
        h=88
        L.append(f'<rect x="40" y="{y}" width="1280" height="{h}" rx="10" fill="{g}" stroke="{LINE2}" stroke-width="1.2"/>')
        L.append(f'<text x="56" y="{y+30}" fill="#e8f2f0" font-size="15" font-weight="700">{n}</text>')
        L.append(f'<text x="56" y="{y+56}" fill="{fg}" font-size="11.5">{s}</text>')
        L.append(f'<rect x="1180" y="{y+24}" width="120" height="40" rx="6" fill="{PANEL2}" stroke="{LINE2}"/>')
        L.append(f'<text x="1240" y="{y+48}" text-anchor="middle" fill="{MUTED}" font-size="10.5">复用Codex: {rc}</text>')
        y+=h+8
    # 安全合规贯穿条
    L.append(f'<rect x="40" y="{y+4}" width="1280" height="38" rx="8" fill="{PANEL2}" stroke="{RED}" stroke-width="1.4"/>')
    L.append(f'<text x="660" y="{y+28}" text-anchor="middle" fill="#fecaca" font-size="12.5" font-weight="700">贯穿 · 安全与合规：租户隔离(四重取证) · KMS(按租户CMK) · 网络策略 · 审计留存 · 内容安全 · 红队演练</text>')
    cap(L,"图 1 · 八层分层架构（L1-L3/L6-L7 自建，L4 薄壳，L5 复用 Codex Harness 黑盒）",y+58,W)
    fin(L,"system-architecture.svg")

fig1()

# ---- 图2 控制平面 / 执行平面物理切分 ----
def fig2():
    W,H=1600,720; L=open_svg(W,H)
    title(L,"Nexus 控制平面 / 执行平面 · 物理切分","最关键的一条线：长期有状态多租户控制面 ↔ 一次性单任务可销毁执行面，仅经 app-server 协议与对象存储通信",40)
    # 控制平面大框
    L.append(f'<rect x="40" y="90" width="1520" height="250" rx="14" fill="{PANEL}" stroke="{ACCENT2}" stroke-width="2"/>')
    L.append(f'<rect x="40" y="90" width="1520" height="34" rx="14" fill="url(#gt)"/>')
    L.append(f'<rect x="40" y="110" width="1520" height="14" fill="url(#gt)"/>')
    L.append(f'<text x="60" y="113" fill="#e8f2f0" font-size="14" font-weight="700">控制平面（长期有状态 · 多租户 · 强一致）</text>')
    cps=[("API/WS\n网关",0),("任务编排器\n(Temporal)",1),("审批中心\n(HITL)",2),("策略中心\n(execpolicy)",3),("配额计费\n(四维)",4),("连接器治理\n(MCP Gateway)",5),("知识库\nRAG+ACL",6)]
    for n,i in cps:
        x=70+i*210
        node(L,x,140,190,70,"",fill=PANEL2,stroke=LINE2)
        for k,ln in enumerate(n.split("\n")):
            L.append(f'<text x="{x+95}" y="{168+k*16}" text-anchor="middle" fill="{TEXT}" font-size="11.5" font-weight="700">{ln}</text>')
    # Postgres + 对象存储
    L.append(f'<rect x="70" y="230" width="700" height="90" rx="8" fill="{PANEL2}" stroke="{LINE2}"/>')
    L.append(f'<text x="420" y="258" text-anchor="middle" fill="{ACCENT}" font-size="13" font-weight="700">Postgres（RLS + 分区）</text>')
    L.append(f'<text x="420" y="280" text-anchor="middle" fill="{MUTED}" font-size="10.5">tenant · user · thread · turn · item · approval · usage · audit</text>')
    L.append(f'<text x="420" y="298" text-anchor="middle" fill="{MUTED}" font-size="10.5">事件流幂等写入(thread_id+turn_id+item_seq) · WORM 审计</text>')
    L.append(f'<rect x="790" y="230" width="740" height="90" rx="8" fill="{PANEL2}" stroke="{LINE2}"/>')
    L.append(f'<text x="1160" y="258" text-anchor="middle" fill="{GOLD}" font-size="13" font-weight="700">对象存储（按租户前缀 + 按租户 CMK）</text>')
    L.append(f'<text x="1160" y="280" text-anchor="middle" fill="{MUTED}" font-size="10.5">artifacts · rollouts · snapshots · 向量库</text>')
    L.append(f'<text x="1160" y="298" text-anchor="middle" fill="{MUTED}" font-size="10.5">禁用租户 CMK → 数据不可解密</text>')
    # 中间连接
    edge(L,800,340,800,400,color=GOLD,m="ag",w=2.2)
    L.append(f'<text x="820" y="378" fill="{GOLD}" font-size="11" font-weight="700">① 调度指令(K8s Job/Queue)</text>')
    L.append(f'<text x="820" y="394" fill="{ACCENT}" font-size="11" font-weight="700">② app-server JSON-RPC</text>')
    # 执行平面大框
    L.append(f'<rect x="40" y="400" width="1520" height="270" rx="14" fill="{PANEL}" stroke="{GOLD}" stroke-width="2"/>')
    L.append(f'<rect x="40" y="400" width="1520" height="34" rx="14" fill="url(#gg)"/>')
    L.append(f'<rect x="40" y="420" width="1520" height="14" fill="url(#gg)"/>')
    L.append(f'<text x="60" y="423" fill="#e8f2f0" font-size="14" font-weight="700">执行平面（一次性 · 单租户单任务 · 无状态 · 可销毁）</text>')
    # Sandbox Pod
    L.append(f'<rect x="80" y="450" width="1440" height="200" rx="10" fill="#1a2210" stroke="{RED}" stroke-width="1.6" stroke-dasharray="6,4"/>')
    L.append(f'<text x="100" y="475" fill="#fecaca" font-size="12.5" font-weight="700">Sandbox Pod（NetworkPolicy: 默认全禁出站）</text>')
    boxes=[("codex app-server\nconfig.toml+execpolicy\n运行时注入",80,490,"#13242e",ACCENT),
           ("Workspace\ngit worktree\n/PVC 快照",360,490,PANEL2,BLUE),
           ("MCP Gateway 侧车\n凭据注入·工具白名单\n出站代理·审计脱敏",640,490,PANEL2,PURPLE),
           ("凭据代理\n短期令牌(JWT)\n绑定 tenant+thread+turn",920,490,PANEL2,GOLD),
           ("OS 沙箱(自带)\nSeatbelt/Landlock\n+seccomp+bwrap",1200,490,PANEL2,RED)]
    for n,x,yy,fl,c in boxes:
        node(L,x,yy,260,80,"",fill=fl,stroke=c)
        for k,ln in enumerate(n.split("\n")):
            L.append(f'<text x="{x+130}" y="{yy+22+k*16}" text-anchor="middle" fill="{TEXT}" font-size="11" font-weight="700">{ln}</text>')
    # 出站白名单
    L.append(f'<rect x="80" y="595" width="1440" height="40" rx="6" fill="{PANEL2}" stroke="{ACCENT2}"/>')
    L.append(f'<text x="800" y="620" text-anchor="middle" fill="{ACCENT}" font-size="12" font-weight="700">出站仅放行 → Model Gateway 与 MCP Gateway 两个白名单地址（其余全 deny）</text>')
    cap(L,"图 2 · 控制平面 / 执行平面物理切分（三重取证：网络策略 + 独立密钥 + 独立存储前缀）",660,W)
    fin(L,"control-execution-plane.svg")

fig2()

# ---- 图3 一次任务 13 步生命周期 ----
def fig3():
    W,H=1600,520; L=open_svg(W,H)
    title(L,"Nexus 一次任务的完整生命周期（13 步）","★ 为云端落库点；事件即真相，先落库后推送可回放",40)
    steps=["① 客户端提交\n幂等键","② 鉴权+策略\n求值+预扣","③ 调度沙箱Pod\n注入配置令牌",
           "④ 启动app-server\n下发rollout恢复","⑤ 模型采样\n出站只到Gateway","⑥ 事件流回吐\nturn/item/delta",
           "⑦ 控制面消费\n★写Postgres+推WS","⑧ 审批请求★\n落ApprovalTicket","⑨ 用户决策\n先落库再回写",
           "⑩ 工具执行\nexecpolicy+沙箱","⑪ 上下文将满\nauto compact","⑫ turn完成★\n产物+rollout上传","⑬ 用量结算\n+审计→Pod销毁"]
    y=110
    for i,s in enumerate(steps):
        col=i%7; row=i//7
        x=80+col*215
        yy=y+row*180
        star = "★" in s
        c = GOLD if star else ACCENT2
        node(L,x,yy,200,72,"",fill=PANEL2,stroke=c)
        for k,ln in enumerate(s.split("\n")):
            L.append(f'<text x="{x+100}" y="{yy+26+k*17}" text-anchor="middle" fill="{GOLD if star else TEXT}" font-size="11" font-weight="700">{ln}</text>')
        if col<6 and row==0:
            edge(L,x+200,yy+36,x+215,yy+36,w=1.5)
        if col<6 and row==1:
            edge(L,x+200,yy+36,x+215,yy+36,w=1.5)
    # 横向衔接 7->8（换行）
    edge(L,80+6*215+200,y+36,80+6*215+215,y+36,w=1.5)
    L.append(f'<text x="1330" y="{y+30}" fill="{GOLD}" font-size="11" font-weight="700">换行→</text>')
    # 图例
    L.append(f'<rect x="80" y="{y+200}" width="20" height="14" fill="{PANEL2}" stroke="{GOLD}"/>')
    L.append(f'<text x="110" y="{y+212}" fill="{MUTED}" font-size="11">★ 云端落库点（会话真相写入点）</text>')
    L.append(f'<rect x="350" y="{y+200}" width="20" height="14" fill="{PANEL2}" stroke="{ACCENT2}"/>')
    L.append(f'<text x="380" y="{y+212}" fill="{MUTED}" font-size="11">执行/控制面动作</text>')
    cap(L,"图 3 · 13 步任务生命周期（Pod 随时可死，云端状态可重建 resume）",y+240,W)
    fin(L,"task-lifecycle.svg")

fig3()
print("DONE")
