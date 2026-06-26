# Provider Routing Repair Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 修复 Codex Box Runtime/Provider 链路不生效的问题，并产出项目模块冲突分析。

**Architecture:** 先让 Runtime 页展示真实生效链路，再收敛启用逻辑。后端新增只读链路诊断命令，解析 `~/.codex/config.toml`、`~/.opencodex/custom_model_catalog.json`、`~/.opencodex/providers.json` 和代理状态，指出断链原因；同时阻止 port=0 写入。

**Tech Stack:** Tauri v2, Rust, React, TypeScript, Vite, TOML/JSON parsing.

---

### Task 1: 阻止 Runtime 写入 port=0

**Files:**
- Modify: `/Users/liuweijia/Desktop/AI/Codex Box/src-tauri/src/commands/proxy.rs`
- Modify: `/Users/liuweijia/Desktop/AI/Codex Box/src/pages/WorkspacePages.tsx`

- [ ] **Step 1: Write failing Rust test**

Add a test in `src-tauri/src/commands/proxy.rs` proving `rewrite_base_urls` rejects port `0`.

- [ ] **Step 2: Run test to verify fail**

Run: `cargo test --lib proxy::commands::tests::rewrite_base_urls_rejects_zero_port`
Expected: fail because port 0 is currently accepted.

- [ ] **Step 3: Implement validation**

Add `validate_proxy_port(port)` and call it in `proxy_inject_base_url_preview`, `rewrite_base_urls`, and restore callers as needed.

- [ ] **Step 4: Fix frontend fallback**

Replace `proxy?.port ?? 1455` with a helper that treats `0` as invalid and falls back to `1455`.

- [ ] **Step 5: Run targeted tests**

Run: `cargo test --lib proxy::commands`

### Task 2: Add current effective chain diagnostics

**Files:**
- Create: `/Users/liuweijia/Desktop/AI/Codex Box/src-tauri/src/commands/effective_routing.rs`
- Modify: `/Users/liuweijia/Desktop/AI/Codex Box/src-tauri/src/commands/mod.rs`
- Modify: `/Users/liuweijia/Desktop/AI/Codex Box/src-tauri/src/lib.rs`
- Modify: `/Users/liuweijia/Desktop/AI/Codex Box/src/lib/types.ts`
- Modify: `/Users/liuweijia/Desktop/AI/Codex Box/src/pages/WorkspacePages.tsx`

- [ ] **Step 1: Write Rust tests for diagnosis**

Test cases:
1. Missing `model_catalog_json` reports catalog not configured.
2. `base_url` using `127.0.0.1:0` reports invalid port.
3. Catalog model resolves `backend_provider` to upstream provider.

- [ ] **Step 2: Implement command**

Create `effective_routing_status` Tauri command returning current model/provider/base_url/catalog/backend provider/upstream URL/issues.

- [ ] **Step 3: Add TS types and UI panel**

Add `EffectiveRoutingStatus` type and render a Runtime panel named “当前生效链路”.

- [ ] **Step 4: Verify UI build**

Run: `pnpm build`.

### Task 3: Module conflict analysis document

**Files:**
- Create: `/Users/liuweijia/Desktop/AI/Codex Box/docs/architecture/v0.3.3-module-conflict-audit.md`

- [ ] **Step 1: Analyze modules**

Cover Models, Provider Routes, Providers, Profiles, Codex Runtime, Settings/Diagnostics.

- [ ] **Step 2: Classify**

Classify each as Keep / Rename / Merge / Deprecate / Rewrite.

- [ ] **Step 3: Add concrete next actions**

Tie recommendations to files and user-facing behavior.

### Task 4: Verification

- [ ] Run `cargo test --lib`.
- [ ] Run `pnpm build`.
- [ ] Re-run focused commands that inspect the new diagnostic behavior where possible.
