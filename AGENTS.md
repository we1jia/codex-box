# AGENTS.md · Codex Box

> 给所有 AI 代理（Claude / Codex / 其他）的工作规范
> 最后更新：2026-06-25

---

## 全局规则

### 语言
- **默认中文回复**，技术名词保留原文
- 代码、命令、路径、配置键名保持英文
- 代码注释、提交信息、文档、issue 描述默认中文

### 沟通
- 结论优先，再展开细节
- 复杂问题先给摘要，再按需展开
- 不要为了简短省略关键前提、风险
- 不要写低信息密度的内容

---

## 项目边界

### 这是什么
Codex Box 是面向 OpenAI Codex 的**本地桌面控制台、配置管理器与 OpenCodex 能力复现层**。

- 技术栈：`Tauri + React + TypeScript + Tailwind + shadcn/ui + Rust`
- 数据源：`~/.codex/config.toml`（主）、`~/.codex/codex-box/`（自有）
- 风格：现代 Mac Dashboard + frosted glass
- 主线：复现 OpenCodex 的 gateway / mobile access / runtime 管理核心能力，但保持独立代码路径和自有 UI

### 绝对不做（红线）
- ❌ 抓取任何账号 token
- ❌ 绕过 OpenAI 官方登录
- ❌ 规避 rate limit / 账号配额限制
- ❌ 默认修改 Codex Desktop 内部文件
- ❌ 接管系统全局代理
- ❌ 团队同步 SaaS / 上传任何用户配置

详见 [PRD.md §12](./PRD.md)。

### 写入红线（硬规则）
- ✅ 任何 `~/.codex/config.toml` 写入前**必须**先 backup
- ✅ 写入必须 atomic write（`tmp` → `rename`）
- ✅ 写入前**必须**展示 diff
- ✅ 写入失败**必须**能 rollback 到最近一次 backup
- ✅ secret 字段（API key / token）**永远不写日志**

---

## 文档与代码

### 文档位置
```
PRD.md                              # 产品需求
docs/architecture/v0.2.md           # 整体架构、页面结构、数据流
docs/design/v0.1.md                 # UI 设计规范 + 线框图
docs/data-model/v0.2.md             # 数据模型
docs/decisions/                     # 关键决策日志（ADR）
docs/references/                    # 第三方项目技术事实与参考资料
AGENTS.md                           # 本文件
```

### 修改文档前先读
任何修改 PRD / 数据模型 / 设计规范前，**必须先 Read** 完整文件，再 Edit。

### 修改代码前先读
任何修改代码前，**必须先 Read** 当前实现，理解上下文再动刀。

---

## 技术栈约束

| 层 | 选型 | 理由 |
|---|---|---|
| Desktop 框架 | Tauri | 体积小、性能好、Rust 后端 |
| 前端 | React + TypeScript | 生态成熟、类型安全 |
| UI 库 | Tailwind + shadcn/ui | 原子化、可定制 |
| 后端 | Rust | 与 Tauri 一致，性能、内存安全 |
| TOML 解析 | `toml` crate | 官方维护、serde 集成 |
| Diff | `similar` crate | 高质量文本 diff |
| 异步 | tokio | Tauri 默认运行时 |
| 状态管理 | zustand | 轻量、TS 友好 |
| 数据请求 | @tanstack/react-query | 与 Tauri invoke 集成好 |

**不要引入**：
- ❌ 任何重状态管理库（Redux / MobX）
- ❌ 任何 CSS-in-JS（用 Tailwind）
- ❌ 任何 ORM（直接 toml crate + 手写映射）
- ❌ 任何 token 持久化方案（secret 走环境变量引用）

---

## 目录结构约定

