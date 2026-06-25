import { useEffect } from "react";
import { Sidebar } from "@/components/Sidebar";
import { Dashboard } from "@/pages/Dashboard";
import { PAGE_IDS, type PageId, useUIStore } from "@/store/ui";
import { startWindowDragFromMouseEvent } from "@/lib/window-drag";
import { useTranslation } from "react-i18next";
import { Languages, Search, Settings as SettingsIcon } from "lucide-react";
import { setLanguage } from "@/lib/i18n";
import {
  CodexRuntimePage,
  DiagnosticsPage,
  GatewayPage,
  MobileAccessPage,
  ProfilesPage,
  ProvidersPage,
  SettingsPage,
} from "@/pages/WorkspacePages";

function LangSwitch() {
  const { t, i18n } = useTranslation();
  const next = i18n.language === "zh" ? "en" : "zh";
  return (
    <button
      onClick={() => void setLanguage(next as "zh" | "en")}
      title={t("lang.switch")}
      className="w-6 h-6 rounded-md flex items-center justify-center text-ink-500 hover:bg-ink-900/5"
    >
      <Languages size={14} />
    </button>
  );
}

function ActivePage() {
  const activePage = useUIStore((s) => s.activePage);

  switch (activePage) {
    case "gateway":
      return <GatewayPage />;
    case "mobile_access":
      return <MobileAccessPage />;
    case "codex_runtime":
      return <CodexRuntimePage />;
    case "profiles":
      return <ProfilesPage />;
    case "providers":
      return <ProvidersPage />;
    case "diagnostics":
      return <DiagnosticsPage />;
    case "settings":
      return <SettingsPage />;
    case "dashboard":
    default:
      return <Dashboard />;
  }
}

export default function App() {
  const { t } = useTranslation();
  const collapsed = useUIStore((s) => s.sidebarCollapsed);
  const toggleSidebar = useUIStore((s) => s.toggleSidebar);
  const sidebarWidthPx = useUIStore((s) => s.sidebarWidth);
  const setActivePage = useUIStore((s) => s.setActivePage);
  // 收起态:主内容左右使用同一边距;展开态:为侧栏让位。
  const sidebarWidth = collapsed ? "0px" : `${sidebarWidthPx}px`;
  const contentLeft = collapsed
    ? "var(--content-right)"
    : `calc(var(--window-pad) + ${sidebarWidth} + var(--sidebar-content-gap))`;

  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.metaKey && event.key.toLowerCase() === "b") {
        event.preventDefault();
        toggleSidebar();
        return;
      }

      if (!event.metaKey || event.altKey || event.ctrlKey || event.shiftKey) {
        return;
      }

      if (event.key === ",") {
        event.preventDefault();
        setActivePage("settings");
        return;
      }

      const index = Number(event.key) - 1;
      const page = PAGE_IDS[index] as PageId | undefined;
      if (page && page !== "settings") {
        event.preventDefault();
        setActivePage(page);
      }
    };

    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [setActivePage, toggleSidebar]);

  return (
    // Tauri 窗口保留透明能力给圆角边缘，应用内部自己绘制稳定背景。
    <div className="h-full w-full relative bg-transparent text-ink-900 overflow-hidden">
      <div aria-hidden className="absolute inset-0 app-window-bg" />

      {/* 顶部拖拽区：覆盖 sidebar 右侧到窗口右边的整条顶部，解决"主内容卡片上方无法拖动"的问题 */}
      <div
        data-tauri-drag-region
        onMouseDown={startWindowDragFromMouseEvent}
        style={{ left: contentLeft }}
        className="absolute top-0 right-content-right h-14 z-[15] select-none cursor-default"
      >
        <div
          className="h-full flex items-center justify-end gap-2 text-ink-500 pr-1"
        >
          <button
            data-no-window-drag
            className="flex items-center gap-1 h-6 px-1.5 rounded-md hover:bg-black/[0.04] text-ink-500 transition-colors"
            title={t("header.searchHint")}
          >
            <Search size={12} />
            <span className="text-[10px] font-medium text-ink-400">⌘K</span>
          </button>
          <button
            data-no-window-drag
            onClick={() => setActivePage("settings")}
            className="w-6 h-6 rounded-md flex items-center justify-center text-ink-500 hover:bg-black/[0.04] transition-colors"
            title={t("header.settings")}
          >
            <SettingsIcon size={14} />
          </button>
          <LangSwitch />
          <div
            data-no-window-drag
            className="relative w-6 h-6 rounded-full bg-gradient-to-br from-ink-700 to-ink-900 text-white text-[9px] flex items-center justify-center"
          >
            CB
            <span className="absolute -bottom-0.5 -right-0.5 w-1.5 h-1.5 rounded-full bg-status-ok border border-white" />
          </div>
        </div>
      </div>

      {/* 主内容区从 sidebar 分隔线右侧开始，内部由页面组件控制留白。 */}
      <main
        style={{ left: contentLeft, top: "var(--content-top)" }}
        className="absolute right-content-right bottom-content-bottom z-10 overflow-hidden transition-[left] duration-200 ease-out"
      >
        <ActivePage />
      </main>

      <Sidebar />
    </div>
  );
}
