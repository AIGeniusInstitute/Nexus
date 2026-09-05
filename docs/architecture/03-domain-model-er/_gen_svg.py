#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""生成 Nexus 领域模型 ER 图 SVG（archify 风格，深色主题）。
产出：
  domain-model-core.svg       核心域 ER 总图（租户→工作区→会话→审批→用量）
  domain-model-governance.svg 治理/周边域放大图（审计/沙箱/模型/MCP/Skills）
转 PNG: rsvg-convert -w 2400 {name}.svg -o {name}.png
"""
import os
OUT = os.path.dirname(os.path.abspath(__file__))

# ---- archify 色板（复用 01-system-architecture/_gen_svg.py）----
BG="#0b141a"; PANEL="#13242e"; PANEL2="#173341"; TEXT="#e8f2f0"; MUTED="#9db8b4"
LINE="#24414e"; LINE2="#2e5a68"; ACCENT="#35c2b0"; ACCENT2="#028090"; GOLD="#e8b64c"
BLUE="#3b82f6"; RED="#ef4444"; PURPLE="#a855f7"; ORANGE="#f97316"
FONT="'Helvetica Neue',Helvetica,Arial,'PingFang SC','Microsoft YaHei','SimHei',sans-serif"

# 域色板（实体框边框 + 标题色）
DOMAIN = {
    "tenant":   {"stroke": BLUE,    "fill": "#0f1e3a", "grad": "url(#gb)", "label": "租户域",   "fg": "#bfdbfe"},
    "workspace":{"stroke": ACCENT2, "fill": "#0a2620", "grad": "url(#gt)", "label": "工作区域", "fg": "#a7f3e0"},
    "session":  {"stroke": ACCENT,  "fill": "#0a2620", "grad": "url(#gt)", "label": "会话域",   "fg": "#a7f3e0"},
    "approval": {"stroke": GOLD,    "fill": "#2a2110", "grad": "url(#gg)", "label": "审批域",   "fg": "#fde9b8"},
    "billing":  {"stroke": RED,    "fill": "#2a1015", "grad": "url(#gr)", "label": "计费配额域","fg": "#fecaca"},
    "sandbox":  {"stroke": ORANGE, "fill": "#2a1a0a", "grad": "url(#go)", "label": "沙箱域",   "fg": "#fed7aa"},
    "audit":    {"stroke": PURPLE, "fill": "#1a0a2a", "grad": "url(#gp)", "label": "审计域",   "fg": "#e9d5ff"},
    "model":    {"stroke": BLUE,    "fill": "#0f1e3a", "grad": "url(#gb)", "label": "模型域",   "fg": "#bfdbfe"},
    "mcp":      {"stroke": ACCENT2, "fill": "#0a2620", "grad": "url(#gt)", "label": "MCP/连接器域","fg": "#a7f3e0"},
    "skill":    {"stroke": GOLD,    "fill": "#2a2110", "grad": "url(#gg)", "label": "Skills 域","fg": "#fde9b8"},
}

def open_svg(w,h):
    L=[f'<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 {w} {h}">']
    L.append(f'<style>text{{font-family:{FONT}}}</style>')
    L.append('<defs>')
    L.append(f'<marker id="a" viewBox="0 0 10 10" refX="9" refY="5" markerWidth="7" markerHeight="7" orient="auto"><path d="M0 0 L10 5 L0 10 z" fill="{ACCENT}"/></marker>')
    L.append(f'<marker id="ag" viewBox="0 0 10 10" refX="9" refY="5" markerWidth="7" markerHeight="7" orient="auto"><path d="M0 0 L10 5 L0 10 z" fill="{GOLD}"/></marker>')
    L.append(f'<marker id="ab" viewBox="0 0 10 10" refX="9" refY="5" markerWidth="7" markerHeight="7" orient="auto"><path d="M0 0 L10 5 L0 10 z" fill="{BLUE}"/></marker>')
    L.append(f'<marker id="ar" viewBox="0 0 10 10" refX="9" refY="5" markerWidth="7" markerHeight="7" orient="auto"><path d="M0 0 L10 5 L0 10 z" fill="{RED}"/></marker>')
    L.append(f'<marker id="ap" viewBox="0 0 10 10" refX="9" refY="5" markerWidth="7" markerHeight="7" orient="auto"><path d="M0 0 L10 5 L0 10 z" fill="{PURPLE}"/></marker>')
    L.append(f'<marker id="ao" viewBox="0 0 10 10" refX="9" refY="5" markerWidth="7" markerHeight="7" orient="auto"><path d="M0 0 L10 5 L0 10 z" fill="{ORANGE}"/></marker>')
    # 渐变
    L.append(f'<linearGradient id="gt" x1="0" y1="0" x2="1" y2="1"><stop offset="0" stop-color="{ACCENT2}"/><stop offset="1" stop-color="{ACCENT}"/></linearGradient>')
    L.append(f'<linearGradient id="gg" x1="0" y1="0" x2="1" y2="1"><stop offset="0" stop-color="#b07d1a"/><stop offset="1" stop-color="{GOLD}"/></linearGradient>')
    L.append(f'<linearGradient id="gb" x1="0" y1="0" x2="1" y2="1"><stop offset="0" stop-color="#1e3a8a"/><stop offset="1" stop-color="{BLUE}"/></linearGradient>')
    L.append(f'<linearGradient id="gp" x1="0" y1="0" x2="1" y2="1"><stop offset="0" stop-color="#6b21a8"/><stop offset="1" stop-color="{PURPLE}"/></linearGradient>')
    L.append(f'<linearGradient id="gr" x1="0" y1="0" x2="1" y2="1"><stop offset="0" stop-color="#7f1d1d"/><stop offset="1" stop-color="{RED}"/></linearGradient>')
    L.append(f'<linearGradient id="go" x1="0" y1="0" x2="1" y2="1"><stop offset="0" stop-color="#9a3412"/><stop offset="1" stop-color="{ORANGE}"/></linearGradient>')
    L.append('</defs>')
    L.append(f'<rect width="{w}" height="{h}" fill="{BG}"/>')
    return L

def title(L,t,s,y=32):
    L.append(f'<text x="40" y="{y}" fill="{ACCENT}" font-size="22" font-weight="700">{t}</text>')
    if s: L.append(f'<text x="40" y="{y+24}" fill="{MUTED}" font-size="13">{s}</text>')

def cap(L,t,y,w=2400):
    L.append(f'<text x="40" y="{y}" fill="{MUTED}" font-size="12">{t}</text>')

def fin(L,fn):
    L.append('</svg>')
    s='\n'.join(L)
    s=s.replace("&","&amp;")
    open(os.path.join(OUT,fn),'w').write(s); print("wrote",fn)

# ---- ER 实体框 ----
def entity(L,x,y,w,name,fields,domain_key):
    """画一个 ER 实体框：标题栏 + 字段列表。
    fields: [(field_name, annotation, is_pk)] — is_pk=True 在字段前加 PK 标记
    返回 (cx, bottom_y) 供连线。
    """
    d = DOMAIN[domain_key]
    fh = len(fields)*16 + 6
    h = 28 + fh  # 标题栏 28 + 字段区
    # 外框
    L.append(f'<rect x="{x}" y="{y}" width="{w}" height="{h}" rx="6" fill="{d["fill"]}" stroke="{d["stroke"]}" stroke-width="1.4"/>')
    # 标题栏
    L.append(f'<rect x="{x}" y="{y}" width="{w}" height="28" rx="6" fill="{d["grad"]}"/>')
    L.append(f'<rect x="{x}" y="{y+18}" width="{w}" height="10" fill="{d["grad"]}"/>')
    L.append(f'<text x="{x+w/2}" y="{y+19}" text-anchor="middle" fill="#e8f2f0" font-size="13" font-weight="700">{name}</text>')
    # 域标签（右上角小标）
    L.append(f'<text x="{x+w-6}" y="{y+13}" text-anchor="end" fill="{d["fg"]}" font-size="9" opacity="0.7">{d["label"]}</text>')
    # 字段
    fy = y + 24
    for fname, ann, is_pk in fields:
        pk_marker = "PK " if is_pk else "   "
        pk_color = GOLD if is_pk else MUTED
        L.append(f'<text x="{x+10}" y="{fy+12}" fill="{pk_color}" font-size="10.5" font-weight="700">{pk_marker}</text>')
        L.append(f'<text x="{x+38}" y="{fy+12}" fill="{TEXT}" font-size="10.5">{fname}</text>')
        if ann:
            L.append(f'<text x="{x+w-8}" y="{fy+12}" text-anchor="end" fill="{MUTED}" font-size="9">{ann}</text>')
        fy += 16
    return (x + w/2, y + h, y + h/2, x, x + w)  # cx, bottom, mid_y, left, right

def edge(L,x1,y1,x2,y2,color=ACCENT,m="a",label=None,dash=None,w=1.6,curve=False):
    if curve:
        mx=(x1+x2)/2; d=f'M{x1} {y1} C {mx} {y1}, {mx} {y2}, {x2} {y2}'
    else:
        d=f'M{x1} {y1} L{x2} {y2}'
    dd=f' stroke-dasharray="{dash}"' if dash else ''
    L.append(f'<path d="{d}" fill="none" stroke="{color}" stroke-width="{w}" marker-end="url(#{m})"{dd}/>')
    if label:
        lx=(x1+x2)/2; ly=(y1+y2)/2
        L.append(f'<rect x="{lx-26}" y="{ly-10}" width="52" height="16" rx="3" fill="{BG}" stroke="{color}" stroke-width="0.6"/>')
        L.append(f'<text x="{lx}" y="{ly+2}" text-anchor="middle" fill="{color}" font-size="9.5" font-weight="700">{label}</text>')

# ============================================================
# 图1：核心域 ER 总图
# ============================================================
def fig1():
    W,H=2400,1350; L=open_svg(W,H)
    title(L,"Nexus 领域模型 ER 总图 · 核心域",
          "租户域(蓝) → 工作区域(青) → 会话域(青) → 审批域(金) → 计费域(红) · 外键标注基数 1:N / M:N",40)

    # ---- 租户域 ----
    # tenants
    e_tenants = entity(L, 60, 100, 280, "tenants", [
        ("id","BIGSERIAL",True),
        ("name","TEXT",False),
        ("plan","ENUM",False),
        ("isolation_tier","ENUM",False),
        ("cmk_id","TEXT",False),
        ("quota_profile","JSONB",False),
        ("status","ENUM",False),
        ("created_at","TIMESTAMPTZ",False),
    ], "tenant")

    # users
    e_users = entity(L, 420, 100, 280, "users", [
        ("id","BIGSERIAL",True),
        ("tenant_id","FK→tenants",False),
        ("idp_subject","TEXT",False),
        ("email","CITEXT",False),
        ("display_name","TEXT",False),
        ("status","ENUM",False),
    ], "tenant")

    # roles
    e_roles = entity(L, 780, 100, 260, "roles", [
        ("id","BIGSERIAL",True),
        ("tenant_id","FK→tenants",False),
        ("name","TEXT",False),
        ("permissions_json","JSONB",False),
    ], "tenant")

    # tenant_memberships (M:N users↔roles)
    e_mem = entity(L, 1100, 100, 300, "tenant_memberships", [
        ("id","BIGSERIAL",True),
        ("tenant_id","FK→tenants",False),
        ("user_id","FK→users",False),
        ("org_unit_id","FK",False),
        ("role_id","FK→roles",False),
        ("scope_json","JSONB",False),
    ], "tenant")

    # ---- 工作区域 ----
    # workspaces
    e_ws = entity(L, 60, 360, 320, "workspaces", [
        ("id","BIGSERIAL",True),
        ("tenant_id","FK→tenants",False),
        ("name","TEXT",False),
        ("env_tag","ENUM",False),
        ("repos_json","JSONB",False),
        ("connectors_json","JSONB",False),
        ("knowledge_scope_json","JSONB",False),
        ("sandbox_mode","ENUM",False),
        ("approval_policy","ENUM",False),
        ("max_risk_level","ENUM",False),
    ], "workspace")

    # environments
    e_env = entity(L, 460, 360, 260, "environments", [
        ("id","BIGSERIAL",True),
        ("workspace_id","FK→workspaces",False),
        ("name","TEXT",False),
        ("config_json","JSONB",False),
    ], "workspace")

    # knowledge_bases
    e_kb = entity(L, 800, 360, 280, "knowledge_bases", [
        ("id","BIGSERIAL",True),
        ("workspace_id","FK→workspaces",False),
        ("name","TEXT",False),
        ("embedding_model","TEXT",False),
        ("acl_json","JSONB",False),
    ], "workspace")

    # connectors
    e_conn = entity(L, 1160, 360, 300, "connectors", [
        ("id","BIGSERIAL",True),
        ("tenant_id","FK→tenants",False),
        ("name","TEXT",False),
        ("type","ENUM",False),
        ("endpoint","TEXT",False),
        ("auth_mode","ENUM",False),
        ("cred_ref","TEXT",False),
        ("enabled_tools","JSONB",False),
    ], "mcp")

    # ---- 会话域（核心）----
    # threads
    e_thread = entity(L, 60, 620, 360, "threads", [
        ("id","BIGSERIAL",True),
        ("tenant_id","FK→tenants",False),
        ("workspace_id","FK→workspaces",False),
        ("owner_user_id","FK→users",False),
        ("agent_account_id","FK",False),
        ("codex_thread_id","TEXT",False),
        ("title","TEXT",False),
        ("status","ENUM",False),
        ("rollout_object_key","TEXT",False),
        ("rollout_version","INT",False),
        ("permission_snapshot_hash","TEXT",False),
        ("total_tokens","BIGINT",False),
        ("total_cost_micros","BIGINT",False),
        ("created_at","TIMESTAMPTZ",False),
        ("last_active_at","TIMESTAMPTZ",False),
    ], "session")

    # turns
    e_turn = entity(L, 520, 620, 340, "turns", [
        ("id","BIGSERIAL",True),
        ("thread_id","FK→threads",False),
        ("seq","INT",False),
        ("status","ENUM",False),
        ("trigger","ENUM",False),
        ("model","TEXT",False),
        ("sandbox_mode","ENUM",False),
        ("approval_policy","ENUM",False),
        ("input_tokens","INT",False),
        ("output_tokens","INT",False),
        ("cached_tokens","INT",False),
        ("cost_micros","BIGINT",False),
        ("started_at","TIMESTAMPTZ",False),
        ("ended_at","TIMESTAMPTZ",False),
        ("error_code","TEXT",False),
    ], "session")

    # items (最大最热)
    e_item = entity(L, 940, 620, 340, "items", [
        ("id","BIGSERIAL",True),
        ("thread_id","FK→threads",False),
        ("turn_id","FK→turns",False),
        ("seq","INT",False),
        ("kind","ENUM",False),
        ("actor","ENUM",False),
        ("content_ref","TEXT",False),
        ("content_digest","TEXT",False),
        ("summary","TEXT",False),
        ("visibility","ENUM",False),
        ("created_at","TIMESTAMPTZ",False),
        ("UNIQUE(thread_id,turn_id,seq)","★幂等键",False),
    ], "session")

    # steps
    e_step = entity(L, 1380, 620, 300, "steps", [
        ("id","BIGSERIAL",True),
        ("turn_id","FK→turns",False),
        ("seq","INT",False),
        ("sample_status","ENUM",False),
        ("model_output_ref","TEXT",False),
        ("tool_calls_json","JSONB",False),
        ("duration_ms","INT",False),
        ("created_at","TIMESTAMPTZ",False),
    ], "session")

    # ---- 审批域 ----
    e_approval = entity(L, 60, 920, 380, "approval_tickets", [
        ("id","BIGSERIAL",True),
        ("thread_id","FK→threads",False),
        ("turn_id","FK→turns",False),
        ("item_seq","INT",False),
        ("tool_name","TEXT",False),
        ("params_ref","TEXT",False),
        ("params_redacted","JSONB",False),
        ("diff_preview_ref","TEXT",False),
        ("risk_level","ENUM",False),
        ("required_approver_role","TEXT",False),
        ("require_dual","BOOL",False),
        ("status","ENUM",False),
        ("decided_by","FK→users",False),
        ("decided_at","TIMESTAMPTZ",False),
        ("decision_note","TEXT",False),
        ("context_snapshot_ref","TEXT",False),
        ("expires_at","TIMESTAMPTZ",False),
        ("default_action","ENUM",False),
    ], "approval")

    # ---- 计费配额域 ----
    e_usage = entity(L, 520, 920, 360, "usage_records", [
        ("id","BIGSERIAL",True),
        ("tenant_id","FK→tenants",False),
        ("org_unit_id","FK",False),
        ("user_id","FK→users",False),
        ("thread_id","FK→threads",False),
        ("turn_id","FK→turns",False),
        ("metric","ENUM",False),
        ("quantity","NUMERIC",False),
        ("model","TEXT",False),
        ("unit_cost_micros","BIGINT",False),
        ("occurred_at","TIMESTAMPTZ",False),
    ], "billing")

    e_quota = entity(L, 960, 920, 300, "quotas", [
        ("id","BIGSERIAL",True),
        ("tenant_id","FK→tenants",False),
        ("scope","ENUM",False),
        ("metric","ENUM",False),
        ("limit_value","NUMERIC",False),
        ("period","ENUM",False),
        ("used_value","NUMERIC",False),
        ("reset_at","TIMESTAMPTZ",False),
    ], "billing")

    e_budget = entity(L, 1340, 920, 300, "budgets", [
        ("id","BIGSERIAL",True),
        ("tenant_id","FK→tenants",False),
        ("org_unit_id","FK",False),
        ("amount_micros","BIGINT",False),
        ("spent_micros","BIGINT",False),
        ("period","ENUM",False),
        ("alert_threshold","NUMERIC",False),
        ("hard_limit","BOOL",False),
    ], "billing")

    # ---- 关系线 ----
    # tenants 1:N users
    edge(L, e_tenants[3], e_tenants[2], e_users[4], e_users[2], color=BLUE, m="ab", label="1:N")
    # tenants 1:N roles
    edge(L, e_tenants[4], e_tenants[2], e_roles[3], e_roles[2], color=BLUE, m="ab", label="1:N", curve=True)
    # users M:N roles via tenant_memberships
    edge(L, e_users[4], e_users[2], e_mem[3], e_mem[2], color=BLUE, m="ab", label="M:N", curve=True)
    edge(L, e_roles[4], e_roles[2], e_mem[3], e_mem[2], color=BLUE, m="ab", label="1:N")
    # tenants 1:N workspaces
    edge(L, e_tenants[0], e_tenants[1], e_ws[0], e_ws[2], color=ACCENT, m="a", label="1:N", curve=True)
    # workspaces 1:N environments
    edge(L, e_ws[4], e_ws[2], e_env[3], e_env[2], color=ACCENT2, m="a", label="1:N")
    # workspaces 1:N knowledge_bases
    edge(L, e_ws[4], e_ws[2], e_kb[3], e_kb[2], color=ACCENT2, m="a", label="1:N", curve=True)
    # tenants 1:N connectors
    edge(L, e_tenants[0], e_tenants[1]+50, e_conn[0], e_conn[2], color=ACCENT2, m="a", label="1:N", curve=True)
    # workspaces 1:N threads
    edge(L, e_ws[0], e_ws[1], e_thread[0], e_thread[2], color=ACCENT, m="a", label="1:N", curve=True)
    # threads 1:N turns
    edge(L, e_thread[4], e_thread[2], e_turn[3], e_turn[2], color=ACCENT, m="a", label="1:N")
    # turns 1:N items
    edge(L, e_turn[4], e_turn[2], e_item[3], e_item[2], color=ACCENT, m="a", label="1:N")
    # turns 1:N steps
    edge(L, e_turn[4], e_turn[2], e_step[3], e_step[2], color=ACCENT, m="a", label="1:N", curve=True)
    # threads 1:N approval_tickets
    edge(L, e_thread[0], e_thread[1], e_approval[0], e_approval[2], color=GOLD, m="ag", label="1:N", curve=True)
    # turns 1:N approval_tickets
    edge(L, e_turn[0], e_turn[1], e_approval[0]+60, e_approval[2], color=GOLD, m="ag", label="1:N", curve=True)
    # threads 1:N usage_records
    edge(L, e_thread[0]+100, e_thread[1], e_usage[0], e_usage[2], color=RED, m="ar", label="1:N", curve=True)
    # tenants 1:N usage_records
    edge(L, e_tenants[0], e_tenants[1], e_usage[0]+80, e_usage[2], color=RED, m="ar", label="1:N", curve=True)
    # tenants 1:N quotas
    edge(L, e_tenants[0], e_tenants[1]+20, e_quota[0], e_quota[2], color=RED, m="ar", label="1:N", curve=True)
    # tenants 1:N budgets
    edge(L, e_tenants[0], e_tenants[1]+40, e_budget[0], e_budget[2], color=RED, m="ar", label="1:N", curve=True)

    # ---- 图例 ----
    ly = 1250
    L.append(f'<rect x="60" y="{ly}" width="2280" height="60" rx="8" fill="{PANEL}" stroke="{LINE2}"/>')
    lx = 80; ly2 = ly + 24
    for dk in ["tenant","workspace","session","approval","billing"]:
        dd = DOMAIN[dk]
        L.append(f'<rect x="{lx}" y="{ly2-8}" width="16" height="16" rx="3" fill="{dd["fill"]}" stroke="{dd["stroke"]}" stroke-width="1.4"/>')
        L.append(f'<text x="{lx+22}" y="{ly2+4}" fill="{TEXT}" font-size="11">{dd["label"]}</text>')
        lx += 150
    # 基数标注说明
    L.append(f'<text x="{lx}" y="{ly2+4}" fill="{MUTED}" font-size="11">线条标注: 1:N 一对多 · M:N 多对多(经关联表) · PK=主键(金色) · ★ 幂等键</text>')

    cap(L,"图 1 · 核心域 ER 总图（tenants→users→workspaces→threads→turns→items + approval_tickets + usage_records/quotas/budgets）",ly+78,W)
    fin(L,"domain-model-core.svg")

fig1()

# ============================================================
# 图2：治理/周边域放大图（审计/沙箱/模型/MCP/Skills）
# ============================================================
def fig2():
    W,H=2400,1100; L=open_svg(W,H)
    title(L,"Nexus 领域模型 ER · 治理与周边域",
          "审计域(紫·WORM) · 沙箱域(橙) · 模型域(蓝) · MCP/连接器域(青) · Skills域(金) · 关联核心域",40)

    # ---- 审计域 ----
    e_audit = entity(L, 60, 100, 360, "audit_logs", [
        ("id","BIGSERIAL",True),
        ("tenant_id","FK→tenants",False),
        ("actor_type","ENUM",False),
        ("actor_id","TEXT",False),
        ("action","TEXT",False),
        ("resource_type","TEXT",False),
        ("resource_id","TEXT",False),
        ("before_ref","TEXT",False),
        ("after_ref","TEXT",False),
        ("ip","INET",False),
        ("user_agent","TEXT",False),
        ("trace_id","TEXT",False),
        ("occurred_at","TIMESTAMPTZ",False),
        ("★ WORM 禁UPDATE/DELETE","只追加",False),
    ], "audit")

    # ---- 沙箱域 ----
    e_pod = entity(L, 520, 100, 340, "sandbox_pods", [
        ("id","BIGSERIAL",True),
        ("tenant_id","FK→tenants",False),
        ("thread_id","FK→threads",False),
        ("turn_id","FK→turns",False),
        ("pod_name","TEXT",False),
        ("status","ENUM",False),
        ("node","TEXT",False),
        ("cpu_milli","INT",False),
        ("memory_mb","INT",False),
        ("started_at","TIMESTAMPTZ",False),
        ("terminated_at","TIMESTAMPTZ",False),
        ("exit_reason","TEXT",False),
    ], "sandbox")

    e_snap = entity(L, 940, 100, 340, "workspace_snapshots", [
        ("id","BIGSERIAL",True),
        ("workspace_id","FK→workspaces",False),
        ("thread_id","FK→threads",False),
        ("rollout_version","INT",False),
        ("object_key","TEXT",False),
        ("content_digest","TEXT",False),
        ("size_bytes","BIGINT",False),
        ("created_at","TIMESTAMPTZ",False),
        ("★ 对应Codex Rollout","归档对象存储",False),
    ], "sandbox")

    # ---- 模型域 ----
    e_route = entity(L, 60, 460, 360, "model_routes", [
        ("id","BIGSERIAL",True),
        ("tenant_id","FK→tenants",False),
        ("route_name","TEXT",False),
        ("model_id","TEXT",False),
        ("provider","ENUM",False),
        ("priority","INT",False),
        ("fallback_model_id","TEXT",False),
        ("max_tokens","INT",False),
        ("rate_limit_rpm","INT",False),
        ("enabled","BOOL",False),
    ], "model")

    e_cred = entity(L, 520, 460, 340, "model_credentials", [
        ("id","BIGSERIAL",True),
        ("tenant_id","FK→tenants",False),
        ("provider","ENUM",False),
        ("cred_ref","TEXT",False),
        ("api_key_enc","TEXT",False),
        ("org_id","TEXT",False),
        ("enabled","BOOL",False),
        ("last_rotated_at","TIMESTAMPTZ",False),
    ], "model")

    # ---- MCP/连接器域 ----
    e_mcp = entity(L, 940, 460, 340, "mcp_servers", [
        ("id","BIGSERIAL",True),
        ("tenant_id","FK→tenants",False),
        ("workspace_id","FK→workspaces",False),
        ("name","TEXT",False),
        ("transport","ENUM",False),
        ("endpoint","TEXT",False),
        ("command","TEXT",False),
        ("args_json","JSONB",False),
        ("env_redacted","JSONB",False),
        ("tool_whitelist","JSONB",False),
        ("enabled","BOOL",False),
    ], "mcp")

    e_mcp_cred = entity(L, 1360, 460, 340, "mcp_credentials", [
        ("id","BIGSERIAL",True),
        ("tenant_id","FK→tenants",False),
        ("mcp_server_id","FK→mcp_servers",False),
        ("cred_type","ENUM",False),
        ("cred_ref","TEXT",False),
        ("scope_json","JSONB",False),
        ("expires_at","TIMESTAMPTZ",False),
    ], "mcp")

    # ---- Skills 域 ----
    e_skill = entity(L, 60, 820, 340, "skills", [
        ("id","BIGSERIAL",True),
        ("tenant_id","FK→tenants",False),
        ("name","TEXT",False),
        ("description","TEXT",False),
        ("scope","ENUM",False),
        ("status","ENUM",False),
        ("latest_version_id","FK→skill_versions",False),
        ("created_at","TIMESTAMPTZ",False),
    ], "skill")

    e_skill_ver = entity(L, 480, 820, 360, "skill_versions", [
        ("id","BIGSERIAL",True),
        ("skill_id","FK→skills",False),
        ("version","TEXT",False),
        ("content_ref","TEXT",False),
        ("checksum","TEXT",False),
        ("changelog","TEXT",False),
        ("published_by","FK→users",False),
        ("published_at","TIMESTAMPTZ",False),
    ], "skill")

    # tool_call_log (MCP 连接器域)
    e_tool_log = entity(L, 940, 820, 340, "tool_call_logs", [
        ("id","BIGSERIAL",True),
        ("thread_id","FK→threads",False),
        ("turn_id","FK→turns",False),
        ("item_seq","INT",False),
        ("connector_id","FK→connectors",False),
        ("tool_name","TEXT",False),
        ("duration_ms","INT",False),
        ("status","ENUM",False),
        ("error_type","TEXT",False),
        ("cost_micros","BIGINT",False),
    ], "mcp")

    # ---- 关系线 ----
    # tenants 1:N audit_logs
    edge(L, 220, 100, e_audit[0], 100, color=PURPLE, m="ap", label="1:N", curve=True)
    # threads 1:N sandbox_pods
    edge(L, 220, 620, e_pod[0], 100, color=ORANGE, m="ao", label="1:N", curve=True)
    # threads 1:N workspace_snapshots
    edge(L, 220, 640, e_snap[0], 100, color=ORANGE, m="ao", label="1:N", curve=True)
    # workspaces 1:N workspace_snapshots
    edge(L, 460, 440, e_snap[0]+80, 100, color=ORANGE, m="ao", label="1:N", curve=True)
    # tenants 1:N model_routes
    edge(L, 220, 420, e_route[0], 460, color=BLUE, m="ab", label="1:N", curve=True)
    # tenants 1:N model_credentials
    edge(L, 220, 440, e_cred[0], 460, color=BLUE, m="ab", label="1:N", curve=True)
    # model_routes → model_credentials (N:1 provider)
    edge(L, e_route[4], e_route[2], e_cred[3], e_cred[2], color=BLUE, m="ab", label="N:1")
    # tenants 1:N mcp_servers
    edge(L, 220, 480, e_mcp[0], 460, color=ACCENT2, m="a", label="1:N", curve=True)
    # mcp_servers 1:N mcp_credentials
    edge(L, e_mcp[4], e_mcp[2], e_mcp_cred[3], e_mcp_cred[2], color=ACCENT2, m="a", label="1:N")
    # workspaces 1:N mcp_servers
    edge(L, 460, 460, e_mcp[0]+60, 460, color=ACCENT2, m="a", label="1:N", curve=True)
    # tenants 1:N skills
    edge(L, 220, 800, e_skill[0], 820, color=GOLD, m="ag", label="1:N", curve=True)
    # skills 1:N skill_versions
    edge(L, e_skill[4], e_skill[2], e_skill_ver[3], e_skill_ver[2], color=GOLD, m="ag", label="1:N")
    # turns 1:N tool_call_logs
    edge(L, 620, 640, e_tool_log[0], 820, color=ACCENT2, m="a", label="1:N", curve=True)

    # ---- 图例 ----
    ly = 1000
    L.append(f'<rect x="60" y="{ly}" width="2280" height="56" rx="8" fill="{PANEL}" stroke="{LINE2}"/>')
    lx = 80; ly2 = ly + 24
    for dk in ["audit","sandbox","model","mcp","skill"]:
        dd = DOMAIN[dk]
        L.append(f'<rect x="{lx}" y="{ly2-8}" width="16" height="16" rx="3" fill="{dd["fill"]}" stroke="{dd["stroke"]}" stroke-width="1.4"/>')
        L.append(f'<text x="{lx+22}" y="{ly2+4}" fill="{TEXT}" font-size="11">{dd["label"]}</text>')
        lx += 160
    L.append(f'<text x="{lx}" y="{ly2+4}" fill="{MUTED}" font-size="11">audit_logs 只追加(WORM) · workspace_snapshots 对应 Codex Rollout 归档对象存储 · 所有表 RLS 按 tenant_id 隔离</text>')

    cap(L,"图 2 · 治理与周边域 ER（audit_logs WORM · sandbox_pods/workspace_snapshots · model_routes/credentials · mcp_servers/credentials · skills/skill_versions · tool_call_logs）",ly+74,W)
    fin(L,"domain-model-governance.svg")

fig2()
print("DONE")
