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
  assert.ok(runtime.primary.includes("启动 / 停止 / 重启代理"));
  assert.ok(runtime.primary.includes("注入 / 还原 base_url"));
  assert.ok(runtime.advanced.includes("日志"));
  assert.ok(runtime.advanced.includes("会话管理"));
  assert.ok(!runtime.primary.includes("模型配置"));
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
  const runtimeSource = source.slice(
    source.indexOf("export function CodexRuntimePage"),
    source.indexOf("function format_uptime"),
  );
  assert.equal(runtimeSource.includes("official-codex"), false);
  assert.equal(runtimeSource.includes("openrouter-dev"), false);
  assert.equal(runtimeSource.includes("deepseek-test"), false);
  assert.equal(runtimeSource.includes("pages.codexBoxRuntime.logs.items.started"), false);
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

test("runtime page uses the nested i18n namespace for runtime labels", () => {
  const source = readFileSync(new URL("../src/pages/WorkspacePages.tsx", import.meta.url), "utf8");
  assert.equal(/t\((["'`])codexBoxRuntime\./.test(source), false);
  assert.equal(source.includes('t("pages.codexBoxRuntime.'), true);
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
