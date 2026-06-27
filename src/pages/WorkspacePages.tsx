import {
  useCallback,
  useEffect,
  useMemo,
  useState,
  type ReactNode,
} from "react";
import { useTranslation } from "react-i18next";
import {
  Activity,
  AlertTriangle,
  CheckCircle2,
  ChevronRight,
  Copy,
  Cpu,
  Eye,
  EyeOff,
  FileText,
  FolderOpen,
  GitCompare,
  Info,
  KeyRound,
  Languages,
  MousePointer2,
  Network,
  Play,
  Plus,
  Puzzle,
  RefreshCw,
  RotateCcw,
  Route,
  Save,
  ScrollText,
  Server,
  Settings,
  ShieldCheck,
  Sparkles,
  Square,
  TestTube2,
  Trash2,
  Users,
} from "lucide-react";
import { invokeCmd } from "@/lib/api";
import { setLanguage } from "@/lib/i18n";
import { useUIStore } from "@/store/ui";
import {
  mockSettingsSections,
} from "@/lib/mock-data";
import type {
  ApplyConfigChangeResultView,
  ApplyInjectResult,
  ApplyRestoreResult,
  CodexDesktopIntegrationStatus,
  CodexMultirouterPreview,
  CodexMultirouterSyncResult,
  CodexRuntimeStatus,
  ConfigChangePreviewView,
  ConfigChangeRequest,
  ConfigSnapshotView,
  ApplyConversationProviderResult,
  ConversationProviderCandidate,
  ConversationProviderCandidatesView,
  ConversationProviderPreview,
  CodexHistoryReconcileView,
  CodexHistoryUnifyApplyResult,
  CodexHistoryUnifyPreview,
  CodexModelsCacheRestorePreview,
  CodexModelsCacheRestoreResult,
  CodexPickerUnlockResult,
  ApplyConfigImportResult,
  ConfigImportPreview,
  ConfigImportSource,
  DiagnosticGroupView,
  DiffLineView,
  EffectiveRoutingStatus,
  InjectBaseUrlPreview,
  ModelCatalogEntry,
  OpenCodexCustomConfig,
  OpenCodexDeleteRequest,
  OpenCodexWriteRequest,
  OpenCodexWriteResult,
  ProfileView,
  ProviderRoute,
  ProviderView,
  ProxyModelsPreview,
  ProxyRouteTestResult,
  ProxyRuntimeLogs,
  ProxySessionsView,
  ProxyStatusView,
  RestoreBaseUrlPreview,
  SimpleModelConfigResult,
  StatusTone,
} from "@/lib/types";

const DEFAULT_MULTIROUTER_PROVIDER_ID = "codex_model_router_v2";
const COMPAT_MULTIROUTER_PROVIDER_IDS = new Set([
  DEFAULT_MULTIROUTER_PROVIDER_ID,
  "codex_local_access",
  "cc_switch_codex_router",
  "codex_model_router",
]);

type NoticeTone = "info" | "success" | "warning";

interface Notice {
  tone: NoticeTone;
  message: string;
}

function PageShell({
  title,
  subtitle,
  action,
  notice,
  children,
}: {
  title: string;
  subtitle: string;
  action?: ReactNode;
  notice?: Notice | null;
  children: ReactNode;
}) {
  return (
    <div className="h-full overflow-y-auto cb-scroll pr-1">
      <div className="min-h-full flex flex-col gap-4 pb-6">
        <section className="cb-surface p-5">
          <div className="flex items-start justify-between gap-4">
            <div className="min-w-0">
              <h1 className="cb-page-title">{title}</h1>
              <p className="cb-page-subtitle">{subtitle}</p>
            </div>
            {action}
          </div>
          {notice && <NoticeBar notice={notice} />}
        </section>
        {children}
      </div>
    </div>
  );
}

function NoticeBar({ notice }: { notice: Notice }) {
  const toneClass =
    notice.tone === "success"
      ? "border-status-ok/20 bg-status-ok/10 text-status-ok"
      : notice.tone === "warning"
        ? "border-status-warn/25 bg-status-warn/10 text-status-warn"
        : "border-[#0A84FF]/15 bg-[#0A84FF]/10 text-[#0A4F9E]";

  return (
    <div
      className={`mt-4 flex items-center gap-2 rounded-md border px-3 py-2 text-xs ${toneClass}`}
    >
      <CheckCircle2 size={14} />
      <span className="min-w-0 flex-1 leading-[1.55]">{notice.message}</span>
    </div>
  );
}

function Panel({
  title,
  icon,
  action,
  help,
  helpSide,
  children,
}: {
  title: string;
  icon: ReactNode;
  action?: ReactNode;
  help?: string;
  helpSide?: "auto" | "top" | "bottom";
  children: ReactNode;
}) {
  return (
    <section className="cb-surface p-4">
      <div className="mb-3 flex items-center justify-between gap-3">
        <div className="flex min-w-0 items-center gap-2">
          <span className="flex h-7 w-7 shrink-0 items-center justify-center rounded-md bg-ink-900/5 text-ink-700">
            {icon}
          </span>
          <h2 className="cb-section-title truncate">{title}</h2>
          {help && (
            <InfoTip label={title} content={help} side={helpSide ?? "auto"} />
          )}
        </div>
        {action}
      </div>
      {children}
    </section>
  );
}

function StatusPill({ tone }: { tone: StatusTone }) {
  const { t } = useTranslation();
  const cls =
    tone === "ok"
      ? "bg-status-ok/10 text-status-ok"
      : tone === "warn"
        ? "bg-status-warn/10 text-status-warn"
        : tone === "fail"
          ? "bg-status-fail/10 text-status-fail"
          : tone === "running"
            ? "bg-[#0A84FF]/10 text-[#0A84FF]"
            : "bg-ink-900/5 text-ink-500";

  return (
    <span
      className={`inline-flex h-5 shrink-0 items-center whitespace-nowrap rounded px-2 text-[11px] font-medium ${cls}`}
    >
      {t(`status.${tone}`)}
    </span>
  );
}

function DetailRow({ label, value }: { label: string; value: ReactNode }) {
  return (
    <div className="grid grid-cols-[128px_minmax(0,1fr)] items-start gap-3 border-t border-ink-900/[0.06] py-2.5">
      <div className="cb-label pt-0.5">{label}</div>
      <div className="min-w-0 text-[13px] leading-[1.55] text-ink-800">
        {value}
      </div>
    </div>
  );
}

function PathValue({ value }: { value: string }) {
  return (
    <span
      title={value}
      className="block min-w-0 max-w-full truncate font-mono text-[12px] text-ink-800"
    >
      {value}
    </span>
  );
}

function compactPath(value: string, maxLength = 68) {
  if (value.length <= maxLength) {
    return value;
  }

  const parts = value.split("/");
  const fileName = parts.at(-1) || "";
  const first = parts[1] ? `/${parts[1]}` : parts[0] || "";
  const tail = fileName.length > 28 ? fileName.slice(-28) : fileName;

  if (!first || !tail || first === tail) {
    return `${value.slice(0, 28)}...${value.slice(-28)}`;
  }

  return `${first}/.../${tail}`;
}

function PathBlock({ value }: { value: string }) {
  const displayValue = compactPath(value);

  return (
    <div
      title={value}
      className="min-w-0 rounded-md border border-ink-900/[0.06] bg-white/50 px-3 py-2 shadow-inner shadow-white/35"
    >
      <span className="block min-w-0 truncate font-mono text-[12px] leading-[1.55] text-ink-800">
        {displayValue}
      </span>
    </div>
  );
}

function InfoTip({
  label,
  content,
  align = "auto",
  side = "auto",
}: {
  label: string;
  content: string;
  align?: "auto" | "left" | "right";
  side?: "auto" | "top" | "bottom";
}) {
  // 默认向上弹出（避免被下方相邻卡片覆盖）；side="bottom" 显式向下
  const placementClass =
    side === "bottom" ? "top-full mt-2" : "bottom-full mb-2";
  // 对齐策略：auto 模式用 JS 智能判断，left/right 显式指定
  const positionClass =
    align === "right"
      ? "right-0"
      : align === "left"
        ? "left-0"
        : "info-tip-auto";

  const handleShow = (event: React.SyntheticEvent<HTMLSpanElement>) => {
    if (align !== "auto") return;
    const wrap = event.currentTarget;
    const btn = wrap.querySelector("button");
    const tip = wrap.querySelector('[role="tooltip"]');
    if (!btn || !tip) return;
    const wrapRect = wrap.getBoundingClientRect();
    const tipWidth = tip.getBoundingClientRect().width || 288; // w-72 = 18rem
    // 找最近的滚动容器（main 或 overflow-y-auto 父级）
    const scroller = wrap.closest("main, .h-full.overflow-y-auto") as HTMLElement | null;
    const bounds = scroller
      ? scroller.getBoundingClientRect()
      : { left: 0, right: window.innerWidth };
    // 计算 tooltip 实际位置（用当前 placement：默认 left:0）
    const tipLeftIfLeft = wrapRect.left;
    const tipRightIfLeft = wrapRect.left + tipWidth;
    // 如果向左放不下（或贴 main 右边缘太近），则改为 right 对齐
    const overflowLeft = tipLeftIfLeft < bounds.left;
    const tooCloseToRight = tipRightIfLeft > bounds.right - 8;
    if (overflowLeft || tooCloseToRight) {
      wrap.classList.add("info-tip-flip");
    } else {
      wrap.classList.remove("info-tip-flip");
    }
  };
  const handleHide = (event: React.SyntheticEvent<HTMLSpanElement>) => {
    event.currentTarget.classList.remove("info-tip-flip");
  };

  return (
    <span
      className="group relative inline-flex shrink-0 align-middle"
      onMouseEnter={handleShow}
      onFocus={handleShow}
      onMouseLeave={handleHide}
      onBlur={handleHide}
    >
      <button
        type="button"
        aria-label={label}
        title={content}
        className="inline-flex h-5 w-5 items-center justify-center rounded-full border border-ink-900/[0.08] bg-white/55 text-ink-400 transition hover:border-[#0A84FF]/25 hover:bg-[#0A84FF]/10 hover:text-[#0A84FF] focus:outline-none focus:ring-2 focus:ring-[#0A84FF]/25"
      >
        <Info size={12} />
      </button>
      <span
        role="tooltip"
        data-side={side}
        data-default-placement={side === "bottom" ? "bottom" : "top"}
        data-align={align}
        className={`pointer-events-none absolute z-50 hidden w-72 rounded-md border border-ink-900/[0.08] bg-white/95 px-3 py-2 text-[11px] font-normal leading-[1.6] text-ink-600 shadow-[0_12px_28px_rgba(0,0,0,0.10)] backdrop-blur-xl group-hover:block group-focus-within:block ${placementClass} ${positionClass}`}
      >
        {content}
      </span>
    </span>
  );
}

function ToolbarButton({
  icon,
  children,
  onClick,
  variant = "secondary",
  disabled,
  type = "button",
}: {
  icon?: ReactNode;
  children: ReactNode;
  onClick?: () => void;
  variant?: "primary" | "secondary" | "danger";
  disabled?: boolean;
  type?: "button" | "submit";
}) {
  const cls =
    variant === "primary"
      ? "cb-button-primary"
      : variant === "danger"
        ? "cb-button-danger"
        : "cb-button-secondary";

  return (
    <button type={type} className={cls} disabled={disabled} onClick={onClick}>
      {icon}
      <span className="whitespace-nowrap">{children}</span>
    </button>
  );
}

function ConfirmButton({
  idleLabel,
  confirmLabel,
  onConfirm,
  disabled,
}: {
  idleLabel: string;
  confirmLabel: string;
  onConfirm: () => void;
  disabled?: boolean;
}) {
  const [armed, setArmed] = useState(false);
  return (
    <ToolbarButton
      variant="danger"
      icon={<Trash2 size={13} />}
      disabled={disabled}
      onClick={() => {
        if (!armed) {
          setArmed(true);
          return;
        }
        setArmed(false);
        onConfirm();
      }}
    >
      {armed ? confirmLabel : idleLabel}
    </ToolbarButton>
  );
}

function ListButton({
  active,
  title,
  subtitle,
  right,
  onClick,
}: {
  active: boolean;
  title: string;
  subtitle: string;
  right?: ReactNode;
  onClick: () => void;
}) {
  return (
    <button
      className={`cb-list-item ${active ? "cb-list-item-active" : "cb-list-item-idle"}`}
      onClick={onClick}
    >
      <div className="flex items-center gap-2">
        <div className="min-w-0 flex-1">
          <div className="truncate text-[13px] font-medium">{title}</div>
          <div className="mt-0.5 truncate text-[11px] text-ink-500">
            {subtitle}
          </div>
        </div>
        {right}
        <ChevronRight size={14} className="shrink-0 text-ink-400" />
      </div>
    </button>
  );
}

function SummaryStrip({
  items,
}: {
  items: Array<{ label: string; value: string; tone?: StatusTone }>;
}) {
  const gridClass =
    items.length >= 4
      ? "grid grid-cols-1 gap-3 sm:grid-cols-2 xl:grid-cols-4"
      : "grid grid-cols-1 gap-3 sm:grid-cols-3";

  return (
    <div className={gridClass}>
      {items.map((item) => (
        <div key={item.label} className="cb-panel px-3 py-2.5">
          <div className="cb-label">{item.label}</div>
          <div className="mt-1 flex min-w-0 items-center justify-between gap-2">
            <div className="truncate text-[17px] font-semibold leading-tight text-ink-900">
              {item.value}
            </div>
            {item.tone && <StatusPill tone={item.tone} />}
          </div>
        </div>
      ))}
    </div>
  );
}

function SecretText({ value }: { value: string }) {
  const masked = looksLikeSecret(value)
    ? "redacted"
    : value || "not configured";
  return (
    <span className="inline-flex items-center gap-1.5 rounded bg-ink-900/[0.04] px-2 py-1 font-mono text-[12px] text-ink-700">
      <KeyRound size={12} />
      {masked}
    </span>
  );
}

function useNotice() {
  const [notice, setNotice] = useState<Notice | null>(null);
  // 必须稳定引用,否则下游 useCallback/useEffect 会死循环
  const show = useCallback(
    (tone: NoticeTone, message: string) => setNotice({ tone, message }),
    [],
  );
  return useMemo(() => ({ notice, show }), [notice, show]);
}

type ProviderDraft = {
  name: string;
  kind: ProviderView["kind"];
  baseUrl: string;
  wireApi: ProviderView["wireApi"];
  envKey: string;
  models: string;
};

type PendingProviderChange = {
  change: ConfigChangeRequest;
  preview: ConfigChangePreviewView;
  providerName: string;
};

type ConversationProviderDraft = {
  providerId: string;
  displayName: string;
  proxyPort: number;
  wireApi: string;
  requiresOpenaiAuth: boolean;
  originalBaseUrl: string;
};

type ProfileDraft = {
  name: string;
  model: string;
  providerId: string;
  sandbox: string;
  approval: string;
  network: string;
  mcpRefs: string;
};

const EMPTY_PROFILE_DRAFT: ProfileDraft = {
  name: "",
  model: "",
  providerId: "codex-subscription",
  sandbox: "workspace-write",
  approval: "on-request",
  network: "direct",
  mcpRefs: "",
};

type PendingProfileChange = {
  change: ConfigChangeRequest;
  preview: ConfigChangePreviewView;
  successMessage: string;
  selectId?: string;
};

const EMPTY_PROVIDER_DRAFT: ProviderDraft = {
  name: "",
  kind: "compatible_api",
  baseUrl: "https://",
  wireApi: "responses",
  envKey: "",
  models: "",
};

function providerIdFromName(name: string) {
  const normalized = name
    .trim()
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, "-")
    .replace(/^-+|-+$/g, "");
  return normalized || `provider-${Date.now()}`;
}

function looksLikeSecret(value: string) {
  return /(sk-|bearer\s+|xox[baprs]-|AIza|ghp_)/i.test(value);
}

function FormField({
  label,
  children,
}: {
  label: string;
  children: ReactNode;
}) {
  return (
    <label className="flex min-w-0 flex-col gap-1.5">
      <span className="cb-label">{label}</span>
      {children}
    </label>
  );
}

const inputClass =
  "h-9 rounded-md border border-ink-900/[0.08] bg-white/70 px-3 text-[13px] text-ink-800 outline-none transition-colors placeholder:text-ink-300 focus:border-[#0A84FF]/35 focus:bg-white";

function ProviderCreatePanel({
  draft,
  setDraft,
  onCancel,
  onCreate,
}: {
  draft: ProviderDraft;
  setDraft: (draft: ProviderDraft) => void;
  onCancel: () => void;
  onCreate: () => void;
}) {
  const { t } = useTranslation();

  return (
    <Panel title={t("pages.providers.createTitle")} icon={<Plus size={15} />}>
      <form
        className="grid grid-cols-2 gap-3"
        onSubmit={(event) => {
          event.preventDefault();
          onCreate();
        }}
      >
        <FormField label={t("fields.name")}>
          <input
            className={inputClass}
            value={draft.name}
            placeholder={t("placeholders.providerName")}
            onChange={(event) =>
              setDraft({ ...draft, name: event.target.value })
            }
          />
        </FormField>
        <FormField label={t("fields.kind")}>
          <select
            className={inputClass}
            value={draft.kind}
            onChange={(event) =>
              setDraft({
                ...draft,
                kind: event.target.value as ProviderView["kind"],
              })
            }
          >
            {(
              [
                "official_api",
                "compatible_api",
                "local_gateway",
                "subscription",
              ] as const
            ).map((kind) => (
              <option key={kind} value={kind}>
                {t(`providerKind.${kind}`)}
              </option>
            ))}
          </select>
        </FormField>
        <FormField label="base_url">
          <input
            className={inputClass}
            value={draft.baseUrl}
            placeholder="https://api.example.com/v1"
            onChange={(event) =>
              setDraft({ ...draft, baseUrl: event.target.value })
            }
          />
        </FormField>
        <FormField label="wire_api">
          <select
            className={inputClass}
            value={draft.wireApi}
            onChange={(event) =>
              setDraft({
                ...draft,
                wireApi: event.target.value as ProviderView["wireApi"],
              })
            }
          >
            {(["chat", "responses", "sse_stream", "custom"] as const).map(
              (api) => (
                <option key={api} value={api}>
                  {api}
                </option>
              ),
            )}
          </select>
        </FormField>
        <FormField label={t("fields.envKey")}>
          <input
            className={inputClass}
            value={draft.envKey}
            placeholder="OPENROUTER_API_KEY"
            onChange={(event) =>
              setDraft({ ...draft, envKey: event.target.value })
            }
          />
        </FormField>
        <FormField label={t("fields.models")}>
          <input
            className={inputClass}
            value={draft.models}
            placeholder="gpt-4.1, claude-sonnet"
            onChange={(event) =>
              setDraft({ ...draft, models: event.target.value })
            }
          />
        </FormField>
        <div className="col-span-2 flex items-center justify-between gap-3 border-t border-ink-900/[0.06] pt-3">
          <p className="cb-muted">{t("pages.providers.createHint")}</p>
          <div className="flex shrink-0 gap-2">
            <ToolbarButton onClick={onCancel}>
              {t("actions.cancel")}
            </ToolbarButton>
            <ToolbarButton
              icon={<Plus size={13} />}
              variant="primary"
              type="submit"
            >
              {t("actions.create")}
            </ToolbarButton>
          </div>
        </div>
      </form>
    </Panel>
  );
}

function ProfileCreatePanel({
  draft,
  providers,
  setDraft,
  onCancel,
  onCreate,
}: {
  draft: ProfileDraft;
  providers: ProviderView[];
  setDraft: (draft: ProfileDraft) => void;
  onCancel: () => void;
  onCreate: () => void;
}) {
  const { t } = useTranslation();

  return (
    <Panel title={t("pages.profiles.createTitle")} icon={<Plus size={15} />}>
      <form
        className="grid grid-cols-2 gap-3"
        onSubmit={(event) => {
          event.preventDefault();
          onCreate();
        }}
      >
        <FormField label={t("fields.name")}>
          <input
            className={inputClass}
            value={draft.name}
            placeholder="openrouter-dev"
            onChange={(event) =>
              setDraft({ ...draft, name: event.target.value })
            }
          />
        </FormField>
        <FormField label="model">
          <input
            className={inputClass}
            value={draft.model}
            placeholder="openai/gpt-5-mini"
            onChange={(event) =>
              setDraft({ ...draft, model: event.target.value })
            }
          />
        </FormField>
        <FormField label="model_provider">
          <select
            className={inputClass}
            value={draft.providerId}
            onChange={(event) =>
              setDraft({ ...draft, providerId: event.target.value })
            }
          >
            {providers.length > 0 ? (
              providers.map((provider) => (
                <option key={provider.id} value={provider.id}>
                  {provider.name}
                </option>
              ))
            ) : (
              <option value="">{t("common.none")}</option>
            )}
          </select>
        </FormField>
        <FormField label="sandbox_mode">
          <select
            className={inputClass}
            value={draft.sandbox}
            onChange={(event) =>
              setDraft({ ...draft, sandbox: event.target.value })
            }
          >
            {["read-only", "workspace-write", "danger-full-access"].map(
              (value) => (
                <option key={value} value={value}>
                  {value}
                </option>
              ),
            )}
          </select>
        </FormField>
        <FormField label="approval_policy">
          <select
            className={inputClass}
            value={draft.approval}
            onChange={(event) =>
              setDraft({ ...draft, approval: event.target.value })
            }
          >
            {["on-request", "on-failure", "never"].map((value) => (
              <option key={value} value={value}>
                {value}
              </option>
            ))}
          </select>
        </FormField>
        <FormField label="network">
          <input
            className={inputClass}
            value={draft.network}
            placeholder="direct"
            onChange={(event) =>
              setDraft({ ...draft, network: event.target.value })
            }
          />
        </FormField>
        <FormField label="mcp_refs">
          <input
            className={inputClass}
            value={draft.mcpRefs}
            placeholder="filesystem, git"
            onChange={(event) =>
              setDraft({ ...draft, mcpRefs: event.target.value })
            }
          />
        </FormField>
        <div className="col-span-2 flex items-center justify-between gap-3 border-t border-ink-900/[0.06] pt-3">
          <p className="cb-muted">{t("pages.profiles.createHint")}</p>
          <div className="flex shrink-0 gap-2">
            <ToolbarButton onClick={onCancel}>
              {t("actions.cancel")}
            </ToolbarButton>
            <ToolbarButton
              icon={<Plus size={13} />}
              variant="primary"
              type="submit"
            >
              {t("actions.create")}
            </ToolbarButton>
          </div>
        </div>
      </form>
    </Panel>
  );
}

type RenderableDiffLine = {
  id?: string;
  kind: DiffLineView["kind"];
  content: string;
  oldLine?: number | null;
  newLine?: number | null;
};

function DiffLine({ line }: { line: RenderableDiffLine }) {
  const prefix =
    line.kind === "insert"
      ? "+"
      : line.kind === "delete"
        ? "-"
        : line.kind === "change"
          ? "~"
          : " ";
  const cls =
    line.kind === "insert"
      ? "border-status-ok/20 bg-status-ok/[0.08] text-[#116329]"
      : line.kind === "delete"
        ? "border-status-fail/20 bg-status-fail/[0.08] text-[#A8271D]"
        : line.kind === "change"
          ? "border-status-warn/25 bg-status-warn/[0.10] text-[#7A4C00]"
          : "border-transparent text-ink-500";
  return (
    <div
      className={`grid min-h-[22px] grid-cols-[26px_minmax(0,1fr)] border-l px-2 py-0.5 ${cls}`}
    >
      <span className="select-none text-center text-ink-400">{prefix}</span>
      <span className="whitespace-pre-wrap break-words">{line.content}</span>
    </div>
  );
}

function DiffBlock({ lines }: { lines: RenderableDiffLine[] }) {
  return (
    <div className="max-h-[320px] overflow-auto rounded-md border border-ink-900/[0.08] bg-white/55 p-2 font-mono text-[11px] leading-[1.55] shadow-inner cb-scroll">
      {lines.map((line, index) => (
        <DiffLine
          key={`${index}-${line.kind}-${line.oldLine ?? "x"}-${line.newLine ?? "x"}`}
          line={line}
        />
      ))}
    </div>
  );
}

