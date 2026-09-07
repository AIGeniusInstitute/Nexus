import React from "react";
import { api, type GoldenSet, type GoldenSetCase, type Rubric, type EvalBatchRun, type EntityVersion, type CompareResult, type JudgeConfig, type DomainEvent, type EsignRecord, type LineageGraph } from "../api";
import { Card, Table, Button, Pill, Empty, ErrBar, Modal, Field, useAsync, fmtTime } from "../ui";

type Tab = "gs" | "rubric" | "judge" | "runs" | "lineage";

export default function GoldenSets() {
  const [tab, setTab] = React.useState<Tab>("gs");
  return (
    <div>
      <div style={{ display: "flex", gap: 8, marginBottom: 16 }}>
        {([["gs", "Golden Set"], ["rubric", "Rubric 评分标准"], ["judge", "Judge 配置"], ["runs", "批量评测运行"], ["lineage", "溯源 / 签名"]] as [Tab, string][]).map(([k, l]) => (
          <Button key={k} variant={tab === k ? "primary" : "default"} className="sm" onClick={() => setTab(k)}>{l}</Button>
        ))}
      </div>
      {tab === "gs" && <GoldenSetTab />}
      {tab === "rubric" && <RubricTab />}
      {tab === "judge" && <JudgeTab />}
      {tab === "runs" && <RunsTab />}
      {tab === "lineage" && <LineageTab />}
    </div>
  );
}

// ───────────────── Golden Set ─────────────────

function GoldenSetTab() {
  const list = useAsync<GoldenSet[]>(() => api.goldenSets(), []);
  const [create, setCreate] = React.useState(false);
  const [detail, setDetail] = React.useState<GoldenSet | null>(null);
  return (
    <div>
      <Card title={`Golden Set (${list.data?.length || 0})`} action={<Button variant="primary" className="sm" onClick={() => setCreate(true)}>+ 新建</Button>}>
        <ErrBar err={list.err} />
        {list.data && list.data.length === 0 && <Empty>暂无 Golden Set</Empty>}
        {list.data && list.data.length > 0 && (
          <Table cols={[
            { key: "id", label: "ID", render: (g: GoldenSet) => g.id },
            { key: "name", label: "名称", render: (g: GoldenSet) => <a onClick={() => setDetail(g)} style={{ cursor: "pointer" }}><b>{g.name}</b></a> },
            { key: "status", label: "状态", render: (g: GoldenSet) => <Pill tone={g.status === "locked" ? "ok" : "info"}>{g.status}</Pill> },
            { key: "ver", label: "版本", render: (g: GoldenSet) => g.active_version_id ? <Pill tone="ok">v{g.active_version_id}</Pill> : <span className="muted">无</span> },
            { key: "time", label: "创建", render: (g: GoldenSet) => fmtTime(g.created_at) },
          ]} rows={list.data} />
        )}
      </Card>
      {create && <CreateGsModal onClose={() => setCreate(false)} onDone={() => { setCreate(false); list.reload(); }} />}
      {detail && <GsDetailModal gs={detail} onClose={() => setDetail(null)} onChanged={() => list.reload()} />}
    </div>
  );
}

function CreateGsModal({ onClose, onDone }: { onClose: () => void; onDone: () => void }) {
  const [name, setName] = React.useState("");
  const [desc, setDesc] = React.useState("");
  const [err, setErr] = React.useState<string | null>(null);
  return (
    <Modal title="新建 Golden Set" onClose={onClose}>
      <Field label="名称"><input className="input" value={name} onChange={(e) => setName(e.target.value)} placeholder="如：入排筛选评测集" /></Field>
      <Field label="描述"><textarea className="input" value={desc} onChange={(e) => setDesc(e.target.value)} /></Field>
      <ErrBar err={err} />
      <Button variant="primary" onClick={async () => { try { await api.createGoldenSet(name, desc || undefined); onDone(); } catch (e: any) { setErr(String(e?.message || e)); } }}>创建</Button>
    </Modal>
  );
}

