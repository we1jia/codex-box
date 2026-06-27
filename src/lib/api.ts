import { invoke } from "@tauri-apps/api/core";
import { mockCodexRuntime, mockProfiles, mockProviders } from "@/lib/mock-data";
import type {
  ConfigChangePreviewView,
  ConfigSnapshotView,
  DashboardSummary,
  InjectBaseUrlPreview,
  ApplyInjectResult,
  ModelCatalogEntry,
  OpenCodexCustomConfig,
  OpenCodexDeleteRequest,
  OpenCodexWriteRequest,
  OpenCodexWriteResult,
  ProxyModelsPreview,
  ProxyRouteTestResult,
  ProxyRuntimeLogs,
  ProxySessionsView,
  ProxyStatusView,
  ProviderRoute,
  CodexRuntimeStatus,
  CodexDesktopIntegrationStatus,
  CodexHistoryReconcileView,
  CodexHistoryUnifyApplyResult,
  CodexHistoryUnifyPreview,
  RestoreBaseUrlPreview,
  ApplyRestoreResult,
  ApplyConversationProviderResult,
  CodexModelsCacheRestorePreview,
  CodexModelsCacheRestoreResult,
  CodexMultirouterPreview,
  CodexMultirouterSyncResult,
  SimpleModelConfigResult,
  ConversationProviderCandidatesView,
  ConversationProviderPreview,
  EffectiveRoutingStatus,
  ConfigImportSource,
  ConfigImportPreview,
  ApplyConfigImportResult,
} from "@/lib/types";

const DEFAULT_MULTIROUTER_PROVIDER_ID = "codex_model_router_v2";

export type ApiResult<T> =
  | { ok: true; data: T }
  | { ok: false; error: string };

export async function invokeCmd<T>(
  name: string,
  args?: Record<string, unknown>
): Promise<ApiResult<T>> {
  if (!("__TAURI_INTERNALS__" in window)) {
    const fallback = browserFallback<T>(name, args);
    if (fallback) return fallback;
  }

  try {
    const data = await invoke<T>(name, args);
    return { ok: true, data };
  } catch (e) {
    return { ok: false, error: typeof e === "string" ? e : String(e) };
  }
}

