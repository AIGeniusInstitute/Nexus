#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""生成 Nexus 全部系统 API 清单总览图 SVG（深色主题，archify 风格）。
产出：api-overview.svg
转 PNG: rsvg-convert -w 2400 api-overview.svg -o api-overview.png
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

def cap(L,t,y,w=2400):
    L.append(f'<text x="40" y="{y}" fill="{MUTED}" font-size="11.5">{t}</text>')

def fin(L,fn):
    L.append('</svg>')
    s='\n'.join(L)
    s=s.replace("&","&amp;")
    open(os.path.join(OUT,fn),'w').write(s); print("wrote",fn)

def method_block(L,x,y,w,h,name,fill=PANEL2,stroke=LINE2,fg=TEXT,size=9,bold=False,tag=None,tag_color=None):
    L.append(f'<rect x="{x}" y="{y}" width="{w}" height="{h}" rx="4" fill="{fill}" stroke="{stroke}" stroke-width="0.8"/>')
    fw = "700" if bold else "400"
    L.append(f'<text x="{x+w/2}" y="{y+h/2+3}" text-anchor="middle" fill="{fg}" font-size="{size}" font-weight="{fw}">{name}</text>')
    if tag:
        L.append(f'<rect x="{x+w-20}" y="{y+2}" width="16" height="10" rx="2" fill="{tag_color}" opacity="0.8"/>')

W,H=2400,3600; L=open_svg(W,H)
title(L,"Nexus 企业级 AI Agent 平台 · 全部系统 API 清单总览",
      "三大类 API：A. Codex app-server JSON-RPC（L5 Harness 集成面，复用） · B. Nexus 控制平面 REST API（L2 网关，自建） · C. Webhook 与 WebSocket",40)

# Legend
ly = 82
L.append(f'<rect x="40" y="{ly}" width="14" height="10" rx="2" fill="{ACCENT}" opacity="0.8"/>')
L.append(f'<text x="60" y="{ly+9}" fill="{MUTED}" font-size="11">复用 Codex</text>')
L.append(f'<rect x="150" y="{ly}" width="14" height="10" rx="2" fill="{BLUE}" opacity="0.8"/>')
L.append(f'<text x="170" y="{ly+9}" fill="{MUTED}" font-size="11">Nexus 自建</text>')
L.append(f'<rect x="260" y="{ly}" width="14" height="10" rx="2" fill="{GOLD}" opacity="0.8"/>')
L.append(f'<text x="280" y="{ly+9}" fill="{MUTED}" font-size="11">实验性</text>')
L.append(f'<rect x="350" y="{ly}" width="14" height="10" rx="2" fill="{PURPLE}" opacity="0.8"/>')
L.append(f'<text x="370" y="{ly+9}" fill="{MUTED}" font-size="11">Server→Client 请求</text>')
L.append(f'<rect x="490" y="{ly}" width="14" height="10" rx="2" fill="{RED}" opacity="0.8"/>')
L.append(f'<text x="510" y="{ly+9}" fill="{MUTED}" font-size="11">已废弃</text>')

# ====== Lane A: Codex app-server JSON-RPC ======
lane_y = 110
lane_h = 1480
L.append(f'<rect x="20" y="{lane_y}" width="2360" height="{lane_h}" rx="14" fill="{PANEL}" stroke="{ACCENT2}" stroke-width="2"/>')
L.append(f'<rect x="20" y="{lane_y}" width="2360" height="34" rx="14" fill="url(#gt)"/>')
L.append(f'<rect x="20" y="{lane_y+20}" width="2360" height="14" fill="url(#gt)"/>')
L.append(f'<text x="40" y="{lane_y+23}" fill="#e8f2f0" font-size="15" font-weight="700">A. Codex app-server JSON-RPC API（L5 Harness 集成面 · 复用 Codex 黑盒）</text>')
L.append(f'<text x="2100" y="{lane_y+23}" fill="#e8f2f0" font-size="12" font-weight="700">协议: JSON-RPC 2.0 · 传输: stdio/ws/unix</text>')

