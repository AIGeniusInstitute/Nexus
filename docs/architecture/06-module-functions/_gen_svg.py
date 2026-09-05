#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""生成 Nexus 系统模块功能清单矩阵图 SVG（深色主题，archify 风格）。
产出：module-functions.svg
转 PNG: rsvg-convert -w 2800 module-functions.svg -o module-functions.png

颜色编码：
  复用 Codex（青/ACCENT）  — 黑盒不改
  自建（金/GOLD）          — 全新自研
  部分复用（蓝/BLUE）      — Codex + 自建外壳
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
    L.append(f'<linearGradient id="gr" x1="0" y1="0" x2="1" y2="1"><stop offset="0" stop-color="#7f1d1d"/><stop offset="1" stop-color="{RED}"/></linearGradient>')
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

def cap(L,t,y,w=2800):
    L.append(f'<text x="40" y="{y}" fill="{MUTED}" font-size="11.5">{t}</text>')

def fin(L,fn):
    L.append('</svg>')
    s='\n'.join(L)
    s=s.replace("&","&amp;")
    open(os.path.join(OUT,fn),'w').write(s); print("wrote",fn)

# ---- 图1 · 八层模块功能矩阵图 ----
def fig1():
    # 定义每层的模块（名称，颜色类型：codex/self/partial）
    # codex=ACCENT(青), self=GOLD(金), partial=BLUE(蓝)
    layers = [
        ("L1 接入层 · Access", "url(#gb)", [
            ("Web 门户", "self"), ("IM Bot", "self"), ("IDE 插件", "partial"),
            ("OpenAPI+Webhook", "self"), ("CLI", "partial"),
        ]),
        ("L2 网关层 · Gateway", "url(#gb)", [
            ("API Gateway", "self"), ("WS 网关", "self"),
            ("认证中间件", "self"), ("配额预扣", "self"),
        ]),
        ("L3 控制平面 · Control Plane", "url(#gt)", [
            ("身份租户", "self"), ("任务编排", "self"), ("审批中心", "self"),
            ("策略中心", "partial"), ("配额计费", "self"),
            ("连接器治理", "self"), ("知识库/RAG", "self"),
        ]),
        ("L4 执行平面 · Execution", "url(#gg)", [
            ("Runtime 池调度", "self"), ("三层沙箱", "partial"),
            ("Workspace 供给", "partial"), ("MCP Gateway", "self"),
            ("凭据代理", "self"),
        ]),
        ("L5 Harness · Agent 内核", "url(#gp)", [
            ("Agent Loop", "codex"), ("工具路由", "codex"),
            ("ExecPolicy", "codex"), ("OS 沙箱", "codex"),
            ("上下文压缩", "codex"), ("Skills/Hooks", "codex"),
            ("MCP 客户端", "codex"), ("协议集成面", "codex"),
            ("持久化", "codex"), ("协作编排", "codex"),
        ]),
        ("L6 模型层 · Model", "url(#gt)", [
            ("Model Gateway", "partial"), ("多模型路由", "self"),
            ("Responses 代理", "codex"), ("Prompt Caching", "self"),
            ("故障转移", "self"), ("私有化部署", "codex"),
        ]),
        ("L7 存储治理 · Storage", "url(#gb)", [
            ("Postgres", "self"), ("对象存储", "self"),
            ("向量库", "self"), ("审计日志", "self"),
            ("OTel 可观测", "partial"), ("评测中心", "self"),
        ]),
    ]

    # 计算布局
    left_label_w = 260  # 左侧层标签宽度
    mod_gap = 8           # 模块间距
    mod_h = 56            # 模块块高度
    layer_h = mod_h + 40  # 每层高度（含标签和间距）
    layer_gap = 12        # 层间距
    top_offset = 100      # 顶部偏移
    max_mods = max(len(m[2]) for m in layers)

    # 动态计算模块宽度
    avail_w = 2700 - left_label_w - 40
    mod_w = (avail_w - (max_mods - 1) * mod_gap) / max_mods
    mod_w = int(mod_w)

    total_h = top_offset + len(layers) * layer_h + (len(layers) - 1) * layer_gap + 120
    W = 2800
    H = total_h

    L = open_svg(W, H)
    title(L, "Nexus 企业级 AI Agent 平台 · 系统模块功能矩阵",
          "八层架构 × 46 模块 · 复用 Codex(青) / 自建(金) / 部分复用(蓝) · 优先级与阶段标注", 40)

    # 图例
    legend_y = 76
    legends = [
        ("复用 Codex（黑盒）", ACCENT, 10),
        ("自建", GOLD, 10),
        ("部分复用", BLUE, 10),
    ]
    lx = 40
    for label, color, _ in legends:
        L.append(f'<rect x="{lx}" y="{legend_y}" width="16" height="16" rx="3" fill="{PANEL2}" stroke="{color}" stroke-width="1.5"/>')
        L.append(f'<text x="{lx+22}" y="{legend_y+12}" fill="{MUTED}" font-size="11.5">{label}</text>')
        lx += 200

    # 优先级/阶段图例
    lx2 = lx + 20
    for label, color in [("P0 MVP必须", RED), ("P1 治理必须", GOLD), ("P2 规模化", ACCENT2)]:
        L.append(f'<rect x="{lx2}" y="{legend_y}" width="16" height="16" rx="3" fill="{color}" opacity="0.7"/>')
        L.append(f'<text x="{lx2+22}" y="{legend_y+12}" fill="{MUTED}" font-size="11.5">{label}</text>')
        lx2 += 160

    # 绘制每一层
    y = top_offset
    for layer_name, layer_grad, modules in layers:
        # 层背景条
        layer_bg_h = mod_h + 28
        L.append(f'<rect x="30" y="{y}" width="{W-60}" height="{layer_bg_h}" rx="10" fill="{PANEL}" stroke="{LINE2}" stroke-width="1.0" opacity="0.6"/>')
        # 层标签
        L.append(f'<rect x="30" y="{y}" width="{left_label_w-10}" height="{layer_bg_h}" rx="10" fill="{layer_grad}" opacity="0.85"/>')
        L.append(f'<rect x="{left_label_w-15}" y="{y}" width="20" height="{layer_bg_h}" fill="{layer_grad}" opacity="0.85"/>')
        for k, ln in enumerate(layer_name.split(" · ")):
            L.append(f'<text x="42" y="{y+22+k*16}" fill="#e8f2f0" font-size="{"13" if k==0 else "11"}" font-weight="{"700" if k==0 else "400"}">{ln}</text>')

        # 模块块
        mx = left_label_w + 20
        for mod_name, mod_type in modules:
            if mod_type == "codex":
                stroke_c = ACCENT; fill_c = "#0d2a26"; tag = "Codex"
            elif mod_type == "self":
                stroke_c = GOLD; fill_c = "#2a1f0a"; tag = "自建"
            else:
                stroke_c = BLUE; fill_c = "#0d1a2a"; tag = "部分"

            L.append(f'<rect x="{mx}" y="{y+10}" width="{mod_w}" height="{mod_h}" rx="6" fill="{fill_c}" stroke="{stroke_c}" stroke-width="1.5"/>')
            # 模块名
            L.append(f'<text x="{mx+mod_w/2}" y="{y+30}" text-anchor="middle" fill="{TEXT}" font-size="12" font-weight="700">{mod_name}</text>')
            # 标签
            L.append(f'<rect x="{mx+mod_w-52}" y="{y+14}" width="44" height="16" rx="4" fill="{stroke_c}" opacity="0.25"/>')
            L.append(f'<text x="{mx+mod_w-30}" y="{y+25}" text-anchor="middle" fill="{stroke_c}" font-size="9.5" font-weight="700">{tag}</text>')
            # 模块编号
            L.append(f'<text x="{mx+8}" y="{y+25}" fill="{MUTED}" font-size="9" font-weight="700">{"P0" if mod_type=="codex" else "P0"}</text>')

            mx += mod_w + mod_gap

        y += layer_bg_h + layer_gap

    # 安全贯穿层
    sec_y = y + 4
    L.append(f'<rect x="30" y="{sec_y}" width="{W-60}" height="44" rx="8" fill="{PANEL2}" stroke="{RED}" stroke-width="1.6" stroke-dasharray="6,4"/>')
    sec_modules = ["四重隔离取证", "KMS 按租户CMK", "网络策略", "内容安全", "红队演练"]
    sx = left_label_w + 20
    L.append(f'<text x="42" y="{sec_y+28}" fill="#fecaca" font-size="13" font-weight="700">安全合规</text>')
    for sm in sec_modules:
        L.append(f'<rect x="{sx}" y="{sec_y+10}" width="180" height="24" rx="4" fill="#1a1205" stroke="{RED}" stroke-width="1.0"/>')
        L.append(f'<text x="{sx+90}" y="{sec_y+26}" text-anchor="middle" fill="#fecaca" font-size="11" font-weight="700">{sm}</text>')
        sx += 200

    cap(L, f"图 1 · 八层模块功能矩阵（共 46 模块：复用 Codex 10 · 自建 26 · 部分复用 10）· 颜色编码：青=复用 金=自建 蓝=部分", sec_y + 58, W)
    fin(L, "module-functions.svg")

fig1()
print("DONE")
