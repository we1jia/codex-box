# Codex Box · PRD v0.2

> 围绕 Codex Desktop / Codex CLI 的本地控制台、配置管理器与 OpenCodex 能力复现层
> 状态：v0.2 基线 · 2026-06-25

---

## 1. 产品定位

**Codex Box** 是一个面向 OpenAI Codex 用户的本地桌面控制台。

它要做三件事：

1. **复现 OpenCodex 的核心使用价值**：让手机、平板、另一台电脑可以通过浏览器访问目标机器上的 Codex 工作流。
2. **保留 Codex Box 自有体验**：使用 Tauri + React 做原生桌面控制台，采用 Mac Dashboard + frosted glass 视觉，不复制 OpenCodex 的 Web shell UI。
3. **提供安全配置管理**：可视化管理 `~/.codex/config.toml`、profile、provider、gateway、诊断、备份和 diff。

一句话：

> Codex Box = OpenCodex 核心能力复现 + 桌面端控制台 + Codex 配置安全管理。

---

## 2. 目标用户

- 高频使用 `Codex CLI` / `Codex Desktop` 的开发者
- 想在手机、平板或另一台电脑上访问本机 Codex 的用户
- 需要在官方订阅、OpenAI API、OpenAI-compatible API、本地 gateway 间切换的用户
- 需要安全修改 `~/.codex/config.toml` 的用户
- 需要诊断 Codex Desktop、Codex CLI、gateway、provider、MCP、network 状态的用户

---

## 3. MVP 目标

v0.2 的 MVP 不再只是配置管理器，而是要跑通四条主路径：

1. **看得清**：Dashboard 结构化展示 Codex config、profile、provider、gateway、runtime 状态。
2. **改得稳**：所有 `~/.codex/config.toml` 写入必须 backup、diff、confirm、atomic write、rollback。
3. **连得上**：内置 gateway runtime 支持本机访问、LAN 访问、访问密码、health check、日志。
4. **查得快**：Diagnostics 能检查 config、runtime、port、auth、provider、MCP、network。

---

## 4. P0 功能范围

### Overview

- 当前 active profile、provider、model、network、MCP 摘要
- Gateway 运行状态、本机 URL、LAN URL、health 状态
- Codex Runtime 状态：Codex Desktop / Codex CLI / `CODEX_HOME`
- 最近错误、最近备份、快速入口

### Gateway

- 管理内置 gateway runtime 的启动、停止、重启
- 管理 host、port、local URL、LAN URL、runtime dir、log path
- 默认只监听 `127.0.0.1`
- LAN 模式必须先配置访问密码
- health endpoint、端口占用检测、日志查看
- 现阶段可以临时托管外部 OpenCodex checkout，但目标是移除硬编码外部依赖，收敛为 Codex Box 自有 runtime 模块

### Mobile Access

- 展示手机访问 URL、LAN IP、二维码
- 明确区分“候选 LAN 地址”和“已启用移动访问”
- 启用条件：gateway running + LAN host + password configured
- 安全提示：默认本机访问，不自动暴露局域网

### Codex Runtime

- 检测 Codex Desktop 是否安装
- 检测 Codex CLI 是否存在
- 检测 `CODEX_HOME` 是否可读
- 只读扫描官方资源；不默认解包、patch 或修改 Codex Desktop 内部文件
- 记录 runtime path、cache path、共享状态

### Profiles

- 展示 profile config
- 新建、编辑、删除 profile
- 设置 active / default profile
- profile 是模型切换入口，绑定 `model + model_provider + sandbox + approval + network + mcp_refs`
- 首次新增第三方 provider 时，必须保留官方订阅 profile，避免把官方订阅入口覆盖掉

### Providers

- 管理 `model_provider / model_providers`
- 支持：
  - OpenAI subscription（只做状态识别，不复用 token）
  - OpenAI official API
  - OpenAI-compatible API
  - local gateway
  - codex-proxy / CLIProxyAPI 预设
- 第三方 API 必须显式使用 `base_url + env secret` 引用
- secret 不明文展示、不写日志、不导出

### Config 管理

- 读取、解析、展示 `~/.codex/config.toml`
- 写入前生成结构化 diff 和文本 diff
- 写入前自动 backup
- atomic write 写回
- 写入失败 rollback 到最近一次 backup
- 回滚到历史备份

### Diagnostics

