# ADR 0006 · BYOK 本地代理 runtime

> 状态：Accepted
> 日期：2026-06-25
> 决策者：项目组
> 关联：ADR-0005(BYOK 模型下拉与多 provider 路由)

## 背景

ADR-0005 与 ADR-0008 决策了"通过安全读写 `~/.codex/config.toml` 与 `~/.codex/codex-box/*.json` 实现 BYOK 模型下拉,并把 `~/.opencodex/*.json` 降级为只读导入来源"。该方案在配置侧已完整闭环:

- `~/.codex/config.toml` 写入走 backup → diff → confirm → atomic write → rollback
- `~/.codex/codex-box/{providers.json, custom_model_catalog.json}` 同样闭环
- secret 走 `${ENV_VAR}` 引用,不落盘不上日志
- UI(Models / Provider Routes / Codex Runtime / Diagnostics / Settings)骨架就绪

但实施过程中核实发现两个**关键目标缺口**:

### 1. Codex App picker 不会自动合并多 provider 模型

核对 AITabby/opencodex v0.2.2 实际源码(`src/proxy/index.ts`)后确认:

```ts
// AITabby/opencodex 实际做法:把 [model_providers.*].base_url 改成
// http://127.0.0.1:8765/v1,让 Codex App 看到"一个本地代理 provider"
autoPatchCodexConfig()
  → for each [model_providers.<name>]:
       base_url = "http://127.0.0.1:8765/v1"
```

Codex App 自己的 picker **不**做跨 `[model_providers.*]` 段的模型合并;它只对当前 active profile 的 `model_provider.base_url` 发请求,model 列表从该 base_url 拉取。如果用户在 `~/.codex/config.toml` 配了 N 个 provider:

- Codex App 仍然按 `model + model_provider` 组合来切,而不是按"全局统一模型列表"
- 各 provider 之间的模型展示由 Codex App 内部决定,**不一定**全部展示
- 没有"统一命名空间 + visibility toggle"的能力

### 2. 切换后请求的真实路由取决于 base_url 是否合规

如果用户为 DeepSeek 配 `https://api.deepseek.com/v1`(标准 OpenAI-compatible),Codex App 自身的请求适配层大概率能 work;但**鉴权注入、模型命名空间、合并展示、隐藏控制**等能力 Codex App 都不提供。

要实现"像 Cursor 那样在同一个下拉里看并切换官方订阅 / 官方 API / 第三方 / 国产 / 本地 gateway",**必须**让 Codex App 看到一个统一的"假"provider,由 Codex Box 自己按 model id fan-out。

## 决策

Codex Box v0.3.1 在 v0.3 配置侧闭环基础上,**新增本地 HTTP 代理 runtime**,由 Rust 后端内置管理:

### 1. 进程模型

- 进程内 tokio task(不 spawn 子进程,不依赖 Node 运行时)
- 监听 `127.0.0.1:1455`(可改,启动时 probe 端口,冲突 +1 最多 5 次)
- 由 Tauri `AppHandle` 持有 `Arc<ProxyState>`,生命周期随 Codex Box 进程
- 状态:stopped / starting / running(port) / failed,持久化到 `~/.codex/codex-box/runtime-state.json`

### 2. 端点

