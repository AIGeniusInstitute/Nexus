import React from "react";
import { api } from "../api";
import { Card, useAsync, fmtNum, fmtCost, Pill, Button } from "../ui";

const MS = [
  ["M1", "身份+骨架", "ok"], ["M2", "执行闭环", "ok"], ["M3", "审批+策略", "ok"], ["M4", "产物+计量", "ok"],
  ["M5", "并发Turn池", "ok"], ["M6", "策略自学习", "ok"], ["M7", "execpolicy回写", "ok"], ["M8", "真实模型联调", "ok"],
  ["M9", "function calling", "ok"], ["M10", "审计WORM", "ok"], ["M11", "全链路tracing", "ok"], ["M12", "评测/CI门禁", "ok"],
  ["M13", "知识库RAG", "ok"], ["M14", "快照Fork", "ok"], ["M15", "Warm Pool", "ok"], ["M16", "连接器市场", "ok"],
  ["M17", "Skills市场", "ok"], ["M18", "多Agent协作", "ok"], ["M19", "MCP Gateway", "ok"],
] as const;

export default function Overview() {
  const pool = useAsync(() => api.poolStatus(), []);
  const approvals = useAsync(() => api.listApprovals(), []);
  const usage = useAsync(() => api.getUsage(7), []);
  const kbs = useAsync(() => api.kbs(), []);
  const connectors = useAsync(() => api.connectors(), []);
  const skills = useAsync(() => api.skills(), []);

  const pending = (approvals.data || []).filter((a) => a.status === "pending").length;
  const totIn = (usage.data || []).reduce((s, d) => s + d.total_input_tokens, 0);
  const totOut = (usage.data || []).reduce((s, d) => s + d.total_output_tokens, 0);
  const totTurns = (usage.data || []).reduce((s, d) => s + d.total_turns, 0);
  const totCost = (usage.data || []).reduce((s, d) => s + d.total_cost_micros, 0);

  return (
    <div>
      <StatGrid>
        <Stat n={pool.data ? `${pool.data.warmed}/${pool.data.pool_size}` : "—"} l="Runtime Pool (warm)" cls="grn" />
        <Stat n={pool.data ? pool.data.free : "—"} l="空闲槽位" cls="acc" />
        <Stat n={pending} l="待审批工单" cls={pending > 0 ? "gold" : "acc"} />
        <Stat n={fmtNum(totTurns)} l="近7天 Turns" cls="blue" />
        <Stat n={fmtNum(totIn)} l="输入 Tokens" cls="acc" />
        <Stat n={fmtNum(totOut)} l="输出 Tokens" cls="acc" />
        <Stat n={fmtCost(totCost)} l="费用 (元)" cls="gold" />
        <Stat n={(kbs.data || []).length} l="知识库" cls="blue" />
        <Stat n={(connectors.data || []).length} l="连接器" cls="purple" />
        <Stat n={(skills.data || []).length} l="技能" cls="purple" />
      </StatGrid>

      <Card title="里程碑交付状态（M0–M19）" action={<Pill tone="ok">19/19 全交付</Pill>}>
        <div style={{ display: "grid", gridTemplateColumns: "repeat(auto-fill, minmax(160px, 1fr))", gap: 8 }}>
          {MS.map(([m, name]) => (
            <div key={m} style={{ display: "flex", alignItems: "center", gap: 8, padding: "8px 10px", background: "var(--panel2)", borderRadius: 6, border: "1px solid var(--bd)" }}>
              <span className="badge-dot" style={{ background: "var(--grn)" }} />
              <span style={{ fontWeight: 700, fontSize: 12 }}>{m}</span>
              <span className="muted" style={{ fontSize: 12 }}>{name}</span>
            </div>
          ))}
        </div>
      </Card>

      <Card title="系统架构" action={<span className="muted">核心引擎 + 外围管控</span>}>
        <div style={{ display: "grid", gridTemplateColumns: "repeat(auto-fit, minmax(220px, 1fr))", gap: 12 }}>
          <ArchBox title="控制面（nexus-control）" items={["身份 RBAC / 限流幂等", "DriverPool 并发 turn", "审批 HITL 闭环", "策略自学习 + amendment", "计量 cost 推导", "审计 WORM + tracing"]} />
          <ArchBox title="执行面（codex app-server）" items={["stdio JSON-RPC", "thread/turn 原语", "approval 协议级回写", "execpolicy Starlark 规则", "三层沙箱"]} />
          <ArchBox title="数据面" items={["Postgres + pgvector", "会话云端持久化", "事件即真相 seq 幂等", "WORM 审计 trigger", "HNSW 向量召回"]} />
        </div>
      </Card>
    </div>
  );
}

function Stat({ n, l, cls }: { n: React.ReactNode; l: string; cls: string }) {
  return <div className="stat"><div className={`n ${cls}`}>{n}</div><div className="l">{l}</div></div>;
}
function StatGrid({ children }: { children: React.ReactNode }) { return <div className="stat-grid">{children}</div>; }

function ArchBox({ title, items }: { title: string; items: string[] }) {
  return (
    <div style={{ background: "var(--panel2)", borderRadius: 8, padding: 14, border: "1px solid var(--bd)" }}>
      <div style={{ fontWeight: 700, marginBottom: 8, color: "var(--acc)" }}>{title}</div>
      {items.map((it) => <div key={it} style={{ fontSize: 12.5, color: "var(--mut)", padding: "2px 0" }}>· {it}</div>)}
    </div>
  );
}
