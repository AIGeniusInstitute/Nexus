#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""生成 Nexus 系统模块组件交互流程 SVG（深色主题，archify 风格）。
产出：6 张 SVG 时序/流程图 + 1 张总览 SVG
转 PNG: rsvg-convert -w 2400 {name}.svg -o {name}.png
"""
import os
OUT = os.path.dirname(os.path.abspath(__file__))
BG="#0b141a"; PANEL="#13242e"; PANEL2="#173341"; TEXT="#e8f2f0"; MUTED="#9db8b4"
LINE="#24414e"; LINE2="#2e5a68"; ACCENT="#35c2b0"; ACCENT2="#028090"; GOLD="#e8b64c"
BLUE="#3b82f6"; RED="#ef4444"; PURPLE="#a855f7"; GREEN="#22c55e"; ORANGE="#f97316"
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
    L.append(f'<marker id="ao" viewBox="0 0 10 10" refX="9" refY="5" markerWidth="8" markerHeight="8" orient="auto"><path d="M0 0 L10 5 L0 10 z" fill="{ORANGE}"/></marker>')
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

def cap(L,t,y,w=2000):
    L.append(f'<text x="40" y="{y}" fill="{MUTED}" font-size="11.5">{t}</text>')

def fin(L,fn):
    L.append('</svg>')
    s='\n'.join(L)
    s=s.replace("&","&amp;")
    open(os.path.join(OUT,fn),'w').write(s); print("wrote",fn)

def lane_header(L, x, w, name, sub, color=ACCENT):
    L.append(f'<rect x="{x}" y="80" width="{w}" height="46" rx="6" fill="{PANEL2}" stroke="{color}" stroke-width="1.4"/>')
    lines = name.split("|")
    for i,ln in enumerate(lines):
        L.append(f'<text x="{x+w/2}" y="{100+i*16}" text-anchor="middle" fill="{color}" font-size="12.5" font-weight="700">{ln}</text>')
    if sub:
        L.append(f'<text x="{x+w/2}" y="{120}" text-anchor="middle" fill="{MUTED}" font-size="9.5">{sub}</text>')

def lifeline(L, x, y1, y2, color=LINE2):
    L.append(f'<line x1="{x}" y1="{y1}" x2="{x}" y2="{y2}" stroke="{color}" stroke-width="1.2" stroke-dasharray="4,4"/>')

def activation(L, x, y1, y2, w=10, color=ACCENT2):
    L.append(f'<rect x="{x-w/2}" y="{y1}" width="{w}" height="{y2-y1}" rx="2" fill="{color}" opacity="0.65" stroke="{color}" stroke-width="0.8"/>')

def msg(L, x1, y, x2, text, color=ACCENT, m="a", dash=None, fontsize=10.5, bold=False, offset_y=0):
    d=f'M{x1} {y+offset_y} L{x2} {y+offset_y}'
    dd=f' stroke-dasharray="{dash}"' if dash else ''
    L.append(f'<path d="{d}" fill="none" stroke="{color}" stroke-width="1.6" marker-end="url(#{m})"{dd}/>')
    mid_x = (x1+x2)/2
    fw = "700" if bold else "400"
    L.append(f'<text x="{mid_x}" y="{y+offset_y-6}" text-anchor="middle" fill="{TEXT}" font-size="{fontsize}" font-weight="{fw}">{text}</text>')

def self_msg(L, x, y, text, color=ACCENT, m="a", fontsize=10):
    w=30; h=20
    d=f'M{x} {y} L{x+w} {y} L{x+w} {y+h} L{x} {y+h}'
    L.append(f'<path d="{d}" fill="none" stroke="{color}" stroke-width="1.6" marker-end="url(#{m})"/>')
    L.append(f'<text x="{x+w+8}" y="{y+6}" fill="{TEXT}" font-size="{fontsize}">{text}</text>')

def star_note(L, x, y, text, color=GOLD):
    L.append(f'<text x="{x}" y="{y}" text-anchor="middle" fill="{color}" font-size="11" font-weight="700">★ {text}</text>')

def note_box(L, x, y, w, h, text, color=ORANGE):
    L.append(f'<rect x="{x}" y="{y}" width="{w}" height="{h}" rx="4" fill="{PANEL}" stroke="{color}" stroke-width="1" stroke-dasharray="3,2"/>')
    for i,ln in enumerate(text.split("|")):
        L.append(f'<text x="{x+8}" y="{y+16+i*14}" fill="{color}" font-size="10" font-weight="600">{ln}</text>')

def group_box(L, x, y, w, h, label, color=LINE2):
    L.append(f'<rect x="{x}" y="{y}" width="{w}" height="{h}" rx="6" fill="none" stroke="{color}" stroke-width="1" stroke-dasharray="5,3"/>')
    L.append(f'<rect x="{x+8}" y="{y-8}" width="{len(label)*7+12}" height="16" rx="3" fill="{BG}" stroke="{color}" stroke-width="0.8"/>')
    L.append(f'<text x="{x+14}" y="{y+4}" fill="{color}" font-size="10" font-weight="600">{label}</text>')

def draw_msg(L, cx, f, t, yy, txt, c, m, lanes):
    if f==t:
        self_msg(L,cx(f),yy,txt,c,m,9.5)
    else:
        activation(L,cx(f),yy-14,yy+14,color=lanes[f][3])
        activation(L,cx(t),yy-14,yy+14,color=lanes[t][3])
        lines=txt.split("\n")
        for j,ln in enumerate(lines):
            if j==0:
                msg(L,cx(f),yy,cx(t),ln,c,m)
            else:
                L.append(f'<text x="{(cx(f)+cx(t))/2}" y="{yy+j*13-6}" text-anchor="middle" fill="{MUTED}" font-size="9">{ln}</text>')

# ====== 图1: 任务全生命周期 13 步时序图 ======
def fig1_lifecycle():
    W,H=2200,1900; L=open_svg(W,H)
    title(L,"Nexus 任务全生命周期 · 13 步时序图","客户端 > 网关 > 控制面 > 调度Pod > app-server > 模型 > 事件流 > 控制面落库 > 审批 > 工具 > compact > 产物上传 > 结算销毁  ★=云端落库点",32)
    lanes = [
        (60,"Client","客户端",BLUE),
        (320,"Gateway","L2 网关",BLUE),
        (580,"Control|Plane","L3 控制面",ACCENT),
        (840,"Scheduler","L4 调度",GOLD),
        (1100,"app-server","L5 Harness",PURPLE),
        (1360,"Model|Gateway","L6 模型",ACCENT),
        (1620,"Event|Consumer","L3 消费者",ACCENT),
        (1880,"Postgres|+Object Store","L7 存储",BLUE),
    ]
    lw=240
    for x,name,sub,c in lanes:
        lane_header(L,x,lw,name,sub,c)
        lifeline(L,x+lw/2,126,H-80,c)
    cx = lambda i: lanes[i][0]+lw/2
    y=160
    steps = [
        (0,1,"① 提交任务 (Idempotency-Key)",BLUE,"ab"),
        (1,2,"② 鉴权 + 策略求值 + 配额预扣",ACCENT,"a"),
        (2,3,"③ 调度沙箱Pod (注入config/policy/令牌)",GOLD,"ag"),
        (3,4,"④ 启动app-server\n首次thread/start 或 恢复thread/resume+rollout",PURPLE,"ap"),
        (4,5,"⑤ 模型采样请求 (短期令牌,TTL≤任务超时)",ACCENT,"a"),
        (5,4,"模型响应 (token streaming)",ACCENT,"a"),
        (4,6,"⑥ 事件流回吐\nturn/started, item/*, delta",ACCENT,"a"),
        (6,7,"⑦ ★ 写Postgres (thread+turn+item 幂等)",GOLD,"ag"),
        (6,1,"⑦ WS推前端 (先落库后推,可回放)",BLUE,"ab"),
        (4,6,"⑧ 审批请求\nitem/commandExecution/requestApproval",ORANGE,"ao"),
        (6,2,"⑧ ★ 创建ApprovalTicket(pending)",GOLD,"ag"),
        (2,0,"⑧ 推送 Web/IM 审批",BLUE,"ab"),
        (0,2,"⑨ 用户决策 (批准/拒绝/修改后批准)",BLUE,"ab"),
        (2,7,"⑨ ★ 落库 ticket(decided)",GOLD,"ag"),
        (2,4,"⑨ 回写 app-server (decision)",ACCENT,"a"),
        (4,4,"⑩ 工具执行 (execpolicy+沙箱/MCP Gateway)",PURPLE,"ap"),
        (4,4,"⑪ 上下文将满, auto compact",PURPLE,"ap"),
        (4,6,"⑫ turn/completed > 事件流",ACCENT,"a"),
        (6,7,"⑫ ★ 产物+rollout 上传对象存储",GOLD,"ag"),
        (3,2,"⑬ 用量结算 + 归还配额",GOLD,"ag"),
        (2,7,"⑬ ★ 写usage+audit (WORM)",GOLD,"ag"),
        (3,3,"⑬ Pod销毁 (会话保留云端,可resume)",RED,"ar"),
    ]
    for i,(f,t,txt,c,m) in enumerate(steps):
        yy=y+i*65
        draw_msg(L,cx,f,t,yy,txt,c,m,lanes)
        if "★" in txt:
            star_note(L,cx(t),yy+22,"云端落库",GOLD)
    activation(L,cx(4),y-30,y+len(steps)*65-20,color=PURPLE,w=12)
    lx=60; ly=y+len(steps)*65+20
    L.append(f'<rect x="{lx}" y="{ly}" width="700" height="70" rx="6" fill="{PANEL}" stroke="{LINE2}"/>')
    L.append(f'<text x="{lx+12}" y="{ly+18}" fill="{ACCENT}" font-size="11" font-weight="700">图例</text>')
    L.append(f'<text x="{lx+12}" y="{ly+38}" fill="{GOLD}" font-size="10.5">★ 云端落库点: Pod随时可死,云端状态可重建resume</text>')
    L.append(f'<text x="{lx+12}" y="{ly+56}" fill="{MUTED}" font-size="10">激活条=参与方活跃区间; 虚线泳道=生命周期线; 箭头=消息流向</text>')
    cap(L,"图 1 - 任务全生命周期13步时序图 (8个参与方泳道, ★为云端落库点)",ly+90,W)
    fin(L,"flow-01-lifecycle.svg")

# ====== 图2: 会话事件流持久化流程 ======
def fig2_event_persistence():
    W,H=2000,1600; L=open_svg(W,H)
    title(L,"Nexus 会话事件流持久化流程","app-server事件 > 控制面消费者at-least-once > 幂等写入Postgres+对象存储rollout > WS推前端 (含seq缺口补齐/fork/resume语义)",32)
    lanes=[
        (60,"app-server","L5 Harness",PURPLE),
        (400,"Event Consumer","L3 消费者(at-least-once)",ACCENT),
        (740,"Postgres","L7 thread/turn/item分区表",BLUE),
        (1080,"Object Store","L7 rollout/snapshot",GOLD),
        (1420,"WS Gateway","> 前端(仅展示)",BLUE),
        (1760,"Frontend","Web/IM/IDE",GREEN),
    ]
    lw=280
    for x,name,sub,c in lanes:
        lane_header(L,x,lw,name,sub,c)
        lifeline(L,x+lw/2,126,H-60,c)
    cx=lambda i:lanes[i][0]+lw/2
    y=160
    msgs=[
        (0,1,"item/started {threadId,turnId,item}",ACCENT,"a"),
        (0,1,"item/agentMessage/delta {delta}",ACCENT,"a"),
        (0,1,"item/completed {item(完整)}",ACCENT,"a"),
        (0,1,"turn/started {turn}",ACCENT,"a"),
        (0,1,"turn/completed {turn,tokenUsage}",ACCENT,"a"),
        (0,1,"thread/started {thread}",ACCENT,"a"),
        (1,1,"幂等去重: thread_id+turn_id+item_seq 唯一键",GOLD,"ag"),
        (1,2,"★ INSERT item (ON CONFLICT DO NOTHING)",GOLD,"ag"),
        (1,2,"★ UPSERT thread/turn 状态",GOLD,"ag"),
        (1,3,"rollout上传 (每N item或每T秒)",GOLD,"ag"),
        (1,3,"turn结束 > 必传完整rollout",GOLD,"ag"),
        (1,4,"WS推送 (先落库后推送,可回放)",BLUE,"ab"),
        (4,5,"实时渲染事件时间线",GREEN,"a"),
        (1,1,"seq缺口检测: expected_seq vs received",ORANGE,"ao"),
        (1,3,"拉取rollout补齐缺口(顺序恢复)",ORANGE,"ao"),
        (1,2,"补齐缺失item (幂等写入)",GOLD,"ag"),
        (1,4,"补齐后推送前端(seq连续)",BLUE,"ab"),
    ]
    for i,(f,t,txt,c,m) in enumerate(msgs):
        yy=y+i*70
        draw_msg(L,cx,f,t,yy,txt,c,m,lanes)
    fy=y+len(msgs)*70+30
    group_box(L,60,fy,900,120,"fork/resume 语义",PURPLE)
    note_box(L,80,fy+20,860,40,"fork: thread/fork > 新thread_id, 复制item元数据(不复制大字段实体), 用于分叉探索",PURPLE)
    note_box(L,80,fy+65,860,40,"resume: 新Pod > 下载rollout > thread/resume > 新事件seq从云端最大值继续",PURPLE)
    group_box(L,1000,fy,940,120,"关键设计约束",RED)
    note_box(L,1020,fy+20,900,40,"不阻塞Harness: 写库失败不能反压app-server, 降级到本地队列+告警",RED)
    note_box(L,1020,fy+65,900,40,"大字段外置: shell输出/diff >64KB 只存对象存储引用+摘要",RED)
    cap(L,"图 2 - 会话事件流持久化流程 (at-least-once + 幂等键 + seq补齐 + fork/resume)",fy+140,W)
    fin(L,"flow-02-event-persistence.svg")

# ====== 图3: 跨进程审批 HITL 桥接时序 ======
def fig3_approval_hitl():
    W,H=2200,1800; L=open_svg(W,H)
    title(L,"Nexus 跨进程审批 HITL 桥接时序","app-server发requestApproval > 适配层 > ApprovalTicket(pending) > Web/IM > 用户决策 > 先落库 > 回写 > 继续/中止 (含6边界情况)",32)
    lanes=[
        (60,"app-server","L5(进程内审批外部化)",PURPLE),
        (400,"Adapter","L5 适配层",ACCENT),
        (740,"Approval|Center","L3 审批中心",ACCENT),
        (1080,"Postgres","L7 approval_ticket",BLUE),
        (1420,"Web/IM","推送渠道(抽屉/卡片)",GREEN),
        (1760,"User","审批人(跨设备/跨小时)",GOLD),
        (2100,"Audit","WORM 审计日志",RED),
    ]
    lw=280
    for x,name,sub,c in lanes:
        lane_header(L,x,lw,name,sub,c)
        lifeline(L,x+lw/2,126,H-60,c)
    cx=lambda i:lanes[i][0]+lw/2
    y=160
    flow=[
        (0,1,"item/commandExecution/requestApproval\n(Server>Client 请求)",PURPLE,"ap"),
        (1,2,"解析审批请求 > 提取thread/turn/item/工具/参数(脱敏)/diff/风险",ACCENT,"a"),
        (2,3,"★ 创建ApprovalTicket(status=pending)\n含: 上下文快照/审批人策略/超时动作",GOLD,"ag"),
        (2,4,"推送Web抽屉+IM卡片\n按优先级/风险等级选渠道",GREEN,"a"),
        (4,5,"用户看到审批请求(可跨小时/跨设备)",GOLD,"ag"),
        (5,4,"用户决策: 批准/拒绝/修改后批准/转交",GOLD,"ag"),
        (4,2,"回传决策结果",GREEN,"a"),
        (2,3,"★ 先落库ticket(status=decided)\ndecided_by/decided_at/note",GOLD,"ag"),
        (2,6,"★ 审计: 请求快照+决策人+时间+理由(WORM)",RED,"ar"),
        (2,1,"回写app-server(decision payload)",ACCENT,"a"),
        (1,0,"turn/interrupt 或 继续turn",PURPLE,"ap"),
        (0,0,"item/completed(status=completed/declined)",PURPLE,"ap"),
    ]
    for i,(f,t,txt,c,m) in enumerate(flow):
        yy=y+i*72
        draw_msg(L,cx,f,t,yy,txt,c,m,lanes)
    ey=y+len(flow)*72+30
    group_box(L,60,ey,2080,280,"6 个边界情况处理",ORANGE)
    cases=[
        ("① Pod在等待审批时崩了","审批状态在DB,Pod重建后resume;用item_seq去重,同一请求只问一次;已决策的直接重放"),
        ("② 审批超时","按策略默认动作(建议默认拒绝而非批准);通知申请人;ticket状态>expired"),
        ("③ 审批期间用户权限被撤销","决策时重新校验审批人权限,失效则ticket作废(cancelled);通知重新审批"),
        ("④ 用户修改参数后批准","必须重新走策略求值(改参数=新请求);不能沿用旧审批结果;生成新ticket"),
        ("⑤ 批量相似请求","提供'同类操作一律批准'作用域;限定:仅该目录/仅该工具/有效期≤1h"),
        ("⑥ 全量审计","请求快照/决策人/时间/理由全部WORM留存;不可篡改;可导出SIEM"),
    ]
    for i,(t,d) in enumerate(cases):
        col=i%3; row=i//3
        bx=80+col*680; by=ey+25+row*120
        L.append(f'<rect x="{bx}" y="{by}" width="660" height="100" rx="6" fill="{PANEL2}" stroke="{ORANGE}" stroke-width="1"/>')
        L.append(f'<text x="{bx+12}" y="{by+22}" fill="{ORANGE}" font-size="11" font-weight="700">{t}</text>')
        for j,ln in enumerate(d.split(";")):
            L.append(f'<text x="{bx+12}" y="{by+42+j*16}" fill="{MUTED}" font-size="10">{ln}</text>')
    cap(L,"图 3 - 跨进程审批HITL桥接时序 (审批是控制面一等资源,先落库后回写,6边界全覆盖)",ey+300,W)
    fin(L,"flow-03-approval-hitl.svg")

# ====== 图4: thread/resume 恢复流程 ======
def fig4_resume():
    W,H=1800,1300; L=open_svg(W,H)
    title(L,"Nexus thread/resume 恢复流程","Pod崩溃/调度迁移 > 新Pod > 下载rollout对象存储 > thread/resume > 新事件seq从云端最大值继续",32)
    lanes=[
        (60,"Old Pod|(crashed)","旧app-server(已死)",RED),
        (380,"Scheduler","L4 调度器",GOLD),
        (700,"Control Plane","L3(有云端真相)",ACCENT),
        (1020,"Object Store","L7 rollout",BLUE),
        (1340,"New Pod","新app-server",PURPLE),
        (1660,"Event Consumer","L3(继续消费)",ACCENT),
    ]
    lw=240
    for x,name,sub,c in lanes:
        lane_header(L,x,lw,name,sub,c)
        lifeline(L,x+lw/2,126,H-60,c)
    cx=lambda i:lanes[i][0]+lw/2
    y=160
    steps=[
        (0,0,"✗ Pod崩溃 / OOM / 调度回收",RED,"ar"),
        (0,1,"Pod退出信号(或心跳超时)",RED,"ar"),
        (1,2,"检测Pod失活 > 触发恢复",GOLD,"ag"),
        (2,2,"查Postgres: thread/turn/item云端最大seq",ACCENT,"a"),
        (2,3,"确认rollout对象存储key可用",ACCENT,"a"),
        (1,4,"调度新Pod(注入config/policy/令牌)",GOLD,"ag"),
        (2,4,"下发thread_id+rollout_object_key",ACCENT,"a"),
        (4,3,"下载rollout到Pod内本地",PURPLE,"ap"),
        (4,4,"thread/resume(threadId)\n加载rollout > 重建内存状态",PURPLE,"ap"),
        (4,2,"resume成功 > 报告ready",PURPLE,"ap"),
        (4,4,"turn/start(继续未完成turn)\n或等待用户新输入",PURPLE,"ap"),
        (4,5,"新事件流(seq从云端max+1继续)",ACCENT,"a"),
        (5,2,"幂等消费(thread_id+turn_id+item_seq)",ACCENT,"a"),
        (2,2,"对比云端vs新事件 > 无缺口=一致",GREEN,"a"),
    ]
    for i,(f,t,txt,c,m) in enumerate(steps):
        yy=y+i*68
        draw_msg(L,cx,f,t,yy,txt,c,m,lanes)
    gy=y+len(steps)*68+30
    group_box(L,60,gy,1680,100,"一致性保证",GREEN)
    note_box(L,80,gy+20,1640,30,"云端Postgres是唯一真相源; Harness本地SQLite/rollout是可丢弃缓存",GREEN)
    note_box(L,80,gy+55,1640,30,"resume后新事件seq = MAX(云端已落库seq)+1 > 前端无重复无缺失",GREEN)
    cap(L,"图 4 - thread/resume恢复流程 (Pod随时可死,云端状态可重建resume)",gy+120,W)
    fin(L,"flow-04-resume.svg")

# ====== 图5: 多 Agent 协作流程 ======
def fig5_multi_agent():
    W,H=2000,1500; L=open_svg(W,H)
    title(L,"Nexus 多 Agent 协作流程","主Agent > ThreadManager.spawn_subagent > fork_thread > 子Agent > agent-graph-store父子拓扑 > guardian审查",32)
    lanes=[
        (60,"Main Agent","主Thread(orchestrator)",PURPLE),
        (420,"ThreadManager","L5 协作管理器",ACCENT),
        (780,"Sub Agent|Thread","forked子Thread",BLUE),
        (1140,"agent-graph|store","父子拓扑存储",GOLD),
        (1500,"Guardian|Reviewer","自动审查子Agent",RED),
        (1860,"Event Consumer","L3事件消费",ACCENT),
    ]
    lw=280
    for x,name,sub,c in lanes:
        lane_header(L,x,lw,name,sub,c)
        lifeline(L,x+lw/2,126,H-60,c)
    cx=lambda i:lanes[i][0]+lw/2
    y=160
    steps=[
        (0,1,"spawn_subagent(goal, context, tools)",PURPLE,"ap"),
        (1,1,"决策: fork_thread vs new_thread",ACCENT,"a"),
        (1,2,"thread/fork(sourceThreadId)\n复制item元数据, 新thread_id",BLUE,"ab"),
        (1,3,"★ 写spawn-edge: parent>child拓扑",GOLD,"ag"),
        (2,2,"子Agent启动run_turn\n继承父workspace/权限快照",BLUE,"ab"),
        (2,0,"collabToolCall: send_input/resume_agent",BLUE,"ab"),
        (0,0,"父Agent继续(并行/等待)",PURPLE,"ap"),
        (2,4,"Guardian审查子Agent输出\n数据外传/凭据探测/破坏性动作",RED,"ar"),
        (4,2,"审查通过>继续 / 审查拒绝>中止",RED,"ar"),
        (4,1,"审查结果通知(item/autoApprovalReview/*)",RED,"ar"),
        (2,5,"子Agent事件流(item/started/completed)",ACCENT,"a"),
        (5,3,"★ 幂等写入子thread items",GOLD,"ag"),
        (2,0,"subAgentActivity: completed\n结果回灌父Agent turn",BLUE,"ab"),
        (0,0,"父Agent收到子结果 > 继续turn",PURPLE,"ap"),
        (0,5,"父Agent事件流 > 幂等消费",ACCENT,"a"),
        (5,3,"★ 幂等写入父thread items",GOLD,"ag"),
    ]
    for i,(f,t,txt,c,m) in enumerate(steps):
        yy=y+i*68
        draw_msg(L,cx,f,t,yy,txt,c,m,lanes)
    ty=y+len(steps)*68+30
    group_box(L,60,ty,1880,100,"父子拓扑与权限继承",PURPLE)
    note_box(L,80,ty+20,1840,30,"权限继承: 子Agent可用权限 = 父Agent权限 ∩ 子Agent角色上限 ∩ 策略中心允许",PURPLE)
    note_box(L,80,ty+55,1840,30,"thread/list(parentThreadId/ancestorThreadId)可遍历子Agent; Review/Guardian线程不参与spawn-edge生命周期",PURPLE)
    cap(L,"图 5 - 多Agent协作流程 (fork语义 + 父子拓扑 + Guardian审查 + 事件幂等消费)",ty+120,W)
    fin(L,"flow-05-multi-agent.svg")

# ====== 图6: 工具调用流程 ======
def fig6_tool_call():
    W,H=2000,1400; L=open_svg(W,H)
    title(L,"Nexus 工具调用流程","模型采样 > ToolRouter.dispatch > execpolicy评估 > OS沙箱执行(shell)或MCP Gateway注入凭据(MCP) > 记Item > 审批(如需)",32)
    lanes=[
        (60,"Model","L6 模型采样",ACCENT),
        (420,"ToolRouter","L5 工具路由",PURPLE),
        (780,"ExecPolicy","L5 策略求值",GOLD),
        (1140,"Sandbox|/MCP Gateway","L4 沙箱/MCP侧车",RED),
        (1500,"app-server","L5 Item记录",PURPLE),
        (1860,"Approval|Center","L3审批(如需)",ORANGE),
    ]
    lw=280
    for x,name,sub,c in lanes:
        lane_header(L,x,lw,name,sub,c)
        lifeline(L,x+lw/2,126,H-60,c)
    cx=lambda i:lanes[i][0]+lw/2
    y=160
    steps=[
        (0,1,"采样: function_call(tool, arguments)",ACCENT,"a"),
        (1,1,"ToolRouter.dispatch(tool_name, args)",PURPLE,"ap"),
        (1,2,"execpolicy规则求值\nallow/deny/require_approval",GOLD,"ag"),
        (2,1,"决策: allow / deny / require_approval",GOLD,"ag"),
        (1,3,"[shell] 命令 > OS沙箱执行\nSeatbelt/Landlock+seccomp/bwrap",RED,"ar"),
        (3,3,"沙箱内执行(网络仅到Gateway)",RED,"ar"),
        (3,1,"执行结果(stdout/stderr/exitCode)",RED,"ar"),
        (1,3,"[MCP] 工具调用 > MCP Gateway侧车",BLUE,"ab"),
        (3,3,"凭据注入(短期JWT)+白名单过滤\n出站审计+敏感字段脱敏",BLUE,"ab"),
        (3,3,"转发到真实MCP Server/企业API",BLUE,"ab"),
        (3,1,"MCP结果(result/error)",BLUE,"ab"),
        (1,5,"[require_approval] > 审批中心",ORANGE,"ao"),
        (5,1,"审批结果(approved/rejected)",ORANGE,"ao"),
        (1,4,"item/started(commandExecution/mcpToolCall)",PURPLE,"ap"),
        (1,4,"item/completed(status=completed/failed/declined)",PURPLE,"ap"),
        (4,0,"工具结果回灌模型上下文",ACCENT,"a"),
    ]
    for i,(f,t,txt,c,m) in enumerate(steps):
        yy=y+i*64
        draw_msg(L,cx,f,t,yy,txt,c,m,lanes)
    dy=y+len(steps)*64+30
    group_box(L,60,dy,1880,120,"三条路径决策",GOLD)
    note_box(L,80,dy+20,600,30,"[shell] execpolicy=allow > OS沙箱直接执行",GREEN)
    note_box(L,700,dy+20,600,30,"[MCP] Gateway注入凭据 > 白名单 > 企业API",BLUE)
    note_box(L,1320,dy+20,600,30,"[approval] execpolicy=require_approval > 审批中心",ORANGE)
    note_box(L,80,dy+55,1840,30,"安全: config.toml零真实密钥; MCP凭据由Gateway持有; 破坏性工具恒定需审批; 命令显示用redacted值",RED)
    cap(L,"图 6 - 工具调用流程 (模型>路由>策略>沙箱/MCP>记录>审批闭环)",dy+140,W)
    fin(L,"flow-06-tool-call.svg")

# ====== 图7: 总览 ======
def fig7_overview():
    W,H=2200,900; L=open_svg(W,H)
    title(L,"Nexus 系统模块组件交互总览","6大流程全景: ①生命周期 ②事件持久化 ③审批HITL ④resume恢复 ⑤多Agent协作 ⑥工具调用",32)
    flows=[
        ("① 任务全生命周期","13步时序: 提交>鉴权>调度>app-server>模型>事件>★落库>审批>工具>compact>产物>结算","flow-01-lifecycle.svg",ACCENT),
        ("② 事件流持久化","app-server事件>at-least-once消费>幂等写入Postgres+对象存储>WS推前端>seq补齐>fork/resume","flow-02-event-persistence.svg",BLUE),
        ("③ 审批HITL桥接","requestApproval>ApprovalTicket>Web/IM>用户决策>先落库>回写>继续/中止(6边界)","flow-03-approval-hitl.svg",ORANGE),
        ("④ thread/resume恢复","Pod崩溃>新Pod>下载rollout>thread/resume>seq从云端max继续>一致性保证","flow-04-resume.svg",GREEN),
        ("⑤ 多Agent协作","spawn_subagent>fork_thread>子Agent>父子拓扑>Guardian审查>结果回灌","flow-05-multi-agent.svg",PURPLE),
        ("⑥ 工具调用","模型采样>ToolRouter>execpolicy>OS沙箱/MCP Gateway>记Item>审批(如需)","flow-06-tool-call.svg",GOLD),
    ]
    y=90
    for i,(n,d,fn,c) in enumerate(flows):
        col=i%2; row=i//2
        x=60+col*1060
        yy=y+row*250
        L.append(f'<rect x="{x}" y="{yy}" width="1040" height="220" rx="10" fill="{PANEL}" stroke="{c}" stroke-width="1.6"/>')
        L.append(f'<rect x="{x}" y="{yy}" width="1040" height="36" rx="10" fill="{c}" opacity="0.3"/>')
        L.append(f'<text x="{x+16}" y="{yy+24}" fill="{c}" font-size="14" font-weight="700">{n}</text>')
        L.append(f'<text x="{x+16}" y="{yy+56}" fill="{TEXT}" font-size="11.5">{d}</text>')
        L.append(f'<text x="{x+16}" y="{yy+80}" fill="{MUTED}" font-size="10">> {fn}</text>')
        bars=5
        for j in range(bars):
            bx=x+20+j*195
            L.append(f'<rect x="{bx}" y="{yy+100}" width="180" height="24" rx="4" fill="{PANEL2}" stroke="{c}" stroke-width="0.8"/>')
            L.append(f'<text x="{bx+90}" y="{yy+116}" text-anchor="middle" fill="{c}" font-size="9">{["step","flow","gate","persist","done"][j]}</text>')
            if j<bars-1:
                d=f'M{bx+180} {yy+112} L{bx+195} {yy+112}'
                L.append(f'<path d="{d}" fill="none" stroke="{c}" stroke-width="1.2" marker-end="url(#a)"/>')
    cap(L,"图 7 - 6大交互流程总览 (每张子图对应一张详细SVG时序图)",y+3*250+20,W)
    fin(L,"interaction-flow-overview.svg")

fig1_lifecycle()
fig2_event_persistence()
fig3_approval_hitl()
fig4_resume()
fig5_multi_agent()
fig6_tool_call()
fig7_overview()
print("ALL DONE")
