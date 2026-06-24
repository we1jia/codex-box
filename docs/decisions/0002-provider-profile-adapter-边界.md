# ADR 0002 · Provider/Profile/Adapter 边界

> 状态:Accepted
> 日期:2026-06-24
> 决策者:项目组

---

## 背景

Codex Box 的长期目标不是只做 `~/.codex/config.toml` 可视化编辑器，而是要在本地安全管理 Codex 的 provider、profile、network、MCP 和 gateway 配置。

用户希望保留官方 Codex / OpenAI 订阅体验，同时也能接入第三方 OpenAI-compatible API、本地 gateway 或参考 Open Codex、codex-proxy、CLIProxyAPI 一类项目的网关能力。

这里的风险是：官方订阅通道和第三方 API 通道的认证、请求协议、响应协议不同，如果只在 UI 里替换 `model` 字段，会导致请求无法被目标上游接收，也容易把账号认证边界混在一起。

## 决策

采用三层边界：

1. `Provider`
   - 表示一个可调用的上游或本地 gateway。
   - 第三方 API 必须显式声明 `base_url`、`wire_api` 和 `api_key_ref` / `env_key`。
   - 官方订阅通道只允许做状态识别和边界展示，不抓取、不复用、不转发账号 token。

2. `Profile`
   - 模型选择器展示和切换的是 profile。
   - profile 绑定 `model`、`model_provider`、sandbox、approval、network、MCP 引用等完整上下文。
   - 不把 profile 简化为裸模型名。

3. `Protocol Adapter`
   - 负责把内部标准请求映射到 Chat Completions、Responses API 或本地 gateway 自定义格式。
   - 负责把 SSE stream、tool calls、reasoning、usage、error 等响应结构统一回内部格式。
   - M2 只落配置建模，M5 再做 gateway 启停、日志和 preset。

## 兼容策略

- 读取时兼容 `[model_providers.*]`、`[profile.*]`、`[profiles.*]`。
- 未识别字段必须保留在 `other_tables` 或原始 TOML 中，后续写入不能无意删除。
- 第三方 API 只通过环境变量或 Keychain 引用 secret，不落明文。
- Open Codex、CM-NPN、Cockpit Tools 等参考项目只沉淀为 preset、诊断项或管理视图，不直接复制其账号处理逻辑。

## 影响

### 正面

- 官方订阅与第三方 API 不混用认证，安全边界清楚。
- 后续可以渐进接入多个 gateway，而不用重做 Profiles 页面。
- Dashboard、Providers、Diagnostics 都能基于同一套 provider/profile 数据工作。

### 成本

- M2 需要比原计划更早补齐 provider/profile parser。
- Adapter 只先建模，不承诺 M2 完成真实请求转换。

## 红线

- 不抓取任何账号 token。
- 不绕过 OpenAI 官方登录。
- 不规避 rate limit 或账号配额限制。
- 不默认修改 Codex Desktop 内部文件。
- secret 字段不写日志、不导出、不进入诊断报告明文。
