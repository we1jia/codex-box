use crate::config::model::{BackupReason, BackupRecord, DiffLine};
use crate::config::{backup, diff, loader, parser, writer};
use crate::error::{AppError, AppResult};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

const DEFAULT_CONFIG_PATH: &str = ".codex/config.toml";
const BACKUP_DIR: &str = ".codex/codex-box/backups";

#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ConfigChangeRequest {
    AddProvider {
        id: String,
        kind: String,
        #[serde(rename = "baseUrl")]
        base_url: String,
        #[serde(rename = "wireApi")]
        wire_api: String,
        #[serde(rename = "envKey")]
        env_key: String,
        #[serde(default)]
        models: Vec<String>,
    },
    AddProfile {
        name: String,
        model: String,
        #[serde(rename = "providerId")]
        provider_id: String,
        sandbox: String,
        approval: String,
        network: String,
        #[serde(default, rename = "mcpRefs")]
        mcp_refs: Vec<String>,
    },
    SetActiveProfile {
        #[serde(rename = "profileName")]
        profile_name: String,
    },
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ApplyConfigChangeRequest {
    pub change: ConfigChangeRequest,
    pub expected_hash: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConfigChangePreview {
    pub config_path: String,
    pub expected_hash: String,
    pub diff: Vec<DiffLine>,
    pub insertions: usize,
    pub deletions: usize,
    pub requires_confirmation: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ApplyConfigChangeResult {
    pub config_path: String,
    pub backup: BackupRecord,
    pub new_hash: String,
}

#[tauri::command]
pub fn config_change_preview(change: ConfigChangeRequest) -> AppResult<ConfigChangePreview> {
    let path = resolve_config_path()?;
    preview_for_path(&path, &change)
}

#[tauri::command]
pub fn config_change_apply(
    request: ApplyConfigChangeRequest,
) -> AppResult<ApplyConfigChangeResult> {
    let path = resolve_config_path()?;
    apply_for_path(&path, &backup_dir()?, &request)
}

fn resolve_config_path() -> AppResult<PathBuf> {
    let home = dirs::home_dir()
        .ok_or_else(|| AppError::ConfigNotFound("home dir not found".to_string()))?;
    Ok(home.join(DEFAULT_CONFIG_PATH))
}

fn backup_dir() -> AppResult<PathBuf> {
    let home = dirs::home_dir()
        .ok_or_else(|| AppError::ConfigNotFound("home dir not found".to_string()))?;
    Ok(home.join(BACKUP_DIR))
}

fn preview_for_path(path: &Path, change: &ConfigChangeRequest) -> AppResult<ConfigChangePreview> {
    let old = loader::read_raw(path)?;
    let old_hash = loader::metadata(path)?.content_hash;
    let new = apply_change_to_raw(&old, change)?;
    let diff_lines = diff::between(&old, &new);
    let (_, insertions, deletions) = diff::count_by_kind(&diff_lines);

    Ok(ConfigChangePreview {
        config_path: path.to_string_lossy().to_string(),
        expected_hash: old_hash,
        diff: diff_lines,
        insertions,
        deletions,
        requires_confirmation: true,
    })
}

fn apply_for_path(
    path: &Path,
    backup_dir: &Path,
    request: &ApplyConfigChangeRequest,
) -> AppResult<ApplyConfigChangeResult> {
    let old = loader::read_raw(path)?;
    let old_hash = loader::metadata(path)?.content_hash;
    if old_hash != request.expected_hash {
        return Err(AppError::Command(
            "配置文件已变化，请重新预览 diff 后再确认写入。".to_string(),
        ));
    }

    let new = apply_change_to_raw(&old, &request.change)?;
    let backup = backup::create_backup(path, backup_dir, BackupReason::PreWrite)?;

    if let Err(error) = writer::atomic_write(path, &new) {
        rollback_from_backup(path, &backup)?;
        return Err(error);
    }

    let new_hash = loader::metadata(path)?.content_hash;
    Ok(ApplyConfigChangeResult {
        config_path: path.to_string_lossy().to_string(),
        backup,
        new_hash,
    })
}

fn rollback_from_backup(path: &Path, backup: &BackupRecord) -> AppResult<()> {
    let backup_content = std::fs::read_to_string(&backup.file_path)?;
    writer::atomic_write(path, &backup_content)
}

fn apply_change_to_raw(raw: &str, change: &ConfigChangeRequest) -> AppResult<String> {
    match change {
        ConfigChangeRequest::AddProvider { .. } => append_provider(raw, change),
        ConfigChangeRequest::AddProfile { .. } => append_profile(raw, change),
        ConfigChangeRequest::SetActiveProfile { .. } => set_active_profile(raw, change),
    }
}

fn append_provider(raw: &str, change: &ConfigChangeRequest) -> AppResult<String> {
    let ConfigChangeRequest::AddProvider {
        id,
        kind,
        base_url,
        wire_api,
        env_key,
        models,
    } = change
    else {
        unreachable!();
    };
    validate_provider_request(raw, change)?;
    let mut next = ensure_trailing_newline(raw);
    next.push('\n');
    next.push_str(&format!("[model_providers.{}]\n", quoted_key(id)));
    next.push_str(&format!(
        "kind = {}\n",
        quoted_value(&provider_kind_for_config(kind)?)
    ));
    next.push_str(&format!("base_url = {}\n", quoted_value(base_url)));
    next.push_str(&format!("wire_api = {}\n", quoted_value(wire_api)));
    next.push_str(&format!("api_key_env = {}\n", quoted_value(env_key)));
    if !models.is_empty() {
        next.push_str(&format!("models = {}\n", quoted_array(models)));
    }
    Ok(next)
}

fn append_profile(raw: &str, change: &ConfigChangeRequest) -> AppResult<String> {
    let ConfigChangeRequest::AddProfile {
        name,
        model,
        provider_id,
        sandbox,
        approval,
        network,
        mcp_refs,
    } = change
    else {
        unreachable!();
    };
    validate_profile_request(raw, change)?;
    let config = parser::parse(raw)?;
    let mut next = materialize_current_profile(raw, &config, name)?;
    next = ensure_trailing_newline(&next);
    next.push('\n');
    next.push_str(&format!("[profile.{}]\n", quoted_key(name)));
    next.push_str(&format!("model = {}\n", quoted_value(model)));
    next.push_str(&format!("model_provider = {}\n", quoted_value(provider_id)));
    next.push_str(&format!("sandbox_mode = {}\n", quoted_value(sandbox)));
    next.push_str(&format!("approval_policy = {}\n", quoted_value(approval)));
    next.push_str(&format!("network = {}\n", quoted_value(network)));
    if !mcp_refs.is_empty() {
        next.push_str(&format!("mcp_refs = {}\n", quoted_array(mcp_refs)));
    }
    Ok(next)
}

fn materialize_current_profile(
    raw: &str,
    config: &crate::config::model::CodexConfig,
    new_profile_name: &str,
) -> AppResult<String> {
    if !config.profiles.is_empty() {
        return Ok(raw.to_string());
    }

    let Some(model) = top_level_string(config, "model") else {
        return Ok(raw.to_string());
    };
    if model.trim().is_empty() {
        return Ok(raw.to_string());
    }

    let current_name = if new_profile_name == "official-codex" {
        "official-codex-current"
    } else {
        "official-codex"
    };

    let mut next = with_top_level_active_profile(raw, current_name);
    next = ensure_trailing_newline(&next);
    next.push('\n');
    next.push_str(&format!("[profile.{}]\n", quoted_key(current_name)));
    next.push_str(&format!("model = {}\n", quoted_value(&model)));
    next.push_str(&format!(
        "model_provider = {}\n",
        quoted_value(
            &top_level_string(config, "model_provider")
                .unwrap_or_else(|| "codex-subscription".to_string())
        )
    ));
    if let Some(sandbox) = top_level_string(config, "sandbox_mode") {
        next.push_str(&format!("sandbox_mode = {}\n", quoted_value(&sandbox)));
    }
    if let Some(approval) = top_level_string(config, "approval_policy") {
        next.push_str(&format!("approval_policy = {}\n", quoted_value(&approval)));
    }
    if let Some(network) = top_level_string(config, "network_access") {
        next.push_str(&format!("network = {}\n", quoted_value(&network)));
    }
    Ok(next)
}

fn set_active_profile(raw: &str, change: &ConfigChangeRequest) -> AppResult<String> {
    let ConfigChangeRequest::SetActiveProfile { profile_name } = change else {
        unreachable!();
    };
    validate_name(profile_name, "profile name")?;
    let config = parser::parse(raw)?;
    if !config.profiles.iter().any(|p| p.name == *profile_name) {
        return Err(AppError::Command(format!(
            "profile 不存在: {}",
            profile_name
        )));
    }

    let uses_profile_table = uses_singular_profile_table(raw);
    let mut changed_active_profile = false;
    let mut changed_profile = false;
    let mut changed_any = false;
    let mut lines = Vec::new();
    for line in raw.split_inclusive('\n') {
        let trimmed = line.trim_start();
        if trimmed.starts_with("active_profile =")
            || (!uses_profile_table && trimmed.starts_with("profile ="))
        {
            let indent_len = line.len() - trimmed.len();
            let indent = &line[..indent_len];
            let newline = if line.ends_with('\n') { "\n" } else { "" };
            let key = if trimmed.starts_with("active_profile =") {
                changed_active_profile = true;
                "active_profile"
            } else {
                changed_profile = true;
                "profile"
            };
            lines.push(format!(
                "{}{} = {}{}",
                indent,
                key,
                quoted_value(profile_name),
                newline
            ));
            changed_any = true;
        } else {
            lines.push(line.to_string());
        }
    }

    if !changed_any {
        let mut next = String::new();
        next.push_str(&format!(
            "active_profile = {}\n",
            quoted_value(profile_name)
        ));
        next.push_str(raw);
        Ok(next)
    } else if uses_profile_table && !changed_active_profile {
        let mut next = String::new();
        next.push_str(&format!(
            "active_profile = {}\n",
            quoted_value(profile_name)
        ));
        next.push_str(&lines.concat());
        Ok(next)
    } else if !uses_profile_table && !changed_active_profile && changed_profile {
        let mut next = lines.concat();
        next = ensure_trailing_newline(&next);
        next.push_str(&format!(
            "active_profile = {}\n",
            quoted_value(profile_name)
        ));
        Ok(next)
    } else {
        Ok(lines.concat())
    }
}

fn uses_singular_profile_table(raw: &str) -> bool {
    raw.lines().any(|line| {
        let trimmed = line.trim_start();
        trimmed == "[profile]"
            || trimmed.starts_with("[profile.")
            || trimmed.starts_with("[profile\"")
    })
}

fn with_top_level_active_profile(raw: &str, profile_name: &str) -> String {
    let mut next = format!("active_profile = {}\n", quoted_value(profile_name));
    let mut in_top_level = true;
    for line in raw.split_inclusive('\n') {
        let trimmed = line.trim_start();
        if in_top_level && trimmed.starts_with('[') {
            in_top_level = false;
        }
        if in_top_level
            && (trimmed.starts_with("active_profile =") || trimmed.starts_with("profile ="))
        {
            continue;
        }
        next.push_str(line);
    }
    next
}

fn validate_provider_request(raw: &str, change: &ConfigChangeRequest) -> AppResult<()> {
    let ConfigChangeRequest::AddProvider {
        id,
        base_url,
        env_key,
        models,
        ..
    } = change
    else {
        unreachable!();
    };
    validate_name(id, "provider id")?;
    validate_no_secret(env_key)?;
    let config = parser::parse(raw)?;
    if config
        .model_providers
        .iter()
        .any(|provider| provider.name == *id)
    {
        return Err(AppError::Command(format!("model provider 已存在: {}", id)));
    }
    if base_url.trim().is_empty() {
        return Err(AppError::Command("base_url 不能为空".to_string()));
    }
    if env_key.trim().is_empty() {
        return Err(AppError::Command("api_key_env 不能为空".to_string()));
    }
    for model in models {
        validate_name(model, "model")?;
    }
    Ok(())
}

fn validate_profile_request(raw: &str, change: &ConfigChangeRequest) -> AppResult<()> {
    let ConfigChangeRequest::AddProfile {
        name,
        model,
        provider_id,
        ..
    } = change
    else {
        unreachable!();
    };
    validate_name(name, "profile name")?;
    validate_name(model, "model")?;
    validate_name(provider_id, "model_provider")?;
    let config = parser::parse(raw)?;
    if config.profiles.iter().any(|profile| profile.name == *name) {
        return Err(AppError::Command(format!("profile 已存在: {}", name)));
    }
    if !config
        .model_providers
        .iter()
        .any(|provider| provider.name == *provider_id)
        && provider_id != "codex-subscription"
    {
        return Err(AppError::Command(format!(
            "model_provider 不存在: {}",
            provider_id
        )));
    }
    Ok(())
}

fn validate_name(value: &str, label: &str) -> AppResult<()> {
    let trimmed = value.trim();
    if trimmed.is_empty() || trimmed.contains('\n') || trimmed.contains('\r') {
        return Err(AppError::Command(format!("{} 无效", label)));
    }
    Ok(())
}

fn validate_no_secret(value: &str) -> AppResult<()> {
    let lower = value.to_ascii_lowercase();
    let looks_like_secret = lower.contains("sk-")
        || lower.contains("bearer ")
        || lower.starts_with("xox")
        || lower.starts_with("ghp_")
        || value.len() > 80;
    if looks_like_secret {
        Err(AppError::Command(
            "这里只允许填写环境变量名，不能填写明文密钥。".to_string(),
        ))
    } else {
        Ok(())
    }
}

fn top_level_string(config: &crate::config::model::CodexConfig, key: &str) -> Option<String> {
    config
        .top_level
        .get(key)
        .and_then(|value| value.as_str())
        .map(String::from)
}

fn provider_kind_for_config(kind: &str) -> AppResult<String> {
    match kind {
        "subscription" => Ok("openai_subscription".to_string()),
        "official_api" => Ok("openai_official".to_string()),
        "compatible_api" => Ok("openai_compatible".to_string()),
        "local_gateway" => Ok("local_gateway".to_string()),
        _ => Err(AppError::Command(format!("provider kind 不支持: {}", kind))),
    }
}

fn ensure_trailing_newline(raw: &str) -> String {
    if raw.is_empty() || raw.ends_with('\n') {
        raw.to_string()
    } else {
        format!("{raw}\n")
    }
}

fn quoted_key(value: &str) -> String {
    format!("\"{}\"", escape_toml(value))
}

fn quoted_value(value: &str) -> String {
    format!("\"{}\"", escape_toml(value))
}

fn quoted_array(values: &[String]) -> String {
    let items = values
        .iter()
        .map(|value| quoted_value(value))
        .collect::<Vec<_>>()
        .join(", ");
    format!("[{items}]")
}

fn escape_toml(value: &str) -> String {
    value.replace('\\', "\\\\").replace('"', "\\\"")
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn preview_add_provider_has_insert_diff() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("config.toml");
        std::fs::write(&path, "model = \"gpt-5.5\"\n").unwrap();
        let preview = preview_for_path(
            &path,
            &ConfigChangeRequest::AddProvider {
                id: "openrouter".to_string(),
                kind: "compatible_api".to_string(),
                base_url: "https://openrouter.ai/api/v1".to_string(),
                wire_api: "chat".to_string(),
                env_key: "OPENROUTER_API_KEY".to_string(),
                models: vec!["openai/gpt-5-mini".to_string()],
            },
        )
        .unwrap();
        assert!(preview.insertions > 0);
        assert!(preview
            .diff
            .iter()
            .any(|line| line.content.contains("[model_providers.\"openrouter\"]")));
    }

    #[test]
    fn apply_add_provider_creates_backup_and_writes() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("config.toml");
        let backup_dir = dir.path().join("backups");
        std::fs::write(&path, "model = \"gpt-5.5\"\n").unwrap();
        let change = ConfigChangeRequest::AddProvider {
            id: "openrouter".to_string(),
            kind: "compatible_api".to_string(),
            base_url: "https://openrouter.ai/api/v1".to_string(),
            wire_api: "chat".to_string(),
            env_key: "OPENROUTER_API_KEY".to_string(),
            models: vec![],
        };
        let preview = preview_for_path(&path, &change).unwrap();
        let result = apply_for_path(
            &path,
            &backup_dir,
            &ApplyConfigChangeRequest {
                change,
                expected_hash: preview.expected_hash,
            },
        )
        .unwrap();
        assert!(Path::new(&result.backup.file_path).exists());
        assert!(std::fs::read_to_string(&path)
            .unwrap()
            .contains("[model_providers.\"openrouter\"]"));
    }

    #[test]
    fn apply_rejects_stale_hash() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("config.toml");
        let backup_dir = dir.path().join("backups");
        std::fs::write(&path, "model = \"gpt-5.5\"\n").unwrap();
        let result = apply_for_path(
            &path,
            &backup_dir,
            &ApplyConfigChangeRequest {
                change: ConfigChangeRequest::SetActiveProfile {
                    profile_name: "dev".to_string(),
                },
                expected_hash: "wrong".to_string(),
            },
        );
        assert!(result.is_err());
    }

    #[test]
    fn preview_add_profile_binds_provider_and_policy() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("config.toml");
        std::fs::write(
            &path,
            r#"
[model_providers."openrouter"]
kind = "openai_compatible"
base_url = "https://openrouter.ai/api/v1"
wire_api = "chat"
api_key_env = "OPENROUTER_API_KEY"
"#,
        )
        .unwrap();

        let preview = preview_for_path(
            &path,
            &ConfigChangeRequest::AddProfile {
                name: "openrouter-dev".to_string(),
                model: "openai/gpt-5-mini".to_string(),
                provider_id: "openrouter".to_string(),
                sandbox: "workspace-write".to_string(),
                approval: "on-request".to_string(),
                network: "direct".to_string(),
                mcp_refs: vec!["filesystem".to_string(), "git".to_string()],
            },
        )
        .unwrap();

        let diff_text = preview
            .diff
            .iter()
            .map(|line| line.content.as_str())
            .collect::<Vec<_>>()
            .join("\n");
        assert!(diff_text.contains("[profile.\"openrouter-dev\"]"));
        assert!(diff_text.contains("model_provider = \"openrouter\""));
        assert!(diff_text.contains("mcp_refs = [\"filesystem\", \"git\"]"));
    }

    #[test]
    fn add_first_profile_materializes_current_subscription_profile() {
        let raw = r#"
model = "gpt-5-codex"
sandbox_mode = "workspace-write"
approval_policy = "on-request"
network_access = "direct"

[model_providers."openrouter"]
kind = "openai_compatible"
base_url = "https://openrouter.ai/api/v1"
wire_api = "chat"
api_key_env = "OPENROUTER_API_KEY"
"#;

        let next = apply_change_to_raw(
            raw,
            &ConfigChangeRequest::AddProfile {
                name: "router".to_string(),
                model: "openai/gpt-5-mini".to_string(),
                provider_id: "openrouter".to_string(),
                sandbox: "workspace-write".to_string(),
                approval: "on-request".to_string(),
                network: "direct".to_string(),
                mcp_refs: vec![],
            },
        )
        .unwrap();

        assert!(next.starts_with("active_profile = \"official-codex\"\n"));
        assert!(next.contains("[profile.\"official-codex\"]"));
        assert!(next.contains("model_provider = \"codex-subscription\""));
        assert!(next.contains("[profile.\"router\"]"));

        let parsed = parser::parse(&next).expect("generated TOML parses");
        assert!(parsed
            .profiles
            .iter()
            .any(|profile| profile.name == "official-codex" && profile.is_active));
        assert!(parsed
            .profiles
            .iter()
            .any(|profile| profile.name == "router"));
    }

    #[test]
    fn add_first_profile_converts_legacy_profile_scalar_to_active_profile() {
        let raw = r#"
profile = "legacy-current"
model = "gpt-5-codex"

[model_providers."openrouter"]
kind = "openai_compatible"
base_url = "https://openrouter.ai/api/v1"
wire_api = "chat"
api_key_env = "OPENROUTER_API_KEY"
"#;

        let next = apply_change_to_raw(
            raw,
            &ConfigChangeRequest::AddProfile {
                name: "router".to_string(),
                model: "openai/gpt-5-mini".to_string(),
                provider_id: "openrouter".to_string(),
                sandbox: "workspace-write".to_string(),
                approval: "on-request".to_string(),
                network: "direct".to_string(),
                mcp_refs: vec![],
            },
        )
        .unwrap();

        assert!(next.starts_with("active_profile = \"official-codex\"\n"));
        assert!(!next.contains("profile = \"legacy-current\""));
        parser::parse(&next).expect("generated TOML parses");
    }

    #[test]
    fn set_active_profile_updates_active_profile_for_profile_tables() {
        let raw = r#"
active_profile = "official"

[profile.official]
model = "gpt-5-codex"
model_provider = "codex-subscription"

[profile.router]
model = "openai/gpt-5-mini"
model_provider = "openrouter"
"#;

        let next = apply_change_to_raw(
            raw,
            &ConfigChangeRequest::SetActiveProfile {
                profile_name: "router".to_string(),
            },
        )
        .unwrap();

        assert!(next.contains("active_profile = \"router\""));
        assert!(!next.contains("active_profile = \"official\""));
    }

    #[test]
    fn set_active_profile_adds_active_profile_for_profile_tables() {
        let raw = r#"
[profile.official]
model = "gpt-5-codex"
model_provider = "codex-subscription"

[profile.router]
model = "openai/gpt-5-mini"
model_provider = "openrouter"
"#;

        let next = apply_change_to_raw(
            raw,
            &ConfigChangeRequest::SetActiveProfile {
                profile_name: "router".to_string(),
            },
        )
        .unwrap();

        assert!(next.starts_with("active_profile = \"router\"\n"));
        assert!(next.contains("active_profile = \"router\""));
    }
}
