use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use chrono::{SecondsFormat, TimeZone, Utc};
use rusqlite::Connection;
use serde_json::Value;

use crate::config::writer;
use crate::error::{AppError, AppResult};

const DEFAULT_CODEX_CONFIG_FILE: &str = "config.toml";
const DEFAULT_SESSION_INDEX_FILE: &str = "session_index.jsonl";
const DEFAULT_GLOBAL_STATE_FILE: &str = ".codex-global-state.json";
const DEFAULT_TARGET_PROVIDER: &str = "codex_model_router_v2";
const DEFAULT_SOURCE_PROVIDERS: &[&str] = &[
    "openai",
    "custom",
    "codex_local_access",
    "codex_model_router_v2",
    "cc_switch_codex_router",
    "codex_model_router",
];
const DEFAULT_INTERACTIVE_SOURCES: &[&str] = &["cli", "vscode"];
const DEFAULT_FOCUS_ROW_LIMIT: usize = 80;

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CodexHistoryReconcileView {
    pub codex_home: String,
    pub config_path: String,
    pub live_config_model_provider: Option<String>,
    pub suggested_target_provider: String,
    pub source_provider_ids: Vec<String>,
    pub active_state_db_path: Option<String>,
    pub active_state_db_kind: Option<String>,
    pub providers_found: Vec<String>,
    pub sqlite_stores: Vec<CodexHistoryStoreSummary>,
    pub jsonl_summary: CodexHistoryJsonlSummary,
    pub session_index_path: String,
    pub session_index_exists: bool,
    pub global_state_path: String,
    pub global_state_exists: bool,
    pub drift_detected: bool,
    pub provider_rows_to_update: usize,
    pub rollout_provider_lines_to_update: usize,
    pub warnings: Vec<CodexHistoryWarning>,
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CodexHistoryStoreSummary {
    pub path: String,
    pub kind: String,
    pub total: usize,
    pub provider_counts: BTreeMap<String, usize>,
    pub readable: bool,
    pub error: Option<String>,
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CodexHistoryJsonlSummary {
    pub roots: Vec<String>,
    pub total_files: usize,
    pub provider_counts: BTreeMap<String, usize>,
    pub unreadable_files: usize,
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CodexHistoryWarning {
    pub severity: String,
    pub code: String,
    pub message: String,
}

#[derive(Debug, Clone, Default, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CodexHistoryUnifyRequest {
    pub target_provider: Option<String>,
    pub source_provider_ids: Option<Vec<String>>,
    pub project_path: Option<String>,
    pub force: Option<bool>,
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CodexHistoryUnifyPreview {
    pub codex_home: String,
    pub target_provider: String,
    pub source_provider_ids: Vec<String>,
    pub active_state_db_path: Option<String>,
    pub active_state_db_kind: Option<String>,
    pub provider_rows_to_update: usize,
    pub rollout_files_to_update: usize,
    pub rollout_provider_lines_to_update: usize,
    pub user_event_rows_to_update: usize,
    pub visible_candidate_rows: usize,
    pub session_index_missing_to_append: usize,
    pub focus_rows_to_move: usize,
    pub workspace_hints_to_fix: usize,
    pub projectless_ids_to_remove: usize,
    pub saved_workspace_roots_to_add: usize,
    pub session_index_path: String,
    pub session_index_exists: bool,
    pub global_state_path: String,
    pub global_state_exists: bool,
    pub backup_dir: String,
    pub codex_running: bool,
    pub codex_processes: Vec<String>,
    pub can_apply: bool,
    pub warnings: Vec<CodexHistoryWarning>,
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CodexHistoryUnifyApplyResult {
    pub preview: CodexHistoryUnifyPreview,
    pub backup: CodexHistoryBackupSummary,
    pub provider_rows_updated: usize,
    pub rollout_files_updated: usize,
    pub rollout_provider_lines_updated: usize,
    pub user_event_rows_updated: usize,
    pub focus_rows_updated: usize,
    pub session_index_appended: usize,
    pub session_index_rows_moved: usize,
    pub session_index_titles_updated: usize,
    pub workspace_hints_fixed: usize,
    pub projectless_ids_removed: usize,
    pub saved_workspace_roots_added: usize,
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CodexHistoryBackupSummary {
    pub backup_dir: String,
    pub files: Vec<String>,
    pub rollout_manifest_path: String,
}

#[derive(Debug, Clone)]
struct ActiveStateDb {
    path: PathBuf,
    kind: String,
}

#[derive(Debug, Clone)]
struct RolloutRewrite {
    path: PathBuf,
    text: String,
    changed_lines: usize,
}

#[derive(Debug, Clone)]
struct HistoryRow {
    id: String,
    title: String,
    cwd: Option<String>,
    rollout_path: Option<String>,
    model_provider: Option<String>,
    source: Option<String>,
    thread_source: Option<String>,
    archived: i64,
    has_user_event: i64,
    updated_at: i64,
    updated_at_ms: i64,
    preview: Option<String>,
    first_user_message: Option<String>,
}

#[derive(Debug, Clone)]
struct FocusRow {
    id: String,
    title: String,
    cwd: Option<String>,
    updated_at: i64,
    updated_at_ms: i64,
    updated_iso: String,
}

#[derive(Debug, Clone, Default)]
struct HistoryVisibilityPlan {
    provider_update_ids: Vec<String>,
    user_event_update_ids: Vec<String>,
    visible_rows: Vec<HistoryRow>,
    missing_index_rows: Vec<HistoryRow>,
    focus_rows: Vec<FocusRow>,
    workspace_hints_to_fix: usize,
    projectless_ids_to_remove: usize,
    saved_workspace_roots_to_add: usize,
}

#[derive(Debug, Clone)]
struct JsonlEntry {
    value: Option<Value>,
    raw: String,
}

#[derive(Debug, Clone, Default)]
struct GlobalStateUpdateCounts {
    workspace_hints_to_fix: usize,
    projectless_ids_to_remove: usize,
    saved_workspace_roots_to_add: usize,
}

#[derive(Debug, Clone, Default)]
struct SqliteHistoryUpdateCounts {
    provider_rows_updated: usize,
    user_event_rows_updated: usize,
    focus_rows_updated: usize,
}

#[tauri::command]
pub fn codex_history_reconcile() -> AppResult<CodexHistoryReconcileView> {
    let home = dirs::home_dir()
        .ok_or_else(|| AppError::ConfigNotFound("home dir not found".to_string()))?;
    codex_history_reconcile_in_home(&home.join(".codex"))
}

#[tauri::command]
pub fn codex_history_unify_preview(
    request: CodexHistoryUnifyRequest,
) -> AppResult<CodexHistoryUnifyPreview> {
    let home = dirs::home_dir()
        .ok_or_else(|| AppError::ConfigNotFound("home dir not found".to_string()))?;
    codex_history_unify_preview_in_home(&home.join(".codex"), &request)
}

#[tauri::command]
pub fn codex_history_unify_apply(
    request: CodexHistoryUnifyRequest,
) -> AppResult<CodexHistoryUnifyApplyResult> {
    let home = dirs::home_dir()
        .ok_or_else(|| AppError::ConfigNotFound("home dir not found".to_string()))?;
    codex_history_unify_apply_in_home(&home.join(".codex"), &request)
}

fn codex_history_reconcile_in_home(codex_home: &Path) -> AppResult<CodexHistoryReconcileView> {
    let config_path = codex_home.join(DEFAULT_CODEX_CONFIG_FILE);
    let config_text = std::fs::read_to_string(&config_path).unwrap_or_default();
    let live_config_model_provider = parse_top_level_string(&config_text, "model_provider");
    let sqlite_home = parse_top_level_string(&config_text, "sqlite_home")
        .as_deref()
        .and_then(resolve_user_path);
    let env_sqlite_home = std::env::var("CODEX_SQLITE_HOME")
        .ok()
        .and_then(|value| resolve_user_path(&value));
    let active_state = resolve_active_state_db(
        codex_home,
        sqlite_home.as_deref(),
        env_sqlite_home.as_deref(),
    );

    let sqlite_stores = scan_all_state_dbs(
        codex_home,
        sqlite_home.as_deref(),
        env_sqlite_home.as_deref(),
    );
    let jsonl_summary = scan_session_jsonl_providers(codex_home);
    let session_index_path = codex_home.join(DEFAULT_SESSION_INDEX_FILE);
    let global_state_path = codex_home.join(DEFAULT_GLOBAL_STATE_FILE);

    let providers_found = collect_providers(&sqlite_stores, &jsonl_summary);
    let suggested_target_provider = live_config_model_provider
        .clone()
        .unwrap_or_else(|| DEFAULT_TARGET_PROVIDER.to_string());
    let source_provider_ids = providers_found
        .iter()
        .filter(|provider| provider.as_str() != suggested_target_provider)
        .filter(|provider| {
            DEFAULT_SOURCE_PROVIDERS.contains(&provider.as_str())
                || live_config_model_provider
                    .as_deref()
                    .map(|live| provider.as_str() != live)
                    .unwrap_or(true)
        })
        .cloned()
        .collect::<Vec<_>>();

    let active_state_db_path = active_state
        .as_ref()
        .map(|state| state.path.display().to_string());
    let active_state_db_kind = active_state.as_ref().map(|state| state.kind.clone());
    let provider_rows_to_update = active_state
        .as_ref()
        .map(|state| {
            count_sqlite_rows_for_sources(
                &state.path,
                &source_provider_ids,
                &suggested_target_provider,
            )
        })
        .unwrap_or(0);
    let rollout_provider_lines_to_update = count_rollout_lines_for_sources(
        codex_home,
        &source_provider_ids,
        &suggested_target_provider,
    );

    let drift_detected = providers_found
        .iter()
        .any(|provider| provider != &suggested_target_provider);
    let mut warnings = Vec::new();
    if active_state.is_none() {
        warnings.push(history_warning(
            "warn",
            "state_db_missing",
            "未找到 Codex state_5.sqlite，无法判断会话列表真实归属。",
        ));
    }
    if drift_detected {
        warnings.push(history_warning(
            "warn",
            "history_provider_bucket_drift",
            format!(
                "发现多个历史归属 Provider：{}。Codex App 可能按当前 model_provider 只显示其中一部分。",
                providers_found.join(", ")
            ),
        ));
    }
    if provider_rows_to_update > 0 || rollout_provider_lines_to_update > 0 {
        warnings.push(history_warning(
            "warn",
            "history_unify_preview_available",
            format!(
                "如统一到 {suggested_target_provider}，预计需要更新 SQLite {} 行、JSONL {} 行。",
                provider_rows_to_update, rollout_provider_lines_to_update
            ),
        ));
    }

    Ok(CodexHistoryReconcileView {
        codex_home: codex_home.display().to_string(),
        config_path: config_path.display().to_string(),
        live_config_model_provider,
        suggested_target_provider,
        source_provider_ids,
        active_state_db_path,
        active_state_db_kind,
        providers_found,
        sqlite_stores,
        jsonl_summary,
        session_index_path: session_index_path.display().to_string(),
        session_index_exists: session_index_path.exists(),
        global_state_path: global_state_path.display().to_string(),
        global_state_exists: global_state_path.exists(),
        drift_detected,
        provider_rows_to_update,
        rollout_provider_lines_to_update,
        warnings,
    })
}

fn parse_top_level_string(config_text: &str, key: &str) -> Option<String> {
    for line in config_text.lines() {
        let stripped = line.trim();
        if stripped.starts_with('[') {
            break;
        }
        let Some((lhs, rhs)) = stripped.split_once('=') else {
            continue;
        };
        if lhs.trim() != key {
            continue;
        }
        let value = rhs
            .trim()
            .trim_matches('"')
            .trim_matches('\'')
            .trim()
            .to_string();
        if !value.is_empty() {
            return Some(value);
        }
    }
    None
}

fn resolve_user_path(raw: &str) -> Option<PathBuf> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return None;
    }
    if trimmed == "~" {
        return dirs::home_dir();
    }
    if let Some(rest) = trimmed.strip_prefix("~/") {
        return dirs::home_dir().map(|home| home.join(rest));
    }
    Some(PathBuf::from(trimmed))
}

fn codex_history_unify_preview_in_home(
    codex_home: &Path,
    request: &CodexHistoryUnifyRequest,
) -> AppResult<CodexHistoryUnifyPreview> {
    let reconcile = codex_history_reconcile_in_home(codex_home)?;
    let target_provider = request
        .target_provider
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToString::to_string)
        .unwrap_or_else(|| reconcile.suggested_target_provider.clone());
    let source_provider_ids = normalize_source_provider_ids(
        request.source_provider_ids.as_deref(),
        &reconcile.providers_found,
        &target_provider,
    );
    let active_state = active_state_from_reconcile(&reconcile);
    let visibility_plan = active_state
        .as_ref()
        .map(|state| {
            build_history_visibility_plan(
                codex_home,
                state,
                &source_provider_ids,
                &target_provider,
                request.project_path.as_deref(),
            )
        })
        .transpose()?
        .unwrap_or_default();
    let provider_rows_to_update = visibility_plan.provider_update_ids.len();
    let rollout_rewrites =
        collect_rollout_rewrites(codex_home, &source_provider_ids, &target_provider);
    let codex_processes = detect_running_codex_processes();
    let codex_running = !codex_processes.is_empty();
    let mut warnings = Vec::new();

    if active_state.is_none() {
        warnings.push(history_warning(
            "fail",
            "state_db_missing",
            "未找到可写入的 Codex state_5.sqlite，无法执行历史统一。",
        ));
    }
    if source_provider_ids.is_empty() {
        warnings.push(history_warning(
            "warn",
            "history_sources_empty",
            "没有找到需要迁移的源 Provider。",
        ));
    }
    if codex_running && !request.force.unwrap_or(false) {
        warnings.push(history_warning(
            "fail",
            "codex_running",
            "检测到 Codex Desktop/app-server 正在运行。为避免破坏当前索引，历史统一默认拒绝写入；请完全退出 Codex 后再执行。",
        ));
    }
    let total_planned_changes = provider_rows_to_update
        + rollout_rewrites.len()
        + visibility_plan.user_event_update_ids.len()
        + visibility_plan.missing_index_rows.len()
        + visibility_plan.focus_rows.len()
        + visibility_plan.workspace_hints_to_fix
        + visibility_plan.projectless_ids_to_remove
        + visibility_plan.saved_workspace_roots_to_add;
    if total_planned_changes == 0 {
        warnings.push(history_warning(
            "ok",
            "history_already_unified",
            format!("历史归属已统一到 {target_provider}，无需写入。"),
        ));
    }

    Ok(CodexHistoryUnifyPreview {
        codex_home: codex_home.display().to_string(),
        target_provider,
        source_provider_ids,
        active_state_db_path: active_state
            .as_ref()
            .map(|state| state.path.display().to_string()),
        active_state_db_kind: active_state.as_ref().map(|state| state.kind.clone()),
        provider_rows_to_update,
        rollout_files_to_update: rollout_rewrites.len(),
        rollout_provider_lines_to_update: rollout_rewrites
            .iter()
            .map(|rewrite| rewrite.changed_lines)
            .sum(),
        user_event_rows_to_update: visibility_plan.user_event_update_ids.len(),
        visible_candidate_rows: visibility_plan.visible_rows.len(),
        session_index_missing_to_append: visibility_plan.missing_index_rows.len(),
        focus_rows_to_move: visibility_plan.focus_rows.len(),
        workspace_hints_to_fix: visibility_plan.workspace_hints_to_fix,
        projectless_ids_to_remove: visibility_plan.projectless_ids_to_remove,
        saved_workspace_roots_to_add: visibility_plan.saved_workspace_roots_to_add,
        session_index_path: codex_home
            .join(DEFAULT_SESSION_INDEX_FILE)
            .display()
            .to_string(),
        session_index_exists: codex_home.join(DEFAULT_SESSION_INDEX_FILE).exists(),
        global_state_path: codex_home
            .join(DEFAULT_GLOBAL_STATE_FILE)
            .display()
            .to_string(),
        global_state_exists: codex_home.join(DEFAULT_GLOBAL_STATE_FILE).exists(),
        backup_dir: default_history_backup_dir(codex_home).display().to_string(),
        codex_running,
        codex_processes,
        can_apply: active_state.is_some()
            && (!codex_running || request.force.unwrap_or(false))
            && total_planned_changes > 0,
        warnings,
    })
}

fn codex_history_unify_apply_in_home(
    codex_home: &Path,
    request: &CodexHistoryUnifyRequest,
) -> AppResult<CodexHistoryUnifyApplyResult> {
    let preview = codex_history_unify_preview_in_home(codex_home, request)?;
    if preview.codex_running && !request.force.unwrap_or(false) {
        return Err(AppError::Command(
            "Codex Desktop/app-server is running; quit Codex first before applying history unify."
                .to_string(),
        ));
    }
    if !preview.can_apply {
        return Err(AppError::Command(
            "History unify has nothing safe to apply.".to_string(),
        ));
    }

    let active_state = active_state_from_preview(&preview).ok_or_else(|| {
        AppError::ConfigNotFound("active Codex state_5.sqlite not found".to_string())
    })?;
    let rollout_rewrites = collect_rollout_rewrites(
        codex_home,
        &preview.source_provider_ids,
        &preview.target_provider,
    );
    let visibility_plan = build_history_visibility_plan(
        codex_home,
        &active_state,
        &preview.source_provider_ids,
        &preview.target_provider,
        request.project_path.as_deref(),
    )?;
    let backup = create_history_backup(codex_home, &active_state, &rollout_rewrites, &preview)?;
    let sqlite_update = apply_sqlite_history_updates(
        &active_state.path,
        &preview.target_provider,
        &visibility_plan.provider_update_ids,
        &visibility_plan.user_event_update_ids,
        &visibility_plan.focus_rows,
    )?;
    let (rollout_files_updated, rollout_provider_lines_updated) =
        apply_rollout_rewrites(&rollout_rewrites)?;
    let session_index_path = codex_home.join(DEFAULT_SESSION_INDEX_FILE);
    let session_index_appended = append_missing_session_index_rows(
        &session_index_path,
        &visibility_plan.missing_index_rows,
    )?;
    let (session_index_rows_moved, session_index_titles_updated) =
        move_focus_session_index_rows(&session_index_path, &visibility_plan.focus_rows)?;
    let global_state_update = update_global_state(
        &codex_home.join(DEFAULT_GLOBAL_STATE_FILE),
        &visibility_plan.visible_rows,
        &visibility_plan.focus_rows,
        request.project_path.as_deref(),
        true,
    )?;

    Ok(CodexHistoryUnifyApplyResult {
        preview,
        backup,
        provider_rows_updated: sqlite_update.provider_rows_updated,
        rollout_files_updated,
        rollout_provider_lines_updated,
        user_event_rows_updated: sqlite_update.user_event_rows_updated,
        focus_rows_updated: sqlite_update.focus_rows_updated,
        session_index_appended,
        session_index_rows_moved,
        session_index_titles_updated,
        workspace_hints_fixed: global_state_update.workspace_hints_to_fix,
        projectless_ids_removed: global_state_update.projectless_ids_to_remove,
        saved_workspace_roots_added: global_state_update.saved_workspace_roots_to_add,
    })
}

fn normalize_source_provider_ids(
    requested: Option<&[String]>,
    providers_found: &[String],
    target_provider: &str,
) -> Vec<String> {
    let mut sources = BTreeSet::new();
    let candidates = requested
        .filter(|items| !items.is_empty())
        .map(|items| items.to_vec())
        .unwrap_or_else(|| providers_found.to_vec());
    for provider in candidates {
        let provider = provider.trim();
        if provider.is_empty() || provider == target_provider {
            continue;
        }
        sources.insert(provider.to_string());
    }
    sources.into_iter().collect()
}

fn active_state_from_reconcile(reconcile: &CodexHistoryReconcileView) -> Option<ActiveStateDb> {
    Some(ActiveStateDb {
        path: PathBuf::from(reconcile.active_state_db_path.as_deref()?),
        kind: reconcile
            .active_state_db_kind
            .clone()
            .unwrap_or_else(|| "unknown".to_string()),
    })
}

fn active_state_from_preview(preview: &CodexHistoryUnifyPreview) -> Option<ActiveStateDb> {
    Some(ActiveStateDb {
        path: PathBuf::from(preview.active_state_db_path.as_deref()?),
        kind: preview
            .active_state_db_kind
            .clone()
            .unwrap_or_else(|| "unknown".to_string()),
    })
}

fn default_history_backup_dir(codex_home: &Path) -> PathBuf {
    let stamp = Utc::now().format("%Y%m%dT%H%M%S%.3fZ");
    codex_home
        .join("codex-box/backups")
        .join(format!("history-unify-{stamp}"))
}

fn build_history_visibility_plan(
    codex_home: &Path,
    active_state: &ActiveStateDb,
    source_provider_ids: &[String],
    target_provider: &str,
    project_path: Option<&str>,
) -> AppResult<HistoryVisibilityPlan> {
    let rows = load_history_rows(&active_state.path)?;
    let source_set = source_provider_ids
        .iter()
        .map(String::as_str)
        .collect::<BTreeSet<_>>();
    let provider_update_ids = rows
        .iter()
        .filter(|row| {
            row.model_provider
                .as_deref()
                .map(|provider| provider != target_provider && source_set.contains(provider))
                .unwrap_or(false)
        })
        .map(|row| row.id.clone())
        .collect::<BTreeSet<_>>();

    let mut visible_rows = Vec::new();
    let mut user_event_update_ids = Vec::new();
    for row in rows {
        let provider_after = if provider_update_ids.contains(&row.id) {
            Some(target_provider)
        } else {
            row.model_provider.as_deref()
        };
        if provider_after != Some(target_provider) {
            continue;
        }
        if row.archived != 0 || !is_interactive_history_row(&row) {
            continue;
        }
        let has_user =
            row.has_user_event == 1 || rollout_has_user_event(row.rollout_path.as_deref());
        if row.has_user_event != 1 && has_user {
            user_event_update_ids.push(row.id.clone());
        }
        if has_user && visible_text(&row) {
            visible_rows.push(row);
        }
    }

    visible_rows.sort_by(|a, b| {
        b.updated_at_ms
            .cmp(&a.updated_at_ms)
            .then_with(|| b.id.cmp(&a.id))
    });
    let existing_index_ids =
        existing_session_index_ids(&codex_home.join(DEFAULT_SESSION_INDEX_FILE))?;
    let missing_index_rows = visible_rows
        .iter()
        .filter(|row| !existing_index_ids.contains(&row.id))
        .cloned()
        .collect::<Vec<_>>();
    let mut focus_rows = visible_rows
        .iter()
        .take(DEFAULT_FOCUS_ROW_LIMIT)
        .map(focus_from_history_row)
        .collect::<Vec<_>>();
    assign_focus_times(&mut focus_rows);
    let global_state = update_global_state(
        &codex_home.join(DEFAULT_GLOBAL_STATE_FILE),
        &visible_rows,
        &focus_rows,
        project_path,
        false,
    )?;

    Ok(HistoryVisibilityPlan {
        provider_update_ids: provider_update_ids.into_iter().collect(),
        user_event_update_ids,
        visible_rows,
        missing_index_rows,
        focus_rows,
        workspace_hints_to_fix: global_state.workspace_hints_to_fix,
        projectless_ids_to_remove: global_state.projectless_ids_to_remove,
        saved_workspace_roots_to_add: global_state.saved_workspace_roots_to_add,
    })
}

fn load_history_rows(path: &Path) -> AppResult<Vec<HistoryRow>> {
    let connection = Connection::open(path)
        .map_err(|error| AppError::Command(format!("open SQLite {}: {error}", path.display())))?;
    let columns = sqlite_table_columns(&connection, "threads")?;
    if !columns.contains("id") || !columns.contains("model_provider") {
        return Err(AppError::Command(
            "threads table missing required id/model_provider columns".to_string(),
        ));
    }
    let mut statement = connection
        .prepare("SELECT * FROM threads")
        .map_err(|error| AppError::Command(format!("read threads table: {error}")))?;
    let rows = statement
        .query_map([], |row| Ok(history_row_from_sql(row, &columns)))
        .map_err(|error| AppError::Command(format!("scan threads table: {error}")))?;
    let mut result = Vec::new();
    for row in rows {
        result.push(row.map_err(|error| AppError::Command(format!("read thread row: {error}")))?);
    }
    Ok(result)
}

fn sqlite_table_columns(connection: &Connection, table: &str) -> AppResult<BTreeSet<String>> {
    let mut statement = connection
        .prepare(&format!("PRAGMA table_info({table})"))
        .map_err(|error| AppError::Command(format!("inspect SQLite table {table}: {error}")))?;
    let rows = statement
        .query_map([], |row| row.get::<_, String>(1))
        .map_err(|error| AppError::Command(format!("inspect SQLite columns: {error}")))?;
    let mut columns = BTreeSet::new();
    for row in rows {
        columns.insert(
            row.map_err(|error| AppError::Command(format!("read SQLite column: {error}")))?,
        );
    }
    Ok(columns)
}

fn history_row_from_sql(row: &rusqlite::Row<'_>, columns: &BTreeSet<String>) -> HistoryRow {
    let updated_at = sql_i64(row, columns, "updated_at").unwrap_or_default();
    let updated_at_ms = sql_i64(row, columns, "updated_at_ms").unwrap_or(updated_at * 1000);
    let title = row_title(
        sql_string(row, columns, "title"),
        sql_string(row, columns, "preview"),
        sql_string(row, columns, "first_user_message"),
    );
    HistoryRow {
        id: sql_string(row, columns, "id").unwrap_or_default(),
        title,
        cwd: sql_string(row, columns, "cwd"),
        rollout_path: sql_string(row, columns, "rollout_path"),
        model_provider: sql_string(row, columns, "model_provider"),
        source: sql_string(row, columns, "source"),
        thread_source: sql_string(row, columns, "thread_source"),
        archived: sql_i64(row, columns, "archived").unwrap_or_default(),
        has_user_event: sql_i64(row, columns, "has_user_event").unwrap_or_default(),
        updated_at,
        updated_at_ms,
        preview: sql_string(row, columns, "preview"),
        first_user_message: sql_string(row, columns, "first_user_message"),
    }
}

fn sql_string(row: &rusqlite::Row<'_>, columns: &BTreeSet<String>, key: &str) -> Option<String> {
    if !columns.contains(key) {
        return None;
    }
    row.get::<_, Option<String>>(key).ok().flatten()
}

fn sql_i64(row: &rusqlite::Row<'_>, columns: &BTreeSet<String>, key: &str) -> Option<i64> {
    if !columns.contains(key) {
        return None;
    }
    row.get::<_, Option<i64>>(key).ok().flatten()
}

fn row_title(
    title: Option<String>,
    preview: Option<String>,
    first_user_message: Option<String>,
) -> String {
    for value in [title, preview, first_user_message] {
        if let Some(value) = value {
            let trimmed = value.trim();
            if !trimmed.is_empty() {
                return trimmed.to_string();
            }
        }
    }
    "Untitled".to_string()
}

fn is_interactive_history_row(row: &HistoryRow) -> bool {
    let source_ok = row
        .source
        .as_deref()
        .map(|source| source.is_empty() || DEFAULT_INTERACTIVE_SOURCES.contains(&source))
        .unwrap_or(true);
    let thread_source_ok = row
        .thread_source
        .as_deref()
        .map(|source| source.is_empty() || source == "user")
        .unwrap_or(true);
    source_ok && thread_source_ok
}

fn visible_text(row: &HistoryRow) -> bool {
    [
        Some(row.title.as_str()),
        row.preview.as_deref(),
        row.first_user_message.as_deref(),
    ]
    .into_iter()
    .flatten()
    .any(|value| !value.trim().is_empty())
}

fn rollout_has_user_event(raw_path: Option<&str>) -> bool {
    let Some(raw_path) = raw_path else {
        return false;
    };
    let path = PathBuf::from(raw_path);
    let Ok(text) = fs::read_to_string(path) else {
        return false;
    };
    text.lines()
        .filter_map(|line| serde_json::from_str::<Value>(line).ok())
        .any(|value| json_value_has_user_event(&value))
}

fn json_value_has_user_event(value: &Value) -> bool {
    match value {
        Value::Object(object) => {
            let role_is_user = object
                .get("role")
                .and_then(Value::as_str)
                .map(|role| role.eq_ignore_ascii_case("user"))
                .unwrap_or(false);
            if role_is_user {
                return true;
            }
            let type_is_user = object
                .get("type")
                .and_then(Value::as_str)
                .map(|kind| matches!(kind, "user_message" | "user_input" | "user"))
                .unwrap_or(false);
            if type_is_user {
                return true;
            }
            object.values().any(json_value_has_user_event)
        }
        Value::Array(items) => items.iter().any(json_value_has_user_event),
        _ => false,
    }
}

fn focus_from_history_row(row: &HistoryRow) -> FocusRow {
    FocusRow {
        id: row.id.clone(),
        title: row.title.clone(),
        cwd: row.cwd.clone(),
        updated_at: row.updated_at,
        updated_at_ms: row.updated_at_ms,
        updated_iso: iso_from_ms(row.updated_at_ms),
    }
}

fn assign_focus_times(rows: &mut [FocusRow]) {
    if rows.is_empty() {
        return;
    }
    let max_existing = rows
        .iter()
        .map(|row| row.updated_at_ms)
        .max()
        .unwrap_or_default();
    let base = Utc::now().timestamp_millis().max(max_existing) + 10_000;
    let total = rows.len() as i64;
    for (index, row) in rows.iter_mut().enumerate() {
        let ms = base + (total - index as i64) * 250;
        row.updated_at_ms = ms;
        row.updated_at = ms / 1000;
        row.updated_iso = iso_from_ms(ms);
    }
}

fn iso_from_ms(ms: i64) -> String {
    Utc.timestamp_millis_opt(ms)
        .single()
        .unwrap_or_else(Utc::now)
        .to_rfc3339_opts(SecondsFormat::Millis, true)
}

fn normalize_workspace_path(value: Option<&str>) -> Option<String> {
    let mut text = value?.trim().to_string();
    if text.is_empty() {
        return None;
    }
    if let Some(rest) = text.strip_prefix("\\\\?\\") {
        text = rest.to_string();
    }
    while text.ends_with('/') || text.ends_with('\\') {
        text.pop();
    }
    if text.is_empty() {
        None
    } else {
        Some(text)
    }
}

fn resolve_active_state_db(
    codex_home: &Path,
    sqlite_home: Option<&Path>,
    env_sqlite_home: Option<&Path>,
) -> Option<ActiveStateDb> {
    let sqlite_default = codex_home.join("sqlite/state_5.sqlite");
    if sqlite_default.exists() {
        return Some(ActiveStateDb {
            path: sqlite_default,
            kind: "sqlite_subdir".to_string(),
        });
    }
    if let Some(sqlite_home) = sqlite_home {
        let path = sqlite_home.join("state_5.sqlite");
        if path.exists() {
            return Some(ActiveStateDb {
                path,
                kind: "configured_sqlite_home".to_string(),
            });
        }
    } else if let Some(sqlite_home) = env_sqlite_home {
        let path = sqlite_home.join("state_5.sqlite");
        if path.exists() {
            return Some(ActiveStateDb {
                path,
                kind: "env_sqlite_home".to_string(),
            });
        }
    }
    let legacy = codex_home.join("state_5.sqlite");
    if legacy.exists() {
        return Some(ActiveStateDb {
            path: legacy,
            kind: "legacy_root".to_string(),
        });
    }
    None
}

fn state_db_search_dirs(
    codex_home: &Path,
    sqlite_home: Option<&Path>,
    env_sqlite_home: Option<&Path>,
) -> Vec<(String, PathBuf)> {
    let mut dirs = vec![
        ("root".to_string(), codex_home.to_path_buf()),
        ("sqlite_subdir".to_string(), codex_home.join("sqlite")),
    ];
    if let Some(sqlite_home) = sqlite_home {
        dirs.push((
            "configured_sqlite_home".to_string(),
            sqlite_home.to_path_buf(),
        ));
    } else if let Some(sqlite_home) = env_sqlite_home {
        dirs.push(("env_sqlite_home".to_string(), sqlite_home.to_path_buf()));
    }
    dirs
}

fn scan_all_state_dbs(
    codex_home: &Path,
    sqlite_home: Option<&Path>,
    env_sqlite_home: Option<&Path>,
) -> Vec<CodexHistoryStoreSummary> {
    let mut seen = BTreeSet::new();
    let mut stores = Vec::new();
    for (kind, directory) in state_db_search_dirs(codex_home, sqlite_home, env_sqlite_home) {
        let Ok(entries) = std::fs::read_dir(&directory) else {
            continue;
        };
        let mut db_paths = entries
            .filter_map(Result::ok)
            .map(|entry| entry.path())
            .filter(|path| {
                path.file_name()
                    .and_then(|name| name.to_str())
                    .map(|name| name.starts_with("state_") && name.ends_with(".sqlite"))
                    .unwrap_or(false)
            })
            .collect::<Vec<_>>();
        db_paths.sort();
        for db_path in db_paths {
            let canonical = db_path.canonicalize().unwrap_or_else(|_| db_path.clone());
            if !seen.insert(canonical.clone()) {
                continue;
            }
            stores.push(scan_state_db(&canonical, &kind));
        }
    }
    stores
}

fn scan_state_db(path: &Path, kind: &str) -> CodexHistoryStoreSummary {
    match scan_state_db_counts(path) {
        Ok((total, provider_counts)) => CodexHistoryStoreSummary {
            path: path.display().to_string(),
            kind: kind.to_string(),
            total,
            provider_counts,
            readable: true,
            error: None,
        },
        Err(error) => CodexHistoryStoreSummary {
            path: path.display().to_string(),
            kind: kind.to_string(),
            total: 0,
            provider_counts: BTreeMap::new(),
            readable: false,
            error: Some(error),
        },
    }
}

fn scan_state_db_counts(path: &Path) -> Result<(usize, BTreeMap<String, usize>), String> {
    let connection = Connection::open(path).map_err(|error| error.to_string())?;
    let has_threads: bool = connection
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name='threads')",
            [],
            |row| row.get::<_, i64>(0),
        )
        .map_err(|error| error.to_string())?
        != 0;
    if !has_threads {
        return Err("threads table not found".to_string());
    }

    let mut statement = connection
        .prepare("SELECT model_provider, COUNT(*) AS count FROM threads GROUP BY model_provider")
        .map_err(|error| error.to_string())?;
    let rows = statement
        .query_map([], |row| {
            let provider: Option<String> = row.get(0)?;
            let count: i64 = row.get(1)?;
            Ok((provider.unwrap_or_default(), count.max(0) as usize))
        })
        .map_err(|error| error.to_string())?;
    let mut provider_counts = BTreeMap::new();
    let mut total = 0;
    for row in rows {
        let (provider, count) = row.map_err(|error| error.to_string())?;
        if provider.trim().is_empty() {
            continue;
        }
        total += count;
        provider_counts.insert(provider, count);
    }
    Ok((total, provider_counts))
}