function GsDetailModal({ gs, onClose, onChanged }: { gs: GoldenSet; onClose: () => void; onChanged: () => void }) {
  const [cases, setCases] = React.useState<GoldenSetCase[]>([]);
  const [err, setErr] = React.useState<string | null>(null);
  const [add, setAdd] = React.useState(false);
  const [pub, setPub] = React.useState<EntityVersion | null>(null);
  async function load() { try { const r = await api.listCases(gs.id); setCases(r); } catch (e: any) { setErr(String(e)); } }
  React.useEffect(() => { load(); }, []);
  const isLocked = gs.status === "locked";
  return (
    <Modal title={`Golden Set: ${gs.name}`} onClose={onClose}>
      <div className="hint" style={{ marginBottom: 10 }}>
        状态: <Pill tone={isLocked ? "ok" : "info"}>{gs.status}</Pill>
        {gs.active_version_id ? ` 版本 v${gs.active_version_id}` : " 未发布"}
      </div>
      <div style={{ display: "flex", gap: 8, marginBottom: 10 }}>
        {!isLocked && <Button className="sm" onClick={() => setAdd(true)}>+ 添加 Case</Button>}
        {!isLocked && cases.length > 0 && (
          <Button variant="primary" className="sm" onClick={async () => { try { const v = await api.publishGoldenSet(gs.id, "patch"); setPub(v); onChanged(); } catch (e: any) { setErr(String(e?.message || e)); } }}>🔒 发布版本</Button>
        )}
      </div>
      <ErrBar err={err} />
      {pub && <div className="ok-bar" style={{ padding: 10, background: "var(--panel2)", borderRadius: 8, marginBottom: 10 }}>
        ✅ 版本已锁定: <b>{pub.semver}</b> (v{pub.version_no}) · hash: <code style={{ fontSize: 11 }}>{pub.manifest_hash.slice(0, 16)}…</code>
      </div>}
      {cases.length === 0 ? <Empty>暂无 Case</Empty> : (
        <Table cols={[
          { key: "key", label: "Case Key", render: (c: GoldenSetCase) => <code>{c.case_key}</code> },
          { key: "dom", label: "领域", render: (c: GoldenSetCase) => c.domain || "—" },
          { key: "diff", label: "难度", render: (c: GoldenSetCase) => c.difficulty ? <Pill tone="warn">{c.difficulty}</Pill> : "—" },
          { key: "must", label: "必命中点", render: (c: GoldenSetCase) => (c.expected_json?.must_hit_points?.length || 0) + " 项" },
        ]} rows={cases} />
      )}
      {add && <AddCaseModal gsId={gs.id} onClose={() => setAdd(false)} onDone={() => { setAdd(false); load(); }} />}
    </Modal>
  );
}