# A.1 Lifecycle
sx = 40; sy = lane_y + 50
L.append(f'<text x="{sx}" y="{sy+12}" fill="{ACCENT}" font-size="12" font-weight="700">A.1 生命周期 Lifecycle</text>')
items_lc = ["initialize","initialized"]
for i,m in enumerate(items_lc):
    method_block(L,sx+i*110,sy+20,100,28,m,fill=PANEL2,stroke=ACCENT2,fg=TEXT,size=9,bold=True)

# A.2 Thread
sy2 = sy + 62
L.append(f'<text x="{sx}" y="{sy2+12}" fill="{ACCENT}" font-size="12" font-weight="700">A.2 Thread（会话）</text>')
items_th = [
    "thread/start","thread/resume","thread/fork","thread/archive","thread/unarchive",
    "thread/delete","thread/read","thread/list","thread/search","thread/searchOccurrences",
    "thread/loaded/list","thread/turns/list","thread/items/list","thread/inject_items",
    "thread/timeline/list","thread/unsubscribe","thread/rollback*","thread/revert",
    "thread/compact/start","thread/shellCommand","thread/approveGuardianDeniedAction",
    "thread/metadata/update","thread/name/set","thread/memoryMode/set","memory/reset",
]
for i,m in enumerate(items_th):
    col = i % 8; row = i // 8
    mx = sx + col * 145; my = sy2 + 20 + row * 34
    stroke_c = LINE2
    if "rollback" in m: stroke_c = RED
    elif "search" in m or "timeline" in m or "memoryMode" in m: stroke_c = GOLD
    method_block(L,mx,my,135,28,m,fill=PANEL2,stroke=stroke_c,size=8.5)

# Thread sections
sy3 = sy2 + 20 + 4*34 + 6
L.append(f'<text x="{sx}" y="{sy3+12}" fill="{ACCENT}" font-size="12" font-weight="700">A.2b Thread Sections / Queue / Goal</text>')
items_tq = [
    "threadSection/list","threadSection/create","threadSection/update","threadSection/delete",
    "thread/section/move","thread/settings/update","thread/queue/add","thread/queue/list",
    "thread/queue/update","thread/queue/delete","thread/queue/reorder","thread/queue/start",
    "thread/goal/set","thread/goal/get","thread/goal/clear",
]
for i,m in enumerate(items_tq):
    col = i % 8; row = i // 8
    mx = sx + col * 145; my = sy3 + 20 + row * 34
    method_block(L,mx,my,135,28,m,fill=PANEL2,stroke=GOLD,size=8.5)

# Background terminals / realtime
sy4 = sy3 + 20 + 2*34 + 6
L.append(f'<text x="{sx}" y="{sy4+12}" fill="{ACCENT}" font-size="12" font-weight="700">A.2c Background Terminals / Realtime</text>')
items_bt = [
    "bgTerms/clean","bgTerms/list","bgTerms/terminate",
    "realtime/start","realtime/appendAudio","realtime/appendText",
    "realtime/appendSpeech","realtime/stop","realtime/listVoices",
]
for i,m in enumerate(items_bt):
    mx = sx + i * 145; my = sy4 + 20
    method_block(L,mx,my,135,28,m,fill=PANEL2,stroke=GOLD,size=8)

# A.3 Turn
sy5 = sy4 + 60
L.append(f'<text x="{sx}" y="{sy5+12}" fill="{ACCENT}" font-size="12" font-weight="700">A.3 Turn（对话轮次）</text>')
items_turn = ["turn/start","turn/settings/update","turn/steer","turn/interrupt"]
for i,m in enumerate(items_turn):
    method_block(L,sx+i*145,sy5+20,135,28,m,fill=PANEL2,stroke=ACCENT2,size=10,bold=True)