export function ProfilesPage() {
  const { t } = useTranslation();
  const { notice, show } = useNotice();
  const [profiles, setProfiles] = useState<ProfileView[]>([]);
  const [providers, setProviders] = useState<ProviderView[]>([]);
  const [configPath, setConfigPath] = useState("~/.codex/config.toml");
  const [creating, setCreating] = useState(false);
  const [draft, setDraft] = useState<ProfileDraft>(EMPTY_PROFILE_DRAFT);
  const [pendingChange, setPendingChange] =
    useState<PendingProfileChange | null>(null);
  const [writeBusy, setWriteBusy] = useState(false);
  const [selectedId, setSelectedId] = useState("");
  const selected =
    profiles.find((item) => item.id === selectedId) || profiles[0];

  const refreshConfigSnapshot = async (nextSelectedId?: string) => {
    const result = await invokeCmd<ConfigSnapshotView>("config_snapshot");
    if (result.ok) {
      const nextProfiles = result.data.profiles;
      const nextProviders = result.data.providers;
      setProfiles(nextProfiles);
      setProviders(nextProviders);
      setConfigPath(result.data.configPath);
      setSelectedId(
        nextSelectedId ||
          nextProfiles.find((profile) => profile.isActive)?.id ||
          nextProfiles[0]?.id ||
          "",
      );
      setDraft((current) => ({
        ...current,
        providerId: nextProviders[0]?.id || "",
      }));
    } else {
      show("warning", result.error);
    }
  };

  useEffect(() => {
    let cancelled = false;
    void invokeCmd<ConfigSnapshotView>("config_snapshot").then((result) => {
      if (cancelled) return;
      if (result.ok) {
        const nextProfiles = result.data.profiles;
        const nextProviders = result.data.providers;
        setProfiles(nextProfiles);
        setProviders(nextProviders);
        setConfigPath(result.data.configPath);
        setSelectedId(
          nextProfiles.find((profile) => profile.isActive)?.id ||
            nextProfiles[0]?.id ||
            "",
        );
        setDraft((current) => ({
          ...current,
          providerId: nextProviders[0]?.id || "",
        }));
      } else {
        show("warning", result.error);
      }
    });
    return () => {
      cancelled = true;
    };
  }, []);

  const createProfile = async () => {
    const trimmedName = draft.name.trim();
    const trimmedModel = draft.model.trim();
    const mcpRefs = draft.mcpRefs
      .split(",")
      .map((item) => item.trim())
      .filter(Boolean);

    if (!trimmedName || !trimmedModel || !draft.providerId) {
      show("warning", t("feedback.profileFormInvalid"));
      return;
    }
    if (profiles.some((profile) => profile.id === trimmedName)) {
      show("warning", t("feedback.profileExists"));
      return;
    }

    const change: ConfigChangeRequest = {
      type: "add_profile",
      name: trimmedName,
      model: trimmedModel,
      providerId: draft.providerId,
      sandbox: draft.sandbox,
      approval: draft.approval,
      network: draft.network,
      mcpRefs,
    };
    const result = await invokeCmd<ConfigChangePreviewView>(
      "config_change_preview",
      { change },
    );
    if (result.ok) {
      setPendingChange({
        change,
        preview: result.data,
        successMessage: t("feedback.profileCreated", { name: trimmedName }),
        selectId: trimmedName,
      });
      setCreating(false);
      show("info", t("feedback.previewDiff"));
    } else {
      show("warning", result.error);
    }
  };

  const previewSetActive = async () => {
    if (!selected || selected.isActive) return;
    const change: ConfigChangeRequest = {
      type: "set_active_profile",
      profileName: selected.name,
    };
    const result = await invokeCmd<ConfigChangePreviewView>(
      "config_change_preview",
      { change },
    );
    if (result.ok) {
      setPendingChange({
        change,
        preview: result.data,
        successMessage: t("feedback.profileActivated", { name: selected.name }),
        selectId: selected.id,
      });
      show("info", t("feedback.previewDiff"));
    } else {
      show("warning", result.error);
    }
  };

  const applyPendingProfile = async () => {
    if (!pendingChange) return;
    setWriteBusy(true);
    const result = await invokeCmd<ApplyConfigChangeResultView>(
      "config_change_apply",
      {
        request: {
          change: pendingChange.change,
          expectedHash: pendingChange.preview.expectedHash,
        },
      },
    );
    setWriteBusy(false);
    if (result.ok) {
      const selectId = pendingChange.selectId;
      const successMessage = pendingChange.successMessage;
      setDraft(EMPTY_PROFILE_DRAFT);
      setPendingChange(null);
      show("success", successMessage);
      await refreshConfigSnapshot(selectId);
    } else {
      show("warning", result.error);
    }
  };

  return (
    <PageShell
      title={t("pages.profiles.title")}
      subtitle={t("pages.profiles.subtitle")}
      notice={notice}
      action={
        <ToolbarButton
          icon={<Plus size={13} />}
          variant="primary"
          onClick={() => {
            setCreating(true);
            show("info", t("feedback.profileCreate"));
          }}
        >
          {t("actions.newProfile")}
        </ToolbarButton>
      }
    >
      {creating && (
        <ProfileCreatePanel
          draft={draft}
          providers={providers}
          setDraft={setDraft}
          onCancel={() => {
            setCreating(false);
            setDraft(EMPTY_PROFILE_DRAFT);
          }}
          onCreate={createProfile}
        />
      )}
      {pendingChange && (
        <Panel
          title={t("pages.profiles.writePreviewTitle")}
          icon={<GitCompare size={15} />}
        >
          <SummaryStrip
            items={[
              {
                label: t("fields.configPath"),
                value: pendingChange.preview.configPath,
              },
              {
                label: t("diff.insertions"),
                value: String(pendingChange.preview.insertions),
                tone: "ok",
              },
              {
                label: t("diff.deletions"),
                value: String(pendingChange.preview.deletions),
                tone: pendingChange.preview.deletions > 0 ? "warn" : "idle",
              },
            ]}
          />
          <div className="mt-4">
            <DiffBlock lines={pendingChange.preview.diff} />
          </div>
          <div className="mt-4 flex flex-wrap gap-2">
            <ToolbarButton
              icon={<Save size={13} />}
              variant="primary"
              disabled={writeBusy}
              onClick={() => void applyPendingProfile()}
            >
              {t("actions.confirmWrite")}
            </ToolbarButton>
            <ToolbarButton
              icon={<RotateCcw size={13} />}
              disabled={writeBusy}
              onClick={() => setPendingChange(null)}
            >
              {t("actions.cancel")}
            </ToolbarButton>
          </div>
        </Panel>
      )}
      <SummaryStrip
        items={[
          { label: t("summary.totalProfiles"), value: String(profiles.length) },
          {
            label: t("summary.activeProfile"),
            value: profiles.find((item) => item.isActive)?.name || "-",
          },
          {
            label: t("summary.safeWrites"),
            value: t("common.enabled"),
            tone: "ok",
          },
        ]}
      />
      <Panel
        title={t("pages.profiles.sourceTitle")}
        icon={<GitCompare size={15} />}
      >
        <DetailRow
          label={t("fields.configPath")}
          value={<span className="font-mono break-all">{configPath}</span>}
        />
      </Panel>
      <div className="grid grid-cols-[minmax(240px,0.82fr)_minmax(0,1.35fr)] gap-4">
        <Panel title={t("pages.profiles.listTitle")} icon={<Users size={15} />}>
          <div className="flex flex-col gap-2">
            {profiles.length > 0 ? (
              profiles.map((profile) => (
                <ListButton
                  key={profile.id}
                  active={profile.id === selected?.id}
                  title={profile.name}
                  subtitle={`${profile.model} / ${profile.providerId}`}
                  right={<StatusPill tone={profile.status} />}
                  onClick={() => setSelectedId(profile.id)}
                />
              ))
            ) : (
              <div className="rounded-md border border-dashed border-ink-900/[0.08] bg-white/35 px-4 py-6 text-center text-[12px] text-ink-500">
                {t("common.none")}
              </div>
            )}
          </div>
        </Panel>
        <Panel
          title={t("pages.profiles.detailTitle")}
          icon={<ShieldCheck size={15} />}
          action={<StatusPill tone={selected?.status || "idle"} />}
        >
          <DetailRow
            label="model"
            value={<span className="font-mono">{selected?.model || "-"}</span>}
          />
          <DetailRow
            label="model_provider"
            value={
              <span className="font-mono">{selected?.providerId || "-"}</span>
            }
          />
          <DetailRow label="sandbox" value={selected?.sandbox || "-"} />
          <DetailRow label="approval" value={selected?.approval || "-"} />
          <DetailRow label="network" value={selected?.network || "-"} />
          <DetailRow
            label="mcp_refs"
            value={selected?.mcpRefs.join(", ") || "-"}
          />
          <div className="mt-4 flex flex-wrap gap-2">
            <ToolbarButton
              icon={<CheckCircle2 size={13} />}
              disabled={!selected || selected.isActive}
              onClick={() => void previewSetActive()}
            >
              {t("actions.setActive")}
            </ToolbarButton>
            <ToolbarButton
              icon={<Copy size={13} />}
              onClick={() =>
                show(
                  "info",
                  t("feedback.profileCopied", { name: selected?.name || "-" }),
                )
              }
            >
              {t("actions.duplicate")}
            </ToolbarButton>
            <ConfirmButton
              idleLabel={t("actions.delete")}
              confirmLabel={t("actions.confirmDelete")}
              disabled={!selected || selected.isActive}
              onConfirm={() => show("warning", t("feedback.dangerBlocked"))}
            />
          </div>
        </Panel>
      </div>
      {selected?.mcpRefs?.length ? (
        <div className="cb-surface p-4">
          <div className="mb-2 flex items-center gap-2 text-[12px] font-medium text-ink-700">
            <Puzzle size={14} /> {t("pages.profiles.mcpSubsection")}
          </div>
          <div className="flex flex-wrap gap-2">
            {selected.mcpRefs.map((ref) => (
              <span
                key={ref}
                className="rounded-md border border-ink-900/10 bg-white/60 px-2 py-1 text-[11px] text-ink-700"
              >
                {ref}
              </span>
            ))}
          </div>
        </div>
      ) : null}
    </PageShell>
  );
}

export function ProvidersPage() {
  const { t } = useTranslation();
  const { notice, show } = useNotice();
  const [providers, setProviders] = useState<ProviderView[]>([]);
  const [selectedId, setSelectedId] = useState("");
  const [configPath, setConfigPath] = useState("~/.codex/config.toml");
  const [creating, setCreating] = useState(false);
  const [draft, setDraft] = useState<ProviderDraft>(EMPTY_PROVIDER_DRAFT);
  const [pendingChange, setPendingChange] =
    useState<PendingProviderChange | null>(null);
  const [writeBusy, setWriteBusy] = useState(false);
  const selected =
    providers.find((item) => item.id === selectedId) || providers[0];

  const refreshConfigSnapshot = async () => {
    const result = await invokeCmd<ConfigSnapshotView>("config_snapshot");
    if (result.ok) {
      const nextProviders = result.data.providers;
      setProviders(nextProviders);
      setConfigPath(result.data.configPath);
      setSelectedId(nextProviders[0]?.id || "");
    } else {
      show("warning", result.error);
    }
  };

  useEffect(() => {
    let cancelled = false;
    void invokeCmd<ConfigSnapshotView>("config_snapshot").then((result) => {
      if (cancelled) return;
      if (result.ok) {
        const nextProviders = result.data.providers;
        setProviders(nextProviders);
        setConfigPath(result.data.configPath);
        setSelectedId(nextProviders[0]?.id || "");
      } else {
        show("warning", result.error);
      }
    });
    return () => {
      cancelled = true;
    };
  }, []);

  const createProvider = async () => {
    const trimmedName = draft.name.trim();
    const trimmedBaseUrl = draft.baseUrl.trim();
    const trimmedEnvKey = draft.envKey.trim();
    const models = draft.models
      .split(",")
      .map((model) => model.trim())
      .filter(Boolean);

    if (
      !trimmedName ||
      !trimmedBaseUrl ||
      !trimmedEnvKey ||
      models.length === 0
    ) {
      show("warning", t("feedback.providerFormInvalid"));
      return;
    }

    if (looksLikeSecret(trimmedEnvKey)) {
      show("warning", t("feedback.providerSecretRejected"));
      return;
    }

    const baseId = providerIdFromName(trimmedName);
    const existingIds = new Set(providers.map((provider) => provider.id));
    let nextId = baseId;
    let suffix = 2;
    while (existingIds.has(nextId)) {
      nextId = `${baseId}-${suffix}`;
      suffix += 1;
    }

    const change: ConfigChangeRequest = {
      type: "add_provider",
      id: nextId,
      kind: draft.kind,
      baseUrl: trimmedBaseUrl,
      wireApi: draft.wireApi,
      envKey: trimmedEnvKey,
      models,
    };

    const result = await invokeCmd<ConfigChangePreviewView>(
      "config_change_preview",
      { change },
    );
    if (result.ok) {
      setPendingChange({
        change,
        preview: result.data,
        providerName: trimmedName,
      });
      setCreating(false);
      show("info", t("feedback.previewDiff"));
    } else {
      show("warning", result.error);
    }
  };

  const applyPendingProvider = async () => {
    if (!pendingChange) return;
    setWriteBusy(true);
    const result = await invokeCmd<ApplyConfigChangeResultView>(
      "config_change_apply",
      {
        request: {
          change: pendingChange.change,
          expectedHash: pendingChange.preview.expectedHash,
        },
      },
    );
    setWriteBusy(false);
    if (result.ok) {
      setDraft(EMPTY_PROVIDER_DRAFT);
      setPendingChange(null);
      show(
        "success",
        t("feedback.providerCreated", { name: pendingChange.providerName }),
      );
      await refreshConfigSnapshot();
      if ("id" in pendingChange.change) {
        setSelectedId(pendingChange.change.id);
      }
    } else {
      show("warning", result.error);
    }
  };

  return (
    <PageShell
      title={t("pages.providers.title")}
      subtitle={t("pages.providers.subtitle")}
      notice={notice}
      action={
        <ToolbarButton
          icon={<Plus size={13} />}
          variant="primary"
          onClick={() => {
            setCreating(true);
            show("info", t("feedback.providerCreate"));
          }}
        >
          {t("actions.addProvider")}
        </ToolbarButton>
      }
    >
      {creating && (
        <ProviderCreatePanel
          draft={draft}
          setDraft={setDraft}
          onCancel={() => {
            setCreating(false);
            setDraft(EMPTY_PROVIDER_DRAFT);
          }}
          onCreate={createProvider}
        />
      )}
      {pendingChange && (
        <Panel
          title={t("pages.providers.writePreviewTitle")}
          icon={<GitCompare size={15} />}
        >
          <SummaryStrip
            items={[
              {
                label: t("fields.configPath"),
                value: pendingChange.preview.configPath,
              },
              {
                label: t("diff.insertions"),
                value: String(pendingChange.preview.insertions),
                tone: "ok",
              },
              {
                label: t("diff.deletions"),
                value: String(pendingChange.preview.deletions),
                tone: pendingChange.preview.deletions > 0 ? "warn" : "idle",
              },
            ]}
          />
          <div className="mt-4">
            <DiffBlock lines={pendingChange.preview.diff} />
          </div>
          <div className="mt-4 flex flex-wrap gap-2">
            <ToolbarButton
              icon={<Save size={13} />}
              variant="primary"
              disabled={writeBusy}
              onClick={() => void applyPendingProvider()}
            >
              {t("actions.confirmWrite")}
            </ToolbarButton>
            <ToolbarButton
              icon={<RotateCcw size={13} />}
              disabled={writeBusy}
              onClick={() => setPendingChange(null)}
            >
              {t("actions.cancel")}
            </ToolbarButton>
          </div>
        </Panel>
      )}
      <Panel
        title={t("pages.providers.sourceTitle")}
        icon={<GitCompare size={15} />}
      >
        <DetailRow
          label={t("fields.configPath")}
          value={<span className="font-mono break-all">{configPath}</span>}
        />
      </Panel>
      <div className="grid grid-cols-[minmax(260px,0.88fr)_minmax(0,1.35fr)] gap-4">
        <Panel
          title={t("pages.providers.listTitle")}
          icon={<Server size={15} />}
        >
          <div className="flex flex-col gap-2">
            {providers.length > 0 ? (
              providers.map((provider) => (
                <ListButton
                  key={provider.id}
                  active={provider.id === selected?.id}
                  title={provider.name}
                  subtitle={`${t(`providerKind.${provider.kind}`)} / ${provider.wireApi}`}
                  right={<StatusPill tone={provider.status} />}
                  onClick={() => setSelectedId(provider.id)}
                />
              ))
            ) : (
              <div className="rounded-md border border-dashed border-ink-900/[0.08] bg-white/35 px-4 py-6 text-center text-[12px] text-ink-500">
                {t("common.none")}
              </div>
            )}
          </div>
        </Panel>
        <Panel
          title={t("pages.providers.detailTitle")}
          icon={<KeyRound size={15} />}
          action={
            <ToolbarButton
              icon={<TestTube2 size={13} />}
              disabled={!selected}
              onClick={() =>
                show(
                  "success",
                  t("feedback.connectionTested", {
                    name: selected?.name || "-",
                  }),
                )
              }
            >
              {t("actions.testConnection")}
            </ToolbarButton>
          }
        >
          <DetailRow
            label={t("fields.kind")}
            value={selected ? t(`providerKind.${selected.kind}`) : "-"}
          />
          <DetailRow
            label="base_url"
            value={
              <span className="font-mono break-all">
                {selected?.baseUrl || "-"}
              </span>
            }
          />
          <DetailRow
            label="wire_api"
            value={
              <span className="font-mono">{selected?.wireApi || "-"}</span>
            }
          />
          <DetailRow
            label="env"
            value={selected ? <SecretText value={selected.envKey} /> : "-"}
          />
          <DetailRow
            label={t("fields.models")}
            value={selected?.models.join(", ") || "-"}
          />
          <div className="mt-4 flex flex-wrap gap-2">
            <ToolbarButton
              icon={<Copy size={13} />}
              disabled={!selected}
              onClick={() =>
                show(
                  "info",
                  t("feedback.providerCopied", { name: selected?.name || "-" }),
                )
              }
            >
              {t("actions.copyConfig")}
            </ToolbarButton>
            <ToolbarButton
              icon={<GitCompare size={13} />}
              onClick={() => show("info", t("feedback.previewDiff"))}
            >
              {t("actions.previewDiff")}
            </ToolbarButton>
          </div>
        </Panel>
      </div>
      {selected ? (
        <div className="cb-surface p-4">
          <div className="mb-2 flex items-center gap-2 text-[12px] font-medium text-ink-700">
            <Network size={14} /> {t("pages.providers.networkSubsection")}
          </div>
          <div className="flex flex-wrap items-center gap-3 text-[11px] text-ink-600">
            <span>
              {t("fields.kind")}:{" "}
              <span className="font-mono">{selected.kind}</span>
            </span>
            <span>
              {t("fields.baseUrl")}:{" "}
              <span className="font-mono break-all">
                {selected.baseUrl || "-"}
              </span>
            </span>
            <span>
              wire_api: <span className="font-mono">{selected.wireApi}</span>
            </span>
          </div>
        </div>
      ) : null}
    </PageShell>
  );
}

/**
 * Models 页面:BYOK 模型下拉预览
 * - 读取 ~/.codex/codex-box/custom_model_catalog.json 真实条目
 * - 列出"已激活 profile"对应的模型
 * - 提供 toggle visibility(可见性)和 reasoning 配置入口
 * - 写入走 ~/.codex/codex-box/custom_model_catalog.json
 */
function isProtectedSubscriptionModel(entry: ModelCatalogEntry) {
  if (entry.provider.trim().toLowerCase() !== "openai") return false;
  const backendProvider = entry.backendProvider?.trim().toLowerCase();
  return !backendProvider || backendProvider === "openai";
}

const CODEX_BUILTIN_MODELS: ModelCatalogEntry[] = [
  {
    modelId: "gpt-5.5",
    displayName: "GPT-5.5",
    provider: "openai",
    backendModel: "gpt-5.5",
    backendProvider: "openai",
    visible: true,
    reasoning: null,
    note: "Codex 官方订阅默认模型，只读展示",
  },
  {
    modelId: "gpt-5.4",
    displayName: "GPT-5.4",
    provider: "openai",
    backendModel: "gpt-5.4",
    backendProvider: "openai",
    visible: true,
    reasoning: null,
    note: "Codex 官方订阅默认模型，只读展示",
  },
  {
    modelId: "gpt-5.4-mini",
    displayName: "GPT-5.4-Mini",
    provider: "openai",
    backendModel: "gpt-5.4-mini",
    backendProvider: "openai",
    visible: true,
    reasoning: null,
    note: "Codex 官方订阅默认模型，只读展示",
  },
  {
    modelId: "gpt-5.3-codex-spark",
    displayName: "GPT-5.3-Codex-Spark",
    provider: "openai",
    backendModel: "gpt-5.3-codex-spark",
    backendProvider: "openai",
    visible: true,
    reasoning: null,
    note: "Codex 官方订阅默认模型，只读展示",
  },
  {
    modelId: "codex-auto-review",
    displayName: "Codex Auto Review",
    provider: "openai",
    backendModel: "codex-auto-review",
    backendProvider: "openai",
    visible: true,
    reasoning: null,
    note: "Codex 官方订阅默认模型，只读展示",
  },
];

function mergeBuiltinAndCustomModels(customCatalog: ModelCatalogEntry[]) {
  const customIds = new Set(customCatalog.map((entry) => entry.modelId));
  return [
    ...CODEX_BUILTIN_MODELS.filter((entry) => !customIds.has(entry.modelId)),
    ...customCatalog,
  ];
}

function modelRouteLabel(entry: ModelCatalogEntry) {
  const provider = entry.provider.trim();
  const backendProvider = modelUpstreamProvider(entry);
  const providerKey = provider.toLowerCase();
  const displayProvider =
    (providerKey === "opencodex" ||
      COMPAT_MULTIROUTER_PROVIDER_IDS.has(providerKey)) &&
    backendProvider
      ? backendProvider
      : provider || backendProvider || "codex-box";

  return `${displayProvider} / ${entry.modelId}`;
}

function modelUpstreamProvider(entry: ModelCatalogEntry) {
  return (
    entry.targetProvider?.trim() ||
    entry.target_provider?.trim() ||
    entry.backendProvider?.trim() ||
    ""
  );
}

function formatPickerRendererReports(
  result: CodexPickerUnlockResult | null | undefined,
  empty = "-",
) {
  const reports = result?.rendererReports ?? [];
  if (!reports.length) return empty;
  return reports
    .slice(0, 3)
    .map((report) => {
      const target = report.targetId ? report.targetId.slice(0, 8) : "-";
      const modelCount = report.modelCount ?? report.availableModels.length;
      const errorCount = report.errorCount ?? 0;
      return `${report.status || "-"} @${report.port}/${target}: ${modelCount} models / ${errorCount} errors`;
    })
    .join(" | ");
}