fn scan_session_jsonl_providers(codex_home: &Path) -> CodexHistoryJsonlSummary {
    let roots = ["sessions", "archived_sessions"]
        .iter()
        .map(|name| codex_home.join(name))
        .collect::<Vec<_>>();
    let mut provider_counts = BTreeMap::new();
    let mut total_files = 0;
    let mut unreadable_files = 0;
    for root in &roots {
        scan_jsonl_root(
            root,
            &mut total_files,
            &mut unreadable_files,
            &mut provider_counts,
        );
    }
    CodexHistoryJsonlSummary {
        roots: roots
            .iter()
            .map(|path| path.display().to_string())
            .collect(),
        total_files,
        provider_counts,
        unreadable_files,
    }
}

fn scan_jsonl_root(
    root: &Path,
    total_files: &mut usize,
    unreadable_files: &mut usize,
    provider_counts: &mut BTreeMap<String, usize>,
) {
    let Ok(entries) = std::fs::read_dir(root) else {
        return;
    };
    for entry in entries.filter_map(Result::ok) {
        let path = entry.path();
        if path.is_dir() {
            scan_jsonl_root(&path, total_files, unreadable_files, provider_counts);
            continue;
        }
        if path.extension().and_then(|ext| ext.to_str()) != Some("jsonl") {
            continue;
        }
        *total_files += 1;
        match first_line_provider(&path) {
            Ok(Some(provider)) => {
                *provider_counts.entry(provider).or_insert(0) += 1;
            }
            Ok(None) => {}
            Err(_) => {
                *unreadable_files += 1;
            }
        }
    }
}

