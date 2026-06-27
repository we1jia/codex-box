# ADR 0005 · BYOK 模型下拉与多 provider 路由

> 状态：Accepted
> 日期：2026-06-25
> 决策者：项目组
> 替代：ADR-0004（Superseded）

## 背景

Codex Box 在 v0.2 阶段一度按"复现 OpenCodex 的 Web 中间层"（gateway / 移动访问 / 远程 Web shell）来组织产品和代码。后续核对发现：

- `docs/references/opencodex-technical-notes.md` 拆解时引用的源码是 `RyensX/OpenCodex`（远程 Web 中间层），而用户本地与社区真正在用的参考项目是 [`AITabby/opencodex`](https://github.com/AITabby/opencodex)（unlocks Codex Desktop for third-party APIs / BYOK 模型下拉）。
- AITabby/opencodex 的真实做法不是接管 Codex Desktop 入口层，而是**改 `~/.codex/config.toml` 配合 `~/.opencodex/providers.json` 与 `~/.opencodex/custom_model_catalog.json`**，在 Codex App 自己的模型选择链路里把官方订阅、OpenAI 官方 API、OpenAI-compatible 第三方、国产模型、本地/中间层 gateway 全部进同一个 picker。
- 用户的真实诉求是 **BYOK 模型下拉**：让 Codex App 里能同时看到并切换订阅、国产、第三方 API 三类模型，切换时整组 `provider + model + reasoning` 一起切，而不是跑一个独立网关当"控制台替代品"。

v0.2 整条主线（PRD、架构、数据模型、ADR-0004、opencodex.rs 进程 launcher、Gateway / MobileAccess / CodexRuntime 三个 page）都是基于错的项目定性，已经全部归档为 `*.archived-20260625.md`，本 ADR 给出 v0.3 真正要做的事。

## 决策

Codex Box v0.3 采用"**通过安全读写 `~/.codex/config.toml` 实现 BYOK 模型下拉**"策略，吸收 AITabby/opencodex 的公开做法但保持独立代码路径和自有 UI：

### 1. 目标

让 Codex App 的模型下拉里能出现并切换以下来源：

- 官方订阅（`codex-subscription`，只做状态识别，不复用 token）
- OpenAI 官方 API（`openai-official-api`）
- OpenAI-compatible 第三方 API（OpenRouter、DeepSeek、Moonshot、智谱等）
- 国产模型 API（同上 OpenAI-compatible 通道）
- 本地 / 中间层 gateway（Codex Box 自有 runtime 或用户显式配置的 endpoint）

切换的不是裸 `model` 字段，而是整组 `model_provider + model + reasoning + sandbox + approval + network + mcp_refs`，即切换的是 `CodexProfile`。

### 2. 数据源（v0.3 起唯一可信源）

| 用途 | 路径 | 备注 |
|---|---|---|
| Codex App 主配置 | `~/.codex/config.toml` | 唯一外部源；Codex Box 写它就触发 Codex App 重读 |
| Codex Box 自有配置 | `~/.codex/codex-box/config.json` | UI 偏好、备份保留、日志策略等 |
| Provider 路由 | `~/.codex/codex-box/providers.json` | Codex Box 主目录；记录 provider name → base_url / wire_api / env_key |
| 自定义模型目录 | `~/.codex/codex-box/custom_model_catalog.json` | 用户在 Codex Box 里"加模型"最终落在这里；拼装进 `~/.codex/config.toml` 的 `model_catalog_json` |

### 3. 写 `~/.codex/config.toml` 的硬规则（保留 v0.2 既有约束）

- 写入前**必须**先 backup（`~/.codex/codex-box/backups/{ts}-{hash}.toml`）
- 写入前**必须**展示 diff 给用户确认
- 写入必须 atomic（`tmp` → `rename`）
- 写入失败**必须**能 rollback 到最近一次 backup
- 并发写入校验 `content_hash`
- secret 字段（API key / token）**永远不写日志、不落盘**；config 里只允许 `${ENV_VAR}` 引用

### 4. 严格不做（红线，与 v0.2 保持一致）

- ❌ 抓取、复用、转发官方账号 token
- ❌ 绕过 OpenAI 官方登录
- ❌ 规避 rate limit / 账号配额限制
- ❌ 默认 patch 或解包 `~/.codex/` 之外的 Codex Desktop 内部文件（`app.asar`、renderer、IPC）
- ❌ 接管系统全局代理
- ❌ 团队同步 SaaS / 上传任何用户配置
- ❌ 复制 AITabby/opencodex 的源码、UI、长段文案

### 5. 与 AITabby/opencodex 的关系

- Codex Box **不**直接 spawn AITabby/opencodex 进程（不再有 `opencodex.rs` 启停外部 Node runner 的逻辑）。
- Codex Box **不复用** `~/.opencodex/` 作为主目录；该目录只作为只读扫描和导入来源，导入目标是 `~/.codex/codex-box/`。
- 实现保持完全独立：Codex Box 走 Tauri + Rust + React，自有写入闭环、自有 UI、自有 license（项目自有，非 AGPL）。
- 详细事实见 [`docs/references/aitabby-opencodex.md`](../references/aitabby-opencodex.md)。

### 6. M2.5 重新定义为底座

- 旧定义："OpenCodex 能力复现底座 — gateway / auth / runtime locator / log / health"
- 新定义：**"BYOK 模型目录与多 provider 路由底座"** —— 包括 `ModelCatalogEntry` 数据模型、`ProviderRoute` 数据模型、`CodexConfigChange` 写入操作、`~/.codex/codex-box/` JSON 读写、第三方配置只读导入、Codex Desktop 安装检测（只读）。

## 影响

### 正面

- 产品目标对齐到用户截图里要的真实诉求：BYOK 模型下拉。
- 写入闭环（v0.2 已经实现）继续复用，只换"写什么"和"写到哪里"。
- 与 AITabby/opencodex CLI 工具互不冲突，可通过只读扫描和导入共存。

### 成本

- v0.2 整条主线（PRD / 架构 / 数据模型 / ADR-0004 / opencodex.rs / Gateway+MobileAccess+CodexRuntime 三个 page）需要按本 ADR 重新实现或重命名。
- `c9321f2 feat: 接入配置写入与 OpenCodex 运行控制` 中按错源写的代码需要改写。

### 风险

- AITabby/opencodex 自身 schema 可能升级（providers.json / custom_model_catalog.json 字段变化）；Codex Box 只在导入预览/导入写入到自有目录时保留未知字段。
- 国产模型 API 鉴权字段差异较大（部分用 `auth` header、部分用 query 参数），需要在 `ProviderRoute.http_headers` 里暴露。

## 后续动作

1. ✅ `PRD.md`、`docs/architecture/v0.2.md`、`docs/data-model/v0.2.md`、`docs/decisions/0004-*.md`、`docs/references/opencodex-technical-notes.md` 已全部归档为 `*.archived-20260625.md`。
2. ✅ `docs/architecture/v0.3-BYOK.md`、`docs/data-model/v0.3-BYOK.md`、`docs/references/aitabby-opencodex.md` 已新建。
3. ⏳ `src-tauri/src/commands/opencodex.rs` 重写为 BYOK 模型目录读写（保留文件名以兼容 Tauri command 注册；命令名改为 `model_catalog_*`）。
4. ⏳ `src/pages/WorkspacePages.tsx` 中 `GatewayPage` / `MobileAccessPage` / `CodexRuntimePage` 重写为 `ModelsPage` / `ProviderRoutesPage` / `CodexRuntimePage`，对应 `pages.models.*` / `pages.providerRoutes.*` / `pages.codexRuntime.*`。
5. ⏳ `src/locales/{en,zh}.json` 同步重写 i18n key。
6. ⏳ 跑 `pnpm build` 验证前端构建通过。
