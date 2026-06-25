import { useCallback, useEffect, useState, type ReactNode } from "react";
import { useTranslation } from "react-i18next";
import {
  Activity,
  AlertTriangle,
  Cable,
  CheckCircle2,
  ChevronRight,
  Copy,
  Cpu,
  GitCompare,
  Globe,
  KeyRound,
  Languages,
  Network,
  Play,
  Plus,
  Power,
  Puzzle,
  RotateCcw,
  Save,
  Server,
  Settings,
  ShieldCheck,
  Smartphone,
  Square,
  TestTube2,
  Trash2,
  Users,
} from "lucide-react";
import { invokeCmd } from "@/lib/api";
import { setLanguage } from "@/lib/i18n";
import {
  mockDiagnostics,
  mockDiffLines,
  mockGateways,
  mockProfiles,
  mockProviders,
  mockSettingsSections,
} from "@/lib/mock-data";
import type {
  ApplyConfigChangeResultView,
  ConfigChangePreviewView,
  ConfigChangeRequest,
  ConfigSnapshotView,
  DiagnosticGroupView,
  DiffLineView,
  OpenCodexStatus,
  ProfileView,
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
      {/* MCP 子区块:展示当前 profile 引用的 MCP server 列表,只读 */}
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
      {/* Network 子区块:展示 provider 自身的网络特征,只读 */}
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

export function GatewayPage() {
  const { t } = useTranslation();
  const { notice, show } = useNotice();
  const [openCodex, setOpenCodex] = useState<OpenCodexStatus | null>(null);
  const [openCodexBusy, setOpenCodexBusy] = useState(false);
  const [selectedId, setSelectedId] = useState(mockGateways[0].id);
  const [running, setRunning] = useState<Record<string, boolean>>({});
  const selected = mockGateways.find((item) => item.id === selectedId) || mockGateways[0];
  const isRunning = !!running[selected.id];
  const status: StatusTone = isRunning ? "running" : selected.status;
  const openCodexTone: StatusTone = !openCodex?.exists
    ? "fail"
    : openCodex.running
      ? "running"
      : "idle";

  const refreshOpenCodex = async () => {
    const result = await invokeCmd<OpenCodexStatus>("opencodex_status");
    if (result.ok) {
      setOpenCodex(result.data);
    } else {
      show("warning", result.error);
    }
  };

  const runOpenCodexAction = async (
    command:
      | "opencodex_start"
      | "opencodex_start_lan"
      | "opencodex_stop"
      | "opencodex_restart"
      | "opencodex_restart_lan"
      | "opencodex_open_logs",
    successKey: string
  ) => {
    setOpenCodexBusy(true);
    const result = await invokeCmd<OpenCodexStatus>(command);
    setOpenCodexBusy(false);
    if (result.ok) {
      setOpenCodex(result.data);
      show("success", t(successKey));
    } else {
      show("warning", result.error);
      void refreshOpenCodex();
    }
  };

  const openOpenCodexUrl = async (kind: "local" | "mobile") => {
    const result = await invokeCmd<OpenCodexStatus>("opencodex_open_url", { kind });
    if (result.ok) {
      setOpenCodex(result.data);
      show("info", t("feedback.openCodexUrlOpened"));
    } else {
      show("warning", result.error);
    }
  };

  useEffect(() => {
    void refreshOpenCodex();
  }, []);

  return (
    <PageShell
      title={t("pages.gateway.title")}
      subtitle={t("pages.gateway.subtitle")}
      notice={notice}
      action={<ToolbarButton icon={<Plus size={13} />} variant="primary" onClick={() => show("info", t("feedback.gatewayPreset"))}>{t("actions.importPreset")}</ToolbarButton>}
    >
      <Panel
        title={t("pages.gateway.openCodexTitle")}
        icon={<Cable size={15} />}
        action={<StatusPill tone={openCodexTone} />}
      >
        <SummaryStrip
          items={[
            {
              label: t("fields.source"),
              value: openCodex?.exists ? t("common.detected") : t("common.notFound"),
              tone: openCodex?.exists ? "ok" : "fail",
            },
            {
              label: t("fields.status"),
              value: openCodex?.running ? t("status.running") : t("status.idle"),
              tone: openCodexTone,
            },
            {
              label: "health",
              value: openCodex?.healthOk ? "200" : openCodex?.healthStatus ? String(openCodex.healthStatus) : "-",
              tone: openCodex?.healthOk ? "ok" : "idle",
            },
          ]}
        />
        <div className="mt-4">
          <DetailRow label={t("fields.sourcePath")} value={<span className="font-mono break-all">{openCodex?.sourcePath || "-"}</span>} />
          <DetailRow label="host / port" value={`${openCodex?.host || "-"}:${openCodex?.port || "-"}`} />
          <DetailRow label={t("fields.localUrl")} value={<span className="font-mono break-all">{openCodex?.localUrl || "-"}</span>} />
          <DetailRow
            label={t("fields.mobileUrl")}
            value={
              <span className="font-mono break-all">
                {openCodex?.mobileUrl || t("common.none")}
              </span>
            }
          />
          <DetailRow
            label={t("fields.lanAccess")}
            value={openCodex?.lanAccessEnabled ? t("common.enabled") : t("common.disabled")}
          />
          <DetailRow label="CODEX_HOME" value={<span className="font-mono break-all">{openCodex?.codexHome || "-"}</span>} />
          <DetailRow label={t("fields.sharedCodexHome")} value={<span className="font-mono break-all">{openCodex?.sharedCodexHome || "-"}</span>} />
          <DetailRow label={t("fields.health")} value={<span className="font-mono break-all">{openCodex?.healthEndpoint || "-"}</span>} />
          <DetailRow label={t("fields.logPath")} value={<span className="font-mono break-all">{openCodex?.logPath || "-"}</span>} />
          <DetailRow label={t("fields.password")} value={openCodex?.authPasswordConfigured ? t("common.configured") : t("common.notConfigured")} />
          {openCodex?.lastError && (
            <DetailRow label={t("fields.lastError")} value={<span className="text-status-fail">{openCodex.lastError}</span>} />
          )}
        </div>
        <div className="mt-3 rounded-md border border-status-warn/25 bg-status-warn/10 px-3 py-2 text-[12px] leading-[1.55] text-status-warn">
          {t("pages.gateway.lanWarning")}
        </div>
        <div className="mt-4 flex flex-wrap gap-2">
          <ToolbarButton
            icon={<Play size={13} />}
            variant="primary"
            disabled={openCodexBusy || !openCodex?.exists || openCodex.running}
            onClick={() => void runOpenCodexAction("opencodex_start", "feedback.openCodexStarted")}
          >
            {t("actions.start")}
          </ToolbarButton>
          <ToolbarButton
            icon={<Globe size={13} />}
            disabled={openCodexBusy || !openCodex?.exists || openCodex.running || !openCodex.authPasswordConfigured}
            onClick={() => void runOpenCodexAction("opencodex_start_lan", "feedback.openCodexLanStarted")}
          >
            {t("actions.startLan")}
          </ToolbarButton>
          <ToolbarButton
            icon={<Square size={13} />}
            disabled={openCodexBusy || !openCodex?.managed}
            onClick={() => void runOpenCodexAction("opencodex_stop", "feedback.openCodexStopped")}
          >
            {t("actions.stop")}
          </ToolbarButton>
          <ToolbarButton
            icon={<RotateCcw size={13} />}
            disabled={openCodexBusy || !openCodex?.exists}
            onClick={() => void runOpenCodexAction("opencodex_restart", "feedback.openCodexRestarted")}
          >
            {t("actions.restart")}
          </ToolbarButton>
          <ToolbarButton
            icon={<RotateCcw size={13} />}
            disabled={openCodexBusy || !openCodex?.exists || !openCodex.authPasswordConfigured}
            onClick={() => void runOpenCodexAction("opencodex_restart_lan", "feedback.openCodexLanStarted")}
          >
            {t("actions.restartLan")}
          </ToolbarButton>
          <ToolbarButton
            icon={<Globe size={13} />}
            disabled={!openCodex?.localUrl}
            onClick={() => void openOpenCodexUrl("local")}
          >
            {t("actions.openLocalUrl")}
          </ToolbarButton>
          <ToolbarButton
            icon={<Globe size={13} />}
            disabled={!openCodex?.mobileUrlReachable}
            onClick={() => void openOpenCodexUrl("mobile")}
          >
            {t("actions.openMobileUrl")}
          </ToolbarButton>
          <ToolbarButton
            icon={<Activity size={13} />}
            disabled={openCodexBusy}
            onClick={() => void runOpenCodexAction("opencodex_open_logs", "feedback.openLogs")}
          >
            {t("actions.openLogs")}
          </ToolbarButton>
          <ToolbarButton icon={<TestTube2 size={13} />} disabled={openCodexBusy} onClick={() => void refreshOpenCodex()}>
            {t("actions.refresh")}
          </ToolbarButton>
        </div>
      </Panel>
      <div className="grid grid-cols-[minmax(260px,0.85fr)_minmax(0,1.35fr)] gap-4">
        <Panel title={t("pages.gateway.presetsTitle")} icon={<Cable size={15} />}>
          <div className="flex flex-col gap-2">
            {mockGateways.map((gateway) => (
              <ListButton
                key={gateway.id}
                active={gateway.id === selected.id}
                title={gateway.name}
                subtitle={`${gateway.host}:${gateway.port} / ${gateway.adapter}`}
                right={<StatusPill tone={running[gateway.id] ? "running" : gateway.status} />}
                onClick={() => setSelectedId(gateway.id)}
              />
            ))}
          </div>
        </Panel>
        <Panel title={t("pages.gateway.runtimeTitle")} icon={<Activity size={15} />} action={<StatusPill tone={status} />}>
          <SummaryStrip
            items={[
              { label: "host", value: selected.host },
              { label: "port", value: String(selected.port) },
              { label: t("fields.status"), value: t(`status.${status}`), tone: status },
            ]}
          />
          <div className="mt-4">
            <DetailRow label={t("fields.adapter")} value={selected.adapter} />
            <DetailRow label={t("fields.health")} value={<span className="font-mono">{selected.healthPath}</span>} />
            <DetailRow label={t("fields.logPath")} value={<span className="font-mono break-all">{selected.logPath}</span>} />
          </div>
          <div className="mt-4 flex flex-wrap gap-2">
            <ToolbarButton
              icon={<Play size={13} />}
              variant="primary"
              disabled={isRunning}
              onClick={() => {
                setRunning((current) => ({ ...current, [selected.id]: true }));
                show("success", t("feedback.gatewayStarted", { name: selected.name }));
              }}
            >
              {t("actions.start")}
            </ToolbarButton>
            <ToolbarButton
              icon={<Square size={13} />}
              disabled={!isRunning}
              onClick={() => {
                setRunning((current) => ({ ...current, [selected.id]: false }));
                show("warning", t("feedback.gatewayStopped", { name: selected.name }));
              }}
            >
              {t("actions.stop")}
            </ToolbarButton>
            <ToolbarButton icon={<Activity size={13} />} onClick={() => show("info", t("feedback.openLogs"))}>{t("actions.openLogs")}</ToolbarButton>
          </div>
        </Panel>
      </div>
      {/* Config Diff 子区块:展示 Gateway 启动可能改的 config 范围占位,M3 接入真实 diff */}
      <div className="cb-surface p-4">
        <div className="mb-2 flex items-center gap-2 text-[12px] font-medium text-ink-700">
          <GitCompare size={14} /> {t("pages.gateway.diffSubsection")}
        </div>
        <div className="rounded-md bg-ink-900/5 p-3 font-mono text-[11px] leading-[1.7] text-ink-700">
          <div className="text-ink-400"># Gateway 启动不会修改 ~/.codex/config.toml</div>
          <div className="text-ink-400"># Gateway 只会改 ~/.codex/codex-box/opencodex/runtime 与 logs</div>
          <div>
            <span className="text-[#34C759]">+ </span>
            CODEX_HOME={`"$HOME/.codex"`}
          </div>
          <div>
            <span className="text-[#34C759]">+ </span>
            HOST={"127.0.0.1"} PORT={"3737"}
          </div>
          <div className="text-ink-400"># 实际写入流程(draft → backup → diff → confirm → atomic write)M3 接入</div>
        </div>
      </div>
    </PageShell>
  );
}

export function MobileAccessPage() {
  const { t } = useTranslation();
  const { notice, show } = useNotice();
  const [status, setStatus] = useState<OpenCodexStatus | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const refresh = useCallback(async () => {
    setLoading(true);
    setError(null);
    const result = await invokeCmd<OpenCodexStatus>("opencodex_status");
    setLoading(false);
    if (result.ok) {
      setStatus(result.data);
    } else {
      setError(result.error);
    }
  }, []);

  useEffect(() => {
    void refresh();
  }, [refresh]);

  const onStart = useCallback(async () => {
    setLoading(true);
    setError(null);
    const result = await invokeCmd<OpenCodexStatus>("opencodex_start");
    setLoading(false);
    if (result.ok) {
      setStatus(result.data);
      show("success", t("feedback.gatewayStarted"));
    } else {
      setError(result.error);
      show("warning", result.error);
    }
  }, [show, t]);

  const onStartLan = useCallback(async () => {
    setLoading(true);
    setError(null);
    const result = await invokeCmd<OpenCodexStatus>("opencodex_start_lan");
    setLoading(false);
    if (result.ok) {
      setStatus(result.data);
      show("success", t("feedback.gatewayStartedLan"));
    } else {
      setError(result.error);
      show("warning", result.error);
    }
  }, [show, t]);

  const onOpenMobile = useCallback(async () => {
    setError(null);
    const result = await invokeCmd<OpenCodexStatus>("opencodex_open_url", { kind: "mobile" });
    if (!result.ok) {
      setError(result.error);
      show("warning", result.error);
    }
  }, [show]);

  const onOpenLocal = useCallback(async () => {
    setError(null);
    const result = await invokeCmd<OpenCodexStatus>("opencodex_open_url", { kind: null });
    if (!result.ok) {
      setError(result.error);
      show("warning", result.error);
    }
  }, [show]);

  return (
    <PageShell
      title={t("pages.mobileAccess.title")}
      subtitle={t("pages.mobileAccess.subtitle")}
      notice={notice}
      action={
        <div className="flex flex-wrap items-center gap-2">
          <ToolbarButton icon={<RotateCcw size={13} />} onClick={() => void refresh()} disabled={loading}>
            {t("actions.refresh")}
          </ToolbarButton>
          {status?.running ? (
            <ToolbarButton icon={<Square size={13} />} onClick={onStartLan} disabled={loading}>
              {t("actions.startLan")}
            </ToolbarButton>
          ) : (
            <ToolbarButton icon={<Play size={13} />} variant="primary" onClick={onStart} disabled={loading}>
              {t("actions.startLocal")}
            </ToolbarButton>
          )}
        </div>
      }
    >
      <div className="grid grid-cols-[minmax(260px,0.85fr)_minmax(0,1.35fr)] gap-4">
        <Panel title={t("pages.mobileAccess.qrTitle")} icon={<Smartphone size={15} />}>
          {status?.mobileUrl ? (
            <div className="flex flex-col items-center gap-3 py-4">
              {/* 二维码占位:M3 接入 qrcode.react 后替换 */}
              <div className="flex h-40 w-40 items-center justify-center rounded-md border border-ink-900/10 bg-white/70 text-[11px] text-ink-500">
                {t("pages.mobileAccess.qrPlaceholder")}
              </div>
              <code className="break-all rounded bg-ink-900/5 px-2 py-1 text-[12px] text-ink-700">
                {status.mobileUrl}
              </code>
            </div>
          ) : (
            <div className="py-6 text-center text-[12px] text-ink-500">
              {t("pages.mobileAccess.lanOffHint")}
            </div>
          )}
          <div className="mt-2 flex flex-wrap gap-2">
            <ToolbarButton icon={<Cable size={13} />} onClick={onOpenLocal}>
              {t("actions.openLocalUrl")}
            </ToolbarButton>
            <ToolbarButton icon={<Smartphone size={13} />} onClick={onOpenMobile} disabled={!status?.mobileUrlReachable}>
              {t("actions.openMobileUrl")}
            </ToolbarButton>
          </div>
        </Panel>
        <Panel title={t("pages.mobileAccess.infoTitle")} icon={<ShieldCheck size={15} />}>
          <DetailRow label={t("fields.localUrl")} value={<span className="font-mono">{status?.localUrl ?? t("common.unknown")}</span>} />
          <DetailRow
            label={t("fields.lanUrl")}
            value={
              status?.lanUrls?.length
                ? status.lanUrls.map((u) => <div key={u} className="font-mono">{u}</div>)
                : t("common.none")
            }
          />
          <DetailRow
            label={t("fields.lanPassword")}
            value={status?.authPasswordConfigured ? t("common.configured") : t("common.notConfigured")}
          />
          <DetailRow label={t("fields.running")} value={status?.running ? t("common.running") : t("common.stopped")} />
          {error ? (
            <div className="mt-3 rounded-md border border-[#FF9A9A]/40 bg-[#FFD6D6]/40 px-3 py-2 text-[12px] text-[#9B2C2C]">
              {error}
            </div>
          ) : null}
        </Panel>
      </div>
    </PageShell>
  );
}

export function CodexRuntimePage() {
  const { t } = useTranslation();
  const { notice } = useNotice();
  const [status, setStatus] = useState<OpenCodexStatus | null>(null);
  const [loading, setLoading] = useState(false);

  const refresh = useCallback(async () => {
    setLoading(true);
    const result = await invokeCmd<OpenCodexStatus>("opencodex_status");
    setLoading(false);
    if (result.ok) {
      setStatus(result.data);
    }
  }, []);

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
      <div className="grid grid-cols-1 lg:grid-cols-2 gap-4">
        <Panel title={t("pages.codexRuntime.checkoutTitle")} icon={<Server size={15} />}>
          <DetailRow
            label={t("fields.opencodexSource")}
            value={<span className="font-mono break-all">{status?.sourcePath ?? t("common.unknown")}</span>}
          />
          <DetailRow
            label={t("fields.exists")}
            value={status?.exists ? t("common.found") : t("common.missing")}
          />
          <DetailRow
            label={t("fields.configYaml")}
            value={<span className="font-mono break-all">{status?.configYamlPath ?? t("common.unknown")}</span>}
          />
          <DetailRow
            label={t("fields.configExists")}
            value={status?.configExists ? t("common.found") : t("common.missing")}
          />
        </Panel>
        <Panel title={t("pages.codexRuntime.sharedTitle")} icon={<Cpu size={15} />}>
          <DetailRow
            label={t("fields.codexHome")}
            value={<span className="font-mono break-all">{status?.codexHome ?? t("common.unknown")}</span>}
          />
          <DetailRow
            label={t("fields.runtimeDir")}
            value={<span className="font-mono break-all">{status?.runtimeDir ?? t("common.unknown")}</span>}
          />
          <DetailRow
            label={t("fields.logPath")}
            value={<span className="font-mono break-all">{status?.logPath ?? t("common.unknown")}</span>}
          />
          <DetailRow
            label={t("fields.healthEndpoint")}
            value={<span className="font-mono break-all">{status?.healthEndpoint ?? t("common.unknown")}</span>}
          />
          <DetailRow
            label={t("fields.health")}
            value={
              status?.healthOk
                ? `${t("common.ok")}${status.healthStatus ? ` (${status.healthStatus})` : ""}`
                : t("common.notReady")
            }
          />
        </Panel>
      </div>
    </PageShell>
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
    gatewayPresets: true,
    pluginDirs: false,
    desktopScan: false,
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