fn first_line_provider(path: &Path) -> Result<Option<String>, String> {
    let text = std::fs::read_to_string(path).map_err(|error| error.to_string())?;
    let Some(first_line) = text.lines().next() else {
        return Ok(None);
    };
    let value = serde_json::from_str::<Value>(first_line).map_err(|error| error.to_string())?;
    let provider = value
        .get("payload")
        .and_then(|payload| payload.as_object())
        .and_then(|payload| payload.get("model_provider"))
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToString::to_string);
    Ok(provider)
}

fn collect_providers(
    sqlite_stores: &[CodexHistoryStoreSummary],
    jsonl_summary: &CodexHistoryJsonlSummary,
) -> Vec<String> {
    let mut providers = BTreeSet::new();
    for store in sqlite_stores {
        providers.extend(store.provider_counts.keys().cloned());
    }
    providers.extend(jsonl_summary.provider_counts.keys().cloned());
    providers.into_iter().collect()
}

fn count_sqlite_rows_for_sources(
    path: &Path,
    source_provider_ids: &[String],
    target_provider: &str,
) -> usize {
    if source_provider_ids.is_empty() {
        return 0;
    }
    let Ok(connection) = Connection::open(path) else {
        return 0;
    };
    let placeholders = std::iter::repeat("?")
        .take(source_provider_ids.len())
        .collect::<Vec<_>>()
        .join(",");
    let sql = format!(
        "SELECT COUNT(*) FROM threads WHERE model_provider IN ({placeholders}) AND model_provider != ?"
    );
    let mut params = source_provider_ids
        .iter()
        .map(String::as_str)
        .collect::<Vec<_>>();
    params.push(target_provider);
    connection
        .query_row(&sql, rusqlite::params_from_iter(params), |row| {
            row.get::<_, i64>(0)
        })
        .map(|count| count.max(0) as usize)
        .unwrap_or(0)
}

