#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""生成 Nexus 核心组件模块分层依赖图 SVG（深色主题，archify 风格）。
产出：module-layering.svg（八层垂直堆叠 + 层内模块横向 + 跨层依赖箭头）
转 PNG: rsvg-convert -w 2400 module-layering.svg -o module-layering.png
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
    L.append(f'<marker id="ap" viewBox="0 0 10 10" refX="9" refY="5" markerWidth="8" markerHeight="8" orient="auto"><path d="M0 0 L10 5 L0 10 z" fill="{PURPLE}"/></marker>')
    L.append(f'<linearGradient id="gt" x1="0" y1="0" x2="1" y2="1"><stop offset="0" stop-color="{ACCENT2}"/><stop offset="1" stop-color="{ACCENT}"/></linearGradient>')
    L.append(f'<linearGradient id="gg" x1="0" y1="0" x2="1" y2="1"><stop offset="0" stop-color="#b07d1a"/><stop offset="1" stop-color="{GOLD}"/></linearGradient>')
    L.append(f'<linearGradient id="gb" x1="0" y1="0" x2="1" y2="1"><stop offset="0" stop-color="#1e3a8a"/><stop offset="1" stop-color="{BLUE}"/></linearGradient>')
    L.append(f'<linearGradient id="gp" x1="0" y1="0" x2="1" y2="1"><stop offset="0" stop-color="#6b21a8"/><stop offset="1" stop-color="{PURPLE}"/></linearGradient>')
    L.append(f'<linearGradient id="gr" x1="0" y1="0" x2="1" y2="1"><stop offset="0" stop-color="#7f1d1d"/><stop offset="1" stop-color="{RED}"/></linearGradient>')
    L.append('</defs>')
    L.append(f'<rect width="{w}" height="{h}" fill="{BG}"/>')
    return L

def title(L,t,s,y=32):
    L.append(f'<text x="40" y="{y}" fill="{ACCENT}" font-size="22" font-weight="700">{t}</text>')
    if s: L.append(f'<text x="40" y="{y+24}" fill="{MUTED}" font-size="13">{s}</text>')

def mod(L,x,y,w,h,name,crate,fill=PANEL2,stroke=LINE2,fg=TEXT,mg=MUTED):
    """Draw a module box with name + crate name."""
    L.append(f'<rect x="{x}" y="{y}" width="{w}" height="{h}" rx="6" fill="{fill}" stroke="{stroke}" stroke-width="1"/>')
    L.append(f'<text x="{x+w/2}" y="{y+h/2-2}" text-anchor="middle" fill="{fg}" font-size="11" font-weight="700">{name}</text>')
    L.append(f'<text x="{x+w/2}" y="{y+h/2+12}" text-anchor="middle" fill="{mg}" font-size="8.5" font-style="italic">{crate}</text>')

def edge(L,x1,y1,x2,y2,color=ACCENT,m="a",dash=None,w=1.4,curve=False):
    if curve:
        mx=(x1+x2)/2; d=f'M{x1} {y1} C {mx} {y1}, {mx} {y2}, {x2} {y2}'
    else:
        d=f'M{x1} {y1} L{x2} {y2}'
    dd=f' stroke-dasharray="{dash}"' if dash else ''
    L.append(f'<path d="{d}" fill="none" stroke="{color}" stroke-width="{w}" marker-end="url(#{m})"{dd}/>')

def cap(L,t,y,w=3200):
    L.append(f'<text x="40" y="{y}" fill="{MUTED}" font-size="11.5">{t}</text>')

def fin(L,fn):
    L.append('</svg>')
    s='\n'.join(L)
    s=s.replace("&","&amp;")
    open(os.path.join(OUT,fn),'w').write(s); print("wrote",fn)

