#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""生成 Nexus 测试矩阵图 SVG（深色主题，archify 风格）。
横轴：9 类测试类型；纵轴：阶段 M0-M12；单元格标注 P0/P1/P2。
转 PNG: rsvg-convert -w 2400 test-matrix.svg -o test-matrix.png
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
    L.append(f'<linearGradient id="gr" x1="0" y1="0" x2="1" y2="1"><stop offset="0" stop-color="#991b1b"/><stop offset="1" stop-color="{RED}"/></linearGradient>')
    L.append('</defs>')
    L.append(f'<rect width="{w}" height="{h}" fill="{BG}"/>')
    return L

def title(L,t,s,y=32):
    L.append(f'<text x="40" y="{y}" fill="{ACCENT}" font-size="22" font-weight="700">{t}</text>')
    if s: L.append(f'<text x="40" y="{y+24}" fill="{MUTED}" font-size="13">{s}</text>')

def cap(L,t,y,w=1600):
    L.append(f'<text x="40" y="{y}" fill="{MUTED}" font-size="11.5">{t}</text>')

def fin(L,fn):
    L.append('</svg>')
    s='\n'.join(L)
    s=s.replace("&","&amp;")
    open(os.path.join(OUT,fn),'w').write(s); print("wrote",fn)

# ---- 测试矩阵图 ----
def fig_matrix():
    # 9 test types x 13 milestones
    types = [
        "单元测试\n(*_tests.rs)",
        "集成测试\n(core/suite)",
        "快照测试\n(insta)",
        "协议测试\n(JSON-RPC)",
        "端到端\n(PoC三假设)",
        "安全测试\n(红队/越权)",
        "评测体系\n(黄金集)",
        "性能测试\n(并发/延迟)",
        "合规测试\n(审计/WORM)",
    ]
    milestones = ["M0","M1","M2","M3","M4","M5","M6","M7","M8","M9","M10","M11","M12"]

    # matrix[r][c] = priority for milestone c, test type r
    # P0=must, P1=should, P2=nice, ""=N/A
    matrix = [
        # M0  M1  M2  M3  M4  M5  M6  M7  M8  M9  M10 M11 M12
        ["",  "P1","P0","P0","P0","P0","P0","P0","P0","P0","P0","P0","P0"],  # 单元
        ["P0","P0","P0","P0","P0","P0","P0","P0","P0","P0","P0","P0","P0"],  # 集成
        ["",  "P1","P1","P0","P0","P1","P1","P0","P0","P0","P0","P0","P0"],  # 快照
        ["P0","P0","P0","P0","P0","P0","P0","P0","P0","P0","P0","P0","P0"],  # 协议
        ["P0","P1","P0","P1","P0","P0","P0","P0","P1","P0","P0","P0","P0"],  # E2E
        ["",  "",  "P2","P0","P1","P0","P1","P0","P1","P1","P0","P0","P0"],  # 安全
        ["",  "",  "",  "P2","P1","P1","P1","P1","P0","P0","P0","P1","P1"],  # 评测
        ["",  "",  "P2","",  "P2","P2","P1","P1","P2","P0","P2","P0","P0"],  # 性能
        ["",  "",  "",  "P1","P1","P0","P1","P1","P1","P1","P0","P1","P0"],  # 合规
    ]

    cell_w = 100
    cell_h = 44
    label_w = 180
    header_h = 50
    margin_top = 100
    margin_left = 40

    W = margin_left + label_w + len(milestones) * cell_w + 40
    H = margin_top + header_h + len(types) * cell_h + 60

    L = open_svg(W, H)
    title(L, "Nexus 测试矩阵 · 测试类型 x 里程碑",
          "横轴：M0-M13 阶段 | 纵轴：9 类测试 | 单元格：P0(必须) / P1(应该) / P2(建议)", 40)

    # 列头 - milestones
    x0 = margin_left + label_w
    for j, m in enumerate(milestones):
        x = x0 + j * cell_w
        # color by stage
        if m == "M0":
            grad = "url(#gp)"
        elif m in ("M1","M2","M3","M4"):
            grad = "url(#gb)"
        elif m in ("M5","M6","M7"):
            grad = "url(#gr)"
        elif m in ("M8","M9","M10"):
            grad = "url(#gg)"
        else:
            grad = "url(#gt)"
        L.append(f'<rect x="{x}" y="{margin_top}" width="{cell_w}" height="{header_h}" rx="6" fill="{grad}" stroke="{LINE2}" stroke-width="1"/>')
        L.append(f'<text x="{x+cell_w/2}" y="{margin_top+header_h/2+5}" text-anchor="middle" fill="#e8f2f0" font-size="14" font-weight="700">{m}</text>')

    # 阶段分隔标记
    stage_marks = [
        (0, 0, "PoC", "#a855f7"),
        (1, 4, "MVP", "#3b82f6"),
        (5, 7, "隔离", "#ef4444"),
        (8, 10, "治理", "#e8b64c"),
        (11, 12, "规模", "#35c2b0"),
    ]
    for start, end, label, color in stage_marks:
        x1 = x0 + start * cell_w - 2
        x2 = x0 + (end + 1) * cell_w - 2
        y = margin_top + header_h + 4
        L.append(f'<line x1="{x1}" y1="{y}" x2="{x2}" y2="{y}" stroke="{color}" stroke-width="3" stroke-linecap="round"/>')
        L.append(f'<text x="{(x1+x2)/2}" y="{y-4}" text-anchor="middle" fill="{color}" font-size="10" font-weight="700">{label}</text>')

    # 行头 - test types
    for i, t in enumerate(types):
        y = margin_top + header_h + 14 + i * cell_h
        L.append(f'<rect x="{margin_left}" y="{y}" width="{label_w}" height="{cell_h-4}" rx="6" fill="{PANEL2}" stroke="{LINE2}" stroke-width="1"/>')
        lines = t.split("\n")
        for k, ln in enumerate(lines):
            fs = 12 if k == 0 else 10
            fw = "700" if k == 0 else "400"
            clr = TEXT if k == 0 else MUTED
            L.append(f'<text x="{margin_left+12}" y="{y+cell_h/2-2+k*14}" fill="{clr}" font-size="{fs}" font-weight="{fw}">{ln}</text>')

    # 单元格
    p0_fill = "#1a3a2a"
    p0_stroke = ACCENT
    p1_fill = "#1a3041"
    p1_stroke = BLUE
    p2_fill = "#2a2010"
    p2_stroke = GOLD
    for i, row in enumerate(matrix):
        for j, val in enumerate(row):
            x = x0 + j * cell_w
            y = margin_top + header_h + 14 + i * cell_h
            if val == "P0":
                fill, stroke, fg = p0_fill, p0_stroke, ACCENT
            elif val == "P1":
                fill, stroke, fg = p1_fill, p1_stroke, BLUE
            elif val == "P2":
                fill, stroke, fg = p2_fill, p2_stroke, GOLD
            else:
                fill, stroke, fg = PANEL, LINE, MUTED
            L.append(f'<rect x="{x+3}" y="{y+2}" width="{cell_w-6}" height="{cell_h-8}" rx="4" fill="{fill}" stroke="{stroke}" stroke-width="1"/>')
            if val:
                L.append(f'<text x="{x+cell_w/2}" y="{y+cell_h/2+4}" text-anchor="middle" fill="{fg}" font-size="13" font-weight="700">{val}</text>')

    # 图例
    legend_y = margin_top + header_h + 14 + len(types) * cell_h + 10
    items = [("P0 必须", ACCENT, p0_fill), ("P1 应该", BLUE, p1_fill), ("P2 建议", GOLD, p2_fill), ("N/A", MUTED, PANEL)]
    lx = margin_left
    for label_text, sc, fc in items:
        L.append(f'<rect x="{lx}" y="{legend_y}" width="18" height="14" rx="3" fill="{fc}" stroke="{sc}" stroke-width="1"/>')
        L.append(f'<text x="{lx+24}" y="{legend_y+12}" fill="{MUTED}" font-size="11">{label_text}</text>')
        lx += 100

    cap(L, "图 · 测试矩阵（M0 PoC 验证三假设 → M1-M4 单租户 MVP → M5-M7 多租户隔离 → M8-M10 治理 → M11-M12 规模化）", legend_y + 30, W)
    fin(L, "test-matrix.svg")

fig_matrix()
print("DONE")