function AddCaseModal({ gsId, onClose, onDone }: { gsId: number; onClose: () => void; onDone: () => void }) {
  const [key, setKey] = React.useState("");
  const [domain, setDomain] = React.useState("");
  const [diff, setDiff] = React.useState("P1");
  const [query, setQuery] = React.useState("");
  const [mustHit, setMustHit] = React.useState("");
  const [mustNot, setMustNot] = React.useState("");
  const [err, setErr] = React.useState<string | null>(null);
  async function submit() {
    const input_json = { user_query: query };
    const expected_json = {
      must_hit_points: mustHit.split("\n").map((s) => s.trim()).filter(Boolean),
      must_not: mustNot.split("\n").map((s) => s.trim()).filter(Boolean),
      acceptable_answers: [],
    };
    try { await api.addCase(gsId, { case_key: key, domain: domain || undefined, difficulty: diff, input_json, expected_json }); onDone(); }
    catch (e: any) { setErr(String(e?.message || e)); }
  }
  return (
    <Modal title="添加 Case" onClose={onClose}>
      <Field label="Case Key"><input className="input" value={key} onChange={(e) => setKey(e.target.value)} placeholder="CS-ELIG-0231" /></Field>
      <div style={{ display: "flex", gap: 8 }}>
        <Field label="领域"><input className="input" value={domain} onChange={(e) => setDomain(e.target.value)} placeholder="入排筛选" /></Field>
        <Field label="难度"><select className="input" value={diff} onChange={(e) => setDiff(e.target.value)}><option>P1</option><option>P2</option><option>P3</option></select></Field>
      </div>
      <Field label="用户查询 (input)"><textarea className="input" rows={3} value={query} onChange={(e) => setQuery(e.target.value)} placeholder="患者男，58岁，肌酐清除率48mL/min…" /></Field>
      <Field label="必命中点 (每行一项)" hint="must_hit_points"><textarea className="input" rows={3} value={mustHit} onChange={(e) => setMustHit(e.target.value)} placeholder="引用方案§5.2肌酐界值&#10;给出排除结论及依据" /></Field>
      <Field label="禁止项 (每行一项)" hint="must_not"><textarea className="input" rows={2} value={mustNot} onChange={(e) => setMustNot(e.target.value)} placeholder="给出医疗处置建议&#10;输出任何PHI原文" /></Field>
      <ErrBar err={err} />
      <Button variant="primary" onClick={submit}>添加</Button>
    </Modal>
  );
}

// ───────────────── Rubric ─────────────────

function RubricTab() {
  const list = useAsync<Rubric[]>(() => api.rubrics(), []);
  const [create, setCreate] = React.useState(false);
  const [publish, setPublish] = React.useState<Rubric | null>(null);
  return (
    <div>
      <Card title={`Rubric 评分标准 (${list.data?.length || 0})`} action={<Button variant="primary" className="sm" onClick={() => setCreate(true)}>+ 新建</Button>}>
        <ErrBar err={list.err} />
        {list.data && list.data.length === 0 && <Empty>暂无 Rubric</Empty>}
        {list.data && list.data.length > 0 && (
          <Table cols={[
            { key: "id", label: "ID", render: (r: Rubric) => r.id },
            { key: "name", label: "名称", render: (r: Rubric) => <b>{r.name}</b> },
            { key: "status", label: "状态", render: (r: Rubric) => <Pill tone={r.status === "locked" ? "ok" : "info"}>{r.status}</Pill> },
            { key: "act", label: "", render: (r: Rubric) => r.status !== "locked" ? <Button className="sm" onClick={() => setPublish(r)}>发布 →</Button> : null },
          ]} rows={list.data} />
        )}
      </Card>
      {create && <CreateRubricModal onClose={() => setCreate(false)} onDone={() => { setCreate(false); list.reload(); }} />}
      {publish && <PublishRubricModal rb={publish} onClose={() => setPublish(null)} onDone={() => { setPublish(null); list.reload(); }} />}
    </div>
  );
}

function CreateRubricModal({ onClose, onDone }: { onClose: () => void; onDone: () => void }) {
  const [name, setName] = React.useState("");
  const [err, setErr] = React.useState<string | null>(null);
  return (
    <Modal title="新建 Rubric" onClose={onClose}>
      <Field label="名称"><input className="input" value={name} onChange={(e) => setName(e.target.value)} placeholder="如：入排筛选评分标准" /></Field>
      <ErrBar err={err} />
      <Button variant="primary" onClick={async () => { try { await api.createRubric(name); onDone(); } catch (e: any) { setErr(String(e?.message || e)); } }}>创建</Button>
    </Modal>
  );
}

