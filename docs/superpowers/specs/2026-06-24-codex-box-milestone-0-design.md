# Codex Box · 里程碑 0 设计

> **Topic**: Codex Box 里程碑 0 — Tauri 工程 + config 读写主路径打通
> **Status**: Approved
> **Date**: 2026-06-24
> **Author**: brainstorming session

---

## 1. 背景与目标

Codex Box 是面向 OpenAI Codex 的本地配置与网关管理器，PRD 已固化在 [PRD.md](../../../PRD.md)。

**里程碑 0 的目标**：跑通主路径

```
读 ~/.codex/config.toml
  → TOML 解析（保留原文件注释、字段顺序、格式）
  → 结构化展示给前端
  → Dashboard 看到 4 张真实指标卡
```

**M0 不包含**：编辑、回滚、健康检查、备份列表、profile/provider 完整 CRUD。这些属于 M1~M4。

**M0 的成功标准**（来自与用户确认）：
- `cargo test` 全部 pass
- `cargo build --release` 成功
- `pnpm tauri dev` 启动后 Dashboard 4 张指标卡显示真实数据
- 改了 `~/.codex/config.toml` 重启 app 数据更新

---

## 2. 范围 / 非范围

### 2.1 在范围内（M0）

| 类别 | 内容 |
|---|---|
| Rust 模块 | `loader.rs` / `parser.rs` / `backup.rs` / `writer.rs` / `diff.rs` / `error.rs` / `secret.rs` |
| Rust 测试 | `cargo test` 覆盖上述模块，3 个核心模块严格 TDD |
| Tauri 命令 | `dashboard_summary()` 返回真实结构化数据 |
| 前端 | App.tsx / Sidebar.tsx / Dashboard.tsx + 4 张指标卡 |
| 工程 | `pnpm install` 成功 + `pnpm tauri dev` 起来 |
| 版本控制 | git init + worktree + 中文 commit |

### 2.2 不在范围内（M0）

- ❌ 编辑 config.toml（写流程脚手架留接口，不实现）
- ❌ 健康检查的 8 项检查（`health.rs` 留 trait 占位）
- ❌ 备份列表 UI
- ❌ 回滚
- ❌ Config Diff 页面
- ❌ 暗色模式
- ❌ Provider / MCP / Network 完整页面

---

## 3. 技术决策

### 3.1 配置读取策略

M0 用 `toml = "0.8"`（`toml::Value`）做结构化解析，**不引入 toml_edit**。

理由：
- M0 只读不写，不需要保留注释/字段顺序
- `toml::Value` 配合 `serde_json::Value` 够用，结构清晰
- 降低依赖复杂度

M2 引入写入路径时再切 `toml_edit::DocumentMut` 保留原文件注释。

### 3.2 测试框架：标准库 + tempfile

理由：cargo 自带测试运行器，tempfile crate 是社区事实标准。

### 3.3 TDD 粒度：3 模块严格 TDD

`backup.rs` / `writer.rs` / `diff.rs` 三个写入相关模块严格 RED-GREEN-REFACTOR。
其他模块写单测不严格 RED。

理由：
- 写入是核心风险点
- 其他模块逻辑简单，验证式单测足够
- 严格 TDD × 5 个模块工作量 ×2

### 3.4 worktree

在 `.claude/worktrees/codex-box-m0/` 跳，主仓库保留干净。

每完成一个模块一个 commit，最终不 squash。

### 3.5 commit 语言：中文

遵守 AGENTS.md。

### 3.6 错误处理：thiserror + 前端 try/catch

Rust 用 `thiserror` 定义 `AppError`，跨边界时序列化为字符串。
前端 `lib/api.ts` 包成 `ApiResult<T> = { ok, data? , error? }`。

### 3.7 工具链版本

- Rust 1.80+（已要求用户升级）
- Node 22.21.1（已就绪）
- pnpm 10.6.0（已就绪）
- Tauri 2.x

---

## 4. 架构

### 4.1 模块拓扑

