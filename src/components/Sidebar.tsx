import { useCallback, useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import {
  Plus,
  LayoutDashboard,
  Users,
  Server,
  Globe,
  Puzzle,
  GitCompare,
  Activity,
  Settings,
  Cable,
  PanelLeftClose,
  PanelLeftOpen,
} from "lucide-react";
import {
  type PageId,
  useUIStore,
  SIDEBAR_WIDTH_MIN,
  SIDEBAR_WIDTH_MAX,
} from "@/store/ui";
import { startWindowDragFromMouseEvent } from "@/lib/window-drag";

const NAV_ITEMS = [
  { id: "dashboard", Icon: LayoutDashboard, key: "1" },
  { id: "profiles", Icon: Users, key: "2" },
  { id: "providers", Icon: Server, key: "3" },
  { id: "gateway", Icon: Cable, key: "4" },
  { id: "mcp", Icon: Puzzle, key: "5" },
  { id: "network", Icon: Globe, key: "6" },
  { id: "config_diff", Icon: GitCompare, key: "7" },
  { id: "diagnostics", Icon: Activity, key: "8" },
  { id: "settings", Icon: Settings, key: "," },
] as const;

const SIDEBAR_TOGGLE_TOP = "16px";
const SIDEBAR_TOGGLE_COLLAPSED_LEFT = "98px";

interface SidebarProps {
  preview?: boolean;
}

/**
 * 展开/收起按钮的共享逻辑 + 视觉。
 * 通过 variant 控制渲染位置:
 * - "inline": 作为 sidebar 顶部菜单区的 flex 项(展开态)
 * - "floating": fixed 浮窗态(收起态)
 */
function KButton({
  collapsed,
  variant,
  onClick,
  onHoverStart,
  onHoverEnd,
}: {
  collapsed: boolean;
  variant: "inline" | "floating";
  onClick: () => void;
  onHoverStart?: () => void;
  onHoverEnd?: () => void;
}) {
  const { t } = useTranslation();
  // 共享 props
  const sharedProps = {
    "data-no-window-drag": true,
    onClick,
    "aria-label": collapsed ? t("sidebar.expand") : t("sidebar.collapse"),
    title: `${collapsed ? t("sidebar.expand") : t("sidebar.collapse")} (⌘B)`,
    onMouseEnter: onHoverStart,
    onMouseLeave: onHoverEnd,
  } as const;

  const baseClass = [
    "w-7 h-7 rounded-lg flex items-center justify-center shrink-0",
    "bg-white/45 hover:bg-white/75 border border-black/[0.06] hover:border-black/[0.12]",
    "text-ink-700 hover:text-ink-900",
    "opacity-70 hover:opacity-100",
    "transition-all duration-150",
  ].join(" ");
  if (variant === "floating") {
    return (
      <button
        {...sharedProps}
        style={{
          left: SIDEBAR_TOGGLE_COLLAPSED_LEFT,
          top: SIDEBAR_TOGGLE_TOP,
        }}
        className={`fixed z-30 ${baseClass}`}
      >
        <PanelLeftOpen size={14} />
      </button>
    );
  }

  if (variant === "inline") {
    // 展开态:用 fixed 与收起态同一坐标系(top=16 已与红绿灯水平对齐),
    // left 计算为 sidebar 右边缘外 12px,跟随宽度自动跟随。
    const w = useUIStore.getState().sidebarWidth;
    return (
      <button
        {...sharedProps}
        style={{
          top: SIDEBAR_TOGGLE_TOP,
          left: `calc(var(--window-pad) + ${w}px + 12px)`,
        }}
        className={`fixed z-30 ${baseClass}`}
      >
        <PanelLeftClose size={14} />
      </button>
    );
  }
}

/**
 * 侧栏右侧拖拽手柄（macOS Finder 风格）。
 * - 仅保留不可见热区,不显示分隔线。
 * - mousedown 进入拖拽态,实时更新 store 中的 sidebarWidth。
 * - 拖拽期间锁 body 光标和文本选择。
 */
function ResizeHandle({
  width,
  onResize,
}: {
  width: number;
  onResize: (next: number) => void;
}) {
  const { t } = useTranslation();
  const draggingRef = useRef(false);
  const startXRef = useRef(0);
  const startWidthRef = useRef(0);

  useEffect(() => {
    const onMove = (e: MouseEvent) => {
      if (!draggingRef.current) return;
      const next = startWidthRef.current + (e.clientX - startXRef.current);
      onResize(next);
    };
    const onUp = () => {
      if (!draggingRef.current) return;
      draggingRef.current = false;
      document.body.style.cursor = "";
      document.body.style.userSelect = "";
    };
    window.addEventListener("mousemove", onMove);
    window.addEventListener("mouseup", onUp);
    return () => {
      window.removeEventListener("mousemove", onMove);
      window.removeEventListener("mouseup", onUp);
    };
  }, [onResize]);

  return (
    <div
      role="separator"
      aria-orientation="vertical"
      aria-label={t("sidebar.resize", {
        width,
        min: SIDEBAR_WIDTH_MIN,
        max: SIDEBAR_WIDTH_MAX,
      })}
      data-no-window-drag
      onMouseDown={(e) => {
        e.preventDefault();
        e.stopPropagation();
        startXRef.current = e.clientX;
        startWidthRef.current = width;
        draggingRef.current = true;
        document.body.style.cursor = "ew-resize";
        document.body.style.userSelect = "none";
      }}
      className="absolute right-0 top-0 bottom-0 w-2 cursor-ew-resize z-10"
    />
  );
}

function NavList({ closeFlyout }: { closeFlyout?: () => void }) {
  const { t } = useTranslation();
  const activePage = useUIStore((s) => s.activePage);
  const setActivePage = useUIStore((s) => s.setActivePage);

  return (
    <nav className="flex flex-col gap-1 px-4">
      {NAV_ITEMS.map((item) => {
        const Icon = item.Icon;
        const id = item.id as PageId;
        const active = activePage === id;
        const label = t(`nav.${id}`);
        return (
          <button
            key={id}
            onClick={() => {
              setActivePage(id);
              closeFlyout?.();
            }}
            aria-current={active ? "page" : undefined}
            className={[
              "flex items-center w-full gap-2.5 px-3 py-2 rounded-md text-sm transition-colors",
              active
                ? "bg-[#D7E8F2] text-[#0A84FF] font-semibold"
                : "text-ink-700 hover:bg-white/45",
            ].join(" ")}
          >
            <Icon size={15} strokeWidth={1.75} className="shrink-0" />
            <span className="flex-1 text-left truncate">{label}</span>
            <span className="text-[11px] text-ink-400 shrink-0">
              ⌘{item.key}
            </span>
          </button>
        );
      })}
    </nav>
  );
}

export function Sidebar({ preview = false }: SidebarProps) {
  const { t } = useTranslation();
  const storedCollapsed = useUIStore((s) => s.sidebarCollapsed);
  const setSidebar = useUIStore((s) => s.setSidebar);
  const sidebarWidth = useUIStore((s) => s.sidebarWidth);
  const setSidebarWidth = useUIStore((s) => s.setSidebarWidth);
  // 收起态下的 hover 浮窗显示状态:鼠标进入 K 按钮或浮窗时为 true
  const [hoverOpen, setHoverOpen] = useState(false);
  const collapsed = preview ? false : storedCollapsed;

  // 收起态:hover 时显示浮窗;否则隐藏。预览模式始终展开。
  const showFlyout = !preview && collapsed && hoverOpen;

  // 拖拽实时回调:clamp 到合法区间后写入 store(每次 mousemove 触发)。
  // 写入走 React 18 自动 batch,re-render 成本可接受。
  const handleResize = useCallback(
    (raw: number) => setSidebarWidth(raw),
    [setSidebarWidth]
  );

  return (
    <>
      {/* 主体侧栏:
          - 展开态:始终显示。
          - 收起态:不渲染主体,改由 hover 浮窗替代(见下方)。
          - 浮窗态:绝对定位浮在 K 按钮下方,鼠标可进入交互。 */}
      {!collapsed && (
        <aside
          data-tauri-drag-region
          onMouseDown={startWindowDragFromMouseEvent}
          style={{
            left: "var(--window-pad)",
            top: "var(--window-pad)",
            bottom: "var(--window-pad)",
            height: "calc(100vh - (var(--window-pad) * 2))",
            width: `${sidebarWidth}px`,
          }}
          className={[
            "group/sidebar",
            "fixed z-20",
            "bg-[#EAF6FC]/82 backdrop-blur-glass border border-white/60 rounded-md",
            "shadow-[0_1px_2px_rgba(0,0,0,0.03),0_12px_36px_rgba(31,88,122,0.08)]",
            "flex flex-col",
          ].join(" ")}
        >
          {/* 顶部菜单区,留出 traffic lights 安全区。 */}
          <div className="px-5 pt-[76px] pb-4 flex items-center gap-2.5">
            <button
              title={t("sidebar.newChat")}
              aria-label={t("sidebar.newChat")}
              className="w-8 h-8 rounded-full bg-ink-900 text-white hover:bg-ink-700 transition-colors shrink-0 flex items-center justify-center"
            >
              <Plus size={16} strokeWidth={2.25} />
            </button>
            <div className="flex-1 min-w-0">
              <div className="font-serif text-[15px] text-ink-900 leading-none">
                Codex Box
              </div>
              <div className="text-[11px] text-ink-500 mt-1.5 flex items-center gap-1.5">
                <span className="w-1.5 h-1.5 rounded-full bg-status-ok" />
                {t("sidebar.connectedVersion", { version: "v0.0.1" })}
              </div>
            </div>
          </div>

          <NavList />

          <div className="mt-auto p-3">
            <div className="flex items-center gap-2.5">
              <button
                title={t("sidebar.proactiveMode")}
                className="relative w-7 h-7 rounded-full bg-gradient-to-br from-ink-700 to-ink-900 flex items-center justify-center text-white text-[9px] font-semibold shrink-0"
              >
                CB
                <span className="absolute -bottom-0.5 -right-0.5 w-2 h-2 rounded-full bg-status-ok border-2 border-white" />
              </button>
              <div className="flex-1 px-2 py-1 rounded-md bg-white/45 text-[11px] text-ink-700 flex items-center justify-center gap-1.5 truncate">
                <span className="w-1.5 h-1.5 rounded-full bg-status-ok shrink-0" />
                <span className="truncate">{t("sidebar.proactiveMode")}</span>
              </div>
            </div>
          </div>

          {/* 右侧拖拽热区 */}
          <ResizeHandle width={sidebarWidth} onResize={handleResize} />
        </aside>
      )}

      {/* K 按钮:展开态放在 sidebar 外层,避免 backdrop-filter 改变 fixed 定位基准。 */}
      {!collapsed && (
        <KButton
          collapsed={false}
          variant="inline"
          onClick={() => setSidebar(true)}
        />
      )}

      {/* 收起态下的 hover 浮窗:从 K 按钮位置向左滑出 */}
      {showFlyout && (
        <aside
          data-no-window-drag
          onMouseEnter={() => setHoverOpen(true)}
          onMouseLeave={() => setHoverOpen(false)}
          style={{
            left: "var(--window-pad)",
            top: "var(--window-pad)",
            bottom: "var(--window-pad)",
            height: "calc(100vh - (var(--window-pad) * 2))",
          }}
          className={[
            "fixed z-20 w-sidebar",
            "bg-[#EAF6FC]/82 backdrop-blur-glass border border-white/60 rounded-md",
            "shadow-[0_1px_2px_rgba(0,0,0,0.03),0_12px_36px_rgba(31,88,122,0.08)]",
            "flex flex-col",
            "animate-[flyout-in_180ms_ease-out]",
          ].join(" ")}
        >
          <div className="px-5 pt-[76px] pb-4 flex items-center gap-2.5">
            <button
              title={t("sidebar.newChat")}
              aria-label={t("sidebar.newChat")}
              className="w-8 h-8 rounded-full bg-ink-900 text-white hover:bg-ink-700 transition-colors shrink-0 flex items-center justify-center"
            >
              <Plus size={16} strokeWidth={2.25} />
            </button>
            <div className="flex-1 min-w-0">
              <div className="font-serif text-[15px] text-ink-900 leading-none">
                Codex Box
              </div>
              <div className="text-[11px] text-ink-500 mt-1.5 flex items-center gap-1.5">
                <span className="w-1.5 h-1.5 rounded-full bg-status-ok" />
                {t("sidebar.connectedVersion", { version: "v0.0.1" })}
              </div>
            </div>
          </div>

          <NavList closeFlyout={() => setHoverOpen(false)} />

          <div className="mt-auto p-3">
            <div className="flex items-center gap-2.5">
              <button
                title={t("sidebar.proactiveMode")}
                className="relative w-7 h-7 rounded-full bg-gradient-to-br from-ink-700 to-ink-900 flex items-center justify-center text-white text-[9px] font-semibold shrink-0"
              >
                CB
                <span className="absolute -bottom-0.5 -right-0.5 w-2 h-2 rounded-full bg-status-ok border-2 border-white" />
              </button>
              <div className="flex-1 px-2 py-1 rounded-md bg-white/45 text-[11px] text-ink-700 flex items-center justify-center gap-1.5 truncate">
                <span className="w-1.5 h-1.5 rounded-full bg-status-ok shrink-0" />
                <span className="truncate">{t("sidebar.proactiveMode")}</span>
              </div>
            </div>
          </div>
        </aside>
      )}

      {/* K 按钮:收起态浮窗模式 — fixed 在窗口红绿灯右侧,保持全局可达 */}
      {!preview && collapsed && (
        <KButton
          collapsed
          variant="floating"
          onClick={() => setSidebar(false)}
          onHoverStart={() => setHoverOpen(true)}
          onHoverEnd={() => setHoverOpen(false)}
        />
      )}
    </>
  );
}
