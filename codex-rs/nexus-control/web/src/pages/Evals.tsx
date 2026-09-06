import React from "react";
import { api, type EvalCase, type EvalRun } from "../api";
import { Card, Table, Button, Pill, Empty, ErrBar, Modal, Field, useAsync, fmtTime } from "../ui";

export default function Evals() {
  const cases = useAsync<EvalCase[]>(() => api.evalCases(), []);
  const runs = useAsync<EvalRun[]>(() => api.evalRuns(30), []);
  const [create, setCreate] = React.useState(false);
  const [run, setRun] = React.useState<EvalCase | null>(null);
  return (
    <div>
      <Card title={`评测用例 (${cases.data?.length || 0})`} action={<Button variant="primary" className="sm" onClick={() => setCreate(true)}>+ 新建用例</Button>}>
        <ErrBar err={cases.err} />
        {cases.data && cases.data.length === 0 && <Empty>暂无用例</Empty>}
        {cases.data && cases.data.length > 0 && (
          <Table cols={[
            { key: "id", label: "ID", render: (c: EvalCase) => c.id },
            { key: "name", label: "名称", render: (c: EvalCase) => <b>{c.name}</b> },
            { key: "cat", label: "分类", render: (c: EvalCase) => c.category || "—" },
            { key: "exp", label: "期望状态", render: (c: EvalCase) => <Pill tone="info">{c.expected_status}</Pill> },
            { key: "contains", label: "期望包含", className: "mono", render: (c: EvalCase) => c.expected_contains || "—" },
            { key: "act", label: "", render: (c: EvalCase) => <Button className="sm" onClick={() => setRun(c)}>运行 →</Button> },
          ]} rows={cases.data} />
        )}
      </Card>
      <Card title="最近运行记录">
        <ErrBar err={runs.err} />
        {runs.data && runs.data.length === 0 && <Empty>暂无运行</Empty>}
        {runs.data && runs.data.length > 0 && (
          <Table cols={[
            { key: "id", label: "ID", render: (r: EvalRun) => r.id },
            { key: "case", label: "用例", render: (r: EvalRun) => r.case_id },
            { key: "turn", label: "Turn", render: (r: EvalRun) => r.turn_id },
            { key: "passed", label: "结果", render: (r: EvalRun) => r.passed ? <Pill tone="ok">PASS</Pill> : <Pill tone="danger">FAIL</Pill> },
            { key: "time", label: "时间", render: (r: EvalRun) => fmtTime(r.created_at) },
          ]} rows={runs.data} />
        )}
      </Card>
      {create && <CreateCase onClose={() => setCreate(false)} onDone={() => { cases.reload(); }} />}
      {run && <RunModal c={run} onClose={() => setRun(null)} onDone={() => { runs.reload(); }} />}
    </div>
  );
}

function CreateCase({ onClose, onDone }: { onClose: () => void; onDone: () => void }) {
  const [name, setName] = React.useState("");
  const [cat, setCat] = React.useState("");
  const [input, setInput] = React.useState("");
  const [exp, setExp] = React.useState("completed");
  const [contains, setContains] = React.useState("");
  const [err, setErr] = React.useState<string | null>(null);
  return (
    <Modal title="新建评测用例" onClose={onClose}>
      <Field label="名称"><input className="input" value={name} onChange={(e) => setName(e.target.value)} /></Field>
      <Field label="分类"><input className="input" value={cat} onChange={(e) => setCat(e.target.value)} /></Field>
      <Field label="输入"><textarea className="input" value={input} onChange={(e) => setInput(e.target.value)} /></Field>
      <Field label="期望状态"><input className="input" value={exp} onChange={(e) => setExp(e.target.value)} /></Field>
      <Field label="期望包含 (可选)"><input className="input" value={contains} onChange={(e) => setContains(e.target.value)} /></Field>
      <ErrBar err={err} />
      <Button variant="primary" onClick={async () => { try { await api.evalCreateCase({ name, category: cat || undefined, input, expected_status: exp, expected_contains: contains || undefined }); onClose(); onDone(); } catch (e: any) { setErr(String(e?.message || e)); } }}>创建</Button>
    </Modal>
  );
}

function RunModal({ c, onClose, onDone }: { c: EvalCase; onClose: () => void; onDone: () => void }) {
  const [turnId, setTurnId] = React.useState("");
  const [err, setErr] = React.useState<string | null>(null);
  const [result, setResult] = React.useState<EvalRun | null>(null);
  return (
    <Modal title={`运行用例 "${c.name}"`} onClose={onClose}>
      <div className="hint">期望状态: {c.expected_status} · 期望包含: {c.expected_contains || "—"}</div>
      <Field label="Turn ID"><input className="input" type="number" value={turnId} onChange={(e) => setTurnId(e.target.value)} placeholder="已完成 turn 的 ID" /></Field>
      <ErrBar err={err} />
      {result && <div style={{ padding: 10, background: "var(--panel2)", borderRadius: 8, marginBottom: 10 }}>{result.passed ? <Pill tone="ok">PASS</Pill> : <Pill tone="danger">FAIL</Pill>} <span className="muted" style={{ fontSize: 12 }}>{JSON.stringify(result.detail)}</span></div>}
      <Button variant="primary" onClick={async () => { setErr(null); try { const r = await api.evalRun(c.id, Number(turnId)); setResult(r); onDone(); } catch (e: any) { setErr(String(e?.message || e)); } }}>运行断言</Button>
    </Modal>
  );
}
