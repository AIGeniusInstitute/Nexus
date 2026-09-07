import React from "react";

export function Card({ title, action, children }: { title?: React.ReactNode; action?: React.ReactNode; children: React.ReactNode }) {
  return (
    <div className="card">
      {(title || action) && (
        <div className="card-h"><h3>{title}</h3>{action}</div>
      )}
      <div className="card-b">{children}</div>
    </div>
  );
}

export function Table<T>({ cols, rows }: { cols: { key: string; label: string; render?: (r: T) => React.ReactNode; className?: string }[]; rows: T[] }) {
  return (
    <div className="scroll-x">
      <table>
        <thead><tr>{cols.map((c) => <th key={c.key}>{c.label}</th>)}</tr></thead>
        <tbody>
          {rows.map((r, i) => (
            <tr key={i}>{cols.map((c) => <td key={c.key} className={c.className}>{c.render ? c.render(r) : (r as any)[c.key]}</td>)}</tr>
          ))}
        </tbody>
      </table>
    </div>
  );
}

export function Button({ children, onClick, variant = "default", disabled, className, type, style }: {
  children: React.ReactNode; onClick?: () => void; variant?: "default" | "primary" | "danger";
  disabled?: boolean; className?: string; type?: "button" | "submit"; style?: React.CSSProperties;
}) {
  return <button type={type || "button"} className={`btn ${variant === "default" ? "" : variant} ${className || ""}`} onClick={onClick} disabled={disabled} style={style}>{children}</button>;
}

export function Pill({ tone = "mut", children }: { tone?: "ok" | "warn" | "danger" | "info" | "mut"; children: React.ReactNode }) {
  return <span className={`pill ${tone}`}>{children}</span>;
}

export function Empty({ children = "暂无数据" }: { children?: React.ReactNode }) {
  return <div className="empty">{children}</div>;
}

export function ErrBar({ err }: { err: string | null }) {
  if (!err) return null;
  return <div className="err">{err}</div>;
}

export function Field({ label, children, hint }: { label: string; children: React.ReactNode; hint?: string }) {
  return (
    <div className="field">
      <label>{label}</label>
      {children}
      {hint && <div className="hint">{hint}</div>}
    </div>
  );
}

export function Modal({ title, onClose, children }: { title: string; onClose: () => void; children: React.ReactNode }) {
  return (
    <div className="modal-bg" onClick={onClose}>
      <div className="modal" onClick={(e) => e.stopPropagation()}>
        <div className="modal-h"><h3>{title}</h3><Button variant="danger" className="sm" onClick={onClose}>✕</Button></div>
        <div className="modal-b">{children}</div>
      </div>
    </div>
  );
}

