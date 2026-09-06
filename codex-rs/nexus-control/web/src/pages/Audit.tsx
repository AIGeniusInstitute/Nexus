import React from "react";
import { api, type AuditLog } from "../api";
import { Card, Table, Button, Pill, Empty, ErrBar, Field, useAsync, fmtTime } from "../ui";

export default function Audit() {
  const [action, setAction] = React.useState("");
  const [since, setSince] = React.useState("");
  const [tick, setTick] = React.useState(0);
  const list = useAsync<AuditLog[]>(() => api.auditLogs({ action: action || undefined, since: since || undefined, limit: 100 }), [tick, action, since]);

  return (
    <Card title={`审计日志 (${list.data?.length || 0})`} action={<Button className="sm" onClick={() => setTick((t) => t + 1)}>刷新</Button>}>
      <ErrBar err={list.err} />
      <div className="row" style={{ marginBottom: 12 }}>
        <Field label="动作过滤"><input className="input" style={{ width: 200 }} value={action} onChange={(e) => setAction(e.target.value)} placeholder="auth.login / turn.complete / …" /></Field>
        <Field label="起始时间 (ISO)"><input className="input" style={{ width: 220 }} value={since} onChange={(e) => setSince(e.target.value)} placeholder="2026-09-06T00:00:00" /></Field>
      </div>
      <div className="hint" style={{ marginBottom: 10 }}><Pill tone="danger">WORM</Pill> 审计日志只追加不可篡改（PG trigger BEFORE UPDATE/DELETE 拒绝）</div>
      {list.data && list.data.length === 0 && <Empty>暂无审计日志</Empty>}
      {list.data && list.data.length > 0 && (
        <Table cols={[
          { key: "id", label: "ID", render: (a: AuditLog) => a.id },
          { key: "action", label: "动作", className: "mono", render: (a: AuditLog) => <Pill tone="info">{a.action}</Pill> },
          { key: "actor", label: "操作者", render: (a: AuditLog) => a.actor_user_id ?? "—" },
          { key: "target", label: "目标", className: "mono", render: (a: AuditLog) => `${a.target_type || "?"}:${a.target_id || "?"}` },
          { key: "trace", label: "Trace", className: "mono", render: (a: AuditLog) => (a.trace_id || "—").slice(0, 8) },
          { key: "detail", label: "详情", className: "wrap", render: (a: AuditLog) => (a.detail_json || "{}").slice(0, 80) },
          { key: "time", label: "时间", render: (a: AuditLog) => fmtTime(a.created_at) },
        ]} rows={list.data} />
      )}
    </Card>
  );
}
