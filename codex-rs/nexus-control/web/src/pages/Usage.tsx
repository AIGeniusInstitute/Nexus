import React from "react";
import { api, type DailyUsage } from "../api";
import { Card, Table, Button, Empty, ErrBar, useAsync, fmtNum, fmtCost, fmtTime } from "../ui";

export default function Usage() {
  const usage = useAsync<DailyUsage[]>(() => api.getUsage(7), []);
  const data = usage.data || [];
  const totIn = data.reduce((s, d) => s + d.total_input_tokens, 0);
  const totOut = data.reduce((s, d) => s + d.total_output_tokens, 0);
  const totTurns = data.reduce((s, d) => s + d.total_turns, 0);
  const totCost = data.reduce((s, d) => s + d.total_cost_micros, 0);
  const maxTok = Math.max(1, ...data.map((d) => d.total_input_tokens + d.total_output_tokens));

  return (
    <div>
      <div className="stat-grid">
        <div className="stat"><div className="n grn">{fmtNum(totTurns)}</div><div className="l">近7天 Turns</div></div>
        <div className="stat"><div className="n acc">{fmtNum(totIn)}</div><div className="l">输入 Tokens</div></div>
        <div className="stat"><div className="n acc">{fmtNum(totOut)}</div><div className="l">输出 Tokens</div></div>
        <div className="stat"><div className="n gold">¥{fmtCost(totCost)}</div><div className="l">费用</div></div>
      </div>
      <Card title="近 7 天用量趋势" action={<Button className="sm" onClick={usage.reload}>刷新</Button>}>
        <ErrBar err={usage.err} />
        {data.length === 0 && !usage.loading && <Empty>暂无用量数据</Empty>}
        <div style={{ display: "flex", alignItems: "flex-end", gap: 10, height: 200, padding: "10px 0" }}>
          {data.map((d) => {
            const tok = d.total_input_tokens + d.total_output_tokens;
            const h = Math.max(2, (tok / maxTok) * 170);
            const inH = (d.total_input_tokens / Math.max(1, tok)) * h;
            return (
              <div key={d.date} style={{ flex: 1, display: "flex", flexDirection: "column", alignItems: "center", gap: 6 }}>
                <div style={{ height: 170, display: "flex", flexDirection: "column-reverse", width: "100%", maxWidth: 60, borderRadius: 4, overflow: "hidden" }}>
                  <div style={{ height: inH, background: "var(--acc)" }} title={`in: ${d.total_input_tokens}`} />
                  <div style={{ height: h - inH, background: "var(--gold)" }} title={`out: ${d.total_output_tokens}`} />
                </div>
                <div className="muted" style={{ fontSize: 10 }}>{d.date.slice(5)}</div>
              </div>
            );
          })}
        </div>
        <div className="row" style={{ marginTop: 10 }}>
          <span className="badge-dot" style={{ background: "var(--acc)" }} /> <span className="muted" style={{ fontSize: 12 }}>输入</span>
          <span className="badge-dot" style={{ background: "var(--gold)", marginLeft: 12 }} /> <span className="muted" style={{ fontSize: 12 }}>输出</span>
        </div>
      </Card>
      <Card title="明细">
        {data.length > 0 && (
          <Table cols={[
            { key: "date", label: "日期", render: (d: DailyUsage) => d.date },
            { key: "in", label: "输入", render: (d: DailyUsage) => fmtNum(d.total_input_tokens) },
            { key: "out", label: "输出", render: (d: DailyUsage) => fmtNum(d.total_output_tokens) },
            { key: "turns", label: "Turns", render: (d: DailyUsage) => d.total_turns },
            { key: "cost", label: "费用", render: (d: DailyUsage) => "¥" + fmtCost(d.total_cost_micros) },
          ]} rows={[...data].reverse()} />
        )}
      </Card>
    </div>
  );
}
