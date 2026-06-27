# Codex Box

> 面向 OpenAI Codex 的本地桌面控制台与 BYOK 模型下拉参考实现。

Codex Box 使用 `Tauri + React + TypeScript + Tailwind + Rust` 构建。它尝试通过安全管理 `~/.codex/config.toml` 与 `~/.codex/codex-box/` 下的本地配置，让 Codex Desktop / Codex CLI 在同一套本地工作流里看到官方订阅、OpenAI 官方 API、第三方 OpenAI-compatible API 和本地 gateway。

![Codex Box 真实运行截图](./docs/assets/codex-box-real-preview.png)

## 项目定位

这个项目更适合作为一个 **Codex BYOK 客户端样式与本地路由参考**，而不是一个已经面向所有第三方模型完全打磨好的商业成品。

它已经提供了一个可运行的方向：

- 用本地 `127.0.0.1` 代理暴露 `/v1/models`。
- 把多个 provider 的模型合并到 Codex App 的模型下拉框。
- 按模型 id 把请求转发到不同上游 provider。
- 用 `providers.json` 和 `custom_model_catalog.json` 管理模型来源和展示目录。
- 写入配置前做 backup、diff、atomic write 和 rollback。

但第三方模型的兼容细节非常多。很多 OpenAI-compatible 服务只是“接口长得像”，真正放进 Codex Desktop 后，还要处理 `Responses API`、streaming events、tool calls、reasoning、vision、computer use、错误格式、上下文行为和模型白名单等问题。

所以这里不承诺“任意第三方模型都能像官方 Codex 模型一样稳定使用”。如果你要继续用它接客户、接更多模型或做成稳定产品，还需要自己按目标 provider 慢慢适配、测试和完善。

## 工作原理

Codex Box 的核心思路不是修改 Codex Desktop 本体，也不是抓取官方账号 token，而是把 Codex App 原本要访问的模型 provider 指向一个本机代理。

简化后的链路如下：

```text
Codex Desktop / Codex CLI
        ↓ 读取
~/.codex/config.toml
        ↓ 当前 model_provider 指向本机
http://127.0.0.1:1455/v1
        ↓ Codex Box 本地代理
读取 ~/.codex/codex-box/providers.json
读取 ~/.codex/codex-box/custom_model_catalog.json
        ↓ 按 model id 路由
OpenAI 官方 API / 第三方 OpenAI-compatible API / 本地 gateway
```

也就是说，Codex App 看到的是一个统一的本地 provider；Codex Box 在本地代理里再把不同模型分发到不同上游。这样可以让多个 provider 的模型出现在同一个 picker 里，也能把 `model id`、上游真实模型名、鉴权方式和协议格式分开管理。

这个模式主要依赖三类配置：

| 文件 | 作用 |
|---|---|
| `~/.codex/config.toml` | Codex App 读取的主配置；把当前 provider 指向 `127.0.0.1` 本地代理。 |
| `~/.codex/codex-box/providers.json` | 记录真实上游 provider，例如 base URL、协议类型、API Key 环境变量引用。 |
| `~/.codex/codex-box/custom_model_catalog.json` | 记录哪些模型显示在 Codex 下拉框，以及这些模型对应哪个真实上游模型。 |

## 配置模板示例

下面只是脱敏后的通用模板，用来说明结构，不是实际可直接复制的配置。请按自己的 provider、模型名和环境变量调整。

`~/.codex/config.toml` 中大致会有一个本地 router provider：

```toml
model = "example-chat"
model_provider = "codex_model_router_v2"
model_catalog_json = "~/.codex/codex-box/custom_model_catalog.json"

[model_providers.codex_model_router_v2]
name = "Codex Box Router"
base_url = "http://127.0.0.1:1455/v1"
wire_api = "responses"
models = [
  "example-chat",
  "example-reasoner"
]
supports_websockets = false
```

`~/.codex/codex-box/providers.json` 里保存真实上游 provider。API Key 只放环境变量引用，不写明文：

