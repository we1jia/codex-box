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
  GitCompare,
  KeyRound,
  Languages,
  Network,
  Play,
  Plus,
  Puzzle,
  RefreshCw,
  RotateCcw,
  Route,
  Save,
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
import {
  mockCodexRuntime,
  mockDiagnostics,
  mockProfiles,
  mockProviderRoutes,
  mockProviders,
  mockSettingsSections,
} from "@/lib/mock-data";
import type {
  ApplyConfigChangeResultView,
  ApplyInjectResult,
  ApplyRestoreResult,
  CodexRuntimeStatus,
  ConfigChangePreviewView,
  ConfigChangeRequest,
  ConfigSnapshotView,
  ApplyConversationProviderResult,
  ConversationProviderCandidate,
  ConversationProviderCandidatesView,
  ConversationProviderPreview,
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
  ProxyRuntimeLogs,
  ProxySessionsView,
  ProxyStatusView,
  RestoreBaseUrlPreview,
  SimpleModelConfigResult,
  StatusTone,
} from "@/lib/types";

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
      <div className="min-h-full flex flex-col gap-4 pb-1">
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
  children,
}: {
  title: string;
  icon: ReactNode;
  action?: ReactNode;
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
      className={`inline-flex h-5 items-center rounded px-2 text-[11px] font-medium ${cls}`}
    >
      {t(`status.${tone}`)}
    </span>
  );
}