function PublishRubricModal({ rb, onClose, onDone }: { rb: Rubric; onClose: () => void; onDone: () => void }) {
  const [bump, setBump] = React.useState("patch");
  const [criteria, setCriteria] = React.useState('[{"name":"must_hit_rate","weight":1.0},{"name":"must_not_compliance","weight":1.0}]');
  const [err, setErr] = React.useState<string | null>(null);
  const [result, setResult] = React.useState<EntityVersion | null>(null);
  return (
    <Modal title={`发布 Rubric: ${rb.name}`} onClose={onClose}>
      <Field label="版本类型"><select className="input" value={bump} onChange={(e) => setBump(e.target.value)}><option value="patch">patch (勘误)</option><option value="minor">minor (新增)</option><option value="major">major (破坏性)</option></select></Field>
      <Field label="评分点 JSON" hint="criterion 列表"><textarea className="input" rows={4} value={criteria} onChange={(e) => setCriteria(e.target.value)} /></Field>
      <ErrBar err={err} />
      {result && <div className="ok-bar" style={{ padding: 10, background: "var(--panel2)", borderRadius: 8, marginBottom: 10 }}>✅ {result.semver}</div>}
      <Button variant="primary" onClick={async () => { setErr(null); try { const v = await api.publishRubric(rb.id, bump, JSON.parse(criteria)); setResult(v); } catch (e: any) { setErr(String(e?.message || e)); } }}>发布</Button>
    </Modal>
  );
}

// ───────────────── 批量评测运行 ─────────────────

function RunsTab() {
  const list = useAsync<EvalBatchRun[]>(() => api.batchRuns(30), []);
  const [start, setStart] = React.useState(false);
  const [detail, setDetail] = React.useState<EvalBatchRun | null>(null);
  return (
    <div>
      <Card title={`批量评测运行 (${list.data?.length || 0})`} action={<Button variant="primary" className="sm" onClick={() => setStart(true)}>+ 启动运行</Button>}>
        <ErrBar err={list.err} />
        {list.data && list.data.length === 0 && <Empty>暂无运行</Empty>}
        {list.data && list.data.length > 0 && (
          <Table cols={[
            { key: "id", label: "ID", render: (r: EvalBatchRun) => r.id },
            { key: "gs", label: "GoldenSet", render: (r: EvalBatchRun) => `#${r.golden_set_id} v${r.golden_set_version_id}` },
            { key: "status", label: "状态", render: (r: EvalBatchRun) => <Pill tone={r.status === "completed" ? "ok" : r.status === "failed" ? "danger" : "warn"}>{r.status}</Pill> },
            { key: "agg", label: "聚合", render: (r: EvalBatchRun) => r.aggregate ? <span className="mono" style={{ fontSize: 12 }}>acc={((r.aggregate.accuracy as number) * 100).toFixed(0)}% pass={r.aggregate.passed as number}/{r.aggregate.total_cases as number}</span> : "—" },
            { key: "hash", label: "快照 hash", className: "mono", render: (r: EvalBatchRun) => <span style={{ fontSize: 11 }}>{r.snapshot_hash.slice(0, 12)}…</span> },
            { key: "time", label: "时间", render: (r: EvalBatchRun) => fmtTime(r.started_at) },
            { key: "act", label: "", render: (r: EvalBatchRun) => <Button className="sm" onClick={() => setDetail(r)}>详情 →</Button> },
          ]} rows={list.data} />
        )}
      </Card>
      {start && <StartRunModal onClose={() => setStart(false)} onDone={() => { setStart(false); list.reload(); }} />}
      {detail && <RunDetailModal run={detail} onClose={() => setDetail(null)} />}
    </div>
  );
}