function browserFallback<T>(
  name: string,
  args?: Record<string, unknown>
): ApiResult<T> | null {
  if (name === "config_snapshot") {
    const data: ConfigSnapshotView = {
      configPath: "~/.codex/config.toml",
      activeProfile: mockProfiles.find((profile) => profile.isActive)?.name || null,
      profiles: mockProfiles,
      providers: mockProviders,
    };
    return { ok: true, data: data as T };
  }

  if (name === "config_change_preview") {
    const data: ConfigChangePreviewView = {
      configPath: "~/.codex/config.toml",
      expectedHash: "browser-preview",
      insertions: 4,
      deletions: 0,
      requiresConfirmation: true,
      diff: [
        { kind: "insert", content: "[model_providers.\"browser-preview\"]\n", oldLine: null, newLine: 1 },
        { kind: "insert", content: "base_url = \"https://api.example.com/v1\"\n", oldLine: null, newLine: 2 },
        { kind: "insert", content: "wire_api = \"chat\"\n", oldLine: null, newLine: 3 },
        { kind: "insert", content: "api_key_env = \"EXAMPLE_API_KEY\"\n", oldLine: null, newLine: 4 },
      ],
    };
    return { ok: true, data: data as T };
  }

  if (name === "config_change_apply") {
    return { ok: false, error: "浏览器预览模式不能写入 ~/.codex/config.toml，请在 Tauri 应用中确认写入。" };
  }

  if (name === "opencodex_config_read") {
    const data: OpenCodexCustomConfig = {
      schemaVersion: 1,
      providersPath: "~/.codex/codex-box/providers.json",
      catalogPath: "~/.codex/codex-box/custom_model_catalog.json",
      providers: [],
      catalog: [],
      rawProvidersText: "[]",
      rawCatalogText: "[]",
      providersContentHash: "browser-providers-hash",
      catalogContentHash: "browser-catalog-hash",
      readAt: new Date().toISOString(),
      valid: true,
      parseErrors: [],
    };
    return { ok: true, data: data as T };
  }

  if (name === "config_import_sources_scan") {
    const data: ConfigImportSource[] = [
      {
        id: "opencodex",
        displayName: "OpenCodex",
        sourceKind: "opencodex",
        path: "~/.opencodex",
        providers: 0,
        models: 0,
        configSnapshots: 0,
        warnings: [],
        recommendedAction: "预览后导入到 ~/.codex/codex-box/。原 OpenCodex 文件不会被修改。",
        canImport: true,
      },
      {
        id: "codex-configs",
        displayName: "Codex 配置与备份",
        sourceKind: "codex_config",
        path: "~/.codex",
        providers: 0,
        models: 0,
        configSnapshots: 1,
        warnings: [],
        recommendedAction: "从 config.toml 和备份中恢复历史 Provider/Profile。",
        canImport: true,
      },
    ];
    return { ok: true, data: data as T };
  }

  if (name === "opencodex_import_preview") {
    const data: ConfigImportPreview = {
      sourceId: "opencodex",
      providersSourcePath: "~/.opencodex/providers.json",
      catalogSourcePath: "~/.opencodex/custom_model_catalog.json",
      providersTargetPath: "~/.codex/codex-box/providers.json",
      catalogTargetPath: "~/.codex/codex-box/custom_model_catalog.json",
      providersExpectedHash: "browser-providers-hash",
      catalogExpectedHash: "browser-catalog-hash",
      providersDiff: [],
      catalogDiff: [],
      providers: 0,
      models: 0,
      warnings: [],
    };
    return { ok: true, data: data as T };
  }

  if (name === "opencodex_import_apply") {
    const data: ApplyConfigImportResult = {
      providerWrite: {
        filePath: "~/.codex/codex-box/providers.json",
        backupId: "browser-provider-backup",
        newHash: "browser-provider-hash",
      },
      catalogWrite: {
        filePath: "~/.codex/codex-box/custom_model_catalog.json",
        backupId: "browser-catalog-backup",
        newHash: "browser-catalog-hash",
      },
    };
    return { ok: true, data: data as T };
  }

  if (name === "provider_route_upsert" || name === "catalog_entry_upsert") {
    const data: OpenCodexWriteResult = {
      filePath: name === "provider_route_upsert" ? "~/.codex/codex-box/providers.json" : "~/.codex/codex-box/custom_model_catalog.json",
      backupId: "browser-backup-id",
      newHash: "browser-new-hash",
    };
    return { ok: true, data: data as T };
  }

  if (name === "provider_route_delete" || name === "catalog_entry_delete") {
    const data: OpenCodexWriteResult = {
      filePath: name === "provider_route_delete" ? "~/.codex/codex-box/providers.json" : "~/.codex/codex-box/custom_model_catalog.json",
      backupId: "browser-backup-id",
      newHash: "browser-new-hash",
    };
    return { ok: true, data: data as T };
  }

  if (name === "simple_model_config_save") {
    const data: SimpleModelConfigResult = {
      provider: {
        name: "browser-preview",
        baseUrl: "https://api.example.com/v1",
        wireApi:
          (args as { request?: { wireApi?: string } })?.request?.wireApi ||
          "responses",
        apiKeyRef: "BROWSER_PREVIEW_API_KEY",
        httpHeaders: {},
        enabled: true,
        note: "browser preview",
      },
      model: {
        modelId: "browser-preview/example-model",
        displayName: "Example Model",
        provider: "browser-preview",
        backendModel: "example-model",
        backendProvider: "browser-preview",
        visible: true,
        reasoning: { enabled: true, levels: ["medium"] },
        note: "browser preview",
      },
      envKey: "BROWSER_PREVIEW_API_KEY",
      providerWrite: {
        filePath: "~/.codex/codex-box/providers.json",
        backupId: "browser-provider-backup",
        newHash: "browser-provider-hash",
      },
      catalogWrite: {
        filePath: "~/.codex/codex-box/custom_model_catalog.json",
        backupId: "browser-catalog-backup",
        newHash: "browser-catalog-hash",
      },
      requiresMultirouterSync: true,
      restartCodex: Boolean((args as { request?: { restartCodex?: boolean } })?.request?.restartCodex),
    };
    return { ok: true, data: data as T };
  }

  if (name === "codex_multirouter_preview") {
    const data: CodexMultirouterPreview = {
      providersPath: "~/.codex/codex-box/providers.json",
      catalogPath: "~/.codex/codex-box/custom_model_catalog.json",
      configPath: "~/.codex/config.toml",
      modelsCachePath: "~/.codex/models_cache.json",
      injectMapPath: "~/.codex/codex-box/inject-map.json",
      providersExpectedHash: "browser-providers-hash",
      catalogExpectedHash: "browser-catalog-hash",
      configExpectedHash: "browser-config-hash",
      modelsCacheExpectedHash: "browser-models-cache-hash",
      injectMapExpectedHash: "browser-inject-map-hash",
      routerProviderId: DEFAULT_MULTIROUTER_PROVIDER_ID,
      proxyPort: 1455,
      providersDiff: [
        { kind: "insert", content: "\"codexRouting\": { ... }", oldLine: null, newLine: 1 },
      ],
      catalogDiff: [
        { kind: "insert", content: `"backend_provider": "${DEFAULT_MULTIROUTER_PROVIDER_ID}"`, oldLine: null, newLine: 1 },
      ],
      configDiff: [
        { kind: "insert", content: "model_catalog_json = \"~/.codex/codex-box/custom_model_catalog.json\"", oldLine: null, newLine: 1 },
      ],
      modelsCacheDiff: [
        { kind: "insert", content: "\"etag\": \"codex-box-model-catalog\"", oldLine: null, newLine: 1 },
      ],
      injectMapDiff: [
        { kind: "delete", content: "\"originalBaseUrl\": \"http://127.0.0.1:8765/v1\"", oldLine: 1, newLine: null },
      ],
      routerProvider: {
        name: DEFAULT_MULTIROUTER_PROVIDER_ID,
        baseUrl: "http://127.0.0.1:1455/v1",
        wireApi: "responses",
        apiKeyRef: null,
        httpHeaders: {},
        enabled: true,
        note: "browser preview",
        codexRouting: {
          enabled: true,
          defaultRouteId: "minimax",
          routes: [
            {
              id: "minimax",
              label: "minimax",
              enabled: true,
              targetProviderId: "minimax",
              match: { models: ["minimax-m3"], prefixes: [] },
              upstream: {
                apiFormat: "openai_responses",
                auth: { source: "provider_config" },
                modelMap: { "minimax-m3": "MiniMax-M3" },
              },
            },
          ],
        },
      },
      routeCount: 1,
      routedModelCount: 1,
      skippedModels: [],
      proxyBaseUrl: "http://127.0.0.1:1455/v1",
      ensureCodexConfig: true,
      modelsCacheTouched: true,
      injectMapTouched: true,
    };
    return { ok: true, data: data as T };
  }

  if (name === "codex_multirouter_sync") {
    return {
      ok: false,
      error:
        "codex_multirouter_sync 已停用。请先预览 MultiRouter diff，再确认发布。",
    };
  }

  if (name === "codex_multirouter_apply") {
    const data: CodexMultirouterSyncResult = {
      routerProvider: {
        name: DEFAULT_MULTIROUTER_PROVIDER_ID,
        baseUrl: "http://127.0.0.1:1455/v1",
        wireApi: "responses",
        apiKeyRef: null,
        httpHeaders: {},
        enabled: true,
        note: "browser preview",
        codexRouting: {
          enabled: true,
          defaultRouteId: "minimax",
          routes: [
            {
              id: "minimax",
              label: "minimax",
              enabled: true,
              targetProviderId: "minimax",
              match: { models: ["minimax-m3"], prefixes: [] },
              upstream: {
                apiFormat: "openai_responses",
                auth: { source: "provider_config" },
                modelMap: { "minimax-m3": "MiniMax-M3" },
              },
            },
          ],
        },
      },
      routeCount: 1,
      routedModelCount: 1,
      skippedModels: [],
      proxyBaseUrl: "http://127.0.0.1:1455/v1",
      providerWrite: {
        filePath: "~/.codex/codex-box/providers.json",
        backupId: "browser-provider-backup",
        newHash: "browser-provider-hash",
      },
      catalogWrite: {
        filePath: "~/.codex/codex-box/custom_model_catalog.json",
        backupId: "browser-catalog-backup",
        newHash: "browser-catalog-hash",
      },
      configWrite: {
        filePath: "~/.codex/config.toml",
        backupId: "browser-config-backup",
        newHash: "browser-config-hash",
      },
      modelsCacheWrite: {
        filePath: "~/.codex/models_cache.json",
        backupId: "browser-models-cache-backup",
        newHash: "browser-models-cache-hash",
      },
      injectMapWrite: {
        filePath: "~/.codex/codex-box/inject-map.json",
        backupId: "browser-inject-map-backup",
        newHash: "browser-inject-map-hash",
      },
      configTouched: true,
      modelsCacheTouched: true,
      injectMapTouched: true,
    };
    return { ok: true, data: data as T };
  }

  if (name === "codex_models_cache_restore_preview") {
    const data: CodexModelsCacheRestorePreview = {
      modelsCachePath: "~/.codex/models_cache.json",
      backupPath: "~/.codex/models_cache.codex-box-backup.json",
      modelsCacheExpectedHash: "browser-models-cache-hash",
      backupExists: true,
      ownedCache: true,
      restoreAvailable: true,
      willDelete: false,
      diff: [
        { kind: "delete", content: "\"etag\": \"codex-box-model-catalog\"", oldLine: 1, newLine: null },
        { kind: "insert", content: "\"etag\": \"W/\\\"official\\\"\"", oldLine: null, newLine: 1 },
      ],
    };
    return { ok: true, data: data as T };
  }

  if (name === "codex_models_cache_restore_apply") {
    const data: CodexModelsCacheRestoreResult = {
      modelsCachePath: "~/.codex/models_cache.json",
      backupPath: "~/.codex/models_cache.codex-box-backup.json",
      backupId: "browser-models-cache-owned-backup",
      newHash: "browser-official-cache-hash",
      restored: true,
      deleted: false,
    };
    return { ok: true, data: data as T };
  }

  if (name === "codex_runtime_status") {
    const data: CodexRuntimeStatus = mockCodexRuntime;
    return { ok: true, data: data as T };
  }

  if (name === "reveal_path" || name === "open_path") {
    return {
      ok: false,
      error: "浏览器预览模式不能打开本机文件，请在 Tauri 应用中操作。",
    };
  }

  if (name === "proxy_status") {
    const data: ProxyStatusView = {
      status: "stopped",
      port: 0,
      startedAt: "",
      uptimeMs: null,
      lastError: null,
      providerCount: 0,
      providers: [],
    };
    return { ok: true, data: data as T };
  }

  if (name === "proxy_runtime_logs") {
    const data: ProxyRuntimeLogs = {
      redacted: true,
      items: [
        {
          at: new Date().toISOString(),
          level: "info",
          scope: "runtime",
          message: "浏览器预览模式: 本地代理状态已读取",
        },
        {
          at: new Date().toISOString(),
          level: "warn",
          scope: "routes",
          message: "尚未启用任何模型来源",
        },
        {
          at: new Date().toISOString(),
          level: "info",
          scope: "security",
          message: "日志已脱敏,不会输出 API Key 或请求密钥",
        },
      ],
    };
    return { ok: true, data: data as T };
  }

  if (name === "proxy_sessions") {
    const data: ProxySessionsView = {
      activeSessionId: "default",
      sessions: [
        {
          id: "default",
          label: "默认会话",
          status: "idle",
          providerCount: 0,
          modelCount: 0,
          lastUsedAt: "",
        },
      ],
    };
    return { ok: true, data: data as T };
  }

  if (name === "effective_routing_status") {
    const data: EffectiveRoutingStatus = {
      configPath: "~/.codex/config.toml",
      currentModel: "browser-preview/model",
      modelProvider: "openai",
      requestBaseUrl: "http://127.0.0.1:1455/v1",
      requestBaseUrlSource: "openai_base_url",
      modelCatalogPath: "~/.codex/codex-box/custom_model_catalog.json",
      catalogConfigured: true,
      catalogModelFound: false,
      catalogProvider: null,
      backendProvider: null,
      backendModel: null,
      upstreamBaseUrl: null,
      proxyRunning: false,
      proxyPort: null,
      issues: [
        {
          severity: "warn",
          code: "browser_preview",
          message: "浏览器预览模式使用模拟链路；真实生效链路请在 Tauri 应用中查看。",
        },
      ],
    };
    return { ok: true, data: data as T };
  }

  if (name === "codex_desktop_integration_status") {
    const data: CodexDesktopIntegrationStatus = {
      configPath: "~/.codex/config.toml",
      configParsed: true,
      configError: null,
      model: "browser-preview/model",
      modelProvider: DEFAULT_MULTIROUTER_PROVIDER_ID,
      modelCatalogJson: "~/.codex/codex-box/custom_model_catalog.json",
      customModelCatalogPath: "~/.codex/codex-box/custom_model_catalog.json",
      customModelCatalogExists: true,
      customCatalogNativeOpenaiModelCount: 5,
      customCatalogByokModelCount: 1,
      officialRouteConfigured: true,
      officialRouteModelCount: 5,
      officialRouteAuthSource: "managed_codex_oauth",
      officialRouteBaseUrl: "https://chatgpt.com/backend-api/codex",
      routerProviderBaseUrl: "http://127.0.0.1:1455/v1",
      routerProviderRequiresOpenaiAuth: true,
      routerProviderSupportsWebsockets: false,
      routerProviderUsesProxyManagedBearer: true,
      routerProviderModelsCount: 0,
      modelsCachePath: "~/.codex/models_cache.json",
      modelsCacheExists: false,
      modelsCacheOwnedByCodexBox: false,
      modelsCacheModelCount: null,
      modelsCacheClientVersionPresent: false,
      authPath: "~/.codex/auth.json",
      authJsonExists: false,
      authMode: null,
      chatgptAuthLikely: false,
      openaiApiKeyPresentInAuth: false,
      codexRunning: false,
      codexRemoteDebuggingPort: null,
      codexProcesses: [],
      pickerReadinessStatus: "needs_attention",
      pickerReadinessSummary:
        "浏览器预览只能展示样例状态；请在 Tauri 应用中查看真实 picker readiness。",
      pickerReadinessBlockers: [],
      pickerReadinessWarnings: ["browser_preview"],
      issues: [
        {
          severity: "warn",
          code: "browser_preview",
          message: "浏览器预览模式不能检查本机 Codex Desktop 进程；请在 Tauri 应用中查看真实状态。",
        },
      ],
    };
    return { ok: true, data: data as T };
  }

  if (name === "codex_desktop_picker_unlock") {
    const data = {
      attemptedPorts: [9229, 9222, 9223, 9230, 9231],
      debugPort: null,
      targetCount: 0,
      injectedTargetCount: 0,
      rendererReports: [],
      modelCount: 6,
      modelNames: ["gpt-5.5", "gpt-5.4", "minimax-m3"],
      injected: false,
      launched: false,
      codexExecutable: null,
      status: "browser_preview",
      message: "浏览器预览模式不能注入 Codex Desktop renderer；请在 Tauri 应用中执行。",
      errors: [],
    };
    return { ok: true, data: data as T };
  }

  if (name === "codex_desktop_launch_with_debugging_and_unlock") {
    const data = {
      attemptedPorts: [9229],
      debugPort: 9229,
      targetCount: 0,
      injectedTargetCount: 0,
      rendererReports: [],
      modelCount: 6,
      modelNames: ["gpt-5.5", "gpt-5.4", "minimax-m3"],
      injected: false,
      launched: false,
      codexExecutable: null,
      status: "browser_preview",
      message: "浏览器预览模式不能启动 Codex Desktop；请在 Tauri 应用中执行。",
      errors: [],
    };
    return { ok: true, data: data as T };
  }

  if (name === "codex_history_reconcile") {
    const data: CodexHistoryReconcileView = {
      codexHome: "~/.codex",
      configPath: "~/.codex/config.toml",
      liveConfigModelProvider: DEFAULT_MULTIROUTER_PROVIDER_ID,
      suggestedTargetProvider: DEFAULT_MULTIROUTER_PROVIDER_ID,
      sourceProviderIds: ["openai"],
      activeStateDbPath: "~/.codex/sqlite/state_5.sqlite",
      activeStateDbKind: "sqlite_subdir",
      providersFound: ["openai", DEFAULT_MULTIROUTER_PROVIDER_ID, "codex_local_access"],
      sqliteStores: [
        {
          path: "~/.codex/sqlite/state_5.sqlite",
          kind: "sqlite_subdir",
          total: 12,
          providerCounts: { openai: 10, codex_model_router_v2: 2 },
          readable: true,
          error: null,
        },
      ],
      jsonlSummary: {
        roots: ["~/.codex/sessions", "~/.codex/archived_sessions"],
        totalFiles: 12,
        providerCounts: { openai: 10, codex_model_router_v2: 2 },
        unreadableFiles: 0,
      },
      sessionIndexPath: "~/.codex/session_index.jsonl",
      sessionIndexExists: true,
      globalStatePath: "~/.codex/.codex-global-state.json",
      globalStateExists: true,
      driftDetected: true,
      providerRowsToUpdate: 10,
      rolloutProviderLinesToUpdate: 10,
      warnings: [
        {
          severity: "warn",
          code: "browser_preview",
          message:
            "浏览器预览模式使用模拟历史归属；真实分布请在 Tauri 应用中查看。",
        },
      ],
    };
    return { ok: true, data: data as T };
  }

  if (name === "codex_history_unify_preview") {
    const data: CodexHistoryUnifyPreview = {
      codexHome: "~/.codex",
      targetProvider: DEFAULT_MULTIROUTER_PROVIDER_ID,
      sourceProviderIds: ["openai"],
      activeStateDbPath: "~/.codex/sqlite/state_5.sqlite",
      activeStateDbKind: "sqlite_subdir",
      providerRowsToUpdate: 10,
      rolloutFilesToUpdate: 10,
      rolloutProviderLinesToUpdate: 10,
      userEventRowsToUpdate: 4,
      visibleCandidateRows: 10,
      sessionIndexMissingToAppend: 3,
      focusRowsToMove: 10,
      workspaceHintsToFix: 2,
      projectlessIdsToRemove: 1,
      savedWorkspaceRootsToAdd: 1,
      sessionIndexPath: "~/.codex/session_index.jsonl",
      sessionIndexExists: true,
      globalStatePath: "~/.codex/.codex-global-state.json",
      globalStateExists: true,
      backupDir: "~/.codex/codex-box/backups/history-unify-browser",
      codexRunning: true,
      codexProcesses: ["browser preview: Codex process state is simulated"],
      canApply: false,
      warnings: [
        {
          severity: "fail",
          code: "browser_preview",
          message:
            "浏览器预览模式不能写入历史库；真实统一请在 Tauri 应用中执行。",
        },
      ],
    };
    return { ok: true, data: data as T };
  }

  if (name === "codex_history_unify_apply") {
    const data: CodexHistoryUnifyApplyResult = {
      preview: {
        codexHome: "~/.codex",
        targetProvider: DEFAULT_MULTIROUTER_PROVIDER_ID,
        sourceProviderIds: ["openai"],
        activeStateDbPath: "~/.codex/sqlite/state_5.sqlite",
        activeStateDbKind: "sqlite_subdir",
        providerRowsToUpdate: 10,
        rolloutFilesToUpdate: 10,
        rolloutProviderLinesToUpdate: 10,
        userEventRowsToUpdate: 4,
        visibleCandidateRows: 10,
        sessionIndexMissingToAppend: 3,
        focusRowsToMove: 10,
        workspaceHintsToFix: 2,
        projectlessIdsToRemove: 1,
        savedWorkspaceRootsToAdd: 1,
        sessionIndexPath: "~/.codex/session_index.jsonl",
        sessionIndexExists: true,
        globalStatePath: "~/.codex/.codex-global-state.json",
        globalStateExists: true,
        backupDir: "~/.codex/codex-box/backups/history-unify-browser",
        codexRunning: false,
        codexProcesses: [],
        canApply: true,
        warnings: [],
      },
      backup: {
        backupDir: "~/.codex/codex-box/backups/history-unify-browser",
        files: [],
        rolloutManifestPath:
          "~/.codex/codex-box/backups/history-unify-browser/rollout-manifest.json",
      },
      providerRowsUpdated: 10,
      rolloutFilesUpdated: 10,
      rolloutProviderLinesUpdated: 10,
      userEventRowsUpdated: 4,
      focusRowsUpdated: 10,
      sessionIndexAppended: 3,
      sessionIndexRowsMoved: 10,
      sessionIndexTitlesUpdated: 3,
      workspaceHintsFixed: 2,
      projectlessIdsRemoved: 1,
      savedWorkspaceRootsAdded: 1,
    };
    return { ok: true, data: data as T };
  }

  if (name === "proxy_start" || name === "proxy_restart") {
    return { ok: false, error: "浏览器预览模式不能启动本地代理,请在 Tauri 应用中操作。" };
  }

  if (name === "proxy_stop") {
    const data: ProxyStatusView = {
      status: "stopped",
      port: 0,
      startedAt: "",
      uptimeMs: null,
      lastError: null,
      providerCount: 0,
      providers: [],
    };
    return { ok: true, data: data as T };
  }

  if (name === "proxy_models_preview") {
    return { ok: false, error: "浏览器预览模式无法调本地代理的 /v1/models" };
  }

  if (name === "proxy_route_test") {
    const performUpstream = Boolean(
      (args?.request as { performUpstream?: boolean } | undefined)?.performUpstream,
    );
    const data: ProxyRouteTestResult = {
      status: performUpstream ? "failed" : "passed",
      modelId:
        ((args?.request as { modelId?: string } | undefined)?.modelId ||
          "minimax-m3") as string,
      providerName: "minimax",
      upstreamModel: "MiniMax-M3",
      upstreamBaseUrl: "https://api.minimaxi.com/v1",
      wireApi: "chat",
      authSource: null,
      textOnly: true,
      usedChatFallback: true,
      imagePartSentToChat: false,
      upstreamStatusCode: performUpstream ? 401 : null,
      upstreamLatencyMs: performUpstream ? 138 : null,
      chatRequestPreview: {
        model: "MiniMax-M3",
        messages: [{ role: "user", content: "Codex Box route test" }],
        stream: true,
      },
      steps: [
        {
          id: "resolver",
          label: "Resolver",
          status: "passed",
          detail: "minimax-m3 -> minimax (MiniMax-M3)",
        },
        {
          id: "protocol_transform",
          label: "Responses -> Chat",
          status: "passed",
          detail: "response.create converted to chat/completions.",
        },
        {
          id: "text_only_guard",
          label: "Text-only guard",
          status: "passed",
          detail: "Image input was downgraded before chat upstream.",
        },
        {
          id: "upstream_request",
          label: "Upstream request",
          status: performUpstream ? "failed" : "skipped",
          detail: performUpstream
            ? "Mock upstream returned 401 because browser preview has no env key."
            : "Dry-run mode does not call the real upstream.",
        },
      ],
      warnings: [],
    };
    return { ok: true, data: data as T };
  }

  if (name === "proxy_inject_base_url_preview" || name === "proxy_restore_base_url_preview") {
    return { ok: false, error: "浏览器预览模式不能改写 ~/.codex/config.toml" };
  }

  if (name === "conversation_provider_candidates") {
    const data: ConversationProviderCandidatesView = {
      activeProviderId: DEFAULT_MULTIROUTER_PROVIDER_ID,
      configPath: "~/.codex/config.toml",
      candidates: [
        {
          providerId: DEFAULT_MULTIROUTER_PROVIDER_ID,
          displayName: "Codex MultiRouter",
          originalBaseUrl: "http://127.0.0.1:1455/v1",
          wireApi: "responses",
          requiresOpenaiAuth: true,
          sourceKind: "current",
          sourcePath: "~/.codex/config.toml",
          lastSeenAt: new Date().toISOString(),
          isBuiltinOpenai: false,
        },
        {
          providerId: "openai",
          displayName: "OpenAI",
          originalBaseUrl: null,
          wireApi: "responses",
          requiresOpenaiAuth: true,
          sourceKind: "current",
          sourcePath: "~/.codex/config.toml",
          lastSeenAt: new Date().toISOString(),
          isBuiltinOpenai: true,
        },
        {
          providerId: "codex_local_access",
          displayName: "Codex API Service",
          originalBaseUrl: "http://localhost:51232/v1",
          wireApi: "responses",
          requiresOpenaiAuth: true,
          sourceKind: "backup",
          sourcePath: "~/.codex/config.toml.bak",
          lastSeenAt: new Date().toISOString(),
          isBuiltinOpenai: false,
        },
      ],
    };
    return { ok: true, data: data as T };
  }

  if (name === "conversation_provider_preview") {
    const data: ConversationProviderPreview = {
      newConfigText: "",
      expectedHash: "browser-preview",
      insertions: 3,
      deletions: 0,
      providerId: "openai",
      proxyBaseUrl: "http://127.0.0.1:1455/v1",
      diff: [
        { kind: "insert", content: "model_provider = \"openai\"\n", oldLine: null, newLine: 1 },
        { kind: "insert", content: "openai_base_url = \"http://127.0.0.1:1455/v1\"\n", oldLine: null, newLine: 2 },
        { kind: "insert", content: "model_catalog_json = \"~/.codex/codex-box/custom_model_catalog.json\"\n", oldLine: null, newLine: 3 },
      ],
    };
    return { ok: true, data: data as T };
  }

  if (name === "conversation_provider_apply") {
    const data: ApplyConversationProviderResult = {
      newConfigHash: "browser-new-config-hash",
      backup: {
        id: "browser-backup",
        created_at: new Date().toISOString(),
        file_path: "~/.codex/codex-box/backups/browser.toml",
        reason: "pre_write",
        content_hash: "browser-old-hash",
        size_bytes: 0,
      },
    };
    return { ok: true, data: data as T };
  }

  if (name !== "dashboard_summary") return null;

  const data: DashboardSummary = {
    active_profile: "gpt-5.5",
    provider_count: 0,
    mcp_count: { enabled: 5, total: 5 },
    network: "enabled",
    last_backup_at: new Date().toISOString(),
    health_summary: { ok: 5, warn: 1, fail: 0 },
  };

  return { ok: true, data: data as T };
}