export function ModelsPage() {
  const { t } = useTranslation();
  const { notice, show } = useNotice();
  const [catalog, setCatalog] = useState<ModelCatalogEntry[]>([]);
  const [config, setConfig] = useState<OpenCodexCustomConfig | null>(null);
  const [activeProfile, setActiveProfile] = useState<ProfileView | null>(null);
  const [routingStatus, setRoutingStatus] =
    useState<EffectiveRoutingStatus | null>(null);
  const [selectedId, setSelectedId] = useState<string>("");
  const [busy, setBusy] = useState(false);
  const [modelInput, setModelInput] = useState("");
  const [baseUrl, setBaseUrl] = useState("");
  const [apiKey, setApiKey] = useState("");
  const [simpleWireApi, setSimpleWireApi] =
    useState<ProviderView["wireApi"]>("responses");
  const restartCodex = true;
  const [showApiKey, setShowApiKey] = useState(false);
  const [visionEnabled, setVisionEnabled] = useState(false);
  const [visionBaseUrl, setVisionBaseUrl] = useState("");
  const [visionModel, setVisionModel] = useState("");
  const [visionEnvKey, setVisionEnvKey] = useState("");
  const [deleteCandidateId, setDeleteCandidateId] = useState("");
  const [showAdvancedModels, setShowAdvancedModels] = useState(false);
  const [routerPreview, setRouterPreview] =
    useState<CodexMultirouterPreview | null>(null);
  const [routerCacheRestorePreview, setRouterCacheRestorePreview] =
    useState<CodexModelsCacheRestorePreview | null>(null);
  const dropdownCatalog = useMemo(
    () => mergeBuiltinAndCustomModels(catalog),
    [catalog],
  );
  const selected = selectedId
    ? dropdownCatalog.find((item) => item.modelId === selectedId) || null
    : null;
  const deleteCandidate =
    catalog.find((item) => item.modelId === deleteCandidateId) || null;
  const catalogPath =
    config?.catalogPath || "~/.codex/codex-box/custom_model_catalog.json";
  const routerProvider = config?.providers.find(
    (provider) => (provider.codexRouting?.routes?.length ?? 0) > 0,
  );
  const routerRouteCount = routerProvider?.codexRouting?.routes?.length ?? 0;
  const routerModelCount =
    routerProvider?.codexRouting?.routes?.reduce(
      (total, route) => total + (route.match?.models?.length ?? 0),
      0,
    ) ?? 0;
  const routingCatalogPath = routingStatus?.modelCatalogPath || "";
  const codexCatalogConnected =
    Boolean(routingStatus?.catalogConfigured) &&
    routingCatalogPath.includes("codex-box/custom_model_catalog.json");
  const codexProviderConnected =
    routingStatus?.modelProvider != null &&
    COMPAT_MULTIROUTER_PROVIDER_IDS.has(
      routingStatus.modelProvider.toLowerCase(),
    );
  const codexAppConnected = codexCatalogConnected && codexProviderConnected;
  const codexAppStatusText = !routingStatus
    ? t("pages.models.codexLinkUnknown")
    : codexAppConnected
      ? t("pages.models.codexLinkReady")
      : t("pages.models.codexLinkNeedsSync");
  const codexAppStatusDetail = !routingStatus
    ? t("pages.models.codexLinkUnknownDetail")
    : !codexCatalogConnected
      ? t("pages.models.codexLinkMissingCatalog")
      : !codexProviderConnected
        ? t("pages.models.codexLinkWrongProvider", {
            provider: routingStatus.modelProvider || "-",
          })
        : t("pages.models.codexLinkReadyDetail", {
            model: routingStatus.currentModel || "-",
            provider: routingStatus.modelProvider || "-",
          });

  useEffect(() => {
    setVisionEnabled(Boolean(selected?.visionBridgeEnabled));
    setVisionBaseUrl(selected?.visionFallbackBaseUrl || "");
    setVisionModel(selected?.visionFallbackModel || "");
    setVisionEnvKey(selected?.visionFallbackApiKeyRef || "");
  }, [
    selected?.modelId,
    selected?.visionBridgeEnabled,
    selected?.visionFallbackBaseUrl,
    selected?.visionFallbackModel,
    selected?.visionFallbackApiKeyRef,
  ]);

  const refresh = useCallback(async () => {
    setBusy(true);
    const snapshot = await invokeCmd<ConfigSnapshotView>("config_snapshot");
    if (snapshot.ok) {
      const profile =
        snapshot.data.profiles.find((p) => p.isActive) ||
        snapshot.data.profiles[0] ||
        null;
      setActiveProfile(profile);
    } else {
      show("warning", snapshot.error);
    }
    const routing = await invokeCmd<EffectiveRoutingStatus>(
      "effective_routing_status",
    );
    setRoutingStatus(routing.ok ? routing.data : null);
    const opencodex = await invokeCmd<OpenCodexCustomConfig>(
      "opencodex_config_read",
    );
    setBusy(false);
    if (opencodex.ok) {
      setConfig(opencodex.data);
      if (opencodex.data.parseErrors.length > 0) {
        show("warning", opencodex.data.parseErrors[0].message);
      }
      // 明文 api_key 等 provider 问题不应让模型目录整页不可用;
      // catalog 能安全读取时继续展示,写入仍由后端校验和备份流程保护。
      const nextCatalog = opencodex.data.catalog;
      const nextDropdownCatalog = mergeBuiltinAndCustomModels(nextCatalog);
      if (nextCatalog.length > 0) {
        setCatalog(nextCatalog);
        setSelectedId((current) =>
          current &&
          nextDropdownCatalog.some((entry) => entry.modelId === current)
            ? current
            : "",
        );
      } else {
        setCatalog([]);
        setSelectedId((current) =>
          current &&
          nextDropdownCatalog.some((entry) => entry.modelId === current)
            ? current
            : "",
        );
        if (opencodex.data.parseErrors.length === 0) {
          show(
            "info",
            "自定义模型目录为空；当前仅展示 Codex 官方默认模型。",
          );
        }
      }
    } else {
      show("warning", opencodex.error);
    }
  }, [show]);

  useEffect(() => {
    void refresh();
  }, [refresh]);

  const toggleVisibility = async (entry: ModelCatalogEntry) => {
    if (!config) return;
    if (isProtectedSubscriptionModel(entry)) {
      show("warning", t("feedback.subscriptionModelDeleteBlocked"));
      return;
    }
    const next: ModelCatalogEntry = { ...entry, visible: !entry.visible };
    const request: OpenCodexWriteRequest<ModelCatalogEntry> = {
      entry: next,
      expectedHash: config.catalogContentHash,
      note: null,
    };
    const result = await invokeCmd<OpenCodexWriteResult>(
      "catalog_entry_upsert",
      { request },
    );
    if (result.ok) {
      show(
        "success",
        t("feedback.modelVisibilityToggled", {
          name: entry.displayName || entry.modelId,
        }),
      );
      await refresh();
    } else {
      show("warning", result.error);
    }
  };

  const saveSimpleModelConfig = async () => {
    if (!modelInput.trim() || !baseUrl.trim() || !apiKey.trim()) {
      show("warning", t("feedback.simpleModelFormInvalid"));
      return;
    }
    setBusy(true);
    const result = await invokeCmd<SimpleModelConfigResult>(
      "simple_model_config_save",
      {
        request: {
          modelInput,
          baseUrl,
          apiKey,
          wireApi: simpleWireApi,
          displayName: null,
          reasoningLevel: "medium",
          restartCodex,
        },
      },
    );
    setBusy(false);
    if (result.ok) {
      show(
        "success",
        t("feedback.simpleModelSaved", {
          name: result.data.model.displayName || result.data.model.modelId,
        }),
      );
      setModelInput("");
      setBaseUrl("");
      setApiKey("");
      setSimpleWireApi("responses");
      await refresh();
      if (result.data.requiresMultirouterSync) {
        await previewMultirouter();
      }
    } else {
      show("warning", result.error);
    }
  };

  const previewMultirouter = async () => {
    setBusy(true);
    const result = await invokeCmd<CodexMultirouterPreview>(
      "codex_multirouter_preview",
      {
        request: {
          proxyPort: 1455,
          routerProviderId: DEFAULT_MULTIROUTER_PROVIDER_ID,
          ensureCodexConfig: true,
        },
      },
    );
    setBusy(false);
    if (result.ok) {
      setRouterPreview(result.data);
      show("info", t("feedback.multirouterPreviewReady"));
    } else {
      show("warning", result.error);
    }
  };

  const applyMultirouterPreview = async () => {
    if (!routerPreview) return;
    setBusy(true);
    const result = await invokeCmd<CodexMultirouterSyncResult>(
      "codex_multirouter_apply",
      {
        request: {
          preview: routerPreview,
          confirmed: true,
        },
      },
    );
    setBusy(false);
    if (result.ok) {
      show(
        "success",
        t("feedback.multirouterSynced", {
          routes: result.data.routeCount,
          models: result.data.routedModelCount,
        }),
      );
      setRouterPreview(null);
      await refresh();
    } else {
      show("warning", result.error);
    }
  };

  const previewRestoreModelsCache = async () => {
    setBusy(true);
    const result = await invokeCmd<CodexModelsCacheRestorePreview>(
      "codex_models_cache_restore_preview",
      {},
    );
    setBusy(false);
    if (result.ok) {
      setRouterCacheRestorePreview(result.data);
      show(
        result.data.restoreAvailable ? "info" : "warning",
        result.data.restoreAvailable
          ? t("feedback.modelsCacheRestorePreviewReady")
          : t("feedback.modelsCacheRestoreUnavailable"),
      );
    } else {
      show("warning", result.error);
    }
  };

  const applyRestoreModelsCache = async () => {
    if (!routerCacheRestorePreview) return;
    setBusy(true);
    const result = await invokeCmd<CodexModelsCacheRestoreResult>(
      "codex_models_cache_restore_apply",
      {
        request: {
          preview: routerCacheRestorePreview,
          confirmed: true,
        },
      },
    );
    setBusy(false);
    if (result.ok) {
      show(
        result.data.restored ? "success" : "warning",
        result.data.restored
          ? t("feedback.modelsCacheRestored")
          : t("feedback.modelsCacheRestoreUnavailable"),
      );
      setRouterCacheRestorePreview(null);
      await refresh();
    } else {
      show("warning", result.error);
    }
  };

  const saveVisionFallback = async () => {
    if (!config || !selected) return;
    if (isProtectedSubscriptionModel(selected)) {
      show("warning", "官方默认模型由 Codex Desktop 管理，Codex Box 不写入该条目。");
      return;
    }
    const next: ModelCatalogEntry = {
      ...selected,
      visionBridgeEnabled: visionEnabled,
      visionFallbackBaseUrl: visionBaseUrl.trim() || null,
      visionFallbackModel: visionModel.trim() || null,
      visionFallbackApiKeyRef: visionEnvKey.trim() || null,
    };
    const result = await invokeCmd<OpenCodexWriteResult>(
      "catalog_entry_upsert",
      {
        request: {
          entry: next,
          expectedHash: config.catalogContentHash,
          note: "vision fallback config",
        },
      },
    );
    if (result.ok) {
      show("success", t("feedback.visionFallbackSaved"));
      await refresh();
    } else {
      show("warning", result.error);
    }
  };

  const requestDeleteModelEntry = (entry: ModelCatalogEntry) => {
    if (isProtectedSubscriptionModel(entry)) {
      show("warning", t("feedback.subscriptionModelDeleteBlocked"));
      return;
    }
    setSelectedId(entry.modelId);
    setDeleteCandidateId(entry.modelId);
    show(
      "warning",
      t("feedback.modelDeletePending", {
        name: entry.displayName || entry.modelId,
      }),
    );
  };

  const revealCatalogPath = async () => {
    const result = await invokeCmd<void>("reveal_path", {
      request: { path: catalogPath },
    });
    if (result.ok) {
      show("success", t("feedback.pathRevealed"));
    } else {
      show("warning", result.error);
    }
  };

  const openCatalogFile = async () => {
    const result = await invokeCmd<void>("open_path", {
      request: { path: catalogPath },
    });
    if (result.ok) {
      show("success", t("feedback.fileOpened"));
    } else {
      show("warning", result.error);
    }
  };

  const confirmDeleteModelEntry = async (entry: ModelCatalogEntry) => {
    if (!config) {
      show("warning", "模型配置尚未加载完成，请刷新后重试。");
      return;
    }
    if (busy) {
      show("info", "当前正在处理配置，请稍后再试。");
      return;
    }
    if (isProtectedSubscriptionModel(entry)) {
      show("warning", t("feedback.subscriptionModelDeleteBlocked"));
      return;
    }
    const name = entry.displayName || entry.modelId;

    setBusy(true);
    const request: OpenCodexDeleteRequest = {
      key: entry.modelId,
      expectedHash: config.catalogContentHash,
      note: "delete model catalog entry",
    };
    const result = await invokeCmd<OpenCodexWriteResult>(
      "catalog_entry_delete",
      { request },
    );
    setBusy(false);

    if (result.ok) {
      setDeleteCandidateId("");
      show("success", t("feedback.modelDeleted", { name }));
      await refresh();
    } else {
      show("warning", result.error);
    }
  };

  const publishDiffSections: Array<{
    label: string;
    path: string;
    diff: RenderableDiffLine[];
  }> = routerPreview
    ? [
        {
          label: t("pages.models.routerProvidersDiff"),
          path: routerPreview.providersPath,
          diff: routerPreview.providersDiff,
        },
        {
          label: t("pages.models.routerCatalogDiff"),
          path: routerPreview.catalogPath,
          diff: routerPreview.catalogDiff,
        },
        {
          label: t("pages.models.routerConfigDiff"),
          path: routerPreview.configPath,
          diff: routerPreview.configDiff,
        },
        {
          label: t("pages.models.routerModelsCacheDiff"),
          path: routerPreview.modelsCachePath,
          diff: routerPreview.modelsCacheDiff,
        },
        {
          label: t("pages.models.routerInjectMapDiff"),
          path: routerPreview.injectMapPath,
          diff: routerPreview.injectMapDiff,
        },
      ]
    : [];

  return (
    <PageShell
      title={t("pages.models.title")}
      subtitle={t("pages.models.subtitle")}
      notice={notice}
      action={
        <ToolbarButton
          icon={<RotateCcw size={13} />}
          onClick={() => void refresh()}
          disabled={busy}
        >
          {t("actions.refresh")}
        </ToolbarButton>
      }
    >
      <section className="grid grid-cols-1 gap-4 xl:grid-cols-[420px_minmax(0,1fr)]">
        <div className="cb-surface overflow-hidden p-0">
          <div className="border-b border-ink-900/[0.06] bg-white/40 px-5 py-4">
            <div className="flex items-center gap-2">
              <span className="flex h-8 w-8 items-center justify-center rounded-lg bg-ink-900 text-white">
                <KeyRound size={15} />
              </span>
              <div>
                <h2 className="text-[15px] font-semibold leading-tight text-ink-900">
                  {t("pages.models.simpleConfigTitle")}
                </h2>
                <p className="mt-0.5 text-[12px] leading-[1.5] text-ink-500">
                  {t("pages.models.form.simpleHint")}
                </p>
              </div>
            </div>
          </div>
          <div className="flex flex-col gap-4 p-5">
            <FormField label={t("pages.models.form.modelName")}>
              <input
                value={modelInput}
                onChange={(event) => setModelInput(event.target.value)}
                placeholder={t("pages.models.form.modelNamePlaceholder")}
                className="h-11 rounded-lg border border-ink-900/[0.08] bg-white/75 px-3 text-[13px] text-ink-900 outline-none transition focus:border-ink-900/25 focus:bg-white focus:ring-4 focus:ring-ink-900/[0.04]"
              />
            </FormField>
            <FormField label={t("pages.models.form.baseUrl")}>
              <input
                value={baseUrl}
                onChange={(event) => setBaseUrl(event.target.value)}
                placeholder="https://api.deepseek.com/v1"
                className="h-11 rounded-lg border border-ink-900/[0.08] bg-white/75 px-3 font-mono text-[12px] text-ink-900 outline-none transition focus:border-ink-900/25 focus:bg-white focus:ring-4 focus:ring-ink-900/[0.04]"
              />
            </FormField>
            <FormField label={t("pages.models.form.wireApi")}>
              <select
                value={simpleWireApi}
                onChange={(event) =>
                  setSimpleWireApi(
                    event.target.value as ProviderView["wireApi"],
                  )
                }
                className="h-11 rounded-lg border border-ink-900/[0.08] bg-white/75 px-3 font-mono text-[12px] text-ink-900 outline-none transition focus:border-ink-900/25 focus:bg-white focus:ring-4 focus:ring-ink-900/[0.04]"
              >
                <option value="responses">Responses</option>
                <option value="chat">Chat Completions</option>
              </select>
              <span className="text-[11px] leading-[1.5] text-ink-500">
                {simpleWireApi === "responses"
                  ? t("pages.models.form.responsesHint")
                  : t("pages.models.form.chatHint")}
              </span>
            </FormField>
            <FormField label={t("pages.models.form.apiKey")}>
              <div className="flex h-11 items-center rounded-lg border border-ink-900/[0.08] bg-white/75 pr-2 transition focus-within:border-ink-900/25 focus-within:bg-white focus-within:ring-4 focus-within:ring-ink-900/[0.04]">
                <input
                  value={apiKey}
                  onChange={(event) => setApiKey(event.target.value)}
                  type={showApiKey ? "text" : "password"}
                  placeholder="sk-... 或 ${MINIMAX_API_KEY}"
                  className="min-w-0 flex-1 bg-transparent px-3 font-mono text-[12px] text-ink-900 outline-none"
                />
                <button
                  type="button"
                  onClick={() => setShowApiKey((value) => !value)}
                  className="flex h-8 w-8 items-center justify-center rounded-md text-ink-500 hover:bg-ink-900/5"
                  aria-label={showApiKey ? t("actions.hide") : t("actions.show")}
                >
                  {showApiKey ? <EyeOff size={14} /> : <Eye size={14} />}
                </button>
              </div>
              <span className="text-[11px] leading-[1.5] text-ink-500">
                {t("pages.models.form.secretHint")}
              </span>
            </FormField>
            <button
              type="button"
              disabled={busy}
              onClick={() => void saveSimpleModelConfig()}
              className="inline-flex h-11 items-center justify-center gap-2 rounded-lg bg-ink-900 px-4 text-[13px] font-semibold text-white shadow-sm transition hover:bg-ink-800 active:translate-y-px disabled:cursor-not-allowed disabled:opacity-45"
            >
              <Save size={14} />
              {t("pages.models.form.saveAndAdd")}
            </button>
          </div>
        </div>

        <div className="cb-surface overflow-hidden p-0">
          <div className="flex items-center justify-between border-b border-ink-900/[0.06] bg-white/40 px-5 py-4">
            <div className="flex items-center gap-2">
              <span className="flex h-8 w-8 items-center justify-center rounded-lg bg-ink-900/[0.05] text-ink-700">
                <Sparkles size={15} />
              </span>
              <div>
                <h2 className="text-[15px] font-semibold leading-tight text-ink-900">
                  {t("pages.models.dropdownTitle")}
                </h2>
                <p className="mt-0.5 text-[12px] text-ink-500">
                  {dropdownCatalog.filter((m) => m.visible).length} /{" "}
                  {dropdownCatalog.length}
                </p>
              </div>
            </div>
          </div>
          <div
            className={[
              "border-b px-5 py-3",
              codexAppConnected
                ? "border-status-ok/20 bg-status-ok/[0.06]"
                : "border-status-warn/25 bg-status-warn/[0.08]",
            ].join(" ")}
          >
            <div className="flex items-start gap-3">
              <span
                className={[
                  "mt-0.5 flex h-7 w-7 shrink-0 items-center justify-center rounded-md",
                  codexAppConnected
                    ? "bg-status-ok/10 text-status-ok"
                    : "bg-status-warn/10 text-status-warn",
                ].join(" ")}
              >
                {codexAppConnected ? (
                  <CheckCircle2 size={14} />
                ) : (
                  <AlertTriangle size={14} />
                )}
              </span>
              <div className="min-w-0">
                <div className="text-[12px] font-semibold text-ink-900">
                  {t("pages.models.codexLinkTitle")}: {codexAppStatusText}
                </div>
                <div className="mt-1 text-[11px] leading-[1.55] text-ink-600">
                  {codexAppStatusDetail}
                </div>
              </div>
            </div>
          </div>
          {selected ? (
            <div className="border-b border-ink-900/[0.06] bg-white/25 px-5 py-3">
              <div className="mb-2 flex items-center justify-between gap-3">
                <div className="min-w-0">
                  <div className="text-[11px] font-medium text-ink-400">
                    {t("pages.models.selectedTitle")}
                  </div>
                  <div className="mt-0.5 truncate text-[13px] font-semibold text-ink-900">
                    {selected.displayName || selected.modelId}
                  </div>
                </div>
                <ToolbarButton
                  icon={selected.visible ? <EyeOff size={13} /> : <Eye size={13} />}
                  disabled={busy || isProtectedSubscriptionModel(selected)}
                  onClick={() => void toggleVisibility(selected)}
                >
                  {selected.visible ? t("actions.hide") : t("actions.show")}
                </ToolbarButton>
              </div>
              <div className="grid grid-cols-2 gap-x-4 gap-y-2 text-[11px] md:grid-cols-3">
                <div className="min-w-0">
                  <div className="text-ink-400">picker id</div>
                  <div className="truncate font-mono text-ink-700">{selected.modelId}</div>
                </div>
                <div className="min-w-0">
                  <div className="text-ink-400">{t("pages.models.catalogProvider")}</div>
                  <div className="truncate font-mono text-ink-700">{selected.provider || "-"}</div>
                </div>
                <div className="min-w-0">
                  <div className="text-ink-400">{t("pages.models.backendProvider")}</div>
                  <div className="truncate font-mono text-ink-700">
                    {modelUpstreamProvider(selected) || selected.provider || "-"}
                  </div>
                </div>
                <div className="min-w-0">
                  <div className="text-ink-400">{t("pages.models.backendModel")}</div>
                  <div className="truncate font-mono text-ink-700">
                    {selected.backendModel || selected.modelId}
                  </div>
                </div>
                <div className="min-w-0">
                  <div className="text-ink-400">{t("fields.visible")}</div>
                  <div className="truncate text-ink-700">
                    {selected.visible ? t("common.enabled") : t("common.disabled")}
                  </div>
                </div>
                <div className="min-w-0">
                  <div className="text-ink-400">{t("pages.models.route")}</div>
                  <div className="truncate font-mono text-ink-700">{modelRouteLabel(selected)}</div>
                </div>
              </div>
            </div>
          ) : (
            <div className="border-b border-ink-900/[0.06] bg-white/25 px-5 py-4">
              <div className="flex items-start gap-3">
                <span className="flex h-8 w-8 shrink-0 items-center justify-center rounded-lg bg-ink-900/[0.05] text-ink-500">
                  <MousePointer2 size={14} />
                </span>
                <div className="min-w-0">
                  <div className="text-[12px] font-semibold text-ink-900">
                    {t("pages.models.selectModelTitle")}
                  </div>
                  <p className="mt-1 text-[11px] leading-[1.55] text-ink-500">
                    {t("pages.models.selectModelHint")}
                  </p>
                </div>
              </div>
            </div>
          )}
          <div className="grid max-h-[560px] grid-cols-1 gap-2 overflow-y-auto p-4 cb-scroll 2xl:grid-cols-2">
            {dropdownCatalog.map((entry) => {
              const canDelete = !isProtectedSubscriptionModel(entry);
              const canToggle = !isProtectedSubscriptionModel(entry);
              return (
                <div
                  key={entry.modelId}
                  role="button"
                  tabIndex={0}
                  onClick={() => setSelectedId(entry.modelId)}
                  onKeyDown={(event) => {
                    if (event.key !== "Enter" && event.key !== " ") return;
                    event.preventDefault();
                    setSelectedId(entry.modelId);
                  }}
                  className={[
                    "group relative rounded-xl border p-3 pr-12 text-left transition",
                    entry.modelId === selected?.modelId
                      ? "border-ink-900/15 bg-ink-900/[0.04] shadow-sm"
                      : "border-ink-900/[0.06] bg-white/45 hover:bg-white/75",
                  ].join(" ")}
                >
                  <div className="flex items-start gap-3">
                    <input
                      type="checkbox"
                      checked={entry.visible}
                      disabled={!canToggle}
                      onChange={(event) => {
                        event.stopPropagation();
                        void toggleVisibility(entry);
                      }}
                      onClick={(event) => event.stopPropagation()}
                      className="mt-0.5 h-4 w-4 accent-ink-900 disabled:cursor-not-allowed disabled:opacity-45"
                    />
                    <div className="min-w-0 flex-1">
                      <div className="truncate text-[13px] font-semibold text-ink-900">
                        {entry.displayName || entry.modelId}
                      </div>
                      <div className="mt-1 truncate font-mono text-[11px] text-ink-400">
                        {modelRouteLabel(entry)}
                      </div>
                    </div>
                    {canDelete ? (
                      <button
                        type="button"
                        title={t("actions.delete")}
                        aria-label={t("actions.delete")}
                        onClick={(event) => {
                          event.preventDefault();
                          event.stopPropagation();
                          requestDeleteModelEntry(entry);
                        }}
                        onKeyDown={(event) => {
                          if (event.key !== "Enter" && event.key !== " ") return;
                          event.preventDefault();
                          event.stopPropagation();
                          requestDeleteModelEntry(entry);
                        }}
                        className="absolute right-3 top-3 flex h-8 w-8 shrink-0 items-center justify-center rounded-lg border border-transparent text-ink-300 opacity-100 transition hover:border-status-fail/15 hover:bg-status-fail/10 hover:text-status-fail focus-visible:border-status-fail/20 focus-visible:bg-status-fail/10 focus-visible:text-status-fail focus-visible:outline-none sm:opacity-0 sm:group-hover:opacity-100 sm:group-focus-within:opacity-100"
                      >
                        <Trash2 size={13} />
                      </button>
                    ) : null}
                  </div>
                </div>
              );
            })}
            {dropdownCatalog.length === 0 ? (
              <div className="col-span-full rounded-xl border border-dashed border-ink-900/[0.10] bg-white/35 px-4 py-12 text-center text-[12px] leading-[1.6] text-ink-500">
                {t("feedback.modelsEmpty")}
              </div>
            ) : null}
          </div>
        </div>
      </section>

      {routerPreview && (
        <div
          className="fixed inset-0 z-50 flex items-center justify-center bg-ink-900/10 px-4 py-6"
          role="dialog"
          aria-modal="true"
          aria-labelledby="publish-codex-dialog-title"
          onClick={() => {
            if (!busy) setRouterPreview(null);
          }}
        >
          <div
            className="flex max-h-[78vh] w-full max-w-[680px] flex-col overflow-hidden rounded-xl border border-white/70 bg-white/95 shadow-card"
            onClick={(event) => event.stopPropagation()}
          >
            <div className="border-b border-ink-900/[0.06] px-5 py-4">
              <div className="flex items-center gap-2">
                <span className="flex h-8 w-8 items-center justify-center rounded-lg bg-ink-900 text-white">
                  <Save size={15} />
                </span>
                <div>
                  <h3
                    id="publish-codex-dialog-title"
                    className="text-[15px] font-semibold text-ink-900"
                  >
                    {t("pages.models.routerPreviewTitle")}
                  </h3>
                  <p className="mt-0.5 text-[12px] leading-[1.55] text-ink-500">
                    {t("pages.models.publishHint")}
                  </p>
                </div>
              </div>
            </div>
            <div className="min-h-0 flex-1 overflow-y-auto px-5 py-4 cb-scroll">
              <SummaryStrip
                items={[
                  {
                    label: t("pages.models.routerModels"),
                    value: String(routerPreview.routedModelCount),
                  },
                  {
                    label: t("pages.models.routerEndpoint"),
                    value: routerPreview.proxyBaseUrl,
                  },
                  {
                    label: t("fields.configPath"),
                    value: routerPreview.configPath,
                  },
                  {
                    label: t("pages.models.restartHintLabel"),
                    value: t("pages.models.restartHintValue"),
                  },
                ]}
              />
              <div className="mt-4 flex gap-3 rounded-lg border border-amber-200 bg-amber-50 px-3 py-3 text-[12px] leading-[1.6] text-amber-900">
                <AlertTriangle
                  size={15}
                  className="mt-0.5 shrink-0 text-amber-600"
                />
                <div>
                  <div className="font-semibold">
                    {t("pages.models.officialAuthNoticeTitle")}
                  </div>
                  <div className="mt-0.5 text-amber-800">
                    {t("pages.models.officialAuthNoticeBody")}
                  </div>
                </div>
              </div>
              <details className="mt-4 rounded-lg border border-ink-900/[0.06] bg-white/55">
                <summary className="cursor-pointer px-3 py-2 text-[12px] font-semibold text-ink-800">
                  {t("pages.models.showPublishDetails")}
                </summary>
                <div className="space-y-4 border-t border-ink-900/[0.06] p-3">
                  {publishDiffSections.map((section) => (
                    <div className="min-w-0" key={section.label}>
                      <div className="mb-2 flex items-center justify-between gap-2">
                        <div className="cb-label">{section.label}</div>
                        <PathValue value={section.path} />
                      </div>
                      <DiffBlock lines={section.diff} />
                    </div>
                  ))}
                </div>
              </details>
            </div>
            <div className="flex justify-end gap-2 border-t border-ink-900/[0.06] bg-white/80 px-5 py-3">
              <button
                type="button"
                disabled={busy}
                onClick={() => setRouterPreview(null)}
                className="inline-flex h-9 items-center justify-center rounded-md border border-ink-900/10 bg-white/70 px-4 text-[12px] font-medium text-ink-700 hover:bg-white disabled:cursor-not-allowed disabled:opacity-45"
              >
                {t("actions.cancel")}
              </button>
              <button
                type="button"
                disabled={busy}
                onClick={() => void applyMultirouterPreview()}
                className="inline-flex h-9 items-center justify-center rounded-md bg-ink-900 px-4 text-[12px] font-semibold text-white shadow-sm hover:bg-ink-800 disabled:cursor-not-allowed disabled:opacity-45"
              >
                {t("pages.models.routerConfirm")}
              </button>
            </div>
          </div>
        </div>
      )}

      {routerCacheRestorePreview && (
        <Panel
          title={t("pages.models.routerRestorePreviewTitle")}
          icon={<RotateCcw size={15} />}
          action={
            <div className="flex flex-wrap justify-end gap-2">
              <ToolbarButton
                icon={<EyeOff size={13} />}
                disabled={busy}
                onClick={() => setRouterCacheRestorePreview(null)}
              >
                {t("actions.cancel")}
              </ToolbarButton>
              <ToolbarButton
                icon={<Save size={13} />}
                variant="primary"
                disabled={busy || !routerCacheRestorePreview.restoreAvailable}
                onClick={() => void applyRestoreModelsCache()}
              >
                {t("pages.models.routerRestoreConfirm")}
              </ToolbarButton>
            </div>
          }
        >
          <SummaryStrip
            items={[
              {
                label: t("pages.models.routerCache"),
                value: routerCacheRestorePreview.restoreAvailable
                  ? t("common.enabled")
                  : t("common.none"),
              },
              {
                label: t("pages.models.routerCacheBackup"),
                value: routerCacheRestorePreview.backupExists
                  ? t("common.found")
                  : t("common.missing"),
              },
              {
                label: t("pages.models.routerCacheAction"),
                value: routerCacheRestorePreview.willDelete
                  ? t("pages.models.routerCacheDelete")
                  : t("pages.models.routerCacheRestore"),
              },
            ]}
          />
          <div className="mt-4 min-w-0">
            <div className="mb-2 flex items-center justify-between gap-2">
              <div className="cb-label">{t("pages.models.routerModelsCacheDiff")}</div>
              <PathValue value={routerCacheRestorePreview.modelsCachePath} />
            </div>
            <DiffBlock lines={routerCacheRestorePreview.diff} />
          </div>
        </Panel>
      )}

      <div className="cb-surface p-4">
        <div className="flex items-center justify-between gap-3">
          <div className="flex items-center gap-2 text-[13px] font-semibold text-ink-900">
            <Settings size={14} />
            {t("pages.models.advancedTitle")}
          </div>
          <ToolbarButton
            icon={<Eye size={13} />}
            onClick={() => setShowAdvancedModels((value) => !value)}
          >
            {showAdvancedModels ? t("actions.hideDetails") : t("actions.showDetails")}
          </ToolbarButton>
        </div>
      </div>

      {showAdvancedModels && (
        <>
          <div className="cb-surface p-4">
            <div className="flex flex-col gap-4 lg:flex-row lg:items-center lg:justify-between">
              <div className="flex min-w-0 items-start gap-3">
                <span className="flex h-8 w-8 shrink-0 items-center justify-center rounded-lg bg-[#0A84FF]/10 text-[#0A84FF]">
                  <Route size={15} />
                </span>
                <div className="min-w-0">
                  <h2 className="text-[14px] font-semibold leading-tight text-ink-900">
                    {t("pages.models.routerTitle")}
                  </h2>
                  <p className="mt-1 max-w-[76ch] text-[12px] leading-[1.6] text-ink-500">
                    {t("pages.models.routerHint")}
                  </p>
                </div>
              </div>
              <div className="flex flex-wrap justify-end gap-2">
                <ToolbarButton
                  icon={<RotateCcw size={13} />}
                  disabled={busy}
                  onClick={() => void previewRestoreModelsCache()}
                >
                  {t("pages.models.routerRestoreCache")}
                </ToolbarButton>
                <ToolbarButton
                  icon={<RefreshCw size={13} />}
                  variant="primary"
                  disabled={busy}
                  onClick={() => void previewMultirouter()}
                >
                  {t("pages.models.routerPreview")}
                </ToolbarButton>
              </div>
            </div>
            <div className="mt-4 grid grid-cols-1 divide-y divide-ink-900/[0.06] border-t border-ink-900/[0.06] text-[12px] md:grid-cols-4 md:divide-x md:divide-y-0">
              <div className="min-w-0 py-3 md:px-3 md:first:pl-0">
                <div className="cb-label">{t("pages.models.routerProvider")}</div>
                <div className="mt-1 truncate font-mono text-ink-800">
                  {routerProvider?.name || DEFAULT_MULTIROUTER_PROVIDER_ID}
                </div>
              </div>
              <div className="min-w-0 py-3 md:px-3">
                <div className="cb-label">{t("pages.models.routerRoutes")}</div>
                <div className="mt-1 font-semibold text-ink-900">{routerRouteCount}</div>
              </div>
              <div className="min-w-0 py-3 md:px-3">
                <div className="cb-label">{t("pages.models.routerModels")}</div>
                <div className="mt-1 font-semibold text-ink-900">{routerModelCount}</div>
              </div>
              <div className="min-w-0 py-3 md:px-3 md:last:pr-0">
                <div className="cb-label">{t("pages.models.routerEndpoint")}</div>
                <div className="mt-1 truncate font-mono text-ink-800">
                  {routerProvider?.baseUrl || "http://127.0.0.1:1455/v1"}
                </div>
              </div>
            </div>
          </div>
          <div className="grid grid-cols-1 gap-4 xl:grid-cols-2">
            <Panel
              title={t("pages.models.catalogSourceTitle")}
              icon={<GitCompare size={15} />}
              action={
                <div className="flex flex-wrap justify-end gap-2">
                  <ToolbarButton icon={<FolderOpen size={13} />} onClick={() => void revealCatalogPath()}>
                    {t("actions.revealInFinder")}
                  </ToolbarButton>
                  <ToolbarButton icon={<FileText size={13} />} onClick={() => void openCatalogFile()}>
                    {t("actions.openFile")}
                  </ToolbarButton>
                </div>
              }
            >
              <DetailRow label={t("fields.catalogPath")} value={<PathBlock value={catalogPath} />} />
              <DetailRow label={t("fields.contentHash")} value={<PathValue value={config?.catalogContentHash || "-"} />} />
            </Panel>
          </div>
          <Panel title={t("pages.models.visionTitle")} icon={<Eye size={15} />}>
            {selected ? (
              <div className="grid grid-cols-1 gap-4 xl:grid-cols-[minmax(240px,0.8fr)_minmax(0,1.2fr)]">
                <div className="rounded-md border border-ink-900/[0.06] bg-white/35 p-3">
                  <p className="text-[12px] leading-[1.6] text-ink-500">{t("pages.models.visionHint")}</p>
                  <label className="mt-3 flex items-center gap-2 text-[12px] text-ink-700">
                    <input type="checkbox" checked={visionEnabled} disabled={isProtectedSubscriptionModel(selected)} onChange={(event) => setVisionEnabled(event.target.checked)} className="h-4 w-4 accent-ink-900 disabled:cursor-not-allowed disabled:opacity-45" />
                    {t("pages.models.visionEnable")}
                  </label>
                </div>
                <div className="grid grid-cols-1 gap-3 md:grid-cols-3">
                  <FormField label={t("pages.models.form.baseUrl")}>
                    <input className={inputClass} value={visionBaseUrl} placeholder="https://api.example.com/v1" onChange={(event) => setVisionBaseUrl(event.target.value)} />
                  </FormField>
                  <FormField label={t("pages.models.visionModel")}>
                    <input className={inputClass} value={visionModel} placeholder="vision-model" onChange={(event) => setVisionModel(event.target.value)} />
                  </FormField>
                  <FormField label={t("pages.models.visionEnvKey")}>
                    <input className={inputClass} value={visionEnvKey} placeholder="VISION_API_KEY" onChange={(event) => setVisionEnvKey(event.target.value)} />
                  </FormField>
                  <div className="md:col-span-3 flex justify-end">
                    <ToolbarButton icon={<Save size={13} />} variant="primary" disabled={busy || !selected || isProtectedSubscriptionModel(selected)} onClick={() => void saveVisionFallback()}>
                      {t("pages.models.visionSave")}
                    </ToolbarButton>
                  </div>
                </div>
              </div>
            ) : (
              <div className="rounded-md border border-dashed border-ink-900/[0.10] bg-white/35 px-4 py-6 text-center">
                <div className="text-[12px] font-semibold text-ink-800">
                  {t("pages.models.selectModelTitle")}
                </div>
                <p className="mx-auto mt-1 max-w-[46ch] text-[11px] leading-[1.55] text-ink-500">
                  {t("pages.models.selectModelHint")}
                </p>
              </div>
            )}
          </Panel>
        </>
      )}
      {deleteCandidate ? (
        <div
          className="fixed inset-0 z-50 flex items-center justify-center bg-ink-900/10 px-4"
          role="dialog"
          aria-modal="true"
          aria-labelledby="delete-model-dialog-title"
          onClick={() => {
            if (!busy) setDeleteCandidateId("");
          }}
        >
          <div
            className="w-full max-w-[420px] rounded-lg border border-white/70 bg-white/95 p-5 shadow-card"
            onClick={(event) => event.stopPropagation()}
          >
            <div className="mb-3 flex items-center gap-2">
              <span className="flex h-8 w-8 items-center justify-center rounded-md border border-status-fail/15 bg-status-fail/[0.07] text-status-fail">
                <Trash2 size={16} />
              </span>
              <h3
                id="delete-model-dialog-title"
                className="text-[15px] font-semibold text-ink-900"
              >
                {t("actions.delete")}
              </h3>
            </div>
            <p className="text-[13px] leading-[1.7] text-ink-600">
              {t("feedback.modelDeleteConfirm", {
                name: deleteCandidate.displayName || deleteCandidate.modelId,
              })}
            </p>
            <div className="mt-3 rounded-md border border-[#0A84FF]/10 bg-[#0A84FF]/[0.04] px-3 py-2 text-[11px] leading-[1.55] text-ink-500">
              custom_model_catalog.json
            </div>
            <div className="mt-5 flex justify-end gap-2">
              <button
                type="button"
                disabled={busy}
                onClick={() => setDeleteCandidateId("")}
                className="inline-flex h-9 items-center justify-center rounded-md border border-ink-900/10 bg-white/70 px-4 text-[12px] font-medium text-ink-700 hover:bg-white disabled:cursor-not-allowed disabled:opacity-45"
              >
                {t("actions.cancel")}
              </button>
              <button
                type="button"
                disabled={busy}
                onClick={() => void confirmDeleteModelEntry(deleteCandidate)}
                className="inline-flex h-9 items-center justify-center rounded-md bg-status-fail px-4 text-[12px] font-medium text-white shadow-sm shadow-status-fail/15 hover:opacity-90 disabled:cursor-not-allowed disabled:opacity-45"
              >
                {t("actions.confirmDelete")}
              </button>
            </div>
          </div>
        </div>
      ) : null}
    </PageShell>
  );
}