# ============================================================
# 图1: 八层模块依赖总图（核心大图）
# ============================================================
def fig_main():
    W,H=3200,2400; L=open_svg(W,H)
    title(L,"Nexus 企业级 AI Agent 平台 · 核心组件模块分层依赖图",
          "八层垂直堆叠 · 层内模块横向排列 · 跨层箭头表示依赖方向 · L5 复用 Codex Harness 106 crate（黑盒）",40)

    # ---- 层绘制函数 ----
    def layer_band(y,h,label,subtitle,grad,fg_header="#e8f2f0"):
        L.append(f'<rect x="40" y="{y}" width="3120" height="{h}" rx="10" fill="{PANEL}" stroke="{LINE2}" stroke-width="1.2"/>')
        L.append(f'<rect x="40" y="{y}" width="200" height="{h}" rx="10" fill="{grad}"/>')
        L.append(f'<rect x="220" y="{y}" width="20" height="{h}" fill="{grad}"/>')
        L.append(f'<text x="130" y="{y+h//2-8}" text-anchor="middle" fill="{fg_header}" font-size="14" font-weight="700">{label}</text>')
        L.append(f'<text x="130" y="{y+h//2+12}" text-anchor="middle" fill="{fg_header}" font-size="9.5" opacity="0.85">{subtitle}</text>')

    def layer_modules(y,modules,start_x=260,gap=12):
        """modules: list of (name, crate, stroke_color)"""
        mw=180; mh=52
        x=start_x
        for name,crate,sc in modules:
            mod(L,x,y+12,mw,mh,name,crate,fill=PANEL2,stroke=sc)
            x+=mw+gap

    # ==================== L1 接入层 ====================
    y1=100
    layer_band(y1,76,"L1 接入层","Access","url(#gb)")
    layer_modules(y1,[
        ("Web 门户","React+WS",BLUE),
        ("IM Bot","飞书/钉钉/企微",BLUE),
        ("IDE 插件","VS Code/JB",BLUE),
        ("OpenAPI","REST+Webhook",BLUE),
        ("CLI","codex+自定义登录",BLUE),
        ("Webhook 回调","REST",BLUE),
    ])
    y1e=y1+76

    # ==================== L2 网关层 ====================
    y2=y1e+28
    layer_band(y2,76,"L2 网关层","Gateway","url(#gb)")
    layer_modules(y2,[
        ("API Gateway","REST 路由+幂等",BLUE),
        ("WS 网关","事件推送+权限",BLUE),
        ("认证中间件","OIDC/SAML/SCIM",BLUE),
        ("限流引擎","租户+用户+IP",BLUE),
        ("配额预扣","粗粒度拦截",BLUE),
    ])

    # L1->L2 箭头
    edge(L,1600,y1+76,1600,y2,ACCENT,w=2.5)
    L.append(f'<text x="1610" y="{y2-4}" fill="{ACCENT}" font-size="10">所有入口经网关</text>')

    y2e=y2+76

    # ==================== L3 控制平面 ====================
    y3=y2e+36
    h3=320
    layer_band(y3,h3,"L3 控制平面","Control Plane","url(#gt)")
    # 7 大子系统，每行排列
    sub_y=y3+20
    sub_modules=[
        ("身份租户","Tenant/OrgUnit/Role",ACCENT),
        ("任务编排","Temporal Workflow",ACCENT),
        ("审批中心","ApprovalTicket HITL",ACCENT),
        ("策略中心","Policy Engine",ACCENT),
        ("配额计费","四维计量+预算",ACCENT),
        ("连接器治理","MCP 分级+质量分",ACCENT),
        ("知识库 RAG","ACL+混合召回",ACCENT),
    ]
    layer_modules(sub_y,sub_modules,gap=16)
    # Row 2: 子模块细节
    sub_y2=sub_y+72
    detail_modules=[
        ("RBAC+ABAC","权限交集四取",ACCENT2),
        ("Workflow 池","可恢复编排",ACCENT2),
        ("IM 卡片推送","跨设备审批",ACCENT2),
        ("execpolicy 生成","Starlark 下发",ACCENT2),
        ("Token 计量","prompt/reasoning",ACCENT2),
        ("MCP Gateway","凭据注入侧车",ACCENT2),
        ("向量检索","pgvector/Milvus",ACCENT2),
    ]
    layer_modules(sub_y2,detail_modules,gap=16)
    # Row 3: 配置生成 + 事件桥接 + 审计
    sub_y3=sub_y2+72
    bridge_modules=[
        ("配置生成器","config.toml+rules",ACCENT2),
        ("事件消费者","at-least-once",ACCENT2),
        ("审计落库","WORM+SIEM",ACCENT2),
        ("用量归因","tenant->model",ACCENT2),
        ("短期令牌签发","JWT 绑定任务",ACCENT2),
        ("凭据吊销","实时失效",ACCENT2),
        ("权限快照","防漂移哈希",ACCENT2),
    ]
    layer_modules(sub_y3,bridge_modules,gap=16)

    # L2->L3
    edge(L,1600,y2e,1600,y3,ACCENT,w=2.5)
    L.append(f'<text x="1610" y="{y3-4}" fill="{ACCENT}" font-size="10">鉴权后调度</text>')
    y3e=y3+h3

    # ==================== L4 执行平面 ====================
    y4=y3e+36
    h4=160
    layer_band(y4,h4,"L4 执行平面","Execution","url(#gg)")
    layer_modules(y4+20,[
        ("Runtime 池","K8s 调度+预热",GOLD),
        ("沙箱 Pod","Seccomp/AppArmor",GOLD),
        ("MCP Gateway","侧车凭据注入",GOLD),
        ("Workspace","git worktree/PVC",GOLD),
        ("凭据代理","短期 JWT",GOLD),
        ("健康探针","存活+回收",GOLD),
        ("出站白名单","仅 Gateway 两地址",GOLD),
    ],gap=16)
    # L3->L4 多条箭头
    for x_off in [600,1200,1800,2400]:
        edge(L,x_off,y3e,x_off,y4,ACCENT,w=2.0,dash="6,3")
    L.append(f'<text x="1610" y="{y4-4}" fill="{GOLD}" font-size="10">调度指令 + JSON-RPC + 事件回传</text>')
    y4e=y4+h4

    # ==================== L5 Harness（复用 Codex） ====================
    y5=y4e+36
    h5=460
    layer_band(y5,h5,"L5 Harness","Agent 内核 · 106 crate","url(#gp)")
    L.append(f'<text x="260" y="{y5+16}" fill="{PURPLE}" font-size="10.5" font-weight="700">★ 复用 Codex · 黑盒不改 · 仅薄适配层桥接</text>')

    # Harness 内部模块分组 — 4 行
    # Row 1: 核心引擎 + 协议门面
    h_r1=y5+32
    core_mods=[
        ("Agent Loop","core (run_turn 七阶段)",PURPLE),
        ("门面 API","core-api",PURPLE),
        ("插件系统","core-plugins",PURPLE),
        ("协议层","protocol",PURPLE),
        ("上下文碎片","context-fragments",PURPLE),
        ("提示词","prompts",PURPLE),
        ("App Server","app-server (主集成面)",PURPLE),
        ("Server 协议","app-server-protocol",PURPLE),
    ]
    layer_modules(h_r1,core_mods,gap=14)
    # Row 2: 持久化 + 沙箱
    h_r2=h_r1+72
    persist_mods=[
        ("状态管理","state (SQLite)",PURPLE),
        ("Thread 存储","thread-store",PURPLE),
        ("Rollout","rollout (事件回放)",PURPLE),
        ("历史记录","history",PURPLE),
        ("沙箱总控","sandboxing",PURPLE),
        ("Linux 沙箱","linux-sandbox",PURPLE),
        ("Bubblewrap","bwrap",PURPLE),
        ("Windows 沙箱","windows-sandbox-rs",PURPLE),
        ("执行策略","execpolicy (Starlark)",PURPLE),
    ]
    layer_modules(h_r2,persist_mods,gap=12)
    # Row 3: 工具/技能/模型
    h_r3=h_r2+72
    tool_mods=[
        ("MCP 客户端","codex-mcp",PURPLE),
        ("RMCP 客户端","rmcp-client",PURPLE),
        ("Skills","skills",PURPLE),
        ("Hooks","hooks",PURPLE),
        ("Tools","tools (ToolRouter)",PURPLE),
        ("模型 Provider","model-provider",PURPLE),
        ("Provider Info","model-provider-info",PURPLE),
        ("Responses 代理","responses-api-proxy",PURPLE),
        ("Ollama","ollama",PURPLE),
        ("LM Studio","lmstudio",PURPLE),
    ]
    layer_modules(h_r3,tool_mods,gap=10)
    # Row 4: 协作 + CLI + 可观测
    h_r4=h_r3+72
    collab_mods=[
        ("协作模板","collaboration-mode-templates",PURPLE),
        ("Agent 角色","agent-roles",PURPLE),
        ("Agent 身份","agent-identity",PURPLE),
        ("Agent 图存储","agent-graph-store",PURPLE),
        ("CLI","cli",PURPLE),
        ("TUI","tui",PURPLE),
        ("Codex Client","codex-client",PURPLE),
        ("Exec","exec",PURPLE),
        ("Exec Server","exec-server",PURPLE),
        ("OTel","otel",PURPLE),
        ("Analytics","analytics",PURPLE),
        ("Diagnostics","diagnostics",PURPLE),
        ("Rollout Trace","rollout-trace",PURPLE),
    ]
    layer_modules(h_r4,collab_mods,gap=8)

    # L4->L5 (app-server JSON-RPC)
    edge(L,1600,y4e,1600,y5,GOLD,m="ag",w=2.5)
    L.append(f'<text x="1610" y="{y5-4}" fill="{GOLD}" font-size="10">app-server JSON-RPC · 事件流桥接</text>')
    y5e=y5+h5

    # ==================== L6 模型层 ====================
    y6=y5e+36
    layer_band(y6,76,"L6 模型层","Model","url(#gt)")
    layer_modules(y6,[
        ("Model Gateway","LiteLLM/自建",ACCENT),
        ("多模型路由","分档路由",ACCENT),
        ("Responses 代理","responses-api-proxy",ACCENT),
        ("Token 计量","四维计量",ACCENT),
        ("故障转移","主->备->重试",ACCENT),
        ("Prompt Caching","版本化前缀",ACCENT),
    ])
    # L5->L6 (出站只到 Model Gateway)
    edge(L,1200,y5e,1200,y6,PURPLE,m="ap",w=2.0,dash="5,3")
    L.append(f'<text x="1210" y="{y6-4}" fill="{PURPLE}" font-size="10">出站仅 Model Gateway</text>')
    y6e=y6+76

    # ==================== L7 存储与治理 ====================
    y7=y6e+28
    h7=120
    layer_band(y7,h7,"L7 存储与治理","Storage and Gov.","url(#gb)")
    layer_modules(y7+20,[
        ("Postgres","RLS+分区",BLUE),
        ("对象存储","S3/MinIO+CMK",BLUE),
        ("向量库","pgvector/Milvus",BLUE),
        ("审计日志","WORM+SIEM",BLUE),
        ("OTel 追踪","ClickHouse+Grafana",BLUE),
        ("评测中心","LLM-as-judge",BLUE),
    ])
    # L3->L7 (控制面写库) + L5->L7 (rollout 上传)
    edge(L,1000,y3e,1000,y7,ACCENT,w=1.8,dash="4,3")
    edge(L,2200,y5e,2200,y7,PURPLE,m="ap",w=1.8,dash="4,3")
    y7e=y7+h7

    # ==================== 安全合规贯穿条 ====================
    ys=y7e+28
    L.append(f'<rect x="40" y="{ys}" width="3120" height="42" rx="8" fill="{PANEL2}" stroke="{RED}" stroke-width="1.4"/>')
    L.append(f'<text x="1600" y="{ys+27}" text-anchor="middle" fill="#fecaca" font-size="12.5" font-weight="700">贯穿 · 安全与合规：租户隔离(四重取证) · KMS(按租户CMK) · 网络策略 · 审计留存 · 内容安全 · 红队演练</text>')

    cap(L,"图 1 · 八层模块分层依赖总图 — L1-L3/L6-L7 自建(蓝/绿)，L4 薄壳(金)，L5 复用 Codex Harness 106 crate(紫，黑盒不改)，安全合规贯穿(红)",ys+70,W)

    fin(L,"module-layering.svg")

