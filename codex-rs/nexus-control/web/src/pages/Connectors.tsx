import React from "react";
import { api, type Connector, type ToolCall } from "../api";
import { Card, Table, Button, Pill, Empty, ErrBar, Modal, Field, useAsync, fmtTime } from "../ui";

export default function Connectors() {
  const list = useAsync<Connector[]>(() => api.connectors(), []);
  const [sel, setSel] = React.useState<number | null>(null);
  const [create, setCreate] = React.useState(false);
  return (
    <div>
      <Card title={`连接器市场 (${list.data?.length || 0})`} action={<Button variant="primary" className="sm" onClick={() => setCreate(true)}>+ 新建</Button>}>
        <ErrBar err={list.err} />
        {list.data && list.data.length === 0 && <Empty>暂无连接器</Empty>}
        {list.data && list.data.length > 0 && (
          <Table cols={[
            { key: "id", label: "ID", render: (c: Connector) => c.id },
            { key: "name", label: "名称", render: (c: Connector) => <b>{c.name}</b> },
            { key: "kind", label: "类型", render: (c: Connector) => <Pill tone="info">{c.kind}</Pill> },
            { key: "tier", label: "分级", render: (c: Connector) => <Pill tone={c.tier === "official" ? "ok" : c.tier === "enterprise" ? "warn" : "mut"}>{c.tier}</Pill> },
            { key: "status", label: "状态", render: (c: Connector) => <Pill tone={c.status === "published" ? "ok" : c.status === "offline" ? "danger" : "warn"}>{c.status}</Pill> },
            { key: "quality", label: "质量分", render: (c: Connector) => (c.quality_score || 0).toFixed(3) },
            { key: "act", label: "", render: (c: Connector) => <Button className="sm" onClick={() => setSel(c.id)}>详情 →</Button> },
          ]} rows={list.data} />
        )}
      </Card>
      {sel != null && <ConnDetail id={sel} onClose={() => setSel(null)} onChanged={list.reload} />}
      {create && <CreateConn onClose={() => setCreate(false)} onDone={list.reload} />}
    </div>
  );
}

function ConnDetail({ id, onClose, onChanged }: { id: number; onClose: () => void; onChanged: () => void }) {
  const conn = useAsync<Connector>(() => api.connector(id), [id]);
  const calls = useAsync<ToolCall[]>(() => api.connectorCalls(id), [id]);
  const [invoke, setInvoke] = React.useState(false);
  const [lastResult, setLastResult] = React.useState<string | null>(null);
  const c = conn.data;
  return (
    <Card title={`连接器 #${id}`} action={<div className="row"><Button className="sm" onClick={onClose}>← 返回</Button></div>}>
      <ErrBar err={conn.err} />
      {c && (
        <>
          <div className="row" style={{ marginBottom: 12, flexWrap: "wrap" }}>
              <Pill tone="info">{c.kind}</Pill>
              <Pill tone={c.tier === "official" ? "ok" : c.tier === "enterprise" ? "warn" : "mut"}>{c.tier}</Pill>
              <Pill tone={c.status === "published" ? "ok" : c.status === "offline" ? "danger" : "warn"}>{c.status}</Pill>
              {c.status === "draft" && <Button variant="primary" className="sm" onClick={async () => { await api.publishConnector(id); onChanged(); conn.reload(); }}>发布</Button>}
              {c.status === "published" && <Button variant="danger" className="sm" onClick={async () => { await api.offlineConnector(id); onChanged(); conn.reload(); }}>下线</Button>}
              <Button variant="danger" className="sm" onClick={async () => { if (confirm("删除此连接器？")) { await api.deleteConnector(id); onChanged(); onClose(); } }}>删除</Button>
          </div>
          {c.description && <div className="muted" style={{ marginBottom: 10 }}>{c.description}</div>}
          <div className="muted mono" style={{ fontSize: 11, marginBottom: 10 }}>config: {JSON.stringify(c.config_json)}</div>
          <div className="row" style={{ marginBottom: 12 }}>
            <Button variant="primary" onClick={() => setInvoke(true)}>调用 MCP 工具</Button>
            <Button className="sm" onClick={calls.reload}>刷新调用历史</Button>
          </div>
          {lastResult && <div style={{ padding: 10, background: "var(--panel2)", borderRadius: 8, marginBottom: 12, fontFamily: "monospace", fontSize: 12 }}>{lastResult}</div>}
          <h4 style={{ margin: "10px 0 6px", color: "var(--mut)", fontSize: 12 }}>调用历史</h4>
          {calls.data && calls.data.length === 0 && <Empty>暂无调用</Empty>}
          {calls.data && calls.data.length > 0 && (
            <Table cols={[
              { key: "id", label: "ID", render: (t: ToolCall) => t.id },
              { key: "tool", label: "工具", className: "mono", render: (t: ToolCall) => t.tool_name },
              { key: "success", label: "结果", render: (t: ToolCall) => t.success ? <Pill tone="ok">成功</Pill> : <Pill tone="danger">失败</Pill> },
              { key: "time", label: "时间", render: (t: ToolCall) => fmtTime(t.created_at) },
            ]} rows={calls.data} />
          )}
          {invoke && <InvokeModal id={id} onClose={() => setInvoke(false)} onResult={(r) => { setLastResult(r); calls.reload(); }} />}
        </>
      )}
    </Card>
  );
}