- `GET /v1/models`:合并 `~/.codex/config.toml` 的 `[model_providers.*].models` + `~/.codex/codex-box/custom_model_catalog.json` 的 `visible=true` 条目;跳过 `codex-subscription` 与旧 `opencodex/8765` 残留;返回 OpenAI 标准 schema;id 优先使用 catalog 裸 `slug/model_id`,对齐 AITabby/opencodex picker 行为;catalog 条目额外投影 `displayName/contextWindow/maxContextWindow/reasoning/inputModalities/backendProvider` 等 Codex picker 元数据
- `POST /v1/chat/completions` / `POST /v1/responses`:按 model id 解析路由,注入 `${ENV_VAR}` 鉴权 + provider.http_headers,SSE 透传
- HTTP `POST /v1/responses` 对 `wire_api=chat` provider 保持真实 Chat Completions streaming,逐行把 Chat SSE 转回 Responses SSE,不再把 `stream=true` 降级成非流式请求。
- `GET /v1/responses` WebSocket:Codex Desktop 发 `response.create` 时按真实 provider 的 `wire_api` 分流。`wire_api=responses` 优先 passthrough 到上游 `/responses` 并做最小 normalize;`wire_api=chat` 才转换为 `/chat/completions` 再翻译回 Responses events。
- Chat fallback 的 translator 已覆盖 reasoning item 生命周期、tool/function call 参数流式拼接、Responses input 中 function_call/function_call_output/custom_tool_call/tool_search_call 到 Chat messages 的转换、`input_image` 到 Chat `image_url` 的转换、`input_file` / `input_audio` 到 Chat content block 的转换、Responses `tools/tool_choice` 到 Chat `tools/tool_choice` 的转换、custom tool 原始 `format` / grammar 元数据保留、`tool_search_output.tools` 动态补工具、流式 `include_usage` 注入与 usage 映射、已知纯文本模型多模态占位替换,以及按 session 维护的 conversation history。
- MultiRouter 会把 `codexRouting.routes[].capabilities`、catalog/provider extra 中的 `textOnly`、`inputModalities` 传到 Chat fallback translator。明确 text-only 的模型不会收到 Chat `image_url` 块,而是降级为文本占位;若 catalog 或 route 启用 vision bridge,代理会先调用配置的 OpenAI-compatible 视觉模型,把截图替换为 `[截图描述: ...]` 后再交给文本上游。
- native GPT/Codex catalog entry 会优先匹配 MultiRouter 自动生成的 `openai-official` route(`auth.source = "managed_codex_oauth"`,`baseUrl = "https://chatgpt.com/backend-api/codex"`);只有没有 route 时才进入 native OpenAI fallback。
- Diagnostics 会把 `openai-official` 是否存在、是否使用 `managed_codex_oauth`、是否指向 ChatGPT Codex backend 单独列为官方 GPT 路由状态,避免把“官方登录态缺失”和“MultiRouter route 没接上”混成一个问题。
- native OpenAI forward 只透传 Codex Desktop 当前请求携带的真实认证上下文;若检测到 `PROXY_MANAGED` 占位符,本地返回 `native_openai_auth_unresolved`,不会把占位符上送到 ChatGPT/OpenAI 官方后端。
- `GET /healthz`:内存状态

### 3. 写入闭环扩展

新增两个写入操作(走既有 backup → diff → confirm → atomic → rollback):

- **InjectProxyBaseUrl**:把 `~/.codex/config.toml` 的非订阅 provider 的 `base_url` 重写为 `http://127.0.0.1:{port}/v1`,原值记入 `~/.codex/codex-box/inject-map.json`
- **RestoreBaseUrl**:从 inject-map 反向写回原 base_url,清空 inject-map

Models 页与 Runtime 页主按钮共用 **MultiRouter Sync** 写入闭环:

