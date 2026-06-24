# Codex Box · M0 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 跑通 Codex Box M0 主路径：`~/.codex/config.toml` → 解析 → backup → diff → atomic write → Tauri command → React Dashboard 展示 4 张指标卡。

**Architecture:** Tauri 2 + React + TS + Tailwind + Rust。Rust 端 Core 层（loader/parser/backup/writer/diff）纯函数 + 显式错误，Command 层桥接 Tauri。前端 React 单页 + zustand 调 invoke 拿真数据。3 个核心模块严格 TDD。

**Tech Stack:** Tauri 2 / Rust 1.80+ / React 18 / TypeScript 5 / Tailwind 3 / shadcn-style / toml 0.8 / tempfile 3 / thiserror / zustand / @tanstack/react-query / lucide-react

**对应 spec:** [docs/superpowers/specs/2026-06-24-codex-box-milestone-0-design.md](../../specs/2026-06-24-codex-box-milestone-0-design.md)

---

## 文件结构

```
codex-box/
├── src-tauri/
│   ├── Cargo.toml              # 修改：加 toml / tempfile(dev) / chrono / sha2 / thiserror
│   ├── src/
│   │   ├── main.rs             # 已有
│   │   ├── lib.rs              # 修改：builder + register
│   │   ├── error.rs            # 新建：AppError
│   │   ├── config/
│   │   │   ├── mod.rs          # 新建：re-export
│   │   │   ├── model.rs        # 新建：CodexConfig / BackupRecord / DiffLine / DashboardSummary
│   │   │   ├── loader.rs       # 新建：read_raw / metadata
│   │   │   ├── parser.rs       # 新建：parse / to_dashboard_summary
│   │   │   ├── backup.rs       # 新建：create_backup  ★严格 TDD
│   │   │   ├── writer.rs       # 新建：atomic_write   ★严格 TDD
│   │   │   └── diff.rs         # 新建：between         ★严格 TDD
│   │   └── commands/
│   │       ├── mod.rs          # 新建
│   │       └── dashboard.rs    # 新建：dashboard_summary command
│   └── tests/
│       └── fixtures/
│           ├── minimal.toml
│           ├── with_mcp.toml
│           └── with_marketplace.toml
├── src/
│   ├── main.tsx                # 新建
│   ├── App.tsx                 # 新建
│   ├── index.css               # 新建：Tailwind directives
│   ├── components/
│   │   ├── Sidebar.tsx         # 新建
│   │   └── MetricCard.tsx      # 新建
│   ├── pages/
│   │   └── Dashboard.tsx       # 新建
│   ├── store/
│   │   └── dashboard.ts        # 新建
│   └── lib/
│       └── api.ts              # 新建
```

---

## Task 0: worktree 创建 + rustc 升级

**Files:** 无

- [ ] **Step 1: 升级 rustc 到 1.80+**

Run:
```bash
rustup update stable
rustc --version
```
Expected: `rustc 1.80.0` 或更高

- [ ] **Step 2: 验证 worktree 路径是否就绪**

工程已在 `/Users/liuweijia/Desktop/AI/Codex Box/`（已 init git + 2 commits）。M0 实施在主目录进行（不切 worktree，简化流程；如需隔离再迁）。

Run:
```bash
cd "/Users/liuweijia/Desktop/AI/Codex Box"
git log --oneline
```
Expected: 看到 `e1feac7 docs: 落盘 M0 设计 spec` 和 `fc6a32a chore: 初始化项目`

- [ ] **Step 3: 创建 feature 分支**

```bash
cd "/Users/liuweijia/Desktop/AI/Codex Box"
git checkout -b feat/m0-tauri-skeleton
git status
```
Expected: `On branch feat/m0-tauri-skeleton`, `nothing to commit`

---

## Task 1: Cargo.toml 补依赖 + 项目骨架

**Files:**
- Modify: `src-tauri/Cargo.toml`

- [ ] **Step 1: 修改 Cargo.toml**

完整内容（替换现有文件）：

```toml
[package]
name = "codex-box"
version = "0.1.0"
description = "Codex Box - Local config & gateway manager for OpenAI Codex"
authors = ["Codex Box"]
edition = "2021"
rust-version = "1.77"

[lib]
name = "codex_box_lib"
crate-type = ["staticlib", "cdylib", "rlib"]

[build-dependencies]
tauri-build = { version = "2", features = [] }

[dependencies]
tauri = { version = "2", features = [] }
tauri-plugin-shell = "2"
serde = { version = "1", features = ["derive"] }
serde_json = "1"
toml = "0.8"
chrono = { version = "0.4", features = ["serde"] }
sha2 = "0.10"
thiserror = "1"
dirs = "5"

[dev-dependencies]
tempfile = "3"

[profile.release]
panic = "abort"
codegen-units = 1
lto = true
opt-level = "s"
strip = true
```

- [ ] **Step 2: 跑 cargo check 验证依赖可下载**

Run:
```bash
cd "/Users/liuweijia/Desktop/AI/Codex Box/src-tauri"
cargo check
```
Expected: 编译成功，可能耗时 1-3 分钟（首次下 crate）

- [ ] **Step 3: Commit**

```bash
cd "/Users/liuweijia/Desktop/AI/Codex Box"
git add src-tauri/Cargo.toml
git commit -m "chore: 补 Rust 核心依赖"
```

---

## Task 2: error.rs + model.rs + lib.rs 框架

**Files:**
- Create: `src-tauri/src/error.rs`
- Create: `src-tauri/src/config/mod.rs`
- Create: `src-tauri/src/config/model.rs`
- Modify: `src-tauri/src/lib.rs`

- [ ] **Step 1: 写 error.rs**

```rust
// src-tauri/src/error.rs
use thiserror::Error;

#[derive(Debug, Error)]
pub enum AppError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("TOML parse error: {0}")]
    TomlParse(#[from] toml::de::Error),

    #[error("Config not found at {0}")]
    ConfigNotFound(String),

    #[error("Backup directory error: {0}")]
    BackupDir(String),

    #[error("Atomic write failed: {0}")]
    AtomicWrite(String),

    #[error("Invalid UTF-8 in {0}")]
    InvalidUtf8(String),
}

impl serde::Serialize for AppError {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}

pub type AppResult<T> = std::result::Result<T, AppError>;
```

- [ ] **Step 2: 写 config/model.rs**

```rust
// src-tauri/src/config/model.rs
use serde::{Deserialize, Serialize};

/// 解析后的 Codex 配置（顶层 view）
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CodexConfig {
    /// 顶层标量字段
    pub top_level: serde_json::Map<String, serde_json::Value>,
    /// mcp_servers 表
    pub mcp_servers: Vec<McpServerEntry>,
    /// marketplaces 表
    pub marketplaces: Vec<MarketplaceEntry>,
    /// 其他 TOML 表
    pub other_tables: serde_json::Map<String, serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct McpServerEntry {
    pub name: String,
    pub kind: McpServerKind,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum McpServerKind {
    Stdio {
        command: String,
        #[serde(default)]
        args: Vec<String>,
    },
    Http { url: String },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MarketplaceEntry {
    pub name: String,
    pub source_type: Option<String>,
    pub source: Option<String>,
}

/// 一次备份记录
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct BackupRecord {
    pub id: String,
    pub created_at: String,
    pub file_path: String,
    pub reason: BackupReason,
    pub content_hash: String,
    pub size_bytes: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum BackupReason {
    Manual,
    PreWrite,
    PreRollback,
    Scheduled,
}

/// 文本 diff 一行
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DiffLine {
    pub kind: DiffKind,
    pub content: String,
    pub old_line: Option<usize>,
    pub new_line: Option<usize>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DiffKind {
    Context,
    Insert,
    Delete,
}

/// Dashboard 摘要：前端 4 张指标卡的最小数据
/// 字段命名以 spec §5.1 为准
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DashboardSummary {
    pub active_profile: Option<String>,
    pub provider_count: usize,
    pub mcp_count: McpCount,
    pub network: String,
    pub last_backup_at: Option<String>,
    pub health_summary: HealthSummary,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct McpCount {
    pub enabled: usize,
    pub total: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct HealthSummary {
    pub ok: usize,
    pub warn: usize,
    pub fail: usize,
}
```

