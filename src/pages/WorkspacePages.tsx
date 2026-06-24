import { useState, type ReactNode } from "react";
import { useTranslation } from "react-i18next";
import {
  Activity,
  AlertTriangle,
  Cable,
  CheckCircle2,
  ChevronRight,
  Copy,
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
  Square,
  TestTube2,
  Trash2,
  Users,
} from "lucide-react";
import { setLanguage } from "@/lib/i18n";
import {
  mockDiagnostics,
  mockDiffLines,
  mockGateways,
  mockMcpServers,
  mockNetworkRoutes,
  mockProfiles,
  mockProviders,
  mockSettingsSections,
} from "@/lib/mock-data";
import type { DiagnosticGroupView, DiffLineView, StatusTone } from "@/lib/types";
import type { ProviderView } from "@/lib/types";

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
  const masked = value.includes("KEY") ? value : "env ref";
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

export function ProfilesPage() {
  const { t } = useTranslation();
  const { notice, show } = useNotice();
  const [selectedId, setSelectedId] = useState(mockProfiles[0].id);
  const selected = mockProfiles.find((item) => item.id === selectedId) || mockProfiles[0];

  return (
    <PageShell
      title={t("pages.profiles.title")}
      subtitle={t("pages.profiles.subtitle")}
      notice={notice}
      action={<ToolbarButton icon={<Plus size={13} />} variant="primary" onClick={() => show("info", t("feedback.profileCreate"))}>{t("actions.newProfile")}</ToolbarButton>}
    >
      <SummaryStrip
        items={[
          { label: t("summary.totalProfiles"), value: String(mockProfiles.length) },
          { label: t("summary.activeProfile"), value: mockProfiles.find((item) => item.isActive)?.name || "-" },
          { label: t("summary.safeWrites"), value: t("common.enabled"), tone: "ok" },
        ]}
      />
      <div className="grid grid-cols-[minmax(240px,0.82fr)_minmax(0,1.35fr)] gap-4">
        <Panel title={t("pages.profiles.listTitle")} icon={<Users size={15} />}>
          <div className="flex flex-col gap-2">
            {mockProfiles.map((profile) => (
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
          action={<StatusPill tone={selected.status} />}
        >
          <DetailRow label="model" value={<span className="font-mono">{selected.model}</span>} />
          <DetailRow label="model_provider" value={<span className="font-mono">{selected.providerId}</span>} />
          <DetailRow label="sandbox" value={selected.sandbox} />
          <DetailRow label="approval" value={selected.approval} />
          <DetailRow label="network" value={selected.network} />
          <DetailRow label="mcp_refs" value={selected.mcpRefs.join(", ")} />
          <div className="mt-4 flex flex-wrap gap-2">
            <ToolbarButton icon={<CheckCircle2 size={13} />} onClick={() => show("success", t("feedback.profileActivated", { name: selected.name }))}>{t("actions.setActive")}</ToolbarButton>
            <ToolbarButton icon={<Copy size={13} />} onClick={() => show("info", t("feedback.profileCopied", { name: selected.name }))}>{t("actions.duplicate")}</ToolbarButton>
            <ConfirmButton
              idleLabel={t("actions.delete")}
              confirmLabel={t("actions.confirmDelete")}
              disabled={selected.isActive}
              onConfirm={() => show("warning", t("feedback.dangerBlocked"))}
            />
          </div>
        </Panel>
      </div>
    </PageShell>
  );
}

export function ProvidersPage() {
  const { t } = useTranslation();
  const { notice, show } = useNotice();
  const [providers, setProviders] = useState<ProviderView[]>(mockProviders);
  const [selectedId, setSelectedId] = useState(mockProviders[0].id);
  const [creating, setCreating] = useState(false);
  const [draft, setDraft] = useState<ProviderDraft>(EMPTY_PROVIDER_DRAFT);
  const selected = providers.find((item) => item.id === selectedId) || providers[0];

  const createProvider = () => {
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

    const nextProvider: ProviderView = {
      id: nextId,
      name: trimmedName,
      kind: draft.kind,
      baseUrl: trimmedBaseUrl,
      wireApi: draft.wireApi,
      envKey: trimmedEnvKey,
      status: "idle",
      models,
    };

    setProviders((current) => [...current, nextProvider]);
    setSelectedId(nextProvider.id);
    setDraft(EMPTY_PROVIDER_DRAFT);
    setCreating(false);
    show("success", t("feedback.providerCreated", { name: nextProvider.name }));
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
          action={<ToolbarButton icon={<TestTube2 size={13} />} onClick={() => show("success", t("feedback.connectionTested", { name: selected.name }))}>{t("actions.testConnection")}</ToolbarButton>}
        >
          <DetailRow label={t("fields.kind")} value={t(`providerKind.${selected.kind}`)} />
          <DetailRow label="base_url" value={<span className="font-mono break-all">{selected.baseUrl}</span>} />
          <DetailRow label="wire_api" value={<span className="font-mono">{selected.wireApi}</span>} />
          <DetailRow label="env" value={<SecretText value={selected.envKey} />} />
          <DetailRow label={t("fields.models")} value={selected.models.join(", ")} />
          <div className="mt-4 flex flex-wrap gap-2">
            <ToolbarButton icon={<Copy size={13} />} onClick={() => show("info", t("feedback.providerCopied", { name: selected.name }))}>{t("actions.copyConfig")}</ToolbarButton>
            <ToolbarButton icon={<GitCompare size={13} />} onClick={() => show("info", t("feedback.previewDiff"))}>{t("actions.previewDiff")}</ToolbarButton>
          </div>
        </Panel>
      </div>
    </PageShell>
  );
}

export function GatewayPage() {
  const { t } = useTranslation();
  const { notice, show } = useNotice();
  const [selectedId, setSelectedId] = useState(mockGateways[0].id);
  const [running, setRunning] = useState<Record<string, boolean>>({});
  const selected = mockGateways.find((item) => item.id === selectedId) || mockGateways[0];
  const isRunning = !!running[selected.id];
  const status: StatusTone = isRunning ? "running" : selected.status;

  return (
    <PageShell
      title={t("pages.gateway.title")}
      subtitle={t("pages.gateway.subtitle")}
      notice={notice}
      action={<ToolbarButton icon={<Plus size={13} />} variant="primary" onClick={() => show("info", t("feedback.gatewayPreset"))}>{t("actions.importPreset")}</ToolbarButton>}
    >
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
    </PageShell>
  );
}

export function McpServersPage() {
  const { t } = useTranslation();
  const { notice, show } = useNotice();
  const [selectedId, setSelectedId] = useState(mockMcpServers[0].id);
  const [enabled, setEnabled] = useState<Record<string, boolean>>(
    Object.fromEntries(mockMcpServers.map((server) => [server.id, server.enabled]))
  );
  const selected = mockMcpServers.find((item) => item.id === selectedId) || mockMcpServers[0];

  return (
    <PageShell
      title={t("pages.mcp.title")}
      subtitle={t("pages.mcp.subtitle")}
      notice={notice}
      action={<ToolbarButton icon={<Plus size={13} />} variant="primary" onClick={() => show("info", t("feedback.mcpCreate"))}>{t("actions.addMcp")}</ToolbarButton>}
    >
      <div className="grid grid-cols-[minmax(260px,0.85fr)_minmax(0,1.35fr)] gap-4">
        <Panel title={t("pages.mcp.listTitle")} icon={<Puzzle size={15} />}>
          <div className="flex flex-col gap-2">
            {mockMcpServers.map((server) => (
              <ListButton
                key={server.id}
                active={server.id === selected.id}
                title={server.name}
                subtitle={`${server.transport} / ${enabled[server.id] ? t("common.enabled") : t("common.disabled")}`}
                right={<StatusPill tone={server.status} />}
                onClick={() => setSelectedId(server.id)}
              />
            ))}
          </div>
        </Panel>
        <Panel title={t("pages.mcp.detailTitle")} icon={<Server size={15} />}>
          <DetailRow label={t("fields.transport")} value={selected.transport} />
          <DetailRow label={selected.transport === "stdio" ? "command" : "url"} value={<span className="font-mono break-all">{selected.commandOrUrl}</span>} />
          <DetailRow label={t("fields.enabled")} value={enabled[selected.id] ? t("common.enabled") : t("common.disabled")} />
          <DetailRow label="env_refs" value={selected.envRefs.length ? selected.envRefs.join(", ") : t("common.none")} />
          <div className="mt-4 flex flex-wrap gap-2">
            <ToolbarButton
              icon={<Power size={13} />}
              onClick={() => {
                setEnabled((current) => ({ ...current, [selected.id]: !current[selected.id] }));
                show("success", t("feedback.mcpToggled", { name: selected.name }));
              }}
            >
              {enabled[selected.id] ? t("actions.disable") : t("actions.enable")}
            </ToolbarButton>
            <ToolbarButton icon={<TestTube2 size={13} />} onClick={() => show("success", t("feedback.mcpTested", { name: selected.name }))}>{t("actions.test")}</ToolbarButton>
          </div>
        </Panel>
      </div>
    </PageShell>
  );
}

export function NetworkPage() {
  const { t } = useTranslation();
  const { notice, show } = useNotice();
  const [selectedId, setSelectedId] = useState(mockNetworkRoutes[0].id);
  const [latencies, setLatencies] = useState<Record<string, number | null>>(
    Object.fromEntries(mockNetworkRoutes.map((route) => [route.id, route.latencyMs]))
  );
  const selected = mockNetworkRoutes.find((item) => item.id === selectedId) || mockNetworkRoutes[0];

  return (
    <PageShell
      title={t("pages.network.title")}
      subtitle={t("pages.network.subtitle")}
      notice={notice}
      action={<ToolbarButton icon={<Plus size={13} />} variant="primary" onClick={() => show("info", t("feedback.routeCreate"))}>{t("actions.newRoute")}</ToolbarButton>}
    >
      <div className="grid grid-cols-[minmax(260px,0.85fr)_minmax(0,1.35fr)] gap-4">
        <Panel title={t("pages.network.routesTitle")} icon={<Network size={15} />}>
          <div className="flex flex-col gap-2">
            {mockNetworkRoutes.map((route) => (
              <ListButton
                key={route.id}
                active={route.id === selected.id}
                title={route.name}
                subtitle={`${t(`networkKind.${route.kind}`)} / ${route.target}`}
                right={<StatusPill tone={route.status} />}
                onClick={() => setSelectedId(route.id)}
              />
            ))}
          </div>
        </Panel>
        <Panel title={t("pages.network.testTitle")} icon={<Globe size={15} />}>
          <DetailRow label={t("fields.kind")} value={t(`networkKind.${selected.kind}`)} />
          <DetailRow label={t("fields.target")} value={<span className="font-mono break-all">{selected.target}</span>} />
          <DetailRow label={t("fields.latency")} value={latencies[selected.id] ? `${latencies[selected.id]} ms` : t("common.notTested")} />
          <div className="mt-4 flex flex-wrap gap-2">
            <ToolbarButton
              icon={<TestTube2 size={13} />}
              variant="primary"
              onClick={() => {
                const nextLatency = selected.kind === "direct" ? 118 : 236;
                setLatencies((current) => ({ ...current, [selected.id]: nextLatency }));
                show("success", t("feedback.networkTested", { name: selected.name, ms: nextLatency }));
              }}
            >
              {t("actions.testConnectivity")}
            </ToolbarButton>
          </div>
        </Panel>
      </div>
    </PageShell>
  );
}

export function ConfigDiffPage() {
  const { t } = useTranslation();
  const { notice, show } = useNotice();
  const [stage, setStage] = useState<"draft" | "backup" | "diff" | "ready">("diff");

  return (
    <PageShell
      title={t("pages.configDiff.title")}
      subtitle={t("pages.configDiff.subtitle")}
      notice={notice}
      action={<ToolbarButton icon={<Save size={13} />} variant="primary" onClick={() => show("warning", t("feedback.writeBlocked"))}>{t("actions.saveSafely")}</ToolbarButton>}
    >
      <div className="grid grid-cols-[minmax(260px,0.82fr)_minmax(0,1.42fr)] gap-4">
        <Panel title={t("pages.configDiff.flowTitle")} icon={<ShieldCheck size={15} />}>
          {(["draft", "backup", "diff", "ready"] as const).map((item, index) => (
            <button
              key={item}
              className={`mb-2 flex w-full items-center gap-3 rounded-md border px-3 py-2 text-left ${
                stage === item ? "border-[#BBD7E8] bg-[#D7E8F2]/80" : "border-white/60 bg-white/35"
              }`}
              onClick={() => setStage(item)}
            >
              <span className="flex h-6 w-6 shrink-0 items-center justify-center rounded-full bg-white/70 text-[11px] font-semibold text-ink-700">
                {index + 1}
              </span>
              <span className="min-w-0 flex-1">
                <span className="block text-[13px] font-medium text-ink-800">{t(`diffFlow.${item}.title`)}</span>
                <span className="block truncate text-[11px] text-ink-500">{t(`diffFlow.${item}.desc`)}</span>
              </span>
            </button>
          ))}
        </Panel>
        <Panel title={t("pages.configDiff.diffTitle")} icon={<GitCompare size={15} />}>
          <div className="rounded-md bg-[#14171A] p-4 font-mono text-[12px] leading-[1.7] text-white/88 shadow-inner">
            {mockDiffLines.map((line) => (
              <DiffLine key={line.id} line={line} />
            ))}
          </div>
          <div className="mt-4 flex flex-wrap gap-2">
            <ToolbarButton icon={<Copy size={13} />} onClick={() => show("success", t("feedback.diffCopied"))}>{t("actions.copyDiff")}</ToolbarButton>
            <ToolbarButton icon={<RotateCcw size={13} />} onClick={() => show("warning", t("feedback.rollbackConfirm"))}>{t("actions.prepareRollback")}</ToolbarButton>
          </div>
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
