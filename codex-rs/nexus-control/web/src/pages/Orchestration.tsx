import React from "react";
import { api, type Orchestration, type AgentStep } from "../api";
import { Card, Table, Button, Pill, Empty, ErrBar, Modal, Field, useAsync, fmtTime } from "../ui";

const MODES = [
  { key: "orchestrator-worker", label: "编排者-工作者", desc: "编排者出计划 → N 个工作者接力 → 汇总" },
  { key: "peer", label: "对等协作", desc: "N 个 Agent 并行执行 → 扇入合并" },
  { key: "critic-adversarial", label: "批评对抗", desc: "生产者 → 批评者门控 → 修订循环" },
];

export default function Orchestration() {
  const list = useAsync<Orchestration[]>(() => api.orchestrations(), []);
  const [start, setStart] = React.useState(false);
  const [sel, setSel] = React.useState<number | null>(null);
  return (
    <div>
      <Card title={`协作编排 (${list.data?.length || 0})`} action={<Button variant="primary" className="sm" onClick={() => setStart(true)}>+ 启动编排</Button>}>
        <ErrBar err={list.err} />
        {list.data && list.data.length === 0 && <Empty>暂无编排记录</Empty>}
        {list.data && list.data.length > 0 && (
          <Table cols={[
            { key: "id", label: "ID", render: (o: Orchestration) => o.id },
            { key: "name", label: "名称", render: (o: Orchestration) => o.name || "—" },
            { key: "mode", label: "模式", render: (o: Orchestration) => <Pill tone="info">{o.mode}</Pill> },
            { key: "status", label: "状态", render: (o: Orchestration) => <Pill tone={o.status === "completed" ? "ok" : o.status === "failed" ? "danger" : "warn"}>{o.status}</Pill> },
            { key: "created", label: "创建", render: (o: Orchestration) => fmtTime(o.created_at) },
            { key: "act", label: "", render: (o: Orchestration) => <Button className="sm" onClick={() => setSel(o.id)}>详情 →</Button> },
          ]} rows={list.data} />
        )}
      </Card>
      {sel != null && <OrchDetail id={sel} onClose={() => setSel(null)} />}
      {start && <StartModal onClose={() => setStart(false)} onDone={() => { setStart(false); list.reload(); }} />}
    </div>
  );
}

function StartModal({ onClose, onDone }: { onClose: () => void; onDone: () => void }) {
  const [mode, setMode] = React.useState("orchestrator-worker");
  const [prompt, setPrompt] = React.useState("用一句话介绍 Nexus 企业级 AI Agent 平台");
  const [agents, setAgents] = React.useState(2);
  const [err, setErr] = React.useState<string | null>(null);
  const [busy, setBusy] = React.useState(false);
  return (
    <Modal title="启动协作编排" onClose={onClose}>
      <Field label="模式">
        <select className="input" value={mode} onChange={(e) => setMode(e.target.value)}>
          {MODES.map((m) => <option key={m.key} value={m.key}>{m.label}</option>)}
        </select>
        <div className="hint">{MODES.find((m) => m.key === mode)?.desc}</div>
      </Field>
      <Field label="编排 prompt"><textarea className="input" value={prompt} onChange={(e) => setPrompt(e.target.value)} /></Field>
      {(mode === "orchestrator-worker" || mode === "peer") && (
        <Field label="Agent 数量"><input className="input" type="number" min={1} max={5} value={agents} onChange={(e) => setAgents(Number(e.target.value))} /></Field>
      )}
      <ErrBar err={err} />
      <Button variant="primary" disabled={busy} onClick={async () => {
        setBusy(true); setErr(null);
        try { await api.startOrchestration({ mode, prompt, agents: (mode === "orchestrator-worker" || mode === "peer") ? agents : undefined }); onDone(); }
        catch (e: any) { setErr(String(e?.message || e)); }
        finally { setBusy(false); }
      }}>{busy ? "编排执行中…" : "启动"}</Button>
    </Modal>
  );
}

function OrchDetail({ id, onClose }: { id: number; onClose: () => void }) {
  const d = useAsync(() => api.orchestration(id), [id]);
  return (
    <Card title={`编排 #${id} 详情`} action={<Button className="sm" onClick={onClose}>← 返回</Button>}>
      <ErrBar err={d.err} />
      {d.data && (
        <>
          <div className="row" style={{ marginBottom: 12 }}>
            <Pill tone="info">{d.data.orchestration.mode}</Pill>
            <Pill tone={d.data.orchestration.status === "completed" ? "ok" : d.data.orchestration.status === "failed" ? "danger" : "warn"}>{d.data.orchestration.status}</Pill>
            <span className="muted">{fmtTime(d.data.orchestration.created_at)}</span>
          </div>
          {d.data.orchestration.prompt && <div style={{ padding: 10, background: "var(--panel2)", borderRadius: 8, marginBottom: 12, fontSize: 13 }}><b>编排结果：</b>{d.data.orchestration.prompt}</div>}
          <h4 style={{ color: "var(--mut)", fontSize: 12, margin: "10px 0 6px" }}>Agent 步骤 ({d.data.agents.length})</h4>
          <Table cols={[
            { key: "seq", label: "#", render: (a: AgentStep) => a.agent_seq },
            { key: "role", label: "角色", render: (a: AgentStep) => a.role || "—" },
            { key: "thread", label: "Thread", className: "mono", render: (a: AgentStep) => (a.thread_id || "").slice(0, 8) },
            { key: "turn", label: "Turn", render: (a: AgentStep) => a.turn_id || "—" },
            { key: "status", label: "状态", render: (a: AgentStep) => <Pill tone={a.status === "completed" ? "ok" : a.status === "failed" ? "danger" : "warn"}>{a.status}</Pill> },
            { key: "out", label: "输出", className: "wrap", render: (a: AgentStep) => (a.output_ref || "").slice(0, 60) || "—" },
          ]} rows={d.data.agents} />
        </>
      )}
    </Card>
  );
}
