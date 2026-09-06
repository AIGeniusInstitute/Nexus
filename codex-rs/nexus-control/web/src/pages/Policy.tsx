import React from "react";
import { api, type PolicyRule, type PolicyFeedback } from "../api";
import { Card, Table, Button, Pill, Empty, ErrBar, useAsync, fmtTime } from "../ui";

export default function Policy() {
  const rules = useAsync<PolicyRule[]>(() => api.policyRules(), []);
  const fb = useAsync<PolicyFeedback[]>(() => api.policyFeedback(30), []);
  return (
    <div>
      <Card title={`策略规则 (${rules.data?.length || 0})`} action={<span className="muted">下发到 tenant .rules (app-server 每 turn 加载)</span>}>
        <ErrBar err={rules.err} />
        {rules.data && rules.data.length === 0 && <Empty>暂无规则</Empty>}
        {rules.data && rules.data.length > 0 && (
          <Table cols={[
            { key: "id", label: "ID", render: (r: PolicyRule) => r.id },
            { key: "role", label: "角色", render: (r: PolicyRule) => r.role },
            { key: "kind", label: "类型", render: (r: PolicyRule) => r.action_kind },
            { key: "pattern", label: "模式", className: "mono", render: (r: PolicyRule) => r.pattern },
            { key: "decision", label: "决策", render: (r: PolicyRule) => <Pill tone={r.decision === "deny" ? "danger" : r.decision === "allow" ? "ok" : "warn"}>{r.decision}</Pill> },
            { key: "risk", label: "风险", render: (r: PolicyRule) => <Pill tone={r.risk_level === "high" ? "danger" : r.risk_level === "medium" ? "warn" : "ok"}>{r.risk_level || "—"}</Pill> },
            { key: "source", label: "来源", render: (r: PolicyRule) => <Pill tone="mut">{r.source || "seed"}</Pill> },
            { key: "pri", label: "优先级", render: (r: PolicyRule) => r.priority },
            { key: "en", label: "启用", render: (r: PolicyRule) => r.enabled ? <Pill tone="ok">是</Pill> : <Pill tone="danger">否</Pill> },
          ]} rows={rules.data} />
        )}
      </Card>
      <Card title={`人决策反馈历史 (${fb.data?.length || 0})`} action={<span className="muted">M6 自学习闭环数据源</span>}>
        <ErrBar err={fb.err} />
        {fb.data && fb.data.length === 0 && <Empty>暂无反馈</Empty>}
        {fb.data && fb.data.length > 0 && (
          <Table cols={[
            { key: "id", label: "ID", render: (f: PolicyFeedback) => f.id },
            { key: "pattern", label: "模式", className: "mono", render: (f: PolicyFeedback) => f.pattern },
            { key: "decision", label: "人决策", render: (f: PolicyFeedback) => <Pill tone={f.decision === "deny" ? "danger" : "ok"}>{f.decision}</Pill> },
            { key: "rec", label: "策略推荐", render: (f: PolicyFeedback) => <Pill tone="mut">{f.policy_rec || "—"}</Pill> },
            { key: "risk", label: "风险", render: (f: PolicyFeedback) => f.risk_level || "—" },
            { key: "turn", label: "Turn", render: (f: PolicyFeedback) => f.turn_id ?? "—" },
            { key: "time", label: "时间", render: (f: PolicyFeedback) => fmtTime(f.created_at) },
          ]} rows={fb.data} />
        )}
      </Card>
    </div>
  );
}
