# Codex Box · PRD v0.1

> 围绕 OpenAI Codex 的本地配置与网关管理器
> 状态：v0.1 草案 · 2026-06-24

---

## 1. 产品定位

**Codex Box** 是一个面向 OpenAI Codex 用户的本地配置与网关管理器。

核心价值：让用户更安全、更直观地管理 Codex 的配置、profile、provider、MCP server、network route 和诊断信息。

**它不是**：
- ❌ 账号切换器
- ❌ Token 抓取工具
- ❌ 登录绕过、rate limit 规避、账号限制绕过工具
- ❌ Codex Desktop 内部改造工具

---

## 2. 目标用户

- 高频使用 `Codex CLI` / `Codex Desktop` 的开发者
- 需要在多个 provider、profile、proxy、MCP server 之间切换的用户
- 需要安全修改 `~/.codex/config.toml` 的用户
- 需要快速诊断 Codex 网络、provider、MCP、auth 状态的用户

---

## 3. MVP 目标

P0 版本要解决三个核心问题：

1. **看得清**：把 `~/.codex/config.toml` 结构化展示出来
2. **改得稳**：所有配置修改支持 backup、diff、atomic write、rollback
3. **查得快**：能对 provider、network、MCP、auth 做基础健康检查

---

## 4. P0 功能范围

### Dashboard
- 展示当前 active profile、provider、network route、MCP servers、auth 状态
- 最近备份、健康检查摘要
- 启动设置引导（3 步骤：设置 Profile / 连接 Provider / 配置 MCP Server）

### Config 管理
- 读取、解析、展示 `~/.codex/config.toml`
- 修改前自动 backup
- atomic write 写回
- config diff 预览
- 回滚到历史备份

### Profiles
- 展示 profile config
- 新建、编辑、删除 profile
- 设置 active / default profile
- 校验 profile 引用的 provider、sandbox、approval、env

### Providers
- 管理 `model_provider / model_providers`
- 支持 OpenAI official API、OpenAI-compatible API、本地 gateway
- 官方订阅通道仅做状态识别与配置边界展示，不抓取、不复用、不转发 Codex / OpenAI 账号 token
- provider URL、model、env secret 引用管理
- secret 不明文展示

### Gateway / Adapter 底座
- 在 M2 先完成 provider、profile、protocol adapter 的配置建模
- 模型选择器切换完整 profile，而不是只切换裸 `model` 字段
- 第三方 API 统一走显式 `base_url + env secret` 引用
- 不同上游的 Chat Completions、Responses API、SSE stream、tool calls、reasoning、usage、error 结构在 adapter 层统一
- Open Codex、codex-proxy、CLIProxyAPI 等参考项目只作为 provider / gateway preset 接入，不与官方订阅认证混用

### MCP Servers
- 展示 MCP server 配置
- 新建、编辑、禁用、删除
- 检测 command / path / env 是否可用
- 日志与错误信息脱敏

### Network
- direct
- HTTP proxy
- SOCKS proxy
- Clash / Mihomo profile 引用
- 连通性测试
- **不**默认接管系统全局代理

### Diagnostics
- Codex config 语法检查
- provider endpoint 测试
- auth 状态检测
- MCP server 可执行性检测
- proxy 连通性检测
- 输出可复制的脱敏诊断报告

---

## 5. P1 功能范围

- Provider presets
- CLIProxyAPI / codex-proxy / local gateway 启动、停止、日志管理
- 项目目录绑定 profile
- system tray
- 配置历史时间线
- 配置导入导出

---

## 6. 暂缓范围（P2）

- 多账号额度监控
- 自动登录 / OAuth 接管 / token 抓取
- 修改 Codex Desktop 内部 UI
- 深度控制 Clash / Mihomo 全局规则
- 团队同步 SaaS
- 自研完整 gateway

---

## 7. 核心页面

| 页面 | 说明 |
|---|---|
| `Dashboard` | 总览 + 启动引导 + 健康检查 |
| `Profiles` | profile 管理 |
| `Providers` | model provider 管理 |
| `Network` | network route 管理 |
| `MCP Servers` | MCP server 管理 |
| `Config Diff` | 配置 diff 与回滚 |
| `Diagnostics` | 诊断与脱敏报告 |
| `Settings` | 应用设置 |

---

## 8. 关键数据模型

- `CodexConfigSnapshot`
- `CodexProfile`
- `ModelProvider`
- `NetworkRoute`
- `McpServer`
- `BackupRecord`
- `HealthStatus`
- `SecretRef`

详细字段定义见 [data-model/v0.1.md](./data-model/v0.1.md)。

---

## 9. 技术方向

- Desktop：`Tauri`
- Frontend：`React + TypeScript`
- UI：`Tailwind CSS + shadcn/ui`
- Backend：`Rust`
- Config：`TOML parser`（`toml` crate）
- Diff：结构化 diff + 文本 diff
- 写入策略：backup first + atomic write（写临时文件 → rename）

---

## 10. 安全原则

- ✅ 写入前必须 backup
- ✅ 写入前展示 diff
- ✅ 写入必须 atomic write
- ✅ secret 不明文展示
- ✅ 日志必须脱敏
- ✅ 不默认接管系统代理
- ✅ 不默认修改 Codex Desktop 内部文件
- ❌ 不做 token 抓取或登录绕过

---

## 11. 里程碑

| 里程碑 | 范围 |
|---|---|
| M0 | 技术验证：Tauri 读取/写入 TOML、备份、diff、atomic write |
| M1 | 只读 Dashboard |
| M2 | Profile + Provider MVP（含 gateway / adapter 配置底座） |
| M3 | Network + Diagnostics |
| M4 | MCP Manager |
| M5 | Gateway 体验增强（CLIProxyAPI / codex-proxy 启停、日志、preset） |
| M6 | 桌面体验打磨（system tray / 导入导出） |

---

## 12. 风险边界

> 这部分为**红线**，开发时必须遵守。

### 绝对不做
- 不抓取任何账号 token
- 不绕过 OpenAI 官方登录
- 不规避 rate limit / 账号配额限制
- 不默认修改 Codex Desktop 内部文件
- 不接管系统全局代理
- 不做团队同步 SaaS

### 写入红线
- 任何 `~/.codex/config.toml` 写入必须先 backup
- 任何写入必须 atomic write（写 `.tmp` → rename）
- 写入前必须展示 diff
- 写入失败必须能 rollback 到最近一次 backup

### 隐私红线
- secret 字段（API key、token）**永远不写日志**
- 日志默认脱敏（key 显示为 `sk-proj-***`）
- 不上传任何用户配置到外部服务
