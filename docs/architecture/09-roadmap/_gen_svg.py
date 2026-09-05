#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""生成 Nexus 开发计划 RoadMap SVG（深色主题，archify 风格）。
产出：roadmap-gantt.svg（阶段甘特）+ roadmap-milestones.svg（里程碑路线）+ roadmap-critical-path.svg（关键路径）
转 PNG: rsvg-convert -w 2400 roadmap-gantt.svg -o roadmap-gantt.png
"""
import os
OUT = os.path.dirname(os.path.abspath(__file__))
BG="#0b141a"; PANEL="#13242e"; PANEL2="#173341"; TEXT="#e8f2f0"; MUTED="#9db8b4"
LINE="#24414e"; LINE2="#2e5a68"; ACCENT="#35c2b0"; ACCENT2="#028090"; GOLD="#e8b64c"
BLUE="#3b82f6"; RED="#ef4444"; PURPLE="#a855f7"; GREEN="#22c55e"
FONT="'Helvetica Neue',Helvetica,Arial,'PingFang SC','Microsoft YaHei','SimHei',sans-serif"

def open_svg(w,h):
    L=[f'<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 {w} {h}">']
    L.append(f'<style>text{{font-family:{FONT}}}</style>')
    L.append('<defs>')
    L.append(f'<marker id="a" viewBox="0 0 10 10" refX="9" refY="5" markerWidth="8" markerHeight="8" orient="auto"><path d="M0 0 L10 5 L0 10 z" fill="{ACCENT}"/></marker>')
    L.append(f'<marker id="ag" viewBox="0 0 10 10" refX="9" refY="5" markerWidth="8" markerHeight="8" orient="auto"><path d="M0 0 L10 5 L0 10 z" fill="{GOLD}"/></marker>')
    L.append(f'<marker id="ar" viewBox="0 0 10 10" refX="9" refY="5" markerWidth="8" markerHeight="8" orient="auto"><path d="M0 0 L10 5 L0 10 z" fill="{RED}"/></marker>')
    L.append(f'<linearGradient id="gt" x1="0" y1="0" x2="1" y2="0"><stop offset="0" stop-color="{ACCENT2}"/><stop offset="1" stop-color="{ACCENT}"/></linearGradient>')
    L.append(f'<linearGradient id="gg" x1="0" y1="0" x2="1" y2="0"><stop offset="0" stop-color="#b07d1a"/><stop offset="1" stop-color="{GOLD}"/></linearGradient>')
    L.append(f'<linearGradient id="gb" x1="0" y1="0" x2="1" y2="0"><stop offset="0" stop-color="#1e3a8a"/><stop offset="1" stop-color="{BLUE}"/></linearGradient>')
    L.append(f'<linearGradient id="gp" x1="0" y1="0" x2="1" y2="0"><stop offset="0" stop-color="#6b21a8"/><stop offset="1" stop-color="{PURPLE}"/></linearGradient>')
    L.append(f'<linearGradient id="gr" x1="0" y1="0" x2="1" y2="0"><stop offset="0" stop-color="#991b1b"/><stop offset="1" stop-color="{RED}"/></linearGradient>')
    L.append(f'<linearGradient id="gn" x1="0" y1="0" x2="1" y2="0"><stop offset="0" stop-color="#15803d"/><stop offset="1" stop-color="{GREEN}"/></linearGradient>')
    L.append('</defs>')
    L.append(f'<rect width="{w}" height="{h}" fill="{BG}"/>')
    return L

def title(L,t,s,y=34):
    L.append(f'<text x="40" y="{y}" fill="{ACCENT}" font-size="22" font-weight="700">{t}</text>')
    if s: L.append(f'<text x="40" y="{y+24}" fill="{MUTED}" font-size="13">{s}</text>')

def cap(L,t,y,w=1600):
    L.append(f'<text x="40" y="{y}" fill="{MUTED}" font-size="11.5">{t}</text>')

def fin(L,fn):
    L.append('</svg>')
    s='\n'.join(L)
    s=s.replace("&","&amp;")
    open(os.path.join(OUT,fn),'w').write(s); print("wrote",fn)

# ---- 图1 阶段甘特图（M0-M12）----
def fig1():
    W,H=1600,1180; L=open_svg(W,H)
    title(L,"Nexus 开发计划 · 阶段甘特图（M0–M12，约 13 个月）","四阶段：① PoC(M0,4周)  ② 单租户MVP(M1–M4)  ③ 多租户与隔离(M5–M7)  ④ 可靠性与治理(M8–M10)  ⑤ 规模化与生态(M11–M12+)",34)
    # 时间轴
    tx=320; tw=1180  # 时间轴区域 x=320..1500
    months=list(range(0,13))  # M0..M12
    col_w=tw/len(months)
    # 阶段背景色块
    phases=[(0,0,"PoC","#15803d",GREEN),(1,4,"单租户 MVP",ACCENT2,ACCENT),(5,7,"多租户与隔离","#b07d1a",GOLD),(8,10,"可靠性与治理","#6b21a8",PURPLE),(11,12,"规模化与生态","#1e3a8a",BLUE)]
    for s,e,nm,_,c in phases:
        x0=tx+s*col_w; x1=tx+(e+1)*col_w
        L.append(f'<rect x="{x0}" y="80" width="{x1-x0}" height="980" fill="{c}" opacity="0.06"/>')
        L.append(f'<rect x="{x0}" y="80" width="{x1-x0}" height="26" fill="{c}" opacity="0.35" rx="4"/>')
        L.append(f'<text x="{(x0+x1)/2}" y="98" text-anchor="middle" fill="{TEXT}" font-size="12" font-weight="700">{nm}</text>')
    # 月份刻度
    for i,m in enumerate(months):
        x=tx+i*col_w+col_w/2
        L.append(f'<text x="{x}" y="120" text-anchor="middle" fill="{MUTED}" font-size="11">M{m}</text>')
        L.append(f'<line x1="{tx+i*col_w}" y1="125" x2="{tx+i*col_w}" y2="1080" stroke="{LINE}" stroke-width="0.6" opacity="0.5"/>')
    # 任务条（按层分组，每模块一行）
    # (layer, name, start_month_idx, end_month_idx, color, priority)
    tasks=[
        ("L5","Agent Loop（core 复用）",0,0,ACCENT,"P0"),
        ("L5","OS 沙箱（复用）",0,0,ACCENT,"P0"),
        ("L5","协议集成面 app-server",0,0,ACCENT,"P0"),
        ("L4","三层沙箱容器层",0,2,RED,"P0"),
        ("L8","网络策略默认全禁",0,2,RED,"P0"),
        ("L3","身份租户（单租户）",1,1,ACCENT,"P0"),
        ("L2","API Gateway",1,1,ACCENT,"P0"),
        ("L2","WS 网关",1,2,ACCENT,"P0"),
        ("L2","认证中间件 OIDC",1,1,ACCENT,"P0"),
        ("L1","Web 门户骨架",1,2,ACCENT,"P0"),
        ("L1","CLI 登录层",1,1,GOLD,"P1"),
        ("L4","Runtime 池调度",2,2,ACCENT,"P0"),
        ("L4","Workspace 供给",2,2,ACCENT,"P0"),
        ("L3","任务编排 Temporal",2,2,ACCENT,"P0"),
        ("L1","OpenAPI+Webhook",2,2,ACCENT,"P0"),
        ("L6","Model Gateway",2,2,ACCENT,"P0"),
        ("L6","Responses 代理",2,2,ACCENT,"P0"),
        ("L7","对象存储（按租户前缀）",2,2,ACCENT,"P0"),
        ("L2","配额预扣",2,2,ACCENT,"P0"),
        ("L7","Postgres 主库",1,1,ACCENT,"P0"),
        ("L3","审批中心 HITL",3,3,ACCENT,"P0"),
        ("L3","策略中心 execpolicy 下发",3,3,ACCENT,"P0"),
        ("L1","IM Bot 飞书/钉钉",3,3,ACCENT,"P0"),
        ("L3","配额计费（单租户）",4,4,ACCENT,"P0"),
        ("L7","OTel 基础",4,4,GOLD,"P1"),
        ("L1","IDE 插件",4,4,GOLD,"P1"),
        ("L5","Skills/Hooks",4,4,GOLD,"P1"),
        ("L3","身份租户（多租户RBAC+ABAC）",5,5,GOLD,"P0"),
        ("L8","四重隔离取证",5,5,RED,"P0"),
        ("L8","KMS 按租户 CMK",5,5,GOLD,"P1"),
        ("L7","Postgres RLS 策略",5,5,RED,"P0"),
        ("L3","配额计费（多租户归因）",6,6,GOLD,"P0"),
        ("L4","MCP Gateway 侧车",7,7,GOLD,"P1"),
        ("L3","连接器治理",7,7,GOLD,"P1"),
        ("L4","凭据代理 短期令牌",7,7,GOLD,"P1"),
        ("L7","评测中心",8,8,GOLD,"P1"),
        ("L6","多模型路由",9,9,GOLD,"P1"),
        ("L6","Prompt Caching",9,9,GOLD,"P1"),
        ("L6","故障转移",9,9,ACCENT,"P0"),
        ("L7","OTel 全链路",9,9,GOLD,"P1"),
        ("L7","审计日志 WORM",10,10,GOLD,"P1"),
        ("L8","内容安全 PII/注入防护",10,10,RED,"P1"),
        ("L8","红队演练（首次）",10,10,RED,"P1"),
        ("L4","Runtime 池优化（冷启动＜5s）",11,11,GOLD,"P0"),
        ("L3","知识库/RAG（ACL随索引）",11,11,PURPLE,"P2"),
        ("L7","向量库 pgvector",11,11,PURPLE,"P2"),
        ("L5","协作编排 多Agent",11,12,PURPLE,"P2"),
        ("L6","私有化部署 vLLM/Ollama",12,12,PURPLE,"P2"),
    ]
    y0=140; rh=20
    # 图层分组标签
    layers=[("L1 接入层",ACCENT),("L2 网关层",ACCENT),("L3 控制面",ACCENT),("L4 执行面",GOLD),("L5 Harness（复用）",PURPLE),("L6 模型层",BLUE),("L7 存储治理",GREEN),("L8 安全贯穿",RED)]
    cur=y0
    # 先按出现顺序画
    row=0
    for lt,nm,s,e,c,p in tasks:
        y=cur+row*rh
        # 层标签（每5行左侧）省略，直接画条
        x0=tx+s*col_w+2
        x1=tx+(e+1)*col_w-2
        barw=x1-x0
        if barw<8: barw=col_w-8; x1=x0+barw
        op=1.0 if p=="P0" else 0.75 if p=="P1" else 0.55
        L.append(f'<rect x="{x0}" y="{y+2}" width="{barw}" height="14" rx="3" fill="{c}" opacity="{op}"/>')
        L.append(f'<text x="{x0+6}" y="{y+13}" fill="#fff" font-size="9.5" font-weight="600">{nm}</text>')
        # 左侧任务名（在时间轴外）
        L.append(f'<text x="312" y="{y+13}" text-anchor="end" fill="{TEXT}" font-size="10">{nm}</text>')
        row+=1
    # 里程碑菱形（关键验收门）
    ms=[(0,"M0 PoC","三大假设验证",GREEN),(4,"M4 MVP","P0完成率≥70%",ACCENT),(7,"M7 隔离","跨租户越权0通过",GOLD),(10,"M10 治理","任务成功率≥85%",PURPLE),(12,"M12 GA","100+并发稳定",BLUE)]
    for i,nm,desc,c in ms:
        x=tx+(i+1)*col_w
        L.append(f'<path d="M {x} 1060 l 10 12 l -10 12 l -10 -12 z" fill="{c}" stroke="{TEXT}" stroke-width="0.8"/>')
        L.append(f'<text x="{x}" y="1100" text-anchor="middle" fill="{c}" font-size="11" font-weight="700">{nm}</text>')
        L.append(f'<text x="{x}" y="1114" text-anchor="middle" fill="{MUTED}" font-size="9.5">{desc}</text>')
    # 图例
    L.append(f'<text x="40" y="1145" fill="{MUTED}" font-size="11">■ 不透明度=优先级：P0(实) P1(0.75) P2(0.55)  ·  颜色=所属层  ·  ◆ 里程碑验收门</text>')
    cap(L,"图 1 · Nexus 阶段甘特图（46 模块按 M0–M12 排布，5 大里程碑验收门）",1162,W)
    fin(L,"roadmap-gantt.svg")

fig1()

# ---- 图2 里程碑路线图 ----
def fig2():
    W,H=1600,560; L=open_svg(W,H)
    title(L,"Nexus 里程碑路线图 · 五阶段五验收门","每阶段以可验证的退出条件为门禁，未达标不进入下一阶段",34)
    # 主线
    L.append(f'<line x1="100" y1="200" x2="1500" y2="200" stroke="{LINE2}" stroke-width="3"/>')
    ms=[(160,"M0","PoC","三大假设验证\n①app-server长会话可恢复\n②execpolicy规则可下发\n③三层沙箱生效",GREEN,"4周"),
        (430,"M1-M4","单租户 MVP","P0模块完成率≥70%\n端到端任务跑通\n会话落库可resume",ACCENT,"4个月"),
        (700,"M5-M7","多租户隔离","四重隔离取证\n跨租户越权0通过\nKMS+RLS生效",GOLD,"3个月"),
        (970,"M8-M10","可靠性治理","任务成功率≥85%\n评测CI门禁\n审计WORM+红队",PURPLE,"3个月"),
        (1240,"M11-M12+","规模化生态","100+并发稳定\n冷启动＜5s\n知识库/协作/私有化",BLUE,"2个月+")]
    for x,no,nm,desc,c,dur in ms:
        L.append(f'<circle cx="{x}" cy="200" r="22" fill="{BG}" stroke="{c}" stroke-width="3"/>')
        L.append(f'<text x="{x}" y="206" text-anchor="middle" fill="{c}" font-size="13" font-weight="700">{no}</text>')
        # 上方阶段名+周期
        L.append(f'<rect x="{x-80}" y="100" width="160" height="50" rx="8" fill="{PANEL2}" stroke="{c}" stroke-width="1.2"/>')
        L.append(f'<text x="{x}" y="122" text-anchor="middle" fill="{c}" font-size="14" font-weight="700">{nm}</text>')
        L.append(f'<text x="{x}" y="140" text-anchor="middle" fill="{MUTED}" font-size="11">周期 {dur}</text>')
        L.append(f'<line x1="{x}" y1="150" x2="{x}" y2="178" stroke="{c}" stroke-width="1.6" marker-end="url(#a)"/>')
        # 下方退出条件
        for k,ln in enumerate(desc.split("\n")):
            L.append(f'<text x="{x}" y="{246+k*18}" text-anchor="middle" fill="{TEXT}" font-size="11">{ln}</text>')
        # 累计模块数
    cum=[(160,"复用3模块"),(430,"+23 P0模块"),(700,"+10 P1模块"),(970,"+8 P1模块"),(1240,"+6 P2模块")]
    for x,t in cum:
        L.append(f'<text x="{x}" y="340" text-anchor="middle" fill="{MUTED}" font-size="10.5">{t}</text>')
    # 底部累计交付
    L.append(f'<rect x="80" y="380" width="1440" height="120" rx="10" fill="{PANEL}" stroke="{LINE2}"/>')
    L.append(f'<text x="100" y="405" fill="{GOLD}" font-size="12.5" font-weight="700">累计交付能力</text>')
    dels=[("M0","app-server集成·沙箱·策略下发·模型代理"),
          ("M4","Web门户·审批HITL·配额·任务编排·会话云端持久化"),
          ("M7","多租户·四重隔离·KMS·MCP Gateway·凭据代理·连接器治理"),
          ("M10","评测中心·多模型路由·故障转移·审计WORM·内容安全·红队"),
          ("M12","冷启动＜5s·知识库RAG·多Agent协作·私有化部署·生态")]
    for k,(m,d) in enumerate(dels):
        y=425+k*16
        L.append(f'<text x="110" y="{y}" fill="{ACCENT}" font-size="11" font-weight="700">{m}:</text>')
        L.append(f'<text x="160" y="{y}" fill="{TEXT}" font-size="11">{d}</text>')
    cap(L,"图 2 · 五阶段里程碑路线（每阶段验收门未达标不进入下一阶段）",530,W)
    fin(L,"roadmap-milestones.svg")

fig2()

# ---- 图3 关键路径图 ----
def fig3():
    W,H=1600,720; L=open_svg(W,H)
    title(L,"Nexus 关键路径 · 从 PoC 到 MVP 的最长依赖链","红色路径=关键路径（决定 MVP 工期）；任何环节延期=MVP 延期",34)
    # 节点 (id, x, y, label, sub, color)
    nodes=[
        ("n1",120,200,"app-server 协议集成","L5·复用·M0",ACCENT),
        ("n2",360,200,"三层沙箱+网络策略","L4/L8·M0",RED),
        ("n3",600,200,"身份租户+认证","L3/L2·M1",ACCENT),
        ("n4",840,200,"Postgres+会话落库","L7·M1-M2",ACCENT),
        ("n5",1080,200,"Runtime池+Workspace","L4·M2",ACCENT),
        ("n6",1320,200,"任务编排 Temporal","L3·M2",ACCENT),
        ("n7",1320,360,"Model Gateway","L6·M2",ACCENT),
        ("n8",1080,360,"审批中心 HITL","L3·M3",ACCENT),
        ("n9",840,360,"策略中心下发","L3·M3",ACCENT),
        ("n10",600,360,"IM Bot","L1·M3",ACCENT),
        ("n11",360,360,"配额计费","L3·M4",ACCENT),
        ("n12",120,360,"M4 MVP 验收","P0≥70%",GOLD),
    ]
    for nid,x,y,lbl,sub,c in nodes:
        L.append(f'<rect x="{x-95}" y="{y-26}" width="190" height="52" rx="8" fill="{PANEL2}" stroke="{c}" stroke-width="1.6"/>')
        L.append(f'<text x="{x}" y="{y-6}" text-anchor="middle" fill="{TEXT}" font-size="11.5" font-weight="700">{lbl}</text>')
        L.append(f'<text x="{x}" y="{y+12}" text-anchor="middle" fill="{MUTED}" font-size="9.5">{sub}</text>')
    # 关键路径（红色，上方链）
    kp=[("n1","n2"),("n2","n3"),("n3","n4"),("n4","n5"),("n5","n6")]
    for a,b in kp:
        na=[n for n in nodes if n[0]==a][0]; nb=[n for n in nodes if n[0]==b][0]
        L.append(f'<path d="M {na[1]+95} {na[2]} L {nb[1]-95} {nb[2]}" fill="none" stroke="{RED}" stroke-width="2.6" marker-end="url(#ar)"/>')
    # 次要依赖（金色）
    sd=[("n6","n7"),("n7","n8"),("n8","n9"),("n9","n10"),("n10","n11"),("n11","n12"),("n6","n8")]
    for a,b in sd:
        na=[n for n in nodes if n[0]==a][0]; nb=[n for n in nodes if n[0]==b][0]
        # 曲线连接
        x1=na[1]; y1=na[2]; x2=nb[1]; y2=nb[2]
        if y1==y2:
            L.append(f'<path d="M {x1+95} {y1} L {x2-95} {y2}" fill="none" stroke="{GOLD}" stroke-width="1.6" marker-end="url(#ag)"/>')
        else:
            mx=(x1+x2)/2
            L.append(f'<path d="M {x1} {y1+26} C {mx} {y1+60}, {mx} {y2-60}, {x2} {y2-26}" fill="none" stroke="{GOLD}" stroke-width="1.4" stroke-dasharray="5,3" marker-end="url(#ag)"/>')
    # n5->n12 汇合
    L.append(f'<path d="M 1080 374 C 800 500, 400 480, 120 386" fill="none" stroke="{GOLD}" stroke-width="1.4" stroke-dasharray="5,3" marker-end="url(#ag)"/>')
    # 图例
    L.append(f'<rect x="80" y="610" width="1440" height="70" rx="8" fill="{PANEL}" stroke="{LINE2}"/>')
    L.append(f'<line x1="110" y1="635" x2="170" y2="635" stroke="{RED}" stroke-width="2.6" marker-end="url(#ar)"/>')
    L.append(f'<text x="185" y="640" fill="{TEXT}" font-size="11.5">关键路径（MVP 最长依赖链，延期=MVP 延期）</text>')
    L.append(f'<line x1="560" y1="635" x2="620" y2="635" stroke="{GOLD}" stroke-width="1.6" stroke-dasharray="5,3" marker-end="url(#ag)"/>')
    L.append(f'<text x="635" y="640" fill="{TEXT}" font-size="11.5">次要依赖（可并行，不阻塞关键路径）</text>')
    L.append(f'<text x="110" y="662" fill="{MUTED}" font-size="10.5">关键路径长度 = M0(沙箱) + M1(身份/库) + M2(执行闭环) = 决定 MVP 最早完工时间；审批/策略/计费可并行于 M3-M4</text>')
    cap(L,"图 3 · MVP 关键路径（红色链为最长依赖，决定工期下限）",700,W)
    fin(L,"roadmap-critical-path.svg")

fig3()
print("DONE")
