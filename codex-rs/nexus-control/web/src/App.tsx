import { useEffect, useState } from "react";
import { api, getToken, openThreadStream, setToken, type Item, type Thread } from "./api";

export default function App() {
  const token = getToken();
  if (!token) return <Login />;
  return <Dashboard />;
}

function Login() {
  const [email, setEmail] = useState("admin@nexus.local");
  const [pw, setPw] = useState("admin");
  const [err, setErr] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  async function submit(e: React.FormEvent) {
    e.preventDefault();
    setBusy(true);
    setErr(null);
    try {
      const r = await api.login(email, pw);
      setToken(r.token);
      location.reload();
    } catch (e: unknown) {
      setErr(String(e));
    } finally {
      setBusy(false);
    }
  }

  return (
    <div style={s.page}>
      <h1>Nexus M1</h1>
      <form onSubmit={submit} style={s.card}>
        <h2>登录</h2>
        <input value={email} onChange={(e) => setEmail(e.target.value)} placeholder="email" style={s.input} />
        <input type="password" value={pw} onChange={(e) => setPw(e.target.value)} placeholder="password" style={s.input} />
        <button disabled={busy} style={s.btn}>{busy ? "…" : "登录"}</button>
        {err && <div style={s.err}>{err}</div>}
      </form>
    </div>
  );
}

function Dashboard() {
  const [threads, setThreads] = useState<Thread[]>([]);
  const [active, setActive] = useState<Thread | null>(null);

  useEffect(() => {
    api.listThreads().then(setThreads).catch(() => {});
  }, []);

  async function create() {
    const title = prompt("thread title");
    if (!title) return;
    const { id } = await api.createThread(title);
    const list = await api.listThreads();
    setThreads(list);
    const t = list.find((x) => x.id === id) ?? null;
    setActive(t);
  }

  return (
    <div style={s.dash}>
      <aside style={s.sidebar}>
        <h2>会话 ({threads.length})</h2>
        <button onClick={create} style={s.btn}>+ 新建</button>
        {threads.map((t) => (
          <div
            key={t.id}
            onClick={() => setActive(t)}
            style={active?.id === t.id ? s.activeItem : s.item}
          >
            <b>{t.title ?? t.id.slice(0, 8)}</b>
            <div style={s.muted}>{t.status}</div>
          </div>
        ))}
      </aside>
      <main style={s.main}>{active ? <Timeline thread={active} /> : <Empty />}</main>
    </div>
  );
}

function Timeline({ thread }: { thread: Thread }) {
  const [items, setItems] = useState<Item[]>([]);
  const [text, setText] = useState("hello");
  const [ws, setWs] = useState<WebSocket | null>(null);
  const [status, setStatus] = useState("connecting");

  // Initial load + WS live stream.
  useEffect(() => {
    let lastSeq = 0;
    api.listItems(thread.id).then((rows) => {
      setItems(rows);
      lastSeq = rows.reduce((m, r) => Math.max(m, r.seq), 0);
      const sock = openThreadStream(
        thread.id,
        (frame) => {
          setItems((prev) => {
            if (prev.some((p) => p.seq === frame.seq)) return prev;
            return [...prev, {
              id: 0, turn_id: 0, seq: frame.seq, item_type: frame.type,
              content_ref: frame.content, created_at: new Date().toISOString(),
            }].sort((a, b) => a.seq - b.seq);
          });
          lastSeq = Math.max(lastSeq, frame.seq);
        },
        () => setStatus("revoked")
      );
      setWs(sock);
      sock.onopen = () => setStatus("live");
      sock.onerror = () => setStatus("error");
      sock.onclose = () => setStatus("closed");
    });
    return () => ws?.close();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [thread.id]);

  async function sendTurn(e: React.FormEvent) {
    e.preventDefault();
    await api.startTurn(thread.id, text);
    setText("");
  }

  return (
    <div style={s.timeline}>
      <h2>{thread.title ?? thread.id.slice(0, 8)} <span style={s.muted}>· WS: {status}</span></h2>
      <div style={s.feed}>
        {items.map((it) => (
          <div key={it.seq} style={s.event}>
            <span style={s.seq}>#{it.seq}</span>
            <b style={s.kind}>{it.item_type}</b>
            <span style={s.content}>{it.content_ref ?? ""}</span>
          </div>
        ))}
        {items.length === 0 && <div style={s.muted}>暂无事件</div>}
      </div>
      <form onSubmit={sendTurn} style={s.composer}>
        <input value={text} onChange={(e) => setText(e.target.value)} style={s.input} />
        <button style={s.btn}>提交 turn</button>
      </form>
    </div>
  );
}

function Empty() {
  return <div style={s.empty}>选择左侧会话或新建一个会话</div>;
}

const s: Record<string, React.CSSProperties> = {
  page: { fontFamily: "system-ui", maxWidth: 420, margin: "80px auto", padding: 16 },
  card: { display: "flex", flexDirection: "column", gap: 8, padding: 24, border: "1px solid #ddd", borderRadius: 8 },
  dash: { display: "flex", height: "100vh", fontFamily: "system-ui" },
  sidebar: { width: 280, borderRight: "1px solid #eee", padding: 16, overflowY: "auto" },
  main: { flex: 1, display: "flex", flexDirection: "column" },
  item: { padding: "8px 10px", borderRadius: 6, cursor: "pointer" },
  activeItem: { padding: "8px 10px", borderRadius: 6, cursor: "pointer", background: "#eef" },
  timeline: { flex: 1, display: "flex", flexDirection: "column", padding: 16 },
  feed: { flex: 1, overflowY: "auto", border: "1px solid #eee", borderRadius: 6, padding: 8 },
  event: { display: "flex", gap: 8, padding: "4px 0", borderBottom: "1px solid #f4f4f4" },
  seq: { color: "#888", minWidth: 40 },
  kind: { minWidth: 70, color: "#06c" },
  content: { flex: 1 },
  composer: { display: "flex", gap: 8, marginTop: 8 },
  input: { flex: 1, padding: "8px 10px", border: "1px solid #ccc", borderRadius: 6 },
  btn: { padding: "8px 14px", border: "none", borderRadius: 6, background: "#336", color: "#fff", cursor: "pointer" },
  err: { color: "#c33" },
  muted: { color: "#999", fontSize: 12 },
  empty: { margin: "auto", color: "#aaa" },
};
