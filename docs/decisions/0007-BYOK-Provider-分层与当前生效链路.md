# ADR 0007 · BYOK Provider 分层与当前生效链路

> 状态：Accepted
> 日期：2026-06-26
> 决策者：项目组
> 关联：ADR-0005（BYOK 模型下拉与多 provider 路由）、ADR-0006（BYOK 本地代理 runtime）

## 背景

在 Codex Box 的 BYOK 设计里，`Provider` 一词被用于多个层级：Codex 原生 `model_provider`、`[model_providers.*]`、`~/.codex/codex-box/providers.json`、`custom_model_catalog.json` 中的 `provider/backend_provider`。这些概念如果不拆开，会导致 UI 和写入逻辑混乱：用户无法判断“添加模型来源”“Provider 路由”“工作配置”“Codex Runtime 启用”分别影响哪一层。

用户当前已经存在多个 Codex `Model Provider`，并且可以在 Codex App 中自行选择。这个能力本身不是问题。Codex Box 不应该把所有场景强行固定为 `model_provider = "openai"`，也不应该默认切到 `model_provider = "opencodex"`。Codex Box 应该把多 Provider 的当前生效链路解释清楚，并提供安全编辑能力。

## 决策

Codex Box 后续设计必须把 Provider 明确分为四层。

### 1. Codex 会话归属 Provider

对应 `~/.codex/config.toml` 的顶层字段：

```toml
model_provider = "openai"
```

职责：

- 决定 Codex 当前使用哪个 Codex provider。
- 影响 Codex App 会话列表归属、历史记录按 provider 显示/过滤的行为。
- 是用户可选择的既有能力，不应被 Codex Box 默认覆盖成固定值。

设计要求：

- Codex Box 可以展示和编辑该值，但必须明确提示它影响“会话归属”。
- 默认不强制改成 `openai` 或旧 `opencodex`。
- 用户选择 `openai`、`codex_local_access`、其它自定义 provider 都应被视为合法路径，只要对应配置可解析。旧 `opencodex` 只作为历史残留提示和导入来源，不应作为 Codex Box 新写入目标。

### 2. Codex 请求入口 Provider

对应两种入口形态。

内置 OpenAI provider 的入口覆盖：

```toml
model_provider = "openai"
openai_base_url = "http://127.0.0.1:1455/v1"
```

自定义 provider 的入口：

```toml
model_provider = "codex_local_access"

[model_providers.codex_local_access]
base_url = "http://127.0.0.1:1455/v1"
wire_api = "responses"
```

职责：

- 决定 Codex 请求首先发往哪个 endpoint。
- 可以指向 OpenAI 官方、Codex Box 本地代理、外部兼容 API、或其它本地 gateway。

设计要求：

- 对内置 `openai`，应使用 `openai_base_url`，不要创建 `[model_providers.openai]`。
- 对自定义 provider，应使用 `[model_providers.<id>]`。
- UI 必须明确区分“会话归属 provider”和“请求入口 base_url”。

### 3. 上游 API Provider

对应 `~/.codex/codex-box/providers.json`：

```json
{
  "providers": [
    {
      "name": "minimax",
      "base_url": "https://api.minimaxi.com/v1",
      "api_key": "$MINIMAX_API_KEY",
      "wire_api": "chat"
    }
  ]
}
```

职责：

- 决定 Codex Box 本地代理最终转发到哪个真实上游 API。
- 由模型目录里的 `backend_provider` 引用。

设计要求：

- UI 中不要再把它泛称为“Provider”或“模型来源”，应更明确地叫“上游 API”或“Backend Provider”。
- API key 必须使用环境变量引用，不能明文落盘。
- 它不直接决定 Codex App 的会话归属。

### 4. 模型目录 Provider

对应 `~/.codex/codex-box/custom_model_catalog.json` 中每个模型条目：

```json
{
  "slug": "minimax",
  "display_name": "Minimax",
  "provider": "codex_local_access",
  "backend_provider": "minimax",
  "backend_model": "MiniMax-M3",
  "visibility": "list"
}
```

职责：