- Config 语法检查
- Codex Desktop 安装检测
- Codex CLI / `CODEX_HOME` 检测
- Gateway port / health / auth 检测
- Provider endpoint 检测
- MCP server command / path / env 检测
- Network direct / proxy 连通性检测
- 输出可复制的脱敏诊断报告

### Settings

- 语言、主题、启动行为
- gateway 默认 host、port、LAN 访问策略
- 访问密码配置
- 日志大小和保留策略
- 备份保留策略
- 实验功能开关

---

## 5. 暂缓范围

- 抓取、复用或转发官方账号 token
- 自动登录 / OAuth 接管 / 登录绕过
- 规避 rate limit / 账号配额限制
- 默认修改 Codex Desktop 内部文件
- 默认解包官方 `app.asar`
- 接管系统全局代理
- 团队同步 SaaS / 上传用户配置
- 无沙箱第三方插件系统

---

## 6. 核心页面

| 页面 | 说明 |
|---|---|
| `Overview` | 总览、当前状态、快速入口 |
| `Gateway` | 内置 gateway runtime 启停、端口、日志、health |
| `Mobile Access` | 手机访问 URL、LAN 状态、二维码、安全提示 |
| `Codex Runtime` | Codex Desktop / CLI / CODEX_HOME 检测 |
| `Profiles` | 工作配置管理 |
| `Providers` | 模型来源管理 |
| `Diagnostics` | 诊断与脱敏报告 |
| `Settings` | 应用设置 |

`MCP / Network / Config Diff` 暂时作为相关页面中的能力区块，不单独放入主导航；功能稳定后再拆。

---

## 7. 关键数据模型

- `CodexConfigSnapshot`
- `CodexProfile`
- `ModelProvider`
- `GatewayRuntime`
- `GatewayAccess`
- `CodexRuntimeStatus`
- `NetworkRoute`
- `McpServer`
- `BackupRecord`
- `HealthStatus`
- `SecretRef`
- `ConfigChangePreview`

详细字段定义见 [docs/data-model/v0.2.md](./docs/data-model/v0.2.md)。

---

## 8. 技术方向

- Desktop：`Tauri`
- Frontend：`React + TypeScript`
- UI：`Tailwind CSS + shadcn/ui`
- Backend：`Rust`
- Config：`toml` crate + 后续按写入需求引入保格式能力
- Diff：结构化 diff + 文本 diff
- Gateway：Codex Box 自有 runtime 模块，吸收 OpenCodex 的功能事实，不复制其 UI 和长段实现
- State：`zustand` + `@tanstack/react-query`
- 写入策略：backup first + diff confirm + atomic write + rollback

---

## 9. 里程碑

| 里程碑 | 范围 | 状态 |
|---|---|---|
| M0 | Tauri 读取/写入 TOML、backup、diff、atomic write | 已完成 |
| M1 | 只读 Dashboard / Overview | 已完成 |
| M2 | Provider / Profile MVP，写入闭环，共存迁移 | 部分完成 |
| M2.5 | OpenCodex 能力复现底座：gateway / auth / runtime locator / log / health | 进行中 |
| M3 | Gateway / Mobile Access / Codex Runtime 页面接真实 runtime | 待开始 |
| M4 | Diagnostics / Settings 接通真实检查与配置 | 待开始 |
| M5 | 桌面体验打磨：system tray、备份时间线、导入导出 | 待开始 |

---

## 10. 风险边界

### 绝对不做

- 不抓取任何账号 token
- 不绕过 OpenAI 官方登录
- 不规避 rate limit / 账号配额限制
- 不默认修改 Codex Desktop 内部文件
- 不接管系统全局代理
- 不上传任何用户配置
- 不复制 OpenCodex 的 UI、长段文案或源码实现

### OpenCodex 兼容边界

- OpenCodex 是 AGPL-3.0 项目；Codex Box 只吸收公开功能事实、运行方式、配置字段、health 行为和安全边界。
- 如直接复制或派生 OpenCodex 代码，必须先单独评估许可证义务并在 ADR 中确认。
- Codex Box 的实现应保持独立代码路径。

### 写入红线

- 任何 `~/.codex/config.toml` 写入必须先 backup
- 写入前必须展示 diff 并由用户确认
- 写入必须 atomic write（写 `.tmp` → `rename`）
- 写入失败必须 rollback 到最近一次 backup
- 并发写入必须校验 `content_hash`

### 隐私红线

- secret 字段（API key、token、password）永远不写日志
- UI 只展示 env key 名和脱敏显示
- 诊断报告默认脱敏
- 不上传任何用户配置到外部服务