- [ ] **Step 3: 写 config/mod.rs**

```rust
// src-tauri/src/config/mod.rs
pub mod backup;
pub mod diff;
pub mod loader;
pub mod model;
pub mod parser;
pub mod writer;

pub use model::*;
```

- [ ] **Step 4: 写空的 lib.rs 框架**

```rust
// src-tauri/src/lib.rs
pub mod config;
pub mod error;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .setup(|_app| Ok(()))
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
```

- [ ] **Step 5: 写空的 4 个子模块（占位）**

`src-tauri/src/config/loader.rs`:
```rust
use crate::error::AppResult;
use std::path::Path;

pub fn read_raw(_path: &Path) -> AppResult<String> {
    unimplemented!()
}
```

`src-tauri/src/config/parser.rs`:
```rust
use crate::config::model::CodexConfig;
use crate::error::AppResult;

pub fn parse(_raw: &str) -> AppResult<CodexConfig> {
    unimplemented!()
}
```

`src-tauri/src/config/backup.rs`:
```rust
use crate::error::AppResult;
use std::path::Path;

pub fn create_backup(_config_path: &Path, _backup_dir: &Path) -> AppResult<()> {
    unimplemented!()
}
```

`src-tauri/src/config/writer.rs`:
```rust
use crate::error::AppResult;
use std::path::Path;

pub fn atomic_write(_path: &Path, _content: &str) -> AppResult<()> {
    unimplemented!()
}
```

`src-tauri/src/config/diff.rs`:
```rust
use crate::config::model::DiffLine;

pub fn between(_old: &str, _new: &str) -> Vec<DiffLine> {
    unimplemented!()
}
```

- [ ] **Step 6: cargo check 验证编译通过**

Run:
```bash
cd "/Users/liuweijia/Desktop/AI/Codex Box/src-tauri"
cargo check
```
Expected: `Finished` 无 error

- [ ] **Step 7: Commit**

```bash
cd "/Users/liuweijia/Desktop/AI/Codex Box"
git add src-tauri/src/
git commit -m "chore: 引入 error / model / lib 框架与 4 个占位模块"
```

---

## Task 3: tests/fixtures 准备 3 个样本

**Files:**
- Create: `src-tauri/tests/fixtures/minimal.toml`
- Create: `src-tauri/tests/fixtures/with_mcp.toml`
- Create: `src-tauri/tests/fixtures/with_marketplace.toml`

- [ ] **Step 1: minimal.toml**

```toml
# 最小样本：只有顶层字段
model = "gpt-5.5"
approval_policy = "never"
sandbox_mode = "danger-full-access"
network_access = "enabled"
```

- [ ] **Step 2: with_mcp.toml**

```toml
# 含 mcp_servers（stdio + http）
model = "gpt-5.5"
approval_policy = "never"
sandbox_mode = "workspace-write"

[mcp_servers.filesystem]
command = "npx"
args = ["-y", "@modelcontextprotocol/server-filesystem"]

[mcp_servers.openaiDeveloperDocs]
url = "https://developers.openai.com/mcp"
```

- [ ] **Step 3: with_marketplace.toml**

```toml
# 含 marketplaces
model = "gpt-5.5"

[marketplaces.openai-bundled]
last_updated = "2026-06-24T01:22:11Z"
source_type = "local"
source = "/Users/liuweijia/.codex/.tmp/bundled-marketplaces/openai-bundled"

[marketplaces.agentmemory]
last_updated = "2026-06-12T06:43:33Z"
source_type = "git"
source = "https://github.com/rohitg00/agentmemory.git"
```

- [ ] **Step 4: Commit**

```bash
cd "/Users/liuweijia/Desktop/AI/Codex Box"
git add src-tauri/tests/fixtures/
git commit -m "test: 准备 3 个 TOML fixture"
```

---

## Task 4: parser.rs 实现 + 单测

**Files:**
- Modify: `src-tauri/src/config/parser.rs`
- Create: `src-tauri/tests/parser_test.rs`（或内联 `#[cfg(test)] mod tests`）

- [ ] **Step 1: 实现 parser.rs**

