import type {
  DiagnosticGroupView,
  DiffLineView,
  GatewayPresetView,
  McpServerView,
  NetworkRouteView,
  ProfileView,
  ProviderView,
  SettingsSectionView,
} from "@/lib/types";

export const mockProfiles: ProfileView[] = [
  {
    id: "official-codex",
    name: "official-codex",
    model: "gpt-5-codex",
    providerId: "codex-subscription",
    sandbox: "workspace-write",
    approval: "on-request",
    network: "direct",
    mcpRefs: ["filesystem", "git"],
    status: "ok",
    isActive: true,
  },
  {
    id: "openai-api",
    name: "openai-api",
    model: "gpt-5.1",
    providerId: "openai-official-api",
    sandbox: "workspace-write",
    approval: "on-failure",
    network: "direct",
    mcpRefs: ["filesystem"],
    status: "ok",
  },
  {
    id: "openrouter-dev",
    name: "openrouter-dev",
    model: "openai/gpt-5-mini",
    providerId: "openrouter",
    sandbox: "read-only",
    approval: "on-request",
    network: "http-proxy",
    mcpRefs: ["filesystem", "openaiDeveloperDocs"],
    status: "warn",
  },
  {
    id: "local-gateway",
    name: "local-gateway",
    model: "claude-sonnet",
    providerId: "local-gateway",
    sandbox: "workspace-write",
    approval: "never",
    network: "direct",
    mcpRefs: ["git"],
    status: "idle",
  },
];

export const mockProviders: ProviderView[] = [
  {
    id: "codex-subscription",
    name: "Codex Subscription",
    kind: "subscription",
    baseUrl: "official desktop channel",
    wireApi: "custom",
    envKey: "managed by official login",
    status: "ok",
    models: ["gpt-5-codex"],
  },
  {
    id: "openai-official-api",
    name: "OpenAI Official API",
    kind: "official_api",
    baseUrl: "https://api.openai.com/v1",
    wireApi: "responses",
    envKey: "OPENAI_API_KEY",
    status: "ok",
    models: ["gpt-5.1", "gpt-5-mini"],
  },
  {
    id: "openrouter",
    name: "OpenRouter",
    kind: "compatible_api",
    baseUrl: "https://openrouter.ai/api/v1",
    wireApi: "chat",
    envKey: "OPENROUTER_API_KEY",
    status: "warn",
    models: ["openai/gpt-5-mini", "anthropic/claude-sonnet"],
  },
  {
    id: "local-gateway",
    name: "Local Gateway",
    kind: "local_gateway",
    baseUrl: "http://127.0.0.1:8080/v1",
    wireApi: "chat",
    envKey: "CODEX_BOX_GATEWAY_KEY",
    status: "idle",
    models: ["claude-sonnet", "deepseek-chat"],
  },
];

export const mockGateways: GatewayPresetView[] = [
  {
    id: "local-gateway",
    name: "Local Gateway",
    kind: "local",
    host: "127.0.0.1",
    port: 8080,
    status: "idle",
    logPath: "~/.codex/codex-box/logs/gateway.log",
    healthPath: "/api/health",
    adapter: "openai-compatible-chat",
  },
  {
    id: "opencodex",
    name: "OpenCodex",
    kind: "opencodex",
    host: "127.0.0.1",
    port: 3737,
    status: "warn",
    logPath: "~/Library/Application Support/OpenCodex/logs/gateway.log",
    healthPath: "/api/health",
    adapter: "desktop-web-middleware",
  },
  {
    id: "cli-proxy-api",
    name: "CLIProxyAPI",
    kind: "cli_proxy_api",
    host: "127.0.0.1",
    port: 4141,
    status: "idle",
    logPath: "~/.codex/codex-box/logs/cliproxyapi.log",
    healthPath: "/health",
    adapter: "responses-to-chat",
  },
];

export const mockMcpServers: McpServerView[] = [
  {
    id: "filesystem",
    name: "filesystem",
    transport: "stdio",
    commandOrUrl: "npx -y @modelcontextprotocol/server-filesystem",
    enabled: true,
    status: "ok",
    envRefs: ["ROOT"],
  },
  {
    id: "git",
    name: "git",
    transport: "stdio",
    commandOrUrl: "uvx mcp-server-git",
    enabled: true,
    status: "idle",
    envRefs: [],
  },
  {
    id: "openaiDeveloperDocs",
    name: "openaiDeveloperDocs",
    transport: "http",
    commandOrUrl: "https://developers.openai.com/mcp",
    enabled: true,
    status: "ok",
    envRefs: [],
  },
  {
    id: "playwright",
    name: "playwright",
    transport: "stdio",
    commandOrUrl: "npx -y @playwright/mcp",
    enabled: false,
    status: "warn",
    envRefs: ["BROWSER_PROFILE"],
  },
];

