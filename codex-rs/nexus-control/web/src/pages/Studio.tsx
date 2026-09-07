import React from "react";
import { api, openThreadStream, type Thread, type AgentDef, type StreamFrame, type Item } from "../api";
import { Card, Table, Button, Pill, Empty, ErrBar, Modal, Field, useAsync, Markdown, ReasoningBlock, ToolCard, KV, CodeBlock, fmtTime } from "../ui";

/// 一个流式 item 的聚合状态：delta 增量按 type 累积，item/started+completed 给完整 ThreadItem。
interface StudioItem {
  itemId: string;
  kind: string | null;          // ThreadItem.type（agentMessage/reasoning/commandExecution/...）
  raw: any | null;              // ThreadItem JSON（来自 item/started 或 item/completed）
  deltas: Record<string, string>; // type→累积文本（item/reasoning/textDelta 等）
  completed: boolean;
  seq: number;
}

export default function Studio() {
  const list = useAsync<Thread[]>(() => api.listThreads(), []);
  const [active, setActive] = React.useState<Thread | null>(null);
  const [agentModal, setAgentModal] = React.useState(false);
  if (active) return <StudioChat thread={active} onBack={() => { setActive(null); list.reload(); }} />;
  return (
    <Card title={`会话列表 (${list.data?.length || 0})`} action={
      <div className="row">
        <Button className="sm" onClick={() => setAgentModal(true)}>+ Agent 定义</Button>
        <Button variant="primary" onClick={() => {
          const t = prompt("会话标题（可留空）", "");
          if (t === null) return;
          api.createThread(t || undefined).then(({ id }) => {
            setActive({ id, title: t || null, status: "active", created_at: new Date().toISOString() });
          });
        }}>+ 新建会话</Button>
      </div>
    }>
      <ErrBar err={list.err} />
      {list.loading && <Empty>加载中…</Empty>}
      {list.data && list.data.length === 0 && <Empty>暂无会话，点击「新建会话」开始对话</Empty>}
      {list.data && list.data.length > 0 && (
        <Table cols={[
          { key: "id", label: "ID", className: "mono", render: (r: Thread) => r.id.slice(0, 8) + "…" },
          { key: "title", label: "标题", render: (r: Thread) => r.title || <span className="muted">未命名</span> },
          { key: "agent", label: "Agent", render: (r: Thread) => r.agent_def_id ? <Pill tone="info">#{r.agent_def_id}</Pill> : <span className="muted">默认</span> },
          { key: "status", label: "状态", render: (r: Thread) => <Pill tone="info">{r.status}</Pill> },
          { key: "created_at", label: "创建", render: (r: Thread) => fmtTime(r.created_at) },
          { key: "act", label: "", render: (r: Thread) => <Button className="sm" onClick={() => setActive(r)}>进入对话 →</Button> },
        ]} rows={list.data} />
      )}
      {agentModal && <AgentManager onClose={() => setAgentModal(false)} />}
    </Card>
  );
}

/// Agent 定义管理 Modal：CRUD。
function AgentManager({ onClose }: { onClose: () => void }) {
  const list = useAsync<AgentDef[]>(() => api.listAgents(), []);
  const [editing, setEditing] = React.useState<{ name: string; system_prompt: string; description: string; model: string } | null>(null);
  const [editId, setEditId] = React.useState<number | null>(null);
  function startNew() { setEditing({ name: "", system_prompt: "", description: "", model: "" }); setEditId(null); }
  function startEdit(a: AgentDef) { setEditing({ name: a.name, system_prompt: a.system_prompt || "", description: a.description || "", model: a.model || "" }); setEditId(a.id); }
  async function save() {
    if (!editing) return;
    if (!editing.name.trim()) { alert("名称必填"); return; }
    if (editId) await api.updateAgent(editId, editing);
    else await api.createAgent(editing);
    setEditing(null); list.reload();
  }
  return (
    <Modal title="Agent 定义管理" onClose={onClose}>
      {editing ? (
        <div>
          <Field label="名称"><input className="input" value={editing.name} onChange={(e) => setEditing({ ...editing, name: e.target.value })} /></Field>
          <Field label="描述"><input className="input" value={editing.description} onChange={(e) => setEditing({ ...editing, description: e.target.value })} /></Field>
          <Field label="模型（可选，留空用默认）"><input className="input" value={editing.model} onChange={(e) => setEditing({ ...editing, model: e.target.value })} placeholder="deepseek-v4-pro" /></Field>
          <Field label="System Prompt" hint="作为 Agent 的系统提示词，注入 codex base_instructions">
            <textarea className="input" rows={6} style={{ width: "100%", fontFamily: "monospace", resize: "vertical" }} value={editing.system_prompt}
              onChange={(e) => setEditing({ ...editing, system_prompt: e.target.value })}
              placeholder="你是一个资深的软件工程师，擅长…" />
          </Field>
          <div className="row" style={{ marginTop: 10 }}>
            <Button variant="primary" onClick={save}>保存</Button>
            <Button onClick={() => setEditing(null)}>取消</Button>
          </div>
        </div>
      ) : (
        <div>
          <Button variant="primary" className="sm" onClick={startNew} style={{ marginBottom: 10 }}>+ 新建 Agent</Button>
          <ErrBar err={list.err} />
          {list.data && list.data.length === 0 && <Empty>暂无 Agent 定义</Empty>}
          {list.data && list.data.map((a) => (
            <div key={a.id} className="row" style={{ padding: "8px 0", borderBottom: "1px solid var(--bd)", alignItems: "center", gap: 8 }}>
              <div style={{ flex: 1 }}>
                <div style={{ fontWeight: 600 }}>{a.name} <Pill tone="mut">#{a.id}</Pill></div>
                <div className="muted" style={{ fontSize: 12 }}>{a.description || "—"} {a.model && `· ${a.model}`}</div>
              </div>
              <Button className="sm" onClick={() => startEdit(a)}>编辑</Button>
              <Button className="sm danger" onClick={async () => { if (confirm(`删除 Agent「${a.name}」？`)) { await api.deleteAgent(a.id); list.reload(); } }}>删除</Button>
            </div>
          ))}
        </div>
      )}
    </Modal>
  );
}

/// CUI 对话框主体。
function StudioChat({ thread, onBack }: { thread: Thread; onBack: () => void }) {
  const [items, setItems] = React.useState<Map<string, StudioItem>>(new Map());
  const [order, setOrder] = React.useState<string[]>([]);
  const [input, setInput] = React.useState("");
  const [busy, setBusy] = React.useState(false);
  const [err, setErr] = React.useState<string | null>(null);
  const [userBubbles, setUserBubbles] = React.useState<string[]>([]);
  const [pendingApproval, setPendingApproval] = React.useState<any | null>(null);
  const scrollRef = React.useRef<HTMLDivElement>(null);

  // Upsert helper: adds/updates an item in the map + appends to order.
  const upsert = React.useCallback((itemId: string, patch: (it: StudioItem) => StudioItem, seq: number) => {
    setItems((prev) => {
      const next = new Map(prev);
      const existing = next.get(itemId) || { itemId, kind: null, raw: null, deltas: {}, completed: false, seq };
      next.set(itemId, patch(existing));
      return next;
    });
    setOrder((prev) => prev.includes(itemId) ? prev : [...prev, itemId]);
  }, []);

  // Load history from DB (item/started + item/completed only — deltas are transient).
  const loadHistory = React.useCallback(async () => {
    try {
      const its = await api.listItems(thread.id, 0);
      setItems(new Map());
      setOrder([]);
      for (const it of its) {
        if (!it.content_ref) continue;
        try {
          const raw = JSON.parse(it.content_ref);
          const itemId = raw.id || `seq-${it.seq}`;
          const kind = raw.type || null;
          upsert(itemId, (x) => ({ ...x, kind, raw, completed: it.item_type === "item/completed" }), it.seq);
        } catch { /* skip unparseable */ }
      }
    } catch (e: any) { setErr(String(e?.message || e)); }
  }, [thread.id, upsert]);

  React.useEffect(() => {
    loadHistory();
    const ws = openThreadStream(thread.id, (f: StreamFrame) => {
      const t = f.type || "";
      const isDelta = t.endsWith("/delta") || t.endsWith("Delta");
      const itemId = f.item_id || `seq-${f.seq}`;

      if (t === "item/started" || t === "item/completed") {
        if (!f.content) return;
        try {
          const raw = JSON.parse(f.content);
          const id = raw.id || itemId;
          upsert(id, (x) => ({
            ...x,
            kind: raw.type || x.kind,
            raw,
            completed: t === "item/completed" ? true : x.completed,
            seq: f.seq,
          }), f.seq);
        } catch { /* ignore */ }
      } else if (isDelta && f.content != null) {
        // Accumulate delta text by type under this item_id.
        upsert(itemId, (x) => ({
          ...x,
          deltas: { ...x.deltas, [t]: (x.deltas[t] || "") + f.content },
          seq: f.seq,
        }), f.seq);
      } else if (t === "approval/requested") {
        setPendingApproval({ approval_id: f.approval_id, command: f.command, item_id: itemId });
      } else if (t === "turn/completed" || t === "approval/interrupted") {
        setBusy(false);
      }
    });
    return () => ws.close();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [thread.id]);

  // Auto-scroll.
  React.useEffect(() => {
    scrollRef.current?.scrollTo({ top: scrollRef.current.scrollHeight, behavior: "smooth" });
  }, [order, items, busy]);

  async function submit() {
    if (!input.trim() || busy) return;
    const text = input;
    setUserBubbles((prev) => [...prev, text]);
    setInput("");
    setBusy(true); setErr(null);
    // Fire-and-forget: the HTTP blocks until turn/completed, but we render
    // live via WS deltas. Catch errors on the promise separately.
    api.startTurn(thread.id, text).then(() => { /* turn finalized */ })
      .catch((e: any) => { setErr(String(e?.message || e)); setBusy(false); });
  }

  async function resolveApproval(decision: "approve" | "deny") {
    if (!pendingApproval) return;
    const aid = pendingApproval.approval_id;
    setPendingApproval(null);
    try { await api.resolveApproval(aid, decision); }
    catch (e: any) { setErr(String(e?.message || e)); }
  }

  return (
    <div className="studio-chat">
      <div className="studio-head between">
        <div className="row">
          <Button onClick={onBack}>← 返回</Button>
          <span className="crumb" style={{ fontWeight: 600 }}>{thread.title || thread.id.slice(0, 8)}</span>
          {busy ? <Pill tone="warn">生成中…</Pill> : <Pill tone="ok">就绪</Pill>}
        </div>
      </div>
      <div className="studio-stream" ref={scrollRef}>
        {userBubbles.length === 0 && order.length === 0 && <Empty>输入消息开始对话</Empty>}
        {/* Render user + assistant interleaved: user bubbles then assistant items (simplified: user first batch, then assistant stream). */}
        {order.map((id) => {
          const it = items.get(id);
          if (!it) return null;
          // Insert user bubbles before items that came after the corresponding turn —
          // simplified: render user bubbles once at the top of the assistant block.
          return <AssistantBlock key={id} item={it} />;
        })}
      </div>
      {pendingApproval && (
        <div className="approval-card">
          <div style={{ fontWeight: 600 }}>🔐 审批请求</div>
          <CodeBlock text={pendingApproval.command || ""} />
          <div className="row" style={{ marginTop: 8 }}>
            <Button variant="primary" onClick={() => resolveApproval("approve")}>批准</Button>
            <Button variant="danger" onClick={() => resolveApproval("deny")}>拒绝</Button>
          </div>
        </div>
      )}
      <div className="studio-input">
        <ErrBar err={err} />
        <div className="row">
          <textarea className="input" style={{ flex: 1, minHeight: 48, maxHeight: 200, resize: "vertical" }}
            value={input} onChange={(e) => setInput(e.target.value)}
            placeholder="输入消息（Shift+Enter 换行，Enter 发送）…"
            onKeyDown={(e) => { if (e.key === "Enter" && !e.shiftKey) { e.preventDefault(); submit(); } }} />
          <Button variant="primary" disabled={busy} onClick={submit}>{busy ? "生成中…" : "发送"}</Button>
        </div>
      </div>
    </div>
  );
}

/// 渲染一个 assistant item：按 kind 分发到思考/答案/工具卡片/产物。
function AssistantBlock({ item }: { item: StudioItem }) {
  // Delta-accumulated text (live streaming); fall back to raw structure.
  const reasoningText = item.deltas["item/reasoning/textDelta"] || item.deltas["item/reasoning/summaryTextDelta"]
    || (item.raw?.content ? (Array.isArray(item.raw.content) ? item.raw.content.join("\n") : String(item.raw.content)) : "")
    || (item.raw?.summary ? (Array.isArray(item.raw.summary) ? item.raw.summary.join("\n") : "") : "");
  const messageText = item.deltas["item/agentMessage/delta"] || item.raw?.text || "";
  const planText = item.deltas["item/plan/delta"] || item.raw?.text || "";
  const cmdOutput = item.deltas["item/commandExecution/outputDelta"] || item.raw?.aggregated_output || "";
  const kind = item.kind;

  // User input echoed back by codex as a userMessage item — render as a
  // right-aligned user bubble (NOT a 📦 JSON fallback). Extract text from
  // raw.content[].text (codex ThreadItem userMessage shape) or raw.text.
  if (kind === "userMessage") {
    const r = item.raw || {};
    const text = Array.isArray(r.content)
      ? r.content.map((c: any) => c?.text || "").join("\n")
      : (r.text || "");
    return <div className="user-bubble">{text}</div>;
  }

  if (kind === "reasoning") {
    return <ReasoningBlock text={reasoningText} />;
  }
  if (kind === "agentMessage") {
    return <div className="assistant-msg"><Markdown text={messageText} /></div>;
  }
  if (kind === "plan") {
    return <ReasoningBlock text={planText} />;  // plan 也折叠展示
  }
  if (kind === "commandExecution") {
    const r = item.raw || {};
    return (
      <ToolCard title="命令执行" badge={<Pill tone={r.exit_code === 0 ? "ok" : "danger"}>{r.status || (r.exit_code === 0 ? "成功" : "失败")}</Pill>}>
        <KV k="命令" v={<code className="md-ic">{r.command || ""}</code>} />
        {r.cwd && <KV k="目录" v={<code className="md-ic">{r.cwd}</code>} />}
        {(r.exit_code !== undefined && r.exit_code !== null) && <KV k="退出码" v={String(r.exit_code)} />}
        {cmdOutput && <CodeBlock text={cmdOutput} maxHeight={300} />}
      </ToolCard>
    );
  }
  if (kind === "mcpToolCall" || kind === "dynamicToolCall") {
    const r = item.raw || {};
    return (
      <ToolCard title={`MCP 工具调用`} badge={<Pill tone={r.success ? "ok" : "danger"}>{r.status || (r.success ? "成功" : "失败")}</Pill>}>
        <KV k="工具" v={<code className="md-ic">{r.tool || r.name || ""}</code>} />
        {r.arguments && <KV k="参数" v={<code className="md-ic">{typeof r.arguments === "string" ? r.arguments : JSON.stringify(r.arguments)}</code>} />}
        {r.result && <CodeBlock text={typeof r.result === "string" ? r.result : JSON.stringify(r.result, null, 2)} maxHeight={300} />}
        {r.error && <div className="err">{r.error}</div>}
      </ToolCard>
    );
  }
  if (kind === "functionCallOutput") {
    const r = item.raw || {};
    return (
      <ToolCard title="工具输出" badge={<Pill tone="info">{r.name || ""}</Pill>}>
        <CodeBlock text={typeof r.output === "string" ? r.output : JSON.stringify(r.output, null, 2)} maxHeight={300} />
      </ToolCard>
    );
  }
  if (kind === "fileChange") {
    const r = item.raw || {};
    const changes = Array.isArray(r.changes) ? r.changes : [];
    return (
      <ToolCard title="文件变更" badge={<Pill tone="info">{changes.length} 处</Pill>}>
        {changes.map((c: any, i: number) => (
          <div key={i} className="kv">
            <span className="kv-k">{c.change_type || c.op || "edit"}</span>
            <span className="kv-v mono">{c.path || c.name || ""}</span>
          </div>
        ))}
      </ToolCard>
    );
  }
  // Fallback: render raw JSON in a collapsible block.
  if (item.raw) {
    return (
      <details className="tool-card">
        <summary className="tool-card-h"><span className="tool-ic">📦</span><span className="tool-title">{kind || "item"}</span></summary>
        <div className="tool-card-b"><CodeBlock text={JSON.stringify(item.raw, null, 2)} maxHeight={300} /></div>
      </details>
    );
  }
  // Delta-only item (no item/started+completed yet): show accumulated deltas.
  const deltaKeys = Object.keys(item.deltas);
  if (deltaKeys.length > 0) {
    const text = deltaKeys.map((k) => item.deltas[k]).join("");
    if (deltaKeys.some((k) => k.includes("reasoning"))) return <ReasoningBlock text={text} />;
    if (deltaKeys.some((k) => k.includes("agentMessage"))) return <div className="assistant-msg"><Markdown text={text} /></div>;
    return <CodeBlock text={text} maxHeight={200} />;
  }
  return null;
}