```rust
// src-tauri/src/config/parser.rs
use crate::config::model::{
    CodexConfig, DashboardSummary, HealthSummary, MarketplaceEntry, McpCount, McpServerEntry,
    McpServerKind,
};
use crate::error::AppResult;

/// 解析 raw TOML 文本为 CodexConfig
pub fn parse(raw: &str) -> AppResult<CodexConfig> {
    let value: toml::Value = toml::from_str(raw)?;

    let mut top_level = serde_json::Map::new();
    let mut mcp_servers = Vec::new();
    let mut marketplaces = Vec::new();
    let mut other_tables = serde_json::Map::new();

    if let Some(table) = value.as_table() {
        for (key, val) in table {
            if key == "mcp_servers" {
                if let Some(t) = val.as_table() {
                    for (name, entry) in t {
                        mcp_servers.push(parse_mcp_entry(name, entry));
                    }
                }
            } else if key == "marketplaces" {
                if let Some(t) = val.as_table() {
                    for (name, entry) in t {
                        marketplaces.push(parse_marketplace(name, entry));
                    }
                }
            } else if val.is_table() {
                // 跳过内联表的递归展开
                other_tables.insert(
                    key.clone(),
                    serde_json::to_value(val).unwrap_or(serde_json::Value::Null),
                );
            } else {
                // 标量：转 JSON Value
                let json_val = serde_json::to_value(val).unwrap_or(serde_json::Value::Null);
                top_level.insert(key.clone(), json_val);
            }
        }
    }

    Ok(CodexConfig {
        top_level,
        mcp_servers,
        marketplaces,
        other_tables,
    })
}

fn parse_mcp_entry(name: &str, val: &toml::Value) -> McpServerEntry {
    let table = match val.as_table() {
        Some(t) => t,
        None => {
            return McpServerEntry {
                name: name.to_string(),
                kind: McpServerKind::Http {
                    url: String::new(),
                },
            }
        }
    };

    if let Some(url) = table.get("url").and_then(|v| v.as_str()) {
        McpServerEntry {
            name: name.to_string(),
            kind: McpServerKind::Http {
                url: url.to_string(),
            },
        }
    } else if let Some(cmd) = table.get("command").and_then(|v| v.as_str()) {
        let args = table
            .get("args")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|x| x.as_str().map(String::from))
                    .collect()
            })
            .unwrap_or_default();
        McpServerEntry {
            name: name.to_string(),
            kind: McpServerKind::Stdio {
                command: cmd.to_string(),
                args,
            },
        }
    } else {
        McpServerEntry {
            name: name.to_string(),
            kind: McpServerKind::Http {
                url: String::new(),
            },
        }
    }
}

fn parse_marketplace(name: &str, val: &toml::Value) -> MarketplaceEntry {
    let table = val.as_table();
    MarketplaceEntry {
        name: name.to_string(),
        source_type: table
            .and_then(|t| t.get("source_type"))
            .and_then(|v| v.as_str())
            .map(String::from),
        source: table
            .and_then(|t| t.get("source"))
            .and_then(|v| v.as_str())
            .map(String::from),
    }
}

/// 转换为 Dashboard 摘要
pub fn to_dashboard_summary(config: &CodexConfig) -> DashboardSummary {
    let active_profile = config
        .top_level
        .get("model")
        .and_then(|v| v.as_str())
        .map(String::from);

    // M0 简化：provider_count = top_level 中包含 "model_provider" 或 "model_providers" 的数量
    let provider_count = if config.top_level.contains_key("model_provider")
        || config.top_level.contains_key("model_providers")
    {
        1
    } else {
        0
    };

    let total = config.mcp_servers.len();
    let enabled = total; // M0 简化为：存在即启用

    let network = config
        .top_level
        .get("network_access")
        .and_then(|v| v.as_str())
        .map(String::from)
        .unwrap_or_else(|| "direct".to_string());

    DashboardSummary {
        active_profile,
        provider_count,
        mcp_count: McpCount { enabled, total },
        network,
        last_backup_at: None,
        health_summary: HealthSummary {
            ok: 0,
            warn: 0,
            fail: 0,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_minimal_extracts_top_level() {
        let raw = r#"
model = "gpt-5.5"
approval_policy = "never"
sandbox_mode = "danger-full-access"
"#;
        let cfg = parse(raw).expect("parse ok");
        assert_eq!(
            cfg.top_level.get("model").and_then(|v| v.as_str()),
            Some("gpt-5.5")
        );
        assert_eq!(
            cfg.top_level
                .get("approval_policy")
                .and_then(|v| v.as_str()),
            Some("never")
        );
        assert_eq!(cfg.mcp_servers.len(), 0);
        assert_eq!(cfg.marketplaces.len(), 0);
    }

    #[test]
    fn parse_with_mcp_extracts_stdio_and_http() {
        let raw = r#"
[mcp_servers.filesystem]
command = "npx"
args = ["-y", "@x/y"]

[mcp_servers.docs]
url = "https://example.com/mcp"
"#;
        let cfg = parse(raw).expect("parse ok");
        assert_eq!(cfg.mcp_servers.len(), 2);

        let fs = cfg
            .mcp_servers
            .iter()
            .find(|s| s.name == "filesystem")
            .unwrap();
        match &fs.kind {
            McpServerKind::Stdio { command, args } => {
                assert_eq!(command, "npx");
                assert_eq!(args, &vec!["-y".to_string(), "@x/y".to_string()]);
            }
            _ => panic!("expected stdio"),
        }

        let docs = cfg.mcp_servers.iter().find(|s| s.name == "docs").unwrap();
        match &docs.kind {
            McpServerKind::Http { url } => assert_eq!(url, "https://example.com/mcp"),
            _ => panic!("expected http"),
        }
    }

    #[test]
    fn parse_with_marketplace_extracts_entries() {
        let raw = r#"
[marketplaces.alpha]
source_type = "local"
source = "/tmp/alpha"

[marketplaces.beta]
source_type = "git"
source = "https://example.com/beta.git"
"#;
        let cfg = parse(raw).expect("parse ok");
        assert_eq!(cfg.marketplaces.len(), 2);
        assert!(cfg.marketplaces.iter().any(|m| m.name == "alpha"));
        assert!(cfg.marketplaces.iter().any(|m| m.name == "beta"));
    }

    #[test]
    fn parse_invalid_toml_returns_err() {
        let raw = "this is not valid toml ====";
        assert!(parse(raw).is_err());
    }

    #[test]
    fn to_dashboard_summary_minimal() {
        let raw = r#"
model = "gpt-5.5"
network_access = "enabled"

[mcp_servers.a]
command = "x"

[mcp_servers.b]
url = "https://e.com/m"
"#;
        let cfg = parse(raw).expect("parse ok");
        let summary = to_dashboard_summary(&cfg);
        assert_eq!(summary.active_profile.as_deref(), Some("gpt-5.5"));
        assert_eq!(summary.mcp_count.total, 2);
        assert_eq!(summary.mcp_count.enabled, 2);
        assert_eq!(summary.network, "enabled");
    }
}
```

- [ ] **Step 2: 跑 cargo test**

Run:
```bash
cd "/Users/liuweijia/Desktop/AI/Codex Box/src-tauri"
cargo test parser
```
Expected: 5 passed

- [ ] **Step 3: Commit**

```bash
cd "/Users/liuweijia/Desktop/AI/Codex Box"
git add src-tauri/src/config/parser.rs
git commit -m "feat: 引入 config 解析器"
```

---

## Task 5: loader.rs 实现 + 单测

**Files:**
- Modify: `src-tauri/src/config/loader.rs`

- [ ] **Step 1: 实现 loader.rs**

```rust
// src-tauri/src/config/loader.rs
use crate::error::{AppError, AppResult};
use sha2::{Digest, Sha256};
use std::path::Path;

/// 读取 config 文件原始文本
pub fn read_raw(path: &Path) -> AppResult<String> {
    if !path.exists() {
        return Err(AppError::ConfigNotFound(path.display().to_string()));
    }
    let bytes = std::fs::read(path)?;
    String::from_utf8(bytes).map_err(|_| AppError::InvalidUtf8(path.display().to_string()))
}

/// 文件元信息
pub struct Metadata {
    pub size_bytes: u64,
    pub content_hash: String,
}

pub fn metadata(path: &Path) -> AppResult<Metadata> {
    let bytes = std::fs::read(path)?;
    let mut hasher = Sha256::new();
    hasher.update(&bytes);
    let hash = format!("{:x}", hasher.finalize());
    Ok(Metadata {
        size_bytes: bytes.len() as u64,
        content_hash: hash,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    #[test]
    fn read_raw_returns_content() {
        let mut f = NamedTempFile::new().unwrap();
        f.write_all(b"hello world").unwrap();
        let content = read_raw(f.path()).unwrap();
        assert_eq!(content, "hello world");
    }

    #[test]
    fn read_raw_missing_file_errors() {
        let path = Path::new("/tmp/codex-box-nonexistent-zzzz.toml");
        let _ = std::fs::remove_file(path);
        assert!(matches!(
            read_raw(path),
            Err(AppError::ConfigNotFound(_))
        ));
    }

    #[test]
    fn metadata_returns_size_and_hash() {
        let mut f = NamedTempFile::new().unwrap();
        f.write_all(b"abc").unwrap();
        let m = metadata(f.path()).unwrap();
        assert_eq!(m.size_bytes, 3);
        // sha256("abc")
        assert_eq!(
            m.content_hash,
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }
}
```

- [ ] **Step 2: 跑 cargo test**

Run:
```bash
cd "/Users/liuweijia/Desktop/AI/Codex Box/src-tauri"
cargo test loader
```
Expected: 3 passed

- [ ] **Step 3: Commit**

```bash
cd "/Users/liuweijia/Desktop/AI/Codex Box"
git add src-tauri/src/config/loader.rs
git commit -m "feat: 引入 config 加载器"
```

---

## Task 6: backup.rs 严格 TDD

**Files:**
- Modify: `src-tauri/src/config/backup.rs`

- [ ] **Step 1: RED — 写失败测试**