export function useAsync<T>(fn: () => Promise<T>, deps: any[] = []): { data: T | null; loading: boolean; err: string | null; reload: () => void } {
  const [data, setData] = React.useState<T | null>(null);
  const [loading, setLoading] = React.useState(true);
  const [err, setErr] = React.useState<string | null>(null);
  const [tick, setTick] = React.useState(0);
  React.useEffect(() => {
    let alive = true;
    setLoading(true); setErr(null);
    fn().then((d) => { if (alive) { setData(d); setLoading(false); } })
      .catch((e) => { if (alive) { setErr(String(e?.message || e)); setLoading(false); } });
    return () => { alive = false; };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [...deps, tick]);
  return { data, loading, err, reload: () => setTick((t) => t + 1) };
}

export function fmtNum(n: number | null | undefined): string {
  if (n == null) return "—";
  if (n >= 1e9) return (n / 1e9).toFixed(2) + "B";
  if (n >= 1e6) return (n / 1e6).toFixed(2) + "M";
  if (n >= 1e3) return (n / 1e3).toFixed(1) + "K";
  return String(n);
}

export function fmtCost(micros: number): string {
  if (!micros) return "0";
  return (micros / 1e6).toFixed(4);
}

export function fmtTime(s: string): string {
  if (!s) return "—";
  const d = new Date(s);
  if (isNaN(d.getTime())) return s;
  return d.toLocaleString("zh-CN", { month: "2-digit", day: "2-digit", hour: "2-digit", minute: "2-digit", second: "2-digit" });
}

// ---- Agent Studio: CUI 渲染原语 ----

/// 极简 markdown 渲染（React 元素，无 innerHTML → 无 XSS）。覆盖代码块 /
/// 行内代码 / 粗体 / 链接 / 标题 / 列表 / 段落 —— CUI 答案的核心元素。
export function Markdown({ text }: { text: string }) {
  if (!text) return null;
  return <div className="md">{renderMd(text)}</div>;
}

function renderMd(text: string): React.ReactNode[] {
  const nodes: React.ReactNode[] = [];
  const blocks = text.split("```");
  blocks.forEach((blk, i) => {
    if (i % 2 === 1) {
      const lines = blk.replace(/^\n/, "").replace(/\s+$/, "").split("\n");
      let lang = "";
      const first = lines[0] ? lines[0].trim() : "";
      if (first && /^[a-zA-Z0-9+#.-]+$/.test(first) && lines.length > 1) {
        lang = first; lines.shift();
      }
      nodes.push(
        <pre key={`c${i}`} className="md-code" data-lang={lang}>
          <code>{lines.join("\n")}</code>
        </pre>
      );
    } else {
      blk.split(/\n\n+/).forEach((para, pi) => {
        const t = para.trim();
        if (!t) return;
        const hm = t.match(/^(#{1,6})\s+(.*)$/);
        if (hm) {
          nodes.push(React.createElement(`h${hm[1].length}`, { key: `${i}-${pi}`, className: "md-h" }, renderInline(hm[2])));
          return;
        }
        const lines = t.split("\n");
        if (lines.every((l) => /^\s*[-*]\s+/.test(l))) {
          nodes.push(
            <ul key={`${i}-${pi}`} className="md-ul">
              {lines.map((l, li) => <li key={li}>{renderInline(l.replace(/^\s*[-*]\s+/, ""))}</li>)}
            </ul>
          );
          return;
        }
        nodes.push(<p key={`${i}-${pi}`} className="md-p">{renderInline(t.replace(/\n/g, " "))}</p>);
      });
    }
  });
  return nodes;
}

function renderInline(text: string): React.ReactNode[] {
  const nodes: React.ReactNode[] = [];
  let rest = text; let key = 0;
  const re = /(`([^`]+)`)|(\*\*([^*]+)\*\*)|(\[([^\]]+)\]\(([^)]+)\))/;
  while (rest) {
    const m = re.exec(rest);
    if (!m) { nodes.push(rest); break; }
    if (m.index > 0) nodes.push(rest.slice(0, m.index));
    if (m[2]) nodes.push(<code key={key++} className="md-ic">{m[2]}</code>);
    else if (m[4]) nodes.push(<strong key={key++}>{m[4]}</strong>);
    else if (m[6]) nodes.push(<a key={key++} href={m[7]} target="_blank" rel="noreferrer">{m[6]}</a>);
    rest = rest.slice(m.index + m[0].length);
  }
  return nodes;
}

/// 思考过程折叠面板（琥珀色，默认折叠）。
export function ReasoningBlock({ text }: { text: string }) {
  const [open, setOpen] = React.useState(false);
  if (!text) return null;
  return (
    <div className="reasoning">
      <div className="reasoning-h" onClick={() => setOpen(!open)}>
        <span className="reasoning-ic">{open ? "▼" : "▶"}</span>
        <span>思考过程</span>
        <span className="reasoning-len">{text.length} 字符</span>
      </div>
      {open && <div className="reasoning-b"><Markdown text={text} /></div>}
    </div>
  );
}

/// 工具调用卡片：标题 + children（参数/输出）。
export function ToolCard({ title, badge, children }: { title: string; badge?: React.ReactNode; children: React.ReactNode }) {
  return (
    <div className="tool-card">
      <div className="tool-card-h">
        <span className="tool-ic">🔧</span>
        <span className="tool-title">{title}</span>
        {badge && <span className="tool-badge">{badge}</span>}
      </div>
      <div className="tool-card-b">{children}</div>
    </div>
  );
}

/// KV 行（参数/结果展示）。
export function KV({ k, v }: { k: string; v: React.ReactNode }) {
  return (
    <div className="kv">
      <span className="kv-k">{k}</span>
      <span className="kv-v">{v}</span>
    </div>
  );
}

/// 代码块（预格式化输出，如命令输出）。
export function CodeBlock({ text, maxHeight }: { text: string; maxHeight?: number }) {
  if (!text) return null;
  return (
    <pre className="tool-output" style={maxHeight ? { maxHeight, overflow: "auto" } : undefined}>
      <code>{text}</code>
    </pre>
  );
}
