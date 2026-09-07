// Nexus 企业级控制台 API 客户端 — 覆盖 M1-M19 全部后端能力。
const BASE = "/v1";

export interface LoginResp { token: string; user_id: number; perms: string[]; }
export interface Thread { id: string; title: string | null; status: string; created_at: string; }
export interface Item { id: number; turn_id: number; seq: number; item_type: string; content_ref: string | null; created_at: string; }
export interface Approval {
  id: number; thread_id: string; turn_id: number; kind: string | null; status: string;
  command: string | null; cwd: string | null; reason: string | null;
  policy_decision: string | null; risk_level: string | null; created_at: string;
}
export interface DailyUsage { date: string; total_input_tokens: number; total_output_tokens: number; total_cost_micros: number; total_turns: number; }
export interface PoolStatus { pool_size: number; warmed: number; in_flight: number; free: number; }
export interface PolicyRule { id: number; role: string; action_kind: string; pattern: string; decision: string; risk_level: string | null; priority: number; source: string | null; enabled: boolean; }
export interface PolicyFeedback { id: number; pattern: string; decision: string; policy_rec: string | null; risk_level: string | null; turn_id: number | null; created_at: string; }
export interface AuditLog { id: number; tenant_id: number; actor_user_id: number | null; action: string; target_type: string | null; target_id: string | null; trace_id: string | null; detail_json: string; created_at: string; }
export interface TimelineEntry { ts: string; kind: string; turn_id: number; payload: any; }
export interface TraceLookup { turn: any; audit: AuditLog[]; }
export interface EvalCase { id: number; name: string; category: string | null; input: string; expected_status: string; expected_contains: string | null; }
export interface EvalRun { id: number; case_id: number; turn_id: number; passed: boolean; detail: any; created_at: string; }
export interface Kb { id: number; name: string; tenant_id: number; description: string | null; created_at: string; }
export interface KbDoc { id: number; title: string; source_uri: string | null; content_hash: string | null; tokens: number | null; created_at: string; }
export interface KbHit { id: number; title: string; source_uri: string | null; snippet: string; score: number; }
export interface Snapshot { id: number; thread_id: string; turn_id: number; content_digest: string | null; forked_to_thread_id: string | null; created_at: string; }
export interface Connector { id: number; name: string; kind: string; tier: string; status: string; quality_score: number; description: string | null; config_json: any; }
export interface ToolCall { id: number; connector_id: number | null; tool_name: string; success: boolean; result_ref: string | null; created_at: string; }
export interface Skill { id: number; name: string; description: string | null; status: string; active_version_id: number | null; }
export interface SkillVersion { id: number; skill_id: number; version: string; checksum: string | null; content_ref: string | null; created_at: string; }
export interface Orchestration { id: number; name: string | null; mode: string; status: string; prompt: string | null; created_at: string; completed_at: string | null; }
export interface AgentStep { id: number; agent_seq: number; role: string | null; thread_id: string; turn_id: number | null; status: string; output_ref: string | null; }

// M20: 版本化 Golden Set + 运行冻结快照
export interface GoldenSet { id: number; name: string; description: string | null; status: string; active_version_id: number | null; created_at: string; updated_at: string; }
export interface GoldenSetCase { id: number; golden_set_id: number; case_key: string; domain: string | null; difficulty: string | null; source: string | null; input_json: any; expected_json: any; created_at: string; }
export interface Rubric { id: number; name: string; status: string; active_version_id: number | null; created_at: string; updated_at: string; }
export interface EntityVersion { id: number; entity_type: string; entity_id: number; version_no: number; semver: string; manifest_hash: string; status: string; created_at: string; }
export interface EvalBatchRun { id: number; golden_set_id: number; golden_set_version_id: number; rubric_version_id: number | null; thread_id: string | null; snapshot_hash: string; trigger_type: string; status: string; aggregate: any; started_at: string; finished_at: string | null; }
export interface EvalCaseResult { id: number; run_id: number; case_id: number; case_key: string; turn_id: number | null; agent_output: string | null; scores: any; created_at: string; }
export interface CompareResult { incomparable: boolean; baseline_version_id: number; run_version_id: number; diffs: any[]; }

// M21: Judge LLM + 失败样本回流
export interface JudgeConfig { id: number; name: string; description: string | null; status: string; active_version_id: number | null; created_at: string; updated_at: string; }