fig_main()

# ============================================================
# 图2: L5 Harness 内部 crate 依赖拓扑（聚焦图）
# ============================================================
def fig_l5_detail():
    W,H=3200,700; L=open_svg(W,H)
    title(L,"L5 Harness · 内部 crate 依赖拓扑（复用 Codex 106 crate）",
          "按功能域分组 · 箭头表示编译期依赖 · 紫色=核心 · 金色=持久化/沙箱 · 青色=工具/模型 · 蓝色=执行CLI",40)

    def grp(x,y,w,h,title_str,stroke):
        L.append(f'<rect x="{x}" y="{y}" width="{w}" height="{h}" rx="10" fill="{PANEL}" stroke="{stroke}" stroke-width="1.6" opacity="0.95"/>')
        L.append(f'<rect x="{x}" y="{y}" width="{w}" height="26" rx="10" fill="{stroke}" opacity="0.3"/>')
        L.append(f'<text x="{x+w/2}" y="{y+18}" text-anchor="middle" fill="{TEXT}" font-size="12" font-weight="700">{title_str}</text>')

    def crate_mod(x,y,name,stroke,w=170,h=40):
        L.append(f'<rect x="{x}" y="{y}" width="{w}" height="{h}" rx="5" fill="{PANEL2}" stroke="{stroke}" stroke-width="1"/>')
        L.append(f'<text x="{x+w/2}" y="{y+h/2+4}" text-anchor="middle" fill="{TEXT}" font-size="10" font-weight="700">{name}</text>')

    # ---- 核心引擎组 ----
    gx=80; gy=100; gw=680; gh=340
    grp(gx,gy,gw,gh,"核心引擎 (Core Engine)",PURPLE)
    crates_core=[
        ("core (run_turn)",80,140),
        ("core-api",280,140),
        ("core-plugins",480,140),
        ("protocol",80,200),
        ("context-fragments",280,200),
        ("prompts",480,200),
        ("app-server",80,260),
        ("app-server-protocol",280,260),
        ("app-server-client",480,260),
        ("app-server-transport",80,320),
        ("app-server-daemon",280,320),
        ("codex-api",480,320),
    ]
    for n,dx,dy in crates_core:
        crate_mod(gx+dx,gy+dy,n,PURPLE)

    # ---- 持久化组 ----
    px=800; py=100; pw=500; ph=340
    grp(px,py,pw,ph,"持久化与沙箱 (Persist+Sandbox)",GOLD)
    crates_persist=[
        ("state (SQLite)",80,140),
        ("thread-store",280,140),
        ("rollout",80,200),
        ("history",280,200),
        ("sandboxing",80,260),
        ("linux-sandbox",280,260),
        ("bwrap",80,320),
        ("windows-sandbox-rs",280,320),
        ("execpolicy (Starlark)",80,380),
    ]
    for n,dx,dy in crates_persist:
        crate_mod(px+dx,py+dy,n,GOLD)

    # ---- 工具/技能/模型组 ----
    tx=1360; ty=100; tw=760; th=340
    grp(tx,ty,tw,th,"工具·技能·模型 (Tools/Skills/Models)",ACCENT)
    crates_tools=[
        ("codex-mcp",80,140),
        ("rmcp-client",280,140),
        ("skills",480,140),
        ("hooks",80,200),
        ("tools (ToolRouter)",280,200),
        ("model-provider",480,200),
        ("model-provider-info",80,260),
        ("responses-api-proxy",280,260),
        ("ollama",480,260),
        ("lmstudio",80,320),
        ("models-manager",280,320),
        ("connectors",480,320),
    ]
    for n,dx,dy in crates_tools:
        crate_mod(tx+dx,ty+dy,n,ACCENT)

    # ---- 协作组 ----
    cx=2180; cy=100; cw=500; ch=200
    grp(cx,cy,cw,ch,"多 Agent 协作 (Collaboration)",PURPLE)
    crates_collab=[
        ("collaboration-mode-templates",80,140),
        ("agent-roles",80,200),
        ("agent-identity",280,200),
    ]
    for n,dx,dy in crates_collab:
        crate_mod(cx+dx,cy+dy,n,PURPLE)

    # ---- 执行CLI组 ----
    ex=2180; ey=340; ew=500; eh=200
    grp(ex,ey,ew,eh,"执行与 CLI (Exec/CLI)",BLUE)
    crates_exec=[
        ("cli",80,380),
        ("tui",280,380),
        ("codex-client",80,440),
        ("exec",280,440),
        ("exec-server",80,500),
        ("exec-server-protocol",280,500),
    ]
    for n,dx,dy in crates_exec:
        crate_mod(ex+dx,ey+dy-340+340,n,BLUE)

    # ---- 可观测组 ----
    ox=2750; oy=100; ow=400; oh=200
    grp(ox,oy,ow,oh,"可观测 (Observability)",ACCENT)
    crates_obs=[
        ("otel",80,140),
        ("analytics",80,200),
        ("diagnostics",280,200),
    ]
    for n,dx,dy in crates_obs:
        crate_mod(ox+dx,oy+dy,n,ACCENT)

    # ---- 依赖箭头 (关键路径) ----
    # core -> app-server-protocol
    edge(L,750,160,800,165,PURPLE,w=1.5)
    # core -> state
    edge(L,750,175,800,165,GOLD,m="ag",w=2.0)
    # core -> sandboxing
    edge(L,750,200,800,285,GOLD,m="ag",w=1.8)
    # core -> codex-mcp
    edge(L,1350,175,1360,165,ACCENT,w=2.0)
    # core -> tools
    edge(L,1350,200,1360,225,ACCENT,w=1.8)
    # core -> model-provider
    edge(L,1350,175,1360,255,ACCENT,m="a",w=1.5,dash="4,2")
    # core -> agent-roles
    edge(L,1350,175,2180,225,PURPLE,m="ap",w=1.5,dash="6,3")
    # core -> otel
    edge(L,1350,175,2750,165,ACCENT,w=1.5,dash="6,3")
    # execpolicy -> linux-sandbox
    edge(L,900,400,900,330,GOLD,m="ag",w=1.5)

    # Legend
    lx=80; ly=560
    L.append(f'<rect x="{lx}" y="{ly}" width="20" height="14" fill="{PANEL2}" stroke="{PURPLE}"/>')
    L.append(f'<text x="{lx+28}" y="{ly+12}" fill="{MUTED}" font-size="11">核心引擎 crate</text>')
    L.append(f'<rect x="{lx+180}" y="{ly}" width="20" height="14" fill="{PANEL2}" stroke="{GOLD}"/>')
    L.append(f'<text x="{lx+208}" y="{ly+12}" fill="{MUTED}" font-size="11">持久化/沙箱 crate</text>')
    L.append(f'<rect x="{lx+380}" y="{ly}" width="20" height="14" fill="{PANEL2}" stroke="{ACCENT}"/>')
    L.append(f'<text x="{lx+408}" y="{ly+12}" fill="{MUTED}" font-size="11">工具/模型 crate</text>')
    L.append(f'<rect x="{lx+560}" y="{ly}" width="20" height="14" fill="{PANEL2}" stroke="{BLUE}"/>')
    L.append(f'<text x="{lx+588}" y="{ly+12}" fill="{MUTED}" font-size="11">执行/CLI crate</text>')

    cap(L,"图 2 · L5 Harness 内部 106 crate 依赖拓扑 — 按功能域分组，箭头表示关键编译期依赖路径",ly+50,W)
    fin(L,"harness-crate-topology.svg")

