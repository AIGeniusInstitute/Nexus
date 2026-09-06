import React from "react";
import { api, type Approval } from "../api";
import { Card, Table, Button, Pill, Empty, ErrBar, Modal, Field, useAsync, fmtTime } from "../ui";

export default function Approvals() {
  const list = useAsync<Approval[]>(() => api.listApprovals(), []);
  const [amend, setAmend] = React.useState<Approval | null>(null);
  const [cmd, setCmd] = React.useState("");

  async function resolve(id: number, decision: "approve" | "deny" | "cancel") {
    try { await api.resolveApproval(id, decision); list.reload(); }
    catch (e: any) { alert(String(e?.message || e)); }
  }
  async function doAmend() {
    if (!amend) return;
    const args = cmd.split(/\s+/).filter(Boolean);
    try { await api.resolveApproval(amend.id, "approve_with_amendment", args); setAmend(null); setCmd(""); list.reload(); }
    catch (e: any) { alert(String(e?.message || e)); }
  }

  return (
    <Card title={`审批工单 (${list.data?.length || 0})`} action={<Button className="sm" onClick={list.reload}>刷新</Button>}>
      <ErrBar err={list.err} />
      {list.loading && <Empty>加载中…</Empty>}
      {list.data && list.data.length === 0 && <Empty>暂无审批工单</Empty>}
      {list.data && list.data.length > 0 && (
        <Table cols={[
          { key: "id", label: "ID", render: (a: Approval) => a.id },
          { key: "kind", label: "类型", render: (a: Approval) => <Pill tone="info">{a.kind || "—"}</Pill> },
          { key: "status", label: "状态", render: (a: Approval) => {
            const t = a.status === "pending" ? "warn" : a.status === "approved" || a.status === "approved_with_amendment" ? "ok" : "danger";
            return <Pill tone={t as any}>{a.status}</Pill>;
          }},
          { key: "command", label: "命令", className: "mono wrap", render: (a: Approval) => a.command || "—" },
          { key: "policy", label: "策略推荐", render: (a: Approval) => a.policy_decision ? <Pill tone="mut">{a.policy_decision}</Pill> : <span className="muted">—</span> },
          { key: "risk", label: "风险", render: (a: Approval) => {
            const r = a.risk_level || "—";
            const t = r === "high" ? "danger" : r === "medium" ? "warn" : "ok";
            return <Pill tone={t as any}>{r}</Pill>;
          }},
          { key: "created", label: "时间", render: (a: Approval) => fmtTime(a.created_at) },
          { key: "act", label: "操作", render: (a: Approval) => a.status === "pending" ? (
            <div className="row">
              <Button variant="primary" className="sm" onClick={() => resolve(a.id, "approve")}>批准</Button>
              <Button variant="danger" className="sm" onClick={() => resolve(a.id, "deny")}>拒绝</Button>
              <Button className="sm" onClick={() => resolve(a.id, "cancel")}>中断</Button>
              <Button className="sm" onClick={() => setAmend(a)}>改后批</Button>
            </div>
          ) : <span className="muted">—</span> },
        ]} rows={list.data} />
      )}
      {amend && (
        <Modal title={`改后批准 #${amend.id}`} onClose={() => setAmend(null)}>
          <Field label="amendment 命令（空格分隔，将自动 allow 此前缀）" hint="例: git clone → 此后该命令免审批">
            <input className="input" value={cmd} onChange={(e) => setCmd(e.target.value)} placeholder="git clone" />
          </Field>
          <div className="row"><Button variant="primary" onClick={doAmend}>确认改后批</Button><Button onClick={() => setAmend(null)}>取消</Button></div>
          <div className="hint">原命令: <span className="mono">{amend.command}</span></div>
        </Modal>
      )}
    </Card>
  );
}