# A.4 Events (Server Notifications)
sy6 = sy5 + 62
L.append(f'<text x="{sx}" y="{sy6+12}" fill="{ACCENT}" font-size="12" font-weight="700">A.4 事件 ServerNotification（服务器推送通知）</text>')
items_ev = [
    "thread/started","thread/status/changed","thread/archived","thread/unarchived",
    "thread/closed","thread/name/updated","thread/deleted","thread/settings/updated",
    "thread/tokenUsage/updated","thread/goal/updated","thread/goal/cleared","thread/queue/changed",
    "thread/reverted","thread/environment/connected","thread/environment/disconnected",
    "turn/started","turn/completed","turn/diff/updated","turn/plan/updated","turn/moderationMetadata",
    "item/started","item/completed","item/agentMessage/delta","item/plan/delta",
    "item/reasoning/summaryTextDelta","item/reasoning/summaryPartAdded","item/reasoning/textDelta",
    "item/commandExecution/outputDelta","item/fileChange/patchUpdated","item/fileChange/outputDelta*",
    "rawResponse/completed","rawResponseItem/completed","model/safetyBuffering/updated",
    "model/rerouted","model/verification","modelProvider/authRecoveryStarted","modelProvider/authRecoveryCompleted",
    "error","warning","configWarning","skills/changed","app/list/updated",
    "compacted*","thread/project/updated","project/changed",
    "fuzzyFileSearch/sessionUpdated","fuzzyFileSearch/sessionCompleted",
    "thread/realtime/started","thread/realtime/itemAdded","thread/realtime/transcript/delta",
    "thread/realtime/transcript/done","thread/realtime/item/started","thread/realtime/item/transcript/delta",
    "thread/realtime/item/completed","thread/realtime/outputAudio/delta","thread/realtime/error",
    "thread/realtime/closed","thread/realtime/sdp",
    "mcpServer/startupStatus/updated","mcpServer/event/stream/notification",
    "item/autoApprovalReview/started","item/autoApprovalReview/completed",
    "autoApprovalReview/strictReviewRequired","windowsSandbox/setupCompleted",
    "serverRequest/resolved","account/login/completed","account/updated","account/rateLimits/updated",
    "mcpServer/oauthLogin/completed",
]
for i,m in enumerate(items_ev):
    col = i % 9; row = i // 9
    mx = sx + col * 130; my = sy6 + 20 + row * 30
    stroke_c = ACCENT2
    if "*" in m: stroke_c = RED
    method_block(L,mx,my,120,24,m,fill=PANEL2,stroke=stroke_c,size=7.5)