fig_l5_detail()

# ============================================================
# 图3: 控制面自建模块详细设计图
# ============================================================
def fig_control_plane():
    W,H=3200,560; L=open_svg(W,H)
    title(L,"L3 控制平面 · 七大自建子系统详细设计",
          "身份租户 / 任务编排 / 审批中心 / 策略中心 / 配额计费 / 连接器治理 / 知识库 RAG",40)

    def subsystem(x,y,w,h,title_str,stroke):
        L.append(f'<rect x="{x}" y="{y}" width="{w}" height="{h}" rx="10" fill="{PANEL}" stroke="{stroke}" stroke-width="1.6"/>')
        L.append(f'<rect x="{x}" y="{y}" width="{w}" height="28" rx="10" fill="{stroke}" opacity="0.25"/>')
        L.append(f'<text x="{x+w/2}" y="{y+20}" text-anchor="middle" fill="{TEXT}" font-size="12.5" font-weight="700">{title_str}</text>')

    def sub_mod(x,y,w,h,name,detail,stroke):
        L.append(f'<rect x="{x}" y="{y}" width="{w}" height="{h}" rx="5" fill="{PANEL2}" stroke="{stroke}" stroke-width="1"/>')
        L.append(f'<text x="{x+w/2}" y="{y+h/2-2}" text-anchor="middle" fill="{TEXT}" font-size="10" font-weight="700">{name}</text>')
        L.append(f'<text x="{x+w/2}" y="{y+h/2+11}" text-anchor="middle" fill="{MUTED}" font-size="8">{detail}</text>')

    bw=440; bh=220; gap=30
    start_x=80; start_y=100

    # Row 1: 4 subsystems
    positions_r1=[
        (start_x,"身份与租户",ACCENT),
        (start_x+(bw+gap),"任务编排 (Temporal)",ACCENT),
        (start_x+2*(bw+gap),"审批中心 (HITL)",GOLD),
        (start_x+3*(bw+gap),"策略中心",RED),
    ]
    r1_subs={
        "身份与租户":[
            ("Tenant 模型","租户/OrgUnit/Role","RBAC+ABAC"),
            ("Agent 身份","服务账号不等于用户","权限子集"),
            ("连接器身份","MCP/OAuth 委托","按租户隔离"),
            ("权限交集","四取交集","任一空即拒"),
        ],
        "任务编排 (Temporal)":[
            ("Workflow 引擎","可恢复编排","跨小时审批"),
            ("外层循环","平台资源+账本","长周期"),
            ("内层循环","Harness run_turn","短周期"),
            ("调度策略","权重+优先级","并发上限"),
        ],
        "审批中心 (HITL)":[
            ("ApprovalTicket","先落库后回写","跨小时"),
            ("多渠道推送","Web+IM+邮件","按风险选"),
            ("边界处理","Pod 崩/超时/权限","去重重放"),
            ("批量审批","作用域限定","小于等于1h"),
        ],
        "策略中心":[
            ("策略对象","tenant/role/tool","risk_level"),
            ("决策结果","allow/deny/approval","dual_approval"),
            ("求值时机","准入+高危前","不永久通行"),
            ("漂移防护","快照入上下文","新动作新策略"),
        ],
    }
    for x,title_str,stroke in positions_r1:
        subsystem(x,start_y,bw,bh,title_str,stroke)
        mods=r1_subs[title_str]
        for i,(n,d,dd) in enumerate(mods):
            col=i%2; row=i//2
            mx=x+20+col*(bw//2-10)
            my=start_y+40+row*70
            sub_mod(mx,my,bw//2-20,60,n,f"{d} | {dd}",stroke)

    # Row 2: 3 subsystems
    start_y2=start_y+bh+40
    positions_r2=[
        (start_x,"配额与计费",ACCENT),
        (start_x+(bw+gap),"连接器治理",GOLD),
        (start_x+2*(bw+gap),"知识库 / RAG",PURPLE),
    ]
    r2_subs={
        "配额与计费":[
            ("四维计量","token/工具/时长/存储","归因到部门"),
            ("预算控制","软告警降档熔断","不直接杀"),
            ("优雅暂停","保存 rollout","预算恢复 resume"),
            ("成本归因","tenant->model","实时看板"),
        ],
        "连接器治理":[
            ("分级管理","官方/私有/社区","默认禁用"),
            ("MCP Gateway","凭据注入+白名单","出站审计"),
            ("质量分","可用性/P95/错误率","低于阈值下线"),
            ("凭据代理","短期 JWT","绑定 tenant+turn"),
        ],
        "知识库 / RAG":[
            ("ACL 随索引","chunk 携带 tenant+acl","先过滤后召回"),
            ("混合召回","稠密+稀疏+rerank","附 chunk_id"),
            ("权限强制","Gateway 侧校验","不依赖模型"),
            ("知识注入","MCP 工具/自定义 Tool","权限在 Gateway"),
        ],
    }
    for x,title_str,stroke in positions_r2:
        subsystem(x,start_y2,bw,bh,title_str,stroke)
        mods=r2_subs[title_str]
        for i,(n,d,dd) in enumerate(mods):
            col=i%2; row=i//2
            mx=x+20+col*(bw//2-10)
            my=start_y2+40+row*70
            sub_mod(mx,my,bw//2-20,60,n,f"{d} | {dd}",stroke)

    # 跨子系统依赖箭头
    edge(L,start_x+bw,start_y+bh//2,start_x+bw+gap,start_y+bh//2,ACCENT,w=1.5)
    edge(L,start_x+2*(bw+gap)+bw,start_y+bh//2,start_x+3*(bw+gap),start_y+bh//2,GOLD,m="ag",w=1.5)
    edge(L,start_x+(bw+gap)+bw//2,start_y+bh,start_x+2*(bw+gap)+bw//2,start_y2+bh//2,GOLD,m="ag",w=1.5,curve=True)
    edge(L,start_x+2*(bw+gap),start_y+bh,start_x+bw//2,start_y2,ACCENT,w=1.5,curve=True)
    edge(L,start_x+(bw+gap)+bw,start_y2+bh//2,start_x+2*(bw+gap),start_y2+bh//2,PURPLE,m="ap",w=1.5)

    cap(L,"图 3 · L3 控制平面七大自建子系统 — 箭头表示跨子系统依赖（身份->策略->审批、编排->审批->计费、连接器->知识库）",start_y2+bh+40,W)
    fin(L,"control-plane-subsystems.svg")

fig_control_plane()

print("DONE")