function StartRunModal({ onClose, onDone }: { onClose: () => void; onDone: () => void }) {
  const gsList = useAsync<GoldenSet[]>(() => api.goldenSets(), []);
  const threadList = useAsync(() => api.listThreads(), []);
  const [gsId, setGsId] = React.useState<number>(0);
  const [threadId, setThreadId] = React.useState("");
  const [err, setErr] = React.useState<string | null>(null);
  const [busy, setBusy] = React.useState(false);
  const lockedGs = gsList.data?.filter((g) => g.status === "locked") || [];
  return (
    <Modal title="启动批量评测运行" onClose={onClose}>
      <Field label="Golden Set (已发布)"><select className="input" value={gsId} onChange={(e) => setGsId(Number(e.target.value))}><option value={0}>选择…</option>{lockedGs.map((g) => <option key={g.id} value={g.id}>{g.name} (v{g.active_version_id})</option>)}</select></Field>
      <Field label="Agent 线程"><select className="input" value={threadId} onChange={(e) => setThreadId(e.target.value)}><option value="">选择…</option>{(threadList.data || []).map((t) => <option key={t.id} value={t.id}>{t.id.slice(0, 8)}… {t.title || ""}</option>)}</select></Field>
      <div className="hint">运行将冻结快照，逐 case 驱动 agent turn，评分并归档。</div>
      <ErrBar err={err} />
      <Button variant="primary" disabled={busy || !gsId || !threadId} onClick={async () => { setBusy(true); setErr(null); try { await api.startBatchRun({ golden_set_id: gsId, thread_id: threadId }); onDone(); } catch (e: any) { setErr(String(e?.message || e)); } finally { setBusy(false); } }}>{busy ? "运行中…" : "启动"}</Button>
    </Modal>
  );
}

function RunDetailModal({ run, onClose }: { run: EvalBatchRun; onClose: () => void }) {
  const data = useAsync(() => api.batchRun(run.id), [run.id]);
  const [cmpBaseline, setCmpBaseline] = React.useState("");
  const [cmp, setCmp] = React.useState<CompareResult | null>(null);
  return (
    <Modal title={`运行 #${run.id} 详情`} onClose={onClose}>
      <div className="hint" style={{ marginBottom: 8 }}>
        状态: <Pill tone={run.status === "completed" ? "ok" : "warn"}>{run.status}</Pill>
        {" "}GoldenSet #{run.golden_set_id} v{run.golden_set_version_id} · 触发: {run.trigger_type}
      </div>
      <div className="hint" style={{ marginBottom: 8 }}>
        快照 hash: <code style={{ fontSize: 11 }}>{run.snapshot_hash}</code>
      </div>
      {run.aggregate && (
        <div style={{ padding: 10, background: "var(--panel2)", borderRadius: 8, marginBottom: 10 }}>
          <b>聚合</b>: accuracy={((run.aggregate.accuracy as number) * 100).toFixed(1)}% · passed={run.aggregate.passed as number}/{run.aggregate.total_cases as number}
          {" "}· avg_hit_rate={((run.aggregate.avg_must_hit_rate as number) * 100).toFixed(1)}% · 违规={run.aggregate.must_not_violation_count as number}
        </div>
      )}
      <ErrBar err={data.err} />
      {data.data && (
        <>
          <div style={{ fontWeight: 600, marginBottom: 6 }}>逐 Case 结果 ({data.data.case_results.length})</div>
          {data.data.case_results.length > 0 && (
            <Table cols={[
              { key: "key", label: "Case", render: (c: any) => <code style={{ fontSize: 12 }}>{c.case_key}</code> },
              { key: "passed", label: "结果", render: (c: any) => c.scores?.passed ? <Pill tone="ok">PASS</Pill> : <Pill tone="danger">FAIL</Pill> },
              { key: "hit", label: "命中率", render: (c: any) => c.scores?.must_hit_rate != null ? `${(c.scores.must_hit_rate * 100).toFixed(0)}%` : "—" },
              { key: "viol", label: "违规", render: (c: any) => (c.scores?.must_not_violations?.length || 0) > 0 ? <Pill tone="danger">{c.scores.must_not_violations.length}</Pill> : <span className="muted">0</span> },
            ]} rows={data.data.case_results} />
          )}
        </>
      )}
      <div style={{ borderTop: "1px solid var(--bd)", marginTop: 12, paddingTop: 12 }}>
        <div style={{ fontWeight: 600, marginBottom: 6 }}>基线对比</div>
        <div style={{ display: "flex", gap: 8 }}>
          <input className="input" type="number" value={cmpBaseline} onChange={(e) => setCmpBaseline(e.target.value)} placeholder="baseline run ID" />
          <Button className="sm" onClick={async () => { try { const r = await api.compareBatchRun(run.id, Number(cmpBaseline)); setCmp(r); } catch (e: any) { setCmp(null); alert(String(e)); } }}>对比</Button>
        </div>
        {cmp && (
          <div style={{ marginTop: 8 }}>
            {cmp.incomparable && <Pill tone="warn">⚠ Golden Set 版本不同，不可直接对比</Pill>}
            {!cmp.incomparable && <Pill tone="ok">同版本，可对比</Pill>}
            {cmp.diffs.length > 0 && (
              <Table cols={[
                { key: "key", label: "Case", render: (d: any) => <code style={{ fontSize: 12 }}>{d.case_key}</code> },
                { key: "base", label: "基线", render: (d: any) => d.baseline_passed ? <Pill tone="ok">PASS</Pill> : <Pill tone="danger">FAIL</Pill> },
                { key: "run", label: "本次", render: (d: any) => d.run_passed ? <Pill tone="ok">PASS</Pill> : <Pill tone="danger">FAIL</Pill> },
                { key: "delta", label: "变化", render: (d: any) => <Pill tone={d.delta === "improved" ? "ok" : d.delta === "regressed" ? "danger" : "mut"}>{d.delta}</Pill> },
              ]} rows={cmp.diffs} />
            )}
          </div>
        )}
      </div>
    </Modal>
  );
}

