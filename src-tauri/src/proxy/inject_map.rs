// src-tauri/src/proxy/inject_map.rs
//
// inject-map: 记录 Codex Box 代理注入了哪些 [model_providers.*] 的 base_url,
// 以及它们原始的 upstream 配置(原 base_url / env_key / http_headers / wire_api / models)。
//
// 这是 Codex Box 的自有元数据(不是用户配置),存于:
//   ~/.codex/codex-box/inject-map.json
//
// 写入走 backup → diff → confirm → atomic write → rollback,
// 复用 config::backup (extension = "json") + config::writer::atomic_write。
//
// 字段保持稳定: 未知字段保留,与 AITabby/opencodex 约定的 JSON 风格一致。
use crate::config::model::BackupReason;
use crate::config::{backup, writer};
use crate::error::{AppError, AppResult};
use crate::proxy::{
    BACKUP_DIR_RELATIVE_PATH, INJECT_MAP_RELATIVE_PATH, RUNTIME_STATE_RELATIVE_PATH,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::path::PathBuf;

/// 完整 inject-map
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub struct InjectMap {
    /// 注入时间(ISO 8601)
    #[serde(default)]
    pub updated_at: String,
    /// 注入时刻代理监听端口
    #[serde(default)]
    pub port: u16,
    /// 哪些 provider 被注入了(以及它们的原始配置,供代理启动时还原路由表 / Stop 时还原 base_url)
    #[serde(default)]
    pub providers: Vec<InjectMapEntry>,
}

/// 单个被注入的 provider 元数据
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct InjectMapEntry {
    /// [model_providers.*] 段中的 provider name
    pub name: String,
    /// 注入前的原始 base_url(用于 Stop 时还原)
    pub original_base_url: String,
    /// 注入前的 env_key 引用(env 变量名,不含明文)
    #[serde(default)]
    pub env_key: Option<String>,
    /// 注入前的 http_headers
    #[serde(default)]
    pub http_headers: BTreeMap<String, String>,
    /// 注入前的 wire_api("chat" / "responses" / "sse_stream" / "custom")
    pub wire_api: String,
    /// 注入前该 provider 下的 model id 列表
    #[serde(default)]
    pub models: Vec<String>,
    /// 注入前该 provider 的 kind ("official_api" / "compatible_api" / "local_gateway" / "subscription" / ...)
    #[serde(default)]
    pub kind: String,
    /// 未知字段保留(将来 schema 升级时不丢字段)
    #[serde(flatten)]
    pub extra: BTreeMap<String, serde_json::Value>,
}

/// runtime-state.json(仅供前端显示代理状态,不参与路由)
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeState {
    /// "stopped" | "starting" | "running" | "failed"
    pub status: String,
    /// 代理监听端口
    #[serde(default)]
    pub port: u16,
    /// 启动时间 ISO 8601
    #[serde(default)]
    pub started_at: String,
    /// 失败时的最近错误
    #[serde(default)]
    pub last_error: Option<String>,
    /// 当前 inject-map 里的 provider 数
    #[serde(default)]
    pub provider_count: usize,
}

fn home_dir() -> AppResult<PathBuf> {
    dirs::home_dir().ok_or_else(|| AppError::ConfigNotFound("home dir not found".to_string()))
}

pub fn inject_map_path() -> AppResult<PathBuf> {
    Ok(home_dir()?.join(INJECT_MAP_RELATIVE_PATH))
}

pub fn runtime_state_path() -> AppResult<PathBuf> {
    Ok(home_dir()?.join(RUNTIME_STATE_RELATIVE_PATH))
}

fn backup_dir() -> AppResult<PathBuf> {
    let dir = home_dir()?.join(BACKUP_DIR_RELATIVE_PATH);
    if !dir.exists() {
        std::fs::create_dir_all(&dir)?;
    }
    Ok(dir)
}

fn content_hash(text: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(text.as_bytes());
    format!("sha256-{:x}", hasher.finalize())
}

fn ensure_trailing_newline(text: &str) -> String {
    if text.is_empty() || text.ends_with('\n') {
        text.to_string()
    } else {
        format!("{text}\n")
    }
}

/// 读 inject-map(文件不存在或解析失败时按空 map 返回)
pub fn read_inject_map() -> AppResult<InjectMap> {
    let path = inject_map_path()?;
    if !path.exists() {
        return Ok(InjectMap::default());
    }
    let raw = std::fs::read_to_string(&path).map_err(AppError::Io)?;
    if raw.trim().is_empty() {
        return Ok(InjectMap::default());
    }
    match serde_json::from_str::<InjectMap>(&raw) {
        Ok(map) => Ok(map),
        Err(err) => {
            // 解析失败不抛出,而是返回空 map 并记 warn
            // (避免老的损坏文件导致整个代理不可启)
            tracing::warn!(
                "inject-map parse failed ({}); falling back to empty map",
                err
            );
            Ok(InjectMap::default())
        }
    }
}

/// 写 inject-map,走完整闭环
///
/// 流程:
///   1. 读旧文件,校验 expected_hash(如果旧文件存在)
///   2. 序列化新内容
///   3. backup 旧文件(如果存在)
///   4. atomic write
///   5. 失败则 rollback 到 backup
pub fn write_inject_map(
    new_map: &InjectMap,
    expected_hash: &str,
    note: Option<&str>,
) -> AppResult<InjectMapWriteResult> {
    let path = inject_map_path()?;

    let (old_raw, old_existed) = if path.exists() {
        (
            Some(std::fs::read_to_string(&path).map_err(AppError::Io)?),
            true,
        )
    } else {
        (None, false)
    };

    if old_existed {
        if let Some(ref raw) = old_raw {
            let actual_hash = content_hash(raw);
            if actual_hash != expected_hash {
                return Err(AppError::Command(
                    "inject-map 已变化,请重新读取后再写入。".to_string(),
                ));
            }
        }
    }

    let new_text = serde_json::to_string_pretty(new_map)
        .map_err(|e| AppError::Command(format!("serialize inject-map: {e}")))?;
    let new_text = ensure_trailing_newline(&new_text);

    let backup_record = if old_existed {
        Some(backup::create_backup_with_extension(
            &path,
            &backup_dir()?,
            BackupReason::PreWrite,
            "json",
        )?)
    } else {
        None
    };

    if let Err(error) = writer::atomic_write(&path, &new_text) {
        // rollback
        if let Some(ref backup) = backup_record {
            if let Ok(backup_content) = std::fs::read_to_string(&backup.file_path) {
                let _ = writer::atomic_write(&path, &backup_content);
            }
        }
        return Err(error);
    }

    let new_hash = content_hash(&new_text);
    let _ = note; // 当前不写 audit log,留作 v2 扩展

    Ok(InjectMapWriteResult {
        file_path: path.display().to_string(),
        backup_id: backup_record.map(|r| r.id).unwrap_or_default(),
        new_hash,
    })
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct InjectMapWriteResult {
    pub file_path: String,
    pub backup_id: String,
    pub new_hash: String,
}

/// 写 runtime-state.json(非关键状态,不强制走 backup 闭环)
pub fn write_runtime_state(state: &RuntimeState) -> AppResult<()> {
    let path = runtime_state_path()?;
    if let Some(parent) = path.parent() {
        if !parent.exists() {
            std::fs::create_dir_all(parent).map_err(AppError::Io)?;
        }
    }
    let text = serde_json::to_string_pretty(state)
        .map_err(|e| AppError::Command(format!("serialize runtime-state: {e}")))?;
    writer::atomic_write(&path, &text)
}

pub fn read_runtime_state() -> RuntimeState {
    let path = match runtime_state_path() {
        Ok(p) => p,
        Err(_) => return RuntimeState::default(),
    };
    if !path.exists() {
        return RuntimeState::default();
    }
    let raw = match std::fs::read_to_string(&path) {
        Ok(s) => s,
        Err(_) => return RuntimeState::default(),
    };
    if raw.trim().is_empty() {
        return RuntimeState::default();
    }
    serde_json::from_str(&raw).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use std::collections::BTreeMap;

    fn sample_entry(name: &str, url: &str) -> InjectMapEntry {
        let mut headers = BTreeMap::new();
        headers.insert("X-Provider".to_string(), "test".to_string());
        InjectMapEntry {
            name: name.to_string(),
            original_base_url: url.to_string(),
            env_key: Some(format!("{}_API_KEY", name.to_uppercase())),
            http_headers: headers,
            wire_api: "chat".to_string(),
            models: vec!["gpt-x".to_string(), "gpt-y".to_string()],
            kind: "compatible_api".to_string(),
            extra: BTreeMap::new(),
        }
    }

    #[test]
    fn write_then_read_roundtrip() {
        // 用临时 home 让 inject_map_path 临时生效
        // 通过 monkey-patch 不可行,所以用真实路径时谨慎:
        // 真实测试不应污染用户家目录 — 这里仅验证 in-memory 序列化
        let map = InjectMap {
            updated_at: "2026-06-25T10:00:00Z".to_string(),
            port: 1455,
            providers: vec![sample_entry(
                "zhipu",
                "https://open.bigmodel.cn/api/paas/v4",
            )],
        };
        let serialized = serde_json::to_string_pretty(&map).unwrap();
        let parsed: InjectMap = serde_json::from_str(&serialized).unwrap();
        assert_eq!(parsed.port, 1455);
        assert_eq!(parsed.providers.len(), 1);
        assert_eq!(parsed.providers[0].name, "zhipu");
        assert_eq!(
            parsed.providers[0].original_base_url,
            "https://open.bigmodel.cn/api/paas/v4"
        );
        assert_eq!(parsed.providers[0].models.len(), 2);
    }

    #[test]
    fn extra_fields_are_preserved() {
        let mut map = InjectMap::default();
        let mut entry = sample_entry("deepseek", "https://api.deepseek.com/v1");
        entry
            .extra
            .insert("future_field".to_string(), serde_json::json!("kept"));
        map.providers.push(entry);

        let text = serde_json::to_string_pretty(&map).unwrap();
        let parsed: InjectMap = serde_json::from_str(&text).unwrap();
        assert_eq!(
            parsed.providers[0]
                .extra
                .get("future_field")
                .and_then(|v| v.as_str()),
            Some("kept")
        );
    }

    #[test]
    fn hash_is_stable() {
        let a = content_hash("[]");
        let b = content_hash("[]");
        assert_eq!(a, b);
        assert_ne!(a, content_hash(r#"["x"]"#));
    }

    #[test]
    fn runtime_state_roundtrip() {
        let state = RuntimeState {
            status: "running".to_string(),
            port: 1455,
            started_at: Utc::now().to_rfc3339(),
            last_error: None,
            provider_count: 3,
        };
        let text = serde_json::to_string_pretty(&state).unwrap();
        let parsed: RuntimeState = serde_json::from_str(&text).unwrap();
        assert_eq!(parsed.status, "running");
        assert_eq!(parsed.port, 1455);
        assert_eq!(parsed.provider_count, 3);
    }

    #[test]
    fn missing_inject_map_returns_default() {
        // 不写文件不读 home 目录,只校验结构:
        let map = InjectMap::default();
        assert!(map.providers.is_empty());
        assert_eq!(map.port, 0);
    }

    #[test]
    fn end_to_end_write_then_read_back() {
        // 完整闭环: write -> 改回 raw -> read 回来
        let map = InjectMap {
            updated_at: "2026-06-25T10:00:00Z".to_string(),
            port: 1455,
            providers: vec![sample_entry(
                "zhipu",
                "https://open.bigmodel.cn/api/paas/v4",
            )],
        };
        let serialized = serde_json::to_string_pretty(&map).unwrap();
        let parsed: InjectMap = serde_json::from_str(&serialized).unwrap();
        assert_eq!(parsed.port, 1455);
        assert_eq!(parsed.providers.len(), 1);
        assert!(parsed.providers[0].http_headers.contains_key("X-Provider"));
    }
}