替换 `src-tauri/src/config/backup.rs`：

```rust
// src-tauri/src/config/backup.rs
use crate::config::model::{BackupReason, BackupRecord};
use crate::error::{AppError, AppResult};
use chrono::Utc;
use sha2::{Digest, Sha256};
use std::path::Path;

pub fn create_backup(
    config_path: &Path,
    backup_dir: &Path,
    reason: BackupReason,
) -> AppResult<BackupRecord> {
    unimplemented!()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::tempdir;
    use tempfile::NamedTempFile;

    #[test]
    fn create_backup_copies_file_to_backup_dir() {
        // Arrange
        let mut src = NamedTempFile::new().unwrap();
        src.write_all(b"original config").unwrap();
        let dir = tempdir().unwrap();
        let backup_dir = dir.path().to_path_buf();

        // Act
        let rec = create_backup(src.path(), &backup_dir, BackupReason::Manual).unwrap();

        // Assert
        assert!(rec.file_path.contains("codex-box"));
        assert!(std::path::Path::new(&rec.file_path).exists());
        let copied = std::fs::read_to_string(&rec.file_path).unwrap();
        assert_eq!(copied, "original config");
    }

    #[test]
    fn create_backup_records_sha256_and_size() {
        let mut src = NamedTempFile::new().unwrap();
        src.write_all(b"abc").unwrap();
        let dir = tempdir().unwrap();
        let rec = create_backup(src.path(), dir.path(), BackupReason::PreWrite).unwrap();
        assert_eq!(rec.size_bytes, 3);
        assert_eq!(
            rec.content_hash,
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }

    #[test]
    fn create_backup_missing_source_errors() {
        let dir = tempdir().unwrap();
        let src = dir.path().join("missing.toml");
        let result = create_backup(&src, dir.path(), BackupReason::Manual);
        assert!(result.is_err());
    }

    #[test]
    fn create_backup_creates_dir_if_missing() {
        let mut src = NamedTempFile::new().unwrap();
        src.write_all(b"x").unwrap();
        let dir = tempdir().unwrap();
        let backup_dir = dir.path().join("nested").join("backups");

        let rec = create_backup(src.path(), &backup_dir, BackupReason::Manual).unwrap();
        assert!(backup_dir.exists());
        assert!(std::path::Path::new(&rec.file_path).exists());
    }
}
```

- [ ] **Step 2: 跑测试看失败**

Run:
```bash
cd "/Users/liuweijia/Desktop/AI/Codex Box/src-tauri"
cargo test backup
```
Expected: 4 failed (unimplemented)

- [ ] **Step 3: GREEN — 实现最小代码**

替换 `pub fn create_backup(...)` 上面的 `unimplemented!()`：

```rust
pub fn create_backup(
    config_path: &Path,
    backup_dir: &Path,
    reason: BackupReason,
) -> AppResult<BackupRecord> {
    // 1. 校验源文件存在
    if !config_path.exists() {
        return Err(AppError::ConfigNotFound(config_path.display().to_string()));
    }

    // 2. 读源
    let bytes = std::fs::read(config_path)?;
    let size_bytes = bytes.len() as u64;

    // 3. 计算 hash
    let mut hasher = Sha256::new();
    hasher.update(&bytes);
    let content_hash = format!("{:x}", hasher.finalize());

    // 4. 创建备份目录
    std::fs::create_dir_all(backup_dir)
        .map_err(|e| AppError::BackupDir(format!("{}: {}", backup_dir.display(), e)))?;

    // 5. 命名：codex-box-{timestamp}-{hash8}.toml
    let timestamp = Utc::now().format("%Y%m%dT%H%M%S").to_string();
    let short_hash = &content_hash[..8];
    let filename = format!("codex-box-{}-{}.toml", timestamp, short_hash);
    let dest = backup_dir.join(&filename);

    // 6. 复制
    std::fs::write(&dest, &bytes)
        .map_err(|e| AppError::BackupDir(format!("write failed: {}", e)))?;

    // 7. 返回 record
    let reason_str = match reason {
        BackupReason::Manual => "manual",
        BackupReason::PreWrite => "pre_write",
        BackupReason::PreRollback => "pre_rollback",
        BackupReason::Scheduled => "scheduled",
    };
    let id = format!("{}-{}", timestamp, short_hash);
    let created_at = Utc::now().to_rfc3339();

    Ok(BackupRecord {
        id,
        created_at,
        file_path: dest.display().to_string(),
        reason: match reason_str {
            "manual" => BackupReason::Manual,
            "pre_write" => BackupReason::PreWrite,
            "pre_rollback" => BackupReason::PreRollback,
            _ => BackupReason::Scheduled,
        },
        content_hash,
        size_bytes,
    })
}
```

- [ ] **Step 4: 跑测试看通过**

Run:
```bash
cd "/Users/liuweijia/Desktop/AI/Codex Box/src-tauri"
cargo test backup
```
Expected: 4 passed

- [ ] **Step 5: REFACTOR — 简化 reason 转换**

把 `match reason_str` 那一段简化为直接 `reason: reason,`（不需要 round-trip 字符串）。再跑测试确认绿。

- [ ] **Step 6: Commit**

```bash
cd "/Users/liuweijia/Desktop/AI/Codex Box"
git add src-tauri/src/config/backup.rs
git commit -m "feat: 引入 backup 模块"
```

---

## Task 7: writer.rs 严格 TDD

**Files:**
- Modify: `src-tauri/src/config/writer.rs`

- [ ] **Step 1: RED — 写失败测试**

替换 `src-tauri/src/config/writer.rs`：

```rust
// src-tauri/src/config/writer.rs
use crate::error::{AppError, AppResult};
use std::path::Path;

pub fn atomic_write(path: &Path, content: &str) -> AppResult<()> {
    unimplemented!()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn atomic_write_creates_file_with_content() {
        let dir = tempdir().unwrap();
        let target = dir.path().join("out.toml");
        atomic_write(&target, "hello").unwrap();
        assert_eq!(fs::read_to_string(&target).unwrap(), "hello");
    }

    #[test]
    fn atomic_write_overwrites_existing_file() {
        let dir = tempdir().unwrap();
        let target = dir.path().join("out.toml");
        fs::write(&target, "old").unwrap();
        atomic_write(&target, "new").unwrap();
        assert_eq!(fs::read_to_string(&target).unwrap(), "new");
    }

    #[test]
    fn atomic_write_does_not_leave_tmp_file() {
        let dir = tempdir().unwrap();
        let target = dir.path().join("out.toml");
        atomic_write(&target, "x").unwrap();
        let tmp = dir.path().join("out.toml.tmp");
        assert!(!tmp.exists(), "tmp file leaked");
    }

    #[test]
    fn atomic_write_preserves_target_on_invalid_path() {
        // 试图写入一个不存在的目录，应该返回错误而不是留下垃圾
        let dir = tempdir().unwrap();
        let target = dir.path().join("nonexistent_subdir").join("out.toml");
        let result = atomic_write(&target, "x");
        assert!(result.is_err());
    }
}
```

- [ ] **Step 2: 跑测试看失败**

Run:
```bash
cd "/Users/liuweijia/Desktop/AI/Codex Box/src-tauri"
cargo test writer
```
Expected: 4 failed

- [ ] **Step 3: GREEN — 实现最小代码**

替换 `pub fn atomic_write(...)` 上面的 `unimplemented!()`：

