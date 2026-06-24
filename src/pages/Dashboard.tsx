import { useEffect } from "react";
import { useTranslation } from "react-i18next";
import {
  User,
  Database,
  Globe,
  Puzzle,
  Activity,
  RefreshCw,
  Settings as SettingsIcon,
} from "lucide-react";
import { useDashboardStore } from "@/store/dashboard";
import { MetricCard } from "@/components/MetricCard";

/** 返回 i18n key，由渲染侧 t() */
function greetingKey(): "dawn" | "morning" | "afternoon" | "evening" {
  const h = new Date().getHours();
  if (h < 6) return "dawn";
  if (h < 12) return "morning";
  if (h < 18) return "afternoon";
  return "evening";
}

function nowStr(locale: string) {
  return new Date().toLocaleTimeString(locale.startsWith("en") ? "en-US" : "zh-CN", {
    hour: "2-digit",
    minute: "2-digit",
  });
}

export function Dashboard() {
  const { t, i18n } = useTranslation();
  const { data, loading, error, load } = useDashboardStore();

  useEffect(() => {
    load();
  }, [load]);

  return (
    <div className="relative h-full flex flex-col gap-4 min-w-0">
      <section className="rounded-md bg-white/[0.86] border border-white/60 shadow-card p-6">
        <h1 className="font-serif title-display text-[32px] font-semibold text-ink-900">
          {t(`dashboard.greeting.${greetingKey()}`)}
          {t("dashboard.greetingSuffix")}
        </h1>
        <p className="mt-2 text-sm text-ink-500">
          {error
            ? t("dashboard.loadError", { msg: error })
            : loading
              ? t("dashboard.loading")
              : t("dashboard.statusOk", { time: nowStr(i18n.language) })}
        </p>
        <div className="mt-4 flex items-center gap-2">
          <button className="px-3 py-1.5 rounded-md bg-ink-900/5 text-xs text-ink-700 flex items-center gap-1.5">
            <Activity size={12} /> {t("dashboard.buttons.daily")}
          </button>
          <button
            className="px-3 py-1.5 rounded-md bg-ink-900/5 text-xs text-ink-700"
            title={t("dashboard.buttons.settings")}
          >
            <SettingsIcon size={12} />
          </button>
        </div>
      </section>

      <section className="grid grid-cols-4 gap-4">
        <MetricCard
          label={t("metric.activeProfile.label")}
          value={data?.active_profile ?? "—"}
          sub={t("metric.activeProfile.sub")}
          icon={<User size={16} />}
          iconColor="#34C759"
        />
        <MetricCard
          label={t("metric.provider.label")}
          value={data ? t("metric.provider.value", { count: data.provider_count }) : "—"}
          sub={t("metric.provider.sub")}
          icon={<Database size={16} />}
          iconColor="#007AFF"
        />
        <MetricCard
          label={t("metric.network.label")}
          value={data?.network ?? "—"}
          sub={t("metric.network.sub")}
          icon={<Globe size={16} />}
          iconColor="#5AC8FA"
        />
        <MetricCard
          label={t("metric.mcp.label")}
          value={data ? `${data.mcp_count.enabled} / ${data.mcp_count.total}` : "—"}
          sub={t("metric.mcp.sub")}
          icon={<Puzzle size={16} />}
          iconColor="#AF52DE"
        />
      </section>

      <section className="grid grid-cols-[1.5fr_1fr] gap-4">
        <div className="rounded-md bg-white/[0.86] border border-white/60 shadow-card p-5">
          <div className="flex items-center justify-between mb-3">
            <h2 className="text-sm font-semibold text-ink-900">
              {t("health.title")}
            </h2>
            <button
              onClick={load}
              className="px-3 py-1.5 rounded-md bg-ink-900/5 text-xs text-ink-700 flex items-center gap-1.5"
            >
              <RefreshCw size={12} /> {t("dashboard.buttons.recheck")}
            </button>
          </div>
          <ul className="flex flex-col gap-2 text-sm">
            {[
              {
                key: "syntax",
                name: t("health.items.syntax"),
                status: "ok",
                ms: t("health.ms.syntax") as string,
              },
              {
                key: "providerUrl",
                name: t("health.items.providerUrl"),
                status: "ok",
                ms: t("health.ms.providerUrl") as string,
              },
              {
                key: "auth",
                name: t("health.items.auth"),
                status: "warn",
                ms: t("health.ms.auth") as string,
              },
              {
                key: "network",
                name: t("health.items.network"),
                status: "ok",
                ms: t("health.ms.network") as string,
              },
              {
                key: "mcpFs",
                name: t("health.items.mcpFs"),
                status: "ok",
                ms: t("health.ms.mcpFs") as string,
              },
              {
                key: "backupSpace",
                name: t("health.items.backupSpace"),
                status: "ok",
                ms: t("health.ms.backupSpace") as string,
              },
            ].map((c) => (
              <li key={c.key} className="flex items-center gap-2 px-1 py-1">
                <span
                  className={`w-1.5 h-1.5 rounded-full ${
                    c.status === "ok"
                      ? "bg-status-ok"
                      : c.status === "warn"
                        ? "bg-status-warn"
                        : "bg-status-fail"
                  }`}
                />
                <span className="flex-1 text-ink-700">{c.name}</span>
                <span
                  className={`text-[11px] px-1.5 py-0.5 rounded ${
                    c.status === "ok"
                      ? "bg-status-ok/10 text-status-ok"
                      : "bg-status-warn/10 text-status-warn"
                  }`}
                >
                  {t(`status.${c.status}`)}
                </span>
                <span className="text-[11px] text-ink-400 w-20 text-right">
                  {c.ms}
                </span>
              </li>
            ))}
          </ul>
        </div>

        <div className="rounded-md bg-white/[0.86] border border-white/60 shadow-card p-5">
          <div className="flex items-center justify-between mb-3">
            <h2 className="text-sm font-semibold text-ink-900">
              {t("activity.title")}
            </h2>
            <span className="text-[11px] text-ink-400">12</span>
          </div>
          <ul className="flex flex-col gap-2 text-sm">
            {[
              { t: "14:22", d: t("activity.items.backupDev") },
              { t: "14:20", d: t("activity.items.switchProvider") },
              { t: "14:18", d: t("activity.items.enableMcp") },
              { t: "14:10", d: t("activity.items.switchRoute") },
            ].map((a) => (
              <li key={a.t} className="flex items-center gap-2 px-1 py-1">
                <span className="w-1.5 h-1.5 rounded-full bg-status-ok" />
                <span className="text-[11px] text-ink-500 w-12">{a.t}</span>
                <span className="text-ink-700">{a.d}</span>
              </li>
            ))}
          </ul>
          <button className="mt-3 w-full px-3 py-1.5 rounded-md bg-ink-900/5 text-xs text-ink-700 flex items-center justify-center gap-1.5">
            {t("dashboard.buttons.viewAll")}
          </button>
        </div>
      </section>
    </div>
  );
}