```json
{
  "schema_version": 1,
  "providers": [
    {
      "name": "example-provider",
      "base_url": "https://api.example.com/v1",
      "wire_api": "chat",
      "api_key_ref": "${EXAMPLE_PROVIDER_API_KEY}",
      "http_headers": {},
      "enabled": true,
      "note": "Example OpenAI-compatible provider"
    },
    {
      "name": "codex_model_router_v2",
      "base_url": "http://127.0.0.1:1455/v1",
      "wire_api": "responses",
      "api_key_ref": null,
      "http_headers": {},
      "enabled": true,
      "codex_routing": {
        "enabled": true,
        "default_route_id": "example-provider",
        "routes": [
          {
            "id": "example-provider",
            "label": "Example Provider",
            "enabled": true,
            "target_provider_id": "example-provider",
            "match": {
              "models": ["example-chat", "example-reasoner"],
              "prefixes": []
            },
            "upstream": {
              "api_format": "openai_chat",
              "auth": { "source": "provider_config" },
              "model_map": {
                "example-chat": "vendor-chat-model",
                "example-reasoner": "vendor-reasoning-model"
              }
            }
          }
        ]
      }
    }
  ]
}
```

`~/.codex/codex-box/custom_model_catalog.json` 里保存 Codex 下拉框展示的模型：

```json
{
  "schema_version": 1,
  "models": [
    {
      "model_id": "example-chat",
      "display_name": "Example Chat",
      "provider": "codex_model_router_v2",
      "backend_provider": "codex_model_router_v2",
      "backend_model": "vendor-chat-model",
      "visible": true,
      "targetProvider": "example-provider"
    },
    {
      "model_id": "example-reasoner",
      "display_name": "Example Reasoner",
      "provider": "codex_model_router_v2",
      "backend_provider": "codex_model_router_v2",
      "backend_model": "vendor-reasoning-model",
      "visible": true,
      "targetProvider": "example-provider",
      "reasoning": {
        "enabled": true,
        "levels": ["medium"]
      }
    }
  ]
}
```

真实运行时还要根据 provider 能力选择 `wire_api`：

- 上游原生支持 `/v1/responses`：优先用 `responses`，减少协议转换。
- 上游只支持 `/v1/chat/completions`：用 `chat`，由本地代理做 Responses 和 Chat 的转换。
- 上游有特殊 header、特殊 reasoning 参数、纯文本限制或视觉能力差异：需要继续补适配。

## 当前状态

| 模块 | 状态 | 说明 |
|---|---:|---|
| Overview | 可用 | 展示配置读取、picker 连接、secret 安全等关键状态。 |
| Models | 可用底座 | 支持新增模型配置，写入 provider 和模型目录。 |
| Provider Routes | 可用底座 | 读取、启停、删除 `~/.codex/codex-box/providers.json` 中的 provider route。 |
| Custom Model Catalog | 可用底座 | 通过 `~/.codex/codex-box/custom_model_catalog.json` 管理 Codex picker 可见模型。 |
| Local Proxy Runtime | 可用底座 | Rust 后端提供 `127.0.0.1` 本地代理，覆盖 `/v1/models`、`/v1/responses`、`/v1/chat/completions` 等入口。 |
| MultiRouter | 实验性 | 将官方模型和 BYOK 模型收敛到一个本地 router provider，再按模型 id 分流。 |
| Config 写入闭环 | 可用 | 支持 backup、diff、atomic write、rollback、`content_hash` 并发校验。 |
| Logs / Diagnostics | 开发中 | 需要继续补齐 provider 兼容性检查、调用诊断和脱敏报告。 |
| Settings | 开发中 | 管理语言、安全策略、备份策略和实验功能开关。 |

## Provider 自定义能力

底层已经支持自定义 model provider，但还不是完整的可视化 provider 管理器。

已经支持：

- 自定义 `name`、`base_url`、`wire_api`、`api_key_ref`、`http_headers`、`enabled`。
- 通过 `model_id -> backend_provider -> backend_model` 做模型路由。
- 将第三方模型收敛到 `codex_model_router_v2` 这类本地 router provider。
- 通过 `${ENV_VAR}` 引用 API Key，避免把明文 secret 写入配置文件。
- 保留未知字段，方便兼容其他工具或后续 schema。

还需要继续完善：

- Provider 的完整编辑表单。
- `http_headers`、特殊鉴权、特殊 reasoning 参数的 UI。
- Provider 健康检查和一键测试。
- 不同模型厂商的兼容性预设。
- `Responses` / `Chat Completions` / streaming / tool call 的分项测试。
- 对已有 `~/.codex/config.toml`、`~/.opencodex/`、CC Switch 等配置的更完整导入和迁移。