fn count_rollout_lines_for_sources(
    codex_home: &Path,
    source_provider_ids: &[String],
    target_provider: &str,
) -> usize {
    if source_provider_ids.is_empty() {
        return 0;
    }
    let sources = source_provider_ids
        .iter()
        .map(String::as_str)
        .collect::<BTreeSet<_>>();
    let roots = [
        codex_home.join("sessions"),
        codex_home.join("archived_sessions"),
    ];
    let mut count = 0;
    for root in roots {
        count += count_rollout_lines_in_root(&root, &sources, target_provider);
    }
    count
}

fn count_rollout_lines_in_root(
    root: &Path,
    source_provider_ids: &BTreeSet<&str>,
    target_provider: &str,
) -> usize {
    let Ok(entries) = std::fs::read_dir(root) else {
        return 0;
    };
    let mut count = 0;
    for entry in entries.filter_map(Result::ok) {
        let path = entry.path();
        if path.is_dir() {
            count += count_rollout_lines_in_root(&path, source_provider_ids, target_provider);
            continue;
        }
        if path.extension().and_then(|ext| ext.to_str()) != Some("jsonl") {
            continue;
        }
        count += count_rollout_lines_in_file(&path, source_provider_ids, target_provider);
    }
    count
}

fn count_rollout_lines_in_file(
    path: &Path,
    source_provider_ids: &BTreeSet<&str>,
    target_provider: &str,
) -> usize {
    let Ok(text) = std::fs::read_to_string(path) else {
        return 0;
    };
    text.lines()
        .filter_map(|line| serde_json::from_str::<Value>(line).ok())
        .filter_map(|value| {
            value
                .get("payload")
                .and_then(|payload| payload.as_object())
                .and_then(|payload| payload.get("model_provider"))
                .and_then(|value| value.as_str())
                .map(ToString::to_string)
        })
        .filter(|provider| {
            provider != target_provider && source_provider_ids.contains(provider.as_str())
        })
        .count()
}

