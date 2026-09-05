# Nexus 架构产物生成规范（archify 方法论）

> 所有任务二维度产物遵循本规范。每个维度目录 `~/Nexus/docs/architecture/0X-xxx/` 下产出三件套：
> a. `{name}.md` — 详细方案 markdown
> b. `{name}.svg` + `.png` — 架构图（SVG 源 + PNG 位图）
> c. `{name}-report.html` — 交互报告（单文件，内嵌图）

## 参考文件（必读）
- 路线图：`~/Nexus/docs/architecture/Nexus 基于CodexHarness的企业级Agent平台_系统设计与实施路线图.md`
- 系统架构样例：`~/Nexus/docs/architecture/01-system-architecture/system-architecture.md`
- archify 样例脚本：`~/Nexus/docs/architecture/01-system-architecture/_gen_svg.py`（深色主题，node/edge/fin helper）
- 源码：`~/Nexus/codex-rs/`（106 crate），`~/Nexus/docs/codex_docs/`（config.md/execpolicy.md/sandbox.md/skills.md）
- app-server API：`~/Nexus/codex-rs/app-server/README.md`

## SVG 生成方法（archify 风格）
用 Python 脚本 `_gen_svg.py` 生成 SVG。深色主题色板：
```
BG="#0b141a"; PANEL="#13242e"; PANEL2="#173341"; TEXT="#e8f2f0"; MUTED="#9db8b4"
LINE="#24414e"; LINE2="#2e5a68"; ACCENT="#35c2b0"; ACCENT2="#028090"; GOLD="#e8b64c"
BLUE="#3b82f6"; RED="#ef4444"; PURPLE="#a855f7"
FONT="'Helvetica Neue',Helvetica,Arial,'PingFang SC','Microsoft YaHei','SimHei',sans-serif"
```
helper：`open_svg(w,h)`、`title(L,t,s,y)`、`node(L,x,y,w,h,t,...)`、`edge(L,x1,y1,x2,y2,...)`、`cap(L,t,y)`、`fin(L,fn)`。
**关键**：`fin` 写文件前必须 `s=s.replace("&","&amp;")` 转义裸 `&`，否则 rsvg-convert XML parse error。

转 PNG：`rsvg-convert -w 2400 {name}.svg -o {name}.png`（CJK 用 rsvg-convert 避免 cairosvg 中文字体问题）。

## HTML 交互报告要求
单文件，内嵌 CSS+JS，无外链依赖。必须包含：
- 深色/浅色双主题切换（顶部按钮，localStorage 记忆）
- 滚动进度条（顶部固定）
- 左侧 TOC 目录跳转（粘性，当前节高亮）
- 折叠面板（`<details>` 或自定义）展示分层/表格
- 内嵌架构图：`<img src="{name}.svg">` 或 `.png`（同目录相对路径）
- 响应式布局，Mermaid 不用（用 SVG）
- 内容来自对应 md，结构化呈现（表格、列表、代码块）
- 文件名 `{name}-report.html`

## 内容要求
- 基于路线图 + 源码调研，生产级、可落地
- 列具体 crate 名、文件路径、API 方法名作证据
- 与八层架构对齐（L1 接入/L2 网关/L3 控制面/L4 执行面/L5 Harness/L6 模型/L7 存储/安全贯穿）
- 每张 SVG 图配 `cap()` 图注说明