function isRouterProvider(provider: ProviderRoute) {
  return (
    provider.name === DEFAULT_MULTIROUTER_PROVIDER_ID ||
    (provider.codexRouting?.routes?.length ?? 0) > 0
  );
}

export function ModelRouterPage() {
  const { t } = useTranslation();
  const { notice, show } = useNotice();
  const [config, setConfig] = useState<OpenCodexCustomConfig | null>(null);
  const [proxy, setProxy] = useState<ProxyStatusView | null>(null);
  const [routing, setRouting] = useState<EffectiveRoutingStatus | null>(null);
  const [desktop, setDesktop] =
    useState<CodexDesktopIntegrationStatus | null>(null);
  const [routerPreview, setRouterPreview] =
    useState<CodexMultirouterPreview | null>(null);
  const [pickerResult, setPickerResult] =
    useState<CodexPickerUnlockResult | null>(null);
  const [routeTestModel, setRouteTestModel] = useState("");
  const [routeTestResult, setRouteTestResult] =
    useState<ProxyRouteTestResult | null>(null);
  const [loading, setLoading] = useState(false);
  const [busy, setBusy] = useState(false);
  const [routeTestWorking, setRouteTestWorking] = useState(false);

  const visibleModels = useMemo(
    () => mergeBuiltinAndCustomModels(config?.catalog ?? []).filter(
      (model) => model.visible,
    ),
    [config?.catalog],
  );
  const apiProviders = useMemo(
    () => (config?.providers ?? []).filter((provider) => !isRouterProvider(provider)),
    [config?.providers],
  );
  const enabledProviders = apiProviders.filter((provider) => provider.enabled);
  const routerProvider =
    routerPreview?.routerProvider ||
    config?.providers.find((provider) => provider.name === DEFAULT_MULTIROUTER_PROVIDER_ID) ||
    config?.providers.find((provider) => (provider.codexRouting?.routes?.length ?? 0) > 0) ||
    null;
  const routerRoutes =
    routerPreview?.routerProvider.codexRouting?.routes ||
    routerProvider?.codexRouting?.routes ||
    [];
  const activeProxyPort = proxyPortOrDefault(proxy);
  const pickerModelCount =
    desktop?.modelsCacheModelCount ??
    desktop?.routerProviderModelsCount ??
    visibleModels.length;
  const routerReady =
    routing?.modelProvider != null &&
    COMPAT_MULTIROUTER_PROVIDER_IDS.has(routing.modelProvider.toLowerCase()) &&
    Boolean(routing.catalogConfigured);

  const refresh = useCallback(async () => {
    setLoading(true);
    const [configR, proxyR, routingR, desktopR] = await Promise.all([
      invokeCmd<OpenCodexCustomConfig>("opencodex_config_read"),
      invokeCmd<ProxyStatusView>("proxy_status"),
      invokeCmd<EffectiveRoutingStatus>("effective_routing_status"),
      invokeCmd<CodexDesktopIntegrationStatus>(
        "codex_desktop_integration_status",
      ),
    ]);
    setLoading(false);
    if (configR.ok) {
      setConfig(configR.data);
      if (configR.data.parseErrors.length > 0) {
        show("warning", configR.data.parseErrors[0].message);
      }
    } else {
      show("warning", configR.error);
    }
    if (proxyR.ok) setProxy(proxyR.data);
    if (routingR.ok) setRouting(routingR.data);
    if (desktopR.ok) setDesktop(desktopR.data);
  }, [show]);

  useEffect(() => {
    void refresh();
  }, [refresh]);

  useEffect(() => {
    setRouteTestModel((current) => current || visibleModels[0]?.modelId || "");
  }, [visibleModels]);

  const startLocalProxy = useCallback(async () => {
    setBusy(true);
    const result = await invokeCmd<{ port: number; status: string }>(
      "proxy_start",
      { request: { port: activeProxyPort } },
    );
    setBusy(false);
    if (result.ok) {
      show(
        "success",
        t("pages.modelRouter.feedback.proxyStarted", {
          port: result.data.port,
        }),
      );
      void refresh();
    } else {
      show("warning", result.error);
    }
  }, [activeProxyPort, refresh, show, t]);

  const previewRoutes = useCallback(async () => {
    setBusy(true);
    const result = await invokeCmd<CodexMultirouterPreview>(
      "codex_multirouter_preview",
      {
        request: {
          proxyPort: activeProxyPort,
          routerProviderId: DEFAULT_MULTIROUTER_PROVIDER_ID,
          ensureCodexConfig: true,
        },
      },
    );
    setBusy(false);
    if (result.ok) {
      setRouterPreview(result.data);
      show("info", t("pages.modelRouter.feedback.previewReady"));
    } else {
      show("warning", result.error);
    }
  }, [activeProxyPort, show, t]);

  const publishRoutes = useCallback(async () => {
    if (!routerPreview) return;
    setBusy(true);
    const result = await invokeCmd<CodexMultirouterSyncResult>(
      "codex_multirouter_apply",
      {
        request: {
          preview: routerPreview,
          confirmed: true,
        },
      },
    );
    setBusy(false);
    if (!result.ok) {
      show("warning", result.error);
      return;
    }

    setRouterPreview(null);
    const picker = await invokeCmd<CodexPickerUnlockResult>(
      "codex_desktop_picker_unlock",
    );
    if (picker.ok) {
      setPickerResult(picker.data);
    }
    show(
      picker.ok && !picker.data.injected ? "warning" : "success",
      picker.ok && picker.data.injected
        ? t("pages.modelRouter.feedback.publishedAndUnlocked", {
            routes: result.data.routeCount,
            models: result.data.routedModelCount,
          })
        : t("pages.modelRouter.feedback.publishedNeedsUnlock", {
            routes: result.data.routeCount,
            models: result.data.routedModelCount,
            message: picker.ok ? picker.data.message : picker.error,
          }),
    );
    void refresh();
  }, [refresh, routerPreview, show, t]);

  const runRouteTest = useCallback(
    async (performUpstream = false) => {
      const modelId = routeTestModel || visibleModels[0]?.modelId || "";
      if (!modelId) {
        show("warning", t("pages.modelRouter.feedback.noModel"));
        return;
      }
      setRouteTestWorking(true);
      const result = await invokeCmd<ProxyRouteTestResult>("proxy_route_test", {
        request: {
          modelId,
          prompt: "Codex Box route test",
          includeImage: true,
          performUpstream,
        },
      });
      setRouteTestWorking(false);
      if (result.ok) {
        setRouteTestResult(result.data);
        show(
          result.data.status === "passed" ? "success" : "warning",
          result.data.status === "passed"
            ? t("pages.modelRouter.feedback.testPassed")
            : t("pages.modelRouter.feedback.testFailed"),
        );
      } else {
        show("warning", result.error);
      }
    },
    [routeTestModel, show, t, visibleModels],
  );

  const unlockPicker = useCallback(async (
    command:
      | "codex_desktop_picker_unlock"
      | "codex_desktop_launch_with_debugging_and_unlock",
  ) => {
    setBusy(true);
    const result = await invokeCmd<CodexPickerUnlockResult>(command);
    setBusy(false);
    if (result.ok) {
      setPickerResult(result.data);
      show(result.data.injected ? "success" : "warning", result.data.message);
      void refresh();
    } else {
      show("warning", result.error);
    }
  }, [refresh, show]);

  return (
    <PageShell
      title={t("pages.modelRouter.title")}
      subtitle={t("pages.modelRouter.subtitle")}
      notice={notice}
      action={
        <ToolbarButton
          icon={<RotateCcw size={13} />}
          disabled={loading || busy}
          onClick={() => void refresh()}
        >
          {t("actions.refresh")}
        </ToolbarButton>
      }
    >
      <SummaryStrip
        items={[
          {
            label: t("pages.modelRouter.summary.codex"),
            value: routerReady ? t("common.configured") : t("common.notReady"),
            tone: routerReady ? "ok" : "warn",
          },
          {
            label: t("pages.modelRouter.summary.proxy"),
            value:
              proxy?.status === "running"
                ? `${t("common.running")} :${proxy.port}`
                : t("common.stopped"),
            tone: proxy?.status === "running" ? "running" : "idle",
          },
          {
            label: t("pages.modelRouter.summary.sources"),
            value: `${enabledProviders.length} / ${apiProviders.length}`,
          },
          {
            label: t("pages.modelRouter.summary.models"),
            value: String(visibleModels.length),
          },
        ]}
      />

      <div className="grid grid-cols-1 gap-4 xl:grid-cols-[minmax(0,0.95fr)_minmax(0,1.05fr)]">
        <Panel
          title={t("pages.modelRouter.sources.title")}
          icon={<Server size={15} />}
          help={t("pages.modelRouter.sources.help")}
        >
          <div className="grid gap-2">
            {apiProviders.slice(0, 8).map((provider) => (
              <div
                key={provider.name}
                className="grid grid-cols-[minmax(0,1fr)_84px] gap-3 border-t border-ink-900/[0.06] py-2.5 text-[12px]"
              >
                <div className="min-w-0">
                  <div className="truncate font-semibold text-ink-900">
                    {provider.name}
                  </div>
                  <div className="mt-0.5 truncate font-mono text-[11px] text-ink-500">
                    {provider.baseUrl}
                  </div>
                </div>
                <div className="flex items-center justify-end">
                  <StatusPill tone={provider.enabled ? "ok" : "idle"} />
                </div>
              </div>
            ))}
            {apiProviders.length === 0 ? (
              <div className="rounded-md border border-dashed border-ink-900/[0.10] bg-white/35 px-3 py-8 text-center text-[12px] text-ink-500">
                {t("pages.modelRouter.sources.empty")}
              </div>
            ) : null}
          </div>
        </Panel>

        <Panel
          title={t("pages.modelRouter.models.title")}
          icon={<Sparkles size={15} />}
          help={t("pages.modelRouter.models.help")}
        >
          <div className="grid max-h-[310px] grid-cols-1 gap-2 overflow-auto cb-scroll 2xl:grid-cols-2">
            {visibleModels.slice(0, 12).map((model) => (
              <button
                type="button"
                key={model.modelId}
                onClick={() => setRouteTestModel(model.modelId)}
                className={[
                  "min-w-0 rounded-md border px-3 py-2 text-left transition",
                  routeTestModel === model.modelId
                    ? "border-[#0A84FF]/25 bg-[#0A84FF]/[0.06]"
                    : "border-ink-900/[0.06] bg-white/45 hover:bg-white/75",
                ].join(" ")}
              >
                <div className="truncate text-[12px] font-semibold text-ink-900">
                  {model.displayName || model.modelId}
                </div>
                <div className="mt-0.5 truncate font-mono text-[11px] text-ink-500">
                  {modelRouteLabel(model)}
                </div>
              </button>
            ))}
            {visibleModels.length === 0 ? (
              <div className="rounded-md border border-dashed border-ink-900/[0.10] bg-white/35 px-3 py-8 text-center text-[12px] text-ink-500">
                {t("pages.modelRouter.models.empty")}
              </div>
            ) : null}
          </div>
        </Panel>
      </div>

      <Panel
        title={t("pages.modelRouter.routes.title")}
        icon={<Route size={15} />}
        help={t("pages.modelRouter.routes.help")}
        action={
          <div className="flex flex-wrap justify-end gap-2">
            <ToolbarButton
              icon={<Play size={13} />}
              disabled={busy || proxy?.status === "running"}
              onClick={() => void startLocalProxy()}
            >
              {t("pages.modelRouter.routes.startProxy")}
            </ToolbarButton>
            <ToolbarButton
              icon={<GitCompare size={13} />}
              disabled={busy}
              onClick={() => void previewRoutes()}
            >
              {t("pages.modelRouter.routes.preview")}
            </ToolbarButton>
            <ToolbarButton
              icon={<Save size={13} />}
              variant="primary"
              disabled={busy || !routerPreview}
              onClick={() => void publishRoutes()}
            >
              {t("pages.modelRouter.routes.publish")}
            </ToolbarButton>
          </div>
        }
      >
        <SummaryStrip
          items={[
            {
              label: t("pages.modelRouter.routes.provider"),
              value: routerPreview?.routerProviderId || routerProvider?.name || "-",
            },
            {
              label: t("pages.modelRouter.routes.routeCount"),
              value: String(routerPreview?.routeCount ?? routerRoutes.length),
            },
            {
              label: t("pages.modelRouter.routes.routedModels"),
              value: String(
                routerPreview?.routedModelCount ??
                  routerRoutes.reduce(
                    (sum, route) => sum + (route.match?.models?.length ?? 0),
                    0,
                  ),
              ),
            },
            {
              label: t("pages.modelRouter.routes.entry"),
              value:
                routerPreview?.proxyBaseUrl ||
                routerProvider?.baseUrl ||
                `http://127.0.0.1:${activeProxyPort}/v1`,
            },
          ]}
        />
        <div className="mt-4 grid grid-cols-1 gap-3 lg:grid-cols-2">
          <div className="min-w-0">
            <div className="mb-2 cb-label">
              {t("pages.modelRouter.routes.currentRoutes")}
            </div>
            <div className="max-h-[280px] overflow-auto rounded-md border border-ink-900/[0.06] bg-white/45 cb-scroll">
              {routerRoutes.length > 0 ? (
                routerRoutes.map((route) => (
                  <div
                    key={route.id}
                    className="grid grid-cols-[minmax(0,1fr)_minmax(0,1fr)] gap-3 border-t border-ink-900/[0.06] px-3 py-2 first:border-t-0"
                  >
                    <div className="min-w-0">
                      <div className="truncate text-[12px] font-semibold text-ink-900">
                        {route.label || route.id}
                      </div>
                      <div className="mt-0.5 truncate font-mono text-[11px] text-ink-500">
                        {(route.match?.models ?? []).join(", ") || "-"}
                      </div>
                    </div>
                    <div className="min-w-0 text-right">
                      <div className="truncate font-mono text-[11px] text-ink-700">
                        {route.targetProviderId ||
                          route.upstream?.auth?.source ||
                          "managed"}
                      </div>
                      <div className="mt-0.5 truncate font-mono text-[11px] text-ink-400">
                        {route.upstream?.apiFormat || "-"}
                      </div>
                    </div>
                  </div>
                ))
              ) : (
                <div className="px-3 py-8 text-center text-[12px] text-ink-500">
                  {t("pages.modelRouter.routes.empty")}
                </div>
              )}
            </div>
          </div>
          <div className="min-w-0">
            <div className="mb-2 cb-label">
              {t("pages.modelRouter.routes.pendingDiff")}
            </div>
            {routerPreview ? (
              <DiffBlock
                lines={[
                  ...routerPreview.providersDiff,
                  ...routerPreview.catalogDiff,
                  ...routerPreview.configDiff,
                  ...routerPreview.modelsCacheDiff,
                  ...routerPreview.injectMapDiff,
                ]}
              />
            ) : (
              <div className="rounded-md border border-dashed border-ink-900/[0.10] bg-white/35 px-3 py-8 text-center text-[12px] text-ink-500">
                {t("pages.modelRouter.routes.noPreview")}
              </div>
            )}
          </div>
        </div>
      </Panel>

      <Panel
        title={t("pages.modelRouter.test.title")}
        icon={<TestTube2 size={15} />}
        help={t("pages.modelRouter.test.help")}
        action={
          <div className="flex flex-wrap justify-end gap-2">
            <ToolbarButton
              icon={<TestTube2 size={13} />}
              disabled={routeTestWorking || !routeTestModel}
              onClick={() => void runRouteTest(false)}
            >
              {t("pages.modelRouter.test.dryRun")}
            </ToolbarButton>
            <ToolbarButton
              icon={<Network size={13} />}
              disabled={routeTestWorking || !routeTestModel}
              onClick={() => void runRouteTest(true)}
            >
              {t("pages.modelRouter.test.upstream")}
            </ToolbarButton>
            <ToolbarButton
              icon={<Sparkles size={13} />}
              disabled={busy || pickerModelCount === 0}
              onClick={() => void unlockPicker("codex_desktop_picker_unlock")}
            >
              {t("actions.unlockPicker")}
            </ToolbarButton>
            <ToolbarButton
              icon={<Play size={13} />}
              variant="primary"
              disabled={busy || pickerModelCount === 0}
              onClick={() =>
                void unlockPicker(
                  "codex_desktop_launch_with_debugging_and_unlock",
                )
              }
            >
              {t("pages.modelRouter.test.launchUnlock")}
            </ToolbarButton>
          </div>
        }
      >
        <div className="grid grid-cols-1 gap-4 xl:grid-cols-[320px_minmax(0,1fr)]">
          <div className="min-w-0">
            <FormField label={t("pages.modelRouter.test.model")}>
              <select
                className={inputClass}
                value={routeTestModel}
                onChange={(event) => setRouteTestModel(event.target.value)}
              >
                {visibleModels.map((model) => (
                  <option key={model.modelId} value={model.modelId}>
                    {model.displayName || model.modelId}
                  </option>
                ))}
              </select>
            </FormField>
            <div className="mt-3 rounded-md border border-ink-900/[0.06] bg-white/45 p-3">
              <DetailRow
                label={t("pages.modelRouter.test.picker")}
                value={
                  pickerResult?.message ||
                  (desktop?.codexRemoteDebuggingPort
                    ? t("pages.modelRouter.test.cdpReady", {
                        port: desktop.codexRemoteDebuggingPort,
                      })
                    : t("pages.modelRouter.test.cdpMissing"))
                }
              />
              <DetailRow
                label={t("pages.modelRouter.test.cache")}
                value={`${pickerModelCount} ${t("pages.modelRouter.summary.models")}`}
              />
            </div>
          </div>
          <div className="min-w-0">
            {routeTestResult ? (
              <div className="grid gap-3">
                <SummaryStrip
                  items={[
                    {
                      label: t("pages.modelRouter.test.status"),
                      value: routeTestResult.status,
                      tone:
                        routeTestResult.status === "passed" ? "ok" : "warn",
                    },
                    {
                      label: t("pages.modelRouter.test.provider"),
                      value: routeTestResult.providerName || "-",
                    },
                    {
                      label: t("pages.modelRouter.test.upstreamModel"),
                      value: routeTestResult.upstreamModel || "-",
                    },
                    {
                      label: t("pages.modelRouter.test.httpStatus"),
                      value:
                        routeTestResult.upstreamStatusCode == null
                          ? "-"
                          : String(routeTestResult.upstreamStatusCode),
                    },
                  ]}
                />
                <div className="rounded-md border border-ink-900/[0.06] bg-white/45">
                  {routeTestResult.steps.map((step) => (
                    <div
                      key={step.id}
                      className="grid grid-cols-[96px_minmax(0,1fr)] gap-3 border-t border-ink-900/[0.06] px-3 py-2 first:border-t-0"
                    >
                      <StatusPill tone={routeTestStepTone(step.status)} />
                      <div className="min-w-0">
                        <div className="truncate text-[12px] font-semibold text-ink-900">
                          {step.label}
                        </div>
                        <div className="mt-0.5 text-[11px] leading-[1.5] text-ink-500">
                          {step.detail}
                        </div>
                      </div>
                    </div>
                  ))}
                </div>
              </div>
            ) : (
              <div className="rounded-md border border-dashed border-ink-900/[0.10] bg-white/35 px-3 py-12 text-center text-[12px] text-ink-500">
                {t("pages.modelRouter.test.empty")}
              </div>
            )}
          </div>
        </div>
      </Panel>
    </PageShell>
  );
}

/**
 * ProviderRoutes 页面:管理 ~/.codex/codex-box/providers.json 条目
 * - 展示所有 provider 路由
 * - 启用/禁用某个路由(对应 Codex App picker 是否出现该 provider 的模型)
 * - 写入走 ~/.codex/codex-box/providers.json
 */
