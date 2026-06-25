// src/store/ui.ts
// UI 偏好：侧边栏收起/展开、宽度、语言等。persist 到 localStorage 跨刷新保留。
import { create } from "zustand";
import { persist, createJSONStorage } from "zustand/middleware";

/** 侧边栏宽度边界（Finder 风格区间） */
export const SIDEBAR_WIDTH_MIN = 240;
export const SIDEBAR_WIDTH_MAX = 360;
export const SIDEBAR_WIDTH_DEFAULT = 260;

/** 将任意值限制在合法区间内 */
export const clampSidebarWidth = (w: number) =>
  Math.min(SIDEBAR_WIDTH_MAX, Math.max(SIDEBAR_WIDTH_MIN, Math.round(w)));

export type PageId =
  | "dashboard"
  | "gateway"
  | "mobile_access"
  | "codex_runtime"
  | "profiles"
  | "providers"
  | "diagnostics"
  | "settings";

export const PAGE_IDS: PageId[] = [
  "dashboard",
  "gateway",
  "mobile_access",
  "codex_runtime",
  "profiles",
  "providers",
  "diagnostics",
  "settings",
];

interface UIState {
  /** 当前页面 */
  activePage: PageId;
  /** 侧边栏是否收起（图标列态） */
  sidebarCollapsed: boolean;
  /** 侧边栏宽度（px），持久化跨刷新 */
  sidebarWidth: number;
  /** 切换页面 */
  setActivePage: (page: PageId) => void;
  /** 切换侧边栏展开/收起 */
  toggleSidebar: () => void;
  /** 显式设置侧边栏收起状态 */
  setSidebar: (collapsed: boolean) => void;
  /** 设置侧边栏宽度（自动 clamp 到 [min, max]） */
  setSidebarWidth: (w: number) => void;
}

export const useUIStore = create<UIState>()(
  persist(
    (set, get) => ({
      activePage: "dashboard",
      sidebarCollapsed: false,
      sidebarWidth: SIDEBAR_WIDTH_DEFAULT,
      setActivePage: (page) => set({ activePage: page }),
      toggleSidebar: () => set({ sidebarCollapsed: !get().sidebarCollapsed }),
      setSidebar: (v) => set({ sidebarCollapsed: v }),
      setSidebarWidth: (w) => set({ sidebarWidth: clampSidebarWidth(w) }),
    }),
    {
      name: "codex-box.ui",
      storage: createJSONStorage(() => localStorage),
      partialize: (s) => ({
        activePage: s.activePage,
        sidebarCollapsed: s.sidebarCollapsed,
        sidebarWidth: s.sidebarWidth,
      }),
      // 防止历史数据中残留的非法宽度把 store 弄坏
      onRehydrateStorage: () => (state) => {
        if (!state) return;
        if (!PAGE_IDS.includes(state.activePage)) {
          state.activePage = "dashboard";
        }
        if (typeof state.sidebarWidth !== "number") {
          state.sidebarWidth = SIDEBAR_WIDTH_DEFAULT;
        } else {
          state.sidebarWidth = clampSidebarWidth(state.sidebarWidth);
        }
      },
    }
  )
);