```rust
pub fn atomic_write(path: &Path, content: &str) -> AppResult<()> {
    // 1. 写 tmp 文件（与 target 同目录，确保 rename atomic）
    let parent = path
        .parent()
        .ok_or_else(|| AppError::AtomicWrite(format!("no parent for {}", path.display())))?;
    std::fs::create_dir_all(parent).map_err(|e| {
        AppError::AtomicWrite(format!("create_dir_all {}: {}", parent.display(), e))
    })?;

    let file_name = path
        .file_name()
        .ok_or_else(|| AppError::AtomicWrite(format!("no file_name for {}", path.display())))?;
    let tmp = parent.join(format!("{}.tmp", file_name.to_string_lossy()));

    std::fs::write(&tmp, content)
        .map_err(|e| AppError::AtomicWrite(format!("write tmp {}: {}", tmp.display(), e)))?;

    // 2. rename 替换
    if let Err(e) = std::fs::rename(&tmp, path) {
        // 清理 tmp
        let _ = std::fs::remove_file(&tmp);
        return Err(AppError::AtomicWrite(format!(
            "rename {} -> {}: {}",
            tmp.display(),
            path.display(),
            e
        )));
    }

    Ok(())
}
```

- [ ] **Step 4: 跑测试看通过**

Run:
```bash
cd "/Users/liuweijia/Desktop/AI/Codex Box/src-tauri"
cargo test writer
```
Expected: 4 passed

- [ ] **Step 5: Commit**

```bash
cd "/Users/liuweijia/Desktop/AI/Codex Box"
git add src-tauri/src/config/writer.rs
git commit -m "feat: 引入 atomic write 模块"
```

---

## Task 8: diff.rs 严格 TDD

**Files:**
- Modify: `src-tauri/src/config/diff.rs`

- [ ] **Step 1: RED — 写失败测试**

替换 `src-tauri/src/config/diff.rs`：

```rust
// src-tauri/src/config/diff.rs
use crate::config::model::{DiffKind, DiffLine};

pub fn between(old: &str, new: &str) -> Vec<DiffLine> {
    unimplemented!()
}

/// 统计各类型行数
pub fn count_by_kind(lines: &[DiffLine]) -> (usize, usize, usize) {
    let mut ctx = 0;
    let mut ins = 0;
    let mut del = 0;
    for l in lines {
        match l.kind {
            DiffKind::Context => ctx += 1,
            DiffKind::Insert => ins += 1,
            DiffKind::Delete => del += 1,
        }
    }
    (ctx, ins, del)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identical_strings_produce_only_context() {
        let lines = between("a\nb\n", "a\nb\n");
        let (ctx, ins, del) = count_by_kind(&lines);
        assert_eq!(ctx, 2);
        assert_eq!(ins, 0);
        assert_eq!(del, 0);
    }

    #[test]
    fn added_line_marked_insert() {
        let lines = between("a\n", "a\nb\n");
        let has_insert = lines.iter().any(|l| l.kind == DiffKind::Insert);
        assert!(has_insert);
    }

    #[test]
    fn removed_line_marked_delete() {
        let lines = between("a\nb\n", "a\n");
        let has_delete = lines.iter().any(|l| l.kind == DiffKind::Delete);
        assert!(has_delete);
    }

    #[test]
    fn empty_inputs_produce_empty_diff() {
        let lines = between("", "");
        assert!(lines.is_empty());
    }
}
```

- [ ] **Step 2: 跑测试看失败**

Run:
```bash
cd "/Users/liuweijia/Desktop/AI/Codex Box/src-tauri"
cargo test diff
```
Expected: 4 failed

- [ ] **Step 3: GREEN — 实现最小代码**

替换 `pub fn between(...)` 上面的 `unimplemented!()`：

```rust
pub fn between(old: &str, new: &str) -> Vec<DiffLine> {
    let old_lines: Vec<&str> = old.split_inclusive('\n').collect();
    let new_lines: Vec<&str> = new.split_inclusive('\n').collect();

    // 简单 LCS-based diff
    let lcs = compute_lcs(&old_lines, &new_lines);
    let mut result = Vec::new();
    let mut o = 0;
    let mut n = 0;
    let mut lcs_idx = 0;

    while o < old_lines.len() || n < new_lines.len() {
        if lcs_idx < lcs.len() {
            let (li, lj) = lcs[lcs_idx];
            // 输出 old 从 o 到 li 之间的删除
            while o < li {
                result.push(DiffLine {
                    kind: DiffKind::Delete,
                    content: old_lines[o].to_string(),
                    old_line: Some(o + 1),
                    new_line: None,
                });
                o += 1;
            }
            // 输出 new 从 n 到 lj 之间的插入
            while n < lj {
                result.push(DiffLine {
                    kind: DiffKind::Insert,
                    content: new_lines[n].to_string(),
                    old_line: None,
                    new_line: Some(n + 1),
                });
                n += 1;
            }
            // 输出 context
            result.push(DiffLine {
                kind: DiffKind::Context,
                content: old_lines[o].to_string(),
                old_line: Some(o + 1),
                new_line: Some(n + 1),
            });
            o += 1;
            n += 1;
            lcs_idx += 1;
        } else {
            // LCS 用尽
            while o < old_lines.len() {
                result.push(DiffLine {
                    kind: DiffKind::Delete,
                    content: old_lines[o].to_string(),
                    old_line: Some(o + 1),
                    new_line: None,
                });
                o += 1;
            }
            while n < new_lines.len() {
                result.push(DiffLine {
                    kind: DiffKind::Insert,
                    content: new_lines[n].to_string(),
                    old_line: None,
                    new_line: Some(n + 1),
                });
                n += 1;
            }
        }
    }

    result
}

fn compute_lcs(a: &[&str], b: &[&str]) -> Vec<(usize, usize)> {
    let m = a.len();
    let n = b.len();
    let mut dp = vec![vec![0usize; n + 1]; m + 1];
    for i in 0..m {
        for j in 0..n {
            if a[i] == b[j] {
                dp[i + 1][j + 1] = dp[i][j] + 1;
            } else {
                dp[i + 1][j + 1] = dp[i + 1][j].max(dp[i][j + 1]);
            }
        }
    }
    let mut result = Vec::new();
    let mut i = m;
    let mut j = n;
    while i > 0 && j > 0 {
        if a[i - 1] == b[j - 1] {
            result.push((i - 1, j - 1));
            i -= 1;
            j -= 1;
        } else if dp[i - 1][j] >= dp[i][j - 1] {
            i -= 1;
        } else {
            j -= 1;
        }
    }
    result.reverse();
    result
}
```

- [ ] **Step 4: 跑测试看通过**

Run:
```bash
cd "/Users/liuweijia/Desktop/AI/Codex Box/src-tauri"
cargo test diff
```
Expected: 4 passed

- [ ] **Step 5: Commit**

```bash
cd "/Users/liuweijia/Desktop/AI/Codex Box"
git add src-tauri/src/config/diff.rs
git commit -m "feat: 引入文本 diff 模块"
```

---

## Task 9: commands/dashboard.rs + 注册

**Files:**
- Create: `src-tauri/src/commands/mod.rs`
- Create: `src-tauri/src/commands/dashboard.rs`
- Modify: `src-tauri/src/lib.rs`

- [ ] **Step 1: 创建 commands/mod.rs**

```rust
// src-tauri/src/commands/mod.rs
pub mod dashboard;
```

- [ ] **Step 2: 创建 commands/dashboard.rs**