```
src-tauri/src/
├── main.rs              # 入口，调用 lib::run()
├── lib.rs               # Tauri builder + commands 注册
├── error.rs             # AppError + Serialize for Tauri
├── secret.rs            # redact() 字符串脱敏
├── config/
│   ├── mod.rs           # re-export
│   ├── loader.rs        # IO: 读 DocumentMut
│   ├── parser.rs        # DocumentMut ↔ CodexConfigSnapshot
│   ├── backup.rs        # 文件复制 + 命名
│   ├── writer.rs        # atomic write
│   └── diff.rs          # 行级 text diff
├── health/
│   └── mod.rs           # DiagnosticCheck trait + 占位 check
└── snapshot/
    └── mod.rs           # CodexConfigSnapshot + 子类型定义

src/
├── main.tsx             # ReactDOM.createRoot
├── App.tsx              # Sidebar + 路由占位
├── components/
│   ├── Sidebar.tsx      # 独立悬浮玻璃侧栏
│   ├── FloatingCard.tsx # 分层卡片基类
│   └── MetricCard.tsx   # 4 张指标卡
├── pages/
│   └── Dashboard.tsx    # 主页面
├── store/
│   └── dashboard.ts     # zustand store
└── lib/
    └── api.ts           # invokeCmd<T> wrapper
```

### 4.2 职责边界

- `loader` 只 IO，不解析结构
- `parser` 只做 `DocumentMut ↔ Snapshot` 双向转换
- `backup` 只复制 + 命名
- `writer` 只 atomic write，不感知 backup
- `diff` 只 text diff
- `health` 跟 config 解耦（trait + 实现）
- `secret` 全局 redaction
- `error` 跨边界类型

---

## 5. 数据契约

### 5.1 前端 ↔ Rust 命令

```ts
// lib/api.ts
type ApiResult<T> =
  | { ok: true; data: T }
  | { ok: false; error: string };

// store/dashboard.ts 用的类型
interface DashboardSummary {
  activeProfile: string | null;
  providerCount: number;
  mcpCount: { enabled: number; total: number };
  network: string;
  lastBackupAt: string | null;
  healthSummary: { ok: number; warn: number; fail: number };
}
```

### 5.2 Rust 核心类型（snapshot/mod.rs）

```rust
pub struct CodexConfigSnapshot {
    pub path: PathBuf,
    pub read_at: DateTime<Utc>,
    pub raw_text: String,
    pub parsed: ParsedConfig,
    pub content_hash: String,
    pub valid: bool,
    pub parse_errors: Vec<ParseError>,
}

pub struct ParsedConfig {
    pub active_profile: Option<String>,
    pub profiles: Vec<CodexProfile>,
    pub model_providers: Vec<ModelProvider>,
    pub mcp_servers: Vec<McpServer>,
    pub default_network: Option<String>,
    pub network_routes: Vec<NetworkRoute>,
    pub top_level: HashMap<String, toml::Value>,
}
```

详细字段见 [data-model/v0.1.md](../../data-model/v0.1.md)。

---

## 6. 主路径

### 6.1 启动后

```
[Frontend]
  Dashboard.tsx useEffect(() => loadDashboard(), [])
    → invokeCmd<DashboardSummary>('dashboard_summary')

[Rust command: dashboard_summary]
  loader::load_doc(~/.codex/config.toml) → DocumentMut
  parser::to_snapshot(doc) → CodexConfigSnapshot
  parser::to_dashboard_summary(&snapshot) → DashboardSummary
  → return to frontend

[Frontend]
  store.setSummary(data)
  Dashboard.tsx 渲染 4 张指标卡
```

### 6.2 编辑流程（M0 不实现，此处仅说明未来 M2 接口预留）

> M0 阶段不实现写入路径。本节列出 M2 将要实现的接口，作为后续模块的契约参考，**不要求 M0 完成**。