function InvokeModal({ id, onClose, onResult }: { id: number; onClose: () => void; onResult: (r: string) => void }) {
  const [tool, setTool] = React.useState("echo");
  const [args, setArgs] = React.useState('{"message":"hello-nexus"}');
  const [err, setErr] = React.useState<string | null>(null);
  const [busy, setBusy] = React.useState(false);
  return (
    <Modal title="调用 MCP 工具" onClose={onClose}>
      <Field label="工具名"><input className="input" value={tool} onChange={(e) => setTool(e.target.value)} /></Field>
      <Field label="参数 (JSON)" hint="MCP 真实转发：spawn server → initialize → tools/call"><textarea className="input" value={args} onChange={(e) => setArgs(e.target.value)} /></Field>
      <ErrBar err={err} />
      <Button variant="primary" disabled={busy} onClick={async () => {
        setBusy(true); setErr(null);
        try { const r = await api.invokeConnector(id, { tool, args: JSON.parse(args) }); onResult(`mcp=${r.mcp} success=${r.success}\nresult=${r.result}`); onClose(); }
        catch (e: any) { setErr(String(e?.message || e)); }
        finally { setBusy(false); }
      }}>{busy ? "调用中…" : "调用"}</Button>
    </Modal>
  );
}

function CreateConn({ onClose, onDone }: { onClose: () => void; onDone: () => void }) {
  const [name, setName] = React.useState("");
  const [kind, setKind] = React.useState("mcp");
  const [tier, setTier] = React.useState("community");
  const [desc, setDesc] = React.useState("");
  const [cmd, setCmd] = React.useState("python3");
  const [args, setArgs] = React.useState("/app/mcp_echo_server.py");
  const [err, setErr] = React.useState<string | null>(null);
  return (
    <Modal title="新建连接器" onClose={onClose}>
      <Field label="名称"><input className="input" value={name} onChange={(e) => setName(e.target.value)} /></Field>
      <div className="row">
        <Field label="类型"><select className="input" value={kind} onChange={(e) => setKind(e.target.value)}><option value="mcp">mcp</option><option value="tool">tool</option></select></Field>
        <Field label="分级"><select className="input" value={tier} onChange={(e) => setTier(e.target.value)}><option value="community">community</option><option value="official">official</option><option value="enterprise">enterprise</option></select></Field>
      </div>
      <Field label="描述"><input className="input" value={desc} onChange={(e) => setDesc(e.target.value)} /></Field>
      <Field label="MCP command" hint="config_json.command"><input className="input" value={cmd} onChange={(e) => setCmd(e.target.value)} /></Field>
      <Field label="MCP args (空格分隔)" hint="config_json.args"><input className="input" value={args} onChange={(e) => setArgs(e.target.value)} /></Field>
      <ErrBar err={err} />
      <Button variant="primary" onClick={async () => {
        try { await api.createConnector({ name, kind, description: desc || undefined, tier, config_json: { command: cmd, args: args.split(/\s+/).filter(Boolean) } }); onClose(); onDone(); }
        catch (e: any) { setErr(String(e?.message || e)); }
      }}>创建</Button>
    </Modal>
  );
}