fn collect_rollout_rewrites(
    codex_home: &Path,
    source_provider_ids: &[String],
    target_provider: &str,
) -> Vec<RolloutRewrite> {
    if source_provider_ids.is_empty() {
        return Vec::new();
    }
    let sources = source_provider_ids
        .iter()
        .map(String::as_str)
        .collect::<BTreeSet<_>>();
    let mut rewrites = Vec::new();
    for root in [
        codex_home.join("sessions"),
        codex_home.join("archived_sessions"),
    ] {
        collect_rollout_rewrites_in_root(&root, &sources, target_provider, &mut rewrites);
    }
    rewrites
}

fn collect_rollout_rewrites_in_root(
    root: &Path,
    source_provider_ids: &BTreeSet<&str>,
    target_provider: &str,
    rewrites: &mut Vec<RolloutRewrite>,
) {
    let Ok(entries) = fs::read_dir(root) else {
        return;
    };
    for entry in entries.filter_map(Result::ok) {
        let path = entry.path();
        if path.is_dir() {
            collect_rollout_rewrites_in_root(&path, source_provider_ids, target_provider, rewrites);
            continue;
        }
        if path.extension().and_then(|ext| ext.to_str()) != Some("jsonl") {
            continue;
        }
        if let Ok(Some(rewrite)) = rewrite_rollout_text(&path, source_provider_ids, target_provider)
        {
            rewrites.push(rewrite);
        }
    }
}

fn rewrite_rollout_text(
    path: &Path,
    source_provider_ids: &BTreeSet<&str>,
    target_provider: &str,
) -> Result<Option<RolloutRewrite>, String> {
    let raw = fs::read_to_string(path).map_err(|error| error.to_string())?;
    let had_trailing_newline = raw.ends_with('\n');
    let mut changed_lines = 0;
    let mut lines = Vec::new();
    for line in raw.lines() {
        let Ok(mut value) = serde_json::from_str::<Value>(line) else {
            lines.push(line.to_string());
            continue;
        };
        let provider = value
            .get("payload")
            .and_then(|payload| payload.as_object())
            .and_then(|payload| payload.get("model_provider"))
            .and_then(|value| value.as_str())
            .map(ToString::to_string);
        if provider
            .as_deref()
            .map(|provider| provider != target_provider && source_provider_ids.contains(provider))
            .unwrap_or(false)
        {
            if let Some(payload) = value
                .get_mut("payload")
                .and_then(|payload| payload.as_object_mut())
            {
                payload.insert(
                    "model_provider".to_string(),
                    Value::String(target_provider.to_string()),
                );
                changed_lines += 1;
                lines.push(serde_json::to_string(&value).map_err(|error| error.to_string())?);
                continue;
            }
        }
        lines.push(line.to_string());
    }
    if changed_lines == 0 {
        return Ok(None);
    }
    let mut text = lines.join("\n");
    if had_trailing_newline {
        text.push('\n');
    }
    Ok(Some(RolloutRewrite {
        path: path.to_path_buf(),
        text,
        changed_lines,
    }))
}

fn read_jsonl_entries(path: &Path) -> AppResult<Vec<JsonlEntry>> {
    if !path.exists() {
        return Ok(Vec::new());
    }
    let raw = fs::read_to_string(path).map_err(AppError::Io)?;
    Ok(raw
        .lines()
        .map(|line| JsonlEntry {
            value: serde_json::from_str::<Value>(line).ok(),
            raw: line.to_string(),
        })
        .collect())
}

fn write_jsonl_entries(path: &Path, entries: &[JsonlEntry]) -> AppResult<()> {
    let mut text = entries
        .iter()
        .map(|entry| {
            entry
                .value
                .as_ref()
                .and_then(|value| serde_json::to_string(value).ok())
                .unwrap_or_else(|| entry.raw.clone())
        })
        .collect::<Vec<_>>()
        .join("\n");
    if !entries.is_empty() {
        text.push('\n');
    }
    writer::atomic_write(path, &text)
}

