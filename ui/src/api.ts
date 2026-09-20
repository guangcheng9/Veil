export type RuleView = {
  id: string;
  type_prefix: string;
  enabled: boolean;
  priority: number;
  kind: string;
  source: string;
  pattern?: string | null;
  words?: string[] | null;
  description?: string | null;
};

export type TrafficEvent = {
  ts: number;
  method: string;
  path_template: string;
  status: number;
  streaming: boolean;
  hit_types: string[];
  hit_counts: number[];
  latency_ms: number;
  error_class: string | null;
  creator_prefix8: string;
};

export type ClientInfo = {
  id: string;
  label: string;
  kind: string;
  detected: boolean;
  wired: boolean;
  port: number | null;
  upstream: string | null;
  app_base_url: string | null;
  veil_url: string | null;
};

export type ClientRoute = {
  id: string;
  label: string;
  kind: string;
  port: number;
  upstream: string;
};

export type StatusBody = {
  product: string;
  bind: string;
  proxy_port: number;
  management_port: number;
  anthropic_upstream: string;
  openai_completions_upstream: string;
  openai_responses_upstream: string;
  upstream_proxy: string;
  revision: number;
  mapping_ttl_days: number;
  request_body_limit_mib: number;
  graceful_shutdown_timeout_secs: number;
  master_key_set: boolean;
  rule_count: number;
  last_error_class: string | null;
  version?: string;
  instance_id: string;
  pid: number;
  started_at: string;
  control_plane: "running" | "stopping";
  proxy_state: ProxyState;
  listeners: number[];
  last_lifecycle_error: string | null;
};

export type ProxyState = "starting" | "running" | "stopping" | "stopped" | "degraded";

export type LifecycleStatus = {
  instance_id: string;
  pid: number;
  started_at: string;
  control_plane: "running" | "stopping";
  proxy_state: ProxyState;
  listeners: number[];
  last_lifecycle_error: string | null;
  pending_config_change: boolean;
};

let adminToken = sessionStorage.getItem("veil_admin") ?? "";

export function setAdminToken(token: string) {
  adminToken = token;
  if (token) sessionStorage.setItem("veil_admin", token);
  else sessionStorage.removeItem("veil_admin");
}

export type UpdateErrorKind = "network" | "rateLimit" | "noRelease" | "unknown";

// 把后端 /api/update 失败时返回的 "502: <detail>" 翻译成稳定的错误种类，
// 供 Dashboard 展示对应的本地化文案。detail 里带的是 reqwest 的原文错误，
// 足够区分网络不通 / GitHub 限流 / 解析异常三类。
export function classifyUpdateError(raw: string): UpdateErrorKind {
  const m = raw.match(/^\s*(\d+):\s*(.*)$/s);
  const status = m ? m[1] : "";
  const detail = (m ? m[2] : raw).toLowerCase();
  if (status !== "502" && status !== "504" && status !== "503") {
    return "unknown";
  }
  if (/rate limit|secondary rate|too many requests|-> 403|status: 403|forbidden/.test(detail)) {
    return "rateLimit";
  }
  if (/no release tag|release has no|parse|json|invalid|eof/.test(detail)) {
    return "noRelease";
  }
  if (/timed out|timeout|connect|dns|error sending request|connection|pool|reset|broken pipe|gateway/.test(detail)) {
    return "network";
  }
  return "unknown";
}

async function req<T>(path: string, init: RequestInit = {}): Promise<T> {
  const headers = new Headers(init.headers);
  if (adminToken) headers.set("x-veil-admin-token", adminToken);
  if (init.body && !headers.has("content-type")) headers.set("content-type", "application/json");
  const res = await fetch(path, { ...init, headers });
  if (!res.ok) {
    const detail = await res.text().catch(() => "");
    throw new Error(detail ? `${res.status}: ${detail}` : String(res.status));
  }
  if (res.status === 204) return undefined as T;
  const ct = res.headers.get("content-type") ?? "";
  if (ct.includes("json")) return res.json() as Promise<T>;
  return res.text() as unknown as T;
}

export const api = {
  status: () => req<StatusBody>("/api/status"),
  lifecycle: () => req<LifecycleStatus>("/api/lifecycle"),
  startProxy: () => req<LifecycleStatus>("/api/lifecycle/start", { method: "POST" }),
  stopProxy: () => req<LifecycleStatus>("/api/lifecycle/stop", { method: "POST" }),
  restartProxy: () => req<LifecycleStatus>("/api/lifecycle/restart", { method: "POST" }),
  exitVeil: () => req<{ exiting: boolean }>("/api/lifecycle/exit", { method: "POST" }),
  rules: () => req<{ rules: RuleView[]; revision: number }>("/api/rules"),
  putRules: (rules: RuleView[], revision: number) =>
    req<void>("/api/rules", { method: "PUT", body: JSON.stringify({ rules, revision }) }),
  allowlist: () => req<{ allowlist: string[]; revision: number }>("/api/allowlist"),
  putAllowlist: (list: string[], revision: number) =>
    req<void>("/api/allowlist", { method: "PUT", body: JSON.stringify({ allowlist: list, revision }) }),
  upstream: () => req<{ anthropic_upstream: string; openai_completions_upstream: string; openai_responses_upstream: string; upstream_proxy: string; revision: number; routes?: ClientRoute[] }>("/api/upstream"),
  putUpstream: (u: { anthropic_upstream: string; openai_completions_upstream: string; openai_responses_upstream: string; upstream_proxy: string; revision: number; routes?: ClientRoute[] }) =>
    req<void>("/api/upstream", { method: "PUT", body: JSON.stringify(u) }),
  clients: () => req<ClientInfo[]>("/api/clients"),
  setupClient: (id: string) => req<{ id: string; path: string; port: number; upstream: string; restart_veil?: boolean }>("/api/clients/" + id + "/setup", { method: "POST" }),
  putSettings: (s: { mapping_ttl_days?: number; request_body_limit_mib?: number; graceful_shutdown_timeout_secs?: number }) =>
    req<void>("/api/settings", { method: "PUT", body: JSON.stringify(s) }),
  rotateKey: (new_key: string) => req<{ rotated: boolean }>("/api/master-key", { method: "POST", body: JSON.stringify({ new_key }) }),
  purge: (creator_prefix?: string) => req<{ purged: boolean }>("/api/mappings/purge", { method: "POST", body: JSON.stringify({ creator_prefix }) }),
  resetToken: () => req<{ token: string }>("/api/admin-token/reset", { method: "POST" }),
  checkUpdate: () => req<{ current: string; latest: string; newer: boolean; asset: string; url: string }>("/api/update"),
  applyUpdate: () => req<{ restarting?: boolean; latest?: string; current?: string; newer?: boolean }>("/api/update", { method: "POST" }),
  traffic: async () => {
    const text = await req<string>("/api/traffic");
    const line = String(text).split("\n").find((l) => l.startsWith("data: "));
    if (!line) return [] as TrafficEvent[];
    try { return JSON.parse(line.slice(6)) as TrafficEvent[]; } catch { return []; }
  },
};
