import { invoke } from "@tauri-apps/api/core";
import { mockCodexRuntime, mockModelCatalog, mockProfiles, mockProviderRoutes, mockProviders } from "@/lib/mock-data";
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
  ProxyStatusView,
  ProviderRoute,
  CodexRuntimeStatus,
  RestoreBaseUrlPreview,
  ApplyRestoreResult,
} from "@/lib/types";

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
      providersPath: "~/.opencodex/providers.json",
      catalogPath: "~/.opencodex/custom_model_catalog.json",
      providers: mockProviderRoutes,
      catalog: mockModelCatalog,
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

  if (name === "provider_route_upsert" || name === "catalog_entry_upsert") {
    const data: OpenCodexWriteResult = {
      filePath: name === "provider_route_upsert" ? "~/.opencodex/providers.json" : "~/.opencodex/custom_model_catalog.json",
      backupId: "browser-backup-id",
      newHash: "browser-new-hash",
    };
    return { ok: true, data: data as T };
  }

  if (name === "provider_route_delete" || name === "catalog_entry_delete") {
    const data: OpenCodexWriteResult = {
      filePath: name === "provider_route_delete" ? "~/.opencodex/providers.json" : "~/.opencodex/custom_model_catalog.json",
      backupId: "browser-backup-id",
      newHash: "browser-new-hash",
    };
    return { ok: true, data: data as T };
  }

  if (name === "codex_runtime_status") {
    const data: CodexRuntimeStatus = mockCodexRuntime;
    return { ok: true, data: data as T };
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

  if (name === "proxy_inject_base_url_preview" || name === "proxy_restore_base_url_preview") {
    return { ok: false, error: "浏览器预览模式不能改写 ~/.codex/config.toml" };
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