- `provider`：模型在 Codex 下拉/展示侧归属哪个 Codex provider。
- `backend_provider`：本地代理实际转发时使用哪个上游 API provider。
- `backend_model`：发给上游 API 的真实模型名。

设计要求：

- Models 页面必须同时展示 `provider` 与 `backend_provider`，不能只展示一个“来源”。
- 添加模型和 MultiRouter 同步时，默认展示归属 provider 使用稳定桶 `codex_model_router_v2`；旧 `codex_local_access` 只作为历史兼容来源识别，不应继续作为新写入默认值，也不应写死为 `openai` 或旧 `opencodex`。
- 模型可见性只控制是否进入下拉列表，不代表上游 API 可用。

## 当前生效链路

Codex Box 必须提供一个“当前生效链路”视图，按以下顺序解释真实状态：

```text
当前 model_provider
  ↓
请求入口：openai_base_url 或 [model_providers.<id>].base_url
  ↓
模型目录：model_catalog_json
  ↓
当前模型：model / catalog slug
  ↓
展示归属：catalog.provider
  ↓
真实上游：catalog.backend_provider
  ↓
上游地址：providers.json[backend_provider].base_url
```

示例：

```text
当前 Provider: codex_local_access
请求入口: http://127.0.0.1:1455/v1
模型目录: ~/.codex/codex-box/custom_model_catalog.json
当前模型: minimax
展示归属: codex_local_access
真实上游: minimax
上游地址: https://api.minimaxi.com/v1
```

如果链路中任一环缺失，UI 应显示断链原因，而不是只显示“模型不存在”或“provider 不可用”。

## 页面职责调整

### Models

职责：管理模型目录。

- 文件：`~/.codex/codex-box/custom_model_catalog.json`
- 展示：`slug/display_name/provider/backend_provider/backend_model/visibility`
- 不应默认修改 `~/.codex/config.toml`。

### Backend Providers / 上游 API

职责：管理真实 API 来源。

- 文件：`~/.codex/codex-box/providers.json`
- 展示：`name/base_url/wire_api/api_key_ref/enabled`
- 不应暗示它就是 Codex App 的 `model_provider`。

### Codex Model Providers

职责：管理 Codex 原生 provider。

- 文件：`~/.codex/config.toml`
- 字段：`model_provider`、`openai_base_url`、`[model_providers.*]`
- 支持设置当前 active provider。

### Codex Runtime / 当前生效链路

职责：解释和启用 Codex 到本地代理的接入链路。

- 显示当前链路和断链检查。
- 启用时必须预览 diff 并让用户确认。
- 不再把“添加模型”和“启用到 Codex”混在一个保存动作里。

### Work Profiles

职责：工作配置管理。

- 只管理默认模型、reasoning、sandbox、approval、MCP 等工作上下文。
- 后续应对齐新版 Codex profile 文件机制：`~/.codex/<name>.config.toml`。
- 不应继续把上游 API provider 和工作配置混在一起。

## 影响

### 正面

- 用户可以保留并选择已有多个 Codex `Model Provider`。
- UI 能解释“为什么下拉框没有模型”“为什么请求走错上游”“为什么历史会话列表归属变化”。
- 减少 `openai`、`opencodex`、`backend_provider` 三类名字混用带来的误操作。

### 成本

- 需要调整页面命名和文案。
- 需要把 `simple_model_config_save` 这类“一键写三处”的动作拆成更明确的步骤。
- 需要新增当前生效链路聚合视图和断链诊断。

## 后续动作

1. 新增“当前生效链路”只读面板，先读清楚现状，不写配置。
2. 将 `Provider Routes` 页面重命名或文案调整为“上游 API / Backend Providers”。
3. Models 页面显示并允许编辑 `provider` 与 `backend_provider`。
4. Runtime 页收敛为一个明确的“启用到 Codex”流程，避免旧 Inject 与 Conversation Provider 并列造成混乱。
5. 审查 `simple_model_config_save`，避免添加模型时隐式修改 `~/.codex/config.toml`。
6. Profiles 页面后续迁移到新版 Codex profile 文件机制。