- `codex_multirouter_preview` 只生成 `providers.json`、`custom_model_catalog.json`、`config.toml`、`models_cache.json`、`inject-map.json` 的 diff 与校验 hash,不落盘
- `codex_multirouter_apply` 必须携带 `confirmed=true`,把第三方/BYOK 模型收敛到稳定的 `codex_model_router_v2` router provider,并为官方原生 GPT 条目生成 `openai-official` managed route,把合并目录同步到 Codex App 热缓存 `~/.codex/models_cache.json`,并按预览清理旧 `opencodex/8765` inject-map 残留。旧 `codex_local_access` 只作为历史兼容桶和归并来源保留。
- 第三方/BYOK catalog 条目在收敛到 router provider 后,保留 `targetProvider` / `target_provider` 指向真实上游服务商。`backend_provider` 继续作为机器路由归属字段,UI 与 `/v1/models` 展示层优先显示 `targetProvider`,并在需要时额外暴露 `routerProvider`。
- 若本机 `models_cache.json` 没有可导入的官方 GPT/Codex 原生模型,MultiRouter 预览会用 Codex Box 内置官方默认候选生成 native OpenAI catalog entries,避免“Models 页能看到默认 GPT,但 Codex App 实际 catalog/cache 没有这些模型”的断层。官方模型请求仍只透传 Codex Desktop 自身认证上下文,Codex Box 不读取官方 token。
- Runtime 页"连接到 Codex"不再只激活单个 BYOK 模型;它会先确保本地代理可用,再调用 `codex_multirouter_preview`,让用户确认五份 diff 后执行 `codex_multirouter_apply`
- 该流程用于让 Codex App picker 在同一个下拉里显示订阅 GPT、官方 API、第三方 OpenAI-compatible、本地 gateway 等模型;真实请求由本地代理按 `codexRouting.routes` 分流
- `config.toml` 投影必须使用实际 `routerProviderId` 写入顶层 `model_provider` 与 `[model_providers.<routerProviderId>]`,并包含 provider 内联 `models = [...]` 与 `supports_websockets = false`。前者兼容 Codex Desktop custom picker 读取 provider 内模型数组的路径,后者避免 router provider 被误判为内置 OpenAI WebSocket 直连。投影的 `base_url` 必须使用本次 preview/apply 的 `proxyBaseUrl`,不能固定写死默认 `1455` 端口。
- `models_cache.json` 投影必须复用现有 `client_version`,并写入 `etag = "codex-box-model-catalog"` 标记所有权;若现有 cache 缺失或没有 `client_version`,则跳过热缓存同步,避免生成 Codex App 不认的 cache。
- 首次覆盖非 Codex Box owned 的 `~/.codex/models_cache.json` 前,必须额外保留 `~/.codex/models_cache.codex-box-backup.json` sidecar。Models 页提供 `codex_models_cache_restore_preview/apply`,仅在当前 cache 仍带 `etag = "codex-box-model-catalog"` 时允许恢复;有 sidecar 则恢复官方缓存,无 sidecar 则删除 owned cache,让 Codex App 下次重建。

### 4. 与 AITabby/opencodex 的边界

- **兼容导入**:`~/.opencodex/{providers.json, custom_model_catalog.json}` 只作为只读扫描和导入来源,不作为 runtime 实时兜底
- **不复用**:不 spawn 外部 Node runner,不复制其源码/UI/长段文案
- **实现独立**:Codex Box 走 axum + reqwest + tracing,自有 license
- **详细对比**:见 [`docs/architecture/v0.3.1-BYOK-proxy.md §2.2`](../architecture/v0.3.1-BYOK-proxy.md)

### 5. 撤销 v0.3 文档里"不起 HTTP 端口"约束

旧 v0.3 文档(架构 §7、ADR-0005 §3、PRD §5 等)里"不监听任何 HTTP/WS 端口"那条红线作废。新约束:

- **仅** 127.0.0.1(绝不绑 LAN / IPv6 / Unix socket)
- **不**接管系统全局代理
- **不**读取 `HTTPS_PROXY` / `ALL_PROXY` 等环境变量作为 outbound 代理
- secret 仍强制 env 引用
- 日志脱敏(Authorization / api_key 等不上日志)

## 影响

### 正面

- Codex App picker 真正看到合并模型列表(单一 `codex-box` provider + 命名空间 model id)
- 切换后请求真实路由到对应 provider(model id → 解析 → 注入鉴权 → 转发)
- 与 AITabby/opencodex CLI 工具可共存(只读扫描 `~/.opencodex/*.json`,导入到 Codex Box 主目录)
- 安全闭环更强(API key 强制 env 引用;v0.3 已有,本 ADR 不变)
- v0.3 配置侧闭环完全保留(不删除任何已有代码)

### 成本

- v0.3 文档里"不起 HTTP 端口"那条约束要作废(架构 §7、ADR-0005 §3、PRD §5)
- 新增代理 runtime 模块(`src-tauri/src/proxy/*`)
- 新增 Tauri commands(proxy_* 系列)
- 前端 CodexRuntimePage 升级为 CodexBoxRuntimePage(状态卡 + 路由表 + /v1/models 预览 + Inject/Restore)
- 新增依赖:axum / reqwest(rustls-tls)/ tower / tracing / bytes / futures
- tokio 增加 `rt-multi-thread` / `macros` / `sync` / `time` / `net` features
- 文档:`docs/architecture/v0.3.1-BYOK-proxy.md`(新)+ `docs/data-model/v0.3-BYOK.md`(扩展)+ PRD/AGENTS 同步