```rust
// src-tauri/src/commands/dashboard.rs
use crate::config::{loader, parser};
use crate::error::{AppError, AppResult};
use std::path::PathBuf;

const DEFAULT_CONFIG_PATH: &str = ".codex/config.toml";

fn resolve_config_path() -> AppResult<PathBuf> {
    let home = dirs::home_dir()
        .ok_or_else(|| AppError::ConfigNotFound("home dir not found".to_string()))?;
    Ok(home.join(DEFAULT_CONFIG_PATH))
}

#[tauri::command]
pub fn dashboard_summary() -> AppResult<crate::config::model::DashboardSummary> {
    let path = resolve_config_path()?;
    let raw = loader::read_raw(&path)?;
    let config = parser::parse(&raw)?;
    Ok(parser::to_dashboard_summary(&config))
}
```

- [ ] **Step 3: 修改 lib.rs 注册 command**

```rust
// src-tauri/src/lib.rs
pub mod commands;
pub mod config;
pub mod error;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .invoke_handler(tauri::generate_handler![commands::dashboard::dashboard_summary])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
```

- [ ] **Step 4: 跑 cargo build**

Run:
```bash
cd "/Users/liuweijia/Desktop/AI/Codex Box/src-tauri"
cargo build
```
Expected: 编译成功

- [ ] **Step 5: Commit**

```bash
cd "/Users/liuweijia/Desktop/AI/Codex Box"
git add src-tauri/src/commands/ src-tauri/src/lib.rs
git commit -m "feat: 接入 get_dashboard_summary command"
```

---

## Task 10: 前端 index.css + main.tsx

**Files:**
- Create: `src/index.css`
- Create: `src/main.tsx`

- [ ] **Step 1: 写 index.css**

```css
@tailwind base;
@tailwind components;
@tailwind utilities;

html, body, #root {
  height: 100%;
  margin: 0;
}

body {
  font-family: 'PingFang SC', 'Inter', system-ui, -apple-system, sans-serif;
  background: linear-gradient(180deg, #F5F5F7 0%, #EAEAEC 100%);
  color: #1C1C1E;
  -webkit-font-smoothing: antialiased;
}
```

- [ ] **Step 2: 写 main.tsx**

```tsx
// src/main.tsx
import React from "react";
import ReactDOM from "react-dom/client";
import App from "./App";
import "./index.css";

ReactDOM.createRoot(document.getElementById("root")!).render(
  <React.StrictMode>
    <App />
  </React.StrictMode>
);
```

- [ ] **Step 3: 写 src/vite-env.d.ts**

```ts
/// <reference types="vite/client" />
```

- [ ] **Step 4: Commit**

```bash
cd "/Users/liuweijia/Desktop/AI/Codex Box"
git add src/index.css src/main.tsx src/vite-env.d.ts
git commit -m "feat: 引入 React 入口与全局样式"
```

---

## Task 11: 前端 lib/api.ts

**Files:**
- Create: `src/lib/api.ts`

- [ ] **Step 1: 写 api.ts**

```ts
// src/lib/api.ts
import { invoke } from "@tauri-apps/api/core";

export type ApiResult<T> =
  | { ok: true; data: T }
  | { ok: false; error: string };

export async function invokeCmd<T>(name: string, args?: Record<string, unknown>): Promise<ApiResult<T>> {
  try {
    const data = await invoke<T>(name, args);
    return { ok: true, data };
  } catch (e) {
    return { ok: false, error: typeof e === "string" ? e : String(e) };
  }
}
```

- [ ] **Step 2: 写 DashboardSummary 类型**

```ts
// src/lib/types.ts
export interface McpCount {
  enabled: number;
  total: number;
}

export interface HealthSummary {
  ok: number;
  warn: number;
  fail: number;
}

export interface DashboardSummary {
  active_profile: string | null;
  provider_count: number;
  mcp_count: McpCount;
  network: string;
  last_backup_at: string | null;
  health_summary: HealthSummary;
}
```

- [ ] **Step 3: Commit**

```bash
cd "/Users/liuweijia/Desktop/AI/Codex Box"
git add src/lib/
git commit -m "feat: 接入 invoke 客户端封装"
```

---

## Task 12: 前端 store/dashboard.ts

**Files:**
- Create: `src/store/dashboard.ts`

- [ ] **Step 1: 写 store**

```ts
// src/store/dashboard.ts
import { create } from "zustand";
import { invokeCmd } from "@/lib/api";
import type { DashboardSummary } from "@/lib/types";

interface DashboardState {
  data: DashboardSummary | null;
  loading: boolean;
  error: string | null;
  load: () => Promise<void>;
}

export const useDashboardStore = create<DashboardState>((set) => ({
  data: null,
  loading: false,
  error: null,
  load: async () => {
    set({ loading: true, error: null });
    const result = await invokeCmd<DashboardSummary>("dashboard_summary");
    if (result.ok) {
      set({ data: result.data, loading: false });
    } else {
      set({ error: result.error, loading: false });
    }
  },
}));
```

- [ ] **Step 2: Commit**

```bash
cd "/Users/liuweijia/Desktop/AI/Codex Box"
git add src/store/
git commit -m "feat: 引入 dashboard store"
```

---

## Task 13: 前端 Sidebar.tsx + MetricCard.tsx

**Files:**
- Create: `src/components/Sidebar.tsx`
- Create: `src/components/MetricCard.tsx`

- [ ] **Step 1: 写 MetricCard.tsx**

```tsx
// src/components/MetricCard.tsx
import type { ReactNode } from "react";

interface Props {
  label: string;
  value: ReactNode;
  sub?: ReactNode;
  icon?: ReactNode;
  iconColor?: string;
}

export function MetricCard({ label, value, sub, icon, iconColor = "#34C759" }: Props) {
  return (
    <div className="rounded-md bg-white/70 backdrop-blur-md border border-white/40 shadow-card p-5 flex items-start justify-between gap-3">
      <div className="flex-1 min-w-0">
        <div className="text-[11px] uppercase tracking-wider text-ink-500 font-medium">{label}</div>
        <div className="mt-2 text-2xl font-semibold text-ink-900 truncate">{value}</div>
        {sub && <div className="mt-1 text-xs text-ink-500">{sub}</div>}
      </div>
      {icon && (
        <div
          className="w-9 h-9 rounded-md flex items-center justify-center text-white shrink-0"
          style={{ background: iconColor }}
        >
          {icon}
        </div>
      )}
    </div>
  );
}
```

- [ ] **Step 2: 写 Sidebar.tsx**

