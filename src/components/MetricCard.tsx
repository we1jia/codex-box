import type { ReactNode } from "react";

interface Props {
  label: string;
  value: ReactNode;
  sub?: ReactNode;
  icon?: ReactNode;
  iconColor?: string;
}

export function MetricCard({ label, value, sub, icon, iconColor = "#34C759" }: Props) {
  return (
    <div className="rounded-md bg-white/[0.86] border border-white/60 shadow-card p-5 flex items-start justify-between gap-3">
      <div className="flex-1 min-w-0">
        <div className="text-[11px] tracking-[0.05em] text-ink-500 font-medium">{label}</div>
        <div className="mt-2 text-2xl font-semibold text-ink-900 truncate">{value}</div>
        {sub && <div className="mt-1 text-xs text-ink-500">{sub}</div>}
      </div>
      {icon && (
        <div
          className="w-9 h-9 rounded-md flex items-center justify-center text-white shrink-0"
          style={{ background: iconColor }}
        >
          {icon}
        </div>
      )}
    </div>
  );
}
