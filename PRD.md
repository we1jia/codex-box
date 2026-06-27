# Codex Box · PRD v0.3

> 让 Codex App 的模型下拉支持 BYOK:官方订阅、官方 API、第三方 OpenAI-compatible(含国产)、本地 gateway 进同一个 picker
> 状态:v0.3 基线 · 2026-06-25
> 替代:[PRD.v0.2-archived-20260625.md](./PRD.v0.2-archived-20260625.md)(v0.2 把项目定性为"复现 OpenCodex 的 Web 中间层",与 AITabby/opencodex 的真实定位不符)

---

## 1. 产品定位

**Codex Box** 是一个面向 OpenAI Codex 用户的本地桌面控制台,通过安全读写 `~/.codex/config.toml` 与内置 127.0.0.1 本地代理实现 **BYOK 模型下拉**。

它要做三件事:

1. **BYOK 模型下拉**:让 Codex App 的模型选择里同时出现 — 官方订阅、OpenAI 官方 API、第三方 OpenAI-compatible(含国产模型)、本地/中间层 gateway;切换时整组 `provider + model + reasoning` 一起切。**Codex App 真正能看到的合并 picker 由 Codex Box 内置的 127.0.0.1 代理 runtime 暴露**(`/v1/models`)。
2. **保留 Codex Box 自有体验**:使用 Tauri + React 做原生桌面控制台,采用 Mac Dashboard + frosted glass 视觉,不复用任何外部项目的 UI 实现。
3. **安全配置管理**:可视化管理 `~/.codex/config.toml`、`~/.codex/codex-box/inject-map.json`、profile、provider、备份、diff,写入严格走 backup → diff → confirm → atomic write → rollback。

一句话:

> Codex Box = BYOK 模型下拉控制台 + 127.0.0.1 本地代理 runtime + 桌面端 Codex 配置安全管理。

详细代理实现见 [`docs/architecture/v0.3.1-BYOK-proxy.md`](./docs/architecture/v0.3.1-BYOK-proxy.md) 与 [ADR-0006](./docs/decisions/0006-本地代理-runtime.md)。

---

## 2. 目标用户

- 高频使用 `Codex CLI` / `Codex Desktop` 的开发者
- 想在 Codex App 模型下拉里同时管理订阅、官方 API、第三方 API(含国产)、本地 gateway 的用户
- 想用统一 `~/.codex/config.toml` 视角管理多 profile、多 provider 的用户
- 需要安全修改 `~/.codex/config.toml` 的用户
- 同时使用 AITabby/opencodex、Codex++、CC Switch、Cockpit Tools 与 Codex Box 的用户(支持只读扫描与一键导入)

---

## 3. MVP 目标

v0.3 的 MVP 跑通四条主路径:

1. **看得清**:Dashboard 结构化展示 Codex config 状态、active profile、provider 列表(订阅 / 官方 / 第三方 / 本地)、Codex Box 主配置、Codex Desktop 安装情况。
2. **改得稳**:所有 `~/.codex/config.toml` 与 `~/.codex/codex-box/*.json` 写入走 backup → diff → confirm → atomic write → rollback,secret 走 env 引用,绝不写日志。
3. **加得动**:用户能在 UI 里新增 / 删除 / 启用 / 禁用 provider,新增 / 删除 / 切换 model,新增 / 删除 profile;每条改动都立即可回滚。
4. **查得快**:Diagnostics 能检查 config 语法、Codex Desktop 安装、Codex Box 模型目录完整性、第三方导入来源、provider URL 可达性、env 变量是否存在。

---

## 4. P0 功能范围

### 4.1 Models(模型下拉预览)

- 展示当前 Codex App 可见的模型列表(官方订阅、官方 API、OpenAI-compatible 第三方、本地 gateway)
- 每个模型显示:display name / 归属 provider / 可见性 toggle / 切换入口
- 切换模型 = 切到对应 profile 或修改 active profile 的 `model + model_provider`
- 切换前展示 diff 与回滚点

### 4.2 Provider Routes(多 provider 路由)

- 维护 `~/.codex/codex-box/providers.json`(Codex Box 主配置)
- 支持:
  - 官方订阅(`codex-subscription`):只做状态识别,绝不读 token
  - 官方 API(`openai-official-api`):`https://api.openai.com/v1` + `OPENAI_API_KEY`
  - OpenAI-compatible(`compatible-api`):任意 base_url + wire_api + env 引用 key
  - 本地 gateway(`local-gateway`):Codex Box 自有 runtime 或用户显式配置 endpoint