// M22: 事件溯源 + Lineage + 电子签名 + CAS
export interface DomainEvent { id: number; tenant_id: number; entity_type: string; entity_id: number; event_type: string; event_seq: number; payload: any; occurred_at: string; }
export interface EsignRecord { id: number; tenant_id: number; entity_type: string; entity_id: number; version_id: number | null; signer_user_id: number; signature_purpose: string; signature_hash: string; meaning_text: string; signed_at: string; }
export interface LineageGraph { root: string; nodes: { id: string; kind: string; label: string; detail: any }[]; edges: { from: string; to: string; relation: string }[]; }

const TOKEN_KEY = "nexus.token";
export function getToken() { return localStorage.getItem(TOKEN_KEY); }
export function setToken(t: string) { localStorage.setItem(TOKEN_KEY, t); }
export function clearToken() { localStorage.removeItem(TOKEN_KEY); }
function authHeaders(): HeadersInit { const t = getToken(); return t ? { Authorization: `Bearer ${t}` } : {}; }
async function req<T>(path: string, init?: RequestInit): Promise<T> {
  const resp = await fetch(BASE + path, {
    ...init,
    headers: { "content-type": "application/json", ...authHeaders(), ...(init?.headers ?? {}) },
  });
  if (!resp.ok) { const text = await resp.text().catch(() => ""); throw new Error(`${resp.status} ${text}`); }
  if (resp.status === 204) return undefined as T;
  const ct = resp.headers.get("content-type") || "";
  return ct.includes("json") ? resp.json() : ((await resp.text()) as unknown as T);
}

