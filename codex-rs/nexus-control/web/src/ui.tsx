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