```
update_profile(name, fields):
  1. loader::load_doc → DocumentMut v1
  2. backup::create(v1.raw_text, PreWrite) → BackupRecord
  3. parser::mutate_profile(doc, name, fields)
  4. diff::between(v1.raw_text, doc.to_string()) → String
  5. writer::atomic_write(doc.raw_text, path) → Result<()>
  6. return { backup_id, diff }
```

---

## 7. 测试策略

### 7.1 严格 TDD（3 模块）

**backup.rs / writer.rs / diff.rs**

每个 fn：
1. RED：写 `#[test] fn xxx() { ... }`，cargo test，确认失败
2. GREEN：写最小实现让测试过
3. REFACTOR：保持 green 重构

### 7.2 单测补齐（其他模块）

loader / parser / health / secret / error 写测试但不必严格 RED。

### 7.3 Fixture 文件

```
src-tauri/tests/fixtures/
├── minimal.toml          # 只有顶层字段
├── with_mcp.toml         # 含 [mcp_servers.xxx]
└── with_marketplace.toml # 含 [marketplaces.xxx]
```

### 7.4 验证清单

```bash
# 后端
cd src-tauri
cargo test
cargo build --release
cargo clippy -- -D warnings

# 前端
cd ..
pnpm install
pnpm build
pnpm tauri dev  # 起来看 Dashboard

# 端到端冒烟
# - Dashboard 4 张卡有真实数据
# - 关闭再开仍然正确
# - 改 ~/.codex/config.toml 重启后数据更新
```

---

## 8. UI 范围（M0）

### 8.1 必须

- App.tsx：Sidebar + Dashboard
- Sidebar.tsx：8 项主导航 + 分组 + 底部 + frosted glass 视觉
- Dashboard.tsx：
  - 欢迎卡 "晚安，开发者" + 当前时间
  - 4 张指标卡：active profile / provider / network / mcp
  - "最近活动"时间线（hardcoded mock）

### 8.2 不做

- ❌ "配置健康"卡片（M1）
- ❌ "启动设置"卡片（M1）
- ❌ Config Diff 页面（M2）
- ❌ Profiles/Providers/Network/MCP 完整页面（M2/M3/M4）
- ❌ 暗色模式（M6）
- ❌ Settings 页面（M6）

### 8.3 错误态

- Dashboard 顶部 toast 提示错误
- 4 张卡显示 "—"，文案 "暂无数据"

---

## 9. 风险与缓解

| 风险 | 缓解 |
|---|---|
| rustc 1.68 < Tauri 2 要求 1.77 | 用户已确认升级 rustc 到 1.80+ |
| `toml_edit` API 复杂 | 严格 TDD，每个 fn 单独验证 |
| `~/.codex/config.toml` 真实存在但权限 600 | loader 处理权限错误，给清晰错误信息 |
| 启动后读真实 config，破坏用户体验 | 错误态优雅：toast + 占位卡，不崩 |
| 改 ~/.codex/config.toml 的副作用 | M0 只读，不写。完全无副作用。 |

---

## 10. 完成定义 (DoD)

- [ ] git init + worktree 跳起来
- [ ] rustc 已升级到 1.80+
- [ ] `cd src-tauri && cargo test` 全部 pass
- [ ] `cargo build --release` 成功
- [ ] `cargo clippy -- -D warnings` 无 warning
- [ ] `pnpm install` 成功
- [ ] `pnpm tauri dev` 启动无报错
- [ ] Dashboard 4 张指标卡显示真实数据（来自真实 `~/.codex/config.toml`）
- [ ] Sidebar 按 design v0.1 视觉实现
- [ ] 所有 commit message 中文，遵守 AGENTS.md
- [ ] 没有任何 secret 进入 git history
- [ ] 文档齐：README.md 包含运行步骤

---

## 11. 后续步骤

M0 完成后进入 M1：只读 Dashboard 扩展（配置健康、启动设置、最近活动真实化）。
M1 完成后进入 M2：Profile + Provider MVP（编辑、备份、diff、atomic write 真正启用）。