export const api = {
  // Auth
  login: (email: string, password: string) => req<LoginResp>(`/auth/login`, { method: "POST", body: JSON.stringify({ email, password }) }),
  me: () => req<{ user_id: number; email: string; perms: string[]; tenant_id: number }>(`/auth/me`),

  // Threads / Turns / Items
  listThreads: () => req<Thread[]>(`/threads`),
  createThread: (title?: string) => req<{ id: string }>(`/threads`, { method: "POST", body: JSON.stringify(title ? { title } : {}) }),
  startTurn: (threadId: string, input: string) => req<{ turn_id: number }>(`/threads/${threadId}/turns`, { method: "POST", body: JSON.stringify({ input }) }),
  interruptTurn: (threadId: string, turnId: number) => req<{ status: string }>(`/threads/${threadId}/turns/${turnId}/interrupt`, { method: "POST" }),
  listItems: (threadId: string, since = 0) => req<Item[]>(`/threads/${threadId}/items?since=${since}`),

  // Approvals
  listApprovals: () => req<Approval[]>(`/approvals`),
  resolveApproval: (id: number, decision: "approve" | "deny" | "cancel" | "approve_with_amendment", amendment_command?: string[]) =>
    req<{ approval_id: number; status: string }>(`/approvals/${id}/resolve`, { method: "POST", body: JSON.stringify({ decision, amendment_command }) }),

  // Usage
  getUsage: (days = 7) => req<DailyUsage[]>(`/usage?days=${days}`),
  getUserUsage: (uid: number, days = 7) => req<DailyUsage[]>(`/usage/users/${uid}?days=${days}`),

  // Policy
  policyRules: () => req<PolicyRule[]>(`/policy/rules`),
  policyFeedback: (days = 7) => req<PolicyFeedback[]>(`/policy/feedback?days=${days}`),

  // Audit
  auditLogs: (params: { action?: string; since?: string; limit?: number } = {}) => {
    const q = new URLSearchParams();
    if (params.action) q.set("action", params.action);
    if (params.since) q.set("since", params.since);
    q.set("limit", String(params.limit ?? 50));
    return req<AuditLog[]>(`/audit/logs?${q}`);
  },
  auditLog: (id: number) => req<AuditLog>(`/audit/logs/${id}`),

  // Timeline & Trace
  timeline: (threadId: string) => req<TimelineEntry[]>(`/threads/${threadId}/timeline`),
  trace: (traceId: string) => req<TraceLookup>(`/traces/${traceId}`),

  // Evals
  evalCases: () => req<EvalCase[]>(`/evals/cases`),
  evalCreateCase: (c: { name: string; category?: string; input: string; expected_status: string; expected_contains?: string }) =>
    req<EvalCase>(`/evals/cases`, { method: "POST", body: JSON.stringify(c) }),
  evalRun: (caseId: number, turnId: number) => req<EvalRun>(`/evals/runs/${caseId}`, { method: "POST", body: JSON.stringify({ turn_id: turnId }) }),
  evalRuns: (limit = 20) => req<EvalRun[]>(`/evals/runs?limit=${limit}`),

  // Knowledge Base
  kbs: () => req<Kb[]>(`/kbs`),
  kbCreate: (name: string, description?: string) => req<Kb>(`/kbs`, { method: "POST", body: JSON.stringify({ name, description }) }),
  kbDocs: (id: number) => req<KbDoc[]>(`/kbs/${id}/documents`),
  kbIngest: (id: number, doc: { title: string; content: string; source_uri?: string }) =>
    req<KbDoc>(`/kbs/${id}/documents`, { method: "POST", body: JSON.stringify(doc) }),
  kbDeleteDoc: (kbId: number, did: number) => req<void>(`/kbs/${kbId}/documents/${did}`, { method: "DELETE" }),
  kbSearch: (id: number, query: string, keyword?: string, top_k = 5) =>
    req<KbHit[]>(`/kbs/${id}/search`, { method: "POST", body: JSON.stringify({ query, keyword, top_k }) }),

  // Snapshots
  snapshots: (threadId: string) => req<Snapshot[]>(`/threads/${threadId}/snapshots`),
  createSnapshot: (threadId: string, turnId?: number) => req<Snapshot>(`/threads/${threadId}/snapshots`, { method: "POST", body: JSON.stringify(turnId ? { turn_id: turnId } : {}) }),
  forkSnapshot: (threadId: string, sid: number) => req<{ new_thread_id: string }>(`/threads/${threadId}/snapshots/${sid}/fork`, { method: "POST" }),
  rollbackSnapshot: (threadId: string, sid: number) => req<{ deleted_items: number; deleted_turns: number }>(`/threads/${threadId}/snapshots/${sid}/rollback`, { method: "POST" }),

  // Runtime pool
  poolStatus: () => req<PoolStatus>(`/runtime/pool`),

  // Connectors
  connectors: () => req<Connector[]>(`/connectors`),
  connector: (id: number) => req<Connector>(`/connectors/${id}`),
  createConnector: (c: { name: string; kind: string; description?: string; tier?: string; config_json?: any }) =>
    req<Connector>(`/connectors`, { method: "POST", body: JSON.stringify(c) }),
  updateConnector: (id: number, c: Partial<{ name: string; description: string; tier: string; config_json: any }>) =>
    req<Connector>(`/connectors/${id}`, { method: "PUT", body: JSON.stringify(c) }),
  deleteConnector: (id: number) => req<void>(`/connectors/${id}`, { method: "DELETE" }),
  publishConnector: (id: number) => req<Connector>(`/connectors/${id}/publish`, { method: "POST" }),
  offlineConnector: (id: number) => req<Connector>(`/connectors/${id}/offline`, { method: "POST" }),
  connectorQuality: (id: number) => req<{ quality_score: number }>(`/connectors/${id}/quality`),
  invokeConnector: (id: number, body: { tool: string; args?: any }) =>
    req<{ call_id: number; mcp: boolean; success: boolean; result: string }>(`/connectors/${id}/invoke`, { method: "POST", body: JSON.stringify(body) }),
  connectorCalls: (id: number) => req<ToolCall[]>(`/connectors/${id}/calls`),

  // Skills
  skills: () => req<Skill[]>(`/skills`),
  skill: (id: number) => req<Skill>(`/skills/${id}`),
  createSkill: (s: { name: string; description?: string }) => req<Skill>(`/skills`, { method: "POST", body: JSON.stringify(s) }),
  deleteSkill: (id: number) => req<void>(`/skills/${id}`, { method: "DELETE" }),
  publishVersion: (id: number, v: { version: string; checksum?: string; content_ref?: string }) =>
    req<SkillVersion>(`/skills/${id}/versions`, { method: "POST", body: JSON.stringify(v) }),
  skillVersions: (id: number) => req<SkillVersion[]>(`/skills/${id}/versions`),
  rollbackSkill: (id: number, versionId: number) => req<Skill>(`/skills/${id}/rollback`, { method: "POST", body: JSON.stringify({ version_id: versionId }) }),

  // Orchestration
  orchestrations: () => req<Orchestration[]>(`/orchestrations`),
  orchestration: (id: number) => req<{ orchestration: Orchestration; agents: AgentStep[] }>(`/orchestrations/${id}`),
  startOrchestration: (body: { mode: string; prompt: string; agents?: number; name?: string }) =>
    req<{ orchestration_id: number; status: string }>(`/orchestrations`, { method: "POST", body: JSON.stringify(body) }),

  // M20: Golden Set + Rubric + 批量评测运行
  goldenSets: () => req<GoldenSet[]>(`/golden-sets`),
  goldenSet: (id: number) => req<{ golden_set: GoldenSet; cases: GoldenSetCase[] }>(`/golden-sets/${id}`),
  createGoldenSet: (name: string, description?: string) => req<GoldenSet>(`/golden-sets`, { method: "POST", body: JSON.stringify({ name, description }) }),
  addCase: (gsId: number, c: { case_key: string; domain?: string; difficulty?: string; source?: string; input_json: any; expected_json: any }) =>
    req<{ id: number }>(`/golden-sets/${gsId}/cases`, { method: "POST", body: JSON.stringify(c) }),
  listCases: (gsId: number) => req<GoldenSetCase[]>(`/golden-sets/${gsId}/cases`),
  publishGoldenSet: (id: number, bump_type?: string) => req<EntityVersion>(`/golden-sets/${id}/publish`, { method: "POST", body: JSON.stringify({ bump_type }) }),
  rubrics: () => req<Rubric[]>(`/rubrics`),
  createRubric: (name: string) => req<Rubric>(`/rubrics`, { method: "POST", body: JSON.stringify({ name }) }),
  publishRubric: (id: number, bump_type: string, criteria: any) => req<EntityVersion>(`/rubrics/${id}/publish`, { method: "POST", body: JSON.stringify({ bump_type, criteria }) }),
  batchRuns: (limit = 20) => req<EvalBatchRun[]>(`/evals/batch-runs?limit=${limit}`),
  batchRun: (id: number) => req<{ run: EvalBatchRun; case_results: EvalCaseResult[] }>(`/evals/batch-runs/${id}`),
  startBatchRun: (body: { golden_set_id: number; rubric_id?: number; thread_id: string }) =>
    req<{ run_id: number }>(`/evals/batch-runs`, { method: "POST", body: JSON.stringify(body) }),
  compareBatchRun: (id: number, baseline_run_id: number) => req<CompareResult>(`/evals/batch-runs/${id}/compare`, { method: "POST", body: JSON.stringify({ baseline_run_id }) }),

  // M21: Judge LLM + 失败样本回流
  judgeConfigs: () => req<JudgeConfig[]>(`/judge-configs`),
  createJudgeConfig: (name: string, description?: string) => req<JudgeConfig>(`/judge-configs`, { method: "POST", body: JSON.stringify({ name, description }) }),
  addJudgeDimension: (id: number, dimension: any) => req<{ ok: boolean }>(`/judge-configs/${id}/dimensions`, { method: "POST", body: JSON.stringify(dimension) }),
  publishJudgeConfig: (id: number, bump_type: string, config: any) => req<EntityVersion>(`/judge-configs/${id}/publish`, { method: "POST", body: JSON.stringify({ bump_type, config }) }),
  caseFromTurn: (gsId: number, body: { turn_id: number; case_key: string; expected_json: any; user_query?: string }) =>
    req<{ case_id: number }>(`/golden-sets/${gsId}/cases/from-turn`, { method: "POST", body: JSON.stringify(body) }),

  // M22: 事件溯源 + Lineage + 电子签名 + CAS
  events: (entityType: string, entityId: number, asOf?: string) =>
    req<{ entity_type: string; entity_id: number; as_of: string | null; events: DomainEvent[]; replayed_state: any }>(
      `/events/${entityType}/${entityId}${asOf ? `?as_of=${encodeURIComponent(asOf)}&replay=true` : ""}`),
  lineage: (entityType: string, entityId: number) =>
    req<LineageGraph>(`/lineage/${entityType}/${entityId}`),
  esign: (body: { entity_type: string; entity_id: number; version_id?: number; signature_purpose: string; meaning_text: string }) =>
    req<{ esign_id: number }>(`/esign`, { method: "POST", body: JSON.stringify(body) }),
  getEsign: (id: number) => req<EsignRecord>(`/esign/${id}`),
  verifyEsign: (id: number) => req<{ valid: boolean; hash_match: boolean; elements_complete: boolean }>(`/esign/${id}/verify`),
  esigns: (entityType: string, entityId: number) =>
    req<EsignRecord[]>(`/esigns?entity_type=${entityType}&entity_id=${entityId}`),
  storeContent: (content: string, contentType?: string) =>
    req<{ content_hash: string; ref: string }>(`/content`, { method: "POST", body: JSON.stringify({ content, content_type: contentType || "text/plain" }) }),
};

export function openThreadStream(threadId: string, onItem: (f: any) => void, onRevoke?: () => void): WebSocket {
  const proto = location.protocol === "https:" ? "wss" : "ws";
  const token = getToken() ?? "";
  const ws = new WebSocket(`${proto}://${location.host}/v1/ws/threads/${threadId}/events?token=${encodeURIComponent(token)}`);
  ws.onmessage = (ev) => { try { const f = JSON.parse(ev.data); if (f.event === "revoked") onRevoke?.(); else onItem(f); } catch { /* ignore */ } };
  return ws;
}
