import React from "react";
import { api, openThreadStream, type Thread, type Item, type Snapshot } from "../api";
import { Card, Table, Button, Pill, Empty, ErrBar, Modal, Field, useAsync, fmtTime } from "../ui";

export default function Threads() {
  const list = useAsync<Thread[]>(() => api.listThreads(), []);
  const [active, setActive] = React.useState<Thread | null>(null);
  if (active) return <Timeline thread={active} onBack={() => { setActive(null); list.reload(); }} />;
  return (
    <Card title={`会话列表 (${list.data?.length || 0})`} action={<Button variant="primary" onClick={() => {
      const t = prompt("会话标题（可留空）", "");
      if (t === null) return;
      api.createThread(t || undefined).then(({ id }) => { location.hash = "#/threads"; setActive({ id, title: t || null, status: "active", created_at: new Date().toISOString() }); });
    }}>+ 新建会话</Button>}>
      <ErrBar err={list.err} />
      {list.loading && <Empty>加载中…</Empty>}
      {list.data && list.data.length === 0 && <Empty>暂无会话</Empty>}
      {list.data && list.data.length > 0 && (
        <Table cols={[
          { key: "id", label: "ID", className: "mono", render: (r: Thread) => r.id.slice(0, 8) + "…" },
          { key: "title", label: "标题", render: (r: Thread) => r.title || <span className="muted">未命名</span> },
          { key: "status", label: "状态", render: (r: Thread) => <Pill tone="info">{r.status}</Pill> },
          { key: "created_at", label: "创建时间", render: (r: Thread) => fmtTime(r.created_at) },
          { key: "act", label: "", render: (r: Thread) => <Button className="sm" onClick={() => setActive(r)}>进入 →</Button> },
        ]} rows={list.data} />
      )}
    </Card>
  );
}

function Timeline({ thread, onBack }: { thread: Thread; onBack: () => void }) {
  const [items, setItems] = React.useState<Item[]>([]);
  const [input, setInput] = React.useState("");
  const [busy, setBusy] = React.useState(false);
  const [err, setErr] = React.useState<string | null>(null);
  const [tab, setTab] = React.useState<"events" | "snapshots">("events");
  const [maxSeq, setMaxSeq] = React.useState(0);
  const snaps = useAsync<Snapshot[]>(() => api.snapshots(thread.id), [thread.id]);

  const load = React.useCallback(async () => {
    try {
      const its = await api.listItems(thread.id, 0);
      setItems(its); setMaxSeq(its.reduce((m, i) => Math.max(m, i.seq), 0));
    } catch (e: any) { setErr(String(e?.message || e)); }
  }, [thread.id]);
  React.useEffect(() => {
    load();
    const ws = openThreadStream(thread.id, (f) => {
      if (f.event === "item") {
        setItems((prev) => {
          if (prev.some((p) => p.seq === f.seq)) return prev;
          return [...prev, { id: f.seq, turn_id: 0, seq: f.seq, item_type: f.type, content_ref: f.content, created_at: new Date().toISOString() }];
        });
        setMaxSeq((m) => Math.max(m, f.seq));
      }
    });
    return () => ws.close();
  }, [thread.id, load]);

  async function submit() {
    if (!input.trim()) return;
    setBusy(true); setErr(null);
    try {
      await api.startTurn(thread.id, input);
      setInput("");
      setTimeout(load, 1000);
    } catch (e: any) { setErr(String(e?.message || e)); }
    finally { setBusy(false); }
  }

  return (
    <div>
      <div className="between" style={{ marginBottom: 14 }}>
        <div className="row">
          <Button onClick={onBack}>← 返回</Button>
          <span className="crumb" style={{ fontSize: 15, fontWeight: 600 }}>{thread.title || thread.id.slice(0, 8)}</span>
          <Pill tone="info">{thread.status}</Pill>
        </div>
        <div className="row">
          <Button className={tab === "events" ? "sm primary" : "sm"} onClick={() => setTab("events")}>事件流</Button>
          <Button className={tab === "snapshots" ? "sm primary" : "sm"} onClick={() => setTab("snapshots")}>快照/Fork</Button>
        </div>
      </div>

      {tab === "events" ? (
        <>
          <Card title="提交 Turn">
            <div className="row">
              <input className="input" style={{ flex: 1 }} value={input} onChange={(e) => setInput(e.target.value)}
                placeholder="输入任务（回车提交）…" onKeyDown={(e) => { if (e.key === "Enter") submit(); }} />
              <Button variant="primary" disabled={busy} onClick={submit}>{busy ? "执行中…" : "提交"}</Button>
            </div>
            <ErrBar err={err} />
          </Card>
          <Card title={`事件流 (${items.length})`}>
            {items.length === 0 && <Empty>暂无事件</Empty>}
            {items.map((it) => (
              <div key={it.id || it.seq} className="tl-event">
                <div className="tl-ts mono">#{it.seq} · {it.item_type}</div>
                <div style={{ flex: 1, wordBreak: "break-all" }}>{it.content_ref || <span className="muted">（无内容）</span>}</div>
              </div>
            ))}
          </Card>
        </>
      ) : (
        <Card title={`快照 (${snaps.data?.length || 0})`} action={<Button variant="primary" className="sm" onClick={async () => { await api.createSnapshot(thread.id); snaps.reload(); }}>+ 创建快照</Button>}>
          <ErrBar err={snaps.err} />
          {snaps.data && snaps.data.length === 0 && <Empty>暂无快照</Empty>}
          {snaps.data && snaps.data.length > 0 && (
            <Table cols={[
              { key: "id", label: "ID", render: (s: Snapshot) => s.id },
              { key: "turn", label: "Turn", render: (s: Snapshot) => s.turn_id },
              { key: "digest", label: "Digest", className: "mono", render: (s: Snapshot) => (s.content_digest || "—").slice(0, 12) },
              { key: "forked", label: "已Fork", render: (s: Snapshot) => s.forked_to_thread_id ? <Pill tone="ok">是</Pill> : <span className="muted">否</span> },
              { key: "created", label: "创建", render: (s: Snapshot) => fmtTime(s.created_at) },
              { key: "act", label: "", render: (s: Snapshot) => (
                <div className="row">
                  <Button className="sm" onClick={async () => { const r = await api.forkSnapshot(thread.id, s.id); alert("已分叉为新会话: " + r.new_thread_id.slice(0, 8)); }}>Fork</Button>
                  <Button className="sm danger" onClick={async () => { if (!confirm("回滚将删除此快照后的所有 turn/item，确认？")) return; const r = await api.rollbackSnapshot(thread.id, s.id); alert(`已删除 ${r.deleted_items} items / ${r.deleted_turns} turns`); load(); }}>Rollback</Button>
                </div>
              )},
            ]} rows={snaps.data} />
          )}
        </Card>
      )}
    </div>
  );
}
