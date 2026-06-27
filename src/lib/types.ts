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

// BYOK · ProviderRoute · ~/.codex/codex-box/providers.json 条目
export interface ProviderRoute {
  name: string;
  baseUrl: string;
  wireApi: string;
  apiKeyRef: string | null;
  httpHeaders: Record<string, string>;
  enabled: boolean;
  note: string | null;
  codexRouting?: CodexRoutingConfig | null;
  codexChatReasoning?: CodexChatReasoningConfig | null;
}

export type CodexApiFormat = "openai_responses" | "openai_chat" | "openai_messages";

export interface CodexRoutingConfig {
  enabled?: boolean;
  defaultRouteId?: string;
  routes?: CodexRoutingRoute[];
}

export interface CodexRoutingRoute {
  id: string;
  label?: string;
  enabled?: boolean;
  targetProviderId?: string;
  match?: {
    models?: string[];
    prefixes?: string[];
  };
  upstream?: {
    baseUrl?: string;
    apiFormat?: CodexApiFormat;
    auth?: {
      source?: "provider_config" | "managed_account" | "managed_codex_oauth";
      authProvider?: "codex_oauth";
      accountId?: string;
    };
    apiKey?: string;
    modelMap?: Record<string, string>;
    codexChatReasoning?: CodexChatReasoningConfig;
  };
  codexChatReasoning?: CodexChatReasoningConfig;
  capabilities?: {
    inputModalities?: Array<"text" | "image">;
    textOnly?: boolean;
    supportsReasoning?: boolean;
    codexChatReasoning?: CodexChatReasoningConfig;
  };
}

// BYOK · ModelCatalogEntry · ~/.codex/codex-box/custom_model_catalog.json 条目
export interface ModelCatalogEntry {
  modelId: string;
  displayName: string | null;
  provider: string;
  backendModel?: string | null;
  backendProvider?: string | null;
  targetProvider?: string | null;
  target_provider?: string | null;
  visible: boolean;
  reasoning: ReasoningConfig | null;
  note: string | null;
  visionBridgeEnabled?: boolean | null;
  visionFallbackBaseUrl?: string | null;
  visionFallbackModel?: string | null;
  visionFallbackApiKeyRef?: string | null;
  codexChatReasoning?: CodexChatReasoningConfig | null;
}

export interface ReasoningConfig {
  enabled: boolean;
  levels: string[];
}

export type CodexChatThinkingParam =
  | "none"
  | "thinking"
  | "enable_thinking"
  | "reasoning_split";

export type CodexChatEffortParam =
  | "none"
  | "reasoning_effort"
  | "reasoning.effort";

export type CodexChatEffortValueMode =
  | "passthrough"
  | "low_high"
  | "deepseek"
  | "openrouter";

export type CodexChatReasoningOutputFormat =
  | "auto"
  | "reasoning_content"
  | "reasoning"
  | "reasoning_details"
  | "think_tags";