function DetailRow({ label, value }: { label: string; value: ReactNode }) {
  return (
    <div className="grid grid-cols-[132px_minmax(0,1fr)] items-start gap-3 border-t border-ink-900/[0.06] py-2.5">
      <div className="cb-label pt-0.5">{label}</div>
      <div className="min-w-0 text-[13px] leading-[1.55] text-ink-800">
        {value}
      </div>
    </div>
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
      <span className="truncate">{children}</span>
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
  return (
    <div className="grid grid-cols-3 gap-3">
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
  wireApi: "chat",
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
            {providers.map((provider) => (
              <option key={provider.id} value={provider.id}>
                {provider.name}
              </option>
            ))}
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

function DiffLine({ line }: { line: DiffLineView }) {
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
      ? "text-[#9BE7B1]"
      : line.kind === "delete"
        ? "text-[#FF9A9A]"
        : line.kind === "change"
          ? "text-[#FFD18A]"
          : "text-white/60";
  return (
    <div className={`min-h-[20px] whitespace-pre-wrap break-all ${cls}`}>
      <span className="mr-2 select-none text-white/30">{prefix}</span>
      {line.content}
    </div>
  );
}

export function ProfilesPage() {
  const { t } = useTranslation();
  const { notice, show } = useNotice();
  const [profiles, setProfiles] = useState<ProfileView[]>(mockProfiles);
  const [providers, setProviders] = useState<ProviderView[]>(mockProviders);
  const [configPath, setConfigPath] = useState("~/.codex/config.toml");
  const [creating, setCreating] = useState(false);
  const [draft, setDraft] = useState<ProfileDraft>(EMPTY_PROFILE_DRAFT);
  const [pendingChange, setPendingChange] =
    useState<PendingProfileChange | null>(null);
  const [writeBusy, setWriteBusy] = useState(false);
  const [selectedId, setSelectedId] = useState(mockProfiles[0].id);
  const selected =
    profiles.find((item) => item.id === selectedId) || profiles[0];

  const refreshConfigSnapshot = async (nextSelectedId?: string) => {
    const result = await invokeCmd<ConfigSnapshotView>("config_snapshot");
    if (result.ok) {
      const nextProfiles =
        result.data.profiles.length > 0 ? result.data.profiles : mockProfiles;
      const nextProviders =
        result.data.providers.length > 0
          ? result.data.providers
          : mockProviders;
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
        providerId: nextProviders[0]?.id || "codex-subscription",
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
        const nextProfiles =
          result.data.profiles.length > 0 ? result.data.profiles : mockProfiles;
        const nextProviders =
          result.data.providers.length > 0
            ? result.data.providers
            : mockProviders;
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
          providerId: nextProviders[0]?.id || "codex-subscription",
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
          <div className="mt-4 max-h-[320px] overflow-auto rounded-md bg-[#14171A] p-4 font-mono text-[12px] leading-[1.7] text-white/88 shadow-inner">
            {pendingChange.preview.diff.map((line, index) => (
              <DiffLine
                key={`${index}-${line.kind}`}
                line={{
                  id: String(index),
                  kind: line.kind,
                  content: line.content,
                }}
              />
            ))}
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
            {profiles.map((profile) => (
              <ListButton
                key={profile.id}
                active={profile.id === selected.id}
                title={profile.name}
                subtitle={`${profile.model} / ${profile.providerId}`}
                right={<StatusPill tone={profile.status} />}
                onClick={() => setSelectedId(profile.id)}
              />
            ))}
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
  const [providers, setProviders] = useState<ProviderView[]>(mockProviders);
  const [selectedId, setSelectedId] = useState(mockProviders[0].id);
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
      const nextProviders =
        result.data.providers.length > 0
          ? result.data.providers
          : mockProviders;
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
        const nextProviders =
          result.data.providers.length > 0
            ? result.data.providers
            : mockProviders;
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
          <div className="mt-4 max-h-[320px] overflow-auto rounded-md bg-[#14171A] p-4 font-mono text-[12px] leading-[1.7] text-white/88 shadow-inner">
            {pendingChange.preview.diff.map((line, index) => (
              <DiffLine
                key={`${index}-${line.kind}`}
                line={{
                  id: String(index),
                  kind: line.kind,
                  content: line.content,
                }}
              />
            ))}
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
            {providers.map((provider) => (
              <ListButton
                key={provider.id}
                active={provider.id === selected.id}
                title={provider.name}
                subtitle={`${t(`providerKind.${provider.kind}`)} / ${provider.wireApi}`}
                right={<StatusPill tone={provider.status} />}
                onClick={() => setSelectedId(provider.id)}
              />
            ))}
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
 * - 读取 ~/.opencodex/custom_model_catalog.json 真实条目
 * - 列出"已激活 profile"对应的模型
 * - 提供 toggle visibility(可见性)和 reasoning 配置入口
 * - 写入走 ~/.opencodex/custom_model_catalog.json
 */
function isProtectedSubscriptionModel(entry: ModelCatalogEntry) {
  if (entry.provider.trim().toLowerCase() !== "openai") return false;
  const backendProvider = entry.backendProvider?.trim().toLowerCase();
  return !backendProvider || backendProvider === "openai";
}

export function ModelsPage() {
  const { t } = useTranslation();
  const { notice, show } = useNotice();
  const [catalog, setCatalog] = useState<ModelCatalogEntry[]>([]);
  const [config, setConfig] = useState<OpenCodexCustomConfig | null>(null);
  const [activeProfile, setActiveProfile] = useState<ProfileView | null>(null);
  const [selectedId, setSelectedId] = useState<string>("");
  const [busy, setBusy] = useState(false);
  const [modelInput, setModelInput] = useState("");
  const [baseUrl, setBaseUrl] = useState("");
  const [apiKey, setApiKey] = useState("");
  const [restartCodex, setRestartCodex] = useState(true);
  const [showApiKey, setShowApiKey] = useState(false);
  const [visionEnabled, setVisionEnabled] = useState(false);
  const [visionBaseUrl, setVisionBaseUrl] = useState("");
  const [visionModel, setVisionModel] = useState("");
  const [visionEnvKey, setVisionEnvKey] = useState("");
  const [deleteCandidateId, setDeleteCandidateId] = useState("");
  const [catalogSourceExpanded, setCatalogSourceExpanded] = useState(false);
  const selected =
    catalog.find((item) => item.modelId === selectedId) || catalog[0];
  const deleteCandidate =
    catalog.find((item) => item.modelId === deleteCandidateId) || null;

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
      if (opencodex.data.catalog.length > 0) {
        setCatalog(opencodex.data.catalog);
        setSelectedId((current) =>
          current &&
          opencodex.data.catalog.some((entry) => entry.modelId === current)
            ? current
            : opencodex.data.catalog[0]?.modelId || "",
        );
      } else {
        setCatalog([]);
        setSelectedId("");
        if (opencodex.data.parseErrors.length === 0) {
          show(
            "info",
            "custom_model_catalog.json 为空,可在 Provider Routes 页添加模型",
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
      await refresh();
    } else {
      show("warning", result.error);
    }
  };

  const saveVisionFallback = async () => {
    if (!config || !selected) return;
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
      <SummaryStrip
        items={[
          {
            label: t("summary.totalModels"),
            value: String(catalog.length),
            tone: "ok",
          },
          {
            label: t("summary.visibleModels"),
            value: String(catalog.filter((m) => m.visible).length),
          },
          {
            label: t("summary.activeProfile"),
            value: activeProfile?.name || "-",
          },
        ]}
      />
      <div className="grid grid-cols-[minmax(300px,0.95fr)_minmax(0,1.4fr)] gap-4">
        <Panel
          title={t("pages.models.simpleConfigTitle")}
          icon={<KeyRound size={15} />}
        >
          <div className="flex flex-col gap-3">
            <label className="flex flex-col gap-1.5">
              <span className="cb-label">
                {t("pages.models.form.modelName")}
              </span>
              <input
                value={modelInput}
                onChange={(event) => setModelInput(event.target.value)}
                placeholder={t("pages.models.form.modelNamePlaceholder")}
                className="rounded-md border border-ink-900/10 bg-white/55 px-3 py-2 text-[13px] text-ink-900 outline-none focus:border-[#0A84FF]/40 focus:ring-2 focus:ring-[#0A84FF]/15"
              />
              <span className="text-[11px] leading-[1.5] text-ink-500">
                {t("pages.models.form.aliasHint")}
              </span>
            </label>
            <label className="flex flex-col gap-1.5">
              <span className="cb-label">{t("pages.models.form.baseUrl")}</span>
              <input
                value={baseUrl}
                onChange={(event) => setBaseUrl(event.target.value)}
                placeholder="https://api.deepseek.com/v1"
                className="rounded-md border border-ink-900/10 bg-white/55 px-3 py-2 text-[13px] text-ink-900 outline-none focus:border-[#0A84FF]/40 focus:ring-2 focus:ring-[#0A84FF]/15"
              />
            </label>
            <label className="flex flex-col gap-1.5">
              <span className="cb-label">{t("pages.models.form.apiKey")}</span>
              <div className="flex items-center rounded-md border border-ink-900/10 bg-white/55 pr-2 focus-within:border-[#0A84FF]/40 focus-within:ring-2 focus-within:ring-[#0A84FF]/15">
                <input
                  value={apiKey}
                  onChange={(event) => setApiKey(event.target.value)}
                  type={showApiKey ? "text" : "password"}
                  placeholder="sk-..."
                  className="min-w-0 flex-1 bg-transparent px-3 py-2 text-[13px] text-ink-900 outline-none"
                />
                <button
                  type="button"
                  onClick={() => setShowApiKey((value) => !value)}
                  className="flex h-7 w-7 items-center justify-center rounded-md text-ink-500 hover:bg-ink-900/5"
                  aria-label={
                    showApiKey ? t("actions.hide") : t("actions.show")
                  }
                >
                  {showApiKey ? <EyeOff size={14} /> : <Eye size={14} />}
                </button>
              </div>
              <span className="text-[11px] leading-[1.5] text-ink-500">
                {t("pages.models.form.secretHint")}
              </span>
            </label>
            <label className="flex items-center gap-2 text-[12px] text-ink-600">
              <input
                type="checkbox"
                checked={restartCodex}
                onChange={(event) => setRestartCodex(event.target.checked)}
                className="h-4 w-4 accent-[#0A84FF]"
              />
              {t("pages.models.form.restartCodex")}
            </label>
            <ToolbarButton
              icon={<Save size={13} />}
              variant="primary"
              disabled={busy}
              onClick={() => void saveSimpleModelConfig()}
            >
              {t("pages.models.form.saveAndAdd")}
            </ToolbarButton>
          </div>
        </Panel>
        <Panel
          title={t("pages.models.dropdownTitle")}
          icon={<Sparkles size={15} />}
        >
          <div className="flex flex-col gap-2">
            {catalog.map((entry) => {
              const canDelete = !isProtectedSubscriptionModel(entry);
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
                    "cursor-pointer",
                    "rounded-md border px-3 py-3 text-left transition-colors",
                    entry.modelId === selected?.modelId
                      ? "border-[#0A84FF]/25 bg-[#0A84FF]/10"
                      : "border-ink-900/[0.06] bg-white/35 hover:bg-white/55",
                  ].join(" ")}
                >
                  <div className="flex items-center gap-3">
                    <input
                      type="checkbox"
                      checked={entry.visible}
                      onChange={(event) => {
                        event.stopPropagation();
                        void toggleVisibility(entry);
                      }}
                      onClick={(event) => event.stopPropagation()}
                      className="h-4 w-4 accent-[#0A84FF]"
                    />
                    <div className="min-w-0 flex-1">
                      <div className="truncate text-[13px] font-semibold text-ink-900">
                        {entry.displayName || entry.modelId}
                      </div>
                      <div className="mt-0.5 truncate text-[11px] text-ink-500">
                        {entry.provider} · {entry.modelId}
                      </div>
                    </div>
                    <span className="inline-flex items-center gap-2">
                      <span className="rounded-full border border-[#0A84FF]/20 bg-[#0A84FF]/10 px-2 py-0.5 text-[10px] font-medium text-[#0A84FF]">
                        {entry.visible
                          ? t("pages.models.visibleInDropdown")
                          : t("pages.models.hiddenInDropdown")}
                      </span>
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
                          className="inline-flex h-7 items-center justify-center gap-1 rounded-md px-2 text-[11px] font-medium text-ink-500 transition-colors hover:bg-status-fail/10 hover:text-status-fail"
                        >
                          <Trash2 size={13} />
                          {t("actions.delete")}
                        </button>
                      ) : null}
                      <ChevronRight size={14} className="text-ink-400" />
                    </span>
                  </div>
                </div>
              );
            })}
            {catalog.length === 0 ? (
              <div className="rounded-md border border-dashed border-ink-900/10 bg-white/40 px-4 py-6 text-center text-[12px] leading-[1.6] text-ink-500">
                {t("feedback.modelsEmpty")}
              </div>
            ) : null}
            <label className="mt-2 flex items-center gap-2 text-[12px] text-ink-600">
              <input
                type="checkbox"
                checked={restartCodex}
                onChange={(event) => setRestartCodex(event.target.checked)}
                className="h-4 w-4 accent-[#0A84FF]"
              />
              {t("pages.models.form.restartAfterUpdate")}
            </label>
          </div>
        </Panel>
      </div>
      <div className="grid grid-cols-[minmax(300px,0.95fr)_minmax(0,1.4fr)] gap-4">
        <Panel
          title={t("pages.models.catalogSourceTitle")}
          icon={<GitCompare size={15} />}
          action={
            <ToolbarButton
              icon={<ChevronRight size={13} />}
              onClick={() => setCatalogSourceExpanded((value) => !value)}
            >
              {catalogSourceExpanded
                ? t("actions.hideDetails")
                : t("actions.showDetails")}
            </ToolbarButton>
          }
        >
          <DetailRow
            label={t("fields.catalogPath")}
            value={
              <span className="font-mono break-all">
                ~/.opencodex/custom_model_catalog.json
              </span>
            }
          />
          {catalogSourceExpanded ? (
            <>
              <DetailRow
                label={t("fields.absolutePath")}
                value={
                  <span className="font-mono break-all">
                    {config?.catalogPath ||
                      "~/.opencodex/custom_model_catalog.json"}
                  </span>
                }
              />
              <DetailRow
                label={t("fields.contentHash")}
                value={
                  <span className="font-mono break-all">
                    {config?.catalogContentHash || "-"}
                  </span>
                }
              />
            </>
          ) : null}
          {config && config.parseErrors.length > 0 ? (
            <div className="mt-3 rounded-md border border-status-warn/30 bg-status-warn/10 px-3 py-2 text-[12px] text-status-warn">
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
        </Panel>
        <Panel
          title={t("pages.models.detailTitle")}
          icon={<Sparkles size={15} />}
          action={
            <ToolbarButton
              icon={
                selected?.visible ? <EyeOff size={13} /> : <Eye size={13} />
              }
              disabled={!selected || busy}
              onClick={() => selected && void toggleVisibility(selected)}
            >
              {selected?.visible ? t("actions.hide") : t("actions.show")}
            </ToolbarButton>
          }
        >
          {selected ? (
            <>
              <DetailRow
                label="model_id"
                value={<span className="font-mono">{selected.modelId}</span>}
              />
              <DetailRow
                label={t("fields.provider")}
                value={selected.provider}
              />
              <DetailRow
                label={t("fields.displayName")}
                value={selected.displayName || "-"}
              />
              <DetailRow
                label={t("fields.visible")}
                value={
                  selected.visible ? t("common.enabled") : t("common.disabled")
                }
              />
              <DetailRow
                label={t("fields.reasoning")}
                value={
                  selected.reasoning
                    ? `${selected.reasoning.enabled ? t("common.enabled") : t("common.disabled")} · ${selected.reasoning.levels.join(", ")}`
                    : t("common.disabled")
                }
              />
              <DetailRow
                label={t("fields.note")}
                value={selected.note || "-"}
              />
            </>
          ) : (
            <div className="py-6 text-center text-[12px] text-ink-500">
              {t("common.none")}
            </div>
          )}
        </Panel>
      </div>
      <Panel title={t("pages.models.advancedTitle")} icon={<Eye size={15} />}>
        {selected ? (
          <div className="grid grid-cols-[minmax(240px,0.8fr)_minmax(0,1.2fr)] gap-4">
            <div className="rounded-md border border-ink-900/[0.06] bg-white/35 p-3">
              <div className="text-[13px] font-semibold text-ink-900">
                {t("pages.models.visionTitle")}
              </div>
              <p className="mt-1 text-[12px] leading-[1.6] text-ink-500">
                {t("pages.models.visionHint")}
              </p>
              <label className="mt-3 flex items-center gap-2 text-[12px] text-ink-700">
                <input
                  type="checkbox"
                  checked={visionEnabled}
                  onChange={(event) => setVisionEnabled(event.target.checked)}
                  className="h-4 w-4 accent-[#0A84FF]"
                />
                {t("pages.models.visionEnable")}
              </label>
            </div>
            <div className="grid grid-cols-3 gap-3">
              <FormField label={t("pages.models.form.baseUrl")}>
                <input
                  className={inputClass}
                  value={visionBaseUrl}
                  placeholder="https://api.example.com/v1"
                  onChange={(event) => setVisionBaseUrl(event.target.value)}
                />
              </FormField>
              <FormField label={t("pages.models.visionModel")}>
                <input
                  className={inputClass}
                  value={visionModel}
                  placeholder="vision-model"
                  onChange={(event) => setVisionModel(event.target.value)}
                />
              </FormField>
              <FormField label={t("pages.models.visionEnvKey")}>
                <input
                  className={inputClass}
                  value={visionEnvKey}
                  placeholder="VISION_API_KEY"
                  onChange={(event) => setVisionEnvKey(event.target.value)}
                />
              </FormField>
              <div className="col-span-3 flex justify-end">
                <ToolbarButton
                  icon={<Save size={13} />}
                  variant="primary"
                  disabled={busy || !selected}
                  onClick={() => void saveVisionFallback()}
                >
                  {t("pages.models.visionSave")}
                </ToolbarButton>
              </div>
            </div>
          </div>
        ) : (
          <div className="py-6 text-center text-[12px] text-ink-500">
            {t("common.none")}
          </div>
        )}
      </Panel>
      <div className="cb-surface p-4">
        <div className="mb-2 flex items-center gap-2 text-[12px] font-medium text-ink-700">
          <Sparkles size={14} /> {t("pages.models.byokHint")}
        </div>
        <p className="text-[12px] leading-[1.6] text-ink-500">
          {t("pages.models.byokHintBody")}
        </p>
      </div>
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

/**
 * ProviderRoutes 页面:管理 ~/.opencodex/providers.json 条目
 * - 展示所有 provider 路由
 * - 启用/禁用某个路由(对应 Codex App picker 是否出现该 provider 的模型)
 * - 写入走 ~/.opencodex/providers.json
 */
export function ProviderRoutesPage() {
  const { t } = useTranslation();
  const { notice, show } = useNotice();
  const [routes, setRoutes] = useState<ProviderRoute[]>(mockProviderRoutes);
  const [config, setConfig] = useState<OpenCodexCustomConfig | null>(null);
  const [selectedName, setSelectedName] = useState<string>(
    mockProviderRoutes[0]?.name || "",
  );
  const [busy, setBusy] = useState(false);
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
          show("info", "providers.json 为空,请新增 provider");
        }
      }
    } else {
      show("warning", result.error);
    }
  }, [show]);

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
      <Panel
        title={t("pages.providerRoutes.sourceTitle")}
        icon={<GitCompare size={15} />}
      >
        <DetailRow
          label={t("fields.providersPath")}
          value={
            <span className="font-mono break-all">
              {config?.providersPath || "~/.opencodex/providers.json"}
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
        {config && config.parseErrors.length > 0 ? (
          <div className="mt-3 rounded-md border border-status-warn/30 bg-status-warn/10 px-3 py-2 text-[12px] text-status-warn">
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
      </Panel>
      <div className="grid grid-cols-[minmax(280px,0.95fr)_minmax(0,1.4fr)] gap-4">
        <Panel
          title={t("pages.providerRoutes.listTitle")}
          icon={<Route size={15} />}
        >
          <div className="flex flex-col gap-2">
            {routes.map((route) => (
              <ListButton
                key={route.name}
                active={route.name === selected?.name}
                title={route.name}
                subtitle={`${route.wireApi} · ${route.baseUrl}`}
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
            ))}
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
                label="name"
                value={<span className="font-mono">{selected.name}</span>}
              />
              <DetailRow
                label="base_url"
                value={
                  <span className="font-mono break-all">
                    {selected.baseUrl}
                  </span>
                }
              />
              <DetailRow
                label="wire_api"
                value={<span className="font-mono">{selected.wireApi}</span>}
              />
              <DetailRow
                label="api_key_ref"
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
                label={t("fields.enabled")}
                value={
                  selected.enabled ? t("common.enabled") : t("common.disabled")
                }
              />
              <DetailRow
                label={t("fields.note")}
                value={selected.note || "-"}
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
  const [codexRuntime] = useState<CodexRuntimeStatus>(mockCodexRuntime);
  const [loading, setLoading] = useState(false);
  const [busy, setBusy] = useState(false);
  const [preview, setPreview] = useState<ProxyModelsPreview | null>(null);
  const [injectPreview, setInjectPreview] =
    useState<InjectBaseUrlPreview | null>(null);
  const [restorePreview, setRestorePreview] =
    useState<RestoreBaseUrlPreview | null>(null);
  const [runtimeLogs, setRuntimeLogs] = useState<ProxyRuntimeLogs | null>(null);
  const [sessions, setSessions] = useState<ProxySessionsView | null>(null);
  const [effectiveRouting, setEffectiveRouting] =
    useState<EffectiveRoutingStatus | null>(null);
  const [conversationProviders, setConversationProviders] =
    useState<ConversationProviderCandidatesView | null>(null);
  const [conversationDraft, setConversationDraft] =
    useState<ConversationProviderDraft>({
      providerId: "openai",
      displayName: "OpenAI",
      proxyPort: 1455,
      wireApi: "responses",
      requiresOpenaiAuth: true,
      originalBaseUrl: "",
    });
  const [conversationPreview, setConversationPreview] =
    useState<ConversationProviderPreview | null>(null);
  const activeProxyPort = proxyPortOrDefault(proxy);

  const refresh = useCallback(async () => {
    setLoading(true);
    const statusR = await invokeCmd<ProxyStatusView>("proxy_status");
    if (statusR.ok) setProxy(statusR.data);
    const logsR = await invokeCmd<ProxyRuntimeLogs>("proxy_runtime_logs");
    if (logsR.ok) setRuntimeLogs(logsR.data);
    const sessionsR = await invokeCmd<ProxySessionsView>("proxy_sessions");
    if (sessionsR.ok) setSessions(sessionsR.data);
    const routingR = await invokeCmd<EffectiveRoutingStatus>(
      "effective_routing_status",
    );
    if (routingR.ok) setEffectiveRouting(routingR.data);
    const ocR = await invokeCmd<OpenCodexCustomConfig>("opencodex_config_read");
    if (ocR.ok) setOpencodex(ocR.data);
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
          {canStart && (
            <ToolbarButton
              icon={<Play size={13} />}
              variant="primary"
              onClick={() => void start()}
              disabled={busy}
            >
              {t("pages.codexBoxRuntime.start")}
            </ToolbarButton>
          )}
          {canStop && (
            <>
              <ToolbarButton
                icon={<Square size={13} />}
                onClick={() => void stop()}
                disabled={busy}
              >
                {t("pages.codexBoxRuntime.stop")}
              </ToolbarButton>
              <ToolbarButton
                icon={<RotateCcw size={13} />}
                onClick={() => void restart()}
                disabled={busy}
              >
                {t("pages.codexBoxRuntime.restart")}
              </ToolbarButton>
            </>
          )}
        </div>
      }
    >
      <SummaryStrip
        items={[
          {
            label: t("pages.codexBoxRuntime.status"),
            value: t(`pages.codexBoxRuntime.statusName.${statusName}`),
            tone: statusTone,
          },
          {
            label: t("pages.codexBoxRuntime.port"),
            value: port > 0 ? String(port) : "-",
            tone: port > 0 ? "ok" : "idle",
          },
          {
            label: t("pages.codexBoxRuntime.uptime"),
            value:
              proxy && proxy.uptimeMs !== null
                ? format_uptime(proxy.uptimeMs)
                : "-",
            tone: statusTone,
          },
          {
            label: t("pages.codexBoxRuntime.routedProviders"),
            value: String(proxy?.providerCount ?? 0),
            tone: (proxy?.providerCount ?? 0) > 0 ? "ok" : "warn",
          },
        ]}
      />

      {proxy?.lastError && (
        <div className="cb-surface border-status-fail/30 bg-status-fail/[0.06] p-4">
          <div className="mb-1 flex items-center gap-2 text-[12px] font-medium text-status-fail">
            <AlertTriangle size={14} /> {t("pages.codexBoxRuntime.lastError")}
          </div>
          <p className="font-mono text-[12px] text-ink-800 break-all">
            {proxy.lastError}
          </p>
        </div>
      )}

      <Panel
        title={t("pages.codexBoxRuntime.effectiveRouting.title")}
        icon={<Route size={15} />}
      >
        {effectiveRouting ? (
          <div className="space-y-3">
            <p className="text-[12px] leading-[1.6] text-ink-500">
              {t("pages.codexBoxRuntime.effectiveRouting.hint")}
            </p>
            <div className="grid grid-cols-1 gap-x-4 lg:grid-cols-2">
              <DetailRow
                label="model_provider"
                value={
                  <span className="font-mono break-all">
                    {effectiveRouting.modelProvider}
                  </span>
                }
              />
              <DetailRow
                label="model"
                value={
                  <span className="font-mono break-all">
                    {effectiveRouting.currentModel || "-"}
                  </span>
                }
              />
              <DetailRow
                label={t(
                  "pages.codexBoxRuntime.effectiveRouting.requestBaseUrl",
                )}
                value={
                  <span className="font-mono break-all">
                    {effectiveRouting.requestBaseUrl || "-"}
                  </span>
                }
              />
              <DetailRow
                label={t(
                  "pages.codexBoxRuntime.effectiveRouting.requestSource",
                )}
                value={
                  <span className="font-mono break-all">
                    {effectiveRouting.requestBaseUrlSource}
                  </span>
                }
              />
              <DetailRow
                label="model_catalog_json"
                value={
                  <span className="font-mono break-all">
                    {effectiveRouting.modelCatalogPath || "-"}
                  </span>
                }
              />
              <DetailRow
                label={t(
                  "pages.codexBoxRuntime.effectiveRouting.catalogProvider",
                )}
                value={
                  <span className="font-mono break-all">
                    {effectiveRouting.catalogProvider || "-"}
                  </span>
                }
              />
              <DetailRow
                label="backend_provider"
                value={
                  <span className="font-mono break-all">
                    {effectiveRouting.backendProvider || "-"}
                  </span>
                }
              />
              <DetailRow
                label="backend_model"
                value={
                  <span className="font-mono break-all">
                    {effectiveRouting.backendModel || "-"}
                  </span>
                }
              />
              <DetailRow
                label={t("pages.codexBoxRuntime.effectiveRouting.upstream")}
                value={
                  <span className="font-mono break-all">
                    {effectiveRouting.upstreamBaseUrl || "-"}
                  </span>
                }
              />
              <DetailRow
                label={t("pages.codexBoxRuntime.effectiveRouting.proxy")}
                value={
                  <span className="font-mono break-all">
                    {effectiveRouting.proxyRunning
                      ? `127.0.0.1:${effectiveRouting.proxyPort ?? "-"}`
                      : t("pages.codexBoxRuntime.notRunning")}
                  </span>
                }
              />
            </div>
            <div className="rounded-md border border-ink-900/[0.06] bg-white/35 p-3">
              <div className="mb-2 text-[12px] font-medium text-ink-700">
                {t("pages.codexBoxRuntime.effectiveRouting.issues")}
              </div>
              {effectiveRouting.issues.length > 0 ? (
                <div className="flex flex-col gap-2">
                  {effectiveRouting.issues.map((issue) => (
                    <div
                      key={`${issue.code}-${issue.message}`}
                      className="flex items-start gap-2 text-[12px] leading-[1.55]"
                    >
                      <StatusPill tone={routingIssueTone(issue.severity)} />
                      <div className="min-w-0">
                        <div className="font-mono text-[11px] text-ink-500">
                          {issue.code}
                        </div>
                        <div className="text-ink-800">{issue.message}</div>
                      </div>
                    </div>
                  ))}
                </div>
              ) : (
                <div className="flex items-center gap-2 text-[12px] text-status-ok">
                  <StatusPill tone="ok" />
                  <span>
                    {t("pages.codexBoxRuntime.effectiveRouting.noIssues")}
                  </span>
                </div>
              )}
            </div>
          </div>
        ) : (
          <p className="text-[12px] text-ink-500">
            {t("pages.codexBoxRuntime.effectiveRouting.empty")}
          </p>
        )}
      </Panel>

      <Panel
        title={t("pages.codexBoxRuntime.conversationProvider.title")}
        icon={<Users size={15} />}
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
        <p className="mb-3 text-[12px] leading-[1.6] text-ink-500">
          {t("pages.codexBoxRuntime.conversationProvider.hint")}
        </p>
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
              placeholder="openai / custom / codex_local_access"
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
            <div className="rounded-md border border-ink-900/[0.06] bg-ink-900/[0.02] p-3 max-h-64 overflow-y-auto cb-scroll">
              <pre className="font-mono text-[11px] leading-[1.5] text-ink-800 whitespace-pre-wrap break-all">
                {conversationPreview.diff
                  .map(
                    (d, i) =>
                      `${d.kind === "insert" ? "+" : d.kind === "delete" ? "-" : " "} ${d.content}`,
                  )
                  .join("\n")}
              </pre>
            </div>
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
                {codexRuntime.codexHome}
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
          <p className="text-[12px] leading-[1.6] text-ink-500 mb-3">
            {t("pages.codexBoxRuntime.injectHint")}
          </p>
          {injectPreview && (
            <div className="space-y-2">
              <DetailRow
                label={t("pages.codexBoxRuntime.injectDiffSummary")}
                value={`+${injectPreview.insertions} / -${injectPreview.deletions}`}
              />
              <div className="rounded-md border border-ink-900/[0.06] bg-ink-900/[0.02] p-3 max-h-64 overflow-y-auto cb-scroll">
                <pre className="font-mono text-[11px] leading-[1.5] text-ink-800 whitespace-pre-wrap break-all">
                  {injectPreview.diff
                    .map(
                      (d, i) =>
                        `${d.kind === "insert" ? "+" : d.kind === "delete" ? "-" : " "} ${d.content}`,
                    )
                    .join("\n")}
                </pre>
              </div>
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
          <p className="text-[12px] leading-[1.6] text-ink-500 mb-3">
            {t("pages.codexBoxRuntime.restoreHint")}
          </p>
          {restorePreview && (
            <div className="space-y-2">
              <DetailRow
                label={t("pages.codexBoxRuntime.injectDiffSummary")}
                value={`+${restorePreview.insertions} / -${restorePreview.deletions}`}
              />
              <div className="rounded-md border border-ink-900/[0.06] bg-ink-900/[0.02] p-3 max-h-64 overflow-y-auto cb-scroll">
                <pre className="font-mono text-[11px] leading-[1.5] text-ink-800 whitespace-pre-wrap break-all">
                  {restorePreview.diff
                    .map(
                      (d, i) =>
                        `${d.kind === "insert" ? "+" : d.kind === "delete" ? "-" : " "} ${d.content}`,
                    )
                    .join("\n")}
                </pre>
              </div>
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

      <div className="grid grid-cols-1 lg:grid-cols-2 gap-4">
        <Panel
          title={t("pages.codexBoxRuntime.logsPanel")}
          icon={<Activity size={15} />}
          action={
            <ToolbarButton
              icon={<RefreshCw size={13} />}
              onClick={() => void refresh()}
              disabled={loading}
            >
              {t("pages.codexBoxRuntime.refreshLogs")}
            </ToolbarButton>
          }
        >
          <div className="rounded-md border border-ink-900/[0.06] bg-[#14171A] p-3">
            <pre className="max-h-40 overflow-auto whitespace-pre-wrap font-mono text-[11px] leading-[1.6] text-white/75 cb-scroll">
              {(runtimeLogs?.items.length ? runtimeLogs.items : [])
                .map((item) => {
                  const time = item.at
                    ? new Date(item.at).toLocaleTimeString()
                    : "--:--:--";
                  return `[${time}] [${item.level}] [${item.scope}] ${item.message}`;
                })
                .join("\n") || t("pages.codexBoxRuntime.logs.empty")}
            </pre>
          </div>
          <p className="mt-2 text-[12px] leading-[1.6] text-ink-500">
            {t("pages.codexBoxRuntime.logsHint")}
          </p>
        </Panel>

        <Panel
          title={t("pages.codexBoxRuntime.sessionsPanel")}
          icon={<Users size={15} />}
          action={
            <ToolbarButton
              icon={<RefreshCw size={13} />}
              onClick={() => void refresh()}
              disabled={loading}
            >
              {t("pages.codexBoxRuntime.refreshSessions")}
            </ToolbarButton>
          }
        >
          <div className="flex flex-col gap-2">
            {(sessions?.sessions.length ? sessions.sessions : []).map(
              (session) => (
                <div
                  key={session.id}
                  className="flex items-center justify-between rounded-md border border-ink-900/[0.06] bg-white/35 px-3 py-2"
                >
                  <div className="min-w-0">
                    <div className="truncate text-[13px] font-medium text-ink-800">
                      {session.label}
                    </div>
                    <div className="text-[11px] text-ink-500">
                      {session.status === "active"
                        ? t("pages.codexBoxRuntime.sessions.active")
                        : t("pages.codexBoxRuntime.sessions.idle")}
                      {" · "}
                      {t("pages.codexBoxRuntime.sessions.meta", {
                        providers: session.providerCount,
                        models: session.modelCount,
                      })}
                    </div>
                  </div>
                  <StatusPill
                    tone={session.status === "active" ? "running" : "idle"}
                  />
                </div>
              ),
            )}
            {(!sessions || sessions.sessions.length === 0) && (
              <p className="text-[12px] text-ink-500">
                {t("pages.codexBoxRuntime.sessions.empty")}
              </p>
            )}
          </div>
          <p className="mt-2 text-[12px] leading-[1.6] text-ink-500">
            {t("pages.codexBoxRuntime.sessionsHint")}
          </p>
        </Panel>
      </div>

      <div className="cb-surface p-4">
        <div className="mb-2 flex items-center gap-2 text-[12px] font-medium text-ink-700">
          <ShieldCheck size={14} /> {t("pages.codexBoxRuntime.safetyTitle")}
        </div>
        <p className="text-[12px] leading-[1.6] text-ink-500">
          {t("pages.codexBoxRuntime.safetyBody")}
        </p>
      </div>
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

export function DiagnosticsPage() {
  const { t } = useTranslation();
  const { notice, show } = useNotice();
  const [running, setRunning] = useState(false);
  const total = mockDiagnostics.reduce(
    (sum, group) => sum + group.items.length,
    0,
  );
  const warn = mockDiagnostics
    .flatMap((group) => group.items)
    .filter((item) => item.status === "warn").length;

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
          onClick={() => {
            setRunning(true);
            window.setTimeout(() => {
              setRunning(false);
              show("success", t("feedback.diagnosticsDone", { total }));
            }, 500);
          }}
        >
          {running ? t("actions.running") : t("actions.rerunAll")}
        </ToolbarButton>
      }
    >
      <SummaryStrip
        items={[
          { label: t("summary.checks"), value: String(total), tone: "ok" },
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
      <div className="grid grid-cols-2 gap-4">
        {mockDiagnostics.map((group) => (
          <DiagnosticGroup key={group.id} group={group} />
        ))}
      </div>
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
              <div className="mt-0.5 truncate text-[11px] text-ink-500">
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

export function SettingsPage() {
  const { t, i18n } = useTranslation();
  const { notice, show } = useNotice();
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
