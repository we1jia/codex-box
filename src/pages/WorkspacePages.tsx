import { useCallback, useEffect, useMemo, useState, type ReactNode } from "react";
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
  Plus,
  Puzzle,
  RotateCcw,
  Route,
  Save,
  Server,
  Settings,
  ShieldCheck,
  Sparkles,
  TestTube2,
  Trash2,
  Users,
} from "lucide-react";
import { invokeCmd } from "@/lib/api";
import { setLanguage } from "@/lib/i18n";
import {
  mockCodexRuntime,
  mockDiagnostics,
  mockModelCatalog,
  mockProfiles,
  mockProviderRoutes,
  mockProviders,
  mockSettingsSections,
} from "@/lib/mock-data";
import type {
  ApplyConfigChangeResultView,
  CodexRuntimeStatus,
  ConfigChangePreviewView,
  ConfigChangeRequest,
  ConfigSnapshotView,
  DiagnosticGroupView,
  DiffLineView,
  ModelCatalogEntry,
  OpenCodexCustomConfig,
  OpenCodexDeleteRequest,
  OpenCodexWriteRequest,
  OpenCodexWriteResult,
  ProfileView,
  ProviderRoute,
  ProviderView,
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
    <div className={`mt-4 flex items-center gap-2 rounded-md border px-3 py-2 text-xs ${toneClass}`}>
      <CheckCircle2 size={14} />
      <span className="min-w-0 flex-1 truncate">{notice.message}</span>
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
    <span className={`inline-flex h-5 items-center rounded px-2 text-[11px] font-medium ${cls}`}>
      {t(`status.${tone}`)}
    </span>
  );
}