export function ProviderRoutesPage() {
  const { t } = useTranslation();
  const { notice, show } = useNotice();
  const [routes, setRoutes] = useState<ProviderRoute[]>([]);
  const [config, setConfig] = useState<OpenCodexCustomConfig | null>(null);
  const [selectedName, setSelectedName] = useState<string>("");
  const [busy, setBusy] = useState(false);
  const [showApiServiceAdvanced, setShowApiServiceAdvanced] = useState(false);
  const selected =
    routes.find((item) => item.name === selectedName) || routes[0];
  const providerWarningText =
    config?.parseErrors.map((err) => err.message).join(" ") || "";
  const routeNeedsKeyFix = (route: ProviderRoute) =>
    providerWarningText.includes(route.name);

  const refresh = useCallback(async () => {
    setBusy(true);
    const result = await invokeCmd<OpenCodexCustomConfig>(
      "opencodex_config_read",
    );
    setBusy(false);
    if (result.ok) {
      setConfig(result.data);
      if (result.data.parseErrors.length > 0) {
        show("warning", result.data.parseErrors[0].message);
      }
      if (result.data.providers.length > 0) {
        setRoutes(result.data.providers);
        setSelectedName((current) =>
          current &&
          result.data.providers.some((entry) => entry.name === current)
            ? current
            : result.data.providers[0]?.name || "",
        );
      } else {
        setRoutes([]);
        setSelectedName("");
        if (result.data.parseErrors.length === 0) {
          show("info", t("pages.providerRoutes.emptyNotice"));
        }
      }
    } else {
      show("warning", result.error);
    }
  }, [show, t]);

  useEffect(() => {
    void refresh();
  }, [refresh]);

  const toggleEnabled = async (route: ProviderRoute) => {
    if (!config) return;
    const next: ProviderRoute = { ...route, enabled: !route.enabled };
    const request: OpenCodexWriteRequest<ProviderRoute> = {
      entry: next,
      expectedHash: config.providersContentHash,
      note: null,
    };
    const result = await invokeCmd<OpenCodexWriteResult>(
      "provider_route_upsert",
      { request },
    );
    if (result.ok) {
      show("success", t("feedback.providerRouteToggled", { name: route.name }));
      await refresh();
    } else {
      show("warning", result.error);
    }
  };

  const removeRoute = async (route: ProviderRoute) => {
    if (!config) return;
    const request: OpenCodexDeleteRequest = {
      key: route.name,
      expectedHash: config.providersContentHash,
      note: null,
    };
    const result = await invokeCmd<OpenCodexWriteResult>(
      "provider_route_delete",
      { request },
    );
    if (result.ok) {
      show("warning", t("feedback.providerRouteDeleted", { name: route.name }));
      await refresh();
    } else {
      show("warning", result.error);
    }
  };

  return (
    <PageShell
      title={t("pages.providerRoutes.title")}
      subtitle={t("pages.providerRoutes.subtitle")}
      notice={notice}
      action={
        <ToolbarButton
          icon={<RotateCcw size={13} />}
          onClick={() => void refresh()}
          disabled={busy}
        >
          {t("actions.refresh")}
        </ToolbarButton>
      }
    >
      <SummaryStrip
        items={[
          { label: t("summary.totalRoutes"), value: String(routes.length) },
          {
            label: t("summary.enabledRoutes"),
            value: String(routes.filter((r) => r.enabled).length),
            tone: "ok",
          },
          {
            label: t("summary.disabledRoutes"),
            value: String(routes.filter((r) => !r.enabled).length),
            tone: routes.some((r) => !r.enabled) ? "warn" : "idle",
          },
        ]}
      />

      <div className="cb-surface p-4">
        <div className="flex items-start gap-3">
          <div className="flex h-8 w-8 shrink-0 items-center justify-center rounded-md bg-[#0A84FF]/10 text-[#0A84FF]">
            <Server size={15} />
          </div>
          <div className="min-w-0">
            <div className="text-[13px] font-semibold text-ink-900">
              {t("pages.providerRoutes.explainTitle")}
            </div>
            <p className="mt-1 text-[12px] leading-[1.65] text-ink-500">
              {t("pages.providerRoutes.explainBody")}
            </p>
            <div className="mt-3 rounded-md border border-ink-900/[0.06] bg-white/45 px-3 py-2">
              <div className="cb-label">
                {t("pages.providerRoutes.dataSource")}
              </div>
              <div className="mt-1 text-[12px] font-medium text-ink-800">
                {routes.length > 0
                  ? t("pages.providerRoutes.dataSourceLocal")
                  : t("pages.providerRoutes.dataSourceEmpty")}
              </div>
              <p className="mt-1 text-[11px] leading-[1.55] text-ink-500">
                {t("pages.providerRoutes.dataSourceHint")}
              </p>
            </div>
          </div>
        </div>
      </div>

      {config && config.parseErrors.length > 0 ? (
        <div className="rounded-md border border-status-warn/30 bg-status-warn/10 px-3 py-2 text-[12px] text-status-warn">
          {t("feedback.opencodexParseErrors")}
          <ul className="mt-1 list-disc pl-4">
            {config.parseErrors.map((err) => (
              <li key={err.file}>
                {err.file}: {err.message}
              </li>
            ))}
          </ul>
        </div>
      ) : null}

      <div className="grid grid-cols-[minmax(280px,0.95fr)_minmax(0,1.4fr)] gap-4">
        <Panel
          title={t("pages.providerRoutes.listTitle")}
          icon={<Server size={15} />}
        >
          <div className="flex flex-col gap-2">
            {routes.length > 0 ? (
              routes.map((route) => (
                <ListButton
                  key={route.name}
                  active={route.name === selected?.name}
                  title={route.name}
                  subtitle={`${t("pages.providerRoutes.endpointLabel")}: ${route.baseUrl}`}
                  right={
                    <StatusPill
                      tone={
                        routeNeedsKeyFix(route)
                          ? "warn"
                          : route.enabled
                            ? "ok"
                            : "idle"
                      }
                    />
                  }
                  onClick={() => setSelectedName(route.name)}
                />
              ))
            ) : (
              <div className="rounded-md border border-dashed border-ink-900/[0.08] bg-white/35 px-4 py-6 text-center text-[12px] text-ink-500">
                {t("pages.providerRoutes.dataSourceEmpty")}
              </div>
            )}
          </div>
        </Panel>
        <Panel
          title={t("pages.providerRoutes.detailTitle")}
          icon={<Server size={15} />}
          action={
            <div className="flex flex-wrap gap-2">
              <ToolbarButton
                icon={<CheckCircle2 size={13} />}
                disabled={!selected || busy}
                onClick={() => selected && void toggleEnabled(selected)}
              >
                {selected?.enabled ? t("actions.disable") : t("actions.enable")}
              </ToolbarButton>
              <ConfirmButton
                idleLabel={t("actions.delete")}
                confirmLabel={t("actions.confirmDelete")}
                disabled={!selected || busy}
                onConfirm={() => selected && void removeRoute(selected)}
              />
            </div>
          }
        >
          {selected ? (
            <>
              <DetailRow
                label={t("pages.providerRoutes.detail.name")}
                value={<span className="font-mono">{selected.name}</span>}
              />
              <DetailRow
                label={t("pages.providerRoutes.detail.endpoint")}
                value={
                  <span className="font-mono break-all">
                    {selected.baseUrl}
                  </span>
                }
              />
              <DetailRow
                label={t("pages.providerRoutes.detail.apiKey")}
                value={
                  routeNeedsKeyFix(selected) ? (
                    <span className="text-status-warn">
                      {t("feedback.providerKeyNeedsFix")}
                    </span>
                  ) : selected.apiKeyRef ? (
                    <SecretText value={selected.apiKeyRef} />
                  ) : (
                    "-"
                  )
                }
              />
              <DetailRow
                label={t("pages.providerRoutes.detail.enabled")}
                value={
                  selected.enabled ? t("common.enabled") : t("common.disabled")
                }
              />
              <DetailRow
                label={t("pages.providerRoutes.detail.note")}
                value={selected.note || "-"}
              />
              <div className="mt-3 flex justify-end">
                <ToolbarButton
                  icon={<Eye size={13} />}
                  onClick={() =>
                    setShowApiServiceAdvanced((current) => !current)
                  }
                >
                  {showApiServiceAdvanced
                    ? t("actions.hideDetails")
                    : t("actions.showDetails")}
                </ToolbarButton>
              </div>
              {showApiServiceAdvanced && (
                <div className="mt-3 rounded-md border border-ink-900/[0.06] bg-white/35 px-3 py-2">
                  <DetailRow
                    label="wire_api"
                    value={
                      <span className="font-mono">{selected.wireApi}</span>
                    }
                  />
                  <DetailRow
                    label="http_headers"
                    value={
                      Object.keys(selected.httpHeaders).length > 0
                        ? Object.entries(selected.httpHeaders)
                            .map(([k, v]) => `${k}=${v}`)
                            .join(" · ")
                        : "-"
                    }
                  />
                  <DetailRow
                    label={t("fields.providersPath")}
                    value={
                      <span className="font-mono break-all">
                        {config?.providersPath || "~/.codex/codex-box/providers.json"}
                      </span>
                    }
                  />
                  <DetailRow
                    label={t("fields.contentHash")}
                    value={
                      <span className="font-mono break-all">
                        {config?.providersContentHash || "-"}
                      </span>
                    }
                  />
                </div>
              )}
            </>
          ) : (
            <div className="py-6 text-center text-[12px] text-ink-500">
              {t("common.none")}
            </div>
          )}
        </Panel>
      </div>
    </PageShell>
  );
}

/**
 * Codex Box Runtime 页面(v0.3.1 升级):
 * 取代 v0.3 的"只读检测",升级为"本地代理 runtime 运行控制台"。
 * - 状态:Running / Stopped / Failed
 * - 操作:Start / Stop / Restart
 * - 路由表:inject-map(provider → upstream base_url / env_key / wire_api / models)
 * - /v1/models 预览:调本机代理的合并模型列表
 * - Inject / Restore:把 [model_providers.*] base_url 改写为 127.0.0.1:port/v1,Stop 时还原
 */
