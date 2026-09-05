// Nexus M1 API client + WS helper.

export interface Thread {
  id: string;
  title: string | null;
  status: string;
  created_at: string;
}

export interface Item {
  id: number;
  turn_id: number;
  seq: number;
  item_type: string;
  content_ref: string | null;
  created_at: string;
}

export interface Approval {
  id: number;
  thread_id: string;
  turn_id: number;
  kind: string | null;
  status: string;
  command: string | null;
  cwd: string | null;
  reason: string | null;
  policy_decision: string | null;
  risk_level: string | null;
  created_at: string;
}

export interface DailyUsage {
  date: string;
  total_input_tokens: number;
  total_output_tokens: number;
  total_cost_micros: number;
  total_turns: number;
}

export interface LoginResp {
  token: string;
  user_id: number;
  perms: string[];
}

const TOKEN_KEY = "nexus.token";

export function getToken(): string | null {
  return localStorage.getItem(TOKEN_KEY);
}
export function setToken(t: string) {
  localStorage.setItem(TOKEN_KEY, t);
}

function authHeaders(): HeadersInit {
  const t = getToken();
  return t ? { Authorization: `Bearer ${t}` } : {};
}

async function req<T>(path: string, init?: RequestInit): Promise<T> {
  const resp = await fetch(path, {
    ...init,
    headers: { "content-type": "application/json", ...authHeaders(), ...(init?.headers ?? {}) },
  });
  if (!resp.ok) {
    const text = await resp.text().catch(() => "");
    throw new Error(`${resp.status} ${text}`);
  }
  return resp.json() as Promise<T>;
}

export const api = {
  login: (email: string, password: string) =>
    req<LoginResp>("/v1/auth/login", {
      method: "POST",
      body: JSON.stringify({ email, password }),
    }),
  me: () => req<unknown>("/v1/auth/me"),
  listThreads: () => req<Thread[]>("/v1/threads"),
  createThread: (title: string) =>
    req<{ id: string }>("/v1/threads", { method: "POST", body: JSON.stringify({ title }) }),
  startTurn: (threadId: string, input: string) =>
    req<{ turn_id: number }>(`/v1/threads/${threadId}/turns`, {
      method: "POST",
      body: JSON.stringify({ input }),
    }),
  listItems: (threadId: string, since = 0) =>
    req<Item[]>(`/v1/threads/${threadId}/items?since=${since}`),
  listApprovals: () => req<Approval[]>("/v1/approvals"),
  resolveApproval: (id: number, decision: "approve" | "deny" | "cancel") =>
    req<{ approval_id: number; status: string }>(
      `/v1/approvals/${id}/resolve`,
      { method: "POST", body: JSON.stringify({ decision }) }
    ),
  getUsage: (days = 7) => req<DailyUsage[]>(`/v1/usage?days=${days}`),
};

// Open a WS subscription to a thread's event stream.
export function openThreadStream(
  threadId: string,
  onItem: (frame: { seq: number; type: string; content: string | null }) => void,
  onRevoke?: () => void
): WebSocket {
  const proto = location.protocol === "https:" ? "wss" : "ws";
  const token = getToken() ?? "";
  const ws = new WebSocket(
    `${proto}://${location.host}/v1/ws/threads/${threadId}/events?token=${encodeURIComponent(token)}`
  );
  ws.onmessage = (ev) => {
    try {
      const f = JSON.parse(ev.data);
      if (f.event === "revoked") onRevoke?.();
      else onItem(f);
    } catch {
      /* ignore */
    }
  };
  return ws;
}