## 第三方模型兼容性说明

如果你只是把一个第三方 OpenAI-compatible API 接进来，最常见的问题不在“请求能不能发出去”，而在“Codex Desktop 期待的协议细节是不是都对得上”。

可能遇到的问题包括：

- 模型出现在下拉框里，但调用失败。
- 普通聊天可用，但工具调用不完整。
- 非流式可用，但 streaming events 不符合 Codex 预期。
- `reasoning` 字段不兼容。
- 图片、文件、音频输入不兼容。
- 上游错误格式不标准，导致 Codex UI 展示异常。
- 上下文、历史消息或 tool call 状态机需要额外转换。

因此，这个项目可以给你一个客户端和本地代理的样式，也可以作为继续开发的起点；但第三方模型兼容性请按实际 provider 自行验证。是否继续完善、做预设、做商业化交付，大家看情况而定。

## 安全边界

Codex Box 明确不做以下事情：

- 不抓取任何账号 token。
- 不绕过 OpenAI 官方登录。
- 不规避 rate limit 或账号配额限制。
- 不默认修改 Codex Desktop 内部文件，包括 `app.asar`、renderer、IPC。
- 不接管系统全局代理。
- 不上传用户配置，不做团队同步 SaaS。
- 不复制 AITabby/opencodex 源码、UI 或长段文案。

真实写入 `~/.codex/config.toml` 前必须满足：

1. 先创建 backup。
2. 展示 diff 并让用户确认。
3. 使用 atomic write：先写临时文件，再 rename。
4. 写入失败可 rollback 到最近一次 backup。
5. API key / token / secret 永远不写日志，不在 UI 明文展示。

## 技术栈

| 层 | 技术 |
|---|---|
| Desktop | Tauri 2 |
| Frontend | React 18 + TypeScript |
| UI | Tailwind CSS + lucide-react |
| State | zustand |
| Data fetching | TanStack Query + Tauri invoke |
| Backend | Rust |
| Config | `toml` crate + serde |
| Diff | `similar` crate |
| Local proxy | axum + reqwest |

## 项目结构

```text
.
├─ src/                         # React 前端
│  ├─ components/               # 通用组件
│  ├─ pages/                    # Overview / Models / Provider Routes / Runtime / Logs / Settings
│  ├─ lib/                      # API、i18n、mock data、类型
│  ├─ store/                    # zustand stores
│  └─ styles/                   # 字体加载
├─ src-tauri/                   # Rust / Tauri 后端
│  └─ src/
│     ├─ commands/              # Tauri command 边界
│     ├─ config/                # config 读取、解析、diff、backup、writer
│     └─ proxy/                 # 本地代理 runtime
├─ docs/
│  ├─ architecture/             # 架构方案
│  ├─ design/                   # UI 设计规范
│  ├─ data-model/               # 数据模型
│  ├─ decisions/                # ADR
│  └─ references/               # 参考项目技术事实
├─ PRD.md
├─ AGENTS.md
└─ LICENSE
```

## 快速开始

### 环境要求

- Node.js 18+
- pnpm
- Rust stable
- Tauri 2 所需系统依赖

### 安装依赖

```bash
pnpm install
```

### 启动前端开发服务

```bash
pnpm dev
```

默认地址：

```text
http://127.0.0.1:1420/
```

### 启动 Tauri 桌面应用

```bash
pnpm tauri:dev
```

### 构建

```bash
pnpm build
pnpm tauri:build
```

## 验证命令

```bash
pnpm exec tsc --noEmit
pnpm build
cargo test --manifest-path src-tauri/Cargo.toml
```

说明：如果只修改前端页面和文案，通常只需要跑前两条；涉及 Rust config parser、writer、proxy、diagnostics 时再跑 `cargo test`。

## 后续可以继续做什么

- 做 provider preset：OpenRouter、DeepSeek、MiniMax、Moonshot、智谱、硅基流动、本地 gateway。
- 做 provider 兼容性测试矩阵。
- 做可视化 route editor 和 model map editor。
- 做更完整的导入向导。
- 做 Diagnostics，把失败原因翻译成人能处理的建议。
- 做更稳定的 Codex Desktop picker 刷新策略。

## License

MIT License。详见 [LICENSE](./LICENSE)。
