# AITabby/opencodex 技术事实

> 来源：[`AITabby/opencodex`](https://github.com/AITabby/opencodex) (v0.2.2, HEAD `2d8065a`)
> 本地检出：`/Users/liuweijia/Desktop/AI/opencodex`（与 `/Users/liuweijia/Desktop/AI/OpenCodex` 是同一仓库的另一个 clone）
> 拆解时间：2026-06-25
> 替代：[`opencodex-technical-notes.archived-ryensx-20260625.md`](./opencodex-technical-notes.archived-ryensx-20260625.md)（旧版本基于 `RyensX/OpenCodex` 源码包，与当前实际项目不一致）

版权边界：本文只记录本地集成所需的接口事实、运行方式和配置约定，不复制 AITabby/opencodex 的源码、UI 实现或长段文案。Codex Box 的实现应保持独立代码路径，只通过公开文件约定（`~/.opencodex/*.json`、`~/.codex/config.toml` schema）进行兼容。

---

## 1. 项目定位

AITabby/opencodex 不是一个独立的"Web 中间层"，而是一个 **"unlocks Codex Desktop for third-party APIs"** 的本地工具。

它的核心做法：

- 自动改 `~/.codex/config.toml`，在 `[model_providers.*]` 段加入用户配置的第三方 provider。
- 维护 `~/.opencodex/providers.json`（provider 配置）和 `~/.opencodex/custom_model_catalog.json`（自定义模型目录）。
- 提供一个统一的本地 endpoint（实现细节由 AITabby 提供），让 Codex App 看到合并后的模型列表。
- **不** patch Codex Desktop 内部文件，**不** 抓 token，**不** 接管 IPC。

这与 Codex Box v0.3 的关系：

- **要复现**：在 Codex App 的模型下拉里同时出现官方订阅、官方 API、第三方 OpenAI-compatible（含国产）、本地 gateway 几类模型；切换时整组 `provider + model + reasoning` 一起切。
- **要保持独立实现**：Codex Box 不 spawn AITabby/opencodex 进程，不复制其源码、UI、长段文案，自有 license。
- **要复用约定**：`~/.opencodex/providers.json` 和 `~/.opencodex/custom_model_catalog.json` 这两个文件路径与字段约定保持一致，让用户可以在 AITabby/opencodex CLI 和 Codex Box 间无缝切换。

---

## 2. 文件约定（公开事实）

### 2.1 `~/.codex/config.toml`（Codex App 唯一主配置）

Codex Box 写入的"目标态"应符合 Codex App 的 schema：

- 第三方 provider 走 `[model_providers.<name>]` 表，`base_url` + `wire_api` + `env_key` + 可选 `http_headers`。
- 模型选择走 `[profile.<name>]` 表的 `model` + `model_provider` 字段。
- 切换模型即"切到对应 profile"或"修改 active profile 的 `model` 字段"，但 BYOK 场景下更稳的做法是切到对应 profile。

### 2.2 `~/.opencodex/providers.json`（AITabby 约定）

来源：[`SESSION_PROGRESS.md` line 61](https://github.com/AITabby/opencodex/blob/main/SESSION_PROGRESS.md)

记录 provider 配置：name / base_url / wire_api / api_key 引用方式 / http_headers 等。

Codex Box 读取这份文件时必须：

- 未知字段保留并回写（不删字段）。
- 不解析或落盘明文 secret；`api_key` 字段只接受 `${ENV_VAR}` 引用，不接受 inline value。
- 文件不存在时按"空配置"处理，不要报错。

### 2.3 `~/.opencodex/custom_model_catalog.json`（AITabby 约定）

来源：同上。

记录用户自定义的模型条目：`slug` / `model` / `display_name` / `provider` / `backend_model` / `backend_provider` / `visibility` 等。

Codex Box 读取这份文件时同上：未知字段保留、不解析 secret、空配置按空处理。

---

## 3. 与 Codex App 的协作方式

Codex App 在启动或"重读配置"时读 `~/.codex/config.toml`。AITabby/opencodex 的关键不是让 Codex App 自己合并多个 provider，而是把 Codex App 指向一个本地 OpenAI-compatible endpoint：

- 顶层 `openai_base_url = "http://127.0.0.1:8765/v1"`
- 顶层 `model_catalog_json = "~/.opencodex/custom_model_catalog.json"`
- `[model_providers.opencodex]` 的 `base_url = "http://127.0.0.1:8765/v1"`、`wire_api = "responses"`

之后 Codex App 拉 `/v1/models` 时看到的是本地代理合并后的模型列表；发送 `/v1/responses` 或 `/v1/chat/completions` 时，本地代理再按 catalog 的 `slug/backend_model/backend_provider` 和 `providers.json` 路由到真实 upstream。

源码补充事实（2026-06-27 复核）：

- OpenCodex 会读取 `~/.codex/models_cache.json`，把 Codex 原生 GPT 模型合并进 `custom_model_catalog.json`，这些 native model 使用 `provider = "openai"` 且不写 `backend_provider`。
- OpenCodex 的 `/v1/models` 返回 catalog 里的裸 `slug`，不是 `provider/model` 命名空间。
- 对第三方模型，OpenCodex 的 `translator.ts` 提供 Responses ↔ Chat 的兼容层，处理 `<think>` 过滤、reasoning、tool calls、conversation history 等状态。
- 因此 Codex Box 的正确方向不是把 Responses provider 统一降级成 Chat，而是：上游支持 Responses 时优先 passthrough/minimal normalize；上游只支持 Chat 时才走 translator。Codex Box 的 Rust proxy 已补齐 Chat fallback 下的 reasoning/tool-call/history 核心状态机，OpenCodex 的 voice / vision bridge / computer-use 专项逻辑不纳入 BYOK 主线。

CCSwitchMulti / `BigStrongSun/cc-switch` 源码复核（2026-06-27，本地检出：`/Users/liuweijia/Desktop/AI/ccswitchmulti`）：

- Codex MultiRouter 的关键不是把多个真实 provider 直接写进 Codex App，而是创建一个本地 router provider，`settingsConfig.modelCatalog` 决定 Codex 下拉框展示，`settingsConfig.codexRouting.routes` 决定请求按 model id 分流到哪个真实 provider。
- `codexRouting.routes[].match.models/prefixes` 负责匹配 picker model id，`targetProviderId` 指向真实 provider，`upstream.apiFormat` 标记真实上游是 `openai_responses` 还是 `openai_chat`，`upstream.modelMap` 负责 picker model id 到上游 model id 的改写。
- takeover 写入 Codex live config 时，MultiRouter 需要稳定的 router provider id。CCSwitchMulti 使用 `codex_model_router_v2` 作为运行桶，并关闭 WebSocket 直连上游能力；请求进入本地代理后才做 Responses passthrough 或 Responses→Chat translator。Codex Box 新写入默认对齐该稳定桶，旧 `codex_local_access` 仅作为历史兼容来源识别。
- Codex Desktop renderer 可能继续按 Statsig `available_models/use_hidden_models` 白名单过滤模型。CCSwitchMulti 通过 CDP 注入补丁修 `available_models`、`model/list`、`list-models-for-host` 和 renderer 状态；Codex Box 已实现用户显式点击的 `codex_desktop_picker_unlock`,只在 Codex Desktop 已经开放 CDP 时注入当前 renderer,并覆盖 Statsig、响应 JSON、MCP `model/list`、app-server `list-models-for-host` 与 React auth context 路径,不自动重启、不改 `app.asar`。
- Codex Box 已按该 schema 增加 `ProviderRoute.codexRouting` 读写保留，并在 Rust proxy resolver 中实现 catalog 命中 MultiRouter provider 后的二次路由。native GPT/Codex catalog entry 会优先匹配自动生成的 `openai-official` route(`auth.source = "managed_codex_oauth"`),只有无 route 时才落到 native fallback。

所以 AITabby/opencodex 与 Codex Box 的接入链路是：

```
Codex Box UI
   ↓ 写入
~/.codex/codex-box/{providers.json, custom_model_catalog.json}   (Codex Box 内部维护)
   ↓ 注入
~/.codex/config.toml                                       (Codex App 读取 127.0.0.1 base_url + catalog path)
   ↓ /v1/models
Codex Box 本地代理返回合并模型
   ↓ /v1/responses or /v1/chat/completions
Codex Box 本地代理按 model id 路由到真实 provider
```

**Codex Box 不需要 spawn 外部 AITabby/opencodex 进程、不需要 patch Codex App、不需要拦截 IPC**；但要复现这个 BYOK 效果，Codex Box 自身必须有一个仅监听 `127.0.0.1` 的本地代理 runtime。

---

## 4. 对 Codex Box v0.3 的启发

### 必须复用

1. `~/.opencodex/` 两个 JSON 的文件路径与字段语义。
2. "切换模型 = 切 profile"的语义（而不是只改裸 `model`）。
3. "未知字段保留"的 schema 兼容策略。
4. provider 列表的混合展示（订阅 / 官方 / 第三方 / 本地全部进同一个 picker）。

### 严格不复制

1. 不 spawn 外部 AITabby/opencodex 进程。
2. 不复制其 Tauri / Electron UI 实现。
3. 不实现其"语音伴侣"等周边功能（与 BYOK 无关）。
4. 不重命名或重定义 `~/.opencodex/` 下两个 JSON 的字段。

### 不做的事

1. 不抓官方账号 token（与 AITabby 行为一致）。
2. 不接管系统代理。
3. 不默认 patch Codex Desktop 内部文件。

---

## 5. 落地边界

- Codex Box 写 `~/.codex/config.toml` 时遵循项目既有写入闭环：backup → diff → confirm → atomic write → rollback。
- Codex Box 写 `~/.opencodex/*.json` 时走同源 toml crate + serde 风格的 JSON 写入，但版本字段用 `schema_version: 1`，未知字段原样保留。
- Codex App 安装检测走 `codex_runtime/locator` 既有实现（v0.2 已就绪），只读。
- 不实现 AITabby/opencodex 的 `Computer Use engine` / `Vision Bridge` / `Voice Companion`，这些与 BYOK 无关。
