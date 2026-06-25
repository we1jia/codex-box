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

export interface ConfigSnapshotView {
  configPath: string;
  activeProfile: string | null;
  profiles: ProfileView[];
  providers: ProviderView[];
}

export type ConfigChangeRequest =
  | {
      type: "add_provider";
      id: string;
      kind: ProviderView["kind"];
      baseUrl: string;
      wireApi: ProviderView["wireApi"];
      envKey: string;
      models: string[];
    }
  | {
      type: "add_profile";
      name: string;
      model: string;
      providerId: string;
      sandbox: string;
      approval: string;
      network: string;
      mcpRefs: string[];
    }
  | {
      type: "set_active_profile";
      profileName: string;
    };

export interface ConfigDiffLineView {
  kind: "context" | "insert" | "delete";
  content: string;
  oldLine: number | null;
  newLine: number | null;
}

export interface ConfigChangePreviewView {
  configPath: string;
  expectedHash: string;
  diff: ConfigDiffLineView[];
  insertions: number;
  deletions: number;
  requiresConfirmation: boolean;
}

export interface ApplyConfigChangeResultView {
  configPath: string;
  backup: {
    id: string;
    created_at: string;
    file_path: string;
    reason: string;
    content_hash: string;
    size_bytes: number;
  };
  newHash: string;
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

export interface OpenCodexStatus {
  sourcePath: string;
  exists: boolean;
  packageJsonPath: string;
  configYamlPath: string;
  configExists: boolean;
  authPasswordConfigured: boolean;
  running: boolean;
  managed: boolean;
  pid: number | null;
  host: string;
  port: number;
  localUrl: string;
  lanUrls: string[];
  mobileUrl: string | null;
  lanAccessEnabled: boolean;
  mobileUrlReachable: boolean;
  codexHome: string;
  sharedCodexHome: string;
  runtimeDir: string;
  logPath: string;
  healthEndpoint: string;
  healthOk: boolean;
  healthStatus: number | null;
  lastError: string | null;
  lanRequiresPassword: boolean;
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