export interface CodexChatReasoningConfig {
  supportsThinking?: boolean;
  supportsEffort?: boolean;
  thinkingParam?: CodexChatThinkingParam;
  effortParam?: CodexChatEffortParam;
  effortValueMode?: CodexChatEffortValueMode;
  minOutputTokens?: number;
  outputFormat?: CodexChatReasoningOutputFormat;
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

export interface SimpleModelConfigRequest {
  modelInput: string;
  baseUrl: string;
  apiKey: string;
  wireApi: "chat" | "responses" | "sse_stream" | "custom";
  displayName: string | null;
  reasoningLevel: string | null;
  restartCodex: boolean;
}

export interface SimpleModelConfigResult {
  provider: ProviderRoute;
  model: ModelCatalogEntry;
  envKey: string;
  providerWrite: OpenCodexWriteResult;
  catalogWrite: OpenCodexWriteResult;
  requiresMultirouterSync: boolean;
  restartCodex: boolean;
}

export interface CodexMultirouterSyncRequest {
  providersExpectedHash: string;
  catalogExpectedHash: string;
  proxyPort?: number | null;
  routerProviderId?: string | null;
  ensureCodexConfig?: boolean | null;
}

export interface CodexMultirouterSyncResult {
  routerProvider: ProviderRoute;
  routeCount: number;
  routedModelCount: number;
  skippedModels: string[];
  proxyBaseUrl: string;
  providerWrite: OpenCodexWriteResult;
  catalogWrite: OpenCodexWriteResult;
  configWrite?: OpenCodexWriteResult | null;
  modelsCacheWrite?: OpenCodexWriteResult | null;
  injectMapWrite?: OpenCodexWriteResult | null;
  configTouched: boolean;
  modelsCacheTouched: boolean;
  injectMapTouched: boolean;
}

export interface CodexMultirouterPreviewRequest {
  proxyPort?: number | null;
  routerProviderId?: string | null;
  ensureCodexConfig?: boolean | null;
}

export interface CodexMultirouterPreview {
  providersPath: string;
  catalogPath: string;
  configPath: string;
  modelsCachePath: string;
  injectMapPath: string;
  providersExpectedHash: string;
  catalogExpectedHash: string;
  configExpectedHash: string;
  modelsCacheExpectedHash: string;
  injectMapExpectedHash: string;
  routerProviderId: string;
  proxyPort: number;
  providersDiff: ConfigDiffLineView[];
  catalogDiff: ConfigDiffLineView[];
  configDiff: ConfigDiffLineView[];
  modelsCacheDiff: ConfigDiffLineView[];
  injectMapDiff: ConfigDiffLineView[];
  routerProvider: ProviderRoute;
  routeCount: number;
  routedModelCount: number;
  skippedModels: string[];
  proxyBaseUrl: string;
  ensureCodexConfig: boolean;
  modelsCacheTouched: boolean;
  injectMapTouched: boolean;
}

export interface CodexModelsCacheRestorePreview {
  modelsCachePath: string;
  backupPath: string;
  modelsCacheExpectedHash: string;
  backupExists: boolean;
  ownedCache: boolean;
  restoreAvailable: boolean;
  willDelete: boolean;
  diff: ConfigDiffLineView[];
}

export interface CodexModelsCacheRestoreResult {
  modelsCachePath: string;
  backupPath: string;
  backupId: string;
  newHash: string;
  restored: boolean;
  deleted: boolean;
}

export interface ConfigImportSource {
  id: string;
  displayName: string;
  sourceKind: string;
  path: string;
  providers: number;
  models: number;
  configSnapshots: number;
  warnings: string[];
  recommendedAction: string;
  canImport: boolean;
}

export interface ConfigImportPreview {
  sourceId: string;
  providersSourcePath: string;
  catalogSourcePath: string;
  providersTargetPath: string;
  catalogTargetPath: string;
  providersExpectedHash: string;
  catalogExpectedHash: string;
  providersDiff: ConfigDiffLineView[];
  catalogDiff: ConfigDiffLineView[];
  providers: number;
  models: number;
  warnings: string[];
}

export interface ApplyConfigImportResult {
  providerWrite: OpenCodexWriteResult;
  catalogWrite: OpenCodexWriteResult;
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

export interface CodexDesktopIntegrationStatus {
  configPath: string;
  configParsed: boolean;
  configError: string | null;
  model: string | null;
  modelProvider: string | null;
  modelCatalogJson: string | null;
  customModelCatalogPath: string;
  customModelCatalogExists: boolean;
  customCatalogNativeOpenaiModelCount: number;
  customCatalogByokModelCount: number;
  officialRouteConfigured: boolean;
  officialRouteModelCount: number;
  officialRouteAuthSource: string | null;
  officialRouteBaseUrl: string | null;
  routerProviderBaseUrl: string | null;
  routerProviderRequiresOpenaiAuth: boolean | null;
  routerProviderSupportsWebsockets: boolean | null;
  routerProviderUsesProxyManagedBearer: boolean | null;
  routerProviderModelsCount: number | null;
  modelsCachePath: string;
  modelsCacheExists: boolean;
  modelsCacheOwnedByCodexBox: boolean;
  modelsCacheModelCount: number | null;
  modelsCacheClientVersionPresent: boolean;
  authPath: string;
  authJsonExists: boolean;
  authMode: string | null;
  chatgptAuthLikely: boolean;
  openaiApiKeyPresentInAuth: boolean;
  codexRunning: boolean;
  codexRemoteDebuggingPort: number | null;
  codexProcesses: CodexProcessView[];
  pickerReadinessStatus: string;
  pickerReadinessSummary: string;
  pickerReadinessBlockers: string[];
  pickerReadinessWarnings: string[];
  issues: CodexDesktopIntegrationIssue[];
}

export interface CodexPickerUnlockResult {
  attemptedPorts: number[];
  debugPort: number | null;
  targetCount: number;
  injectedTargetCount: number;
  rendererReports: PickerRendererReport[];
  modelCount: number;
  modelNames: string[];
  injected: boolean;
  launched: boolean;
  codexExecutable: string | null;
  status: string;
  message: string;
  errors: string[];
}

export interface PickerRendererReport {
  port: number;
  targetId: string;
  status: string;
  patchKey: string | null;
  modelCount: number | null;
  availableModels: string[];
  errorCount: number | null;
}

export interface CodexProcessView {
  pid: number | null;
  command: string;
  remoteDebuggingPort: number | null;
}

export interface CodexDesktopIntegrationIssue {
  severity: "ok" | "warn" | "fail" | string;
  code: string;
  message: string;
}

export interface CodexHistoryReconcileView {
  codexHome: string;
  configPath: string;
  liveConfigModelProvider: string | null;
  suggestedTargetProvider: string;
  sourceProviderIds: string[];
  activeStateDbPath: string | null;
  activeStateDbKind: string | null;
  providersFound: string[];
  sqliteStores: CodexHistoryStoreSummary[];
  jsonlSummary: CodexHistoryJsonlSummary;
  sessionIndexPath: string;
  sessionIndexExists: boolean;
  globalStatePath: string;
  globalStateExists: boolean;
  driftDetected: boolean;
  providerRowsToUpdate: number;
  rolloutProviderLinesToUpdate: number;
  warnings: CodexHistoryWarning[];
}

export interface CodexHistoryStoreSummary {
  path: string;
  kind: string;
  total: number;
  providerCounts: Record<string, number>;
  readable: boolean;
  error: string | null;
}

export interface CodexHistoryJsonlSummary {
  roots: string[];
  totalFiles: number;
  providerCounts: Record<string, number>;
  unreadableFiles: number;
}

export interface CodexHistoryWarning {
  severity: "ok" | "warn" | "fail" | string;
  code: string;
  message: string;
}

export interface CodexHistoryUnifyRequest {
  targetProvider?: string | null;
  sourceProviderIds?: string[] | null;
  projectPath?: string | null;
  force?: boolean | null;
}

export interface CodexHistoryUnifyPreview {
  codexHome: string;
  targetProvider: string;
  sourceProviderIds: string[];
  activeStateDbPath: string | null;
  activeStateDbKind: string | null;
  providerRowsToUpdate: number;
  rolloutFilesToUpdate: number;
  rolloutProviderLinesToUpdate: number;
  userEventRowsToUpdate: number;
  visibleCandidateRows: number;
  sessionIndexMissingToAppend: number;
  focusRowsToMove: number;
  workspaceHintsToFix: number;
  projectlessIdsToRemove: number;
  savedWorkspaceRootsToAdd: number;
  sessionIndexPath: string;
  sessionIndexExists: boolean;
  globalStatePath: string;
  globalStateExists: boolean;
  backupDir: string;
  codexRunning: boolean;
  codexProcesses: string[];
  canApply: boolean;
  warnings: CodexHistoryWarning[];
}

export interface CodexHistoryBackupSummary {
  backupDir: string;
  files: string[];
  rolloutManifestPath: string;
}

export interface CodexHistoryUnifyApplyResult {
  preview: CodexHistoryUnifyPreview;
  backup: CodexHistoryBackupSummary;
  providerRowsUpdated: number;
  rolloutFilesUpdated: number;
  rolloutProviderLinesUpdated: number;
  userEventRowsUpdated: number;
  focusRowsUpdated: number;
  sessionIndexAppended: number;
  sessionIndexRowsMoved: number;
  sessionIndexTitlesUpdated: number;
  workspaceHintsFixed: number;
  projectlessIdsRemoved: number;
  savedWorkspaceRootsAdded: number;
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

export interface ProxyRouteTestStep {
  id: string;
  label: string;
  status: "passed" | "failed" | "warning" | "skipped" | string;
  detail: string;
}

export interface ProxyRouteTestResult {
  status: "passed" | "failed" | string;
  modelId: string;
  providerName: string | null;
  upstreamModel: string | null;
  upstreamBaseUrl: string | null;
  wireApi: string | null;
  authSource: string | null;
  textOnly: boolean;
  usedChatFallback: boolean;
  imagePartSentToChat: boolean;
  upstreamStatusCode: number | null;
  upstreamLatencyMs: number | null;
  chatRequestPreview: unknown | null;
  steps: ProxyRouteTestStep[];
  warnings: string[];
}

export interface ProxyRuntimeLogEntry {
  at: string;
  level: "info" | "warn" | "error" | string;
  scope: string;
  message: string;
}

export interface ProxyRuntimeLogs {
  redacted: boolean;
  items: ProxyRuntimeLogEntry[];
}

export interface ProxySessionEntry {
  id: string;
  label: string;
  status: "active" | "idle" | string;
  providerCount: number;
  modelCount: number;
  lastUsedAt: string;
}

export interface ProxySessionsView {
  activeSessionId: string | null;
  sessions: ProxySessionEntry[];
}

export interface EffectiveRoutingIssue {
  severity: "info" | "warn" | "fail" | string;
  code: string;
  message: string;
}

export interface EffectiveRoutingStatus {
  configPath: string;
  currentModel: string | null;
  modelProvider: string;
  requestBaseUrl: string | null;
  requestBaseUrlSource: string;
  modelCatalogPath: string | null;
  catalogConfigured: boolean;
  catalogModelFound: boolean;
  catalogProvider: string | null;
  backendProvider: string | null;
  backendModel: string | null;
  upstreamBaseUrl: string | null;
  proxyRunning: boolean;
  proxyPort: number | null;
  issues: EffectiveRoutingIssue[];
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

export interface ByokActivationPreview {
  newConfigText: string;
  expectedHash: string;
  diff: ConfigDiffLineView[];
  insertions: number;
  deletions: number;
  modelId: string;
  backendProvider: string;
  backendModel: string;
  proxyBaseUrl: string;
  modelCatalogPath: string;
  conversationProviderId: string;
  reasoningEffort: string | null;
}

export interface ApplyByokActivationResult {
  newConfigHash: string;
  backup: {
    id: string;
    created_at: string;
    file_path: string;
    reason: string;
    content_hash: string;
    size_bytes: number;
  };
  modelId: string;
  backendProvider: string;
  proxyBaseUrl: string;
}

// =====================================================================
// 会话归属 Provider: 用于保持 Codex 对话列表归属,请求仍可映射到本地代理
// =====================================================================

export interface ConversationProviderCandidate {
  providerId: string;
  displayName: string | null;
  originalBaseUrl: string | null;
  wireApi: string;
  requiresOpenaiAuth: boolean | null;
  sourceKind: "current" | "backup" | "profile" | string;
  sourcePath: string;
  lastSeenAt: string;
  isBuiltinOpenai: boolean;
}

export interface ConversationProviderCandidatesView {
  activeProviderId: string;
  configPath: string;
  candidates: ConversationProviderCandidate[];
}

export interface ConversationProviderRequest {
  providerId: string;
  displayName: string | null;
  proxyPort: number;
  wireApi: string;
  requiresOpenaiAuth: boolean;
  originalBaseUrl: string | null;
  expectedHash?: string | null;
}

export interface ConversationProviderPreview {
  newConfigText: string;
  expectedHash: string;
  diff: ConfigDiffLineView[];
  insertions: number;
  deletions: number;
  providerId: string;
  proxyBaseUrl: string;
}

export interface ApplyConversationProviderResult {
  newConfigHash: string;
  backup: {
    id: string;
    created_at: string;
    file_path: string;
    reason: string;
    content_hash: string;
    size_bytes: number;
  };
}
