use crate::config::model::{BackupReason, BackupRecord, DiffLine};
use crate::config::{backup, loader, writer};
use crate::error::{AppError, AppResult};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

const DEFAULT_CONFIG_PATH: &str = ".codex/config.toml";
const BACKUP_DIR: &str = ".codex/codex-box/backups";
const OPENCODEX_CATALOG_PATH: &str = ".opencodex/custom_model_catalog.json";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ConversationProviderCandidate {
    pub provider_id: String,
    pub display_name: Option<String>,
    pub original_base_url: Option<String>,
    pub wire_api: String,
    pub requires_openai_auth: Option<bool>,
    pub source_kind: String,
    pub source_path: String,
    pub last_seen_at: String,
    pub is_builtin_openai: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ConversationProviderCandidatesView {
    pub active_provider_id: String,
    pub config_path: String,
    pub candidates: Vec<ConversationProviderCandidate>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ConversationProviderRequest {
    pub provider_id: String,
    pub display_name: Option<String>,
    pub proxy_port: u16,
    pub wire_api: String,
    pub requires_openai_auth: bool,
    pub original_base_url: Option<String>,
    #[serde(default)]
    pub expected_hash: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ConversationProviderPreview {
    pub new_config_text: String,
    pub expected_hash: String,
    pub diff: Vec<DiffLine>,
    pub insertions: usize,
    pub deletions: usize,
    pub provider_id: String,
    pub proxy_base_url: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ApplyConversationProviderRequest {
    pub preview: ConversationProviderPreview,
    pub confirmed: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ApplyConversationProviderResult {
    pub new_config_hash: String,
    pub backup: BackupRecord,
}

#[tauri::command]
pub fn conversation_provider_candidates() -> AppResult<ConversationProviderCandidatesView> {
    let home = home_dir()?;
    discover_candidates_in_home(&home)
}

#[tauri::command]
pub fn conversation_provider_preview(
    request: ConversationProviderRequest,
) -> AppResult<ConversationProviderPreview> {
    let home = home_dir()?;
    let config_path = home.join(DEFAULT_CONFIG_PATH);
    let catalog_path = home.join(OPENCODEX_CATALOG_PATH);
    let old = loader::read_raw(&config_path)?;
    let old_hash = loader::metadata(&config_path)?.content_hash;
    if let Some(expected) = request.expected_hash.as_deref() {
        if expected != old_hash {
            return Err(AppError::Command(
                "config.toml 已变化,请重新读取后再预览".to_string(),
            ));
        }
    }
    let new_config_text = rewrite_conversation_provider(&old, &request, &catalog_path)?;
    let diff = crate::config::diff::between(&old, &new_config_text);
    let (_, insertions, deletions) = crate::config::diff::count_by_kind(&diff);
    Ok(ConversationProviderPreview {
        new_config_text,
        expected_hash: old_hash,
        diff,
        insertions,
        deletions,
        provider_id: normalize_provider_id(&request.provider_id)?,
        proxy_base_url: proxy_base_url(request.proxy_port),
    })
}

#[tauri::command]
pub fn conversation_provider_apply(
    request: ApplyConversationProviderRequest,
) -> AppResult<ApplyConversationProviderResult> {
    if !request.confirmed {
        return Err(AppError::Command(
            "应用会话归属 Provider 需要 confirmed=true".to_string(),
        ));
    }

    let home = home_dir()?;
    let config_path = home.join(DEFAULT_CONFIG_PATH);
    let backup_dir_path = home.join(BACKUP_DIR);
    let current_hash = loader::metadata(&config_path)?.content_hash;
    if current_hash != request.preview.expected_hash {
        return Err(AppError::Command(
            "config.toml 已变化,请重新预览 diff 后再确认写入".to_string(),
        ));
    }

    let backup = backup::create_backup(&config_path, &backup_dir_path, BackupReason::PreWrite)?;
    if let Err(error) = writer::atomic_write(&config_path, &request.preview.new_config_text) {
        let backup_content = std::fs::read_to_string(&backup.file_path)?;
        let _ = writer::atomic_write(&config_path, &backup_content);
        return Err(error);
    }
    let new_config_hash = loader::metadata(&config_path)?.content_hash;
    Ok(ApplyConversationProviderResult {
        new_config_hash,
        backup,
    })
}

fn home_dir() -> AppResult<PathBuf> {
    dirs::home_dir().ok_or_else(|| AppError::ConfigNotFound("home dir not found".to_string()))
}

fn proxy_base_url(port: u16) -> String {
    format!("http://127.0.0.1:{port}/v1")
}

fn normalize_provider_id(provider_id: &str) -> AppResult<String> {
    let trimmed = provider_id.trim();
    if trimmed.is_empty() {
        return Err(AppError::Command("Provider ID 不能为空".to_string()));
    }
    if trimmed
        .chars()
        .any(|ch| ch.is_control() || ch.is_whitespace())
    {
        return Err(AppError::Command(
            "Provider ID 不能包含空白字符或控制字符".to_string(),
        ));
    }
    Ok(trimmed.to_string())
}

fn discover_candidates_in_home(home: &Path) -> AppResult<ConversationProviderCandidatesView> {
    let config_path = home.join(DEFAULT_CONFIG_PATH);
    let mut candidates = BTreeMap::<String, ConversationProviderCandidate>::new();
    let mut active_provider_id = "openai".to_string();

    let mut files = candidate_config_files(home);
    files.sort_by(|a, b| file_mtime(b).cmp(&file_mtime(a)));
    if let Some(index) = files.iter().position(|path| path == &config_path) {
        let current = files.remove(index);
        files.insert(0, current);
    }

    for path in files.iter() {
        let Ok(raw) = std::fs::read_to_string(path) else {
            continue;
        };
        let Ok(value) = toml::from_str::<toml::Value>(&raw) else {
            continue;
        };
        let source_kind = source_kind_for(home, path, &config_path);
        if path == &config_path {
            active_provider_id = value
                .get("model_provider")
                .and_then(|v| v.as_str())
                .filter(|s| !s.trim().is_empty())
                .unwrap_or("openai")
                .to_string();
        }
        collect_candidates_from_value(
            &mut candidates,
            &value,
            path,
            &source_kind,
            file_mtime_iso(path),
        );
    }

    let openai_candidate = ConversationProviderCandidate {
        provider_id: "openai".to_string(),
        display_name: Some("OpenAI".to_string()),
        original_base_url: current_openai_base_url(&config_path),
        wire_api: "responses".to_string(),
        requires_openai_auth: Some(true),
        source_kind: "current".to_string(),
        source_path: config_path.display().to_string(),
        last_seen_at: file_mtime_iso(&config_path),
        is_builtin_openai: true,
    };
    candidates
        .entry("openai".to_string())
        .or_insert(openai_candidate);

    let mut list: Vec<_> = candidates.into_values().collect();
    list.sort_by(|a, b| {
        (b.source_kind == "current")
            .cmp(&(a.source_kind == "current"))
            .then_with(|| b.last_seen_at.cmp(&a.last_seen_at))
            .then_with(|| a.provider_id.cmp(&b.provider_id))
    });

    Ok(ConversationProviderCandidatesView {
        active_provider_id,
        config_path: config_path.display().to_string(),
        candidates: list,
    })
}

fn candidate_config_files(home: &Path) -> Vec<PathBuf> {
    let codex = home.join(".codex");
    let mut files = Vec::new();
    let current = codex.join("config.toml");
    if current.exists() {
        files.push(current);
    }
    if let Ok(entries) = std::fs::read_dir(&codex) {
        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_file() {
                continue;
            }
            let Some(name) = path.file_name().and_then(|s| s.to_str()) else {
                continue;
            };
            if name.starts_with("config.toml.bak")
                || name.ends_with(".config.toml")
                || name.contains("config.toml.bak")
            {
                files.push(path);
            }
        }
    }
    let backups = codex.join("codex-box/backups");
    if let Ok(entries) = std::fs::read_dir(backups) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_file() && path.extension().and_then(|s| s.to_str()) == Some("toml") {
                files.push(path);
            }
        }
    }
    files.sort();
    files.dedup();
    files
}

fn source_kind_for(home: &Path, path: &Path, config_path: &Path) -> String {
    if path == config_path {
        return "current".to_string();
    }
    if path
        .strip_prefix(home.join(".codex/codex-box/backups"))
        .is_ok()
    {
        return "backup".to_string();
    }
    if path
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("")
        .ends_with(".config.toml")
    {
        return "profile".to_string();
    }
    "backup".to_string()
}

fn file_mtime(path: &Path) -> i64 {
    path.metadata()
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

fn file_mtime_iso(path: &Path) -> String {
    path.metadata()
        .and_then(|m| m.modified())
        .ok()
        .map(DateTime::<Utc>::from)
        .map(|t| t.to_rfc3339())
        .unwrap_or_default()
}

fn current_openai_base_url(config_path: &Path) -> Option<String> {
    let raw = std::fs::read_to_string(config_path).ok()?;
    let value = toml::from_str::<toml::Value>(&raw).ok()?;
    value
        .get("openai_base_url")
        .and_then(|v| v.as_str())
        .map(ToString::to_string)
}

fn collect_candidates_from_value(
    candidates: &mut BTreeMap<String, ConversationProviderCandidate>,
    value: &toml::Value,
    path: &Path,
    source_kind: &str,
    last_seen_at: String,
) {
    if let Some(providers) = value.get("model_providers").and_then(|v| v.as_table()) {
        for (provider_id, entry) in providers {
            let Some(table) = entry.as_table() else {
                continue;
            };
            let candidate = ConversationProviderCandidate {
                provider_id: provider_id.clone(),
                display_name: table
                    .get("name")
                    .and_then(|v| v.as_str())
                    .map(ToString::to_string),
                original_base_url: table
                    .get("base_url")
                    .and_then(|v| v.as_str())
                    .map(ToString::to_string),
                wire_api: table
                    .get("wire_api")
                    .and_then(|v| v.as_str())
                    .unwrap_or("responses")
                    .to_string(),
                requires_openai_auth: table.get("requires_openai_auth").and_then(|v| v.as_bool()),
                source_kind: source_kind.to_string(),
                source_path: path.display().to_string(),
                last_seen_at: last_seen_at.clone(),
                is_builtin_openai: provider_id == "openai",
            };
            candidates.entry(provider_id.clone()).or_insert(candidate);
        }
    }

    if let Some(provider_id) = value
        .get("model_provider")
        .and_then(|v| v.as_str())
        .filter(|s| !s.trim().is_empty())
    {
        candidates
            .entry(provider_id.to_string())
            .or_insert_with(|| ConversationProviderCandidate {
                provider_id: provider_id.to_string(),
                display_name: None,
                original_base_url: None,
                wire_api: "responses".to_string(),
                requires_openai_auth: None,
                source_kind: source_kind.to_string(),
                source_path: path.display().to_string(),
                last_seen_at,
                is_builtin_openai: provider_id == "openai",
            });
    }
}

fn rewrite_conversation_provider(
    raw: &str,
    request: &ConversationProviderRequest,
    catalog_path: &Path,
) -> AppResult<String> {
    let provider_id = normalize_provider_id(&request.provider_id)?;
    let mut value: toml::Value = toml::from_str(raw)?;
    let table = value
        .as_table_mut()
        .ok_or_else(|| AppError::Command("config.toml 顶层不是 table".to_string()))?;

    table.insert(
        "model_provider".to_string(),
        toml::Value::String(provider_id.clone()),
    );
    table.insert(
        "model_catalog_json".to_string(),
        toml::Value::String(catalog_path.display().to_string()),
    );

    if provider_id == "openai" {
        table.insert(
            "openai_base_url".to_string(),
            toml::Value::String(proxy_base_url(request.proxy_port)),
        );
        return toml::to_string_pretty(&value)
            .map_err(|e| AppError::Command(format!("serialize config: {e}")));
    }

    table.remove("openai_base_url");
    let providers = table
        .entry("model_providers".to_string())
        .or_insert_with(|| toml::Value::Table(toml::map::Map::new()));
    let toml::Value::Table(providers_table) = providers else {
        return Err(AppError::Command(
            "model_providers 不是 table,无法写入会话归属 Provider".to_string(),
        ));
    };

    let entry = providers_table
        .entry(provider_id.clone())
        .or_insert_with(|| toml::Value::Table(toml::map::Map::new()));
    let toml::Value::Table(provider_table) = entry else {
        return Err(AppError::Command(format!(
            "model_providers.{provider_id} 不是 table"
        )));
    };

    let display_name = request
        .display_name
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or(&provider_id);
    provider_table.insert(
        "name".to_string(),
        toml::Value::String(display_name.to_string()),
    );
    provider_table.insert(
        "base_url".to_string(),
        toml::Value::String(proxy_base_url(request.proxy_port)),
    );
    provider_table.insert(
        "wire_api".to_string(),
        toml::Value::String(request.wire_api.trim().to_string()),
    );
    provider_table.insert(
        "requires_openai_auth".to_string(),
        toml::Value::Boolean(request.requires_openai_auth),
    );
    provider_table.insert(
        "experimental_bearer_token".to_string(),
        toml::Value::String("dummy".to_string()),
    );
    provider_table.insert("request_max_retries".to_string(), toml::Value::Integer(3));
    provider_table.insert("stream_max_retries".to_string(), toml::Value::Integer(3));
    provider_table.insert(
        "stream_idle_timeout_ms".to_string(),
        toml::Value::Integer(600000),
    );

    toml::to_string_pretty(&value).map_err(|e| AppError::Command(format!("serialize config: {e}")))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn discovers_current_and_backup_provider_candidates_without_hardcoded_ids() {
        let dir = tempdir().unwrap();
        let codex = dir.path().join(".codex");
        let backups = codex.join("codex-box/backups");
        fs::create_dir_all(&backups).unwrap();
        fs::write(
            codex.join("config.toml"),
            r#"
model = "gpt-5.5"

[model_providers.opencodex]
name = "OpenCodex"
base_url = "http://127.0.0.1:8765/v1"
wire_api = "responses"
"#,
        )
        .unwrap();
        fs::write(
            backups.join("old.toml"),
            r#"
model_provider = "codex_local_access"

[model_providers.codex_local_access]
name = "Codex API Service"
base_url = "http://localhost:51232/v1"
wire_api = "responses"
requires_openai_auth = true
"#,
        )
        .unwrap();

        let view = discover_candidates_in_home(dir.path()).unwrap();

        assert_eq!(view.active_provider_id, "openai");
        assert!(view
            .candidates
            .iter()
            .any(|candidate| candidate.provider_id == "openai" && candidate.is_builtin_openai));
        let legacy = view
            .candidates
            .iter()
            .find(|candidate| candidate.provider_id == "codex_local_access")
            .unwrap();
        assert_eq!(legacy.display_name.as_deref(), Some("Codex API Service"));
        assert_eq!(
            legacy.original_base_url.as_deref(),
            Some("http://localhost:51232/v1")
        );
        assert_eq!(legacy.source_kind, "backup");
    }

    #[test]
    fn rewrite_for_builtin_openai_uses_openai_base_url_not_custom_provider_table() {
        let raw = r#"
model = "gpt-5.5"
model_provider = "custom"

[model_providers.custom]
name = "Custom"
base_url = "http://localhost:8888/v1"
wire_api = "responses"
"#;

        let next = rewrite_conversation_provider(
            raw,
            &ConversationProviderRequest {
                provider_id: "openai".to_string(),
                display_name: Some("OpenAI".to_string()),
                proxy_port: 1455,
                wire_api: "responses".to_string(),
                requires_openai_auth: true,
                original_base_url: None,
                expected_hash: None,
            },
            Path::new("/Users/example/.opencodex/custom_model_catalog.json"),
        )
        .unwrap();

        assert!(next.contains("model_provider = \"openai\""));
        assert!(next.contains("openai_base_url = \"http://127.0.0.1:1455/v1\""));
        assert!(next.contains(
            "model_catalog_json = \"/Users/example/.opencodex/custom_model_catalog.json\""
        ));
        assert!(!next.contains("[model_providers.openai]"));
    }

    #[test]
    fn rewrite_for_custom_provider_preserves_id_and_maps_base_url_to_proxy() {
        let raw = r#"
model = "gpt-5.5"
"#;

        let next = rewrite_conversation_provider(
            raw,
            &ConversationProviderRequest {
                provider_id: "codex_local_access".to_string(),
                display_name: Some("Codex API Service".to_string()),
                proxy_port: 1455,
                wire_api: "responses".to_string(),
                requires_openai_auth: true,
                original_base_url: Some("http://localhost:51232/v1".to_string()),
                expected_hash: None,
            },
            Path::new("/Users/example/.opencodex/custom_model_catalog.json"),
        )
        .unwrap();

        assert!(next.contains("model_provider = \"codex_local_access\""));
        assert!(next.contains("[model_providers.codex_local_access]"));
        assert!(next.contains("name = \"Codex API Service\""));
        assert!(next.contains("base_url = \"http://127.0.0.1:1455/v1\""));
        assert!(next.contains("wire_api = \"responses\""));
        assert!(next.contains("requires_openai_auth = true"));
    }
}
