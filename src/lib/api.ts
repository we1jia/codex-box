import { invoke } from "@tauri-apps/api/core";
import type { DashboardSummary } from "@/lib/types";

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