# A.5 Approvals (Server→Client Requests)
sy7 = sy6 + 20 + ((len(items_ev)-1)//9 + 1) * 30 + 8
L.append(f'<text x="{sx}" y="{sy7+12}" fill="{PURPLE}" font-size="12" font-weight="700">A.5 审批 Server→Client 请求（HITL 审批流）</text>')
items_ap = [
    "item/commandExecution/requestApproval","item/fileChange/requestApproval","item/permissions/requestApproval",
    "item/tool/requestUserInput","mcpServer/elicitation/request","item/tool/call (DynamicToolCall)",
    "attestation/generate","currentTime/read",
]
for i,m in enumerate(items_ap):
    col = i % 4; row = i // 4
    mx = sx + col * 290; my = sy7 + 20 + row * 34
    method_block(L,mx,my,275,28,m,fill=PANEL2,stroke=PURPLE,size=8.5)

# A.6 Tools / FS / MCP / Plugin / Skills / Hooks
sy8 = sy7 + 20 + 2*34 + 8
L.append(f'<text x="{sx}" y="{sy8+12}" fill="{ACCENT}" font-size="12" font-weight="700">A.6 工具/文件/MCP/插件/Skills/Hooks</text>')
items_tf = [
    "app/read","app/list","app/installed","app/read(batch)",
    "fs/readFile","fs/writeFile","fs/createDirectory","fs/getMetadata","fs/readDirectory","fs/remove","fs/copy","fs/watch","fs/unwatch","fs/changed",
    "plugin/list","plugin/search","plugin/installed","plugin/reconcile","plugin/read","plugin/skill/read","plugin/install","plugin/uninstall",
    "skills/list","skills/extraRoots/set","skills/config/write","hooks/list",
    "marketplace/add","marketplace/remove","marketplace/upgrade",
    "mcpServer/oauth/login","mcpServer/tool/call","mcpServer/resource/read","mcpServerStatus/list","config/mcpServer/reload",
    "mcpServer/event/stream/start","mcpServer/event/stream/stop",
    "command/exec","command/exec/write","command/exec/resize","command/exec/terminate","command/exec/outputDelta",
    "process/spawn","process/writeStdin","process/resizePty","process/kill","process/outputDelta","process/exited",
    "fuzzyFileSearch/sessionStart*","fuzzyFileSearch/query*",
]
for i,m in enumerate(items_tf):
    col = i % 9; row = i // 9
    mx = sx + col * 130; my = sy8 + 20 + row * 30
    stroke_c = ACCENT2
    if "*" in m: stroke_c = GOLD
    method_block(L,mx,my,120,24,m,fill=PANEL2,stroke=stroke_c,size=7.5)

# A.7 Config / Model / Account
sy9 = sy8 + 20 + ((len(items_tf)-1)//9 + 1) * 30 + 8
L.append(f'<text x="{sx}" y="{sy9+12}" fill="{ACCENT}" font-size="12" font-weight="700">A.7 配置/模型/账户</text>')
items_cfg = [
    "config/read","config/value/write","config/batchWrite","configRequirements/read",
    "model/list","modelProvider/capabilities/read","permissionProfile/list",
    "experimentalFeature/list","experimentalFeature/enablement/set",
    "collaborationMode/list","environment/add","environment/info","environment/status",
    "account/read","account/login/start","account/login/cancel","account/logout",
    "account/rateLimits/read","account/usage/read","account/workspaceMessages/read",
    "account/rateLimitResetCredit/consume","account/sendAddCreditsNudgeEmail",
    "account/bedrock/discover*","account/bedrock/setup*",
    "review/start","server/diagnostics","feedback/upload",
    "remoteControl/enable*","remoteControl/disable*","remoteControl/status/read*",
    "remoteControl/pairing/start*","remoteControl/pairing/status*","remoteControl/client/list*","remoteControl/client/revoke*",
    "windowsSandbox/setupStart","externalAgentConfig/detect*","externalAgentConfig/import*",
    "externalAgentConfig/import/readHistories*",
    "project/list*","project/read*","project/create*","project/import*","project/update*","project/move*","project/delete*",
]
for i,m in enumerate(items_cfg):
    col = i % 10; row = i // 10
    mx = sx + col * 117; my = sy9 + 20 + row * 30
    stroke_c = ACCENT2
    if "*" in m: stroke_c = GOLD
    method_block(L,mx,my,110,24,m,fill=PANEL2,stroke=stroke_c,size=7)

cap(L,"图 A · Codex app-server JSON-RPC API 全清单（紫色=Server→Client审批请求，红色*=已废弃，金色*=实验性）",lane_y+lane_h-10,W)

# ====== Lane B: Nexus REST API ======
lane2_y = lane_y + lane_h + 20
lane2_h = 1300
L.append(f'<rect x="20" y="{lane2_y}" width="2360" height="{lane2_h}" rx="14" fill="{PANEL}" stroke="{BLUE}" stroke-width="2"/>')
L.append(f'<rect x="20" y="{lane2_y}" width="2360" height="34" rx="14" fill="url(#gb)"/>')
L.append(f'<rect x="20" y="{lane2_y+20}" width="2360" height="14" fill="url(#gb)"/>')
L.append(f'<text x="40" y="{lane2_y+23}" fill="#e8f2f0" font-size="15" font-weight="700">B. Nexus 控制平面 REST API（L2 网关 · 自建 · 鉴权: Bearer JWT + RBAC）</text>')
L.append(f'<text x="1900" y="{lane2_y+23}" fill="#e8f2f0" font-size="12" font-weight="700">协议: HTTPS REST · 内容: JSON</text>')

# B.1 Auth
bx = 40; by = lane2_y + 50
L.append(f'<text x="{bx}" y="{by+12}" fill="{BLUE}" font-size="12" font-weight="700">B.1 认证 Auth</text>')
items_ba = [
    "POST /auth/login","POST /auth/refresh","POST /auth/logout","GET /auth/me",
    "POST /auth/api-keys","DELETE /auth/api-keys/{id}","GET /auth/sessions","DELETE /auth/sessions/{id}",
]
for i,m in enumerate(items_ba):
    col = i % 4; row = i // 4
    mx = bx + col * 290; my = by + 20 + row * 30
    method_block(L,mx,my,275,24,m,fill=PANEL2,stroke=BLUE,size=8.5)

# B.2 Tenant / Org / Users / RBAC
by2 = by + 20 + 2*30 + 6
L.append(f'<text x="{bx}" y="{by2+12}" fill="{BLUE}" font-size="12" font-weight="700">B.2 租户/组织/用户/RBAC</text>')
items_bo = [
    "GET /tenants","POST /tenants","GET /tenants/{id}","PUT /tenants/{id}",
    "GET /org-units","POST /org-units","PUT /org-units/{id}","DELETE /org-units/{id}",
    "GET /users","POST /users","GET /users/{id}","PUT /users/{id}","DELETE /users/{id}",
    "GET /roles","POST /roles","PUT /roles/{id}","DELETE /roles/{id}",
    "GET /users/{id}/memberships","POST /users/{id}/memberships","DELETE /memberships/{id}",
]
for i,m in enumerate(items_bo):
    col = i % 6; row = i // 6
    mx = bx + col * 195; my = by2 + 20 + row * 30
    method_block(L,mx,my,185,24,m,fill=PANEL2,stroke=BLUE,size=8)

# B.3 Workspaces
by3 = by2 + 20 + 4*30 + 6
L.append(f'<text x="{bx}" y="{by3+12}" fill="{BLUE}" font-size="12" font-weight="700">B.3 工作区 Workspaces</text>')
items_bw = [
    "GET /workspaces","POST /workspaces","GET /workspaces/{id}","PUT /workspaces/{id}","DELETE /workspaces/{id}",
    "GET /workspaces/{id}/members","POST /workspaces/{id}/members","PUT /workspaces/{id}/members/{uid}","DELETE /workspaces/{id}/members/{uid}",
    "GET /workspaces/{id}/settings","PUT /workspaces/{id}/settings",
]
for i,m in enumerate(items_bw):
    col = i % 6; row = i // 6
    mx = bx + col * 195; my = by3 + 20 + row * 30
    method_block(L,mx,my,185,24,m,fill=PANEL2,stroke=BLUE,size=8)

# B.4 Threads (REST mapping to JSON-RPC)
by4 = by3 + 20 + 2*30 + 6
L.append(f'<text x="{bx}" y="{by4+12}" fill="{BLUE}" font-size="12" font-weight="700">B.4 会话 Threads（REST 网关 → 映射 app-server JSON-RPC）</text>')
items_bt2 = [
    "GET /threads","POST /threads (→thread/start)","GET /threads/{id} (→thread/read)","DELETE /threads/{id} (→thread/delete)",
    "POST /threads/{id}/resume (→thread/resume)","POST /threads/{id}/fork (→thread/fork)","POST /threads/{id}/archive (→thread/archive)",
    "GET /threads/{id}/turns","POST /threads/{id}/turns (→turn/start)","POST /threads/{id}/interrupt (→turn/interrupt)",
    "POST /threads/{id}/steer (→turn/steer)","GET /threads/{id}/items","POST /threads/{id}/messages",
    "GET /threads/{id}/timeline (→thread/timeline/list)","POST /threads/{id}/compact (→thread/compact/start)",
    "GET /threads/{id}/search (→thread/searchOccurrences)","PUT /threads/{id}/metadata (→thread/metadata/update)",
    "PUT /threads/{id}/settings (→thread/settings/update)","POST /threads/{id}/shell (→thread/shellCommand)",
]
for i,m in enumerate(items_bt2):
    col = i % 4; row = i // 4
    mx = bx + col * 290; my = by4 + 20 + row * 30
    method_block(L,mx,my,275,24,m,fill=PANEL2,stroke=BLUE,size=7.5)

# B.5 Approvals
by5 = by4 + 20 + 5*30 + 6
L.append(f'<text x="{bx}" y="{by5+12}" fill="{BLUE}" font-size="12" font-weight="700">B.5 审批中心 Approvals（HITL）</text>')
items_bap = [
    "GET /approvals","GET /approvals/pending","POST /approvals","GET /approvals/{id}",
    "POST /approvals/{id}/decide","GET /approvals/stats","GET /approvals/rules","PUT /approvals/rules",
]
for i,m in enumerate(items_bap):
    col = i % 4; row = i // 4
    mx = bx + col * 290; my = by5 + 20 + row * 30
    method_block(L,mx,my,275,24,m,fill=PANEL2,stroke=PURPLE,size=8.5)

# B.6 Connectors / MCP
by6 = by5 + 20 + 2*30 + 6
L.append(f'<text x="{bx}" y="{by6+12}" fill="{BLUE}" font-size="12" font-weight="700">B.6 连接器 Connectors / MCP Gateway</text>')
items_bc = [
    "GET /connectors","POST /connectors","GET /connectors/{id}","PUT /connectors/{id}","DELETE /connectors/{id}",
    "GET /connectors/{id}/tools","POST /connectors/{id}/tools/call","GET /connectors/{id}/health",
    "GET /connectors/{id}/resources","POST /connectors/{id}/oauth/start","POST /connectors/{id}/oauth/callback",
]
for i,m in enumerate(items_bc):
    col = i % 6; row = i // 6
    mx = bx + col * 195; my = by6 + 20 + row * 30
    method_block(L,mx,my,185,24,m,fill=PANEL2,stroke=BLUE,size=7.5)

# B.7 Usage / Audit / Cost
by7 = by6 + 20 + 2*30 + 6
L.append(f'<text x="{bx}" y="{by7+12}" fill="{BLUE}" font-size="12" font-weight="700">B.7 计量/审计/成本</text>')
items_bu = [
    "GET /usage","GET /usage/breakdown","GET /usage/export","GET /usage/realtime",
    "GET /audit-logs","GET /audit-logs/{id}","POST /audit-logs/export",
    "GET /cost-dashboard","GET /cost-dashboard/by-tenant","GET /cost-dashboard/by-workspace","GET /cost-dashboard/by-model",
]
for i,m in enumerate(items_bu):
    col = i % 6; row = i // 6
    mx = bx + col * 195; my = by7 + 20 + row * 30
    method_block(L,mx,my,185,24,m,fill=PANEL2,stroke=BLUE,size=7.5)

# B.8 Knowledge Base
by8 = by7 + 20 + 2*30 + 6
L.append(f'<text x="{bx}" y="{by8+12}" fill="{BLUE}" font-size="12" font-weight="700">B.8 知识库 Knowledge Base / RAG</text>')
items_bk = [
    "GET /kb/documents","POST /kb/documents","GET /kb/documents/{id}","PUT /kb/documents/{id}","DELETE /kb/documents/{id}",
    "POST /kb/search","POST /kb/embeddings","GET /kb/collections","POST /kb/collections","DELETE /kb/collections/{id}",
    "POST /kb/ingest","GET /kb/stats",
]
for i,m in enumerate(items_bk):
    col = i % 6; row = i // 6
    mx = bx + col * 195; my = by8 + 20 + row * 30
    method_block(L,mx,my,185,24,m,fill=PANEL2,stroke=BLUE,size=7.5)

cap(L,"图 B · Nexus 控制平面 REST API 全清单（自建 · L2 网关 · Bearer JWT + RBAC 鉴权）",lane2_y+lane2_h-10,W)

# ====== Lane C: Webhook & WebSocket ======
lane3_y = lane2_y + lane2_h + 20
lane3_h = 460
L.append(f'<rect x="20" y="{lane3_y}" width="2360" height="{lane3_h}" rx="14" fill="{PANEL}" stroke="{GOLD}" stroke-width="2"/>')
L.append(f'<rect x="20" y="{lane3_y}" width="2360" height="34" rx="14" fill="url(#gg)"/>')
L.append(f'<rect x="20" y="{lane3_y+20}" width="2360" height="14" fill="url(#gg)"/>')
L.append(f'<text x="40" y="{lane3_y+23}" fill="#e8f2f0" font-size="15" font-weight="700">C. Webhook 与 WebSocket（事件推送 · 权限驱动订阅）</text>')

# C.1 Webhook
cx = 40; cy = lane3_y + 50
L.append(f'<text x="{cx}" y="{cy+12}" fill="{GOLD}" font-size="12" font-weight="700">C.1 任务完成 Webhook（POST 回调 · HMAC-SHA256 签名 · 幂等）</text>')
items_cw = [
    "POST {callback_url}  ·  X-Nexus-Signature: hmac_sha256(payload, secret)  ·  X-Nexus-Event: turn.completed",
    "POST {callback_url}  ·  X-Nexus-Event: approval.requested",
    "POST {callback_url}  ·  X-Nexus-Event: thread.archived",
    "POST {callback_url}  ·  X-Nexus-Event: goal.blocked",
    "POST {callback_url}  ·  X-Nexus-Event: usage.threshold_exceeded",
]
for i,m in enumerate(items_cw):
    mx = cx; my = cy + 20 + i * 34
    method_block(L,mx,my,800,28,m,fill=PANEL2,stroke=GOLD,size=9)

# Webhook management
items_cw2 = [
    "POST /webhooks","GET /webhooks","PUT /webhooks/{id}","DELETE /webhooks/{id}",
    "POST /webhooks/{id}/test","GET /webhooks/{id}/deliveries","POST /webhooks/{id}/deliveries/{did}/retry",
]
L.append(f'<text x="900" y="{cy+12}" fill="{GOLD}" font-size="12" font-weight="700">C.2 Webhook 管理</text>')
for i,m in enumerate(items_cw2):
    col = i % 2; row = i // 2
    mx = 900 + col * 300; my = cy + 20 + row * 34
    method_block(L,mx,my,285,28,m,fill=PANEL2,stroke=GOLD,size=8.5)

# C.3 WebSocket
cy2 = cy + 20 + 5*34 + 6
L.append(f'<text x="{cx}" y="{cy2+12}" fill="{GOLD}" font-size="12" font-weight="700">C.3 WebSocket 事件推送 WS /ws/threads/{{id}}/events（权限驱动订阅）</text>')
items_cws = [
    "WS /ws/threads/{threadId}/events  ·  subscribe → thread/turn/item 事件实时推送",
    "WS /ws/threads/{threadId}/events  ·  subscribe → item/agentMessage/delta 流式",
    "WS /ws/threads/{threadId}/events  ·  subscribe → item/commandExecution/outputDelta",
    "WS /ws/threads/{threadId}/events  ·  subscribe → turn/started / turn/completed",
    "WS /ws/threads/{threadId}/events  ·  subscribe → approval 请求推送 + 响应",
    "WS /ws/dashboard  ·  subscribe → 全局用量/状态/告警实时推送",
]
for i,m in enumerate(items_cws):
    mx = cx; my = cy2 + 20 + i * 30
    method_block(L,mx,my,800,24,m,fill=PANEL2,stroke=GOLD,size=8.5)

# C.4 SSE fallback
L.append(f'<text x="900" y="{cy2+12}" fill="{GOLD}" font-size="12" font-weight="700">C.4 SSE 降级（WS 不可用时）</text>')
items_csse = [
    "GET /threads/{id}/events/stream  ·  text/event-stream 降级推送",
    "GET /dashboard/stream  ·  全局仪表盘 SSE 降级",
]
for i,m in enumerate(items_csse):
    mx = 900; my = cy2 + 20 + i * 30
    method_block(L,mx,my,600,24,m,fill=PANEL2,stroke=GOLD,size=8.5)

cap(L,"图 C · Webhook（HMAC 签名 POST 回调 · 幂等）与 WebSocket（权限驱动实时订阅 · SSE 降级）",lane3_y+lane3_h-10,W)

# Summary stats at bottom
sy_final = lane3_y + lane3_h + 20
L.append(f'<text x="40" y="{sy_final}" fill="{ACCENT}" font-size="14" font-weight="700">统计概要</text>')
stats = [
    f"A. Codex JSON-RPC: ~180 方法（含通知/审批请求） · 复用 Codex 黑盒 · L5 Harness 集成面",
    f"B. Nexus REST API: ~85 路由 · 自建 · L2 网关 · Bearer JWT + RBAC + 幂等键",
    f"C. Webhook/WebSocket: 6 事件类型回调 + 8 WS 通道 + 2 SSE 降级 · HMAC 签名 · 权限驱动",
]
for i,s in enumerate(stats):
    L.append(f'<text x="60" y="{sy_final + 25 + i*20}" fill="{MUTED}" font-size="11">{s}</text>')

fin(L,"api-overview.svg")
print("DONE")