- "新增 provider"会写 `~/.codex/codex-box/providers.json` + 在 `~/.codex/config.toml` 的 `[model_providers.*]` 段加表

### 4.3 Custom Model Catalog(自定义模型目录)

- 维护 `~/.codex/codex-box/custom_model_catalog.json`(Codex Box 主模型目录)
- 用户在 Codex Box 里"加模型"会落在这份 JSON
- 拼装进 `~/.codex/config.toml` 的 `[model_providers.*].models` 字段

### 4.4 Profiles(工作配置)

- 展示 profile 列表(每个 profile 绑定 `model + model_provider + reasoning + sandbox + approval + network + mcp_refs`)
- 新建 / 编辑 / 删除 profile
- 设置 active / default profile
- 切换 profile 即切换整套配置
- 首次新增第三方 provider 时,必须保留官方订阅 profile,避免把官方入口覆盖掉

### 4.5 Codex Runtime(Codex Desktop 检测,只读)

- 检测 Codex Desktop 是否安装、版本、二进制路径
- 检测 Codex CLI 是否存在
- 检测 `CODEX_HOME` 与 `~/.codex/config.toml` 可读性
- **不**修改 Codex Desktop 内部文件,**不**解包 `app.asar`,**不** patch IPC

### 4.6 Config 管理

- 读取、解析、展示 `~/.codex/config.toml`
- 写入前生成结构化 diff 和文本 diff
- 写入前自动 backup
- atomic write 写回
- 写入失败 rollback 到最近一次 backup
- 支持回滚到历史 backup

### 4.7 Diagnostics

- Config 语法检查
- Codex Desktop 安装检测(只读)
- `~/.codex/codex-box/*.json` 完整性与 `~/.opencodex/*.json` 兼容导入检测
- provider URL 可达性
- env 变量存在性
- 输出可复制的脱敏诊断报告

### 4.8 Settings

- 语言、主题、启动行为
- 写入策略(默认强制 backup + diff + confirm)
- secret 脱敏(默认开启)
- 备份保留策略
- 日志大小和保留策略
- 第三方配置导入(OpenCodex / Codex++ / CC Switch / Cockpit Tools / Codex 备份)
- 实验功能开关

---

## 5. 暂缓范围(红线)

- ❌ 抓取、复用或转发官方账号 token
- ❌ 自动登录 / OAuth 接管 / 登录绕过
- ❌ 规避 rate limit / 账号配额限制
- ❌ 默认修改 Codex Desktop 内部文件(`app.asar` / renderer / IPC)
- ❌ 默认解包 `app.asar`
- ❌ 接管系统全局代理
- ❌ 团队同步 SaaS / 上传用户配置
- ❌ 复制 AITabby/opencodex 源码、UI、长段文案
- ❌ spawn 外部 AITabby/opencodex 进程(走独立 runtime 不走外部 launcher)
- ❌ 引入无沙箱第三方插件系统

**v0.3.1 起允许**:

- ✅ 内置 127.0.0.1 本地 HTTP 代理(`/v1/models`、`/v1/chat/completions`、`/v1/responses`、`/healthz`)
- ✅ 代理**仅**监听 127.0.0.1,**不**绑 LAN / IPv6 / Unix socket
- ✅ 代理**不**读取 `HTTPS_PROXY` / `ALL_PROXY` 等系统代理环境变量
- ✅ 鉴权走 `${ENV_VAR}` 引用,按需懒读,绝不落盘或上日志
- ✅ Codex App 端看到的"统一 picker"由代理提供,Codex App 内部 picker 协议不需理解

---

## 6. 核心页面

| 页面 | 说明 |
|---|---|
| `Overview` | 总览、当前状态、快速入口 |
| `Models` | 模型下拉预览 + 切换 |
| `Provider Routes` | 多 provider 路由 + 新增 / 编辑 / 启用 / 禁用 |
| `Codex Runtime` | Codex Desktop / CLI / CODEX_HOME 检测(只读) |
| `Profiles` | 工作配置管理 |
| `Custom Model Catalog` | 自定义模型目录管理(可选内嵌) |
| `Diagnostics` | 诊断与脱敏报告 |
| `Settings` | 应用设置 |