fn existing_session_index_ids(path: &Path) -> AppResult<BTreeSet<String>> {
    Ok(read_jsonl_entries(path)?
        .into_iter()
        .filter_map(|entry| {
            entry
                .value
                .as_ref()
                .and_then(|value| value.get("id"))
                .and_then(|value| value.as_str())
                .map(ToString::to_string)
        })
        .collect())
}

fn append_missing_session_index_rows(path: &Path, missing_rows: &[HistoryRow]) -> AppResult<usize> {
    if missing_rows.is_empty() {
        return Ok(0);
    }
    let mut entries = read_jsonl_entries(path)?;
    let mut ids = entries
        .iter()
        .filter_map(|entry| {
            entry
                .value
                .as_ref()
                .and_then(|value| value.get("id"))
                .and_then(|value| value.as_str())
                .map(ToString::to_string)
        })
        .collect::<BTreeSet<_>>();
    let mut appended = 0;
    for row in missing_rows {
        if ids.contains(&row.id) {
            continue;
        }
        entries.push(JsonlEntry {
            value: Some(serde_json::json!({
                "id": row.id,
                "thread_name": row.title,
                "updated_at": iso_from_ms(row.updated_at_ms),
            })),
            raw: String::new(),
        });
        ids.insert(row.id.clone());
        appended += 1;
    }
    write_jsonl_entries(path, &entries)?;
    Ok(appended)
}

fn move_focus_session_index_rows(
    path: &Path,
    focus_rows: &[FocusRow],
) -> AppResult<(usize, usize)> {
    if focus_rows.is_empty() {
        return Ok((0, 0));
    }
    let selected = focus_rows
        .iter()
        .map(|row| (row.id.as_str(), row))
        .collect::<BTreeMap<_, _>>();
    let mut kept = Vec::new();
    let mut moved_by_id = BTreeMap::new();
    let mut titles_updated = 0;
    for mut entry in read_jsonl_entries(path)? {
        let thread_id = entry
            .value
            .as_ref()
            .and_then(|value| value.get("id"))
            .and_then(|value| value.as_str())
            .map(ToString::to_string);
        let Some(thread_id) = thread_id else {
            kept.push(entry);
            continue;
        };
        let Some(focus) = selected.get(thread_id.as_str()) else {
            kept.push(entry);
            continue;
        };
        let mut object = entry
            .value
            .take()
            .and_then(|value| value.as_object().cloned())
            .unwrap_or_default();
        let old_title = object
            .get("thread_name")
            .and_then(|value| value.as_str())
            .unwrap_or("")
            .trim()
            .to_string();
        object.insert(
            "updated_at".to_string(),
            Value::String(focus.updated_iso.clone()),
        );
        if !focus.title.trim().is_empty() && old_title != focus.title.trim() {
            object.insert(
                "thread_name".to_string(),
                Value::String(focus.title.clone()),
            );
            titles_updated += 1;
        }
        moved_by_id.insert(
            thread_id,
            JsonlEntry {
                value: Some(Value::Object(object)),
                raw: String::new(),
            },
        );
    }
    let mut moved = Vec::new();
    for focus in focus_rows {
        moved.push(moved_by_id.remove(&focus.id).unwrap_or_else(|| JsonlEntry {
            value: Some(serde_json::json!({
                "id": focus.id,
                "thread_name": focus.title,
                "updated_at": focus.updated_iso,
            })),
            raw: String::new(),
        }));
    }
    let moved_count = moved.len();
    kept.extend(moved);
    write_jsonl_entries(path, &kept)?;
    Ok((moved_count, titles_updated))
}

fn read_json_object(path: &Path) -> AppResult<serde_json::Map<String, Value>> {
    if !path.exists() {
        return Ok(Default::default());
    }
    let raw = fs::read_to_string(path).map_err(AppError::Io)?;
    let value =
        serde_json::from_str::<Value>(&raw).unwrap_or_else(|_| Value::Object(Default::default()));
    Ok(value.as_object().cloned().unwrap_or_default())
}

fn write_json_object(path: &Path, object: &serde_json::Map<String, Value>) -> AppResult<()> {
    let text = serde_json::to_string_pretty(&Value::Object(object.clone()))
        .map_err(|error| AppError::AtomicWrite(format!("serialize JSON object: {error}")))?;
    writer::atomic_write(path, &(text + "\n"))
}