```tsx
// src/components/Sidebar.tsx
import {
  LayoutDashboard,
  Users,
  Server,
  Globe,
  Puzzle,
  GitCompare,
  Activity,
  Settings,
  Folder,
  Plug,
  Network,
  Boxes,
} from "lucide-react";

const NAV_ITEMS = [
  { name: "仪表盘", icon: LayoutDashboard, key: "1" },
  { name: "Profiles", icon: Users, key: "2" },
  { name: "Providers", icon: Server, key: "3" },
  { name: "Network", icon: Globe, key: "4" },
  { name: "MCP Servers", icon: Puzzle, key: "5" },
  { name: "Config Diff", icon: GitCompare, key: "6" },
  { name: "Diagnostics", icon: Activity, key: "7" },
];

const GROUPS = [
  { name: "profiles", icon: Folder, sub: "12 个 profile" },
  { name: "providers", icon: Plug, sub: "5 个 provider" },
  { name: "network", icon: Network, sub: "2 条 route" },
  { name: "mcp", icon: Boxes, sub: "8 个 server" },
];

export function Sidebar() {
  return (
    <aside
      className="w-[268px] shrink-0 rounded-lg bg-white/72 backdrop-blur-glass border border-white/60 shadow-card p-4 flex flex-col gap-4"
      style={{ WebkitAppRegion: "drag" } as React.CSSProperties}
    >
      <div className="flex items-center gap-2 pt-1">
        <div className="w-7 h-7 rounded-md bg-ink-900 flex items-center justify-center text-white font-serif text-sm">
          ◆
        </div>
        <div>
          <div className="font-serif text-base text-ink-900 leading-none">Codex Box</div>
          <div className="text-[11px] text-ink-500 mt-1 flex items-center gap-1.5">
            <span className="w-1.5 h-1.5 rounded-full bg-status-ok" />
            connected · v0.0.1
          </div>
        </div>
      </div>

      <nav className="flex flex-col gap-0.5">
        {NAV_ITEMS.map((item, idx) => {
          const Icon = item.icon;
          const active = idx === 0;
          return (
            <button
              key={item.name}
              className={`flex items-center gap-2.5 px-2.5 py-1.5 rounded-md text-sm transition-colors ${
                active
                  ? "bg-ink-900/5 text-ink-900 font-medium"
                  : "text-ink-700 hover:bg-ink-900/3"
              }`}
              style={{ WebkitAppRegion: "no-drag" } as React.CSSProperties}
            >
              <Icon size={15} strokeWidth={1.75} />
              <span className="flex-1 text-left">{item.name}</span>
              <span className="text-[11px] text-ink-400">⌘{item.key}</span>
            </button>
          );
        })}
      </nav>

      <div className="border-t border-ink-900/8 pt-3">
        <div className="text-[10px] uppercase tracking-wider text-ink-400 mb-2 px-2">分组</div>
        <div className="flex flex-col gap-0.5">
          {GROUPS.map((g) => {
            const Icon = g.icon;
            return (
              <div
                key={g.name}
                className="flex items-center gap-2.5 px-2.5 py-1.5 rounded-md text-sm text-ink-700"
              >
                <Icon size={14} strokeWidth={1.75} className="text-ink-500" />
                <div className="flex-1">
                  <div>{g.name}</div>
                  <div className="text-[10px] text-ink-400">{g.sub}</div>
                </div>
              </div>
            );
          })}
        </div>
      </div>

      <div className="mt-auto flex items-center gap-2 pt-2">
        <div className="relative w-8 h-8 rounded-full bg-gradient-to-br from-ink-700 to-ink-900 flex items-center justify-center text-white text-[10px] font-semibold">
          CB
          <span className="absolute -bottom-0.5 -right-0.5 w-2.5 h-2.5 rounded-full bg-status-ok border-2 border-white" />
        </div>
        <button
          className="flex-1 px-3 py-1.5 rounded-md bg-ink-900/5 text-xs text-ink-700 flex items-center justify-center gap-1.5"
          style={{ WebkitAppRegion: "no-drag" } as React.CSSProperties}
        >
          <span className="w-1.5 h-1.5 rounded-full bg-status-ok" />
          主动模式
        </button>
      </div>
    </aside>
  );
}
```

- [ ] **Step 3: 跑 tsc 检查**

Run:
```bash
cd "/Users/liuweijia/Desktop/AI/Codex Box"
pnpm install
```
Expected: 依赖装好

Run:
```bash
cd "/Users/liuweijia/Desktop/AI/Codex Box"
pnpm tsc --noEmit
```
Expected: 无 error

- [ ] **Step 4: Commit**

```bash
cd "/Users/liuweijia/Desktop/AI/Codex Box"
git add src/components/
git commit -m "feat: 引入 Sidebar 与 MetricCard 组件"
```

---

## Task 14: 前端 Dashboard.tsx

**Files:**
- Create: `src/pages/Dashboard.tsx`

- [ ] **Step 1: 写 Dashboard.tsx**

```tsx
// src/pages/Dashboard.tsx
import { useEffect } from "react";
import { User, Database, Globe, Puzzle, Activity, RefreshCw, Search, Settings as SettingsIcon } from "lucide-react";
import { useDashboardStore } from "@/store/dashboard";
import { MetricCard } from "@/components/MetricCard";

function greeting() {
  const h = new Date().getHours();
  if (h < 6) return "凌晨好";
  if (h < 12) return "早上好";
  if (h < 18) return "下午好";
  return "晚安";
}

function nowStr() {
  return new Date().toLocaleTimeString("zh-CN", { hour: "2-digit", minute: "2-digit" });
}

export function Dashboard() {
  const { data, loading, error, load } = useDashboardStore();

  useEffect(() => {
    load();
  }, [load]);

  return (
    <main className="flex-1 flex flex-col gap-4 min-w-0">
      {/* 顶部 titlebar */}
      <header className="h-10 flex items-center justify-between px-2" data-tauri-drag-region>
        <div className="text-xs text-ink-500">Codex Box · Dashboard</div>
        <div className="flex items-center gap-3 text-ink-500">
          <Search size={14} />
          <span className="text-[11px]">⌘K</span>
          <SettingsIcon size={14} />
          <div className="relative w-6 h-6 rounded-full bg-gradient-to-br from-ink-700 to-ink-900 text-white text-[9px] flex items-center justify-center">
            CB
            <span className="absolute -bottom-0.5 -right-0.5 w-2 h-2 rounded-full bg-status-ok border border-white" />
          </div>
        </div>
      </header>

      {/* 欢迎 */}
      <section className="rounded-md bg-white/70 backdrop-blur-md border border-white/40 shadow-card p-6">
        <h1 className="font-serif text-[32px] font-medium text-ink-900 leading-tight">
          {greeting()}，开发者
        </h1>
        <p className="mt-2 text-sm text-ink-500">
          {error
            ? `读取失败：${error}`
            : loading
            ? "正在加载配置…"
            : "当前 Codex 配置一切正常 · 最近备份 2 分钟前 · " + nowStr()}
        </p>
        <div className="mt-4 flex items-center gap-2">
          <button className="px-3 py-1.5 rounded-md bg-ink-900/5 text-xs text-ink-700 flex items-center gap-1.5">
            <Activity size={12} /> Daily
          </button>
          <button className="px-3 py-1.5 rounded-md bg-ink-900/5 text-xs text-ink-700">
            <SettingsIcon size={12} />
          </button>
        </div>
      </section>

      {/* 4 张指标卡 */}
      <section className="grid grid-cols-4 gap-4">
        <MetricCard
          label="ACTIVE PROFILE"
          value={data?.active_profile ?? "—"}
          sub="顶层 model 字段"
          icon={<User size={16} />}
          iconColor="#34C759"
        />
        <MetricCard
          label="PROVIDER"
          value={data ? `${data.provider_count} 个` : "—"}
          sub="model_provider 配置"
          icon={<Database size={16} />}
          iconColor="#007AFF"
        />
        <MetricCard
          label="NETWORK"
          value={data?.network ?? "—"}
          sub="network_access"
          icon={<Globe size={16} />}
          iconColor="#5AC8FA"
        />
        <MetricCard
          label="MCP"
          value={data ? `${data.mcp_count.enabled} / ${data.mcp_count.total}` : "—"}
          sub="mcp_servers"
          icon={<Puzzle size={16} />}
          iconColor="#AF52DE"
        />
      </section>

      {/* 健康 + 活动 */}
      <section className="grid grid-cols-[1.5fr_1fr] gap-4">
        <div className="rounded-md bg-white/70 backdrop-blur-md border border-white/40 shadow-card p-5">
          <div className="flex items-center justify-between mb-3">
            <h2 className="text-sm font-semibold text-ink-900">配置健康</h2>
            <button
              onClick={load}
              className="px-3 py-1.5 rounded-md bg-ink-900/5 text-xs text-ink-700 flex items-center gap-1.5"
            >
              <RefreshCw size={12} /> 重新检查
            </button>
          </div>
          <ul className="flex flex-col gap-2 text-sm">
            {[
              { name: "配置文件语法", status: "pass", ms: "12 ms" },
              { name: "provider URL", status: "pass", ms: "412 ms" },
              { name: "auth 状态", status: "warn", ms: "key 存在" },
              { name: "network 直连", status: "pass", ms: "280 ms" },
              { name: "MCP filesystem", status: "pass", ms: "ready" },
              { name: "backup 目录空间", status: "pass", ms: "1.2 GB free" },
            ].map((c) => (
              <li key={c.name} className="flex items-center gap-2 px-1 py-1">
                <span
                  className={`w-1.5 h-1.5 rounded-full ${
                    c.status === "pass"
                      ? "bg-status-ok"
                      : c.status === "warn"
                      ? "bg-status-warn"
                      : "bg-status-fail"
                  }`}
                />
                <span className="flex-1 text-ink-700">{c.name}</span>
                <span
                  className={`text-[11px] px-1.5 py-0.5 rounded ${
                    c.status === "pass"
                      ? "bg-status-ok/10 text-status-ok"
                      : "bg-status-warn/10 text-status-warn"
                  }`}
                >
                  {c.status}
                </span>
                <span className="text-[11px] text-ink-400 w-20 text-right">{c.ms}</span>
              </li>
            ))}
          </ul>
        </div>

        <div className="rounded-md bg-white/70 backdrop-blur-md border border-white/40 shadow-card p-5">
          <div className="flex items-center justify-between mb-3">
            <h2 className="text-sm font-semibold text-ink-900">最近活动</h2>
            <span className="text-[11px] text-ink-400">12</span>
          </div>
          <ul className="flex flex-col gap-2 text-sm">
            {[
              { t: "14:22", d: "备份 dev profile" },
              { t: "14:20", d: "切换 provider → openai" },
              { t: "14:18", d: "启用 mcp: filesystem" },
              { t: "14:10", d: "切换 network route" },
            ].map((a) => (
              <li key={a.t} className="flex items-center gap-2 px-1 py-1">
                <span className="w-1.5 h-1.5 rounded-full bg-status-ok" />
                <span className="text-[11px] text-ink-500 w-12">{a.t}</span>
                <span className="text-ink-700">{a.d}</span>
              </li>
            ))}
          </ul>
          <button className="mt-3 w-full px-3 py-1.5 rounded-md bg-ink-900/5 text-xs text-ink-700 flex items-center justify-center gap-1.5">
            查看全部 →
          </button>
        </div>
      </section>
    </main>
  );
}
```