```
codex-box/
├─ src-tauri/                  # Rust 后端
│  ├─ src/
│  │  ├─ config/               # config 读写
│  │  │  ├─ loader.rs
│  │  │  ├─ parser.rs
│  │  │  ├─ backup.rs
│  │  │  ├─ diff.rs
│  │  │  └─ writer.rs
│  │  ├─ health/               # 诊断检查
│  │  ├─ secret/               # 凭据引用
│  │  └─ lib.rs
│  └─ Cargo.toml
├─ src/                        # React 前端
│  ├─ components/              # 通用组件
│  ├─ pages/                   # 路由页面
│  │  ├─ Dashboard.tsx
│  │  ├─ Gateway.tsx
│  │  ├─ MobileAccess.tsx
│  │  ├─ CodexRuntime.tsx
│  │  ├─ Profiles.tsx
│  │  ├─ Providers.tsx
│  │  ├─ Diagnostics.tsx
│  │  └─ Settings.tsx
│  ├─ store/                   # zustand stores
│  ├─ lib/                     # 工具、API 封装
│  └─ App.tsx
├─ docs/                       # 文档
├─ PRD.md
└─ AGENTS.md
```

---

## 提交规范（gitflow）

### 分支命名
- `main` — 主分支
- `develop` — 开发分支
- `feature/*` — 新功能
- `fix/*` — 修复
- `chore/*` — 杂项（依赖、配置）
- `docs/*` — 文档

### 提交信息格式
```
<type>: <中文描述>

<可选 body，说明为什么、怎么改>

Co-Authored-By: Claude Opus 4.8 <noreply@anthropic.com>
```

type 取值：
- `feat` — 新功能
- `fix` — 修复
- `refactor` — 重构
- `chore` — 杂项
- `docs` — 文档
- `style` — 格式
- `test` — 测试
- `perf` — 性能

### 示例
```
feat: 接入 config.toml 只读解析

- 添加 src-tauri/src/config/loader.rs
- 使用 toml crate 解析并返回 CodexConfigSnapshot
- 前端 Dashboard 展示 active profile、provider 数量

Co-Authored-By: Claude Opus 4.8 <noreply@anthropic.com>
```

---

## AI 行为准则

### 接到 Codex Box 任务时
1. 先 Read `PRD.md` 理解产品
2. 再 Read `docs/architecture/v0.2.md` 理解整体架构、页面结构和数据流
3. 再 Read `docs/design/v0.1.md` 理解 UI 规范
4. 再 Read `docs/data-model/v0.2.md` 理解数据模型
5. 再读 `docs/decisions/` 最近的 ADR 理解决策历史
6. 最后再动手

### 写代码前
- 列出要改的文件
- 说明改动的目的
- 给出可执行的步骤

### 写代码后
- 跑测试（如果存在）
- 跑 lint
- 更新文档（如有字段、API 变更）

### 遇到模糊需求时
- **不要**自行扩展 PRD / 设计规范
- **应该**先与用户对齐，再动手
- 把决策写到 `docs/decisions/0001-xxx.md`

### 遇到红线场景时
- **拒绝**执行
- **解释**为什么
- **建议**合法替代方案

---

## 当前里程碑

- **M0**：技术验证 — Tauri 读取/写入 TOML、备份、diff、atomic write
- **M1**：只读 Dashboard
- **M2**：Provider / Profile MVP，写入闭环，共存迁移
- **M2.5**：OpenCodex 能力复现底座 — gateway / auth / runtime locator / log / health
- **M3**：Gateway / Mobile Access / Codex Runtime 页面接真实 runtime
- **M4**：Diagnostics / Settings 接通真实检查与配置
- **M5**：桌面体验打磨 — system tray、备份时间线、导入导出

详见 [PRD.md §9](./PRD.md)。

---

## 联系方式 / 上游文档

- 原始 PRD：见 [PRD.md](./PRD.md)
- 架构方案：见 [docs/architecture/v0.2.md](./docs/architecture/v0.2.md)
- UI 设计稿：`docs/design/v0.1.md` 含线框图
- 数据模型：[docs/data-model/v0.2.md](./docs/data-model/v0.2.md)
- 决策日志：`docs/decisions/`

---

## 版本

- v0.2 · 2026-06-25 · OpenCodex 能力复现路线收敛