### 风险

- Codex App 升级后可能改 picker 行为;**但** Codex Box 对 Codex App 呈现的就是"base_url=127.0.0.1:{port} 的 provider",大多数 picker 协议变化对代理实现是透明的
- 国产模型 API 协议差异:少数非标准 OpenAI-compatible 通过 provider.http_headers 暴露自定义 header;v1 范围透传 body 不做协议转换
- MiniMax 等 provider 即使支持 Responses,也可能在 event、reasoning、tool call、history、图片/文件/音频输入细节上与 Codex Desktop 期待的 OpenAI 官方 Responses 形状存在差异。Codex Box 的策略不是默认降级成 Chat,而是 Responses 优先 passthrough/minimal normalize;只有 `wire_api=chat` provider 才走 Chat→Responses 转换。Chat fallback 的核心 translator 状态机已补齐到 reasoning/tool/history/image_url/input_file/input_audio/custom tool metadata/usage 层。
- SSE 大流量性能未压测;v2 优化
- 端口冲突:probe_port 5 次重试,都失败返回 AppError;用户在 UI 看到具体原因

## 后续动作

1. ✅ `src-tauri/src/proxy/{mod,inject_map,state,routing,models_endpoint,upstream,server,health,lifecycle}.rs` 已实现
2. ✅ `src-tauri/src/commands/proxy.rs` 已实现,Tauri command 注册完毕
3. ✅ `src/pages/WorkspacePages.tsx` CodexRuntimePage 升级为 Codex Box Runtime 控制台
4. ✅ i18n key `pages.codexBoxRuntime.*` 已加(en + zh)
5. ✅ `GET /v1/models` 已按裸 `slug/model_id` 输出,避免 `provider/model` 污染 Codex App picker,并投影 Codex picker 需要的上下文窗口、reasoning、输入模态、真实上游 `backendProvider` 与本地 `routerProvider` 等元数据
6. ✅ WebSocket `/v1/responses` 已按 `wire_api` 分流:`responses` provider passthrough,`chat` provider 转换
7. ✅ Chat fallback translator 已补 reasoning/tool-call/history/image_url/input_file/input_audio/tools/custom tool metadata/usage 核心状态机,HTTP 与 WebSocket streaming 均保持真实流式桥接
8. ✅ MultiRouter preview/apply 已把 `models_cache.json` 纳入 diff、expected hash、backup/atomic/rollback 链路
9. ✅ MultiRouter owned `models_cache.json` 已支持 sidecar 备份与恢复官方缓存入口
10. ✅ Diagnostics 已提供显式 `codex_desktop_picker_unlock` 入口:仅对已开启 CDP 的 Codex Desktop renderer 注入下拉框白名单补丁,覆盖 Statsig、响应 JSON、MCP `model/list`、app-server `list-models-for-host` 与 React auth context 路径,不写配置、不改 `app.asar`
11. ✅ Diagnostics 已提供显式 `codex_desktop_launch_with_debugging_and_unlock` 入口:只在 Codex Desktop 未运行时用 `--remote-debugging-port` 启动 Desktop 并注入 patch;如果 Desktop 已普通运行,返回 `needs_quit_first`,不杀进程、不强制重启,也不启动/重启 Codex CLI 或 Codex CPP 终端
12. ✅ `cargo test --lib` 通过(routing / inject_map / models_endpoint / upstream / health / lifecycle / responses websocket / conversation history)
13. ✅ 前端类型检查 + Vite bundle 通过
14. ✅ 基础 vision bridge 已接入 Chat fallback;OpenCodex 的 voice / computer-use 专项能力不纳入当前 BYOK 主线,图片压缩与持久描述缓存留作增强

## 里程碑

- v0.3.1:BYOK 本地代理 runtime 落地
- 旧 M2.5 (BYOK 模型目录与多 provider 路由底座)保留
- 新增 M2.6 (本地代理 runtime 落地)
- M3/M4/M5 不变