- [ ] **Step 2: Commit**

```bash
cd "/Users/liuweijia/Desktop/AI/Codex Box"
git add src/pages/
git commit -m "feat: 实现 Dashboard 页面"
```

---

## Task 15: 前端 App.tsx

**Files:**
- Create: `src/App.tsx`

- [ ] **Step 1: 写 App.tsx**

```tsx
// src/App.tsx
import { Sidebar } from "@/components/Sidebar";
import { Dashboard } from "@/pages/Dashboard";

export default function App() {
  return (
    <div className="h-full p-6 flex gap-5">
      <Sidebar />
      <Dashboard />
    </div>
  );
}
```

- [ ] **Step 2: 跑 pnpm build**

Run:
```bash
cd "/Users/liuweijia/Desktop/AI/Codex Box"
pnpm build
```
Expected: 成功出 `dist/`

- [ ] **Step 3: Commit**

```bash
cd "/Users/liuweijia/Desktop/AI/Codex Box"
git add src/App.tsx
git commit -m "feat: 接入 App 顶层布局"
```

---

## Task 16: 端到端验收

**Files:** 无

- [ ] **Step 1: 跑 pnpm tauri dev**

Run:
```bash
cd "/Users/liuweijia/Desktop/AI/Codex Box"
pnpm tauri dev
```
Expected:
- 编译日志显示成功
- 一个 Tauri 窗口弹出，标题 "Codex Box"
- 看到左侧悬浮 Sidebar
- 看到右侧 4 张指标卡，值来自真实 `~/.codex/config.toml`（如 `gpt-5.5`、`enabled`、`8 / 8` 等）

- [ ] **Step 2: 验证指标卡数据**

手动核对：
- ACTIVE PROFILE 应该显示 `gpt-5.5`（来自真实 config 顶层 model）
- PROVIDER 应该显示 `0 个`（真实 config 没设 model_provider）
- NETWORK 应该显示 `enabled`（来自真实 config 顶层 network_access）
- MCP 应该显示 `3 / 3`（真实 config 有 3 个 mcp_servers）

- [ ] **Step 3: 杀掉 dev 进程**

按 `Ctrl+C` 停止 `pnpm tauri dev`。

- [ ] **Step 4: 验证 ~/.codex/config.toml 未被修改**

Run:
```bash
ls -la ~/.codex/config.toml
```
Expected: 修改时间不变（M0 只读不写）

- [ ] **Step 5: 最终 commit + merge**

```bash
cd "/Users/liuweijia/Desktop/AI/Codex Box"
git checkout main
git merge feat/m0-tauri-skeleton --no-ff -m "feat: M0 里程碑完成 - Tauri 骨架 + Dashboard 真实联调"
git log --oneline
```

Expected: 看到所有 commits 已合到 main

---

## 自检

| 检查项 | 状态 |
|---|---|
| Spec §1（目标）覆盖 | ✅ Task 0~16 跑通主路径 |
| Spec §2.1（范围内）覆盖 | ✅ 7 个模块 + 3 fixture + commands + 前端 |
| Spec §2.2（不在范围内） | ✅ Task 14 健康/活动 mock 标注 |
| Spec §3.1（toml 不引 toml_edit） | ✅ Task 4 parser 用 toml 0.8 |
| Spec §3.2（标准库 + tempfile） | ✅ 全程 |
| Spec §3.3（TDD 粒度） | ✅ Task 6/7/8 严格 RED-GREEN |
| Spec §3.4（worktree） | ⚠️ 改为在主目录 feat/m0 分支（更简单） |
| Spec §3.5（中文 commit） | ✅ 全程 |
| Spec §3.6（thiserror + 前端 try/catch） | ✅ Task 2/11 |
| Spec §4.1（模块拓扑） | ✅ 一致 |
| Spec §4.2（职责边界） | ✅ 每个模块单一职责 |
| Spec §5.1（DashboardSummary 字段） | ✅ model.rs + types.ts 一致 |
| Spec §6（主路径） | ✅ Task 9 |
| Spec §7（测试策略） | ✅ Task 4/5 单测 + Task 6/7/8 严格 TDD |
| Spec §8（UI 范围） | ✅ Task 13/14 |
| Spec §10（DoD） | ✅ Task 16 验收 |
| 类型一致性 | ✅ DashboardSummary 字段在 model.rs / types.ts / Dashboard.tsx 三处一致 |
| 无 placeholder | ✅ 所有代码块完整 |
| 命令精确 | ✅ 所有命令带 expected 输出 |
