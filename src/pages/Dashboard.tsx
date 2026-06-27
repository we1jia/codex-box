import { useEffect } from "react";
import { useTranslation } from "react-i18next";
import { Settings as SettingsIcon } from "lucide-react";
import { useDashboardStore } from "@/store/dashboard";
import { useUIStore } from "@/store/ui";

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
  const { loading, error, load } = useDashboardStore();
  const setActivePage = useUIStore((s) => s.setActivePage);

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
          <button
            onClick={() => setActivePage("settings")}
            className="px-3 py-1.5 rounded-md bg-ink-900/5 text-xs text-ink-700 hover:bg-ink-900/10 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-[#0A84FF]/30 flex items-center justify-center"
            aria-label={t("dashboard.buttons.settings")}
            title={t("dashboard.buttons.settings")}
          >
            <SettingsIcon size={12} />
          </button>
        </div>
      </section>

      <section className="grid grid-cols-1 gap-4 xl:grid-cols-2">
        <div className="rounded-md bg-white/[0.86] border border-white/60 shadow-card p-5">
          <div className="mb-3">
            <h2 className="text-sm font-semibold text-ink-900">
              {t("dashboard.systemStatus.title")}
            </h2>
            <p className="mt-1 text-[12px] leading-[1.6] text-ink-500">
              {t("dashboard.systemStatus.desc")}
            </p>
          </div>
          <ul className="flex flex-col gap-2 text-sm">
            {[
              {
                key: "syntax",
                name: t("dashboard.systemStatus.items.config"),
                status: "ok",
                detail: t("dashboard.systemStatus.detail.config"),
              },
              {
                key: "runtime",
                name: t("dashboard.systemStatus.items.runtime"),
                status: "ok",
                detail: t("dashboard.systemStatus.detail.runtime"),
              },
              {
                key: "secrets",
                name: t("dashboard.systemStatus.items.secrets"),
                status: "warn",
                detail: t("dashboard.systemStatus.detail.secrets"),
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
                <span className="text-[11px] text-ink-400 w-28 text-right">
                  {c.detail}
                </span>
              </li>
            ))}
          </ul>
        </div>

        <div className="rounded-md bg-white/[0.86] border border-white/60 shadow-card p-5">
          <div className="mb-3">
            <h2 className="text-sm font-semibold text-ink-900">
              {t("dashboard.entry.title")}
            </h2>
            <p className="mt-1 text-[12px] leading-[1.6] text-ink-500">
              {t("dashboard.entry.desc")}
            </p>
          </div>
          <div className="grid grid-cols-1 gap-2 sm:grid-cols-2">
            {[
              { key: "models", page: "models" as const },
              { key: "logs", page: "logs" as const },
              { key: "settings", page: "settings" as const },
            ].map((entry) => (
              <button
                key={entry.key}
                onClick={() => setActivePage(entry.page)}
                className="rounded-md border border-ink-900/[0.06] bg-white/45 px-3 py-3 text-left transition-colors hover:bg-white/75"
              >
                <div className="text-[13px] font-semibold text-ink-800">
                  {t(`dashboard.entry.${entry.key}.title`)}
                </div>
                <div className="mt-1 text-[11px] leading-[1.45] text-ink-500">
                  {t(`dashboard.entry.${entry.key}.desc`)}
                </div>
              </button>
            ))}
          </div>
        </div>
      </section>
    </div>
  );
}
