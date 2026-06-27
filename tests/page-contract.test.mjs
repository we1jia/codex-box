import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";

import pageContract from "../src/lib/page-contract.json" with { type: "json" };
import enLocale from "../src/locales/en.json" with { type: "json" };
import zhLocale from "../src/locales/zh.json" with { type: "json" };

function getByPath(object, path) {
  return path.split(".").reduce((current, segment) => current?.[segment], object);
}

test("models page only contains model configuration responsibilities", () => {
  const models = pageContract.pages.models.zh;
  assert.deepEqual(models.primary, [
    "填写模型名称",
    "填写接口地址",
    "填写 API Key",
    "保存模型",
    "控制哪些模型显示在 Codex 下拉框",
  ]);
  assert.ok(!models.advanced.includes("日志"));
  assert.ok(!models.advanced.includes("会话"));
});

test("runtime page groups runtime-only operations", () => {
  const runtime = pageContract.pages.codex_runtime.zh;
  assert.ok(runtime.primary.includes("启动本地 127.0.0.1 连接"));
  assert.ok(runtime.primary.includes("生成发布变更预览"));
  assert.ok(runtime.primary.includes("把模型目录注入 Codex App 下拉框"));
  assert.ok(runtime.primary.includes("检查模型到 API 服务的链路"));
  assert.ok(runtime.advanced.includes("日志"));
  assert.ok(runtime.advanced.includes("会话管理"));
  assert.ok(!runtime.primary.includes("模型配置"));
});

test("model router page owns the full publish chain without editing sources", () => {
  const router = pageContract.pages.model_router.zh;
  assert.ok(router.primary.includes("汇总 API 服务来源"));
  assert.ok(router.primary.includes("汇总下拉框模型"));
  assert.ok(router.primary.includes("生成发布变更预览"));
  assert.ok(router.primary.includes("发布到 Codex App"));
  assert.ok(router.primary.includes("检查模型到 API 服务的链路"));
  assert.ok(router.primary.includes("注入 Codex App 下拉框"));
  assert.ok(router.excluded.includes("模型编辑"));
  assert.ok(router.excluded.includes("API 服务编辑"));
});

test("route diagnostics workspace uses real publish commands", () => {
  const source = readFileSync(new URL("../src/pages/WorkspacePages.tsx", import.meta.url), "utf8");
  const routerSource = source.slice(
    source.indexOf("export function ModelRouterPage"),
    source.indexOf("export function ProviderRoutesPage"),
  );

  assert.equal(routerSource.includes('"opencodex_config_read"'), true);
  assert.equal(routerSource.includes('"effective_routing_status"'), true);
  assert.equal(routerSource.includes('"codex_desktop_integration_status"'), true);
  assert.equal(routerSource.includes('"proxy_start"'), true);
  assert.equal(routerSource.includes('"codex_multirouter_preview"'), true);
  assert.equal(routerSource.includes('"codex_multirouter_apply"'), true);
  assert.equal(routerSource.includes('"proxy_route_test"'), true);
  assert.equal(routerSource.includes('"codex_desktop_picker_unlock"'), true);
  assert.equal(
    routerSource.includes('"codex_desktop_launch_with_debugging_and_unlock"'),
    true,
  );
  assert.equal(routerSource.includes('"simple_model_config_save"'), false);
  assert.equal(routerSource.includes('"provider_route_upsert"'), false);
});

test("dashboard keeps only key status and entry points", () => {
  const dashboard = pageContract.pages.dashboard.zh;
  assert.ok(dashboard.primary.includes("系统状态"));
  assert.ok(dashboard.primary.includes("关键入口"));
  assert.ok(!dashboard.primary.includes("日志"));
  assert.ok(!dashboard.primary.includes("会话"));
});

test("dashboard does not render detailed metric cards", () => {
  const source = readFileSync(new URL("../src/pages/Dashboard.tsx", import.meta.url), "utf8");
  assert.equal(source.includes("<MetricCard"), false);
});

test("runtime page reads logs and sessions from commands instead of placeholders", () => {
  const source = readFileSync(new URL("../src/pages/WorkspacePages.tsx", import.meta.url), "utf8");
  const runtimeSource = source.slice(source.indexOf("export function CodexRuntimePage"));
  assert.equal(runtimeSource.includes("official-codex"), false);
  assert.equal(runtimeSource.includes("openrouter-dev"), false);
  assert.equal(runtimeSource.includes("deepseek-test"), false);
  assert.equal(runtimeSource.includes("pages.codexBoxRuntime.logs.items.started"), false);
  assert.equal(source.includes("mockCodexRuntime"), false);
  assert.equal(runtimeSource.includes('invokeCmd<CodexRuntimeStatus>("codex_runtime_status")'), true);
  assert.equal(runtimeSource.includes('invokeCmd<ProxyRuntimeLogs>("proxy_runtime_logs")'), true);
  assert.equal(runtimeSource.includes('invokeCmd<ProxySessionsView>("proxy_sessions")'), true);
});

