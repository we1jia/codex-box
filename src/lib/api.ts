import { invoke } from "@tauri-apps/api/core";
import { mockProfiles, mockProviders } from "@/lib/mock-data";
import type {
  ConfigChangePreviewView,
  ConfigSnapshotView,
  DashboardSummary,
  OpenCodexStatus,
} from "@/lib/types";

export type ApiResult<T> =
  | { ok: true; data: T }
  | { ok: false; error: string };

export async function invokeCmd<T>(
  name: string,
  args?: Record<string, unknown>
): Promise<ApiResult<T>> {
  if (!("__TAURI_INTERNALS__" in window)) {
    const fallback = browserFallback<T>(name);
    if (fallback) return fallback;
  }

  try {
    const data = await invoke<T>(name, args);
    return { ok: true, data };
  } catch (e) {
    return { ok: false, error: typeof e === "string" ? e : String(e) };
  }
}

function browserFallback<T>(name: string): ApiResult<T> | null {
  if (name.startsWith("opencodex_")) {
    const data: OpenCodexStatus = {
      sourcePath: "/Users/liuweijia/Desktop/AI/OpenCodex",
      exists: true,
      packageJsonPath: "/Users/liuweijia/Desktop/AI/OpenCodex/package.json",
      configYamlPath: "/Users/liuweijia/Desktop/AI/OpenCodex/config.yaml",
      configExists: false,
      authPasswordConfigured: false,
      running: false,
      managed: false,
      pid: null,
      host: "127.0.0.1",
      port: 3737,
      localUrl: "http://127.0.0.1:3737",
      lanUrls: ["http://192.168.1.10:3737"],
      mobileUrl: "http://192.168.1.10:3737",
      lanAccessEnabled: false,
      mobileUrlReachable: false,
      codexHome: "~/.codex",
      sharedCodexHome: "~/.codex",
      runtimeDir: "~/.codex/codex-box/opencodex/runtime",
      logPath: "~/.codex/codex-box/logs/opencodex-gateway.log",
      healthEndpoint: "http://127.0.0.1:3737/api/health",
      healthOk: false,
      healthStatus: null,
      lastError: null,
      lanRequiresPassword: true,
    };
    return { ok: true, data: data as T };
  }

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