`Config Diff` 作为相关页面中的能力区块,不单独放入主导航;功能稳定后再拆。

---

## 7. 关键数据模型

- `CodexConfigSnapshot`
- `CodexProfile`
- `ModelProvider`
- `ModelCatalogEntry`(自定义模型目录条目)
- `ProviderRoute`(`~/.codex/codex-box/providers.json` 条目)
- `CodexBoxCustomConfig`(`~/.codex/codex-box/custom_model_catalog.json` 完整快照)
- `CodexRuntimeStatus`
- `BackupRecord`
- `HealthStatus`
- `SecretRef`
- `ConfigChangePreview`

详细字段定义见 [docs/data-model/v0.3-BYOK.md](./docs/data-model/v0.3-BYOK.md)。

---

## 8. 技术方向

- Desktop:`Tauri`
- Frontend:`React + TypeScript`
- UI:`Tailwind CSS + shadcn/ui`
- Backend:`Rust`
- Config 解析:`toml` crate + 自写保格式层
- JSON 解析:`serde_json`(用于 `~/.codex/codex-box/*.json` 与第三方导入来源)
- Diff:结构化 diff + 文本 diff(`similar` crate)
- 写入策略:backup first + diff confirm + atomic write + rollback
- State:`zustand` + `@tanstack/react-query`
- 绝不引入 Redux / MobX / 任何 CSS-in-JS / 任何 ORM / 任何 token 持久化方案

---

## 9. 里程碑

| 里程碑 | 范围 | 状态 |
|---|---|---|
| M0 | Tauri 读取/写入 TOML、backup、diff、atomic write | 已完成 |
| M1 | 只读 Dashboard / Overview | 已完成 |
| M2 | Provider / Profile MVP,写入闭环,共存迁移 | 已完成 |
| M2.5 | **BYOK 模型目录与多 provider 路由底座** | 已完成 |
| M2.6 | **本地代理 runtime 落地**(`src-tauri/src/proxy/*` + Tauri commands + UI 升级) | 已完成 |
| M3 | Models / Provider Routes / Codex Runtime 页面接真实配置 | 待开始 |
| M4 | Diagnostics / Settings 接通真实检查与配置 | 待开始 |
| M5 | 桌面体验打磨:system tray、备份时间线、导入导出 | 待开始 |

---

## 10. 风险边界

### 绝对不做

- 不抓取任何账号 token
- 不绕过 OpenAI 官方登录
- 不规避 rate limit / 账号配额限制
- **不**默认修改 Codex Desktop 内部文件(`app.asar`、renderer、IPC)
- **不**接管系统全局代理
- 不上传任何用户配置
- 不复制 AITabby/opencodex 源码、UI、长段文案

### 第三方配置兼容边界

- Codex Box 使用 `~/.codex/codex-box/*.json` 作为主写入目录。
- `~/.opencodex/*.json` 仅作为 AITabby/opencodex 只读扫描与一键导入来源,不参与 Codex Box 实时模型列表、代理路由或 API 服务链路兜底。
- Codex++、CC Switch、Cockpit Tools 作为只读扫描和导入来源,不依赖其内部私有数据库格式。
- Codex Box 不 spawn AITabby/opencodex 进程,不复用其源码,实现完全独立。
- 详细事实见 [docs/references/aitabby-opencodex.md](./docs/references/aitabby-opencodex.md)。

### 写入红线

- 任何 `~/.codex/config.toml` 写入必须先 backup
- 写入前必须展示 diff 并由用户确认
- 写入必须 atomic write(写 `.tmp` → `rename`)
- 写入失败必须 rollback 到最近一次 backup
- 并发写入必须校验 `content_hash`
- 任何 `~/.codex/codex-box/*.json` 写入走相同闭环
- 任何第三方目录导入默认只读扫描,写入目标只能是 Codex Box 主目录

### 隐私红线

- secret 字段(API key、token、password)永远不写日志
- UI 只展示 env key 名和脱敏显示
- 诊断报告默认脱敏
- 不上传任何用户配置到外部服务

---

## 版本

- v0.3 · 2026-06-25 · BYOK 模型下拉与多 provider 路由主线收敛
