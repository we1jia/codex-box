import { create } from "zustand";
import { invokeCmd } from "@/lib/api";
import type { DashboardSummary } from "@/lib/types";

interface DashboardState {
  data: DashboardSummary | null;
  loading: boolean;
  error: string | null;
  load: () => Promise<void>;
}

export const useDashboardStore = create<DashboardState>((set) => ({
  data: null,
  loading: false,
  error: null,
  load: async () => {
    set({ loading: true, error: null });
    const result = await invokeCmd<DashboardSummary>("dashboard_summary");
    if (result.ok) {
      set({ data: result.data, loading: false });
    } else {
      set({ error: result.error, loading: false });
    }
  },
}));