test("models page never renders the mock catalog as real dropdown data", () => {
  const pageSource = readFileSync(new URL("../src/pages/WorkspacePages.tsx", import.meta.url), "utf8");
  const apiSource = readFileSync(new URL("../src/lib/api.ts", import.meta.url), "utf8");
  assert.equal(pageSource.includes("mockModelCatalog"), false);
  assert.equal(apiSource.includes("mockModelCatalog"), false);
  assert.equal(pageSource.includes("GPT-5 Codex"), false);
  assert.equal(apiSource.includes("GPT-5 Codex"), false);
});

test("models page does not auto-select the first picker model", () => {
  const source = readFileSync(new URL("../src/pages/WorkspacePages.tsx", import.meta.url), "utf8");
  const modelsSource = source.slice(
    source.indexOf("export function ModelsPage"),
    source.indexOf("function isRouterProvider"),
  );

  assert.equal(modelsSource.includes("const selected = selectedId"), true);
  assert.equal(modelsSource.includes("dropdownCatalog[0]"), false);
  assert.equal(modelsSource.includes("pages.models.selectModelTitle"), true);
});

test("configuration pages do not fall back to mock profiles or providers", () => {
  const source = readFileSync(new URL("../src/pages/WorkspacePages.tsx", import.meta.url), "utf8");
  const profilesSource = source.slice(
    source.indexOf("export function ProfilesPage"),
    source.indexOf("export function ProvidersPage"),
  );
  const providersSource = source.slice(
    source.indexOf("export function ProvidersPage"),
    source.indexOf("export function ModelsPage"),
  );
  const providerRoutesSource = source.slice(
    source.indexOf("export function ProviderRoutesPage"),
    source.indexOf("export function CodexRuntimePage"),
  );

  assert.equal(profilesSource.includes("mockProfiles"), false);
  assert.equal(profilesSource.includes("mockProviders"), false);
  assert.equal(providersSource.includes("mockProviders"), false);
  assert.equal(providerRoutesSource.includes("mockProviderRoutes"), false);
  assert.equal(source.includes("mockProviderRoutes"), false);
});

