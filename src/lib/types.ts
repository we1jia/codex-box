export interface McpCount {
  enabled: number;
  total: number;
}

export interface HealthSummary {
  ok: number;
  warn: number;
  fail: number;
}

export interface DashboardSummary {
  active_profile: string | null;
  provider_count: number;
  mcp_count: McpCount;
  network: string;
  last_backup_at: string | null;
  health_summary: HealthSummary;
}

export type StatusTone = "ok" | "warn" | "fail" | "idle" | "running";

export interface ProfileView {
  id: string;
  name: string;
  model: string;
  providerId: string;
  sandbox: string;
  approval: string;
  network: string;
  mcpRefs: string[];
  status: StatusTone;
  isActive?: boolean;
}

export interface ProviderView {
  id: string;
  name: string;
  kind: "subscription" | "official_api" | "compatible_api" | "local_gateway";
  baseUrl: string;
  wireApi: "chat" | "responses" | "sse_stream" | "custom";
  envKey: string;
  status: StatusTone;
  models: string[];
}

export interface GatewayPresetView {
  id: string;
  name: string;
  kind: "local" | "opencodex" | "codex_proxy" | "cli_proxy_api";
  host: string;
  port: number;
  status: StatusTone;
  logPath: string;
  healthPath: string;
  adapter: string;
}

export interface McpServerView {
  id: string;
  name: string;
  transport: "stdio" | "http";
  commandOrUrl: string;
  enabled: boolean;
  status: StatusTone;
  envRefs: string[];
}

export interface NetworkRouteView {
  id: string;
  name: string;
  kind: "direct" | "http_proxy" | "socks_proxy" | "clash_profile";
  target: string;
  status: StatusTone;
  latencyMs: number | null;
}

export interface DiffLineView {
  id: string;
  kind: "insert" | "delete" | "change" | "context";
  content: string;
}

export interface DiagnosticGroupView {
  id: string;
  titleKey: string;
  items: Array<{
    id: string;
    labelKey: string;
    detail: string;
    status: StatusTone;
    latencyMs?: number;
  }>;
}

export interface SettingsSectionView {
  id: string;
  titleKey: string;
  optionKeys: string[];
}