export function CodexRuntimePage() {
  const { t } = useTranslation();
  const { notice, show } = useNotice();
  const [proxy, setProxy] = useState<ProxyStatusView | null>(null);
  const [opencodex, setOpencodex] = useState<OpenCodexCustomConfig | null>(
    null,
  );
  const [codexRuntime, setCodexRuntime] =
    useState<CodexRuntimeStatus | null>(null);
  const [desktopIntegration, setDesktopIntegration] =
    useState<CodexDesktopIntegrationStatus | null>(null);
  const [loading, setLoading] = useState(false);
  const [busy, setBusy] = useState(false);
  const [runtimePickerWorking, setRuntimePickerWorking] = useState(false);
  const [runtimePickerResult, setRuntimePickerResult] =
    useState<CodexPickerUnlockResult | null>(null);
  const [preview, setPreview] = useState<ProxyModelsPreview | null>(null);
  const [routeTestModel, setRouteTestModel] = useState("");
  const [routeTestWorking, setRouteTestWorking] = useState(false);
  const [routeTestResult, setRouteTestResult] =
    useState<ProxyRouteTestResult | null>(null);
  const [injectPreview, setInjectPreview] =
    useState<InjectBaseUrlPreview | null>(null);
  const [restorePreview, setRestorePreview] =
    useState<RestoreBaseUrlPreview | null>(null);
  const [effectiveRouting, setEffectiveRouting] =
    useState<EffectiveRoutingStatus | null>(null);
  const [conversationProviders, setConversationProviders] =
    useState<ConversationProviderCandidatesView | null>(null);
  const [conversationDraft, setConversationDraft] =
    useState<ConversationProviderDraft>({
      providerId: DEFAULT_MULTIROUTER_PROVIDER_ID,
      displayName: "Codex Box 代理",
      proxyPort: 1455,
      wireApi: "responses",
      requiresOpenaiAuth: true,
      originalBaseUrl: "",
    });
  const [conversationPreview, setConversationPreview] =
    useState<ConversationProviderPreview | null>(null);
  const [runtimeRouterPreview, setRuntimeRouterPreview] =
    useState<CodexMultirouterPreview | null>(null);
  const [importSources, setImportSources] = useState<ConfigImportSource[]>([]);
  const [runtimeImportPreview, setRuntimeImportPreview] =
    useState<ConfigImportPreview | null>(null);
  const [runtimeImportWorking, setRuntimeImportWorking] = useState(false);
  const [showAdvancedRuntime, setShowAdvancedRuntime] = useState(false);
  const activeProxyPort = proxyPortOrDefault(proxy);

  const refresh = useCallback(async () => {
    setLoading(true);
    const statusR = await invokeCmd<ProxyStatusView>("proxy_status");
    if (statusR.ok) setProxy(statusR.data);
    const runtimeR = await invokeCmd<CodexRuntimeStatus>("codex_runtime_status");
    if (runtimeR.ok) setCodexRuntime(runtimeR.data);
    const desktopR = await invokeCmd<CodexDesktopIntegrationStatus>(
      "codex_desktop_integration_status",
    );
    if (desktopR.ok) setDesktopIntegration(desktopR.data);
    const routingR = await invokeCmd<EffectiveRoutingStatus>(
      "effective_routing_status",
    );
    if (routingR.ok) setEffectiveRouting(routingR.data);
    const ocR = await invokeCmd<OpenCodexCustomConfig>("opencodex_config_read");
    if (ocR.ok) setOpencodex(ocR.data);
    const importSourcesR = await invokeCmd<ConfigImportSource[]>(
      "config_import_sources_scan",
    );
    if (importSourcesR.ok) setImportSources(importSourcesR.data);
    const cpR = await invokeCmd<ConversationProviderCandidatesView>(
      "conversation_provider_candidates",
    );
    if (cpR.ok) {
      setConversationProviders(cpR.data);
      const active =
        cpR.data.candidates.find(
          (candidate) => candidate.providerId === cpR.data.activeProviderId,
        ) || cpR.data.candidates[0];
      if (active) {
        setConversationDraft(
          draftFromConversationCandidate(active, activeProxyPort),
        );
      }
    }
    setLoading(false);
  }, [activeProxyPort]);

  useEffect(() => {
    void refresh();
  }, [refresh]);

  const start = useCallback(async () => {
    setBusy(true);
    const r = await invokeCmd<{ port: number; status: string }>("proxy_start", {
      request: { port: 1455 },
    });
    setBusy(false);
    if (r.ok) {
      show(
        "success",
        t("pages.codexBoxRuntime.notice.started", { port: r.data.port }),
      );
      void refresh();
    } else {
      show("warning", r.error);
    }
  }, [refresh, show, t]);

  const stop = useCallback(async () => {
    setBusy(true);
    const r = await invokeCmd<ProxyStatusView>("proxy_stop");
    setBusy(false);
    if (r.ok) {
      show("success", t("pages.codexBoxRuntime.notice.stopped"));
      void refresh();
    } else {
      show("warning", r.error);
    }
  }, [refresh, show, t]);

  const restart = useCallback(async () => {
    setBusy(true);
    const r = await invokeCmd<{ port: number; status: string }>(
      "proxy_restart",
      { request: { port: 1455 } },
    );
    setBusy(false);
    if (r.ok) {
      show(
        "success",
        t("pages.codexBoxRuntime.notice.restarted", { port: r.data.port }),
      );
      void refresh();
    } else {
      show("warning", r.error);
    }
  }, [refresh, show, t]);

  const modelsPreview = useCallback(async () => {
    setBusy(true);
    const r = await invokeCmd<ProxyModelsPreview>("proxy_models_preview");
    setBusy(false);
    if (r.ok) {
      setPreview(r.data);
      show("success", t("pages.codexBoxRuntime.notice.previewOk"));
    } else {
      show("warning", r.error);
    }
  }, [show, t]);

  const runRouteTest = useCallback(async (performUpstream = false) => {
    const modelId =
      routeTestModel.trim() ||
      effectiveRouting?.currentModel ||
      opencodex?.catalog.find((model) => model.visible)?.modelId ||
      "";
    if (!modelId) {
      show("warning", t("pages.codexBoxRuntime.routeTest.noModel"));
      return;
    }
    setRouteTestWorking(true);
    const result = await invokeCmd<ProxyRouteTestResult>("proxy_route_test", {
      request: {
        modelId,
        prompt: "Codex Box route test",
        includeImage: true,
        performUpstream,
      },
    });
    setRouteTestWorking(false);
    if (result.ok) {
      setRouteTestModel(modelId);
      setRouteTestResult(result.data);
      show(
        result.data.status === "passed" ? "success" : "warning",
        result.data.status === "passed"
          ? performUpstream
            ? t("pages.codexBoxRuntime.routeTest.upstreamPassed")
            : t("pages.codexBoxRuntime.routeTest.passed")
          : performUpstream
            ? t("pages.codexBoxRuntime.routeTest.upstreamFailed")
            : t("pages.codexBoxRuntime.routeTest.failed"),
      );
    } else {
      show("warning", result.error);
    }
  }, [effectiveRouting?.currentModel, opencodex?.catalog, routeTestModel, show, t]);

  const previewRuntimeMultirouter = useCallback(
    async (proxyPort: number) => {
      const previewResult = await invokeCmd<CodexMultirouterPreview>(
        "codex_multirouter_preview",
        {
          request: {
            proxyPort,
            routerProviderId: DEFAULT_MULTIROUTER_PROVIDER_ID,
            ensureCodexConfig: true,
          },
        },
      );

      if (previewResult.ok) {
        setRuntimeRouterPreview(previewResult.data);
        show("info", t("pages.codexBoxRuntime.notice.multirouterPreviewOk"));
        return true;
      }

      show("warning", previewResult.error);
      return false;
    },
    [show, t],
  );

  const connectCodex = useCallback(async () => {
    setBusy(true);
    const shouldStartProxy =
      (proxy?.status ?? "stopped") === "stopped" ||
      (proxy?.status ?? "stopped") === "failed";
    let proxyPort = conversationDraft.proxyPort || activeProxyPort;

    if (shouldStartProxy) {
      const startResult = await invokeCmd<{ port: number; status: string }>(
        "proxy_start",
        { request: { port: proxyPort } },
      );
      if (!startResult.ok) {
        setBusy(false);
        show("warning", startResult.error);
        return;
      }
      proxyPort = startResult.data.port;
    }

    const previewOk = await previewRuntimeMultirouter(proxyPort);
    setBusy(false);

    if (previewOk) {
      void refresh();
    }
  }, [
    activeProxyPort,
    conversationDraft.proxyPort,
    previewRuntimeMultirouter,
    proxy?.status,
    refresh,
    show,
  ]);

  const previewRuntimeImport = useCallback(async () => {
    setRuntimeImportWorking(true);
    const result = await invokeCmd<ConfigImportPreview>(
      "opencodex_import_preview",
    );
    setRuntimeImportWorking(false);
    if (result.ok) {
      setRuntimeImportPreview(result.data);
      show("info", t("pages.codexBoxRuntime.import.previewReady"));
    } else {
      show("warning", result.error);
    }
  }, [show, t]);

  const applyRuntimeImport = useCallback(async () => {
    if (!runtimeImportPreview) return;
    setRuntimeImportWorking(true);
    const result = await invokeCmd<ApplyConfigImportResult>(
      "opencodex_import_apply",
      {
        request: {
          preview: runtimeImportPreview,
          confirmed: true,
        },
      },
    );
    setRuntimeImportWorking(false);
    if (result.ok) {
      setRuntimeImportPreview(null);
      const proxyPort = conversationDraft.proxyPort || activeProxyPort;
      const previewOk = await previewRuntimeMultirouter(proxyPort);
      show(
        previewOk ? "success" : "warning",
        previewOk
          ? t("pages.codexBoxRuntime.import.applyDone")
          : t("pages.codexBoxRuntime.import.applyDoneNeedsConnect"),
      );
      void refresh();
    } else {
      show("warning", result.error);
    }
  }, [
    activeProxyPort,
    conversationDraft.proxyPort,
    previewRuntimeMultirouter,
    refresh,
    runtimeImportPreview,
    show,
    t,
  ]);

  const applyRuntimeMultirouterPreview = useCallback(async () => {
    if (!runtimeRouterPreview) return;
    setBusy(true);
    const result = await invokeCmd<CodexMultirouterSyncResult>(
      "codex_multirouter_apply",
      {
        request: {
          preview: runtimeRouterPreview,
          confirmed: true,
        },
      },
    );
    setBusy(false);
    if (result.ok) {
      setRuntimeRouterPreview(null);
      setRuntimePickerWorking(true);
      const pickerResult = await invokeCmd<CodexPickerUnlockResult>(
        "codex_desktop_picker_unlock",
      );
      setRuntimePickerWorking(false);
      if (pickerResult.ok) {
        setRuntimePickerResult(pickerResult.data);
      }
      show(
        pickerResult.ok && !pickerResult.data.injected ? "warning" : "success",
        pickerResult.ok && pickerResult.data.injected
          ? t("feedback.multirouterSyncedAndPickerUnlocked", {
              routes: result.data.routeCount,
              models: result.data.routedModelCount,
            })
          : t("feedback.multirouterSyncedPickerPending", {
              routes: result.data.routeCount,
              models: result.data.routedModelCount,
              message: pickerResult.ok
                ? pickerResult.data.message
                : pickerResult.error,
            }),
      );
      void refresh();
    } else {
      show("warning", result.error);
    }
  }, [refresh, runtimeRouterPreview, show, t]);

  const unlockRuntimePicker = useCallback(
    async (
      command:
        | "codex_desktop_picker_unlock"
        | "codex_desktop_launch_with_debugging_and_unlock",
    ) => {
      setRuntimePickerWorking(true);
      const result = await invokeCmd<CodexPickerUnlockResult>(command);
      setRuntimePickerWorking(false);
      if (result.ok) {
        setRuntimePickerResult(result.data);
        show(
          result.data.injected ? "success" : "warning",
          result.data.injected
            ? command === "codex_desktop_launch_with_debugging_and_unlock"
              ? t("pages.codexBoxRuntime.desktopPicker.launchDone")
              : t("pages.codexBoxRuntime.desktopPicker.done")
            : result.data.message,
        );
        void refresh();
      } else {
        show("warning", result.error);
      }
    },
    [refresh, show, t],
  );

  const injectPreviewFn = useCallback(async () => {
    setBusy(true);
    const r = await invokeCmd<InjectBaseUrlPreview>(
      "proxy_inject_base_url_preview",
      {
        request: { port: activeProxyPort },
      },
    );
    setBusy(false);
    if (r.ok) {
      setInjectPreview(r.data);
    } else {
      show("warning", r.error);
    }
  }, [activeProxyPort, show]);

  const injectApply = useCallback(async () => {
    if (!injectPreview) return;
    setBusy(true);
    const r = await invokeCmd<ApplyInjectResult>(
      "proxy_inject_base_url_apply",
      {
        request: { preview: injectPreview, confirmed: true },
      },
    );
    setBusy(false);
    if (r.ok) {
      show("success", t("pages.codexBoxRuntime.notice.injectOk"));
      setInjectPreview(null);
      void refresh();
    } else {
      show("warning", r.error);
    }
  }, [injectPreview, refresh, show, t]);

  const restorePreviewFn = useCallback(async () => {
    setBusy(true);
    const r = await invokeCmd<RestoreBaseUrlPreview>(
      "proxy_restore_base_url_preview",
      {},
    );
    setBusy(false);
    if (r.ok) {
      setRestorePreview(r.data);
    } else {
      show("warning", r.error);
    }
  }, [show]);

  const restoreApply = useCallback(async () => {
    if (!restorePreview) return;
    setBusy(true);
    const r = await invokeCmd<ApplyRestoreResult>(
      "proxy_restore_base_url_apply",
      {
        request: { preview: restorePreview, confirmed: true },
      },
    );
    setBusy(false);
    if (r.ok) {
      show("success", t("pages.codexBoxRuntime.notice.restoreOk"));
      setRestorePreview(null);
      void refresh();
    } else {
      show("warning", r.error);
    }
  }, [refresh, restorePreview, show, t]);

  const selectConversationProvider = useCallback(
    (providerId: string) => {
      const candidate = conversationProviders?.candidates.find(
        (item) => item.providerId === providerId,
      );
      if (!candidate) {
        setConversationDraft((current) => ({ ...current, providerId }));
        return;
      }
      setConversationDraft(
        draftFromConversationCandidate(candidate, activeProxyPort),
      );
      setConversationPreview(null);
    },
    [activeProxyPort, conversationProviders?.candidates],
  );

  const previewConversationProvider = useCallback(async () => {
    const providerId = conversationDraft.providerId.trim();
    if (!providerId) {
      show("warning", t("feedback.conversationProviderInvalid"));
      return;
    }
    setBusy(true);
    const r = await invokeCmd<ConversationProviderPreview>(
      "conversation_provider_preview",
      {
        request: {
          providerId,
          displayName: conversationDraft.displayName.trim() || null,
          proxyPort: conversationDraft.proxyPort || activeProxyPort,
          wireApi: conversationDraft.wireApi.trim() || "responses",
          requiresOpenaiAuth: conversationDraft.requiresOpenaiAuth,
          originalBaseUrl: conversationDraft.originalBaseUrl.trim() || null,
        },
      },
    );
    setBusy(false);
    if (r.ok) {
      setConversationPreview(r.data);
      show("info", t("feedback.previewDiff"));
    } else {
      show("warning", r.error);
    }
  }, [activeProxyPort, conversationDraft, show, t]);

  const applyConversationProvider = useCallback(async () => {
    if (!conversationPreview) return;
    setBusy(true);
    const r = await invokeCmd<ApplyConversationProviderResult>(
      "conversation_provider_apply",
      {
        request: { preview: conversationPreview, confirmed: true },
      },
    );
    setBusy(false);
    if (r.ok) {
      show(
        "success",
        t("feedback.conversationProviderSaved", {
          name: conversationPreview.providerId,
        }),
      );
      setConversationPreview(null);
      void refresh();
    } else {
      show("warning", r.error);
    }
  }, [conversationPreview, refresh, show, t]);

  const statusName = proxy?.status ?? "stopped";
  const statusTone: StatusTone =
    statusName === "running"
      ? "running"
      : statusName === "starting"
        ? "warn"
        : statusName === "failed"
          ? "fail"
          : "idle";
  const port = proxy?.port ?? 0;
  const canStart = statusName === "stopped" || statusName === "failed";
  const canStop = statusName === "running" || statusName === "starting";
  const failIssues =
    effectiveRouting?.issues.filter((issue) => issue.severity === "fail") ??
    [];
  const warnIssues =
    effectiveRouting?.issues.filter((issue) => issue.severity === "warn") ??
    [];
  const visibleModelCount =
    opencodex?.catalog.filter((model) => model.visible).length ?? 0;
  const enabledProviderCount =
    opencodex?.providers.filter((provider) => provider.enabled).length ?? 0;
  const activationIssueCodes = new Set([
    "catalog_not_configured",
    "request_entry_not_configured",
  ]);
  const hasActivationIssue =
    effectiveRouting?.issues.some((issue) =>
      activationIssueCodes.has(issue.code),
    ) ?? false;
  const activationIssueCount =
    effectiveRouting?.issues.filter((issue) =>
      activationIssueCodes.has(issue.code),
    ).length ?? 0;
  const blockingFailCount = failIssues.filter(
    (issue) => !activationIssueCodes.has(issue.code),
  ).length;
  const displayWarnCount = warnIssues.length + activationIssueCount;
  const hasBlockingIssue = failIssues.some(
    (issue) => !activationIssueCodes.has(issue.code),
  );
  const requestBaseUrl = effectiveRouting?.requestBaseUrl || null;
  const localEndpoint =
    port > 0
      ? `http://127.0.0.1:${port}/v1`
      : `http://127.0.0.1:${activeProxyPort}/v1`;
  const isLocalRequestEntry =
    Boolean(requestBaseUrl) &&
    (requestBaseUrl!.includes("127.0.0.1") ||
      requestBaseUrl!.includes("localhost"));
  const readinessKey = !effectiveRouting
    ? "unknown"
    : hasBlockingIssue
      ? "blocked"
      : !effectiveRouting.proxyRunning
        ? "proxyStopped"
        : hasActivationIssue || !isLocalRequestEntry
          ? "notEnabled"
          : warnIssues.length > 0
            ? "needsAttention"
            : "ready";
  const readinessTone: StatusTone =
    readinessKey === "ready"
      ? "running"
      : readinessKey === "blocked"
        ? "fail"
        : readinessKey === "unknown"
          ? "idle"
          : "warn";
  const visibleModels = opencodex?.catalog.filter((model) => model.visible) ?? [];
  const blockingIssues = effectiveRouting?.issues.filter((issue) =>
    issue.severity === "fail" && !activationIssueCodes.has(issue.code),
  ) ?? [];
  const visibleIssues = blockingIssues.length > 0 ? blockingIssues : warnIssues;
  const missingBackendProviderIds = useMemo(() => {
    if (!opencodex) return [];
    const providers = new Set(opencodex.providers.map((provider) => provider.name));
    return Array.from(
      new Set(
        opencodex.catalog
          .map((model) => model.backendProvider)
          .filter((provider): provider is string =>
            Boolean(
              provider &&
                provider !== "openai" &&
                !COMPAT_MULTIROUTER_PROVIDER_IDS.has(provider) &&
                !providers.has(provider),
            ),
          ),
      ),
    );
  }, [opencodex]);
  const opencodexImportSource = importSources.find(
    (source) => source.id === "opencodex" && source.canImport,
  );
  const shouldShowRuntimeImport =
    Boolean(opencodexImportSource) &&
    ((opencodex?.providers.length ?? 0) === 0 ||
      missingBackendProviderIds.length > 0);
  const desktopPickerTone: StatusTone = runtimePickerResult?.injected
    ? "ok"
    : desktopIntegration?.codexRunning &&
        desktopIntegration.codexRemoteDebuggingPort === null
      ? "warn"
      : desktopIntegration?.codexRemoteDebuggingPort
        ? "running"
        : "idle";
  const desktopPickerModelCount =
    (desktopIntegration?.customCatalogNativeOpenaiModelCount ?? 0) +
    (desktopIntegration?.customCatalogByokModelCount ?? 0);

  return (
    <PageShell
      title={t("pages.codexBoxRuntime.title")}
      subtitle={t("pages.codexBoxRuntime.subtitle")}
      notice={notice}
      action={
        <div className="flex items-center gap-2">
          <ToolbarButton
            icon={<RotateCcw size={13} />}
            onClick={() => void refresh()}
            disabled={loading}
          >
            {t("actions.refresh")}
          </ToolbarButton>
        </div>
      }
    >
      <section className="cb-surface p-5">
        <div className="flex flex-col gap-5 lg:flex-row lg:items-center lg:justify-between">
          <div className="min-w-0">
            <div className="flex items-center gap-2">
              <StatusPill tone={readinessTone} />
              <span className="text-[12px] font-medium text-ink-500">
                {canStop ? localEndpoint : t("pages.codexBoxRuntime.notRunning")}
              </span>
            </div>
            <h2 className="mt-3 text-[26px] font-semibold leading-tight text-ink-900">
              {t(`pages.codexBoxRuntime.readiness.headline.${readinessKey}`)}
            </h2>
            <p className="mt-2 max-w-[620px] text-[13px] leading-[1.7] text-ink-500">
              {t(`pages.codexBoxRuntime.readiness.message.${readinessKey}`)}
            </p>
          </div>
          <div className="flex shrink-0 flex-wrap items-center gap-2">
            <button
              type="button"
              onClick={() => void connectCodex()}
              disabled={busy || visibleModelCount === 0}
              className="inline-flex h-11 items-center justify-center gap-2 rounded-lg bg-ink-900 px-5 text-[13px] font-semibold text-white shadow-sm transition hover:bg-ink-800 active:translate-y-px disabled:cursor-not-allowed disabled:opacity-45"
            >
              <Play size={15} />
              {t("pages.codexBoxRuntime.primaryAction")}
            </button>
            {canStop ? (
              <ToolbarButton
                icon={<Square size={13} />}
                onClick={() => void stop()}
                disabled={busy}
              >
                {t("pages.codexBoxRuntime.stop")}
              </ToolbarButton>
            ) : null}
          </div>
        </div>
      </section>

      <div className="grid grid-cols-1 gap-4 lg:grid-cols-[minmax(0,1fr)_360px]">
        <Panel
          title={t("pages.codexBoxRuntime.modelsTitle")}
          icon={<Sparkles size={15} />}
          action={
            <span className="text-[11px] text-ink-500">
              {t("pages.codexBoxRuntime.modelsCount", {
                count: visibleModelCount,
              })}
            </span>
          }
        >
          {visibleModels.length > 0 ? (
            <div className="grid grid-cols-1 gap-2 md:grid-cols-2">
              {visibleModels.slice(0, 6).map((model) => (
                <div
                  key={model.modelId}
                  className="rounded-md border border-ink-900/[0.06] bg-white/40 px-3 py-2.5"
                >
                  <div className="truncate text-[13px] font-semibold text-ink-900">
                    {model.displayName || model.modelId}
                  </div>
                  <div className="mt-0.5 truncate text-[11px] text-ink-500">
                    {model.provider}
                  </div>
                </div>
              ))}
            </div>
          ) : (
            <div className="rounded-md border border-dashed border-ink-900/[0.08] bg-white/35 px-4 py-6 text-center text-[12px] text-ink-500">
              {t("pages.codexBoxRuntime.modelsEmpty")}
            </div>
          )}
        </Panel>

        <Panel
          title={t("pages.codexBoxRuntime.statusTitle")}
          icon={<ShieldCheck size={15} />}
          action={<StatusPill tone={readinessTone} />}
        >
          <DetailRow
            label={t("pages.codexBoxRuntime.effectiveRouting.model")}
            value={
              <span className="font-mono break-all">
                {effectiveRouting?.currentModel || "-"}
              </span>
            }
          />
          <DetailRow
            label={t("pages.codexBoxRuntime.effectiveRouting.apiService")}
            value={
              <span className="font-mono break-all">
                {effectiveRouting?.backendProvider || "-"}
              </span>
            }
          />
          <DetailRow
            label={t("pages.codexBoxRuntime.uptime")}
            value={
              proxy && proxy.uptimeMs !== null ? format_uptime(proxy.uptimeMs) : "-"
            }
          />
        </Panel>
      </div>

      <Panel
        title={t("pages.codexBoxRuntime.routeTest.title")}
        icon={<TestTube2 size={15} />}
        help={t("pages.codexBoxRuntime.routeTest.help")}
        action={
          <div className="flex flex-wrap items-center justify-end gap-2">
            <ToolbarButton
              icon={<TestTube2 size={13} />}
              disabled={routeTestWorking || visibleModelCount === 0}
              onClick={() => void runRouteTest(false)}
            >
              {routeTestWorking
                ? t("actions.running")
                : t("pages.codexBoxRuntime.routeTest.run")}
            </ToolbarButton>
            <ToolbarButton
              icon={<Network size={13} />}
              disabled={routeTestWorking || visibleModelCount === 0}
              onClick={() => void runRouteTest(true)}
            >
              {routeTestWorking
                ? t("actions.running")
                : t("pages.codexBoxRuntime.routeTest.runUpstream")}
            </ToolbarButton>
          </div>
        }
      >
        <div className="grid grid-cols-1 gap-3 xl:grid-cols-[320px_minmax(0,1fr)]">
          <div className="space-y-3">
            <label className="block text-[11px] font-medium text-ink-500">
              {t("pages.codexBoxRuntime.routeTest.model")}
            </label>
            <select
              value={
                routeTestModel ||
                effectiveRouting?.currentModel ||
                visibleModels[0]?.modelId ||
                ""
              }
              onChange={(event) => {
                setRouteTestModel(event.target.value);
                setRouteTestResult(null);
              }}
              className="h-10 w-full rounded-lg border border-ink-900/[0.08] bg-white/70 px-3 text-[13px] font-medium text-ink-900 outline-none transition focus:border-accent-500/40 focus:ring-2 focus:ring-accent-500/10"
            >
              {visibleModels.length === 0 ? (
                <option value="">
                  {t("pages.codexBoxRuntime.routeTest.noModel")}
                </option>
              ) : null}
              {visibleModels.map((model) => (
                <option key={model.modelId} value={model.modelId}>
                  {model.displayName || model.modelId}
                </option>
              ))}
            </select>
            <div className="rounded-md border border-ink-900/[0.06] bg-white/35 px-3 py-2 text-[12px] leading-[1.55] text-ink-500">
              {t("pages.codexBoxRuntime.routeTest.dryRun")}
            </div>
          </div>
          <div className="min-w-0">
            {routeTestResult ? (
              <div className="space-y-3">
                <div className="grid grid-cols-1 gap-3 md:grid-cols-4">
                  <DetailRow
                    label={t("pages.codexBoxRuntime.routeTest.provider")}
                    value={routeTestResult.providerName || "-"}
                  />
                  <DetailRow
                    label={t("pages.codexBoxRuntime.routeTest.upstream")}
                    value={
                      <span className="font-mono break-all">
                        {routeTestResult.upstreamModel || "-"}
                      </span>
                    }
                  />
                  <DetailRow
                    label={t("pages.codexBoxRuntime.routeTest.mode")}
                    value={
                      routeTestResult.usedChatFallback
                        ? "responses -> chat"
                        : routeTestResult.wireApi || "-"
                    }
                  />
                  <DetailRow
                    label={t("pages.codexBoxRuntime.routeTest.upstreamStatus")}
                    value={
                      routeTestResult.upstreamStatusCode
                        ? `${routeTestResult.upstreamStatusCode} / ${
                            routeTestResult.upstreamLatencyMs ?? "-"
                          }ms`
                        : "-"
                    }
                  />
                </div>
                <div className="divide-y divide-ink-900/[0.06] rounded-md border border-ink-900/[0.06] bg-white/35">
                  {routeTestResult.steps.map((step) => (
                    <div
                      key={step.id}
                      className="grid grid-cols-[84px_minmax(0,1fr)] gap-3 px-3 py-2.5"
                    >
                      <div className="flex items-center gap-2">
                        <StatusPill tone={routeTestStepTone(step.status)} />
                        <span className="text-[11px] font-medium text-ink-500">
                          {step.status}
                        </span>
                      </div>
                      <div className="min-w-0">
                        <div className="text-[12px] font-semibold text-ink-900">
                          {step.label}
                        </div>
                        <div className="mt-0.5 text-[12px] leading-[1.55] text-ink-500">
                          {step.detail}
                        </div>
                      </div>
                    </div>
                  ))}
                </div>
                {routeTestResult.warnings.length > 0 ? (
                  <div className="rounded-md border border-status-warn/20 bg-status-warn/[0.08] px-3 py-2 text-[12px] leading-[1.55] text-ink-700">
                    {routeTestResult.warnings.join(" / ")}
                  </div>
                ) : null}
              </div>
            ) : (
              <div className="rounded-md border border-dashed border-ink-900/[0.08] bg-white/35 px-4 py-6 text-center text-[12px] text-ink-500">
                {t("pages.codexBoxRuntime.routeTest.empty")}
              </div>
            )}
          </div>
        </div>
      </Panel>

      {visibleIssues.length > 0 || proxy?.lastError ? (
        <Panel
          title={t("pages.codexBoxRuntime.issueTitle")}
          icon={<AlertTriangle size={15} />}
          action={
            <span className="text-[11px] text-ink-500">
              {blockingFailCount} fail / {displayWarnCount} warn
            </span>
          }
        >
          <div className="flex flex-col gap-2">
            {proxy?.lastError ? (
              <div className="rounded-md border border-status-fail/20 bg-status-fail/[0.06] px-3 py-2 text-[12px] leading-[1.55] text-ink-700">
                {proxy.lastError}
              </div>
            ) : null}
            {visibleIssues.map((issue) => (
              <div
                key={`${issue.code}-${issue.message}`}
                className="flex items-start gap-2 rounded-md border border-ink-900/[0.06] bg-white/35 px-3 py-2 text-[12px] leading-[1.55]"
              >
                <StatusPill tone={routingIssueTone(issue.severity)} />
                <span className="min-w-0 text-ink-700">{issue.message}</span>
              </div>
            ))}
          </div>
        </Panel>
      ) : null}

      {shouldShowRuntimeImport && opencodexImportSource ? (
        <Panel
          title={t("pages.codexBoxRuntime.import.title")}
          icon={<FolderOpen size={15} />}
          help={t("pages.codexBoxRuntime.import.help")}
          action={
            <ToolbarButton
              icon={<GitCompare size={13} />}
              disabled={busy || runtimeImportWorking}
              onClick={() => void previewRuntimeImport()}
            >
              {runtimeImportWorking
                ? t("actions.running")
                : t("pages.codexBoxRuntime.import.preview")}
            </ToolbarButton>
          }
        >
          <div className="grid grid-cols-1 gap-3 xl:grid-cols-2">
            <div>
              <DetailRow
                label={t("pages.codexBoxRuntime.import.source")}
                value={<PathValue value={opencodexImportSource.path} />}
              />
              <DetailRow
                label={t("pages.codexBoxRuntime.import.available")}
                value={t("pages.codexBoxRuntime.import.availableValue", {
                  providers: opencodexImportSource.providers,
                  models: opencodexImportSource.models,
                })}
              />
              <DetailRow
                label={t("pages.codexBoxRuntime.import.missing")}
                value={
                  missingBackendProviderIds.length > 0
                    ? missingBackendProviderIds.join(", ")
                    : t("pages.codexBoxRuntime.import.providersEmpty")
                }
              />
            </div>
            <div>
              <DetailRow
                label={t("pages.codexBoxRuntime.import.action")}
                value={opencodexImportSource.recommendedAction}
              />
              <DetailRow
                label={t("pages.codexBoxRuntime.import.warning")}
                value={
                  opencodexImportSource.warnings[0] ||
                  t("pages.codexBoxRuntime.import.noWarning")
                }
              />
            </div>
          </div>
          {runtimeImportPreview ? (
            <div className="mt-3 space-y-3">
              <div className="grid grid-cols-1 gap-3 md:grid-cols-2">
                <DetailRow
                  label={t("pages.codexBoxRuntime.import.providersDiff")}
                  value={t("pages.codexBoxRuntime.import.diffCount", {
                    count: runtimeImportPreview.providersDiff.length,
                  })}
                />
                <DetailRow
                  label={t("pages.codexBoxRuntime.import.catalogDiff")}
                  value={t("pages.codexBoxRuntime.import.diffCount", {
                    count: runtimeImportPreview.catalogDiff.length,
                  })}
                />
              </div>
              {runtimeImportPreview.warnings.length > 0 ? (
                <div className="rounded-md border border-status-warn/20 bg-status-warn/[0.08] px-3 py-2 text-[12px] leading-[1.55] text-ink-700">
                  {runtimeImportPreview.warnings.join(" / ")}
                </div>
              ) : null}
              <DiffBlock
                lines={[
                  ...runtimeImportPreview.providersDiff,
                  ...runtimeImportPreview.catalogDiff,
                ].slice(0, 80)}
              />
              <div className="flex justify-end gap-2">
                <ToolbarButton
                  onClick={() => setRuntimeImportPreview(null)}
                  disabled={runtimeImportWorking}
                >
                  {t("actions.cancel")}
                </ToolbarButton>
                <ToolbarButton
                  icon={<Save size={13} />}
                  variant="primary"
                  onClick={() => void applyRuntimeImport()}
                  disabled={runtimeImportWorking}
                >
                  {t("pages.codexBoxRuntime.import.apply")}
                </ToolbarButton>
              </div>
            </div>
          ) : null}
        </Panel>
      ) : null}

      <Panel
        title={t("pages.codexBoxRuntime.desktopPicker.title")}
        icon={<Sparkles size={15} />}
        help={t("pages.codexBoxRuntime.desktopPicker.help")}
        action={
          <div className="flex flex-wrap items-center justify-end gap-2">
            <ToolbarButton
              icon={<Sparkles size={13} />}
              disabled={
                busy || runtimePickerWorking || desktopPickerModelCount === 0
              }
              onClick={() =>
                void unlockRuntimePicker("codex_desktop_picker_unlock")
              }
            >
              {runtimePickerWorking
                ? t("actions.running")
                : t("pages.codexBoxRuntime.desktopPicker.inject")}
            </ToolbarButton>
            <ToolbarButton
              icon={<Play size={13} />}
              variant="primary"
              disabled={
                busy || runtimePickerWorking || desktopPickerModelCount === 0
              }
              onClick={() =>
                void unlockRuntimePicker(
                  "codex_desktop_launch_with_debugging_and_unlock",
                )
              }
            >
              {runtimePickerWorking
                ? t("actions.running")
                : t("pages.codexBoxRuntime.desktopPicker.launch")}
            </ToolbarButton>
          </div>
        }
      >
        <div className="grid grid-cols-1 gap-3 xl:grid-cols-2">
          <div>
            <DetailRow
              label={t("pages.codexBoxRuntime.desktopPicker.status")}
              value={
                <div className="flex items-center gap-2">
                  <StatusPill tone={desktopPickerTone} />
                  <span>
                    {runtimePickerResult?.status ||
                      (desktopIntegration?.codexRunning
                        ? "desktop_running"
                        : "desktop_not_running")}
                  </span>
                </div>
              }
            />
            <DetailRow
              label={t("pages.codexBoxRuntime.desktopPicker.process")}
              value={
                desktopIntegration
                  ? `${desktopIntegration.codexRunning ? "running" : "not running"} / cdp=${desktopIntegration.codexRemoteDebuggingPort ?? "-"}`
                  : "-"
              }
            />
            <DetailRow
              label={t("pages.codexBoxRuntime.desktopPicker.catalog")}
              value={
                desktopIntegration
                  ? `native=${desktopIntegration.customCatalogNativeOpenaiModelCount} / byok=${desktopIntegration.customCatalogByokModelCount}`
                  : "-"
              }
            />
          </div>
          <div>
            <DetailRow
              label={t("pages.codexBoxRuntime.desktopPicker.lastResult")}
              value={
                runtimePickerResult?.message ||
                t("pages.codexBoxRuntime.desktopPicker.noResult")
              }
            />
            <DetailRow
              label={t("pages.codexBoxRuntime.desktopPicker.targets")}
              value={
                runtimePickerResult
                  ? `${runtimePickerResult.injectedTargetCount}/${runtimePickerResult.targetCount}`
                  : "-"
              }
            />
            <DetailRow
              label={t("pages.codexBoxRuntime.desktopPicker.models")}
              value={
                runtimePickerResult
                  ? `${runtimePickerResult.modelCount} (${runtimePickerResult.modelNames.slice(0, 5).join(", ") || "-"})`
                  : String(desktopPickerModelCount)
              }
            />
            <DetailRow
              label={t("pages.codexBoxRuntime.desktopPicker.renderer")}
              value={formatPickerRendererReports(runtimePickerResult)}
            />
          </div>
        </div>
      </Panel>

      {runtimeRouterPreview && (
        <div
          className="fixed inset-0 z-50 flex items-center justify-center bg-ink-900/15 px-4"
          role="dialog"
          aria-modal="true"
          aria-labelledby="multirouter-dialog-title"
          onClick={() => {
            if (!busy) setRuntimeRouterPreview(null);
          }}
        >
          <div
            className="max-h-[86vh] w-full max-w-[920px] overflow-y-auto rounded-xl border border-white/70 bg-white/95 p-5 shadow-card backdrop-blur-xl cb-scroll"
            onClick={(event) => event.stopPropagation()}
          >
            <div className="flex items-start justify-between gap-4">
              <div>
                <h3
                  id="multirouter-dialog-title"
                  className="text-[16px] font-semibold text-ink-900"
                >
                  {t("pages.codexBoxRuntime.multirouterPlan")}
                </h3>
                <p className="mt-1 text-[12px] leading-[1.6] text-ink-500">
                  {t("pages.codexBoxRuntime.multirouterBody")}
                </p>
              </div>
              <StatusPill tone="warn" />
            </div>
            <div className="mt-4">
              <SummaryStrip
                items={[
                  {
                    label: t("pages.models.routerProvider"),
                    value: runtimeRouterPreview.routerProviderId,
                  },
                  {
                    label: t("pages.models.routerRoutes"),
                    value: String(runtimeRouterPreview.routeCount),
                  },
                  {
                    label: t("pages.models.routerModels"),
                    value: String(runtimeRouterPreview.routedModelCount),
                  },
                  {
                    label: t("pages.models.routerEndpoint"),
                    value: runtimeRouterPreview.proxyBaseUrl,
                  },
                  {
                    label: t("pages.models.routerCache"),
                    value: runtimeRouterPreview.modelsCacheTouched
                      ? t("common.yes")
                      : t("common.no"),
                  },
                  {
                    label: t("pages.models.routerInjectMap"),
                    value: runtimeRouterPreview.injectMapTouched
                      ? t("common.yes")
                      : t("common.no"),
                  },
                ]}
              />
            </div>
            {runtimeRouterPreview.skippedModels.length > 0 ? (
              <div className="mt-3 rounded-md border border-status-warn/25 bg-status-warn/[0.08] px-3 py-2 text-[12px] leading-[1.55] text-ink-700">
                {t("pages.codexBoxRuntime.multirouterSkipped", {
                  count: runtimeRouterPreview.skippedModels.length,
                })}
              </div>
            ) : null}
            <div className="mt-4 grid grid-cols-1 gap-3 lg:grid-cols-2">
              <details className="rounded-md border border-ink-900/[0.06] bg-ink-900/[0.02]" open>
                <summary className="cursor-pointer px-3 py-2 text-[12px] font-medium text-ink-600">
                  {t("pages.models.routerProvidersDiff")}
                </summary>
                <div className="border-t border-ink-900/[0.06] p-3">
                  <PathValue value={runtimeRouterPreview.providersPath} />
                  <div className="mt-2">
                    <DiffBlock lines={runtimeRouterPreview.providersDiff} />
                  </div>
                </div>
              </details>
              <details className="rounded-md border border-ink-900/[0.06] bg-ink-900/[0.02]" open>
                <summary className="cursor-pointer px-3 py-2 text-[12px] font-medium text-ink-600">
                  {t("pages.models.routerCatalogDiff")}
                </summary>
                <div className="border-t border-ink-900/[0.06] p-3">
                  <PathValue value={runtimeRouterPreview.catalogPath} />
                  <div className="mt-2">
                    <DiffBlock lines={runtimeRouterPreview.catalogDiff} />
                  </div>
                </div>
              </details>
              <details className="rounded-md border border-ink-900/[0.06] bg-ink-900/[0.02]" open>
                <summary className="cursor-pointer px-3 py-2 text-[12px] font-medium text-ink-600">
                  {t("pages.models.routerConfigDiff")}
                </summary>
                <div className="border-t border-ink-900/[0.06] p-3">
                  <PathValue value={runtimeRouterPreview.configPath} />
                  <div className="mt-2">
                    <DiffBlock lines={runtimeRouterPreview.configDiff} />
                  </div>
                </div>
              </details>
              <details className="rounded-md border border-ink-900/[0.06] bg-ink-900/[0.02]">
                <summary className="cursor-pointer px-3 py-2 text-[12px] font-medium text-ink-600">
                  {t("pages.models.routerModelsCacheDiff")}
                </summary>
                <div className="border-t border-ink-900/[0.06] p-3">
                  <PathValue value={runtimeRouterPreview.modelsCachePath} />
                  <div className="mt-2">
                    <DiffBlock lines={runtimeRouterPreview.modelsCacheDiff} />
                  </div>
                </div>
              </details>
              <details className="rounded-md border border-ink-900/[0.06] bg-ink-900/[0.02]">
                <summary className="cursor-pointer px-3 py-2 text-[12px] font-medium text-ink-600">
                  {t("pages.models.routerInjectMapDiff")}
                </summary>
                <div className="border-t border-ink-900/[0.06] p-3">
                  <PathValue value={runtimeRouterPreview.injectMapPath} />
                  <div className="mt-2">
                    <DiffBlock lines={runtimeRouterPreview.injectMapDiff} />
                  </div>
                </div>
              </details>
            </div>
            <div className="mt-5 flex justify-end gap-2">
              <ToolbarButton
                onClick={() => setRuntimeRouterPreview(null)}
                disabled={busy}
              >
                {t("actions.cancel")}
              </ToolbarButton>
              <ToolbarButton
                variant="primary"
                onClick={() => void applyRuntimeMultirouterPreview()}
                disabled={busy}
              >
                {t("pages.codexBoxRuntime.multirouterConfirm")}
              </ToolbarButton>
            </div>
          </div>
        </div>
      )}

      <div className="cb-surface p-4">
        <div className="flex flex-col gap-3 sm:flex-row sm:items-center sm:justify-between">
          <div className="min-w-0">
            <div className="flex items-center gap-2 text-[13px] font-semibold text-ink-900">
              <Settings size={14} />
              {t("pages.codexBoxRuntime.advanced.title")}
              <InfoTip
                label={t("pages.codexBoxRuntime.advanced.help")}
                content={t("pages.codexBoxRuntime.advanced.hint")}
              />
            </div>
          </div>
          <ToolbarButton
            icon={<Eye size={13} />}
            onClick={() => setShowAdvancedRuntime((value) => !value)}
          >
            {showAdvancedRuntime
              ? t("actions.hideDetails")
              : t("actions.showDetails")}
          </ToolbarButton>
        </div>
      </div>

      {showAdvancedRuntime && (
        <>
          <Panel
            title={t("pages.codexBoxRuntime.conversationProvider.title")}
            icon={<Users size={15} />}
            help={t("pages.codexBoxRuntime.conversationProvider.hint")}
            action={
              <ToolbarButton
                icon={<GitCompare size={13} />}
                onClick={() => void previewConversationProvider()}
                disabled={busy}
              >
                {t("pages.codexBoxRuntime.conversationProvider.preview")}
              </ToolbarButton>
            }
          >
        <div className="grid grid-cols-1 gap-3 lg:grid-cols-3">
          <FormField
            label={t("pages.codexBoxRuntime.conversationProvider.history")}
          >
            <select
              className={inputClass}
              value={conversationDraft.providerId}
              onChange={(event) =>
                selectConversationProvider(event.target.value)
              }
            >
              {(conversationProviders?.candidates || []).map((candidate) => (
                <option
                  key={`${candidate.providerId}-${candidate.sourcePath}`}
                  value={candidate.providerId}
                >
                  {formatConversationProviderOption(candidate)}
                </option>
              ))}
              {!conversationProviders?.candidates.some(
                (candidate) =>
                  candidate.providerId === conversationDraft.providerId,
              ) && (
                <option value={conversationDraft.providerId}>
                  {conversationDraft.providerId}
                </option>
              )}
            </select>
          </FormField>
          <FormField label="Provider ID">
            <input
              className={inputClass}
              value={conversationDraft.providerId}
              onChange={(event) => {
                setConversationDraft((current) => ({
                  ...current,
                  providerId: event.target.value,
                }));
                setConversationPreview(null);
              }}
              placeholder={DEFAULT_MULTIROUTER_PROVIDER_ID}
            />
          </FormField>
          <FormField label={t("fields.displayName")}>
            <input
              className={inputClass}
              value={conversationDraft.displayName}
              onChange={(event) => {
                setConversationDraft((current) => ({
                  ...current,
                  displayName: event.target.value,
                }));
                setConversationPreview(null);
              }}
              placeholder="OpenAI / Custom Provider"
            />
          </FormField>
          <FormField label="wire_api">
            <select
              className={inputClass}
              value={conversationDraft.wireApi}
              onChange={(event) => {
                setConversationDraft((current) => ({
                  ...current,
                  wireApi: event.target.value,
                }));
                setConversationPreview(null);
              }}
            >
              <option value="responses">responses</option>
              <option value="chat">chat</option>
            </select>
          </FormField>
          <FormField label={t("pages.codexBoxRuntime.port")}>
            <input
              className={inputClass}
              type="number"
              min={1}
              max={65535}
              value={conversationDraft.proxyPort}
              onChange={(event) => {
                setConversationDraft((current) => ({
                  ...current,
                  proxyPort: Number(event.target.value) || 1455,
                }));
                setConversationPreview(null);
              }}
            />
          </FormField>
          <FormField
            label={t(
              "pages.codexBoxRuntime.conversationProvider.originalBaseUrl",
            )}
          >
            <input
              className={inputClass}
              value={conversationDraft.originalBaseUrl}
              onChange={(event) =>
                setConversationDraft((current) => ({
                  ...current,
                  originalBaseUrl: event.target.value,
                }))
              }
              placeholder="可为空;仅用于回显历史来源"
            />
          </FormField>
        </div>
        <label className="mt-3 flex items-start gap-2 text-[12px] text-ink-700">
          <input
            type="checkbox"
            className="mt-0.5"
            checked={conversationDraft.requiresOpenaiAuth}
            onChange={(event) => {
              setConversationDraft((current) => ({
                ...current,
                requiresOpenaiAuth: event.target.checked,
              }));
              setConversationPreview(null);
            }}
          />
          <span>
            {t("pages.codexBoxRuntime.conversationProvider.requiresOpenaiAuth")}
          </span>
        </label>
        <div className="mt-3 grid grid-cols-1 gap-2 lg:grid-cols-2">
          <DetailRow
            label={t("pages.codexBoxRuntime.conversationProvider.proxyBaseUrl")}
            value={
              <span className="font-mono break-all">{`http://127.0.0.1:${conversationDraft.proxyPort || 1455}/v1`}</span>
            }
          />
          <DetailRow
            label={t("pages.codexBoxRuntime.conversationProvider.source")}
            value={
              <span className="font-mono break-all">
                {selectedConversationProviderSource(
                  conversationProviders,
                  conversationDraft.providerId,
                )}
              </span>
            }
          />
        </div>
        {conversationPreview && (
          <div className="mt-4 space-y-2">
            <DetailRow
              label={t("pages.codexBoxRuntime.injectDiffSummary")}
              value={`+${conversationPreview.insertions} / -${conversationPreview.deletions}`}
            />
            <DiffBlock lines={conversationPreview.diff} />
            <div className="flex justify-end gap-2">
              <ToolbarButton
                onClick={() => setConversationPreview(null)}
                disabled={busy}
              >
                {t("actions.cancel")}
              </ToolbarButton>
              <ToolbarButton
                variant="primary"
                onClick={() => void applyConversationProvider()}
                disabled={busy}
              >
                {t("pages.codexBoxRuntime.conversationProvider.confirm")}
              </ToolbarButton>
            </div>
          </div>
        )}
      </Panel>

      <div className="grid grid-cols-1 lg:grid-cols-2 gap-4">
        <Panel
          title={t("pages.codexBoxRuntime.controlPanel")}
          icon={<Server size={15} />}
        >
          <DetailRow
            label={t("pages.codexBoxRuntime.stateTitle")}
            value={<StatusPill tone={statusTone} />}
          />
          <DetailRow
            label={t("pages.codexBoxRuntime.endpoint")}
            value={
              port > 0 ? (
                <span className="font-mono break-all">{`http://127.0.0.1:${port}/v1`}</span>
              ) : (
                <span className="text-ink-500">
                  {t("pages.codexBoxRuntime.notRunning")}
                </span>
              )
            }
          />
          <DetailRow
            label={t("pages.codexBoxRuntime.startedAt")}
            value={proxy?.startedAt || "-"}
          />
          <DetailRow
            label={t("pages.codexBoxRuntime.healthz")}
            value={
              port > 0 ? (
                <a
                  className="font-mono text-[#0A84FF] underline-offset-2 hover:underline"
                  href={`http://127.0.0.1:${port}/healthz`}
                  target="_blank"
                  rel="noreferrer"
                >
                  {`http://127.0.0.1:${port}/healthz`}
                </a>
              ) : (
                <span className="text-ink-500">-</span>
              )
            }
          />
          <DetailRow
            label={t("fields.codexHome")}
            value={
              <span className="font-mono break-all">
                {codexRuntime?.codexHome || "-"}
              </span>
            }
          />
          <DetailRow
            label={t("fields.codexCliPath")}
            value={
              <span className="font-mono break-all">
                {codexRuntime?.codexCliPath || "-"}
              </span>
            }
          />
          <DetailRow
            label={t("fields.codexDesktopAppPath")}
            value={
              <span className="font-mono break-all">
                {codexRuntime?.codexDesktopAppPath || "-"}
              </span>
            }
          />
          <DetailRow
            label={t("fields.codexDesktopVersion")}
            value={codexRuntime?.codexDesktopVersion || "-"}
          />
          <DetailRow
            label={t("fields.configReadable")}
            value={
              codexRuntime
                ? codexRuntime.configReadable
                  ? t("common.yes")
                  : t("common.no")
                : "-"
            }
          />
          <DetailRow
            label="auth.json"
            value={
              codexRuntime
                ? codexRuntime.authStateDetected
                  ? t("common.yes")
                  : t("common.no")
                : "-"
            }
          />
          <DetailRow
            label={t("fields.opencodexDir")}
            value={
              <span className="font-mono break-all">
                {codexRuntime?.opencodexDir || "-"}
              </span>
            }
          />
        </Panel>

        <Panel
          title={t("pages.codexBoxRuntime.previewPanel")}
          icon={<Eye size={15} />}
          action={
            <ToolbarButton
              icon={<Eye size={13} />}
              onClick={() => void modelsPreview()}
              disabled={busy || !canStop}
            >
              {t("pages.codexBoxRuntime.preview")}
            </ToolbarButton>
          }
        >
          {preview ? (
            <div className="space-y-2">
              <DetailRow
                label={t("pages.codexBoxRuntime.previewUrl")}
                value={
                  <span className="font-mono break-all">{preview.baseUrl}</span>
                }
              />
              <div className="rounded-md border border-ink-900/[0.06] bg-ink-900/[0.02] p-3 max-h-72 overflow-y-auto cb-scroll">
                <pre className="font-mono text-[11px] leading-[1.5] text-ink-800 whitespace-pre-wrap break-all">
                  {JSON.stringify(preview.rawJson, null, 2)}
                </pre>
              </div>
            </div>
          ) : (
            <p className="text-[12px] text-ink-500">
              {t("pages.codexBoxRuntime.previewHint")}
            </p>
          )}
        </Panel>
      </div>

      <div className="grid grid-cols-1 lg:grid-cols-2 gap-4">
        <Panel
          title={t("pages.codexBoxRuntime.injectPanel")}
          icon={<Route size={15} />}
          help={t("pages.codexBoxRuntime.injectHint")}
          helpSide="top"
          action={
            <ToolbarButton
              icon={<GitCompare size={13} />}
              onClick={() => void injectPreviewFn()}
              disabled={busy}
            >
              {t("pages.codexBoxRuntime.injectPreview")}
            </ToolbarButton>
          }
        >
          {injectPreview && (
            <div className="space-y-2">
              <DetailRow
                label={t("pages.codexBoxRuntime.injectDiffSummary")}
                value={`+${injectPreview.insertions} / -${injectPreview.deletions}`}
              />
              <DiffBlock lines={injectPreview.diff} />
              <div className="flex justify-end gap-2">
                <ToolbarButton
                  onClick={() => setInjectPreview(null)}
                  disabled={busy}
                >
                  {t("actions.cancel")}
                </ToolbarButton>
                <ToolbarButton
                  variant="primary"
                  onClick={() => void injectApply()}
                  disabled={busy}
                >
                  {t("pages.codexBoxRuntime.injectConfirm")}
                </ToolbarButton>
              </div>
            </div>
          )}
        </Panel>

        <Panel
          title={t("pages.codexBoxRuntime.restorePanel")}
          icon={<RotateCcw size={15} />}
          help={t("pages.codexBoxRuntime.restoreHint")}
          helpSide="top"
          action={
            <ToolbarButton
              icon={<GitCompare size={13} />}
              onClick={() => void restorePreviewFn()}
              disabled={busy}
            >
              {t("pages.codexBoxRuntime.restorePreview")}
            </ToolbarButton>
          }
        >
          {restorePreview && (
            <div className="space-y-2">
              <DetailRow
                label={t("pages.codexBoxRuntime.injectDiffSummary")}
                value={`+${restorePreview.insertions} / -${restorePreview.deletions}`}
              />
              <DiffBlock lines={restorePreview.diff} />
              <div className="flex justify-end gap-2">
                <ToolbarButton
                  onClick={() => setRestorePreview(null)}
                  disabled={busy}
                >
                  {t("actions.cancel")}
                </ToolbarButton>
                <ToolbarButton
                  variant="primary"
                  onClick={() => void restoreApply()}
                  disabled={busy}
                >
                  {t("pages.codexBoxRuntime.restoreConfirm")}
                </ToolbarButton>
              </div>
            </div>
          )}
        </Panel>
      </div>

      <Panel
        title={t("pages.codexBoxRuntime.routeTable")}
        icon={<Network size={15} />}
        action={
          <span className="text-[11px] text-ink-500">
            {t("pages.codexBoxRuntime.routeTableCount", {
              count: proxy?.providers.length ?? 0,
            })}
          </span>
        }
      >
        {(proxy?.providers.length ?? 0) === 0 ? (
          <p className="text-[12px] text-ink-500">
            {t("pages.codexBoxRuntime.routeTableEmpty")}
          </p>
        ) : (
          <div className="overflow-x-auto">
            <table className="w-full text-[12px]">
              <thead>
                <tr className="text-left text-ink-500">
                  <th className="px-2 py-2 font-medium">
                    {t("pages.codexBoxRuntime.col.name")}
                  </th>
                  <th className="px-2 py-2 font-medium">
                    {t("pages.codexBoxRuntime.col.upstream")}
                  </th>
                  <th className="px-2 py-2 font-medium">
                    {t("pages.codexBoxRuntime.col.wire")}
                  </th>
                  <th className="px-2 py-2 font-medium">
                    {t("pages.codexBoxRuntime.col.envKey")}
                  </th>
                  <th className="px-2 py-2 font-medium">
                    {t("pages.codexBoxRuntime.col.kind")}
                  </th>
                  <th className="px-2 py-2 font-medium">
                    {t("pages.codexBoxRuntime.col.models")}
                  </th>
                </tr>
              </thead>
              <tbody>
                {proxy!.providers.map((p) => (
                  <tr key={p.name} className="border-t border-ink-900/[0.06]">
                    <td className="px-2 py-2 font-mono text-ink-800">
                      {p.name}
                    </td>
                    <td className="px-2 py-2 font-mono text-[11px] text-ink-700 break-all">
                      {p.originalBaseUrl}
                    </td>
                    <td className="px-2 py-2 font-mono text-[11px]">
                      {p.wireApi}
                    </td>
                    <td className="px-2 py-2 font-mono text-[11px]">
                      {p.envKey || "-"}
                    </td>
                    <td className="px-2 py-2 text-[11px]">{p.kind}</td>
                    <td className="px-2 py-2 text-[11px]">{p.models.length}</td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        )}
      </Panel>
        </>
      )}

    </PageShell>
  );
}

function format_uptime(ms: number): string {
  if (ms < 1000) return `${ms}ms`;
  const s = Math.floor(ms / 1000);
  if (s < 60) return `${s}s`;
  const m = Math.floor(s / 60);
  if (m < 60) return `${m}m ${s % 60}s`;
  const h = Math.floor(m / 60);
  return `${h}h ${m % 60}m`;
}

function proxyPortOrDefault(proxy: ProxyStatusView | null): number {
  const port = proxy?.port ?? 0;
  return port > 0 ? port : 1455;
}

function routingIssueTone(severity: string): StatusTone {
  if (severity === "fail") return "fail";
  if (severity === "warn") return "warn";
  if (severity === "info") return "idle";
  return "idle";
}

function routeTestStepTone(status: string): StatusTone {
  if (status === "passed") return "ok";
  if (status === "failed") return "fail";
  if (status === "warning") return "warn";
  return "idle";
}

function draftFromConversationCandidate(
  candidate: ConversationProviderCandidate,
  proxyPort: number,
): ConversationProviderDraft {
  return {
    providerId: candidate.providerId,
    displayName: candidate.displayName || candidate.providerId,
    proxyPort,
    wireApi: candidate.wireApi || "responses",
    requiresOpenaiAuth: candidate.requiresOpenaiAuth ?? true,
    originalBaseUrl: candidate.originalBaseUrl || "",
  };
}

function formatConversationProviderOption(
  candidate: ConversationProviderCandidate,
): string {
  const name =
    candidate.displayName && candidate.displayName !== candidate.providerId
      ? ` · ${candidate.displayName}`
      : "";
  return `${candidate.providerId}${name} · ${candidate.sourceKind}`;
}

function selectedConversationProviderSource(
  view: ConversationProviderCandidatesView | null,
  providerId: string,
): string {
  return (
    view?.candidates.find((candidate) => candidate.providerId === providerId)
      ?.sourcePath || "-"
  );
}

function severityToTone(severity: string): StatusTone {
  if (severity === "fail") return "fail";
  if (severity === "warn") return "warn";
  if (severity === "ok") return "ok";
  return "idle";
}

function routingCodesTone(
  routing: EffectiveRoutingStatus | null,
  codes: string[],
): StatusTone {
  const issue = routing?.issues.find((item) => codes.includes(item.code));
  return issue ? severityToTone(issue.severity) : "ok";
}

function desktopIssue(
  desktop: CodexDesktopIntegrationStatus | null,
  code: string,
) {
  return desktop?.issues.find((item) => item.code === code) ?? null;
}

function pickerReadinessTone(
  desktop: CodexDesktopIntegrationStatus | null,
): StatusTone {
  if (!desktop) return "idle";
  if (desktop.pickerReadinessStatus === "ready") return "ok";
  if (desktop.pickerReadinessStatus === "blocked") return "fail";
  return "warn";
}

function buildDiagnosticsGroups(
  routing: EffectiveRoutingStatus | null,
  desktop: CodexDesktopIntegrationStatus | null,
  history: CodexHistoryReconcileView | null,
): DiagnosticGroupView[] {
  if (!routing && !desktop && !history) {
    return [];
  }

  const configParsed = desktop?.configParsed ?? false;
  const rendererIssue = desktopIssue(
    desktop,
    "codex_renderer_picker_filter_risk",
  );
  const authIssue = desktopIssue(desktop, "native_openai_auth_unverified");
  const officialRouteIssue =
    desktopIssue(desktop, "official_managed_route_missing") ||
    desktopIssue(desktop, "official_managed_route_auth_unmanaged") ||
    desktopIssue(desktop, "official_managed_route_base_url_unexpected");
  const providerSchemaIssue = desktopIssue(
    desktop,
    "router_requires_openai_auth_false",
  );
  const proxyBearerIssue = desktopIssue(
    desktop,
    "router_proxy_managed_bearer_missing",
  );
  const catalogCoverageIssue = desktopIssue(
    desktop,
    "custom_model_catalog_empty",
  );
  const requestEntryTone = routingCodesTone(routing, [
    "request_entry_not_configured",
    "model_provider_missing_base_url",
    "invalid_proxy_port_zero",
    "proxy_not_running",
    "proxy_port_mismatch",
  ]);
  const upstreamAuthTone = routingCodesTone(routing, [
    "upstream_api_key_plaintext_ignored",
    "upstream_api_key_env_missing",
    "upstream_api_key_missing",
  ]);

  const groups: DiagnosticGroupView[] = [
    {
      id: "config",
      titleKey: "diagnostics.groups.config",
      items: [
        {
          id: "syntax",
          labelKey: "diagnostics.items.syntax",
          detail: desktop?.configError || desktop?.configPath || "-",
          status: configParsed ? "ok" : "fail",
        },
        {
          id: "activeModel",
          labelKey: "diagnostics.items.activeModel",
          detail: `model=${desktop?.model || routing?.currentModel || "-"} / provider=${desktop?.modelProvider || routing?.modelProvider || "-"}`,
          status: desktop?.modelProvider || routing?.modelProvider ? "ok" : "warn",
        },
        {
          id: "modelCatalogJson",
          labelKey: "diagnostics.items.modelCatalogJson",
          detail: desktop?.modelCatalogJson || routing?.modelCatalogPath || "-",
          status:
            desktop?.modelCatalogJson || routing?.modelCatalogPath
              ? "ok"
              : "warn",
        },
        {
          id: "modelCatalogCoverage",
          labelKey: "diagnostics.items.modelCatalogCoverage",
          detail: `file=${desktop?.customModelCatalogExists ? "yes" : "no"} / native_gpt=${desktop?.customCatalogNativeOpenaiModelCount ?? "-"} / byok=${desktop?.customCatalogByokModelCount ?? "-"}`,
          status: catalogCoverageIssue
            ? severityToTone(catalogCoverageIssue.severity)
            : desktop?.customModelCatalogExists
              ? "ok"
              : "warn",
        },
      ],
    },
    {
      id: "desktop",
      titleKey: "diagnostics.groups.desktop",
      items: [
        {
          id: "pickerReadiness",
          labelKey: "diagnostics.items.pickerReadiness",
          detail:
            desktop?.pickerReadinessSummary ||
            "Picker readiness has not been checked yet.",
          status: pickerReadinessTone(desktop),
        },
        {
          id: "codexProcess",
          labelKey: "diagnostics.items.codexProcess",
          detail: desktop?.codexRunning
            ? `running / cdp=${desktop.codexRemoteDebuggingPort ?? "-"}`
            : "not running",
          status: desktop?.codexRunning ? "ok" : "idle",
        },
        {
          id: "rendererFilter",
          labelKey: "diagnostics.items.rendererFilter",
          detail:
            rendererIssue?.message ||
            "No renderer whitelist risk detected from current process state.",
          status: rendererIssue ? severityToTone(rendererIssue.severity) : "ok",
        },
        {
          id: "officialAuth",
          labelKey: "diagnostics.items.officialAuth",
          detail: `auth_mode=${desktop?.authMode || "-"} / chatgpt=${desktop?.chatgptAuthLikely ? "yes" : "no"}`,
          status:
            authIssue || !desktop?.authJsonExists
              ? "warn"
              : desktop?.chatgptAuthLikely
                ? "ok"
                : "warn",
        },
        {
          id: "officialRoute",
          labelKey: "diagnostics.items.officialRoute",
          detail:
            officialRouteIssue?.message ||
            `configured=${desktop?.officialRouteConfigured ? "yes" : "no"} / auth=${desktop?.officialRouteAuthSource || "-"} / models=${desktop?.officialRouteModelCount ?? "-"} / base=${desktop?.officialRouteBaseUrl || "-"}`,
          status: officialRouteIssue
            ? severityToTone(officialRouteIssue.severity)
            : (desktop?.customCatalogNativeOpenaiModelCount ?? 0) > 0
              ? desktop?.officialRouteConfigured
                ? "ok"
                : "warn"
              : "idle",
        },
        {
          id: "providerSchema",
          labelKey: "diagnostics.items.providerSchema",
          detail:
            providerSchemaIssue?.message ||
            proxyBearerIssue?.message ||
            `requires_openai_auth=${desktop?.routerProviderRequiresOpenaiAuth ?? "-"} / supports_websockets=${desktop?.routerProviderSupportsWebsockets ?? "-"} / proxy_managed=${desktop?.routerProviderUsesProxyManagedBearer ?? "-"}`,
          status: providerSchemaIssue || proxyBearerIssue
            ? severityToTone(
                (providerSchemaIssue || proxyBearerIssue)!.severity,
              )
            : "ok",
        },
        {
          id: "modelsCache",
          labelKey: "diagnostics.items.modelsCache",
          detail: `exists=${desktop?.modelsCacheExists ? "yes" : "no"} / owned=${desktop?.modelsCacheOwnedByCodexBox ? "codex-box" : "native"} / models=${desktop?.modelsCacheModelCount ?? "-"}`,
          status:
            desktop?.modelsCacheExists && desktop.modelsCacheClientVersionPresent
              ? "ok"
              : "warn",
        },
      ],
    },
    {
      id: "byok",
      titleKey: "diagnostics.groups.byok",
      items: [
        {
          id: "requestEntry",
          labelKey: "diagnostics.items.requestEntry",
          detail: `${routing?.requestBaseUrlSource || "-"} -> ${routing?.requestBaseUrl || "-"}`,
          status: requestEntryTone,
        },
        {
          id: "proxyRuntime",
          labelKey: "diagnostics.items.proxyRuntime",
          detail: `running=${routing?.proxyRunning ? "yes" : "no"} / port=${routing?.proxyPort ?? "-"}`,
          status: routing?.proxyRunning ? "ok" : "fail",
        },
        {
          id: "catalogRoute",
          labelKey: "diagnostics.items.catalogRoute",
          detail: `catalog=${routing?.catalogConfigured ? "yes" : "no"} / model_found=${routing?.catalogModelFound ? "yes" : "no"} / backend=${routing?.backendProvider || "-"}/${routing?.backendModel || "-"}`,
          status:
            routing?.catalogConfigured && routing.catalogModelFound
              ? "ok"
              : "warn",
        },
        {
          id: "upstreamAuth",
          labelKey: "diagnostics.items.upstreamAuth",
          detail: routing?.upstreamBaseUrl || routing?.backendProvider || "-",
          status: upstreamAuthTone,
        },
      ],
    },
  ];

  if (history) {
    groups.push({
      id: "history",
      titleKey: "diagnostics.groups.history",
      items: [
        {
          id: "liveHistoryProvider",
          labelKey: "diagnostics.items.liveHistoryProvider",
          detail: `live=${history.liveConfigModelProvider || "-"} / target=${history.suggestedTargetProvider}`,
          status: history.liveConfigModelProvider ? "ok" : "warn",
        },
        {
          id: "activeStateDb",
          labelKey: "diagnostics.items.activeStateDb",
          detail: `${history.activeStateDbKind || "-"} -> ${history.activeStateDbPath || "-"}`,
          status: history.activeStateDbPath ? "ok" : "warn",
        },
        {
          id: "sqliteHistoryBuckets",
          labelKey: "diagnostics.items.sqliteHistoryBuckets",
          detail: formatHistorySqliteStores(history),
          status: history.driftDetected ? "warn" : "ok",
        },
        {
          id: "jsonlHistoryBuckets",
          labelKey: "diagnostics.items.jsonlHistoryBuckets",
          detail: `files=${history.jsonlSummary.totalFiles} / ${formatProviderCounts(history.jsonlSummary.providerCounts)}`,
          status: history.jsonlSummary.unreadableFiles > 0 ? "warn" : "ok",
        },
        {
          id: "historyRepairPreview",
          labelKey: "diagnostics.items.historyRepairPreview",
          detail: `sources=${history.sourceProviderIds.join(", ") || "-"} / sqlite_rows=${history.providerRowsToUpdate} / jsonl_lines=${history.rolloutProviderLinesToUpdate}`,
          status:
            history.providerRowsToUpdate > 0 ||
            history.rolloutProviderLinesToUpdate > 0
              ? "warn"
              : "ok",
        },
      ],
    });
  }

  const issueItems = [
    ...(routing?.issues.map((issue) => ({
      id: `routing-${issue.code}`,
      labelKey: "diagnostics.items.issue",
      detail: issue.message,
      status: severityToTone(issue.severity),
    })) ?? []),
    ...(desktop?.issues.map((issue) => ({
      id: `desktop-${issue.code}`,
      labelKey: "diagnostics.items.issue",
      detail: issue.message,
      status: severityToTone(issue.severity),
    })) ?? []),
    ...(history?.warnings.map((issue) => ({
      id: `history-${issue.code}`,
      labelKey: "diagnostics.items.issue",
      detail: issue.message,
      status: severityToTone(issue.severity),
    })) ?? []),
  ];

  if (issueItems.length > 0) {
    groups.push({
      id: "issues",
      titleKey: "diagnostics.groups.issues",
      items: issueItems,
    });
  }

  return groups;
}

function formatProviderCounts(counts: Record<string, number>): string {
  const entries = Object.entries(counts).filter(([, count]) => count > 0);
  if (entries.length === 0) return "-";
  return entries
    .sort((a, b) => b[1] - a[1] || a[0].localeCompare(b[0]))
    .map(([provider, count]) => `${provider}:${count}`)
    .join(" / ");
}

function formatHistorySqliteStores(history: CodexHistoryReconcileView): string {
  if (history.sqliteStores.length === 0) return "-";
  return history.sqliteStores
    .map((store) => {
      const counts = formatProviderCounts(store.providerCounts);
      return `${store.kind}:${store.total} (${counts})`;
    })
    .join(" · ");
}

function formatCountSummary(
  items: Array<[label: string, count: number]>,
  emptyLabel: string,
): string {
  const parts = items
    .filter(([, count]) => count > 0)
    .map(([label, count]) => `${label}=${count}`);
  return parts.length > 0 ? parts.join(" / ") : emptyLabel;
}

export function DiagnosticsPage() {
  const { t } = useTranslation();
  const { notice, show } = useNotice();
  const [running, setRunning] = useState(true);
  const [routing, setRouting] = useState<EffectiveRoutingStatus | null>(null);
  const [desktop, setDesktop] =
    useState<CodexDesktopIntegrationStatus | null>(null);
  const [history, setHistory] =
    useState<CodexHistoryReconcileView | null>(null);
  const [historyPreview, setHistoryPreview] =
    useState<CodexHistoryUnifyPreview | null>(null);
  const [historyApplyResult, setHistoryApplyResult] =
    useState<CodexHistoryUnifyApplyResult | null>(null);
  const [historyWorking, setHistoryWorking] = useState(false);
  const [pickerUnlockWorking, setPickerUnlockWorking] = useState(false);
  const [pickerUnlockResult, setPickerUnlockResult] =
    useState<CodexPickerUnlockResult | null>(null);

  const groups = useMemo(
    () => buildDiagnosticsGroups(routing, desktop, history),
    [desktop, history, routing],
  );
  const total = groups.reduce(
    (sum, group) => sum + group.items.length,
    0,
  );
  const warn = groups
    .flatMap((group) => group.items)
    .filter((item) => item.status === "warn").length;
  const fail = groups
    .flatMap((group) => group.items)
    .filter((item) => item.status === "fail").length;

  const refresh = useCallback(
    async (notify = false, preserveHistoryToolState = false) => {
      setRunning(true);
      const [routingResult, desktopResult, historyResult] = await Promise.all([
        invokeCmd<EffectiveRoutingStatus>("effective_routing_status"),
        invokeCmd<CodexDesktopIntegrationStatus>(
          "codex_desktop_integration_status",
        ),
        invokeCmd<CodexHistoryReconcileView>("codex_history_reconcile"),
      ]);
      setRunning(false);

      let nextRouting: EffectiveRoutingStatus | null = null;
      let nextDesktop: CodexDesktopIntegrationStatus | null = null;
      let nextHistory: CodexHistoryReconcileView | null = null;
      if (routingResult.ok) {
        nextRouting = routingResult.data;
        setRouting(routingResult.data);
      } else {
        show("warning", routingResult.error);
      }
      if (desktopResult.ok) {
        nextDesktop = desktopResult.data;
        setDesktop(desktopResult.data);
      } else {
        show("warning", desktopResult.error);
      }
      if (historyResult.ok) {
        nextHistory = historyResult.data;
        setHistory(historyResult.data);
        if (!preserveHistoryToolState) {
          setHistoryPreview(null);
          setHistoryApplyResult(null);
        }
      } else {
        show("warning", historyResult.error);
      }
      if (notify) {
        const nextTotal = buildDiagnosticsGroups(
          nextRouting,
          nextDesktop,
          nextHistory,
        ).reduce((sum, group) => sum + group.items.length, 0);
        show("success", t("feedback.diagnosticsDone", { total: nextTotal }));
      }
    },
    [show, t],
  );

  const historyRequest = useMemo(
    () => ({
      targetProvider: history?.suggestedTargetProvider ?? null,
      sourceProviderIds: history?.sourceProviderIds ?? null,
      projectPath: null,
      force: false,
    }),
    [history],
  );

  const previewHistoryUnify = useCallback(async () => {
    setHistoryWorking(true);
    const result = await invokeCmd<CodexHistoryUnifyPreview>(
      "codex_history_unify_preview",
      { request: historyRequest },
    );
    setHistoryWorking(false);
    if (result.ok) {
      setHistoryPreview(result.data);
      setHistoryApplyResult(null);
      show(
        result.data.canApply ? "success" : "warning",
        result.data.canApply
          ? t("pages.diagnostics.historyUnify.previewReady")
          : result.data.warnings[0]?.message ||
              t("pages.diagnostics.historyUnify.previewBlocked"),
      );
    } else {
      show("warning", result.error);
    }
  }, [historyRequest, show, t]);

  const applyHistoryUnify = useCallback(async () => {
    setHistoryWorking(true);
    const result = await invokeCmd<CodexHistoryUnifyApplyResult>(
      "codex_history_unify_apply",
      { request: historyRequest },
    );
    setHistoryWorking(false);
    if (result.ok) {
      setHistoryApplyResult(result.data);
      setHistoryPreview(result.data.preview);
      show("success", t("pages.diagnostics.historyUnify.applyDone"));
      void refresh(false, true);
    } else {
      show("warning", result.error);
    }
  }, [historyRequest, refresh, show, t]);

  const unlockPicker = useCallback(async () => {
    setPickerUnlockWorking(true);
    const result = await invokeCmd<CodexPickerUnlockResult>(
      "codex_desktop_picker_unlock",
    );
    setPickerUnlockWorking(false);
    if (result.ok) {
      setPickerUnlockResult(result.data);
      show(
        result.data.injected ? "success" : "warning",
        result.data.injected
          ? t("pages.diagnostics.pickerUnlock.done")
          : result.data.message,
      );
      void refresh(false, true);
    } else {
      show("warning", result.error);
    }
  }, [refresh, show, t]);

  const launchAndUnlockPicker = useCallback(async () => {
    setPickerUnlockWorking(true);
    const result = await invokeCmd<CodexPickerUnlockResult>(
      "codex_desktop_launch_with_debugging_and_unlock",
    );
    setPickerUnlockWorking(false);
    if (result.ok) {
      setPickerUnlockResult(result.data);
      show(
        result.data.injected ? "success" : "warning",
        result.data.injected
          ? t("pages.diagnostics.pickerUnlock.launchDone")
          : result.data.message,
      );
      void refresh(false, true);
    } else {
      show("warning", result.error);
    }
  }, [refresh, show, t]);

  useEffect(() => {
    void refresh(false);
  }, [refresh]);

  const noHistoryChanges = t("pages.diagnostics.historyUnify.none");
  const previewImpact = historyPreview
    ? formatCountSummary(
        [
          [
            t("pages.diagnostics.historyUnify.counts.providerRows"),
            historyPreview.providerRowsToUpdate,
          ],
          [
            t("pages.diagnostics.historyUnify.counts.rolloutFiles"),
            historyPreview.rolloutFilesToUpdate,
          ],
          [
            t("pages.diagnostics.historyUnify.counts.rolloutLines"),
            historyPreview.rolloutProviderLinesToUpdate,
          ],
          [
            t("pages.diagnostics.historyUnify.counts.userEvents"),
            historyPreview.userEventRowsToUpdate,
          ],
        ],
        noHistoryChanges,
      )
    : t("pages.diagnostics.historyUnify.noPreview");
  const previewVisibility = historyPreview
    ? formatCountSummary(
        [
          [
            t("pages.diagnostics.historyUnify.counts.visibleRows"),
            historyPreview.visibleCandidateRows,
          ],
          [
            t("pages.diagnostics.historyUnify.counts.sessionIndex"),
            historyPreview.sessionIndexMissingToAppend,
          ],
          [
            t("pages.diagnostics.historyUnify.counts.focusRows"),
            historyPreview.focusRowsToMove,
          ],
        ],
        noHistoryChanges,
      )
    : t("pages.diagnostics.historyUnify.noPreview");
  const previewGlobalState = historyPreview
    ? formatCountSummary(
        [
          [
            t("pages.diagnostics.historyUnify.counts.workspaceHints"),
            historyPreview.workspaceHintsToFix,
          ],
          [
            t("pages.diagnostics.historyUnify.counts.projectless"),
            historyPreview.projectlessIdsToRemove,
          ],
          [
            t("pages.diagnostics.historyUnify.counts.savedRoots"),
            historyPreview.savedWorkspaceRootsToAdd,
          ],
        ],
        noHistoryChanges,
      )
    : t("pages.diagnostics.historyUnify.noPreview");
  const applyProviderResult = historyApplyResult
    ? formatCountSummary(
        [
          [
            t("pages.diagnostics.historyUnify.counts.providerRows"),
            historyApplyResult.providerRowsUpdated,
          ],
          [
            t("pages.diagnostics.historyUnify.counts.rolloutFiles"),
            historyApplyResult.rolloutFilesUpdated,
          ],
          [
            t("pages.diagnostics.historyUnify.counts.rolloutLines"),
            historyApplyResult.rolloutProviderLinesUpdated,
          ],
          [
            t("pages.diagnostics.historyUnify.counts.userEvents"),
            historyApplyResult.userEventRowsUpdated,
          ],
        ],
        noHistoryChanges,
      )
    : null;
  const applyVisibilityResult = historyApplyResult
    ? formatCountSummary(
        [
          [
            t("pages.diagnostics.historyUnify.counts.focusRows"),
            historyApplyResult.focusRowsUpdated,
          ],
          [
            t("pages.diagnostics.historyUnify.counts.sessionIndex"),
            historyApplyResult.sessionIndexAppended,
          ],
          [
            t("pages.diagnostics.historyUnify.counts.indexMoved"),
            historyApplyResult.sessionIndexRowsMoved,
          ],
          [
            t("pages.diagnostics.historyUnify.counts.indexTitles"),
            historyApplyResult.sessionIndexTitlesUpdated,
          ],
        ],
        noHistoryChanges,
      )
    : null;
  const applyGlobalStateResult = historyApplyResult
    ? formatCountSummary(
        [
          [
            t("pages.diagnostics.historyUnify.counts.workspaceHints"),
            historyApplyResult.workspaceHintsFixed,
          ],
          [
            t("pages.diagnostics.historyUnify.counts.projectless"),
            historyApplyResult.projectlessIdsRemoved,
          ],
          [
            t("pages.diagnostics.historyUnify.counts.savedRoots"),
            historyApplyResult.savedWorkspaceRootsAdded,
          ],
        ],
        noHistoryChanges,
      )
    : null;

  return (
    <PageShell
      title={t("pages.diagnostics.title")}
      subtitle={t("pages.diagnostics.subtitle")}
      notice={notice}
      action={
        <ToolbarButton
          icon={<RotateCcw size={13} />}
          variant="primary"
          disabled={running}
          onClick={() => void refresh(true)}
        >
          {running ? t("actions.running") : t("actions.rerunAll")}
        </ToolbarButton>
      }
    >
      <SummaryStrip
        items={[
          { label: t("summary.checks"), value: String(total), tone: "ok" },
          {
            label: t("summary.failures"),
            value: String(fail),
            tone: fail > 0 ? "fail" : "ok",
          },
          {
            label: t("summary.warnings"),
            value: String(warn),
            tone: warn > 0 ? "warn" : "ok",
          },
          {
            label: t("summary.report"),
            value: t("common.redacted"),
            tone: "ok",
          },
        ]}
      />
      <div className="grid grid-cols-1 gap-4 xl:grid-cols-2">
        {groups.map((group) => (
          <DiagnosticGroup key={group.id} group={group} />
        ))}
      </div>
      <Panel
        title={t("pages.diagnostics.pickerUnlock.title")}
        icon={<Sparkles size={15} />}
        help={t("pages.diagnostics.pickerUnlock.help")}
        action={
          <div className="flex flex-wrap items-center justify-end gap-2">
            <ToolbarButton
              icon={<Sparkles size={13} />}
              disabled={pickerUnlockWorking}
              onClick={() => void unlockPicker()}
            >
              {pickerUnlockWorking
                ? t("actions.running")
                : t("pages.diagnostics.pickerUnlock.action")}
            </ToolbarButton>
            <ToolbarButton
              icon={<Play size={13} />}
              variant="primary"
              disabled={pickerUnlockWorking}
              onClick={() => void launchAndUnlockPicker()}
            >
              {pickerUnlockWorking
                ? t("actions.running")
                : t("pages.diagnostics.pickerUnlock.launchAction")}
            </ToolbarButton>
          </div>
        }
      >
        <div className="grid grid-cols-1 gap-3 xl:grid-cols-2">
          <div>
            <DetailRow
              label={t("pages.diagnostics.pickerUnlock.status")}
              value={
                <div className="flex items-center gap-2">
                  <StatusPill
                    tone={
                      pickerUnlockResult?.injected
                        ? "ok"
                        : pickerUnlockResult
                          ? "warn"
                          : "idle"
                    }
                  />
                  <span>{pickerUnlockResult?.status || "-"}</span>
                </div>
              }
            />
            <DetailRow
              label={t("pages.diagnostics.pickerUnlock.message")}
              value={
                pickerUnlockResult?.message ||
                t("pages.diagnostics.pickerUnlock.noResult")
              }
            />
            <DetailRow
              label={t("pages.diagnostics.pickerUnlock.port")}
              value={
                pickerUnlockResult
                  ? `${pickerUnlockResult.debugPort ?? "-"} / ${pickerUnlockResult.attemptedPorts.join(", ")}`
                  : "-"
              }
            />
            <DetailRow
              label={t("pages.diagnostics.pickerUnlock.launched")}
              value={pickerUnlockResult?.launched ? "yes" : "no"}
            />
          </div>
          <div>
            <DetailRow
              label={t("pages.diagnostics.pickerUnlock.targets")}
              value={
                pickerUnlockResult
                  ? `${pickerUnlockResult.injectedTargetCount}/${pickerUnlockResult.targetCount}`
                  : "-"
              }
            />
            <DetailRow
              label={t("pages.diagnostics.pickerUnlock.models")}
              value={
                pickerUnlockResult
                  ? `${pickerUnlockResult.modelCount} (${pickerUnlockResult.modelNames.slice(0, 6).join(", ") || "-"})`
                  : "-"
              }
            />
            <DetailRow
              label={t("pages.diagnostics.pickerUnlock.renderer")}
              value={formatPickerRendererReports(pickerUnlockResult)}
            />
            <DetailRow
              label={t("pages.diagnostics.pickerUnlock.errors")}
              value={
                pickerUnlockResult?.errors[0] ||
                t("pages.diagnostics.pickerUnlock.noErrors")
              }
            />
            <DetailRow
              label={t("pages.diagnostics.pickerUnlock.executable")}
              value={pickerUnlockResult?.codexExecutable || "-"}
            />
          </div>
        </div>
      </Panel>
      <Panel
        title={t("pages.diagnostics.historyUnify.title")}
        icon={<GitCompare size={15} />}
        help={t("pages.diagnostics.historyUnify.help")}
        action={
          <div className="flex flex-wrap items-center justify-end gap-2">
            <ToolbarButton
              icon={<Eye size={13} />}
              disabled={historyWorking || !history}
              onClick={() => void previewHistoryUnify()}
            >
              {t("actions.preview")}
            </ToolbarButton>
            <ToolbarButton
              icon={<Save size={13} />}
              variant="danger"
              disabled={
                historyWorking ||
                !historyPreview ||
                !historyPreview.canApply ||
                historyPreview.codexRunning
              }
              onClick={() => void applyHistoryUnify()}
            >
              {t("pages.diagnostics.historyUnify.apply")}
            </ToolbarButton>
          </div>
        }
      >
        <div className="grid grid-cols-1 gap-3 xl:grid-cols-2">
          <div>
            <DetailRow
              label={t("pages.diagnostics.historyUnify.target")}
              value={historyPreview?.targetProvider || history?.suggestedTargetProvider || "-"}
            />
            <DetailRow
              label={t("pages.diagnostics.historyUnify.sources")}
              value={
                historyPreview?.sourceProviderIds.join(", ") ||
                history?.sourceProviderIds.join(", ") ||
                "-"
              }
            />
            <DetailRow
              label={t("pages.diagnostics.historyUnify.impact")}
              value={previewImpact}
            />
            <DetailRow
              label={t("pages.diagnostics.historyUnify.visibility")}
              value={previewVisibility}
            />
            <DetailRow
              label={t("pages.diagnostics.historyUnify.globalState")}
              value={previewGlobalState}
            />
            <DetailRow
              label={t("pages.diagnostics.historyUnify.codexState")}
              value={
                <div className="flex items-center gap-2">
                  <StatusPill
                    tone={historyPreview?.codexRunning ? "fail" : "ok"}
                  />
                  <span>
                    {historyPreview?.codexRunning
                      ? t("pages.diagnostics.historyUnify.codexRunning")
                      : t("pages.diagnostics.historyUnify.codexNotRunning")}
                  </span>
                </div>
              }
            />
          </div>
          <div>
            <DetailRow
              label={t("pages.diagnostics.historyUnify.stateDb")}
              value={
                <PathValue
                  value={
                    historyPreview?.activeStateDbPath ||
                    history?.activeStateDbPath ||
                    "-"
                  }
                />
              }
            />
            <DetailRow
              label={t("pages.diagnostics.historyUnify.backupDir")}
              value={
                <PathValue
                  value={
                    historyApplyResult?.backup.backupDir ||
                    historyPreview?.backupDir ||
                    "-"
                  }
                />
              }
            />
            <DetailRow
              label={t("pages.diagnostics.historyUnify.result")}
              value={
                historyApplyResult
                  ? applyProviderResult || noHistoryChanges
                  : historyPreview?.warnings[0]?.message ||
                    t("pages.diagnostics.historyUnify.awaiting")
              }
            />
            <DetailRow
              label={t("pages.diagnostics.historyUnify.resultVisibility")}
              value={
                historyApplyResult
                  ? applyVisibilityResult || noHistoryChanges
                  : t("pages.diagnostics.historyUnify.awaiting")
              }
            />
            <DetailRow
              label={t("pages.diagnostics.historyUnify.resultGlobalState")}
              value={
                historyApplyResult
                  ? applyGlobalStateResult || noHistoryChanges
                  : t("pages.diagnostics.historyUnify.awaiting")
              }
            />
          </div>
        </div>
      </Panel>
    </PageShell>
  );
}

function DiagnosticGroup({ group }: { group: DiagnosticGroupView }) {
  const { t } = useTranslation();
  return (
    <Panel title={t(group.titleKey)} icon={<Activity size={15} />}>
      <div className="divide-y divide-ink-900/[0.06]">
        {group.items.map((item) => (
          <div key={item.id} className="flex items-center gap-3 py-2.5">
            <div className="min-w-0 flex-1">
              <div className="truncate text-[13px] font-medium text-ink-800">
                {t(item.labelKey)}
              </div>
              <div className="mt-0.5 break-words text-[11px] leading-relaxed text-ink-500">
                {item.detail}
              </div>
            </div>
            {item.latencyMs && (
              <span className="text-[11px] text-ink-400">
                {item.latencyMs} ms
              </span>
            )}
            <StatusPill tone={item.status} />
          </div>
        ))}
      </div>
    </Panel>
  );
}

type LogLevelFilter = "all" | "info" | "warn" | "error";

export function LogsPage() {
  const { t } = useTranslation();
  const { notice, show } = useNotice();
  const [proxy, setProxy] = useState<ProxyStatusView | null>(null);
  const [logs, setLogs] = useState<ProxyRuntimeLogs | null>(null);
  const [sessions, setSessions] = useState<ProxySessionsView | null>(null);
  const [loading, setLoading] = useState(false);
  const [levelFilter, setLevelFilter] = useState<LogLevelFilter>("all");

  const refresh = useCallback(async () => {
    setLoading(true);
    const [statusR, logsR, sessionsR] = await Promise.all([
      invokeCmd<ProxyStatusView>("proxy_status"),
      invokeCmd<ProxyRuntimeLogs>("proxy_runtime_logs"),
      invokeCmd<ProxySessionsView>("proxy_sessions"),
    ]);
    setLoading(false);

    if (statusR.ok) {
      setProxy(statusR.data);
    } else {
      show("warning", statusR.error);
    }
    if (logsR.ok) {
      setLogs(logsR.data);
    } else {
      show("warning", logsR.error);
    }
    if (sessionsR.ok) {
      setSessions(sessionsR.data);
    } else {
      show("warning", sessionsR.error);
    }
  }, [show]);

  useEffect(() => {
    void refresh();
  }, [refresh]);

  const visibleLogs = useMemo(() => {
    const items = logs?.items ?? [];
    if (levelFilter === "all") return items;
    return items.filter((item) => item.level === levelFilter);
  }, [levelFilter, logs]);

  const copyLogs = useCallback(() => {
    const text =
      visibleLogs
        .map((item) => {
          const time = item.at
            ? new Date(item.at).toLocaleString()
            : "--:--:--";
          return `[${time}] [${item.level}] [${item.scope}] ${item.message}`;
        })
        .join("\n") || t("pages.logs.empty");

    void navigator.clipboard?.writeText(text);
    show("success", t("feedback.logsCopied"));
  }, [show, t, visibleLogs]);

  const sessionsList = sessions?.sessions ?? [];
  const activeSessionCount = sessionsList.filter(
    (session) => session.status === "active",
  ).length;

  return (
    <PageShell
      title={t("pages.logs.title")}
      subtitle={t("pages.logs.subtitle")}
      notice={notice}
      action={
        <ToolbarButton
          icon={<RefreshCw size={13} />}
          onClick={() => void refresh()}
          disabled={loading}
        >
          {t("actions.refresh")}
        </ToolbarButton>
      }
    >
      <SummaryStrip
        items={[
          {
            label: t("pages.logs.summary.runtime"),
            value: proxy
              ? t(`pages.codexBoxRuntime.statusName.${proxy.status}`)
              : t("common.unknown"),
            tone: proxy?.status === "running" ? "ok" : "warn",
          },
          {
            label: t("pages.logs.summary.events"),
            value: String(logs?.items.length ?? 0),
            tone: "ok",
          },
          {
            label: t("pages.logs.summary.sessions"),
            value: String(sessionsList.length),
            tone: activeSessionCount > 0 ? "running" : "idle",
          },
          {
            label: t("pages.logs.summary.redaction"),
            value: logs?.redacted ? t("common.enabled") : t("common.unknown"),
            tone: logs?.redacted ? "ok" : "warn",
          },
        ]}
      />

      <div className="grid grid-cols-1 gap-4 xl:grid-cols-[minmax(0,1.35fr)_minmax(360px,0.65fr)]">
        <Panel
          title={t("pages.logs.eventsTitle")}
          icon={<ScrollText size={15} />}
          help={t("pages.logs.eventsHelp")}
          action={
            <div className="flex flex-wrap items-center justify-end gap-2">
              <div className="flex rounded-lg border border-ink-900/[0.06] bg-white/40 p-1">
                {(["all", "info", "warn", "error"] as LogLevelFilter[]).map(
                  (level) => (
                    <button
                      key={level}
                      type="button"
                      onClick={() => setLevelFilter(level)}
                      className={`h-7 rounded-md px-2.5 text-[12px] font-medium transition ${
                        levelFilter === level
                          ? "bg-white text-ink-900 shadow-sm"
                          : "text-ink-500 hover:text-ink-800"
                      }`}
                    >
                      {t(`pages.logs.filters.${level}`)}
                    </button>
                  ),
                )}
              </div>
              <ToolbarButton icon={<Copy size={13} />} onClick={copyLogs}>
                {t("actions.copyLogs")}
              </ToolbarButton>
            </div>
          }
        >
          <div className="max-h-[520px] overflow-auto rounded-lg border border-ink-900/[0.06] bg-white/35 cb-scroll">
            {visibleLogs.length > 0 ? (
              <div className="divide-y divide-ink-900/[0.06]">
                {visibleLogs.map((item, index) => (
                  <div
                    key={`${item.at}-${item.scope}-${index}`}
                    className="grid grid-cols-[96px_64px_110px_minmax(0,1fr)] gap-3 px-3 py-2.5 text-[12px]"
                  >
                    <span className="font-mono text-ink-400">
                      {item.at
                        ? new Date(item.at).toLocaleTimeString()
                        : "--:--:--"}
                    </span>
                    <StatusPill tone={logLevelTone(item.level)} />
                    <span className="truncate font-mono text-ink-500">
                      {item.scope}
                    </span>
                    <span className="min-w-0 break-words leading-[1.55] text-ink-700">
                      {item.message}
                    </span>
                  </div>
                ))}
              </div>
            ) : (
              <div className="px-3 py-8 text-center text-[12px] text-ink-500">
                {t("pages.logs.empty")}
              </div>
            )}
          </div>
        </Panel>

        <Panel
          title={t("pages.logs.sessionsTitle")}
          icon={<Users size={15} />}
          help={t("pages.logs.sessionsHelp")}
        >
          <div className="flex flex-col gap-2">
            {sessionsList.map((session) => (
              <div
                key={session.id}
                className="rounded-lg border border-ink-900/[0.06] bg-white/35 p-3"
              >
                <div className="flex items-start justify-between gap-3">
                  <div className="min-w-0">
                    <div className="truncate text-[13px] font-semibold text-ink-850">
                      {session.label}
                    </div>
                    <div className="mt-1 text-[11px] text-ink-500">
                      {session.status === "active"
                        ? t("pages.logs.sessionStatus.active")
                        : t("pages.logs.sessionStatus.idle")}
                      {" · "}
                      {t("pages.logs.sessionMeta", {
                        providers: session.providerCount,
                        models: session.modelCount,
                      })}
                    </div>
                  </div>
                  <StatusPill
                    tone={session.status === "active" ? "running" : "idle"}
                  />
                </div>
                <div className="mt-2 truncate font-mono text-[11px] text-ink-400">
                  {session.id}
                </div>
              </div>
            ))}
            {sessionsList.length === 0 && (
              <div className="rounded-lg border border-dashed border-ink-900/[0.08] px-3 py-6 text-center text-[12px] text-ink-500">
                {t("pages.logs.sessionsEmpty")}
              </div>
            )}
          </div>
        </Panel>
      </div>

      <div className="cb-surface p-4">
        <div className="flex items-center gap-2 text-[12px] font-medium text-ink-700">
          <ShieldCheck size={14} />
          {t("pages.logs.sourceTitle")}
          <InfoTip
            label={t("pages.logs.sourceTitle")}
            content={t("pages.logs.sourceHelp")}
          />
        </div>
      </div>
    </PageShell>
  );
}

function logLevelTone(level: string): StatusTone {
  if (level === "error") return "fail";
  if (level === "warn") return "warn";
  return "ok";
}

export function SettingsPage() {
  const { t, i18n } = useTranslation();
  const { notice, show } = useNotice();
  const setActivePage = useUIStore((state) => state.setActivePage);
  const [selectedId, setSelectedId] = useState(mockSettingsSections[0].id);
  const [toggles, setToggles] = useState<Record<string, boolean>>({
    startupCheck: true,
    requireDiff: true,
    maskSecrets: true,
    confirmDanger: true,
    backupFirst: true,
    retention: true,
    rollback: true,
    redaction: true,
    maxSize: true,
    exportReport: true,
    byokWriteEnabled: true,
    byokSchemaPreserve: true,
    byokVisibilitySync: true,
  });
  const selected =
    mockSettingsSections.find((item) => item.id === selectedId) ||
    mockSettingsSections[0];

  return (
    <PageShell
      title={t("pages.settings.title")}
      subtitle={t("pages.settings.subtitle")}
      notice={notice}
    >
      <div className="grid grid-cols-[minmax(230px,0.72fr)_minmax(0,1.5fr)] gap-4">
        <Panel
          title={t("pages.settings.sectionsTitle")}
          icon={<Settings size={15} />}
        >
          <div className="flex flex-col gap-2">
            {mockSettingsSections.map((section) => (
              <ListButton
                key={section.id}
                active={section.id === selected.id}
                title={t(section.titleKey)}
                subtitle={t(`settings.sectionDesc.${section.id}`)}
                onClick={() => setSelectedId(section.id)}
              />
            ))}
          </div>
        </Panel>
        <Panel
          title={t(selected.titleKey)}
          icon={<AlertTriangle size={15} />}
          action={
            <div className="flex gap-2">
              <ToolbarButton
                icon={<Languages size={13} />}
                onClick={() => {
                  const next = i18n.language === "zh" ? "en" : "zh";
                  void setLanguage(next as "zh" | "en");
                  show("success", t("feedback.languageChanged"));
                }}
              >
                {t("actions.switchLanguage")}
              </ToolbarButton>
            </div>
          }
        >
          <div className="divide-y divide-ink-900/[0.06]">
            {selected.optionKeys.map((key) => (
              <SettingToggle
                key={key}
                label={t(`settings.options.${key}.label`)}
                desc={t(`settings.options.${key}.desc`)}
                checked={!!toggles[key]}
                onChange={() => {
                  setToggles((current) => ({
                    ...current,
                    [key]: !current[key],
                  }));
                  show("info", t("feedback.settingUpdated"));
                }}
              />
            ))}
          </div>
        </Panel>
      </div>
      <Panel
        title={t("pages.settings.advancedTitle")}
        icon={<Settings size={15} />}
      >
        <div className="grid grid-cols-1 gap-3 md:grid-cols-2">
          <button
            type="button"
            onClick={() => setActivePage("codex_runtime")}
            className="rounded-lg border border-ink-900/[0.06] bg-white/45 p-4 text-left transition hover:bg-white/75"
          >
            <div className="text-[13px] font-semibold text-ink-900">
              {t("pages.settings.runtimeDiagnosticsTitle")}
            </div>
            <div className="mt-1 text-[12px] leading-[1.55] text-ink-500">
              {t("pages.settings.runtimeDiagnosticsHint")}
            </div>
          </button>
          <button
            type="button"
            onClick={() => setActivePage("model_router")}
            className="rounded-lg border border-ink-900/[0.06] bg-white/45 p-4 text-left transition hover:bg-white/75"
          >
            <div className="text-[13px] font-semibold text-ink-900">
              {t("pages.settings.routeDiagnosticsTitle")}
            </div>
            <div className="mt-1 text-[12px] leading-[1.55] text-ink-500">
              {t("pages.settings.routeDiagnosticsHint")}
            </div>
          </button>
        </div>
      </Panel>
    </PageShell>
  );
}

function SettingToggle({
  label,
  desc,
  checked,
  onChange,
}: {
  label: string;
  desc: string;
  checked: boolean;
  onChange: () => void;
}) {
  return (
    <button
      className="flex w-full items-center gap-4 py-3 text-left"
      onClick={onChange}
    >
      <div className="min-w-0 flex-1">
        <div className="text-[13px] font-medium text-ink-800">{label}</div>
        <div className="mt-0.5 text-[12px] leading-[1.55] text-ink-500">
          {desc}
        </div>
      </div>
      <span
        className={`relative h-6 w-10 rounded-full transition-colors ${
          checked ? "bg-[#0A84FF]" : "bg-ink-900/12"
        }`}
      >
        <span
          className={`absolute top-1 h-4 w-4 rounded-full bg-white shadow-sm transition-transform ${
            checked ? "translate-x-[18px]" : "translate-x-1"
          }`}
        />
      </span>
    </button>
  );
}