test("runtime page uses the nested i18n namespace for runtime labels", () => {
  const source = readFileSync(new URL("../src/pages/WorkspacePages.tsx", import.meta.url), "utf8");
  assert.equal(/t\((["'`])codexBoxRuntime\./.test(source), false);
  assert.equal(source.includes('t("pages.codexBoxRuntime.'), true);
});

test("runtime primary connect path previews full publish sync", () => {
  const source = readFileSync(new URL("../src/pages/WorkspacePages.tsx", import.meta.url), "utf8");
  const runtimeSource = source.slice(source.indexOf("export function CodexRuntimePage"));
  const connectSource = runtimeSource.slice(
    runtimeSource.indexOf("const connectCodex"),
    runtimeSource.indexOf("const injectPreviewFn"),
  );
  const previewSource = runtimeSource.slice(
    runtimeSource.indexOf("const previewRuntimeMultirouter"),
    runtimeSource.indexOf("const connectCodex"),
  );

  assert.equal(previewSource.includes('"codex_multirouter_preview"'), true);
  assert.equal(runtimeSource.includes('"codex_multirouter_sync"'), false);
  assert.equal(previewSource.includes('"byok_activation_preview"'), false);
  assert.equal(previewSource.includes("routerProviderId: DEFAULT_MULTIROUTER_PROVIDER_ID"), true);
  assert.equal(connectSource.includes("previewRuntimeMultirouter(proxyPort)"), true);
  assert.equal(source.includes('DEFAULT_MULTIROUTER_PROVIDER_ID = "codex_model_router_v2"'), true);
});

test("runtime exposes dry-run route test before upstream calls", () => {
  const source = readFileSync(new URL("../src/pages/WorkspacePages.tsx", import.meta.url), "utf8");
  const runtimeSource = source.slice(
    source.indexOf("export function CodexRuntimePage"),
    source.indexOf("export function DiagnosticsPage"),
  );

  assert.equal(runtimeSource.includes('"proxy_route_test"'), true);
  assert.equal(runtimeSource.includes("runRouteTest = useCallback(async (performUpstream = false)"), true);
  assert.equal(runtimeSource.includes("performUpstream,"), true);
  assert.equal(runtimeSource.includes("runRouteTest(true)"), true);
  assert.equal(runtimeSource.includes("routeTestResult.steps"), true);
  assert.equal(runtimeSource.includes("routeTestResult.upstreamStatusCode"), true);
});

test("diagnostics exposes explicit picker unlock without auto restart", () => {
  const source = readFileSync(new URL("../src/pages/WorkspacePages.tsx", import.meta.url), "utf8");
  const diagnosticsSource = source.slice(source.indexOf("export function DiagnosticsPage"));
  assert.equal(diagnosticsSource.includes('"codex_desktop_picker_unlock"'), true);
  assert.equal(diagnosticsSource.includes('"codex_desktop_launch_with_debugging_and_unlock"'), true);
  assert.equal(diagnosticsSource.includes("pickerUnlock"), true);
  assert.equal(source.includes("pickerReadiness"), true);
  assert.equal(diagnosticsSource.includes('"proxy_restart"'), false);
  assert.equal(diagnosticsSource.toLowerCase().includes("kill"), false);
});

test("runtime exposes picker unlock as part of the connect flow", () => {
  const source = readFileSync(new URL("../src/pages/WorkspacePages.tsx", import.meta.url), "utf8");
  const runtimeSource = source.slice(
    source.indexOf("export function CodexRuntimePage"),
    source.indexOf("export function DiagnosticsPage"),
  );
  assert.equal(runtimeSource.includes("desktopPicker"), true);
  assert.equal(runtimeSource.includes('"codex_desktop_picker_unlock"'), true);
  assert.equal(
    runtimeSource.includes('"codex_desktop_launch_with_debugging_and_unlock"'),
    true,
  );
  const unlockSource = runtimeSource.slice(
    runtimeSource.indexOf("const unlockRuntimePicker"),
    runtimeSource.indexOf("const injectPreviewFn"),
  );
  assert.equal(unlockSource.includes('"proxy_restart"'), false);
  assert.equal(unlockSource.toLowerCase().includes("kill"), false);
});

test("runtime sync attempts picker injection without launching Codex", () => {
  const source = readFileSync(new URL("../src/pages/WorkspacePages.tsx", import.meta.url), "utf8");
  const runtimeSource = source.slice(
    source.indexOf("export function CodexRuntimePage"),
    source.indexOf("export function DiagnosticsPage"),
  );
  const applySource = runtimeSource.slice(
    runtimeSource.indexOf("const applyRuntimeMultirouterPreview"),
    runtimeSource.indexOf("const unlockRuntimePicker"),
  );

  assert.equal(applySource.includes('"codex_multirouter_apply"'), true);
  assert.equal(applySource.includes('"codex_desktop_picker_unlock"'), true);
  assert.equal(
    applySource.includes('"codex_desktop_launch_with_debugging_and_unlock"'),
    false,
  );
  assert.equal(applySource.includes('"proxy_restart"'), false);
  assert.equal(applySource.toLowerCase().includes("kill"), false);
});

test("runtime exposes OpenCodex import when live API services are missing", () => {
  const source = readFileSync(new URL("../src/pages/WorkspacePages.tsx", import.meta.url), "utf8");
  const runtimeSource = source.slice(
    source.indexOf("export function CodexRuntimePage"),
    source.indexOf("export function DiagnosticsPage"),
  );
  assert.equal(runtimeSource.includes('"config_import_sources_scan"'), true);
  assert.equal(runtimeSource.includes('"opencodex_import_preview"'), true);
  assert.equal(runtimeSource.includes('"opencodex_import_apply"'), true);
  assert.equal(runtimeSource.includes("runtimeImportPreview"), true);
  assert.equal(runtimeSource.includes("missingBackendProviderIds"), true);
  assert.equal(runtimeSource.includes("previewRuntimeMultirouter"), true);
  const importApplySource = runtimeSource.slice(
    runtimeSource.indexOf("const applyRuntimeImport"),
    runtimeSource.indexOf("const applyRuntimeMultirouterPreview"),
  );
  assert.equal(
    importApplySource.indexOf('"opencodex_import_apply"') <
      importApplySource.indexOf("previewRuntimeMultirouter(proxyPort)"),
    true,
  );
});

test("zh runtime copy avoids raw English product labels", () => {
  const runtimeCopy = JSON.stringify(zhLocale.pages.codexBoxRuntime);
  for (const rawLabel of [
    "Endpoint",
    "Healthz URL",
    "Inject",
    "Restore",
    "upstream provider",
    "Wire API",
    "Env 引用",
  ]) {
    assert.equal(runtimeCopy.includes(rawLabel), false, rawLabel);
  }
});

test("runtime i18n literal keys exist in both zh and en resources", () => {
  const source = readFileSync(new URL("../src/pages/WorkspacePages.tsx", import.meta.url), "utf8");
  const keys = [...source.matchAll(/t\("((?:pages\.)codexBoxRuntime\.[^"]+)"/g)].map(
    (match) => match[1],
  );
  assert.ok(keys.length > 0);

  for (const key of new Set(keys)) {
    assert.notEqual(getByPath(zhLocale, key), undefined, `zh missing ${key}`);
    assert.notEqual(getByPath(enLocale, key), undefined, `en missing ${key}`);
  }
});
