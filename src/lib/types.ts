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

// BYOK · ProviderRoute · ~/.opencodex/providers.json 条目
export interface ProviderRoute {
  name: string;
  baseUrl: string;
  wireApi: string;
  apiKeyRef: string | null;
  httpHeaders: Record<string, string>;
  enabled: boolean;
  note: string | null;
}

// BYOK · ModelCatalogEntry · ~/.opencodex/custom_model_catalog.json 条目
export interface ModelCatalogEntry {
  modelId: string;
  displayName: string | null;
  provider: string;
  backendModel?: string | null;
  backendProvider?: string | null;
  visible: boolean;
  reasoning: ReasoningConfig | null;
  note: string | null;
}

export interface ReasoningConfig {
  enabled: boolean;
  levels: string[];
}

// BYOK · ~/.opencodex/ 完整快照
export interface OpenCodexCustomConfig {
  schemaVersion: number;
  providersPath: string;
  catalogPath: string;
  providers: ProviderRoute[];
  catalog: ModelCatalogEntry[];
  rawProvidersText: string;
  rawCatalogText: string;
  providersContentHash: string;
  catalogContentHash: string;
  readAt: string;
  valid: boolean;
  parseErrors: OpenCodexParseError[];
}

export interface OpenCodexParseError {
  file: string;
  message: string;
}

export interface OpenCodexWriteRequest<T> {
  entry: T;
  expectedHash: string;
  note: string | null;
}

export interface OpenCodexDeleteRequest {
  key: string;
  expectedHash: string;
  note: string | null;
}

export interface OpenCodexWriteResult {
  filePath: string;
  backupId: string;
  newHash: string;
}

// Codex Runtime 检测(只读)
export interface CodexRuntimeStatus {
  codexHome: string;
  codexCliPath: string | null;
  codexDesktopAppPath: string | null;
  codexDesktopVersion: string | null;
  desktopInstalled: boolean;
  cliAvailable: boolean;
  configReadable: boolean;
  authStateDetected: boolean;
  opencodexDir: string;
  opencodexDirExists: boolean;
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

// =====================================================================
// Codex Box 本地代理 runtime (BYOK 真链路)
// =====================================================================

export type ProxyStatusName = "stopped" | "starting" | "running" | "failed";

export interface ProxyRouteEntry {
  name: string;
  originalBaseUrl: string;
  envKey: string | null;
  wireApi: string;
  kind: string;
  models: string[];
}

export interface ProxyStatusView {
  status: ProxyStatusName;
  port: number;
  startedAt: string;
  uptimeMs: number | null;
  lastError: string | null;
  providerCount: number;
  providers: ProxyRouteEntry[];
}

export interface ProxyModelsPreview {
  baseUrl: string;
  rawJson: unknown;
}

export interface InjectBaseUrlPreview {
  newConfigText: string;
  newHash: string;
  diff: ConfigDiffLineView[];
  insertions: number;
  deletions: number;
  injectMap: {
    updatedAt: string;
    port: number;
    providers: ProxyRouteEntry[];
  };
  injectMapHash: string;
  backupId: string;
}

export interface ApplyInjectResult {
  newConfigHash: string;
  injectMapWrite: OpenCodexWriteResult;
  backup: {
    id: string;
    created_at: string;
    file_path: string;
    reason: string;
    content_hash: string;
    size_bytes: number;
  };
}

export interface RestoreBaseUrlPreview {
  newConfigText: string;
  newHash: string;
  diff: ConfigDiffLineView[];
  insertions: number;
  deletions: number;
  restoredCount: number;
}

export interface ApplyRestoreResult {
  newConfigHash: string;
  backup: {
    id: string;
    created_at: string;
    file_path: string;
    reason: string;
    content_hash: string;
    size_bytes: number;
  };
  injectMap: {
    updatedAt: string;
    port: number;
    providers: ProxyRouteEntry[];
  };
  clearedInjectMapHash: string;
}
