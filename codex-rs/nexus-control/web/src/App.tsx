import React from "react";
import { api, getToken, setToken, clearToken, openThreadStream, type LoginResp } from "./api";
import { Button, Pill, useAsync } from "./ui";

import Overview from "./pages/Overview";
import Threads from "./pages/Threads";
import Approvals from "./pages/Approvals";
import Usage from "./pages/Usage";
import KnowledgeBase from "./pages/KnowledgeBase";
import Connectors from "./pages/Connectors";
import Skills from "./pages/Skills";
import Orchestration from "./pages/Orchestration";
import Evals from "./pages/Evals";
import Audit from "./pages/Audit";
import Policy from "./pages/Policy";

const NAV: { group: string; items: { key: string; label: string; icon: string }[] }[] = [
  { group: "运行", items: [
    { key: "overview", label: "概览", icon: "◎" },
    { key: "threads", label: "会话", icon: "💬" },
    { key: "approvals", label: "审批", icon: "✓" },
    { key: "orchestration", label: "协作编排", icon: "🧩" },
  ]},
  { group: "知识与技能", items: [
    { key: "kb", label: "知识库", icon: "📚" },
    { key: "skills", label: "技能市场", icon: "⚡" },
    { key: "connectors", label: "连接器", icon: "🔌" },
  ]},
  { group: "治理", items: [
    { key: "usage", label: "用量计量", icon: "📊" },
    { key: "policy", label: "策略", icon: "🛡" },
    { key: "evals", label: "评测", icon: "🎯" },
    { key: "audit", label: "审计日志", icon: "📜" },
  ]},
];

function Login() {
  const [email, setEmail] = React.useState("admin@nexus.local");
  const [pw, setPw] = React.useState("admin123");
  const [err, setErr] = React.useState<string | null>(null);
  const [busy, setBusy] = React.useState(false);
  async function submit(e: React.FormEvent) {
    e.preventDefault();
    setBusy(true); setErr(null);
    try {
      const r: LoginResp = await api.login(email, pw);
      setToken(r.token);
      location.hash = "#/overview";
      location.reload();
    } catch (e: unknown) { setErr(String(e)); }
    finally { setBusy(false); }
  }
  return (
    <div style={{ minHeight: "100vh", display: "flex", alignItems: "center", justifyContent: "center", background: "var(--bg)" }}>
      <div style={{ width: 380 }}>
        <div style={{ textAlign: "center", marginBottom: 28 }}>
          <div style={{ fontSize: 34, fontWeight: 800, letterSpacing: 1,
            background: "linear-gradient(120deg, var(--acc), var(--gold))", WebkitBackgroundClip: "text", backgroundClip: "text", WebkitTextFillColor: "transparent" }}>
            Nexus
          </div>
          <div style={{ color: "var(--mut)", fontSize: 13, marginTop: 6 }}>企业级 Agent-Native 平台</div>
        </div>
        <form onSubmit={submit} className="card" style={{ padding: 22 }}>
          <div className="field"><label>邮箱</label>
            <input className="input" value={email} onChange={(e) => setEmail(e.target.value)} /></div>
          <div className="field"><label>密码</label>
            <input className="input" type="password" value={pw} onChange={(e) => setPw(e.target.value)} /></div>
          <Button type="submit" variant="primary" disabled={busy} style={{ width: "100%" as any, justifyContent: "center" }}>{busy ? "登录中…" : "登录"}</Button>
          {err && <div className="err" style={{ marginTop: 10 }}>{err}</div>}
        </form>
      </div>
    </div>
  );
}

function Shell() {
  const [route, setRoute] = React.useState(location.hash.replace(/^#\/?/, "") || "overview");
  React.useEffect(() => {
    const onHash = () => setRoute(location.hash.replace(/^#\/?/, "") || "overview");
    window.addEventListener("hashchange", onHash);
    return () => window.removeEventListener("hashchange", onHash);
  }, []);
  const me = useAsync(() => api.me(), []);
  const cur = NAV.flatMap((g) => g.items).find((i) => i.key === route) || NAV[0].items[0];

  function page() {
    switch (route) {
      case "overview": return <Overview />;
      case "threads": return <Threads />;
      case "approvals": return <Approvals />;
      case "orchestration": return <Orchestration />;
      case "kb": return <KnowledgeBase />;
      case "skills": return <Skills />;
      case "connectors": return <Connectors />;
      case "usage": return <Usage />;
      case "policy": return <Policy />;
      case "evals": return <Evals />;
      case "audit": return <Audit />;
      default: return <Overview />;
    }
  }
  function logout() { clearToken(); location.reload(); }

  return (
    <div className="app">
      <aside className="sidebar">
        <div className="brand">
          <div className="logo">Nexus</div>
          <div className="sub">Agent-Native Platform · M19</div>
        </div>
        <nav className="nav">
          {NAV.map((g) => (
            <React.Fragment key={g.group}>
              <div className="nav-group">{g.group}</div>
              {g.items.map((it) => (
                <a key={it.key} className={route === it.key ? "active" : ""} onClick={() => { location.hash = `#/${it.key}`; }}>
                  <span className="ic">{it.icon}</span>{it.label}
                </a>
              ))}
            </React.Fragment>
          ))}
        </nav>
        <div className="sidebar-foot">
          {me.data ? <div>👤 {me.data.email}<br /><Pill tone="info">{me.data.perms?.includes("*:*") ? "管理员" : "用户"}</Pill></div> : <span className="muted">加载中…</span>}
          <div style={{ marginTop: 8 }}><a onClick={logout} style={{ cursor: "pointer" }}>退出登录</a></div>
        </div>
      </aside>
      <main className="main">
        <div className="topbar">
          <div className="crumb">{cur.icon} {cur.label}</div>
          <div className="right">
            <button className="btn sm" onClick={() => {
              const t = document.documentElement.getAttribute("data-theme");
              const next = t === "light" ? "dark" : "light";
              document.documentElement.setAttribute("data-theme", next);
              localStorage.setItem("nexus.theme", next);
            }}>🌗 主题</button>
          </div>
        </div>
        <div className="content">{page()}</div>
      </main>
    </div>
  );
}

export default function App() {
  React.useEffect(() => {
    const saved = localStorage.getItem("nexus.theme") || "dark";
    document.documentElement.setAttribute("data-theme", saved);
  }, []);
  if (!getToken()) return <Login />;
  return <Shell />;
}