function DetailRow({ label, value }: { label: string; value: ReactNode }) {
  return (
    <div className="grid grid-cols-[132px_minmax(0,1fr)] items-start gap-3 border-t border-ink-900/[0.06] py-2.5">
      <div className="cb-label pt-0.5">{label}</div>
      <div className="min-w-0 text-[13px] leading-[1.55] text-ink-800">{value}</div>
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
          <div className="mt-0.5 truncate text-[11px] text-ink-500">{subtitle}</div>
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
  const masked = looksLikeSecret(value) ? "redacted" : value || "not configured";
  return (
    <span className="inline-flex items-center gap-1.5 rounded bg-ink-900/[0.04] px-2 py-1 font-mono text-[12px] text-ink-700">
      <KeyRound size={12} />
      {masked}
    </span>
  );
}

function useNotice() {
  const [notice, setNotice] = useState<Notice | null>(null);
  const show = (tone: NoticeTone, message: string) => setNotice({ tone, message });
  return { notice, show };
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
            onChange={(event) => setDraft({ ...draft, name: event.target.value })}
          />
        </FormField>
        <FormField label={t("fields.kind")}>
          <select
            className={inputClass}
            value={draft.kind}
            onChange={(event) =>
              setDraft({ ...draft, kind: event.target.value as ProviderView["kind"] })
            }
          >
            {(["official_api", "compatible_api", "local_gateway", "subscription"] as const).map((kind) => (
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
            onChange={(event) => setDraft({ ...draft, baseUrl: event.target.value })}
          />
        </FormField>
        <FormField label="wire_api">
          <select
            className={inputClass}
            value={draft.wireApi}
            onChange={(event) =>
              setDraft({ ...draft, wireApi: event.target.value as ProviderView["wireApi"] })
            }
          >
            {(["chat", "responses", "sse_stream", "custom"] as const).map((api) => (
              <option key={api} value={api}>
                {api}
              </option>
            ))}
          </select>
        </FormField>
        <FormField label={t("fields.envKey")}>
          <input
            className={inputClass}
            value={draft.envKey}
            placeholder="OPENROUTER_API_KEY"
            onChange={(event) => setDraft({ ...draft, envKey: event.target.value })}
          />
        </FormField>
        <FormField label={t("fields.models")}>
          <input
            className={inputClass}
            value={draft.models}
            placeholder="gpt-4.1, claude-sonnet"
            onChange={(event) => setDraft({ ...draft, models: event.target.value })}
          />
        </FormField>
        <div className="col-span-2 flex items-center justify-between gap-3 border-t border-ink-900/[0.06] pt-3">
          <p className="cb-muted">{t("pages.providers.createHint")}</p>
          <div className="flex shrink-0 gap-2">
            <ToolbarButton onClick={onCancel}>{t("actions.cancel")}</ToolbarButton>
            <ToolbarButton icon={<Plus size={13} />} variant="primary" type="submit">
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
            onChange={(event) => setDraft({ ...draft, name: event.target.value })}
          />
        </FormField>
        <FormField label="model">
          <input
            className={inputClass}
            value={draft.model}
            placeholder="openai/gpt-5-mini"
            onChange={(event) => setDraft({ ...draft, model: event.target.value })}
          />
        </FormField>
        <FormField label="model_provider">
          <select
            className={inputClass}
            value={draft.providerId}
            onChange={(event) => setDraft({ ...draft, providerId: event.target.value })}
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
            onChange={(event) => setDraft({ ...draft, sandbox: event.target.value })}
          >
            {["read-only", "workspace-write", "danger-full-access"].map((value) => (
              <option key={value} value={value}>
                {value}
              </option>
            ))}
          </select>
        </FormField>
        <FormField label="approval_policy">
          <select
            className={inputClass}
            value={draft.approval}
            onChange={(event) => setDraft({ ...draft, approval: event.target.value })}
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
            onChange={(event) => setDraft({ ...draft, network: event.target.value })}
          />
        </FormField>
        <FormField label="mcp_refs">
          <input
            className={inputClass}
            value={draft.mcpRefs}
            placeholder="filesystem, git"
            onChange={(event) => setDraft({ ...draft, mcpRefs: event.target.value })}
          />
        </FormField>
        <div className="col-span-2 flex items-center justify-between gap-3 border-t border-ink-900/[0.06] pt-3">
          <p className="cb-muted">{t("pages.profiles.createHint")}</p>
          <div className="flex shrink-0 gap-2">
            <ToolbarButton onClick={onCancel}>{t("actions.cancel")}</ToolbarButton>
            <ToolbarButton icon={<Plus size={13} />} variant="primary" type="submit">
              {t("actions.create")}
            </ToolbarButton>
          </div>
        </div>
      </form>
    </Panel>
  );
}

function DiffLine({ line }: { line: DiffLineView }) {
  const prefix = line.kind === "insert" ? "+" : line.kind === "delete" ? "-" : line.kind === "change" ? "~" : " ";
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
  const [pendingChange, setPendingChange] = useState<PendingProfileChange | null>(null);
  const [writeBusy, setWriteBusy] = useState(false);
  const [selectedId, setSelectedId] = useState(mockProfiles[0].id);
  const selected = profiles.find((item) => item.id === selectedId) || profiles[0];

  const refreshConfigSnapshot = async (nextSelectedId?: string) => {
    const result = await invokeCmd<ConfigSnapshotView>("config_snapshot");
    if (result.ok) {
      const nextProfiles = result.data.profiles.length > 0 ? result.data.profiles : mockProfiles;
      const nextProviders = result.data.providers.length > 0 ? result.data.providers : mockProviders;
      setProfiles(nextProfiles);
      setProviders(nextProviders);
      setConfigPath(result.data.configPath);
      setSelectedId(
        nextSelectedId ||
          nextProfiles.find((profile) => profile.isActive)?.id ||
          nextProfiles[0]?.id ||
          ""
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
        const nextProfiles = result.data.profiles.length > 0 ? result.data.profiles : mockProfiles;
        const nextProviders = result.data.providers.length > 0 ? result.data.providers : mockProviders;
        setProfiles(nextProfiles);
        setProviders(nextProviders);
        setConfigPath(result.data.configPath);
        setSelectedId(
          nextProfiles.find((profile) => profile.isActive)?.id || nextProfiles[0]?.id || ""
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
    const result = await invokeCmd<ConfigChangePreviewView>("config_change_preview", { change });
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
    const result = await invokeCmd<ConfigChangePreviewView>("config_change_preview", { change });
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
    const result = await invokeCmd<ApplyConfigChangeResultView>("config_change_apply", {
      request: {
        change: pendingChange.change,
        expectedHash: pendingChange.preview.expectedHash,
      },
    });
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
      action={<ToolbarButton icon={<Plus size={13} />} variant="primary" onClick={() => { setCreating(true); show("info", t("feedback.profileCreate")); }}>{t("actions.newProfile")}</ToolbarButton>}
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
        <Panel title={t("pages.profiles.writePreviewTitle")} icon={<GitCompare size={15} />}>
          <SummaryStrip
            items={[
              { label: t("fields.configPath"), value: pendingChange.preview.configPath },
              { label: t("diff.insertions"), value: String(pendingChange.preview.insertions), tone: "ok" },
              { label: t("diff.deletions"), value: String(pendingChange.preview.deletions), tone: pendingChange.preview.deletions > 0 ? "warn" : "idle" },
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
          { label: t("summary.activeProfile"), value: profiles.find((item) => item.isActive)?.name || "-" },
          { label: t("summary.safeWrites"), value: t("common.enabled"), tone: "ok" },
        ]}
      />
      <Panel title={t("pages.profiles.sourceTitle")} icon={<GitCompare size={15} />}>
        <DetailRow label={t("fields.configPath")} value={<span className="font-mono break-all">{configPath}</span>} />
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
          <DetailRow label="model" value={<span className="font-mono">{selected?.model || "-"}</span>} />
          <DetailRow label="model_provider" value={<span className="font-mono">{selected?.providerId || "-"}</span>} />
          <DetailRow label="sandbox" value={selected?.sandbox || "-"} />
          <DetailRow label="approval" value={selected?.approval || "-"} />
          <DetailRow label="network" value={selected?.network || "-"} />
          <DetailRow label="mcp_refs" value={selected?.mcpRefs.join(", ") || "-"} />
          <div className="mt-4 flex flex-wrap gap-2">
            <ToolbarButton icon={<CheckCircle2 size={13} />} disabled={!selected || selected.isActive} onClick={() => void previewSetActive()}>{t("actions.setActive")}</ToolbarButton>
            <ToolbarButton icon={<Copy size={13} />} onClick={() => show("info", t("feedback.profileCopied", { name: selected?.name || "-" }))}>{t("actions.duplicate")}</ToolbarButton>
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
  const [pendingChange, setPendingChange] = useState<PendingProviderChange | null>(null);
  const [writeBusy, setWriteBusy] = useState(false);
  const selected = providers.find((item) => item.id === selectedId) || providers[0];

  const refreshConfigSnapshot = async () => {
    const result = await invokeCmd<ConfigSnapshotView>("config_snapshot");
    if (result.ok) {
      const nextProviders = result.data.providers.length > 0 ? result.data.providers : mockProviders;
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
        const nextProviders = result.data.providers.length > 0 ? result.data.providers : mockProviders;
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

    if (!trimmedName || !trimmedBaseUrl || !trimmedEnvKey || models.length === 0) {
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

    const result = await invokeCmd<ConfigChangePreviewView>("config_change_preview", { change });
    if (result.ok) {
      setPendingChange({ change, preview: result.data, providerName: trimmedName });
      setCreating(false);
      show("info", t("feedback.previewDiff"));
    } else {
      show("warning", result.error);
    }
  };

  const applyPendingProvider = async () => {
    if (!pendingChange) return;
    setWriteBusy(true);
    const result = await invokeCmd<ApplyConfigChangeResultView>("config_change_apply", {
      request: {
        change: pendingChange.change,
        expectedHash: pendingChange.preview.expectedHash,
      },
    });
    setWriteBusy(false);
    if (result.ok) {
      setDraft(EMPTY_PROVIDER_DRAFT);
      setPendingChange(null);
      show("success", t("feedback.providerCreated", { name: pendingChange.providerName }));
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
        <Panel title={t("pages.providers.writePreviewTitle")} icon={<GitCompare size={15} />}>
          <SummaryStrip
            items={[
              { label: t("fields.configPath"), value: pendingChange.preview.configPath },
              { label: t("diff.insertions"), value: String(pendingChange.preview.insertions), tone: "ok" },
              { label: t("diff.deletions"), value: String(pendingChange.preview.deletions), tone: pendingChange.preview.deletions > 0 ? "warn" : "idle" },
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
      <Panel title={t("pages.providers.sourceTitle")} icon={<GitCompare size={15} />}>
        <DetailRow label={t("fields.configPath")} value={<span className="font-mono break-all">{configPath}</span>} />
      </Panel>
      <div className="grid grid-cols-[minmax(260px,0.88fr)_minmax(0,1.35fr)] gap-4">
        <Panel title={t("pages.providers.listTitle")} icon={<Server size={15} />}>
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
          action={<ToolbarButton icon={<TestTube2 size={13} />} disabled={!selected} onClick={() => show("success", t("feedback.connectionTested", { name: selected?.name || "-" }))}>{t("actions.testConnection")}</ToolbarButton>}
        >
          <DetailRow label={t("fields.kind")} value={selected ? t(`providerKind.${selected.kind}`) : "-"} />
          <DetailRow label="base_url" value={<span className="font-mono break-all">{selected?.baseUrl || "-"}</span>} />
          <DetailRow label="wire_api" value={<span className="font-mono">{selected?.wireApi || "-"}</span>} />
          <DetailRow label="env" value={selected ? <SecretText value={selected.envKey} /> : "-"} />
          <DetailRow label={t("fields.models")} value={selected?.models.join(", ") || "-"} />
          <div className="mt-4 flex flex-wrap gap-2">
            <ToolbarButton icon={<Copy size={13} />} disabled={!selected} onClick={() => show("info", t("feedback.providerCopied", { name: selected?.name || "-" }))}>{t("actions.copyConfig")}</ToolbarButton>
            <ToolbarButton icon={<GitCompare size={13} />} onClick={() => show("info", t("feedback.previewDiff"))}>{t("actions.previewDiff")}</ToolbarButton>
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
              {t("fields.kind")}: <span className="font-mono">{selected.kind}</span>
            </span>
            <span>
              {t("fields.baseUrl")}: <span className="font-mono break-all">{selected.baseUrl || "-"}</span>
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
 * - 聚合 mockModelCatalog 全部条目
 * - 列出"已激活 profile"对应的模型
 * - 提供 toggle visibility(可见性)和 reasoning 配置入口
 * - 写入走 ~/.opencodex/custom_model_catalog.json
 */
export function ModelsPage() {
  const { t } = useTranslation();
  const { notice, show } = useNotice();
  const [catalog, setCatalog] = useState<ModelCatalogEntry[]>(mockModelCatalog);
  const [config, setConfig] = useState<OpenCodexCustomConfig | null>(null);
  const [activeProfile, setActiveProfile] = useState<ProfileView | null>(null);
  const [selectedId, setSelectedId] = useState<string>(mockModelCatalog[0]?.modelId || "");
  const [busy, setBusy] = useState(false);
  const selected = catalog.find((item) => item.modelId === selectedId) || catalog[0];

  const refresh = useCallback(async () => {
    setBusy(true);
    const snapshot = await invokeCmd<ConfigSnapshotView>("config_snapshot");
    if (snapshot.ok) {
      const profile = snapshot.data.profiles.find((p) => p.isActive) || snapshot.data.profiles[0] || null;
      setActiveProfile(profile);
    } else {
      show("warning", snapshot.error);
    }
    const opencodex = await invokeCmd<OpenCodexCustomConfig>("opencodex_config_read");
    setBusy(false);
    if (opencodex.ok) {
      setConfig(opencodex.data);
      setCatalog(opencodex.data.catalog.length > 0 ? opencodex.data.catalog : mockModelCatalog);
      setSelectedId((current) =>
        current && opencodex.data.catalog.some((entry) => entry.modelId === current)
          ? current
          : (opencodex.data.catalog[0]?.modelId || mockModelCatalog[0]?.modelId || "")
      );
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
    const result = await invokeCmd<OpenCodexWriteResult>("catalog_entry_upsert", { request });
    if (result.ok) {
      show("success", t("feedback.modelVisibilityToggled", { name: entry.displayName || entry.modelId }));
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
        <ToolbarButton icon={<RotateCcw size={13} />} onClick={() => void refresh()} disabled={busy}>
          {t("actions.refresh")}
        </ToolbarButton>
      }
    >
      <SummaryStrip
        items={[
          { label: t("summary.totalModels"), value: String(catalog.length), tone: "ok" },
          { label: t("summary.visibleModels"), value: String(catalog.filter((m) => m.visible).length) },
          { label: t("summary.activeProfile"), value: activeProfile?.name || "-" },
        ]}
      />
      <Panel title={t("pages.models.catalogSourceTitle")} icon={<GitCompare size={15} />}>
        <DetailRow
          label={t("fields.catalogPath")}
          value={<span className="font-mono break-all">{config?.catalogPath || "~/.opencodex/custom_model_catalog.json"}</span>}
        />
        <DetailRow
          label={t("fields.contentHash")}
          value={<span className="font-mono break-all">{config?.catalogContentHash || "-"}</span>}
        />
        {config && !config.valid ? (
          <div className="mt-3 rounded-md border border-status-fail/30 bg-status-fail/10 px-3 py-2 text-[12px] text-status-fail">
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
      <div className="grid grid-cols-[minmax(300px,0.95fr)_minmax(0,1.4fr)] gap-4">
        <Panel title={t("pages.models.listTitle")} icon={<Sparkles size={15} />}>
          <div className="flex flex-col gap-2">
            {catalog.map((entry) => (
              <ListButton
                key={entry.modelId}
                active={entry.modelId === selected?.modelId}
                title={entry.displayName || entry.modelId}
                subtitle={`${entry.provider} · ${entry.modelId}`}
                right={
                  <span className="inline-flex items-center gap-1.5">
                    {entry.reasoning?.enabled ? <StatusPill tone="running" /> : null}
                    {entry.visible ? (
                      <Eye size={12} className="text-status-ok" />
                    ) : (
                      <EyeOff size={12} className="text-ink-400" />
                    )}
                  </span>
                }
                onClick={() => setSelectedId(entry.modelId)}
              />
            ))}
          </div>
        </Panel>
        <Panel
          title={t("pages.models.detailTitle")}
          icon={<Sparkles size={15} />}
          action={
            <ToolbarButton
              icon={selected?.visible ? <EyeOff size={13} /> : <Eye size={13} />}
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
              <DetailRow label={t("fields.provider")} value={selected.provider} />
              <DetailRow
                label={t("fields.displayName")}
                value={selected.displayName || "-"}
              />
              <DetailRow
                label={t("fields.visible")}
                value={selected.visible ? t("common.enabled") : t("common.disabled")}
              />
              <DetailRow
                label={t("fields.reasoning")}
                value={
                  selected.reasoning
                    ? `${selected.reasoning.enabled ? t("common.enabled") : t("common.disabled")} · ${selected.reasoning.levels.join(", ")}`
                    : t("common.disabled")
                }
              />
              <DetailRow label={t("fields.note")} value={selected.note || "-"} />
            </>
          ) : (
            <div className="py-6 text-center text-[12px] text-ink-500">
              {t("common.none")}
            </div>
          )}
        </Panel>
      </div>
      <div className="cb-surface p-4">
        <div className="mb-2 flex items-center gap-2 text-[12px] font-medium text-ink-700">
          <Sparkles size={14} /> {t("pages.models.byokHint")}
        </div>
        <p className="text-[12px] leading-[1.6] text-ink-500">{t("pages.models.byokHintBody")}</p>
      </div>
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
  const [selectedName, setSelectedName] = useState<string>(mockProviderRoutes[0]?.name || "");
  const [busy, setBusy] = useState(false);
  const selected = routes.find((item) => item.name === selectedName) || routes[0];

  const refresh = useCallback(async () => {
    setBusy(true);
    const result = await invokeCmd<OpenCodexCustomConfig>("opencodex_config_read");
    setBusy(false);
    if (result.ok) {
      setConfig(result.data);
      setRoutes(result.data.providers.length > 0 ? result.data.providers : mockProviderRoutes);
      setSelectedName((current) =>
        current && result.data.providers.some((entry) => entry.name === current)
          ? current
          : (result.data.providers[0]?.name || mockProviderRoutes[0]?.name || "")
      );
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
    const result = await invokeCmd<OpenCodexWriteResult>("provider_route_upsert", { request });
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
    const result = await invokeCmd<OpenCodexWriteResult>("provider_route_delete", { request });
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
        <ToolbarButton icon={<RotateCcw size={13} />} onClick={() => void refresh()} disabled={busy}>
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
      <Panel title={t("pages.providerRoutes.sourceTitle")} icon={<GitCompare size={15} />}>
        <DetailRow
          label={t("fields.providersPath")}
          value={<span className="font-mono break-all">{config?.providersPath || "~/.opencodex/providers.json"}</span>}
        />
        <DetailRow
          label={t("fields.contentHash")}
          value={<span className="font-mono break-all">{config?.providersContentHash || "-"}</span>}
        />
      </Panel>
      <div className="grid grid-cols-[minmax(280px,0.95fr)_minmax(0,1.4fr)] gap-4">
        <Panel title={t("pages.providerRoutes.listTitle")} icon={<Route size={15} />}>
          <div className="flex flex-col gap-2">
            {routes.map((route) => (
              <ListButton
                key={route.name}
                active={route.name === selected?.name}
                title={route.name}
                subtitle={`${route.wireApi} · ${route.baseUrl}`}
                right={<StatusPill tone={route.enabled ? "ok" : "idle"} />}
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
              <DetailRow label="name" value={<span className="font-mono">{selected.name}</span>} />
              <DetailRow label="base_url" value={<span className="font-mono break-all">{selected.baseUrl}</span>} />
              <DetailRow label="wire_api" value={<span className="font-mono">{selected.wireApi}</span>} />
              <DetailRow label="api_key_ref" value={selected.apiKeyRef ? <SecretText value={selected.apiKeyRef} /> : "-"} />
              <DetailRow
                label={t("fields.enabled")}
                value={selected.enabled ? t("common.enabled") : t("common.disabled")}
              />
              <DetailRow label={t("fields.note")} value={selected.note || "-"} />
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
 * Codex Runtime 页面:Codex Desktop / CLI / ~/.opencodex/ 只读检测
 * 取代 v0.2 的"OpenCodex 检出"语义:不再有外部 OpenCodex 进程,只剩"Codex 桌面安装检测 + BYOK JSON 文件位置"。
 */
export function CodexRuntimePage() {
  const { t } = useTranslation();
  const { notice, show } = useNotice();
  const [status, setStatus] = useState<CodexRuntimeStatus>(mockCodexRuntime);
  const [opencodex, setOpencodex] = useState<OpenCodexCustomConfig | null>(null);
  const [loading, setLoading] = useState(false);

  const refresh = useCallback(async () => {
    setLoading(true);
    // 当前 MVP 阶段 codex_runtime_status 没有实现,直接展示 mock + opencodex read
    void invokeCmd<OpenCodexCustomConfig>("opencodex_config_read").then((result) => {
      if (result.ok) {
        setOpencodex(result.data);
        setStatus((current) => ({
          ...current,
          opencodexDir: result.data.providersPath.replace(/\/[^/]+$/, ""),
          opencodexDirExists: true,
        }));
      } else {
        show("warning", result.error);
      }
    });
    setLoading(false);
  }, [show]);

  useEffect(() => {
    void refresh();
  }, [refresh]);

  return (
    <PageShell
      title={t("pages.codexRuntime.title")}
      subtitle={t("pages.codexRuntime.subtitle")}
      notice={notice}
      action={
        <ToolbarButton icon={<RotateCcw size={13} />} onClick={() => void refresh()} disabled={loading}>
          {t("actions.refresh")}
        </ToolbarButton>
      }
    >
      <SummaryStrip
        items={[
          { label: t("fields.desktopInstalled"), value: status.desktopInstalled ? t("common.detected") : t("common.missing"), tone: status.desktopInstalled ? "ok" : "warn" },
          { label: t("fields.cliAvailable"), value: status.cliAvailable ? t("common.detected") : t("common.missing"), tone: status.cliAvailable ? "ok" : "warn" },
          { label: t("fields.opencodexDir"), value: opencodex ? t("common.detected") : t("common.notFound") },
        ]}
      />
      <div className="grid grid-cols-1 lg:grid-cols-2 gap-4">
        <Panel title={t("pages.codexRuntime.desktopTitle")} icon={<Server size={15} />}>
          <DetailRow
            label={t("fields.codexHome")}
            value={<span className="font-mono break-all">{status.codexHome}</span>}
          />
          <DetailRow
            label={t("fields.codexCliPath")}
            value={<span className="font-mono break-all">{status.codexCliPath || t("common.unknown")}</span>}
          />
          <DetailRow
            label={t("fields.codexDesktopAppPath")}
            value={<span className="font-mono break-all">{status.codexDesktopAppPath || t("common.unknown")}</span>}
          />
          <DetailRow
            label={t("fields.codexDesktopVersion")}
            value={status.codexDesktopVersion || t("common.unknown")}
          />
          <DetailRow
            label={t("fields.configReadable")}
            value={status.configReadable ? t("common.enabled") : t("common.disabled")}
          />
        </Panel>
        <Panel title={t("pages.codexRuntime.opencodexTitle")} icon={<Cpu size={15} />}>
          <DetailRow
            label={t("fields.opencodexDir")}
            value={
              <span className="font-mono break-all">
                {opencodex ? opencodex.providersPath.replace(/\/[^/]+$/, "") : status.opencodexDir}
              </span>
            }
          />
          <DetailRow
            label={t("fields.providersPath")}
            value={<span className="font-mono break-all">{opencodex?.providersPath || "~/.opencodex/providers.json"}</span>}
          />
          <DetailRow
            label={t("fields.catalogPath")}
            value={<span className="font-mono break-all">{opencodex?.catalogPath || "~/.opencodex/custom_model_catalog.json"}</span>}
          />
          <DetailRow
            label={t("fields.providersCount")}
            value={String(opencodex?.providers.length || 0)}
          />
          <DetailRow
            label={t("fields.catalogCount")}
            value={String(opencodex?.catalog.length || 0)}
          />
          <DetailRow
            label={t("fields.readAt")}
            value={<span className="font-mono break-all">{opencodex?.readAt || "-"}</span>}
          />
        </Panel>
      </div>
      <div className="cb-surface p-4">
        <div className="mb-2 flex items-center gap-2 text-[12px] font-medium text-ink-700">
          <ShieldCheck size={14} /> {t("pages.codexRuntime.readonlyHint")}
        </div>
        <p className="text-[12px] leading-[1.6] text-ink-500">
          {t("pages.codexRuntime.readonlyHintBody")}
        </p>
      </div>
    </PageShell>
  );
}

export function DiagnosticsPage() {
  const { t } = useTranslation();
  const { notice, show } = useNotice();
  const [running, setRunning] = useState(false);
  const total = mockDiagnostics.reduce((sum, group) => sum + group.items.length, 0);
  const warn = mockDiagnostics.flatMap((group) => group.items).filter((item) => item.status === "warn").length;

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
          { label: t("summary.warnings"), value: String(warn), tone: warn > 0 ? "warn" : "ok" },
          { label: t("summary.report"), value: t("common.redacted"), tone: "ok" },
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
              <div className="truncate text-[13px] font-medium text-ink-800">{t(item.labelKey)}</div>
              <div className="mt-0.5 truncate text-[11px] text-ink-500">{item.detail}</div>
            </div>
            {item.latencyMs && <span className="text-[11px] text-ink-400">{item.latencyMs} ms</span>}
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
  const selected = mockSettingsSections.find((item) => item.id === selectedId) || mockSettingsSections[0];

  return (
    <PageShell
      title={t("pages.settings.title")}
      subtitle={t("pages.settings.subtitle")}
      notice={notice}
    >
      <div className="grid grid-cols-[minmax(230px,0.72fr)_minmax(0,1.5fr)] gap-4">
        <Panel title={t("pages.settings.sectionsTitle")} icon={<Settings size={15} />}>
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
                  setToggles((current) => ({ ...current, [key]: !current[key] }));
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
    <button className="flex w-full items-center gap-4 py-3 text-left" onClick={onChange}>
      <div className="min-w-0 flex-1">
        <div className="text-[13px] font-medium text-ink-800">{label}</div>
        <div className="mt-0.5 text-[12px] leading-[1.55] text-ink-500">{desc}</div>
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