fn update_global_state(
    path: &Path,
    visible_rows: &[HistoryRow],
    focus_rows: &[FocusRow],
    project_path: Option<&str>,
    apply: bool,
) -> AppResult<GlobalStateUpdateCounts> {
    let mut state = read_json_object(path)?;
    let mut expected = BTreeMap::new();
    for row in visible_rows {
        if let Some(cwd) = normalize_workspace_path(row.cwd.as_deref()) {
            expected.insert(row.id.clone(), cwd);
        }
    }
    for row in focus_rows {
        if let Some(cwd) = normalize_workspace_path(project_path.or(row.cwd.as_deref())) {
            expected.insert(row.id.clone(), cwd);
        }
    }

    let mut hints = state
        .get("thread-workspace-root-hints")
        .and_then(|value| value.as_object())
        .cloned()
        .unwrap_or_default();
    let workspace_hints_to_fix = expected
        .iter()
        .filter(|(id, cwd)| hints.get(*id).and_then(|value| value.as_str()) != Some(cwd.as_str()))
        .count();
    let expected_ids = expected.keys().cloned().collect::<BTreeSet<_>>();
    let projectless_ids_to_remove = state
        .get("projectless-thread-ids")
        .and_then(|value| value.as_array())
        .map(|items| {
            items
                .iter()
                .filter_map(|value| value.as_str())
                .filter(|id| expected_ids.contains(*id))
                .count()
        })
        .unwrap_or(0);
    let normalized_project = normalize_workspace_path(project_path);
    let saved_workspace_roots_to_add = normalized_project
        .as_deref()
        .map(|project| {
            let exists = state
                .get("electron-saved-workspace-roots")
                .and_then(|value| value.as_array())
                .map(|roots| roots.iter().any(|value| value.as_str() == Some(project)))
                .unwrap_or(false);
            if exists {
                0
            } else {
                1
            }
        })
        .unwrap_or(0);

    let counts = GlobalStateUpdateCounts {
        workspace_hints_to_fix,
        projectless_ids_to_remove,
        saved_workspace_roots_to_add,
    };
    if !apply
        || (workspace_hints_to_fix == 0
            && projectless_ids_to_remove == 0
            && saved_workspace_roots_to_add == 0)
    {
        return Ok(counts);
    }

    for (id, cwd) in expected {
        hints.insert(id, Value::String(cwd));
    }
    state.insert(
        "thread-workspace-root-hints".to_string(),
        Value::Object(hints),
    );
    if projectless_ids_to_remove > 0 {
        let remaining = state
            .get("projectless-thread-ids")
            .and_then(|value| value.as_array())
            .map(|items| {
                items
                    .iter()
                    .filter(|value| {
                        value
                            .as_str()
                            .map(|id| !expected_ids.contains(id))
                            .unwrap_or(true)
                    })
                    .cloned()
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        state.insert(
            "projectless-thread-ids".to_string(),
            Value::Array(remaining),
        );
    }
    if let Some(project) = normalized_project {
        let mut roots = state
            .get("electron-saved-workspace-roots")
            .and_then(|value| value.as_array())
            .cloned()
            .unwrap_or_default();
        if !roots
            .iter()
            .any(|value| value.as_str() == Some(project.as_str()))
        {
            roots.push(Value::String(project));
            state.insert(
                "electron-saved-workspace-roots".to_string(),
                Value::Array(roots),
            );
        }
    }
    write_json_object(path, &state)?;
    Ok(counts)
}

fn create_history_backup(
    codex_home: &Path,
    active_state: &ActiveStateDb,
    rollout_rewrites: &[RolloutRewrite],
    preview: &CodexHistoryUnifyPreview,
) -> AppResult<CodexHistoryBackupSummary> {
    let backup_dir = PathBuf::from(&preview.backup_dir);
    fs::create_dir_all(&backup_dir)
        .map_err(|error| AppError::BackupDir(format!("{}: {}", backup_dir.display(), error)))?;
    let mut files = Vec::new();
    let db_dir = backup_dir.join(if active_state.kind == "sqlite_subdir" {
        "sqlite"
    } else {
        "legacy-root"
    });
    fs::create_dir_all(&db_dir)
        .map_err(|error| AppError::BackupDir(format!("{}: {}", db_dir.display(), error)))?;
    for suffix in ["", "-wal", "-shm"] {
        let source = PathBuf::from(format!("{}{}", active_state.path.display(), suffix));
        if source.exists() {
            let target = db_dir.join(
                source
                    .file_name()
                    .unwrap_or_else(|| std::ffi::OsStr::new("state_5.sqlite")),
            );
            fs::copy(&source, &target).map_err(|error| {
                AppError::BackupDir(format!(
                    "copy {} -> {}: {}",
                    source.display(),
                    target.display(),
                    error
                ))
            })?;
            files.push(target.display().to_string());
        }
    }
    for name in [DEFAULT_SESSION_INDEX_FILE, DEFAULT_GLOBAL_STATE_FILE] {
        let source = codex_home.join(name);
        if source.exists() {
            let target = backup_dir.join(name);
            fs::copy(&source, &target).map_err(|error| {
                AppError::BackupDir(format!(
                    "copy {} -> {}: {}",
                    source.display(),
                    target.display(),
                    error
                ))
            })?;
            files.push(target.display().to_string());
        }
    }

    let rollout_dir = backup_dir.join("rollouts");
    fs::create_dir_all(&rollout_dir)
        .map_err(|error| AppError::BackupDir(format!("{}: {}", rollout_dir.display(), error)))?;
    let mut manifest = Vec::new();
    for (index, rewrite) in rollout_rewrites.iter().enumerate() {
        if !rewrite.path.exists() {
            continue;
        }
        let target = rollout_dir.join(format!(
            "{index:04}-{}",
            rewrite
                .path
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or("rollout.jsonl")
        ));
        fs::copy(&rewrite.path, &target).map_err(|error| {
            AppError::BackupDir(format!(
                "copy {} -> {}: {}",
                rewrite.path.display(),
                target.display(),
                error
            ))
        })?;
        files.push(target.display().to_string());
        manifest.push(serde_json::json!({
            "source": rewrite.path.display().to_string(),
            "backup": target.display().to_string(),
            "changedLines": rewrite.changed_lines,
        }));
    }
    let rollout_manifest_path = backup_dir.join("rollout-manifest.json");
    writer::atomic_write(
        &rollout_manifest_path,
        &serde_json::to_string_pretty(&manifest).map_err(|error| {
            AppError::AtomicWrite(format!("serialize rollout manifest: {error}"))
        })?,
    )?;
    files.push(rollout_manifest_path.display().to_string());

    Ok(CodexHistoryBackupSummary {
        backup_dir: backup_dir.display().to_string(),
        files,
        rollout_manifest_path: rollout_manifest_path.display().to_string(),
    })
}

fn apply_sqlite_history_updates(
    path: &Path,
    target_provider: &str,
    provider_update_ids: &[String],
    user_event_update_ids: &[String],
    focus_rows: &[FocusRow],
) -> AppResult<SqliteHistoryUpdateCounts> {
    let mut connection = Connection::open(path)
        .map_err(|error| AppError::Command(format!("open SQLite {}: {error}", path.display())))?;
    let columns = sqlite_table_columns(&connection, "threads")?;
    let transaction = connection.transaction().map_err(|error| {
        AppError::Command(format!(
            "start SQLite transaction {}: {error}",
            path.display()
        ))
    })?;
    let provider_rows_updated = if provider_update_ids.is_empty() {
        0
    } else {
        let placeholders = std::iter::repeat("?")
            .take(provider_update_ids.len())
            .collect::<Vec<_>>()
            .join(",");
        let sql = format!("UPDATE threads SET model_provider = ? WHERE id IN ({placeholders})");
        let mut params = Vec::with_capacity(provider_update_ids.len() + 1);
        params.push(target_provider.to_string());
        params.extend(provider_update_ids.iter().cloned());
        transaction
            .execute(
                &sql,
                rusqlite::params_from_iter(params.iter().map(String::as_str)),
            )
            .map_err(|error| AppError::Command(format!("update SQLite provider bucket: {error}")))?
    };
    let user_event_rows_updated =
        if user_event_update_ids.is_empty() || !columns.contains("has_user_event") {
            0
        } else {
            let placeholders = std::iter::repeat("?")
                .take(user_event_update_ids.len())
                .collect::<Vec<_>>()
                .join(",");
            let sql = format!("UPDATE threads SET has_user_event = 1 WHERE id IN ({placeholders})");
            transaction
                .execute(
                    &sql,
                    rusqlite::params_from_iter(user_event_update_ids.iter().map(String::as_str)),
                )
                .map_err(|error| AppError::Command(format!("update SQLite user events: {error}")))?
        };
    let mut focus_rows_updated = 0;
    if columns.contains("updated_at") || columns.contains("updated_at_ms") {
        for focus in focus_rows {
            let updated = if columns.contains("updated_at") && columns.contains("updated_at_ms") {
                transaction.execute(
                    "UPDATE threads SET updated_at = ?, updated_at_ms = ? WHERE id = ?",
                    rusqlite::params![focus.updated_at, focus.updated_at_ms, focus.id],
                )
            } else if columns.contains("updated_at_ms") {
                transaction.execute(
                    "UPDATE threads SET updated_at_ms = ? WHERE id = ?",
                    rusqlite::params![focus.updated_at_ms, focus.id],
                )
            } else {
                transaction.execute(
                    "UPDATE threads SET updated_at = ? WHERE id = ?",
                    rusqlite::params![focus.updated_at, focus.id],
                )
            }
            .map_err(|error| AppError::Command(format!("update SQLite focus row: {error}")))?;
            focus_rows_updated += updated;
        }
    }
    transaction
        .commit()
        .map_err(|error| AppError::Command(format!("commit SQLite history updates: {error}")))?;
    Ok(SqliteHistoryUpdateCounts {
        provider_rows_updated,
        user_event_rows_updated,
        focus_rows_updated,
    })
}

fn apply_rollout_rewrites(rollout_rewrites: &[RolloutRewrite]) -> AppResult<(usize, usize)> {
    let mut files_updated = 0;
    let mut lines_updated = 0;
    for rewrite in rollout_rewrites {
        writer::atomic_write(&rewrite.path, &rewrite.text)?;
        files_updated += 1;
        lines_updated += rewrite.changed_lines;
    }
    Ok((files_updated, lines_updated))
}

fn detect_running_codex_processes() -> Vec<String> {
    let output = if cfg!(target_os = "windows") {
        Command::new("wmic")
            .args(["process", "get", "ProcessId,CommandLine", "/FORMAT:LIST"])
            .output()
    } else {
        Command::new("ps")
            .args(["-axo", "pid=,comm=,args="])
            .output()
    };
    let Ok(output) = output else {
        return Vec::new();
    };
    if !output.status.success() {
        return Vec::new();
    }
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter(|line| {
            let lower = line.to_ascii_lowercase();
            lower.contains("codex")
                && !lower.contains("codex box")
                && !lower.contains("codex-box")
                && (lower.contains("openai.codex")
                    || lower.contains("codex.app")
                    || lower.contains("codex.exe")
                    || lower.contains("app-server"))
        })
        .take(8)
        .map(|line| line.trim().to_string())
        .collect()
}

fn history_warning(
    severity: impl Into<String>,
    code: impl Into<String>,
    message: impl Into<String>,
) -> CodexHistoryWarning {
    CodexHistoryWarning {
        severity: severity.into(),
        code: code.into(),
        message: message.into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_top_level_model_provider_before_tables() {
        let text = r#"
model_provider = "custom"

[model_providers.openai]
model_provider = "ignored"
"#;
        assert_eq!(
            parse_top_level_string(text, "model_provider").as_deref(),
            Some("custom")
        );
    }

    #[test]
    fn scans_sqlite_and_jsonl_provider_buckets() {
        let dir = tempfile::tempdir().unwrap();
        let codex_home = dir.path().join(".codex");
        std::fs::create_dir_all(codex_home.join("sqlite")).unwrap();
        std::fs::create_dir_all(codex_home.join("sessions/2026/06/27")).unwrap();
        std::fs::write(
            codex_home.join("config.toml"),
            r#"model_provider = "codex_local_access""#,
        )
        .unwrap();
        let db_path = codex_home.join("sqlite/state_5.sqlite");
        let connection = Connection::open(&db_path).unwrap();
        connection
            .execute(
                "CREATE TABLE threads (id TEXT PRIMARY KEY, model_provider TEXT NOT NULL)",
                [],
            )
            .unwrap();
        connection
            .execute(
                "INSERT INTO threads (id, model_provider) VALUES ('1', 'openai'), ('2', 'codex_local_access')",
                [],
            )
            .unwrap();
        std::fs::write(
            codex_home.join("sessions/2026/06/27/rollout.jsonl"),
            r#"{"payload":{"model_provider":"openai"}}"#,
        )
        .unwrap();

        let view = codex_history_reconcile_in_home(&codex_home).unwrap();
        assert_eq!(view.active_state_db_kind.as_deref(), Some("sqlite_subdir"));
        assert!(view.providers_found.contains(&"openai".to_string()));
        assert!(view
            .providers_found
            .contains(&"codex_local_access".to_string()));
        assert!(view.drift_detected);
        assert_eq!(view.provider_rows_to_update, 1);
        assert_eq!(view.rollout_provider_lines_to_update, 1);
    }

    #[test]
    fn uses_legacy_root_state_db_when_sqlite_subdir_is_missing() {
        let dir = tempfile::tempdir().unwrap();
        let codex_home = dir.path().join(".codex");
        std::fs::create_dir_all(&codex_home).unwrap();
        let db_path = codex_home.join("state_5.sqlite");
        let connection = Connection::open(&db_path).unwrap();
        connection
            .execute(
                "CREATE TABLE threads (id TEXT PRIMARY KEY, model_provider TEXT NOT NULL)",
                [],
            )
            .unwrap();

        let view = codex_history_reconcile_in_home(&codex_home).unwrap();
        assert_eq!(view.active_state_db_kind.as_deref(), Some("legacy_root"));
    }

    #[test]
    fn reconcile_defaults_missing_model_provider_to_codex_box_router() {
        let dir = tempfile::tempdir().unwrap();
        let codex_home = dir.path().join(".codex");
        std::fs::create_dir_all(&codex_home).unwrap();
        std::fs::write(codex_home.join("config.toml"), "").unwrap();

        let view = codex_history_reconcile_in_home(&codex_home).unwrap();

        assert_eq!(view.live_config_model_provider, None);
        assert_eq!(view.suggested_target_provider, "codex_model_router_v2");
    }

    #[test]
    fn unify_apply_updates_sqlite_and_jsonl_after_backup() {
        let dir = tempfile::tempdir().unwrap();
        let codex_home = dir.path().join(".codex");
        std::fs::create_dir_all(codex_home.join("sqlite")).unwrap();
        std::fs::create_dir_all(codex_home.join("sessions/2026/06/27")).unwrap();
        std::fs::write(
            codex_home.join("config.toml"),
            r#"model_provider = "codex_local_access""#,
        )
        .unwrap();
        let db_path = codex_home.join("sqlite/state_5.sqlite");
        let connection = Connection::open(&db_path).unwrap();
        connection
            .execute(
                "CREATE TABLE threads (id TEXT PRIMARY KEY, model_provider TEXT NOT NULL)",
                [],
            )
            .unwrap();
        connection
            .execute(
                "INSERT INTO threads (id, model_provider) VALUES ('1', 'openai'), ('2', 'codex_local_access')",
                [],
            )
            .unwrap();
        drop(connection);

        let rollout = codex_home.join("sessions/2026/06/27/rollout.jsonl");
        std::fs::write(
            &rollout,
            "{\"payload\":{\"model_provider\":\"openai\"}}\n{\"payload\":{\"model_provider\":\"codex_local_access\"}}\n",
        )
        .unwrap();

        let result = codex_history_unify_apply_in_home(
            &codex_home,
            &CodexHistoryUnifyRequest {
                force: Some(true),
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(result.provider_rows_updated, 1);
        assert_eq!(result.rollout_files_updated, 1);
        assert_eq!(result.rollout_provider_lines_updated, 1);
        assert!(std::path::Path::new(&result.backup.backup_dir).exists());
        assert!(std::path::Path::new(&result.backup.rollout_manifest_path).exists());

        let connection = Connection::open(&db_path).unwrap();
        let openai_count: i64 = connection
            .query_row(
                "SELECT COUNT(*) FROM threads WHERE model_provider = 'openai'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        let unified_count: i64 = connection
            .query_row(
                "SELECT COUNT(*) FROM threads WHERE model_provider = 'codex_local_access'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(openai_count, 0);
        assert_eq!(unified_count, 2);
        let rollout_text = std::fs::read_to_string(&rollout).unwrap();
        assert!(!rollout_text.contains("\"model_provider\":\"openai\""));
        assert!(rollout_text.contains("\"model_provider\":\"codex_local_access\""));
    }

    #[test]
    fn unify_apply_repairs_visibility_index_and_global_state() {
        let dir = tempfile::tempdir().unwrap();
        let codex_home = dir.path().join(".codex");
        let project = dir.path().join("project");
        std::fs::create_dir_all(codex_home.join("sqlite")).unwrap();
        std::fs::create_dir_all(codex_home.join("sessions/2026/06/27")).unwrap();
        std::fs::create_dir_all(&project).unwrap();
        std::fs::write(
            codex_home.join("config.toml"),
            r#"model_provider = "codex_local_access""#,
        )
        .unwrap();
        let rollout = codex_home.join("sessions/2026/06/27/rollout.jsonl");
        std::fs::write(
            &rollout,
            "{\"payload\":{\"model_provider\":\"openai\"}}\n{\"type\":\"user_message\",\"payload\":{\"text\":\"hello\"}}\n",
        )
        .unwrap();
        let db_path = codex_home.join("sqlite/state_5.sqlite");
        let connection = Connection::open(&db_path).unwrap();
        connection
            .execute(
                "CREATE TABLE threads (
                    id TEXT PRIMARY KEY,
                    rollout_path TEXT,
                    model_provider TEXT NOT NULL,
                    source TEXT,
                    thread_source TEXT,
                    archived INTEGER,
                    has_user_event INTEGER,
                    title TEXT,
                    preview TEXT,
                    first_user_message TEXT,
                    cwd TEXT,
                    updated_at INTEGER,
                    updated_at_ms INTEGER
                )",
                [],
            )
            .unwrap();
        connection
            .execute(
                "INSERT INTO threads (
                    id, rollout_path, model_provider, source, thread_source, archived,
                    has_user_event, title, preview, first_user_message, cwd, updated_at, updated_at_ms
                ) VALUES (?1, ?2, 'openai', 'vscode', 'user', 0, 0, 'Old title', '', '', ?3, 100, 100000)",
                rusqlite::params!["thread-1", rollout.display().to_string(), project.display().to_string()],
            )
            .unwrap();
        drop(connection);
        std::fs::write(
            codex_home.join("session_index.jsonl"),
            "{\"id\":\"other\",\"thread_name\":\"Other\",\"updated_at\":\"2026-01-01T00:00:00.000Z\"}\n",
        )
        .unwrap();
        std::fs::write(
            codex_home.join(".codex-global-state.json"),
            serde_json::json!({
                "thread-workspace-root-hints": {},
                "projectless-thread-ids": ["thread-1", "other"],
                "electron-saved-workspace-roots": []
            })
            .to_string(),
        )
        .unwrap();

        let result = codex_history_unify_apply_in_home(
            &codex_home,
            &CodexHistoryUnifyRequest {
                project_path: Some(project.display().to_string()),
                force: Some(true),
                ..Default::default()
            },
        )
        .unwrap();

        assert_eq!(result.provider_rows_updated, 1);
        assert_eq!(result.user_event_rows_updated, 1);
        assert_eq!(result.session_index_appended, 1);
        assert_eq!(result.session_index_rows_moved, 1);
        assert_eq!(result.workspace_hints_fixed, 1);
        assert_eq!(result.projectless_ids_removed, 1);
        assert_eq!(result.saved_workspace_roots_added, 1);

        let connection = Connection::open(&db_path).unwrap();
        let (provider, has_user_event, updated_at_ms): (String, i64, i64) = connection
            .query_row(
                "SELECT model_provider, has_user_event, updated_at_ms FROM threads WHERE id = 'thread-1'",
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .unwrap();
        assert_eq!(provider, "codex_local_access");
        assert_eq!(has_user_event, 1);
        assert!(updated_at_ms > 100000);

        let session_index =
            std::fs::read_to_string(codex_home.join("session_index.jsonl")).unwrap();
        assert!(session_index.contains("\"id\":\"thread-1\""));
        assert!(session_index.contains("\"thread_name\":\"Old title\""));

        let state: Value = serde_json::from_str(
            &std::fs::read_to_string(codex_home.join(".codex-global-state.json")).unwrap(),
        )
        .unwrap();
        assert_eq!(
            state
                .get("thread-workspace-root-hints")
                .and_then(|value| value.get("thread-1"))
                .and_then(|value| value.as_str()),
            Some(project.to_str().unwrap())
        );
        assert!(!state
            .get("projectless-thread-ids")
            .and_then(|value| value.as_array())
            .unwrap()
            .iter()
            .any(|value| value.as_str() == Some("thread-1")));
        assert!(state
            .get("electron-saved-workspace-roots")
            .and_then(|value| value.as_array())
            .unwrap()
            .iter()
            .any(|value| value.as_str() == project.to_str()));
    }

    #[test]
    fn rollout_user_event_detection_parses_jsonl_shapes() {
        let dir = tempfile::tempdir().unwrap();
        let rollout = dir.path().join("rollout.jsonl");
        std::fs::write(
            &rollout,
            "{ \"type\": \"session_meta\", \"payload\": { \"model_provider\": \"openai\" } }\n\
             { \"type\": \"response_item\", \"payload\": { \"item\": { \"type\": \"message\", \"role\": \"user\", \"content\": [] } } }\n",
        )
        .unwrap();

        assert!(rollout_has_user_event(Some(rollout.to_str().unwrap())));
    }
}