export const mockNetworkRoutes: NetworkRouteView[] = [
  { id: "direct", name: "direct", kind: "direct", target: "no proxy", status: "ok", latencyMs: 126 },
  { id: "http-proxy", name: "http-proxy", kind: "http_proxy", target: "http://127.0.0.1:7890", status: "idle", latencyMs: null },
  { id: "socks5", name: "socks5", kind: "socks_proxy", target: "socks5://127.0.0.1:7891", status: "idle", latencyMs: null },
  { id: "clash-profile", name: "clash-profile", kind: "clash_profile", target: "~/.config/mihomo/config.yaml", status: "warn", latencyMs: null },
];

export const mockDiffLines: DiffLineView[] = [
  { id: "1", kind: "context", content: "[model_providers.local_gateway]" },
  { id: "2", kind: "insert", content: "base_url = \"http://127.0.0.1:8080/v1\"" },
  { id: "3", kind: "insert", content: "wire_api = \"chat\"" },
  { id: "4", kind: "insert", content: "api_key_env = \"CODEX_BOX_GATEWAY_KEY\"" },
  { id: "5", kind: "context", content: "" },
  { id: "6", kind: "change", content: "[profile.dev] model_provider: openai -> local_gateway" },
];

export const mockDiagnostics: DiagnosticGroupView[] = [
  {
    id: "config",
    titleKey: "diagnostics.groups.config",
    items: [
      { id: "syntax", labelKey: "diagnostics.items.syntax", detail: "~/.codex/config.toml", status: "ok", latencyMs: 12 },
      { id: "profileRefs", labelKey: "diagnostics.items.profileRefs", detail: "4 profiles / 4 providers", status: "ok" },
    ],
  },
  {
    id: "provider",
    titleKey: "diagnostics.groups.provider",
    items: [
      { id: "providerUrl", labelKey: "diagnostics.items.providerUrl", detail: "https://api.openai.com/v1", status: "ok", latencyMs: 412 },
      { id: "authEnv", labelKey: "diagnostics.items.authEnv", detail: "OPENAI_API_KEY", status: "warn" },
    ],
  },
  {
    id: "gateway",
    titleKey: "diagnostics.groups.gateway",
    items: [
      { id: "gatewayHealth", labelKey: "diagnostics.items.gatewayHealth", detail: "127.0.0.1:8080/api/health", status: "idle" },
      { id: "port", labelKey: "diagnostics.items.port", detail: "8080", status: "idle" },
    ],
  },
  {
    id: "mcp",
    titleKey: "diagnostics.groups.mcp",
    items: [
      { id: "mcpCommand", labelKey: "diagnostics.items.mcpCommand", detail: "npx / uvx", status: "ok" },
      { id: "mcpEnv", labelKey: "diagnostics.items.mcpEnv", detail: "env refs only", status: "ok" },
    ],
  },
  {
    id: "backup",
    titleKey: "diagnostics.groups.backup",
    items: [
      { id: "backupDir", labelKey: "diagnostics.items.backupDir", detail: "~/.codex/codex-box/backups", status: "ok" },
      { id: "atomicWrite", labelKey: "diagnostics.items.atomicWrite", detail: "tmp -> rename", status: "ok" },
    ],
  },
];

export const mockSettingsSections: SettingsSectionView[] = [
  {
    id: "general",
    titleKey: "settings.sections.general",
    optionKeys: ["language", "theme", "startupCheck"],
  },
  {
    id: "security",
    titleKey: "settings.sections.security",
    optionKeys: ["requireDiff", "maskSecrets", "confirmDanger"],
  },
  {
    id: "backup",
    titleKey: "settings.sections.backup",
    optionKeys: ["backupFirst", "retention", "rollback"],
  },
  {
    id: "logs",
    titleKey: "settings.sections.logs",
    optionKeys: ["redaction", "maxSize", "exportReport"],
  },
  {
    id: "experiments",
    titleKey: "settings.sections.experiments",
    optionKeys: ["gatewayPresets", "pluginDirs", "desktopScan"],
  },
];
