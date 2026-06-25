# ADR 0006 · BYOK 本地代理 runtime

> 状态：Accepted
> 日期：2026-06-25
> 决策者：项目组
> 关联：ADR-0005(BYOK 模型下拉与多 provider 路由)

## 背景

ADR-0005 决策了"通过安全读写 `~/.codex/config.toml` 与 `~/.opencodex/*.json` 实现 BYOK 模型下拉"。该方案在配置侧已完整闭环:

- `~/.codex/config.toml` 写入走 backup → diff → confirm → atomic write → rollback
- `~/.opencodex/{providers.json, custom_model_catalog.json}` 同样闭环
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

- `GET /v1/models`:合并 `~/.codex/config.toml` 的 `[model_providers.*].models` + `~/.opencodex/custom_model_catalog.json` 的 `visible=true` 条目;跳过 `codex-subscription`;返回 OpenAI 标准 schema;id 命名空间化为 `provider/model`
- `POST /v1/chat/completions` / `POST /v1/responses`:按 model id 解析路由,注入 `${ENV_VAR}` 鉴权 + provider.http_headers,SSE 透传
- `GET /healthz`:内存状态

### 3. 写入闭环扩展

新增两个写入操作(走既有 backup → diff → confirm → atomic → rollback):

- **InjectProxyBaseUrl**:把 `~/.codex/config.toml` 的非订阅 provider 的 `base_url` 重写为 `http://127.0.0.1:{port}/v1`,原值记入 `~/.codex/codex-box/inject-map.json`
- **RestoreBaseUrl**:从 inject-map 反向写回原 base_url,清空 inject-map

### 4. 与 AITabby/opencodex 的边界

- **复用**:`~/.opencodex/{providers.json, custom_model_catalog.json}` 的文件路径与字段约定
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
- 与 AITabby/opencodex CLI 工具可共存(共享 `~/.opencodex/*.json` 约定)
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

- Codex App 升级后可能改 picker 行为;**但** Codex Box 对 Codex App 呈现的就是"base_url=127.0.0.1:1455 的 provider",大多数 picker 协议变化对代理实现是透明的
- 国产模型 API 协议差异:少数非标准 OpenAI-compatible 通过 provider.http_headers 暴露自定义 header;v1 范围透传 body 不做协议转换
- SSE 大流量性能未压测;v2 优化
- 端口冲突:probe_port 5 次重试,都失败返回 AppError;用户在 UI 看到具体原因

## 后续动作

1. ✅ `src-tauri/src/proxy/{mod,inject_map,state,routing,models_endpoint,upstream,server,health,lifecycle}.rs` 已实现
2. ✅ `src-tauri/src/commands/proxy.rs` 已实现,Tauri command 注册完毕
3. ✅ `src/pages/WorkspacePages.tsx` CodexRuntimePage 升级为 Codex Box Runtime 控制台
4. ✅ i18n key `pages.codexBoxRuntime.*` 已加(en + zh)
5. ✅ `cargo test --lib` 65 个测试全部通过(routing / inject_map / models_endpoint / upstream / health / lifecycle)
6. ✅ `pnpm build` 前端类型检查 + Vite bundle 通过
7. ⏳ v0.3 文档同步(PRD.md §1 §5 §9、AGENTS.md M2.6、docs/architecture/v0.3-BYOK.md 拓扑图 + §7 安全边界 + 新增 §10)

## 里程碑

- v0.3.1:BYOK 本地代理 runtime 落地
- 旧 M2.5 (BYOK 模型目录与多 provider 路由底座)保留
- 新增 M2.6 (本地代理 runtime 落地)
- M3/M4/M5 不变