// ───────────────── Judge 配置 (M21) ─────────────────

function JudgeTab() {
  const list = useAsync(() => api.judgeConfigs(), []);
  const [name, setName] = React.useState("");
  const [desc, setDesc] = React.useState("");
  const [modal, setModal] = React.useState<JudgeConfig | null>(null);
  const [dimName, setDimName] = React.useState("");
  const [dimWeight, setDimWeight] = React.useState("0.5");
  const [dimDesc, setDimDesc] = React.useState("");
  const [dims, setDims] = React.useState<any[]>([]);
  const [pub, setPub] = React.useState<EntityVersion | null>(null);
  return (
    <Card>
      <h3>Judge LLM 配置</h3>
      <p className="muted" style={{ fontSize: 13, margin: "4px 0 12px" }}>版本化的 LLM 评分配置：prompt 模板 + 评分维度 + judge 模型。draft→publish(locked 不可变)。</p>
      {list.err && <ErrBar err={list.err} />}
      <Table cols={[
        { key: "id", label: "ID", render: (r: JudgeConfig) => r.id },
        { key: "name", label: "名称", render: (r: JudgeConfig) => <a onClick={() => { setModal(r); setDims(r.description ? (JSON.parse(r.description) as any[]) : []); }}>{r.name}</a> },
        { key: "status", label: "状态", render: (r: JudgeConfig) => <Pill tone={r.status === "locked" ? "info" : "mut"}>{r.status}</Pill> },
        { key: "vid", label: "版本", render: (r: JudgeConfig) => r.active_version_id ?? "—" },
        { key: "time", label: "时间", render: (r: JudgeConfig) => fmtTime(r.updated_at) },
      ]} rows={list.data ?? []} />
      {list.data && list.data.length === 0 && <Empty>暂无 Judge 配置</Empty>}
      <div style={{ marginTop: 12, display: "flex", gap: 8, flexWrap: "wrap" }}>
        <Field label="名称"><input className="input" value={name} onChange={(e) => setName(e.target.value)} placeholder="如：临床推理评分器" /></Field>
        <Field label="描述"><input className="input" value={desc} onChange={(e) => setDesc(e.target.value)} /></Field>
        <Button variant="primary" onClick={async () => { try { await api.createJudgeConfig(name, desc); setName(""); setDesc(""); list.reload(); } catch (e: any) { alert(String(e)); } }}>创建</Button>
      </div>

      {modal && (
      <Modal onClose={() => { setModal(null); setPub(null); }} title={`Judge: ${modal.name}`}>
          <div>
            <div style={{ marginBottom: 12 }}>
              <Pill tone={modal.status === "locked" ? "info" : "mut"}>{modal.status}</Pill>
              {modal.active_version_id && <span style={{ marginLeft: 8 }} className="muted">version #{modal.active_version_id}</span>}
            </div>
            {modal.status === "draft" && (
              <div style={{ marginBottom: 16, padding: 12, border: "1px solid var(--border)", borderRadius: 8 }}>
                <div style={{ fontWeight: 600, marginBottom: 8 }}>添加评分维度</div>
                <div style={{ display: "flex", gap: 8, flexWrap: "wrap" }}>
                  <Field label="维度名"><input className="input" value={dimName} onChange={(e) => setDimName(e.target.value)} placeholder="accuracy" /></Field>
                  <Field label="权重"><input className="input" style={{ width: 80 }} value={dimWeight} onChange={(e) => setDimWeight(e.target.value)} /></Field>
                  <Field label="描述"><input className="input" value={dimDesc} onChange={(e) => setDimDesc(e.target.value)} /></Field>
                  <Button className="sm" onClick={async () => {
                    if (!modal) return;
                    try { await api.addJudgeDimension(modal.id, { name: dimName, weight: Number(dimWeight), description: dimDesc }); setDims([...dims, { name: dimName, weight: Number(dimWeight), description: dimDesc }]); setDimName(""); setDimDesc(""); list.reload(); } catch (e: any) { alert(String(e)); }
                  }}>添加</Button>
                </div>
                <Button variant="primary" className="sm" style={{ marginTop: 12 }} onClick={async () => {
                  if (!modal) return;
                  try { const v = await api.publishJudgeConfig(modal.id, "major", { prompt_template: "你是临床研究评测专家。根据评分维度对 Agent 回复逐项打分（0.0-1.0），返回纯 JSON。", dimensions: dims, temperature: 0.0 }); setPub(v); list.reload(); } catch (e: any) { alert(String(e)); }
                }}>发布 v1.0.0</Button>
              </div>
            )}
            {pub && <Pill tone="ok">✅ 已发布 v{pub.semver} (hash: {pub.manifest_hash.slice(0, 12)}…)</Pill>}
            {dims.length > 0 && (
              <div>
                <div style={{ fontWeight: 600, margin: "8px 0" }}>评分维度</div>
                {dims.map((d, i) => <div key={i} style={{ fontSize: 13, padding: "4px 0" }}>• {d.name} (权重 {d.weight}) — {d.description}</div>)}
              </div>
            )}
          </div>
      </Modal>
      )}
    </Card>
  );
}

// ───────────────── M22: 溯源 / 签名 ─────────────────
function LineageTab() {
  const [etype, setEtype] = React.useState("eval_batch_run");
  const [eid, setEid] = React.useState("");
  const [sig, setSig] = React.useState<{ purpose: string; meaning: string }>({ purpose: "审核批准", meaning: "本人审核确认此评测结果可发布" });
  const ev = useAsync(() => eid ? api.events(etype, Number(eid)) : Promise.resolve(null), [etype, eid]);
  const lg = useAsync(() => eid ? api.lineage(etype, Number(eid)) : Promise.resolve(null), [etype, eid]);
  const sigs = useAsync(() => eid ? api.esigns(etype, Number(eid)) : Promise.resolve([]), [etype, eid]);
  const [verify, setVerify] = React.useState<{ valid: boolean } | null>(null);

  function doSign() {
    api.esign({ entity_type: etype, entity_id: Number(eid), signature_purpose: sig.purpose, meaning_text: sig.meaning })
      .then(() => { sigs.reload(); setVerify(null); })
      .catch((e) => alert(String(e)));
  }
  function doVerify(id: number) {
    api.verifyEsign(id).then((v) => setVerify(v)).catch((e) => alert(String(e)));
  }

  return (
    <Card title="溯源 · 事件流 · 电子签名（M22）">
      <Field label="实体类型">
        <select className="input" value={etype} onChange={(e) => setEtype(e.target.value)}>
          <option value="eval_batch_run">eval_batch_run</option>
          <option value="golden_set">golden_set</option>
          <option value="judge_config">judge_config</option>
          <option value="approval">approval</option>
        </select>
      </Field>
      <Field label="实体 ID">
        <input className="input" value={eid} onChange={(e) => setEid(e.target.value)} placeholder="如 5" />
      </Field>

      <h4 style={{ marginTop: 18 }}>事件流（as-of 回放）</h4>
      <ErrBar err={ev.err} />
      {ev.data?.events && ev.data.events.length === 0 && <Empty>暂无事件</Empty>}
      {ev.data?.events && ev.data.events.length > 0 && (
        <table className="tbl"><thead><tr><th>seq</th><th>event_type</th><th>occurred_at</th><th>payload</th></tr></thead><tbody>
          {ev.data.events.map((e: DomainEvent) => (
            <tr key={e.id}><td>{e.event_seq}</td><td><Pill tone="info">{e.event_type}</Pill></td><td>{fmtTime(e.occurred_at)}</td><td><code style={{ fontSize: 11 }}>{JSON.stringify(e.payload).slice(0, 80)}</code></td></tr>
          ))}</tbody></table>
      )}
      {ev.data?.replayed_state && (
        <details style={{ marginTop: 8 }}><summary>回放状态快照</summary><pre style={{ fontSize: 11, overflow: "auto" }}>{JSON.stringify(ev.data.replayed_state, null, 2)}</pre></details>
      )}

      <h4 style={{ marginTop: 18 }}>血缘图（Lineage）</h4>
      <ErrBar err={lg.err} />
      {lg.data && lg.data.nodes.length > 0 && (
        <div style={{ display: "flex", flexWrap: "wrap", gap: 6 }}>
          {lg.data.nodes.map((n) => (<Pill key={n.id} tone="info">{n.kind}: {n.label}</Pill>))}
        </div>
      )}
      {lg.data && lg.data.edges.length > 0 && (
        <table className="tbl"><thead><tr><th>from</th><th>relation</th><th>to</th></tr></thead><tbody>
          {lg.data.edges.map((e, i) => (<tr key={i}><td>{e.from}</td><td>{e.relation}</td><td>{e.to}</td></tr>))}
        </tbody></table>
      )}

      <h4 style={{ marginTop: 18 }}>电子签名（21 CFR Part 11）</h4>
      <Field label="签名含义（purpose）"><input className="input" value={sig.purpose} onChange={(e) => setSig({ ...sig, purpose: e.target.value })} /></Field>
      <Field label="含义声明（meaning）"><input className="input" value={sig.meaning} onChange={(e) => setSig({ ...sig, meaning: e.target.value })} /></Field>
      <Button variant="primary" className="sm" onClick={doSign}>签署</Button>
      <ErrBar err={sigs.err} />
      {sigs.data && sigs.data.length > 0 && (
        <table className="tbl"><thead><tr><th>id</th><th>signer</th><th>purpose</th><th>signed_at</th><th>verify</th></tr></thead><tbody>
          {sigs.data.map((s: EsignRecord) => (
            <tr key={s.id}><td>{s.id}</td><td>user#{s.signer_user_id}</td><td>{s.signature_purpose}</td><td>{fmtTime(s.signed_at)}</td>
              <td><Button className="sm" onClick={() => doVerify(s.id)}>验证</Button></td></tr>
          ))}</tbody></table>
      )}
      {verify && (
        <div style={{ marginTop: 8 }}>
          <Pill tone={verify.valid ? "ok" : "warn"}>{verify.valid ? "✓ 签名有效" : "✗ 签名校验失败"}</Pill>
        </div>
      )}
    </Card>
  );
}
